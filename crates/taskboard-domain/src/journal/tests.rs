// SPDX-License-Identifier: MIT OR Apache-2.0
//! The journal substrate's unit suite: apply-arm semantics, retention
//! eviction, serde round-trips, and the derived [`UndoView`] projection.
//! One behavior, one test, lowest layer.

use chrono::{TimeZone, Utc};

use super::*;
use crate::idgen::IdGenerator as _;
use crate::persistence::{PersistenceAction, apply_actions};
use crate::test_support::CountingIds;

fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

fn entry_at(ids: &CountingIds, parent: Option<JournalId>, created_at: i64) -> JournalEntry {
    JournalEntry {
        id: ids.new_journal_id(),
        parent,
        command: crate::messages::StateCommand::RequestSync,
        facts: UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony),
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(created_at),
    }
}

#[test]
fn record_journal_appends_and_moves_the_head() {
    let ids = CountingIds::new();
    let mut state = crate::persistence::PersistedState::default();
    let first = entry_at(&ids, None, 100);
    let second = entry_at(&ids, Some(first.id), 200);
    apply_actions(
        &mut state,
        &[
            PersistenceAction::RecordJournal(first.clone()),
            PersistenceAction::RecordJournal(second.clone()),
        ],
    );
    assert_eq!(state.journal.entries, vec![first, second]);
    assert_eq!(state.journal.head, Some(state.journal.entries[1].id));
}

#[test]
fn move_journal_head_moves_the_head_both_directions() {
    let ids = CountingIds::new();
    let mut state = crate::persistence::PersistedState::default();
    let entry = entry_at(&ids, None, 100);
    apply_actions(
        &mut state,
        &[PersistenceAction::RecordJournal(entry.clone())],
    );
    assert_eq!(state.journal.head, Some(entry.id));
    apply_actions(&mut state, &[PersistenceAction::MoveJournalHead(None)]);
    assert_eq!(state.journal.head, None);
    apply_actions(
        &mut state,
        &[PersistenceAction::MoveJournalHead(Some(entry.id))],
    );
    assert_eq!(state.journal.head, Some(entry.id));
}

#[test]
fn mark_journal_pushed_flips_idempotently() {
    let ids = CountingIds::new();
    let mut state = crate::persistence::PersistedState::default();
    let entry = entry_at(&ids, None, 100);
    apply_actions(
        &mut state,
        &[PersistenceAction::RecordJournal(entry.clone())],
    );
    apply_actions(
        &mut state,
        &[
            PersistenceAction::MarkJournalPushed(entry.id),
            PersistenceAction::MarkJournalPushed(entry.id),
        ],
    );
    assert!(state.journal.entry(entry.id).unwrap().pushed);
    // An unknown id (evicted or foreign) is a no-op — the flag flip is
    // idempotent by construction.
    let stranger = CountingIds::new().new_journal_id();
    apply_actions(
        &mut state,
        &[PersistenceAction::MarkJournalPushed(stranger)],
    );
    assert_eq!(state.journal.entries.len(), 1);
}

#[test]
fn record_journal_replays_replace_in_place() {
    let ids = CountingIds::new();
    let mut state = crate::persistence::PersistedState::default();
    let mut entry = entry_at(&ids, None, 100);
    apply_actions(
        &mut state,
        &[PersistenceAction::RecordJournal(entry.clone())],
    );
    entry.pushed = true;
    apply_actions(
        &mut state,
        &[PersistenceAction::RecordJournal(entry.clone())],
    );
    assert_eq!(state.journal.entries, vec![entry]);
}

/// Builds a straight line of `n` entries at one-minute spacing starting
/// at `ts(0)`.
fn line(ids: &CountingIds, n: usize) -> (Vec<JournalEntry>, Vec<JournalId>) {
    let mut entries = Vec::new();
    let mut parent = None;
    for i in 0..n {
        let entry = entry_at(ids, parent, i64::try_from(i).unwrap() * 60);
        parent = Some(entry.id);
        entries.push(entry);
    }
    let ids = entries.iter().map(|entry| entry.id).collect();
    (entries, ids)
}

