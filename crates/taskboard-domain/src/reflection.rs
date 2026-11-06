// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reflection entries (DR-7): the user-facing review surface's entity
//! family, its curated attribute vocabulary, and its state home.
//!
//! The family is **device-local by ruling, roaming-ready by
//! construction** (ADR 0021): rows carry their own window instants and
//! LWW mutation clock, deletes are in-place tombstones, and the serde
//! decode tolerates unknown flag names — the future files-sync
//! import-fold reads rows exactly as stored. Windows and calendars are
//! always *data* here: nothing reads the clock or the environment (the
//! crate's clock rule).
//!
//! Vocabulary note (DR-7 D-J1): the user-facing name of this feature is
//! the review surface's `journal` selector (chunks 2–3); every code
//! symbol here is `Reflection*` — the undo command journal owns the
//! `Journal*` spelling (enforced by the crate's vocabulary-guard test).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::days::{DayCalendar, window_start};
use crate::habit::{PeriodKind, period_after, period_start};
use crate::ids::ReflectionId;

/// How wide the reviewed window is — which walk or verb produced the
/// entry, and the only calendar parameter the stored row keeps. Its own
/// enum (not `habit::PeriodKind`): a separate concern with a separate
/// doc and its own wire vocabulary (`"day"`/`"week"`/`"month"` — the
/// future storage column values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReviewCadence {
    /// One day window (the `DayCalendar` day).
    Day,
    /// One ISO week (Monday-start).
    Week,
    /// One calendar month.
    Month,
}

impl ReviewCadence {
    /// Parses the serde unit name (`"day"` / `"week"` / `"month"`) —
    /// the same text the storage column and the CLI `--cadence` flag
    /// store. An unknown cadence is a boundary parse rejection, never a
    /// domain error variant.
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        match raw {
            "day" => Some(Self::Day),
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            _ => None,
        }
    }

    /// The canonical text form (the serde unit name).
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

/// Hand-rolled A1 rating: private field, fallible mint rejecting 0 and
/// ≥6 — the `NonZeroCount` mirror. Decode is strict (a stored 0 or 6+
/// cannot deserialize — mints decode strictly, leniency lives one
/// layer up; no legacy rows exist, so strictness risks nothing).
///
/// # Minting vs. leniency
///
/// Every write path mints through [`Rating::try_new`] (the planner) or
/// passes this strict decode (serde), so no validate-and-forget path
/// exists — decoded rows are trusted-written, documented here as the
/// type's deliberate leniency posture (DR-7 decision P-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8")]
pub struct Rating(u8);

/// The rating mint rejected the value (outside `1..=5`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("rating must be between 1 and 5")]
pub struct InvalidRating;

impl Rating {
    /// The mint: `n` must be in `1..=5`.
    ///
    /// # Errors
    ///
    /// [`InvalidRating`] when `n` is 0 or ≥6.
    pub const fn try_new(n: u8) -> Result<Self, InvalidRating> {
        if n == 0 || n >= 6 {
            Err(InvalidRating)
        } else {
            Ok(Self(n))
        }
    }

    /// The rating (`1..=5` always).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Rating {
    type Error = InvalidRating;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        Self::try_new(n)
    }
}

/// The curated health-flag vocabulary, v1 (R-5; the member set is the
/// 2026-11-05 user ruling — `low-medication-day` was removed, the two
/// headache grades added, `migraine` stays the severe case).
///
/// Extension is **append-only**: the serialized form is the kebab-case
/// variant name; never rename or renumber. `Ord` is declaration order,
/// so a `BTreeSet<HealthFlag>` serializes deterministically. An unknown
/// *name* arriving on a decode path parks verbatim in
/// [`ReflectionAttributes::extra_flags`] (the tolerant parking) — this
/// mint accepts exactly the nine ruled names, case-sensitively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthFlag {
    /// Migraine — the severe case, distinct from the graded headaches.
    Migraine,
    /// Minor headache.
    MinorHeadache,
    /// Medium headache.
    MediumHeadache,
    /// Cold or flu.
    ColdOrFlu,
    /// Allergies.
    Allergies,
    /// Cramps.
    Cramps,
    /// Poor sleep.
    PoorSleep,
    /// Overwhelmed.
    Overwhelmed,
    /// The positive marker — a good day.
    GoodDay,
}

/// The health-flag mint rejected the name (not one of the nine ruled
/// kebab names); the offending name is carried verbatim.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown health flag")]
pub struct UnknownHealthFlag {
    /// The name that did not match the curated set.
    pub name: String,
}

impl HealthFlag {
    /// The mint: accepts exactly the nine ruled kebab names, case
    /// exact (`"Migraine"` is a rejection — case matters). The typed
    /// rejection lives at the mint, not in `CommandError`: command
    /// payloads carry typed `BTreeSet<HealthFlag>`, so the planner
    /// structurally cannot see an unknown name; the CLI `--flag` parse
    /// (chunk 2) consumes this error.
    ///
    /// # Errors
    ///
    /// [`UnknownHealthFlag`] with the name verbatim on any non-member.
    pub fn try_new(name: &str) -> Result<Self, UnknownHealthFlag> {
        Self::parse_text_form(name).ok_or(UnknownHealthFlag {
            name: name.to_owned(),
        })
    }

    /// Parses the serde unit name (the kebab string). `None` on any
    /// non-member — the inverse of [`text_form`](Self::text_form).
    #[must_use]
    pub fn parse_text_form(raw: &str) -> Option<Self> {
        match raw {
            "migraine" => Some(Self::Migraine),
            "minor-headache" => Some(Self::MinorHeadache),
            "medium-headache" => Some(Self::MediumHeadache),
            "cold-or-flu" => Some(Self::ColdOrFlu),
            "allergies" => Some(Self::Allergies),
            "cramps" => Some(Self::Cramps),
            "poor-sleep" => Some(Self::PoorSleep),
            "overwhelmed" => Some(Self::Overwhelmed),
            "good-day" => Some(Self::GoodDay),
            _ => None,
        }
    }

