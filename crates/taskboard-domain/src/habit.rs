// SPDX-License-Identifier: MIT OR Apache-2.0
//! Habit trackers: the [`Habit`] entity, its [`HabitState`] home, and
//! the period-boundary vocabulary (DR-6/5).
//!
//! **Nothing ticks** — never-done is native. A habit has no spawn/roll/
//! done machinery of any kind; a window's state *is* its log row, and
//! the log is keyed `(habit, window)` with ≤ 1 row per key — the
//! reminder fact shape with a count payload. Every period instant
//! derives through the `days` vocabulary ([`crate::days`]); this module
//! introduces no second local-date concept.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::days::DayCalendar;
use crate::ids::{BoardId, HabitId, SeriesId};

/// Hand-rolled A1 count: private field, fallible mint rejecting zero.
/// Decode is strict (a stored `0` cannot deserialize — the recur.rs
/// posture: mints decode strictly, leniency lives one layer up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32")]
pub struct NonZeroCount(u32);

/// The count mint rejected zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("habit target count must be at least 1")]
pub struct InvalidHabitTarget;

impl NonZeroCount {
    /// The mint: `n` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidHabitTarget`] when `n` is 0.
    pub const fn new(n: u32) -> Result<Self, InvalidHabitTarget> {
        if n == 0 {
            Err(InvalidHabitTarget)
        } else {
            Ok(Self(n))
        }
    }

    /// The count (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for NonZeroCount {
    type Error = InvalidHabitTarget;

    fn try_from(n: u32) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// Which period a habit's target counts over. The window vocabulary of
/// the whole module ([`period_start`] and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PeriodKind {
    /// One day window (the `DayCalendar` day).
    Day,
    /// One ISO week (Monday-start).
    Week,
    /// One calendar month.
    Month,
}

/// The habit's quota over its period. Externally tagged on the wire
/// (`{"PerDay": 3}`); zero is unrepresentable at every layer because
/// the payload goes through the [`NonZeroCount`] mint's strict decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HabitTarget {
    /// The quota counts per day window.
    PerDay(NonZeroCount),
    /// The quota counts per ISO week.
    PerWeek(NonZeroCount),
    /// The quota counts per calendar month.
    PerMonth(NonZeroCount),
}

impl HabitTarget {
    /// The period this target counts over.
    #[must_use]
    pub const fn kind(&self) -> PeriodKind {
        match self {
            HabitTarget::PerDay(_) => PeriodKind::Day,
            HabitTarget::PerWeek(_) => PeriodKind::Week,
            HabitTarget::PerMonth(_) => PeriodKind::Month,
        }
    }

    /// The quota (≥ 1 always — the mint).
    #[must_use]
    pub const fn quota(&self) -> NonZeroCount {
        match self {
            HabitTarget::PerDay(n) | HabitTarget::PerWeek(n) | HabitTarget::PerMonth(n) => *n,
        }
    }
}

/// A canonical period-start local date. Private field; the infallible
/// mint floors to the period start ([`WindowStart::of_period`]), so
/// command-path keys are canonical by construction.
///
/// # Minting vs. leniency
///
/// Decoded rows are **lenient by design**: a non-canonical date decodes
/// as-is (it may be a synced fact keyed by a former target kind — D-H12)
/// and is *analytically inert*: every projection enumerates canonical
/// windows of the current kind only, so a non-canonical row reads as
/// absent. `AdjustHabitLog` rejects non-canonical windows explicitly at
/// the planner (typed `InvalidHabitWindow`), so no validate-and-forget
/// path exists on the write side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowStart(NaiveDate);

impl WindowStart {
    /// The canonical window-start of `date`'s period: floors the date
    /// to its period boundary ([`period_start`]).
    #[must_use]
    pub fn of_period(kind: PeriodKind, date: NaiveDate) -> Self {
        Self(period_start(kind, date))
    }

    /// The period-start date.
    #[must_use]
    pub const fn as_date(self) -> NaiveDate {
        self.0
    }
}

/// The habit's per-field LWW write clocks (the §4.1 clock list,
/// verbatim). `continued_from` carries deliberately no slot: it is
/// conversion-seam data, not contested content (D-H7a — same treatment
/// as `Task.remote`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitClocks {
    /// Title write clock.
    pub title: DateTime<Utc>,
    /// Target write clock.
    pub target: DateTime<Utc>,
    /// Order write clock.
    pub order: DateTime<Utc>,
    /// Anchor-time write clock.
    pub anchor_time: DateTime<Utc>,
    /// Tombstone clock.
    pub deleted: DateTime<Utc>,
}

