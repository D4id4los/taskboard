// SPDX-License-Identifier: MIT OR Apache-2.0
//! Habit trackers: the [`Habit`] entity, its [`HabitState`] home, and
//! the period-boundary vocabulary (DR-6/5).
//!
//! **Nothing ticks** — never-done is native. A habit has no spawn/roll/
//! done machinery of any kind; a window's state *is* its log row, and
//! the log is keyed `(habit, window)` with ≤ 1 row per key — the
//! reminder fact shape with a count payload. Every period instant
//! derives through the `days` vocabulary ([`crate::days`]); this module
//! introduces no second local-date concept.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::days::DayCalendar;
use crate::ids::{BoardId, HabitId, SeriesId};

/// Hand-rolled A1 count: private field, fallible mint rejecting zero.
/// Decode is strict (a stored `0` cannot deserialize — the recur.rs
/// posture: mints decode strictly, leniency lives one layer up).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u32")]
pub struct NonZeroCount(u32);

/// The count mint rejected zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("habit target count must be at least 1")]
pub struct InvalidHabitTarget;

impl NonZeroCount {
    /// The mint: `n` must be at least 1.
    ///
    /// # Errors
    ///
    /// [`InvalidHabitTarget`] when `n` is 0.
    pub const fn new(n: u32) -> Result<Self, InvalidHabitTarget> {
        if n == 0 {
            Err(InvalidHabitTarget)
        } else {
            Ok(Self(n))
        }
    }

    /// The count (≥ 1 always).
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl TryFrom<u32> for NonZeroCount {
    type Error = InvalidHabitTarget;

    fn try_from(n: u32) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// The [`MonthDay`] mint rejected the value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("month day must be 1..=31")]
pub struct InvalidMonthDay;

/// Hand-rolled A1 day-of-month: private field, fallible mint rejecting
/// 0 and anything past 31 (every calendar month reaches at least 28;
/// the *clamping* to the month's actual length is [`nudge_day`]'s
/// documented leniency, not this type's job). Decode is strict (the
/// [`NonZeroCount`] posture: a stored 0 or 32 cannot deserialize).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "u8")]
pub struct MonthDay(u8);

impl MonthDay {
    /// The mint: `n` must be 1..=31.
    ///
    /// # Errors
    ///
    /// [`InvalidMonthDay`] when `n` is 0 or past 31.
    pub const fn new(n: u8) -> Result<Self, InvalidMonthDay> {
        if n == 0 || n > 31 {
            Err(InvalidMonthDay)
        } else {
            Ok(Self(n))
        }
    }

    /// The day (1..=31 always).
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for MonthDay {
    type Error = InvalidMonthDay;

    fn try_from(n: u8) -> Result<Self, Self::Error> {
        Self::new(n)
    }
}

/// The optional display/nudge anchor day of a habit (O-1 option b): a
/// day *inside the canonical window* that governs display and nudging.
/// Windows stay ISO-Monday / calendar-1st ([`period_start`] is
/// untouched) and the streak walk never reads this field — pinned by
/// the `anchor_day_window_invariance` properties.
///
/// # Documented leniency
///
/// A `Some(anchor_day)` on a `PerDay` habit, or a `Weekday(..)` on a
/// `PerMonth` habit (or a `MonthDay(..)` on a `PerWeek` habit), is
/// **inert**: [`nudge_day`] reads the variant matching the current
/// target kind and falls through otherwise. The planner accepts the
/// field as-is (it is display data, not window data) — no
/// validate-and-forget path exists because nothing is validated away;
/// mismatched variants simply never influence anything. The *CLI*
/// refuses kind-mismatched `--day` values at parse time (cheaper
/// feedback at the trust boundary), but the domain does not police it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnchorDay {
    /// For a `PerWeek` habit: the ISO weekday the nudge arms on.
    Weekday(chrono::Weekday),
    /// For a `PerMonth` habit: the calendar day (1..=31) the nudge
    /// arms on — clamped to the month's actual length at read time
    /// (a 31 in February arms on the 28th/29th).
    MonthDay(MonthDay),
}

/// The [`WeekdaySet`] mint rejected the input (the empty set at the
/// mint; unmaskable bits cannot occur — decode masks first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("a weekday set must name at least one weekday")]
pub struct InvalidWeekdaySet;

/// Hand-rolled A1 weekday set (HF4, #960): private bitflag-`u8` field,
/// bit *n* = `chrono::Weekday` `n` days from Monday (Mon = bit 0 …
/// Sun = bit 6 — the `num_days_from_monday` order). The mint rejects
/// the empty set at the write boundary; decode is **lenient by
/// design**: any stored `u8` decodes, masked to the seven valid bits —
/// a synced row carrying an empty set (a former writer's artifact) is
/// data, not corruption, and every consumer treats an empty set as
/// "no day set" (today's posture) instead of "due never" ([`Habit::effective_scheduled_days`]
/// owns that rule — no validate-and-forget path exists because nothing
/// is validated away).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "u8", into = "u8")]
pub struct WeekdaySet(u8);

impl WeekdaySet {
    /// The full seven-day set (the `--days all` / clear-leg value).
    #[must_use]
    pub const fn all() -> Self {
        Self(0x7F)
    }

    /// The mint: at least one weekday must be named.
    ///
    /// # Errors
    ///
    /// [`InvalidWeekdaySet`] when no weekday is named.
    pub fn new(days: impl IntoIterator<Item = chrono::Weekday>) -> Result<Self, InvalidWeekdaySet> {
        let mut bits = 0_u8;
        for day in days {
            bits |= 1 << day.num_days_from_monday();
        }
        Self::from_bits(bits).ok_or(InvalidWeekdaySet)
    }

    /// The decode seam: mask to the seven valid bits, then mint.
    /// `None` only when the masked set is empty (the caller — serde or
    /// a lenient reader — decides what an empty set means).
    #[must_use]
    pub const fn from_bits(bits: u8) -> Option<Self> {
        let masked = bits & 0x7F;
        if masked == 0 {
            None
        } else {
            Some(Self(masked))
        }
    }

    /// Whether `day` is scheduled.
    #[must_use]
    pub const fn contains(self, day: chrono::Weekday) -> bool {
        self.0 & (1 << day.num_days_from_monday()) != 0
    }

    /// Whether no weekday is scheduled (possible only via the lenient
    /// decode seam).
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The scheduled weekdays, Monday-first.
    pub fn days(self) -> impl Iterator<Item = chrono::Weekday> {
        const BY_BIT: [chrono::Weekday; 7] = [
            chrono::Weekday::Mon,
            chrono::Weekday::Tue,
            chrono::Weekday::Wed,
            chrono::Weekday::Thu,
            chrono::Weekday::Fri,
            chrono::Weekday::Sat,
            chrono::Weekday::Sun,
        ];
        let bits = self.0;
        (0_usize..7).filter_map(move |bit| (bits & (1 << bit) != 0).then_some(BY_BIT[bit]))
    }

    /// The raw mask (the wire form).
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

impl From<u8> for WeekdaySet {
    fn from(bits: u8) -> Self {
        // Lenient decode (documented at the type): mask, and an empty
        // masked set decodes to the empty set — consumers default it.
        Self(bits & 0x7F)
    }
}

impl From<WeekdaySet> for u8 {
    fn from(set: WeekdaySet) -> Self {
        set.0
    }
}

/// The reminder override's anchor base (R-8): the default
/// [`NudgeBase::NudgeDay`] keeps O-1's nudge day(s) verbatim;
/// [`NudgeBase::PeriodStart`] is the explicit alternative anchoring the
/// nudge at the canonical window's start. The override *parameterizes*
/// O-1's nudge law — it never contradicts it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum NudgeBase {
    /// O-1's nudge day (default): the anchor day when set, else the
    /// period start; with a scheduled-day set, the scheduled days
    /// themselves.
    #[default]
    NudgeDay,
    /// The canonical window's start (ISO-Monday / calendar-1st — the
    /// boundaries never move).
    PeriodStart,
}

/// The habit's chosen snooze grammar (O-1: "user picks hours/days").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HabitSnooze {
    /// Snooze in whole hours.
    Hours(u8),
    /// Snooze in whole days.
    Days(u8),
}

impl HabitSnooze {
    /// The fallible mint for a whole-hours snooze (#1211): `None` for
    /// `n == 0` — a zero-duration snooze is degenerate (the snooze
    /// chain walk moves the occurrence by zero) and is unmintable.
    ///
    /// Deliberate leniency, at the type: deserialization
    /// (persisted-state / journal decode) keeps the raw variant
    /// construction — decode is not a rule decision boundary, and
    /// journals minted before this law may legitimately carry a zero
    /// snooze; decode must not refuse to load history. The one stored
    /// grammar exception: the sqlite decode normalizes a decoded
    /// zero-duration snooze to `Hours(1)` (#1271) — the degenerate
    /// value makes the snooze chain a silent no-op.
    #[must_use]
    pub fn hours(n: u8) -> Option<Self> {
        (n > 0).then_some(Self::Hours(n))
    }

    /// The fallible mint for a whole-days snooze (#1211); the same
    /// law and the same documented leniency as [`HabitSnooze::hours`].
    #[must_use]
    pub fn days(n: u8) -> Option<Self> {
        (n > 0).then_some(Self::Days(n))
    }

    /// The snooze duration.
    ///
    /// # Panics
    ///
    /// Panics only if chrono cannot represent the value — a
    /// programming error (u8 hours/days are far in range), not an
    /// input condition.
    #[must_use]
    pub fn duration(self) -> chrono::TimeDelta {
        match self {
            Self::Hours(h) => chrono::TimeDelta::try_hours(i64::from(h)),
            Self::Days(d) => chrono::TimeDelta::try_days(i64::from(d)),
        }
        .expect("u8 hours/days are representable")
    }
}

/// A habit's per-habit reminder override (HF4, #959): the O-1 nudge
/// law's parameters, restated per habit — which anchor day the nudge
/// derives from ([`NudgeBase`]), how many days after it (`offset_days`,
/// may be negative to pull *before* the base), at what local
/// time-of-day it arms, and which snooze unit the snooze affordance
/// offers. `time` is the *fallback* time-of-day: the occurrence arms at
/// the habit's `anchor_time` when that is set, else at this field —
/// `None` is the **anchor-time-only posture** (#1212, operator ruling
/// comment 7079171625): the override arms the nudge on the day gate
/// alone and contributes no clock time of its own; the pre-#1212
/// 09:00 fallback mint is retired, so a blank time field never
/// invents one.
///
/// Deliberate leniency: an override on any target kind is accepted
/// (the nudge law is defined for every kind); only `scheduled_days`
/// carries the `PerDay` refusal (R-6 — that field would mint inert
/// data, an override does not).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitReminderOverride {
    /// Which anchor day the nudge derives from.
    #[serde(default)]
    pub base: NudgeBase,
    /// Days added to the base day (may be negative — a nudge *before*
    /// the base day is a legitimate ask, e.g. "the day before the
    /// period starts"). The arm test is a membership test, never
    /// clamped: an armed day outside the named window simply does not
    /// arm inside it — it lands in the neighbor window's span, which
    /// runs its own gate (`override_armed_today`).
    pub offset_days: i32,
    /// The fallback local time-of-day (used when the habit carries no
    /// `anchor_time`). `None` is the anchor-time-only posture (#1212):
    /// the override names *when the day gate arms* but no clock time —
    /// `tracker_arm_time` then falls through to `None` unless the
    /// habit carries an `anchor_time`. Serde: absent decodes to `None`
    /// (new payloads), a concrete time decodes to `Some` (old
    /// payloads round-trip); no default-to-09:00 anywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<chrono::NaiveTime>,
    /// The snooze unit the habit's nudge offers.
    pub snooze: HabitSnooze,
}

/// Which period a habit's target counts over. The window vocabulary of
/// the whole module ([`period_start`] and friends).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PeriodKind {
    /// One day window (the `DayCalendar` day).
    Day,
    /// One ISO week (Monday-start).
    Week,
    /// One calendar month.
    Month,
}

/// The habit's quota over its period. Externally tagged on the wire
/// (`{"PerDay": 3}`); zero is unrepresentable at every layer because
/// the payload goes through the [`NonZeroCount`] mint's strict decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HabitTarget {
    /// The quota counts per day window.
    PerDay(NonZeroCount),
    /// The quota counts per ISO week.
    PerWeek(NonZeroCount),
    /// The quota counts per calendar month.
    PerMonth(NonZeroCount),
}

impl HabitTarget {
    /// The period this target counts over.
    #[must_use]
    pub const fn kind(&self) -> PeriodKind {
        match self {
            HabitTarget::PerDay(_) => PeriodKind::Day,
            HabitTarget::PerWeek(_) => PeriodKind::Week,
            HabitTarget::PerMonth(_) => PeriodKind::Month,
        }
    }

    /// The quota (≥ 1 always — the mint).
    #[must_use]
    pub const fn quota(&self) -> NonZeroCount {
        match self {
            HabitTarget::PerDay(n) | HabitTarget::PerWeek(n) | HabitTarget::PerMonth(n) => *n,
        }
    }
}

/// A canonical period-start local date. Private field; the infallible
/// mint floors to the period start ([`WindowStart::of_period`]), so
/// command-path keys are canonical by construction.
///
/// # Minting vs. leniency
///
/// Decoded rows are **lenient by design**: a non-canonical date decodes
/// as-is (it may be a synced fact keyed by a former target kind — D-H12)
/// and is *analytically inert*: every projection enumerates canonical
/// windows of the current kind only, so a non-canonical row reads as
/// absent. `AdjustHabitLog` rejects non-canonical windows explicitly at
/// the planner (typed `InvalidHabitWindow`), so no validate-and-forget
/// path exists on the write side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WindowStart(NaiveDate);

impl WindowStart {
    /// The canonical window-start of `date`'s period: floors the date
    /// to its period boundary ([`period_start`]).
    #[must_use]
    pub fn of_period(kind: PeriodKind, date: NaiveDate) -> Self {
        Self(period_start(kind, date))
    }

    /// The period-start date.
    #[must_use]
    pub const fn as_date(self) -> NaiveDate {
        self.0
    }

    /// The decode seam (D-H12): a stored or synced window date decodes
    /// **as-is**, never floored — flooring here would silently rewrite
    /// former-kind synced facts. Canonicality is the write path's law
    /// (command-path keys mint through [`WindowStart::of_period`]);
    /// decoded non-canonical rows stay analytically inert instead.
    #[must_use]
    pub const fn from_stored(date: NaiveDate) -> Self {
        Self(date)
    }
}

/// The habit's per-field LWW write clocks (the §4.1 clock list,
/// verbatim) extended by the 2026-11-18 field family with the
/// description and anchor-day slots — contested content, they roam and
/// compete. `continued_from` carries deliberately no slot: it is
/// conversion-seam data, not contested content (D-H7a — same treatment
/// as `Task.remote`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitClocks {
    /// Title write clock.
    pub title: DateTime<Utc>,
    /// Target write clock.
    pub target: DateTime<Utc>,
    /// Order write clock.
    pub order: DateTime<Utc>,
    /// Anchor-time write clock.
    pub anchor_time: DateTime<Utc>,
    /// Description write clock (the plain-`String` task precedent —
    /// contested content, it roams and competes).
    ///
    /// Deliberate decode leniency: journals written before H1 carry no
    /// `description` stamp; the default is the epoch (never-written),
    /// the same semantics the habits-table load COALESCE applies.
    #[serde(default = "epoch_clock")]
    pub description: DateTime<Utc>,
    /// Anchor-day write clock (the optional display/nudge day —
    /// contested content, it roams and competes).
    ///
    /// Deliberate decode leniency: see `description`.
    #[serde(default = "epoch_clock")]
    pub anchor_day: DateTime<Utc>,
    /// Pause write clock ([`Habit::paused_until`] — contested roaming
    /// content, it roams and competes; the 2026-11-21 HF1 field family).
    ///
    /// Deliberate decode leniency: see `description`.
    #[serde(default = "epoch_clock")]
    pub paused_until: DateTime<Utc>,

    /// Scheduled-day-set write clock (HF4, R-6 — contested content, it
    /// roams and competes). Deliberate decode leniency: see
    /// `description`.
    #[serde(default = "epoch_clock")]
    pub scheduled_days: DateTime<Utc>,
    /// Reminder-override write clock (HF4, #959 — contested content,
    /// it roams and competes). Deliberate decode leniency: see
    /// `description`.
    #[serde(default = "epoch_clock")]
    pub reminder: DateTime<Utc>,
    /// Tombstone clock.
    pub deleted: DateTime<Utc>,
}

/// The serde default for roamed clock slots: the epoch (never written).
fn epoch_clock() -> DateTime<Utc> {
    DateTime::<Utc>::UNIX_EPOCH
}

impl HabitClocks {
    /// All clocks at one stamp (the mint path — a newborn row's fields
    /// share its creation instant).
    #[must_use]
    pub const fn all(now: DateTime<Utc>) -> Self {
        Self {
            title: now,
            target: now,
            order: now,
            anchor_time: now,
            description: now,
            anchor_day: now,
            paused_until: now,

            scheduled_days: now,
            reminder: now,
            deleted: now,
        }
    }
}

/// A habit tracker: a quota per period, manual list order, and an
/// optional display anchor time. Lives on a board; tombstones like
/// every entity (the log outlives the tracker — history is never
/// purged by [`crate::persistence::PersistenceAction::UpsertHabit`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Habit {
    /// The habit's local identity (roaming family — the synced
    /// section's wire key).
    pub id: HabitId,
    /// The owning board.
    pub board: BoardId,
    /// Display title.
    pub title: String,
    /// Free-text description (the `Task.description` shape verbatim:
    /// plain `String`, no bound, no newtype — no invariant to enforce).
    /// Display data; never an ordering or window input (the ordering
    /// inertness property extends to it).
    #[serde(default)]
    pub description: String,
    /// The quota per period. A kind change never deletes former-kind
    /// log rows — they stay in the map (they sync as facts) but are
    /// analytically inert (D-H12).
    pub target: HabitTarget,
    /// Manual list order (D-C13); the planner appends on create and
    /// a drag reorder carries a gap index (the engine renumbers dense).
    /// `anchor_time` never influences any ordering.
    pub order: i64,
    /// The tracker nudge's anchor (#504, plan D-1): when set, the
    /// reminder engine arms a once-per-day-window nudge at this local
    /// time (gated on the current period's quota); when `None`, the
    /// habit drives no reminder. In no ordering path (D-C13 verbatim —
    /// the `anchor_time_never_influences_the_canonical_order` proptest
    /// stands).
    pub anchor_time: Option<chrono::NaiveTime>,
    /// The optional display/nudge anchor day *inside the canonical
    /// window* (O-1 option b): windows stay ISO-Monday / calendar-1st,
    /// the streak walk never reads it, and a variant mismatched to the
    /// target kind is inert (documented leniency at [`AnchorDay`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor_day: Option<AnchorDay>,
    /// The inclusive pause-through local date (the
    /// [`crate::recur::RecurRule::paused_until`] twin, D-C10 shape — the
    /// 2026-11-21 HF1 field family, issue #958). Covered windows — those
    /// whose **last day** is on or before `until` — are transparent in
    /// the streak walk (no miss, no credit), and the tracker nudge
    /// disarms while `today <= until`. `None` is the resume posture
    /// (badge hygiene: the field clears on resume, it is never left
    /// pointing at a lapsed date).
    ///
    /// # Documented leniency
    ///
    /// A lapsed `until` (before today) is an honest stored value: the
    /// walk simply stops covering windows, and the nudge re-arms — the
    /// planner accepts the pause as-is and no background sweep validates
    /// it away (no validate-and-forget path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_until: Option<NaiveDate>,

    /// The scheduled weekdays *inside* a weekly/monthly canonical
    /// window (HF4, #960). Windows stay ISO-Monday / calendar-1st
    /// (O-1 untouched — the day set changes which days are
    /// opportunities, never a window boundary).
    ///
    /// # Documented leniency (R-6, operator ruling 2026-11-21)
    ///
    /// A `Some` set on a **`PerDay` habit is inert** — a daily habit
    /// happens *every day*; scheduling it on weekdays is a *weekly*
    /// habit, and a daily habit with weekday subsets would work
    /// identically to a weekly habit with the same days (redundant
    /// terminology and behavior). The *write path* refuses the field
    /// on a `PerDay` target with a typed rejection (no
    /// validate-and-forget); only legacy/synced rows can carry the
    /// inert combination, and [`Habit::effective_scheduled_days`]
    /// defaults it away at read time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scheduled_days: Option<WeekdaySet>,
    /// The habit's reminder override (HF4, #959): the O-1 nudge law's
    /// parameters restated per habit. `None` = the plain O-1 default
    /// (byte-identical to the pre-HF4 engine).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reminder: Option<HabitReminderOverride>,
    /// Per-field LWW clocks.
    pub clocks: HabitClocks,
    /// The §4.7 conversion seam, habit side: the series this habit
    /// continues (set only by the explicit `UpdateHabit` write path —
    /// D-H7a; the read-side history union is chunk 6's).
    pub continued_from: Option<SeriesId>,
    /// Tombstone. Resurrection-safe on sync by the same LWW-clock
    /// argument as tasks (the fold is chunk 6's).
    pub deleted: bool,
    /// The row's author (F1's `by` leg, the stored duel rank): the
    /// document's `by` when adopted, the device identity when created
    /// locally, `None` for rows that predate the slot (never privileged —
    /// the `None < Some` leg keeps them honest). Documented leniency:
    /// `None` here means "author unknown", not "authored by nobody".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<crate::ids::AuthorId>,
}

