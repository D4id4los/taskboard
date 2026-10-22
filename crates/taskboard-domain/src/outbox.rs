// SPDX-License-Identifier: MIT OR Apache-2.0
//! The local outbox: operations created client-side and pending push.
//!
//! Ops are id-scoped and carry no field sets: `TaskClocks` already knows
//! which fields changed, and every Deck write is full-send anyway (the
//! sparse-PUT hazard makes partial payloads useless on the wire). Op
//! *coalescing* (update+delete → delete, etc.) is Phase 2/4 actor/storage
//! logic built on these types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::push::EntityTables;

/// Identity of a pending outbox operation (`UUIDv7`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId(pub Uuid);

/// A local user operation awaiting push to the remote board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LocalOp {
    /// Card create; binds the remote ref on success (R9a).
    CreateTask(TaskId),
    /// Card update (title/description/duedate/done/archived).
    UpdateTask(TaskId),
    /// Card move (stack and/or order).
    MoveTask(TaskId),
    /// Card delete; the tombstone finalizes on success (R9c).
    DeleteTask(TaskId),
    /// Stack create.
    CreateStack(StackId),
    /// Stack rename.
    RenameStack(StackId),
    /// Stack reorder (full-send on push, like [`LocalOp::RenameStack`]).
    MoveStack(StackId),
    /// Stack delete.
    DeleteStack(StackId),
    /// Label create.
    CreateLabel(LabelId),
    /// Label edit (title/color).
    UpdateLabel(LabelId),
    /// Label delete.
    DeleteLabel(LabelId),
    /// Label assignment on a card.
    AssignLabel(TaskId, LabelId),
    /// Label removal from a card.
    UnassignLabel(TaskId, LabelId),
    /// Board edit (title/color/archived — every Deck board write is a
    /// full-send, like [`LocalOp::RenameStack`]).
    UpdateBoard(crate::ids::BoardId),
    /// Board delete; the tombstone finalizes on success (R9c).
    DeleteBoard(crate::ids::BoardId),
    /// App-data document entry for a task is stale: fields and/or
    /// reminder rules. Full-send at flush; coalesces per board into the
    /// single sidecar document upload (D-S5). Nothing enqueues it yet —
    /// the flush lane is chunk 4's.
    UpdateAppData(TaskId),
}

/// An outbox entry: which operation, when it was queued.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOp {
    /// Unique operation id (used by push outcomes to address the op).
    pub op_id: OpId,
    /// The operation itself.
    pub op: LocalOp,
    /// When the operation entered the outbox.
    pub queued_at: DateTime<Utc>,
}

impl LocalOp {
    /// The board that owns this op's entity (board ops: themselves;
    /// stack ops: the stack's board; label ops: the label's board;
    /// task ops: the task's stack's board). `None` when the owning
    /// entity row no longer exists (clear-board content wipes).
    #[must_use]
    pub fn owning_board(&self, entities: &EntityTables<'_>) -> Option<BoardId> {
        match *self {
            Self::UpdateBoard(board) | Self::DeleteBoard(board) => Some(board),
            Self::CreateStack(stack)
            | Self::RenameStack(stack)
            | Self::MoveStack(stack)
            | Self::DeleteStack(stack) => entities.stacks.get(&stack).map(|s| s.board),
            Self::CreateLabel(label) | Self::UpdateLabel(label) | Self::DeleteLabel(label) => {
                entities.labels.get(&label).map(|l| l.board)
            }
            Self::CreateTask(task)
            | Self::UpdateTask(task)
            | Self::MoveTask(task)
            | Self::DeleteTask(task)
            | Self::AssignLabel(task, _)
            | Self::UnassignLabel(task, _)
            | Self::UpdateAppData(task) => entities
                .tasks
                .get(&task)
                .and_then(|t| entities.stacks.get(&t.stack))
                .map(|s| s.board),
        }
    }
}

