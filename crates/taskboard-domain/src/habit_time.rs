// SPDX-License-Identifier: MIT OR Apache-2.0
//! Habit time tracking: the additive [`HabitTimeEntry`] fact family
//! (journey plan 2026-11-24-habit-tracking-ux-plan.md D-1, O-2 additive
//! — NOT a [`crate::tracking::TimeEntry`] keying rewrite).
//!
//! **The open entry (`end: None`) IS the running timer** — the task
//! tracker's grammar verbatim. Nothing ticks: elapsed is always derived
//! at read ([`HabitTimeEntry::duration`]), and an end is stamped only by
//! a command at the acting planner's `now`. Rows live in
//! [`crate::habit::HabitState::time_log`], `serde(default)` default-
//! absent (the `day_log` additive law).
//!
//! D-2 (the plan's ruling): habit time entries never feed `log`,
//! `streak`, `adherence`, `log_window`, or any stats projection — the
//! standalone invariant proptest below pins the byte-identical
//! projections under arbitrary `time_log` mutations.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::ids::{AuthorId, HabitId, HabitTimeEntryId};

/// One habit time entry (the [`crate::tracking::TimeEntry`] shape,
/// habit-keyed, no `kind` — D-2 of the plan: no break-reminder consumer
/// exists for habits; adding a kind later is additive).
///
/// # Minting vs. leniency
///
/// The planner writes every stamp as a pure function of
/// (pre-state, command, `now`); close stamps are exempt from a strict
/// span predicate by ruling (a skewed stop stamps `end = now`
/// unconditionally — `StopHabitTracking` stays total). Decoded rows
/// (persisted state, the synced fold) are **trusted-written** — they
/// were validated at their originating device's planner — so they decode
/// as-is; the read-side projection clamps degenerate shapes to zero.
/// Deliberate leniency at the type, the `TimeEntry` ruling verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitTimeEntry {
    /// The entry's local identity (roaming family — the synced
    /// `habit-time-entries` section's wire key).
    pub id: HabitTimeEntryId,
    /// The tracked habit. The reference survives the habit's tombstone
    /// (D-4): history outlives the habit — the reference is the entry's
    /// data, not a join.
    pub habit: HabitId,
    /// The habit's title, snapshotted immutably at start (D-4). Renames
    /// and tombstones never rewrite history.
    pub habit_title: String,
    /// Span start.
    pub start: DateTime<Utc>,
    /// Span end; `None` = open = the running timer.
    pub end: Option<DateTime<Utc>>,
    /// Row creation stamp (always the minting planner's `now`, never
    /// span data).
    pub created_at: DateTime<Utc>,
    /// The LWW mutation clock (always the mutating planner's `now`).
    pub updated_at: DateTime<Utc>,
    /// The row's author (F1's `by` leg, the stored duel rank). Documented
    /// leniency: `None` means "author unknown", not "authored by
    /// nobody".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
}

