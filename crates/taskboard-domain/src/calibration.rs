// SPDX-License-Identifier: MIT OR Apache-2.0
//! The effort-calibration projection (#214, P14): the pure estimate-vs-
//! actual report over the completed tasks of a half-open window.
//!
//! **Pure**: `&AppState` + window bounds + `now` in, the typed
//! [`CalibrationReport`] out — no IO, no clock reads, no runtime. The
//! `tracked_in_window` discipline: `now` and the window bounds arrive
//! as data; the consumer derives them per refresh pass.
//!
//! Semantics (the stage-1 plan's D-3/D-4/D-5):
//!
//! - **Membership**: live tasks (`deleted == false`) with
//!   `done ∈ [start, end)` — the window selects tasks by *completion
//!   date* and never clips their work.
//! - **Admission**: a task is a row iff it has an estimate **or** at
//!   least one `Work` entry. A completion with neither renders nowhere.
//! - **Actuals are lifetime**: Σ [`TimeEntry::duration`] over the
//!   task's `TimeEntryKind::Work` entries of *all time*, open entries
//!   accruing to `now` — the same entries the task-detail view's total
//!   shows. The kind filter is the D-11 ruling (respite counts against
//!   the work streak, never toward it).
//! - **The gap counts only compared rows** — both sides observed. A
//!   missing estimate is an *exclusion* (`skipped_unestimated`), never
//!   a zero; a missing actual likewise (`skipped_untracked`).
//! - **Known limitation (D-6)**: `Task.effort` is a current field —
//!   there is no estimate history, so a task re-estimated *after*
//!   completion is compared against its post-hoc estimate. Accepted
//!   for v1; the field is the task's estimate of record.
//!
//! The aggregate is derived by folding the same row vector the surface
//! lists (law L8), so a second tally path cannot drift from the
//! ledger.

use chrono::{DateTime, Utc};

use crate::entities::Effort;
use crate::ids::TaskId;
use crate::state::AppState;
use crate::tracking::TimeEntryKind;

/// One completed task's calibration row (the ledger's unit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationRow {
    /// The completed task.
    pub task: TaskId,
    /// The live task's current title (renames re-label history rows —
    /// the row is a projection of the task, not of the entries' title
    /// snapshots).
    pub title: String,
    /// The completion stamp that admitted the row.
    pub done: DateTime<Utc>,
    /// The task's estimate of record; `None` = unestimated.
    pub estimate: Option<Effort>,
    /// Σ `Work` durations, lifetime (never window-clipped), open
    /// entries accruing to `now`.
    pub actual: chrono::TimeDelta,
    /// ≥ 1 `Work` entry exists — the actual side is *observed*.
    pub tracked: bool,
}

impl CalibrationRow {
    /// `Some(actual − estimate)` iff both sides are observed — the
    /// settled comparison. Positive = took longer than planned.
    #[must_use]
    pub fn gap(&self) -> Option<chrono::TimeDelta> {
        let estimate = self.estimate?;
        if !self.tracked {
            return None;
        }
        let estimate_duration =
            chrono::TimeDelta::try_seconds(i64::try_from(estimate.seconds()).unwrap_or(i64::MAX))
                .unwrap_or(chrono::TimeDelta::MAX);
        Some(self.actual - estimate_duration)
    }

    /// Both sides observed — the row counts toward the aggregate gap.
    #[must_use]
    pub const fn compared(&self) -> bool {
        self.estimate.is_some() && self.tracked
    }
}

/// The window aggregate: the compared-only gap plus the visible skip
/// counts that keep the gap's denominator honest (D-5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CalibrationAggregate {
    /// Σ estimates over the compared rows.
    pub estimated_seconds: u64,
    /// Σ actuals over the compared rows.
    pub actual_seconds: u64,
    /// `actual − estimated`, signed (`> 0` = under-estimated — the
    /// planning-fallacy direction).
    pub gap_seconds: i64,
    /// Rows with both sides observed.
    pub compared: usize,
    /// Tracked completions with no estimate (excluded from the gap,
    /// never zero-counted).
    pub skipped_unestimated: usize,
    /// Estimated completions never tracked (excluded from the gap,
    /// never zero-counted).
    pub skipped_untracked: usize,
}

