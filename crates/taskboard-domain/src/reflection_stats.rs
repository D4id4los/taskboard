// SPDX-License-Identifier: MIT OR Apache-2.0
//! The reflection-statistics projection (#623 slice B) — the pure
//! attribute folds behind the Review view's reflection sections and
//! its past-entries ledger (the `tracking_stats` precedent: entries +
//! window bounds + `now` in, typed readings out — no IO, no clock
//! reads; `now` and every bound arrive as data).
//!
//! **Readings only**: means, occurrence counts, and an ordering. No
//! correlation, no pattern inference, no advice — the X9 guardrail;
//! anything beyond juxtaposition is fenced to a future ruling.
//!
//! **Membership rule (D-B2)**: an entry joins the window whose
//! half-open range contains the entry's **stored `window_start`**
//! instant — stored, never derived at read (the row contract). The
//! limitation recorded, not papered over: a Week/Month entry's stored
//! `window_start` is its period floor, so it stamps at the *start* of
//! its (possibly long) reviewed window, not at the journaling instant.
//! `created_at` is deliberately not the key: a device-side write
//! stamp, meaningless across future roaming.
//!
//! Tombstones never enter a fold: every reading re-checks
//! [`ReflectionEntry::is_live`] — the caller may pass any iterator.

use std::collections::BTreeMap;
use std::num::NonZeroU32;

use chrono::{DateTime, Days, FixedOffset, NaiveDate, Utc};

use crate::days::{trailing_days_window, window_start};
use crate::reflection::{HealthFlag, ReflectionEntry, ReflectionState};

/// The integer fold behind one rating field's mean: `total / count`
/// stays exact in the domain layer (no float); the presentation renders
/// `count == 0` as the unanswered dash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RatingMean {
    /// The summed ratings of the rated entries.
    pub total: u32,
    /// How many entries carried this rating.
    pub count: usize,
}

impl RatingMean {
    /// The mean as a presentation-side render aid; `None` when no
    /// entry was rated (unanswered is never a zero).
    #[must_use]
    pub fn mean(&self) -> Option<f32> {
        if self.count == 0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)] // a 1..=5 mean at f32 granularity
        Some(self.total as f32 / self.count as f32)
    }

    fn add(&mut self, rating: Option<crate::reflection::Rating>) {
        if let Some(rating) = rating {
            self.total += u32::from(rating.get());
            self.count += 1;
        }
    }
}

/// Flag occurrences over a window: an entry carrying *k* flags
/// contributes *k*. Curated flags key by variant; parked `extra_flags`
/// names count verbatim into `extra` — never merged into the curated
/// map (the decode-side tolerance rule holds at read too).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FlagOccurrences {
    /// The curated nine, by variant.
    pub curated: BTreeMap<HealthFlag, usize>,
    /// Parked unknown names, verbatim.
    pub extra: BTreeMap<String, usize>,
}

impl FlagOccurrences {
    /// Every occurrence, curated and parked alike.
    #[must_use]
    pub fn total(&self) -> usize {
        self.curated.values().sum::<usize>() + self.extra.values().sum::<usize>()
    }
}

/// One window's attribute fold over its live entries.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowRatings {
    /// How many live entries joined the window — the section's
    /// collapse predicate (zero entries), distinct from "entries that
    /// answered no rating".
    pub entries: usize,
    /// The mood fold.
    pub mood: RatingMean,
    /// The energy fold.
    pub energy: RatingMean,
    /// The flag occurrences.
    pub flags: FlagOccurrences,
}

/// Folds the live entries whose stored `window_start` lies in
/// `[window.0, window.1)` — see the module doc's membership rule.
#[must_use]
pub fn window_ratings<'a>(
    entries: impl IntoIterator<Item = &'a ReflectionEntry>,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> WindowRatings {
    let (start, end) = window;
    let mut ratings = WindowRatings::default();
    for entry in entries
        .into_iter()
        .filter(|entry| entry.is_live() && entry.window_start >= start && entry.window_start < end)
    {
        ratings.entries += 1;
        ratings.mood.add(entry.attributes.mood);
        ratings.energy.add(entry.attributes.energy);
        for flag in &entry.attributes.flags {
            *ratings.flags.curated.entry(*flag).or_default() += 1;
        }
        for name in &entry.attributes.extra_flags {
            *ratings.flags.extra.entry(name.clone()).or_default() += 1;
        }
    }
    ratings
}

