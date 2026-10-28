// SPDX-License-Identifier: MIT OR Apache-2.0
//! Habit strategies (DR-6/5): targets, entities, and a fixture pairing
//! a habit with a log over consecutive canonical periods plus a `now`
//! inside its current window — the bounded domain every attribution and
//! streak property runs over (the recur.rs posture: bounded so the
//! chrono-saturation paths are unit-pinned, never luck).

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use proptest::prelude::*;

use super::{string_strategy, timestamp_strategy};
use crate::habit::{
    Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitState, HabitTarget, NonZeroCount,
    PeriodKind, period_start,
};

/// One of the three period kinds, uniform.
pub fn period_kind_strategy() -> impl Strategy<Value = PeriodKind> {
    prop_oneof![
        Just(PeriodKind::Day),
        Just(PeriodKind::Week),
        Just(PeriodKind::Month),
    ]
}

/// A nonzero quota (1..=50 — bounded so count arithmetic never overflows
/// `u32` in the properties).
///
/// # Panics
///
/// Panics only if the mint rejects the bounded range's lower end — a
/// programming error, not an input condition.
pub fn quota_strategy() -> impl Strategy<Value = NonZeroCount> {
    (1_u32..=50).prop_map(|n| NonZeroCount::new(n).expect("nonzero"))
}

/// A random target over a random period kind.
pub fn habit_target_strategy() -> impl Strategy<Value = HabitTarget> {
    (period_kind_strategy(), quota_strategy()).prop_map(|(kind, quota)| match kind {
        PeriodKind::Day => HabitTarget::PerDay(quota),
        PeriodKind::Week => HabitTarget::PerWeek(quota),
        PeriodKind::Month => HabitTarget::PerMonth(quota),
    })
}

/// A random habit entity (anchor chips and seam links included — the
/// negative pin that they influence no projection reads them).
///
/// # Panics
///
/// Panics only if chrono cannot build the bounded time-of-day — a
/// programming error, not an input condition.
pub fn habit_strategy() -> impl Strategy<Value = Habit> {
    (
        super::habit_id_strategy(),
        super::board_id_strategy(),
        string_strategy(),
        habit_target_strategy(),
        -100_i64..=100,
        proptest::option::of((0_u32..86_400).prop_map(|secs| {
            chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, 0)
                .expect("in-range seconds")
        })),
        proptest::option::of(super::series_id_strategy()),
        timestamp_strategy(),
        any::<bool>(),
    )
        .prop_map(
            |(id, board, title, target, order, anchor_time, continued_from, stamp, deleted)| {
                Habit {
                    id,
                    board,
                    title,
                    target,
                    order,
                    anchor_time,
                    clocks: HabitClocks::all(stamp),
                    continued_from,
                    deleted,
                }
            },
        )
}

/// The bounded date window the fixtures anchor on (±10 y around 2020 —
/// comfortably inside chrono's range, so no saturation path is ever
/// exercised by the properties; the extremes are unit-pinned instead).
fn anchor_date_strategy() -> impl Strategy<Value = NaiveDate> {
    let lo = NaiveDate::from_ymd_opt(2015, 1, 1)
        .expect("bound")
        .num_days_from_ce();
    let hi = NaiveDate::from_ymd_opt(2035, 12, 31)
        .expect("bound")
        .num_days_from_ce();
    (lo..=hi).prop_map(|ce| NaiveDate::from_num_days_from_ce_opt(ce).expect("in-range CE day"))
}

/// A habit paired with a log over consecutive canonical windows of its
/// *current* kind, and a `now` inside the anchor's window. Log rows
/// cover the anchor's window and the `depth` periods before it, each
/// with a random count (0..=80 — beyond every quota, so the "met" side
/// of the properties is reachable); clocks sit before `now`.
#[derive(Debug, Clone)]
pub struct HabitFixture {
    /// The fixture habit (live, one board).
    pub habit: Habit,
    /// The count log over the anchor's canonical windows.
    pub log: BTreeMap<HabitLogKey, HabitLogEntry>,
    /// The fixture's `now`: inside the anchor window's day.
    pub now: DateTime<Utc>,
    /// The UTC-midnight day calendar.
    pub day: crate::days::DayCalendar,
}

impl HabitFixture {
    /// The canonical window containing `now` (the natural window of the
    /// fixture's `now` under its calendar).
    #[must_use]
    pub fn natural(&self) -> crate::habit::WindowStart {
        crate::habit::WindowStart::of_period(self.habit.target.kind(), self.day.today(self.now))
    }

    /// The canonical window before `natural`.
    #[must_use]
    pub fn prev(&self) -> crate::habit::WindowStart {
        crate::habit::WindowStart::of_period(
            self.habit.target.kind(),
            crate::habit::period_before(self.habit.target.kind(), self.natural().as_date()),
        )
    }

    /// The fixture's state home.
    #[must_use]
    pub fn state(&self) -> HabitState {
        HabitState {
            habits: BTreeMap::from([(self.habit.id, self.habit.clone())]),
            log: self.log.clone(),
        }
    }
}

/// The composite fixture (see [`HabitFixture`]).
///
/// # Panics
///
/// Panics only if chrono cannot represent the bounded in-day offset and
/// row-clock spacing — a programming error, not an input condition.
pub fn habit_fixture_strategy() -> impl Strategy<Value = HabitFixture> {
    (
        habit_strategy(),
        anchor_date_strategy(),
        0_u32..=86_399,
        1_u32..=8,
        prop::collection::vec(0_u32..=80, 0..9),
    )
        .prop_map(|(mut habit, anchor, secs_into_day, depth, counts)| {
            let kind = habit.target.kind();
            // The properties command the habit; fixtures are live.
            habit.deleted = false;
            let day = crate::days::DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
            };
            let natural_date = period_start(habit.target.kind(), anchor);
            let now = crate::days::window_start(natural_date, day.day_start_time, day.offset)
                + chrono::TimeDelta::try_seconds(i64::from(secs_into_day)).expect("in-day offset");
            // Walk backward from the anchor so its window is the newest
            // row and `depth` covers its predecessors.
            let mut log = BTreeMap::new();
            let mut cursor = natural_date;
            for (i, count) in counts.into_iter().take(depth as usize).enumerate() {
                log.insert(
                    HabitLogKey {
                        habit: habit.id,
                        window: crate::habit::WindowStart::of_period(kind, cursor),
                    },
                    HabitLogEntry {
                        count,
                        clock: now
                            - chrono::TimeDelta::try_hours(i64::try_from(i).unwrap() * 24 + 48)
                                .expect("bounded offsets"),
                    },
                );
                cursor = crate::habit::period_before(habit.target.kind(), cursor);
            }
            HabitFixture {
                habit,
                log,
                now,
                day,
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(32))]

        /// Smoke: the strategy machinery must itself be sound — a
        /// sampled fixture's natural/prev are canonical and distinct.
        #[test]
        fn fixture_windows_are_canonical_and_ordered(
            fixture in habit_fixture_strategy(),
        ) {
            let kind = fixture.habit.target.kind();
            prop_assert_eq!(
                period_start(kind, fixture.natural().as_date()),
                fixture.natural().as_date()
            );
            prop_assert!(fixture.prev().as_date() < fixture.natural().as_date());
        }
    }
}
