// SPDX-License-Identifier: MIT OR Apache-2.0
//! The undo journal substrate (ADR 0019): pure data shapes, the
//! deterministic retention eviction, and the derived [`UndoView`]
//! projection.
//!
//! Everything here is pure: no clock, no ids, no IO. An entry records
//! what an accepted mutating command *did* — the verbatim intent, the
//! pre-images captured from the pre-state, the post-write clock stamps
//! (the invertibility watermark), and the outbox ops its batch enqueued.
//! Undo/redo planning lives in [`crate::command`] (it synthesizes inverse
//! commands through [`crate::command::plan_command`]); this module is the
//! line's vocabulary.
//!
//! Determinism devices (the state.rs convergence property): the watermark
//! and the task-membership maps are `BTreeMap`s, entries and op ids are
//! `Vec`s in append order — equal inputs serialize equally, so the journal
//! rides the snapshot corpus byte-stably.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{
    Board, BoardClocks, Label, LabelClocks, MembershipState, Stack, StackClocks, Task, TaskClocks,
};
use crate::habit::{Habit, HabitClocks};
use crate::ids::{BoardId, HabitId, JournalId, LabelId, StackId, TaskId, TimeEntryId};
use crate::messages::StateCommand;
use crate::outbox::OpId;
use crate::tracking::TimeEntry;

/// The retention cap (R-U11): the journal evicts oldest-first **leaf**
/// entries beyond this many.
pub const JOURNAL_CAP: usize = 1000;

/// The retention age floor (R-U11): an entry younger than this never
/// evicts, however far past the cap the line is.
pub const JOURNAL_AGE_FLOOR: TimeDelta = TimeDelta::days(90);

/// The bounded "recent commands" window of the [`UndoView`] projection.
pub const UNDO_RECENT_WINDOW: usize = 10;

/// One journal node: an accepted mutating command and the facts its
/// inverse needs. `parent` is the line head when the command planned —
/// day-one tree shape (D-U4), linear v1 line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The entry's identity (the undo/redo receipt names it).
    pub id: JournalId,
    /// The head this command planned from; `None` = the line's first
    /// entry.
    pub parent: Option<JournalId>,
    /// The intent, verbatim (the undo label; redo re-plans it).
    pub command: StateCommand,
    /// The pre-images and watermark — or the typed reason the command is
    /// not undoable.
    pub facts: UndoFacts,
    /// The outbox ops the command's batch enqueued, in batch order. The
    /// pushed-edit rule reads this list (`FailOp` retraction before the
    /// push boundary).
    pub ops: Vec<OpId>,
    /// `false` until a completed push completed one of `ops`
    /// ([`PersistenceAction::MarkJournalPushed`] flips it — never
    /// planning).
    pub pushed: bool,
    /// Best-effort touched-board tag (D-U4's per-board note deferred to
    /// v2; `None` when not board-scoped, e.g. `StopTracking`).
    pub board: Option<BoardId>,
    /// The planner's stamp (the redo tie-break's first key; the eviction
    /// floor's reference is the *incoming* entry's stamp).
    pub created_at: DateTime<Utc>,
}

/// What undo needs to know about the command's effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UndoFacts {
    /// The command is invertible in v1: the entities it touched with
    /// their pre-images, and the post-write clock stamps it left (the
    /// watermark, full clock structs per D8 — the per-arm *checked*
    /// subset is the check site's decision, decision D7).
    PreImages {
        /// One per entity the batch upserted; `before: None` = the
        /// command created it.
        images: Vec<PreImage>,
        /// Per touched entity, the post-write clock set.
        watermark: UndoWatermark,
    },
    /// The command's class is not undoable in v1 — refuse and name it
    /// (R-U6).
    NotUndoable(NotUndoableReason),
}