#[test]
fn cap_overflow_evicts_oldest_leaves_only_and_a_parent_survives() {
    let ids = CountingIds::new();
    // A chain of cap+10: every entry but the newest has a descendant, so
    // the only leaf is the head — eviction can take nothing without the
    // age floor satisfied? No: the head IS a leaf. The cap rule evicts
    // oldest-first *leaves*; with one leaf (the head, the youngest) the
    // overflow cannot be met and the floor stops the sweep immediately.
    let (entries, _) = line(&ids, JOURNAL_CAP + 10);
    let incoming = entries.last().unwrap().created_at;
    let evicted = evict_journal(&entries, incoming);
    assert!(
        evicted.is_empty(),
        "a pure chain has one leaf: the young head"
    );
}

#[test]
fn cap_overflow_evicts_oldest_first_when_leaves_allow() {
    let ids = CountingIds::new();
    // Every entry its own root (parent = None): every entry is a leaf.
    let entries: Vec<JournalEntry> = (0..JOURNAL_CAP + 10)
        .map(|i| entry_at(&ids, None, i64::try_from(i).unwrap() * 60))
        .collect();
    // The incoming stamp lands 90+ days after the newest entry, so the
    // age floor never shields the overflow.
    let incoming = entries.last().unwrap().created_at + JOURNAL_AGE_FLOOR;
    let evicted = evict_journal(&entries, incoming);
    assert_eq!(evicted.len(), 10);
    // The oldest ten, in order.
    for (i, id) in evicted.iter().enumerate() {
        assert_eq!(*id, entries[i].id, "oldest-first");
    }
}

#[test]
fn the_age_floor_wins_over_the_cap() {
    let ids = CountingIds::new();
    let entries: Vec<JournalEntry> = (0..JOURNAL_CAP + 10)
        .map(|i| entry_at(&ids, None, i64::try_from(i).unwrap()))
        .collect();
    // The incoming stamp one second after the newest: every entry is
    // younger than the 90-day floor, however far past the cap.
    let incoming = entries.last().unwrap().created_at + chrono::TimeDelta::seconds(1);
    assert_eq!(evict_journal(&entries, incoming), Vec::new());
}

#[test]
fn the_apply_arm_prunes_the_oldest_leaf_and_keeps_the_head() {
    let ids = CountingIds::new();
    let mut state = crate::persistence::PersistedState::default();
    // Seed the cap exactly at the limit via apply (all roots, old stamps).
    let old: Vec<PersistenceAction> = (0..JOURNAL_CAP)
        .map(|i| PersistenceAction::RecordJournal(entry_at(&ids, None, i64::try_from(i).unwrap())))
        .collect();
    apply_actions(&mut state, &old);
    let oldest = state.journal.entries[0].id;
    // One more entry, 90+ days later: the oldest leaf (a root, hence a
    // leaf) crosses both the cap and the floor.
    let fresh = entry_at(&ids, None, 100 * 86_400);
    apply_actions(
        &mut state,
        &[PersistenceAction::RecordJournal(fresh.clone())],
    );
    assert_eq!(state.journal.head, Some(fresh.id));
    assert_eq!(state.journal.entries.len(), JOURNAL_CAP);
    assert!(
        state.journal.entry(oldest).is_none(),
        "the oldest leaf was pruned"
    );
}

