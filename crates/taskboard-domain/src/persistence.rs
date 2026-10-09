// SPDX-License-Identifier: MIT OR Apache-2.0
//! The persistence port and its data contract.
//!
//! The port is async-shaped without any async-runtime dependency: methods
//! return [`BoxFuture`] (the same hand-rolled pattern the sync crate uses
//! for its `RetrySleep` seam). One batched write side (`apply`) keeps the
//! contract surface small and stable; Phase 2 wraps the batch in a single
//! sqlite transaction.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::entities::{Board, Label, Stack};
use crate::outbox::{OpId, PendingOp};
use crate::state::{AppState, SyncStatus};

/// Hand-rolled boxed future alias: the domain may not depend on
/// `futures`, `tokio`, or `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Local disk-cache port (implemented by `taskboard-storage-sqlite` in
/// Phase 2; faked by `taskboard_domain::test_support::InMemoryRepository`).
pub trait TaskRepository: Send + Sync + std::fmt::Debug {
    /// Full hydration for boot. Returns an empty/default state for a fresh
    /// database.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Unavailable`] while the backend is unreachable;
    /// [`RepositoryError::Corrupted`] when persisted data fails to decode.
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;

    /// Apply a batch atomically: entity upserts and outbox/validator
    /// transitions land together or fail together.
    ///
    /// # Errors
    ///
    /// [`RepositoryError::Unavailable`] while the backend is unreachable.
    fn apply(&self, actions: Vec<PersistenceAction>) -> BoxFuture<'_, Result<(), RepositoryError>>;
}

/// Persistence-layer failure classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RepositoryError {
    /// Backend unreachable/locked/busy — transient, caller may retry.
    #[error("repository unavailable")]
    Unavailable,
    /// Persisted data failed integrity/decoding — not retryable.
    #[error("repository data corrupted")]
    Corrupted,
}

/// The full persisted dataset, hydrated at boot.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PersistedState {
    /// Boards.
    pub boards: BTreeMap<crate::ids::BoardId, Board>,
    /// Stacks.
    pub stacks: BTreeMap<crate::ids::StackId, Stack>,
    /// Tasks.
    pub tasks: BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    /// Labels.
    pub labels: BTreeMap<crate::ids::LabelId, Label>,
    /// Pending outbox operations, in queue order.
    pub outbox: Vec<PendingOp>,
    /// Opaque conditional-read validators (mirror of the sync crate's
    /// `Validators`), persisted so polls survive restarts.
    pub validators: BTreeMap<ValidatorKey, SyncValidators>,
    /// Last-known sync status.
    pub sync: SyncStatus,
}

impl From<PersistedState> for AppState {
    fn from(persisted: PersistedState) -> Self {
        Self {
            boards: persisted.boards,
            stacks: persisted.stacks,
            tasks: persisted.tasks,
            labels: persisted.labels,
            sync: persisted.sync,
            last_updated: None,
        }
    }
}

/// Which remote resource a validator bundle belongs to.
///
/// Serializes as a string (`"boards"`, `"stacks:<remote-board-id>"`) so it
/// can act as a JSON/YAML map key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ValidatorKey {
    /// The boards listing endpoint.
    Boards,
    /// The stacks listing endpoint of one board.
    Stacks(crate::ids::RemoteBoardId),
}

impl Serialize for ValidatorKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Boards => serializer.serialize_str("boards"),
            Self::Stacks(board) => serializer.serialize_str(&format!("stacks:{}", board.get())),
        }
    }
}

impl<'de> Deserialize<'de> for ValidatorKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        if raw == "boards" {
            return Ok(Self::Boards);
        }
        raw.strip_prefix("stacks:")
            .and_then(|num| num.parse::<u64>().ok())
            .map(|num| Self::Stacks(crate::ids::RemoteBoardId(num)))
            .ok_or_else(|| serde::de::Error::custom("invalid validator key"))
    }
}

/// Opaque conditional-read validators for one endpoint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncValidators {
    /// `ETag` header value.
    pub etag: Option<String>,
    /// `Last-Modified` header value.
    pub last_modified: Option<String>,
}

