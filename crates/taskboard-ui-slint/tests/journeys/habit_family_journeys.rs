// SPDX-License-Identifier: AGPL-3.0-only

//! The habit family's HF1 journeys (skip + pause, #955/#958; the
//! journey plan's §6 wiring rows W-1..W-4): the skip menu row and
//! keyboard route, the typed skip chip, the pause editor's
//! mount/commit/cancel grammar, and the paused badge. Assertion
//! discipline is the journeys suite's: terminal state + the typed
//! commands that reached the fake-engine boundary; labels are
//! addresses, never text oracles.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{AppState, StateCommand};
use taskboard_ui_slint::KeyboardState;

fn keyboard(harness: &Harness) -> KeyboardState<'_> {
    harness.handle.window().global::<KeyboardState>()
}

/// The journeys' habit fixture (the lib fixture's twin): one live
/// daily habit on the fixture board.
fn state_habit(state: &mut AppState, board: taskboard_domain::BoardId) {
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
        clocks: taskboard_domain::habit::HabitClocks::all(chrono::Utc::now()),
        continued_from: None,
        deleted: false,
        paused_until: None,
        by: None,
    };
    state.habits.habits.insert(habit.id, habit);
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
    let today = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch");
    let _ = today;
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