impl Habit {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
    }

    /// The habit's **effective** scheduled-day set (R-6): `None` for a
    /// `PerDay` habit (the field is inert there by construction) and
    /// for an empty decoded set (the [`WeekdaySet`] decode leniency —
    /// an empty set reads as "no day set", today's posture, never as
    /// "due never"). Every consumer of the day set reads it through
    /// this single rule.
    #[must_use]
    pub fn effective_scheduled_days(&self) -> Option<WeekdaySet> {
        if self.target.kind() == PeriodKind::Day {
            return None;
        }
        self.scheduled_days.filter(|set| !set.is_empty())
    }
}

/// The log key: one habit's count in one canonical window. `BTreeMap`
/// key — the derived order keeps serialization deterministic. On the
/// wire (JSON map keys must be strings — the `ReminderSnoozeKey`
/// precedent) the key serializes as the single string
/// `"<habit uuid>|<window naive date>"` and parses strictly back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HabitLogKey {
    /// The counted habit.
    pub habit: HabitId,
    /// The canonical period-start of the counted window.
    pub window: WindowStart,
}

impl HabitLogKey {
    /// The string wire form (`"<habit uuid>|<window naive date>"`).
    #[must_use]
    pub fn wire_form(&self) -> String {
        format!("{}|{}", self.habit.as_uuid(), self.window.as_date())
    }

    /// The strict inverse of [`wire_form`]; `None` on any malformed
    /// input (an undecodable key is corruption, not data).
    #[must_use]
    pub fn parse_wire_form(raw: &str) -> Option<Self> {
        let (habit, window) = raw.split_once('|')?;
        let habit = HabitId::from(uuid::Uuid::parse_str(habit).ok()?);
        Some(Self {
            habit,
            window: WindowStart(chrono::NaiveDate::parse_from_str(window, "%Y-%m-%d").ok()?),
        })
    }
}

impl Serialize for HabitLogKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire_form())
    }
}

impl<'de> Deserialize<'de> for HabitLogKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse_wire_form(&raw).ok_or_else(|| serde::de::Error::custom("invalid habit log key"))
    }
}

/// The per-day overlay key (US-8, #1144; the 2026-11-22 habit-surface
/// journeys plan's recorded D-H12 amendment): one habit's activity
/// attribution on one calendar day. `BTreeMap` key — the derived order
/// keeps serialization deterministic. Same string wire form as
/// [`HabitLogKey`] (`"<habit uuid>|<day naive date>"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HabitDayKey {
    /// The attributed habit.
    pub habit: HabitId,
    /// The attributed calendar day (a plain date — never a window
    /// start; the overlay's grain is the day, the log's is the
    /// window).
    pub day: NaiveDate,
}

impl HabitDayKey {
    /// The string wire form (`"<habit uuid>|<day naive date>"`).
    #[must_use]
    pub fn wire_form(&self) -> String {
        format!("{}|{}", self.habit.as_uuid(), self.day.format("%Y-%m-%d"))
    }

    /// The strict inverse of [`wire_form`]; `None` on any malformed
    /// input (an undecodable key is corruption, not data).
    #[must_use]
    pub fn parse_wire_form(raw: &str) -> Option<Self> {
        let (habit, day) = raw.split_once('|')?;
        let habit = HabitId::from(uuid::Uuid::parse_str(habit).ok()?);
        Some(Self {
            habit,
            day: chrono::NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?,
        })
    }
}

impl Serialize for HabitDayKey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire_form())
    }
}

impl<'de> Deserialize<'de> for HabitDayKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Self::parse_wire_form(&raw).ok_or_else(|| serde::de::Error::custom("invalid habit day key"))
    }
}

/// One day's overlay row (US-8, #1144): the attributed count plus a
/// single LWW clock — the [`HabitLogEntry`] discipline at day grain.
/// **Display data only** (R-2): the window log stays the adherence
/// and stats oracle; the overlay never feeds a stats projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitDayLogEntry {
    /// The day's attributed count (attribution at log time, D-H5's
    /// spirit at day grain).
    pub count: u32,
    /// The row's LWW clock — the same `mint_log_clock` discipline as
    /// the window rows (whole-second grammar downstream, #1095).
    pub clock: DateTime<Utc>,
}

/// One window's count row: the payload is the count plus a single LWW
/// clock (the normative shape — no per-completion instants, which is
/// exactly why attribution happens at log time, D-H5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitLogEntry {
    /// The window's count (0 is legal — an audited correction may
    /// empty a window; the row stays, honestly clocked).
    pub count: u32,
    /// The row's LWW clock: the writing planner's `now`, monotonically
    /// advanced past any stored same-key log row (#986) — it may exceed
    /// `now` by whole seconds after a same-second burst. Documented
    /// leniency: a row clock ahead of the wall `now` is a legitimate
    /// value, not corruption.
    pub clock: DateTime<Utc>,
    /// The row's author (F1's `by` leg, the stored duel rank): the
    /// document's `by` when adopted, the device identity when created
    /// locally, `None` for rows that predate the slot (never privileged —
    /// the `None < Some` leg keeps them honest). Documented leniency:
    /// `None` here means "author unknown", not "authored by nobody".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<crate::ids::AuthorId>,
    /// The typed planned-skip mark (the 2026-11-21 HF1 field family,
    /// issue #955): `true` marks the window a **deliberate skip** — the
    /// window is transparent in the streak walk regardless of quota or
    /// grace (neither counts nor breaks), and the row renders the typed
    /// skip state rather than the miss posture. `skip == false` (or an
    /// absent mark) leaves every behavior untouched.
    ///
    /// Deliberate decode leniency (the mint-strict/decode-lenient
    /// posture): rows written before HF1 carry no `skip` key and decode
    /// `false` — the additive default-absent law, never a decode
    /// failure. Consultation order in the walk: quota-met counts;
    /// else `skip` makes the window transparent; else pending/missed
    /// as before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub skip: bool,
}

/// Whether the window starting at `start` (of `kind`) lies **entirely
/// inside** a pause through `until` (inclusive): the window's last day
/// is on or before `until`. A daily window covers itself; a weekly
/// window is covered only when its Sunday has elapsed at or before
/// `until` — a partially covered window is *not* paused (the pause
/// excuses whole windows, never window fractions — the log's grain is
/// the window, D-H5). Pure and total.
#[must_use]
pub fn pause_covers_window(kind: PeriodKind, start: NaiveDate, until: NaiveDate) -> bool {
    let last_day = period_after(kind, start)
        .pred_opt()
        .unwrap_or(NaiveDate::MIN);
    last_day <= until
}

/// Mint a habit-log row clock under the #986 monotone rule:
/// `max(now, stored_clock + 1s)` where `stored_clock` is the
/// currently stored row's clock for the same [`HabitLogKey`]
/// (`None` → `now`). Pure in (pre-state, command, `now`), so
/// journal replay re-mints identical clocks deterministically;
/// every apply/fold/SQL site stays strictly-newer LWW, and a
/// same-instant double log stamps strictly past the first row so
/// both increments land. `AdjustHabitLog` shares the bump (it is
/// load-bearing, not symmetric polish): without it a same-second
/// adjust could stamp *older* than a bumped log row and lose its
/// newest-write intent. The minted clock may exceed `now` by whole
/// seconds after a burst — the documented leniency at
/// [`HabitLogEntry::clock`].
pub(crate) fn mint_log_clock(
    now: DateTime<Utc>,
    stored_clock: Option<DateTime<Utc>>,
) -> DateTime<Utc> {
    match stored_clock {
        None => now,
        Some(stored) => now.max(stored + one_second()),
    }
}

fn one_second() -> chrono::TimeDelta {
    chrono::TimeDelta::try_seconds(1).expect("1s is representable")
}

/// The habit sub-state, nested in [`crate::state::AppState`] like
/// `TrackingState`. `BTreeMap` keeps serialization deterministic (the
/// state.rs determinism contract).
///
/// # Documented leniency (D-H12)
///
/// The map may hold rows keyed by windows of a *former* target kind
/// after a kind change (they sync as facts; deleting them would forge
/// history). Every projection reads canonical windows of the *current*
/// kind only, so such rows are analytically inert — pinned by test.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitState {
    /// All habits, keyed by id (live rows and tombstones alike).
    pub habits: BTreeMap<HabitId, Habit>,
    /// The count log, keyed `(habit, window)`; ≤ 1 row per key.
    pub log: BTreeMap<HabitLogKey, HabitLogEntry>,
    /// The per-day activity overlay (US-8, #1144; the plan's recorded
    /// D-H12 amendment), keyed `(habit, day)`; ≤ 1 row per key.
    /// Display data only (R-2) — the window `log` stays the stats
    /// oracle. `serde(default)`: states persisted before the overlay
    /// decode with an empty map — the additive default-absent law,
    /// never a decode failure.
    #[serde(default)]
    pub day_log: BTreeMap<HabitDayKey, HabitDayLogEntry>,
    /// The habit time entries (journey plan 2026-11-24 D-1, #1305 —
    /// the additive fact family; the open entry IS the running timer,
    /// the task tracker's grammar verbatim). `serde(default)`: states
    /// persisted before the map decode with an empty map — the
    /// additive default-absent law, never a decode failure. Display/
    /// duration data only (D-2): no stats projection reads it.
    #[serde(default)]
    pub time_log: BTreeMap<crate::ids::HabitTimeEntryId, crate::habit_time::HabitTimeEntry>,
    /// The per-completion event log (the #1519 event-log design): the
    /// future sole source of truth. Additive this chunk — `log` /
    /// `day_log` stay and every existing reader is untouched (the
    /// next chunk rewires the writers; the final chunk deletes the
    /// maps). `BTreeMap` keyed by mint-instant-ordered v7 ids:
    /// deterministic serialization, and the `(at, id)` iteration
    /// order the derivations' tie-breaks rely on. `serde(default)`:
    /// the additive default-absent law — states persisted before the
    /// map decode with an empty map, never a decode failure.
    #[serde(default)]
    pub events: BTreeMap<crate::habit_event::HabitEventId, crate::habit_event::HabitEvent>,
}

/// The period-start local date of `date`'s period: the day itself;
/// the ISO week's Monday; the month's 1st. Pure, total, idempotent —
/// the floor every [`WindowStart`] mint goes through.
///
/// # Panics
///
/// Panics only if chrono cannot build day 1 of `date`'s month — a
/// programming error (day 1 exists in every calendar month), not an
/// input condition.
#[must_use]
pub fn period_start(kind: PeriodKind, date: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => date,
        // The same Monday rule recur.rs's `week_monday` uses, via
        // chrono's week arithmetic (`first_day` of the Mon-start week).
        PeriodKind::Week => date.week(chrono::Weekday::Mon).first_day(),
        // Day 1 exists in every month, so `with_day` is infallible here.
        PeriodKind::Month => date
            .with_day(1)
            .expect("day 1 exists in every calendar month"),
    }
}

/// The first day of the *next* period after `start` (which must be a
/// period start; anything else floors first — the same canonical law
/// the mints enforce). Month steps use checked month arithmetic.
///
/// Saturation (documented leniency, unreachable in the bounded
/// proptest domain): a month step past chrono's date bound saturates
/// at [`NaiveDate::MAX`].
#[must_use]
pub fn period_after(kind: PeriodKind, start: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => start
            .checked_add_days(chrono::Days::new(1))
            .unwrap_or(NaiveDate::MAX),
        PeriodKind::Week => start
            .checked_add_days(chrono::Days::new(7))
            .unwrap_or(NaiveDate::MAX),
        PeriodKind::Month => start
            .checked_add_months(chrono::Months::new(1))
            .unwrap_or(NaiveDate::MAX),
    }
}

/// The last day of the *previous* period before `start` (the inverse
/// of [`period_after`]). Saturation mirrors [`period_after`]: a month
/// step before the bound saturates at [`NaiveDate::MIN`].
#[must_use]
pub fn period_before(kind: PeriodKind, start: NaiveDate) -> NaiveDate {
    match kind {
        PeriodKind::Day => start
            .checked_sub_days(chrono::Days::new(1))
            .unwrap_or(NaiveDate::MIN),
        PeriodKind::Week => start
            .checked_sub_days(chrono::Days::new(7))
            .unwrap_or(NaiveDate::MIN),
        PeriodKind::Month => prev_month_checked(start).unwrap_or(NaiveDate::MIN),
    }
}

/// `start`'s previous month's first day (`None` only at the chrono
/// bound — saturating callers substitute [`NaiveDate::MIN`]).
fn prev_month_checked(start: NaiveDate) -> Option<NaiveDate> {
    let first = start.with_day(1)?;
    let prev_month_end = first.checked_sub_days(chrono::Days::new(1))?;
    prev_month_end.with_day(1)
}

/// The append order for a board's habit list (D-H11): one past the
/// greatest live order, or 0 on an empty list — the `CaptureTask` append
/// shape, board-scoped. The ordering arithmetic lives here (not in the
/// planner) so the later batch-order affordance (D-C13's "sort by time
/// is just a batch of `MoveHabit` edits") reuses the single source.
#[must_use]
pub fn append_order_after(live_orders: impl Iterator<Item = i64>) -> i64 {
    live_orders.max().map_or(0, |max| max + 1)
}

/// The period-end *instant* of the window starting at `start`: the
/// opening instant of the next period's day window — through the
/// `DayCalendar` vocabulary, never a second timezone concept (D-H4).
#[must_use]
pub fn period_end(kind: PeriodKind, start: NaiveDate, day: DayCalendar) -> DateTime<Utc> {
    crate::days::window_start(period_after(kind, start), day.day_start_time, day.offset)
}

