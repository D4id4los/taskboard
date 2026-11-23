// SPDX-License-Identifier: MIT OR Apache-2.0
//! The undo planning layer's unit suite (ADR 0019 §14, domain layer):
//! wrapper contract, pre-images, watermark scopes, the synthesis
//! catalogue per undoable arm, line mechanics, and the pushed-edit
//! rule. One behavior, one test, lowest layer — pure functions over a
//! seeded `PersistedState`, fixed/stepping clocks, typed outcomes.

use chrono::TimeZone;

use super::*;
use crate::persistence::{PersistedState, apply_actions};
use crate::test_support::CountingIds;

fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc.timestamp_opt(secs, 0).unwrap()
}

/// The pure harness: a `PersistedState` the batches apply to, an
/// `AppState` view over it, a stepping clock (one step = 60s), and the
/// journal. `exec` advances memory exactly the way the engine does
/// (plan → apply → interpret the same batch).
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

    fn app(&self) -> AppState {
        let mut persisted = self.disk.clone();
        crate::state::derive_sync_states(
            &persisted.boards,
            &persisted.stacks,
            &persisted.tasks,
            &persisted.labels,
            &persisted.outbox,
            &mut persisted.sync,
        );
        AppState::from(persisted)
    }

    fn day() -> DayCalendar {
        DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
        }
    }

    fn tick(&mut self) -> chrono::DateTime<chrono::Utc> {
        self.now += 60;
        ts(self.now)
    }

    fn exec(&mut self, command: &StateCommand) -> Result<CommandOutcome, CommandError> {
        let now = self.tick();
        let plan = plan_journaled_command(
            &self.app(),
            &self.outbox,
            &self.journal,
            command,
            &self.ids,
            &crate::reminders::ReminderDefaults::default(),
            Self::day(),
            now,
            None,
        )?;
        apply_actions(&mut self.disk, &plan.actions);
        self.outbox = self.disk.outbox.clone();
        self.journal = self.disk.journal.clone();
        Ok(plan.outcome)
    }

    /// Executes with the raw plan visible (batch-shape assertions).
    fn exec_plan(&mut self, command: &StateCommand) -> Result<CommandPlan, CommandError> {
        let now = self.tick();
        let plan = plan_journaled_command(
            &self.app(),
            &self.outbox,
            &self.journal,
            command,
            &self.ids,
            &crate::reminders::ReminderDefaults::default(),
            Self::day(),
            now,
            None,
        )?;
        apply_actions(&mut self.disk, &plan.actions);
        self.outbox = self.disk.outbox.clone();
        self.journal = self.disk.journal.clone();
        Ok(plan)
    }

    /// Re-derives the harness views after a direct `disk` mutation in a
    /// fixture (the engine reads the persisted shape).
    fn sync(&mut self) {
        self.outbox = self.disk.outbox.clone();
        self.journal = self.disk.journal.clone();
    }

    fn board(&mut self) -> crate::ids::BoardId {
        match self
            .exec(&StateCommand::CreateBoard {
                title: "board".into(),
                color: crate::entities::Color::new("0000ff"),
            })
            .expect("board created")
        {
            CommandOutcome::CreatedBoard(id) => id,
            other => panic!("expected CreatedBoard, got {other:?}"),
        }
    }

    fn stack(&mut self, board: crate::ids::BoardId) -> crate::ids::StackId {
        match self
            .exec(&StateCommand::CreateStack {
                board,
                title: "stack".into(),
                order: 1,
            })
            .expect("stack created")
        {
            CommandOutcome::CreatedStack(id) => id,
            other => panic!("expected CreatedStack, got {other:?}"),
        }
    }

    fn task(&mut self, stack: crate::ids::StackId, title: &str) -> crate::ids::TaskId {
        match self
            .exec(&StateCommand::CreateTask {
                title: title.into(),
                stack,
                order: 1,
            })
            .expect("task created")
        {
            CommandOutcome::CreatedTask(id) => id,
            other => panic!("expected CreatedTask, got {other:?}"),
        }
    }

    fn label(&mut self, board: crate::ids::BoardId) -> crate::ids::LabelId {
        match self
            .exec(&StateCommand::CreateLabel {
                board,
                title: "label".into(),
                color: crate::entities::Color::new("00ff00"),
            })
            .expect("label created")
        {
            CommandOutcome::CreatedLabel(id) => id,
            other => panic!("expected CreatedLabel, got {other:?}"),
        }
    }
}

use crate::command::{CommandError, CommandOutcome, CommandPlan};
use crate::days::DayCalendar;
use crate::journal::{JournalState, NotUndoableReason};
use crate::messages::StateCommand;
use crate::state::AppState;

// ---- wrapper contract --------------------------------------------------

#[test]
fn an_accepted_mutation_appends_record_journal_last_with_the_prior_head() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let plan = h
        .exec_plan(&StateCommand::UpdateTask {
            id: task,
            changes: crate::messages::TaskChanges {
                title: Some("renamed".into()),
                ..Default::default()
            },
        })
        .expect("planned");
    let last = plan.actions.last().expect("non-empty batch");
    let PersistenceAction::RecordJournal(entry) = last else {
        panic!("the batch must end with RecordJournal, got {last:?}");
    };
    // parent = the prior head (the CreateTask entry), command verbatim.
    let prior_head = h.journal.entries[2].id;
    assert_eq!(entry.parent, Some(prior_head));
    assert_eq!(
        entry.command,
        StateCommand::UpdateTask {
            id: task,
            changes: crate::messages::TaskChanges {
                title: Some("renamed".into()),
                ..Default::default()
            }
        }
    );
    // ops = the batch's enqueues in order.
    let enqueued: Vec<_> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::EnqueueOp(op) => Some(op.op_id),
            _ => None,
        })
        .collect();
    assert_eq!(entry.ops, enqueued);
    assert!(!entry.pushed);
}

#[test]
fn an_empty_batch_journals_nothing() {
    let mut h = Harness::new();
    h.board();
    // RequestSync plans zero actions (and the engine early-returns it
    // before the wrapper anyway).
    let plan = h.exec_plan(&StateCommand::RequestSync).expect("planned");
    assert_eq!(plan.actions, Vec::new());
    // An accepted no-op (the empty changeset) plans zero actions.
    let board = *h.disk.boards.keys().next().unwrap();
    let stack = h.stack(board);
    let task = h.task(stack, "same");
    let plan = h
        .exec_plan(&StateCommand::UpdateTask {
            id: task,
            changes: crate::messages::TaskChanges::default(),
        })
        .expect("planned");
    assert!(plan.actions.is_empty(), "an empty changeset is a no-op");
    assert_eq!(
        h.journal.entries.len(),
        3,
        "the no-op journaled nothing (only the three creates)"
    );
}

#[test]
fn pre_images_capture_creates_and_overwrites() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    // The journal's last entry is the create: its pre-image is `None`.
    let create_entry = h.journal.entries.last().unwrap();
    let UndoFacts::PreImages { images, .. } = &create_entry.facts else {
        panic!("CreateTask journals PreImages");
    };
    assert!(
        images
            .iter()
            .any(|image| matches!(image, PreImage::Task { before: None, .. })),
        "a create captures before: None"
    );
    // The update's entry captures the pre entity.
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    let update_entry = h.journal.entries.last().unwrap();
    let UndoFacts::PreImages { images, .. } = &update_entry.facts else {
        panic!("UpdateTask journals PreImages");
    };
    let PreImage::Task {
        before: Some(pre), ..
    } = &images[0]
    else {
        panic!("an overwrite captures the pre entity");
    };
    assert_eq!(pre.title, "t");
}

fn habit_target() -> crate::habit::HabitTarget {
    crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(1).expect("valid quota"))
}

#[test]
fn log_time_capture_journals_the_minted_entry_id_and_pre_absence() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry = h.journal.entries.last().unwrap();
    let UndoFacts::PreImages { images, .. } = &entry.facts else {
        panic!("LogTime journals PreImages");
    };
    // The minted id is journaled — there is no other source for it.
    let minted = *h.disk.tracking.entries.keys().next().expect("entry row");
    assert_eq!(
        images.iter().find_map(|image| match image {
            PreImage::TimeEntry { id, before, .. } => Some((*id, before.is_none())),
            _ => None,
        }),
        Some((minted, true)),
        "the minted id rides the entry with before: None"
    );
}

#[test]
fn record_habit_log_capture_carries_the_pre_row_and_post_stamp() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    // Seed the window row (via LogHabit — journal-invertible since
    // 2026-11-21, habit-family UX plan R-7; the row it writes is the
    // adjust's pre-image source).
    h.exec(&StateCommand::LogHabit { habit })
        .expect("seeded the row");
    let key = *h.disk.habits.log.keys().next().expect("log row");
    let pre_row = h.disk.habits.log[&key];
    let seed_len = h.journal.entries.len();
    // An adjust onto the existing window: undoable, capture walks
    // `RecordHabitLog`.
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 5,
    })
    .expect("adjusted");
    let entry = h.journal.entries.last().unwrap();
    assert_eq!(h.journal.entries.len(), seed_len + 1);
    let UndoFacts::PreImages { images, watermark } = &entry.facts else {
        panic!("an existing-window adjust journals PreImages");
    };
    assert_eq!(
        images.iter().find_map(|image| match image {
            PreImage::HabitLogRow { key: k, before } => Some((*k, *before)),
            _ => None,
        }),
        Some((key, Some(pre_row))),
        "the pre row rides the entry"
    );
    let post_row = h.disk.habits.log[&key];
    assert_eq!(
        watermark.0.get(&JournalEntity::HabitLog(key)),
        Some(&EntityStamps::HabitLog(post_row)),
        "the post-write row is the watermark stamp"
    );
    // The no-op repeat (identical count) journals nothing.
    let before_len = h.journal.entries.len();
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 5,
    })
    .expect("accepted no-op");
    assert_eq!(h.journal.entries.len(), before_len);
}

#[test]
fn widened_noop_repeats_journal_nothing() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    let order = h.disk.habits.habits[&habit].order;
    let entry_id = {
        h.exec(&StateCommand::LogTime {
            task,
            start: ts(400),
            end: ts(500),
            kind: crate::tracking::TimeEntryKind::Work,
        })
        .expect("logged");
        *h.disk.tracking.entries.keys().next().expect("entry row")
    };
    let (entry_start, entry_end) = {
        let stored = &h.disk.tracking.entries[&entry_id];
        (stored.start, stored.end)
    };
    let before = h.journal.entries.len();
    h.exec(&StateCommand::MoveHabit { habit, order })
        .expect("accepted no-op");
    h.exec(&StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: Some(entry_start),
        end: entry_end,
        kind: None,
    })
    .expect("accepted no-op");
    h.exec(&StateCommand::UpdateHabit {
        habit,
        changes: crate::messages::HabitChanges::default(),
    })
    .expect("accepted no-op");
    assert_eq!(
        h.journal.entries.len(),
        before,
        "identical-order/unchanged-mask repeats plan empty batches"
    );
}

// ---- the widened round-trips (#555: verb → undo → pre-state → redo) -----

#[test]
fn log_time_round_trips() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let logged = h
        .exec(&StateCommand::LogTime {
            task,
            start: ts(400),
            end: ts(500),
            kind: crate::tracking::TimeEntryKind::Work,
        })
        .expect("logged");
    let CommandOutcome::TimeLogged { entry } = logged else {
        panic!("expected TimeLogged");
    };
    let entry_ops = h.journal.entries.last().unwrap().ops.clone();
    let plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    // Retraction: the un-pushed entry's op fails, nothing enqueues.
    let fails: Vec<_> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::FailOp(op) => Some(*op),
            _ => None,
        })
        .collect();
    assert_eq!(fails, entry_ops);
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::EnqueueOp(_)))
    );
    assert!(
        !h.disk.tracking.entries.contains_key(&entry.id),
        "the undo deleted the created entry"
    );
    // Redo re-mints: a fresh id, the same intent.
    h.exec(&StateCommand::Redo).expect("redone");
    let fresh = h
        .disk
        .tracking
        .entries
        .values()
        .next()
        .expect("re-minted entry");
    assert_ne!(fresh.id, entry.id, "the old id stays retired");
    assert_eq!(
        (fresh.task, fresh.start, fresh.end),
        (entry.task, entry.start, entry.end)
    );
    assert_eq!(fresh.task_title, entry.task_title);
}

#[test]
fn adjust_time_entry_round_trips_on_a_closed_entry() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry");
    let pre = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: Some(ts(420)),
        end: Some(ts(560)),
        kind: None,
    })
    .expect("adjusted");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tracking.entries[&entry_id];
    assert_eq!((after.start, after.end), (pre.start, pre.end));
    assert_eq!(after.updated_at, ts(h.now), "fresh write clock");
    // Redo re-applies the exact forward mask.
    h.exec(&StateCommand::Redo).expect("redone");
    let after = &h.disk.tracking.entries[&entry_id];
    assert_eq!((after.start, after.end), (ts(420), Some(ts(560))));
}

#[test]
fn adjust_open_entry_start_round_trips_and_stays_open() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::StartTracking {
        task,
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("started (journals NotUndoable, mints the open entry)");
    let entry_id = h
        .disk
        .tracking
        .entries
        .values()
        .find(|entry| entry.end.is_none())
        .expect("open entry")
        .id;
    let pre_start = h.disk.tracking.entries[&entry_id].start;
    h.exec(&StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: Some(ts(600)),
        end: None,
        kind: None,
    })
    .expect("adjusted");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tracking.entries[&entry_id];
    assert_eq!(after.start, pre_start);
    assert!(
        after.end.is_none(),
        "a start-only adjust of an open entry undoes without closing"
    );
}

#[test]
fn create_habit_round_trips() {
    let mut h = Harness::new();
    let board = h.board();
    let created = h
        .exec(&StateCommand::CreateHabit {
            board,
            title: "meditate".into(),
            target: habit_target(),
        })
        .expect("created");
    let CommandOutcome::HabitCreated(habit) = created else {
        panic!("expected HabitCreated");
    };
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(
        h.disk.habits.habits[&habit].deleted,
        "the undo tombstoned the created habit"
    );
    // Redo re-mints at the appended order (the old id stays retired).
    h.exec(&StateCommand::Redo).expect("redone");
    let fresh = h
        .disk
        .habits
        .habits
        .values()
        .find(|row| row.is_live())
        .expect("re-minted habit");
    assert_ne!(fresh.id, habit);
    assert_eq!((fresh.title.as_str(), fresh.board), ("meditate", board));
}

#[test]
fn update_habit_round_trips_over_all_three_clocked_fields() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "meditate".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    // Give the habit a chip first, so the big edit's anchor-clear is a
    // Some→None round-trip (the double-option discipline).
    h.exec(&StateCommand::UpdateHabit {
        habit,
        changes: crate::messages::HabitChanges {
            anchor_time: Some(Some(chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap())),
            ..Default::default()
        },
    })
    .expect("chip set");
    let pre = h.disk.habits.habits[&habit].clone();
    let week = crate::habit::HabitTarget::PerWeek(
        crate::habit::NonZeroCount::new(2).expect("valid quota"),
    );
    h.exec(&StateCommand::UpdateHabit {
        habit,
        changes: crate::messages::HabitChanges {
            title: Some("meditate 2".into()),
            target: Some(week),
            // `Some(None)` clears the chip.
            anchor_time: Some(None),
            ..Default::default()
        },
    })
    .expect("edited");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.habits.habits[&habit];
    assert_eq!(after.title, pre.title);
    assert_eq!(after.target, pre.target);
    assert_eq!(after.anchor_time, pre.anchor_time);
    assert_eq!(after.clocks.title, ts(h.now), "fresh clocks on restore");
    // Redo re-applies the exact forward changeset.
    h.exec(&StateCommand::Redo).expect("redone");
    let after = &h.disk.habits.habits[&habit];
    assert_eq!(after.title, "meditate 2");
    assert_eq!(after.target, week);
    assert_eq!(after.anchor_time, None);
}

#[test]
fn move_habit_round_trips_restoring_the_pre_image_order() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    let pre_order = h.disk.habits.habits[&habit].order;
    h.exec(&StateCommand::MoveHabit { habit, order: 42 })
        .expect("moved");
    // The pre-image itself carries the pre order position (never the
    // payload of any later edit).
    let UndoFacts::PreImages { images, .. } = &h.journal.entries.last().unwrap().facts else {
        panic!("MoveHabit journals PreImages");
    };
    let Some(PreImage::Habit {
        before: Some(pre), ..
    }) = images
        .iter()
        .find(|image| matches!(image, PreImage::Habit { id, .. } if *id == habit))
    else {
        panic!("the move captured the pre entity");
    };
    assert_eq!(pre.order, pre_order);
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.habits.habits[&habit];
    assert_eq!(after.order, pre_order, "the pre-image's order restored");
    assert_eq!(after.clocks.order, ts(h.now), "fresh order clock");
    h.exec(&StateCommand::Redo).expect("redone");
    assert_eq!(h.disk.habits.habits[&habit].order, 42);
}

#[test]
fn habit_reorder_undo_restores_the_section() {
    // The reorder inverse class (chunk A1 D1, #1198): the section's
    // pre-move orders (every touched row) restore as one journaled
    // inverse step — a single-row pre-image cannot express it.
    let mut h = Harness::new();
    let board = h.board();
    let mut ids = Vec::new();
    for k in 0..3 {
        let created = h
            .exec(&StateCommand::CreateHabit {
                board,
                title: format!("habit {k}"),
                target: habit_target(),
            })
            .expect("created");
        match created {
            CommandOutcome::HabitCreated(id) => ids.push(id),
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    }
    let seq = |h: &Harness| -> Vec<crate::ids::HabitId> {
        let mut rows: Vec<(i64, crate::ids::HabitId)> = h
            .disk
            .habits
            .habits
            .values()
            .filter(|habit| !habit.deleted)
            .map(|habit| (habit.order, habit.id))
            .collect();
        rows.sort();
        rows.into_iter().map(|(_, id)| id).collect()
    };
    let pre = seq(&h);
    h.exec(&StateCommand::HabitReorder {
        habit: ids[2],
        target: 0,
    })
    .expect("moved");
    assert_eq!(seq(&h), vec![ids[2], ids[0], ids[1]]);
    h.exec(&StateCommand::Undo).expect("undone");
    assert_eq!(seq(&h), pre, "the section's pre-move sequence restored");
    h.exec(&StateCommand::Redo).expect("redone");
    assert_eq!(seq(&h), vec![ids[2], ids[0], ids[1]]);
}

#[test]
fn adjust_habit_log_round_trips() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    // Seed the window row (1) — the seeding verbs are excluded arms;
    // they only lay the state the adjust under test edits.
    h.exec(&StateCommand::LogHabit { habit })
        .expect("seeded count 1");
    let key = *h.disk.habits.log.keys().next().expect("log row");
    let pre_row = h.disk.habits.log[&key];
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 5,
    })
    .expect("adjusted");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = h.disk.habits.log[&key];
    assert_eq!(after.count, pre_row.count, "the pre count restored");
    assert_eq!(after.clock, ts(h.now), "fresh row clock; the row stays");
    h.exec(&StateCommand::Redo).expect("redone");
    assert_eq!(h.disk.habits.log[&key].count, 5);
}

#[test]
fn log_time_undo_after_task_delete_still_deletes_the_entry() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry");
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("task deleted");
    // The delete's undo pops first (resurrect), then the log-time undo
    // proceeds: entry-only resolution (R-10) — a deleted task never
    // blocks history editing.
    h.exec(&StateCommand::Undo).expect("the delete undoes");
    h.exec(&StateCommand::Undo).expect("the log time undoes");
    assert!(
        !h.disk.tracking.entries.contains_key(&entry_id),
        "the entry deleted despite the task's tombstone history"
    );
}

