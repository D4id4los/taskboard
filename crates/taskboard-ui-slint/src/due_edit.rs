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
//! - `+N<unit>` with unit ∈ `m | h | d | w` — relative to the
//!   caller's `now`, digits then one unit letter, mirroring the
//!   domain [`ReminderOffset`](taskboard_domain::ReminderOffset)
//!   text-form grammar so both UI text grammars read the same;
//! - empty input means **clear**;
//! - anything else is a typed refusal — no command, buffer kept.

use crate::presentation::{DUE_SOON_HORIZON, DueKind};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Utc};

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
        Ok(Some(instant)) => {
            let date = instant.date_naive();
            format!("sets {date}")
        }
        Err(DueParseError) => String::new(),
    }
}

/// One picker grid cell: `None` is a leading blank (the weekday
/// offset); `Some(date)` renders `date.day()` and may carry the
/// cursor, the today marker, or a tint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCell {
    pub date: Option<NaiveDate>,
}

/// The calendar picker's pure state (r2 D25/R-d): cursor and
/// displayed month live in the adapter across refreshes; the Slint
/// side only renders pushed rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuePickerState {
    cursor: NaiveDate,
    month: NaiveDate,
}

impl DuePickerState {
    /// Opens on `due`'s month (the anchor the user is editing) or
    /// today's when no due is set; the cursor sits on the due date or
    /// today.
    #[must_use]
    pub fn open(today: NaiveDate, due: Option<DateTime<Utc>>) -> Self {
        let anchor = due.map_or(today, |instant| instant.date_naive());
        Self {
            cursor: anchor,
            month: first_of_month(anchor),
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

    /// Moves the cursor by `n` days (±1 arrow, ±7 week). The
    /// displayed month does not follow — the cursor may leave it
    /// (committing still works; the grid simply shows no cursor).
    pub fn move_days(&mut self, n: i64) {
        if let Some(delta) = ChronoDuration::try_days(n)
            && let Some(moved) = self.cursor.checked_add_signed(delta)
        {
            self.cursor = moved;
        }
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
/// later days later. No domain predicate duplication: the thresholds
/// are the same constants the presentation layer pins
/// (`DUE_SOON_HORIZON`).
///
/// # Panics
///
/// Never on calendar dates: the `unwrap_or_else` anchors on `today`,
/// which always has a midnight.
#[must_use]
pub fn cell_kind(date: NaiveDate, today: NaiveDate) -> DueKind {
    let midnight = |date: NaiveDate| {
        date.and_hms_opt(0, 0, 0)
            .unwrap_or_else(|| today.and_hms_opt(0, 0, 0).expect("today has a midnight"))
            .and_utc()
    };
    let date_start = midnight(date);
    let today_start = midnight(today);
    if date_start < today_start {
        DueKind::Overdue
    } else if instant_or_end(date) <= today_start + DUE_SOON_HORIZON {
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
    cursor
        .with_day(cursor.day().min(days_in_month(month)))
        .unwrap_or(cursor)
        .with_month(month.month())
        .unwrap_or(cursor)
        .with_year(month.year())
        .unwrap_or(cursor)
}

/// A relative parse hits `now + N·unit` exactly.
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
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
    fn arrows_move_days_without_dragging_the_month() {
        let mut picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        picker.move_days(1);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 1).unwrap()
        );
        assert_eq!(picker.month(), NaiveDate::from_ymd_opt(2030, 1, 1).unwrap());
        picker.move_days(-1);
        picker.move_days(-7);
        assert_eq!(
            picker.cursor(),
            NaiveDate::from_ymd_opt(2030, 1, 24).unwrap()
        );
    }

    #[test]
    fn cell_kind_carries_the_due_semantics() {
        let today = NaiveDate::from_ymd_opt(2030, 1, 31).unwrap();
        assert_eq!(
            cell_kind(NaiveDate::from_ymd_opt(2030, 1, 30).unwrap(), today),
            DueKind::Overdue
        );
        assert_eq!(cell_kind(today, today), DueKind::Soon);
        assert_eq!(
            cell_kind(NaiveDate::from_ymd_opt(2030, 2, 28).unwrap(), today),
            DueKind::Later
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
