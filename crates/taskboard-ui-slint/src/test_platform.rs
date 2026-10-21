// SPDX-License-Identifier: AGPL-3.0-only
//! Headless test platform (chunk decision D14): a dev-only
//! [`slint::platform::Platform`] implementation so component
//! instantiation tests run with no display and no GPU. nextest's
//! process-per-test model makes the process-global `set_platform`
//! registration safe; `cargo test --doc` never instantiates
//! components.
//!
//! The renderer is constructed for **headless draw through this
//! module's explicit render pass-through** ([`NullWindowAdapter::
//! render_into`]); nothing renders unless a harness calls it — no
//! implicit draws, no event loop. Windows are 0×0 until the public
//! resize path (`Window::set_size`, via [`NullWindowAdapter::
//! set_size`]) gives them real geometry. [`window_adapter`] hands the
//! harness the last-created adapter (the one-window-per-process
//! reality of both the lib tests and the bench).

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use slint::platform::software_renderer::{
    PhysicalRegion, RepaintBufferType, SoftwareRenderer, TargetPixel,
};
use slint::platform::{Platform, PlatformError, Renderer, WindowAdapter, WindowEvent};
use slint::{PhysicalSize, Window, WindowSize};

/// A platform that hands out null window adapters: no event loop can
/// run (tests never call `run_event_loop`); drawing happens only
/// through the adapter's explicit render pass-through.
#[derive(Default)]
pub(crate) struct NullPlatform;

// The registry: the last-created adapter, weakly — the harness's
// way to reach the mounted window's adapter. Thread-local because
// the platform — and everything touching it — is thread-affine
// (`on_slint_thread`'s worker in tests, the main thread in the
// bench).
thread_local! {
    static LAST_ADAPTER: RefCell<Option<Weak<NullWindowAdapter>>> = const { RefCell::new(None) };
}

/// The live adapter of the most recently created window on this
/// thread's platform, if it is still alive.
pub(crate) fn window_adapter() -> Option<Rc<NullWindowAdapter>> {
    LAST_ADAPTER.with(|slot| slot.borrow().as_ref().and_then(Weak::upgrade))
}

/// The null window adapter: a real [`Window`] object (so component
/// instances can bind to it) with a no-op backend behind it. The
/// renderer is the software renderer — draw-capable headlessly via
/// the explicit [`Self::render_into`] pass-through only; it is never
/// asked to draw implicitly.
pub(crate) struct NullWindowAdapter {
    window: Window,
    renderer: SoftwareRenderer,
    /// The physical size the window was last resized to through the
    /// public path; 0×0 until then.
    size: Cell<PhysicalSize>,
}

impl NullWindowAdapter {
    fn new() -> Rc<Self> {
        Rc::new_cyclic(|weak: &Weak<Self>| Self {
            window: Window::new(weak.clone()),
            renderer: SoftwareRenderer::new(),
            size: Cell::new(PhysicalSize::new(0, 0)),
        })
    }

    /// Renders the mounted component into `buffer` (stride
    /// `pixel_stride` pixels per row) — a thin pass-through over the
    /// software renderer's own public API. Returns the drawn region.
    pub(crate) fn render_into<P: TargetPixel>(
        &self,
        buffer: &mut [P],
        pixel_stride: usize,
    ) -> PhysicalRegion {
        self.renderer.render(buffer, pixel_stride)
    }

    /// Selects the repaint-buffer mode (thin pass-through):
    /// `NewBuffer` (the default) redraws the full window;
    /// `ReusedBuffer` opts into dirty-tracked partial rendering when
    /// the same buffer is passed each call.
    pub(crate) fn set_repaint_buffer_type(&self, mode: RepaintBufferType) {
        self.renderer.set_repaint_buffer_type(mode);
    }
}

impl WindowAdapter for NullWindowAdapter {
    fn window(&self) -> &Window {
        &self.window
    }

    fn renderer(&self) -> &dyn Renderer {
        &self.renderer
    }

    fn size(&self) -> PhysicalSize {
        self.size.get()
    }

    /// The public resize path (`Window::set_size` lands here): store
    /// the physical size and dispatch `Resized` with the logical size
    /// (scale factor 1 on this platform) so the mounted component's
    /// root gets real geometry — the testing backend's own shape
    /// (`TestingWindow::set_size`).
    fn set_size(&self, size: WindowSize) {
        let physical = size.to_physical(1.0);
        self.size.set(physical);
        self.window.dispatch_event(WindowEvent::Resized {
            size: physical.to_logical(1.0),
        });
    }
}