/// Why a journaled command's head refuses undo (the #353–#357 families
/// plus the two post-design surfaces).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotUndoableReason {
    /// Restore-class verbs need restore machinery v1 does not ship
    /// (#356): `DeleteStack`, `DeleteLabel`, `DeleteTimeEntry`,
    /// `DeleteHabit`.
    RestoreClass,
    /// Board ceremonies are their own defense (#357): the create/bind/
    /// rename/color/archive/delete/reset/unbind/reconcile family.
    BoardCeremony,
    /// Reminder-surface ephemera are cheap to redo by hand (#354):
    /// snoozes, mutes, dismissals, rule edits, adhoc alarms.
    ReminderSurface,
    /// Tracking verbs are post-design surface with a scope-closed v1
    /// (decision D17); `StartTracking`/`StopTracking` are additionally
    /// structurally non-invertible (no command re-opens a closed entry).
    TrackingSurface,
    /// Habit verbs are post-design surface with a scope-closed v1
    /// (decision D17); `LogHabit` is additionally lossy to invert.
    HabitSurface,
}

/// The pre-image of one entity the batch upserted. `before: None` = the
/// entity did not exist before the command (a create — its inverse is a
/// delete/retraction, never a value restore).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreImage {
    Board {
        id: BoardId,
        before: Option<Box<Board>>,
    },
    Stack {
        id: StackId,
        before: Option<Box<Stack>>,
    },
    Task {
        id: TaskId,
        before: Option<Box<Task>>,
    },
    Label {
        id: LabelId,
        before: Option<Box<Label>>,
    },
    Habit {
        id: HabitId,
        before: Option<Box<Habit>>,
    },
    TimeEntry {
        id: TimeEntryId,
        before: Option<Box<TimeEntry>>,
    },
}

/// The invertibility watermark (R-U7): per touched entity, the post-write
/// clock set the command's own upserts carried. A `BTreeMap` internally —
/// deterministic order — serialized as a sorted pair sequence (an
/// enum-keyed JSON map is not representable; the seq is).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UndoWatermark(pub BTreeMap<JournalEntity, EntityStamps>);

impl Serialize for UndoWatermark {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.0.iter())
    }
}

impl<'de> Deserialize<'de> for UndoWatermark {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let pairs: Vec<(JournalEntity, EntityStamps)> = Vec::deserialize(deserializer)?;
        Ok(Self(pairs.into_iter().collect()))
    }
}

/// A watermark key: one entity, by kind. `Ord` = declaration order, then
/// the id — deterministic for equal inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum JournalEntity {
    Board(BoardId),
    Stack(StackId),
    Task(TaskId),
    Label(LabelId),
    Habit(HabitId),
    TimeEntry(TimeEntryId),
}

/// The post-write clock set of one touched entity (full structs, D8;
/// the membership map forfeits `Copy` — clone-at-record is the cost).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityStamps {
    Task {
        clocks: TaskClocks,
        /// The post-write membership map (the OR-set stamps the
        /// assign/unassign watermark reads).
        memberships: BTreeMap<LabelId, MembershipState>,
    },
    Stack(StackClocks),
    Board(BoardClocks),
    Label(LabelClocks),
    Habit(HabitClocks),
    TimeEntry(DateTime<Utc>),
}

/// One divergent clocked field, named by a refusal (the toast payload;
/// error-payload-only — the check site knows the entity).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UndoField {
    Title,
    Description,
    Duedate,
    Scheduled,
    Done,
    Position,
    Effort,
    Importance,
    Urgency,
    Recur,
    Archived,
    Deleted,
    Order,
    Color,
    Membership,
    UpdatedAt,
}

/// The persisted undo line (R-U10): the entries in append order
/// (= chronological; storage mirrors it with `ORDER BY rowid`) and the
/// explicit head. The head moves on undo/redo without a new node, so it
/// cannot be derived — it is persisted state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct JournalState {
    /// Append order = chronological.
    pub entries: Vec<JournalEntry>,
    /// The line head; `None` = the empty line.
    pub head: Option<JournalId>,
}

