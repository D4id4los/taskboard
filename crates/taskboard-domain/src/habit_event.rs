// SPDX-License-Identifier: MIT OR Apache-2.0
//! The habit event log (#1519): one event per completion or skip
//! intent — the future sole source of truth for habit statistics.
//!
//! This module holds the *model* only. The read-time derivations
//! ([`crate::habit`]'s `log_window` remains the attribution oracle
//! until the legacy maps die in the dispatch's final chunk) re-key
//! onto the fold functions added alongside these types. Nothing here
//! mints, arbitrates, or persists: minting goes through the
//! [`crate::idgen::IdGenerator`] seam; the sync fold and the sqlite
//! rows are later chunks' transport work.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

use std::collections::BTreeMap;

use crate::days::DayCalendar;
use crate::habit::{Habit, WindowStart};
use crate::ids::{AuthorId, HabitId};

/// Identity of one completion or one skip-intent mark. Minted at log
/// time via the id seam — never derived from the completion instant,
/// so same-second completions stay distinct individuals.
///
/// # Minting vs. leniency
///
/// Production mints only through
/// [`crate::idgen::IdGenerator::new_habit_event_id`]; [`From<Uuid>`]
/// is the *trust-boundary* constructor (pub(crate)): the wire/storage
/// decode and the migration's deterministic synthetic ids (a later
/// chunk) mint legitimate ids that never passed through the
/// generator. Deliberate leniency at the type: `From<Uuid>` accepts
/// any uuid version — synthetic non-v7 ids are valid members, so a
/// version check here would be validate-and-forget against a future
/// fact family. Text-input paths go through [`HabitEventId::parse`],
/// which is strict (`AuthorId::parse`'s shape).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HabitEventId(Uuid);

impl HabitEventId {
    /// Strict UUID parsing — the decode path for text forms. `None`
    /// on any non-UUID input.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        Uuid::parse_str(raw).ok().map(Self)
    }

    /// The underlying UUID.
    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// The trusted-internal constructor from an already-typed UUID (the
/// id seam's mint, the wire/storage decode, deterministic migration
/// synthesis). Never a text-input path — text goes through
/// [`HabitEventId::parse`].
impl From<Uuid> for HabitEventId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for HabitEventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `Uuid`'s `Display` is already the lowercase hyphenated form.
        self.0.fmt(f)
    }
}

impl Serialize for HabitEventId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HabitEventId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).ok_or_else(|| serde::de::Error::custom("an event id must be a UUID"))
    }
}

/// What kind of fact the event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HabitEventKind {
    /// One completed quota unit. Carries no window: the window is
    /// derived at read time (the whole point of the event-log design —
    /// a config change re-derives history instead of stranding rows).
    Completion,
    /// A deliberate planned-skip mark for one derived window.
    ///
    /// Deliberate leniency at the field: the stored `window` is the
    /// *mint-time* derivation (sync provenance, the fold's wire
    /// shape), so it may name a window of a former target kind after
    /// a config change. The read-time derivation never reads it —
    /// marks re-attribute under the *current* config like completions
    /// — so the stale value is honest provenance, not corruption, and
    /// decode keeps it as-is (the [`WindowStart`] decode posture).
    Skip {
        /// The window the mark was minted for (provenance only).
        window: WindowStart,
    },
    /// Withdrawal of the window's skip mark.
    ///
    /// Leniency: see [`HabitEventKind::Skip`] — same provenance-only
    /// stored window, never read by the derivation.
    Unskip {
        /// The window the mark was minted for (provenance only).
        window: WindowStart,
    },
}

/// The per-event deletion tombstone (Ruling C2): a correction is an
/// appended fact, never a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEventTombstone {
    /// When the deletion happened.
    pub at: DateTime<Utc>,
    /// Who deleted it. Deliberate leniency: `None` means "author
    /// unknown" (rows that predate the author slot) and folds under
    /// the `None < Some` rank — never "authored by nobody" (the
    /// `HabitLogEntry::by` posture verbatim).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
}

