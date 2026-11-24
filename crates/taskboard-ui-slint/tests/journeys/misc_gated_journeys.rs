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

use crate::walk_planner_support as planner_support;

use slint::Model as _;
use taskboard_domain::{AppState, StateCommand, SystemEvent};
use taskboard_ui_slint::UiSettings;
use taskboard_ui_slint::actions::ActionId;

// -- shared fixtures -----------------------------------------------------

/// The fixture's pool pane: task0 (carried), task1, and the two triage
/// rows — all disarmed.
const POOL_DISARMED: [bool; 4] = [false, false, false, false];

/// The walk-planner journeys' fixture: the walk stage ladder's rows
/// (two triage tasks, task0 carried from yesterday) plus one task
/// scheduled into tomorrow's morning — the embedded planner's placed
/// box row. Returns the box task's id. The builders come from the
/// suite's shared `walk_planner_support` module.
fn walk_planner_fixture() -> (fixture::Fixture, AppState, taskboard_domain::TaskId) {
    let (fx, mut state) = fixture::golden_state();
    // The golden state pins the review card away (reflection entries);
    // the walk journeys need the due state — clear the pin (the
    // walk_fixture_state convention).
    state.reflection.entries.clear();
    let inbox_stack = planner_support::insert_inbox_stack(&mut state);
    for title in ["triage-me", "triage-me-too"] {
        let task = planner_support::walk_new_task(inbox_stack, title);
        state.tasks.insert(task.id, task);
    }
    planner_support::anchor_yesterday(&mut state, fx.task0);
    let box_task = planner_support::place_stack_b_box(&mut state, fx.stack_b);
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
/// undo suite, never re-tested through the fake engine, which keeps
/// no capture journal), and `AppRedo` re-checks the forward arm
/// (S4). The journey layer's contract is the ROUTING: the three
/// typed commands at the boundary, in order.
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
            (vec![false], Vec::from(POOL_DISARMED)),
            "a fresh embedded planner is disarmed"
        );
        harness.handle.dispatch_action(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], Vec::from(POOL_DISARMED)),
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
            (vec![true], Vec::from(POOL_DISARMED)),
            "the last row clamps (no wrap)"
        );
        harness.handle.dispatch_action(ActionId::NavFocusUp);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::NavFocusUp);
        harness.driver.advance_time(0);
        assert_eq!(
            plan_cursor_bits(harness),
            (vec![true], Vec::from(POOL_DISARMED)),
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
            (vec![true], Vec::from(POOL_DISARMED)),
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
            (vec![true], Vec::from(POOL_DISARMED)),
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
            (vec![false], Vec::from(POOL_DISARMED)),
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
