// SPDX-License-Identifier: AGPL-3.0-only

//! The today view's two-column wide arm's journeys (#1436, journey
//! plan 2026-11-25 `2026-11-25-sound-today-colour-journeys-today-two-
//! column-ux-plan.md`, accepted with the 2026-11-25 operator
//! amendment: the kiosk mount is NOT special-cased — the width test
//! alone rules every mount). Step-ID-named per the plan's §8 list.
//!
//! Assertion discipline: typed outcomes only — the `TodayLayoutFacts`
//! grid-arm fact, the section row sets (ids + counts), and the walk's
//! emphasis landing (which row kind carries the focus ring). No label
//! or status text is ever matched. Clock discipline: the fixtures'
//! wall-derived captions ride the harness clock only through the
//! pre-pinned today fixtures; the walk and the layout facts are
//! wall-free, so no sleep and no clock freezing is needed
//! (`advance_time(0)` instantiation steps only).

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::ComponentHandle as _;
use slint::Model as _;
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;
use taskboard_ui_slint::view_state::{KioskMode, KioskRotation};

/// The wide arm's test width: comfortably above the shared
/// breakpoint (1440 logical px — `Theme.layout-today-two-column-min-
/// width`, pinned equal to `EXPANSIVE_MIN_WIDTH`).
const WIDE: (u32, u32) = (1600, 900);
/// The narrow arm's test width: the regular band's interior, below
/// the breakpoint.
const NARROW: (u32, u32) = (1000, 900);

/// The today fixture for this suite: the two-board journey state with
/// its overdue rows, plus one completed task (the Done section's row)
/// and one live habit (the right column's rows) — every walkable row
/// kind present, several sections non-empty, two live boards (the A1
/// aggregation's "see = sum" posture).
fn grid_fixture() -> taskboard_domain::AppState {
    let (two, mut state, _walked) = fixture::today_overdue_state();
    let now = chrono::Utc::now();
    let _ = fixture::add_completed_task(
        &mut state,
        two.stack_c,
        "finished",
        now - chrono::TimeDelta::minutes(30),
        None,
        None,
    );
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
        scheduled_days: None,
        reminder: None,
        clocks: taskboard_domain::habit::HabitClocks::all(now),
        continued_from: None,
        deleted: false,
        paused_until: None,
        by: None,
    };
    state.habits.habits.insert(habit.id, habit);
    state
}

/// The static kiosk posture pinned to the Today rotation stage: the
/// rotation constructor demands ≥ 2 distinct modes, so Today leads
/// and the flip target sits behind a dwell the tests never advance
/// across.
fn kiosk_today_settings() -> UiSettings {
    UiSettings {
        kiosk: true,
        kiosk_rotation: Some(
            KioskRotation::new(
                vec![KioskMode::Today, KioskMode::Board],
                std::time::Duration::from_secs(3600),
            )
            .expect("two distinct modes mint"),
        ),
        ..UiSettings::default()
    }
}

fn grid_arm(mounted: &Harness) -> bool {
    mounted
        .handle
        .window()
        .global::<taskboard_ui_slint::TodayLayoutFacts>()
        .get_grid_arm()
}

/// The task rows' stable ids, flattened in walk order (section
/// display order outer, row order inner) — the US-17 AC-3 oracle and
/// the walk tests' left-column domain.
fn task_row_ids(mounted: &Harness) -> Vec<String> {
    let model = mounted.handle.window().get_today_model();
    let mut ids = Vec::new();
    for section in model.sections.iter() {
        let rows = section.rows;
        for row in rows.iter() {
            ids.push(row.id.to_string());
        }
    }
    ids
}

fn habit_count(mounted: &Harness) -> usize {
    mounted.handle.window().get_habits_model().iter().count()
}

/// The flat task-row index carrying the walk's emphasis ring, if any.
fn emphasized_task_row(mounted: &Harness) -> Option<usize> {
    let model = mounted.handle.window().get_today_model();
    let mut flat = 0;
    for section in model.sections.iter() {
        let rows = section.rows;
        for row in rows.iter() {
            if row.card.emphasis {
                return Some(flat);
            }
            flat += 1;
        }
    }
    None
}