/// The pure projection's output: the window's rows plus the aggregate
/// folded from exactly those rows (law L8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalibrationReport {
    /// The half-open window the projection ran over (echoed data —
    /// the consumer's own bounds, round-tripped).
    pub window: (DateTime<Utc>, DateTime<Utc>),
    /// The ledger rows, sorted `done` descending (newest completion
    /// first), ties by task ascending.
    pub rows: Vec<CalibrationRow>,
    /// `reduce(rows)` by construction.
    pub aggregate: CalibrationAggregate,
}

/// The effort-calibration projection: the completed-in-`window` live
/// tasks as [`CalibrationRow`]s plus the folded aggregate. Pure — the
/// window bounds and `now` are data, never reads.
#[must_use]
pub fn calibrate(
    state: &AppState,
    window: (DateTime<Utc>, DateTime<Utc>),
    now: DateTime<Utc>,
) -> CalibrationReport {
    let (window_start, window_end) = window;
    // Membership: live ∧ `done ∈ [start, end)` (half-open). Admission
    // (D-3): an estimate or at least one Work entry — the work filter
    // is the D-11 ruling; the actual sums lifetime, open entries
    // accruing to `now` (`duration`'s clamp keeps skewed spans out).
    let mut rows: Vec<CalibrationRow> = state
        .tasks
        .values()
        .filter(|task| task.is_live())
        .filter_map(|task| {
            let done = task
                .done
                .filter(|&done| done >= window_start && done < window_end)?;
            let work: Vec<_> = state
                .tracking
                .entries
                .values()
                .filter(|entry| entry.task == task.id && entry.kind == TimeEntryKind::Work)
                .collect();
            let tracked = !work.is_empty();
            if task.effort.is_none() && !tracked {
                return None; // neither side observed — renders nowhere
            }
            let actual = work
                .iter()
                .map(|entry| entry.duration(now))
                .fold(chrono::TimeDelta::zero(), |acc, span| acc + span);
            Some(CalibrationRow {
                task: task.id,
                title: task.title.clone(),
                done,
                estimate: task.effort,
                actual,
                tracked,
            })
        })
        .collect();
    // The ledger reads newest-completion-up; ties by task ascending.
    rows.sort_by(|a, b| b.done.cmp(&a.done).then_with(|| a.task.cmp(&b.task)));
    let aggregate = fold(&rows);
    CalibrationReport {
        window,
        rows,
        aggregate,
    }
}

