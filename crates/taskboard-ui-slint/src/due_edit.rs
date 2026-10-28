// SPDX-License-Identifier: AGPL-3.0-only

//! The due-date editor's pure core (r2 D25): the typed grammar, the
//! live-echo phrase, and the calendar picker's cursor/month state —
//! no slint types, no IO, total on arbitrary input. The Slint side
//! renders model rows pushed from the adapter; all arithmetic lives
//! here and is unit- and property-tested.
//!
//! The grammar is deliberately two forms (design D25; natural
//! language stays deferred):
//!
//! - `YYYY-MM-DD` — committed as **00:00 UTC** of that date (a
//!   date-only entry means "that day begins"; `due_phrase` still
//!   renders the relative phrase);
//! - `YYYY-MM-DD HH:MM` — that UTC wall-clock instant (round 3
//!   D9/F15); exactly one space separates the parts, and out-of-range
//!   `HH`/`MM` refuse — never clamp. A time suffix on a relative
//!   form (`+2d 14:30`) is deliberately **not** accepted: the
//!   relative arm stays single-component;
//! - `+N<unit>` with unit ∈ `m | h | d | w` — relative to the
//!   caller's `now`, digits then one unit letter, mirroring the
//!   domain [`ReminderOffset`](taskboard_domain::ReminderOffset)
//!   text-form grammar so both UI text grammars read the same;
//! - empty input means **clear**;
//! - anything else is a typed refusal — no command, buffer kept.
//!
//! UTC is the grammar's zone: `14:30` means 14:30 UTC, and
//! [`format_entry`] is the inverse seed formatter — the two cannot
//! disagree by an offset because both are UTC-fixed and this crate
//! has no tz database.

use crate::presentation::DueKind;
use chrono::{
    DateTime, Datelike, Duration as ChronoDuration, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta,
    Timelike, Utc,
};

/// The typed refusal (r2 D25): carries no text — the caller keeps the
/// buffer and dispatches nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DueParseError;

/// Parses one typed entry against `now`. `Ok(None)` = clear;
/// `Ok(Some(instant))` = commit; `Err` = refuse.
///
/// # Errors
///
/// [`DueParseError`] when the entry matches neither grammar form —
/// the caller keeps the buffer and dispatches nothing.
pub fn parse(text: &str, now: DateTime<Utc>) -> Result<Option<DateTime<Utc>>, DueParseError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Some((date_part, time_part)) = trimmed.split_once(' ') {
        // Exactly one space (D9): a double space is a refusal, and
        // `NaiveTime`'s parse refuses any trailing junk — `HH:MM`
        // only, out-of-range fields refuse rather than clamp.
        if !time_part.starts_with(' ')
            && let Ok(date) = NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
            && let Ok(time) = NaiveTime::parse_from_str(time_part, "%H:%M")
        {
            return Ok(Some(NaiveDateTime::new(date, time).and_utc()));
        }
        return Err(DueParseError);
    }
    if let Ok(date) = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        let midnight = date.and_hms_opt(0, 0, 0).ok_or(DueParseError)?;
        return Ok(Some(midnight.and_utc()));
    }
    if let Some(instant) = parse_relative(trimmed, now) {
        return Ok(Some(instant));
    }
    Err(DueParseError)
}

/// The relative arm: `+N<unit>`, unit ∈ `m | h | d | w` — one
/// component, digits then one unit letter (the single-component shape
/// of the domain offset grammar, plus weeks).
fn parse_relative(text: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let body = text.strip_prefix('+')?;
    let digits = body.len() - body.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 || digits == body.len() {
        return None;
    }
    let (value, unit) = body.split_at(digits);
    let value = value.parse::<u64>().ok()?;
    let delta = match unit {
        "m" => ChronoDuration::try_minutes(i64::try_from(value).ok()?)?,
        "h" => ChronoDuration::try_hours(i64::try_from(value).ok()?)?,
        "d" => ChronoDuration::try_days(i64::try_from(value).ok()?)?,
        "w" => ChronoDuration::try_weeks(i64::try_from(value).ok()?)?,
        _ => return None,
    };
    now.checked_add_signed(delta)
}

