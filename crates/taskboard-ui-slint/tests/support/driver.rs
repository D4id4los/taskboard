// SPDX-License-Identifier: AGPL-3.0-only

//! The journey-driver module (design §8.1, chunk decision D3) — the
//! churn-isolation seam: the **only** file that names
//! `i_slint_backend_testing` or Slint's platform event APIs. A Slint
//! bump edits this file and nothing else in the harness.
//!
//! Shape: a long-lived worker thread owns the process-global testing
//! platform (and the font context); journey jobs run on it serially
//! via [`run`]. Under nextest's process-per-test model every journey
//! gets a fresh process; under a threaded runner (`cargo test`,
//! llvm-cov) jobs serialise onto the worker. The platform is the
//! testing backend with `mock_time` (AGENTS §5: the mock clock is the
//! only clock — [`JourneyDriver::advance_time`] fires timers,
//! animations, change handlers, and tree instantiation) and, when
//! requested, the embedded software renderer for [`JourneyDriver::
//! snapshot`].
//!
//! Key injection goes through `Window::dispatch_event` — the same
//! public delivery path (window → focused item → the root
//! `FocusScope`'s capture handler) real backends use and Slint's own
//! gated test helpers call internally (D3). Modifier-carrying strokes
//! press and release their modifier keys around the character event;
//! a stroke is one physical press — held modifiers are plumbing and
//! contribute no count (§9.2 accounting).

#![allow(clippy::missing_panics_doc)] // harness-internal invariants

use i_slint_backend_testing::ElementHandle;
use i_slint_backend_testing::ElementQuery;
use i_slint_backend_testing::testing_backend::{
    TestingBackend, TestingBackendOptions, mock_elapsed_time,
};
use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle as _, LogicalSize, SharedString, WindowSize};

use taskboard_ui_slint::UiHandle;
use taskboard_ui_slint::keymap::Stroke;

use crate::support::golden_compare::Rgba8Image;

/// How the platform is initialised. Decides the first `run` call's
/// registration; a process has exactly one platform.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DriverOptions {
    /// Embeds the software rasterizer so `Window::take_snapshot`
    /// works headlessly (the golden tier). Compile-time guarantee:
    /// without the `renderer-software` dev feature the field is
    /// deleted — a compile error, never a silent fallback.
    pub headless_renderer: bool,
}

/// The harness's vocabulary over the testing backend (the whole
/// trait is the seam). Implemented by [`SlintDriver`].
pub trait JourneyDriver {
    /// Binds the driver to a mounted UI: injection, sizing, capture,
    /// and queries address its window.
    fn bind(&mut self, handle: &UiHandle);
    /// Advances the mock clock by `ms` (fires timers incl. the D1
    /// repaint driver, repeaters, change handlers).
    fn advance_time(&mut self, ms: u64);
    /// Injects one stroke through the real key path. Returns the
    /// count of physical presses contributed (always 1).
    fn press_stroke(&mut self, stroke: Stroke) -> u32;
    /// Delivers a real pointer click (press + release) at the logical
    /// window position — the dismissal layer's and the context menu's
    /// coordinate-space seam (round-1 D2/D13).
    fn click_at(&mut self, x: f32, y: f32);
    /// Delivers a pointer move (hover) at the logical window
    /// position — the tooltip trigger's seam (round-1 D8).
    fn hover_at(&mut self, x: f32, y: f32);
    /// The pointer-drag primitives (r2 chunk-8): raw left press /
    /// move / release dispatches, the slint `native_drag.rs` driving
    /// pattern — a press, moves past the 8-px slop, a release. The
    /// mock clock's handler pass (`advance_time`) delivers the
    /// property-change callbacks between them.
    fn press_at(&mut self, x: f32, y: f32);
    fn drag_move_to(&mut self, x: f32, y: f32);
    fn release_at(&mut self, x: f32, y: f32);
    /// A right-button press (r3 D4): the body-context seam's probe —
    /// the pointer-event down fires with the right button.
    fn right_press_at(&mut self, x: f32, y: f32);
    /// Sets the window size in logical px (scale factor 1 → also
    /// physical); drives the `Responsive` breakpoints.
    fn set_window_size(&mut self, width: u32, height: u32);
    /// Renders and captures the window (software renderer) as RGBA8.
    fn snapshot(&mut self) -> Rgba8Image;
    /// Finds the first element whose accessible label equals `label`
    /// (labels are element *addresses*, never assertion text).
    fn find_by_label(&self, label: &str) -> Option<Element>;
    /// Finds every element with the given qualified element id
    /// (`Component::element-name`) — one per repetition instance.
    fn find_all_by_id(&self, id: &str) -> Vec<Element>;
    /// Finds every element whose accessible label equals `label`.
    fn find_all_by_label(&self, label: &str) -> Vec<Element>;
    /// Delivers a real click to the labelled element's accessible
    /// default action (§7.1's click-equivalence seam — no coordinate
    /// arithmetic).
    fn click_by_label(&mut self, label: &str);
}

