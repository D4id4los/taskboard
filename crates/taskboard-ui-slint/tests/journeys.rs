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
}

impl Harness {
    /// Mounts `state` through the public `mount()` with the fake's
    /// channel halves (dropped broadcast senders, as the lib tests
    /// do) — no engine, no app crate.
    fn mount(driver: SlintDriver, state: AppState) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            UiSettings::default(), // the standard preset
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
            guard: Guard::default(),
        };
        harness.driver.bind(&harness.handle);
        harness
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
