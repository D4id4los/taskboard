// SPDX-License-Identifier: MIT OR Apache-2.0

//! Recurring-series rules: the typed grammar (D-C3) and its A1 mints.
//!
//! Every numeric component is a hand-rolled private-field newtype with a
//! fallible mint (the [`ReminderOffset`](crate::reminders::ReminderOffset)
//! exemplar; AGENTS §2's A1 ruling): zero intervals, an empty weekday
//! set, and out-of-range day/nth components are unrepresentable. No
//! derive crates, no RRULE strings anywhere.
//!
//! # Decode leniency (the recorded ruling, one layer out)
//!
//! The mints' serde impls are **strict by design**: a stored or wire
//! value that fails the mint surfaces as a decode error — an
//! `every: 0` rule cannot deserialize, ever. The *leniency* lives one
//! layer out, at the untrusted decode boundaries: the appdata codec
//! demotes an undecodable `recur` value to verbatim unmodeled content
//! (the document never fails), and the sqlite codec reports a typed
//! `Corrupted`. Leniency without this comment would be indistinguishable
//! from a gap; strictness without it would read like one too.

use std::collections::BTreeSet;

use chrono::{Datelike as _, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::ids::{HabitId, SeriesId};

/// Why a [`RecurRule`] component's mint rejected a value. One variant
/// per rejected shape; carries the offending input where a range was
/// violated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InvalidRecur {
    /// An interval or count must be at least 1 (a zero interval repeats
    /// every no days; a zero-count series is exhausted at birth).
    #[error("interval/count must be at least 1")]
    ZeroInterval,
    /// A weekday set must not be empty.
    #[error("weekday set must not be empty")]
    EmptyWeekdaySet,
    /// A day-of-month anchor outside `1..=31`.
    #[error("day of month out of range 1..=31")]
    DayOutOfRange {
        /// The rejected day number verbatim.
        got: u32,
    },
    /// An nth-weekday position outside `1..=5`.
    #[error("nth weekday out of range 1..=5")]
    WeekOutOfRange {
        /// The rejected position verbatim.
        got: u32,
    },
}

/// Repeat interval in whole days; the inner value is never 0.
///
/// # Minting vs. leniency
///
/// [`EveryDays::new`] is the only construction path. The serde impl is
/// **strict decode by design**: a stored/wire value of `0` fails the
/// mint and surfaces as a decode error — never a silently-wrong rule.
/// The leniency lives one layer out (appdata demotion / sqlite
/// `Corrupted`; see the module doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u16")]
pub struct EveryDays(u16);

impl EveryDays {
    /// The mint: `n` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::ZeroInterval`] when `n` is 0.
    pub const fn new(n: u16) -> Result<Self, InvalidRecur> {
        if n == 0 {
            Err(InvalidRecur::ZeroInterval)
        } else {
            Ok(Self(n))
        }
    }

    /// The interval in days (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for EveryDays {
    type Error = InvalidRecur;

    fn try_from(n: u16) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// Repeat interval in whole weeks; the inner value is never 0.
///
/// Strict decode by design — see [`EveryDays`] and the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u16")]
pub struct EveryWeeks(u16);

impl EveryWeeks {
    /// The mint: `n` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::ZeroInterval`] when `n` is 0.
    pub const fn new(n: u16) -> Result<Self, InvalidRecur> {
        if n == 0 {
            Err(InvalidRecur::ZeroInterval)
        } else {
            Ok(Self(n))
        }
    }

    /// The interval in weeks (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

impl TryFrom<u16> for EveryWeeks {
    type Error = InvalidRecur;

    fn try_from(n: u16) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// A non-empty, canonical set of weekdays (canonical order = Mon..Sun;
/// `chrono::Weekday` itself carries no `Ord`, so the members are stored
/// by their `num_days_from_monday` index). Duplicates dedup silently at
/// the mint — set semantics, not an error.
///
/// Strict decode by design — see [`EveryDays`] and the module doc.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "Vec<Weekday>")]
pub struct WeekdaySet(BTreeSet<u32>);

impl Serialize for WeekdaySet {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

impl WeekdaySet {
    /// The mint from any weekday iterable; empty input is rejected.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::EmptyWeekdaySet`] when the input has no elements
    /// (after set dedup — which is a no-op on duplicates).
    pub fn new(days: impl IntoIterator<Item = Weekday>) -> Result<Self, InvalidRecur> {
        let set: BTreeSet<u32> = days.into_iter().map(|d| d.num_days_from_monday()).collect();
        if set.is_empty() {
            Err(InvalidRecur::EmptyWeekdaySet)
        } else {
            Ok(Self(set))
        }
    }

    /// `true` when `day` is a member.
    #[must_use]
    pub fn contains(&self, day: Weekday) -> bool {
        self.0.contains(&day.num_days_from_monday())
    }

    /// The members in canonical (Mon..Sun) order.
    pub fn iter(&self) -> impl Iterator<Item = Weekday> + '_ {
        self.0.iter().map(|index| match index {
            0 => Weekday::Mon,
            1 => Weekday::Tue,
            2 => Weekday::Wed,
            3 => Weekday::Thu,
            4 => Weekday::Fri,
            5 => Weekday::Sat,
            _ => Weekday::Sun,
        })
    }

    /// The member count (≥ 1 always; the mint guarantees non-emptiness,
    /// the accessor stays total for symmetry).
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always `false` by the mint's invariant; total for API symmetry.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }
}

impl TryFrom<Vec<Weekday>> for WeekdaySet {
    type Error = InvalidRecur;

    fn try_from(days: Vec<Weekday>) -> Result<Self, Self::Error> {
        Self::new(days)
    }
}

/// A monthly day-of-month anchor, `1..=31`. Short months clamp at
/// scheduling time (Feb 31 → Feb 28/29) — the clamp is the membership
/// predicate, not a mint concern.
///
/// Strict decode by design — see [`EveryDays`] and the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32")]
pub struct DayOfMonth(u8);

impl DayOfMonth {
    /// The mint: `d` must be in `1..=31`.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::DayOutOfRange`] with `d` verbatim when outside
    /// `1..=31`.
    pub fn new(d: u32) -> Result<Self, InvalidRecur> {
        let day = u8::try_from(d).map_err(|_| InvalidRecur::DayOutOfRange { got: d })?;
        if (1..=31).contains(&day) {
            Ok(Self(day))
        } else {
            Err(InvalidRecur::DayOutOfRange { got: d })
        }
    }

    /// The anchored day number (`1..=31`).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u32> for DayOfMonth {
    type Error = InvalidRecur;