impl HabitTimeEntry {
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

/// The habit-side tracking reads over [`crate::habit::HabitState`]
/// (deterministic `BTreeMap` iteration — the state.rs determinism
/// contract).
pub trait HabitTimeLog {
    /// All habit time entries, keyed by id.
    fn time_entries(&self) -> &BTreeMap<HabitTimeEntryId, HabitTimeEntry>;
    /// The derived open set — the running timer has no stored home.
    fn open_time_entries(&self) -> impl Iterator<Item = &HabitTimeEntry> {
        self.time_entries().values().filter(|entry| entry.is_open())
    }
    /// The single-open pick (D-T8's habit twin): the
    /// most-recently-started open entry, ties broken by the greater
    /// entry id.
    fn newest_open_time_entry(&self) -> Option<&HabitTimeEntry> {
        self.open_time_entries()
            .max_by_key(|entry| (entry.start, entry.id))
    }
}

impl HabitTimeLog for crate::habit::HabitState {
    fn time_entries(&self) -> &BTreeMap<HabitTimeEntryId, HabitTimeEntry> {
        &self.time_log
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::habit::HabitState;
    use crate::test_support::{
        HabitFixture, habit_fixture_strategy, proptest_config, timestamp_strategy,
    };
    use chrono::TimeZone;
    use proptest::prelude::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(raw: u128, start: i64, end: Option<i64>) -> HabitTimeEntry {
        HabitTimeEntry {
            id: HabitTimeEntryId::from(uuid::Uuid::from_u128(raw)),
            habit: HabitId::from(uuid::Uuid::from_u128(900)),
            habit_title: "habit 900".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
            by: None,
        }
    }

    #[test]
    fn habit_time_entry_serde_roundtrip() {
        let row = entry(1, 1_000, None);
        let json = serde_json::to_string(&row).unwrap();
        let back: HabitTimeEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
        assert!(
            json.contains("\"end\":null"),
            "the open row serializes its null end"
        );
    }

    #[test]
    fn old_state_without_time_log_decodes_empty() {
        // The additive default-absent law (the `day_log` precedent): a
        // state persisted before the field decodes with an empty map —
        // never a decode failure.
        let old = serde_json::json!({
            "habits": {},
            "log": {},
        });
        let decoded: HabitState = serde_json::from_value(old).unwrap();
        assert!(decoded.time_log.is_empty());
    }

    #[test]
    fn newest_open_picks_the_most_recently_started_open_entry_with_the_id_tie_break() {
        let mut state = HabitState::default();
        assert_eq!(state.newest_open_time_entry(), None);
        let early = entry(1, 1_000, None);
        let late = entry(2, 5_000, None);
        state.time_log.insert(early.id, early);
        let late_id = late.id;
        state.time_log.insert(late.id, late.clone());
        // A closed entry never enters the pick, however recent.
        let closed = entry(3, 9_000, Some(9_100));
        state.time_log.insert(closed.id, closed);
        assert_eq!(
            state.newest_open_time_entry().map(|open| open.id),
            Some(late_id)
        );
        // Equal starts: the greater entry id wins.
        let tie_high = entry(4, 5_000, None);
        let tie_high_id = tie_high.id;
        state.time_log.insert(tie_high.id, tie_high);
        assert_eq!(
            state.newest_open_time_entry().map(|open| open.id),
            Some(tie_high_id)
        );
        assert_eq!(state.open_time_entries().count(), 3);
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
        let mut inverted = entry(3, 1_000, Some(900));
        inverted.end = Some(ts(900));
        assert_eq!(inverted.duration(ts(2_000)), chrono::TimeDelta::zero());
    }

    /// The projection census the D-2 invariant guards: everything the
    /// habit engine derives that time tracking must never touch.
    fn projections(fixture: &HabitFixture) -> (u32, crate::habit::AdherenceRatio, Vec<u32>) {
        let state = fixture.state();
        let now = fixture.now;
        let day = fixture.day;
        let habit = state.habits.values().next().expect("fixture habit");
        let (counts, posture) = crate::habit::derived_views_from_log(habit, &state.log);
        let streak = crate::habit::streak(habit, &counts, &posture, now, day);
        let adherence = crate::habit::adherence(habit, &counts, &posture, now, day);
        let horizon = crate::habit::statistics_horizon(habit, &counts, now, day)
            .iter()
            .map(|date| {
                counts
                    .get(&crate::habit::WindowStart::of_period(
                        habit.target.kind(),
                        *date,
                    ))
                    .copied()
                    .unwrap_or(0)
            })
            .collect::<Vec<_>>();
        (streak, adherence, horizon)
    }

    proptest! {
        #![proptest_config(proptest_config(64))]

        /// The D-2 invariant (journey plan): adding, removing, or
        /// mutating `time_log` rows leaves every existing habit
        /// projection byte-identical — time tracking measures duration,
        /// the window vocabulary counts quota, and the two never
        /// entangle.
        #[test]
        fn time_log_mutations_leave_all_habit_projections_identical_prop(
            fixture in habit_fixture_strategy(),
            raw_ids in proptest::collection::vec(0_u128..1_000_000, 0..4),
            start in timestamp_strategy(),
            open_len in 0_i64..=3_600,
            close_secs in 0_i64..=3_600,
            mutate in 0_u8..2,
        ) {
            let state = fixture.state();
            let before = projections(&fixture);
            let mut perturbed = state.clone();
            for (index, raw) in raw_ids.iter().enumerate() {
                let row = entry(raw.wrapping_add(1_000_000 + u128::try_from(index).unwrap()), 0, None);
                let row = HabitTimeEntry {
                    start,
                    end: if mutate == 1 && index % 2 == 0 {
                        Some(start + chrono::TimeDelta::try_seconds(close_secs).unwrap())
                    } else if mutate == 1 {
                        Some(start)
                    } else {
                        None
                    },
                    updated_at: start + chrono::TimeDelta::try_seconds(open_len).unwrap(),
                    ..row
                };
                perturbed.time_log.insert(row.id, row);
            }
            // Byte-identical through the serde face: the projections'
            // values — not just their equality — are unchanged.
            let habit = perturbed.habits.values().next().expect("fixture habit");
            let (counts, posture) = crate::habit::derived_views_from_log(habit, &perturbed.log);
            let streak = crate::habit::streak(habit, &counts, &posture, fixture.now, fixture.day);
            let adherence =
                crate::habit::adherence(habit, &counts, &posture, fixture.now, fixture.day);
            let horizon = crate::habit::statistics_horizon(habit, &counts, fixture.now, fixture.day)
                .iter()
                .map(|date| counts.get(&crate::habit::WindowStart::of_period(habit.target.kind(), *date)).copied().unwrap_or(0))
                .collect::<Vec<_>>();
            prop_assert_eq!((streak, adherence, horizon), before);
        }
    }
}
