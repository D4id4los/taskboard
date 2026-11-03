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
/// The picker's key vocabulary (round-3 D25): the closed enum of the
/// ten widget codes plus the commit — the registry's `picker.*`
/// actions map onto it and the click seams (day cells, month/time
/// `IconButtons`) reuse the same codes. Time steppers stay
/// button-only: no preset binds them, the enum carries them for the
/// click seam alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePickerKey {
    Left,
    Right,
    Up,
    Down,
    PrevMonth,
    NextMonth,
    MonthStart,
    MonthEnd,
    Commit,
    TimeBack,
    TimeForward,
}

impl DatePickerKey {
    /// The widget's `picker-key(int)` code space (the click seams).
    #[must_use]
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            2 => Some(Self::Up),
            3 => Some(Self::Down),
            4 => Some(Self::PrevMonth),
            5 => Some(Self::NextMonth),
            6 => Some(Self::MonthStart),
            7 => Some(Self::MonthEnd),
            8 => Some(Self::Commit),
            9 => Some(Self::TimeBack),
            10 => Some(Self::TimeForward),
            _ => None,
        }
    }
}

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

/// The grid's reserved cell count (r4 D22): six week-rows always.
const GRID_CELLS: usize = 42;

/// The time combobox's lenient parse (r4 D26): zone-free, total on
/// arbitrary input, never panics, never clamps — every out-of-range
/// field refuses (the grammar's rule). The accepted forms:
///
/// - 1–2 digits: the hour, minutes 00 (`9`, `09` → 09:00);
/// - 3–4 digits: the last two are the minutes, the rest the hour
///   (`930` → 09:30, `0930` → 09:30);
/// - `H:MM` / `HH:MM` — the strict form (`9:45`, `09:45`);
/// - leading/trailing whitespace is trimmed; the digit forms are
///   digits only.
///
/// Deliberate leniency boundary: `9:5` refuses (minutes need two
/// digits); seconds are not the grammar; hour > 23 or minute > 59
/// refuse.
#[must_use]
pub fn parse_time_lenient(text: &str) -> Option<NaiveTime> {
    let text = text.trim();
    let time_of = |hour: u32, minute: u32| NaiveTime::from_hms_opt(hour, minute, 0);
    if let Some((hour, minute)) = text.split_once(':') {
        // The strict form: the hour 1–2 digits, the minutes exactly
        // two — `9:5` refuses.
        let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let hour_ok = (1..=2).contains(&hour.len()) && digits(hour);
        let minute_ok = minute.len() == 2 && digits(minute);
        if !hour_ok || !minute_ok {
            return None;
        }
        return time_of(hour.parse().ok()?, minute.parse().ok()?);
    }
    // The digit forms: digits only after trim.
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (hour, minute) = match text.len() {
        1 | 2 => (text.parse::<u32>().ok()?, 0),
        3 | 4 => {
            let (h, m) = text.split_at(text.len() - 2);
            (h.parse::<u32>().ok()?, m.parse::<u32>().ok()?)
        }
        _ => return None,
    };
    time_of(hour, minute)
}

/// The wrapping ±[`TIME_STEP_MINUTES`] math (r4 D26): the wall-clock
/// wrap generalized out of [`DuePickerState::step_time`] — one
/// implementation, two callers (the picker keeps its date-following
/// behavior around this; the combobox consumes it directly).
///
/// # Panics
///
/// Never: the midnight anchor is a compile-time constant shape.
#[must_use]
pub fn step_minutes(time: NaiveTime, n: i32) -> NaiveTime {
    let current = i64::from(time.num_seconds_from_midnight() / 60);
    let total = current + i64::from(n) * i64::from(TIME_STEP_MINUTES);
    let wrapped = total.rem_euclid(i64::from(24 * 60));
    NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
        + ChronoDuration::try_minutes(wrapped).expect("wrapped minutes fit")
}

/// The quick list's pure source (r4 D26): the 96 quarter-hour slots
/// of the day, `00:00` first.
#[must_use]
pub fn quarter_slots() -> Vec<NaiveTime> {
    (0..24 * 4)
        .map(|slot| step_minutes(NaiveTime::MIN, slot))
        .collect()
}

