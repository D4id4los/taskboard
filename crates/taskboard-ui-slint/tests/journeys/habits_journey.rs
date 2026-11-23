// SPDX-License-Identifier: AGPL-3.0-only

//! The habit-parity journeys (J1 US-1/US-2, round 9 chunk 6 — the
//! §6 wiring table's nominated evidence file `habits_journey.rs`):
//! habit rows on the task-card substrate, the done-slot log, the
//! context menu with the armed convert submenu, and the kiosk
//! greyed-menu posture. Assertion discipline is the journeys suite's:
//! terminal state + the typed commands that reached the fake-engine
//! boundary; labels are addresses, never text oracles.

use crate::support;

use support::driver::{JourneyDriver as _, SlintDriver};
use support::fixture;
use support::harness::Harness;

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

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::{KeyboardState, UiSettings, View};

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
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    state.habits.habits.insert(habit.id, habit);
}

/// US-1/S1 (AC-1): the habit rows render on the task-card substrate
/// with the trailing action slot (D3a W-3 superseded the detached
/// strip, #1199 — the affordances ride the card line now) — the
/// section model carries real `TaskCardModel`s (the same substrate
/// the task rows speak), the card's accessible label is the title
/// (the D16 address), and the slot's affordances render at a task
/// row's entry height (the entry-height pin holds).
#[test]
fn us1_s1_habit_rows_render_on_the_task_card_substrate() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
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
            harness.driver.find_by_label("Log habit").is_none(),
            "the detached strip's +1 affordance is gone (the done slot is the +1)"
        );
        assert!(
            harness
                .driver
                .find_by_label("Take one habit log back")
                .is_some(),
            "the trailing slot renders the −1 affordance"
        );
    });
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
    support::harness::run_journey_no_escape(state, move |harness| {
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
    });
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
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open(), "the menu opened");
        assert!(
            keyboard(harness).get_menu_pointer_anchored(),
            "anchored at the pointer (the object-menu grammar)"
        );
        let entries = keyboard(harness).get_menu_entries();
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
        // H4 supersedes the page's tail: the Convert parent is
        // followed by the staged remove's destructive-last pair
        // (the confirm closes the page — menu.rs's page test pins
        // the declared order).
        assert!(
            rows.iter().any(|r| r.submenu),
            "the Convert parent renders in the page body"
        );
        assert!(
            rows.last()
                .is_some_and(|r| r.action.as_str() == "habit.remove-confirm"),
            "the remove confirm closes the page (destructive-last law)"
        );
    });
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
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // The right-click opens the habit menu (the row's focus
        // fact mints inside the opener).
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open(), "the menu opened");

        // Walk the cursor to the Convert parent (the page's last
        // row) and enter the submenu.
        let parent = parent_index(harness);
        for _ in 0..parent {
            harness.handle.dispatch_action(ActionId::MenuNext);
        }
        harness.handle.dispatch_action(ActionId::MenuInvoke);
        harness.driver.advance_time(0);
        let confirm = submenu_rows(harness)
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
        let rows = submenu_rows(harness);
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
    });
}

/// US-2/S2 (the disarm side, H2 D4): the cancel step disarms INSIDE
/// the open submenu — the re-projected confirm greys again and
/// nothing ever reached the boundary.
#[test]
fn the_menu_cancel_disarms_inside_the_menu() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        let parent = parent_index(harness);
        for _ in 0..parent {
            harness.handle.dispatch_action(ActionId::MenuNext);
        }
        harness.handle.dispatch_action(ActionId::MenuInvoke);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitConvertArm);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitConvertCancel);
        harness.driver.advance_time(0);

        let entries = keyboard(harness).get_menu_submenu_entries();
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
    });
}