impl HabitClocks {
    /// All clocks at one stamp (the mint path — a newborn row's fields
    /// share its creation instant).
    #[must_use]
    pub const fn all(now: DateTime<Utc>) -> Self {
        Self {
            title: now,
            target: now,
            order: now,
            anchor_time: now,
            deleted: now,
        }
    }
}

/// A habit tracker: a quota per period, manual list order, and an
/// optional display anchor time. Lives on a board; tombstones like
/// every entity (the log outlives the tracker — history is never
/// purged by [`crate::persistence::PersistenceAction::UpsertHabit`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    /// The habit's local identity (roaming family — the synced
    /// section's wire key).
    pub id: HabitId,
    /// The owning board.
    pub board: BoardId,
    /// Display title.
    pub title: String,
    /// The quota per period. A kind change never deletes former-kind
    /// log rows — they stay in the map (they sync as facts) but are
    /// analytically inert (D-H12).
    pub target: HabitTarget,
    /// Manual list order (D-C13); the planner appends on create and
    /// the UI computes midpoints on drag. `anchor_time` never
    /// influences any ordering.
    pub order: i64,
    /// Display chip only (D-C13) — a local time-of-day shown next to
    /// the habit; it is in no ordering path and drives no reminder.
    pub anchor_time: Option<chrono::NaiveTime>,
    /// Per-field LWW clocks.
    pub clocks: HabitClocks,
    /// The §4.7 conversion seam, habit side: the series this habit
    /// continues (set only by the explicit `UpdateHabit` write path —
    /// D-H7a; the read-side history union is chunk 6's).
    pub continued_from: Option<SeriesId>,
    /// Tombstone. Resurrection-safe on sync by the same LWW-clock
    /// argument as tasks (the fold is chunk 6's).
    pub deleted: bool,
}

impl Habit {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
    }
}

/// The log key: one habit's count in one canonical window. `BTreeMap`
/// key — the derived order keeps serialization deterministic. On the
/// wire (JSON map keys must be strings — the `ReminderSnoozeKey`
/// precedent) the key serializes as the single string
/// `"<habit uuid>|<window naive date>"` and parses strictly back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HabitLogKey {
    /// The counted habit.
    pub habit: HabitId,
    /// The canonical period-start of the counted window.
    pub window: WindowStart,
}

impl HabitLogKey {
    /// The string wire form (`"<habit uuid>|<window naive date>"`).
    #[must_use]
    pub fn wire_form(&self) -> String {
        format!("{}|{}", self.habit.as_uuid(), self.window.as_date())
    }

    /// The strict inverse of [`wire_form`]; `None` on any malformed
    /// input (an undecodable key is corruption, not data).
    #[must_use]
    pub fn parse_wire_form(raw: &str) -> Option<Self> {
        let (habit, window) = raw.split_once('|')?;
        let habit = HabitId::from(uuid::Uuid::parse_str(habit).ok()?);
        Some(Self {
            habit,
            window: WindowStart(chrono::NaiveDate::parse_from_str(window, "%Y-%m-%d").ok()?),
        })
    }
}

impl Serialize for HabitLogKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire_form())
    }
}

impl<'de> Deserialize<'de> for HabitLogKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse_wire_form(&raw).ok_or_else(|| serde::de::Error::custom("invalid habit log key"))
    }
}

/// One window's count row: the payload is the count plus a single LWW
/// clock (the normative shape — no per-completion instants, which is
/// exactly why attribution happens at log time, D-H5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitLogEntry {
    /// The window's count (0 is legal — an audited correction may
    /// empty a window; the row stays, honestly clocked).
    pub count: u32,
    /// The row's LWW clock (always the writing planner's `now`).
    pub clock: DateTime<Utc>,
}

/// The habit sub-state, nested in [`crate::state::AppState`] like
/// `TrackingState`. `BTreeMap` keeps serialization deterministic (the
/// state.rs determinism contract).
///
/// # Documented leniency (D-H12)
///
/// The map may hold rows keyed by windows of a *former* target kind
/// after a kind change (they sync as facts; deleting them would forge
/// history). Every projection reads canonical windows of the *current*
/// kind only, so such rows are analytically inert — pinned by test.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitState {
    /// All habits, keyed by id (live rows and tombstones alike).
    pub habits: BTreeMap<HabitId, Habit>,
    /// The count log, keyed `(habit, window)`; ≤ 1 row per key.
    pub log: BTreeMap<HabitLogKey, HabitLogEntry>,
}