/// The live echo under the field (r2 D25): the due phrase of the
/// parsed entry, or empty when the entry is empty or refused — the
/// phrase reuses the presentation bucketing; no second grammar.
#[must_use]
pub fn echo(text: &str, now: DateTime<Utc>) -> String {
    match parse(text, now) {
        Ok(None) => "clears the due date".to_owned(),
        Ok(Some(instant)) => format!("sets {}", format_entry(Some(instant))),
        Err(DueParseError) => String::new(),
    }
}

/// The seed formatter (round 3 D11): the inverse of [`parse`]'s
/// absolute forms — `None` is the empty buffer; a midnight instant
/// formats date-only; any other instant carries its UTC `HH:MM`.
/// `parse(format_entry(x)) == x` (truncated to the minute) is the
/// pinned round-trip.
#[must_use]
pub fn format_entry(value: Option<DateTime<Utc>>) -> String {
    match value {
        None => String::new(),
        Some(instant) if instant.time() == NaiveTime::MIN => instant.format("%Y-%m-%d").to_string(),
        Some(instant) => instant.format("%Y-%m-%d %H:%M").to_string(),
    }
}

/// One picker grid cell: `None` is a leading blank (the weekday
/// offset); `Some(date)` renders `date.day()` and may carry the
/// cursor, the today marker, or a tint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
    pub date: Option<NaiveDate>,
}

/// The calendar picker's pure state (r2 D25/R-d): cursor, displayed
/// month, and the optional time-of-day live in the adapter across
/// refreshes; the Slint side only renders pushed rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuePickerState {
    cursor: NaiveDate,
    month: NaiveDate,
    /// `None` is date-only (commits midnight — today's shape, so
    /// untouched steppers keep existing instants stable); `Some` is
    /// the picker's time-of-day the steppers move (round 3 D9).
    time: Option<NaiveTime>,
}

/// One 15-minute stepper press (round 3 D9).
pub const TIME_STEP_MINUTES: u32 = 15;

impl DuePickerState {
    /// Opens on `due`'s month (the anchor the user is editing) or
    /// today's when no due is set; the cursor sits on the due date or
    /// today. A non-midnight anchor seeds the time (the steppers pick
    /// up where the value is); a midnight/absent anchor stays
    /// date-only.
    #[must_use]
    pub fn open(today: NaiveDate, due: Option<DateTime<Utc>>) -> Self {
        let anchor = due.map_or(today, |instant| instant.date_naive());
        Self {
            cursor: anchor,
            month: first_of_month(anchor),
            time: due
                .map(|instant| instant.time())
                .filter(|time| *time != NaiveTime::MIN),
        }
    }

    #[must_use]
    pub fn cursor(&self) -> NaiveDate {
        self.cursor
    }

    /// The first of the displayed month.
    #[must_use]
    pub fn month(&self) -> NaiveDate {
        self.month
    }

    /// The picker's time-of-day (`None` = date-only).
    #[must_use]
    pub fn time(&self) -> Option<NaiveTime> {
        self.time
    }

    /// Moves the cursor by `n` days (±1 arrow, ±7 week). The
    /// displayed month follows the cursor (round 3 D12/F28 — the
    /// cursor can never leave the grid), so an arrow across a month
    /// edge flips the caption.
    pub fn move_days(&mut self, n: i64) {
        if let Some(delta) = ChronoDuration::try_days(n)
            && let Some(moved) = self.cursor.checked_add_signed(delta)
        {
            self.cursor = moved;
            self.month = first_of_month(moved);
        }
    }

    /// Steps the time-of-day by ±15 minutes (the `n`-fold step),
    /// wrapping over midnight — the date follows (`move_days`), a
    /// step is an instant move, not a wall-clock edit. A date-only
    /// picker stays date-only: the steppers are inert until a time
    /// exists (typed in or seeded).
    ///
    /// # Panics
    ///
    /// Never: the midnight anchor is a compile-time constant shape.
    pub fn step_time(&mut self, n: i32) {
        let Some(time) = self.time else {
            return;
        };
        let current = i64::from(time.num_seconds_from_midnight() / 60);
        let total = current + i64::from(n) * i64::from(TIME_STEP_MINUTES);
        let wrapped = total.rem_euclid(i64::from(24 * 60));
        if total < 0 {
            self.move_days(-1);
        } else if total >= i64::from(24 * 60) {
            self.move_days(1);
        }
        self.time = Some(
            NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
                + ChronoDuration::try_minutes(wrapped).expect("wrapped minutes fit"),
        );
    }