/// One habit fact: a completion or a skip-intent mark, plus its
/// deletion tombstone. Small and pure — the whole struct is `Copy`,
/// so the fold passes events by value freely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HabitEvent {
    /// The event's identity (roaming family — the synced section's
    /// wire key).
    pub id: HabitEventId,
    /// The counted habit.
    pub habit: HabitId,
    /// What happened.
    pub kind: HabitEventKind,
    /// The completion / mark instant — the event's own fact. No clock
    /// inflation: the LWW `mint_log_clock` bump (#986) is row-map
    /// machinery; the derivation fold arbitrates nothing and never
    /// reads a clock.
    ///
    /// Deliberate leniency: a tombstone `at` later than `at` itself
    /// is legitimate — corrections happen after the fact.
    pub at: DateTime<Utc>,
    /// Display-only author (the F1 `by` leg survives for display,
    /// never for arbitration). Deliberate leniency: absent means
    /// "author unknown", the `None < Some` posture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<AuthorId>,
    /// Deletion tombstone (Ruling C2): default-absent; the newest
    /// `(at, by)` tombstone wins in the sync fold (a later chunk).
    /// Absent on every event the local write path mints or loads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<HabitEventTombstone>,
}

impl HabitEvent {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        self.deleted.is_none()
    }
}

/// The liveness/membership re-check (no validate-and-forget): the
/// fold re-checks each event itself rather than trusting a
/// pre-filtered input — the `habit_log_for` posture. An event
/// participates in a derivation only if it is live, belongs to
/// `habit`, and carries the kind the fold wants.
fn participates(event: &HabitEvent, habit: &Habit, want_mark: bool) -> bool {
    if event.habit != habit.id || !event.is_live() {
        return false;
    }
    let is_mark = !matches!(event.kind, HabitEventKind::Completion);
    is_mark == want_mark
}

/// The shared derivation fold (the event-log design §2.1/§2.3): every
/// live event of `habit` — completions *and* skip marks — processed
/// in `(at, id)` lexicographic order (the input iterator's order is
/// irrelevant; the fold sorts internally).
///
/// - a completion attributes to the window [`crate::habit`]'s
///   attribution rule picks for its `at` under the *running* counts,
///   and increments that window's count;
/// - a mark attributes the same way (it participates in attribution,
///   never in counts — the surplus-repair guard is count-dependent,
///   so a mark's window is only well-defined relative to the
///   completions logged before it) and records `(at, id, is_skip)`
///   in its window's mark history.
///
/// The recursion resolves exactly as the legacy write path did:
/// `log_window` at log time also read the *stored* counts; the fold
/// reproduces that order online. The stored `window` field on
/// `Skip`/`Unskip` is mint-time provenance (sync data) and is never
/// read here — marks re-derive under the current config exactly like
/// completions do.
struct SharedFold {
    counts: BTreeMap<WindowStart, u32>,
    marks: BTreeMap<WindowStart, BTreeMap<(DateTime<Utc>, HabitEventId), bool>>,
}

fn shared_fold<'a>(
    habit: &Habit,
    events: impl Iterator<Item = &'a HabitEvent>,
    day: DayCalendar,
) -> SharedFold {
    let mut live: Vec<&HabitEvent> = events
        .filter(|event| participates(event, habit, false) || participates(event, habit, true))
        .collect();
    live.sort_by_key(|event| (event.at, event.id));

    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let mut fold = SharedFold {
        counts: BTreeMap::new(),
        marks: BTreeMap::new(),
    };
    for event in live {
        let window = crate::habit::attribute_window(
            kind,
            quota,
            |w| fold.counts.get(&w).copied().unwrap_or(0),
            event.at,
            day,
        );
        match event.kind {
            HabitEventKind::Completion => {
                *fold.counts.entry(window).or_default() += 1;
            }
            HabitEventKind::Skip { .. } => {
                fold.marks
                    .entry(window)
                    .or_default()
                    .insert((event.at, event.id), true);
            }
            HabitEventKind::Unskip { .. } => {
                fold.marks
                    .entry(window)
                    .or_default()
                    .insert((event.at, event.id), false);
            }
        }
    }
    fold
}

/// The derived per-window completion counts (#1519): the event-log
/// successor of the stored `HabitState.log` rows. Pure and total over
/// the event set + the habit's *current* config — a kind or day-start
/// change re-derives all history by re-running the fold. Only
/// windows with count >= 1 appear (count-0 rows do not exist in the
/// event grammar).
#[must_use]
pub fn derive_window_counts<'a>(
    habit: &Habit,
    events: impl Iterator<Item = &'a HabitEvent>,
    day: DayCalendar,
) -> BTreeMap<WindowStart, u32> {
    shared_fold(habit, events, day).counts
}