fn fixture_task() -> crate::entities::Task {
    crate::entities::Task {
        id: crate::ids::TaskId::from_uuid(uuid::Uuid::from_u128(1)),
        remote: None,
        title: "fixture".into(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack: crate::ids::StackId::from_uuid(uuid::Uuid::from_u128(2)),
        order: 0,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: crate::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: ts(100),
            description: ts(100),
            duedate: ts(100),
            scheduled: ts(100),
            done: ts(100),
            position: ts(100),
            effort: ts(100),
            importance: ts(100),
            urgency: ts(100),
            recur: ts(100),
            kind: ts(100),
            archived: ts(100),
            deleted: ts(100),
        },
        authors: crate::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

#[test]
#[allow(clippy::too_many_lines)] // the widened fixture rows ARE the test
fn journal_entry_round_trips_through_serde_all_facts_variants() {
    let ids = CountingIds::new();
    let mut task = fixture_task();
    task.id = ids.new_task_id();
    let not_undoable = JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command: crate::messages::StateCommand::SnoozeReminder {
            reminder: crate::reminders::ReminderRef::Default(task.id),
            occurrence: ts(100),
            postpone: chrono::TimeDelta::minutes(5),
        },
        facts: UndoFacts::NotUndoable(NotUndoableReason::ReminderSurface),
        ops: vec![ids.new_op_id()],
        pushed: true,
        board: None,
        created_at: ts(100),
    };
    let pre_images = UndoFacts::PreImages {
        images: vec![
            PreImage::Task {
                id: task.id,
                before: Some(Box::new(task.clone())),
            },
            PreImage::Stack {
                id: crate::ids::StackId::from_uuid(ids.new_stack_id().as_uuid()),
                before: None,
            },
            PreImage::HabitLogRow {
                key: crate::habit::HabitLogKey {
                    habit: crate::ids::HabitId::from_uuid(ids.new_habit_id().as_uuid()),
                    window: crate::habit::WindowStart::from_stored(
                        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                    ),
                },
                before: Some(crate::habit::HabitLogEntry {
                    count: 2,
                    clock: ts(100),
                    by: None,

                    skip: false,
                }),
            },
            PreImage::HabitLogRow {
                key: crate::habit::HabitLogKey {
                    habit: crate::ids::HabitId::from_uuid(ids.new_habit_id().as_uuid()),
                    window: crate::habit::WindowStart::from_stored(
                        chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
                    ),
                },
                before: None,
            },
        ],
        watermark: UndoWatermark(
            [
                (
                    JournalEntity::Task(task.id),
                    EntityStamps::Task {
                        clocks: task.clocks,
                        memberships: task.label_memberships.clone(),
                    },
                ),
                (
                    JournalEntity::HabitLog(crate::habit::HabitLogKey {
                        habit: crate::ids::HabitId::from_uuid(ids.new_habit_id().as_uuid()),
                        window: crate::habit::WindowStart::from_stored(
                            chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(),
                        ),
                    }),
                    EntityStamps::HabitLog(crate::habit::HabitLogEntry {
                        count: 5,
                        clock: ts(150),
                        by: None,

                        skip: false,
                    }),
                ),
            ]
            .into_iter()
            .collect(),
        ),
    };
    let undoable = JournalEntry {
        id: ids.new_journal_id(),
        parent: Some(not_undoable.id),
        command: crate::messages::StateCommand::CreateTask {
            title: "t".into(),
            stack: crate::ids::StackId::from_uuid(ids.new_stack_id().as_uuid()),
            order: 1,
        },
        facts: pre_images,
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(200),
    };
    for entry in [not_undoable, undoable] {
        let json = serde_json::to_string(&entry).unwrap();
        let back: JournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }
    // The new UndoField variants (error-payload vocabulary) round-trip.
    for field in [
        UndoField::HabitTarget,
        UndoField::HabitAnchorTime,
        UndoField::HabitLogCount,
    ] {
        let json = serde_json::to_string(&field).unwrap();
        let back: UndoField = serde_json::from_str(&json).unwrap();
        assert_eq!(back, field);
    }
}

#[test]
fn undo_view_derives_the_line_projection() {
    let ids = CountingIds::new();
    let mut journal = JournalState::default();
    let empty = UndoView::derive(&journal);
    assert!(!empty.can_undo);
    assert!(!empty.can_redo);
    assert_eq!(empty.head_label, None);
    assert_eq!(empty.recent, Vec::new());

    let first = entry_at(&ids, None, 100);
    journal.entries.push(first.clone());
    journal.head = Some(first.id);
    let single = UndoView::derive(&journal);
    assert!(single.can_undo);
    assert!(!single.can_redo, "no child yet");
    assert_eq!(single.recent.len(), 1);

    let second = entry_at(&ids, Some(first.id), 200);
    journal.entries.push(second.clone());
    journal.head = Some(second.id);
    let two = UndoView::derive(&journal);
    assert!(!two.can_redo, "head is the tip");
    // Undo moves the head back without a new node: the head then has a
    // child, so redo lights up.
    journal.head = Some(first.id);
    let retreating = UndoView::derive(&journal);
    assert!(retreating.can_undo);
    assert!(retreating.can_redo);
    // Recent is newest-first and bounded.
    assert_eq!(retreating.recent[0].id, second.id);
    for i in 0..UNDO_RECENT_WINDOW + 5 {
        let entry = entry_at(&ids, None, 300 + i64::try_from(i).unwrap());
        journal.entries.push(entry);
    }
    assert_eq!(UndoView::derive(&journal).recent.len(), UNDO_RECENT_WINDOW);
}

/// DR-7 D-J7: the reflection arm's serde name is the variant name
/// (consistent with the sibling surfaces), and the `command_label`
/// lockstep grew its three arms — compilation is the lockstep; the
/// labels themselves are presentation-only and unasserted beyond their
/// existence.
#[test]
fn reflection_surface_serde_name_pins_and_labels_carry_the_arms() {
    let json = serde_json::to_string(&NotUndoableReason::ReflectionSurface).unwrap();
    assert_eq!(json, r#""ReflectionSurface""#);
    let back: NotUndoableReason = serde_json::from_str(&json).unwrap();
    assert_eq!(back, NotUndoableReason::ReflectionSurface);
    for (command, _label) in [
        (
            crate::messages::StateCommand::RecordReflection {
                cadence: crate::reflection::ReviewCadence::Day,
                day: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
                window: (ts(100), ts(100 + 86_400)),
                text: String::new(),
                mood: None,
                energy: None,
                flags: std::collections::BTreeSet::new(),
            },
            "Record reflection",
        ),
        (
            crate::messages::StateCommand::UpdateReflection {
                id: crate::ids::ReflectionId::from(uuid::Uuid::from_u128(1)),
                text: None,
                mood: None,
                energy: None,
                flags_add: std::collections::BTreeSet::new(),
                flags_remove: std::collections::BTreeSet::new(),
            },
            "Edit reflection",
        ),
        (
            crate::messages::StateCommand::DeleteReflection {
                id: crate::ids::ReflectionId::from(uuid::Uuid::from_u128(1)),
            },
            "Delete reflection",
        ),
    ] {
        let label = command_label(&command);
        assert!(!label.is_empty(), "every arm carries a label");
    }
}

/// D-8 persisted-compatibility: the #356 arms decode byte-stably — a
/// journal entry carrying `RestoreStack` and the reclassified
/// `DeleteStack` round-trip through serde JSON unchanged (the new
/// `StateCommand` arm is appended, so no persisted payload renumbers).
/// US-1/S3 + US-2/S3 (UF1, W-3, US-1 AC-4's typed face): the
/// `command_label` lockstep carries the three rule-verb presentation
/// arms, and a journal headed by a rule-verb entry projects
/// `UndoView.head_label: Some(_)` — the label text itself is
/// presentation and never asserted (the journeys-suite law).
#[test]
fn us1_s3_command_label_arms_present() {
    let task = crate::ids::TaskId::from_uuid(uuid::Uuid::from_u128(1));
    let rule = crate::ids::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(2));
    let commands = [
        crate::messages::StateCommand::AddReminderRule {
            task,
            offset: crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30))
                .expect("valid"),
            at: None,
            insistence: crate::reminders::Insistence::Normal,
        },
        crate::messages::StateCommand::RemoveReminderRule { rule },
        crate::messages::StateCommand::ClearReminderRules { task },
    ];
    for command in commands {
        let label = command_label(&command);
        assert!(!label.is_empty(), "every rule-verb arm carries a label");
        let ids = CountingIds::new();
        let entry = JournalEntry {
            command: command.clone(),
            ..entry_at(&ids, None, 100)
        };
        let mut journal = JournalState::default();
        journal.entries.push(entry.clone());
        journal.head = Some(entry.id);
        let view = UndoView::derive(&journal);
        assert!(
            view.head_label.is_some(),
            "a rule-verb head projects a label: {command:?}"
        );
    }
}

