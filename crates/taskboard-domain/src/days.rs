// SPDX-License-Identifier: MIT OR Apache-2.0

//! Day boundaries: the user-configurable "today" window.

use chrono::{DateTime, Days, FixedOffset, NaiveDateTime, Offset, Utc};

/// The 24h window `[start, start + 24h)` containing `now`, where
/// `start` is the most recent instant whose wall-clock time-of-day in
/// the fixed UTC offset `offset` equals `day_start_time` (#345's
/// contract; D-F6).
///
/// Pure, total, deterministic: time *and* offset are data — the
/// crate's clock rule ([`crate::clock`]: nothing reads the
/// environment) applies to the offset too, so local-timezone and
/// tz-database lookups never appear here. The consumer passes its
/// *current* fixed offset; on a DST transition day the boundary's
/// wall-clock label may wobble ≤1h from the tz-database answer —
/// accepted by ruling (the instants stay unambiguous; a fixed offset
/// has no transitions, so the instant math is gap-free by
/// construction).
///
/// # Consumers contract (D-F6)
///
/// App bootstrap parses `[preferences] day_start_time` once (typed
/// config validation) and resolves the device's fixed offset from
/// the OS timezone at derivation time; consumers (UI today view, CLI
/// markers, later engine features via bootstrap injection) receive
/// `(day_start_time, offset)` explicitly and call this function.
/// Nothing reads figment mid-flight; nothing mutates
/// [`crate::AppState`] for it.
///
/// Total for every representable `now` at least one day inside
/// chrono's date range (the `Days` arithmetic saturates only at
/// [`chrono::NaiveDate`]'s extremes).
#[must_use]
pub fn day_window(
    now: DateTime<Utc>,
    day_start_time: chrono::NaiveTime,
    offset: FixedOffset,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let local_date = now.with_timezone(&offset).date_naive();
    let candidate = offset_local_to_utc(local_date.and_time(day_start_time), offset);
    let start_date = if candidate > now {
        local_date - Days::new(1)
    } else {
        local_date
    };
    let start = offset_local_to_utc(start_date.and_time(day_start_time), offset);
    let end = offset_local_to_utc((start_date + Days::new(1)).and_time(day_start_time), offset);
    (start, end)
}

