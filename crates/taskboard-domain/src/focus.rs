// SPDX-License-Identifier: MIT OR Apache-2.0
//! Focus-mode block durations ([`FocusTiming`], issue #496) and the
//! focus-chain walk (issue #600): injected configuration data and one
//! pure derivation consumed by the app-layer focus-session runtime and
//! the ui-slint concentration overlay.
//!
//! The serde lane (DR-9 D-BF3, the roaming `settings` section): the
//! four numbers as a plain integer-seconds struct — a decode that fails
//! the [`FocusTiming::try_new`] mint is a typed serde error, never a
//! silent default (the config-load trust boundary stays the raw-integer
//! mint; this lane only carries already-minted rows).

use std::collections::{BTreeMap, BTreeSet};

use chrono::TimeDelta;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ids::TimeEntryId;
use crate::tracking::{TimeEntry, TimeEntryKind};

/// Focus-mode block durations and the long-break cadence. Private
/// fields; the fallible mint is the config-load trust boundary — every
/// duration is strictly positive whole-second (a zero-length block
/// would fire its transition instantly and forever) and the cadence
/// count is at least one (zero would make the modulo law meaningless).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTiming {
    work: TimeDelta,
    respite: TimeDelta,
    long_break: TimeDelta,
    long_break_every: u32,
}

/// The focus-timing predicate failed, naming the offending field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InvalidFocusTiming {
    /// The work block duration is not strictly positive whole seconds.
    #[error("focus work duration must be a positive whole number of seconds")]
    Work,
    /// The respite (break) block duration is not strictly positive
    /// whole seconds.
    #[error("focus respite duration must be a positive whole number of seconds")]
    Respite,
    /// The long-break duration is not strictly positive whole seconds.
    #[error("focus long-break duration must be a positive whole number of seconds")]
    LongBreak,
    /// The long-break cadence count is zero (every Nth break is long
    /// requires N ≥ 1).
    #[error("focus long-break cadence must be at least one")]
    Every,
}

impl FocusTiming {
    /// The mint: accepts the timing only when every duration is
    /// strictly positive and whole-second (the
    /// [`crate::tracking::WorkThreshold`] rules — every consumer
    /// derives deadlines by adding these to entry starts, and a
    /// sub-second component would re-cross at a moved instant) and the
    /// cadence count is ≥ 1.
    ///
    /// # Errors
    ///
    /// [`InvalidFocusTiming::Work`], [`InvalidFocusTiming::Respite`],
    /// [`InvalidFocusTiming::LongBreak`], or
    /// [`InvalidFocusTiming::Every`] when the respective predicate
    /// fails.
    pub fn try_new(
        work: TimeDelta,
        respite: TimeDelta,
        long_break: TimeDelta,
        long_break_every: u32,
    ) -> Result<Self, InvalidFocusTiming> {
        let valid = |dur: TimeDelta| dur > TimeDelta::zero() && dur.subsec_nanos() == 0;
        if !valid(work) {
            return Err(InvalidFocusTiming::Work);
        }
        if !valid(respite) {
            return Err(InvalidFocusTiming::Respite);
        }
        if !valid(long_break) {
            return Err(InvalidFocusTiming::LongBreak);
        }
        if long_break_every == 0 {
            return Err(InvalidFocusTiming::Every);
        }
        Ok(Self {
            work,
            respite,
            long_break,
            long_break_every,
        })
    }

    /// The validated work-block duration (strictly positive always).
    #[must_use]
    pub const fn work(self) -> TimeDelta {
        self.work
    }

    /// The validated respite (short-break) block duration (strictly
    /// positive always).
    #[must_use]
    pub const fn respite(self) -> TimeDelta {
        self.respite
    }

    /// The validated long-break duration (strictly positive always).
    #[must_use]
    pub const fn long_break(self) -> TimeDelta {
        self.long_break
    }

    /// The long-break cadence: every Nth completed work block of a
    /// cycle hands off to the long break (always ≥ 1).
    #[must_use]
    pub const fn long_break_every(self) -> u32 {
        self.long_break_every
    }