impl JournalState {
    /// The entry with this id, if the line still carries it (eviction may
    /// have pruned it).
    #[must_use]
    pub fn entry(&self, id: JournalId) -> Option<&JournalEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// The ids of the entries whose `parent` is `id` (the redo
    /// candidates, D5).
    #[must_use]
    pub fn children_of(&self, id: JournalId) -> Vec<&JournalEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.parent == Some(id))
            .collect()
    }
}

/// The UI-facing undo projection (memory-only, like `last_updated`):
/// derived from the [`JournalState`] at every publish — never persisted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UndoView {
    /// A head exists. A `NotUndoable` head still shows `true` — the undo
    /// attempt refuses and names the reason (R-U6).
    pub can_undo: bool,
    /// The head has a child (a redo target exists).
    pub can_redo: bool,
    /// Derived display text of the head's command (never test-asserted —
    /// the label is presentation, the typed facts are the contract).
    pub head_label: Option<String>,
    /// The bounded recent-command window, newest first.
    pub recent: Vec<UndoLabel>,
}

/// One recent-command row of the [`UndoView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UndoLabel {
    pub id: JournalId,
    pub label: String,
}

impl UndoView {
    /// The single derivation used by both `EngineCore::app()` and
    /// `From<PersistedState> for AppState`.
    #[must_use]
    pub fn derive(journal: &JournalState) -> Self {
        let head = journal.head.and_then(|id| journal.entry(id));
        let can_redo = journal
            .head
            .is_some_and(|head| !journal.children_of(head).is_empty());
        UndoView {
            can_undo: head.is_some(),
            can_redo,
            head_label: head.map(|entry| command_label(&entry.command)),
            recent: journal
                .entries
                .iter()
                .rev()
                .take(UNDO_RECENT_WINDOW)
                .map(|entry| UndoLabel {
                    id: entry.id,
                    label: command_label(&entry.command),
                })
                .collect(),
        }
    }
}

/// The derived display text of a journaled intent. Presentation-only
/// (the `head_label` contract); a plain verb phrase per arm.
#[must_use]
pub fn command_label(command: &StateCommand) -> String {
    match command {
        StateCommand::CreateTask { .. } => "Create task".into(),
        StateCommand::CaptureTask { .. } => "Capture task".into(),
        StateCommand::UpdateTask { .. } => "Edit task".into(),
        StateCommand::SetTaskDone { .. } => "Mark done".into(),
        StateCommand::MoveTask { .. } => "Move task".into(),
        StateCommand::DeleteTask { .. } => "Delete task".into(),
        StateCommand::CreateStack { .. } => "Create stack".into(),
        StateCommand::RenameStack { .. } => "Rename stack".into(),
        StateCommand::DeleteStack { .. } => "Delete stack".into(),
        StateCommand::MoveStack { .. } => "Move stack".into(),
        StateCommand::CreateLabel { .. } => "Create label".into(),
        StateCommand::UpdateLabel { .. } => "Edit label".into(),
        StateCommand::DeleteLabel { .. } => "Delete label".into(),
        StateCommand::AssignLabel { .. } => "Assign label".into(),
        StateCommand::UnassignLabel { .. } => "Unassign label".into(),
        StateCommand::CreateBoard { .. } => "Create board".into(),
        StateCommand::BindBoard { .. } => "Bind board".into(),
        StateCommand::RenameBoard { .. } => "Rename board".into(),
        StateCommand::SetBoardColor { .. } => "Set board color".into(),
        StateCommand::SetBoardArchived { .. } => "Archive board".into(),
        StateCommand::DeleteBoard { .. } => "Delete board".into(),
        StateCommand::ResetBoard { .. } => "Reset board".into(),
        StateCommand::SnoozeReminder { .. } => "Snooze reminder".into(),
        StateCommand::SetTaskRemindersMuted { .. } => "Mute reminders".into(),
        StateCommand::DismissReminder { .. } => "Dismiss reminder".into(),
        StateCommand::AddReminderRule { .. } => "Add reminder rule".into(),
        StateCommand::RemoveReminderRule { .. } => "Remove reminder rule".into(),
        StateCommand::ClearReminderRules { .. } => "Clear reminder rules".into(),
        StateCommand::UnbindBoard { .. } => "Unbind board".into(),
        StateCommand::ReconcileByTitle { .. } => "Reconcile duplicates".into(),
        StateCommand::CreateAdhocAlarm { .. } => "Create alarm".into(),
        StateCommand::CancelAdhocAlarm { .. } => "Cancel alarm".into(),
        StateCommand::StartTracking { .. } => "Start tracking".into(),
        StateCommand::StopTracking => "Stop tracking".into(),
        StateCommand::LogTime { .. } => "Log time".into(),
        StateCommand::AdjustTimeEntry { .. } => "Adjust time entry".into(),
        StateCommand::DeleteTimeEntry { .. } => "Delete time entry".into(),
        StateCommand::CreateHabit { .. } => "Create habit".into(),
        StateCommand::UpdateHabit { .. } => "Edit habit".into(),
        StateCommand::MoveHabit { .. } => "Move habit".into(),
        StateCommand::LogHabit { .. } => "Log habit".into(),
        StateCommand::AdjustHabitLog { .. } => "Adjust habit log".into(),
        StateCommand::DeleteHabit { .. } => "Delete habit".into(),
        StateCommand::RestoreTask { .. } => "Restore task".into(),
        StateCommand::Undo | StateCommand::Redo => "Undo/redo".into(),
        StateCommand::RequestSync => "Sync".into(),
    }
}

