//! The habit family's journeys (HF1's skip + pause, #955/#958, the
//! journey plan's §6 wiring rows W-1..W-4; HF2's global-undo leg of a
//! logged completion, US-3/S2, and the inline quota popover,
//! US-4/S1-S3, the 2026-11-21 habit-family UX plan's W-5/W-12
//! evidence; HF3's history dialog, streak badge and statistics,
//! W-8/W-9/W-10, US-7/US-8/US-9). Assertion discipline is the
//! journeys suite's: terminal state + the typed commands that reached
//! the fake-engine boundary; labels are addresses, never text oracles.

// SPDX-License-Identifier: AGPL-3.0-only

use crate::support;

use chrono::Datelike as _;
use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_domain::{AppState, HabitLogEntry, HabitLogKey, StateCommand};
use taskboard_ui_slint::KeyboardState;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::keymap::stroke::parse;

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

/// The create form's target picker radio stop (US-1/S3): the label
/// addresses the container's shared string too, so the radio is
/// picked by role — the element carrying the checked bit and the
/// pick's default action (the AT ≡ pick seam).
fn segment_radio(harness: &Harness, label: &str) -> support::driver::Element {
    harness
        .driver
        .find_all_by_label(label)
        .into_iter()
        .find(support::driver::Element::is_radio_button)
        .expect("the segmented stop is addressable")
}

/// The stop's checked bit — the typed selected fact (the element
/// read, not the r2 model fallback; backend-testing 1.18 exposes the
/// read).
fn segment_checked(harness: &Harness, label: &str) -> bool {
    segment_radio(harness, label).is_checked()
}

/// US-1/S2 (W-1, the r7 create plan's retro bound-test): the title
/// draft echoes in the field's text model — a form-local edit that
/// dispatches nothing — and Return commits the draft as the exact
/// `CreateHabit` (the title field's commit seam, the create-row
/// precedent).
#[test]
fn us1_s2_create_title_draft_echoes_and_commits_on_return() {
    let (_fixture, state) = fixture::journey_state();
    let board = *state.boards.keys().next().expect("fixture board");
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.driver.advance_time(0);
        harness.handle.window().invoke_habits_create_asked();
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_habits_create_open(),
            "the form armed"
        );
        harness.assert_commands(&[]);

        // The title field: the a11y label is the address; the value
        // (never the label) is the echo oracle.
        let field = harness
            .driver
            .find_by_label("Habit title")
            .filter(support::driver::Element::is_text_input)
            .expect("the armed form's title field is addressable");
        let (x, y) = support::driver::centre(&field);
        harness.driver.click_at(x, y);
        harness.driver.advance_time(0);
        harness.type_text("Chamomile");
        harness.driver.advance_time(0);
        assert_eq!(
            field.value().as_deref(),
            Some("Chamomile"),
            "the draft echoes in the field's text model"
        );
        harness.assert_commands(&[]);

        // Return commits: the exact command over the section's board,
        // the default quota and target arming (the untouched fields).
        harness.stroke(parse("Return").expect("Return parses"));
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [
                StateCommand::CreateHabit {
                    board: created_board,
                    title,
                    target,
                },
            ] => {
                assert_eq!(created_board, &board, "the section's board");
                assert_eq!(title, "Chamomile");
                assert_eq!(
                    target,
                    &taskboard_domain::HabitTarget::PerDay(
                        taskboard_domain::NonZeroCount::new(1).unwrap(),
                    ),
                    "the untouched quota and target arm their defaults"
                );
            }
            other => panic!("Return commits the draft, got {other:?}"),
        }
        assert!(
            !harness.handle.window().get_habits_create_open(),
            "the commit disarms the form"
        );
    });
}

