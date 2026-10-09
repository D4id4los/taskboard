// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reusable test support: property strategies, an in-memory
//! [`TaskRepository`](crate::persistence::TaskRepository) fake, and the
//! repository contract harness.
//!
//! Ships behind the non-default `test-support` feature so later-phase
//! crates (which cannot share dev-dependencies) can depend on it in tests.
//!
//! Layout mirrors the crate: entity strategies live in [`entities`],
//! remote-view/snapshot strategies in [`remote`], state-level strategies
//! in [`state`], and changeset strategies in [`messages`]. The composite
//! strategies assemble from named row structs (not flat tuples) so the
//! referential-consistency logic stays readable; all public functions are
//! re-exported here flat, so consumers import
//! `taskboard_domain::test_support::<name>` regardless of file.

pub mod contract;
pub use memory::InMemoryRepository;
pub mod entities;
pub mod memory;
pub mod messages;
pub mod remote;
pub mod state;

pub use entities::{
    label_clocks_strategy, stack_clocks_strategy, task_clocks_strategy, task_strategy,
};
pub use messages::{label_changes_strategy, state_command_strategy, task_changes_strategy};
pub use remote::{push_outcome_strategy, remote_task_strategy, snapshot_strategy};
pub use state::{app_state_strategy, persisted_state_strategy};

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use uuid::Uuid;

use crate::idgen::IdGenerator;
use crate::ids::{BoardId, LabelId, StackId, TaskId};
use crate::outbox::OpId;

/// Deterministic id source: UUIDs from a monotonically increasing
/// counter. Promoted from the domain's `cfg(test)` fixtures so downstream
/// crates (state engine tests and later) reuse one fake instead of
/// re-declaring it. Ids are stable across a scenario: the n-th id request
/// always yields the same UUID.
#[derive(Debug, Default)]
pub struct CountingIds(AtomicU64);

impl CountingIds {
    /// A fresh counter starting at the first UUID.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn next(&self) -> u128 {
        u128::from(self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }
}

impl IdGenerator for CountingIds {
    fn new_board_id(&self) -> BoardId {
        BoardId::from(Uuid::from_u128(self.next()))
    }

    fn new_stack_id(&self) -> StackId {
        StackId::from(Uuid::from_u128(self.next()))
    }

    fn new_task_id(&self) -> TaskId {
        TaskId::from(Uuid::from_u128(self.next()))
    }

    fn new_label_id(&self) -> LabelId {
        LabelId::from(Uuid::from_u128(self.next()))
    }

    fn new_op_id(&self) -> OpId {
        OpId(Uuid::from_u128(self.next()))
    }
}

/// Bounded, serde-stable strings (no control characters).
pub fn string_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ._/-]{0,32}"
}

/// Logical timestamps across a sane range (1971–2100), seconds resolution.
pub fn timestamp_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (31_536_000_i64..=4_102_444_800_i64).prop_map(|secs| Utc.timestamp_opt(secs, 0).unwrap())
}

macro_rules! id_strategy {
    ($fn_name:ident, $id_type:ty) => {
        /// A random local id (from a random 128-bit UUID).
        pub fn $fn_name() -> impl Strategy<Value = $id_type> {
            any::<u128>().prop_map(|raw| <$id_type>::from(Uuid::from_u128(raw)))
        }
    };
}

id_strategy!(board_id_strategy, BoardId);
id_strategy!(stack_id_strategy, StackId);
id_strategy!(task_id_strategy, TaskId);
id_strategy!(label_id_strategy, LabelId);

/// A random op id.
pub fn op_id_strategy() -> impl Strategy<Value = OpId> {
    any::<u128>().prop_map(|raw| OpId(Uuid::from_u128(raw)))
}

/// A positive remote deck number (0 is never generated: it is the
/// epoch-0 sentinel of old servers).
pub(crate) fn remote_number() -> impl Strategy<Value = u64> {
    1_u64..=1_000_000
}