    /// The chain gap tolerance (issue #600, D-FF4): the longest
    /// sanctioned pause between two consecutive blocks of one cycle.
    /// Derived from the user's own knobs, never config — recorded
    /// leniency: a `long_break_mins` of 240 makes a 4-hour gap
    /// chain-continuing.
    #[must_use]
    pub fn chain_tolerance(self) -> TimeDelta {
        std::cmp::max(self.respite, self.long_break)
    }
}

/// How many work blocks the live cycle has completed, counting the
/// given open work entry. Derived from closed-entry history at every
/// call — never stored (issue #600, D-FF1: an accumulated counter plus
/// a recovery rule is two derivations that must agree; the walk is one,
/// so a restart mid-cycle re-derives the same position and the
/// double-count bug class does not exist).
///
/// The backwards walk (one core shared with [`focus_break_is_long`]):
/// each hop moves to the closed entry with the greatest
/// `(end, id)` satisfying `end ≤ current.start` and
/// `current.start − end ≤` the chain tolerance — overlapping or later
/// rows (the transient double-open ghost class) never qualify. Any
/// kind is crossed (a task switch without a break is the same flow;
/// Work→Work hops count), only `Work` entries increment the count, and
/// the walk stops at a **long break**: a closed `Respite` whose
/// recorded duration is `≥ timing.long_break()`, or whose id is the
/// caller's `decided_long` anchor (the runtime's owned decision —
/// history cannot recover it when the break closed early). Short
/// respites are crossed, not stopping. No alternation is required; a
/// cold history ends the walk.
#[must_use]
pub fn focus_work_position(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    open_work: &TimeEntry,
    timing: &FocusTiming,
    decided_long: Option<TimeEntryId>,
) -> u32 {
    walk_works(entries, open_work, timing, decided_long)
}

/// Whether the currently open respite is a long break: true iff the
/// cycle position of the work block immediately preceding it hits the
/// cadence (`position % long_break_every == 0`, position > 0 — a cold
/// break is short). The walk counts the works strictly preceding the
/// break, which is the same number the runtime's hand-off used — the
/// two computations agree structurally because they are one walk
/// (issue #600, D-FF3). Duration inference is anchor-free here: this
/// break is being classified, not crossed.
#[must_use]
pub fn focus_break_is_long(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    open_respite: &TimeEntry,
    timing: &FocusTiming,
    decided_long: Option<TimeEntryId>,
) -> bool {
    let position = walk_works(entries, open_respite, timing, decided_long);
    position > 0 && position.is_multiple_of(timing.long_break_every())
}

