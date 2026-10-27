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

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand, TaskId};
use taskboard_ui_slint::actions::{ActionId, ContextId};
use taskboard_ui_slint::keymap::stroke::{self, parse};
use taskboard_ui_slint::{UiSettings, View};

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
            return vec![ContextId::ViInsert, ContextId::Global];
        }
        let view = match window.get_current_view() {
            View::Today => Some(ContextId::Today),
            View::Board => Some(ContextId::Board),
            View::StackDetail => Some(ContextId::StackDetail),
            View::TaskDetail => Some(ContextId::TaskDetail),
            View::Settings => Some(ContextId::Settings),
        };
        let mut chain = Vec::with_capacity(3);
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

/// §7.3 — the ten core verbs in one continuous session (one `#[test]`
/// is the contract; per-step asserts inside — the length is the
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
                View::Today,
                "the view cycle wraps through Today (chunk 10 D2)"
            );
            harness.press(ActionId::NavNextView);
            assert_eq!(harness.handle.window().get_current_view(), View::Board);
            harness.press(ActionId::NavPrevView);
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
            harness.press(ActionId::NavPrevView);
            assert_eq!(current_view(&harness), View::Today, "wrap backwards");
            harness.press(ActionId::NavPrevView);
            assert_eq!(current_view(&harness), View::TaskDetail);
            harness.press(ActionId::NavFocusBoard);

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

/// Round-1 chunk 2 (D18, T-10) — the keyboard resize journey: wider
/// steps the model's rendered width by the scaled step, narrower clamps
/// at the readable floor, and every step's full-snapshot request
/// reaches the lane typed (exactly the `StackWidthsChanged` payloads,
/// latest snapshot wins).
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

            // One full-snapshot request per key press, drained per
            // step (the lane is drop-on-full by design — the journey
            // never gives it a reason to drop).
            let expect_snapshot = |harness: &Harness, width: u32| {
                let requests = harness.drain_requests();
                assert_eq!(requests.len(), 1, "one request per key press");
                assert!(
                    matches!(
                        &requests[0],
                        taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                            board: payload_board,
                            widths,
                        } if *payload_board == board
                            && widths.as_slice() == [(fixture.stack_a, width)]
                    ),
                    "the typed snapshot carries the model's width"
                );
            };

            harness.press(ActionId::StackWider);
            assert_eq!(
                harness.column_width_px(0),
                280.0 + 32.0,
                "the first keyboard step seeds DEFAULT and adds STEP (scale 1)"
            );
            expect_snapshot(&harness, 312);

            harness.press(ActionId::StackNarrower);
            assert_eq!(harness.column_width_px(0), 280.0, "back to the seed");
            expect_snapshot(&harness, 280);

            // Narrowing clamps at the readable floor and keeps
            // reporting the clamped snapshot.
            for step in 1..=8u32 {
                harness.press(ActionId::StackNarrower);
                let expected = 280u32
                    .saturating_sub(32 * step)
                    .max(taskboard_ui_slint::widths::STACK_WIDTH_MIN_PX);
                // px values, far below f32's mantissa reach.
                #[allow(clippy::cast_precision_loss)]
                let expected_px = expected as f32;
                assert_eq!(harness.column_width_px(0), expected_px);
                expect_snapshot(&harness, expected);
            }
        },
    );
}
/// Round-1 chunk 2 (D17, T-11) — the mouse drag journey: press
/// captures the pre-drag base, drag re-renders live (clamped), a stale
/// column index is absorbed, and exactly one request lands on release.
#[test]
fn drag_seam_resizes_live_and_persists_on_release() {
    let (fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();

            window.invoke_board_stack_width_press(0, 300.0);
            window.invoke_board_stack_width_drag(0, -50.0);
            assert_eq!(
                harness.column_width_px(0),
                250.0,
                "the column re-rendered live at base + delta"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "drag ticks never persist (release only)"
            );

            // A stale index (tables moved between press and tick) is
            // absorbed silently.
            window.invoke_board_stack_width_drag(9, 500.0);
            assert_eq!(harness.column_width_px(0), 250.0, "stale tick ignored");

            window.invoke_board_stack_width_release(0);
            let requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one request on release");
            assert!(
                matches!(
                    &requests[0],
                    taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                        board: payload_board,
                        widths,
                    } if *payload_board == board
                        && widths.as_slice() == [(fixture.stack_a, 250u32)]
                ),
                "the release snapshot carries the dragged width"
            );

            // Release without a press is inert (no request).
            window.invoke_board_stack_width_release(0);
            assert!(harness.drain_requests().is_empty());
        },
    );
}