/// One addressed element: an [`ElementHandle`] behind only the a11y
/// read the journeys assert through — never item-tree internals
/// (design §14 journey-brittleness).
pub struct Element {
    handle: ElementHandle,
}

impl Element {
    /// The accessible label (the address, for `assert!` diagnostics).
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.handle.accessible_label().map(Into::into)
    }

    /// The element's size in logical px (the geometry oracle for the
    /// layout pins — header heights, column fills, glyph squares).
    #[must_use]
    pub fn size(&self) -> slint::LogicalSize {
        self.handle.size()
    }

    /// The accessible value (a text-input's buffer) — the
    /// buffer-emptiness oracle (r2 D24); a value is never assertion
    /// text, only emptiness/presence is pinned.
    #[must_use]
    pub fn value(&self) -> Option<String> {
        self.handle.accessible_value().map(Into::into)
    }

    /// The element's position in window coordinates (logical px).
    #[must_use]
    pub fn absolute_position(&self) -> slint::LogicalPosition {
        self.handle.absolute_position()
    }

    /// Whether the element is an accessible button — the
    /// disambiguator when a label (a column title) addresses several
    /// items (the column list *and* its header button).
    #[must_use]
    pub fn is_accessible_button(&self) -> bool {
        self.handle.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::Button)
    }

    /// Whether the element is an accessible radio button — the
    /// segmented-control pin's role read (r2 D26; `accessible-checked`
    /// has no backend read, so the checked-state contract pins on the
    /// model instead — the plan's recorded fallback).
    #[must_use]
    pub fn is_radio_button(&self) -> bool {
        self.handle.accessible_role() == Some(i_slint_backend_testing::AccessibleRole::RadioButton)
    }

    /// Invokes the element's accessible default action (the click
    /// equivalence seam, §7.1).
    pub fn invoke_default_action(&self) {
        self.handle.invoke_accessible_default_action();
    }
}

/// The [`JourneyDriver`] implementation over the testing backend.
#[derive(Default)]
pub struct SlintDriver {
    window: Option<slint::Weak<taskboard_ui_slint::AppWindow>>,
}

impl SlintDriver {
    fn window(&self) -> taskboard_ui_slint::AppWindow {
        self.window
            .as_ref()
            .expect("driver bound to a mounted UI first")
            .upgrade()
            .expect("the journey's window lives for the job's duration")
    }
}

// The trait surface the journey/golden binaries exercise; each binary
// uses its subset (the module-level `dead_code` allow in `mod.rs`
// covers the per-binary rest).
impl JourneyDriver for SlintDriver {
    fn bind(&mut self, handle: &UiHandle) {
        self.window = Some(handle.window().as_weak());
    }

    fn advance_time(&mut self, ms: u64) {
        mock_elapsed_time(ms);
    }

    fn press_stroke(&mut self, stroke: Stroke) -> u32 {
        let component = self.window();
        let window = component.window();
        let mut held: Vec<Key> = Vec::new();
        if stroke.ctrl {
            held.push(Key::Control);
        }
        if stroke.alt {
            held.push(Key::Alt);
        }
        if stroke.shift {
            held.push(Key::Shift);
        }
        if stroke.meta {
            held.push(Key::Meta);
        }
        for key in &held {
            dispatch(window, Key::into(*key), true);
        }
        let mut key = String::new();
        key.push(stroke.key);
        dispatch(window, SharedString::from(key), true);
        for key in held.iter().rev() {
            dispatch(window, Key::into(*key), false);
        }
        1
    }

