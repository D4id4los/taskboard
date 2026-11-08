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
    BoardId, LabelId, RemoteBoardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId,
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
    /// Scheduled start (Deck's native `startdate`); `None` = none.
    pub scheduled: Option<DateTime<Utc>>,
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
    /// Server-side creation stamp (#150): the crash-window duplicate
    /// detection's window input. Same sentinel rule as `last_modified` —
    /// epoch 0 decodes to the minimum `DateTime`, which fails the
    /// detection window and degrades to today's adopt-as-new behavior.
    pub created_at: DateTime<Utc>,
}

/// The complete remote view of one board at sync time, in its
/// backend's shape (D-BF4: the payload itself is the tag — no sidecar
/// discriminator field exists).
///
/// # Completeness contract
///
/// Binding on the sync actor: a [`RemoteBoardSnapshot`] carried in
/// `SyncReport::Completed` is the actor's complete view of the board at
/// sync time — a resource absent from it is absent on the server. The
/// actor satisfies this by fetching the full stacks list whenever
/// anything reports changed (validators gate it: unchanged stacks may
/// be filled from its cached copy). Absence detection (the R3 pull
/// rule) is only as correct as this contract.
///
/// # Serde form (deliberate leniency)
///
/// Untagged, like the id family: the `Deck` leg serializes as the bare
/// struct — byte-identical with the pre-widening report face — and a
/// historical bare-struct payload decodes as `Deck`. The two legs'
/// shapes are disjoint (a files snapshot is identity-only in chunk 3),
/// so the decode is total and never ambiguous.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RemoteBoardSnapshot {
    /// Deck's full stacks/cards/labels view (the pre-widening struct
    /// verbatim; the Deck actor's payload).
    Deck(DeckBoardSnapshot),
    /// The files substrate: identity-only in chunk 3 — chunk 5 grows
    /// the payload (folded board sections), never this chunk.
    Files(FilesBoardSnapshot),
}

impl RemoteBoardSnapshot {
    /// The Deck leg; `None` for a `Files` payload. The Deck lane's
    /// read-side accessor — callers that hold a Deck connector's report
    /// match or expect, never assume.
    #[must_use]
    pub const fn as_deck(&self) -> Option<&DeckBoardSnapshot> {
        match self {
            Self::Deck(snapshot) => Some(snapshot),
            Self::Files(_) => None,
        }
    }

    /// The files leg; `None` for a `Deck` payload.
    #[must_use]
    pub const fn as_files(&self) -> Option<&FilesBoardSnapshot> {
        match self {
            Self::Files(snapshot) => Some(snapshot),
            Self::Deck(_) => None,
        }
    }
}

/// The Deck-shaped snapshot body (the pre-widening `RemoteBoardSnapshot`
/// struct, renamed so the enum owns the name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckBoardSnapshot {
    /// The board itself.
    pub board: RemoteBoard,
    /// All stacks of the board.
    pub stacks: Vec<RemoteStack>,
    /// All cards, flattened from all stacks.
    pub tasks: Vec<RemoteTask>,
    /// All labels of the board.
    pub labels: Vec<RemoteLabel>,
}

/// The files-substrate snapshot body (D-BF4: the remote identity *is*
/// the domain UUID — there is nothing else to carry until chunk 5's
/// folded sections arrive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesBoardSnapshot {
    /// The board's remote identity: the domain UUID by construction.
    pub board: RemoteBoardId,
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
    /// Board id by remote board id (echo adoption of a board rename).
    pub board_by_ref: HashMap<RemoteBoardId, BoardId>,
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
            board_by_ref: HashMap::new(),
        }
    }

    /// Builds the index for the bound entities of an [`AppState`].
    #[must_use]
    pub fn from_state(state: &crate::state::AppState) -> Self {
        let mut index = Self::from_bindings(
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
        );
        index.board_by_ref = state
            .boards
            .iter()
            .filter_map(|(id, b)| Some((b.remote.as_ref()?.board, *id)))
            .collect();
        index
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
///
/// Grouped per push group by the sync actor's `GroupOutcome`
/// (`taskboard-sync-nextcloud`) — per-group abort/landed bookkeeping
/// around these per-op results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PushOutcome {
    /// The outbox op this outcome belongs to.
    pub op: OpId,
    /// What happened.
    pub result: PushResult,
}

/// Result of one push attempt.
//
// The echo variant dominates the enum's size (a full card echo, the
// R9 echo-adoption input); the widened id payloads grew it past the
// lint's ratio. The outcomes live in short-lived per-report vectors,
// so boxing the echo would not buy anything worth the deref noise.
#[allow(clippy::large_enum_variant)]
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
    /// Echoed board.
    Board(RemoteBoard),
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
        fn files_snapshot_roundtrips_through_serde(board in any::<u128>()) {
            let snap = RemoteBoardSnapshot::Files(FilesBoardSnapshot {
                board: crate::ids::RemoteBoardId::files(uuid::Uuid::from_u128(board)),
            });
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

    /// T3 — the byte-compat pin: the pre-widening `RemoteBoardSnapshot`
    /// struct's serialized face (the bare struct, no envelope) decodes
    /// into the tagged enum as the `Deck` leg, unchanged. The historical
    /// report bytes — journal rows, in-flight payloads — survive the
    /// widening verbatim.
    #[test]
    fn pre_widening_snapshot_bytes_decode_as_the_deck_leg() {
        let old = serde_json::json!({
            "board": {
                "id": 42,
                "title": "golden",
                "color": "3366cc",
                "archived": false,
                "deleted_at": null,
                "last_modified": "1970-01-01T00:00:00Z",
            },
            "stacks": [],
            "tasks": [],
            "labels": [],
        });
        let snap: RemoteBoardSnapshot = serde_json::from_value(old).expect("old bytes decode");
        let RemoteBoardSnapshot::Deck(deck) = snap else {
            panic!("the bare struct must decode as the Deck leg");
        };
        assert_eq!(deck.board.id, crate::ids::RemoteBoardId::deck(42));
    }
}