/// The quick list's digit-prefix filter (r4 D26): the typed digits
/// match a slot when the slot's canonical 4-digit rendering
/// (`"0930"` for 09:30) *or* its leading-zero-stripped rendering
/// (`"930"`) starts with them — `9` matches the 09-hundred family
/// via the de-zeroed `"900"`, `930` matches 09:30, `0` matches
/// everything starting `0…`. Empty text matches everything. The
/// digits argument is the *stripped* digit prefix the caller derives
/// (`filter_digits`); non-digit characters never reach here.
#[must_use]
pub fn quick_list_matches(typed_digits: &str, slot: NaiveTime) -> bool {
    if typed_digits.is_empty() {
        return true;
    }
    let canonical = slot.format("%H%M").to_string();
    let de_zeroed = canonical.trim_start_matches('0').to_owned();
    canonical.starts_with(typed_digits) || de_zeroed.starts_with(typed_digits)
}

/// The typed text stripped to its digit prefix (r4 D26): the filter's
/// input — non-digit characters are dropped; an empty result means
/// "show all slots".
#[must_use]
pub fn filter_digits(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// The combobox's canonical `HH:MM` rendering (r4 D26).
#[must_use]
pub fn format_time_hm(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

/// Minutes-of-day ⇄ time (r4 D26): the quick-list pick seam carries
/// minutes; the picker state is a `NaiveTime`. `time_of_minutes`
/// refuses anything outside `0..1439` (never wraps — the pick seam is
/// adapter-generated).
#[must_use]
pub fn minutes_of_day(time: NaiveTime) -> i32 {
    i32::try_from(time.num_seconds_from_midnight() / 60).unwrap_or(0)
}

/// # Errors
///
/// `None` when `minutes` is outside `0..1439`.
///
/// # Panics
///
/// Never: the midnight anchor is a compile-time constant shape.
#[must_use]
pub fn time_of_minutes(minutes: i32) -> Option<NaiveTime> {
    if !(0..24 * 60).contains(&minutes) {
        return None;
    }
    Some(
        NaiveTime::from_hms_opt(0, 0, 0).expect("midnight")
            + ChronoDuration::try_minutes(i64::from(minutes)).expect("minutes fit"),
    )
}

/// The nearest slot's index in `slots` (r4 D26's scroll-to-nearest):
/// the slot with the smallest minutes distance to `time`; 0 when
/// `time` is `None` (the list opens on 00:00). Ties resolve upward.
#[must_use]
pub fn nearest_slot(slots: &[NaiveTime], time: Option<NaiveTime>) -> usize {
    let Some(time) = time else {
        return 0;
    };
    let minutes = minutes_of_day(time);
    slots
        .iter()
        .enumerate()
        .min_by_key(|(index, slot)| {
            let distance = (minutes_of_day(**slot) - minutes).abs();
            (distance, *index)
        })
        .map_or(0, |(index, _)| index)
}

/// The time combobox's tri-state validity probe (r4 D27): empty or
/// the unset display (`--:--`) is `None` (neutral); a lenient-parse
/// hit is `Some(true)`; anything else `Some(false)`. Tri-state only —
/// the copy never crosses this seam.
#[must_use]
pub fn time_field_valid(text: &str) -> Option<bool> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "--:--" {
        return None;
    }
    Some(parse_time_lenient(trimmed).is_some())
}

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

    /// Sets the picker's time-of-day (r4 D26): `None` returns to
    /// date-only. The combobox's typed-entry and quick-pick seams are
    /// the writers; the commit funnel reads it back.
    pub fn set_time(&mut self, time: Option<NaiveTime>) {
        self.time = time;
    }

    /// The sync helper's constructor (r4 D25): a picker state at
    /// `cursor` with the given time-of-day — the month follows the
    /// cursor.
    #[must_use]
    pub fn from_parts(cursor: NaiveDate, time: Option<NaiveTime>) -> Self {
        Self {
            cursor,
            month: first_of_month(cursor),
            time,
        }
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
    /// step is an instant move, not a wall-clock edit. An unset time
    /// is **seeded at midnight first** (r4 D23): the first `+` click
    /// yields 00:15, the first `−` click 23:45 on the previous day.
    /// A time exists exactly when something created one — a step or
    /// a typed entry; until then the picker stays date-only.
    ///
    /// # Panics
    ///
    /// Never: the midnight anchor is a compile-time constant shape.
    pub fn step_time(&mut self, n: i32) {
        let time = *self.time.get_or_insert(NaiveTime::MIN);
        let current = i64::from(time.num_seconds_from_midnight() / 60);
        let total = current + i64::from(n) * i64::from(TIME_STEP_MINUTES);
        if total < 0 {
            self.move_days(-1);
        } else if total >= i64::from(24 * 60) {
            self.move_days(1);
        }
        self.time = Some(step_minutes(time, n));
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
    /// cell per day, padded to **42 cells / 6 week-rows always**
    /// (r4 D22 — the six-row reserve): the panel's preferred height
    /// is constant while open, so the overlay's above/below fit test
    /// cannot flip mid-interaction when month paging changes the row
    /// count. Trailing blanks render nothing (the common calendar
    /// convention — every month the same grid height).
    #[must_use]
    pub fn grid(&self) -> Vec<GridCell> {
        let leading = weekday_index(self.month);
        let days = days_in_month(self.month);
        let mut cells = vec![GridCell { date: None }; leading];
        cells.extend((1..=days).map(|day| GridCell {
            date: self.month.with_day(day),
        }));
        cells.resize(GRID_CELLS, GridCell { date: None });
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

/// The bidirectional sync's source (r4 D25): which side moved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncSource {
    /// The picker moved (a cursor key, a stepper, a quick pick): the
    /// buffer re-renders from the selection.
    Picker,
    /// The buffer moved (typing): a parseable buffer re-seeds the
    /// picker.
    Buffer,
}

/// One bidirectional sync step (r4 D25): the one writer of both
/// sides. Returns `None` when the pair is already consistent — the
/// fixed point the no-oscillation property pins — or the fresh
/// `(buffer, picker)` pair; the picker half is `None` when only the
/// buffer moved.
///
/// Direction rules (pinned):
///
/// - `Picker`: buffer := the selection's rendering — date-only when
///   the picker's time is `None`, `date HH:MM` otherwise (the
///   two-midnights convention: an explicit `00:00` *stays* in the
///   rendering; normalizing it away would rewrite the user's text).
/// - `Buffer`: empty leaves the pair untouched (clearing is a
///   commit-time act, not a live one); junk or a partial prefix
///   leaves the picker untouched (the validity flavour shows the
///   state); a parseable buffer re-seeds the picker — cursor on the
///   date, the month follows, the time is `Some` exactly when the
///   text carried an `HH:MM` part (an explicit `00:00` counts — a
///   typed time exists).
#[must_use]
pub fn sync_buffer_picker(
    buffer: &str,
    picker: &DuePickerState,
    source: SyncSource,
    now: DateTime<Utc>,
) -> Option<(String, Option<DuePickerState>)> {
    match source {
        SyncSource::Picker => {
            let date = picker.cursor().format("%Y-%m-%d").to_string();
            let text = picker.time().map_or(date.clone(), |time| {
                format!("{date} {}", format_time_hm(time))
            });
            Some((text, None))
        }
        SyncSource::Buffer => match parse(buffer, now) {
            Ok(Some(instant)) => {
                let time = buffer
                    .trim()
                    .split_once(' ')
                    .and_then(|(_, time_part)| NaiveTime::parse_from_str(time_part, "%H:%M").ok());
                Some((
                    buffer.to_owned(),
                    Some(DuePickerState::from_parts(instant.date_naive(), time)),
                ))
            }
            Ok(None) | Err(DueParseError) => None,
        },
    }
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
        // cells, padded to the six-row reserve (42).
        let grid = picker.grid();
        assert_eq!(grid.len(), 42);
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
    fn grid_is_always_six_week_rows() {
        // (r4 D22) The three source shapes all reserve 42 cells: a
        // 4-row month (Feb 2021 — 28 days, picked by construction,
        // not by calendar trivia), a 5-row month (January 2030, the
        // journeys' month), and a natively 6-row month (December
        // 2029). Leading blanks stay < 7; the padding is trailing.
        for (year, month) in [(2021, 2), (2030, 1), (2029, 12)] {
            let month = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
            let picker = DuePickerState::open(month, None);
            let grid = picker.grid();
            assert_eq!(grid.len(), 42, "{year}-{month:02}");
            let leading = grid.iter().take_while(|c| c.date.is_none()).count();
            assert!(leading < 7, "{year}-{month:02}");
            let trailing = grid.iter().rev().take_while(|c| c.date.is_none()).count();
            assert!(
                trailing >= 42 - leading - grid.iter().filter(|c| c.date.is_some()).count(),
                "{year}-{month:02}: the reserve is trailing padding"
            );
        }
        // February 2021 is a known 28-cell month: 16 trailing blanks.
        let feb = DuePickerState::open(NaiveDate::from_ymd_opt(2021, 2, 1).unwrap(), None);
        let grid = feb.grid();
        assert_eq!(grid.iter().filter(|c| c.date.is_some()).count(), 28);
    }

    #[test]
    fn stepping_an_unset_time_seeds_midnight_then_steps() {
        // (r4 D23, the reconciled pin) While no time exists, the
        // commit stays the cursor's midnight — no phantom `00:00`
        // exists until something creates one.
        let mut picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        assert_eq!(picker.time(), None, "no anchor time = date-only");
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap()
        );
        // The first backward step seeds midnight and lands on the
        // previous day's 23:45 (a step is an instant move — the date
        // follows the wrap).
        picker.step_time(-1);
        assert_eq!(
            picker.time(),
            Some(NaiveTime::from_hms_opt(23, 45, 0).unwrap())
        );
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc
                .with_ymd_and_hms(2030, 1, 30, 23, 45, 0)
                .unwrap()
        );
        // A fresh seed stepping forward yields 00:15 the same day.
        let mut picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        picker.step_time(1);
        assert_eq!(
            picker.time(),
            Some(NaiveTime::from_hms_opt(0, 15, 0).unwrap())
        );
        assert_eq!(
            picker.commit_instant(),
            chrono::Utc.with_ymd_and_hms(2030, 1, 31, 0, 15, 0).unwrap()
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

    #[test]
    fn parse_time_lenient_truth_table() {
        // (r4 D26) The documented lenient forms.
        let hm = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        for (text, expected) in [
            ("9", Some(hm(9, 0))),
            ("09", Some(hm(9, 0))),
            ("930", Some(hm(9, 30))),
            ("0930", Some(hm(9, 30))),
            ("9:45", Some(hm(9, 45))),
            ("09:45", Some(hm(9, 45))),
            (" 9:45 ", Some(hm(9, 45))),
            ("0:00", Some(hm(0, 0))),
            ("23:59", Some(hm(23, 59))),
            ("2359", Some(hm(23, 59))),
            ("0000", Some(hm(0, 0))),
            // The deliberate refusals.
            ("9:5", None),   // minutes need two digits
            ("2450", None),  // hour > 23 — never clamp
            ("24:00", None), // never clamp
            ("9:60", None),  // minute > 59
            ("9x", None),    // non-digit junk
            ("", None),      // empty
            ("   ", None),   // whitespace-only
            ("12345", None), // too many digits
            ("9:450", None), // trailing minutes junk
        ] {
            assert_eq!(parse_time_lenient(text), expected, "{text}");
        }
    }

    #[test]
    fn step_minutes_wraps_midnight_both_ways() {
        let hm = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(step_minutes(hm(23, 50), 1), hm(0, 5), "wraps forward");
        assert_eq!(step_minutes(hm(0, 5), -1), hm(23, 50), "wraps backward");
        assert_eq!(
            step_minutes(hm(0, 0), -1),
            hm(23, 45),
            "backward from midnight"
        );
        assert_eq!(step_minutes(hm(0, 0), 1), hm(0, 15));
        assert_eq!(step_minutes(hm(12, 0), 0), hm(12, 0), "zero is a no-op");
    }

    #[test]
    fn quarter_slots_are_the_96_quarter_hours() {
        let slots = quarter_slots();
        assert_eq!(slots.len(), 96);
        let hm = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(slots[0], hm(0, 0));
        assert_eq!(slots[1], hm(0, 15));
        assert_eq!(slots[95], hm(23, 45));
        assert!(slots.windows(2).all(|pair| pair[0] < pair[1]), "ascending");
    }

    #[test]
    fn quick_list_filter_truth_table() {
        let hm = |h: u32, m: u32| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let morning = hm(9, 30);
        let evening = hm(19, 30);
        assert!(quick_list_matches("", morning), "empty matches all");
        assert!(quick_list_matches("9", morning), "de-zeroed prefix");
        assert!(
            !quick_list_matches("9", evening),
            "19:xx carries no leading zero"
        );
        assert!(quick_list_matches("930", morning));
        assert!(quick_list_matches("09", morning), "canonical leading zero");
        assert!(quick_list_matches("19", evening));
        assert!(!quick_list_matches("9", hm(8, 0)));
        assert!(!quick_list_matches("93", evening), "19:30 is not 0930");
        assert_eq!(filter_digits(" 9:45 "), "945");
        assert_eq!(filter_digits("abc"), "");
        assert!(quick_list_matches(&filter_digits(" 9:3"), morning));
    }

    #[test]
    fn sync_directions_follow_the_pinned_rules() {
        let picker = DuePickerState::open(NaiveDate::from_ymd_opt(2030, 1, 31).unwrap(), None);
        // Picker source: date-only renders date-only.
        assert_eq!(
            sync_buffer_picker("", &picker, SyncSource::Picker, now()),
            Some(("2030-01-31".to_owned(), None))
        );
        // An explicit time renders `date HH:MM`.
        let mut timed = picker.clone();
        timed.set_time(Some(NaiveTime::from_hms_opt(14, 30, 0).unwrap()));
        assert_eq!(
            sync_buffer_picker("", &timed, SyncSource::Picker, now()),
            Some(("2030-01-31 14:30".to_owned(), None))
        );
        // An explicit midnight *stays* in the rendering (the
        // two-midnights convention).
        timed.set_time(Some(NaiveTime::MIN));
        assert_eq!(
            sync_buffer_picker("", &timed, SyncSource::Picker, now()),
            Some(("2030-01-31 00:00".to_owned(), None))
        );
        // Buffer source: parseable text re-seeds the picker; the
        // explicit `00:00` counts — a typed time exists.
        let Some((_, Some(seeded))) =
            sync_buffer_picker("2030-02-14 00:00", &picker, SyncSource::Buffer, now())
        else {
            panic!("parseable buffer seeds");
        };
        assert_eq!(
            seeded.cursor(),
            NaiveDate::from_ymd_opt(2030, 2, 14).unwrap()
        );
        assert_eq!(seeded.time(), Some(NaiveTime::MIN));
        // Date-only text: the picker stays date-only.
        let Some((_, Some(seeded))) =
            sync_buffer_picker("2030-02-14", &picker, SyncSource::Buffer, now())
        else {
            panic!("date-only seeds");
        };
        assert_eq!(seeded.time(), None);
        // Empty and junk leave the pair untouched.
        assert_eq!(
            sync_buffer_picker("", &picker, SyncSource::Buffer, now()),
            None
        );
        assert_eq!(
            sync_buffer_picker("soon", &picker, SyncSource::Buffer, now()),
            None
        );
        assert_eq!(
            sync_buffer_picker("2030-01-", &picker, SyncSource::Buffer, now()),
            None,
            "a partial prefix is junk"
        );
        // Recorded: chrono's strptime accepts non-padded fields, so
        // `2030-01-3` parses (as day 3) — pre-existing grammar
        // leniency, not the sync's concern; the seed then matches
        // what a Return would commit.
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
            // The combobox's parse is total too (r4 D26): never
            // panics, and a hit is always a real in-range time.
            if let Some(time) = parse_time_lenient(&text) {
                prop_assert!(time.hour() <= 23 && time.minute() <= 59);
                prop_assert!(text.trim() != "9:5" && text.trim() != "2450");
            }
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

        /// The grid always reserves six week-rows (r4 D22), blanks
        /// lead, and every non-blank cell stays inside the displayed
        /// month.
        #[test]
        fn grid_invariants(days in 0i64..=4000) {
            let date = (now() + ChronoDuration::try_days(days).unwrap()).date_naive();
            let picker = DuePickerState::open(date, None);
            let grid = picker.grid();
            prop_assert_eq!(grid.len(), 42);
            let leading = grid.iter().take_while(|c| c.date.is_none()).count();
            let trailing = grid.iter().rev().take_while(|c| c.date.is_none()).count();
            prop_assert!(leading < 7);
            prop_assert!(trailing >= 42 - 7 - 31, "the reserve slack is trailing");
            let days_cells = &grid[leading..grid.len() - trailing];
            for cell in days_cells {
                let cell_date = cell.date.expect("the run between blanks is all days");
                prop_assert_eq!(cell_date.month(), picker.month().month());
                prop_assert_eq!(cell_date.year(), picker.month().year());
            }
        }

        /// The sync helper's no-oscillation property (r4 D25, review
        /// focus c): one-step idempotence for both sources, and the
        /// round trip converges — a `Picker` step after a `Buffer`
        /// step re-renders exactly the buffer the `Buffer` step
        /// produced (the formatting is parse's inverse modulo the
        /// two-midnights convention, exercised explicitly).
        fn the_sync_never_oscillates(
            days in -3650i64..=3650,
            hour in 0u32..24,
            minute in (0u32..4).prop_map(|q| q * 15),
            explicit_midnight in proptest::bool::ANY,
        ) {
            let now = now();
            let date = (now + ChronoDuration::try_days(days).unwrap()).date_naive();
            let time = if explicit_midnight {
                Some(NaiveTime::MIN)
            } else {
                NaiveTime::from_hms_opt(hour, minute, 0)
            };
            let picker = DuePickerState::from_parts(date, time);

            // One-step idempotence, Picker source.
            let Some((buffer, None)) =
                sync_buffer_picker("", &picker, SyncSource::Picker, now)
            else {
                panic!("picker source never touches the picker");
            };
            let second = sync_buffer_picker(&buffer, &picker, SyncSource::Picker, now)
                .expect("picker renders");
            prop_assert_eq!(second, (buffer.clone(), None));

            // One-step idempotence, Buffer source.
            let Some((_, Some(seeded))) = sync_buffer_picker(&buffer, &picker, SyncSource::Buffer, now)
            else {
                panic!("the canonical buffer carries a picker");
            };
            let again = sync_buffer_picker(&buffer, &seeded, SyncSource::Buffer, now);
            prop_assert_eq!(
                again.map(|(_, p)| p),
                Some(Some(seeded.clone())),
                "the buffer step is a fixed point"
            );

            // The round trip: Picker-after-Buffer re-renders the
            // Buffer step's buffer.
            let Some((_, None)) =
                sync_buffer_picker(&buffer, &seeded, SyncSource::Picker, now)
            else {
                panic!("picker renders");
            };
            let round = sync_buffer_picker(&buffer, &seeded, SyncSource::Buffer, now)
                .expect("round trips parse");
            prop_assert_eq!(round.0, buffer);
        }

        /// Arbitrary junk buffers never make the sync panic (totality
        /// rides with the parser's own).
        fn the_sync_is_total_on_arbitrary_buffers(text in ".*") {
            let picker = DuePickerState::open(now().date_naive(), None);
            for source in [SyncSource::Picker, SyncSource::Buffer] {
                let _ = sync_buffer_picker(&text, &picker, source, now());
            }
        }
    }
}