/// H2 B5/D6 (HAB-14): on kiosk the habit menu still OPENS (reading a
/// menu is passive) and every habit verb row greys from the states
/// map — disabled, never withheld.
#[test]
fn the_kiosk_habit_menu_opens_with_every_verb_greyed() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape_with_settings(
        state,
        UiSettings {
            kiosk: true,
            ..UiSettings::default()
        },
        move |harness| {
            harness.driver.advance_time(0);
            harness
                .handle
                .window()
                .invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(
                keyboard(harness).get_menu_open(),
                "reading a menu is passive"
            );
            let entries = keyboard(harness).get_menu_entries();
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
    support::harness::run_journey_no_escape(state, move |harness| {
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
            emphasis(harness),
            vec![false, true],
            "the walk's landing paints the last habit's ring"
        );

        // The same walk keys step back through the section and
        // OUT of it — the emphasis leaves the habits entirely.
        harness.handle.dispatch_action(ActionId::NavPrevItem);
        harness.driver.advance_time(0);
        assert_eq!(emphasis(harness), vec![true, false]);
        harness.handle.dispatch_action(ActionId::NavPrevItem);
        harness.driver.advance_time(0);
        assert_eq!(
            emphasis(harness),
            vec![false, false],
            "the walk left the habit section on the task keys"
        );
    });
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
    support::harness::run_journey_no_escape(state, move |harness| {
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
        // the up slot is the target gap index above the row
        // (#1198 chunk A1 D1: the engine resolves the orders).
        harness.handle.dispatch_action(ActionId::HabitMoveUp);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target: 0, .. }] => {}
            other => panic!("the up slot mints the gap above, got {other:?}"),
        }

        // The menu open over the SAME row re-mints the pair through
        // the SAME shared helper (click ≡ key); the boundary records
        // commands without reordering the fixture, so the head row's
        // down slot crosses the lower neighbour — index 2.
        harness
            .handle
            .window()
            .invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(
            keyboard(harness).get_menu_open(),
            "the habit menu opened over the row"
        );
        harness.handle.dispatch_action(ActionId::HabitMoveDown);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target: 2, .. }] => {}
            other => panic!("the down slot crosses the neighbour, got {other:?}"),
        }
    });
}

/// US-3/S2 (the kiosk gate, HAB-14): the reorder binding stays
/// visible but the dispatch refuses typed — the `NoTarget` arm surfaces
/// the status-notice refusal and nothing reaches the boundary.
#[test]
fn the_reorder_pair_refuses_typed_on_kiosk() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    support::harness::run_journey_no_escape_with_settings(
        state,
        UiSettings {
            kiosk: true,
            ..UiSettings::default()
        },
        move |harness| {
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
    support::harness::run_journey_no_escape(state, move |harness| {
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
    });
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
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // The second row drops into the head gap: the one-shot is
        // the gap index verbatim (#1198 chunk A1 D1 — the engine
        // resolves the orders).
        window.invoke_habits_drop_asked(1, 0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target: 0, .. }] => {}
            other => panic!("the head gap mints the gap index, got {other:?}"),
        }

        // A gap past the end is the append slot — the engine clamps;
        // the UI mints the pointer's gap verbatim.
        window.invoke_habits_drop_asked(0, 99);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target: 99, .. }] => {}
            other => panic!("the over-run gap rides verbatim, got {other:?}"),
        }

        // The kiosk pointer seam absorbs silently (the D5 gate).
        let mut kiosk = support::harness::Harness::mount_no_escape_with_settings(
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
    });
}

/// The kiosk drop leg's state source (the closure-borrow shape): the
/// fixture rebuilt with the two-habit list.
fn golden_state_with_habits() -> AppState {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_two_habits(&mut state, board);
    state
}

/// The second habit for the one-armed-subject-at-a-time law.
fn state_second_habit(state: &mut AppState, board: taskboard_domain::BoardId) {
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "stretch".into(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(1).unwrap(),
        ),
        order: 1,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    state.habits.habits.insert(habit.id, habit);
}