impl Platform for NullPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let adapter = NullWindowAdapter::new();
        LAST_ADAPTER.with(|slot| *slot.borrow_mut() = Some(Rc::downgrade(&adapter)));
        Ok(adapter)
    }
}

/// Registers the null platform as Slint's process-global platform and
/// runs `f` on the dedicated slint test thread, returning its result.
///
/// Slint's platform (and font context) is process-global *and
/// thread-affine*: it must be registered and used from one thread for
/// the whole process lifetime. A single long-lived worker thread owns
/// it — it registers the null platform before its first job — so
/// component tests are correct under both nextest's process-per-test
/// model and `cargo test`'s threaded runner (llvm-cov).
///
/// # Panics
///
/// Panics if the platform is already registered on the worker (never:
/// the worker registers exactly once) or if the worker died.
pub(crate) fn on_slint_thread<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> R {
    use std::sync::mpsc;
    type Job = Box<dyn FnOnce() + Send>;

    static SENDER: std::sync::OnceLock<mpsc::Sender<Job>> = std::sync::OnceLock::new();
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("slint-tests".into())
            .spawn(move || {
                slint::platform::set_platform(Box::new(NullPlatform))
                    .expect("null platform registers exactly once per test process");
                for job in rx {
                    job();
                }
            })
            .expect("the slint test worker spawns");
        tx
    });

    let (reply_tx, reply_rx) = mpsc::channel();
    sender
        .send(Box::new(move || {
            let result = f();
            let _ = reply_tx.send(result);
        }))
        .expect("the slint test worker is alive");
    reply_rx
        .recv()
        .expect("the slint test worker returned a result")
}