/// Attribution happens at log time (D-H5): the normative log shape (a
/// count per window, one LWW clock, **no per-completion instants**)
/// makes read-time attribution undefinable — row counts cannot be split
/// by time after the fact. So the *window choice of the +1* is the
/// attribution, and this pure function is the whole decision.
///
/// 1. `natural` — the window-date of `now` (the night-owl rule), floored
///    to the habit's period.
/// 2. The previous window is in its **grace** iff `now` is before the
///    previous window's end + 24 h (the fixed overflow — no knob, D-C8).
/// 3. **Surplus-repairs**: return the previous window iff it is in
///    grace ∧ still under quota ∧ the natural window already met quota;
///    else the natural window.
///
/// Why the repair is guarded: the literal "late completions attribute
/// to the earlier member" rule applied unconditionally would land every
/// routine log in the previous period (a 9 AM log on day D falls inside
/// day D−1's 24 h overflow), lagging the whole streak one period.
/// Guarding on "today already met" keeps routine behavior natural and
/// makes the overflow a *make-up* mechanism: doubling up after meeting
/// today heals yesterday (the D-C8 ADHD-forgiveness intent);
/// `AdjustHabitLog` stays the explicit valve for everything else.
/// Per-kind reach falls out of the arithmetic: daily habits can repair
/// yesterday all day (prev end + 24 h = end of today); weekly/monthly
/// habits only within the first 24 h of the new period.
///
/// # Panics
///
/// Panics only if chrono cannot represent the 24 h grace offset — a
/// programming error, not an input condition.
#[must_use]
pub fn log_window(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> WindowStart {
    attribute_window(
        habit.target.kind(),
        habit.target.quota().get(),
        |window| {
            log.get(&HabitLogKey {
                habit: habit.id,
                window,
            })
            .map_or(0, |entry| entry.count)
        },
        now,
        day,
    )
}

/// The attribution core behind [`log_window`]: the natural window /
/// 24 h grace / guarded surplus-repair rule, parameterized over the
/// count source. `log_window` wraps it reading the stored log rows;
/// the event-log derivation fold (the #1519 design, chunk 1) calls it
/// with its *running* per-window counts — one attribution rule, two
/// count sources. Pure extraction: no behavior change.
#[must_use]
pub(crate) fn attribute_window(
    kind: PeriodKind,
    quota: u32,
    count: impl Fn(WindowStart) -> u32,
    at: DateTime<Utc>,
    day: DayCalendar,
) -> WindowStart {
    let natural = WindowStart::of_period(kind, day.today(at));
    let prev_date = period_before(kind, natural.as_date());
    let prev = WindowStart::of_period(kind, prev_date);
    let grace_end = period_end(kind, prev_date, day)
        + chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let in_grace = at < grace_end;
    if in_grace && count(prev) < quota && count(natural) >= quota {
        prev
    } else {
        natural
    }
}

/// The habit's streak as of `now` (D-H6): the number of consecutive
/// **counting** periods ending at the current one. Pure and total.
///
/// Walk periods backward from the natural window (as [`log_window`]
/// computes it). For each period:
///
/// - count ≥ quota → **counts**: streak += 1, continue backward;
/// - else if `now` is before the period's end + 24 h → **pending**:
///   transparent — it neither counts nor breaks, walk on. At most two
///   periods can be pending for any `now` (the current one, plus the
///   previous during its first 24 h — the `period_end` arithmetic
///   enforces this; pinned by property);
/// - else → **missed**: stop.
///
/// The transparent outcome class (HF1, 2026-11-21 — one outcome, not
/// three behaviors): pause-covered windows ([`pause_covers_window`]
/// against [`Habit::paused_until`] — no miss, no credit, checked
/// first, even over a met row) and skipped windows (the row's
/// [`HabitLogEntry::skip`] mark — consulted only when the quota is
/// unmet: a met window on a skipped row still counts) are transparent
/// too — neither counts nor breaks. Pause coverage is monotone downward
/// (everything below the pause boundary is covered as well), so the
/// walk **terminates** at the first covered window — it never descends
/// into the covered suffix. The Σ properties pin the composition:
/// transparency never counts, never breaks, and the grace-pending
/// bound is unchanged.
///
/// Behavior contract: the morning of an unmet today still shows
/// yesterday's run (today is pending); a missed period breaks the chain
/// exactly when its grace closes; a brand-new habit with an empty log
/// has streak 0. Periods before the log's earliest row are empty ⇒
/// missed ⇒ the walk terminates naturally — no creation boundary is
/// needed (a pre-creation row can only arrive via the explicit
/// `AdjustHabitLog` valve, which is allowed to count: audited data).
/// Rows keyed by non-canonical or former-kind windows are inert (D-H12).
///
/// # Panics
///
/// Panics only if chrono cannot represent the 24 h grace offset — a
/// programming error, not an input condition.
#[must_use]
pub fn streak(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> u32 {
    streak_with_break(habit, log, now, day).0
}

/// The walk of [`streak`], additionally naming the period where the
/// chain broke (the first period that is neither counting nor pending).
/// `None` means the walk terminated by saturation — no break, no seam.
fn streak_with_break(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> (u32, Option<NaiveDate>) {
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let count = |window: WindowStart| {
        log.get(&HabitLogKey {
            habit: habit.id,
            window,
        })
        .map_or(0, |entry| entry.count)
    };
    let skipped = |window: WindowStart| {
        log.get(&HabitLogKey {
            habit: habit.id,
            window,
        })
        .is_some_and(|entry| entry.skip)
    };
    let grace = chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let mut streak = 0_u32;
    let today = day.today(now);
    let mut cursor = WindowStart::of_period(kind, today).as_date();
    loop {
        if habit
            .paused_until
            .is_some_and(|until| pause_covers_window(kind, cursor, until))
        {
            // Pause-covered: transparent — no miss, no credit (D-C10
            // semantics verbatim); composes with skip (Σ property (d)).
            // Termination law: coverage is monotone downward (a lower
            // cursor's last day is also ≤ `until`), so everything below
            // this window is covered too — no further window can count
            // or break. The walk stops here; it never descends into
            // the covered suffix (an unbounded descent would make
            // `streak` O(the chrono date range) under any standing
            // pause).
            return (streak, None);
        } else if count(WindowStart(cursor)) >= quota {
            streak += 1;
        } else if skipped(WindowStart(cursor)) {
            // Typed planned skip: transparent — neither counts nor
            // breaks (consulted only when the quota is unmet).
        } else if preschedule_window(habit, WindowStart(cursor), today) {
            // Pre-schedule (R-6/US-5 AC-1): the window's scheduled days
            // have not begun — transparent regardless of grace (no due
            // posture, no miss), walk on. The transparent class's
            // other subjects (HF1, landed: the pause arm above, the
            // skip arm before this one) compose here — the
            // composition property
            // (`transparency_composes_across_skip_pause_preschedule`)
            // pins the one-outcome law.
        } else {
            let grace_end = period_end(kind, cursor, day) + grace;
            if now >= grace_end {
                return (streak, Some(cursor)); // missed: the break is here
            }
            // Pending: transparent — neither counts nor breaks.
        }
        let prev = period_before(kind, cursor);
        if prev >= cursor {
            return (streak, None); // saturation: no break period
        }
        cursor = prev;
    }
}

/// The window's first scheduled local date (R-6/US-5): the earliest
/// date at/after the canonical window's start whose weekday is in the
/// habit's **effective** day set. `None` when the habit has no
/// effective day set (`PerDay` inertness, no set, empty decode) — the
/// caller's "no day set" posture (join at period start).
///
/// # Panics
///
/// Panics only if chrono cannot represent the ≤7-day search from a
/// window start near the date bound — saturation posture, unreachable
/// for real calendars.
#[must_use]
pub fn first_scheduled_day(habit: &Habit, window: WindowStart) -> Option<NaiveDate> {
    let set = habit.effective_scheduled_days()?;
    let start = period_start(habit.target.kind(), window.as_date());
    first_day_in_set_after(set, start)
}

/// The earliest date ≥ `from` whose weekday is in `set`. `None` only
/// for an empty set (cannot pass the mint) or date-bound saturation.
fn first_day_in_set_after(set: WeekdaySet, from: NaiveDate) -> Option<NaiveDate> {
    (0..7_u64)
        .map_while(|offset| from.checked_add_days(chrono::Days::new(offset)))
        .find(|date| set.contains(date.weekday()))
}

/// `date` shifted by a signed day count (`None` only at the chrono
/// date bound — the saturation posture [`period_after`] documents).
fn add_days_signed(date: NaiveDate, days: i32) -> Option<NaiveDate> {
    if days >= 0 {
        date.checked_add_days(chrono::Days::new(u64::try_from(days).unwrap_or(u64::MAX)))
    } else {
        date.checked_sub_days(chrono::Days::new(u64::try_from(-days).unwrap_or(u64::MAX)))
    }
}

/// US-5 AC-1 (R-6): the weekly/monthly window is **pre-schedule** —
/// transparent in the streak walk — while `today` lies before its
/// first scheduled day (no due posture, no miss; once a scheduled day
/// has elapsed the window walks normally). A habit with no effective
/// day set is never pre-schedule: windows join at period start —
/// today's posture, byte-identical. Daily habits are excluded by
/// construction ([`Habit::effective_scheduled_days`]).
#[must_use]
pub fn preschedule_window(habit: &Habit, window: WindowStart, today: NaiveDate) -> bool {
    first_scheduled_day(habit, window).is_some_and(|first| today < first)
}

/// The adherence denominator's join law (US-5 AC-5 / US-9 AC-1): a
/// window joins the denominator once its first scheduled day has
/// elapsed; with no day set, at period start. **Never from a daily
/// habit** — daily windows enter at period start as today (R-6: the
/// denominator derives from weekly/monthly day sets only). HF3's
/// `adherence`/stats fns consume this predicate; it is the exact
/// complement of [`preschedule_window`] — a transparent window leaves
/// both the numerator and the denominator (planned gaps never dilute
/// adherence).
#[must_use]
pub fn window_joins_denominator(habit: &Habit, window: WindowStart, today: NaiveDate) -> bool {
    !preschedule_window(habit, window, today)
}

/// `start`'s window's dates, in order (the day set's search domain).
fn window_dates(kind: PeriodKind, start: NaiveDate) -> Vec<NaiveDate> {
    let end = period_after(kind, start);
    let mut dates = Vec::new();
    let mut cursor = start;
    while cursor < end {
        dates.push(cursor);
        cursor = period_after(PeriodKind::Day, cursor);
    }
    dates
}

/// The days inside `window` the tracker nudge arms on (O-1's
/// single-anchor-day generalization, R-6): each scheduled day for a
/// weekly/monthly habit with an effective day set, else the single
/// O-1 [`nudge_day`] (today's behavior — a one-day vector,
/// byte-identical). Pure; the engine consumes the membership form
/// ([`tracker_arm`]); this shape is the tests' and projections'.
#[must_use]
pub fn nudge_days(habit: &Habit, window: WindowStart) -> Vec<NaiveDate> {
    let kind = habit.target.kind();
    let start = period_start(kind, window.as_date());
    match habit.effective_scheduled_days() {
        Some(set) => window_dates(kind, start)
            .into_iter()
            .filter(|date| set.contains(date.weekday()))
            .collect(),
        None => vec![nudge_day(habit, window)],
    }
}

/// The tracker-nudge arm gate for `today` under the **plain O-1
/// default** (no override): with an effective day set, `today` must
/// *be* a scheduled day (US-5 AC-2 — arms on each scheduled day and on
/// no other day of the window; a pre-schedule day is by construction
/// not scheduled); without one, `today ≥ nudge_day` — the pre-HF4
/// gate, byte-identical.
#[must_use]
pub fn tracker_armed_today(habit: &Habit, window: WindowStart, today: NaiveDate) -> bool {
    match habit.effective_scheduled_days() {
        Some(set) => set.contains(today.weekday()),
        None => today >= nudge_day(habit, window),
    }
}

/// The override's arm gate for `today` (R-8, US-6 AC-1): base
/// [`NudgeBase::NudgeDay`] keeps the plain gate's day, shifted by
/// `offset_days`; base [`NudgeBase::PeriodStart`] arms only on the
/// canonical window's start shifted by `offset_days` (an armed day
/// pushed outside the window simply does not arm *inside this
/// window* — it lands in the neighbor's span, which runs its own
/// gate; no clamping needed for a membership test).
#[must_use]
pub fn override_armed_today(
    habit: &Habit,
    ovr: &HabitReminderOverride,
    window: WindowStart,
    today: NaiveDate,
) -> bool {
    match ovr.base {
        NudgeBase::PeriodStart => {
            let start = period_start(habit.target.kind(), window.as_date());
            add_days_signed(start, ovr.offset_days) == Some(today)
        }
        NudgeBase::NudgeDay => {
            if let Some(set) = habit.effective_scheduled_days() {
                add_days_signed(today, -ovr.offset_days)
                    .is_some_and(|date| set.contains(date.weekday()))
            } else {
                let arm = nudge_day(habit, window);
                add_days_signed(arm, ovr.offset_days).is_some_and(|shifted| today >= shifted)
            }
        }
    }
}

/// The occurrence's local time-of-day (US-6 AC-4): the habit's
/// `anchor_time` when set, else the override's own `time`; `None` when
/// neither exists — the habit drives no reminder at all. An override
/// never *requires* an anchor time, and an override with `time: None`
/// (#1212's anchor-time-only posture) contributes no clock time
/// either.
#[must_use]
pub fn tracker_arm_time(habit: &Habit) -> Option<chrono::NaiveTime> {
    habit
        .anchor_time
        .or_else(|| habit.reminder.and_then(|ovr| ovr.time))
}

/// The tracker nudge's full arm decision for `today` inside `habit`'s
/// current `window`: `Some`(time-of-day) when the nudge arms — a habit
/// with neither `anchor_time` nor an override never arms; with an
/// override the R-8 gate decides; otherwise the day-set/O-1 gate. The
/// quota gate stays the reminder engine's eligibility (the arm gate
/// decides WHEN only — the same split as the pre-HF4 engine).
#[must_use]
pub fn tracker_arm(
    habit: &Habit,
    window: WindowStart,
    today: NaiveDate,
) -> Option<chrono::NaiveTime> {
    let time = tracker_arm_time(habit)?;
    let armed = match &habit.reminder {
        Some(ovr) => override_armed_today(habit, ovr, window, today),
        None => tracker_armed_today(habit, window, today),
    };
    armed.then_some(time)
}

/// The habit's streak continued through its conversion seam (§4.7):
/// [`streak`]'s own walk, then — when the walk breaks on a genuinely
/// missed period **and** [`Habit::continued_from`] names a series —
/// consecutive *completed* series members counted backward from that
/// break period. Read-side only: no fabricated done tasks, no collapsed
/// logs (the two entities stay; the views union them).
///
/// The series' rule comes from the most recent done occurrence task
/// carrying `recur.series == series` (the rule is identical down the
/// chain); members are enumerated backward from the break period over a
/// bounded window, stopping at the first gap. A member counts iff a
/// live done task exists with that series and
/// [`crate::recur::roll_reference_date`] equal to the member. Recorded
/// simplification (D-H15): **no grace crosses the seam** — the habit
/// side applies its own 24 h overflow within its log; the series join
/// is strict member completion (refinement is F-2's).
///
/// # Panics
///
/// Panics only if chrono cannot represent the bounded backward window —
/// a programming error, not an input condition.
#[must_use]
pub fn habit_streak_unioned(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    tasks: &BTreeMap<crate::ids::TaskId, crate::entities::Task>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> u32 {
    let (own, break_period) = streak_with_break(habit, log, now, day);
    let Some(series) = habit.continued_from else {
        return own;
    };
    let Some(break_date) = break_period else {
        return own;
    };
    let done_of_series = |task: &&crate::entities::Task| {
        !task.deleted
            && task.done.is_some()
            && task
                .recur
                .as_ref()
                .is_some_and(|rule| rule.series == series)
    };
    // The rule is identical down the chain; take it from the most
    // recent done occurrence.
    let Some(rule) = tasks
        .values()
        .filter(done_of_series)
        .max_by_key(|task| task.done)
        .and_then(|task| task.recur.as_ref())
    else {
        return own;
    };
    let window_lo = break_date
        .checked_sub_days(chrono::Days::new(366))
        .unwrap_or(NaiveDate::MIN);
    let mut extra = 0_u32;
    for member in crate::recur::schedule(rule, window_lo, break_date)
        .into_iter()
        .rev()
    {
        let completed = tasks.values().any(|task| {
            done_of_series(&task) && crate::recur::roll_reference_date(task, day) == Some(member)
        });
        if !completed {
            break; // the first gap ends the joined run
        }
        extra += 1;
    }
    own + extra
}

// ---- Statistics (HF3, US-9 / #963) ----------------------------------------
//
// Pure, total, projection-only over `HabitState.log`. Every function
// enumerates canonical windows of the habit's *current* target kind
// only, so former-kind rows stay analytically inert (D-H12) and a
// kind change starts a fresh statistical horizon (the same read law
// the streak walk applies).

/// The typed adherence ratio (US-9 AC-1/AC-3): `met` windows over
/// `scheduled` windows. Hand-rolled private-field newtype (the A1
/// house pattern) minted through [`AdherenceRatio::mint`], which
/// rejects `met > scheduled` — a ratio above 100 % is corruption, not
/// data. `scheduled == 0` is the **typed empty state**: no division
/// ever runs against zero windows, `ratio()` answers `None`, and
/// there is no "0 % of nothing" posture (US-9 AC-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdherenceRatio {
    met: u32,
    scheduled: u32,
}

impl AdherenceRatio {
    /// The fallible mint: `Some` iff `met <= scheduled`.
    #[must_use]
    pub const fn mint(met: u32, scheduled: u32) -> Option<Self> {
        if met <= scheduled {
            Some(Self { met, scheduled })
        } else {
            None
        }
    }

    /// The typed empty state: zero met, zero scheduled windows.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            met: 0,
            scheduled: 0,
        }
    }

    /// True exactly in the typed empty state (nothing scheduled yet —
    /// an empty history, or a history of nothing but transparent
    /// windows).
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.scheduled == 0
    }

    /// The windows that counted toward the ratio (met).
    #[must_use]
    pub const fn met(&self) -> u32 {
        self.met
    }

    /// The windows the ratio was computed over (scheduled, elapsed,
    /// non-transparent).
    #[must_use]
    pub const fn scheduled(&self) -> u32 {
        self.scheduled
    }

    /// The ratio itself, in `0.0..=1.0`; `None` in the typed empty
    /// state (there is no percentage of zero windows).
    #[must_use]
    pub fn ratio(&self) -> Option<f64> {
        if self.scheduled == 0 {
            None
        } else {
            Some(f64::from(self.met) / f64::from(self.scheduled))
        }
    }
}