/// `true` when the pending op addresses the given task: the id-scoping
/// rule for op cancellation, shared by the command layer and the sync
/// pipeline so the two can never drift.
#[must_use]
pub fn op_targets_task(op: &LocalOp, task: TaskId) -> bool {
    match op {
        LocalOp::CreateTask(id)
        | LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::DeleteTask(id)
        | LocalOp::AssignLabel(id, _)
        | LocalOp::UnassignLabel(id, _)
        | LocalOp::UpdateAppData(id) => *id == task,
        LocalOp::CreateStack(_)
        | LocalOp::RenameStack(_)
        | LocalOp::MoveStack(_)
        | LocalOp::DeleteStack(_)
        | LocalOp::CreateLabel(_)
        | LocalOp::UpdateLabel(_)
        | LocalOp::DeleteLabel(_)
        | LocalOp::UpdateBoard(_)
        | LocalOp::DeleteBoard(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::{Board, Label, Stack, Task};
    use crate::push::EntityTables;
    use chrono::TimeZone;
    use std::collections::BTreeMap;

    fn stack_clocks() -> crate::entities::StackClocks {
        let t = ts(100);
        crate::entities::StackClocks {
            title: t,
            order: t,
            deleted: t,
        }
    }

    fn label_clocks() -> crate::entities::LabelClocks {
        let t = ts(100);
        crate::entities::LabelClocks {
            title: t,
            color: t,
            deleted: t,
        }
    }

    fn task_clocks() -> crate::entities::TaskClocks {
        let t = ts(100);
        crate::entities::TaskClocks {
            title: t,
            description: t,
            duedate: t,
            scheduled: t,
            done: t,
            position: t,
            labels: t,
            effort: t,
            importance: t,
            urgency: t,
            archived: t,
            deleted: t,
        }
    }

    fn ts(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn stack_row(raw: u128, board: BoardId, deleted: bool) -> Stack {
        Stack {
            id: StackId::from(Uuid::from_u128(raw)),
            remote: None,
            board,
            title: "stack".into(),
            order: 0,
            archived: false,
            deleted,
            clocks: stack_clocks(),
            remote_seen: None,
        }
    }

    fn label_row(raw: u128, board: BoardId, deleted: bool) -> Label {
        Label {
            id: LabelId::from(Uuid::from_u128(raw)),
            remote: None,
            board,
            title: "label".into(),
            color: crate::entities::Color::new("00ff00"),
            deleted,
            clocks: label_clocks(),
            remote_seen: None,
        }
    }

    fn task_row(raw: u128, stack: StackId, deleted: bool) -> Task {
        Task {
            id: TaskId::from(Uuid::from_u128(raw)),
            remote: None,
            title: "task".into(),
            description: String::new(),
            duedate: None,
            done: None,
            scheduled: None,
            stack,
            order: 0,
            labels: std::collections::BTreeSet::default(),
            effort: None,
            importance: None,
            urgency: None,
            archived: false,
            deleted,
            clocks: task_clocks(),
            remote_seen: None,
        }
    }

    fn tables<'a>(
        tasks: &'a BTreeMap<TaskId, Task>,
        stacks: &'a BTreeMap<StackId, Stack>,
        labels: &'a BTreeMap<LabelId, Label>,
        boards: &'a BTreeMap<BoardId, Board>,
    ) -> EntityTables<'a> {
        EntityTables {
            tasks,
            stacks,
            labels,
            boards,
        }
    }

    #[test]
    fn board_ops_own_their_board_even_without_a_row() {
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        let board = BoardId::from(Uuid::from_u128(9));
        assert_eq!(
            LocalOp::UpdateBoard(board).owning_board(&t),
            Some(board),
            "board ops resolve themselves"
        );
        assert_eq!(LocalOp::DeleteBoard(board).owning_board(&t), Some(board));
    }

    #[test]
    fn stack_ops_resolve_through_the_stack_row() {
        let board = BoardId::from(Uuid::from_u128(1));
        let stack = StackId::from(Uuid::from_u128(2));
        let tombstoned = StackId::from(Uuid::from_u128(3));
        let missing = StackId::from(Uuid::from_u128(4));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_row(2, board, false));
        stacks.insert(tombstoned, stack_row(3, board, true));
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let labels: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        assert_eq!(LocalOp::RenameStack(stack).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteStack(tombstoned).owning_board(&t),
            Some(board),
            "a tombstoned row still resolves (rows survive deletion)"
        );
        assert_eq!(
            LocalOp::CreateStack(missing).owning_board(&t),
            None,
            "a missing row has no owner"
        );
    }

    #[test]
    fn label_ops_resolve_through_the_label_row() {
        let board = BoardId::from(Uuid::from_u128(1));
        let label = LabelId::from(Uuid::from_u128(2));
        let tombstoned = LabelId::from(Uuid::from_u128(3));
        let missing = LabelId::from(Uuid::from_u128(4));
        let mut labels = BTreeMap::new();
        labels.insert(label, label_row(2, board, false));
        labels.insert(tombstoned, label_row(3, board, true));
        let empty: BTreeMap<TaskId, Task> = BTreeMap::new();
        let stacks: BTreeMap<StackId, Stack> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&empty, &stacks, &labels, &boards);
        assert_eq!(LocalOp::UpdateLabel(label).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteLabel(tombstoned).owning_board(&t),
            Some(board)
        );
        assert_eq!(LocalOp::CreateLabel(missing).owning_board(&t), None);
    }

    #[test]
    fn task_ops_resolve_through_the_task_stack_chain() {
        let board = BoardId::from(Uuid::from_u128(1));
        let stack = StackId::from(Uuid::from_u128(2));
        let task = TaskId::from(Uuid::from_u128(3));
        let tombstoned = TaskId::from(Uuid::from_u128(4));
        let stackless = TaskId::from(Uuid::from_u128(5));
        let missing = TaskId::from(Uuid::from_u128(6));
        let mut stacks = BTreeMap::new();
        stacks.insert(stack, stack_row(2, board, false));
        let mut tasks = BTreeMap::new();
        tasks.insert(task, task_row(3, stack, false));
        tasks.insert(tombstoned, task_row(4, stack, true));
        tasks.insert(
            stackless,
            task_row(5, StackId::from(Uuid::from_u128(99)), false),
        );
        let empty: BTreeMap<LabelId, Label> = BTreeMap::new();
        let boards: BTreeMap<BoardId, Board> = BTreeMap::new();
        let t = tables(&tasks, &stacks, &empty, &boards);
        assert_eq!(LocalOp::UpdateTask(task).owning_board(&t), Some(board));
        assert_eq!(
            LocalOp::DeleteTask(tombstoned).owning_board(&t),
            Some(board),
            "a tombstoned task still resolves (its delete push drains)"
        );
        assert_eq!(
            LocalOp::AssignLabel(stackless, LabelId::from(Uuid::from_u128(7))).owning_board(&t),
            None,
            "a task whose stack row is gone has no owner"
        );
        assert_eq!(LocalOp::MoveTask(missing).owning_board(&t), None);
    }

    #[test]
    fn pending_op_roundtrips_through_serde() {
        let op = PendingOp {
            // Fixed from_u128 ids and stamp: the subject is serde symmetry
            // (version-agnostic representation); real v7 generation stays
            // covered by idgen's tests.
            op_id: OpId(Uuid::from_u128(1)),
            op: LocalOp::AssignLabel(
                TaskId::from(Uuid::from_u128(2)),
                LabelId::from(Uuid::from_u128(3)),
            ),
            queued_at: Utc.timestamp_opt(1_700_100_011, 0).unwrap(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: PendingOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }
}