/// US-1/S3 (W-1, the r7 create plan's retro bound-test): picking a
/// target variant moves the control's typed selected fact (exactly
/// one checked stop) and dispatches nothing; bare Escape disarms and
/// discards the draft (the alternative-flow leg).
#[test]
fn us1_s3_create_target_pick_shows_via_selected_index() {
    let (_fixture, state) = fixture::journey_state();
    support::harness::run_journey_no_escape(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        harness.driver.advance_time(0);
        harness.handle.window().invoke_habits_create_asked();
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_habits_create_open(),
            "the form armed"
        );

        assert!(segment_checked(harness, "Per day"), "the default selects");
        assert!(
            !segment_checked(harness, "Per week") && !segment_checked(harness, "Per month"),
            "exactly one stop is selected"
        );

        segment_radio(harness, "Per week").invoke_default_action();
        harness.driver.advance_time(0);
        assert!(
            segment_checked(harness, "Per week"),
            "the pick shows via the checked stop"
        );
        assert!(
            !segment_checked(harness, "Per day") && !segment_checked(harness, "Per month"),
            "the selection moved off the default"
        );
        harness.assert_commands(&[]);

        // The alternative-flow leg: Escape disarms, the draft is
        // discarded, the boundary stays silent.
        harness.stroke(parse("Escape").expect("Escape parses"));
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_habits_create_open(),
            "Escape disarmed the form"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the pick and the disarm dispatched nothing"
        );
    });
}

/// The journeys' settings: the render clock frozen at the fixture's
/// pinned instant (the clock law's committed-artifact anchor — the
/// `habits_journey` twin), the desktop posture.
fn pinned_settings(kiosk: bool) -> UiSettings {
    UiSettings {
        kiosk,
        clock: Some(fixture::PINNED_NOW),
        ..UiSettings::default()
    }
}

/// The journeys' habit fixture: one live daily habit (quota 2) with
/// a log over the past days — a met window two days back, a missed
/// one three days back (both grace-closed for a daily kind), the
/// current window pending.
fn state_habit_with_history(state: &mut AppState) -> taskboard_domain::HabitId {
    let board = *state.boards.keys().next().expect("fixture board");
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
        paused_until: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: false,
        by: None,
    };
    let id = habit.id;
    let today = fixture::PINNED_NOW.date_naive();
    let put = |log: &mut std::collections::BTreeMap<HabitLogKey, HabitLogEntry>,
               window: chrono::NaiveDate,
               count: u32| {
        log.insert(
            HabitLogKey {
                habit: id,
                window: taskboard_domain::habit::WindowStart::of_period(
                    taskboard_domain::PeriodKind::Day,
                    window,
                ),
            },
            HabitLogEntry {
                count,
                clock: fixture::PINNED_NOW,
                by: None,
                skip: false,
            },
        );
    };
    let mut log = std::collections::BTreeMap::new();
    let met = today - chrono::Duration::days(2);
    let missed = today - chrono::Duration::days(3);
    put(&mut log, met, 2);
    put(&mut log, missed, 0);
    // An October row: the pager's back edge lives in a prior month.
    put(
        &mut log,
        chrono::NaiveDate::from_ymd_opt(2026, 10, 30).expect("date"),
        2,
    );
    state.habits.log = log;
    state.habits.habits.insert(id, habit);
    id
}

/// Opens the habit context menu and walks to the History row (the
/// menu's rank-245 `habit.history` entry), then invokes it.
fn open_history_from_menu(harness: &mut Harness) {
    let window = harness.handle.window();
    window.invoke_habits_context_requested(0, 120.0, 120.0);
    harness.driver.advance_time(0);
    assert!(keyboard(harness).get_menu_open(), "the menu opened");
    let entries = keyboard(harness).get_menu_entries();
    let history_index = (0..entries.row_count())
        .find(|i| entries.row_data(*i).expect("row").action.as_str() == "habit.history")
        .expect("the History row renders on the habit page");
    for _ in 0..history_index {
        harness.handle.dispatch_action(ActionId::MenuNext);
    }
    harness.handle.dispatch_action(ActionId::MenuInvoke);
    harness.driver.advance_time(0);
}

/// US-7/S1 (AC-1): the context menu's History row mounts the
/// read-role dialog — the month grid model carries the visible
/// month's typed cells, and nothing ever reached the command
/// boundary (the projection is zero-command per US-7/S2–S4).
#[test]
fn us7_s1_history_dialog_mounts_from_the_menu() {
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let window = harness.handle.window();
            assert!(
                window.get_habit_history_open(),
                "the history dialog mounted from the menu row"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the mount dispatches nothing (pure projection)"
            );
            // The grid model: the visible (current) month's daily
            // windows, typed.
            let model = window.get_habit_history();
            assert!(
                model.cells.row_count() >= 28,
                "a month's daily windows render as cells"
            );
        },
    );
}

