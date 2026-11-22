// SPDX-License-Identifier: AGPL-3.0-only

//! The shared journey harness (#1062, chunk B4a plan §3/D-1062): the
//! generalization of `focus_journey.rs`'s `JourneyHarness` seed and the
//! former private `Harness` of the `journeys` binary (the superset:
//! keypress guard, files-sync watch, request-lane drain). One fixture,
//! one `driver::run`, one mount — [`run_journey`] is the per-test
//! preamble; the `DriverOptions` literal is gone because the options
//! come from the root binary's `crate::HEADLESS_RENDERER` (one
//! platform per process; #1016).

use super::driver::{DriverOptions, JourneyDriver as _, SlintDriver};
use super::fake_engine::FakeEngine;
use super::guard::Guard;
use super::health::detached_health;
use taskboard_domain::{AppState, StateCommand};

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke;
use taskboard_ui_slint::{UiSettings, View};

/// Whether the mount performs the central board escape (one
/// `nav.focus-board` after mount). `Yes` for the board-level journeys
/// (the default); `No` for the today/kiosk-default legs, which observe
/// the desktop's Today default. The former env-var seam is gone — see
/// [`Harness::mount_no_escape`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeBoard {
    Yes,
    No,
}

pub struct Harness {
    pub driver: SlintDriver,
    pub handle: taskboard_ui_slint::UiHandle,
    pub engine: FakeEngine,
    pub guard: Guard,
    /// The files watch's sender half held open (the watch's
    /// `has_changed` gate errors once every sender drops — the drained
    /// chip's seed must stay a live change).
    pub files_tx:
        Option<tokio::sync::watch::Sender<taskboard_ui_slint::presentation::FilesSyncChip>>,
    /// The request lane's receiver half (the tests drain it directly —
    /// the `FakeEngine` pattern, minus the engine). Drained by the
    /// width journeys (round-1 T-10–T-12); the request-lane plumbing
    /// commit leaves it as harness surface.
    #[allow(dead_code)]
    pub requests_rx:
        std::sync::Mutex<tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>>,
}

impl Harness {
    /// Mounts `state` through the public `mount()` with the fake's
    /// channel halves (dropped broadcast senders, as the lib tests
    /// do) — no engine, no app crate. The request lane's halves are
    /// drained by the tests through `requests_drain`.
    pub fn mount(driver: SlintDriver, state: AppState) -> Self {
        Self::mount_with_settings(driver, state, UiSettings::default())
    }

    /// The settings journeys' variant: the hand-over carries the
    /// configured connectors (the settings row list's seed).
    pub fn mount_with_settings(driver: SlintDriver, state: AppState, settings: UiSettings) -> Self {
        Self::mount_full(driver, state, settings, None)
    }

    /// The files-watch journeys' variant (the files-sync-status plan
    /// US-8, D-8): the mount carries a live `FilesSyncChip` watch —
    /// the per-board adoption states the roster rows fold over.
    pub fn mount_with_files_sync(
        driver: SlintDriver,
        state: AppState,
        settings: UiSettings,
        chip: taskboard_ui_slint::presentation::FilesSyncChip,
    ) -> Self {
        let (tx, rx) =
            tokio::sync::watch::channel(taskboard_ui_slint::presentation::FilesSyncChip::default());
        // The chip rides the channel's first send (the watch's
        // has_changed gate — the drain consumes it); the sender stays
        // alive on the harness (a closed channel errors the gate).
        let _ = tx.send(chip);
        let mut harness = Self::mount_full(driver, state, settings, Some(rx));
        harness.files_tx = Some(tx);
        harness
    }

    pub fn mount_full(
        driver: SlintDriver,
        state: AppState,
        settings: UiSettings,
        files_sync: Option<
            tokio::sync::watch::Receiver<taskboard_ui_slint::presentation::FilesSyncChip>,
        >,
    ) -> Self {
        Self::mount_full_inner(driver, state, settings, files_sync, EscapeBoard::Yes)
    }

    /// The pixel/geometry probes' variant (#1172 — the populated
    /// preset of `usability_components.rs`): the caller owns the
    /// driver (the `driver::run` job's), so the mount binds *that*
    /// driver and hands the harness back around it. Behavior-additive
    /// harness surface, no existing mount changes.
    pub fn mount_on_driver_no_escape_with_settings(
        driver: SlintDriver,
        state: AppState,
        settings: UiSettings,
    ) -> Self {
        Self::mount_full_inner(driver, state, settings, None, EscapeBoard::No)
    }

    /// The today legs' variant (chunk 10 D12): the mount stays on the
    /// desktop's Today default — no central board escape. The former
    /// `TASKBOARD_JOURNEYS_STAY_ON_TODAY` env seam is gone (#1016
    /// residue: a process-global env var set by one test leaked into
    /// every sibling under a threaded plain `cargo test` run); the
    /// choice is now a per-mount argument, not process state.
    pub fn mount_no_escape(driver: SlintDriver, state: AppState) -> Self {
        Self::mount_no_escape_with_settings(driver, state, UiSettings::default())
    }