    /// The canonical text form (the serde unit name).
    #[must_use]
    pub const fn text_form(self) -> &'static str {
        match self {
            Self::Migraine => "migraine",
            Self::MinorHeadache => "minor-headache",
            Self::MediumHeadache => "medium-headache",
            Self::ColdOrFlu => "cold-or-flu",
            Self::Allergies => "allergies",
            Self::Cramps => "cramps",
            Self::PoorSleep => "poor-sleep",
            Self::Overwhelmed => "overwhelmed",
            Self::GoodDay => "good-day",
        }
    }
}

/// The structured part of a reflection entry: the mood/energy ratings
/// and the curated health-flag set.
///
/// Encode derives (both sets serialize as sorted name arrays —
/// `BTreeSet` order is deterministic). Decode is **hand-written and
/// tolerant** (DR-7 D-J3 / the contract's "unknown-field tolerance in
/// any serialized form"): a flag name the curated set does not know
/// parks verbatim into [`extra_flags`](ReflectionAttributes::extra_flags)
/// instead of failing the row; unknown struct keys are ignored by
/// serde's default behavior (pinned, not configured).
/// `extra_flags` never merges back into `flags` on encode — decode →
/// encode is byte-stable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReflectionAttributes {
    /// Mood rating, `1..=5` (`None` = unanswered).
    pub mood: Option<Rating>,
    /// Energy rating, `1..=5` (`None` = unanswered).
    pub energy: Option<Rating>,
    /// The curated flags carried by the entry.
    pub flags: BTreeSet<HealthFlag>,
    /// Unknown flag names parked verbatim by the tolerant decode
    /// (written by a newer build). Never merged back into `flags` on
    /// encode; `Default` = empty (write paths never park).
    pub extra_flags: BTreeSet<String>,
}

impl<'de> Deserialize<'de> for ReflectionAttributes {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            mood: Option<u8>,
            #[serde(default)]
            energy: Option<u8>,
            #[serde(default)]
            flags: Vec<String>,
            #[serde(default)]
            extra_flags: BTreeSet<String>,
        }
        let raw = Raw::deserialize(deserializer)?;
        // Ratings decode strictly (the `Rating` mint's serde posture):
        // a stored 0 or 6+ is a data error, never a clamp.
        let mood = raw
            .mood
            .map(Rating::try_new)
            .transpose()
            .map_err(|_| serde::de::Error::custom("rating out of range"))?;
        let energy = raw
            .energy
            .map(Rating::try_new)
            .transpose()
            .map_err(|_| serde::de::Error::custom("rating out of range"))?;
        let mut flags = BTreeSet::new();
        let mut extra_flags = raw.extra_flags;
        for name in raw.flags {
            match HealthFlag::parse_text_form(&name) {
                Some(flag) => {
                    flags.insert(flag);
                }
                None => {
                    extra_flags.insert(name);
                }
            }
        }
        Ok(Self {
            mood,
            energy,
            flags,
            extra_flags,
        })
    }
}

/// One reflection entry: a self-contained, editable, tombstone-in-place
/// row (DR-7 D-J2). The window instants are **stored, never derived at
/// read** — the day-window preference is device-local and may differ
/// across devices, so a stored entry means the same window everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectionEntry {
    /// The entry's local identity (roaming family — the future
    /// import-fold's wire key; never synced in v0.2).
    pub id: ReflectionId,
    /// Which walk/verb produced the entry.
    pub cadence: ReviewCadence,
    /// The window-date label (the `DayCalendar::today` reading of
    /// [`window_start`](ReflectionEntry::window_start) at mint time).
    pub day: NaiveDate,
    /// The reviewed window's opening instant (stored verbatim).
    pub window_start: DateTime<Utc>,
    /// The reviewed window's closing instant, exclusive (stored
    /// verbatim).
    pub window_end: DateTime<Utc>,
    /// The free-text body (may be empty).
    pub text: String,
    /// Mood/energy ratings + health flags.
    pub attributes: ReflectionAttributes,
    /// Row creation stamp (the minting planner's `now`).
    pub created_at: DateTime<Utc>,
    /// The LWW mutation clock (embedded per-entry — the roaming-readiness).
    pub updated_at: DateTime<Utc>,
    /// The in-place tombstone: delete flips it and bumps
    /// [`updated_at`](ReflectionEntry::updated_at); the row survives in
    /// the map (never silent absence).
    pub deleted: bool,
}

impl ReflectionEntry {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
    }
}

/// The reflection sub-state, nested in [`crate::state::AppState`] like
/// `TrackingState`. `BTreeMap` keeps serialization deterministic (the
/// state.rs determinism contract).
///
/// Tombstoned rows stay in the map — reads filter through
/// [`live_entries`](ReflectionState::live_entries), the one derived
/// live-read seam (the `TrackingState::open_entries` precedent; no
/// second stored set).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReflectionState {
    /// All entries, keyed by id (live rows and tombstones alike).
    pub entries: BTreeMap<ReflectionId, ReflectionEntry>,
}

impl ReflectionState {
    /// The live (non-tombstoned) entries, in deterministic id order —
    /// the one live-read seam every projection site goes through.
    #[must_use = "the live set is only derived, never stored — the call is the read"]
    pub fn live_entries(&self) -> impl Iterator<Item = &ReflectionEntry> {
        self.entries.values().filter(|entry| entry.is_live())
    }
}

/// A curated reflection template: typed data, not config (DR-7 D-J4 —
/// no `[templates]` TOML section, no stored rows in v1). The type shape
/// is the roaming-ready seam for stored templates (#639): rows keyed by
/// the same slug vocabulary become stored entities and
/// [`DEFAULT_TEMPLATES`] becomes seed data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReflectionTemplate {
    /// The stable slug (`"daily-close"`, …). Builtin slugs are a
    /// reserved namespace once stored templates exist (#639's mint
    /// rule).
    pub id: &'static str,
    /// The display name.
    pub name: &'static str,
    /// The cadence the template closes.
    pub cadence: ReviewCadence,
    /// The ruled prompt core, in walk order. A `const` cannot populate
    /// `Vec` fields (const-context allocation), hence the static slice
    /// (decision P-1) — which is also why no `Deserialize` exists: a
    /// decoded row would need owned data, and stored templates are
    /// #639's row shape, not this compiled-in const's.
    pub prompts: &'static [&'static str],
}