/// Round-1 chunk 2 (D16, T-12) — the auto-board guarantee: a
/// never-resized board pushes the `−1.0` sentinel on every column and
/// refreshes alone never emit a persistence request (the goldens'
/// auto-layout proof's journey half).
#[test]
fn never_resized_board_pushes_auto_widths_and_no_requests() {
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let harness = Harness::mount(SlintDriver::default(), state);
            let board_model = harness.handle.window().get_board_model();
            for column in 0..board_model.stacks.row_count() {
                assert!(
                    harness.column_width_px(column) <= 0.0,
                    "column {column} stays auto"
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

/// R2 D1 (T-4) — stored widths are floors, never pins: after dragging
/// column 0 down to its floor, a window resize still distributes the
/// row's full width (every column ≥ its stored floor, Σ columns +
/// the add-column affordance + gaps == window width within rounding).
#[test]
fn stored_widths_fill_the_window_as_floors() {
    let (_fixture, state) = fixture::journey_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            let window = harness.handle.window();
            harness.driver.set_window_size(1000, 800);

            // Drag column 0 to its readable floor (the seam's press
            // base is the rendered width; the delta clamps at min).
            window.invoke_board_stack_width_press(0, 500.0);
            window.invoke_board_stack_width_drag(0, -500.0);
            window.invoke_board_stack_width_release(0);
            assert!(
                harness.drain_requests().iter().any(|request| {
                    matches!(
                        request,
                        taskboard_ui_slint::requests::UiRequest::StackWidthsChanged { .. }
                    )
                }),
                "the drag persisted"
            );

            // A wider window fills: the stored width holds only as the
            // column's floor, the surplus redistributes.
            harness.driver.set_window_size(1500, 900);
            // The a11y items of one column (list + header button)
            // share the column's width; the titles are the addresses.
            let add_col = harness
                .driver
                .find_all_by_id("BoardLayout::add-col-hit")
                .pop()
                .expect("the add-column affordance is mounted");
            #[allow(clippy::cast_precision_loss)]
            let floor = taskboard_ui_slint::widths::STACK_WIDTH_MIN_PX as f32; // at scale 1
            let mut total = add_col.size().width;
            for title in ["a", "b"] {
                let column = harness
                    .driver
                    .find_by_label(title)
                    .unwrap_or_else(|| panic!("column {title} is addressable"));
                let width = column.size().width;
                assert!(
                    width >= floor,
                    "no column renders below its stored floor, got {width}"
                );
                total += width;
            }
            // Two inter-column gaps (space-3 = 12 px at scale 1), the
            // row's side strip padding (r2 D35, another 2 × 12), plus
            // the trailing add-column counted above; ±1 px of physical
            // rounding.
            let expected = 1500.0 - 4.0 * 12.0;
            assert!(
                (total - expected).abs() <= 1.0,
                "the columns row fills the window: {total} vs {expected}"
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

/// R2 D5 (T-6) — the add-stack plus glyph is square: the renderer's
/// bounding box of the painted glyph inside the trailing add-column
/// strip measures the space-6 square (element ids cannot address the
/// glyph directly — unnamed decoration items merge in the item tree —
/// so this pin reads the pixels, golden-adjacent).
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
                (28.0..=36.0).contains(&f64::from(width))
                    && (28.0..=36.0).contains(&f64::from(height)),
                "the glyph bounding box is a space-6 square, got {width}x{height}"
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
                        read_only: false,
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
                read_only: false,
            },
            // The D4 structural surface: a legacy-world row arrives
            // flagged, and the flag survives the push (the row renders
            // its Test/Remove controls dead — markup-owned).
            taskboard_ui_slint::requests::ConnectorInfo {
                name: "legacy".into(),
                server_url: "https://legacy.example".into(),
                username: "bob".into(),
                read_only: true,
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
            assert!(
                !rows.row_data(0).expect("row").read_only,
                "a live connector's row is mutable"
            );
            assert!(
                rows.row_data(1).expect("row").read_only,
                "the legacy row arrives flagged read-only"
            );

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
// r2 D28: the status line's text and auto-clear lifetime
// ---------------------------------------------------------------------

/// Terminal status shows mapped text and auto-clears under the mock
/// clock (r2 D28): the seam-local Rejected (a scale parse miss — no
/// lane round-trip) is visible for `STATUS_VISIBLE_MS`, then returns to
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

/// A new outcome re-arms the auto-clear (r2 D28): seven seconds after
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
                        read_only: false,
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
// r2 D29: the scale slider's gesture (press / preview / release)
// ---------------------------------------------------------------------

/// The slider previews live and commits once per gesture (r2 D29):
/// five previews move `Theme.base-scale` (the visible live feedback)
/// while the lane stays empty — no request construction exists in the
/// preview arm; the release routes exactly one `SetScale` carrying
/// the final value, whose Ok reply leaves the outcome Ok and the
/// scale persisted.
#[test]
fn the_slider_previews_live_and_commits_once_per_gesture() {
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
            for v in [1.2f32, 1.5, 1.8, 2.1, 2.4] {
                window.invoke_settings_scale_preview(v);
                assert_eq!(scale(), v, "the preview live-applies onto the theme");
            }
            assert!(
                harness.drain_requests().is_empty(),
                "the preview arm writes no lane request"
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
/// the silent pending gate absorbs press, previews, and release — no
/// `SetScale`, no scale movement, no outcome change.
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
            window.invoke_settings_scale_preview(2.4);
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
/// Err reply restores the pre-**gesture** scale (the whole preview
/// undoes), and the outcome is the typed fold of the error —
/// outcome-agnostic revert.
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
            window.invoke_settings_scale_preview(1.7);
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
            let facts = window.global::<taskboard_ui_slint::SettingsScrollFacts>();
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

            let scale_field = driver
                .find_by_label("Scale")
                .expect("the scale field is addressable");
            let pos = scale_field.absolute_position();
            let size = scale_field.size();
            assert!(
                pos.y >= 0.0 && pos.y + size.height <= 400.0 * 3.0,
                "the scale field sits inside the window after the scroll"
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
            read_only: false,
        }],
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
                4,
                "Name, Server, Username, and App password each carry a tip"
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
    let (_fixture, state) = fixture::golden_state();
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |driver: &mut SlintDriver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.press(ActionId::AppSettings);
            driver.bind(&harness.handle);
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
#[test]
fn the_rule_list_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let task1 = fixture.task1;
    // Two rules on the focused task, distinct offsets; ids ascend with
    // insertion order so the row table is deterministic.
    let rule1 = taskboard_domain::ReminderRule {
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(11)),
        task: task1,
        offset: taskboard_domain::ReminderOffset::at_due(),
        created_at: taskboard_domain::BoardClocks::epoch(),
        updated_at: taskboard_domain::BoardClocks::epoch(),
    };
    let rule2 = taskboard_domain::ReminderRule {
        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(12)),
        task: task1,
        offset: taskboard_domain::ReminderOffset::before(
            TimeDelta::try_minutes(-10).expect("-10m"),
        )
        .expect("legal offset"),
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
            }]);

            // A parse miss dispatches nothing (absorbed — D12).
            harness
                .handle
                .window()
                .invoke_rule_add_custom_accepted("soon".into());
            harness.assert_commands(&[]);

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

            window.invoke_board_stack_width_press(0, 300.0);
            window.invoke_board_stack_width_drag(0, -20.0);
            assert_eq!(harness.column_width_px(0), 280.0, "first tick");
            harness.handle.refresh();
            window.invoke_board_stack_width_drag(0, -30.0);
            assert_eq!(
                harness.column_width_px(0),
                270.0,
                "the refresh did not disturb the drag"
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
                    labels: std::collections::BTreeSet::new(),
                    effort: None,
                    importance: None,
                    urgency: None,
                    recur: None,

                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::TaskClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        description: taskboard_domain::BoardClocks::epoch(),
                        duedate: taskboard_domain::BoardClocks::epoch(),
                        scheduled: taskboard_domain::BoardClocks::epoch(),
                        done: taskboard_domain::BoardClocks::epoch(),
                        position: taskboard_domain::BoardClocks::epoch(),
                        labels: taskboard_domain::BoardClocks::epoch(),
                        effort: taskboard_domain::BoardClocks::epoch(),
                        importance: taskboard_domain::BoardClocks::epoch(),
                        urgency: taskboard_domain::BoardClocks::epoch(),
                        recur: taskboard_domain::BoardClocks::epoch(),

                        archived: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            );
            harness.engine.replace_state(changed, &harness.handle);
            window.invoke_board_stack_width_drag(0, -40.0);
            assert_eq!(
                harness.column_width_px(0),
                260.0,
                "the engine change did not disturb the drag"
            );
            assert!(
                harness.drain_requests().is_empty(),
                "drag ticks never persist (release only)"
            );

            window.invoke_board_stack_width_release(0);
            let requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one request on release");
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::StackWidthsChanged {
                    board: payload_board,
                    widths,
                } if *payload_board == board
                    && widths.as_slice() == [(fixture.stack_a, 260u32)]
            ));
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
            }]);
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

