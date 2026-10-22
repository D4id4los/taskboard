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
use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::guard::{Guard, Verb};

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
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
                View::Board,
                "the view cycle wraps"
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
            assert_eq!(current_view(&harness), View::TaskDetail, "wrap backwards");
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
                0,
                "the compact board starts on the first stack"
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
                let expected = 280u32.saturating_sub(32 * step).max(96);
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
            // member.
            for expected in [View::StackDetail, View::TaskDetail, View::Board] {
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
            reply
                .send(Ok(vec![taskboard_ui_slint::requests::ConnectorInfo {
                    name: "home".into(),
                    server_url: "https://c.example".into(),
                    username: "alice".into(),
                    read_only: false,
                }]))
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
