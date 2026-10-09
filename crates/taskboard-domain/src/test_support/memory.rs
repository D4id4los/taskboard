// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-memory [`TaskRepository`] fake: a `std::sync::Mutex` over
//! [`PersistedState`] whose futures are immediately ready — no tokio, no
//! sleeps, Miri-clean. Phase 2's sqlite implementation must satisfy the
//! same contract harness.

use std::sync::Mutex;

use crate::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, SyncStateReader, TaskRepository,
    apply_actions,
};

/// Shared in-memory fake. Apply is "atomic" trivially: the whole batch is
/// processed under one lock. The action semantics are *not* re-implemented
/// here: the fake delegates to [`apply_actions`], the same function the
/// state engine uses to advance memory — one executable definition of the
/// port contract, which is exactly the point.
#[derive(Debug, Default)]
pub struct InMemoryRepository {
    state: Mutex<PersistedState>,
}

impl InMemoryRepository {
    /// A fake starting empty (equivalent to a fresh database).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A fake pre-loaded with the given state.
    #[must_use]
    pub fn with_state(state: PersistedState) -> Self {
        Self {
            state: Mutex::new(state),
        }
    }

    /// Snapshot of the current fake contents (test assertion helper).
    ///
    /// # Panics
    ///
    /// Panics if the internal lock is poisoned (i.e. a previous panic).
    #[must_use]
    pub fn snapshot(&self) -> PersistedState {
        self.state.lock().expect("poisoned").clone()
    }
}

impl TaskRepository for InMemoryRepository {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async {
            let mut state = self.snapshot();
            // `pending_ops` is derived, never stored (parity with the sqlite
            // implementation): the outbox is the single source of truth.
            state.sync.pending_ops = u32::try_from(state.outbox.len()).unwrap_or(u32::MAX);
            Ok(state)
        })
    }

    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>> {
        Box::pin(async move {
            let mut state = self.state.lock().expect("poisoned");
            apply_actions(&mut state, &actions);
            Ok(())
        })
    }
}

/// The fake doubles as the sync actor's read port, so every test layer
/// (engine and actor) shares one source of truth: `read_state` returns the
/// same post-apply snapshot `load` would.
impl SyncStateReader for InMemoryRepository {
    fn read_state(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>> {
        Box::pin(async { Ok(self.snapshot()) })
    }
}

#[cfg(test)]
mod read_port_tests {
    use super::*;
    use crate::test_support::contract::block_on;

    /// Routing contract (one per impl): `read_state` returns the post-apply
    /// persisted shape, identical to `load`.
    #[test]
    fn in_memory_read_state_returns_the_post_apply_shape() {
        let repo = InMemoryRepository::new();
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(1)),
            op: crate::outbox::LocalOp::CreateStack(crate::ids::StackId::from(
                uuid::Uuid::from_u128(2),
            )),
            queued_at: chrono::Utc::now(),
        };
        block_on(repo.apply(vec![crate::persistence::PersistenceAction::EnqueueOp(op)]))
            .expect("apply");
        let read = block_on(repo.read_state()).expect("read_state");
        assert_eq!(read, repo.snapshot());
        assert_eq!(read.outbox.len(), 1);
    }
}
