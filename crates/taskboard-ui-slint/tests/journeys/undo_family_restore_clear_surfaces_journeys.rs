// SPDX-License-Identifier: AGPL-3.0-only

//! The undo-family restore/clear-surface journeys (the accepted
//! journey plan's §8 wiring rows W-2..W-12, #1146 + #1154; the
//! zero-input amendment D-5 of 2026-11-23).
//!
//! Step-ID-named over the journeys suite's discipline: terminal state
//! in the presented model + the typed commands at the fake-engine
//! boundary; labels are element *addresses*, never text oracles.
//!
//! The roster rows are hand-seeded fixture state (the engine's derive
//! is the fold suite's oracle — the journeys own routing only); the
//! fake engine's restore arms are fixture continuity, not an oracle.

use crate::support;

use support::driver::JourneyDriver as _;
use support::fixture;
use support::harness::Harness;

use slint::Model as _;
use taskboard_domain::StateCommand;
use taskboard_ui_slint::View;
use taskboard_ui_slint::actions::ActionId;

/// Seeds one tombstoned label on the fixture board (the roster's row
/// source; the derive is the fold suite's oracle).
fn seed_tombstoned_label(state: &mut taskboard_domain::AppState) -> taskboard_domain::LabelId {
    let board = *state.boards.keys().next().expect("fixture board");
    let id = taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7());
    state.labels.insert(
        id,
        taskboard_domain::Label {
            id,
            remote: None,
            board,
            title: "buried".into(),
            color: taskboard_domain::Color::new("31cc7c"),
            deleted: true,
            clocks: taskboard_domain::LabelClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                color: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::LabelAuthors::default(),
            remote_seen: None,
        },
    );
    state.roster.tombstoned_labels = vec![taskboard_domain::RestoreRow {
        id,
        name: "buried".into(),
    }];
    id
}

/// Seeds one live habit + one tombstoned habit (the context menu's
/// row and the roster's restore row).
fn seed_habits(
    state: &mut taskboard_domain::AppState,
) -> (taskboard_domain::HabitId, taskboard_domain::HabitId) {
    let board = *state.boards.keys().next().expect("fixture board");
    let now = chrono::Utc::now();
    let make = |title: &str, deleted: bool| taskboard_domain::habit::Habit {
        id: taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7()),
        board,
        title: title.into(),
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
        deleted,
        by: None,
        paused_until: None,
    };
    let live = make("water", false);
    let gone = make("buried-habit", true);
    let (live_id, gone_id) = (live.id, gone.id);
    state.habits.habits.insert(live.id, live);
    state.habits.habits.insert(gone.id, gone);
    state.roster.tombstoned_habits = vec![taskboard_domain::RestoreRow {
        id: gone_id,
        name: "buried-habit".into(),
    }];
    (live_id, gone_id)
}

/// Seeds one deleted-entry roster row (the zero-input restore's row
/// source — the journal id is the engine arm's address).
fn seed_deleted_entry(
    state: &mut taskboard_domain::AppState,
    label: &str,
) -> (taskboard_domain::TimeEntryId, taskboard_domain::JournalId) {
    let entry_id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    let journal_id = taskboard_domain::JournalId::from_uuid(uuid::Uuid::now_v7());
    state
        .roster
        .deleted_entries
        .push(taskboard_domain::DeletedEntryRow {
            entry_id,
            journal_id,
            label: label.into(),
        });
    (entry_id, journal_id)
}

/// The detail walk over the fixture's first card (the reminder-undo
/// journeys' idiom — through the real dispatch path).
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

// ---- US-1: restore a deleted label from the labels overlay (W-2) ----

/// US-1/S1 (W-2, AC-1): the overlay's deleted section lists the
/// tombstoned label from the roster projection (typed rows, non-empty
/// flag); the empty roster renders the section absent (no dead
/// chrome).
#[test]
fn us1_s1_overlay_lists_tombstoned_label() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_has_deleted(), "the section mounts");
        assert_eq!(
            window.get_label_overlay_deleted_rows().row_count(),
            1,
            "the tombstoned label derives one row"
        );
    });
    // The empty-roster mount: the section renders absent.
    let (_fx, state) = fixture::golden_state();
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(
            !window.get_label_overlay_has_deleted(),
            "no dead chrome for an empty roster"
        );
    });
}

