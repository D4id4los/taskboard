// SPDX-License-Identifier: AGPL-3.0-only
#![deny(unsafe_code)]
//! Slint presentation layer for `taskboard`.
//!
//! The UI is a "dumb presentation client": it reads state via lock-free
//! `arc_swap::ArcSwap<AppState>` loads, maps fields onto widgets, and emits
//! non-blocking [`tokio::sync::mpsc`] commands on user interaction.
//! Business logic and persistence are forbidden here.
//!
//! NOTE: `deny`, not `forbid`: the Slint-generated code carries internal
//! `allow(unsafe_code)` scopes that a `forbid` would overrule (E0453 ×36,
//! audit chunk 7 §2.3). The crate-level deny coexists with those internal
//! allows and fires exactly on hand-written `unsafe` — a future
//! hand-written block fails the build unless a local, grep-visible
//! `#[allow(unsafe_code)]` is added at the site. This remains the only
//! workspace crate without `#![forbid(unsafe_code)]`.

slint::include_modules!();

pub mod actions;
pub mod adapter;
pub mod fonts;
pub mod keymap;
pub mod menu;
pub mod models;
pub mod presentation;
pub mod resolver;
pub mod responsive;
pub mod router;
pub mod shell;
pub mod view_state;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use tokio::sync::{broadcast, mpsc};

use taskboard_domain::{AppState, EngineSignal, StateCommand, SystemEvent};

#[cfg(test)]
mod test_platform;
#[cfg(test)]
mod token_tests;

/// Validated configuration values handed over by `taskboard-app` (the
/// UI crate reads no configuration itself — IO-free by design; design
/// §1.2).
#[derive(Debug, Clone, PartialEq)]
pub struct UiSettings {
    /// The `[ui] scale` value: `Theme.base-scale`'s initial value
    /// (§6.1), validated app-side to the 0.5–3.0 range.
    pub scale: f64,
    /// The validated effective keymap (D14): preset ⊕ surviving
    /// overrides, built app-side — the crate receives typed values.
    pub keymap: keymap::Keymap,
    /// The Q15 startup-warning state (count of dropped/lossy keymap
    /// findings), rendered by chunk 3's status bar.
    pub startup: adapter::StartupWarnings,
    /// The kiosk window profile (Q5): mount applies fullscreen to the
    /// shared `AppWindow`. Derives from the run mode — never a config
    /// key; the desktop hand-over leaves it `false`.
    pub kiosk: bool,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scale: 1.0,
            keymap: keymap::Keymap::from_preset(keymap::Preset::Standard),
            startup: adapter::StartupWarnings::default(),
            kiosk: false,
        }
    }
}