    fn try_from(d: u32) -> Result<Self, Self::Error> {
        Self::new(d)
    }
}

/// The nth weekday-of-month position, `1..=5` (a month lacking a 5th
/// weekday contributes no occurrence — no clamping at this anchor).
///
/// Strict decode by design — see [`EveryDays`] and the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32")]
pub struct NthWeek(u8);

impl NthWeek {
    /// The mint: `n` must be in `1..=5`.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::WeekOutOfRange`] with `n` verbatim when outside
    /// `1..=5`.
    pub fn new(n: u32) -> Result<Self, InvalidRecur> {
        let week = u8::try_from(n).map_err(|_| InvalidRecur::WeekOutOfRange { got: n })?;
        if (1..=5).contains(&week) {
            Ok(Self(week))
        } else {
            Err(InvalidRecur::WeekOutOfRange { got: n })
        }
    }

    /// The position (`1..=5`).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u32> for NthWeek {
    type Error = InvalidRecur;

    fn try_from(n: u32) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// An occurrence-count bound; the inner value is never 0 (a zero-count
/// series is exhausted at birth — meaningless, rejected). The count is
/// evaluated against completion history at the engine; the pure
/// arithmetic never sees it.
///
/// Strict decode by design — see [`EveryDays`] and the module doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32")]
pub struct OccurrenceCount(u32);

impl OccurrenceCount {
    /// The mint: `n` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::ZeroInterval`] when `n` is 0 (the shared
    /// "must be ≥ 1" variant — it covers counts too).
    pub const fn new(n: u32) -> Result<Self, InvalidRecur> {
        if n == 0 {
            Err(InvalidRecur::ZeroInterval)
        } else {
            Ok(Self(n))
        }
    }

    /// The remaining-target count (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for OccurrenceCount {
    type Error = InvalidRecur;

    fn try_from(n: u32) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// Which days of a monthly pattern the occurrences land on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MonthlyAnchor {
    /// A fixed day of month, clamped in short months.
    OnDay(DayOfMonth),
    /// The nth weekday of the month (no 5th → no occurrence that month).
    NthWeekday(NthWeek, Weekday),
}

/// The recurrence pattern proper: interval grammar only — anchor and
/// bound ride [`RecurRule`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurPattern {
    /// Every `every` days, anchor-aligned (occurrences at
    /// `anchor + k·every`).
    Daily {
        /// The day interval (≥ 1).
        every: EveryDays,
    },
    /// On the given weekdays of every `interval`-th week (Monday-start
    /// weeks, anchor week aligned).
    Weekly {
        /// The week interval (≥ 1).
        interval: EveryWeeks,
        /// The non-empty weekday set.
        on: WeekdaySet,
    },
    /// Monthly, on the given anchor.
    Monthly {
        /// The monthly anchor.
        on: MonthlyAnchor,
    },
}

/// Where a series stops. `Until` is arithmetic (the pure schedule
/// filters it); `Count` is history (evaluated at the engine, which the
/// pure functions cannot see — no counter column exists).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecurBound {
    /// No bound.
    None,
    /// No member after this date (inclusive).
    Until(NaiveDate),
    /// At most this many completed occurrences, engine-evaluated.
    Count(OccurrenceCount),
}

/// A recurring-series rule — the open head task's template
/// (origin §3.1). `anchor` is the first scheduled local date, fixed at
/// creation; roll and spawn never touch it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecurRule {
    /// Stable series identity across the whole occurrence chain
    /// (`UUIDv7`, minted client-side).
    pub series: SeriesId,
    /// First scheduled local date; the phase origin of every pattern.
    pub anchor: NaiveDate,
    /// The interval grammar.
    pub pattern: RecurPattern,
    /// Where the series stops.
    pub bound: RecurBound,
    /// D-C12's conversion seam: the habit this series was created from
    /// (or `None`). Entity lands in a later chunk; the seam is typed
    /// now so misfiling stays recoverable without data fabrication.
    pub continued_from: Option<HabitId>,
}

/// The Monday-start week index of a date: CE day 1 is a Monday
/// (`ce % 7 == 1`, Sundays `== 0`), so the grouping divisor needs the
/// `-1` shift — a bare `div_euclid(7)` would group Sun..Fri. Week-index
/// arithmetic — not day-mod — is what makes a weekly set with
/// `on ≠ {weekday(anchor)}` correct.
fn week_index(d: NaiveDate) -> i64 {
    (i64::from(d.num_days_from_ce()) - 1).div_euclid(7)
}

/// The Monday that starts `d`'s week, by back-stepping the weekday
/// offset (not a CE-day formula — the CE remainder convention is easy
/// to get wrong and this cannot drift).
fn week_monday(d: NaiveDate) -> NaiveDate {
    d - chrono::TimeDelta::try_days(i64::from(d.weekday().num_days_from_monday()))
        .expect("day delta")
}

/// The last day number (`1..=31`) of `(year, month)`.
fn last_day_of(year: i32, month: u32) -> u32 {
    let first_of_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    };
    first_of_next
        .expect("a representable month's successor is representable")
        .pred_opt()
        .expect("a first-of-month has a predecessor")
        .day()
}

/// `(year, month)` stepped by one month (never past December without
/// carrying the year).
fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

/// The clamped monthly anchor day of `(year, month)`: `min(n, last day)`
/// — the clamp *is* the membership predicate (Feb 31 → Feb 28/29).
fn clamped_day(day: DayOfMonth, year: i32, month: u32) -> NaiveDate {
    let d = u32::from(day.get()).min(last_day_of(year, month));
    NaiveDate::from_ymd_opt(year, month, d).expect("clamped day is valid")
}

/// The k-th `weekday` of `(year, month)`; `None` when the month lacks
/// one (no 5th Friday) — clamping is an `OnDay`-only ruling.
fn nth_weekday(k: NthWeek, weekday: Weekday, year: i32, month: u32) -> Option<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let offset = (weekday.num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
    let day = 1 + offset + 7 * (u32::from(k.get()) - 1);
    if day <= last_day_of(year, month) {
        NaiveDate::from_ymd_opt(year, month, day)
    } else {
        None
    }
}

/// The pattern's alignment predicate (the phase rule against the
/// anchor), without the `d ≥ anchor` gate — [`occurs_on`] applies it.
fn aligned(pattern: &RecurPattern, anchor: NaiveDate, d: NaiveDate) -> bool {
    match pattern {
        RecurPattern::Daily { every } => {
            let step = i64::from(every.get());
            (i64::from(d.num_days_from_ce()) - i64::from(anchor.num_days_from_ce()))
                .rem_euclid(step)
                == 0
        }
        RecurPattern::Weekly { interval, on } => {
            on.contains(d.weekday())
                && (week_index(d) - week_index(anchor)).rem_euclid(i64::from(interval.get())) == 0
        }
        RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(day),
        } => d == clamped_day(*day, d.year(), d.month()),
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(k, weekday),
        } => d.weekday() == *weekday && nth_weekday(*k, *weekday, d.year(), d.month()) == Some(d),
    }
}

