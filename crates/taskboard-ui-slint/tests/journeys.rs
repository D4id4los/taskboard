// SPDX-License-Identifier: AGPL-3.0-only

//! The keyboard-journey suite (design §7.3/§8.1, chunk plan §5): the
//! ten core verbs as one continuous session plus the cold-start view
//! graph walk, driven through the `tests/support` harness.
//!
//! Assertion discipline (§10.3): terminal state + focus ownership +
//! the typed commands that reached the forwarder boundary. Accessible
//! labels are element *addresses*; status/error message text is never
//! matched.

mod support;

use chrono::TimeDelta;
use chrono::TimeZone as _;
use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::guard::{Guard, Verb};
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand, TaskId};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke::{self, parse};
use taskboard_ui_slint::{FieldValidity, SettingsOutcome, UiSettings, View};

/// The harness (chunk plan §5): driver + fake engine + fixture + the
/// keypress guard. Everything runs on the driver thread inside one
/// `driver::run` job.
struct Harness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
    guard: Guard,
    /// The request lane's receiver half (the tests drain it directly —
    /// the `FakeEngine` pattern, minus the engine). Drained by the
    /// width journeys (round-1 T-10–T-12); the request-lane plumbing
    /// commit leaves it as harness surface.
    #[allow(dead_code)]
    requests_rx:
        std::sync::Mutex<tokio::sync::mpsc::Receiver<taskboard_ui_slint::requests::UiRequest>>,
}

impl Harness {
    /// Mounts `state` through the public `mount()` with the fake's
    /// channel halves (dropped broadcast senders, as the lib tests
    /// do) — no engine, no app crate. The request lane's halves are
    /// drained by the tests through `requests_drain`.
    fn mount(driver: SlintDriver, state: AppState) -> Self {
        Self::mount_with_settings(driver, state, UiSettings::default())
    }

    /// The settings journeys' variant: the hand-over carries the
    /// configured connectors (the settings row list's seed).
    fn mount_with_settings(driver: SlintDriver, state: AppState, settings: UiSettings) -> Self {
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
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
            guard: Guard::default(),
            requests_rx: std::sync::Mutex::new(requests_rx),
        };
        harness.driver.bind(&harness.handle);
        // The desktop default is Today (chunk 10 D2); the board-level
        // journeys escape centrally — one `nav.focus-board` after
        // mount (the deliberate regen; the today leg mounts and stays).
        if !std::env::var("TASKBOARD_JOURNEYS_STAY_ON_TODAY").is_ok_and(|v| v == "1") {
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
    fn drain_requests(&self) -> Vec<taskboard_ui_slint::requests::UiRequest> {
        let mut rx = self.requests_rx.lock().expect("requests lock");
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    /// The visible context chain (the adapter's `shell.chain()`
    /// mirror, read from the window globals + current view).
    fn chain(&self) -> Vec<ContextId> {
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
    fn press(&mut self, action: ActionId) {
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
    fn stroke(&mut self, stroke: taskboard_ui_slint::keymap::Stroke) {
        let presses = self.driver.press_stroke(stroke);
        self.guard.press_n(presses);
    }

    /// Types raw text (the title-entry convention: the characters go
    /// through the same `press_stroke` path — they count).
    fn type_text(&mut self, text: &str) {
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
    fn clear_seeded(&mut self) {
        self.driver
            .find_by_label("Clear")
            .expect("the x affordance is mounted on a seeded, focused field")
            .invoke_default_action();
        self.driver.advance_time(0);
    }

    /// Drains the boundary and asserts exactly `expected` arrived
    /// since the last check (the "nothing else dispatched"
    /// convention).
    fn assert_commands(&mut self, expected: &[StateCommand]) {
        let drained = self.engine.drain();
        assert_eq!(&drained, expected, "the boundary captured exactly these");
    }

    /// The board's card counts per column.
    fn board_shape(&self) -> Vec<usize> {
        let stacks = self.handle.window().get_board_model().stacks;
        (0..stacks.row_count())
            .map(|column| stacks.row_data(column).expect("column").tasks.row_count())
            .collect()
    }

    /// The pushed rendered width of column `column` (round-1 D16):
    /// ≤ 0 = auto.
    fn column_width_px(&self, column: usize) -> f32 {
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
    fn emphasized_card(&self) -> Option<(usize, usize)> {
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

fn keyboard(harness: &Harness) -> taskboard_ui_slint::KeyboardState<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::KeyboardState>()
}
fn model_snapshot(harness: &Harness) -> Vec<String> {
    let entries = keyboard(harness).get_menu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row").action.to_string())
        .collect()
}

fn tooltip_state(harness: &Harness) -> taskboard_ui_slint::TooltipState<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TooltipState>()
}

fn facts(harness: &Harness) -> taskboard_ui_slint::MenuPanelFacts<'_> {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::MenuPanelFacts>()
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

/// The device-local wall-clock's UTC instant (r4 D28): the journeys'
/// tz-invariant expectation helper — the grammar speaks the local
/// zone, storage stays UTC instants; the journey computes the seam
/// conversion the way the app does. Fixed dates/times only — never a
/// DST edge.
#[allow(clippy::too_many_arguments)]
fn local_wall(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::Local
        .with_ymd_and_hms(y, mo, d, h, mi, 0)
        .single()
        .expect("a resolvable local wall-clock")
        .with_timezone(&chrono::Utc)
}

/// The local midnight convenience.
fn local_midnight(y: i32, mo: u32, d: u32) -> chrono::DateTime<chrono::Utc> {
    local_wall(y, mo, d, 0, 0)
}

/// §7.3 — the ten core verbs in one continuous session (one
/// continuous journey; per-step asserts inside — the length is the
/// contract's, not sprawl).
#[allow(clippy::too_many_lines)]
#[test]
fn the_ten_step_keyboard_session() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // -- step 1: launch → the board view owns focus --------------
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            assert_eq!(harness.board_shape(), vec![2, 1], "one column per stack");
            assert!(
                harness.driver.find_by_label("Board").is_some(),
                "the board region is addressable"
            );
            // Focus *selection* seeds on the first walk step (the model
            // seam starts unfocused); ownership asserted from step 2 on.
            harness.assert_commands(&[]);

            // -- step 2: navigate items and views ------------------------
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "the first walk step lands on the first card"
            );
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "the second task of the first stack takes the emphasis"
            );
            harness.press(ActionId::NavPrevItem);
            assert_eq!(harness.emphasized_card(), Some((0, 0)));
            harness.assert_commands(&[]); // navigation dispatches nothing

            harness.press(ActionId::NavNextView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail
            );
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.press(ActionId::NavNextView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Review,
                "the view cycle crosses the review surface (#214) before wrapping"
            );
            harness.press(ActionId::NavNextView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Today,
                "the view cycle wraps through Today (chunk 10 D2)"
            );
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            harness.press(ActionId::NavPrevView);
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Review,
                "the backwards wrap crosses the review surface (#214)"
            );
            harness.press(ActionId::NavPrevView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail
            );
            harness.press(ActionId::NavPrevView);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            harness.assert_commands(&[]);

            // -- step 3: task.create — one-character title (the guard
            //    convention: create counts N + x + Return against ≤ 3) --
            let create_row = |harness: &Harness, column: usize| {
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(column)
                    .expect("column")
                    .create_open
            };
            harness.guard.begin(Verb::Create);
            harness.press(ActionId::TaskCreate);
            assert!(create_row(&harness, 0), "the create row opened in stack a");
            harness.assert_commands(&[]);
            harness.type_text("x");
            harness.stroke(parse("Return").expect("Return"));
            harness.guard.end(Verb::Create);
            assert!(!create_row(&harness, 0), "the commit closed the row");
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "x".into(),
                stack: fixture.stack_a,
                order: 2,
            }]);
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.board_shape(),
                vec![3, 1],
                "the created task joined stack a"
            );

            // -- step 4: task.edit — walk to the created task, open the
            //    detail, type over the selected title, save (R1's risk
            //    case: the title field's keyboard reachability) ---------
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 2)),
                "the created task takes the emphasis"
            );
            harness.guard.begin(Verb::Edit);
            harness.press(ActionId::TaskEdit);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::TaskDetail,
                "task.edit opens the detail (focus ownership)"
            );
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit never auto-arms the title; the editor arms
            // via the bound `task.title-edit` action (F2 in the
            // standard preset).
            harness.press(ActionId::TaskTitleEdit);
            harness.type_text("y");
            harness.stroke(parse("Return").expect("Return"));
            harness.guard.end(Verb::Edit);
            // The created task's id only exists once the fake applied the
            // create; the edit's command is matched structurally (exact
            // variant + typed title) and its id carries steps 5–7.
            let drained = harness.engine.drain();
            let created = match drained.as_slice() {
                [StateCommand::UpdateTask { id, changes }]
                    if changes.title.as_deref() == Some("y") =>
                {
                    Some(*id)
                }
                other => panic!("the title edit reached the boundary, got {other:?}"),
            };
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.handle.window().get_task_model().title,
                "y",
                "the detail model reflects the retitled task"
            );
            let created = created.expect("the edit command carried the created task's id");

            // -- step 5: task.complete (the standard preset binds Space in
            //    the Board context; the keyboard path back is focus-board)
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            harness.guard.begin(Verb::Complete);
            harness.press(ActionId::TaskComplete);
            harness.guard.end(Verb::Complete);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: created,
                done: true,
            }]);
            harness.engine.settle(&harness.handle);

            // -- step 6: task.move — begin, retarget, confirm -------------
            harness.guard.begin(Verb::Move);
            harness.press(ActionId::TaskMove);
            assert!(
                harness.handle.window().get_flow_hint_model().visible,
                "the flow hint shows while moving"
            );
            harness.stroke(parse("Down").expect("Down"));
            assert_eq!(
                harness.handle.window().get_board_model().move_target,
                1,
                "the second stack highlights as the target"
            );
            harness.stroke(parse("Return").expect("Return"));
            harness.guard.end(Verb::Move);
            assert!(
                !harness.handle.window().get_flow_hint_model().visible,
                "the confirm ends the flow hint"
            );
            harness.assert_commands(&[StateCommand::MoveTask {
                id: created,
                stack: fixture.stack_b,
                order: 1,
            }]);
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.board_shape(),
                vec![2, 2],
                "the created task joined stack b"
            );

            // -- step 7: task.delete — the confirmation-free flow ---------
            harness.press(ActionId::TaskDelete);
            harness.assert_commands(&[StateCommand::DeleteTask { id: created }]);
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.board_shape(),
                vec![2, 1],
                "the view reconciles off the deleted subject"
            );
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the session stays on the board"
            );

            // -- step 8: app.menu — open, cursor walk to the last row
            //    (Quit is pinned last by the curation), invoke: the
            //    keyboard re-dispatch path closes the menu and fires
            //    the exit effect (counter seam).
            harness.guard.begin(Verb::MenuInvoke);
            harness.press(ActionId::AppMenu);
            macro_rules! keyboard_state {
                () => {
                    harness
                        .handle
                        .window()
                        .global::<taskboard_ui_slint::KeyboardState>()
                };
            }
            assert!(keyboard_state!().get_menu_open(), "app.menu opens the menu");
            assert_eq!(keyboard_state!().get_menu_selected_index(), 0);
            let rows = keyboard_state!().get_menu_entries().row_count();
            assert!(rows > 1, "the curated root page offers multiple rows");
            for _ in 1..rows {
                harness.press(ActionId::MenuNext);
            }
            assert_eq!(
                keyboard_state!().get_menu_selected_index(),
                i32::try_from(rows - 1).unwrap_or(i32::MAX),
                "the cursor reaches the pinned-last Quit row"
            );
            assert_eq!(
                keyboard_state!()
                    .get_menu_entries()
                    .row_data(rows - 1)
                    .expect("row")
                    .action,
                "app.exit",
                "Quit is the last row"
            );
            harness.press(ActionId::MenuInvoke);
            harness.guard.end(Verb::MenuInvoke);
            assert!(
                !keyboard_state!().get_menu_open(),
                "the invoke closed the menu"
            );
            assert_eq!(
                harness.handle.quit_requests(),
                1,
                "the re-dispatched Quit fired the exit effect"
            );
            harness.assert_commands(&[]);

            // -- step 9: read the sync status indicator -------------------
            //    Typed model read (the badge's label text is data, never
            //    an assertion target).
            let sync = harness.handle.window().get_sync_model();
            assert_eq!(sync.phase, taskboard_ui_slint::SyncPhaseUi::Offline);
            assert_eq!(sync.pending, 2, "the fixture's pending ops render");

            // -- step 10: app.exit — the typed quit outcome (the second
            //    quit of the session; step 8's menu invoke was the
            //    first) -----------------------------------------------
            harness.press(ActionId::AppExit);
            assert_eq!(
                harness.handle.quit_requests(),
                2,
                "the exit effect fired once per quit (menu + direct)"
            );
            harness.assert_commands(&[]);

            // -- T7: the §9.2 verdict over the session's counts -----------
            for (verb, count) in harness.guard.recorded() {
                tracing::info!(?verb, count, "keypress count");
            }
            assert_eq!(
                support::guard::verdict(harness.guard.recorded()),
                [],
                "no §9.2 ceiling exceeded"
            );
        },
    );
}

/// The chunk-7 board leg (design §3.8, one behaviour, one test): the
/// keyboard walk over the canonical board order with no wrap, the
/// second board's stacks re-rendered, the toolbar title switched,
/// nothing dispatched at any step (the dumb-client pin), and a
/// compact leg — the strip hidden, keyboard switching remains.
#[test]
fn board_switching_walks_boards_by_keyboard() {
    let (_fixture, state) = fixture::two_board_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            assert_eq!(harness.board_shape(), vec![2, 1], "board one's stacks");
            assert_eq!(
                harness.handle.window().get_toolbar_title(),
                "board one",
                "the seed is the lowest live board"
            );
            harness.assert_commands(&[]);

            harness.press(ActionId::BoardNext);
            assert_eq!(
                harness.board_shape(),
                vec![1],
                "the second board's stacks re-rendered"
            );
            assert_eq!(harness.handle.window().get_toolbar_title(), "board two");
            harness.assert_commands(&[]);

            harness.press(ActionId::BoardNext);
            assert_eq!(harness.board_shape(), vec![1], "the last end holds");
            assert_eq!(harness.handle.window().get_toolbar_title(), "board two");

            harness.press(ActionId::BoardPrevious);
            assert_eq!(harness.board_shape(), vec![2, 1], "back to board one");
            assert_eq!(harness.handle.window().get_toolbar_title(), "board one");
            harness.assert_commands(&[]);

            // The compact leg (R9/kiosk): at 600 px the strip is
            // hidden and the keyboard walk still switches boards.
            harness.driver.set_window_size(600, 800);
            assert!(
                harness.handle.window().get_compact_active(),
                "600 px is the compact breakpoint"
            );
            harness.press(ActionId::BoardNext);
            assert_eq!(
                harness.board_shape(),
                vec![1],
                "keyboard switching works while the strip is hidden"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// D2/P1 — the outside-click dismissal layer: a click off the panel
/// closes the menu (before the layer existed it did nothing), and a
/// stolen hamburger click — the full-window layer covers the toolbar —
/// converges on the same full close. The layer's z-order is pinned by
/// this behavior, not by comments.
#[test]
fn an_outside_click_dismisses_the_menu() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // A fixed geometry: the hamburger button occupies the
            // toolbar's right edge (x 952–984, y 16–48 at 1000 px).
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0); // instantiate the layer
            assert!(keyboard(&harness).get_menu_open());
            // Off-panel: the window's left edge, mid-height.
            harness.driver.click_at(10.0, 450.0);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "an outside click dismisses"
            );

            // The stolen hamburger click: the layer covers the toolbar,
            // so the click dismisses instead of re-toggling.
            harness.handle.window().invoke_toolbar_menu_clicked();
            harness.driver.advance_time(0); // instantiate the layer
            assert!(keyboard(&harness).get_menu_open());
            harness.driver.click_at(968.0, 32.0);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "a click over the hamburger while open is a dismissal, not a re-open"
            );
        },
    );
}

/// D6 — the two-tier menu in both presentation modes: a parent row's
/// invocation opens the group's page (fly-out at regular width — its
/// facts land beside the root panel's; replace + a "Back" address at
/// compact width), `menu.next` walks the submenu, a leaf invocation
/// routes and closes, and Escape walks back level by level.
#[test]
#[allow(clippy::too_many_lines)] // the two-mode walk is the contract's length
fn the_two_tier_menu_navigates_levels_in_both_modes() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Walk the cursor to the first submenu-parent row (the
            // quick rows precede the parents on the root page).
            let open_at_parent = |harness: &mut Harness| {
                harness.press(ActionId::AppMenu);
                let parent_index = (0..{
                    let entries = keyboard(harness).get_menu_entries();
                    entries.row_count()
                })
                    .find(|i| {
                        keyboard(harness)
                            .get_menu_entries()
                            .row_data(*i)
                            .expect("row")
                            .submenu
                    })
                    .expect("the root page carries submenu parents");
                for _ in 0..parent_index {
                    harness.press(ActionId::MenuNext);
                }
                harness.press(ActionId::MenuInvoke);
                assert!(
                    keyboard(harness).get_menu_active_group() >= 0,
                    "the parent invocation opened the group's page"
                );
                assert!(
                    keyboard(harness).get_menu_submenu_entries().row_count() > 0,
                    "the submenu model is mounted"
                );
            };

            // -- regular/expansive: the fly-out beside the root panel --
            harness.driver.set_window_size(1000, 900);
            open_at_parent(&mut harness);
            assert!(
                facts(&harness).get_flyout_width() > 0.0,
                "the fly-out panel is mounted"
            );
            assert!(
                facts(&harness).get_flyout_x() + facts(&harness).get_flyout_width()
                    <= facts(&harness).get_x(),
                "the fly-out sits left of the root panel, clear of it"
            );
            assert!(
                facts(&harness).get_flyout_x() >= 8.0,
                "the fly-out stays inside the window"
            );
            // `menu.next` walks the submenu; invoking the first leaf
            // (task.create over the board chain) routes and closes.
            harness.press(ActionId::MenuInvoke);
            assert!(
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "the leaf invocation routed (the create row opened)"
            );
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the leaf invocation closed the menu"
            );
            harness.stroke(parse("Escape").expect("Escape")); // discard the flow

            // Escape walks back level by level.
            open_at_parent(&mut harness);
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                keyboard(&harness).get_menu_open()
                    && keyboard(&harness).get_menu_active_group() == -1,
                "Escape at a submenu returns to the root page"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                !keyboard(&harness).get_menu_open(),
                "Escape at the root closes"
            );

            // -- compact: replace mode with the Back address --
            harness.driver.set_window_size(600, 800);
            open_at_parent(&mut harness);
            // Compact replaces: the Back row (chrome above the list)
            // exists only in this mode.
            harness.driver.click_by_label("Back");
            assert!(
                keyboard(&harness).get_menu_open()
                    && keyboard(&harness).get_menu_active_group() == -1,
                "the back row returned to the root page"
            );
            // The replaced page's rows dispatch: the root cursor was
            // preserved on the parent row, so re-entering is one
            // invoke; the second invoke fires the submenu's first leaf
            // (task.create over the board chain).
            harness.press(ActionId::MenuInvoke);
            assert!(
                keyboard(&harness).get_menu_active_group() >= 0,
                "re-entered"
            );
            harness.press(ActionId::MenuInvoke);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the compact leaf invocation closed the menu"
            );
            assert!(
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("col")
                    .create_open,
                "the compact leaf invocation routed"
            );
        },
    );
}

/// D4/P2/P2b — the panel is bounded on all four edges at every window
/// size, and the narrow fallback engages at 320 px (the panel spans
/// the window minus the space-2 margins). Structural reads of the
/// `MenuPanelFacts` rendering facts.
#[test]
fn the_menu_panel_stays_inside_the_window_at_every_size() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            for (width, height) in [(1000u32, 800u32), (600, 800), (320, 400)] {
                harness.driver.set_window_size(width, height);
                harness.press(ActionId::AppMenu);
                harness.driver.advance_time(0);
                assert!(
                    harness
                        .handle
                        .window()
                        .global::<taskboard_ui_slint::KeyboardState>()
                        .get_menu_open()
                );
                let (x, w) = (facts(&harness).get_x(), facts(&harness).get_width());
                let margin = 8.0; // Theme.space-2 at scale 1
                #[allow(clippy::cast_precision_loss)]
                let (win_w, win_h) = (width as f32, height as f32);
                assert!(
                    x >= margin && x + w <= win_w - margin,
                    "panel [{x}, {}] inside [{margin}, {}] at {width}x{height}",
                    x + w,
                    win_w - margin,
                );
                let y = facts(&harness).get_y();
                let h = facts(&harness).get_height();
                assert!(
                    y >= margin && y + h <= win_h - margin,
                    "panel vertically inside at {width}x{height}: y={y} h={h}"
                );
                if width == 320 {
                    assert_eq!(
                        w, 304.0,
                        "the narrow fallback spans the window minus margins"
                    );
                }
                harness.stroke(parse("Escape").expect("Escape"));
                assert!(
                    !harness
                        .handle
                        .window()
                        .global::<taskboard_ui_slint::KeyboardState>()
                        .get_menu_open()
                );
            }
        },
    );
}

/// D3/F2 — a mouse row invocation routes AND closes: clicking the
/// "Edit task" row over a focused card navigates to the task detail
/// and the menu is closed afterwards — the same observable end state
/// the keyboard invoke path has always had.
#[test]
fn a_mouse_row_invocation_routes_and_closes() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // The curated root page's only enabled dispatching row is
            // the pinned Quit row (the view verbs live behind the
            // submenus) — the typed routing outcome is the exit effect.
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0); // instantiate the menu tree
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );

            harness.driver.click_by_label("Quit taskboard");
            assert_eq!(
                harness.handle.quit_requests(),
                1,
                "the routed action dispatched"
            );
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open(),
                "the mouse invoke closed the menu"
            );
            harness.assert_commands(&[]); // nothing reached the engine
        },
    );
}

/// Undo v1 surfaces (undo/2, ADR 0019) — J-1: the undo rows grey from
/// the published `UndoView`: an empty line renders both rows disabled;
/// a published `can_undo`/`can_redo` view enables them while the menu
/// holds (the models pass rebuilds the greying map on refresh).
#[test]
fn the_undo_rows_grey_from_the_published_undo_view() {
    let (_fixture, mut state) = fixture::golden_state();
    // golden_state carries the default (empty-line) view already; make
    // it explicit, then the live view for the second phase.
    state.undo = taskboard_domain::UndoView::default();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mounted_state = state.clone();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0); // instantiate the menu tree

            let row_state = |harness: &Harness, action: &str| {
                let entries = keyboard(harness).get_menu_entries();
                (0..entries.row_count())
                    .map(|i| entries.row_data(i).expect("row"))
                    .find(|row| row.action == action)
                    .expect("the row renders on the root page")
                    .enabled
            };
            assert!(
                !row_state(&harness, "app.undo") && !row_state(&harness, "app.redo"),
                "an empty line greys both rows"
            );

            // The engine publishes a live view; the open menu re-greys.
            // (The published snapshot is the one this test mounted —
            // nothing else has written — so the local clone is it.)
            let mut live = mounted_state;
            live.undo = taskboard_domain::UndoView {
                can_undo: true,
                can_redo: true,
                head_label: None,
                recent: vec![],
            };
            harness.engine.replace_state(live, &harness.handle);
            harness.driver.advance_time(0);
            assert!(
                row_state(&harness, "app.undo") && row_state(&harness, "app.redo"),
                "a live view enables both rows while the menu holds"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// Undo v1 surfaces (undo/2, ADR 0019) — J-2: invoking the rows sends
/// the target-free engine verbs through the fire-and-forget command
/// boundary (the dumb-client pin at the UI edge).
#[test]
fn invoking_the_undo_rows_sends_the_engine_verbs() {
    let (_fixture, mut state) = fixture::golden_state();
    state.undo = taskboard_domain::UndoView {
        can_undo: true,
        can_redo: true,
        head_label: None,
        recent: vec![],
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);

            harness.driver.click_by_label("Undo");
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::Undo]);

            // The menu closed on the routed click (the mouse-row
            // contract); reopen for the redo row.
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Redo");
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::Redo]);
        },
    );
}

/// F1/D1 — the hamburger is a stateful toggle and every open shows
/// the same entries: open → second click closes → third click opens a
/// model equal to the first open's (row count + per-row action ids,
/// structural).
#[test]
fn the_hamburger_toggles_and_every_open_shows_the_same_entries() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            harness.handle.window().invoke_toolbar_menu_clicked();
            assert!(keyboard(&harness).get_menu_open(), "the first click opens");
            let first = model_snapshot(&harness);
            assert!(first.len() > 1, "the populated board offers rows");

            harness.handle.window().invoke_toolbar_menu_clicked();
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the second click closes"
            );

            harness.handle.window().invoke_toolbar_menu_clicked();
            assert!(
                keyboard(&harness).get_menu_open(),
                "the third click reopens"
            );
            assert_eq!(
                model_snapshot(&harness),
                first,
                "the second open reproduces the first open's entries"
            );

            // The keyboard path never re-fires while open: Escape
            // (menu.dismiss) closes cleanly after the toggle.
            let opened = keyboard(&harness).get_menu_open();
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(opened, "the third click left the menu open");
            assert!(!keyboard(&harness).get_menu_open());
        },
    );
}

/// §8.1 scriptability — the cold-start view-graph walk (chunk plan
/// §5.2): every view reachable by action, `nav.focus-board` returns
/// from everywhere, the menu and the edit flow's Escape always land
/// in a stable state (no dead ends), and the compact board's stack
/// switcher is keyboard-complete.
#[test]
#[allow(clippy::too_many_lines)] // the walk is the contract's length
fn the_view_graph_walks_from_a_cold_start_without_dead_ends() {
    fn current_view(harness: &Harness) -> View {
        harness.handle.window().get_current_view()
    }
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // Cold start: the board, no selection yet.
            assert_eq!(current_view(&harness), View::Board);
            assert_eq!(harness.emphasized_card(), None, "no focus at rest");

            // Every view is reachable by action, and nav.focus-board
            // (Alt+1) returns from everywhere.
            harness.press(ActionId::NavNextView);
            assert_eq!(current_view(&harness), View::StackDetail);
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);

            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(current_view(&harness), View::TaskDetail);
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);

            // The cycle wraps from any starting view (no one-way edges).
            // Review (#214) sits between TaskDetail and Today: the
            // backwards wrap from Board crosses Today first, then
            // Review, then TaskDetail.
            harness.press(ActionId::NavPrevView);
            assert_eq!(current_view(&harness), View::Today, "wrap backwards");
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                current_view(&harness),
                View::Review,
                "the wrap's far edge is the review surface"
            );
            harness.press(ActionId::NavPrevView);
            assert_eq!(current_view(&harness), View::TaskDetail);
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);

            // The forward cycle reaches Review the same way: it is
            // read-only (a ledger), so entering it arms no flow and
            // nav.focus-board still returns.
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(current_view(&harness), View::Review);
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);

            // The menu opens from the board and dismisses cleanly.
            harness.press(ActionId::AppMenu);
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );

            // The create flow's Escape ends the flow back at rest.
            harness.press(ActionId::TaskCreate);
            assert!(
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("column")
                    .create_open
            );
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                !harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("column")
                    .create_open,
                "Escape discarded the create flow"
            );

            // The edit flow's Escape leaves a stable TaskDetail (no dead
            // end): focus-board still works afterwards.
            harness.press(ActionId::NavNextItem); // seed the focus walk
            harness.press(ActionId::TaskEdit);
            assert_eq!(current_view(&harness), View::TaskDetail);
            harness.stroke(parse("Escape").expect("Escape"));
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_vi_insert(),
                "the discard returned to normal mode"
            );
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);
            harness.assert_commands(&[]); // the whole walk dispatched nothing

            // One compact leg (§6.4's keyboard-completeness proof): at
            // 600 px the switcher leads — nav.next-item walks the stacks
            // with selection-follows-focus, wrapping.
            harness.driver.set_window_size(600, 800);
            assert!(
                harness.handle.window().get_compact_active(),
                "600 px is the compact breakpoint"
            );
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                -1,
                "no stack is explicitly selected at mount (compact still renders the first via its effective-selected clamp)"
            );
            // Retreat hands the card focus back to the switcher level
            // (D8's selection-follows-focus entry point).
            harness.press(ActionId::NavPrevItem);
            assert_eq!(harness.emphasized_card(), None, "the switcher leads");
            // From the switcher, Down selects stack a and lands its first
            // task; the next Down walks that stack's cards.
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                0,
                "the switcher step selects stack a"
            );
            assert_eq!(harness.emphasized_card(), Some((0, 0)));
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                0,
                "the walk continues inside stack a"
            );
            assert_eq!(harness.emphasized_card(), Some((0, 1)));
            // Retreating past the first card hands focus back to the
            // switcher; the next step from there advances the selection
            // (wrap) to stack b and lands its first task.
            harness.press(ActionId::NavPrevItem);
            harness.press(ActionId::NavPrevItem);
            assert_eq!(harness.emphasized_card(), None, "back at the switcher");
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the switcher selection advanced (wrapped) to stack b"
            );
            assert_eq!(
                harness.emphasized_card(),
                Some((1, 0)),
                "the emphasis sits on stack b's first card (its own column)"
            );
            // And one more switcher round wraps back to stack a.
            harness.press(ActionId::NavPrevItem);
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                0,
                "the switcher walk wraps"
            );
            assert_eq!(harness.emphasized_card(), Some((0, 0)));

            // Back at the regular width, the full board is keyboard-alive.
            harness.driver.set_window_size(1000, 800);
            assert!(!harness.handle.window().get_compact_active());
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "the regular full-board walk resumes across columns"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// D12 — the task-detail back button: the click equivalent of
/// `nav.focus-board` (§7.1) returns from the task detail to the board.
#[test]
fn the_back_button_returns_to_the_board() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit never auto-arms; the editor arms via the
            // `task.title-edit` action.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.driver.click_by_label("Back to board");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the back button routed focus_board"
            );
            // Leaving mid-edit commits the buffer first (the D9
            // leaving rule — the back button is an ordinary leaving
            // effect, no special casing).
            let drained = harness.engine.drain();
            assert!(
                matches!(
                    drained.as_slice(),
                    [StateCommand::UpdateTask { changes, .. }]
                        if changes.title.as_deref() == Some("task-0")
                ),
                "the leaving commit reached the boundary, got {drained:?}"
            );
        },
    );
}

/// D13 — the right-click task context menu: opening focuses the
/// addressed card, the entries are the Task group's flat page (no
/// parents, no Quit), and invoking "Complete task" routes the exact
/// typed command for the card's task and closes the menu.
#[test]
fn the_context_menu_focuses_and_routes_the_card() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // The fixture's known card: stack a's second task.
            harness
                .handle
                .window()
                .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_menu_open(),
                "the context request opened the menu"
            );
            assert!(
                keyboard(&harness).get_menu_pointer_anchored(),
                "the menu is anchored at the pointer"
            );

            // The entries are the Task group's flat page: no parent
            // rows, no Quit, Complete task present.
            let entries = keyboard(&harness).get_menu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            assert!(rows.iter().all(|r| !r.submenu), "flat: no parents");
            assert!(rows.iter().all(|r| r.action != "app.exit"), "no Quit");
            assert!(
                rows.iter().any(|r| r.action == "task.complete"),
                "the card context offers Complete task"
            );

            // Invoke it: the exact typed command at the boundary, menu
            // closed.
            harness.driver.click_by_label("Complete task");
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: fixture.task1,
                done: true,
            }]);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the invocation closed the menu"
            );
        },
    );
}

/// R3-F1-half (D6) — the task card's done checkbox: a click toggles
/// the addressed card (`SetTaskDone` true, then false on the round
/// trip), the keyboard's focused-task route produces the same command
/// shape (click ≡ key), and the model round-trip flips
/// `TaskCardModel.done` on the next push — no local checked state
/// anywhere.
#[test]
fn the_card_checkbox_toggles_the_addressed_task() {
    let (fixture, state) = fixture::golden_state();
    // The first card of stack "a": the checkbox click addresses it
    // (find_by_label takes the tree's first match).
    let task0 = state
        .tasks
        .values()
        .find(|task| task.stack == fixture.stack_a && task.order == 0)
        .map(|task| task.id)
        .expect("the fixture's first task");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);

            // Click the first card's checkbox: the toggled-on command.
            harness.driver.click_by_label("Mark task done");
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: task0,
                done: true,
            }]);
            // The keyboard's focused-task route produces the same
            // command shape (click ≡ key): task.complete on the
            // focused card sends the same toggled-on SetTaskDone.
            harness.handle.window().invoke_board_task_clicked(0, 0);
            harness.press(ActionId::TaskComplete);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: task0,
                done: true,
            }]);

            // The engine applies both toggles (one flip): the model
            // round-trip lands done on the next push, the box offers
            // the undone verb, and the click toggles back off.
            harness.engine.settle(&harness.handle);
            let done_bit = harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .tasks
                .row_data(0)
                .expect("card")
                .done;
            assert!(done_bit, "the refresh push flipped the model bit");
            harness.driver.click_by_label("Mark task not done");
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: task0,
                done: false,
            }]);
        },
    );
}

/// R3-F6 (D4, probe P-B1) — right-clicking a stack's bare card
/// surface (an empty stack's body, the space below the last card)
/// opens the pointer-anchored stack context menu and selects the
/// clicked stack — the header seam's body twin. The probe drives a
/// real right-button press (below the last card of a task-less
/// stack), not the seam invocation.
#[test]
fn the_body_right_click_selects_and_opens_the_stack_menu() {
    let (fixture, mut state) = fixture::golden_state();
    // Stack "b" loses its task: its whole card body is bare surface.
    let stack_b = fixture.stack_b;
    state.tasks.retain(|_, task| task.stack != stack_b);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let header = harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("the stack-b header button is present");
            let pos = header.absolute_position();
            let size = header.size();
            // Below the header: the empty stack's bare body.
            #[allow(clippy::cast_precision_loss)]
            let bare_y = pos.y + size.height * 3.0;
            harness
                .driver
                .right_press_at(pos.x + size.width / 2.0, bare_y);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());
            assert!(
                keyboard(&harness).get_menu_pointer_anchored(),
                "the menu anchors at the pointer"
            );
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the right-click selected the clicked stack"
            );
        },
    );
}

/// R3-F10/11/41 (D5) — the empty-board page: the board bar renders on
/// a zero-stack board (its tab is addressable — the strongest
/// structural proof available; F11 pinned by addressing), the
/// New-stack CTA opens the same flow state the keyboard's route arms,
/// and the preset CTA drains the table's `CreateStack` commands in
/// table order with explicit 0..n orders (titles are domain payloads
/// — asserted as command payloads, not presentation text).
#[test]
fn the_empty_board_renders_bar_ctas_and_the_preset() {
    let state = fixture::empty_board_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);

            // The bar: the fresh board's one tab is addressable.
            assert!(
                harness.driver.find_by_label("board").is_some(),
                "the empty board's tab is addressable"
            );

            // The New-stack CTA arms the same flow state the `S`
            // binding does; Escape-like settle closes it.
            harness.driver.click_by_label("New stack");
            assert!(
                harness.handle.window().get_board_model().stack_create_open,
                "the CTA opens the stack-create flow"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);

            // The preset CTA: the table's stacks, explicit orders.
            harness.driver.click_by_label("Create Default Kanban Setup");
            let drained = harness.engine.drain();
            let mut created: Vec<(&str, i64)> = Vec::new();
            for command in &drained {
                if let StateCommand::CreateStack { title, order, .. } = command {
                    created.push((title.as_str(), *order));
                }
            }
            assert_eq!(
                created,
                vec![
                    ("Backlog", 0),
                    ("To Do", 1),
                    ("In Progress", 2),
                    ("Done", 3)
                ],
                "the preset's stacks, table order, explicit orders"
            );
        },
    );
}

/// R3-F12 (D3) — an Add Task ghost hover arms the generalized
/// tooltip: pending on enter, visible after the delay with a
/// non-empty description (content never asserted), dismissed on
/// leave. The regular leg's ghost is probed; the compact ghost shares
/// the wiring.
#[test]
fn an_add_task_ghost_hover_arms_the_tooltip() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let ghost = harness
                .driver
                .find_by_label("Add task")
                .expect("the add-task ghost is addressable");
            let pos = ghost.absolute_position();
            let size = ghost.size();
            harness
                .driver
                .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            assert!(
                tooltip_state(&harness).get_pending(),
                "the hover armed the delay"
            );
            harness.driver.advance_time(600);
            assert!(tooltip_state(&harness).get_visible(), "the delay fired");
            assert_ne!(
                tooltip_state(&harness).get_description(),
                "",
                "the description slot is populated"
            );
            harness.driver.hover_at(4.0, 4.0);
            assert!(!tooltip_state(&harness).get_pending());
            assert!(!tooltip_state(&harness).get_visible());
        },
    );
}

/// r4 D16/D17 (blanket rule a) — the task view's header row and
/// summary elements arm their tooltips on hover: the done checkbox,
/// the tracking button, and the summary value each arm with a
/// populated description and dismiss on leave. Label content is
/// never asserted.
#[test]
fn the_task_view_header_and_summary_elements_arm_their_tooltips() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

            for label in ["Mark task done", "Start tracking", "Tracked time total"] {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("{label} is addressable"));
                let pos = element.absolute_position();
                let size = element.size();
                harness
                    .driver
                    .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
                assert!(
                    tooltip_state(&harness).get_pending(),
                    "{label}'s hover armed the delay"
                );
                harness.driver.advance_time(600);
                assert!(tooltip_state(&harness).get_visible(), "{label} showed");
                assert_ne!(
                    tooltip_state(&harness).get_description(),
                    "",
                    "{label}'s description slot is populated"
                );
                harness.driver.hover_at(4.0, 4.0);
                assert!(!tooltip_state(&harness).get_pending());
                assert!(!tooltip_state(&harness).get_visible());
            }
        },
    );
}

/// r4 D21 (F26, acceptance e) — the hover sweep over the chunk's new
/// interactive elements: the four closed cells and the preset chips
/// arm the generalized tooltip (label + description, hint keys where
/// bound), hover shows it after the delay, and moving away hides it.
/// Historical note: the field sites were tip-less when this journey
/// was written (the planned `TextField` tip arm died at stage 3 — the
/// inner `TextInput` accepts its own hover moves, so a sibling
/// `TouchArea` never hovers and one above breaks caret handling).
/// The r3-3.1 chunk landed them via the sibling-cell `TippedField`
/// pair instead; see
/// `.artifacts/plans/2026-11-03-taskview-field-tooltips-plan.md` and
/// the origin report §5 for the history — the field sites' tips are
/// now pinned by `every_task_view_field_editor_carries_its_info_tip`
/// below.
#[test]
fn every_task_view_cell_and_chip_carries_its_info_tip() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            harness.driver.advance_time(0);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

            // The structural inventory: one distinct tip spot per cell
            // trigger and four chip triggers (the settings sweep's
            // distinct-geometry mechanism — the id query reports the
            // touch subtree too).
            let distinct_spots = |harness: &Harness, id: &str| -> usize {
                let mut spots: Vec<(f32, f32)> = harness
                    .driver
                    .find_all_by_id(id)
                    .iter()
                    .map(|tip| {
                        let p = tip.absolute_position();
                        (p.x, p.y)
                    })
                    .collect();
                spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
                spots.dedup();
                spots.len()
            };
            for id in [
                "TaskView::title-touch",
                "TaskView::due-touch",
                "TaskView::scheduled-touch",
                "TaskView::description-touch",
            ] {
                assert_eq!(
                    distinct_spots(&harness, id),
                    1,
                    "exactly one tip trigger at {id}"
                );
            }
            assert_eq!(
                distinct_spots(&harness, "PresetChip::touch"),
                4,
                "each preset chip carries its own tip trigger"
            );

            // The functional legs: hover arms the delay and shows the
            // panel with a populated description; moving away hides.
            let cell = |harness: &Harness, label: &str| {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("{label} is addressable"));
                let pos = element.absolute_position();
                let size = element.size();
                (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
            };
            for label in ["Edit title", "Edit due date", "5m"] {
                let (x, y) = cell(&harness, label);
                harness.driver.hover_at(x, y);
                assert!(
                    tooltip_state(&harness).get_pending(),
                    "{label}'s hover armed the delay"
                );
                harness.driver.advance_time(600);
                assert!(tooltip_state(&harness).get_visible(), "{label} showed");
                assert_ne!(
                    tooltip_state(&harness).get_description(),
                    "",
                    "{label}'s description slot is populated"
                );
                harness.driver.hover_at(4.0, 4.0);
                harness.driver.advance_time(0);
                assert!(!tooltip_state(&harness).get_pending());
                assert!(!tooltip_state(&harness).get_visible());
            }
        },
    );
}

/// Chunk r3-3.1 — the five field sites (title/description editors,
/// due/scheduled editors, the custom reminder offset) carry their
/// `TippedField` info tips (issue #590; plan
/// `.artifacts/plans/2026-11-03-taskview-field-tooltips-plan.md` §3).
/// The custom-offset square is always mounted (exactly one in the
/// closed view); each open editor adds exactly one distinct square —
/// never zero, never duplicated. Per site the square trails the
/// field as a sibling layout cell (tip.x ≥ field.x + field.width —
/// the mechanism pin: a square inside or over the field's rect would
/// reintroduce the r4 caret infeasibility; see the origin report
/// `2026-11-03-ui-r4-taskview-focus-stage3-report.md` §5). The
/// functional legs ride the settings sweep's grammar: hover arms the
/// delay, the mock clock's 600 ms shows the panel with a populated
/// description slot (content never asserted), moving away hides —
/// except at the two date-editor sites, where the test currently
/// pins the picker overlay's hover precedence instead (see the leg
/// comment below for the mechanism and the tripwire).
#[test]
#[allow(clippy::too_many_lines)] // the five site legs are the contract's length
fn every_task_view_field_editor_carries_its_info_tip() {
    let sites: [(&str, &str); 5] = [
        ("Edit title", "Task title"),
        ("Edit description", "Description"),
        ("Edit due date", "Due date entry"),
        ("Edit start date", "Start date entry"),
        // The offset field is always mounted; no cell opens it.
        ("", "Custom reminder offset"),
    ];

    // The closed view: exactly the always-mounted offset square.
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            harness.driver.advance_time(0);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            // The offset square rides TippedField; the description's
            // square is the manual edit-mount composition's own
            // (round-5 chunk 2, D-MLE-8) — both count.
            let mut spots: Vec<(f32, f32)> = harness
                .driver
                .find_all_by_id("TippedField::help-tip")
                .iter()
                .chain(
                    harness
                        .driver
                        .find_all_by_id("TaskView::description-help-tip")
                        .iter(),
                )
                .map(|tip| {
                    let p = tip.absolute_position();
                    (p.x, p.y)
                })
                .collect();
            spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
            spots.dedup();
            assert_eq!(
                spots.len(),
                1,
                "the closed view carries only the offset square"
            );
        },
    );

    for (cell, field_label) in sites {
        let (_fixture, state) = fixture::golden_state();
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness = Harness::mount(SlintDriver::default(), state);
                harness.driver.set_window_size(1000, 900);
                harness.press(ActionId::NavNextItem);
                harness.press(ActionId::TaskEdit);
                harness.driver.advance_time(0);
                if !cell.is_empty() {
                    harness.driver.click_by_label(cell);
                    harness.driver.advance_time(0);
                }
                // The offset square rides TippedField; the
                // description site's square is the manual edit-mount
                // composition's own (round-5 chunk 2, D-MLE-8).
                let mut tips = harness.driver.find_all_by_id("TippedField::help-tip");
                tips.extend(
                    harness
                        .driver
                        .find_all_by_id("TaskView::description-help-tip"),
                );
                assert_eq!(
                    tips.len(),
                    if cell.is_empty() { 1 } else { 2 },
                    "{field_label}: the open site adds exactly one square to the offset's"
                );

                // The mechanism pin: the site's square pairs with the
                // field it annotates — a labelled element sharing the
                // square's row that the square trails (tip.x ≥ field
                // right edge — the square never sits inside or over
                // the field's rect, the caret-guard geometry). The
                // label lookup is a set, not a single: a section
                // caption can share the field's label (the
                // description's caption does), and only the real pair
                // satisfies the relation.
                let fields = harness.driver.find_all_by_label(field_label);
                let mut pairs: Vec<_> = Vec::new();
                for tip in &tips {
                    let tpos = tip.absolute_position();
                    let tsize = tip.size();
                    for field in &fields {
                        let fpos = field.absolute_position();
                        let fsize = field.size();
                        let same_row =
                            tpos.y + tsize.height > fpos.y && tpos.y < fpos.y + fsize.height;
                        if same_row && tpos.x >= fpos.x + fsize.width {
                            pairs.push((tip, field));
                        }
                    }
                }
                assert_eq!(
                    pairs.len(),
                    1,
                    "{field_label}: exactly one square trails the field in its row"
                );
                let (tip, _) = pairs.remove(0);

                // The functional legs. The three non-picker sites ride
                // the settings sweep's grammar: hover arms, the delay
                // shows, the description slot is populated, away
                // hides. The two date-editor sites currently pin a
                // limitation instead: the adapter's typing-first
                // default-open mounts the window-level picker overlay
                // for the editor's whole life, and its dismissal
                // layer (r4 D2) sits on top of everything — any click
                // settles the editor (r4 D24), so no click path
                // hovers the tip while the editor is open. The help
                // still reaches AT users via the field's
                // accessible-description; reworking the overlay is
                // r4-surface territory, not this chunk's. The pin
                // asserts the precedence as the live contract — a
                // future change that frees the hover flips the first
                // assertion red, which is the intended tripwire.
                let picker_leg = cell == "Edit due date" || cell == "Edit start date";
                let (x, y) = support::driver::centre(tip);
                harness.driver.hover_at(x, y);
                if picker_leg {
                    assert!(
                        !tooltip_state(&harness).get_pending(),
                        "{field_label}: the picker overlay's dismissal layer outranks the tip hover"
                    );
                    harness.driver.hover_at(4.0, 4.0);
                } else {
                    assert!(
                        tooltip_state(&harness).get_pending(),
                        "{field_label}'s hover armed the delay"
                    );
                    harness.driver.advance_time(600);
                    assert!(
                        tooltip_state(&harness).get_visible(),
                        "{field_label} showed"
                    );
                    assert_ne!(
                        tooltip_state(&harness).get_description(),
                        "",
                        "{field_label}'s description slot is populated"
                    );
                    harness.driver.hover_at(4.0, 4.0);
                    harness.driver.advance_time(0);
                    assert!(!tooltip_state(&harness).get_pending());
                    assert!(!tooltip_state(&harness).get_visible());
                }
            },
        );
    }
}

/// Chunk r3-3.1 — the caret-guard companion: hovering the offset
/// field's tip square mid-edit leaves the caret and the keyboard
/// surface intact (issue #590's original infeasibility must not
/// re-enter). The typed buffer survives the hover round-trip and
/// commits whole; the insert-mask fact still reads true (focus never
/// left the field). Typed outcomes + structural facts only.
#[test]
fn a_field_tip_hover_leaves_the_caret_intact() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            harness.driver.advance_time(0);
            click_field_centre(&mut harness, "Custom reminder offset");
            harness.type_text("45");

            let tip = harness
                .driver
                .find_all_by_id("TippedField::help-tip")
                .into_iter()
                .next()
                .expect("the offset square is mounted");
            let (x, y) = support::driver::centre(&tip);
            harness.driver.hover_at(x, y);
            assert!(
                tooltip_state(&harness).get_pending(),
                "the hover armed the delay"
            );
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip shows mid-edit"
            );

            harness.type_text("m");
            harness.stroke(parse("Return").expect("return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::AddReminderRule { task, offset, .. }] => {
                    assert_eq!(*task, task0, "the subject's rule");
                    assert_eq!(
                        *offset,
                        taskboard_domain::ReminderOffset::before(
                            TimeDelta::try_minutes(-45).expect("-45m")
                        )
                        .expect("legal offset")
                    );
                }
                other => panic!("the full typed content committed, got {other:?}"),
            }
            assert!(
                keyboard(&harness).get_vi_insert(),
                "the field's focus masks the chain"
            );
        },
    );
}

/// D22 — the compact add-stack affordance: at the compact width the
/// relocated trailing button opens the same stack-create flow (the
/// click equivalent of stack.create, §7.1).
#[test]
fn the_compact_add_stack_affordance_opens_the_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(600, 800);
            assert!(harness.handle.window().get_compact_active());
            harness.driver.click_by_label("New stack");
            assert!(
                harness.handle.window().get_board_model().stack_create_open,
                "the compact button opens the stack-create flow"
            );
        },
    );
}

/// G8/G9 — the stack-create flow has a visible surface, and the
/// on-board New-stack button is the click equivalent of the `S` key
/// (usability pass D7, dispatch issue 6): both routes open the same
/// flow state and land the same typed command at the boundary.
#[test]
fn stack_create_has_a_visible_click_equal_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let create_open =
                |harness: &Harness| harness.handle.window().get_board_model().stack_create_open;

            // (a) The keyboard route: `S` opens the flow's field.
            harness.press(ActionId::StackCreate);
            assert!(create_open(&harness), "the flow's field is visible");
            harness.assert_commands(&[]);
            harness.type_text("s");
            harness.stroke(parse("Return").expect("Return"));
            assert!(!create_open(&harness), "the commit closed the flow");
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateStack { title, order, .. }] => {
                    assert_eq!(title.as_str(), "s");
                    assert_eq!(*order, 2, "append after the fixture's two stacks");
                }
                other => panic!("the create-stack command reached the boundary, got {other:?}"),
            }

            // (b) The click route: the on-board button opens the same
            // flow state and the same commit path fires — click ≡ key
            // at the observed boundary (§7.1).
            harness.driver.click_by_label("New stack");
            assert!(create_open(&harness), "the button opens the same flow");
            harness.type_text("s");
            harness.stroke(parse("Return").expect("Return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateStack { title, order, .. }] => {
                    assert_eq!(title.as_str(), "s");
                    assert_eq!(*order, 2, "the same command shape");
                }
                other => panic!("the click route's command reached the boundary, got {other:?}"),
            }
        },
    );
}

/// R4 D6 (dispatch acceptance (a)/(e)) — a divider between two of
/// four columns redistributes space between exactly those two: every
/// non-adjacent column's edge is bit-stable while the dragged boundary
/// moves.
#[test]
fn divider_drag_moves_exactly_two_neighbouring_columns() {
    let (stacks, state) = fixture::golden_state_with_stacks(4);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();
            // A deterministic row: 1000 − the chrome (2·space-3 side
            // padding + four space-3 gaps + the 40 px affordance) =
            // 888 rendered px, four equal shares of 222 at scale 1.
            harness.driver.set_window_size(1000, 800);
            for column in 0..4 {
                assert_eq!(
                    harness.column_width_px(column),
                    222.0,
                    "the four columns seed at equal shares"
                );
            }

            // Drag the middle boundary (between columns 1 and 2) by
            // +100: column 1 takes +δ, column 2 −δ, columns 0 and 3
            // never move (the non-adjacent isolation, four columns).
            window.invoke_board_stack_width_press(1);
            window.invoke_board_stack_width_drag(1, 100.0);
            window.invoke_board_stack_width_release(1);
            assert_eq!(
                harness.column_width_px(0),
                222.0,
                "the left far column is bit-stable"
            );
            assert_eq!(
                harness.column_width_px(3),
                222.0,
                "the right far column is bit-stable"
            );
            assert_eq!(harness.column_width_px(1), 322.0);
            assert_eq!(harness.column_width_px(2), 122.0);

            // The released row re-resolves stably: a fresh refresh
            // does not move any edge.
            harness.handle.refresh();
            assert_eq!(harness.column_width_px(0), 222.0);
            assert_eq!(harness.column_width_px(1), 322.0);
            assert_eq!(harness.column_width_px(2), 122.0);
            assert_eq!(harness.column_width_px(3), 222.0);
            let _ = stacks;
        },
    );
}

/// R4 D7/decision 6 (dispatch acceptance (c), the add half) — a board
/// with partial stored widths rebalances when a stack joins: the
/// absent entries seed at the equal share and the row re-fits
/// proportionally over all columns, without any gesture.
#[test]
fn stored_widths_rebalance_when_a_stack_joins() {
    let (stacks, state) = fixture::golden_state_with_stacks(3);
    let board = *state.boards.keys().next().expect("fixture board");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // The device-local overlay carries only the first stack's
            // width — the other two are absent entries (a downlevel
            // config, or a stack created after the last snapshot).
            let settings = taskboard_ui_slint::UiSettings {
                stack_widths: std::collections::BTreeMap::from([(
                    board.as_uuid(),
                    std::collections::BTreeMap::from([(stacks[0].as_uuid(), 500u32)]),
                )]),
                ..taskboard_ui_slint::UiSettings::default()
            };
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.driver.set_window_size(1000, 800);

            // The row (900 rendered px) re-fits over the stored basis
            // and the two equal-share seeds: every column positive, at
            // or above its content minimum, summing to the row; the
            // stored column narrows and the newcomers land near the
            // average share (± the fit's rounding).
            let widths: Vec<f32> = (0..3).map(|c| harness.column_width_px(c)).collect();
            let total: f32 = widths.iter().sum();
            for width in &widths {
                assert!(*width >= 120.0, "every column ≥ its minimum, got {width}");
            }
            assert!((total - 900.0).abs() <= 2.0, "the row fills: {total}");
            assert!(widths[0] < 500.0, "the stored basis gave up slack");
            assert!(
                (widths[1] - widths[2]).abs() <= 2.0,
                "the two absent-seeded columns land together"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "a passive rebalance never persists"
            );
        },
    );
}

/// Round-1 chunk 2 (D18, T-10), adapted to the r4 pair model (D8) —
/// the keyboard resize journey: wider steps the focused column against
/// its neighbour (which seesaws), narrowing clamps at the content
/// minimum with the neighbour absorbing the rest, and every step's
/// full-snapshot request reaches the lane typed (exactly the
/// `StackWidthsChanged` payloads; the materialized overlay persists
/// both columns — the lane's full-snapshot contract).
#[test]
fn keyboard_resize_steps_clamps_and_requests_persistence() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // A deterministic row: 1000 − the chrome (2·space-3 side
            // padding + two space-3 gaps + the 40 px affordance) = 912
            // rendered px, two equal shares of 456 at scale 1.
            harness.driver.set_window_size(1000, 800);
            assert_eq!(harness.column_width_px(0), 456.0, "equal-share seed");
            assert_eq!(harness.column_width_px(1), 456.0, "equal-share seed");

            // One full-snapshot request per key press, drained per
            // step; the materialized overlay carries both columns, so
            // the matcher is order-insensitive (BTreeMap key order).
            let expect_snapshot =
                |harness: &Harness, expected: [(taskboard_domain::StackId, u32); 2]| {
                    let requests = harness.drain_requests();
                    assert_eq!(requests.len(), 1, "one request per key press");
                    let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                        board: payload_board,
                        widths,
                    } = &requests[0]
                    else {
                        panic!("the typed snapshot reached the lane");
                    };
                    assert_eq!(payload_board, &board);
                    let mut got = widths.clone();
                    got.sort_by_key(|(stack, _)| *stack);
                    let mut want = expected.to_vec();
                    want.sort_by_key(|(stack, _)| *stack);
                    assert_eq!(got, want, "the typed snapshot carries the row");
                };

            harness.press(ActionId::StackWider);
            assert_eq!(
                harness.column_width_px(0),
                488.0,
                "the first step takes 32 from the neighbour (scale 1)"
            );
            assert_eq!(harness.column_width_px(1), 424.0, "the neighbour seesaws");
            expect_snapshot(&harness, [(fixture.stack_a, 488), (fixture.stack_b, 424)]);

            harness.press(ActionId::StackNarrower);
            assert_eq!(harness.column_width_px(0), 456.0, "back to the share");
            assert_eq!(harness.column_width_px(1), 456.0);
            expect_snapshot(&harness, [(fixture.stack_a, 456), (fixture.stack_b, 456)]);

            // Narrowing clamps at the content minimum (120 for the
            // short fixture titles) and keeps reporting the clamped
            // snapshot; the neighbour absorbs the rest of the row.
            for step in 1..=12u32 {
                harness.press(ActionId::StackNarrower);
                let width = 456u32
                    .saturating_sub(32 * step)
                    .max(taskboard_ui_slint::widths::STACK_WIDTH_READABILITY_MIN_PX);
                // px values, far below f32's mantissa reach.
                #[allow(clippy::cast_precision_loss)]
                let expected_px = width as f32;
                assert_eq!(harness.column_width_px(0), expected_px);
                #[allow(clippy::cast_precision_loss)]
                let neighbour = (912 - width) as f32;
                assert_eq!(harness.column_width_px(1), neighbour);
                expect_snapshot(
                    &harness,
                    [(fixture.stack_a, width), (fixture.stack_b, 912 - width)],
                );
            }
        },
    );
}
/// Round-1 chunk 2 (D17, T-11), adapted to the r4 divider model —
/// the mouse drag journey: press materializes the row and captures the
/// boundary, drag re-renders live (the pair at the boundary moves,
/// clamped at both content minimums), a stale column index is
/// absorbed, and exactly one request lands on release carrying both
/// columns.
#[test]
fn drag_seam_resizes_live_and_persists_on_release() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();
            // A deterministic row: 912 rendered px, two shares of 456.
            harness.driver.set_window_size(1000, 800);

            window.invoke_board_stack_width_press(0);
            window.invoke_board_stack_width_drag(0, -50.0);
            assert_eq!(
                harness.column_width_px(0),
                406.0,
                "the left column re-rendered live at base + delta"
            );
            assert_eq!(
                harness.column_width_px(1),
                506.0,
                "the right neighbour takes exactly −δ"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "drag ticks never persist (release only)"
            );

            // A stale index (tables moved between press and tick) is
            // absorbed silently.
            window.invoke_board_stack_width_drag(9, 500.0);
            assert_eq!(harness.column_width_px(0), 406.0, "stale tick ignored");

            window.invoke_board_stack_width_release(0);
            let requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one request on release");
            let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                board: payload_board,
                widths,
            } = &requests[0]
            else {
                panic!("the typed snapshot reached the lane");
            };
            assert_eq!(payload_board, &board);
            let mut got = widths.clone();
            got.sort_by_key(|(stack, _)| *stack);
            assert_eq!(
                got,
                vec![(fixture.stack_a, 406), (fixture.stack_b, 506)],
                "the release snapshot carries both boundary columns"
            );

            // Release without a press is inert (no request).
            window.invoke_board_stack_width_release(0);
            assert!(harness.drain_requests().is_empty());
        },
    );
}

/// Round-1 chunk 2 (D16, T-12), adapted to the r4 explicit-width row
/// (chunk-2 plan §5) — a never-resized board pushes positive
/// equal-share widths on every column (the rendered image of the old
/// stretch-equal distribution; the −1.0 sentinel is gone) and
/// refreshes alone never emit a persistence request.
#[test]
fn never_resized_board_pushes_positive_widths_and_no_requests() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // A deterministic row: 912 rendered px over two columns.
            harness.driver.set_window_size(1000, 800);
            let board_model = harness.handle.window().get_board_model();
            let count = board_model.stacks.row_count();
            assert!(count >= 2);
            for column in 0..count {
                let width = harness.column_width_px(column);
                assert!(width > 0.0, "column {column} pushes a positive width");
                assert_eq!(
                    width, 456.0,
                    "every column carries its equal share of the row"
                );
            }
            harness.handle.refresh();
            harness.handle.refresh();
            assert!(
                harness.drain_requests().is_empty(),
                "refreshes alone never persist"
            );
        },
    );
}

/// R4 D5/decision 6 (dispatch acceptance (c)) — a window resize
/// distributes the delta proportionally over the stored bases: drag
/// column 0 down to its content minimum, enlarge the window, and every
/// column grows; the resolved row sums to the window minus the chrome
/// (2·space-3 side padding + two space-3 gaps + the 40 px affordance
/// at scale 1 = 88 px — the corrected arithmetic the code now
/// matches).
#[test]
fn window_resize_distributes_delta_proportionally() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();
            harness.driver.set_window_size(1000, 800);

            // Drag column 0 to its content minimum (the divider's
            // press materializes; the delta clamps at the pair's
            // minimums).
            window.invoke_board_stack_width_press(0);
            window.invoke_board_stack_width_drag(0, -500.0);
            window.invoke_board_stack_width_release(0);
            assert_eq!(
                harness.column_width_px(0),
                120.0,
                "the dragged column clamps at its content minimum"
            );
            assert_eq!(harness.column_width_px(1), 792.0, "the row still sums");

            // A wider window fills: the delta distributes
            // proportionally over the bases (120/792 of 912).
            harness.driver.set_window_size(1500, 900);
            let w0 = harness.column_width_px(0);
            let w1 = harness.column_width_px(1);
            assert!(w0 > 120.0, "the min-basis column grows too, got {w0}");
            assert!(w1 > 792.0, "the wide column keeps its lead, got {w1}");
            // Σ widths == 1500 − 88 chrome (±1 px of physical
            // rounding).
            let total = w0 + w1;
            assert!(
                (total - 1412.0).abs() <= 1.0,
                "the resolved row fills the window: {total} vs 1412"
            );
        },
    );
}

/// R2 D3 (T-5) — the header is one locked control row regardless of
/// content: a 0-task stack's header measures exactly the D20b constant,
/// identical to its populated neighbour's.
#[test]
fn stack_headers_are_one_control_row_regardless_of_content() {
    let (fixture, mut state) = fixture::golden_state();
    state.tasks.retain(|_, task| task.stack != fixture.stack_b);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(harness.board_shape(), vec![2, 0], "stack b is empty");
            #[allow(clippy::cast_precision_loss)]
            let control_row = (32.0 + 2.0 * 4.0) * 1.0; // space-6 + 2·space-1, scale 1
            for title in ["a", "b"] {
                let header = harness
                    .driver
                    .find_all_by_label(title)
                    .into_iter()
                    .find(support::driver::Element::is_accessible_button)
                    .unwrap_or_else(|| panic!("stack {title}'s header is addressable"));
                let height = header.size().height;
                assert_eq!(
                    height, control_row,
                    "stack {title}'s header is one control row"
                );
            }
        },
    );
}

/// R2 D4 (T-10) — the model pushes the *true* selection: a fresh mount
/// carries −1 (no explicit selection), and a header click moves it to
/// the clicked stack's index.
#[test]
fn mount_pushes_no_selection_and_a_header_click_moves_it() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                -1,
                "a fresh mount pushes no selection"
            );
            let click_header = |harness: &Harness, title: &str| {
                harness
                    .driver
                    .find_all_by_label(title)
                    .into_iter()
                    .find(support::driver::Element::is_accessible_button)
                    .unwrap_or_else(|| panic!("stack {title}'s header is addressable"))
                    .invoke_default_action();
            };
            click_header(&harness, "a");
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                0,
                "the header click selected stack a"
            );
            click_header(&harness, "b");
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the header click moved the selection to stack b"
            );
        },
    );
}

/// R2 D5 (T-6), amended by r3 D2 — the add-stack plus glyph is square
/// at the `IconButton` icon-box footprint (space-6 − 2·space-2, the
/// halving the review asked for): the renderer's bounding box of the
/// painted glyph inside the trailing add-column strip measures that
/// square (element ids cannot address the glyph directly — unnamed
/// decoration items merge in the item tree — so this pin reads the
/// pixels, golden-adjacent).
#[test]
fn the_add_stack_glyph_is_square() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let header_b = harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack b's header is addressable");
            // The add-col strip starts one space-3 gap after the last
            // column and is space-6 + space-2 wide (scale 1).
            let strip_x0 = header_b.absolute_position().x + header_b.size().width + 12.0;

            let image = harness.driver.snapshot();
            let (mut x_min, mut x_max, mut y_min, mut y_max) = (u32::MAX, 0u32, u32::MAX, 0u32);
            // Scan only the columns row's band: the toolbar and
            // status bar also paint at the window's right edge.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            // px coordinates at scale 1
            let row_y0 = header_b.absolute_position().y as u32;
            let row_y1 = (row_y0 + 600).min(image.height);
            for y in row_y0..row_y1 {
                // 995: inside the board region's focus ring (whose
                // right border also paints at the window edge).
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (scan_x0, scan_x1) = (strip_x0 as u32, strip_x0 as u32 + 40);
                for x in scan_x0..scan_x1.min(995) {
                    let i = (y as usize * image.width as usize + x as usize) * 4;
                    let (r, g, b) = (
                        i64::from(image.data[i]),
                        i64::from(image.data[i + 1]),
                        i64::from(image.data[i + 2]),
                    );
                    let background =
                        (r - 20).abs() <= 12 && (g - 22).abs() <= 12 && (b - 28).abs() <= 12;
                    if !background {
                        x_min = x_min.min(x);
                        x_max = x_max.max(x);
                        y_min = y_min.min(y);
                        y_max = y_max.max(y);
                    }
                }
            }
            assert!(x_max > x_min, "the glyph paints inside the strip");
            let width = x_max - x_min + 1;
            let height = y_max - y_min + 1;
            assert!(
                (12.0..=20.0).contains(&f64::from(width))
                    && (12.0..=20.0).contains(&f64::from(height)),
                "the glyph bounding box is the icon-box square, got {width}x{height}"
            );
        },
    );
}
/// R2 D6 (T-7) — the add-stack button carries the stack.create binding
/// hint: non-empty under the standard preset, and (since round 2 fills
/// the D15 gap with `Ctrl+c s`) non-empty under emacs too. Structural
/// presence only — the hint's content is never asserted.
#[test]
fn the_add_stack_button_carries_the_stack_create_hint() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            assert!(
                harness.handle.window().get_stack_create_hint().row_count() > 0,
                "the standard preset binds stack.create"
            );

            let emacs = taskboard_ui_slint::keymap::Keymap::from_preset(
                taskboard_ui_slint::keymap::Preset::Emacs,
            );
            let (_fixture, state) = fixture::golden_state();
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: emacs,
                    keymap_preset: taskboard_ui_slint::keymap::Preset::Emacs,
                    ..UiSettings::default()
                },
            );
            assert!(
                harness.handle.window().get_stack_create_hint().row_count() > 0,
                "emacs binds stack.create (Ctrl+c s; the D15 gap closed)"
            );
        },
    );
}

/// R2 D7 (T-8) — compact mounts exactly one add-stack entry, inline:
/// at the smallest viewport the regular full-height affordance is
/// absent and a single "New stack" accessible entry exists (the
/// switcher's trailing button); at a regular width the affordance is
/// present.
#[test]
fn compact_mounts_exactly_one_add_stack_entry() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(600, 800);
            assert!(harness.handle.window().get_compact_active());
            assert!(
                harness
                    .driver
                    .find_all_by_id("BoardLayout::add-col-hit")
                    .is_empty(),
                "the regular add-column unmounts in compact"
            );
            assert_eq!(
                harness.driver.find_all_by_label("New stack").len(),
                1,
                "exactly one compact add-stack entry"
            );

            harness.driver.set_window_size(1000, 800);
            assert!(!harness.handle.window().get_compact_active());
            assert!(
                !harness
                    .driver
                    .find_all_by_id("BoardLayout::add-col-hit")
                    .is_empty(),
                "the regular add-column is mounted"
            );
        },
    );
}

/// R2 D9 (§2.7 test 3) — the model identity pin: consecutive
/// unchanged refreshes leave the board's model handle untouched (the
/// repetition-probe's no-op path at the transport level), and a title
/// edit still lands through a row update with the handle stable
/// across that refresh (F8/F11's TouchArea-survival mechanism).
#[test]
fn consecutive_refreshes_keep_the_board_model_identity() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let before = harness.handle.window().get_board_model().stacks;
            harness.handle.refresh();
            harness.handle.refresh();
            assert_eq!(
                before,
                harness.handle.window().get_board_model().stacks,
                "unchanged refreshes keep the model handle"
            );
            // One changed refresh (a retitled card, applied by the
            // fake engine's settle + the commit's refresh) keeps the
            // handle and lands the new title through a row update.
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit opens the detail display and never auto-arms;
            // the title editor arms via the `task.title-edit` action.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);
            harness.type_text("z");
            harness.stroke(parse("Return").expect("Return"));
            harness.engine.settle(&harness.handle);
            let after = harness.handle.window().get_board_model().stacks;
            assert_eq!(before, after, "the changed refresh keeps the model handle");
            let title = after
                .row_data(0)
                .expect("column")
                .tasks
                .row_data(0)
                .expect("card")
                .title;
            assert_eq!(title, "z", "the edit landed through the row update");
        },
    );
}

/// R2 D8 (T-9) — the per-column Add Task ghost is the click
/// equivalent of `task.create` anchored to that stack: clicking it
/// selects the stack and opens the create row there; typing + Return
/// lands the exact typed command the keyboard's action produces from
/// the same selection state. Empty stacks are anchored too; the
/// compact ghost addresses the effective-selected stack.
#[test]
fn the_add_task_ghost_is_the_click_equivalent_of_task_create() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // (a) The click route: stack b's ghost (document order —
            // a's ghost is first).
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 2, "one ghost per stack");
            ghosts[1].invoke_default_action();
            assert!(
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(1)
                    .expect("column")
                    .create_open,
                "the create row opened in stack b"
            );
            // Recorded decision (2026-10-23-ui-r2-interaction-repairs
            // plan §2.6 residual + §2.7): under the D9 row-diff
            // transport the create field mounts at the next frame's
            // render visit instead of the old wholesale model reset,
            // and the testing backend has no frame pass between this
            // callback and a synthesized keystroke — so the headless
            // journey commits through the same field seam the Return
            // key reaches. The click-opens-the-flow half stays
            // asserted above; the mouse-open→type→Return feel is the
            // Tier-4 manual checklist item.
            harness
                .handle
                .window()
                .invoke_board_create_committed(1, "n".into());
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 1,
            }]);

            // (b) The keyboard route from the same selection state
            // (fresh mount): select stack b via its header, then
            // task.create — the identical command shape.
            let (fixture, state) = fixture::journey_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack b's header")
                .invoke_default_action();
            harness.press(ActionId::TaskCreate);
            harness.type_text("n");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 1,
            }]);

            // (c) The empty-stack variant: stack b has no tasks and
            // its ghost still anchors the flow there.
            let (fixture, mut state) = fixture::golden_state();
            state.tasks.retain(|_, task| task.stack != fixture.stack_b);
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 2);
            ghosts[1].invoke_default_action();
            // The D9 frame-step residual, as in (a) (recorded
            // decision, 2026-10-23-ui-r2-interaction-repairs plan
            // §2.6): commit through the field seam.
            harness
                .handle
                .window()
                .invoke_board_create_committed(1, "n".into());
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_b,
                order: 0,
            }]);

            // (d) The compact variant: the single ghost addresses the
            // effective-selected stack (a at mount).
            let (fixture, state) = fixture::journey_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(600, 800);
            let ghosts = harness.driver.find_all_by_label("Add task");
            assert_eq!(ghosts.len(), 1, "the compact column's ghost");
            ghosts[0].invoke_default_action();
            harness.type_text("n");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::CreateTask {
                title: "n".into(),
                stack: fixture.stack_a,
                order: 2,
            }]);
        },
    );
}

/// DDF-5 — the anchor card is hidden from the board's list surfaces:
/// board card counts (and, on the same single pass, the stack-detail
/// cards) exclude the exact-title anchor; a byte-level near-miss stays
/// visible.
#[test]
fn the_anchor_card_is_hidden_from_the_board() {
    let (fixture, mut state) = fixture::golden_state();
    // Clone stack a's first task, retitle it to the exact anchor title,
    // and plant it back into stack a.
    let stack_a = fixture.stack_a;
    let source = state
        .tasks_in_stack(stack_a)
        .first()
        .copied()
        .expect("fixture task")
        .clone();
    let mut anchor = source.clone();
    anchor.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
    state.tasks.insert(anchor.id, anchor);
    // The near-miss rides stack a too: same words, trailing space.
    let mut near_miss = source;
    near_miss.id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7());
    near_miss.title = format!("{} ", taskboard_domain::ANCHOR_CARD_TITLE);
    state.tasks.insert(near_miss.id, near_miss);

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(
                harness.board_shape(),
                vec![3, 1],
                "the anchor is filtered; the near-miss and the rest stay"
            );
            let cards = harness.handle.window().get_board_model().stacks;
            let first = cards.row_data(0).expect("stack a");
            let titles: Vec<String> = (0..first.tasks.row_count())
                .map(|row| first.tasks.row_data(row).expect("card").title.to_string())
                .collect();
            assert!(
                !titles
                    .iter()
                    .any(|title| title == taskboard_domain::ANCHOR_CARD_TITLE),
                "no anchor card in the model"
            );
            assert!(
                titles
                    .iter()
                    .any(|title| { title == &format!("{} ", taskboard_domain::ANCHOR_CARD_TITLE) }),
                "the near-miss stays visible (byte-exact filtering)"
            );

            // The stack-detail surface (the second half of the same
            // pass): navigate there and count again.
            harness.press(ActionId::NavNextView);
            let stack_model = harness.handle.window().get_stack_model();
            assert_eq!(
                stack_model.tasks.row_count(),
                3,
                "stack detail filters the anchor in the same pass"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// D-S2 — the loud fence on the status bar: a selected-board row
/// carrying `Unreadable` evidence renders a non-empty sidecar warning
/// (typed model read — the caption wording is data, never asserted);
/// a healthy row renders nothing.
#[test]
fn the_sidecar_warning_renders_only_when_the_health_says_so() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state.sync.get_mut(board).expect("sync row").sidecar =
        taskboard_domain::SidecarHealth::Unreadable(
            taskboard_domain::AppDataUnreadable::FormatTooNew { format: 2 },
        );

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let sync = harness.handle.window().get_sync_model();
            assert_ne!(
                sync.sidecar_warning, "",
                "the unreadable evidence renders the warning"
            );
            harness.assert_commands(&[]);
        },
    );

    // The healthy baseline: the same fixture without the evidence —
    // absent evidence is the ruled silence.
    let (_fixture, healthy) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), healthy);
            let sync = harness.handle.window().get_sync_model();
            assert_eq!(sync.sidecar_warning, "", "healthy is silent");
            harness.assert_commands(&[]);
        },
    );
}

// ---------------------------------------------------------------------
// Round-1 chunk 3 (D5/D28b): the settings surface
// ---------------------------------------------------------------------

/// J1 — `Ctrl+,` from the board opens Settings; Escape closes onto the
/// board; the back affordance closes too. The opener is a toggle (D6.3)
/// — a second dispatch closes.
#[test]
fn settings_open_closes_and_toggles() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);

            // The opener action through the same resolve-and-inject
            // path every journey uses (Ctrl+, in the standard preset).
            harness.press(ActionId::AppSettings);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Settings,
                "the opener opened the settings surface"
            );
            assert_eq!(
                harness.chain(),
                vec![ContextId::Settings, ContextId::ViNormal, ContextId::Global],
                "the settings context heads the chain"
            );

            // Escape (the Settings-context binding) closes onto the
            // return view.
            harness.stroke(parse("Escape").expect("Escape"));
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "Escape restored the exact previous view"
            );

            // Re-open, then the toggle: a second opener dispatch closes.
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);
            harness.press(ActionId::AppSettings);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the opener toggles"
            );

            // Re-open, then the back affordance closes too.
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);
            harness.driver.click_by_label("Back");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the back affordance routed the close"
            );
        },
    );
}

/// J2 — the menu row invocation: the menu closes first, then Settings
/// opens (the D3 × D28b close-then-route rule).
#[test]
fn the_menu_row_closes_the_menu_then_opens_settings() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppMenu);
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );

            // The "Open settings" row: index-resolved through the
            // window's pushed root model (the D2 id-table rule — no
            // text matching).
            let entries = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_entries();
            let index = (0..entries.row_count())
                .find(|i| {
                    entries
                        .row_data(*i)
                        .is_some_and(|row| row.action == "app.settings")
                })
                .unwrap_or_else(|| panic!("the settings row is in the root page"));
            harness
                .handle
                .window()
                .invoke_menu_entry_invoked(index.try_into().expect("index fits"));

            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open(),
                "the menu closed first"
            );
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Settings,
                "then the settings surface opened"
            );
        },
    );
}

/// J3 — from `StackDetail`: open → close returns to the **exact
/// previous view** (the recorded snapshot, not a cycle walk).
#[test]
fn settings_closes_onto_the_exact_previous_view() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail
            );

            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);
            harness.press(ActionId::AppSettings);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail,
                "the close restored StackDetail exactly"
            );
        },
    );
}

/// J4 — cycle exclusion: with Settings open, the next/prev-view verbs
/// change nothing; from the return view, the full cycle never lands on
/// Settings.
#[test]
fn the_view_cycle_never_passes_through_settings() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Settings,
                "the cycle verbs are inert while Settings is open"
            );
            harness.press(ActionId::AppSettings);

            // A full cycle walk from the board: Settings is never a
            // member; Today is the cycle head (chunk 10 D2).
            for expected in [
                View::StackDetail,
                View::TaskDetail,
                View::Review,
                View::Today,
                View::Board,
            ] {
                harness.press(ActionId::NavNextView);
                assert_eq!(harness.handle.window().get_current_view(), expected);
            }
        },
    );
}

/// J5 — the form journey: the field mirrors ride the edited seams, the
/// Save click routes `SaveConnection` on the drained lane with the
/// typed payload (field reads, never message text); the request's
/// `Debug` shows no secret; a second Save with the same name is an
/// edit (the upsert payload). A save with an invalid draft routes
/// nothing (the form's enablement gate).
#[test]
fn the_settings_form_routes_the_save_ceremony() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);

            // An invalid draft (empty name): the Save click is
            // absorbed — nothing on the lane.
            harness.handle.window().invoke_settings_save_clicked(
                "".into(),
                "https://c.example".into(),
                "alice".into(),
                "pw".into(),
            );
            assert!(harness.drain_requests().is_empty(), "the gate held");

            // Fill the fields through the edited seams (the mirror
            // path the real fields drive).
            let window = harness.handle.window();
            window.invoke_settings_field_edited(0, "home".into());
            window.invoke_settings_field_edited(1, "https://c.example".into());
            window.invoke_settings_field_edited(2, "alice".into());
            window.invoke_settings_field_edited(3, "hunter2-secret".into());

            window.invoke_settings_save_clicked(
                "home".into(),
                "https://c.example".into(),
                "alice".into(),
                "hunter2-secret".into(),
            );
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "one SaveConnection routed");
            assert!(
                matches!(
                    &requests[0],
                    taskboard_ui_slint::requests::UiRequest::SaveConnection {
                        profile,
                        ..
                    } if profile.name == "home"
                        && profile.server_url == "https://c.example"
                        && profile.username == "alice"
                        && profile.password.0 == "hunter2-secret"
                ),
                "the typed payload carries the draft"
            );
            assert!(
                !format!("{:?}", requests[0]).contains("hunter2-secret"),
                "the request Debug never carries the secret"
            );

            // The app half answers (the A6 choreography); the drain
            // clears the pending state and lands the fresh rows.
            let taskboard_ui_slint::requests::UiRequest::SaveConnection { reply, .. } =
                requests.remove(0)
            else {
                panic!("the save request routed");
            };
            // The r2 D32 reply carries the full pair (boards +
            // connectors): both surfaces re-render from one reply.
            reply
                .send(Ok(taskboard_ui_slint::requests::ConnectionsSnapshot {
                    boards: Vec::new(),
                    connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
                        name: "home".into(),
                        server_url: "https://c.example".into(),
                        username: "alice".into(),
                    }],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            let rows = window
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_rows();
            assert_eq!(rows.row_count(), 1, "the reply's fresh list landed");
            assert_eq!(
                rows.row_data(0).expect("row").name,
                "home",
                "the reply payload is the secret-free row shape"
            );

            // A second save with the same name is an edit (the same
            // keyed payload — the upsert semantics, plan D7).
            window.invoke_settings_save_clicked(
                "home".into(),
                "https://other.example".into(),
                "alice".into(),
                "pw2".into(),
            );
            let requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::SaveConnection {
                    profile,
                    ..
                } if profile.name == "home"
                    && profile.server_url == "https://other.example"
            ));
        },
    );
}

/// J6 — the live status projection (D8): the row's status flips on
/// `ConnectorLost` and restores on `ConnectorRestored` (the down
/// override wins; the restore falls back to the bound-phases fold).
#[test]
fn connector_episodes_flip_the_row_status() {
    use taskboard_ui_slint::SettingsRowStatus;
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        connectors: vec![
            taskboard_ui_slint::requests::ConnectorInfo {
                name: "nextcloud".into(),
                server_url: "https://c.example".into(),
                username: "alice".into(),
            },
            // A second plain row (the two-row shape the status
            // episode below reads; the D4 read-only flag is gone with
            // round-3 D21 — every row is mutable).
            taskboard_ui_slint::requests::ConnectorInfo {
                name: "legacy".into(),
                server_url: "https://legacy.example".into(),
                username: "bob".into(),
            },
        ],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);

            let row_status = |harness: &Harness| {
                let rows = harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_rows();
                let row = rows.row_data(0).expect("the seeded row");
                row.status
            };
            assert_eq!(row_status(&harness), SettingsRowStatus::Idle);
            let rows = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsModel>()
                .get_rows();
            assert_eq!(rows.row_count(), 2, "both rows arrive");

            harness.handle.observe_connector_episode(
                &taskboard_domain::SystemEvent::ConnectorLost(taskboard_domain::ConnectorId::from(
                    "nextcloud",
                )),
            );
            assert_eq!(row_status(&harness), SettingsRowStatus::Down);

            harness.handle.observe_connector_episode(
                &taskboard_domain::SystemEvent::ConnectorRestored(
                    taskboard_domain::ConnectorId::from("nextcloud"),
                ),
            );
            assert_eq!(
                row_status(&harness),
                SettingsRowStatus::Idle,
                "the restore falls back to the fold (no bound boards → Idle)"
            );
        },
    );
}

/// J7 — the preset flow's two halves: the pick routes
/// `SetKeymapPreset` on the lane; applying the reply through the drain
/// swaps the keymap live — vi's colon opener resolves in the settings
/// chain right after (T3's journey half).
#[test]
fn the_preset_pick_routes_and_the_reply_swaps_the_keymap_live() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            #[allow(unused_mut)]
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);

            // Before: the standard preset has no colon binding — the
            // stroke falls through (unhandled) and the menu stays shut.
            harness.stroke(parse(":").expect("colon"));
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open()
            );

            // The pick routes the request.
            harness.handle.window().invoke_settings_preset_picked(2);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { preset, reply, .. } =
                requests.remove(0)
            else {
                panic!("the preset request routed");
            };
            assert_eq!(preset, taskboard_ui_slint::keymap::Preset::Vi);

            // The app half answers; the drain applies the swap live.
            reply
                .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                        taskboard_ui_slint::keymap::Preset::Vi,
                    ),
                    findings: 0,
                    preset: taskboard_ui_slint::keymap::Preset::Vi,
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();

            // After: vi's colon opens the menu through the same
            // settings-context chain (the live-apply fact, D9).
            harness.stroke(parse(":").expect("colon"));
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_menu_open(),
                "the swapped keymap resolves vi's colon in the settings chain"
            );
        },
    );
}

/// J8 — the selection echoes the *replied* preset, never the click
/// (r2 D27): the pick leaves the index untouched until the reply's
/// `KeymapUpdate.preset` lands through the drain, and a synthetic
/// reply for a different preset moves it again.
#[test]
fn the_preset_selection_echoes_the_replied_preset_not_the_click() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);

            let selection = |harness: &Harness| {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_preset_index()
            };
            // Mount: the standard preset, index 0.
            assert_eq!(selection(&harness), 0);

            // The pick: the request routes, but the selection does NOT
            // move — no optimistic click-echo.
            harness.handle.window().invoke_settings_preset_picked(2);
            assert_eq!(
                selection(&harness),
                0,
                "the click alone never moves the selection"
            );

            // The reply names vi; the drain echoes it.
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { reply, .. } =
                requests.remove(0)
            else {
                panic!("the preset request routed");
            };
            reply
                .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                        taskboard_ui_slint::keymap::Preset::Vi,
                    ),
                    findings: 0,
                    preset: taskboard_ui_slint::keymap::Preset::Vi,
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(selection(&harness), 2, "the replied preset is echoed");

            // A second pick (now allowed — pending cleared) replies for
            // a different preset: the reply, not the click, moves the
            // selection again.
            harness.handle.window().invoke_settings_preset_picked(1);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetKeymapPreset { reply, .. } =
                requests.remove(0)
            else {
                panic!("the second preset request routed");
            };
            reply
                .send(Ok(taskboard_ui_slint::requests::KeymapUpdate {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(
                        taskboard_ui_slint::keymap::Preset::Emacs,
                    ),
                    findings: 0,
                    preset: taskboard_ui_slint::keymap::Preset::Emacs,
                }))
                .expect("the reply lane is still alive");
            harness.handle.drain_setting_replies();
            assert_eq!(selection(&harness), 1, "a new reply re-echoes");
        },
    );
}

// ---------------------------------------------------------------------
// r4 D38: the scale release commit (the preview arm is gone)
// ---------------------------------------------------------------------

/// The scale drag moves nothing until release (r4 D38): the gesture
/// holds between press and release — the theme, the echo, and the
/// lane are all untouched (the drag itself is the widget's own, the
/// grab probe's p4); release emits exactly one `SetScale` carrying
/// the final value, the theme moves once, and the echo re-renders the
/// canonical commit.
#[test]
fn the_scale_drag_moves_nothing_until_release() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            let scale = || {
                window
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };
            assert_eq!(scale(), 1.0);

            // Mid-gesture: nothing observable moves.
            window.invoke_settings_scale_pressed();
            assert_eq!(scale(), 1.0, "the theme holds the committed scale");
            assert_eq!(
                model().get_scale_echo(),
                "1.0",
                "the echo holds the committed scale"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "no request mid-gesture"
            );

            // Release: one request, one theme move, the canonical echo.
            window.invoke_settings_scale_released(1.7);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one SetScale per gesture");
            // The seam's f32 hand-over pins the comparison term: the
            // committed value is exactly `f64::from(1.7f32)`.
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::SetScale { scale, .. }
                    if scale.total_cmp(&f64::from(1.7f32)) == std::cmp::Ordering::Equal
            ));
            assert!(
                (scale() - 1.7).abs() < 0.001,
                "the theme moves once, on release"
            );
            assert_eq!(
                model().get_scale_echo(),
                "1.7",
                "the echo re-renders the commit"
            );
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok,
            );
        },
    );
}

/// The scale echo renders the committed scale (r4 D38): mount echo;
/// a typed draft survives a concurrent drag (nothing writes the echo
/// mid-gesture anymore); focus loss discards the draft and re-syncs
/// to the committed scale; a commit re-renders the canonical echo and
/// an Err reply follows the revert; a Return-less draft never
/// survives the surface (plan §5.1 draft invariants).
#[test]
fn the_scale_echo_renders_the_committed_scale() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            let scale = || {
                window
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };

            // Mount: the canonical echo of the live scale.
            assert_eq!(model().get_scale_echo(), "1.0");

            // A typed draft survives a concurrent drag: mid-gesture
            // nothing writes the echo (the preview writer is gone).
            window.invoke_settings_field_edited(4, "2.2".into());
            assert_eq!(model().get_scale_echo(), "2.2", "the draft is the echo");
            window.invoke_settings_scale_pressed();
            assert_eq!(
                model().get_scale_echo(),
                "2.2",
                "the drag never touches the typed draft"
            );
            assert_eq!(scale(), 1.0, "the theme holds until release");

            // Focus loss discards the Return-less edit and re-syncs
            // to the committed scale (unchanged by the gesture).
            window.invoke_settings_scale_focus_lost();
            assert_eq!(
                model().get_scale_echo(),
                "1.0",
                "the echo re-syncs to the committed scale"
            );

            // Commit "2": canonical echo, one request, Err reverts.
            window.invoke_settings_field_edited(4, "2".into());
            window.invoke_settings_scale_committed("2".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "one SetScale");
            assert_eq!(model().get_scale_echo(), "2.0", "the canonical echo");
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply
                .send(Err(
                    taskboard_ui_slint::requests::SettingsError::PersistFailed,
                ))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(
                (scale() - 1.0).abs() < 0.001,
                "the Err reply reverts the theme"
            );
            assert_eq!(
                model().get_scale_echo(),
                "1.0",
                "the echo follows the reverted committed scale"
            );

            // A Return-less draft never survives the surface: close
            // and reopen settings, and the mount re-renders the
            // canonical live echo (plan §5.1 draft invariants).
            window.invoke_settings_field_edited(4, "9.9".into());
            assert_eq!(model().get_scale_echo(), "9.9", "the draft renders");
            harness.press(ActionId::AppSettings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_scale_echo(),
                "1.0",
                "the mount discards the stale draft"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r3 D20c: the scale commit's scroll anchor
// ---------------------------------------------------------------------

/// The scale commit anchors the scroll on the scale row (r3 D20c,
/// dispatch §5 g): scrolled to the bottom at scale 1.0 in a window
/// that overflows (driven by REAL wheel events — the production
/// scroll path), an UPWARD commit to 3.0 preserves the row's viewport
/// position (within a row height), the downward commit back to 1.0
/// leaves the viewport within bounds (no blank screen, finding 25),
/// and manual scrolling afterwards is never fought (one commit, one
/// consume).
#[test]
fn a_scale_commit_anchors_the_scroll_on_the_scale_row() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            driver.bind(&harness.handle);
            driver.set_window_size(480, 460);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let facts = window.global::<taskboard_ui_slint::SettingsScrollFacts>();
            driver.advance_time(0);
            assert!(
                facts.get_content_height() > facts.get_viewport_height(),
                "the overflow exists at scale 1.0 in the small window: {} vs {}",
                facts.get_content_height(),
                facts.get_viewport_height()
            );

            // Scroll to the bottom with real wheel events (the
            // production scroll path — the flickable processes them,
            // severing any property bindings on content-y exactly as
            // in production; the anchor must survive that).
            for _ in 0..24 {
                window
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(240.0, 230.0),
                        delta_x: 0.0,
                        delta_y: -60.0,
                    });
                driver.advance_time(30);
            }
            let overflow = facts.get_content_height() - facts.get_viewport_height();
            assert!(
                (facts.get_viewport_y() + overflow).abs() < 20.0,
                "the wheel reached the bottom"
            );

            // The row's viewport offset before the commit.
            let offset_before = facts.get_scale_row_y() + facts.get_viewport_y();

            // Commit UP via the field; the Ok reply settles.
            window.invoke_settings_field_edited(4, "3.0".into());
            window.invoke_settings_scale_committed("3.0".into());
            // The #412 capture rides the same ticked lane: this
            // journey's resized window legitimately emitted one
            // geometry snapshot on the first tick after the resize.
            // Drained here per the plan's stray-capture rule (the
            // test counts the commit's own lane, never the capture's).
            let mut requests: Vec<_> = harness
                .drain_requests()
                .into_iter()
                .filter(|request| {
                    !matches!(
                        request,
                        taskboard_ui_slint::requests::UiRequest::WindowGeometryChanged { .. }
                    )
                })
                .collect();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            driver.advance_time(0);

            // Up-leg: the row's viewport offset is preserved within a
            // row height.
            let offset_after = facts.get_scale_row_y() + facts.get_viewport_y();
            assert!(
                (offset_after - offset_before).abs() <= 60.0,
                "the scale row stayed put across the up-commit: {offset_before} -> {offset_after}"
            );
            let vy = facts.get_viewport_y();
            let min_vy = (facts.get_viewport_height() - facts.get_content_height()).min(0.0);
            assert!(
                vy >= min_vy - 0.01 && vy <= 0.01,
                "in bounds after the up-commit"
            );

            // Commit DOWN again; the viewport must land within bounds
            // (no blank screen — the finding-25 direction).
            window.invoke_settings_field_edited(4, "1.0".into());
            window.invoke_settings_scale_committed("1.0".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the second scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            driver.advance_time(0);
            let vy = facts.get_viewport_y();
            let min_vy = (facts.get_viewport_height() - facts.get_content_height()).min(0.0);
            assert!(
                vy >= min_vy - 0.01 && vy <= 0.01,
                "the viewport is within bounds after the down-commit (no blank): {vy} in [{min_vy}, 0]"
            );

            // Manual scrolling afterwards is never fought: a wheel
            // event moves the viewport (the anchor consumed once).
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::PointerScrolled {
                    position: slint::LogicalPosition::new(240.0, 230.0),
                    delta_x: 0.0,
                    delta_y: -60.0,
                });
            driver.advance_time(30);
            assert!(
                facts.get_viewport_y() < vy,
                "the manual wheel scroll stuck (the anchor consumed once)"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r3 D18: the status line's pinned placement
// ---------------------------------------------------------------------

/// The status line sits outside the scroll viewport (r3 D18, dispatch
/// §5 c): at scale 3.0 in the min-floor window, triggering an outcome
/// renders the pinned row above the Flickable's band — readable
/// without scrolling (finding 20) — while the pinned header stays
/// put; the hidden state leaves the scroll geometry untouched
/// (layouts skip invisible children).
#[test]
fn the_status_line_pins_above_the_scroll_viewport() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let settings = UiSettings {
                scale: taskboard_ui_slint::UiScale::new(3.0).expect("valid"),
                ..UiSettings::default()
            };
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            driver.bind(&harness.handle);
            driver.set_window_size(480 * 3, 400 * 3);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            driver.advance_time(0);

            // Hidden mount: no outcome, and the scroll host occupies
            // its geometry.
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None
            );
            let back_y = |driver: &SlintDriver| {
                driver
                    .find_by_label("Back")
                    .expect("the pinned Back affordance")
                    .absolute_position()
                    .y
            };
            let back_hidden = back_y(driver);

            // Trigger an outcome (the seam-local scale refusal).
            window.invoke_settings_scale_committed("not a number".into());
            driver.advance_time(0);
            assert_ne!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the outcome landed"
            );

            // The status row renders OUTSIDE the Flickable's band:
            // entirely above the viewport's top edge (measured after
            // the outcome — a visible row legitimately shrinks the
            // viewport, the fix).
            let flick_y = driver
                .find_all_by_id("SettingsView::flick")
                .into_iter()
                .next()
                .expect("the scroll host")
                .absolute_position()
                .y;
            let status = driver
                .find_all_by_id("SettingsView::status-line")
                .into_iter()
                .next()
                .expect("the pinned status line");
            let pos = status.absolute_position();
            let size = status.size();
            assert!(
                pos.y + size.height <= flick_y,
                "the status row (y {}, h {}) sits above the viewport top {}",
                pos.y,
                size.height,
                flick_y
            );

            // The pinned header never moved.
            assert_eq!(back_y(driver), back_hidden, "the header stays pinned");
        },
    );
}

// ---------------------------------------------------------------------
// r3 D16: the password eye toggle
// ---------------------------------------------------------------------

/// The eye toggle (r3 D16, dispatch §5 a): the secret field carries a
/// discoverable a11y button beside it; toggling is markup-local —
/// every model property stays untouched and the lane stays silent.
/// The reveal state itself (dots vs plaintext) is a Tier-4 visual,
/// not harness-pinned; the write-only redaction contract (the
/// emptiness oracle, nothing secret on any model/Debug) is pinned by
/// the save ceremony journey and the adapter's draft mirror.
#[test]
fn the_eye_toggles_locally_without_leaking_the_secret() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Type a secret through the mirror path.
            window.invoke_settings_field_edited(3, "hunter2-secret".into());
            let model_snapshot = (
                model().get_draft_name(),
                model().get_draft_server(),
                model().get_draft_username(),
                model().get_save_enabled(),
            );

            // The eye is discoverable as an accessible button beside
            // the password field.
            let eyes: Vec<_> = harness
                .driver
                .find_all_by_label("Reveal")
                .into_iter()
                .filter(support::driver::Element::is_accessible_button)
                .collect();
            assert_eq!(eyes.len(), 1, "exactly one eye on the form");

            // Toggle: no model property moves, the lane stays silent.
            eyes.into_iter()
                .next()
                .expect("the eye")
                .invoke_default_action();
            harness.driver.advance_time(0);
            assert_eq!(
                (
                    model().get_draft_name(),
                    model().get_draft_server(),
                    model().get_draft_username(),
                    model().get_save_enabled(),
                ),
                model_snapshot,
                "the toggle touched nothing but the field's local state"
            );
            assert!(harness.drain_requests().is_empty(), "zero lane traffic");

            // Re-toggle: the label swaps with the state.
            let concealed: Vec<_> = harness
                .driver
                .find_all_by_label("Conceal")
                .into_iter()
                .filter(support::driver::Element::is_accessible_button)
                .collect();
            assert_eq!(concealed.len(), 1, "the eye relabels with the state");
            concealed
                .into_iter()
                .next()
                .expect("the relabelled eye")
                .invoke_default_action();
            harness.driver.advance_time(0);
            assert_eq!(
                (
                    model().get_draft_name(),
                    model().get_draft_server(),
                    model().get_draft_username(),
                    model().get_save_enabled(),
                ),
                model_snapshot,
                "the second toggle is equally local"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r3 D19: the validity flavour (per-field, per keystroke)
// ---------------------------------------------------------------------

/// The validity flavour tracks every field per keystroke (r3 D19,
/// dispatch §5 d): empty renders neutral with no error line, a
/// failing field renders invalid with a non-empty error element, a
/// passing field renders valid — and the Save gate (the same ladder,
/// one source of truth) tracks the draft throughout. The error copy
/// itself is presentation text and is never asserted (§7-R3).
#[test]
fn the_validity_flavour_tracks_each_field_per_keystroke() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The empty mount: every field neutral, no error line.
            assert_eq!(model().get_validity_name(), FieldValidity::Neutral);
            assert_eq!(model().get_error_name(), "");
            assert!(
                harness
                    .driver
                    .find_all_by_id("LabeledField::error-line")
                    .is_empty(),
                "no error line renders on the empty mount"
            );

            // An invalid name: invalid flavour + non-empty error
            // element, the Save gate closed.
            window.invoke_settings_field_edited(0, "Work".into());
            assert_eq!(model().get_validity_name(), FieldValidity::Invalid);
            assert_ne!(model().get_error_name(), "");
            assert_eq!(model().get_validity_server(), FieldValidity::Neutral);
            assert!(!model().get_save_enabled(), "the gate follows the ladder");

            // Fixed: valid flavour, the line gone.
            window.invoke_settings_field_edited(0, "work".into());
            assert_eq!(model().get_validity_name(), FieldValidity::Valid);
            assert_eq!(model().get_error_name(), "");

            // Empty again: back to neutral, never an error line.
            window.invoke_settings_field_edited(0, "".into());
            assert_eq!(model().get_validity_name(), FieldValidity::Neutral);
            assert_eq!(model().get_error_name(), "");

            // The server field's grammar: a bad URL is invalid, a
            // good one valid.
            window.invoke_settings_field_edited(1, "not a url".into());
            assert_eq!(model().get_validity_server(), FieldValidity::Invalid);
            assert_ne!(model().get_error_server(), "");
            window.invoke_settings_field_edited(1, "https://c.example".into());
            assert_eq!(model().get_validity_server(), FieldValidity::Valid);

            // The username: whitespace-only is invalid (the trimmed
            // ladder rule), a name valid.
            window.invoke_settings_field_edited(2, "   ".into());
            assert_eq!(model().get_validity_username(), FieldValidity::Invalid);
            window.invoke_settings_field_edited(2, "alice".into());
            assert_eq!(model().get_validity_username(), FieldValidity::Valid);

            // The password rides the draft mirror: typed text is
            // valid; the redaction contract is pinned by the save
            // ceremony journey.
            window.invoke_settings_field_edited(3, "pw".into());
            assert_eq!(model().get_validity_password(), FieldValidity::Valid);

            // All fields pass: the gate opened.
            window.invoke_settings_field_edited(0, "work".into());
            assert!(model().get_save_enabled(), "the valid draft enables Save");
        },
    );
}

/// The scale field's flavour carries the typed detail (r3 D20a): a
/// parse miss on Return renders the invalid flavour + error line and
/// the status Rejected *with* the typed scale detail (the recorded
/// decision upgrading the r2 pins' detail-less Rejected), and the
/// typo stays in the field.
#[test]
fn a_scale_parse_miss_renders_the_invalid_flavour_and_typed_detail() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The typed draft rides the edited seam (the field's
            // per-keystroke path), then Return.
            window.invoke_settings_field_edited(4, "abc".into());
            window.invoke_settings_scale_committed("abc".into());
            assert_eq!(model().get_last_outcome(), SettingsOutcome::Rejected);
            assert_eq!(model().get_validity_scale(), FieldValidity::Invalid);
            assert_ne!(model().get_error_scale(), "", "the error line renders");
            assert_eq!(
                model().get_scale_echo(),
                "abc",
                "the typo stays visible under the flavour"
            );

            // A typed Err reply's revert and a successful commit are
            // pinned by the scale trio journeys.
        },
    );
}

// ---------------------------------------------------------------------
// r2 D28: the status line's text and auto-clear lifetime
// ---------------------------------------------------------------------

/// Terminal status shows mapped text and auto-clears under the mock
/// clock (r2 D28): the seam-local Rejected (a scale parse miss — no
/// lane round-trip; recorded decision, r3 D20a: since then it also
/// carries the typed `Invalid{Scale, NotKebab}` detail and the field
/// flavour — these pins assert outcome lifetime only) is visible for
/// `STATUS_VISIBLE_MS`, then returns to
/// hidden (outcome None, text empty — the VM cleared, so no refresh
/// can resurrect it).
#[test]
fn a_terminal_status_shows_text_then_auto_clears_under_the_mock_clock() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let model = || {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsModel>()
            };
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the mount starts hidden"
            );

            harness
                .handle
                .window()
                .invoke_settings_scale_committed("not a number".into());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the terminal outcome landed"
            );
            assert_ne!(
                model().get_status_text(),
                "",
                "the status shows its mapped text"
            );

            harness
                .driver
                .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 1);
            assert_ne!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "still visible one tick before the deadline"
            );
            harness.driver.advance_time(1);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the auto-clear fired at the deadline"
            );
            assert_eq!(model().get_status_text(), "", "the text cleared with it");
        },
    );
}

/// A new outcome re-arms the auto-clear (r2 D28; the scale commit's
/// typed detail arrived later, r3 D20a — recorded decision, the pins
/// here assert outcome lifetime only): seven seconds after
/// the first terminal outcome a second one landed three seconds in —
/// the status must still be visible, clearing only `STATUS_VISIBLE_MS`
/// after the *second* outcome (the tooltip-restart pin's shape).
#[test]
fn a_new_outcome_re_arms_the_status_clear() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let model = || {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsModel>()
            };
            let window = harness.handle.window();

            window.invoke_settings_scale_committed("not a number".into());
            harness
                .driver
                .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 2_000);
            window.invoke_settings_scale_committed("0.0".into());
            harness
                .driver
                .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS - 2_000);
            assert_ne!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "still visible past the FIRST outcome's deadline — the re-arm reset it"
            );
            harness.driver.advance_time(2_000);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the clear rides the second outcome's deadline"
            );
        },
    );
}

/// A busy status never auto-clears mid-flight (r2 D28): the Save
/// click's Busy holds past twice the clear window; the Ok reply arms
/// the clear, which then fires.
#[test]
fn busy_status_holds_until_the_reply_lands() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let model = || {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::SettingsModel>()
            };

            // A valid draft: the Save click routes the ceremony.
            let window = harness.handle.window();
            window.invoke_settings_field_edited(0, "home".into());
            window.invoke_settings_field_edited(1, "https://c.example".into());
            window.invoke_settings_field_edited(2, "alice".into());
            window.invoke_settings_field_edited(3, "hunter2-secret".into());
            window.invoke_settings_save_clicked(
                "home".into(),
                "https://c.example".into(),
                "alice".into(),
                "hunter2-secret".into(),
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Busy,
            );

            // Twice the clear window: the busy status holds (the
            // timer was stopped, never armed).
            harness
                .driver
                .advance_time(2 * taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Busy,
                "busy never auto-clears mid-flight"
            );

            // The reply lands through the drain: Ok, the clear armed.
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SaveConnection { reply, .. } =
                requests.remove(0)
            else {
                panic!("the save request routed");
            };
            // The r2 D32 reply carries the full pair (boards +
            // connectors): both surfaces re-render from one reply.
            reply
                .send(Ok(taskboard_ui_slint::requests::ConnectionsSnapshot {
                    boards: Vec::new(),
                    connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
                        name: "home".into(),
                        server_url: "https://c.example".into(),
                        username: "alice".into(),
                    }],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok,
                "the Ok reply is visible"
            );
            harness
                .driver
                .advance_time(taskboard_ui_slint::settings_status::STATUS_VISIBLE_MS);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::None,
                "the Ok status auto-clears"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r2 D29, r4 D38: the scale slider's gesture (press / release)
// ---------------------------------------------------------------------

/// The slider's release commits once per gesture (r2 D29, r4 D38):
/// release routes exactly one `SetScale` carrying the final value,
/// whose Ok reply leaves the outcome Ok and the scale persisted; the
/// lane sees nothing before the release.
#[test]
fn the_slider_holds_the_gesture_and_commits_once_on_release() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let scale = || {
                window
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };

            window.invoke_settings_scale_pressed();
            assert!(
                harness.drain_requests().is_empty(),
                "the held gesture writes no lane request"
            );

            window.invoke_settings_scale_released(2.0);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one SetScale per gesture");
            // 2.0 is exactly representable — the guard pattern keeps
            // clippy's float_cmp appeased without a magic margin.
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::SetScale { scale, .. }
                    if scale.total_cmp(&2.0) == std::cmp::Ordering::Equal
            ));
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Busy,
            );
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply.send(Ok(())).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok,
            );
            assert_eq!(scale(), 2.0, "the committed scale stands");
        },
    );
}

/// A slider gesture is inert while an operation is pending (r2 D29):
/// the silent pending gate absorbs press and release — no `SetScale`,
/// no scale movement, no outcome change.
#[test]
fn a_slider_gesture_is_inert_while_an_op_is_pending() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let scale = || {
                window
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Make an op pending (the preset pick; the request rides
            // the lane and stays unanswered — pending is the state).
            window.invoke_settings_preset_picked(2);
            assert_eq!(harness.drain_requests().len(), 1);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Busy,
            );
            assert_eq!(scale(), 1.0, "the mount scale stands");

            window.invoke_settings_scale_pressed();
            window.invoke_settings_scale_released(2.0);
            assert!(
                harness.drain_requests().is_empty(),
                "no SetScale while pending"
            );
            assert_eq!(scale(), 1.0, "the scale never moved");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Busy,
                "the outcome is untouched"
            );
        },
    );
}

/// A slider commit reverts on a typed error reply (r2 D29): the
/// Err reply restores the pre-**gesture** scale, and the outcome is
/// the typed fold of the error — outcome-agnostic revert.
#[test]
fn a_slider_commit_reverts_on_a_typed_error_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let scale = || {
                window
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            assert_eq!(scale(), 1.0);

            window.invoke_settings_scale_pressed();
            window.invoke_settings_scale_released(2.0);
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetScale { reply, .. } =
                requests.remove(0)
            else {
                panic!("the scale request routed");
            };
            reply
                .send(Err(
                    taskboard_ui_slint::requests::SettingsError::PersistFailed,
                ))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                scale(),
                1.0,
                "the revert restores the persisted scale, not the preview"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Failed,
                "the typed error folds to its outcome"
            );
        },
    );
}

// ---------------------------------------------------------------------
// r2 D30: the settings content scrolls under the pinned header
// ---------------------------------------------------------------------

/// The scale-3.0 reachability contract (r2 D30, dispatch §6 f): at
/// scale 3.0 in the scale-multiplied min-floor window the content
/// overflows, the scroll reaches the scale controls (the content's
/// tail), the pinned header never moves, and the Window size
/// invariant holds throughout.
#[test]
fn settings_scroll_reaches_the_scale_controls_at_scale_3() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let settings = UiSettings {
                scale: taskboard_ui_slint::UiScale::new(3.0).expect("valid"),
                ..UiSettings::default()
            };
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            driver.bind(&harness.handle);
            driver.set_window_size(480 * 3, 400 * 3);
            harness.press(ActionId::AppSettings);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);

            let window = harness.handle.window();
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::SettingsScrollFacts>();
            assert!(
                facts.get_content_height() > facts.get_viewport_height(),
                "the overflow exists at scale 3.0 in the min-floor window"
            );

            let back_y = |driver: &mut SlintDriver| {
                driver
                    .find_by_label("Back")
                    .expect("the pinned Back affordance")
                    .absolute_position()
                    .y
            };
            let before = back_y(driver);

            // Scroll to the bottom: the content's tail — the scale
            // row and the status row — becomes reachable.
            let overflow = facts.get_content_height() - facts.get_viewport_height();
            facts.set_viewport_y(-overflow);
            driver.advance_time(0);
            assert!(
                (facts.get_viewport_y() + overflow).abs() < 0.01,
                "the write-back landed (the two-way binding)"
            );

            // The content's tail moved past the scale row (the GUI
            // settings editor's sections joined below it): the
            // display section's field is the deepest labelled
            // addressable, and it sits inside the window after the
            // scroll.
            let tail_field = driver
                .find_by_label("Due soon (days)")
                .expect("the display field is addressable");
            let pos = tail_field.absolute_position();
            let size = tail_field.size();
            assert!(
                pos.y >= 0.0 && pos.y + size.height <= 400.0 * 3.0,
                "the content tail sits inside the window after the scroll"
            );
            // The status row is the content's last child: with the
            // viewport at the bottom the content's bottom edge is
            // exactly the viewport's bottom edge, so the tail is on
            // screen by construction.
            assert!(
                (facts.get_viewport_y() + facts.get_content_height() - facts.get_viewport_height())
                    .abs()
                    < 0.01,
                "the content tail (the status row) is reachable"
            );

            // The pinned header: the Back affordance's y never moved.
            assert_eq!(back_y(driver), before, "the header never scrolls away");

            // The Window invariant: no width/height binding may react
            // to the scale — the dims out-properties are unchanged
            // across the whole journey.
            assert_eq!(window.get_window_w(), 480.0 * 3.0);
            assert_eq!(window.get_window_h(), 400.0 * 3.0);
        },
    );
}

// ---------------------------------------------------------------------
// r2 D31: the InfoTip help affordances
// ---------------------------------------------------------------------

/// Every settings field carries its `InfoTip` (r2 D31): with one seeded
/// connector row the seven placements resolve by element id — the
/// four `LabeledField` tips (Name, Server, Username, App password),
/// the scale row's, the keymap header's, and the connection row's.
/// Role-less elements have no accessible-label address, hence id
/// addressing; the count pins the placements, never the text.
#[test]
fn every_settings_field_carries_its_info_tip() {
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        connectors: vec![taskboard_ui_slint::requests::ConnectorInfo {
            name: "nextcloud".into(),
            server_url: "https://c.example".into(),
            username: "alice".into(),
        }],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            // The general fields' tips sit deep in the scroll: the
            // testing backend addresses laid-out elements only, so
            // the window grows tall enough to lay every placement out
            // on screen (the id addressing, not visibility, is the
            // pin's subject).
            harness.driver.set_window_size(1000, 2400);
            harness.driver.advance_time(0);
            assert_eq!(harness.handle.window().get_current_view(), View::Settings);
            let driver = &mut harness.driver;
            driver.advance_time(0);

            // The backend's id match reports the tip's subtree too
            // (the chain carries the id), so every count pins DISTINCT
            // physical squares — one per placement.
            let distinct_spots = |driver: &SlintDriver, id: &str| {
                let mut spots: Vec<(f32, f32)> = driver
                    .find_all_by_id(id)
                    .iter()
                    .map(|tip| {
                        let p = tip.absolute_position();
                        (p.x, p.y)
                    })
                    .collect();
                spots.sort_by(|a, b| a.partial_cmp(b).expect("no NaN positions"));
                spots.dedup();
                spots.len()
            };
            assert_eq!(
                distinct_spots(driver, "LabeledField::help-tip"),
                15,
                "the four connection fields, the seven general fields, \
                 and the four focus minute fields each carry a tip"
            );
            for id in [
                "SettingsView::help-scale",
                "SettingsView::help-keymap",
                "SettingsView::help-rows",
            ] {
                assert_eq!(distinct_spots(driver, id), 1, "exactly one tip at {id}");
            }
        },
    );
}

/// A hover on an `InfoTip` arms the generalized tooltip (r2 D31): the
/// pending state arms immediately, the mock clock's 600 ms delay
/// shows it with a non-empty description (the help fact — content
/// never asserted), and moving away hides it.
#[test]
fn an_info_tip_hover_arms_the_generalized_tooltip() {
    let (_fixture, mut state) = fixture::golden_state();
    // The golden fixture now seeds a running timer (DR-5/4, D-TU12);
    // its chip control trips the testing backend's element traversal
    // (a second conditional control in the status row makes the
    // SettingsView query return empty — backend limitation, not
    // behavior: the settings journeys pass with the chip mounted).
    // The tooltip machinery is this test's subject; it runs on a
    // chip-free chrome. Issue-worthy against the backend harness.
    state.tracking.entries.clear();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            driver.bind(&harness.handle);
            // The regular/expansive breakpoint: at the compact
            // default the backend's element traversal returns empty
            // over the selector's label-above branch (the recorded
            // chip-free-chrome limitation, extended by one branch).
            driver.set_window_size(1000, 900);
            driver.advance_time(0);

            let tip = driver
                .find_all_by_id("SettingsView::help-scale")
                .into_iter()
                .next()
                .expect("the scale tip is present");
            let pos = tip.absolute_position();
            let size = tip.size();
            driver.hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);

            let tooltip = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::TooltipState>();
            assert!(tooltip.get_pending(), "the hover armed the delay");
            driver.advance_time(600);
            assert!(tooltip.get_visible(), "the delay fired");
            assert_ne!(
                tooltip.get_description(),
                "",
                "the help fact rides the description slot"
            );

            // Moving away dismisses (the D13 lifecycle).
            driver.hover_at(4.0, 4.0);
            assert!(!tooltip.get_pending());
            assert!(!tooltip.get_visible());
        },
    );
}

/// The rule-list journey leg (chunk 6, D12/D13): the task detail's
/// composition renders the subject task's rules (rows + the derived
/// hint) and its seams dispatch the exact typed commands — preset add,
/// custom add, absorbed parse miss, row remove resolved through the id
/// tables. Rows-render and hint-presence are asserted here at the
/// composition's live mount (the component-level read folded in — one
/// behavior, one test).
/// The at/in grammar legs (r4 D30/D6, T8): "in 2h" dispatches the
/// At(In) shape — the command carries the duration, the planner stamps
/// the instant; "at 17:00" dispatches the At(At) shape with a concrete
/// instant (the host-local seam resolved it — a typed instant arrived,
/// which is the structural fact); bare garbage and a non-positive
/// in-magnitude are the absorbed misses (D12).
fn at_in_legs(harness: &mut Harness, task1: TaskId) {
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("in 2h".into());
    harness.assert_commands(&[StateCommand::AddReminderRule {
        task: task1,
        offset: taskboard_domain::ReminderOffset::at_due(),
        at: Some(taskboard_domain::AdhocWhen::In(
            TimeDelta::try_hours(2).expect("2h"),
        )),
        insistence: taskboard_domain::Insistence::Normal,
    }]);
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("at 17:00".into());
    let commands = harness.engine.drain();
    let [
        StateCommand::AddReminderRule {
            at: Some(taskboard_domain::AdhocWhen::At(_)),
            ..
        },
    ] = commands[..]
    else {
        panic!("expected one AddReminderRule at Some(At(_)), got {commands:?}");
    };
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("soon".into());
    harness
        .handle
        .window()
        .invoke_rule_add_custom_accepted("in -10m".into());
    harness.assert_commands(&[]);
}

/// T9 (r4 D30/D31a, F25): the rows per kind and the un-mute
/// round-trip. An At row renders the countdown + fixed fire time
/// (structural shape, not label text) and survives a one-second
/// repaint (the seconds-surface term is active); an unarmed
/// `DueOffset` row presents the needs-a-due-date state and its jump affordance
/// opens the due editor (`Flow::EditDate { field: Due }`); mute shows
/// the muted indicator and un-mute dispatches the mute-clearing
/// command over the subject.
#[test]
fn rule_rows_present_per_kind_and_unmute_round_trips() {
    let (fixture, mut state) = fixture::journey_state();
    let task1 = fixture.task1;
    // The subject is undated in this journey (the F25 class): an At
    // rule and an unarmed DueOffset rule live side by side.
    let live = state.tasks.get_mut(&task1).expect("task");
    live.duedate = None;
    let at_instant = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp() + 7_200, 0)
        .expect("whole second");
    let at_rule = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(11)),
        task: task1,
        kind: taskboard_domain::ReminderKind::At(
            taskboard_domain::AtInstant::whole_second(at_instant).expect("whole-second"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
    };
    let unarmed_rule = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(12)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(
            taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).expect("-10m"))
                .expect("legal offset"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
    };
    state.reminders.muted.insert(task1);
    state.reminders.rules.insert(at_rule.id, at_rule.clone());
    state.reminders.rules.insert(unarmed_rule.id, unarmed_rule);

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);

            let task_model = harness.handle.window().get_task_model();
            // Mute is visible where it bites (D31a — the ranked cause
            // of "never fires" made visible).
            assert!(
                task_model.rules_muted,
                "the muted indicator rides the model"
            );
            assert_eq!(task_model.rules.row_count(), 2, "one row per rule");
            // The At row: armed, the countdown + fixed fire time shape.
            let at_row = task_model.rules.row_data(0).expect("row 0");
            assert!(at_row.armed, "an At row is armed regardless of due date");
            assert!(
                at_row.before.starts_with("in "),
                "the countdown line is the d/h/m/s shape"
            );
            assert_eq!(
                at_row.before.split(' ').count(),
                5,
                "in <d>d <h>h <m>m <s>s — five tokens"
            );
            assert!(
                at_row.detail.starts_with("@ "),
                "the fixed local fire time carries the @ prefix shape"
            );
            // The unarmed offset row: the explicit state + the jump
            // affordance's derived bit.
            let unarmed_row = task_model.rules.row_data(1).expect("row 1");
            assert!(
                !unarmed_row.armed,
                "a due-less offset row derives not-armed in the projection"
            );

            // The countdown is a seconds surface: a one-second repaint
            // re-projects the row (and never panics).
            harness.driver.advance_time(1_000);
            let task_model = harness.handle.window().get_task_model();
            assert_eq!(
                task_model.rules.row_count(),
                2,
                "the rows survive the repaint"
            );
            let at_row = task_model.rules.row_data(0).expect("row 0 after repaint");
            assert!(
                at_row.before.starts_with("in "),
                "the countdown re-projected"
            );

            // The jump affordance opens the due editor for the subject
            // (the existing due-edit seam).
            harness
                .handle
                .window()
                .invoke_task_rule_jump_to_due_clicked();
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the jump affordance opens the due editor"
            );

            // Un-mute dispatches the mute-clearing command over the
            // subject (D31a).
            harness.handle.window().invoke_task_rule_unmute_clicked();
            harness.assert_commands(&[StateCommand::SetTaskRemindersMuted {
                task: task1,
                muted: false,
            }]);
        },
    );
}

#[test]
fn the_rule_list_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let task1 = fixture.task1;
    // Two rules on the focused task, distinct offsets; ids ascend with
    // insertion order so the row table is deterministic.
    let rule1 = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(11)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(taskboard_domain::ReminderOffset::at_due()),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
    };
    let rule2 = taskboard_domain::ReminderRule {
        insistence: taskboard_domain::Insistence::Normal,
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(12)),
        task: task1,
        kind: taskboard_domain::ReminderKind::DueOffset(
            taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).expect("-10m"))
                .expect("legal offset"),
        ),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
    };
    state.reminders.rules.insert(rule1.id, rule1);
    state.reminders.rules.insert(rule2.id, rule2.clone());

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Walk to task1's detail: two item steps land on (0, 1) —
            // the fixture's second task — then two view steps.
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            assert_eq!(harness.emphasized_card(), Some((0, 1)));
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.assert_commands(&[]);

            // The composition rendered from state: two rows in rule-id
            // order and the derived-fact hint present.
            let task_model = harness.handle.window().get_task_model();
            assert_eq!(task_model.rules.row_count(), 2, "one row per rule");
            assert_eq!(
                task_model.rules.row_data(0).expect("row 0").before,
                "0s",
                "the canonical text form"
            );
            assert_eq!(task_model.rules.row_data(1).expect("row 1").before, "10m");
            assert!(
                !task_model.rules_hint.is_empty(),
                "a rule set presents the derived hint"
            );
            assert_eq!(task_model.rule_presets.row_count(), 4, "the preset chips");

            // Preset add (chip 0 = 5m) routes the exact typed command.
            harness.handle.window().invoke_rule_add_preset_clicked(0);
            harness.assert_commands(&[StateCommand::AddReminderRule {
                task: task1,
                offset: taskboard_domain::ReminderOffset::before(
                    TimeDelta::try_minutes(-5).expect("-5m"),
                )
                .expect("legal offset"),
                at: None,
                insistence: taskboard_domain::Insistence::Normal,
            }]);

            // A custom entry parses at the adapter ("45m").
            harness
                .handle
                .window()
                .invoke_rule_add_custom_accepted("45m".into());
            harness.assert_commands(&[StateCommand::AddReminderRule {
                task: task1,
                offset: taskboard_domain::ReminderOffset::before(
                    TimeDelta::try_minutes(-45).expect("-45m"),
                )
                .expect("legal offset"),
                at: None,
                insistence: taskboard_domain::Insistence::Normal,
            }]);

            at_in_legs(&mut harness, task1);

            // Row remove resolves the index through the id tables.
            harness.handle.window().invoke_task_rule_remove_clicked(1);
            harness.assert_commands(&[StateCommand::RemoveReminderRule { rule: rule2.id }]);

            // A stale row index (past the table) absorbs.
            harness.handle.window().invoke_task_rule_remove_clicked(9);
            harness.assert_commands(&[]);
        },
    );
}

/// R2 D10 (§3) — the synthesized double-click journey: two clicks at
/// a card's centre with a refresh (the first click's focus refresh)
/// in between and no mock-clock advance land on the same card
/// instance under the D9 transport, and the second release fires the
/// card's `double-clicked => activated()` wiring — the task-detail
/// view. The window-level click counter runs on the mock clock; the
/// journey never advances it.
#[test]
fn a_double_click_across_a_refresh_opens_the_task_detail() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let pos = card.absolute_position();
            let size = card.size();
            let cx = pos.x + size.width / 2.0;
            let cy = pos.y + size.height / 2.0;

            harness.driver.click_at(cx, cy);
            harness.handle.refresh();
            harness.driver.click_at(cx, cy);

            assert_eq!(
                harness.handle.window().get_current_view(),
                View::TaskDetail,
                "the second release activated the card"
            );
        },
    );
}

/// R2 D11 (§4) — the drag survives refreshes: press, tick, refresh,
/// tick, a concurrent engine change (a new task in the other stack) +
/// refresh, tick, release. Every tick's live width stays on the model,
/// the interleaved passes never disturb the drag (the tables guard
/// still resolves column 0), requests stay empty until release, and
/// the release emits exactly one full-snapshot request with the final
/// width. The `TouchArea` survival itself is the repetition-probe's
/// mechanism; the feel is the Tier-4 item.
#[test]
fn the_drag_survives_interleaved_refreshes_and_engine_changes() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state.clone());
            let window = harness.handle.window();
            // A deterministic row: 912 rendered px, two shares of 456.
            harness.driver.set_window_size(1000, 800);

            window.invoke_board_stack_width_press(0);
            window.invoke_board_stack_width_drag(0, -20.0);
            assert_eq!(harness.column_width_px(0), 436.0, "first tick");
            harness.handle.refresh();
            window.invoke_board_stack_width_drag(0, -30.0);
            assert_eq!(
                harness.column_width_px(0),
                426.0,
                "the refresh did not disturb the drag (delta stays absolute)"
            );

            // A concurrent engine change: a new task joins stack b.
            let mut changed = state.clone();
            changed.tasks.insert(
                taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                taskboard_domain::Task {
                    id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                    remote: None,
                    title: "mid-drag".into(),
                    description: String::new(),
                    duedate: None,
                    scheduled: None,
                    done: None,
                    stack: fixture.stack_b,
                    order: 1,
                    label_memberships: std::collections::BTreeMap::new(),
                    effort: None,
                    importance: None,
                    urgency: None,
                    recur: None,
                    kind: taskboard_domain::entities::TaskKind::Standard,

                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::TaskClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        description: taskboard_domain::BoardClocks::epoch(),
                        duedate: taskboard_domain::BoardClocks::epoch(),
                        scheduled: taskboard_domain::BoardClocks::epoch(),
                        done: taskboard_domain::BoardClocks::epoch(),
                        position: taskboard_domain::BoardClocks::epoch(),

                        effort: taskboard_domain::BoardClocks::epoch(),
                        importance: taskboard_domain::BoardClocks::epoch(),
                        urgency: taskboard_domain::BoardClocks::epoch(),
                        recur: taskboard_domain::BoardClocks::epoch(),
                        kind: taskboard_domain::BoardClocks::epoch(),

                        archived: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                    remote_baseline: None,
                },
            );
            harness.engine.replace_state(changed, &harness.handle);
            window.invoke_board_stack_width_drag(0, -40.0);
            assert_eq!(
                harness.column_width_px(0),
                416.0,
                "the engine change did not disturb the drag"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "drag ticks never persist (release only)"
            );

            window.invoke_board_stack_width_release(0);
            let requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one request on release");
            let taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                board: payload_board,
                widths,
            } = &requests[0]
            else {
                panic!("the typed snapshot reached the lane");
            };
            assert_eq!(payload_board, &board);
            let mut got = widths.clone();
            got.sort_by_key(|(stack, _)| *stack);
            assert_eq!(
                got,
                vec![(fixture.stack_a, 416), (fixture.stack_b, 496)],
                "the release snapshot carries the materialized row"
            );
        },
    );
}

/// R2 D12 (§5.3) — the mouse parent path syncs: clicking a parent row
/// in the wide (fly-out) presentation pushes `menu-active-group` (the
/// fly-out mounts — F7's "clicking parents does nothing") and the
/// fly-out's leaf click switches the view.
#[test]
fn a_wide_parent_click_opens_the_fly_out_and_its_leaf_switches_the_view() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);

            harness.driver.click_by_label("View & navigation");
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_menu_active_group() >= 0,
                "the parent click pushed the active group"
            );
            assert!(
                facts(&harness).get_flyout_width() > 0.0,
                "the fly-out mounted"
            );

            harness.driver.click_by_label("Next view");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail,
                "the fly-out leaf switched the view"
            );
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the leaf invocation closed the menu"
            );
        },
    );
}

/// R2 D12 (§5.3) — the compact replace page resolves its own index
/// space: the parent click replaces the panel's page, and the leaf
/// click resolves against the DISPLAYED page (pre-fix the click
/// resolved against the root page's vector — the F-B3 mismatch).
#[test]
fn a_compact_parent_click_replaces_the_page_and_its_leaf_resolves_it() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(600, 800);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);

            harness.driver.click_by_label("View & navigation");
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_menu_active_group() >= 0,
                "the parent click pushed the active group"
            );

            // The replaced page's rows are addressable and resolve
            // against the displayed (submenu) page.
            harness.driver.click_by_label("Next view");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::StackDetail,
                "the replace page's leaf switched the view"
            );
        },
    );
}

/// R2 D13 (§6.3) — a visible tooltip does not survive a view switch:
/// the interpret leaving block dismisses the trigger, so the pending
/// or visible tip is gone after `nav.next-view` (the anchor unmounted).
#[test]
fn a_visible_tooltip_does_not_survive_a_view_switch() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            // Hover a card through its a11y geometry, let the delay
            // fire.
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let pos = card.absolute_position();
            let size = card.size();
            let mut armed = false;
            let mut y = pos.y + 2.0;
            while y < pos.y + size.height - 2.0 {
                harness.driver.hover_at(pos.x + size.width / 2.0, y);
                harness.driver.advance_time(0);
                if tooltip_state(&harness).get_pending() {
                    armed = true;
                    break;
                }
                y += 4.0;
            }
            assert!(armed, "the card hover armed the trigger");
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            harness.press(ActionId::NavNextView);
            harness.driver.advance_time(0);
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the view switch dismissed the tooltip"
            );
        },
    );
}

/// R2 D13 (§6.3) — a menu-row click unhovers: the pending tip armed by
/// hovering a row is cleared by clicking a (parent) row; the submenu
/// opening itself is the D12 journey's assertion.
#[test]
fn a_menu_row_click_unhovers_the_tooltip() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);

            // The existing row-hover scan pattern: arm the trigger on
            // some enabled row.
            let facts = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::MenuPanelFacts>();
            let (px, pw) = (facts.get_x(), facts.get_width());
            let (py, ph) = (facts.get_y(), facts.get_height());
            let mut armed = false;
            let mut y = py + 8.0;
            while y < py + ph - 8.0 {
                harness.driver.hover_at(px + pw / 2.0, y);
                harness.driver.advance_time(0);
                if tooltip_state(&harness).get_pending() {
                    armed = true;
                    break;
                }
                y += 6.0;
            }
            assert!(armed, "a row hover armed the tooltip");

            harness.driver.click_by_label("View & navigation");
            harness.driver.advance_time(0);
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_pending() && !tooltip.get_visible(),
                "the row click unhovered the tooltip"
            );
        },
    );
}

/// R2 D14 (§7) — the tooltip width includes the hint row, relation
/// only (no font metrics): the same caption with a two-keycap hint is
/// wider than without; the absurd-caption case keeps the window cap;
/// a keys-less hover is not widened by an empty model.
#[test]
fn the_tooltip_width_includes_the_hint_row_and_keeps_the_cap() {
    use slint::{ModelRc, SharedString, VecModel};
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);

            // (a) Short caption, no keys → w1; same caption plus a
            //     two-keycap hint → wider.
            tooltip_state(&harness).invoke_hover(
                "short".into(),
                "".into(),
                ModelRc::default(),
                500.0,
                300.0,
                32.0,
                32.0,
            );
            harness.driver.advance_time(600);
            harness.driver.advance_time(0);
            let plain = tooltip_state(&harness).get_tip_width();
            assert!(plain > 0.0, "the tip measured its width");
            tooltip_state(&harness).invoke_unhover();

            let keys = ModelRc::new(VecModel::from(vec![
                SharedString::from("Ctrl"),
                SharedString::from("K"),
            ]));
            tooltip_state(&harness).invoke_hover(
                "short".into(),
                "".into(),
                keys,
                500.0,
                300.0,
                32.0,
                32.0,
            );
            harness.driver.advance_time(600);
            harness.driver.advance_time(0);
            let hinted = tooltip_state(&harness).get_tip_width();
            assert!(
                hinted > plain,
                "the hint row widens the tip: {hinted} vs {plain}"
            );
            tooltip_state(&harness).invoke_unhover();

            // (b) The absurd caption keeps the window cap.
            tooltip_state(&harness).invoke_hover(
                "an absurdly long tooltip label that cannot possibly fit a 1000 px window without wrapping somewhere long long long"
                    .into(),
                "".into(),
                ModelRc::default(),
                500.0,
                300.0,
                32.0,
                32.0,
            );
            harness.driver.advance_time(600);
            harness.driver.advance_time(0);
            assert!(
                tooltip_state(&harness).get_tip_width() <= 1000.0 * 0.6 + 0.5,
                "the cap holds: {}",
                tooltip_state(&harness).get_tip_width()
            );
            tooltip_state(&harness).invoke_unhover();

            // (c) A keys-less hover is not widened by an empty model:
            //     the width returns to the caption term.
            tooltip_state(&harness).invoke_hover(
                "short".into(),
                "".into(),
                ModelRc::default(),
                500.0,
                300.0,
                32.0,
                32.0,
            );
            harness.driver.advance_time(600);
            harness.driver.advance_time(0);
            assert_eq!(
                tooltip_state(&harness).get_tip_width(),
                plain,
                "the keys-less width matches the plain caption term"
            );
            tooltip_state(&harness).invoke_unhover();
        },
    );
}

/// Chunk 9 (design D15) — the palette journey: open with the empty
/// query floor, type a needle across boards (cursor walk + overflow
/// count), prove the anchor's absence structurally (a live matching
/// anchor missing from the row counts is the filter's proof), jump to
/// a unique hit (board + focus retarget), and close on Escape without
/// dispatching anything to the engine.
#[test]
fn the_palette_searches_and_jumps_across_boards() {
    let (fixture, mut state) = fixture::two_board_state();
    // Board one's first stack: the decoy + the anchor ride there.
    let stack_a = state
        .stacks
        .values()
        .find(|stack| stack.board == fixture.board_one)
        .expect("board one stack")
        .id;
    let plant =
        |state: &mut AppState, stack: taskboard_domain::StackId, order: i64, title: &str| {
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut task = state.tasks.values().next().expect("fixture task").clone();
            task.id = id;
            task.stack = stack;
            task.order = order;
            task.title = title.to_owned();
            state.tasks.insert(id, task);
        };
    // The unique needle lives on board two (the jump crosses boards);
    // order 5 puts it at canonical row 1 of stack "c".
    plant(&mut state, fixture.stack_c, 5, "zebra needle");
    plant(&mut state, stack_a, 5, "zebra decoy");
    plant(&mut state, stack_a, 6, taskboard_domain::ANCHOR_CARD_TITLE);
    for n in 0..9 {
        plant(
            &mut state,
            fixture.stack_c,
            10 + n,
            &format!("zebra extra {n}"),
        );
    }

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }

            // Open: the empty-query floor, end to end.
            harness.press(ActionId::PaletteSearch);
            assert!(window!().get_palette_open(), "the palette opened");
            assert_eq!(window!().get_palette_model().row_count(), 0);
            assert_eq!(window!().get_palette_overflow(), 0);
            harness.assert_commands(&[]);

            // "zeb" matches the needle, the decoy, and nine extras —
            // eleven candidates. The cap keeps 8; the overflow counts
            // the real remainder (the anchor does not match this
            // query, so nothing is filtered here).
            harness.type_text("zeb");
            assert_eq!(window!().get_palette_model().row_count(), 8);
            assert_eq!(window!().get_palette_overflow(), 3);

            // The cursor ring-walks: Down lands on row 1.
            harness.stroke(parse("Down").expect("down"));
            assert_eq!(window!().get_palette_selected_index(), 1);

            // Escape closes without effect: the view, the board, and
            // the engine boundary are untouched.
            harness.stroke(parse("Escape").expect("escape"));
            assert!(!window!().get_palette_open(), "Escape closed");
            assert_eq!(window!().get_current_view(), View::Board);
            harness.assert_commands(&[]);

            // The anchor proof: the anchor task is live and its title
            // matches "taskboard" as a subsequence — an unfiltered
            // count of 1 would be a visible row; zero rows IS the
            // presentation-side filter.
            harness.press(ActionId::PaletteSearch);
            assert!(window!().get_palette_open());
            harness.type_text("taskboard");
            assert_eq!(
                window!().get_palette_model().row_count(),
                0,
                "the matching anchor is filtered from the palette rows"
            );
            assert_eq!(window!().get_palette_overflow(), 0);
            harness.stroke(parse("Escape").expect("escape"));
            assert!(!window!().get_palette_open());

            // The jump: a query only the needle matches, Return
            // activates — the view retargets to board two and the
            // needle's card renders emphasized (canonical row 1 of
            // the single stack).
            harness.press(ActionId::PaletteSearch);
            harness.type_text("zebra needle");
            assert_eq!(window!().get_palette_model().row_count(), 1);
            harness.stroke(parse("Return").expect("return"));
            assert!(!window!().get_palette_open(), "the jump ends the flow");
            assert_eq!(window!().get_current_view(), View::Board);
            assert_eq!(
                window!().get_board_bar_model().selected,
                1,
                "the jump selected board two"
            );
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "the needle's card is emphasized"
            );
        },
    );
}
/// Round-3 D23 (R3-F35) — the show-task repro journey: open the
/// palette **from Today**, query a task that lives **on another
/// board**, Return. The board view mounts on the task's board, the
/// task's stack is selected, the card is emphasized (the pre-fix bug
/// left the board's first stack selected and the view unchanged), and
/// the `FocusScrollHint` global arms with the task's column/row so
/// the column `ListView` seeks. Sibling: activation from Settings.
#[test]
fn a_palette_activation_shows_the_task() {
    let (fixture, mut state) = fixture::two_board_state();
    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut needle = state.tasks.values().next().expect("fixture task").clone();
    needle.id = id;
    needle.stack = fixture.stack_c;
    needle.order = 5;
    needle.title = "zebra needle".to_owned();
    state.tasks.insert(id, needle);

    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }

            // From Today: the source view is part of the repro.
            harness.stroke(parse("Ctrl+T").expect("ctrl t"));
            assert_eq!(window!().get_current_view(), View::Today);

            // The palette round-trip: one unique hit, Return shows it.
            harness.press(ActionId::PaletteSearch);
            assert!(window!().get_palette_open());
            harness.type_text("zebra needle");
            assert_eq!(window!().get_palette_model().row_count(), 1);
            let before = window!()
                .global::<taskboard_ui_slint::FocusScrollHint>()
                .get_nonce();
            harness.stroke(parse("Return").expect("return"));

            assert!(!window!().get_palette_open(), "the jump ends the flow");
            assert_eq!(window!().get_current_view(), View::Board);
            assert_eq!(
                window!().get_board_bar_model().selected,
                1,
                "the task's board (board two) is selected"
            );
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "the needle's card is focused in its stack"
            );
            let hint = window!().global::<taskboard_ui_slint::FocusScrollHint>();
            assert!(
                hint.get_nonce() > before,
                "the retarget armed the scroll hint"
            );
            assert_eq!(
                (hint.get_target_a(), hint.get_target_b()),
                (0, 1),
                "column 0, canonical row 1 of the task's stack"
            );

            // From Settings: one semantics, no per-source branching —
            // the activation still mounts the board view on the
            // task's home.
            harness.press(ActionId::AppSettings);
            assert_eq!(window!().get_current_view(), View::Settings);
            harness.press(ActionId::PaletteSearch);
            harness.type_text("zebra needle");
            harness.stroke(parse("Return").expect("return"));
            assert_eq!(
                window!().get_current_view(),
                View::Board,
                "the view is always the task's home board"
            );
            assert_eq!(
                window!().get_board_bar_model().selected,
                1,
                "board two stayed selected"
            );
        },
    );
}

/// Round-3 D22 — the toolbar view switcher journey: the clicks drive
/// the same registry actions the keyboard uses (click ≡ key), the
/// pushed index tracks the mounted view in both directions
/// (reply-sourced), the settings round-trip shows the return view's
/// segment, the compact breakpoint keeps the switcher (it drops the
/// inbox chip, never the switcher), and a live board-create flow is
/// settled by a switcher click (the leaving rule).
#[test]
fn the_switcher_drives_and_tracks_the_view() {
    let (_fixture, state) = fixture::two_board_state();

    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }

            #[allow(clippy::items_after_statements)] // the run closure's helper
            fn segment(driver: &SlintDriver, label: &str) -> support::driver::Element {
                driver
                    .find_all_by_label(label)
                    .into_iter()
                    .find(support::driver::Element::is_radio_button)
                    .unwrap_or_else(|| panic!("the {label} segment is present"))
            }

            // The pushed index tracks the view from the mount on.
            assert_eq!(window!().get_switcher_index(), 0, "the board branch");

            // Click → action: Today. The segment's radio-button role
            // is the address (the visible text labels echo the same
            // string); AT activation is the click path here.
            segment(&harness.driver, "Today").invoke_default_action();
            assert_eq!(window!().get_current_view(), View::Today);
            harness.driver.advance_time(0);
            assert_eq!(window!().get_switcher_index(), 1, "today lit");

            // Keyboard equivalence: Alt+1 lands where the Board
            // segment click does, and the echo is reply-sourced.
            harness.stroke(parse("Alt+1").expect("alt 1"));
            assert_eq!(window!().get_current_view(), View::Board);
            harness.driver.advance_time(0);
            assert_eq!(window!().get_switcher_index(), 0);

            // Settings round-trip: the segment names the return view.
            harness.press(ActionId::AppSettings);
            assert_eq!(window!().get_current_view(), View::Settings);
            harness.driver.advance_time(0);
            assert_eq!(window!().get_switcher_index(), 0, "return reads board");
            segment(&harness.driver, "Today").invoke_default_action();
            assert_eq!(window!().get_current_view(), View::Today);
            harness.driver.advance_time(0);
            assert_eq!(window!().get_switcher_index(), 1);

            // Compact: the switcher survives the breakpoint that
            // drops the inbox chip.
            harness.driver.set_window_size(600, 400);
            assert!(window!().get_compact_active(), "600 px is compact");
            assert!(
                harness.driver.find_by_label("Today").is_some(),
                "the switcher is mounted in compact"
            );
            segment(&harness.driver, "Board").invoke_default_action();
            assert_eq!(window!().get_current_view(), View::Board);
            harness.driver.advance_time(0);
            assert_eq!(window!().get_switcher_index(), 0);
            harness.driver.set_window_size(1280, 800);

            // A live board-create flow settles under the switcher
            // click (the leaving rule): the field closes, nothing
            // dispatches, the switch happens.
            harness.press(ActionId::BoardCreate);
            harness.driver.advance_time(0);
            assert!(window!().get_board_create_open(), "the flow armed");
            segment(&harness.driver, "Today").invoke_default_action();
            harness.driver.advance_time(0);
            assert!(
                !window!().get_board_create_open(),
                "the leaving settle ended the flow"
            );
            assert_eq!(window!().get_current_view(), View::Today);
            harness.assert_commands(&[]);
        },
    );
}

/// Round-3 D24 (R3-F34) — the palette input row's geometry pin: with
/// zero rows the panel's height is exactly the input row (the field's
/// natural single-line height) plus the panel padding — the
/// full-screen-field regression's pin; with a full cap of rows the
/// panel stays content-sized and clamped above the status bar at the
/// minimum viewport.
#[test]
fn the_palette_input_row_stays_one_line() {
    let (_fixture, mut state) = fixture::two_board_state();
    let stack = *state.stacks.keys().next().expect("fixture stack");
    for n in 0..12 {
        let id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut task = state.tasks.values().next().expect("fixture task").clone();
        task.id = id;
        task.stack = stack;
        task.order = 10 + n;
        task.title = format!("zebra row {n}");
        state.tasks.insert(id, task);
    }

    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }

            #[allow(clippy::items_after_statements)] // the run closure's helper
            fn palette_panel(driver: &SlintDriver) -> support::driver::Element {
                driver
                    .find_all_by_id("AppWindow::palette-panel")
                    .into_iter()
                    .next()
                    .expect("the palette panel is mounted")
            }

            harness.press(ActionId::PaletteSearch);
            harness.driver.advance_time(0);
            // Zero rows: the panel is content-sized — the full-window
            // pane regression (the surplus distributing into the
            // field) cannot recur; the input row reads one line.
            let panel = palette_panel(&harness.driver);
            let field = harness
                .driver
                .find_all_by_label("Search")
                .into_iter()
                .next()
                .expect("the query field is addressable");
            let natural = f64::from(field.size().height);
            assert!(
                natural > 0.0 && natural < 100.0,
                "the input row is one line, not a pane"
            );
            let height = f64::from(panel.size().height);
            assert!(
                height < natural * 3.0,
                "the zero-row panel is content-sized ({height}), not a pane"
            );

            // Eight rows: content-driven, clamped above the status
            // bar, even at the minimum viewport.
            harness.type_text("zebra");
            harness.driver.advance_time(0);
            let panel = palette_panel(&harness.driver);
            assert_eq!(window!().get_palette_model().row_count(), 8);
            assert!(
                f64::from(panel.absolute_position().y + panel.size().height)
                    <= f64::from(window!().get_status_top()),
                "the panel stays above the status bar"
            );
        },
    );
}

// ---- round 2 (D16–D21): the action round-up journeys --------------------

/// Presses one action through `preset`'s own binding table (the
/// `Harness::press` pattern with a non-default keymap): resolves the
/// chain-visible binding the same way the generated menu does and
/// injects each stroke.
fn press_preset(
    harness: &mut Harness,
    preset: taskboard_ui_slint::keymap::Preset,
    action: ActionId,
) {
    let keymap = taskboard_ui_slint::keymap::Keymap::from_preset(preset);
    let chain = harness.chain();
    let hint = keymap
        .first_binding_display(action, &chain)
        .unwrap_or_else(|| panic!("{action:?} is bound in the visible chain {chain:?}"));
    for spelling in &hint {
        let stroke = stroke::parse(spelling)
            .unwrap_or_else(|error| panic!("{action:?}'s hint {spelling:?}: {error}"));
        let presses = harness.driver.press_stroke(stroke);
        harness.guard.press_n(presses);
    }
}

fn menu_rows(harness: &Harness) -> Vec<taskboard_ui_slint::MenuEntryModel> {
    let entries = keyboard(harness).get_menu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row"))
        .collect()
}

fn menu_actions(harness: &Harness) -> Vec<String> {
    menu_rows(harness)
        .into_iter()
        .map(|r| r.action.to_string())
        .collect()
}

/// The dispatch (c) contract, journey-level: every preset's move flow
/// cancels through Escape with zero commands drained; emacs' board-view
/// C-g is the cancel alternate (and inert outside a move), and its
/// `Ctrl+g Ctrl+/` sequence still resolves over the task-detail chain
/// (the resolver-level pin's echo at the window boundary).
#[test]
fn move_cancel_routes_through_escape_and_emacs_c_g() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let (fixture, state) = fixture::golden_state();
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness = Harness::mount_with_settings(
                    SlintDriver::default(),
                    state,
                    UiSettings {
                        keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                        keymap_preset: preset,
                        ..UiSettings::default()
                    },
                );
                harness.handle.window().invoke_board_task_clicked(0, 0);
                press_preset(&mut harness, preset, ActionId::TaskMove);
                assert!(
                    harness.handle.window().get_flow_hint_model().visible,
                    "{name}: the move flow is visible"
                );
                assert!(
                    harness.handle.window().get_board_model().move_target >= 0,
                    "{name}: the move target highlights"
                );
                harness.stroke(parse("Escape").expect("Escape"));
                assert!(
                    !harness.handle.window().get_flow_hint_model().visible,
                    "{name}: Escape ends the flow"
                );
                harness.assert_commands(&[]);

                // Extra pin (emacs): the board-view C-g is inert
                // outside a move and cancels inside one.
                if preset == taskboard_ui_slint::keymap::Preset::Emacs {
                    harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
                    harness.assert_commands(&[]);
                    press_preset(&mut harness, preset, ActionId::TaskMove);
                    harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
                    assert!(
                        !harness.handle.window().get_flow_hint_model().visible,
                        "the lone C-g cancels the move"
                    );
                    harness.assert_commands(&[]);
                }
                let _ = fixture;
            },
        );
    }
}

/// Round-3 D28 — the emacs double-C-g convention, end to end: from
/// the today chain (where the lone C-g never resolves), a live move
/// flow is cancelled by `C-g C-g` — the first C-g parks as the
/// pending prefix (the status signal), the second dispatches the
/// cancel; nothing moves and the flow is gone.
#[test]
fn emacs_double_c_g_cancels_from_the_today_chain() {
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            harness.stroke(parse("Ctrl+T").expect("Ctrl+T"));
            press_preset(&mut harness, preset, ActionId::NavNextItem); // seed a today row
            press_preset(&mut harness, preset, ActionId::TaskMove);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_flow_hint_model().visible,
                "the move flow armed"
            );

            // The convention: the first C-g parks (the prefix-abort
            // status signal rides the pending feedback), the second
            // cancels.
            harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_flow_hint_model().visible,
                "the lone C-g is only a pending prefix here"
            );
            harness.stroke(parse("Ctrl+g").expect("Ctrl+g"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_flow_hint_model().visible,
                "C-g C-g cancelled the move"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// The dispatch (d) contract: the stack walk wraps at both ends and
/// lands focus on the new stack's first task.
#[test]
fn the_stack_walk_wraps_at_both_ends() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Select the last stack first (the wrap-back check needs a
            // real end): the header click is the established seam.
            harness.handle.window().invoke_board_header_activated(1);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the header click selected stack b"
            );
            harness.press(ActionId::StackNext);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                0,
                "next at the last stack wraps to the first"
            );
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "focus lands on the wrapped stack's first task"
            );
            harness.press(ActionId::StackPrev);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "previous from the first stack wraps to the last"
            );
            harness.assert_commands(&[]);
            let _ = fixture;
        },
    );
}

/// The dispatch (e)/(f) contract: board create via the keyboard and
/// via the board-bar menu click ≡ key; board delete and stack delete
/// drain the exact typed commands; reopen routes the inverted done
/// flag and is a no-op dispatch on an open task.
#[test]
fn the_lifecycle_verbs_drain_their_exact_commands() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // Board create via the keyboard binding.
            harness.press(ActionId::BoardCreate);
            assert!(
                harness.handle.window().get_board_create_open(),
                "the board field opened"
            );
            harness.type_text("second");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::CreateBoard {
                title: "second".to_owned(),
                color: taskboard_domain::Color::new(""),
            }]);
            harness.engine.settle(&harness.handle);

            // Stack delete via the keyboard binding: the selection
            // (seeded to the first stack) is the target.
            harness.press(ActionId::StackDelete);
            harness.assert_commands(&[StateCommand::DeleteStack {
                id: fixture.stack_a,
            }]);
            harness.engine.settle(&harness.handle);

            // Reopen: focus a card, complete it, then invert it.
            harness.handle.window().invoke_board_task_clicked(0, 1);
            harness.driver.advance_time(0);
            harness.press(ActionId::TaskComplete);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: fixture.task1,
                done: true,
            }]);
            harness.engine.settle(&harness.handle);
            harness.press(ActionId::TaskReopen);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: fixture.task1,
                done: false,
            }]);
            harness.engine.settle(&harness.handle);
            // An open task absorbs the verb (NoTarget at the router).
            harness.press(ActionId::TaskReopen);
            harness.assert_commands(&[]);

            // Board delete via the keyboard binding: the exact verb,
            // the only live board the target (matched structurally —
            // the fixture does not expose the board id).
            harness.press(ActionId::BoardDelete);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), 1);
            assert!(
                matches!(drained[0], StateCommand::DeleteBoard { .. }),
                "board.delete drains DeleteBoard"
            );
        },
    );
}

/// The dispatch (g) contract: the first/last jumps land on the far
/// ends of the reading order and are idempotent at the target.
#[test]
fn the_first_last_jumps_land_on_the_far_ends_and_stay() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavLastItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((1, 0)),
                "last lands on the reading order's end"
            );
            harness.press(ActionId::NavLastItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((1, 0)),
                "repeating last is a stay"
            );
            harness.press(ActionId::NavFirstItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "first lands on the reading order's start"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// The stack-move flow: begin highlights the moving stack's slot,
/// cycling wraps over the insertion slots, confirm drains the exact
/// re-normalization set, and Escape mid-flow drains nothing.
#[test]
fn the_stack_move_flow_cycles_and_confirms_minimally() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Move stack a (selected, slot 0) after stack b (slot 1):
            // highlight the insertion point, confirm, drain the set.
            harness.handle.window().invoke_board_header_activated(0);
            harness.press(ActionId::StackMove);
            assert_eq!(
                harness.handle.window().get_board_model().move_target,
                0,
                "the flow highlights the moving stack's own slot"
            );
            harness.stroke(parse("Down").expect("Down"));
            assert_eq!(
                harness.handle.window().get_board_model().move_target,
                1,
                "the insertion point follows the cycle"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);
            harness.press(ActionId::StackMove);
            // Two Downs: slot 2 of 0..=2 = "append after the last
            // stack"; the highlight clamps to the last stack.
            harness.stroke(parse("Down").expect("Down"));
            harness.stroke(parse("Down").expect("Down"));
            assert_eq!(
                harness.handle.window().get_board_model().move_target,
                1,
                "the append slot highlights the last stack"
            );
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[
                StateCommand::MoveStack {
                    id: fixture.stack_b,
                    order: 0,
                },
                StateCommand::MoveStack {
                    id: fixture.stack_a,
                    order: 1,
                },
            ]);
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "stack a now renders at position 1"
            );

            // An in-place confirm dispatches nothing.
            harness.press(ActionId::StackMove);
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[]);
        },
    );
}

/// The dispatch (h) contract: right-clicking a stack header opens the
/// pointer-anchored menu with exactly the stack context's ids, every
/// bound row hinted, and an invocation routes against the clicked
/// object (Widen stack steps the width request).
#[test]
fn the_header_right_click_opens_the_stack_menu_and_routes() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness
                .handle
                .window()
                .invoke_board_header_context_requested(1, 300.0, 300.0);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());
            assert!(keyboard(&harness).get_menu_pointer_anchored());
            let rows = menu_rows(&harness);
            assert!(rows.iter().all(|r| !r.submenu), "flat: no parents");
            assert!(
                !menu_actions(&harness).contains(&"app.exit".to_owned()),
                "no Quit"
            );
            for action in [
                "task.create",
                "stack.move",
                "stack.rename",
                "stack.wider",
                "stack.narrower",
                "stack.delete",
            ] {
                assert!(
                    menu_actions(&harness).contains(&action.to_owned()),
                    "the stack menu offers {action}"
                );
            }
            for row in &rows {
                let keymap = UiSettings::default().keymap;
                if let Some(id) =
                    taskboard_ui_slint::actions::ActionId::from_id(row.action.as_str())
                {
                    let bound = keymap
                        .first_binding_display(
                            id,
                            &[
                                taskboard_ui_slint::actions::ContextId::Board,
                                taskboard_ui_slint::actions::ContextId::Global,
                            ],
                        )
                        .is_some();
                    if bound {
                        assert!(row.hint.row_count() > 0, "{} renders a hint", row.action);
                    }
                }
            }

            // Invoking Widen stack routes the width verb (no command,
            // a width request over the request lane).
            harness.driver.click_by_label("Widen stack");
            harness.assert_commands(&[]);
            let requests = harness.drain_requests();
            assert!(
                requests.iter().any(|r| matches!(
                    r,
                    taskboard_ui_slint::requests::UiRequest::StackWidthsChanged { .. }
                )),
                "the width verb stepped the persist request"
            );
            assert!(!keyboard(&harness).get_menu_open());
            // The clicked stack was selected first: the moved selection
            // addresses stack b on the next verb.
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "right-click selected the clicked column"
            );
            let _ = fixture;
        },
    );
}

/// Right-clicking a board-bar segment selects that board and opens
/// the board context menu (the recorded heavier-than-card semantics).
#[test]
fn the_segment_right_click_selects_and_opens_the_board_menu() {
    let (_fixture, state) = fixture::two_board_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness
                .handle
                .window()
                .invoke_board_segment_context_requested(1, 200.0, 200.0);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());
            for action in [
                "task.create",
                "stack.create",
                "board.create",
                "board.rename",
                "board.delete",
            ] {
                assert!(
                    menu_actions(&harness).contains(&action.to_owned()),
                    "the board menu offers {action}"
                );
            }
            assert!(
                !menu_actions(&harness).contains(&"app.exit".to_owned()),
                "no Quit"
            );
            // Right-click selected the clicked board (the board bar's
            // selected index follows).
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                1,
                "the clicked segment is now the selected board"
            );
        },
    );
}

/// The rename flows: the header menu's Rename stack opens the seeded
/// field inside the locked header; commit drains the exact command;
/// Escape discards. Board rename opens the window-level slot seeded
/// from the current title.
#[test]
fn the_rename_flows_seed_commit_and_discard() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // Stack rename via the header menu.
            harness
                .handle
                .window()
                .invoke_board_header_context_requested(0, 300.0, 300.0);
            harness.driver.click_by_label("Rename stack");
            harness.driver.advance_time(0);
            let stacks = harness.handle.window().get_board_model().stacks;
            let column = stacks.row_data(0).expect("column");
            assert!(column.rename_open, "the header field opened");
            assert_eq!(column.initial_title, "a", "the buffer seeded");
            // The seeded title starts selected (the task-editor
            // convention): typing replaces it wholesale.
            harness.type_text("renamed");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::RenameStack {
                id: fixture.stack_a,
                new_title: "renamed".to_owned(),
            }]);
            harness.engine.settle(&harness.handle);

            // Board rename via the board menu: seeded, Escape discards.
            harness
                .handle
                .window()
                .invoke_board_segment_context_requested(0, 200.0, 200.0);
            harness.driver.click_by_label("Rename board");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_board_rename_open());
            assert_eq!(
                harness.handle.window().get_board_flow_initial(),
                "board",
                "seeded from the current title"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);
            assert!(!harness.handle.window().get_board_rename_open());
        },
    );
}

/// R2 D23 — the checkmark commit affordance: clicking it dispatches
/// exactly what Return dispatches (click ≡ key at the observed
/// boundary, §7.1), and it stays inert on an empty buffer.
#[test]
fn the_commit_affordance_click_equals_return_on_the_title_field() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Into the title editor: the field mounts focused with its
            // seed selected.
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit never auto-arms; the editor arms via the
            // `task.title-edit` action.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);

            // (a) The click path: type a fresh title, invoke Commit.
            harness.type_text("clicked");
            harness.driver.click_by_label("Commit");
            let clicked = harness.engine.drain();
            assert!(
                matches!(&clicked[..], [StateCommand::UpdateTask { changes, .. }]
                    if changes.title.as_deref() == Some("clicked")),
                "the click route dispatched the title commit, got {clicked:?}"
            );

            // (b) The key path: the same buffer through Return produces
            // the identical command (equality is the pin). The editor
            // re-opens from the board, focus still on the same task.
            harness.press(ActionId::NavFocusBoard);
            harness.press(ActionId::TaskEdit);
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit never auto-arms; the editor arms via the
            // `task.title-edit` action.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);
            harness.type_text("clicked");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(clicked.as_slice());
        },
    );
}

/// R2 D23 — the affordance renders only while a non-empty buffer
/// holds focus: an emptied field offers no Commit element and the
/// check seam cannot fire (nothing dispatched).
#[test]
fn the_commit_affordance_is_inert_on_an_empty_buffer() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            // Recorded decision (round-3 chunk 1, plan §6 — F26):
            // task.edit never auto-arms; the editor arms via the
            // `task.title-edit` action.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);
            harness.driver.advance_time(0);
            assert!(
                harness.driver.find_by_label("Commit").is_some(),
                "a populated focused field offers the affordance"
            );

            // Empty the seeded selection: the affordance unmounts.
            harness.stroke(parse("Backspace").expect("Backspace"));
            harness.driver.advance_time(0);
            assert!(
                harness.driver.find_by_label("Commit").is_none(),
                "an empty buffer hides the affordance"
            );
            harness.assert_commands(&[]);
        },
    );
}

/// R2 D23 — the create-row sites share the primitive: the stack
/// create field's `Commit` click lands the same `CreateStack` command the
/// Return path produces (one widget, one seam — second data point).
#[test]
fn the_commit_affordance_click_equals_return_on_the_stack_create_row() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::StackCreate);
            harness.type_text("s");
            harness.driver.click_by_label("Commit");
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateStack { title, order, .. }] => {
                    assert_eq!(title.as_str(), "s");
                    assert_eq!(*order, 2, "the same command shape as Return");
                }
                other => panic!("the click route's command reached the boundary, got {other:?}"),
            }
        },
    );
}

/// R2 D24 — a parse-missed custom offset still dispatches nothing
/// (the D12 absorb) but now resets the buffer: the field reads empty
/// afterwards, so it never looks stuck. The buffer is typed through
/// the real path — the field focused, the entry committed by Return —
/// because the seam's `false` return is what drives the
/// composition's own clear. A parse hit routes `ReminderRuleAdd`
/// unchanged (the regression pin).
#[test]
fn a_parse_missed_custom_offset_clears_its_buffer() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            harness.driver.advance_time(0);

            let field = |harness: &Harness| {
                harness
                    .driver
                    .find_by_label("Custom reminder offset")
                    .expect("the custom-offset field is mounted")
            };
            // A pointer click is the field's focus seam (no AT default
            // action on a text input, and nothing auto-focuses it).
            let focus = |harness: &mut Harness| {
                let field = field(harness);
                let pos = field.absolute_position();
                let size = field.size();
                harness
                    .driver
                    .click_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            };
            assert_eq!(
                field(&harness).value().as_deref(),
                Some(""),
                "the field mounts empty"
            );

            // A parse miss: nothing at the boundary, buffer reset.
            focus(&mut harness);
            harness.type_text("soon");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[]);
            assert_eq!(
                field(&harness).value().as_deref(),
                Some(""),
                "the reject reset the buffer"
            );

            // A parse hit routes unchanged.
            focus(&mut harness);
            harness.type_text("45m");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::AddReminderRule {
                task: fixture.task1,
                offset: taskboard_domain::ReminderOffset::before(
                    TimeDelta::try_minutes(-45).expect("-45m"),
                )
                .expect("legal offset"),
                at: None,
                insistence: taskboard_domain::Insistence::Normal,
            }]);
        },
    );
}

// ---- r4 chunk 6 (D18): the activation stops + the walk ------------------

/// The task view's walk-stop fact, read back through the generated
/// global.
fn walk_stop(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::TaskViewWalkFacts>()
        .get_stop_id()
}

/// Walks `steps` control steps, returning the stop fact observed after
/// each (the reading-order inventory).
fn walk_steps(harness: &mut Harness, steps: usize) -> Vec<i32> {
    let mut observed = Vec::with_capacity(steps);
    for _ in 0..steps {
        if harness.chain().contains(&ContextId::ViInsert) {
            // An engaged field masks the view context: the walk
            // stroke is the native Tab (unbound in the insert chain).
            harness.stroke(parse("Tab").expect("tab"));
        } else {
            press_preset(
                harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextControl,
            );
        }
        harness.driver.advance_time(0);
        observed.push(walk_stop(harness));
    }
    observed
}

/// r4 D18 (F12, acceptance a) — the structural stop inventory: the
/// activation stops report their ids in reading order under repeated
/// control walk, the non-stop interactables (header `IconButton`s, the
/// custom-offset field) occupy exactly one step without a report, and
/// the walk wraps; the open-editor branches replace their cell with
/// the field (one stop, no double report).
#[test]
fn the_task_view_stop_inventory_walks_in_reading_order() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextItem,
            );
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::TaskEdit,
            );
            harness.driver.advance_time(0);
            assert_eq!(walk_stop(&harness), 0, "no stop report before the walk");

            // The reading-order prefix: back (unreported), done, title,
            // tracking (unreported), due, scheduled, chips, the
            // custom-offset field (unreported), description. The wrap
            // then cycles through the window chrome (unreported stops)
            // and re-enters at the done box — the wrap pin.
            // The entries affordance (r4 chunk 10, D-TS8) joins the
            // unreported stops (the tracking verbs' precedent): it
            // sits after the scheduled cell, so the stale-repeat
            // window carries one more 6 before the rules rows.
            let observed = walk_steps(&mut harness, 20);
            assert_eq!(
                &observed[..13],
                &[0, 2, 3, 3, 5, 6, 6, 90, 91, 92, 93, 93, 11],
                "the reading-order inventory"
            );
            assert_eq!(
                observed[19], 2,
                "the walk wraps through the chrome and re-enters at the done box"
            );

            // The open-title branch: the field replaces the cell —
            // one stop, no report, and the later cells keep their
            // order.
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().edit_open,
                "the editor is armed for the open-branch leg"
            );
            // The field is the cell's one stop: the walk leaves the
            // focused field and the next REPORTED stop is the title
            // cell (the field itself and the branch's Chrome never
            // re-report 3 while the field is mounted).
            let observed = walk_steps(&mut harness, 8);
            assert_eq!(
                &observed[..7],
                &[2, 2, 2, 2, 2, 2, 2],
                "no cell re-report while the field holds the branch"
            );
            assert_eq!(observed[7], 3, "the walk's next report is the title cell");

            // The open-due branch: the DueEditor replaces the cell.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            // Walk back to the due cell and open it.
            let mut guard = 0;
            while walk_stop(&harness) != 5 && guard < 20 {
                press_preset(
                    &mut harness,
                    taskboard_ui_slint::keymap::Preset::Standard,
                    ActionId::NavNextControl,
                );
                harness.driver.advance_time(0);
                guard += 1;
            }
            assert_eq!(walk_stop(&harness), 5, "the walk reached the due cell");
            harness
                .driver
                .find_by_label("Edit due date")
                .expect("the due cell is present")
                .invoke_default_action();
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the due editor is armed for the open-branch leg"
            );
            // The editor's field is the cell's one stop: under the
            // engaged field the walk stroke is the native Tab (the
            // insert chain leaves it unbound); the editor's own
            // affordances occupy the steps between it and the next
            // reported cell — the scheduled cell.
            let mut guard = 0;
            while walk_stop(&harness) != 6 && guard < 12 {
                harness.stroke(parse("Tab").expect("tab"));
                harness.driver.advance_time(0);
                guard += 1;
            }
            assert_eq!(
                walk_stop(&harness),
                6,
                "the scheduled cell follows the editor"
            );
        },
    );
}

/// r4 D18 §2 (acceptance d; chunk 7's entry seam) — the cell
/// activation matrix: bare Return/Space on the focused cell activate
/// the cell's own seam under every preset — emacs Return never
/// resolves `task.title-edit` onto the cell, vi Return never
/// `task.edit`-activates and vi Space never completes — and the
/// reservation's prefix discipline holds: an emacs pending prefix
/// aborts on Return and consumes it (the feed-first ordering).
#[test]
#[allow(clippy::too_many_lines)] // the matrix's rows are the contract's length
fn the_focused_cell_activates_on_return_and_space_under_any_preset() {
    let matrix: Vec<(taskboard_ui_slint::keymap::Preset, i32, &'static str)> = vec![
        (taskboard_ui_slint::keymap::Preset::Standard, 3, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 6, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 11, "Return"),
        (taskboard_ui_slint::keymap::Preset::Standard, 90, "Return"),
        (taskboard_ui_slint::keymap::Preset::Emacs, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Emacs, 5, "Space"),
        (taskboard_ui_slint::keymap::Preset::Vi, 5, "Return"),
        (taskboard_ui_slint::keymap::Preset::Vi, 5, "Space"),
    ];
    for (preset, stop, stroke_spelling) in matrix {
        let (fixture, state) = fixture::golden_state();
        let task0 = fixture.task0;
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness = Harness::mount_with_settings(
                    SlintDriver::default(),
                    state,
                    UiSettings {
                        keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                        keymap_preset: preset,
                        ..UiSettings::default()
                    },
                );
                press_preset(&mut harness, preset, ActionId::NavNextItem);
                press_preset(&mut harness, preset, ActionId::TaskEdit);
                harness.driver.advance_time(0);
                let mut guard = 0;
                while walk_stop(&harness) != stop && guard < 20 {
                    press_preset(&mut harness, preset, ActionId::NavNextControl);
                    harness.driver.advance_time(0);
                    guard += 1;
                }
                assert_eq!(walk_stop(&harness), stop, "the walk reached the cell");
                harness.stroke(parse(stroke_spelling).expect(stroke_spelling));
                harness.driver.advance_time(0);
                let task_model = harness.handle.window().get_task_model();
                match stop {
                    // The title cell arms the title editor.
                    3 => assert!(task_model.edit_open, "the title editor armed"),
                    // The due cell opens the due editor — under emacs
                    // the two-phase entry seam, under vi neither the
                    // `task.edit` activate nor the done toggle.
                    5 => {
                        assert!(task_model.due_open, "the due editor opened");
                        assert!(!task_model.done, "the done state never moved");
                    }
                    // The scheduled cell opens the start-date editor.
                    6 => assert!(task_model.scheduled_open, "the editor opened"),
                    // The description cell opens the editor.
                    11 => assert!(
                        task_model.description_edit_open,
                        "the description editor opened"
                    ),
                    // A chip adds its preset offset (5m).
                    90 => match harness.engine.drain().as_slice() {
                        [StateCommand::AddReminderRule { task, offset, .. }] => {
                            assert_eq!(*task, task0);
                            assert_eq!(
                                *offset,
                                taskboard_domain::ReminderOffset::before(
                                    TimeDelta::try_minutes(-5).expect("-5m")
                                )
                                .expect("legal offset")
                            );
                        }
                        other => panic!("the chip added its preset, got {other:?}"),
                    },
                    other => panic!("unreachable matrix row {other}"),
                }
            },
        );
    }

    // The done box (stop 2): Return toggles the subject's done state
    // — the checkbox row keeps its own semantics under the
    // reservation.
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Vi;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            press_preset(&mut harness, preset, ActionId::NavNextItem);
            press_preset(&mut harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            let mut guard = 0;
            while walk_stop(&harness) != 2 && guard < 20 {
                press_preset(&mut harness, preset, ActionId::NavNextControl);
                harness.driver.advance_time(0);
                guard += 1;
            }
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: task0,
                done: true,
            }]);
        },
    );

    // The prefix discipline: an emacs pending prefix (C-x) aborts on
    // Return and consumes it — the cell does not activate, and the
    // next bare Return does.
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            press_preset(&mut harness, preset, ActionId::NavNextItem);
            press_preset(&mut harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);
            let mut guard = 0;
            while walk_stop(&harness) != 5 && guard < 20 {
                press_preset(&mut harness, preset, ActionId::NavNextControl);
                harness.driver.advance_time(0);
                guard += 1;
            }
            harness.stroke(parse("Ctrl+x").expect("Ctrl+x"));
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the pending prefix consumed the Return: no activation"
            );
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the bare Return activates the cell"
            );
        },
    );
}

// ---- r4 chunk 6 (D20): the focus-driven insert engagement --------------

/// Click-focuses the element at `label`'s centre (a text input has no
/// AT default action; the pointer press is the field's focus seam —
/// the parse-miss journey's helper shape).
fn click_field_centre(harness: &mut Harness, label: &str) {
    let element = harness
        .driver
        .find_by_label(label)
        .unwrap_or_else(|| panic!("the field {label:?} is present"));
    let (x, y) = support::driver::centre(&element);
    harness.driver.click_at(x, y);
    // The focus-change notification (and so the D20 publish) rides
    // the clock's handler pass.
    harness.driver.advance_time(0);
}

/// The edited-facts singleton, read back through the generated global.
fn text_focus(harness: &Harness) -> i32 {
    harness
        .handle
        .window()
        .global::<taskboard_ui_slint::EditingFacts>()
        .get_text_focus_count()
}

/// r4 D20 — the Return-never-hijacks matrix (F24, plan §6.C): every
/// preset against the four field sites. A focused text field masks
/// the view context, so the preset-bound Return reaches the field's
/// own commit — never arms a view flow. The emacs column is the F24
/// repro verbatim (Return resolved `task.title-edit` mid-typing).
#[test]
#[allow(clippy::too_many_lines)] // the matrix's four field legs are the contract's length
fn return_never_hijacks_on_a_focused_field_under_any_preset() {
    let presets = [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ];
    for (preset, name) in presets {
        let settings = move || UiSettings {
            keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
            keymap_preset: preset,
            ..UiSettings::default()
        };

        // -- the custom reminder offset field (the repro verbatim) --
        let (fixture, state) = fixture::golden_state();
        let task0 = fixture.task0;
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness =
                    Harness::mount_with_settings(SlintDriver::default(), state, settings());
                press_preset(&mut harness, preset, ActionId::NavNextItem);
                press_preset(&mut harness, preset, ActionId::TaskEdit);
                harness.driver.advance_time(0);
                click_field_centre(&mut harness, "Custom reminder offset");
                harness.type_text("45m");
                harness.stroke(parse("Return").expect("return"));
                match harness.engine.drain().as_slice() {
                    [StateCommand::AddReminderRule { task, offset, .. }] => {
                        assert_eq!(*task, task0, "{name}: the subject's rule");
                        assert_eq!(
                            *offset,
                            taskboard_domain::ReminderOffset::before(
                                TimeDelta::try_minutes(-45).expect("-45m")
                            )
                            .expect("legal offset")
                        );
                    }
                    other => panic!("{name}: the offset committed, got {other:?}"),
                }
                assert!(
                    !harness.handle.window().get_task_model().edit_open,
                    "{name}: no view flow armed over the field"
                );
                assert!(
                    keyboard(&harness).get_vi_insert(),
                    "{name}: the field's focus masks the chain"
                );
            },
        );

        // -- the title field --
        let (_fixture, state) = fixture::golden_state();
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness =
                    Harness::mount_with_settings(SlintDriver::default(), state, settings());
                press_preset(&mut harness, preset, ActionId::NavNextItem);
                press_preset(&mut harness, preset, ActionId::TaskEdit);
                harness.driver.advance_time(0);
                harness.driver.click_by_label("Edit title");
                harness.driver.advance_time(0);
                harness.type_text("x");
                harness.stroke(parse("Return").expect("return"));
                assert!(
                    !harness.handle.window().get_task_model().edit_open,
                    "{name}: the commit closed the editor"
                );
                match harness.engine.drain().as_slice() {
                    [StateCommand::UpdateTask { changes, .. }] => {
                        assert_eq!(changes.title.as_deref(), Some("x"));
                    }
                    other => panic!("{name}: the title committed, got {other:?}"),
                }
            },
        );

        // -- the description field --
        let (_fixture, state) = fixture::golden_state();
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness =
                    Harness::mount_with_settings(SlintDriver::default(), state, settings());
                press_preset(&mut harness, preset, ActionId::NavNextItem);
                press_preset(&mut harness, preset, ActionId::TaskEdit);
                harness.driver.advance_time(0);
                harness.driver.click_by_label("Edit description");
                harness.driver.advance_time(0);
                // D-MLE-6 (the D1 field-level redefinition): plain
                // Return inserts a newline and never commits; the
                // commit seam is Shift+Return.
                harness.type_text("hello");
                harness.stroke(parse("Return").expect("return"));
                harness.driver.advance_time(0);
                assert!(
                    harness
                        .handle
                        .window()
                        .get_task_model()
                        .description_edit_open,
                    "{name}: plain Return never closed the editor"
                );
                assert!(
                    harness.engine.drain().is_empty(),
                    "{name}: plain Return dispatched nothing"
                );
                harness.type_text("world");
                harness.stroke(parse("Shift+Return").expect("shift+return"));
                assert!(
                    !harness
                        .handle
                        .window()
                        .get_task_model()
                        .description_edit_open,
                    "{name}: the commit closed the editor"
                );
                match harness.engine.drain().as_slice() {
                    [StateCommand::UpdateTask { changes, .. }] => {
                        assert_eq!(changes.description.as_deref(), Some("hello\nworld"));
                    }
                    other => panic!("{name}: the description committed, got {other:?}"),
                }
            },
        );

        // -- the today timers' timing field --
        let (_fixture, state) = fixture::golden_state();
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness =
                    Harness::mount_with_settings(SlintDriver::default(), state, settings());
                press_preset(&mut harness, preset, ActionId::ViewToday);
                assert_eq!(current_view(&harness), View::Today);
                click_field_centre(&mut harness, "Timer label");
                harness.type_text("Tea");
                click_field_centre(&mut harness, "Timer timing (in 10m or at 09:30)");
                harness.type_text("in 3m");
                harness.stroke(parse("Return").expect("return"));
                match harness.engine.drain().as_slice() {
                    [StateCommand::CreateAdhocAlarm { label, when, .. }] => {
                        assert_eq!(label.as_str(), "Tea", "{name}");
                        assert_eq!(
                            *when,
                            taskboard_domain::AdhocWhen::In(TimeDelta::minutes(3)),
                            "{name}"
                        );
                    }
                    other => panic!("{name}: the timer created, got {other:?}"),
                }
                assert_eq!(
                    current_view(&harness),
                    View::Today,
                    "{name}: Return never hijacked into a task view"
                );
            },
        );
    }
}

/// r4 D20 — the palette exception (plan §3.5): the palette's own chain
/// branch precedes the mode check, so Return is the jump under every
/// preset — never a field commit, never a hijack.
#[test]
fn the_palette_return_jumps_under_every_preset() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let (fixture, mut state) = fixture::two_board_state();
        let id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut task = state.tasks.values().next().expect("fixture task").clone();
        task.id = id;
        task.stack = fixture.stack_c;
        task.order = 5;
        task.title = "zebra needle".to_owned();
        state.tasks.insert(id, task);
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness = Harness::mount_with_settings(
                    SlintDriver::default(),
                    state,
                    UiSettings {
                        keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                        keymap_preset: preset,
                        ..UiSettings::default()
                    },
                );
                press_preset(&mut harness, preset, ActionId::PaletteSearch);
                assert!(
                    harness.handle.window().get_palette_open(),
                    "{name}: the palette opened"
                );
                harness.type_text("zebra needle");
                harness.stroke(parse("Return").expect("return"));
                assert!(
                    !harness.handle.window().get_palette_open(),
                    "{name}: the jump closed the palette"
                );
                assert_eq!(
                    harness.handle.window().get_board_bar_model().selected,
                    1,
                    "{name}: the jump selected the needle's board"
                );
                harness.assert_commands(&[]);
            },
        );
    }
}

/// r4 D20 — the focus-engage deltas the per-mount seams could not
/// do: the custom offset field's Space is a space once the field
/// holds focus (vi's `task.complete` never toggles over an engaged
/// field), and the whole engagement rides the click-focus — zero
/// `edited` events involved anywhere.
#[test]
fn focus_alone_engages_and_reserves_the_field_strokes() {
    let (fixture, state) = fixture::golden_state();
    let task0 = fixture.task0;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Vi;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            press_preset(&mut harness, preset, ActionId::NavNextItem);
            press_preset(&mut harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);

            // Space over the click-focused field inserts, never
            // toggles the subject's done state.
            click_field_centre(&mut harness, "Custom reminder offset");
            harness.type_text("4");
            harness.stroke(parse("Space").expect("space"));
            harness.type_text("5m");
            let field = harness
                .driver
                .find_by_label("Custom reminder offset")
                .expect("the field is mounted");
            assert_eq!(
                field.value().as_deref(),
                Some("4 5m"),
                "the Space reached the field's buffer"
            );
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().done,
                "the done state never toggled over the field"
            );

            // Return commits (the parse hit), still no done toggle:
            // clear the spaced buffer the way a user retypes.
            for _ in 0..4 {
                harness.stroke(parse("Backspace").expect("backspace"));
            }
            harness.type_text("45m");
            harness.stroke(parse("Return").expect("return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::AddReminderRule { task, .. }] => assert_eq!(*task, task0),
                other => panic!("the offset committed, got {other:?}"),
            }
        },
    );
}

/// r4 D20 — the count lifecycle over the generated global: the
/// publish rides focus (click alone), the A→B transition stays at one
/// through the unspecified handler order, the flow end resets both
/// legs, and the view bindings resolve again after the unmask (the
/// emacs Return re-arms the title — the unmask pin).
#[test]
fn the_editing_facts_count_lifecycle_tracks_focus() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let preset = taskboard_ui_slint::keymap::Preset::Emacs;
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                    keymap_preset: preset,
                    ..UiSettings::default()
                },
            );
            press_preset(&mut harness, preset, ActionId::NavNextItem);
            press_preset(&mut harness, preset, ActionId::TaskEdit);
            harness.driver.advance_time(0);

            // Gain: the click alone publishes.
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            assert_eq!(text_focus(&harness), 1, "the focused field published");

            // The empty commit closes the flow: the flow-end reset
            // zeroes the count and the insert mirror.
            for _ in 0..7 {
                harness.stroke(parse("Backspace").expect("backspace"));
            }
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().edit_open,
                "the empty commit closed the editor"
            );
            assert_eq!(text_focus(&harness), 0, "the flow end reset the count");
            assert!(
                !keyboard(&harness).get_vi_insert(),
                "the unmask dropped the insert mirror"
            );

            // Unmask: the view chain resolves again — emacs Return
            // re-arms the title editor (the field re-publishes).
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().edit_open,
                "the unmasked Return resolved the view binding"
            );

            // A→B: the armed title flow settles through Escape, then
            // the due editor's field mounts and publishes — the count
            // tracks each field's own focus, never a stranded state.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the due click armed the editor"
            );
            assert!(
                text_focus(&harness) >= 1,
                "the newly mounted editor field published"
            );

            // Escape settles the flow; the reset seam zeroes again.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            assert_eq!(text_focus(&harness), 0, "the settle reset the count");
        },
    );
}

/// R2 D24 — the preset chips carry the design's padding tokens: each
/// chip's painted width is at least the space-6 floor at scale 1.0
/// (the label no longer touches the chip edges).
#[test]
fn the_preset_chips_hold_their_token_floor() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.advance_time(0);
            // The id query reports each repetition through every
            // ancestor chain; dedup by painted geometry.
            let mut seen: Vec<(f32, f32)> = Vec::new();
            let chips = harness
                .driver
                .find_all_by_id("RuleList::preset-chip")
                .into_iter()
                .filter(|chip| {
                    let pos = chip.absolute_position();
                    let key = (pos.x, pos.y);
                    let fresh = !seen.contains(&key);
                    if fresh {
                        seen.push(key);
                    }
                    fresh
                })
                .collect::<Vec<_>>();
            assert_eq!(chips.len(), 4, "the fixture's four preset chips");
            for chip in chips {
                assert!(
                    chip.size().width >= 32.0, // Theme.space-6 at scale 1
                    "every chip is at least the space-6 floor, got {}",
                    chip.size().width
                );
            }
        },
    );
}

/// R2 D24 — the custom-offset entry stretches into the presets row's
/// leftover: its painted width lands well past the primitive's token
/// floor — where it would sit if the row sized every cell to its
/// preferred width (the stretch default is load-bearing).
#[test]
fn the_custom_offset_field_stretches_into_the_presets_row() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.advance_time(0);
            let field = harness
                .driver
                .find_by_label("Custom reminder offset")
                .expect("the custom-offset field is mounted");
            assert!(
                field.size().width > 160.0, // the 96 px floor plus the row's leftover
                "the field takes the row's leftover, got {}",
                field.size().width
            );
        },
    );
}

/// Round-3 D25 — the control walk reaches the picker: Tab out of the
/// armed due field lands on the calendar grid (the native traversal —
/// Tab is unbound in the insert chain), the grid's focus seam arms
/// the `DatePicker` context (the pushed mirror reads true), the
/// preset's keys drive the grid (picker.right moves the cursor a
/// day), and Escape still closes the editor cleanly — the context
/// never masks. On close the context disarms (the flow-end belt).
#[test]
fn the_control_walk_reaches_fields_buttons_and_the_picker() {
    let (fixture, mut state) = fixture::journey_state();
    // The walk's subject: one NavNextItem focuses stack "a"'s FIRST
    // task, so the due is seeded there — the picker opens anchored on
    // it (today's fallback would be wall-clock-dependent). January
    // 2030 starts on a Tuesday: one leading blank puts the 30th at
    // flat grid index 30 on every calendar day.
    let due = local_midnight(2030, 1, 30);
    state
        .tasks
        .get_mut(&fixture.task0)
        .expect("fixture task")
        .duedate = Some(due);
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().date_picker_open,
                "the picker is default-open; the field keeps focus"
            );
            let grid_armed = |harness: &Harness| {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_date_picker_grid()
            };
            assert!(
                !grid_armed(&harness),
                "the context is disarmed while the field holds focus"
            );

            let cursor_index = |harness: &Harness| -> Option<usize> {
                let grid = harness.handle.window().get_task_model().date_grid;
                let mut flat = 0usize;
                for r in 0..grid.row_count() {
                    let row = grid.row_data(r).expect("row");
                    for c in 0..row.row_count() {
                        if row.row_data(c).expect("cell").cursor {
                            return Some(flat);
                        }
                        flat += 1;
                    }
                }
                None
            };
            // The seeded due is January 30, 2030: one Monday-first
            // leading blank (the month opens on a Tuesday) puts the
            // 30th at flat index 30.
            assert_eq!(cursor_index(&harness), Some(30), "the due anchors");

            // The walk enters the picker: Down-in-field hands focus
            // to the grid (the chunk-3 focus-on-mount seam) and the
            // focus seam arms the context — the same arming a Tab
            // traversal onto the grid produces.
            harness.stroke(parse("Down").expect("Down"));
            harness.driver.advance_time(0);
            assert!(
                grid_armed(&harness),
                "the grid took focus and armed the context"
            );

            // The preset's keys drive the grid: picker.right moves
            // the cursor one day (January 2030 stays on-grid).
            harness.press(ActionId::PickerRight);
            harness.driver.advance_time(0);
            assert_eq!(cursor_index(&harness), Some(31), "one day forward");

            // The context never masks: Escape resolves ModeNormal
            // from ViInsert — the editor's clean close.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().date_picker_open,
                "the editor closed clean"
            );
            assert!(!grid_armed(&harness), "the context disarmed with the flow");
            harness.assert_commands(&[]);
        },
    );
}

/// Round-3 D26 — the scroll actions page the focused area: the
/// settings pane pages adapter-side through the two-way facts and
/// clamps at both ends (the end of content IS the save row's
/// reachability), the board column hint arms for the focused column,
/// and the task detail — no container — is a typed no-op.
#[test]
fn scroll_actions_page_the_focused_area() {
    let (_fixture, state) = fixture::two_board_state();

    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }
            macro_rules! scroll_hint {
                ($h:expr) => {
                    $h.handle
                        .window()
                        .global::<taskboard_ui_slint::ScrollPageHint>()
                };
            }

            // -- settings: pages to the end and clamps at both ends --
            harness.press(ActionId::AppSettings);
            harness.driver.advance_time(0);
            macro_rules! scroll_facts {
                ($h:expr) => {
                    $h.handle
                        .window()
                        .global::<taskboard_ui_slint::SettingsScrollFacts>()
                };
            }
            let viewport = scroll_facts!(harness).get_viewport_height();
            let content = scroll_facts!(harness).get_content_height();
            assert!(
                content > viewport,
                "precondition: the settings pane scrolls"
            );
            harness.press(ActionId::ViewScrollDown);
            harness.driver.advance_time(0);
            let after_one = scroll_facts!(harness).get_viewport_y();
            assert!(after_one > 0.0, "one page advanced the pane");

            // Page to the end: the clamp is content − viewport (the
            // save row's reachability), and further pages stay there.
            for _ in 0..20 {
                harness.press(ActionId::ViewScrollDown);
                harness.driver.advance_time(0);
            }
            let max = scroll_facts!(harness).get_viewport_y();
            assert!(
                (max - (content - viewport)).abs() < 1.0,
                "the pane clamped at the end of content"
            );
            harness.press(ActionId::ViewScrollDown);
            harness.driver.advance_time(0);
            assert_eq!(
                scroll_facts!(harness).get_viewport_y(),
                max,
                "the clamp holds"
            );

            // Page up clamps at the top.
            for _ in 0..20 {
                harness.press(ActionId::ViewScrollUp);
                harness.driver.advance_time(0);
            }
            assert_eq!(
                scroll_facts!(harness).get_viewport_y(),
                0.0,
                "the top clamp"
            );

            // -- board: the focused task's column pages --
            harness.press(ActionId::NavFocusBoard);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::ViewScrollDown);
            harness.driver.advance_time(0);
            let h = scroll_hint!(harness);
            assert!(h.get_nonce() > 0, "the focused column's page hint armed");
            assert_eq!(h.get_family(), 1, "the board-column family");
            assert_eq!(h.get_page(), 1, "the down direction");

            // -- task detail: no container, a typed no-op --
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(window!().get_current_view(), View::TaskDetail);
            let nonce_before = scroll_hint!(harness).get_nonce();
            harness.press(ActionId::ViewScrollDown);
            harness.driver.advance_time(0);
            assert_eq!(
                scroll_hint!(harness).get_nonce(),
                nonce_before,
                "the containerless detail hints nothing"
            );
        },
    );
}

/// Round-3 D27 — the menu curates itself: the `ViewNav` page rides the
/// re-rank (the go-to verbs above the cycle, read off the pushed
/// submenu model by action id — ids are addresses, never label
/// text), the greying flips with state (the move-cancel row is
/// disabled without a flow and enabled when a move flow holds), and
/// the root page's parent rows shrink by the preset-irrelevant Mode
/// group (structural: the parent count over the standard preset).
#[test]
#[allow(clippy::too_many_lines)] // the curation walk is the contract's length
fn the_menu_curates_itself() {
    let (_fixture, state) = fixture::two_board_state();

    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            macro_rules! window {
                () => {
                    harness.handle.window()
                };
            }

            harness.press(ActionId::NavNextItem); // seed a focus
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);

            macro_rules! keyboard {
                () => {
                    window!().global::<taskboard_ui_slint::KeyboardState>()
                };
            }
            let find = |model: &slint::ModelRc<taskboard_ui_slint::MenuEntryModel>,
                        action: &str| {
                (0..model.row_count())
                    .map(|i| model.row_data(i).expect("row"))
                    .find(|row| row.action == action)
                    .unwrap_or_else(|| panic!("the {action} row is on the page"))
            };
            // Walk to a parent row and open it (the cursor walks the
            // root page; the first parent is the Task group).
            macro_rules! open_first_parent {
                ($h:expr) => {{
                    let opened = loop {
                        let k = keyboard!();
                        let entries = k.get_menu_entries();
                        #[allow(clippy::cast_sign_loss)] // the getter's −1 sentinel
                        let index = k.get_menu_selected_index().max(0) as usize;
                        if let Some(row) = entries.row_data(index) {
                            if row.submenu {
                                break true;
                            }
                        }
                        #[allow(clippy::cast_possible_truncation)] // page-bounded walk
                        #[allow(clippy::cast_possible_wrap)]
                        if i32::try_from(index).is_ok_and(|i| i > 24) {
                            break false;
                        }
                        $h.stroke(parse("Down").expect("Down"));
                        $h.driver.advance_time(0);
                    };
                    if opened {
                        $h.stroke(parse("Return").expect("Return"));
                        $h.driver.advance_time(0);
                    }
                    opened
                }};
            }

            // The Mode parent is filtered under the standard preset:
            // five parents ride the root page (D27b).
            let root = keyboard!().get_menu_entries();
            let parents = (0..root.row_count())
                .filter(|i| root.row_data(*i).expect("row").submenu)
                .count();
            assert_eq!(parents, 5, "the Mode parent is filtered (D27b)");

            // The greying flip: without a move flow the Task page's
            // cancel row reads disabled.
            assert!(open_first_parent!(harness), "a parent row opened");
            let task_page = keyboard!().get_menu_submenu_entries();
            assert!(
                !find(&task_page, "task.move.cancel").enabled,
                "the cancel row greys without a flow"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(!keyboard!().get_menu_open(), "the menu closed");

            // The ViewNav page's order: the go-to verbs above the
            // cycle (D27a). The ViewNav parent is the fourth of the
            // five rendered parents.
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            let mut parent_ordinal = 0;
            for _ in 0..20 {
                let k = keyboard!();
                let entries = k.get_menu_entries();
                #[allow(clippy::cast_sign_loss)] // the getter's −1 sentinel
                let index = k.get_menu_selected_index().max(0) as usize;
                let row = entries.row_data(index).expect("row");
                if row.submenu {
                    parent_ordinal += 1;
                    if parent_ordinal == 4 {
                        break;
                    }
                }
                harness.stroke(parse("Down").expect("Down"));
                harness.driver.advance_time(0);
            }
            assert_eq!(parent_ordinal, 4, "the ViewNav parent is the fourth");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            let submenu = keyboard!().get_menu_submenu_entries();
            let index = |action: &str| {
                (0..submenu.row_count())
                    .map(|i| submenu.row_data(i).expect("row"))
                    .position(|row| row.action == action)
                    .unwrap_or_else(|| panic!("the {action} row rides the page"))
            };
            assert!(index("view.today") < index("nav.next-view"));
            assert!(index("nav.focus-board") < index("nav.next-view"));
            assert!(index("nav.next-view") < index("nav.next-control"));

            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(!keyboard!().get_menu_open(), "the menu closed");

            // The flip's reachable half: with a live move flow the
            // cancel row reads enabled (the menu-open settle cannot
            // coexist with the flow, so the enabled side rides the
            // entry_states pin at the adapter layer).
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("M").expect("M")); // task.move
            harness.driver.advance_time(0);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            assert!(open_first_parent!(harness), "a parent row opened");
            let task_page = keyboard!().get_menu_submenu_entries();
            assert!(
                !find(&task_page, "task.move.cancel").enabled,
                "opening the menu settled the flow (F9): the cancel row greys again"
            );
        },
    );
}

/// R2 D25 — the due editor's typed path: ISO commits at 00:00 UTC,
/// `+N<unit>` commits relative to now (bracketed by the journey's own
/// clock reads), an empty accept clears, and a refusal dispatches
/// nothing while keeping the buffer.
#[test]
#[allow(clippy::too_many_lines)] // the walk is the contract's length
fn the_typed_due_entries_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 31);
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(due);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // One walk into task1's detail; the editor re-opens by
            // its cell after every commit (the view stays put).
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            let open_editor = |harness: &mut Harness| {
                assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
                harness.driver.click_by_label("Edit due date");
                harness.driver.advance_time(0);
                assert!(
                    keyboard(harness).get_vi_insert(),
                    "the editor opened in insert mode"
                );
            };

            // ISO: commits at the date's midnight UTC.
            open_editor(&mut harness);
            harness.clear_seeded();
            harness.type_text("2030-02-28");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 2, 28))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // Round-3 D11: opening seeds the buffer with the set
            // value — Return untouched re-commits the SAME value (the
            // accidental clear is dead; the engine treats it as a
            // content no-op, blessed by the acceptance).
            open_editor(&mut harness);
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 1, 31))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // The x-clear stages the clear; Return commits it
            // (`Some(None)`).
            open_editor(&mut harness);
            harness
                .driver
                .find_by_label("Clear")
                .expect("the x affordance is mounted")
                .invoke_default_action();
            harness.driver.advance_time(0);
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // Relative: now + 2d, bracketed by the journey's own reads.
            open_editor(&mut harness);
            harness.clear_seeded();
            let before = chrono::Utc::now();
            harness.type_text("+2d");
            harness.stroke(parse("Return").expect("Return"));
            let after = chrono::Utc::now();
            let drained = harness.engine.drain();
            match drained.as_slice() {
                [StateCommand::UpdateTask { id, changes, .. }] => {
                    assert_eq!(id, &fixture.task1);
                    let offset = changes
                        .duedate
                        .expect("the due arm is Some(_)")
                        .expect("a set, not a clear")
                        - before;
                    assert!(
                        offset
                            >= TimeDelta::try_days(2).unwrap() - TimeDelta::try_seconds(1).unwrap()
                            && offset <= TimeDelta::try_days(2).unwrap() + (after - before),
                        "+2d lands inside the journey's clock bracket, got {offset}"
                    );
                }
                other => panic!("the relative commit reached the boundary, got {other:?}"),
            }

            // A refusal dispatches nothing and keeps the buffer.
            open_editor(&mut harness);
            harness.clear_seeded();
            harness.type_text("junk");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[]);
            let field = harness
                .driver
                .find_by_label("Due date entry")
                .expect("the editor stays open on a refusal");
            assert_eq!(
                field.value().as_deref(),
                Some("junk"),
                "the refused buffer is kept"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);
        },
    );
}

/// Round-3 D8 (acceptance b) — scheduled parity: the start-date cell
/// opens the *same* editor over the other field; a typed ISO reaches
/// the boundary as `scheduled: Some(Some(_))`, the x-clear + Return
/// as `Some(None)`, and `duedate` stays untouched throughout.
#[test]
fn the_scheduled_editor_reaches_the_boundary_with_parity() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .scheduled = Some(local_midnight(2030, 3, 15));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit start date");
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_vi_insert(),
                "the editor opened in insert mode"
            );

            // Typed ISO lands on `scheduled`, `duedate` untouched.
            harness.clear_seeded();
            harness.type_text("2030-04-01");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(local_midnight(2030, 4, 1))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // The x-clear stages the clear; Return commits `Some(None)`.
            harness.driver.click_by_label("Edit start date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
        },
    );
}

/// R4 D23 (F19) — the steppers work from an empty time: a date-only
/// picker seeds midnight on the first backward step (23:45 the
/// previous day, the readout leaves `--:--`), and the first forward
/// step lands 00:15; the eventual commit carries the stepped instant.
#[test]
fn steppers_seed_and_step_from_empty() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_time_text,
                "--:--",
                "a midnight anchor stays date-only — no phantom time"
            );

            // The first backward step seeds midnight and wraps to the
            // previous day's 23:45; the readout follows.
            harness.handle.window().invoke_task_date_picker_key(9);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_time_text,
                "23:45",
                "the seed made a time exist and the step wrapped"
            );

            // The commit carries the stepped instant — cursor date
            // followed the wrap backward. (Grid-Enter no longer
            // commits — r4 D25's hand-over; the combobox funnel does.)
            harness
                .handle
                .window()
                .invoke_task_date_time_accepted("23:45".into());
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_wall(2030, 1, 30, 23, 45))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // A fresh editor: the first forward step from empty is
            // 00:15 the same day.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.handle.window().invoke_task_date_picker_key(10);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_time_text,
                "00:15",
                "the seed stepped forward"
            );
        },
    );
}

/// R4 D24 (F20) — the click-away rule on the date fields: a
/// parseable non-empty buffer commits (the same funnel as Return),
/// junk and empty discard (no command, the prior value survives, the
/// editor collapses). Both date fields ride the same rule.
#[test]
#[allow(clippy::too_many_lines)] // the rule's three legs, both fields
fn click_away_commits_parseable_and_discards_junk() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            // (a) Junk: the click-away discards — no command, the
            // editor collapses, the prior due survives a reopen.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.type_text("soon");
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "junk cannot linger — the editor collapsed"
            );
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().due_text.as_str(),
                "2030-01-31",
                "the prior value re-seeded"
            );
            // The two-press ladder out: step-back to the grid, then
            // the clean close — leg (b) starts from a disarmed editor.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the ladder closed the editor"
            );

            // (b) Parseable: the click-away commits the same command
            // Return would.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.type_text("2030-02-14");
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 2, 14))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the editor collapsed on the commit"
            );

            // (c) Empty: the click-away never means clear — no
            // command, the due survives.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.driver.click_by_label("Clear");
            harness.driver.advance_time(0);
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the editor collapsed on the empty discard"
            );
            // The absence of a command IS the survival: empty on
            // focus-loss never means clear (a structural fact, not a
            // state-timing read).

            // (d) The start-date field rides the same rule: junk
            // discards (its seeded buffer is empty — typing is the
            // whole entry).
            harness.driver.click_by_label("Edit start date");
            harness.driver.advance_time(0);
            harness.type_text("soon");
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().scheduled_open,
                "the scheduled editor collapsed"
            );
            harness.driver.click_by_label("Edit start date");
            harness.driver.advance_time(0);
            harness.type_text("2030-03-01");
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    scheduled: Some(Some(local_midnight(2030, 3, 1))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
        },
    );
}

/// R4 D27 (F27) — the date field's tri-state validity flips with the
/// authoritative parse: junk reads invalid, a parseable entry valid,
/// the untouched field neutral. The model property is the oracle —
/// never the label text.
#[test]
fn the_date_field_validity_flips_with_the_parse() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_validity,
                taskboard_ui_slint::FieldValidity::Valid,
                "the seeded buffer parses"
            );
            harness.clear_seeded();
            harness.type_text("soon");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_validity,
                taskboard_ui_slint::FieldValidity::Invalid
            );
            harness.clear_seeded();
            harness.type_text("2030-02-14");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_validity,
                taskboard_ui_slint::FieldValidity::Valid
            );
        },
    );
}

/// R4 D27 (F27) — the custom-offset and timers-`when` fields ride
/// their authoritative grammars' validity: junk reads invalid, a
/// parseable entry valid, untouched neutral — model reads, never
/// label text.
#[test]
fn the_offset_and_timer_fields_flip_validity_with_their_grammars() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);

            // The custom-offset field on the task detail.
            harness
                .handle
                .window()
                .invoke_rule_offset_edited("soon".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().offset_validity,
                taskboard_ui_slint::FieldValidity::Invalid
            );
            harness
                .handle
                .window()
                .invoke_rule_offset_edited("45m".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().offset_validity,
                taskboard_ui_slint::FieldValidity::Valid
            );
            harness.handle.window().invoke_rule_offset_edited("".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().offset_validity,
                taskboard_ui_slint::FieldValidity::Neutral
            );

            // The timers form's `when` field on the today view.
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            harness
                .handle
                .window()
                .invoke_timers_when_edited("soon".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_timer_when_validity(),
                taskboard_ui_slint::FieldValidity::Invalid
            );
            harness
                .handle
                .window()
                .invoke_timers_when_edited("in 10m".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_timer_when_validity(),
                taskboard_ui_slint::FieldValidity::Valid
            );
            harness.handle.window().invoke_timers_when_edited("".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_timer_when_validity(),
                taskboard_ui_slint::FieldValidity::Neutral
            );
        },
    );
}

/// Chunk-9 mount regression — the section's `when` field rides the
/// pushed validity *through the mount*: typing into the rendered
/// field (not the window callback) flips the `TodayView` root flavour.
/// The adapter seam is pinned by the journey above; this one pins the
/// wiring — the mount's forward was never wired and the seam journey
/// could not see it.
#[test]
fn typing_into_the_timers_section_drives_the_when_validity() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            let click_field = |harness: &mut Harness, label: &str| {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("the field {label:?} is present"));
                let (x, y) = support::driver::centre(&element);
                harness.driver.click_at(x, y);
            };
            click_field(&mut harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("soon");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_timer_when_validity(),
                taskboard_ui_slint::FieldValidity::Invalid,
                "typed junk flips the flavour through the mount"
            );
            // No Clear affordance on this field (it is opt-in): the
            // retype leg deletes through the same stroke path.
            let backspace = parse("Backspace").expect("Backspace");
            for _ in 0.."soon".chars().count() {
                harness.stroke(backspace);
            }
            harness.type_text("in 10m");
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_timer_when_validity(),
                taskboard_ui_slint::FieldValidity::Valid,
                "parseable text re-flips through the mount"
            );
        },
    );
}

/// R4 D24 (chunk-6 acceptance e) — the foreign-flow guard: a focus
/// steal into the description field while the due editor is armed
/// settles the due per the rule and leaves the description's own
/// engage flow intact.
#[test]
fn a_foreign_field_steal_settles_the_due_per_the_rule() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);

            // Junk in the due field: the steal discards — no command,
            // the due editor collapses, the description's engage flow
            // is untouched.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.type_text("soon");
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the due editor collapsed on the steal"
            );
            assert!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "the description's own engage flow is intact"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "the description closed again"
            );

            // A parseable buffer: the steal commits the due.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.type_text("2030-03-01");
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 3, 1))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
            assert!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "the description's engage flow still armed"
            );
        },
    );
}

/// R4 D26 (F22, review focus d) — the combobox is the picker's only
/// time path: the typed filter seeds the picker live (and junk shows
/// invalid without touching it), a quick-pick renders the canonical
/// value AND re-renders the due buffer, and combobox-Enter commits
/// the editor through the one funnel.
#[test]
fn the_combobox_is_the_only_time_path() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);
            assert_eq!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .date_time_field_text,
                "--:--",
                "the unset echo is the value mirror"
            );
            assert_eq!(
                harness.handle.window().get_task_model().date_time_validity,
                taskboard_ui_slint::FieldValidity::Neutral
            );

            // Typed filter: a lenient-parse hit seeds the picker live
            // and reads valid.
            harness
                .handle
                .window()
                .invoke_task_date_time_edited("9".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_time_validity,
                taskboard_ui_slint::FieldValidity::Valid
            );
            assert_eq!(
                harness.handle.window().get_task_model().date_time_text,
                "09:00",
                "the typed hour seeded the picker"
            );

            // Junk: the validity flips invalid and the picker's time
            // is untouched.
            harness
                .handle
                .window()
                .invoke_task_date_time_edited("9x".into());
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_task_model().date_time_validity,
                taskboard_ui_slint::FieldValidity::Invalid
            );
            assert_eq!(
                harness.handle.window().get_task_model().date_time_text,
                "09:00",
                "junk never rewrote the seeded time"
            );

            // A quick-pick: canonical value in the mirror, and the
            // due buffer re-renders from the selection (Picker-source
            // sync).
            harness
                .handle
                .window()
                .invoke_task_date_time_picked(9 * 60 + 30);
            harness.driver.advance_time(0);
            assert_eq!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .date_time_field_text,
                "09:30",
                "the pick wrote the canonical rendering"
            );
            assert_eq!(
                harness.handle.window().get_task_model().due_text,
                "2030-01-31 09:30",
                "the buffer mirrors the picker's selection"
            );

            // Combobox-Enter commits the editor through the funnel.
            harness
                .handle
                .window()
                .invoke_task_date_time_accepted("09:45".into());
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_wall(2030, 1, 31, 9, 45))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the editor collapsed on the combobox commit"
            );
        },
    );
}

/// Round-5 F8/F10 (the mouse-only quick-time session): with the picker
/// opened through the field's own affordances (no keyboard), the
/// quick-times chevron opens the quick list without settling the
/// armed editor — the picker stays open — and a set time still seeds
/// the FULL 96-slot list at open (the typed-digit narrowing is armed
/// by keystrokes, not by the value mirror).
#[test]
fn the_quick_time_button_opens_the_menu_without_closing_the_picker() {
    let (fixture, mut state) = fixture::journey_state();
    // The timed value is what makes both halves red: the settle ends
    // the whole flow on the chevron's focus steal (F8), and the value
    // mirror filters the list to the one matching slot (F10).
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_wall(2030, 1, 31, 14, 30));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);

            // Mouse only: the chevron click must not settle the armed
            // editor — the picker stays open under the quick list.
            harness.driver.click_by_label("Quick times");
            harness.driver.advance_time(0);
            // The second loop turn delivers the deferred anchor-geometry
            // mirror (the mount pass ran in the first — the nonce
            // bump's recorded delivery shape).
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().date_picker_open,
                "the quick-time click leaves the picker open"
            );
            {
                let facts = harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::TimeFieldFacts>();
                assert!(facts.get_open(), "the quick list flyout is open");
                // Below the anchor: the realized panel sits under the
                // field row (the clamp/flip grammar's default leg).
                assert!(
                    facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                    "the flyout mounts below its anchor (panel-y={:?} anchor-y={:?} anchor-h={:?} anchor-x={:?})",
                    facts.get_panel_y(),
                    facts.get_anchor_y(),
                    facts.get_anchor_height(),
                    facts.get_anchor_x()
                );
            }
            // The full 96-slot list at open — a set time must not seed
            // the one-row mirror-filtered list.
            let slots = harness.handle.window().get_task_model().date_time_slots;
            assert_eq!(
                slots.row_count(),
                96,
                "the quick list opens unfiltered over the quarter-hour grid"
            );
        },
    );
}

/// Round-5 F10's other half: typed-digit narrowing keeps working per
/// keystroke after an open that seeded the full list — the first
/// keystroke re-arms the digit filter over the value mirror.
#[test]
fn the_quick_list_narrows_per_keystroke_after_an_open_with_a_set_time() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_wall(2030, 1, 31, 14, 30));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);
            harness.driver.click_by_label("Quick times");
            harness.driver.advance_time(0);
            let slots = harness.handle.window().get_task_model().date_time_slots;
            assert_eq!(slots.row_count(), 96, "the open seeds the full list");

            // The first keystroke narrows again: the mirror's digits
            // ("1430" + the typed text) filter the grid per keystroke.
            harness
                .handle
                .window()
                .invoke_task_date_time_edited("143".into());
            harness.driver.advance_time(0);
            let slots = harness.handle.window().get_task_model().date_time_slots;
            let narrowed = slots.row_count();
            assert!(
                narrowed > 0 && narrowed < 96,
                "typed digits narrow the list (got {narrowed})"
            );
        },
    );
}

/// R4 D25 — the calendar picker's two-phase ladder and live sync
/// (reworks the r2 boundary journey's Enter leg): grid-Enter hands
/// focus to the field and dispatches nothing, the typed path and
/// day-click keep their commands, the Escape ladder steps field →
/// grid → close without a command until the close, and the buffer
/// mirrors cursor moves while the grid holds focus.
#[allow(clippy::too_many_lines)] // the walk is the contract's length
#[test]
fn the_due_picker_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 31);
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(due);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().date_picker_open,
                "the picker is default-open on edit entry (round-3 D10)"
            );

            // January 2030: one leading blank (Tuesday start), 31
            // days, padded to the six-row reserve — 42 cells in 6
            // rows (r4 D22); the cursor sits on the due (flattened
            // index 1 + 30).
            let cursor_index = |harness: &Harness| -> Option<usize> {
                let grid = harness.handle.window().get_task_model().date_grid;
                let mut flat = 0usize;
                for r in 0..grid.row_count() {
                    let row = grid.row_data(r).expect("row");
                    for c in 0..row.row_count() {
                        if row.row_data(c).expect("cell").cursor {
                            return Some(flat);
                        }
                        flat += 1;
                    }
                }
                None
            };
            let grid_shape = |harness: &Harness| -> (usize, usize) {
                let grid = harness.handle.window().get_task_model().date_grid;
                let rows = grid.row_count();
                let cells: usize = (0..rows)
                    .map(|r| grid.row_data(r).expect("row").row_count())
                    .sum();
                (rows, cells)
            };
            assert_eq!(grid_shape(&harness), (6, 42));
            assert_eq!(cursor_index(&harness), Some(31));

            // Down-in-field hands focus to the grid (the one-shot
            // focus-on-mount; round-3 D10): the arrows drive from
            // here. Right follows the month (round-3 D12/F28): the
            // caption flips to February and the cursor stays on-grid
            // — Feb 1 2030 is a Friday (4 leading blanks, flattened
            // 4), 28 days padded to the six-row reserve (42 cells).
            // Left returns
            // to the due's January grid.
            harness.stroke(parse("Down").expect("Down"));
            harness.driver.advance_time(0);
            // Tab from the grid lands on the header's first month
            // button (the widget-level tab stop); Return activates it
            // (round-3 D12/F28, acceptance f): the month steps back —
            // December 2029 starts on a Saturday (5 leading blanks,
            // 31 days padded to 42 cells in 6 rows), the cursor on
            // the 31st (flattened 35).
            harness.stroke(parse("Tab").expect("Tab"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            assert_eq!(
                grid_shape(&harness),
                (6, 42),
                "the month button answered Return — and every month \
                renders the same six-row reserve (r4 D22)"
            );
            assert_eq!(cursor_index(&harness), Some(35));
            // Back to January for the legs below.
            harness.handle.window().invoke_task_date_picker_key(5);
            harness.driver.advance_time(0);
            harness.stroke(parse("Down").expect("Down"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Right").expect("Right"));
            harness.driver.advance_time(0);
            assert_eq!(grid_shape(&harness), (6, 42));
            assert_eq!(
                cursor_index(&harness),
                Some(4),
                "Feb 1 stays on-grid — the displayed month follows the cursor"
            );
            harness.stroke(parse("Left").expect("Left"));
            harness.driver.advance_time(0);
            assert_eq!(grid_shape(&harness), (6, 42));
            assert_eq!(cursor_index(&harness), Some(31));

            // PageDown: February 2030, the cursor clamped to the 28th
            // (leading 4 + 27 = flattened 31 again, 35 cells).
            harness.stroke(parse("PageDown").expect("PageDown"));
            harness.driver.advance_time(0);
            assert_eq!(grid_shape(&harness), (6, 42));
            assert_eq!(cursor_index(&harness), Some(31));

            // Grid-Enter is the phase hand-over, never a commit
            // (r4 D25, review focus b): the field takes focus (the
            // one-shot flag), nothing dispatches, and the buffer
            // mirrors the cursor's date (the Picker-source live sync).
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                harness.handle.window().get_task_model().date_field_focus,
                "the hand-over flagged the field"
            );
            assert_eq!(
                harness.handle.window().get_task_model().due_text.as_str(),
                "2030-02-28",
                "the buffer mirrors the cursor"
            );
            // The field's Return now commits the same command the
            // typed path produces for the same date.
            harness.stroke(parse("Return").expect("Return"));
            let picked = vec![StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 2, 28))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }];
            harness.assert_commands(picked.as_slice());

            // The typed path for the same date: the identical command.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.clear_seeded();
            harness.type_text("2030-02-28");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(picked.as_slice());

            // A day-cell click ≡ cursor+Enter: the 15th commits.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.handle.window().invoke_task_date_day_clicked(1 + 14);
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 1, 15))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // The Escape ladder (r4 D25): the field phase's Escape is
            // the phase step back — the grid takes focus, the buffer
            // survives, nothing dispatches; the second Escape (the
            // picker phase) is the clean close, no command, the editor
            // collapses (the default-open picker dies with it). A
            // reopen starts default-open again, and the cycle leaves
            // the view closable by its own back binding (R-c).
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                keyboard(&harness).get_date_picker_grid(),
                "the first Escape stepped back to the grid"
            );
            assert!(
                harness.handle.window().get_task_model().due_open,
                "the field phase's Escape keeps the editor armed"
            );
            assert_eq!(
                harness.handle.window().get_task_model().due_text.as_str(),
                "2030-01-31",
                "the seeded buffer survived the step back"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);
            assert!(!keyboard(&harness).get_vi_insert(), "back to normal");
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the editor collapsed"
            );
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_task_model().date_picker_open,
                "a reopen starts default-open — the picker is part of \
                 the editor's armed affordance"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the view's back binding still works after the cycle"
            );
        },
    );
}

/// Round-3 D10 (acceptance d, review focus c) — the overlay: the
/// picker mounts at window level and pushes nothing (the metadata
/// row's y is unchanged while it is open), and the dismissal layer's
/// outside click drops the picker only — the editor stays armed with
/// its seeded buffer, nothing dispatches, and Escape still ends the
/// whole editor.
#[test]
fn the_picker_overlay_dismisses_without_disturbing_the_editor() {
    let (fixture, mut state) = fixture::journey_state();
    state
        .tasks
        .get_mut(&fixture.task1)
        .expect("fixture task")
        .duedate = Some(local_midnight(2030, 1, 31));
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);
            let caption = |harness: &Harness| {
                harness
                    .driver
                    .find_by_label("Due date")
                    .expect("the metadata caption is mounted")
                    .absolute_position()
                    .y
            };
            let y_open = caption(&harness);

            // Outside click (r4 D24, superseding the round-3 D10
            // picker-only rule): the editor settles per the rule — the
            // seeded buffer is parseable, so click-away ≡ done: the
            // same instant re-commits and the editor collapses.
            harness.driver.click_at(600.0, 450.0);
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().date_picker_open,
                "the outside click dismissed the picker"
            );
            assert_eq!(
                caption(&harness),
                y_open,
                "the metadata row's y is unchanged — the picker pushes nothing"
            );
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(local_midnight(2030, 1, 31))),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
            assert!(
                !harness.handle.window().get_task_model().due_open,
                "the editor collapsed on the settle"
            );
        },
    );
}

/// Round-3 D15/#228 (acceptance i) — the description edits in place:
/// the read-only body's click arms the multiline field (the seeded
/// buffer mirrors the current text), Return commits
/// `description: Some(_)` (the multiline Enter probe: no newline
/// inserts), Escape discards without dispatching.
#[test]
fn the_description_edits_in_place() {
    let (fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_vi_insert(),
                "the editor opened in insert mode"
            );

            // Typed text + Shift+Return commits `Some(_)` through
            // the funnel; the plain Return in the middle inserted a
            // newline (the D-MLE-6 grammar) and dispatched nothing.
            // The fixture's description is empty, so the seeded
            // buffer starts empty: typing is the whole entry.
            harness.type_text("steep, then");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[]);
            assert!(
                harness
                    .handle
                    .window()
                    .get_task_model()
                    .description_edit_open,
                "plain Return never closed the editor"
            );
            harness.type_text("serve");
            harness.stroke(parse("Shift+Return").expect("Shift+Return"));
            let drained = harness.engine.drain();
            match drained.as_slice() {
                [StateCommand::UpdateTask { id, changes, .. }] => {
                    assert_eq!(id, &fixture.task1);
                    let description = changes
                        .description
                        .as_deref()
                        .expect("the description arm is Some(_)");
                    assert_eq!(description, "steep, then\nserve", "the newline persisted");
                }
                other => panic!("the description commit reached the boundary, got {other:?}"),
            }

            // Escape discards without dispatching.
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            harness.type_text("half-typed");
            harness.stroke(parse("Escape").expect("Escape"));
            harness.assert_commands(&[]);
            assert!(
                !keyboard(&harness).get_vi_insert(),
                "back to normal after the discard"
            );
        },
    );
}

/// Round-5 chunk 2 (F7, D-MLE-6): a multi-line buffer commits
/// verbatim through the existing funnel — the newlines survive the
/// round trip with zero Rust-side change (verified, not touched).
#[test]
fn the_multiline_buffer_commits_verbatim() {
    let (fixture, mut state) = fixture::journey_state();
    let task = state.tasks.get_mut(&fixture.task1).expect("fixture task");
    task.description = "first\nsecond".into();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit description");
            harness.driver.advance_time(0);
            // No typing: the seeded buffer commits as-is.
            harness.stroke(parse("Shift+Return").expect("Shift+Return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { id, changes, .. }] => {
                    assert_eq!(id, &fixture.task1);
                    assert_eq!(
                        changes.description.as_deref(),
                        Some("first\nsecond"),
                        "the newlines survived the round trip"
                    );
                }
                other => panic!("the verbatim commit reached the boundary, got {other:?}"),
            }
        },
    );
}

/// Round-5 chunk 2 (adoption wiring, F12): the display mount is the
/// editor (a labelled text-input cell, not a bare text row) — the
/// viewport's scroll mechanics themselves are pinned at the widget
/// layer (the probe's P-arms); the journey asserts only the wiring,
/// the lowest layer that can see the adoption.
#[test]
fn the_description_display_mounts_the_editor() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            let display = harness
                .driver
                .find_by_label("Description")
                .expect("the description cell is mounted");
            assert!(
                !display.is_accessible_button(),
                "the display mount is a text-input cell"
            );
        },
    );
}

/// The appended legs' view read (the earlier journeys' local helper
/// duplicated once for the chunk-10 block).
fn current_view(harness: &Harness) -> View {
    harness.handle.window().get_current_view()
}

/// Chunk 10 D12 — the today leg: the desktop mount stays on Today; the
/// section walk follows `today_tasks` in display order with the
/// emphasis tracking; the walk retargets the board selection
/// (selection-follows-focus); `task.complete` dispatches the exact
/// `SetTaskDone`; the re-commit verb writes a fresh
/// `duedate = Some(Some(_))` confirmation-free; `nav.focus-board`
/// escapes. Rollover is NOT here — it is the models suite's
/// stepped-`now` push proof.
#[test]
fn the_today_view_walks_completes_and_recommits() {
    // SAFETY: single-threaded journey startup; the driver thread reads
    // it only inside `run` below.
    unsafe { std::env::set_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY", "1") };
    let (_fixture, state, [board_one_task, _board_two_task]) = fixture::today_overdue_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // The desktop default: Today, no escape.
            assert_eq!(current_view(&harness), View::Today);
            assert_eq!(
                harness.handle.window().get_toolbar_title(),
                "Today",
                "the today view captions itself"
            );

            // The walk takes `today_tasks` display order: board one's
            // overdue row, then board two's (order, id) — with the
            // emphasis following.
            harness.press(taskboard_ui_slint::actions::ActionId::NavNextItem);
            let today = harness.handle.window().get_today_model();
            let overdue = today.sections.row_data(0).expect("overdue section");
            assert_eq!(overdue.count, 2, "both overdue rows render");
            assert!(
                overdue.rows.row_data(0).expect("row").card.emphasis,
                "the walk enters at the first row"
            );
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                0,
                "selection-follows-focus stays on board one"
            );

            harness.press(taskboard_ui_slint::actions::ActionId::NavNextItem);
            assert!(
                overdue.rows.row_data(1).expect("row").card.emphasis,
                "the emphasis follows the walk"
            );
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                1,
                "the walk retargets the board to board two"
            );

            // Back to the first row; complete it (the exact command).
            harness.press(taskboard_ui_slint::actions::ActionId::NavPrevItem);
            harness.press(taskboard_ui_slint::actions::ActionId::TaskComplete);
            harness.assert_commands(&[StateCommand::SetTaskDone {
                id: board_one_task,
                done: true,
            }]);
            // The fake engine applies the drained command on settle
            // (the ten-step's apply discipline).
            harness.engine.settle(&harness.handle);
            let today = harness.handle.window().get_today_model();
            let done_section = today.sections.row_data(3).expect("done section");
            assert_eq!(done_section.count, 1, "done-placement");

            // The re-commit verb (no default stroke; the menu path's
            // action): a fresh double-option due write, free of
            // confirmations.
            let before = harness.engine.drain().len();
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskDueToday);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), before + 1);
            assert!(
                matches!(
                    &drained[before],
                    StateCommand::UpdateTask {
                        id,
                        changes: taskboard_domain::TaskChanges {
                            duedate: Some(Some(_)),
                            ..
                        },
                    } if *id == board_one_task
                ),
                "the re-commit stamps the day window's end"
            );

            // The escape: back to the kanban board.
            harness.press(taskboard_ui_slint::actions::ActionId::NavFocusBoard);
            assert_eq!(current_view(&harness), View::Board);
        },
    );
}

/// Chunk 10 D12 — the triage leg: `view.inbox` jumps to the Inbox's
/// canonical-first stack; the move ring cycles ACROSS the board edge
/// (the deliberate behavior change) with the hint naming the target;
/// confirming moves; the chip then reads zero (triage-done).
#[test]
fn the_inbox_triage_moves_across_the_board_edge() {
    // SAFETY: single-threaded journey startup (see the set site).
    unsafe { std::env::remove_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY") };
    let (fixture, state, caught) = fixture::triage_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // The chip counts the inbox from any view (desktop-only).
            assert_eq!(
                harness.handle.window().get_toolbar_inbox_count(),
                1,
                "one open inbox row"
            );

            // view.inbox jumps (read-side; StackDetail on the
            // canonical-first stack). No default stroke (D11) — the
            // chip click and the menu route the same action.
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::ViewInbox);
            assert_eq!(current_view(&harness), View::StackDetail);

            harness.press(taskboard_ui_slint::actions::ActionId::NavNextItem);
            // The move ring: cycle once — across the board edge onto
            // board two's stack "c" (the deliberate behavior change).
            // task.move has no StackDetail stroke (the preset table's
            // shape) — the menu path's action routes it. The Inbox
            // stack sits last in the flat ring, so one step BACKWARD
            // crosses the board edge onto board two.
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskMove);
            harness.press(taskboard_ui_slint::actions::ActionId::NavPrevItem);
            let hint = harness.handle.window().get_flow_hint_model().text;
            assert!(
                hint.contains('\u{203a}') && hint.contains("board two"),
                "the hint names the cross-board target: {hint}"
            );
            // The confirm is the Activate intent; StackDetail has no
            // Return binding in the preset table, so the action routes
            // it (the move flow's Activate arm is the confirm).
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::TaskEdit);
            harness.assert_commands(&[StateCommand::MoveTask {
                id: caught,
                stack: fixture.stack_c,
                order: 1,
            }]);

            // The fake engine applies the move on settle; the chip
            // then reads zero.
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.handle.window().get_toolbar_inbox_count(),
                0,
                "triage-done: the chip counts to zero"
            );
        },
    );
}

/// Chunk 10 D2 — the kiosk mount pin: the kiosk profile still launches
/// on the board view (the desktop-only default's scope).
#[test]
fn the_kiosk_mount_still_lands_on_the_board() {
    // SAFETY: single-threaded journey startup (see the set site).
    unsafe { std::env::remove_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY") };
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert_eq!(current_view(&harness), View::Board);
        },
    );
}

// -- r2 D32/D33/D34, r4 D39: the board-scoped connection surface -----

use taskboard_ui_slint::requests::{ConnectionsSnapshot, ConnectorInfo};

/// The secret-free connector rows the tests hand over at mount.
fn connector_row(name: &str) -> ConnectorInfo {
    ConnectorInfo {
        name: name.to_owned(),
        server_url: "https://c.example".into(),
        username: "alice".into(),
    }
}

/// Adds one board to the engine state (r4 D39: the selector projects
/// the STATE every pass — the tests seed boards here, the install
/// hand-over's boards half is subsumed).
fn state_board(state: &mut AppState, id: uuid::Uuid, title: &str, connector: Option<&str>) {
    use taskboard_domain::{
        Board, BoardClocks, BoardId, Color, ConnectorId, RemoteBoardBinding, RemoteBoardId,
    };
    state.boards.insert(
        BoardId::from_uuid(id),
        Board {
            id: BoardId::from_uuid(id),
            remote: connector.map(|c| RemoteBoardBinding {
                board: RemoteBoardId(1),
                connector: ConnectorId::from(c),
            }),
            title: title.to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            remote_seen: None,
        },
    );
}

/// The pushed dropdown options (the journeys' model facts).
fn options(harness: &Harness) -> Vec<taskboard_ui_slint::DropdownRow> {
    let model = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::SettingsModel>();
    let rows = model.get_board_options();
    (0..rows.row_count())
        .filter_map(|i| rows.row_data(i))
        .collect()
}

/// Finds one option row by its board title (data identity, not text
/// assertion — titles are the fixture's keys).
fn option_of(harness: &Harness, title: &str) -> taskboard_ui_slint::DropdownRow {
    options(harness)
        .into_iter()
        .find(|row| row.label == title)
        .unwrap_or_else(|| panic!("the {title} option renders"))
}

/// Walks the settings surface Tab-by-Tab, pressing Return on each
/// stop, until the selector's flyout opens — the walk reaches the
/// field (a real walk stop), and Return on it opens. The facts'
/// `field-focused` mirror is unusable here (the backend fires no
/// `changed` hook on a bound out property — the recorded negative),
/// so the OPEN state itself is the walk's success fact. The bound is
/// Real-pointer-clicks the selector's field (the element query is the
/// address at the regular breakpoint; the click lands on the field's
/// left half — the Detach button rides the row's right edge).
fn click_selector_field(driver: &mut SlintDriver) {
    // The backend's element traversal after a view swap is racy (the
    // recorded limitation): retry the query across a few clock turns
    // before giving up.
    let mut row = None;
    for _ in 0..20 {
        driver.advance_time(0);
        let found = driver.find_all_by_id("SettingsView::picker-row");
        if let Some(first) = found.into_iter().next() {
            row = Some(first);
            break;
        }
    }
    let row = row.expect("the picker row renders");
    let pos = row.absolute_position();
    let size = row.size();
    // 60% in: past the label (the row's left slot), inside the field,
    // short of the Detach button (the row's right edge). The pointer
    // move seeds the cursor position first (the grab probe's dispatch
    // shape — press lands where the move said).
    let x = pos.x + size.width * 0.6;
    let y = pos.y + size.height / 2.0;
    driver.hover_at(x, y);
    driver.advance_time(0);
    driver.click_at(x, y);
    driver.advance_time(30);
}

/// The settings model global, fresh per call (the global handle ties
/// to the window's borrow — module fns are lifetime-generic where
/// closures are not).
fn model(h: &Harness) -> taskboard_ui_slint::SettingsModel<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::SettingsModel>()
}

/// The board selector's facts global, fresh per call.
fn board_facts(h: &Harness) -> taskboard_ui_slint::SettingsBoardFacts<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::SettingsBoardFacts>()
}

/// The board bar's facts global, fresh per call (the lifetime pattern
/// above — the global handle ties to the window's borrow).
fn bar_facts(h: &Harness) -> taskboard_ui_slint::BoardBarFacts<'_> {
    h.handle
        .window()
        .global::<taskboard_ui_slint::BoardBarFacts>()
}

/// Dispatches one raw key event to the focused element (the widget
/// ladder's ladder — the action layer never sees these strokes).
fn raw_key(harness: &Harness, key: slint::platform::Key) {
    harness
        .handle
        .window()
        .window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.into() });
}

/// J1 (rewired r4 D39) — the selector's options project the engine
/// STATE (indicator + tone fold included); selection drives the
/// Bind-only seam and dispatches NOTHING; a reply snapshot's boards
/// half cannot feed the list (the one-writer rule).
#[test]
fn the_selector_options_project_the_state_and_selection_dispatches_nothing() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The projection: two live boards, the bound one carrying
            // the link indicator in the idle fold's success tone; the
            // unbound one link-off/neutral. Nothing is selected.
            let rows = options(&harness);
            assert_eq!(rows.len(), 2, "the state's live boards render");
            let away_row = option_of(&harness, "away");
            assert_eq!(
                away_row.indicator,
                taskboard_ui_slint::DropdownIndicator::Link
            );
            assert_eq!(away_row.tone, taskboard_ui_slint::DropdownTone::Success);
            let home_row = option_of(&harness, "home");
            assert_eq!(
                home_row.indicator,
                taskboard_ui_slint::DropdownIndicator::LinkOff
            );
            assert_eq!(home_row.tone, taskboard_ui_slint::DropdownTone::Neutral);
            assert_eq!(
                model().get_selected_board(),
                "",
                "nothing selected at mount"
            );

            // A click selects: radio semantics over the model facts,
            // and the lane stays empty — Bind-only (review focus (c)).
            window.invoke_settings_board_selected(home.to_string().into());
            assert_eq!(model().get_selected_board(), home.to_string());
            assert_eq!(model().get_selected_board_label(), "home");
            assert!(!model().get_selected_board_bound(), "home is unbound");
            let selected = options(&harness)
                .into_iter()
                .find(|row| row.selected)
                .expect("one selected row");
            assert_eq!(selected.label, "home");
            assert!(
                harness.drain_requests().is_empty(),
                "selecting dispatches nothing"
            );

            // A reply snapshot's boards half cannot feed the list:
            // route the save ceremony and answer with a FOREIGN board
            // list — the options keep following the state, the
            // connector rows follow the reply.
            window.invoke_settings_field_edited(0, "newc".into());
            window.invoke_settings_field_edited(1, "https://n.example".into());
            window.invoke_settings_field_edited(2, "alice".into());
            window.invoke_settings_field_edited(3, "pw".into());
            window.invoke_settings_save_clicked(
                "newc".into(),
                "https://n.example".into(),
                "alice".into(),
                "pw".into(),
            );
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SaveConnection { reply, .. } =
                requests.remove(0)
            else {
                panic!("the save routed");
            };
            reply
                .send(Ok(ConnectionsSnapshot {
                    boards: Vec::new(),
                    connectors: vec![connector_row("work"), connector_row("newc")],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                options(&harness).len(),
                2,
                "the reply's boards half is ignored — the state feeds the list"
            );
            assert_eq!(
                model().get_rows().row_count(),
                2,
                "the reply's connector list landed on the rows"
            );
        },
    );
}

/// J2 — bind routing (r2 D33): a selected board + a connector row's
/// Bind routes exactly one `AttachConnector` with the typed ids; the
/// Ok reply clears the busy gate; a `BindRefused` reply lands
/// `Rejected` with non-empty status text (structure, never strings).
#[test]
fn bind_routes_attach_and_the_typed_reply_lands() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            window.invoke_settings_board_selected(home.to_string().into());

            window.invoke_settings_bind_clicked(0);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one attach routed");
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::AttachConnector {
                    board, connector, ..
                } if board.as_uuid() == home && connector == "work"
            ));
            assert!(model().get_busy(), "the gate is busy mid-flight");

            // The Ok reply clears the gate.
            let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the attach routed");
            };
            reply
                .send(Ok(ConnectionsSnapshot {
                    boards: Vec::new(),
                    connectors: vec![connector_row("work")],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy(), "the gate cleared with the reply");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok,
            );

            // A BindRefused reply lands Rejected + non-empty text.
            window.invoke_settings_bind_clicked(0);
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the second attach routed");
            };
            let _ = reply.send(Err(
                taskboard_ui_slint::requests::SettingsError::BindRefused,
            ));
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the bind refusal is Rejected"
            );
            assert!(
                !model().get_status_text().is_empty(),
                "the refusal sentence is visible"
            );
        },
    );
}

/// J3 (rewired r4 D39 §3.3) — detach routes from the SELECTION: the
/// lane carries the typed pair, the busy ceremony rides the selected
/// option, and the Ok reply clears it. A missing or unbound selection
/// is absorbed silently.
#[test]
fn detach_routes_from_the_selection_and_the_ceremony_rides_the_option() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let plain = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", Some("work"));
    state_board(&mut state, plain, "plain", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // No selection: absorbed.
            window.invoke_settings_detach_selected();
            assert!(
                harness.drain_requests().is_empty(),
                "no selection, no detach"
            );

            // An unbound selection: absorbed.
            window.invoke_settings_board_selected(plain.to_string().into());
            window.invoke_settings_detach_selected();
            assert!(harness.drain_requests().is_empty(), "unbound, no detach");

            // The bound selection routes, and the busy marker rides
            // the selected OPTION.
            window.invoke_settings_board_selected(home.to_string().into());
            window.invoke_settings_detach_selected();
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one detach routed");
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::DetachConnector {
                    board, connector, ..
                } if board.as_uuid() == home && connector == "work"
            ));
            assert!(model().get_busy());
            assert_eq!(model().get_busy_target(), "home");
            assert!(
                option_of(&harness, "home").busy,
                "the ceremony rides the selected option"
            );

            // The Ok reply clears the ceremony.
            let taskboard_ui_slint::requests::UiRequest::DetachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the detach routed");
            };
            reply
                .send(Ok(ConnectionsSnapshot::default()))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy(), "the gate cleared");
            assert!(
                !option_of(&harness, "home").busy,
                "the option's busy flag cleared with the reply"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok,
            );
        },
    );
}

/// J4 — the busy gate (r2 D34): with an attach pending, Save/Test/a
/// second Bind route nothing (one op at a time); the initiating
/// connector row's busy flag is set; the reply clears it
/// (reply-driven — no clock).
#[test]
fn the_busy_gate_holds_one_op_and_the_reply_clears_the_row_marker() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            window.invoke_settings_board_selected(home.to_string().into());

            // The initiating op: an attach, left unanswered.
            window.invoke_settings_bind_clicked(0);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            assert!(model().get_busy());
            assert_eq!(model().get_busy_target(), "work");
            assert!(
                model().get_rows().row_data(0).expect("row").busy,
                "the initiating connector row carries the busy flag"
            );

            // While pending: a valid Save routes nothing (visible
            // Rejected instead), Test and a second Bind route nothing.
            window.invoke_settings_save_clicked(
                "x".into(),
                "https://x.example".into(),
                "alice".into(),
                "pw".into(),
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            window.invoke_settings_test_clicked(0);
            window.invoke_settings_bind_clicked(0);
            assert!(
                harness.drain_requests().is_empty(),
                "the pending gate routed nothing"
            );

            // The reply clears the row marker (reply-driven); the
            // initiating request was retained from the first drain.
            let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the attach routed");
            };
            let _ = reply.send(Ok(ConnectionsSnapshot {
                boards: Vec::new(),
                connectors: vec![connector_row("work")],
            }));
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy(), "the gate cleared");
            assert!(
                !model().get_rows().row_data(0).expect("row").busy,
                "the row busy flag cleared with the reply"
            );
        },
    );
}

/// r4 D39 §2.3 — the staleness repair (F29): a board created after
/// install renders on the next pass (by construction — the list is
/// state-derived), and a selection whose board no longer renders is
/// pruned silently (Bind never stays armed on a ghost).
#[test]
fn a_board_created_after_install_renders_and_a_gone_selection_prunes() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            window.invoke_settings_board_selected(home.to_string().into());
            assert_eq!(model().get_selected_board(), home.to_string());

            // A board created after install; `home` tombstoned
            // elsewhere. One state swap, one refresh.
            let mut next = AppState::default();
            let late = uuid::Uuid::new_v4();
            state_board(&mut next, late, "late", None);
            harness.engine.replace_state(next, &harness.handle);
            harness.handle.drain_setting_replies();

            let titles: Vec<String> = options(&harness)
                .into_iter()
                .map(|row| row.label.to_string())
                .collect();
            assert!(
                titles.contains(&"late".to_string()),
                "the post-install board renders: {titles:?}"
            );
            assert!(
                !titles.contains(&"home".to_string()),
                "the tombstoned board is out: {titles:?}"
            );
            assert_eq!(
                model().get_selected_board(),
                "",
                "the gone selection pruned silently"
            );
            assert!(!model().get_selected_board_bound());
            assert_eq!(model().get_selected_board_label(), "", "the label cleared");
        },
    );
}

/// r4 D39 §3.5 — the dropdown's user journey on the settings surface:
/// the tab walk reaches the field, Return opens, the ladder moves the
/// highlight from the selection anchor, Return selects (the seam
/// lands the selection, the label, and Bind's gate), Escape closes
/// without selecting, an outside click closes through the underlay.
#[test]
fn the_selector_dropdown_opens_walks_and_selects() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();

    // Exactly TWO boards: the board bar's segment row trips the
    // testing backend's element traversal past two segments (the
    // recorded chip-free-chrome limitation's sibling); two keep the
    // ladder's move meaningful.
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", None);
    // NO connector rows on the hand-over: the row list's conditionals
    // trip the testing backend's element traversal (the recorded
    // chip-free-chrome limitation), and this journey's facts live on
    // the selector, not the rows.
    let settings = UiSettings::default();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            harness.driver.set_window_size(1000, 900);
            harness.driver.advance_time(0);

            // Pre-select `home`: the open-time anchor is the selected
            // row, not 0.
            harness
                .handle
                .window()
                .invoke_settings_board_selected(home.to_string().into());

            // Open via the field click — the click focuses the field,
            // so the ladder keys route through the field's arm.
            click_selector_field(&mut harness.driver);
            let mut saw_open = false;
            for _ in 0..6 {
                harness.driver.advance_time(0);
                saw_open = saw_open || board_facts(&harness).get_open();
                assert!(
                    !saw_open || board_facts(&harness).get_open(),
                    "the flyout opened and then closed itself"
                );
            }
            assert!(
                board_facts(&harness).get_open(),
                "the field click opens the flyout"
            );
            let anchor = options(&harness)
                .iter()
                .position(|r| r.selected)
                .expect("the selected row anchors");

            assert_eq!(
                board_facts(&harness).get_highlight(),
                i32::try_from(anchor).expect("index fits"),
                "the anchor is the selection"
            );

            // The ladder moves; the list seeks (the fact pattern).
            // The step direction adapts: the boards sit in uuid order,
            // so the selection may anchor at the last row.
            let before = board_facts(&harness).get_highlight();
            let at_last = before == i32::try_from(options(&harness).len() - 1).expect("index fits");
            if at_last {
                raw_key(&harness, slint::platform::Key::UpArrow);
            } else {
                raw_key(&harness, slint::platform::Key::DownArrow);
            }
            harness.driver.advance_time(0);
            let expected = if at_last { before - 1 } else { before + 1 };
            assert_eq!(
                board_facts(&harness).get_highlight(),
                expected,
                "the ladder moves off the anchor"
            );

            // Return selects: the seam lands the selection, the
            // label, and the flyout closes. (The dismissal matrix —
            // Escape, the outside click, the Tab walk — lives in the
            // dropdown probe: the backend's element traversal does not
            // survive the flyout's unmount, so the journey opens once.)
            raw_key(&harness, slint::platform::Key::Return);
            harness.driver.advance_time(0);
            assert!(!board_facts(&harness).get_open(), "selecting closes");
            let picked_index = board_facts(&harness).get_highlight();
            let picked_row = usize::try_from(picked_index).expect("index fits");
            let picked = options(&harness)[picked_row].label.to_string();
            let picked_id = {
                let ids = model(&harness).get_board_ids();
                ids.row_data(picked_row).expect("id").to_string()
            };
            assert_eq!(
                model(&harness).get_selected_board(),
                picked_id,
                "the picked row's id landed on the seam"
            );
            assert_eq!(
                model(&harness).get_selected_board_label(),
                picked,
                "the field re-renders the picked board"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "selecting still dispatches nothing"
            );
        },
    );
}

/// J5 — the save click-through (r2 D34, F21): every Save click reacts.
/// An invalid draft lands a typed Rejected with non-empty status text
/// and an empty lane; a valid-but-pending click lands Rejected with
/// the lane untouched; a valid idle click routes (covered by the
/// save-ceremony journey, unchanged).
#[test]
fn every_save_click_produces_a_visible_reaction_and_the_lane_stays_clean() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Invalid draft (empty name): the click fires *through*
            // the disabled button and the refusal is visible.
            window.invoke_settings_save_clicked(
                "".into(),
                "https://c.example".into(),
                "alice".into(),
                "pw".into(),
            );
            assert!(harness.drain_requests().is_empty(), "no request routed");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the invalid-draft refusal is visible"
            );
            assert!(
                !model().get_status_text().is_empty(),
                "the status line carries the mapped sentence"
            );

            // Valid draft, but a click while pending: visible
            // Rejected, lane untouched.
            window.invoke_settings_field_edited(0, "home".into());
            window.invoke_settings_field_edited(1, "https://c.example".into());
            window.invoke_settings_field_edited(2, "alice".into());
            window.invoke_settings_field_edited(3, "pw".into());
            window.invoke_settings_save_clicked(
                "home".into(),
                "https://c.example".into(),
                "alice".into(),
                "pw".into(),
            );
            assert_eq!(harness.drain_requests().len(), 1, "the idle click routed");
            window.invoke_settings_save_clicked(
                "home".into(),
                "https://c.example".into(),
                "alice".into(),
                "pw".into(),
            );
            assert!(
                harness.drain_requests().is_empty(),
                "the pending click routed nothing"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the busy-click refusal is visible"
            );
        },
    );
}

// -- the pointer drag-to-move journeys (r2 chunk-8, T-6..T-10) ----------

/// The drag journeys' geometry helper: the accessible button whose
/// label addresses the column (the header; the column list carries
/// the same label — the `is_accessible_button` disambiguator).
fn column_header(harness: &Harness, title: &str) -> (f32, f32) {
    let button = harness
        .driver
        .find_all_by_label(title)
        .into_iter()
        .find(support::driver::Element::is_accessible_button)
        .unwrap_or_else(|| panic!("the {title:?} header button is present"));
    let pos = button.absolute_position();
    let size = button.size();
    (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// The drag flow's model surface (r2 D-D): the ghost's open flag, the
/// ghost card's snapshot title, and the target highlight index.
fn drag_surface(harness: &Harness) -> (bool, String, i32) {
    let window = harness.handle.window();
    let ghost = window
        .global::<taskboard_ui_slint::DragState>()
        .get_ghost_model();
    (
        window.get_board_drag_open(),
        ghost.title.to_string(),
        window.get_board_model().move_target,
    )
}

/// The D-F acceptance (b): press-hold-drag past the slop lifts the
/// ghost (the source card dims), moving over another column
/// highlights it, and the release dispatches **the same `MoveTask`**
/// the keyboard flow confirms — two mounts, one fixture shape, one
/// literal expected command.
#[test]
fn the_pointer_drag_drops_the_keyboard_move_command() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    let expected = StateCommand::MoveTask {
        id: task0,
        stack: fixture.stack_b,
        order: 1,
    };

    // The drag path.
    let (drag_state, expected_drag) = (state.clone(), expected.clone());
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), drag_state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            let (hx, hy) = column_header(&harness, "b");
            // The arming move is consumed by the StartDrag hand-off;
            // the next move is the first DragMove the DropAreas see.
            harness.driver.drag_move_to(hx, hy);
            harness.driver.drag_move_to(hx, hy);
            harness.driver.advance_time(0);
            let (open, ghost_title, target) = drag_surface(&harness);
            assert!(open, "the drag is on the model surface");
            assert_eq!(ghost_title, "task-0", "the ghost carries the snapshot");
            assert_eq!(target, 1, "the hovered column highlights");
            let source_dimmed = harness
                .handle
                .window()
                .get_board_model()
                .stacks
                .row_data(0)
                .expect("column")
                .tasks
                .row_data(0)
                .expect("card")
                .dragging_source;
            assert!(source_dimmed, "the source card carries the dim bit");
            harness.driver.release_at(hx, hy);
            harness.driver.advance_time(0);
            harness.assert_commands(&[expected_drag]);
            assert!(
                !harness.handle.window().get_board_drag_open(),
                "the drop closes the ghost"
            );
        },
    );

    // The keyboard path: the same transition, the same literal command.
    let expected_kb = expected;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.window().invoke_board_task_clicked(0, 0);
            harness.press(ActionId::TaskMove);
            harness.stroke(parse("Down").expect("Down"));
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[expected_kb]);
        },
    );
}

/// R3-F4 (D1) — the ghost is card-sized: arming a drag captures the
/// source card's rendered size into `DragState` before the ghost's
/// first frame, so the ghost `TaskCard` mounts at the card's geometry —
/// never the window fill. Structural: the globals against the live
/// card element; no pixel assertions.
#[test]
fn the_drag_ghost_carries_the_source_cards_size() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            let card_size = card.size();
            harness.driver.press_at(cx, cy);
            let (hx, hy) = column_header(&harness, "b");
            harness.driver.drag_move_to(hx, hy);
            harness.driver.drag_move_to(hx, hy);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_board_drag_open(),
                "the drag is on the model surface"
            );
            let drag = harness
                .handle
                .window()
                .global::<taskboard_ui_slint::DragState>();
            let (gw, gh) = (drag.get_ghost_width(), drag.get_ghost_height());
            assert!(
                gw > 0.0 && gh > 0.0,
                "the captured size is set before the ghost's first frame, got {gw}x{gh}"
            );
            assert_eq!(gw, card_size.width, "the ghost width is the card's");
            assert_eq!(gh, card_size.height, "the ghost height is the card's");
            harness.driver.release_at(hx, hy);
            harness.driver.advance_time(0);
        },
    );
}

/// The (d) acceptance: a background refresh mid-drag re-derives the
/// ghost flag, the ghost snapshot, and the highlight from the flow —
/// they survive — and the release still dispatches (T-7).
#[test]
fn the_drag_surface_survives_interleaved_refreshes() {
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    // The mid-drag engine snapshot: the SAME fixture (stable ids —
    // the dragged task must survive) plus a new task in stack b.
    let mut changed = state.clone();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            let (hx, hy) = column_header(&harness, "b");
            harness.driver.drag_move_to(hx, hy);
            harness.driver.drag_move_to(hx, hy);
            harness.driver.advance_time(0);

            // A concurrent engine change (the width journey's shape).
            changed.tasks.insert(
                taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                taskboard_domain::Task {
                    id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
                    remote: None,
                    title: "mid-drag".into(),
                    description: String::new(),
                    duedate: None,
                    scheduled: None,
                    done: None,
                    stack: fixture.stack_b,
                    order: 0,
                    label_memberships: std::collections::BTreeMap::new(),
                    effort: None,
                    importance: None,
                    urgency: None,
                    recur: None,
                    kind: taskboard_domain::entities::TaskKind::Standard,

                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::TaskClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        description: taskboard_domain::BoardClocks::epoch(),
                        duedate: taskboard_domain::BoardClocks::epoch(),
                        scheduled: taskboard_domain::BoardClocks::epoch(),
                        done: taskboard_domain::BoardClocks::epoch(),
                        position: taskboard_domain::BoardClocks::epoch(),

                        effort: taskboard_domain::BoardClocks::epoch(),
                        importance: taskboard_domain::BoardClocks::epoch(),
                        urgency: taskboard_domain::BoardClocks::epoch(),
                        recur: taskboard_domain::BoardClocks::epoch(),
                        kind: taskboard_domain::BoardClocks::epoch(),

                        archived: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                    remote_baseline: None,
                },
            );
            harness.engine.replace_state(changed, &harness.handle);
            let (open, ghost_title, target) = drag_surface(&harness);
            assert!(open, "the refresh keeps the drag open");
            assert_eq!(ghost_title, "task-0", "the ghost snapshot re-derives");
            assert_eq!(target, 1, "the highlight re-derives");
            harness.driver.release_at(hx, hy);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::MoveTask {
                id: task0,
                stack: fixture.stack_b,
                order: 1,
            }]);
        },
    );
}

/// The (c) acceptance, arm 1: a release outside any column (over the
/// toolbar) cancels — no dispatch, the ghost closes (T-8a).
#[test]
fn a_release_outside_any_column_cancels_the_drag() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            harness.driver.drag_move_to(500.0, 12.0);
            harness.driver.drag_move_to(500.0, 12.0);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_board_drag_open(),
                "the drag opened over the dead zone"
            );
            harness.driver.release_at(500.0, 12.0);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_board_drag_open(),
                "the cancel closes the ghost"
            );
            assert_eq!(
                harness.handle.window().get_board_model().move_target,
                -1,
                "no highlight survives"
            );
        },
    );
}

/// The (c) acceptance, arm 2: Escape mid-drag cancels through D16's
/// binding with no dispatch, tears the native drag down, and the real
/// release afterwards cannot degrade into a click (T-8b).
#[test]
fn escape_mid_drag_cancels_and_the_release_clicks_nothing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            let (hx, hy) = column_header(&harness, "b");
            harness.driver.drag_move_to(hx, hy);
            harness.driver.drag_move_to(hx, hy);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_board_drag_open());
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_board_drag_open(),
                "Escape ends the drag"
            );
            // The real release after the teardown: no spurious click.
            harness.driver.release_at(hx, hy);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                harness.emphasized_card().is_none(),
                "the post-teardown release selects nothing"
            );
        },
    );
}

/// The (c) acceptance, arm 3: a leaving effect (nav.next-view)
/// mid-drag cancels without dispatch (T-8c).
#[test]
fn a_leaving_effect_mid_drag_cancels_the_drag() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            let (hx, hy) = column_header(&harness, "b");
            harness.driver.drag_move_to(hx, hy);
            harness.driver.drag_move_to(hx, hy);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_board_drag_open());
            harness.press(ActionId::NavNextView);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !harness.handle.window().get_board_drag_open(),
                "the view switch cancels the drag"
            );
        },
    );
}

/// The D-H slop boundary (T-9): below the 8-px slop a release is a
/// click (the typed selection outcome); past it, the same gesture is
/// a drag whose release on the source column reorders the task to its
/// own stack's end — the keyboard ring's legal move, and still no
/// click.
#[test]
fn the_slop_boundary_separates_clicks_from_drags() {
    // Sub-slop: a click.
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            harness.driver.drag_move_to(cx + 4.0, cy);
            harness.driver.release_at(cx + 4.0, cy);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 0)),
                "the sub-slop release selected the card"
            );
        },
    );

    // Past the slop: a drag that drops back on the source column.
    let (fixture, state) = fixture::golden_state();
    let task0 = state
        .tasks
        .values()
        .find(|task| task.title == "task-0")
        .map(|task| task.id)
        .expect("the fixture's first task");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            harness.driver.drag_move_to(cx + 20.0, cy);
            harness.driver.drag_move_to(cx + 20.0, cy);
            harness.driver.advance_time(0);
            let (open, _, _) = drag_surface(&harness);
            assert!(open, "the drag armed past the slop");
            harness.driver.release_at(cx + 20.0, cy);
            harness.driver.advance_time(0);
            harness.assert_commands(&[StateCommand::MoveTask {
                id: task0,
                stack: fixture.stack_a,
                order: 2,
            }]);
            assert!(
                harness.emphasized_card().is_none(),
                "the dragged release never clicked"
            );
        },
    );
}

/// The T-10 refusal: with a flow already open (the create row), a
/// press-move on a card arms nothing — the `DragArea`s are disabled and
/// the view state refuses — and the open flow's field survives.
#[test]
fn a_drag_never_arms_while_another_flow_holds() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::TaskCreate);
            let create_open = |harness: &Harness| {
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("column")
                    .create_open
            };
            assert!(create_open(&harness), "the create row opened");
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let (cx, cy) = support::driver::centre(&card);
            harness.driver.press_at(cx, cy);
            harness.driver.drag_move_to(cx + 20.0, cy);
            harness.driver.drag_move_to(cx + 20.0, cy);
            harness.driver.advance_time(0);
            let (open, _, target) = drag_surface(&harness);
            assert!(!open, "the busy flow refuses the drag arm");
            assert_eq!(target, -1, "no highlight from the refused arm");
            assert!(create_open(&harness), "the create row survived");
            harness.assert_commands(&[]);
        },
    );
}

// -- K13: the kiosk ambient rotation (issue #363) ------------------------

use std::time::Duration;

use taskboard_ui_slint::view_state::{KioskMode, KioskRotation};

/// The K13 rotation hand-over: board ⇄ today, 30 s dwell. Built
/// directly (the journeys bypass config; the typed surface is the
/// same `UiSettings` field the appliance hands over).
fn rotation_settings() -> UiSettings {
    UiSettings {
        kiosk: true,
        kiosk_rotation: Some(
            KioskRotation::new(
                vec![KioskMode::Board, KioskMode::Today],
                Duration::from_secs(30),
            )
            .expect("two distinct modes mint"),
        ),
        ..UiSettings::default()
    }
}

/// The today sections' per-row counts (the pinned-board filter's
/// surface witness).
fn today_section_counts(harness: &Harness) -> Vec<usize> {
    let today = harness.handle.window().get_today_model();
    (0..today.sections.row_count())
        .map(|section| {
            usize::try_from(today.sections.row_data(section).expect("section").count)
                .expect("section count fits")
        })
        .collect()
}

/// J-K1 — the rotation flips on the dwell timer with zero input: the
/// declared order seeds the start view, the mock clock fires the
/// flip exactly at the dwell boundary, focus resets each flip, the
/// banner and the sync badge carry over, and the engine's command
/// log stays empty throughout (the no-engine-command end-to-end
/// witness).
#[test]
fn the_kiosk_rotation_flips_modes_on_the_dwell_timer() {
    // SAFETY: single-threaded journey startup (see the set site).
    unsafe { std::env::remove_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY") };
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness =
                Harness::mount_with_settings(SlintDriver::default(), state, rotation_settings());
            assert_eq!(
                current_view(&harness),
                View::Board,
                "the appliance opens on modes[0]"
            );

            let banner_rows_before = harness.handle.window().get_banner_model().row_count();
            let sync_phase_before = harness.handle.window().get_sync_model().phase;

            harness.driver.advance_time(29_999);
            assert_eq!(
                current_view(&harness),
                View::Board,
                "one millisecond inside the dwell: no flip"
            );
            harness.driver.advance_time(1);
            assert_eq!(
                current_view(&harness),
                View::Today,
                "the dwell fires the flip"
            );
            assert_eq!(harness.emphasized_card(), None, "the flip resets focus");

            harness.driver.advance_time(30_000);
            assert_eq!(current_view(&harness), View::Board, "the cycle wraps");
            assert_eq!(harness.emphasized_card(), None);

            // The carry-over is structural (K13 D6): pinned by
            // assertion across the flips.
            assert_eq!(
                harness.handle.window().get_banner_model().row_count(),
                banner_rows_before,
                "the banner rows are unchanged across flips"
            );
            assert_eq!(
                harness.handle.window().get_sync_model().phase,
                sync_phase_before,
                "the sync badge's phase is unchanged across flips"
            );

            assert_eq!(
                harness.engine.drain(),
                [] as [StateCommand; 0],
                "rotation must never reach the engine"
            );
        },
    );
}

/// J-K2 — the passive reading: in both modes the kiosk gates hold
/// (inbox chip 0, notification surface empty) and the today model is
/// the PINNED board's degeneration — board one's overdue row, not
/// the desktop's cross-board two.
#[test]
fn the_kiosk_today_mode_renders_only_the_pinned_board() {
    // SAFETY: single-threaded journey startup (see the set site).
    unsafe { std::env::remove_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY") };
    let (_fixture, mut state, _walked) = fixture::today_overdue_state();
    // Live and awaiting alarms in the data: the gates must hold
    // against non-empty timer state, not just an empty one.
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let fired = fixture::add_alarm(&mut state, "kiosk-alarm", fired_at);
    fixture::deliver_alarm(&mut state, fired, fired_at);
    let _live = fixture::add_alarm(
        &mut state,
        "kiosk-timer",
        chrono::Utc::now() + TimeDelta::hours(1),
    );
    let state = state;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness =
                Harness::mount_with_settings(SlintDriver::default(), state, rotation_settings());
            // Board mode (modes[0]): the gates hold.
            assert_eq!(current_view(&harness), View::Board);
            assert_eq!(
                harness.handle.window().get_toolbar_inbox_count(),
                0,
                "kiosk is passive: the inbox chip counts to zero"
            );
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                0,
                "kiosk is banner-only: the surface model stays empty"
            );
            // The timers gate (DR-3/3, D-I6): the section, its rows,
            // and the status-bar chip are all forced off model-side.
            assert!(!harness.handle.window().get_timers_visible());
            assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
            assert_eq!(harness.handle.window().get_timers_awaiting(), 0);
            assert!(!harness.handle.window().get_timers_chip_visible());

            harness.driver.advance_time(30_000);
            assert_eq!(current_view(&harness), View::Today);
            let counts = today_section_counts(&harness);
            assert_eq!(
                counts.first().copied(),
                Some(1),
                "the pinned board's one overdue row renders (the desktop \
                 would show two — the filter is the kiosk degeneration)"
            );
            assert_eq!(
                harness.handle.window().get_toolbar_inbox_count(),
                0,
                "the gates hold across the flip"
            );
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                0
            );
            assert!(!harness.handle.window().get_timers_visible());
            assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
            assert!(!harness.handle.window().get_timers_chip_visible());

            harness.driver.advance_time(30_000);
            assert_eq!(
                current_view(&harness),
                View::Board,
                "the flip back renders the board"
            );
        },
    );
}

/// J-K3 — the desktop is inert: no rotation in the hand-over, so
/// advancing the mock clock past several would-be dwells flips
/// nothing, dispatches nothing, and the view stays the desktop
/// default.
#[test]
fn the_desktop_mount_never_rotates() {
    // SAFETY: single-threaded journey startup (see the set site). The
    // var stays set: the harness's board escape is suppressed so the
    // desktop's Today default is the observed surface.
    unsafe { std::env::set_var("TASKBOARD_JOURNEYS_STAY_ON_TODAY", "1") };
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness =
                Harness::mount_with_settings(SlintDriver::default(), state, UiSettings::default());
            assert_eq!(current_view(&harness), View::Today);
            for _ in 0..3 {
                harness.driver.advance_time(30_000);
                assert_eq!(current_view(&harness), View::Today, "no flips");
            }
            assert_eq!(harness.engine.drain(), [] as [StateCommand; 0]);
        },
    );
}

/// The empty-board create flow is visible (round-3 R5/F10, minimal
/// fix): the empty-board leg renders the same create field the
/// populated legs do, typing lands, and Return creates the stack.
/// The full centered-CTA empty-board page stays chunk 2 (D5).
#[test]
fn the_empty_board_stack_create_flow_opens_a_visible_field() {
    let state = fixture::empty_board_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let create_open =
                |harness: &Harness| harness.handle.window().get_board_model().stack_create_open;

            harness.press(ActionId::StackCreate);
            assert!(create_open(&harness), "the flow arms on the empty board");
            assert!(
                harness.driver.find_by_label("New stack name").is_some(),
                "the empty-board leg renders the create field (F10)"
            );
            harness.type_text("first");
            harness.stroke(parse("Return").expect("Return"));
            assert!(!create_open(&harness), "the commit closed the flow");
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateStack { title, order, .. }] => {
                    assert_eq!(title.as_str(), "first");
                    assert_eq!(*order, 0, "the first stack on the empty board");
                }
                other => panic!("the create reached the boundary, got {other:?}"),
            }
        },
    );
}

/// Round-3 F26 — the title re-edit journey: detail entry never arms
/// (plan §6), the closed title's click arms the seeded editor, the
/// commit closes it, and clicking again re-arms — the exact repro
/// that was stuck before. The empty commit closes without dispatch.
#[test]
fn the_title_edits_clicks_arms_and_re_arms() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let edit_open = |harness: &Harness| harness.handle.window().get_task_model().edit_open;

            // Entry (the standard preset's Return on the focused
            // card) shows the display: no field, no flow, Normal
            // mode.
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            assert!(!edit_open(&harness), "entry never arms (plan §6)");
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_vi_insert(),
                "entry stays in normal mode"
            );
            harness.assert_commands(&[]);

            // Click the closed title: the field arms, seeded from the
            // display title (click ≡ key, §7.1).
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            assert!(edit_open(&harness), "the click arms the editor");
            assert_eq!(
                harness.handle.window().get_task_model().initial_text,
                "task-0",
                "the seed mirrors the current title"
            );

            // An empty commit closes without dispatching: backspace
            // past the whole seed (the click path's selection state
            // is not pinned), then Return on the empty buffer.
            for _ in 0..7 {
                harness.stroke(parse("Backspace").expect("Backspace"));
            }
            harness.stroke(parse("Return").expect("Return"));
            assert!(!edit_open(&harness), "the empty commit closes");
            harness.assert_commands(&[]);

            // The exact F26 repro: clicking again re-arms.
            harness.driver.click_by_label("Edit title");
            harness.driver.advance_time(0);
            assert!(edit_open(&harness), "the title is always re-armable");

            // A typed commit dispatches and closes.
            harness.type_text("renamed");
            harness.stroke(parse("Return").expect("Return"));
            assert!(!edit_open(&harness), "the commit closed the editor");
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { changes, .. }] => {
                    assert_eq!(changes.title.as_deref(), Some("renamed"));
                }
                other => panic!("the title commit reached the boundary, got {other:?}"),
            }

            // The geometry pin (plan §6): the field stretches to the
            // detail body's full width — it is no longer nested in
            // the back row's centered column.
            harness.handle.dispatch_action(ActionId::TaskTitleEdit);
            harness.driver.advance_time(0);
            let field = harness
                .driver
                .find_by_label("Task title")
                .expect("the field is mounted");
            assert!(
                field.size().width > 300.0,
                "the field spans the body width, got {:?}",
                field.size()
            );
        },
    );
}

/// Round-3 R1/F7 — the click-out journeys: an empty create flow
/// settles (discards) on a dead-zone click; a typed one commits
/// exactly once, and Return afterwards dispatches nothing (the
/// double-commit guard).
#[test]
fn a_dead_zone_click_settles_the_create_flow() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let create_open = |harness: &Harness| {
                harness
                    .handle
                    .window()
                    .get_board_model()
                    .stacks
                    .row_data(0)
                    .expect("column")
                    .create_open
            };

            // Empty buffer: the click-out discards, the mode returns,
            // and New Task works again without a second click.
            harness.press(ActionId::TaskCreate);
            assert!(create_open(&harness));
            harness.driver.advance_time(0);
            harness.driver.click_at(10.0, 450.0);
            harness.driver.advance_time(0);
            assert!(!create_open(&harness), "the empty click-out discards");
            assert!(!keyboard(&harness).get_vi_insert(), "normal mode returns");
            harness.assert_commands(&[]);
            harness.press(ActionId::TaskCreate);
            assert!(create_open(&harness), "New Task works again");
            harness.driver.click_at(10.0, 450.0);
            harness.assert_commands(&[]);

            // Typed buffer: the click-out commits exactly once, and a
            // Return afterwards dispatches nothing (the teardown
            // focus-lost echo is absorbed).
            harness.press(ActionId::TaskCreate);
            harness.type_text("clicked out");
            harness.driver.click_at(10.0, 450.0);
            harness.driver.advance_time(0);
            assert!(!create_open(&harness), "the typed click-out commits");
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateTask { title, .. }] => {
                    assert_eq!(title.as_str(), "clicked out");
                }
                other => panic!("one create reached the boundary, got {other:?}"),
            }
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[]);
        },
    );
}

/// DR-3/3 J-T1 — the desktop create→row→cancel loop: the form's
/// commit dispatches the exact `CreateAdhocAlarm`, the settle makes
/// the row render in the section (the chip lights up with it), and
/// the row's cancel affordance dispatches the exact
/// `CancelAdhocAlarm`, emptying the section by derivation.
#[test]
fn the_timers_form_creates_then_cancels() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            assert_eq!(current_view(&harness), View::Today);
            assert!(!harness.handle.window().get_timers_chip_visible());
            // Fill the form: click-focus each field (the fields opt
            // out of the mount-time focus grab), type, commit on the
            // timing field's Return.
            let click_field = |harness: &mut Harness, label: &str| {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("the field {label:?} is present"));
                let (x, y) = support::driver::centre(&element);
                harness.driver.click_at(x, y);
            };
            click_field(&mut harness, "Timer label");
            harness.type_text("Tea");
            click_field(&mut harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("in 3m");
            harness.stroke(parse("Return").expect("Return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateAdhocAlarm { label, when, .. }] => {
                    assert_eq!(label.as_str(), "Tea");
                    assert_eq!(
                        *when,
                        taskboard_domain::AdhocWhen::In(TimeDelta::minutes(3))
                    );
                }
                other => panic!("the create reached the boundary, got {other:?}"),
            }

            harness.engine.settle(&harness.handle);
            assert_eq!(harness.handle.window().get_timers_model().row_count(), 1);
            assert!(harness.handle.window().get_timers_chip_visible());

            // The row's cancel affordance.
            harness.driver.click_by_label("Cancel timer");
            match harness.engine.drain().as_slice() {
                [StateCommand::CancelAdhocAlarm { .. }] => {}
                other => panic!("the cancel reached the boundary, got {other:?}"),
            }
            harness.engine.settle(&harness.handle);
            assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
            assert!(!harness.handle.window().get_timers_chip_visible());
        },
    );
}

/// r4 D32 — the two-part countdown: a live timer's row renders the
/// fire instant (the `@` detail line) beside the countdown text, and
/// the insistence rung projects the badge bit — `normal` stays
/// badge-less (D-I9's ladder), `alarm` arms it. The per-second motion
/// is pinned at the unit tier (the seconds-gate tests + `SecondTick`):
/// the journey driver's mock clock moves slint timers, not the wall
/// clock the repaint driver reads, so a "changes across one clock
/// step" pin here would be nondeterministic (recorded deviation from
/// the stage-1 catalogue).
#[test]
fn the_timer_row_renders_the_two_part_countdown_and_the_rung() {
    let (_fixture, mut state) = fixture::journey_state();
    let _tea = fixture::add_alarm(
        &mut state,
        "Tea",
        chrono::Utc::now() + TimeDelta::minutes(5),
    );
    let _wake = fixture::add_alarm_with_rung(
        &mut state,
        "Wake",
        chrono::Utc::now() + TimeDelta::minutes(7),
        taskboard_domain::Insistence::Alarm,
    );
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            let rows = harness.handle.window().get_timers_model();
            assert_eq!(rows.row_count(), 2, "both live timers render");
            let normal = rows.row_data(0).expect("the ordered-first row");
            assert!(
                !normal.fire_time.is_empty(),
                "the fire instant's detail line renders"
            );
            assert!(!normal.countdown.is_empty(), "the countdown text renders");
            assert!(
                !normal.alarm_rung,
                "the normal rung is the silent default — no badge"
            );
            let escalation = rows.row_data(1).expect("the ordered-second row");
            assert!(
                escalation.alarm_rung,
                "the alarm rung projects the badge bit"
            );
            assert!(harness.handle.window().get_timers_chip_visible());
            assert!(
                !harness.handle.window().get_timers_chip().is_empty(),
                "the chip names the nearest fire"
            );
        },
    );
}

/// r4 D-T7 — the insistence picker: the inline form's picker rides
/// the commit seam's third parameter — the alarm rung lands on the
/// captured command verbatim; the default pick is the silent `Normal`
/// rung. Typed commands, never label text.
#[test]
fn the_timers_form_carries_the_insistence_rung() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            let click_field = |harness: &mut Harness, label: &str| {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("the element {label:?} is present"));
                let (x, y) = support::driver::centre(&element);
                harness.driver.click_at(x, y);
            };
            click_field(&mut harness, "Timer label");
            harness.type_text("Wake");
            click_field(&mut harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("in 2m");
            click_field(&mut harness, "Alarm");
            harness.stroke(parse("Return").expect("Return"));
            match harness.engine.drain().as_slice() {
                [
                    StateCommand::CreateAdhocAlarm {
                        label, insistence, ..
                    },
                ] => {
                    assert_eq!(label.as_str(), "Wake");
                    assert_eq!(
                        *insistence,
                        taskboard_domain::Insistence::Alarm,
                        "the alarm rung rides the commit seam"
                    );
                }
                other => panic!("the create reached the boundary, got {other:?}"),
            }

            // The default pick is the silent Normal rung (D-I9's
            // ladder): a fresh commit with the picker untouched.
            harness.engine.settle(&harness.handle);
            click_field(&mut harness, "Timer label");
            harness.type_text("Tea");
            click_field(&mut harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("in 2m");
            harness.stroke(parse("Return").expect("Return"));
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateAdhocAlarm { insistence, .. }] => {
                    assert_eq!(
                        *insistence,
                        taskboard_domain::Insistence::Normal,
                        "the default rung is the silent base"
                    );
                }
                other => panic!("the create reached the boundary, got {other:?}"),
            }
        },
    );
}

// -- r4 D33: the timers mouse setter (F13) --------------------------------

/// Drives one popover leg: opens via the header button and returns the
/// harness. The popover's label field shares its a11y address with the
/// inline form's (both say "Timer label" — the D33-pinned copy); the
/// popover mounts last in the tree, so the *last* match is the
/// popover's (the ordered-fixture discipline, asserted by the legs
/// below behaving: text typed into the wrong field would fail the
/// captured-command shapes).
fn open_setter(harness: &mut Harness) {
    harness.driver.click_by_label("Set a timer");
    harness.driver.advance_time(0);
    assert!(
        harness.handle.window().get_timer_setter_open(),
        "the header button opened the popover"
    );
}

/// The segment click (the toolbar journey's idiom): the radio-role
/// element is the address — a label can echo on non-button elements
/// whose default action is inert.
fn click_segment(harness: &mut Harness, label: &str) {
    harness
        .driver
        .find_all_by_label(label)
        .into_iter()
        .find(support::driver::Element::is_radio_button)
        .unwrap_or_else(|| panic!("the {label} segment is present"))
        .invoke_default_action();
    harness.driver.advance_time(0);
}

fn click_popover_label(harness: &mut Harness) {
    let matches = harness.driver.find_all_by_label("Timer label");
    let element = matches
        .last()
        .expect("the popover's label field is mounted");
    let (x, y) = support::driver::centre(element);
    harness.driver.click_at(x, y);
}

/// r4 D33 acceptance (b) — the parity contract: a timer created via
/// the popover (open → preset → label → commit) and one created via
/// the typed text form with the equivalent grammar dispatch the
/// `PartialEq`-equal `CreateAdhocAlarm` — same `AdhocWhen`, same
/// insistence. One parser, one seam, proven.
#[test]
fn the_setter_and_the_text_form_hit_the_same_commit_seam() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);

            // The popover leg: open, quick-fill the 5m preset (never
            // auto-commit, D-T6), label, commit.
            open_setter(&mut harness);
            click_segment(&mut harness, "5m");
            click_popover_label(&mut harness);
            harness.type_text("Tea");
            harness.driver.click_by_label("Set timer");
            harness.driver.advance_time(0);
            let popover_command = {
                let drained = harness.engine.drain();
                assert_eq!(
                    drained.len(),
                    1,
                    "the popover dispatched exactly the create"
                );
                drained.into_iter().next().expect("one command")
            };
            assert!(
                !harness.handle.window().get_timer_setter_open(),
                "a successful commit closes the popover"
            );
            harness.engine.settle(&harness.handle);

            // The text-form leg: the same grammar typed, committed on
            // the timing field's Return.
            let click_field = |harness: &mut Harness, label: &str| {
                let element = harness
                    .driver
                    .find_by_label(label)
                    .unwrap_or_else(|| panic!("the field {label:?} is present"));
                let (x, y) = support::driver::centre(&element);
                harness.driver.click_at(x, y);
            };
            click_field(&mut harness, "Timer label");
            harness.type_text("Tea");
            click_field(&mut harness, "Timer timing (in 10m or at 09:30)");
            harness.type_text("in 5m");
            harness.stroke(parse("Return").expect("Return"));
            let form_command = {
                let drained = harness.engine.drain();
                assert_eq!(drained.len(), 1, "the form dispatched exactly the create");
                drained.into_iter().next().expect("one command")
            };
            assert_eq!(
                popover_command, form_command,
                "popover ≡ text form ≡ same command (the one-parser contract)"
            );
        },
    );
}

/// r4 D-T4 — the custom row's grammar: a picked time plus an explicit
/// Tomorrow composes `at <RFC-3339>` and commits as `At(instant)` at
/// the picked time of the next day — the one parser between the
/// composition and the engine (typed match, never label text).
#[test]
fn the_setter_composes_the_custom_row_grammar() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // Single-read capture (D-3/R3): `today` is this test's only
            // clock read; the expected `At` instant derives from it.
            // Residual (accepted, R2/R3): the adapter's
            // `recompose_timer_setter` keeps its own `Utc::now()` read
            // feeding `compose_timer_at` — a local midnight falling
            // between this capture and the compose is the accepted
            // millisecond-wide window.
            let today = chrono::Local::now().date_naive();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            open_setter(&mut harness);
            // The stepper's seed-on-step (r4 D23): one `+` from the
            // unset state composes 00:15 — the pushed mirror moves off
            // the `--:--` sentinel and the fire caption renders. The
            // seam seeds the *stepped* value; seeding the pre-step
            // anchor left the display frozen while the composition
            // moved (the TimeField's re-seed contract).
            harness.driver.click_by_label("Step time forward");
            harness.driver.advance_time(0);
            assert_ne!(
                harness.handle.window().get_timer_setter_time_text(),
                "--:--",
                "the stepper seeds the display with the stepped value"
            );
            assert!(
                !harness
                    .handle
                    .window()
                    .get_timer_setter_fire_time()
                    .is_empty(),
                "the stepped composition renders its fire instant"
            );
            // A fresh draft for the custom-row leg below: Escape
            // clears the setter-local state with the panel.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            open_setter(&mut harness);
            // The custom row: click-focus the time field, type the
            // lenient digits, pick Tomorrow.
            let time_field = harness
                .driver
                .find_by_label("Custom time")
                .expect("the custom time field is mounted");
            let (x, y) = support::driver::centre(&time_field);
            harness.driver.click_at(x, y);
            harness.type_text("09:30");
            harness.driver.advance_time(0);
            click_segment(&mut harness, "Tomorrow");
            assert!(
                !harness
                    .handle
                    .window()
                    .get_timer_setter_fire_time()
                    .is_empty(),
                "the composed fire instant's caption renders"
            );
            click_popover_label(&mut harness);
            harness.type_text("Standup");
            harness.driver.click_by_label("Set timer");
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::CreateAdhocAlarm { when, .. }] => {
                    let tomorrow = (today + chrono::TimeDelta::try_days(1).expect("one day"))
                        .and_hms_opt(9, 30, 0)
                        .expect("09:30");
                    let expected = chrono::Local
                        .from_local_datetime(&tomorrow)
                        .single()
                        .expect("resolves")
                        .with_timezone(&chrono::Utc);
                    assert_eq!(
                        *when,
                        taskboard_domain::AdhocWhen::At(expected),
                        "the explicit day choice is honored — no roll"
                    );
                }
                other => panic!("the create reached the boundary, got {other:?}"),
            }
            assert!(
                !harness.handle.window().get_timer_setter_open(),
                "the commit closed the popover"
            );
        },
    );
}

/// r4 D33 §4.2 — the dismissal grammar: the scrim click and Escape
/// close without committing (drafts are setter-local, nothing is
/// dispatched); a successful commit closes; an absorbed miss (the
/// whitespace-empty label, D-P14) keeps the panel open with nothing
/// sent.
#[test]
fn the_setter_dismissal_layers_close_without_committing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);

            // (a) The scrim click closes — no command.
            open_setter(&mut harness);
            harness.driver.click_at(600.0, 40.0);
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_timer_setter_open(),
                "the outside click dismissed the panel"
            );
            harness.assert_commands(&[]);

            // (b) Escape closes — widget-internal, the action layer
            // never sees it, and still no command. The label click
            // hands the panel the focus a real open flow has.
            open_setter(&mut harness);
            click_popover_label(&mut harness);
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_timer_setter_open(),
                "Escape closed the panel"
            );
            harness.assert_commands(&[]);

            // (c) An absorbed miss keeps the panel open: a preset
            // quick-filled, no label — the empty-label absorb (D-P14).
            open_setter(&mut harness);
            click_segment(&mut harness, "10m");
            harness.driver.click_by_label("Set timer");
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_timer_setter_open(),
                "the miss keeps the panel open"
            );
            harness.assert_commands(&[]);

            // (d) A successful commit closes and dispatches exactly
            // the create.
            click_popover_label(&mut harness);
            harness.type_text("Tea");
            harness.driver.click_by_label("Set timer");
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_timer_setter_open(),
                "the commit closed the panel"
            );
            harness.assert_commands(&[StateCommand::CreateAdhocAlarm {
                label: "Tea".to_owned(),
                when: taskboard_domain::AdhocWhen::In(TimeDelta::minutes(10)),
                insistence: taskboard_domain::Insistence::Normal,
            }]);
        },
    );
}

// -- DR-5/4: the tracking affordances (issue #489) -----------------------

/// DR-5/4 — the chip: the open entry names itself (title snapshot +
/// elapsed) on desktop WITH the stop affordance, whose click routes
/// the exact payload-free `StopTracking`; a closed-only state hides
/// the chip. The kiosk leg pins D-T16's gate: the display STAYS (the
/// appliance shows what is being worked on), the action empties — a
/// hidden chip on kiosk is the stage-3 blocking finding.
#[test]
fn the_tracking_chip_renders_and_the_kiosk_gate_empties_only_the_action() {
    let (_fixture, state) = fixture::golden_state();
    // The seeded entry (D-TU12) is the chip's input; a closed-only
    // variant for the hidden leg.
    let mut closed_only = state.clone();
    closed_only.tracking.entries.clear();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // Desktop: chip + action visible, text present (presence,
            // never content — AGENTS §5).
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert!(harness.handle.window().get_tracking_chip_visible());
            assert!(harness.handle.window().get_tracking_chip_action_visible());
            assert!(
                !harness.handle.window().get_tracking_chip().is_empty(),
                "the chip names the open entry"
            );

            // The stop affordance routes the exact command (the seam
            // invoked directly — the testing backend exposes no a11y
            // node for a TouchArea control, so a labelled click cannot
            // address it; the gate's render arm is the `if` over the
            // property pinned below).
            harness.handle.window().invoke_tracking_chip_stop_clicked();
            harness.assert_commands(&[taskboard_domain::StateCommand::StopTracking]);

            // Closed-only: no chip, no action.
            drop(harness);
            let harness = Harness::mount(SlintDriver::default(), closed_only);
            assert!(!harness.handle.window().get_tracking_chip_visible());
            assert!(!harness.handle.window().get_tracking_chip_action_visible());

            // Kiosk: the display STAYS, the action empties.
            drop(harness);
            let (_fixture, kiosk_state) = fixture::golden_state();
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                kiosk_state,
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert!(
                harness.handle.window().get_tracking_chip_visible(),
                "the kiosk appliance still shows what is being worked on"
            );
            assert!(
                !harness.handle.window().get_tracking_chip().is_empty(),
                "the kiosk chip carries the title and elapsed"
            );
            assert!(
                !harness.handle.window().get_tracking_chip_action_visible(),
                "the gate empties the action, not the display"
            );
        },
    );
}

/// DR-5/4 (D-TU3) — the badge rides the decorations slot: exactly one
/// card on the board carries the tracking decoration, the open
/// entry's card; the ghost and compact collapsing ride the existing
/// slot semantics, never re-derived here.
#[test]
fn the_tracking_badge_rides_only_the_open_entrys_card() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            let board = harness.handle.window().get_board_model();
            let mut badged: Vec<(usize, usize)> = Vec::new();
            for column in 0..board.stacks.row_count() {
                let tasks = board.stacks.row_data(column).expect("column").tasks;
                for row in 0..tasks.row_count() {
                    let card = tasks.row_data(row).expect("card");
                    let mut decorations = Vec::new();
                    for index in 0..card.decorations.row_count() {
                        decorations.push(card.decorations.row_data(index).expect("decoration"));
                    }
                    if decorations.iter().any(|decoration| {
                        decoration.kind == taskboard_ui_slint::DecorationKind::Tracking
                    }) {
                        badged.push((column, row));
                    }
                }
            }
            assert_eq!(
                badged,
                vec![(0, 1)],
                "exactly the open entry's card badges (stack a, order 1)"
            );
        },
    );
}

/// DR-5/4 (D-TU7) — the notification Start Task seam: exactly one
/// command leaves the UI, the exact `StartTracking { task }` — the
/// implicit dismissal lives planner-side, so a `DismissReminder`
/// alongside would be the blocking duplicate. A stale index and an
/// alarm row degrade to zero commands.
#[test]
fn the_notification_start_task_dispatches_exactly_one_command() {
    let (fixture, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, fixture.task1, due);
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();

            // The task row renders the affordance (the anchor table).
            let rows = window.get_notification_model();
            assert_eq!(rows.row_count(), 1);
            assert!(rows.row_data(0).expect("row").can_track_start);

            // The seam dispatches exactly one command.
            window.invoke_notification_track_start_clicked(0);
            harness.assert_commands(&[taskboard_domain::StateCommand::StartTracking {
                task: fixture.task1,
                kind: taskboard_domain::tracking::TimeEntryKind::Work,
            }]);

            // A stale index degrades to a no-op.
            harness
                .handle
                .window()
                .invoke_notification_track_start_clicked(999);
            harness.assert_commands(&[]);
        },
    );
}

/// DR-5/4 (D-TU7) — the anchor table at the seam: an alarm-anchored
/// row never offers Start Task and its seam dispatches nothing.
#[test]
fn the_alarm_row_never_offers_start_task() {
    let (_fixture, mut state) = fixture::golden_state();
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let alarm = fixture::add_alarm(&mut state, "tea", fired_at);
    fixture::deliver_alarm(&mut state, alarm, fired_at);
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let rows = harness.handle.window().get_notification_model();
            assert_eq!(rows.row_count(), 1, "the awaiting alarm renders");
            assert!(
                !rows.row_data(0).expect("row").can_track_start,
                "an Adhoc row never renders Start Task"
            );
            drop(rows);
            harness
                .handle
                .window()
                .invoke_notification_track_start_clicked(0);
            harness.assert_commands(&[]);
        },
    );
}

/// DR-5/4 (D-TU6) — the detail button carries exactly one verb per
/// subject: Start for the untracked subject, Stop for the subject
/// holding the open entry (no done-gate in the UI — R-8 is the
/// engine's).
#[test]
fn the_detail_button_carries_one_verb_per_subject() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        support::driver::DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            // The first card (task-0, untracked): Start only.
            harness.press(ActionId::NavFirstItem);
            harness.press(ActionId::TaskEdit);
            let model = harness.handle.window().get_task_model();
            assert!(model.can_track_start && !model.can_track_stop);

            // The tracked subject: Stop only.
            harness.press(ActionId::NavFocusBoard);
            harness.press(ActionId::NavNextItem);
            assert_eq!(
                harness.emphasized_card(),
                Some((0, 1)),
                "the walk reaches the tracked card"
            );
            harness.press(ActionId::TaskEdit);
            let model = harness.handle.window().get_task_model();
            assert!(!model.can_track_start && model.can_track_stop);
        },
    );
}

/// DR-3/3 — the awaiting pointer: a delivered (fired ∧ ¬dismissed)
/// alarm renders exactly one surface row (Cancel, no Mark Done) and
/// counts in the section's pointer badge; the dismissal removes both
/// by derivation.
#[test]
fn the_awaiting_alarm_renders_then_dismisses() {
    let (_fixture, mut state) = fixture::golden_state();
    let fired_at = chrono::Utc::now() - TimeDelta::minutes(1);
    let alarm = fixture::add_alarm(&mut state, "tea", fired_at);
    fixture::deliver_alarm(&mut state, alarm, fired_at);
    let state = state;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                1
            );
            assert_eq!(harness.handle.window().get_timers_awaiting(), 1);
            // Awaiting rows stay out of the active section (one
            // source of truth — D-I7).
            assert_eq!(harness.handle.window().get_timers_model().row_count(), 0);
            // The alarm row grew the Cancel affordance (D-P11).
            assert!(harness.driver.find_by_label("Cancel timer").is_some());

            harness.driver.click_by_label("Dismiss notification");
            match harness.engine.drain().as_slice() {
                [
                    StateCommand::DismissReminder {
                        reminder,
                        occurrence,
                    },
                ] => {
                    assert_eq!(*reminder, taskboard_domain::ReminderRef::Adhoc(alarm));
                    assert_eq!(*occurrence, fired_at);
                }
                other => panic!("the dismissal reached the boundary, got {other:?}"),
            }
            harness.engine.settle(&harness.handle);
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                0
            );
            assert_eq!(harness.handle.window().get_timers_awaiting(), 0);
            assert!(harness.driver.find_by_label("Cancel timer").is_none());
        },
    );
}

/// DR-3/3 — the row-template repair must not shrink the task side: a
/// task-anchored awaiting row keeps all four task-shaped affordances
/// (Done, Snooze, Mute — plus Dismiss) and grows no Cancel.
#[test]
fn a_task_notification_row_keeps_all_four_buttons() {
    let (golden, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, golden.task1, due);
    let state = state;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                1
            );
            let row = harness
                .handle
                .window()
                .get_notification_model()
                .row_data(0)
                .expect("the surface's single row");
            assert!(row.can_done);
            assert!(row.can_mute);
            assert!(!row.can_cancel);
            assert!(harness.driver.find_by_label("Mark task done").is_some());
            assert!(harness.driver.find_by_label("Mute reminders").is_some());
            assert!(harness.driver.find_by_label("Cancel timer").is_none());
        },
    );
}

// ---- R4-F6 (r4 design D2): a right-click outside an open overlay
// closes it — every full-window dismissal layer carries the same
// right-down arm the left-click `clicked` seam already has.

/// The menu layer: a right-press off the panel dismisses, typed via
/// `KeyboardState.menu-open` (the outside-left-click journey's twin).
#[test]
fn a_right_press_outside_dismisses_the_menu() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());
            harness.driver.right_press_at(10.0, 450.0);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the right-down closed the menu"
            );
        },
    );
}

/// The palette layer: the typed `palette-open` bit falls on a
/// right-press in the corner (the centered panel never reaches it).
#[test]
fn a_right_press_outside_dismisses_the_palette() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::PaletteSearch);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_palette_open());
            harness.driver.right_press_at(990.0, 890.0);
            assert!(
                !harness.handle.window().get_palette_open(),
                "the right-down closed the palette"
            );
        },
    );
}

/// The capture layer: the typed `capture-open` bit falls on a
/// right-press in the corner (the centered modal never reaches it).
#[test]
fn a_right_press_outside_dismisses_the_capture_overlay() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::CaptureQuick);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_capture_open());
            harness.driver.right_press_at(990.0, 890.0);
            assert!(
                !harness.handle.window().get_capture_open(),
                "the right-down closed the capture overlay"
            );
        },
    );
}

/// The date-picker layer: the typed `date-picker-open` bit falls on a
/// right-press far from the anchored panel.
#[test]
fn a_right_press_outside_dismisses_the_date_picker() {
    let (fixture, mut state) = fixture::journey_state();
    let due = local_midnight(2030, 1, 30);
    state
        .tasks
        .get_mut(&fixture.task0)
        .expect("fixture task")
        .duedate = Some(due);
    support::driver::run(
        DriverOptions {
            headless_renderer: false,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);
            harness.driver.set_window_size(1000, 900);
            harness.driver.right_press_at(990.0, 890.0);
            assert!(
                !harness.handle.window().get_task_model().date_picker_open,
                "the right-down closed the picker"
            );
        },
    );
}

/// The guard half of D2: the dismissal layer eats the right-press —
/// the menu closes AND no context menu opens behind it (the layer is
/// hit-tested before every context seam beneath it). The press lands
/// on stack a's bare body (well below the cards, and left of the
/// right-aligned menu panel, which keeps absorbing its own hits).
#[test]
fn a_right_press_on_the_bare_board_closes_the_menu_and_opens_nothing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let header = harness
                .driver
                .find_all_by_label("a")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("the stack-a header button is present");
            let pos = header.absolute_position();
            harness.press(ActionId::AppMenu);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());
            #[allow(clippy::cast_precision_loss)]
            let bare_x = pos.x + 10.0;
            harness.driver.right_press_at(bare_x, 700.0);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the layer's right-down closed the menu"
            );
            assert!(
                !keyboard(&harness).get_menu_pointer_anchored(),
                "no context menu opened behind the dismissal"
            );
        },
    );
}

// ---- R4-F7 (r4 design D3): a left-click on a column's bare body
// selects the stack — the body seam's left-click twin of the header.

/// The empty stack: a left-click on its bare body selects it — the
/// exact state the header journey pins (selected stack moved to the
/// clicked column).
#[test]
fn a_body_left_click_on_an_empty_stack_selects_it() {
    let (fixture, mut state) = fixture::golden_state();
    let stack_b = fixture.stack_b;
    state.tasks.retain(|_, task| task.stack != stack_b);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                -1,
                "a fresh mount pushes no selection"
            );
            let header = harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("the stack-b header button is present");
            let pos = header.absolute_position();
            let size = header.size();
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height * 3.0);
            harness.driver.click_at(x, y);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the body click selected stack b"
            );
        },
    );
}

/// The populated stack: a click below the last card (the pass-through
/// region) selects the stack and focuses its first card.
#[test]
fn a_body_left_click_below_the_last_card_selects_and_focuses() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let header = harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("the stack-b header button is present");
            let pos = header.absolute_position();
            #[allow(clippy::cast_precision_loss)]
            let (x, y) = (pos.x + 10.0, pos.y + 250.0);
            harness.driver.click_at(x, y);
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the below-last-card click selected stack b"
            );
            assert_eq!(
                harness.emphasized_card(),
                Some((1, 0)),
                "the stack's first card is focused"
            );
        },
    );
}

// ---- R4-F8 (r4 design D4): one physical click on a column's
// Add-task ghost opens the create field in THAT column and selects it
// — the targeting carried by the effect payload, not by selection
// ordering. The journeys below are the F8 harness: physical
// press+release delivery (`click_at`), never the a11y seam.

fn ghost_centre(harness: &Harness, index: usize) -> (f32, f32) {
    let ghost = harness
        .driver
        .find_all_by_label("Add task")
        .into_iter()
        .nth(index)
        .expect("the column's ghost is present");
    let pos = ghost.absolute_position();
    let size = ghost.size();
    #[allow(clippy::cast_precision_loss)]
    (pos.x + size.width / 2.0, pos.y + size.height / 2.0)
}

/// (a/b) Under `Flow::None`, one physical click on a non-active
/// column's ghost opens the create row in that column and moves the
/// selection there — one click, not two.
#[test]
fn one_ghost_click_opens_the_create_field_in_the_clicked_column() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            let (x, y) = ghost_centre(&harness, 1);
            harness.driver.click_at(x, y);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(1).expect("column b").create_open,
                "one click opened the create row in stack b"
            );
            assert_eq!(
                harness.handle.window().get_board_model().selected_stack,
                1,
                "the clicked column is selected"
            );
        },
    );
}

/// (c) Under a foreign flow (a stack rename armed on stack a), the
/// ghost click settles the flow and opens in the clicked column —
/// settle-then-run with no eaten click.
#[test]
fn a_ghost_click_under_a_foreign_flow_settles_and_opens_in_the_clicked_column() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            // Arm the foreign flow on stack a (the keyboard route).
            harness
                .driver
                .find_all_by_label("a")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack a's header")
                .invoke_default_action();
            harness.press(ActionId::StackRename);
            harness.driver.advance_time(0);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(0).expect("column a").rename_open,
                "the rename flow is armed on stack a"
            );
            let (x, y) = ghost_centre(&harness, 1);
            harness.driver.click_at(x, y);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                !stacks.row_data(0).expect("column a").rename_open,
                "the foreign flow settled"
            );
            assert!(
                stacks.row_data(1).expect("column b").create_open,
                "the create row opened in the clicked column"
            );
        },
    );
}

/// (d) With another column's create field open and focused, the ghost
/// click settles that flow and opens the clicked column's field.
#[test]
fn a_ghost_click_replaces_another_columns_open_create_field() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness
                .driver
                .find_all_by_label("a")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack a's header")
                .invoke_default_action();
            harness.press(ActionId::TaskCreate);
            harness.driver.advance_time(0);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(stacks.row_data(0).expect("column a").create_open);
            let (x, y) = ghost_centre(&harness, 1);
            harness.driver.click_at(x, y);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                !stacks.row_data(0).expect("column a").create_open,
                "the old create flow settled"
            );
            assert!(
                stacks.row_data(1).expect("column b").create_open,
                "the clicked column's field opened"
            );
        },
    );
}

/// (e) Ctrl+O is unchanged: with no selection the field opens in the
/// first stack; with a selection, in the selected stack.
#[test]
fn task_create_without_a_click_targets_the_selection_chain() {
    let (fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            // No selection: the first stack.
            harness.press(ActionId::TaskCreate);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(0).expect("column a").create_open,
                "no selection falls through to the first stack"
            );
            // A fresh mount, selection on stack b: the selected stack.
            let (_fixture, state) = fixture::golden_state();
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 800);
            harness
                .driver
                .find_all_by_label("b")
                .into_iter()
                .find(support::driver::Element::is_accessible_button)
                .expect("stack b's header")
                .invoke_default_action();
            harness.press(ActionId::TaskCreate);
            let stacks = harness.handle.window().get_board_model().stacks;
            assert!(
                stacks.row_data(1).expect("column b").create_open,
                "the selected stack wins"
            );
            let _ = fixture;
        },
    );
}

// -- r4 chunk 4 (D14): the dismiss journeys ------------------------------
//
// The structural walk pins the routing table; these five pin the
// user-facing transitions through the real seams (hover scan, delay
// fire, the production dispatch paths).

/// The F14 repro verbatim: hover a task card, let the delay fire, then
/// activate the focused card into the task view — the tooltip is gone.
/// (Pre-fix: the ghost tip rode the board→task transition.)
#[test]
fn a_tooltip_does_not_survive_activating_the_hovered_task() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            // Arm on the first card (the established hover-scan
            // pattern), and seed the card focus the activation needs.
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let pos = card.absolute_position();
            let size = card.size();
            let mut armed = false;
            let mut y = pos.y + 2.0;
            while y < pos.y + size.height - 2.0 {
                harness.driver.hover_at(pos.x + size.width / 2.0, y);
                harness.driver.advance_time(0);
                if tooltip_state(&harness).get_pending() {
                    armed = true;
                    break;
                }
                y += 4.0;
            }
            assert!(armed, "the card hover armed the trigger");
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::TaskEdit);
            harness.driver.advance_time(0);
            assert_eq!(
                current_view(&harness),
                View::TaskDetail,
                "the activation mounted the task view"
            );
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the activation dismissed the tooltip"
            );
        },
    );
}

/// The inbox jump: a visible tip does not survive `ViewInbox` — the
/// `OpenInbox` effect was missing from the old leaving set entirely.
#[test]
fn a_tooltip_does_not_survive_the_inbox_jump() {
    let (fixture, mut state) = fixture::two_board_state();
    // The Inbox board: the jump's target (the exact-title adoption).
    if let Some(board) = state.boards.get_mut(&fixture.board_two) {
        board.title = "Inbox".to_owned();
    }
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let pos = card.absolute_position();
            let size = card.size();
            harness
                .driver
                .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            // ViewInbox is bound in no preset (the D19 record): the
            // mouse path is the toolbar's inbox chip — a bare
            // TouchArea, so the coordinate click seam drives it.
            let chip = harness
                .driver
                .find_by_label("Go to inbox")
                .expect("the inbox chip is addressable");
            let cpos = chip.absolute_position();
            let csize = chip.size();
            harness
                .driver
                .click_at(cpos.x + csize.width / 2.0, cpos.y + csize.height / 2.0);
            harness.driver.advance_time(0);
            assert_eq!(
                current_view(&harness),
                View::StackDetail,
                "the inbox jump mounted the stack detail"
            );
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the inbox jump dismissed the tooltip"
            );
        },
    );
}

/// The palette invoke: a visible tip armed on the board does not
/// survive the palette's Return-jump across boards —
/// `apply_palette_invoke` lands in the choke point through
/// `InvokeSelected` (the tooltip is armed via the state global: the
/// palette row's own hover geometry is not this journey's seam).
#[test]
fn a_tooltip_does_not_survive_the_palette_invoke() {
    let (fixture, mut state) = fixture::two_board_state();
    let id = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut needle = state.tasks.values().next().expect("fixture task").clone();
    needle.id = id;
    needle.stack = fixture.stack_c;
    needle.order = 5;
    needle.title = "zebra needle".to_owned();
    state.tasks.insert(id, needle);

    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            // Arm a visible tip while the board view is mounted.
            tooltip_state(&harness).invoke_hover(
                "armed".into(),
                "".into(),
                slint::ModelRc::default(),
                500.0,
                300.0,
                32.0,
                32.0,
            );
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            // The palette round-trip: one unique cross-board hit,
            // Return jumps (the `menu.invoke` → `InvokeSelected` →
            // `apply_palette_invoke` path).
            harness.press(ActionId::PaletteSearch);
            harness.type_text("zebra needle");
            harness.stroke(parse("Return").expect("return"));
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_board_bar_model().selected,
                1,
                "the jump selected board two"
            );
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the palette invoke dismissed the tooltip"
            );
        },
    );
}

/// The ghost: a visible tip on the add-task ghost does not survive
/// the click that opens the create field — the hovered `TouchArea`
/// itself unmounts (the ghost case; the identity tuple does not move).
#[test]
fn a_tooltip_does_not_survive_the_ghost_click() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            let ghost = harness
                .driver
                .find_by_label("Add task")
                .expect("the add-task ghost is addressable");
            let pos = ghost.absolute_position();
            let size = ghost.size();
            harness
                .driver
                .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            harness.driver.click_by_label("Add task");
            harness.driver.advance_time(0);
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the ghost click dismissed the tooltip"
            );
        },
    );
}

/// The kiosk flip (J-K rider): a visible tip does not survive the
/// rotation's view swap — the flip happens outside `apply_effect`
/// entirely, so it carries its own dismiss.
#[test]
fn a_tooltip_does_not_survive_a_kiosk_flip() {
    let (_fixture, state, _walked) = fixture::today_overdue_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness =
                Harness::mount_with_settings(SlintDriver::default(), state, rotation_settings());
            assert_eq!(current_view(&harness), View::Board);
            // Arm on the first card of the board view.
            let card = harness
                .driver
                .find_by_label("task-0")
                .expect("the first card is addressable");
            let pos = card.absolute_position();
            let size = card.size();
            harness
                .driver
                .hover_at(pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            harness.driver.advance_time(600);
            assert!(
                tooltip_state(&harness).get_visible(),
                "the tip went visible"
            );

            // The dwell fires the flip (J-K1's boundary: 30 s).
            harness.driver.advance_time(30_000);
            harness.driver.advance_time(0);
            assert_eq!(current_view(&harness), View::Today, "the flip fired");
            let tooltip = tooltip_state(&harness);
            assert!(
                !tooltip.get_visible() && !tooltip.get_pending() && !tooltip.get_placed(),
                "the kiosk flip dismissed the tooltip"
            );
        },
    );
}

// ---- the tracker task kind (backlog round 2 chunk 4.2, #492) ---------

/// A tracker fixture: the golden state's task1 kind-marked (the same
/// card the context-menu journey addresses).
fn tracker_journey_state() -> (fixture::Fixture, AppState) {
    let (fixture, mut state) = fixture::journey_state();
    let mut tracker = state.tasks.get(&fixture.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    state.tasks.insert(fixture.task1, tracker);
    (fixture, state)
}

/// U3 — the task-view header checkbox hides for a tracker subject:
/// the detail model's flag passes through the push (the checkbox and
/// its focus/touch scopes mount inside the `if`), and the tracking
/// buttons stay on.
#[test]
fn the_task_view_hides_the_header_checkbox_for_a_tracker() {
    let (_fixture, state) = tracker_journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Walk to task1's detail: two item steps, two view steps.
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
            let model = harness.handle.window().get_task_model();
            assert!(
                !model.can_done,
                "the tracker subject hides the header checkbox"
            );
            assert!(
                !model.done,
                "the tracker is open — the status text stays data-driven"
            );
        },
    );
}

/// U5 — the tracker card's context menu omits the impossible complete
/// verb and offers the kind toggle; invoking the toggle sends the
/// inverted plain kind edit (the context-menu journey's shape).
#[test]
fn the_tracker_context_menu_omits_complete_and_offers_the_toggle() {
    let (fixture, state) = tracker_journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // The toggle is the Task page's last row: a tall window lets
            // the panel realize every row (the cap is host-pushed).
            harness.driver.set_window_size(1000, 1800);
            harness
                .handle
                .window()
                .invoke_board_task_context_requested(0, 1, 300.0, 300.0);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open());

            let entries = keyboard(&harness).get_menu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            assert!(
                rows.iter().all(|r| r.action != "task.complete"),
                "the tracker's menu omits Complete task"
            );
            assert!(
                rows.iter().any(|r| r.action == "task.reopen"),
                "the reopen no-op stays — only the impossible verb hides"
            );
            assert!(
                rows.iter().any(|r| r.action == "task.toggle-tracker"),
                "the menu offers the kind toggle"
            );

            harness.driver.advance_time(0);
            harness.driver.click_by_label("Toggle tracker");
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    kind: Some(Some(taskboard_domain::entities::TaskKind::Standard)),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);
            assert!(
                !keyboard(&harness).get_menu_open(),
                "the invocation closed the menu"
            );
        },
    );
}

/// U6 — the shared suppression under the kiosk pin: the same pinned
/// the app window renders the tracker card without the done checkbox and
/// the standard card with it — zero kiosk-specific code (D-11).
#[test]
fn the_done_checkbox_seam_is_inert_in_kiosk_mode() {
    let (fixture, mut state) = fixture::golden_state();
    let mut tracker = state.tasks.get(&fixture.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    tracker.done = None;
    state.tasks.insert(fixture.task1, tracker);
    let state = state;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                taskboard_ui_slint::UiSettings {
                    kiosk: true,
                    ..taskboard_ui_slint::UiSettings::default()
                },
            );
            assert_eq!(current_view(&harness), View::Board);
            // The tracker card's done affordance is gone ambiently; the
            // standard sibling's stays.
            let board = harness.handle.window().get_board_model();
            let mut hidden = 0;
            let mut shown = 0;
            for column in 0..board.stacks.row_count() {
                let tasks = board.stacks.row_data(column).expect("column").tasks;
                for row in 0..tasks.row_count() {
                    if tasks.row_data(row).expect("card").can_done {
                        shown += 1;
                    } else {
                        hidden += 1;
                    }
                }
            }
            assert_eq!(
                (shown, hidden),
                (2, 1),
                "exactly the tracker card hides its done box"
            );
            // Only the realized stack publishes a11y elements: the one
            // visible standard card keeps its checkbox, the tracker's
            // is gone ambiently.
            assert_eq!(
                harness
                    .driver
                    .find_by_label("Mark task done")
                    .iter()
                    .count(),
                1,
                "the visible standard card keeps its checkbox"
            );
        },
    );
}

/// U7 — a tracker task's notification rows drop the done button (the
/// existing per-row flag, the task-notification journey's twin).
#[test]
fn notification_rows_drop_the_done_button_for_a_tracker() {
    let (golden, mut state) = fixture::golden_state();
    let due = chrono::Utc::now() - TimeDelta::minutes(5);
    fixture::deliver_task_notification(&mut state, golden.task1, due);
    let mut tracker = state.tasks.get(&golden.task1).expect("task").clone();
    tracker.kind = taskboard_domain::entities::TaskKind::Tracker;
    state.tasks.insert(golden.task1, tracker);
    let state = state;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(
                harness.handle.window().get_notification_model().row_count(),
                1
            );
            let row = harness
                .handle
                .window()
                .get_notification_model()
                .row_data(0)
                .expect("the surface's single row");
            assert!(!row.can_done, "the tracker row drops the done button");
            assert!(row.can_mute, "the mute button is untouched");
            assert!(
                row.can_track_start,
                "tracking stays live (the liveness pin)"
            );
        },
    );
}

/// #214 — the review surface journey: the cycle's backwards wrap
/// lands on Review, the ledger model matches the fixture's completed
/// set (structure over text — row counts, emptiness flags, and the
/// dashes as structural absences), and nav.focus-board returns. The
/// rows are read-only (D-8): no activation exists to assert.
#[test]
fn the_review_surface_ledgers_and_returns() {
    let (fixture, mut state) = fixture::golden_state();
    let now = chrono::Utc::now();
    let span = |start: chrono::TimeDelta, secs: i64| {
        (
            now + start,
            now + start + chrono::TimeDelta::try_seconds(secs).unwrap(),
        )
    };
    let _ = fixture::add_completed_task(
        &mut state,
        fixture.stack_a,
        "compared-a",
        now - chrono::TimeDelta::try_hours(2).unwrap(),
        Some(5_400),
        Some(span(chrono::TimeDelta::try_hours(-3).unwrap(), 3_600)),
    );
    let _ = fixture::add_completed_task(
        &mut state,
        fixture.stack_a,
        "untracked-b",
        now - chrono::TimeDelta::try_hours(1).unwrap(),
        Some(600),
        None,
    );
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Cold start is the board; the backwards wrap crosses
            // Today into Review (#214 D-1).
            harness.press(ActionId::NavPrevView);
            assert_eq!(harness.handle.window().get_current_view(), View::Today);
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Review,
                "the wrap lands on the review surface"
            );
            let model = harness.handle.window().get_review_model();
            assert!(!model.empty, "the fixture completes two tasks");
            assert_eq!(model.rows.row_count(), 2, "the ledger mirrors calibrate");
            let first = model.rows.row_data(0).expect("row 0");
            assert_eq!(first.title, "untracked-b", "newest completion first");
            assert!(!first.estimate.is_empty(), "the estimate side is observed");
            assert!(
                first.actual.is_empty(),
                "an untracked actual is a structural absence"
            );
            let second = model.rows.row_data(1).expect("row 1");
            assert_eq!(second.title, "compared-a");
            assert!(!second.gap.is_empty(), "the settled comparison speaks");
            // The tracking sections (#623 slice A) speak structurally:
            // the planted work span keeps them populated and the strips
            // carry the windows' day counts — the read-only contract
            // below is unaffected.
            assert!(!model.tracking_month.empty);
            assert!(!model.tracking_month.work.is_empty());
            assert_eq!(model.tracking_week.bars.row_count(), 7);
            assert_eq!(model.tracking_month.bars.row_count(), 30);

            // nav.focus-board returns from everywhere (the walk's
            // contract).
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            harness.assert_commands(&[]); // a read-only view dispatches nothing
        },
    );
}

/// The entries overlay's keyboard journey (r4 chunk 10, D35/D36
/// blanket rule; stage-3 review): the nonce grabs the panel root, one
/// walk step lands on a row, Return activates it into the seeded edit
/// form, and the Escape ladder steps form → list → closed. The row
/// here is an unreported stop (the tracking verbs' precedent), so the
/// reachability pins ride the observable surfaces (the form flag, the
/// mount flag) — the arm/confirm semantics stay at the flow-table
/// layer.
#[test]
fn the_entries_overlay_walks_rows_and_the_escape_ladder_steps_back() {
    let (fx, mut state) = fixture::golden_state();
    // One closed entry on the walk's focus target: the overlay's row.
    let start = chrono::Utc::now() - chrono::TimeDelta::hours(1);
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task: fx.task0,
            task_title: "task-0".to_owned(),
            start,
            end: Some(start + chrono::TimeDelta::minutes(30)),
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        },
    );
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextItem,
            );
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::TaskEdit,
            );
            harness.driver.advance_time(0);
            // Open through the window-level dispatch seam; the
            // affordance's real click path (button → component
            // callback → mount forwarding → this seam) is the
            // through-the-real-mount journey's pin. The second loop
            // turn delivers the deferred nonce bump (the mount pass
            // ran in the first).
            harness.handle.window().invoke_task_track_entries_clicked();
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            assert!(
                window.get_task_model().track_entries_open,
                "the overlay mounts over the subject"
            );
            assert_eq!(
                window.get_task_model().track_entries.row_count(),
                1,
                "the subject's entry renders a row"
            );
            // The height-transition grab (post-mount) holds the walk
            // root — the fresh-mount init grab does not take (the
            // walk probe's P4/P5 split).
            {
                let walk_facts = harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::TrackPanelWalkFacts>();
                assert!(
                    walk_facts.get_panel_root_focused(),
                    "the height-transition grab lands on the panel root"
                );
            }
            // Row activation (the inventory's accessible-action
            // idiom): the row's a11y default action IS the
            // row-activated seam. The walk's step count across the
            // window-level mount stays the Tier-4 probe item (the
            // stop itself is structural — the row FocusScope markup
            // above; the control walk's ordering over the overlay is
            // runtime-sensitive and unproven here).
            let start_local = start.with_timezone(&chrono::Local);
            let row_label = start_local.format("%Y-%m-%d %H:%M").to_string();
            assert!(
                !harness.handle.window().get_track_form_open(),
                "the form stays closed before activation"
            );
            harness
                .driver
                .find_by_label(row_label.as_str())
                .expect("the entry row renders under its span label")
                .invoke_default_action();
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_track_form_open(),
                "activating the row opens the seeded edit form"
            );
            // The seed is the entry's own local wall-clock.
            assert_eq!(
                harness.handle.window().get_track_form_start_text(),
                start_local.format("%H:%M").to_string(),
                "the edit seeds the entry's local start"
            );
            // The ladder, level 2: Escape steps the form back to the
            // list — the overlay stays.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_track_form_open(),
                "the form steps back"
            );
            assert!(
                harness.handle.window().get_task_model().track_entries_open,
                "the overlay stays through the form's step"
            );
            // The ladder, level 3: Escape closes the overlay.
            harness.stroke(parse("Escape").expect("escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().track_entries_open,
                "the second Escape closes the overlay"
            );
        },
    );
}

/// The entries affordance's real click path (round-5 F1): the
/// "Time entries" button fires the view component's
/// `task-track-entries-clicked`, the window mount forwards it to the
/// window-level seam, and the overlay opens over the subject. The
/// window-level invoke alone is NOT this path — a dead mount
/// forwarding silently no-ops the click (F1), invisible to a
/// window-level journey.
#[test]
fn the_time_entries_affordance_opens_the_overlay_through_the_real_mount() {
    let (fx, mut state) = fixture::golden_state();
    // One closed entry on the subject: the overlay's row pins the
    // open went over fx.task0 (no other task carries entries).
    let start = chrono::Utc::now() - chrono::TimeDelta::hours(1);
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task: fx.task0,
            task_title: "task-0".to_owned(),
            start,
            end: Some(start + chrono::TimeDelta::minutes(30)),
            created_at: start,
            updated_at: start,
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        },
    );
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextItem,
            );
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::TaskEdit,
            );
            harness.driver.advance_time(0);
            // The real click: the button's accessible default action
            // IS the IconButton's `clicked` handler (§7.1's
            // click-equivalence seam — no coordinate arithmetic). The
            // overlay panel shares the label but is unmounted at
            // click time, so the address is unique.
            harness.driver.click_by_label("Time entries");
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            assert!(
                window.get_task_model().track_entries_open,
                "the overlay mounts over the subject through the real click"
            );
            assert_eq!(
                window.get_task_model().track_entries.row_count(),
                1,
                "the subject's entry renders a row"
            );
            let start_local = start.with_timezone(&chrono::Local);
            let row_label = start_local.format("%Y-%m-%d %H:%M").to_string();
            assert!(
                harness.driver.find_by_label(row_label.as_str()).is_some(),
                "the row is the subject entry's local span"
            );
        },
    );
}

/// The bar dropdown's modal ladder (r4 D40) under every keymap preset:
/// while the flyout is open, the ladder keys reach the widget — the
/// capture-phase resolver declines them (the shared decline arm) —
/// and the action layer dispatches nothing. The anchor is the viewed
/// board; Escape closes. (The bar probe cannot host this: the decline
/// arm lives on the app window's key-root capture, absent from a
/// standalone probe.)
#[test]
fn the_bar_dropdown_ladder_declines_the_action_layer_under_every_preset() {
    for (preset, name) in [
        (taskboard_ui_slint::keymap::Preset::Standard, "standard"),
        (taskboard_ui_slint::keymap::Preset::Emacs, "emacs"),
        (taskboard_ui_slint::keymap::Preset::Vi, "vi"),
    ] {
        let mut state = AppState::default();
        // Nine boards: past the segment budget (the bar's dropdown-at),
        // so the bar mounts the dropdown switcher.
        let boards: Vec<_> = (0..9)
            .map(|i| (uuid::Uuid::new_v4(), format!("board-{i}")))
            .collect();
        for (id, title) in &boards {
            state_board(&mut state, *id, title, None);
        }
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness = Harness::mount_with_settings(
                    SlintDriver::default(),
                    state,
                    UiSettings {
                        keymap: taskboard_ui_slint::keymap::Keymap::from_preset(preset),
                        keymap_preset: preset,
                        ..UiSettings::default()
                    },
                );
                harness.driver.set_window_size(1000, 900);
                harness.driver.advance_time(0);

                // Open via the field click (the click focuses the
                // field — the ladder's home; the element query is the
                // address in dropdown mode, where no segments render).
                let mut field = None;
                for _ in 0..20 {
                    harness.driver.advance_time(0);
                    let found = harness.driver.find_all_by_id("BoardBar::bar-dd");
                    if let Some(first) = found.into_iter().next() {
                        field = Some(first);
                        break;
                    }
                }
                let field = field.expect("{name}: the bar dropdown renders past the budget");
                let pos = field.absolute_position();
                let size = field.size();
                let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
                harness.driver.hover_at(x, y);
                harness.driver.advance_time(0);
                harness.driver.click_at(x, y);
                harness.driver.advance_time(30);

                assert!(
                    bar_facts(&harness).get_open(),
                    "{name}: the field click opens"
                );

                // The anchor is the viewed board (segment 0 here — the
                // engine's current board is the first inserted).
                let anchor = bar_facts(&harness).get_highlight();

                // Down moves the highlight, and the lane stays empty:
                // the capture-phase resolver declined the stroke.
                raw_key(&harness, slint::platform::Key::DownArrow);
                harness.driver.advance_time(0);
                assert_eq!(
                    bar_facts(&harness).get_highlight(),
                    anchor + 1,
                    "{name}: Down moves the ladder"
                );
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: the arrow dispatched nothing"
                );

                // The C-n spelling shares the arm — the emacs binding
                // must not win the capture race either.
                harness.stroke(parse("Ctrl+n").expect("Ctrl+n parses"));
                harness.driver.advance_time(0);
                assert_eq!(
                    bar_facts(&harness).get_highlight(),
                    anchor + 2,
                    "{name}: Ctrl+n moves the ladder too"
                );
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: the emacs stroke dispatched nothing"
                );

                // Escape closes without selecting; the lane stays
                // empty and the viewed board is unchanged.
                raw_key(&harness, slint::platform::Key::Escape);
                harness.driver.advance_time(0);
                assert!(!bar_facts(&harness).get_open(), "{name}: Escape closes");
                assert!(
                    harness.drain_requests().is_empty(),
                    "{name}: closing selected nothing"
                );
            },
        );
    }
}

// ---------------------------------------------------------------------
// The settings editor's general surface (the GUI settings editor
// chunk, J-1..J-7 of the plan's catalogue). Assertion discipline as
// everywhere: typed model properties and the typed requests that
// reached the lane boundary; label/status text is never matched.

use std::collections::BTreeSet;
use taskboard_ui_slint::requests::{
    ApplySemantics, ReminderSink, SectionApply, SettingsFact, SettingsSnapshot,
};

/// The general snapshot fixture (the hand-over's shape): typed
/// editable values, the exhaustive apply map, and the read-only
/// facts. Env-locked variants clone-and-lock.
fn general_snapshot() -> SettingsSnapshot {
    let mut applies = std::collections::BTreeMap::new();
    for key in [
        "preferences.day_start_time",
        "reminders.enabled",
        "reminders.tick_secs",
        "reminders.default_before",
        "reminders.sink",
        "sync.poll_interval_secs",
        "sync.backoff_initial_secs",
        "sync.backoff_max_secs",
        "ui.due_soon_days",
        "focus.enabled",
        "focus.work_mins",
        "focus.break_mins",
        "focus.chain",
        "focus.long_break_mins",
        "focus.long_break_every",
        "focus.notify",
    ] {
        applies.insert(key.to_owned(), ApplySemantics::on_restart("fixture"));
    }
    SettingsSnapshot {
        day_start_time: chrono::NaiveTime::from_hms_opt(5, 30, 0).expect("t"),
        reminders_enabled: true,
        reminder_tick_secs: 30,
        reminder_before: "5m".to_owned(),
        reminder_sink: ReminderSink::Auto,
        poll_interval_secs: 30,
        backoff_initial_secs: 5,
        backoff_max_secs: 300,
        due_soon_days: 1,
        focus_enabled: true,
        focus_work_mins: 25,
        focus_break_mins: 5,
        focus_chain: false,
        focus_long_break_mins: 15,
        focus_long_break_every: 4,
        focus_notify: true,
        env_overridden: BTreeSet::new(),
        facts: vec![
            SettingsFact {
                key: "app.mode",
                value: "desktop".to_owned(),
                apply: SectionApply::OnRestart,
            },
            SettingsFact {
                key: "web.token",
                value: "true".to_owned(),
                apply: SectionApply::OnRestart,
            },
            SettingsFact {
                key: "ui.stack_widths",
                value: "3".to_owned(),
                apply: SectionApply::Live,
            },
        ],
        applies,
    }
}

fn general_harness(state: AppState, snapshot: SettingsSnapshot) -> Harness {
    let settings = UiSettings {
        general: snapshot,
        ..UiSettings::default()
    };
    Harness::mount_with_settings(SlintDriver::default(), state, settings)
}

/// J-1 — the render journey: the new sections render the snapshot's
/// values (typed echoes), the read-only facts render with their apply
/// badges, and nothing dispatches at mount.
#[test]
fn the_general_sections_render_the_snapshot() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // The typed echoes (draft-free at mount).
            assert_eq!(model().get_day_start_echo(), "05:30");
            assert_eq!(model().get_tick_echo(), "30");
            assert_eq!(model().get_before_echo(), "5m");
            assert_eq!(model().get_poll_echo(), "30");
            assert_eq!(model().get_backoff_initial_echo(), "5");
            assert_eq!(model().get_backoff_max_echo(), "300");
            assert_eq!(model().get_due_soon_echo(), "1");
            assert_eq!(
                model().get_reminders_enabled_index(),
                0,
                "On renders selected"
            );
            assert_eq!(model().get_sink_index(), 0, "auto renders selected");

            // The read-only facts render one row per fact.
            assert_eq!(model().get_facts().row_count(), 3);
            let token_row = (0..model().get_facts().row_count())
                .find_map(|i| {
                    model()
                        .get_facts()
                        .row_data(i)
                        .filter(|row| row.key == "web.token")
                })
                .expect("the token presence row renders");
            assert_eq!(token_row.value, "true", "presence, never the value");
            let widths_row = (0..model().get_facts().row_count())
                .find_map(|i| {
                    model()
                        .get_facts()
                        .row_data(i)
                        .filter(|row| row.key == "ui.stack_widths")
                })
                .expect("the fenced-subtree presence row renders");
            assert!(!widths_row.on_restart, "the fenced subtree applies live");

            // The apply facts render per editable field (J-5's
            // structural presence, pinned here for the group).
            assert!(model().get_day_start_on_restart());
            assert!(model().get_tick_on_restart());
            assert!(model().get_reminders_enabled_on_restart());
            assert!(model().get_due_soon_on_restart());

            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// The apply badges render the registry's verdicts mechanically
/// (round-5 chunk 5, D-5.6): a `Live` key renders without the restart
/// badge, a justified `OnRestart` key with it — the structural
/// `on-restart` property, never label text.
#[test]
fn the_apply_badges_follow_the_registry_verdicts() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut snapshot = general_snapshot();
            snapshot
                .applies
                .insert("reminders.tick_secs".to_owned(), ApplySemantics::live());
            snapshot.applies.insert(
                "preferences.day_start_time".to_owned(),
                ApplySemantics::live(),
            );
            snapshot.applies.insert(
                "reminders.sink".to_owned(),
                ApplySemantics::on_restart("the sink is one boot arc"),
            );
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();
            assert!(
                !model().get_tick_on_restart(),
                "a live key carries no badge"
            );
            assert!(!model().get_day_start_on_restart());
            assert!(model().get_sink_on_restart(), "a justified key carries it");
            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// J-2 — the edit round-trip (the acceptance core): a valid edit
/// commits, exactly the typed intent dispatches with the minted
/// payload, the fresh-snapshot reply re-renders the row's echo (never
/// the click), and the outcome lands Ok.
#[test]
fn the_general_edit_round_trip_routes_and_echoes_the_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Stage the edit (per keystroke), commit on Return's seam.
            window.invoke_settings_field_edited(6, "45".into());
            assert_eq!(model().get_tick_echo(), "45", "the draft is the echo");
            window.invoke_settings_general_committed(6, "45".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one typed intent");
            let taskboard_ui_slint::requests::UiRequest::SetReminderTick { secs, reply } =
                requests.remove(0)
            else {
                panic!("the tick intent routed");
            };
            assert_eq!(secs, 45, "the minted payload rides the lane");

            // The reply carries the fresh snapshot; the echo follows
            // the reply (the committed value), not the click.
            let mut fresh = general_snapshot();
            fresh.reminder_tick_secs = 45;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_tick_echo(), "45");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok
            );

            // The choice grammar: pick dispatches on the pick (the
            // keymap preset's rule), the reply re-renders the index.
            window.invoke_settings_reminders_picked(1);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetRemindersEnabled { enabled, reply } =
                requests.remove(0)
            else {
                panic!("the toggle intent routed");
            };
            assert!(!enabled);
            let mut fresh = general_snapshot();
            fresh.reminders_enabled = false;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_reminders_enabled_index(),
                1,
                "the selection echoes the replied value"
            );
            window.invoke_settings_sink_picked(2);
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetReminderSink { sink, reply } =
                requests.remove(0)
            else {
                panic!("the sink intent routed");
            };
            assert_eq!(sink, ReminderSink::Desktop);
            let mut fresh = general_snapshot();
            fresh.reminder_sink = ReminderSink::Desktop;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_sink_index(), 2);
        },
    );
}

/// J-3 — the typed-refusal journey: an unparseable day start, a zero
/// tick, and the `initial > max` crossing refuse at the seam — no
/// request dispatches, the field's validity flavour renders the typed
/// kind, the status line renders the refusal outcome. A refused draft
/// stays visible (the typo remains under its flavour).
#[test]
fn invalid_general_commits_refuse_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(6, "0".into());
            window.invoke_settings_general_committed(6, "0".into());
            assert!(
                harness.drain_requests().is_empty(),
                "a refusal routes nothing"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            assert_eq!(
                model().get_validity_tick(),
                taskboard_ui_slint::FieldValidity::Invalid,
                "the flavour renders the typed kind"
            );

            // Unparseable day start: same shape (the draft staged
            // first — the flavour renders from the staged buffer, the
            // scale field's grammar).
            window.invoke_settings_field_edited(5, "abc".into());
            window.invoke_settings_general_committed(5, "abc".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_validity_day_start(),
                taskboard_ui_slint::FieldValidity::Invalid
            );

            // The ordering rule (plan D8): initial above the committed
            // max refuses at the seam, attributed to the offending
            // field — no lane traffic.
            window.invoke_settings_field_edited(9, "400".into());
            window.invoke_settings_general_committed(9, "400".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the crossing refuses visibly at the seam (the status                  line carries it; the per-field flavour is the                  per-keystroke predicate's)"
            );
            // Symmetric: max below the committed initial.
            window.invoke_settings_field_edited(10, "3".into());
            window.invoke_settings_general_committed(10, "3".into());
            assert!(harness.drain_requests().is_empty());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );

            // A valid commit clears the stale drafts' flavours: the
            // tick edit refused above stays in its draft; focus loss
            // discards it (the scale field's grammar).
            window.invoke_settings_general_focus_lost(6);
            assert_eq!(
                model().get_validity_tick(),
                taskboard_ui_slint::FieldValidity::Neutral,
                "the discarded draft renders neutral again"
            );
            assert_eq!(model().get_tick_echo(), "30", "the echo re-syncs");
        },
    );
}

/// J-4 — the env-override journey: a key in the hand-over's env set
/// renders read-only (the env-locked fact present, the badge fact
/// pushed), and its commit is absorbed at the defence arm — nothing
/// dispatches, nothing pretends to apply.
#[test]
fn env_overridden_keys_render_locked_and_refuse_the_seam() {
    let (_fixture, state) = fixture::golden_state();
    let mut snapshot = general_snapshot();
    snapshot.env_overridden = BTreeSet::from(["reminders.tick_secs".to_owned()]);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert!(model().get_tick_env_locked(), "the env fact rides the row");

            // The defence in depth: even a routed commit on the
            // locked key never dispatches (the effective value cannot
            // move while the env stands).
            window.invoke_settings_general_committed(6, "45".into());
            assert!(
                harness.drain_requests().is_empty(),
                "the locked key never dispatches"
            );

            // A reply must not lift the lock: the env set is
            // hand-over state, preserved across replies.
            window.invoke_settings_general_committed(11, "7".into());
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetDueSoonDays { reply, .. } =
                requests.remove(0)
            else {
                panic!("the unlocked field routed");
            };
            let fresh = general_snapshot();
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(
                model().get_tick_env_locked(),
                "the reply preserved the standing env set"
            );
        },
    );
}

/// An empty Return refuses typed and dispatches nothing: the field's
/// `accepted` seam fires on Return regardless of content, and the
/// mirror's empty contract is Neutral — so an empty Return reaches the
/// commit seam where the typed mints `expect` a validated parse. The
/// refusal is the visible Rejected outcome, never a mint attempt.
#[test]
fn an_empty_return_refuses_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Every text seam, committed empty (the untouched field's
            // Return) and whitespace-only (the cleared field's
            // Return): no mint attempt, no lane traffic.
            for index in [5, 6, 7, 8, 9, 10, 11] {
                for text in ["", "   "] {
                    window.invoke_settings_general_committed(index, text.into());
                    assert!(
                        harness.drain_requests().is_empty(),
                        "an empty commit on field {index} routes nothing"
                    );
                    assert_eq!(
                        model().get_last_outcome(),
                        taskboard_ui_slint::SettingsOutcome::Rejected,
                        "field {index}: the empty commit refuses visibly"
                    );
                }
            }
        },
    );
}

/// J-6 — the general fields' draft lifecycle: a Return-less edit
/// never survives the surface (the scale field's §5.1 invariant,
/// extended to the new fields — the mount dies the staged buffer and
/// the echo re-renders the committed value). The insert-mask
/// engagement itself is the shared `TextField` widget's D18 machinery,
/// pinned once by the editing-facts lifecycle journey (the testing
/// backend cannot focus a settings field: the recorded
/// "a `TouchArea` click never moves focus" negative, and the fields
/// opt out of autofocus by the initial-focus contract — the plan's
/// focus/commit halves are therefore pinned at the draft seam here).
#[test]
fn a_general_return_less_edit_never_survives_the_surface() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            // Stage a Return-less edit: the draft is the echo.
            window.invoke_settings_field_edited(7, "90m".into());
            assert_eq!(model().get_before_echo(), "90m", "the draft renders");

            // The surface closes and reopens: the staged draft died.
            harness.press(ActionId::AppSettings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "5m",
                "the mount re-renders the committed echo"
            );

            // The focus-loss seam shares the grammar: a staged draft
            // dies and the echo re-syncs without any surface change.
            window.invoke_settings_field_edited(7, "2h".into());
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "2h"
            );
            window.invoke_settings_general_focus_lost(7);
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_before_echo(),
                "5m",
                "the focus-loss discard re-syncs the echo"
            );
        },
    );
}

/// J-7 — the busy machine covers the new arms: an editor op in flight
/// disables the other settings ops — a second commit during the
/// flight refuses visibly and routes nothing (the D34 machine's one
/// op at a time).
#[test]
fn a_general_op_in_flight_gates_the_other_settings_ops() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(6, "45".into());
            assert!(model().get_busy(), "the busy gate is up");

            // A second editor commit mid-flight: visible refusal,
            // nothing routes.
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );

            // The reply lands: the gate lifts, the next op routes.
            let mut lane = harness.requests_rx.lock().expect("requests lock");
            let drained: Vec<_> = std::iter::from_fn(|| lane.try_recv().ok()).collect();
            assert_eq!(
                drained.len(),
                1,
                "exactly the first op is in the lane — the busy gate                  routed nothing extra"
            );
            let taskboard_ui_slint::requests::UiRequest::SetReminderTick { reply, .. } =
                drained.into_iter().next().expect("the op routes")
            else {
                panic!("the tick op routes");
            };
            drop(lane);
            reply
                .send(Ok(general_snapshot()))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy());
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(harness.drain_requests().len(), 1, "the gate lifted");
        },
    );
}

// ---- #625: the focus settings editor's journeys (J-F1..J-F9) -----------

/// J-F1 — the render journey: the seven focus rows render the
/// snapshot's typed echoes and segmented indices, the on-restart
/// facts ride the rows, the promoted keys are absent from the facts
/// model, and nothing dispatches at mount.
#[test]
fn the_focus_rows_render_the_snapshot() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert_eq!(model().get_focus_work_echo(), "25");
            assert_eq!(model().get_focus_break_echo(), "5");
            assert_eq!(model().get_focus_long_break_echo(), "15");
            assert_eq!(model().get_focus_every_echo(), "4");
            assert_eq!(model().get_focus_enabled_index(), 0, "On renders selected");
            assert_eq!(model().get_focus_chain_index(), 1, "Off renders selected");
            assert_eq!(model().get_focus_notify_index(), 0, "On renders selected");

            // The on-restart captions' facts are pushed per row.
            assert!(model().get_focus_work_on_restart());
            assert!(model().get_focus_every_on_restart());
            assert!(model().get_focus_enabled_on_restart());
            assert!(model().get_focus_notify_on_restart());

            // The promoted keys left the facts: a key renders exactly
            // once, never both editable and read-only (D-FS2).
            for row_index in 0..model().get_facts().row_count() {
                let row = model().get_facts().row_data(row_index).expect("row");
                assert!(
                    !row.key.starts_with("focus."),
                    "the {key} fact left the read-only list",
                    key = row.key
                );
            }

            assert!(
                harness.drain_requests().is_empty(),
                "rendering dispatches nothing"
            );
        },
    );
}

/// J-F2 — the edit round-trip: a staged edit commits on Return,
/// exactly the typed intent dispatches with the minted payload, the
/// fresh-snapshot reply re-renders the echo; a pick dispatches the
/// toggle intent and the reply re-renders the index.
#[test]
fn a_focus_edit_round_trips_and_echoes_the_reply() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(12, "45".into());
            assert_eq!(model().get_focus_work_echo(), "45", "the draft renders");
            window.invoke_settings_general_committed(12, "45".into());
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one typed intent");
            let taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { mins, reply } =
                requests.remove(0)
            else {
                panic!("the work-minutes intent routed");
            };
            assert_eq!(mins, 45, "the minted payload rides the lane");

            let mut fresh = general_snapshot();
            fresh.focus_work_mins = 45;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(model().get_focus_work_echo(), "45");
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Ok
            );

            // The pick grammar: dispatch on the pick, the reply
            // re-renders the selection.
            window.invoke_settings_focus_enabled_picked(1);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1);
            let taskboard_ui_slint::requests::UiRequest::SetFocusEnabled { enabled, reply } =
                requests.remove(0)
            else {
                panic!("the toggle intent routed");
            };
            assert!(!enabled);
            let mut fresh = general_snapshot();
            fresh.focus_enabled = false;
            reply.send(Ok(fresh)).expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model().get_focus_enabled_index(),
                1,
                "the selection echoes the replied value"
            );
        },
    );
}

/// J-F3 — the typed-refusal journey: out-of-bounds minutes refuse at
/// the seam — the validity flavour renders Invalid, the status shows
/// the refusal outcome, zero requests dispatch, and the draft stays
/// visible under its flavour.
#[test]
fn invalid_focus_commits_refuse_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            for text in ["0", "241"] {
                window.invoke_settings_field_edited(12, text.into());
                window.invoke_settings_general_committed(12, text.into());
                assert!(
                    harness.drain_requests().is_empty(),
                    "{text}: a refusal routes nothing"
                );
                assert_eq!(
                    model().get_validity_focus_work(),
                    taskboard_ui_slint::FieldValidity::Invalid,
                    "{text}: the flavour renders the typed kind"
                );
                assert_eq!(
                    model().get_last_outcome(),
                    taskboard_ui_slint::SettingsOutcome::Rejected
                );
                assert_eq!(
                    model().get_focus_work_echo(),
                    text,
                    "{text}: the refused draft stays visible"
                );
            }

            for text in ["0", "25"] {
                window.invoke_settings_field_edited(15, text.into());
                window.invoke_settings_general_committed(15, text.into());
                assert!(
                    harness.drain_requests().is_empty(),
                    "{text}: the every bound refuses without routing"
                );
                assert_eq!(
                    model().get_validity_focus_every(),
                    taskboard_ui_slint::FieldValidity::Invalid
                );
            }

            // The discarded draft renders neutral again.
            window.invoke_settings_general_focus_lost(12);
            assert_eq!(
                model().get_validity_focus_work(),
                taskboard_ui_slint::FieldValidity::Neutral
            );
            assert_eq!(model().get_focus_work_echo(), "25", "the echo re-syncs");
        },
    );
}

/// J-F4 — the env-override journey: a focus key in the hand-over env
/// set locks its row and the commit/pick is absorbed at the defence
/// arm — nothing dispatches.
#[test]
fn env_locked_focus_rows_render_locked_and_refuse_the_seam() {
    let (_fixture, state) = fixture::golden_state();
    let mut snapshot = general_snapshot();
    snapshot.env_overridden =
        BTreeSet::from(["focus.work_mins".to_owned(), "focus.notify".to_owned()]);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, snapshot);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            assert!(model().get_focus_work_env_locked());
            assert!(model().get_focus_notify_env_locked());
            assert!(!model().get_focus_break_env_locked());

            window.invoke_settings_general_committed(12, "45".into());
            window.invoke_settings_focus_notify_picked(1);
            assert!(
                harness.drain_requests().is_empty(),
                "the locked keys never dispatch"
            );
        },
    );
}

/// J-F5 — the empty-Return arm at the new indices: no mint attempt,
/// no lane traffic, a visible typed refusal.
#[test]
fn an_empty_focus_return_refuses_typed_without_routing() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            for index in [12, 13, 14, 15] {
                for text in ["", "   "] {
                    window.invoke_settings_general_committed(index, text.into());
                    assert!(
                        harness.drain_requests().is_empty(),
                        "an empty commit on field {index} routes nothing"
                    );
                    assert_eq!(
                        model().get_last_outcome(),
                        taskboard_ui_slint::SettingsOutcome::Rejected,
                        "field {index}: the empty commit refuses visibly"
                    );
                }
            }
        },
    );
}

/// J-F6 — the draft lifecycle arm: a Return-less focus edit never
/// survives — focus loss re-syncs the echo to the committed value.
#[test]
fn a_focus_return_less_edit_never_survives_the_surface() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_field_edited(12, "60".into());
            assert_eq!(model().get_focus_work_echo(), "60", "the draft renders");

            window.invoke_settings_general_focus_lost(12);
            assert_eq!(
                model().get_focus_work_echo(),
                "25",
                "the focus-loss discard re-syncs the echo"
            );

            // The mount shares the grammar: closing and reopening the
            // surface kills a staged draft.
            window.invoke_settings_field_edited(15, "8".into());
            harness.press(ActionId::AppSettings);
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            assert_eq!(
                window
                    .global::<taskboard_ui_slint::SettingsModel>()
                    .get_focus_every_echo(),
                "4",
                "the mount re-renders the committed echo"
            );
        },
    );
}

/// J-F7 — the busy machine covers the focus seams: an editor op in
/// flight gates the other settings ops (one op at a time).
#[test]
fn a_focus_op_in_flight_gates_the_other_settings_ops() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(12, "30".into());
            assert!(model().get_busy(), "the busy gate is up");

            window.invoke_settings_general_committed(11, "7".into());
            window.invoke_settings_focus_enabled_picked(1);
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected
            );
            let mut lane = harness.requests_rx.lock().expect("requests lock");
            let drained: Vec<_> = std::iter::from_fn(|| lane.try_recv().ok()).collect();
            drop(lane);
            assert_eq!(
                drained.len(),
                1,
                "exactly the first op is in the lane — the gate routed nothing extra"
            );
            let taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { reply, .. } =
                drained.into_iter().next().expect("the op routes")
            else {
                panic!("the focus op routes");
            };
            reply
                .send(Ok(general_snapshot()))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert!(!model().get_busy(), "the reply lifted the gate");
        },
    );
}

/// J-F8 — the failed-persist revert: an `Err` reply leaves the VM's
/// snapshot untouched, so the echo re-derives from the last-known-good
/// file value; the outcome renders typed; no retry loop, no stuck
/// Busy.
#[test]
fn a_failed_focus_persist_reverts_the_echo_and_shows_the_outcome() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();
            let model = || window.global::<taskboard_ui_slint::SettingsModel>();

            window.invoke_settings_general_committed(13, "10".into());
            let mut requests = harness.drain_requests();
            let taskboard_ui_slint::requests::UiRequest::SetFocusBreakMins { reply, .. } =
                requests.remove(0)
            else {
                panic!("the break intent routed");
            };
            reply
                .send(Err(
                    taskboard_ui_slint::requests::SettingsError::PersistRejected,
                ))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();

            assert_eq!(
                model().get_focus_break_echo(),
                "5",
                "the failed value never sticks — the echo re-derives"
            );
            assert_eq!(
                model().get_last_outcome(),
                taskboard_ui_slint::SettingsOutcome::Rejected,
                "the typed outcome renders"
            );
            assert!(!model().get_busy(), "no stuck Busy");

            // No retry loop: the next op routes normally.
            window.invoke_settings_general_committed(11, "7".into());
            assert_eq!(harness.drain_requests().len(), 1);
        },
    );
}

/// J-F9 — the commit-on-release structural pin (D-FS8): these rows
/// mount no gesture widget — per-keystroke edits dispatch zero
/// requests; only the single Return seam routes, one intent per
/// commit.
#[test]
fn focus_keystrokes_never_dispatch_until_the_commit_seam() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = general_harness(state, general_snapshot());
            harness.press(ActionId::AppSettings);
            let window = harness.handle.window();

            for partial in ["4", "45", "450"] {
                window.invoke_settings_field_edited(12, partial.into());
            }
            assert!(
                harness.drain_requests().is_empty(),
                "keystrokes dispatch nothing"
            );

            window.invoke_settings_general_committed(12, "45".into());
            let requests = harness.drain_requests();
            assert_eq!(
                requests.len(),
                1,
                "the single Return seam dispatches exactly one intent"
            );
            assert!(matches!(
                requests[0],
                taskboard_ui_slint::requests::UiRequest::SetFocusWorkMins { .. }
            ));
        },
    );
}

// ---- #622: the plan-next-day surface's journeys ------------------------

/// J-P1 — the opener routes to the planner and Escape restores the
/// exact return view (the J3 shape): open from the board, Escape back
/// onto the board, re-open, and the toggle closes.
#[test]
fn planner_open_closes_and_toggles() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);

            harness.press(ActionId::ViewPlanNextDay);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the opener opened the planner"
            );
            assert_eq!(
                harness.chain(),
                vec![
                    ContextId::PlanNextDay,
                    ContextId::ViNormal,
                    ContextId::Global,
                ],
                "the planner context heads the chain"
            );

            // Escape (the planner-context binding) restores the exact
            // return view.
            harness.stroke(parse("Escape").expect("Escape"));
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "Escape restored the exact previous view"
            );

            // Re-open, then the toggle: a second opener dispatch closes.
            harness.press(ActionId::ViewPlanNextDay);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            harness.press(ActionId::ViewPlanNextDay);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
        },
    );
}

/// J-P2 — cycle exclusion: with the planner open the next/prev-view
/// verbs change nothing; from the return view the full cycle never
/// lands on the planner (the J4 shape).
#[test]
fn the_view_cycle_never_passes_through_the_planner() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);

            harness.press(ActionId::ViewPlanNextDay);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavPrevView);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the cycle verbs are inert while the planner is open"
            );
            harness.press(ActionId::ViewPlanNextDay);

            // A full cycle walk from the board: the planner is never a
            // member.
            for expected in [
                View::StackDetail,
                View::TaskDetail,
                View::Review,
                View::Today,
                View::Board,
            ] {
                harness.press(ActionId::NavNextView);
                assert_eq!(harness.handle.window().get_current_view(), expected);
            }
        },
    );
}

/// J-P3 — the placement: pick a pool row, type the time-of-day and an
/// effort, commit — ONE `UpdateTask` with both masks reaches the
/// boundary; the row moves to the placed pane; `AppUndo` routes the
/// undo step.
#[test]
#[allow(clippy::too_many_lines)] // the placement journey's ceremony
fn the_placement_routes_one_mask_and_undo_returns_it() {
    // The clock pinned (docs/testing_strategy.org §6.2): the pass's
    // next-day window and the expected instant derive from the one
    // `now`, so neither side can straddle a day boundary the other
    // does not see.
    let now = fixture::PINNED_NOW;
    let (fixture, state) = fixture::golden_state_at(now);
    let task0 = fixture.task0;
    let title = state.tasks.get(&task0).expect("fixture task").title.clone();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    clock: Some(now),
                    ..UiSettings::default()
                },
            );
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);

            // The pool renders the candidate: pick task0 (the
            // row affordance's addressable label).
            let row = harness
                .driver
                .find_by_label(&format!("Place {title}"))
                .unwrap_or_else(|| panic!("task0's pool row is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_plan_model().placing,
                "the placement form armed"
            );

            // Type the time-of-day and the optional effort, commit.
            click_field_centre(&mut harness, "Time");
            harness.type_text("09:30");
            click_field_centre(&mut harness, "Effort (optional)");
            harness.type_text("30");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);

            // The one honest write: both masks in one command (the
            // window derives from the same pinned instant the mount
            // froze — no clock read on either side).
            let day_start = (
                taskboard_domain::SettingsSnapshot::default().day_start_time,
                UiSettings::default().boot_offset,
            );
            let (next_start, _) =
                taskboard_domain::days::next_day_window(now, day_start.0, day_start.1);
            let date = next_start.date_naive();
            let time = chrono::NaiveTime::from_hms_opt(9, 30, 0).expect("09:30");
            let expected_instant = taskboard_domain::DayCalendar {
                day_start_time: day_start.0,
                offset: day_start.1,
            }
            .instant(date, time);
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { id, changes }] => {
                    assert_eq!(*id, task0, "the addressed row");
                    assert_eq!(changes.scheduled, Some(Some(expected_instant)));
                    assert_eq!(
                        changes.effort,
                        Some(Some(
                            taskboard_domain::Effort::parse_text_form("30").expect("30 parses")
                        ))
                    );
                }
                other => panic!("one placement write, got {other:?}"),
            }

            // The fake engine folds the write in (fixture
            // continuity): the row moved to the placed pane; the pool
            // shrank.
            harness.engine.settle(&harness.handle);
            let plan = harness.handle.window().get_plan_model();
            assert_eq!(plan.boxes.row_count(), 1, "the box rendered");
            assert_eq!(plan.pool.row_count(), 2, "the pool shrank");
            assert!(!plan.placing, "the form disarmed on commit");

            // The undo arm: AppUndo routes the engine's undo verb (the
            // engine semantics are the undo suite's own contract).
            harness.press(ActionId::AppUndo);
            match harness.engine.drain().as_slice() {
                [StateCommand::Undo] => {}
                other => panic!("one undo step, got {other:?}"),
            }
        },
    );
}

/// J-P4 — the reorder: two placed boxes; "Move box later" on the
/// earlier box routes the two honest `UpdateTask` swap writes; two
/// undo steps route back.
#[test]
fn the_reorder_routes_the_two_swap_writes() {
    // The clock pinned (docs/testing_strategy.org §6.2): the seeds
    // and the pass both derive from the one `now`, so "tomorrow" is
    // the same window on both sides of the boundary at any wall time.
    let now = fixture::PINNED_NOW;
    let (fixture, mut state) = fixture::golden_state_at(now);
    let (task0, task1) = (fixture.task0, fixture.task1);
    // Two boxes tomorrow: 09:00 and 11:00 off the frozen next-day
    // window (the default day-start pair makes it the next UTC
    // midnight block).
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let (next_start, _) = taskboard_domain::days::next_day_window(now, day_start.0, day_start.1);
    let early = next_start + TimeDelta::hours(9);
    let late = next_start + TimeDelta::hours(11);
    state.tasks.get_mut(&task0).expect("task0").scheduled = Some(early);
    state.tasks.get_mut(&task1).expect("task1").scheduled = Some(late);
    let title0 = state.tasks.get(&task0).expect("task0").title.clone();
    let title1 = state.tasks.get(&task1).expect("task1").title.clone();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    clock: Some(now),
                    ..UiSettings::default()
                },
            );
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);

            // Both boxes render, the earlier one first (the day
            // shape's scheduled order).
            assert!(
                harness
                    .driver
                    .find_by_label(&format!("Box {title0}"))
                    .is_some(),
                "the earlier box renders"
            );
            assert_eq!(
                harness.handle.window().get_plan_model().boxes.row_count(),
                2
            );

            // "Move box later" on the earlier box: the two swap
            // writes, instants exchanged, ids preserved.
            let row = harness
                .driver
                .find_by_label("Move box later")
                .unwrap_or_else(|| panic!("the later affordance is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [
                    a @ StateCommand::UpdateTask { .. },
                    b @ StateCommand::UpdateTask { .. },
                ] => {
                    for (command, expected_id, expected_instant) in
                        [(a, task0, late), (b, task1, early)]
                    {
                        let StateCommand::UpdateTask { id, changes } = command else {
                            unreachable!("matched above")
                        };
                        assert_eq!(*id, expected_id, "the swap's ids");
                        assert_eq!(changes.scheduled, Some(Some(expected_instant)));
                    }
                }
                other => panic!("two swap writes, got {other:?}"),
            }

            // The fake engine folds the swap in: the display order
            // flips (the late box now leads, `(scheduled, order, id)`).
            harness.engine.settle(&harness.handle);
            let boxes = harness.handle.window().get_plan_model().boxes;
            assert_eq!(boxes.row_count(), 2);
            assert_eq!(
                boxes.row_data(0).expect("box").card.title.to_string(),
                title1,
                "the late box leads after the swap"
            );

            // The undo arms: two honest steps back.
            harness.press(ActionId::AppUndo);
            harness.press(ActionId::AppUndo);
            let drained = harness.engine.drain();
            assert_eq!(drained.len(), 2, "the swap steps back as two writes");
            assert!(drained.iter().all(|c| matches!(c, StateCommand::Undo)));
        },
    );
}

/// J-P5 — the planner is reachable from every cycle view by its
/// action and escapable everywhere: no dead ends (the walk's reach
/// contract, D-4).
#[test]
fn the_planner_is_reachable_and_escapable_from_every_cycle_view() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Visit every cycle member; from each, the opener reaches
            // the planner and Escape restores that exact member.
            let members = [
                (ActionId::NavNextView, View::StackDetail),
                (ActionId::NavNextView, View::TaskDetail),
                (ActionId::NavNextView, View::Review),
                (ActionId::NavNextView, View::Today),
                (ActionId::NavNextView, View::Board),
            ];
            for (_, expected) in members {
                harness.press(ActionId::NavNextView);
                assert_eq!(harness.handle.window().get_current_view(), expected);
                harness.press(ActionId::ViewPlanNextDay);
                assert_eq!(
                    harness.handle.window().get_current_view(),
                    View::PlanNextDay
                );
                harness.stroke(parse("Escape").expect("Escape"));
                assert_eq!(
                    harness.handle.window().get_current_view(),
                    expected,
                    "Escape restored the exact member"
                );
            }
        },
    );
}

/// J-P6 (the kiosk pin's journey half): a kiosk mount never shows the
/// planner and its opener dispatch is structurally inert.
#[test]
fn the_kiosk_mount_never_shows_the_planner() {
    let (_fixture, state) = fixture::golden_state();
    let settings = UiSettings {
        kiosk: true,
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::ViewPlanNextDay);
            assert_ne!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the kiosk dispatch is inert"
            );
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
        },
    );
}

// -- round-5 chunk 3: the anchored flyouts (F8/F9/F10/F14, D6) ----------

/// The quick-list flyout's realized facts, flattened for the journey
/// assertions: `(open, site, panel-x, panel-y, anchor-x, anchor-y,
/// anchor-height)` (the global handle borrows the window; the tuple
/// crosses the borrow out).
fn quick_list_facts(
    window: &taskboard_ui_slint::AppWindow,
) -> (bool, i32, f32, f32, f32, f32, f32) {
    let facts = window.global::<taskboard_ui_slint::TimeFieldFacts>();
    (
        facts.get_open(),
        facts.get_site(),
        facts.get_panel_x(),
        facts.get_panel_y(),
        facts.get_anchor_x(),
        facts.get_anchor_y(),
        facts.get_anchor_height(),
    )
}

/// Round-5 F8's calendar-toggle seam (D-3.4 seam 2): the calendar
/// button's click grab must not settle the armed editor — the toggle
/// opens the picker over the typed buffer, the flow stays armed, and
/// the field keeps focus (the default-open contract).
#[test]
fn the_calendar_toggle_keeps_the_armed_editor_and_refocuses_the_field() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextItem);
            harness.press(ActionId::NavNextView);
            harness.press(ActionId::NavNextView);
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().date_picker_open);
            // Close the picker again: the toggle's close arm sets the
            // latch too (the button's grab must not settle here).
            harness.handle.window().invoke_task_date_picker_toggled();
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_task_model().date_picker_open,
                "the toggle's close arm closes the picker"
            );

            // Type a parseable buffer, then toggle the calendar open:
            // the flow survives the button's click grab, the buffer is
            // untouched, and the field keeps focus.
            harness.clear_seeded();
            harness.type_text("2030-03-01");
            harness.driver.click_by_label("Calendar");
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            let model = harness.handle.window().get_task_model();
            assert!(
                model.date_picker_open,
                "the toggle opened the picker over the armed editor"
            );
            assert!(model.due_open, "the armed editor survived the toggle");
            assert_eq!(
                model.due_text, "2030-03-01",
                "the toggle never settled the typed buffer"
            );
            assert!(
                model.date_field_focus,
                "the field keeps focus on the default open"
            );
        },
    );
}

/// Round-5 F14: with no live boards, the settings picker renders the
/// visible empty state — the field is non-interactive (a click opens
/// nothing, the lane stays empty) and the consumer's caption renders.
#[test]
fn the_empty_board_list_renders_the_disabled_picker_with_its_caption() {
    let state = AppState::default();
    // A connector without boards: the picker's rows project the
    // state's live boards — none exist.
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::AppSettings);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            let copy = window.global::<taskboard_ui_slint::SettingsBoardCopy>();
            assert!(
                options(&harness).is_empty(),
                "the zero-board state projects no rows"
            );

            // The field click opens nothing (the disabled state).
            harness.driver.click_by_label(&copy.get_label());
            harness.driver.advance_time(0);
            let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
            assert!(
                !facts.get_open(),
                "the empty picker's field click opens nothing"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "the empty state dispatched nothing"
            );

            // The caption renders (the address lookup — the copy
            // constant is the element's a11y label, never asserted).
            assert!(
                harness
                    .driver
                    .find_by_label(&copy.get_empty_hint())
                    .is_some(),
                "the empty state's caption renders"
            );
        },
    );
}

/// Round-5 D6's per-site matrix, dropdown site 0 (the settings board
/// picker): the flyout mounts window-level below the anchor, the
/// ladder still moves through the decline arm, and an outside click
/// dismisses only the flyout.
#[test]
fn the_settings_picker_flyout_mounts_below_its_anchor_and_dismisses() {
    let mut state = AppState::default();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    state_board(&mut state, home, "home", None);
    state_board(&mut state, away, "away", Some("work"));
    let settings = UiSettings {
        connectors: vec![connector_row("work")],
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.driver.set_window_size(1000, 900);
            harness.press(ActionId::AppSettings);
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            // Open via the field click — the proven coordinate seam
            // (the a11y default action alone is racy right after the
            // view swap).
            click_selector_field(&mut harness.driver);
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            {
                let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
                assert!(facts.get_open(), "the field click opens the flyout");
                assert!(
                    facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                    "the flyout mounts below its anchor"
                );
            }

            // The ladder still moves (the decline arm hands the stroke
            // to the field; the lane stays empty).
            let anchor = board_facts(&harness).get_highlight();
            raw_key(&harness, slint::platform::Key::DownArrow);
            harness.driver.advance_time(0);
            assert_eq!(
                board_facts(&harness).get_highlight(),
                anchor + 1,
                "Down moves the ladder"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "the arrow dispatched nothing"
            );

            // Outside click: a real coordinate click onto another
            // control's spot — the window-level underlay receives it
            // and dismisses only the flyout (the Detach button's
            // enabled gate stays untouched).
            let detach = harness
                .driver
                .find_by_label("Detach")
                .expect("the Detach button renders");
            let (x, y) = support::driver::centre(&detach);
            harness.driver.click_at(x, y);
            harness.driver.advance_time(0);
            assert!(
                !window
                    .global::<taskboard_ui_slint::DropdownFacts>()
                    .get_open(),
                "the outside click dismissed the flyout"
            );
        },
    );
}

/// Round-5 D6's per-site matrix, dropdown site 1 (the board-bar
/// switcher): same grammar over the bar's dropdown mode.
#[test]
fn the_board_bar_flyout_mounts_below_its_anchor_and_dismisses() {
    let mut state = AppState::default();
    let boards: Vec<_> = (0..9)
        .map(|i| (uuid::Uuid::new_v4(), format!("board-{i}")))
        .collect();
    for (id, title) in &boards {
        state_board(&mut state, *id, title, None);
    }
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            harness.driver.advance_time(0);

            let mut field = None;
            for _ in 0..20 {
                harness.driver.advance_time(0);
                let found = harness.driver.find_all_by_id("BoardBar::bar-dd");
                if let Some(first) = found.into_iter().next() {
                    field = Some(first);
                    break;
                }
            }
            let field = field.expect("the bar dropdown renders past the budget");
            let pos = field.absolute_position();
            let size = field.size();
            let (x, y) = (pos.x + size.width / 2.0, pos.y + size.height / 2.0);
            harness.driver.click_at(x, y);
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);

            let window = harness.handle.window();
            {
                let facts = window.global::<taskboard_ui_slint::DropdownFacts>();
                assert!(facts.get_open(), "the field click opens the flyout");
                assert!(
                    facts.get_panel_y() >= facts.get_anchor_y() + facts.get_anchor_height(),
                    "the flyout mounts below its anchor"
                );
            }

            // Outside click onto the status bar's spot dismisses only
            // the flyout.
            let stop = harness
                .driver
                .find_by_label("Boards")
                .expect("the bar's a11y container renders");
            let (bx, by) = support::driver::centre(&stop);
            harness.driver.click_at(bx, by + 200.0);
            harness.driver.advance_time(0);
            assert!(
                !window
                    .global::<taskboard_ui_slint::DropdownFacts>()
                    .get_open(),
                "the outside click dismissed the flyout"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "the dismissal selected nothing"
            );
        },
    );
}

/// Round-5 D6's per-site matrix, time-field site 1 (the timer
/// setter's combobox): the quick list opens under its field, the
/// setter stays open, and an outside click dismisses only the list.
#[test]
fn the_setter_quick_list_takes_the_flip_leg_and_dismisses() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.set_window_size(1000, 900);
            // The setter's header button lives in the today view's
            // section (the harness's mount escape routes to Board).
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            open_setter(&mut harness);
            assert!(harness.handle.window().get_timer_setter_open());

            harness
                .driver
                .find_by_label("Quick times")
                .expect("the setter's chevron renders")
                .invoke_default_action();
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);

            let window = harness.handle.window();
            {
                let facts = window.global::<taskboard_ui_slint::TimeFieldFacts>();
                assert!(facts.get_open(), "the chevron opened the quick list");
                assert_eq!(facts.get_site(), 1, "the setter's site discriminates");
                // The setter's combobox sits low in the popover over a
                // 900px window, so the list takes the clamp/flip
                // grammar's flip leg: above the anchor, inside the
                // window.
                assert!(
                    facts.get_panel_y() <= facts.get_anchor_y(),
                    "the low anchor flips the panel above (py={:?} ay={:?} ah={:?})",
                    facts.get_panel_y(),
                    facts.get_anchor_y(),
                    facts.get_anchor_height()
                );
                assert!(facts.get_panel_y() >= 0.0, "the panel stays in the window");
            }
            assert!(
                window.get_timer_setter_open(),
                "the setter stays open under the quick list"
            );
            let slots = window.get_timer_setter_time_slots();
            assert_eq!(
                slots.row_count(),
                96,
                "the open seeds the full quarter-hour grid"
            );

            // Outside click at the window corner dismisses only the
            // quick list — the popover stays.
            harness.driver.click_at(2.0, 2.0);
            harness.driver.advance_time(0);
            assert!(
                !window
                    .global::<taskboard_ui_slint::TimeFieldFacts>()
                    .get_open(),
                "the outside click dismissed the quick list"
            );
            assert!(
                window.get_timer_setter_open(),
                "the setter survived the quick list's dismissal"
            );
        },
    );
}

/// Round-5 D6's per-site matrix, time-field sites 2 and 3 (the
/// track-form's start and end fields — side-by-side in one row, so
/// they share an anchor y and differ in x): each field opens its own
/// full-grid list keyed on the site discriminator, each panel mounts
/// under its own anchor's x, and the dismissal leaves the form open.
/// (The clamp/flip leg is journeyed at the timer-setter site — the
/// track form's row layout cannot produce a low anchor.)
#[test]
fn the_track_form_quick_lists_open_per_field_and_dismiss() {
    let (_fx, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::NavNextItem,
            );
            press_preset(
                &mut harness,
                taskboard_ui_slint::keymap::Preset::Standard,
                ActionId::TaskEdit,
            );
            harness.driver.click_by_label("Time entries");
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_task_model().track_entries_open);
            harness.handle.window().invoke_track_form_add_requested();
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_track_form_open());

            let open_field = |harness: &Harness, index: usize| {
                let chevrons = harness.driver.find_all_by_label("Quick times");
                assert!(
                    chevrons.len() >= 2,
                    "the track form mounts both time fields"
                );
                chevrons
                    .into_iter()
                    .nth(index)
                    .expect("the field's chevron renders")
                    .invoke_default_action();
                // The facts read happens at the call site (the global
                // handle borrows the window).
            };

            // The start field (site 2): opens its own list under the
            // below leg (the upper field's panel fits between the
            // anchor and the status bar), seeded unfiltered.
            open_field(&harness, 0);
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            {
                let (open, site, panel_x, panel_y, anchor_x, anchor_y, anchor_h) =
                    quick_list_facts(harness.handle.window());
                assert!(open, "the start chevron opened the list");
                assert_eq!(site, 2, "the start field discriminates");
                assert!(
                    panel_y >= anchor_y + anchor_h,
                    "the start field's panel takes the below leg (panel-y={panel_y:?} anchor-y={anchor_y:?})",
                );
                assert!(
                    (panel_x - anchor_x).abs() < 1.0,
                    "the start panel aligns with its own anchor (panel-x={panel_x:?} anchor-x={anchor_x:?})",
                );
            }
            assert_eq!(
                harness
                    .handle
                    .window()
                    .get_track_form_start_slots()
                    .row_count(),
                96,
                "the start list seeds the full grid"
            );
            harness.driver.click_at(2.0, 2.0);
            harness.driver.advance_time(0);
            assert!(
                !quick_list_facts(harness.handle.window()).0,
                "the outside click dismissed the start list"
            );
            assert!(
                harness.handle.window().get_track_form_open(),
                "the form survived the dismissal"
            );

            // The end field (site 3): the same grammar, its own list —
            // same anchor y, a different anchor x, so the panel's
            // alignment proves the mirror follows this field's anchor.
            open_field(&harness, 1);
            harness.driver.advance_time(0);
            harness.driver.advance_time(0);
            {
                let (open, site, panel_x, panel_y, anchor_x, anchor_y, anchor_h) =
                    quick_list_facts(harness.handle.window());
                assert!(open, "the end chevron opened the list");
                assert_eq!(site, 3, "the end field discriminates");
                assert!(
                    (panel_x - anchor_x).abs() < 1.0,
                    "the end panel aligns with its own anchor (panel-x={panel_x:?} anchor-x={anchor_x:?})",
                );
                assert!(
                    panel_y >= anchor_y + anchor_h,
                    "the end field's panel takes the below leg (panel-y={panel_y:?} anchor-y={anchor_y:?})",
                );
            }
            assert_eq!(
                harness
                    .handle
                    .window()
                    .get_track_form_end_slots()
                    .row_count(),
                96,
                "the end list seeds the full grid"
            );
        },
    );
}

// ---- round 5 chunk 4 (F19): the segmented and slider keyboard paths ------

/// Tabs (raw) until the stop labelled `label` holds focus. The bound
/// is structural (the settings stop count with headroom).
fn tab_to(harness: &mut Harness, label: &str) {
    for _ in 0..200 {
        if harness.driver.focused_label().as_deref() == Some(label) {
            return;
        }
        harness.stroke(parse("Tab").expect("tab"));
        harness.driver.advance_time(0);
    }
    panic!("the walk never reached the stop {label:?}");
}

/// F19a — a focused segment activates on bare Return and Space, once
/// per stroke, through the same `picked` seam pointer and AT use; a
/// disabled segment swallows the stroke.
#[test]
fn segments_activate_on_return_and_space() {
    use taskboard_ui_slint::keymap::Preset;
    use taskboard_ui_slint::requests::UiRequest;
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            harness.driver.set_window_size(1000, 6000);

            tab_to(&mut harness, "Standalone");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            assert!(
                harness.drain_requests().is_empty(),
                "a disabled segment swallows the pick"
            );

            tab_to(&mut harness, "Emacs");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            let requests = harness.drain_requests();
            assert!(
                matches!(
                    requests.as_slice(),
                    [UiRequest::SetKeymapPreset {
                        preset: Preset::Emacs,
                        ..
                    }]
                ),
                "Return picks the focused segment exactly once: {requests:?}"
            );
            // Settle the busy machine: a dropped reply ends the op.
            drop(requests);
            harness.handle.drain_setting_replies();

            tab_to(&mut harness, "Vi");
            harness.stroke(parse("Space").expect("Space"));
            harness.driver.advance_time(0);
            let requests = harness.drain_requests();
            assert!(
                matches!(
                    requests.as_slice(),
                    [UiRequest::SetKeymapPreset {
                        preset: Preset::Vi,
                        ..
                    }]
                ),
                "Space picks the focused segment exactly once: {requests:?}"
            );
        },
    );
}

/// F19b — the slider steps from the keyboard with the r4 D38
/// ceremony: strokes preview widget-internally (no request, no theme
/// move), the release commits once, a stray release commits nothing,
/// and Tab-ing away mid-gesture commits once.
#[test]
fn the_slider_steps_from_the_keyboard_and_commits_on_release() {
    use taskboard_ui_slint::requests::UiRequest;
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            harness.driver.set_window_size(1000, 6000);
            let scale = |harness: &Harness| {
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::Theme>()
                    .get_base_scale()
            };
            let committed = |requests: &[UiRequest]| -> Vec<f64> {
                requests
                    .iter()
                    .filter_map(|request| match request {
                        UiRequest::SetScale { scale, .. } => Some(*scale),
                        _ => None,
                    })
                    .collect()
            };
            assert_eq!(scale(&harness), 1.0);

            tab_to(&mut harness, "Scale slider");
            let right = parse("Right").expect("Right");
            for _ in 0..2 {
                harness.stroke(right);
                harness.driver.advance_time(0);
            }
            assert!(
                harness.drain_requests().is_empty(),
                "no request mid-gesture"
            );
            assert_eq!(scale(&harness), 1.0, "the theme holds until release");

            harness.driver.release_stroke(right);
            harness.driver.advance_time(0);
            let requests = harness.drain_requests();
            let scales = committed(&requests);
            assert_eq!(scales.len(), 1, "one commit per gesture: {requests:?}");
            assert!((scales[0] - 1.2).abs() < 1e-3, "two ticks up");
            for request in requests {
                if let UiRequest::SetScale { reply, .. } = request {
                    reply.send(Ok(())).expect("the reply lane is alive");
                }
            }
            harness.handle.drain_setting_replies();

            harness.driver.release_stroke(right);
            harness.driver.advance_time(0);
            assert!(
                committed(&harness.drain_requests()).is_empty(),
                "a release without a stroke commits nothing"
            );

            harness.stroke(parse("Left").expect("Left"));
            harness.driver.advance_time(0);
            harness.stroke(parse("Tab").expect("Tab"));
            harness.driver.advance_time(0);
            let requests = harness.drain_requests();
            let scales = committed(&requests);
            assert_eq!(scales.len(), 1, "focus loss commits the pending gesture");
            assert!((scales[0] - 1.1).abs() < 1e-3, "one tick down from 1.2");
            for request in requests {
                if let UiRequest::SetScale { reply, .. } = request {
                    reply.send(Ok(())).expect("the reply lane is alive");
                }
            }
            harness.handle.drain_setting_replies();

            // A refused commit reverts the thumb; the next gesture
            // steps from the reverted value, not the refused one.
            tab_to(&mut harness, "Scale slider");
            harness.stroke(right);
            harness.driver.advance_time(0);
            harness.driver.release_stroke(right);
            harness.driver.advance_time(0);
            for request in harness.drain_requests() {
                if let UiRequest::SetScale { reply, .. } = request {
                    reply
                        .send(Err(
                            taskboard_ui_slint::requests::SettingsError::StoreFailed,
                        ))
                        .expect("the reply lane is alive");
                }
            }
            harness.handle.drain_setting_replies();
            harness.driver.advance_time(0);
            let left = parse("Left").expect("Left");
            harness.stroke(left);
            harness.driver.advance_time(0);
            harness.driver.release_stroke(left);
            harness.driver.advance_time(0);
            let scales = committed(&harness.drain_requests());
            assert_eq!(scales.len(), 1, "one commit per gesture");
            assert!(
                (scales[0] - 1.0).abs() < 1e-3,
                "one tick down from the reverted 1.1, got {}",
                scales[0]
            );
        },
    );
}

/// The ring-coloured pixel count inside `rect` (a channel delta < 24
/// from `Semantic.focus-ring` counts as ring ink).
fn ring_ink(harness: &mut Harness, rect: (f32, f32, f32, f32)) -> u32 {
    let ring = harness
        .handle
        .window()
        .global::<taskboard_ui_slint::Semantic>()
        .get_focus_ring();
    let image = harness.driver.snapshot();
    let (x, y, width, height) = rect;
    let mut count = 0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // UI px
    for row in (y as u32)..((y + height) as u32).min(image.height) {
        for col in (x as u32)..((x + width) as u32).min(image.width) {
            let i = (row as usize * image.width as usize + col as usize) * 4;
            let near = [ring.red(), ring.green(), ring.blue()]
                .iter()
                .zip(&image.data[i..i + 3])
                .all(|(a, b)| a.abs_diff(*b) < 24);
            if near {
                count += 1;
            }
        }
    }
    count
}

/// F19 "visibly focused" — a focused segment, the slider, and Save
/// each carry ring ink, and strictly more than the same rect once the
/// walk moves on (a floor-vs-floor comparison, never a ceiling).
#[test]
fn focused_settings_controls_render_the_ring() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            harness.driver.set_window_size(1000, 6000);
            harness.driver.advance_time(0);
            for label in ["Emacs", "Scale slider", "Save"] {
                tab_to(&mut harness, label);
                let rect = harness.driver.focused_rect().expect("a stop holds focus");
                let focused = ring_ink(&mut harness, rect);
                harness.stroke(parse("Tab").expect("tab"));
                harness.driver.advance_time(0);
                let unfocused = ring_ink(&mut harness, rect);
                assert!(
                    focused >= 40 && focused > unfocused,
                    "{label}: focused ring ink {focused} vs unfocused {unfocused}"
                );
            }
        },
    );
}

// ---- #638: the review walk's journeys (the plan §4 catalogue) ----------

use taskboard_domain::{Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, Task};

/// The walk fixture: the golden state plus the global Inbox board
/// (P1's capture home) with two triage rows, and task0 scheduled
/// inside yesterday's window (a carry row). Returns the fixture ids
/// and the state.
#[allow(clippy::too_many_lines)] // one fixture builder, kept flat
fn walk_fixture_state() -> (fixture::Fixture, AppState, StackId, Vec<TaskId>) {
    let (fixture, mut state) = fixture::golden_state();
    // The golden state pins the review card away (wall-clock text in
    // goldens is the forbidden class); the walk journeys need the due
    // state — clear the pin.
    state.reflection.entries.clear();
    let inbox = BoardId::from_uuid(uuid::Uuid::new_v4());
    state.boards.insert(
        inbox,
        Board {
            id: inbox,
            remote: None,
            title: "Inbox".to_owned(),
            color: Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            remote_seen: None,
        },
    );
    let inbox_stack = StackId::from_uuid(uuid::Uuid::new_v4());
    state.stacks.insert(
        inbox_stack,
        Stack {
            id: inbox_stack,
            remote: None,
            board: inbox,
            title: "Inbox".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: BoardClocks::epoch(),
                order: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            remote_seen: None,
        },
    );
    let mk = |title: &str, scheduled: Option<chrono::DateTime<chrono::Utc>>| {
        let mut task = Task {
            id: TaskId::from_uuid(uuid::Uuid::now_v7()),
            remote: None,
            label_memberships: std::collections::BTreeMap::new(),
            title: title.to_owned(),
            description: String::new(),
            duedate: None,
            scheduled,
            done: None,
            stack: inbox_stack,
            order: 0,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::TaskClocks {
                title: BoardClocks::epoch(),
                description: BoardClocks::epoch(),
                duedate: BoardClocks::epoch(),
                scheduled: BoardClocks::epoch(),
                done: BoardClocks::epoch(),
                position: BoardClocks::epoch(),
                effort: BoardClocks::epoch(),
                importance: BoardClocks::epoch(),
                urgency: BoardClocks::epoch(),
                recur: BoardClocks::epoch(),
                kind: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            remote_seen: None,
            remote_baseline: None,
        };
        if let Some(instant) = scheduled {
            task.scheduled = Some(instant);
        }
        (task.id, task)
    };
    let mut inbox_ids = Vec::new();
    for title in ["triage-me", "triage-me-too"] {
        let (id, task) = mk(title, None);
        state.tasks.insert(id, task);
        inbox_ids.push(id);
    }
    // task0 carries a scheduled anchor inside yesterday's window (the
    // midnight-aligned convention: the journey harness's wall-clock
    // anchor reads the default day-start pair).
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let now = chrono::Utc::now();
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let carry = calendar.instant(
        yesterday,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let mut task0 = state.tasks.get(&fixture.task0).expect("task0").clone();
    task0.scheduled = Some(carry);
    state.tasks.insert(fixture.task0, task0);
    (fixture, state, inbox_stack, inbox_ids)
}

/// Opens the walk over the fixture (the card click — J1's route).
fn open_walk(harness: &mut Harness) {
    // The card lives on the Today view — mount it first (a no-op when
    // the walk is opened from there).
    harness.press(ActionId::ViewToday);
    harness.driver.advance_time(0);
    assert!(
        harness.handle.window().get_review_card_visible(),
        "the surfacing card is visible"
    );
    let label = format!(
        "Review {}",
        chrono::Utc::now()
            .date_naive()
            .pred_opt()
            .expect("yesterday")
    );
    // The a11y tree exposes the freshly mounted card on a later
    // tick (the view-mount settle; the planner pool row's retry).
    let mut card = None;
    for _ in 0..20 {
        harness.driver.advance_time(0);
        if let Some(found) = harness.driver.find_by_label(&label) {
            card = Some(found);
            break;
        }
    }
    let card = card.unwrap_or_else(|| panic!("the surfacing card renders"));
    card.invoke_default_action();
    harness.driver.advance_time(0);
}

fn walk_open(harness: &Harness) -> bool {
    harness.handle.window().get_walk_model().open
}

fn walk_stage(harness: &Harness) -> i32 {
    harness.handle.window().get_walk_model().stage_kind
}

fn walk_rows(harness: &Harness) -> usize {
    harness.handle.window().get_walk_model().rows.row_count()
}

/// J1 — unreviewed closed day → the Today card; the click opens the
/// walk at the Inbox stage over the due window's reference.
#[test]
fn the_card_opens_the_walk_over_the_due_window() {
    let (_fixture, state, _stack, inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_review_card_visible(),
                "an unreviewed closed day surfaces the card"
            );
            open_walk(&mut harness);
            assert!(walk_open(&harness), "the card opened the walk");
            assert_eq!(walk_stage(&harness), 0, "the walk opens at the Inbox");
            assert_eq!(
                walk_rows(&harness),
                inbox_ids.len(),
                "the triage rows are the inbox reading"
            );
            assert_eq!(
                harness.engine.drain(),
                Vec::new(),
                "opening dispatches nothing"
            );
        },
    );
}

/// J2 — a live Day entry for the closed day clears the card.
#[test]
fn a_recorded_day_entry_clears_the_card() {
    let (fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let entry = taskboard_domain::ReflectionEntry {
        id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: taskboard_domain::ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: "done".to_owned(),
        attributes: taskboard_domain::ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry.clone());
    let _ = fixture;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_review_card_visible(),
                "a live entry for the window clears the card"
            );
        },
    );
}

/// J3 — the boundary crossing rides the render clock seam (the
/// `UiSettings::clock` freeze): with a live entry for yesterday the
/// frozen clock just before the next boundary shows no card; frozen
/// past it, the card appears — with NO command dispatched and no
/// request sent (D-W11's no-machinery pin).
#[test]
fn the_card_appears_at_the_boundary_without_dispatching() {
    for (past_boundary, expect_card) in [(false, false), (true, true)] {
        let (_fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
        let t0 = chrono::Utc::now();
        let day_start = (
            taskboard_domain::SettingsSnapshot::default().day_start_time,
            UiSettings::default().boot_offset,
        );
        let calendar = taskboard_domain::DayCalendar {
            day_start_time: day_start.0,
            offset: day_start.1,
        };
        let yesterday = calendar.today(t0).pred_opt().expect("yesterday");
        let window = taskboard_domain::review_window(
            taskboard_domain::ReviewCadence::Day,
            yesterday,
            calendar,
        );
        let entry = taskboard_domain::ReflectionEntry {
            id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
            cadence: taskboard_domain::ReviewCadence::Day,
            day: window.day(),
            window_start: window.start(),
            window_end: window.end(),
            text: "done".to_owned(),
            attributes: taskboard_domain::ReflectionAttributes::default(),
            created_at: t0,
            updated_at: t0,
            deleted: false,
        };
        state.reflection.entries.insert(entry.id, entry);
        // The boundary: a full day past the fixture's `now` — the
        // latest closed window becomes the day the fixture clock
        // called "today", which carries no entry.
        let settings = UiSettings {
            clock: past_boundary.then(|| t0 + TimeDelta::try_hours(24).expect("day")),
            ..UiSettings::default()
        };
        support::driver::run(
            DriverOptions {
                headless_renderer: true,
            },
            move |_driver| {
                let mut harness =
                    Harness::mount_with_settings(SlintDriver::default(), state, settings);
                harness.press(ActionId::ViewToday);
                harness.driver.advance_time(0);
                assert_eq!(
                    harness.handle.window().get_review_card_visible(),
                    expect_card,
                    "the card follows the frozen clock across the boundary"
                );
                assert!(
                    harness.engine.drain().is_empty(),
                    "the boundary crossing dispatched nothing"
                );
                assert!(harness.drain_requests().is_empty(), "no request rode it");
            },
        );
    }
}

/// J4 — `review.open` with no closed gap: the walk opens over the
/// CURRENT day window (the evening review before the boundary).
#[test]
fn the_opener_falls_back_to_the_current_day() {
    let (fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let entry = taskboard_domain::ReflectionEntry {
        id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
        cadence: taskboard_domain::ReviewCadence::Day,
        day: window.day(),
        window_start: window.start(),
        window_end: window.end(),
        text: String::new(),
        attributes: taskboard_domain::ReflectionAttributes::default(),
        created_at: now,
        updated_at: now,
        deleted: false,
    };
    state.reflection.entries.insert(entry.id, entry);
    let _ = fixture;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            assert!(walk_open(&harness));
            let header = harness.handle.window().get_walk_model().header.to_string();
            let today = calendar.today(now);
            assert!(
                header.contains(&today.to_string()),
                "the header names the current day window ({header})"
            );
        },
    );
}

/// J5 — File to… → pick a stack → commit: exactly one `MoveTask` with
/// the end-of-stack order; the row leaves at the next refresh.
#[test]
fn filing_commits_one_move_task() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let stack_a = fixture.stack_a;
    let expected_order = state
        .tasks
        .values()
        .filter(|t| t.stack == stack_a && !t.deleted)
        .map(|t| t.order)
        .max()
        .unwrap_or(0)
        + 1;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            open_walk(&mut harness);
            harness.handle.dispatch_action(ActionId::ReviewInboxFile);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_walk_model().filing,
                "File to… armed the picker"
            );
            // The first candidate is stack "a" (the fixture board's
            // first): pick it via the pointer seam and commit.
            harness.handle.window().invoke_walk_candidate_activated(0);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewFileCommit);
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            match drained.as_slice() {
                [StateCommand::MoveTask { id, stack, order }] => {
                    assert_eq!(*stack, stack_a, "the picked candidate");
                    assert_eq!(*order, expected_order, "the end-of-stack append");
                    assert_ne!(*id, TaskId::from_uuid(uuid::Uuid::nil()));
                }
                other => panic!("one MoveTask, got {other:?}"),
            }
            // The fake engine folds it in: the row leaves the inbox.
            harness.engine.settle(&harness.handle);
            assert_eq!(walk_rows(&harness), 1, "the filed row left the reading");
        },
    );
}

/// J6 — Keep in inbox on the last row: no command, the stage passes
/// to Carry forward.
#[test]
fn keep_on_the_last_row_passes_the_stage() {
    let (_fixture, state, _stack, inbox_ids) = walk_fixture_state();
    assert_eq!(inbox_ids.len(), 2, "the fixture's two triage rows");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            open_walk(&mut harness);
            // Step to the last row, then keep: the stage passes.
            harness.stroke(parse("Down").expect("Down"));
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
            assert!(harness.engine.drain().is_empty(), "keep dispatches nothing");
            assert_eq!(walk_stage(&harness), 1, "the stage passed to Carry forward");
        },
    );
}

/// J7 — an empty inbox opens the walk at Carry forward, the strip
/// carrying the Inbox empty flag (the b-line is the pushed note).
#[test]
fn an_empty_inbox_opens_at_carry_forward() {
    let (_fixture, mut state, _stack, _inbox_ids) = walk_fixture_state();
    let inbox_board = state
        .boards
        .values()
        .find(|board| board.title == "Inbox")
        .expect("the inbox board")
        .id;
    let dead: Vec<_> = state
        .tasks
        .values()
        .filter(|task| {
            state
                .stacks
                .get(&task.stack)
                .is_some_and(|stack| stack.board == inbox_board)
        })
        .map(|task| task.id)
        .collect();
    for id in dead {
        state.tasks.get_mut(&id).expect("live").deleted = true;
    }
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            assert!(walk_open(&harness));
            assert_eq!(walk_stage(&harness), 1, "the empty inbox is skipped");
            let strip = harness.handle.window().get_walk_model().strip;
            let inbox_note = (0..strip.row_count())
                .find_map(|i| strip.row_data(i))
                .expect("the inbox strip row");
            assert!(
                !inbox_note.note.is_empty(),
                "the skipped stage's b-line rides the strip"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the skip dispatched nothing"
            );
        },
    );
}

/// J8 — Move to tomorrow: one `UpdateTask` to the next window's
/// opening instant; the due date is untouched.
#[test]
fn carry_tomorrow_moves_to_the_next_window_start() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let due_before = state.tasks.get(&task0).expect("task0").duedate;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            open_walk(&mut harness);
            // Pass the inbox (keep both rows), land on Carry forward.
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&harness), 1);
            harness
                .handle
                .dispatch_action(ActionId::ReviewCarryTomorrow);
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            let day_start = (
                taskboard_domain::SettingsSnapshot::default().day_start_time,
                UiSettings::default().boot_offset,
            );
            let calendar = taskboard_domain::DayCalendar {
                day_start_time: day_start.0,
                offset: day_start.1,
            };
            let now = chrono::Utc::now();
            let today = calendar.today(now);
            let next = taskboard_domain::review_window(
                taskboard_domain::ReviewCadence::Day,
                today,
                calendar,
            );
            match drained.as_slice() {
                [StateCommand::UpdateTask { id, changes }] => {
                    assert_eq!(*id, task0, "the cursor row");
                    assert_eq!(
                        changes.scheduled,
                        Some(Some(next.start())),
                        "the next window's opening instant"
                    );
                    assert_eq!(changes.duedate, None, "due dates are never touched");
                }
                other => panic!("one carry write, got {other:?}"),
            }
            // The row left the carry reading (its anchor moved out of
            // the window).
            harness.engine.settle(&harness.handle);
            assert_eq!(walk_rows(&harness), 0, "the carried row left");
        },
    );
    let _ = due_before;
}

/// J9 — Pick a date: a parseable date commits one scheduled edit; an
/// unparseable one dispatches nothing and keeps the text.
#[test]
fn the_date_pick_commits_parseable_input_only() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            open_walk(&mut harness);
            for _ in 0..2 {
                harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
                harness.driver.advance_time(0);
            }
            harness
                .handle
                .dispatch_action(ActionId::ReviewCarryPickDate);
            harness.driver.advance_time(0);
            // Unparseable: nothing dispatches, the buffer is kept.
            harness
                .handle
                .window()
                .invoke_walk_date_edited("not a date".into());
            harness
                .handle
                .dispatch_action(ActionId::ReviewCarryDateCommit);
            harness.driver.advance_time(0);
            assert!(
                harness.engine.drain().is_empty(),
                "an unparseable date dispatches nothing"
            );
            // Parseable: one scheduled write.
            harness
                .handle
                .window()
                .invoke_walk_date_edited("2030-03-01".into());
            harness
                .handle
                .dispatch_action(ActionId::ReviewCarryDateCommit);
            harness.driver.advance_time(0);
            let day_start = (
                taskboard_domain::SettingsSnapshot::default().day_start_time,
                UiSettings::default().boot_offset,
            );
            let calendar = taskboard_domain::DayCalendar {
                day_start_time: day_start.0,
                offset: day_start.1,
            };
            let expected = calendar.instant(
                chrono::NaiveDate::from_ymd_opt(2030, 3, 1).expect("date"),
                chrono::NaiveTime::MIN,
            );
            match harness.engine.drain().as_slice() {
                [StateCommand::UpdateTask { id, changes }] => {
                    assert_eq!(*id, task0);
                    assert_eq!(changes.scheduled, Some(Some(expected)));
                }
                other => panic!("one dated write, got {other:?}"),
            }
        },
    );
}

/// J10 — advancing out of Carry forward lands the walk on the
/// planner: flow `None`, view `PlanNextDay`, the walk's return view
/// snapshot untouched (the planner's own return seam).
#[test]
fn the_advance_hands_off_to_the_planner() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            // Advance through Inbox (empty via keeps? the fixture has
            // rows — advance twice: once past Inbox, once out of
            // Carry forward).
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&harness), 1);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness), "the walk closed at the handoff");
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the planner is mounted"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the handoff dispatched no command"
            );
        },
    );
}

/// J11 — closing the planner without placing anything is the built-in
/// skip: the walk resumes at Prompts (P12 — nothing forced, nothing
/// lost).
#[test]
fn closing_the_planner_resumes_at_prompts() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            // The planner's Escape rung (its own context binding).
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(walk_open(&harness), "the walk resumed");
            assert_eq!(walk_stage(&harness), 2, "at Prompts");
            assert!(
                harness.engine.drain().is_empty(),
                "the resume dispatched nothing"
            );
        },
    );
}

/// J13 — leaving the planner by another route (`view.today`) drops
/// the hint: no walk resumes, and a later planner close resumes
/// nothing (the walk never ambushes later).
#[test]
fn leaving_the_planner_by_another_route_drops_the_hint() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            // Out by the view intent, not the planner's close.
            harness.handle.dispatch_action(ActionId::ViewToday);
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness), "no walk ambushes the Today view");
            // A later planner open + close resumes nothing.
            harness.handle.dispatch_action(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness), "the hint is gone");
        },
    );
}

/// J14 — Record with two answers: exactly one `RecordReflection` with
/// the reference's cadence/day/window; after the entry lands in the
/// published state the card is gone.
#[test]
fn recording_routes_one_entry_and_clears_the_card() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let _ = task0;
    let day_start = (
        taskboard_domain::SettingsSnapshot::default().day_start_time,
        UiSettings::default().boot_offset,
    );
    let calendar = taskboard_domain::DayCalendar {
        day_start_time: day_start.0,
        offset: day_start.1,
    };
    let now = chrono::Utc::now();
    let yesterday = calendar.today(now).pred_opt().expect("yesterday");
    let expected_window =
        taskboard_domain::review_window(taskboard_domain::ReviewCadence::Day, yesterday, calendar);
    let published = {
        let mut state = state.clone();
        let entry = taskboard_domain::ReflectionEntry {
            id: taskboard_domain::ReflectionId::from_uuid(uuid::Uuid::now_v7()),
            cadence: taskboard_domain::ReviewCadence::Day,
            day: expected_window.day(),
            window_start: expected_window.start(),
            window_end: expected_window.end(),
            text: "shipped the walk".to_owned(),
            attributes: taskboard_domain::ReflectionAttributes::default(),
            created_at: now,
            updated_at: now,
            deleted: false,
        };
        state.reflection.entries.insert(entry.id, entry);
        state
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            open_walk(&mut harness);
            // Straight to the prompts: the walk advance runs the
            // handoff and the planner close resumes at Prompts.
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&harness), 2);
            // Two answers ride the mirror seam (the editors' path).
            harness
                .handle
                .window()
                .invoke_walk_answer_edited(0, "shipped the walk".into());
            harness
                .handle
                .window()
                .invoke_walk_answer_edited(3, "the planner leg".into());
            harness.handle.dispatch_action(ActionId::ReviewRecord);
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            // The advance/handoff legs dispatch nothing; the record is
            // the journey's ONE command.
            match drained.as_slice() {
                [
                    StateCommand::RecordReflection {
                        cadence,
                        day,
                        window,
                        text,
                        mood,
                        energy,
                        flags,
                    },
                ] => {
                    assert_eq!(*cadence, taskboard_domain::ReviewCadence::Day);
                    assert_eq!(*day, yesterday);
                    assert_eq!(*window, expected_window.bounds());
                    assert!(text.contains("shipped the walk"));
                    assert!(text.contains("the planner leg"));
                    assert_eq!(*mood, None);
                    assert_eq!(*energy, None);
                    assert!(flags.is_empty(), "attributes stay the CLI's (D-W9)");
                }
                other => panic!("one record, got {other:?}"),
            }
            assert!(!walk_open(&harness), "the walk closed on the record");
            harness.engine.replace_state(published, &harness.handle);
            assert!(
                !harness.handle.window().get_review_card_visible(),
                "the recorded window no longer surfaces"
            );
        },
    );
}

/// J15 — an all-blank Record and `Finish without recording` both end
/// the walk gently: no command, flow closed.
#[test]
fn blank_records_and_finish_dispatch_nothing() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            for exit in [ActionId::ReviewRecord, ActionId::ReviewFinish] {
                harness.handle.dispatch_action(ActionId::ReviewOpen);
                harness.driver.advance_time(0);
                harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
                harness.driver.advance_time(0);
                harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
                harness.driver.advance_time(0);
                harness.stroke(parse("Escape").expect("Escape"));
                harness.driver.advance_time(0);
                assert_eq!(walk_stage(&harness), 2);
                harness.handle.dispatch_action(exit);
                harness.driver.advance_time(0);
                assert!(
                    harness.engine.drain().is_empty(),
                    "{exit:?} dispatched nothing"
                );
                assert!(!walk_open(&harness), "{exit:?} closed the walk");
            }
        },
    );
}

/// J16 — the Escape ladder: an armed date field disengages first
/// (the focused field's Insert), then the sub-state clears, then the
/// stage steps back, then the walk closes — no command at any rung.
#[test]
fn the_escape_ladder_never_dispatches() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            open_walk(&mut harness);
            assert!(walk_open(&harness));
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewInboxKeep);
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&harness), 1);
            harness
                .handle
                .dispatch_action(ActionId::ReviewCarryPickDate);
            harness.driver.advance_time(0);
            assert!(harness.handle.window().get_walk_model().picking);
            // Rung 1: the focused field disengages Insert; the armed
            // field stays (the TrackEntryDismiss ladder's shape).
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_walk_model().picking,
                "the armed field stays on the mode rung"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the ladder dispatched nothing"
            );
            // Rung 2: the sub-state clears.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_walk_model().picking,
                "the armed field disengaged"
            );
            assert_eq!(walk_stage(&harness), 1, "the stage holds");
            // Rung 3: the stage steps back to the Inbox.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert_eq!(walk_stage(&harness), 0, "the stage stepped back");
            // Rung 4: the walk closes.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness));
            assert!(
                harness.engine.drain().is_empty(),
                "no ladder rung dispatched"
            );
        },
    );
}

/// J17 — Capture opened mid-walk: the walk is replaced without
/// committing (the XOR rule), nothing dispatched.
#[test]
fn capture_replaces_the_walk_without_committing() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::CaptureQuick);
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness), "the walk was replaced");
            assert!(
                harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_vi_insert(),
                "capture's insert mode engages"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the replacement dispatched nothing"
            );
        },
    );
}

/// J18 — a refresh mid-walk re-derives the rows for the SAME window:
/// a concurrent engine write moves the rows, the walk stays open with
/// the reference pinned (no re-target at a state change).
#[test]
fn a_mid_walk_refresh_rederives_the_same_window() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task1 = fixture.task1;
    let published = {
        let mut updated = state.clone();
        updated.tasks.get_mut(&task1).expect("live").done = Some(chrono::Utc::now());
        updated
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            open_walk(&mut harness);
            let header_before = harness.handle.window().get_walk_model().header.to_string();
            // A concurrent engine write lands mid-walk: task1 is done.
            harness.engine.replace_state(published, &harness.handle);
            harness.driver.advance_time(0);
            assert!(walk_open(&harness), "the walk survives the refresh");
            assert_eq!(
                harness.handle.window().get_walk_model().header.to_string(),
                header_before,
                "the reference is pinned — the header cannot drift"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the refresh dispatched nothing"
            );
        },
    );
}

/// J19 — the kiosk pins (D-J14): the `review.open` dispatch is
/// structurally inert (layer 2) and neither the card nor the walk
/// model renders (layer 3) even with a due window on the wall.
#[test]
fn the_kiosk_mount_never_surfaces_the_walk() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let settings = UiSettings {
        kiosk: true,
        ..UiSettings::default()
    };
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(SlintDriver::default(), state, settings);
            harness.press(ActionId::ViewToday);
            harness.driver.advance_time(0);
            assert!(
                !harness.handle.window().get_review_card_visible(),
                "no card on the appliance (layer 3)"
            );
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            assert!(!walk_open(&harness), "the dispatch is inert (layer 2)");
            assert!(
                harness.engine.drain().is_empty(),
                "the kiosk dispatch routed nothing"
            );
            assert!(
                !harness
                    .handle
                    .window()
                    .global::<taskboard_ui_slint::KeyboardState>()
                    .get_date_picker_grid(),
                "no overlay context leaked"
            );
        },
    );
}

/// J20 — the walk opened over the planner: advancing out of Carry
/// forward keeps the planner mounted (no toggle-close — the handoff's
/// skip-leg).
#[test]
fn the_walk_over_the_planner_hands_off_without_closing_it() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            // Open the planner first, then the walk over it.
            harness.handle.dispatch_action(ActionId::ViewPlanNextDay);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            assert!(walk_open(&harness));
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay,
                "the planner stays mounted (no toggle-close)"
            );
            assert!(!walk_open(&harness), "the walk closed at the handoff");
        },
    );
}

/// J12 — the planner leg places a task, then closes: the placement's
/// `UpdateTask` lands and the walk resumes at Prompts (placed tasks
/// reach it as plain state).
#[test]
fn the_planner_leg_places_then_resumes() {
    let (fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    let task0 = fixture.task0;
    let title = state.tasks.get(&task0).expect("task0").title.clone();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
            harness.driver.advance_time(0);
            harness.engine.drain();
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::PlanNextDay
            );
            // Place task0 from the pool: the placement form's funnel.
            let row = harness
                .driver
                .find_by_label(&format!("Place {title}"))
                .unwrap_or_else(|| panic!("task0's pool row is mounted"));
            row.invoke_default_action();
            harness.driver.advance_time(0);
            click_field_centre(&mut harness, "Time");
            harness.type_text("09:30");
            harness.stroke(parse("Return").expect("Return"));
            harness.driver.advance_time(0);
            let drained = harness.engine.drain();
            assert!(
                drained
                    .iter()
                    .any(|c| matches!(c, StateCommand::UpdateTask { .. })),
                "the placement write landed"
            );
            // The planner's Escape: the walk resumes at Prompts.
            harness.stroke(parse("Escape").expect("Escape"));
            harness.driver.advance_time(0);
            assert!(walk_open(&harness));
            assert_eq!(walk_stage(&harness), 2);
        },
    );
}

/// The view-mount inventory row for the overlay (the walk's structural
/// fact: the overlay is a flow-gated window-level mount, never a view).
#[test]
fn the_walk_overlay_mount_is_flow_gated_window_level() {
    let (_fixture, state, _stack, _inbox_ids) = walk_fixture_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.handle.dispatch_action(ActionId::ReviewOpen);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            assert!(window.get_walk_model().open);
            // The overlay is NOT a view: the cycle selector stays on
            // its surface and the kiosk rotation cannot reach the walk.
            assert_ne!(window.get_current_view(), View::PlanNextDay);
        },
    );
}