fn emphasized_habit_row(mounted: &Harness) -> Option<usize> {
    mounted
        .handle
        .window()
        .get_habits_model()
        .iter()
        .position(|habit| habit.card.emphasis)
}

/// US-17/S1 (AC-1): a viewport at/above the shared breakpoint mounts
/// the grid arm, and the section viewports' geometry channel still
/// converges under the grid (the chunk plan's #1310 risk: the clip
/// heights settle to their measured extents with the sections
/// re-parented into a column).
#[test]
fn us17_s1_wide_viewport_mounts_the_two_column_grid() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(
            grid_arm(mounted),
            "the wide viewport mounts the grid arm (typed layout fact)"
        );
        // The geometry channel's convergence law composes with the
        // grid: every section's realized clip mirrors its measured
        // extent (the fixture's sections fit far below the page cap
        // at 900 px height), and the overdue extent covers its live
        // rows.
        let facts = mounted
            .handle
            .window()
            .global::<taskboard_ui_slint::TodayClipFacts>();
        let overdue_extent = facts.get_e0();
        assert!(
            overdue_extent > 0.0,
            "the overdue extent covers its live rows under the grid arm"
        );
        #[allow(clippy::float_cmp)]
        {
            assert_eq!(
                facts.get_h0(),
                overdue_extent,
                "the overdue clip mirrors its extent under the grid arm"
            );
        }
    });
}

/// US-17/S1 (AC-2, the narrow half): a viewport below the breakpoint
/// keeps the single-column arm — the typed grid fact is false and the
/// full-width band stays out of the wide arrangement.
#[test]
fn us17_s2_narrow_viewport_keeps_the_single_column() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(NARROW.0, NARROW.1);
        mounted.driver.advance_time(0);
        assert!(
            !grid_arm(mounted),
            "the narrow viewport mounts the single-column arm"
        );
    });
}

/// US-17/S3 (AC-3, the A1 aggregation pin): the SAME state renders
/// the SAME section row sets and habit rows on both arms — two
/// columns reflow placement only, membership and ordering-within-
/// section are the all-live-boards aggregates either way. The resize
/// across the breakpoint also exercises the arm's re-flow (the
/// escape half of US-17/S1).
#[test]
fn us17_s3_aggregated_row_sets_unchanged_in_both_columns() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(grid_arm(mounted), "precondition: the wide arm mounted");
        let wide_rows = task_row_ids(mounted);
        let wide_habits = habit_count(mounted);
        assert!(
            wide_rows.len() >= 3 && wide_habits >= 1,
            "precondition: the fixture populates several sections and a habit"
        );

        mounted.driver.set_window_size(NARROW.0, NARROW.1);
        mounted.driver.advance_time(0);
        assert!(
            !grid_arm(mounted),
            "the resize below the breakpoint re-flows to the narrow arm"
        );
        assert_eq!(
            task_row_ids(mounted),
            wide_rows,
            "the task-section row sets are arm-independent (placement only)"
        );
        assert_eq!(
            habit_count(mounted),
            wide_habits,
            "the aggregated habit rows are arm-independent"
        );
    });
}