    fn click_at(&mut self, x: f32, y: f32) {
        let position = slint::LogicalPosition::new(x, y);
        let button = i_slint_core::items::PointerEventButton::Left;
        for event in [
            slint::platform::WindowEvent::PointerMoved { position },
            slint::platform::WindowEvent::PointerPressed { position, button },
            slint::platform::WindowEvent::PointerReleased { position, button },
        ] {
            self.dispatch_pointer(event);
        }
    }

    fn hover_at(&mut self, x: f32, y: f32) {
        self.dispatch_pointer(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
    }

    fn press_at(&mut self, x: f32, y: f32) {
        self.dispatch_pointer(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: i_slint_core::items::PointerEventButton::Left,
        });
    }

    fn drag_move_to(&mut self, x: f32, y: f32) {
        self.dispatch_pointer(slint::platform::WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(x, y),
        });
    }

    fn release_at(&mut self, x: f32, y: f32) {
        self.dispatch_pointer(slint::platform::WindowEvent::PointerReleased {
            position: slint::LogicalPosition::new(x, y),
            button: i_slint_core::items::PointerEventButton::Left,
        });
    }

    fn right_press_at(&mut self, x: f32, y: f32) {
        self.dispatch_pointer(slint::platform::WindowEvent::PointerPressed {
            position: slint::LogicalPosition::new(x, y),
            button: i_slint_core::items::PointerEventButton::Right,
        });
    }

    fn set_window_size(&mut self, width: u32, height: u32) {
        #[allow(clippy::cast_precision_loss)] // UI-scale px values
        let size = LogicalSize::new(width as f32, height as f32);
        self.window().window().set_size(WindowSize::from(size));
        // The markup's width push is a property change handler; the
        // mock clock's handler pass delivers it.
        mock_elapsed_time(0);
    }

    fn snapshot(&mut self) -> Rgba8Image {
        let binding = self.window();
        let buffer = binding
            .window()
            .take_snapshot()
            .expect("the software renderer captures the window (renderer-presence contract)");
        Rgba8Image::new(buffer.width(), buffer.height(), buffer.as_bytes().to_vec())
    }

    fn find_by_label(&self, label: &str) -> Option<Element> {
        let window = self.window();
        ElementHandle::find_by_accessible_label(&window, label)
            .next()
            .map(|handle| Element { handle })
    }

    fn find_all_by_id(&self, id: &str) -> Vec<Element> {
        let window = self.window();
        dedupe_instances(
            ElementQuery::from_root(&window)
                .match_descendants()
                .match_id(id)
                .find_all()
                .into_iter()
                // The backend's chain match also reports a match for
                // descendants of the addressed element; only the exact
                // qualified id is the element itself.
                .filter(|handle| handle.id().as_deref() == Some(id))
                .map(|handle| Element { handle })
                .collect(),
        )
    }

    fn find_all_by_label(&self, label: &str) -> Vec<Element> {
        let window = self.window();
        ElementHandle::find_by_accessible_label(&window, label)
            .map(|handle| Element { handle })
            .collect()
    }

    fn click_by_label(&mut self, label: &str) {
        let element = self
            .find_by_label(label)
            .unwrap_or_else(|| panic!("the labelled element {label:?} is present"));
        element.invoke_default_action();
    }
}

impl SlintDriver {
    fn dispatch_pointer(&mut self, event: slint::platform::WindowEvent) {
        let component = self.window();
        component.window().dispatch_event(event);
    }
}

/// The test-local component seams (the #373 virtualisation probes):
/// the same queries as the `JourneyDriver` methods, over any
/// generated component — the probes instantiate `slint::slint!`
/// hosts that are not the `AppWindow`, and this module stays the only
/// file naming the testing backend.
pub fn find_all_by_id_on<C: slint::ComponentHandle>(component: &C, id: &str) -> Vec<Element> {
    dedupe_instances(
        ElementQuery::from_root(component)
            .match_id(id)
            .find_all()
            .into_iter()
            .filter(|handle| handle.id().as_deref() == Some(id))
            .map(|handle| Element { handle })
            .collect(),
    )
}