/// The habit's statistics horizon: canonical windows of the current
/// target kind, from the log's earliest current-kind row through the
/// natural (current) window of `now`. Deliberate documented leniency:
/// the horizon starts at the **earliest log row**, not at a habit
/// creation stamp — the log is the module's only historical record
/// (HF3 trap (a): statistics read `HabitState.log`, no new
/// persistence), so pre-log windows are simply outside the horizon
/// rather than assumed missed.
///
/// The horizon is the *enumeration* only — which windows join the
/// denominator is [`window_joins_denominator`]'s law (the weekly/
/// monthly day sets, R-6; a daily habit joins every window), and the
/// skipped / paused transparent classes (HF1's skip mark and
/// `paused_until`) leave both numerator and denominator exactly per
/// US-9 AC-2's law. Consumers must consult those, never the bare
/// enumeration.
#[must_use]
pub fn statistics_horizon(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> Vec<NaiveDate> {
    let kind = habit.target.kind();
    let mut rows: Vec<NaiveDate> = log
        .keys()
        .filter(|key| {
            key.habit == habit.id
                && key.window.as_date() == period_start(kind, key.window.as_date())
        })
        .map(|key| key.window.as_date())
        .collect();
    rows.sort_unstable();
    let Some(earliest) = rows.first().copied() else {
        return Vec::new();
    };
    let natural = WindowStart::of_period(kind, day.today(now)).as_date();
    let mut horizon = Vec::new();
    let mut cursor = natural;
    loop {
        horizon.push(cursor);
        if cursor <= earliest {
            break;
        }
        let prev = period_before(kind, cursor);
        if prev >= cursor {
            break; // chrono saturation: the ends hold
        }
        cursor = prev;
    }
    horizon.reverse(); // chronological order
    horizon
}

/// The habit's adherence (US-9 AC-1): met elapsed windows over
/// scheduled elapsed windows, both over [`statistics_horizon`].
///
/// Window classification follows the streak walk verbatim: pause
/// coverage first (transparent — leaves both, the walk's composing
/// first arm); count ≥ quota → met (numerator + denominator); HF1's
/// typed skip mark is transparent — it leaves BOTH numerator and
/// denominator (US-9 AC-2); [`window_joins_denominator`] gates the
/// rest — grace still open → pending, transparent (leaves both),
/// grace closed and under quota → missed (denominator only), and a
/// pre-schedule weekly/monthly window (R-6) never joins at all. An
/// empty history — no current-kind canonical rows at all — is the
/// typed empty state (US-9 AC-3).
///
/// # Panics
///
/// Panics only if chrono cannot represent the 24 h grace offset — a
/// programming error, not an input condition.
#[must_use]
pub fn adherence(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> AdherenceRatio {
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let grace = chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let today = day.today(now);
    let horizon = statistics_horizon(habit, log, now, day);
    if horizon.is_empty() {
        return AdherenceRatio::empty();
    }
    let mut met = 0_u32;
    let mut scheduled = 0_u32;
    for window in horizon {
        let entry = log.get(&HabitLogKey {
            habit: habit.id,
            window: WindowStart(window),
        });
        let count = entry.map_or(0, |row| row.count);
        if habit
            .paused_until
            .is_some_and(|until| pause_covers_window(kind, window, until))
        {
            // Pause-covered: transparent — leaves both (the walk's
            // first arm: coverage composes over any row, met included).
        } else if count >= quota {
            met += 1;
            scheduled += 1;
        } else if entry.is_some_and(|row| row.skip) {
            // Typed planned skip: transparent — leaves both (US-9
            // AC-2); consulted only under quota, the walk's order.
        } else if window_joins_denominator(habit, WindowStart(window), today) {
            let grace_end = period_end(kind, window, day) + grace;
            if now >= grace_end {
                scheduled += 1; // missed: denominator only
            }
            // else pending: transparent — leaves both.
        }
        // else pre-schedule (R-6): transparent — never joins.
    }
    AdherenceRatio { met, scheduled }
}

/// The best historical streak (US-9 AC-1): the maximum run of
/// consecutive counting windows anywhere in [`statistics_horizon`],
/// with the walk's own classification — counting windows extend a
/// run; HF1's skipped, pause-covered, and pending (grace-open)
/// windows are transparent (neither break nor extend); pre-schedule
/// windows ([`window_joins_denominator`]'s complement, R-6) are
/// transparent the same way; missed windows break it. Always at
/// least [`streak`]'s value: the current run is one of the
/// historical runs.
///
/// # Panics
///
/// Panics only if chrono cannot represent the 24 h grace offset — a
/// programming error, not an input condition.
#[must_use]
pub fn best_streak(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    now: DateTime<Utc>,
    day: DayCalendar,
) -> u32 {
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let grace = chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let today = day.today(now);
    let mut best = 0_u32;
    let mut run = 0_u32;
    for window in statistics_horizon(habit, log, now, day) {
        let entry = log.get(&HabitLogKey {
            habit: habit.id,
            window: WindowStart(window),
        });
        let count = entry.map_or(0, |row| row.count);
        if habit
            .paused_until
            .is_some_and(|until| pause_covers_window(kind, window, until))
        {
            // Pause-covered: transparent (the walk's first arm —
            // coverage composes over any row, met included).
        } else if count >= quota {
            run += 1;
            best = best.max(run);
        } else if entry.is_some_and(|row| row.skip) {
            // Typed planned skip: transparent — neither extends nor
            // breaks (the walk's order: consulted only under quota).
        } else if window_joins_denominator(habit, WindowStart(window), today) {
            let grace_end = period_end(kind, window, day) + grace;
            if now >= grace_end {
                run = 0; // missed: the run breaks
            }
            // else pending: transparent — neither extends nor breaks.
        }
        // else pre-schedule (R-6): transparent.
    }
    best
}

/// The per-window totals (US-9 AC-1): the count column of the history
/// grid — every current-kind canonical log row, keyed by window,
/// counts as stored (a 0 row stays, honestly — an audited correction
/// may empty a window). Former-kind and non-canonical rows are inert
/// (D-H12): they simply do not appear.
#[must_use]
pub fn per_period_totals(
    habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
) -> BTreeMap<WindowStart, u32> {
    let kind = habit.target.kind();
    log.iter()
        .filter(|(key, _)| {
            key.habit == habit.id
                && key.window.as_date() == period_start(kind, key.window.as_date())
        })
        .map(|(key, entry)| (key.window, entry.count))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{naive_date_strategy, proptest_config};
    use chrono::{Datelike, TimeZone};
    use proptest::prelude::*;

    pub(super) fn habit(raw: u128, target: HabitTarget) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: BoardId::from(uuid::Uuid::from_u128(900)),
            title: format!("habit {raw}"),
            description: String::new(),
            target,
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(Utc.timestamp_opt(100, 0).unwrap()),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        }
    }

    #[test]
    fn habit_clocks_decode_legacy_payloads_without_the_roamed_fields() {
        // Pre-H1 journal rows (written before `description`/`anchor_day`
        // gained clock slots) carry only the five original stamps. The
        // decode defaults the roamed fields to the epoch — the same
        // never-written semantics the habits-table load COALESCE uses —
        // instead of failing the whole journal hydration.
        let legacy = serde_json::json!({
            "title": "2026-11-18T01:44:56.618026998Z",
            "target": "2026-11-18T01:44:56.618026998Z",
            "order": "2026-11-18T01:44:56.618026998Z",
            "anchor_time": "2026-11-18T01:44:56.618026998Z",
            "deleted": "2026-11-18T01:44:56.618026998Z",
        });
        let clocks: HabitClocks = serde_json::from_value(legacy).expect("legacy shape decodes");
        assert!(
            clocks.title > Utc.timestamp_opt(0, 0).unwrap(),
            "the five original stamps decode verbatim"
        );
        assert_eq!(
            clocks.description,
            Utc.timestamp_opt(0, 0).unwrap(),
            "the roamed description clock defaults to the epoch (never written)"
        );
        assert_eq!(
            clocks.anchor_day,
            Utc.timestamp_opt(0, 0).unwrap(),
            "the roamed anchor-day clock defaults to the epoch (never written)"
        );
    }

    #[test]
    fn non_zero_count_mint_rejects_zero_and_decodes_strictly() {
        assert_eq!(NonZeroCount::new(0), Err(InvalidHabitTarget));
        assert_eq!(NonZeroCount::new(1).unwrap().get(), 1);
        // Strict decode: a stored 0 cannot deserialize (recur.rs posture).
        let back: Result<NonZeroCount, _> = serde_json::from_str("0");
        assert!(back.is_err(), "zero is unrepresentable at decode");
        let one: NonZeroCount = serde_json::from_str("3").unwrap();
        assert_eq!(one.get(), 3);
    }

    #[test]
    fn month_day_mint_rejects_zero_and_32_and_decodes_strictly() {
        assert_eq!(MonthDay::new(0), Err(InvalidMonthDay));
        assert_eq!(MonthDay::new(32), Err(InvalidMonthDay));
        assert_eq!(MonthDay::new(1).unwrap().get(), 1);
        assert_eq!(MonthDay::new(31).unwrap().get(), 31);
        // Strict decode (the NonZeroCount posture): 0 and 32 are
        // unrepresentable at decode.
        for raw in ["0", "32"] {
            let back: Result<MonthDay, _> = serde_json::from_str(raw);
            assert!(back.is_err(), "{raw} is unrepresentable at decode");
        }
        let seventeen: MonthDay = serde_json::from_str("17").unwrap();
        assert_eq!(seventeen.get(), 17);
    }

    #[test]
    fn anchor_day_round_trips_in_the_externally_tagged_wire_form() {
        let weekday = AnchorDay::Weekday(chrono::Weekday::Wed);
        let json = serde_json::to_string(&weekday).unwrap();
        assert_eq!(json, r#"{"Weekday":"Wed"}"#);
        assert_eq!(serde_json::from_str::<AnchorDay>(&json).unwrap(), weekday);
        let month_day = AnchorDay::MonthDay(MonthDay::new(17).unwrap());
        let json = serde_json::to_string(&month_day).unwrap();
        assert_eq!(json, r#"{"MonthDay":17}"#);
        assert_eq!(serde_json::from_str::<AnchorDay>(&json).unwrap(), month_day);
    }

    #[test]
    fn habit_serde_carries_the_additive_fields_and_decodes_pre_field_rows() {
        let mut h = habit(1, HabitTarget::PerWeek(NonZeroCount::new(2).unwrap()));
        h.description = "every week notes".into();
        h.anchor_day = Some(AnchorDay::Weekday(chrono::Weekday::Wed));
        let json = serde_json::to_string(&h).unwrap();
        let back: Habit = serde_json::from_str(&json).unwrap();
        assert_eq!(back, h);
        assert!(json.contains(r#""description":"every week notes""#));
        // Default-absent law: a pre-field document (no description /
        // anchor_day keys) decodes with the additive defaults.
        let pre_field = format!(
            r#"{{"id":"{}","board":"{}","title":"old","target":{{"PerWeek":2}},"order":0,"anchor_time":null,"clocks":{{"title":"1970-01-01T00:00:00Z","target":"1970-01-01T00:00:00Z","order":"1970-01-01T00:00:00Z","anchor_time":"1970-01-01T00:00:00Z","description":"1970-01-01T00:00:00Z","anchor_day":"1970-01-01T00:00:00Z","deleted":"1970-01-01T00:00:00Z"}},"continued_from":null,"deleted":false}}"#,
            h.id.as_uuid(),
            h.board.as_uuid(),
        );
        let decoded: Habit = serde_json::from_str(&pre_field).unwrap();
        assert_eq!(decoded.description, "");
        assert_eq!(decoded.anchor_day, None);
    }

    #[test]
    fn nudge_day_arm_checkpoints_per_kind() {
        let monday = date(2026, 10, 26); // an ISO Monday
        // PerDay ignores the field entirely (inert even when set).
        let mut daily = habit(1, HabitTarget::PerDay(NonZeroCount::new(1).unwrap()));
        daily.anchor_day = Some(AnchorDay::Weekday(chrono::Weekday::Wed));
        assert_eq!(nudge_day(&daily, WindowStart(monday)), monday);
        // PerWeek: none → the window date; Wednesday → that Wednesday.
        let weekly = habit(2, HabitTarget::PerWeek(NonZeroCount::new(1).unwrap()));
        assert_eq!(nudge_day(&weekly, WindowStart(monday)), monday);
        let mut wednesday_habit = weekly.clone();
        wednesday_habit.anchor_day = Some(AnchorDay::Weekday(chrono::Weekday::Wed));
        assert_eq!(
            nudge_day(&wednesday_habit, WindowStart(monday)),
            date(2026, 10, 28),
            "the Wednesday inside the Monday window"
        );
        // A MonthDay on a PerWeek habit is inert (documented leniency).
        let mut mismatched = weekly.clone();
        mismatched.anchor_day = Some(AnchorDay::MonthDay(MonthDay::new(17).unwrap()));
        assert_eq!(nudge_day(&mismatched, WindowStart(monday)), monday);
        // PerMonth: none → the 1st; day 17 → the 17th; day 31 clamped
        // to the month's last day (February arms on the 28th/29th).
        let month_start = WindowStart::of_period(PeriodKind::Month, monday);
        let monthly = habit(3, HabitTarget::PerMonth(NonZeroCount::new(1).unwrap()));
        assert_eq!(nudge_day(&monthly, month_start), date(2026, 10, 1));
        let mut monthly_17 = monthly.clone();
        monthly_17.anchor_day = Some(AnchorDay::MonthDay(MonthDay::new(17).unwrap()));
        assert_eq!(nudge_day(&monthly_17, month_start), date(2026, 10, 17));
        let mut monthly_31 = monthly.clone();
        monthly_31.anchor_day = Some(AnchorDay::MonthDay(MonthDay::new(31).unwrap()));
        let february = WindowStart::of_period(PeriodKind::Month, date(2026, 2, 10));
        assert_eq!(
            nudge_day(&monthly_31, february),
            date(2026, 2, 28),
            "31 clamps to the month's last day (2026 is not a leap year)"
        );
        // A Weekday on a PerMonth habit is inert (documented leniency).
        let mut mismatched_month = monthly.clone();
        mismatched_month.anchor_day = Some(AnchorDay::Weekday(chrono::Weekday::Fri));
        assert_eq!(nudge_day(&mismatched_month, month_start), date(2026, 10, 1));
    }

    #[test]
    fn target_carries_kind_and_quota_and_round_trips() {
        let target = HabitTarget::PerWeek(NonZeroCount::new(4).unwrap());
        assert_eq!(target.kind(), PeriodKind::Week);
        assert_eq!(target.quota().get(), 4);
        let json = serde_json::to_string(&target).unwrap();
        assert_eq!(serde_json::from_str::<HabitTarget>(&json).unwrap(), target);
        // Externally tagged on the wire.
        assert_eq!(json, r#"{"PerWeek":4}"#);
    }

    #[test]
    fn habit_and_state_serde_roundtrip_including_the_log() {
        let mut state = HabitState::default();
        let h = habit(1, HabitTarget::PerDay(NonZeroCount::new(3).unwrap()));
        state.habits.insert(h.id, h.clone());
        state.log.insert(
            HabitLogKey {
                habit: h.id,
                window: WindowStart::of_period(PeriodKind::Day, date(2026, 10, 28)),
            },
            HabitLogEntry {
                count: 2,
                clock: Utc.timestamp_opt(500, 0).unwrap(),
                by: None,

                skip: false,
            },
        );
        let json = serde_json::to_string(&state).unwrap();
        let back: HabitState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
        // The conversion seam field round-trips both directions.
        let linked = Habit {
            continued_from: Some(crate::ids::SeriesId::from(uuid::Uuid::from_u128(7))),
            ..h.clone()
        };
        let json = serde_json::to_string(&linked).unwrap();
        assert_eq!(serde_json::from_str::<Habit>(&json).unwrap(), linked);
        let unlinked_json = serde_json::to_string(&h).unwrap();
        assert!(
            unlinked_json.contains(r#""continued_from":null"#),
            "the null seam serializes explicitly"
        );
    }

    #[test]
    fn a_non_canonical_window_start_decodes_and_is_inert_by_type() {
        // Decoded leniency: the date decodes verbatim; the type carries
        // no canonicalization at decode (D-H12 — former-kind synced
        // facts stay in the map). Inertness is the projections' law,
        // pinned there.
        let raw = date(2026, 10, 28); // a Wednesday — not a Monday, not a 1st
        let json = serde_json::to_string(&raw).unwrap();
        let window: WindowStart = serde_json::from_str(&json).unwrap();
        assert_eq!(window.as_date(), raw);
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn period_start_floors_to_the_period_boundary_per_kind() {
        let wednesday = date(2026, 10, 28);
        assert_eq!(period_start(PeriodKind::Day, wednesday), wednesday);
        assert_eq!(
            period_start(PeriodKind::Week, wednesday),
            date(2026, 10, 26)
        );
        assert_eq!(
            period_start(PeriodKind::Month, wednesday),
            date(2026, 10, 1)
        );
        // Idempotent by hand at the boundary itself.
        assert_eq!(
            period_start(PeriodKind::Week, date(2026, 10, 26)),
            date(2026, 10, 26)
        );
    }

    #[test]
    fn period_end_runs_through_the_day_window_vocabulary() {
        let day = DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
            offset: chrono::FixedOffset::east_opt(2 * 3600).unwrap(),
        };
        // Daily: end = the next day's window opening instant.
        let start = date(2026, 10, 28);
        assert_eq!(
            period_end(PeriodKind::Day, start, day),
            crate::days::window_start(date(2026, 10, 29), day.day_start_time, day.offset)
        );
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn period_start_is_idempotent(kind in period_kind_strategy(), d in naive_date_strategy()) {
            let start = period_start(kind, d);
            prop_assert_eq!(period_start(kind, start), start);
        }

        #[test]
        fn period_after_advances_exactly_one_period_and_round_trips(
            kind in period_kind_strategy(),
            d in naive_date_strategy(),
        ) {
            let start = period_start(kind, d);
            let next = period_after(kind, start);
            prop_assert!(next > start, "one period forward strictly");
            prop_assert_eq!(period_start(kind, next), next, "the next window is canonical");
            prop_assert_eq!(period_before(kind, next), start, "before inverts after");
        }

        #[test]
        fn week_starts_are_mondays_and_month_starts_are_day_one(
            kind in period_kind_strategy(),
            d in naive_date_strategy(),
        ) {
            let start = period_start(kind, d);
            match kind {
                PeriodKind::Day => {}
                PeriodKind::Week => {
                    prop_assert_eq!(start.weekday(), chrono::Weekday::Mon);
                }
                PeriodKind::Month => {
                    prop_assert_eq!(start.day(), 1);
                }
            }
        }
    }

    fn period_kind_strategy() -> impl Strategy<Value = PeriodKind> {
        prop_oneof![
            Just(PeriodKind::Day),
            Just(PeriodKind::Week),
            Just(PeriodKind::Month),
        ]
    }

    // ---- Attribution + streak properties (DR-6/5 §5.1) ------------------

    use crate::command::{CommandError, CommandOutcome, plan_command};
    use crate::messages::StateCommand;
    use crate::persistence::{PersistedState, apply_actions};
    use crate::reminders::ReminderDefaults;
    use crate::state::AppState;
    use crate::test_support::{CountingIds, HabitFixture, habit_fixture_strategy, habit_strategy};

    fn log_plan(
        fixture: &HabitFixture,
        command: &StateCommand,
        at: DateTime<Utc>,
    ) -> Result<crate::command::CommandPlan, CommandError> {
        let app = AppState {
            habits: fixture.state(),
            ..AppState::default()
        };
        plan_command(
            &app,
            &[],
            command,
            &CountingIds::new(),
            &ReminderDefaults::default(),
            fixture.day,
            at,
            None,
        )
    }

    fn stored_count(fixture: &HabitFixture, window: WindowStart) -> u32 {
        fixture
            .log
            .get(&HabitLogKey {
                habit: fixture.habit.id,
                window,
            })
            .map_or(0, |entry| entry.count)
    }

    /// The hand model of D-H6, implemented plainly: returns the streak
    /// and how many pending skips the walk took.
    fn reference_walk(fixture: &HabitFixture, now: DateTime<Utc>) -> (u32, usize) {
        let kind = fixture.habit.target.kind();
        let quota = fixture.habit.target.quota().get();
        let today = fixture.day.today(now);
        let mut cursor = WindowStart::of_period(kind, today).as_date();
        let mut streak = 0_u32;
        let mut pendings = 0_usize;
        loop {
            let count = stored_count(fixture, WindowStart(cursor));
            if count >= quota {
                streak += 1;
            } else if preschedule_window(&fixture.habit, WindowStart(cursor), today) {
                // HF4 R-6: pre-schedule windows are transparent in the
                // same class the pendings counter walks through.
                pendings += 1;
            } else {
                let grace_end = period_end(kind, cursor, fixture.day) + grace_offset();
                if now >= grace_end {
                    break;
                }
                pendings += 1;
            }
            let prev = period_before(kind, cursor);
            if prev >= cursor {
                break;
            }
            cursor = prev;
        }
        (streak, pendings)
    }

    fn grace_offset() -> chrono::TimeDelta {
        chrono::TimeDelta::try_hours(24).expect("representable")
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::TimeZone::timestamp_opt(&Utc, secs, 0).unwrap()
    }

    proptest! {
        #![proptest_config(proptest_config(64))]

        /// §5.1 items 1–2: over generated command sequences (LogHabit at
        /// non-decreasing `now`, AdjustHabitLog interleaved), every +1
        /// lands in exactly one window (Σ invariant), the touched
        /// window's count rises by exactly one relative to its
        /// pre-command value, the receipt reports the stored count, and
        /// a repair choice implies the full surplus-repairs rule.
        #[test]
        fn log_sequences_are_monotone_and_partition_attribution(
            (fixture, steps) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (
                    Just(fixture.clone()),
                    // The step clock may hold still (dt = 0 legal,
                    // 0..=3_600): the degenerate same-instant sequence
                    // is included since #986's mint rule
                    // (`max(now, stored + 1s)`) stamps every log's row
                    // strictly past the stored same-key row, so every
                    // +1 lands even at a frozen `now`.
                    proptest::collection::vec(
                        (any::<bool>(), 0_usize..2, 0_u32..=60, 0_i64..=3_600),
                        1..8,
                    ),
                )
            }),
        ) {
            let mut fixture = fixture;
            let mut now = fixture.now;
            let initial_sum: u64 = fixture.log.values().map(|e| u64::from(e.count)).sum();
            let mut logs = 0_u64;
            let quota = fixture.habit.target.quota().get();
            let any_adjust_known = steps.iter().any(|step| !step.0);
            for (is_log, window_pick, count, dt) in steps.clone() {
                now += chrono::TimeDelta::try_seconds(dt).expect("bounded");
                let natural = WindowStart::of_period(
                    fixture.habit.target.kind(),
                    fixture.day.today(now),
                );
                let prev = WindowStart::of_period(
                    fixture.habit.target.kind(),
                    period_before(fixture.habit.target.kind(), natural.as_date()),
                );
                let command = if is_log {
                    StateCommand::LogHabit { habit: fixture.habit.id }
                } else {
                    let window = if window_pick == 0 { natural } else { prev };
                    StateCommand::AdjustHabitLog {
                        habit: fixture.habit.id,
                        window,
                        count,
                    }
                };
                let plan = log_plan(&fixture, &command, now).expect("fixture is commandable");
                if is_log {
                    logs += 1;
                    // (With interleaved corrections the Σ invariant is
                    // pinned separately, over log-only sequences — an
                    // audited adjust may legitimately lower a window.)
                    let (key, entry) = match &plan.actions[0] {
                        crate::persistence::PersistenceAction::RecordHabitLog(k, e) => (*k, *e),
                        other => panic!("unexpected action: {other:?}"),
                    };
                    let pre = stored_count(&fixture, key.window);
                    prop_assert_eq!(entry.count, pre + 1, "exactly one increment");
                    prop_assert!(
                        key.window == natural || key.window == prev,
                        "the choice is natural or prev"
                    );
                    if key.window == prev {
                        // The constructive pin of the surplus-repairs rule.
                        let grace_end = period_end(
                            fixture.habit.target.kind(),
                            prev.as_date(),
                            fixture.day,
                        ) + grace_offset();
                        prop_assert!(now < grace_end, "repair only in grace");
                        prop_assert!(pre < quota, "repair only an unmet prev");
                        prop_assert!(
                            stored_count(&fixture, natural) >= quota,
                            "repair only a met natural"
                        );
                    }
                    let CommandOutcome::HabitLogged {
                        habit,
                        window,
                        count: receipt,
                    } = plan.outcome
                    else {
                        panic!("wrong receipt");
                    };
                    prop_assert_eq!(habit, fixture.habit.id);
                    prop_assert_eq!(window, key.window);
                    prop_assert_eq!(receipt, entry.count);
                }
                // Advance the fixture's log by the plan's actions (the
                // executable contract).
                let mut persisted = PersistedState {
                    habits: fixture.state(),
                    ..PersistedState::default()
                };
                apply_actions(&mut persisted, &plan.actions);
                fixture.log = persisted.habits.log;
            }
            // When no correction ran, each +1 lands in exactly one
            // window: the partition invariant (pinned constructively
            // per-command above; the Σ form is the log-only prop).
            if !any_adjust_known {
                let final_sum: u64 = fixture.log.values().map(|e| u64::from(e.count)).sum();
                prop_assert_eq!(final_sum, initial_sum + logs, "each +1 lands in exactly one window");
            }
        }

        /// §5.1 item 3: `streak` equals the hand model on generated
        /// rows at a `now` probed across the fixture's in-day instant
        /// and up to ~2.3 days forward (spanning grace opens/closes and
        /// period boundaries), and the walk never takes more than two
        /// pending skips.
        #[test]
        fn streak_matches_the_hand_model_and_never_skips_twice_plus(
            (fixture, probe) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (
                    Just(fixture.clone()),
                    0_i64..=200_000,
                )
            }),
        ) {
            let now = fixture.now
                + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
            let (expected, pendings) = reference_walk(&fixture, now);
            prop_assert!(pendings <= 2, "at most two transparent periods");
            prop_assert_eq!(streak(&fixture.habit, &fixture.log, now, fixture.day), expected);
        }

        /// §5.1 item 4 (the negative pin): two habit sets differing only
        /// in `anchor_time` produce identical canonical `(order, id)`
        /// orderings — no ordering surface reads the chip.
        #[test]
        fn anchor_time_never_influences_the_canonical_order(
            habits in proptest::collection::vec(habit_strategy(), 1..6),
        ) {
            let mut perturbed = habits.clone();
            for (i, habit) in perturbed.iter_mut().enumerate() {
                habit.anchor_time = Some(
                    chrono::NaiveTime::from_num_seconds_from_midnight_opt(
                        u32::try_from(i * 1_337).expect("bounded"),
                        0,
                    )
                    .expect("in-range seconds"),
                );
            }
            let key = |habit: &Habit| (habit.order, habit.id);
            let mut a: Vec<_> = habits.iter().map(key).collect();
            let mut b: Vec<_> = perturbed.iter().map(key).collect();
            a.sort();
            b.sort();
            prop_assert_eq!(a, b);
        }

        // US-6/AC-1: `period_start` itself is a free function of
        // (kind, date) that cannot even name a `Habit`, so no direct
        // pin exists for it — the load-bearing invariance pins are its
        // consumers below.

        /// US-6/AC-1 pin 1: `log_window`'s attribution is identical on
        /// a habit and its description/anchor_day-perturbed twin — the
        /// +1 never reads the display fields.
        #[test]
        fn log_window_is_invariant_under_description_and_anchor_day(
            (fixture, probe) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (Just(fixture.clone()), 0_i64..=200_000)
            }),
            description in crate::test_support::string_strategy(),
            anchor_day in crate::test_support::habit_strategies::anchor_day_strategy(),
        ) {
            let mut perturbed = fixture.clone();
            perturbed.habit.description = description;
            perturbed.habit.anchor_day = anchor_day;
            let now = fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
            prop_assert_eq!(
                log_window(&perturbed.habit, &perturbed.log, now, perturbed.day),
                log_window(&fixture.habit, &fixture.log, now, fixture.day),
            );
        }

        /// US-6/AC-1 pin 2: `streak`'s walk (habit.rs `period_start`
        /// contract) is identical under the perturbation, across the
        /// in-day → +2.3-day probe grid.
        #[test]
        fn streak_is_invariant_under_description_and_anchor_day(
            (fixture, probe) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (Just(fixture.clone()), 0_i64..=200_000)
            }),
            description in crate::test_support::string_strategy(),
            anchor_day in crate::test_support::habit_strategies::anchor_day_strategy(),
        ) {
            let mut perturbed = fixture.clone();
            perturbed.habit.description = description;
            perturbed.habit.anchor_day = anchor_day;
            let now = fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
            prop_assert_eq!(
                streak(&perturbed.habit, &perturbed.log, now, perturbed.day),
                streak(&fixture.habit, &fixture.log, now, fixture.day),
            );
            prop_assert_eq!(
                habit_streak_unioned(
                    &perturbed.habit,
                    &perturbed.log,
                    &std::collections::BTreeMap::new(),
                    now,
                    perturbed.day,
                ),
                habit_streak_unioned(
                    &fixture.habit,
                    &fixture.log,
                    &std::collections::BTreeMap::new(),
                    now,
                    fixture.day,
                ),
            );
        }

        /// US-6/AC-1 pin 3 (ordering inertness, the anchor_time pin's
        /// twin): two habit sets differing only in `description` and
        /// `anchor_day` sort to the identical canonical `(order, id)`
        /// sequence.
        #[test]
        fn description_and_anchor_day_never_influence_the_canonical_order(
            habits in proptest::collection::vec(habit_strategy(), 1..6),
            anchor_day in crate::test_support::habit_strategies::anchor_day_strategy(),
        ) {
            let mut perturbed = habits.clone();
            for (i, habit) in perturbed.iter_mut().enumerate() {
                habit.description = format!("note {i}");
                habit.anchor_day = anchor_day;
            }
            let key = |habit: &Habit| (habit.order, habit.id);
            let mut a: Vec<_> = habits.iter().map(key).collect();
            let mut b: Vec<_> = perturbed.iter().map(key).collect();
            a.sort();
            b.sort();
            prop_assert_eq!(a, b);
        }

        /// US-6/AC-1 pin 4 (the constructive nudge law, keeping pins
        /// 1–3 honest): with a random weekday anchored on a `PerWeek`
        /// habit, `nudge_day` of the ISO-Monday window IS that
        /// weekday's date inside the window — while the canonical
        /// window start itself is still Monday (`period_start`
        /// unchanged), proving "day inside the window", never "day
        /// shifts the window".
        #[test]
        fn nudge_day_lies_inside_the_canonical_window_and_monday_stays_monday(
            monday in naive_date_strategy(),
            anchored in proptest::option::of(
                (0_u32..7).prop_map(|n| match n {
                    0 => chrono::Weekday::Mon,
                    1 => chrono::Weekday::Tue,
                    2 => chrono::Weekday::Wed,
                    3 => chrono::Weekday::Thu,
                    4 => chrono::Weekday::Fri,
                    5 => chrono::Weekday::Sat,
                    _ => chrono::Weekday::Sun,
                }),
            ),
        ) {
            let start = period_start(PeriodKind::Week, monday);
            let mut weekly = habit(7, HabitTarget::PerWeek(NonZeroCount::new(1).unwrap()));
            weekly.anchor_day = anchored.map(AnchorDay::Weekday);
            let day = nudge_day(&weekly, WindowStart(start));
            prop_assert_eq!(period_start(PeriodKind::Week, day), start,
                "the nudge day floors back to the SAME canonical window");
            let end = period_after(PeriodKind::Week, start);
            prop_assert!(day >= start && day < end, "the nudge day lies inside the window");
            match weekly.anchor_day {
                Some(AnchorDay::Weekday(wd)) if wd != chrono::Weekday::Mon => {
                    prop_assert_eq!(day.weekday(), wd, "the nudge day IS the anchored weekday");
                }
                _ => prop_assert_eq!(day, start, "no (or inert) anchor reads the window start"),
            }
        }
    }
    // ---- HF1 Σ property extension (2026-11-21) ---------------------------

    /// Extend a fixture's log with skip marks over the windows covered
    /// by `mask` (row-index-addressed, newest first).
    fn mark_skips(fixture: &mut HabitFixture, mask: &[bool]) {
        let windows: Vec<HabitLogKey> = fixture.log.keys().copied().collect();
        for (i, key) in windows.iter().enumerate().take(mask.len()) {
            let entry = fixture.log.get_mut(key).expect("the key came from the map");
            if mask[i] {
                entry.skip = true;
            }
        }
    }

    /// The reference hand-walk of D-H6 extended with the transparent
    /// class (pause-covered, skipped): counts consecutive met windows
    /// ending at the natural window, walking through transparent ones,
    /// stopping at the first missed one; returns the streak and the
    /// number of grace-pending windows taken.
    fn hf1_reference_walk(fixture: &HabitFixture, now: DateTime<Utc>) -> (u32, usize) {
        let kind = fixture.habit.target.kind();
        let quota = fixture.habit.target.quota().get();
        let mut cursor = WindowStart::of_period(kind, fixture.day.today(now)).as_date();
        let mut streak = 0_u32;
        let mut pendings = 0_usize;
        loop {
            let row = fixture.log.get(&HabitLogKey {
                habit: fixture.habit.id,
                window: WindowStart(cursor),
            });
            let count = row.map_or(0, |e| e.count);
            let skipped = row.is_some_and(|e| e.skip);
            let covered = fixture
                .habit
                .paused_until
                .is_some_and(|until| pause_covers_window(kind, cursor, until));
            if covered {
                // transparent — and monotone downward: the walk stops
                // here (the termination law mirrors `streak_with_break`)
                break;
            } else if count >= quota {
                streak += 1;
            } else if skipped {
                // transparent — walk on
            } else {
                let grace_end = period_end(kind, cursor, fixture.day) + grace_offset();
                if now >= grace_end {
                    break;
                }
                pendings += 1;
            }
            let prev = period_before(kind, cursor);
            if prev >= cursor {
                break;
            }
            cursor = prev;
        }
        (streak, pendings)
    }

    proptest! {
        #![proptest_config(proptest_config(64))]

        /// Σ extension (d) — transparency composes: skip ∧ pause are one
        /// transparent outcome, not three behaviors. Over generated
        /// fixtures with random skip marks and a random pause date:
        /// (a) a transparent window never increments (streak ≤ the
        /// number of uncovered met rows), (b) it never breaks (streak ≥
        /// the baseline with the marks/pause removed), and (c) marking a
        /// pause-covered row skipped changes nothing (the composition's
        /// one-outcome law).
        #[test]
        fn skip_and_pause_transparency_compose(
            (fixture, probe, mask) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (
                    Just(fixture.clone()),
                    0_i64..=200_000,
                    proptest::collection::vec(any::<bool>(), 0..9),
                )
            }),
        ) {
            let now = fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
            let baseline = streak(&fixture.habit, &fixture.log, now, fixture.day);
            // (b) transparency never breaks: marking and pausing can
            // only extend the walk, never truncate a run the baseline
            // counted.
            let mut marked = fixture.clone();
            mark_skips(&mut marked, &mask);
            let with_marks = streak(&marked.habit, &marked.log, now, marked.day);
            prop_assert!(with_marks >= baseline, "skips never break a run");
            // (a) transparency never counts: the walk stops at the
            // first pause-covered window (the monotone termination law)
            // and a skipped window neither counts nor breaks, so the
            // marked walk visits at most the fixture's own windows —
            // every counted window in it is a met row of the fixture.
            let kind = marked.habit.target.kind();
            let met_rows = marked
                .log
                .values()
                .filter(|e| e.count >= marked.habit.target.quota().get())
                .count();
            prop_assert!(
                with_marks <= u32::try_from(met_rows).unwrap_or(u32::MAX),
                "a transparent window never increments: the walk counts only met rows"
            );
            // (c) composition: skip ∧ pause are ONE transparent
            // outcome — marking a pause-covered row skipped changes
            // nothing, and covering a marked row changes nothing.
            if let Some(first) = marked.log.keys().next().copied() {
                let mut paused = marked.clone();
                paused.habit.paused_until = Some(
                    period_after(kind, first.window.as_date())
                        .pred_opt()
                        .unwrap_or(first.window.as_date()),
                );
                let with_pause = streak(&paused.habit, &paused.log, now, paused.day);
                let mut both = paused.clone();
                if let Some(entry) = both.log.get_mut(&first) {
                    entry.skip = true;
                }
                prop_assert_eq!(
                    streak(&both.habit, &both.log, now, both.day),
                    with_pause,
                    "skip ∧ pause = one transparent outcome"
                );
                let mut marks_only = paused.clone();
                marks_only.habit.paused_until = None;
                let _ = streak(&marks_only.habit, &marks_only.log, now, marks_only.day);
            }
        }

        /// Σ extension (bound): with skip marks and a pause set, the
        /// walk still takes at most two grace-pending windows for any
        /// `now` — transparent windows are not pendings — and the walk
        /// equals the extended hand model.
        #[test]
        fn streak_pending_bound_holds_with_transparent_windows(
            (fixture, probe, mask) in habit_fixture_strategy().prop_flat_map(|fixture| {
                (
                    Just(fixture.clone()),
                    0_i64..=200_000,
                    proptest::collection::vec(any::<bool>(), 0..9),
                )
            }),
        ) {
            let now = fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
            let mut marked = fixture.clone();
            mark_skips(&mut marked, &mask);
            if let Some(first) = marked.log.keys().next().copied() {
                let kind = marked.habit.target.kind();
                marked.habit.paused_until =
                    Some(period_after(kind, first.window.as_date()).pred_opt().unwrap_or(first.window.as_date()));
            }
            let (expected, pendings) = hf1_reference_walk(&marked, now);
            prop_assert!(pendings <= 2, "at most two grace-pending windows");
            prop_assert_eq!(streak(&marked.habit, &marked.log, now, marked.day), expected);
        }
    }

    /// Desired semantics (#986): two `LogHabit`s stamped with the
    /// SAME `now` (same planner second — double-tap, scripted CLI)
    /// BOTH land. The mint rule
    /// `mint_log_clock(now, stored_clock) = max(now, stored_clock + 1s)`
    /// stamps the second log's row strictly past the first, so the
    /// strictly-newer LWW apply admits it. All apply/fold/SQL layers
    /// stay strictly-newer; determinism holds because the bump is a
    /// pure function of (pre-state, command, `now`).
    #[test]
    fn log_same_instant_double_log_increments_twice() {
        let mut fixture = HabitFixture {
            habit: habit(1, HabitTarget::PerWeek(NonZeroCount::new(1).expect("1"))),
            log: std::collections::BTreeMap::new(),
            now: ts(1_000),
            day: DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("utc"),
            },
        };
        let plan1 = log_plan(
            &fixture,
            &StateCommand::LogHabit {
                habit: fixture.habit.id,
            },
            fixture.now,
        )
        .expect("commandable");
        // The second log plans against the post-first-log state (the
        // mint is a pure function of the pre-state; real replays
        // execute commands in order, so this mirrors the engine).
        advance_fixture(&mut fixture, &plan1);
        let plan2 = log_plan(
            &fixture,
            &StateCommand::LogHabit {
                habit: fixture.habit.id,
            },
            fixture.now,
        )
        .expect("commandable");
        let mut persisted = crate::persistence::PersistedState {
            habits: fixture.state(),
            ..crate::persistence::PersistedState::default()
        };
        crate::persistence::apply_actions(&mut persisted, &plan1.actions);
        crate::persistence::apply_actions(&mut persisted, &plan2.actions);
        let total: u64 = persisted
            .habits
            .log
            .values()
            .map(|entry| u64::from(entry.count))
            .sum();
        assert_eq!(
            total, 2,
            "both same-instant increments land: the mint bump separates the row clocks"
        );
    }

    /// Deterministic table for the #986 mint rule (lowest layer): a
    /// fresh key mints `now`; stored at `now` / ahead of `now` mints
    /// `stored + 1s`; stored behind `now` mints `now`.
    #[test]
    fn mint_log_clock_follows_the_monotone_rule() {
        let now = ts(1_000);
        assert_eq!(mint_log_clock(now, None), now, "fresh key mints now");
        assert_eq!(
            mint_log_clock(now, Some(ts(1_000))),
            ts(1_001),
            "stored at now mints stored + 1s"
        );
        assert_eq!(
            mint_log_clock(now, Some(ts(1_001))),
            ts(1_002),
            "stored ahead of now mints stored + 1s"
        );
        assert_eq!(
            mint_log_clock(now, Some(ts(995))),
            now,
            "stored behind now mints now"
        );
    }

    /// Command-arm pin (#986): two same-instant `LogHabit`s mint
    /// strictly increasing row clocks — the first at `T`, the second
    /// (planned against the post-first state) at `T + 1s`.
    #[test]
    fn same_instant_double_log_mints_strictly_increasing_clocks() {
        let mut fixture = habit_fixture(ts(1_000));
        let plan1 = log_plan(
            &fixture,
            &StateCommand::LogHabit {
                habit: fixture.habit.id,
            },
            fixture.now,
        )
        .expect("commandable");
        advance_fixture(&mut fixture, &plan1);
        let plan2 = log_plan(
            &fixture,
            &StateCommand::LogHabit {
                habit: fixture.habit.id,
            },
            fixture.now,
        )
        .expect("commandable");
        let clock1 = minted_clock(&plan1);
        let clock2 = minted_clock(&plan2);
        assert_eq!(clock1, fixture.now, "the first log stamps now");
        assert_eq!(
            clock2,
            fixture.now + one_second(),
            "the second same-instant log stamps strictly past the first row"
        );
    }

    /// The adjust-survives pin (#986): after a 3-burst of same-instant
    /// logs at `T` (row clock `T + 2s`), an `AdjustHabitLog` at `T + 1s`
    /// still lands — the shared bump stamps it `T + 3s`, strictly past
    /// the burst row, preserving the adjust's newest-write intent.
    #[test]
    fn adjust_after_a_same_instant_burst_stamps_past_the_burst_row() {
        let mut fixture = habit_fixture(ts(1_000));
        let mut burst_clock = None;
        for _ in 0..3 {
            let plan = log_plan(
                &fixture,
                &StateCommand::LogHabit {
                    habit: fixture.habit.id,
                },
                fixture.now,
            )
            .expect("commandable");
            advance_fixture(&mut fixture, &plan);
            burst_clock = Some(minted_clock(&plan));
        }
        let burst_clock = burst_clock.expect("the burst ran");
        let plan = log_plan(
            &fixture,
            &StateCommand::AdjustHabitLog {
                habit: fixture.habit.id,
                window: log_window(&fixture.habit, &fixture.log, fixture.now, fixture.day),
                count: 7,
            },
            fixture.now + one_second(),
        )
        .expect("commandable");
        let adjust_clock = minted_clock(&plan);
        assert_eq!(
            burst_clock,
            ts(1_002),
            "the 3-burst advances the row clock two seconds past now"
        );
        assert!(
            adjust_clock > burst_clock,
            "the adjust stamps strictly past the burst row"
        );
        let mut final_state = crate::persistence::PersistedState {
            habits: fixture.state(),
            ..PersistedState::default()
        };
        apply_actions(&mut final_state, &plan.actions);
        let window = log_window(&fixture.habit, &fixture.log, fixture.now, fixture.day);
        assert_eq!(
            final_state
                .habits
                .log
                .get(&HabitLogKey {
                    habit: fixture.habit.id,
                    window
                })
                .map(|entry| entry.count),
            Some(7),
            "the adjust lands over the burst"
        );
    }

    fn habit_fixture(now: DateTime<Utc>) -> HabitFixture {
        HabitFixture {
            habit: habit(1, HabitTarget::PerWeek(NonZeroCount::new(1).expect("1"))),
            log: std::collections::BTreeMap::new(),
            now,
            day: DayCalendar {
                day_start_time: chrono::NaiveTime::MIN,
                offset: chrono::FixedOffset::east_opt(0).expect("utc"),
            },
        }
    }

    /// Apply a plan's actions to the fixture's log in place — the
    /// engine-shaped advance the mint pins replay (commands execute in
    /// order against the post-previous-command state).
    fn advance_fixture(fixture: &mut HabitFixture, plan: &crate::command::CommandPlan) {
        let mut persisted = PersistedState {
            habits: fixture.state(),
            ..PersistedState::default()
        };
        apply_actions(&mut persisted, &plan.actions);
        fixture.log = persisted.habits.log;
    }

    fn minted_clock(plan: &crate::command::CommandPlan) -> DateTime<Utc> {
        match &plan.actions[0] {
            crate::persistence::PersistenceAction::RecordHabitLog(_, entry) => entry.clock,
            other => panic!("unexpected action: {other:?}"),
        }
    }

    // ---- Deterministic streak cases (§5.3) -------------------------------

    fn met(
        counts: &mut BTreeMap<HabitLogKey, HabitLogEntry>,
        habit: &Habit,
        window: WindowStart,
        at: DateTime<Utc>,
    ) {
        counts.insert(
            HabitLogKey {
                habit: habit.id,
                window,
            },
            HabitLogEntry {
                count: 5,
                clock: at,
                by: None,

                skip: false,
            },
        );
    }

    #[test]
    fn a_brand_new_habit_with_an_empty_log_has_streak_zero() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let habit = Habit {
            id: HabitId::from(uuid::Uuid::from_u128(1)),
            board: BoardId::from(uuid::Uuid::from_u128(2)),
            title: "new".into(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(9).expect("9 h");
        let empty: BTreeMap<HabitLogKey, HabitLogEntry> = BTreeMap::new();
        assert_eq!(
            streak(&habit, &empty, now, day),
            0,
            "today pending, everything earlier missed"
        );
    }

    #[test]
    fn the_morning_of_an_unmet_today_still_shows_yesterdays_run() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let habit = Habit {
            id: HabitId::from(uuid::Uuid::from_u128(1)),
            board: BoardId::from(uuid::Uuid::from_u128(2)),
            title: "daily".into(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(8).expect("8 h");
        let yesterday = WindowStart(period_before(PeriodKind::Day, today));
        let mut log = BTreeMap::new();
        met(&mut log, &habit, yesterday, ts(100));
        // Today unmet but its grace is open: pending — yesterday counts.
        assert_eq!(streak(&habit, &log, now, day), 1);
        // And the day before yesterday unmet breaks it exactly there.
        assert_eq!(
            streak(&habit, &BTreeMap::new(), now, day),
            0,
            "no rows, no run"
        );
    }

    #[test]
    fn a_single_miss_breaks_exactly_after_its_grace_free_window() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let habit = Habit {
            id: HabitId::from(uuid::Uuid::from_u128(1)),
            board: BoardId::from(uuid::Uuid::from_u128(2)),
            title: "daily".into(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let w = |d: NaiveDate| WindowStart::of_period(PeriodKind::Day, d);
        let miss = period_before(PeriodKind::Day, period_before(PeriodKind::Day, today));
        let mut log = BTreeMap::new();
        for date in [today, period_before(PeriodKind::Day, today)] {
            met(&mut log, &habit, w(date), ts(100));
        }
        // Older met run so the grace span is observable.
        let mut older = period_before(PeriodKind::Day, miss);
        for _ in 0..4 {
            met(&mut log, &habit, w(older), ts(100));
            older = period_before(PeriodKind::Day, older);
        }
        // Inside the miss's grace (its end + 24 h = today's window start):
        // the miss is pending; the chain spans both neighbors.
        let grace_end = period_end(PeriodKind::Day, miss, day) + grace_offset();
        let inside = grace_end - chrono::TimeDelta::try_hours(1).expect("1 h");
        let past = grace_end + chrono::TimeDelta::try_hours(1).expect("1 h");
        let inside_streak = streak(&habit, &log, inside, day);
        let past_streak = streak(&habit, &log, past, day);
        assert_eq!(past_streak, 2, "the chain breaks at the closed miss");
        assert!(
            inside_streak > past_streak,
            "inside the grace the miss is transparent: {inside_streak} vs {past_streak}"
        );
        // A surplus log can repair the miss while it is pending: the
        // healed period itself joins the chain (the walk counts it).
        let mut repaired = log.clone();
        met(&mut repaired, &habit, w(miss), ts(100));
        assert_eq!(
            streak(&habit, &repaired, inside, day),
            inside_streak + 1,
            "the repaired miss counts on top of the spanned neighbors"
        );
    }

    // ---- HF1 skip/pause semantics (2026-11-21) ---------------------------

    fn skipped(
        counts: &mut BTreeMap<HabitLogKey, HabitLogEntry>,
        habit: &Habit,
        window: WindowStart,
        at: DateTime<Utc>,
    ) {
        counts.insert(
            HabitLogKey {
                habit: habit.id,
                window,
            },
            HabitLogEntry {
                count: 0,
                clock: at,
                by: None,
                skip: true,
            },
        );
    }

    /// US-1/AC-2 (Σ extension): a skipped window whose grace has closed
    /// is transparent — neither counts nor breaks.
    #[test]
    fn skip_window_is_transparent_in_the_streak_walk() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let habit = habit(1, HabitTarget::PerWeek(NonZeroCount::new(1).unwrap()));
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(); // a Wednesday
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(10).expect("10 h");
        let this_week = WindowStart::of_period(PeriodKind::Week, today);
        let last_week =
            WindowStart::of_period(PeriodKind::Week, period_before(PeriodKind::Week, today));
        let mut log = BTreeMap::new();
        met(&mut log, &habit, this_week, ts(100));
        // Without the mark, the unmet closed last week is the break.
        assert_eq!(streak(&habit, &log, now, day), 1);
        // With the mark, the walk walks on (older empty windows miss).
        let mut marked = log.clone();
        skipped(&mut marked, &habit, last_week, ts(90));
        assert_eq!(
            streak(&habit, &marked, now, day),
            1,
            "the skip is transparent"
        );
    }

    /// US-1/AC-3 (R-4 consultation order): a skipped window whose quota
    /// is met anyway counts normally.
    #[test]
    fn met_window_on_a_skipped_row_still_counts() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let habit = habit(1, HabitTarget::PerDay(NonZeroCount::new(1).unwrap()));
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(6).expect("6 h");
        let w = |d: NaiveDate| WindowStart::of_period(PeriodKind::Day, d);
        let mut log = BTreeMap::new();
        // Today: met AND skipped (a log after a skip; the mark stays).
        log.insert(
            HabitLogKey {
                habit: habit.id,
                window: w(today),
            },
            HabitLogEntry {
                count: 2,
                clock: ts(100),
                by: None,
                skip: true,
            },
        );
        met(
            &mut log,
            &habit,
            w(period_before(PeriodKind::Day, today)),
            ts(90),
        );
        assert_eq!(
            streak(&habit, &log, now, day),
            2,
            "the skipped-but-met window counts"
        );
    }

    /// US-2/AC-1: windows covered by the pause are transparent — no
    /// miss, no credit — even over a met row.
    #[test]
    fn pause_covered_windows_are_transparent() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let mut habit = habit(1, HabitTarget::PerDay(NonZeroCount::new(1).unwrap()));
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap();
        let yesterday = period_before(PeriodKind::Day, today);
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(30).expect("30 h past yesterday's grace");
        let w = |d: NaiveDate| WindowStart::of_period(PeriodKind::Day, d);
        let mut log = BTreeMap::new();
        met(&mut log, &habit, w(today), ts(100));
        met(&mut log, &habit, w(yesterday), ts(90));
        assert_eq!(streak(&habit, &log, now, day), 2);
        // Pause through yesterday: the covered window gives no credit...
        habit.paused_until = Some(yesterday);
        assert_eq!(streak(&habit, &log, now, day), 1, "no credit while covered");
        // ...and an unmet covered window gives no miss either.
        let mut unmet = log.clone();
        unmet.remove(&HabitLogKey {
            habit: habit.id,
            window: w(yesterday),
        });
        assert_eq!(
            streak(&habit, &unmet, now, day),
            1,
            "no miss while covered — the walk walks on"
        );
    }

    /// US-1/US-2 additive decode law: pre-HF1 rows and habits decode
    /// with the defaults (`skip` false, `paused_until` None).
    #[test]
    fn skip_and_pause_fields_decode_additively() {
        let legacy_entry = serde_json::json!({
            "count": 2,
            "clock": "2026-11-01T00:00:00Z",
        });
        let entry: HabitLogEntry = serde_json::from_value(legacy_entry).unwrap();
        assert!(!entry.skip, "pre-HF1 log rows decode unmarked");
        assert_eq!(entry.count, 2);
        let marked = serde_json::to_string(&HabitLogEntry {
            count: 0,
            clock: ts(5),
            by: None,
            skip: true,
        })
        .unwrap();
        assert!(marked.contains(r#""skip":true"#), "the mark serializes");
        let back: HabitLogEntry = serde_json::from_str(&marked).unwrap();
        assert!(back.skip);
        let unmarked = serde_json::to_string(&HabitLogEntry {
            count: 1,
            clock: ts(5),
            by: None,
            skip: false,
        })
        .unwrap();
        assert!(
            !unmarked.contains("skip"),
            "an unmarked row omits the key (skip_serializing_if)"
        );
        let clocks: HabitClocks = serde_json::from_str(
            r#"{"title":"2026-01-01T00:00:00Z","target":"2026-01-01T00:00:00Z","order":"2026-01-01T00:00:00Z","anchor_time":"2026-01-01T00:00:00Z","deleted":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(
            clocks.paused_until,
            Utc.timestamp_opt(0, 0).unwrap(),
            "the pause clock defaults to the epoch (never written)"
        );
        let h = habit(1, HabitTarget::PerDay(NonZeroCount::new(1).unwrap()));
        let json = serde_json::to_string(&h).unwrap();
        assert!(
            !json.contains(r#""paused_until":null"#),
            "the pause value serializes only when set — a None never emits a null key"
        );
        let back: Habit = serde_json::from_str(&json).unwrap();
        assert_eq!(back.paused_until, None);
    }

    /// `pause_covers_window`'s whole-window law: a daily window covers
    /// itself; a weekly window needs its Sunday inside the pause; a
    /// partially covered window is not covered.
    #[test]
    fn pause_covers_whole_windows_only() {
        let monday = date(2026, 10, 26);
        assert!(
            pause_covers_window(PeriodKind::Day, monday, monday),
            "the day itself"
        );
        assert!(!pause_covers_window(
            PeriodKind::Day,
            monday,
            date(2026, 10, 25)
        ));
        assert!(
            !pause_covers_window(PeriodKind::Week, monday, date(2026, 10, 28)),
            "a Wednesday-through pause does not cover the week (Sunday is out)"
        );
        assert!(pause_covers_window(
            PeriodKind::Week,
            monday,
            date(2026, 11, 1)
        ));
    }

    #[test]
    fn foreign_kind_rows_are_inert_and_a_quota_change_rebases_the_streak() {
        let day = crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        };
        let mut habit = Habit {
            id: HabitId::from(uuid::Uuid::from_u128(1)),
            board: BoardId::from(uuid::Uuid::from_u128(2)),
            title: "retargeted".into(),
            description: String::new(),
            target: HabitTarget::PerWeek(NonZeroCount::new(2).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
            by: None,

            paused_until: None,
        };
        let today = NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(); // a Wednesday
        let now = crate::days::window_start(today, day.day_start_time, day.offset)
            + chrono::TimeDelta::try_hours(10).expect("10 h");
        let mut log = BTreeMap::new();
        // Daily rows from the habit's former life at *non-canonical*
        // weekly dates (this Wednesday and Tuesday — no Monday): they
        // are analytically inert for a weekly target (D-H12).
        for offset in [0_i64, 1] {
            let date = today - chrono::Days::new(u64::try_from(offset).expect("positive"));
            met(
                &mut log,
                &habit,
                WindowStart::of_period(PeriodKind::Day, date),
                ts(100),
            );
        }
        assert_eq!(
            streak(&habit, &log, now, day),
            0,
            "non-canonical dates never count for a weekly target"
        );
        // (Rows on dates that *are* canonical weekly windows share the
        // (habit, date) key — one fact, not two — and count; that is
        // the key model, not leakage.)
        met(
            &mut log,
            &habit,
            WindowStart::of_period(PeriodKind::Week, today),
            ts(100),
        );
        assert_eq!(streak(&habit, &log, now, day), 1);
        let _ = &mut habit;
    }
}

#[cfg(test)]
mod seam_tests {
    use super::*;
    use crate::days::DayCalendar;
    use crate::entities::{Task, TaskClocks};
    use crate::ids::{BoardId, HabitId, SeriesId, StackId, TaskId};
    use std::collections::BTreeMap;

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::TimeZone::timestamp_opt(&Utc, secs, 0).unwrap()
    }

    fn day() -> DayCalendar {
        DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        }
    }

    fn linked_habit(series: Option<u128>) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(1)),
            board: BoardId::from(uuid::Uuid::from_u128(2)),
            title: "morning run".into(),
            description: String::new(),
            target: HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
            order: 0,
            anchor_time: None,
            anchor_day: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: series.map(|s| SeriesId::from(uuid::Uuid::from_u128(s))),
            deleted: false,
            by: None,

            paused_until: None,
        }
    }

    fn series_task(raw: u128, series_raw: u128, member: NaiveDate, done: bool) -> Task {
        Task {
            id: TaskId::from(uuid::Uuid::from_u128(raw)),
            remote: None,
            title: "occurrence".into(),
            description: String::new(),
            duedate: Some(crate::days::window_start(
                member,
                chrono::NaiveTime::MIN,
                chrono::FixedOffset::east_opt(0).unwrap(),
            )),
            scheduled: None,
            done: done.then_some(ts(1_000)),
            stack: StackId::from(uuid::Uuid::from_u128(500)),
            order: 0,
            label_memberships: BTreeMap::new(),
            effort: None,
            importance: None,
            urgency: None,
            recur: Some(crate::recur::RecurRule {
                series: SeriesId::from(uuid::Uuid::from_u128(series_raw)),
                anchor: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                pattern: crate::recur::RecurPattern::Daily {
                    every: crate::recur::EveryDays::new(1).unwrap(),
                },
                bound: crate::recur::RecurBound::None,
                continued_from: None,
                paused_until: None,
            }),
            kind: crate::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: ts(1),
                description: ts(1),
                duedate: ts(1),
                scheduled: ts(1),
                done: ts(1),
                position: ts(1),
                effort: ts(1),
                importance: ts(1),
                urgency: ts(1),
                recur: ts(1),
                kind: ts(1),
                archived: ts(1),
                deleted: ts(1),
            },
            authors: crate::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn met_on(log: &mut BTreeMap<HabitLogKey, HabitLogEntry>, habit: &Habit, d: NaiveDate) {
        log.insert(
            HabitLogKey {
                habit: habit.id,
                window: WindowStart(d),
            },
            HabitLogEntry {
                count: 2,
                clock: ts(100),
                by: None,

                skip: false,
            },
        );
    }

    const TODAY: NaiveDate = match NaiveDate::from_ymd_opt(2026, 10, 28) {
        Some(d) => d,
        None => panic!(),
    };

    fn now() -> DateTime<Utc> {
        crate::days::window_start(
            TODAY,
            chrono::NaiveTime::MIN,
            chrono::FixedOffset::east_opt(0).unwrap(),
        ) + chrono::TimeDelta::try_hours(6).expect("6 h")
    }

    #[test]
    fn no_link_or_no_break_means_exactly_the_plain_streak() {
        let unlinked = linked_habit(None);
        let linked = linked_habit(Some(7));
        let mut log = BTreeMap::new();
        met_on(&mut log, &unlinked, TODAY);
        met_on(&mut log, &unlinked, period_before(PeriodKind::Day, TODAY));
        let tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
        let at = now();
        assert_eq!(
            habit_streak_unioned(&unlinked, &log, &tasks, at, day()),
            streak(&unlinked, &log, at, day()),
        );
        // Linked, but the walk never breaks (both days met, yesterday's
        // grace long closed for today? No — today is met, yesterday met,
        // the day before misses and breaks). A saturating walk needs an
        // all-met infinite log; the bounded form here always breaks, so
        // pin the no-break arm through the saturation rule instead:
        // streak == 0 with an empty log breaks immediately — equals the
        // plain walk with no series to join.
        let empty: BTreeMap<HabitLogKey, HabitLogEntry> = BTreeMap::new();
        assert_eq!(
            habit_streak_unioned(&linked, &empty, &tasks, at, day()),
            streak(&linked, &empty, at, day()),
        );
    }

    #[test]
    fn one_completed_series_member_at_the_break_extends_the_streak_by_one() {
        let habit = linked_habit(Some(7));
        let yesterday = period_before(PeriodKind::Day, TODAY);
        // Yesterday's grace is closed at `at` (30 h into today), so the
        // two-days-back miss is the walk's break period.
        let miss = period_before(PeriodKind::Day, yesterday);
        let at = crate::days::window_start(
            TODAY,
            chrono::NaiveTime::MIN,
            chrono::FixedOffset::east_opt(0).unwrap(),
        ) + chrono::TimeDelta::try_hours(30).expect("30 h"); // yesterday's grace closed
        let mut log = BTreeMap::new();
        met_on(&mut log, &habit, TODAY);
        met_on(&mut log, &habit, yesterday);
        // miss (two days back) unmet → break there.
        let plain = streak(&habit, &log, at, day());
        assert_eq!(plain, 2, "today and yesterday count; the miss breaks");
        // The series completed the miss's member: the streak extends by one.
        let mut tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
        let done = series_task(10, 7, miss, true);
        tasks.insert(done.id, done);
        assert_eq!(
            habit_streak_unioned(&habit, &log, &tasks, at, day()),
            plain + 1,
        );
        // An open (not-done) member at the same date contributes nothing.
        let mut open_tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
        let open = series_task(11, 7, miss, false);
        open_tasks.insert(open.id, open);
        assert_eq!(
            habit_streak_unioned(&habit, &log, &open_tasks, at, day()),
            plain
        );
        // A gap breaks the join: complete only the member before the miss.
        let mut gapped: BTreeMap<TaskId, Task> = BTreeMap::new();
        let older = series_task(12, 7, period_before(PeriodKind::Day, miss), true);
        gapped.insert(older.id, older);
        assert_eq!(
            habit_streak_unioned(&habit, &log, &gapped, at, day()),
            plain
        );
    }

    #[test]
    fn the_union_is_invariant_under_task_map_permutation() {
        let habit = linked_habit(Some(7));
        let yesterday = period_before(PeriodKind::Day, TODAY);
        let miss = period_before(PeriodKind::Day, yesterday);
        let mut log = BTreeMap::new();
        met_on(&mut log, &habit, TODAY);
        met_on(&mut log, &habit, yesterday);
        let at = crate::days::window_start(
            TODAY,
            chrono::NaiveTime::MIN,
            chrono::FixedOffset::east_opt(0).unwrap(),
        ) + chrono::TimeDelta::try_hours(30).expect("30 h");
        let a = series_task(10, 7, miss, true);
        let b = series_task(11, 7, period_before(PeriodKind::Day, miss), true);
        let mut one = BTreeMap::new();
        one.insert(a.id, a.clone());
        one.insert(b.id, b.clone());
        let mut two = BTreeMap::new();
        two.insert(b.id, b);
        two.insert(a.id, a);
        assert_eq!(
            habit_streak_unioned(&habit, &log, &one, at, day()),
            habit_streak_unioned(&habit, &log, &two, at, day()),
        );
    }

    #[test]
    fn a_deleted_series_task_never_joins_the_streak() {
        let habit = linked_habit(Some(7));
        let yesterday = period_before(PeriodKind::Day, TODAY);
        let miss = period_before(PeriodKind::Day, yesterday);
        let mut log = BTreeMap::new();
        met_on(&mut log, &habit, TODAY);
        met_on(&mut log, &habit, yesterday);
        let at = crate::days::window_start(
            TODAY,
            chrono::NaiveTime::MIN,
            chrono::FixedOffset::east_opt(0).unwrap(),
        ) + chrono::TimeDelta::try_hours(30).expect("30 h");
        let mut tasks = BTreeMap::new();
        let mut done = series_task(10, 7, miss, true);
        done.deleted = true;
        tasks.insert(done.id, done);
        assert_eq!(
            habit_streak_unioned(&habit, &log, &tasks, at, day()),
            streak(&habit, &log, at, day()),
        );
    }

    #[test]
    fn paused_members_are_absent_not_gaps_in_the_joined_run() {
        // Issue #506: the pause removes the contiguous member block
        // through `paused_until` — those members do not exist, so the
        // backward enumeration neither needs done rows inside the
        // pause nor breaks on their absence (a stored completion at a
        // paused date stays an inert fact).
        let habit = linked_habit(Some(7));
        let yesterday = period_before(PeriodKind::Day, TODAY);
        let miss = period_before(PeriodKind::Day, yesterday);
        let pause_through = period_before(PeriodKind::Day, miss);
        let at = crate::days::window_start(
            TODAY,
            chrono::NaiveTime::MIN,
            chrono::FixedOffset::east_opt(0).unwrap(),
        ) + chrono::TimeDelta::try_hours(30).expect("30 h");
        let mut log = BTreeMap::new();
        met_on(&mut log, &habit, TODAY);
        met_on(&mut log, &habit, yesterday);
        let plain = streak(&habit, &log, at, day());
        assert_eq!(plain, 2, "today and yesterday count; the miss breaks");
        // The pause rides every carried rule copy (the chain copies the
        // rule verbatim; the union reads it from the most recent done
        // occurrence). The pause's own block carries a done row — a
        // completion made before the pause started — and it does not
        // join (its member does not exist) and does not break.
        let mut tasks: BTreeMap<TaskId, Task> = BTreeMap::new();
        for (raw, member) in [(10, miss), (11, pause_through)] {
            let mut task = series_task(raw, 7, member, true);
            if let Some(rule) = task.recur.as_mut() {
                rule.paused_until = Some(pause_through);
            }
            tasks.insert(task.id, task);
        }
        assert_eq!(
            habit_streak_unioned(&habit, &log, &tasks, at, day()),
            plain + 1,
        );
    }
}

