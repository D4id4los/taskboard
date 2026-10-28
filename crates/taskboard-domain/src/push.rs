// SPDX-License-Identifier: MIT OR Apache-2.0
//! The push coalescing planner: from the pending outbox to the minimal
//! sequence of client writes for one sync cycle (phase 4 plan §4.3).
//!
//! [`plan_pushes`] is pure: it reads the outbox plus the *current* entity
//! tables (never remote bindings — the push executor resolves those through
//! its overlay) and groups the ops per entity. One group materializes at
//! most one client call; subsumed ops ride on it and are reported together
//! with it. The normative grouping table:
//!
//! | Group contains            | Materialized                                   | Subsumed                        |
//! |---------------------------|------------------------------------------------|---------------------------------|
//! | any `Delete*`             | `Delete*`                                      | every other op of the entity    |
//! | `Create*` (+ edits/moves) | `Create*` from current state                   | the updates/moves/label ops     |
//! | else (edits only)         | one `UpdateTask` (full-send) and/or `MoveTask` | all other edit ops of the entity |
//!
//! Cross-entity order: stacks → labels → tasks (the dependency direction of
//! card-into-stack creates and label resolution); within a kind, the queue
//! position of the group's first op.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::outbox::{LocalOp, OpId, PendingOp};

/// Which entity a [`PushGroup`] materializes against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PushTarget {
    /// A board entity (board writes precede their children's).
    Board(BoardId),
    /// A task (card) entity.
    Task(TaskId),
    /// A stack entity.
    Stack(StackId),
    /// A label entity.
    Label(LabelId),
    /// A board's sidecar document (the flush; one per board per cycle,
    /// always the last group — the entries key on remote card ids that
    /// exist only after the cycle's creates echoed).
    Document(BoardId),
}

/// The surviving client intent of one [`PushGroup`].
///
/// The shapes carry the *current local state* of the target entity at plan
/// time; the sync crate maps them onto its client's write-param structs.
/// [`MaterializedOp::Noop`] is produced for groups whose entity is absent
/// from the tables (nothing to send — every member op completes
/// synthetically) and by the executor for unbound-entity deletes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializedOp {
    /// POST a new card for `task` into `stack`.
    CreateTask {
        /// The (unbound) task to create.
        task: TaskId,
        /// The owning local stack (resolved through the overlay by the
        /// executor).
        stack: StackId,
        /// The card fields to send.
        new_card: NewCardShape,
    },
    /// Full-send PUT of the edited card's fields.
    UpdateTask {
        /// The task to update.
        task: TaskId,
        /// The fields to send.
        card: CardShape,
    },
    /// The reorder/move primitive (destination stack goes in the body).
    MoveTask {
        /// The task to move.
        task: TaskId,
        /// Target local stack and order.
        to: (StackId, i64),
    },
    /// DELETE the card.
    DeleteTask {
        /// The task to delete.
        task: TaskId,
    },
    /// PUT the board's full changeset (title + color + archived).
    UpdateBoard {
        /// The bound board to update.
        board: BoardId,
    },
    /// DELETE the board.
    DeleteBoard {
        /// The bound board to delete.
        board: BoardId,
    },
    /// POST a new stack.
    CreateStack {
        /// The (unbound) stack to create.
        stack: StackId,
    },
    /// PUT the stack's full changeset (title + order).
    RenameStack {
        /// The stack to update.
        stack: StackId,
    },
    /// PUT the stack's full changeset (order + title; the same full-send
    /// wire call as [`MaterializedOp::RenameStack`]).
    MoveStack {
        /// The stack to update.
        stack: StackId,
    },
    /// DELETE the stack.
    DeleteStack {
        /// The stack to delete.
        stack: StackId,
    },
    /// POST a new label.
    CreateLabel {
        /// The (unbound) label to create.
        label: LabelId,
    },
    /// PUT the label's full changeset (title + color).
    UpdateLabel {
        /// The label to update.
        label: LabelId,
    },
    /// DELETE the label.
    DeleteLabel {
        /// The label to delete.
        label: LabelId,
    },
    /// The board's sidecar flush (one PUT of the unioned document).
    /// Carries no content: the flush materializes from the sync actor's
    /// state + cache at execution time (between the cycle's pull and its
    /// report), like every shape carries "current local state at plan
    /// time" and the executor maps it.
    FlushAppData {
        /// The owning local board.
        board: BoardId,
    },
    /// Nothing to send: every op of the group completes synthetically.
    Noop,
}

impl MaterializedOp {
    /// Whether the group materializes no client call at all.
    #[must_use]
    pub const fn is_noop(&self) -> bool {
        matches!(self, Self::Noop)
    }
}

/// The card fields of a create (domain-side mirror of the client's
/// `NewCard` plus the fields Deck cannot take at create time — the executor
/// applies those as post-create sub-endpoint calls).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCardShape {
    /// Card title.
    pub title: String,
    /// Sort order.
    pub order: i64,
    /// Description.
    pub description: String,
    /// Due date.
    pub duedate: Option<DateTime<Utc>>,
    /// Scheduled start (Deck's native `startdate` slot).
    pub scheduled: Option<DateTime<Utc>>,
    /// Completion timestamp (sent post-create: Deck's card PUT owns `done`).
    pub done: Option<DateTime<Utc>>,
    /// Archived flag (sent post-create via the archive sub-endpoint).
    pub archived: bool,
    /// The current label set (local ids; the executor resolves them through
    /// the overlay — unresolvable labels are deferred, never dropped).
    pub labels: std::collections::BTreeSet<LabelId>,
}

/// The full-send PUT fields of a card update (domain-side mirror of the
/// client's round-trip write fields).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardShape {
    /// Card title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Due date.
    pub duedate: Option<DateTime<Utc>>,
    /// Scheduled start (Deck's native `startdate` slot).
    pub scheduled: Option<DateTime<Utc>>,
    /// Completion timestamp.
    pub done: Option<DateTime<Utc>>,
    /// Sort order within the (fetched) stack.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// The current label set (local ids).
    pub labels: std::collections::BTreeSet<LabelId>,
}

