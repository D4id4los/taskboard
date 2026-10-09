// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared helpers for the storage crate's integration tests.

use taskboard_domain::persistence::{PersistedState, PersistenceAction};

/// The one `apply` batch that persists a whole state (the batching
/// discipline Phase 3's engine will use): entity upserts, outbox appends,
/// validator upserts, and the sync status.
#[must_use]
pub fn actions_from_state(state: &PersistedState) -> Vec<PersistenceAction> {
    let mut actions = Vec::new();
    for board in state.boards.values() {
        actions.push(PersistenceAction::UpsertBoard(board.clone()));
    }
    for stack in state.stacks.values() {
        actions.push(PersistenceAction::UpsertStack(stack.clone()));
    }
    for label in state.labels.values() {
        actions.push(PersistenceAction::UpsertLabel(label.clone()));
    }
    for task in state.tasks.values() {
        actions.push(PersistenceAction::UpsertTask(task.clone()));
    }
    for op in &state.outbox {
        actions.push(PersistenceAction::EnqueueOp(op.clone()));
    }
    for (key, validators) in &state.validators {
        actions.push(PersistenceAction::UpsertValidators(
            *key,
            validators.clone(),
        ));
    }
    actions.push(PersistenceAction::UpsertSyncStatus(state.sync.clone()));
    actions
}
