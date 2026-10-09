// SPDX-License-Identifier: MIT OR Apache-2.0
//! In-memory [`TaskRepository`] fake: a `std::sync::Mutex` over
//! [`PersistedState`] whose futures are immediately ready — no tokio, no
//! sleeps, Miri-clean. Phase 2's sqlite implementation must satisfy the
//! same contract harness.

use std::sync::Mutex;

use crate::outbox::PendingOp;
use crate::persistence::{
    BoxFuture, PersistedState, PersistenceAction, RepositoryError, TaskRepository,
};

/// Shared in-memory fake. Apply is "atomic" trivially: the whole batch is
/// processed under one lock.
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
            for action in actions {
                match action {
                    PersistenceAction::UpsertBoard(board) => {
                        state.boards.insert(board.id, board);
                    }
                    PersistenceAction::UpsertStack(stack) => {
                        state.stacks.insert(stack.id, stack);
                    }
                    PersistenceAction::UpsertTask(task) => {
                        state.tasks.insert(task.id, task);
                    }
                    PersistenceAction::UpsertLabel(label) => {
                        state.labels.insert(label.id, label);
                    }
                    PersistenceAction::EnqueueOp(op) => {
                        // Re-enqueueing an existing id replaces the entry in
                        // place and keeps its queue position (parity with the
                        // sqlite upsert on `op_id`, contract-pinned).
                        match state.outbox.iter_mut().find(|e| e.op_id == op.op_id) {
                            Some(slot) => *slot = op,
                            None => state.outbox.push(op),
                        }
                    }
                    PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) => {
                        state
                            .outbox
                            .retain(|PendingOp { op_id: id, .. }| *id != op_id);
                    }
                    PersistenceAction::UpsertValidators(key, validators) => {
                        state.validators.insert(key, validators);
                    }
                    PersistenceAction::UpsertSyncStatus(status) => {
                        state.sync.phase = status.phase;
                        state.sync.last_success = status.last_success;
                    }
                }
            }
            Ok(())
        })
    }
}
