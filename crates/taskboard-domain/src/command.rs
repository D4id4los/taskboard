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

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::capabilities::DeckFeatures;
use crate::entities::{Label, Stack, Task, TaskClocks};
use crate::idgen::IdGenerator;
use crate::ids::{BoardId, LabelId, ReminderRuleId, RemoteBoardId, StackId, TaskId};
use crate::messages::{LabelChanges, StateCommand, TaskChanges};
use crate::outbox::{LocalOp, OpId, PendingOp, op_targets_task};
use crate::persistence::PersistenceAction;
use crate::reminders::{
    ReminderDefaults, ReminderDismissal, ReminderRef, ReminderSnooze, active_notifications,
};
use crate::state::AppState;
use crate::tracking::{ClosedSpan, TimeEntry};

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
    /// No board with this id exists.
    #[error("unknown board")]
    UnknownBoard(BoardId),
    /// The board is tombstoned; edit-style intents cannot target it.
    #[error("board is deleted")]
    BoardDeleted(BoardId),
    /// The named board is already bound to a remote board (differing
    /// from the bind target).
    #[error("the board is already bound to a remote board")]
    BoardAlreadyBound {
        /// The named board.
        board: BoardId,
        /// The remote board it is bound to.
        bound: RemoteBoardId,
    },
    /// The remote board is already bound to a (named) local board: one
    /// remote board binds to at most one local board.
    #[error("the remote board is already bound to another local board")]
    RemoteBoardInUse {
        /// The contested remote board.
        remote: RemoteBoardId,
        /// The local board that holds the binding.
        board: BoardId,
    },
    /// No reminder rule with this id exists.
    #[error("unknown reminder rule")]
    UnknownReminderRule(ReminderRuleId),
    /// No adhoc alarm with this id exists.
    #[error("unknown adhoc alarm")]
    UnknownAdhocAlarm(crate::ids::AdhocAlarmId),
    /// An alarm duration must be positive ("in 0 seconds" fires
    /// nothing and "in the past" is [`AdhocWhen::At`] data).
    #[error("alarm duration must be positive")]
    InvalidAdhocWhen,
    /// A snooze postpone must be positive ("snooze" that does not move
    /// the occurrence forward is not a snooze).
    #[error("snooze postpone must be positive")]
    InvalidSnoozePostpone,
    /// No time entry with this id exists.
    #[error("unknown time entry")]
    UnknownTimeEntry(crate::ids::TimeEntryId),
    /// A user-authored span must satisfy `end > start`.
    #[error("time span must be positive")]
    InvalidSpan,
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
    /// The command removed a task's reminder rules.
    ReminderCleared {
        /// How many rules were deleted.
        removed: usize,
    },
    /// The command added a reminder rule — or hit the offset dedup:
    /// `added: false` means zero actions were planned and `rule` names
    /// the surviving rule at that offset.
    ReminderRuleAdded {
        /// The fresh rule, or the dedup survivor.
        rule: ReminderRuleId,
        /// `false` = the dedup hit (nothing changed).
        added: bool,
    },
    /// The command removed the named reminder rule.
    ReminderRuleRemoved {
        /// The removed rule.
        rule: ReminderRuleId,
    },
    /// The command created the standalone alarm with this id.
    AdhocAlarmCreated {
        /// The fresh alarm.
        alarm: crate::ids::AdhocAlarmId,
    },
    /// The command cancelled the named standalone alarm.
    AdhocAlarmCancelled {
        /// The cancelled alarm.
        alarm: crate::ids::AdhocAlarmId,
    },
    /// The command opened the entry and closed the named ones (entry-id
    /// ascending).
    TrackingStarted {
        /// The freshly minted open entry.
        entry: crate::tracking::TimeEntry,
        /// The open entries the start closed, entry-id ascending.
        closed: Vec<crate::ids::TimeEntryId>,
    },
    /// The command closed these entries, post-stamp (entry-id ascending).
    TrackingStopped {
        /// The stamped rows.
        entries: Vec<crate::tracking::TimeEntry>,
    },
    /// The command logged the entry.
    TimeLogged {
        /// The freshly minted closed entry.
        entry: crate::tracking::TimeEntry,
    },
    /// The command adjusted the entry (post-adjust row).
    EntryAdjusted {
        /// The adjusted entry.
        entry: crate::tracking::TimeEntry,
    },
    /// The command deleted the named entry.
    EntryDeleted {
        /// The deleted entry.
        id: crate::ids::TimeEntryId,
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
    /// The command bound the board to a remote board; `wiped_refs` is
    /// the exact count of descendant remote refs the bind cleared
    /// (foreign-ref descendants of the freshly bound board — zero when
    /// the board carries none).
    Bound {
        /// How many descendant refs the bind wiped.
        wiped_refs: usize,
    },
    /// The command detached the board's remote binding; `stranded_ops`
    /// is the count of outbox ops attributed to the board that now wait
    /// for a re-bind (honest stranding — nothing was cancelled).
    Unbound {
        /// How many queued ops strand until a re-bind.
        stranded_ops: usize,
    },
    /// The command merged same-title duplicates; the counts are the
    /// merged pairs per entity kind plus the cancelled create-ops.
    Reconciled {
        /// Merged stack pairs.
        stacks: usize,
        /// Merged label pairs.
        labels: usize,
        /// Merged task pairs.
        tasks: usize,
        /// Create-ops cancelled on survivors that just adopted a ref.
        cancelled_ops: usize,
    },
    /// Accepted, redundant repeat: nothing changed, nothing persisted.
    NoOp,
    /// Sync request accepted; the engine forwards it to the sync actor.
    SyncRequested,
}

/// The named board, refusing typed when the id names nothing or a
/// tombstone (the parameterised replacement of the implicit
/// first-live-board pick; edit-style intents target live boards).
/// Archived boards stay commandable — the lookup checks `deleted` only.
fn live_board_of(app: &AppState, board: BoardId) -> Result<BoardId, CommandError> {
    match app.boards.get(&board) {
        None => Err(CommandError::UnknownBoard(board)),
        Some(b) if b.deleted => Err(CommandError::BoardDeleted(board)),
        Some(_) => Ok(board),
    }
}