/// Mount failures, typed (thiserror, the app-crate error convention).
/// (Font registration is deliberately absent: the shared-collection
/// registration path is infallible by construction — it fails by
/// panicking, which surfaces before window creation and is not a
/// recoverable mount outcome.)
#[derive(Debug, thiserror::Error)]
pub enum UiMountError {
    /// Slint's window/component creation failed (platform init,
    /// adapter creation).
    #[error("window creation failed")]
    WindowCreation(#[source] slint::PlatformError),
}

/// Event-loop failures, typed.
#[derive(Debug, thiserror::Error)]
pub enum UiRunError {
    /// Showing the window before entering the loop failed.
    #[error("window show failed")]
    WindowShow(#[source] slint::PlatformError),
    /// The slint event loop returned an error.
    #[error("event loop failed")]
    EventLoop(#[source] slint::PlatformError),
}

/// The mounted UI: the window instance plus the seam halves chunks 2–3
/// snap into (the resolver dispatches through `commands`, the views
/// read `state`, the status bar reads `system`/`signals`). No
/// machinery is built around them in chunk 1 — undrained broadcast
/// receivers merely go `Lagged`, which is inert.
pub struct UiHandle {
    window: AppWindow,
    /// The keymap adapter (chunk 2): interception, shell state, the
    /// view-state machine, and the D9 interpretation table.
    adapter: Rc<RefCell<adapter::Adapter>>,
    /// Lock-free state reads for the rendering thread (AGENTS §3);
    /// shared with the adapter at mount, kept here for the tests'
    /// state injection.
    #[cfg_attr(not(test), allow(dead_code))]
    state: Arc<ArcSwap<AppState>>,
    /// The D1 repaint driver (signal drain + minute boundary +
    /// refresh). Started by `mount`; never fires without a running
    /// event loop (the tests call `refresh` directly).
    timer: slint::Timer,
    /// The tooltip delay driver (usability D3). Arming is
    /// invocation-driven so a repeated hover restarts the window.
    /// Never read: the closures hold the working handles; this field
    /// owns the timer's lifetime alongside the window.
    #[allow(dead_code)]
    tooltip_timer: std::rc::Rc<slint::Timer>,
}

/// Creates the window and applies the theme state (theme init is a
/// window-construction step, §3.1): registers the bundled fonts
/// (idempotent), instantiates [`AppWindow`] on the platform's backend,
/// and pushes `settings.scale` into `Theme.base-scale` through the
/// generated global setter.
///
/// # Errors
///
/// [`UiMountError`] — typed, see the variant docs.
pub fn mount(
    state: Arc<ArcSwap<AppState>>,
    signals: broadcast::Receiver<EngineSignal>,
    system: broadcast::Receiver<SystemEvent>,
    commands: mpsc::Sender<StateCommand>,
    settings: UiSettings,
) -> Result<UiHandle, UiMountError> {
    fonts::register_bundled_fonts();
    let window = AppWindow::new().map_err(UiMountError::WindowCreation)?;
    // The §6.1 root multiplier, one line per window construction — a
    // future second window re-runs it; no global-once init exists.
    // Slint `float` properties are f32; the validated 0.5–3.0 range
    // converts with precision far below any visible scale step.
    #[allow(clippy::cast_possible_truncation)]
    window
        .global::<Theme>()
        .set_base_scale(settings.scale as f32);
    // The kiosk window profile (Q5): fullscreen on the shared desktop
    // window. Slint 1.18.1 has no independent decoration control; on
    // the winit backend fullscreen suppresses decorations by
    // construction, so fullscreen alone is the complete profile.
    if settings.kiosk {
        window.window().set_fullscreen(true);
    }
    // The keymap is applied one-way at UI start (§5.6): install the
    // root-scope interception over the validated keymap.
    let adapter = adapter::install(
        &window,
        settings.keymap,
        settings.startup,
        commands,
        Arc::clone(&state),
    );
    let tooltip_timer = install_tooltip_delay(&window);
    let handle = UiHandle {
        window,
        adapter,
        state,
        timer: slint::Timer::default(),
        tooltip_timer,
    };
    start_repaint_driver(&handle, signals, system);
    // The first paint is a refresh, not a timer wait (D1's
    // first-refresh flag, collapsed into an immediate pass).
    handle.refresh();
    // The cold-start harness's moment (chunk-5 plan D5): the mount
    // seam's proxy for "first rendered frame" — after this initial
    // refresh the window's models hold the full first-frame content;
    // the show→presented delta has no public slint 1.18.1 API.
    // Consumed by structured-field selection on a JSON log stream,
    // never by message-text parsing.
    tracing::info!(
        target: "taskboard_perf",
        moment = "first-frame",
        "first-frame moment: initial refresh pushed models to the window"
    );
    Ok(handle)
}

/// Arms the tooltip delay driver (usability pass D3): a one-shot
/// slint timer restarted on every `delay-requested` invocation — the
/// invocation path (not property changes) is what makes a re-hover
/// inside the delay window restart it. Fires under the testing
/// backend's mock clock, so the component tests are deterministic.
fn install_tooltip_delay(window: &AppWindow) -> std::rc::Rc<slint::Timer> {
    let tooltip = window.global::<TooltipState>();
    let weak = window.as_weak();
    let timer = std::rc::Rc::new(slint::Timer::default());
    let arm = std::rc::Rc::clone(&timer);
    let fire_weak = weak.clone();
    tooltip.on_delay_requested(move || {
        let fire_weak = fire_weak.clone();
        arm.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(TOOLTIP_DELAY_MS),
            move || {
                if let Some(window) = fire_weak.upgrade() {
                    let tooltip = window.global::<TooltipState>();
                    tooltip.set_visible(true);
                    tooltip.set_pending(false);
                }
            },
        );
    });
    let dismiss_timer = std::rc::Rc::clone(&timer);
    tooltip.on_dismiss_requested(move || {
        dismiss_timer.stop();
    });
    timer
}

/// The tooltip delay (usability D3): a duration, not a layout
/// dimension.
pub const TOOLTIP_DELAY_MS: u64 = 600;

/// Starts the D1 repaint driver: a repeating slint `Timer` on the
/// slint thread that drains the engine-signal and system-event
/// receivers (non-blocking; `Lagged` counts as changed — chunk 1's
/// recorded semantics), detects the wall-minute boundary (the
/// reminders §7 time repaint), and refreshes when anything fired.
fn start_repaint_driver(
    handle: &UiHandle,
    mut signals: broadcast::Receiver<EngineSignal>,
    mut system: broadcast::Receiver<SystemEvent>,
) {
    let weak = handle.window.as_weak();
    let adapter = Rc::clone(&handle.adapter);
    let minute = std::cell::Cell::new(None::<i64>);
    handle.timer.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(adapter::REPAINT_CADENCE_MS),
        move || {
            let Some(window) = weak.upgrade() else { return };
            let mut dirty = false;
            loop {
                match signals.try_recv() {
                    Ok(_) => dirty = true,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {
                        dirty = true;
                        break;
                    }
                    Err(
                        broadcast::error::TryRecvError::Empty
                        | broadcast::error::TryRecvError::Closed,
                    ) => break,
                }
            }
            loop {
                match system.try_recv() {
                    // `Shutdown` is inert here (the app-side exit
                    // watcher owns quit); the rest are refresh
                    // triggers only — sync truth renders from state.
                    Ok(_) => dirty = true,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => {
                        dirty = true;
                        break;
                    }
                    Err(
                        broadcast::error::TryRecvError::Empty
                        | broadcast::error::TryRecvError::Closed,
                    ) => break,
                }
            }
            let wall_minute = chrono::Utc::now().timestamp() / 60;
            if minute.get() != Some(wall_minute) {
                minute.set(Some(wall_minute));
                dirty = true;
            }
            if dirty {
                adapter.borrow_mut().refresh(&window);
            }
        },
    );
}

impl UiHandle {
    /// The D1 refresh: a deterministic, timer-free full re-derive
    /// pass over the current state snapshot. The repaint timer calls
    /// this; tests may call it directly.
    pub fn refresh(&self) {
        if let Some(window) = self.window.as_weak().upgrade() {
            self.adapter.borrow_mut().refresh(&window);
        }
    }

    /// The exit-effect counter's read side (design §7.3 step 10 /
    /// chunk decision D6): the keyboard-journey suite (an integration
    /// binary) asserts the typed quit outcome without killing the
    /// platform. Presentation state, not new behaviour.
    #[must_use]
    pub fn quit_requests(&self) -> usize {
        self.adapter.borrow().quit_requests
    }