/// US-7/S2 (AC-1/AC-2): the grid cells color by the typed state —
/// the met, missed, and pending windows classify per the walk's
/// semantics; the D-H12 class never surfaces as a miss on the
/// current-kind grid.
#[test]
fn us7_s2_history_grid_renders_the_typed_window_states() {
    use taskboard_ui_slint::HabitCellState as S;
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let window = harness.handle.window();
            assert!(window.get_habit_history_open());
            let model = window.get_habit_history();
            let today = fixture::PINNED_NOW.date_naive();
            let met_day = (today - chrono::Duration::days(2)).day().to_string();
            let missed_day = (today - chrono::Duration::days(3)).day().to_string();
            let today_label = today.day().to_string();
            let mut saw_met = false;
            let mut saw_missed = false;
            let mut saw_pending = false;
            for index in 0..model.cells.row_count() {
                let cell = model.cells.row_data(index).expect("cell");
                match cell.state {
                    S::Met if cell.label.as_str() == met_day => {
                        assert_eq!(cell.count, 2);
                        saw_met = true;
                    }
                    S::Missed if cell.label.as_str() == missed_day => {
                        assert_eq!(cell.count, 0);
                        saw_missed = true;
                    }
                    S::Pending if cell.label.as_str() == today_label => {
                        saw_pending = true;
                    }
                    _ => {}
                }
            }
            assert!(saw_met, "the met window renders typed");
            assert!(
                saw_missed,
                "the grace-closed unmet window renders as the miss class"
            );
            assert!(saw_pending, "the current window renders pending");
        },
    );
}

/// US-7/S3 (AC-4): the pager clamps at the log's edges — back is
/// gated by the earliest row's month, forward by the current month;
/// no wrap, and the dialog stays open (pure display navigation).
#[test]
fn us7_s3_pager_clamps_at_the_edges() {
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let window = harness.handle.window();
            assert!(window.get_habit_history_open());
            let model = window.get_habit_history();
            // Current month: back possible (two rows behind), forward
            // impossible.
            assert!(model.can_page_back, "the rows' months page back");
            assert!(!model.can_page_forward, "the current month clamps forward");
            // Forward past the clamp is refused: the dialog holds, the
            // model's month label unchanged (the label is an address —
            // the clamp is the typed fact).
            window.invoke_habit_history_month_flipped(1);
            harness.driver.advance_time(0);
            assert!(
                window.get_habit_history_open(),
                "the clamp keeps the dialog"
            );
            // Back flips to the previous month and arms the forward side.
            window.invoke_habit_history_month_flipped(-1);
            harness.driver.advance_time(0);
            let flipped = window.get_habit_history();
            assert!(flipped.can_page_forward, "the previous month arms forward");
        },
    );
}

/// US-7/S4 (AC-3): the read-role close leaves zero state change —
/// the dialog unmounts, nothing dispatched, the engine untouched.
#[test]
fn us7_s4_close_leaves_zero_state_change() {
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let window = harness.handle.window();
            assert!(window.get_habit_history_open());
            let before = harness.engine.drain();
            window.invoke_habit_history_close_requested();
            harness.driver.advance_time(0);
            assert!(
                !window.get_habit_history_open(),
                "the read-role close unmounts the dialog"
            );
            assert!(
                harness.engine.drain().is_empty(),
                "the close dispatches nothing (zero state change)"
            );
            assert!(
                before.is_empty(),
                "the whole journey stayed projection-only"
            );
        },
    );
}