/// The retention eviction (R-U11): the oldest-first **leaf** entries (no
/// descendants via `parent`) to drop when the line exceeds
/// [`JOURNAL_CAP`], judged against the *incoming* entry's stamp —
/// deterministic, no `now` injection. Entries younger than
/// [`JOURNAL_AGE_FLOOR`] never evict (the cap may temporarily stand
/// exceeded — the floor wins). Both the shared `apply_actions` and the
/// sqlite mirror call this one function, so memory ≡ disk holds by
/// construction (D11).
#[must_use]
pub fn evict_journal(
    entries: &[JournalEntry],
    incoming_created_at: DateTime<Utc>,
) -> Vec<JournalId> {
    let line: Vec<(JournalId, Option<JournalId>, DateTime<Utc>)> = entries
        .iter()
        .map(|entry| (entry.id, entry.parent, entry.created_at))
        .collect();
    evict_journal_ids(&line, incoming_created_at)
}

/// The single eviction decision over the line's lean shape — the shape
/// the sqlite mirror reads straight off its table columns (D11: one pure
/// function, two callers, memory ≡ disk by construction).
#[must_use]
pub fn evict_journal_ids(
    line: &[(JournalId, Option<JournalId>, DateTime<Utc>)],
    incoming_created_at: DateTime<Utc>,
) -> Vec<JournalId> {
    if line.len() <= JOURNAL_CAP {
        return Vec::new();
    }
    let parents: BTreeSet<JournalId> = line.iter().filter_map(|(_, parent, _)| *parent).collect();
    let mut leaves: Vec<&(JournalId, Option<JournalId>, DateTime<Utc>)> = line
        .iter()
        .filter(|(id, _, _)| !parents.contains(id))
        .collect();
    // Oldest first: the (created_at, id) order, the redo tie-break's own.
    leaves.sort_by_key(|leaf| (leaf.2, leaf.0));
    let overflow = line.len() - JOURNAL_CAP;
    let mut evicted = Vec::new();
    for leaf in leaves {
        if evicted.len() >= overflow {
            break;
        }
        // Sorted oldest-first: the first too-young leaf ends the sweep.
        if incoming_created_at - leaf.2 < JOURNAL_AGE_FLOOR {
            break;
        }
        evicted.push(leaf.0);
    }
    evicted
}

#[cfg(test)]
mod tests;