/// One local day's fold (the bucket identity rides along; the push pass
/// consumes only the values).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayRatings {
    /// The bucket's local (window) date.
    pub date: NaiveDate,
    /// That day-window's fold.
    pub ratings: WindowRatings,
}

/// The trailing `days` local-day buckets, oldest first, tiling
/// [`trailing_days_window`] exactly (the `daily_split_series` shape):
/// bucket *i* spans `[window_start(dateᵢ), window_start(dateᵢ₊₁))`.
/// Per bucket: [`window_ratings`] over that bucket's own window, so the
/// membership rule is never re-implemented here.
#[must_use]
pub fn daily_rating_series<'a>(
    entries: impl IntoIterator<Item = &'a ReflectionEntry>,
    days: NonZeroU32,
    day_start_time: chrono::NaiveTime,
    offset: FixedOffset,
    now: DateTime<Utc>,
) -> Vec<DayRatings> {
    let rows: Vec<&ReflectionEntry> = entries.into_iter().collect();
    let (window_start_instant, _) = trailing_days_window(now, day_start_time, offset, days);
    let first_date = window_start_instant.with_timezone(&offset).date_naive();
    (0..days.get())
        .map(|index| {
            let date = first_date + Days::new(u64::from(index));
            let start = window_start(date, day_start_time, offset);
            let end = window_start(date + Days::new(1), day_start_time, offset);
            DayRatings {
                date,
                ratings: window_ratings(rows.iter().copied(), (start, end)),
            }
        })
        .collect()
}