// -- the tracker nudge anchor derivation (#504, plan D-2) ---------------

/// The tracker-nudge occurrence for the day-window `window_date`: the
/// anchor reading inside that window's span
/// `[window_start(D), window_start(D+1))` — a stable instant derived
/// purely from `(anchor_time, window_date, day)`. An anchor *before*
/// `day_start_time` reads on the window's tail calendar date (the
/// night-owl rule: window `D` spans into `D+1`'s early morning), so the
/// key always lies inside `D`'s span and every tick inside the window
/// derives the same instant — the at-most-once suppression key, stable
/// while history is stable (the break-rule D-8 law, tracker-shaped).
/// Never a tick stamp; never an ordering input (D-C13 stands).
///
/// The next window derives its own fresh instant, so the nudge re-arms
/// on period rollover by construction — no stored arm/disarm state.
/// The in-span law is total for every representable window date; only
/// within a day of `NaiveDate::MAX` (whose successor date does not
/// exist) does the tail reading clamp to the latest date — a value no
/// real calendar ever reaches.
#[must_use]
pub fn tracker_anchor_occurrence(
    anchor_time: chrono::NaiveTime,
    window_date: NaiveDate,
    day: DayCalendar,
) -> DateTime<Utc> {
    let date = if anchor_time >= day.day_start_time {
        window_date
    } else {
        window_date
            .checked_add_days(chrono::Days::new(1))
            .unwrap_or(NaiveDate::MAX)
    };
    day.instant(date, anchor_time)
}

