// SPDX-License-Identifier: MIT OR Apache-2.0
//! The local command semantics catalogue: what each [`StateCommand`]
//! *means* against an [`AppState`].
//!
//! [`plan_command`] is pure, total, and clock/id-injected — the command
//! counterpart of [`crate::pipeline::apply_sync_report`]. It returns the
//! persistence batch plus a typed receipt; it can never fail for
//! transport or lifecycle reasons (those are the state-engine crate's
//! [`ExecuteError`](https://docs.rs/taskboard-state) classes).
//!
//! Semantics highlights (the contract, pinned per-case in tests):
//! - every stamp is the injected `now`; fresh ids come from the injected
//!   [`IdGenerator`];
//! - *terminal* intents repeated are accepted no-ops
//!   ([`CommandOutcome::NoOp`], zero actions): delete-already-deleted,
//!   assign-already-assigned, move-to-identical-position, and so on. An
//!   *edit*-style intent against a tombstone is a [`CommandError`]
//!   because it cannot be meaningless-redundant;
//! - a locally-deleted *bound* entity keeps its remote binding until the
//!   push confirms (R9c); an *unbound* entity (never pushed) cancels its
//!   pending ops instead of enqueueing a delete the server could not
//!   understand;
//! - `DeleteStack` cascades tombstones to the stack's live tasks with the
//!   same bound/unbound split (mirroring the remote R6 cascade so the
//!   local view is immediately consistent);
//! - op coalescing is Phase 4 push-side logic and deliberately absent
//!   here: every accepted intent enqueues its own op.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::entities::{Label, Stack, Task, TaskClocks};
use crate::idgen::IdGenerator;
use crate::ids::{BoardId, LabelId, ReminderRuleId, RemoteBoardId, StackId, TaskId};
use crate::messages::{LabelChanges, StateCommand, TaskChanges};
use crate::outbox::{LocalOp, OpId, PendingOp, op_targets_task};
use crate::persistence::PersistenceAction;
use crate::reminders::{ReminderRef, ReminderSnooze};
use crate::state::AppState;

/// A planned command: the persistence batch and the user-facing receipt.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandPlan {
    /// Entity upserts and outbox transitions, in emission order.
    pub actions: Vec<PersistenceAction>,
    /// What the command did (a fresh id for creates, `NoOp` for
    /// idempotent repeats).
    pub outcome: CommandOutcome,
}

/// Semantic rejection classes of [`plan_command`]: what a command *means*
/// against the current state. Transport and lifecycle failures live in
/// the engine crate's `ExecuteError`, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    /// No task with this id exists.
    #[error("unknown task")]
    UnknownTask(TaskId),
    /// The task is tombstoned; edit-style intents cannot target it.
    #[error("task is deleted")]
    TaskDeleted(TaskId),
    /// No stack with this id exists.
    #[error("unknown stack")]
    UnknownStack(StackId),
    /// The stack is tombstoned; intents cannot target it.
    #[error("stack is deleted")]
    StackDeleted(StackId),
    /// No label with this id exists.
    #[error("unknown label")]
    UnknownLabel(LabelId),
    /// The label is tombstoned; edit-style intents cannot target it.
    #[error("label is deleted")]
    LabelDeleted(LabelId),
    /// No live board is bound, so board-scoped creates are impossible.
    #[error("no live board is bound")]
    NoBoard,
    /// A live board already exists; the single-board MVP refuses a second.
    #[error("a live board already exists")]
    BoardExists,
    /// The live board is already bound to a remote board (differing from
    /// the bind target).
    #[error("the board is already bound to a remote board")]
    BoardAlreadyBound(RemoteBoardId),
    /// No reminder rule with this id exists.
    #[error("unknown reminder rule")]
    UnknownReminderRule(ReminderRuleId),
    /// A snooze postpone must be positive ("snooze" that does not move
    /// the occurrence forward is not a snooze).
    #[error("snooze postpone must be positive")]
    InvalidSnoozePostpone,
}

/// Typed receipt of an accepted command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The command created a task with this id.
    CreatedTask(TaskId),
    /// The command created a stack with this id.
    CreatedStack(StackId),
    /// The command created a label with this id.
    CreatedLabel(LabelId),
    /// The command created the board with this id.
    CreatedBoard(BoardId),
    /// The command replaced a task's reminder rules; carries the new
    /// rule's id.
    ReminderSet {
        /// The fresh rule.
        rule: ReminderRuleId,
    },
    /// The command removed a task's reminder rules.
    ReminderCleared {
        /// How many rules were deleted.
        removed: usize,
    },
    /// The command muted or unmuted a task's reminder firings.
    ReminderMuted {
        /// The affected task.
        task: TaskId,
        /// The new mute state.
        muted: bool,
    },
    /// The command changed state.
    Applied,
    /// Accepted, redundant repeat: nothing changed, nothing persisted.
    NoOp,
    /// Sync request accepted; the engine forwards it to the sync actor.
    SyncRequested,
}

/// The smallest live board id, deterministically (documented totality for
/// the pre-multi-board window when several could coexist; the MVP holds
/// 0..1).
fn live_board(app: &AppState) -> Option<BoardId> {
    app.boards
        .values()
        .filter(|b| !b.deleted)
        .map(|b| b.id)
        .min()
}

fn live_task(app: &AppState, id: TaskId) -> Result<&Task, CommandError> {
    let task = app.tasks.get(&id).ok_or(CommandError::UnknownTask(id))?;
    if task.deleted {
        Err(CommandError::TaskDeleted(id))
    } else {
        Ok(task)
    }
}

/// An existing task for a *terminal* intent: deleted is not an error here
/// (the repeat is an accepted no-op), only absence is.
fn existing_task(app: &AppState, id: TaskId) -> Result<&Task, CommandError> {
    app.tasks.get(&id).ok_or(CommandError::UnknownTask(id))
}

fn live_stack(app: &AppState, id: StackId) -> Result<&Stack, CommandError> {
    let stack = app.stacks.get(&id).ok_or(CommandError::UnknownStack(id))?;
    if stack.deleted {
        Err(CommandError::StackDeleted(id))
    } else {
        Ok(stack)
    }
}

fn existing_stack(app: &AppState, id: StackId) -> Result<&Stack, CommandError> {
    app.stacks.get(&id).ok_or(CommandError::UnknownStack(id))
}

fn live_label(app: &AppState, id: LabelId) -> Result<&Label, CommandError> {
    let label = app.labels.get(&id).ok_or(CommandError::UnknownLabel(id))?;
    if label.deleted {
        Err(CommandError::LabelDeleted(id))
    } else {
        Ok(label)
    }
}

fn existing_label(app: &AppState, id: LabelId) -> Result<&Label, CommandError> {
    app.labels.get(&id).ok_or(CommandError::UnknownLabel(id))
}

fn op_targets_label(op: &LocalOp, label: LabelId) -> bool {
    matches!(
        op,
        LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) if *id == label
    )
}

/// The pending ops of the outbox that address `task` (used to cancel a
/// locally-deleted *unbound* task's queued writes — the server never knew
/// the entity).
fn task_ops(outbox: &[PendingOp], task: TaskId) -> Vec<OpId> {
    outbox
        .iter()
        .filter(|pending| op_targets_task(&pending.op, task))
        .map(|pending| pending.op_id)
        .collect()
}

fn label_ops(outbox: &[PendingOp], label: LabelId) -> Vec<OpId> {
    outbox
        .iter()
        .filter(|pending| op_targets_label(&pending.op, label))
        .map(|pending| pending.op_id)
        .collect()
}

/// The D-c delete cascade: every rule watching `task` is tombstoned with
/// it (rule ids are known — no id-generator involvement). Deliveries,
/// snooze links, and the task's mute entry stay: inert while the task is
/// dead (the evaluator requires live + open).
fn task_rule_deletes(app: &AppState, task: TaskId) -> impl Iterator<Item = PersistenceAction> {
    app.reminders
        .rules
        .values()
        .filter(move |rule| rule.task == task)
        .map(|rule| PersistenceAction::DeleteReminderRule(rule.id))
}

