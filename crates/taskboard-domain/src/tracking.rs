// SPDX-License-Identifier: MIT OR Apache-2.0
//! Time tracking: the [`TimeEntry`] entity, its [`TrackingState`] home,
//! the planner-side span validation seam (ADR 0017), and the pure
//! work-streak projections the break-reminder trigger consumes.
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
    /// What the entry counts as. Default-off: old serialized rows
    /// (journal payloads, receipt JSON) decode [`TimeEntryKind::Work`].
    #[serde(default)]
    pub kind: TimeEntryKind,
}

/// What a time entry counts as. `Work` is the default and the
/// overwhelming case; `Respite` marks break activity — consumed by the
/// break-reminder trigger's reset condition and by review statistics.
/// Deliberately an enum, not a bool: the DR-5 §7 vocabulary reservation
/// names *kind*s (plural, forward-extensible) and a second bool would
/// fork the fold/wire/export plumbing per flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TimeEntryKind {
    /// Tracked work — the default and the overwhelming case.
    #[default]
    Work,
    /// Break activity: counts *against* the work streak, never toward
    /// it. The marker is data, not behavior — nothing auto-fires from
    /// it; the break-reminder trigger is the consumer.
    Respite,
}

impl TimeEntryKind {
    /// Parses the serde unit name (`"work"` / `"respite"`) — the same
    /// text the storage column and the wire row store. The clap
    /// value-parser seam delegates here; an unknown kind is a boundary
    /// parse rejection, never a domain error variant.
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        match raw {
            "work" => Some(Self::Work),
            "respite" => Some(Self::Respite),
            _ => None,
        }
    }

    /// The canonical text form (the serde unit name).
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        match self {
            Self::Work => "work",
            Self::Respite => "respite",
        }
    }
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

/// Work-only tracked time inside `window` as of `now`: the
/// [`TimeEntryKind::Work`]-filtered [`tracked_in_window`] — the
/// overlap-sum clipping oracle stays the single definition of window
/// arithmetic; the kind filter is the D-11 ruling (respite entries
/// count against the work streak, never toward it).
#[must_use]
pub fn work_in_window<'a>(
    entries: impl IntoIterator<Item = &'a TimeEntry>,
    window: (DateTime<Utc>, DateTime<Utc>),
    now: DateTime<Utc>,
) -> chrono::TimeDelta {
    tracked_in_window(
        entries
            .into_iter()
            .filter(|entry| entry.kind == TimeEntryKind::Work),
        window,
        now,
    )
}

/// A break rule's accumulated-work threshold. Private field; the
/// fallible mint is the command-layer trust boundary — a zero or
/// negative threshold can never enter a row (it would fire immediately
/// and permanently).
///
/// Serde keeps the derived chrono tuple wire shape
/// (`(seconds, nanoseconds)`) but mints through [`WorkThreshold::try_new`]:
/// a corrupt tuple is a data error, never a clamped value (the
/// `ReminderOffset` precedent — every persistence lane stores whole
/// seconds, so the evaluated and persisted forms can never diverge).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkThreshold(chrono::TimeDelta);

impl Serialize for WorkThreshold {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        <(i64, i32) as Serialize>::serialize(
            &(self.0.num_seconds(), self.0.subsec_nanos()),
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for WorkThreshold {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (secs, nanos) = <(i64, i32) as Deserialize>::deserialize(deserializer)?;
        let nanos =
            u32::try_from(nanos).map_err(|_| serde::de::Error::custom("invalid work threshold"))?;
        let dur = chrono::TimeDelta::new(secs, nanos)
            .ok_or_else(|| serde::de::Error::custom("TimeDelta out of bounds"))?;
        Self::try_new(dur).map_err(|_| serde::de::Error::custom("invalid work threshold"))
    }
}

/// The threshold predicate failed (≤ 0 or sub-second).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("work threshold must be a positive whole number of seconds")]
pub struct InvalidWorkThreshold;

impl WorkThreshold {
    /// The mint: accepts the threshold only when strictly positive and
    /// whole-second (the [`crate::reminders::ReminderOffset`] mirror —
    /// every persistence lane stores whole seconds; a sub-second
    /// threshold would diverge from its persisted form on reload and
    /// re-cross at a moved instant).
    ///
    /// # Errors
    ///
    /// [`InvalidWorkThreshold`] when `dur` is not strictly positive, or
    /// carries a sub-second component.
    pub fn try_new(dur: chrono::TimeDelta) -> Result<Self, InvalidWorkThreshold> {
        if dur > chrono::TimeDelta::zero() && dur.subsec_nanos() == 0 {
            Ok(Self(dur))
        } else {
            Err(InvalidWorkThreshold)
        }
    }