/// Back-converts an offset-local wall-clock reading to its instant:
/// `utc = local − offset`, exact for every representable reading
/// because a fixed offset has no transitions, so no reading is
/// ambiguous or missing.
fn offset_local_to_utc(local: NaiveDateTime, offset: FixedOffset) -> DateTime<Utc> {
    DateTime::from_naive_utc_and_offset(local - offset.fix(), Utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{proptest_config, timestamp_strategy};
    use chrono::{NaiveTime, TimeZone};
    use proptest::prelude::*;

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn offset_hours(h: i32) -> FixedOffset {
        FixedOffset::east_opt(h * 3600).unwrap()
    }

    fn utc(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// D7a: a midnight start at `+02:00` — the window opens at that
    /// day's local midnight, an earlier UTC instant than a naive UTC
    /// reading of `now`'s date suggests. Hand-computed: local
    /// 2026-06-15 14:00 +02:00 = 12:00 UTC; the day's local midnight
    /// is 2026-06-14 22:00 UTC.
    #[test]
    fn midnight_start_opens_at_the_local_midnight() {
        let now = utc(1_781_524_800); // 2026-06-15 12:00:00 UTC
        let (start, end) = day_window(now, time(0, 0), offset_hours(2));
        assert_eq!(start, utc(1_781_474_400)); // 2026-06-14 22:00:00 UTC
        assert_eq!(end, utc(1_781_560_800)); // 2026-06-15 22:00:00 UTC
    }

    /// D7b: night-owl `"05:00"`, local 03:00 — the window opened
    /// *yesterday* at 05:00 local (the step-back rule).
    #[test]
    fn night_owl_start_rolls_back_to_yesterday() {
        // Local 2026-06-15 03:00 +02:00 = 01:00 UTC. Yesterday 05:00
        // local = 2026-06-14 03:00 UTC; end = 2026-06-15 03:00 UTC.
        let now = utc(1_781_485_200);
        let (start, end) = day_window(now, time(5, 0), offset_hours(2));
        assert_eq!(start, utc(1_781_406_000));
        assert_eq!(end, utc(1_781_492_400));
    }

    /// D7c: `now` exactly at the boundary — the new window opens at
    /// `now` (the equality branch takes the candidate).
    #[test]
    fn now_exactly_at_the_boundary_opens_the_new_window() {
        // Local 2026-06-15 05:00 +02:00 = 03:00 UTC = the boundary.
        let now = utc(1_781_492_400);
        let (start, end) = day_window(now, time(5, 0), offset_hours(2));
        assert_eq!(start, now);
        assert_eq!(end, utc(1_781_578_800)); // +24h
    }

    /// D7d: `"00:00"` at `+00:00` is `[local midnight, +24h)` in UTC.
    #[test]
    fn utc_midnight_start_is_the_plain_day() {
        let now = utc(1_781_524_800);
        let (start, end) = day_window(now, time(0, 0), offset_hours(0));
        assert_eq!(start, utc(1_781_481_600)); // 2026-06-15 00:00 UTC
        assert_eq!(end, utc(1_781_568_000)); // 2026-06-16 00:00 UTC
    }

    fn day_start_time_strategy() -> impl Strategy<Value = NaiveTime> {
        (0_u32..=86_399).prop_map(|s| NaiveTime::from_num_seconds_from_midnight_opt(s, 0).unwrap())
    }

    fn offset_strategy() -> impl Strategy<Value = FixedOffset> {
        (-86_399_i32..=86_399).prop_map(|s| FixedOffset::east_opt(s).unwrap())
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn window_contains_now(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let (start, end) = day_window(now, day_start_time, offset);
            prop_assert!(start <= now && now < end);
        }

        #[test]
        fn window_is_exactly_24_hours(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let (start, end) = day_window(now, day_start_time, offset);
            prop_assert_eq!(end - start, chrono::TimeDelta::try_hours(24).unwrap());
        }

        #[test]
        fn stepping_now_across_the_boundary_flips_the_window(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            // Independently construct today's offset-local boundary
            // instant `B` and sample both flanks of the flip.
            let local_date = now.with_timezone(&offset).date_naive();
            let boundary = offset
                .from_local_datetime(&local_date.and_time(day_start_time))
                .single()
                .unwrap();
            let boundary_utc: DateTime<Utc> = boundary.into();
            let (start_at, _) = day_window(boundary_utc, day_start_time, offset);
            prop_assert_eq!(start_at, boundary_utc, "at B the new window opens");

            let one_sec = chrono::TimeDelta::try_seconds(1).unwrap();
            let day = chrono::TimeDelta::try_hours(24).unwrap();
            let (start_before, _) =
                day_window((boundary - one_sec).into(), day_start_time, offset);
            prop_assert_eq!(start_before, boundary_utc - day, "before B yesterday's window");
            let (start_after, _) =
                day_window((boundary + one_sec).into(), day_start_time, offset);
            prop_assert_eq!(start_after, boundary_utc, "after B the new window");
        }

        #[test]
        fn one_day_later_the_window_advances_one_day(
            now in timestamp_strategy(),
            day_start_time in day_start_time_strategy(),
            offset in offset_strategy(),
        ) {
            let day = chrono::TimeDelta::try_hours(24).unwrap();
            let (start, end) = day_window(now, day_start_time, offset);
            let (start_later, end_later) =
                day_window(now + day, day_start_time, offset);
            prop_assert_eq!(start_later, start + day);
            prop_assert_eq!(end_later, end + day);
        }
    }
}