/// The schedule-membership law with the anchor gate: `d` is a member iff
/// `d ≥ anchor` (the anchor is the first scheduled local date) and the
/// pattern aligns `d` against `anchor`. Total, pure, deterministic.
#[must_use]
pub fn occurs_on(pattern: &RecurPattern, anchor: NaiveDate, d: NaiveDate) -> bool {
    d >= anchor && aligned(pattern, anchor, d)
}

/// The least member strictly after `from`. Precondition `anchor ≤ from`
/// for meaningful use (production always satisfies it: the head sits
/// at/after the anchor); total regardless — `from < anchor` returns the
/// least member ≥ anchor, which is > `from`. Members are infinite for
/// every pattern, so the construction terminates.
/// # Panics
///
/// Only if a generated member falls outside `chrono::NaiveDate`'s
/// representable range — a programming error (the strategies bound the
/// inputs far inside it), never an input condition.
#[must_use]
pub fn next_after(pattern: &RecurPattern, anchor: NaiveDate, from: NaiveDate) -> NaiveDate {
    let start = from.succ_opt().filter(|d| *d >= anchor).unwrap_or(anchor);
    match pattern {
        RecurPattern::Daily { every } => {
            let step = i64::from(every.get());
            let diff = i64::from(start.num_days_from_ce() - anchor.num_days_from_ce());
            // Ceiling: the least aligned member ≥ `start` — floor would
            // answer the anchor itself for a mid-interval `from` (≤ from,
            // violating strictness) and loop any forward enumeration.
            let k = (diff + step - 1) / step;
            anchor
                .checked_add_signed(chrono::TimeDelta::try_days(k * step).expect("day delta"))
                .expect("an in-range anchor steps to an in-range member")
        }
        RecurPattern::Weekly { interval, on } => {
            let step = i64::from(interval.get());
            let base = week_index(anchor);
            let wi_start = week_index(start);
            let mut wi = wi_start;
            loop {
                if (wi - base).rem_euclid(step) == 0 {
                    let monday = week_monday(start)
                        + chrono::TimeDelta::try_days((wi - wi_start) * 7).expect("day delta");
                    for index in on.iter() {
                        let candidate = monday
                            + chrono::TimeDelta::try_days(i64::from(index.num_days_from_monday()))
                                .expect("day delta");
                        if candidate >= start {
                            return candidate;
                        }
                    }
                }
                wi += 1;
            }
        }
        RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(day),
        } => {
            let (mut y, mut m) = (start.year(), start.month());
            loop {
                let candidate = clamped_day(*day, y, m);
                if candidate >= start {
                    return candidate;
                }
                (y, m) = next_month(y, m);
            }
        }
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(k, weekday),
        } => {
            let (mut y, mut m) = (start.year(), start.month());
            loop {
                if let Some(candidate) = nth_weekday(*k, *weekday, y, m)
                    && candidate >= start
                {
                    return candidate;
                }
                (y, m) = next_month(y, m);
            }
        }
    }
}

/// The latest member on or before `today` (and ≥ `anchor`); `None` when
/// no member satisfies both. The backward half of the arithmetic —
/// [`roll_target`]'s catch-up case. Structured per pattern (no day-by-
/// day scan), so an offline gap of years costs the same as a day.
fn prev_on_or_before(
    pattern: &RecurPattern,
    anchor: NaiveDate,
    today: NaiveDate,
) -> Option<NaiveDate> {
    if today < anchor {
        return None;
    }
    match pattern {
        RecurPattern::Daily { every } => {
            let step = i64::from(every.get());
            let diff = i64::from(today.num_days_from_ce() - anchor.num_days_from_ce());
            Some(
                anchor
                    .checked_add_signed(
                        chrono::TimeDelta::try_days(diff.div_euclid(step) * step)
                            .expect("day delta"),
                    )
                    .expect("an in-range anchor steps to an in-range member"),
            )
        }
        RecurPattern::Weekly { interval, on } => {
            let step = i64::from(interval.get());
            let base = week_index(anchor);
            if week_index(today) < base {
                return None;
            }
            let wi_today = week_index(today);
            let mut wi = base + (wi_today - base).div_euclid(step) * step;
            loop {
                let monday = week_monday(today)
                    + chrono::TimeDelta::try_days((wi - wi_today) * 7).expect("day delta");
                let best = on
                    .iter()
                    .map(|index| {
                        monday
                            + chrono::TimeDelta::try_days(i64::from(index.num_days_from_monday()))
                                .expect("day delta")
                    })
                    .filter(|d| *d <= today && *d >= anchor)
                    .max();
                if let Some(found) = best {
                    return Some(found);
                }
                if wi == base {
                    return None;
                }
                wi -= step;
            }
        }
        RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(day),
        } => {
            let (ay, am) = (anchor.year(), anchor.month());
            let (mut y, mut m) = (today.year(), today.month());
            loop {
                if (y, m) < (ay, am) {
                    return None;
                }
                let candidate = clamped_day(*day, y, m);
                if candidate <= today && candidate >= anchor {
                    return Some(candidate);
                }
                if (y, m) == (ay, am) {
                    return None;
                }
                m = if m == 1 { 12 } else { m - 1 };
                if m == 12 {
                    y -= 1;
                }
            }
        }
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(k, weekday),
        } => {
            let (ay, am) = (anchor.year(), anchor.month());
            let (mut y, mut m) = (today.year(), today.month());
            loop {
                if (y, m) < (ay, am) {
                    return None;
                }
                if let Some(candidate) = nth_weekday(*k, *weekday, y, m)
                    && candidate <= today
                    && candidate >= anchor
                {
                    return Some(candidate);
                }
                if (y, m) == (ay, am) {
                    return None;
                }
                m = if m == 1 { 12 } else { m - 1 };
                if m == 12 {
                    y -= 1;
                }
            }
        }
    }
}

/// The roll-forward law (origin §3.4, D-C7): the target a stale head's
/// scheduled date moves to. Two cases —
///
/// 1. **Catch-up**: some member lies in `(stale, today]` → the *latest*
///    such member (`Daily { every: 1 }` ⇒ always `today`).
/// 2. **Skip-ahead**: no member in that range (e.g. weekly, today is
///    Wednesday) → the first member `> stale` (next Monday).
///
/// Pure in `(pattern, anchor, stale, today)`: identical inputs give
/// identical targets on every device — the no-dedup-for-rolls property.
/// Precondition `stale < today`; for `stale ≥ today` it degrades
/// total-and-harmless to the skip-ahead case. Bound checks (`Until`
/// suppression) are the caller's — this function is bound-free
/// arithmetic.
#[must_use]
pub fn roll_target(
    pattern: &RecurPattern,
    anchor: NaiveDate,
    stale: NaiveDate,
    today: NaiveDate,
) -> NaiveDate {
    let first = next_after(pattern, anchor, stale);
    match prev_on_or_before(pattern, anchor, today) {
        Some(latest) if latest > stale => latest,
        _ => first,
    }
}