    /// The wrapped duration (strictly positive always).
    #[must_use]
    pub const fn as_duration(self) -> chrono::TimeDelta {
        self.0
    }

    /// The canonical text form: rendered as `<h>h<m>m<s>s`, zero
    /// components omitted (`"90m"`, `"1h30m"`). The
    /// [`parse_text_form`](Self::parse_text_form) inverse round-trips it.
    #[must_use]
    pub fn text_form(self) -> String {
        use std::fmt::Write as _;
        let secs = self.0.num_seconds();
        let (h, m, s) = (secs / 3_600, (secs % 3_600) / 60, secs % 60);
        let mut out = String::new();
        if h > 0 {
            let _ = write!(out, "{h}h");
        }
        if m > 0 {
            let _ = write!(out, "{m}m");
        }
        if s > 0 || out.is_empty() {
            let _ = write!(out, "{s}s");
        }
        out
    }

    /// Parses the threshold text form — the existing duration-phrase
    /// grammar (a non-empty sequence of `<unsigned-int><unit>`
    /// components, unit ∈ `s | m | h | d`, no sign, no spaces; the total
    /// must be strictly positive and fit a `TimeDelta`).
    ///
    /// # Errors
    ///
    /// [`InvalidWorkThreshold`] for a garbage form or a non-positive
    /// total.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidWorkThreshold> {
        if raw.is_empty() {
            return Err(InvalidWorkThreshold);
        }
        let bytes = raw.as_bytes();
        let mut total: i64 = 0;
        let mut i = 0;
        while i < bytes.len() {
            let digits_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i == digits_start {
                return Err(InvalidWorkThreshold); // a component without its number
            }
            let Some(value_u64) = raw[digits_start..i].parse::<u64>().ok() else {
                return Err(InvalidWorkThreshold);
            };
            let Some(value) = i64::try_from(value_u64).ok() else {
                return Err(InvalidWorkThreshold);
            };
            let unit_secs = match bytes.get(i) {
                Some(b's') => 1,
                Some(b'm') => 60,
                Some(b'h') => 3_600,
                Some(b'd') => 86_400,
                _ => return Err(InvalidWorkThreshold),
            };
            i += 1;
            let Some(sum) = value
                .checked_mul(unit_secs)
                .and_then(|v| total.checked_add(v))
            else {
                return Err(InvalidWorkThreshold);
            };
            total = sum;
        }
        match chrono::TimeDelta::try_seconds(total) {
            Some(dur) => Self::try_new(dur),
            None => Err(InvalidWorkThreshold),
        }
    }
}

/// The streak boundary: walking entries in `(start, id)` order, each
/// [`TimeEntryKind::Respite`] entry sets the boundary — a *closed*
/// respite to its `end` ("since your last break ended"), an *open*
/// respite to its `start` (a moving boundary would break accumulation
/// monotonicity — the D-8/R-5 ruling). No respite ⇒ the representable
/// floor (all history is one streak). A function of the entries alone —
/// never of `now` — which is what makes law L-1 hold.
fn streak_boundary(entries: &[&TimeEntry]) -> DateTime<Utc> {
    let mut boundary = DateTime::<Utc>::MIN_UTC;
    for entry in entries {
        if entry.kind == TimeEntryKind::Respite {
            boundary = match entry.end {
                Some(end) => end,
                None => entry.start,
            };
        }
    }
    boundary
}