/// US-1/S2 (W-5): the restore click arms the ceremony — the armed
/// strip's mirror index flips, nothing dispatches (the D-2 grammar).
#[test]
fn us1_s2_restore_arm_mounts_the_strip() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_restore_armed_index(),
            0,
            "the armed strip mounts over the clicked row"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "arming stages; the confirm dispatches"
        );
    });
}

/// US-1/S3 (W-2/W-6, AC-2): the confirm dispatches the exact
/// `RestoreLabel { id }` and the settle flips the tombstone live (the
/// roster drains — the section unmounts); the stale-live-target edge
/// is the typed absent row (AC-3).
#[test]
fn us1_s3_restore_label_dispatches_and_receipts() {
    let (_fx, mut state) = fixture::golden_state();
    let label = seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        let armed_ok = window.get_label_overlay_restore_armed_index();
        let _ = armed_ok;
        harness.assert_commands(&[StateCommand::RestoreLabel { id: label }]);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            !window.get_label_overlay_has_deleted(),
            "the restored label left the roster (the section drained)"
        );
    });
}

/// US-1/S3 (AC-3): a stale roster row over a live label is the typed
/// absent row — the row never renders, the armed mirror stays -1,
/// nothing dispatches.
#[test]
fn us1_s3_stale_live_target_is_accepted_noop() {
    let (_fx, mut state) = fixture::golden_state();
    // A roster row naming a label that is *not* tombstoned (absent
    // from the map entirely — the stalest shape).
    state.roster.tombstoned_labels = vec![taskboard_domain::RestoreRow {
        id: taskboard_domain::LabelId::from_uuid(uuid::Uuid::now_v7()),
        name: "ghost".into(),
    }];
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        assert!(
            !window.get_label_overlay_has_deleted(),
            "the stale row derives no rendered row"
        );
        assert_eq!(window.get_label_overlay_restore_armed_index(), -1);
        assert!(
            harness.engine.drain().is_empty(),
            "the boundary stayed quiet"
        );
    });
}

/// US-1/S4 (W-5, AC-4): Escape at the armed strip (the overlay
/// ladder's dismissal rung) disarms — nothing dispatches.
#[test]
fn us1_s4_escape_disarms_without_dispatch() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        assert_eq!(window.get_label_overlay_restore_armed_index(), 0);
        window.invoke_labels_overlay_dismiss_requested();
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_label_overlay_restore_armed_index(),
            -1,
            "the escape rung disarmed"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "nothing dispatched on the escape"
        );
    });
}

/// US-1/S5 (#1245): the labels overlay's bulk row — the armed confirm
/// dispatches one `RestoreLabel` per board-scoped roster row; the arm
/// alone dispatches nothing.
#[test]
fn us1_s5_restore_all_dispatches_one_bulk_fact() {
    let (_fx, mut state) = fixture::golden_state();
    let buried = seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_all_requested();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the bulk arm stages; the confirm dispatches"
        );
        assert!(
            window.get_label_overlay_restore_all_armed(),
            "the bulk strip mounted"
        );
        window.invoke_label_restore_all_requested();
        harness.driver.advance_time(0);
        // #1345: ONE `RestoreLabelsBulk` fact — the pre-image bundle
        // makes the roster one undo step, not N per-row facts.
        harness.assert_commands(&[StateCommand::RestoreLabelsBulk { ids: vec![buried] }]);
    });
}

