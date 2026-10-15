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
use taskboard_ui_slint::menu;
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
    /// (`menu::entries`' hint path — reuse, not reimplement) and
    /// injecting each stroke; counts the presses on the open guard
    /// phase (§9.2 accounting).
    fn press(&mut self, action: ActionId) {
        let keymap = UiSettings::default().keymap;
        let entries = menu::entries(&keymap, &self.chain());
        let entry = entries
            .iter()
            .find(|entry| entry.action == action)
            .unwrap_or_else(|| {
                panic!(
                    "{action:?} is bound in the visible chain {:?}",
                    self.chain()
                )
            });
        assert!(entry.enabled, "{action:?} is enabled");
        for spelling in &entry.hint {
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
    support::driver::run(DriverOptions::default(), move |_driver| {
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
            [StateCommand::UpdateTask { id, changes }] if changes.title.as_deref() == Some("y") => {
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

        // -- step 8: app.menu — open, cursor, invoke ------------------
        //    The cursor lands on the second row (registry order puts
        //    app.exit first, app.menu second), so the invoke
        //    re-dispatches app.menu itself — the visible re-dispatch
        //    path: the menu closes and immediately reopens fresh.
        harness.guard.begin(Verb::MenuInvoke);
        harness.press(ActionId::AppMenu);
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "app.menu opens the menu"
        );
        assert_eq!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_selected_index(),
            0
        );
        harness.press(ActionId::MenuNext);
        assert_eq!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_selected_index(),
            1
        );
        harness.press(ActionId::MenuInvoke);
        harness.guard.end(Verb::MenuInvoke);
        assert!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "the invoke closed the menu and the re-dispatched app.menu reopened it"
        );
        assert_eq!(
            harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_selected_index(),
            0,
            "the reopened menu's cursor is fresh"
        );
        harness.stroke(parse("Escape").expect("Escape")); // menu.dismiss
        assert!(
            !harness
                .handle
                .window()
                .global::<taskboard_ui_slint::KeyboardState>()
                .get_menu_open(),
            "dismiss closes the menu"
        );
        harness.assert_commands(&[]);

        // -- step 9: read the sync status indicator -------------------
        //    Typed model read (the badge's label text is data, never
        //    an assertion target).
        let sync = harness.handle.window().get_sync_model();
        assert_eq!(sync.phase, taskboard_ui_slint::SyncPhaseUi::Offline);
        assert_eq!(sync.pending, 2, "the fixture's pending ops render");

        // -- step 10: app.exit — the typed quit outcome ---------------
        harness.press(ActionId::AppExit);
        assert_eq!(
            harness.handle.quit_requests(),
            1,
            "the exit effect fired exactly once"
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
    });
}