    pub fn mount_no_escape_with_settings(
        driver: SlintDriver,
        state: AppState,
        settings: UiSettings,
    ) -> Self {
        Self::mount_full_inner(driver, state, settings, None, EscapeBoard::No)
    }

    fn mount_full_inner(
        driver: SlintDriver,
        state: AppState,
        settings: UiSettings,
        files_sync: Option<
            tokio::sync::watch::Receiver<taskboard_ui_slint::presentation::FilesSyncChip>,
        >,
        escape: EscapeBoard,
    ) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, requests_rx) = tokio::sync::mpsc::channel(16);
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            requests_tx,
            settings,
            detached_health(),
            files_sync,
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
            guard: Guard::default(),
            files_tx: None,
            requests_rx: std::sync::Mutex::new(requests_rx),
        };
        harness.driver.bind(&harness.handle);
        // The desktop default is Today (chunk 10 D2); the board-level
        // journeys escape centrally — one `nav.focus-board` after
        // mount (the deliberate regen; the today leg mounts and stays).
        if escape == EscapeBoard::Yes {
            // Routed, not `press`ed: the escape must not depend on the
            // harness's preset (press resolves against the standard
            // keymap).
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
        }
        harness
    }

    /// Drains the request lane: the typed `UiRequest`s that reached
    /// the boundary since the last check. Drained by the width
    /// journeys (round-1 T-10–T-12); the plumbing commit leaves it as
    /// harness surface.
    #[allow(dead_code)]
    pub fn drain_requests(&self) -> Vec<taskboard_ui_slint::requests::UiRequest> {
        let mut rx = self.requests_rx.lock().expect("requests lock");
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    /// The visible context chain (the adapter's `shell.chain()`
    /// mirror, read from the window globals + current view).
    pub fn chain(&self) -> Vec<ContextId> {
        let window = self.handle.window();
        let keyboard = window.global::<taskboard_ui_slint::KeyboardState>();
        if keyboard.get_menu_open() {
            return vec![ContextId::Menu];
        }
        if keyboard.get_vi_insert() {
            if keyboard.get_date_picker_grid() {
                return vec![
                    ContextId::DatePicker,
                    ContextId::ViInsert,
                    ContextId::Global,
                ];
            }
            return vec![ContextId::ViInsert, ContextId::Global];
        }
        let view = match window.get_current_view() {
            View::Today => Some(ContextId::Today),
            View::Board => Some(ContextId::Board),
            View::StackDetail => Some(ContextId::StackDetail),
            View::TaskDetail => Some(ContextId::TaskDetail),
            View::Review => Some(ContextId::Review),
            View::Settings => Some(ContextId::Settings),
            View::Status => Some(ContextId::StatusPanel),
            View::PlanNextDay => Some(ContextId::PlanNextDay),
        };
        let mut chain = Vec::with_capacity(4);
        if keyboard.get_date_picker_grid() {
            chain.push(ContextId::DatePicker);
        }
        chain.extend(view);
        chain.push(ContextId::ViNormal);
        chain.push(ContextId::Global);
        chain
    }

    /// Presses one *action* by resolving its chain-visible binding
    /// through the same lookup the generated menu uses
    /// (`Keymap::first_binding_display` + the registry's `enabled`
    /// bit — reuse, not reimplement) and injecting each stroke;
    /// counts the presses on the open guard phase (§9.2 accounting).
    pub fn press(&mut self, action: ActionId) {
        let keymap = UiSettings::default().keymap;
        let chain = self.chain();
        let registry = taskboard_ui_slint::actions::entry(action);
        assert!(registry.enabled, "{action:?} is enabled");
        let hint = keymap
            .first_binding_display(action, &chain)
            .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain {chain:?}"));
        for spelling in &hint {
            let stroke = stroke::parse(spelling)
                .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
            let presses = self.driver.press_stroke(stroke);
            self.guard.press_n(presses);
        }
    }

    /// Injects one raw stroke (a text field's Return, a flow's
    /// arrow) — counted like any press.
    pub fn stroke(&mut self, stroke: taskboard_ui_slint::keymap::Stroke) {
        let presses = self.driver.press_stroke(stroke);
        self.guard.press_n(presses);
    }

    /// Types raw text (the title-entry convention: the characters go
    /// through the same `press_stroke` path — they count).
    pub fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            let stroke = taskboard_ui_slint::keymap::Stroke {
                key: ch,
                ctrl: false,
                alt: false,
                shift: false,
                meta: false,
            };
            let presses = self.driver.press_stroke(stroke);
            self.guard.press_n(presses);
        }
    }

    /// Clears the seeded buffer through the x affordance (round-3
    /// D11): the typed-entry legs start from an emptied field, the
    /// way a user retypes.
    pub fn clear_seeded(&mut self) {
        self.driver
            .find_by_label("Clear")
            .expect("the x affordance is mounted on a seeded, focused field")
            .invoke_default_action();
        self.driver.advance_time(0);
    }

    /// Drains the boundary and asserts exactly `expected` arrived
    /// since the last check (the "nothing else dispatched"
    /// convention).
    pub fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The board's card counts per column.
    pub fn board_shape(&self) -> Vec<usize> {
        let stacks = self.handle.window().get_board_model().stacks;
        (0..stacks.row_count())
            .map(|column| stacks.row_data(column).expect("column").tasks.row_count())
            .collect()
    }

    /// The pushed rendered width of column `column` (round-1 D16):
    /// ≤ 0 = auto.
    pub fn column_width_px(&self, column: usize) -> f32 {
        self.handle
            .window()
            .get_board_model()
            .stacks
            .row_data(column)
            .expect("column")
            .width_px
    }

    /// The emphasized card, resolved through the id tables' ordering:
    /// `Some((column, row))` of the card whose model carries the
    /// focus bit.
    pub fn emphasized_card(&self) -> Option<(usize, usize)> {
        let board = self.handle.window().get_board_model();
        for column in 0..board.stacks.row_count() {
            let tasks = board.stacks.row_data(column).expect("column").tasks;
            for row in 0..tasks.row_count() {
                if tasks.row_data(row).expect("card").emphasis {
                    return Some((column, row));
                }
            }
        }
        None
    }
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