#[test]
fn create_habit_undo_after_log_keeps_the_log() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    h.exec(&StateCommand::LogHabit { habit })
        .expect("logged (excluded arm — the row lands)");
    assert_eq!(h.disk.habits.log.len(), 1);
    // Park the head on the create entry — the line state a v2 branch
    // picker could produce (the tree is queryable, D-U4; the fixture
    // seam the occupancy test uses too).
    let create_entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::CreateHabit { .. }))
        .unwrap()
        .id;
    apply_actions(
        &mut h.disk,
        &[PersistenceAction::MoveJournalHead(Some(create_entry))],
    );
    h.sync();
    h.exec(&StateCommand::Undo).expect("the create undoes");
    // D26: no occupancy guard — the log map outlives the tombstone.
    assert!(h.disk.habits.habits[&habit].deleted);
    assert_eq!(
        h.disk.habits.log.len(),
        1,
        "the log row survives the create-undo untouched"
    );
}

// ---- the record-time dynamic refusals (§7) ------------------------------

#[test]
fn an_adjust_that_closes_an_open_entry_journals_not_undoable() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::StartTracking {
        task,
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("started");
    let entry_id = h
        .disk
        .tracking
        .entries
        .values()
        .find(|entry| entry.end.is_none())
        .expect("open entry")
        .id;
    let before = h.journal.entries.len();
    let closing_end = ts(h.now + 60);
    h.exec(&StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: None,
        end: Some(closing_end),
        kind: None,
    })
    .expect("accepted (the adjust itself is legal)");
    let entry = h.journal.entries.last().unwrap();
    assert_eq!(
        entry.facts,
        UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface),
        "the close is refused at record time (D23)"
    );
    let head = h.journal.head;
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::TrackingSurface
        ))
    ));
    assert_eq!(h.journal.head, head, "the refusal never moved the head");
    assert_eq!(h.journal.entries.len(), before + 1);
    assert!(
        h.disk.tracking.entries[&entry_id].end.is_some(),
        "the closed end stands"
    );
}

#[test]
fn an_absent_window_adjust_journals_not_undoable() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    let absent = crate::habit::WindowStart::of_period(
        crate::habit::PeriodKind::Day,
        chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
    );
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: absent,
        count: 3,
    })
    .expect("accepted (the backfill itself is legal)");
    let entry = h.journal.entries.last().unwrap();
    assert_eq!(
        entry.facts,
        UndoFacts::NotUndoable(NotUndoableReason::HabitSurface),
        "the absent window refuses at record time (D24)"
    );
    let head = h.journal.head;
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::HabitSurface
        ))
    ));
    assert_eq!(h.journal.head, head);
}

#[test]
fn a_seam_addressed_update_journals_not_undoable_for_the_whole_edit() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    // Mixed with a clocked change: the whole edit refuses, not just the
    // seam field — a partial restore would print "Undone" over an
    // unrestored field (D25).
    h.exec(&StateCommand::UpdateHabit {
        habit,
        changes: crate::messages::HabitChanges {
            title: Some("renamed".into()),
            continued_from: Some(Some(crate::ids::SeriesId::from_uuid(
                uuid::Uuid::from_u128(0x5e),
            ))),
            ..Default::default()
        },
    })
    .expect("accepted");
    let entry = h.journal.entries.last().unwrap();
    assert_eq!(
        entry.facts,
        UndoFacts::NotUndoable(NotUndoableReason::HabitSurface),
    );
    let head = h.journal.head;
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::HabitSurface
        ))
    ));
    assert_eq!(h.journal.head, head);
    assert_eq!(
        h.disk.habits.habits[&habit].title, "renamed",
        "the accepted edit stands"
    );
}

// ---- the watermark refusals over the new scopes (§11.4) -----------------

#[test]
fn a_created_habit_edited_since_refuses_the_undo_field_named() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    h.exec(&StateCommand::UpdateHabit {
        habit,
        changes: crate::messages::HabitChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("edited");
    // The first undo pops the edit (restoring the title with a fresh
    // clock — the create entry's recorded mint stamps have diverged).
    h.exec(&StateCommand::Undo).expect("the edit undoes");
    let err = h.exec(&StateCommand::Undo).expect_err("refused");
    assert!(matches!(
        err,
        CommandError::TargetNoLongerInvertible {
            field: UndoField::Title,
            ..
        }
    ));
    assert!(
        h.disk.habits.habits[&habit].is_live(),
        "state untouched by the refusal"
    );
}

#[test]
fn a_logged_entry_adjusted_since_refuses_the_log_time_undo() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry");
    h.exec(&StateCommand::AdjustTimeEntry {
        id: entry_id,
        start: None,
        end: Some(ts(560)),
        kind: None,
    })
    .expect("adjusted");
    h.exec(&StateCommand::Undo).expect("the adjust undoes");
    let err = h.exec(&StateCommand::Undo).expect_err("refused");
    assert!(matches!(
        err,
        CommandError::TargetNoLongerInvertible {
            field: UndoField::UpdatedAt,
            ..
        }
    ));
}

#[test]
fn a_log_window_written_since_refuses_the_older_adjust_undo() {
    let mut h = Harness::new();
    let board = h.board();
    let habit = {
        match h
            .exec(&StateCommand::CreateHabit {
                board,
                title: "habit".into(),
                target: habit_target(),
            })
            .expect("created")
        {
            CommandOutcome::HabitCreated(id) => id,
            other => panic!("expected HabitCreated, got {other:?}"),
        }
    };
    h.exec(&StateCommand::LogHabit { habit })
        .expect("seeded count 1");
    let key = *h.disk.habits.log.keys().next().expect("log row");
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 5,
    })
    .expect("first adjust");
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: key.window,
        count: 7,
    })
    .expect("second adjust");
    // The first undo pops the newest adjust (its own stamps stand).
    h.exec(&StateCommand::Undo)
        .expect("the newest adjust undoes");
    assert_eq!(h.disk.habits.log[&key].count, 5);
    // The older adjust's recorded post-row has diverged (the restore
    // wrote a fresh clock) — refuse, the v1 covered-field-edit echo.
    let err = h.exec(&StateCommand::Undo).expect_err("refused");
    assert!(matches!(
        err,
        CommandError::TargetNoLongerInvertible {
            field: UndoField::HabitLogCount,
            ..
        }
    ));
    assert_eq!(h.disk.habits.log[&key].count, 5, "state untouched");
}

// ---- the synthesis catalogue -------------------------------------------

#[test]
fn undo_of_create_retracts_delete_class_ops_and_moves_the_head() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let outbox_before = h.outbox.clone();
    // The head (the entry being undone) and its op list, captured
    // before the undo pops it.
    let undone_entry = h.journal.entries.last().unwrap().clone();
    let plan = h.exec_plan(&StateCommand::Undo).expect("undo planned");
    // Typed receipt names the undone entry (the CreateTask node).
    assert!(
        matches!(plan.outcome, CommandOutcome::Undone { .. }),
        "got {:?}",
        plan.outcome
    );
    // Retraction: every entry op is failed, zero fresh enqueues.
    let fails: Vec<_> = plan
        .actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::FailOp(op) => Some(*op),
            _ => None,
        })
        .collect();
    assert_eq!(fails, undone_entry.ops);
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::EnqueueOp(_))),
        "a retraction enqueues nothing"
    );
    // The task is tombstoned locally; the outbox op is gone.
    assert!(h.disk.tasks[&task].deleted);
    assert_eq!(h.disk.outbox.len(), outbox_before.len() - 1);
    // The head moved without a new node.
    assert_eq!(h.journal.head, undone_entry.parent);
    assert_eq!(h.journal.entries.len(), 3);
}

#[test]
fn undo_of_update_restores_pre_image_values_with_fresh_clocks() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let pre_clocks = h.disk.tasks[&task].clocks;
    let remote_seen_before = h.disk.tasks[&task].remote_seen;
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&task];
    assert_eq!(after.title, "t");
    // Fresh write clock on the restored field only.
    assert_eq!(after.clocks.title, ts(h.now));
    assert_eq!(after.clocks.description, pre_clocks.description);
    // Observation machinery never rewound (D-U3): remote_seen is
    // byte-identical through plan→undo (both leave it alone).
    assert_eq!(after.remote_seen, remote_seen_before);
}

#[test]
fn a_covered_field_edit_refuses_the_undo_typed() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("first".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    // A later write on the covered field (title).
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("second".into()),
            ..Default::default()
        },
    })
    .expect("updated again");
    // The first undo pops the newest edit off the head (it wrote
    // last — its own stamps still stand).
    h.exec(&StateCommand::Undo).expect("the newest edit undoes");
    // Now the first edit's recorded title stamp has diverged (the
    // undo-restored value carries a fresh clock): refuse.
    let err = h.exec(&StateCommand::Undo).expect_err("refused");
    assert!(matches!(
        err,
        CommandError::TargetNoLongerInvertible {
            field: UndoField::Title,
            ..
        }
    ));
    assert_eq!(h.disk.tasks[&task].title, "first", "state untouched");
}

#[test]
fn an_uncovered_field_edit_does_not_refuse_the_done_undo() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::SetTaskDone {
        id: task,
        done: true,
    })
    .expect("done");
    // An unrelated touch after the completion: a description edit —
    // outside the done-undo's checked scope (the `done` clock only).
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            description: Some("touched".into()),
            ..Default::default()
        },
    })
    .expect("touched");
    // The interposed edit pops first; the done-undo then proceeds.
    h.exec(&StateCommand::Undo).expect("the edit undoes");
    h.exec(&StateCommand::Undo).expect("undo proceeds");
    assert_eq!(h.disk.tasks[&task].done, None, "the flag restored");
    assert_eq!(
        h.disk.tasks[&task].description, "",
        "the description edit was undone too"
    );
}

#[test]
fn undo_of_set_done_reopens_but_one_way_facts_stay() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::SetTaskDone {
        id: task,
        done: true,
    })
    .expect("done");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&task];
    assert_eq!(after.done, None, "the completion flag restored");
    assert_eq!(after.clocks.done, ts(h.now), "fresh done clock");
    // D9: the undo never rewinds observation machinery — and the
    // watermark's `done`-only scope means the undo proceeds.
}

#[test]
fn undo_of_move_restores_stack_and_order() {
    let mut h = Harness::new();
    let board = h.board();
    let stack_a = h.stack(board);
    let stack_b = h.stack(board);
    let task = h.task(stack_a, "t");
    h.exec(&StateCommand::MoveTask {
        id: task,
        stack: stack_b,
        order: 5,
    })
    .expect("moved");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&task];
    assert_eq!((after.stack, after.order), (stack_a, 1));
    assert_eq!(after.clocks.position, ts(h.now));
}

#[test]
fn undo_of_task_compact_move_restores_the_order_through_move_task() {
    // US-14 (#1031): the compact-move pair shares `MoveTask`'s inverse
    // class — the pre-image row's (stack, order) restores through the
    // ordinary `MoveTask`, so the undo line carries one verb family
    // for positional edits.
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let _below = h.task(stack, "below");
    let subject = h.task(stack, "subject");
    // Widen the gap so the up slot is an interior midpoint (subject
    // pushed to 10, below stays at 1 → midpoint 5), then compact-move
    // the subject up.
    h.exec(&StateCommand::MoveTask {
        id: subject,
        stack,
        order: 10,
    })
    .expect("widened");
    h.exec(&StateCommand::TaskMoveUp { id: subject })
        .expect("moved up");
    assert_eq!(h.disk.tasks[&subject].order, 5, "the floor midpoint slot");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&subject];
    // The pre-image is the compact move's own pre-state (order 10 —
    // the widening landed before it), never a recomputation.
    assert_eq!(after.order, 10, "the pre-image order restored");
    assert_eq!(after.clocks.position, ts(h.now), "fresh position clock");
}

#[test]
fn undo_of_rename_stack_restores_the_title() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    h.exec(&StateCommand::RenameStack {
        id: stack,
        new_title: "renamed".into(),
    })
    .expect("renamed");
    h.exec(&StateCommand::Undo).expect("undone");
    assert_eq!(h.disk.stacks[&stack].title, "stack");
}

#[test]
fn undo_of_move_stack_restores_the_order() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    h.exec(&StateCommand::MoveStack {
        id: stack,
        order: 9,
    })
    .expect("moved");
    h.exec(&StateCommand::Undo).expect("undone");
    assert_eq!(h.disk.stacks[&stack].order, 1);
}

#[test]
fn undo_of_create_label_deletes_the_label() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let label = h.label(board);
    // An assignment after the create must not block (memberships are
    // not entity destruction — §3 row 11).
    h.exec(&StateCommand::AssignLabel { task, label })
        .expect("assigned");
    h.exec(&StateCommand::Undo)
        .expect("undo of the assign first");
    h.exec(&StateCommand::Undo)
        .expect("undo of the create proceeds");
    assert!(h.disk.labels[&label].deleted);
}

#[test]
fn undo_of_update_label_restores_both_fields() {
    let mut h = Harness::new();
    let board = h.board();
    let label = h.label(board);
    h.exec(&StateCommand::UpdateLabel {
        id: label,
        changes: crate::messages::LabelChanges {
            title: Some("new".into()),
            color: Some(crate::entities::Color::new("ff0000")),
        },
    })
    .expect("updated");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.labels[&label];
    assert_eq!(after.title, "label");
    assert_eq!(after.color.as_str(), "00ff00");
}

#[test]
fn undo_of_unassign_reassigns_and_of_assign_unassigns() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let label = h.label(board);
    // Unassign of an absent pair is the accepted no-op (terminal
    // repeat) — nothing journaled.
    let outcome = h
        .exec(&StateCommand::UnassignLabel { task, label })
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
    h.exec(&StateCommand::AssignLabel { task, label })
        .expect("assigned");
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(matches!(
        h.disk.tasks[&task].label_memberships.get(&label),
        Some(crate::entities::MembershipState::Removed { .. })
    ));
    // Redo the forward intent: assigned again (fresh stamp).
    h.exec(&StateCommand::Redo).expect("redone");
    assert!(matches!(
        h.disk.tasks[&task].label_memberships.get(&label),
        Some(crate::entities::MembershipState::Added { .. })
    ));
}

#[test]
fn undo_of_delete_resurrects_locally_when_never_pushed() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "pre-image");
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    h.exec(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&task];
    assert!(!after.deleted);
    assert_eq!(after.title, "pre-image", "the pre-image values ride back");
    assert_eq!(after.clocks.deleted, ts(h.now), "fresh tombstone clock");
    // No new op: the server never knew. (The unbound delete cancelled
    // the task's pending create op; the resurrect mints none. The
    // stack's own create op legitimately stands.)
    assert!(
        !h.disk
            .outbox
            .iter()
            .any(|pending| crate::outbox::op_targets_task(&pending.op, task)),
        "no task op survives or re-posts"
    );
}

#[test]
fn undo_of_pushed_delete_reposts_as_a_create() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "bound");
    // Push-bound: adopt a remote ref (the echo adoption shape), clear
    // the outbox, flip the entry's pushed flag (the R-U9 boundary).
    {
        let mut bound = h.disk.tasks[&task].clone();
        bound.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId::deck(7),
            stack: crate::ids::RemoteStackId::deck(8),
            card: crate::ids::RemoteCardId::deck(9),
        });
        h.disk.tasks.insert(task, bound);
        h.disk.outbox.clear();
        for entry in &mut h.disk.journal.entries {
            entry.pushed = true;
        }
        h.sync();
    }
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    // The delete push completes: the pending op lands, the entry flips
    // at the boundary (the R-U9 pass — simulated at the seam here).
    h.disk.outbox.clear();
    h.disk.journal.entries.last_mut().unwrap().pushed = true;
    h.sync();
    let outbox_before = h.disk.outbox.len();
    h.exec_plan(&StateCommand::Undo).expect("undone");
    let after = &h.disk.tasks[&task];
    assert!(!after.deleted);
    // A pushed delete undoes as a new edit: the re-POST create op.
    assert_eq!(h.disk.outbox.len(), outbox_before + 1);
    assert!(
        h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::CreateTask(t) if t == task)
        ),
        "Deck undelete is re-POST"
    );
}

#[test]
fn undo_of_create_stack_refuses_when_occupied() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    // A live task in the created stack: a create after the stack (the
    // natural shape; every later task carries no pre-image stamps the
    // watermark could see — presence, not clocks).
    let _squat = h.task(stack, "squat");
    // Park the head back onto the stack-create entry — the line state a
    // v2 branch picker could produce (the tree is queryable, D-U4).
    let stack_entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::CreateStack { .. }))
        .unwrap()
        .id;
    apply_actions(
        &mut h.disk,
        &[PersistenceAction::MoveJournalHead(Some(stack_entry))],
    );
    h.sync();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::UndoTargetHasDependents { stack }) if stack == stack
    ));
}

#[test]
fn undo_of_create_stack_refuses_after_a_rename() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    h.exec(&StateCommand::RenameStack {
        id: stack,
        new_title: "renamed".into(),
    })
    .expect("renamed");
    // Undo the rename: the title value restores, but with a fresh stamp
    // — the create entry's recorded title stamp has since diverged.
    h.exec(&StateCommand::Undo).expect("the rename undoes");
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible { .. })
    ));
}

#[test]
fn not_undoable_heads_refuse_and_name_the_reason() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    // Reminder surface.
    h.exec(&StateCommand::SnoozeReminder {
        reminder: crate::reminders::ReminderRef::Default(task),
        occurrence: ts(500),
        postpone: chrono::TimeDelta::minutes(5),
    })
    .expect("snoozed");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::ReminderSurface
        ))
    ));
    // The refusal must not move the head: undo the snooze refuses, the
    // prior entry stays reachable.
    // Tracking surface (StartTracking: structurally non-invertible per
    // ADR 0017; LogTime/CreateHabit joined the undoable catalogue in
    // #555 — the representatives moved with them, D30).
    h.exec(&StateCommand::StartTracking {
        task,
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("started");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::TrackingSurface
        ))
    ));
    // Habit surface (the absent-window `AdjustHabitLog` keeps its D24
    // fence — R-7's 2026-11-21 supersession removed `LogHabit` from the
    // fence only; CreateHabit is undoable now, so it is seeded first
    // and simply becomes part of the line).
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    let habit = *h.disk.habits.habits.keys().next().expect("habit row");
    h.exec(&StateCommand::AdjustHabitLog {
        habit,
        window: crate::habit::WindowStart::of_period(
            crate::habit::PeriodKind::Day,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
        ),
        count: 3,
    })
    .expect("the backfill itself is legal");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::HabitSurface
        ))
    ));
    // (`DeleteStack` left the restore-class family in #356's D-1
    // reclassification; #901 moved `DeleteLabel`/`DeleteTimeEntry`/
    // `DeleteHabit` to `PreImages` too — their heads now undo through
    // the restore machinery, pinned by the restore-family tests. The
    // retired `RestoreClass` reason's decode path is pinned by
    // `a_retired_restore_class_reason_still_refuses_with_its_recorded_reason`.)
    // The head never moved through all the refusals.
    assert_eq!(
        h.journal.head,
        Some(h.journal.entries.last().unwrap().id),
        "a refusal does not advance the line"
    );
}

