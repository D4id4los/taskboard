//! The habit family's journeys (HF1's skip + pause, #955/#958, the
//! journey plan's §6 wiring rows W-1..W-4; HF2's global-undo leg of a
//! logged completion, US-3/S2, and the inline quota popover,
//! US-4/S1-S3, the 2026-11-21 habit-family UX plan's W-5/W-12
//! evidence). Assertion discipline is the journeys suite's: terminal
//! state + the typed commands that reached the fake-engine boundary;
//! labels are addresses, never text oracles.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::KeyboardState;
use taskboard_ui_slint::actions::ActionId;

fn keyboard(harness: &Harness) -> KeyboardState<'_> {
    harness.handle.window().global::<KeyboardState>()
}

/// The journeys' habit fixture (the lib fixture's twin): one live
/// daily habit (quota 2) on the fixture board.
fn state_habit(
    state: &mut AppState,
    board: taskboard_domain::BoardId,
) -> taskboard_domain::HabitId {
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "water".to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(2).unwrap(),
        ),
        order: 0,
        anchor_time: None,
        anchor_day: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        paused_until: None,
        by: None,
    };
    let id = habit.id;
    state.habits.habits.insert(habit.id, habit);
    id
}

/// The habit fixture's paused twin (US-2/S4): the same habit with a
/// standing pause through the far future.
fn state_paused_habit(state: &mut AppState, board: taskboard_domain::BoardId) {
    state_habit(state, board);
    let habit = state.habits.habits.values_mut().next().expect("one habit");
    habit.paused_until = Some(chrono::NaiveDate::from_ymd_opt(2999, 12, 31).expect("valid date"));
}

/// US-1/S1 (W-1): the habit context menu's page carries the Skip row
/// and the pause/resume pair beside the pre-existing verbs — the menu
/// arms the row subject (the open is the arm).
#[test]
fn us1_s1_skip_menu_row_arms_the_subject() {
    let (_fixture, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_context_requested(0, 120.0, 120.0);
        harness.driver.advance_time(0);
        assert!(keyboard(harness).get_menu_open(), "the menu opened");
        let entries = keyboard(harness).get_menu_entries();
        let actions: Vec<String> = (0..entries.row_count())
            .map(|i| entries.row_data(i).expect("row").action.to_string())
            .collect();
        for verb in ["habit.skip", "habit.pause", "habit.resume"] {
            assert!(
                actions.contains(&verb.to_string()),
                "the HF1 row renders: {verb} in {actions:?}"
            );
        }
    });
}

/// US-1/S2 (W-1, AC-1): activating the strip's skip affordance routes
/// the exact `StateCommand::SkipHabit` over the row (the one-shot
/// habit subject).
#[test]
fn us1_s2_skip_dispatches_the_exact_command() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_skip_asked(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::SkipHabit { .. }] => {}
            other => panic!("the skip routes the exact command, got {other:?}"),
        }
    });
}

/// US-1/S2 (W-11, the O-8 parity leg): the Today-context chord over
/// the walk's landing routes the same `StateCommand::SkipHabit` — the
/// keyboard path mints the one-shot habit subject exactly as the row
/// seam does (no click or menu fact needed).
#[test]
fn us1_s2_skip_key_routes_over_the_row_subject() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        // The walk enters the habits section's row (the last Today
        // section) — no pointer fact, the keyboard path alone.
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::HabitSkip);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::SkipHabit { .. }] => {}
            other => panic!("the chord routes the exact command, got {other:?}"),
        }
    });
}

/// US-1/S3 (W-2, AC-5): a skipped window renders the typed skip chip
/// on the row (display only — no command).
#[test]
fn us1_s3_row_shows_the_typed_skip_state() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    // Mark the current window skipped in the seed (the engine's
    // typed mark, read back through the projection).
    let habit = *state.habits.habits.keys().next().expect("one habit");
    let mut log = std::collections::BTreeMap::new();
    let window = taskboard_domain::WindowStart::of_period(
        taskboard_domain::PeriodKind::Day,
        chrono::Utc::now().date_naive(),
    );
    log.insert(
        taskboard_domain::HabitLogKey { habit, window },
        taskboard_domain::HabitLogEntry {
            count: 0,
            clock: chrono::Utc::now(),
            by: None,
            skip: true,
        },
    );
    state.habits.log = log;
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let row = harness
            .handle
            .window()
            .get_habits_model()
            .row_data(0)
            .expect("row");
        assert!(row.skip, "the typed skip state rides the row model");
        let chips: Vec<String> = (0..row.card.decorations.row_count())
            .map(|i| {
                row.card
                    .decorations
                    .row_data(i)
                    .expect("chip")
                    .label
                    .to_string()
            })
            .collect();
        assert!(
            chips.iter().any(|c| c == "skipped"),
            "the skip chip renders: {chips:?}"
        );
    });
}

