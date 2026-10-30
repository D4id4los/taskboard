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
            archived: ts(100),
            deleted: ts(100),
        },
        remote_seen: None,
        remote_baseline: None,
    }
}

#[test]
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
        ],
        watermark: UndoWatermark(
            [(
                JournalEntity::Task(task.id),
                EntityStamps::Task {
                    clocks: task.clocks,
                    memberships: task.label_memberships.clone(),
                },
            )]
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
