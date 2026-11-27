// SPDX-License-Identifier: MIT OR Apache-2.0
//! The habit event log (#1519): one event per completion or skip
//! intent — the future sole source of truth for habit statistics.
//!
//! This module holds the *model* only. The read-time derivations
//! ([`crate::habit`]'s `log_window` remains the attribution oracle
//! until the legacy maps die in the dispatch's final chunk) re-key
//! onto the fold functions added alongside these types. Nothing here
//! mints, arbitrates, or persists: minting goes through the
//! [`crate::idgen::IdGenerator`] seam; the sync fold and the sqlite
//! rows are later chunks' transport work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use crate::habit::WindowStart;
use crate::ids::{AuthorId, HabitId};

/// Identity of one completion or one skip-intent mark. Minted at log
/// time via the id seam — never derived from the completion instant,
/// so same-second completions stay distinct individuals.
///
/// # Minting vs. leniency
///
/// Production mints only through
/// [`crate::idgen::IdGenerator::new_habit_event_id`]; [`From<Uuid>`]
/// is the *trust-boundary* constructor (pub(crate)): the wire/storage
/// decode and the migration's deterministic synthetic ids (a later
/// chunk) mint legitimate ids that never passed through the
/// generator. Deliberate leniency at the type: `From<Uuid>` accepts
/// any uuid version — synthetic non-v7 ids are valid members, so a
/// version check here would be validate-and-forget against a future
/// fact family. Text-input paths go through [`HabitEventId::parse`],
/// which is strict (`AuthorId::parse`'s shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HabitEventId(Uuid);