/// The shared backwards walk: the number of `Work` entries on the
/// chain ending at `from` (inclusive), stopping at a long respite or a
/// gap beyond the tolerance.
fn walk_works(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    from: &TimeEntry,
    timing: &FocusTiming,
    decided_long: Option<TimeEntryId>,
) -> u32 {
    let tolerance = timing.chain_tolerance();
    let mut count = u32::from(from.kind == TimeEntryKind::Work);
    let mut visited = BTreeSet::from([from.id]);
    let mut cur = from;
    loop {
        let Some(pred) = entries
            .values()
            .filter(|e| e.end.is_some())
            .filter(|e| {
                let end = e.end.expect("filtered above");
                end <= cur.start && cur.start - end <= tolerance
            })
            .max_by_key(|e| (e.end.expect("filtered above"), e.id))
        else {
            return count;
        };
        if !visited.insert(pred.id) {
            return count;
        }
        if pred.kind == TimeEntryKind::Respite
            && (pred.end.expect("filtered above") - pred.start >= timing.long_break()
                || Some(pred.id) == decided_long)
        {
            return count;
        }
        if pred.kind == TimeEntryKind::Work {
            count += 1;
        }
        cur = pred;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn timing() -> FocusTiming {
        FocusTiming::try_new(
            Duration::minutes(25),
            Duration::minutes(5),
            Duration::minutes(15),
            4,
        )
        .expect("positive whole-second durations mint")
    }

    #[test]
    fn focus_timing_mints_positive_whole_second_durations() {
        let timing = FocusTiming::try_new(
            Duration::minutes(25),
            Duration::minutes(5),
            Duration::minutes(15),
            4,
        )
        .expect("positive whole-second durations mint");
        assert_eq!(timing.work(), Duration::minutes(25));
        assert_eq!(timing.respite(), Duration::minutes(5));
        assert_eq!(timing.long_break(), Duration::minutes(15));
        assert_eq!(timing.long_break_every(), 4);
    }

    #[test]
    fn zero_or_negative_work_or_respite_rejected_typed() {
        for (work, respite, expected) in [
            (
                Duration::zero(),
                Duration::minutes(5),
                InvalidFocusTiming::Work,
            ),
            (
                Duration::minutes(-25),
                Duration::minutes(5),
                InvalidFocusTiming::Work,
            ),
            (
                Duration::minutes(25),
                Duration::zero(),
                InvalidFocusTiming::Respite,
            ),
            (
                Duration::minutes(25),
                Duration::seconds(-5),
                InvalidFocusTiming::Respite,
            ),
        ] {
            let err = FocusTiming::try_new(work, respite, Duration::minutes(15), 4)
                .expect_err("a non-positive duration never mints");
            assert_eq!(err, expected);
        }
    }

    #[test]
    fn zero_or_sub_second_long_break_rejected_typed() {
        for long_break in [Duration::zero(), Duration::seconds(-15)] {
            let err =
                FocusTiming::try_new(Duration::minutes(25), Duration::minutes(5), long_break, 4)
                    .expect_err("a non-positive long break never mints");
            assert_eq!(err, InvalidFocusTiming::LongBreak);
        }
        let err = FocusTiming::try_new(
            Duration::minutes(25),
            Duration::minutes(5),
            Duration::seconds(15) + Duration::milliseconds(500),
            4,
        )
        .expect_err("a sub-second long break never mints");
        assert_eq!(err, InvalidFocusTiming::LongBreak);
    }

    #[test]
    fn zero_long_break_every_rejected_typed() {
        let err = FocusTiming::try_new(
            Duration::minutes(25),
            Duration::minutes(5),
            Duration::minutes(15),
            0,
        )
        .expect_err("a zero cadence never mints");
        assert_eq!(err, InvalidFocusTiming::Every);
    }

    #[test]
    fn accessors_round_trip_the_cadence() {
        let timing = FocusTiming::try_new(
            Duration::seconds(1500),
            Duration::seconds(300),
            Duration::seconds(900),
            6,
        )
        .expect("positive whole-second durations mint");
        assert_eq!(timing.work().num_seconds(), 1500);
        assert_eq!(timing.respite().num_seconds(), 300);
        assert_eq!(timing.long_break().num_seconds(), 900);
        assert_eq!(timing.long_break_every(), 6);
        // The tolerance follows the longer of the two pauses.
        assert_eq!(timing.chain_tolerance(), Duration::seconds(900));
        let short_long = FocusTiming::try_new(
            Duration::seconds(1500),
            Duration::seconds(300),
            Duration::seconds(200),
            4,
        )
        .expect("mints");
        assert_eq!(short_long.chain_tolerance(), Duration::seconds(300));
    }

    // -- the walk fixtures ------------------------------------------------

    const T0: chrono::DateTime<chrono::Utc> =
        chrono::DateTime::<chrono::Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    fn m(mins: i64) -> chrono::TimeDelta {
        chrono::TimeDelta::minutes(mins)
    }

    fn row(
        kind: TimeEntryKind,
        start: chrono::DateTime<chrono::Utc>,
        end: Option<chrono::DateTime<chrono::Utc>>,
    ) -> TimeEntry {
        let mut e = TimeEntry {
            id: TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            task: crate::ids::TaskId::from_uuid(uuid::Uuid::now_v7()),
            task_title: "t".into(),
            start,
            end,
            created_at: start,
            updated_at: start,
            kind,
        };
        if let Some(end) = end {
            e.updated_at = end;
        }
        e
    }

    fn map(rows: &[TimeEntry]) -> BTreeMap<TimeEntryId, TimeEntry> {
        rows.iter().map(|e| (e.id, e.clone())).collect()
    }

    /// `W(s,e)` closed work, `R(s,e)` closed respite, `open_work`/`open_respite`.
    fn w(start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>) -> TimeEntry {
        row(TimeEntryKind::Work, start, Some(end))
    }

    fn r(start: chrono::DateTime<chrono::Utc>, end: chrono::DateTime<chrono::Utc>) -> TimeEntry {
        row(TimeEntryKind::Respite, start, Some(end))
    }

    #[test]
    fn a_fresh_work_block_counts_itself() {
        let open = row(TimeEntryKind::Work, T0, None);
        let entries = map(&[]);
        assert_eq!(
            focus_work_position(&entries, &open, &timing(), None),
            1,
            "cold history: the open block is position 1"
        );
    }

    #[test]
    fn consecutive_blocks_with_breaks_accumulate() {
        // W R W R W + open W ⇒ 4 works in the live cycle.
        let c1 = w(T0, T0 + m(25));
        let b1 = r(T0 + m(25), T0 + m(30));
        let c2 = w(T0 + m(30), T0 + m(55));
        let b2 = r(T0 + m(55), T0 + m(60));
        let c3 = w(T0 + m(60), T0 + m(85));
        let open = row(TimeEntryKind::Work, T0 + m(85), None);
        let entries = map(&[c1, b1, c2, b2, c3, open.clone()]);
        assert_eq!(focus_work_position(&entries, &open, &timing(), None), 4);
    }

    #[test]
    fn work_to_work_hops_cross_and_count() {
        // A task switch without a break is the same pomodoro flow.
        let c1 = w(T0, T0 + m(25));
        let c2 = w(T0 + m(25), T0 + m(50));
        let open = row(TimeEntryKind::Work, T0 + m(50), None);
        let entries = map(&[c1, c2, open.clone()]);
        assert_eq!(focus_work_position(&entries, &open, &timing(), None), 3);
    }

    #[test]
    fn a_long_break_ends_the_cycle() {
        let c1 = w(T0, T0 + m(25));
        let b1 = r(T0 + m(25), T0 + m(30));
        let long = r(T0 + m(30), T0 + m(45)); // 15m ≥ long_break
        let open = row(TimeEntryKind::Work, T0 + m(45), None);
        let entries = map(&[c1, b1, long, open.clone()]);
        assert_eq!(
            focus_work_position(&entries, &open, &timing(), None),
            1,
            "the long break stops the walk: the new cycle counts itself only"
        );
    }

    #[test]
    fn the_decided_long_anchor_ends_the_cycle_even_when_the_duration_undercuts() {
        // The memory-vs-history pin: a runtime-opened long break stopped
        // early looks short in history; the anchor alone declares it.
        let c1 = w(T0, T0 + m(25));
        let b1 = r(T0 + m(25), T0 + m(30));
        let aborted_long = r(T0 + m(30), T0 + m(32)); // 2m < long_break
        let open = row(TimeEntryKind::Work, T0 + m(32), None);
        let entries = map(&[c1, b1, aborted_long.clone(), open.clone()]);
        assert_eq!(
            focus_work_position(&entries, &open, &timing(), None),
            2,
            "without the anchor the short-looking respite is crossed"
        );
        assert_eq!(
            focus_work_position(&entries, &open, &timing(), Some(aborted_long.id)),
            1,
            "with the anchor the walk stops: the cycle reset holds"
        );
    }

    #[test]
    fn a_medium_manual_respite_is_crossed_not_stopping() {
        // Duration between respite and long_break: crossed, counted
        // neither as stop nor as work.
        let c1 = w(T0, T0 + m(25));
        let medium = r(T0 + m(25), T0 + m(25) + m(10));
        let open = row(TimeEntryKind::Work, T0 + m(25) + m(10), None);
        let entries = map(&[c1, medium, open.clone()]);
        assert_eq!(focus_work_position(&entries, &open, &timing(), None), 2);
    }

    #[test]
    fn a_gap_beyond_the_tolerance_starts_a_new_cycle() {
        let c1 = w(T0, T0 + m(25));
        // A gap of one hour: beyond max(respite, long_break) = 15m.
        let open = row(TimeEntryKind::Work, T0 + m(25) + m(60), None);
        let entries = map(&[c1, open.clone()]);
        assert_eq!(focus_work_position(&entries, &open, &timing(), None), 1);
    }

    #[test]
    fn a_gap_within_the_tolerance_continues() {
        let c1 = w(T0, T0 + m(25));
        // A 14-minute gap: within the 15-minute tolerance.
        let open = row(TimeEntryKind::Work, T0 + m(25) + m(14), None);
        let entries = map(&[c1, open.clone()]);
        assert_eq!(focus_work_position(&entries, &open, &timing(), None), 2);
    }

    #[test]
    fn overlapping_rows_never_qualify_as_predecessors() {
        // The double-open ghost: an overlapping later row is skipped,
        // the true predecessor governs.
        let true_pred = w(T0, T0 + m(25));
        let ghost = w(T0 + m(20), T0 + m(70)); // overlaps AND ends after open's start
        let open = row(TimeEntryKind::Work, T0 + m(30), None);
        let entries = map(&[true_pred, ghost, open.clone()]);
        assert_eq!(
            focus_work_position(&entries, &open, &timing(), None),
            2,
            "the ghost is not a predecessor (its end is past the open start); the true pred counts"
        );
    }

    #[test]
    fn ties_break_by_the_higher_id() {
        // Two closed works ending at the same instant: the walk's
        // `(end, id)` maximum picks exactly one predecessor — the
        // chain stays deterministic (one work counted, never two).
        let mut low = w(T0, T0 + m(25));
        let mut high = w(T0, T0 + m(25));
        if high.id < low.id {
            std::mem::swap(&mut low, &mut high);
        }
        let open = row(TimeEntryKind::Work, T0 + m(25), None);
        let entries = map(&[low, high, open.clone()]);
        let position = focus_work_position(&entries, &open, &timing(), None);
        assert_eq!(
            position, 2,
            "the tie resolves to one predecessor, counted once"
        );
    }

    #[test]
    fn an_open_break_is_long_exactly_when_its_preceding_position_hits_the_cadence() {
        let timing = timing(); // every = 4

        // Cold break: position 0 ⇒ short (0 % every excluded).
        let cold = row(TimeEntryKind::Respite, T0, None);
        let entries = map(std::slice::from_ref(&cold));
        assert!(!focus_break_is_long(&entries, &cold, &timing, None));

        // One preceding work ⇒ 1 % 4 ≠ 0 ⇒ short.
        let c1 = w(T0, T0 + m(25));
        let short = row(TimeEntryKind::Respite, T0 + m(25), None);
        let entries = map(&[c1.clone(), short.clone()]);
        assert!(!focus_break_is_long(&entries, &short, &timing, None));

        // Four preceding works (with short breaks between) ⇒ long.
        let b1 = r(T0 + m(25), T0 + m(30));
        let c2 = w(T0 + m(30), T0 + m(55));
        let b2 = r(T0 + m(55), T0 + m(60));
        let c3 = w(T0 + m(60), T0 + m(85));
        let b3 = r(T0 + m(85), T0 + m(90));
        let c4 = w(T0 + m(90), T0 + m(115));
        let long = row(TimeEntryKind::Respite, T0 + m(115), None);
        let entries = map(&[c1, b1, c2, b2, c3, b3, c4, long.clone()]);
        assert!(focus_break_is_long(&entries, &long, &timing, None));
    }
}

/// The `FocusTiming` serde wire shape (module docs): whole seconds.
/// Crate-shared because the settings snapshot's `Option<FocusTiming>`
/// lane (`settings.rs::focus_timing_serde`) rides the same grammar —
/// one wire shape, two decode postures (strict here, lenient there).
#[derive(Serialize, Deserialize)]
pub(crate) struct FocusTimingWire {
    pub(crate) work_secs: i64,
    pub(crate) respite_secs: i64,
    pub(crate) long_break_secs: i64,
    pub(crate) long_break_every: u32,
}

impl FocusTimingWire {
    pub(crate) fn of(timing: &FocusTiming) -> Self {
        Self {
            work_secs: timing.work.num_seconds(),
            respite_secs: timing.respite.num_seconds(),
            long_break_secs: timing.long_break.num_seconds(),
            long_break_every: timing.long_break_every,
        }
    }
}

impl Serialize for FocusTiming {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        FocusTimingWire::of(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FocusTiming {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = FocusTimingWire::deserialize(deserializer)?;
        Self::try_new(
            TimeDelta::seconds(wire.work_secs),
            TimeDelta::seconds(wire.respite_secs),
            TimeDelta::seconds(wire.long_break_secs),
            wire.long_break_every,
        )
        .map_err(serde::de::Error::custom)
    }
}
