// SPDX-License-Identifier: MIT OR Apache-2.0
//! Inter-actor message payloads.
//!
//! Every type here is pure data. The channel-bearing *envelopes* (e.g. a
//! persistence `Flush { reply: oneshot::Sender<()> }` barrier for CLI
//! exit) are declared by the actor crates that own the channels — the
//! domain cannot name `tokio::sync::oneshot::Sender`. All of these travel
//! on `tokio::sync::mpsc`/`broadcast` channels verbatim.

use serde::{Deserialize, Serialize};

use crate::entities::Color;
use crate::ids::{LabelId, RemoteBoardId, StackId, TaskId};
use crate::remote::{PushOutcome, RemoteBoardSnapshot};
use crate::state::SyncErrorKind;

/// UI/CLI → State Engine command (mpsc payload).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateCommand {
    /// Create a task in the given stack at the given order.
    CreateTask {
        /// Initial title.
        title: String,
        /// Target stack.
        stack: StackId,
        /// Requested sort order.
        order: i64,
    },
    /// Edit task fields (sparse changeset; untouched fields keep clocks).
    UpdateTask {
        /// Task to edit.
        id: TaskId,
        /// The edits.
        changes: TaskChanges,
    },
    /// Complete or reopen a task.
    SetTaskDone {
        /// Task to change.
        id: TaskId,
        /// `true` = complete (stamp now), `false` = reopen.
        done: bool,
    },
    /// Move a task (stack and/or order — one composite intent).
    MoveTask {
        /// Task to move.
        id: TaskId,
        /// Target stack.
        stack: StackId,
        /// Requested sort order.
        order: i64,
    },
    /// Delete (tombstone) a task.
    DeleteTask {
        /// Task to delete.
        id: TaskId,
    },
    /// Create a stack at the given order on the bound board.
    CreateStack {
        /// Stack title.
        title: String,
        /// Requested sort order.
        order: i64,
    },
    /// Rename a stack.
    RenameStack {
        /// Stack to rename.
        id: StackId,
        /// New title.
        new_title: String,
    },
    /// Delete a stack.
    DeleteStack {
        /// Stack to delete.
        id: StackId,
    },
    /// Create a label on the bound board.
    CreateLabel {
        /// Label title.
        title: String,
        /// Label color.
        color: Color,
    },
    /// Edit a label.
    UpdateLabel {
        /// Label to edit.
        id: LabelId,
        /// The edits.
        changes: LabelChanges,
    },
    /// Delete a label.
    DeleteLabel {
        /// Label to delete.
        id: LabelId,
    },
    /// Attach a label to a task.
    AssignLabel {
        /// Task.
        task: TaskId,
        /// Label.
        label: LabelId,
    },
    /// Remove a label from a task.
    UnassignLabel {
        /// Task.
        task: TaskId,
        /// Label.
        label: LabelId,
    },
    /// User-visible refresh request; the engine forwards it to the sync
    /// actor.
    RequestSync,
}

/// Sparse task edit: `None` = untouched, `Some(_)` = set. Stamp the
/// matching field clock only for `Some` entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskChanges {
    /// New title.
    pub title: Option<String>,
    /// New description.
    pub description: Option<String>,
    /// New due date; `Some(None)` clears it, `None` leaves it untouched.
    /// The inner layer survives serde: an explicit `null` deserializes to
    /// `Some(None)` (clear), an absent field to `None` (untouched).
    #[serde(
        default,
        deserialize_with = "deserialize_double_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub duedate: Option<Option<chrono::DateTime<chrono::Utc>>>,
    /// New archived flag.
    pub archived: Option<bool>,
}

/// Deserializes `T` into `Some(Some(T))`, preserving an explicit `null`
/// as `Some(None)` — needed for the clear-vs-untouched distinction.
#[allow(clippy::option_option)] // clear (`Some(None)`) vs untouched (`None`)
fn deserialize_double_option<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(de).map(Some)
}

/// Sparse label edit: `None` = untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelChanges {
    /// New title.
    pub title: Option<String>,
    /// New color.
    pub color: Option<Color>,
}

/// Global system notification (broadcast payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemEvent {
    /// Network connectivity lost.
    NetworkLost,
    /// Network connectivity restored.
    NetworkRestored,
    /// Graceful shutdown requested.
    Shutdown,
}

/// Engine → UI repaint trigger (broadcast payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineSignal {
    /// The published `AppState` changed; re-render.
    StateUpdated,
}

/// Engine/app → sync actor command (mpsc payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncCommand {
    /// First-run board binding (CLI `boards select`); MVP is single-board.
    SetBoard(RemoteBoardId),
    /// Nudge: run a sync cycle ASAP (after local edits and for `sync`).
    SyncNow,
}

/// Sync actor → engine report (mpsc payload).
#[derive(Debug, Clone, PartialEq)]
pub enum SyncReport {
    /// A cycle finished pulling (and pushing); the engine executes the
    /// merge policy against this data.
    Completed {
        /// Complete remote view of the board at sync time (completeness
        /// contract: absence means the resource is gone server-side).
        snapshot: RemoteBoardSnapshot,
        /// Outcomes of the ops pushed during this cycle.
        pushes: Vec<PushOutcome>,
    },
    /// The cycle failed before producing a snapshot.
    Failed {
        /// Deck-agnostic failure class.
        kind: SyncErrorKind,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn task_changes_roundtrip_through_serde(changes in test_support::task_changes_strategy()) {
            let json = serde_json::to_string(&changes).unwrap();
            let back: TaskChanges = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, changes);
        }

        #[test]
        fn label_changes_roundtrip_through_serde(changes in test_support::label_changes_strategy()) {
            let json = serde_json::to_string(&changes).unwrap();
            let back: LabelChanges = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, changes);
        }
    }

    #[test]
    fn copy_enums_travel_by_value() {
        let events = [SystemEvent::Shutdown, SystemEvent::NetworkLost];
        for _event in events {}
        let commands = [
            SyncCommand::SyncNow,
            SyncCommand::SetBoard(RemoteBoardId(1)),
        ];
        for _command in commands {}
        let signals = [EngineSignal::StateUpdated; 2];
        assert_eq!(signals[0], signals[1]);
    }
}