/// The derived per-window skip posture (#1519, Ruling B): `true` =
/// the window is skipped. Per window, the newest `Skip` and the
/// newest `Unskip` compare under `(at, id)` lexicographic order and
/// the greater kind wins; a window with only skips is skipped, one
/// with only unskips is unskipped. Absent key = unskipped (the
/// `HabitLogEntry::skip == false` read shape, so the streak walk's
/// re-keying stays mechanical).
#[must_use]
pub fn derive_skip_posture<'a>(
    habit: &Habit,
    events: impl Iterator<Item = &'a HabitEvent>,
    day: DayCalendar,
) -> BTreeMap<WindowStart, bool> {
    let marks = shared_fold(habit, events, day).marks;
    marks
        .into_iter()
        .map(|(window, history)| {
            // The newest key is the max of the (at, id) map — the
            // Ruling B law; an empty history cannot occur (a mark
            // always inserts).
            let is_skip = match history.values().next_back() {
                Some(&skip) => skip,
                None => false,
            };
            (window, is_skip)
        })
        .collect()
}

/// The derived per-calendar-day completion counts (#1519 §2.2, the
/// heatmap row): every live completion attributes to the plain
/// calendar day of its instant under the device's day calendar —
/// `day.today(event.at)`. No grace, no surplus-repair: the day
/// overlay is display data (R-2), and order cannot affect the result
/// (a pure per-event map).
#[must_use]
pub fn derive_day_counts<'a>(
    habit: &Habit,
    events: impl Iterator<Item = &'a HabitEvent>,
    day: DayCalendar,
) -> BTreeMap<chrono::NaiveDate, u32> {
    let mut counts = BTreeMap::new();
    for event in events.filter(|event| participates(event, habit, false)) {
        *counts.entry(day.today(event.at)).or_default() += 1;
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn stamp(raw: u128) -> Uuid {
        Uuid::from_u128(raw)
    }

    fn event(raw: u128) -> HabitEvent {
        HabitEvent {
            id: HabitEventId::from(stamp(raw)),
            habit: HabitId::from(stamp(42)),
            kind: HabitEventKind::Completion,
            at: chrono::Utc
                .timestamp_opt(1_700_000_000, 0)
                .single()
                .expect("valid instant"),
            by: None,
            deleted: None,
        }
    }

    #[test]
    fn event_round_trips_with_default_absent_optional_fields() {
        let original = event(1);
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(decoded.by.is_none());
        assert!(decoded.deleted.is_none());
    }

    #[test]
    fn event_json_omits_absent_optional_fields() {
        let json = serde_json::to_value(event(2)).expect("serializes");
        // Structural fact, not text: the absent Option legs have no key.
        assert!(json.get("by").is_none());
        assert!(json.get("deleted").is_none());
    }

    #[test]
    fn tombstoned_event_round_trips() {
        let mut original = event(3);
        original.deleted = Some(HabitEventTombstone {
            at: chrono::Utc
                .timestamp_opt(1_700_000_500, 0)
                .single()
                .expect("valid instant"),
            by: Some(AuthorId::from_uuid(stamp(7))),
        });
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(!decoded.is_live());
    }

    #[test]
    fn tombstone_decodes_with_absent_author() {
        let tombstoned = serde_json::json!({
            "id": stamp(11).to_string(),
            "habit": stamp(42).to_string(),
            "kind": { "kind": "completion" },
            "at": "2023-11-14T22:13:20Z",
            "deleted": { "at": "2023-11-14T23:13:20Z" },
        });
        let decoded: HabitEvent =
            serde_json::from_value(tombstoned).expect("decodes with default legs");
        let tombstone = decoded.deleted.expect("tombstone present");
        assert!(tombstone.by.is_none());
    }

    #[test]
    fn skip_mark_round_trips_with_stored_window() {
        let window = WindowStart::of_period(
            crate::habit::PeriodKind::Week,
            chrono::NaiveDate::from_ymd_opt(2026, 11, 23).expect("valid date"),
        );
        let mut original = event(4);
        original.kind = HabitEventKind::Skip { window };
        let json = serde_json::to_string(&original).expect("serializes");
        let decoded: HabitEvent = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, original);
        assert!(matches!(decoded.kind, HabitEventKind::Skip { .. }));
    }

    #[test]
    fn unparsable_event_id_decode_fails_and_parse_returns_none() {
        let json = serde_json::json!({
            "id": "not-a-uuid",
            "habit": stamp(42).to_string(),
            "kind": { "kind": "completion" },
            "at": "2023-11-14T22:13:20Z",
        });
        let decoded: Result<HabitEvent, _> = serde_json::from_value(json);
        assert!(decoded.is_err());
        assert!(HabitEventId::parse("not-a-uuid").is_none());
    }

    #[test]
    fn event_id_serde_is_the_uuid_string_form() {
        let id = HabitEventId::from(stamp(5));
        let json = serde_json::to_string(&id).expect("serializes");
        let round: HabitEventId = serde_json::from_str(&json).expect("decodes");
        assert_eq!(round, id);
    }
}