/// The period-start local date of `date`'s period: the day itself;
/// the ISO week's Monday; the month's 1st. Pure, total, idempotent —
/// the floor every [`WindowStart`] mint goes through.
///
/// # Panics
///
/// Panics only if chrono cannot build day 1 of `date`'s month — a
/// programming error (day 1 exists in every calendar month), not an
/// input condition.
#[must_use]
pub fn period_start(kind: PeriodKind, date: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => date,
        // The same Monday rule recur.rs's `week_monday` uses, via
        // chrono's week arithmetic (`first_day` of the Mon-start week).
        PeriodKind::Week => date.week(chrono::Weekday::Mon).first_day(),
        // Day 1 exists in every month, so `with_day` is infallible here.
        PeriodKind::Month => date
            .with_day(1)
            .expect("day 1 exists in every calendar month"),
    }
}

/// The first day of the *next* period after `start` (which must be a
/// period start; anything else floors first — the same canonical law
/// the mints enforce). Month steps use checked month arithmetic.
///
/// Saturation (documented leniency, unreachable in the bounded
/// proptest domain): a month step past chrono's date bound saturates
/// at [`NaiveDate::MAX`].
#[must_use]
pub fn period_after(kind: PeriodKind, start: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => start
            .checked_add_days(chrono::Days::new(1))
            .unwrap_or(NaiveDate::MAX),
        PeriodKind::Week => start
            .checked_add_days(chrono::Days::new(7))
            .unwrap_or(NaiveDate::MAX),
        PeriodKind::Month => start
            .checked_add_months(chrono::Months::new(1))
            .unwrap_or(NaiveDate::MAX),
    }
}

/// The last day of the *previous* period before `start` (the inverse
/// of [`period_after`]). Saturation mirrors [`period_after`]: a month
/// step before the bound saturates at [`NaiveDate::MIN`].
#[must_use]
pub fn period_before(kind: PeriodKind, start: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => start
            .checked_sub_days(chrono::Days::new(1))
            .unwrap_or(NaiveDate::MIN),
        PeriodKind::Week => start
            .checked_sub_days(chrono::Days::new(7))
            .unwrap_or(NaiveDate::MIN),
        PeriodKind::Month => prev_month_checked(start).unwrap_or(NaiveDate::MIN),
    }
}

/// `start`'s previous month's first day (`None` only at the chrono
/// bound — saturating callers substitute [`NaiveDate::MIN`]).
fn prev_month_checked(start: NaiveDate) -> Option<NaiveDate> {
    let first = start.with_day(1)?;
    let prev_month_end = first.checked_sub_days(chrono::Days::new(1))?;
    prev_month_end.with_day(1)
}