/// US-18/S1 (AC-1): on the grid arm the walk is column-major —
/// stepping past the left column's LAST task row (the Done section's
/// row) wraps to the right column's first habit row.
#[test]
fn us18_s1_walk_wraps_left_column_to_right() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(grid_arm(mounted), "precondition: the wide arm mounted");
        let task_rows = task_row_ids(mounted).len();

        // Entering the walk lands on the first row (the first overdue
        // task — the left column's top); stepping to the left
        // column's bottom row holds the emphasis on a task row.
        mounted.handle.dispatch_action(ActionId::NavNextItem);
        mounted.driver.advance_time(0);
        assert_eq!(
            emphasized_task_row(mounted),
            Some(0),
            "the walk enters at the left column's first task row"
        );
        for _ in 0..(task_rows - 1) {
            mounted.handle.dispatch_action(ActionId::NavNextItem);
            mounted.driver.advance_time(0);
        }
        let last_task = emphasized_task_row(mounted);
        assert_eq!(
            last_task,
            Some(task_rows - 1),
            "the walk sits on the left column's last task row"
        );

        // The column wrap: the next step lands on the right column's
        // first habit row (a HABIT landing — the typed row-kind
        // oracle, not a position guess).
        mounted.handle.dispatch_action(ActionId::NavNextItem);
        mounted.driver.advance_time(0);
        assert_eq!(
            emphasized_task_row(mounted),
            None,
            "the task cursor cleared on the habit landing (the exclusivity law)"
        );
        assert_eq!(
            emphasized_habit_row(mounted),
            Some(0),
            "the wrap lands on the right column's first habit row"
        );
    });
}

/// US-18/S2 (AC-2): the prev mapping mirrors the wrap (right column
/// back into the left column's bottom row), and the walked row's
/// chords still dispatch — the done toggle on a task row routes the
/// same typed command on the grid arm as on the single column.
#[test]
fn us18_s2_prev_mapping_mirrors_and_row_chords_hold() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(grid_arm(mounted), "precondition: the wide arm mounted");
        let task_rows = task_row_ids(mounted).len();

        // Walk onto the first habit row, then mirror back.
        for _ in 0..task_rows {
            mounted.handle.dispatch_action(ActionId::NavNextItem);
            mounted.driver.advance_time(0);
        }
        assert_eq!(
            emphasized_habit_row(mounted),
            Some(0),
            "precondition: the walk sits on the right column's first habit"
        );
        mounted.handle.dispatch_action(ActionId::NavPrevItem);
        mounted.driver.advance_time(0);
        assert_eq!(
            emphasized_task_row(mounted),
            Some(task_rows - 1),
            "prev mirrors the wrap back onto the left column's last task row"
        );

        // The row chord holds: the complete chord on the focused row
        // routes the typed command over the fake-engine boundary
        // (the row verbs are arm-independent).
        mounted.handle.dispatch_action(ActionId::TaskComplete);
        mounted.driver.advance_time(0);
        let drained = mounted.engine.drain();
        assert!(
            drained.iter().any(|command| matches!(
                command,
                taskboard_domain::StateCommand::SetTaskDone { .. }
            )),
            "the walked row's complete chord routes the typed done command: {drained:?}"
        );
    });
}

/// US-19-as-amended (2026-11-25 operator ruling, #1436): the kiosk
/// mount is NOT special-cased — the SAME width test rules the arm on
/// a kiosk window. Wide kiosk: the grid mounts; narrow kiosk: the
/// single column. No kiosk branch exists in the layout logic, so the
/// fold is `(width)` alone.
#[test]
fn us19_s1_kiosk_mounts_the_grid_when_space_allows() {
    let state = grid_fixture();
    support::harness::run_journey_no_escape(state, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(
            grid_arm(mounted),
            "the kiosk mount renders two-column when space allows (the amendment)"
        );
        mounted.driver.set_window_size(NARROW.0, NARROW.1);
        mounted.driver.advance_time(0);
        assert!(
            !grid_arm(mounted),
            "the kiosk mount renders single-column below the breakpoint"
        );
    });
}

/// US-19-as-amended's kiosk fixture twin: the same assertions from a
/// kiosk-settings mount (the posture, not a resized desktop window).
#[test]
fn us19_s1_kiosk_posture_mounts_the_grid_when_space_allows() {
    let state = grid_fixture();
    let settings = kiosk_today_settings();
    support::harness::run_journey_no_escape_with_settings(state, settings, move |mounted| {
        mounted.driver.set_window_size(WIDE.0, WIDE.1);
        mounted.driver.advance_time(0);
        assert!(
            grid_arm(mounted),
            "the kiosk posture mounts the grid arm at the wide width"
        );
    });
}
