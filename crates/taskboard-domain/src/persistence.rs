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
use crate::ops::{OpId, PendingOp};
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;

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