#[cfg(test)]
mod derivation_tests {
    use super::*;
    use crate::days::window_start;
    use crate::habit::{HabitTarget, NonZeroCount, PeriodKind};
    use crate::test_support::{habit_strategy, proptest_config};
    use chrono::{Datelike, NaiveDate, NaiveTime};
    use proptest::prelude::*;

    /// A UTC-or-shifted fixed-offset day calendar over the plan's
    /// bounded day-start/offset variants (testing §6.1: instants are
    /// fixture data — nothing reads the wall clock).
    fn day_calendar_strategy() -> impl Strategy<Value = DayCalendar> {
        (
            prop::sample::select(vec![0_u32, 3 * 3600, 9 * 3600]),
            prop::sample::select(vec![-18_000_i32, 0, 18_000]),
        )
            .prop_map(|(secs, off)| DayCalendar {
                day_start_time: NaiveTime::from_num_seconds_from_midnight_opt(secs, 0)
                    .expect("in-range seconds"),
                offset: chrono::FixedOffset::east_opt(off).expect("in-range offset"),
            })
    }

    /// A live habit over the plan's bounded config domain (quotas
    /// 1..=5; kinds Day/Week/Month).
    fn bounded_habit_strategy() -> impl Strategy<Value = Habit> {
        (
            habit_strategy(),
            prop::sample::select(vec![1_u32, 2, 3, 5]),
            period_kind(),
        )
            .prop_map(|(mut habit, quota, kind)| {
                habit.deleted = false;
                habit.target = match kind {
                    PeriodKind::Day => {
                        HabitTarget::PerDay(NonZeroCount::new(quota).expect("nonzero"))
                    }
                    PeriodKind::Week => {
                        HabitTarget::PerWeek(NonZeroCount::new(quota).expect("nonzero"))
                    }
                    PeriodKind::Month => {
                        HabitTarget::PerMonth(NonZeroCount::new(quota).expect("nonzero"))
                    }
                };
                habit
            })
    }

    fn period_kind() -> impl Strategy<Value = PeriodKind> {
        prop_oneof![
            Just(PeriodKind::Day),
            Just(PeriodKind::Week),
            Just(PeriodKind::Month),
        ]
    }

    /// A deterministic event-id source for fixtures: distinct,
    /// increasing ids (the id seam's shape, now-as-data).
    struct Ids(u128);
    impl Ids {
        fn next(&mut self) -> HabitEventId {
            self.0 += 1;
            HabitEventId::from(Uuid::from_u128(self.0))
        }
    }

    /// Completion sequences over the plan's bounded domain: instants
    /// within a seeded day ± 40 h (crossing window and grace
    /// boundaries), non-decreasing but sometimes equal (equal
    /// instants are the tie-law's subject), distinct monotone ids.
    fn completion_sequence_strategy() -> impl Strategy<Value = Vec<HabitEvent>> {
        (
            naive_date_bounded(),
            prop::collection::vec(-144_000_i64..=144_000, 0..40),
            day_calendar_strategy(),
        )
            .prop_map(|(anchor, offsets, day)| {
                let base = window_start(anchor, day.day_start_time, day.offset);
                let mut ids = Ids(1_000);
                let mut sorted = offsets;
                sorted.sort_unstable();
                sorted
                    .into_iter()
                    .map(|off| HabitEvent {
                        id: ids.next(),
                        habit: HabitId::from(Uuid::from_u128(42)),
                        kind: HabitEventKind::Completion,
                        at: base + chrono::TimeDelta::try_seconds(off).expect("in-range"),
                        by: None,
                        deleted: None,
                    })
                    .collect()
            })
    }