/// US-1/S6 (#1245): Escape at the ARMED BULK strip runs the same
/// ladder rung as the per-row twin — it disarms first (the C1 law's
/// staged-state rung), the overlay closes only on the next Escape.
#[test]
fn us1_s6_escape_disarms_the_bulk_strip() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_all_requested();
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_restore_all_armed());
        window.invoke_labels_overlay_dismiss_requested();
        harness.driver.advance_time(0);
        assert!(
            !window.get_label_overlay_restore_all_armed(),
            "the escape rung disarmed the bulk strip"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "nothing dispatched on the escape"
        );
    });
}

/// US-1/S7 (#1245): arming a per-row restore disarms the armed bulk
/// strip — one ceremony at a time (the habits twin's law).
#[test]
fn us1_s7_per_row_arm_disarms_the_bulk_strip() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_all_requested();
        harness.driver.advance_time(0);
        assert!(window.get_label_overlay_restore_all_armed());
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        assert!(
            !window.get_label_overlay_restore_all_armed(),
            "the per-row arm took the ceremony over"
        );
        assert!(
            harness.engine.drain().is_empty(),
            "the handover staged; nothing dispatched"
        );
    });
}

// ---- US-2: restore a deleted habit from the deleted-habits section
// (#1290's enumeration amendment: the roster enumerates in the
// labels-overlay section shape; the newest-only context-menu arm
// retired with it) ----

/// Seeds a second tombstoned habit (the enumeration pin's tail: the
/// older tombstone the retired menu arm could never reach).
fn seed_second_tombstoned_habit(
    state: &mut taskboard_domain::AppState,
) -> taskboard_domain::HabitId {
    let board = *state.boards.keys().next().expect("fixture board");
    let now = chrono::Utc::now();
    let id = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
    state.habits.habits.insert(
        id,
        taskboard_domain::habit::Habit {
            id,
            board,
            title: "older-tombstone".into(),
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
            deleted: true,
            by: None,
            paused_until: None,
        },
    );
    // The roster is id-ordered (the derive's BTreeMap order); the
    // second tombstone lands after the seed's.
    state
        .roster
        .tombstoned_habits
        .push(taskboard_domain::RestoreRow {
            id,
            name: "older-tombstone".into(),
        });
    state
        .roster
        .tombstoned_habits
        .sort_by_key(|row| row.id.as_uuid());
    id
}

/// US-2/S1 (AC-1): the section lists every tombstoned habit with its
/// name — the enumeration pin (#1290): with TWO tombstones, both rows
/// are reachable (the retired arm reached the newest only).
#[test]
fn us2_s1_deleted_section_enumerates_the_roster() {
    let (_fx, mut state) = fixture::golden_state();
    let (_live, _gone) = seed_habits(&mut state);
    let older = seed_second_tombstoned_habit(&mut state);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        let rows = window.get_habits_deleted_rows();
        assert_eq!(
            rows.row_count(),
            2,
            "the section enumerates the whole roster, not the newest only"
        );
        // The armed strip's index resolution reaches the OLDER row
        // (the defect's typed pin: the retired arm could address only
        // the roster head).
        window.invoke_habits_habit_restore_requested(1);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the arm stages; the confirm dispatches"
        );
        window.invoke_habits_habit_restore_requested(1);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::RestoreHabit { id: older }]);
    });
}

/// US-2/S2 (AC-2): the confirm dispatches the exact
/// `RestoreHabit { id }`; the settle flips the tombstone (the log is
/// never touched — D26).
#[test]
fn us2_s2_restore_habit_dispatches_log_untouched() {
    let (_fx, mut state) = fixture::golden_state();
    let (_live, gone) = seed_habits(&mut state);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_habit_restore_requested(0);
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the arm stages; the confirm dispatches"
        );
        window.invoke_habits_habit_restore_requested(0);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::RestoreHabit { id: gone }]);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        let snapshot = harness.engine.snapshot();
        let row = snapshot.habits.habits.get(&gone).expect("the habit row");
        assert!(!row.deleted, "the habit flipped live verbatim");
        assert!(
            snapshot.habits.events.is_empty(),
            "the log is never touched (D26)"
        );
    });
}