#[test]
fn the_kind_switching_start_tracking_journals_the_tracking_surface() {
    // P-5 (#599): the same-task different-kind switch is an effectful
    // StartTracking — it journals one node classified
    // NotUndoable(TrackingSurface) exactly like every other mutating
    // tracking command (the old kind-blind arm returned a no-op that
    // journaled nothing, leaving the previous command as the head).
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    // The prior undoable command on the line: the create-task's
    // pre-images are head-worthy, so an undo *before* the switch
    // would target them.
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("renamed");
    // Seed the open Work entry directly (the engine's apply shape).
    h.disk.tracking.entries.insert(
        crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(7_001)),
        crate::tracking::TimeEntry {
            id: crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(7_001)),
            task,
            task_title: "t".into(),
            start: ts(999_000),
            end: None,
            created_at: ts(999_000),
            updated_at: ts(999_000),
            kind: crate::tracking::TimeEntryKind::Work,
            by: None,
        },
    );
    h.sync();
    h.exec(&StateCommand::StartTracking {
        task,
        kind: crate::tracking::TimeEntryKind::Respite,
    })
    .expect("switched");
    // The head moved onto the switch node and names the tracking
    // surface.
    let head_id = h.journal.head.expect("the switch journaled a node");
    let head = h.journal.entry(head_id).expect("the head is on the line");
    assert!(matches!(
        &head.command,
        StateCommand::StartTracking {
            kind: crate::tracking::TimeEntryKind::Respite,
            ..
        }
    ));
    assert!(matches!(
        head.facts,
        UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
    ));
    // Undo right after the switch refuses typed instead of undoing
    // the rename underneath.
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::TrackingSurface
        ))
    ));
}

// ---- line mechanics -----------------------------------------------------

#[test]
fn undo_moves_the_head_via_the_batch_and_redo_reapplies() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    let plan = h.exec_plan(&StateCommand::Undo).expect("undo planned");
    assert!(
        plan.actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::MoveJournalHead(_))),
        "the head move rides the undo batch (D4)"
    );
    assert_eq!(h.disk.tasks[&task].title, "t");
    h.exec(&StateCommand::Redo).expect("redone");
    assert_eq!(h.disk.tasks[&task].title, "renamed");
    assert_eq!(
        h.disk.tasks[&task].clocks.title,
        ts(h.now),
        "redo is a fresh edit"
    );
}

#[test]
fn nothing_to_undo_and_nothing_to_redo_on_the_empty_line_and_tip() {
    let mut h = Harness::new();
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::NothingToUndo)
    ));
    assert!(matches!(
        h.exec(&StateCommand::Redo),
        Err(CommandError::NothingToRedo)
    ));
    let board = h.board();
    let stack = h.stack(board);
    let _task = h.task(stack, "t");
    // At the tip: nothing to redo.
    assert!(matches!(
        h.exec(&StateCommand::Redo),
        Err(CommandError::NothingToRedo)
    ));
}

#[test]
fn a_new_edit_after_undos_forks_and_the_old_redo_path_dies() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "one");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("two".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    h.exec(&StateCommand::Undo).expect("undone");
    let retreat_head = h.journal.head.unwrap();
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("three".into()),
            ..Default::default()
        },
    })
    .expect("forked");
    // The new edit's parent is the head it forked from (the tree shape).
    let fork = h.journal.entries.last().unwrap();
    assert_eq!(fork.parent, Some(retreat_head));
    // D5: the fork retires the old branch's redo path — the head (the
    // fork) has no children, so redo is empty and "two" is dead.
    assert!(matches!(
        h.exec(&StateCommand::Redo),
        Err(CommandError::NothingToRedo)
    ));
    assert_eq!(h.disk.tasks[&task].title, "three");
}

#[test]
fn a_redo_that_reenqueues_crosses_the_push_boundary() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    h.task(stack, "t");
    // Undo the create (retraction), then redo: the re-planned create
    // mints a fresh task and enqueues a fresh op — the redo's writes
    // are heading to the server, so its batch must flip the entry's
    // flag past the push boundary (a redo is a new edit, always).
    h.exec(&StateCommand::Undo).expect("undone");
    let redo_plan = h.exec_plan(&StateCommand::Redo).expect("redone");
    assert!(
        redo_plan
            .actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::EnqueueOp(_)))
    );
    let target = h.journal.entries.last().unwrap().id;
    assert!(
        redo_plan.actions.iter().any(|action| matches!(
            action,
            PersistenceAction::MarkJournalPushed(id) if *id == target
        )),
        "the re-enqueueing redo carries its own boundary flip"
    );
    assert!(
        h.journal.entry(target).is_some_and(|entry| entry.pushed),
        "the flag survives in the applied journal"
    );
}

#[test]
fn the_pushed_flag_keeps_the_inverse_ops_and_drops_the_retraction() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed".into()),
            ..Default::default()
        },
    })
    .expect("updated");
    // The push boundary lands: the entry flips pushed (the R-U9 pass).
    for entry in &mut h.journal.entries {
        entry.pushed = true;
    }
    h.disk.journal = h.journal.clone();
    let outbox_before = h.disk.outbox.len();
    let plan = h.exec_plan(&StateCommand::Undo).expect("undo planned");
    assert!(
        plan.actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::EnqueueOp(_))),
        "a pushed undo enqueues the inverse as a new edit"
    );
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::FailOp(_))),
        "no retraction past the boundary"
    );
    assert_eq!(h.disk.outbox.len(), outbox_before + 1);
    assert_eq!(h.disk.tasks[&task].title, "t");
}

#[test]
fn engine_routed_commands_are_typed_rejections_in_the_catalogue() {
    let mut h = Harness::new();
    let board = h.board();
    // Direct dispatch through the plain catalogue is a typed rejection
    // (the wrapper is the only legal route). The scoped arms (#355)
    // ride the same rejection arm — engine-routed like Undo/Redo.
    for command in [
        StateCommand::Undo,
        StateCommand::Redo,
        StateCommand::UndoOnBoard { board },
        StateCommand::RedoOnBoard { board },
    ] {
        let err = crate::command::plan_command(
            &h.app(),
            &[],
            &command,
            &h.ids,
            &crate::reminders::ReminderDefaults::default(),
            Harness::day(),
            ts(h.now),
            None,
        )
        .expect_err("typed");
        assert!(matches!(err, CommandError::EngineRouted));
    }
}

// ---- journal_push_marks (R-U9) -------------------------------------------

#[test]
fn push_marks_flag_exactly_the_owners_of_completed_ops_once() {
    let ids = CountingIds::new();
    let op = ids.new_op_id();
    let mut a = JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command: StateCommand::RequestSync,
        facts: UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony),
        ops: vec![op],
        pushed: false,
        board: None,
        created_at: ts(100),
    };
    let mut b = a.clone();
    b.id = ids.new_journal_id();
    b.parent = Some(a.id);
    b.created_at = ts(200);
    let mut c = b.clone();
    c.id = ids.new_journal_id();
    c.parent = Some(b.id);
    c.ops = Vec::new();
    c.created_at = ts(300);
    a.pushed = true; // already past the boundary: never re-flagged
    let journal = JournalState {
        entries: vec![a.clone(), b.clone(), c],
        head: None,
    };
    let marks = journal_push_marks(&journal, &[PersistenceAction::CompleteOp(op)]);
    // Only b owns the completed op and is still unpushed; a is pushed,
    // c owns nothing. One mark, once.
    assert_eq!(marks, vec![PersistenceAction::MarkJournalPushed(b.id)]);
    // A batch without completions marks nothing.
    assert_eq!(journal_push_marks(&journal, &[]), Vec::new());
    // Double-completion of the same op dedups to one mark.
    let marks = journal_push_marks(
        &journal,
        &[
            PersistenceAction::CompleteOp(op),
            PersistenceAction::CompleteOp(op),
        ],
    );
    assert_eq!(marks.len(), 1);
}

// ---- the lockstep smoke ---------------------------------------------------

/// Walks every `StateCommand` arm through the wrapper against a seeded
/// state asserting *entry-or-noop only* — the classification table's
/// completeness smoke (the per-arm semantics live in the tests above
/// and in the catalogue's own suite). A newly added arm that skips this
/// table is exactly what the no-wildcard matches catch at compile time;
/// this test catches a row that journals when it must not (or the
/// reverse) at run time.
#[allow(clippy::too_many_lines)] // one arm per row; the table IS the test
#[test]
fn every_arm_journals_entry_or_nothing() {
    use crate::ids::TimeEntryId;
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let label = h.label(board);
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: crate::habit::HabitTarget::PerDay(
            crate::habit::NonZeroCount::new(1).expect("valid quota"),
        ),
    })
    .expect("habit");
    let habit_id = *h.disk.habits.habits.keys().next().expect("habit row");
    let entry_id: TimeEntryId = {
        h.exec(&StateCommand::LogTime {
            task,
            start: ts(400),
            end: ts(500),
            kind: crate::tracking::TimeEntryKind::Work,
        })
        .expect("logged");
        *h.disk.tracking.entries.keys().next().expect("entry row")
    };
    let rule_id = {
        h.exec(&StateCommand::AddReminderRule {
            task,
            offset: crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30))
                .expect("valid"),
            at: None,
            insistence: crate::reminders::Insistence::Normal,
        })
        .expect("rule");
        *h.disk.reminders.rules.keys().next().expect("rule row")
    };
    let alarm_id = {
        h.exec(&StateCommand::CreateAdhocAlarm {
            label: "alarm".into(),
            when: crate::reminders::AdhocWhen::In(chrono::TimeDelta::minutes(5)),
            insistence: crate::reminders::Insistence::Normal,
        })
        .expect("alarm");
        *h.disk.reminders.alarms.keys().next().expect("alarm row")
    };
    // The entry payload for the restore row below, captured before the
    // vec (the entry is hard-deleted mid-list by `DeleteTimeEntry`).
    let entry_payload = h.disk.tracking.entries[&entry_id].clone();
    let commands: Vec<StateCommand> = vec![
        StateCommand::CreateTask {
            title: "n".into(),
            stack,
            order: 2,
        },
        StateCommand::CaptureTask {
            title: "c".into(),
            board: Some(board),
        },
        StateCommand::UpdateTask {
            id: task,
            changes: crate::messages::TaskChanges {
                title: Some("u".into()),
                ..Default::default()
            },
        },
        StateCommand::SetTaskDone {
            id: task,
            done: true,
        },
        StateCommand::MoveTask {
            id: task,
            stack,
            order: 3,
        },
        StateCommand::DeleteTask { id: task },
        StateCommand::CreateStack {
            board,
            title: "s2".into(),
            order: 2,
        },
        StateCommand::RenameStack {
            id: stack,
            new_title: "s-renamed".into(),
        },
        StateCommand::MoveStack {
            id: stack,
            order: 2,
        },
        StateCommand::CreateLabel {
            board,
            title: "l2".into(),
            color: crate::entities::Color::new("0000ff"),
        },
        StateCommand::UpdateLabel {
            id: label,
            changes: crate::messages::LabelChanges {
                title: Some("l-renamed".into()),
                color: None,
            },
        },
        StateCommand::DeleteLabel { id: label },
        StateCommand::AssignLabel { task, label },
        StateCommand::UnassignLabel { task, label },
        StateCommand::CreateBoard {
            title: "b2".into(),
            color: crate::entities::Color::new("0000ff"),
        },
        StateCommand::BindBoard {
            board,
            remote: crate::ids::RemoteBoardId::deck(1),
            connector: crate::ids::ConnectorId::from("test"),
        },
        StateCommand::RenameBoard {
            board,
            title: "b-renamed".into(),
        },
        StateCommand::SetBoardColor {
            board,
            color: crate::entities::Color::new("ff0000"),
        },
        StateCommand::SetBoardArchived {
            board,
            archived: true,
        },
        StateCommand::SnoozeReminder {
            reminder: crate::reminders::ReminderRef::Default(task),
            occurrence: ts(500),
            postpone: chrono::TimeDelta::minutes(5),
        },
        StateCommand::SetTaskRemindersMuted { task, muted: true },
        StateCommand::DismissReminder {
            reminder: crate::reminders::ReminderRef::Default(task),
            occurrence: ts(999),
        },
        StateCommand::AddReminderRule {
            task,
            offset: crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-15))
                .expect("valid"),
            at: None,
            insistence: crate::reminders::Insistence::Normal,
        },
        StateCommand::RemoveReminderRule { rule: rule_id },
        StateCommand::ClearReminderRules { task },
        StateCommand::CreateAdhocAlarm {
            label: "a2".into(),
            when: crate::reminders::AdhocWhen::In(chrono::TimeDelta::minutes(9)),
            insistence: crate::reminders::Insistence::Normal,
        },
        StateCommand::CancelAdhocAlarm { alarm: alarm_id },
        StateCommand::StartTracking {
            task,
            kind: crate::tracking::TimeEntryKind::Work,
        },
        StateCommand::StopTracking,
        StateCommand::LogTime {
            task,
            start: ts(600),
            end: ts(700),
            kind: crate::tracking::TimeEntryKind::Work,
        },
        StateCommand::AdjustTimeEntry {
            id: entry_id,
            start: None,
            end: Some(ts(650)),
            kind: None,
        },
        StateCommand::DeleteTimeEntry { id: entry_id },
        StateCommand::CreateHabit {
            board,
            title: "h2".into(),
            target: crate::habit::HabitTarget::PerWeek(
                crate::habit::NonZeroCount::new(2).expect("valid quota"),
            ),
        },
        StateCommand::UpdateHabit {
            habit: habit_id,
            changes: crate::messages::HabitChanges {
                title: Some("h-renamed".into()),
                ..Default::default()
            },
        },
        StateCommand::MoveHabit {
            habit: habit_id,
            order: 2,
        },
        StateCommand::LogHabit { habit: habit_id },
        StateCommand::AdjustHabitLog {
            habit: habit_id,
            window: crate::habit::WindowStart::of_period(
                crate::habit::PeriodKind::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 26).unwrap(),
            ),
            count: 3,
        },
        StateCommand::DeleteHabit { habit: habit_id },
        StateCommand::RestoreTask {
            id: task,
            resurrect: Box::new(h.disk.tasks[&task].clone()),
        },
        // The #901 restore-family rows: the three deletes journaled
        // PreImages above; the restore verbs (each a journaled PreImages
        // entry of its own write) ride here, after the earlier arms made
        // their targets tombstoned/absent.
        StateCommand::RestoreLabel { id: label },
        StateCommand::RestoreHabit { id: habit_id },
        StateCommand::RestoreTimeEntry {
            entry: Box::new(entry_payload),
        },
        // The destructive ceremonies go last: they hard-remove the rows
        // the earlier arms read fixtures from.
        StateCommand::UnbindBoard { board },
        StateCommand::ReconcileByTitle { board },
        StateCommand::DeleteStack { id: stack },
        // The tombstoned stack takes the restore verb (the #356 row:
        // accepted, journals one PreImages entry, stack live again).
        StateCommand::RestoreStack { id: stack },
        StateCommand::ResetBoard { board },
        StateCommand::DeleteBoard { board },
        // The engine-routed scoped arms (#355): like Undo/Redo, they
        // route to the line and journal nothing — the lockstep table's
        // two "nothing" rows; the line mechanics live in the scoped
        // tests above. Skipped here for the same reason as Undo/Redo.
        StateCommand::UndoOnBoard { board },
        StateCommand::RedoOnBoard { board },
        StateCommand::RequestSync,
    ];
    for command in &commands {
        // Undo/Redo (and their scoped #355 twins) route to the line (no
        // journaling classification) — exercised in the line-mechanics
        // tests; skip here.
        if matches!(
            command,
            StateCommand::Undo
                | StateCommand::Redo
                | StateCommand::UndoOnBoard { .. }
                | StateCommand::RedoOnBoard { .. }
        ) {
            continue;
        }
        let before = h.journal.entries.len();
        // Rejections are legal here (a terminal repeat, an unassign of
        // an absent pair); the smoke asserts only the *journaling*
        // contract on acceptance.
        if let Ok(plan) = h.exec_plan(command) {
            let after = h.journal.entries.len();
            if plan.actions.is_empty() {
                assert_eq!(
                    after, before,
                    "an empty batch journals nothing: {command:?}"
                );
            } else {
                assert_eq!(
                    after,
                    before + 1,
                    "an accepted mutating command journals exactly one entry: {command:?}"
                );
                assert!(
                    matches!(
                        plan.actions.last(),
                        Some(PersistenceAction::RecordJournal(_))
                    ),
                    "RecordJournal rides last: {command:?}"
                );
            }
        }
    }
}

// ---- RestoreTask's own three cases (the planner arm) ----------------------

#[test]
fn restore_task_on_a_live_target_is_the_accepted_noop() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let outcome = h
        .exec(&StateCommand::RestoreTask {
            id: task,
            resurrect: Box::new(h.disk.tasks[&task].clone()),
        })
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
}

#[test]
fn restore_task_with_a_pending_delete_op_retracts_without_a_new_op() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    // A pushed (bound) task: adopt a remote ref and clear the standing
    // ops so the delete enqueues exactly one pending delete op.
    {
        let mut bound = h.disk.tasks[&task].clone();
        bound.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId::deck(7),
            stack: crate::ids::RemoteStackId::deck(8),
            card: crate::ids::RemoteCardId::deck(9),
        });
        h.disk.tasks.insert(task, bound);
        h.disk.outbox.clear();
        h.sync();
    }
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    let pending_delete = h
        .disk
        .outbox
        .iter()
        .find(|pending| matches!(pending.op, crate::outbox::LocalOp::DeleteTask(t) if t == task))
        .map(|pending| pending.op_id)
        .expect("the delete op is pending");
    let outbox_len = h.disk.outbox.len();
    h.exec(&StateCommand::RestoreTask {
        id: task,
        resurrect: Box::new(h.disk.tasks[&task].clone()),
    })
    .expect("restored");
    assert!(!h.disk.tasks[&task].deleted);
    assert!(
        !h.disk
            .outbox
            .iter()
            .any(|pending| pending.op_id == pending_delete),
        "the pending delete failed"
    );
    assert_eq!(
        h.disk.outbox.len(),
        outbox_len - 1,
        "no new op minted (pure retraction)"
    );
}

#[test]
fn restore_task_of_an_unknown_id_is_typed() {
    let mut h = Harness::new();
    let stranger = crate::ids::TaskId::from_uuid(h.ids.new_task_id().as_uuid());
    let err = h.exec(&StateCommand::RestoreTask {
        id: stranger,
        resurrect: Box::new(crate::entities::Task {
            id: stranger,
            remote: None,
            title: "ghost".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: crate::ids::StackId::from_uuid(h.ids.new_stack_id().as_uuid()),
            order: 1,
            label_memberships: std::collections::BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: crate::entities::TaskKind::Standard,
            archived: false,
            deleted: true,
            clocks: crate::entities::TaskClocks {
                title: ts(1),
                description: ts(1),
                duedate: ts(1),
                scheduled: ts(1),
                done: ts(1),
                position: ts(1),
                effort: ts(1),
                importance: ts(1),
                urgency: ts(1),
                recur: ts(1),
                kind: ts(1),
                archived: ts(1),
                deleted: ts(1),
            },
            authors: crate::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }),
    });
    assert!(matches!(err, Err(CommandError::UnknownTask(_))));
}

// ---- the tracker task kind (backlog round 2 chunk 4.2, #492) ------

#[test]
fn undo_restores_a_kind_edit_and_the_watermark_names_it() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            kind: Some(Some(crate::entities::TaskKind::Tracker)),
            ..Default::default()
        },
    })
    .expect("converted");
    h.exec(&StateCommand::Undo).expect("the kind edit undoes");
    assert_eq!(
        h.disk.tasks[&task].kind,
        crate::entities::TaskKind::Standard,
        "the kind restores"
    );
    // The watermark names the field: a kind clock diverged since the
    // recorded write refuses typed, with `Kind` in the payload.
    let mut diverged = h.disk.clone();
    let entry = diverged.tasks.get_mut(&task).unwrap();
    entry.kind = crate::entities::TaskKind::Tracker;
    entry.clocks.kind = ts(1_002_000);
    h.disk = diverged;
    h.sync();
    let err = h.exec(&StateCommand::Undo).expect_err("refused");
    assert!(matches!(
        err,
        CommandError::TargetNoLongerInvertible {
            field: UndoField::Kind,
            ..
        }
    ));
}

