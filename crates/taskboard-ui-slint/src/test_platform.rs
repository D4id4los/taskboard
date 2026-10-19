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

use slint::platform::software_renderer::SoftwareRenderer;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{UiSettings, mount};
    use slint::ComponentHandle as _;

    /// T1: after `mount` on the null platform, the adapter registry
    /// hands back a live adapter whose window is the mounted window.
    #[test]
    fn registry_hands_back_the_mounted_window_adapter() {
        on_slint_thread(|| {
            let (commands_tx, _rx) = tokio::sync::mpsc::channel(8);
            let (_stx, signals) = tokio::sync::broadcast::channel(4);
            let (_syt, system) = tokio::sync::broadcast::channel(4);
            let handle = mount(
                std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
                    taskboard_domain::AppState::default(),
                )),
                signals,
                system,
                commands_tx,
                UiSettings::default(),
            )
            .expect("mount succeeds on the null platform");

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
            let (commands_tx, _rx) = tokio::sync::mpsc::channel(8);
            let (_stx, signals) = tokio::sync::broadcast::channel(4);
            let (_syt, system) = tokio::sync::broadcast::channel(4);
            let handle = mount(
                std::sync::Arc::new(arc_swap::ArcSwap::from_pointee(
                    taskboard_domain::AppState::default(),
                )),
                signals,
                system,
                commands_tx,
                UiSettings::default(),
            )
            .expect("mount succeeds on the null platform");
            let adapter = window_adapter().expect("the registry holds the mounted adapter");
            assert_eq!(adapter.size(), PhysicalSize::new(0, 0));

            handle
                .window()
                .window()
                .set_size(slint::WindowSize::Physical(PhysicalSize::new(1366, 768)));
            assert_eq!(adapter.size(), PhysicalSize::new(1366, 768));
        });
    }
}