impl NewCardShape {
    fn from_task(task: &Task) -> Self {
        Self {
            title: task.title.clone(),
            order: task.order,
            description: task.description.clone(),
            duedate: task.duedate,
            scheduled: task.scheduled,
            done: task.done,
            archived: task.archived,
            labels: task.live_labels(),
        }
    }
}

impl CardShape {
    fn from_task(task: &Task) -> Self {
        Self {
            title: task.title.clone(),
            description: task.description.clone(),
            duedate: task.duedate,
            scheduled: task.scheduled,
            done: task.done,
            order: task.order,
            archived: task.archived,
            labels: task.live_labels(),
        }
    }
}

/// The current entity tables the planner materializes intents from.
///
/// One borrow group instead of a full [`crate::persistence::PersistedState`]:
/// the planner needs exactly these four maps and nothing else.
#[derive(Debug, Clone, Copy)]
pub struct EntityTables<'a> {
    /// Current tasks.
    pub tasks: &'a BTreeMap<TaskId, Task>,
    /// Current stacks.
    pub stacks: &'a BTreeMap<StackId, Stack>,
    /// Current labels.
    pub labels: &'a BTreeMap<LabelId, Label>,
    /// Current boards (a queued `UpdateBoard` materializes only for a
    /// *bound* board; a local-only one completes synthetically).
    pub boards: &'a BTreeMap<BoardId, Board>,
}

/// One coalesced push: a materialized client intent plus the outbox ops
/// that ride on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushGroup {
    /// Which entity the group materializes against.
    pub target: PushTarget,
    /// The surviving client intent for this cycle.
    pub materialized: MaterializedOp,
    /// The materialized op's own id (an outcome is always emitted for it).
    pub primary: OpId,
    /// Op ids that succeed/fail together with `materialized` (subsumed), in
    /// queue order. Empty when the group is a bare move or noop.
    pub subsumed: Vec<OpId>,
}

/// Plans the minimal push sequence for the outbox.
///
/// Groups partition the outbox: every op id appears exactly once, either as
/// its group's `primary` or in its `subsumed` list. Groups are ordered
/// stacks-first, labels-second, tasks-last; within a kind, by the queue
/// position of the group's first op.
///
/// # Examples
///
/// Repeated edits of one card collapse into a single full-send PUT; the
/// subsumed op ids ride on the materialized op's outcome:
///
/// ```
/// use std::collections::BTreeMap;
///
/// use chrono::{TimeZone, Utc};
/// use taskboard_domain::{
///     plan_pushes, EntityTables, LocalOp, OpId, PendingOp, StackId, Task, TaskClocks,
///     TaskId,
/// };
///
/// let t = |secs: i64| Utc.timestamp_opt(secs, 0).unwrap();
/// let task_id = TaskId::from(uuid::Uuid::from_u128(1));
/// let stack_id = StackId::from(uuid::Uuid::from_u128(2));
/// let clocks = TaskClocks {
///     title: t(100),
///     description: t(100),
///     duedate: t(100),
///     scheduled: t(100),
///     done: t(100),
///     position: t(100),
///     effort: t(100),
///     importance: t(100),
///     urgency: t(100),
///     recur: t(100),
///     archived: t(100),
///     deleted: t(100),
/// };
/// let task = Task {
///     id: task_id,
///     remote: None,
///     title: "edited twice".into(),
///     description: String::new(),
///     duedate: None,
///     scheduled: None,
///     done: None,
///     stack: stack_id,
///     order: 0,
///     label_memberships: std::collections::BTreeMap::new(),
///     effort: None,
///     importance: None,
///     urgency: None,
///     recur: None,
///     archived: false,
///     deleted: false,
///     clocks,
///     remote_seen: None,
///     remote_baseline: None,
/// };
/// let mut tasks = BTreeMap::new();
/// tasks.insert(task_id, task);
///
/// let op = |n: u128, op: LocalOp| PendingOp {
///     op_id: OpId(uuid::Uuid::from_u128(n)),
///     op,
///     queued_at: t(200),
/// };
/// let outbox = vec![
///     op(10, LocalOp::UpdateTask(task_id)),
///     op(11, LocalOp::UpdateTask(task_id)),
/// ];
///
/// let stacks: BTreeMap<StackId, taskboard_domain::Stack> = BTreeMap::new();
/// let labels: BTreeMap<taskboard_domain::LabelId, taskboard_domain::Label> = BTreeMap::new();
/// let boards: BTreeMap<taskboard_domain::BoardId, taskboard_domain::Board> = BTreeMap::new();
/// let groups = plan_pushes(&outbox, EntityTables {
///     tasks: &tasks,
///     stacks: &stacks,
///     labels: &labels,
///     boards: &boards,
/// });
/// assert_eq!(groups.len(), 1);
/// assert_eq!(groups[0].primary, OpId(uuid::Uuid::from_u128(10)));
/// assert_eq!(groups[0].subsumed, vec![OpId(uuid::Uuid::from_u128(11))]);
/// ```
#[must_use]
pub fn plan_pushes(outbox: &[PendingOp], entities: EntityTables<'_>) -> Vec<PushGroup> {
    // Bucket op (queue index, op) pairs per entity, preserving queue order.
    let mut task_ops: Vec<(TaskId, usize, &PendingOp)> = Vec::new();
    let mut stack_ops: Vec<(StackId, usize, &PendingOp)> = Vec::new();
    let mut label_ops: Vec<(LabelId, usize, &PendingOp)> = Vec::new();
    let mut board_ops: Vec<(BoardId, usize, &PendingOp)> = Vec::new();
    let mut document_ops: Vec<(BoardId, usize, &PendingOp)> = Vec::new();
    for (idx, entry) in outbox.iter().enumerate() {
        match entry.op {
            LocalOp::CreateTask(id)
            | LocalOp::UpdateTask(id)
            | LocalOp::MoveTask(id)
            | LocalOp::DeleteTask(id)
            | LocalOp::AssignLabel(id, _)
            | LocalOp::UnassignLabel(id, _) => task_ops.push((id, idx, entry)),
            LocalOp::CreateStack(id)
            | LocalOp::RenameStack(id)
            | LocalOp::MoveStack(id)
            | LocalOp::DeleteStack(id) => {
                stack_ops.push((id, idx, entry));
            }
            LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                label_ops.push((id, idx, entry));
            }
            LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => {
                board_ops.push((id, idx, entry));
            }
            // The document lane: `UpdateAppData` coalesces into the
            // board's sidecar flush, never a native wire group. The
            // owning board resolves through the entity graph; an
            // unresolvable op (its task row wiped) buckets into no
            // group — no outcome ⇒ it stays queued (the delete
            // cascades cancel it by target, D7 row 7).
            LocalOp::UpdateAppData(task_id) => {
                if let Some(board) = entities
                    .tasks
                    .get(&task_id)
                    .and_then(|task| entities.stacks.get(&task.stack))
                    .map(|stack| stack.board)
                {
                    document_ops.push((board, idx, entry));
                }
            }
        }
    }

    let mut groups: Vec<(usize, PushGroup)> = Vec::new();
    groups.extend(plan_board_groups(&board_ops, entities.boards));
    groups.extend(plan_stack_groups(&stack_ops, entities.stacks));
    groups.extend(plan_label_groups(&label_ops, entities.labels));
    groups.extend(plan_task_groups(&task_ops, entities.tasks));
    groups.extend(plan_document_groups(&document_ops));

    // Boards → stacks → labels → tasks → the document flush last (the
    // flush's entries key on remote card ids that exist only post-echo),
    // then queue position within a kind.
    groups.sort_by_key(|(first_idx, group)| (kind_rank(group.target), *first_idx));
    groups.into_iter().map(|(_, group)| group).collect()
}