/// One write the engine asks the repository to persist. Entity upserts and
/// outbox/validator transitions are batched so they can land in one
/// transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PersistenceAction {
    /// Insert or replace a board.
    UpsertBoard(Board),
    /// Insert or replace a stack.
    UpsertStack(Stack),
    /// Insert or replace a task.
    UpsertTask(crate::entities::Task),
    /// Insert or replace a label.
    UpsertLabel(Label),
    /// Append an operation to the outbox.
    EnqueueOp(PendingOp),
    /// Remove a completed (successfully pushed) op.
    CompleteOp(OpId),
    /// Remove a dead-lettered / cancelled op.
    FailOp(OpId),
    /// Store validators for one endpoint.
    UpsertValidators(ValidatorKey, SyncValidators),
    /// Persist the sync status (phase + last success). `pending_ops` is NOT
    /// stored: implementations derive it at load time from the outbox depth,
    /// so a stored counter can never drift from the queue it summarizes.
    UpsertSyncStatus(SyncStatus),
}

/// Applies a batch of [`PersistenceAction`]s to a [`PersistedState`] —
/// the executable definition of the port's batch contract, shared by the
/// state engine's memory advance and the repository implementations
/// (fakes and sqlite alike). One semantics, enforced once.
///
/// Per-action semantics: entity upserts replace by id; [`PersistenceAction::
/// EnqueueOp`] appends (an in-place replace on id collision mirrors the
/// sqlite upsert on `op_id` — contract-pinned); `CompleteOp`/`FailOp`
/// remove by op id; `UpsertValidators` replaces the key;
/// `UpsertSyncStatus` sets phase + last success only (`pending_ops` is
/// never stored). A final derivation re-computes `sync.pending_ops` from
/// the outbox depth, so the counter can never drift from the queue it
/// summarizes (both this function and `load()` derive it).
pub fn apply_actions(state: &mut PersistedState, actions: &[PersistenceAction]) {
    for action in actions {
        match action {
            PersistenceAction::UpsertBoard(board) => {
                state.boards.insert(board.id, board.clone());
            }
            PersistenceAction::UpsertStack(stack) => {
                state.stacks.insert(stack.id, stack.clone());
            }
            PersistenceAction::UpsertTask(task) => {
                state.tasks.insert(task.id, task.clone());
            }
            PersistenceAction::UpsertLabel(label) => {
                state.labels.insert(label.id, label.clone());
            }
            PersistenceAction::EnqueueOp(op) => {
                // Re-enqueueing an existing id replaces the entry in place
                // and keeps its queue position (parity with the sqlite
                // upsert on `op_id`, contract-pinned).
                match state.outbox.iter_mut().find(|e| e.op_id == op.op_id) {
                    Some(slot) => *slot = op.clone(),
                    None => state.outbox.push(op.clone()),
                }
            }
            PersistenceAction::CompleteOp(op_id) | PersistenceAction::FailOp(op_id) => {
                state.outbox.retain(|entry| entry.op_id != *op_id);
            }
            PersistenceAction::UpsertValidators(key, validators) => {
                state.validators.insert(*key, validators.clone());
            }
            PersistenceAction::UpsertSyncStatus(status) => {
                state.sync.phase = status.phase;
                state.sync.last_success = status.last_success;
            }
        }
    }
    // Derived, never tracked: after every batch the pending count is
    // re-derived from the outbox it summarizes.
    state.sync.pending_ops = u32::try_from(state.outbox.len()).unwrap_or(u32::MAX);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    use chrono::{DateTime, TimeZone, Utc};

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task_with_id(raw: u128) -> crate::entities::Task {
        let id = crate::ids::TaskId::from(uuid::Uuid::from_u128(raw));
        crate::entities::Task {
            id,
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate: None,
            done: None,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order: 0,
            labels: BTreeSet::default(),
            archived: false,
            deleted: false,
            clocks: crate::entities::TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                done: ts(100),
                position: ts(100),
                labels: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
        }
    }

    fn op(raw: u128, target: crate::ids::TaskId) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(raw)),
            op: crate::outbox::LocalOp::UpdateTask(target),
            queued_at: ts(200),
        }
    }

    #[test]
    fn entity_upserts_replace_by_id() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        apply_actions(&mut state, &[PersistenceAction::UpsertTask(task.clone())]);
        let mut renamed = task.clone();
        renamed.title = "renamed".into();
        apply_actions(&mut state, &[PersistenceAction::UpsertTask(renamed)]);
        assert_eq!(state.tasks.len(), 1, "replace, not append");
        assert_eq!(state.tasks[&task.id].title, "renamed");
    }

    #[test]
    fn enqueue_appends_and_reenqueue_keeps_queue_position() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        let first = op(10, task.id);
        let second = op(11, task.id);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(first.clone()),
                PersistenceAction::EnqueueOp(second.clone()),
            ],
        );
        assert_eq!(state.outbox, vec![first.clone(), second.clone()]);

        // Same id, new payload: replaces in place, keeps its position.
        let mut replaced = first.clone();
        replaced.queued_at = ts(300);
        apply_actions(
            &mut state,
            &[PersistenceAction::EnqueueOp(replaced.clone())],
        );
        assert_eq!(state.outbox, vec![replaced, second]);
    }

    #[test]
    fn complete_and_fail_remove_by_op_id() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        let a = op(10, task.id);
        let b = op(11, task.id);
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(a.clone()),
                PersistenceAction::EnqueueOp(b.clone()),
                PersistenceAction::CompleteOp(a.op_id),
                PersistenceAction::FailOp(b.op_id),
            ],
        );
        assert!(state.outbox.is_empty(), "both removals land");
    }

    #[test]
    fn validators_upsert_and_status_write_partial_fields() {
        let mut state = PersistedState::default();
        let key = ValidatorKey::Boards;
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertValidators(
                key,
                SyncValidators {
                    etag: Some("v1".into()),
                    last_modified: None,
                },
            )],
        );
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertValidators(
                key,
                SyncValidators {
                    etag: None,
                    last_modified: Some("lm".into()),
                },
            )],
        );
        assert_eq!(
            state.validators[&key],
            SyncValidators {
                etag: None,
                last_modified: Some("lm".into())
            },
            "upsert replaces the whole bundle"
        );

        state.sync.pending_ops = 7;
        apply_actions(
            &mut state,
            &[PersistenceAction::UpsertSyncStatus(SyncStatus {
                phase: crate::state::SyncPhase::Offline,
                last_success: Some(ts(400)),
                pending_ops: 9999, // ignored: derived, never stored
            })],
        );
        assert_eq!(state.sync.phase, crate::state::SyncPhase::Offline);
        assert_eq!(state.sync.last_success, Some(ts(400)));
        assert_eq!(
            state.sync.pending_ops, 0,
            "the counter is re-derived from the (empty) outbox after the batch"
        );
    }

    #[test]
    fn pending_ops_are_rederived_from_the_outbox_after_every_batch() {
        let mut state = PersistedState::default();
        let task = task_with_id(1);
        state.sync.pending_ops = 42;
        apply_actions(
            &mut state,
            &[
                PersistenceAction::EnqueueOp(op(10, task.id)),
                PersistenceAction::EnqueueOp(op(11, task.id)),
            ],
        );
        assert_eq!(state.sync.pending_ops, 2);
        apply_actions(
            &mut state,
            &[PersistenceAction::CompleteOp(OpId(uuid::Uuid::from_u128(
                10,
            )))],
        );
        assert_eq!(state.sync.pending_ops, 1);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn empty_batch_is_identity(state in test_support::persisted_state_strategy()) {
            // The strategy may generate a raw `pending_ops` that contradicts
            // its own outbox; identity is claimed on invariant-satisfying
            // states, so normalize the counter before comparing.
            let mut expected = state.clone();
            expected.sync.pending_ops =
                u32::try_from(expected.outbox.len()).unwrap_or(u32::MAX);
            let mut got = state;
            apply_actions(&mut got, &[]);
            prop_assert_eq!(got, expected);
        }
    }

    #[test]
    fn invalid_validator_key_string_is_rejected() {
        assert!(serde_json::from_str::<ValidatorKey>("\"not-a-key\"").is_err());
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn persisted_state_roundtrips_through_serde(state in test_support::persisted_state_strategy()) {
            let json = serde_json::to_string(&state).unwrap();
            let back: PersistedState = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, state);
        }

        #[test]
        fn persisted_state_maps_into_app_state(state in test_support::persisted_state_strategy()) {
            let app: AppState = state.clone().into();
            prop_assert_eq!(app.boards, state.boards);
            prop_assert_eq!(app.stacks, state.stacks);
            prop_assert_eq!(app.tasks, state.tasks);
            prop_assert_eq!(app.labels, state.labels);
            prop_assert_eq!(app.sync, state.sync);
            prop_assert!(app.last_updated.is_none());
        }

        #[test]
        fn board_id_used_as_validator_key_roundtrips(b in any::<u64>()) {
            let key = ValidatorKey::Stacks(crate::ids::RemoteBoardId(b));
            let back: ValidatorKey =
                serde_json::from_str(&serde_json::to_string(&key).unwrap()).unwrap();
            prop_assert_eq!(back, key);
        }
    }
}