/// The compiled-in curated template set (DR-7 D-J4, the amended
/// four-prompt core verbatim — achievements, gratitude, improvement
/// ideas, open loops; exactly one template per cadence). The prompt
/// texts are ruled data: pinned byte-identical by test, editable only
/// by a ruling.
pub const DEFAULT_TEMPLATES: &[ReflectionTemplate] = &[
    ReflectionTemplate {
        id: "daily-close",
        name: "Daily close",
        cadence: ReviewCadence::Day,
        prompts: &[
            "What did I get done today?",
            "What am I grateful for today?",
            "What would I do differently next time?",
            "What's still on my mind?",
        ],
    },
    ReflectionTemplate {
        id: "weekly-close",
        name: "Weekly close",
        cadence: ReviewCadence::Week,
        prompts: &[
            "What did I get done this week?",
            "What am I grateful for this week?",
            "What would I do differently next week?",
            "What's still on my mind?",
        ],
    },
    ReflectionTemplate {
        id: "monthly-close",
        name: "Monthly close",
        cadence: ReviewCadence::Month,
        prompts: &[
            "What did I get done this month?",
            "What am I grateful for this month?",
            "What would I do differently next month?",
            "What's still on my mind?",
        ],
    },
];

/// The calendar-blind A1 mint the planner validates through (DR-7
/// D-J6): private fields, `Copy`; windows enter command payloads as
/// plain instants and cannot enter a row without passing
/// [`ReviewWindow::try_new`].
///
/// # Minting vs. leniency
///
/// Validation is **structural only** — `end > start`, plus per-cadence
/// length sanity (Day = 24h exactly, Week = 168h exactly; Month length
/// varies, so only monotonicity is checkable calendar-free). The `day`
/// label is taken on trust as mint input (the
/// `WindowStart::from_stored`/D-H12 posture): the mint enforces nothing
/// about the label — a non-Monday label on a week window is tolerated,
/// because the label is data the caller owns, and the derivation
/// ([`review_window`]) produces labels that satisfy the law by
/// construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewWindow {
    cadence: ReviewCadence,
    day: NaiveDate,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
}

/// The window mint rejected the payload: the duration did not match the
/// cadence's structural rule (or was not monotone at all).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("review window does not match its cadence")]
pub struct InvalidReviewWindow;

/// One day's duration (24h): the Day cadence's exact window length.
const DAY: chrono::TimeDelta = chrono::TimeDelta::try_hours(24).unwrap();
/// One ISO week's duration (168h): the Week cadence's exact length.
const WEEK: chrono::TimeDelta = chrono::TimeDelta::try_hours(24 * 7).unwrap();

