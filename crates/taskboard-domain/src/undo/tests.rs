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
            board: crate::ids::RemoteBoardId(7),
            stack: crate::ids::RemoteStackId(8),
            card: crate::ids::RemoteCardId(9),
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
    let undo_plan = h.exec_plan(&StateCommand::Undo).expect("undone");
    eprintln!("DBG pushed: {}", h.journal.entries.last().unwrap().pushed);
    eprintln!(
        "DBG actions: {:?}",
        undo_plan
            .actions
            .iter()
            .map(|a| format!("{a:?}"))
            .collect::<Vec<_>>()
    );
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
    // Tracking surface (StopTracking with nothing open is a NoOp — use
    // a LogTime instead).
    h.exec(&StateCommand::LogTime {
        task,
        start: ts(400),
        end: ts(500),
    })
    .expect("logged");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::TrackingSurface
        ))
    ));
    // Habit surface.
    h.exec(&StateCommand::CreateHabit {
        board,
        title: "habit".into(),
        target: crate::habit::HabitTarget::PerDay(
            crate::habit::NonZeroCount::new(1).expect("valid quota"),
        ),
    })
    .expect("created");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::HabitSurface
        ))
    ));
    // Restore class.
    h.exec(&StateCommand::DeleteStack { id: stack })
        .expect("deleted");
    assert!(matches!(
        h.exec(&StateCommand::Undo),
        Err(CommandError::TargetNotUndoable(
            NotUndoableReason::RestoreClass
        ))
    ));
    // The head never moved through all the refusals.
    assert_eq!(
        h.journal.head,
        Some(h.journal.entries.last().unwrap().id),
        "a refusal does not advance the line"
    );
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
    let h = Harness::new();
    // Direct dispatch through the plain catalogue is a typed rejection
    // (the wrapper is the only legal route).
    let err = crate::command::plan_command(
        &h.app(),
        &[],
        &StateCommand::Undo,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        ts(h.now),
    )
    .expect_err("typed");
    assert!(matches!(err, CommandError::EngineRouted));
    let err = crate::command::plan_command(
        &h.app(),
        &[],
        &StateCommand::Redo,
        &h.ids,
        &crate::reminders::ReminderDefaults::default(),
        Harness::day(),
        ts(h.now),
    )
    .expect_err("typed");
    assert!(matches!(err, CommandError::EngineRouted));
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
        })
        .expect("logged");
        *h.disk.tracking.entries.keys().next().expect("entry row")
    };
    let rule_id = {
        h.exec(&StateCommand::AddReminderRule {
            task,
            offset: crate::reminders::ReminderOffset::before(chrono::TimeDelta::minutes(-30))
                .expect("valid"),
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
            remote: crate::ids::RemoteBoardId(1),
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
        StateCommand::StartTracking { task },
        StateCommand::StopTracking,
        StateCommand::LogTime {
            task,
            start: ts(600),
            end: ts(700),
        },
        StateCommand::AdjustTimeEntry {
            id: entry_id,
            start: None,
            end: Some(ts(650)),
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
        // The destructive ceremonies go last: they hard-remove the rows
        // the earlier arms read fixtures from.
        StateCommand::UnbindBoard { board },
        StateCommand::ReconcileByTitle { board },
        StateCommand::DeleteStack { id: stack },
        StateCommand::ResetBoard { board },
        StateCommand::DeleteBoard { board },
        StateCommand::RequestSync,
    ];
    for command in &commands {
        // Undo/Redo route to the line (no journaling classification) —
        // exercised in the line-mechanics tests; skip here.
        if matches!(command, StateCommand::Undo | StateCommand::Redo) {
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
            board: crate::ids::RemoteBoardId(7),
            stack: crate::ids::RemoteStackId(8),
            card: crate::ids::RemoteCardId(9),
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
                archived: ts(1),
                deleted: ts(1),
            },
            remote_seen: None,
            remote_baseline: None,
        }),
    });
    assert!(matches!(err, Err(CommandError::UnknownTask(_))));
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