/// Plans one local command against the current view.
///
/// Returns the persistence batch and the receipt. The function is total:
/// any (state, command) combination yields a plan or a typed
/// [`CommandError`], never a panic. The outbox never influences the
/// outcome: rejections and no-ops are identical for any queue contents,
/// and accepted plans differ only in the `FailOp`s the delete paths
/// derive from it.
///
/// # Errors
///
/// The [`CommandError`] variant matching the violated precondition
/// (unknown/deleted task, stack, or label; no live board). Transport and
/// lifecycle failures are not representable here.
///
/// `outbox` is the engine's current queue slice: the delete paths cancel
/// a target's pending ops by id, exactly like
/// [`crate::pipeline::apply_sync_report`] takes it.
#[allow(clippy::too_many_lines)] // one catalogue; splitting hides the table
pub fn plan_command(
    app: &AppState,
    outbox: &[PendingOp],
    command: &StateCommand,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> Result<CommandPlan, CommandError> {
    match command {
        StateCommand::CreateTask {
            title,
            stack,
            order,
        } => {
            let target = live_stack(app, *stack)?;
            let id = ids.new_task_id();
            let task = Task {
                id,
                remote: None,
                title: title.clone(),
                description: String::new(),
                duedate: None,
                done: None,
                stack: target.id,
                order: *order,
                labels: BTreeSet::default(),
                archived: false,
                deleted: false,
                clocks: all_task_clocks(now),
                remote_seen: None,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(task),
                    enqueue(LocalOp::CreateTask(id), ids, now),
                ],
                outcome: CommandOutcome::CreatedTask(id),
            })
        }
        StateCommand::UpdateTask { id, changes } => {
            // Existence before the changeset guard, like every other
            // command: an empty edit against an unknown/deleted task is a
            // rejection, not a silent no-op.
            let task = live_task(app, *id)?;
            if changes == &TaskChanges::default() {
                return Ok(no_op());
            }
            let mut updated = task.clone();
            if let Some(title) = &changes.title {
                updated.title.clone_from(title);
                updated.clocks.title = now;
            }
            if let Some(description) = &changes.description {
                updated.description.clone_from(description);
                updated.clocks.description = now;
            }
            if let Some(duedate) = changes.duedate {
                updated.duedate = duedate;
                updated.clocks.duedate = now;
            }
            if let Some(archived) = changes.archived {
                updated.archived = archived;
                updated.clocks.archived = now;
            }
            Ok(applied_task(&updated, ids, now))
        }
        StateCommand::SetTaskDone { id, done } => {
            let task = live_task(app, *id)?;
            if task.done.is_some() == *done {
                return Ok(no_op());
            }
            let mut updated = task.clone();
            updated.done = done.then_some(now);
            updated.clocks.done = now;
            Ok(applied_task(&updated, ids, now))
        }
        StateCommand::MoveTask { id, stack, order } => {
            let task = live_task(app, *id)?;
            if task.stack == *stack && task.order == *order {
                return Ok(no_op());
            }
            let target = live_stack(app, *stack)?;
            let mut updated = task.clone();
            updated.stack = target.id;
            updated.order = *order;
            // Composite intent: stack + order share one write clock.
            updated.clocks.position = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(updated),
                    enqueue(LocalOp::MoveTask(*id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::DeleteTask { id } => {
            // Terminal intent: a repeat is an accepted no-op, not an error.
            let task = existing_task(app, *id)?;
            if task.deleted {
                return Ok(no_op());
            }
            let mut updated = task.clone();
            updated.deleted = true;
            // The binding survives until the push confirms (R9c).
            updated.clocks.deleted = now;
            let mut actions = vec![PersistenceAction::UpsertTask(updated)];
            // D-c: the task's reminder rules cannot outlive it.
            actions.extend(task_rule_deletes(app, *id));
            if task.remote.is_some() {
                actions.push(enqueue(LocalOp::DeleteTask(*id), ids, now));
            } else {
                // Unbound (never pushed): cancel the queued writes; the
                // server never knew the entity.
                actions.extend(
                    task_ops(outbox, *id)
                        .into_iter()
                        .map(PersistenceAction::FailOp),
                );
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::CreateStack { title, order } => {
            let board = live_board(app).ok_or(CommandError::NoBoard)?;
            let id = ids.new_stack_id();
            let stack = Stack {
                id,
                remote: None,
                board,
                title: title.clone(),
                order: *order,
                archived: false,
                deleted: false,
                clocks: crate::entities::StackClocks {
                    title: now,
                    order: now,
                    deleted: now,
                },
                remote_seen: None,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertStack(stack),
                    enqueue(LocalOp::CreateStack(id), ids, now),
                ],
                outcome: CommandOutcome::CreatedStack(id),
            })
        }
        StateCommand::RenameStack { id, new_title } => {
            let stack = live_stack(app, *id)?;
            if stack.title == *new_title {
                return Ok(no_op());
            }
            let mut updated = stack.clone();
            updated.title.clone_from(new_title);
            updated.clocks.title = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertStack(updated),
                    enqueue(LocalOp::RenameStack(*id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::MoveStack { id, order } => {
            let stack = live_stack(app, *id)?;
            if stack.order == *order {
                return Ok(no_op());
            }
            let mut updated = stack.clone();
            updated.order = *order;
            updated.clocks.order = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertStack(updated),
                    enqueue(LocalOp::MoveStack(*id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::DeleteStack { id } => {
            // Terminal intent: a repeat is an accepted no-op.
            let stack = existing_stack(app, *id)?;
            if stack.deleted {
                return Ok(no_op());
            }
            let mut updated = stack.clone();
            updated.deleted = true;
            updated.clocks.deleted = now;
            let mut actions = vec![PersistenceAction::UpsertStack(updated)];

            // Cascade: every live task of the stack follows the DeleteTask
            // bound/unbound split (mirroring the remote R6 cascade so the
            // local view matches what the server will do).
            let live_tasks: Vec<Task> = app
                .tasks
                .values()
                .filter(|task| task.is_live() && task.stack == *id)
                .cloned()
                .collect();
            for task in live_tasks {
                let mut tombstoned = task.clone();
                tombstoned.deleted = true;
                tombstoned.clocks.deleted = now;
                actions.push(PersistenceAction::UpsertTask(tombstoned));
                // D-c: the cascade tombstones the task's reminder rules too.
                actions.extend(task_rule_deletes(app, task.id));
                if task.remote.is_some() {
                    actions.push(enqueue(LocalOp::DeleteTask(task.id), ids, now));
                } else {
                    actions.extend(
                        task_ops(outbox, task.id)
                            .into_iter()
                            .map(PersistenceAction::FailOp),
                    );
                }
            }
            actions.push(enqueue(LocalOp::DeleteStack(*id), ids, now));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::CreateLabel { title, color } => {
            let board = live_board(app).ok_or(CommandError::NoBoard)?;
            let id = ids.new_label_id();
            let label = Label {
                id,
                remote: None,
                board,
                title: title.clone(),
                color: color.clone(),
                deleted: false,
                clocks: crate::entities::LabelClocks {
                    title: now,
                    color: now,
                    deleted: now,
                },
                remote_seen: None,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertLabel(label),
                    enqueue(LocalOp::CreateLabel(id), ids, now),
                ],
                outcome: CommandOutcome::CreatedLabel(id),
            })
        }
        StateCommand::UpdateLabel { id, changes } => {
            // Existence before the changeset guard, like `UpdateTask`.
            let label = live_label(app, *id)?;
            if changes == &LabelChanges::default() {
                return Ok(no_op());
            }
            let mut updated = label.clone();
            if let Some(title) = &changes.title {
                updated.title.clone_from(title);
                updated.clocks.title = now;
            }
            if let Some(color) = &changes.color {
                updated.color = color.clone();
                updated.clocks.color = now;
            }
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertLabel(updated),
                    enqueue(LocalOp::UpdateLabel(*id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::DeleteLabel { id } => {
            // Terminal intent: a repeat is an accepted no-op.
            let label = existing_label(app, *id)?;
            if label.deleted {
                return Ok(no_op());
            }
            let mut updated = label.clone();
            updated.deleted = true;
            // Task label sets stay untouched: the tombstone hides the
            // label; R7 reconciles remotely.
            updated.clocks.deleted = now;
            let mut actions = vec![PersistenceAction::UpsertLabel(updated)];
            if label.remote.is_some() {
                actions.push(enqueue(LocalOp::DeleteLabel(*id), ids, now));
            } else {
                actions.extend(
                    label_ops(outbox, *id)
                        .into_iter()
                        .map(PersistenceAction::FailOp),
                );
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::AssignLabel { task, label } => {
            let source = live_task(app, *task)?;
            live_label(app, *label)?;
            if source.labels.contains(label) {
                return Ok(no_op());
            }
            let mut updated = source.clone();
            updated.labels.insert(*label);
            // Whole-set field: one clock covers the entire label set.
            updated.clocks.labels = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(updated),
                    enqueue(LocalOp::AssignLabel(*task, *label), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::UnassignLabel { task, label } => {
            let source = live_task(app, *task)?;
            live_label(app, *label)?;
            if !source.labels.contains(label) {
                return Ok(no_op());
            }
            let mut updated = source.clone();
            updated.labels.remove(label);
            updated.clocks.labels = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(updated),
                    enqueue(LocalOp::UnassignLabel(*task, *label), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::CreateBoard { title, color } => {
            // Single-board invariant, create-side guard.
            if live_board(app).is_some() {
                return Err(CommandError::BoardExists);
            }
            let id = ids.new_board_id();
            let board = crate::entities::Board {
                id,
                remote: None,
                title: title.clone(),
                color: color.clone(),
                archived: false,
                deleted: false,
                // Board creation is a local write of every field.
                clocks: crate::entities::BoardClocks {
                    title: now,
                    color: now,
                    archived: now,
                    deleted: now,
                },
                remote_seen: None,
            };
            // No outbox op: a local board creation is not a remote intent —
            // server-side creation is the publish ceremony's job.
            Ok(CommandPlan {
                actions: vec![PersistenceAction::UpsertBoard(board)],
                outcome: CommandOutcome::CreatedBoard(id),
            })
        }
        StateCommand::RenameBoard { title } => {
            let board_id = live_board(app).ok_or(CommandError::NoBoard)?;
            let board = &app.boards[&board_id];
            if board.title == *title {
                return Ok(no_op());
            }
            let mut updated = board.clone();
            updated.title.clone_from(title);
            updated.clocks.title = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertBoard(updated),
                    enqueue(LocalOp::UpdateBoard(board_id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::SetBoardColor { color } => {
            let board_id = live_board(app).ok_or(CommandError::NoBoard)?;
            let board = &app.boards[&board_id];
            if board.color == *color {
                return Ok(no_op());
            }
            let mut updated = board.clone();
            updated.color = color.clone();
            updated.clocks.color = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertBoard(updated),
                    enqueue(LocalOp::UpdateBoard(board_id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::SetBoardArchived { archived } => {
            let board_id = live_board(app).ok_or(CommandError::NoBoard)?;
            let board = &app.boards[&board_id];
            if board.archived == *archived {
                return Ok(no_op());
            }
            let mut updated = board.clone();
            updated.archived = *archived;
            updated.clocks.archived = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertBoard(updated),
                    enqueue(LocalOp::UpdateBoard(board_id), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::DeleteBoard => {
            // Terminal intent: a repeat (a tombstoned board still exists)
            // is an accepted no-op; only total absence is a rejection.
            let Some(board_id) = live_board(app) else {
                if app.boards.is_empty() {
                    return Err(CommandError::NoBoard);
                }
                return Ok(no_op());
            };
            let mut actions = Vec::new();

            // Tombstone with binding kept (R9c: the push clears it).
            let board = &app.boards[&board_id];
            let mut tombstoned = board.clone();
            tombstoned.deleted = true;
            tombstoned.clocks.deleted = now;
            actions.push(PersistenceAction::UpsertBoard(tombstoned));

            // Cascade (the remote R6 shape, applied at command time so the
            // local view matches what the server will do): live stacks,
            // their live tasks, and the board's live labels follow; the
            // D-c reminder-rule cascade fires per task.
            let doomed_stacks: Vec<StackId> = app
                .stacks
                .values()
                .filter(|s| s.board == board_id && !s.deleted)
                .map(|s| s.id)
                .collect();
            let doomed_tasks: Vec<TaskId> = app
                .tasks
                .values()
                .filter(|t| t.is_live() && doomed_stacks.contains(&t.stack))
                .map(|t| t.id)
                .collect();
            let doomed_labels: Vec<LabelId> = app
                .labels
                .values()
                .filter(|l| l.board == board_id && !l.deleted)
                .map(|l| l.id)
                .collect();
            for stack_id in &doomed_stacks {
                let mut tombstoned = app.stacks[stack_id].clone();
                tombstoned.deleted = true;
                tombstoned.clocks.deleted = now;
                actions.push(PersistenceAction::UpsertStack(tombstoned));
            }
            for task_id in &doomed_tasks {
                let mut tombstoned = app.tasks[task_id].clone();
                tombstoned.deleted = true;
                tombstoned.clocks.deleted = now;
                actions.push(PersistenceAction::UpsertTask(tombstoned));
                actions.extend(task_rule_deletes(app, *task_id));
            }
            for label_id in &doomed_labels {
                let mut tombstoned = app.labels[label_id].clone();
                tombstoned.deleted = true;
                tombstoned.clocks.deleted = now;
                actions.push(PersistenceAction::UpsertLabel(tombstoned));
            }

            // The board DELETE cascades server-side; pushing per-child
            // deletes would chase 404s through the board's corpse. Every
            // queued op addressing the board or any of its children is
            // cancelled instead (the pipeline's remote-death model).
            let doomed_tasks: std::collections::BTreeSet<TaskId> =
                doomed_tasks.into_iter().collect();
            let doomed_stacks: std::collections::BTreeSet<StackId> =
                doomed_stacks.into_iter().collect();
            let doomed_labels: std::collections::BTreeSet<LabelId> =
                doomed_labels.into_iter().collect();
            actions.extend(
                outbox
                    .iter()
                    .filter(|pending| match pending.op {
                        LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => id == board_id,
                        LocalOp::CreateTask(id)
                        | LocalOp::UpdateTask(id)
                        | LocalOp::MoveTask(id)
                        | LocalOp::DeleteTask(id)
                        | LocalOp::AssignLabel(id, _)
                        | LocalOp::UnassignLabel(id, _) => doomed_tasks.contains(&id),
                        LocalOp::CreateStack(id)
                        | LocalOp::RenameStack(id)
                        | LocalOp::MoveStack(id)
                        | LocalOp::DeleteStack(id) => doomed_stacks.contains(&id),
                        LocalOp::CreateLabel(id)
                        | LocalOp::UpdateLabel(id)
                        | LocalOp::DeleteLabel(id) => doomed_labels.contains(&id),
                    })
                    .map(|pending| PersistenceAction::FailOp(pending.op_id)),
            );

            // Exactly one wire intent, uniformly for bound and unbound
            // boards (the planner noop-completes an unbound group).
            actions.push(enqueue(LocalOp::DeleteBoard(board_id), ids, now));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::ResetBoard => {
            let board_id = live_board(app).ok_or(CommandError::NoBoard)?;
            // The already-empty probe: content, the board's queued ops,
            // and its tasks' reminder facts. (Validators ride the bulk
            // action unconditionally; with zero content there is nothing
            // a 304 could serve stale.) Zero of all three means the
            // repair has nothing to do (house rule: nothing changed,
            // nothing persisted).
            let board_stacks: std::collections::BTreeSet<StackId> = app
                .stacks
                .values()
                .filter(|s| s.board == board_id)
                .map(|s| s.id)
                .collect();
            let board_tasks: std::collections::BTreeSet<TaskId> = app
                .tasks
                .values()
                .filter(|t| board_stacks.contains(&t.stack))
                .map(|t| t.id)
                .collect();
            let board_labels: std::collections::BTreeSet<LabelId> = app
                .labels
                .values()
                .filter(|l| l.board == board_id)
                .map(|l| l.id)
                .collect();
            let board_ops = outbox.iter().any(|pending| match pending.op {
                LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => id == board_id,
                LocalOp::CreateTask(id)
                | LocalOp::UpdateTask(id)
                | LocalOp::MoveTask(id)
                | LocalOp::DeleteTask(id)
                | LocalOp::AssignLabel(id, _)
                | LocalOp::UnassignLabel(id, _) => board_tasks.contains(&id),
                LocalOp::CreateStack(id)
                | LocalOp::RenameStack(id)
                | LocalOp::MoveStack(id)
                | LocalOp::DeleteStack(id) => board_stacks.contains(&id),
                LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                    board_labels.contains(&id)
                }
            });
            let board_reminders = app
                .reminders
                .rules
                .values()
                .any(|r| board_tasks.contains(&r.task))
                || app.reminders.deliveries.iter().any(|d| match d.reminder {
                    ReminderRef::Default(t) => board_tasks.contains(&t),
                    // A surviving rule reference belongs to a live rule,
                    // which the rules arm above already covers.
                    ReminderRef::Rule(_) => false,
                })
                || board_tasks.iter().any(|t| app.reminders.muted.contains(t));
            if board_stacks.is_empty()
                && board_tasks.is_empty()
                && board_labels.is_empty()
                && !board_ops
                && !board_reminders
            {
                return Ok(no_op());
            }
            // One bulk action: the row deletes are the repository's SQL
            // (scoped by board id / subqueries); the board row, its
            // binding, clocks, and the sync status survive.
            Ok(CommandPlan {
                actions: vec![PersistenceAction::ClearBoardContent(board_id)],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::BindBoard { remote } => {
            let board_id = live_board(app).ok_or(CommandError::NoBoard)?;
            let board = &app.boards[&board_id];
            match board.remote {
                Some(bound) if bound == *remote => return Ok(no_op()),
                Some(bound) => return Err(CommandError::BoardAlreadyBound(bound)),
                None => {}
            }
            let mut updated = board.clone();
            updated.remote = Some(*remote);
            Ok(CommandPlan {
                actions: vec![PersistenceAction::UpsertBoard(updated)],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::SnoozeReminder {
            reminder,
            occurrence,
            postpone,
        } => {
            // Edit-style intent: snoozing a dead task's reminder is a
            // rejection, not a meaningless-redundant repeat.
            let task = match *reminder {
                ReminderRef::Rule(rule_id) => {
                    let rule = app
                        .reminders
                        .rules
                        .get(&rule_id)
                        .ok_or(CommandError::UnknownReminderRule(rule_id))?;
                    rule.task
                }
                ReminderRef::Default(task) => task,
            };
            live_task(app, task)?;
            if *postpone <= chrono::TimeDelta::zero() {
                return Err(CommandError::InvalidSnoozePostpone);
            }
            // Total arithmetic: an unrepresentable target saturates at
            // chrono's ceiling instead of panicking the planner.
            let rescheduled_to = now
                .checked_add_signed(*postpone)
                .unwrap_or(DateTime::<Utc>::MAX_UTC);
            Ok(CommandPlan {
                actions: vec![PersistenceAction::RecordReminderSnooze(ReminderSnooze {
                    reminder: *reminder,
                    occurrence: *occurrence,
                    rescheduled_to,
                })],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::SetTaskRemindersMuted { task, muted } => {
            // Existence before the membership guard, like every other
            // command.
            live_task(app, *task)?;
            if app.reminders.muted.contains(task) == *muted {
                return Ok(no_op());
            }
            Ok(CommandPlan {
                actions: vec![PersistenceAction::SetTaskRemindersMuted(*task, *muted)],
                outcome: CommandOutcome::ReminderMuted {
                    task: *task,
                    muted: *muted,
                },
            })
        }
        StateCommand::SetReminderRule { task, offset } => {
            // Edit-style intent: reminding a dead task is a rejection,
            // not a meaningless-redundant repeat.
            live_task(app, *task)?;
            // Replace-all: the task's existing rules are tombstoned by
            // id (the same delete cascade `DeleteTask` uses), then the
            // fresh rule is upserted. Rules are local-only — no outbox
            // op is enqueued (reminder sync is out of scope).
            let mut actions: Vec<PersistenceAction> = task_rule_deletes(app, *task).collect();
            let rule = crate::reminders::ReminderRule {
                id: ids.new_reminder_rule_id(),
                task: *task,
                offset: *offset,
                created_at: now,
            };
            actions.push(PersistenceAction::UpsertReminderRule(rule.clone()));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::ReminderSet { rule: rule.id },
            })
        }
        StateCommand::ClearReminderRules { task } => {
            live_task(app, *task)?;
            let deletes: Vec<PersistenceAction> = task_rule_deletes(app, *task).collect();
            if deletes.is_empty() {
                return Ok(no_op());
            }
            let removed = deletes.len();
            Ok(CommandPlan {
                actions: deletes,
                outcome: CommandOutcome::ReminderCleared { removed },
            })
        }
        StateCommand::RequestSync => Ok(CommandPlan {
            actions: Vec::new(),
            outcome: CommandOutcome::SyncRequested,
        }),
    }
}

fn enqueue(op: LocalOp, ids: &dyn IdGenerator, now: DateTime<Utc>) -> PersistenceAction {
    PersistenceAction::EnqueueOp(PendingOp {
        op_id: ids.new_op_id(),
        op,
        queued_at: now,
    })
}

fn all_task_clocks(now: DateTime<Utc>) -> TaskClocks {
    TaskClocks {
        title: now,
        description: now,
        duedate: now,
        done: now,
        position: now,
        labels: now,
        archived: now,
        deleted: now,
    }
}

fn no_op() -> CommandPlan {
    CommandPlan {
        actions: Vec::new(),
        outcome: CommandOutcome::NoOp,
    }
}

/// An applied task edit: the upsert plus the generic `UpdateTask` op (all
/// task edits except create/move/delete push as update).
fn applied_task(task: &Task, ids: &dyn IdGenerator, now: DateTime<Utc>) -> CommandPlan {
    CommandPlan {
        actions: vec![
            PersistenceAction::UpsertTask(task.clone()),
            enqueue(LocalOp::UpdateTask(task.id), ids, now),
        ],
        outcome: CommandOutcome::Applied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{LabelClocks, StackClocks};
    use crate::test_support::CountingIds;
    use chrono::TimeZone;

    const T0: i64 = 36_000; // 10:00:00Z, the injected "now" of the tests
    const BOARD: u64 = 800;
    const STACK: u64 = 900;
    const TASK: u64 = 1;
    const LABEL: u64 = 500;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn now() -> DateTime<Utc> {
        ts(T0)
    }

    fn clocks(secs: i64) -> TaskClocks {
        TaskClocks {
            title: ts(secs),
            description: ts(secs),
            duedate: ts(secs),
            done: ts(secs),
            position: ts(secs),
            labels: ts(secs),
            archived: ts(secs),
            deleted: ts(secs),
        }
    }

    fn task(id: u64, stack_id: StackId, bound: bool) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(u128::from(id))),
            remote: bound.then_some(crate::ids::RemoteCardRef {
                board: crate::ids::RemoteBoardId(77),
                stack: crate::ids::RemoteStackId(1),
                card: crate::ids::RemoteCardId(id),
            }),
            title: format!("task {id}"),
            description: String::new(),
            duedate: None,
            done: None,
            stack: stack_id,
            order: 1,
            labels: BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: clocks(T0 - 3_600),
            remote_seen: None,
        }
    }

    fn stack(id: u64, bound: bool) -> Stack {
        Stack {
            id: StackId::from(uuid::Uuid::from_u128(u128::from(id))),
            remote: bound.then_some(crate::ids::RemoteStackRef {
                board: crate::ids::RemoteBoardId(77),
                stack: crate::ids::RemoteStackId(id),
            }),
            board: BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD))),
            title: format!("stack {id}"),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(T0 - 3_600),
                order: ts(T0 - 3_600),
                deleted: ts(T0 - 3_600),
            },
            remote_seen: None,
        }
    }

    fn label(id: u64, bound: bool) -> Label {
        Label {
            id: LabelId::from(uuid::Uuid::from_u128(u128::from(id))),
            remote: bound.then_some(crate::ids::RemoteLabelRef {
                board: crate::ids::RemoteBoardId(77),
                label: crate::ids::RemoteLabelId(id),
            }),
            board: BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD))),
            title: format!("label {id}"),
            color: crate::entities::Color::new("ff0000"),
            deleted: false,
            clocks: LabelClocks {
                title: ts(T0 - 3_600),
                color: ts(T0 - 3_600),
                deleted: ts(T0 - 3_600),
            },
            remote_seen: None,
        }
    }

    /// Fixture: one live bound board, one live stack, one live bound task,
    /// one live bound label, empty outbox.
    fn base() -> (AppState, Vec<PendingOp>) {
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let stack_id = StackId::from(uuid::Uuid::from_u128(u128::from(STACK)));
        let app = AppState {
            boards: [(
                board_id,
                crate::entities::Board {
                    id: board_id,
                    remote: Some(crate::ids::RemoteBoardId(77)),
                    title: "board".into(),
                    color: crate::entities::Color::new("0000ff"),
                    archived: false,
                    deleted: false,
                    clocks: crate::entities::BoardClocks {
                        title: crate::entities::BoardClocks::epoch(),
                        color: crate::entities::BoardClocks::epoch(),
                        archived: crate::entities::BoardClocks::epoch(),
                        deleted: crate::entities::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            )]
            .into_iter()
            .collect(),
            stacks: [(stack_id, stack(STACK, true))].into_iter().collect(),
            tasks: [(task(TASK, stack_id, true).id, task(TASK, stack_id, true))]
                .into_iter()
                .collect(),
            labels: [(label(LABEL, true).id, label(LABEL, true))]
                .into_iter()
                .collect(),
            reminders: crate::reminders::ReminderState::default(),
            sync: crate::state::SyncStates::new(),
            last_updated: None,
        };
        let task_id = TaskId::from(uuid::Uuid::from_u128(u128::from(TASK)));
        let label_id = LabelId::from(uuid::Uuid::from_u128(u128::from(LABEL)));
        // The fixture task already carries the fixture label in its set,
        // so unassign tests have something to remove.
        let mut app = app;
        app.tasks.get_mut(&task_id).unwrap().labels.insert(label_id);
        (app, Vec::new())
    }

    fn stack_id() -> StackId {
        StackId::from(uuid::Uuid::from_u128(u128::from(STACK)))
    }

    fn task_id() -> TaskId {
        TaskId::from(uuid::Uuid::from_u128(u128::from(TASK)))
    }

    fn label_id() -> LabelId {
        LabelId::from(uuid::Uuid::from_u128(u128::from(LABEL)))
    }

    fn run(
        app: &AppState,
        outbox: &[PendingOp],
        command: &StateCommand,
    ) -> Result<CommandPlan, CommandError> {
        plan_command(app, outbox, command, &CountingIds::default(), now())
    }

    fn op_for(raw: u128, op: LocalOp) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(raw)),
            op,
            queued_at: ts(T0 - 60),
        }
    }

    // ---- CreateTask --------------------------------------------------

    #[test]
    fn create_task_builds_fresh_entity_and_enqueues_create() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateTask {
                title: "new".into(),
                stack: stack_id(),
                order: 7,
            },
        )
        .unwrap();
        let outcome = plan.outcome.clone();
        let CommandOutcome::CreatedTask(id) = &outcome else {
            panic!("wrong outcome");
        };
        let PersistenceAction::UpsertTask(created) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(&created.id, id);
        assert_eq!(created.title, "new");
        assert_eq!(created.stack, stack_id());
        assert_eq!(created.order, 7);
        assert!(!created.deleted && created.done.is_none() && created.remote.is_none());
        assert_eq!(created.clocks, all_task_clocks(now()));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::CreateTask(created_id) if created_id == *id));
    }

    #[test]
    fn create_task_rejects_unknown_and_deleted_stack() {
        let (app, outbox) = base();
        let missing = StackId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::CreateTask {
                    title: "x".into(),
                    stack: missing,
                    order: 1,
                }
            ),
            Err(CommandError::UnknownStack(missing))
        );

        let mut with_dead = app.clone();
        let mut dead = stack(901, true);
        dead.deleted = true;
        with_dead.stacks.insert(dead.id, dead.clone());
        let err = run(
            &with_dead,
            &outbox,
            &StateCommand::CreateTask {
                title: "x".into(),
                stack: dead.id,
                order: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, CommandError::StackDeleted(dead_id) if dead_id == dead.id));
    }

    // ---- UpdateTask / SetTaskDone / MoveTask -------------------------

    #[test]
    fn update_task_stamps_only_touched_field_clocks() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    title: Some("edited".into()),
                    duedate: Some(Some(ts(T0 + 86_400))),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.title, "edited");
        assert_eq!(updated.duedate, Some(ts(T0 + 86_400)));
        assert_eq!(updated.clocks.title, now());
        assert_eq!(updated.clocks.duedate, now());
        // Untouched fields keep their clocks — including position, labels,
        // and the tombstone clock.
        assert_eq!(updated.clocks.position, ts(T0 - 3_600));
        assert_eq!(updated.clocks.labels, ts(T0 - 3_600));
        assert_eq!(updated.clocks.done, ts(T0 - 3_600));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateTask(id) if id == task_id()));
    }

    #[test]
    fn empty_changeset_is_a_no_op() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges::default(),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn empty_changeset_against_unknown_target_is_still_rejected() {
        // Coherence with every other command: the existence guard fires
        // before the changeset guard.
        let (app, outbox) = base();
        let missing = TaskId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::UpdateTask {
                    id: missing,
                    changes: TaskChanges::default(),
                }
            ),
            Err(CommandError::UnknownTask(missing))
        );
        let missing_label = LabelId::from(uuid::Uuid::from_u128(4243));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::UpdateLabel {
                    id: missing_label,
                    changes: LabelChanges::default(),
                }
            ),
            Err(CommandError::UnknownLabel(missing_label))
        );
    }

    #[test]
    fn update_task_rejects_unknown_and_deleted() {
        let (app, outbox) = base();
        let missing = TaskId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::UpdateTask {
                    id: missing,
                    changes: TaskChanges {
                        title: Some("x".into()),
                        ..Default::default()
                    },
                }
            ),
            Err(CommandError::UnknownTask(missing))
        );

        let mut dead_state = app.clone();
        let mut dead = task(2, stack_id(), true);
        dead.deleted = true;
        dead.clocks.deleted = ts(T0 - 30);
        let dead_id = dead.id;
        dead_state.tasks.insert(dead_id, dead);
        assert_eq!(
            run(
                &dead_state,
                &outbox,
                &StateCommand::UpdateTask {
                    id: dead_id,
                    changes: TaskChanges {
                        title: Some("x".into()),
                        ..Default::default()
                    },
                }
            ),
            Err(CommandError::TaskDeleted(dead_id))
        );
    }

    #[test]
    fn set_task_done_stamps_done_clock_and_toggles() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: true,
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.done, Some(now()));
        assert_eq!(updated.clocks.done, now());

        // Reopen path.
        let mut done_state = app.clone();
        let mut done_task = done_state.tasks[&task_id()].clone();
        done_task.done = Some(ts(T0 - 10));
        done_state.tasks.insert(done_task.id, done_task);
        let plan = run(
            &done_state,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: false,
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.done, None);
        assert_eq!(updated.clocks.done, now());
    }

    #[test]
    fn update_task_archived_edit_flips_the_flag_and_stamps_the_clock() {
        // F3-11: the arm `task edit --archive` drives (command.rs
        // UpdateTask.archived): archived is a field like any other — flag
        // flips, its clock stamps, untouched clocks stand, and the generic
        // update op enqueues.
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    archived: Some(true),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert!(updated.archived);
        assert_eq!(updated.clocks.archived, now());
        assert_eq!(updated.clocks.title, ts(T0 - 3_600));
        assert_eq!(updated.clocks.duedate, ts(T0 - 3_600));
        assert_eq!(updated.clocks.position, ts(T0 - 3_600));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateTask(id) if id == task_id()));
    }

    #[test]
    fn move_to_a_different_stack_with_unchanged_order_is_not_a_no_op() {
        // The order conjunct of the MoveTask no-op guard: a same-order
        // move across stacks is a real move (position is one composite
        // field — the stack half alone changes it).
        let (mut app, outbox) = base();
        let other = stack(901, true);
        app.stacks.insert(other.id, other.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::MoveTask {
                id: task_id(),
                stack: other.id,
                order: 1,
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.stack, other.id);
        assert_eq!(updated.order, 1);
        assert_eq!(updated.clocks.position, now());
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::MoveTask(id) if id == task_id()));
    }

    #[test]
    fn stack_delete_cascade_spares_live_tasks_of_other_stacks() {
        // The cascade filter conjunction: only live tasks *of the deleted
        // stack* follow the delete — another stack's live bound task is
        // absent from the plan entirely.
        let (mut app, outbox) = base();
        let other = stack(901, true);
        app.stacks.insert(other.id, other.clone());
        let spared = task(2, other.id, true);
        let spared_id = spared.id;
        app.tasks.insert(spared_id, spared);

        let plan = run(&app, &outbox, &StateCommand::DeleteStack { id: stack_id() }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert!(
            !plan
                .actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.id == spared_id)),
            "the other stack's task is not upserted",
        );
        assert!(
            !plan.actions.iter().any(|a| matches!(
                a,
                PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::DeleteTask(id) if id == spared_id)
            )),
            "the other stack's task is not cascaded",
        );
        let deletes: Vec<&PersistenceAction> = plan
            .actions
            .iter()
            .filter(|a| matches!(a, PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::DeleteTask(_))))
            .collect();
        assert_eq!(deletes.len(), 1, "only the in-stack task cascades");
        let PersistenceAction::EnqueueOp(op) = deletes[0] else {
            unreachable!("filtered above");
        };
        assert!(matches!(op.op, LocalOp::DeleteTask(id) if id == task_id()));
    }

    #[test]
    fn set_task_done_to_current_state_is_a_no_op() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: false,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn move_task_stamps_the_composite_position_clock_once() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::MoveTask {
                id: task_id(),
                stack: stack_id(),
                order: 42,
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.order, 42);
        assert_eq!(updated.clocks.position, now());
        assert_eq!(updated.clocks.title, ts(T0 - 3_600));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::MoveTask(id) if id == task_id()));
    }

    #[test]
    fn move_to_identical_position_is_a_no_op() {
        let (app, outbox) = base();
        let current = &app.tasks[&task_id()];
        let plan = run(
            &app,
            &outbox,
            &StateCommand::MoveTask {
                id: task_id(),
                stack: current.stack,
                order: current.order,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn move_task_rejects_deleted_target_stack() {
        let (app, outbox) = base();
        let mut dead = stack(902, true);
        dead.deleted = true;
        let dead_id = dead.id;
        let mut state = app.clone();
        state.stacks.insert(dead_id, dead);
        let err = run(
            &state,
            &outbox,
            &StateCommand::MoveTask {
                id: task_id(),
                stack: dead_id,
                order: 1,
            },
        )
        .unwrap_err();
        assert!(matches!(err, CommandError::StackDeleted(id) if id == dead_id));
    }

    // ---- DeleteTask: bound vs unbound --------------------------------

    #[test]
    fn delete_bound_task_tombstones_keeps_binding_and_enqueues_delete() {
        let (app, outbox) = base();
        let plan = run(&app, &outbox, &StateCommand::DeleteTask { id: task_id() }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert!(updated.deleted);
        assert_eq!(updated.clocks.deleted, now());
        assert!(
            updated.remote.is_some(),
            "binding survives until the push (R9c)"
        );
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("bound task must enqueue a delete op");
        };
        assert!(matches!(op.op, LocalOp::DeleteTask(id) if id == task_id()));
    }

    #[test]
    fn delete_unbound_task_cancels_its_ops_and_enqueues_nothing() {
        let stack_id = stack_id();
        let mut app = base().0;
        let unbound = task(3, stack_id, false);
        let unbound_id = unbound.id;
        app.tasks.insert(unbound_id, unbound);
        let outbox = vec![
            op_for(11, LocalOp::UpdateTask(unbound_id)),
            op_for(12, LocalOp::UpdateTask(task_id())),
        ];
        let plan = run(&app, &outbox, &StateCommand::DeleteTask { id: unbound_id }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert!(
            !plan
                .actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::EnqueueOp(_))),
            "an unbound delete must not enqueue a server delete"
        );
        assert_eq!(
            plan.actions,
            vec![
                PersistenceAction::UpsertTask({
                    let mut t = task(3, stack_id, false);
                    t.deleted = true;
                    t.clocks.deleted = now();
                    t
                }),
                PersistenceAction::FailOp(OpId(uuid::Uuid::from_u128(11))),
            ],
            "exactly the target's own pending op is cancelled"
        );
    }

    #[test]
    fn delete_already_deleted_task_is_a_no_op() {
        let (mut app, outbox) = base();
        let mut dead = task(4, stack_id(), true);
        dead.deleted = true;
        let dead_id = dead.id;
        app.tasks.insert(dead_id, dead);
        let plan = run(&app, &outbox, &StateCommand::DeleteTask { id: dead_id }).unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn delete_unknown_task_is_an_error() {
        let (app, outbox) = base();
        let missing = TaskId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(&app, &outbox, &StateCommand::DeleteTask { id: missing }),
            Err(CommandError::UnknownTask(missing))
        );
    }

    // ---- Stacks -------------------------------------------------------

    #[test]
    fn create_stack_lands_on_the_live_board() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateStack {
                title: "column".into(),
                order: 3,
            },
        )
        .unwrap();
        let CommandOutcome::CreatedStack(id) = &plan.outcome else {
            panic!("wrong outcome");
        };
        let PersistenceAction::UpsertStack(created) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(&created.id, id);
        assert_eq!(
            created.board,
            BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)))
        );
        assert_eq!(
            created.clocks,
            StackClocks {
                title: now(),
                order: now(),
                deleted: now()
            }
        );
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::CreateStack(created_id) if created_id == *id));
    }

    #[test]
    fn create_stack_without_a_live_board_is_rejected() {
        let (mut app, outbox) = base();
        for board in app.boards.values_mut() {
            board.deleted = true;
        }
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::CreateStack {
                    title: "x".into(),
                    order: 1,
                }
            ),
            Err(CommandError::NoBoard)
        );
        // No board at all is the same class.
        let mut empty = base().0;
        empty.boards.clear();
        assert_eq!(
            run(
                &empty,
                &outbox,
                &StateCommand::CreateStack {
                    title: "x".into(),
                    order: 1,
                }
            ),
            Err(CommandError::NoBoard)
        );
    }

    #[test]
    fn rename_stack_stamps_title_clock_and_skips_identical() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::RenameStack {
                id: stack_id(),
                new_title: "renamed".into(),
            },
        )
        .unwrap();
        let PersistenceAction::UpsertStack(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.title, "renamed");
        assert_eq!(updated.clocks.title, now());
        assert_eq!(updated.clocks.order, ts(T0 - 3_600));

        let plan = run(
            &app,
            &outbox,
            &StateCommand::RenameStack {
                id: stack_id(),
                new_title: app.stacks[&stack_id()].title.clone(),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn rename_board_stamps_the_title_clock_and_enqueues_the_op() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::RenameBoard {
                title: "renamed".into(),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.title, "renamed");
        assert_eq!(updated.clocks.title, now());
        let PersistenceAction::EnqueueOp(pending) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert_eq!(
            pending.op,
            LocalOp::UpdateBoard(BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD))))
        );
    }

    #[test]
    fn rename_board_with_unchanged_title_is_a_no_op() {
        let (app, outbox) = base();
        let title = app.boards[&BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)))]
            .title
            .clone();
        let plan = run(&app, &outbox, &StateCommand::RenameBoard { title }).unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn rename_board_without_a_live_board_is_no_board() {
        let (mut app, outbox) = base();
        app.boards.clear();
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::RenameBoard { title: "x".into() }
            ),
            Err(CommandError::NoBoard)
        );
    }

    #[test]
    fn move_stack_stamps_order_clock_enqueues_and_persists() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::MoveStack {
                id: stack_id(),
                order: 7,
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertStack(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.order, 7);
        assert_eq!(updated.clocks.order, now());
        assert_eq!(
            updated.clocks.title,
            ts(T0 - 3_600),
            "title clock untouched"
        );
        let PersistenceAction::EnqueueOp(pending) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert_eq!(pending.op, LocalOp::MoveStack(stack_id()));
    }

    #[test]
    fn move_stack_with_unchanged_order_is_a_no_op_without_an_outbox_op() {
        let (app, outbox) = base();
        let order = app.stacks[&stack_id()].order;
        let plan = run(
            &app,
            &outbox,
            &StateCommand::MoveStack {
                id: stack_id(),
                order,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn move_stack_rejects_unknown_and_deleted_stacks() {
        let (mut app, outbox) = base();
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::MoveStack {
                    id: StackId::from(uuid::Uuid::from_u128(999)),
                    order: 1,
                }
            ),
            Err(CommandError::UnknownStack(StackId::from(
                uuid::Uuid::from_u128(999)
            )))
        );
        let mut dead = stack(903, true);
        dead.deleted = true;
        let dead_id = dead.id;
        app.stacks.insert(dead_id, dead);
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::MoveStack {
                    id: dead_id,
                    order: 1,
                }
            ),
            Err(CommandError::StackDeleted(dead_id))
        );
    }

    #[test]
    fn rename_deleted_stack_is_rejected_but_delete_repeat_is_a_no_op() {
        let (mut app, outbox) = base();
        let mut dead = stack(903, true);
        dead.deleted = true;
        let dead_id = dead.id;
        app.stacks.insert(dead_id, dead);
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::RenameStack {
                    id: dead_id,
                    new_title: "x".into(),
                }
            ),
            Err(CommandError::StackDeleted(dead_id))
        );
        let plan = run(&app, &outbox, &StateCommand::DeleteStack { id: dead_id }).unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn delete_stack_cascades_bound_and_unbound_tasks() {
        let stack_id = stack_id();
        let (app, _outbox) = base();
        let mut unbound = task(5, stack_id, false);
        unbound.title = "unbound".into();
        let unbound_id = unbound.id;
        let mut state = app;
        state.tasks.insert(unbound_id, unbound);
        let outbox = vec![
            op_for(21, LocalOp::UpdateTask(unbound_id)),
            op_for(22, LocalOp::CreateStack(stack_id)),
        ];

        let plan = run(&state, &outbox, &StateCommand::DeleteStack { id: stack_id }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);

        // Stack tombstone first, then per-task upserts.
        let PersistenceAction::UpsertStack(deleted_stack) = &plan.actions[0] else {
            panic!("first action must be the stack upsert");
        };
        assert!(deleted_stack.deleted);

        let bound_id = task_id();
        let task_upserts: Vec<&Task> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::UpsertTask(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(
            task_upserts.len(),
            2,
            "both live tasks of the stack cascade"
        );
        assert!(task_upserts.iter().all(|t| t.deleted));

        let enqueued: Vec<&LocalOp> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::EnqueueOp(op) => Some(&op.op),
                _ => None,
            })
            .collect();
        assert!(
            enqueued
                .iter()
                .any(|op| matches!(op, LocalOp::DeleteStack(sid) if *sid == stack_id))
        );
        assert_eq!(
            enqueued
                .iter()
                .filter(|op| matches!(op, LocalOp::DeleteTask(id) if *id == bound_id))
                .count(),
            1,
            "the bound task's delete is pushed"
        );
        assert!(
            !enqueued
                .iter()
                .any(|op| matches!(op, LocalOp::DeleteTask(id) if *id == unbound_id)),
            "the unbound task's delete is not pushed"
        );

        let failed: Vec<OpId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::FailOp(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(
            failed,
            vec![OpId(uuid::Uuid::from_u128(21))],
            "only the cascaded unbound task's pending op is cancelled; the stack op stays"
        );
    }

    // ---- Labels -------------------------------------------------------

    #[test]
    fn create_update_delete_label_roundtrip() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateLabel {
                title: "urgent".into(),
                color: crate::entities::Color::new("00ff00"),
            },
        )
        .unwrap();
        let CommandOutcome::CreatedLabel(id) = &plan.outcome else {
            panic!("wrong outcome");
        };
        let PersistenceAction::UpsertLabel(created) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(&created.id, id);
        assert_eq!(
            created.clocks,
            LabelClocks {
                title: now(),
                color: now(),
                deleted: now()
            }
        );

        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateLabel {
                id: label_id(),
                changes: LabelChanges {
                    title: None,
                    color: Some(crate::entities::Color::new("0000ff")),
                },
            },
        )
        .unwrap();
        let PersistenceAction::UpsertLabel(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.color.as_str(), "0000ff");
        assert_eq!(updated.clocks.color, now());
        assert_eq!(updated.clocks.title, ts(T0 - 3_600));

        let plan = run(&app, &outbox, &StateCommand::DeleteLabel { id: label_id() }).unwrap();
        let PersistenceAction::UpsertLabel(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert!(updated.deleted);
        assert!(
            updated.remote.is_some(),
            "binding survives until the push (R9c)"
        );
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("bound label must enqueue a delete op");
        };
        assert!(matches!(op.op, LocalOp::DeleteLabel(id) if id == label_id()));
    }

    #[test]
    fn delete_unbound_label_cancels_its_ops() {
        let (mut app, _outbox) = base();
        let unbound = label(501, false);
        let unbound_id = unbound.id;
        app.labels.insert(unbound_id, unbound);
        let outbox = vec![
            op_for(31, LocalOp::UpdateLabel(unbound_id)),
            op_for(32, LocalOp::UpdateLabel(label_id())),
        ];
        let plan = run(&app, &outbox, &StateCommand::DeleteLabel { id: unbound_id }).unwrap();
        let failed: Vec<OpId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::FailOp(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(failed, vec![OpId(uuid::Uuid::from_u128(31))]);
    }

    #[test]
    fn empty_label_changeset_is_a_no_op() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateLabel {
                id: label_id(),
                changes: LabelChanges::default(),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn label_edit_against_tombstone_is_rejected_but_delete_repeat_is_not() {
        let (mut app, outbox) = base();
        let mut dead = label(502, true);
        dead.deleted = true;
        let dead_id = dead.id;
        app.labels.insert(dead_id, dead);
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::UpdateLabel {
                    id: dead_id,
                    changes: LabelChanges {
                        title: Some("x".into()),
                        color: None,
                    },
                }
            ),
            Err(CommandError::LabelDeleted(dead_id))
        );
        let plan = run(&app, &outbox, &StateCommand::DeleteLabel { id: dead_id }).unwrap();
        assert_eq!(plan, no_op());
    }

    // ---- Assign / Unassign ---------------------------------------------

    #[test]
    fn assign_and_unassign_stamp_the_whole_set_clock() {
        let (mut app, outbox) = base();
        let fresh = label(503, true);
        let fresh_id = fresh.id;
        app.labels.insert(fresh_id, fresh);
        app.tasks
            .get_mut(&task_id())
            .unwrap()
            .labels
            .remove(&fresh_id);

        let plan = run(
            &app,
            &outbox,
            &StateCommand::AssignLabel {
                task: task_id(),
                label: fresh_id,
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert!(updated.labels.contains(&fresh_id));
        assert_eq!(updated.clocks.labels, now());
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::AssignLabel(t, l) if t == task_id() && l == fresh_id));

        // Repeat is a no-op.
        let plan = run(
            &app,
            &outbox,
            &StateCommand::AssignLabel {
                task: task_id(),
                label: label_id(),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn unassign_not_assigned_is_a_no_op() {
        let (app, outbox) = base();
        let stranger = LabelId::from(uuid::Uuid::from_u128(999));
        // The label must still exist (live) to exercise the set guard, not
        // the existence guard.
        let mut stranger_label = label(999, true);
        stranger_label.id = stranger;
        let mut state = app.clone();
        state.labels.insert(stranger, stranger_label);
        let plan = run(
            &state,
            &outbox,
            &StateCommand::UnassignLabel {
                task: task_id(),
                label: stranger,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());

        let plan = run(
            &app,
            &outbox,
            &StateCommand::UnassignLabel {
                task: task_id(),
                label: label_id(),
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert!(!updated.labels.contains(&label_id()));
        assert_eq!(updated.clocks.labels, now());
    }

    // ---- Boards --------------------------------------------------------

    fn unbound_board_state() -> AppState {
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let mut app = AppState {
            boards: [(
                board_id,
                crate::entities::Board {
                    id: board_id,
                    remote: None,
                    title: "local".into(),
                    color: crate::entities::Color::new("00ccff"),
                    archived: false,
                    deleted: false,
                    clocks: crate::entities::BoardClocks {
                        title: crate::entities::BoardClocks::epoch(),
                        color: crate::entities::BoardClocks::epoch(),
                        archived: crate::entities::BoardClocks::epoch(),
                        deleted: crate::entities::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            )]
            .into_iter()
            .collect(),
            stacks: std::collections::BTreeMap::new(),
            tasks: std::collections::BTreeMap::new(),
            labels: std::collections::BTreeMap::new(),
            reminders: crate::reminders::ReminderState::default(),
            sync: crate::state::SyncStates::new(),
            last_updated: None,
        };
        // Give the board content so bind tests exercise a non-empty board.
        let stack = stack(901, false);
        app.stacks.insert(stack.id, stack);
        app
    }

    #[test]
    fn create_board_mints_a_local_board_without_an_outbox_op() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &empty,
            &outbox,
            &StateCommand::CreateBoard {
                title: "Personal".into(),
                color: crate::entities::Color::new("3366cc"),
            },
        )
        .unwrap();
        let CommandOutcome::CreatedBoard(id) = plan.outcome else {
            panic!("wrong outcome");
        };
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::UpsertBoard(crate::entities::Board {
                id,
                remote: None,
                title: "Personal".into(),
                color: crate::entities::Color::new("3366cc"),
                archived: false,
                deleted: false,
                clocks: crate::entities::BoardClocks {
                    title: now(),
                    color: now(),
                    archived: now(),
                    deleted: now(),
                },
                remote_seen: None,
            })],
            "exactly the board upsert; no outbox op"
        );
    }

    #[test]
    fn create_board_refuses_when_a_live_board_exists() {
        let (app, outbox) = base();
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::CreateBoard {
                    title: "second".into(),
                    color: crate::entities::Color::new("000000"),
                }
            ),
            Err(CommandError::BoardExists)
        );
    }

    #[test]
    fn bind_board_sets_the_remote_binding_without_an_outbox_op() {
        let app = unbound_board_state();
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::BindBoard {
                remote: crate::ids::RemoteBoardId(42),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert_eq!(updated.remote, Some(crate::ids::RemoteBoardId(42)));
        assert_eq!(plan.actions.len(), 1, "bind carries no outbox op");
    }

    #[test]
    fn bind_board_without_a_live_board_is_rejected() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        assert_eq!(
            run(
                &empty,
                &outbox,
                &StateCommand::BindBoard {
                    remote: crate::ids::RemoteBoardId(1)
                }
            ),
            Err(CommandError::NoBoard)
        );
    }

    #[test]
    fn bind_repeat_to_the_same_remote_is_a_no_op_but_a_new_remote_is_rejected() {
        let outbox: Vec<PendingOp> = Vec::new();
        let bound = {
            let mut app = unbound_board_state();
            let board_id = live_board(&app).unwrap();
            app.boards.get_mut(&board_id).unwrap().remote = Some(crate::ids::RemoteBoardId(42));
            app
        };
        let plan = run(
            &bound,
            &outbox,
            &StateCommand::BindBoard {
                remote: crate::ids::RemoteBoardId(42),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
        assert_eq!(
            run(
                &bound,
                &outbox,
                &StateCommand::BindBoard {
                    remote: crate::ids::RemoteBoardId(43)
                }
            ),
            Err(CommandError::BoardAlreadyBound(crate::ids::RemoteBoardId(
                42
            )))
        );
    }

    // ---- Board color / archive / delete / reset -------------------------

    #[test]
    fn set_board_color_stamps_only_the_color_clock_and_enqueues() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardColor {
                color: crate::entities::Color::new("00ff00"),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert_eq!(updated.color.as_str(), "00ff00");
        assert_eq!(updated.clocks.color, now());
        assert_eq!(
            updated.clocks.title,
            crate::entities::BoardClocks::epoch(),
            "a color edit must not clobber the title clock"
        );
        assert_eq!(
            updated.clocks.archived,
            crate::entities::BoardClocks::epoch(),
            "the archived clock stands"
        );
        let PersistenceAction::EnqueueOp(pending) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert_eq!(
            pending.op,
            LocalOp::UpdateBoard(BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD))))
        );
    }

    #[test]
    fn set_board_color_same_color_is_a_no_op() {
        let (app, outbox) = base();
        let color = app.boards.values().next().unwrap().color.clone();
        let plan = run(&app, &outbox, &StateCommand::SetBoardColor { color }).unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn set_board_color_without_a_live_board_is_no_board() {
        let (mut app, outbox) = base();
        app.boards.clear();
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::SetBoardColor {
                    color: crate::entities::Color::new("00ff00"),
                }
            ),
            Err(CommandError::NoBoard)
        );

        // A tombstoned board is not commandable either (the live-board
        // guard filters deleted only — the edit-against-tombstone class).
        let mut dead = base().0;
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let mut board = dead.boards[&board_id].clone();
        board.deleted = true;
        board.clocks.deleted = ts(T0 - 60);
        dead.boards.insert(board_id, board);
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::SetBoardColor {
                    color: crate::entities::Color::new("00ff00"),
                }
            ),
            Err(CommandError::NoBoard)
        );
    }

    #[test]
    fn set_board_archived_stamps_the_archived_clock_both_directions() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardArchived { archived: true },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert!(updated.archived);
        assert_eq!(updated.clocks.archived, now());
        assert_eq!(
            updated.clocks.title,
            crate::entities::BoardClocks::epoch(),
            "title clock untouched"
        );

        // Unarchive on an archived board (archived boards stay
        // commandable — `live_board` filters `deleted` only).
        let mut archived = app;
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let mut board = archived.boards[&board_id].clone();
        board.archived = true;
        board.clocks.archived = ts(T0 - 60);
        archived.boards.insert(board_id, board);
        let plan = run(
            &archived,
            &outbox,
            &StateCommand::SetBoardArchived { archived: false },
        )
        .unwrap();
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert!(!updated.archived);
        assert_eq!(updated.clocks.archived, now());

        // A redundant repeat is an accepted no-op.
        assert_eq!(
            run(
                &archived,
                &outbox,
                &StateCommand::SetBoardArchived { archived: true }
            ),
            Ok(no_op())
        );
    }

    #[test]
    fn an_archived_board_stays_commandable() {
        let (mut app, outbox) = base();
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let mut board = app.boards[&board_id].clone();
        board.archived = true;
        board.clocks.archived = ts(T0 - 60);
        app.boards.insert(board_id, board);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardColor {
                color: crate::entities::Color::new("123456"),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
    }

    #[test]
    fn delete_board_cascades_tombstones_and_cancels_pending_ops() {
        let (app, _outbox) = base();
        let second = stack(901, true);
        let second_id = second.id;
        let mut state = app;
        state.stacks.insert(second_id, second);
        let other = task(2, second_id, true);
        let other_id = other.id;
        state.tasks.insert(other_id, other);
        let own_rule = reminder_rule(66, task_id());
        state.reminders.rules.insert(own_rule.id, own_rule.clone());
        let outbox = vec![
            op_for(41, LocalOp::UpdateTask(task_id())),
            op_for(42, LocalOp::CreateStack(second_id)),
            op_for(
                43,
                LocalOp::UpdateBoard(BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)))),
            ),
        ];

        let plan = run(&state, &outbox, &StateCommand::DeleteBoard).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);

        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let PersistenceAction::UpsertBoard(board_upsert) = &plan.actions[0] else {
            panic!("first action must be the board tombstone");
        };
        assert!(board_upsert.deleted);
        assert_eq!(board_upsert.clocks.deleted, now());
        assert!(
            board_upsert.remote.is_some(),
            "binding kept until the push (R9c)"
        );

        let tombstoned_stacks: Vec<&Stack> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::UpsertStack(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(tombstoned_stacks.len(), 2, "both live stacks cascade");
        assert!(
            tombstoned_stacks
                .iter()
                .all(|s| s.deleted && s.clocks.deleted == now())
        );

        let tombstoned_tasks: Vec<&Task> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::UpsertTask(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(tombstoned_tasks.len(), 2, "both live tasks cascade");
        assert!(tombstoned_tasks.iter().all(|t| t.deleted));

        let rule_deletes: Vec<_> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::DeleteReminderRule(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(
            rule_deletes,
            vec![own_rule.id],
            "the D-c cascade fires per task"
        );

        let failed: Vec<OpId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::FailOp(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(
            failed,
            vec![
                OpId(uuid::Uuid::from_u128(41)),
                OpId(uuid::Uuid::from_u128(42)),
                OpId(uuid::Uuid::from_u128(43)),
            ],
            "every queued op of the board or its children is cancelled (no 404 chase)"
        );

        let enqueued: Vec<&LocalOp> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::EnqueueOp(op) => Some(&op.op),
                _ => None,
            })
            .collect();
        assert_eq!(
            enqueued,
            vec![&LocalOp::DeleteBoard(board_id)],
            "exactly one wire intent"
        );
    }

    #[test]
    fn delete_board_repeat_is_a_no_op_and_absence_is_no_board() {
        let (mut app, outbox) = base();
        let plan = run(&app, &outbox, &StateCommand::DeleteBoard).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);

        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let mut board = app.boards[&board_id].clone();
        board.deleted = true;
        board.clocks.deleted = ts(T0 - 30);
        app.boards.insert(board_id, board);
        assert_eq!(run(&app, &outbox, &StateCommand::DeleteBoard), Ok(no_op()));

        let mut empty = app;
        empty.boards.clear();
        assert_eq!(
            run(&empty, &outbox, &StateCommand::DeleteBoard),
            Err(CommandError::NoBoard)
        );
    }

    #[test]
    fn delete_board_on_an_unbound_board_still_queues_the_op() {
        let app = unbound_board_state();
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let plan = run(&app, &[], &StateCommand::DeleteBoard).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert!(
            plan.actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::EnqueueOp(op) if op.op == LocalOp::DeleteBoard(board_id))),
            "uniform wire intent; the planner noop-completes an unbound group"
        );
    }

    #[test]
    fn reset_board_is_one_bulk_action_without_a_board_upsert() {
        let (app, _outbox) = base();
        let mut state = app;
        let own_rule = reminder_rule(67, task_id());
        state.reminders.rules.insert(own_rule.id, own_rule.clone());
        state.reminders.muted.insert(task_id());
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let outbox = vec![
            op_for(51, LocalOp::UpdateTask(task_id())),
            op_for(52, LocalOp::UpdateBoard(board_id)),
        ];
        let plan = run(&state, &outbox, &StateCommand::ResetBoard).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::ClearBoardContent(board_id)],
            "exactly the bulk action; the board row (title, color, binding,
             clocks, remote_seen) is untouched by design"
        );
    }

    #[test]
    fn reset_board_on_an_already_empty_board_is_a_no_op() {
        // A live board with zero content, zero board-scoped ops, and zero
        // reminder facts: nothing to repair.
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        let app = AppState {
            boards: [(
                board_id,
                crate::entities::Board {
                    id: board_id,
                    remote: Some(crate::ids::RemoteBoardId(77)),
                    title: "board".into(),
                    color: crate::entities::Color::new("0000ff"),
                    archived: false,
                    deleted: false,
                    clocks: crate::entities::BoardClocks {
                        title: crate::entities::BoardClocks::epoch(),
                        color: crate::entities::BoardClocks::epoch(),
                        archived: crate::entities::BoardClocks::epoch(),
                        deleted: crate::entities::BoardClocks::epoch(),
                    },
                    remote_seen: None,
                },
            )]
            .into_iter()
            .collect(),
            stacks: std::collections::BTreeMap::new(),
            tasks: std::collections::BTreeMap::new(),
            labels: std::collections::BTreeMap::new(),
            reminders: crate::reminders::ReminderState::default(),
            sync: crate::state::SyncStates::new(),
            last_updated: None,
        };
        assert_eq!(run(&app, &[], &StateCommand::ResetBoard), Ok(no_op()));
    }

    #[test]
    fn reset_board_without_a_live_board_is_no_board() {
        let (mut app, outbox) = base();
        app.boards.clear();
        assert_eq!(
            run(&app, &outbox, &StateCommand::ResetBoard),
            Err(CommandError::NoBoard)
        );
    }

    // ---- RequestSync ----------------------------------------------------

    #[test]
    fn request_sync_is_an_action_free_sync_requested() {
        let (app, outbox) = base();
        let plan = run(&app, &outbox, &StateCommand::RequestSync).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::SyncRequested);
        assert!(plan.actions.is_empty(), "no-op must carry zero actions");
    }

    // ---- SnoozeReminder / SetTaskRemindersMuted ------------------------

    fn reminder_rule(raw: u64, task: TaskId) -> crate::reminders::ReminderRule {
        crate::reminders::ReminderRule {
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(u128::from(raw))),
            task,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: now(),
        }
    }

    fn with_rules(app: &mut AppState, rules: &[crate::reminders::ReminderRule]) {
        for rule in rules {
            app.reminders.rules.insert(rule.id, rule.clone());
        }
    }

    fn rule_id(raw: u64) -> crate::ids::ReminderRuleId {
        crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(u128::from(raw)))
    }

    #[test]
    fn snooze_plans_the_engine_stamped_link() {
        let (mut app, outbox) = base();
        with_rules(&mut app, &[reminder_rule(60, task_id())]);
        let rule = rule_id(60);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SnoozeReminder {
                reminder: crate::reminders::ReminderRef::Rule(rule),
                occurrence: ts(T0 - 60),
                postpone: chrono::TimeDelta::try_minutes(10).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::RecordReminderSnooze(
                crate::reminders::ReminderSnooze {
                    reminder: crate::reminders::ReminderRef::Rule(rule),
                    occurrence: ts(T0 - 60),
                    rescheduled_to: ts(T0 + 600),
                }
            )],
            "exactly one engine-stamped snooze link"
        );
    }

    #[test]
    fn snooze_rejections_are_typed() {
        let (mut app, outbox) = base();
        with_rules(&mut app, &[reminder_rule(60, task_id())]);
        let unknown_rule = rule_id(4242);
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::SnoozeReminder {
                    reminder: crate::reminders::ReminderRef::Rule(unknown_rule),
                    occurrence: ts(T0),
                    postpone: chrono::TimeDelta::try_minutes(1).unwrap(),
                }
            ),
            Err(CommandError::UnknownReminderRule(unknown_rule))
        );

        let stranger = TaskId::from(uuid::Uuid::from_u128(4243));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::SnoozeReminder {
                    reminder: crate::reminders::ReminderRef::Default(stranger),
                    occurrence: ts(T0),
                    postpone: chrono::TimeDelta::try_minutes(1).unwrap(),
                }
            ),
            Err(CommandError::UnknownTask(stranger))
        );

        let mut dead_state = app.clone();
        let mut dead = task(2, stack_id(), true);
        dead.deleted = true;
        let dead_id = dead.id;
        dead_state.tasks.insert(dead_id, dead);
        assert_eq!(
            run(
                &dead_state,
                &outbox,
                &StateCommand::SnoozeReminder {
                    reminder: crate::reminders::ReminderRef::Default(dead_id),
                    occurrence: ts(T0),
                    postpone: chrono::TimeDelta::try_minutes(1).unwrap(),
                }
            ),
            Err(CommandError::TaskDeleted(dead_id))
        );

        for secs in [0_i64, -30] {
            let result = run(
                &app,
                &outbox,
                &StateCommand::SnoozeReminder {
                    reminder: crate::reminders::ReminderRef::Default(task_id()),
                    occurrence: ts(T0),
                    postpone: chrono::TimeDelta::try_seconds(secs).unwrap(),
                },
            );
            assert!(
                matches!(result, Err(CommandError::InvalidSnoozePostpone)),
                "postpone {secs} s must be rejected as non-positive"
            );
        }
    }

    #[test]
    fn snooze_saturates_an_unrepresentable_target() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SnoozeReminder {
                reminder: crate::reminders::ReminderRef::Default(task_id()),
                occurrence: ts(T0),
                postpone: chrono::TimeDelta::MAX,
            },
        )
        .unwrap();
        let PersistenceAction::RecordReminderSnooze(link) = &plan.actions[0] else {
            panic!("the action must be the snooze link");
        };
        assert_eq!(link.rescheduled_to, DateTime::<Utc>::MAX_UTC);
    }

    #[test]
    fn mute_toggles_and_repeats_are_no_ops() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskRemindersMuted {
                task: task_id(),
                muted: true,
            },
        )
        .unwrap();
        assert_eq!(
            plan.outcome,
            CommandOutcome::ReminderMuted {
                task: task_id(),
                muted: true
            }
        );
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::SetTaskRemindersMuted(task_id(), true)]
        );

        // Repeat on a muted task: accepted no-op.
        let mut muted_state = app.clone();
        muted_state.reminders.muted.insert(task_id());
        assert_eq!(
            run(
                &muted_state,
                &outbox,
                &StateCommand::SetTaskRemindersMuted {
                    task: task_id(),
                    muted: true,
                }
            ),
            Ok(no_op())
        );
        // Unset mutes off again.
        let plan = run(
            &muted_state,
            &outbox,
            &StateCommand::SetTaskRemindersMuted {
                task: task_id(),
                muted: false,
            },
        )
        .unwrap();
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::SetTaskRemindersMuted(task_id(), false)]
        );

        let stranger = TaskId::from(uuid::Uuid::from_u128(4244));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::SetTaskRemindersMuted {
                    task: stranger,
                    muted: true,
                }
            ),
            Err(CommandError::UnknownTask(stranger))
        );
    }

    #[test]
    fn delete_task_prunes_its_reminder_rules() {
        let (mut app, outbox) = base();
        let own_rule = reminder_rule(61, task_id());
        let second_own_rule = reminder_rule(62, task_id());
        let stranger_task = TaskId::from(uuid::Uuid::from_u128(4245));
        let stranger_rule = reminder_rule(63, stranger_task);
        with_rules(
            &mut app,
            &[own_rule.clone(), second_own_rule.clone(), stranger_rule],
        );
        // The stranger must exist as a live task so it is unaffected
        // context, not an error path.
        app.tasks.insert(stranger_task, {
            let mut t = task(9, stack_id(), false);
            t.id = stranger_task;
            t
        });

        let plan = run(&app, &outbox, &StateCommand::DeleteTask { id: task_id() }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let rule_deletes: Vec<&crate::ids::ReminderRuleId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::DeleteReminderRule(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(
            rule_deletes,
            vec![&own_rule.id, &second_own_rule.id],
            "exactly the deleted task's rules are pruned, in rule-id order"
        );

        // A delete repeat stays a zero-action no-op (the rules are already
        // gone with the tombstone short-circuit).
        let mut deleted_state = app;
        let mut dead = task(TASK, stack_id(), true);
        dead.deleted = true;
        deleted_state.tasks.insert(dead.id, dead);
        assert_eq!(
            run(
                &deleted_state,
                &outbox,
                &StateCommand::DeleteTask { id: task_id() }
            ),
            Ok(no_op())
        );
    }

    #[test]
    fn delete_stack_cascades_reminder_rules_of_its_tasks_only() {
        let (mut app, outbox) = base();
        let cascaded_rule = reminder_rule(64, task_id());
        with_rules(&mut app, std::slice::from_ref(&cascaded_rule));

        // A second stack holds the untouched control task + rule.
        let other_stack = stack(905, true);
        let other_stack_id = other_stack.id;
        app.stacks.insert(other_stack_id, other_stack);
        let control = task(8, other_stack_id, false);
        let control_id = control.id;
        app.tasks.insert(control_id, control);
        let control_rule = reminder_rule(65, control_id);
        app.reminders
            .rules
            .insert(control_rule.id, control_rule.clone());

        let plan = run(&app, &outbox, &StateCommand::DeleteStack { id: stack_id() }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let rule_deletes: Vec<crate::ids::ReminderRuleId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::DeleteReminderRule(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(
            rule_deletes,
            vec![cascaded_rule.id],
            "the cascaded task's rule is pruned; the other stack's rule is untouched"
        );
    }

    // ---- SetReminderRule / ClearReminderRules --------------------------

    fn offset_before(secs: i64) -> crate::reminders::ReminderOffset {
        crate::reminders::ReminderOffset::before(
            chrono::TimeDelta::try_seconds(-secs).expect("valid offset"),
        )
        .expect("negative duration is a legal offset")
    }

    #[test]
    fn set_rule_plans_a_new_rule_for_a_live_task() {
        let (app, outbox) = base();
        let offset = offset_before(600);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetReminderRule {
                task: task_id(),
                offset,
            },
        )
        .unwrap();
        let CommandOutcome::ReminderSet { rule } = plan.outcome else {
            panic!("expected ReminderSet, got {:?}", plan.outcome);
        };
        let [PersistenceAction::UpsertReminderRule(stored)] = &plan.actions[..] else {
            panic!(
                "expected exactly one UpsertReminderRule, got {:?}",
                plan.actions
            );
        };
        assert_eq!(stored.id, rule);
        assert_eq!(stored.task, task_id());
        assert_eq!(stored.offset, offset);
        assert_eq!(
            stored.created_at,
            now(),
            "the rule stamp is the injected now"
        );
    }

    #[test]
    fn set_rule_replaces_the_tasks_existing_rules() {
        let (mut app, outbox) = base();
        let own1 = reminder_rule(61, task_id());
        let own2 = reminder_rule(62, task_id());
        let stranger = TaskId::from(uuid::Uuid::from_u128(4245));
        let foreign = reminder_rule(63, stranger);
        with_rules(&mut app, &[own1.clone(), own2.clone(), foreign.clone()]);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetReminderRule {
                task: task_id(),
                offset: offset_before(300),
            },
        )
        .unwrap();
        let mut deletes: Vec<crate::ids::ReminderRuleId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::DeleteReminderRule(id) => Some(*id),
                _ => None,
            })
            .collect();
        deletes.sort();
        assert_eq!(
            deletes,
            vec![own1.id, own2.id],
            "exactly the task's own rules are deleted, foreign rules untouched"
        );
        assert!(matches!(
            plan.actions.last(),
            Some(PersistenceAction::UpsertReminderRule(_))
        ));
    }

    #[test]
    fn set_rule_rejects_unknown_and_deleted_tasks() {
        let (app, outbox) = base();
        let stranger = TaskId::from(uuid::Uuid::from_u128(4245));
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::SetReminderRule {
                    task: stranger,
                    offset: offset_before(60)
                }
            ),
            Err(CommandError::UnknownTask(_))
        ));
        let mut dead = app;
        let mut tombstone = task(TASK, stack_id(), true);
        tombstone.deleted = true;
        dead.tasks.insert(tombstone.id, tombstone);
        assert!(matches!(
            run(
                &dead,
                &outbox,
                &StateCommand::SetReminderRule {
                    task: task_id(),
                    offset: offset_before(60)
                }
            ),
            Err(CommandError::TaskDeleted(_))
        ));
    }

    #[test]
    fn set_rule_on_a_task_without_a_duedate_is_legal() {
        // The base fixture task carries no duedate: the rule arms when a
        // duedate arrives (due-anchored occurrence arithmetic).
        let (app, outbox) = base();
        assert!(
            app.tasks
                .get(&task_id())
                .expect("fixture")
                .duedate
                .is_none()
        );
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetReminderRule {
                task: task_id(),
                offset: offset_before(60),
            },
        )
        .unwrap();
        assert!(matches!(plan.outcome, CommandOutcome::ReminderSet { .. }));
    }

    #[test]
    fn clear_rules_plans_deletes_and_reports_the_count() {
        let (mut app, outbox) = base();
        let own1 = reminder_rule(61, task_id());
        let own2 = reminder_rule(62, task_id());
        let stranger = TaskId::from(uuid::Uuid::from_u128(4245));
        let foreign = reminder_rule(63, stranger);
        with_rules(&mut app, &[own1.clone(), own2.clone(), foreign.clone()]);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::ClearReminderRules { task: task_id() },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::ReminderCleared { removed: 2 });
        let deletes: Vec<crate::ids::ReminderRuleId> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::DeleteReminderRule(id) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(deletes, vec![own1.id, own2.id]);
        assert!(app.reminders.rules.contains_key(&foreign.id));
    }

    #[test]
    fn clear_rules_with_no_rules_is_a_no_op() {
        let (app, outbox) = base();
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::ClearReminderRules { task: task_id() }
            ),
            Ok(no_op())
        );
    }

    #[test]
    fn clear_rules_rejects_dead_tasks() {
        let (mut app, outbox) = base();
        let mut tombstone = task(TASK, stack_id(), true);
        tombstone.deleted = true;
        app.tasks.insert(tombstone.id, tombstone);
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::ClearReminderRules { task: task_id() }
            ),
            Err(CommandError::TaskDeleted(_))
        ));
    }

    #[test]
    fn mute_outcome_carries_the_direction() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskRemindersMuted {
                task: task_id(),
                muted: true,
            },
        )
        .unwrap();
        assert_eq!(
            plan.outcome,
            CommandOutcome::ReminderMuted {
                task: task_id(),
                muted: true
            }
        );
        // The redundant repeat is still the accepted no-op — only the
        // success outcome's shape changed.
        let mut muted_state = app.clone();
        muted_state.reminders.muted.insert(task_id());
        assert_eq!(
            run(
                &muted_state,
                &outbox,
                &StateCommand::SetTaskRemindersMuted {
                    task: task_id(),
                    muted: true
                }
            ),
            Ok(no_op())
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::test_support::{
        CountingIds, app_state_strategy, persisted_state_strategy, proptest_config,
        state_command_strategy,
    };
    use chrono::TimeZone;
    use proptest::prelude::*;

    /// The entity an op addresses, if it is still in the post-batch maps.
    fn op_target_exists(
        op: &LocalOp,
        boards: &std::collections::BTreeMap<BoardId, crate::entities::Board>,
        stacks: &std::collections::BTreeMap<StackId, Stack>,
        tasks: &std::collections::BTreeMap<TaskId, Task>,
        labels: &std::collections::BTreeMap<LabelId, Label>,
    ) -> bool {
        match op {
            LocalOp::CreateTask(id)
            | LocalOp::UpdateTask(id)
            | LocalOp::MoveTask(id)
            | LocalOp::DeleteTask(id) => tasks.contains_key(id),
            LocalOp::CreateStack(id)
            | LocalOp::RenameStack(id)
            | LocalOp::MoveStack(id)
            | LocalOp::DeleteStack(id) => stacks.contains_key(id),
            LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                labels.contains_key(id)
            }
            LocalOp::AssignLabel(task, label) | LocalOp::UnassignLabel(task, label) => {
                tasks.contains_key(task) && labels.contains_key(label)
            }
            LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => boards.contains_key(id),
        }
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn plan_command_is_total_and_ok_plans_are_self_consistent(
            persisted in persisted_state_strategy(),
            command in state_command_strategy(),
        ) {
            let app: AppState = AppState {
                boards: persisted.boards.clone(),
                stacks: persisted.stacks.clone(),
                tasks: persisted.tasks.clone(),
                labels: persisted.labels.clone(),
                reminders: persisted.reminders.clone(),
                sync: persisted.sync,
                last_updated: None,
            };
            let outbox = persisted.outbox.clone();
            let result = plan_command(&app, &outbox, &command, &CountingIds::default(),
                chrono::Utc.timestamp_opt(36_000, 0).unwrap());

            // Totality: never a panic — always a plan or a typed error.
            let Ok(plan) = result else {
                return Ok(());
            };

            // No-op and rejection outcomes carry no actions.
            if plan.outcome == CommandOutcome::NoOp {
                prop_assert!(plan.actions.is_empty(), "no-op must carry zero actions");
            }

            // Self-consistency: every enqueued op's target exists in the
            // post-batch state.
            let mut post = app;
            for action in &plan.actions {
                match action {
                    PersistenceAction::UpsertBoard(b) => { post.boards.insert(b.id, b.clone()); }
                    PersistenceAction::UpsertStack(s) => { post.stacks.insert(s.id, s.clone()); }
                    PersistenceAction::UpsertTask(t) => { post.tasks.insert(t.id, t.clone()); }
                    PersistenceAction::UpsertLabel(l) => { post.labels.insert(l.id, l.clone()); }
                    _ => {}
                }
            }
            for action in &plan.actions {
                if let PersistenceAction::EnqueueOp(pending) = action {
                    prop_assert!(
                        op_target_exists(&pending.op, &post.boards, &post.stacks, &post.tasks, &post.labels),
                        "enqueued op {:?} must target an existing entity",
                        pending.op
                    );
                }
            }
        }

        #[test]
        fn plan_command_outcome_is_independent_of_the_outbox(
            state in app_state_strategy(),
            outbox_state in persisted_state_strategy(),
            command in state_command_strategy(),
        ) {
            // The outbox never influences the outcome: identical
            // rejections and identical no-ops for any queue contents; an
            // accepted plan differs only in the FailOps the delete paths
            // derive from the outbox. Fresh `CountingIds` on both sides
            // keep the fresh ids (entity and op) comparable.
            let empty: Vec<PendingOp> = Vec::new();
            let now = chrono::Utc.timestamp_opt(36_000, 0).unwrap();
            let baseline = plan_command(&state, &empty, &command, &CountingIds::default(), now);
            let variant = plan_command(
                &state,
                &outbox_state.outbox,
                &command,
                &CountingIds::default(),
                now,
            );

            match (&baseline, &variant) {
                (Err(a), Err(b)) => prop_assert_eq!(a, b),
                (Ok(p), Ok(q)) if p.outcome == CommandOutcome::NoOp => prop_assert_eq!(p, q),
                (Ok(p), Ok(q)) => {
                    let without_fails = |plan: &CommandPlan| {
                        plan.actions
                            .iter()
                            .filter(|action| !matches!(action, PersistenceAction::FailOp(_)))
                            .cloned()
                            .collect::<Vec<_>>()
                    };
                    prop_assert_eq!(&p.outcome, &q.outcome);
                    prop_assert_eq!(without_fails(p), without_fails(q));
                }
                (Err(_), Ok(_)) | (Ok(_), Err(_)) => {
                    prop_assert!(
                        false,
                        "acceptance must not depend on the outbox: {baseline:?} vs {variant:?}"
                    );
                }
            }
        }
    }
}
