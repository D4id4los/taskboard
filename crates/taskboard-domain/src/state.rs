// SPDX-License-Identifier: MIT OR Apache-2.0
//! The canonical application state and its derived views.
//!
//! [`AppState`] is the single source of truth the state engine owns and
//! publishes through `ArcSwap<AppState>`. All public shapes use
//! `BTreeMap` so serialization and merge results are deterministic for
//! equal inputs (convergence property, insta snapshots).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{BoardId, LabelId, StackId, TaskId};

/// The complete, immutable UI-facing application state.
///
/// The state engine publishes snapshots of this type through an
/// `arc_swap::ArcSwap<AppState>`; the UI reads them lock-free. Treat any
/// change to this type's shape as a snapshot-breaking change: `insta`
/// tests will surface the diff.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppState {
    /// Boards. The MVP holds 0..1 entries; the map shape keeps multi-board
    /// support additive instead of a rewrite.
    pub boards: BTreeMap<BoardId, Board>,
    /// Stacks across all boards.
    pub stacks: BTreeMap<StackId, Stack>,
    /// Tasks across all stacks.
    pub tasks: BTreeMap<TaskId, Task>,
    /// Labels across all boards.
    pub labels: BTreeMap<LabelId, Label>,
    /// Current sync status.
    pub sync: SyncStatus,
    /// Time of the last engine mutation from ANY source (local command or
    /// sync ingestion), stamped via the injected `Clock`.
    pub last_updated: Option<DateTime<Utc>>,
}

/// Sync status summary for the UI badge and CLI output.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    /// Current sync phase.
    pub phase: SyncPhase,
    /// Last successful full sync.
    pub last_success: Option<DateTime<Utc>>,
    /// Outbox depth (pending operations).
    pub pending_ops: u32,
}

/// Phase of the sync lifecycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncPhase {
    /// Nothing running; last cycle succeeded (or none ran yet).
    #[default]
    Idle,
    /// A sync cycle is in flight.
    Syncing,
    /// Offline; retries resume when the network returns.
    Offline,
    /// The last cycle failed with the given error class.
    Failed {
        /// Deck-agnostic failure classification.
        last_error: SyncErrorKind,
    },
}

/// Deck-agnostic sync failure classification. The sync actor maps
/// `DeckError` into this (Phase 4); no text payloads (AGENTS §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncErrorKind {
    /// Transport failure, rate limit, or service unavailable — retryable.
    Network,
    /// Unauthorized.
    Auth,
    /// Forbidden.
    Forbidden,
    /// Server-side failure or unresolvable conflict.
    Server,
    /// Our write was rejected as malformed — dead-letters the op.
    BadRequest,
    /// Local data problem (e.g. repository unavailable during sync).
    LocalData,
    /// No sync target is bound yet — a cycle fired before `SetBoard`.
    NoBoard,
}

impl AppState {
    /// Live (non-tombstoned) tasks, in canonical order: `order` ascending,
    /// ties broken by task id ascending — the single ordering every UI and
    /// test uses.
    #[must_use]
    pub fn live_tasks(&self) -> Vec<&Task> {
        Self::sorted_tasks(self.tasks.values().filter(|t| t.is_live()))
    }

    /// Live tasks of one stack, in canonical order.
    #[must_use]
    pub fn tasks_in_stack(&self, stack: StackId) -> Vec<&Task> {
        Self::sorted_tasks(
            self.tasks
                .values()
                .filter(|t| t.is_live() && t.stack == stack),
        )
    }

    /// Canonical ordering: `order` asc, tie-break id asc.
    fn sorted_tasks<'a, I>(tasks: I) -> Vec<&'a Task>
    where
        I: IntoIterator<Item = &'a Task>,
    {
        let mut tasks: Vec<&Task> = tasks.into_iter().collect();
        tasks.sort_by_key(|t| (t.order, t.id));
        tasks
    }
}

#[cfg(test)]
mod tests {
    use crate::ids::TaskId;
    use crate::test_support;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn live_tasks_exclude_tombstones_and_sort_canonically(
            state in test_support::app_state_strategy(),
        ) {
            // Exact-set equality, not a filter: an empty result must be
            // distinguishable from the real answer.
            let mut expected: Vec<TaskId> = state
                .tasks
                .values()
                .filter(|t| t.is_live())
                .map(|t| t.id)
                .collect();
            expected.sort_by_key(|id| {
                let t = &state.tasks[id];
                (t.order, *id)
            });
            let live = state.live_tasks();
            prop_assert_eq!(live.iter().map(|t| t.id).collect::<Vec<_>>(), expected);
            for pair in live.windows(2) {
                prop_assert!(pair[0].order < pair[1].order
                    || (pair[0].order == pair[1].order && pair[0].id < pair[1].id));
            }
        }

        #[test]
        fn tasks_in_stack_only_contains_that_stack(
            state in test_support::app_state_strategy(),
        ) {
            for stack_id in state.stacks.keys() {
                // Exact membership, in canonical order.
                let mut expected: Vec<TaskId> = state
                    .tasks
                    .values()
                    .filter(|t| t.is_live() && t.stack == *stack_id)
                    .map(|t| t.id)
                    .collect();
                expected.sort_by_key(|id| {
                    let t = &state.tasks[id];
                    (t.order, *id)
                });
                let got = state.tasks_in_stack(*stack_id);
                prop_assert_eq!(got.iter().map(|t| t.id).collect::<Vec<_>>(), expected);
                for task in &got {
                    prop_assert_eq!(task.stack, *stack_id);
                }
            }
        }

        #[test]
        fn app_state_serialization_is_deterministic(state in test_support::app_state_strategy()) {
            let first = serde_json::to_string(&state).unwrap();
            let second = serde_json::to_string(&state).unwrap();
            prop_assert_eq!(first, second);
        }
    }
}