/// The series' schedule: its sorted members in `[from, to]` (empty when
/// `from > to`). **Bound handling** (D6): `Until(u)` filters (`d ≤ u`
/// only) — the schedule is the arithmetic truth of the series;
/// `Count(n)` does *not* filter — the count is evaluated against
/// completion history at the engine (no counter column exists, so the
/// pure function cannot see it).
#[must_use]
pub fn schedule(rule: &RecurRule, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    let mut members = Vec::new();
    if from > to {
        return members;
    }
    let mut cursor = from.pred_opt();
    while let Some(prev) = cursor {
        let member = next_after(&rule.pattern, rule.anchor, prev);
        if member > to {
            break;
        }
        if let RecurBound::Until(until) = rule.bound
            && member > until
        {
            break;
        }
        members.push(member);
        cursor = Some(member);
    }
    members
}

/// How an occurrence day of a series concluded, as of a reading time
/// (origin §3.5; the "missed" view is derived, never stored).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OccurrenceOutcome {
    /// Some completion attributes to the day; `at` is the earliest such
    /// stamp.
    Done {
        /// The earliest attributing completion.
        at: chrono::DateTime<chrono::Utc>,
    },
    /// The attribution window closed with nothing attributed.
    Missed,
    /// The attribution window is still open (unopened or in progress).
    NotYetReachable,
}