/// An existing board for a *terminal* intent: tombstoned is not an error
/// here (the repeat is an accepted no-op), only absence is.
fn existing_board(app: &AppState, board: BoardId) -> Result<&crate::entities::Board, CommandError> {
    app.boards
        .get(&board)
        .ok_or(CommandError::UnknownBoard(board))
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

/// Closes every open entry at `now` (close stamps are exempt from the
/// span predicate — R-5 of the tracking plan: a skewed close must not
/// reject the whole command; projections clamp degenerate spans to
/// zero). Returns the stamped rows entry-id ascending and the distinct
/// touched task ids ascending.
fn close_all_open(app: &AppState, now: DateTime<Utc>) -> (Vec<TimeEntry>, Vec<TaskId>) {
    let mut rows = Vec::new();
    let mut touched = std::collections::BTreeSet::new();
    for entry in app
        .tracking
        .entries
        .values()
        .filter(|entry| entry.end.is_none())
    {
        let mut stamped = entry.clone();
        stamped.end = Some(now);
        stamped.updated_at = now;
        touched.insert(stamped.task);
        rows.push(stamped);
    }
    // Distinct tasks ascending (the ops' emission order).
    (rows, touched.into_iter().collect())
}

/// `close_all_open` narrowed to one task's entries (the
/// done/deleted-close shape), entry-id ascending.
fn task_tracking_closes(
    app: &AppState,
    task: TaskId,
    now: DateTime<Utc>,
) -> Vec<PersistenceAction> {
    app.tracking
        .entries
        .values()
        .filter(|entry| entry.task == task && entry.end.is_none())
        .map(|entry| {
            let mut stamped = entry.clone();
            stamped.end = Some(now);
            stamped.updated_at = now;
            PersistenceAction::UpsertTimeEntry(stamped)
        })
        .collect()
}

/// The `UpdateAppData` ops every touched task's sidecar flush trigger
/// rides (content-free; enqueued unconditionally like every accepted
/// tracking write — the `DismissReminder` precedent).
fn flush_ops(
    tasks: &[TaskId],
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> Vec<PersistenceAction> {
    tasks
        .iter()
        .map(|task| enqueue(LocalOp::UpdateAppData(*task), ids, now))
        .collect()
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
/// (unknown/deleted task, stack, label, or board). Transport and
/// lifecycle failures are not representable here.
///
/// `outbox` is the engine's current queue slice: the delete paths cancel
/// a target's pending ops by id, exactly like
/// [`crate::pipeline::apply_sync_report`] takes it.
///
/// `defaults` is the injected per-device reminder configuration
/// (`[reminders] default_before`) — the only config input of the
/// active-notification view the Mark Done planner consults.
#[allow(clippy::too_many_lines)] // one catalogue; splitting hides the table
pub fn plan_command(
    app: &AppState,
    outbox: &[PendingOp],
    command: &StateCommand,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
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
                scheduled: None,
                done: None,
                stack: target.id,
                order: *order,
                label_memberships: BTreeMap::default(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,

                archived: false,
                deleted: false,
                clocks: all_task_clocks(now),
                remote_seen: None,
                remote_baseline: None,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(task),
                    enqueue(LocalOp::CreateTask(id), ids, now),
                ],
                outcome: CommandOutcome::CreatedTask(id),
            })
        }
        StateCommand::CaptureTask { title, board } => {
            // Resolve-or-create compose, one batch: capture never errors
            // on the happy path — the landing board/stack is found or
            // minted, then the task rides the `CreateTask` shape. Batch
            // order: board upsert, stack upsert, task upsert, then the
            // enqueues (persistence applies them in order).
            let mut actions = Vec::new();
            let mut minted_stack: Option<StackId> = None;
            let target_stack: StackId = match board {
                None => {
                    if let Some(inbox) = app.inbox_board() {
                        if let Some(found) = app.canonical_first_stack(inbox) {
                            found
                        } else {
                            let stack = mint_capture_stack(
                                app,
                                &mut actions,
                                inbox,
                                crate::state::INBOX_BOARD_TITLE,
                                ids,
                                now,
                            );
                            minted_stack = Some(stack);
                            stack
                        }
                    } else {
                        let board_id = ids.new_board_id();
                        actions.push(PersistenceAction::UpsertBoard(crate::entities::Board {
                            id: board_id,
                            // Unbound by construction: binding is the
                            // user's ceremony; the queued ops strand
                            // honestly until then.
                            remote: None,
                            title: crate::state::INBOX_BOARD_TITLE.to_string(),
                            color: crate::entities::Color::new(
                                crate::entities::DEFAULT_BOARD_COLOR,
                            ),
                            archived: false,
                            deleted: false,
                            clocks: crate::entities::BoardClocks {
                                title: now,
                                color: now,
                                archived: now,
                                deleted: now,
                            },
                            remote_seen: None,
                        }));
                        let stack = mint_capture_stack(
                            app,
                            &mut actions,
                            board_id,
                            crate::state::INBOX_BOARD_TITLE,
                            ids,
                            now,
                        );
                        minted_stack = Some(stack);
                        stack
                    }
                }
                Some(board_id) => {
                    let board_id = live_board_of(app, *board_id)?;
                    if let Some(found) = app.backlog_stack(board_id) {
                        found
                    } else {
                        let stack = mint_capture_stack(
                            app,
                            &mut actions,
                            board_id,
                            crate::state::BACKLOG_STACK_TITLE,
                            ids,
                            now,
                        );
                        minted_stack = Some(stack);
                        stack
                    }
                }
            };
            // A minted landing stack defers its op to the enqueue tail.
            let minted = minted_stack;
            let order = app
                .tasks_in_stack(target_stack)
                .last()
                .map_or(0, |t| t.order + 1);
            let id = ids.new_task_id();
            actions.push(PersistenceAction::UpsertTask(Task {
                id,
                remote: None,
                title: title.clone(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack: target_stack,
                order,
                label_memberships: BTreeMap::default(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,

                archived: false,
                deleted: false,
                clocks: all_task_clocks(now),
                remote_seen: None,
                remote_baseline: None,
            }));
            if let Some(stack) = minted {
                actions.push(enqueue(LocalOp::CreateStack(stack), ids, now));
            }
            actions.push(enqueue(LocalOp::CreateTask(id), ids, now));
            Ok(CommandPlan {
                actions,
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
            if let Some(scheduled) = changes.scheduled {
                updated.scheduled = scheduled;
                updated.clocks.scheduled = now;
            }
            if let Some(effort) = changes.effort {
                updated.effort = effort;
                updated.clocks.effort = now;
            }
            if let Some(importance) = changes.importance {
                updated.importance = importance;
                updated.clocks.importance = now;
            }
            if let Some(urgency) = changes.urgency {
                updated.urgency = urgency;
                updated.clocks.urgency = now;
            }
            if let Some(recur) = &changes.recur {
                updated.recur.clone_from(recur);
                updated.clocks.recur = now;
            }
            if let Some(archived) = changes.archived {
                updated.archived = archived;
                updated.clocks.archived = now;
            }
            // The lane split (D-F3's enqueue contract, chunk 4): native
            // slots ride `UpdateTask` (full-send), the app slots ride
            // `UpdateAppData`, `scheduled` is flag-routed per D-V2, and
            // a mixed changeset enqueues both ops — full-send covers
            // both lanes.
            let board = app.stacks.get(&task.stack).map(|s| s.board);
            let features = board.map_or(DeckFeatures::default(), |b| board_features(app, b));
            let native_native = changes.title.is_some()
                || changes.description.is_some()
                || changes.duedate.is_some()
                || changes.archived.is_some()
                || (changes.scheduled.is_some() && features.card_startdate);
            let app_edit = changes.effort.is_some()
                || changes.importance.is_some()
                || changes.urgency.is_some()
                || changes.recur.is_some()
                || (changes.scheduled.is_some() && !features.card_startdate);
            let mut actions = vec![PersistenceAction::UpsertTask(updated)];
            if native_native {
                actions.push(enqueue(LocalOp::UpdateTask(*id), ids, now));
            }
            if app_edit {
                actions.push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::SetTaskDone { id, done } => {
            let task = live_task(app, *id)?;
            if task.done.is_some() == *done {
                return Ok(no_op());
            }
            let mut updated = task.clone();
            updated.done = done.then_some(now);
            updated.clocks.done = now;
            let mut plan = applied_task(&updated, ids, now);
            // The reopen hazard (R3.2, R3.5.5): completing the task also
            // dismisses its currently-active occurrences — explicit rows
            // in the same batch, derived from the same
            // `active_notifications` view the UI reads (one definition,
            // two consumers). Without them, a Done→reopen cycle would
            // resurrect stale receipts as active notifications (the done
            // state syncs on a different lane with different clocks).
            // Reopening emits nothing: a dismissal is permanent. The
            // emitted rows sync only if the watched task's document group
            // forms — the same `UpdateAppData` op `DismissReminder`
            // enqueues (the flush gate plans a flush from that op alone).
            if *done {
                // A completed task's open entries close at the done
                // stamp (D-T6) — closes precede dismissals in the batch,
                // matching the StartTracking order. Each block enqueues
                // its own conditional op; two `UpdateAppData` ops when
                // both fire is correct under the catalogue rule
                // (coalescing is push-side). Reopening closes nothing:
                // a stamped end is as permanent as a dismissal.
                let closes = task_tracking_closes(app, *id, now);
                if !closes.is_empty() {
                    plan.actions.extend(closes);
                    plan.actions
                        .push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
                }
                let dismissals: Vec<PersistenceAction> = active_notifications(
                    &app.tasks,
                    &app.reminders,
                    defaults,
                    now,
                )
                .into_iter()
                .filter(|note| {
                    matches!(note.anchor, crate::reminders::ReminderAnchor::Task(t) if t == *id)
                })
                .map(|note| {
                    PersistenceAction::RecordReminderDismissal(ReminderDismissal {
                        reminder: note.reminder,
                        occurrence: note.occurrence,
                        dismissed_at: now,
                    })
                })
                .collect();
                if !dismissals.is_empty() {
                    plan.actions.extend(dismissals);
                    plan.actions
                        .push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
                }
            }
            Ok(plan)
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
            // D-T6: the task's open entries close at the delete stamp;
            // the rows themselves survive (history outlives the task).
            // The op enqueues regardless of the binding — a soft-deleted
            // task keeps its local row, so the flush trigger stays
            // resolvable.
            let closes = task_tracking_closes(app, *id, now);
            if !closes.is_empty() {
                actions.extend(closes);
                actions.push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
            }
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
        StateCommand::CreateStack {
            board,
            title,
            order,
        } => {
            let board = live_board_of(app, *board)?;
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
        StateCommand::CreateLabel {
            board,
            title,
            color,
        } => {
            let board = live_board_of(app, *board)?;
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
            // Entity-death dominance (ADR 0004's labels amendment, D-2e
            // applied to the local door): a dead label's live memberships
            // are force-removed — a surviving `Added` membership would
            // push a dead id in the card PUT (it would 404) and render a
            // ghost. Removals are stamped, never deleted. Queued
            // assignments for an unbound label already fail below; a
            // bound label keeps its queued `DeleteLabel` push.
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
            let removal = crate::entities::MembershipState::Removed { at: now };
            for task_id in app
                .tasks
                .iter()
                .filter(|(_, t)| {
                    t.is_live()
                        && matches!(
                            t.label_memberships.get(id),
                            Some(crate::entities::MembershipState::Added { .. })
                        )
                })
                .map(|(task_id, _)| *task_id)
                .collect::<Vec<_>>()
            {
                let mut updated_task = app.tasks[&task_id].clone();
                let entry = updated_task.label_memberships.remove(id);
                updated_task
                    .label_memberships
                    .insert(*id, crate::merge::merge_membership_opt(entry, removal));
                actions.push(PersistenceAction::UpsertTask(updated_task));
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::AssignLabel { task, label } => {
            let source = live_task(app, *task)?;
            live_label(app, *label)?;
            // Membership-guarded (the OR-set verb, #129): an already-live
            // membership is a no-op; an absent one — or a remembered
            // tombstone — re-adds, the new stamp overwriting the removal.
            if matches!(
                source.label_memberships.get(label),
                Some(crate::entities::MembershipState::Added { .. })
            ) {
                return Ok(no_op());
            }
            let mut updated = source.clone();
            updated
                .label_memberships
                .insert(*label, crate::entities::MembershipState::Added { at: now });
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
            // Tombstone-aware absent-guard: removing an absent — or
            // already removed — membership is a no-op, not a clock bump.
            if !matches!(
                source.label_memberships.get(label),
                Some(crate::entities::MembershipState::Added { .. })
            ) {
                return Ok(no_op());
            }
            let mut updated = source.clone();
            updated.label_memberships.insert(
                *label,
                crate::entities::MembershipState::Removed { at: now },
            );
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTask(updated),
                    enqueue(LocalOp::UnassignLabel(*task, *label), ids, now),
                ],
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::CreateBoard { title, color } => {
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
        StateCommand::RenameBoard { board, title } => {
            let board_id = live_board_of(app, *board)?;
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
        StateCommand::SetBoardColor { board, color } => {
            let board_id = live_board_of(app, *board)?;
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
        StateCommand::SetBoardArchived { board, archived } => {
            let board_id = live_board_of(app, *board)?;
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
        StateCommand::DeleteBoard { board } => {
            // Terminal intent: a repeat (a tombstoned board still exists)
            // is an accepted no-op; only absence is a rejection, typed.
            let board_id = existing_board(app, *board)?.id;
            if app.boards[&board_id].deleted {
                return Ok(no_op());
            }
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
                        | LocalOp::UnassignLabel(id, _)
                        | LocalOp::UpdateAppData(id) => doomed_tasks.contains(&id),
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
        StateCommand::ResetBoard { board } => {
            let board_id = live_board_of(app, *board)?;
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
                | LocalOp::UnassignLabel(id, _)
                | LocalOp::UpdateAppData(id) => board_tasks.contains(&id),
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
                    // which the rules arm above already covers; alarms
                    // are device-global, not board content — a board
                    // clear never touches either.
                    ReminderRef::Rule(_) | ReminderRef::Adhoc(_) => false,
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
        StateCommand::BindBoard {
            board,
            remote,
            connector,
        } => {
            let board_id = live_board_of(app, *board)?;
            let board = &app.boards[&board_id];
            // Binding-scoped equality (D6-2): remote ids are namespaced
            // per connector, so "the same remote" is the full
            // `(connector, remote)` pair — the same numeric id through a
            // different connector is a genuinely different remote board.
            let binding = crate::ids::RemoteBoardBinding {
                connector: connector.clone(),
                board: *remote,
            };
            match board.remote.as_ref() {
                Some(bound) if *bound == binding => return Ok(no_op()),
                Some(bound) => {
                    return Err(CommandError::BoardAlreadyBound {
                        board: board_id,
                        bound: bound.board,
                    });
                }
                None => {}
            }
            // One remote board binds to at most one local board — scoped
            // to the same connector: another connector's remote with the
            // same numeric id is a different board and cannot collide.
            if let Some(holder) = app.boards.values().find(|b| {
                b.id != board_id
                    && b.remote
                        .as_ref()
                        .is_some_and(|r| r.board == *remote && r.connector == *connector)
            }) {
                return Err(CommandError::RemoteBoardInUse {
                    remote: *remote,
                    board: holder.id,
                });
            }
            // The bind-wipe: foreign refs among the board's live
            // descendants clear (the entity graph adopts the new
            // remote's refs on the next pull); refs already naming the
            // bind target stay (pathological-but-possible, not ours to
            // destroy); create-class ops stay queued (they re-create
            // under the new remote); update-class ops on wiped entities
            // strand until the next push cycle dead-letters them
            // (D3-3). Tombstoned descendants keep their refs — the R9c
            // finalize pushes need them.
            let mut actions: Vec<PersistenceAction> = Vec::new();
            let mut wiped_refs = 0usize;
            let board_stacks: Vec<Stack> = app
                .stacks
                .values()
                .filter(|s| s.board == board_id && !s.deleted)
                .cloned()
                .collect();
            let board_stack_ids: std::collections::BTreeSet<StackId> =
                board_stacks.iter().map(|s| s.id).collect();
            for mut stack in board_stacks {
                if stack.remote.is_some_and(|r| r.board != *remote) {
                    stack.remote = None;
                    wiped_refs += 1;
                    actions.push(PersistenceAction::UpsertStack(stack));
                }
            }
            for mut task in app
                .tasks
                .values()
                .filter(|t| t.is_live() && board_stack_ids.contains(&t.stack))
                .cloned()
                .collect::<Vec<Task>>()
            {
                if task.remote.is_some_and(|r| r.board != *remote) {
                    task.remote = None;
                    // The binding is gone, so the observed remote snapshot
                    // goes with it — the baseline clears with the binding
                    // (ADR 0004's remote baseline).
                    task.remote_baseline = None;
                    wiped_refs += 1;
                    actions.push(PersistenceAction::UpsertTask(task));
                }
            }
            for mut label in app
                .labels
                .values()
                .filter(|l| l.board == board_id && !l.deleted)
                .cloned()
                .collect::<Vec<Label>>()
            {
                if label.remote.is_some_and(|r| r.board != *remote) {
                    label.remote = None;
                    wiped_refs += 1;
                    actions.push(PersistenceAction::UpsertLabel(label));
                }
            }
            let mut updated = board.clone();
            // The mint source is the command (D6-1): the app layer
            // resolves and guards the name; the engine is config-blind.
            updated.remote = Some(binding);
            actions.push(PersistenceAction::UpsertBoard(updated));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Bound { wiped_refs },
            })
        }
        StateCommand::UnbindBoard { board } => {
            // Edit-style intent: a tombstoned board refuses (the R9c
            // tombstone push needs its binding until it finalizes —
            // unbinding would strand the remote delete and let the
            // planner's unbound-delete Noop finalize it locally while
            // the remote copy survives).
            let board_id = live_board_of(app, *board)?;
            let board = &app.boards[&board_id];
            if board.remote.is_none() {
                // Idempotent repeat: already unbound.
                return Ok(no_op());
            }
            // Honest stranding: the outbox is untouched; the count is
            // the same attribution the pending_ops projection uses.
            let stranded_ops = count_stranded_ops(app, outbox, board_id);
            let mut updated = board.clone();
            updated.remote = None;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertBoard(updated),
                    PersistenceAction::DeleteSyncStatus(board_id),
                ],
                outcome: CommandOutcome::Unbound { stranded_ops },
            })
        }
        StateCommand::ReconcileByTitle { board } => {
            let board_id = live_board_of(app, *board)?;
            let (actions, counts) = crate::reconcile::plan_reconcile(app, outbox, board_id);
            if actions.is_empty() {
                return Ok(no_op());
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Reconciled {
                    stacks: counts.stacks,
                    labels: counts.labels,
                    tasks: counts.tasks,
                    cancelled_ops: counts.cancelled_ops,
                },
            })
        }
        StateCommand::SnoozeReminder {
            reminder,
            occurrence,
            postpone,
        } => {
            // Edit-style intent: snoozing a dead task's reminder is a
            // rejection, not a meaningless-redundant repeat. Alarms have
            // no task and no liveness states — the arm is only the
            // existence guard, and the shared `live_task` guard below
            // runs on task-anchored refs only.
            let task = match *reminder {
                ReminderRef::Rule(rule_id) => {
                    let rule = app
                        .reminders
                        .rules
                        .get(&rule_id)
                        .ok_or(CommandError::UnknownReminderRule(rule_id))?;
                    Some(rule.task)
                }
                ReminderRef::Default(task) => Some(task),
                ReminderRef::Adhoc(alarm_id) => {
                    app.reminders
                        .alarms
                        .get(&alarm_id)
                        .ok_or(CommandError::UnknownAdhocAlarm(alarm_id))?;
                    None
                }
            };
            if let Some(task) = task {
                live_task(app, task)?;
            }
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
        StateCommand::DismissReminder {
            reminder,
            occurrence,
        } => {
            // The ref must resolve (a dismissal of a stranger is a
            // rejection, like every other named-target command), but the
            // anchor's liveness is not required: a dismissal is a
            // terminal fact, not an edit — dismissing a dead task's
            // leftover notification is meaningful and idempotent.
            let task = match *reminder {
                ReminderRef::Rule(rule_id) => {
                    let rule = app
                        .reminders
                        .rules
                        .get(&rule_id)
                        .ok_or(CommandError::UnknownReminderRule(rule_id))?;
                    Some(rule.task)
                }
                ReminderRef::Default(task) => Some(task),
                ReminderRef::Adhoc(alarm_id) => {
                    app.reminders
                        .alarms
                        .get(&alarm_id)
                        .ok_or(CommandError::UnknownAdhocAlarm(alarm_id))?;
                    None
                }
            };
            // A dismissal is permanent: the repeat is the accepted no-op
            // (nothing changed, nothing persisted). An unfired occurrence
            // is accepted the same way — pre-dismissing is a no-op
            // semantics, not an error.
            if app.reminders.is_dismissed(reminder, occurrence) {
                return Ok(no_op());
            }
            let mut actions = vec![PersistenceAction::RecordReminderDismissal(
                ReminderDismissal {
                    reminder: *reminder,
                    occurrence: *occurrence,
                    dismissed_at: now,
                },
            )];
            // Rule/default refs sync through the watched task's document;
            // adhoc alarms are device-local — no op, nothing to flush.
            if let Some(task) = task {
                actions.push(enqueue(LocalOp::UpdateAppData(task), ids, now));
            }
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::Applied,
            })
        }
        StateCommand::AddReminderRule { task, offset } => {
            // Edit-style intent: reminding a dead task is a rejection,
            // even when the offset would "dedup".
            live_task(app, *task)?;
            // Dedup by offset: a rule at this exact offset already
            // answers, so plan nothing and name the survivor (`BTreeMap`
            // iteration is id-ordered — the hit is deterministic).
            if let Some(survivor) = app
                .reminders
                .rules
                .values()
                .find(|rule| rule.task == *task && rule.offset == *offset)
            {
                return Ok(CommandPlan {
                    actions: Vec::new(),
                    outcome: CommandOutcome::ReminderRuleAdded {
                        rule: survivor.id,
                        added: false,
                    },
                });
            }
            // The fresh rule upserts with both stamps at `now` (D9); the
            // document-lane trigger rides the add — the op is the flush
            // trigger, never a native card write.
            let rule = crate::reminders::ReminderRule {
                id: ids.new_reminder_rule_id(),
                task: *task,
                offset: *offset,
                created_at: now,
                updated_at: now,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertReminderRule(rule.clone()),
                    enqueue(LocalOp::UpdateAppData(*task), ids, now),
                ],
                outcome: CommandOutcome::ReminderRuleAdded {
                    rule: rule.id,
                    added: true,
                },
            })
        }
        StateCommand::RemoveReminderRule { rule } => {
            // Existence-only lookup (like the snooze arm): the command
            // targets the rule, not a task; `DeleteTask`'s cascade makes
            // a rule of a dead task unreachable.
            let task = app
                .reminders
                .rules
                .get(rule)
                .ok_or(CommandError::UnknownReminderRule(*rule))?
                .task;
            // One op for one removal: the flush derives the document
            // tombstone stamped with this removal instant; the
            // persistence cascade prunes the rule's deliveries and
            // snooze links.
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::DeleteReminderRule(*rule),
                    enqueue(LocalOp::UpdateAppData(task), ids, now),
                ],
                outcome: CommandOutcome::ReminderRuleRemoved { rule: *rule },
            })
        }
        StateCommand::ClearReminderRules { task } => {
            live_task(app, *task)?;
            let deletes: Vec<PersistenceAction> = task_rule_deletes(app, *task).collect();
            if deletes.is_empty() {
                return Ok(no_op());
            }
            let removed = deletes.len();
            let mut actions = deletes;
            // The removals become flush-time tombstones (D8); an empty
            // clear stayed a no-op above.
            actions.push(enqueue(LocalOp::UpdateAppData(*task), ids, now));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::ReminderCleared { removed },
            })
        }
        StateCommand::CreateAdhocAlarm { label, when } => {
            // Device-local by ruling: no outbox op, no document-lane
            // trigger — the alarm never leaves this machine.
            let fire_at = match *when {
                crate::reminders::AdhocWhen::In(dur) => {
                    if dur <= chrono::TimeDelta::zero() {
                        return Err(CommandError::InvalidAdhocWhen);
                    }
                    // Total arithmetic: an unrepresentable target
                    // saturates at chrono's ceiling (the SnoozeReminder
                    // precedent), never panics the planner.
                    now.checked_add_signed(dur)
                        .unwrap_or(DateTime::<Utc>::MAX_UTC)
                }
                // A past instant is legal data: it fires on the next
                // tick.
                crate::reminders::AdhocWhen::At(t) => t,
            };
            let alarm = crate::reminders::AdhocAlarm {
                id: ids.new_adhoc_alarm_id(),
                label: label.clone(),
                fire_at,
                created_at: now,
            };
            Ok(CommandPlan {
                actions: vec![PersistenceAction::UpsertAdhocAlarm(alarm.clone())],
                outcome: CommandOutcome::AdhocAlarmCreated { alarm: alarm.id },
            })
        }
        StateCommand::CancelAdhocAlarm { alarm } => {
            // Existence-only lookup (terminal-by-deletion, like
            // `RemoveReminderRule`): no tombstone exists, so a repeat is
            // a typed rejection, not a no-op. The persistence cascade
            // prunes the alarm's deliveries and snooze links.
            app.reminders
                .alarms
                .get(alarm)
                .ok_or(CommandError::UnknownAdhocAlarm(*alarm))?;
            Ok(CommandPlan {
                actions: vec![PersistenceAction::DeleteAdhocAlarm(*alarm)],
                outcome: CommandOutcome::AdhocAlarmCancelled { alarm: *alarm },
            })
        }
        StateCommand::StartTracking { task } => {
            // No done-gate: a completed task is trackable (R-8); the
            // guard resolves liveness only.
            let live = live_task(app, *task)?;
            // Idempotent repeat (R-6): exactly one open entry and it is
            // this task's. Any other open shape (a foreign open entry, a
            // transient double-open) closes all and reopens.
            let open: Vec<&TimeEntry> = app
                .tracking
                .entries
                .values()
                .filter(|entry| entry.end.is_none())
                .collect();
            if open.len() == 1 && open[0].task == *task {
                return Ok(no_op());
            }
            // Resolve → close-all → open → dismiss → flush ops (the
            // order the stage gate enforces).
            let (closed_rows, touched_tasks) = close_all_open(app, now);
            let closed_ids: Vec<crate::ids::TimeEntryId> =
                closed_rows.iter().map(|row| row.id).collect();
            let entry = TimeEntry {
                id: ids.new_time_entry_id(),
                task: *task,
                task_title: live.title.clone(),
                start: now,
                end: None,
                created_at: now,
                updated_at: now,
            };
            let mut actions: Vec<PersistenceAction> = closed_rows
                .iter()
                .map(|row| PersistenceAction::UpsertTimeEntry(row.clone()))
                .collect();
            actions.push(PersistenceAction::UpsertTimeEntry(entry.clone()));
            // The exact SetTaskDone dismissal machinery (D-T5): the
            // task's currently-active occurrences become dismissal rows
            // in the same batch; the actor's withdraw direction picks
            // them up for the OS toasts.
            actions.extend(
                active_notifications(&app.tasks, &app.reminders, defaults, now)
                    .into_iter()
                    .filter(|note| {
                        matches!(note.anchor, crate::reminders::ReminderAnchor::Task(t) if t == *task)
                    })
                    .map(|note| {
                        PersistenceAction::RecordReminderDismissal(ReminderDismissal {
                            reminder: note.reminder,
                            occurrence: note.occurrence,
                            dismissed_at: now,
                        })
                    }),
            );
            // The target's op last and always: a start always mints a
            // row the sidecar must flush, even when it closed nothing.
            let mut ops_tasks = touched_tasks;
            ops_tasks.retain(|t| *t != *task);
            ops_tasks.push(*task);
            actions.extend(flush_ops(&ops_tasks, ids, now));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::TrackingStarted {
                    entry,
                    closed: closed_ids,
                },
            })
        }
        StateCommand::StopTracking => {
            if app
                .tracking
                .entries
                .values()
                .all(|entry| entry.end.is_some())
            {
                return Ok(no_op());
            }
            let (closed_rows, touched_tasks) = close_all_open(app, now);
            let mut actions: Vec<PersistenceAction> = closed_rows
                .iter()
                .map(|row| PersistenceAction::UpsertTimeEntry(row.clone()))
                .collect();
            actions.extend(flush_ops(&touched_tasks, ids, now));
            Ok(CommandPlan {
                actions,
                outcome: CommandOutcome::TrackingStopped {
                    entries: closed_rows,
                },
            })
        }
        StateCommand::LogTime { task, start, end } => {
            let live = live_task(app, *task)?;
            // User-authored span: the strict predicate applies (R-5's
            // exemption covers close stamps only).
            let span = ClosedSpan::try_new(*start, *end).map_err(|_| CommandError::InvalidSpan)?;
            let entry = TimeEntry {
                id: ids.new_time_entry_id(),
                task: *task,
                task_title: live.title.clone(),
                start: span.start(),
                end: Some(span.end()),
                // The mutation clocks are the planner's `now`, never the
                // historical span — retroactive rows stay newest-write
                // under LWW (R-14).
                created_at: now,
                updated_at: now,
            };
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTimeEntry(entry.clone()),
                    enqueue(LocalOp::UpdateAppData(*task), ids, now),
                ],
                outcome: CommandOutcome::TimeLogged { entry },
            })
        }
        StateCommand::AdjustTimeEntry { id, start, end } => {
            // Entry-only resolution (R-10): the task may be
            // soft-deleted; history stays editable.
            let stored = app
                .tracking
                .entries
                .get(id)
                .ok_or(CommandError::UnknownTimeEntry(*id))?;
            // Change-mask-never-value: both `None` is the empty edit; a
            // mask naming only unchanged values cannot churn
            // `updated_at` onto the sync wire (the MoveTask
            // identical-position precedent).
            let new_start = start.unwrap_or(stored.start);
            let new_end = end.or(stored.end);
            if new_start == stored.start && new_end == stored.end {
                return Ok(no_op());
            }
            // The strict predicate applies to the resulting *known*
            // span; an open entry adjusted start-only has no end to
            // validate and stays open. `new_end` is `None` only when
            // the entry itself is open and the mask left it open —
            // a history edit can structurally never reopen (ADR 0017):
            // there is no payload that clears a stamped end.
            if let Some(new_end) = new_end {
                ClosedSpan::try_new(new_start, new_end).map_err(|_| CommandError::InvalidSpan)?;
            }
            let mut adjusted = stored.clone();
            adjusted.start = new_start;
            adjusted.end = new_end;
            adjusted.updated_at = now;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::UpsertTimeEntry(adjusted.clone()),
                    enqueue(LocalOp::UpdateAppData(adjusted.task), ids, now),
                ],
                outcome: CommandOutcome::EntryAdjusted { entry: adjusted },
            })
        }
        StateCommand::DeleteTimeEntry { id } => {
            // Absent is the accepted no-op (R-7): a synced tombstone may
            // have folded the row away already, making the local repeat
            // redundant rather than wrong.
            let Some(stored) = app.tracking.entries.get(id) else {
                return Ok(no_op());
            };
            let task = stored.task;
            Ok(CommandPlan {
                actions: vec![
                    PersistenceAction::DeleteTimeEntry(*id),
                    enqueue(LocalOp::UpdateAppData(task), ids, now),
                ],
                outcome: CommandOutcome::EntryDeleted { id: *id },
            })
        }
        StateCommand::RequestSync => Ok(CommandPlan {
            actions: Vec::new(),
            outcome: CommandOutcome::SyncRequested,
        }),
    }
}