#[test]
fn restore_stack_and_reclassified_delete_stack_entries_round_trip() {
    let ids = CountingIds::new();
    let stack = crate::ids::StackId::from_uuid(uuid::Uuid::from_u128(42));
    let mk = |command| JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command,
        facts: UndoFacts::PreImages {
            images: Vec::new(),
            watermark: crate::journal::UndoWatermark::default(),
        },
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(100),
    };
    let restore = mk(crate::messages::StateCommand::RestoreStack { id: stack });
    let delete = mk(crate::messages::StateCommand::DeleteStack { id: stack });
    for entry in [restore, delete] {
        let json = serde_json::to_string(&entry).expect("serializes");
        let back: JournalEntry = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back, entry);
    }
}

// ---- the per-board projection (#355, D-6) --------------------------------

/// The `derive_for_board` projection over a two-board journal: the scope
/// predicate (`entry.board == Some(board)`), the head gate, the sibling
/// rule for redo, and the per-board recent window — one behavior, one
/// test, lowest layer.
#[test]
fn derive_for_board_filters_can_undo_can_redo_and_the_recent_window() {
    let ids = CountingIds::new();
    let board_a = crate::ids::BoardId::from_uuid(uuid::Uuid::from_u128(11));
    let board_b = crate::ids::BoardId::from_uuid(uuid::Uuid::from_u128(22));
    let tagged = |board: Option<crate::ids::BoardId>, parent, stamp: i64| JournalEntry {
        board,
        ..entry_at(&ids, parent, stamp)
    };
    let mut journal = JournalState::default();
    // Empty line: the scoped question answers no for every board.
    for board in [board_a, board_b] {
        let view = UndoView::derive_for_board(&journal, board);
        assert!(!view.can_undo);
        assert!(!view.can_redo);
        assert_eq!(view.head_label, None);
    }
    // A two-board line: a-root → b-child → a-tip.
    let a_root = tagged(Some(board_a), None, 100);
    let b_child = tagged(Some(board_b), Some(a_root.id), 200);
    let a_tip = tagged(Some(board_a), Some(b_child.id), 300);
    journal.entries = vec![a_root.clone(), b_child.clone(), a_tip.clone()];
    journal.head = Some(a_tip.id);
    // Board A: head in scope, but the head has no child.
    let a_view = UndoView::derive_for_board(&journal, board_a);
    assert!(a_view.can_undo);
    assert!(!a_view.can_redo, "the tip has no child");
    assert!(a_view.head_label.is_some(), "label only when can_undo");
    // Board B: the head is out of scope — can_undo and head_label both
    // refuse; the tip has no child at all, so no redo either.
    let b_view = UndoView::derive_for_board(&journal, board_b);
    assert!(!b_view.can_undo);
    assert_eq!(b_view.head_label, None);
    assert!(!b_view.can_redo);
    // Retreat the head into the middle: A's head (a_root) has the
    // out-of-scope b_child — no in-scope child, so no redo for A.
    journal.head = Some(a_root.id);
    let a_mid = UndoView::derive_for_board(&journal, board_a);
    assert!(a_mid.can_undo);
    assert!(!a_mid.can_redo, "b_child is out of A's scope");
    let b_mid = UndoView::derive_for_board(&journal, board_b);
    assert!(!b_mid.can_undo);
    assert!(b_mid.can_redo, "b_child is in scope for B");
    // A None-tagged entry belongs to no board scope.
    let none_tip = tagged(None, Some(a_root.id), 400);
    journal.entries.push(none_tip.clone());
    journal.head = Some(none_tip.id);
    for board in [board_a, board_b] {
        let view = UndoView::derive_for_board(&journal, board);
        assert!(!view.can_undo, "a None-tagged head is out of scope");
        assert_eq!(view.head_label, None);
    }
    // The recent window is the per-board slice, still bounded: 12
    // A-tagged entries above the newest B/None tags.
    for i in 0..12i64 {
        journal.entries.push(tagged(Some(board_a), None, 500 + i));
    }
    let a_windowed = UndoView::derive_for_board(&journal, board_a);
    assert_eq!(a_windowed.recent.len(), UNDO_RECENT_WINDOW);
    assert_eq!(
        a_windowed.recent.first().map(|row| row.id),
        journal.entries.last().map(|entry| entry.id),
        "newest-first within the scope"
    );
    assert!(
        a_windowed.recent.iter().all(|row| journal
            .entry(row.id)
            .is_some_and(|entry| entry.board == Some(board_a))),
        "the window carries in-scope rows only"
    );
    // B's window sees nothing of A's twelve.
    assert_eq!(
        UndoView::derive_for_board(&journal, board_b)
            .recent
            .iter()
            .filter(|row| journal
                .entry(row.id)
                .is_some_and(|entry| entry.created_at >= ts(500)))
            .count(),
        0
    );
}

