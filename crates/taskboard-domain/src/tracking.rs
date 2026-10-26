// SPDX-License-Identifier: MIT OR Apache-2.0
//! Time tracking: the [`TimeEntry`] entity, its [`TrackingState`] home,
//! and the planner-side span validation seam (ADR 0017).
//!
//! **The open entry (`end: None`) IS the running timer** — persisted
//! engine state, not runtime intent. Nothing ticks: elapsed is always
//! derived at read ([`TimeEntry::duration`]), and an end is stamped only
//! by a command at the acting planner's `now`.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{TaskId, TimeEntryId};

/// One settled block of tracked work. The open entry (`end: None`) is
/// the running timer (ADR 0017); nothing ticks — elapsed is always
/// derived at read, and an end is stamped only by a command at planner
/// `now`.
///
/// # Minting vs. leniency
///
/// Planner-written spans are predicate-clean: every *user-authored*
/// closed span enters through [`ClosedSpan::try_new`] (the planner's
/// trust boundary), and close stamps are exempt from the strict
/// predicate by ruling (a skewed remote stop stamps `end = now`
/// unconditionally; `StopTracking` must stay total). Decoded rows
/// (persisted state, and later the synced sidecar fold) are
/// **trusted-written** — they were validated at their originating
/// device's planner — so they decode as-is; the read-side projections
/// clamp degenerate shapes to zero so a skew-produced inverted span can
/// never escape to a surface as a negative duration. This is deliberate
/// leniency at the type, not validate-and-forget: the planner-side
/// validate-and-build path flows the minted values into the row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeEntry {
    /// The entry's local identity (roaming family — the synced section's
    /// wire key).
    pub id: TimeEntryId,
    /// The tracked task. The reference survives the task's soft delete:
    /// history outlives tasks (ADR 0017).
    pub task: TaskId,
    /// The task's title, snapshotted immutably at start. Renames and
    /// deletions never rewrite history.
    pub task_title: String,
    /// Span start.
    pub start: DateTime<Utc>,
    /// Span end; `None` = open = the running timer.
    pub end: Option<DateTime<Utc>>,
    /// Row creation stamp (always the minting planner's `now`, never
    /// span data — retroactive rows stay newest-write under LWW).
    pub created_at: DateTime<Utc>,
    /// The LWW mutation clock (always the mutating planner's `now`).
    pub updated_at: DateTime<Utc>,
}

/// The tracking sub-state, nested in [`crate::state::AppState`] like
/// `ReminderState`. `BTreeMap` keeps serialization deterministic (the
/// state.rs determinism contract).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingState {
    /// All time entries, keyed by id (settled history and open entries
    /// alike — the open set is derived, never stored separately).
    pub entries: BTreeMap<TimeEntryId, TimeEntry>,
}

/// A closed span the planner has validated (`end > start`). Private
/// fields; the fallible mint is the planner-side trust boundary —
/// user-authored spans cannot enter a row without passing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClosedSpan {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

impl ClosedSpan {
    /// The mint: accepts the span only when `end > start`.
    ///
    /// # Errors
    ///
    /// [`SpanRejected`] when `end <= start` (empty and inverted spans
    /// are equally meaningless as authored history).
    pub fn try_new(start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Self, SpanRejected> {
        if end > start {
            Ok(Self { start, end })
        } else {
            Err(SpanRejected)
        }
    }

    /// The validated start.
    #[must_use]
    pub fn start(self) -> DateTime<Utc> {
        self.start
    }

    /// The validated end (strictly after [`Self::start`]).
    #[must_use]
    pub fn end(self) -> DateTime<Utc> {
        self.end
    }
}

/// The span predicate failed (`end <= start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("time span invalid")]
pub struct SpanRejected;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(raw: u128, start: i64, end: Option<i64>) -> TimeEntry {
        TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task: TaskId::from(uuid::Uuid::from_u128(900)),
            task_title: "task 900".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
        }
    }

    #[test]
    fn tracking_state_serde_roundtrip_including_an_open_row() {
        let mut state = TrackingState::default();
        state.entries.insert(
            entry(1, 1_000, None).id,
            TimeEntry {
                task_title: "open work".into(),
                ..entry(1, 1_000, None)
            },
        );
        state
            .entries
            .insert(entry(2, 500, Some(900)).id, entry(2, 500, Some(900)));
        let json = serde_json::to_string(&state).unwrap();
        let back: TrackingState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
        assert!(
            json.contains("\"end\":null"),
            "the open row serializes its null end"
        );
    }

    #[test]
    fn closed_span_mint_admits_only_strictly_positive_spans() {
        assert!(ClosedSpan::try_new(ts(100), ts(200)).is_ok());
        // Empty and inverted are the same typed rejection.
        assert_eq!(ClosedSpan::try_new(ts(100), ts(100)), Err(SpanRejected));
        assert_eq!(ClosedSpan::try_new(ts(200), ts(100)), Err(SpanRejected));
        let span = ClosedSpan::try_new(ts(100), ts(200)).unwrap();
        assert_eq!(span.start(), ts(100));
        assert_eq!(span.end(), ts(200));
    }
}
