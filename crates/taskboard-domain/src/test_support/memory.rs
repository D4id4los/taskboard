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
        Box::pin(async { Ok(self.snapshot()) })
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
                        state.outbox.push(op);
                    }
                    PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) => {
                        state
                            .outbox
                            .retain(|PendingOp { op_id: id, .. }| *id != op_id);
                    }
                    PersistenceAction::UpsertValidators(key, validators) => {
                        state.validators.insert(key, validators);
                    }
                }
            }
            Ok(())
        })
    }
}
