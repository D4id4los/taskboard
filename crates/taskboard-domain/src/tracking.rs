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

impl TimeEntry {
    /// `end.is_none()` — the entry is the running timer.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.end.is_none()
    }

    /// Duration as of `now`: `end.unwrap_or(now) − start`, clamped at
    /// zero (a skew-produced inverted span contributes nothing).
    #[must_use]
    pub fn duration(&self, now: DateTime<Utc>) -> chrono::TimeDelta {
        let end = self.end.unwrap_or(now);
        (end - self.start).max(chrono::TimeDelta::zero())
    }
}

impl TrackingState {
    /// The derived open set — the running timer has no stored home.
    pub fn open_entries(&self) -> impl Iterator<Item = &TimeEntry> {
        self.entries.values().filter(|entry| entry.is_open())
    }

    /// The reader's single-open pick (D-T8): the most-recently-started
    /// open entry, ties broken by the greater entry id.
    #[must_use]
    pub fn newest_open(&self) -> Option<&TimeEntry> {
        self.open_entries()
            .max_by_key(|entry| (entry.start, entry.id))
    }
}

/// Total tracked time inside `window` as of `now`. Each entry's span
/// (`[start, end)`, or `[start, now)` while open) clips to the window;
/// overlaps with the window are summed — the entry is conceptually
/// split at each boundary and only the in-window part counts; rows are
/// never physically split (the days.rs no-derived-bit rule). `now`
/// clamps open accrual at `min(window.end, now)`.
#[must_use]
pub fn tracked_in_window<'a>(
    entries: impl IntoIterator<Item = &'a TimeEntry>,
    window: (DateTime<Utc>, DateTime<Utc>),
    now: DateTime<Utc>,
) -> chrono::TimeDelta {
    let (window_start, window_end) = window;
    entries
        .into_iter()
        .map(|entry| {
            // `unwrap_or(now)` clamps open accrual at the evaluation
            // instant (never into the window's future); `min(window_end)`
            // clips at the boundary; the final clamp zeroes both
            // inverted raw spans and disjoint entries.
            let span_end = entry.end.unwrap_or(now).min(window_end);
            let span_start = entry.start.max(window_start);
            (span_end - span_start).max(chrono::TimeDelta::zero())
        })
        .fold(chrono::TimeDelta::zero(), |acc, span| acc + span)
}

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

    // ---- Pure projections (DR-5/1 M4) ------------------------------------

    use proptest::prelude::*;

    use crate::test_support::{proptest_config, time_entry_strategy, timestamp_strategy};

    #[test]
    fn newest_open_picks_the_most_recently_started_open_entry_with_the_id_tie_break() {
        let mut state = TrackingState::default();
        assert_eq!(state.newest_open(), None, "the empty map has no pick");
        let early = entry(1, 1_000, None);
        let late = entry(2, 5_000, None);
        state.entries.insert(early.id, early);
        state.entries.insert(late.id, late.clone());
        let late_id = late.id;
        // A closed entry never enters the pick, however recent.
        let closed = entry(3, 9_000, Some(9_100));
        state.entries.insert(closed.id, closed);
        assert_eq!(state.newest_open().map(|open| open.id), Some(late_id));
        // Equal starts: the greater entry id wins.
        let tie_high = entry(4, 5_000, None);
        state.entries.insert(tie_high.id, tie_high.clone());
        let tie_high_id = tie_high.id;
        assert_eq!(state.newest_open().map(|open| open.id), Some(tie_high_id));
        assert_eq!(state.open_entries().count(), 3);
    }

    #[test]
    fn duration_clamps_at_zero_and_accrues_while_open() {
        let open = entry(1, 1_000, None);
        assert_eq!(
            open.duration(ts(1_500)),
            chrono::TimeDelta::try_seconds(500).unwrap()
        );
        let closed = entry(2, 1_000, Some(1_200));
        assert_eq!(
            closed.duration(ts(9_999)),
            chrono::TimeDelta::try_seconds(200).unwrap()
        );
        // A skew-produced inverted raw span contributes nothing.
        let mut inverted = entry(3, 1_000, Some(900));
        inverted.end = Some(ts(900));
        assert_eq!(inverted.duration(ts(2_000)), chrono::TimeDelta::zero());
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn tracked_in_window_matches_the_boundary_split_oracle_prop(
            entries in prop::collection::vec(time_entry_strategy(), 0..4),
            window_start in timestamp_strategy(),
            window_secs in 1_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let window_end = window_start
                + chrono::TimeDelta::try_seconds(window_secs).unwrap();
            // The oracle splits each entry at both boundaries and sums
            // the inside segments (the conceptual split; rows are never
            // physically split).
            let oracle = entries.iter().map(|entry| {
                let span_end = entry.end.unwrap_or(now);
                let segments = [
                    (entry.start, span_end.min(window_start)),
                    (entry.start.max(window_start), span_end.min(window_end)),
                    (entry.start.max(window_end), span_end),
                ];
                segments
                    .into_iter()
                    .map(|(a, b)| (b.min(window_end) - a.max(window_start)).max(chrono::TimeDelta::zero()))
                    .fold(chrono::TimeDelta::zero(), |acc, span| acc + span)
            }).fold(chrono::TimeDelta::zero(), |acc, span| acc + span);
            let got = super::tracked_in_window(&entries, (window_start, window_end), now);
            prop_assert_eq!(got, oracle);
            prop_assert!(got >= chrono::TimeDelta::zero(), "never negative");
        }

        #[test]
        fn a_window_fully_inside_a_closed_span_yields_exactly_the_window_length(
            start in timestamp_strategy(),
            window_secs in 1_i64..=3_600,
            span_secs in 3_601_i64..=86_400,
        ) {
            let window_end = start + chrono::TimeDelta::try_seconds(window_secs).unwrap();
            let span_end = start + chrono::TimeDelta::try_seconds(span_secs).unwrap();
            let spanned = entry(1, 0, None);
            let spanned = TimeEntry { start, end: Some(span_end), ..spanned };
            let got = super::tracked_in_window(
                [&spanned],
                (start, window_end),
                span_end,
            );
            prop_assert_eq!(got, chrono::TimeDelta::try_seconds(window_secs).unwrap());
        }
    }

    #[test]
    fn open_accrual_stops_at_now_and_inverted_spans_contribute_zero() {
        // Open entry started before the window: accrual stops at `now`
        // inside the window.
        let open = entry(1, 1_000, None);
        let got = super::tracked_in_window([&open], (ts(2_000), ts(9_000)), ts(3_000));
        assert_eq!(got, chrono::TimeDelta::try_seconds(1_000).unwrap());
        // `now` beyond the window clamps at the window's end.
        let got = super::tracked_in_window([&open], (ts(2_000), ts(9_000)), ts(99_999));
        assert_eq!(got, chrono::TimeDelta::try_seconds(7_000).unwrap());
        // An inverted raw span contributes zero, never a negative total.
        let mut inverted = entry(2, 5_000, None);
        inverted.start = ts(6_000);
        let got = super::tracked_in_window([&inverted], (ts(2_000), ts(9_000)), ts(5_500));
        assert_eq!(got, chrono::TimeDelta::zero());
    }

    #[test]
    fn tracking_rows_serde_roundtrip_keeps_the_whole_entry() {
        let row = entry(1, 1_000, None);
        let json = serde_json::to_string(&row).unwrap();
        let back: TimeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
    }
}