/// The past-entries reading: every live entry, newest first — ordered
/// by `(window_start desc, id desc)`, the deterministic companion of
/// the map's id order. No window and no cap: the ledger is "one place
/// to look back", and the consumer virtualizes.
#[must_use]
pub fn past_entries(state: &ReflectionState) -> Vec<&ReflectionEntry> {
    let mut rows: Vec<&ReflectionEntry> = state.live_entries().collect();
    rows.sort_by(|a, b| {
        b.window_start
            .cmp(&a.window_start)
            .then_with(|| b.id.cmp(&a.id))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ReflectionId;
    use crate::reflection::{Rating, ReflectionAttributes, ReviewCadence};
    use crate::test_support::proptest_config;
    use chrono::{FixedOffset, NaiveTime, TimeZone as _};
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn time(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    fn offset_hours(h: i32) -> FixedOffset {
        FixedOffset::east_opt(h * 3600).unwrap()
    }

    fn days(n: u32) -> NonZeroU32 {
        NonZeroU32::new(n).unwrap()
    }

    fn rating(n: u8) -> Rating {
        Rating::try_new(n).unwrap()
    }

    /// A Day entry whose stored window opens at `start`.
    fn entry(raw: u128, start: i64) -> ReflectionEntry {
        ReflectionEntry {
            id: ReflectionId::from(uuid::Uuid::from_u128(raw)),
            cadence: ReviewCadence::Day,
            day: ts(start).date_naive(),
            window_start: ts(start),
            window_end: ts(start + 86_400),
            text: String::new(),
            attributes: ReflectionAttributes::default(),
            created_at: ts(start + 90_000),
            updated_at: ts(start + 90_000),
            deleted: false,
        }
    }

    fn rated(raw: u128, start: i64, mood: Option<u8>, energy: Option<u8>) -> ReflectionEntry {
        let mut row = entry(raw, start);
        row.attributes.mood = mood.map(rating);
        row.attributes.energy = energy.map(rating);
        row
    }

    fn state_of(rows: Vec<ReflectionEntry>) -> ReflectionState {
        ReflectionState {
            entries: rows.into_iter().map(|row| (row.id, row)).collect(),
        }
    }

    // 1: the half-open law at both edges.
    #[test]
    fn window_edges_attribute_to_the_right_bucket() {
        let window = (ts(1_000), ts(2_000));
        let at_start = rated(1, 1_000, Some(4), None);
        let at_end = rated(2, 2_000, Some(2), None);
        let before = rated(3, 999, Some(1), None);
        let rows = [&at_start, &at_end, &before];
        let this = window_ratings(rows, window);
        assert_eq!(this.entries, 1, "`== start` joins this window");
        assert_eq!(this.mood, RatingMean { total: 4, count: 1 });
        let next = window_ratings(rows, (ts(2_000), ts(3_000)));
        assert_eq!(next.entries, 1, "`== end` joins the NEXT window");
        assert_eq!(next.mood, RatingMean { total: 2, count: 1 });
    }

    // 2: tombstones contribute nothing anywhere.
    #[test]
    fn tombstoned_entries_never_enter_the_folds() {
        let mut dead = rated(1, 1_500, Some(5), Some(5));
        dead.attributes.flags.insert(HealthFlag::Migraine);
        dead.deleted = true;
        let fold = window_ratings([&dead], (ts(1_000), ts(2_000)));
        assert_eq!(fold, WindowRatings::default());
        let series = daily_rating_series([&dead], days(2), time(0, 0), offset_hours(0), ts(1_500));
        assert!(series.iter().all(|day| day.ratings.entries == 0));
        assert_eq!(past_entries(&state_of(vec![dead])).len(), 0);
    }

    // 3: multiple entries per window all count (D-J3's legality).
    #[test]
    fn multiple_entries_per_window_all_count() {
        let mut a = rated(1, 1_100, Some(3), Some(2));
        a.attributes.flags.insert(HealthFlag::PoorSleep);
        let mut b = rated(2, 1_200, Some(5), None);
        b.attributes.flags.insert(HealthFlag::PoorSleep);
        b.attributes.flags.insert(HealthFlag::GoodDay);
        let fold = window_ratings([&a, &b], (ts(1_000), ts(2_000)));
        assert_eq!(fold.entries, 2);
        assert_eq!(fold.mood, RatingMean { total: 8, count: 2 });
        assert_eq!(fold.energy, RatingMean { total: 2, count: 1 });
        assert_eq!(fold.flags.curated[&HealthFlag::PoorSleep], 2);
        assert_eq!(fold.flags.curated[&HealthFlag::GoodDay], 1);
        assert_eq!(fold.flags.total(), 3);
    }

    // 4: the integer fold and the unanswered mean.
    #[test]
    fn rating_means_are_exact_integer_folds() {
        let a = rated(1, 1_100, Some(2), None);
        let b = rated(2, 1_200, None, None);
        let c = rated(3, 1_300, Some(5), None);
        let fold = window_ratings([&a, &b, &c], (ts(1_000), ts(2_000)));
        assert_eq!(fold.mood, RatingMean { total: 7, count: 2 });
        assert_eq!(fold.mood.mean(), Some(3.5));
        assert_eq!(fold.energy, RatingMean { total: 0, count: 0 });
        assert_eq!(fold.energy.mean(), None, "all-unrated is unanswered");
        assert_eq!(fold.entries, 3);
    }

    // 5: D-B2's recorded limitation, pinned by name.
    #[test]
    fn period_floor_entries_stamp_at_their_window_start() {
        // A Month entry labeled by its floored first day: its stored
        // `window_start` (day 0) is weeks before `created_at`.
        let mut month = rated(1, 0, Some(4), None);
        month.cadence = ReviewCadence::Month;
        month.window_end = ts(30 * 86_400);
        month.created_at = ts(29 * 86_400);
        month.updated_at = month.created_at;
        let at_floor = window_ratings([&month], (ts(0), ts(86_400)));
        assert_eq!(
            at_floor.entries, 1,
            "joins the bucket holding the stored floor"
        );
        let at_write = window_ratings([&month], (ts(29 * 86_400), ts(30 * 86_400)));
        assert_eq!(at_write.entries, 0, "`created_at` is not the key");
    }

    // 6: k flags = k occurrences; parked names stay parked.
    #[test]
    fn flag_occurrences_count_curated_and_extra_separately() {
        let mut a = entry(1, 1_100);
        a.attributes.flags = BTreeSet::from([HealthFlag::Cramps, HealthFlag::Allergies]);
        a.attributes.extra_flags = BTreeSet::from(["solar-flare".to_owned()]);
        let mut b = entry(2, 1_200);
        b.attributes.extra_flags = BTreeSet::from(["solar-flare".to_owned()]);
        let fold = window_ratings([&a, &b], (ts(1_000), ts(2_000)));
        assert_eq!(fold.flags.curated.len(), 2);
        assert_eq!(fold.flags.curated[&HealthFlag::Cramps], 1);
        assert_eq!(fold.flags.extra["solar-flare"], 2);
        assert!(
            fold.flags
                .curated
                .keys()
                .all(|flag| flag.text_form() != "solar-flare"),
            "parked names never merge into the curated map"
        );
        assert_eq!(fold.flags.total(), 4);
    }

    // 7: the buckets tile the trailing window; a stamp migrates buckets
    // when the STAMP's `now` advances a day.
    #[test]
    fn daily_series_buckets_tile_the_window_exactly() {
        let now = ts(1_781_524_800); // 2026-06-15 12:00 UTC
        let (start, end) = trailing_days_window(now, time(0, 0), offset_hours(0), days(7));
        let today_noon = rated(1, 1_781_524_800 - 3_600, Some(3), None);
        let series = daily_rating_series([&today_noon], days(7), time(0, 0), offset_hours(0), now);
        assert_eq!(series.len(), 7);
        let first_edge = window_start(series[0].date, time(0, 0), offset_hours(0));
        let last_date = series.last().unwrap().date.succ_opt().unwrap();
        let last_edge = window_start(last_date, time(0, 0), offset_hours(0));
        assert_eq!(first_edge, start, "bucket 0 opens the window (oldest)");
        assert_eq!(last_edge, end, "the last bucket closes it");
        for pair in series.windows(2) {
            assert_eq!(pair[0].date.succ_opt().unwrap(), pair[1].date);
        }
        assert_eq!(series[6].ratings.entries, 1, "the running day holds it");
        // Advance the stamp by a day: the entry migrates one bucket back.
        let later = now + chrono::TimeDelta::try_days(1).unwrap();
        let series =
            daily_rating_series([&today_noon], days(7), time(0, 0), offset_hours(0), later);
        assert_eq!(series[5].ratings.entries, 1);
        assert_eq!(series[6].ratings.entries, 0);
    }

    // 8: newest first by (window_start desc, id desc).
    #[test]
    fn past_entries_orders_newest_first_by_window_start_then_id_desc() {
        let old_day = entry(5, 1_000);
        let mut week = entry(2, 5_000);
        week.cadence = ReviewCadence::Week;
        let tied_low = entry(3, 5_000);
        let newest = entry(1, 9_000);
        let state = state_of(vec![old_day, week, tied_low, newest]);
        let ids: Vec<u128> = past_entries(&state)
            .iter()
            .map(|row| row.id.as_uuid().as_u128())
            .collect();
        assert_eq!(ids, vec![1, 3, 2, 5]);
    }

    /// A generated entry: a stamp inside ±8 days of the fixed `now`,
    /// optional ratings, a random flag subset, a tombstone bit.
    fn entry_strategy() -> impl Strategy<Value = ReflectionEntry> {
        (
            any::<u128>(),
            -8_i64 * 86_400..=8 * 86_400,
            proptest::option::of(1_u8..=5),
            proptest::option::of(1_u8..=5),
            proptest::collection::btree_set(
                proptest::sample::select(crate::test_support::all_health_flag_names()),
                0..4,
            ),
            any::<bool>(),
        )
            .prop_map(|(raw, shift, mood, energy, flags, deleted)| {
                let mut row = rated(raw, 1_781_524_800 + shift, mood, energy);
                row.attributes.flags = flags
                    .iter()
                    .filter_map(|f| HealthFlag::parse_text_form(f))
                    .collect();
                row.deleted = deleted;
                row
            })
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        // 9: bucket membership partitions the window — every live
        // in-window entry counted exactly once across the series, and
        // the series' sum equals the whole window's fold.
        #[test]
        fn daily_series_partitions_the_window_prop(
            entries in proptest::collection::vec(entry_strategy(), 0..12),
            shift in -3_i64 * 86_400..=3 * 86_400,
            day_secs in 0_u32..=86_399,
        ) {
            let now = ts(1_781_524_800 + shift);
            let day_start_time = NaiveTime::from_num_seconds_from_midnight_opt(day_secs, 0).unwrap();
            let series = daily_rating_series(&entries, days(7), day_start_time, offset_hours(2), now);
            let window = trailing_days_window(now, day_start_time, offset_hours(2), days(7));
            let whole = window_ratings(&entries, window);
            let counted: usize = series.iter().map(|d| d.ratings.entries).sum();
            prop_assert_eq!(counted, whole.entries, "each live in-window entry exactly once");
            let mood_total: u32 = series.iter().map(|d| d.ratings.mood.total).sum();
            prop_assert_eq!(mood_total, whole.mood.total);
            let flags_total: usize = series.iter().map(|d| d.ratings.flags.total()).sum();
            prop_assert_eq!(flags_total, whole.flags.total());
            // The oracle per bucket: live ∧ stamp in the bucket.
            let live_in_window = entries
                .iter()
                .filter(|e| e.is_live() && e.window_start >= window.0 && e.window_start < window.1)
                .count();
            prop_assert_eq!(whole.entries, live_in_window);
        }
    }
}