#[test]
fn undoing_past_a_conversion_never_recompletes_a_tracker() {
    // The LIFO walk: done → reopen → convert → undo pops the
    // *conversion* first (a legal kind restore), so the reopen's
    // inverse replans against the restored `Standard` kind and
    // succeeds — the journal can never produce a re-completion of a
    // tracker, because the kind restore always precedes the replan.
    // The uniformity of the guard at the journal boundary is pinned
    // separately below (the stage-1 plan's D-13 refusal scenario is
    // unreachable through plain LIFO undo — a recorded correction).
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::SetTaskDone {
        id: task,
        done: true,
    })
    .expect("done");
    h.exec(&StateCommand::SetTaskDone {
        id: task,
        done: false,
    })
    .expect("reopened");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            kind: Some(Some(crate::entities::TaskKind::Tracker)),
            ..Default::default()
        },
    })
    .expect("converted");
    h.exec(&StateCommand::Undo).expect("the conversion undoes");
    assert_eq!(
        h.disk.tasks[&task].kind,
        crate::entities::TaskKind::Standard,
        "the kind restored"
    );
    // The reopen's inverse replans a completion — against the restored
    // standard kind it succeeds.
    h.exec(&StateCommand::Undo).expect("the reopen undoes");
    assert!(h.disk.tasks[&task].done.is_some());

    // The boundary pin (D-5 row 7's uniformity): a journal-level
    // replan of a completion against a tracker task hits the engine's
    // guard typed — the wrapper does not bypass it.
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            kind: Some(Some(crate::entities::TaskKind::Tracker)),
            ..Default::default()
        },
    })
    .expect("converted");
    let entries_before = h.journal.entries.len();
    let head_before = h.journal.head;
    let err = h
        .exec(&StateCommand::SetTaskDone {
            id: task,
            done: true,
        })
        .expect_err("the guard applies at the journal boundary");
    assert!(matches!(
        err,
        CommandError::TrackerTaskNotDoneable(id) if id == task
    ));
    assert_eq!(h.journal.entries.len(), entries_before);
    assert_eq!(h.journal.head, head_before, "the head keeps its line");
    assert_eq!(
        h.disk.tasks[&task].kind,
        crate::entities::TaskKind::Tracker,
        "state untouched by the refusal"
    );
}

// ---- the property: same-verb lines undo all the way back ------------------

proptest::proptest! {
    #![proptest_config(crate::test_support::proptest_config(64))]

    #[test]
    fn random_distinct_field_lines_undo_all_the_way_back(
        edits in proptest::collection::vec((0u8..4, "[a-z][a-z0-9 ]{0,11}"), 1..5),
    ) {
        // Keep the first edit per field: any second write to a field
        // makes the earlier edit's undo refuse by design (R-U7), which
        // the interleaved-refusal property covers separately.
        let mut seen = std::collections::BTreeSet::new();
        let edits: Vec<_> = edits
            .into_iter()
            .filter(|(kind, _)| seen.insert(*kind))
            .collect();
        let mut h = Harness::new();
        let board = h.board();
        let stack = h.stack(board);
        let task = h.task(stack, "origin");
        let original = h.disk.tasks[&task].clone();
        for (kind, text) in &edits {
            let command = match kind {
                0 => StateCommand::UpdateTask {
                    id: task,
                    changes: crate::messages::TaskChanges {
                        title: Some(text.clone()),
                        ..Default::default()
                    },
                },
                1 => StateCommand::UpdateTask {
                    id: task,
                    changes: crate::messages::TaskChanges {
                        description: Some(text.clone()),
                        ..Default::default()
                    },
                },
                2 => StateCommand::MoveTask {
                    id: task,
                    stack,
                    order: 7,
                },
                _ => StateCommand::SetTaskDone { id: task, done: true },
            };
            h.exec(&command).expect("accepted");
        }
        // Undo all the way back over the edit entries (the seeded
        // board/stack/task creates stay).
        let guard = h.journal.entries.len() - 3;
        for _ in 0..guard {
            h.exec(&StateCommand::Undo).expect("undo proceeds: fields are disjoint");
        }
        let restored = h.disk.tasks[&task].clone();
        // Field-level pre-image restoration; the observation fields
        // never moved (D-U3).
        assert_eq!(restored.title, original.title);
        assert_eq!(restored.description, original.description);
        assert_eq!(restored.stack, original.stack);
        assert_eq!(restored.order, original.order);
        assert_eq!(restored.done, original.done);
        assert_eq!(restored.remote_seen, original.remote_seen);
        assert_eq!(restored.remote_baseline, original.remote_baseline);
        // Redo re-applies every edit forward, fresh.
        for _ in 0..guard {
            h.exec(&StateCommand::Redo).expect("redo planned");
        }
        assert_eq!(h.journal.head, h.journal.entries.last().map(|e| e.id));
    }

    #[test]
    fn an_interleaved_covered_write_is_a_typed_refusal_never_corruption(
        title_a in "[a-z]{1,8}",
        title_b in "[a-z]{1,8}",
        title_c in "[a-z]{1,8}",
    ) {
        proptest::prelude::prop_assume!(title_a != title_b && title_b != title_c);
        let mut h = Harness::new();
        let board = h.board();
        let stack = h.stack(board);
        let task = h.task(stack, "base");
        for title in [&title_a, &title_b, &title_c] {
            h.exec(&StateCommand::UpdateTask {
                id: task,
                changes: crate::messages::TaskChanges {
                    title: Some(title.clone()),
                    ..Default::default()
                },
            })
            .unwrap();
        }
        // Undo pops C (its own stamps stand). B's recorded title stamp
        // has since diverged — C's undo wrote the restored value with a
        // fresh clock — so the next undo refuses typed, and every byte
        // stays exactly where the last successful operation left it.
        h.exec(&StateCommand::Undo).expect("the newest edit undoes");
        let err = h.exec(&StateCommand::Undo);
        assert!(matches!(
            err,
            Err(CommandError::TargetNoLongerInvertible { .. })
        ));
        assert_eq!(h.disk.tasks[&task].title, title_b);
        assert_eq!(
            h.journal.head,
            Some(h.journal.entries[h.journal.entries.len() - 2].id),
            "the head did not move past the pop"
        );
    }
}

// ---- the series pause (issue #506): one-field edit, UpdateTask
// inverse -----------------------------------------------------------

fn daily_rule() -> crate::recur::RecurRule {
    crate::recur::RecurRule {
        series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(9_000)),
        anchor: chrono::NaiveDate::from_ymd_opt(2026, 6, 1).unwrap(),
        pattern: crate::recur::RecurPattern::Daily {
            every: crate::recur::EveryDays::new(1).unwrap(),
        },
        bound: crate::recur::RecurBound::None,
        continued_from: None,
        paused_until: None,
    }
}

fn recur_task(h: &mut Harness, title: &str) -> (crate::ids::TaskId, crate::recur::RecurRule) {
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, title);
    let rule = daily_rule();
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            recur: Some(Some(rule.clone())),
            ..Default::default()
        },
    })
    .expect("rule set");
    (task, rule)
}

#[test]
fn a_pause_journals_pre_images_and_undoes_to_the_pre_pause_rule() {
    let mut h = Harness::new();
    let (task, rule) = recur_task(&mut h, "chore");
    let until = chrono::NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
    h.exec(&StateCommand::PauseSeries {
        head: task,
        until: Some(until),
    })
    .expect("paused");
    // The journaled facts are PreImages with the pre-pause rule.
    let entry = h.journal.entries.last().unwrap();
    let UndoFacts::PreImages { images, .. } = &entry.facts else {
        panic!("PauseSeries journals PreImages");
    };
    let PreImage::Task {
        before: Some(pre), ..
    } = &images[0]
    else {
        panic!("the pause captures the pre entity");
    };
    assert_eq!(pre.recur.as_ref().unwrap().paused_until, None);
    // Undo restores the pre-pause rule byte-for-byte.
    h.exec(&StateCommand::Undo).expect("undone");
    let restored = h.disk.tasks[&task].recur.clone().unwrap();
    assert_eq!(restored, rule);
    // Redo re-applies the pause.
    h.exec(&StateCommand::Redo).expect("redone");
    let redone = h.disk.tasks[&task].recur.clone().unwrap();
    assert_eq!(redone.paused_until, Some(until));
    assert_eq!(redone.pattern, rule.pattern, "only the pause field moves");
}

#[test]
fn a_pause_with_a_later_rule_edit_refuses_typed() {
    // The watermark scope is the recur clock: a concurrent pattern
    // edit after the pause diverges the clock, so the undo refuses
    // instead of restoring a stale rule (R-U7).
    let mut h = Harness::new();
    let (task, _) = recur_task(&mut h, "chore");
    let until = chrono::NaiveDate::from_ymd_opt(2026, 7, 10).unwrap();
    h.exec(&StateCommand::PauseSeries {
        head: task,
        until: Some(until),
    })
    .expect("paused");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            recur: Some(Some({
                let mut edited = daily_rule();
                edited.paused_until = Some(until);
                edited.pattern = crate::recur::RecurPattern::Daily {
                    every: crate::recur::EveryDays::new(2).unwrap(),
                };
                edited
            })),
            ..Default::default()
        },
    })
    .expect("rule edited after the pause");
    // The later edit undoes first (its own clock stands).
    h.exec(&StateCommand::Undo)
        .expect("the later rule edit undoes");
    // Undoing the pause now would restore a stale rule across the
    // edited recur clock (rewritten by the restore) — refuse typed.
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible { .. })
    ));
}

// ---- the convert composites (issue #506): the D25 extension -------

#[test]
fn both_convert_directions_journal_not_undoable_and_refuse_typed() {
    use crate::habit::{HabitTarget, NonZeroCount};
    let mut h = Harness::new();
    let (task, _) = recur_task(&mut h, "chore");
    // Leg A: series → habit.
    h.exec(&StateCommand::ConvertSeriesToHabit {
        head: task,
        target: HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
    })
    .expect("converted");
    let entry = h.journal.entries.last().unwrap();
    assert!(matches!(
        entry.facts,
        UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
    ));
    let err = h.exec(&StateCommand::Undo);
    assert!(
        matches!(
            err,
            Err(CommandError::TargetNotUndoable(
                NotUndoableReason::HabitSurface
            ))
        ),
        "the composite refuses-and-names, never skips"
    );
    // Leg B: habit → series.
    let board = h.board();
    let habit = match h
        .exec(&StateCommand::CreateHabit {
            board,
            title: "stretch".into(),
            target: HabitTarget::PerDay(NonZeroCount::new(2).unwrap()),
        })
        .expect("habit created")
    {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    };
    h.exec(&StateCommand::ConvertHabitToSeries {
        habit,
        pattern: crate::recur::RecurPattern::Daily {
            every: crate::recur::EveryDays::new(1).unwrap(),
        },
    })
    .expect("converted back");
    let entry = h.journal.entries.last().unwrap();
    assert!(matches!(
        entry.facts,
        UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
    ));
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::HabitSurface
        ))
    ));
    // The seams stay readable from state: the retired habit is
    // tombstoned but present, the minted head carries the link.
    let retired = h.disk.habits.habits[&habit].clone();
    assert!(retired.deleted);
    let linked = h
        .disk
        .tasks
        .values()
        .find(|t| {
            t.recur
                .as_ref()
                .is_some_and(|r| r.continued_from == Some(habit))
        })
        .expect("the minted head links to the habit");
    assert!(linked.is_live());
}

// ---- DR-7 chunk 1: the reflection undo arm (§5.3) ----

/// A record → the typed `ReflectionSurface` fact; a later undo of
/// the reflection head refuses with the same typed reason.
#[test]
fn reflection_heads_are_typed_not_undoable() {
    let mut h = Harness::new();
    let (day, (start, end)) = reflection_day_window();
    let recorded = h
        .exec(&StateCommand::RecordReflection {
            cadence: crate::reflection::ReviewCadence::Day,
            day,
            window: (start, end),
            text: "the day's close".into(),
            mood: Some(4),
            energy: None,
            flags: std::collections::BTreeSet::from([crate::reflection::HealthFlag::GoodDay]),
        })
        .expect("accepted");
    let CommandOutcome::ReflectionRecorded { entry } = recorded else {
        panic!("wrong receipt");
    };
    let facts = &h.journal.entries.last().expect("journaled").facts;
    assert!(
        matches!(
            facts,
            UndoFacts::NotUndoable(NotUndoableReason::ReflectionSurface)
        ),
        "typed classification, no text"
    );
    // The head refuses typed: `TargetNotUndoable(ReflectionSurface)`.
    let err = plan_undo(
        &h.app(),
        &h.outbox,
        &h.journal,
        UndoScope::Global,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        ts(h.now),
        None,
    )
    .expect_err("the head is not undoable");
    assert!(matches!(
        err,
        CommandError::TargetNotUndoable(NotUndoableReason::ReflectionSurface)
    ));
    let _ = entry;
}

/// An edit and a tombstone classify the same way; a no-op record
/// journals nothing (the empty-batch rule at the undo tier).
#[test]
fn reflection_edits_and_deletes_classify_not_undoable() {
    let mut h = Harness::new();
    let (day, (start, end)) = reflection_day_window();
    let id = crate::ids::ReflectionId::from(uuid::Uuid::from_u128(701));
    h.disk.reflection.entries.insert(
        id,
        crate::reflection::ReflectionEntry {
            id,
            cadence: crate::reflection::ReviewCadence::Day,
            day,
            window_start: start,
            window_end: end,
            text: "seeded".into(),
            attributes: crate::reflection::ReflectionAttributes::default(),
            created_at: ts(900_000),
            updated_at: ts(900_000),
            deleted: false,
        },
    );
    h.exec(&StateCommand::UpdateReflection {
        id,
        text: Some("edited".into()),
        mood: None,
        energy: None,
        flags_add: std::collections::BTreeSet::new(),
        flags_remove: std::collections::BTreeSet::new(),
    })
    .expect("accepted");
    h.exec(&StateCommand::DeleteReflection { id })
        .expect("accepted");
    for entry in &h.journal.entries {
        assert!(matches!(
            entry.facts,
            UndoFacts::NotUndoable(NotUndoableReason::ReflectionSurface)
        ));
    }
    assert_eq!(h.journal.entries.len(), 2);
    let _ = (day, start, end);
}

fn reflection_day_window() -> (
    chrono::NaiveDate,
    (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
) {
    use chrono::TimeZone as _;
    let day = chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap();
    let start = chrono::Utc.with_ymd_and_hms(2026, 11, 5, 0, 0, 0).unwrap();
    let end = chrono::Utc.with_ymd_and_hms(2026, 11, 6, 0, 0, 0).unwrap();
    (day, (start, end))
}

// ---- the stack restore surface (#356, round C8) ---------------------------

/// Seeds `stack` + `n` tasks (titles "t0".."t{n}") and returns the ids.
fn seeded_stack(
    h: &mut Harness,
    board: crate::ids::BoardId,
    n: usize,
) -> (crate::ids::StackId, Vec<crate::ids::TaskId>) {
    let stack = h.stack(board);
    let tasks = (0..n).map(|i| h.task(stack, &format!("t{i}"))).collect();
    (stack, tasks)
}

/// Adopts a remote ref on the stack and every named task, then clears
/// the standing create ops (the pushed/bound seed shape).
fn bind_stack_and_tasks(h: &mut Harness, stack: crate::ids::StackId, tasks: &[crate::ids::TaskId]) {
    {
        let mut bound = h.disk.stacks[&stack].clone();
        bound.remote = Some(crate::ids::RemoteStackRef {
            board: crate::ids::RemoteBoardId::deck(7),
            stack: crate::ids::RemoteStackId::deck(8),
        });
        h.disk.stacks.insert(stack, bound);
        for task in tasks {
            let mut bound = h.disk.tasks[task].clone();
            bound.remote = Some(crate::ids::RemoteCardRef {
                board: crate::ids::RemoteBoardId::deck(7),
                stack: crate::ids::RemoteStackId::deck(8),
                card: crate::ids::RemoteCardId::deck(9),
            });
            h.disk.tasks.insert(*task, bound);
        }
        h.disk.outbox.clear();
        h.sync();
    }
}

#[test]
fn delete_stack_journals_pre_images_and_is_undoable() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 2);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    let entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::DeleteStack { .. }))
        .expect("the delete journals one entry");
    let UndoFacts::PreImages { images, .. } = &entry.facts else {
        panic!("DeleteStack journals PreImages, got {:?}", entry.facts);
    };
    // The capture is arm-agnostic: the stack image plus one image per
    // cascaded task, each with a `Some` pre-state.
    let stack_images = images
        .iter()
        .filter(|image| {
            matches!(
                image,
                PreImage::Stack {
                    before: Some(_),
                    ..
                }
            )
        })
        .count();
    let task_images = images
        .iter()
        .filter(|image| {
            matches!(
                image,
                PreImage::Task {
                    before: Some(_),
                    ..
                }
            )
        })
        .count();
    assert_eq!(stack_images, 1);
    assert_eq!(task_images, tasks.len());
    // Undoable: the head targets the delete entry and undo plans.
    assert_eq!(h.journal.head, Some(entry.id));
    let plan = h
        .exec_plan(&StateCommand::Undo)
        .expect("the reclassified delete undoes");
    assert!(matches!(plan.outcome, CommandOutcome::Undone { .. }));
}

#[test]
fn undo_of_delete_stack_restores_stack_and_cascade_locally() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 2);
    let create_ops_before = h
        .disk
        .outbox
        .iter()
        .filter(|pending| matches!(pending.op, crate::outbox::LocalOp::CreateStack(_)))
        .count();
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    h.exec(&StateCommand::Undo).expect("undone");
    let create_ops_after = h
        .disk
        .outbox
        .iter()
        .filter(|pending| matches!(pending.op, crate::outbox::LocalOp::CreateStack(_)))
        .count();
    assert_eq!(create_ops_after, create_ops_before);
    // The stack and every member live again, pre-delete values intact.
    assert!(!h.disk.stacks[&stack].deleted);
    for task in &tasks {
        assert!(!h.disk.tasks[task].deleted);
    }
    assert_eq!(h.disk.stacks[&stack].title, "stack");
    // The pending stack-delete op retracted (the unbound shape: no new
    // create ops minted).
    assert!(
        !h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::DeleteStack(s) if s == stack)
        )
    );
    // The head moved off the delete entry.
    let delete_entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::DeleteStack { .. }))
        .expect("delete entry");
    assert_eq!(h.journal.head, delete_entry.parent);
}

#[test]
fn undo_of_pushed_delete_stack_reposts_as_creates() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 2);
    bind_stack_and_tasks(&mut h, stack, &tasks);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    // The delete reached the server: flip the pushed flag and retire
    // the entry's ops from the outbox (the R-U9 post-pass post-state:
    // completed ops leave the pending queue).
    let delete_entry_id = h.journal.head.expect("delete is the head");
    let mut j = h.disk.journal.clone();
    let entry = j
        .entries
        .iter_mut()
        .find(|entry| entry.id == delete_entry_id)
        .expect("delete entry");
    entry.pushed = true;
    let completed = entry.ops.clone();
    h.disk.journal = j;
    h.disk
        .outbox
        .retain(|pending| !completed.contains(&pending.op_id));
    h.sync();
    h.exec(&StateCommand::Undo).expect("undone");
    // Repost: a CreateStack op for the stack and a CreateTask op per
    // member; the entry's own ops are not re-failed (the server
    // completed them).
    assert!(
        h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::CreateStack(s) if s == stack)
        )
    );
    for task in &tasks {
        assert!(h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::CreateTask(t) if t == *task)
        ));
    }
    assert!(!h.disk.stacks[&stack].deleted);
}

