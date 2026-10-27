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

use chrono::{NaiveDate, Weekday};
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
