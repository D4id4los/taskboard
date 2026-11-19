// SPDX-License-Identifier: AGPL-3.0-only

//! The habit-parity journeys (J1 US-1/US-2, round 9 chunk 6 — the
//! §6 wiring table's nominated evidence file `habits_journey.rs`):
//! habit rows on the task-card substrate, the done-slot log, the
//! context menu with the armed convert submenu, and the kiosk
//! greyed-menu posture. Assertion discipline is the journeys suite's:
//! terminal state + the typed commands that reached the fake-engine
//! boundary; labels are addresses, never text oracles.

mod support;

use support::driver::{DriverOptions, JourneyDriver, SlintDriver};
use support::fake_engine::FakeEngine;
use support::fixture;
use support::health::detached_health;

use arc_swap::ArcSwap;
use slint::ComponentHandle as _;
use slint::Model as _;
use std::sync::Arc;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::{KeyboardState, UiSettings};

/// The minimal habit-journey harness: driver + fake engine + mount.
/// The journeys suite's harness is that suite's surface; this file
/// carries only what the habit journeys speak.
struct Harness {
    driver: SlintDriver,
    handle: taskboard_ui_slint::UiHandle,
    engine: FakeEngine,
}

/// The open submenu's rows (the flip the arm re-projection drives is
/// a submenu-page fact — the confirm lives under the Convert parent).
fn submenu_rows(harness: &Harness) -> Vec<taskboard_ui_slint::MenuEntryModel> {
    let entries = keyboard(harness).get_menu_submenu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row"))
        .collect()
}

/// The first submenu-parent row's index on the open root page.
fn parent_index(harness: &Harness) -> usize {
    let entries = keyboard(harness).get_menu_entries();
    (0..entries.row_count())
        .find(|i| entries.row_data(*i).expect("row").submenu)
        .expect("the Convert parent renders")
}

impl Harness {
    fn mount(driver: SlintDriver, state: AppState) -> Self {
        Self::mount_with_settings(driver, state, UiSettings::default())
    }

    fn mount_with_settings(driver: SlintDriver, state: AppState, settings: UiSettings) -> Self {
        let state = Arc::new(ArcSwap::from_pointee(state));
        let (commands, engine) = FakeEngine::channel(Arc::clone(&state));
        let (requests_tx, _requests_rx) = tokio::sync::mpsc::channel(16);
        let handle = taskboard_ui_slint::mount(
            state,
            dummy_receiver(),
            dummy_system_receiver(),
            commands,
            requests_tx,
            settings,
            detached_health(),
            None,
        )
        .expect("mount succeeds on the testing backend");
        let mut harness = Self {
            driver,
            handle,
            engine,
        };
        harness.driver.bind(&harness.handle);
        harness
    }
}

fn dummy_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::EngineSignal> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn dummy_system_receiver() -> tokio::sync::broadcast::Receiver<taskboard_domain::SystemEvent> {
    let (_tx, rx) = tokio::sync::broadcast::channel(4);
    rx
}

fn keyboard(harness: &Harness) -> KeyboardState<'_> {
    harness.handle.window().global::<KeyboardState>()
}

/// The journeys' habit fixture (the lib fixture's twin): one live
/// daily habit on the fixture board.
fn state_habit(state: &mut AppState, board: taskboard_domain::BoardId) {
    state_habit_at(state, board, 0, "water");
}

/// The habit fixture's order/titled generalization (chunk 8): the
/// reorder journeys need a multi-row list in stored order.
fn state_habit_at(state: &mut AppState, board: taskboard_domain::BoardId, order: i64, title: &str) {
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: title.to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(2).unwrap(),
        ),
        order,
        anchor_time: None,
        anchor_day: None,
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        by: None,
    };
    state.habits.habits.insert(habit.id, habit);
}

/// US-1/S1 (AC-1): the habit rows render on the task-card substrate
/// with the horizontal action strip — the section model carries real
/// `TaskCardModel`s (the same substrate the task rows speak), the
/// card's accessible label is the title (the D16 address), and the
/// strip's log affordance renders at a task row's entry height (no
/// fixed px: the card's own min-height governs).
#[test]
fn us1_s1_habit_rows_render_on_the_task_card_substrate() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // The mount's default view IS Today (chunk 10 D2); the
            // habits section mounts with it.
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            let model = window.get_habits_model();
            assert_eq!(model.row_count(), 1, "the row renders");
            let row = model.row_data(0).expect("row");
            assert_eq!(
                row.card.title.as_str(),
                "water",
                "the card substrate carries the habit"
            );
            assert!(
                harness.driver.find_by_label("water").is_some(),
                "the card's accessible address is the habit title"
            );
            assert!(
                harness.driver.find_by_label("Log habit").is_some(),
                "the horizontal strip renders the +1 affordance"
            );
            assert!(
                harness
                    .driver
                    .find_by_label("Take one habit log back")
                    .is_some(),
                "the strip renders the −1 affordance"
            );
        },
    );
}

