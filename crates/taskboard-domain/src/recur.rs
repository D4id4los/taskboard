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

use chrono::{DateTime, Datelike as _, NaiveDate, Utc, Weekday};
use serde::{Deserialize, Serialize};

use crate::entities::Task;
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
    /// A weekday token the three-letter lowercase vocabulary does not
    /// name (`Mon`, `xyz`, an empty comma slot — the text-form grammar's
    /// only input class whose rejection is not a range violation).
    #[error("unknown weekday {got:?}")]
    BadWeekday {
        /// The rejected token verbatim.
        got: String,
    },
    /// A numeric token that is not a number (`abc` where `Nd`/digits
    /// belong). Range violations of *parsed* numbers keep their own
    /// variants; this carries the unparsable text verbatim.
    #[error("not a number {got:?}")]
    BadNumber {
        /// The rejected token verbatim.
        got: String,
    },
}

/// The lowercase three-letter weekday token of `day` (`mon`..`sun`).
fn weekday_token(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "mon",
        Weekday::Tue => "tue",
        Weekday::Wed => "wed",
        Weekday::Thu => "thu",
        Weekday::Fri => "fri",
        Weekday::Sat => "sat",
        Weekday::Sun => "sun",
    }
}

/// The weekday a lowercase three-letter token names; case-sensitive by
/// ruling (the importance-vocabulary posture — `Mon` is rejected, the
/// vocabulary is lowercase everywhere it renders).
fn weekday_from_token(token: &str) -> Option<Weekday> {
    match token {
        "mon" => Some(Weekday::Mon),
        "tue" => Some(Weekday::Tue),
        "wed" => Some(Weekday::Wed),
        "thu" => Some(Weekday::Thu),
        "fri" => Some(Weekday::Fri),
        "sat" => Some(Weekday::Sat),
        "sun" => Some(Weekday::Sun),
        _ => None,
    }
}

/// The nth-weekday position's text token (`1st`…`5th`).
fn nth_token(n: u8) -> String {
    let suffix = match n {
        1 => "st",
        2 => "nd",
        3 => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
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

    /// The canonical text form (`3d`), the `--daily` flag's vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        format!("{}d", self.0)
    }

    /// Parses the text form: digits with an optional `d` suffix
    /// (`3`, `3d`). Documented leniency: the suffix is the canonical
    /// render; bare digits are accepted so the flag can read `--daily 3`.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for non-numeric text,
    /// [`InvalidRecur::ZeroInterval`] for 0.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let digits = raw.strip_suffix('d').unwrap_or(raw);
        let n = digits.parse::<u16>().map_err(|_| InvalidRecur::BadNumber {
            got: raw.to_owned(),
        })?;
        Self::new(n)
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

    /// The canonical text form (`2w`), the `--every-weeks` flag's
    /// vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        format!("{}w", self.0)
    }

    /// Parses the text form: digits with an optional `w` suffix
    /// (`2`, `2w`); the same documented bare-digit leniency as
    /// [`EveryDays::parse_text_form`].
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for non-numeric text,
    /// [`InvalidRecur::ZeroInterval`] for 0.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let digits = raw.strip_suffix('w').unwrap_or(raw);
        let n = digits.parse::<u16>().map_err(|_| InvalidRecur::BadNumber {
            got: raw.to_owned(),
        })?;
        Self::new(n)
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

    /// The canonical text form: the members, comma-joined, in canonical
    /// (Mon..Sun) order (`mon,wed,fri`) — the `--weekly` flag's
    /// vocabulary.
    #[must_use]
    pub fn text_form(&self) -> String {
        self.iter().map(weekday_token).collect::<Vec<_>>().join(",")
    }

    /// Parses the comma-separated three-letter lowercase vocabulary
    /// (`mon,wed,fri`); duplicates dedup silently (set semantics) and
    /// the canonical order is the mint's, not the input's.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::EmptyWeekdaySet`] for empty input (after split —
    /// whitespace is not a member), [`InvalidRecur::BadWeekday`] for a
    /// token the vocabulary does not name, case-sensitively.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        if raw.trim().is_empty() {
            return Err(InvalidRecur::EmptyWeekdaySet);
        }
        let mut days = Vec::new();
        for token in raw.split(',') {
            let day = weekday_from_token(token.trim()).ok_or_else(|| InvalidRecur::BadWeekday {
                got: token.to_owned(),
            })?;
            days.push(day);
        }
        Self::new(days)
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

    /// The canonical text form (`15`), the `--monthly` day anchor's
    /// vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        self.0.to_string()
    }

    /// Parses the text form: bare digits.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for non-numeric text,
    /// [`InvalidRecur::DayOutOfRange`] outside `1..=31`.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let n = raw.parse::<u32>().map_err(|_| InvalidRecur::BadNumber {
            got: raw.to_owned(),
        })?;
        Self::new(n)
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

    /// The canonical text form (`2nd`), the `--monthly` nth anchor's
    /// position vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        nth_token(self.0)
    }

    /// Parses the text form: digits with an ordinal suffix (`2nd`; the
    /// suffix is part of the vocabulary — `2` alone is an `OnDay` day).
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for non-numeric or suffix-less text,
    /// [`InvalidRecur::WeekOutOfRange`] outside `1..=5`.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let bad = || InvalidRecur::BadNumber {
            got: raw.to_owned(),
        };
        let digits = raw
            .strip_suffix("st")
            .or_else(|| raw.strip_suffix("nd"))
            .or_else(|| raw.strip_suffix("rd"))
            .or_else(|| raw.strip_suffix("th"))
            .ok_or_else(bad)?;
        let n = digits.parse::<u32>().map_err(|_| bad())?;
        Self::new(n)
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

    /// The canonical text form (`10`), the `--count` flag's vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        self.0.to_string()
    }

    /// Parses the text form: bare digits.
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for non-numeric text,
    /// [`InvalidRecur::ZeroInterval`] for 0.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let n = raw.parse::<u32>().map_err(|_| InvalidRecur::BadNumber {
            got: raw.to_owned(),
        })?;
        Self::new(n)
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