/// US-2/S3 (AC-3): the cancel step disarms — nothing dispatched, the
/// strip leaves (the armed index falls back to -1).
#[test]
fn us2_s3_escape_disarms_without_dispatch() {
    let (_fx, mut state) = fixture::golden_state();
    let (_live, _gone) = seed_habits(&mut state);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_habit_restore_requested(0);
        harness.driver.advance_time(0);
        window.invoke_habits_habit_restore_cancelled();
        harness.driver.advance_time(0);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
        assert_eq!(
            window.get_habits_restore_armed_index(),
            -1,
            "the escape rung disarmed"
        );
    });
}

/// US-2/S4 (#1245): the bulk row's armed confirm dispatches one
/// `RestoreHabit` per roster row; the arm alone dispatches nothing.
#[test]
fn us2_s4_restore_all_dispatches_one_bulk_fact() {
    let (_fx, mut state) = fixture::golden_state();
    let (_live, gone) = seed_habits(&mut state);
    let older = seed_second_tombstoned_habit(&mut state);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_habit_restore_all_requested();
        harness.driver.advance_time(0);
        assert!(
            harness.engine.drain().is_empty(),
            "the bulk arm stages; the confirm dispatches"
        );
        assert!(
            window.get_habits_restore_all_armed(),
            "the bulk strip mounted"
        );
        window.invoke_habits_habit_restore_all_requested();
        harness.driver.advance_time(0);
        // #1345: ONE `RestoreHabitsBulk` fact carrying the whole
        // roster (one undo walks it back).
        harness.assert_commands(&[StateCommand::RestoreHabitsBulk {
            ids: vec![gone, older],
        }]);
    });
}

/// US-2/S5 (#1245): the bulk cancel disarms without dispatching.
#[test]
fn us2_s5_restore_all_cancel_disarms_without_dispatch() {
    let (_fx, mut state) = fixture::golden_state();
    let (_live, _gone) = seed_habits(&mut state);
    support::harness::run_journey(state, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_habits_habit_restore_all_requested();
        harness.driver.advance_time(0);
        window.invoke_habits_habit_restore_all_cancelled();
        harness.driver.advance_time(0);
        assert!(harness.engine.drain().is_empty(), "nothing dispatched");
        assert!(
            !window.get_habits_restore_all_armed(),
            "the cancel disarmed the bulk strip"
        );
    });
}

// ---- US-3: clear a task's reminder rules (W-4) ----

/// The detail walk with one armed rule seeded (the reminder-undo
/// journeys' fixture shape).
fn detail_with_rules(harness: &mut Harness, count: usize) {
    open_detail_first_card(harness);
    let window = harness.handle.window();
    assert_eq!(
        window.get_task_model().rules.row_count(),
        count,
        "the seeded rules render"
    );
}

/// US-3/S1 (W-4, AC-3): zero rules renders the row disabled — the
/// typed structural fact (`rules_clearable` is the push verdict, D-2).
#[test]
fn us3_s1_clear_row_disabled_at_zero_rules() {
    let (_fx, state) = fixture::golden_state();
    assert!(
        state.reminders.rules.is_empty(),
        "the fixture card starts rule-less"
    );
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        assert!(
            !harness.handle.window().get_rules_clearable(),
            "the disabled verdict at zero rules"
        );
        // The disabled row's inert: the ask dispatches nothing.
        harness.handle.window().invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        assert!(harness.engine.drain().is_empty(), "no ceremony for a no-op");
    });
}

/// US-3/S2 (W-5): the row click arms (the strip mounts, the armed
/// mirror flips) — nothing dispatches.
#[test]
fn us3_s2_clear_arm_mounts_the_strip() {
    let (fx, mut state) = fixture::golden_state();
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB21);
    support::harness::run_journey(state, move |harness| {
        detail_with_rules(harness, 1);
        let window = harness.handle.window();
        assert!(window.get_rules_clearable(), "n ≥ 1 enables the row");
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        assert!(window.get_rule_clear_armed(), "the strip mounted");
        assert!(harness.engine.drain().is_empty(), "arming stages");
    });
}