/// US-2/S1 (W-3, AC-1): the pause editor mounts over the row and the
/// cancel writes nothing (the destructive-role dismissal law — zero
/// commands at the boundary).
#[test]
fn us2_s1_pause_editor_mounts_and_cancels_clean() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        assert!(window.get_habits_pause_open(), "the editor mounted");
        window.invoke_habits_pause_cancelled();
        harness.driver.advance_time(0);
        assert!(
            !window.get_habits_pause_open(),
            "the cancel dismissed the editor"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the cancel dispatched nothing"
        );
    });
}

/// US-2/S2 (W-3, AC-1): committing the typed through-date routes the
/// exact `StateCommand::PauseHabit { until: Some(date) }`.
#[test]
fn us2_s2_pause_commit_routes_the_exact_command() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        window.invoke_habits_pause_until_picked("2999-12-31".into());
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::PauseHabit {
                    until: Some(_until),
                    ..
                },
            ] => {}
            other => panic!("the commit routes the exact command, got {other:?}"),
        }
    });
}

/// US-2/S2 (W-3, the escape leg): a through-date before today is the
/// typed draft refusal — the notice arms, nothing reaches the
/// boundary, and the editor stays open for the correction.
#[test]
fn us2_s2_pause_past_date_refuses_and_stays_open() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_pause_asked(0);
        harness.driver.advance_time(0);
        window.invoke_habits_pause_until_picked("2000-01-01".into());
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the refused draft routes nothing"
        );
        assert!(
            window.get_habits_pause_open(),
            "the editor stays open for the correction"
        );
        assert!(
            window.get_notice_visible(),
            "the typed invalid-pause refusal surfaced"
        );
    });
}

/// US-2/S3 (W-3, AC-4): the resume affordance routes the `None` clear
/// leg (badge hygiene; the `PauseSeries` posture).
#[test]
fn us2_s3_resume_routes_the_none_clear_leg() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_paused_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_resume_asked(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::PauseHabit { until: None, .. }] => {}
            other => panic!("the resume routes the clear leg, got {other:?}"),
        }
    });
}

/// US-2/S4 (W-4, AC-3): a paused habit's row renders the paused badge
/// with the through-date (display only).
#[test]
fn us2_s4_row_shows_the_paused_state() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_paused_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let row = harness
            .handle
            .window()
            .get_habits_model()
            .row_data(0)
            .expect("row");
        assert!(row.paused, "the paused bit rides the row model");
        let chips: Vec<String> = (0..row.card.decorations.row_count())
            .map(|i| {
                row.card
                    .decorations
                    .row_data(i)
                    .expect("chip")
                    .label
                    .to_string()
            })
            .collect();
        assert!(
            chips.iter().any(|c| c.starts_with("paused through")),
            "the paused badge renders the through-date: {chips:?}"
        );
    });
}

/// R-3 (the kiosk gate): the skip/pause/resume rows join the habit
/// menu's greyed set (visible-disabled, never withheld), the pause
/// editor never mounts, and the action dispatch is structurally inert
/// (the H4 editor gate's grammar).
#[test]
fn kiosk_gates_the_hf1_affordances_disabled() {
    let (_fixture, mut state) = fixture::golden_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape_with_settings(
        state,
        taskboard_ui_slint::UiSettings {
            kiosk: true,
            ..taskboard_ui_slint::UiSettings::default()
        },
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            // The menu opens (reading is passive) and the HF1 rows grey.
            window.invoke_habits_context_requested(0, 120.0, 120.0);
            harness.driver.advance_time(0);
            let entries = keyboard(harness).get_menu_entries();
            let rows: Vec<_> = (0..entries.row_count())
                .map(|i| entries.row_data(i).expect("row"))
                .collect();
            for action in ["habit.skip", "habit.pause", "habit.resume"] {
                let row = rows
                    .iter()
                    .find(|r| r.action == action)
                    .unwrap_or_else(|| panic!("{action} renders, greyed"));
                assert!(!row.enabled, "{action} greys on kiosk");
            }
            // The action dispatch is structurally inert: the pause
            // editor never mounts, nothing reaches the boundary.
            harness
                .handle
                .dispatch_action(taskboard_ui_slint::actions::ActionId::HabitPause);
            harness.driver.advance_time(0);
            assert!(
                !window.get_habits_pause_open(),
                "the pause editor never mounts on kiosk"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the kiosk dispatch sends nothing"
            );
        },
    );
}

