// SPDX-License-Identifier: AGPL-3.0-only

//! The misc-gated-family journeys (round 5, D4; the accepted plan
//! `.artifacts/plans/2026-11-22-misc-gated-journeys-ux-plan.md`):
//! US-21 (the accidental capture commit's undo recovery, #1151 —
//! R-1: the recovery IS the v1 undo line, no confirm-stage), US-22
//! (the review walk's embedded planner cursor, #1234 — the ONE K4
//! cursor mechanism over the `Flow::ReviewPlanner` routing), and
//! US-23/S1's kiosk-profile badge pin (#1152). US-23/S2 and US-25's
//! budget guard are `menu.rs` structural tests; US-24 is docs-only;
//! US-26 ships no rows (design-only, R-4).

use crate::support;

use support::fixture;
use support::harness::Harness;

use support::driver::JourneyDriver as _;

use slint::Model as _;
use taskboard_domain::{AppState, StateCommand, SystemEvent};
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;

// -- shared fixtures -----------------------------------------------------

/// A fresh triage row on `stack` (the walk fixtures' shared task
/// literal — the `walk_new_task` shape).
fn walk_new_task(stack: taskboard_domain::StackId, title: &str) -> taskboard_domain::Task {
    taskboard_domain::Task {
        id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
        remote: None,
        label_memberships: std::collections::BTreeMap::new(),
        title: title.to_owned(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack,
        order: 0,
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::entities::TaskClocks {
            title: taskboard_domain::entities::BoardClocks::epoch(),
            description: taskboard_domain::entities::BoardClocks::epoch(),
            duedate: taskboard_domain::entities::BoardClocks::epoch(),
            scheduled: taskboard_domain::entities::BoardClocks::epoch(),
            done: taskboard_domain::entities::BoardClocks::epoch(),
            position: taskboard_domain::entities::BoardClocks::epoch(),
            effort: taskboard_domain::entities::BoardClocks::epoch(),
            importance: taskboard_domain::entities::BoardClocks::epoch(),
            urgency: taskboard_domain::entities::BoardClocks::epoch(),
            recur: taskboard_domain::entities::BoardClocks::epoch(),
            kind: taskboard_domain::entities::BoardClocks::epoch(),
            archived: taskboard_domain::entities::BoardClocks::epoch(),
            deleted: taskboard_domain::entities::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

/// An inbox board + stack to hang the triage rows on (the walk
/// fixtures' `insert_inbox_board_and_stack` shape).
fn insert_inbox_stack(state: &mut AppState) -> taskboard_domain::StackId {
    use taskboard_domain::entities::{
        Board, BoardAuthors, BoardClocks, Stack, StackAuthors, StackClocks,
    };
    let inbox = taskboard_domain::BoardId::from_uuid(uuid::Uuid::new_v4());
    state.boards.insert(
        inbox,
        Board {
            id: inbox,
            remote: None,
            title: "Inbox".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: BoardAuthors::default(),
            remote_seen: None,
        },
    );
    let inbox_stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::new_v4());
    state.stacks.insert(
        inbox_stack,
        Stack {
            id: inbox_stack,
            remote: None,
            board: inbox,
            title: "Inbox".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: BoardClocks::epoch(),
                order: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: StackAuthors::default(),
            remote_seen: None,
        },
    );
    inbox_stack
}

/// Anchors `task` inside yesterday's window (the walk's carry row) —
/// the clock discipline's relative-oracle anchor over the harness
/// calendar's vocabulary (never a wall-clock assert).
fn anchor_yesterday(state: &mut AppState, task: taskboard_domain::TaskId) {
    let calendar = fixture::harness_calendar();
    let yesterday = calendar
        .today(chrono::Utc::now())
        .pred_opt()
        .expect("yesterday");
    let carry = calendar.instant(
        yesterday,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let mut row = state.tasks.get(&task).expect("fixture task").clone();
    row.scheduled = Some(carry);
    state.tasks.insert(task, row);
}

/// The walk-planner journeys' fixture: the walk stage ladder's rows
/// (two triage tasks, task0 carried from yesterday) plus one task
/// scheduled into tomorrow's morning — the embedded planner's placed
/// box row. Returns the box task's id.
fn walk_planner_fixture() -> (fixture::Fixture, AppState, taskboard_domain::TaskId) {
    let (fx, mut state) = fixture::golden_state();
    let inbox_stack = insert_inbox_stack(&mut state);
    for title in ["triage-me", "triage-me-too"] {
        let task = walk_new_task(inbox_stack, title);
        state.tasks.insert(task.id, task);
    }
    anchor_yesterday(&mut state, fx.task0);
    // Tomorrow's box: stack b's task (the fixture's third row — the
    // planner journeys' placed-row convention).
    let calendar = fixture::harness_calendar();
    let tomorrow = calendar
        .today(chrono::Utc::now())
        .succ_opt()
        .expect("tomorrow");
    let box_instant = calendar.instant(
        tomorrow,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let box_task = state
        .tasks
        .values()
        .find(|task| task.stack == fx.stack_b)
        .expect("stack b's task")
        .id;
    let mut boxed = state.tasks.get(&box_task).expect("box task").clone();
    boxed.scheduled = Some(box_instant);
    state.tasks.insert(box_task, boxed);
    (fx, state, box_task)
}

/// The plan model's cursor bits: `(boxes, pool)` armed flags (the
/// US-15 journeys' read seam — a typed row property, never text).
fn plan_cursor_bits(harness: &Harness) -> (Vec<bool>, Vec<bool>) {
    let model = harness.handle.window().get_plan_model();
    let boxes: Vec<bool> = (0..model.boxes.row_count())
        .map(|i| model.boxes.row_data(i).expect("row").cursor)
        .collect();
    let pool: Vec<bool> = (0..model.pool.row_count())
        .map(|i| model.pool.row_data(i).expect("row").cursor)
        .collect();
    (boxes, pool)
}

/// Opens the review walk (the action route) and advances into the
/// embedded planner child (US-22/S1): two stage advances land on
/// Carry, the planner choice mounts the `Flow::ReviewPlanner` child
/// (the walk stays open beneath, stage 3).
fn enter_walk_planner(harness: &mut Harness) {
    harness.handle.dispatch_action(ActionId::ReviewOpen);
    harness.driver.advance_time(0);
    assert!(
        harness.handle.window().get_walk_model().open,
        "the walk opened"
    );
    for _ in 0..2 {
        harness.handle.dispatch_action(ActionId::ReviewWalkAdvance);
        harness.driver.advance_time(0);
    }
    harness.handle.dispatch_action(ActionId::ReviewWalkPlanner);
    harness.driver.advance_time(0);
    assert_eq!(
        harness.handle.window().get_walk_model().stage_kind,
        3,
        "the embedded planner is the walk's own stage"
    );
}

// -- US-21: the accidental capture commit is recoverable (#1151) ---------

/// US-21/S2..S4 (`us21_s3_capture_commit_undo_recovers`): an
/// outside-click commit of a non-empty draft dispatches
/// `StateCommand::CaptureTask` (S2, the landed R-4 semantics), the
/// ordinary `AppUndo` routes `StateCommand::Undo` over that fresh
/// head (S3 — R-1: the recovery IS the v1 undo line; the inversion
/// itself — undo deletes the created task — is pinned in the domain
/// undo suite, not re-tested through the fake engine), and `AppRedo`
/// re-checks the forward arm (S4).
#[test]
fn us21_s3_capture_commit_undo_recovers() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1000, 900);
        // S1: open capture, type a non-empty draft.
        harness.press(ActionId::CaptureQuick);
        harness.driver.advance_time(0);
        assert!(harness.handle.window().get_capture_open());
        harness.type_text("recover me");
        // S2: the outside click commits and closes.
        harness.driver.click_at(6.0, 6.0);
        harness.driver.advance_time(0);
        assert!(
            !harness.handle.window().get_capture_open(),
            "the overlay closed on the outside-click commit"
        );
        harness.assert_commands(&[StateCommand::CaptureTask {
            title: "recover me".into(),
            board: None,
        }]);
        harness.engine.settle(&harness.handle);
        assert!(
            harness
                .engine
                .snapshot()
                .tasks
                .values()
                .any(|task| task.title == "recover me"),
            "the accidental commit created the task"
        );
        // S3: the ordinary undo routes over the fresh head.
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::Undo] => {}
            other => panic!("one undo over the capture head, got {other:?}"),
        }
        // S4: the redo re-check rides the journal's forward arm.
        harness.press(ActionId::AppRedo);
        harness.driver.advance_time(0);
        match harness.engine.drain().as_slice() {
            [StateCommand::Redo] => {}
            other => panic!("one redo after the undo, got {other:?}"),
        }
    });
}