/// US-1/S2 (AC-1): the done slot IS the +1 log control — clicking
/// complete routes `StateCommand::LogHabit` (the shared seam the
/// strip's +1 routes; surplus logs are first-class, never a done
/// bit).
#[test]
fn us1_s2_clicking_the_done_slot_logs_the_habit() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // The mount's default view IS Today (chunk 10 D2); the
            // habits section mounts with it.
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            // The card model's done slot: the log control's contract
            // (can-done, never done).
            let row = harness
                .handle
                .window()
                .get_habits_model()
                .row_data(0)
                .expect("row");
            assert!(row.card.can_done && !row.card.done);
            // The shared seam (done slot ≡ strip +1): the LogHabit
            // command reaches the boundary.
            harness.handle.window().invoke_habits_log_clicked(0);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::LogHabit { .. }] => {}
                other => panic!("the done slot routes the log, got {other:?}"),
            }
        },
    );
}

/// US-2/S1 (AC-1): the right-click opens the habit context menu —
/// the task-parity grammar (pointer-anchored object menu over the
/// one chain) with the habit-specific verbs and the Convert parent;
/// no task-only row leaks in.
#[test]
fn us2_s1_right_click_opens_the_habit_context_menu() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            // The mount's default view IS Today (chunk 10 D2); the
            // habits section mounts with it.
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            window.invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open(), "the menu opened");
            assert!(
                keyboard(&harness).get_menu_pointer_anchored(),
                "anchored at the pointer (the object-menu grammar)"
            );
            let entries = keyboard(&harness).get_menu_entries();
            let actions: Vec<String> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row").action.to_string())
                .collect();
            for verb in [
                "habit.log",
                "habit.unlog",
                "habit.move-up",
                "habit.move-down",
                "habit.convert-arm",
            ] {
                assert!(
                    actions.contains(&verb.to_string()),
                    "the habit verb renders: {verb} in {actions:?}"
                );
            }
            assert!(
                actions.iter().all(|a| !a.starts_with("task.")),
                "no task-only row leaks into the habit page"
            );
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            assert!(
                rows.last().is_some_and(|r| r.submenu),
                "the Convert parent closes the page (destructive-last law)"
            );
        },
    );
}

/// US-2/S2 (H2 D4, O-7 Q3): the habit menu's arm routes INSIDE the
/// open Convert submenu — the re-projected page flips the confirm row
/// from greyed to enabled, greys the arm row, and joins the cancel
/// step; nothing dispatches at the arm step (the consequence-first
/// staging grammar; the row-strip echo this journey pinned before
/// retires with the strip itself). The confirm then routes the
/// domain verb.
#[test]
fn us2_s2_menu_arm_enables_the_confirm_inside_the_menu() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            // The right-click opens the habit menu (the row's focus
            // fact mints inside the opener).
            window.invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(keyboard(&harness).get_menu_open(), "the menu opened");

            // Walk the cursor to the Convert parent (the page's last
            // row) and enter the submenu.
            let parent = parent_index(&harness);
            for _ in 0..parent {
                harness.handle.dispatch_action(ActionId::MenuNext);
            }
            harness.handle.dispatch_action(ActionId::MenuInvoke);
            harness.driver.advance_time(0);
            let confirm = submenu_rows(&harness)
                .into_iter()
                .find(|r| r.action == "habit.convert-to-series")
                .expect("the confirm child renders");
            assert!(
                !confirm.enabled,
                "unarmed: the confirm greys (consequence first)"
            );

            // The arm routes: the re-projection flips the confirm on
            // INSIDE the open submenu; nothing dispatched.
            harness.handle.dispatch_action(ActionId::HabitConvertArm);
            harness.driver.advance_time(0);
            let rows = submenu_rows(&harness);
            let confirm_enabled = rows
                .iter()
                .find(|r| r.action == "habit.convert-to-series")
                .map(|r| r.enabled)
                .expect("the confirm child still renders");
            assert!(
                confirm_enabled,
                "the re-projection enables the armed confirm inside the menu"
            );
            assert!(
                rows.iter().any(|r| r.action == "habit.convert-cancel"),
                "the cancel child joins while armed"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the arm stages the grammar; the confirm dispatches"
            );

            // The confirm routes the domain verb with the armed
            // pattern.
            harness
                .handle
                .dispatch_action(ActionId::HabitConvertToSeries);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::ConvertHabitToSeries { .. }] => {}
                other => panic!("the confirm routes the conversion, got {other:?}"),
            }
        },
    );
}