impl HabitEventId {
    /// Strict UUID parsing — the decode path for text forms. `None`
    /// on any non-UUID input.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Uuid::parse_str(raw).ok().map(Self)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// The trusted-internal constructor from an already-typed UUID (the
/// id seam's mint, the wire/storage decode, deterministic migration
/// synthesis). Never a text-input path — text goes through
/// [`HabitEventId::parse`].
impl From<Uuid> for HabitEventId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for HabitEventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Uuid`'s `Display` is already the lowercase hyphenated form.
        self.0.fmt(f)
    }
}

impl Serialize for HabitEventId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HabitEventId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| serde::de::Error::custom("an event id must be a UUID"))
    }
}

/// What kind of fact the event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HabitEventKind {
    /// One completed quota unit. Carries no window: the window is
    /// derived at read time (the whole point of the event-log design —
    /// a config change re-derives history instead of stranding rows).
    Completion,
    /// A deliberate planned-skip mark for one derived window.
    ///
    /// Deliberate leniency at the field: the stored `window` is the
    /// *mint-time* derivation (sync provenance, the fold's wire
    /// shape), so it may name a window of a former target kind after
    /// a config change. The read-time derivation never reads it —
    /// marks re-attribute under the *current* config like completions
    /// — so the stale value is honest provenance, not corruption, and
    /// decode keeps it as-is (the [`WindowStart`] decode posture).
    Skip {
        /// The window the mark was minted for (provenance only).
        window: WindowStart,
    },
    /// Withdrawal of the window's skip mark.
    ///
    /// Leniency: see [`HabitEventKind::Skip`] — same provenance-only
    /// stored window, never read by the derivation.
    Unskip {
        /// The window the mark was minted for (provenance only).
        window: WindowStart,
    },
}

/// The per-event deletion tombstone (Ruling C2): a correction is an
/// appended fact, never a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEventTombstone {
    /// When the deletion happened.
    pub at: DateTime<Utc>,
    /// Who deleted it. Deliberate leniency: `None` means "author
    /// unknown" (rows that predate the author slot) and folds under
    /// the `None < Some` rank — never "authored by nobody" (the
    /// `HabitLogEntry::by` posture verbatim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
}

/// One habit fact: a completion or a skip-intent mark, plus its
/// deletion tombstone. Small and pure — the whole struct is `Copy`,
/// so the fold passes events by value freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEvent {
    /// The event's identity (roaming family — the synced section's
    /// wire key).
    pub id: HabitEventId,
    /// The counted habit.
    pub habit: HabitId,
    /// What happened.
    pub kind: HabitEventKind,
    /// The completion / mark instant — the event's own fact. No clock
    /// inflation: the LWW `mint_log_clock` bump (#986) is row-map
    /// machinery; the derivation fold arbitrates nothing and never
    /// reads a clock.
    ///
    /// Deliberate leniency: a tombstone `at` later than `at` itself
    /// is legitimate — corrections happen after the fact.
    pub at: DateTime<Utc>,
    /// Display-only author (the F1 `by` leg survives for display,
    /// never for arbitration). Deliberate leniency: absent means
    /// "author unknown", the `None < Some` posture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
    /// Deletion tombstone (Ruling C2): default-absent; the newest
    /// `(at, by)` tombstone wins in the sync fold (a later chunk).
    /// Absent on every event the local write path mints or loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<HabitEventTombstone>,
}

impl HabitEvent {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.deleted.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn stamp(raw: u128) -> Uuid {
        Uuid::from_u128(raw)
    }

    fn event(raw: u128) -> HabitEvent {
        HabitEvent {
            id: HabitEventId::from(stamp(raw)),
            habit: HabitId::from(stamp(42)),
            kind: HabitEventKind::Completion,
            at: chrono::Utc
                .timestamp_opt(1_700_000_000, 0)
                .single()
                .expect("valid instant"),
            by: None,
            deleted: None,
        }
    }

    #[test]
    fn event_round_trips_with_default_absent_optional_fields() {
        let original = event(1);
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(decoded.by.is_none());
        assert!(decoded.deleted.is_none());
    }

    #[test]
    fn event_json_omits_absent_optional_fields() {
        let json = serde_json::to_value(event(2)).expect("serializes");
        // Structural fact, not text: the absent Option legs have no key.
        assert!(json.get("by").is_none());
        assert!(json.get("deleted").is_none());
    }

    #[test]
    fn tombstoned_event_round_trips() {
        let mut original = event(3);
        original.deleted = Some(HabitEventTombstone {
            at: chrono::Utc
                .timestamp_opt(1_700_000_500, 0)
                .single()
                .expect("valid instant"),
            by: Some(AuthorId::from_uuid(stamp(7))),
        });
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(!decoded.is_live());
    }

    #[test]
    fn tombstone_decodes_with_absent_author() {
        let tombstoned = serde_json::json!({
            "id": stamp(11).to_string(),
            "habit": stamp(42).to_string(),
            "kind": { "kind": "completion" },
            "at": "2023-11-14T22:13:20Z",
            "deleted": { "at": "2023-11-14T23:13:20Z" },
        });
        let decoded: HabitEvent =
            serde_json::from_value(tombstoned).expect("decodes with default legs");
        let tombstone = decoded.deleted.expect("tombstone present");
        assert!(tombstone.by.is_none());
    }

    #[test]
    fn skip_mark_round_trips_with_stored_window() {
        let window = WindowStart::of_period(
            crate::habit::PeriodKind::Week,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 23).expect("valid date"),
        );
        let mut original = event(4);
        original.kind = HabitEventKind::Skip { window };
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(matches!(decoded.kind, HabitEventKind::Skip { .. }));
    }

    #[test]
    fn unparsable_event_id_decode_fails_and_parse_returns_none() {
        let json = serde_json::json!({
            "id": "not-a-uuid",
            "habit": stamp(42).to_string(),
            "kind": { "kind": "completion" },
            "at": "2023-11-14T22:13:20Z",
        });
        let decoded: Result<HabitEvent, _> = serde_json::from_value(json);
        assert!(decoded.is_err());
        assert!(HabitEventId::parse("not-a-uuid").is_none());
    }

    #[test]
    fn event_id_serde_is_the_uuid_string_form() {
        let id = HabitEventId::from(stamp(5));
        let json = serde_json::to_string(&id).expect("serializes");
        let round: HabitEventId = serde_json::from_str(&json).expect("decodes");
        assert_eq!(round, id);
    }
}