impl MonthlyAnchor {
    /// The canonical text form: `15` ([`MonthlyAnchor::OnDay`]) or
    /// `2nd-mon` ([`MonthlyAnchor::NthWeekday`]) — the `--monthly`
    /// flag's vocabulary.
    #[must_use]
    pub fn text_form(self) -> String {
        match self {
            Self::OnDay(day) => day.text_form(),
            Self::NthWeekday(k, weekday) => format!("{}-{}", k.text_form(), weekday_token(weekday)),
        }
    }

    /// Parses the text form: bare digits → [`MonthlyAnchor::OnDay`],
    /// `<k><suffix>-<day>` → [`MonthlyAnchor::NthWeekday`]. A month
    /// lacking the k-th weekday contributes no member — legal grammar,
    /// legal rule (chunk 1's D5).
    ///
    /// # Errors
    ///
    /// [`InvalidRecur::BadNumber`] for an unparsable position,
    /// [`InvalidRecur::WeekOutOfRange`] for a position outside `1..=5`
    /// (`15th-mon` is a range rejection, not a fifth meaning),
    /// [`InvalidRecur::DayOutOfRange`] / [`InvalidRecur::BadNumber`] /
    /// [`InvalidRecur::BadWeekday`] per [`DayOfMonth::parse_text_form`]
    /// / the weekday token.
    pub fn parse_text_form(raw: &str) -> Result<Self, InvalidRecur> {
        let Some((position, day)) = raw.split_once('-') else {
            return DayOfMonth::parse_text_form(raw).map(Self::OnDay);
        };
        let k = NthWeek::parse_text_form(position)?;
        let weekday = weekday_from_token(day.trim()).ok_or_else(|| InvalidRecur::BadWeekday {
            got: day.to_owned(),
        })?;
        Ok(Self::NthWeekday(k, weekday))
    }
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

impl RecurPattern {
    /// The canonical render-only text form (`daily`, `every 3d`,
    /// `weekly on mon,fri`, `every 2w on mon`, `monthly on 15`,
    /// `monthly on 2nd-mon`) — the receipt/badge label glue's input.
    /// Deliberately no `parse_text_form`: the flag grammar is the only
    /// input syntax (D-S8), so no stringly grammar can grow here.
    #[must_use]
    pub fn text_form(&self) -> String {
        match self {
            Self::Daily { every } if every.get() == 1 => "daily".to_owned(),
            Self::Daily { every } => format!("every {}", every.text_form()),
            Self::Weekly { interval, on } => {
                let weeks = if interval.get() == 1 {
                    "weekly".to_owned()
                } else {
                    format!("every {}", interval.text_form())
                };
                format!("{weeks} on {}", on.text_form())
            }
            Self::Monthly { on } => format!("monthly on {}", on.text_form()),
        }
    }
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

impl RecurBound {
    /// The canonical render-only text form (empty when unbounded, else
    /// `until 2026-12-31` | `count 10`) — the receipt's bound field's
    /// vocabulary, and deliberately parse-free (D-S8, the flag grammar
    /// is the only input syntax).
    #[must_use]
    pub fn text_form(&self) -> String {
        match self {
            Self::None => String::new(),
            Self::Until(until) => format!("until {until}"),
            Self::Count(n) => format!("count {}", n.text_form()),
        }
    }
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

/// The head's reference date — the plain calendar day the series
/// "is on" (D-E5's chain): the calendar date of `scheduled`, else of
/// `duedate`, else the completion day (the window-date of `now`). Plain
/// dates make the spawn drift-free: a head scheduled 03:00 references
/// the date its instant literally sits on, and the successor's own
/// reference reproduces it exactly. Series vocabulary shared by the
/// engine (spawn/roll), the CLI verb (the `tasks repeat` anchor) and
/// the next-occurrence law ([`next_occurrence`]) — no raw-UTC date is
/// ever treated as local: every leg goes through
/// [`DayCalendar::calendar_date`]/[`DayCalendar::today`].
#[must_use]
pub fn series_reference_date(
    head: &Task,
    day: crate::days::DayCalendar,
    now: DateTime<Utc>,
) -> NaiveDate {
    if let Some(scheduled) = head.scheduled {
        return day.calendar_date(scheduled);
    }
    head.duedate
        .map_or_else(|| day.today(now), |d| day.calendar_date(d))
}

/// The roll's stale-reference: the same chain minus the dateless
/// fallback — a dateless head is never stale (nothing anchors it to a
/// day), so `None` means "skip".
#[must_use]
pub fn roll_reference_date(task: &Task, day: crate::days::DayCalendar) -> Option<NaiveDate> {
    task.scheduled
        .map(|s| day.calendar_date(s))
        .or_else(|| task.duedate.map(|d| day.calendar_date(d)))
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

/// What a head advertises as its next occurrence — the one law both
/// surfaces (the `tasks repeat` receipt and the recurring badge) read,
/// so they can never disagree (D-S5). Composed from the existing
/// arithmetic: no new scheduling law.
///
/// - **Stale** (open, dated, reference strictly before today's
///   window-date — [`roll_reference_date`] minus the dateless fallback,
///   the roll predicate's read side): advertises [`roll_target`].
/// - **Fresh** (dated, reference ≥ today): advertises
///   [`next_after`] from the reference date.
/// - **Dateless**: never stale; projects from the window-date of
///   `now`.
///
/// `date: None` names the `Until` bound excluding the next member —
/// this head is the series' last (the badge renders the pattern; the
/// engine's own spawn/roll enforces the bound). `Count` is never
/// evaluated here: it is engine-evaluated from completion history
/// (chunk 1's D6), and the badge shows schedule truth, not quota
/// state. The caller gates on liveness/completion itself (surfaces
/// check `is_open()` — the done head's badge is a display policy).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextOccurrence {
    /// The next advertised member date; `None` past the `Until` bound.
    pub date: Option<NaiveDate>,
    /// The overdue-recurring flag: the head's reference date is
    /// strictly before today's window-date.
    pub stale: bool,
}

/// Evaluates [`NextOccurrence`] for `head` under its rule.
///
/// # Panics
///
/// Only via the stale arm's `expect`: staleness is derived from the
/// same `roll_reference_date` read, so a stale verdict implies a
/// reference date — the `expect` guards the invariant, never input.
#[must_use]
pub fn next_occurrence(
    rule: &RecurRule,
    head: &Task,
    day: crate::days::DayCalendar,
    now: DateTime<Utc>,
) -> NextOccurrence {
    let today = day.today(now);
    let reference = roll_reference_date(head, day);
    let stale = reference.is_some_and(|r| r < today);
    let until_admits = |d: NaiveDate| match rule.bound {
        RecurBound::Until(until) => d <= until,
        _ => true,
    };
    let date = if stale {
        let r = reference.expect("stale implies a reference date");
        let target = roll_target(&rule.pattern, rule.anchor, r, today);
        until_admits(target).then_some(target)
    } else {
        let from = reference.unwrap_or(today);
        let next = next_after(&rule.pattern, rule.anchor, from);
        until_admits(next).then_some(next)
    };
    NextOccurrence { date, stale }
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

    // ---- RT — the serde round-trip properties (origin §3.9 item 8,
    // chunk-1 half) ----

    /// A depth-limited arbitrary JSON value.
    fn json_strategy() -> impl Strategy<Value = serde_json::Value> {
        let leaf = prop_oneof![
            Just(serde_json::Value::Null),
            any::<bool>().prop_map(serde_json::Value::Bool),
            (-1_000_000_000_i64..=1_000_000_000).prop_map(serde_json::Value::from),
            "[a-zA-Z0-9_ ./-]{0,16}".prop_map(serde_json::Value::String),
        ];
        leaf.prop_recursive(3, 24, 6, |inner| {
            prop_oneof![
                proptest::collection::vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
                proptest::collection::hash_map("[a-z]{1,6}", inner, 0..4)
                    .prop_map(|m| serde_json::Value::Object(m.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn rules_round_trip_through_serde_losslessly(rule in recur_rule_strategy()) {
            let json = serde_json::to_value(&rule).unwrap();
            let back: RecurRule = serde_json::from_value(json).unwrap();
            prop_assert_eq!(back, rule);
        }

        // Strict-decode totality: arbitrary JSON is `Ok(valid) ∨ Err`,
        // never a panic and never a silently-wrong rule; every `Ok`
        // re-encodes to a value that decodes equal (§6.3).
        #[test]
        fn arbitrary_json_decodes_ok_or_err_and_ok_reencodes_equal(j in json_strategy()) {
            if let Ok(rule) = serde_json::from_value::<RecurRule>(j.clone()) {
                let re = serde_json::to_value(&rule).unwrap();
                let back: RecurRule = serde_json::from_value(re).unwrap();
                prop_assert_eq!(back, rule);
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

#[cfg(test)]
mod text_form_tests {
    use super::*;
    use crate::test_support::{proptest_config, weekday_strategy};
    use InvalidRecur as E;
    use chrono::Weekday::*;
    use proptest::prelude::*;

    fn set(days: &[Weekday]) -> WeekdaySet {
        WeekdaySet::new(days.iter().copied()).unwrap()
    }

    #[test]
    fn interval_text_forms_round_trip_and_reject_the_edges() {
        // Accept edges and the canonical form.
        assert_eq!(EveryDays::parse_text_form("3d").unwrap().get(), 3);
        assert_eq!(EveryDays::parse_text_form("3").unwrap().get(), 3);
        assert_eq!(EveryDays::parse_text_form("1").unwrap().text_form(), "1d");
        assert_eq!(EveryWeeks::parse_text_form("2w").unwrap().text_form(), "2w");
        assert_eq!(EveryWeeks::parse_text_form("2").unwrap().get(), 2);
        // Reject edges: zero intervals and garbage.
        assert!(matches!(
            EveryDays::parse_text_form("0d"),
            Err(E::ZeroInterval)
        ));
        assert!(matches!(
            EveryWeeks::parse_text_form("0w"),
            Err(E::ZeroInterval)
        ));
        assert!(matches!(
            EveryDays::parse_text_form("abc"),
            Err(E::BadNumber { got }) if got == "abc"
        ));
        assert!(matches!(
            EveryWeeks::parse_text_form("soon"),
            Err(E::BadNumber { got }) if got == "soon"
        ));
    }

    #[test]
    fn count_text_form_rejects_zero_and_garbage() {
        assert_eq!(
            OccurrenceCount::parse_text_form("10").unwrap().text_form(),
            "10"
        );
        assert!(matches!(
            OccurrenceCount::parse_text_form("0"),
            Err(E::ZeroInterval)
        ));
        assert!(matches!(
            OccurrenceCount::parse_text_form("many"),
            Err(E::BadNumber { .. })
        ));
    }

    #[test]
    fn weekday_set_text_form_is_canonical_case_sensitive_and_set_semantic() {
        assert_eq!(set(&[Mon, Wed, Fri]).text_form(), "mon,wed,fri");
        // Canonical order regardless of input order; duplicates dedup.
        assert_eq!(
            WeekdaySet::parse_text_form("sun,wed,mon").unwrap(),
            set(&[Mon, Wed, Sun])
        );
        assert_eq!(
            WeekdaySet::parse_text_form("mon,mon").unwrap(),
            set(&[Mon]),
            "duplicates dedup silently"
        );
        // Case-sensitive: `Mon` is not in the lowercase vocabulary.
        assert!(matches!(
            WeekdaySet::parse_text_form("Mon"),
            Err(E::BadWeekday { got }) if got == "Mon"
        ));
        // An empty comma slot is a typed weekday rejection.
        assert!(matches!(
            WeekdaySet::parse_text_form("mon,,"),
            Err(E::BadWeekday { got }) if got.is_empty()
        ));
        assert!(matches!(
            WeekdaySet::parse_text_form(""),
            Err(E::EmptyWeekdaySet)
        ));
    }

    #[test]
    fn day_of_month_text_form_rejects_the_range_edges() {
        assert_eq!(DayOfMonth::parse_text_form("15").unwrap().text_form(), "15");
        // The day/nth range edges, typed.
        for bad in ["0", "32"] {
            let err = DayOfMonth::parse_text_form(bad).expect_err(bad);
            assert!(
                matches!(err, E::DayOutOfRange { got } if got.to_string() == bad),
                "{bad}"
            );
        }
        assert!(matches!(
            DayOfMonth::parse_text_form("fifteen"),
            Err(E::BadNumber { .. })
        ));
        // The ordinal suffix is part of the nth vocabulary.
        assert_eq!(NthWeek::parse_text_form("2nd").unwrap().get(), 2);
        assert!(matches!(
            NthWeek::parse_text_form("6th"),
            Err(E::WeekOutOfRange { got }) if got == 6
        ));
        assert!(matches!(
            NthWeek::parse_text_form("2"),
            Err(E::BadNumber { .. })
        ));
    }

    #[test]
    fn monthly_anchor_text_form_round_trips_both_shapes() {
        let on_day = MonthlyAnchor::parse_text_form("15").unwrap();
        assert_eq!(on_day, MonthlyAnchor::OnDay(DayOfMonth::new(15).unwrap()));
        assert_eq!(on_day.text_form(), "15");
        let nth = MonthlyAnchor::parse_text_form("2nd-mon").unwrap();
        assert_eq!(
            nth,
            MonthlyAnchor::NthWeekday(NthWeek::new(2).unwrap(), Mon)
        );
        assert_eq!(nth.text_form(), "2nd-mon");
        // A position outside 1..=5 is a typed range rejection, never a
        // fifth meaning.
        assert!(matches!(
            MonthlyAnchor::parse_text_form("15th-mon"),
            Err(E::WeekOutOfRange { got }) if got == 15
        ));
        // The day token stays vocabulary-checked.
        assert!(matches!(
            MonthlyAnchor::parse_text_form("2nd-XYZ"),
            Err(E::BadWeekday { got }) if got == "XYZ"
        ));
        assert!(matches!(
            MonthlyAnchor::parse_text_form("zz"),
            Err(E::BadNumber { .. })
        ));
    }

    #[test]
    fn pattern_and_bound_text_forms_render_canonically() {
        let one = EveryDays::new(1).unwrap();
        let three = EveryDays::new(3).unwrap();
        let one_week = EveryWeeks::new(1).unwrap();
        let two_weeks = EveryWeeks::new(2).unwrap();
        assert_eq!(RecurPattern::Daily { every: one }.text_form(), "daily");
        assert_eq!(RecurPattern::Daily { every: three }.text_form(), "every 3d");
        assert_eq!(
            RecurPattern::Weekly {
                interval: one_week,
                on: set(&[Mon, Fri])
            }
            .text_form(),
            "weekly on mon,fri"
        );
        assert_eq!(
            RecurPattern::Weekly {
                interval: two_weeks,
                on: set(&[Mon])
            }
            .text_form(),
            "every 2w on mon"
        );
        assert_eq!(
            RecurPattern::Monthly {
                on: MonthlyAnchor::OnDay(DayOfMonth::new(15).unwrap())
            }
            .text_form(),
            "monthly on 15"
        );
        assert_eq!(
            RecurPattern::Monthly {
                on: MonthlyAnchor::NthWeekday(NthWeek::new(2).unwrap(), Fri)
            }
            .text_form(),
            "monthly on 2nd-fri"
        );
        assert_eq!(RecurBound::None.text_form(), "");
        assert_eq!(
            RecurBound::Until(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap()).text_form(),
            "until 2026-12-31"
        );
        assert_eq!(
            RecurBound::Count(OccurrenceCount::new(10).unwrap()).text_form(),
            "count 10"
        );
    }

    // T2 — the round-trip property over generated components.
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn component_text_forms_round_trip(
            days in 1_u16..=u16::MAX,
            weeks in 1_u16..=u16::MAX,
            count in 1_u32..=u32::MAX,
            day in 1_u32..=31,
            nth in 1_u32..=5,
            weekday in weekday_strategy(),
        ) {
            let ed = EveryDays::new(days).unwrap();
            prop_assert_eq!(EveryDays::parse_text_form(&ed.text_form()), Ok(ed));
            let ew = EveryWeeks::new(weeks).unwrap();
            prop_assert_eq!(EveryWeeks::parse_text_form(&ew.text_form()), Ok(ew));
            let oc = OccurrenceCount::new(count).unwrap();
            prop_assert_eq!(OccurrenceCount::parse_text_form(&oc.text_form()), Ok(oc));
            let dm = DayOfMonth::new(day).unwrap();
            prop_assert_eq!(DayOfMonth::parse_text_form(&dm.text_form()), Ok(dm));
            let nw = NthWeek::new(nth).unwrap();
            prop_assert_eq!(NthWeek::parse_text_form(&nw.text_form()), Ok(nw));
            let anchor = MonthlyAnchor::NthWeekday(nw, weekday);
            prop_assert_eq!(
                MonthlyAnchor::parse_text_form(&anchor.text_form()),
                Ok(anchor)
            );
        }

        #[test]
        fn weekday_set_text_form_round_trips(days in proptest::collection::vec(weekday_strategy(), 1..8)) {
            let parsed = WeekdaySet::new(days).unwrap();
            prop_assert_eq!(WeekdaySet::parse_text_form(&parsed.text_form()), Ok(parsed));
        }
    }
}

#[cfg(test)]
mod next_occurrence_tests {
    use super::*;
    use crate::entities::TaskClocks;
    use crate::test_support::{proptest_config, recur_rule_strategy};
    use chrono::{TimeZone as _, Weekday};
    use proptest::prelude::*;

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// An open head scheduled at midnight UTC of `scheduled` (or
    /// dateless when `None`).
    fn head(scheduled: Option<NaiveDate>) -> Task {
        Task {
            id: crate::ids::TaskId::from_uuid(uuid::Uuid::from_u128(1)),
            remote: None,
            title: "head".into(),
            description: String::new(),
            duedate: None,
            scheduled: scheduled.map(|d| ts(0) + days((d - date(1970, 1, 1)).num_days())),
            done: None,
            stack: crate::ids::StackId::from_uuid(uuid::Uuid::from_u128(2)),
            order: 0,
            label_memberships: std::collections::BTreeMap::default(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(0),
                description: ts(0),
                duedate: ts(0),
                scheduled: ts(0),
                done: ts(0),
                position: ts(0),
                effort: ts(0),
                importance: ts(0),
                urgency: ts(0),
                recur: ts(0),
                archived: ts(0),
                deleted: ts(0),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn calendar(start: chrono::NaiveTime, offset_seconds: i32) -> crate::days::DayCalendar {
        crate::days::DayCalendar {
            day_start_time: start,
            offset: chrono::FixedOffset::east_opt(offset_seconds).unwrap(),
        }
    }

    fn rule(pattern: RecurPattern, anchor: NaiveDate, bound: RecurBound) -> RecurRule {
        RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor,
            pattern,
            bound,
            continued_from: None,
        }
    }

    fn days(n: i64) -> chrono::TimeDelta {
        chrono::TimeDelta::try_days(n).unwrap()
    }

    /// The epoch instant of `d`'s UTC midnight.
    fn midnight_utc(d: NaiveDate) -> DateTime<Utc> {
        ts((d - date(1970, 1, 1)).num_days() * 86_400)
    }

    #[test]
    fn stale_daily_advertises_today_and_is_stale() {
        let day = calendar(chrono::NaiveTime::MIN, 0);
        let anchor = date(2026, 10, 1);
        let r = rule(
            RecurPattern::Daily {
                every: EveryDays::new(1).unwrap(),
            },
            anchor,
            RecurBound::None,
        );
        // Head scheduled 2026-10-19, today is 2026-10-21 (midnight UTC).
        let now = midnight_utc(date(2026, 10, 21));
        let h = head(Some(date(2026, 10, 19)));
        let next = next_occurrence(&r, &h, day, now);
        assert!(next.stale);
        assert_eq!(next.date, Some(date(2026, 10, 21)));
    }

    #[test]
    fn stale_weekly_skips_ahead_to_the_next_member() {
        let day = calendar(chrono::NaiveTime::MIN, 0);
        // Weekly on Mondays, anchor a Monday; today is Wednesday
        // 2026-10-21, head stale at Monday 2026-10-19 → next Monday.
        let monday = date(2026, 10, 5);
        let r = rule(
            RecurPattern::Weekly {
                interval: EveryWeeks::new(1).unwrap(),
                on: WeekdaySet::new([Weekday::Mon]).unwrap(),
            },
            monday,
            RecurBound::None,
        );
        let now = midnight_utc(date(2026, 10, 21));
        let next = next_occurrence(&r, &head(Some(date(2026, 10, 19))), day, now);
        assert!(next.stale);
        assert_eq!(next.date, Some(date(2026, 10, 26)));
    }

    #[test]
    fn fresh_head_advertises_next_after_from_its_reference() {
        let day = calendar(chrono::NaiveTime::MIN, 0);
        let anchor = date(2026, 10, 1);
        let r = rule(
            RecurPattern::Daily {
                every: EveryDays::new(3).unwrap(),
            },
            anchor,
            RecurBound::None,
        );
        // Reference today (2026-10-21): the next member is strictly
        // after — the anchor-aligned grid 1,4,…,19,22 lands on the 22nd.
        let now = midnight_utc(date(2026, 10, 21));
        let next = next_occurrence(&r, &head(Some(date(2026, 10, 21))), day, now);
        assert!(!next.stale);
        assert_eq!(next.date, Some(date(2026, 10, 22)));
    }

    #[test]
    fn a_dateless_head_is_never_stale_and_projects_from_the_window_date() {
        let day = calendar(chrono::NaiveTime::MIN, 0);
        let anchor = date(2026, 10, 1);
        let r = rule(
            RecurPattern::Daily {
                every: EveryDays::new(1).unwrap(),
            },
            anchor,
            RecurBound::None,
        );
        let now = midnight_utc(date(2026, 10, 21));
        let next = next_occurrence(&r, &head(None), day, now);
        assert!(!next.stale);
        assert_eq!(next.date, Some(date(2026, 10, 22)));
    }

    #[test]
    fn the_night_owl_window_reads_early_morning_as_yesterday() {
        // Day starts 05:00, offset +02:00. `now` is 03:00 local on the
        // 6th — the window-date is the *5th*; a head scheduled the 5th
        // is fresh (reference == today), not stale.
        let day = calendar(chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(), 7_200);
        let anchor = date(2026, 10, 1);
        let r = rule(
            RecurPattern::Daily {
                every: EveryDays::new(1).unwrap(),
            },
            anchor,
            RecurBound::None,
        );
        // 2026-10-06 03:00 local (+02:00) = 01:00 UTC on the 6th.
        let now = midnight_utc(date(2026, 10, 6)) + chrono::TimeDelta::try_hours(1).unwrap();
        let next = next_occurrence(&r, &head(Some(date(2026, 10, 5))), day, now);
        assert!(!next.stale, "03:00 with a 05:00 start is still the 5th");
        assert_eq!(next.date, Some(date(2026, 10, 6)));
    }

    #[test]
    fn an_exhausted_until_bound_advertises_nothing() {
        let day = calendar(chrono::NaiveTime::MIN, 0);
        let anchor = date(2026, 10, 1);
        let r = rule(
            RecurPattern::Daily {
                every: EveryDays::new(1).unwrap(),
            },
            anchor,
            RecurBound::Until(date(2026, 10, 21)),
        );
        let now = midnight_utc(date(2026, 10, 21));
        // Fresh head: next_after says the 22nd, past the bound → None.
        let next = next_occurrence(&r, &head(Some(date(2026, 10, 21))), day, now);
        assert!(!next.stale);
        assert_eq!(next.date, None);
        // A stale head whose roll target passes the bound likewise.
        let stale_head = head(Some(date(2026, 10, 19)));
        let next = next_occurrence(&r, &stale_head, day, now);
        // roll_target catches up to the 21st, which the inclusive bound
        // still admits.
        assert_eq!(next.date, Some(date(2026, 10, 21)));
    }

    // The property: the law never disagrees with the arithmetic it
    // composes — stale ⇒ roll_target (and forward), fresh ⇒ next_after,
    // and the Until bound is never exceeded by an advertised date.
    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn next_occurrence_matches_its_composed_laws(
            rule in recur_rule_strategy(),
            scheduled_offset in 0_i64..=4_000,
            now_offset in 0_i64..=4_000,
            day_start_seconds in 0_u32..=86_399,
            offset_seconds in -50_400_i32..=50_400,
        ) {
            let day = calendar(
                chrono::NaiveTime::from_num_seconds_from_midnight_opt(day_start_seconds, 0)
                    .expect("valid time of day"),
                offset_seconds,
            );
            let scheduled = rule
                .anchor
                .checked_add_signed(days(scheduled_offset.min(now_offset)))
                .expect("in range");
            let base = date(2026, 1, 1).and_hms_opt(0, 0, 0).unwrap();
            let now = (base + days(now_offset)).and_utc();
            let h = head(Some(scheduled));
            let next = next_occurrence(&rule, &h, day, now);
            let today = day.today(now);
            let reference = roll_reference_date(&h, day).unwrap();
            let admits = |d: NaiveDate| match rule.bound {
                RecurBound::Until(u) if d > u => None,
                _ => Some(d),
            };
            if reference < today {
                prop_assert!(next.stale);
                prop_assert_eq!(
                    next.date,
                    admits(roll_target(&rule.pattern, rule.anchor, reference, today))
                );
                if let Some(d) = next.date {
                    prop_assert!(d > reference);
                }
            } else {
                prop_assert!(!next.stale);
                prop_assert_eq!(
                    next.date,
                    admits(next_after(&rule.pattern, rule.anchor, reference))
                );
            }
            // The Until bound is never exceeded (Count is invisible).
            if let RecurBound::Until(u) = rule.bound {
                prop_assert!(next.date.is_none_or(|d| d <= u));
            }
        }
    }
}