/// Accumulated tracked **work** since the current streak's boundary as
/// of `now`: the boundary walk (each respite-marked entry resets it —
/// [`streak_boundary`]) then [`work_in_window`] over `(boundary, now]`
/// (the overlap-sum clipping convention; overlapping work entries each
/// contribute, rows are never physically split; inverted spans clamp at
/// zero). Pure: no wall reads, no IO — the trigger fn's raw accumulator
/// (laws L-1/L-6 in the law suite).
#[must_use]
pub fn accumulated_work<'a>(
    entries: impl IntoIterator<Item = &'a TimeEntry>,
    now: DateTime<Utc>,
) -> chrono::TimeDelta {
    let mut rows: Vec<&TimeEntry> = entries.into_iter().collect();
    rows.sort_by_key(|entry| (entry.start, entry.id));
    let boundary = streak_boundary(&rows);
    work_in_window(rows.iter().copied(), (boundary, now), now)
}

/// The threshold-crossing instant: the point inside a work span where
/// the cumulative tracked work since the current streak's boundary
/// first reaches `threshold` (`None` below the threshold — the history
/// saturates or accrues but never touches it). Stable while history is
/// stable (the instant lies in the past once crossed; `now` only bounds
/// open accrual), so it is the occurrence key that composes with the
/// `(reminder, occurrence)` suppression guard without a per-tick
/// double-fire (D-8). Pure in `(entries, threshold, now)`.
///
/// Implementation: the accumulated-work curve is continuous and
/// piecewise linear (the overlap-sum over clipped spans), with
/// breakpoints at the clipped span edges plus `now`; the walk finds the
/// first segment whose linear rise touches the threshold.
#[must_use]
pub fn break_crossing<'a>(
    entries: impl IntoIterator<Item = &'a TimeEntry>,
    threshold: WorkThreshold,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let mut rows: Vec<&TimeEntry> = entries.into_iter().collect();
    rows.sort_by_key(|entry| (entry.start, entry.id));
    let boundary = streak_boundary(&rows);
    // The clipped work spans inside the streak window `[boundary, now]`
    // (half-open), inverted shapes skipped (they contribute zero).
    let spans: Vec<(DateTime<Utc>, DateTime<Utc>)> = rows
        .iter()
        .filter(|entry| entry.kind == TimeEntryKind::Work)
        .map(|entry| (entry.start.max(boundary), entry.end.unwrap_or(now).min(now)))
        .filter(|(start, end)| end > start)
        .collect();
    // Breakpoints: every span edge, plus `now` as the final accrual
    // boundary. Sorted, deduplicated; `now` is always present, so the
    // segment walk covers `[boundary, now]` end to end.
    let mut points: Vec<DateTime<Utc>> = spans
        .iter()
        .flat_map(|(start, end)| [*start, *end])
        .chain([now])
        .filter(|point| *point >= boundary && *point <= now)
        .collect();
    points.sort_unstable();
    points.dedup();
    // Accumulated work at an instant: the overlap-sum over the clipped
    // spans (one definition — the same arithmetic `work_in_window`
    // performs over the window).
    let accumulated_at = |at: DateTime<Utc>| -> chrono::TimeDelta {
        spans
            .iter()
            // The spans are pre-clipped at the streak boundary; only the
            // evaluation instant clips the far end.
            .map(|(start, end)| (*end.min(&at) - *start).max(chrono::TimeDelta::zero()))
            .fold(chrono::TimeDelta::zero(), |acc, span| acc + span)
    };
    // Active-span count on a segment starting at `point` (a span is
    // active while `start ≤ point < end`): the segment's constant slope.
    let active_at = |point: DateTime<Utc>| -> usize {
        spans
            .iter()
            .filter(|(start, end)| *start <= point && *end > point)
            .count()
    };
    let threshold_dur = threshold.as_duration();
    let mut acc = chrono::TimeDelta::zero();
    for pair in points.windows(2) {
        let (point, next) = (pair[0], pair[1]);
        let Ok(slope) = i64::try_from(active_at(point)) else {
            continue;
        };
        if slope == 0 {
            acc = accumulated_at(next);
            continue;
        }
        let rise_ns = (threshold_dur - acc)
            .num_nanoseconds()
            .unwrap_or(i64::MAX / 2);
        // Ceiling division at nanosecond granularity (`i64::div_ceil` is
        // unstable): the first instant where the segment's linear rise
        // *reaches* the threshold — a truncating division could land a
        // hair short and fail law L-5's `accumulated_work(c) ≥
        // threshold` half.
        let (quotient, remainder) = (rise_ns / slope, rise_ns % slope);
        let crossing_ns = if remainder == 0 {
            quotient
        } else {
            quotient + 1
        };
        if let Some(crossing) = point
            .checked_add_signed(chrono::TimeDelta::nanoseconds(crossing_ns))
            .filter(|candidate| *candidate <= next)
        {
            return Some(crossing);
        }
        acc = accumulated_at(next);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(raw: u128, start: i64, end: Option<i64>) -> TimeEntry {
        entry_with_kind(raw, start, end, TimeEntryKind::Work)
    }

    fn entry_with_kind(raw: u128, start: i64, end: Option<i64>, kind: TimeEntryKind) -> TimeEntry {
        TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task: TaskId::from(uuid::Uuid::from_u128(900)),
            task_title: "task 900".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
            kind,
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

    // ---- The respite kind (#493, D-1/D-11) -------------------------------

    #[test]
    fn time_entry_kind_serde_unit_names_roundtrip() {
        for (kind, wire) in [
            (TimeEntryKind::Work, "\"work\""),
            (TimeEntryKind::Respite, "\"respite\""),
        ] {
            let json = serde_json::to_string(&kind).unwrap();
            assert_eq!(json, wire, "the serde unit name is the wire/storage text");
            let back: TimeEntryKind = serde_json::from_str(&json).unwrap();
            assert_eq!(back, kind);
        }
        assert_eq!(TimeEntryKind::default(), TimeEntryKind::Work);
        assert_eq!(
            TimeEntryKind::parse_text_form("respite"),
            Some(TimeEntryKind::Respite)
        );
        assert_eq!(TimeEntryKind::parse_text_form("sabbatical"), None);
        assert_eq!(TimeEntryKind::Respite.text_form(), "respite");
    }

    #[test]
    fn old_entry_json_without_kind_decodes_work() {
        // The default-off entity decode: an old journal payload (no
        // `kind` key) folds `Work`.
        let old = serde_json::json!({
            "id": uuid::Uuid::from_u128(1),
            "task": uuid::Uuid::from_u128(900),
            "task_title": "old row",
            "start": ts(1_000),
            "end": ts(2_000),
            "created_at": ts(100),
            "updated_at": ts(100),
        });
        let decoded: TimeEntry = serde_json::from_value(old).unwrap();
        assert_eq!(decoded.kind, TimeEntryKind::Work);
    }

    // ---- WorkThreshold (D-6) -------------------------------------------

    #[test]
    fn work_threshold_mint_admits_only_positive_whole_seconds() {
        assert_eq!(
            WorkThreshold::try_new(chrono::TimeDelta::zero()),
            Err(InvalidWorkThreshold)
        );
        assert_eq!(
            WorkThreshold::try_new(chrono::TimeDelta::try_seconds(-90).unwrap()),
            Err(InvalidWorkThreshold)
        );
        assert_eq!(
            WorkThreshold::try_new(chrono::TimeDelta::try_milliseconds(500).unwrap()),
            Err(InvalidWorkThreshold)
        );
        let threshold =
            WorkThreshold::try_new(chrono::TimeDelta::try_seconds(5_400).unwrap()).unwrap();
        assert_eq!(
            threshold.as_duration(),
            chrono::TimeDelta::try_seconds(5_400).unwrap()
        );
    }

    #[test]
    fn work_threshold_text_form_roundtrips_the_duration_phrases() {
        let cases = [(90_i64, "1m30s"), (5_400, "1h30m"), (86_400, "24h")];
        for (secs, form) in cases {
            let threshold =
                WorkThreshold::try_new(chrono::TimeDelta::try_seconds(secs).unwrap()).unwrap();
            assert_eq!(threshold.text_form(), form);
            assert_eq!(WorkThreshold::parse_text_form(form), Ok(threshold));
        }
        for garbage in ["", "0s", "10min", "-90m", "m", "10", "1h30x"] {
            assert!(
                WorkThreshold::parse_text_form(garbage).is_err(),
                "{garbage:?} must be rejected"
            );
        }
    }

    // ---- work_in_window (D-11) ------------------------------------------

    #[test]
    fn respite_only_window_is_zero() {
        let respite = entry_with_kind(1, 1_000, Some(5_000), TimeEntryKind::Respite);
        let got = work_in_window([&respite], (ts(2_000), ts(9_000)), ts(9_999));
        assert_eq!(got, chrono::TimeDelta::zero());
    }

    // ---- The trigger fns (D-8) deterministic units -----------------------

    fn threshold(secs: i64) -> WorkThreshold {
        WorkThreshold::try_new(chrono::TimeDelta::try_seconds(secs).unwrap()).unwrap()
    }

    #[test]
    fn closed_respite_boundary_is_its_end_and_open_respite_is_its_start() {
        // Work before a closed respite does not survive the reset.
        let early = entry(1, 1_000, Some(2_000));
        let respite = entry_with_kind(2, 3_000, Some(4_000), TimeEntryKind::Respite);
        assert_eq!(
            accumulated_work([&early, &respite], ts(10_000)),
            chrono::TimeDelta::zero(),
            "closed respite: the boundary is its end"
        );
        // An open respite (the running break) anchors at its start.
        let open_respite = entry_with_kind(3, 3_000, None, TimeEntryKind::Respite);
        assert_eq!(
            accumulated_work([&early, &open_respite], ts(10_000)),
            chrono::TimeDelta::zero()
        );
    }

    #[test]
    fn accumulated_work_counts_only_work_inside_the_streak_window() {
        // 60m work, 15m closed respite, then 90m work: the total is the
        // last 90m only.
        let first = entry(1, 0, Some(3_600));
        let respite = entry_with_kind(2, 3_600, Some(4_500), TimeEntryKind::Respite);
        let second = entry(3, 4_500, Some(9_900));
        assert_eq!(
            accumulated_work([&first, &respite, &second], ts(9_900)),
            chrono::TimeDelta::try_seconds(5_400).unwrap()
        );
        // An open work entry accrues to `now`.
        let running = entry(4, 9_900, None);
        assert_eq!(
            accumulated_work([&first, &respite, &running], ts(10_800)),
            chrono::TimeDelta::try_seconds(900).unwrap()
        );
    }

    #[test]
    fn multiple_respites_walk_order_and_the_last_one_wins() {
        let a = entry_with_kind(1, 1_000, Some(1_500), TimeEntryKind::Respite);
        let work = entry(2, 2_000, Some(3_000));
        let b = entry_with_kind(3, 4_000, Some(4_200), TimeEntryKind::Respite);
        let work2 = entry(4, 5_000, Some(6_000));
        // Total work after the last respite only.
        let got = accumulated_work([&a, &work, &b, &work2], ts(6_000));
        assert_eq!(got, chrono::TimeDelta::try_seconds(1_000).unwrap());
    }

    #[test]
    fn retroactive_respite_moves_or_retracts_the_crossing() {
        // 90m of work; a retroactive respite in the middle resets the
        // accumulator: the earlier crossing instant no longer stands.
        let work = entry(1, 0, Some(9_000));
        let crossing = break_crossing([&work], threshold(5_400), ts(9_999)).unwrap();
        assert_eq!(crossing, ts(5_400));
        let respite = entry_with_kind(2, 2_000, Some(3_000), TimeEntryKind::Respite);
        let after = break_crossing([&work, &respite], threshold(5_400), ts(9_999));
        assert_eq!(
            after,
            Some(ts(3_000 + 5_400)),
            "the crossing re-derives from the post-respite remainder"
        );
        let retract = entry_with_kind(3, 2_000, Some(4_000), TimeEntryKind::Respite);
        assert_eq!(
            break_crossing([&work, &retract], threshold(5_400), ts(9_999)),
            None,
            "a retroactive respite that leaves only 5_000s of work never crosses a 5_400s threshold"
        );
    }

    #[test]
    fn inverted_and_degenerate_spans_contribute_zero_and_crossing_stays_inside_a_span() {
        // An inverted raw span contributes nothing (the clamp).
        let inverted = entry(1, 5_000, Some(4_000));
        assert_eq!(
            accumulated_work([&inverted], ts(9_999)),
            chrono::TimeDelta::zero()
        );
        // The crossing lands inside the (single) span, not at its end.
        let work = entry(2, 1_000, Some(4_600));
        assert_eq!(
            break_crossing([&work], threshold(3_000), ts(9_999)),
            Some(ts(4_000)),
            "started at 1000, crosses 3000 at 4000"
        );
    }

    #[test]
    fn crossing_inside_an_open_entry_accrues_to_now() {
        let running = entry(1, 1_000, None);
        // At now = 3000 only 2000 accrued: no crossing.
        assert_eq!(
            break_crossing([&running], threshold(3_000), ts(3_000)),
            None
        );
        // At now = 4000 the threshold touches inside the open span.
        assert_eq!(
            break_crossing([&running], threshold(3_000), ts(4_000)),
            Some(ts(4_000))
        );
        // Past the touch the crossing instant does not move (L-4 shape).
        assert_eq!(
            break_crossing([&running], threshold(3_000), ts(9_999)),
            Some(ts(4_000))
        );
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        // L-1: fixed entries, the accumulator is monotone in `now`.
        #[test]
        fn l1_accumulated_work_is_monotone_in_now(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            t1 in timestamp_strategy(),
            t2 in timestamp_strategy(),
        ) {
            let (early, late) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            prop_assert!(
                accumulated_work(&entries, early) <= accumulated_work(&entries, late)
            );
        }

        // L-2: work wholly before a closed respite, nothing after — the
        // accumulator is exactly zero once the respite has ended.
        #[test]
        fn l2_respite_resets_the_accumulator_to_zero(
            work_start in timestamp_strategy(),
            work_secs in 1_i64..=3_600,
            gap_secs in 1_i64..=600,
            respite_secs in 1_i64..=600,
            probe_secs in 0_i64..=3_600,
        ) {
            let work_end = work_start + chrono::TimeDelta::try_seconds(work_secs).unwrap();
            let respite_start = work_end + chrono::TimeDelta::try_seconds(gap_secs).unwrap();
            let respite_end = respite_start + chrono::TimeDelta::try_seconds(respite_secs).unwrap();
            let work = entry(1, 0, None);
            let work = TimeEntry { start: work_start, end: Some(work_end), ..work };
            let respite = entry_with_kind(2, 0, Some(0), TimeEntryKind::Respite);
            let respite = TimeEntry { start: respite_start, end: Some(respite_end), ..respite };
            let probe = respite_end + chrono::TimeDelta::try_seconds(probe_secs).unwrap();
            let got = accumulated_work([&work, &respite], probe);
            prop_assert_eq!(got, chrono::TimeDelta::zero());
            // And no crossing either (nothing accumulates).
            prop_assert_eq!(
                break_crossing([&work, &respite], threshold(60), probe),
                None
            );
        }

        // L-3: below the threshold there is no crossing — and the
        // evaluator-side consequence (no firing) is N.2's pin.
        #[test]
        fn l3_no_crossing_below_the_threshold(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            threshold_secs in 60_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let t = threshold(threshold_secs);
            let got = break_crossing(&entries, t, now);
            if let Some(crossing) = got {
                prop_assert!(accumulated_work(&entries, crossing) >= t.as_duration());
            }
        }

        // L-4: the crossing instant is stable once reached — `now`
        // advancing never moves it (fixed entries).
        #[test]
        fn l4_crossing_is_stable_as_now_advances(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            t1 in timestamp_strategy(),
            t2 in timestamp_strategy(),
            threshold_secs in 1_i64..=3_600,
        ) {
            let (early, late) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
            let t = WorkThreshold::try_new(chrono::TimeDelta::try_seconds(threshold_secs).unwrap()).unwrap();
            match (break_crossing(&entries, t, early), break_crossing(&entries, t, late)) {
                (Some(c1), Some(c2)) => prop_assert_eq!(c1, c2),
                // Earlier `now` may be below the accrual reach while the
                // later `now` has crossed; the reverse is impossible.
                (Some(_), None) => prop_assert!(false, "a reached crossing vanished"),
                (None, _) => {}
            }
        }

        // L-5: the crossing is the exact first-touch point.
        #[test]
        fn l5_crossing_is_the_exact_first_touch_point(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            threshold_secs in 1_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let Some(t_raw) = chrono::TimeDelta::try_seconds(threshold_secs) else { return Ok(()) };
            let t = WorkThreshold::try_new(t_raw).unwrap();
            if let Some(crossing) = break_crossing(&entries, t, now) {
                prop_assert!(
                    accumulated_work(&entries, crossing) >= t.as_duration(),
                    "the crossing instant has reached the threshold"
                );
                let eps = chrono::TimeDelta::nanoseconds(1);
                let Some(before) = crossing.checked_sub_signed(eps) else { return Ok(()) };
                prop_assert!(
                    accumulated_work(&entries, before) < t.as_duration(),
                    "a hair earlier it had not (first touch, not later)"
                );
                prop_assert!(crossing <= now);
            }
        }

        // L-6: the accumulator is the window oracle over the boundary
        // walk (one definition of clipping, two doors).
        #[test]
        fn l6_accumulated_work_equals_the_boundary_window_oracle(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            now in timestamp_strategy(),
        ) {
            let mut rows: Vec<&TimeEntry> = entries.iter().collect();
            rows.sort_by_key(|entry| (entry.start, entry.id));
            let mut boundary = DateTime::<Utc>::MIN_UTC;
            for entry in &rows {
                if entry.kind == TimeEntryKind::Respite {
                    boundary = entry.end.unwrap_or(entry.start);
                }
            }
            let oracle = tracked_in_window(
                rows.iter().copied().filter(|entry| entry.kind == TimeEntryKind::Work),
                (boundary, now),
                now,
            );
            prop_assert_eq!(accumulated_work(&entries, now), oracle);
        }

        // D-11 oracle: `work_in_window` is `tracked_in_window` over the
        // `Work`-filtered rows.
        #[test]
        fn work_in_window_matches_the_kind_filtered_oracle_prop(
            entries in prop::collection::vec(time_entry_strategy(), 0..6),
            window_start in timestamp_strategy(),
            window_secs in 1_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let window_end = window_start + chrono::TimeDelta::try_seconds(window_secs).unwrap();
            let oracle = tracked_in_window(
                entries.iter().filter(|entry| entry.kind == TimeEntryKind::Work),
                (window_start, window_end),
                now,
            );
            prop_assert_eq!(
                work_in_window(&entries, (window_start, window_end), now),
                oracle
            );
        }
    }
}