#[test]
fn undo_of_delete_stack_refuses_field_named_when_a_member_diverged() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 2);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    // A member moved since: bump its tombstone clock (any later writer
    // on the watermarked field).
    let member = tasks[0];
    let mut diverged = h.disk.tasks[&member].clone();
    diverged.clocks.deleted = ts(h.now + 10_000);
    h.disk.tasks.insert(member, diverged);
    h.sync();
    let head_before = h.journal.head;
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Deleted,
            ..
        })
    ));
    // Atomicity: the whole line refuses, the head never moves.
    assert_eq!(h.journal.head, head_before);
    assert!(h.disk.stacks[&stack].deleted);
}

#[test]
fn undo_of_delete_stack_refuses_when_a_member_was_individually_restored() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 2);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    // One member takes its own recovery path: it resurrects live, and
    // the stack-delete entry's recorded tombstone stamp has diverged.
    // The head rewinds to the stack delete (a later line walk — the
    // undo targets the delete entry, not the member restore).
    let member = tasks[1];
    h.exec(&StateCommand::RestoreTask {
        id: member,
        resurrect: Box::new(h.disk.tasks[&member].clone()),
    })
    .expect("restored");
    assert!(!h.disk.tasks[&member].deleted);
    let delete_entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::DeleteStack { .. }))
        .expect("delete entry");
    h.disk.journal.head = Some(delete_entry.id);
    h.sync();
    let head_before = h.journal.head;
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Deleted,
            ..
        })
    ));
    assert_eq!(h.journal.head, head_before);
    assert!(h.disk.stacks[&stack].deleted, "no partial restore");
}

#[test]
fn restore_stack_on_a_live_stack_is_the_accepted_noop() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, _) = seeded_stack(&mut h, board, 1);
    let before = h.journal.entries.len();
    let outcome = h
        .exec(&StateCommand::RestoreStack { id: stack })
        .expect("accepted");
    assert_eq!(outcome, CommandOutcome::NoOp);
    assert_eq!(h.journal.entries.len(), before, "nothing journaled");
}

#[test]
fn restore_stack_then_undo_re_deletes_through_the_cascade() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 1);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    // Forward restore: the stack entity only — the member keeps its own
    // tombstone (the documented leniency).
    let outcome = h
        .exec(&StateCommand::RestoreStack { id: stack })
        .expect("restored");
    assert!(matches!(outcome, CommandOutcome::StackRestored { .. }));
    assert!(!h.disk.stacks[&stack].deleted);
    assert!(h.disk.tasks[&tasks[0]].deleted);
    // Undo re-deletes the stack through the ordinary cascade.
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(h.disk.stacks[&stack].deleted);
    assert!(
        !h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::DeleteStack(s) if s == stack)
        ),
        "the unbound re-delete retracts its own op"
    );
    // Redo re-plans the forward intent: the stack is restored again.
    h.exec(&StateCommand::Redo).expect("redone");
    assert!(!h.disk.stacks[&stack].deleted);
}

#[test]
fn a_new_edit_after_stack_restores_forks_and_the_old_redo_path_dies() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, _) = seeded_stack(&mut h, board, 1);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    h.exec(&StateCommand::Undo).expect("undone");
    // The new edit forks the line; the delete's redo path dies.
    h.exec(&StateCommand::RenameStack {
        id: stack,
        new_title: "renamed".into(),
    })
    .expect("renamed");
    let err = h.exec(&StateCommand::Redo);
    assert!(matches!(err, Err(CommandError::NothingToRedo)));
}

#[test]
fn stack_delete_and_restore_entries_carry_the_board_tag() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, _) = seeded_stack(&mut h, board, 1);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    h.exec(&StateCommand::RestoreStack { id: stack })
        .expect("restored");
    for command in h.journal.entries.iter().rev() {
        match command.command {
            StateCommand::DeleteStack { .. } | StateCommand::RestoreStack { .. } => {
                assert_eq!(command.board, Some(board));
            }
            _ => {}
        }
    }
}

#[test]
fn undo_of_delete_stack_refuses_when_the_stack_row_gone() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, _tasks) = seeded_stack(&mut h, board, 1);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    // The stack row is hard-removed (an external sweep / eviction).
    h.disk.stacks.remove(&stack);
    h.sync();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Deleted,
            ..
        })
    ));
}

// ---- the proptest corpus (#356) -------------------------------------------

use crate::test_support::proptest_config;
use proptest::prelude::*;

proptest! {
    #![proptest_config(proptest_config(16))]

    /// P1: undo of a stack delete either refuses typed or restores
    /// exactly the entry's own pre-image set — the stack and its N
    /// members — leaving the board (and everything else) untouched.
    #[test]
    fn undo_delete_stack_restores_exactly_the_cascade(
        n in 1usize..=4,
        bind in proptest::bool::ANY,
    ) {
        let mut h = Harness::new();
        let board = h.board();
        let (stack, tasks) = seeded_stack(&mut h, board, n);
        if bind {
            bind_stack_and_tasks(&mut h, stack, &tasks);
        }
        let board_before = h.disk.boards[&board].clone();
        let pre_titles: Vec<String> =
            tasks.iter().map(|t| h.disk.tasks[t].title.clone()).collect();
        h.exec(&StateCommand::DeleteStack { id: stack }).expect("deleted");
        match h.exec(&StateCommand::Undo) {
            Ok(_) => {
                prop_assert!(!h.disk.stacks[&stack].deleted);
                for (i, task) in tasks.iter().enumerate() {
                    prop_assert!(!h.disk.tasks[task].deleted);
                    prop_assert_eq!(&h.disk.tasks[task].title, &pre_titles[i]);
                }
                prop_assert_eq!(&h.disk.boards[&board], &board_before);
            }
            Err(CommandError::UndoTargetHasDependents { .. }) => {
                // The occupancy guard: only reachable when the inverse is
                // a DeleteStack — never for this composite restore.
                prop_assert!(false, "the composite restore never trips the guard");
            }
            Err(other) => {
                // Any refusal is typed and leaves the tombstones standing.
                let _ = other;
                prop_assert!(h.disk.stacks[&stack].deleted);
            }
        }
    }

    /// P2: delete → undo round-trips every member's content fields to
    /// their pre-delete values (the refreshed tombstone clock excluded).
    #[test]
    fn delete_undo_round_trip_preserves_member_fields(n in 1usize..=4) {
        let mut h = Harness::new();
        let board = h.board();
        let (stack, tasks) = seeded_stack(&mut h, board, n);
        let mut pre = std::collections::BTreeMap::new();
        for task in &tasks {
            pre.insert(*task, h.disk.tasks[task].clone());
        }
        h.exec(&StateCommand::DeleteStack { id: stack }).expect("deleted");
        h.exec(&StateCommand::Undo).expect("undone");
        for task in &tasks {
            let before = &pre[task];
            let after = &h.disk.tasks[task];
            prop_assert!(!after.deleted);
            prop_assert_eq!(&after.title, &before.title);
            prop_assert_eq!(after.stack, before.stack);
            prop_assert_eq!(after.order, before.order);
            prop_assert_eq!(after.done, before.done);
            prop_assert_eq!(after.duedate, before.duedate);
            prop_assert_eq!(after.archived, before.archived);
            prop_assert_eq!(after.clocks.title, before.clocks.title);
            prop_assert_eq!(after.clocks.position, before.clocks.position);
        }
        prop_assert!(!h.disk.stacks[&stack].deleted);
    }
}

/// P3 (D-6 fence property): over the board-ceremony family,
/// every journaled entry's facts are `NotUndoable(BoardCeremony)` — the
/// ceremony family never journals pre-images, so no stack-entry
/// reclassification moved the fence.
#[test]
fn board_ceremony_commands_never_journal_pre_images() {
    let mut h = Harness::new();
    let board = h.board();
    let ceremonies = [
        StateCommand::RenameBoard {
            board,
            title: "renamed".into(),
        },
        StateCommand::SetBoardColor {
            board,
            color: crate::entities::Color::new("ff0000"),
        },
        StateCommand::SetBoardArchived {
            board,
            archived: true,
        },
        StateCommand::UnbindBoard { board },
        StateCommand::ReconcileByTitle { board },
    ];
    for command in ceremonies {
        if let Ok(plan) = h.exec_plan(&command) {
            if plan.actions.is_empty() {
                continue;
            }
            let Some(PersistenceAction::RecordJournal(entry)) = plan.actions.last() else {
                panic!("RecordJournal rides last for {command:?}");
            };
            assert!(
                matches!(
                    entry.facts,
                    UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony)
                ),
                "a ceremony journaled non-ceremony facts for {command:?}"
            );
        }
    }
}

// ---- the per-board undo scope (#355, D-1..D-5) ----------------------------

#[test]
fn scoped_undo_refuses_typed_when_the_head_is_another_boards_edit() {
    let mut h = Harness::new();
    let board_a = h.board();
    let board_b = h.board();
    let stack_a = h.stack(board_a);
    let stack_b = h.stack(board_b);
    h.task(stack_a, "a");
    h.task(stack_b, "b");
    // The head is board B's edit.
    let disk_before = h.disk.clone();
    let journal_before = h.journal.clone();
    let err = h
        .exec(&StateCommand::UndoOnBoard { board: board_a })
        .expect_err("the frontier belongs to another board");
    assert!(matches!(
        err,
        CommandError::NothingToUndoForBoard { board } if board == board_a
    ));
    // State and journal byte-identical, head unmoved: the head is
    // checked, never skipped past (the strict frontier rule, D-1).
    assert_eq!(h.disk, disk_before);
    assert_eq!(h.journal, journal_before);
    // The global undo still reaches the head.
    h.exec(&StateCommand::Undo).expect("global undo");
}

#[test]
fn scoped_undo_restores_when_the_head_is_in_scope() {
    let mut h = Harness::new();
    let board_a = h.board();
    let board_b = h.board();
    let stack_b = h.stack(board_b);
    h.task(stack_b, "b");
    let stack_a = h.stack(board_a);
    let task = h.task(stack_a, "a");
    // The head is board A's edit: the scoped undo plans the same shape
    // as the global undo — the create undone.
    let outcome = h
        .exec(&StateCommand::UndoOnBoard { board: board_a })
        .expect("in scope");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    assert!(h.disk.tasks[&task].deleted);
    // Composition with the C8 restore (D-1): a DeleteStack head in
    // scope fires the combined stack+cascade restore through the
    // scoped arm.
    let (stack, tasks) = seeded_stack(&mut h, board_a, 2);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    let outcome = h
        .exec(&StateCommand::UndoOnBoard { board: board_a })
        .expect("combined restore");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    assert!(!h.disk.stacks[&stack].deleted);
    for task in &tasks {
        assert!(!h.disk.tasks[task].deleted);
    }
}

#[test]
fn scoped_undo_on_a_none_tagged_head_refuses() {
    let mut h = Harness::new();
    let board = h.board();
    // The honest-ambiguity fixture: an inbox capture carries no board
    // tag (D-3: `None` belongs to no board scope).
    h.exec(&StateCommand::CaptureTask {
        title: "inbox".into(),
        board: None,
    })
    .expect("captured");
    let disk_before = h.disk.clone();
    let err = h
        .exec(&StateCommand::UndoOnBoard { board })
        .expect_err("a None tag is out of every board scope");
    assert!(matches!(
        err,
        CommandError::NothingToUndoForBoard { board: b } if b == board
    ));
    assert_eq!(h.disk, disk_before, "the scoped refusal is inert");
    // The global undo still reaches the None-tagged head.
    h.exec(&StateCommand::Undo).expect("global undo reaches it");
}

#[test]
fn scoped_undo_on_a_not_undoable_head_still_names_the_reason() {
    let mut h = Harness::new();
    let board = h.board();
    h.exec(&StateCommand::RenameBoard {
        board,
        title: "renamed".into(),
    })
    .expect("renamed");
    // In scope, so the classification check runs — the D-5 passthrough
    // row, verbatim from the global undo.
    let err = h
        .exec(&StateCommand::UndoOnBoard { board })
        .expect_err("ceremony head");
    assert!(matches!(
        err,
        CommandError::TargetNotUndoable(NotUndoableReason::BoardCeremony)
    ));
}

#[test]
fn scoped_redo_picks_the_greatest_in_scope_child() {
    let mut h = Harness::new();
    let board_a = h.board();
    let board_b = h.board();
    let stack_a = h.stack(board_a);
    let stack_b = h.stack(board_b);
    h.task(stack_a, "seed");
    // Fork off the seed entry: first an in-scope (A) child, then an
    // out-of-scope (B) child — the B child is newer, so the global
    // greatest is the B child.
    h.exec(&StateCommand::Undo).expect("retreat to the seed");
    let _child_a = match h
        .exec(&StateCommand::CreateTask {
            title: "child-a".into(),
            stack: stack_a,
            order: 1,
        })
        .expect("child a")
    {
        CommandOutcome::CreatedTask(id) => id,
        other => panic!("expected CreatedTask, got {other:?}"),
    };
    h.exec(&StateCommand::Undo).expect("retreat again");
    h.exec(&StateCommand::CreateTask {
        title: "child-b".into(),
        stack: stack_b,
        order: 1,
    })
    .expect("child b");
    h.exec(&StateCommand::Undo)
        .expect("retreat to the fork point");
    let entry_a = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::CreateTask { ref title, .. } if title == "child-a"))
        .map(|entry| entry.id)
        .expect("the A child entry");
    let entry_b = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::CreateTask { ref title, .. } if title == "child-b"))
        .map(|entry| entry.id)
        .expect("the B child entry");
    // Scoped: the sibling rule narrowed — the in-scope child wins even
    // though the out-of-scope sibling is newer.
    let outcome = h
        .exec(&StateCommand::RedoOnBoard { board: board_a })
        .expect("in-scope child");
    assert_eq!(
        outcome,
        CommandOutcome::Redone { entry: entry_a },
        "the greatest in-scope child"
    );
    // Global: the D5 rule unchanged — the greatest overall.
    h.exec(&StateCommand::Undo)
        .expect("retreat to the fork point");
    let outcome = h.exec(&StateCommand::Redo).expect("global redo");
    assert_eq!(outcome, CommandOutcome::Redone { entry: entry_b });
}

#[test]
fn scoped_redo_refuses_when_no_child_is_in_scope() {
    let mut h = Harness::new();
    let board_a = h.board();
    let board_b = h.board();
    let stack_b = h.stack(board_b);
    // Both the seed and the child sit on board B: the head retreats to
    // the stack_b entry, whose children are all out of A's scope.
    h.task(stack_b, "seed");
    h.exec(&StateCommand::Undo).expect("retreat to the seed");
    h.exec(&StateCommand::CreateTask {
        title: "child-b".into(),
        stack: stack_b,
        order: 1,
    })
    .expect("child b");
    h.exec(&StateCommand::Undo)
        .expect("retreat to the fork point");
    let err = h
        .exec(&StateCommand::RedoOnBoard { board: board_a })
        .expect_err("the only child is out of scope");
    assert!(matches!(
        err,
        CommandError::NothingToRedoForBoard { board: b } if b == board_a
    ));
    // The global redo still picks the out-of-scope child.
    h.exec(&StateCommand::Redo).expect("global redo");
}

#[test]
fn scoped_undo_moves_the_head_and_journals_nothing() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    h.task(stack, "t");
    let head = h.journal.head.expect("a head exists");
    let head_parent = h.journal.entry(head).expect("the head entry").parent;
    let plan = h
        .exec_plan(&StateCommand::UndoOnBoard { board })
        .expect("planned");
    // The engine-routed contract, asserted structurally: the batch ends
    // with the head move and carries no RecordJournal.
    assert!(
        !plan
            .actions
            .iter()
            .any(|action| matches!(action, PersistenceAction::RecordJournal(_)))
    );
    assert_eq!(
        plan.actions.last(),
        Some(&PersistenceAction::MoveJournalHead(head_parent)),
        "the head moves to the undone entry's parent"
    );
}

#[test]
fn cross_board_move_entry_is_tagged_with_the_source_board() {
    let mut h = Harness::new();
    let board_a = h.board();
    let board_b = h.board();
    let stack_a = h.stack(board_a);
    let stack_b = h.stack(board_b);
    let task = h.task(stack_a, "mover");
    h.exec(&StateCommand::MoveTask {
        id: task,
        stack: stack_b,
        order: 1,
    })
    .expect("moved");
    let entry = h.journal.entries.last().expect("journaled");
    // D-4: `touched_board` reads the pre-state, so the cross-board move
    // is tagged with the *source* board — the board whose state the
    // entry's inverse restores.
    assert_eq!(entry.board, Some(board_a));
    // The recorded consequence (R-3): the source scope can undo the
    // move; the destination scope refuses typed.
    h.exec(&StateCommand::UndoOnBoard { board: board_b })
        .expect_err("the destination scope cannot undo a move into it");
    h.exec(&StateCommand::UndoOnBoard { board: board_a })
        .expect("the source scope undoes the move");
    assert_eq!(h.disk.tasks[&task].stack, stack_a);
}

// ---- the per-board proptest corpus (#355) ---------------------------------

/// The shared two-board edit fixture: board B's edits land first, board
/// A's last — so the head's scope is B when `edits_a == 0 && edits_b > 0`,
/// and A otherwise. Every edit is a distinct-title `UpdateTask` (never a
/// no-op, so the line always grows as the counts say).
fn two_board_edit_fixture(
    h: &mut Harness,
    edits_a: usize,
    edits_b: usize,
) -> (
    crate::ids::BoardId,
    crate::ids::BoardId,
    crate::ids::TaskId,
    crate::ids::TaskId,
) {
    let board_a = h.board();
    let board_b = h.board();
    let stack_a = h.stack(board_a);
    let stack_b = h.stack(board_b);
    let task_a = h.task(stack_a, "a");
    let task_b = h.task(stack_b, "b");
    for i in 0..edits_b {
        h.exec(&StateCommand::UpdateTask {
            id: task_b,
            changes: crate::messages::TaskChanges {
                title: Some(format!("b-edited-{i}")),
                ..Default::default()
            },
        })
        .expect("edit b");
    }
    for i in 0..edits_a {
        h.exec(&StateCommand::UpdateTask {
            id: task_a,
            changes: crate::messages::TaskChanges {
                title: Some(format!("a-edited-{i}")),
                ..Default::default()
            },
        })
        .expect("edit a");
    }
    (board_a, board_b, task_a, task_b)
}