    /// The commit instant: the cursor's date at the picker's time —
    /// midnight when date-only (today's shape). Enter and the day
    /// cells both funnel through this.
    ///
    /// # Panics
    ///
    /// Never on calendar dates: the fallback anchors on the cursor,
    /// which always has a midnight.
    #[must_use]
    pub fn commit_instant(&self) -> DateTime<Utc> {
        let (hour, minute) = self.time.map_or((0, 0), |t| (t.hour(), t.minute()));
        self.cursor
            .and_hms_opt(hour, minute, 0)
            .unwrap_or_else(|| {
                self.cursor
                    .and_hms_opt(0, 0, 0)
                    .expect("a date has a midnight")
            })
            .and_utc()
    }

    /// Switches the displayed month by `n` (PageUp/PageDown); the
    /// cursor follows, clamped into the month (Jan 31 → Feb 28).
    pub fn move_months(&mut self, n: i32) {
        let Some(shifted) = shift_month(self.month, n) else {
            return;
        };
        self.month = shifted;
        self.cursor = clamp_into_month(self.cursor, self.month);
    }

    /// Home/End: the month's first/last day (the month stays put).
    pub fn month_edge(&mut self, first: bool) {
        self.cursor = if first {
            self.month
        } else {
            last_of_month(self.month)
        };
    }

    /// A day-cell click: the cursor lands on the cell (click ≡
    /// cursor+Enter by construction at the seam).
    pub fn set_cursor(&mut self, date: NaiveDate) {
        self.cursor = date;
        self.month = first_of_month(date);
    }

    /// The displayed month's grid: leading weekday blanks then one
    /// cell per day — always a multiple of 7 (Monday-first).
    #[must_use]
    pub fn grid(&self) -> Vec<GridCell> {
        let leading = weekday_index(self.month);
        let days = days_in_month(self.month);
        let mut cells = vec![GridCell { date: None }; leading];
        cells.extend((1..=days).map(|day| GridCell {
            date: self.month.with_day(day),
        }));
        while !cells.len().is_multiple_of(7) {
            cells.push(GridCell { date: None });
        }
        cells
    }
}

/// The month grid's tint classification (r2 D25): the
/// [`DueKind`] semantics a *date* carries relative to `today` —
/// past days read overdue, today and the due-soon horizon read soon,
/// later days later. No domain predicate duplication: the horizon is
/// the same typed value the presentation layer's `due_sets` consumes
/// (round 3 D14 — one knob everywhere).
///
/// # Panics
///
/// Never on calendar dates: the `unwrap_or_else` anchors on `today`,
/// which always has a midnight.
#[must_use]
pub fn cell_kind(date: NaiveDate, today: NaiveDate, horizon: TimeDelta) -> DueKind {
    let midnight = |date: NaiveDate| {
        date.and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| today.and_hms_opt(0, 0, 0).expect("today has a midnight"))
            .and_utc()
    };
    let date_start = midnight(date);
    let today_start = midnight(today);
    if date_start < today_start {
        DueKind::Overdue
    } else if instant_or_end(date) <= today_start + horizon {
        DueKind::Soon
    } else {
        DueKind::Later
    }
}

fn instant_or_end(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(23, 59, 59)
        .unwrap_or_else(|| date.and_hms_opt(0, 0, 0).expect("a date has a midnight"))
        .and_utc()
}

/// Monday-first weekday index (0..6).
fn weekday_index(date: NaiveDate) -> usize {
    (date.weekday().number_from_monday() - 1) as usize
}

fn first_of_month(date: NaiveDate) -> NaiveDate {
    date.with_day(1).unwrap_or(date)
}

fn last_of_month(date: NaiveDate) -> NaiveDate {
    let next = shift_month(first_of_month(date), 1).unwrap_or(date);
    next.checked_add_signed(ChronoDuration::try_days(-1).unwrap_or(ChronoDuration::zero()))
        .unwrap_or(date)
}

fn days_in_month(date: NaiveDate) -> u32 {
    last_of_month(date).day()
}

fn shift_month(date: NaiveDate, n: i32) -> Option<NaiveDate> {
    let total = date.year() * 12 + i32::try_from(date.month()).unwrap_or(1) - 1 + n;
    let year = total.div_euclid(12);
    let month = total.rem_euclid(12) as u32 + 1;
    NaiveDate::from_ymd_opt(year, month, 1)
}