/// D-2's cheap insurance: the two appended arms' serde representation
/// pins (externally tagged, field-named) — appended, never inserted, so
/// the existing representations stay byte-stable.
#[test]
fn undo_on_board_and_redo_on_board_commands_round_trip() {
    use crate::messages::StateCommand;
    let board = crate::ids::BoardId::from_uuid(uuid::Uuid::from_u128(77));
    let undo = StateCommand::UndoOnBoard { board };
    let redo = StateCommand::RedoOnBoard { board };
    for command in [undo, redo] {
        let json = serde_json::to_string(&command).expect("serializes");
        let back: StateCommand = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back, command);
    }
}

// ---- UF2: the branch_children projection (#353) ------------------------

/// US-1/AC-1 (W-4): the head's children derive as typed sibling rows —
/// newest-first by the redo tie-break's `(created_at, id)` order, each
/// carrying its journaled command's engine-derived label (trap c: the
/// UI renders, never synthesizes).
#[test]
fn us1_ac1_branch_children_derive_with_labels() {
    let ids = CountingIds::new();
    let fork = entry_at(&ids, None, 100);
    let mut older = entry_at(&ids, Some(fork.id), 200);
    older.command = crate::messages::StateCommand::CreateTask {
        title: "older".into(),
        stack: crate::ids::StackId::from_uuid(uuid::Uuid::nil()),
        order: 1,
    };
    let mut newer = entry_at(&ids, Some(fork.id), 300);
    newer.command = crate::messages::StateCommand::MoveTask {
        id: crate::ids::TaskId::from_uuid(uuid::Uuid::nil()),
        stack: crate::ids::StackId::from_uuid(uuid::Uuid::nil()),
        order: 2,
    };
    let journal = JournalState {
        entries: vec![fork.clone(), older.clone(), newer.clone()],
        head: Some(fork.id),
    };
    let view = UndoView::derive(&journal);
    assert!(view.can_redo);
    let children: Vec<_> = view.branch_children.iter().map(|row| row.id).collect();
    assert_eq!(children, vec![newer.id, older.id], "newest sibling first");
    assert_eq!(view.branch_children[0].label, "Move task");
    assert_eq!(view.branch_children[1].label, "Create task");
}

