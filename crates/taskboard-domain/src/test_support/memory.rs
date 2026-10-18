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
    fail_reads: std::sync::atomic::AtomicBool,
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
            fail_reads: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Test seam: while set, every read (`load` and `read_state`) fails
    /// with [`RepositoryError::Unavailable`]; `apply` still succeeds.
    /// Exists to reach the sync actor's read-failure arm, which a healthy
    /// fake can never take.
    pub fn set_read_failure(&self, failing: bool) {
        self.fail_reads
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }

    fn read(&self) -> Result<PersistedState, RepositoryError> {
        if self.fail_reads.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(RepositoryError::Unavailable);
        }
        Ok(self.snapshot())
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
            let mut state = self.read()?;
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
        Box::pin(async { self.read() })
    }
}

#[cfg(test)]
mod read_port_tests {
    use super::*;
    use crate::test_support::contract::block_on;
    use chrono::TimeZone;

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
            queued_at: chrono::Utc.timestamp_opt(1_700_100_021, 0).unwrap(),
        };
        block_on(repo.apply(vec![crate::persistence::PersistenceAction::EnqueueOp(op)]))
            .expect("apply");
        let read = block_on(repo.read_state()).expect("read_state");
        assert_eq!(read, repo.snapshot());
        assert_eq!(read.outbox.len(), 1);
    }

    /// Routing contract extension: with the fail-reads flag set, both read
    /// paths fail `Unavailable` while `apply` keeps succeeding; clearing
    /// the flag restores the reads.
    #[test]
    fn in_memory_read_state_fails_while_the_read_failure_flag_is_set() {
        use crate::persistence::RepositoryError;
        let repo = InMemoryRepository::new();
        repo.set_read_failure(true);
        assert!(matches!(
            block_on(repo.read_state()),
            Err(RepositoryError::Unavailable)
        ));
        assert!(matches!(
            block_on(repo.load()),
            Err(RepositoryError::Unavailable)
        ));
        block_on(repo.apply(vec![])).expect("apply still succeeds");
        repo.set_read_failure(false);
        assert!(block_on(repo.read_state()).is_ok());
        assert!(block_on(repo.load()).is_ok());
    }
}