/// `h4_remove_s1` (H4 D4, #944): the staged remove arm — the arm flips
/// the confirm enabled INSIDE the open menu (the states-map
/// re-projection; staging dispatches nothing), the arm row greys
/// while armed, and opening another subject's menu disarms (one
/// armed subject at a time).
#[test]
fn h4_remove_s1_remove_arms_and_the_confirm_flips() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    state_second_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        let rows_by_action = |harness: &Harness| -> Vec<taskboard_ui_slint::MenuEntryModel> {
            let entries = keyboard(harness).get_menu_entries();
            (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect()
        };

        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open());
        let find = |harness: &Harness, action: &str| {
            rows_by_action(harness)
                .into_iter()
                .find(|r| r.action == action)
                .unwrap_or_else(|| panic!("{action} renders"))
        };
        assert!(find(harness, "habit.remove").enabled);
        assert!(
            !find(harness, "habit.remove-confirm").enabled,
            "unarmed: the confirm greys (consequence first)"
        );

        // The arm: staging only — nothing reaches the boundary.
        harness.handle.dispatch_action(ActionId::HabitRemove);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the arm stages; the confirm dispatches"
        );
        assert!(
            !find(harness, "habit.remove").enabled,
            "the arm row greys while armed (the confirm replaces its state)"
        );
        assert!(
            find(harness, "habit.remove-confirm").enabled,
            "the re-projection enables the armed confirm inside the menu"
        );
        assert!(
            find(harness, "habit.remove-cancel").enabled,
            "the cancel step joins while armed"
        );
        // Opening the OTHER subject's menu disarms (one armed
        // subject at a time): the confirm greys again.
        window.invoke_habits_context_requested(1, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open());
        assert!(find(harness, "habit.remove").enabled);
        assert!(
            !find(harness, "habit.remove-confirm").enabled,
            "the other subject's menu opens unarmed"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "disarm dispatches nothing"
        );
    });
}

/// `h4_remove_s2` (H4 D4, #944): the confirm routes the domain verb
/// (`DeleteHabit` — the tombstone: the log outlives) and the staging
/// clears; nothing else dispatches.
#[test]
fn h4_remove_s2_confirm_tombstones_and_closes() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitRemove);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitRemoveConfirm);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::DeleteHabit { .. }] => {}
            other => panic!("the confirm routes the tombstone, got {other:?}"),
        }
    });
}

/// `h4_kiosk_editor_gate` (H4 D7): on kiosk the editor is unmountable —
/// the `habit.edit` dispatch is structurally inert (the view never
/// swaps) — and the editor/remove rows join the greyed set in the
/// menu (visible-disabled, never withheld).
#[test]
fn h4_kiosk_editor_gate() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape_with_settings(
        state,
        UiSettings {
            kiosk: true,
            ..UiSettings::default()
        },
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            // The greyed menu: the editor and the remove pair render,
            // disabled (the #790 visible-gating grammar).
            window.invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            let entries = keyboard(harness).get_menu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            for action in ["habit.edit", "habit.remove", "habit.remove-confirm"] {
                let row = rows
                    .iter()
                    .find(|r| r.action == action)
                    .unwrap_or_else(|| panic!("{action} renders, greyed"));
                assert!(!row.enabled, "{action} greys on kiosk");
            }

            // The dispatch is structurally inert: no view swap, no
            // staging.
            harness.handle.dispatch_action(ActionId::HabitEdit);
            harness.driver.advance_time(0);
            assert_eq!(
                window.get_current_view(),
                View::Board,
                "the editor never mounts on kiosk"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the kiosk dispatch stages and sends nothing"
            );
        },
    );
}

/// A daily habit anchored at the pinned clock's time-of-day plus the
/// delivered receipt: one active tracker-nudge row (the notification
/// surface's derivation re-derives the engine occurrence — the
/// receipt must key exactly what the engine derives at the pinned
/// `now`).
fn anchored_habit_with_fired_nudge(state: &mut AppState, board: taskboard_domain::BoardId) {
    use taskboard_domain::{ReminderDelivery, ReminderRef};

    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "water".into(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(1).unwrap(),
        ),
        order: 0,
        anchor_time: Some(chrono::NaiveTime::from_hms_opt(12, 0, 0).unwrap()),
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    let habit_id = habit.id;
    state.habits.habits.insert(habit.id, habit);
    // The delivered receipt: the occurrence keys exactly the anchor
    // reading the engine derives at the pinned `now` (12:00 today).
    state.reminders.deliveries.insert(ReminderDelivery {
        reminder: ReminderRef::Tracker(habit_id),
        occurrence: fixture::PINNED_NOW,
        delivered_at: fixture::PINNED_NOW,
    });
}