type SignalReceiver = tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent>;

fn dummy_system_receiver() -> SignalReceiver {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

/// The per-test preamble (#1062 D-1062): one fixture (held at the
/// call site — the test names which one it speaks), one
/// `driver::run`, one mount. Options come from
/// `crate::HEADLESS_RENDERER` — no per-test literal (the literal is
/// what made the #1016 mode mix possible).
pub fn run_journey<R>(
    state: taskboard_domain::AppState,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_with_settings(state, UiSettings::default(), job)
}

/// The settings journeys' preamble: `run_journey` with a
/// configured `UiSettings` hand-over (the settings row list's
/// seed).
pub fn run_journey_with_settings<R>(
    state: taskboard_domain::AppState,
    settings: UiSettings,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_full(state, settings, None, move |harness| job(harness))
}

/// The files-watch journeys' preamble (the files-sync-status plan
/// US-8, D-8): the mount carries a live `FilesSyncChip` watch —
/// the sender half stays on the harness (`mount_with_files_sync`
/// verbatim), so the chip's seed outlives the job's body.
pub fn run_journey_with_files_sync<R>(
    state: taskboard_domain::AppState,
    settings: UiSettings,
    chip: taskboard_ui_slint::presentation::FilesSyncChip,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_full_inner(move |_driver| {
        let mut harness =
            Harness::mount_with_files_sync(SlintDriver::default(), state, settings, chip);
        job(&mut harness)
    })
}

/// The today-legs' preamble: `run_journey` without the central board
/// escape — the mount stays on the desktop's Today default (the
/// legacy private preamble of `habits_journey.rs`, migrated with
/// #1112: the habit journeys' Today posture is their assertion
/// surface, not incidental).
pub fn run_journey_no_escape<R>(
    state: taskboard_domain::AppState,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_no_escape_with_settings(state, UiSettings::default(), job)
}

/// The today-legs' settings variant: `run_journey_no_escape` with a
/// configured `UiSettings` (the kiosk-posture legs).
pub fn run_journey_no_escape_with_settings<R>(
    state: taskboard_domain::AppState,
    settings: UiSettings,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_full_inner(move |_driver| {
        let mut harness =
            Harness::mount_no_escape_with_settings(SlintDriver::default(), state, settings);
        job(&mut harness)
    })
}

/// The full-shape preamble: explicit settings and files-sync
/// receiver. The `driver::run` `SENDER`/`OPTIONS` `OnceLock`
/// mechanics are untouched — this wraps them, reading
/// `crate::HEADLESS_RENDERER` for the one-platform decision.
fn run_journey_full<R>(
    state: taskboard_domain::AppState,
    settings: UiSettings,
    files_sync: Option<
        tokio::sync::watch::Receiver<taskboard_ui_slint::presentation::FilesSyncChip>,
    >,
    job: impl FnOnce(&mut Harness) -> R + Send + 'static,
) -> R
where
    R: Send + 'static,
{
    run_journey_full_inner(move |_driver| {
        let mut harness = Harness::mount_full(SlintDriver::default(), state, settings, files_sync);
        job(&mut harness)
    })
}

fn run_journey_full_inner<R>(job: impl FnOnce(&mut SlintDriver) -> R + Send + 'static) -> R
where
    R: Send + 'static,
{
    super::driver::run(
        DriverOptions {
            headless_renderer: crate::HEADLESS_RENDERER,
        },
        job,
    )
}
