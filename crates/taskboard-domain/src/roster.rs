// SPDX-License-Identifier: MIT OR Apache-2.0
//! The restore-roster projection (the undo-family journey plan W-1/W-8,
//! decision D-1): the typed row source the GUI restore affordances
//! read — which tombstoned labels/habits and which hard-deleted time
//! entries are restorable, with engine-derived display facts.
//!
//! Everything here is pure and derived at every publish (the
//! [`crate::journal::UndoView::derive`] precedent): memory-only, never
//! persisted, bounded (the entity maps' tombstone population and the
//! journal's retention ring are the honest bounds). The UI renders the
//! rows; it never synthesizes ids or names.

use crate::entities::Label;
use crate::ids::{HabitId, JournalId, LabelId, TimeEntryId};
use crate::journal::{JournalState, PreImage, UndoFacts};
use crate::messages::StateCommand;
use crate::state::AppState;

/// One restorable tombstoned entity (label or habit): its typed id and
/// its engine-derived name. A plain projection record, **not** a
/// trust-boundary mint — no fallible constructor (the A1 policy's
/// documented leniency: the derive reads engine-owned rows verbatim,
/// there is no user input to validate).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct RestoreRow<Id> {
    /// The tombstoned entity's id (the restore command's target).
    pub id: Id,
    /// The entity's current title (pre-tombstone values survive the
    /// tombstone, so this is the name the restore brings back).
    pub name: String,
}

/// One restorable hard-deleted time entry (the zero-input restore's
/// row source, D-5): the entry's id, the journal entry whose pre-image
/// carries the verbatim payload, and the engine-derived label the
/// surfaces render. Bounded by the journal's retention ring — a
/// pre-image beyond the ring is honestly absent (the journey plan's
/// N5); the flag grammar stays the out-of-band recovery path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct DeletedEntryRow {
    /// The deleted entry's id (the row's identity in the roster).
    pub entry_id: TimeEntryId,
    /// The journaled `DeleteTimeEntry` whose pre-image holds the
    /// payload — the zero-input `RestoreTimeEntryFromJournal { from }`
    /// target.
    pub journal_id: JournalId,
    /// The engine-derived display label: the entry's snapshotted task
    /// title plus its span (the UI renders, never synthesizes).
    pub label: String,
}

/// The publish-derived restore roster (D-1): tombstoned labels,
/// tombstoned habits, and the journal's still-restorable deleted
/// entries. Derived at every publish beside the `UndoView`; never
/// persisted; empty by default.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct RestoreRoster {
    /// Tombstoned labels, id order (the `BTreeMap`'s own —
    /// deterministic for equal inputs).
    pub tombstoned_labels: Vec<RestoreRow<LabelId>>,
    /// Tombstoned habits, id order.
    pub tombstoned_habits: Vec<RestoreRow<HabitId>>,
    /// The journal's restorable deleted entries, newest delete first
    /// (the restore surface reads like a "recently deleted" list);
    /// one row per entry id — the *newest* delete's pre-image wins.
    pub deleted_entries: Vec<DeletedEntryRow>,
}

/// The deleted-entry display label: the snapshotted task title plus
/// the span (the plan's engine-derived label; presentation text, so
/// never a test oracle — the typed ids are).
fn deleted_entry_label(row: &crate::tracking::TimeEntry) -> String {
    let end = row.end.map_or_else(
        || "running".to_owned(),
        |end| end.format("%Y-%m-%d %H:%M").to_string(),
    );
    format!(
        "{} · {} – {}",
        row.task_title,
        row.start.format("%Y-%m-%d %H:%M"),
        end
    )
}

impl RestoreRoster {
    /// The single derivation used by every publish site (the engine
    /// core and the hydration fold): the typed inputs are the entity
    /// maps' tombstone populations and the journal's bounded ring.
    #[must_use]
    pub fn derive_parts(
        labels: &std::collections::BTreeMap<LabelId, Label>,
        habits: &crate::habit::HabitState,
        tracking: &crate::tracking::TrackingState,
        journal: &JournalState,
    ) -> Self {
        let tombstoned_labels = labels
            .values()
            .filter(|label| label.deleted)
            .map(|label| RestoreRow {
                id: label.id,
                name: label.title.clone(),
            })
            .collect();
        let tombstoned_habits = habits
            .habits
            .values()
            .filter(|habit| habit.deleted)
            .map(|habit| RestoreRow {
                id: habit.id,
                name: habit.title.clone(),
            })
            .collect();
        // The deleted-entry section (W-8): scan the bounded journal for
        // `DeleteTimeEntry` entries whose captured pre-image row is
        // currently absent. Append-order iteration + map insert means
        // the *newest* delete of an entry id wins (a delete → restore →
        // delete pair leaves one honest row); the Vec is then reversed
        // into newest-first display order.
        let mut newest_delete: std::collections::BTreeMap<TimeEntryId, (JournalId, String)> =
            std::collections::BTreeMap::new();
        for entry in &journal.entries {
            let StateCommand::DeleteTimeEntry { .. } = entry.command else {
                continue;
            };
            let UndoFacts::PreImages { images, .. } = &entry.facts else {
                continue;
            };
            for image in images {
                if let PreImage::TimeEntry { id, before } = image {
                    let Some(row) = before else {
                        continue;
                    };
                    if tracking.entries.contains_key(id) {
                        // The row is present (restored or re-created) —
                        // nothing to restore, whatever the journal says.
                        continue;
                    }
                    newest_delete.insert(*id, (entry.id, deleted_entry_label(row)));
                }
            }
        }
        let mut deleted_entries: Vec<DeletedEntryRow> = newest_delete
            .into_iter()
            .map(|(entry_id, (journal_id, label))| DeletedEntryRow {
                entry_id,
                journal_id,
                label,
            })
            .collect();
        deleted_entries.reverse();
        Self {
            tombstoned_labels,
            tombstoned_habits,
            deleted_entries,
        }
    }

    /// The `AppState`-shaped convenience: the engine's publish path.
    #[must_use]
    pub fn derive(app: &AppState, journal: &JournalState) -> Self {
        Self::derive_parts(&app.labels, &app.habits, &app.tracking, journal)
    }
}

#[cfg(test)]
mod tests;