/// US-8/S1 (AC-1): the row renders the streak badge — the typed
/// streak value rides the decoration row (the Badge base widget's
/// model); the emoji slot is gone (the label is the value; the
/// wording is judged at walkthrough, never asserted).
#[test]
fn us8_s1_row_renders_the_streak_badge() {
    let (_fx, mut state) = fixture::golden_state();
    let id = state_habit_with_history(&mut state);
    // Give the current window a met count so the streak is >= 1.
    let today = fixture::PINNED_NOW.date_naive();
    state.habits.log.insert(
        HabitLogKey {
            habit: id,
            window: taskboard_domain::habit::WindowStart::of_period(
                taskboard_domain::PeriodKind::Day,
                today,
            ),
        },
        HabitLogEntry {
            count: 2,
            clock: fixture::PINNED_NOW,
            by: None,
            skip: false,
        },
    );
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            let row = harness
                .handle
                .window()
                .get_habits_model()
                .row_data(0)
                .expect("row");
            let mut decorations = Vec::new();
            for index in 0..row.card.decorations.row_count() {
                decorations.push(row.card.decorations.row_data(index).expect("decoration"));
            }
            let streaks: Vec<_> = decorations
                .iter()
                .filter(|d| d.label.as_str().chars().all(|c| c.is_ascii_digit()))
                .collect();
            assert_eq!(
                streaks.len(),
                1,
                "exactly one streak badge rides the row, got {decorations:?}"
            );
            let parsed: u32 = streaks[0].label.as_str().parse().expect("the streak value");
            assert!(
                parsed >= 1,
                "the badge carries the typed streak value (the met today + the met window back)"
            );
            assert!(
                decorations
                    .iter()
                    .all(|d| !d.label.as_str().contains('\u{1F525}')),
                "the emoji slot is retired"
            );
        },
    );
}

/// US-9/S1 (AC-1/AC-3): the statistics section renders the domain
/// fns' typed values — the populated history carries the met/
/// scheduled pair and the best streak; the empty twin carries the
/// typed empty state, never a 0 %.
#[test]
fn us9_s1_statistics_render_the_typed_values() {
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let section = harness.handle.window().get_habit_history().stats;
            assert!(
                !section.adherence_empty,
                "the populated history is not the empty state"
            );
            assert!(section.adherence_met >= 1, "the met window counts");
            assert!(section.adherence_scheduled >= section.adherence_met);
            assert_eq!(section.best_streak, 1, "the met run back of the pendings");
        },
    );
}

/// US-9/S1's empty twin (AC-3): a habit with no log rows renders the
/// typed empty state — adherence-empty, never a 0 % posture.
#[test]
fn us9_s1_empty_history_renders_the_typed_empty_state() {
    let (_fx, mut state) = fixture::golden_state();
    // The fixture without rows: the habit alone.
    let board = *state.boards.keys().next().expect("fixture board");
    let habit = taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: "empty".to_owned(),
        description: String::new(),
        target: taskboard_domain::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(1).unwrap(),
        ),
        order: 0,
        anchor_time: None,
        anchor_day: None,
        paused_until: None,
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(fixture::PINNED_NOW),
        continued_from: None,
        deleted: false,
        by: None,
    };
    state.habits.habits.insert(habit.id, habit);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(false),
        move |harness| {
            harness.driver.advance_time(0);
            open_history_from_menu(harness);
            let section = harness.handle.window().get_habit_history().stats;
            assert!(
                section.adherence_empty,
                "the empty history is the typed empty state, never 0 %"
            );
            assert_eq!(section.adherence_met, 0);
            assert_eq!(section.adherence_scheduled, 0);
            assert_eq!(section.best_streak, 0);
        },
    );
}

/// The kiosk gate (R-3): the history dialog is absent on the
/// appliance — the opener dispatch is structurally inert and the
/// mount bit never arms; the badge (W-9) still renders (display
/// only, the D-P13 posture).
#[test]
fn kiosk_history_dialog_is_absent_and_the_badge_renders() {
    let (_fx, mut state) = fixture::golden_state();
    state_habit_with_history(&mut state);
    support::harness::run_journey_no_escape_with_settings(
        state,
        pinned_settings(true),
        move |harness| {
            harness.driver.advance_time(0);
            let window = harness.handle.window();
            // The opener dispatch: structurally inert on kiosk.
            window.invoke_habit_editor_history_clicked();
            harness.driver.advance_time(0);
            assert!(
                !window.get_habit_history_open(),
                "the kiosk never mounts the history dialog (R-3)"
            );
            // The badge renders display-only.
            let row = window.get_habits_model().row_data(0).expect("row");
            let mut decorations = Vec::new();
            for index in 0..row.card.decorations.row_count() {
                decorations.push(row.card.decorations.row_data(index).expect("decoration"));
            }
            assert!(
                decorations
                    .iter()
                    .any(|d| d.label.as_str().chars().all(|c| c.is_ascii_digit())),
                "the streak badge renders on kiosk (display only)"
            );
        },
    );
}
