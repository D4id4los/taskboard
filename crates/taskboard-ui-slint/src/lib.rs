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

pub use scale::{UiScale, UiScaleError};

pub mod actions;
pub mod adapter;
pub mod board_presets;
pub mod due_edit;
pub mod fonts;
pub mod keymap;
pub mod menu;
pub mod models;
pub mod presentation;
pub mod requests;
pub mod resolver;
pub mod responsive;
pub mod router;
pub mod scale;
pub mod settings_form;
pub mod settings_status;
pub mod shell;
pub mod tooltip;
pub mod view_state;
pub mod widths;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::Arc;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use tokio::sync::{broadcast, mpsc};

use taskboard_domain::{AppState, EngineSignal, StateCommand, SystemEvent};

#[cfg(test)]
mod flow_table_tests;
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
    /// (§6.1), minted app-side ([`UiScale`]) — validated by
    /// construction.
    pub scale: UiScale,
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
    /// The device-local stack column widths (round-1 D16/D20): board
    /// uuid → stack uuid → width in logical px. Seeded into the fresh
    /// `ViewState` at mount, before the first refresh — widths are
    /// applied before the first board render. Kept uuid-keyed
    /// (serde-native shape; the domain newtypes deliberately carry no
    /// serde impls); `adapter::install` converts once.
    pub stack_widths: BTreeMap<uuid::Uuid, BTreeMap<uuid::Uuid, u32>>,
    /// The configured connectors, secret-free (round-1 D2): resolved
    /// app-side via `configured_connectors` at hand-over, so the
    /// settings list is populated before the first open.
    pub connectors: Vec<requests::ConnectorInfo>,
    /// The today window's derivation pair (chunk 10 D4): the parsed
    /// `[preferences] day_start_time` plus the device's fixed offset,
    /// resolved once at hand-over (the consumers contract — nothing
    /// reads config mid-flight). Defaults to midnight + UTC.
    pub day_start: (chrono::NaiveTime, chrono::FixedOffset),
    /// The live local boards for the settings board selector (r2
    /// D32): secret-free rows with the binding name, resolved
    /// app-side at hand-over; refreshed only from reply snapshots.
    pub boards: Vec<requests::BoardInfo>,
    /// The `[ui.keymap] preset` word's typed value (round-1 D9): the
    /// settings picker's initial selection.
    pub keymap_preset: keymap::Preset,
    /// The injected reminder defaults (R3 chunk C): the derivation
    /// input the in-app notification surface's `active_notifications`
    /// read needs for the implicit default reminder's occurrence —
    /// resolved app-side at hand-over, held, never re-read (the
    /// `day_start` consumers contract).
    pub reminder_defaults: taskboard_domain::ReminderDefaults,
    /// The due-soon horizon (round-3 D14/#230): `[ui] due_soon_days`
    /// converted to a typed delta at hand-over — the consumers
    /// contract, nothing reads config mid-flight. Defaults to the
    /// presentation constant (the pre-config behaviour).
    pub due_soon_horizon: chrono::TimeDelta,
    /// The kiosk display-mode rotation (K13, issue #363): minted
    /// app-side from the validated `[kiosk.rotation]` table — the
    /// hand-over carries typed values only. `None` is no rotation:
    /// the desktop path is structurally inert (no state, no timer).
    pub kiosk_rotation: Option<view_state::KioskRotation>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            scale: UiScale::default(),
            keymap: keymap::Keymap::from_preset(keymap::Preset::Standard),
            startup: adapter::StartupWarnings::default(),
            kiosk: false,
            kiosk_rotation: None,
            stack_widths: BTreeMap::new(),
            connectors: Vec::new(),
            boards: Vec::new(),
            keymap_preset: keymap::Preset::Standard,
            day_start: (
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).expect("zero"),
            ),
            reminder_defaults: taskboard_domain::ReminderDefaults::default(),
            due_soon_horizon: presentation::DUE_SOON_HORIZON,
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
    /// The settings status line's auto-clear driver (r2 D28). Never
    /// read here: the adapter arms and the closure fires; this field
    /// owns the timer's lifetime alongside the window.
    #[allow(dead_code)]
    status_timer: std::rc::Rc<slint::Timer>,
    /// The kiosk rotation flip's dwell timer (K13). Armed by
    /// `install` only when the hand-over carries a rotation; this
    /// field owns the timer's lifetime alongside the window.
    #[allow(dead_code)]
    rotation_timer: std::rc::Rc<slint::Timer>,
    /// The alarm-window lanes (DR-3/4, D-I9.2): the lazily mounted
    /// second window, its last row set (the attention edge's previous
    /// state), and the derivation defaults. `None` under the kiosk
    /// profile — the appliance gets none of it (the R3.5.3 fence).
    alarm: Option<std::rc::Rc<AlarmLanes>>,
}

/// The alarm window's mount-time state (see [`UiHandle::alarm`]).
struct AlarmLanes {
    /// The lazily built window (hidden until the pass opens it) — the
    /// pass owns the lifetime; an idle mount grows no window at all.
    window: std::cell::RefCell<Option<AlarmWindow>>,
    /// The command sender the window's buttons dispatch through.
    commands: mpsc::Sender<StateCommand>,
    /// The last pass's rows (the gained-edge's previous state) — shared
    /// with the button callbacks (the index→row map) and the pass.
    rows: std::rc::Rc<std::cell::RefCell<Vec<presentation::AlarmRow>>>,

    /// The derivation defaults injected at hand-over (the same
    /// consumers contract as the notification surface's).
    defaults: taskboard_domain::ReminderDefaults,
}

// -- test seams (the lib flow/buffer harnesses; never shipped) ------

#[cfg(test)]
impl UiHandle {
    /// The flow transition-table harness reaches the adapter to arm
    /// flows and drive the interpretation table directly.
    pub(crate) fn test_adapter(&self) -> Rc<RefCell<adapter::Adapter>> {
        Rc::clone(&self.adapter)
    }