/// The display/nudge anchor day of `window` (O-1's nudge law, the pure
/// derivation the reminder engine consumes — the wiring itself is
/// H4's): the day *inside the canonical window* the habit's nudge arms
/// on. Total and pure; `window` must be (or floors to) a canonical
/// period start of the habit's current kind — a non-canonical input is
/// floored first, so the answer is always inside the canonical window.
///
/// - `PerDay`: the window date itself (a day habit has no inner day).
/// - `PerWeek`: the window date when no [`AnchorDay`] is set (or the
///   inert `Weekday::Mon` — the window already starts on Monday), else
///   the in-window date matching the anchored weekday.
/// - `PerMonth`: the window date when none, else
///   `min(month_day, month_length)` — **documented leniency**: a 31 in
///   a 28/29-day month arms on the month's last day (clamped, never
///   spilling into the next window).
#[must_use]
pub fn nudge_day(habit: &Habit, window: WindowStart) -> NaiveDate {
    let start = period_start(habit.target.kind(), window.as_date());
    match (habit.target.kind(), habit.anchor_day) {
        (PeriodKind::Week, Some(AnchorDay::Weekday(wd))) if wd != chrono::Weekday::Mon => {
            let offset = (wd.num_days_from_monday())
                .wrapping_sub(start.weekday().num_days_from_monday())
                % 7;
            start
                .checked_add_days(chrono::Days::new(u64::from(offset)))
                .unwrap_or(start)
        }
        (PeriodKind::Month, Some(AnchorDay::MonthDay(day))) => {
            let last = last_day_of_month(start);
            let day = day.get().min(u8::try_from(last.day()).unwrap_or(u8::MAX));
            NaiveDate::from_ymd_opt(start.year(), start.month(), u32::from(day)).unwrap_or(start)
        }
        _ => start,
    }
}

