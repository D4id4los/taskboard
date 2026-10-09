// SPDX-License-Identifier: MIT OR Apache-2.0
//! Deck-agnostic views of what the remote server returned, plus the
//! push-outcome types the merge policy ingests.
//!
//! These are the merge policy's inputs; the sync crate maps its DTOs into
//! them (Phase 4) and is property-tested on that mapping there. The
//! timestamps here are entity-level and (in Deck's case) seconds-resolution:
//! that granularity limit is exactly why the local side carries per-field
//! clocks.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{
    LabelId, RemoteBoardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId,
    RemoteStackRef, StackId, TaskId,
};
use crate::outbox::OpId;
use crate::state::SyncErrorKind;

/// A board as observed on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteBoard {
    /// Remote board id.
    pub id: RemoteBoardId,
    /// Board title.
    pub title: String,
    /// Board color (raw wire string).
    pub color: String,
    /// Archived flag.
    pub archived: bool,
    /// Soft-delete stamp (boards have a real one); `None` = live.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp — the only remote versioning input.
    pub last_modified: DateTime<Utc>,
}

/// A stack as observed on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStack {
    /// Remote location (board context included).
    pub id: RemoteStackRef,
    /// Stack title.
    pub title: String,
    /// Sort position within the board.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Soft-delete stamp; `None` = live.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

/// A label as observed on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLabel {
    /// Remote location (board context included).
    pub id: RemoteLabelRef,
    /// Label title.
    pub title: String,
    /// Label color (raw wire string).
    pub color: String,
    /// Soft-delete stamp; `None` = live.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

/// A task card as observed on the remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTask {
    /// Remote location (board/stack/card context).
    pub id: RemoteCardRef,
    /// Card title.
    pub title: String,
    /// Card description.
    pub description: String,
    /// Due date; `None` = none.
    pub duedate: Option<DateTime<Utc>>,
    /// Completion timestamp; `None` = open.
    pub done: Option<DateTime<Utc>>,
    /// Owning remote stack.
    pub stack: RemoteStackId,
    /// Sort order within the stack.
    pub order: i64,
    /// Attached remote labels.
    pub labels: BTreeSet<RemoteLabelId>,
    /// Archived flag.
    pub archived: bool,
    /// Entity-level, seconds-resolution version stamp — the ONLY remote
    /// versioning input. Epoch 0 on old servers decodes to the minimum
    /// `DateTime`, i.e. loses to everything.
    pub last_modified: DateTime<Utc>,
}

/// The complete remote view of one board at sync time.
///
/// # Completeness contract
///
/// Binding on the Phase 4 sync actor: a [`RemoteBoardSnapshot`] carried in
/// `SyncReport::Completed` is the actor's complete view of the board at
/// sync time — a resource absent from it is absent on the server. The
/// actor satisfies this by fetching the full stacks list whenever anything
/// reports changed (validators gate it: unchanged stacks may be filled
/// from its cached copy). Absence detection (the R3 pull rule) is only as
/// correct as this contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteBoardSnapshot {
    /// The board itself.
    pub board: RemoteBoard,
    /// All stacks of the board.
    pub stacks: Vec<RemoteStack>,
    /// All cards, flattened from all stacks.
    pub tasks: Vec<RemoteTask>,
    /// All labels of the board.
    pub labels: Vec<RemoteLabel>,
}

/// Lookup-only index from remote refs to local ids, derived from the
/// current bound entities (rebuilt O(n) per sync; incremental maintenance
/// is backlogged). Pure data — merge functions receive it, never rebuild
/// it. Build it with [`RemoteIndex::from_state`] (whole [`AppState`]) or
/// [`RemoteIndex::from_bindings`] (explicit pairs).
#[derive(Debug, Default)]
pub struct RemoteIndex {
    /// Task id by full remote card ref.
    pub task_by_ref: HashMap<RemoteCardRef, TaskId>,
    /// Stack id by remote stack ref.
    pub stack_by_ref: HashMap<RemoteStackRef, StackId>,
    /// Label id by remote label ref.
    pub label_by_ref: HashMap<RemoteLabelRef, LabelId>,
}

impl RemoteIndex {
    /// Builds the index from explicit `(remote ref, local id)` binding
    /// pairs of already-bound entities.
    #[must_use]
    pub fn from_bindings(
        tasks: impl Iterator<Item = (RemoteCardRef, TaskId)>,
        stacks: impl Iterator<Item = (RemoteStackRef, StackId)>,
        labels: impl Iterator<Item = (RemoteLabelRef, LabelId)>,
    ) -> Self {
        Self {
            task_by_ref: tasks.collect(),
            stack_by_ref: stacks.collect(),
            label_by_ref: labels.collect(),
        }
    }