/// Attributes completions to the occurrence `day` of `rule` and names
/// its outcome as of `now` (D8's window attribution, no knob).
///
/// Window start `ws(d)` = [`crate::days::window_start`] of the local
/// date. A completion stamp `t` attributes to the **latest member `m`
/// with `ws(m) ≤ t`** — so a late completion counts for the *earlier*
/// occurrence (completing Tuesday's rolled head on Wednesday counts
/// for Tuesday). The outcome is:
///
/// - [`OccurrenceOutcome::Done`] iff some completion lies in
///   `[ws(day), ws(next member))` (`at` = the earliest such stamp);
/// - else [`OccurrenceOutcome::Missed`] iff `now ≥ ws(next member)`;
/// - else [`OccurrenceOutcome::NotYetReachable`].
///
/// For the series' **final** member (a `Until` bound closes the member
/// list after it) the window is open-ended, so the final member is
/// never `Missed` — the open head simply sits on the board (D8: no
/// invented deadline, no knob). `Count` bounds are history-evaluated at
/// the engine and invisible here (D6).
#[must_use]
pub fn occurrence_outcome(
    rule: &RecurRule,
    day: NaiveDate,
    now: chrono::DateTime<chrono::Utc>,
    day_start_time: chrono::NaiveTime,
    offset: chrono::FixedOffset,
    completions: &[chrono::DateTime<chrono::Utc>],
) -> OccurrenceOutcome {
    let ws_day = crate::days::window_start(day, day_start_time, offset);
    let next = next_after(&rule.pattern, rule.anchor, day);
    let window_end = match rule.bound {
        RecurBound::Until(until) if next > until => None,
        _ => Some(crate::days::window_start(next, day_start_time, offset)),
    };
    if let Some(at) = completions
        .iter()
        .filter(|t| **t >= ws_day && window_end.is_none_or(|end| **t < end))
        .min()
    {
        return OccurrenceOutcome::Done { at: *at };
    }
    match window_end {
        Some(end) if now >= end => OccurrenceOutcome::Missed,
        _ => OccurrenceOutcome::NotYetReachable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use InvalidRecur as E;
    use chrono::Weekday::*;

    fn every_days(n: u16) -> EveryDays {
        EveryDays::new(n).unwrap()
    }

    fn set(days: &[Weekday]) -> WeekdaySet {
        WeekdaySet::new(days.iter().copied()).unwrap()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn rule(pattern: RecurPattern) -> RecurRule {
        RecurRule {
            series: SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor: date(2026, 1, 1),
            pattern,
            bound: RecurBound::None,
            continued_from: None,
        }
    }

    #[test]
    fn interval_mints_reject_zero_and_accept_the_boundaries() {
        assert!(matches!(EveryDays::new(0), Err(E::ZeroInterval)));
        assert!(matches!(EveryWeeks::new(0), Err(E::ZeroInterval)));
        assert!(matches!(OccurrenceCount::new(0), Err(E::ZeroInterval)));
        for n in [1, 2, u16::MAX - 1] {
            assert_eq!(EveryDays::new(n).unwrap().get(), n);
            assert_eq!(EveryWeeks::new(n).unwrap().get(), n);
        }
        assert_eq!(EveryDays::new(u16::MAX).unwrap().get(), u16::MAX);
        assert_eq!(EveryWeeks::new(u16::MAX).unwrap().get(), u16::MAX);
        assert_eq!(OccurrenceCount::new(1).unwrap().get(), 1);
        assert_eq!(OccurrenceCount::new(u32::MAX).unwrap().get(), u32::MAX);
    }

    #[test]
    fn day_of_month_mints_reject_the_range_edges() {
        for bad in [0, 32, 100, u32::MAX] {
            let err = DayOfMonth::new(bad).expect_err("reject");
            assert!(matches!(err, E::DayOutOfRange { got } if got == bad));
        }
        for good in [1, 15, 28, 29, 30, 31] {
            assert_eq!(u32::from(DayOfMonth::new(good).unwrap().get()), good);
        }
    }

    #[test]
    fn nth_week_mints_reject_the_range_edges() {
        for bad in [0, 6, 100, u32::MAX] {
            let err = NthWeek::new(bad).expect_err("reject");
            assert!(matches!(err, E::WeekOutOfRange { got } if got == bad));
        }
        for good in 1..=5 {
            assert_eq!(u32::from(NthWeek::new(good).unwrap().get()), good);
        }
    }

    #[test]
    fn weekday_set_mint_rejects_empty_and_dedups_silently() {
        assert!(matches!(
            WeekdaySet::new(Vec::new()),
            Err(E::EmptyWeekdaySet)
        ));
        let deduped = set(&[Mon, Fri, Mon, Fri]);
        assert_eq!(deduped.len(), 2);
        assert!(deduped.contains(Mon) && deduped.contains(Fri));
        assert!(!deduped.contains(Tue));
        // Canonical order: Mon..Sun regardless of mint order.
        let ordered: Vec<Weekday> = set(&[Sun, Wed, Mon]).iter().collect();
        assert_eq!(ordered, vec![Mon, Wed, Sun]);
        // `is_empty` is the documented constant.
        assert!(!deduped.is_empty());
    }

    #[test]
    fn mints_decode_strictly_the_zero_interval_cannot_deserialize() {
        assert!(serde_json::from_value::<EveryDays>(serde_json::json!(0)).is_err());
        assert!(serde_json::from_value::<EveryWeeks>(serde_json::json!(0)).is_err());
        assert!(serde_json::from_value::<OccurrenceCount>(serde_json::json!(0)).is_err());
        // The valid shape is a bare number.
        let ok: EveryDays = serde_json::from_value(serde_json::json!(3)).unwrap();
        assert_eq!(ok, every_days(3));
        // An empty weekday sequence cannot deserialize.
        assert!(serde_json::from_value::<WeekdaySet>(serde_json::json!([])).is_err());
        let weekdays: WeekdaySet =
            serde_json::from_value(serde_json::json!(["Mon", "Fri"])).unwrap();
        assert_eq!(weekdays, set(&[Mon, Fri]));
        // Range edges on the wire.
        assert!(serde_json::from_value::<DayOfMonth>(serde_json::json!(0)).is_err());
        assert!(serde_json::from_value::<DayOfMonth>(serde_json::json!(32)).is_err());
        assert!(serde_json::from_value::<NthWeek>(serde_json::json!(6)).is_err());
        let day: DayOfMonth = serde_json::from_value(serde_json::json!(31)).unwrap();
        assert_eq!(day.get(), 31);
    }

    #[test]
    fn the_grammar_round_trips_through_its_default_serde_shape() {
        let daily = rule(RecurPattern::Daily {
            every: every_days(3),
        });
        let weekly = RecurRule {
            pattern: RecurPattern::Weekly {
                interval: crate::recur::EveryWeeks::new(2).unwrap(),
                on: set(&[Mon, Fri]),
            },
            ..rule(RecurPattern::Daily {
                every: every_days(1),
            })
        };
        let monthly = RecurRule {
            pattern: RecurPattern::Monthly {
                on: MonthlyAnchor::NthWeekday(NthWeek::new(3).unwrap(), Fri),
            },
            bound: RecurBound::Until(date(2027, 6, 30)),
            ..weekly.clone()
        };
        for sample in [daily, weekly, monthly] {
            let json = serde_json::to_value(&sample).unwrap();
            let back: RecurRule = serde_json::from_value(json).unwrap();
            assert_eq!(back, sample);
        }
        // The wire shape of a cleared continued_from is `null`.
        let json = serde_json::to_value(rule(RecurPattern::Daily {
            every: every_days(1),
        }))
        .unwrap();
        assert_eq!(json["continued_from"], serde_json::Value::Null);
        assert_eq!(json["anchor"], serde_json::json!("2026-01-01"));
    }
}

#[cfg(test)]
mod arithmetic_tests {
    use super::*;
    use crate::test_support::{
        naive_date_strategy, proptest_config, recur_pattern_strategy, recur_rule_strategy,
        weekday_strategy,
    };
    use chrono::{TimeZone as _, Weekday};
    use proptest::prelude::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn every_days(n: u16) -> EveryDays {
        EveryDays::new(n).unwrap()
    }

    fn set(days: &[Weekday]) -> WeekdaySet {
        WeekdaySet::new(days.iter().copied()).unwrap()
    }

    #[test]
    fn monthly_clamping_table() {
        let anchor = date(2026, 1, 1);
        let on31 = RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(DayOfMonth::new(31).unwrap()),
        };
        // Feb 2026 has 28 days: the clamp is the membership predicate.
        assert!(occurs_on(&on31, anchor, date(2026, 2, 28)));
        assert!(!occurs_on(&on31, anchor, date(2026, 2, 27)));
        // Mar/Apr: 31 and 30.
        assert!(occurs_on(&on31, anchor, date(2026, 3, 31)));
        assert!(!occurs_on(&on31, anchor, date(2026, 3, 30)));
        assert!(occurs_on(&on31, anchor, date(2026, 4, 30)));
        // Leap February 2024: 29 days, both 29 and 31 clamp onto it.
        let leap_anchor = date(2024, 1, 1);
        let on29 = RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(DayOfMonth::new(29).unwrap()),
        };
        assert!(occurs_on(&on29, leap_anchor, date(2024, 2, 29)));
        assert!(occurs_on(&on31, leap_anchor, date(2024, 2, 29)));
        assert!(!occurs_on(&on31, leap_anchor, date(2024, 2, 28)));
    }

    #[test]
    fn roll_target_origin_examples() {
        // Weekly, today is Wednesday → next Monday (skip-ahead case).
        let weekly = RecurPattern::Weekly {
            interval: EveryWeeks::new(1).unwrap(),
            on: set(&[Weekday::Mon]),
        };
        let anchor = date(2026, 10, 5); // a Monday
        assert_eq!(
            roll_target(&weekly, anchor, date(2026, 10, 19), date(2026, 10, 21)),
            date(2026, 10, 26)
        );
        // Daily { every: 1 } → always today (catch-up case).
        let daily = RecurPattern::Daily {
            every: every_days(1),
        };
        assert_eq!(
            roll_target(&daily, anchor, date(2026, 10, 19), date(2026, 10, 21)),
            date(2026, 10, 21)
        );
        // Catch-up picks the *latest* member in the window.
        assert_eq!(
            roll_target(&daily, anchor, date(2026, 10, 19), date(2026, 10, 25)),
            date(2026, 10, 25)
        );
    }

    #[test]
    fn nth_weekday_short_month_contributes_no_member() {
        // February 2026 (28 days, starts on a Sunday) has no 5th Friday.
        let fifth_friday = RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(NthWeek::new(5).unwrap(), Weekday::Fri),
        };
        let anchor = date(2026, 1, 1);
        for day in [1, 6, 13, 20, 27, 28] {
            assert!(!occurs_on(&fifth_friday, anchor, date(2026, 2, day)));
        }
        // The scan skips Feb (4 Fridays), Mar (4), Apr (4) and lands on
        // May's 5th Friday (2026-05-29).
        assert_eq!(
            next_after(&fifth_friday, anchor, date(2026, 2, 1)),
            date(2026, 5, 29)
        );
        // A month that *does* have a 5th Friday contains its member.
        assert!(occurs_on(&fifth_friday, anchor, date(2026, 5, 29)));
        assert!(!occurs_on(&fifth_friday, anchor, date(2026, 5, 22)));
    }

    #[test]
    fn weekly_alignment_is_week_indexed_not_day_modded() {
        // on = {Mon, Wed}, anchor Monday: the Wednesday members are the
        // anchor weeks' Wednesdays — a day-distance mod against a Monday
        // anchor would misalign them.
        let weekly = RecurPattern::Weekly {
            interval: EveryWeeks::new(2).unwrap(),
            on: set(&[Weekday::Mon, Weekday::Wed]),
        };
        let anchor = date(2026, 10, 5); // Monday, week 0
        assert!(occurs_on(&weekly, anchor, date(2026, 10, 7))); // Wed week 0
        assert!(occurs_on(&weekly, anchor, date(2026, 10, 19))); // Mon week 2
        assert!(occurs_on(&weekly, anchor, date(2026, 10, 21))); // Wed week 2
        assert!(!occurs_on(&weekly, anchor, date(2026, 10, 12))); // Mon week 1
        assert!(!occurs_on(&weekly, anchor, date(2026, 10, 14))); // Wed week 1
        // The gate: pre-anchor aligned dates are not members.
        assert!(!occurs_on(&weekly, anchor, date(2026, 9, 30)));
        assert!(occurs_on(&weekly, anchor, anchor));
    }

    #[test]
    fn daily_phase_is_anchor_aligned_not_spawn_drifted() {
        let daily = RecurPattern::Daily {
            every: every_days(3),
        };
        let anchor = date(2026, 10, 1);
        assert!(occurs_on(&daily, anchor, date(2026, 10, 4)));
        assert!(occurs_on(&daily, anchor, date(2026, 10, 7)));
        assert!(!occurs_on(&daily, anchor, date(2026, 10, 5)));
    }

    #[test]
    fn schedule_filters_until_but_not_count() {
        let mut rule = RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor: date(2026, 10, 1),
            pattern: RecurPattern::Daily {
                every: every_days(2),
            },
            bound: RecurBound::Until(date(2026, 10, 9)),
            continued_from: None,
        };
        let got = schedule(&rule, date(2026, 10, 1), date(2026, 10, 31));
        assert_eq!(
            got,
            vec![
                date(2026, 10, 1),
                date(2026, 10, 3),
                date(2026, 10, 5),
                date(2026, 10, 7),
                date(2026, 10, 9),
            ]
        );
        // Count is history-evaluated at the engine: the arithmetic truth
        // is unchanged (D6).
        rule.bound = RecurBound::Count(OccurrenceCount::new(1).unwrap());
        assert_eq!(
            schedule(&rule, date(2026, 10, 1), date(2026, 10, 31)),
            vec![
                date(2026, 10, 1),
                date(2026, 10, 3),
                date(2026, 10, 5),
                date(2026, 10, 7),
                date(2026, 10, 9),
                date(2026, 10, 11),
                date(2026, 10, 13),
                date(2026, 10, 15),
                date(2026, 10, 17),
                date(2026, 10, 19),
                date(2026, 10, 21),
                date(2026, 10, 23),
                date(2026, 10, 25),
                date(2026, 10, 27),
                date(2026, 10, 29),
                date(2026, 10, 31),
            ]
        );
        // An empty range is empty.
        assert_eq!(
            schedule(&rule, date(2026, 11, 1), date(2026, 10, 1)),
            [] as [NaiveDate; 0]
        );
    }

    #[test]
    fn daily_next_after_is_strictly_after_even_mid_interval() {
        // The floor-division bug this pins: a `from` between members used
        // to answer the anchor itself (≤ from), looping every forward
        // enumeration (the schedule) forever.
        let daily = RecurPattern::Daily {
            every: every_days(3),
        };
        let anchor = date(2026, 10, 1);
        assert_eq!(
            next_after(&daily, anchor, date(2026, 10, 2)),
            date(2026, 10, 4)
        );
        assert_eq!(
            next_after(&daily, anchor, date(2026, 10, 4)),
            date(2026, 10, 7)
        );
        assert_eq!(
            next_after(&daily, anchor, date(2026, 10, 6)),
            date(2026, 10, 7)
        );
        // The schedule stays finite over the mid-interval start.
        let rule = RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor,
            pattern: daily,
            bound: RecurBound::None,
            continued_from: None,
        };
        assert_eq!(
            schedule(&rule, date(2026, 10, 2), date(2026, 10, 10)),
            vec![date(2026, 10, 4), date(2026, 10, 7), date(2026, 10, 10),]
        );
    }

    // ---- attribution (§4.4): unit table ----

    use crate::days::window_start;

    fn rule_until(pattern: RecurPattern, until: NaiveDate) -> RecurRule {
        RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor: date(2026, 10, 1),
            pattern,
            bound: RecurBound::Until(until),
            continued_from: None,
        }
    }

    fn utc(secs: i64) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn offset_hours(h: i32) -> chrono::FixedOffset {
        chrono::FixedOffset::east_opt(h * 3600).unwrap()
    }

    #[test]
    fn a_completion_inside_the_window_is_done_with_the_earliest_stamp() {
        let rule = rule_until(
            RecurPattern::Daily {
                every: every_days(1),
            },
            date(2026, 12, 31),
        );
        let start = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let offset = offset_hours(0);
        let day = date(2026, 10, 5);
        let ws = window_start(day, start, offset).timestamp();
        let next = window_start(date(2026, 10, 6), start, offset).timestamp();
        let outcome = occurrence_outcome(
            &rule,
            day,
            utc(next + 10),
            start,
            offset,
            &[utc(ws + 100), utc(ws + 50), utc(ws + 70)],
        );
        assert_eq!(outcome, OccurrenceOutcome::Done { at: utc(ws + 50) });
        // A stamp exactly at the window start attributes; exactly at the
        // next window start does not (half-open windows).
        let at_start = occurrence_outcome(&rule, day, utc(next), start, offset, &[utc(ws)]);
        assert!(matches!(at_start, OccurrenceOutcome::Done { .. }));
        let at_end = occurrence_outcome(&rule, day, utc(next), start, offset, &[utc(next)]);
        assert_eq!(at_end, OccurrenceOutcome::Missed);
    }

    #[test]
    fn a_mid_gap_completion_attributes_to_the_earlier_member() {
        // Weekly on Fridays: a completion the following Wednesday sits
        // in *Friday's* attribution window (the window runs to the next
        // member's start), so the late completion counts for Friday.
        let rule = rule_until(
            RecurPattern::Weekly {
                interval: EveryWeeks::new(1).unwrap(),
                on: set(&[Weekday::Fri]),
            },
            date(2026, 12, 31),
        );
        let start = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let offset = offset_hours(0);
        let friday = date(2026, 10, 2);
        let next_friday = date(2026, 10, 9);
        let stamp = window_start(date(2026, 10, 7), start, offset); // Wednesday
        let friday_outcome = occurrence_outcome(&rule, friday, stamp, start, offset, &[stamp]);
        assert!(matches!(friday_outcome, OccurrenceOutcome::Done { .. }));
        let next_outcome = occurrence_outcome(&rule, next_friday, stamp, start, offset, &[stamp]);
        assert_eq!(next_outcome, OccurrenceOutcome::NotYetReachable);
    }

    #[test]
    fn night_owl_start_reattributes_early_morning_stamps() {
        // With a 05:00 day start, a 03:00-local completion belongs to
        // *yesterday's* window.
        let rule = rule_until(
            RecurPattern::Daily {
                every: every_days(1),
            },
            date(2026, 12, 31),
        );
        let start = chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap();
        let offset = offset_hours(2);
        // 2026-10-06 03:00 local (+02:00) = 01:00 UTC on the 6th; the
        // window that contains it opened 2026-10-05 05:00 local.
        let yesterday = date(2026, 10, 5);
        let today = date(2026, 10, 6);
        // 2026-10-06 03:00 local (+02:00): two hours before the 05:00
        // window start, so it belongs to *yesterday's* window.
        let stamp = window_start(today, start, offset) - chrono::TimeDelta::try_hours(2).unwrap();
        let y = occurrence_outcome(&rule, yesterday, stamp, start, offset, &[stamp]);
        assert!(matches!(y, OccurrenceOutcome::Done { .. }));
        let t = occurrence_outcome(&rule, today, stamp, start, offset, &[stamp]);
        assert_eq!(t, OccurrenceOutcome::NotYetReachable);
    }

    #[test]
    fn the_final_member_is_never_missed() {
        let rule = rule_until(
            RecurPattern::Daily {
                every: every_days(1),
            },
            date(2026, 10, 3),
        );
        let start = chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let offset = offset_hours(0);
        // Long after everything, with no completion: the final head is
        // open-ended (D8), never Missed.
        let final_day = date(2026, 10, 3);
        let late = utc(1_800_000_000);
        assert_eq!(
            occurrence_outcome(&rule, final_day, late, start, offset, &[]),
            OccurrenceOutcome::NotYetReachable
        );
        // A non-final day *is* Missed once its window closes.
        assert_eq!(
            occurrence_outcome(&rule, date(2026, 10, 2), late, start, offset, &[]),
            OccurrenceOutcome::Missed
        );
    }

    // P5 — attribution: windows partition the timeline; every stamp
    // attributes to exactly one member; Done stamps sit inside their
    // member's window; the final member is never Missed.
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn stamps_attribute_to_exactly_one_member(
            rule in recur_rule_strategy(),
            day_start_seconds in 0_u32..=86_399,
            offset_seconds in -50_400_i32..=50_400,
            now_offset in 0_i64..=400_000,
            stamps in proptest::collection::vec(0_i64..=60, 0..8),
        ) {
            let start = chrono::NaiveTime::from_num_seconds_from_midnight_opt(day_start_seconds, 0)
                .expect("valid time of day");
            let offset = chrono::FixedOffset::east_opt(offset_seconds).expect("valid offset");
            let boundless = RecurRule { bound: RecurBound::None, ..rule.clone() };
            // 400 days: even a 5th-weekday monthly pattern has a member
            // within the horizon (some month always has five).
            let horizon = rule
                .anchor
                .checked_add_signed(chrono::TimeDelta::try_days(400).expect("day delta"))
                .expect("in range");
            let members = schedule(&boundless, rule.anchor, horizon);
            prop_assert!(!members.is_empty(), "the horizon always contains the anchor's window");
            let ws = |d: NaiveDate| crate::days::window_start(d, start, offset);
            let ws_anchor = ws(rule.anchor);
            let now = ws_anchor + chrono::TimeDelta::try_seconds(now_offset).expect("delta");
            let completions: Vec<chrono::DateTime<chrono::Utc>> = stamps
                .iter()
                .map(|s| ws_anchor + chrono::TimeDelta::try_seconds(*s * 3600).expect("delta"))
                .collect();
            // Consecutive windows are consecutive: strictly increasing
            // starts, each window ending exactly at the next start.
            for pair in members.windows(2) {
                let (a, b) = (ws(pair[0]), ws(pair[1]));
                prop_assert!(a < b, "window starts strictly increase");
                let outcome_a = occurrence_outcome(&rule, pair[0], now, start, offset, &completions);
                if let OccurrenceOutcome::Done { at } = outcome_a {
                    prop_assert!(at >= a && at < b, "Done stamps sit inside the window");
                }
            }
            // Every generated stamp attributes to exactly one member:
            // the member whose window contains it, cross-checked through
            // occurrence_outcome.
            for t in &completions {
                // Stamps before the first member attribute to nothing
                // (§4.4: stamps from before the series existed are not
                // produced; the partition runs over real members).
                if *t < ws(members[0]) {
                    continue;
                }
                let mut attributed = 0;
                for (index, m) in members.iter().enumerate() {
                    let is_last = index + 1 == members.len();
                    let end = if is_last {
                        ws(*m) + chrono::TimeDelta::try_days(400).expect("day delta")
                    } else {
                        ws(members[index + 1])
                    };
                    if ws(*m) <= *t && *t < end {
                        attributed += 1;
                        let outcome =
                            occurrence_outcome(&rule, *m, now, start, offset, &[*t]);
                        prop_assert!(
                            matches!(outcome, OccurrenceOutcome::Done { .. }),
                            "the containing member's outcome is Done for its stamp"
                        );
                    } else {
                        let outcome =
                            occurrence_outcome(&rule, *m, now, start, offset, &[*t]);
                        prop_assert!(
                            !matches!(outcome, OccurrenceOutcome::Done { .. }),
                            "a stamp outside the window never attributes"
                        );
                    }
                }
                prop_assert_eq!(attributed, 1, "windows partition the timeline");
            }
            // The final member's window never closes: never Missed (D8).
            let last = *members.last().expect("non-empty");
            if !completions.is_empty() {
                let outcome = occurrence_outcome(&rule, last, now, start, offset, &completions);
                prop_assert!(
                    !matches!(outcome, OccurrenceOutcome::Missed),
                    "the open final head is never Missed"
                );
            }
        }
    }

    // P1 — next_after is total, strictly after `from`, and a member.
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn next_after_is_strictly_after_and_a_member(
            anchor in naive_date_strategy(),
            from_offset in 0_i64..=40_000,
            pattern in recur_pattern_strategy(),
        ) {
            let from = anchor
                .checked_add_signed(chrono::TimeDelta::try_days(from_offset).expect("day delta"))
                .expect("in range");
            let next = next_after(&pattern, anchor, from);
            prop_assert!(next > from);
            prop_assert!(occurs_on(&pattern, anchor, next));
        }

        #[test]
        fn total_when_from_precedes_the_anchor(
            anchor in naive_date_strategy(),
            back in 1_i64..=4_000,
            pattern in recur_pattern_strategy(),
        ) {
            let from = anchor
                .checked_sub_signed(chrono::TimeDelta::try_days(back).expect("day delta"));
            if let Some(from) = from {
                let next = next_after(&pattern, anchor, from);
                prop_assert!(next >= anchor, "the least member ≥ anchor");
                prop_assert!(occurs_on(&pattern, anchor, next));
            }
        }

        // P2 — the chain and the schedule agree: seeding at the least
        // member ≥ `from` (found by an independent membership scan) and
        // stepping `next_after` enumerates exactly `schedule`, and
        // `next_after(m)` from any member yields the next member.
        #[test]
        fn chain_enumeration_equals_schedule(
            rule in recur_rule_strategy(),
            from in naive_date_strategy(),
            span in 1_i64..=400,
        ) {
            let to = from
                .checked_add_signed(chrono::TimeDelta::try_days(span).expect("day delta"))
                .expect("in range");
            let expected = schedule(&rule, from, to);
            // The membership predicate is pattern-level; the `Until`
            // bound is the schedule's filter, so the chain enumerates
            // against the effective horizon.
            let horizon = match rule.bound {
                RecurBound::Until(u) => to.min(u),
                _ => to,
            };
            // Independent seeding via the membership predicate.
            let mut cursor = from.max(rule.anchor);
            while !occurs_on(&rule.pattern, rule.anchor, cursor) && cursor <= horizon {
                cursor = cursor.succ_opt().expect("in range");
            }
            if cursor > horizon {
                prop_assert!(expected.is_empty());
                return Ok(());
            }
            let mut chained = vec![cursor];
            loop {
                let next = next_after(&rule.pattern, rule.anchor, *chained.last().expect("seeded"));
                if next > horizon {
                    break;
                }
                chained.push(next);
                if chained.len() > 1_000 {
                    break; // bounded by the span; guard against drift
                }
            }
            prop_assert_eq!(chained, expected);
        }

        // P3 — clamping: Monthly OnDay yields exactly one member per
        // whole month in range, equal to min(n, last day).
        #[test]
        fn monthly_on_day_is_exactly_one_member_per_month(
            n in 1_u32..=31,
            year in 2001_i32..=2099,
            month in 1_u32..=12,
            months in 1_u32..=6,
        ) {
            let anchor = date(year, month, 1);
            let pattern = RecurPattern::Monthly {
                on: MonthlyAnchor::OnDay(DayOfMonth::new(n).unwrap()),
            };
            let (mut y, mut m) = next_month(year, month);
            for _ in 0..months {
                let expected_day = n.min(last_day_of(y, m));
                let got = schedule(
                    &RecurRule {
                        series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
                        anchor,
                        pattern: pattern.clone(),
                        bound: RecurBound::None,
                        continued_from: None,
                    },
                    date(y, m, 1),
                    date(y, m, last_day_of(y, m)),
                );
                let expected = vec![date(y, m, expected_day)];
                prop_assert_eq!(got, expected);
                (y, m) = next_month(y, m);
            }
        }

        // P4 — the roll laws.
        #[test]
        fn roll_target_laws(
            anchor in naive_date_strategy(),
            stale_offset in 0_i64..=20_000,
            gap in 1_i64..=2_000,
            pattern in recur_pattern_strategy(),
        ) {
            let stale = anchor
                .checked_add_signed(chrono::TimeDelta::try_days(stale_offset).expect("day delta"))
                .expect("in range");
            let today = stale
                .checked_add_signed(chrono::TimeDelta::try_days(gap).expect("day delta"))
                .expect("in range");
            let target = roll_target(&pattern, anchor, stale, today);
            prop_assert!(target > stale, "the head always moves forward");
            if target <= today {
                prop_assert!(occurs_on(&pattern, anchor, target), "catch-up lands on a member");
                prop_assert!(
                    next_after(&pattern, anchor, target) > today,
                    "it is the latest member ≤ today"
                );
            } else {
                prop_assert_eq!(target, next_after(&pattern, anchor, stale), "skip-ahead");
                prop_assert!(
                    next_after(&pattern, anchor, stale) > today,
                    "no member existed in (stale, today]"
                );
            }
            // Daily { every: 1 } always rolls onto today itself.
            if let RecurPattern::Daily { every } = &pattern
                && every.get() == 1
            {
                prop_assert_eq!(target, today);
            }
            // Determinism: identical inputs, identical target.
            prop_assert_eq!(target, roll_target(&pattern, anchor, stale, today));
        }

        // P4 (convergence, recorded deviation): catch-up is *gap
        // agnostic* — within one member-gap, the target is invariant to
        // how stale the head is. (The plan's day-by-day simulation
        // wording is a different, stateful process: intermediate ticks
        // legitimately apply skip-ahead, so it does not equal the
        // single-roll target — pinned here as the invariance law the
        // origin's "offline gaps change nothing but today" names.)
        #[test]
        fn roll_catch_up_is_gap_agnostic(
            anchor in naive_date_strategy(),
            stale_offset in 0_i64..=20_000,
            gap in 1_i64..=2_000,
            skip in 0_i64..=6,
            pattern in recur_pattern_strategy(),
        ) {
            let stale = anchor
                .checked_add_signed(chrono::TimeDelta::try_days(stale_offset).expect("day delta"))
                .expect("in range");
            let today = stale
                .checked_add_signed(chrono::TimeDelta::try_days(gap).expect("day delta"))
                .expect("in range");
            let first = next_after(&pattern, anchor, stale);
            if first <= today {
                // Catch-up window: shift `stale` forward but keep it
                // strictly inside the same member-gap — the window
                // (stale2, today] then contains the same members, so the
                // target cannot move.
                let stale2 = (stale + chrono::TimeDelta::try_days(skip).expect("day delta"))
                    .min(first.pred_opt().expect("a successor has a predecessor"));
                if stale2 < today {
                    prop_assert_eq!(
                        roll_target(&pattern, anchor, stale, today),
                        roll_target(&pattern, anchor, stale2, today),
                        "the target depends only on (pattern, anchor, today), not the gap"
                    );
                }
            }
        }

        // Weekly patterns generate members on every listed weekday of
        // every interval-th week; a cross-check over one aligned week's
        // neighbors keeps the week-index arithmetic honest.
        #[test]
        fn weekly_members_respect_the_week_interval(
            anchor in naive_date_strategy(),
            interval in 1_u16..=8_u16,
            weekday in weekday_strategy(),
            weeks in 0_i64..=200,
        ) {
            let pattern = RecurPattern::Weekly {
                interval: EveryWeeks::new(interval).unwrap(),
                on: set(&[weekday]),
            };
            let monday = week_monday(anchor)
                + chrono::TimeDelta::try_days(
                    i64::from(interval) * weeks * 7,
                )
                .expect("day delta");
            let candidate = monday
                + chrono::TimeDelta::try_days(i64::from(weekday.num_days_from_monday()))
                    .expect("day delta");
            if candidate >= anchor {
                prop_assert!(occurs_on(&pattern, anchor, candidate));
                if interval > 1 {
                    let neighbor = candidate + chrono::TimeDelta::try_days(7).expect("day delta");
                    prop_assert!(
                        !occurs_on(&pattern, anchor, neighbor),
                        "adjacent weeks never align"
                    );
                }
            }
        }
    }
}