// Compiled into the lib under cfg(test); when the file is
// #[path]-included into the bench target, cfg(test) is set there too
// and the tests' own imports warn unused (the tests never run under
// the criterion harness) — the allow covers that include shape.
#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::{UiSettings, mount};
    use slint::ComponentHandle as _;
    use taskboard_domain::entities::{Board, Color, Stack, Task};
    use taskboard_domain::ids::{BoardId, StackId, TaskId};

    type Handover = (
        std::sync::Arc<arc_swap::ArcSwap<taskboard_domain::AppState>>,
        tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal>,
        tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent>,
        tokio::sync::mpsc::Sender<taskboard_domain::StateCommand>,
    );

    fn handover(state: taskboard_domain::AppState) -> Handover {
        let (commands_tx, _rx) = tokio::sync::mpsc::channel(8);
        let (_stx, signals) = tokio::sync::broadcast::channel(4);
        let (_syt, system) = tokio::sync::broadcast::channel(4);
        (
            std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(state)),
            signals,
            system,
            commands_tx,
        )
    }

    fn mounted(state: taskboard_domain::AppState) -> crate::UiHandle {
        let (state, signals, system, commands) = handover(state);
        mount(state, signals, system, commands, UiSettings::default())
            .expect("mount succeeds on the null platform")
    }

    /// A small but real board: one board, two stacks, six cards —
    /// enough scene for the draw seam's touched-pixels assertion to
    /// be meaningful (an empty board would render chrome only).
    fn populated_state() -> taskboard_domain::AppState {
        use chrono::TimeZone as _;
        let stamp = |secs: i64| {
            chrono::Utc
                .timestamp_opt(1_704_067_200 + secs, 0)
                .single()
                .expect("fixed stamp")
        };
        let board = Board {
            id: BoardId::from(uuid::Uuid::from_u128(0x1000)),
            remote: None,
            title: "Seam board".into(),
            color: Color::new("3366ff"),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: stamp(0),
                color: stamp(0),
                archived: stamp(0),
                deleted: stamp(0),
            },
            remote_seen: None,
        };
        let stacks: Vec<Stack> = (0..2)
            .map(|s| Stack {
                id: StackId::from(uuid::Uuid::from_u128(
                    0x2000 + u128::try_from(s).expect("small"),
                )),
                remote: None,
                board: board.id,
                title: format!("Stack {s}"),
                order: s,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: stamp(1),
                    order: stamp(1),
                    deleted: stamp(1),
                },
                remote_seen: None,
            })
            .collect();
        let tasks: Vec<Task> = (0..6)
            .map(|i| Task {
                id: TaskId::from(uuid::Uuid::from_u128(
                    0x3000 + u128::try_from(i).expect("small"),
                )),
                remote: None,
                title: format!("Task {i} seam fixture"),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack: stacks[i % 2].id,
                order: i64::try_from(i).expect("small"),
                labels: std::collections::BTreeSet::new(),
                effort: None,
                importance: None,
                urgency: None,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::TaskClocks {
                    title: stamp(2),
                    description: stamp(2),
                    duedate: stamp(2),
                    scheduled: stamp(2),
                    done: stamp(2),
                    position: stamp(2),
                    labels: stamp(2),
                    effort: stamp(2),
                    importance: stamp(2),
                    urgency: stamp(2),
                    archived: stamp(2),
                    deleted: stamp(2),
                },
                remote_seen: None,
            })
            .collect();
        taskboard_domain::AppState {
            boards: std::iter::once((board.id, board)).collect(),
            stacks: stacks.iter().map(|s| (s.id, s.clone())).collect(),
            tasks: tasks.into_iter().map(|t| (t.id, t)).collect(),
            ..taskboard_domain::AppState::default()
        }
    }

    fn render_buffer(width: usize, height: usize) -> Vec<slint::Rgb8Pixel> {
        vec![slint::Rgb8Pixel::new(0, 0, 0); width * height]
    }

    /// T1: after `mount` on the null platform, the adapter registry
    /// hands back a live adapter whose window is the mounted window.
    #[test]
    fn registry_hands_back_the_mounted_window_adapter() {
        on_slint_thread(|| {
            let handle = mounted(taskboard_domain::AppState::default());

            let adapter = window_adapter().expect("the registry holds the mounted adapter");
            // The registry entry is a `Weak`, but slint core retains
            // its own strong reference to the adapter beyond the
            // component's drop (observed: the weak keeps upgrading) —
            // that retention is slint-internal, not this seam's
            // contract, so only liveness is pinned here.
            std::mem::drop(handle);
            assert_eq!(adapter.size(), PhysicalSize::new(0, 0));
        });
    }

    /// T2: the adapter's size is 0×0 until the public resize path
    /// (`Window::set_size`) lands the physical size; the default must
    /// not change (existing tests' semantics).
    #[test]
    fn set_size_lands_the_physical_size() {
        on_slint_thread(|| {
            let handle = mounted(taskboard_domain::AppState::default());
            let adapter = window_adapter().expect("the registry holds the mounted adapter");
            assert_eq!(adapter.size(), PhysicalSize::new(0, 0));

            handle
                .window()
                .window()
                .set_size(slint::WindowSize::Physical(PhysicalSize::new(1366, 768)));
            assert_eq!(adapter.size(), PhysicalSize::new(1366, 768));
        });
    }

    /// T3: the draw seam draws a populated board headlessly into a
    /// caller buffer — full-window region (the default `NewBuffer`
    /// repaint type draws everything) and pixels actually touched (a
    /// degenerate scene fails here).
    #[test]
    fn render_into_draws_a_full_frame_headlessly() {
        on_slint_thread(|| {
            let handle = mounted(populated_state());
            handle
                .window()
                .window()
                .set_size(slint::WindowSize::Physical(PhysicalSize::new(64, 48)));
            let adapter = window_adapter().expect("the registry holds the mounted adapter");
            handle.refresh();

            let mut buffer = render_buffer(64, 48);
            let region = adapter.render_into(&mut buffer, 64);
            assert_eq!(
                region.bounding_box_size(),
                slint::PhysicalSize::new(64, 48),
                "the default NewBuffer repaint type draws the full window"
            );
            assert!(
                buffer.iter().any(|p| *p != slint::Rgb8Pixel::new(0, 0, 0)),
                "the render touched pixels (non-degenerate scene)"
            );
        });
    }

    /// T4: `ReusedBuffer` dirty tracking — after a full render, a
    /// second render with no property change yields a region strictly
    /// smaller than the window (the cache-hit contract the partial
    /// lane's honesty rests on; not an exact-shape pin).
    #[test]
    fn reused_buffer_second_render_is_dirty_tracked() {
        on_slint_thread(|| {
            let handle = mounted(populated_state());
            handle
                .window()
                .window()
                .set_size(slint::WindowSize::Physical(PhysicalSize::new(64, 48)));
            let adapter = window_adapter().expect("the registry holds the mounted adapter");
            handle.refresh();
            adapter.set_repaint_buffer_type(
                slint::platform::software_renderer::RepaintBufferType::ReusedBuffer,
            );

            let mut buffer = render_buffer(64, 48);
            let first = adapter.render_into(&mut buffer, 64);
            assert_eq!(
                first.bounding_box_size().width * first.bounding_box_size().height,
                64 * 48
            );
            let second = adapter.render_into(&mut buffer, 64);
            let area = |r: slint::platform::software_renderer::PhysicalRegion| {
                let s = r.bounding_box_size();
                u64::from(s.width) * u64::from(s.height)
            };
            assert!(
                area(second) < area(first),
                "unchanged scene re-render must hit the dirty cache (region < full window)"
            );
        });
    }
}
