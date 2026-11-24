// SPDX-License-Identifier: AGPL-3.0-only

//! The habit time tracking journeys (the 2026-11-24 habit-tracking UX
//! journey plan's §8-nominated evidence file): US-30's row play
//! control, US-31's menu row + chord parity, US-32's running display.
//! Assertion discipline is the journeys suite's: typed commands at the
//! fake-engine boundary, never text oracles (the elapsed chip is
//! asserted structurally — a non-empty derived phrase rides the row
//! model, its wording is the stage-3 walkthrough's).

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{AppState, HabitTimeEntry, StateCommand};
use taskboard_ui_slint::KeyboardState;
use taskboard_ui_slint::actions::ActionId;

fn keyboard(harness: &Harness) -> KeyboardState<'_> {
    harness.handle.window().global::<KeyboardState>()
}

/// The journeys' habit fixture (the habit-family file's twin): one
/// live daily habit (quota 2) on the fixture board.
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

/// Seeds an open habit time entry (the running timer) on `habit`.
fn state_open_entry(state: &mut AppState, habit: taskboard_domain::HabitId) {
    let now = chrono::Utc::now();
    let entry = HabitTimeEntry {
        id: taskboard_domain::ids::HabitTimeEntryId::from_uuid(uuid::Uuid::now_v7()),
        habit,
        habit_title: "water".to_owned(),
        start: now - chrono::TimeDelta::try_minutes(5).unwrap(),
        end: None,
        created_at: now - chrono::TimeDelta::try_minutes(5).unwrap(),
        updated_at: now - chrono::TimeDelta::try_minutes(5).unwrap(),
        by: None,
    };
    state.habits.time_log.insert(entry.id, entry);
}

/// US-30/S1 (W-1, AC-1): pressing the row's play control routes the
/// exact `StateCommand::StartHabitTracking` over the row (the dead
/// affordance's repair — the control is live).
#[test]
fn us30_s1_play_control_routes_habit_track_toggle() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_track_asked(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::StartHabitTracking { .. }] => {}
            other => panic!("the play control routes StartHabitTracking, got {other:?}"),
        }
    });
}

/// US-31/S1 (W-2, AC-2): the habit context menu carries the tracking
/// row, and activating the same route dispatches exactly what the play
/// control dispatches (click ≡ menu — one semantics, one seam).
#[test]
fn us31_s1_menu_row_routes_the_same_toggle() {
    let (_fixture, mut state) = fixture::journey_state();
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
        assert!(
            actions.contains(&"habit.track-toggle".to_owned()),
            "the tracking row renders in {actions:?}"
        );
        // The menu's rows route `ActionId`s (W-2: no new seam) — the
        // same route the play control speaks dispatches the same
        // command over the armed subject.
        harness
            .handle
            .dispatch_action(taskboard_ui_slint::actions::ActionId::HabitTrackToggle);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::StartHabitTracking { .. }] => {}
            other => panic!("the menu row routes the same toggle, got {other:?}"),
        }
    });
}

/// US-31/S2 (W-3, AC-1): the chord over the walk's landing routes the
/// same toggle — the keyboard path mints the one-shot habit subject
/// exactly as the row seam does (the skip chord's shape).
#[test]
fn us31_s2_chord_routes_the_same_toggle() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    state_habit(&mut state, board);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        // The walk enters the habits section's row (the last Today
        // section) — no pointer fact, the keyboard path alone.
        harness.handle.dispatch_action(ActionId::NavLastItem);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::HabitTrackToggle);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::StartHabitTracking { .. }] => {}
            other => panic!("the chord routes the same toggle, got {other:?}"),
        }
    });
}

/// US-31/S3 (W-1, AC-3 of US-30): with this habit's open entry
/// running, the same toggle routes `StopHabitTracking` — the state
/// half lives in the router arm, the idempotent `NoOps` own the rest.
#[test]
fn us31_s3_toggle_stop_dispatches_stop() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    state_open_entry(&mut state, habit);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_track_asked(0);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::StopHabitTracking { .. }] => {}
            other => panic!("the toggle routes StopHabitTracking, got {other:?}"),
        }
    });
}

/// US-32/S1 (W-4/W-5, AC-1): with an open habit entry, the row's
/// `track_state` is `RunningHere` and the elapsed chip derives from
/// the open entry's start (display derives at read — structural
/// assertion, never chip-wording text).
#[test]
fn us32_s1_running_chip_derives_from_open_entry() {
    let (_fixture, mut state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = state_habit(&mut state, board);
    state_open_entry(&mut state, habit);
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        let row = window.get_habits_model().row_data(0).expect("row");
        assert!(
            row.card.track_state == taskboard_ui_slint::TrackCardState::RunningHere,
            "the fold reports RunningHere while the entry is open"
        );
        // The elapsed chip mounts with a derived (non-empty) phrase.
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
            chips.iter().any(|label| !label.is_empty()) && chips.len() >= 2,
            "the elapsed chip derives from the open entry, chips {chips:?}"
        );
        let _ = habit;
    });
}