/// US-1/AC-3 (W-4): no abandoned branch (the head has no children) —
/// the sibling section derives empty while the line's other facts stay
/// intact.
#[test]
fn us1_ac3_branch_children_empty_without_fork() {
    let ids = CountingIds::new();
    let only = entry_at(&ids, None, 100);
    let journal = JournalState {
        entries: vec![only.clone()],
        head: Some(only.id),
    };
    let view = UndoView::derive(&journal);
    assert_eq!(view.branch_children, [] as [UndoLabel; 0]);
    assert!(view.can_undo);
    assert!(!view.can_redo);
}

// ---- #1226: the payload enrichment (D1ii W-4, D-2/D-3) -----------------

/// US-1/AC-3 (D1ii): two sibling branches of the same verb carry
/// distinct typed payloads — the recognition contract (#1226). Pure
/// projection inequality, never label text.
#[test]
fn us1_ac3_sibling_rows_carry_distinct_payloads() {
    let ids = CountingIds::new();
    let fork = entry_at(&ids, None, 100);
    let fork_id = fork.id;
    let mut older = entry_at(&ids, Some(fork.id), 200);
    older.command = crate::messages::StateCommand::CreateTask {
        title: "alpha".into(),
        stack: crate::ids::StackId::from_uuid(uuid::Uuid::nil()),
        order: 1,
    };
    let mut newer = entry_at(&ids, Some(fork.id), 300);
    newer.command = crate::messages::StateCommand::CreateTask {
        title: "beta".into(),
        stack: crate::ids::StackId::from_uuid(uuid::Uuid::nil()),
        order: 2,
    };
    let journal = JournalState {
        entries: vec![fork, older, newer],
        head: Some(fork_id),
    };
    let view = UndoView::derive(&journal);
    assert_eq!(view.branch_children.len(), 2);
    assert_eq!(
        view.branch_children[0].payload.as_deref(),
        Some("beta"),
        "newest sibling first"
    );
    assert_eq!(view.branch_children[1].payload.as_deref(), Some("alpha"));
    assert_ne!(
        view.branch_children[0].payload, view.branch_children[1].payload,
        "same-verb siblings are distinguishable by typed payload"
    );
}