/// The period-end *instant* of the window starting at `start`: the
/// opening instant of the next period's day window — through the
/// `DayCalendar` vocabulary, never a second timezone concept (D-H4).
#[must_use]
pub fn period_end(kind: PeriodKind, start: NaiveDate, day: DayCalendar) -> DateTime<Utc> {
    crate::days::window_start(period_after(kind, start), day.day_start_time, day.offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{naive_date_strategy, proptest_config};
    use chrono::{Datelike, TimeZone};
    use proptest::prelude::*;

    fn habit(raw: u128, target: HabitTarget) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: BoardId::from(uuid::Uuid::from_u128(900)),
            title: format!("habit {raw}"),
            target,
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(Utc.timestamp_opt(100, 0).unwrap()),
            continued_from: None,
            deleted: false,
        }
    }

    #[test]
    fn non_zero_count_mint_rejects_zero_and_decodes_strictly() {
        assert_eq!(NonZeroCount::new(0), Err(InvalidHabitTarget));
        assert_eq!(NonZeroCount::new(1).unwrap().get(), 1);
        // Strict decode: a stored 0 cannot deserialize (recur.rs posture).
        let back: Result<NonZeroCount, _> = serde_json::from_str("0");
        assert!(back.is_err(), "zero is unrepresentable at decode");
        let one: NonZeroCount = serde_json::from_str("3").unwrap();
        assert_eq!(one.get(), 3);
    }

    #[test]
    fn target_carries_kind_and_quota_and_round_trips() {
        let target = HabitTarget::PerWeek(NonZeroCount::new(4).unwrap());
        assert_eq!(target.kind(), PeriodKind::Week);
        assert_eq!(target.quota().get(), 4);
        let json = serde_json::to_string(&target).unwrap();
        assert_eq!(serde_json::from_str::<HabitTarget>(&json).unwrap(), target);
        // Externally tagged on the wire.
        assert_eq!(json, r#"{"PerWeek":4}"#);
    }

    #[test]
    fn habit_and_state_serde_roundtrip_including_the_log() {
        let mut state = HabitState::default();
        let h = habit(1, HabitTarget::PerDay(NonZeroCount::new(3).unwrap()));
        state.habits.insert(h.id, h.clone());
        state.log.insert(
            HabitLogKey {
                habit: h.id,
                window: WindowStart::of_period(PeriodKind::Day, date(2026, 10, 28)),
            },
            HabitLogEntry {
                count: 2,
                clock: Utc.timestamp_opt(500, 0).unwrap(),
            },
        );
        let json = serde_json::to_string(&state).unwrap();
        let back: HabitState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
        // The conversion seam field round-trips both directions.
        let linked = Habit {
            continued_from: Some(crate::ids::SeriesId::from(uuid::Uuid::from_u128(7))),
            ..h.clone()
        };
        let json = serde_json::to_string(&linked).unwrap();
        assert_eq!(serde_json::from_str::<Habit>(&json).unwrap(), linked);
        let unlinked_json = serde_json::to_string(&h).unwrap();
        assert!(
            unlinked_json.contains(r#""continued_from":null"#),
            "the null seam serializes explicitly"
        );
    }

    #[test]
    fn a_non_canonical_window_start_decodes_and_is_inert_by_type() {
        // Decoded leniency: the date decodes verbatim; the type carries
        // no canonicalization at decode (D-H12 — former-kind synced
        // facts stay in the map). Inertness is the projections' law,
        // pinned there.
        let raw = date(2026, 10, 28); // a Wednesday — not a Monday, not a 1st
        let json = serde_json::to_string(&raw).unwrap();
        let window: WindowStart = serde_json::from_str(&json).unwrap();
        assert_eq!(window.as_date(), raw);
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn period_start_floors_to_the_period_boundary_per_kind() {
        let wednesday = date(2026, 10, 28);
        assert_eq!(period_start(PeriodKind::Day, wednesday), wednesday);
        assert_eq!(
            period_start(PeriodKind::Week, wednesday),
            date(2026, 10, 26)
        );
        assert_eq!(
            period_start(PeriodKind::Month, wednesday),
            date(2026, 10, 1)
        );
        // Idempotent by hand at the boundary itself.
        assert_eq!(
            period_start(PeriodKind::Week, date(2026, 10, 26)),
            date(2026, 10, 26)
        );
    }

    #[test]
    fn period_end_runs_through_the_day_window_vocabulary() {
        let day = DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
            offset: chrono::FixedOffset::east_opt(2 * 3600).unwrap(),
        };
        // Daily: end = the next day's window opening instant.
        let start = date(2026, 10, 28);
        assert_eq!(
            period_end(PeriodKind::Day, start, day),
            crate::days::window_start(date(2026, 10, 29), day.day_start_time, day.offset)
        );
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn period_start_is_idempotent(kind in period_kind_strategy(), d in naive_date_strategy()) {
            let start = period_start(kind, d);
            prop_assert_eq!(period_start(kind, start), start);
        }

        #[test]
        fn period_after_advances_exactly_one_period_and_round_trips(
            kind in period_kind_strategy(),
            d in naive_date_strategy(),
        ) {
            let start = period_start(kind, d);
            let next = period_after(kind, start);
            prop_assert!(next > start, "one period forward strictly");
            prop_assert_eq!(period_start(kind, next), next, "the next window is canonical");
            prop_assert_eq!(period_before(kind, next), start, "before inverts after");
        }

        #[test]
        fn week_starts_are_mondays_and_month_starts_are_day_one(
            kind in period_kind_strategy(),
            d in naive_date_strategy(),
        ) {
            let start = period_start(kind, d);
            match kind {
                PeriodKind::Day => {}
                PeriodKind::Week => {
                    prop_assert_eq!(start.weekday(), chrono::Weekday::Mon);
                }
                PeriodKind::Month => {
                    prop_assert_eq!(start.day(), 1);
                }
            }
        }
    }

    fn period_kind_strategy() -> impl Strategy<Value = PeriodKind> {
        prop_oneof![
            Just(PeriodKind::Day),
            Just(PeriodKind::Week),
            Just(PeriodKind::Month),
        ]
    }
}