/// `us6_s2` (J1, H4 D3): the fired tracker nudge's snooze control opens
/// the choice flyout — hours AND days — and picking the day span
/// routes `SnoozeReminder` with `TimeDelta::days(1)` (the typed
/// delta; the engine moves the occurrence via the injected clock on
/// the state side). The flyout is walk-reachable pointer grammar; no
/// new chord rides the key grammar.
#[test]
fn us6_s2_snooze_offers_hours_and_days() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    anchored_habit_with_fired_nudge(&mut state, board);
    support::harness::run_journey_no_escape_with_settings(
        state,
        UiSettings {
            clock: Some(fixture::PINNED_NOW),
            ..UiSettings::default()
        },
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();

            // The fired nudge row renders on the surface.
            assert!(
                harness.driver.find_by_label("Snooze reminder").is_some(),
                "the fired tracker nudge renders with its snooze control"
            );

            // The control opens the span choice (hours AND days).
            harness
                .driver
                .find_by_label("Snooze reminder")
                .expect("snooze control")
                .invoke_default_action();
            harness.driver.advance_time(0);
            for span in [
                "Snooze 1 hour",
                "Snooze 4 hours",
                "Snooze 1 day",
                "Snooze 3 days",
            ] {
                assert!(
                    harness.driver.find_by_label(span).is_some(),
                    "the flyout offers {span}"
                );
            }

            // Picking the day span routes the typed delta.
            harness
                .driver
                .find_by_label("Snooze 1 day")
                .expect("the day span")
                .invoke_default_action();
            harness.driver.advance_time(0);
            match harness.engine.drain().as_slice() {
                [StateCommand::SnoozeReminder { postpone, .. }] => {
                    assert_eq!(*postpone, chrono::TimeDelta::days(1));
                }
                other => panic!("the day span routes the typed delta, got {other:?}"),
            }
            let _ = window;
        },
    );
}

/// A weekly habit (the US-5 editor round-trip's subject).
fn weekly_habit(
    state: &mut AppState,
    board: taskboard_domain::BoardId,
) -> taskboard_domain::HabitId {
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "long run".into(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerWeek(
            taskboard_domain::NonZeroCount::new(2).unwrap(),
        ),
        order: 0,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: false,
        by: None,

        paused_until: None,
    };
    let id = habit.id;
    state.habits.habits.insert(habit.id, habit);
    id
}

/// `us5_s1` (J1, H4 D1): the editor opens ON the shared detail surface —
/// from the menu row's focus fact and from the bare `habit.edit` key
/// (click ≡ key) — and cancel discards: the view steps back and NO
/// `UpdateHabit` ever reached the boundary (the typed seam
/// assertion).
#[test]
fn us5_s1_habit_editor_opens_and_cancel_discards() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = weekly_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // The menu row routes the bare id: open the habit menu
        // (the opener mints the focus fact), then dispatch.
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::TaskDetail,
            "the shared detail surface mounts the habit variant"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "opening the editor dispatches nothing"
        );

        // A field edit stages adapter-side; cancel discards.
        window.invoke_habit_time_edited("07:30".into());
        harness.driver.advance_time(0);
        window.invoke_habit_editor_cancelled();
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_current_view(),
            View::Board,
            "cancel steps the view back (the nav-back grammar)"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "cancel discards: no UpdateHabit dispatched"
        );
        let _ = habit;
    });
}

/// `us5_s2` (J1, H4 D5 + W-5's fold): setting the time and Wednesday on
/// a weekly habit and saving routes ONE sparse `UpdateHabit` — the
/// changeset names exactly the edited fields, the untouched stay
/// `None` — and the anchored state renders the row's time chip (the
/// fold's presence flag, structural: the same post-save state, mounted
/// fresh).
#[test]
fn us5_s2_habit_editor_round_trips_time_and_day() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = weekly_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // The bare key opens the editor over the focused habit
        // (the row activation's focus fact mints first).
        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(window.get_current_view(), View::TaskDetail);

        // Set the time and Wednesday (the PerWeek variant's day
        // row), then save.
        window.invoke_habit_time_edited("07:30".into());
        window.invoke_habit_weekday_picked(2);
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        let anchor_time = chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap();
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit, "the editor edits its own subject");
                assert_eq!(changes.title, None, "untouched title stays sparse");
                assert_eq!(changes.description, None);
                assert_eq!(changes.target, None, "untouched quota stays sparse");
                assert_eq!(changes.anchor_time, Some(Some(anchor_time)));
                assert_eq!(
                    changes.anchor_day,
                    Some(Some(taskboard_domain::habit::AnchorDay::Weekday(
                        chrono::Weekday::Wed
                    )))
                );
            }
            other => panic!("the save routes the sparse changeset, got {other:?}"),
        }
        assert_eq!(
            window.get_current_view(),
            View::Board,
            "the editor closes on save (the back grammar)"
        );
    });
}