proptest! {
    #![proptest_config(proptest_config(16))]

    /// P1 (#355's core property, mechanical via the frontier rule): a
    /// scoped undo either refuses typed or leaves every entity of every
    /// other board byte-identical.
    #[test]
    fn scoped_undo_never_touches_out_of_scope_entities(
        edits_a in 0usize..=4,
        edits_b in 0usize..=4,
    ) {
        let mut h = Harness::new();
        let (board_a, board_b, task_a, task_b) =
            two_board_edit_fixture(&mut h, edits_a, edits_b);
        let task_a_before = h.disk.tasks[&task_a].clone();
        let task_b_before = h.disk.tasks[&task_b].clone();
        let stack_b_before = h.disk.stacks.values().find(|s| s.board == board_b).cloned().unwrap();
        match h.exec(&StateCommand::UndoOnBoard { board: board_a }) {
            Err(CommandError::NothingToUndoForBoard { .. }) => {
                // The strict frontier rule: the head is B's edit, the
                // refusal is typed, and nothing moved at all.
                prop_assert_eq!(&h.disk.tasks[&task_a], &task_a_before);
                prop_assert_eq!(&h.disk.tasks[&task_b], &task_b_before);
            }
            Ok(_) => {
                // The undo touched A's frontier only: board B's task and
                // stack are byte-identical.
                prop_assert_eq!(&h.disk.tasks[&task_b], &task_b_before);
                let stack_b_after = h.disk.stacks.values().find(|s| s.board == board_b).cloned().unwrap();
                prop_assert_eq!(stack_b_after, stack_b_before);
            }
            Err(other) => prop_assert!(false, "unexpected typed refusal: {other:?}"),
        }
    }

    /// P2 (D-6's by-construction agreement, proven): `derive_for_board`
    /// and the scoped planners share one predicate — `can_undo` iff a
    /// scoped `plan_undo` does not refuse `NothingToUndoForBoard`, and
    /// `can_redo` iff a scoped `plan_redo` does not refuse
    /// `NothingToRedoForBoard`.
    #[test]
    fn derive_for_board_agrees_with_the_scoped_planners(
        edits_a in 0usize..=3,
        edits_b in 0usize..=3,
        retreats in 0usize..=2,
    ) {
        let mut h = Harness::new();
        let (board_a, board_b, _, _) = two_board_edit_fixture(&mut h, edits_a, edits_b);
        // Retreats along the global line (every head here is undoable
        // until the line's NotUndoable ceremony roots); a typed refusal
        // just ends the retreats early.
        for _ in 0..retreats {
            if h.exec(&StateCommand::Undo).is_err() {
                break;
            }
        }
        for board in [board_a, board_b] {
            let view = crate::journal::UndoView::derive_for_board(&h.journal, board);
            let undo_refused = matches!(
                plan_undo(
                    &h.app(),
                    &h.outbox,
                    &h.journal,
                    UndoScope::Board(board),
                    &h.ids,
                    &crate::reminders::ReminderDefaults::default(),
                    Harness::day(),
                    ts(h.now),
                    None,
                ),
                Err(CommandError::NothingToUndoForBoard { .. })
            );
            prop_assert_eq!(view.can_undo, !undo_refused);
            let redo_refused = matches!(
                plan_redo(
                    &h.app(),
                    &h.outbox,
                    &h.journal,
                    UndoScope::Board(board),
                    &h.ids,
                    &crate::reminders::ReminderDefaults::default(),
                    Harness::day(),
                    ts(h.now),
                    None,
                ),
                Err(CommandError::NothingToRedoForBoard { .. })
            );
            prop_assert_eq!(view.can_redo, !redo_refused);
        }
    }
}

// ---- the restore family (#913 + #900 + #901) -------------------------------

/// Fixture: one reminder rule on a task (the legacy due-offset shape).
fn rule_for(h: &mut Harness, task: crate::ids::TaskId) -> crate::reminders::ReminderRule {
    h.exec(&StateCommand::AddReminderRule {
        task,
        offset: crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30))
            .expect("valid"),
        at: None,
        insistence: crate::reminders::Insistence::Normal,
    })
    .expect("rule");
    h.disk
        .reminders
        .rules
        .values()
        .next()
        .cloned()
        .expect("rule row")
}

/// Fixture: flips every journal entry past the push boundary (the
/// `the_pushed_flag_keeps_the_inverse_ops_and_drops_the_retraction`
/// precedent) so an undo's inverse enqueues survive the retraction.
fn mark_journal_pushed(h: &mut Harness) {
    for entry in &mut h.journal.entries {
        entry.pushed = true;
    }
    h.disk.journal = h.journal.clone();
}

#[test]
fn undo_of_restore_task_re_deletes_through_delete_task() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let resurrect = h.disk.tasks[&task].clone();
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    // The restore step is the journaled forward verb (the #913 entry
    // shape): undoing *it* is the bug's scenario.
    h.exec(&StateCommand::RestoreTask {
        id: task,
        resurrect: Box::new(resurrect),
    })
    .expect("restored");
    assert!(!h.disk.tasks[&task].deleted);
    // The #913 scenario: undoing the restore re-deletes through the
    // ordinary DeleteTask cascade — NOT NothingToUndo.
    h.exec(&StateCommand::Undo)
        .expect("undo the restore: task re-deleted");
    assert!(h.disk.tasks[&task].deleted);
    // The restore entry journaled PreImages of its own writes.
    let restore_entry = h
        .journal
        .entries
        .iter()
        .find(|entry| matches!(entry.command, StateCommand::RestoreTask { .. }))
        .expect("restore entry journaled");
    assert!(matches!(restore_entry.facts, UndoFacts::PreImages { .. }));
    // The head moved back past the restore entry.
    assert_eq!(h.journal.head, restore_entry.parent);
}

#[test]
fn undo_of_restore_task_refuses_field_named_when_edited_since() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let resurrect = h.disk.tasks[&task].clone();
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    h.exec(&StateCommand::RestoreTask {
        id: task,
        resurrect: Box::new(resurrect),
    })
    .expect("restored");
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("edited since".into()),
            ..Default::default()
        },
    })
    .expect("edited");
    // Retreat past the edit entry so the restore entry is the head.
    h.exec(&StateCommand::Undo).expect("undone the edit");
    let state_after_edit_undo = h.disk.tasks[&task].clone();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Title,
            ..
        })
    ));
    // Zero actions: the refusal leaves the state byte-identical.
    assert_eq!(h.disk.tasks[&task], state_after_edit_undo);
}

#[test]
fn undo_of_restore_task_refuses_when_re_tombstoned_since() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let resurrect = h.disk.tasks[&task].clone();
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    h.exec(&StateCommand::RestoreTask {
        id: task,
        resurrect: Box::new(resurrect),
    })
    .expect("restored");
    // Reach the restore entry as the head, then tombstone the task
    // externally (an ordinary re-delete journals its own entry, so a
    // *standing* re-tombstone under a restore head is divergence — the
    // `undo_of_delete_stack_refuses_when_the_stack_row_gone` precedent).
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("re-tombstoned");
    h.exec(&StateCommand::Undo).expect("undone the re-delete");
    let mut diverged = h.disk.tasks[&task].clone();
    diverged.deleted = true;
    diverged.clocks.deleted = ts(5);
    h.disk.tasks.insert(task, diverged);
    h.sync();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Deleted,
            ..
        })
    ));
    // The guard is the tombstone stamp, not the DeleteStack-shaped
    // occupancy guard: a dependents refusal is structurally impossible
    // for a DeleteTask inverse.
    assert!(!matches!(
        err,
        Err(CommandError::UndoTargetHasDependents { .. })
    ));
}

#[test]
fn undo_of_delete_stack_resurrects_the_cascaded_reminder_rules() {
    let mut h = Harness::new();
    let board = h.board();
    let (stack, tasks) = seeded_stack(&mut h, board, 1);
    let rule = rule_for(&mut h, tasks[0]);
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    assert!(
        h.disk.reminders.rules.is_empty(),
        "the cascade removed the rule"
    );
    // Past the push boundary: the resurrect's flush triggers survive the
    // undo (the delete's own document-lane trigger, mirrored).
    mark_journal_pushed(&mut h);
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(!h.disk.tasks[&tasks[0]].deleted);
    // Every rule row back verbatim (same id, same created_at, whole row).
    assert_eq!(h.disk.reminders.rules.get(&rule.id), Some(&rule));
    // The flush trigger enqueued per rule's task.
    assert!(
        h.disk
            .outbox
            .iter()
            .any(|pending| matches!(pending.op, crate::outbox::LocalOp::UpdateAppData(t) if t == rule.task)),
        "the resurrect enqueued the document-lane flush trigger"
    );
    // Deliveries stay pruned: a resurrect does not forge delivery proofs
    // back into existence (D-3, correct-by-design).
    assert!(
        h.disk
            .reminders
            .deliveries
            .iter()
            .all(|d| d.reminder != crate::reminders::ReminderRef::Rule(rule.id)),
        "no delivery proofs were forged back"
    );
}

#[test]
fn undo_of_delete_task_resurrects_its_rules_and_a_rule_readded_since_is_skipped() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let rule_a = rule_for(&mut h, task);
    let rule_b = {
        h.exec(&StateCommand::RemoveReminderRule { rule: rule_a.id })
            .expect("removed");
        rule_for(&mut h, task)
    };
    let rule_b_original = rule_b.clone();
    h.exec(&StateCommand::DeleteTask { id: task })
        .expect("deleted");
    assert!(h.disk.reminders.rules.is_empty());
    // A prior resurrect (or external forge) placed a *different* row
    // under rule_a's id: the skip-not-refuse guard leaves it standing.
    let mut rule_a_modified = rule_a.clone();
    rule_a_modified.insistence = crate::reminders::Insistence::Alarm;
    h.disk
        .reminders
        .rules
        .insert(rule_a.id, rule_a_modified.clone());
    h.sync();
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(!h.disk.tasks[&task].deleted);
    // The present same-id row was not clobbered…
    assert_eq!(
        h.disk.reminders.rules.get(&rule_a.id),
        Some(&rule_a_modified)
    );
    // …and the absent rule came back verbatim.
    assert_eq!(
        h.disk.reminders.rules.get(&rule_b.id),
        Some(&rule_b_original)
    );
}

// ---- the rule-verb undo family (#354, UF1) ---------------------------------

/// Fixture: one reminder rule on a task, the exact outcome id returned
/// (unambiguous beside other rules). `seq` varies the offset so
/// consecutive calls never dedup into a survivor.
fn added_rule(
    h: &mut Harness,
    task: crate::ids::TaskId,
    seq: i64,
) -> crate::reminders::ReminderRule {
    let offset = crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30 - seq))
        .expect("valid offset");
    match h
        .exec(&StateCommand::AddReminderRule {
            task,
            offset,
            at: None,
            insistence: crate::reminders::Insistence::Normal,
        })
        .expect("rule added")
    {
        CommandOutcome::ReminderRuleAdded { rule, added: true } => {
            h.disk.reminders.rules[&rule].clone()
        }
        other => panic!("unexpected outcome: {other:?}"),
    }
}

/// US-1/S4 (AC-1, W-1/W-2): a journaled rule removal inverts through
/// the #900 verbatim resurrect — same id, same `created_at`, whole row
/// — never a re-added fresh identity.
#[test]
fn us1_s4_removed_rule_resurrects_verbatim() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let rule = added_rule(&mut h, task, 1);
    let original = rule.clone();
    h.exec(&StateCommand::RemoveReminderRule { rule: rule.id })
        .expect("removed");
    assert!(
        !h.disk.reminders.rules.contains_key(&rule.id),
        "the removal removed the row"
    );
    h.exec(&StateCommand::Undo).expect("undone");
    assert_eq!(
        h.disk.reminders.rules.get(&rule.id),
        Some(&original),
        "the resurrect restores the exact stored fact"
    );
    assert!(
        h.disk
            .reminders
            .deliveries
            .iter()
            .all(|d| d.reminder != crate::reminders::ReminderRef::Rule(rule.id)),
        "no delivery proofs were forged back"
    );
}

/// US-1/S4 (AC-2, the D-U2 pushed class): past the push boundary the
/// undo keeps the inverse's document-lane flush — the resurrect's
/// trigger restores the server document, never a retraction.
#[test]
fn us1_s4b_pushed_removal_enqueues_flush_not_retraction() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let rule = added_rule(&mut h, task, 1);
    h.exec(&StateCommand::RemoveReminderRule { rule: rule.id })
        .expect("removed");
    mark_journal_pushed(&mut h);
    let plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, PersistenceAction::EnqueueOp(_))),
        "the flushed undo keeps an enqueue"
    );
    assert!(
        !plan
            .actions
            .iter()
            .any(|a| matches!(a, PersistenceAction::FailOp(_))),
        "a pushed entry's undo fails nothing"
    );
    assert!(
        h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::UpdateAppData(t) if t == task)
        ),
        "the resurrect's document-lane flush survived the push boundary"
    );
    assert_eq!(
        h.disk.reminders.rules.get(&rule.id),
        Some(&rule),
        "the rule came back verbatim"
    );
}

/// US-1/S4 (AC-3, the D-U2 unpushed class): the server never knew the
/// removal — the undo fails the entry's own ops and leaves no enqueue
/// of its own (the resurrect's upsert stays action-level; only its
/// flush trigger is dropped).
#[test]
fn us1_s4c_unpushed_removal_retracts() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let rule = added_rule(&mut h, task, 1);
    h.exec_plan(&StateCommand::RemoveReminderRule { rule: rule.id })
        .expect("removed");
    let entry_ops: Vec<_> = h
        .journal
        .entries
        .last()
        .map(|entry| entry.ops.clone())
        .expect("the removal journaled");
    assert!(!entry_ops.is_empty(), "the removal enqueued its flush");
    let undo_plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    assert_eq!(
        h.disk.reminders.rules.get(&rule.id),
        Some(&rule),
        "the resurrect restores the row"
    );
    assert!(
        !undo_plan
            .actions
            .iter()
            .any(|a| matches!(a, PersistenceAction::EnqueueOp(_))),
        "the unpushed undo enqueues nothing of its own"
    );
    let retracted = undo_plan
        .actions
        .iter()
        .filter_map(|a| match a {
            PersistenceAction::FailOp(op) => Some(*op),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        retracted, entry_ops,
        "the entry's own ops are failed, each once"
    );
}

/// US-2/S4 (AC-1, W-1/W-2): the add's head inverts through the
/// ordinary remove over the minted id — the plan carries the
/// delete-rule action for exactly the added rule.
#[test]
fn us2_s4_added_rule_deletes_again() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let rule = added_rule(&mut h, task, 1);
    let add_parent = h
        .journal
        .entries
        .last()
        .map(|entry| entry.parent)
        .expect("the add journaled");
    let plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, PersistenceAction::DeleteReminderRule(id) if *id == rule.id)),
        "the inverse deletes the added rule's id"
    );
    assert!(
        !h.disk.reminders.rules.contains_key(&rule.id),
        "the undo removed the rule"
    );
    // The journal head moved to the entry's parent (the undo line).
    assert_eq!(h.journal.head, add_parent, "the line popped past the add");
}

/// US-2/S4 (AC-2, the hard constraint): a delivery that fired between
/// the add and the undo changes nothing about the plan — the plan pins
/// the same `DeleteReminderRule` inverse with **no action addressing
/// the delivery record** (the record's own prune-on-rule-delete is the
/// pre-existing `DeleteReminderRule` action contract, the same shape a
/// hand removal takes; the undo adds no delivery-addressing action of
/// its own).
#[test]
fn us2_s4b_delivery_proof_untouched() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let rule = added_rule(&mut h, task, 1);
    h.disk
        .reminders
        .deliveries
        .insert(crate::reminders::ReminderDelivery {
            reminder: crate::reminders::ReminderRef::Rule(rule.id),
            occurrence: ts(h.now + 60),
            delivered_at: ts(h.now + 60),
        });
    h.sync();
    let plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    assert!(
        plan.actions
            .iter()
            .any(|a| matches!(a, PersistenceAction::DeleteReminderRule(id) if *id == rule.id)),
        "the rule edit still inverts"
    );
    assert!(
        !plan.actions.iter().any(|a| matches!(
            a,
            PersistenceAction::RecordReminderDismissal(_)
                | PersistenceAction::RecordReminderSnooze(_)
                | PersistenceAction::RecordReminderDelivery(_)
        )),
        "no action addresses the delivery record"
    );
}

/// US-3/S4 (AC-1, the catalogue-only verb): a journaled clear inverts
/// through one verbatim resurrect per cleared rule, id-ascending.
#[test]
fn us3_s4_cleared_set_resurrects_id_ascending() {
    let mut h = Harness::new();
    let board = h.board();
    let (_stack, tasks) = seeded_stack(&mut h, board, 1);
    let task = tasks[0];
    let mut rules = Vec::new();
    for seq in 1..=3 {
        rules.push(added_rule(&mut h, task, i64::from(seq)));
    }
    rules.sort_by_key(|rule| rule.id);
    h.exec(&StateCommand::ClearReminderRules { task })
        .expect("cleared");
    assert!(h.disk.reminders.rules.is_empty(), "the clear cleared");
    let plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    let resurrected: Vec<_> = plan
        .actions
        .iter()
        .filter_map(|a| match a {
            PersistenceAction::UpsertReminderRule(rule) => Some(rule.id),
            _ => None,
        })
        .collect();
    let expected: Vec<_> = rules.iter().map(|rule| rule.id).collect();
    assert_eq!(resurrected, expected, "id-ascending verbatim resurrect");
    for rule in &rules {
        assert_eq!(
            h.disk.reminders.rules.get(&rule.id),
            Some(rule),
            "each cleared rule came back verbatim"
        );
    }
}

#[test]
fn delete_label_journals_pre_images_and_undo_restores_label_and_memberships() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task_a = h.task(stack, "a");
    let task_b = h.task(stack, "b");
    let label = h.label(board);
    h.exec(&StateCommand::AssignLabel {
        task: task_a,
        label,
    })
    .expect("assigned");
    let plan = h
        .exec_plan(&StateCommand::DeleteLabel { id: label })
        .expect("deleted");
    // The delete journals the full cascade pre-images (label + stamped
    // membership removals).
    let Some(PersistenceAction::RecordJournal(entry)) = plan.actions.last() else {
        panic!("RecordJournal rides last");
    };
    assert!(matches!(entry.facts, UndoFacts::PreImages { .. }));
    assert!(h.disk.labels[&label].deleted);
    assert!(!matches!(
        h.disk.tasks[&task_a].label_memberships.get(&label),
        Some(crate::entities::MembershipState::Added { .. })
    ));
    h.exec(&StateCommand::Undo).expect("undone");
    // The label is live again…
    assert!(!h.disk.labels[&label].deleted);
    // …and the member task's membership re-Added through the inverse.
    assert!(matches!(
        h.disk.tasks[&task_a].label_memberships.get(&label),
        Some(crate::entities::MembershipState::Added { .. })
    ));
    // The unassigned member stays unassigned (the entry's own facts).
    assert!(!matches!(
        h.disk.tasks[&task_b].label_memberships.get(&label),
        Some(crate::entities::MembershipState::Added { .. })
    ));
}

#[test]
fn undo_of_delete_label_refuses_when_a_member_membership_diverged() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let checked = h.label(board);
    let unrelated = {
        h.exec(&StateCommand::CreateLabel {
            board,
            title: "unrelated".into(),
            color: crate::entities::Color::new("ff00ff"),
        })
        .expect("created");
        *h.disk.labels.keys().last().expect("label row")
    };
    for label in [checked, unrelated] {
        h.exec(&StateCommand::AssignLabel { task, label })
            .expect("assigned");
    }
    h.exec(&StateCommand::DeleteLabel { id: checked })
        .expect("deleted");
    // An unrelated label's membership edit must not refuse (the
    // covered-field discipline — only the deleted label's own
    // membership stamp is checked).
    h.exec(&StateCommand::UnassignLabel {
        task,
        label: unrelated,
    })
    .expect("unassigned unrelated");
    // Two retreats: past the unrelated edit, then the label restore.
    h.exec(&StateCommand::Undo)
        .expect("undone the unrelated edit");
    h.exec(&StateCommand::Undo).expect("restored the label");
    assert!(!h.disk.labels[&checked].deleted);
    // A fresh delete + a direct edit to the *checked* membership does
    // refuse, field-named.
    h.exec(&StateCommand::DeleteLabel { id: checked })
        .expect("re-deleted");
    let mut diverged = h.disk.tasks[&task].clone();
    diverged.label_memberships.insert(
        checked,
        crate::entities::MembershipState::Added { at: ts(5) },
    );
    h.disk.tasks.insert(task, diverged);
    h.sync();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Membership,
            ..
        })
    ));
}

