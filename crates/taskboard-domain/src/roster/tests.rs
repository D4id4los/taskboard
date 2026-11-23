// SPDX-License-Identifier: MIT OR Apache-2.0
//! The restore-roster projection's unit suite (the undo-family journey
//! plan W-1/W-8): tombstone derivation, the deleted-entry section's
//! pre-image scan, the absence/`None`-pre-image edges, and the roster's
//! honest bound. Step-ID-named (the journey plan's wiring rows), typed
//! assertions only.

use chrono::TimeZone;
use chrono::Utc;

use super::*;
use crate::command::CommandOutcome;
use crate::persistence::{PersistedState, apply_actions};
use crate::reminders::ReminderDefaults;
use crate::test_support::CountingIds;
use crate::undo::plan_journaled_command;

fn habit_target() -> crate::habit::HabitTarget {
    crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(1).expect("valid quota"))
}

/// The pure harness (the undo suite's shape, narrowed): plan → apply →
/// re-derive, one stepping clock.
struct Harness {
    disk: PersistedState,
    journal: JournalState,
    outbox: Vec<crate::outbox::PendingOp>,
    ids: CountingIds,
    now: i64,
}

impl Harness {
    fn new() -> Self {
        Self {
            disk: PersistedState::default(),
            journal: JournalState::default(),
            outbox: Vec::new(),
            ids: CountingIds::new(),
            now: 1_000_000,
        }
    }

    fn tick(&mut self) -> chrono::DateTime<Utc> {
        self.now += 60;
        Utc.timestamp_opt(self.now, 0).unwrap()
    }

    fn exec(&mut self, command: &StateCommand) -> CommandOutcome {
        let now = self.tick();
        let plan = plan_journaled_command(
            &AppState::from(self.disk.clone()),
            &self.outbox,
            &self.journal,
            command,
            &self.ids,
            &ReminderDefaults::default(),
            crate::days::DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
            },
            now,
            None,
        )
        .expect("planned");
        apply_actions(&mut self.disk, &plan.actions);
        self.outbox = self.disk.outbox.clone();
        self.journal = self.disk.journal.clone();
        plan.outcome
    }

    /// The published view over the current disk + journal.
    fn roster(&self) -> RestoreRoster {
        RestoreRoster::derive_parts(
            &self.disk.labels,
            &self.disk.habits,
            &self.disk.tracking,
            &self.journal,
        )
    }

    fn board(&mut self) -> crate::ids::BoardId {
        match self.exec(&StateCommand::CreateBoard {
            title: "board".into(),
            color: crate::entities::Color::new("0000ff"),
        }) {
            CommandOutcome::CreatedBoard(id) => id,
            other => panic!("expected CreatedBoard, got {other:?}"),
        }
    }

    fn stack(&mut self, board: crate::ids::BoardId) -> crate::ids::StackId {
        match self.exec(&StateCommand::CreateStack {
            board,
            title: "stack".into(),
            order: 1,
        }) {
            CommandOutcome::CreatedStack(id) => id,
            other => panic!("expected CreatedStack, got {other:?}"),
        }
    }

    fn task(&mut self, stack: crate::ids::StackId, title: &str) -> crate::ids::TaskId {
        match self.exec(&StateCommand::CreateTask {
            title: title.into(),
            stack,
            order: 1,
        }) {
            CommandOutcome::CreatedTask(id) => id,
            other => panic!("expected CreatedTask, got {other:?}"),
        }
    }

    fn label(&mut self, board: crate::ids::BoardId, title: &str) -> crate::ids::LabelId {
        match self.exec(&StateCommand::CreateLabel {
            board,
            title: title.into(),
            color: crate::entities::Color::new("00ff00"),
        }) {
            CommandOutcome::CreatedLabel(id) => id,
            other => panic!("expected CreatedLabel, got {other:?}"),
        }
    }

    fn habit(&mut self, board: crate::ids::BoardId, title: &str) -> crate::ids::HabitId {
        match self.exec(&StateCommand::CreateHabit {
            board,
            title: title.into(),
            target: habit_target(),
        }) {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    }
}

// ---- US-1/S1 / US-2/S1 (W-1): the tombstoned sections -------------------

/// US-1/S1 + US-2/S1: a tombstoned label and a tombstoned habit derive
/// roster rows with their typed ids and pre-tombstone names.
#[test]
fn us1_s1_restore_roster_derives_tombstoned_rows() {
    let mut h = Harness::new();
    let board = h.board();
    let _stack = h.stack(board);
    let label = h.label(board, "urgent");
    let habit = h.habit(board, "stretch");
    let roster = h.roster();
    assert!(
        roster.tombstoned_labels.is_empty() && roster.tombstoned_habits.is_empty(),
        "nothing deleted yet, the sections render absent"
    );
    h.exec(&StateCommand::DeleteLabel { id: label });
    h.exec(&StateCommand::DeleteHabit { habit });
    let roster = h.roster();
    assert_eq!(
        roster.tombstoned_labels,
        vec![RestoreRow {
            id: label,
            name: "urgent".into()
        }]
    );
    assert_eq!(
        roster.tombstoned_habits,
        vec![RestoreRow {
            id: habit,
            name: "stretch".into()
        }]
    );
}