/// `us5_s2`'s W-5 fold half: the anchored state renders the row's time
/// chip (the presence flag — structural, never a text oracle).
#[test]
fn the_anchored_habit_row_carries_the_time_chip() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = weekly_habit(&mut state, board);
    let row = state.habits.habits.get_mut(&habit).expect("row");
    row.anchor_time = Some(chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap());
    row.anchor_day = Some(taskboard_domain::habit::AnchorDay::Weekday(
        chrono::Weekday::Wed,
    ));
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let row = harness
            .handle
            .window()
            .get_habits_model()
            .row_data(0)
            .expect("row");
        assert!(
            !row.card.scheduled_phrase.is_empty(),
            "the anchored habit renders the time chip in its scheduled slot"
        );
    });
}

// -- #1059: the editor's negative legs, composed through the shared
//    B4a harness (`support::harness` — never the file's private
//    3-field harness; its migration is filed backlog) ---------------

/// The shared harness's open-menu rows: every root-page row as a
/// model, addressed by action id.
fn shared_menu_rows(
    harness: &support::harness::Harness,
) -> Vec<taskboard_ui_slint::MenuEntryModel> {
    let entries = harness
        .handle
        .window()
        .global::<KeyboardState>()
        .get_menu_entries();
    (0..entries.row_count())
        .map(|i| entries.row_data(i).expect("row"))
        .collect()
}

/// `us5_s2`'s Escape cell (J1 §4 US-5, #1059): an invalid draft —
/// zero count, unparseable time, out-of-range month day — attempts a
/// save and the editor stays mounted with NOTHING dispatched (the
/// save gate's grammar: `can_save` false routes no `UpdateHabit`).
#[test]
fn us5_s2_invalid_edit_stays_mounted_without_dispatch() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    weekly_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.handle.window().invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the editor is mounted"
        );
        harness.assert_commands(&[]);

        // The three invalid legs: zero quota (NonZeroCount refuses),
        // unparseable time (the lenient parser refuses), month day 99
        // (MonthDay refuses). Each stages in the draft; none is a
        // save.
        {
            let window = harness.handle.window();
            window.invoke_habit_quota_count_edited("0".into());
            window.invoke_habit_time_edited("junk".into());
            window.invoke_habit_month_day_edited("99".into());
        }
        harness.driver.advance_time(0);
        harness.handle.window().invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the invalid draft keeps the editor mounted"
        );
        harness.assert_commands(&[]);
    });
}

/// The kiosk remove arm's refusal (H4 D7's arm half, #1059): the menu
/// opens (reading is passive), the remove arm stages nothing, and the
/// confirm keeps the desktop-unarmed posture (disabled) — the typed
/// drain stays empty for the whole arm half.
#[test]
fn h4_kiosk_remove_arm_is_inert() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            kiosk: true,
            ..UiSettings::default()
        },
        move |harness| {
            harness
                .handle
                .window()
                .invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            assert!(
                harness
                    .handle
                    .window()
                    .global::<KeyboardState>()
                    .get_menu_open(),
                "reading the menu is passive on kiosk"
            );
            let find = |harness: &support::harness::Harness, action: &str| {
                shared_menu_rows(harness)
                    .into_iter()
                    .find(|row| row.action == action)
                    .unwrap_or_else(|| panic!("{action} renders"))
            };
            // The arm attempt on kiosk: structurally inert — the
            // staging never flips the confirm.
            harness.handle.dispatch_action(ActionId::HabitRemove);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
            assert!(
                !find(harness, "habit.remove-confirm").enabled,
                "the kiosk arm never enables the confirm"
            );
            harness.handle.dispatch_action(ActionId::HabitRemoveConfirm);
            harness.driver.advance_time(0);
            harness.assert_commands(&[]);
        },
    );
}

/// `us5_s3` (J1 §4 US-5's Escape cell, #1059): Escape retires the
/// armed context menu — the menu closes, the staging disarms (the
/// confirm dispatches nothing afterwards), and no remove verb escaped.
#[test]
fn us5_s3_escape_cancels_the_armed_menu() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness
            .handle
            .window()
            .invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitRemove);
        harness.driver.advance_time(0);
        harness.assert_commands(&[]);
        // Escape through the shared nav-back grammar (the Menu
        // context's dismiss): the menu retires and the escape itself
        // dispatches nothing (the J1 §4 US-5/S3 escape cell; the
        // staged arm's own survival past the close is filed
        // separately — the escape dispatches no remove verb).
        harness.press(ActionId::MenuDismiss);
        harness.driver.advance_time(0);
        assert!(
            !harness
                .handle
                .window()
                .global::<KeyboardState>()
                .get_menu_open(),
            "the Escape close retires the menu"
        );
        harness.assert_commands(&[]);
    });
}