impl ReviewWindow {
    /// The mint: structural validation only, per the type doc.
    ///
    /// # Errors
    ///
    /// [`InvalidReviewWindow`] when `end <= start`, when a Day window is
    /// not exactly 24h, or when a Week window is not exactly 168h. A
    /// Month window needs only monotonicity (month lengths vary;
    /// calendar-blindness is the ruling).
    pub fn try_new(
        cadence: ReviewCadence,
        day: NaiveDate,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Self, InvalidReviewWindow> {
        let span = end - start;
        let ok = match cadence {
            ReviewCadence::Day => span == DAY,
            ReviewCadence::Week => span == WEEK,
            ReviewCadence::Month => span > chrono::TimeDelta::zero(),
        };
        if ok {
            Ok(Self {
                cadence,
                day,
                start,
                end,
            })
        } else {
            Err(InvalidReviewWindow)
        }
    }

    /// The window's cadence.
    #[must_use]
    pub const fn cadence(&self) -> ReviewCadence {
        self.cadence
    }

    /// The window-date label (mint input, taken on trust).
    #[must_use]
    pub const fn day(&self) -> NaiveDate {
        self.day
    }

    /// The window's opening instant.
    #[must_use]
    pub const fn start(&self) -> DateTime<Utc> {
        self.start
    }

    /// The window's closing instant (exclusive).
    #[must_use]
    pub const fn end(&self) -> DateTime<Utc> {
        self.end
    }

    /// The `(start, end)` pair the projections take.
    #[must_use]
    pub const fn bounds(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        (self.start, self.end)
    }
}

/// The pure window derivation (DR-7 D-J8): floors `reference_date` to
/// its cadence period in the given calendar and returns the half-open
/// window `[period start, next period start)`.
///
/// One flooring implementation, two consumers (habits, review): the
/// period flooring composes `habit::period_start`/`period_after` with
/// `days::window_start` — no second flooring definition exists. Pure;
/// `calendar` is data (the D-F6 consumer contract). Fixed offsets have
/// no transitions, so the arithmetic is gap-free by construction.
///
/// Total for every representable reference date at least one period
/// inside chrono's date range (the period-step arithmetic saturates
/// only at chrono's extremes, the `period_after` documented leniency).
///
/// The output satisfies [`ReviewWindow::try_new`] by construction and
/// `calendar.calendar_date(window.day())` equals the floored
/// period-start date (the label law — both pinned by property).
///
/// # Panics
///
/// Panics only via `period_start`'s day-1 construction (impossible for
/// real calendar months) — never an input condition.
#[must_use]
pub fn review_window(
    cadence: ReviewCadence,
    reference_date: NaiveDate,
    calendar: DayCalendar,
) -> ReviewWindow {
    let kind = match cadence {
        ReviewCadence::Day => PeriodKind::Day,
        ReviewCadence::Week => PeriodKind::Week,
        ReviewCadence::Month => PeriodKind::Month,
    };
    let floored = period_start(kind, reference_date);
    let next = period_after(kind, floored);
    let start = window_start(floored, calendar.day_start_time, calendar.offset);
    let end = window_start(next, calendar.day_start_time, calendar.offset);
    ReviewWindow {
        cadence,
        day: floored,
        start,
        end,
    }
}

/// The window's carry-forward list (DR-7 D-J8): every **live, open**
/// task whose anchor instant — `scheduled` or `duedate` — fell inside
/// the window *and* before `now`, ordered by
/// `(anchor, order, id)` ascending (the scheduled section's
/// `(scheduled, order, id)` precedent generalized over the mixed
/// anchor set). A task carrying both fields qualifies through either;
/// its sort anchor is the earliest qualifying instant. Ids only — the
/// caller reads entities from its own map (the `TodaySections`
/// discipline). Pure; `now` and the window are data.
#[must_use]
pub fn window_misses(
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    now: DateTime<Utc>,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> Vec<crate::ids::TaskId> {
    let (start, end) = window;
    let mut misses: Vec<(DateTime<Utc>, i64, crate::ids::TaskId)> = Vec::new();
    for task in tasks.values() {
        if !task.is_live() || !task.is_open() {
            continue;
        }
        let qualifies = |instant: DateTime<Utc>| instant >= start && instant < end && instant < now;
        let anchor = [task.scheduled, task.duedate]
            .into_iter()
            .flatten()
            .filter(|instant| qualifies(*instant))
            .min();
        if let Some(anchor) = anchor {
            misses.push((anchor, task.order, task.id));
        }
    }
    misses.sort();
    misses.into_iter().map(|(_, _, id)| id).collect()
}

/// The window's completion list (DR-7 D-J8): every **live** task whose
/// `done` stamp fell inside the half-open window, ordered
/// `(done, id)` ascending — the `today_sections.done_today` shape
/// verbatim. Ids only; pure; the window is data.
#[must_use]
pub fn window_done(
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> Vec<crate::ids::TaskId> {
    let (start, end) = window;
    let mut dones: Vec<(DateTime<Utc>, crate::ids::TaskId)> = tasks
        .values()
        .filter_map(|task| {
            let done = task.done?;
            (task.is_live() && done >= start && done < end).then_some((done, task.id))
        })
        .collect();
    dones.sort();
    dones.into_iter().map(|(_, id)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TaskId;
    use crate::test_support::{proptest_config, timestamp_strategy};
    use proptest::prelude::*;

    /// The ruled kebab names, in declaration order — the test-side
    /// enumeration of the curated set (kept in sync with the variant
    /// declaration order by the declaration-order pin below).
    fn ruled_flag_names() -> Vec<&'static str> {
        crate::test_support::all_health_flag_names()
    }

    use chrono::{Datelike as _, FixedOffset, TimeZone as _};

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(id: u128) -> ReflectionEntry {
        ReflectionEntry {
            id: ReflectionId::from(uuid::Uuid::from_u128(id)),
            cadence: ReviewCadence::Day,
            day: NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            window_start: ts(100),
            window_end: ts(100 + 86_400),
            text: "shipped the reflection family".into(),
            attributes: ReflectionAttributes::default(),
            created_at: ts(200),
            updated_at: ts(200),
            deleted: false,
        }
    }

    #[test]
    fn rating_mint_accepts_one_through_five_and_rejects_the_rest() {
        for n in 1..=5 {
            assert_eq!(Rating::try_new(n).unwrap().get(), n);
        }
        for n in [0_u8, 6, 7, 255] {
            assert_eq!(Rating::try_new(n), Err(InvalidRating));
        }
    }

    #[test]
    fn rating_serde_decode_is_strict() {
        let three: Rating = serde_json::from_str("3").unwrap();
        assert_eq!(three.get(), 3);
        for bad in ["0", "6", "255"] {
            assert!(
                serde_json::from_str::<Rating>(bad).is_err(),
                "stored {bad} cannot decode"
            );
        }
        assert_eq!(
            serde_json::to_string(&Rating::try_new(4).unwrap()).unwrap(),
            "4"
        );
    }

    #[test]
    fn health_flag_mint_accepts_exactly_the_nine_ruled_names() {
        for (i, name) in ruled_flag_names().iter().enumerate() {
            let flag = HealthFlag::try_new(name).unwrap();
            assert_eq!(flag.text_form(), *name);
            // Declaration order: the ruled set's order is the enum's
            // discriminant order (deterministic BTreeSet snapshots).
            assert_eq!(flag as usize, i, "{name} sits at its ruled position");
        }
    }

    #[test]
    fn health_flag_mint_rejects_non_members_case_sensitively() {
        for bad in [
            "",
            "Migraine",
            "migraines",
            "low-medication-day",
            "good day",
            "GOOD-DAY",
        ] {
            let err = HealthFlag::try_new(bad).expect_err("reject");
            assert_eq!(err.name, bad, "offending name carried verbatim");
        }
    }

    #[test]
    fn health_flag_serde_wire_form_is_the_kebab_name() {
        for name in ruled_flag_names() {
            let flag = HealthFlag::parse_text_form(name).unwrap();
            let json = serde_json::to_string(&flag).unwrap();
            assert_eq!(json, format!(r#""{name}""#), "unit name is the wire form");
            let back: HealthFlag = serde_json::from_str(&json).unwrap();
            assert_eq!(back, flag);
        }
    }

    #[test]
    fn review_cadence_text_forms_are_the_kebab_names() {
        for (cadence, name) in [
            (ReviewCadence::Day, "day"),
            (ReviewCadence::Week, "week"),
            (ReviewCadence::Month, "month"),
        ] {
            assert_eq!(cadence.text_form(), name);
            assert_eq!(ReviewCadence::parse_text_form(name), Some(cadence));
            assert_eq!(
                serde_json::to_string(&cadence).unwrap(),
                format!(r#""{name}""#)
            );
        }
        assert_eq!(ReviewCadence::parse_text_form("fortnight"), None);
    }

    /// Unknown flag names interleave with known ones in the stored
    /// array: the tolerant decode parks them verbatim in `extra_flags`,
    /// keeps the knowns in `flags`, and never loses either.
    #[test]
    fn attributes_decode_parks_unknown_flag_names_verbatim() {
        let json = r#"{"flags":["good-day","solar-flare","cramps","low-medication-day"],"mood":3}"#;
        let attrs: ReflectionAttributes = serde_json::from_str(json).unwrap();
        assert_eq!(attrs.mood, Some(Rating::try_new(3).unwrap()));
        assert_eq!(
            attrs.flags.into_iter().collect::<Vec<_>>(),
            vec![HealthFlag::Cramps, HealthFlag::GoodDay]
        );
        let extra: Vec<_> = attrs.extra_flags.into_iter().collect();
        assert_eq!(extra, vec!["low-medication-day", "solar-flare"]);
    }

    /// The tolerant decode is round-trip-stable: decode → encode →
    /// decode yields identical values, and the re-encoded JSON parses
    /// back identically (byte stability of the parked names).
    #[test]
    fn attributes_decode_encode_is_byte_stable() {
        let json = r#"{"flags":["good-day","solar-flare"],"energy":5}"#;
        let attrs: ReflectionAttributes = serde_json::from_str(json).unwrap();
        let encoded = serde_json::to_string(&attrs).unwrap();
        let back: ReflectionAttributes = serde_json::from_str(&encoded).unwrap();
        assert_eq!(back, attrs);
        let re_encoded = serde_json::to_string(&back).unwrap();
        assert_eq!(re_encoded, encoded);
    }

    /// Unknown struct keys are tolerated by serde's default ignore
    /// behavior — pinned, not configured (the contract's tolerance
    /// clause).
    #[test]
    fn attributes_decode_ignores_unknown_struct_keys() {
        let json = r#"{"flags":[],"mood":2,"future_field":{"nested":1}}"#;
        let attrs: ReflectionAttributes = serde_json::from_str(json).unwrap();
        assert_eq!(attrs.mood, Some(Rating::try_new(2).unwrap()));
        assert!(attrs.extra_flags.is_empty());
    }

    /// A tombstoned entry's full row round-trips through serde (the
    /// tombstone shape is a first-class stored fact).
    #[test]
    fn tombstoned_entry_row_roundtrips_through_serde() {
        let mut row = entry(7);
        row.deleted = true;
        row.attributes.extra_flags.insert("legacy-flag".into());
        let json = serde_json::to_string(&row).unwrap();
        let back: ReflectionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, row);
        assert!(json.contains(r#""deleted":true"#));
    }

    #[test]
    fn default_templates_are_one_per_cadence_with_unique_ids() {
        assert_eq!(DEFAULT_TEMPLATES.len(), 3);
        for cadence in [
            ReviewCadence::Day,
            ReviewCadence::Week,
            ReviewCadence::Month,
        ] {
            let matches: Vec<_> = DEFAULT_TEMPLATES
                .iter()
                .filter(|t| t.cadence == cadence)
                .collect();
            assert_eq!(matches.len(), 1, "exactly one template per cadence");
            assert_eq!(matches[0].prompts.len(), 4, "the ruled four-prompt core");
        }
        let ids: BTreeSet<&str> = DEFAULT_TEMPLATES.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), DEFAULT_TEMPLATES.len(), "ids unique");
    }

    /// The ruled prompt core, byte-verbatim (DR-7 D-J4 as amended
    /// 2026-11-05 — data, not UI text; a golden assertion on the const).
    #[test]
    fn default_templates_carry_the_ruled_prompt_texts_verbatim() {
        let day = &DEFAULT_TEMPLATES[0];
        assert_eq!(day.id, "daily-close");
        assert_eq!(day.name, "Daily close");
        assert_eq!(
            day.prompts,
            &[
                "What did I get done today?",
                "What am I grateful for today?",
                "What would I do differently next time?",
                "What's still on my mind?",
            ]
        );
        let week = &DEFAULT_TEMPLATES[1];
        assert_eq!(week.id, "weekly-close");
        assert_eq!(week.name, "Weekly close");
        assert_eq!(
            week.prompts,
            &[
                "What did I get done this week?",
                "What am I grateful for this week?",
                "What would I do differently next week?",
                "What's still on my mind?",
            ]
        );
        let month = &DEFAULT_TEMPLATES[2];
        assert_eq!(month.id, "monthly-close");
        assert_eq!(month.name, "Monthly close");
        assert_eq!(
            month.prompts,
            &[
                "What did I get done this month?",
                "What am I grateful for this month?",
                "What would I do differently next month?",
                "What's still on my mind?",
            ]
        );
    }

    #[test]
    fn live_entries_filters_tombstones_and_stays_deterministic() {
        let mut state = ReflectionState::default();
        let live = entry(1);
        let mut dead = entry(2);
        dead.deleted = true;
        state.entries.insert(dead.id, dead);
        state.entries.insert(live.id, live);
        let got: Vec<_> = state.live_entries().map(|e| e.id).collect();
        assert_eq!(got, vec![ReflectionId::from(uuid::Uuid::from_u128(1))]);
        assert_eq!(state.entries.len(), 2, "the tombstone stays in the map");
    }

    /// A bounded reference-date domain: every cadence's flooring
    /// arithmetic stays a period inside chrono's representable range
    /// (the derivation's documented totality posture).
    fn reference_date() -> impl Strategy<Value = NaiveDate> {
        (1_u64..=45_000_u64).prop_map(|days| {
            NaiveDate::from_ymd_opt(1972, 1, 1)
                .unwrap()
                .checked_add_days(chrono::Days::new(days))
                .unwrap()
        })
    }

    /// The calendar domain: a few day-start times (midnight, the
    /// night-owl 05:00, a late 23:00) across a few fixed offsets (UTC,
    /// +2h, −9:30) — the days.rs property shapes.
    fn calendar() -> impl Strategy<Value = DayCalendar> {
        prop_oneof![
            Just(DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: FixedOffset::east_opt(0).unwrap(),
            }),
            Just(DayCalendar {
                day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
                offset: FixedOffset::east_opt(2 * 3600).unwrap(),
            }),
            Just(DayCalendar {
                day_start_time: chrono::NaiveTime::from_hms_opt(23, 0, 0).unwrap(),
                offset: FixedOffset::west_opt(9 * 3600 + 1800).unwrap(),
            }),
        ]
    }

    fn cadence() -> impl Strategy<Value = ReviewCadence> {
        prop::sample::select(&[
            ReviewCadence::Day,
            ReviewCadence::Week,
            ReviewCadence::Month,
        ])
    }

    /// A task fixture with exactly the fields the projections read
    /// (everything else inert defaults).
    #[allow(clippy::too_many_arguments)]
    fn task_fixture(
        raw: u128,
        scheduled: Option<DateTime<Utc>>,
        duedate: Option<DateTime<Utc>>,
        done: Option<DateTime<Utc>>,
        order: i64,
        deleted: bool,
    ) -> crate::entities::Task {
        let clock = ts(1);
        crate::entities::Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: String::new(),
            description: String::new(),
            duedate,
            scheduled,
            done,
            stack: crate::ids::StackId::from(uuid::Uuid::from_u128(1)),
            order,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: crate::entities::TaskKind::Standard,
            archived: false,
            deleted,
            clocks: crate::entities::TaskClocks {
                title: clock,
                description: clock,
                duedate: clock,
                scheduled: clock,
                done: clock,
                position: clock,
                effort: clock,
                importance: clock,
                urgency: clock,
                recur: clock,
                kind: clock,
                archived: clock,
                deleted: clock,
            },
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn task_map(values: &[crate::entities::Task]) -> BTreeMap<TaskId, crate::entities::Task> {
        values.iter().map(|t| (t.id, t.clone())).collect()
    }

    /// The miss predicate's exact-shape table: anchors at exactly `now`,
    /// outside the half-open window, done tasks, and tombstones are all
    /// not-missed; both anchors qualify through either field.
    #[test]
    fn window_misses_predicate_shapes() {
        let start = ts(100_000);
        let end = ts(100_000 + 86_400);
        let now = ts(100_000 + 3_600);
        let in_window = |minutes: i64| ts(100_000 + 60 * minutes);
        let tasks = task_map(&[
            // scheduled inside, before now: the canonical miss.
            task_fixture(1, Some(in_window(30)), None, None, 0, false),
            // duedate inside, before now: the overdue miss.
            task_fixture(2, None, Some(in_window(45)), None, 0, false),
            // both anchors inside: qualifies through either.
            task_fixture(3, Some(in_window(30)), Some(in_window(45)), None, 0, false),
            // scheduled inside but at exactly `now`: not yet missed.
            task_fixture(4, Some(now), None, None, 0, false),
            // scheduled inside but after `now`: the future.
            task_fixture(5, Some(in_window(12 * 60)), None, None, 0, false),
            // anchor before the window: outside (half-open low edge).
            task_fixture(
                6,
                Some(start - chrono::TimeDelta::try_seconds(1).unwrap()),
                None,
                None,
                0,
                false,
            ),
            // anchor at `end`: outside (exclusive high edge).
            task_fixture(7, Some(end), None, None, 0, false),
            // done inside the window: the open gate excludes it.
            task_fixture(8, Some(in_window(30)), None, Some(in_window(45)), 0, false),
            // a miss, tombstoned: the live gate excludes it.
            task_fixture(9, Some(in_window(90)), None, None, 0, true),
            // scheduled qualifies (90min) while the duedate sits outside
            // the window: qualifies through the scheduled anchor alone.
            task_fixture(
                10,
                Some(in_window(45)),
                Some(start - chrono::TimeDelta::try_seconds(1).unwrap()),
                None,
                5,
                false,
            ),
        ]);
        let misses = window_misses(&tasks, now, (start, end));
        // Ordered (anchor, order, id): 1 and 3 tie at 30min (order 0 →
        // id break), then 2 at 45min, then 10 at 90min.
        assert_eq!(
            misses,
            vec![
                TaskId::from(uuid::Uuid::from_u128(1)),
                TaskId::from(uuid::Uuid::from_u128(3)),
                TaskId::from(uuid::Uuid::from_u128(2)),
                TaskId::from(uuid::Uuid::from_u128(10)),
            ],
            "anchor-chronological with (order, id) tie-breaks"
        );
    }

    /// The done list: live ∧ `done ∈ [start, end)`, ordered
    /// `(done, id)`; tombstoned and out-of-window dones excluded.
    #[test]
    fn window_done_predicate_shapes() {
        let start = ts(100_000);
        let end = ts(100_000 + 86_400);
        let tasks = task_map(&[
            task_fixture(1, None, None, Some(ts(100_500)), 0, false),
            task_fixture(2, None, None, Some(ts(100_200)), 0, false),
            // equal to `start` (inclusive low edge) — sorts first.
            task_fixture(3, None, None, Some(start), 0, false),
            // at `end`: excluded (exclusive high edge).
            task_fixture(4, None, None, Some(end), 0, false),
            // done outside (before window): excluded.
            task_fixture(5, None, None, Some(ts(90_000)), 0, false),
            // tombstoned: excluded.
            task_fixture(6, None, None, Some(ts(100_100)), 0, true),
        ]);
        assert_eq!(
            window_done(&tasks, (start, end)),
            vec![
                TaskId::from(uuid::Uuid::from_u128(3)),
                TaskId::from(uuid::Uuid::from_u128(2)),
                TaskId::from(uuid::Uuid::from_u128(1)),
            ]
        );
    }

    /// One generated anchor tuple: `(scheduled offset, due offset, done
    /// offset, order, has-scheduled, has-due, tombstone, id raw)`,
    /// offsets in seconds from the window start; a negative done offset
    /// means "no done stamp".
    type AnchorTuple = (i64, i64, i64, i64, bool, bool, bool, u128);

    fn generated_tasks(anchors: &[AnchorTuple]) -> BTreeMap<TaskId, crate::entities::Task> {
        anchors
            .iter()
            .map(
                |&(sched, due, done, order, with_sched, with_due, deleted, raw)| {
                    let t = task_fixture(
                        raw,
                        with_sched.then(|| ts(100_000 + sched)),
                        with_due.then(|| ts(100_000 + due)),
                        (done >= 0).then(|| ts(100_000 + done)),
                        order,
                        deleted,
                    );
                    (t.id, t)
                },
            )
            .collect()
    }

    fn anchor_set() -> impl Strategy<Value = Vec<AnchorTuple>> {
        proptest::collection::vec(
            (
                -2_i64 * 86_400..=2 * 86_400,
                -2_i64 * 86_400..=2 * 86_400,
                -2_i64 * 86_400..=2 * 86_400,
                -100_i64..=100,
                any::<bool>(),
                any::<bool>(),
                any::<bool>(),
                any::<u128>(),
            ),
            0..20,
        )
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        /// Generated task sets with anchors wandering ±2 days around a
        /// fixed window: the projections are deterministic, the miss
        /// list is output-ordered `(anchor, order, id)`, and both lists
        /// only ever contain live tasks (the gates are structural
        /// facts of the output; the predicate itself is pinned by the
        /// unit tables and the today_sections consistency pins).
        #[test]
        fn projections_are_deterministic_ordered_and_gated(
            anchors in anchor_set(),
        ) {
            let start = ts(100_000);
            let end = ts(100_000 + 86_400);
            let now = ts(100_000 + 3_600);
            let tasks = generated_tasks(&anchors);
            let first = window_misses(&tasks, now, (start, end));
            prop_assert_eq!(&first, &window_misses(&tasks, now, (start, end)), "determinism");
            // Output ordering: consecutive rows are non-decreasing in
            // (anchor, order, id).
            for pair in first.windows(2) {
                let qualifies = |x: DateTime<Utc>| x >= start && x < end && x < now;
                let key = |id: &TaskId| {
                    let t = &tasks[id];
                    let anchor = [t.scheduled, t.duedate]
                        .into_iter()
                        .flatten()
                        .filter(|x| qualifies(*x))
                        .min()
                        .expect("a listed miss carries a qualifying anchor");
                    (anchor, t.order, *id)
                };
                prop_assert!(key(&pair[0]) <= key(&pair[1]), "(anchor, order, id) order");
            }
            for id in &first {
                let t = &tasks[id];
                prop_assert!(t.is_live() && t.is_open(), "misses gate on live ∧ open");
            }
            let done_first = window_done(&tasks, (start, end));
            prop_assert_eq!(&done_first, &window_done(&tasks, (start, end)), "done determinism");
            for id in &done_first {
                let t = &tasks[id];
                prop_assert!(t.is_live(), "done gates on live");
                let done = t.done.expect("a listed done carries a stamp");
                prop_assert!(done >= start && done < end, "done inside the window");
            }
        }

        /// Consistency pin 11 (the no-second-classification guarantee):
        /// every miss is classified by `today_sections`' overdue or
        /// scheduled section (anchor-qualifying), and every
        /// anchor-qualifying member of those two sections is a miss —
        /// a two-sided set equality.
        #[test]
        fn day_misses_are_the_anchor_qualifying_overdue_and_scheduled(
            anchors in anchor_set(),
        ) {
            let start = ts(100_000);
            let end = ts(100_000 + 86_400);
            let now = ts(100_000 + 3_600);
            let tasks = generated_tasks(&anchors);
            let sections = crate::days::today_sections(&tasks, now, (start, end));
            // "Anchor-qualifying" is task-level (either anchor qualifies):
            // an overdue-classified task whose *scheduled* anchor sits in
            // the window still misses through that anchor.
            let anchor_qualifies = |id: &TaskId| {
                let t = &tasks[id];
                let in_past_window = |x: DateTime<Utc>| x >= start && x < end && x < now;
                t.scheduled.is_some_and(in_past_window)
                    || t.duedate.is_some_and(in_past_window)
            };
            let expected: BTreeSet<TaskId> = sections
                .overdue
                .into_iter()
                .chain(sections.scheduled)
                .filter(|id| anchor_qualifies(id))
                .collect();
            prop_assert_eq!(
                window_misses(&tasks, now, (start, end)).into_iter().collect::<BTreeSet<_>>(),
                expected,
                "no third classification exists"
            );
        }

        /// Consistency pin 12: for the day window, `window_done` equals
        /// `today_sections.done_today` exactly — predicate and order.
        #[test]
        fn day_done_equals_the_done_today_section(
            anchors in proptest::collection::vec(
                (
                    -2_i64 * 86_400..=2 * 86_400,
                    -2_i64 * 86_400..=2 * 86_400,
                    0_i64..=2 * 86_400,
                    -100_i64..=100,
                    any::<bool>(),
                    any::<bool>(),
                    any::<bool>(),
                    any::<u128>(),
                ),
                0..20,
            ),
        ) {
            let start = ts(100_000);
            let end = ts(100_000 + 86_400);
            let now = ts(100_000 + 3_600);
            let tasks = generated_tasks(&anchors);
            let sections = crate::days::today_sections(&tasks, now, (start, end));
            prop_assert_eq!(
                window_done(&tasks, (start, end)),
                sections.done_today,
                "predicate and order"
            );
        }
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        // ---- the window derivation battery (plan §5.1 items 1–6) ----

        /// Day windows: exactly 24h, opening at the reference's own
        /// window-start, labeled by the reference, containing it.
        #[test]
        fn day_windows_are_exact_and_labeled(
            reference in reference_date(),
            cal in calendar(),
        ) {
            let window = review_window(ReviewCadence::Day, reference, cal);
            prop_assert_eq!(window.end() - window.start(), DAY);
            prop_assert_eq!(window.start(), window_start(reference, cal.day_start_time, cal.offset));
            prop_assert_eq!(cal.calendar_date(window.start()), reference, "label law");
            prop_assert!(window.start() <= window_start(reference, cal.day_start_time, cal.offset)
                && window_start(reference, cal.day_start_time, cal.offset) < window.end());
        }

        /// Week windows: exactly 168h, Monday-anchored; mid-week
        /// references floor to the same window.
        #[test]
        fn week_windows_are_exact_monday_anchored_and_midweek_stable(
            reference in reference_date(),
            mid_week_shift in 0_u64..7,
            cal in calendar(),
        ) {
            let floored = period_start(PeriodKind::Week, reference);
            let mid_week = floored
                .checked_add_days(chrono::Days::new(mid_week_shift))
                .unwrap();
            let window = review_window(ReviewCadence::Week, mid_week, cal);
            prop_assert_eq!(window.end() - window.start(), WEEK);
            prop_assert_eq!(
                window.start(),
                window_start(floored, cal.day_start_time, cal.offset)
            );
            // Monday-start: the label reads Monday in the offset
            // calendar (chrono weekday, week starting Monday).
            prop_assert_eq!(cal.calendar_date(window.start()).weekday(), chrono::Weekday::Mon);
            prop_assert_eq!(
                window,
                review_window(ReviewCadence::Week, reference, cal),
                "mid-week reference floors to the same window"
            );
        }

        /// Month windows: bounded by day 1 and next month's day 1;
        /// monotone across month lengths and leap years.
        #[test]
        fn month_windows_span_calendar_months(
            reference in reference_date(),
            cal in calendar(),
        ) {
            let floored = period_start(PeriodKind::Month, reference);
            let next = period_after(PeriodKind::Month, floored);
            let window = review_window(ReviewCadence::Month, reference, cal);
            prop_assert_eq!(window.start(), window_start(floored, cal.day_start_time, cal.offset));
            prop_assert_eq!(window.end(), window_start(next, cal.day_start_time, cal.offset));
            prop_assert_eq!(cal.calendar_date(window.start()).day(), 1);
            prop_assert!(window.end() > window.start(), "monotone across 28–31 day months");
        }

        /// Cadence-uniform pins: the derived window satisfies the mint
        /// (composition law); equal inputs equal outputs; the day
        /// derivation IS `day_window` — no second day-window definition.
        #[test]
        fn derived_windows_satisfy_the_mint_and_day_equals_day_window(
            now in timestamp_strategy(),
            reference in reference_date(),
            cadence in cadence(),
            cal in calendar(),
        ) {
            let window = review_window(cadence, reference, cal);
            prop_assert_eq!(
                ReviewWindow::try_new(cadence, window.day(), window.start(), window.end()),
                Ok(window),
                "composition law"
            );
            prop_assert_eq!(window, review_window(cadence, reference, cal), "determinism");
            let day = review_window(ReviewCadence::Day, cal.today(now), cal);
            prop_assert_eq!(day.bounds(), crate::days::day_window(now, cal.day_start_time, cal.offset));
        }

        /// The DST battery composes through the fixed-offset rule: the
        /// derivation's bounds equal the plain `window_start`
        /// composition at every transition-adjacent instant's date.
        #[test]
        fn dst_boundary_dates_compose_through_the_plain_composition(
            stamp in proptest::sample::select(crate::test_support::dst_boundary_instants()),
            cal in calendar(),
            cadence in cadence(),
        ) {
            let reference = cal.calendar_date(stamp);
            let window = review_window(cadence, reference, cal);
            let floored = period_start(
                match cadence {
                    ReviewCadence::Day => PeriodKind::Day,
                    ReviewCadence::Week => PeriodKind::Week,
                    ReviewCadence::Month => PeriodKind::Month,
                },
                reference,
            );
            let next = period_after(
                match cadence {
                    ReviewCadence::Day => PeriodKind::Day,
                    ReviewCadence::Week => PeriodKind::Week,
                    ReviewCadence::Month => PeriodKind::Month,
                },
                floored,
            );
            prop_assert_eq!(window.start(), window_start(floored, cal.day_start_time, cal.offset));
            prop_assert_eq!(window.end(), window_start(next, cal.day_start_time, cal.offset));
        }

        /// Mint rejections: inverted spans and wrong-duration windows
        /// for Day and Week; a 400-day "month" span is accepted — the
        /// documented calendar-blindness.
        #[test]
        fn mint_rejects_structurally_invalid_windows(
            start in timestamp_strategy(),
            day in reference_date(),
        ) {
            let hour = chrono::TimeDelta::try_hours(1).unwrap();
            let end = start + DAY;
            // Inverted / empty spans reject on every cadence.
            for cadence in [ReviewCadence::Day, ReviewCadence::Week, ReviewCadence::Month] {
                prop_assert_eq!(ReviewWindow::try_new(cadence, day, end, start), Err(InvalidReviewWindow));
                prop_assert_eq!(ReviewWindow::try_new(cadence, day, start, start), Err(InvalidReviewWindow));
            }
            // Day must be exactly 24h; Week exactly 168h.
            prop_assert_eq!(
                ReviewWindow::try_new(ReviewCadence::Day, day, start, end + hour),
                Err(InvalidReviewWindow)
            );
            prop_assert_eq!(
                ReviewWindow::try_new(ReviewCadence::Day, day, start, end - hour),
                Err(InvalidReviewWindow)
            );
            let week_end = start + WEEK;
            prop_assert_eq!(
                ReviewWindow::try_new(ReviewCadence::Week, day, start, week_end + hour),
                Err(InvalidReviewWindow)
            );
            prop_assert_eq!(
                ReviewWindow::try_new(ReviewCadence::Week, day, start, week_end - hour),
                Err(InvalidReviewWindow)
            );
            prop_assert_eq!(
                ReviewWindow::try_new(ReviewCadence::Week, day, start, end),
                Err(InvalidReviewWindow),
                "a 24h week-window rejects"
            );
            // Month checks monotonicity only: a 400-day span passes.
            let four_hundred_days = chrono::TimeDelta::try_days(400).unwrap();
            prop_assert!(
                ReviewWindow::try_new(ReviewCadence::Month, day, start, start + four_hundred_days).is_ok()
            );
            // The label is never validated: a non-Monday label on a
            // well-formed week window is accepted (the documented trust).
            prop_assert!(ReviewWindow::try_new(ReviewCadence::Week, day, start, week_end).is_ok());
        }

        // ---- the commit-1 battery ----

        #[test]
        fn attributes_roundtrip_through_serde(
            mood in proptest::option::of(1_u8..=5),
            energy in proptest::option::of(1_u8..=5),
            flags in proptest::collection::vec(
                proptest::sample::select(ruled_flag_names()),
                0..9,
            ),
        ) {
            let to_rating = |n: Option<u8>| n.and_then(|v| Rating::try_new(v).ok());
            let attrs = ReflectionAttributes {
                mood: to_rating(mood),
                energy: to_rating(energy),
                flags: flags.iter().filter_map(|f| HealthFlag::parse_text_form(f)).collect(),
                extra_flags: BTreeSet::new(),
            };
            let json = serde_json::to_string(&attrs).unwrap();
            let back: ReflectionAttributes = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, attrs);
        }

        #[test]
        fn entry_rows_roundtrip_through_serde(
            stamp in timestamp_strategy(),
            deleted in any::<bool>(),
            text in crate::test_support::string_strategy(),
        ) {
            let mut row = entry(9);
            row.created_at = stamp;
            row.updated_at = stamp;
            row.deleted = deleted;
            row.text = text;
            let json = serde_json::to_string(&row).unwrap();
            let back: ReflectionEntry = serde_json::from_str(&json).unwrap();
            prop_assert_eq!(back, row);
        }
    }
}
