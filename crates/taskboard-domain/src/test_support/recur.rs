// SPDX-License-Identifier: MIT OR Apache-2.0

//! Strategies for the recur grammar (shared by the domain proptests and
//! later chunks' spawn/roll/dedup tests).

use chrono::{Datelike as _, NaiveDate};
use proptest::prelude::*;

use crate::recur::{
    DayOfMonth, EveryDays, EveryWeeks, MonthlyAnchor, NthWeek, OccurrenceCount, RecurBound,
    RecurPattern, RecurRule, WeekdaySet,
};

/// A bounded `NaiveDate` (2000–2100): the arithmetic uses
/// `num_days_from_ce` differences and month stepping — both saturate
/// only at [`NaiveDate`]'s extremes, so the strategy stays clear of
/// them.
/// # Panics
///
/// Only if chrono cannot represent the 2000/2100 bounds — a
/// programming error, never an input condition.
pub fn naive_date_strategy() -> impl Strategy<Value = NaiveDate> {
    let lo = NaiveDate::from_ymd_opt(2000, 1, 1)
        .expect("bound")
        .num_days_from_ce();
    let hi = NaiveDate::from_ymd_opt(2100, 12, 31)
        .expect("bound")
        .num_days_from_ce();
    (lo..=hi).prop_map(|ce| NaiveDate::from_num_days_from_ce_opt(ce).expect("in-range CE day"))
}

/// One of the seven weekdays, uniform.
pub fn weekday_strategy() -> impl Strategy<Value = chrono::Weekday> {
    (0_u32..7).prop_map(|index| match index {
        0 => chrono::Weekday::Mon,
        1 => chrono::Weekday::Tue,
        2 => chrono::Weekday::Wed,
        3 => chrono::Weekday::Thu,
        4 => chrono::Weekday::Fri,
        5 => chrono::Weekday::Sat,
        _ => chrono::Weekday::Sun,
    })
}

/// A non-empty weekday set (bitmask over Mon..Sun, never 0).
pub fn weekday_set_strategy() -> impl Strategy<Value = WeekdaySet> {
    (1_u32..=127_u32).prop_filter_map("non-empty", |mask| {
        let days: Vec<chrono::Weekday> = (0..7)
            .filter(|i| mask & (1 << i) != 0)
            .map(|i| match i {
                0 => chrono::Weekday::Mon,
                1 => chrono::Weekday::Tue,
                2 => chrono::Weekday::Wed,
                3 => chrono::Weekday::Thu,
                4 => chrono::Weekday::Fri,
                5 => chrono::Weekday::Sat,
                _ => chrono::Weekday::Sun,
            })
            .collect();
        WeekdaySet::new(days).ok()
    })
}

/// The full pattern grammar, uniform over the three arms.
/// # Panics
///
/// Only via the minted components' `expect`s on constants that are
/// in range by construction — a programming error, never an input
/// condition.
pub fn recur_pattern_strategy() -> impl Strategy<Value = RecurPattern> {
    prop_oneof![
        (1_u16..=30).prop_map(|every| RecurPattern::Daily {
            every: EveryDays::new(every).expect("nonzero"),
        }),
        (1_u16..=8, weekday_set_strategy()).prop_map(|(interval, on)| RecurPattern::Weekly {
            interval: EveryWeeks::new(interval).expect("nonzero"),
            on,
        }),
        prop_oneof![
            (1_u32..=31).prop_map(|d| MonthlyAnchor::OnDay(DayOfMonth::new(d).expect("in range"))),
            (1_u32..=5, weekday_strategy()).prop_map(|(n, wd)| MonthlyAnchor::NthWeekday(
                NthWeek::new(n).expect("in range"),
                wd,
            )),
        ]
        .prop_map(|on| RecurPattern::Monthly { on }),
    ]
}

/// A complete rule over the bounded date range; the anchor and the
/// optional `Until` bound are ordered (anchor ≤ until) so generated
/// rules are always schedulable.
/// # Panics
///
/// Only via the minted components' `expect`s on constants that are in
/// range by construction — a programming error, never an input
/// condition.
pub fn recur_rule_strategy() -> impl Strategy<Value = RecurRule> {
    (
        any::<u128>(),
        naive_date_strategy(),
        recur_pattern_strategy(),
        proptest::option::of(naive_date_strategy()),
        proptest::option::of(any::<u128>()),
    )
        .prop_filter_map(
            "anchor ≤ until",
            |(seed, anchor, pattern, until, from)| {
                if until.is_some_and(|u| u < anchor) {
                    return None;
                }
                Some(RecurRule {
                    series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(seed)),
                    anchor,
                    pattern,
                    bound: until.map_or(RecurBound::None, RecurBound::Until),
                    continued_from: from
                        .map(|seed| crate::ids::HabitId::from_uuid(uuid::Uuid::from_u128(seed))),
                })
            },
        )
}

/// An occurrence-count bound (≥ 1).
/// # Panics
///
/// Only via the mint's `expect` on an in-range constant — a
/// programming error, never an input condition.
pub fn occurrence_count_strategy() -> impl Strategy<Value = OccurrenceCount> {
    (1_u32..=1_000).prop_map(|n| OccurrenceCount::new(n).expect("nonzero"))
}