/// US-2/S2 (the disarm side, H2 D4): the cancel step disarms INSIDE
/// the open submenu — the re-projected confirm greys again and
/// nothing ever reached the boundary.
#[test]
fn the_menu_cancel_disarms_inside_the_menu() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            window.invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            let parent = parent_index(&harness);
            for _ in 0..parent {
                harness.handle.dispatch_action(ActionId::MenuNext);
            }
            harness.handle.dispatch_action(ActionId::MenuInvoke);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::HabitConvertArm);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::HabitConvertCancel);
            harness.driver.advance_time(0);

            let entries = keyboard(&harness).get_menu_submenu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            let confirm = rows
                .iter()
                .find(|r| r.action == "habit.convert-to-series")
                .expect("the confirm child renders");
            assert!(
                !confirm.enabled,
                "the cancel disarms: the confirm greys again inside the menu"
            );
            assert!(
                rows.iter().all(|r| r.action != "habit.convert-cancel"),
                "the cancel child leaves when disarmed"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "cancel and disarm dispatch nothing"
            );
        },
    );
}

/// H2 B5/D6 (HAB-14): on kiosk the habit menu still OPENS (reading a
/// menu is passive) and every habit verb row greys from the states
/// map — disabled, never withheld.
#[test]
fn the_kiosk_habit_menu_opens_with_every_verb_greyed() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            );
            harness.driver.advance_time(0);
            harness
                .handle
                .window()
                .invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_menu_open(),
                "reading a menu is passive"
            );
            let entries = keyboard(&harness).get_menu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            for row in rows.iter().filter(|r| !r.action.is_empty()) {
                assert!(
                    !row.enabled,
                    "every habit verb greys on kiosk: {:?}",
                    row.action
                );
            }
            assert!(!rows.is_empty(), "the rows render, greyed — never withheld");
        },
    );
}

// -- chunk 8: keyboard parity + drag (J1 US-3/US-4, #934/#935) --------

/// The chunk-8 fixture: the golden state plus two live habits on the
/// fixture board in stored order — a two-row list makes reorder
/// arithmetic observable at both ends.
fn state_two_habits(state: &mut AppState, board: taskboard_domain::BoardId) {
    state_habit_at(state, board, 0, "water");
    state_habit_at(state, board, 10, "stretch");
}

/// US-3/S1 (J1 AC-1, #934's traversal half): the walk enters and
/// leaves the habit section on the SAME keys as the task rows — the
/// landing paints the card emphasis ring (the walk cursor's render
/// fact), the cursor halves stay exclusive, and stepping back out
/// hands the emphasis to a task row again.
#[test]
fn us3_s1_the_walk_enters_and_leaves_habit_rows_with_the_task_keys() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);

            // Jump to the walk's last row: the LAST HABIT (the
            // widened domain's tail — tasks lead, habits follow).
            harness.handle.dispatch_action(ActionId::NavLastItem);
            harness.driver.advance_time(0);
            let emphasis = |harness: &Harness| -> Vec<bool> {
                let model = harness.handle.window().get_habits_model();
                (0..model.row_count())
                    .map(|i| model.row_data(i).expect("row").card.emphasis)
                    .collect()
            };
            assert_eq!(
                emphasis(&harness),
                vec![false, true],
                "the walk's landing paints the last habit's ring"
            );

            // The same walk keys step back through the section and
            // OUT of it — the emphasis leaves the habits entirely.
            harness.handle.dispatch_action(ActionId::NavPrevItem);
            harness.driver.advance_time(0);
            assert_eq!(emphasis(&harness), vec![true, false]);
            harness.handle.dispatch_action(ActionId::NavPrevItem);
            harness.driver.advance_time(0);
            assert_eq!(
                emphasis(&harness),
                vec![false, false],
                "the walk left the habit section on the task keys"
            );
        },
    );
}