/// R2 D25 — the due editor's typed path: ISO commits at 00:00 UTC,
/// `+N<unit>` commits relative to now (bracketed by the journey's own
/// clock reads), an empty accept clears, and a refusal dispatches
/// nothing while keeping the buffer.
#[test]
fn the_typed_due_entries_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = chrono::Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
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
            harness.type_text("2030-02-28");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(
                        chrono::Utc.with_ymd_and_hms(2030, 2, 28, 0, 0, 0).unwrap(),
                    )),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // Relative: now + 2d, bracketed by the journey's own reads.
            open_editor(&mut harness);
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

            // Empty accept clears.
            open_editor(&mut harness);
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(None),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // A refusal dispatches nothing and keeps the buffer.
            open_editor(&mut harness);
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

/// R2 D25 — the calendar picker (seeded due 2030-01-31): the grid
/// opens on the due's month, the keyboard drives the cursor and the
/// months, Enter and the typed path produce the identical command,
/// a day click ≡ cursor+Enter, and Escape closes clean.
#[allow(clippy::too_many_lines)] // the walk is the contract's length
#[test]
fn the_due_picker_journeys_reach_the_boundary() {
    let (fixture, mut state) = fixture::journey_state();
    let due = chrono::Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
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
            harness.handle.window().invoke_task_due_picker_toggled();
            harness.driver.advance_time(0);

            // January 2030: one leading blank (Tuesday start), 31
            // days, padded to 35 cells in 5 rows; the cursor sits on
            // the due (flattened index 1 + 30).
            let cursor_index = |harness: &Harness| -> Option<usize> {
                let grid = harness.handle.window().get_task_model().due_grid;
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
                let grid = harness.handle.window().get_task_model().due_grid;
                let rows = grid.row_count();
                let cells: usize = (0..rows)
                    .map(|r| grid.row_data(r).expect("row").row_count())
                    .sum();
                (rows, cells)
            };
            assert_eq!(grid_shape(&harness), (5, 35));
            assert_eq!(cursor_index(&harness), Some(31));

            // Right leaves the month (no cursor flag visible); Left
            // returns to the due.
            harness.stroke(parse("Right").expect("Right"));
            harness.driver.advance_time(0);
            assert_eq!(cursor_index(&harness), None, "Feb 1 is off-grid");
            harness.stroke(parse("Left").expect("Left"));
            harness.driver.advance_time(0);
            assert_eq!(cursor_index(&harness), Some(31));

            // PageDown: February 2030, the cursor clamped to the 28th
            // (leading 4 + 27 = flattened 31 again, 35 cells).
            harness.stroke(parse("PageDown").expect("PageDown"));
            harness.driver.advance_time(0);
            assert_eq!(grid_shape(&harness), (5, 35));
            assert_eq!(cursor_index(&harness), Some(31));

            // Enter on the cursor dispatches the same command the
            // typed path produces for the same date (click ≡ key).
            harness.stroke(parse("Return").expect("Return"));
            let picked = vec![StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(
                        chrono::Utc.with_ymd_and_hms(2030, 2, 28, 0, 0, 0).unwrap(),
                    )),
                    ..taskboard_domain::TaskChanges::default()
                },
            }];
            harness.assert_commands(picked.as_slice());

            // The typed path for the same date: the identical command.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.type_text("2030-02-28");
            harness.stroke(parse("Return").expect("Return"));
            harness.assert_commands(picked.as_slice());

            // A day-cell click ≡ cursor+Enter: the 15th commits.
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.handle.window().invoke_task_due_picker_toggled();
            harness.driver.advance_time(0);
            harness.handle.window().invoke_task_due_day_clicked(1 + 14);
            harness.assert_commands(&[StateCommand::UpdateTask {
                id: fixture.task1,
                changes: taskboard_domain::TaskChanges {
                    duedate: Some(Some(
                        chrono::Utc.with_ymd_and_hms(2030, 1, 15, 0, 0, 0).unwrap(),
                    )),
                    ..taskboard_domain::TaskChanges::default()
                },
            }]);

            // Escape closes the whole editor clean — no command, the
            // editor collapses, a reopen starts with the picker closed
            // (the clean close closed it), and the cycle leaves the
            // view closable by its own back binding (R-c).
            harness.driver.click_by_label("Edit due date");
            harness.driver.advance_time(0);
            harness.handle.window().invoke_task_due_picker_toggled();
            harness.driver.advance_time(0);
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
                !harness.handle.window().get_task_model().due_picker_open,
                "a reopen starts collapsed — the clean close closed the picker"
            );
            harness.stroke(parse("Escape").expect("Escape"));
            harness.press(ActionId::NavFocusBoard);
            assert_eq!(
                harness.handle.window().get_current_view(),
                View::Board,
                "the view's back binding still works after the cycle"
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

// -- r2 D32/D33/D34: the board-scoped connection surface -------------

use taskboard_ui_slint::requests::{BoardInfo, ConnectionsSnapshot, ConnectorInfo};

/// The secret-free board rows the tests hand over at mount.
fn board_row(id: uuid::Uuid, name: &str, bound: Option<&str>) -> BoardInfo {
    BoardInfo {
        id,
        name: name.to_owned(),
        bound_connector: bound.map(str::to_owned),
    }
}

/// The secret-free connector rows the tests hand over at mount.
fn connector_row(name: &str) -> ConnectorInfo {
    ConnectorInfo {
        name: name.to_owned(),
        server_url: "https://c.example".into(),
        username: "alice".into(),
        read_only: false,
    }
}

/// J1 — the board selector (r2 D32): rows render from the mount seed
/// (count, badge, selection flags as model facts); a click selects
/// (radio semantics, one selected); a reply snapshot re-renders both
/// surfaces (F18's no-stale pin) — the boards *and* the connector
/// rows move together off one reply.
#[test]
fn the_board_selector_renders_selects_and_refreshes_from_the_reply() {
    let (_fixture, state) = fixture::golden_state();
    let home = uuid::Uuid::new_v4();
    let away = uuid::Uuid::new_v4();
    let settings = UiSettings {
        boards: vec![
            board_row(home, "home", None),
            board_row(away, "away", Some("work")),
        ],
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

            let boards = model().get_boards();
            assert_eq!(boards.row_count(), 2, "the mount seed landed");
            assert!(!boards.row_data(0).expect("row").bound);
            let away_row = boards.row_data(1).expect("row");
            assert!(away_row.bound);
            assert_eq!(away_row.bound_connector, "work");
            assert!(
                !model().get_boards().row_data(0).expect("row").selected,
                "nothing is selected at mount"
            );

            // A click selects (radio semantics).
            window.invoke_settings_board_selected(home.to_string().into());
            assert_eq!(model().get_selected_board(), home.to_string());
            let boards = model().get_boards();
            assert!(boards.row_data(0).expect("row").selected);
            assert!(!boards.row_data(1).expect("row").selected);

            // A reply snapshot re-renders both surfaces: route the
            // save ceremony and answer with the pair.
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
                    boards: vec![
                        board_row(home, "home", Some("newc")),
                        board_row(away, "away", Some("work")),
                    ],
                    connectors: vec![connector_row("work"), connector_row("newc")],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();

            let boards = model().get_boards();
            assert_eq!(
                boards.row_data(0).expect("row").bound_connector,
                "newc",
                "the reply's binding landed on the selector"
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
/// Ok snapshot lands both surfaces; a `BindRefused` reply lands
/// `Rejected` with non-empty status text (structure, never strings).
#[test]
fn bind_routes_attach_and_the_typed_reply_lands() {
    let (_fixture, state) = fixture::golden_state();
    let home = uuid::Uuid::new_v4();
    let settings = UiSettings {
        boards: vec![board_row(home, "home", None)],
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

            // The Ok snapshot lands both surfaces.
            let taskboard_ui_slint::requests::UiRequest::AttachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the attach routed");
            };
            reply
                .send(Ok(ConnectionsSnapshot {
                    boards: vec![board_row(home, "home", Some("work"))],
                    connectors: vec![connector_row("work")],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            assert_eq!(
                model()
                    .get_boards()
                    .row_data(0)
                    .expect("row")
                    .bound_connector,
                "work",
                "the badge shows the fresh binding"
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

/// J3 — detach routing from a bound board row (r2 D33): the lane
/// carries the typed pair; the Ok snapshot clears the badge.
#[test]
fn detach_routes_from_the_board_row_and_the_ok_snapshot_clears_the_badge() {
    let (_fixture, state) = fixture::golden_state();
    let home = uuid::Uuid::new_v4();
    let settings = UiSettings {
        boards: vec![board_row(home, "home", Some("work"))],
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

            window.invoke_settings_detach_clicked(0);
            let mut requests = harness.drain_requests();
            assert_eq!(requests.len(), 1, "exactly one detach routed");
            assert!(matches!(
                &requests[0],
                taskboard_ui_slint::requests::UiRequest::DetachConnector {
                    board, connector, ..
                } if board.as_uuid() == home && connector == "work"
            ));
            let taskboard_ui_slint::requests::UiRequest::DetachConnector { reply, .. } =
                requests.remove(0)
            else {
                panic!("the detach routed");
            };
            reply
                .send(Ok(ConnectionsSnapshot {
                    boards: vec![board_row(home, "home", None)],
                    connectors: vec![connector_row("work")],
                }))
                .expect("the reply lane is alive");
            harness.handle.drain_setting_replies();
            let row = model().get_boards().row_data(0).expect("row");
            assert!(!row.bound, "the badge cleared");
            assert_eq!(row.bound_connector, "");
        },
    );
}

/// J4 — the busy gate (r2 D34): with an attach pending, Save/Test/a
/// second Bind route nothing (one op at a time); the initiating row's
/// busy flag is set; the reply clears it (reply-driven — no clock).
#[test]
fn the_busy_gate_holds_one_op_and_the_reply_clears_the_row_marker() {
    let (_fixture, state) = fixture::golden_state();
    let home = uuid::Uuid::new_v4();
    let settings = UiSettings {
        boards: vec![board_row(home, "home", None)],
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
                boards: vec![board_row(home, "home", Some("work"))],
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
                    labels: std::collections::BTreeSet::new(),
                    effort: None,
                    importance: None,
                    urgency: None,
                    recur: None,

                    archived: false,
                    deleted: false,
                    clocks: taskboard_domain::TaskClocks {
                        title: taskboard_domain::BoardClocks::epoch(),
                        description: taskboard_domain::BoardClocks::epoch(),
                        duedate: taskboard_domain::BoardClocks::epoch(),
                        scheduled: taskboard_domain::BoardClocks::epoch(),
                        done: taskboard_domain::BoardClocks::epoch(),
                        position: taskboard_domain::BoardClocks::epoch(),
                        labels: taskboard_domain::BoardClocks::epoch(),
                        effort: taskboard_domain::BoardClocks::epoch(),
                        importance: taskboard_domain::BoardClocks::epoch(),
                        urgency: taskboard_domain::BoardClocks::epoch(),
                        recur: taskboard_domain::BoardClocks::epoch(),

                        archived: taskboard_domain::BoardClocks::epoch(),
                        deleted: taskboard_domain::BoardClocks::epoch(),
                    },
                    remote_seen: None,
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
    let (_fixture, state, _walked) = fixture::today_overdue_state();
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
