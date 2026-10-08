// SPDX-License-Identifier: MIT OR Apache-2.0
//! Canonical entities: [`Board`], [`Stack`], [`Task`], [`Label`].
//!
//! Conflict model: each editable field of an entity carries a
//! `DateTime<Utc>` write clock (per-field last-writer-wins). Local edits
//! stamp the touched fields only; adopting a remote version stamps every
//! field with the remote entity-level `lastModified`. Position
//! (`stack` + `order`) shares one clock — a move is a single user intent.
//! Tombstones are `deleted: bool` plus the `deleted` field clock.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardRef, RemoteLabelRef, RemoteStackRef, StackId, TaskId,
};

/// Lenient color value: construction passes anything through and `Display`
/// round-trips. Validation is the sync adapter's job (`DeckColor`); the
/// domain only needs the value to survive storage and echo adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(String);

impl Color {
    /// Wraps any wire string (lenient by design).
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// The raw string form.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Color {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Per-field write timestamps of a [`Task`] (decision: per-field LWW).
///
/// Local edits touch only the edited fields; remote adoption stamps every
/// clock with the remote `lastModified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `description`.
    pub description: DateTime<Utc>,
    /// Write time of `duedate`.
    pub duedate: DateTime<Utc>,
    /// Write time of `done`.
    pub done: DateTime<Utc>,
    /// Write time of the composite position (`stack` + `order`).
    pub position: DateTime<Utc>,
    /// Write time of the whole label set.
    pub labels: DateTime<Utc>,
    /// Write time of `archived`.
    pub archived: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// Per-field write timestamps of a [`Stack`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `order`.
    pub order: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// Per-field write timestamps of a [`Label`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelClocks {
    /// Write time of `title`.
    pub title: DateTime<Utc>,
    /// Write time of `color`.
    pub color: DateTime<Utc>,
    /// Write time of the tombstone (`deleted`).
    pub deleted: DateTime<Utc>,
}

/// A board. No per-field clocks in the MVP: no local board-edit commands
/// exist, so board content only arrives via remote adoption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    /// Local identity.
    pub id: BoardId,
    /// Remote binding; `None` until bound to a Deck board.
    pub remote: Option<RemoteBoardId>,
    /// Board title.
    pub title: String,
    /// Board color (lenient).
    pub color: Color,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag (boards have real remote delete timestamps).
    pub deleted: bool,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// A column within a board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    /// Local identity.
    pub id: StackId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteStackRef>,
    /// Owning board (local id).
    pub board: BoardId,
    /// Stack title.
    pub title: String,
    /// Sort position within the board.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: StackClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// A task card — the central entity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// Local identity.
    pub id: TaskId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteCardRef>,
    /// Task title.
    pub title: String,
    /// Task description (Markdown, as Deck stores it).
    pub description: String,
    /// Due date; `None` = none.
    pub duedate: Option<DateTime<Utc>>,
    /// Completion timestamp; `None` = open.
    pub done: Option<DateTime<Utc>>,
    /// Owning stack — half of the composite position field.
    pub stack: StackId,
    /// Sort order within the stack — half of the composite position field.
    pub order: i64,
    /// Attached labels (whole-set LWW as one mergeable field).
    pub labels: std::collections::BTreeSet<LabelId>,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag; `clocks.deleted` is the tombstone's write time.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: TaskClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    /// Fast-path guard for the pull rules (unchanged remote ⇒ keep local).
    pub remote_seen: Option<DateTime<Utc>>,
}

impl Task {
    /// `true` while the task is not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
    }

    /// `true` while the task is not completed.
    #[must_use]
    pub const fn is_open(&self) -> bool {
        self.done.is_none()
    }
}

/// A label defined on a board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    /// Local identity.
    pub id: LabelId,
    /// Remote binding; `None` until the pending create is pushed.
    pub remote: Option<RemoteLabelRef>,
    /// Owning board (local id).
    pub board: BoardId,
    /// Label title.
    pub title: String,
    /// Label color (lenient).
    pub color: Color,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: LabelClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    pub remote_seen: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        #[test]
        fn entities_roundtrip_through_serde(task in test_support::task_strategy()) {
            let json = serde_json::to_string(&task).unwrap();
            let back: Task = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, task);
        }

        #[test]
        fn color_roundtrips(raw in ".{0,64}") {
            let color = Color::new(raw.clone());
            prop_assert_eq!(color.as_str(), raw.as_str());
            prop_assert_eq!(color.to_string(), raw);
        }

        #[test]
        fn task_helpers_match_flags(task in test_support::task_strategy()) {
            prop_assert_eq!(task.is_live(), !task.deleted);
            prop_assert_eq!(task.is_open(), task.done.is_none());
        }
    }
}