/// US-3/S2 (J1 AC-1): the habit verbs resolve under the focused habit
/// — `Space` (the task row's complete stroke) routes the habit log
/// through the D3 translation, and the Shift+Up preset (the D4/D7
/// keyboard reorder) mints the shared floor-midpoint order — the
/// SAME order the menu row's one-shot mint produces for the same
/// row. Click ≡ key at the command boundary.
#[test]
fn us3_s2_the_task_complete_stroke_and_shift_up_route_the_habit_verbs() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);

            // The walk lands on the last habit (the second row).
            harness.handle.dispatch_action(ActionId::NavLastItem);
            harness.driver.advance_time(0);

            // The task row's complete stroke routes the habit log.
            harness.handle.dispatch_action(ActionId::TaskComplete);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::LogHabit { .. }] => {}
                other => panic!("the complete stroke routes the log, got {other:?}"),
            }

            // Shift+Up on the focused habit: the walk's landing mints
            // the reorder one-shot through the shared arithmetic —
            // the up slot sits between the head (0) and the row's own
            // order (10), so the floor midpoint is 5.
            harness.handle.dispatch_action(ActionId::HabitMoveUp);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::MoveHabit { order: 5, .. }] => {}
                other => panic!("the up slot mints the floor midpoint, got {other:?}"),
            }

            // The menu open over the same row re-mints the pair
            // through the SAME shared helper — the equivalence probe:
            // the boundary records commands without reordering the
            // fixture, so the down slot's free interval is (10, None)
            // → 11, and the mint sites agree by construction.
            harness
                .handle
                .window()
                .invoke_habits_context_requested(1, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(
                keyboard(&harness).get_menu_open(),
                "the habit menu opened over the row"
            );
            harness.handle.dispatch_action(ActionId::HabitMoveDown);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::MoveHabit { order: 11, .. }] => {}
                other => panic!("the tail slot mints the appended order, got {other:?}"),
            }
        },
    );
}

/// US-3/S2 (the kiosk gate, HAB-14): the reorder binding stays
/// visible but the dispatch refuses typed — the `NoTarget` arm surfaces
/// the status-notice refusal and nothing reaches the boundary.
#[test]
fn the_reorder_pair_refuses_typed_on_kiosk() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount_with_settings(
                SlintDriver::default(),
                state,
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            );
            harness.driver.advance_time(0);

            // The walk still enters the habit row (traversal renders
            // on kiosk — D-P13); the reorder dispatch refuses.
            harness.handle.dispatch_action(ActionId::NavLastItem);
            harness.driver.advance_time(0);
            harness.handle.dispatch_action(ActionId::HabitMoveUp);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_notice_visible(),
                "the typed refusal surfaced on the notice lane"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the kiosk reorder dispatches nothing"
            );
        },
    );
}

/// Chunk 8 D6 (#935 / O-7 Q5): the uniform typed refusal — a
/// target-less habit dispatch surfaces the status-notice from the
/// action's registry description and routes no command; the Disabled
/// posture is the registry arm's, untouched here.
#[test]
fn the_targetless_habit_dispatch_surfaces_the_typed_refusal() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);

            // No walk has entered and no row was clicked: the habit
            // verb has no subject — the typed NoTarget.
            harness.handle.dispatch_action(ActionId::HabitUndoLog);
            harness.driver.advance_time(0);
            assert!(
                harness.handle.window().get_notice_visible(),
                "the refusal surfaced"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "a subjectless dispatch routes nothing"
            );
        },
    );
}

/// US-4/S1 (J1 AC-1): the #918 gap-drop over the flat habit list —
/// a drop at the head gap mints the list's leading order, a drop
/// past the end clamps to the append slot (the board's clamp law),
/// and kiosk absorbs the pointer seam silently.
#[test]
fn us4_s1_the_gap_drop_reorders_across_the_habit_list() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::driver::run(
        DriverOptions {
            headless_renderer: true,
        },
        move |_driver| {
            let mut harness = Harness::mount(SlintDriver::default(), state);
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            // The second row drops into the head gap: the slot's free
            // interval is (None, 0) — the floor midpoint −1.
            window.invoke_habits_drop_asked(1, 0);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::MoveHabit { order: -1, .. }] => {}
                other => panic!("the head gap mints the leading order, got {other:?}"),
            }

            // A gap past the end clamps to the append slot: the row
            // source's tail order + 1.
            window.invoke_habits_drop_asked(0, 99);
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::MoveHabit { order: 11, .. }] => {}
                other => panic!("the over-run gap clamps to the append slot, got {other:?}"),
            }

            // The kiosk pointer seam absorbs silently (the D5 gate).
            let mut kiosk = Harness::mount_with_settings(
                SlintDriver::default(),
                golden_state_with_habits(),
                UiSettings {
                    kiosk: true,
                    ..UiSettings::default()
                },
            );
            kiosk.driver.advance_time(0);
            kiosk.handle.window().invoke_habits_drop_asked(1, 0);
            kiosk.driver.advance_time(0);
            assert!(
                kiosk.engine.drain().is_empty(),
                "the kiosk drop dispatches nothing"
            );
        },
    );
}

/// The kiosk drop leg's state source (the closure-borrow shape): the
/// fixture rebuilt with the two-habit list.
fn golden_state_with_habits() -> AppState {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    state
}