    /// The window component's read seam for the integration-test
    /// harness (design §10 component tier): journeys assert through
    /// the generated getters and the a11y tree exactly as the
    /// in-crate component tests do. Read-only by convention — the
    /// tests mutate state through the fake engine's `ArcSwap`, never
    /// by pushing window properties.
    #[must_use]
    pub fn window(&self) -> &AppWindow {
        &self.window
    }
}

/// Runs the slint event loop on the calling thread — the process main
/// thread (winit's macOS requirement; the multi-thread tokio runtime's
/// workers keep driving every actor while `block_on` is blocked here).
///
/// # Errors
///
/// [`UiRunError`] — typed, see the variant docs.
pub fn run(handle: UiHandle) -> Result<(), UiRunError> {
    // A quit requested before the loop existed (a shutdown signal
    // racing the boot) is honored here: skip the loop entirely — the
    // caller's shutdown tail runs and the window is never shown.
    if QUIT_REQUESTED_BEFORE_LOOP.swap(false, std::sync::atomic::Ordering::SeqCst) {
        tracing::info!("quit was requested before the event loop started; skipping run");
        return Ok(());
    }
    // Consumes the handle: the window lives for the loop's duration
    // and drops with it when the loop returns.
    let UiHandle { window, .. } = handle;
    window.show().map_err(UiRunError::WindowShow)?;
    EVENT_LOOP_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    slint::run_event_loop().map_err(UiRunError::EventLoop)
}

/// A quit requested before [`run`] started the event loop. The winit
/// backend's proxy `quit_event_loop` reports `Ok` even when the loop
/// does not exist yet and silently drops the request — a shutdown
/// signal racing the boot (hydrate + mount) left the process running
/// forever. [`quit`] parks the request here; [`run`] honors it.
static QUIT_REQUESTED_BEFORE_LOOP: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether [`run`] has started the slint event loop (one loop per
/// process; never unset). See [`event_loop_running`].
static EVENT_LOOP_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// True once [`run`] has started the slint event loop — the only state
/// in which a [`quit`] is guaranteed to be delivered by the backend.
#[must_use]
pub fn event_loop_running() -> bool {
    EVENT_LOOP_RUNNING.load(std::sync::atomic::Ordering::SeqCst)
}

/// Requests the event loop to quit. Also parks the request on
/// [`QUIT_REQUESTED_BEFORE_LOOP`]: a quit landing before [`run`] started
/// the loop is not reliably delivered by the backend, so [`run`] skips
/// straight to shutdown when the parked flag is set. Harmless when the
/// loop is already running (one loop per process).
///
/// # Errors
///
/// [`slint::EventLoopError`] — as reported by the backend (the loop
/// already terminated); the pre-loop parking path returns the backend's
/// own result unchanged.
pub fn quit() -> Result<(), slint::EventLoopError> {
    QUIT_REQUESTED_BEFORE_LOOP.store(true, std::sync::atomic::Ordering::SeqCst);
    slint::quit_event_loop()
}

#[cfg(test)]
mod mount_tests {
    use super::*;
    use crate::test_platform::on_slint_thread;

    pub(super) fn handover() -> (
        Arc<ArcSwap<AppState>>,
        broadcast::Receiver<EngineSignal>,
        broadcast::Receiver<SystemEvent>,
        mpsc::Sender<StateCommand>,
    ) {
        let (commands_tx, _commands_rx) = mpsc::channel(64);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        drop(system_tx);
        drop(signals_tx);
        (
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
        )
    }

    /// The dispatch acceptance line: a valid scale reaches
    /// `Theme.base-scale` via the generated `Global` setter.
    #[test]
    fn mount_pushes_the_configured_scale_into_the_theme() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                UiSettings {
                    scale: 2.0,
                    ..UiSettings::default()
                },
            )
            .expect("mount succeeds on the null platform");
            let base_scale = handle.window.global::<Theme>().get_base_scale();
            assert_eq!(base_scale, 2.0);
        });
    }

    #[test]
    fn mount_defaults_the_theme_scale_to_one() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            assert_eq!(handle.window.global::<Theme>().get_base_scale(), 1.0);
        });
    }

    /// The D17 width push round-trip: a width reported through the
    /// window callback lands as the mapped breakpoint on the
    /// `Responsive` global; the null platform's 0×0 keeps the default.
    #[test]
    fn width_push_maps_the_breakpoint() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            let responsive = handle.window.global::<Responsive>();
            assert_eq!(responsive.get_current_breakpoint(), Breakpoint::Regular);

            handle.window.invoke_window_width_changed(600.0);
            assert_eq!(responsive.get_current_breakpoint(), Breakpoint::Compact);
            handle.window.invoke_window_width_changed(1000.0);
            assert_eq!(responsive.get_current_breakpoint(), Breakpoint::Regular);
            handle.window.invoke_window_width_changed(2000.0);
            assert_eq!(responsive.get_current_breakpoint(), Breakpoint::Expansive);
            handle.window.invoke_window_width_changed(0.0);
            assert_eq!(
                responsive.get_current_breakpoint(),
                Breakpoint::Regular,
                "no-measurement widths fall back to the default breakpoint"
            );
        });
    }

    /// Theme init is a window-construction step (§3.1): mount produces
    /// a window instance that `run()` will show — still hidden here
    /// (the event loop never ran).
    #[test]
    fn mount_produces_a_hidden_window_instance() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            assert!(!handle.window.window().is_visible());
        });
    }

    /// The kiosk window profile (Q5): mount applies fullscreen to the
    /// shared desktop window when the flag is handed over.
    #[test]
    fn mount_applies_the_kiosk_fullscreen_profile() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            )
            .expect("mount succeeds on the null platform");
            assert!(handle.window.window().is_fullscreen());
        });
    }

    /// The flag defaults off and the guarded application is a no-op:
    /// the desktop mount stays windowed (covers both the `Default`
    /// arm and the guard's off-path in one observable behavior).
    #[test]
    fn mount_without_the_kiosk_flag_stays_windowed() {
        on_slint_thread(|| {
            let (state, signals, system, commands) = handover();
            let handle = mount(state, signals, system, commands, UiSettings::default())
                .expect("mount succeeds on the null platform");
            assert!(!handle.window.window().is_fullscreen());
        });
    }
}

/// The cold-start harness's contract (chunk-5 plan §2/D5): `mount`
/// emits exactly one `taskboard_perf` event carrying the structured
/// field `moment = "first-frame"` — selected by the field, never by
/// message prose. Captured through a handwritten capturing `Layer`
/// over `Registry` (fakes over mocks); no message text is asserted.
#[cfg(test)]
mod first_frame_tests {
    use super::mount_tests::handover;
    use crate::test_platform::on_slint_thread;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::prelude::*;

    type Captured = Arc<Mutex<Vec<(String, BTreeMap<String, String>)>>>;

    /// A capturing layer: every event lands as (target, field map),
    /// stringified via `Debug` — the typed field survives verbatim.
    struct FieldCapture(Captured);

