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
use crate::ids::{BoardId, LabelId, RemoteBoardId, StackId, TaskId};
use crate::reminders::ReminderRef;
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
    /// Create a stack at the given order on the named board.
    CreateStack {
        /// Target board.
        board: BoardId,
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
    /// Reorder a stack on the bound board (no-op when the order is
    /// unchanged; pushes as a full-send like [`StateCommand::RenameStack`]).
    MoveStack {
        /// Stack to reorder.
        id: StackId,
        /// Requested sort order within the board.
        order: i64,
    },
    /// Create a label on the named board.
    CreateLabel {
        /// Target board.
        board: BoardId,
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
    /// Create a local board (always mints; titles are not uniqueness
    /// keys). The stand-alone first-run command; carries no outbox op —
    /// server-side creation is the `publish` ceremony's job.
    CreateBoard {
        /// Board title.
        title: String,
        /// Board color (lenient; validated at the CLI boundary).
        color: Color,
    },
    /// Attach the named local board to an existing remote board. The next
    /// pull matches by this binding and merges instead of adopting a
    /// duplicate; the outbox then drains onto the remote board.
    BindBoard {
        /// The local board to bind.
        board: BoardId,
        /// The remote board to bind to.
        remote: RemoteBoardId,
    },
    /// Rename the named board's title (accepted no-op when unchanged;
    /// pushes as a full-send when the board is bound).
    RenameBoard {
        /// Target board.
        board: BoardId,
        /// New title.
        title: String,
    },
    /// Set the named board's color (accepted no-op when unchanged; pushes
    /// as a full-send when the board is bound).
    SetBoardColor {
        /// Target board.
        board: BoardId,
        /// The new color (lenient domain value; boundary layers validate).
        color: Color,
    },
    /// Archive or unarchive the named board (one verb, both directions;
    /// accepted no-op when unchanged). Archived boards stay commandable.
    /// Pushes as a full-send when the board is bound.
    SetBoardArchived {
        /// Target board.
        board: BoardId,
        /// `true` = archive, `false` = unarchive.
        archived: bool,
    },
    /// Delete the named board: a local tombstone ceremony that
    /// cascades to its live stacks, tasks, and labels, cancels the
    /// pending ops of the doomed entities, and queues one board-delete
    /// op (uniformly, bound or not — the planner noop-completes an
    /// unbound board's group). Repeat is an accepted no-op.
    DeleteBoard {
        /// The board to delete.
        board: BoardId,
    },
    /// Clear the named board's local content (stacks, tasks, labels,
    /// outbox entries, sync validators, reminder facts) in one
    /// local-only repair action; the board row and the connect session
    /// survive. A bound board re-materializes from the next pull under
    /// fresh local ids. Already-empty is an accepted no-op.
    ResetBoard {
        /// The board to reset.
        board: BoardId,
    },
    /// Snooze a fired reminder occurrence: reschedule it to
    /// `now + postpone` (engine-stamped via the injected clock). The
    /// occurrence is opaque link data — a stale link is inert by map
    /// semantics, so no effective-occurrence check is made.
    SnoozeReminder {
        /// Which reminder was snoozed.
        reminder: ReminderRef,
        /// The occurrence being snoozed.
        occurrence: chrono::DateTime<chrono::Utc>,
        /// How far to push the occurrence out (must be positive).
        postpone: chrono::TimeDelta,
    },
    /// Mute (`true`) or unmute (`false`) all firings of a task. The
    /// overdue banner is unaffected (board truth, not nagging policy).
    SetTaskRemindersMuted {
        /// The task whose firings are muted or unmuted.
        task: TaskId,
        /// `true` = mute, `false` = unmute.
        muted: bool,
    },
    /// Set (replace) the task's explicit reminder rules with one rule:
    /// the task's existing rules are deleted and the new one upserted.
    /// Legal on a task without a duedate (the rule arms when a duedate
    /// arrives — occurrence arithmetic is due-anchored).
    SetReminderRule {
        /// The task whose rules are replaced.
        task: TaskId,
        /// The single new rule's due-anchored offset.
        offset: crate::reminders::ReminderOffset,
    },
    /// Remove all of the task's explicit reminder rules. Zero existing
    /// rules is an accepted no-op (idempotent repeat).
    ClearReminderRules {
        /// The task whose rules are removed.
        task: TaskId,
    },
    /// Detach the named board's remote binding: the board row, its
    /// content, and its queued ops survive (the ops strand honestly and
    /// drain on a re-bind); only the binding and the board's sync-status
    /// row go. Tombstoned boards refuse (the R9c tombstone push needs
    /// the binding until it finalizes); an already-unbound board is an
    /// accepted no-op.
    UnbindBoard {
        /// The board to unbind.
        board: BoardId,
    },
    /// Deterministically merge same-title duplicate entities the bind
    /// ceremony's pull adopted alongside pre-existing local-only rows
    /// (survivor keeps the local id and gains the remote ref; the
    /// duplicate row is neutralized as a ref-less tombstone). Idempotent:
    /// a second pass over settled state is an accepted no-op.
    ReconcileByTitle {
        /// The board whose entities reconcile.
        board: BoardId,
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
    /// A persistence write failed and was rejected (memory unchanged);
    /// first failure since the last success — the banner signal for the
    /// UI, not an error report (the command reply carries the typed
    /// repository error).
    StorageFailed,
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
    /// Connector-only detach (WI-H): the actor finishes any in-flight
    /// cycle, then returns — dropping its client (the in-memory
    /// credential) and its reports sender. Never the `Shutdown`
    /// broadcast, which drains the whole engine; this stops the
    /// connector only (ADR 0012 amendment).
    Detach,
}

/// Sync actor → engine report (mpsc payload).
///
/// Evidence-then-verdict (phase 4 decision 3): a `Failed` cycle still
/// carries the push outcomes that *completed* before the abort, so the
/// engine lands them (ops complete, create echoes bind) instead of
/// re-POSTing completed creates next cycle — Deck has no idempotency keys.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncReport {
    /// A cycle finished pushing and pulling; the engine executes the merge
    /// policy against this data.
    Completed {
        /// Complete remote view of the board at sync time (completeness
        /// contract: absence means the resource is gone server-side).
        /// Boxed: a snapshot dwarfs the other variants and reports travel
        /// by value over mpsc.
        snapshot: Box<RemoteBoardSnapshot>,
        /// Conditional-read validators of the three pulled endpoints; the
        /// engine keys them against its own board binding.
        validators: crate::remote::BoardPullValidators,
        /// Outcomes of the ops pushed during this cycle.
        pushes: Vec<PushOutcome>,
        /// The instant the actor read the engine state this cycle planned
        /// from. Outbox ops queued after it were never seen by the cycle's
        /// push, so the snapshot's values for their fields are not evidence
        /// against the local intent — the merge protects those fields
        /// instead of letting the pull's stamps self-clobber newer local
        /// edits (see `apply_sync_report`). The engine passes this through
        /// verbatim; both sides share one process clock.
        read_at: chrono::DateTime<chrono::Utc>,
    },
    /// The cycle failed before producing a snapshot.
    Failed {
        /// Deck-agnostic failure class.
        kind: SyncErrorKind,
        /// Successful push outcomes from the aborted cycle; empty when the
        /// cycle failed before (or during) its first push.
        pushes: Vec<PushOutcome>,
        /// Same contract as `Completed::read_at`: the echo adoption in the
        /// landed push evidence must not clobber edits queued after the
        /// cycle's read either.
        read_at: chrono::DateTime<chrono::Utc>,
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
            SyncCommand::Detach,
        ];
        for _command in commands {}
        let signals = [EngineSignal::StateUpdated; 2];
        assert_eq!(signals[0], signals[1]);
    }

    /// The detach variant travels the wire like its siblings (lossless
    /// serde round trip).
    #[test]
    fn sync_command_detach_round_trips_through_serde() {
        let json = serde_json::to_string(&SyncCommand::Detach).expect("serialize");
        let back: SyncCommand = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, SyncCommand::Detach);
    }
}