// -- US-22: the embedded planner cursor walk (#1234) ---------------------

/// US-22/AC-1 (`us22_s1_embedded_cursor_arms`): inside the walk's
/// planner child the first vertical stroke arms the SAME K4 cursor
/// the standalone planner uses — the shared `plan_cursor` state over
/// the amended `Flow::ReviewPlanner` routing (no second mechanism:
/// the armed bit is the model's `cursor` row property).
#[test]
fn us22_s1_embedded_cursor_arms() {
    let (_fx, state, _box_task) = walk_planner_fixture();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1200, 900);
        enter_walk_planner(harness);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![false], vec![false, false]),
            "a fresh embedded planner is disarmed"
        );
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], vec![false, false]),
            "the arming stroke lands the box row (arm-then-step)"
        );
    });
}

/// US-22/AC-1's clamp leg (`us22_s2_embedded_cursor_walks_clamped`):
/// the armed embedded cursor clamps at both edges of the focused
/// pane — no wrap, the ends hold (the landed planner law, the same
/// seam).
#[test]
fn us22_s2_embedded_cursor_walks_clamped() {
    let (_fx, state, _box_task) = walk_planner_fixture();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1200, 900);
        enter_walk_planner(harness);
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        // The single box row clamps both ways (no wrap).
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], vec![false, false]),
            "the last row clamps (no wrap)"
        );
        harness.handle.dispatch_action(ActionId::NavFocusUp);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavFocusUp);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], vec![false, false]),
            "row 0 clamps (no wrap)"
        );
    });
}