/// The cross-entity dependency rank: board first (a board write precedes
/// its children's), then stacks, labels, tasks, and the document flush
/// strictly last (its entries key on remote card ids that exist only
/// after this cycle's creates echoed).
fn kind_rank(target: PushTarget) -> u8 {
    match target {
        PushTarget::Board(_) => 0,
        PushTarget::Stack(_) => 1,
        PushTarget::Label(_) => 2,
        PushTarget::Task(_) => 3,
        PushTarget::Document(_) => 4,
    }
}

/// The document lane's coalescing rule (D10): all of a board's pending
/// `UpdateAppData` ops bucket into ONE flush group — `primary` is the
/// first in queue order, `subsumed` the rest. One PUT per flush; the
/// partition property holds (every op id appears exactly once).
fn plan_document_groups(ops: &[(BoardId, usize, &PendingOp)]) -> Vec<(usize, PushGroup)> {
    grouped(ops)
        .into_iter()
        .map(|(board, members)| {
            let primary = members[0].1.op_id;
            let group = PushGroup {
                target: PushTarget::Document(board),
                materialized: MaterializedOp::FlushAppData { board },
                primary,
                subsumed: others(&members, primary),
            };
            // The group's first op's queue position (the sort key).
            let first_idx = members[0].0;
            (first_idx, group)
        })
        .collect()
}