/// Collapses the testing backend's element-query traversal
/// duplicates to one `Element` per real instance. The backend's
/// `visit_descendants` yields the same node once per multi-element
/// ancestor it is reached through (the #373 instrument lesson: an
/// id query over a deep tree reported an 11x multiplicity the
/// repeater — `ops.len()` — did not have), so identity is taken
/// from the node's own geometry: duplicates of one instance read
/// back exactly equal positions and sizes; distinct list
/// delegates never overlap.
fn dedupe_instances(elements: Vec<Element>) -> Vec<Element> {
    let mut seen = std::collections::HashSet::new();
    elements
        .into_iter()
        .filter(|element| {
            let position = element.absolute_position();
            let size = element.size();
            let key = (
                position.x.to_bits(),
                position.y.to_bits(),
                size.width.to_bits(),
                size.height.to_bits(),
            );
            seen.insert(key)
        })
        .collect()
}

/// The labelled-element search over any generated component (the
/// "engaged but renders nothing" guard's seam).
pub fn find_by_label_on<C: slint::ComponentHandle>(component: &C, label: &str) -> Option<Element> {
    ElementHandle::find_by_accessible_label(component, label)
        .next()
        .map(|handle| Element { handle })
}

/// Advances the mock clock for the platform the process registered
/// through [`run`] (the probes instantiate their own components but
/// share the one registered testing backend).
pub fn advance_probe_clock(ms: u64) {
    mock_elapsed_time(ms);
}