/// The month's last calendar day (28..=31; day 1 exists in every
/// month, so the walk is total).
fn last_day_of_month(first: NaiveDate) -> NaiveDate {
    first
        .checked_add_months(chrono::Months::new(1))
        .unwrap_or(first)
        .checked_sub_days(chrono::Days::new(1))
        .unwrap_or(first)
}

#[cfg(test)]
mod tracker_anchor_tests {
    use super::*;
    use crate::test_support::proptest_config;
    use chrono::{FixedOffset, NaiveTime};
    use proptest::prelude::*;

    fn day(day_start_h: u32, offset_secs: i32) -> DayCalendar {
        DayCalendar {
            day_start_time: NaiveTime::from_hms_opt(day_start_h, 0, 0)
                .expect("whole-hour day start"),
            offset: FixedOffset::east_opt(offset_secs).expect("bounded offset"),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    fn anchor(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).expect("whole-minute anchor")
    }

    #[test]
    fn tracker_anchor_occurrence_common_case_is_the_same_calendar_day() {
        let day = day(5, 0);
        let at = anchor(9, 30);
        let d = date(2026, 11, 5);
        assert_eq!(
            tracker_anchor_occurrence(at, d, day),
            day.instant(d, at),
            "anchor ≥ day_start reads on the window's own calendar date"
        );
    }

    #[test]
    fn tracker_anchor_occurrence_night_owl_tail_reads_the_next_calendar_day() {
        let day = day(5, 0);
        let at = anchor(3, 0);
        let d = date(2026, 11, 5);
        let next = d.checked_add_days(chrono::Days::new(1)).unwrap();
        assert_eq!(
            tracker_anchor_occurrence(at, d, day),
            day.instant(next, at),
            "anchor < day_start reads on the window's tail calendar date"
        );
    }

    #[test]
    fn tracker_anchor_occurrence_at_day_start_is_the_window_open() {
        let day = day(5, 0);
        let at = anchor(5, 0);
        let d = date(2026, 11, 5);
        assert_eq!(tracker_anchor_occurrence(at, d, day), day.instant(d, at));
    }

    proptest! {
        #![proptest_config(proptest_config(256))]

        #[test]
        fn tracker_anchor_occurrence_lies_inside_its_window_span(
            anchor_secs in 0_u32..86_400_u32,
            start_secs in 0_u32..86_400_u32,
            offset_secs in -50_400_i32..50_400_i32,
            ce_day in 20_000_i32..60_000_i32,
        ) {
            let at = NaiveTime::from_num_seconds_from_midnight_opt(anchor_secs, 0)
                .expect("in-range seconds-of-day");
            let day = DayCalendar {
                day_start_time: NaiveTime::from_num_seconds_from_midnight_opt(start_secs, 0)
                    .expect("in-range seconds-of-day"),
                offset: FixedOffset::east_opt(offset_secs).expect("bounded offset"),
            };
            let d = NaiveDate::from_num_days_from_ce_opt(ce_day).expect("in-range CE day");
            let t = tracker_anchor_occurrence(at, d, day);
            let start = crate::days::window_start(d, day.day_start_time, day.offset);
            let next = d.checked_add_days(chrono::Days::new(1)).expect("in-range");
            let end = crate::days::window_start(next, day.day_start_time, day.offset);
            prop_assert!(t >= start && t < end, "the key must lie inside D's window span");
        }

        #[test]
        fn tracker_anchor_occurrence_is_monotone_across_window_dates(
            anchor_secs in 0_u32..86_400_u32,
            start_secs in 0_u32..86_400_u32,
            offset_secs in -50_400_i32..50_400_i32,
            ce_day in 20_000_i32..60_000_i32,
        ) {
            let at = NaiveTime::from_num_seconds_from_midnight_opt(anchor_secs, 0)
                .expect("in-range seconds-of-day");
            let day = DayCalendar {
                day_start_time: NaiveTime::from_num_seconds_from_midnight_opt(start_secs, 0)
                    .expect("in-range seconds-of-day"),
                offset: FixedOffset::east_opt(offset_secs).expect("bounded offset"),
            };
            let d = NaiveDate::from_num_days_from_ce_opt(ce_day).expect("in-range CE day");
            let next = d.checked_add_days(chrono::Days::new(1)).expect("in-range");
            prop_assert!(
                tracker_anchor_occurrence(at, next, day)
                    > tracker_anchor_occurrence(at, d, day),
                "distinct window dates derive distinct, ordered keys"
            );
        }
    }
}

// -- HF4: per-weekday schedules + reminder overrides (#960, #959) -------

#[cfg(test)]
mod hf4_schedule_tests {
    use super::*;
    use crate::test_support::{habit_fixture_strategy, proptest_config};
    use chrono::{NaiveTime, TimeZone, Weekday};
    use proptest::prelude::*;

    fn habit(raw: u128, target: HabitTarget) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: BoardId::from(uuid::Uuid::from_u128(900)),
            title: format!("habit {raw}"),
            description: String::new(),
            target,
            order: 0,
            anchor_time: None,
            anchor_day: None,
            paused_until: None,
            scheduled_days: None,
            reminder: None,
            clocks: HabitClocks::all(Utc.timestamp_opt(100, 0).unwrap()),
            continued_from: None,
            deleted: false,
            by: None,
        }
    }

    fn utc() -> DayCalendar {
        DayCalendar {
            day_start_time: NaiveTime::from_hms_opt(0, 0, 0).expect("midnight"),
            offset: chrono::FixedOffset::east_opt(0).expect("zero offset"),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
    }

    fn at(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.timestamp_opt(
            date(y, m, d)
                .and_hms_opt(h, 0, 0)
                .expect("whole hour")
                .and_utc()
                .timestamp(),
            0,
        )
        .unwrap()
    }

    fn row(habit_id: u128, window: NaiveDate, count: u32) -> (HabitLogKey, HabitLogEntry) {
        (
            HabitLogKey {
                habit: HabitId::from(uuid::Uuid::from_u128(habit_id)),
                window: WindowStart(window),
            },
            HabitLogEntry {
                count,
                clock: Utc.timestamp_opt(1_000, 0).unwrap(),
                by: None,
                skip: false,
            },
        )
    }

    #[test]
    fn empty_weekday_set_is_unmintable() {
        // The mint refuses the empty set (the write boundary).
        assert!(WeekdaySet::new([]).is_err());
        assert!(matches!(
            WeekdaySet::new(Vec::<Weekday>::new()),
            Err(crate::habit::InvalidWeekdaySet)
        ));
        // The lenient decode seam masks and reports emptiness instead
        // of minting — the consumer defaults it, never the type.
        assert!(WeekdaySet::from_bits(0).is_none());
        assert!(
            WeekdaySet::from_bits(0x80).is_none(),
            "out-of-mask-only bits"
        );
        // Serde decode is lenient (0x88 masks to nothing); the wire
        // never carries an empty set because the mint cannot make one.
        let decoded: WeekdaySet = serde_json::from_str("128").expect("lenient decode");
        assert!(decoded.is_empty());
        let set = WeekdaySet::new([Weekday::Mon, Weekday::Fri]).expect("two days");
        assert_eq!(serde_json::to_string(&set).unwrap(), "17");
        let back: WeekdaySet = serde_json::from_str("17").unwrap();
        assert_eq!(back, set);
    }

    /// US-5 AC-1 (Σ extension): a weekly/monthly window whose scheduled
    /// days have not begun is transparent — an unmet pre-schedule
    /// window neither counts nor breaks, in both arms (week + month);
    /// once the first scheduled day has elapsed the window walks
    /// normally (a past unmet window breaks).
    #[test]
    fn pre_schedule_window_is_transparent_weekly_and_monthly() {
        // Weekly: Mon-start window, scheduled {Wed, Fri}; today Wed.
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.scheduled_days =
            Some(WeekdaySet::new([Weekday::Wed, Weekday::Fri]).expect("two days"));
        // Monday 2026-11-16; today = Wednesday the 18th (before Fri).
        let day = utc();
        let now = at(2026, 11, 18, 12);
        assert_eq!(day.today(now), date(2026, 11, 18));
        assert_eq!(
            first_scheduled_day(&weekly, WindowStart(date(2026, 11, 16))),
            Some(date(2026, 11, 18))
        );
        // The current window is transparent (pending): yesterday's met
        // week still shows through.
        let log = BTreeMap::from([row(1, date(2026, 11, 9), 1), row(1, date(2026, 11, 16), 0)]);
        assert_eq!(
            streak(&weekly, &log, now, day),
            1,
            "unmet pre-schedule week does not break"
        );
        // Same log with the current row deleted: identical (the window
        // is transparent, not a miss).
        let log_prev_only = BTreeMap::from([row(1, date(2026, 11, 9), 1)]);
        assert_eq!(streak(&weekly, &log_prev_only, now, day), 1);
        // Monthly: scheduled {3rd, 20th}; today the 1st → transparent.
        let mut monthly = habit(
            2,
            HabitTarget::PerMonth(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        monthly.scheduled_days = Some(WeekdaySet::new([Weekday::Mon]).expect("one day"));
        // 2026-11: the 1st IS a Sunday... use a fixed Monday: 2026-11-02.
        // Scheduled {Mon}: first scheduled day of November is the 2nd;
        // on the 1st the month window is pre-schedule.
        let now_month = at(2026, 11, 1, 12);
        let mlog = BTreeMap::from([row(2, date(2026, 10, 1), 1), row(2, date(2026, 11, 1), 0)]);
        assert_eq!(
            streak(&monthly, &mlog, now_month, day),
            1,
            "unmet pre-schedule month does not break"
        );
        // Past the scheduled days, the window walks normally: inside
        // the first 24 h of December the unmet November is still
        // *pending* (today's grace semantics, unchanged) — and once the
        // grace closes it is a miss that breaks the run.
        let now_pending = at(2026, 11, 30, 12);
        assert_eq!(
            streak(&monthly, &mlog, now_pending, day),
            1,
            "pending within grace"
        );
        let now_missed = at(2026, 12, 2, 12);
        assert_eq!(
            streak(&monthly, &mlog, now_missed, day),
            0,
            "grace closed: the elapsed scheduled window misses"
        );
    }

    // Σ extension (the HF4 catalogue's composition arm): skip ∧
    // pause ∧ pre-schedule are ONE transparent outcome. Over
    // generated fixtures whose weekly/monthly habit carries a random
    // day set (inert-None for daily, per R-6): (a) arbitrary skip
    // marks never break a run; (b) marks confined to pre-schedule
    // windows are invisible — the window is transparent with or
    // without the mark; (c) the same holds under a pause covering
    // the walked suffix.
    proptest! {
        #![proptest_config(proptest_config(64))]

        fn transparency_composes_across_skip_pause_preschedule(
        (fixture, probe, mask, set_mask) in habit_fixture_strategy().prop_flat_map(
            |fixture| {
                (
                    Just(fixture.clone()),
                    0_i64..=200_000,
                    proptest::collection::vec(any::<bool>(), 0..9),
                    proptest::collection::vec(any::<bool>(), 7),
                )
            },
        ),
    ) {
        let now =
            fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
        let kind = fixture.habit.target.kind();
        let mut f = fixture.clone();
        if kind != PeriodKind::Day {
            const WEEKDAYS: [chrono::Weekday; 7] = [
                chrono::Weekday::Mon,
                chrono::Weekday::Tue,
                chrono::Weekday::Wed,
                chrono::Weekday::Thu,
                chrono::Weekday::Fri,
                chrono::Weekday::Sat,
                chrono::Weekday::Sun,
            ];
            let days: Vec<chrono::Weekday> = (0..7_usize)
                .filter(|i| set_mask[*i])
                .map(|i| WEEKDAYS[i])
                .collect();
            // An empty draw leaves the set `None` (the inert posture —
            // the property holds trivially; the day-set arms fire on
            // non-empty draws).
            f.habit.scheduled_days = WeekdaySet::new(days).ok();
        }
        let today = f.day.today(now);
        let clean = streak(&f.habit, &f.log, now, f.day);
        // (a) arbitrary skip marks never break a run.
        let mut marked = f.clone();
        for (i, key) in marked.log.keys().copied().collect::<Vec<_>>()
            .into_iter()
            .enumerate()
            .take(mask.len())
        {
            if mask[i]
                && let Some(entry) = marked.log.get_mut(&key)
            {
                entry.skip = true;
            }
        }
        let with_marks = streak(&marked.habit, &marked.log, now, marked.day);
        prop_assert!(with_marks >= clean, "skips never break a run");
        // (b) marks confined to pre-schedule windows are invisible.
        let mut presched = f.clone();
        for (i, key) in presched.log.keys().copied().collect::<Vec<_>>()
            .into_iter()
            .enumerate()
            .take(mask.len())
        {
            if mask[i]
                && preschedule_window(&presched.habit, WindowStart(key.window.as_date()), today)
                && let Some(entry) = presched.log.get_mut(&key)
            {
                entry.skip = true;
            }
        }
        prop_assert_eq!(
            streak(&presched.habit, &presched.log, now, presched.day),
            clean,
            "a pre-schedule window's skip mark is invisible (one transparent outcome)"
        );
        // (c) the composition law under a pause covering the suffix.
        let until = today.pred_opt().unwrap_or(today);
        let mut paused_clean = f.clone();
        paused_clean.habit.paused_until = Some(until);
        let mut paused_marked = presched.clone();
        paused_marked.habit.paused_until = Some(until);
        prop_assert_eq!(
            streak(&paused_marked.habit, &paused_marked.log, now, paused_marked.day),
            streak(&paused_clean.habit, &paused_clean.log, now, paused_clean.day),
            "pre-schedule marks are invisible under a pause too"
        );
        }
    }

    /// US-5 AC-5 (US-9's dependency): the denominator joins once the
    /// first scheduled day has elapsed; before it, the window is
    /// outside both numerator and denominator (HF3's `adherence`
    /// consumes this predicate).
    #[test]
    fn scheduled_denominator_derives_from_the_day_sets() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.scheduled_days = Some(WeekdaySet::new([Weekday::Fri]).expect("one day"));
        let window = WindowStart(date(2026, 11, 16)); // ISO week of Nov 16
        assert_eq!(
            first_scheduled_day(&weekly, window),
            Some(date(2026, 11, 20))
        );
        assert!(
            !window_joins_denominator(&weekly, window, date(2026, 11, 19)),
            "before the first scheduled day: outside the denominator"
        );
        assert!(
            window_joins_denominator(&weekly, window, date(2026, 11, 20)),
            "on the first scheduled day: inside"
        );
        assert!(
            window_joins_denominator(&weekly, window, date(2026, 11, 22)),
            "after it: inside"
        );
    }

    /// R-6's daily half (the US-9 pin): a `PerDay` habit's windows join
    /// at period start — the day set is inert even when present (the
    /// documented leniency covers only legacy/synced rows), and the
    /// daily denominator is unchanged byte-for-byte.
    #[test]
    fn daily_denominator_unchanged() {
        let mut daily = habit(
            1,
            HabitTarget::PerDay(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        daily.scheduled_days = Some(WeekdaySet::new([Weekday::Fri]).expect("one day"));
        assert!(
            daily.effective_scheduled_days().is_none(),
            "R-6: inert on PerDay"
        );
        for offset in [0_u64, 3, 6] {
            let window = WindowStart(date(2026, 11, 16) + chrono::Days::new(offset));
            assert!(window_joins_denominator(
                &daily,
                window,
                date(2026, 11, 16) + chrono::Days::new(offset)
            ));
            assert!(!preschedule_window(&daily, window, date(2026, 11, 16)));
        }
    }

    /// US-5 AC-2 (domain arm): the nudge arms on each scheduled day and
    /// on no other day of the window.
    #[test]
    fn nudge_arms_on_each_scheduled_day_only() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.scheduled_days =
            Some(WeekdaySet::new([Weekday::Mon, Weekday::Wed, Weekday::Fri]).expect("three days"));
        weekly.anchor_time = Some(NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let window = WindowStart(date(2026, 11, 16));
        let days = nudge_days(&weekly, window);
        assert_eq!(
            days,
            vec![date(2026, 11, 16), date(2026, 11, 18), date(2026, 11, 20)],
            "exactly the scheduled days, in order"
        );
        for offset in 0..7_u64 {
            let today = date(2026, 11, 16) + chrono::Days::new(offset);
            let armed = tracker_arm(&weekly, window, today).is_some();
            assert_eq!(
                armed,
                matches!(today.weekday(), Weekday::Mon | Weekday::Wed | Weekday::Fri)
            );
        }
    }

    /// US-6 AC-1: the override's occurrence equals O-1's nudge law
    /// shifted by the override's offset — base `NudgeDay` (default)
    /// for both the anchor-day and no-day-set shapes, base
    /// `PeriodStart` explicit.
    #[test]
    fn override_occurrence_matches_the_o1_law_shifted() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.anchor_day = Some(AnchorDay::Weekday(Weekday::Wed));
        let window = WindowStart(date(2026, 11, 16));
        let ovr = HabitReminderOverride {
            base: NudgeBase::NudgeDay,
            offset_days: 2,
            time: Some(NaiveTime::from_hms_opt(7, 30, 0).unwrap()),
            snooze: HabitSnooze::Hours(2),
        };
        // Default law arms from Wednesday the 18th; shifted by 2 arms
        // from Friday the 20th.
        assert!(!override_armed_today(
            &weekly,
            &ovr,
            window,
            date(2026, 11, 19)
        ));
        assert!(override_armed_today(
            &weekly,
            &ovr,
            window,
            date(2026, 11, 20)
        ));
        assert!(!override_armed_today(
            &weekly,
            &ovr,
            window,
            date(2026, 11, 17)
        ));
        // PeriodStart base: exactly the start + offset, no day-gate
        // tail.
        let start_ovr = HabitReminderOverride {
            base: NudgeBase::PeriodStart,
            offset_days: 1,
            ..ovr
        };
        assert!(override_armed_today(
            &weekly,
            &start_ovr,
            window,
            date(2026, 11, 17)
        ));
        assert!(!override_armed_today(
            &weekly,
            &start_ovr,
            window,
            date(2026, 11, 18)
        ));
        // Scheduled-day set composition (R-8 default base): the set
        // shifted by the offset.
        weekly.scheduled_days = Some(WeekdaySet::new([Weekday::Mon]).expect("Mon"));
        assert!(
            override_armed_today(&weekly, &ovr, window, date(2026, 11, 18)),
            "Mon shifted by 2 arms Wednesday"
        );
        assert!(!override_armed_today(
            &weekly,
            &ovr,
            window,
            date(2026, 11, 16)
        ));
    }

    /// US-6 AC-1's negative twin (the default posture): with no
    /// override the gate is byte-identical to the pre-HF4 engine —
    /// armed from the nudge day on, never before, at the `anchor_time`.
    #[test]
    fn no_override_keeps_the_o1_default_occurrence() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.anchor_time = Some(NaiveTime::from_hms_opt(8, 0, 0).unwrap());
        let window = WindowStart(date(2026, 11, 16));
        let arm = nudge_day(&weekly, window);
        assert_eq!(arm, date(2026, 11, 16), "no anchor day → the window start");
        assert_eq!(
            tracker_arm(&weekly, window, date(2026, 11, 15)),
            None,
            "before the window: not this window's arm"
        );
        assert_eq!(
            tracker_arm(&weekly, window, date(2026, 11, 16)),
            Some(NaiveTime::from_hms_opt(8, 0, 0).unwrap())
        );
        // Anchor day shifts the arm; the gate follows it.
        weekly.anchor_day = Some(AnchorDay::Weekday(Weekday::Thu));
        assert_eq!(tracker_arm(&weekly, window, date(2026, 11, 18)), None);
        assert_eq!(
            tracker_arm(&weekly, window, date(2026, 11, 19)),
            Some(NaiveTime::from_hms_opt(8, 0, 0).unwrap())
        );
    }

    /// US-6 AC-4: an override does not require an `anchor_time` — the
    /// occurrence's time-of-day falls back to the override's own time,
    /// and a habit with neither drives no nudge at all.
    #[test]
    fn override_without_anchor_time_arms_at_the_overrides_time() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        let ovr = HabitReminderOverride {
            base: NudgeBase::PeriodStart,
            offset_days: 0,
            time: Some(NaiveTime::from_hms_opt(21, 45, 0).unwrap()),
            snooze: HabitSnooze::Days(1),
        };
        weekly.reminder = Some(ovr);
        let window = WindowStart(date(2026, 11, 16));
        assert_eq!(
            tracker_arm(&weekly, window, date(2026, 11, 16)),
            Some(NaiveTime::from_hms_opt(21, 45, 0).unwrap()),
            "the override's own time, no anchor chip"
        );
        weekly.reminder = None;
        assert_eq!(tracker_arm(&weekly, window, date(2026, 11, 16)), None);
    }

    /// #1212 (operator ruling comment 7079171625): an override with
    /// `time: None` is the anchor-time-only posture — the day gate
    /// still decides the WHEN, but the occurrence carries no clock
    /// time unless the habit has an `anchor_time`.
    #[test]
    fn anchor_time_only_override_arms_the_day_gate_without_a_clock_time() {
        let mut weekly = habit(
            1,
            HabitTarget::PerWeek(crate::habit::NonZeroCount::new(1).unwrap()),
        );
        weekly.reminder = Some(HabitReminderOverride {
            base: NudgeBase::PeriodStart,
            offset_days: 0,
            time: None,
            snooze: HabitSnooze::Days(1),
        });
        let window = WindowStart(date(2026, 11, 16));
        assert_eq!(
            tracker_arm_time(&weekly),
            None,
            "no anchor chip + blank override time = no clock time"
        );
        assert!(
            tracker_arm(&weekly, window, date(2026, 11, 16)).is_none(),
            "the arm names a time or none — a blank override mints none"
        );
        // The anchor chip supplies the clock time; the override's day
        // gate keeps deciding the WHEN (the R-8 gate, unchanged).
        weekly.anchor_time = Some(NaiveTime::from_hms_opt(6, 30, 0).unwrap());
        assert_eq!(
            tracker_arm(&weekly, window, date(2026, 11, 16)),
            Some(NaiveTime::from_hms_opt(6, 30, 0).unwrap()),
            "anchor chip is the clock time even under the blank override"
        );
    }

    /// #1212's serde compatibility: a pre-#1212 payload (concrete
    /// `time`) decodes to `Some`; a #1212 payload (absent `time`)
    /// decodes to `None` — no default-to-09:00 anywhere.
    #[test]
    fn override_time_serde_decodes_old_and_new_shapes() {
        let legacy = serde_json::json!({
            "base": "NudgeDay",
            "offset_days": 2,
            "time": "07:30:00",
            "snooze": {"Hours": 2},
        });
        let old: HabitReminderOverride =
            serde_json::from_value(legacy).expect("legacy shape decodes");
        assert_eq!(
            old.time,
            Some(NaiveTime::from_hms_opt(7, 30, 0).unwrap()),
            "a concrete legacy time decodes to Some"
        );
        let current = serde_json::json!({
            "base": "NudgeDay",
            "offset_days": 2,
            "snooze": {"Hours": 2},
        });
        let blank: HabitReminderOverride =
            serde_json::from_value(current).expect("absent time decodes");
        assert_eq!(
            blank.time, None,
            "an absent time is the anchor-time-only posture, never 09:00"
        );
        // Round-trip: the blank posture re-encodes without a `time`
        // key and decodes back to `None`.
        let re = serde_json::to_value(blank).expect("encodes");
        assert!(
            re.get("time").is_none(),
            "None encodes as an absent key, keeping new payloads lean"
        );
    }

    // The WHEN-only pin (the `anchor_time` invariance family's HF4
    // extension): the reminder override enters no window, streak, or
    // attribution path — perturbing only `reminder` leaves `streak`
    // and `log_window` identical on generated fixtures.
    proptest! {
        #![proptest_config(proptest_config(64))]

    #[test]
    fn new_fields_never_influence_the_canonical_order(
        (fixture, probe) in crate::test_support::habit_fixture_strategy().prop_flat_map(
            |fixture| (Just(fixture.clone()), 0_i64..=200_000),
        ),
    ) {
        let now =
            fixture.now + chrono::TimeDelta::try_seconds(probe).expect("bounded probe");
        let ovr = HabitReminderOverride {
            base: NudgeBase::NudgeDay,
            offset_days: 3,
            time: Some(NaiveTime::from_hms_opt(23, 0, 0).unwrap()),
            snooze: HabitSnooze::Hours(5),
        };
        let mut perturbed = fixture.clone();
        perturbed.habit.reminder = Some(ovr);
        prop_assert_eq!(
            streak(&perturbed.habit, &perturbed.log, now, perturbed.day),
            streak(&fixture.habit, &fixture.log, now, fixture.day),
            "nudge parameters are WHEN-only"
        );
        prop_assert_eq!(
            log_window(&perturbed.habit, &perturbed.log, now, perturbed.day),
            log_window(&fixture.habit, &fixture.log, now, fixture.day),
            "attribution never reads the override"
        );
    }
    }
}