/// Partitions a bucket into per-entity groups, ordered by first-op queue
/// position (the bucket is already in queue order, so a stable sort over
/// the bucket walk is enough).
fn grouped<'a, K: Ord + Copy>(
    ops: &[(K, usize, &'a PendingOp)],
) -> Vec<(K, Vec<(usize, &'a PendingOp)>)> {
    let mut order: Vec<K> = Vec::new();
    let mut buckets: BTreeMap<K, Vec<(usize, &PendingOp)>> = BTreeMap::new();
    for (key, idx, op) in ops {
        if !buckets.contains_key(key) {
            order.push(*key);
        }
        buckets.entry(*key).or_default().push((*idx, *op));
    }
    order
        .into_iter()
        .map(|key| (key, buckets.remove(&key).unwrap_or_default()))
        .collect()
}

fn plan_stack_groups(
    ops: &[(StackId, usize, &PendingOp)],
    stacks: &BTreeMap<StackId, Stack>,
) -> Vec<(usize, PushGroup)> {
    grouped(ops)
        .into_iter()
        .map(|(stack, members)| {
            let first_idx = members[0].0;
            let group = if let Some((_, del)) = members
                .iter()
                .find(|(_, op)| matches!(op.op, LocalOp::DeleteStack(id) if id == stack))
            {
                PushGroup {
                    target: PushTarget::Stack(stack),
                    materialized: MaterializedOp::DeleteStack { stack },
                    primary: del.op_id,
                    subsumed: others(&members, del.op_id),
                }
            } else if let Some((_, create)) = members
                .iter()
                .find(|(_, op)| matches!(op.op, LocalOp::CreateStack(id) if id == stack))
            {
                let materialized = if stacks.contains_key(&stack) {
                    MaterializedOp::CreateStack { stack }
                } else {
                    MaterializedOp::Noop
                };
                PushGroup {
                    target: PushTarget::Stack(stack),
                    materialized,
                    primary: create.op_id,
                    subsumed: others(&members, create.op_id),
                }
            } else if stacks.contains_key(&stack) {
                let (_, first) = members[0];
                // A rename and a move share one full-send PUT; the first
                // queued op decides which tag the group carries.
                let materialized = if matches!(first.op, LocalOp::MoveStack(_)) {
                    MaterializedOp::MoveStack { stack }
                } else {
                    MaterializedOp::RenameStack { stack }
                };
                PushGroup {
                    target: PushTarget::Stack(stack),
                    materialized,
                    primary: first.op_id,
                    subsumed: others(&members, first.op_id),
                }
            } else {
                noop_group(PushTarget::Stack(stack), &members)
            };
            (first_idx, group)
        })
        .collect()
}

/// Plans the board groups: one full-send PUT per bound board with a
/// queued rename; a local-only board's op completes synthetically (the
/// `publish` ceremony re-sends the same title idempotently after
/// binding).
fn plan_board_groups(
    ops: &[(BoardId, usize, &PendingOp)],
    boards: &BTreeMap<BoardId, Board>,
) -> Vec<(usize, PushGroup)> {
    grouped(ops)
        .into_iter()
        .map(|(board, members)| {
            let first_idx = members[0].0;
            let group = if let Some((_, del)) = members
                .iter()
                .find(|(_, op)| matches!(op.op, LocalOp::DeleteBoard(id) if id == board))
            {
                // Delete dominance (the stacks rule): one DELETE subsumes
                // the board's other ops — but only a *bound* board
                // materializes (the tombstone keeps its binding until the
                // push confirms, R9c); an unbound board's group is a
                // synthetic completion.
                let materialized = match boards.get(&board) {
                    Some(row) if row.remote.is_some() => MaterializedOp::DeleteBoard { board },
                    _ => MaterializedOp::Noop,
                };
                PushGroup {
                    target: PushTarget::Board(board),
                    materialized,
                    primary: del.op_id,
                    subsumed: others(&members, del.op_id),
                }
            } else {
                match boards.get(&board) {
                    Some(row) if row.remote.is_some() && !row.deleted => {
                        let (_, first) = members[0];
                        PushGroup {
                            target: PushTarget::Board(board),
                            materialized: MaterializedOp::UpdateBoard { board },
                            primary: first.op_id,
                            subsumed: others(&members, first.op_id),
                        }
                    }
                    _ => noop_group(PushTarget::Board(board), &members),
                }
            };
            (first_idx, group)
        })
        .collect()
}

fn plan_label_groups(
    ops: &[(LabelId, usize, &PendingOp)],
    labels: &BTreeMap<LabelId, Label>,
) -> Vec<(usize, PushGroup)> {
    grouped(ops)
        .into_iter()
        .map(|(label, members)| {
            let first_idx = members[0].0;
            let group = if let Some((_, del)) = members
                .iter()
                .find(|(_, op)| matches!(op.op, LocalOp::DeleteLabel(id) if id == label))
            {
                PushGroup {
                    target: PushTarget::Label(label),
                    materialized: MaterializedOp::DeleteLabel { label },
                    primary: del.op_id,
                    subsumed: others(&members, del.op_id),
                }
            } else if let Some((_, create)) = members
                .iter()
                .find(|(_, op)| matches!(op.op, LocalOp::CreateLabel(id) if id == label))
            {
                PushGroup {
                    target: PushTarget::Label(label),
                    materialized: MaterializedOp::CreateLabel { label },
                    primary: create.op_id,
                    subsumed: others(&members, create.op_id),
                }
            } else if labels.contains_key(&label) {
                let (_, update) = members[0];
                PushGroup {
                    target: PushTarget::Label(label),
                    materialized: MaterializedOp::UpdateLabel { label },
                    primary: update.op_id,
                    subsumed: others(&members, update.op_id),
                }
            } else {
                noop_group(PushTarget::Label(label), &members)
            };
            (first_idx, group)
        })
        .collect()
}

#[allow(clippy::too_many_lines)] // one normative table; the arms ARE the design
fn plan_task_groups(
    ops: &[(TaskId, usize, &PendingOp)],
    tasks: &BTreeMap<TaskId, Task>,
) -> Vec<(usize, PushGroup)> {
    grouped(ops)
        .into_iter()
        .flat_map(|(task, members)| {
            let mut groups: Vec<(usize, PushGroup)> = Vec::new();
            let deletes: Vec<_> = members
                .iter()
                .filter(|(_, op)| matches!(op.op, LocalOp::DeleteTask(id) if id == task))
                .copied()
                .collect();
            let creates: Vec<_> = members
                .iter()
                .filter(|(_, op)| matches!(op.op, LocalOp::CreateTask(id) if id == task))
                .copied()
                .collect();
            let moves: Vec<_> = members
                .iter()
                .filter(|(_, op)| matches!(op.op, LocalOp::MoveTask(id) if id == task))
                .copied()
                .collect();

            if let Some(&(_, del)) = deletes.first() {
                groups.push((
                    members[0].0,
                    PushGroup {
                        target: PushTarget::Task(task),
                        materialized: MaterializedOp::DeleteTask { task },
                        primary: del.op_id,
                        subsumed: others(&members, del.op_id),
                    },
                ));
            } else if let Some(&(_, create)) = creates.first() {
                let materialized = match tasks.get(&task) {
                    Some(row) => MaterializedOp::CreateTask {
                        task,
                        stack: row.stack,
                        new_card: NewCardShape::from_task(row),
                    },
                    None => MaterializedOp::Noop,
                };
                groups.push((
                    members[0].0,
                    PushGroup {
                        target: PushTarget::Task(task),
                        materialized,
                        primary: create.op_id,
                        subsumed: others(&members, create.op_id),
                    },
                ));
            } else {
                let edits: Vec<_> = members
                    .iter()
                    .filter(|(_, op)| !matches!(op.op, LocalOp::MoveTask(id) if id == task))
                    .copied()
                    .collect();
                if !edits.is_empty() {
                    let materialized = match tasks.get(&task) {
                        Some(row) => MaterializedOp::UpdateTask {
                            task,
                            card: CardShape::from_task(row),
                        },
                        None => MaterializedOp::Noop,
                    };
                    let primary = edits[0].1.op_id;
                    groups.push((
                        edits[0].0,
                        PushGroup {
                            target: PushTarget::Task(task),
                            materialized,
                            primary,
                            subsumed: others(edits.as_slice(), primary),
                        },
                    ));
                }
                if let Some((first_move_idx, first_move)) = moves.first().copied() {
                    let materialized = match tasks.get(&task) {
                        Some(row) => MaterializedOp::MoveTask {
                            task,
                            to: (row.stack, row.order),
                        },
                        None => MaterializedOp::Noop,
                    };
                    groups.push((
                        first_move_idx,
                        PushGroup {
                            target: PushTarget::Task(task),
                            materialized,
                            primary: first_move.op_id,
                            subsumed: others(moves.as_slice(), first_move.op_id),
                        },
                    ));
                }
            }
            groups
        })
        .collect()
}

/// All member op ids except `primary`, in queue order.
fn others(members: &[(usize, &PendingOp)], primary: OpId) -> Vec<OpId> {
    members
        .iter()
        .map(|(_, op)| op.op_id)
        .filter(|id| *id != primary)
        .collect()
}

fn noop_group(target: PushTarget, members: &[(usize, &PendingOp)]) -> PushGroup {
    let primary = members[0].1.op_id;
    PushGroup {
        target,
        materialized: MaterializedOp::Noop,
        primary,
        subsumed: others(members, primary),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{LabelClocks, StackClocks, TaskClocks};
    use crate::ids::{LabelId, StackId, TaskId};
    use chrono::TimeZone;
    use proptest::prelude::*;
    use uuid::Uuid;

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn uuid(raw: u128) -> Uuid {
        Uuid::from_u128(raw)
    }

    fn task_id(raw: u128) -> TaskId {
        TaskId::from(uuid(raw))
    }

    fn stack_id(raw: u128) -> StackId {
        StackId::from(uuid(raw))
    }

    fn label_id(raw: u128) -> LabelId {
        LabelId::from(uuid(raw))
    }

    fn clocks(at: i64) -> TaskClocks {
        let t = ts(at);
        TaskClocks {
            title: t,
            description: t,
            duedate: t,
            scheduled: t,
            done: t,
            position: t,
            effort: t,
            importance: t,
            urgency: t,
            recur: t,

            archived: t,
            deleted: t,
        }
    }

    fn stack_clocks(at: i64) -> StackClocks {
        let t = ts(at);
        StackClocks {
            title: t,
            order: t,
            deleted: t,
        }
    }

    fn label_clocks(at: i64) -> LabelClocks {
        let t = ts(at);
        LabelClocks {
            title: t,
            color: t,
            deleted: t,
        }
    }

    fn task_row(raw: u128, stack: StackId) -> Task {
        Task {
            id: task_id(raw),
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack,
            order: 0,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: clocks(100),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn stack_row(raw: u128) -> Stack {
        Stack {
            id: stack_id(raw),
            remote: None,
            board: crate::ids::BoardId::from(uuid(900)),
            title: format!("stack {raw}"),
            order: 0,
            archived: false,
            deleted: false,
            clocks: stack_clocks(100),
            remote_seen: None,
        }
    }

    fn label_row(raw: u128, board: crate::ids::BoardId) -> Label {
        Label {
            id: label_id(raw),
            remote: None,
            board,
            title: format!("label {raw}"),
            color: crate::entities::Color::new("00ff00"),
            deleted: false,
            clocks: label_clocks(100),
            remote_seen: None,
        }
    }

    fn board_row(raw: u128) -> Board {
        Board {
            id: crate::ids::BoardId::from(uuid(raw)),
            remote: None,
            title: format!("board {raw}"),
            color: crate::entities::Color::new("00ff00"),
            archived: false,
            deleted: false,
            clocks: crate::entities::BoardClocks {
                title: ts(100),
                color: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn op(raw: u128, kind: LocalOp) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid(raw)),
            op: kind,
            queued_at: ts(200),
        }
    }

    static EMPTY: std::sync::LazyLock<BTreeMap<crate::ids::BoardId, Board>> =
        std::sync::LazyLock::new(BTreeMap::new);
    static EMPTY_LABELS: std::sync::LazyLock<
        BTreeMap<crate::ids::LabelId, crate::entities::Label>,
    > = std::sync::LazyLock::new(BTreeMap::new);

    fn tables<'a>(
        tasks: &'a BTreeMap<TaskId, Task>,
        stacks: &'a BTreeMap<StackId, Stack>,
        labels: &'a BTreeMap<LabelId, Label>,
    ) -> EntityTables<'a> {
        EntityTables {
            tasks,
            stacks,
            labels,
            boards: &EMPTY,
        }
    }

    fn all_ids(groups: &[PushGroup]) -> Vec<OpId> {
        let mut ids: Vec<OpId> = groups.iter().map(|g| g.primary).collect();
        ids.extend(groups.iter().flat_map(|g| g.subsumed.iter().copied()));
        ids
    }

    #[test]
    fn delete_subsumes_every_other_op_of_the_entity() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let task = task_row(10, stack.id);
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::UpdateTask(task.id)),
            op(101, LocalOp::AssignLabel(task.id, label_id(20))),
            op(102, LocalOp::MoveTask(task.id)),
            op(103, LocalOp::DeleteTask(task.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(
            group.materialized,
            MaterializedOp::DeleteTask { task: task.id }
        );
        assert_eq!(group.primary, OpId(uuid(103)));
        assert_eq!(
            group.subsumed,
            vec![OpId(uuid(100)), OpId(uuid(101)), OpId(uuid(102))]
        );
    }

    #[test]
    fn create_subsumes_updates_moves_and_label_ops() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.title = "created offline".into();
        task.order = 3;
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::CreateTask(task.id)),
            op(101, LocalOp::UpdateTask(task.id)),
            op(102, LocalOp::MoveTask(task.id)),
            op(103, LocalOp::AssignLabel(task.id, label_id(20))),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        let MaterializedOp::CreateTask {
            stack: target_stack,
            new_card,
            ..
        } = &group.materialized
        else {
            panic!(
                "expected a create materialization, got {:?}",
                group.materialized
            );
        };
        assert_eq!(*target_stack, stack.id);
        assert_eq!(new_card.title, "created offline");
        assert_eq!(new_card.order, 3);
        assert_eq!(group.primary, OpId(uuid(100)));
        assert_eq!(
            group.subsumed,
            vec![OpId(uuid(101)), OpId(uuid(102)), OpId(uuid(103))]
        );
    }

    #[test]
    fn multi_edit_collapse_is_one_full_send() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.title = "edited twice".into();
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::UpdateTask(task.id)),
            op(101, LocalOp::UpdateTask(task.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].primary, OpId(uuid(100)));
        assert_eq!(groups[0].subsumed, vec![OpId(uuid(101))]);
        let MaterializedOp::UpdateTask { card, .. } = &groups[0].materialized else {
            panic!("expected an update materialization");
        };
        assert_eq!(card.title, "edited twice");
    }

    #[test]
    fn scheduled_rides_both_push_shapes_from_the_task() {
        // D-F2: the native `startdate` slot materializes into both the
        // create and the full-send update shapes.
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.scheduled = Some(ts(400));
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let update = vec![op(100, LocalOp::UpdateTask(task.id))];
        let groups = plan_pushes(&update, tables(&tasks, &stacks, &labels));
        let MaterializedOp::UpdateTask { card, .. } = &groups[0].materialized else {
            panic!("expected an update materialization");
        };
        assert_eq!(card.scheduled, Some(ts(400)));

        let create = vec![op(101, LocalOp::CreateTask(task.id))];
        let groups = plan_pushes(&create, tables(&tasks, &stacks, &labels));
        let MaterializedOp::CreateTask { new_card, .. } = &groups[0].materialized else {
            panic!("expected a create materialization");
        };
        assert_eq!(new_card.scheduled, Some(ts(400)));
    }

    #[test]
    fn move_only_group_materializes_a_move() {
        let stack_a = stack_row(1);
        let stack_b = stack_row(2);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack_a.id, stack_a.clone());
        stacks.insert(stack_b.id, stack_b.clone());
        let mut task = task_row(10, stack_b.id);
        task.order = 7;
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![op(100, LocalOp::MoveTask(task.id))];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].materialized,
            MaterializedOp::MoveTask {
                task: task.id,
                to: (stack_b.id, 7)
            }
        );
        assert_eq!(
            groups[0].subsumed.len(),
            0,
            "a bare move has no subsumed ops"
        );
    }

    #[test]
    fn edits_and_moves_split_into_update_then_move_groups() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.title = "renamed".into();
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        // The move was queued first; the two groups keep that order but each
        // materializes exactly one call.
        let outbox = vec![
            op(100, LocalOp::MoveTask(task.id)),
            op(101, LocalOp::UpdateTask(task.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0].materialized,
            MaterializedOp::MoveTask {
                task: task.id,
                to: (stack.id, task.order)
            }
        );
        assert!(matches!(
            groups[1].materialized,
            MaterializedOp::UpdateTask { .. }
        ));
    }

    #[test]
    fn label_only_task_group_is_a_full_send_that_carries_the_label_set() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.label_memberships.insert(
            label_id(20),
            crate::entities::MembershipState::Added { at: ts(0) },
        );
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![op(100, LocalOp::AssignLabel(task.id, label_id(20)))];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        let MaterializedOp::UpdateTask { card, .. } = &groups[0].materialized else {
            panic!("expected an update materialization");
        };
        assert_eq!(card.labels, task.live_labels());
    }

    #[test]
    fn create_plus_delete_of_an_offline_entity_is_a_single_delete_group() {
        let stacks = BTreeMap::new();
        let tasks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::CreateTask(task_id(10))),
            op(101, LocalOp::DeleteTask(task_id(10))),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].materialized,
            MaterializedOp::DeleteTask { task: task_id(10) }
        );
        assert_eq!(groups[0].primary, OpId(uuid(101)));
        assert_eq!(groups[0].subsumed, vec![OpId(uuid(100))]);
    }

    #[test]
    fn ops_on_entities_missing_from_the_tables_degrade_to_noop() {
        let stacks = BTreeMap::new();
        let tasks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::UpdateTask(task_id(10))),
            op(101, LocalOp::RenameStack(stack_id(20))),
            op(102, LocalOp::UpdateLabel(label_id(30))),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 3);
        assert!(
            groups
                .iter()
                .all(|g| g.materialized == MaterializedOp::Noop)
        );
        assert_eq!(all_ids(&groups).len(), 3);
    }

    #[test]
    fn groups_are_ordered_stacks_labels_tasks() {
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let task = task_row(10, stack.id);
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let label = label_row(20, stack.board);
        let mut labels = BTreeMap::new();
        labels.insert(label.id, label.clone());

        // Queue order deliberately reversed: task, label, stack.
        let outbox = vec![
            op(100, LocalOp::UpdateTask(task.id)),
            op(101, LocalOp::UpdateLabel(label.id)),
            op(102, LocalOp::RenameStack(stack.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        let targets: Vec<PushTarget> = groups.iter().map(|g| g.target).collect();
        assert_eq!(
            targets,
            vec![
                PushTarget::Stack(stack.id),
                PushTarget::Label(label.id),
                PushTarget::Task(task.id)
            ]
        );
    }

    #[test]
    fn stack_create_subsumes_later_renames() {
        let mut stacks = BTreeMap::new();
        let stack = stack_row(1);
        stacks.insert(stack.id, stack.clone());
        let tasks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::CreateStack(stack.id)),
            op(101, LocalOp::RenameStack(stack.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));

        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].materialized,
            MaterializedOp::CreateStack { stack: stack.id }
        );
        assert_eq!(groups[0].subsumed, vec![OpId(uuid(101))]);
    }

    #[test]
    fn unbound_stack_rename_still_materializes() {
        let stacks = BTreeMap::new();
        let tasks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![op(100, LocalOp::RenameStack(stack_id(20)))];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));
        // The stack row exists check is against the tables, which are empty:
        // a rename of a stack the planner cannot see degrades to Noop.
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].materialized, MaterializedOp::Noop);
    }

    // ---- chunk 4: the document push group ------------------------------

    fn bound_task_fixture(raw: u128, stack: StackId) -> Task {
        Task {
            id: task_id(raw),
            remote: Some(crate::ids::RemoteCardRef {
                board: crate::ids::RemoteBoardId(7),
                stack: crate::ids::RemoteStackId(1),
                card: crate::ids::RemoteCardId(u64::try_from(raw).unwrap()),
            }),
            title: "t".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order: 0,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,

            archived: false,
            deleted: false,
            clocks: clocks(100),
            remote_seen: Some(ts(100)),
            remote_baseline: None,
        }
    }

    fn stack_fixture(raw: u128, board: crate::ids::BoardId) -> crate::entities::Stack {
        crate::entities::Stack {
            id: stack_id(raw),
            remote: None,
            board,
            title: "s".into(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(100),
                order: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    #[test]
    fn document_ops_coalesce_into_one_group_per_board_ordered_last() {
        let board_a = crate::ids::BoardId::from(uuid(500));
        let board_b = crate::ids::BoardId::from(uuid(501));
        let stack_a = stack_id(900);
        let stack_b = stack_id(901);
        let mut tasks = BTreeMap::new();
        tasks.insert(task_id(1), bound_task_fixture(1, stack_a));
        tasks.insert(task_id(2), bound_task_fixture(2, stack_a));
        tasks.insert(task_id(3), bound_task_fixture(3, stack_b));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack_a, stack_fixture(900, board_a));
        stacks.insert(stack_b, stack_fixture(901, board_b));

        let outbox = vec![
            op(10, LocalOp::UpdateAppData(task_id(1))),
            op(11, LocalOp::UpdateAppData(task_id(3))),
            op(12, LocalOp::UpdateAppData(task_id(2))),
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &EMPTY_LABELS,
                boards: &EMPTY,
            },
        );
        // Two flush groups: one per board, primary first-in-queue-order.
        let documents: Vec<&PushGroup> = groups
            .iter()
            .filter(|g| matches!(g.target, PushTarget::Document(_)))
            .collect();
        assert_eq!(documents.len(), 2);
        let group_a = documents
            .iter()
            .find(|g| matches!(g.materialized, MaterializedOp::FlushAppData { board } if board == board_a))
            .unwrap();
        assert_eq!(group_a.primary, OpId(uuid(10)));
        assert_eq!(group_a.subsumed, vec![OpId(uuid(12))]);
        let group_b = documents
            .iter()
            .find(|g| matches!(g.materialized, MaterializedOp::FlushAppData { board } if board == board_b))
            .unwrap();
        assert_eq!(group_b.primary, OpId(uuid(11)));
        assert_eq!(group_b.subsumed, Vec::new());

        // Last position: behind every native group of the cycle.
        let document_rank = groups
            .iter()
            .position(|g| matches!(g.target, PushTarget::Document(_)))
            .unwrap();
        assert_eq!(document_rank, groups.len() - 2, "flushes are the tail");
    }

    #[test]
    fn document_ops_partition_and_an_unresolvable_op_buckets_into_nothing() {
        let board = crate::ids::BoardId::from(uuid(500));
        let stack = stack_id(900);
        let mut tasks = BTreeMap::new();
        tasks.insert(task_id(1), bound_task_fixture(1, stack));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_fixture(900, board));

        let outbox = vec![
            op(10, LocalOp::UpdateAppData(task_id(1))),
            op(11, LocalOp::UpdateAppData(task_id(99))), // task row wiped
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &EMPTY_LABELS,
                boards: &EMPTY,
            },
        );
        // Partition: the resolvable op appears exactly once, the
        // unresolvable one in no group (no outcome ⇒ stays queued).
        let ids: Vec<OpId> = groups
            .iter()
            .flat_map(|g| std::iter::once(g.primary).chain(g.subsumed.iter().copied()))
            .collect();
        assert_eq!(ids, vec![OpId(uuid(10))]);
    }

    #[test]
    fn flush_groups_come_after_task_create_groups() {
        // The ruling's hard form: entries key on remote card ids, which
        // exist only post-echo — the flush must be the cycle's last
        // group even against creates.
        let board = crate::ids::BoardId::from(uuid(500));
        let stack = stack_id(900);
        let mut tasks = BTreeMap::new();
        tasks.insert(task_id(1), bound_task_fixture(1, stack));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_fixture(900, board));
        let outbox = vec![
            op(10, LocalOp::CreateTask(task_id(2))),
            op(11, LocalOp::UpdateAppData(task_id(1))),
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &EMPTY_LABELS,
                boards: &EMPTY,
            },
        );
        assert_eq!(groups.len(), 2);
        assert!(matches!(groups[0].target, PushTarget::Task(_)));
        assert!(matches!(groups[1].target, PushTarget::Document(_)));
    }

    proptest! {
        #![proptest_config(crate::test_support::proptest_config(256))]

        #[test]
        fn groups_partition_the_outbox(
            state in crate::test_support::persisted_state_strategy(),
        ) {
            // Outbox op ids are unique in production (UUIDv7 per op); the
            // shrinker can collapse generated ids onto one another, which
            // would make "exactly once" ill-defined.
            prop_assume!(
                {
                    let ids: std::collections::HashSet<uuid::Uuid> =
                        state.outbox.iter().map(|op| op.op_id.0).collect();
                    ids.len() == state.outbox.len()
                },
                "unique outbox op ids"
            );
            let tables = EntityTables {
                tasks: &state.tasks,
                stacks: &state.stacks,
                labels: &state.labels,
                boards: &state.boards,
            };
            let groups = plan_pushes(&state.outbox, tables);

            let mut planned: Vec<uuid::Uuid> =
                all_ids(&groups).into_iter().map(|id| id.0).collect();
            planned.sort();
            let mut queued: Vec<uuid::Uuid> =
                state.outbox.iter().map(|op| op.op_id.0).collect();
            queued.sort();
            prop_assert_eq!(planned, queued, "every op id exactly once");

            for group in &groups {
                for id in std::iter::once(&group.primary).chain(group.subsumed.iter()) {
                    prop_assert!(
                        state.outbox.iter().any(|op| op.op_id == *id),
                        "planned ids must come from the outbox"
                    );
                }
            }
        }

        #[test]
        fn group_order_respects_stacks_labels_tasks(
            state in crate::test_support::persisted_state_strategy(),
        ) {
            let tables = EntityTables {
                tasks: &state.tasks,
                stacks: &state.stacks,
                labels: &state.labels,
                boards: &state.boards,
            };
            let groups = plan_pushes(&state.outbox, tables);
            let ranks: Vec<u8> = groups.iter().map(|g| kind_rank(g.target)).collect();
            let mut sorted = ranks.clone();
            sorted.sort_unstable();
            prop_assert_eq!(ranks, sorted, "stacks before labels before tasks");
        }

        #[test]
        fn deletes_subsume_and_a_materialized_op_is_one_of_its_members(
            state in crate::test_support::persisted_state_strategy(),
        ) {
            let tables = EntityTables {
                tasks: &state.tasks,
                stacks: &state.stacks,
                labels: &state.labels,
                boards: &state.boards,
            };
            let groups = plan_pushes(&state.outbox, tables);
            for group in &groups {
                let member_ops: Vec<LocalOp> = state
                    .outbox
                    .iter()
                    .filter(|op| {
                        op.op_id == group.primary || group.subsumed.contains(&op.op_id)
                    })
                    .map(|op| op.op)
                    .collect();
                if member_ops.iter().any(|op| matches!(op, LocalOp::DeleteTask(_))) {
                    assert!(matches!(group.materialized, MaterializedOp::DeleteTask { .. }));
                } else if member_ops.iter().any(|op| matches!(op, LocalOp::DeleteStack(_))) {
                    assert!(matches!(group.materialized, MaterializedOp::DeleteStack { .. }));
                } else if member_ops.iter().any(|op| matches!(op, LocalOp::DeleteLabel(_))) {
                    assert!(matches!(group.materialized, MaterializedOp::DeleteLabel { .. }));
                }
                // The primary is always a member of the group.
                prop_assert!(
                    state
                        .outbox
                        .iter()
                        .any(|op| op.op_id == group.primary),
                    "primary must be an outbox op"
                );
            }
        }
    }

    #[test]
    fn board_delete_dominates_and_subsumes_the_other_board_ops() {
        let mut bound = board_row(1);
        bound.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        let bound_id = bound.id;
        let mut boards = BTreeMap::new();
        boards.insert(bound_id, bound);
        let tasks = BTreeMap::new();
        let stacks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::UpdateBoard(bound_id)),
            op(101, LocalOp::DeleteBoard(bound_id)),
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &labels,
                boards: &boards,
            },
        );
        assert_eq!(groups.len(), 1, "one DELETE subsumes the board's ops");
        let group = &groups[0];
        assert_eq!(
            group.materialized,
            MaterializedOp::DeleteBoard { board: bound_id }
        );
        assert_eq!(group.primary, OpId(uuid(101)));
        assert_eq!(group.subsumed, vec![OpId(uuid(100))]);
    }

    #[test]
    fn board_delete_on_an_unbound_board_is_a_noop_group() {
        let unbound = board_row(2);
        let unbound_id = unbound.id;
        let mut boards = BTreeMap::new();
        boards.insert(unbound_id, unbound);
        let tasks = BTreeMap::new();
        let stacks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![op(110, LocalOp::DeleteBoard(unbound_id))];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &labels,
                boards: &boards,
            },
        );
        assert_eq!(groups.len(), 1);
        assert!(groups[0].materialized.is_noop(), "synthetic completion");
    }

    #[test]
    fn board_update_materializes_only_for_a_bound_live_board() {
        let mut bound = board_row(1);
        bound.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        let unbound = board_row(2);
        let bound_id = bound.id;
        let unbound_id = unbound.id;
        let mut boards = BTreeMap::new();
        boards.insert(bound_id, bound);
        boards.insert(unbound_id, unbound);
        let tasks = BTreeMap::new();
        let stacks = BTreeMap::new();
        let labels = BTreeMap::new();

        let outbox = vec![
            op(100, LocalOp::UpdateBoard(bound_id)),
            op(101, LocalOp::UpdateBoard(unbound_id)),
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &labels,
                boards: &boards,
            },
        );
        assert_eq!(groups.len(), 2);
        assert!(matches!(
            groups
                .iter()
                .find(|g| g.target == PushTarget::Board(bound_id))
                .expect("bound group")
                .materialized,
            MaterializedOp::UpdateBoard { .. }
        ));
        assert!(
            groups
                .iter()
                .find(|g| g.target == PushTarget::Board(unbound_id))
                .expect("unbound group")
                .materialized
                .is_noop(),
            "a local-only board completes synthetically"
        );
    }

    #[test]
    fn board_groups_precede_their_children() {
        let mut board = board_row(1);
        board.remote = crate::test_support::test_binding(crate::ids::RemoteBoardId(42));
        let stack = stack_row(10);
        let board_id = board.id;
        let stack_id = stack.id;
        let mut boards = BTreeMap::new();
        boards.insert(board_id, board);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack_id, stack);
        let tasks = BTreeMap::new();
        let labels = BTreeMap::new();

        // Queue order reversed: the stack op is queued first.
        let outbox = vec![
            op(100, LocalOp::RenameStack(stack_id)),
            op(101, LocalOp::UpdateBoard(board_id)),
        ];
        let groups = plan_pushes(
            &outbox,
            EntityTables {
                tasks: &tasks,
                stacks: &stacks,
                labels: &labels,
                boards: &boards,
            },
        );
        assert_eq!(groups[0].target, PushTarget::Board(board_id));
        assert_eq!(groups[1].target, PushTarget::Stack(stack_id));
    }

    #[test]
    fn plan_pushes_doc_example() {
        // One edited task (two updates coalesce) and one stack rename.
        let stack = stack_row(1);
        let mut stacks = BTreeMap::new();
        stacks.insert(stack.id, stack.clone());
        let mut task = task_row(10, stack.id);
        task.title = "final".into();
        let mut tasks = BTreeMap::new();
        tasks.insert(task.id, task.clone());
        let labels = BTreeMap::new();

        let outbox = vec![
            op(1, LocalOp::UpdateTask(task.id)),
            op(2, LocalOp::UpdateTask(task.id)),
            op(3, LocalOp::RenameStack(stack.id)),
        ];
        let groups = plan_pushes(&outbox, tables(&tasks, &stacks, &labels));
        assert_eq!(groups.len(), 2, "one PUT per coalesced entity");
    }
}