    /// The live state snapshot for the harness's re-arm passes.
    pub(crate) fn test_state(&self) -> Arc<ArcSwap<AppState>> {
        Arc::clone(&self.state)
    }
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
    requests: mpsc::Sender<requests::UiRequest>,
    settings: UiSettings,
    delivery_health: tokio::sync::watch::Receiver<crate::presentation::DeliveryHealth>,
) -> Result<UiHandle, UiMountError> {
    fonts::register_bundled_fonts();
    let window = AppWindow::new().map_err(UiMountError::WindowCreation)?;
    // The §6.1 root multiplier, one line per window construction — a
    // future second window re-runs it; no global-once init exists.
    window
        .global::<Theme>()
        .set_base_scale(settings.scale.get_f32());
    // The kiosk window profile (Q5): fullscreen on the shared desktop
    // window. Slint 1.18.1 has no independent decoration control; on
    // the winit backend fullscreen suppresses decorations by
    // construction, so fullscreen alone is the complete profile.
    if settings.kiosk {
        window.window().set_fullscreen(true);
    }
    // The keymap is applied one-way at UI start (§5.6): install the
    // root-scope interception over the validated keymap.
    // The status line's auto-clear driver (r2 D28): created at mount
    // beside the tooltip delay (the same lifetime pattern), handed to
    // the adapter — the choke point arms it, the mock clock fires it.
    // The alarm window (DR-3/4, D-I9.2): mounted lazily iff NOT kiosk —
    // the appliance's banner-only fence (R3.5.3); daemon mode mounts no
    // UI at all (this function never runs there). It rides the same
    // command sender the adapter holds, cloned before `install` moves
    // its clone.
    let alarm = if settings.kiosk {
        None
    } else {
        let commands = commands.clone();
        Some(std::rc::Rc::new(install_alarm_lanes(
            commands,
            settings.reminder_defaults,
        )))
    };
    let status_timer = std::rc::Rc::new(slint::Timer::default());
    // The kiosk rotation's dwell timer (K13): the same lifetime
    // pattern — created unconditionally, armed by `install` only when
    // the hand-over carries a rotation.
    let rotation_timer = std::rc::Rc::new(slint::Timer::default());
    let adapter = adapter::install(
        &window,
        settings.keymap,
        settings.keymap_preset,
        settings.startup,
        commands,
        requests,
        settings.stack_widths,
        settings.connectors,
        settings.day_start,
        settings.due_soon_horizon,
        settings.kiosk,
        settings.kiosk_rotation,
        settings.reminder_defaults,
        settings.boards,
        Arc::clone(&state),
        Rc::clone(&status_timer),
        Rc::clone(&rotation_timer),
        delivery_health,
    );
    let tooltip_timer = install_tooltip_delay(&window);
    let handle = UiHandle {
        window,
        adapter,
        state,
        timer: slint::Timer::default(),
        tooltip_timer,
        status_timer,
        rotation_timer,
        alarm,
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
    // The measured tip size (the size hand-off, round-1 D10): shared
    // by the observation callback and the delay fire, so placement
    // lands whenever both facts are present, whichever comes last.
    let tip_size: std::rc::Rc<std::cell::Cell<Option<(f32, f32)>>> =
        std::rc::Rc::new(std::cell::Cell::new(None));
    {
        let weak = weak.clone();
        let tip_size = std::rc::Rc::clone(&tip_size);
        tooltip.on_size_observed(move |w, h| {
            tip_size.set(Some((w, h)));
            if let Some(window) = weak.upgrade() {
                place_tooltip(&window, &tip_size);
            }
        });
    }
    let fire_weak = weak.clone();
    let fire_tip = std::rc::Rc::clone(&tip_size);
    tooltip.on_delay_requested(move || {
        let fire_weak = fire_weak.clone();
        let fire_tip = std::rc::Rc::clone(&fire_tip);
        arm.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(TOOLTIP_DELAY_MS),
            move || {
                if let Some(window) = fire_weak.upgrade() {
                    let tooltip = window.global::<TooltipState>();
                    tooltip.set_visible(true);
                    tooltip.set_pending(false);
                    place_tooltip(&window, &fire_tip);
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

/// The placement hand-off (round-1 D10): when the tooltip is visible
/// and its size is known, compute the top-left with the pure
/// [`tooltip::place`] math and mark it placed. Either input may
/// arrive last — both call sites run this.
fn place_tooltip(window: &AppWindow, tip_size: &std::cell::Cell<Option<(f32, f32)>>) {
    let Some((tip_w, tip_h)) = tip_size.get() else {
        return;
    };
    let tooltip = window.global::<TooltipState>();
    if !tooltip.get_visible() {
        return;
    }
    let scale = window.global::<Theme>().get_base_scale();
    let input = tooltip::TooltipInput {
        // Slint lengths cross the boundary as plain f32 logical px.
        anchor: (
            tooltip.get_anchor_x(),
            tooltip.get_anchor_y(),
            tooltip.get_anchor_width(),
            tooltip.get_anchor_height(),
        ),
        tip: (tip_w, tip_h),
        window: (window.get_window_w(), window.get_window_h()),
        status_top: window.get_status_top(),
        margin: 8.0 * scale,
        gap: 4.0 * scale,
    };
    let (x, y) = tooltip::place(&input);
    tooltip.set_tip_x(x);
    tooltip.set_tip_y(y);
    tooltip.set_placed(true);
}

/// The tooltip delay (usability D3): a duration, not a layout
/// dimension.
pub const TOOLTIP_DELAY_MS: u64 = 600;

/// Wires the alarm lanes (the window itself is built lazily by the
/// first pass): the button callbacks dispatch the existing engine
/// commands onto the same sender the adapter holds (D-I8's reuse
/// ruling; no new commands, a mouse-only surface). A stray index
/// degrades to a no-op.
fn install_alarm_lanes(
    commands: mpsc::Sender<StateCommand>,
    defaults: taskboard_domain::ReminderDefaults,
) -> AlarmLanes {
    AlarmLanes {
        window: std::cell::RefCell::new(None),
        commands,
        rows: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        defaults,
    }
}

/// The attention channel (D-6): request — never focus. The types come
/// through slint's own `unstable-winit-030` re-export (no direct winit
/// dependency enters any manifest); a non-winit backend fails
/// `has_winit_window` and the channel no-ops structurally.
fn request_attention(window: &AlarmWindow) {
    use slint::winit_030::{WinitWindowAccessor, winit};
    let slint_window = window.window();
    if !slint_window.has_winit_window() {
        return;
    }
    slint_window.with_winit_window(|w| {
        w.request_user_attention(Some(winit::window::UserAttentionType::Critical));
    });
}

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
    let alarm = handle.alarm.clone();
    let state = Arc::clone(&handle.state);
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
            let mut events = Vec::new();
            loop {
                match system.try_recv() {
                    // `Shutdown` is inert here (the app-side exit
                    // watcher owns quit); the rest are refresh
                    // triggers — and the connector episodes feed the
                    // settings liveness map (round-1 D8).
                    Ok(event) => {
                        events.push(event);
                        dirty = true;
                    }
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
            if !events.is_empty() {
                adapter.borrow_mut().record_system_events(&events);
            }
            // The delivery-health drain (round-3 chunk 7, D31): the
            // same non-blocking poll — a changed snapshot pushes the
            // chip and counts as changed (the Lagged semantics' twin:
            // strictly better delivery only clears the surface, so a
            // missed tick can never strand a stale failure chip).
            if adapter.borrow_mut().drain_delivery_health(&window) {
                dirty = true;
            }
            // The settings reply drain (D25b): the non-blocking poll
            // under the same cadence — a settled reply refreshes.
            adapter.borrow_mut().drain_setting_replies(&window);
            let mut minute_boundary = false;
            let wall_minute = chrono::Utc::now().timestamp() / 60;
            if minute.get() != Some(wall_minute) {
                minute.set(Some(wall_minute));
                dirty = true;
                minute_boundary = true;
            }
            // The notice's expiry rides the same cadence tick's wall
            // clock (no UI-held timer); the push is cheap enough to
            // run unconditionally.
            adapter.borrow_mut().push_notice(&window);
            if dirty {
                if minute_boundary {
                    // The wall clock moved, the state pointer did not —
                    // the time-derived rows (badges, due-soon, day
                    // window) need the full derive regardless of the
                    // unchanged-refresh guard (#463).
                    adapter.borrow_mut().refresh_forced(&window);
                } else {
                    adapter.borrow_mut().refresh(&window);
                }
            }
            // The alarm pass (D-I9.2) rides the driver's two existing
            // wake branches: a state change (dirty) or the wall-minute
            // boundary (time passage for the elapsed phrases + the
            // level-triggered attention re-assert).
            if (dirty || minute_boundary)
                && let Some(lanes) = alarm.as_ref()
            {
                run_alarm_pass(lanes, &state, minute_boundary);
            }
        },
    );
}

/// The repaint driver's alarm half, detached from `UiHandle` so the
/// closure can hold cheap clones: derive + open/hide + attention (see
/// the mount tests, which drive it directly).
fn run_alarm_pass(
    lanes: &std::rc::Rc<AlarmLanes>,
    state: &Arc<ArcSwap<AppState>>,
    minute_boundary: bool,
) {
    let now = chrono::Utc::now();
    let rows = presentation::alarm_window_rows(&state.load(), &lanes.defaults, now);
    let gained = presentation::attention_due(&rows, &lanes.rows.borrow());
    if presentation::alarm_window_open(&rows) {
        // Lazy build (the mount grows no window until an alarm waits):
        // the pass runs on the slint thread, so construction is legal
        // here. A failed build degrades that channel to a no-op pass —
        // logged, never fatal.
        let built = match lanes.window.borrow_mut().take() {
            Some(window) => Some(window),
            None => match AlarmWindow::new() {
                Ok(window) => {
                    install_alarm_callbacks(&window, &lanes.commands, &lanes.rows);
                    Some(window)
                }
                Err(err) => {
                    tracing::warn!(%err, "alarm window: construction failed (visual-only)");
                    None
                }
            },
        };
        let Some(window) = built else {
            *lanes.rows.borrow_mut() = rows;
            return;
        };
        let model: Vec<AlarmRowModel> = rows
            .iter()
            .map(|row| AlarmRowModel {
                title: row.title.clone().into(),
                phrase: presentation::notification_phrase(row.occurrence, now).into(),
            })
            .collect();
        window.set_rows(slint::ModelRc::new(slint::VecModel::from(model)));
        if let Err(err) = window.show() {
            tracing::warn!(%err, "alarm window: show failed");
        }
        if gained || minute_boundary {
            request_attention(&window);
        }
        // Held again: the pass owns the lifetime.
        *lanes.window.borrow_mut() = Some(window);
    } else if let Some(window) = lanes.window.borrow_mut().take()
        && let Err(err) = window.hide()
    {
        tracing::warn!(%err, "alarm window: hide failed");
        // Dropped: a drained set tears the window down again.
    }
    *lanes.rows.borrow_mut() = rows;
}

/// The alarm window's button callbacks: index-keyed over the shared
/// rows cell, each dispatching exactly one engine command (D-I8). A
/// full inbox degrades to a logged drop, never a panic.
fn install_alarm_callbacks(
    window: &AlarmWindow,
    commands: &mpsc::Sender<StateCommand>,
    rows: &std::rc::Rc<std::cell::RefCell<Vec<presentation::AlarmRow>>>,
) {
    /// Resolves a button index to its row (a stray index degrades to a
    /// no-op), then dispatches one command; a full inbox logs the drop.
    fn on_row(
        rows: &std::rc::Rc<std::cell::RefCell<Vec<presentation::AlarmRow>>>,
        sender: &mpsc::Sender<StateCommand>,
        action: &'static str,
        command: impl Fn(&presentation::AlarmRow) -> StateCommand + 'static,
    ) -> impl Fn(i32) + 'static {
        let sink = std::rc::Rc::clone(rows);
        let sender = sender.clone();
        move |index| {
            let Ok(index) = usize::try_from(index) else {
                return;
            };
            let Some(row) = sink.borrow().get(index).cloned() else {
                return;
            };
            if let Err(err) = sender.try_send(command(&row)) {
                tracing::warn!(%err, "alarm window: {action} command dropped");
            }
        }
    }
    window.on_dismiss(on_row(rows, commands, "dismiss", |row| {
        StateCommand::DismissReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
        }
    }));
    window.on_snooze(on_row(rows, commands, "snooze", |row| {
        StateCommand::SnoozeReminder {
            reminder: row.reminder,
            occurrence: row.occurrence,
            postpone: presentation::ALARM_WINDOW_SNOOZE,
        }
    }));
    window.on_cancel(on_row(rows, commands, "cancel", |row| {
        StateCommand::CancelAdhocAlarm { alarm: row.alarm }
    }));
}

impl UiHandle {
    /// The D1 refresh: a deterministic, timer-free full re-derive
    /// pass over the current state snapshot — guarded (#463): an
    /// unchanged snapshot skips the derive+diff, the cheap wipe and
    /// settings tail still run. The repaint timer calls this; tests
    /// may call it directly.
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

    /// The settings reply drain (round-1 chunk 3, D25b): the same
    /// non-blocking poll the repaint timer runs every cadence tick.
    /// Public for the component journeys (the established
    /// [`UiHandle`]-seam pattern): the test plays the app half by
    /// answering a drained request's oneshot, then drives the drain.
    pub fn drain_setting_replies(&self) {
        if let Some(window) = self.window.as_weak().upgrade() {
            self.adapter.borrow_mut().drain_setting_replies(&window);
        }
    }

    /// One delivery-health drain (round-3 chunk 7, D31): the same
    /// non-blocking watch poll the repaint timer runs every cadence
    /// tick. Public for the component journeys (the established
    /// [`UiHandle`]-seam pattern): the test plays the app half by
    /// sending on the watch, then drives the drain.
    pub fn drain_delivery_health(&self) -> bool {
        if let Some(window) = self.window.as_weak().upgrade() {
            self.adapter.borrow_mut().drain_delivery_health(&window)
        } else {
            false
        }
    }

    /// One connector-liveness episode (round-1 chunk 3, D8): the same
    /// record-then-refresh path the system drain's pre-refresh hook
    /// runs. Public for the component journeys (broadcast injection
    /// needs a live engine; the journeys' harness has none).
    pub fn observe_connector_episode(&self, event: &taskboard_domain::SystemEvent) {
        if let Some(window) = self.window.as_weak().upgrade() {
            let mut adapter = self.adapter.borrow_mut();
            adapter.record_system_event(event);
            adapter.refresh(&window);
        }
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

    /// Routes one action through the adapter (the automation/test
    /// seam for the action layer without a keyboard event loop).
    pub fn dispatch_action(&self, action: crate::actions::ActionId) {
        self.adapter.borrow_mut().route(self.window(), action);
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

    /// One mount hand-over's channel-plus-state bundle (lib tests).
    type Handover = (
        Arc<ArcSwap<AppState>>,
        broadcast::Receiver<EngineSignal>,
        broadcast::Receiver<SystemEvent>,
        mpsc::Sender<StateCommand>,
        mpsc::Sender<requests::UiRequest>,
        mpsc::Receiver<requests::UiRequest>,
        tokio::sync::watch::Receiver<crate::presentation::DeliveryHealth>,
    );

    /// The health-free hand-over (tests that exercise the health lane
    /// bring their own watch channel; the sender half is the fixture).
    type Handover6 = (
        Arc<ArcSwap<AppState>>,
        broadcast::Receiver<EngineSignal>,
        broadcast::Receiver<SystemEvent>,
        mpsc::Sender<StateCommand>,
        mpsc::Sender<requests::UiRequest>,
        mpsc::Receiver<requests::UiRequest>,
    );

    /// The health-free hand-over: tests that exercise the health lane
    /// bring their own watch channel (the sender half is the fixture).
    pub(super) fn handover6() -> Handover6 {
        let (commands_tx, _commands_rx) = mpsc::channel(64);
        let (requests_tx, requests_rx) = mpsc::channel(16);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        drop(system_tx);
        drop(signals_tx);
        (
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            requests_rx,
        )
    }

    pub(super) fn handover() -> Handover {
        let (commands_tx, _commands_rx) = mpsc::channel(64);
        let (requests_tx, requests_rx) = mpsc::channel(16);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        // The delivery-health lane: a detached receiver (its sender
        // dropped) — the health tests bring their own channel.
        let (_health_tx, health_rx) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        drop(system_tx);
        drop(signals_tx);
        (
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            requests_rx,
            health_rx,
        )
    }

    /// The delivery-health lane (round-3 chunk 7, D31): a degraded
    /// snapshot pushed on the watch surfaces as the visible chip after
    /// one drain pass; a later success clears it. Typed edges only —
    /// the chip text is never asserted.
    #[test]
    fn the_repaint_driver_refreshes_on_health_change() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx) = handover6();
            let (health_tx, health_rx) =
                tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health_rx,
            )
            .expect("mount succeeds on the null platform");
            // The healthy default: the chip is absent after the mount
            // refresh.
            assert!(!handle.window.get_delivery_chip_visible());
            let now = chrono::Utc::now();
            let degraded = crate::presentation::DeliveryHealth {
                last_attempt: Some(now),
                last_failure: Some(now),
                ..crate::presentation::DeliveryHealth::default()
            };
            health_tx.send(degraded.clone()).expect("watch send");
            assert!(handle.drain_delivery_health());
            handle.refresh();
            assert!(handle.window.get_delivery_chip_visible());
            assert_ne!(handle.window.get_delivery_chip(), "");
            // Strictly better delivery clears the surface.
            let cleared = crate::presentation::DeliveryHealth {
                last_success: Some(now + chrono::TimeDelta::seconds(1)),
                ..degraded
            };
            health_tx.send(cleared).expect("watch send");
            assert!(handle.drain_delivery_health());
            handle.refresh();
            assert!(!handle.window.get_delivery_chip_visible());
        });
    }

    /// The dispatch acceptance line: a valid scale reaches
    /// `Theme.base-scale` via the generated `Global` setter.
    #[test]
    fn mount_pushes_the_configured_scale_into_the_theme() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings {
                    scale: UiScale::new(2.0).expect("valid"),
                    ..UiSettings::default()
                },
                health,
            )
            .expect("mount succeeds on the null platform");
            let base_scale = handle.window.global::<Theme>().get_base_scale();
            assert_eq!(base_scale, 2.0);
        });
    }

    #[test]
    fn mount_defaults_the_theme_scale_to_one() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
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
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
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

    /// A waiting-alarm state: one alarm-insistent adhoc alarm, fired
    /// and delivered one wall-minute before `now`.
    fn awaiting_alarm_state(state: &Arc<ArcSwap<AppState>>) -> taskboard_domain::AdhocAlarmId {
        use taskboard_domain::{
            AdhocAlarm, AdhocAlarmId, Insistence, ReminderDelivery, ReminderRef,
        };
        let id = AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let fire = chrono::Utc::now() - chrono::TimeDelta::minutes(1);
        let mut next = Arc::unwrap_or_clone(state.load_full());
        next.reminders.alarms.insert(
            id,
            AdhocAlarm {
                insistence: Insistence::Alarm,
                id,
                label: "tea".to_owned(),
                fire_at: fire,
                created_at: fire - chrono::TimeDelta::hours(1),
            },
        );
        next.reminders.deliveries.insert(ReminderDelivery {
            reminder: ReminderRef::Adhoc(id),
            occurrence: fire,
            delivered_at: fire,
        });
        state.store(Arc::new(next));
        id
    }

    /// DR-3/4 (D-I9.2): the desktop mount carries the alarm lanes; the
    /// kiosk profile gets none of it (the banner-only fence, R3.5.3).
    #[test]
    fn the_alarm_lanes_mount_iff_not_kiosk() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
            .expect("mount succeeds on the null platform");
            assert!(handle.alarm.is_some(), "the desktop mount has lanes");

            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
                health,
            )
            .expect("mount succeeds on the null platform");
            assert!(handle.alarm.is_none(), "the kiosk mount has no lanes");
        });
    }

    /// The pass over a non-empty awaiting set: the window opens with
    /// the row model pushed; over an empty set it closes. Both on the
    /// testing backend (the `ProbeWindow` precedent), structural only.
    #[test]
    fn the_alarm_pass_opens_and_pushes_rows_and_closes() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                Arc::clone(&state),
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
            .expect("mount succeeds on the null platform");
            let lanes = handle.alarm.as_ref().expect("desktop lanes").clone();

            // Empty state: the pass runs and no window even exists yet
            // (the lazy build grows nothing while idle).
            run_alarm_pass(&lanes, &state, false);
            assert!(lanes.window.borrow().is_none());

            // An awaiting alarm: the window builds, opens, and the row
            // model carries the row.
            awaiting_alarm_state(&state);
            run_alarm_pass(&lanes, &state, false);
            let (visible, rows) = {
                let held = lanes.window.borrow();
                let window = held.as_ref().expect("the window built");
                (
                    window.window().is_visible(),
                    slint::Model::row_count(&window.get_rows()),
                )
            };
            assert!(visible, "open iff the set is non-empty");
            assert_eq!(rows, 1);

            // A second pass over the same set: still open (idempotent).
            run_alarm_pass(&lanes, &state, false);
            assert!(
                lanes
                    .window
                    .borrow()
                    .as_ref()
                    .is_some_and(|w| w.window().is_visible()),
                "the pass keeps the window"
            );

            // Drained: closed and torn down again.
            state.store(Arc::new(AppState::default()));
            run_alarm_pass(&lanes, &state, false);
            assert!(
                lanes.window.borrow().is_none(),
                "a drained set tears the window down"
            );
        });
    }

    /// Theme init is a window-construction step (§3.1): mount produces
    /// a window instance that `run()` will show — still hidden here
    /// (the event loop never ran).
    #[test]
    fn mount_produces_a_hidden_window_instance() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
            .expect("mount succeeds on the null platform");
            assert!(!handle.window.window().is_visible());
        });
    }

    /// K13: the default hand-over carries no rotation and the mount
    /// shape is unchanged — the desktop is structurally inert (no
    /// rotation state, no armed timer; the timer's firing itself is
    /// exercised end-to-end by the journeys, whose mock clock is the
    /// lowest layer that owns slint-timer behaviour).
    #[test]
    fn mount_without_rotation_arms_nothing() {
        assert!(
            UiSettings::default().kiosk_rotation.is_none(),
            "the default hand-over is no rotation"
        );
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
            .expect("mount succeeds on the null platform");
            assert!(!handle.window.window().is_fullscreen());
        });
    }

    /// The kiosk window profile (Q5): mount applies fullscreen to the
    /// shared desktop window when the flag is handed over.
    #[test]
    fn mount_applies_the_kiosk_fullscreen_profile() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
                health,
            )
            .expect("mount succeeds on the null platform");
            assert!(handle.window.window().is_fullscreen());
        });
    }

    /// The load path (round-1 D16, review trap f): widths in
    /// `UiSettings` land in the view state at mount, before the first
    /// refresh — and seeding alone never emits a request (the lane
    /// carries *changes* only).
    #[test]
    fn mount_applies_seeded_widths_before_first_refresh() {
        on_slint_thread(|| {
            let board = taskboard_domain::BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
            let stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
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
            let (commands_tx, _commands_rx) = mpsc::channel(16);
            let (requests_tx, mut requests_rx) = mpsc::channel(16);
            let (_health_tx, health) =
                tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
            let handle = mount(
                Arc::new(ArcSwap::from_pointee(state)),
                {
                    let (_tx, rx) = broadcast::channel(4);
                    rx
                },
                {
                    let (_tx, rx) = broadcast::channel(4);
                    rx
                },
                commands_tx,
                requests_tx,
                UiSettings {
                    stack_widths: BTreeMap::from([(
                        uuid::Uuid::from_u128(0x1001),
                        BTreeMap::from([(uuid::Uuid::from_u128(0x2001), 240u32)]),
                    )]),
                    ..UiSettings::default()
                },
                health,
            )
            .expect("mount succeeds on the null platform");
            let seeded = handle.adapter.borrow().view_state.stack_width(board, stack);
            assert_eq!(
                seeded,
                Some(240),
                "the seeded width is live before any interaction"
            );
            assert!(
                requests_rx.try_recv().is_err(),
                "seeding alone never emits a persistence request"
            );
        });
    }

    /// The flag defaults off and the guarded application is a no-op:
    /// the desktop mount stays windowed (covers both the `Default`
    /// arm and the guard's off-path in one observable behavior).
    #[test]
    fn mount_without_the_kiosk_flag_stays_windowed() {
        on_slint_thread(|| {
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            let handle = mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                UiSettings::default(),
                health,
            )
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
            let (state, signals, system, commands, requests_tx, _requests_rx, health) = handover();
            crate::mount(
                state,
                signals,
                system,
                commands,
                requests_tx,
                crate::UiSettings::default(),
                health,
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
    use super::view_component_tests::route_action;
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
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (system_tx, system_rx) = broadcast::channel(4);
        let (signals_tx, signals_rx) = broadcast::channel(4);
        drop(system_tx);
        drop(signals_tx);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            Arc::new(ArcSwap::from_pointee(AppState::default())),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            UiSettings {
                keymap: vi_keymap(),
                ..UiSettings::default()
            },
            health,
        )
        .expect("mount succeeds on the null platform");
        // Escape the desktop default (chunk 10 D2) for the board-level
        // mechanics these tests exercise.
        route_action(&handle, crate::actions::ActionId::NavFocusBoard);
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
            let (requests_tx, _requests_rx) = mpsc::channel(16);
            let (system_tx, system_rx) = broadcast::channel(4);
            let (signals_tx, signals_rx) = broadcast::channel(4);
            drop(system_tx);
            drop(signals_tx);
            let (_health_tx, health) =
                tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
            let handle = mount(
                Arc::new(ArcSwap::from_pointee(AppState::default())),
                signals_rx,
                system_rx,
                commands_tx,
                requests_tx,
                UiSettings::default(), // standard preset
                health,
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
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            recur: epoch(),

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
            label_memberships: std::collections::BTreeMap::new(),
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: task_clocks(),
            remote_seen: None,
            remote_baseline: None,
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
        let (requests_tx, _requests_rx) = mpsc::channel(16);
        let (system_tx, system_rx) = broadcast::channel(16);
        let (signals_tx, signals_rx) = broadcast::channel(16);
        drop(system_tx);
        drop(signals_tx);
        let (_health_tx, health) =
            tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
        let handle = mount(
            Arc::new(ArcSwap::from_pointee(state)),
            signals_rx,
            system_rx,
            commands_tx,
            requests_tx,
            UiSettings {
                keymap,
                ..UiSettings::default()
            },
            health,
        )
        .expect("mount succeeds on the null platform");
        // The desktop default is Today (chunk 10 D2); these component
        // tests target board mechanics, so they escape centrally the
        // way the journeys do — one `nav.focus-board` after mount.
        route_action(&handle, crate::actions::ActionId::NavFocusBoard);
        (handle, commands_rx)
    }

    /// Dispatches one action through the adapter's route (the
    /// component-test-sized analogue of the journeys' press).
    pub(super) fn route_action(handle: &UiHandle, action: crate::actions::ActionId) {
        handle.dispatch_action(action);
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
            // The D3 reconciliation (round 1): the mouse invoke closed
            // the menu (F2's fix) — the menu stays live mid-edit.
            //
            // Recorded decision (round-3 chunk 1, plan §3.2 + the
            // audit report §5): the matrix's OpenMenu cell settles a
            // foreign flow before the modal opens — the old pin
            // ("the edit flow survived open/invoke/close") held the
            // armed-flow-over-modal behaviour the round removes; an
            // armed flow under a dismissed menu kept masking the
            // view keys (the F9 family). The menu liveness claim is
            // unchanged; the flow's settle is the new contract.
            assert!(
                !handle.window.global::<KeyboardState>().get_menu_open(),
                "the mouse invoke closed the menu"
            );
            assert!(
                !handle
                    .window
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "the menu open settled the edit flow"
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

    /// R3 chunk C — the notification surface end-to-end: a fired,
    /// undismissed occurrence renders one row, and each button
    /// dispatches exactly its engine command (Dismiss →
    /// `DismissReminder`, Done → `SetTaskDone`, Snooze/Mute the routed
    /// verbs over the row's target).
    #[test]
    fn notification_surface_buttons_send_exact_commands() {
        on_slint_thread(|| {
            let defaults = taskboard_domain::ReminderDefaults::default();
            let mut state = fixture_state();
            let overdue_id = {
                let now = chrono::Utc::now();
                taskboard_domain::overdue_tasks(&state.tasks, now)
                    .into_iter()
                    .map(|t| t.id)
                    .next()
                    .expect("fixture has one overdue task")
            };
            // The fixture task is rule-less: the implicit default fires
            // at its effective occurrence; the delivery row IS the
            // fired fact (design D-6).
            let occurrence = taskboard_domain::effective_occurrence(
                &taskboard_domain::ReminderRef::Default(overdue_id),
                &state.reminders,
                &state.tasks,
                &defaults,
            )
            .expect("the default's occurrence derives");
            state
                .reminders
                .deliveries
                .insert(taskboard_domain::ReminderDelivery {
                    reminder: taskboard_domain::ReminderRef::Default(overdue_id),
                    occurrence,
                    delivered_at: chrono::Utc::now(),
                });

            let (handle, mut commands) = handover_with(state, UiSettings::default().keymap);
            assert_eq!(
                handle.window.get_notification_model().row_count(),
                1,
                "one fired, undismissed occurrence renders one row"
            );

            handle.window.invoke_notification_dismiss_clicked(0);
            match commands.try_recv().expect("the dismiss command arrived") {
                StateCommand::DismissReminder {
                    reminder,
                    occurrence: dismissed,
                } => {
                    assert_eq!(reminder, taskboard_domain::ReminderRef::Default(overdue_id));
                    assert_eq!(dismissed, occurrence);
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }

            handle.window.invoke_notification_done_clicked(0);
            assert_eq!(
                commands.try_recv().expect("the done command arrived"),
                StateCommand::SetTaskDone {
                    id: overdue_id,
                    done: true
                }
            );

            handle.window.invoke_notification_snooze_clicked(0);
            match commands.try_recv().expect("the snooze command arrived") {
                StateCommand::SnoozeReminder {
                    reminder,
                    occurrence: snoozed,
                    postpone,
                } => {
                    assert_eq!(reminder, taskboard_domain::ReminderRef::Default(overdue_id));
                    assert_eq!(snoozed, occurrence);
                    assert_eq!(postpone, TimeDelta::hours(1));
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }

            handle.window.invoke_notification_mute_clicked(0);
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

    /// R3-F1-half (D6) — the done checkbox's seam: (column, row)
    /// resolves against the id tables and raw-sends the *toggled*
    /// `SetTaskDone`, both directions; a stale index dispatches
    /// nothing.
    #[test]
    fn the_done_checkbox_seam_sends_the_toggled_command() {
        on_slint_thread(|| {
            let mut state = fixture_state();
            // The fixture's second-stack task (the reverse direction's
            // subject), pre-marked done.
            let t2 = {
                let second = state
                    .stacks
                    .values()
                    .find(|stack| stack.order == 1)
                    .expect("fixture's second stack")
                    .id;
                state
                    .tasks
                    .values()
                    .find(|task| task.stack == second)
                    .map(|task| task.id)
                    .expect("the second stack's task")
            };
            let mut done = state.tasks.get(&t2).expect("task").clone();
            done.done = Some(chrono::Utc::now());
            state.tasks.insert(t2, done);

            let (handle, mut commands) = handover_with(state, UiSettings::default().keymap);

            // Column 0 row 0 is a not-done fixture task → done: true.
            handle.window.invoke_board_task_done_toggled(0, 0);
            match commands.try_recv().expect("the toggle-on command arrived") {
                StateCommand::SetTaskDone { id, done: value } => {
                    assert!(value, "an undone card toggles on");
                    assert_ne!(id, t2);
                }
                other => panic!("typed outcome expected, got {other:?}"),
            }

            // Column 1 row 0 is the done task → done: false.
            handle.window.invoke_board_task_done_toggled(1, 0);
            assert_eq!(
                commands.try_recv().expect("the toggle-off command arrived"),
                StateCommand::SetTaskDone {
                    id: t2,
                    done: false
                }
            );

            // A stale index against the tables dispatches nothing.
            handle.window.invoke_board_task_done_toggled(9, 9);
            assert!(commands.try_recv().is_err(), "stale index absorbed");
        });
    }

    /// R3.5.3 — the kiosk gate: the same fired occurrence renders zero
    /// notification rows in appliance mode (the banner-only ruling).
    #[test]
    fn the_notification_surface_stays_empty_on_kiosk() {
        on_slint_thread(|| {
            let defaults = taskboard_domain::ReminderDefaults::default();
            let mut state = fixture_state();
            let overdue_id = {
                let now = chrono::Utc::now();
                taskboard_domain::overdue_tasks(&state.tasks, now)
                    .into_iter()
                    .map(|t| t.id)
                    .next()
                    .expect("fixture has one overdue task")
            };
            let occurrence = taskboard_domain::effective_occurrence(
                &taskboard_domain::ReminderRef::Default(overdue_id),
                &state.reminders,
                &state.tasks,
                &defaults,
            )
            .expect("the default's occurrence derives");
            state
                .reminders
                .deliveries
                .insert(taskboard_domain::ReminderDelivery {
                    reminder: taskboard_domain::ReminderRef::Default(overdue_id),
                    occurrence,
                    delivered_at: chrono::Utc::now(),
                });

            let (commands_tx, _commands_rx) = mpsc::channel(16);
            let (requests_tx, _requests_rx) = mpsc::channel(16);
            let (system_tx, system_rx) = broadcast::channel(16);
            let (signals_tx, signals_rx) = broadcast::channel(16);
            drop(system_tx);
            drop(signals_tx);
            let (_health_tx, health) =
                tokio::sync::watch::channel(crate::presentation::DeliveryHealth::default());
            let handle = mount(
                Arc::new(ArcSwap::from_pointee(state)),
                signals_rx,
                system_rx,
                commands_tx,
                requests_tx,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
                health,
            )
            .expect("mount succeeds on the null platform");
            assert_eq!(
                handle.window.get_notification_model().row_count(),
                0,
                "kiosk is banner-only: the surface model stays empty"
            );
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
                View::Today,
                "the cycle wraps through Today"
            );
            press(&handle, "g");
            press(&handle, "t");
            assert_eq!(handle.window.get_current_view(), View::Board);

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
    /// task verbs and the reminder actions (enabled bits over the
    /// pages; the snapshot covers the dump).
    #[test]
    fn board_menu_offers_the_enabled_verbs() {
        use crate::actions::{ActionId, MenuGroup};
        use crate::menu;
        let keymap = UiSettings::default().keymap;
        let chain = [ContextId::Board, ContextId::ViNormal, ContextId::Global];
        let mut rows = menu::root_page(&keymap, &chain, &std::collections::BTreeMap::new());
        for group in MenuGroup::all() {
            rows.extend(menu::group_page(
                &keymap,
                &chain,
                &std::collections::BTreeMap::new(),
                *group,
            ));
        }
        let enabled = |id: ActionId| rows.iter().any(|r| r.action == Some(id) && r.enabled);
        assert!(enabled(ActionId::TaskComplete));
        assert!(enabled(ActionId::TaskCreate));
        assert!(enabled(ActionId::TaskMove));
        assert!(enabled(ActionId::ReminderSnooze));
        assert!(enabled(ActionId::ReminderMute));
        assert!(
            rows.iter().all(|r| r.action != Some(ActionId::MenuNext)),
            "menu cursor rows never appear in a view menu"
        );
    }
}

/// The stage-1 repetition probe (r2 interaction plan §2.7 test 1),
/// checked in: pins the pinned Slint's `for`-repeater semantics —
/// the mechanism D9's row-diff push relies on (in-place
/// `set_row_data` re-instantiates nothing; a fresh `ModelRc` resets
/// the repetition). The canary for future Slint bumps: a failure here
/// invalidates D9's no-op-path assumption and must be triaged before
/// any transport change is trusted.
#[cfg(test)]
mod repetition_probe_tests {
    use crate::test_platform::{self, on_slint_thread};
    use i_slint_core::graphics::Rgb8Pixel;
    use slint::{ComponentHandle as _, Model as _, ModelRc, SharedString, VecModel};
    use std::rc::Rc;

    slint::slint! {
        export global ProbeCounter {
            in-out property <int> count: 0;
            in-out property <int> cond: 0;
        }
        component ProbeRow inherits Text {
            init => {
                ProbeCounter.count += 1;
            }
        }
        component ProbeCondRow inherits Text {
            init => {
                ProbeCounter.cond += 1;
            }
        }

        export struct ProbeGroup {
            rows: [string],
        }
        export struct ProbeColumn {
            open: bool,
            cards: [string],
        }
        export struct ProbeBoard {
            columns: [ProbeColumn],
        }
        export component ProbeWindow inherits Window {
            width: 200px;
            height: 200px;
            in property <[string]> rows;
            // The struct-property shape (the per-refresh `set_board_model`
            // push): a plain struct re-assigned wholesale, its model
            // field carried by handle.
            in property <ProbeGroup> group;
            // The nested shape (the column → cards pair): an outer
            // model of structs, each carrying an inner model.
            in property <[ProbeGroup]> groups;
            in property <[ProbeColumn]> columns;
            // The real push's wiring: a struct property whose model
            // field rebinds through an intermediate property.
            in property <ProbeBoard> board;
            in property <[ProbeColumn]> mirror: root.board.columns;
            board-mirror := VerticalLayout {
                for c in mirror: VerticalLayout {
                    if (c.open): ProbeRow {
                        text: "wired";
                    }
                }
            }
            // The second hop: a repeater bound through an
            // intermediate property, like BoardLayout.stacks.
            wired := VerticalLayout {
                for c in root.mirror: VerticalLayout {
                    if (c.open): ProbeRow {
                        text: "wired2";
                    }
                }
            }
            VerticalLayout {
                for row in rows: ProbeRow {
                    text: row;
                }
                for inner in group.rows: ProbeRow {
                    text: inner;
                }
                for g in groups: VerticalLayout {
                    for inner in g.rows: ProbeRow {
                        text: inner;
                    }
                }
                for c in columns: VerticalLayout {
                    HorizontalLayout {
                        for card in c.cards: ProbeRow {
                            text: card;
                        }
                    }
                    if (c.open): ProbeCondRow {
                        text: "cond";
                    }
                }
            }
        }
    }

    fn mounted() -> ProbeWindow {
        let window = ProbeWindow::new().expect("the probe window instantiates");
        window
            .window()
            .set_size(slint::WindowSize::Physical(slint::PhysicalSize::new(
                200, 200,
            )));
        window
    }

    /// Forces a layout pass (the repeaters update during layout) and
    /// returns how many row instances were instantiated since the
    /// last `reset`.
    fn settle_with_parts(window: &ProbeWindow) -> (i32, i32) {
        let adapter = test_platform::window_adapter().expect("the probe owns the last window");
        let mut buffer = vec![Rgb8Pixel::default(); 200 * 200];
        let _ = adapter.render_into(&mut buffer, 200);
        let g = window.global::<ProbeCounter>();
        (g.get_count(), g.get_cond())
    }

    fn settle(window: &ProbeWindow) -> i32 {
        let adapter = test_platform::window_adapter().expect("the probe owns the last window");
        let mut buffer = vec![Rgb8Pixel::default(); 200 * 200];
        let _ = adapter.render_into(&mut buffer, 200);
        window.global::<ProbeCounter>().get_count()
    }

    fn reset(window: &ProbeWindow) {
        window.global::<ProbeCounter>().set_count(0);
    }

    fn strings(values: &[&str]) -> Vec<SharedString> {
        values.iter().map(|v| SharedString::from(*v)).collect()
    }

    /// A conditional inside a repeated row mounts when the row's
    /// scalar flips via `set_row_data` (the D9 create-row mechanism).
    #[test]
    fn a_conditional_inside_a_repeated_row_mounts_on_an_in_place_flip() {
        on_slint_thread(|| {
            let window = mounted();
            let columns = Rc::new(VecModel::from(vec![
                ProbeColumn {
                    open: false,
                    cards: ModelRc::new(VecModel::from(strings(&["c0"]))),
                },
                ProbeColumn {
                    open: false,
                    cards: ModelRc::new(VecModel::from(strings(&["c1"]))),
                },
            ]));
            window.set_columns(ModelRc::from(columns.clone()));
            assert_eq!(settle(&window), 2, "the cards mount; nothing conditional");
            // Keep the same inner cards handle, exactly as the D9 push
            // does for an unchanged column.
            let same_cards = columns.row_data(1).expect("row").cards;
            columns.set_row_data(
                1,
                ProbeColumn {
                    open: true,
                    cards: same_cards,
                },
            );
            let (_, cond) = settle_with_parts(&window);
            assert_eq!(cond, 1, "the flipped row's conditional mounts");
        });
    }

    /// The real push's wiring: struct re-assignment (equal handles)
    /// around a `set_row_data` flip must not swallow the row update.
    #[test]
    fn a_struct_reassignment_around_an_in_place_flip_keeps_the_mount() {
        on_slint_thread(|| {
            let window = mounted();
            let columns = Rc::new(VecModel::from(vec![
                ProbeColumn {
                    open: false,
                    cards: ModelRc::default(),
                },
                ProbeColumn {
                    open: false,
                    cards: ModelRc::default(),
                },
            ]));
            window.set_board(ProbeBoard {
                columns: ModelRc::from(columns.clone()),
            });
            assert_eq!(settle(&window), 0, "nothing conditional mounts");
            columns.set_row_data(
                1,
                ProbeColumn {
                    open: true,
                    cards: ModelRc::default(),
                },
            );
            window.set_board(ProbeBoard {
                columns: ModelRc::from(columns.clone()),
            });
            assert_eq!(settle(&window), 2, "the flip mounts through both hops");
        });
    }

    /// The F-3 table (plan §1, probe results 1–8): mount-once,
    /// in-place row updates and same-handle re-assignments churn
    /// nothing, a fresh `ModelRc` resets everything, `insert` adds
    /// exactly one, and the handle/struct equality is identity-based.
    #[test]
    fn the_repetition_semantics_table_holds_on_the_pinned_slint() {
        on_slint_thread(|| {
            let window = mounted();
            let rows = Rc::new(VecModel::from(strings(&["a", "b", "c"])));
            window.set_rows(ModelRc::from(rows.clone()));

            // 1. The initial mount instantiates each row exactly once,
            //    and an unchanged re-render adds nothing.
            assert_eq!(settle(&window), 3, "initial mount: one instance per row");
            reset(&window);
            assert_eq!(settle(&window), 0, "an unchanged re-render adds nothing");

            // 2. set_row_data — equal or changed — updates in place,
            //    re-instantiating nothing.
            rows.set_row_data(0, SharedString::from("a"));
            assert_eq!(settle(&window), 0, "an equal set_row_data is a no-op");
            rows.set_row_data(0, SharedString::from("a2"));
            assert_eq!(
                settle(&window),
                0,
                "a changed set_row_data updates in place"
            );

            // 3. insert adds exactly one instance; remove adds none.
            rows.insert(1, SharedString::from("x"));
            assert_eq!(settle(&window), 1, "insert adds exactly one instance");
            reset(&window);
            let _removed = rows.remove(1);
            assert_eq!(settle(&window), 0, "remove re-instantiates nothing");

            // 4. A different ModelRc — even with byte-equal content —
            //    resets the repetition: every row re-instantiates.
            window.set_rows(ModelRc::new(VecModel::from(strings(&["a2", "b", "c"]))));
            assert_eq!(settle(&window), 3, "a fresh ModelRc resets the repetition");

            // 5. Re-assigning the current handle is a no-op (capture
            //    the installed handle — phase 4 replaced it).
            let installed = window.get_rows();
            reset(&window);
            window.set_rows(installed.clone());
            assert_eq!(settle(&window), 0, "the same handle re-assigns for free");

            // 8. ModelRc's PartialEq is handle identity; the generated
            //    struct's is value equality over its fields (handles
            //    included).
            let same = installed;
            let other: ModelRc<SharedString> =
                ModelRc::new(VecModel::from(strings(&["a2", "b", "c"])));
            assert_eq!(
                same,
                window.get_rows(),
                "the installed handle reads back equal"
            );
            assert_ne!(same, other, "distinct handles compare unequal");
        });
    }

    /// F-3.6/7 — a struct property re-assigned wholesale with the same
    /// inner model handle churns nothing, an inner `set_row_data`
    /// reaches the inner repeater with zero churn and no outer write,
    /// and an outer row update over a persistent outer model that
    /// keeps the inner handle churns nothing inner.
    #[test]
    fn struct_reassignment_with_persistent_handles_and_inner_updates_churn_nothing() {
        on_slint_thread(|| {
            let window = mounted();
            let inner = Rc::new(VecModel::from(strings(&["x", "y"])));
            window.set_group(ProbeGroup {
                rows: ModelRc::from(inner.clone()),
            });
            assert_eq!(settle(&window), 2, "the inner rows mount once");
            reset(&window);

            // 6. A fresh struct value carrying the same inner handle:
            //    zero re-instantiation.
            window.set_group(ProbeGroup {
                rows: ModelRc::from(inner.clone()),
            });
            assert_eq!(
                settle(&window),
                0,
                "same-handle struct re-assignment is a no-op"
            );

            // 7a. An inner set_row_data reaches the inner repeater in
            //     place, with no outer write.
            inner.set_row_data(0, SharedString::from("x2"));
            assert_eq!(settle(&window), 0, "the inner update is in place");
        });
    }

    /// F-3.7 (nested models) — an outer `set_row_data` (equal or
    /// changed scalars) that keeps the same inner handle
    /// re-instantiates nothing inner.
    #[test]
    fn an_outer_row_update_keeping_the_inner_handle_churns_nothing_inner() {
        on_slint_thread(|| {
            let window = mounted();
            let inner = Rc::new(VecModel::from(strings(&["x", "y"])));
            let outer: Rc<VecModel<ProbeGroup>> = Rc::new(VecModel::from(vec![ProbeGroup {
                rows: ModelRc::from(inner.clone()),
            }]));
            window.set_groups(ModelRc::from(outer.clone()));
            assert_eq!(settle(&window), 2, "the inner rows mount once");
            reset(&window);

            outer.set_row_data(
                0,
                ProbeGroup {
                    rows: ModelRc::from(inner.clone()),
                },
            );
            assert_eq!(
                settle(&window),
                0,
                "the outer row update churns nothing inner"
            );

            // The struct's PartialEq compares its fields (handles by
            // identity).
            let g = ProbeGroup {
                rows: ModelRc::from(inner.clone()),
            };
            assert_eq!(g, g.clone(), "structs with identical handles compare equal");
            assert_ne!(
                g,
                ProbeGroup {
                    rows: ModelRc::new(VecModel::from(strings(&["x", "y"]))),
                },
                "a fresh inner handle makes the struct unequal"
            );
        });
    }
}