/// US-3/S3 (W-4/W-6, AC-1): the confirm dispatches the exact
/// `ClearReminderRules { task }` once; the settle clears the rows and
/// the armed mirror resets with the post-clear refresh.
#[test]
fn us3_s3_clear_all_dispatches_once() {
    let (fx, mut state) = fixture::golden_state();
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB21);
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB22);
    support::harness::run_journey(state, move |harness| {
        detail_with_rules(harness, 2);
        let window = harness.handle.window();
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::ClearReminderRules { task: fx.task0 }]);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            0,
            "the list rendered empty"
        );
    });
}

/// US-3/S5 (W-7, AC-2): the clear journaled, one global-undo step
/// restores all n rules (UF1's `PreImage::ReminderRule` synthesis —
/// the fake's batch continuity mirrors the domain fold's oracle).
#[test]
fn us3_s5_clear_undo_restores_all_rules() {
    let (fx, mut state) = fixture::golden_state();
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB21);
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB22);
    support::harness::run_journey(state, move |harness| {
        detail_with_rules(harness, 2);
        let window = harness.handle.window();
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_task_model().rules.row_count(),
            0,
            "the clear cleared"
        );
        // The one undo step.
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        assert_eq!(
            harness.handle.window().get_task_model().rules.row_count(),
            2,
            "one undo step restored all n rules"
        );
    });
}

/// US-3/S4 (W-5, AC-4): the strip's cancel disarms — nothing
/// dispatches.
#[test]
fn us3_s4_escape_disarms_without_dispatch() {
    let (fx, mut state) = fixture::golden_state();
    let _ = fixture::add_armed_rule(&mut state, fx.task0, 0xB21);
    support::harness::run_journey(state, move |harness| {
        detail_with_rules(harness, 1);
        let window = harness.handle.window();
        window.invoke_rules_clear_asked();
        harness.driver.advance_time(0);
        window.invoke_rules_clear_cancelled();
        harness.driver.advance_time(0);
        assert!(!window.get_rule_clear_armed(), "the cancel disarmed");
        assert!(
            harness.engine.drain().is_empty(),
            "the boundary stayed quiet"
        );
    });
}

// ---- US-4: the undo line composes (W-7, the regression fence) ----

/// US-4/S1 (W-7, AC-1): a GUI restore undone re-tombstones (the
/// #1187 law, the GUI leg) — the global undo reflex rides the same
/// journal facts.
#[test]
fn us4_s1_gui_restore_undo_redeletes() {
    let (_fx, mut state) = fixture::golden_state();
    let label = seed_tombstoned_label(&mut state);
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        let window = harness.handle.window();
        window.invoke_labels_manage_asked();
        harness.driver.advance_time(0);
        window.invoke_label_restore_requested(0);
        window.invoke_label_restore_requested(0);
        harness.driver.advance_time(0);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        // The overlay's ladder closes first (the modal context owns
        // the strokes; the undo reflex rides the detail's chain).
        window.invoke_labels_overlay_dismiss_requested();
        harness.driver.advance_time(0);
        // The global undo reflex (the pre-existing affordance).
        harness.press(ActionId::AppUndo);
        harness.driver.advance_time(0);
        harness.assert_commands(&[StateCommand::Undo]);
        harness.engine.settle(&harness.handle);
        harness.driver.advance_time(0);
        let snapshot = harness.engine.snapshot();
        assert!(
            snapshot.labels.get(&label).expect("the label row").deleted,
            "the undo re-tombstoned the restored label"
        );
    });
}

// ---- US-5: the zero-input time-entry restore (W-8/W-9/W-12, D-5) ----

/// US-5/S1 (W-9, AC-1): the track surface's deleted-entries section
/// lists the roster row (engine-derived label, typed journal id
/// behind the index seam); the empty roster renders the section
/// absent.
#[test]
fn us5_s1_track_surface_lists_deleted_entry() {
    let (_fx, mut state) = fixture::golden_state();
    seed_deleted_entry(&mut state, "story · 2026-11-20 09:00 – 2026-11-20 10:30");
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        harness.handle.dispatch_action(ActionId::TaskTrackEntries);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(window.get_track_has_deleted(), "the section mounts");
        assert_eq!(
            window.get_track_deleted_rows().row_count(),
            1,
            "the roster row renders with its engine-derived label"
        );
    });
}