/// The aggregate is literally the rows' fold (law L8) — a second tally
/// path cannot drift from the ledger because this is the only one.
fn fold(rows: &[CalibrationRow]) -> CalibrationAggregate {
    let mut aggregate = CalibrationAggregate::default();
    for row in rows {
        if row.compared() {
            aggregate.compared += 1;
            aggregate.estimated_seconds += row
                .estimate
                .map_or(0, |estimate| estimate.seconds().min(i64::MAX as u64));
            aggregate.actual_seconds += u64::try_from(row.actual.num_seconds()).unwrap_or(0);
        } else if row.tracked {
            aggregate.skipped_unestimated += 1;
        } else {
            aggregate.skipped_untracked += 1;
        }
    }
    aggregate.gap_seconds = i64::try_from(aggregate.actual_seconds).unwrap_or(i64::MAX)
        - i64::try_from(aggregate.estimated_seconds).unwrap_or(i64::MAX);
    aggregate
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::TimeZone;
    use proptest::prelude::*;

    use super::*;
    use crate::entities::{Task, TaskClocks};
    use crate::ids::TimeEntryId;
    use crate::test_support::{proptest_config, timestamp_strategy};
    use crate::tracking::TimeEntry;
    use crate::tracking::TimeEntryKind as Kind;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn task(raw: u128, done: Option<DateTime<Utc>>, effort: Option<u64>) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: format!("task {raw}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order: 0,
            label_memberships: BTreeMap::default(),
            effort: effort.map(Effort::new),
            importance: None,
            urgency: None,
            recur: None,
            kind: crate::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(100),
                description: ts(100),
                duedate: ts(100),
                scheduled: ts(100),
                done: ts(100),
                position: ts(100),
                effort: ts(100),
                importance: ts(100),
                urgency: ts(100),
                recur: ts(100),
                kind: ts(100),
                archived: ts(100),
                deleted: ts(100),
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn entry(raw: u128, task: TaskId, start: i64, end: Option<i64>, kind: Kind) -> TimeEntry {
        TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task,
            task_title: "snapshotted".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
            kind,
        }
    }

    fn state_of(tasks: Vec<Task>, entries: Vec<TimeEntry>) -> AppState {
        let mut state = AppState::default();
        for t in tasks {
            state.tasks.insert(t.id, t);
        }
        for e in entries {
            state.tracking.entries.insert(e.id, e);
        }
        state
    }

    // -- the generated scenario (laws L1/L3/L4/L6/L7/L8/L9) ----------------

    /// `(raw, done_offset, effort)` — a candidate task: `done_offset`
    /// is seconds from the window's start (`None` = open), `effort` in
    /// seconds (`None` = unestimated).
    type TaskSpec = (u128, Option<i64>, Option<u64>);
    /// `(task_index, start_offset, span_secs, open, kind)`.
    type EntrySpec = (usize, i64, i64, bool, Kind);
    /// `(window_start, window_end, now, task specs, entry specs)`.
    type Scenario = (
        DateTime<Utc>,
        DateTime<Utc>,
        DateTime<Utc>,
        Vec<TaskSpec>,
        Vec<EntrySpec>,
    );

    fn kind_strategy() -> impl Strategy<Value = Kind> {
        prop::sample::select(vec![Kind::Work, Kind::Respite])
    }

    fn scenario_strategy() -> impl Strategy<Value = Scenario> {
        (
            timestamp_strategy(),
            1_i64..=30 * 86_400,
            prop::collection::vec(
                (
                    0_u128..8,
                    prop::option::of(-2 * 86_400_i64..=4 * 86_400),
                    prop::option::of(0_u64..=100_000),
                ),
                0..6,
            ),
            prop::collection::vec(
                (
                    0_usize..8,
                    -2 * 86_400_i64..=4 * 86_400,
                    0_i64..=86_400,
                    any::<bool>(),
                    kind_strategy(),
                ),
                0..8,
            ),
        )
            .prop_map(|(start, window_secs, tasks, entries)| {
                let window_end = start + chrono::TimeDelta::try_seconds(window_secs).unwrap();
                let now = window_end + chrono::TimeDelta::try_hours(1).unwrap();
                (start, window_end, now, tasks, entries)
            })
    }

    fn built_tasks(start: DateTime<Utc>, tasks: &[TaskSpec]) -> Vec<Task> {
        tasks
            .iter()
            .map(|(raw, done_off, effort)| {
                task(
                    *raw,
                    done_off.map(|off| start + chrono::TimeDelta::try_seconds(off).unwrap()),
                    *effort,
                )
            })
            .collect()
    }

    fn build(start: DateTime<Utc>, tasks: &[TaskSpec], entries: &[EntrySpec]) -> AppState {
        let built = built_tasks(start, tasks);
        let built_entries: Vec<TimeEntry> = entries
            .iter()
            .enumerate()
            .filter_map(|(k, (idx, start_off, span, open, kind))| {
                let t = built.get(*idx)?;
                Some(entry(
                    1_000 + (k as u128) * 16 + (*idx as u128),
                    t.id,
                    start.timestamp() + start_off,
                    open.then(|| start.timestamp() + start_off + span),
                    *kind,
                ))
            })
            .collect();
        state_of(built, built_entries)
    }

    /// The independently written actual oracle for one task (L1): the
    /// same filter the law names, written out by hand.
    fn oracle_actual(state: &AppState, task: TaskId, now: DateTime<Utc>) -> chrono::TimeDelta {
        state
            .tracking
            .entries
            .values()
            .filter(|entry| entry.task == task && entry.kind == Kind::Work)
            .map(|entry| entry.duration(now))
            .fold(chrono::TimeDelta::zero(), |acc, span| acc + span)
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        // L1 — single source: every row's actual equals the
        // independently written Work-filtered lifetime sum.
        #[test]
        fn l1_row_actual_matches_the_independent_oracle(
            (start, end, now, tasks, entries) in scenario_strategy(),
        ) {
            let state = build(start, &tasks, &entries);
            let report = calibrate(&state, (start, end), now);
            for row in &report.rows {
                prop_assert_eq!(row.actual, oracle_actual(&state, row.task, now));
            }
        }

        // L3 — the no-estimate exclusion: un-estimating a compared row
        // removes exactly its estimate from the sum, decrements
        // `compared`, increments `skipped_unestimated` — never folds
        // the task as zero.
        #[test]
        fn l3_unestimating_a_compared_row_excludes_it_never_zero_counts(
            (start, end, now, mut tasks, entries) in scenario_strategy(),
        ) {
            let state = build(start, &tasks, &entries);
            let report = calibrate(&state, (start, end), now);
            let Some(row) = report.rows.iter().find(|row| row.compared()) else {
                return Ok(());
            };
            let estimate = row.estimate.expect("compared rows carry estimates");
            let victim = row.task;
            // Flip the estimate side off; everything else identical.
            for spec in &mut tasks {
                if TaskId::from(uuid::Uuid::from_u128(spec.0)) == victim {
                    *spec = (spec.0, spec.1, None);
                }
            }
            let flipped = calibrate(&build(start, &tasks, &entries), (start, end), now);
            let agg = &report.aggregate;
            let flipped_agg = &flipped.aggregate;
            prop_assert_eq!(
                flipped_agg.estimated_seconds,
                agg.estimated_seconds.saturating_sub(estimate.seconds()),
                "the estimate left the sum, nothing else did"
            );
            prop_assert_eq!(flipped_agg.compared, agg.compared - 1);
            prop_assert_eq!(flipped_agg.skipped_unestimated, agg.skipped_unestimated + 1);
            // The task is still a row (tracked admits it).
            prop_assert!(
                flipped.rows.iter().any(|r| r.task == victim),
                "the unestimated completion stays in the ledger"
            );
        }

        // L4 — the no-actuals exclusion: dropping every Work entry of a
        // compared row moves it out of the gap (skipped_untracked) and
        // out of `actual_seconds` exactly, but not out of the ledger.
        #[test]
        fn l4_untracking_a_compared_row_excludes_it_from_the_gap_not_the_ledger(
            (start, end, now, tasks, entries) in scenario_strategy(),
        ) {
            let state = build(start, &tasks, &entries);
            let report = calibrate(&state, (start, end), now);
            let Some(row) = report.rows.iter().find(|row| row.compared()) else {
                return Ok(());
            };
            let victim = row.task;
            let victim_actual = row.actual;
            let state2 = {
                let mut s = state.clone();
                let victim_entries: Vec<TimeEntryId> = s
                    .tracking
                    .entries
                    .values()
                    .filter(|e| e.task == victim && e.kind == Kind::Work)
                    .map(|e| e.id)
                    .collect();
                for id in victim_entries {
                    s.tracking.entries.remove(&id);
                }
                s
            };
            let flipped = calibrate(&state2, (start, end), now);
            let agg = &report.aggregate;
            let flipped_agg = &flipped.aggregate;
            prop_assert_eq!(flipped_agg.compared, agg.compared - 1);
            prop_assert_eq!(flipped_agg.skipped_untracked, agg.skipped_untracked + 1);
            prop_assert_eq!(
                flipped_agg.actual_seconds,
                agg.actual_seconds
                    .saturating_sub(u64::try_from(victim_actual.num_seconds()).unwrap_or(0)),
                "exactly the victim's contribution left the actual sum"
            );
            prop_assert!(
                flipped.rows.iter().any(|r| r.task == victim),
                "the estimate still admits the row"
            );
            prop_assert!(!flipped.rows.iter().any(|r| r.task == victim && r.compared()));
        }

        // L6 — respite never counts: appending Respite entries of
        // arbitrary spans to any task leaves the whole report
        // identical (the D-11 ruling).
        #[test]
        fn l6_appending_respite_entries_leaves_the_report_unchanged(
            (start, end, now, tasks, entries) in scenario_strategy(),
            respites in prop::collection::vec(
                (0_usize..8, -2 * 86_400_i64..=4 * 86_400, 0_i64..=86_400, any::<bool>()),
                1..4,
            ),
        ) {
            let state = build(start, &tasks, &entries);
            let baseline = calibrate(&state, (start, end), now);
            let with_respites = {
                let mut s = state.clone();
                let all = built_tasks(start, &tasks);
                for (k, (idx, start_off, span, open)) in respites.iter().enumerate() {
                    let Some(t) = all.get(*idx) else {
                        continue;
                    };
                    let e = entry(
                        9_000 + k as u128,
                        t.id,
                        start.timestamp() + start_off,
                        open.then(|| start.timestamp() + start_off + span),
                        Kind::Respite,
                    );
                    s.tracking.entries.insert(e.id, e);
                }
                s
            };
            prop_assert_eq!(calibrate(&with_respites, (start, end), now), baseline);
        }

        // L7 — open-entry accrual passes `now` through: an open Work
        // entry makes the row's actual at least its closed part and
        // monotone non-decreasing in `now`.
        #[test]
        fn l7_open_entries_accrue_monotonically_in_now(
            (start, end, now, mut tasks, mut entries) in scenario_strategy(),
        ) {
            // Force one open Work entry on a task completing at the
            // window's midpoint (always inside the half-open window).
            let half = (end - start).num_seconds() / 2;
            tasks.push((9, Some(half), Some(60)));
            entries.push((9, 0, 10_000, true, Kind::Work));
            let state = build(start, &tasks, &entries);
            let victim = TaskId::from(uuid::Uuid::from_u128(9));
            let report = calibrate(&state, (start, end), now);
            let Some(row) = report.rows.iter().find(|r| r.task == victim) else {
                prop_assert!(false, "the forced task must be admitted (tracked)");
                return Ok(());
            };
            let later = now + chrono::TimeDelta::try_hours(2).unwrap();
            let report_later = calibrate(&state, (start, end), later);
            let row_later = report_later
                .rows
                .iter()
                .find(|r| r.task == victim)
                .expect("still admitted at the later now");
            prop_assert!(
                row.actual >= chrono::TimeDelta::zero(),
                "the open accrual is non-negative"
            );
            prop_assert!(
                row_later.actual >= row.actual,
                "the actual is monotone non-decreasing in `now` while open"
            );
        }

        // L8 — the aggregate is the fold of the rows (no second path).
        #[test]
        fn l8_aggregate_equals_the_fold_of_the_rows(
            (start, end, now, tasks, entries) in scenario_strategy(),
        ) {
            let state = build(start, &tasks, &entries);
            let report = calibrate(&state, (start, end), now);
            let mut agg = CalibrationAggregate::default();
            for row in &report.rows {
                if row.compared() {
                    agg.compared += 1;
                    agg.estimated_seconds += row
                        .estimate
                        .expect("compared carries estimate")
                        .seconds();
                    agg.actual_seconds += u64::try_from(row.actual.num_seconds()).unwrap_or(0);
                } else if row.tracked {
                    agg.skipped_unestimated += 1;
                } else {
                    agg.skipped_untracked += 1;
                }
            }
            agg.gap_seconds = i64::try_from(agg.actual_seconds).unwrap_or(i64::MAX)
                .saturating_sub(i64::try_from(agg.estimated_seconds).unwrap_or(i64::MAX));
            prop_assert_eq!(report.aggregate, agg);
        }

        // L9 — determinism: the same inputs reproduce the identical
        // report, rows sorted `done` desc then `task` asc.
        #[test]
        fn l9_the_report_is_deterministic_and_sorted(
            (start, end, now, tasks, entries) in scenario_strategy(),
        ) {
            let state = build(start, &tasks, &entries);
            let a = calibrate(&state, (start, end), now);
            let b = calibrate(&state, (start, end), now);
            prop_assert_eq!(&a, &b);
            for pair in a.rows.windows(2) {
                prop_assert!(
                    (pair[0].done, pair[0].task) > (pair[1].done, pair[1].task),
                    "newest completion first, ties by task ascending"
                );
            }
        }

        // L2 — half-open membership over generated windows.
        #[test]
        fn l2_membership_is_half_open_on_done(
            start in timestamp_strategy(),
            window_secs in 1_i64..=30 * 86_400,
            effort in 0_u64..=1_000,
        ) {
            let end = start + chrono::TimeDelta::try_seconds(window_secs).unwrap();
            let now = end + chrono::TimeDelta::try_hours(1).unwrap();
            let entries = vec![entry(1, TaskId::from(uuid::Uuid::from_u128(1)), 0, Some(10), Kind::Work)];
            let at_start = state_of(vec![task(1, Some(start), Some(effort))], entries.clone());
            prop_assert_eq!(calibrate(&at_start, (start, end), now).rows.len(), 1, "done == start admits");
            let below = state_of(
                vec![task(1, Some(start - chrono::TimeDelta::try_seconds(1).unwrap()), Some(effort))],
                entries.clone(),
            );
            prop_assert!(
                calibrate(&below, (start, end), now).rows.is_empty(),
                "done below start does not admit"
            );
            let at_end = state_of(vec![task(1, Some(end), Some(effort))], entries.clone());
            prop_assert!(
                calibrate(&at_end, (start, end), now).rows.is_empty(),
                "done == end does not admit (half-open)"
            );
            // Membership keys on `done` only: an open task with many
            // entries yields no row.
            let open = state_of(
                vec![task(1, None, Some(effort))],
                (0..4)
                    .map(|k| entry(k, TaskId::from(uuid::Uuid::from_u128(1)), 0, Some(10), Kind::Work))
                    .collect(),
            );
            prop_assert!(
                calibrate(&open, (start, end), now).rows.is_empty(),
                "an open task never surfaces, however many entries"
            );
        }

        // L5 — lifetime actuals: an entry wholly before the window
        // contributes its full duration to its in-window completion.
        #[test]
        fn l5_actuals_are_lifetime_not_window_clipped(
            start in timestamp_strategy(),
            window_secs in 1_i64..=30 * 86_400,
            span_secs in 1_i64..=86_400,
            effort in 1_u64..=1_000,
        ) {
            let end = start + chrono::TimeDelta::try_seconds(window_secs).unwrap();
            let now = end + chrono::TimeDelta::try_hours(1).unwrap();
            let done = start + chrono::TimeDelta::try_seconds(window_secs / 2).unwrap();
            // The work happened entirely before the window opened.
            let work_start = start - chrono::TimeDelta::try_seconds(span_secs).unwrap();
            let id = TaskId::from(uuid::Uuid::from_u128(1));
            let state = state_of(
                vec![task(1, Some(done), Some(effort))],
                vec![entry(1, id, work_start.timestamp(), Some(start.timestamp()), Kind::Work)],
            );
            let report = calibrate(&state, (start, end), now);
            prop_assert_eq!(report.rows.len(), 1);
            prop_assert_eq!(
                report.rows[0].actual,
                chrono::TimeDelta::try_seconds(span_secs).unwrap(),
                "the window never clips the spans"
            );
        }
    }

    // ---- Unit pins beside the laws ---------------------------------------

    #[test]
    fn a_zero_length_only_task_is_tracked_and_compared_at_zero() {
        let start = ts(1_000_000);
        let end = start + chrono::TimeDelta::try_days(7).unwrap();
        let now = end + chrono::TimeDelta::try_hours(1).unwrap();
        let id = TaskId::from(uuid::Uuid::from_u128(1));
        let state = state_of(
            vec![task(1, Some(start), Some(90))],
            vec![entry(
                1,
                id,
                start.timestamp(),
                Some(start.timestamp()),
                Kind::Work,
            )],
        );
        let report = calibrate(&state, (start, end), now);
        assert_eq!(report.rows.len(), 1);
        let row = &report.rows[0];
        assert!(row.tracked, "the user observed the work");
        assert!(row.compared(), "a zero-length observation still compares");
        assert_eq!(row.actual, chrono::TimeDelta::zero());
        assert_eq!(report.aggregate.actual_seconds, 0);
        assert_eq!(report.aggregate.gap_seconds, -90);
    }

    #[test]
    fn tombstones_never_surface() {
        let start = ts(1_000_000);
        let end = start + chrono::TimeDelta::try_days(7).unwrap();
        let now = end + chrono::TimeDelta::try_hours(1).unwrap();
        let mut deleted = task(1, Some(start), Some(90));
        deleted.deleted = true;
        let id = deleted.id;
        let state = state_of(vec![deleted], vec![entry(1, id, 0, Some(10), Kind::Work)]);
        assert_eq!(
            calibrate(&state, (start, end), now).rows,
            [] as [CalibrationRow; 0]
        );
    }

    #[test]
    fn a_task_with_neither_side_has_no_row() {
        let start = ts(1_000_000);
        let end = start + chrono::TimeDelta::try_days(7).unwrap();
        let now = end + chrono::TimeDelta::try_hours(1).unwrap();
        let state = state_of(vec![task(1, Some(start), None)], vec![]);
        let report = calibrate(&state, (start, end), now);
        assert_eq!(report.rows, [] as [CalibrationRow; 0]);
        assert_eq!(report.aggregate, CalibrationAggregate::default());
    }

    #[test]
    fn an_empty_selection_yields_the_empty_report() {
        let start = ts(1_000_000);
        let end = start + chrono::TimeDelta::try_days(7).unwrap();
        let now = end + chrono::TimeDelta::try_hours(1).unwrap();
        let report = calibrate(&AppState::default(), (start, end), now);
        assert_eq!(report.rows, [] as [CalibrationRow; 0]);
        assert_eq!(report.aggregate, CalibrationAggregate::default());
        assert_eq!(report.window, (start, end), "the bounds round-trip");
    }

    #[test]
    fn the_skips_are_counted_symmetrically() {
        let start = ts(1_000_000);
        let end = start + chrono::TimeDelta::try_days(7).unwrap();
        let now = end + chrono::TimeDelta::try_hours(1).unwrap();
        let unestimated = task(1, Some(start), None);
        let untracked = task(2, Some(start), Some(90));
        let id_unestimated = unestimated.id;
        let state = state_of(
            vec![unestimated, untracked],
            vec![entry(1, id_unestimated, 0, Some(10), Kind::Work)],
        );
        let report = calibrate(&state, (start, end), now);
        assert_eq!(report.rows.len(), 2);
        assert_eq!(
            report.aggregate,
            CalibrationAggregate {
                estimated_seconds: 0,
                actual_seconds: 0,
                gap_seconds: 0,
                compared: 0,
                skipped_unestimated: 1,
                skipped_untracked: 1,
            }
        );
    }
}