/// US-1/AC-5 (D1ii D-3): an arm with no natural subject in the
/// journaled command's own data derives `None` — never an empty
/// string.
#[test]
fn us1_ac5_subjectless_arm_derives_none_payload() {
    use crate::messages::StateCommand;
    let ids = CountingIds::new();
    let only = entry_at(&ids, None, 100);
    let subjectless = [
        StateCommand::TaskMoveUp {
            id: crate::ids::TaskId::from_uuid(uuid::Uuid::nil()),
        },
        StateCommand::StopTracking,
        StateCommand::RequestSync,
    ];
    for command in subjectless {
        let entry = JournalEntry {
            command: command.clone(),
            ..only.clone()
        };
        let journal = JournalState {
            entries: vec![entry.clone()],
            head: Some(entry.id),
        };
        let view = UndoView::derive(&journal);
        assert_eq!(view.recent.len(), 1);
        assert!(
            view.recent[0].payload.is_none(),
            "subjectless arm derives None: {command:?}"
        );
    }
}

/// US-2/S1 (D1ii D-2): the enrichment is one projection — the `recent`
/// and `branch_children` windows carry it from the same derivation.
#[test]
fn us2_s1_both_windows_carry_the_enrichment() {
    let ids = CountingIds::new();
    let head = entry_at(&ids, None, 100);
    let mut branch = entry_at(&ids, Some(head.id), 200);
    branch.command = crate::messages::StateCommand::CreateStack {
        board: crate::ids::BoardId::from_uuid(uuid::Uuid::nil()),
        title: "sibling stack".into(),
        order: 1,
    };
    let journal = JournalState {
        entries: vec![head.clone(), branch],
        head: Some(head.id),
    };
    let view = UndoView::derive(&journal);
    assert_eq!(
        view.recent.first().and_then(|row| row.payload.as_deref()),
        Some("sibling stack"),
        "newest-first window: the one subjectful row is the head of the window"
    );
    assert_eq!(
        view.branch_children
            .first()
            .and_then(|row| row.payload.as_deref()),
        Some("sibling stack")
    );
    // The per-board projection carries the same enrichment (one
    // derivation, D-2).
    let board = crate::ids::BoardId::from_uuid(uuid::Uuid::from_u128(9));
    let mut scoped = entry_at(&ids, None, 50);
    scoped.command = crate::messages::StateCommand::CreateLabel {
        board,
        title: "scoped label".into(),
        color: crate::entities::Color::new("0000ff"),
    };
    scoped.board = Some(board);
    let mut journal = journal;
    journal.entries.push(scoped);
    let view = UndoView::derive_for_board(&journal, board);
    assert_eq!(
        view.recent.last().and_then(|row| row.payload.as_deref()),
        Some("scoped label"),
        "oldest in the newest-first window is the scoped label row"
    );
}
