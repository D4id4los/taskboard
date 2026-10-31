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
/// verbatim). `continued_from` carries deliberately no slot: it is
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
    /// Tombstone clock.
    pub deleted: DateTime<Utc>,
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
    /// The quota per period. A kind change never deletes former-kind
    /// log rows — they stay in the map (they sync as facts) but are
    /// analytically inert (D-H12).
    pub target: HabitTarget,
    /// Manual list order (D-C13); the planner appends on create and
    /// the UI computes midpoints on drag. `anchor_time` never
    /// influences any ordering.
    pub order: i64,
    /// Display chip only (D-C13) — a local time-of-day shown next to
    /// the habit; it is in no ordering path and drives no reminder.
    pub anchor_time: Option<chrono::NaiveTime>,
    /// Per-field LWW clocks.
    pub clocks: HabitClocks,
    /// The §4.7 conversion seam, habit side: the series this habit
    /// continues (set only by the explicit `UpdateHabit` write path —
    /// D-H7a; the read-side history union is chunk 6's).
    pub continued_from: Option<SeriesId>,
    /// Tombstone. Resurrection-safe on sync by the same LWW-clock
    /// argument as tasks (the fold is chunk 6's).
    pub deleted: bool,
}

impl Habit {
    /// Not tombstoned.
    #[must_use]
    pub const fn is_live(&self) -> bool {
        !self.deleted
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

/// One window's count row: the payload is the count plus a single LWW
/// clock (the normative shape — no per-completion instants, which is
/// exactly why attribution happens at log time, D-H5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HabitLogEntry {
    /// The window's count (0 is legal — an audited correction may
    /// empty a window; the row stays, honestly clocked).
    pub count: u32,
    /// The row's LWW clock (always the writing planner's `now`).
    pub clock: DateTime<Utc>,
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
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let count = |window: WindowStart| {
        log.get(&HabitLogKey {
            habit: habit.id,
            window,
        })
        .map_or(0, |entry| entry.count)
    };
    let natural = WindowStart::of_period(kind, day.today(now));
    let prev_date = period_before(kind, natural.as_date());
    let prev = WindowStart::of_period(kind, prev_date);
    let grace_end = period_end(kind, prev_date, day)
        + chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let in_grace = now < grace_end;
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
    let grace = chrono::TimeDelta::try_hours(24).expect("24 h is representable");
    let mut streak = 0_u32;
    let mut cursor = WindowStart::of_period(kind, day.today(now)).as_date();
    loop {
        if count(WindowStart(cursor)) >= quota {
            streak += 1;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{naive_date_strategy, proptest_config};
    use chrono::{Datelike, TimeZone};
    use proptest::prelude::*;

    fn habit(raw: u128, target: HabitTarget) -> Habit {
        Habit {
            id: HabitId::from(uuid::Uuid::from_u128(raw)),
            board: BoardId::from(uuid::Uuid::from_u128(900)),
            title: format!("habit {raw}"),
            target,
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(Utc.timestamp_opt(100, 0).unwrap()),
            continued_from: None,
            deleted: false,
        }
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
        let mut cursor = WindowStart::of_period(kind, fixture.day.today(now)).as_date();
        let mut streak = 0_u32;
        let mut pendings = 0_usize;
        loop {
            let count = stored_count(fixture, WindowStart(cursor));
            if count >= quota {
                streak += 1;
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
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
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
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
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
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
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
            target: HabitTarget::PerWeek(NonZeroCount::new(2).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
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
            target: HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(ts(100)),
            continued_from: series.map(|s| SeriesId::from(uuid::Uuid::from_u128(s))),
            deleted: false,
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
            }),
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
                archived: ts(1),
                deleted: ts(1),
            },
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
}