    struct FieldVisitor<'a>(&'a mut BTreeMap<String, String>);

    impl tracing::field::Visit for FieldVisitor<'_> {
        fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
            self.0.insert(field.name().to_owned(), value.to_owned());
        }

        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            self.0.insert(field.name().to_owned(), format!("{value:?}"));
        }
    }

    impl<S> tracing_subscriber::Layer<S> for FieldCapture
    where
        S: tracing::Subscriber,
    {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            let mut fields = BTreeMap::new();
            event.record(&mut FieldVisitor(&mut fields));
            self.0
                .lock()
                .expect("capture lock")
                .push((event.metadata().target().to_owned(), fields));
        }
    }

    #[test]
    fn mount_emits_exactly_one_structured_first_frame_event() {
        let captured: Captured = Arc::new(Mutex::new(Vec::new()));
        let reader: Captured = Arc::clone(&captured);
        on_slint_thread(move || {
            let subscriber =
                tracing_subscriber::Registry::default().with(FieldCapture(Arc::clone(&captured)));
            // Thread-local default on the slint thread; `mount` runs
            // there, so its event is captured. The guard is dropped at
            // closure end.
            let guard = tracing::subscriber::set_default(subscriber);
            let (state, signals, system, commands) = handover();
            crate::mount(
                state,
                signals,
                system,
                commands,
                crate::UiSettings::default(),
            )
            .expect("mount succeeds on the null platform");
            drop(guard);
        });
        let events = reader.lock().expect("capture lock");
        let perf_events: Vec<_> = events
            .iter()
            .filter(|(target, _)| target == "taskboard_perf")
            .collect();
        assert_eq!(
            perf_events.len(),
            1,
            "exactly one taskboard_perf event per mount, got: {events:?}"
        );
        let moment = perf_events[0]
            .1
            .get("moment")
            .expect("the first-frame event carries a `moment` field");
        assert_eq!(moment, "first-frame");
    }
}

#[cfg(test)]
mod font_smoke_tests {
    use slint::ComponentHandle as _;

    use crate::fonts::register_bundled_fonts;
    use crate::test_platform::on_slint_thread;

    /// D11/D14 smoke: with the null platform registered, the bundled
    /// fonts register, a window component instantiates (its font
    /// properties resolve against the registered families' names), and
    /// the instance drops cleanly — no panic anywhere in the cycle.
    #[test]
    fn registration_and_instantiation_smoke() {
        on_slint_thread(|| {
            register_bundled_fonts(); // idempotent; a second call is a no-op
            register_bundled_fonts();

            let window = crate::AppWindow::new().expect("window instantiates on the null platform");
            // The theme's Typography globals resolve their family names;
            // reading them exercises the generated global accessors.
            let _ = window.global::<crate::Typography>().get_family_sans();
            let _ = window.global::<crate::Typography>().get_family_mono();
            drop(window);
        });
    }
}

#[cfg(test)]
mod keymap_adapter_tests {
    use super::*;
    use crate::keymap::{Preset, RawKeymap, build};
    use crate::test_platform::on_slint_thread;
    use slint::Model as _;
    use slint::platform::WindowEvent;
    use std::collections::BTreeMap;

    fn vi_keymap() -> keymap::Keymap {
        build(&RawKeymap {
            preset: Preset::Vi,
            overrides: BTreeMap::new(),
        })
        .keymap
    }

    fn handover_vi() -> (UiHandle, mpsc::Receiver<StateCommand>) {
        let (commands_tx, commands_rx) = mpsc::channel(8);
        let (system_tx, system_rx) = broadcast::channel(4);
        let (signals_tx, signals_rx) = broadcast::channel(4);
        drop(system_tx);
        drop(signals_tx);
        let handle = mount(
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
            UiSettings {
                keymap: vi_keymap(),
                ..UiSettings::default()
            },
        )
        .expect("mount succeeds on the null platform");
        (handle, commands_rx)
    }

    /// Injects one key through the same public dispatch path real
    /// backends use (the root `FocusScope`'s capture handler; focused
    /// by the window's `init`).
    fn press(handle: &UiHandle, text: &str) {
        handle
            .window
            .window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
    }

    /// T25 — the generated menu journey: `app.menu` (vi `:`) opens
    /// the menu, `menu.next` (`j`) moves the cursor, and invoking an
    /// entry closes the menu and re-dispatches the entry's action.
    #[test]
    fn injected_keys_open_navigate_and_invoke_the_menu() {
        on_slint_thread(|| {
            let (handle, _commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();

            press(&handle, ":");
            assert!(
                keyboard.get_menu_open(),
                "app.menu opens the generated menu"
            );
            assert_eq!(keyboard.get_menu_selected_index(), 0);
            assert!(
                !keyboard
                    .get_menu_entries()
                    .row_data(0)
                    .expect("rows")
                    .label
                    .is_empty()
            );

            press(&handle, "j");
            assert_eq!(
                keyboard.get_menu_selected_index(),
                1,
                "menu.next moves the cursor"
            );
            press(&handle, "k");
            assert_eq!(
                keyboard.get_menu_selected_index(),
                0,
                "menu.previous moves back"
            );

            // Invoke the selected entry (app.exit is the first
            // application row): the menu closes; the re-dispatched
            // action is the adapter's to route.
            press(&handle, "\u{a}");
            assert!(!keyboard.get_menu_open(), "invoke closes the menu");
        });
    }

    /// T26 — the chunk-2 end-to-end path: a `task.complete` keystroke
    /// with focused entities set sends the exact `StateCommand` to
    /// the mpsc receiver (the forwarder boundary).
    #[test]
    fn complete_keystroke_sends_the_exact_command() {
        on_slint_thread(|| {
            let (commands_tx, mut commands_rx) = mpsc::channel(8);
            let (system_tx, system_rx) = broadcast::channel(4);
            let (signals_tx, signals_rx) = broadcast::channel(4);
            drop(system_tx);
            drop(signals_tx);
            let handle = mount(
                Arc::new(ArcSwap::from_pointee(AppState::default())),
                signals_rx,
                system_rx,
                commands_tx,
                UiSettings::default(), // standard preset
            )
            .expect("mount succeeds on the null platform");

            // The chunk-3 focus seam, driven from the test side: a
            // board view with a focused task.
            let task = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            handle.set_view_context(Some(actions::ContextId::Board));
            handle.set_focused_entities(Some(task), None);

            press(&handle, " "); // task.complete = Space in Board
            let command = commands_rx
                .try_recv()
                .expect("the command reached the mpsc");
            assert_eq!(
                command,
                StateCommand::SetTaskDone {
                    id: task,
                    done: true
                }
            );

            press(&handle, "\u{7f}"); // task.delete = Delete in Board
            let command = commands_rx.try_recv().expect("the delete command arrived");
            assert_eq!(command, StateCommand::DeleteTask { id: task });

            assert!(commands_rx.try_recv().is_err(), "nothing else dispatched");
        });
    }

    /// T27 — the §5.7 status signal: a consumed prefix pushes
    /// prefix-pending, a failed continuation pushes prefix-abort, and
    /// an unbound key dispatches nothing (rejected event).
    #[test]
    fn prefix_feedback_and_unhandled_keys() {
        on_slint_thread(|| {
            let (handle, mut commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();

            press(&handle, "d"); // prefix of `dd` (task.delete in ViNormal)
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::PrefixPending);

            press(&handle, "x"); // not the second `d`: prefix-abort
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::PrefixAbort);
            assert!(commands.try_recv().is_err(), "abort dispatches nothing");

            press(&handle, "q"); // unbound everywhere in the vi chain
            assert_eq!(keyboard.get_last_feedback(), KeyFeedback::None);
            assert!(commands.try_recv().is_err());

            // The abort left the resolver idle: `dd` completes after.
            let task = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            handle.set_focused_entities(Some(task), None);
            press(&handle, "d");
            press(&handle, "d");
            let command = commands.try_recv().expect("the chord completed");
            assert_eq!(command, StateCommand::DeleteTask { id: task });
        });
    }