/// The capture path's landing stack when none exists: mints one
/// (`title`) appended after the board's live stacks (the
/// `append_stack_order` arithmetic, engine-side), pushing only the
/// upsert — the caller queues the `CreateStack` op after the task
/// upsert (the plan's batch order).
fn mint_capture_stack(
    app: &AppState,
    actions: &mut Vec<PersistenceAction>,
    board: BoardId,
    title: &str,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> StackId {
    let order = app
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.order)
        .max()
        .map_or(0, |max| max + 1);
    let id = ids.new_stack_id();
    actions.push(PersistenceAction::UpsertStack(Stack {
        id,
        remote: None,
        board,
        title: title.to_string(),
        order,
        archived: false,
        deleted: false,
        clocks: crate::entities::StackClocks {
            title: now,
            order: now,
            deleted: now,
        },
        remote_seen: None,
    }));
    id
}

fn enqueue(op: LocalOp, ids: &dyn IdGenerator, now: DateTime<Utc>) -> PersistenceAction {
    PersistenceAction::EnqueueOp(PendingOp {
        op_id: ids.new_op_id(),
        op,
        queued_at: now,
    })
}

/// The outbox ops attributed to `board` through the entity graph — the
/// same attribution the pending-ops projection derives (one truth for
/// the unbind receipt's stranded count and the app's pre-count).
fn count_stranded_ops(app: &AppState, outbox: &[PendingOp], board: BoardId) -> usize {
    outbox
        .iter()
        .filter(|pending| {
            crate::state::attributed_board(&pending.op, &app.stacks, &app.tasks, &app.labels)
                == Some(board)
        })
        .count()
}

/// How many descendant refs a `BindBoard { board, remote }` would wipe:
/// live stacks/tasks/labels of the board whose ref names a different
/// remote board. The bind receipt's `wiped_refs` is this exact count.
#[must_use]
pub fn bind_wipe_count(app: &AppState, board: BoardId, remote: RemoteBoardId) -> usize {
    let board_stacks: std::collections::BTreeSet<StackId> = app
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.id)
        .collect();
    let stacks = app
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .filter(|s| s.remote.is_some_and(|r| r.board != remote))
        .count();
    let tasks = app
        .tasks
        .values()
        .filter(|t| t.is_live() && board_stacks.contains(&t.stack))
        .filter(|t| t.remote.is_some_and(|r| r.board != remote))
        .count();
    let labels = app
        .labels
        .values()
        .filter(|l| l.board == board && !l.deleted)
        .filter(|l| l.remote.is_some_and(|r| r.board != remote))
        .count();
    stacks + tasks + labels
}

/// The update-class outbox ops targeting entities a `BindBoard { board,
/// remote }` would wipe — the bind receipt's *forecast* of stranded ops
/// (an upper bound: some are re-matched by the follow-up reconcile and
/// become pushable; the exact count lands push-side, D3-3/D3-6).
#[must_use]
pub fn bind_wipe_forecast(
    app: &AppState,
    outbox: &[PendingOp],
    board: BoardId,
    remote: RemoteBoardId,
) -> usize {
    let board_stacks: std::collections::BTreeSet<StackId> = app
        .stacks
        .values()
        .filter(|s| s.board == board && !s.deleted)
        .map(|s| s.id)
        .collect();
    let wipes_stack = |id: &StackId| {
        app.stacks.get(id).is_some_and(|s| {
            s.board == board && !s.deleted && s.remote.is_some_and(|r| r.board != remote)
        })
    };
    let wipes_task = |id: &TaskId| {
        app.tasks.get(id).is_some_and(|t| {
            t.is_live()
                && board_stacks.contains(&t.stack)
                && t.remote.is_some_and(|r| r.board != remote)
        })
    };
    let wipes_label = |id: &LabelId| {
        app.labels.get(id).is_some_and(|l| {
            l.board == board && !l.deleted && l.remote.is_some_and(|r| r.board != remote)
        })
    };
    outbox
        .iter()
        .filter(|pending| match pending.op {
            LocalOp::UpdateTask(id)
            | LocalOp::MoveTask(id)
            | LocalOp::AssignLabel(id, _)
            | LocalOp::UnassignLabel(id, _) => wipes_task(&id),
            LocalOp::RenameStack(id) | LocalOp::MoveStack(id) => wipes_stack(&id),
            LocalOp::UpdateLabel(id) => wipes_label(&id),
            _ => false,
        })
        .count()
}

