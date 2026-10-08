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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_op_roundtrips_through_serde() {
        let op = PendingOp {
            op_id: OpId(Uuid::now_v7()),
            op: LocalOp::AssignLabel(TaskId::from(Uuid::now_v7()), LabelId::from(Uuid::now_v7())),
            queued_at: Utc::now(),
        };
        let json = serde_json::to_string(&op).unwrap();
        let back: PendingOp = serde_json::from_str(&json).unwrap();
        assert_eq!(back, op);
    }
}