    /// D5: the vi insert intent opens the view's create row (no void
    /// insert — without a create target `i` does nothing); Escape (the
    /// only insert binding) ends the flow and returns to normal mode.
    #[test]
    fn insert_intent_opens_the_create_row_and_escape_returns() {
        on_slint_thread(|| {
            let (handle, mut commands) = handover_vi();
            let keyboard = handle.window.global::<KeyboardState>();
            assert!(!keyboard.get_vi_insert());
            // No board yet: the insert intent is inert (no void mode).
            press(&handle, "i");
            assert!(!keyboard.get_vi_insert(), "no create target, no insert");

            // With a board and stack, `i` opens the create row in
            // insert mode; Escape (the only insert binding) ends it.
            let board = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
            let stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = taskboard_domain::AppState::default();
            state.boards.insert(
                board,
                taskboard_domain::Board {
                    id: board,
                    remote: None,
                    title: "b".to_owned(),
                    color: taskboard_domain::Color::new(""),
                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::BoardClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        color: taskboard_domain::BoardClocks::epoch(),
                        archived: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            );
            state.stacks.insert(
                stack,
                taskboard_domain::Stack {
                    id: stack,
                    remote: None,
                    board,
                    title: "s".to_owned(),
                    order: 0,
                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::StackClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        order: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            );
            handle.state.store(state.into());
            handle.refresh(); // the board selection seeds at reconcile
            press(&handle, "i");
            assert!(
                keyboard.get_vi_insert(),
                "the create row opened in insert mode"
            );
            assert!(commands.try_recv().is_err(), "opening dispatches nothing");
            press(&handle, "\u{1b}");
            assert!(!keyboard.get_vi_insert(), "mode.normal returned");
        });
    }
}

#[cfg(test)]
mod view_component_tests {
    use super::*;
    use crate::actions::ContextId;
    use crate::test_platform::on_slint_thread;
    use chrono::TimeDelta;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use slint::Model as _;

    type Handover = (UiHandle, mpsc::Receiver<StateCommand>);

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    fn task_clocks() -> taskboard_domain::TaskClocks {
        taskboard_domain::TaskClocks {
            title: epoch(),
            description: epoch(),
            duedate: epoch(),
            scheduled: epoch(),
            done: epoch(),
            position: epoch(),
            labels: epoch(),
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            archived: epoch(),
            deleted: epoch(),
        }
    }

    fn task(
        id: taskboard_domain::TaskId,
        stack: taskboard_domain::StackId,
        order: i64,
    ) -> taskboard_domain::Task {
        taskboard_domain::Task {
            id,
            remote: None,
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            labels: BTreeSet::new(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted: false,
            clocks: task_clocks(),
            remote_seen: None,
        }
    }

    fn stack(
        id: taskboard_domain::StackId,
        board: taskboard_domain::BoardId,
        order: i64,
    ) -> taskboard_domain::Stack {
        taskboard_domain::Stack {
            id,
            remote: None,
            board,
            title: format!("stack-{order}"),
            order,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    fn board(id: taskboard_domain::BoardId) -> taskboard_domain::Board {
        taskboard_domain::Board {
            id,
            remote: None,
            title: "board".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: taskboard_domain::BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            remote_seen: None,
        }
    }

    /// Marks the fixture's sync offline with `pending` derived pending
    /// ops (the projection is derived; the ops are fabrication-only).
    fn sync_offline_pending(state: &mut taskboard_domain::AppState, pending: u32) {
        use taskboard_domain::{LocalOp, OpId, PendingOp};
        let board = *state.boards.keys().next().expect("fixture board");
        let task = *state.tasks.keys().next().expect("fixture task");
        state
            .sync
            .set_stored(board, taskboard_domain::SyncPhase::Offline, None);
        let ops: Vec<PendingOp> = (0..pending)
            .map(|i| PendingOp {
                op_id: OpId(uuid::Uuid::from_u128(u128::from(0x9000 + i))),
                op: LocalOp::UpdateTask(task),
                queued_at: chrono::Utc::now(),
            })
            .collect();
        taskboard_domain::derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &ops,
            &mut state.sync,
        );
    }

    /// Two stacks (a: tasks 0,1; b: task 2), task 1 overdue, sync idle.
    fn fixture_state() -> taskboard_domain::AppState {
        let board_id = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
        let s0 = taskboard_domain::StackId::from_uuid(uuid::Uuid::now_v7());
        let s1 = taskboard_domain::StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = taskboard_domain::AppState::default();
        state.boards.insert(board_id, board(board_id));
        state.stacks.insert(s0, stack(s0, board_id, 0));
        state.stacks.insert(s1, stack(s1, board_id, 1));
        let t0 = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
        let t1 = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
        let t2 = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut overdue = task(t1, s0, 1);
        overdue.duedate = Some(chrono::Utc::now() - TimeDelta::hours(2));
        state.tasks.insert(t0, task(t0, s0, 0));
        state.tasks.insert(t1, overdue);
        state.tasks.insert(t2, task(t2, s1, 0));
        state
    }

    fn handover_with(state: taskboard_domain::AppState, keymap: keymap::Keymap) -> Handover {
        let (commands_tx, commands_rx) = mpsc::channel(16);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        drop(system_tx);
        drop(signals_tx);
        let handle = mount(
            Arc::new(ArcSwap::from_pointee(state)),
            signals_rx,
            system_rx,
            commands_tx,
            UiSettings {
                keymap,
                ..UiSettings::default()
            },
        )
        .expect("mount succeeds on the null platform");
        (handle, commands_rx)
    }

    fn press(handle: &UiHandle, text: &str) {
        handle
            .window
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
    }

    fn replace_state(handle: &UiHandle, state: taskboard_domain::AppState) {
        handle.state.store(Arc::new(state));
        handle.refresh();
    }

    fn vi_keymap() -> keymap::Keymap {
        keymap::Keymap::from_preset(keymap::Preset::Vi)
    }

    /// §6 test 1 — refresh from a fake `AppState`: column count,
    /// per-column card counts, banner row count, sync badge phase —
    /// structural facts, no engine involved.
    #[test]
    fn refresh_pushes_the_board_structure_from_a_fake_state() {
        on_slint_thread(|| {
            let mut state = fixture_state();
            sync_offline_pending(&mut state, 2);
            let (handle, _commands) = handover_with(state, UiSettings::default().keymap);

            let board = handle.window.get_board_model();
            assert_eq!(board.stacks.row_count(), 2, "one column per stack");
            let column0 = board.stacks.row_data(0).expect("column");
            assert_eq!(column0.tasks.row_count(), 2, "canonical tasks");
            let column1 = board.stacks.row_data(1).expect("column");
            assert_eq!(column1.tasks.row_count(), 1);

            assert_eq!(
                handle.window.get_banner_model().row_count(),
                1,
                "one overdue banner row"
            );
            assert_eq!(handle.window.get_sync_model().phase, SyncPhaseUi::Offline);
            assert_eq!(handle.window.get_sync_model().pending, 2);
            assert_eq!(
                handle.window.get_current_view(),
                View::Board,
                "the cycle starts on the board"
            );
        });
    }

    /// Two boards with deterministic ids (board one < board two, so
    /// board one seeds): "board one" carries the classic two-stack
    /// shape, "board two" one stack with one task; stored sync phases
    /// differ per board so the badge read is attributable.
    fn two_board_fixture_state() -> taskboard_domain::AppState {
        use taskboard_domain::{StackId, SyncPhase};
        let b1 = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
        let b2 = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
        let sa = StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
        let sb = StackId::from_uuid(uuid::Uuid::from_u128(0x2002));
        let sc = StackId::from_uuid(uuid::Uuid::from_u128(0x2003));
        let mut state = taskboard_domain::AppState::default();
        for (id, title) in [(b1, "board one"), (b2, "board two")] {
            let mut board = board(id);
            board.title = title.to_owned();
            state.boards.insert(id, board);
        }
        state.stacks.insert(sa, stack(sa, b1, 0));
        state.stacks.insert(sb, stack(sb, b1, 1));
        state.stacks.insert(sc, stack(sc, b2, 0));
        for (order, st) in [(0, sa), (1, sa), (2, sb), (0, sc)] {
            let id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
            state.tasks.insert(id, task(id, st, order));
        }
        state.sync.set_stored(b1, SyncPhase::Syncing, None);
        state.sync.set_stored(b2, SyncPhase::Offline, None);
        taskboard_domain::derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &[],
            &mut state.sync,
        );
        state
    }

    /// The chunk-7 board dimension (design §3.8), T10: the push reads
    /// the *selected* board — the seeded first board's title/columns/
    /// badge row, and after a `board.next` press board two's. The bar
    /// model carries one segment per live board with the selected
    /// index; switching dispatches nothing (the dumb-client pin).
    #[test]
    fn the_push_reads_the_selected_board() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(two_board_fixture_state(), UiSettings::default().keymap);
            assert_eq!(handle.window.get_toolbar_title(), "board one", "the seed");
            assert_eq!(handle.window.get_board_model().stacks.row_count(), 2);
            let bar = handle.window.get_board_bar_model();
            assert_eq!(bar.segments.row_count(), 2, "one segment per live board");
            assert_eq!(bar.selected, 0);
            assert_eq!(
                handle.window.get_sync_model().phase,
                SyncPhaseUi::Syncing,
                "board one's row"
            );

            press(&handle, "b"); // board.next (the standard Board binding)
            assert_eq!(handle.window.get_toolbar_title(), "board two");
            assert_eq!(
                handle.window.get_board_model().stacks.row_count(),
                1,
                "board two's columns re-rendered"
            );
            assert_eq!(handle.window.get_board_bar_model().selected, 1);
            assert_eq!(
                handle.window.get_sync_model().phase,
                SyncPhaseUi::Offline,
                "the badge follows the selected board"
            );
            assert!(commands.try_recv().is_err(), "switching dispatches nothing");
        });
    }

