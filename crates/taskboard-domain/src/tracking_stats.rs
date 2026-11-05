// SPDX-License-Identifier: MIT OR Apache-2.0
//! The tracking-statistics projection (#623 slice A) — the pure folds
//! behind the Review view's tracking sections. Red first: the laws
//! below name the module's contract; the implementation lands in the
//! follow-up commit.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{TaskId, TimeEntryId};
    use crate::test_support::{proptest_config, time_entry_strategy, timestamp_strategy};
    use crate::tracking::{TimeEntryKind, tracked_in_window, work_in_window};
    use chrono::{DateTime, FixedOffset, NaiveTime, TimeZone, Utc};
    use proptest::prelude::*;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry_with_kind(
        raw: u128,
        start: i64,
        end: Option<i64>,
        kind: TimeEntryKind,
    ) -> crate::tracking::TimeEntry {
        crate::tracking::TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task: TaskId::from(uuid::Uuid::from_u128(900)),
            task_title: "task 900".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
            kind,
        }
    }

    fn work(raw: u128, start: i64, end: Option<i64>) -> crate::tracking::TimeEntry {
        entry_with_kind(raw, start, end, TimeEntryKind::Work)
    }

    fn respite(raw: u128, start: i64, end: Option<i64>) -> crate::tracking::TimeEntry {
        entry_with_kind(raw, start, end, TimeEntryKind::Respite)
    }

    fn secs(n: i64) -> chrono::TimeDelta {
        chrono::TimeDelta::try_seconds(n).unwrap()
    }

    /// A kind-flavored entry strategy: the house `time_entry_strategy`
    /// (valid spans) with the kind arm randomized.
    fn kind_entry_strategy() -> impl Strategy<Value = crate::tracking::TimeEntry> {
        (time_entry_strategy(), any::<bool>()).prop_map(|(mut entry, respite)| {
            if respite {
                entry.kind = TimeEntryKind::Respite;
            }
            entry
        })
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn offset_hours(h: i32) -> FixedOffset {
        FixedOffset::east_opt(h * 3600).unwrap()
    }

    fn days(n: u32) -> std::num::NonZeroU32 {
        std::num::NonZeroU32::new(n).unwrap()
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        // L-1: the exhaustive two-arm kind split tiles the whole —
        // symptom: a kind arm double-counted or dropped.
        #[test]
        fn window_split_tiles_the_tracked_total_prop(
            entries in prop::collection::vec(kind_entry_strategy(), 0..6),
            window_start in timestamp_strategy(),
            window_secs in 1_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let window_end = window_start + secs(window_secs);
            let split = window_split(&entries, (window_start, window_end), now);
            let whole = tracked_in_window(&entries, (window_start, window_end), now);
            prop_assert_eq!(split.work + split.respite, whole);
            prop_assert!(split.work >= chrono::TimeDelta::zero());
            prop_assert!(split.respite >= chrono::TimeDelta::zero());
        }

        // L-2: the work side IS work_in_window — symptom: a second
        // clipping implementation drifting from the oracle.
        #[test]
        fn window_split_work_side_matches_work_in_window_prop(
            entries in prop::collection::vec(kind_entry_strategy(), 0..6),
            window_start in timestamp_strategy(),
            window_secs in 1_i64..=86_400,
            now in timestamp_strategy(),
        ) {
            let window_end = window_start + secs(window_secs);
            let split = window_split(&entries, (window_start, window_end), now);
            prop_assert_eq!(
                split.work,
                work_in_window(&entries, (window_start, window_end), now)
            );
        }

        // L-4: each bucket equals the oracle over that bucket's own
        // window — symptom: per-bucket math drifting from
        // tracked_in_window.
        #[test]
        fn daily_series_buckets_come_from_the_single_oracle_prop(
            entries in prop::collection::vec(kind_entry_strategy(), 0..6),
            now in timestamp_strategy(),
            day_secs in 0_u32..=86_399,
        ) {
            let day_start_time = NaiveTime::from_num_seconds_from_midnight_opt(day_secs, 0).unwrap();
            let series = daily_split_series(
                &entries,
                days(3),
                day_start_time,
                offset_hours(2),
                now,
            );
            for (i, total) in series.iter().enumerate() {
                let b0 = crate::days::window_start(
                    total.date,
                    day_start_time,
                    offset_hours(2),
                );
                let b1 = crate::days::window_start(
                    total.date.succ_opt().unwrap(),
                    day_start_time,
                    offset_hours(2),
                );
                let oracle = window_split(&entries, (b0, b1), now);
                prop_assert_eq!(total.split, oracle, "bucket {i} re-derives from the oracle");
            }
        }
    }

    // L-3: the buckets tile the trailing window exactly — symptom:
    // edges computed by `now − n·24h` instead of window_start
    // composition.
    #[test]
    fn daily_series_tiles_the_trailing_window() {
        let now = ts(1_781_524_800); // 2026-06-15 12:00 UTC
        for n in [1_u32, 7, 30] {
            let series = daily_split_series(
                &[] as [&crate::tracking::TimeEntry; 0],
                days(n),
                time(0, 0),
                offset_hours(0),
                now,
            );
            let (start, end) =
                crate::days::trailing_days_window(now, time(0, 0), offset_hours(0), days(n));
            assert_eq!(
                series.len(),
                n as usize,
                "one bucket per trailing local day"
            );
            let first_edge = crate::days::window_start(series[0].date, time(0, 0), offset_hours(0));
            let last_date = series.last().unwrap().date.succ_opt().unwrap();
            let last_edge = crate::days::window_start(last_date, time(0, 0), offset_hours(0));
            assert_eq!(first_edge, start, "the first bucket opens the window");
            assert_eq!(last_edge, end, "the last bucket closes the window");
            for pair in series.windows(2) {
                assert_eq!(
                    pair[0].date.succ_opt().unwrap(),
                    pair[1].date,
                    "consecutive local dates, oldest first"
                );
            }
        }
    }

    // L-5: the headline boundary split — 23:00→01:00 counts 1h in each
    // of the two midnight buckets.
    #[test]
    fn a_span_crossing_the_day_boundary_counts_in_both_buckets() {
        // 2026-06-15 23:00 → 2026-06-16 01:00 UTC.
        let night = work(1, 1_781_564_400, Some(1_781_571_600));
        let now = ts(1_781_572_000);
        let series = daily_split_series([&night], days(2), time(0, 0), offset_hours(0), now);
        assert_eq!(series.len(), 2);
        assert_eq!(
            series[0].split.work,
            secs(3_600),
            "the first bucket holds 23:00–00:00"
        );
        assert_eq!(
            series[1].split.work,
            secs(3_600),
            "the second bucket holds 00:00–01:00"
        );
        assert_eq!(series[0].split.work + series[1].split.work, secs(7_200));
    }

    // L-6: a 05:00 day start puts a 03:00 stamp into the previous
    // local day's bucket (the night-owl rule riding window_start).
    #[test]
    fn the_day_start_time_moves_the_bucket_edges() {
        // Local (UTC here) 2026-06-15 03:00 with a 05:00 start belongs
        // to 2026-06-14's window.
        let early = work(1, 1_781_492_400, Some(1_781_492_400 + 1_800)); // 03:00–03:30
        let now = ts(1_781_495_000);
        let series = daily_split_series([&early], days(2), time(5, 0), offset_hours(0), now);
        // now (04:30 local) itself sits in 06-14's window: bucket 0
        // (06-13) is empty, bucket 1 (06-14) holds the whole span.
        assert_eq!(series[0].split.work, chrono::TimeDelta::zero());
        assert_eq!(series[1].split.work, secs(1_800));
        assert_eq!(
            series[1].date,
            chrono::NaiveDate::from_ymd_opt(2026, 6, 14).unwrap()
        );
    }

    // L-7: membership by start, durations full — both semantics side
    // by side (the two stated windows never re-implement each other).
    #[test]
    fn work_blocks_select_by_start_and_report_full_durations() {
        let now = ts(10_000);
        // Started before the window, runs deep into it.
        let straddler = work(1, 1_000, Some(9_000));
        let inside = work(2, 2_000, Some(3_000));
        let entries = [&straddler, &inside];
        let window = (ts(1_500), ts(9_500));
        let reading = work_blocks(entries, window, now);
        assert_eq!(reading.blocks, 1, "only the in-window start counts");
        assert_eq!(
            reading.total,
            secs(1_000),
            "the full duration of the counted block"
        );
        // The same window's split still shows the straddler's
        // in-window time (two stated semantics, both honest).
        let split = window_split(entries, window, now);
        assert_eq!(split.work, secs(7_500), "the straddler clips in the split");
        // An open block accrues to `now`.
        let running = work(3, 9_000, None);
        let reading = work_blocks([&running], window, now);
        assert_eq!(reading.blocks, 1);
        assert_eq!(reading.total, secs(1_000), "open accrual stops at now");
    }

    // L-8: the D-S5 limitation's name-bearing pin — Respite entries
    // never count as blocks.
    #[test]
    fn work_blocks_are_kind_marked_entries_not_focus_sessions() {
        let now = ts(10_000);
        let breaks = [
            respite(1, 1_000, Some(2_000)),
            respite(2, 3_000, Some(4_000)),
        ];
        let reading = work_blocks(&breaks, (ts(0), ts(9_000)), now);
        assert_eq!(reading.blocks, 0);
        assert_eq!(reading.total, chrono::TimeDelta::zero());
        assert_eq!(
            window_split(&breaks, (ts(0), ts(9_000)), now).respite,
            secs(2_000),
            "respite still speaks in the split"
        );
    }

    // L-9: the total floor.
    #[test]
    fn empty_entries_yield_zero_splits_and_no_blocks() {
        let empty = [] as [crate::tracking::TimeEntry; 0];
        let window = (ts(0), ts(9_000));
        let split = window_split(&empty, window, ts(9_000));
        assert_eq!(split.work, chrono::TimeDelta::zero());
        assert_eq!(split.respite, chrono::TimeDelta::zero());
        assert_eq!(work_blocks(&empty, window, ts(9_000)).blocks, 0);
        let series = daily_split_series(&empty, days(7), time(0, 0), offset_hours(0), ts(9_000));
        assert!(
            series
                .iter()
                .all(|t| t.split.work == chrono::TimeDelta::zero()
                    && t.split.respite == chrono::TimeDelta::zero())
        );
    }

    // L-10: the running day's bucket clamps at `now`, never into the
    // future.
    #[test]
    fn open_accrual_fills_the_running_day_up_to_now() {
        // An entry open since 2026-06-15 10:00 UTC; `now` is 12:00 the
        // same day.
        let running = work(1, 1_781_517_600, None);
        let now = ts(1_781_524_800);
        let series = daily_split_series([&running], days(1), time(0, 0), offset_hours(0), now);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].split.work, secs(7_200), "exactly now − start");
    }
}
