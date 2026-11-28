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
    AnchorDay, Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitReminderOverride, HabitSnooze,
    HabitState, HabitTarget, MonthDay, NonZeroCount, NudgeBase, PeriodKind, WeekdaySet,
    period_start,
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

/// A random optional [`AnchorDay`], deliberately *unpaired* with any
/// target kind: the invariance properties need the mismatched pairings
/// (a `Weekday` on a `PerMonth` habit and a `MonthDay` on a `PerWeek`
/// habit are the documented-inert shapes) exercised too.
///
/// # Panics
///
/// Panics only if the [`MonthDay`] mint rejects its bounded 1..=31
/// domain — a programming error, not an input condition.
pub fn anchor_day_strategy() -> impl Strategy<Value = Option<AnchorDay>> {
    prop_oneof![
        Just(None),
        (0_u32..7).prop_map(|n| {
            Some(AnchorDay::Weekday(
                [
                    chrono::Weekday::Mon,
                    chrono::Weekday::Tue,
                    chrono::Weekday::Wed,
                    chrono::Weekday::Thu,
                    chrono::Weekday::Fri,
                    chrono::Weekday::Sat,
                    chrono::Weekday::Sun,
                ][n as usize],
            ))
        }),
        (1_u8..=31).prop_map(|n| Some(AnchorDay::MonthDay(MonthDay::new(n).expect("1..=31")))),
    ]
}

/// A random weekday set (never empty — the mint's law).
pub fn weekday_set_strategy() -> impl Strategy<Value = WeekdaySet> {
    (1_u8..=7, 0_u8..=127).prop_filter_map("at least one masked weekday", |(count, raw)| {
        // Take `count` distinct bits off the raw mask, Monday-first.
        let mut bits = 0_u8;
        let mut remaining = count;
        for bit in 0..7 {
            if raw & (1 << bit) != 0 {
                bits |= 1 << bit;
                remaining -= 1;
            }
            if remaining == 0 {
                break;
            }
        }
        if remaining > 0 {
            // Fill the shortfall from the low bits (mask too thin).
            for bit in 0..7 {
                if remaining == 0 {
                    break;
                }
                if bits & (1 << bit) == 0 {
                    bits |= 1 << bit;
                    remaining -= 1;
                }
            }
        }
        WeekdaySet::from_bits(bits)
    })
}

/// A random reminder override (HF4 — nudge parameters; no window,
/// streak, or ordering projection may read them).
///
/// # Panics
///
/// Panics only if chrono cannot build the bounded time-of-day — a
/// programming error, not an input condition.
pub fn reminder_override_strategy() -> impl Strategy<Value = HabitReminderOverride> {
    (
        prop_oneof![Just(NudgeBase::NudgeDay), Just(NudgeBase::PeriodStart)],
        -14_i32..=14,
        (0_u32..86_400).prop_map(|secs| {
            chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, 0)
                .expect("in-range seconds")
        }),
        prop_oneof![
            (1_u8..=72).prop_map(HabitSnooze::Hours),
            (1_u8..=14).prop_map(HabitSnooze::Days),
        ],
    )
        .prop_map(|(base, offset_days, time, snooze)| HabitReminderOverride {
            base,
            offset_days,
            time: Some(time),
            snooze,
        })
}

/// A random habit entity (anchor chips and seam links included — the
/// negative pin that they influence no projection reads them). The
/// description and anchor day ride along for the same reason: display
/// data that no window, streak, or ordering projection may read. The
/// HF4 fields (`scheduled_days`, `reminder`) ride for the same reason
/// plus their own pins (R-6 transparency; the nudge gate decides WHEN
/// only).
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
        string_strategy(),
        habit_target_strategy(),
        -100_i64..=100,
        proptest::option::of((0_u32..86_400).prop_map(|secs| {
            chrono::NaiveTime::from_num_seconds_from_midnight_opt(secs, 0)
                .expect("in-range seconds")
        })),
        anchor_day_strategy(),
        // The two HF4 fields ride as one nested pair (the tuple
        // strategy's arity ceiling): set + override.
        (
            proptest::option::of(weekday_set_strategy()),
            proptest::option::of(reminder_override_strategy()),
        ),
        proptest::option::of(super::series_id_strategy()),
        (timestamp_strategy(), any::<bool>()),
        proptest::option::of(
            (any::<u128>())
                .prop_map(|raw| crate::ids::AuthorId::from_uuid(uuid::Uuid::from_u128(raw))),
        ),
    )
        .prop_map(
            |(
                id,
                board,
                title,
                description,
                target,
                order,
                anchor_time,
                anchor_day,
                (scheduled_days, reminder),
                continued_from,
                (stamp, deleted),
                by,
            )| {
                Habit {
                    id,
                    board,
                    title,
                    description,
                    target,
                    order,
                    anchor_time,
                    anchor_day,
                    scheduled_days,
                    reminder,
                    clocks: HabitClocks::all(stamp),
                    continued_from,
                    deleted,
                    by,

                    paused_until: None,
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
    /// The habit's event log (the #1519 grammar): command-driving
    /// fixtures accumulate the minted events here so a sequential
    /// plan reads the previous plans' derived truth.
    pub events: BTreeMap<crate::habit_event::HabitEventId, crate::habit_event::HabitEvent>,
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
            time_log: std::collections::BTreeMap::default(),
            events: self.events.clone(),
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
            // The event-grammar fixture (the #1519 chunk-5 re-key):
            // the count log IS the derivation of the seeded events, so
            // the keystone mirror law holds by construction and the
            // command-driving properties read one truth. Events are
            // minted oldest-window-first, spaced inside their windows;
            // the fold's counts (computed below) become the log rows.
            let mut events = BTreeMap::new();
            let mut cursor = natural_date;
            let mut draft: Vec<(crate::habit::WindowStart, u32)> = Vec::new();
            for count in counts.into_iter().take(depth as usize) {
                draft.push((crate::habit::WindowStart::of_period(kind, cursor), count));
                cursor = crate::habit::period_before(habit.target.kind(), cursor);
            }
            draft.reverse(); // oldest window first
            for (window, count) in draft {
                let open =
                    crate::days::window_start(window.as_date(), day.day_start_time, day.offset);
                for ordinal in 0..count {
                    let at = open
                        + chrono::TimeDelta::try_seconds(i64::from(ordinal) + 1)
                            .expect("bounded spacing");
                    let id = crate::habit_event::legacy_habit_event_id(
                        habit.id,
                        window.as_date(),
                        at,
                        Some(ordinal),
                    );
                    let event = crate::habit_event::HabitEvent {
                        id,
                        habit: habit.id,
                        kind: crate::habit_event::HabitEventKind::Completion,
                        at,
                        by: None,
                        deleted: None,
                    };
                    events.insert(id, event);
                }
            }
            // The log is the fold of the seeded events (the keystone
            // law, by construction).
            let derived = crate::habit_event::derive_window_counts(&habit, events.values(), day);
            let mut log = BTreeMap::new();
            for (window, count) in derived {
                log.insert(
                    HabitLogKey {
                        habit: habit.id,
                        window,
                    },
                    HabitLogEntry {
                        count,
                        clock: now,
                        by: None,

                        skip: false,
                    },
                );
            }
            HabitFixture {
                habit,
                log,
                events,
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
