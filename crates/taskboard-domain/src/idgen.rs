// SPDX-License-Identifier: MIT OR Apache-2.0
//! Identity seam.
//!
//! Merge and pipeline functions never generate ids: identity is always
//! *data* passed into them. Production wiring uses [`UuidV7Generator`];
//! tests inject a deterministic counter generator
//! (`merge_testutil::CountingIds`). (The time seam lives in
//! [`crate::clock`].)

use uuid::Uuid;

use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::outbox::OpId;

/// Source of fresh local entity ids (`UUIDv7`).
pub trait IdGenerator: Send + Sync + std::fmt::Debug {
    /// A fresh board id.
    fn new_board_id(&self) -> BoardId;
    /// A fresh stack id.
    fn new_stack_id(&self) -> StackId;
    /// A fresh task id.
    fn new_task_id(&self) -> TaskId;
    /// A fresh label id.
    fn new_label_id(&self) -> LabelId;
    /// A fresh outbox-operation id.
    fn new_op_id(&self) -> OpId;
}

/// Production generator: a fresh `Uuid::now_v7()` per id kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7Generator;

impl IdGenerator for UuidV7Generator {
    fn new_board_id(&self) -> BoardId {
        BoardId::from(Uuid::now_v7())
    }

    fn new_stack_id(&self) -> StackId {
        StackId::from(Uuid::now_v7())
    }

    fn new_task_id(&self) -> TaskId {
        TaskId::from(Uuid::now_v7())
    }

    fn new_label_id(&self) -> LabelId {
        LabelId::from(Uuid::now_v7())
    }

    fn new_op_id(&self) -> OpId {
        OpId(Uuid::now_v7())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v7_generator_returns_distinct_correctly_typed_ids() {
        let generator = UuidV7Generator;
        let a = generator.new_task_id();
        let b = generator.new_task_id();
        assert_ne!(a, b, "v7 generation must not repeat");
        let _ = (
            generator.new_board_id(),
            generator.new_stack_id(),
            generator.new_label_id(),
        );
        let _ = generator.new_op_id();
        // v7 = time-ordered: the version nibble is 7.
        assert_eq!(a.as_uuid().get_version_num(), 7);
    }
}