    /// T11 — the board-bar click seam: a segment click switches the
    /// view state through `board.select` and the command channel stays
    /// empty (the dumb-client boundary); a stale index against an
    /// older tables pass is absorbed silently.
    #[test]
    fn the_board_bar_click_seam_switches_without_commands() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(two_board_fixture_state(), UiSettings::default().keymap);
            handle.window.invoke_board_bar_segment_activated(1);
            assert_eq!(handle.window.get_toolbar_title(), "board two");
            assert_eq!(handle.window.get_board_bar_model().selected, 1);
            assert_eq!(handle.window.get_board_model().stacks.row_count(), 1);
            assert!(commands.try_recv().is_err(), "the click dispatched nothing");

            handle.window.invoke_board_bar_segment_activated(9);
            assert_eq!(
                handle.window.get_toolbar_title(),
                "board two",
                "a stale index is absorbed"
            );
            assert!(commands.try_recv().is_err());

            handle.window.invoke_board_bar_segment_activated(0);
            assert_eq!(handle.window.get_toolbar_title(), "board one");
            assert_eq!(handle.window.get_board_bar_model().selected, 0);
        });
    }

    /// §6 test 2 — breakpoint demotion: the width push through the
    /// generated callback flips the read-back `compact-active` fact;
    /// the board model itself is breakpoint-independent (the degeneration
    /// is a rendering concern, D8).
    #[test]
    fn width_push_flips_the_compact_rendering_fact() {
        on_slint_thread(|| {
            let (handle, _commands) = handover_with(fixture_state(), UiSettings::default().keymap);
            assert!(!handle.window.get_compact_active(), "default is regular");

            handle.window.invoke_window_width_changed(600.0);
            assert!(handle.window.get_compact_active(), "compact below 720");
            handle.window.invoke_window_width_changed(1500.0);
            assert!(!handle.window.get_compact_active(), "expansive above 1440");

            let stacks = handle.window.get_board_model().stacks;
            assert_eq!(stacks.row_count(), 2, "the model stays breakpoint-blind");
        });
    }

    /// T13 — the leaving rule for the board dimension (chunk-7 plan
    /// D5): a bar click mid-edit-flow commits the buffer first, against
    /// the board being left; then the view switches. Typed command +
    /// title read; no text assertions.
    #[test]
    fn a_board_switch_mid_flow_commits_against_the_board_being_left() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(two_board_fixture_state(), UiSettings::default().keymap);
            press(&handle, "S"); // stack.create (the standard Board binding)
            assert!(matches!(
                handle.adapter.borrow().view_state.flow(),
                crate::view_state::Flow::CreateStack
            ));
            // The Rust buffer mirror (no rendered field seeds it here).
            handle
                .adapter
                .borrow_mut()
                .view_state
                .set_buffer("x".to_owned());
            handle.window.invoke_board_bar_segment_activated(1);

            let board_one = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
            assert_eq!(
                commands
                    .try_recv()
                    .expect("the commit reached the boundary"),
                StateCommand::CreateStack {
                    board: board_one,
                    title: "x".into(),
                    order: 2, // append after board one's two stacks
                },
                "the flow committed against the board being left"
            );
            assert_eq!(
                *handle.adapter.borrow().view_state.flow(),
                crate::view_state::Flow::None,
                "the flow closed"
            );
            assert_eq!(
                handle.window.get_toolbar_title(),
                "board two",
                "the view switched after the commit"
            );
            assert!(commands.try_recv().is_err(), "nothing else dispatched");
        });
    }

    /// §6 test 3 — editing safety (standard preset): `N` opens the
    /// create row; typing dispatches nothing; Escape ends the flow;
    /// Ctrl+Q stays live mid-edit (the quit counter seam); a refresh
    /// mid-edit never closes the user's flow.
    #[test]
    fn editing_dispatches_nothing_and_exits_via_escape() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(fixture_state(), UiSettings::default().keymap);
            press(&handle, "N"); // task.create (standard Board — the keymap spells the shifted key)
            assert!(
                handle
                    .window
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "the create row opened in the first stack"
            );
            assert!(commands.try_recv().is_err(), "opening dispatches nothing");

            press(&handle, "h"); // a Board printable — must reach the field
            assert!(commands.try_recv().is_err(), "typing dispatches nothing");

            // A signal arrives mid-edit: the refresh must not close the
            // flow (D9 rule 4).
            handle.refresh();
            assert!(
                handle
                    .window
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "mid-edit refresh keeps the flow open"
            );

            press(&handle, "\u{1b}"); // Escape → mode.normal → discard
            assert!(
                !handle
                    .window
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "Escape ended the flow"
            );
            assert!(commands.try_recv().is_err(), "cancel dispatches nothing");
        });
    }

    /// §6 test 3b — the action layer stays live mid-edit: the menu
    /// opens over the flow and invoking `app.exit` fires the quit
    /// effect (counter seam, not the platform's quit). Chords cannot
    /// be injected modifier-carrying on this backend; the menu is the
    /// §7.1 action-equivalent path for the same claim.
    #[test]
    fn menu_path_stays_live_mid_edit() {
        on_slint_thread(|| {
            let (handle, _commands) = handover_with(fixture_state(), UiSettings::default().keymap);
            press(&handle, "N"); // task.create: the create flow holds insert
            handle.window.invoke_toolbar_menu_clicked();
            assert!(
                handle.window.global::<KeyboardState>().get_menu_open(),
                "the menu opens over the edit flow"
            );
            // Invoke the first application row (app.exit) by index.
            let entries = handle.window.global::<KeyboardState>().get_menu_entries();
            let exit_index = (0..entries.row_count())
                .find(|i| entries.row_data(*i).expect("row").action == "app.exit")
                .expect("app.exit is a menu row");
            handle
                .window
                .invoke_menu_entry_invoked(i32::try_from(exit_index).unwrap_or(i32::MAX));
            assert!(
                handle.adapter.borrow().quit_requests > 0,
                "the exit effect fired over the edit flow"
            );
        });
    }

    /// §6 test 4 — banner end-to-end: activating a row then snooze/mute
    /// sends the exact typed commands (rule-less task → the implicit
    /// default with the duedate anchor).
    #[test]
    fn banner_actions_send_exact_commands() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(fixture_state(), UiSettings::default().keymap);
            let overdue_id = {
                let state = handle.state.load();
                taskboard_domain::overdue_tasks(&state.tasks, chrono::Utc::now())
                    .into_iter()
                    .map(|t| t.id)
                    .next()
                    .expect("fixture has one overdue task")
            };

            handle.window.invoke_banner_row_activated(0);
            handle.window.invoke_banner_snooze_clicked(0);
            let command = commands.try_recv().expect("the snooze command arrived");
            match command {
                StateCommand::SnoozeReminder {
                    reminder,
                    occurrence,
                    postpone,
                } => {
                    assert_eq!(reminder, taskboard_domain::ReminderRef::Default(overdue_id));
                    // The anchor is the refresh pass's clock read; a
                    // one-second window is the honest tolerance.
                    let expected = chrono::Utc::now() - TimeDelta::hours(2);
                    assert!((occurrence - expected).abs() < TimeDelta::seconds(1));
                    assert_eq!(postpone, TimeDelta::hours(1));
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }

            handle.window.invoke_banner_row_activated(0);
            handle.window.invoke_banner_mute_clicked(0);
            assert_eq!(
                commands.try_recv().expect("the mute command arrived"),
                StateCommand::SetTaskRemindersMuted {
                    task: overdue_id,
                    muted: true
                }
            );
            assert!(commands.try_recv().is_err(), "nothing else dispatched");
        });
    }

    /// §6 test 5 — the move flow (standard preset): begin with `M`,
    /// arrows retarget (read-back via `move-target`), Return confirms
    /// with an append `MoveTask`, and a re-request cancels without
    /// dispatching.
    #[test]
    fn move_flow_retargets_confirms_and_cancels() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(fixture_state(), UiSettings::default().keymap);
            let task = {
                let state = handle.state.load();
                state
                    .tasks_in_stack(state.stacks.keys().copied().next().expect("stack"))
                    .first()
                    .map(|t| t.id)
            };
            handle.set_focused_entities(task, None);

            press(&handle, "M"); // task.move
            assert!(
                handle.window.get_flow_hint_model().visible,
                "the flow hint shows while moving"
            );
            press(&handle, "\u{F701}"); // Down (nav.next-item) — retarget
            assert_eq!(
                handle.window.get_board_model().move_target,
                1,
                "the second stack highlights as the target"
            );

            press(&handle, "\u{a}"); // Return — confirm
            let command = commands.try_recv().expect("the move command arrived");
            match command {
                StateCommand::MoveTask { id, stack, order } => {
                    assert_eq!(id, task.expect("focused task"));
                    let state = handle.state.load();
                    let board = state.boards.values().next().expect("board").id;
                    let ordered: Vec<_> = crate::presentation::stacks_in_board(&state, board);
                    assert_eq!(
                        stack, ordered[1].id,
                        "append to the retargeted stack (display order)"
                    );
                    assert_eq!(order, 1, "append after the target stack's task");
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }

            // Re-request is the cancel (toggle): the flow hint appears
            // with the new flow and disappears on the second press —
            // and nothing reaches the mpsc.
            press(&handle, "M");
            assert!(
                handle.window.get_flow_hint_model().visible,
                "a fresh move flow opens after the confirm"
            );
            press(&handle, "M");
            assert!(
                !handle.window.get_flow_hint_model().visible,
                "the re-request cancelled the flow"
            );
            press(&handle, "\u{e015}"); // Down now walks focus, not a target
            assert!(commands.try_recv().is_err(), "cancel dispatches nothing");
        });
    }

    /// §6 test 6 — the view cycle (vi preset: `g t` advances) and the
    /// reconciliation pop: a deleted focused task pops `TaskDetail` →
    /// Board on refresh.
    #[test]
    fn view_cycle_advances_and_reconcile_pops_deleted_subjects() {
        on_slint_thread(|| {
            let initial = fixture_state();
            // The board selection seeds at reconcile; the reading order
            // is the seeded view state's.
            let mut seeded = crate::view_state::ViewState::new();
            seeded.reconcile(&initial);
            let victim = seeded
                .reading_order(&initial)
                .first()
                .copied()
                .expect("fixture has tasks");
            let (handle, _commands) = handover_with(initial.clone(), vi_keymap());
            assert_eq!(handle.window.get_current_view(), View::Board);

            press(&handle, "g");
            press(&handle, "t");
            assert_eq!(handle.window.get_current_view(), View::StackDetail);
            press(&handle, "g");
            press(&handle, "t");
            assert_eq!(handle.window.get_current_view(), View::TaskDetail);
            press(&handle, "g");
            press(&handle, "t");
            assert_eq!(
                handle.window.get_current_view(),
                View::Board,
                "the cycle wraps"
            );

            // A deleted TaskDetail subject pops to Board on refresh.
            handle.set_focused_entities(Some(victim), None);
            press(&handle, "g");
            press(&handle, "t");
            press(&handle, "g");
            press(&handle, "t");
            assert_eq!(handle.window.get_current_view(), View::TaskDetail);
            let mut state = initial;
            state.tasks.get_mut(&victim).expect("task").deleted = true;
            replace_state(&handle, state);
            assert_eq!(
                handle.window.get_current_view(),
                View::Board,
                "a deleted subject pops the view"
            );
        });
    }

    /// §6 test 3c — the commit seams' post-flow state (the §7.3
    /// journey caught both): committing the create row through the
    /// field's seam (a) pushes normal mode onto the window global
    /// (`apply_outcomes` re-syncs the shell), and (b) hands keyboard
    /// focus back so the next stroke reaches the root scope.
    #[test]
    fn committing_the_create_row_restores_mode_and_keyboard_delivery() {
        on_slint_thread(|| {
            let (handle, mut commands) =
                handover_with(fixture_state(), UiSettings::default().keymap);
            press(&handle, "N"); // task.create: the row opens, insert mode
            assert!(
                handle.window.global::<KeyboardState>().get_vi_insert(),
                "the flow holds insert mode"
            );
            handle.window.invoke_board_create_committed(0, "x".into());
            assert!(
                !handle.window.global::<KeyboardState>().get_vi_insert(),
                "the commit seam pushes normal mode (apply_outcomes sync)"
            );
            let state = handle.state.load();
            let mut seeded = crate::view_state::ViewState::new();
            seeded.reconcile(&state);
            let expected_stack = seeded.stack_ids(&state)[0];
            assert_eq!(
                commands.try_recv().expect("the create command arrived"),
                StateCommand::CreateTask {
                    title: "x".into(),
                    stack: expected_stack,
                    order: 2,
                }
            );

            // The field that held slint focus unmounted; the next
            // stroke must still reach the resolver (the refocus
            // hand-off): the walk seeds the first card's emphasis.
            press(&handle, "\u{F701}"); // Down (nav.next-item)
            let board = handle.window.get_board_model();
            let emphasized = (0..board.stacks.row_count()).find_map(|column| {
                let tasks = board.stacks.row_data(column).expect("column");
                (0..tasks.tasks.row_count())
                    .find(|row| tasks.tasks.row_data(*row).expect("card").emphasis)
                    .map(|row| (column, row))
            });
            assert_eq!(
                emphasized,
                Some((0, 0)),
                "the walk delivers after the flow's field unmounted"
            );
        });
    }

    /// §6 test 7 — the Board chain's generated menu offers the enabled
    /// task verbs and the reminder actions (row-count + enabled bits;
    /// the snapshot covers the dump).
    #[test]
    fn board_menu_offers_the_enabled_verbs() {
        use crate::actions::ActionId;
        use crate::menu;
        let keymap = UiSettings::default().keymap;
        let rows = menu::entries(
            &keymap,
            &[ContextId::Board, ContextId::ViNormal, ContextId::Global],
        );
        let enabled = |id: ActionId| rows.iter().any(|r| r.action == id && r.enabled);
        assert!(enabled(ActionId::TaskComplete));
        assert!(enabled(ActionId::TaskCreate));
        assert!(enabled(ActionId::TaskMove));
        assert!(enabled(ActionId::ReminderSnooze));
        assert!(enabled(ActionId::ReminderMute));
        assert!(
            rows.iter().all(|r| r.action != ActionId::MenuNext),
            "menu cursor rows never appear in a view menu"
        );
    }
}