/// The card/element centre helper the drag journeys address by.
pub fn centre(element: &Element) -> (f32, f32) {
    let pos = element.absolute_position();
    let size = element.size();
    (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

fn dispatch(window: &slint::Window, text: SharedString, pressed: bool) {
    let event = if pressed {
        WindowEvent::KeyPressed { text }
    } else {
        WindowEvent::KeyReleased { text }
    };
    window.dispatch_event(event);
}

/// Runs `job` on the driver thread, registering the testing platform
/// there on the first call. Every journey runs as one whole job:
/// mount, drive, assert, drop.
///
/// # Panics
///
/// Panics if the job panics, if the platform is already registered
/// elsewhere, or if two `run` calls request different
/// [`DriverOptions`] (never: options are per test binary).
pub fn run<R>(options: DriverOptions, job: impl FnOnce(&mut SlintDriver) -> R + Send + 'static) -> R
where
    R: Send + 'static,
{
    type Job = Box<dyn FnOnce() + Send>;
    static SENDER: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
    static OPTIONS: std::sync::OnceLock<DriverOptions> = std::sync::OnceLock::new();

    let first = OPTIONS.get_or_init(|| options);
    assert_eq!(
        *first, options,
        "a process has one platform: journey binaries must agree on DriverOptions"
    );
    let sender = SENDER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("journey-driver".into())
            .spawn(move || {
                let renderer_name = options
                    .headless_renderer
                    .then(|| slint::SharedString::from("software"));
                slint::platform::set_platform(Box::new(TestingBackend::new(
                    TestingBackendOptions {
                        mock_time: true,
                        threading: false,
                        renderer_name,
                    },
                )))
                .expect("the testing platform registers exactly once per journey process");
                for job in rx {
                    job();
                }
            })
            .expect("the driver worker spawns");
        tx
    });

    let (reply_tx, reply_rx) = std::sync::mpsc::channel();
    sender
        .send(Box::new(move || {
            let mut driver = SlintDriver::default();
            let _ = reply_tx.send(job(&mut driver));
        }))
        .expect("the driver worker is alive");
    reply_rx.recv().expect("the journey job returned a result")
}

#[cfg(test)]
mod tests {
    use super::*;
    use arc_swap::ArcSwap;
    use std::sync::Arc;
    use taskboard_domain::AppState;
    use taskboard_ui_slint::UiSettings;
    use tokio::sync::{broadcast, mpsc};

    /// T4 — the harness itself boots: platform init, mount, first
    /// refresh, the accessible "Board" query resolves, and
    /// `set_window_size` maps the breakpoints.
    #[test]
    fn driver_boots_mounts_and_addresses_the_board() {
        run(
            DriverOptions {
                headless_renderer: true,
            },
            |driver: &mut SlintDriver| {
                let (commands_tx, _commands_rx) = mpsc::channel(16);
                let (requests_tx, _requests_rx) = mpsc::channel(16);
                let (system_tx, system_rx) = broadcast::channel(4);
                let (signals_tx, signals_rx) = broadcast::channel(4);
                drop(system_tx);
                drop(signals_tx);
                let handle = taskboard_ui_slint::mount(
                    Arc::new(ArcSwap::from_pointee(AppState::default())),
                    signals_rx,
                    system_rx,
                    commands_tx,
                    requests_tx,
                    UiSettings::default(),
                )
                .expect("mount succeeds on the testing backend");
                // Escape the desktop Today default (chunk 10 D2): the
                // self-tests address the board region.
                handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
                driver.bind(&handle);
                // Instantiate conditional trees before querying.
                driver.advance_time(0);

                let board = driver
                    .find_by_label("Board")
                    .expect("the board region is addressable by its a11y label");
                assert_eq!(board.label().as_deref(), Some("Board"));

                driver.set_window_size(600, 800);
                assert_eq!(
                    handle
                        .window()
                        .global::<taskboard_ui_slint::Responsive>()
                        .get_current_breakpoint(),
                    taskboard_ui_slint::Breakpoint::Compact,
                    "the width push maps the compact breakpoint"
                );
                driver.set_window_size(1500, 900);
                assert_eq!(
                    handle
                        .window()
                        .global::<taskboard_ui_slint::Responsive>()
                        .get_current_breakpoint(),
                    taskboard_ui_slint::Breakpoint::Expansive,
                );

                // The mock clock advances without an event loop.
                driver.advance_time(1_000);
            },
        );
    }

    /// T-0 (r2 chunk 1) — the geometry oracle exists: the bound
    /// element reads its logical size back (window-filling region),
    /// the find-all queries address repetition instances (empty over
    /// an absent id / a single over the one labelled element).
    ///
    /// Uses the house `headless_renderer: true` convention: `run`
    /// pins one `DriverOptions` per process (`#431`), so both
    /// self-tests in this binary must agree; every journey uses
    /// `true` and the geometry reads below do not depend on the
    /// renderer-name arm that `false` skips. Covering that arm is a
    /// separate test binary's job, deliberately out of scope here.
    #[test]
    fn driver_reads_element_geometry_and_queries_all_instances() {
        run(
            DriverOptions {
                headless_renderer: true,
            },
            |driver: &mut SlintDriver| {
                let (commands_tx, _commands_rx) = mpsc::channel(16);
                let (requests_tx, _requests_rx) = mpsc::channel(16);
                let (system_tx, system_rx) = broadcast::channel(4);
                let (signals_tx, signals_rx) = broadcast::channel(4);
                drop(system_tx);
                drop(signals_tx);
                let handle = taskboard_ui_slint::mount(
                    Arc::new(ArcSwap::from_pointee(AppState::default())),
                    signals_rx,
                    system_rx,
                    commands_tx,
                    requests_tx,
                    UiSettings::default(),
                )
                .expect("mount succeeds on the testing backend");
                // Escape the desktop Today default (chunk 10 D2): the
                // self-tests address the board region.
                handle.dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
                driver.bind(&handle);
                driver.advance_time(0);
                driver.set_window_size(1000, 800);

                let board = driver
                    .find_by_label("Board")
                    .expect("the board region is addressable");
                let size = board.size();
                assert_eq!(size.width, 1000.0, "the region reads its width back");
                assert!(size.height > 0.0, "the region reads a real height");
                let position = board.absolute_position();
                assert_eq!(position.x, 0.0);
                assert!(
                    position.y > 0.0,
                    "the region sits below the toolbar (window coordinates)"
                );

                assert!(
                    driver.find_all_by_id("BoardLayout::column-body").is_empty(),
                    "an absent element id finds nothing"
                );
                assert_eq!(
                    driver.find_all_by_label("Board").len(),
                    1,
                    "the find-all label query addresses every instance"
                );
            },
        );
    }
}
