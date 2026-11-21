// SPDX-License-Identifier: AGPL-3.0-only

//! The reminder-undo journeys (UF1, #354 — the accepted journey plan's
//! §6 wiring rows W-4/W-2): the rule edit verbs compose with the
//! pre-existing global undo reflex (`ActionId::AppUndo` →
//! `StateCommand::Undo`) — no new affordance anywhere, the delta is
//! that the composition now works. Assertion discipline is the
//! journeys suite's: terminal state in the presented model + the typed
//! commands at the fake-engine boundary; labels are addresses, never
//! text oracles. The undo line's continuity half lives in the
//! `FakeEngine` (fixture continuity, not an oracle — the domain fold
//! suite owns the undo semantics).

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::Model as _;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::ActionId;

/// The detail walk over the fixture's first card (the
/// `NavFocusBoard → NavFirstItem → TaskEdit` idiom the golden tier's
/// opener uses — through the real dispatch path, no direct state).
fn open_detail_first_card(harness: &mut Harness) {
    harness.handle.dispatch_action(ActionId::NavFocusBoard);
    harness.driver.advance_time(0);
    harness.handle.dispatch_action(ActionId::NavFirstItem);
    harness.driver.advance_time(0);
    harness.handle.dispatch_action(ActionId::TaskEdit);
    harness.driver.advance_time(0);
    assert_eq!(
        harness.handle.window().get_current_view(),
        View::TaskDetail,
        "the detail is open"
    );
}

/// US-1/S2 (W-4, AC-1): removing the rule row (the pre-existing
/// affordance) then the global undo reflex restores the row in the
/// presented model — the composition works end-to-end through the
/// driver, no UI change anywhere.
#[test]
fn us1_s2_undo_restores_removed_rule_row() {
    let (fixture, mut state) = fixture::golden_state();
    let rule_id = fixture::add_armed_rule(&mut state, fixture.task0, 0xB21);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let rules = harness.handle.window().get_task_model().rules;
        assert_eq!(rules.row_count(), 1, "the seeded rule renders");
        // The pre-existing remove affordance (the armed cursor's
        // activate grammar, the rule cursor journeys' path).
        harness.press(ActionId::NavFocusDown);
        harness.driver.advance_time(0);
        harness.handle.dispatch_action(ActionId::TaskEdit);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::RemoveReminderRule { rule: rule_id }]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            0,
            "the rule row left the presented roster"
        );
        // The global undo reflex (the pre-existing affordance; W-4's
        // zero-change wiring).
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::Undo]);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            1,
            "the undo restored the rule row"
        );
    });
}

/// US-2/S2 (W-4): adding a rule (the preset chip) then the global undo
/// reflex deletes the added row again — the mistaken rule never fires.
#[test]
fn us2_s2_undo_deletes_added_rule_row() {
    let (fixture, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            0,
            "the fixture card starts rule-less"
        );
        // The pre-existing add affordance (the preset chip seam).
        harness.handle.window().invoke_rule_add_preset_clicked(0);
        harness.driver.advance_time(0);
        let drained = harness.engine.drain();
        assert!(
            matches!(&drained[..], [StateCommand::AddReminderRule { task, .. }] if *task == fixture.task0),
            "the preset chip routes the exact add: {drained:?}"
        );
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            1,
            "the added rule renders"
        );
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.engine.settle(&harness.handle);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            0,
            "the undo deleted the added rule row"
        );
        let log = harness.engine.drain();
        assert!(
            matches!(&log[..], []),
            "only the two routed commands, nothing else: {log:?}"
        );
    });
}