    fn naive_date_bounded() -> impl Strategy<Value = NaiveDate> {
        let lo = NaiveDate::from_ymd_opt(2025, 1, 1)
            .expect("bound")
            .num_days_from_ce();
        let hi = NaiveDate::from_ymd_opt(2026, 12, 31)
            .expect("bound")
            .num_days_from_ce();
        (lo..=hi).prop_map(|ce| NaiveDate::from_num_days_from_ce_opt(ce).expect("in-range"))
    }

    fn habit_for(events: &[HabitEvent], habit: &Habit) -> Vec<HabitEvent> {
        events
            .iter()
            .map(|event| HabitEvent {
                habit: habit.id,
                ..*event
            })
            .collect()
    }

    /// A deterministic pseudo-random permutation: stable sort by a
    /// seeded hash key (the shuffle-invariance properties).
    fn permute(seed: u64, items: &[HabitEvent]) -> Vec<HabitEvent> {
        let mut keyed: Vec<(u64, HabitEvent)> = items
            .iter()
            .enumerate()
            .map(|(i, event)| {
                let mut h = seed ^ (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
                h ^= h >> 30;
                h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h ^= h >> 27;
                (h, *event)
            })
            .collect();
        keyed.sort_by_key(|(k, _)| *k);
        keyed.into_iter().map(|(_, event)| event).collect()
    }

    fn completions_only(events: &[HabitEvent]) -> Vec<HabitEvent> {
        events
            .iter()
            .filter(|e| matches!(e.kind, HabitEventKind::Completion))
            .copied()
            .collect()
    }

    proptest! {
        #![proptest_config(proptest_config(64))]

        /// The equivalence oracle (T1): the fold reproduces the
        /// legacy log-time attribution exactly — the oracle is the
        /// untouched `log_window` driven with the legacy map shape,
        /// so the test fails if either leg changed the rule.
        #[test]
        fn derive_window_counts_matches_legacy_log_time_attribution(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
        ) {
            let events = habit_for(&events, &habit);
            // Simulate legacy logging: for each event in (at, id)
            // order, attribute via log_window over the stored map and
            // upsert count+1 (the pre-event-log write semantics;
            // clocks/by never influence attribution).
            let mut legacy: std::collections::BTreeMap<
                crate::habit::HabitLogKey,
                crate::habit::HabitLogEntry,
            > = std::collections::BTreeMap::new();
            let mut ordered = events.clone();
            ordered.sort_by_key(|e| (e.at, e.id));
            for event in &ordered {
                let window = crate::habit::log_window(&habit, &legacy, event.at, day);
                let key = crate::habit::HabitLogKey { habit: habit.id, window };
                let entry = legacy.entry(key).or_insert_with(|| crate::habit::HabitLogEntry {
                    count: 0,
                    clock: event.at,
                    by: None,
                    skip: false,
                });
                entry.count += 1;
            }
            let expected: BTreeMap<WindowStart, u32> = legacy
                .into_iter()
                .filter(|(key, entry)| key.habit == habit.id && entry.count > 0)
                .map(|(key, entry)| (key.window, entry.count))
                .collect();

            let derived = derive_window_counts(&habit, events.iter(), day);
            prop_assert_eq!(derived, expected);
        }

        /// Totality/conservation (T2, W-7 us1_s3): every live
        /// completion lands in exactly one window, and every derived
        /// key is a canonical window of the current kind.
        #[test]
        fn us1_s3_derived_read_is_total(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
        ) {
            let events = habit_for(&events, &habit);
            let live = u32::try_from(completions_only(&events).len()).expect("strategy caps at 39");
            let derived = derive_window_counts(&habit, events.iter(), day);
            prop_assert_eq!(derived.values().copied().sum::<u32>(), live);
            for key in derived.keys() {
                prop_assert_eq!(
                    WindowStart::of_period(habit.target.kind(), key.as_date()),
                    *key
                );
            }
        }

        /// The fold sorts internally (T4): any deterministic
        /// permutation of the same event set derives identically.
        #[test]
        fn shuffled_input_order_yields_identical_derivation(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
            seed in any::<u64>(),
        ) {
            let events = habit_for(&events, &habit);
            let reference = derive_window_counts(&habit, events.iter(), day);
            let shuffled = derive_window_counts(&habit, permute(seed, &events).iter(), day);
            prop_assert_eq!(shuffled, reference);
            let posture_reference = derive_skip_posture(&habit, events.iter(), day);
            let posture_shuffled = derive_skip_posture(&habit, permute(seed, &events).iter(), day);
            prop_assert_eq!(posture_shuffled, posture_reference);
        }

        /// The max law's property leg (T5): the posture is idempotent
        /// under re-fold and permutation-invariant.
        #[test]
        fn derive_skip_posture_refolds_and_permutes_identically(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
            seed in any::<u64>(),
        ) {
            let events = habit_for(&events, &habit);
            let reference = derive_skip_posture(&habit, events.iter(), day);
            let shuffled = derive_skip_posture(&habit, permute(seed, &events).iter(), day);
            prop_assert_eq!(shuffled, reference);
        }

        /// The day law (T9): every live completion lands on the
        /// plain calendar day of its instant under the device's day
        /// calendar; the sum conserves.
        #[test]
        fn derive_day_counts_attributes_calendar_days(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
        ) {
            let events = habit_for(&events, &habit);
            let derived = derive_day_counts(&habit, events.iter(), day);
            let live = completions_only(&events);
            prop_assert_eq!(derived.values().copied().sum::<u32>(), u32::try_from(live.len()).expect("strategy caps at 39"));
            let mut expected = BTreeMap::new();
            for event in &live {
                *expected.entry(day.today(event.at)).or_default() += 1;
            }
            prop_assert_eq!(derived, expected);
        }

        /// The day overlay's oracle leg (T10): on the correspondence
        /// sets (at == log-now) the derived day map equals the legacy
        /// overlay counts — one increment per completion at
        /// day.today(log-now), keyed in the legacy HabitDayKey shape.
        #[test]
        fn derive_day_counts_match_legacy_overlay_on_correspondence_sets(
            habit in bounded_habit_strategy(),
            events in completion_sequence_strategy(),
            day in day_calendar_strategy(),
        ) {
            let events = habit_for(&events, &habit);
            let mut legacy: BTreeMap<crate::habit::HabitDayKey, u32> = BTreeMap::new();
            for event in completions_only(&events) {
                let key = crate::habit::HabitDayKey { habit: habit.id, day: day.today(event.at) };
                *legacy.entry(key).or_default() += 1;
            }
            let derived = derive_day_counts(&habit, events.iter(), day);
            let legacy_by_day: BTreeMap<chrono::NaiveDate, u32> =
                legacy.into_iter().map(|(key, count)| (key.day, count)).collect();
            prop_assert_eq!(derived, legacy_by_day);
        }
    }

    fn utc_cal() -> DayCalendar {
        DayCalendar {
            day_start_time: NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
        }
    }

    fn day_habit(raw: u128, quota: u32) -> Habit {
        Habit {
            id: HabitId::from(Uuid::from_u128(raw)),
            board: crate::ids::BoardId::from(Uuid::from_u128(900)),
            title: "fixture".to_string(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(quota).expect("nonzero")),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: crate::habit::HabitClocks::all(
                chrono::TimeZone::timestamp_opt(&chrono::Utc, 100, 0)
                    .single()
                    .expect("valid instant"),
            ),
            continued_from: None,
            deleted: false,
            by: None,
            paused_until: None,
        }
    }

    /// T3: two same-instant completions with distinct ids stay
    /// distinct individuals — the window count is 2 — and the
    /// insertion order never changes the derived map.
    #[test]
    fn same_second_completions_stay_distinct() {
        let habit = day_habit(42, 5);
        let day = utc_cal();
        let at = window_start(
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("valid date"),
            day.day_start_time,
            day.offset,
        );
        let make = |id: u128| HabitEvent {
            id: HabitEventId::from(Uuid::from_u128(id)),
            habit: habit.id,
            kind: HabitEventKind::Completion,
            at,
            by: None,
            deleted: None,
        };
        let first = [make(1), make(2)];
        let swapped = [make(2), make(1)];
        let derived = derive_window_counts(&habit, first.iter(), day);
        let swapped_derived = derive_window_counts(&habit, swapped.iter(), day);
        assert_eq!(derived, swapped_derived);
        assert_eq!(derived.values().copied().sum::<u32>(), 2);
        assert_eq!(derived.len(), 1);
    }

    fn with_mark(
        base: &mut Vec<HabitEvent>,
        habit: &Habit,
        id: u128,
        at: chrono::DateTime<Utc>,
        kind: HabitEventKind,
    ) {
        base.push(HabitEvent {
            id: HabitEventId::from(Uuid::from_u128(id)),
            habit: habit.id,
            kind,
            at,
            by: None,
            deleted: None,
        });
    }

    const FIXTURE_DAY: chrono::NaiveDate = match NaiveDate::from_ymd_opt(2026, 11, 27) {
        Some(date) => date,
        None => panic!("valid date"),
    };

    fn skip_at(habit: &Habit, _at: chrono::DateTime<Utc>) -> HabitEventKind {
        // The stored window is mint-time provenance; the fold never
        // reads it, so the fixture's value only needs to be a date.
        HabitEventKind::Skip {
            window: WindowStart::of_period(habit.target.kind(), FIXTURE_DAY),
        }
    }

    fn unskip_at(habit: &Habit, _at: chrono::DateTime<Utc>) -> HabitEventKind {
        HabitEventKind::Unskip {
            window: WindowStart::of_period(habit.target.kind(), FIXTURE_DAY),
        }
    }

    /// T5, unit leg: the (at, id) max law. Equal instants break the
    /// tie on the id — the greater id wins regardless of insertion
    /// order.
    #[test]
    fn derive_skip_posture_newest_at_id_wins() {
        let habit = day_habit(42, 2);
        let day = utc_cal();
        let at = window_start(
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("valid date"),
            day.day_start_time,
            day.offset,
        );

        // Skip first, Unskip second with the greater id at the same
        // instant → unskipped.
        let mut events = Vec::new();
        with_mark(&mut events, &habit, 1, at, skip_at(&habit, at));
        with_mark(&mut events, &habit, 2, at, unskip_at(&habit, at));
        let posture = derive_skip_posture(&habit, events.iter(), day);
        assert_eq!(posture.get(&skipped_window(&habit, at)), Some(&false));

        // Same instants, Unskip minted "first" in insertion order but
        // Skip carries the greater id → skipped.
        let mut events = Vec::new();
        with_mark(&mut events, &habit, 2, at, unskip_at(&habit, at));
        with_mark(&mut events, &habit, 3, at, skip_at(&habit, at));
        let posture = derive_skip_posture(&habit, events.iter(), day);
        assert_eq!(posture.get(&skipped_window(&habit, at)), Some(&true));

        // No marks → no key at all (absent = unskipped).
        let posture = derive_skip_posture(&habit, std::iter::empty(), day);
        assert!(posture.is_empty());
    }

    fn skipped_window(habit: &Habit, at: chrono::DateTime<Utc>) -> WindowStart {
        let day = utc_cal();
        WindowStart::of_period(habit.target.kind(), day.today(at))
    }

    /// T6: a tombstoned completion contributes to neither window nor
    /// day counts; a tombstoned mark contributes to no posture
    /// (Ruling C2 posture — nothing writes tombstones until the
    /// correction chunk; these are decode-shaped fixtures).
    #[test]
    fn tombstoned_events_are_excluded_from_every_derivation() {
        let habit = day_habit(42, 3);
        let day = utc_cal();
        let at = window_start(
            NaiveDate::from_ymd_opt(2026, 11, 27).expect("valid date"),
            day.day_start_time,
            day.offset,
        );
        let mut completion = HabitEvent {
            id: HabitEventId::from(Uuid::from_u128(1)),
            habit: habit.id,
            kind: HabitEventKind::Completion,
            at,
            by: None,
            deleted: None,
        };
        assert_eq!(
            derive_window_counts(&habit, [&completion].into_iter(), day).len(),
            1
        );
        assert_eq!(
            derive_day_counts(&habit, [&completion].into_iter(), day).len(),
            1
        );
        completion.deleted = Some(HabitEventTombstone { at, by: None });
        assert!(derive_window_counts(&habit, [&completion].into_iter(), day).is_empty());
        assert!(derive_day_counts(&habit, [&completion].into_iter(), day).is_empty());

        let mut mark = HabitEvent {
            id: HabitEventId::from(Uuid::from_u128(2)),
            habit: habit.id,
            kind: skip_at(&habit, at),
            at,
            by: None,
            deleted: None,
        };
        assert_eq!(
            derive_skip_posture(&habit, [&mark].into_iter(), day).get(&skipped_window(&habit, at)),
            Some(&true)
        );
        mark.deleted = Some(HabitEventTombstone { at, by: None });
        assert!(derive_skip_posture(&habit, [&mark].into_iter(), day).is_empty());
    }
}