/// US-3/S2 (R-7, #956): the logged completion reverts through the
/// GLOBAL undo line — the pre-existing `AppUndo` verb routes
/// `StateCommand::Undo` after the log; no parallel undo path exists.
#[test]
fn us3_s2_undo_reverts_the_logged_completion() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_log_clicked(0);
        harness.driver.advance_time(0);
        // The global undo verb (the pre-existing affordance, W-12's
        // changed classification): the same line every other verb
        // undoes through.
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::LogHabit { habit }, StateCommand::Undo]);
    });
}

/// US-4/S1 (AC-1, #961): clicking the row's progress phrase mounts
/// the quota popover seeded AT the current quota, over the phrase as
/// the anchor; the kind phrase is the row's own (locked picker).
#[test]
fn us4_s1_quota_popover_mounts_from_the_row_phrase() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            harness.driver.find_by_label("0/2").is_some(),
            "the row renders the clickable progress phrase"
        );
        window.invoke_habits_quota_edit_asked(0);
        harness.driver.advance_time(0);
        assert!(
            window.get_habits_quota_open(),
            "the popover mounted over the phrase"
        );
        assert_eq!(window.get_habits_quota_count(), 2, "seeded at the quota");
        assert!(window.get_habits_quota_valid(), "the seeded draft is valid");
    });
}

/// US-4/S2 (AC-2, #961): the stepper's commit routes the EXISTING
/// `UpdateHabit` with `HabitChanges { target: Some(..) }` — the
/// period kind locked to the stored row's kind (no new command).
#[test]
fn us4_s2_quota_commit_routes_update_habit() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        {
            let window = harness.handle.window();
            window.invoke_habits_quota_edit_asked(0);
            window.invoke_habits_quota_stepped(1);
            assert_eq!(window.get_habits_quota_count(), 3, "the rung stepped");
            window.invoke_habits_quota_committed();
        }
        harness.driver.advance_time(0);
        let expected = [StateCommand::UpdateHabit {
            habit,
            changes: taskboard_domain::messages::HabitChanges {
                target: Some(taskboard_domain::HabitTarget::PerDay(
                    taskboard_domain::NonZeroCount::new(3).unwrap(),
                )),
                ..Default::default()
            },
        }];
        harness.assert_commands(&expected);
        assert!(
            !harness.handle.window().get_habits_quota_open(),
            "the popover closed on commit"
        );
    });
}

/// US-4/S2's escape failure (AC-3, #961): a zero draft blocks the
/// commit — the typed invalid-draft state renders, no route fires,
/// the popover stays open (the rungs are the recovery).
#[test]
fn us4_s3_zero_draft_blocks_commit() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_quota_edit_asked(0);
        harness.driver.advance_time(0);
        // 2 → 1 → 0: the floor of the stepper is the invalid draft.
        window.invoke_habits_quota_stepped(-1);
        window.invoke_habits_quota_stepped(-1);
        harness.driver.advance_time(0);
        assert_eq!(window.get_habits_quota_count(), 0);
        assert!(
            !window.get_habits_quota_valid(),
            "the zero draft is the typed invalid state"
        );
        window.invoke_habits_quota_committed();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the zero draft never routes"
        );
        assert!(
            window.get_habits_quota_open(),
            "commit blocked keeps the popover open"
        );
        // The escape rung (the host's dismissal funnel): Escape/
        // outside lands the disarm with zero state change.
        window.invoke_habits_quota_dismissed();
        harness.driver.advance_time(0);
        assert!(!window.get_habits_quota_open());
    });
}

/// The key twin (O-8 parity, W-11): the focused row's
/// `HabitQuotaEdit` key routes the same opener the click routes.
#[test]
fn us4_s1_quota_key_routes_over_the_focused_row() {
    let (_fx, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        {
            let window = harness.handle.window();
            window.invoke_habits_row_activated(0);
        }
        harness.driver.advance_time(0);
        harness.press(ActionId::HabitQuotaEdit);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_habits_quota_open(),
            "the key routes the same opener (click ≡ key)"
        );
    });
}