    /// Builds the index for the bound entities of an [`AppState`].
    #[must_use]
    pub fn from_state(state: &crate::state::AppState) -> Self {
        Self::from_bindings(
            state
                .tasks
                .iter()
                .filter_map(|(id, t)| Some((t.remote?, *id))),
            state
                .stacks
                .iter()
                .filter_map(|(id, s)| Some((s.remote?, *id))),
            state
                .labels
                .iter()
                .filter_map(|(id, l)| Some((l.remote?, *id))),
        )
    }
}

/// Resolves a remote label to its local id, if the label is already bound.
#[must_use]
pub fn resolve_label(
    index: &RemoteIndex,
    board: RemoteBoardId,
    remote: RemoteLabelId,
) -> Option<LabelId> {
    index
        .label_by_ref
        .get(&RemoteLabelRef {
            board,
            label: remote,
        })
        .copied()
}

/// A pushed operation's outcome, as reported by the sync actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushOutcome {
    /// The outbox op this outcome belongs to.
    pub op: OpId,
    /// What happened.
    pub result: PushResult,
}

/// Result of one push attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushResult {
    /// Server accepted. `echo` is the response body as a remote view where
    /// the endpoint returns one (create/update/delete do; reorder does not).
    Applied {
        /// Server echo of the written resource, if the endpoint returns one.
        echo: Option<RemoteEcho>,
    },
    /// 404/403-shaped "gone" — falls through to the delete rules.
    RemoteMissing,
    /// The write was rejected; the op stays queued (except `BadRequest`,
    /// which dead-letters it).
    Rejected {
        /// Deck-agnostic failure class.
        kind: SyncErrorKind,
    },
}

/// Server echo of a pushed resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteEcho {
    /// Echoed card.
    Task(RemoteTask),
    /// Echoed stack.
    Stack(RemoteStack),
    /// Echoed label.
    Label(RemoteLabel),
}

/// Conditional-read validators for one board pull (phase 4 decision 4).
///
/// Exactly the three conditional endpoints one sync cycle touches. The
/// engine maps these onto `ValidatorKey::{Boards, Stacks(board),
/// ArchivedStacks(board)}` against its own board binding when appending the
/// persistence batch — the sync actor never reasons about storage keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPullValidators {
    /// Validators of the boards listing.
    pub boards: crate::persistence::SyncValidators,
    /// Validators of the board's active-stacks listing.
    pub stacks: crate::persistence::SyncValidators,
    /// Validators of the board's archived-stacks listing.
    pub archived_stacks: crate::persistence::SyncValidators,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;

    /// Random validators for one endpoint (P4 helper).
    fn sync_validators_strategy() -> impl Strategy<Value = crate::persistence::SyncValidators> {
        (
            proptest::option::of("[a-zA-Z0-9\"-]{1,24}"),
            proptest::option::of("[a-zA-Z0-9 :,;-]{5,40}"),
        )
            .prop_map(|(etag, last_modified)| crate::persistence::SyncValidators {
                etag,
                last_modified,
            })
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn remote_views_roundtrip_through_serde(task in test_support::remote_task_strategy()) {
            let json = serde_json::to_string(&task).unwrap();
            let back: RemoteTask = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, task);
        }

        #[test]
        fn snapshot_roundtrips_through_serde(snap in test_support::snapshot_strategy()) {
            let json = serde_json::to_string(&snap).unwrap();
            let back: RemoteBoardSnapshot = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, snap);
        }

        #[test]
        fn push_outcome_roundtrips_through_serde(outcome in test_support::push_outcome_strategy()) {
            let json = serde_json::to_string(&outcome).unwrap();
            let back: PushOutcome = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, outcome);
        }

        #[test]
        fn board_pull_validators_roundtrip_through_serde(
            validators in (sync_validators_strategy(), sync_validators_strategy(), sync_validators_strategy())
                .prop_map(|(boards, stacks, archived_stacks)| BoardPullValidators {
                    boards, stacks, archived_stacks,
                }),
        ) {
            let json = serde_json::to_string(&validators).unwrap();
            let back: BoardPullValidators = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, validators);
        }
    }
}