/// US-22/AC-2 (`us22_s3_embedded_activate_uses_own_seam`): Activate
/// over the armed cursor dispatches the cursor'd row's OWN
/// `PlanBeginPlace` seam — the placement form arms inside the walk
/// (the child mount, J4 D-2), never a foreign verb.
#[test]
fn us22_s3_embedded_activate_uses_own_seam() {
    let (_fx, state, _box_task) = walk_planner_fixture();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1200, 900);
        enter_walk_planner(harness);
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], vec![false, false]),
            "armed on the box"
        );
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        assert!(
            harness.handle.window().get_plan_model().placing,
            "the cursor'd row's own place seam armed inside the walk"
        );
    });
}

/// US-22/AC-3 (`us22_s4_walk_exit_clears_cursor`): leaving the
/// planner child (the walk's Prev — the Cancelled exit) clears the
/// K4 cursor pane-scoped (the landed clear law); the walk stays open
/// on Carry.
#[test]
fn us22_s4_walk_exit_clears_cursor() {
    let (_fx, state, _box_task) = walk_planner_fixture();
    support::harness::run_journey(state, move |harness| {
        harness.driver.set_window_size(1200, 900);
        enter_walk_planner(harness);
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], vec![false, false]),
            "armed on the box"
        );
        harness.handle.dispatch_action(ActionId::ReviewWalkPrev);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_walk_model().stage_kind,
            1,
            "the Cancelled exit returned to Carry"
        );
        assert!(
            harness.handle.window().get_walk_model().open,
            "the walk never closed"
        );
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![false], vec![false, false]),
            "the child's exit cleared the cursor"
        );
    });
}

// -- US-23/S1: the degraded badge renders on the kiosk profile (#1152) ---

/// US-23/AC-1 (`us23_s1_degraded_badge_renders_in_kiosk`): on the
/// kiosk profile the `JournalDegraded` broadcast flips the badge
/// property — the status bar's display lane is not kiosk-gated (the
/// structural pin: a property value, never badge text).
#[test]
fn us23_s1_degraded_badge_renders_in_kiosk() {
    let (_fixture, state) = fixture::golden_state();
    support::harness::run_journey_with_settings(
        state,
        UiSettings {
            kiosk: true,
            ..UiSettings::default()
        },
        move |harness| {
            assert!(
                !harness.handle.window().get_journal_degraded_visible(),
                "a clean kiosk boot renders no badge"
            );
            harness
                .handle
                .observe_system_event(&SystemEvent::JournalDegraded {
                    details: vec![taskboard_domain::persistence::CorruptionDetail::UNKNOWN],
                    dropped: 2,
                    head: None,
                });
            assert!(
                harness.handle.window().get_journal_degraded_visible(),
                "the badge renders on the kiosk profile too"
            );
        },
    );
}