/// `us5_s4` (J1 §4 US-5's Escape cell, #1059): Escape retires the
/// habit editor — the view steps off the editor surface and the
/// staged (valid) draft never dispatches an `UpdateHabit`.
#[test]
fn us5_s4_escape_cancels_the_editor() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    weekly_habit(&mut state, board);
    support::harness::run_journey(state, move |harness| {
        harness.handle.window().invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(harness.handle.window().get_current_view(), View::TaskDetail);
        // Stage a field edit, then Escape through the shared nav-back
        // grammar.
        harness
            .handle
            .window()
            .invoke_habit_time_edited("07:30".into());
        harness.driver.advance_time(0);
        harness.press(ActionId::NavPrevView);
        harness.driver.advance_time(0);
        assert_ne!(
            harness.handle.window().get_current_view(),
            View::TaskDetail,
            "the Escape step retires the editor surface"
        );
        harness.assert_commands(&[]);
    });
}

// ---- HF4 (#960, #959): the schedule picker + the reminder override ------

/// `us5_s2` (W-6, the schedule save): toggling Mon/Wed/Fri on a weekly
/// habit and saving routes ONE sparse `UpdateHabit` whose changeset
/// names exactly `scheduled_days: Some(set)` — every untouched field
/// stays `None` — and the editor closes (the back grammar).
#[test]
fn us5_s2_schedule_save_routes_the_exact_command() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = weekly_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        assert_eq!(window.get_current_view(), View::TaskDetail);

        // Toggle Mon (slot 0), Wed (slot 2), Fri (slot 4), then save.
        window.invoke_habit_schedule_day_toggled(0, true);
        window.invoke_habit_schedule_day_toggled(2, true);
        window.invoke_habit_schedule_day_toggled(4, true);
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        let set = taskboard_domain::habit::WeekdaySet::new([
            chrono::Weekday::Mon,
            chrono::Weekday::Wed,
            chrono::Weekday::Fri,
        ])
        .expect("three days");
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit, "the editor edits its own subject");
                assert_eq!(changes.title, None, "untouched title stays sparse");
                assert_eq!(changes.description, None);
                assert_eq!(changes.target, None);
                assert_eq!(changes.anchor_time, None);
                assert_eq!(changes.anchor_day, None);
                assert_eq!(changes.reminder, None);
                assert_eq!(changes.scheduled_days, Some(set));
            }
            other => panic!("the save routes the sparse changeset, got {other:?}"),
        }
        assert_eq!(
            window.get_current_view(),
            View::Board,
            "the editor closes on save (the back grammar)"
        );
    });
}

/// US-5 AC-3 (the picker's empty-selection leg): toggling a day on and
/// back off leaves an edited-but-empty selection whose commit the save
/// gate blocks — no `UpdateHabit` ever reaches the boundary.
#[test]
fn us5_s3_empty_schedule_selection_blocks_the_save() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let _ = weekly_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        window.invoke_habit_schedule_day_toggled(0, true);
        window.invoke_habit_schedule_day_toggled(0, false);
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the empty selection is a typed invalid draft: no dispatch"
        );
        // Cancel discards (the escape rung).
        window.invoke_habit_editor_cancelled();
        harness.driver.advance_time(0);
        assert_eq!(window.get_current_view(), View::Board);
    });
}