#[test]
fn restore_label_on_a_live_label_is_the_accepted_noop() {
    let mut h = Harness::new();
    let board = h.board();
    let label = h.label(board);
    let plan = h
        .exec_plan(&StateCommand::RestoreLabel { id: label })
        .expect("planned");
    assert_eq!(plan.actions, Vec::new());
    assert!(matches!(plan.outcome, CommandOutcome::NoOp));
}

#[test]
fn restore_label_of_an_unknown_id_is_typed() {
    let mut h = Harness::new();
    let stranger = crate::ids::LabelId::from(uuid::Uuid::from_u128(7_777));
    let err = h.exec(&StateCommand::RestoreLabel { id: stranger });
    assert!(matches!(err, Err(CommandError::UnknownLabel(_))));
}

#[test]
fn restore_label_then_undo_re_deletes_and_redo_re_restores() {
    let mut h = Harness::new();
    let board = h.board();
    let label = h.label(board);
    h.exec(&StateCommand::DeleteLabel { id: label })
        .expect("deleted");
    let outcome = h
        .exec(&StateCommand::RestoreLabel { id: label })
        .expect("restored");
    assert!(matches!(outcome, CommandOutcome::LabelRestored { .. }));
    assert!(!h.disk.labels[&label].deleted);
    // Undo the restore: the label is tombstoned again through the
    // ordinary DeleteLabel.
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(h.disk.labels[&label].deleted);
    // Redo re-plans the forward intent: live again.
    h.exec(&StateCommand::Redo).expect("redone");
    assert!(!h.disk.labels[&label].deleted);
}

#[test]
fn delete_time_entry_journals_pre_images_and_undo_restores_the_row_verbatim() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let row_before = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    assert!(!h.disk.tracking.entries.contains_key(&entry_id));
    mark_journal_pushed(&mut h);
    h.exec(&StateCommand::Undo).expect("undone");
    // The row returns byte-identical (id, task, start/end, description,
    // both stamps — the pre-image payload is the resurrect source).
    assert_eq!(h.disk.tracking.entries.get(&entry_id), Some(&row_before));
    // The flush trigger enqueued (the delete's own UpdateAppData shape).
    assert!(
        h.disk.outbox.iter().any(
            |pending| matches!(pending.op, crate::outbox::LocalOp::UpdateAppData(t) if t == task)
        ),
        "the restore enqueued the document-lane flush trigger"
    );
}

#[test]
fn undo_of_delete_time_entry_refuses_when_a_row_reappeared() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let mut forged = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    // A present same-id row with different content is divergence (ids
    // are minted once): refuse, never clobber.
    forged.start = ts(405);
    h.disk.tracking.entries.insert(entry_id, forged);
    h.sync();
    let err = h.exec(&StateCommand::Undo);
    assert!(matches!(
        err,
        Err(CommandError::TargetNoLongerInvertible {
            field: UndoField::Deleted,
            ..
        })
    ));
}

#[test]
fn restore_time_entry_undo_re_deletes_the_restored_row() {
    // The #1187 regression: undoing a CLI `track restore` must answer
    // the re-delete inverse. The restore's own journal shape carries a
    // `before: None` time-entry pre-image (the row was hard-absent
    // pre-restore), which the inverse synthesis must accept — the
    // `Some`-demanding shape belongs to the delete family's journal.
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let payload = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(payload),
    })
    .expect("restored");
    h.exec(&StateCommand::Undo).expect("the restore undoes");
    assert!(
        !h.disk.tracking.entries.contains_key(&entry_id),
        "the undo re-deleted the restored row"
    );
    // Redo re-restores (the line's child is the restore, replayed
    // through the ordinary catalogue).
    h.exec(&StateCommand::Redo).expect("the re-delete redoes");
    assert!(
        h.disk.tracking.entries.contains_key(&entry_id),
        "the redo re-materialized the row"
    );
}

#[test]
fn restore_time_entry_round_trips_and_conflicts_typed() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let payload = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    // The forward verb restores a journaled payload verbatim.
    let outcome = h
        .exec(&StateCommand::RestoreTimeEntry {
            entry: Box::new(payload.clone()),
        })
        .expect("restored");
    assert!(matches!(outcome, CommandOutcome::EntryRestored { .. }));
    assert_eq!(h.disk.tracking.entries.get(&entry_id), Some(&payload));
    // A direct dispatch onto a present different row refuses typed.
    let mut mutant = payload.clone();
    mutant.start = ts(405);
    let err = h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(mutant),
    });
    assert!(matches!(err, Err(CommandError::TimeEntryAlreadyPresent(_))));
    // A gone owner task is a typed rejection.
    let mut orphan = payload.clone();
    orphan.task = crate::ids::TaskId::from_uuid(h.ids.new_task_id().as_uuid());
    let err = h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(orphan),
    });
    assert!(matches!(err, Err(CommandError::UnknownTask(_))));
}

#[test]
fn delete_habit_journals_pre_images_and_undo_restores_the_tombstone_target() {
    let mut h = Harness::new();
    let board = h.board();
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    let habit_id = *h.disk.habits.habits.keys().next().expect("habit row");
    let habit_before = h.disk.habits.habits[&habit_id].clone();
    h.exec(&StateCommand::LogHabit { habit: habit_id })
        .expect("logged");
    let log_before = h.disk.habits.log.clone();
    h.exec(&StateCommand::DeleteHabit { habit: habit_id })
        .expect("deleted");
    assert!(h.disk.habits.habits[&habit_id].deleted);
    h.exec(&StateCommand::Undo).expect("undone");
    // The habit is live again on its pre-tombstone values with a fresh
    // deleted clock; the log map is untouched (D26 — history outlives
    // the tracker, pinned as structural map equality).
    let after = &h.disk.habits.habits[&habit_id];
    assert!(!after.deleted);
    assert_eq!(after.title, habit_before.title);
    assert_eq!(after.target, habit_before.target);
    assert_eq!(after.order, habit_before.order);
    assert_eq!(after.clocks.title, habit_before.clocks.title);
    assert_eq!(after.clocks.target, habit_before.clocks.target);
    assert_eq!(after.clocks.order, habit_before.clocks.order);
    assert_eq!(h.disk.habits.log, log_before);
}

#[test]
fn restore_habit_on_a_live_habit_is_the_accepted_noop() {
    let mut h = Harness::new();
    let board = h.board();
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    let habit_id = *h.disk.habits.habits.keys().next().expect("habit row");
    let plan = h
        .exec_plan(&StateCommand::RestoreHabit { id: habit_id })
        .expect("planned");
    assert_eq!(plan.actions, Vec::new());
    assert!(matches!(plan.outcome, CommandOutcome::NoOp));
}

#[test]
fn restore_habit_then_undo_re_deletes_through_delete_habit() {
    let mut h = Harness::new();
    let board = h.board();
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    let habit_id = *h.disk.habits.habits.keys().next().expect("habit row");
    h.exec(&StateCommand::DeleteHabit { habit: habit_id })
        .expect("deleted");
    let outcome = h
        .exec(&StateCommand::RestoreHabit { id: habit_id })
        .expect("restored");
    assert!(matches!(outcome, CommandOutcome::HabitRestored { .. }));
    assert!(!h.disk.habits.habits[&habit_id].deleted);
    // Undo the restore: tombstoned again through the ordinary
    // DeleteHabit.
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(h.disk.habits.habits[&habit_id].deleted);
    // Redo re-plans the forward intent: live again.
    h.exec(&StateCommand::Redo).expect("redone");
    assert!(!h.disk.habits.habits[&habit_id].deleted);
}

#[test]
fn label_habit_and_time_entry_restore_entries_carry_the_board_tag() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    let label = h.label(board);
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let payload = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    let habit_id = *h.disk.habits.habits.keys().next().expect("habit row");
    h.exec(&StateCommand::DeleteLabel { id: label })
        .expect("label deleted");
    h.exec(&StateCommand::RestoreLabel { id: label })
        .expect("label restored");
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("entry deleted");
    h.exec(&StateCommand::RestoreTimeEntry {
        entry: Box::new(payload),
    })
    .expect("entry restored");
    h.exec(&StateCommand::DeleteHabit { habit: habit_id })
        .expect("habit deleted");
    h.exec(&StateCommand::RestoreHabit { id: habit_id })
        .expect("habit restored");
    for entry in &h.journal.entries {
        match entry.command {
            StateCommand::DeleteLabel { .. }
            | StateCommand::RestoreLabel { .. }
            | StateCommand::DeleteTimeEntry { .. }
            | StateCommand::RestoreTimeEntry { .. }
            | StateCommand::DeleteHabit { .. }
            | StateCommand::RestoreHabit { .. } => {
                assert_eq!(entry.board, Some(board), "tag for {entry:?}");
            }
            _ => {}
        }
    }
}

#[test]
fn a_retired_restore_class_reason_still_refuses_with_its_recorded_reason() {
    // Persisted-journal compatibility (D-7): a pre-#901 entry carrying
    // the retired `RestoreClass` reason decodes unchanged and keeps
    // refusing with that reason.
    let ids = CountingIds::new();
    let entry = JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command: StateCommand::RequestSync,
        facts: UndoFacts::NotUndoable(NotUndoableReason::RestoreClass),
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(100),
    };
    let journal = JournalState {
        entries: vec![entry],
        head: None,
    };
    let head = journal.entries[0].id;
    let journal = JournalState {
        head: Some(head),
        ..journal
    };
    let err = plan_undo(
        &Harness::new().app(),
        &[],
        &journal,
        UndoScope::Global,
        &ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        ts(200),
        None,
    )
    .expect_err("refuses");
    assert!(matches!(
        err,
        CommandError::TargetNotUndoable(NotUndoableReason::RestoreClass)
    ));
}

#[test]
fn the_restore_family_new_shapes_round_trip_serde() {
    let ids = CountingIds::new();
    let rule = crate::reminders::ReminderRule {
        id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(7_101)),
        task: crate::ids::TaskId::from(uuid::Uuid::from_u128(7_102)),
        kind: crate::reminders::ReminderKind::DueOffset(
            crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30))
                .expect("valid"),
        ),
        insistence: crate::reminders::Insistence::Normal,
        created_at: ts(100),
        updated_at: ts(100),
        by: None,
    };
    let payload = crate::tracking::TimeEntry {
        id: crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(7_103)),
        task: crate::ids::TaskId::from(uuid::Uuid::from_u128(7_102)),
        task_title: "t".into(),
        start: ts(400),
        end: Some(ts(500)),
        created_at: ts(400),
        updated_at: ts(400),
        kind: crate::tracking::TimeEntryKind::Work,
        by: None,
    };
    let entry = JournalEntry {
        id: ids.new_journal_id(),
        parent: None,
        command: StateCommand::RestoreTimeEntry {
            entry: Box::new(payload.clone()),
        },
        facts: UndoFacts::PreImages {
            images: vec![
                PreImage::ReminderRule {
                    id: rule.id,
                    before: Some(Box::new(rule.clone())),
                },
                PreImage::TimeEntry {
                    id: payload.id,
                    before: None,
                },
            ],
            watermark: UndoWatermark::default(),
        },
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ts(100),
    };
    let json = serde_json::to_string(&entry).expect("serializes");
    let back: JournalEntry = serde_json::from_str(&json).expect("decodes");
    assert_eq!(back, entry);
    // The three new arms decode byte-stably.
    for command in [
        StateCommand::RestoreLabel {
            id: crate::ids::LabelId::from(uuid::Uuid::from_u128(7_104)),
        },
        StateCommand::RestoreTimeEntry {
            entry: Box::new(payload),
        },
        StateCommand::RestoreHabit {
            id: crate::ids::HabitId::from(uuid::Uuid::from_u128(7_105)),
        },
    ] {
        let json = serde_json::to_string(&command).expect("serializes");
        let back: StateCommand = serde_json::from_str(&json).expect("decodes");
        assert_eq!(back, command);
    }
    // The retired reason decodes unchanged (serde stability).
    let json = serde_json::to_string(&NotUndoableReason::RestoreClass).unwrap();
    let back: NotUndoableReason = serde_json::from_str(&json).unwrap();
    assert_eq!(back, NotUndoableReason::RestoreClass);
}

// ---- the restore-family proptests ------------------------------------------

fn label_fixture(h: &mut Harness, board: crate::ids::BoardId) -> crate::ids::LabelId {
    h.label(board)
}

fn habit_fixture(h: &mut Harness, board: crate::ids::BoardId) -> crate::ids::HabitId {
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: habit_target(),
    })
    .expect("created");
    *h.disk.habits.habits.keys().next().expect("habit row")
}

fn entry_fixture(h: &mut Harness, task: crate::ids::TaskId) -> crate::ids::TimeEntryId {
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    *h.disk.tracking.entries.keys().next().expect("entry row")
}

proptest! {
    #![proptest_config(proptest_config(16))]

    /// P1 (the C8 P2 shape generalized): for a generated task, label
    /// (with a member), habit, and time entry, delete → undo (the
    /// restore) leaves every content field at its pre-delete value
    /// modulo the documented fresh-clock fields. (The restore-entry
    /// re-delete/redo legs ride the forward restore verbs — covered by
    /// the dedicated round-trip tests above; the head-move line makes
    /// undo-of-delete itself the restore step here.)
    #[test]
    fn restore_undo_round_trips_never_lose_fields(n in 1usize..=3) {
        let mut h = Harness::new();
        let board = h.board();
        let (_stack, tasks) = seeded_stack(&mut h, board, n);
        let label = label_fixture(&mut h, board);
        let habit = habit_fixture(&mut h, board);
        h.exec(&StateCommand::AssignLabel { task: tasks[0], label })
            .expect("assigned");
        let entry = entry_fixture(&mut h, tasks[0]);
        let pre_task = h.disk.tasks[&tasks[0]].clone();
        let pre_label = h.disk.labels[&label].clone();
        let pre_habit = h.disk.habits.habits[&habit].clone();
        let pre_entry = h.disk.tracking.entries[&entry].clone();

        // Task: delete → undo → every content field at its pre value.
        h.exec(&StateCommand::DeleteTask { id: tasks[0] }).expect("deleted");
        h.exec(&StateCommand::Undo).expect("restored");
        let after = &h.disk.tasks[&tasks[0]];
        prop_assert!(!after.deleted);
        prop_assert_eq!(&after.title, &pre_task.title);
        prop_assert_eq!(after.stack, pre_task.stack);
        prop_assert_eq!(after.order, pre_task.order);
        prop_assert_eq!(after.duedate, pre_task.duedate);
        prop_assert_eq!(after.done, pre_task.done);
        prop_assert_eq!(after.clocks.title, pre_task.clocks.title);
        prop_assert_eq!(after.clocks.position, pre_task.clocks.position);

        // Label: delete → undo → live again, title verbatim, the member
        // task's membership re-Added through the composite inverse.
        h.exec(&StateCommand::DeleteLabel { id: label }).expect("deleted");
        h.exec(&StateCommand::Undo).expect("restored");
        prop_assert!(!h.disk.labels[&label].deleted);
        prop_assert_eq!(&h.disk.labels[&label].title, &pre_label.title);
        let membership_added = matches!(
            h.disk.tasks[&tasks[0]].label_memberships.get(&label),
            Some(crate::entities::MembershipState::Added { .. })
        );
        prop_assert!(membership_added);

        // Habit: delete → undo → live again, title/target verbatim.
        h.exec(&StateCommand::DeleteHabit { habit }).expect("deleted");
        h.exec(&StateCommand::Undo).expect("restored");
        prop_assert!(!h.disk.habits.habits[&habit].deleted);
        prop_assert_eq!(&h.disk.habits.habits[&habit].title, &pre_habit.title);
        prop_assert_eq!(h.disk.habits.habits[&habit].target, pre_habit.target);

        // Time entry: delete → undo → the row returns verbatim.
        h.exec(&StateCommand::DeleteTimeEntry { id: entry }).expect("deleted");
        h.exec(&StateCommand::Undo).expect("restored");
        prop_assert_eq!(h.disk.tracking.entries.get(&entry), Some(&pre_entry));
    }

    /// P2: over a generated stack with N tasks × M rules, undo of the
    /// delete leaves exactly the rules the cascade removed — no more,
    /// no fewer — byte-identical.
    #[test]
    fn rule_resurrect_restores_exactly_the_cascaded_set(n in 1usize..=3, m in 1usize..=3) {
        let mut h = Harness::new();
        let board = h.board();
        let (stack, tasks) = seeded_stack(&mut h, board, n);
        let mut rules_before = std::collections::BTreeMap::new();
        for (i, task) in tasks.iter().enumerate() {
            for j in 0..m {
                let distinct: i64 = (i * 10 + j)
                    .try_into()
                    .expect("fixture sizes fit an i64");
                h.exec(&StateCommand::AddReminderRule {
                    task: *task,
                    offset: crate::reminders::ReminderOffset::before(
                        chrono::TimeDelta::try_minutes(-30 - distinct)
                            .expect("representable"),
                    )
                    .expect("valid"),
                    at: None,
                    insistence: crate::reminders::Insistence::Normal,
                })
                .expect("rule");
            }
        }
        for (id, rule) in &h.disk.reminders.rules {
            rules_before.insert(*id, rule.clone());
        }
        prop_assert_eq!(rules_before.len(), n * m);
        h.exec(&StateCommand::DeleteStack { id: stack }).expect("deleted");
        prop_assert!(h.disk.reminders.rules.is_empty());
        mark_journal_pushed(&mut h);
        h.exec(&StateCommand::Undo).expect("undone");
        prop_assert_eq!(h.disk.reminders.rules, rules_before);
    }
}

/// P3 (the D-6 fence property, re-proven over the migrated table): the
/// ceremony, reminder-surface, and reflection families never journal
/// `PreImages` — the #901 reclassification moved exactly three rows.
#[test]
fn ceremony_and_surface_fences_hold() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    // The #354 reclassification (UF1) moved Add/Remove/ClearReminderRule
    // out of the fenced family into the PreImages catalogue — their
    // coverage lives in the us1/us2/us3 fold tests below. The fence
    // keeps the delivery-surface representatives (the hard-constraint
    // rows) plus mute (R-1).
    let reminder_surface = [
        StateCommand::SnoozeReminder {
            reminder: crate::reminders::ReminderRef::Default(task),
            occurrence: ts(500),
            postpone: chrono::TimeDelta::minutes(5),
        },
        StateCommand::SetTaskRemindersMuted { task, muted: true },
        StateCommand::CreateAdhocAlarm {
            label: "a".into(),
            when: crate::reminders::AdhocWhen::In(chrono::TimeDelta::minutes(9)),
            insistence: crate::reminders::Insistence::Normal,
        },
    ];
    let reflection_surface = [StateCommand::RecordReflection {
        cadence: crate::reflection::ReviewCadence::Day,
        day: chrono::NaiveDate::from_ymd_opt(2026, 11, 18).unwrap(),
        window: (ts(100), ts(200)),
        text: "review".into(),
        mood: None,
        energy: None,
        flags: std::collections::BTreeSet::default(),
    }];
    let ceremonies = [
        StateCommand::RenameBoard {
            board,
            title: "renamed".into(),
        },
        StateCommand::ResetBoard { board },
    ];
    for command in reminder_surface
        .into_iter()
        .chain(reflection_surface)
        .chain(ceremonies)
    {
        if let Ok(plan) = h.exec_plan(&command) {
            if plan.actions.is_empty() {
                continue;
            }
            let Some(PersistenceAction::RecordJournal(entry)) = plan.actions.last() else {
                panic!("RecordJournal rides last for {command:?}");
            };
            assert!(
                !matches!(entry.facts, UndoFacts::PreImages { .. }),
                "a fenced family journaled PreImages for {command:?}"
            );
        }
    }
}

// ---- HF1: the skip/unskip mirror-verb inverse (2026-11-21) -------------