/// US-5/S2 (W-5): the restore click arms — the strip's mirror flips,
/// nothing dispatches.
#[test]
fn us5_s2_restore_arm_mounts_the_strip() {
    let (_fx, mut state) = fixture::golden_state();
    seed_deleted_entry(&mut state, "story · span");
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        harness.handle.dispatch_action(ActionId::TaskTrackEntries);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_entry_restore_requested(0);
        harness.driver.advance_time(0);
        assert_eq!(
            window.get_track_restore_armed_index(),
            0,
            "the armed strip mounts"
        );
        assert!(harness.engine.drain().is_empty(), "arming stages");
    });
}

/// US-5/S3 (W-9/W-12, AC-2, the zero-input pin — the definition-of-done bar):
/// the confirm dispatches `RestoreTimeEntryFromJournal { from }` and
/// the exact command payload carries **only the journal id** — no
/// payload field is user-provided on any surface (the D-5 bar); the
/// stale armed target absorbs (the roster's stale law).
#[test]
fn us5_s3_restore_from_journal_dispatches_id_only() {
    let (_fx, mut state) = fixture::golden_state();
    let (_entry, journal) = seed_deleted_entry(&mut state, "story · span");
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        harness.handle.dispatch_action(ActionId::TaskTrackEntries);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_entry_restore_requested(0);
        harness.driver.advance_time(0);
        window.invoke_entry_restore_requested(0);
        harness.driver.advance_time(0);
        // The pin: the drained boundary carries the wrapper, its
        // payload the journal id and nothing else.
        harness.assert_commands(&[StateCommand::RestoreTimeEntryFromJournal { from: journal }]);
    });
}

/// US-5/S4 (W-5, AC-4): Escape at the armed strip disarms — nothing
/// dispatched.
#[test]
fn us5_s4_escape_disarms_without_dispatch() {
    let (_fx, mut state) = fixture::golden_state();
    seed_deleted_entry(&mut state, "story · span");
    support::harness::run_journey(state, move |harness| {
        open_detail_first_card(harness);
        harness.handle.dispatch_action(ActionId::TaskTrackEntries);
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        window.invoke_entry_restore_requested(0);
        harness.driver.advance_time(0);
        window.invoke_entry_restore_cancelled();
        harness.driver.advance_time(0);
        assert_eq!(window.get_track_restore_armed_index(), -1, "disarmed");
        assert!(
            harness.engine.drain().is_empty(),
            "the boundary stayed quiet"
        );
    });
}

// ---- R-5/D-4: the kiosk absence (structural, not pixel) ----

/// R-5 (I-3): the kiosk profile's published halves carry empty
/// sections and the disabled clear verdict — the affordances render
/// absent structurally (the adapter's kiosk gates), whatever the
/// roster holds.
#[test]
fn r5_kiosk_renders_the_affordances_absent() {
    let (_fx, mut state) = fixture::golden_state();
    seed_tombstoned_label(&mut state);
    seed_deleted_entry(&mut state, "story · span");
    let settings = taskboard_ui_slint::UiSettings {
        kiosk: true,
        ..taskboard_ui_slint::UiSettings::default()
    };
    support::harness::run_journey_with_settings(state, settings, move |harness| {
        harness.driver.advance_time(0);
        let window = harness.handle.window();
        assert!(
            !window.get_label_overlay_has_deleted(),
            "the labels restore section renders absent on kiosk"
        );
        assert!(
            !window.get_track_has_deleted(),
            "the deleted-entries section renders absent on kiosk"
        );
        assert!(
            !window.get_rules_clearable(),
            "the clear-all row greys on kiosk (the editing gate's posture)"
        );
    });
}