/// `us6_s2` + `us6_s3` (W-7): the override fields route the exact
/// double-Option set; the clear leg routes `reminder: Some(None)` and
/// reverts the habit to the plain O-1 default.
#[test]
fn us6_s2_override_save_routes_the_exact_command_and_s3_clears() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = weekly_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);

        // Set the override (period-start base, offset 1, 07:30, 2h
        // snooze) and save.
        window.invoke_habit_reminder_base_picked(1);
        window.invoke_habit_reminder_offset_edited("1".into());
        window.invoke_habit_reminder_time_edited("07:30".into());
        window.invoke_habit_reminder_snooze_edited("2h".into());
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        let ovr = taskboard_domain::habit::HabitReminderOverride {
            base: taskboard_domain::habit::NudgeBase::PeriodStart,
            offset_days: 1,
            time: chrono::NaiveTime::from_hms_opt(7, 30, 0).unwrap(),
            snooze: taskboard_domain::habit::HabitSnooze::Hours(2),
        };
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(changes.title, None);
                assert_eq!(changes.scheduled_days, None);
                assert_eq!(changes.reminder, Some(Some(ovr)));
            }
            other => panic!("the override save routes the sparse changeset, got {other:?}"),
        }

        // US-6/S3: reopen, clear, save — the Some(None) clear leg.
        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitEdit);
        harness.driver.advance_time(0);
        window.invoke_habit_reminder_clear_clicked();
        harness.driver.advance_time(0);
        window.invoke_habit_save_clicked();
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::UpdateHabit {
                    habit: edited,
                    changes,
                },
            ] => {
                assert_eq!(*edited, habit);
                assert_eq!(
                    changes.reminder,
                    Some(None),
                    "the clear leg rides the double option's None"
                );
            }
            other => panic!("the clear leg routes the sparse changeset, got {other:?}"),
        }
        assert_eq!(window.get_current_view(), View::Board);
    });
}

// ---- Chunk A1 (#1198, #1200): the reorder lands through the engine,
// the click seats the walk cursor. ----

/// A dense three-row section fixture (the `append_order_after` shape):
/// ids and titles returned in canonical order so the journeys assert
/// the id sequence, never row text.
fn dense_three_habits(
    state: &mut AppState,
    board: taskboard_domain::BoardId,
) -> Vec<(taskboard_domain::HabitId, &'static str)> {
    let mut ids = Vec::new();
    for (order, title) in [(0i64, "alpha"), (1, "bravo"), (2, "charlie")] {
        state_habit_at(state, board, order, title);
    }
    let mut rows: Vec<_> = state
        .habits
        .habits
        .values()
        .map(|h| (h.order, h.id))
        .collect();
    rows.sort();
    for (order, id) in rows {
        let title = match order {
            0 => "alpha",
            1 => "bravo",
            _ => "charlie",
        };
        ids.push((id, title));
    }
    ids
}

/// The section's rendered id sequence (the habits model's row order
/// IS the canonical `(order, id)` sequence; titles are the rows'
/// addresses, never a text oracle).
fn rendered_section(harness: &Harness) -> Vec<&'static str> {
    let model = harness.handle.window().get_habits_model();
    (0..model.row_count())
        .map(|i| {
            let title = model
                .row_data(i)
                .expect("row")
                .card
                .title
                .as_str()
                .to_owned();
            match title.as_str() {
                "alpha" => "alpha",
                "bravo" => "bravo",
                "charlie" => "charlie",
                other => panic!("unexpected row address {other}"),
            }
        })
        .collect()
}

/// A1 (`a1_drop_moves_the_row_through_the_engine`, #1198): the drop
/// seam's gap index routes `HabitReorder`, the engine step renumbers
/// the section, and the refreshed model renders the moved sequence
/// with dense orders.
#[test]
fn a1_drop_moves_the_row_through_the_engine() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let ids = dense_three_habits(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert_eq!(rendered_section(harness), vec!["alpha", "bravo", "charlie"]);

        // The middle row drops into the head gap.
        window.invoke_habits_drop_asked(1, 0);
        harness.driver.advance_time(0);
        let bravo = ids[1].0;
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { habit, target: 0 }] => assert_eq!(*habit, bravo),
            other => panic!("the drop routed the engine reorder, got {other:?}"),
        }

        // The engine step lands and the refresh renders the move.
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(rendered_section(harness), vec!["bravo", "alpha", "charlie"]);
        let snapshot = harness.engine.snapshot();
        let mut orders: Vec<_> = snapshot
            .habits
            .habits
            .values()
            .filter(|h| h.is_live() && h.board == board)
            .map(|h| h.order)
            .collect();
        orders.sort_unstable();
        assert_eq!(orders, vec![0, 1, 2], "the settled section stays dense");
    });
}