/// The skip's journal entry carries the prior row shape (or none) as
/// its pre-image; the undo plans the mirror verb; the mark flips and
/// the count survives; redo re-applies the mark. The `LogHabit` under
/// the skip stays reachable on the line (its own invertibility is
/// HF2's; here it simply pops as the next entry when it is newest).
#[test]
fn a_skip_journal_entry_inverts_through_the_mirror_verb() {
    let mut h = Harness::new();
    let board = h.board();
    let outcome = h
        .exec(&StateCommand::CreateHabit {
            board,
            title: "water".into(),
            target: crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(2).unwrap()),
        })
        .expect("accepted");
    let habit = match outcome {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    };
    h.exec(&StateCommand::LogHabit { habit }).expect("accepted");
    h.exec(&StateCommand::SkipHabit { habit })
        .expect("accepted");
    // The window date derives from the harness clock; find the touched
    // window from the journal entry instead of hardcoding.
    let entry = h.journal.entries.last().expect("journaled").clone();
    let key = match &entry.command {
        StateCommand::SkipHabit { .. } => h
            .disk
            .habits
            .log
            .keys()
            .find(|k| k.habit == habit && h.disk.habits.log[k].skip)
            .copied()
            .expect("the marked window"),
        _ => panic!("expected the skip journaled"),
    };
    assert!(h.disk.habits.log[&key].skip);
    assert_eq!(h.disk.habits.log[&key].count, 1);
    // Undo: the mirror verb flips the mark, keeps the count.
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(!h.disk.habits.log[&key].skip, "the mark cleared");
    assert_eq!(h.disk.habits.log[&key].count, 1, "the count survived");
    // Redo re-applies the skip intent.
    h.exec(&StateCommand::Redo).expect("redone");
    assert!(h.disk.habits.log[&key].skip, "redo re-applied the mark");
    // The pre-image round-trips through the persisted journal.
    let persisted = h.disk.clone();
    let restored: JournalState =
        serde_json::from_str(&serde_json::to_string(&persisted.journal).unwrap()).unwrap();
    let restored_entry = restored.entries.last().expect("journaled").clone();
    assert_eq!(restored_entry.command, entry.command);
}

/// The skip of an EMPTY window (pre-image "none" shape) inverts the
/// same way — the mark flips, the row stays a count-0 unmarked row
/// (analytically identical to absence for every projection).
#[test]
fn a_skip_of_an_empty_window_inverts_to_the_unmarked_row() {
    let mut h = Harness::new();
    let board = h.board();
    let outcome = h
        .exec(&StateCommand::CreateHabit {
            board,
            title: "run".into(),
            target: crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(1).unwrap()),
        })
        .expect("accepted");
    let habit = match outcome {
        CommandOutcome::HabitCreated(id) => id,
        other => panic!("expected HabitCreated, got {other:?}"),
    };
    h.exec(&StateCommand::SkipHabit { habit })
        .expect("accepted");
    let key = h
        .disk
        .habits
        .log
        .keys()
        .find(|k| k.habit == habit)
        .copied()
        .expect("the marked window");
    assert!(h.disk.habits.log[&key].skip);
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(
        !h.disk.habits.log[&key].skip,
        "the mark cleared on the none-pre-image shape too"
    );
    assert_eq!(h.disk.habits.log[&key].count, 0);
}

// ---- UF2: the UndoTo/RedoTo walk planners (#353) -----------------------

/// Builds the fork fixture the walk tests share: board → stack → task
/// "alpha" (entry 1), undo, task "beta" (entry 2) — two sibling
/// branches under the stack's fork, head on "beta".
fn fork_fixture() -> (Harness, crate::ids::JournalId, crate::ids::JournalId) {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let _alpha = h.task(stack, "alpha");
    h.exec(&StateCommand::Undo).expect("undo alpha");
    let _beta = h.task(stack, "beta");
    let beta_entry = h.journal.entries.last().expect("beta journaled").id;
    let alpha_entry = h.journal.entries[h.journal.entries.len() - 2].id;
    (h, alpha_entry, beta_entry)
}

/// US-2/AC-1 (W-5): the ancestor walk lands the head exactly on the
/// target — every intermediate inverse applied (the task the newer
/// entries created is gone from state), one merged batch, the receipt
/// naming the walk's final undone entry.
#[test]
fn us2_ac1_undo_to_lands_head_exactly_on_target() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let alpha = h.task(stack, "alpha");
    let beta = h.task(stack, "beta");
    let gamma = h.task(stack, "gamma");
    let alpha_entry = h.journal.entries[2].id;
    let beta_entry = h.journal.entries[3].id;
    let gamma_entry = h.journal.entries[4].id;
    // Batch shape: one plan, N steps' inverses, one head move onto the
    // target (the walk's internal head moves name the interim entries).
    let plan = h
        .exec_plan(&StateCommand::UndoTo {
            target: alpha_entry,
        })
        .expect("the ancestor walk commits");
    let target_moves = plan
        .actions
        .iter()
        .filter(|action| {
            matches!(action, PersistenceAction::MoveJournalHead(Some(id)) if *id == alpha_entry)
        })
        .count();
    assert_eq!(target_moves, 1, "the walk ends on the target's head move");
    assert!(matches!(plan.outcome, CommandOutcome::Undone { entry } if entry == beta_entry));
    assert_eq!(
        h.journal.head,
        Some(alpha_entry),
        "head lands on the target"
    );
    assert!(h.journal.entry(gamma_entry).is_some());
    // The state reflects the earlier point: beta and gamma are gone
    // (their inverses tombstoned them), alpha still stands.
    let tasks = h.app().tasks;
    assert!(tasks.get(&alpha).is_some_and(|task| !task.deleted));
    assert!(tasks.get(&beta).is_some_and(|task| task.deleted));
    assert!(tasks.get(&gamma).is_some_and(|task| task.deleted));
}

/// US-2/AC-2 (W-5): a target not on the current line refuses typed —
/// `TargetNotInLine` for both verbs, head unmoved — the stale picker
/// row after an intervening edit. `RedoTo` additionally refuses a
/// non-descendant sibling that `UndoTo` accepts (the direction law).
#[test]
fn us2_ac2_stale_target_refuses_typed() {
    let (mut h, alpha_entry, beta_entry) = fork_fixture();
    // Head is beta; alpha is a *sibling*, not an ancestor: UndoTo
    // refuses typed.
    let now = h.tick();
    let err = plan_undo_to(
        &h.app(),
        &h.outbox,
        &h.journal,
        alpha_entry,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    )
    .expect_err("a sibling is not an ancestor");
    assert!(matches!(err, CommandError::TargetNotInLine));
    assert_eq!(h.journal.head, Some(beta_entry), "head unmoved");
    // And an unknown/evicted id refuses for both verbs (no fork in
    // common with the head — the stale-picker-row case).
    let stranger = h.ids.new_journal_id();
    let now = h.tick();
    let err = plan_undo_to(
        &h.app(),
        &h.outbox,
        &h.journal,
        stranger,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    )
    .expect_err("a foreign id is not on the line");
    assert!(matches!(err, CommandError::TargetNotInLine));
    let now = h.tick();
    let err = plan_redo_to(
        &h.app(),
        &h.outbox,
        &h.journal,
        stranger,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    )
    .expect_err("a foreign id is not reachable");
    assert!(matches!(err, CommandError::TargetNotInLine));
}

/// US-1/S2b (W-5): the walk stops at a `NotUndoable` entry on the path
/// — the planner's own typed refusal surfaces verbatim (never a skip),
/// and because an error plan emits no actions the head stays exactly
/// where the walk started.
#[test]
fn us1_s2b_walk_stops_at_unreachable_target() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let _alpha = h.task(stack, "alpha");
    let board_entry = h.journal.entries[0].id;
    let alpha_entry = h.journal.entries[2].id;
    // Splice a `NotUndoable` surface entry between the board ceremony
    // and the task entry (the by-hand construction is the only way to
    // place a refusing entry below an undoable one — real surfaces
    // cannot journal a newer entry above an undone line). The walk's
    // law: the refusal stops the walk before crossing it, and the
    // ceremony's ancestors stay unreachable with it.
    let mut fence = h.journal.entries[1].clone();
    fence.id = h.ids.new_journal_id();
    fence.parent = Some(board_entry);
    fence.command = StateCommand::SnoozeReminder {
        reminder: crate::reminders::ReminderRef::Adhoc(crate::ids::AdhocAlarmId::from_uuid(
            uuid::Uuid::nil(),
        )),
        occurrence: chrono::DateTime::from_timestamp(1_000, 0).expect("timestamp"),
        postpone: chrono::TimeDelta::try_seconds(600).expect("positive duration"),
    };
    fence.facts = UndoFacts::NotUndoable(crate::journal::NotUndoableReason::ReminderSurface);
    let fence_id = fence.id;
    h.journal.entries.insert(2, fence);
    h.journal.entries[3].parent = Some(fence_id);
    // The splice rides the disk mirror too — the wrapper's exec
    // re-reads the journal from the persisted shape.
    h.disk.journal = h.journal.clone();
    h.sync();
    let beta_entry = h.journal.entries[3].id;
    assert_eq!(h.journal.head, Some(beta_entry));
    // The walk to the board ceremony stops at the fence: the planner's
    // typed refusal surfaces verbatim (never a skip), and because an
    // error plan emits no actions the head stays where the walk began.
    let now = h.tick();
    let err = plan_undo_to(
        &h.app(),
        &h.outbox,
        &h.journal,
        board_entry,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    )
    .expect_err("the fence blocks the walk");
    assert!(
        matches!(
            err,
            CommandError::TargetNotUndoable(crate::journal::NotUndoableReason::ReminderSurface)
        ),
        "the walk refuses before crossing the unreachable entry"
    );
    assert_eq!(h.journal.head, Some(beta_entry), "head unmoved");
    // The bounded target just below the fence (the fence itself) still
    // serves — landing *on* a refusing entry needs no crossing.
    let outcome = h
        .exec(&StateCommand::UndoTo { target: fence_id })
        .expect("the walk stops exactly at the fence");
    assert!(matches!(outcome, CommandOutcome::Undone { .. }));
    assert_eq!(h.journal.head, Some(fence_id));
    // The alpha entry (now parented under the fence) is unchanged.
    assert_eq!(
        h.journal.entry(alpha_entry).map(|e| e.parent),
        Some(Some(fence_id))
    );
}

/// US-3/AC-1 (W-5): the regression fence — plain `Redo` keeps the D5
/// greatest-child choice even with two siblings present. The picker
/// never changes the reflex's semantics; `RedoTo` is the only way to
/// the non-greatest branch.
#[test]
fn us3_ac1_redo_greatest_child_unchanged_with_siblings() {
    let (mut h, _alpha_entry, beta_entry) = fork_fixture();
    // The reflex's starting point is the picker's: the head sits at
    // the fork (beta undone), two siblings beneath it.
    h.exec(&StateCommand::Undo)
        .expect("undo beta back to the fork");
    let outcome = h.exec(&StateCommand::Redo).expect("redo plans");
    match outcome {
        CommandOutcome::Redone { entry } => {
            assert_eq!(entry, beta_entry, "greatest child (newest) wins");
        }
        other => panic!("expected Redone, got {other:?}"),
    }
}

/// US-1/S2 (W-5): the sibling walk — `RedoTo` takes the *non-greatest*
/// branch (the one thing repeated `Redo` cannot express), replans the
/// forward intents, and lands the head on the chosen branch's tip with
/// the receipt naming it. The v1 composes: plain `Redo` afterwards has
/// no children left on the chosen branch.
#[test]
fn us1_s2_redo_to_takes_the_chosen_sibling_branch() {
    let (mut h, alpha_entry, _beta_entry) = fork_fixture();
    // Without the picker verb, repeated Redo lands on beta (the fence
    // above); with it, alpha is reachable directly.
    let now = h.tick();
    let plan = plan_redo_to(
        &h.app(),
        &h.outbox,
        &h.journal,
        alpha_entry,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    )
    .expect("the sibling walk plans");
    // One merged batch, one final head move onto the branch tip.
    let head_moves = plan
        .actions
        .iter()
        .filter(|action| {
            matches!(action, PersistenceAction::MoveJournalHead(Some(id)) if *id == alpha_entry)
        })
        .count();
    assert_eq!(head_moves, 1, "the walk ends on the branch tip's move");
    assert!(matches!(plan.outcome, CommandOutcome::Redone { .. }));
    // Commit through the wrapper and verify the terminal state.
    let outcome = h
        .exec(&StateCommand::RedoTo {
            target: alpha_entry,
        })
        .expect("the sibling walk commits");
    match outcome {
        CommandOutcome::Redone { entry } => assert_eq!(entry, alpha_entry, "the tip is named"),
        other => panic!("expected Redone, got {other:?}"),
    }
    assert_eq!(h.journal.head, Some(alpha_entry));
    // The presented state reflects the chosen branch's head: the walk
    // undid beta back to the fork, then replanned alpha's forward
    // intent (a fresh live row — redo is a new edit).
    let tasks = h.app().tasks;
    let alpha_live = tasks
        .values()
        .any(|task| task.title == "alpha" && !task.deleted);
    let beta_live = tasks
        .values()
        .any(|task| task.title == "beta" && !task.deleted);
    assert!(alpha_live, "the chosen branch's content is live");
    assert!(!beta_live, "the abandoned sibling stays parked");
}

// ---- the zero-input journal-addressed restore (D-5, W-10) ---------------

/// US-5/AC-3 (W-10): a `DeleteTimeEntry` head whose capture carries no
/// row (`before: None`, the pre-#901 decode ghost's shape) refuses
/// typed — the bounded honesty of the retention law's edge. A
/// non-delete entry refuses with its own kind, and an unknown journal
/// id refuses before anything plans.
#[test]
fn us5_ac3_none_preimage_refuses_typed() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let _task = h.task(stack, "t");
    let non_delete_head = h.journal.head.expect("the line has entries");
    let ghost_id = crate::ids::JournalId::from_uuid(uuid::Uuid::now_v7());
    let entry_id = crate::ids::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    let ghost_stamp = h.tick();
    h.journal.entries.push(crate::journal::JournalEntry {
        id: ghost_id,
        parent: Some(non_delete_head),
        command: StateCommand::DeleteTimeEntry { id: entry_id },
        facts: UndoFacts::PreImages {
            images: vec![PreImage::TimeEntry {
                id: entry_id,
                before: None,
            }],
            watermark: UndoWatermark::default(),
        },
        ops: Vec::new(),
        pushed: false,
        board: None,
        created_at: ghost_stamp,
    });
    // Not-a-delete: an edit entry's id names no restorable row.
    match h.exec(&StateCommand::RestoreTimeEntryFromJournal {
        from: non_delete_head,
    }) {
        Err(CommandError::JournalEntryNotTimeEntryDelete(_)) => {}
        other => panic!("typed not-a-delete refusal, got {other:?}"),
    }
    // The `None` pre-image ghost.
    match h.exec(&StateCommand::RestoreTimeEntryFromJournal { from: ghost_id }) {
        Err(CommandError::JournalEntryPreImageMissing(_)) => {}
        other => panic!("typed none-preimage refusal, got {other:?}"),
    }
    // Unknown id (evicted or never existed).
    let unknown = crate::ids::JournalId::from_uuid(uuid::Uuid::now_v7());
    match h.exec(&StateCommand::RestoreTimeEntryFromJournal { from: unknown }) {
        Err(CommandError::UnknownJournalEntry(id)) => assert_eq!(id, unknown),
        other => panic!("typed unknown-id refusal, got {other:?}"),
    }
}

/// US-5/S3 (W-10): the wrapper reads the delete's pre-image and
/// re-materializes the row verbatim through the pinned payload verb;
/// the journaled head is the *wrapper* with `PreImages` of its own
/// writes, so the global undo re-deletes (the #1187 law) and redo
/// re-restores.
#[test]
fn us5_s3_from_journal_restores_verbatim_and_journals() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    let row_before = h.disk.tracking.entries[&entry_id].clone();
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    let delete_journal_id = h.journal.head.expect("the delete journaled");
    // A newer edit on top: plain undo would walk it back too — the
    // out-of-line restore must not.
    h.exec(&StateCommand::UpdateTask {
        id: task,
        changes: crate::messages::TaskChanges {
            title: Some("renamed on top".into()),
            ..Default::default()
        },
    })
    .expect("edited");
    let outcome = h
        .exec(&StateCommand::RestoreTimeEntryFromJournal {
            from: delete_journal_id,
        })
        .expect("restored from the journal");
    assert!(matches!(outcome, CommandOutcome::EntryRestored { id } if id == entry_id));
    assert_eq!(
        h.disk.tracking.entries.get(&entry_id),
        Some(&row_before),
        "the row came back verbatim from the pre-image"
    );
    // The journaled head is the wrapper itself.
    let head = h
        .journal
        .entry(h.journal.head.expect("the restore journaled"))
        .expect("head entry");
    assert_eq!(
        head.command,
        StateCommand::RestoreTimeEntryFromJournal {
            from: delete_journal_id
        },
        "the wrapper intent is what the line records"
    );
    assert!(matches!(head.facts, UndoFacts::PreImages { .. }));
    // Undo re-deletes (the #1187 law, the wrapper's own undo).
    h.exec(&StateCommand::Undo).expect("undone");
    assert!(!h.disk.tracking.entries.contains_key(&entry_id));
    h.exec(&StateCommand::Redo).expect("redone");
    assert_eq!(h.disk.tracking.entries.get(&entry_id), Some(&row_before));
}

/// US-6/S1 (W-10/W-11): the CLI-resolved journal id and the GUI
/// roster's journal id plan the *same* engine arm — the roster row's
/// `journal_id` is the wrapper's target, an already-restored equal row
/// is the accepted no-op (nothing journaled), and a direct
/// `plan_command` route is the typed `EngineRouted` rejection (the
/// wrapper is journal-addressed; the catalogue never sees it).
#[test]
fn us6_s1_cli_verb_and_gui_verb_share_the_engine_arm() {
    let mut h = Harness::new();
    let board = h.board();
    let stack = h.stack(board);
    let task = h.task(stack, "t");
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
        kind: crate::tracking::TimeEntryKind::Work,
    })
    .expect("logged");
    let entry_id = *h.disk.tracking.entries.keys().next().expect("entry row");
    h.exec(&StateCommand::DeleteTimeEntry { id: entry_id })
        .expect("deleted");
    // The roster's deleted-entry row carries the journal id both
    // surfaces resolve to.
    let roster = crate::roster::RestoreRoster::derive_parts(
        &h.disk.labels,
        &h.disk.habits,
        &h.disk.tracking,
        &h.journal,
    );
    let from = {
        let [row] = roster.deleted_entries.as_slice() else {
            panic!("one restorable row");
        };
        row.journal_id
    };
    let outcome = h
        .exec(&StateCommand::RestoreTimeEntryFromJournal { from })
        .expect("restored");
    assert!(matches!(outcome, CommandOutcome::EntryRestored { .. }));
    // The exact repeat: the accepted no-op, nothing journaled.
    let head_before = h.journal.head;
    let actions_before = h.journal.entries.len();
    let outcome = h
        .exec(&StateCommand::RestoreTimeEntryFromJournal { from })
        .expect("repeat planned");
    assert!(matches!(outcome, CommandOutcome::NoOp));
    assert_eq!(h.journal.head, head_before, "the no-op journals nothing");
    assert_eq!(h.journal.entries.len(), actions_before);
    // The catalogue route is typed-closed.
    let now = h.tick();
    match plan_command(
        &h.app(),
        &h.outbox,
        &StateCommand::RestoreTimeEntryFromJournal { from },
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        now,
        None,
    ) {
        Err(CommandError::EngineRouted) => {}
        other => panic!("the wrapper is journal-routed, got {other:?}"),
    }
}
