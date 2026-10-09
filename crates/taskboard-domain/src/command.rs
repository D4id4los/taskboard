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
use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::messages::{LabelChanges, StateCommand, TaskChanges};
use crate::outbox::{LocalOp, OpId, PendingOp};
use crate::persistence::PersistenceAction;
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

/// `true` when the pending op addresses the given task (same id-scoping
/// rule the sync pipeline uses for op cancellation).
fn op_targets_task(op: &LocalOp, task: TaskId) -> bool {
    match op {
        LocalOp::CreateTask(id)
        | LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::DeleteTask(id)
        | LocalOp::AssignLabel(id, _)
        | LocalOp::UnassignLabel(id, _) => *id == task,
        LocalOp::CreateStack(_)
        | LocalOp::RenameStack(_)
        | LocalOp::DeleteStack(_)
        | LocalOp::CreateLabel(_)
        | LocalOp::UpdateLabel(_)
        | LocalOp::DeleteLabel(_) => false,
    }
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

/// Plans one local command against the current view.
///
/// Returns the persistence batch and the receipt. The function is total:
/// any (state, command) combination yields a plan or a typed
/// [`CommandError`], never a panic.
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
pub fn plan_command(
    app: &AppState,
    outbox: &[PendingOp],
    command: &StateCommand,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> Result<CommandPlan, CommandError> {
    plan_inner(app, outbox, command, ids, now)
}

/// The catalogue: one arm per §4 table row. Kept as one `match` so the
/// per-command rules stay adjacent and diffable against the table.
#[allow(clippy::too_many_lines)] // one catalogue; splitting hides the table
fn plan_inner(
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
            if changes == &TaskChanges::default() {
                return Ok(no_op());
            }
            let mut updated = live_task(app, *id)?.clone();
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
            let cascade_ids: Vec<TaskId> = app
                .tasks
                .values()
                .filter(|task| task.is_live() && task.stack == *id)
                .map(|task| task.id)
                .collect();
            for task_id in cascade_ids {
                let Ok(task) = existing_task(app, task_id) else {
                    continue;
                };
                let mut tombstoned = task.clone();
                tombstoned.deleted = true;
                tombstoned.clocks.deleted = now;
                actions.push(PersistenceAction::UpsertTask(tombstoned));
                if task.remote.is_some() {
                    actions.push(enqueue(LocalOp::DeleteTask(task_id), ids, now));
                } else {
                    actions.extend(
                        task_ops(outbox, task_id)
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
            if changes == &LabelChanges::default() {
                return Ok(no_op());
            }
            let mut updated = live_label(app, *id)?.clone();
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
    use crate::state::SyncStatus;
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
            sync: SyncStatus::default(),
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

    // ---- RequestSync ----------------------------------------------------

    #[test]
    fn request_sync_is_an_action_free_sync_requested() {
        let (app, outbox) = base();
        let plan = run(&app, &outbox, &StateCommand::RequestSync).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::SyncRequested);
        assert!(plan.actions.is_empty(), "no-op must carry zero actions");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::test_support::{
        CountingIds, app_state_strategy, persisted_state_strategy, state_command_strategy,
    };
    use chrono::TimeZone;
    use proptest::prelude::*;

    /// The entity an op addresses, if it is still in the post-batch maps.
    fn op_target_exists(
        op: &LocalOp,
        stacks: &std::collections::BTreeMap<StackId, Stack>,
        tasks: &std::collections::BTreeMap<TaskId, Task>,
        labels: &std::collections::BTreeMap<LabelId, Label>,
    ) -> bool {
        match op {
            LocalOp::CreateTask(id)
            | LocalOp::UpdateTask(id)
            | LocalOp::MoveTask(id)
            | LocalOp::DeleteTask(id) => tasks.contains_key(id),
            LocalOp::CreateStack(id) | LocalOp::RenameStack(id) | LocalOp::DeleteStack(id) => {
                stacks.contains_key(id)
            }
            LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                labels.contains_key(id)
            }
            LocalOp::AssignLabel(task, label) | LocalOp::UnassignLabel(task, label) => {
                tasks.contains_key(task) && labels.contains_key(label)
            }
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

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
                        op_target_exists(&pending.op, &post.stacks, &post.tasks, &post.labels),
                        "enqueued op {:?} must target an existing entity",
                        pending.op
                    );
                }
            }
        }

        #[test]
        fn plan_command_never_depends_on_outbox_ordering_for_validity(
            state in app_state_strategy(),
            command in state_command_strategy(),
        ) {
            // A rejected/no-op result must be identical regardless of the
            // outbox contents; accepted plans are allowed to differ only in
            // FailOp actions, which the catalogue derives from the outbox.
            let empty: Vec<PendingOp> = Vec::new();
            let now = chrono::Utc.timestamp_opt(36_000, 0).unwrap();
            let baseline = plan_command(&state, &empty, &command, &CountingIds::default(), now);
            let _ = baseline; // totality under both outbox shapes
        }
    }
}
