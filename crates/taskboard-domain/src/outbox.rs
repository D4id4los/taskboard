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

use crate::ids::{LabelId, StackId, TaskId};

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
        | LocalOp::UnassignLabel(id, _) => *id == task,
        LocalOp::CreateStack(_)
        | LocalOp::RenameStack(_)
        | LocalOp::MoveStack(_)
        | LocalOp::DeleteStack(_)
        | LocalOp::CreateLabel(_)
        | LocalOp::UpdateLabel(_)
        | LocalOp::DeleteLabel(_)
        | LocalOp::UpdateBoard(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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