fn clamp_into_month(cursor: NaiveDate, month: NaiveDate) -> NaiveDate {
    // Reconstructed whole, not field-patched: a chained `with_month`/
    // `with_day` walk hits intermediate invalid dates (Oct 31 → Feb)
    // whose `None`s silently skipped the clamp and left the cursor
    // behind (the proptest-found desync).
    NaiveDate::from_ymd_opt(
        month.year(),
        month.month(),
        cursor.day().min(days_in_month(month)),
    )
    .unwrap_or(cursor)
}

/// A relative parse hits `now + N·unit` exactly.
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};
    use proptest::prelude::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2030, 1, 31, 12, 34, 56).unwrap()
    }

    #[test]
    fn iso_parses_to_midnight_utc() {
        let parsed = parse("2030-02-28", now()).expect("valid ISO");
        assert_eq!(
            parsed,
            Some(Utc.with_ymd_and_hms(2030, 2, 28, 0, 0, 0).unwrap())
        );
    }

    #[test]
    fn empty_means_clear() {
        assert_eq!(parse("", now()), Ok(None));
        assert_eq!(parse("   ", now()), Ok(None));
    }

    #[test]
    fn relative_units_hit_now_plus_n() {
        for (text, delta) in [
            ("+0m", ChronoDuration::zero()),
            ("+2m", ChronoDuration::try_minutes(2).unwrap()),
            ("+3h", ChronoDuration::try_hours(3).unwrap()),
            ("+2d", ChronoDuration::try_days(2).unwrap()),
            ("+2w", ChronoDuration::try_weeks(2).unwrap()),
        ] {
            assert_eq!(
                parse(text, now()).expect("valid relative"),
                Some(now() + delta),
                "{text}"
            );
        }
    }

    #[test]
    fn junk_refuses() {
        for text in ["soon", "2030-13-01", "next friday", "+", "+2x", "2d", "-2d"] {
            assert_eq!(parse(text, now()), Err(DueParseError), "{text}");
        }
    }

    #[test]
    fn echo_phrases_both_forms_and_refuses_silently() {
        assert_eq!(echo("", now()), "clears the due date");
        assert_eq!(echo("2030-02-28", now()), "sets 2030-02-28");
        assert_eq!(echo("junk", now()), "");
    }

    #[test]
    fn date_time_parses_to_the_utc_wall_clock() {
        let parsed = parse("2030-01-31 14:30", now()).expect("valid time form");
        assert_eq!(
            parsed,
            Some(Utc.with_ymd_and_hms(2030, 1, 31, 14, 30, 0).unwrap())
        );
    }

    #[test]
    fn time_form_edges_refuse() {
        for text in [
            "2030-01-31 24:00",
            "2030-01-31 12:60",
            "2030-01-31  14:30",   // double space
            "2030-01-3114:30",     // no space
            "+2d 14:30",           // no time suffix on the relative form
            "2030-01-31 14:30:15", // seconds are not the grammar
        ] {
            assert_eq!(parse(text, now()), Err(DueParseError), "{text}");
        }
    }

    #[test]
    fn echo_shows_the_time_only_when_present() {
        assert_eq!(echo("2030-01-31", now()), "sets 2030-01-31");
        assert_eq!(echo("2030-01-31 14:30", now()), "sets 2030-01-31 14:30");
    }

    #[test]
    fn format_entry_seeds_the_buffer_forms() {
        assert_eq!(format_entry(None), "");
        let midnight = Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
        assert_eq!(format_entry(Some(midnight)), "2030-01-31");
        let timed = Utc.with_ymd_and_hms(2030, 1, 31, 14, 30, 0).unwrap();
        assert_eq!(format_entry(Some(timed)), "2030-01-31 14:30");
    }

    #[test]
    fn picker_opens_on_the_due_anchor() {
        let due = Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
        let picker = DuePickerState::open(now().date_naive(), Some(due));
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 1, 31).unwrap()
        );
        assert_eq!(picker.month(), NaiveDate::from_ymd_opt(2030, 1, 1).unwrap());
        // January 2030 starts on a Tuesday: one leading blank, 31
        // cells, padded to 35.
        let grid = picker.grid();
        assert_eq!(grid.len(), 35);
        assert_eq!(grid[0], GridCell { date: None });
        assert_eq!(
            grid[1].date,
            Some(NaiveDate::from_ymd_opt(2030, 1, 1).unwrap())
        );
        assert_eq!(
            grid[31].date,
            Some(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap())
        );
    }

    #[test]
    fn page_keys_clamp_the_cursor_into_the_month() {
        let picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        let mut picker = picker;
        picker.move_months(1);
        assert_eq!(picker.month(), NaiveDate::from_ymd_opt(2030, 2, 1).unwrap());
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 28).unwrap()
        );
        picker.month_edge(true);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 1).unwrap()
        );
        picker.month_edge(false);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 28).unwrap()
        );
    }

    #[test]
    fn arrows_move_days_and_the_month_follows() {
        // Round 3 D12/F28 redefinition: the displayed month syncs to
        // the moved cursor — the "grid simply shows no cursor" desync
        // is gone.
        let mut picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        picker.move_days(1);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 1).unwrap()
        );
        assert_eq!(
            picker.month(),
            NaiveDate::from_ymd_opt(2030, 2, 1).unwrap(),
            "the month flipped with the cursor"
        );
        picker.move_days(-1);
        picker.move_days(-7);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 1, 24).unwrap()
        );
        assert_eq!(picker.month(), NaiveDate::from_ymd_opt(2030, 1, 1).unwrap());
    }

    #[test]
    fn time_steppers_step_fifteen_minutes_and_wrap_midnight() {
        let due = chrono::Utc
            .with_ymd_and_hms(2030, 1, 31, 14, 30, 0)
            .unwrap();
        let mut picker =
            DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), Some(due));
        assert_eq!(
            picker.time(),
            Some(NaiveTime::from_hms_opt(14, 30, 0).unwrap()),
            "open seeds a non-midnight anchor's time"
        );
        picker.step_time(1);
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc
                .with_ymd_and_hms(2030, 1, 31, 14, 45, 0)
                .unwrap()
        );
        // Wrap forward over midnight: the date follows.
        let late = chrono::Utc
            .with_ymd_and_hms(2030, 1, 31, 23, 50, 0)
            .unwrap();
        let mut picker =
            DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), Some(late));
        picker.step_time(1);
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc.with_ymd_and_hms(2030, 2, 1, 0, 5, 0).unwrap()
        );
        // Wrap backward: the date follows the other way.
        picker.step_time(-1);
        picker.step_time(-1);
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc
                .with_ymd_and_hms(2030, 1, 31, 23, 35, 0)
                .unwrap()
        );
    }

    #[test]
    fn date_only_picker_stays_midnight_until_a_time_exists() {
        let mut picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        assert_eq!(picker.time(), None, "no anchor time = date-only");
        picker.step_time(1);
        assert_eq!(picker.time(), None, "the steppers are inert on date-only");
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap()
        );
    }

    #[test]
    fn cell_kind_carries_the_due_semantics() {
        let today = NaiveDate::from_ymd_opt(2030, 1, 31).unwrap();
        let horizon = crate::presentation::DUE_SOON_HORIZON;
        assert_eq!(
            cell_kind(
                NaiveDate::from_ymd_opt(2030, 1, 30).unwrap(),
                today,
                horizon
            ),
            DueKind::Overdue
        );
        assert_eq!(cell_kind(today, today, horizon), DueKind::Soon);
        assert_eq!(
            cell_kind(
                NaiveDate::from_ymd_opt(2030, 2, 28).unwrap(),
                today,
                horizon
            ),
            DueKind::Later
        );
        // The horizon is the knob: a longer window reads soon what the
        // default reads later.
        let week = TimeDelta::try_days(7).unwrap();
        assert_eq!(
            cell_kind(NaiveDate::from_ymd_opt(2030, 2, 5).unwrap(), today, week),
            DueKind::Soon
        );
    }

    #[test]
    fn relative_and_domain_grammar_agree_on_the_shared_shape() {
        // "+45m" reads the same in both text grammars (the shape
        // mirror; the offset itself negates into its own domain).
        let offset =
            taskboard_domain::ReminderOffset::parse_text_form("45m").expect("domain grammar");
        let expected = now() + ChronoDuration::try_minutes(45).unwrap();
        assert_eq!(parse("+45m", now()).expect("ui grammar"), Some(expected));
        assert_eq!(offset.text_form(), "45m");
    }

    proptest::proptest! {
        /// The seed formatter and the parser agree on every instant
        /// ±10 y: `parse(format_entry(x)) == x` truncated to the
        /// minute — the date-shift guard (round 3 D9): both are
        /// UTC-fixed, so no offset can sneak between them.
        #[test]
        fn entry_round_trips_without_date_shift(secs in -316_224_000i64..=316_224_000) {
            let now = now();
            let instant = (now + ChronoDuration::try_seconds(secs).unwrap())
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap();
            let text = format_entry(Some(instant));
            let parsed = parse(&text, now).expect("formatted entries parse");
            assert_eq!(parsed, Some(instant), "{text}");
        }

        /// ISO round-trips: every date within ±10 y parses to its own
        /// midnight.
        #[test]
        fn iso_round_trips(days in -3650i64..=3650) {
            let now = now();
            let date = (now + ChronoDuration::try_days(days).unwrap()).date_naive();
            let text = date.format("%Y-%m-%d").to_string();
            let parsed = parse(&text, now).expect("formatted dates parse");
            let midnight = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
            assert_eq!(parsed, Some(midnight));
        }

        /// Relative parses are total on well-formed input and hit
        /// now + N·unit; saturating magnitudes refuse rather than
        /// wrap.
        #[test]
        fn relative_is_now_plus_n(value in 0i64..1_000_000, unit in 0..4u8) {
            let unit = ["m", "h", "d", "w"][unit as usize];
            let text = format!("+{value}{unit}");
            match parse(&text, now()) {
                Ok(Some(instant)) => {
                    let delta = ChronoDuration::try_minutes(value * match unit {
                        "m" => 1,
                        "h" => 60,
                        "d" => 60 * 24,
                        _ => 60 * 24 * 7,
                    })
                    .unwrap_or(ChronoDuration::zero());
                    assert_eq!(instant, now() + delta, "{text}");
                }
                // Only astronomic magnitudes may refuse; they never
                // wrap to a wrong instant.
                Ok(None) | Err(DueParseError) => {
                    prop_assert!(value > 100_000, "{text} should parse");
                }
            }
        }

        /// Arbitrary junk is a refusal or a valid instant — never a
        /// panic, never a wrong commit.
        #[test]
        fn junk_never_panics(text in ".*") {
            let _ = parse(&text, now());
        }

        /// Month-follow invariant (round 3 D12): after any move
        /// sequence — day arrows, month jumps, edges — the cursor is
        /// inside the displayed month. The caption can never drift
        /// off the cursor.
        #[test]
        fn the_cursor_stays_inside_the_displayed_month(
            moves in prop::collection::vec((-10i64..=10, 0..3u8), 0..24),
            seed in -3650i64..=3650,
        ) {
            let date = (now() + ChronoDuration::try_days(seed).unwrap()).date_naive();
            let mut picker = DuePickerState::open(date, None);
            for &(amount, kind) in &moves {
                match kind {
                    0 => picker.move_days(amount),
                    1 => picker.move_months(i32::try_from(amount).unwrap_or(0)),
                    _ => picker.month_edge(amount >= 0),
                }
                prop_assert_eq!(picker.cursor().month(), picker.month().month());
                prop_assert_eq!(picker.cursor().year(), picker.month().year());
            }
        }

        /// The grid is always 7-divisible, blanks lead, and every
        /// non-blank cell stays inside the displayed month.
        #[test]
        fn grid_invariants(days in 0i64..=4000) {
            let date = (now() + ChronoDuration::try_days(days).unwrap()).date_naive();
            let picker = DuePickerState::open(date, None);
            let grid = picker.grid();
            prop_assert_eq!(grid.len() % 7, 0);
            let leading = grid.iter().take_while(|c| c.date.is_none()).count();
            let trailing = grid.iter().rev().take_while(|c| c.date.is_none()).count();
            prop_assert!(leading < 7);
            prop_assert!(trailing < 7);
            let days_cells = &grid[leading..grid.len() - trailing];
            for cell in days_cells {
                let cell_date = cell.date.expect("the run between blanks is all days");
                prop_assert_eq!(cell_date.month(), picker.month().month());
                prop_assert_eq!(cell_date.year(), picker.month().year());
            }
        }
    }
}