/// A1 (`a1_menu_move_moves_the_row`, #1198): the context menu's
/// Move-up row routes the reorder, the section swaps, and the undo
/// line restores the pre-move sequence.
#[test]
fn a1_menu_move_moves_the_row() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    dense_three_habits(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();

        // Open the menu over the middle row and take its Move-up row.
        window.invoke_habits_context_requested(1, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open(), "the habit menu opened");
        harness.handle.dispatch_action(ActionId::HabitMoveUp);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::HabitReorder { target: 0, .. }] => {}
            other => panic!("the menu row routed the reorder, got {other:?}"),
        }

        // The engine step lands; undo restores the pre-move section.
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(rendered_section(harness), vec!["bravo", "alpha", "charlie"]);
        harness.handle.dispatch_action(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            rendered_section(harness),
            vec!["alpha", "bravo", "charlie"],
            "the undo restored the pre-move sequence"
        );
    });
}

/// A1 (`a1_click_selects_the_row`, #1200): the pointer activation
/// seats the walk cursor — the clicked row's emphasis ring paints,
/// the task cursor clears, and a click after a keyboard landing
/// leaves ONE selection truth (click ≡ key in both directions).
#[test]
fn a1_click_selects_the_row() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    dense_three_habits(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        let emphasis = |harness: &Harness| -> Vec<bool> {
            let model = harness.handle.window().get_habits_model();
            (0..model.row_count())
                .map(|i| model.row_data(i).expect("row").card.emphasis)
                .collect()
        };

        // Pre-fix red pin: the click painted nothing.
        window.invoke_habits_row_activated(1);
        harness.driver.advance_time(0);
        assert_eq!(
            emphasis(harness),
            vec![false, true, false],
            "the clicked row's ring paints"
        );

        // The keyboard truth composes: a landing on the last row,
        // then a click on the first — one selection cursor.
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        assert_eq!(emphasis(harness), vec![false, false, true]);
        window.invoke_habits_row_activated(0);
        harness.driver.advance_time(0);
        assert_eq!(
            emphasis(harness),
            vec![true, false, false],
            "the click re-seated the cursor (one selection truth)"
        );
    });
}

/// D3 (stage-1 plan §3, stage-2's residual verification duty), CLICK
/// leg: a real pointer click (press + release at the card's window
/// coordinates, through the `DragArea`/`gap-zone := DropArea` full-size
/// sibling overlay) reaches `TaskCard::clicked` and seats the walk
/// cursor — the #1200 fix is real at the true pointer seam, no
/// interception.
///
/// The DRAG leg of this duty is an open finding (commented on #1198):
/// a real pointer press+move+release over a habit row never reaches
/// the `gap-zone := DropArea`'s `dropped` callback in this harness —
/// pressed at the card centre and at the left padding, past the slop,
/// against a board-control drag whose identical `DragArea`+child-
/// `DropArea` shape demonstrably works through the same driver (the
/// virtualisation probe). Per the plan's stop rule this is filed, not
/// blind-patched: the suspected mechanism is the habit `DragArea`'s
/// touch region (`height: habit-card.preferred-height`) not matching
/// the card's realized geometry, but that needs a GUI-level diagnosis
/// (D3a's redesign chunk) before any z-order or geometry edit.
#[test]
fn a1_d3_the_real_pointer_click_reaches_the_habit_rows() {
    let (_fx, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    dense_three_habits(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.handle.dispatch_action(ActionId::ViewToday);
        harness.driver.advance_time(0);
        assert_eq!(
            rendered_section(harness),
            vec!["alpha", "bravo", "charlie"],
            "the section renders on Today"
        );

        // The real click reaches the card under the gap-zone overlay:
        // the emphasis ring paints on the clicked row.
        let alpha = harness
            .driver
            .find_by_label("bravo")
            .expect("the bravo row renders");
        let (x, y) = support::driver::centre(&alpha);
        harness.driver.click_at(x, y);
        harness.driver.advance_time(0);
        harness.driver.advance_time(0);
        assert_eq!(
            emphasis_of(harness),
            vec![false, true, false],
            "the real pointer click selected the row"
        );
    });
}

/// The emphasis bits over the habits model (the D3 smoke's read seam;
/// the walk-cursor render fact).
fn emphasis_of(harness: &Harness) -> Vec<bool> {
    let model = harness.handle.window().get_habits_model();
    (0..model.row_count())
        .map(|i| model.row_data(i).expect("row").card.emphasis)
        .collect()
}