fn all_task_clocks(now: DateTime<Utc>) -> TaskClocks {
    TaskClocks {
        title: now,
        description: now,
        duedate: now,
        scheduled: now,
        done: now,
        position: now,
        effort: now,
        importance: now,
        urgency: now,
        recur: now,

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

/// The task's board's recorded capability flags (D6's routing input):
/// the same entity chain the planner walks (`task → stack → board`).
/// No row (an unbound board, a not-yet-reported one) is the D-V1
/// fallback — `CONSERVATIVE` routes `scheduled` to the document lane.
fn board_features(app: &AppState, board: BoardId) -> DeckFeatures {
    app.sync
        .get(board)
        .map(|row| row.features)
        .unwrap_or_default()
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
    use proptest::prelude::*;

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
            scheduled: ts(secs),
            done: ts(secs),
            position: ts(secs),
            effort: ts(secs),
            importance: ts(secs),
            urgency: ts(secs),
            recur: ts(secs),

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
            scheduled: None,
            stack: stack_id,
            order: 1,
            label_memberships: BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: clocks(T0 - 3_600),
            remote_seen: None,
            remote_baseline: None,
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
                    remote: crate::test_support::test_binding(crate::ids::RemoteBoardId(77)),
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
            tracking: crate::tracking::TrackingState::default(),
            sync: crate::state::SyncStates::with_row(
                BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD))),
                crate::state::SyncStatus::default(),
            ),
            last_updated: None,
        };
        let task_id = TaskId::from(uuid::Uuid::from_u128(u128::from(TASK)));
        let label_id = LabelId::from(uuid::Uuid::from_u128(u128::from(LABEL)));
        // The fixture task already carries the fixture label in its set,
        // so unassign tests have something to remove.
        let mut app = app;
        app.tasks
            .get_mut(&task_id)
            .unwrap()
            .label_memberships
            .insert(
                label_id,
                crate::entities::MembershipState::Added { at: now() },
            );
        (app, Vec::new())
    }

    fn stack_id() -> StackId {
        StackId::from(uuid::Uuid::from_u128(u128::from(STACK)))
    }

    fn board_id() -> BoardId {
        BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)))
    }

    fn board(raw: u64, live: bool) -> crate::entities::Board {
        crate::entities::Board {
            id: BoardId::from(uuid::Uuid::from_u128(u128::from(raw))),
            remote: None,
            title: format!("board {raw}"),
            color: crate::entities::Color::new("00ccff"),
            archived: false,
            deleted: !live,
            clocks: crate::entities::BoardClocks {
                title: crate::entities::BoardClocks::epoch(),
                color: crate::entities::BoardClocks::epoch(),
                archived: crate::entities::BoardClocks::epoch(),
                deleted: crate::entities::BoardClocks::epoch(),
            },
            remote_seen: None,
        }
    }

    // ---- Board resolvers -----------------------------------------------

    #[test]
    fn live_board_of_maps_absent_tombstoned_and_live_ids_typed() {
        let (app, _outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            live_board_of(&app, stranger),
            Err(CommandError::UnknownBoard(stranger))
        );

        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(
            live_board_of(&dead, stranger),
            Err(CommandError::BoardDeleted(stranger))
        );
        assert_eq!(live_board_of(&dead, board_id()), Ok(board_id()));
    }

    #[test]
    fn existing_board_admits_a_tombstone_and_refuses_only_absence() {
        let (app, _outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            existing_board(&app, stranger).map(|b| b.id),
            Err(CommandError::UnknownBoard(stranger))
        );

        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(existing_board(&dead, stranger).map(|b| b.id), Ok(stranger));
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
        plan_command(
            app,
            outbox,
            command,
            &CountingIds::default(),
            &crate::reminders::ReminderDefaults::default(),
            now(),
        )
    }

    fn op_for(raw: u128, op: LocalOp) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(raw)),
            op,
            queued_at: ts(T0 - 60),
        }
    }

    // ---- DismissReminder + Mark Done dismissal emission (R3) ----------

    /// A due-anchored fixture state: the fixture task gains a due date and
    /// a delivered, undismissed rule occurrence, so it has exactly one
    /// active notification.
    fn due_task_with_fired_rule() -> (AppState, Vec<PendingOp>, ReminderRuleId) {
        let (mut app, outbox) = base();
        let rule_id = ReminderRuleId::from(uuid::Uuid::from_u128(4_001));
        let task_id = task_id();
        {
            let task = app.tasks.get_mut(&task_id).unwrap();
            task.duedate = Some(ts(T0 + 3_600));
        }
        let rule = crate::reminders::ReminderRule {
            id: rule_id,
            task: task_id,
            // Two hours before the T0 + 3_600 due: the occurrence
            // T0 - 3_600 already fired on this device.
            offset: crate::reminders::ReminderOffset::before(
                chrono::TimeDelta::try_hours(-2).unwrap(),
            )
            .unwrap(),
            created_at: ts(T0 - 60),
            updated_at: ts(T0 - 60),
        };
        app.reminders.rules.insert(rule.id, rule);
        app.reminders
            .deliveries
            .insert(crate::reminders::ReminderDelivery {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
                delivered_at: ts(T0 - 3_600),
            });
        (app, outbox, rule_id)
    }

    #[test]
    fn dismiss_reminder_persists_and_syncs_task_anchored_refs() {
        // Rule ref: dismissal row + UpdateAppData for the watched task.
        let (app, outbox, rule_id) = due_task_with_fired_rule();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DismissReminder {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert_eq!(
            plan.actions[0],
            PersistenceAction::RecordReminderDismissal(crate::reminders::ReminderDismissal {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
                dismissed_at: ts(T0),
            })
        );
        assert!(plan.actions[1..].iter().any(|action| matches!(
            action,
            PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::UpdateAppData(t) if t == task_id())
        )));

        // Default ref: same shape, the task is its own resolution.
        let (mut app, outbox) = base();
        app.reminders.rules.clear();
        app.reminders
            .deliveries
            .insert(crate::reminders::ReminderDelivery {
                reminder: ReminderRef::Default(task_id()),
                occurrence: ts(T0 + 3_600),
                delivered_at: ts(T0 + 3_600),
            });
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DismissReminder {
                reminder: ReminderRef::Default(task_id()),
                occurrence: ts(T0 + 3_600),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::UpdateAppData(t) if t == task_id())
        )));

        // Adhoc ref: the dismissal persists, but alarms do not sync — no
        // outbox op.
        let (mut app, outbox) = base();
        let alarm_id = crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(4_100));
        app.reminders.alarms.insert(
            alarm_id,
            crate::reminders::AdhocAlarm {
                id: alarm_id,
                label: "tea".into(),
                fire_at: ts(T0 - 60),
                created_at: ts(T0 - 120),
            },
        );
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DismissReminder {
                reminder: ReminderRef::Adhoc(alarm_id),
                occurrence: ts(T0 - 60),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert_eq!(plan.actions.len(), 1);
        assert!(matches!(
            &plan.actions[0],
            PersistenceAction::RecordReminderDismissal(_)
        ));
    }

    #[test]
    fn dismiss_reminder_repeat_is_a_no_op_and_unknown_ref_is_typed() {
        let (mut app, outbox, rule_id) = due_task_with_fired_rule();
        app.reminders
            .dismissals
            .insert(crate::reminders::ReminderDismissal {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
                dismissed_at: ts(T0 - 30),
            });
        // Already dismissed: permanent for the occurrence, accepted no-op.
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DismissReminder {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(plan.actions, Vec::new());

        // Unknown rule / unknown alarm: the typed rejections.
        let stranger_rule = ReminderRuleId::from(uuid::Uuid::from_u128(4_999));
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::DismissReminder {
                    reminder: ReminderRef::Rule(stranger_rule),
                    occurrence: ts(T0 - 3_600),
                },
            ),
            Err(CommandError::UnknownReminderRule(id)) if id == stranger_rule
        ));
        let stranger_alarm = crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(4_998));
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::DismissReminder {
                    reminder: ReminderRef::Adhoc(stranger_alarm),
                    occurrence: ts(T0 - 60),
                },
            ),
            Err(CommandError::UnknownAdhocAlarm(id)) if id == stranger_alarm
        ));
    }

    #[test]
    fn set_task_done_emits_dismissals_exactly_for_active_occurrences() {
        // The fixture has one active rule occurrence; Mark Done must
        // dismiss exactly it (the reopen hazard, R3.5.5) — and nothing
        // else, not another reminder's, not an already-dismissed one.
        let (app, outbox, rule_id) = due_task_with_fired_rule();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: true,
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let dismissals: Vec<_> = plan
            .actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::RecordReminderDismissal(d) => Some(*d),
                _ => None,
            })
            .collect();
        assert_eq!(
            dismissals,
            vec![crate::reminders::ReminderDismissal {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
                dismissed_at: ts(T0),
            }],
            "exactly the currently-active occurrences are dismissed"
        );
        // The emitted rows must actually flush: the plan queues the watched
        // task's document-group op (the only thing `has_flush_work` plans a
        // flush from) — without it the dismissal rows would sync never on a
        // quiet board and the reopen hazard would stay open fleet-wide.
        assert!(plan.actions.iter().any(|action| matches!(
            action,
            PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::UpdateAppData(t) if t == task_id())
        )));

        // Reopening emits nothing (a dismissal is permanent) — and after
        // the batch lands, the reopen cannot resurrect the stale
        // notification: the pair is dismissed.
        let reopen = run(
            &app,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: false,
            },
        )
        .unwrap();
        assert!(
            reopen
                .actions
                .iter()
                .all(|action| !matches!(action, PersistenceAction::RecordReminderDismissal(_))),
            "reopen dismisses nothing"
        );
    }

    #[test]
    fn set_task_done_on_a_task_without_active_notifications_emits_none() {
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
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert!(
            plan.actions
                .iter()
                .all(|action| !matches!(action, PersistenceAction::RecordReminderDismissal(_))),
            "no active occurrences, no dismissal rows"
        );
        assert!(
            plan.actions.iter().all(|action| !matches!(
                action,
                PersistenceAction::EnqueueOp(op) if matches!(op.op, LocalOp::UpdateAppData(_))
            )),
            "nothing dismissed, nothing to flush"
        );
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
        // Untouched fields keep their clocks — including position and
        // the tombstone clock (label memberships carry no field clock).
        assert_eq!(updated.clocks.position, ts(T0 - 3_600));
        assert_eq!(updated.clocks.done, ts(T0 - 3_600));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateTask(id) if id == task_id()));
    }

    #[test]
    fn update_task_sets_and_stamps_the_field_family() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    scheduled: Some(Some(ts(T0 + 172_800))),
                    effort: Some(Some(crate::entities::Effort::new(1_500))),
                    importance: Some(Some(crate::entities::Importance::High)),
                    urgency: Some(Some(crate::entities::Urgency::VeryLow)),
                    recur: None,

                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.scheduled, Some(ts(T0 + 172_800)));
        assert_eq!(updated.effort, Some(crate::entities::Effort::new(1_500)));
        assert_eq!(updated.importance, Some(crate::entities::Importance::High));
        assert_eq!(updated.urgency, Some(crate::entities::Urgency::VeryLow));
        assert_eq!(updated.clocks.scheduled, now());
        assert_eq!(updated.clocks.effort, now());
        assert_eq!(updated.clocks.importance, now());
        assert_eq!(updated.clocks.urgency, now());
        // Untouched fields keep their clocks.
        assert_eq!(updated.clocks.title, ts(T0 - 3_600));
        assert_eq!(updated.clocks.duedate, ts(T0 - 3_600));
    }

    #[test]
    fn update_task_clearing_stamps_the_clock() {
        // Clearing is a write (D-S2's {v: null, clock} semantics): a
        // Some(None) slot writes the value None AND advances the clock —
        // regardless of the prior value.
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    scheduled: Some(None),
                    importance: Some(None),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let PersistenceAction::UpsertTask(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.scheduled, None, "cleared");
        assert_eq!(updated.importance, None, "cleared");
        assert_eq!(updated.clocks.scheduled, now());
        assert_eq!(updated.clocks.importance, now());
        // The untouched effort clock stays.
        assert_eq!(updated.clocks.effort, ts(T0 - 3_600));
    }

    #[test]
    fn app_field_edits_enqueue_the_appdata_lane() {
        // Chunk 4's lane reversal: effort/importance/urgency ride
        // `UpdateAppData` — never a native card write.
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    effort: Some(Some(crate::entities::Effort::new(900))),
                    importance: Some(Some(crate::entities::Importance::Normal)),
                    urgency: Some(Some(crate::entities::Urgency::High)),
                    recur: None,

                    ..Default::default()
                },
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        assert_eq!(plan.actions.len(), 2, "upsert + one enqueue");
        assert!(matches!(plan.actions[0], PersistenceAction::UpsertTask(_)));
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id()));
    }

    #[test]
    fn scheduled_is_flag_routed_and_mixed_changesets_enqueue_both_ops() {
        // D-V2's lane rule + D-F3's mixed contract: the unreported
        // board's CONSERVATIVE default routes `scheduled` to the
        // document lane; a native slot beside it adds the `UpdateTask`
        // full-send.
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    scheduled: Some(Some(ts(T0 + 86_400))),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id()));

        // The capability-true board row routes the same edit natively.
        let (mut app, outbox) = base();
        let board_id = BoardId::from(uuid::Uuid::from_u128(u128::from(BOARD)));
        app.sync.get_mut(board_id).unwrap().features = crate::capabilities::DeckFeatures::NATIVE;
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    scheduled: Some(Some(ts(T0 + 86_400))),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("second action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateTask(id) if id == task_id()));

        // Mixed: one native slot + one app slot ⇒ both ops.
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: TaskChanges {
                    title: Some("renamed".into()),
                    effort: Some(Some(crate::entities::Effort::new(5))),
                    ..Default::default()
                },
            },
        )
        .unwrap();
        let ops: Vec<LocalOp> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::EnqueueOp(op) => Some(op.op),
                _ => None,
            })
            .collect();
        assert_eq!(ops.len(), 2, "mixed changeset enqueues both ops");
        assert!(
            ops.iter()
                .any(|op| matches!(op, LocalOp::UpdateTask(id) if *id == task_id()))
        );
        assert!(
            ops.iter()
                .any(|op| matches!(op, LocalOp::UpdateAppData(id) if *id == task_id()))
        );
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
    fn create_stack_lands_on_the_named_board() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateStack {
                board: board_id(),
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
    fn create_stack_refuses_unknown_and_deleted_boards_with_the_named_id() {
        let (app, outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::CreateStack {
                    board: stranger,
                    title: "x".into(),
                    order: 1,
                }
            ),
            Err(CommandError::UnknownBoard(stranger))
        );

        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::CreateStack {
                    board: stranger,
                    title: "x".into(),
                    order: 1,
                }
            ),
            Err(CommandError::BoardDeleted(stranger))
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
                board: board_id(),
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
        let plan = run(
            &app,
            &outbox,
            &StateCommand::RenameBoard {
                board: board_id(),
                title,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn rename_board_refuses_unknown_and_deleted_boards_with_the_named_id() {
        let (app, outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::RenameBoard {
                    board: stranger,
                    title: "x".into(),
                }
            ),
            Err(CommandError::UnknownBoard(stranger))
        );

        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::RenameBoard {
                    board: stranger,
                    title: "x".into(),
                }
            ),
            Err(CommandError::BoardDeleted(stranger))
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
                board: board_id(),
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
    fn assign_and_unassign_stamp_the_membership_state() {
        let (mut app, outbox) = base();
        let fresh = label(503, true);
        let fresh_id = fresh.id;
        app.labels.insert(fresh_id, fresh);
        app.tasks
            .get_mut(&task_id())
            .unwrap()
            .label_memberships
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
        assert!(updated.live_labels().contains(&fresh_id));
        assert_eq!(
            updated.label_memberships[&fresh_id],
            crate::entities::MembershipState::Added { at: now() }
        );
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
        assert!(!updated.live_labels().contains(&label_id()));
        assert_eq!(
            updated.label_memberships[&label_id()],
            crate::entities::MembershipState::Removed { at: now() }
        );
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
            tracking: crate::tracking::TrackingState::default(),
            sync: crate::state::SyncStates::new(),
            last_updated: None,
        };
        // Give the board content so bind tests exercise a non-empty board.
        let stack = stack(901, false);
        app.stacks.insert(stack.id, stack);
        app
    }

    // ---- CaptureTask (#347, D-V5 v2) -------------------------------------

    fn inbox_stack(id: u64, board: BoardId, title: &str, order: i64) -> Stack {
        Stack {
            id: StackId::from(uuid::Uuid::from_u128(u128::from(id))),
            remote: None,
            board,
            title: title.into(),
            order,
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

    fn titled_board(raw: u64, title: &str) -> crate::entities::Board {
        crate::entities::Board {
            id: BoardId::from(uuid::Uuid::from_u128(u128::from(raw))),
            remote: None,
            title: title.into(),
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
        }
    }

    /// The actions' op kinds in order (upserts and enqueues flattened
    /// to a readable shape for the batch-order pins).
    fn action_shapes(plan: &CommandPlan) -> Vec<&'static str> {
        plan.actions
            .iter()
            .map(|a| match a {
                PersistenceAction::UpsertBoard(_) => "board",
                PersistenceAction::UpsertStack(_) => "stack",
                PersistenceAction::UpsertTask(_) => "task",
                PersistenceAction::EnqueueOp(op) => match op.op {
                    LocalOp::CreateStack(_) => "op create-stack",
                    LocalOp::CreateTask(_) => "op create-task",
                    _ => "op other",
                },
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn first_capture_mints_the_unbound_inbox_batch_in_order() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &empty,
            &outbox,
            &StateCommand::CaptureTask {
                title: "milk".into(),
                board: None,
            },
        )
        .unwrap();
        let CommandOutcome::CreatedTask(task_id) = plan.outcome else {
            panic!("wrong outcome");
        };
        assert_eq!(
            action_shapes(&plan),
            vec![
                "board",
                "stack",
                "task",
                "op create-stack",
                "op create-task"
            ],
            "board upsert, stack upsert, task upsert, then the enqueues"
        );
        // The board row is unbound by construction and carries no outbox
        // op of its own.
        let PersistenceAction::UpsertBoard(board) = &plan.actions[0] else {
            panic!("wrong action");
        };
        assert_eq!(board.title, "Inbox");
        assert_eq!(
            board.color,
            crate::entities::Color::new(crate::entities::DEFAULT_BOARD_COLOR)
        );
        assert_eq!(board.remote, None);
        let PersistenceAction::UpsertStack(stack) = &plan.actions[1] else {
            panic!("wrong action");
        };
        assert_eq!(stack.title, "Inbox");
        assert_eq!(stack.order, 0);
        assert_eq!(stack.board, board.id);
        let PersistenceAction::UpsertTask(task) = &plan.actions[2] else {
            panic!("wrong action");
        };
        assert_eq!(task.id, task_id);
        assert_eq!(task.title, "milk");
        assert_eq!(task.stack, stack.id);
        assert_eq!(task.order, 0);
        assert_eq!(task.remote, None);
    }

    #[test]
    fn capture_adopts_the_existing_inbox_and_appends_after_its_last_task() {
        let mut app = AppState::default();
        let board = titled_board(2, "Inbox");
        let stack = inbox_stack(901, board.id, "Inbox", 0);
        let existing = task(31, stack.id, false);
        let mut existing = existing;
        existing.order = 5;
        app.boards.insert(board.id, board);
        app.stacks.insert(stack.id, stack);
        app.tasks.insert(existing.id, existing);
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "second".into(),
                board: None,
            },
        )
        .unwrap();
        assert_eq!(
            action_shapes(&plan),
            vec!["task", "op create-task"],
            "an adopted inbox mints the task only"
        );
        let PersistenceAction::UpsertTask(t) = &plan.actions[0] else {
            panic!("wrong action");
        };
        assert_eq!(t.stack, StackId::from(uuid::Uuid::from_u128(901)));
        assert_eq!(t.order, 6, "after the stack's canonical last order");
    }

    #[test]
    fn capture_with_two_live_inbox_titled_boards_adopts_the_canonical_first() {
        let mut app = AppState::default();
        let first = titled_board(2, "Inbox");
        let second = titled_board(1, "Inbox");
        app.boards.insert(first.id, first.clone());
        app.boards.insert(second.id, second.clone());
        let stack = inbox_stack(901, second.id, "Inbox", 0);
        app.stacks.insert(stack.id, stack.clone());
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "x".into(),
                board: None,
            },
        )
        .unwrap();
        // Board 1 sorts below board 2, so board 1 is the inbox — but it
        // has no stacks, so its landing stack is minted (order 0).
        assert_eq!(
            action_shapes(&plan),
            vec!["task", "op create-task"],
            "the canonical board is adopted, stacks and all"
        );
        let PersistenceAction::UpsertTask(t) = &plan.actions[0] else {
            panic!("wrong action");
        };
        assert_eq!(
            t.stack, stack.id,
            "board 1 sorts below board 2, so its stack is the landing"
        );
        let _ = &first;
    }

    #[test]
    fn board_directed_capture_creates_the_backlog_on_demand() {
        let mut app = AppState::default();
        let board = titled_board(2, "personal");
        app.boards.insert(board.id, board);
        let outbox: Vec<PendingOp> = Vec::new();
        let board_id = BoardId::from(uuid::Uuid::from_u128(2));
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "x".into(),
                board: Some(board_id),
            },
        )
        .unwrap();
        assert_eq!(
            action_shapes(&plan),
            vec!["stack", "task", "op create-stack", "op create-task"]
        );
        let PersistenceAction::UpsertStack(s) = &plan.actions[0] else {
            panic!("wrong action");
        };
        assert_eq!(s.title, "Backlog");
        assert_eq!(s.order, 0, "an empty board appends at 0");
    }

    #[test]
    fn board_directed_capture_lands_canonical_first_among_backlogs() {
        let mut app = AppState::default();
        let board = titled_board(2, "personal");
        let late = inbox_stack(902, board.id, "Backlog", 4);
        let early = inbox_stack(901, board.id, "Backlog", 3);
        app.boards.insert(board.id, board);
        app.stacks.insert(late.id, late);
        app.stacks.insert(early.id, early);
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "x".into(),
                board: Some(BoardId::from(uuid::Uuid::from_u128(2))),
            },
        )
        .unwrap();
        assert_eq!(
            action_shapes(&plan),
            vec!["task", "op create-task"],
            "an existing Backlog is adopted, canonical-first"
        );
        let PersistenceAction::UpsertTask(t) = &plan.actions[0] else {
            panic!("wrong action");
        };
        assert_eq!(t.stack, StackId::from(uuid::Uuid::from_u128(901)));
    }

    #[test]
    fn board_directed_capture_rejects_unknown_and_deleted_boards_typed() {
        let mut app = AppState::default();
        let dead = titled_board(3, "old");
        dead.clone().deleted = true;
        let dead = crate::entities::Board {
            deleted: true,
            ..titled_board(3, "old")
        };
        app.boards.insert(dead.id, dead);
        let outbox: Vec<PendingOp> = Vec::new();
        let unknown = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "x".into(),
                board: Some(BoardId::from(uuid::Uuid::from_u128(4242))),
            },
        )
        .unwrap_err();
        assert!(matches!(unknown, CommandError::UnknownBoard(_)));
        let deleted = run(
            &app,
            &outbox,
            &StateCommand::CaptureTask {
                title: "x".into(),
                board: Some(BoardId::from(uuid::Uuid::from_u128(3))),
            },
        )
        .unwrap_err();
        assert!(matches!(deleted, CommandError::BoardDeleted(_)));
    }

    #[test]
    fn capture_accepts_any_title_verbatim() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        for title in ["", "   ", "  padded  "] {
            let plan = run(
                &empty,
                &outbox,
                &StateCommand::CaptureTask {
                    title: title.into(),
                    board: None,
                },
            )
            .unwrap();
            let PersistenceAction::UpsertTask(t) = &plan.actions[2] else {
                panic!("wrong action");
            };
            assert_eq!(&t.title, title, "the engine never rewrites titles");
        }
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
    fn create_board_always_mints_and_titles_are_not_unique_keys() {
        // The refusal lift: with a live board present a second CreateBoard
        // still mints a distinct id — even for an identical title.
        let (app, outbox) = base();
        let ids = CountingIds::default();
        let command = StateCommand::CreateBoard {
            title: "second".into(),
            color: crate::entities::Color::new("000000"),
        };
        let first = plan_command(
            &app,
            &outbox,
            &command,
            &ids,
            &crate::reminders::ReminderDefaults::default(),
            now(),
        )
        .unwrap();
        let CommandOutcome::CreatedBoard(first_id) = first.outcome else {
            panic!("wrong outcome");
        };

        let mut grown = app.clone();
        grown.boards.insert(
            first_id,
            crate::entities::Board {
                id: first_id,
                deleted: false,
                ..grown.boards[&board_id()].clone()
            },
        );
        let second = plan_command(
            &grown,
            &outbox,
            &command,
            &ids,
            &crate::reminders::ReminderDefaults::default(),
            now(),
        )
        .unwrap();
        let CommandOutcome::CreatedBoard(second_id) = second.outcome else {
            panic!("wrong outcome");
        };
        assert_ne!(second_id, first_id, "always mints a distinct board");
        assert_eq!(
            second.actions.len(),
            1,
            "still no outbox op for a local board creation"
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
                board: board_id(),
                remote: crate::ids::RemoteBoardId(42),
                connector: crate::test_support::test_connector(),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Bound { wiped_refs: 0 });
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert_eq!(
            updated.remote,
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42))
        );
        assert_eq!(plan.actions.len(), 1, "bind carries no outbox op");
    }

    #[test]
    fn bind_board_mints_the_binding_with_the_commands_connector_tag() {
        // The mint source is the command (D6-1): the tag it carries is
        // the tag the binding gets. The legacy-world corpus tag keeps
        // the old fixture's exact binding value.
        let app = unbound_board_state();
        let plan = run(
            &app,
            &[],
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(42),
                connector: crate::test_support::test_connector(),
            },
        )
        .unwrap();
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert_eq!(
            updated.remote,
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42))
        );
    }

    #[test]
    fn bind_board_mints_a_non_default_connector_tag_verbatim() {
        // The engine is config-blind (the topology rule): a non-default
        // tag rides through unchanged — the move-board surface (D5-18's
        // deferred chunk-6 motion).
        let app = unbound_board_state();
        let custom = crate::ids::ConnectorId::from("work");
        let plan = plan_command(
            &app,
            &[],
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(42),
                connector: custom.clone(),
            },
            &CountingIds::default(),
            &crate::reminders::ReminderDefaults::default(),
            now(),
        )
        .unwrap();
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the board upsert");
        };
        assert_eq!(
            updated.remote,
            Some(crate::ids::RemoteBoardBinding {
                connector: custom,
                board: crate::ids::RemoteBoardId(42),
            })
        );
    }

    #[test]
    fn same_remote_id_through_a_different_connector_refuses_board_already_bound() {
        // Binding-scoped equality (D6-2): remote 42 via beta is a
        // genuinely different remote board than remote 42 via alpha —
        // a board holding the alpha binding cannot be re-bound onto the
        // beta remote as if it were the same remote.
        let mut app = unbound_board_state();
        app.boards.get_mut(&board_id()).unwrap().remote =
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        assert_eq!(
            run(
                &app,
                &[],
                &StateCommand::BindBoard {
                    board: board_id(),
                    remote: crate::ids::RemoteBoardId(42),
                    connector: crate::ids::ConnectorId::from("beta"),
                },
            ),
            Err(CommandError::BoardAlreadyBound {
                board: board_id(),
                bound: crate::ids::RemoteBoardId(42),
            })
        );
    }

    #[test]
    fn cross_connector_remote_id_does_not_collide() {
        // The RemoteBoardInUse guard scopes to the connector (D6-2):
        // another board holding (alpha, 42) does not block a bind of
        // (beta, 42) — the numeric ids name different remote boards.
        let mut app = unbound_board_state();
        let other = BoardId::from(uuid::Uuid::from_u128(801));
        let mut holder = app.boards[&board_id()].clone();
        holder.id = other;
        holder.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        app.boards.insert(other, holder);
        let plan = run(
            &app,
            &[],
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(42),
                connector: crate::ids::ConnectorId::from("beta"),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Bound { wiped_refs: 0 });
    }

    #[test]
    fn same_binding_held_by_another_board_still_refuses_remote_board_in_use() {
        // The cross-board guard stays binding-scoped: exactly
        // (connector, remote) held by another live board refuses.
        let mut app = unbound_board_state();
        let other = BoardId::from(uuid::Uuid::from_u128(802));
        let mut holder = app.boards[&board_id()].clone();
        holder.id = other;
        holder.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        app.boards.insert(other, holder);
        assert_eq!(
            run(
                &app,
                &[],
                &StateCommand::BindBoard {
                    board: board_id(),
                    remote: crate::ids::RemoteBoardId(42),
                    connector: crate::test_support::test_connector(),
                },
            ),
            Err(CommandError::RemoteBoardInUse {
                remote: crate::ids::RemoteBoardId(42),
                board: other,
            })
        );
    }

    #[test]
    fn same_remote_rebind_is_an_accepted_no_op_under_the_binding_type() {
        let mut app = unbound_board_state();
        app.boards.get_mut(&board_id()).unwrap().remote =
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        assert_eq!(
            run(
                &app,
                &[],
                &StateCommand::BindBoard {
                    board: board_id(),
                    remote: crate::ids::RemoteBoardId(42),
                    connector: crate::test_support::test_connector(),
                },
            )
            .unwrap()
            .outcome,
            CommandOutcome::NoOp
        );
    }

    #[test]
    fn bind_remote_held_by_another_board_refuses_remote_board_in_use() {
        let mut app = unbound_board_state();
        let other_raw = 801_u64;
        let other = BoardId::from(uuid::Uuid::from_u128(u128::from(other_raw)));
        let mut holder = app.boards[&board_id()].clone();
        holder.id = other;
        holder.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        app.boards.insert(other, holder);
        assert_eq!(
            run(
                &app,
                &[],
                &StateCommand::BindBoard {
                    board: board_id(),
                    remote: crate::ids::RemoteBoardId(42),
                    connector: crate::test_support::test_connector(),
                }
            ),
            Err(CommandError::RemoteBoardInUse {
                remote: crate::ids::RemoteBoardId(42),
                board: other,
            })
        );
    }

    /// A populated `RemoteTaskBaseline` for the bind-wipe fixtures: the
    /// shape an R1 observation leaves on a bound task.
    fn observed_baseline() -> crate::entities::RemoteTaskBaseline {
        crate::entities::RemoteTaskBaseline {
            title: "observed".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: crate::ids::RemoteStackId(5),
            order: 1,
            labels: std::collections::BTreeSet::new(),
            archived: false,
        }
    }

    #[test]
    fn bind_wipe_clears_only_foreign_refs() {
        let mut app = unbound_board_state();
        let stack = stack(901, false);
        let board_stack = stack.id;
        app.stacks.insert(board_stack, stack);
        // Live descendants: one carrying an R1 ref (wipes), one already
        // naming the bind target R2 (stays), one unbound (stays).
        let mut foreign = task(2, board_stack, true);
        foreign.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(1),
            stack: crate::ids::RemoteStackId(5),
            card: crate::ids::RemoteCardId(2),
        });
        foreign.remote_seen = Some(chrono::DateTime::from_timestamp(1_000, 0).expect("ts"));
        foreign.remote_baseline = Some(observed_baseline());
        let foreign_id = foreign.id;
        app.tasks.insert(foreign_id, foreign);
        let mut already = task(3, board_stack, true);
        already.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(2),
            stack: crate::ids::RemoteStackId(5),
            card: crate::ids::RemoteCardId(3),
        });
        already.remote_baseline = Some(observed_baseline());
        let already_id = already.id;
        app.tasks.insert(already_id, already);
        let local_only = task(4, board_stack, false);
        let local_id = local_only.id;
        app.tasks.insert(local_id, local_only);
        // A tombstoned descendant keeps its ref (the R9c finalize needs it).
        let mut dead = task(5, board_stack, true);
        dead.deleted = true;
        dead.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(1),
            stack: crate::ids::RemoteStackId(5),
            card: crate::ids::RemoteCardId(5),
        });
        let dead_id = dead.id;
        app.tasks.insert(dead_id, dead);

        let plan = run(
            &app,
            &[],
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(2),
                connector: crate::test_support::test_connector(),
            },
        )
        .unwrap();
        let cleared: Vec<_> = plan
            .actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::UpsertTask(t) => Some((t.id, t.remote)),
                _ => None,
            })
            .collect();
        assert!(
            cleared.contains(&(foreign_id, None)),
            "the foreign ref wipes"
        );
        // The wipe is a binding clear: the observed remote snapshot goes
        // with it (ADR 0004's remote baseline).
        assert!(
            plan.actions.iter().any(|a| matches!(
                a,
                PersistenceAction::UpsertTask(t)
                    if t.id == foreign_id && t.remote_baseline.is_none()
            )),
            "the wiped row's baseline clears with the binding"
        );
        assert!(
            !cleared.contains(&(already_id, None)),
            "a ref already naming the bind target stays"
        );
        assert!(
            !cleared.contains(&(local_id, None)),
            "an unbound entity stays"
        );
        assert!(
            !plan
                .actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.id == dead_id)),
            "a tombstoned descendant keeps its ref (R9c finalize)"
        );
        let PersistenceAction::UpsertBoard(board_row) = plan
            .actions
            .iter()
            .find(|a| matches!(a, PersistenceAction::UpsertBoard(_)))
            .expect("board row upserted")
        else {
            unreachable!("filtered above");
        };
        assert_eq!(
            board_row.remote,
            crate::test_support::test_binding(crate::ids::RemoteBoardId(2))
        );
    }

    #[test]
    fn bind_outcome_counts_wiped_refs() {
        let mut app = unbound_board_state();
        let stack = stack(901, false);
        let board_stack = stack.id;
        app.stacks.insert(board_stack, stack);
        let mut foreign = task(2, board_stack, true);
        foreign.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(1),
            stack: crate::ids::RemoteStackId(5),
            card: crate::ids::RemoteCardId(2),
        });
        app.tasks.insert(foreign.id, foreign);
        let plan = run(
            &app,
            &[],
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(9),
                connector: crate::test_support::test_connector(),
            },
        )
        .unwrap();
        assert_eq!(
            plan.outcome,
            CommandOutcome::Bound { wiped_refs: 1 },
            "the outcome counts exactly the cleared refs"
        );
    }

    #[test]
    fn bind_wipe_forecast_counts_stranded_update_ops() {
        let mut app = unbound_board_state();
        let stack = stack(901, false);
        let board_stack = stack.id;
        app.stacks.insert(board_stack, stack);
        let mut foreign = task(2, board_stack, true);
        foreign.remote = Some(crate::ids::RemoteCardRef {
            board: crate::ids::RemoteBoardId(1),
            stack: crate::ids::RemoteStackId(5),
            card: crate::ids::RemoteCardId(2),
        });
        let foreign_id = foreign.id;
        app.tasks.insert(foreign.id, foreign);
        let create = op_for(11, LocalOp::CreateTask(foreign_id));
        let update = op_for(12, LocalOp::UpdateTask(foreign_id));
        let outbox = vec![create, update];
        assert_eq!(
            bind_wipe_forecast(&app, &outbox, board_id(), crate::ids::RemoteBoardId(9)),
            1,
            "only the update-class op on a to-be-wiped entity forecasts"
        );
        assert_eq!(
            bind_wipe_count(&app, board_id(), crate::ids::RemoteBoardId(9)),
            1,
            "the wipe count names the one foreign ref"
        );
    }

    #[test]
    fn bind_board_refuses_an_unknown_board_with_the_named_id() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &empty,
                &outbox,
                &StateCommand::BindBoard {
                    board: stranger,
                    remote: crate::ids::RemoteBoardId(1),
                    connector: crate::test_support::test_connector(),
                }
            ),
            Err(CommandError::UnknownBoard(stranger))
        );
    }

    #[test]
    fn bind_repeat_to_the_same_remote_is_a_no_op_but_a_new_remote_is_rejected() {
        let outbox: Vec<PendingOp> = Vec::new();
        let bound = {
            let mut app = unbound_board_state();
            app.boards.get_mut(&board_id()).unwrap().remote =
                crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
            app
        };
        let plan = run(
            &bound,
            &outbox,
            &StateCommand::BindBoard {
                board: board_id(),
                remote: crate::ids::RemoteBoardId(42),
                connector: crate::test_support::test_connector(),
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
        assert_eq!(
            run(
                &bound,
                &outbox,
                &StateCommand::BindBoard {
                    board: board_id(),
                    remote: crate::ids::RemoteBoardId(43),
                    connector: crate::test_support::test_connector(),
                }
            ),
            Err(CommandError::BoardAlreadyBound {
                board: board_id(),
                bound: crate::ids::RemoteBoardId(42)
            })
        );
    }

    // ---- UnbindBoard --------------------------------------------------

    #[test]
    fn unbind_unknown_board_refuses() {
        let empty = AppState::default();
        let outbox: Vec<PendingOp> = Vec::new();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &empty,
                &outbox,
                &StateCommand::UnbindBoard { board: stranger }
            ),
            Err(CommandError::UnknownBoard(stranger))
        );
    }

    #[test]
    fn unbind_tombstoned_board_refuses() {
        let outbox: Vec<PendingOp> = Vec::new();
        let mut dead = unbound_board_state();
        let board = dead.boards.get_mut(&board_id()).unwrap();
        board.deleted = true;
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::UnbindBoard { board: board_id() }
            ),
            Err(CommandError::BoardDeleted(board_id()))
        );
    }

    #[test]
    fn unbind_unbound_board_is_accepted_noop() {
        let app = unbound_board_state();
        let outbox: Vec<PendingOp> = Vec::new();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UnbindBoard { board: board_id() },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn unbind_clears_binding_and_drops_status_row() {
        let mut app = unbound_board_state();
        app.boards.get_mut(&board_id()).unwrap().remote =
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        // Two ops attributed to the board (stack op + board op), one
        // other entity op that attributes nowhere near it.
        let stack = app.stacks.values().next().unwrap().id;
        let outbox = vec![
            op_for(11, LocalOp::RenameStack(stack)),
            op_for(12, LocalOp::UpdateBoard(board_id())),
            op_for(13, LocalOp::UpdateTask(task_id())),
        ];
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UnbindBoard { board: board_id() },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Unbound { stranded_ops: 2 });
        assert_eq!(
            plan.actions
                .iter()
                .filter(|a| matches!(a, PersistenceAction::UpsertBoard(b) if b.remote.is_none() && b.id == board_id()))
                .count(),
            1,
            "the board row is upserted with the binding cleared"
        );
        assert!(
            plan.actions
                .contains(&PersistenceAction::DeleteSyncStatus(board_id())),
            "the board's sync-status row drops with the binding"
        );
    }

    #[test]
    fn unbind_leaves_outbox_and_descendant_refs_untouched() {
        let mut app = unbound_board_state();
        app.boards.get_mut(&board_id()).unwrap().remote =
            crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        let stack = app.stacks.values().next().unwrap().id;
        let outbox = vec![op_for(11, LocalOp::RenameStack(stack))];
        let plan = run(
            &app,
            &outbox,
            &StateCommand::UnbindBoard { board: board_id() },
        )
        .unwrap();
        assert!(
            !plan.actions.iter().any(|a| matches!(
                a,
                PersistenceAction::EnqueueOp(_) | PersistenceAction::FailOp(_)
            )),
            "unbind never touches the outbox (honest stranding)"
        );
        assert!(
            !plan.actions.iter().any(|a| matches!(
                a,
                PersistenceAction::UpsertStack(_)
                    | PersistenceAction::UpsertTask(_)
                    | PersistenceAction::UpsertLabel(_)
            )),
            "unbind never touches descendant rows (same-remote rebind re-adopts by them)"
        );
    }

    // ---- Board color / archive / delete / reset -------------------------

    #[test]
    fn board_scoped_intents_touch_only_the_named_board() {
        // The multi-board pin: with two live boards seeded, a board-scoped
        // command lands its effect on the named board and leaves the other
        // byte-untouched.
        let (mut app, outbox) = base();
        let other = board(801, true);
        let other_id = other.id;
        app.boards.insert(other_id, other);

        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateStack {
                board: other_id,
                title: "other column".into(),
                order: 1,
            },
        )
        .unwrap();
        let PersistenceAction::UpsertStack(created) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(created.board, other_id);

        let plan = run(
            &app,
            &outbox,
            &StateCommand::RenameBoard {
                board: other_id,
                title: "renamed".into(),
            },
        )
        .unwrap();
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.id, other_id);

        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardColor {
                board: board_id(),
                color: crate::entities::Color::new("123456"),
            },
        )
        .unwrap();
        let PersistenceAction::UpsertBoard(updated) = &plan.actions[0] else {
            panic!("first action must be the upsert");
        };
        assert_eq!(updated.id, board_id());
        assert_eq!(app.boards[&other_id].title, "board 801", "untouched");
    }

    #[test]
    fn set_board_color_stamps_only_the_color_clock_and_enqueues() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardColor {
                board: board_id(),
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
        let color = app.boards[&board_id()].color.clone();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardColor {
                board: board_id(),
                color,
            },
        )
        .unwrap();
        assert_eq!(plan, no_op());
    }

    #[test]
    fn set_board_color_refuses_unknown_and_deleted_boards_with_the_named_id() {
        let (app, outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::SetBoardColor {
                    board: stranger,
                    color: crate::entities::Color::new("00ff00"),
                }
            ),
            Err(CommandError::UnknownBoard(stranger))
        );

        // A tombstoned board is not commandable either (the
        // edit-against-tombstone class).
        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::SetBoardColor {
                    board: stranger,
                    color: crate::entities::Color::new("00ff00"),
                }
            ),
            Err(CommandError::BoardDeleted(stranger))
        );
    }

    #[test]
    fn set_board_archived_stamps_the_archived_clock_both_directions() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetBoardArchived {
                board: board_id(),
                archived: true,
            },
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
        let board_id = board_id();
        let mut board = archived.boards[&board_id].clone();
        board.archived = true;
        board.clocks.archived = ts(T0 - 60);
        archived.boards.insert(board_id, board);
        let plan = run(
            &archived,
            &outbox,
            &StateCommand::SetBoardArchived {
                board: board_id,
                archived: false,
            },
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
                &StateCommand::SetBoardArchived {
                    board: board_id,
                    archived: true
                }
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
                board: board_id,
                color: crate::entities::Color::new("123456"),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one cascade table; splitting hides the shape
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

        let plan = run(
            &state,
            &outbox,
            &StateCommand::DeleteBoard { board: board_id() },
        )
        .unwrap();
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
    fn delete_board_repeat_is_a_no_op_and_absence_is_unknown_board() {
        let (mut app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DeleteBoard { board: board_id() },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);

        let board_id = board_id();
        let mut board = app.boards[&board_id].clone();
        board.deleted = true;
        board.clocks.deleted = ts(T0 - 30);
        app.boards.insert(board_id, board);
        assert_eq!(
            run(
                &app,
                &outbox,
                &StateCommand::DeleteBoard { board: board_id }
            ),
            Ok(no_op())
        );

        let mut empty = app;
        empty.boards.clear();
        assert_eq!(
            run(
                &empty,
                &outbox,
                &StateCommand::DeleteBoard { board: board_id }
            ),
            Err(CommandError::UnknownBoard(board_id))
        );
    }

    #[test]
    fn delete_board_on_an_unbound_board_still_queues_the_op() {
        let app = unbound_board_state();
        let board_id = board_id();
        let plan = run(&app, &[], &StateCommand::DeleteBoard { board: board_id }).unwrap();
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
        let plan = run(
            &state,
            &outbox,
            &StateCommand::ResetBoard { board: board_id },
        )
        .unwrap();
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
                    remote: crate::test_support::test_binding(crate::ids::RemoteBoardId(77)),
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
            tracking: crate::tracking::TrackingState::default(),
            sync: crate::state::SyncStates::new(),
            last_updated: None,
        };
        assert_eq!(
            run(&app, &[], &StateCommand::ResetBoard { board: board_id }),
            Ok(no_op())
        );
    }

    #[test]
    fn reset_board_refuses_unknown_and_deleted_boards_with_the_named_id() {
        let (app, outbox) = base();
        let stranger = BoardId::from(uuid::Uuid::from_u128(4242));
        assert_eq!(
            run(&app, &outbox, &StateCommand::ResetBoard { board: stranger }),
            Err(CommandError::UnknownBoard(stranger))
        );

        let mut dead = app.clone();
        dead.boards.insert(
            stranger,
            crate::entities::Board {
                id: stranger,
                deleted: true,
                ..dead.boards[&board_id()].clone()
            },
        );
        assert_eq!(
            run(
                &dead,
                &outbox,
                &StateCommand::ResetBoard { board: stranger }
            ),
            Err(CommandError::BoardDeleted(stranger))
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
            updated_at: now(),
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

    // ---- AddReminderRule / RemoveReminderRule / ClearReminderRules -----

    fn offset_before(secs: i64) -> crate::reminders::ReminderOffset {
        crate::reminders::ReminderOffset::before(
            chrono::TimeDelta::try_seconds(-secs).expect("valid offset"),
        )
        .expect("negative duration is a legal offset")
    }

    fn add_at(secs: i64) -> StateCommand {
        StateCommand::AddReminderRule {
            task: task_id(),
            offset: offset_before(secs),
        }
    }

    #[test]
    fn add_rule_on_a_rule_less_task_plans_mint_and_one_enqueue() {
        // The first-remind flow: mint + upsert + exactly one enqueue —
        // byte-shape-identical to what the retired replace path planned
        // on the same rule-less fixture (the identity pin, snapshot-proven
        // by s10's verb swap with no diff).
        let (app, outbox) = base();
        let add = run(&app, &outbox, &add_at(600)).unwrap();
        let CommandOutcome::ReminderRuleAdded { rule, added: true } = add.outcome else {
            panic!("expected ReminderRuleAdded, got {:?}", add.outcome);
        };
        assert_eq!(add.actions.len(), 2, "upsert + exactly one enqueue");
        let PersistenceAction::UpsertReminderRule(stored) = &add.actions[0] else {
            panic!("expected UpsertReminderRule, got {:?}", add.actions);
        };
        assert_eq!(stored.id, rule);
        assert_eq!(stored.task, task_id());
        assert_eq!(stored.offset, offset_before(600));
        assert_eq!(
            stored.created_at,
            now(),
            "the rule stamp is the injected now"
        );
        assert_eq!(stored.updated_at, stored.created_at, "creation stamps both");
        let PersistenceAction::EnqueueOp(op) = add.actions.last().expect("enqueue") else {
            panic!("last action must be the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id()));
    }

    #[test]
    fn add_rule_at_an_existing_offset_plans_nothing_and_names_the_survivor() {
        let (mut app, outbox) = base();
        let mut own = reminder_rule(61, task_id());
        own.offset = offset_before(300);
        let mut foreign = reminder_rule(63, TaskId::from(uuid::Uuid::from_u128(4245)));
        foreign.offset = offset_before(300);
        with_rules(&mut app, &[own.clone(), foreign]);
        let plan = run(&app, &outbox, &add_at(300)).unwrap();
        assert_eq!(
            plan.outcome,
            CommandOutcome::ReminderRuleAdded {
                rule: own.id,
                added: false
            }
        );
        assert!(plan.actions.is_empty(), "the dedup hit plans zero actions");
    }

    #[test]
    fn add_rule_at_a_second_offset_grows_the_rule_set() {
        let (mut app, outbox) = base();
        let mut own = reminder_rule(61, task_id());
        own.offset = offset_before(300);
        with_rules(&mut app, &[own]);
        let plan = run(&app, &outbox, &add_at(600)).unwrap();
        let CommandOutcome::ReminderRuleAdded { added: true, .. } = plan.outcome else {
            panic!("expected an add, got {:?}", plan.outcome);
        };
        assert_eq!(plan.actions.len(), 2, "upsert + exactly one enqueue");
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("expected the enqueue");
        };
        assert!(matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id()));
    }

    #[test]
    fn add_rule_rejects_unknown_and_deleted_tasks_before_any_dedup() {
        let (mut app, outbox) = base();
        let mut own = reminder_rule(61, task_id());
        own.offset = offset_before(60);
        with_rules(&mut app, &[own]);
        let stranger = TaskId::from(uuid::Uuid::from_u128(4245));
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::AddReminderRule {
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
            run(&dead, &outbox, &add_at(60)),
            Err(CommandError::TaskDeleted(_))
        ));
    }

    #[test]
    fn remove_unknown_rule_is_typed() {
        let (app, outbox) = base();
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::RemoveReminderRule { rule: rule_id(99) }
            ),
            Err(CommandError::UnknownReminderRule(_))
        ));
    }

    #[test]
    fn remove_rule_plans_the_delete_and_one_enqueue() {
        let (mut app, outbox) = base();
        let own = reminder_rule(61, task_id());
        let foreign = reminder_rule(63, TaskId::from(uuid::Uuid::from_u128(4245)));
        with_rules(&mut app, &[own.clone(), foreign.clone()]);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::RemoveReminderRule { rule: own.id },
        )
        .unwrap();
        assert_eq!(
            plan.outcome,
            CommandOutcome::ReminderRuleRemoved { rule: own.id }
        );
        assert_eq!(plan.actions.len(), 2, "one delete + exactly one enqueue");
        assert_eq!(
            plan.actions[0],
            PersistenceAction::DeleteReminderRule(own.id)
        );
        let PersistenceAction::EnqueueOp(op) = &plan.actions[1] else {
            panic!("expected the enqueue, got {:?}", plan.actions);
        };
        assert!(matches!(op.op, LocalOp::UpdateAppData(id) if id == task_id()));
        assert!(app.reminders.rules.contains_key(&foreign.id));
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(64))]

        #[test]
        fn add_dedup_is_idempotent_per_offset(
            offsets in proptest::collection::vec(1_i64..=86_400, 1..8),
        ) {
            let (mut app, outbox) = base();
            // Distinct rule ids ascending with insertion order: the dedup
            // survivor at an offset is the id-lowest rule holding it.
            let mut survivor_at: std::collections::BTreeMap<i64, crate::ids::ReminderRuleId> =
                std::collections::BTreeMap::new();
            for (i, secs) in offsets.iter().enumerate() {
                let mut rule = reminder_rule(i as u64 + 1, task_id());
                rule.offset = offset_before(*secs);
                survivor_at.entry(*secs).or_insert(rule.id);
                app.reminders.rules.insert(rule.id, rule);
            }
            for (secs, survivor) in &survivor_at {
                let plan = run(&app, &outbox, &add_at(*secs)).unwrap();
                prop_assert!(plan.actions.is_empty(), "a repeat add plans nothing");
                prop_assert_eq!(
                    plan.outcome,
                    CommandOutcome::ReminderRuleAdded {
                        rule: *survivor,
                        added: false
                    }
                );
            }
        }
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

    // ---- Adhoc alarm verbs (chunk-7) -----------------------------------

    fn alarm(raw: u128, fire_at: chrono::DateTime<Utc>) -> crate::reminders::AdhocAlarm {
        crate::reminders::AdhocAlarm {
            id: crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(raw)),
            label: "steep the tea".into(),
            fire_at,
            created_at: now(),
        }
    }

    #[test]
    fn create_adhoc_alarm_in_stamps_and_never_enqueues() {
        let (app, outbox) = base();
        let dur = chrono::TimeDelta::try_minutes(5).unwrap();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateAdhocAlarm {
                label: "tea".into(),
                when: crate::reminders::AdhocWhen::In(dur),
            },
        )
        .expect("accepted");
        // Exactly one action, the upsert — device-local (no outbox op,
        // no document-lane trigger).
        assert_eq!(plan.actions.len(), 1);
        let PersistenceAction::UpsertAdhocAlarm(alarm) = &plan.actions[0] else {
            panic!("the upsert is the only action");
        };
        assert_eq!(alarm.fire_at, ts(T0) + dur, "engine-stamped now + dur");
        assert_eq!(alarm.created_at, ts(T0));
        assert_eq!(alarm.label, "tea");
        assert_eq!(
            plan.outcome,
            CommandOutcome::AdhocAlarmCreated { alarm: alarm.id }
        );
    }

    #[test]
    fn create_adhoc_alarm_in_saturates_an_unrepresentable_target() {
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateAdhocAlarm {
                label: "tea".into(),
                when: crate::reminders::AdhocWhen::In(chrono::TimeDelta::MAX),
            },
        )
        .expect("accepted");
        let PersistenceAction::UpsertAdhocAlarm(alarm) = &plan.actions[0] else {
            panic!("the upsert is the only action");
        };
        assert_eq!(
            alarm.fire_at,
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            "unrepresentable target degrades to the ceiling, never panics"
        );
    }

    #[test]
    fn create_adhoc_alarm_at_is_verbatim_and_past_is_legal() {
        let (app, outbox) = base();
        let past = ts(T0 - 3_600);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CreateAdhocAlarm {
                label: "tea".into(),
                when: crate::reminders::AdhocWhen::At(past),
            },
        )
        .expect("a past instant is legal data: it fires on the next tick");
        let PersistenceAction::UpsertAdhocAlarm(alarm) = &plan.actions[0] else {
            panic!("the upsert is the only action");
        };
        assert_eq!(alarm.fire_at, past, "At is verbatim, never re-stamped");
    }

    #[test]
    fn create_adhoc_alarm_rejects_non_positive_in_typed() {
        let (app, outbox) = base();
        for secs in [0_i64, -1, -600] {
            let err = run(
                &app,
                &outbox,
                &StateCommand::CreateAdhocAlarm {
                    label: "tea".into(),
                    when: crate::reminders::AdhocWhen::In(
                        chrono::TimeDelta::try_seconds(secs).unwrap(),
                    ),
                },
            )
            .expect_err("dur <= 0 is a typed rejection");
            assert!(
                matches!(err, CommandError::InvalidAdhocWhen),
                "{secs}s must be InvalidAdhocWhen"
            );
        }
    }

    #[test]
    fn cancel_adhoc_alarm_deletes_and_a_repeat_is_typed() {
        let (app, outbox) = base();
        let unknown = crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(4242));
        let err = run(
            &app,
            &outbox,
            &StateCommand::CancelAdhocAlarm { alarm: unknown },
        )
        .expect_err("unknown id is a typed rejection");
        assert!(matches!(err, CommandError::UnknownAdhocAlarm(id) if id == unknown));

        let a = alarm(20, ts(T0 + 60));
        let mut app = app;
        app.reminders.alarms.insert(a.id, a.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::CancelAdhocAlarm { alarm: a.id },
        )
        .expect("accepted");
        assert_eq!(
            plan.actions,
            vec![PersistenceAction::DeleteAdhocAlarm(a.id)],
            "exactly the delete — the persistence cascade prunes dependents"
        );
        assert_eq!(
            plan.outcome,
            CommandOutcome::AdhocAlarmCancelled { alarm: a.id }
        );
    }

    #[test]
    fn snooze_adhoc_alarm_guards_existence_without_live_task() {
        let (app, outbox) = base();
        let unknown = crate::ids::AdhocAlarmId::from(uuid::Uuid::from_u128(4243));
        let err = run(
            &app,
            &outbox,
            &StateCommand::SnoozeReminder {
                reminder: crate::reminders::ReminderRef::Adhoc(unknown),
                occurrence: ts(T0),
                postpone: chrono::TimeDelta::try_minutes(1).unwrap(),
            },
        )
        .expect_err("unknown alarm is a typed rejection");
        assert!(matches!(err, CommandError::UnknownAdhocAlarm(id) if id == unknown));

        // An alarm is not a task: no liveness states, so the snooze
        // applies with no `live_task` involvement.
        let a = alarm(21, ts(T0 - 60));
        let mut app = app;
        app.reminders.alarms.insert(a.id, a);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::SnoozeReminder {
                reminder: crate::reminders::ReminderRef::Adhoc(crate::ids::AdhocAlarmId::from(
                    uuid::Uuid::from_u128(21),
                )),
                occurrence: ts(T0 - 60),
                postpone: chrono::TimeDelta::try_minutes(1).unwrap(),
            },
        )
        .expect("accepted");
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.outcome, CommandOutcome::Applied);
    }

    #[test]
    fn clear_board_probe_ignores_alarms_they_are_device_global() {
        let (app, outbox) = base();
        // Empty content + alarm rows and their facts: the repair has
        // nothing to do — alarms are device-global, not board content.
        let mut app = app;
        app.stacks.clear();
        app.tasks.clear();
        app.labels.clear();
        let a = alarm(22, ts(T0 - 60));
        let reminder = crate::reminders::ReminderRef::Adhoc(a.id);
        app.reminders.alarms.insert(a.id, a);
        app.reminders
            .deliveries
            .insert(crate::reminders::ReminderDelivery {
                reminder,
                occurrence: ts(T0 - 60),
                delivered_at: ts(T0),
            });
        app.reminders.snoozes.insert(
            crate::reminders::ReminderSnoozeKey {
                reminder,
                occurrence: ts(T0 - 60),
            },
            ts(T0 + 60),
        );
        let plan = run(
            &app,
            &outbox,
            &StateCommand::ResetBoard { board: board_id() },
        )
        .expect("planned");
        assert_eq!(
            plan.outcome,
            CommandOutcome::NoOp,
            "alarms never count as board content"
        );
        assert_eq!(plan.actions, Vec::new());
    }
    // ---- Tracking verbs (DR-5/1) ----------------------------------------

    use crate::tracking::TrackingState;

    /// Fixture time-entry id: high raw values never collide with the
    /// fresh ids `run()`'s `CountingIds` mints (uuid 1, 2, ...).
    fn entry_id(raw: u64) -> crate::ids::TimeEntryId {
        crate::ids::TimeEntryId::from(uuid::Uuid::from_u128(u128::from(7_000 + raw)))
    }

    fn tracked_entry(raw: u64, task_id: TaskId, start: i64, end: Option<i64>) -> TimeEntry {
        TimeEntry {
            id: entry_id(raw),
            task: task_id,
            task_title: "frozen title".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(T0 - 120),
            updated_at: ts(T0 - 120),
        }
    }

    /// Seeds one entry into the fixture state's tracking map.
    fn with_entry(mut app: AppState, entry: TimeEntry) -> AppState {
        app.tracking.entries.insert(entry.id, entry);
        app
    }

    fn task_b() -> TaskId {
        TaskId::from(uuid::Uuid::from_u128(2))
    }

    /// The fixture's second live task (on the fixture stack, bound).
    fn base_with_task_b() -> (AppState, Vec<PendingOp>) {
        let (mut app, outbox) = base();
        let second = task(2, stack_id(), true);
        app.tasks.insert(second.id, second);
        (app, outbox)
    }

    fn entry_upserts(plan: &CommandPlan) -> Vec<TimeEntry> {
        plan.actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::UpsertTimeEntry(entry) => Some(entry.clone()),
                _ => None,
            })
            .collect()
    }

    fn flush_targets(plan: &CommandPlan) -> Vec<TaskId> {
        plan.actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::EnqueueOp(op) => match op.op {
                    LocalOp::UpdateAppData(task) => Some(task),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    #[test]
    fn start_tracking_opens_with_title_snapshot_and_flush_op() {
        let (app, outbox) = base_with_task_b();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        let CommandOutcome::TrackingStarted { entry, closed } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert!(closed.is_empty(), "nothing was open");
        assert_eq!(entry.task, task_id());
        assert_eq!(
            entry.task_title, "task 1",
            "the title snapshot is taken at start"
        );
        assert_eq!(entry.start, now());
        assert_eq!(entry.end, None);
        assert_eq!(entry.created_at, now());
        assert_eq!(entry.updated_at, now());
        // Batch shape: the entry upsert, then exactly one flush op.
        assert_eq!(entry_upserts(&plan).len(), 1);
        assert_eq!(flush_targets(&plan), vec![task_id()]);
    }

    #[test]
    fn start_tracking_closes_the_foreign_open_entry_first() {
        let (app, outbox) = base_with_task_b();
        let foreign = tracked_entry(1, task_b(), T0 - 600, None);
        let app = with_entry(app, foreign.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        let CommandOutcome::TrackingStarted { entry: _, closed } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(closed, &vec![foreign.id], "the foreign open entry closed");
        // Batch order (R-13): close row, then the fresh open row, then ops.
        let rows = entry_upserts(&plan);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, foreign.id);
        assert_eq!(rows[0].end, Some(now()), "stamped at the planner now");
        assert_eq!(rows[0].updated_at, now());
        assert_eq!(rows[1].end, None, "the fresh entry is open");
        assert_eq!(rows[1].task, task_id());
        // Post-apply the open set is exactly the new entry.
        let mut disk = crate::persistence::PersistedState {
            tracking: TrackingState::default(),
            ..crate::persistence::PersistedState::default()
        };
        crate::persistence::apply_actions(&mut disk, &plan.actions);
        let open: Vec<_> = disk
            .tracking
            .entries
            .values()
            .filter(|entry| entry.end.is_none())
            .collect();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].task, task_id());
    }

    #[test]
    fn start_tracking_on_the_only_open_entry_is_a_no_op() {
        let (app, outbox) = base();
        let open = tracked_entry(1, task_id(), T0 - 60, None);
        let app = with_entry(app, open);
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(plan.actions, Vec::new());
    }

    #[test]
    fn start_tracking_over_a_double_open_closes_and_reopens() {
        let (app, outbox) = base_with_task_b();
        let first = tracked_entry(1, task_id(), T0 - 900, None);
        let second = tracked_entry(2, task_id(), T0 - 300, None);
        let app = with_entry(with_entry(app, first.clone()), second.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        let CommandOutcome::TrackingStarted { closed, .. } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(
            closed,
            &vec![first.id, second.id],
            "both stamped, entry-id asc"
        );
        assert_eq!(
            entry_upserts(&plan).len(),
            3,
            "two closes plus one fresh row"
        );
    }

    #[test]
    fn start_tracking_emits_dismissals_exactly_for_active_occurrences() {
        // The SetTaskDone mirror (D-T5): the active occurrence is
        // dismissed, the other task's and the already-dismissed one are
        // not.
        let (app, outbox, rule_id) = due_task_with_fired_rule();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        let CommandOutcome::TrackingStarted { .. } = plan.outcome else {
            panic!("wrong receipt");
        };
        let dismissals: Vec<_> = plan
            .actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::RecordReminderDismissal(d) => Some(*d),
                _ => None,
            })
            .collect();
        assert_eq!(
            dismissals,
            vec![crate::reminders::ReminderDismissal {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: ts(T0 - 3_600),
                dismissed_at: ts(T0),
            }],
            "exactly the currently-active occurrences are dismissed"
        );

        // The no-occurrences case emits no dismissal rows but still
        // flushes the target (the minted row must reach the sidecar).
        let (app, outbox) = base();
        let plan = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        assert!(
            plan.actions
                .iter()
                .all(|action| !matches!(action, PersistenceAction::RecordReminderDismissal(_))),
            "no active occurrences, no dismissal rows"
        );
        assert_eq!(
            flush_targets(&plan),
            vec![task_id()],
            "the start still flushes"
        );
    }

    #[test]
    fn set_task_done_closes_open_entries_at_the_done_stamp() {
        let (app, outbox) = base();
        let mine = tracked_entry(1, task_id(), T0 - 900, None);
        let foreign = tracked_entry(2, task_b(), T0 - 300, None);
        let app = with_entry(with_entry(app, foreign), mine);

        // Reopening a done task closes nothing (a stamped end is as
        // permanent as a dismissal).
        let (mut done_app, done_outbox) = base();
        {
            let task = done_app.tasks.get_mut(&task_id()).unwrap();
            task.done = Some(ts(T0 - 60));
        }
        let done_entry = tracked_entry(3, task_id(), T0 - 900, None);
        let done_app = with_entry(done_app, done_entry);
        let reopen = run(
            &done_app,
            &done_outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: false,
            },
        )
        .unwrap();
        assert_eq!(reopen.outcome, CommandOutcome::Applied);
        assert!(
            reopen
                .actions
                .iter()
                .all(|action| !matches!(action, PersistenceAction::UpsertTimeEntry(_))),
            "reopen closes nothing"
        );

        let plan = run(
            &app,
            &outbox,
            &StateCommand::SetTaskDone {
                id: task_id(),
                done: true,
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let rows = entry_upserts(&plan);
        assert_eq!(rows.len(), 1, "only the done task's entry closes");
        assert_eq!(rows[0].id, entry_id(1));
        assert_eq!(rows[0].end, Some(now()), "stamped at the done stamp");
        assert_eq!(rows[0].updated_at, now());
        // Closes precede dismissals in the batch (the stage gate's
        // order) — here the fixture has no active notifications, so the
        // close block's own flush op is the last action.
        assert_eq!(flush_targets(&plan).last(), Some(&task_id()));
        assert_eq!(rows[0].task, task_id());
    }

    #[test]
    fn delete_task_closes_open_entries_but_keeps_the_rows() {
        let (mut app, outbox) = base();
        let mine = tracked_entry(1, task_id(), T0 - 900, None);
        let foreign = tracked_entry(2, task_b(), T0 - 300, None);
        app = with_entry(with_entry(app, foreign.clone()), mine.clone());
        let plan = run(&app, &outbox, &StateCommand::DeleteTask { id: task_id() }).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::Applied);
        let rows = entry_upserts(&plan);
        assert_eq!(rows.len(), 1, "only the deleted task's entry closes");
        assert_eq!(rows[0].id, mine.id);
        assert_eq!(rows[0].end, Some(now()));
        // No DeleteTimeEntry action anywhere: history outlives the task.
        assert!(
            plan.actions
                .iter()
                .all(|action| !matches!(action, PersistenceAction::DeleteTimeEntry(_))),
            "the rows survive the tombstone"
        );
        // The foreign open entry is untouched and its task unflushed.
        assert_eq!(flush_targets(&plan), vec![task_id()]);
        // Post-apply the foreign entry is still open (seeded, so the
        // batch cannot smuggle it in).
        let mut disk = crate::persistence::PersistedState::default();
        disk.tracking.entries.insert(foreign.id, foreign.clone());
        crate::persistence::apply_actions(&mut disk, &plan.actions);
        assert_eq!(
            disk.tracking
                .entries
                .get(&foreign.id)
                .map(|entry| entry.end),
            Some(None),
            "the foreign entry stays open"
        );
    }

    #[test]
    fn rename_never_rewrites_the_title_snapshot() {
        // start → UpdateTask title → adjust: the entry's task_title
        // stays the snapshot taken at start (D-T2).
        let (app, outbox) = base();
        let start = run(
            &app,
            &outbox,
            &StateCommand::StartTracking { task: task_id() },
        )
        .unwrap();
        let mut disk = crate::persistence::PersistedState {
            boards: app.boards.clone(),
            stacks: app.stacks.clone(),
            tasks: app.tasks.clone(),
            labels: app.labels.clone(),
            ..crate::persistence::PersistedState::default()
        };
        crate::persistence::apply_actions(&mut disk, &start.actions);
        let mid: AppState = disk.clone().into();

        let rename = run(
            &mid,
            &disk.outbox,
            &StateCommand::UpdateTask {
                id: task_id(),
                changes: crate::messages::TaskChanges {
                    title: Some("renamed".into()),
                    ..crate::messages::TaskChanges::default()
                },
            },
        )
        .unwrap();
        crate::persistence::apply_actions(&mut disk, &rename.actions);
        let after_rename: AppState = disk.clone().into();

        let entry_id = disk.tracking.entries.keys().copied().next().unwrap();
        let adjust = run(
            &after_rename,
            &disk.outbox,
            &StateCommand::AdjustTimeEntry {
                id: entry_id,
                start: Some(ts(T0 - 60)),
                end: None,
            },
        )
        .unwrap();
        crate::persistence::apply_actions(&mut disk, &adjust.actions);
        let row = &disk.tracking.entries[&entry_id];
        assert_eq!(row.task_title, "task 1", "the snapshot outlives the rename");
        assert_eq!(app.tasks[&task_id()].title, "task 1");
        let _ = rename;
    }

    #[test]
    fn start_tracking_rejects_unknown_and_deleted_tasks() {
        let (mut app, outbox) = base();
        let stranger = TaskId::from(uuid::Uuid::from_u128(4_242));
        assert!(matches!(
            run(&app, &outbox, &StateCommand::StartTracking { task: stranger }),
            Err(CommandError::UnknownTask(id)) if id == stranger
        ));
        let mut deleted = task(9, stack_id(), true);
        deleted.deleted = true;
        let dead_id = deleted.id;
        app.tasks.insert(dead_id, deleted);
        assert!(matches!(
            run(&app, &outbox, &StateCommand::StartTracking { task: dead_id }),
            Err(CommandError::TaskDeleted(id)) if id == dead_id
        ));
    }

    #[test]
    fn stop_tracking_closes_all_and_reports_stamped_rows() {
        let (app, outbox) = base_with_task_b();
        let mine = tracked_entry(2, task_id(), T0 - 900, None);
        let foreign = tracked_entry(1, task_b(), T0 - 300, None);
        let app = with_entry(with_entry(app, foreign.clone()), mine.clone());
        let plan = run(&app, &outbox, &StateCommand::StopTracking).unwrap();
        let CommandOutcome::TrackingStopped { entries } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            vec![foreign.id, mine.id],
            "post-stamp rows, entry-id ascending"
        );
        assert!(entries.iter().all(|entry| entry.end == Some(now())));
        // One flush op per distinct touched task, ascending.
        assert_eq!(flush_targets(&plan), vec![task_id(), task_b()]);
    }

    #[test]
    fn stop_tracking_with_nothing_open_is_a_no_op() {
        let (app, outbox) = base();
        let plan = run(&app, &outbox, &StateCommand::StopTracking).unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(plan.actions, Vec::new());
    }

    #[test]
    fn log_time_mints_closed_entry_without_touching_the_open_set() {
        let (app, outbox) = base();
        let open = tracked_entry(1, task_id(), T0 - 60, None);
        let app = with_entry(app, open.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::LogTime {
                task: task_id(),
                start: ts(T0 - 86_400),
                end: ts(T0 - 79_200),
            },
        )
        .unwrap();
        let CommandOutcome::TimeLogged { entry } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(entry.task, task_id());
        assert_eq!(entry.start, ts(T0 - 86_400));
        assert_eq!(entry.end, Some(ts(T0 - 79_200)));
        // The mutation clocks are the planner's now, never span data (R-14).
        assert_eq!(entry.created_at, now());
        assert_eq!(entry.updated_at, now());
        assert_eq!(entry.task_title, "task 1");
        // The open set is untouched: only the logged row is upserted.
        let rows = entry_upserts(&plan);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, entry.id);
        assert_eq!(flush_targets(&plan), vec![task_id()]);

        // Overlap with the open entry is legal (R-9).
        let plan = run(
            &app,
            &outbox,
            &StateCommand::LogTime {
                task: task_id(),
                start: ts(T0 - 30),
                end: ts(T0 - 10),
            },
        )
        .unwrap();
        assert!(matches!(plan.outcome, CommandOutcome::TimeLogged { .. }));
    }

    #[test]
    fn log_time_rejects_inverted_and_empty_spans() {
        let (app, outbox) = base();
        for (start, end) in [(T0 - 60, T0 - 3_600), (T0 - 60, T0 - 60)] {
            assert!(matches!(
                run(
                    &app,
                    &outbox,
                    &StateCommand::LogTime {
                        task: task_id(),
                        start: ts(start),
                        end: ts(end),
                    },
                ),
                Err(CommandError::InvalidSpan)
            ));
        }
    }

    #[test]
    fn adjust_sets_fields_stamps_and_can_close_an_open_entry() {
        let (app, outbox) = base();

        // Start-only on an open entry: stays open.
        let open = tracked_entry(1, task_id(), T0 - 600, None);
        let plan = run(
            &with_entry(app.clone(), open.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: open.id,
                start: Some(ts(T0 - 900)),
                end: None,
            },
        )
        .unwrap();
        let CommandOutcome::EntryAdjusted { entry } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(entry.start, ts(T0 - 900));
        assert_eq!(entry.end, None, "start-only keeps the entry open");
        assert_eq!(entry.updated_at, now());

        // End-set on an open entry: closes it at the given instant (the
        // forgotten-timer remedy).
        let plan = run(
            &with_entry(app.clone(), open.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: open.id,
                start: None,
                end: Some(ts(T0 - 100)),
            },
        )
        .unwrap();
        let CommandOutcome::EntryAdjusted { entry } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(entry.end, Some(ts(T0 - 100)));
        assert_eq!(entry.updated_at, now());

        // Both-set on a closed entry: moves the span.
        let closed = tracked_entry(2, task_id(), T0 - 800, Some(T0 - 700));
        let plan = run(
            &with_entry(app, closed.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: closed.id,
                start: Some(ts(T0 - 750)),
                end: Some(ts(T0 - 650)),
            },
        )
        .unwrap();
        let CommandOutcome::EntryAdjusted { entry } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(entry.start, ts(T0 - 750));
        assert_eq!(entry.end, Some(ts(T0 - 650)));
    }

    #[test]
    fn adjust_structurally_cannot_reopen() {
        let (app, outbox) = base();
        let closed = tracked_entry(1, task_id(), T0 - 800, Some(T0 - 700));
        // The mask `start: Some(x), end: None` names an unchanged end —
        // there is no payload that clears a stamped end.
        let plan = run(
            &with_entry(app, closed.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: closed.id,
                start: Some(ts(T0 - 850)),
                end: None,
            },
        )
        .unwrap();
        let CommandOutcome::EntryAdjusted { entry } = &plan.outcome else {
            panic!("wrong receipt");
        };
        assert_eq!(entry.end, Some(ts(T0 - 700)), "the stamped end survives");
    }

    #[test]
    fn adjust_no_op_shapes_leave_updated_at_alone() {
        let (app, outbox) = base();
        let closed = tracked_entry(1, task_id(), T0 - 800, Some(T0 - 700));

        // Both masks None.
        let plan = run(
            &with_entry(app.clone(), closed.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: closed.id,
                start: None,
                end: None,
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(plan.actions, Vec::new());

        // A values-equal mask changes nothing.
        let plan = run(
            &with_entry(app, closed.clone()),
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: closed.id,
                start: Some(closed.start),
                end: Some(closed.end.unwrap()),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(
            plan.actions,
            Vec::new(),
            "nothing churns onto the sync wire"
        );
    }

    #[test]
    fn adjust_rejects_inverted_resulting_spans_and_unknown_entries() {
        let (app, outbox) = base();
        // Closed entry ending at T0 - 700; a new start after the kept end.
        let closed = tracked_entry(1, task_id(), T0 - 800, Some(T0 - 700));
        assert!(matches!(
            run(
                &with_entry(app.clone(), closed),
                &outbox,
                &StateCommand::AdjustTimeEntry {
                    id: entry_id(1),
                    start: Some(ts(T0 - 600)),
                    end: None,
                },
            ),
            Err(CommandError::InvalidSpan)
        ));
        let stranger = entry_id(9_999);
        assert!(matches!(
            run(
                &app,
                &outbox,
                &StateCommand::AdjustTimeEntry {
                    id: stranger,
                    start: Some(ts(T0)),
                    end: None,
                },
            ),
            Err(CommandError::UnknownTimeEntry(id)) if id == stranger
        ));
    }

    #[test]
    fn entries_of_deleted_tasks_stay_adjustable_and_deletable() {
        let (mut app, outbox) = base();
        let mut dead = task(9, stack_id(), true);
        dead.deleted = true;
        let dead_id = dead.id;
        app.tasks.insert(dead_id, dead);
        let entry = tracked_entry(1, dead_id, T0 - 800, None);
        let app = with_entry(app, entry.clone());

        let plan = run(
            &app,
            &outbox,
            &StateCommand::AdjustTimeEntry {
                id: entry.id,
                start: None,
                end: Some(ts(T0 - 100)),
            },
        )
        .unwrap();
        assert!(matches!(plan.outcome, CommandOutcome::EntryAdjusted { .. }));

        let plan = run(
            &app,
            &outbox,
            &StateCommand::DeleteTimeEntry { id: entry.id },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::EntryDeleted { id: entry.id });
        assert_eq!(
            plan.actions[0],
            PersistenceAction::DeleteTimeEntry(entry.id),
            "the row removal is planned"
        );
        assert_eq!(flush_targets(&plan), vec![dead_id], "the flush op rides");
    }

    #[test]
    fn delete_time_entry_removes_rows_and_absent_id_is_a_no_op() {
        let (app, outbox) = base();
        let entry = tracked_entry(1, task_id(), T0 - 800, Some(T0 - 700));
        let app = with_entry(app, entry.clone());
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DeleteTimeEntry { id: entry.id },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::EntryDeleted { id: entry.id });

        // The repeat (row already folded away by a synced tombstone):
        // the accepted no-op (R-7).
        let plan = run(
            &app,
            &outbox,
            &StateCommand::DeleteTimeEntry {
                id: entry_id(9_424),
            },
        )
        .unwrap();
        assert_eq!(plan.outcome, CommandOutcome::NoOp);
        assert_eq!(plan.actions, Vec::new());
    } // ---- Single-global convergence (DR-5/1, §5.18) -----------------------

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]

        #[test]
        fn single_global_convergence_prop(
            tracking in crate::test_support::tracking_state_strategy(),
            target in 0_u64..4,
            stop in proptest::bool::ANY,
        ) {
            // A small live-task pool: the tracking verbs resolve against
            // it, nothing else in the state matters.
            let mut tasks = std::collections::BTreeMap::new();
            for raw in 0_u64..4 {
                let row = task(raw, stack_id(), false);
                tasks.insert(row.id, row);
            }
            let target = TaskId::from(uuid::Uuid::from_u128(u128::from(target)));
            let app = AppState {
                tasks: tasks.clone(),
                tracking: tracking.clone(),
                ..AppState::default()
            };
            let command = if stop {
                StateCommand::StopTracking
            } else {
                StateCommand::StartTracking { task: target }
            };
            let plan = plan_command(
                &app,
                &[],
                &command,
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
                now(),
            )
            .expect("the verbs accept every (state, pool) shape here");
            let mut disk = crate::persistence::PersistedState {
                tracking: tracking.clone(),
                ..crate::persistence::PersistedState::default()
            };
            crate::persistence::apply_actions(&mut disk, &plan.actions);
            let open: Vec<_> = disk
                .tracking
                .entries
                .values()
                .filter(|entry| entry.end.is_none())
                .collect();
            if stop {
                prop_assert_eq!(open.len(), 0, "stop converges to zero open entries");
            } else if plan.outcome == CommandOutcome::NoOp {
                // The NoOp precondition: the open set was exactly the
                // target's single entry.
                let pre_open: Vec<_> = tracking
                    .entries
                    .values()
                    .filter(|entry| entry.end.is_none())
                    .cloned()
                    .collect();
                prop_assert_eq!(pre_open.len(), 1);
                prop_assert_eq!(pre_open[0].task, target);
                prop_assert_eq!(open.len(), 1);
            } else {
                prop_assert_eq!(open.len(), 1, "start converges to one open entry");
                prop_assert_eq!(open[0].task, target);
            }
        }

        #[test]
        fn unioned_single_global_states_converge_at_the_next_write(
            left in crate::test_support::tracking_state_strategy(),
            right in crate::test_support::tracking_state_strategy(),
            target in 0_u64..4,
        ) {
            // The single-global precondition each device maintains
            // locally: at most one open entry per state. Both devices'
            // open picks may name different tasks — exactly the
            // concurrent-starts shape D-T8 rules on.
            let open_count = |state: &crate::tracking::TrackingState| {
                state.entries.values().filter(|entry| entry.end.is_none()).count()
            };
            prop_assume!(open_count(&left) <= 1, "left is single-global");
            prop_assume!(open_count(&right) <= 1, "right is single-global");
            // Union by a test-local per-key newer-`updated_at`-wins
            // oracle (the documented stand-in for the storage chunk's
            // `union_time_entries`); ties go to the overlay, matching
            // the sidecar's union contract. The union may hold two open
            // entries — the transient shape the next write must clean.
            let mut union = left.clone();
            for (id, row) in &right.entries {
                match union.entries.get(id) {
                    Some(known) if known.updated_at > row.updated_at => {}
                    _ => {
                        union.entries.insert(*id, row.clone());
                    }
                }
            }
            let mut tasks = std::collections::BTreeMap::new();
            for raw in 0_u64..4 {
                let row = task(raw, stack_id(), false);
                tasks.insert(row.id, row);
            }
            let app = AppState {
                tasks: tasks.clone(),
                tracking: union.clone(),
                ..AppState::default()
            };
            // The next write converges: stop → 0 open, start → 1.
            let stop = plan_command(
                &app,
                &[],
                &StateCommand::StopTracking,
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
                now(),
            )
            .unwrap();
            let mut disk = crate::persistence::PersistedState {
                tracking: union,
                ..crate::persistence::PersistedState::default()
            };
            crate::persistence::apply_actions(&mut disk, &stop.actions);
            prop_assert!(
                disk.tracking
                    .entries
                    .values()
                    .all(|entry| entry.end.is_some()),
                "the union's next stop converges to zero open"
            );
            let target = TaskId::from(uuid::Uuid::from_u128(u128::from(target)));
            let app = AppState {
                tasks: tasks.clone(),
                tracking: disk.tracking.clone(),
                ..AppState::default()
            };
            let start = plan_command(
                &app,
                &[],
                &StateCommand::StartTracking { task: target },
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
                now(),
            )
            .unwrap();
            crate::persistence::apply_actions(&mut disk, &start.actions);
            let open: Vec<_> = disk
                .tracking
                .entries
                .values()
                .filter(|entry| entry.end.is_none())
                .collect();
            prop_assert_eq!(open.len(), 1, "the union's next start converges to one");
            prop_assert_eq!(open[0].task, target);
        }
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
            | LocalOp::DeleteTask(id)
            | LocalOp::UpdateAppData(id) => tasks.contains_key(id),
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
                tracking: persisted.tracking.clone(),
                sync: persisted.sync,
                last_updated: None,
            };
            let outbox = persisted.outbox.clone();
            let result = plan_command(
                &app,
                &outbox,
                &command,
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
                chrono::Utc.timestamp_opt(36_000, 0).unwrap(),
            );

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
            let baseline = plan_command(
                &state,
                &empty,
                &command,
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
                now,
            );
            let variant = plan_command(
                &state,
                &outbox_state.outbox,
                &command,
                &CountingIds::default(),
                &crate::reminders::ReminderDefaults::default(),
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

        #[test]
        fn live_board_of_is_ok_exactly_for_live_boards_and_typed_otherwise(
            persisted in persisted_state_strategy(),
            board in crate::test_support::board_id_strategy(),
        ) {
            let app: AppState = AppState {
                boards: persisted.boards.clone(),
                stacks: persisted.stacks.clone(),
                tasks: persisted.tasks.clone(),
                labels: persisted.labels.clone(),
                reminders: persisted.reminders.clone(),
                tracking: persisted.tracking.clone(),
                sync: persisted.sync,
                last_updated: None,
            };
            let result = live_board_of(&app, board);
            match app.boards.get(&board) {
                None => prop_assert_eq!(result, Err(CommandError::UnknownBoard(board))),
                Some(b) if b.deleted => {
                    prop_assert_eq!(result, Err(CommandError::BoardDeleted(board)));
                }
                Some(_) => prop_assert_eq!(result, Ok(board)),
            }
        }
    }
}