/// US-1/S1: live rows stay out of the tombstoned sections; the roster
/// is bounded by the tombstone population and the journal ring (the
/// plan's bound assertion — restoring drains the section again).
#[test]
fn us1_s1_live_rows_stay_out_of_the_roster() {
    let mut h = Harness::new();
    let board = h.board();
    let _stack = h.stack(board);
    let live_label = h.label(board, "kept");
    let live_habit = h.habit(board, "kept-habit");
    let gone_label = h.label(board, "gone");
    h.exec(&StateCommand::DeleteLabel { id: gone_label });
    let roster = h.roster();
    assert_eq!(roster.tombstoned_labels.len(), 1);
    assert!(
        roster
            .tombstoned_labels
            .iter()
            .all(|row| row.id == gone_label),
        "the live label never derives a roster row"
    );
    assert!(roster.tombstoned_habits.is_empty(), "the habit is live");
    // The bound is honest both ways: restoring drains the section.
    h.exec(&StateCommand::RestoreLabel { id: gone_label });
    let roster = h.roster();
    assert_eq!(
        roster.tombstoned_labels,
        [] as [RestoreRow<crate::ids::LabelId>; 0]
    );
    assert_eq!(
        roster.tombstoned_habits,
        [] as [RestoreRow<crate::ids::HabitId>; 0]
    );
    let _ = (live_label, live_habit);
}

// ---- US-5/S1 (W-8): the deleted-entry section ---------------------------

/// US-5/S1: a journaled `DeleteTimeEntry` whose row is absent derives a
/// deleted-entry row with the delete's journal id and an engine-derived
/// label (the pre-image row's task title + span); the delete → restore
/// → re-delete pair leaves one honest row — the newest delete wins.
#[test]
fn us5_s1_deleted_entries_derive_from_delete_preimages() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "story");
    let start = Utc.timestamp_opt(400, 0).unwrap();
    let end = Utc.timestamp_opt(500, 0).unwrap();
    h.exec(&StateCommand::LogTime {
        task,
        start,
        end,
        kind: crate::tracking::TimeEntryKind::Work,
    });
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id });
    let roster = h.roster();
    assert_eq!(roster.deleted_entries.len(), 1, "the delete derives a row");
    let row = &roster.deleted_entries[0];
    assert_eq!(row.entry_id, entry_id);
    let delete_journal_id = h.journal.head.expect("the delete journaled");
    assert_eq!(row.journal_id, delete_journal_id);
    assert!(
        row.label.contains("story"),
        "the label names the snapshotted task title, got {:?}",
        row.label
    );
    // Restore drains the section (the row is present again).
    let pre_row = journal_pre_image_row(&h.journal, delete_journal_id);
    h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(pre_row),
    });
    assert_eq!(h.roster().deleted_entries, [] as [DeletedEntryRow; 0]);
    // Re-delete: one row again, now naming the *newest* delete's
    // journal id (the section never shows the stale pre-image).
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id });
    let roster = h.roster();
    assert_eq!(roster.deleted_entries.len(), 1);
    assert_eq!(
        roster.deleted_entries[0].journal_id,
        h.journal.head.expect("the re-delete journaled")
    );
}

/// US-5/S1 (AC-3): a present row (the delete undone, or the entry
/// re-created) and a `before: None` pre-image ghost stay out of the
/// section — the bounded honesty of the journey plan's N5 edge.
#[test]
fn us5_s1_absent_row_or_none_preimage_stays_out() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let start = Utc.timestamp_opt(400, 0).unwrap();
    let end = Utc.timestamp_opt(500, 0).unwrap();
    h.exec(&StateCommand::LogTime {
        task,
        start,
        end,
        kind: crate::tracking::TimeEntryKind::Work,
    });
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let delete_journal_id = h.journal.head.expect("not journaled yet");
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id });
    // The row re-appears (a plain undo of the delete, say): the journal
    // still carries the delete, but the present row stays out.
    let pre_row = journal_pre_image_row(&h.journal, h.journal.head.expect("delete journaled"));
    h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(pre_row),
    });
    assert!(
        h.roster().deleted_entries.is_empty(),
        "a present row never derives a restore row"
    );
    let _ = delete_journal_id;
    // The `before: None` ghost: a hand-shaped journal entry whose
    // command is a delete but whose capture carries no row (the
    // pre-#901 decode ghost's shape) — the derive skips it.
    let ghost = crate::journal::JournalEntry {
        id: crate::ids::JournalId::from_uuid(uuid::Uuid::now_v7()),
        parent: h.journal.head,
        command: StateCommand::DeleteTimeEntry { id: entry_id },
        facts: crate::journal::UndoFacts::PreImages {
            images: vec![crate::journal::PreImage::TimeEntry {
                id: entry_id,
                before: None,
            }],
            watermark: crate::journal::UndoWatermark::default(),
        },
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: h.tick(),
    };
    h.journal.entries.push(ghost);
    h.journal.head = Some(h.journal.entries.last().expect("the ghost appended").id);
    assert!(
        h.roster().deleted_entries.is_empty(),
        "a None-pre-image delete never derives a restore row"
    );
}

/// Reads the delete journal entry's `PreImage::TimeEntry` row (the
/// test-side twin of the engine's resolution; a fixture read, not an
/// oracle — the resolution law's tests live in the undo suite).
fn journal_pre_image_row(
    journal: &JournalState,
    id: crate::ids::JournalId,
) -> crate::tracking::TimeEntry {
    let entry = journal.entry(id).expect("the delete journaled");
    let StateCommand::DeleteTimeEntry { .. } = entry.command else {
        panic!("not a delete entry");
    };
    let crate::journal::UndoFacts::PreImages { images, .. } = &entry.facts else {
        panic!("no pre-images");
    };
    images
        .iter()
        .find_map(|image| match image {
            crate::journal::PreImage::TimeEntry {
                before: Some(row), ..
            } => Some((**row).clone()),
            _ => None,
        })
        .expect("the delete captured its row")
}