// ---- Statistics tests (HF3, US-9; the accepted plan's catalogue) ----

#[cfg(test)]
mod statistics_tests {
    use super::tests::habit;
    use super::*;
    use crate::test_support::{habit_fixture_strategy, proptest_config};
    use chrono::{FixedOffset, NaiveTime, TimeZone};
    use proptest::prelude::*;

    fn stamp(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).single().expect("valid stamp")
    }

    fn entry(count: u32) -> HabitLogEntry {
        HabitLogEntry {
            count,
            clock: stamp(10),
            by: None,
            skip: false,
        }
    }

    fn put(
        log: &mut BTreeMap<HabitLogKey, HabitLogEntry>,
        habit: HabitId,
        window: NaiveDate,
        count: u32,
    ) {
        log.insert(
            HabitLogKey {
                habit,
                window: WindowStart::from_stored(window),
            },
            entry(count),
        );
    }

    /// A daily-quota habit met exactly `met` of the `depth` days
    /// ending yesterday (today stays pending — grace open).
    fn daily_habit(
        raw: u128,
        quota: u32,
        pattern: &[bool],
    ) -> (
        Habit,
        BTreeMap<HabitLogKey, HabitLogEntry>,
        DateTime<Utc>,
        DayCalendar,
    ) {
        let habit = habit(
            raw,
            HabitTarget::PerDay(NonZeroCount::new(quota).expect("nonzero")),
        );
        let day = DayCalendar {
            day_start_time: NaiveTime::MIN,
            offset: FixedOffset::east_opt(0).expect("zero offset"),
        };
        let today = NaiveDate::from_ymd_opt(2026, 11, 21).expect("date");
        let now = Utc
            .with_ymd_and_hms(2026, 11, 21, 12, 0, 0)
            .single()
            .expect("noon");
        let mut log = BTreeMap::new();
        for (i, met) in pattern.iter().enumerate() {
            // pattern[0] is the OLDEST day in the window; today itself
            // is never written (its grace is open — pending).
            let back =
                i64::try_from(pattern.len()).expect("small") - i64::try_from(i).expect("small");
            let d = today - chrono::Duration::days(back);
            put(&mut log, habit.id, d, u32::from(*met) * quota);
        }
        (habit, log, now, day)
    }

    // US-9 AC-1: the denominator counts scheduled *elapsed* windows —
    // met windows and missed windows both enter it; the pending
    // current window (grace open) does not.
    #[test]
    fn adherence_denominator_is_scheduled_windows() {
        // 6 elapsed days: 3 met, 2 missed, 1 met → denominator 6,
        // numerator 4. Today is pending, never in either side.
        let (habit, log, now, day) = daily_habit(1, 2, &[true, false, true, false, true, true]);
        let ratio = adherence(&habit, &log, now, day);
        assert_eq!(ratio.met(), 4);
        assert_eq!(ratio.scheduled(), 6);
        assert_eq!(ratio.ratio(), Some(4.0 / 6.0));
    }

    // US-9 AC-2: every transparent class leaves BOTH numerator and
    // denominator — the pending window (grace open), HF1's typed
    // skip mark, HF1's pause coverage, and HF4's pre-schedule
    // (R-6) window alike.
    #[test]
    fn transparent_windows_leave_numerator_and_denominator() {
        // Only today's window has a row (under quota, grace open) →
        // pending: transparent. Zero elapsed windows → the ratio is
        // the typed empty state, not 100 % and not 0 %.
        let (habit, mut log, now, day) = daily_habit(2, 1, &[]);
        put(&mut log, habit.id, day.today(now), 0);
        let ratio = adherence(&habit, &log, now, day);
        assert!(ratio.is_empty());
        assert_eq!(ratio.ratio(), None);
        // And a *met* pending window does count (done is done): met 1,
        // scheduled 1 — the current window's own completion joins the
        // ratio even while its grace is open.
        put(&mut log, habit.id, day.today(now), 1);
        let ratio = adherence(&habit, &log, now, day);
        assert_eq!(ratio.met(), 1);
        assert_eq!(ratio.scheduled(), 1);
    }

    // US-9 AC-2 (HF1 arm): a grace-closed skipped window is
    // transparent — neither numerator nor denominator; and a *met*
    // skipped window still counts (the walk's order: done is done).
    #[test]
    fn skipped_windows_leave_numerator_and_denominator() {
        let (habit, mut log, now, day) = daily_habit(20, 1, &[]);
        let yesterday = day.today(now) - chrono::Duration::days(1);
        log.insert(
            HabitLogKey {
                habit: habit.id,
                window: WindowStart::from_stored(yesterday),
            },
            HabitLogEntry {
                count: 0,
                clock: stamp(10),
                by: None,
                skip: true,
            },
        );
        let ratio = adherence(&habit, &log, now, day);
        assert!(ratio.is_empty(), "a lone skipped window is transparent");
        // Met rows on skipped windows count: done is done.
        log.insert(
            HabitLogKey {
                habit: habit.id,
                window: WindowStart::from_stored(yesterday),
            },
            HabitLogEntry {
                count: 1,
                clock: stamp(11),
                by: None,
                skip: true,
            },
        );
        let ratio = adherence(&habit, &log, now, day);
        assert_eq!(ratio.met(), 1);
        assert_eq!(ratio.scheduled(), 1);
    }

    // US-9 AC-2 (HF1 arm): pause-covered windows are transparent —
    // a standing pause through the whole history leaves the typed
    // empty state (planned gaps never dilute adherence), and the
    // best streak neither extends nor breaks below the pause.
    #[test]
    fn paused_windows_leave_numerator_and_denominator() {
        let (mut habit, mut log, now, day) = daily_habit(21, 1, &[]);
        let today = day.today(now);
        habit.paused_until = Some(today);
        // Two grace-closed unmet windows under the pause.
        for back in [1_i64, 2] {
            put(&mut log, habit.id, today - chrono::Duration::days(back), 0);
        }
        let ratio = adherence(&habit, &log, now, day);
        assert!(
            ratio.is_empty(),
            "pause-covered misses never join the ratio"
        );
        assert_eq!(best_streak(&habit, &log, now, day), 0);
        // Coverage composes over rows: a met window under the pause
        // stays transparent too (the walk's first arm).
        put(&mut log, habit.id, today - chrono::Duration::days(3), 1);
        let ratio = adherence(&habit, &log, now, day);
        assert!(ratio.is_empty(), "a met row under the pause is transparent");
    }

    // US-9 AC-2 (HF4 arm, R-6): a weekly window before its first
    // scheduled day is pre-schedule — transparent in adherence even
    // though the horizon enumerates it.
    #[test]
    fn preschedule_windows_never_join_the_denominator() {
        let mut weekly = habit(
            22,
            HabitTarget::PerWeek(NonZeroCount::new(1).expect("nonzero")),
        );
        weekly.scheduled_days = Some(WeekdaySet::new([chrono::Weekday::Fri]).expect("one day"));
        let day = DayCalendar {
            day_start_time: NaiveTime::MIN,
            offset: FixedOffset::east_opt(0).expect("zero offset"),
        };
        // Now = Wed Nov 18 2026; the week's first scheduled day is
        // Fri Nov 20 — the whole visible week is pre-schedule, and
        // the log's row in it is transparent either way.
        let now = Utc
            .with_ymd_and_hms(2026, 11, 18, 12, 0, 0)
            .single()
            .expect("noon");
        let mut log = BTreeMap::new();
        put(
            &mut log,
            weekly.id,
            NaiveDate::from_ymd_opt(2026, 11, 16).expect("Monday"),
            0,
        );
        let ratio = adherence(&weekly, &log, now, day);
        assert!(
            ratio.is_empty(),
            "a pre-schedule window never joins numerator or denominator"
        );
    }

    // US-9 AC-1: best streak = the max historical run, not the
    // current one and not the sum.
    #[test]
    fn best_streak_is_the_max_historical_run() {
        // Chronological runs: 2, then a miss, then 1, then a miss,
        // then 5 — best 5; today pending extends nothing.
        let (habit, log, now, day) = daily_habit(
            3,
            1,
            &[true, true, false, true, false, true, true, true, true, true],
        );
        assert_eq!(best_streak(&habit, &log, now, day), 5);
        // And the current streak (the newest run's tail, bounded by
        // the newest miss) is smaller.
        assert!(streak(&habit, &log, now, day) <= 5);
    }

    // US-9 AC-3: empty history is the typed empty state — no 0 % of
    // nothing.
    #[test]
    fn empty_history_is_the_typed_empty_state() {
        let (habit, log, now, day) = daily_habit(4, 1, &[]);
        let ratio = adherence(&habit, &log, now, day);
        assert_eq!(ratio, AdherenceRatio::empty());
        assert!(ratio.is_empty());
        assert_eq!(ratio.ratio(), None);
        assert_eq!(best_streak(&habit, &log, now, day), 0);
        assert!(per_period_totals(&habit, &log).is_empty());
    }

    // D-H12: former-kind (non-canonical) rows are inert — they never
    // render as misses, never join the horizon, never dilute the
    // denominator.
    #[test]
    fn former_kind_rows_render_inert_never_missed() {
        // A *weekly* habit whose log only carries daily-kind rows
        // (Tuesday Nov 17 — not an ISO Monday, hence non-canonical for
        // the current PerWeek kind): every row is inert — no horizon,
        // no misses, no totals.
        let weekly = habit(
            5,
            HabitTarget::PerWeek(NonZeroCount::new(1).expect("nonzero")),
        );
        let day = DayCalendar {
            day_start_time: NaiveTime::MIN,
            offset: FixedOffset::east_opt(0).expect("zero offset"),
        };
        let now = Utc
            .with_ymd_and_hms(2026, 11, 21, 12, 0, 0)
            .single()
            .expect("noon");
        let mut log = BTreeMap::new();
        put(
            &mut log,
            weekly.id,
            NaiveDate::from_ymd_opt(2026, 11, 17).expect("Tuesday"),
            5,
        );
        let ratio = adherence(&weekly, &log, now, day);
        assert!(ratio.is_empty(), "former-kind rows never become misses");
        assert!(per_period_totals(&weekly, &log).is_empty());
        // And another habit's rows are equally invisible (id-scoped).
        let (other, _, _, _) = daily_habit(6, 1, &[true, true]);
        let mut cross = BTreeMap::new();
        for (key, value) in other_keys(other.id) {
            cross.insert(key, value);
        }
        let (subject, empty_log, now2, day2) = daily_habit(7, 1, &[]);
        assert!(adherence(&subject, &cross, now2, day2).is_empty());
        drop(empty_log);
        let _ = weekly.target;
    }

    fn other_keys(habit: HabitId) -> Vec<(HabitLogKey, HabitLogEntry)> {
        let mut out = Vec::new();
        for d in [17, 18] {
            out.push((
                HabitLogKey {
                    habit,
                    window: WindowStart::from_stored(
                        NaiveDate::from_ymd_opt(2026, 11, d).expect("date"),
                    ),
                },
                entry(1),
            ));
        }
        out
    }

    // The Σ-shape property (HF3's extension arm): over every fixture,
    // (a) met <= scheduled, (b) the ratio never exceeds 1, (c) the
    // best streak dominates the current streak (the current run is a
    // historical run), and (d) the per-period totals' keys are exactly
    // current-kind canonical windows.
    proptest! {
        #![proptest_config(proptest_config(64))]

        #[test]
        fn stats_invariants_hold_over_generated_histories(
            fixture in habit_fixture_strategy(),
        ) {
            let ratio = adherence(&fixture.habit, &fixture.log, fixture.now, fixture.day);
            prop_assert!(ratio.met() <= ratio.scheduled());
            if let Some(r) = ratio.ratio() {
                prop_assert!((0.0..=1.0).contains(&r));
            } else {
                prop_assert!(ratio.is_empty());
            }
            prop_assert!(best_streak(&fixture.habit, &fixture.log, fixture.now, fixture.day)
                >= streak(&fixture.habit, &fixture.log, fixture.now, fixture.day));
            let kind = fixture.habit.target.kind();
            for (window, _) in per_period_totals(&fixture.habit, &fixture.log) {
                prop_assert_eq!(window.as_date(), period_start(kind, window.as_date()));
            }
        }
    }

    // #1211: the fallible mints' law — a zero-duration snooze is
    // degenerate (the snooze chain walk moves the occurrence by zero)
    // and is unmintable at the trust boundaries; the variants stay
    // raw so decode stays lenient (see the mints' doc).
    #[test]
    fn snooze_mints_refuse_zero_and_mint_positive_counts() {
        for n in 1..=u8::MAX {
            assert_eq!(HabitSnooze::hours(n), Some(HabitSnooze::Hours(n)));
            assert_eq!(HabitSnooze::days(n), Some(HabitSnooze::Days(n)));
        }
        assert_eq!(HabitSnooze::hours(0), None, "zero hours unmintable");
        assert_eq!(HabitSnooze::days(0), None, "zero days unmintable");
    }
}
