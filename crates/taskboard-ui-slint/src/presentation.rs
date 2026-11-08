// SPDX-License-Identifier: AGPL-3.0-only

//! The presentation helpers (design §4.3): pure display-domain
//! mapping over domain types — no slint types, nothing generated, no
//! IO, no mutation (the extraction seam: a second Rust consumer lifts
//! this module into its own crate unchanged; a non-Rust UI reuses the
//! *domain* over IPC and re-implements the mapping natively — the
//! concepts port, the module does not).
//!
//! Domain truth is never reinvented: due classification is
//! set-membership over the domain's own `overdue_tasks`/`due_soon`
//! outputs, and stack display order reads the domain's persisted
//! `Stack.order` field (the domain defines the concept and the write
//! commands; only the reading helper lives here — D10).

use std::collections::BTreeSet;
use std::fmt::Write as _;

use chrono::{
    DateTime, Datelike as _, NaiveDate, NaiveDateTime, NaiveTime, TimeDelta, TimeZone, Utc,
};
use taskboard_domain::{
    AdhocAlarmId, AppState, Board, BoardId, Effort, ReminderRef, SidecarHealth, Stack, StackId,
    SyncPhase, Task, TaskId, TimeEntryId, TimeEntryKind, focus::FocusTiming,
    focus::focus_break_is_long, tracking::TimeEntry,
};

/// The due-soon window (design §4): a presentation constant, not
/// config — Q22 fixes the v0.1.0 `[ui]` surface at scale + keymap
/// (the tunable horizon is recorded backlog debt).
pub const DUE_SOON_HORIZON: TimeDelta = TimeDelta::hours(24);

/// The OS-delivery health snapshot (round-3 D31, chunk 7): the
/// delivery seam's observed attempt/outcome tally, app-owned and
/// pushed to the UI over a `tokio::sync::watch` — defined here
/// (presentation-side) so the app actor and the dumb UI share one
/// ui-safe shape with no app→domain detour. Wall-clock telemetry,
/// never engine state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeliveryHealth {
    pub last_attempt: Option<DateTime<Utc>>,
    pub last_success: Option<DateTime<Utc>>,
    pub last_failure: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
}

impl DeliveryHealth {
    /// The chip's visibility rule: degraded iff the last attempt
    /// failed (a later success clears it — litmus-safe: strictly
    /// better delivery can only clear the surface).
    #[must_use]
    pub fn degraded(&self) -> bool {
        matches!(self.last_failure, Some(f) if self.last_success.is_none_or(|s| f > s))
    }
}

/// The degraded chip's sentence (never asserted in tests — only the
/// visibility edge is): states the failure and names the in-app
/// surface as the fallback, the same honest ceiling as the boot
/// diagnosis.
pub const DELIVERY_CHIP_TEXT: &str = "desktop toast delivery failing — reminders surface in-app";

/// The delivery chip's pure mapping (the `settings_status` totality
/// discipline): `Some` (visible) iff the health is degraded, `None`
/// (absent) while healthy — and never timer-cleared: the chip clears
/// when the next attempt succeeds, a state of the world, not a
/// message.
#[must_use]
pub fn delivery_chip(health: &DeliveryHealth) -> Option<String> {
    health.degraded().then(|| DELIVERY_CHIP_TEXT.to_owned())
}

/// The files-sync chip's state (DR-8 chunk 4, D-FH5): the plain
/// ui-safe payload the app-side pump produces from the transport
/// crate's typed findings — the UI never names the IO crate. No ages
/// on the chip (the derivation stays clock-free and the UI test
/// surface deterministic; the CLI line is the "N min ago" consumer).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilesSyncChip {
    pub state: FilesSyncChipState,
}

/// The chip's precedence ladder rung (D-FH5): Forked > Diverged >
/// Parked > `CycleFailed` > `Ok`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FilesSyncChipState {
    #[default]
    Ok,
    CycleFailed,
    Parked,
    Diverged,
    Forked,
}

/// The chip's caption (presentation text, never asserted — only the
/// visibility and state edges are).
#[must_use]
pub fn files_sync_chip_caption(state: FilesSyncChipState) -> &'static str {
    match state {
        FilesSyncChipState::Ok => "files-sync running",
        FilesSyncChipState::CycleFailed => "files-sync cycle failing",
        FilesSyncChipState::Parked => "files-sync: peer data held aside",
        FilesSyncChipState::Diverged => "files-sync: devices diverged",
        FilesSyncChipState::Forked => "files-sync forked — writes stopped",
    }
}

/// The due classification vocabulary (slint-free; `models` maps it
/// 1:1 onto the generated enum for markup conditionals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DueKind {
    None,
    Done,
    Later,
    Soon,
    Overdue,
}

/// The domain classification outputs, computed once per refresh and
/// keyed for membership asks (no predicate duplication anywhere).
#[derive(Debug, Clone, Default)]
pub struct DueSets {
    pub overdue: BTreeSet<TaskId>,
    pub soon: BTreeSet<TaskId>,
}

/// Computes [`DueSets`] for `now` by calling the domain functions
/// collection-side — the overdue/due-soon semantics exist only there.
/// The horizon is the handed-over typed value (round-3 D14/#230: the
/// `[ui] due_soon_days` knob; the const stays the default).
#[must_use]
pub fn due_sets(state: &AppState, now: DateTime<Utc>, horizon: TimeDelta) -> DueSets {
    DueSets {
        overdue: task_id_keys(taskboard_domain::overdue_tasks(&state.tasks, now)),
        soon: task_id_keys(taskboard_domain::due_soon(&state.tasks, now, horizon)),
    }
}

fn task_id_keys<'a>(tasks: impl IntoIterator<Item = &'a Task>) -> BTreeSet<TaskId> {
    tasks.into_iter().map(|t| t.id).collect()
}

/// Classifies one task against the sets: done → `Done`; no duedate →
/// `None`; then set membership — no reimplementation of the domain
/// predicates.
#[must_use]
pub fn due_kind(task: &Task, sets: &DueSets) -> DueKind {
    if task.done.is_some() {
        DueKind::Done
    } else if task.duedate.is_none() {
        DueKind::None
    } else if sets.overdue.contains(&task.id) {
        DueKind::Overdue
    } else if sets.soon.contains(&task.id) {
        DueKind::Soon
    } else {
        DueKind::Later
    }
}

/// The typed numeric bucketing behind due phrases (string assembly is
/// trivial glue over the bucket — tests assert buckets, not text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationBucket {
    Now,
    Minutes(u32),
    Hours(u32),
    Days(u32),
}

/// Buckets a (non-negative) duration. Negative inputs clamp to
/// [`DurationBucket::Now`]; extreme magnitudes saturate their unit's
/// `u32` — total on arbitrary input.
#[must_use]
pub fn duration_bucket(delta: TimeDelta) -> DurationBucket {
    let seconds = delta.num_seconds().max(0);
    if seconds < 60 {
        return DurationBucket::Now;
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return DurationBucket::Minutes(u32::try_from(minutes).unwrap_or(u32::MAX));
    }
    let hours = minutes / 60;
    if hours < 24 {
        return DurationBucket::Hours(u32::try_from(hours).unwrap_or(u32::MAX));
    }
    DurationBucket::Days(u32::try_from(hours / 24).unwrap_or(u32::MAX))
}

/// The direction of a scheduled start relative to now — the only place
/// the sign comparison for `scheduled_phrase` lives (issue #458: the
/// direction is a typed value, the wording consumes it). Deliberate
/// leniency at the mint: `of(TimeDelta::zero())` yields `Future`, but
/// zero never reaches the mint — the `Now` bucket arm renders first
/// and never consults the direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Direction {
    Future,
    Past,
}

impl Direction {
    /// Total: the sign of the delta; zero's `Future` is unreachable
    /// past the `Now` arm (see the type doc).
    fn of(delta: TimeDelta) -> Self {
        if delta < TimeDelta::zero() {
            Self::Past
        } else {
            Self::Future
        }
    }
}

/// The ` · HH:MM` time suffix (round 3 D9/F15, r4 D28): empty for an
/// absent or midnight instant — a midnight instant renders exactly
/// the date-only shape it always did — else the **local** wall-clock
/// time over the injected zone (the display tells the truth about
/// when the task is due; a UTC 23:30 renders `01:30` next-day in a
/// +02:00 zone). One place: both date phrases compose it; the
/// tint/direction language is untouched.
#[must_use]
pub fn at_time_suffix(instant: Option<DateTime<Utc>>, tz: &impl chrono::TimeZone) -> String {
    match instant {
        Some(instant) => {
            let local = tz.from_utc_datetime(&instant.naive_utc());
            if local.time() == NaiveTime::MIN {
                String::new()
            } else {
                format!(" \u{b7} {}", local.time().format("%H:%M"))
            }
        }
        None => String::new(),
    }
}

/// The display phrase for a due classification (mono role), assembled
/// from the bucket with the direction taken from the sign of
/// `duedate - now`.
#[must_use]
pub fn due_phrase(
    kind: DueKind,
    duedate: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> String {
    let Some(duedate) = duedate else {
        return String::new();
    };
    let suffix = at_time_suffix(Some(duedate), tz);
    let delta = duedate - now;
    let bucket = duration_bucket(delta.abs());
    let quantity = match bucket {
        DurationBucket::Now => return format!("now{suffix}"),
        DurationBucket::Minutes(n) => format!("{n}m"),
        DurationBucket::Hours(n) => format!("{n}h"),
        DurationBucket::Days(n) => format!("{n}d"),
    };
    let word = match kind {
        DueKind::Overdue | DueKind::Done => "overdue",
        _ => "due",
    };
    if delta < TimeDelta::zero() {
        format!("{quantity} {word}{suffix}")
    } else {
        format!("in {quantity}{suffix}")
    }
}

/// The display phrase for a scheduled start (mono role): empty when the
/// field is absent (the `due_phrase` convention); otherwise
/// self-describing — unlike due, no tone carries the direction, so the
/// words do (`starts now` / `starts in {q}` / `started {q} ago`). The
/// bucketing edges are `duration_bucket`'s tested contract.
#[must_use]
pub fn scheduled_phrase(
    scheduled: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> String {
    let Some(scheduled) = scheduled else {
        return String::new();
    };
    let suffix = at_time_suffix(Some(scheduled), tz);
    let delta = scheduled - now;
    let bucket = duration_bucket(delta.abs());
    // The `Now` arm never consults the direction (the zero delta never
    // reaches `Direction::of`'s comparison semantically — its unused
    // `Future` value is discarded with the arm's wildcard).
    let direction = Direction::of(delta);
    match (direction, bucket) {
        (_, DurationBucket::Now) => format!("starts now{suffix}"),
        (Direction::Future, DurationBucket::Minutes(n)) => format!("starts in {n}m{suffix}"),
        (Direction::Future, DurationBucket::Hours(n)) => format!("starts in {n}h{suffix}"),
        (Direction::Future, DurationBucket::Days(n)) => format!("starts in {n}d{suffix}"),
        (Direction::Past, DurationBucket::Minutes(n)) => format!("started {n}m ago{suffix}"),
        (Direction::Past, DurationBucket::Hours(n)) => format!("started {n}h ago{suffix}"),
        (Direction::Past, DurationBucket::Days(n)) => format!("started {n}d ago{suffix}"),
    }
}

/// The display phrase for an estimated effort: empty when absent, else
/// the domain's canonical text form — the UI wraps the domain formatter,
/// never re-implements the grammar.
#[must_use]
pub fn effort_phrase(effort: Option<Effort>) -> String {
    effort.map_or(String::new(), Effort::text_form)
}

/// The placed box's time-of-day phrase (#622, D-8): the scheduled
/// instant's fixed-offset local reading, `HH:mm`. Pure — the instant
/// and the calendar are data.
#[must_use]
pub fn plan_box_time_phrase(
    instant: Option<chrono::DateTime<Utc>>,
    calendar: taskboard_domain::DayCalendar,
) -> String {
    instant.map_or(String::new(), |instant| {
        instant
            .with_timezone(&calendar.offset)
            .format("%H:%M")
            .to_string()
    })
}

/// The anchor-card test (DDF-5, the presentation-side half): an exact
/// byte-equal title match — no trim, no case-fold, no normalization (a
/// trailing space is a visible near-miss, and must stay visible).
/// Never enters the domain crate; resolution keeps resolving the anchor.
#[must_use]
pub fn is_anchor_task(task: &Task) -> bool {
    task.title == taskboard_domain::ANCHOR_CARD_TITLE
}

/// The sync phase as the UI badge vocabulary (slint-free).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhaseUi {
    Idle,
    Syncing,
    Offline,
    Failed,
}

/// The sync badge model (D11): phase + pending depth; recency text is
/// markup-side trivia over `last_success`, not part of the badge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncBadge {
    pub phase: SyncPhaseUi,
    pub pending: u32,
    /// The selected board's derived sidecar health (the loud fence's
    /// subject); `Healthy` (the default) renders nothing.
    pub sidecar: SidecarHealth,
}

/// Maps the *selected* board's sync row onto the badge (design §3.2,
/// option (a)): `Some(board)` reads that board's row — the derivation
/// materializes a default row per known board, so a bound-but-never-
/// synced board reads `Idle`/0; `None` (no boards at all) reads the
/// default. The aggregate fold is not a UI display read (the app-side
/// daemon `status` keeps it).
#[must_use]
pub fn sync_badge_for_board(
    sync: &taskboard_domain::SyncStates,
    board: Option<BoardId>,
) -> SyncBadge {
    let status = board.and_then(|b| sync.get(b)).cloned().unwrap_or_default();
    let phase = match status.phase {
        SyncPhase::Idle => SyncPhaseUi::Idle,
        SyncPhase::Syncing => SyncPhaseUi::Syncing,
        SyncPhase::Offline => SyncPhaseUi::Offline,
        SyncPhase::Failed { .. } => SyncPhaseUi::Failed,
    };
    SyncBadge {
        phase,
        pending: status.pending_ops,
        sidecar: status.sidecar,
    }
}

/// The sidecar warning caption (D-S2's loud fence): empty for healthy
/// (absent evidence is the ruled silence); an unreadable reason renders
/// the "update this app" warning class with the typed reason tag. The
/// wording is presentation-edge text — the `status` CLI's precedent.
#[must_use]
pub fn sidecar_caption(health: SidecarHealth) -> String {
    match health {
        SidecarHealth::Healthy => String::new(),
        SidecarHealth::Unreadable(taskboard_domain::AppDataUnreadable::FormatTooNew { format }) => {
            format!("board data unreadable (format {format} is too new) — update this app")
        }
        SidecarHealth::Unreadable(reason) => {
            let tag = match reason {
                taskboard_domain::AppDataUnreadable::Transport => "transport",
                taskboard_domain::AppDataUnreadable::AccessDenied => "access-denied",
                taskboard_domain::AppDataUnreadable::ServerFault => "server-fault",
                taskboard_domain::AppDataUnreadable::Decode => "decode",
                taskboard_domain::AppDataUnreadable::ForeignBoard => "foreign-board",
                taskboard_domain::AppDataUnreadable::FormatTooNew { .. } => unreachable!(),
            };
            format!("board data unreadable ({tag}) — update this app")
        }
    }
}

/// A banner snooze/mute target (D4/D12): the task plus the concrete
/// `ReminderRef`/occurrence pair the commands carry. For a task with
/// explicit rules this is the first rule's `occurrence_of` output;
/// rule-less tasks use the implicit default with the duedate anchor
/// (occurrence is opaque link data by engine contract — the *nag*
/// effect of these links lands with the reminders toast plan).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReminderTarget {
    pub anchor: taskboard_domain::ReminderAnchor,
    pub reminder: ReminderRef,
    pub occurrence: DateTime<Utc>,
}

/// The rule-list add affordance's preset offsets (chunk 6, D12):
/// presentation const data — the labels are derived through
/// `ReminderOffset::text_form`, never hand-written strings.
pub const RULE_PRESETS: &[chrono::TimeDelta] = &[
    chrono::TimeDelta::try_minutes(5).expect("5m fits"),
    chrono::TimeDelta::try_minutes(30).expect("30m fits"),
    chrono::TimeDelta::try_hours(1).expect("1h fits"),
    chrono::TimeDelta::try_hours(24).expect("24h fits"),
];

/// The preset buttons' labels, derived from [`RULE_PRESETS`] through the
/// domain's canonical text form (`"5m"`, `"30m"`, `"1h"`, `"24h"`).
///
/// # Panics
///
/// Only via `expect` on the offset construction: the preset magnitudes
/// are const negative durations by construction.
#[must_use]
pub fn rule_preset_labels() -> Vec<String> {
    RULE_PRESETS
        .iter()
        .map(|dur| {
            taskboard_domain::ReminderOffset::before(-*dur)
                .expect("preset magnitudes negate into the offset domain")
                .text_form()
        })
        .collect()
}

/// The rule-list's derived-fact hint (chunk 6, D12): while any explicit
/// rule exists, the implicit default never fires (`due_firings`'s
/// rules-replace-default derivation). Empty for a rule-less task. The
/// muted variant (r4 D31a — F25's other ranked cause: mute silences
/// every rule, and the section says so instead of looking stale).
#[must_use]
pub fn rule_list_hint(rule_count: usize, muted: bool) -> String {
    if rule_count == 0 {
        String::new()
    } else if muted {
        "Reminders on this task are muted: none of these rules fire. Un-mute below.".to_owned()
    } else {
        "Explicit rules replace the task's default reminder: only these rules fire.".to_owned()
    }
}

/// One countdown's d/h/m/s decomposition (r4 D30): the structural
/// remaining-time shape shared by rule rows and chunk 9's timer rows —
/// segments, never label text (the no-text-assertions rule; tests pin
/// segment counts and round-trips by re-summing the seconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CountdownParts {
    /// Whole days remaining.
    pub days: u64,
    /// Hours remaining after the days.
    pub hours: u64,
    /// Minutes remaining after the hours.
    pub minutes: u64,
    /// Seconds remaining after the minutes.
    pub seconds: u64,
}

impl CountdownParts {
    /// The total remaining seconds — the structural round-trip.
    #[must_use]
    pub fn total_seconds(self) -> i64 {
        (self.days * 86_400 + self.hours * 3_600 + self.minutes * 60 + self.seconds)
            .try_into()
            .unwrap_or(i64::MAX)
    }
}

/// Decomposes `effective − now` into d/h/m/s, clamping at zero (the
/// `countdown_phrase` clamp precedent: a fired-undelivered transient is
/// total, never a panic or a negative).
#[must_use]
pub fn countdown_parts(effective: DateTime<Utc>, now: DateTime<Utc>) -> CountdownParts {
    let remaining = (effective - now).num_seconds().max(0);
    let days = u64::try_from(remaining / 86_400).unwrap_or_default();
    let hours = u64::try_from((remaining % 86_400) / 3_600).unwrap_or_default();
    let minutes = u64::try_from((remaining % 3_600) / 60).unwrap_or_default();
    let seconds = u64::try_from(remaining % 60).unwrap_or_default();
    CountdownParts {
        days,
        hours,
        minutes,
        seconds,
    }
}

/// The countdown's fixed text shape: `in {d}d {h}h {m}m {s}s` — zero
/// segments kept (the F23 spec shows the full d/h/m/s ladder). The
/// renderer of [`countdown_parts`]; structure is pinned via the parts.
#[must_use]
pub fn countdown_text(effective: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let parts = countdown_parts(effective, now);
    format!(
        "in {}d {}h {}m {}s",
        parts.days, parts.hours, parts.minutes, parts.seconds
    )
}

/// The fire instant's local, ISO-ordered `YYYY-MM-DD HH:MM:SS` form
/// (the F23 spec: colon-separated per ISO 8601) — the shared fixed-time
/// renderer for rule rows and chunk 9's timer rows. `tz` is the
/// injected seam (the parse-side twin).
#[must_use]
pub fn iso_local_fire_time(effective: DateTime<Utc>, tz: &impl chrono::TimeZone) -> String {
    let local = tz.from_utc_datetime(&effective.naive_utc());
    // The naive-local datetime formats without an Offset bound (the
    // `at_time_suffix` precedent).
    local.naive_local().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// The mouse setter's quick-preset durations (r4 D33, F13): the pinned
/// 1m/5m/10m/25m/1h ladder, in minutes — the setter's preset order is
/// the array order.
pub const TIMER_PRESETS_MIN: [u32; 5] = [1, 5, 10, 25, 60];

/// A preset's grammar text (r4 D-T4): `"in {m}m"`, hours form at 60 —
/// both re-parsed by the one timer grammar verbatim.
#[must_use]
pub fn preset_when_text(minutes: u32) -> String {
    if minutes == 60 {
        "in 1h".to_owned()
    } else {
        format!("in {minutes}m")
    }
}

/// The custom row's composed fire instant (r4 D-T4): `(today + offset)
/// .and_time(H:M:0)` resolved host-local through `tz` with the
/// `.single()` fold — a DST-gap pick is `None`, never a guessed
/// instant. Pure; the setter's commit composes grammar text from it.
#[must_use]
pub fn compose_timer_instant(
    minutes_of_day: u32,
    day_offset_days: u32,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<DateTime<Utc>> {
    let today = now.with_timezone(tz).date_naive();
    let date = today + chrono::TimeDelta::try_days(i64::from(day_offset_days))?;
    let time = chrono::NaiveTime::from_hms_opt(minutes_of_day / 60, minutes_of_day % 60, 0)?;
    tz.from_local_datetime(&date.and_time(time))
        .single()
        .map(|local| local.with_timezone(&Utc))
}

/// The custom row's composed grammar text (r4 D-T4): `at <RFC-3339
/// with the local offset>` — re-parsed by [`parse_timer_when`]'s
/// RFC-3339 arm back to the exact instant (the one-parser contract;
/// no roll — an explicit day choice is honored).
#[must_use]
pub fn compose_timer_at(
    minutes_of_day: u32,
    day_offset_days: u32,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<String> {
    compose_timer_instant(minutes_of_day, day_offset_days, now, tz)
        .map(|instant| format!("at {}", instant.with_timezone(tz).to_rfc3339()))
}

/// One rule row's presentation texts (r4 D30): the kind-specific
/// primary line plus the secondary detail line, and the derived armed
/// bit (`task.duedate.is_none() && kind.is_due_offset()` at push time —
/// derived in the projection, never stored, so no state desync is
/// possible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleRowTexts {
    /// The primary line: the authored offset text, the live countdown,
    /// or the explicit needs-a-due-date copy.
    pub primary: String,
    /// The secondary line: the effective fire instant (armed offset
    /// rows) or the fixed local fire time (At rows); empty otherwise.
    pub detail: String,
    /// The needs-a-due-date state: an unarmed `DueOffset` row presents
    /// the jump affordance.
    pub armed: bool,
}

/// The needs-a-due-date row copy (r4 D30 — F25's communication half:
/// the cause of "never fires" made visible).
pub const RULE_ROW_UNARMED: &str = "needs a due date — not armed";

/// The projection for one rule row (r4 D30): see [`RuleRowTexts`].
#[must_use]
pub fn rule_row_texts(
    rule: &taskboard_domain::ReminderRule,
    duedate: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> RuleRowTexts {
    match rule.kind {
        taskboard_domain::ReminderKind::At(at) => {
            let effective = at.as_datetime();
            RuleRowTexts {
                primary: countdown_text(effective, now),
                detail: format!("@ {}", iso_local_fire_time(effective, tz)),
                armed: true,
            }
        }
        taskboard_domain::ReminderKind::DueOffset(offset) => {
            match duedate.map(|d| d.checked_add_signed(offset.as_duration())) {
                Some(Some(effective)) => RuleRowTexts {
                    primary: offset.text_form(),
                    detail: format!("fires {}", iso_local_fire_time(effective, tz)),
                    armed: true,
                },
                // Unarmed: no due date (or degenerate arithmetic) — the
                // explicit state, not silently stale.
                _ => RuleRowTexts {
                    primary: RULE_ROW_UNARMED.to_owned(),
                    detail: String::new(),
                    armed: false,
                },
            }
        }
    }
}

/// The rule row's created stamp (round-3 D13/F29): the UTC calendar
/// date, short form — created is bookkeeping metadata; the rfc3339
/// `T`/offset noise was the finding, and date precision sits below
/// the at-most-second ceiling. `now` is unused today (the short form
/// needs no anchor) but keeps the phrase helpers' shape.
#[must_use]
pub fn rule_created_phrase(created_at: DateTime<Utc>, _now: DateTime<Utc>) -> String {
    created_at.format("%Y-%m-%d").to_string()
}

/// One overdue-banner row (D12): the task truth plus its snooze
/// target. Visibility never consults mute/snooze/deliveries — board
/// truth, not nagging policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BannerRow {
    pub task: TaskId,
    pub kind: DueKind,
    pub target: ReminderTarget,
}

/// The banner rows for `now`: `overdue_tasks` order (canonical), one
/// row per task, each with a well-formed target (D12).
#[must_use]
pub fn banner_rows(state: &AppState, now: DateTime<Utc>) -> Vec<BannerRow> {
    taskboard_domain::overdue_tasks(&state.tasks, now)
        .into_iter()
        // The anchor card is never a banner row (DDF-5: the exact-title
        // filter is presentation-layer; `banner_overflow` follows).
        .filter(|task| !is_anchor_task(task))
        .map(|task| {
            let (reminder, occurrence) = snooze_target(state, task);
            BannerRow {
                task: task.id,
                kind: DueKind::Overdue,
                target: ReminderTarget {
                    anchor: taskboard_domain::ReminderAnchor::Task(task.id),
                    reminder,
                    occurrence,
                },
            }
        })
        .collect()
}

/// The snooze target for one overdue task (D12): first explicit rule
/// (rule-id order, the map's canonical order) with `occurrence_of`;
/// rule-less (or a rule whose occurrence arithmetic is degenerate) →
/// `ReminderRef::Default(task)` with the duedate anchor.
fn snooze_target(state: &AppState, task: &Task) -> (ReminderRef, DateTime<Utc>) {
    let rule = state.reminders.rules.values().find(|r| r.task == task.id);
    let occurrence = rule.and_then(|r| taskboard_domain::occurrence_of(r, task.duedate));
    if let (Some(rule), Some(occurrence)) = (rule, occurrence) {
        return (ReminderRef::Rule(rule.id), occurrence);
    }
    let anchor = task
        .duedate
        .unwrap_or_else(taskboard_domain::BoardClocks::epoch);
    (ReminderRef::Default(task.id), anchor)
}

/// One in-app notification row (R3 chunk C, design D-10): the fired
/// occurrence's command targets its buttons dispatch. `task` is `None`
/// for an alarm row (no Done/Mute button exists for an anchor-less
/// alarm; Dismiss and Snooze carry the ref + occurrence themselves).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRow {
    /// The watched task (the Done/Mute buttons' target); `None` marks
    /// an adhoc-alarm row.
    pub task: Option<TaskId>,
    /// The reminder ref both the dismiss and snooze commands carry.
    pub reminder: ReminderRef,
    /// The effective occurrence that fired.
    pub occurrence: DateTime<Utc>,
    /// The title snapshot the domain view assembled (task title or
    /// alarm label) — carried, never re-derived.
    pub title: String,
}

/// The in-app notification rows for `now` (design D-6): the domain's
/// own `active_notifications` — set-membership over fired-∧-not-
/// dismissed, never reinvented here — in its canonical
/// `(occurrence, reminder)` order. The anchor card is filtered (the
/// banner's DDF-5 precedent: the anchor is never a reminder surface).
/// Desktop-only surface: the kiosk's mount gate sits model-side.
#[must_use]
pub fn notification_rows(
    state: &AppState,
    defaults: &taskboard_domain::ReminderDefaults,
    day: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
) -> Vec<NotificationRow> {
    taskboard_domain::active_notifications(
        &state.tasks,
        &state.tracking,
        &state.habits,
        day,
        &state.reminders,
        defaults,
        now,
    )
    .into_iter()
    .filter(|note| match note.anchor {
        taskboard_domain::ReminderAnchor::Task(task) => {
            state.tasks.get(&task).is_none_or(|t| !is_anchor_task(t))
        }
        // A break row has no card to filter (D-18): always shown, like
        // an alarm row — and a tracker nudge row too (no card exists).
        taskboard_domain::ReminderAnchor::Alarm(_)
        | taskboard_domain::ReminderAnchor::Break(_)
        | taskboard_domain::ReminderAnchor::Tracker(_) => true,
    })
    .map(|note| NotificationRow {
        task: match note.anchor {
            taskboard_domain::ReminderAnchor::Task(task) => Some(task),
            // No Done/Mute target; Dismiss/Snooze carry the ref and
            // occurrence themselves (the alarm-row precedent).
            taskboard_domain::ReminderAnchor::Alarm(_)
            | taskboard_domain::ReminderAnchor::Break(_)
            | taskboard_domain::ReminderAnchor::Tracker(_) => None,
        },
        reminder: note.reminder,
        occurrence: note.occurrence,
        title: note.title,
    })
    .collect()
}

/// The notification row's context phrase (mono role): elapsed since the
/// fired occurrence, bucketed by [`duration_bucket`] — the due phrase's
/// tested contract, past-direction ("fired now" / "fired {q} ago").
#[must_use]
pub fn notification_phrase(occurrence: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let bucket = duration_bucket(now - occurrence);
    match bucket {
        DurationBucket::Now => "fired now".to_owned(),
        DurationBucket::Minutes(n) => format!("fired {n}m ago"),
        DurationBucket::Hours(n) => format!("fired {n}h ago"),
        DurationBucket::Days(n) => format!("fired {n}d ago"),
    }
}

/// The status-bar notice's expiry horizon (#130): a presentation
/// constant — the resurrect line is transient, never blocking (the
/// kiosk/daemon rule); expiry rides the cadence tick's wall clock.
pub const RESURRECT_NOTICE_HORIZON: chrono::TimeDelta =
    chrono::TimeDelta::try_seconds(10).expect("non-zero constant");

/// The notice's visibility decision (#130): visible while the expiry
/// instant is still ahead of `now` — the pure half of the cadence tick's
/// clear.
#[must_use]
pub fn notice_visible(now: DateTime<Utc>, until: Option<DateTime<Utc>>) -> bool {
    until.is_some_and(|until| until > now)
}

/// The resurrect notice's passive status line (#130). Pure; asserted
/// non-empty and deterministic — never asserted on its copy (AGENTS §5).
#[must_use]
pub fn resurrect_notice_phrase(title: &str) -> String {
    format!("\"{title}\" was deleted remotely while it had unsynced work \u{2014} re-created")
}

/// The tracking chip's parts (DR-5/4, D-T17): the newest-open entry's
/// task, its title SNAPSHOT (survives rename and deletion — the D-T2
/// history ruling), and its start. `now` is data; nothing ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackingChip {
    pub task: TaskId,
    pub title: String,
    pub start: DateTime<Utc>,
}

/// The status chip's pure decision (DR-5/4, D-T16/D-T17): `None`
/// (hidden) while no open entry exists; else the D-T8 reader pick over
/// the domain's `newest_open` — chip and badge share this one
/// derivation so they can never disagree under the transient
/// double-open ruling.
#[must_use]
pub fn tracking_chip(state: &AppState) -> Option<TrackingChip> {
    state.tracking.newest_open().map(|entry| TrackingChip {
        task: entry.task,
        title: entry.task_title.clone(),
        start: entry.start,
    })
}

/// The shared open-entry task (the card badge's visibility input):
/// one derivation with [`tracking_chip`].
#[must_use]
pub fn tracking_open_task(state: &AppState) -> Option<TaskId> {
    tracking_chip(state).map(|chip| chip.task)
}

/// The open entry's elapsed phrase (D-T17): minute buckets, past
/// direction, over [`duration_bucket`] — `0m` for the sub-minute
/// `Now` bucket (an honest zero, not a countdown-style "now").
#[must_use]
pub fn open_elapsed_phrase(start: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match duration_bucket(now - start) {
        DurationBucket::Now => "0m".to_owned(),
        DurationBucket::Minutes(n) => format!("{n}m"),
        DurationBucket::Hours(n) => format!("{n}h"),
        DurationBucket::Days(n) => format!("{n}d"),
    }
}

// -- the concentration overlay (#496): pure derivations ---------------

/// The switcher row: the task and its live title. The id is the
/// dispatch datum (`track_target`); the title is the card's live
/// title (the switcher targets *now*, unlike the chip's snapshot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusSwitcherRow {
    pub task: TaskId,
    pub title: String,
}

/// The switcher pool's bound (#496, D-F10): nine rows — the palette's
/// row-cap idiom; a focus target is picked, not scrolled.
pub const FOCUS_SWITCHER_CAP: usize = 9;

/// The overlay's switcher pool: live, not-done tasks in board reading
/// order (board-id ascending → stack `order`/id → task `order`/id —
/// the deterministic walk; not the today window's pool, a focus
/// target may not be scheduled today), bounded to
/// [`FOCUS_SWITCHER_CAP`]. Pure over the published state.
#[must_use]
pub fn focus_switcher_rows(state: &AppState) -> Vec<FocusSwitcherRow> {
    let mut rows = Vec::new();
    'boards: for board in live_boards(state) {
        let mut stacks: Vec<&Stack> = state
            .stacks
            .values()
            .filter(|s| s.board == board.id && !s.deleted && !s.archived)
            .collect();
        stacks.sort_by_key(|s| (s.order, s.id));
        for stack in stacks {
            for task in state.tasks_in_stack(stack.id) {
                if task.done.is_none() {
                    rows.push(FocusSwitcherRow {
                        task: task.id,
                        title: task.title.clone(),
                    });
                    if rows.len() == FOCUS_SWITCHER_CAP {
                        break 'boards;
                    }
                }
            }
        }
    }
    rows
}

/// Which phase the open entry puts the session in (#496, D-F1): the
/// kind discriminator over [`AppState::newest_open`]-shaped picks —
/// the session IS the open entry; no runtime truth exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPhase {
    /// Nothing open: the runtime is parked, the next focus is the
    /// user's act.
    Idle,
    /// A `Work` entry is open: the focus block runs.
    Focus,
    /// A `Respite` entry is open: the break runs.
    Break,
}

impl FocusPhase {
    /// The stable slint-enum mapping (the `View` precedent: Rust
    /// decides, slint displays).
    #[must_use]
    pub const fn from_entry(entry: Option<&TimeEntry>) -> Self {
        match entry {
            Some(e) => match e.kind {
                TimeEntryKind::Work => Self::Focus,
                TimeEntryKind::Respite => Self::Break,
            },
            None => Self::Idle,
        }
    }
}

/// The countdown's remaining-phrase: `mm:ss`, floor-clamped at zero
/// (a skew-produced negative remaining is a presentation `00:00`,
/// never a negative lie — the D-T17 clamp's countdown direction).
#[must_use]
pub fn focus_remaining_phrase(deadline: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let total_secs = (deadline - now).max(TimeDelta::zero()).num_seconds();
    format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
}

/// The overlay's one-frame derivation (D-F11): phase, remaining time,
/// and the current task's title snapshot — all from the published
/// state, the injected timings, and the render-time `now` the push
/// pass carries. Pure; the slint tree computes nothing (the
/// dumb-client line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusOverlay {
    pub phase: FocusPhase,
    /// Whether the open break is the cycle's long break (issue #600).
    /// Inference-only: the walk over published entry history cannot
    /// see the runtime's owned long-break decision, so after an
    /// externally-aborted long break this flag (and the countdown
    /// with it) can disagree with the runtime until the next boundary
    /// — recorded leniency (R-1); always `false` while a focus block
    /// runs or the mode is idle.
    pub long_break: bool,
    /// The countdown text: `mm:ss` to the phase deadline while a
    /// phase runs, empty while idle.
    pub remaining_text: String,
    /// The open entry's title snapshot (empty while idle).
    pub task_title: String,
}

/// See [`FocusOverlay`]. `open` is the caller's newest-open pick. The
/// deadline is derived through the same domain walk the runtime uses
/// (issue #600, D-FF11): an open break at the cadence arms the long
/// length, everything else keeps the landed lengths.
#[must_use]
pub fn focus_overlay(state: &AppState, timing: &FocusTiming, now: DateTime<Utc>) -> FocusOverlay {
    let open = state.tracking.newest_open();
    let phase = FocusPhase::from_entry(open);
    match open {
        Some(entry) => {
            let long_break = match entry.kind {
                TimeEntryKind::Work => false,
                TimeEntryKind::Respite => {
                    focus_break_is_long(&state.tracking.entries, entry, timing, None)
                }
            };
            let block = match entry.kind {
                TimeEntryKind::Work => timing.work(),
                TimeEntryKind::Respite => {
                    if long_break {
                        timing.long_break()
                    } else {
                        timing.respite()
                    }
                }
            };
            let deadline = entry.start + block;
            FocusOverlay {
                phase,
                long_break,
                remaining_text: focus_remaining_phrase(deadline, now),
                task_title: entry.task_title.clone(),
            }
        }
        None => FocusOverlay {
            phase,
            long_break: false,
            remaining_text: String::new(),
            task_title: String::new(),
        },
    }
}

/// The kiosk gate (D-T16): the gate empties the ACTION, never the
/// display — the appliance shows what is being worked on, with no
/// tracking commands (tracking is a desktop/REPL surface).
#[must_use]
pub const fn tracking_chip_action_visible(kiosk: bool) -> bool {
    !kiosk
}

/// The card badge's visibility (D-T16): the card's task holds the
/// shared open-entry pick.
#[must_use]
pub fn tracking_badge_visible(open_task: Option<TaskId>, card_task: TaskId) -> bool {
    open_task == Some(card_task)
}

/// The recurring badge's typed parts (DR-6/4, D-S7): the domain's
/// next-occurrence law, displayed. `next: None` renders the
/// pattern-only badge (the series' last head — the `Until` bound
/// excludes the next member); `stale` is the overdue-recurring display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecurBadge {
    /// The next advertised member; `None` past the `Until` bound.
    pub next: Option<chrono::NaiveDate>,
    /// The head's reference date is before today's window-date.
    pub stale: bool,
    /// The pattern, the label glue's input.
    pub pattern: taskboard_domain::RecurPattern,
    /// The pause field's truth (issue #506): while `today <=
    /// paused_until` the covered members do not exist, so the label
    /// renders the paused chip instead of a next-member phrase.
    pub paused_until: Option<chrono::NaiveDate>,
}

/// The recurring badge's pure decision (D-S7): `None` (hidden) unless
/// the task carries a rule **and** is open — done heads are closed
/// occurrences (the successor carries the series forward and gets its
/// own badge). The body is the domain's `next_occurrence` + the rule
/// echo — no UI-held recurrence state; derived per frame from the
/// published state. `Count` is never surfaced here (engine-evaluated
/// history; the badge shows schedule truth, not quota state).
#[must_use]
pub fn recur_badge(
    task: &Task,
    day: taskboard_domain::days::DayCalendar,
    now: DateTime<Utc>,
) -> Option<RecurBadge> {
    let rule = task.recur.as_ref()?;
    if !task.is_open() {
        return None;
    }
    let next = taskboard_domain::next_occurrence(rule, task, day, now);
    Some(RecurBadge {
        next: next.date,
        stale: next.stale,
        pattern: rule.pattern.clone(),
        paused_until: rule.paused_until,
    })
}

/// The badge's label glue: the pattern's canonical text form plus a
/// day-granularity next phrase over the [`duration_bucket`] day bucket
/// of `next - today` (never a wall-clock delta — day granularity keeps
/// the label diff-stable within a window, so mid-day re-pushes write
/// identical labels). A past roll target clamps to `Now` ("today" —
/// the stale tone carries the overdue-recurring display);
/// `next: None` renders the pattern-only badge. The string is UX copy:
/// tests assert non-empty and deterministic, never the wording.
#[must_use]
pub fn recur_badge_label(badge: &RecurBadge, today: chrono::NaiveDate) -> String {
    // The paused chip (issue #506, plan D-8): while the pause stands
    // the covered members do not exist — a "next in Nd" phrase would
    // be a lie, so the chip names the pause's truth instead. An
    // expired pause is inert (the domain law), and this read is the
    // same date comparison — the chip disappears with the pause.
    if let Some(pause) = badge.paused_until.filter(|pause| today <= *pause) {
        return format!(
            "{} \u{b7} paused through {pause}",
            badge.pattern.text_form()
        );
    }
    let phrase = badge.next.map(|next| {
        match duration_bucket(next.signed_duration_since(today)) {
            // A past roll target clamps into `Now` (the stale tone
            // carries the overdue display); the sub-day buckets cannot
            // arise (both sides are midnight dates) but share the arm.
            DurationBucket::Now | DurationBucket::Minutes(_) | DurationBucket::Hours(_) => {
                "today".to_owned()
            }
            DurationBucket::Days(1) => "tomorrow".to_owned(),
            DurationBucket::Days(n) => format!("in {n}d"),
        }
    });
    match phrase {
        Some(phrase) => format!("{} \u{b7} {phrase}", badge.pattern.text_form()),
        None => badge.pattern.text_form(),
    }
}

/// The task-detail button's verb (D-T16): `Stop` when the subject
/// holds the open entry, `Start` otherwise. No done-gate — the
/// engine's R-8 ruling ("a completed task is trackable") owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackDetail {
    Start,
    Stop,
}

/// The detail button's pure decision: `None` with no subject, else the
/// verb over the shared open-entry pick.
#[must_use]
pub fn detail_tracking_verb(state: &AppState, subject: Option<TaskId>) -> Option<TrackDetail> {
    let subject = subject?;
    let holding = tracking_open_task(state) == Some(subject);
    Some(if holding {
        TrackDetail::Stop
    } else {
        TrackDetail::Start
    })
}

/// The task-view summary row's numeric reduction (r4 D17): the task's
/// total tracked time at **second precision** — the parts ARE the
/// precision contract, so the phrase glue needs no text pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TrackedTotal {
    pub hours: u32,
    pub minutes: u32,
    pub seconds: u32,
}

/// The task's tracked-time total (r4 D17): Σ over the engine's
/// `TimeEntry`s for `subject`, each [`taskboard_domain::TimeEntry::duration`]
/// as of `now` — a **closed** entry contributes its span, an **open**
/// one its elapsed-to-now (the running timer's accrual; D17's "incl.
/// a running entry's elapsed"). `duration`'s zero-clamp keeps inverted
/// skew spans out; `now` is data, nothing ticks. Pure, no UI types.
#[must_use]
pub fn tracked_total(state: &AppState, subject: TaskId, now: DateTime<Utc>) -> TrackedTotal {
    let total_seconds: i64 = state
        .tracking
        .entries
        .values()
        // The D-11 ruling (DR-5): respite counts against the work
        // streak, never toward it — the summary is Work-only (the
        // `work_in_window` semantics at lifetime scope).
        .filter(|entry| {
            entry.task == subject && entry.kind == taskboard_domain::TimeEntryKind::Work
        })
        .map(|entry| entry.duration(now).num_seconds())
        .sum();
    let total_seconds = u64::try_from(total_seconds.max(0)).unwrap_or(u64::MAX);
    TrackedTotal {
        hours: u32::try_from(total_seconds / 3600).unwrap_or(u32::MAX),
        minutes: u32::try_from((total_seconds % 3600) / 60).unwrap_or(u32::MAX),
        seconds: u32::try_from(total_seconds % 60).unwrap_or(u32::MAX),
    }
}

/// The summary's glue (r4 D17): zero-leading segments drop (hours,
/// then minutes); seconds always render — "1h 23m 45s", "2m 5s", and
/// the all-zero canonical "0s". Deterministic and diff-stable within
/// a second; UX copy — tests assert structure, never wording.
#[must_use]
pub fn tracked_total_phrase(total: &TrackedTotal) -> String {
    let mut segments = Vec::new();
    if total.hours > 0 {
        segments.push(format!("{}h", total.hours));
    }
    if total.hours > 0 || total.minutes > 0 {
        segments.push(format!("{}m", total.minutes));
    }
    segments.push(format!("{}s", total.seconds));
    segments.join(" ")
}

// ---- The tracking surfaces (r4 chunk 10, D35–D37) -----------------------

/// The running row's end-cell copy constant (D-TS3): an open entry has
/// no end to render — the cell names the state. UX copy; tests assert
/// structural facts, never this wording.
pub const TRACK_RUNNING_END: &str = "running";

/// The midnight-crossing end's mark (D-TS3): rendered after the end
/// time when the end's local date differs from the start's.
pub const TRACK_END_PLUS_DAY: &str = "+1d";

/// The entries form's date hint (D-TS4's field decorations; the times
/// reuse [`crate::due_edit::TIME_SYNTAX_HINT`]).
pub const TRACK_DATE_HINT: &str = "YYYY-MM-DD";

/// One entries-overlay row (D-TS3): the display parts plus the entry
/// id (the row model in the markup mirrors these; the id travels only
/// through the `IdTables` index seam, never the markup).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackEntryRow {
    /// The entry (row-activate → edit; delete arms against it).
    pub id: TimeEntryId,
    /// The start's local `YYYY-MM-DD`.
    pub date: String,
    /// The start's local `HH:MM`.
    pub start: String,
    /// The end's local `HH:MM`; the running constant while open.
    pub end: String,
    /// The end's local date differs from the start's (the `+1d` mark
    /// predicate; never set while running).
    pub end_other_day: bool,
    /// The row's spent phrase — [`TimeEntry::duration`] at `now` through
    /// [`tracked_total_phrase`] (F16's seconds-finest precision, one
    /// phrase vocabulary).
    pub spent: String,
    /// `end.is_none()` — the running mark bit (the status dot).
    pub running: bool,
}

/// The subject's entry rows in display order (D-TS3): **newest-first**,
/// `(start, id)` descending — the UI's convention (the chip's open
/// list, notifications), recorded here, not inherited from the CLI's
/// file-format order. Every entry of the subject renders (respite rows
/// included — history renders honestly); the overlay's header total is
/// the caller's [`tracked_total`] verbatim (Work-only — reconciliation
/// by construction).
#[must_use]
pub fn track_entry_rows(
    state: &AppState,
    subject: TaskId,
    now: DateTime<Utc>,
    tz: &impl TimeZone,
) -> Vec<TrackEntryRow> {
    let mut rows: Vec<(&TimeEntry, TrackEntryRow)> = state
        .tracking
        .entries
        .values()
        .filter(|entry| entry.task == subject)
        .map(|entry| {
            let start_local = tz.from_utc_datetime(&entry.start.naive_utc());
            let row = TrackEntryRow {
                id: entry.id,
                date: start_local.date_naive().format("%Y-%m-%d").to_string(),
                start: start_local.time().format("%H:%M").to_string(),
                end: match entry.end {
                    None => TRACK_RUNNING_END.to_owned(),
                    Some(end) => tz
                        .from_utc_datetime(&end.naive_utc())
                        .time()
                        .format("%H:%M")
                        .to_string(),
                },
                end_other_day: match entry.end {
                    None => false,
                    Some(end) => {
                        tz.from_utc_datetime(&end.naive_utc()).date_naive()
                            != start_local.date_naive()
                    }
                },
                spent: tracked_total_phrase(&part_total(entry.duration(now))),
                running: entry.is_open(),
            };
            (entry, row)
        })
        .collect();
    rows.sort_by_key(|(entry, _)| std::cmp::Reverse((entry.start, entry.id)));
    rows.into_iter().map(|(_, row)| row).collect()
}

/// `TrackedTotal` over a raw duration (the row phrase's glue — the
/// same second-finest parts [`tracked_total`] assembles, sourced from
/// a duration instead of the state).
#[must_use]
pub fn part_total(duration: TimeDelta) -> TrackedTotal {
    let seconds = u64::try_from(duration.num_seconds().max(0)).unwrap_or(u64::MAX);
    TrackedTotal {
        hours: u32::try_from(seconds / 3600).unwrap_or(u32::MAX),
        minutes: u32::try_from((seconds % 3600) / 60).unwrap_or(u32::MAX),
        seconds: u32::try_from(seconds % 60).unwrap_or(u32::MAX),
    }
}

/// Which form field a rejection lands on (D-TS4's mapping table). The
/// pair's rejection (`end <= start`) lands on the **end** field — the
/// later boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackField {
    Date,
    Start,
    End,
}

/// The form's parsed draft (D-TS4): the committed instants, local
/// wall-clocks resolved to store-UTC through the injected zone. An
/// `end: None` rides only an **edit** commit (the mask's keep-open);
/// an **add** commit always carries a closed span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackEntryDraft {
    pub start: DateTime<Utc>,
    /// `None` = the end field stayed empty (edit only: `end: None` in
    /// the adjust mask — the entry stays open, well-defined).
    pub end: Option<DateTime<Utc>>,
}

/// The form's live parse (D-TS4's validity table): per-field tri-state
/// validity (the due editor's shape), the field a pair rejection
/// landed on, and the commit-ready draft (`None` = commit refused).
/// Total on arbitrary input; DST gap/overlap wall-clocks refuse (the
/// due editor's `.single()` rule — a guessed instant is worse than a
/// re-aimed one).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackFormParse {
    pub date_valid: Option<bool>,
    pub start_valid: Option<bool>,
    pub end_valid: Option<bool>,
    /// The live rejection's field (the pair's rejection lands on
    /// [`TrackField::End`]); `None` = nothing rejected.
    pub rejected_by: Option<TrackField>,
    /// `Some` = the commit seam may fill `track_entry_draft`.
    pub draft: Option<TrackEntryDraft>,
}

/// Parses the tracking form (D-TS4). `allow_open_end` selects the
/// add/edit posture: `false` (add) demands a parsed end for a draft
/// (a `LogTime` span is always closed — an empty end leaves the draft
/// `None`, the field neutral until typed); `true` (edit) maps an
/// empty end onto `draft.end: None` (the mask's keep-open). The date
/// resolves empty to `today` (local). Local→UTC goes through the one
/// `resolve_local` seam (the due editor's zone rule).
#[allow(clippy::too_many_arguments)] // three fields + posture + zone; the table's inputs
#[must_use]
pub fn parse_track_form(
    date_text: &str,
    start_text: &str,
    end_text: &str,
    allow_open_end: bool,
    today: NaiveDate,
    tz: &impl TimeZone,
) -> TrackFormParse {
    let mut out = TrackFormParse::default();
    let date = date_text.trim();
    let date = if date.is_empty() {
        today
    } else if let Ok(parsed) = NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        parsed
    } else {
        out.date_valid = Some(false);
        out.rejected_by = Some(TrackField::Date);
        return out;
    };
    let resolve = |time_text: &str| -> Result<Option<DateTime<Utc>>, ()> {
        let text = time_text.trim();
        if text.is_empty() || text == "--:--" {
            return Ok(None);
        }
        match crate::due_edit::parse_time_lenient(text) {
            // A DST-refused wall-clock is a typed refusal (never a
            // guessed instant, never a neutral) — the same
            // `.single()` rule as the due editor's zone seam.
            None => Err(()),
            Some(time) => crate::due_edit::resolve_local(NaiveDateTime::new(date, time), tz)
                .map(Some)
                .map_err(|_| ()),
        }
    };
    let (start_instant, end_instant) = match (resolve(start_text), resolve(end_text)) {
        (Ok(start), Ok(end)) => (start, end),
        // An unparseable or DST-refused wall-clock is a typed entry
        // rejection — it maps to its field, the honest refusal.
        (Err(()), _) => {
            out.start_valid = Some(false);
            out.rejected_by = Some(TrackField::Start);
            return out;
        }
        (_, Err(())) => {
            out.end_valid = Some(false);
            out.rejected_by = Some(TrackField::End);
            return out;
        }
    };
    out.start_valid = start_instant.map(|_| true);
    out.end_valid = end_instant.map(|_| true);
    let Some(start) = start_instant else {
        // The seeded neutral: nothing typed yet, no draft.
        return out;
    };
    if let Some(end) = end_instant {
        // The pair's rejection lands on the later boundary; commit
        // refused.
        if end <= start {
            out.end_valid = Some(false);
            out.rejected_by = Some(TrackField::End);
            return out;
        }
        out.draft = Some(TrackEntryDraft {
            start,
            end: Some(end),
        });
    } else if allow_open_end {
        // Edit + empty end: the keep-open mask is well-defined.
        out.draft = Some(TrackEntryDraft { start, end: None });
    }
    // Add + empty end: draft stays `None` (a `LogTime` span is always
    // closed); the field reads neutral until typed.
    out
}

/// The #490 today section's per-task rows (D-TS7): Work-only per-task
/// sums over the day window (`work_in_window` — the aggregator
/// deviation recorded in the plan §5: the CLI's `track status` and the
/// summary row already define "tracked" work-only; a GUI day total
/// disagreeing with `track status` for the same window would be a bug
/// class). Canonical task order = the state map's key order (the
/// `BTreeMap`'s deterministic order — the same convention the state's
/// serialization contract names). Live tasks only — the today-section
/// pattern's `is_live` filter (a tombstoned task's surviving history
/// must not render a row, let alone an openable one). Tasks without
/// tracked work are absent — an empty day yields the empty vec (the
/// collapse's input).
#[must_use]
pub fn tracked_today_rows(
    state: &AppState,
    window: (DateTime<Utc>, DateTime<Utc>),
    now: DateTime<Utc>,
) -> Vec<(TaskId, TimeDelta)> {
    state
        .tasks
        .values()
        .filter(|task| task.is_live())
        .map(|task| {
            let total = taskboard_domain::tracking::work_in_window(
                state
                    .tracking
                    .entries
                    .values()
                    .filter(|entry| entry.task == task.id),
                window,
                now,
            );
            (task.id, total)
        })
        .filter(|(_, total)| *total > TimeDelta::zero())
        .collect()
}

/// One active timer row (D-I7): the alarm plus its walked effective
/// occurrence, carried so the push pass derives the countdown phrase
/// once per refresh. The insistence rung rides verbatim — the row
/// model projects the badge bit, the projection never defaults
/// (r4 D34 remainder #5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerRow {
    /// The alarm the cancel affordance dispatches against.
    pub alarm: taskboard_domain::AdhocAlarmId,
    /// The label snapshot, carried verbatim.
    pub label: String,
    /// The effective occurrence (snooze walk included).
    pub effective: DateTime<Utc>,
    /// The alarm's insistence rung (the escalation ladder's datum).
    pub insistence: taskboard_domain::Insistence,
}

/// The status-bar chip's parts (D-I6): visibility and nearest-fire
/// selection are the pure decision; the push assembles the display
/// text. `effective` may be past (an awaiting alarm is a live timer
/// whose occurrence already fired — the chip still names it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerChip {
    pub alarm: taskboard_domain::AdhocAlarmId,
    pub label: String,
    pub effective: DateTime<Utc>,
}

/// The per-row button visibility table for the notification surface
/// (D-P11): the anchor-aware repair as reviewed data, not markup
/// drift. Dismiss/Snooze are universal; task anchors add Done + Mute
/// (task-shaped) + Start Task (DR-5/4, D-T16 — the tracking verb is
/// task-shaped too); alarm anchors add Cancel (the timer's off-switch).
/// (The six-bool shape is the deliberate table — clippy's
/// state-machine suggestion would obscure the reviewed row.)
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AwaitingButtons {
    pub dismiss: bool,
    pub done: bool,
    pub snooze: bool,
    pub mute: bool,
    pub cancel: bool,
    pub track_start: bool,
}

/// The active timer rows for `now` (D-I7, D-P17): one row per live
/// alarm still counting down — walked effective occurrence future —
/// plus the tick-boundary transient (effective past but not yet
/// delivered; its countdown clamps to `"now"`). Awaiting rows
/// (delivered ∧ ¬dismissed at the effective) are the notification
/// surface's, never duplicated here. Canonical `(effective, id)`
/// order; the walk is the domain's `alarm_occurrence`, never
/// re-derived.
#[must_use]
pub fn timer_rows(state: &AppState, now: DateTime<Utc>) -> Vec<TimerRow> {
    let mut rows: Vec<TimerRow> = state
        .reminders
        .alarms
        .values()
        .filter_map(|alarm| {
            let effective = taskboard_domain::alarm_occurrence(&state.reminders, alarm.id)?;
            let counting = effective > now
                || !state
                    .reminders
                    .is_delivered(&ReminderRef::Adhoc(alarm.id), &effective);
            counting.then(|| TimerRow {
                alarm: alarm.id,
                label: alarm.label.clone(),
                effective,
                insistence: alarm.insistence,
            })
        })
        .collect();
    rows.sort_by_key(|row| (row.effective, row.alarm));
    rows
}

/// The awaiting (fired ∧ ¬dismissed) alarm-anchored row count (D-I7):
/// the notification surface's own rows filtered to Adhoc refs — one
/// derivation, no second list to drift.
#[must_use]
pub fn awaiting_alarm_count(
    state: &AppState,
    defaults: &taskboard_domain::ReminderDefaults,
    day: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
) -> usize {
    notification_rows(state, defaults, day, now)
        .iter()
        .filter(|row| matches!(row.reminder, ReminderRef::Adhoc(_)))
        .count()
}

/// The status-bar chip's pure decision (D-P5): `None` (hidden) while
/// no live alarm exists; else the nearest row by `(effective, id)`
/// over ALL live alarms — awaiting included, the surface owns its row.
#[must_use]
pub fn timer_chip(state: &AppState) -> Option<TimerChip> {
    let mut nearest: Option<(DateTime<Utc>, taskboard_domain::AdhocAlarmId)> = None;
    for alarm in state.reminders.alarms.values() {
        let Some(effective) = taskboard_domain::alarm_occurrence(&state.reminders, alarm.id) else {
            continue;
        };
        let key = (effective, alarm.id);
        if nearest.is_none_or(|best| key < best) {
            nearest = Some(key);
        }
    }
    nearest.map(|(effective, id)| {
        let alarm = &state.reminders.alarms[&id];
        TimerChip {
            alarm: id,
            label: alarm.label.clone(),
            effective,
        }
    })
}

/// The timers form's timing grammar (D-P7/D-P8): the D-R9 CLI ruling
/// mirrored as a pure, tz-injected parse. Prefix-dispatched on the
/// first whitespace token: `"in <dur>"` rides the domain's
/// [`taskboard_domain::ReminderOffset::parse_text_form`] (a `≤ 0`
/// magnitude is `None` — the domain's `InvalidAdhocWhen` rejected at
/// the seam); `"at <t>"` mirrors the CLI's `parse_at_flag` (RFC-3339
/// any-offset; `YYYY-MM-DD` host-local midnight; `HH:MM[:SS]` today
/// host-local, rolled to tomorrow when already past — `now` is data,
/// `tz` the injected seam; a DST-unresolvable local time is `None`).
/// Anything else — empty, missing prefix, unknown form — is `None`.
#[must_use]
pub fn parse_timer_when(
    raw: &str,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<taskboard_domain::AdhocWhen> {
    let (prefix, rest) = raw.split_once(char::is_whitespace)?;
    match prefix {
        "in" => {
            let offset = taskboard_domain::ReminderOffset::parse_text_form(rest.trim())?;
            let duration = -offset.as_duration();
            (duration > TimeDelta::zero()).then_some(taskboard_domain::AdhocWhen::In(duration))
        }
        "at" => parse_at_instant(rest, now, tz).map(taskboard_domain::AdhocWhen::At),
        _ => None,
    }
}

/// The shared at-instant parser (r4 D30/D6): exactly one definition of
/// the `"at <t>"` arm, extracted verbatim from [`parse_timer_when`] and
/// shared with [`parse_rule_when`]. RFC-3339 any-offset; `YYYY-MM-DD`
/// host-local midnight; `HH:MM[:SS]` today host-local, rolled to
/// tomorrow when already past — `now` is data, `tz` the injected seam;
/// a DST-unresolvable local time is `None`.
#[must_use]
pub fn parse_at_instant(
    rest: &str,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<DateTime<Utc>> {
    let rest = rest.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rest) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(rest, "%Y-%m-%d") {
        let midnight = date.and_hms_opt(0, 0, 0)?;
        return tz
            .from_local_datetime(&midnight)
            .single()
            .map(|local| local.with_timezone(&chrono::Utc));
    }
    let time = chrono::NaiveTime::parse_from_str(rest, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(rest, "%H:%M:%S"))
        .ok()?;
    let today = now.with_timezone(tz).date_naive();
    let resolve = |day: chrono::NaiveDate| {
        tz.from_local_datetime(&day.and_time(time))
            .single()
            .map(|local| local.with_timezone(&chrono::Utc))
    };
    let mut instant = resolve(today)?;
    if instant <= now {
        instant = resolve(today.succ_opt()?)?;
    }
    Some(instant)
}

/// The parsed rule-add timing input (r4 D30/D6): either the legacy
/// due-anchored offset (bare `"10m"` — the parity anchor) or the at/in
/// shape (`"in 10m"` / `"at 17:00"`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RuleWhen {
    /// Fire this far before the task's due date.
    DueOffset(taskboard_domain::ReminderOffset),
    /// The at/in input — carried as domain `AdhocWhen` verbatim so the
    /// planner is the only instant-stamping site (`In` needs no clock
    /// read anywhere in the UI).
    At(taskboard_domain::AdhocWhen),
}

/// The rule-add field's grammar (r4 decision 2): prefix-dispatched on
/// the first whitespace token like [`parse_timer_when`] — `"in <dur>"`
/// rides the domain's offset parser (`≤ 0` magnitude is `None`, the
/// same seam refusal), `"at <t>"` rides the shared
/// [`parse_at_instant`]; **anything else** is the bare-offset fallback
/// through [`taskboard_domain::ReminderOffset::parse_text_form`] (bare
/// `"10m"` stays due-anchored — the parity rule). Empty is `None`.
///
/// The exactly-one contract: one at-instant parser
/// ([`parse_at_instant`]), one in/bare duration grammar (the domain's
/// `parse_text_form`), one prefix dispatcher per consumer grammar
/// ([`parse_timer_when`] for timers, this for rules — they differ in
/// the bare fallback, which is the feature).
#[must_use]
pub fn parse_rule_when(
    raw: &str,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<RuleWhen> {
    let trimmed = raw.trim();
    if let Some((prefix, rest)) = trimmed.split_once(char::is_whitespace) {
        match prefix {
            "in" => {
                let offset = taskboard_domain::ReminderOffset::parse_text_form(rest.trim())?;
                let duration = -offset.as_duration();
                return (duration > TimeDelta::zero())
                    .then_some(RuleWhen::At(taskboard_domain::AdhocWhen::In(duration)));
            }
            "at" => {
                return parse_at_instant(rest, now, tz)
                    .map(|instant| RuleWhen::At(taskboard_domain::AdhocWhen::At(instant)));
            }
            // A prefixed-but-unknown form ("soon 10m") falls through to
            // the bare fallback below, which rejects it — one miss
            // shape, no second grammar.
            _ => {}
        }
    }
    taskboard_domain::ReminderOffset::parse_text_form(trimmed).map(RuleWhen::DueOffset)
}

/// The tracker badge's visibility predicate (issue #492): the
/// kind-marked card carries the badge — the kiosk-visible name of the
/// kind. Pure derivation of the domain datum; the Slint file owns the
/// glyph only.
#[must_use]
pub fn tracker_badge_visible(task: &taskboard_domain::entities::Task) -> bool {
    task.kind == taskboard_domain::entities::TaskKind::Tracker
}

/// The custom-offset field's syntax hint (r4 D27): the copy constant.
pub const OFFSET_SYNTAX_HINT: &str = "45m, 3h, 2d, 1w";

/// The timers `when` field's syntax hint (r4 D27).
pub const TIMER_SYNTAX_HINT: &str = "in 10m · at 09:30 · at 2030-04-01";

/// The rule-add field's three-form syntax hint (r4 D30/D6): bare stays
/// due-anchored (the parity anchor), the at/in prefixes mint the
/// absolute kind.
pub const RULE_WHEN_SYNTAX_HINT: &str = "45m · in 2h · at 17:00";

/// The rule-add field's tri-state validity probe (r4 D30/D6) over
/// [`parse_rule_when`] — the same injected-tz seam and refusal
/// discipline as [`timer_when_validity`]. Empty is neutral.
#[must_use]
pub fn rule_when_validity(
    text: &str,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<bool> {
    if text.trim().is_empty() {
        return None;
    }
    Some(parse_rule_when(text, now, tz).is_some())
}

/// The custom-offset field's tri-state validity probe (r4 D27): the
/// predicate *calls* the authoritative domain parser — no second
/// grammar. Empty is neutral (`None`).
#[must_use]
pub fn offset_validity(text: &str) -> Option<bool> {
    if text.trim().is_empty() {
        return None;
    }
    Some(taskboard_domain::ReminderOffset::parse_text_form(text).is_some())
}

/// The timers `when` field's tri-state validity probe (r4 D27) over
/// [`parse_timer_when`] — the same injected-tz seam, the same refusal
/// discipline. Empty is neutral.
#[must_use]
pub fn timer_when_validity(
    text: &str,
    now: DateTime<Utc>,
    tz: &impl chrono::TimeZone,
) -> Option<bool> {
    if text.trim().is_empty() {
        return None;
    }
    Some(parse_timer_when(text, now, tz).is_some())
}

/// The anchor-aware per-row button table (D-P11): the notification
/// surface's total visibility decision. See [`AwaitingButtons`].
#[must_use]
pub const fn awaiting_row_buttons(anchor: taskboard_domain::ReminderAnchor) -> AwaitingButtons {
    match anchor {
        taskboard_domain::ReminderAnchor::Task(_) => AwaitingButtons {
            dismiss: true,
            done: true,
            snooze: true,
            mute: true,
            cancel: false,
            track_start: true,
        },
        taskboard_domain::ReminderAnchor::Alarm(_) => AwaitingButtons {
            dismiss: true,
            done: false,
            snooze: true,
            mute: false,
            cancel: true,
            track_start: false,
        },
        // A break row behaves like an alarm row for affordances: no
        // task target, so no Done/Mute/Start; the insistence ladder
        // applies as-is.
        // Break and tracker rows behave like an alarm row for
        // affordances: no task target, so no Done/Mute/Start; the
        // ladder never applies to a tracker nudge (Normal pinned, #504
        // D-6).
        taskboard_domain::ReminderAnchor::Break(_)
        | taskboard_domain::ReminderAnchor::Tracker(_) => AwaitingButtons {
            dismiss: true,
            done: false,
            snooze: true,
            mute: false,
            cancel: false,
            track_start: false,
        },
    }
}

/// The board's live stacks in display order (D10): `(order, id)`
/// ascending — presentation's own reading of the domain's persisted
/// sort position (the domain owns the concept and the write side;
/// this is the display-side reader).
#[must_use]
pub fn stacks_in_board(state: &AppState, board: BoardId) -> Vec<&Stack> {
    let mut stacks: Vec<&Stack> = state
        .stacks
        .values()
        .filter(|s| !s.deleted && s.board == board)
        .collect();
    stacks.sort_by_key(|s| (s.order, s.id));
    stacks
}

/// The live, non-archived boards in canonical order: `state.boards`
/// `BTreeMap` iteration order — board-id ascending, the exact order
/// the CLI's `boards list` renders. The board walk's domain and the
/// seed rule's candidate list.
#[must_use]
pub fn live_boards(state: &AppState) -> Vec<&Board> {
    state
        .boards
        .values()
        .filter(|b| !b.deleted && !b.archived)
        .collect()
}

/// The global inbox's open tasks (chunk 10 D9; the one inbox reading
/// — the toolbar chip counts it, the review walk's Inbox stage lists
/// it): live, open, anchor-filtered tasks whose stack belongs to the
/// global Inbox board, in stack order then canonical `(order, id)`.
/// Empty when no Inbox board exists (an empty inbox is triage-done).
#[must_use]
pub fn inbox_tasks(state: &AppState) -> Vec<TaskId> {
    let Some(inbox) = state.inbox_board() else {
        return Vec::new();
    };
    let mut tasks: Vec<(i64, StackId, i64, TaskId)> = state
        .tasks
        .values()
        .filter(|task| task.is_live() && task.is_open() && !is_anchor_task(task))
        .filter_map(|task| {
            let stack = state.stacks.get(&task.stack)?;
            (stack.board == inbox).then_some((stack.order, stack.id, task.order, task.id))
        })
        .collect();
    tasks.sort();
    tasks.into_iter().map(|(_, _, _, id)| id).collect()
}

/// The global inbox's open count: `inbox_tasks`' length — one inbox,
/// one number, the `due_sets` template.
#[must_use]
pub fn inbox_count(state: &AppState) -> usize {
    inbox_tasks(state).len()
}

/// The today row's context line (chunk 10 D7, D-V3): the stack title
/// always; the `"<board> › <stack>"` board badge only on a multi-board
/// desktop — and suppressed entirely on kiosk (no badges). A
/// deterministic state-derived decision, made once.
#[must_use]
pub fn today_row_context(state: &AppState, task: &Task, kiosk: bool, multi_board: bool) -> String {
    let Some(stack) = state.stacks.get(&task.stack) else {
        return String::new();
    };
    if kiosk || !multi_board {
        stack.title.clone()
    } else {
        state.boards.get(&stack.board).map_or_else(
            || stack.title.clone(),
            |board| format!("{} \u{203a} {}", board.title, stack.title),
        )
    }
}

/// The move-flow hint (chunk 10 D6, D-V5's literal vocabulary): the
/// target stack's `"<board> › <stack>"` — always both parts, uniform,
/// no conditional abbreviation. Empty on a dangling stack (data
/// degradation, not an error).
#[must_use]
pub fn move_hint(state: &AppState, target: StackId) -> String {
    state
        .stacks
        .get(&target)
        .and_then(|stack| state.boards.get(&stack.board).map(|board| (stack, board)))
        .map_or_else(String::new, |(stack, board)| {
            format!("{} \u{203a} {}", board.title, stack.title)
        })
}

/// The single live, non-archived board — the view-state seed rule's
/// pick ([`ViewState`](crate::view_state::ViewState)'s `board` when
/// `None`): the first of [`live_boards`]. Its only remaining role;
/// selection lives in the view state.
#[must_use]
pub fn live_board(state: &AppState) -> Option<&Board> {
    live_boards(state).into_iter().next()
}

// -- the alarm window (DR-3/4, D-I9.2) -------------------------------

/// The alarm window's snooze postpone: a fixed presentation constant —
/// the same default the notification surface's snooze flow lands on;
/// per-run pickers are a non-goal (the stage-1 fence).
pub const ALARM_WINDOW_SNOOZE: TimeDelta = TimeDelta::try_minutes(10).expect("non-zero constant");

/// One awaiting alarm-insistent occurrence, the alarm window's row:
/// identity for the button commands plus the carried title snapshot.
/// Deliberately not [`NotificationRow`] — the anchor-card filter and
/// affordance table differ; each surface keeps one derivation over the
/// same domain function, nothing second-evaluates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlarmRow {
    /// The adhoc alarm (the Cancel command's target).
    pub alarm: AdhocAlarmId,
    /// The reminder ref the dismiss and snooze commands carry.
    pub reminder: ReminderRef,
    /// The effective occurrence that fired.
    pub occurrence: DateTime<Utc>,
    /// The title snapshot the domain view assembled (the alarm label)
    /// — carried, never re-derived.
    pub title: String,
}

/// The alarm window's rows for `now` (D-I9.2): the domain's own
/// `active_notifications` filtered one step to the alarm-insistent
/// payloads — the escalation driver's awaiting set,
/// presentation-shaped. Canonical `(occurrence, reminder)` order.
#[must_use]
pub fn alarm_window_rows(
    state: &AppState,
    defaults: &taskboard_domain::ReminderDefaults,
    day: taskboard_domain::DayCalendar,
    now: DateTime<Utc>,
) -> Vec<AlarmRow> {
    taskboard_domain::active_notifications(
        &state.tasks,
        &state.tracking,
        &state.habits,
        day,
        &state.reminders,
        defaults,
        now,
    )
    .into_iter()
    .filter_map(|note| {
        if note.insistence != taskboard_domain::Insistence::Alarm {
            return None;
        }
        // An alarm-insistent payload is always alarm-anchored (the
        // implicit default reminder is always `Normal`, D-I3) —
        // unreachable by derivation, total by the filter_map.
        let taskboard_domain::ReminderAnchor::Alarm(alarm) = note.anchor else {
            return None;
        };
        Some(AlarmRow {
            alarm,
            reminder: note.reminder,
            occurrence: note.occurrence,
            title: note.title,
        })
    })
    .collect()
}

/// The review aggregate's window phrases (#214, D-8): estimated and
/// actual totals, the signed gap phrase, and the comparison counts.
/// Phrases are second-granular like the task view's
/// [`tracked_total_phrase`] (r4 D17's precision contract). `empty`
/// collapses the values when the window holds no comparable tasks
/// (D-9 tier 2) — the section skeleton stays, the values don't.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReviewWindow {
    pub estimated: String,
    pub actual: String,
    pub gap: String,
    pub compared: String,
    pub empty: bool,
}

// -- the review walk's phrase vocabulary (DR-7 chunk 3, plan D-W7) -----
//
// The walk's and the surfacing card's copy lives here, in pure phrase
// fns over data (the dumb-client line: slint computes nothing). The
// ruled strings (DR-7 D-J9, C1–C8/C10/C11 approved verbatim) are the
// phrase fns' own units' to assert — no other test may assert walk
// text (AGENTS §5). The vocabulary guard unit pins the P9 rule: no
// verdict language in any walk/card phrase.

/// One today-view surfacing card (DR-7 D-J10): at most one — the
/// title, the effort-transparent body, the click affordance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewCardRow {
    pub title: String,
    pub body: String,
    pub action: String,
}

/// The card's title (C1): the cadence noun, not an instruction.
#[must_use]
pub fn review_card_title(cadence: taskboard_domain::ReviewCadence) -> String {
    let noun = match cadence {
        taskboard_domain::ReviewCadence::Day => "Day",
        taskboard_domain::ReviewCadence::Week => "Week",
        taskboard_domain::ReviewCadence::Month => "Month",
    };
    format!("{noun} review")
}

/// The count as a spelled word (the ruled text for 4); mechanical —
/// zero..=twelve spelled, "many" beyond (no ruled text exists there).
fn count_word(count: usize) -> &'static str {
    const WORDS: [&str; 13] = [
        "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
        "eleven", "twelve",
    ];
    WORDS.get(count).unwrap_or(&"many")
}

/// The card's body (C2, as amended to four prompts): effort-transparent,
/// no obligation language. Both the effort estimate and the prompt
/// count derive from the template data; for the ruled four-prompt day
/// template this reproduces the approved C2 text verbatim.
#[must_use]
pub fn review_card_body(
    cadence: taskboard_domain::ReviewCadence,
    day: chrono::NaiveDate,
) -> String {
    let template = taskboard_domain::DEFAULT_TEMPLATES
        .iter()
        .find(|template| template.cadence == cadence)
        .unwrap_or(&taskboard_domain::DEFAULT_TEMPLATES[0]);
    let prompts = template.prompts.len();
    let minutes = prompts.div_ceil(2);
    format!(
        "A review for {} is open — about {} minutes, {} prompts.",
        review_day_phrase(day),
        count_word(minutes),
        count_word(prompts)
    )
}

/// The card's click affordance (C3).
#[must_use]
pub fn review_card_action(day: chrono::NaiveDate) -> String {
    format!("Review {}", review_day_phrase(day))
}

/// The house day phrase for review surfaces: the ISO date
/// (`date_naive()` convention — the review ledger's date-column form).
/// No second formatter exists to reuse — this is the one (the
/// today/planner headers carry no date).
#[must_use]
pub fn review_day_phrase(day: chrono::NaiveDate) -> String {
    day.to_string()
}

/// The walk's header (C4): `Review — {date}` for the day cadence,
/// `Review — week of {date}` for the week, `Review — {month year}` for
/// the month (the offset-local reading).
#[must_use]
pub fn walk_header(
    cadence: taskboard_domain::ReviewCadence,
    reference: chrono::NaiveDate,
) -> String {
    match cadence {
        taskboard_domain::ReviewCadence::Day => {
            format!("Review — {}", review_day_phrase(reference))
        }
        taskboard_domain::ReviewCadence::Week => {
            format!("Review — week of {}", review_day_phrase(reference))
        }
        taskboard_domain::ReviewCadence::Month => format!(
            "Review — {} {}",
            reference.format("%B").to_string().to_lowercase(),
            reference.year()
        ),
    }
}

/// The progress strip's stage headers (C5/C7/C10): the two neutral
/// stage nouns and the template's own name for the prompts stage.
#[must_use]
pub fn walk_stage_header(
    stage: crate::view_state::WalkStage,
    template: &taskboard_domain::ReflectionTemplate,
) -> String {
    match stage {
        crate::view_state::WalkStage::Inbox { .. } => "Inbox".to_owned(),
        crate::view_state::WalkStage::CarryForward { .. } => "Carry forward".to_owned(),
        crate::view_state::WalkStage::Prompts => template.name.to_owned(),
    }
}

/// The skipped/empty stage's b-line (C5b/C7b); the prompts stage has
/// none (empty).
#[must_use]
pub const fn walk_stage_empty_line(stage: crate::view_state::WalkStage) -> &'static str {
    match stage {
        crate::view_state::WalkStage::Inbox { .. } => "The inbox is empty.",
        crate::view_state::WalkStage::CarryForward { .. } => "Nothing to carry forward.",
        crate::view_state::WalkStage::Prompts => "",
    }
}

/// The Inbox row verbs (C6).
#[must_use]
pub const fn walk_inbox_actions() -> (&'static str, &'static str) {
    ("File to…", "Keep in inbox")
}

/// The Carry forward row verbs (C8).
#[must_use]
pub const fn walk_carry_actions() -> (&'static str, &'static str, &'static str) {
    ("Move to tomorrow", "Pick a date…", "Leave as is")
}

/// The Prompts commit verbs (C11): equal-prominence exits (P12).
#[must_use]
pub const fn walk_commit_actions() -> (&'static str, &'static str) {
    ("Record", "Finish without recording")
}

/// The File-to picker's candidate stacks (plan D-W4): every live
/// non-inbox stack in board/stack order — board canonical order, each
/// board's stacks in order. The walk never offers the inbox itself
/// (filing to the inbox is a no-op).
#[must_use]
pub fn walk_file_candidates(state: &AppState) -> Vec<StackId> {
    let inbox = state.inbox_board();
    let mut candidates: Vec<StackId> = Vec::new();
    for board in live_boards(state) {
        for stack in stacks_in_board(state, board.id) {
            if Some(stack.board) != inbox {
                candidates.push(stack.id);
            }
        }
    }
    candidates
}

/// The walk's inbox rows (plan D-W4): the one shared inbox reading —
/// the same ids the toolbar chip counts, in the same order.
#[must_use]
pub fn walk_inbox_rows(state: &AppState) -> Vec<TaskId> {
    inbox_tasks(state)
}

/// The walk's carry-forward rows (plan D-W4): for the Day cadence the
/// today sections read **verbatim** as the ordered, de-duplicated
/// union `overdue ++ scheduled ++ due_today` (first occurrence wins —
/// VF-9: no parallel classification); for Week/Month the domain's
/// `window_misses` over the window (the structural arm, unreachable
/// from the v0.2 day-only openers, unit-covered).
#[must_use]
pub fn walk_carry_rows(
    tasks: &std::collections::BTreeMap<TaskId, Task>,
    cadence: taskboard_domain::ReviewCadence,
    window: taskboard_domain::ReviewWindow,
    now: DateTime<Utc>,
) -> Vec<TaskId> {
    match cadence {
        taskboard_domain::ReviewCadence::Day => {
            let sections = taskboard_domain::days::today_sections(tasks, now, window.bounds());
            let mut rows: Vec<TaskId> = Vec::with_capacity(
                sections.overdue.len() + sections.scheduled.len() + sections.due_today.len(),
            );
            for id in sections
                .overdue
                .into_iter()
                .chain(sections.scheduled)
                .chain(sections.due_today)
            {
                if !rows.contains(&id) {
                    rows.push(id);
                }
            }
            rows
        }
        taskboard_domain::ReviewCadence::Week | taskboard_domain::ReviewCadence::Month => {
            taskboard_domain::window_misses(tasks, now, window.bounds())
        }
    }
}

/// Second-granular duration glue over the [`TrackedTotal`] parts —
/// the review surface's precision contract rides the same reduction.
#[must_use]
pub fn review_seconds_phrase(seconds: u64) -> String {
    let total = TrackedTotal {
        hours: u32::try_from(seconds / 3600).unwrap_or(u32::MAX),
        minutes: u32::try_from((seconds % 3600) / 60).unwrap_or(u32::MAX),
        seconds: u32::try_from(seconds % 60).unwrap_or(u32::MAX),
    };
    tracked_total_phrase(&total)
}

/// The gap's direction rendering (D-8): structural — the three sign
/// classes are three distinct non-empty forms; the wording is UX copy.
#[must_use]
pub fn review_gap_phrase(gap_seconds: i64) -> String {
    match gap_seconds.cmp(&0) {
        std::cmp::Ordering::Greater => {
            format!(
                "over by {}",
                review_seconds_phrase(gap_seconds.unsigned_abs())
            )
        }
        std::cmp::Ordering::Less => {
            format!(
                "under by {}",
                review_seconds_phrase(gap_seconds.unsigned_abs())
            )
        }
        std::cmp::Ordering::Equal => "on target".to_owned(),
    }
}

/// One window's aggregate phrases from the domain fold. The gap counts
/// only compared rows; the skips keep the denominator honest ("N of M
/// compared" plus the named skips when nonzero — D-5).
#[must_use]
pub fn review_window(aggregate: &taskboard_domain::CalibrationAggregate) -> ReviewWindow {
    let total = aggregate.compared + aggregate.skipped_unestimated + aggregate.skipped_untracked;
    let mut compared = format!("{} of {total} compared", aggregate.compared);
    if aggregate.skipped_unestimated > 0 || aggregate.skipped_untracked > 0 {
        let _ = write!(
            compared,
            ", {} unestimated, {} untracked",
            aggregate.skipped_unestimated, aggregate.skipped_untracked
        );
    }
    let empty = aggregate.compared == 0;
    ReviewWindow {
        estimated: review_seconds_phrase(aggregate.estimated_seconds),
        actual: review_seconds_phrase(aggregate.actual_seconds),
        gap: review_gap_phrase(aggregate.gap_seconds),
        compared,
        empty,
    }
}

/// Phrases for one Review tracking section (#623 slice A, D-S7): the
/// Work total, the Respite total, and the Work-block reading (count +
/// duration in one phrase), each through [`review_seconds_phrase`]
/// (the second-granular precision contract). A window with no tracked
/// time of either kind collapses all three phrases to `""` — the view
/// renders the dashes structurally (D-9 tier 2). A zero block count
/// inside a non-empty section is an observed fact and stays phrased;
/// the dash is reserved for the collapsed section.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrackingSectionPhrases {
    pub work: String,
    pub respite: String,
    pub blocks: String,
    pub empty: bool,
}

/// Derives one tracking section's phrases from the domain folds.
#[must_use]
pub fn review_tracking_section(
    split: &taskboard_domain::WindowSplit,
    blocks: &taskboard_domain::WorkBlockReading,
) -> TrackingSectionPhrases {
    if split.is_empty() {
        return TrackingSectionPhrases {
            empty: true,
            ..TrackingSectionPhrases::default()
        };
    }
    let duration = |delta: chrono::TimeDelta| {
        review_seconds_phrase(delta.num_seconds().max(0).cast_unsigned())
    };
    TrackingSectionPhrases {
        work: duration(split.work),
        respite: duration(split.respite),
        blocks: format!("{} · {}", blocks.blocks, duration(blocks.total)),
        empty: false,
    }
}

/// One day-bar's pushed fractions (the pure half of D-S4): Work and
/// Respite heights as fractions of the strip's maximum day total. The
/// push pass maps this onto the view's `StatBarModel` — the pure layer
/// stays free of generated types.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReviewBar {
    pub work: f32,
    pub respite: f32,
}

/// The strip's bar fractions (D-S4's normalization): per day,
/// `work = work_seconds / strip_max` and `respite = respite_seconds /
/// strip_max`, where `strip_max` is the greatest day total (work +
/// respite) on the strip; an all-zero strip stays all zeros (never
/// NaN); every fraction is finite and ≤ 1.0. The strip shows *shape* —
/// absolute magnitudes live in the section's stat cells.
#[must_use]
pub fn review_bar_fractions(series: &[taskboard_domain::DayTotal]) -> Vec<ReviewBar> {
    #[allow(clippy::cast_precision_loss)] // a bar-height ratio at f32 granularity
    let seconds = |delta: chrono::TimeDelta| delta.num_seconds().max(0) as f32;
    let strip_max = series
        .iter()
        .map(|day| seconds(day.split.work) + seconds(day.split.respite))
        .fold(0.0_f32, f32::max);
    if strip_max <= 0.0 {
        return vec![ReviewBar::default(); series.len()];
    }
    series
        .iter()
        .map(|day| ReviewBar {
            work: (seconds(day.split.work) / strip_max).min(1.0),
            respite: (seconds(day.split.respite) / strip_max).min(1.0),
        })
        .collect()
}

/// Phrases for one Review reflection section (#623 slice B, D-B4):
/// the Mood and Energy means, the flag-occurrence total, and the
/// counts-only per-flag line. Counts and phrases only — no inference,
/// no advice (the X9 fence). A window with no live entries collapses
/// every phrase to `""` (D-9 tier 2 — the view renders the dashes
/// structurally); inside a standing section an unanswered rating is
/// the dash (never a zero) while a zero flag total is an observed fact
/// and stays phrased.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReflectionSectionPhrases {
    pub mood: String,
    pub energy: String,
    pub flags: String,
    /// The joined per-flag counts; `""` when no flag occurred — a
    /// missing line is the honest "no flags", not a collapsed section.
    pub per_flag: String,
    pub empty: bool,
}

/// Derives one reflection section's phrases from the domain fold.
#[must_use]
pub fn review_reflection_section(
    ratings: &taskboard_domain::WindowRatings,
) -> ReflectionSectionPhrases {
    if ratings.entries == 0 {
        return ReflectionSectionPhrases {
            empty: true,
            ..ReflectionSectionPhrases::default()
        };
    }
    let mean = |fold: &taskboard_domain::RatingMean| {
        fold.mean()
            .map_or_else(String::new, |value| format!("{value:.1} / 5"))
    };
    let per_flag = reflection_flag_counts(&ratings.flags)
        .into_iter()
        .map(|(name, count)| format!("{name} ×{count}"))
        .collect::<Vec<_>>()
        .join(" · ");
    ReflectionSectionPhrases {
        mood: mean(&ratings.mood),
        energy: mean(&ratings.energy),
        flags: ratings.flags.total().to_string(),
        per_flag,
        empty: false,
    }
}

/// The per-flag counts line's typed order (D-B4): curated ∪ extra
/// occurrences, count descending then name ascending — curated names
/// by their kebab form, parked names verbatim.
#[must_use]
pub fn reflection_flag_counts(flags: &taskboard_domain::FlagOccurrences) -> Vec<(String, usize)> {
    let mut rows: Vec<(String, usize)> = flags
        .curated
        .iter()
        .map(|(flag, count)| (flag.text_form().to_owned(), *count))
        .chain(
            flags
                .extra
                .iter()
                .map(|(name, count)| (name.clone(), *count)),
        )
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows
}

/// One day-bar's pushed rating fractions (the pure half of D-B6): the
/// Mood and Energy means as fractions of the strip's stacked scale.
/// The push pass maps Mood → `base`, Energy → `stack` on the strip.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReflectionBar {
    pub mood: f32,
    pub energy: f32,
}

/// The two stacked five-point fields' combined scale: a 5/5 pair fills
/// the plot exactly, so the stack never overflows it.
const STACKED_RATING_SCALE: f32 = 10.0;

/// The reflection strip's bar fractions (D-B6): **fixed-scale**
/// normalization, no strip-max rescaling (ratings own a natural scale;
/// normalizing by the strip's maximum would hide the absolute level,
/// which for ratings IS the reading). The strip *stacks* the two
/// segments, so the fixed scale is the stacked pair's
/// ([`STACKED_RATING_SCALE`]): `mean / 10` per field — a mean of 5 is
/// half the plot, and the pair's top edge reads `(mood + energy) / 10`.
/// An unrated bucket is `0.0` (the baseline tick); every fraction is
/// finite, the pair sums to ≤ 1.0, and the length equals the series'.
#[must_use]
pub fn review_rating_fractions(series: &[taskboard_domain::DayRatings]) -> Vec<ReflectionBar> {
    let fraction = |fold: &taskboard_domain::RatingMean| {
        fold.mean().map_or(0.0_f32, |mean| {
            (mean / STACKED_RATING_SCALE).clamp(0.0, 1.0)
        })
    };
    series
        .iter()
        .map(|day| ReflectionBar {
            mood: fraction(&day.ratings.mood),
            energy: fraction(&day.ratings.energy),
        })
        .collect()
}

/// The open/closed predicate: open iff the awaiting set is not empty.
#[must_use]
pub fn alarm_window_open(rows: &[AlarmRow]) -> bool {
    !rows.is_empty()
}

/// The attention edge: request when the set GAINED an occurrence (a
/// row identity absent from the previous pass). Identity is the
/// `(reminder, occurrence)` pair — the ladder memory's key.
#[must_use]
pub fn attention_due(rows: &[AlarmRow], previous: &[AlarmRow]) -> bool {
    rows.iter().any(|row| {
        !previous
            .iter()
            .any(|prev| prev.reminder == row.reminder && prev.occurrence == row.occurrence)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use taskboard_domain::{
        BoardClocks, Effort, ReminderOffset, ReminderRule, SidecarHealth, StackId, TaskClocks,
    };

    /// Local fixture clocks (the domain's `test_support` is
    /// feature-gated; these tests need only epoch stamps).
    fn epoch() -> DateTime<Utc> {
        taskboard_domain::BoardClocks::epoch()
    }

    #[test]
    fn the_delivery_chip_maps_degraded_health_to_visible() {
        let mut health = DeliveryHealth::default();
        // Healthy (never attempted): the chip is absent.
        assert!(delivery_chip(&health).is_none());
        // A failure after no success: degraded, visible, non-empty.
        health.last_attempt = Some(epoch());
        health.last_failure = Some(epoch());
        let chip = delivery_chip(&health).expect("degraded health shows the chip");
        assert_ne!(chip, "");
        // A later success clears it — strictly better delivery can
        // only clear the surface (the litmus rule).
        health.last_success = Some(epoch() + TimeDelta::seconds(1));
        assert!(delivery_chip(&health).is_none());
        // A failure NEWER than the last success degrades again.
        health.last_failure = Some(epoch() + TimeDelta::seconds(2));
        assert!(delivery_chip(&health).is_some());
    }

    fn task_clocks() -> TaskClocks {
        TaskClocks {
            title: epoch(),
            description: epoch(),
            duedate: epoch(),
            scheduled: epoch(),
            done: epoch(),
            position: epoch(),
            effort: epoch(),
            importance: epoch(),
            urgency: epoch(),
            recur: epoch(),
            kind: epoch(),

            archived: epoch(),
            deleted: epoch(),
        }
    }

    fn task(id: TaskId, stack: StackId, order: i64) -> Task {
        Task {
            id,
            remote: None,
            label_memberships: std::collections::BTreeMap::new(),
            title: format!("task-{order}"),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack,
            order,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,

            archived: false,
            deleted: false,
            clocks: task_clocks(),
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn stack(id: StackId, board: BoardId, order: i64) -> Stack {
        Stack {
            id,
            remote: None,
            board,
            title: format!("stack-{order}"),
            order,
            archived: false,
            deleted: false,
            clocks: taskboard_domain::StackClocks {
                title: epoch(),
                order: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::StackAuthors::default(),
            remote_seen: None,
        }
    }

    fn board(id: BoardId) -> Board {
        Board {
            id,
            remote: None,
            title: "board".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: epoch(),
                color: epoch(),
                archived: epoch(),
                deleted: epoch(),
            },
            authors: taskboard_domain::entities::BoardAuthors::default(),
            remote_seen: None,
        }
    }

    #[test]
    fn due_kind_follows_the_sets() {
        let now = Utc::now();
        let state = AppState::default();
        let sets = due_sets(&state, now, DUE_SOON_HORIZON);
        let mut overdue = task(
            TaskId::from_uuid(uuid::Uuid::now_v7()),
            StackId::from_uuid(uuid::Uuid::now_v7()),
            0,
        );
        overdue.duedate = Some(now - TimeDelta::hours(2));
        let mut done = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 1);
        done.done = Some(now);
        done.duedate = Some(now - TimeDelta::hours(5));

        let mut sets = sets;
        sets.overdue.insert(overdue.id);
        assert_eq!(due_kind(&overdue, &sets), DueKind::Overdue);
        assert_eq!(due_kind(&done, &sets), DueKind::Done, "done wins over sets");
        let undated = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 2);
        assert_eq!(due_kind(&undated, &sets), DueKind::None);
        let mut future = task(TaskId::from_uuid(uuid::Uuid::now_v7()), overdue.stack, 3);
        future.duedate = Some(now + TimeDelta::hours(72));
        assert_eq!(due_kind(&future, &sets), DueKind::Later);
    }

    #[test]
    fn duration_bucket_edges() {
        assert_eq!(duration_bucket(TimeDelta::seconds(59)), DurationBucket::Now);
        assert_eq!(
            duration_bucket(TimeDelta::seconds(60)),
            DurationBucket::Minutes(1)
        );
        assert_eq!(
            duration_bucket(TimeDelta::minutes(59)),
            DurationBucket::Minutes(59)
        );
        assert_eq!(
            duration_bucket(TimeDelta::minutes(60)),
            DurationBucket::Hours(1)
        );
        assert_eq!(
            duration_bucket(TimeDelta::hours(23)),
            DurationBucket::Hours(23)
        );
        assert_eq!(
            duration_bucket(TimeDelta::hours(24)),
            DurationBucket::Days(1)
        );
        assert_eq!(
            duration_bucket(-TimeDelta::hours(24)),
            DurationBucket::Now,
            "negative clamps"
        );
    }

    #[test]
    fn due_phrase_renders_when_dated_and_stays_empty_when_not() {
        // Structural facts only (AGENTS §5): the phrase assembly is
        // glue over `duration_bucket`, whose edges the tests above own
        // — no label-text assertions here.
        let now = Utc::now();
        assert!(
            due_phrase(DueKind::None, None, now, &Utc).is_empty(),
            "undated"
        );
        assert!(
            !due_phrase(DueKind::Later, Some(now + TimeDelta::hours(3)), now, &Utc).is_empty(),
            "dated future"
        );
        assert!(
            !due_phrase(DueKind::Overdue, Some(now - TimeDelta::hours(3)), now, &Utc).is_empty(),
            "dated past"
        );
    }

    #[test]
    fn sync_badge_follows_the_named_board() {
        // Two stored rows (Syncing on board 1, Offline on board 2,
        // Failed on board 3) plus derived pending ops attributed to
        // board 1 only: the badge reads the *named* board's row —
        // phase and pending both — not a fold.
        let b1 = BoardId::from_uuid(uuid::Uuid::from_u128(1));
        let b2 = BoardId::from_uuid(uuid::Uuid::from_u128(2));
        let b3 = BoardId::from_uuid(uuid::Uuid::from_u128(3));
        let mut states = taskboard_domain::SyncStates::new();
        states.set_stored(b1, SyncPhase::Syncing, None);
        states.set_stored(b2, SyncPhase::Offline, None);
        states.set_stored(
            b3,
            SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::Network,
            },
            None,
        );
        let mut state = AppState::default();
        let stack_id = StackId::from_uuid(uuid::Uuid::from_u128(2));
        let task_id = TaskId::from_uuid(uuid::Uuid::from_u128(3));
        state.boards.insert(b1, board(b1));
        state.boards.insert(b2, board(b2));
        state.boards.insert(b3, board(b3));
        state.stacks.insert(stack_id, stack(stack_id, b1, 0));
        state.tasks.insert(task_id, task(task_id, stack_id, 0));
        let ops: Vec<taskboard_domain::PendingOp> = (0..2)
            .map(|i| taskboard_domain::PendingOp {
                op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(10 + i)),
                op: taskboard_domain::LocalOp::UpdateTask(task_id),
                queued_at: chrono::Utc::now(),
            })
            .collect();
        taskboard_domain::derive_sync_states(
            &state.boards,
            &state.stacks,
            &state.tasks,
            &state.labels,
            &ops,
            &mut states,
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b1)),
            SyncBadge {
                phase: SyncPhaseUi::Syncing,
                pending: 2,
                sidecar: SidecarHealth::Healthy
            }
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b2)),
            SyncBadge {
                phase: SyncPhaseUi::Offline,
                pending: 0,
                sidecar: SidecarHealth::Healthy
            }
        );
        assert_eq!(
            sync_badge_for_board(&states, Some(b3)),
            SyncBadge {
                phase: SyncPhaseUi::Failed,
                pending: 0,
                sidecar: SidecarHealth::Healthy
            }
        );
        let default = SyncBadge {
            phase: SyncPhaseUi::Idle,
            pending: 0,
            sidecar: SidecarHealth::Healthy,
        };
        assert_eq!(sync_badge_for_board(&states, None), default, "no boards");
        let unknown = BoardId::from_uuid(uuid::Uuid::from_u128(0xFEED));
        assert_eq!(
            sync_badge_for_board(&states, Some(unknown)),
            default,
            "an unknown board reads the default row"
        );
    }

    #[test]
    fn stacks_in_board_filters_and_orders() {
        let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let s1 = StackId::from_uuid(uuid::Uuid::now_v7());
        let s2 = StackId::from_uuid(uuid::Uuid::now_v7());
        let s3 = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.boards.insert(b1, board(b1));
        // (order, id) order with a tie on order broken by id.
        state.stacks.insert(s2, stack(s2, b1, 1));
        state.stacks.insert(s1, stack(s1, b1, 1));
        state.stacks.insert(s3, stack(s3, b2, 0));
        let mut deleted = stack(s3, b1, 2);
        deleted.deleted = true;
        state.stacks.insert(s3, deleted);

        let ordered: Vec<StackId> = stacks_in_board(&state, b1)
            .into_iter()
            .map(|s| s.id)
            .collect();
        let mut expected = vec![s1, s2];
        expected.sort();
        assert_eq!(ordered, expected, "other boards and tombstones filtered");
    }

    #[test]
    fn live_board_finds_the_single_live_board() {
        let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        assert_eq!(live_board(&state), None);
        state.boards.insert(b1, board(b1));
        assert_eq!(live_board(&state).map(|b| b.id), Some(b1));
        let mut dead = board(b1);
        dead.deleted = true;
        state.boards.insert(b1, dead);
        state.boards.insert(b2, board(b2));
        let mut archived = board(b2);
        archived.archived = true;
        state.boards.insert(b2, archived);
        assert_eq!(live_board(&state), None, "deleted/archived boards skipped");
    }

    #[test]
    fn live_boards_filters_to_live_boards_in_canonical_order() {
        // Deterministic ids: live low + live high, one deleted and one
        // archived between them; the walk order is board-id ascending
        // (the `boards list` order) and `live_board` is its first.
        let low = BoardId::from_uuid(uuid::Uuid::from_u128(0x01));
        let dead = BoardId::from_uuid(uuid::Uuid::from_u128(0x02));
        let archived = BoardId::from_uuid(uuid::Uuid::from_u128(0x03));
        let high = BoardId::from_uuid(uuid::Uuid::from_u128(0x04));
        let mut state = AppState::default();
        state.boards.insert(high, board(high));
        state.boards.insert(archived, {
            let mut b = board(archived);
            b.archived = true;
            b
        });
        state.boards.insert(dead, {
            let mut b = board(dead);
            b.deleted = true;
            b
        });
        state.boards.insert(low, board(low));

        let ids: Vec<BoardId> = live_boards(&state).into_iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![low, high]);
        assert_eq!(live_board(&state).map(|b| b.id), Some(low));
    }

    #[test]
    fn banner_rows_cover_rule_and_ruleless_targets() {
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let ruled = TaskId::from_uuid(uuid::Uuid::now_v7());
        let ruleless = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();

        let mut t1 = task(ruled, stack, 0);
        t1.duedate = Some(now - TimeDelta::hours(1));
        let rule = ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7()),
            task: ruled,
            kind: taskboard_domain::ReminderKind::DueOffset(
                ReminderOffset::before(TimeDelta::minutes(-30)).expect("valid offset"),
            ),
            created_at: now,
            updated_at: now,
        };
        state.reminders.rules.insert(rule.id, rule);
        let mut t2 = task(ruleless, stack, 1);
        t2.duedate = Some(now - TimeDelta::hours(2));
        state.tasks.insert(ruled, t1);
        state.tasks.insert(ruleless, t2);

        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].task, ruled, "canonical (order, id) order");
        assert_eq!(
            rows[0].target.reminder,
            ReminderRef::Rule(state.reminders.rules.values().next().expect("rule row").id)
        );
        assert_eq!(
            rows[0].target.occurrence,
            now - TimeDelta::hours(1) - TimeDelta::minutes(30)
        );
        assert_eq!(rows[1].target.reminder, ReminderRef::Default(ruleless));
        assert_eq!(rows[1].target.occurrence, now - TimeDelta::hours(2));

        // Banner truth ignores mute: a muted task still renders.
        state.reminders.muted.insert(ruleless);
        assert_eq!(banner_rows(&state, now).len(), 2);

        // Done tasks drop out.
        let mut t1 = state.tasks.get(&ruled).expect("task").clone();
        t1.done = Some(now);
        state.tasks.insert(ruled, t1);
        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task, ruleless);
    }

    /// Chunk-7 anchor pins (D14): alarm rows never surface — a banner
    /// target is always Task-anchored even when the state carries
    /// alarms (the #352 no-UI ruling), and a state with alarms but no
    /// overdue tasks renders zero banner rows.
    #[test]
    fn banner_targets_stay_task_anchored_when_alarms_exist() {
        use taskboard_domain::{AdhocAlarm, AdhocAlarmId, ReminderAnchor};
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.reminders.alarms.insert(
            AdhocAlarmId::from_uuid(uuid::Uuid::now_v7()),
            AdhocAlarm {
                insistence: taskboard_domain::Insistence::Normal,
                id: AdhocAlarmId::from_uuid(uuid::Uuid::now_v7()),
                label: "steep the tea".into(),
                fire_at: now - TimeDelta::hours(1), // overdue as an alarm
                created_at: now,
            },
        );
        assert!(
            banner_rows(&state, now).is_empty(),
            "alarms are not banner rows"
        );

        // With an overdue task too: every target is Task-anchored.
        let task_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut t = task(task_id, stack, 0);
        t.duedate = Some(now - TimeDelta::hours(2));
        state.tasks.insert(task_id, t);
        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].target.anchor,
            ReminderAnchor::Task(task_id),
            "banner targets carry Task anchors only"
        );
    }

    #[test]
    fn tracker_rows_render_taskless_with_the_habit_title() {
        use taskboard_domain::ReminderDelivery;
        use taskboard_domain::habit::{Habit, HabitClocks, HabitTarget, NonZeroCount};
        // Late-evening fixed instant: the day's anchor reading (09:00,
        // midnight day start) is already behind it — deterministic, no
        // wall-clock coupling.
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("valid ts");
        let habit_id = taskboard_domain::HabitId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state.habits.habits.insert(
            habit_id,
            Habit {
                id: habit_id,
                board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
                title: "stretch".into(),
                target: HabitTarget::PerDay(NonZeroCount::new(1).unwrap()),
                order: 0,
                anchor_time: Some(chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap()),
                clocks: HabitClocks::all(now),
                continued_from: None,
                deleted: false,
            },
        );
        // The delivered key is the window's anchor reading (the same
        // derivation the evaluator mints), not an arbitrary stamp.
        let occurrence = calendar().instant(
            calendar().today(now),
            chrono::NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
        );
        state.reminders.deliveries.insert(ReminderDelivery {
            reminder: ReminderRef::Tracker(habit_id),
            occurrence,
            delivered_at: occurrence,
        });
        let rows = notification_rows(&state, &defaults(), calendar(), now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].task, None, "a tracker row has no Done/Mute target");
        assert_eq!(rows[0].title, "stretch", "the title snapshot is carried");
        assert_eq!(rows[0].reminder, ReminderRef::Tracker(habit_id));
        assert_eq!(rows[0].occurrence, occurrence);
        // The affordance table: dismiss + snooze, never Done/Mute/Start.
        let buttons = awaiting_row_buttons(taskboard_domain::ReminderAnchor::Tracker(habit_id));
        assert!(buttons.dismiss && buttons.snooze);
        assert!(!buttons.done && !buttons.mute && !buttons.track_start);
    }

    #[test]
    fn notification_rows_follow_the_domain_view_and_filter_the_anchor() {
        use taskboard_domain::ReminderDelivery;
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let ruled = TaskId::from_uuid(uuid::Uuid::now_v7());
        let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut t = task(ruled, stack, 0);
        t.duedate = Some(now - TimeDelta::hours(1));
        state.tasks.insert(ruled, t);
        state.reminders.rules.insert(
            rule_id,
            taskboard_domain::ReminderRule {
                insistence: taskboard_domain::Insistence::Normal,
                id: rule_id,
                task: ruled,
                kind: taskboard_domain::ReminderKind::DueOffset(ReminderOffset::at_due()),
                created_at: now,
                updated_at: now,
            },
        );
        // No delivery yet: not active (the surface is fired ∧ not
        // dismissed — the domain view's contract, not a UI rule).
        assert_eq!(
            notification_rows(&state, &defaults(), calendar(), now),
            [] as [NotificationRow; 0]
        );

        let occurrence = now - TimeDelta::hours(1);
        state.reminders.deliveries.insert(ReminderDelivery {
            reminder: ReminderRef::Rule(rule_id),
            occurrence,
            delivered_at: now,
        });
        let rows = notification_rows(&state, &defaults(), calendar(), now);
        assert_eq!(
            rows,
            vec![NotificationRow {
                task: Some(ruled),
                reminder: ReminderRef::Rule(rule_id),
                occurrence,
                title: state
                    .tasks
                    .get(&ruled)
                    .map(|t| t.title.clone())
                    .unwrap_or_default(),
            }]
        );

        // Dismissed: gone from the surface, permanently.
        state
            .reminders
            .dismissals
            .insert(taskboard_domain::ReminderDismissal {
                reminder: ReminderRef::Rule(rule_id),
                occurrence,
                dismissed_at: now,
            });
        assert_eq!(
            notification_rows(&state, &defaults(), calendar(), now),
            [] as [NotificationRow; 0]
        );
    }

    /// A break row is shown, carries no task target (no Done/Mute
    /// buttons), and its title is the domain-assembled rule-label
    /// snapshot (D-18 — carried, never re-derived).
    #[test]
    fn notification_rows_render_a_break_row_without_a_task_target() {
        use taskboard_domain::{
            BreakRule, BreakRuleId, Insistence, ReminderDelivery, WorkThreshold,
        };
        let now = Utc::now();
        let mut state = AppState::default();
        let rule = BreakRule {
            id: BreakRuleId::from_uuid(uuid::Uuid::now_v7()),
            board: taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7()),
            label: "time for a break".to_owned(),
            threshold: WorkThreshold::try_new(TimeDelta::try_minutes(90).unwrap())
                .expect("positive threshold"),
            insistence: Insistence::Alarm,
            created_at: now,
        };
        let reminder = ReminderRef::Break(rule.id);
        // The crossing instant: the span opened 200m ago, the first
        // 90m mark was touched 110m ago — that instant is the
        // occurrence (D-8), so the delivery receipt keys on it.
        let occurrence = now - TimeDelta::minutes(110);
        state.reminders.break_rules.insert(rule.id, rule);
        // 90m of unbroken tracked work in the history — the crossing is
        // derived from the entries at read time.
        let entry_id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            entry_id,
            taskboard_domain::TimeEntry {
                id: entry_id,
                task: TaskId::from_uuid(uuid::Uuid::now_v7()),
                task_title: "work".to_owned(),
                start: now - TimeDelta::minutes(200),
                end: Some(now - TimeDelta::minutes(20)),
                created_at: now - TimeDelta::minutes(200),
                updated_at: now - TimeDelta::minutes(20),
                kind: taskboard_domain::TimeEntryKind::Work,
            },
        );
        state.reminders.deliveries.insert(ReminderDelivery {
            reminder,
            occurrence,
            delivered_at: now,
        });
        let rows = notification_rows(&state, &defaults(), calendar(), now);
        assert_eq!(rows.len(), 1, "a break row is shown (no card to filter)");
        assert_eq!(rows[0].task, None, "no Done/Mute target");
        assert_eq!(rows[0].title, "time for a break", "the label snapshot");
        assert_eq!(rows[0].reminder, reminder);
        assert_eq!(rows[0].occurrence, occurrence);
        // Dismissed: gone, the house law.
        state
            .reminders
            .dismissals
            .insert(taskboard_domain::ReminderDismissal {
                reminder,
                occurrence,
                dismissed_at: now,
            });
        assert_eq!(
            notification_rows(&state, &defaults(), calendar(), now),
            [] as [NotificationRow; 0]
        );
    }

    /// The anchor card never renders as a notification row (DDF-5, the
    /// banner precedent).
    #[test]
    fn notification_rows_exclude_the_anchor() {
        use taskboard_domain::ReminderDelivery;
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let anchor = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut t = task(anchor, stack, 0);
        t.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
        t.duedate = Some(now - TimeDelta::hours(1));
        state.tasks.insert(anchor, t);
        state.reminders.deliveries.insert(ReminderDelivery {
            reminder: ReminderRef::Default(anchor),
            occurrence: now - TimeDelta::hours(1),
            delivered_at: now,
        });
        assert_eq!(
            notification_rows(&state, &defaults(), calendar(), now),
            [] as [NotificationRow; 0]
        );
    }

    // -- The timers surface (DR-3/3, D-I6–D-I8) -------------------------

    fn alarm(
        id: taskboard_domain::AdhocAlarmId,
        label: &str,
        fire_at: DateTime<Utc>,
    ) -> taskboard_domain::AdhocAlarm {
        taskboard_domain::AdhocAlarm {
            insistence: taskboard_domain::Insistence::Normal,
            id,
            label: label.to_owned(),
            fire_at,
            created_at: fire_at - TimeDelta::hours(1),
        }
    }

    fn deliver_alarm(state: &mut AppState, id: taskboard_domain::AdhocAlarmId, at: DateTime<Utc>) {
        state
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: ReminderRef::Adhoc(id),
                occurrence: at,
                delivered_at: at,
            });
    }

    /// A structurally DST-refusing zone (the constructed
    /// ambiguity): every local datetime folds to `None` — the
    /// `.single()` discipline's worst case, deterministic without a
    /// tz database.
    #[derive(Clone)]
    struct RefusingZone;

    impl chrono::TimeZone for RefusingZone {
        type Offset = chrono::FixedOffset;
        fn offset_from_utc_datetime(&self, _utc: &chrono::NaiveDateTime) -> Self::Offset {
            chrono::FixedOffset::east_opt(0).expect("zero")
        }
        fn offset_from_utc_date(&self, _date: &chrono::NaiveDate) -> Self::Offset {
            chrono::FixedOffset::east_opt(0).expect("zero")
        }
        fn from_offset(_offset: &Self::Offset) -> Self {
            RefusingZone
        }
        fn offset_from_local_datetime(
            &self,
            _local: &chrono::NaiveDateTime,
        ) -> chrono::LocalResult<Self::Offset> {
            chrono::LocalResult::None
        }
        fn offset_from_local_date(
            &self,
            _date: &chrono::NaiveDate,
        ) -> chrono::LocalResult<Self::Offset> {
            chrono::LocalResult::None
        }
    }

    #[test]
    fn preset_when_text_speaks_the_pinned_grammar() {
        for minutes in TIMER_PRESETS_MIN {
            let text = preset_when_text(minutes);
            assert!(
                text.starts_with("in "),
                "the preset rides the in-grammar ({minutes}m)"
            );
            assert!(
                taskboard_domain::ReminderOffset::parse_text_form(&text["in ".len()..]).is_some(),
                "the preset re-parses through the domain grammar ({minutes}m)"
            );
        }
        assert_eq!(preset_when_text(60), "in 1h", "the hour form at 60");
        assert_eq!(preset_when_text(25), "in 25m");
        assert_eq!(
            TIMER_PRESETS_MIN,
            [1, 5, 10, 25, 60],
            "the D33-pinned ladder"
        );
    }

    #[test]
    fn compose_timer_at_round_trips_through_the_one_parser() {
        let now = Utc
            .with_ymd_and_hms(2030, 4, 1, 10, 0, 0)
            .single()
            .expect("now");
        // Today's remaining pick resolves to today's instant.
        let today_text = compose_timer_at(9 * 60 + 30, 0, now, &Utc).expect("composes");
        let today_when = parse_timer_when(&today_text, now, &Utc);
        assert_eq!(
            today_when,
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2030, 4, 1, 9, 30, 0)
                    .single()
                    .expect("t")
            )),
            "today pick composes the exact instant ({today_text})"
        );
        // Tomorrow's pick is +1 day's instant — an explicit day is
        // honored, no roll.
        let tomorrow_text = compose_timer_at(9 * 60 + 30, 1, now, &Utc).expect("composes");
        assert_eq!(
            parse_timer_when(&tomorrow_text, now, &Utc),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2030, 4, 2, 9, 30, 0)
                    .single()
                    .expect("t")
            )),
            "tomorrow pick composes the +1-day instant"
        );
        // A past-today pick is still Some — an explicit choice is
        // honored (the At semantics fire next tick).
        assert!(
            compose_timer_at(8 * 60, 0, now, &Utc).is_some(),
            "a past-today pick composes — no silent day flip"
        );
    }

    #[test]
    fn compose_timer_at_folds_a_dst_gap_to_none() {
        let now = Utc
            .with_ymd_and_hms(2030, 4, 1, 10, 0, 0)
            .single()
            .expect("now");
        assert_eq!(
            compose_timer_at(9 * 60 + 30, 0, now, &RefusingZone),
            None,
            "the unresolvable local pick folds to None — never a guessed instant"
        );
    }

    #[test]
    fn timer_rows_carry_the_insistence_rung_verbatim() {
        let now = Utc::now();
        let normal_id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let alarm_id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut escalation = alarm(alarm_id, "wake", now + TimeDelta::minutes(5));
        escalation.insistence = taskboard_domain::Insistence::Alarm;
        state.reminders.alarms.insert(
            normal_id,
            alarm(normal_id, "tea", now + TimeDelta::minutes(3)),
        );
        state.reminders.alarms.insert(alarm_id, escalation);
        let rows = timer_rows(&state, now);
        assert_eq!(
            rows.iter()
                .map(|row| (row.insistence, row.alarm))
                .collect::<Vec<_>>(),
            vec![
                (taskboard_domain::Insistence::Normal, normal_id),
                (taskboard_domain::Insistence::Alarm, alarm_id),
            ],
            "the rung rides the projection verbatim — no defaulting"
        );
    }

    #[test]
    fn timer_rows_order_by_the_walked_occurrence_and_id() {
        let now = Utc::now();
        let early = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let late = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        // `early` fires sooner by base `fire_at` but a snooze chain
        // walks it past `late`'s — the *walked* occurrence orders.
        state
            .reminders
            .alarms
            .insert(early, alarm(early, "early", now + TimeDelta::hours(1)));
        state
            .reminders
            .alarms
            .insert(late, alarm(late, "late", now + TimeDelta::hours(2)));
        state.reminders.snoozes.insert(
            taskboard_domain::ReminderSnoozeKey {
                reminder: ReminderRef::Adhoc(early),
                occurrence: now + TimeDelta::hours(1),
            },
            now + TimeDelta::hours(3),
        );
        let rows = timer_rows(&state, now);
        assert_eq!(
            rows.iter().map(|r| r.alarm).collect::<Vec<_>>(),
            vec![late, early],
            "the snoozed alarm's walked occurrence orders it last"
        );

        // A tie on the walked occurrence breaks by id.
        let a = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let b = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let mut state = AppState::default();
        state
            .reminders
            .alarms
            .insert(a, alarm(a, "a", now + TimeDelta::minutes(5)));
        state
            .reminders
            .alarms
            .insert(b, alarm(b, "b", now + TimeDelta::minutes(5)));
        let rows = timer_rows(&state, now);
        assert_eq!(
            rows.iter().map(|r| r.alarm).collect::<Vec<_>>(),
            vec![lo, hi]
        );
    }

    #[test]
    fn timer_rows_split_fired_from_awaiting_by_derivation() {
        let now = Utc::now();
        let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state
            .reminders
            .alarms
            .insert(id, alarm(id, "tea", now - TimeDelta::minutes(1)));
        // The fired-but-undelivered tick-boundary transient stays a row
        // (totality, D-P17); the label is carried verbatim.
        let rows = timer_rows(&state, now);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].alarm, id);
        assert_eq!(rows[0].label, "tea");

        // Delivered (awaiting): the row belongs to the notification
        // surface, never duplicated in the timers section.
        deliver_alarm(&mut state, id, now - TimeDelta::minutes(1));
        assert_eq!(timer_rows(&state, now), [] as [TimerRow; 0]);
    }

    #[test]
    fn awaiting_alarm_count_follows_the_fire_snooze_refire_script() {
        let now = Utc::now();
        let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let base = now - TimeDelta::minutes(5);
        let mut state = AppState::default();
        state.reminders.alarms.insert(id, alarm(id, "tea", base));

        // Not yet delivered: not awaiting.
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), now),
            0
        );

        // Fired + delivered: one awaiting row.
        deliver_alarm(&mut state, id, base);
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), now),
            1
        );

        // Snoozed to a future target: the delivery keys the old
        // occurrence, the effective walked — the row leaves by
        // derivation.
        state.reminders.snoozes.insert(
            taskboard_domain::ReminderSnoozeKey {
                reminder: ReminderRef::Adhoc(id),
                occurrence: base,
            },
            now + TimeDelta::minutes(10),
        );
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), now),
            0
        );

        // Delivered at the new target: awaiting again at that target.
        deliver_alarm(&mut state, id, now + TimeDelta::minutes(10));
        let later = now + TimeDelta::minutes(11);
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), later),
            1
        );

        // Dismissed at the effective: gone for good.
        state
            .reminders
            .dismissals
            .insert(taskboard_domain::ReminderDismissal {
                reminder: ReminderRef::Adhoc(id),
                occurrence: now + TimeDelta::minutes(10),
                dismissed_at: later,
            });
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), later),
            0
        );
    }

    #[test]
    fn awaiting_alarm_counts_task_rows_never() {
        let now = Utc::now();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let ruled = TaskId::from_uuid(uuid::Uuid::now_v7());
        let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut t = task(ruled, stack, 0);
        t.duedate = Some(now - TimeDelta::hours(1));
        state.tasks.insert(ruled, t);
        state.reminders.rules.insert(
            rule_id,
            taskboard_domain::ReminderRule {
                insistence: taskboard_domain::Insistence::Normal,
                id: rule_id,
                task: ruled,
                kind: taskboard_domain::ReminderKind::DueOffset(ReminderOffset::at_due()),
                created_at: now,
                updated_at: now,
            },
        );
        state
            .reminders
            .deliveries
            .insert(taskboard_domain::ReminderDelivery {
                reminder: ReminderRef::Rule(rule_id),
                occurrence: now - TimeDelta::hours(1),
                delivered_at: now,
            });
        assert_eq!(
            notification_rows(&state, &defaults(), calendar(), now).len(),
            1,
            "the task row renders on the surface"
        );
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), calendar(), now),
            0,
            "task-anchored rows never count as awaiting alarms"
        );
    }

    #[test]
    fn timer_chip_hides_while_empty_and_names_the_nearest() {
        let now = Utc::now();
        let state = AppState::default();
        assert_eq!(timer_chip(&state), None, "no live alarm, no chip");

        let near = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let far = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        state
            .reminders
            .alarms
            .insert(far, alarm(far, "far", now + TimeDelta::hours(2)));
        state
            .reminders
            .alarms
            .insert(near, alarm(near, "near", now + TimeDelta::minutes(2)));
        assert_eq!(
            timer_chip(&state),
            Some(TimerChip {
                alarm: near,
                label: "near".to_owned(),
                effective: now + TimeDelta::minutes(2),
            })
        );

        // An awaiting-only map still yields a chip (the chip says
        // "now"; the surface owns the row).
        state.reminders.alarms.clear();
        let fired = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        state
            .reminders
            .alarms
            .insert(fired, alarm(fired, "fired", now - TimeDelta::minutes(1)));
        assert_eq!(
            timer_chip(&state),
            Some(TimerChip {
                alarm: fired,
                label: "fired".to_owned(),
                effective: now - TimeDelta::minutes(1),
            })
        );
    }

    /// A closed-or-open time entry fixture (DR-5/4): planner-shaped —
    /// stamps from the same instant, never inverted.
    fn entry(
        id: taskboard_domain::TimeEntryId,
        task: TaskId,
        title: &str,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
    ) -> taskboard_domain::TimeEntry {
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: title.to_owned(),
            start,
            end,
            created_at: start,
            updated_at: end.unwrap_or(start),
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        }
    }

    /// DR-5/4 — the chip's pick: hidden while the tracking map holds
    /// only closed rows, naming the `newest_open` entry (with the
    /// domain's id tie-break) otherwise, carrying the title snapshot.
    #[test]
    fn tracking_chip_picks_the_newest_open_entry() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let state = AppState::default();
        assert_eq!(tracking_chip(&state), None, "no entries, no chip");

        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        // A closed row never chips.
        let closed = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            closed,
            entry(closed, task, "closed", now - TimeDelta::hours(2), Some(now)),
        );
        assert_eq!(tracking_chip(&state), None, "closed-only, no chip");

        let early = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        let late = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        let tie_low = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::from_u128(1));
        let tie_high = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::from_u128(2));
        state.tracking.entries.insert(
            early,
            entry(early, task, "early", now - TimeDelta::minutes(30), None),
        );
        state.tracking.entries.insert(
            late,
            entry(late, task, "late", now - TimeDelta::minutes(5), None),
        );
        assert_eq!(
            tracking_chip(&state),
            Some(TrackingChip {
                task,
                title: "late".to_owned(),
                start: now - TimeDelta::minutes(5),
            }),
            "the most-recently-started open entry"
        );

        // The same-start tie-break: the greater entry id wins, and the
        // chip and the shared open-task pick agree.
        state.tracking.entries.remove(&late);
        state.tracking.entries.insert(
            tie_low,
            entry(tie_low, task, "tie-low", now - TimeDelta::minutes(5), None),
        );
        state.tracking.entries.insert(
            tie_high,
            entry(
                tie_high,
                task,
                "tie-high",
                now - TimeDelta::minutes(5),
                None,
            ),
        );
        assert_eq!(
            tracking_open_task(&state),
            Some(task),
            "one shared derivation for chip and badge"
        );
        assert_eq!(
            tracking_chip(&state).map(|chip| chip.title),
            Some("tie-high".to_owned()),
            "the id tie-break mirrors the domain's newest_open"
        );
    }

    /// DR-5/4 — the elapsed phrase's minute buckets (D-T17): the
    /// sub-minute bucket renders the honest zero, the edges follow
    /// `duration_bucket`'s tested contract, and a skew-produced
    /// inverted span clamps to the zero.
    #[test]
    fn open_elapsed_phrase_buckets_the_past_direction() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        assert_eq!(open_elapsed_phrase(now, now), "0m");
        assert_eq!(open_elapsed_phrase(now - TimeDelta::seconds(59), now), "0m");
        assert_eq!(
            open_elapsed_phrase(now - TimeDelta::minutes(12), now),
            "12m"
        );
        assert_eq!(open_elapsed_phrase(now - TimeDelta::minutes(60), now), "1h");
        assert_eq!(open_elapsed_phrase(now - TimeDelta::hours(25), now), "1d");
        assert_eq!(
            open_elapsed_phrase(now + TimeDelta::minutes(5), now),
            "0m",
            "an inverted span contributes nothing"
        );
    }

    /// DR-5/4 — the kiosk gate (D-T16): the gate empties the ACTION;
    /// the display decision is not this function's.
    #[test]
    fn the_kiosk_gate_empties_the_action_only() {
        assert!(tracking_chip_action_visible(false));
        assert!(!tracking_chip_action_visible(true));
    }

    /// DR-5/4 — the badge's visibility: exactly the open entry's card.
    #[test]
    fn the_tracking_badge_visible_only_on_the_open_entrys_card() {
        let tracked = TaskId::from_uuid(uuid::Uuid::now_v7());
        let other = TaskId::from_uuid(uuid::Uuid::now_v7());
        assert!(tracking_badge_visible(Some(tracked), tracked));
        assert!(!tracking_badge_visible(Some(tracked), other));
        assert!(
            !tracking_badge_visible(None, tracked),
            "nothing open, no badge"
        );
    }

    /// DR-5/4 — the detail verb: Stop while the subject holds the
    /// open entry, Start otherwise — including a completed subject
    /// (R-8 is the engine's ruling, not a UI gate), and `None` with
    /// no subject.
    #[test]
    fn detail_tracking_verb_follows_the_shared_pick_without_a_done_gate() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let tracked = TaskId::from_uuid(uuid::Uuid::now_v7());
        let other = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let open = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state.tracking.entries.insert(
            open,
            entry(open, tracked, "tracked", now - TimeDelta::minutes(5), None),
        );

        assert_eq!(detail_tracking_verb(&state, None), None);
        assert_eq!(
            detail_tracking_verb(&state, Some(tracked)),
            Some(TrackDetail::Stop)
        );
        // The verb reads only the open-entry pick — a subject without
        // one (fresh or completed alike) is Start; R-8's done-gate is
        // the engine's ruling, never a UI gate.
        assert_eq!(
            detail_tracking_verb(&state, Some(other)),
            Some(TrackDetail::Start)
        );
    }

    /// r4 D17 — the summary reduction sums only the subject's entries;
    /// another task's history is excluded.
    #[test]
    fn tracked_total_applies_the_work_filter() {
        // The D-11 ruling (DR-5): respite counts against the work
        // streak, never toward effort — the detail view's total is
        // Work-only, matching `work_in_window`'s semantics.
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mint = |kind| {
            let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
            (
                id,
                taskboard_domain::TimeEntry {
                    kind,
                    ..entry(id, subject, "t", now - TimeDelta::minutes(30), Some(now))
                },
            )
        };
        let (work_id, work) = mint(taskboard_domain::tracking::TimeEntryKind::Work);
        state.tracking.entries.insert(work_id, work);
        let (respite_id, respite) = mint(taskboard_domain::tracking::TimeEntryKind::Respite);
        state.tracking.entries.insert(respite_id, respite);

        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal {
                hours: 0,
                minutes: 30,
                seconds: 0,
            },
            "the respite entry never inflates the total"
        );
    }

    #[test]
    fn tracked_total_sums_only_the_subjects_entries() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let other = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mint = |task: TaskId, start: DateTime<Utc>, end: Option<DateTime<Utc>>| {
            let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
            (id, entry(id, task, "t", start, end))
        };
        let (id, e) = mint(
            subject,
            now - TimeDelta::minutes(30),
            Some(now - TimeDelta::minutes(10)),
        );
        state.tracking.entries.insert(id, e);
        let (id, e) = mint(other, now - TimeDelta::hours(5), Some(now));
        state.tracking.entries.insert(id, e);

        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal {
                hours: 0,
                minutes: 20,
                seconds: 0,
            },
            "the other task's 5h entry is excluded"
        );
    }

    /// r4 D17 — an open entry's elapsed-to-now lands in the parts
    /// (the running timer's accrual).
    #[test]
    fn tracked_total_includes_an_open_entrys_elapsed() {
        let start = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let now = start + TimeDelta::seconds(90);
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        state
            .tracking
            .entries
            .insert(id, entry(id, subject, "t", start, None));

        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal {
                hours: 0,
                minutes: 1,
                seconds: 30,
            },
        );
    }

    /// r4 D17 — a degenerate/inverted span contributes zero (the
    /// domain clamp, pinned through the helper); the total is never
    /// negative.
    #[test]
    fn tracked_total_never_goes_negative_on_a_degenerate_span() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
        // Inverted: start after the claimed end.
        state.tracking.entries.insert(
            id,
            entry(id, subject, "t", now, Some(now - TimeDelta::hours(1))),
        );

        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal::default(),
            "the clamp pins the span at zero"
        );
    }

    /// r4 D17 — no entries at all is the honest zero.
    #[test]
    fn tracked_total_without_entries_is_the_zero() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let state = AppState::default();
        assert_eq!(tracked_total(&state, subject, now), TrackedTotal::default());
    }

    /// r4 D17 — the second-precision contract, numerically exact:
    /// seconds survive the minute rounding.
    #[test]
    fn tracked_total_reduces_with_second_precision() {
        let now = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
        let subject = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mint = |start: DateTime<Utc>, end: Option<DateTime<Utc>>| {
            let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
            (id, entry(id, subject, "t", start, end))
        };
        // 3661 s = 1h 1m 1s.
        let (id, e) = mint(now - TimeDelta::seconds(3661), Some(now));
        state.tracking.entries.insert(id, e);
        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal {
                hours: 1,
                minutes: 1,
                seconds: 1,
            },
        );

        // 59 s stays seconds; 90 s → 1m 30s.
        let mut state = AppState::default();
        let (id, e) = mint(now - TimeDelta::seconds(59), Some(now));
        state.tracking.entries.insert(id, e);
        let (id, e) = mint(
            now - TimeDelta::seconds(90),
            Some(now - TimeDelta::seconds(59)),
        );
        state.tracking.entries.insert(id, e);
        assert_eq!(
            tracked_total(&state, subject, now),
            TrackedTotal {
                hours: 0,
                minutes: 1,
                seconds: 30,
            },
        );
    }

    /// r4 D17 — the phrase glue: structural only (the numeric parts
    /// carry the precision contract). Deterministic, non-empty for
    /// every triple, the zero rendering distinct from 1 s, and the
    /// segment count grows as parts enter.
    #[test]
    fn tracked_total_phrase_glue_is_deterministic_and_zero_canonical() {
        let zero = TrackedTotal::default();
        let one_second = TrackedTotal {
            hours: 0,
            minutes: 0,
            seconds: 1,
        };
        assert_eq!(tracked_total_phrase(&zero), tracked_total_phrase(&zero));
        assert_ne!(
            tracked_total_phrase(&zero),
            tracked_total_phrase(&one_second),
            "the canonical zero is distinct from one second"
        );
        for total in [
            zero,
            one_second,
            TrackedTotal {
                hours: 0,
                minutes: 2,
                seconds: 5,
            },
            TrackedTotal {
                hours: 1,
                minutes: 23,
                seconds: 45,
            },
        ] {
            let phrase = tracked_total_phrase(&total);
            assert_ne!(phrase, "", "every part triple renders non-empty");
            assert_eq!(
                tracked_total_phrase(&total),
                phrase,
                "the glue is deterministic"
            );
            assert!(
                phrase.ends_with('s'),
                "seconds always render (structural tail)"
            );
        }
        // Segment count grows as parts enter (structural, not textual).
        let segments = |phrase: &str| phrase.split(' ').count();
        assert_eq!(
            segments(&tracked_total_phrase(&one_second)),
            1,
            "sub-minute renders one segment"
        );
        assert_eq!(
            segments(&tracked_total_phrase(&TrackedTotal {
                hours: 0,
                minutes: 2,
                seconds: 5,
            })),
            2,
            "a minute enters a segment"
        );
        assert_eq!(
            segments(&tracked_total_phrase(&TrackedTotal {
                hours: 1,
                minutes: 23,
                seconds: 45,
            })),
            3,
            "an hour enters a segment"
        );
    }

    /// DR-5/4 — the anchor table grows `track_start`: task rows only;
    /// an Adhoc row never renders Start Task.
    #[test]
    fn awaiting_row_buttons_offer_track_start_on_task_anchors_only() {
        let task = TaskId::from_uuid(uuid::Uuid::now_v7());
        let alarm = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let task_row = awaiting_row_buttons(taskboard_domain::ReminderAnchor::Task(task));
        assert!(task_row.track_start);
        let alarm_row = awaiting_row_buttons(taskboard_domain::ReminderAnchor::Alarm(alarm));
        assert!(!alarm_row.track_start);
    }

    #[test]
    fn offset_validity_follows_the_domain_parser() {
        // (r4 D27) Tri-state: empty neutral, the domain grammar's
        // forms valid, junk invalid — the predicate calls the
        // authoritative parser.
        assert_eq!(offset_validity(""), None);
        assert_eq!(offset_validity("   "), None);
        assert_eq!(offset_validity("45m"), Some(true));
        assert_eq!(offset_validity("3h"), Some(true));
        assert_eq!(offset_validity("2d"), Some(true));
        // The domain grammar's units are s | m | h | d — weeks are
        // not the offset grammar (the due relative arm's `+1w` is a
        // different grammar; recorded so the hints stay honest).
        assert_eq!(offset_validity("1w"), Some(false));
        assert_eq!(offset_validity("30s"), Some(true));
        assert_eq!(offset_validity("soon"), Some(false));
        assert_eq!(offset_validity("45x"), Some(false));
    }

    #[test]
    fn rule_list_hint_names_the_muted_state() {
        // Rule-less: empty regardless of mute (nothing to hint).
        assert_eq!(rule_list_hint(0, false), "");
        assert_eq!(rule_list_hint(0, true), "");
        // Armed rules: the derived-fact hint.
        assert_ne!(rule_list_hint(2, false), "");
        // Muted: the indicator is the hint (F25's ranked cause made
        // visible — no silent staleness).
        assert_ne!(rule_list_hint(2, true), "");
        assert_ne!(rule_list_hint(2, true), rule_list_hint(2, false));
    }

    #[test]
    fn countdown_parts_decompose_clamp_and_roundtrip() {
        use super::countdown_parts;
        let now = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
        let effective = Utc.with_ymd_and_hms(2026, 11, 5, 11, 3, 41).unwrap();
        let parts = countdown_parts(effective, now);
        // 2d 2h 3m 41s.
        assert_eq!(
            (parts.days, parts.hours, parts.minutes, parts.seconds),
            (2, 2, 3, 41),
            "the d/h/m/s decomposition"
        );
        assert_eq!(
            parts.total_seconds(),
            (effective - now).num_seconds(),
            "round-trip"
        );
        // The fired-undelivered transient clamps at zero.
        let past = countdown_parts(now - TimeDelta::hours(3), now);
        assert_eq!(
            (past.days, past.hours, past.minutes, past.seconds),
            (0, 0, 0, 0)
        );
        // Segment counts stay below each unit's ceiling by construction.
        let parts2 = countdown_parts(now + TimeDelta::seconds(59), now);
        assert_eq!(
            (parts2.days, parts2.hours, parts2.minutes, parts2.seconds),
            (0, 0, 0, 59)
        );
    }

    #[test]
    fn iso_local_fire_time_roundtrips_in_the_injected_tz() {
        use super::iso_local_fire_time;
        let instant = Utc.with_ymd_and_hms(2026, 11, 2, 9, 30, 0).unwrap();
        // A fixed offset: the local wall form re-parses to the same
        // instant (structural round-trip, not a label pin).
        let tz = chrono::FixedOffset::east_opt(2 * 3_600).unwrap();
        let text = iso_local_fire_time(instant, &tz);
        let parsed =
            chrono::NaiveDateTime::parse_from_str(&text, "%Y-%m-%d %H:%M:%S").expect("parses");
        let back = tz.from_local_datetime(&parsed).single().expect("resolves");
        assert_eq!(
            back.with_timezone(&Utc),
            instant,
            "the local form round-trips"
        );
        // The +02 wall clock is two hours ahead of UTC.
        assert_eq!(text, "2026-11-02 11:30:00");
    }

    #[test]
    fn rule_row_texts_project_per_kind_and_derive_armed() {
        use super::{RULE_ROW_UNARMED, countdown_text, iso_local_fire_time, rule_row_texts};
        let now = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        let effective = Utc.with_ymd_and_hms(2026, 11, 3, 11, 3, 41).unwrap();
        let task_id = taskboard_domain::TaskId::from_uuid(uuid::Uuid::from_u128(2));
        // An At row: the countdown + the fixed local fire time, always
        // armed (no due date involved).
        let at_rule = taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(1)),
            task: task_id,
            kind: taskboard_domain::ReminderKind::At(
                taskboard_domain::AtInstant::whole_second(effective).unwrap(),
            ),
            created_at: now,
            updated_at: now,
        };
        let texts = rule_row_texts(&at_rule, None, now, &tz);
        assert!(texts.armed, "an At row is armed regardless of due date");
        assert_eq!(texts.primary, countdown_text(effective, now));
        assert_eq!(
            texts.detail,
            format!("@ {}", iso_local_fire_time(effective, &tz))
        );
        // An unarmed DueOffset row: the explicit state, never silent —
        // and armed = false derives from the missing due date.
        let offset_rule = taskboard_domain::ReminderRule {
            id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::from_u128(3)),
            kind: taskboard_domain::ReminderKind::DueOffset(
                taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).unwrap())
                    .unwrap(),
            ),
            ..at_rule.clone()
        };
        let texts = rule_row_texts(&offset_rule, None, now, &tz);
        assert!(!texts.armed, "a due-less offset row is not armed");
        assert_eq!(texts.primary, RULE_ROW_UNARMED);
        assert_eq!(texts.detail, "");
        // The same rule with a due date: armed — the authored offset
        // plus the effective fire instant.
        let duedate = Utc.with_ymd_and_hms(2026, 11, 3, 10, 0, 0).unwrap();
        let texts = rule_row_texts(&offset_rule, Some(duedate), now, &tz);
        assert!(texts.armed);
        assert_eq!(
            texts.primary,
            taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).unwrap())
                .unwrap()
                .text_form()
        );
        assert_eq!(
            texts.detail,
            format!(
                "fires {}",
                iso_local_fire_time(
                    duedate
                        .checked_add_signed(TimeDelta::try_minutes(-10).unwrap())
                        .unwrap(),
                    &tz
                )
            )
        );
    }

    #[test]
    fn parse_rule_when_dispatches_the_three_forms() {
        use super::{RuleWhen, parse_rule_when};
        let now = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        // Bare: the due-anchored parity anchor.
        assert_eq!(
            parse_rule_when("10m", now, &tz),
            Some(RuleWhen::DueOffset(
                taskboard_domain::ReminderOffset::before(TimeDelta::try_minutes(-10).unwrap())
                    .unwrap()
            ))
        );
        // in/at: the absolute kind, carried as AdhocWhen verbatim.
        assert_eq!(
            parse_rule_when("in 2h", now, &tz),
            Some(RuleWhen::At(taskboard_domain::AdhocWhen::In(
                TimeDelta::try_hours(2).unwrap()
            )))
        );
        assert_eq!(
            parse_rule_when("at 2030-04-01T05:00:00Z", now, &tz),
            Some(RuleWhen::At(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2030, 4, 1, 5, 0, 0).unwrap()
            )))
        );
        // The `at HH:MM` form resolves in the injected tz (the
        // structural instant, not a label).
        match parse_rule_when("at 17:00", now, &tz) {
            Some(RuleWhen::At(taskboard_domain::AdhocWhen::At(instant))) => {
                assert_eq!(
                    instant,
                    Utc.with_ymd_and_hms(2026, 11, 3, 17, 0, 0).unwrap()
                );
            }
            other => panic!("expected At(At), got {other:?}"),
        }
        // The miss classes: non-positive in-magnitude, unknown prefix,
        // bare garbage, empty.
        assert_eq!(parse_rule_when("in -10m", now, &tz), None);
        assert_eq!(parse_rule_when("in", now, &tz), None);
        assert_eq!(parse_rule_when("soon 10m", now, &tz), None);
        assert_eq!(parse_rule_when("soon", now, &tz), None);
        assert_eq!(parse_rule_when("", now, &tz), None);
    }

    #[test]
    fn rule_when_validity_follows_the_rule_grammar() {
        use super::rule_when_validity;
        let now = Utc.with_ymd_and_hms(2026, 11, 3, 9, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        assert_eq!(rule_when_validity("", now, &tz), None);
        assert_eq!(rule_when_validity("   ", now, &tz), None);
        assert_eq!(rule_when_validity("45m", now, &tz), Some(true));
        assert_eq!(rule_when_validity("in 2h", now, &tz), Some(true));
        assert_eq!(rule_when_validity("at 17:00", now, &tz), Some(true));
        assert_eq!(rule_when_validity("soon", now, &tz), Some(false));
        assert_eq!(rule_when_validity("in", now, &tz), Some(false));
    }

    #[test]
    fn timer_when_validity_follows_the_timer_grammar() {
        // (r4 D27) The injected-tz probe over parse_timer_when's own
        // refusal discipline.
        let now = Utc.with_ymd_and_hms(2030, 1, 31, 12, 0, 0).unwrap();
        assert_eq!(timer_when_validity("", now, &Utc), None);
        assert_eq!(timer_when_validity("   ", now, &Utc), None);
        assert_eq!(timer_when_validity("in 10m", now, &Utc), Some(true));
        assert_eq!(timer_when_validity("at 13:30", now, &Utc), Some(true));
        assert_eq!(timer_when_validity("at 2030-02-01", now, &Utc), Some(true));
        assert_eq!(timer_when_validity("soon", now, &Utc), Some(false));
        assert_eq!(timer_when_validity("in", now, &Utc), Some(false));
        assert_eq!(timer_when_validity("at 25:00", now, &Utc), Some(false));
    }

    #[test]
    fn parse_timer_when_accepts_the_in_and_at_forms() {
        let now = Utc.with_ymd_and_hms(2026, 10, 27, 10, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(2 * 3600).unwrap();

        assert_eq!(
            parse_timer_when("in 3m", now, &tz),
            Some(taskboard_domain::AdhocWhen::In(TimeDelta::minutes(3)))
        );
        assert_eq!(
            parse_timer_when("in 1h30m", now, &tz),
            Some(taskboard_domain::AdhocWhen::In(
                TimeDelta::hours(1) + TimeDelta::minutes(30)
            ))
        );
        // RFC-3339 any-offset parses to the exact instant.
        assert_eq!(
            parse_timer_when("at 2026-10-28T05:00:00Z", now, &tz),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2026, 10, 28, 5, 0, 0).unwrap()
            ))
        );
        // Bare date: host-local midnight (00:00+02:00 → 22:00Z the
        // day before).
        assert_eq!(
            parse_timer_when("at 2026-10-28", now, &tz),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2026, 10, 27, 22, 0, 0).unwrap()
            ))
        );
    }

    #[test]
    fn parse_timer_when_rolls_a_past_bare_time_to_tomorrow() {
        let evening = Utc.with_ymd_and_hms(2026, 10, 27, 20, 0, 0).unwrap();
        let morning = Utc.with_ymd_and_hms(2026, 10, 27, 6, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(2 * 3600).unwrap();

        // 09:30 local with now 22:00 local ⇒ tomorrow 09:30.
        assert_eq!(
            parse_timer_when("at 09:30", evening, &tz),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2026, 10, 28, 7, 30, 0).unwrap()
            ))
        );
        // now 08:00 local ⇒ today 09:30.
        assert_eq!(
            parse_timer_when("at 09:30", morning, &tz),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2026, 10, 27, 7, 30, 0).unwrap()
            ))
        );
        // The seconds form parses; a time exactly now rolls (≤ now).
        assert_eq!(
            parse_timer_when("at 22:00:00", evening, &tz),
            Some(taskboard_domain::AdhocWhen::At(
                Utc.with_ymd_and_hms(2026, 10, 28, 20, 0, 0).unwrap()
            ))
        );
    }

    #[test]
    fn parse_timer_when_rejects_every_malformed_class() {
        let now = Utc.with_ymd_and_hms(2026, 10, 27, 10, 0, 0).unwrap();
        let tz = chrono::FixedOffset::east_opt(0).unwrap();
        for raw in [
            "",
            "in",
            "in 0s",
            "in -3m",
            "in 3x",
            "3m",
            "at",
            "at not-a-time",
            "at 25:00",
            "soon 3m",
        ] {
            assert_eq!(
                parse_timer_when(raw, now, &tz),
                None,
                "{raw:?} must not parse"
            );
        }
    }

    proptest! {
        /// The `in` form's positivity property (D-P8): whatever the
        /// magnitude text, the parse is `None` or a strictly positive
        /// `In` — the domain's `InvalidAdhocWhen` never crosses the
        /// seam.
        #[test]
        fn the_in_form_never_mints_a_nonpositive_duration(
            components in proptest::collection::vec(
                (1u64..1_000, proptest::sample::select(vec!["s", "m", "h", "d"])),
                1..4,
            ),
        ) {
            let now = Utc.with_ymd_and_hms(2026, 10, 27, 10, 0, 0).unwrap();
            let tz = chrono::FixedOffset::east_opt(0).unwrap();
            let mut magnitude = String::new();
            for (n, unit) in &components {
                magnitude.push_str(&n.to_string());
                magnitude.push_str(unit);
            }
            match parse_timer_when(&format!("in {magnitude}"), now, &tz) {
                None => {}
                Some(taskboard_domain::AdhocWhen::In(d)) => assert!(d > TimeDelta::zero()),
                Some(other) => panic!("the in form minted {other:?}"),
            }
        }
    }

    #[test]
    fn awaiting_row_buttons_follow_the_anchor_table() {
        let task_anchor =
            taskboard_domain::ReminderAnchor::Task(TaskId::from_uuid(uuid::Uuid::now_v7()));
        let alarm_anchor = taskboard_domain::ReminderAnchor::Alarm(
            taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7()),
        );
        // Task rows keep all five task-shaped affordances, no cancel.
        assert_eq!(
            awaiting_row_buttons(task_anchor),
            AwaitingButtons {
                dismiss: true,
                done: true,
                snooze: true,
                mute: true,
                cancel: false,
                track_start: true,
            }
        );
        // Alarm rows: Dismiss / Snooze / Cancel — no Mark Done (no
        // task), no Mute (mute is task-shaped), no Start Task.
        assert_eq!(
            awaiting_row_buttons(alarm_anchor),
            AwaitingButtons {
                dismiss: true,
                done: false,
                snooze: true,
                mute: false,
                cancel: true,
                track_start: false,
            }
        );
    }

    #[test]
    fn notification_phrase_buckets_the_elapsed_time() {
        let now = Utc::now();
        assert_eq!(notification_phrase(now, now), "fired now");
        // Structural facts only (AGENTS §5): past-direction at every
        // bucket, monotone wording, never a future tense.
        let past = notification_phrase(now - TimeDelta::hours(2), now);
        assert!(past.starts_with("fired ") && past.ends_with(" ago"));
        assert_ne!(past, notification_phrase(now - TimeDelta::hours(3), now));
    }

    #[test]
    fn alarm_window_rows_follow_the_domain_view_filtered_to_alarm() {
        let now = Utc::now();
        let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut tea = alarm(id, "tea", now - TimeDelta::minutes(1));
        tea.insistence = taskboard_domain::Insistence::Alarm;
        state.reminders.alarms.insert(id, tea);
        // A normal-insistence alarm alongside: filtered out.
        let normal = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        state
            .reminders
            .alarms
            .insert(normal, alarm(normal, "quiet", now - TimeDelta::minutes(1)));
        // Both delivered (awaiting).
        deliver_alarm(&mut state, id, now - TimeDelta::minutes(1));
        deliver_alarm(&mut state, normal, now - TimeDelta::minutes(1));

        let rows = alarm_window_rows(&state, &defaults(), calendar(), now);
        assert_eq!(rows.len(), 1, "only the alarm-insistent row survives");
        assert_eq!(rows[0].alarm, id);
        assert_eq!(rows[0].title, "tea");
        assert_eq!(
            rows[0].reminder,
            ReminderRef::Adhoc(id),
            "the commands carry the reminder ref"
        );
        assert!(alarm_window_open(&rows));

        // A dismissed alarm leaves the set.
        state
            .reminders
            .dismissals
            .insert(taskboard_domain::ReminderDismissal {
                reminder: ReminderRef::Adhoc(id),
                occurrence: now - TimeDelta::minutes(1),
                dismissed_at: now,
            });
        let rows = alarm_window_rows(&state, &defaults(), calendar(), now);
        assert_eq!(rows.len(), 0, "dismissed is gone for good");
        assert!(!alarm_window_open(&rows));
    }

    #[test]
    fn attention_is_the_gained_edge() {
        let a = AlarmRow {
            alarm: taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7()),
            reminder: ReminderRef::Default(TaskId::from_uuid(uuid::Uuid::now_v7())),
            occurrence: Utc::now(),
            title: "a".into(),
        };
        let mut b = a.clone();
        b.alarm = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
        b.occurrence = Utc::now();

        let a_slice = std::slice::from_ref(&a);
        assert!(attention_due(a_slice, &[]), "empty -> non-empty gains");
        assert!(!attention_due(a_slice, a_slice), "same set: no edge");
        assert!(attention_due(&[a.clone(), b], a_slice), "b is new");
        assert!(
            !attention_due(&[], a_slice),
            "drained: the window closes, no attention"
        );
    }

    fn defaults() -> taskboard_domain::ReminderDefaults {
        taskboard_domain::ReminderDefaults::default()
    }

    fn calendar() -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }

    #[test]
    fn scheduled_phrase_is_empty_when_absent_and_directional_when_set() {
        // Structural facts only (AGENTS §5), the `due_phrase` precedent:
        // the words are presentation wording, the direction is the
        // behavior — pinned as future ≠ past at the same bucket.
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("ts");
        assert_eq!(scheduled_phrase(None, now, &Utc), "");
        assert!(
            !scheduled_phrase(Some(now), now, &Utc).is_empty(),
            "the boundary bucket renders"
        );
        let future = scheduled_phrase(Some(now + TimeDelta::hours(2)), now, &Utc);
        let past = scheduled_phrase(Some(now - TimeDelta::hours(2)), now, &Utc);
        assert_ne!(future, "", "the future phrase renders");
        assert_ne!(past, "", "the past phrase renders");
        assert_ne!(future, past, "the direction is carried at one bucket");
    }

    #[test]
    fn direction_partition_is_typed() {
        // Issue #458: the direction is a typed value; the sign
        // comparison is pinned per bucket class with matches!-class
        // assertions — no text anywhere (AGENTS §5).
        for delta in [
            TimeDelta::try_minutes(2).unwrap(),
            TimeDelta::try_hours(2).unwrap(),
            TimeDelta::try_days(2).unwrap(),
        ] {
            assert!(
                matches!(Direction::of(delta), Direction::Future),
                "a later start is Future"
            );
            assert!(
                matches!(Direction::of(-delta), Direction::Past),
                "an earlier start is Past"
            );
        }
    }

    #[test]
    fn effort_phrase_wraps_the_domain_text_form() {
        assert_eq!(effort_phrase(None), "");
        assert_eq!(
            effort_phrase(Some(Effort::new(5400))),
            Effort::new(5400).text_form()
        );
        assert_eq!(effort_phrase(Some(Effort::new(0))), "0s");
    }

    #[test]
    fn due_sets_honours_the_handed_over_horizon() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2030, 1, 31, 12, 0, 0).unwrap();
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let id = TaskId::from_uuid(uuid::Uuid::now_v7());
        let mut t = task(id, stack, 0);
        // One hour past the default horizon: not soon under it, soon
        // under a week — the knob moves the boundary (round-3 D14).
        t.duedate = Some(now + DUE_SOON_HORIZON + TimeDelta::hours(1));
        let mut state = AppState::default();
        state.tasks.insert(id, t);
        let default_sets = due_sets(&state, now, DUE_SOON_HORIZON);
        assert!(!default_sets.soon.contains(&id));
        let week = due_sets(&state, now, TimeDelta::try_days(7).unwrap());
        assert!(week.soon.contains(&id));
        // 0 days: nothing is due soon.
        let zero = due_sets(&state, now, TimeDelta::zero());
        assert!(zero.soon.is_empty());
    }

    #[test]
    fn rule_created_phrase_is_the_short_utc_calendar_date() {
        // Structural facts only (AGENTS §5): no `T`, no offset sign,
        // bounded length — the humanized stamp, not an rfc3339 dump.
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2030, 1, 31, 12, 0, 0).unwrap();
        let created = Utc.with_ymd_and_hms(2029, 12, 1, 9, 30, 15).unwrap();
        let phrase = rule_created_phrase(created, now);
        assert!(phrase.len() <= 10);
        assert!(!phrase.contains('T'));
        assert!(!phrase.contains('+'));
    }

    #[test]
    fn at_time_suffix_is_empty_for_absent_and_midnight() {
        use chrono::TimeZone;
        assert_eq!(at_time_suffix(None, &Utc), "");
        let midnight = Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
        assert_eq!(at_time_suffix(Some(midnight), &Utc), "");
    }

    #[test]
    fn at_time_suffix_renders_the_local_wall_clock() {
        // (r4 D28 §7 pin 5) 23:30Z in a +02:00 zone renders 01:30
        // next-day — the date rolls; midnight local renders empty.
        let tz = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        let late = Utc.with_ymd_and_hms(2030, 7, 1, 23, 30, 0).unwrap();
        assert_eq!(at_time_suffix(Some(late), &tz), " \u{b7} 01:30");
        let local_midnight = Utc.with_ymd_and_hms(2030, 7, 1, 22, 0, 0).unwrap();
        assert_eq!(at_time_suffix(Some(local_midnight), &tz), "");
    }

    #[test]
    fn at_time_suffix_carries_the_utc_clock_and_both_phrases_compose_it() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2030, 1, 31, 12, 0, 0).unwrap();
        let timed = Utc.with_ymd_and_hms(2030, 2, 2, 14, 30, 0).unwrap();
        assert_eq!(at_time_suffix(Some(timed), &Utc), " \u{b7} 14:30");
        // Midnight instants render exactly the date-only shape.
        let midnight = Utc.with_ymd_and_hms(2030, 2, 2, 0, 0, 0).unwrap();
        let timed_phrase = due_phrase(DueKind::Later, Some(timed), now, &Utc);
        let midnight_phrase = due_phrase(DueKind::Later, Some(midnight), now, &Utc);
        assert!(timed_phrase.ends_with(" \u{b7} 14:30"));
        assert_eq!(midnight_phrase.find('\u{b7}'), None);
        let sched_timed = scheduled_phrase(Some(timed), now, &Utc);
        let sched_midnight = scheduled_phrase(Some(midnight), now, &Utc);
        assert!(sched_timed.ends_with(" \u{b7} 14:30"));
        assert_eq!(sched_midnight.find('\u{b7}'), None);
    }

    #[test]
    fn is_anchor_task_matches_the_exact_title_only() {
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut anchor = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 0);
        anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
        assert!(is_anchor_task(&anchor));
        for near_miss in [
            "[taskboard] app-data ",
            " [taskboard] app-data",
            "[taskboard] app-data.",
            "[TASKBOARD] APP-DATA",
        ] {
            let mut t = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 1);
            t.title = near_miss.to_owned();
            assert!(!is_anchor_task(&t), "{near_miss:?} is a visible near-miss");
        }
    }

    #[test]
    fn banner_rows_exclude_the_anchor() {
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("ts");
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let mut state = AppState::default();
        let mut overdue = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 0);
        overdue.duedate = Some(now - TimeDelta::hours(2));
        state.tasks.insert(overdue.id, overdue);
        let mut anchor = task(TaskId::from_uuid(uuid::Uuid::now_v7()), stack, 1);
        anchor.title = taskboard_domain::ANCHOR_CARD_TITLE.to_owned();
        anchor.duedate = Some(now - TimeDelta::hours(3));
        let anchor_id = anchor.id;
        state.tasks.insert(anchor_id, anchor);
        let rows = banner_rows(&state, now);
        assert_eq!(rows.len(), 1, "the anchor is filtered from the banner");
        assert!(!rows.iter().any(|r| r.task == anchor_id));
    }

    #[test]
    fn sidecar_caption_is_silent_when_healthy_and_typed_when_not() {
        assert_eq!(sidecar_caption(SidecarHealth::Healthy), "");
        let too_new = sidecar_caption(SidecarHealth::Unreadable(
            taskboard_domain::AppDataUnreadable::FormatTooNew { format: 2 },
        ));
        assert!(too_new.contains('2'), "the too-new format rides along");
        for reason in [
            taskboard_domain::AppDataUnreadable::Transport,
            taskboard_domain::AppDataUnreadable::AccessDenied,
            taskboard_domain::AppDataUnreadable::ServerFault,
            taskboard_domain::AppDataUnreadable::Decode,
            taskboard_domain::AppDataUnreadable::ForeignBoard,
        ] {
            assert_ne!(sidecar_caption(SidecarHealth::Unreadable(reason)), "");
        }
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        /// `due_sets` membership pinned against independently computed
        /// predicates in the test (production keeps one implementation):
        /// overdue ⇔ `duedate < now` (strict), soon ⇔ `(now, now+24h]`.
        #[test]
        fn due_sets_match_the_domain_predicates(
            offset_secs in -172_800i64..=172_800,
            done in prop::bool::ANY,
        ) {
            let now = Utc::now();
            let stack = StackId::from_uuid(uuid::Uuid::now_v7());
            let id = TaskId::from_uuid(uuid::Uuid::now_v7());
            let mut t = task(id, stack, 0);
            t.duedate = Some(now + TimeDelta::seconds(offset_secs));
            if done {
                t.done = Some(now);
            }
            let mut state = AppState::default();
            state.tasks.insert(id, t.clone());

            let sets = due_sets(&state, now, DUE_SOON_HORIZON);
            let horizon = now + DUE_SOON_HORIZON;
            let overdue = !done && t.duedate.is_some_and(|d| d < now);
            let soon = !done && t.duedate.is_some_and(|d| d > now && d <= horizon);
            prop_assert_eq!(sets.overdue.contains(&id), overdue);
            prop_assert_eq!(sets.soon.contains(&id), soon);
            prop_assert!(!(sets.overdue.contains(&id) && sets.soon.contains(&id)));
        }

        /// `duration_bucket` is monotone and saturates without panic.
        #[test]
        fn duration_bucket_is_monotone(a in -10_000_000i64..=10_000_000, b in -10_000_000i64..=10_000_000) {
            let rank = |b: &DurationBucket| match *b {
                DurationBucket::Now => 0,
                DurationBucket::Minutes(n) => u64::from(n),
                DurationBucket::Hours(n) => u64::from(n) * 60,
                DurationBucket::Days(n) => u64::from(n) * 1440,
            };
            let (low, high) = if a <= b { (a, b) } else { (b, a) };
            prop_assert!(rank(&duration_bucket(TimeDelta::seconds(low)))
                <= rank(&duration_bucket(TimeDelta::seconds(high))));
        }

        /// `banner_rows` over generated states: live+open+overdue only,
        /// canonical order, targets well-formed.
        #[test]
        fn banner_rows_are_canonical_and_well_formed(
            offsets in prop::collection::vec(-48i64..48, 0..8),
            rules in prop::collection::vec(prop::bool::ANY, 0..8),
        ) {
            let now = Utc::now();
            let stack = StackId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            let rule_this = rules.first().copied().unwrap_or(false);
            for (i, offset) in offsets.iter().enumerate() {
                let id = TaskId::from_uuid(uuid::Uuid::now_v7());
                let mut t = task(id, stack, i64::try_from(i).unwrap_or(i64::MAX));
                t.duedate = Some(now + TimeDelta::hours(*offset));
                if *offset >= 0 {
                    t.done = Some(now); // only overdue tasks belong
                }
                state.tasks.insert(id, t);
                if rule_this {
                    let rule = ReminderRule {
                        insistence: taskboard_domain::Insistence::Normal,
                        id: taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7()),
                        task: id,
                        kind: taskboard_domain::ReminderKind::DueOffset(
                            ReminderOffset::before(TimeDelta::minutes(-15))
                                .expect("valid offset"),
                        ),
                        created_at: now,
                        updated_at: now,
                    };
                    state.reminders.rules.insert(rule.id, rule);
                }
            }
            let rows = banner_rows(&state, now);
            let expected: Vec<TaskId> = taskboard_domain::overdue_tasks(&state.tasks, now)
                .into_iter()
                .map(|t| t.id)
                .collect();
            let got: Vec<TaskId> = rows.iter().map(|r| r.task).collect();
            prop_assert_eq!(got, expected);
            for row in &rows {
                prop_assert_eq!(row.kind, DueKind::Overdue);
                // Banner rows are task-only by contract: an alarm-anchored
                // target is unconstructible through `banner_rows`.
                let taskboard_domain::ReminderAnchor::Task(anchor_task) = row.target.anchor else {
                    panic!("banner rows never carry alarm anchors");
                };
                prop_assert_eq!(anchor_task, row.task);
                match row.target.reminder {
                    ReminderRef::Rule(_) => {
                        prop_assert!(state.reminders.rules.values().any(|r| r.task == row.task));
                    }
                    ReminderRef::Default(task) => prop_assert_eq!(task, row.task),
                    ReminderRef::Adhoc(_) | ReminderRef::Break(_)
                    | ReminderRef::Tracker(_) => {
                        panic!("banner rows never carry alarm, break, or tracker refs");
                    }
                }
            }
        }

        /// `stacks_in_board` equals the reference `(order, id)` sort of
        /// the board's live stacks (test-side model recomputation).
        #[test]
        fn stacks_in_board_matches_reference_sort(
            orders in prop::collection::vec(0i64..5, 1..6),
        ) {
            let b1 = BoardId::from_uuid(uuid::Uuid::now_v7());
            let b2 = BoardId::from_uuid(uuid::Uuid::now_v7());
            let mut state = AppState::default();
            state.boards.insert(b1, board(b1));
            let mut ids = Vec::new();
            for (i, order) in orders.iter().enumerate() {
                let id = StackId::from_uuid(uuid::Uuid::now_v7());
                ids.push((id, *order));
                state.stacks.insert(id, stack(id, b1, *order));
                if i == 0 {
                    let other = StackId::from_uuid(uuid::Uuid::now_v7());
                    state.stacks.insert(other, stack(other, b2, 0));
                }
            }
            let mut expected: Vec<(i64, StackId)> =
                ids.into_iter().map(|(id, order)| (order, id)).collect();
            expected.sort();
            let got: Vec<(i64, StackId)> = stacks_in_board(&state, b1)
                .into_iter()
                .map(|s| (s.order, s.id))
                .collect();
            prop_assert_eq!(got, expected);
        }
    }

    // -- the review walk's phrase vocabulary (DR-7 chunk 3, D-W7) --
    // The phrase fns OWN their strings: these are the only tests in
    // the tree allowed to assert walk/card text.

    fn walk_calendar() -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("utc"),
        }
    }

    fn walk_day() -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(2026, 11, 5).expect("date")
    }

    #[test]
    fn the_review_card_phrases_carry_the_ruled_strings() {
        let day = walk_day();
        assert_eq!(
            review_card_title(taskboard_domain::ReviewCadence::Day),
            "Day review"
        );
        assert_eq!(
            review_card_body(taskboard_domain::ReviewCadence::Day, day),
            "A review for 2026-11-05 is open — about two minutes, four prompts.",
            "C2 as amended (four prompts; the effort estimate derives from the prompt count)"
        );
        assert_eq!(review_card_action(day), "Review 2026-11-05");
    }

    #[test]
    fn the_walk_header_phrases_carry_the_ruled_strings_per_cadence() {
        let day = walk_day();
        assert_eq!(
            walk_header(taskboard_domain::ReviewCadence::Day, day),
            "Review — 2026-11-05"
        );
        assert_eq!(
            walk_header(taskboard_domain::ReviewCadence::Week, day),
            "Review — week of 2026-11-05"
        );
        assert_eq!(
            walk_header(taskboard_domain::ReviewCadence::Month, day),
            "Review — november 2026"
        );
    }

    #[test]
    fn the_walk_stage_and_action_phrases_carry_the_ruled_strings() {
        let day = &taskboard_domain::DEFAULT_TEMPLATES[0];
        let week = &taskboard_domain::DEFAULT_TEMPLATES[1];

        assert_eq!(
            walk_stage_header(crate::view_state::WalkStage::Inbox { filing: None }, day),
            "Inbox"
        );
        assert_eq!(
            walk_stage_header(
                crate::view_state::WalkStage::CarryForward {
                    picking_date: false
                },
                day
            ),
            "Carry forward"
        );
        assert_eq!(
            walk_stage_header(crate::view_state::WalkStage::Prompts, week),
            week.name
        );
        assert_eq!(
            walk_stage_empty_line(crate::view_state::WalkStage::Inbox { filing: None }),
            "The inbox is empty."
        );
        assert_eq!(
            walk_stage_empty_line(crate::view_state::WalkStage::CarryForward {
                picking_date: false
            }),
            "Nothing to carry forward."
        );
        assert_eq!(
            walk_stage_empty_line(crate::view_state::WalkStage::Prompts),
            ""
        );
        let (file, keep) = walk_inbox_actions();
        assert_eq!((file, keep), ("File to…", "Keep in inbox"));
        let (tomorrow, pick, leave) = walk_carry_actions();
        assert_eq!(
            (tomorrow, pick, leave),
            ("Move to tomorrow", "Pick a date…", "Leave as is")
        );
        let (record, finish) = walk_commit_actions();
        assert_eq!((record, finish), ("Record", "Finish without recording"));
    }

    /// The P9 vocabulary guard (a floor on the copy, not a label
    /// test): no walk or card phrase ever carries verdict language —
    /// "missed", "failed", or "overdue". If the code got strictly
    /// better copy this still passes.
    #[test]
    fn the_walk_vocabulary_never_carries_verdict_language() {
        let day = walk_day();
        let day_template = &taskboard_domain::DEFAULT_TEMPLATES[0];
        let stages = [
            crate::view_state::WalkStage::Inbox { filing: None },
            crate::view_state::WalkStage::Inbox {
                filing: Some(StackId::from_uuid(uuid::Uuid::now_v7())),
            },
            crate::view_state::WalkStage::CarryForward {
                picking_date: false,
            },
            crate::view_state::WalkStage::CarryForward { picking_date: true },
            crate::view_state::WalkStage::Prompts,
        ];
        for cadence in [
            taskboard_domain::ReviewCadence::Day,
            taskboard_domain::ReviewCadence::Week,
            taskboard_domain::ReviewCadence::Month,
        ] {
            let phrases = vec![
                review_card_title(cadence),
                review_card_body(cadence, day),
                review_card_action(day),
                walk_header(cadence, day),
            ];
            for phrase in phrases {
                let lower = phrase.to_lowercase();
                for verdict in ["missed", "failed", "overdue"] {
                    assert!(
                        !lower.contains(verdict),
                        "verdict language {verdict:?} leaked into {phrase:?}"
                    );
                }
            }
        }
        for stage in stages {
            for phrase in [
                walk_stage_header(stage, day_template).clone(),
                walk_stage_empty_line(stage).to_owned(),
            ] {
                let lower = phrase.to_lowercase();
                for verdict in ["missed", "failed", "overdue"] {
                    assert!(
                        !lower.contains(verdict),
                        "verdict language {verdict:?} leaked into {phrase:?}"
                    );
                }
            }
        }
        let actions = [
            walk_inbox_actions().0.to_owned(),
            walk_inbox_actions().1.to_owned(),
            walk_carry_actions().0.to_owned(),
            walk_carry_actions().1.to_owned(),
            walk_carry_actions().2.to_owned(),
            walk_commit_actions().0.to_owned(),
            walk_commit_actions().1.to_owned(),
        ];
        for phrase in actions {
            let lower = phrase.to_lowercase();
            for verdict in ["missed", "failed", "overdue"] {
                assert!(!lower.contains(verdict), "verdict language in {phrase:?}");
            }
        }
    }

    // -- the carry-forward rows (plan D-W4): the verbatim union pin --

    fn walk_task(
        raw: u128,
        scheduled: Option<DateTime<Utc>>,
        duedate: Option<DateTime<Utc>>,
        order: i64,
    ) -> Task {
        let mut t = task(
            TaskId::from_uuid(uuid::Uuid::from_u128(raw)),
            StackId::from_uuid(uuid::Uuid::from_u128(1)),
            order,
        );
        t.scheduled = scheduled;
        t.duedate = duedate;
        t
    }

    #[test]
    fn the_day_carry_rows_are_the_sections_union_deduplicated_in_order() {
        let start = epoch();
        let now = start + TimeDelta::hours(1);
        let end = start + TimeDelta::hours(24);
        let cal = walk_calendar();
        let window = taskboard_domain::review_window(
            taskboard_domain::ReviewCadence::Day,
            cal.today(now),
            cal,
        );
        // Task 3 carries both anchors: classified overdue (first
        // match), so the union lists it once.
        let tasks = std::collections::BTreeMap::from([
            (
                TaskId::from_uuid(uuid::Uuid::from_u128(1)),
                walk_task(1, None, Some(start + TimeDelta::hours(5)), 0),
            ),
            (
                TaskId::from_uuid(uuid::Uuid::from_u128(2)),
                walk_task(2, Some(start + TimeDelta::hours(3)), None, 0),
            ),
            (
                TaskId::from_uuid(uuid::Uuid::from_u128(3)),
                walk_task(
                    3,
                    Some(start + TimeDelta::hours(2)),
                    Some(start + TimeDelta::hours(6)),
                    0,
                ),
            ),
            (
                TaskId::from_uuid(uuid::Uuid::from_u128(4)),
                walk_task(4, Some(start + TimeDelta::hours(12)), None, 0),
            ),
        ]);
        let sections = taskboard_domain::days::today_sections(&tasks, now, (start, end));
        let mut expected: Vec<TaskId> = Vec::new();
        for id in sections
            .overdue
            .iter()
            .chain(&sections.scheduled)
            .chain(&sections.due_today)
        {
            if !expected.contains(id) {
                expected.push(*id);
            }
        }
        assert_eq!(
            walk_carry_rows(&tasks, taskboard_domain::ReviewCadence::Day, window, now),
            expected,
            "first occurrence wins, section order preserved"
        );
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(64))]

        /// The equality pin (plan §3 step 4): for any generated task
        /// set, the Day cadence's carry rows are exactly the
        /// de-duplicated `today_sections` union — no second
        /// classification exists (VF-9).
        #[test]
        fn day_carry_rows_equal_the_sections_union_over_generated_states(
            anchors in proptest::collection::vec(
                (
                    -2_i64..=30 * 3600,
                    -2_i64..=30 * 3600,
                    -100_i64..=100,
                    proptest::bool::ANY,
                    proptest::bool::ANY,
                    proptest::arbitrary::any::<u128>(),
                ),
                0..15,
            ),
        ) {
            let start = epoch();
            let now = start + TimeDelta::hours(3);
            let cal = walk_calendar();
            let window = taskboard_domain::review_window(
                taskboard_domain::ReviewCadence::Day,
                cal.today(now),
                cal,
            );
            let tasks: std::collections::BTreeMap<TaskId, Task> = anchors
                .iter()
                .map(|&(sched, due, order, with_sched, with_due, raw)| {
                    let t = walk_task(
                        raw,
                        with_sched.then(|| start + TimeDelta::seconds(sched)),
                        with_due.then(|| start + TimeDelta::seconds(due)),
                        order,
                    );
                    (t.id, t)
                })
                .collect();
            let sections = taskboard_domain::days::today_sections(
                &tasks,
                now,
                window.bounds(),
            );
            let mut expected: Vec<TaskId> = Vec::new();
            for id in sections
                .overdue
                .iter()
                .chain(&sections.scheduled)
                .chain(&sections.due_today)
            {
                if !expected.contains(id) {
                    expected.push(*id);
                }
            }
            prop_assert_eq!(
                walk_carry_rows(
                    &tasks,
                    taskboard_domain::ReviewCadence::Day,
                    window,
                    now
                ),
                expected
            );
            // The structural Week/Month arm reads the domain's
            // window_misses over the same window.
            prop_assert_eq!(
                walk_carry_rows(
                    &tasks,
                    taskboard_domain::ReviewCadence::Week,
                    window,
                    now
                ),
                taskboard_domain::window_misses(&tasks, now, window.bounds())
            );
        }
    }
}

/// A connector row's live status (round-1 D8): the down-override wins;
/// otherwise the bound boards' phases fold with the badge ordering
/// (`Syncing > Failed > Offline > Idle`, the `sync_badge_for_board`
/// precedent); no bound boards → `Idle` ("attached, nothing to do").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorStatus {
    Idle,
    Syncing,
    Offline,
    Failed,
    /// The `ConnectorLost` episode override (the liveness map, D8).
    Down,
}

/// The fold. Pure over the state snapshot plus the override flag —
/// unit-pinned (T4).
#[must_use]
pub fn connector_row_status(
    state: &AppState,
    connector: &taskboard_domain::ConnectorId,
    down: bool,
) -> ConnectorStatus {
    if down {
        return ConnectorStatus::Down;
    }
    let mut worst = ConnectorStatus::Idle;
    for board in state.bound_boards_of(connector) {
        let Some(status) = state.sync.get(board) else {
            continue;
        };
        let phase = match status.phase {
            SyncPhase::Syncing => ConnectorStatus::Syncing,
            SyncPhase::Failed { .. } => ConnectorStatus::Failed,
            SyncPhase::Offline => ConnectorStatus::Offline,
            SyncPhase::Idle => ConnectorStatus::Idle,
        };
        if phase_rank(phase) > phase_rank(worst) {
            worst = phase;
        }
    }
    worst
}

/// The row-status tone vocabulary (r4 D39): the `RowDot` colour
/// language as a plain enum — `success`/`accent`/`warning`/`danger`/
/// `neutral` — so the dropdown reads like the dots it replaces
/// without this module ever naming a slint type (the extraction
/// seam's rule). The adapter maps it onto the widget's tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusTone {
    Neutral,
    Success,
    Accent,
    Warning,
    Danger,
}

/// The status→tone fold (r4 D39, `RowDot`'s exact language):
/// `Idle→Success, Syncing→Accent, Offline→Warning, Failed/Down→Danger`.
#[must_use]
pub fn status_tone(status: ConnectorStatus) -> StatusTone {
    match status {
        ConnectorStatus::Idle => StatusTone::Success,
        ConnectorStatus::Syncing => StatusTone::Accent,
        ConnectorStatus::Offline => StatusTone::Warning,
        ConnectorStatus::Failed | ConnectorStatus::Down => StatusTone::Danger,
    }
}

/// The board projection (r4 D39 §2.3): the ui-side twin of the app
/// reply's board half (`current_connections`'s board arm) — the live
/// boards (`!deleted && !archived`) in state order, the binding's
/// connector name carried across. `push_settings` re-derives the
/// selector's options from this EVERY pass, so the list can no longer
/// go stale by construction (finding F29's ghost list). Pure over the
/// snapshot; no slint types (`BoardInfo` is the hand-written requests
/// shape).
#[must_use]
pub fn boards_of(state: &AppState) -> Vec<crate::requests::BoardInfo> {
    state
        .boards
        .values()
        .filter(|board| !board.deleted && !board.archived)
        .map(|board| crate::requests::BoardInfo {
            id: board.id.as_uuid(),
            name: board.title.clone(),
            bound_connector: board
                .remote
                .as_ref()
                .map(|binding| binding.connector.to_string()),
        })
        .collect()
}

/// One account card (round-5 chunk 8, D-8.2): the connectors sharing
/// one exact `(server_url, username)` identity, in config order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountCard {
    pub server_url: String,
    pub username: String,
    /// The card's connector names, config order (one Deck row each).
    pub connectors: Vec<String>,
}

/// The grouping fold (D-8.2): connectors → account cards keyed by
/// exact string equality of `(server_url, username)` — no URL
/// normalization (the credential store keys on the exact pair, so a
/// folded variant would silently merge two keyring entries). Cards
/// order by first appearance; connectors keep config order.
#[must_use]
pub fn account_cards(rows: &[crate::requests::ConnectorInfo]) -> Vec<AccountCard> {
    let mut cards: Vec<AccountCard> = Vec::new();
    for row in rows {
        match cards
            .iter_mut()
            .find(|card| card.server_url == row.server_url && card.username == row.username)
        {
            Some(card) => card.connectors.push(row.name.clone()),
            None => cards.push(AccountCard {
                server_url: row.server_url.clone(),
                username: row.username.clone(),
                connectors: vec![row.name.clone()],
            }),
        }
    }
    cards
}

/// The card's folded status dot: the worst of its connectors'
/// [`connector_row_status`] folds (the badge ordering).
#[must_use]
pub fn account_status(
    state: &AppState,
    card: &AccountCard,
    down: impl Fn(&str) -> bool,
) -> ConnectorStatus {
    card.connectors
        .iter()
        .map(|name| {
            connector_row_status(
                state,
                &taskboard_domain::ConnectorId::from(name.clone()),
                down(name),
            )
        })
        .max_by_key(|status| phase_rank(*status))
        .unwrap_or(ConnectorStatus::Idle)
}

#[cfg(test)]
mod account_card_tests {
    use super::*;
    use crate::requests::ConnectorInfo;

    fn row(name: &str, server: &str, user: &str) -> ConnectorInfo {
        ConnectorInfo {
            name: name.into(),
            server_url: server.into(),
            username: user.into(),
        }
    }

    #[test]
    fn connectors_group_by_exact_identity_in_first_appearance_order() {
        let cards = account_cards(&[
            row("b", "https://y.example", "bob"),
            row("a", "https://x.example", "alice"),
            row("a2", "https://x.example", "alice"),
            row("slash", "https://x.example/", "alice"),
            row("c", "https://x.example", "carol"),
        ]);
        let shape: Vec<(&str, &str, Vec<&str>)> = cards
            .iter()
            .map(|c| {
                (
                    c.server_url.as_str(),
                    c.username.as_str(),
                    c.connectors.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                ("https://y.example", "bob", vec!["b"]),
                ("https://x.example", "alice", vec!["a", "a2"]),
                ("https://x.example/", "alice", vec!["slash"]),
                ("https://x.example", "carol", vec!["c"]),
            ]
        );
        assert_eq!(account_cards(&[]), Vec::new());
    }

    #[test]
    fn the_card_status_is_the_worst_of_its_connectors() {
        let card = AccountCard {
            server_url: "https://x.example".into(),
            username: "alice".into(),
            connectors: vec!["a".into(), "b".into()],
        };
        let state = AppState::default();
        assert_eq!(
            account_status(&state, &card, |_| false),
            ConnectorStatus::Idle
        );
        assert_eq!(
            account_status(&state, &card, |name| name == "b"),
            ConnectorStatus::Down
        );
    }
}

/// The badge ordering: `Syncing > Failed > Offline > Idle`.
fn phase_rank(phase: ConnectorStatus) -> u8 {
    match phase {
        ConnectorStatus::Idle => 0,
        ConnectorStatus::Offline => 1,
        ConnectorStatus::Failed => 2,
        ConnectorStatus::Syncing => 3,
        ConnectorStatus::Down => 4,
    }
}

#[cfg(test)]
mod connector_status_tests {
    use super::*;
    use taskboard_domain::{Board, BoardClocks, BoardId, ConnectorId, RemoteBoardBinding};

    fn bound_state(connector: &str, board: BoardId, phase: SyncPhase) -> AppState {
        let mut state = AppState::default();
        let epoch = BoardClocks::epoch();
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: Some(RemoteBoardBinding {
                    board: taskboard_domain::RemoteBoardId::deck(1),
                    connector: ConnectorId::from(connector),
                }),
                title: "b".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: epoch,
                    color: epoch,
                    archived: epoch,
                    deleted: epoch,
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        set_stored(&mut state, board, phase);
        state
    }

    fn set_stored(state: &mut AppState, board: BoardId, phase: SyncPhase) {
        let mut states = taskboard_domain::SyncStates::new();
        states.set_stored(board, phase, None);
        state.sync = states;
    }

    /// The status→tone fold is `RowDot`'s language (r4 D39): typed
    /// equality, no text, no pixels.
    #[test]
    fn the_status_tone_fold_is_the_rowdot_language() {
        assert_eq!(status_tone(ConnectorStatus::Idle), StatusTone::Success);
        assert_eq!(status_tone(ConnectorStatus::Syncing), StatusTone::Accent);
        assert_eq!(status_tone(ConnectorStatus::Offline), StatusTone::Warning);
        assert_eq!(status_tone(ConnectorStatus::Failed), StatusTone::Danger);
        assert_eq!(status_tone(ConnectorStatus::Down), StatusTone::Danger);
    }

    /// The board projection (r4 D39 §2.3): live boards only, the
    /// binding's connector name carried across, tombstones and
    /// archived boards excluded — by construction, every pass.
    #[test]
    fn boards_of_projects_the_live_boards_with_their_bindings() {
        let mut state = AppState::default();
        let live = BoardId::from_uuid(uuid::Uuid::now_v7());
        let dead = BoardId::from_uuid(uuid::Uuid::now_v7());
        let archived = BoardId::from_uuid(uuid::Uuid::now_v7());
        let mut make =
            |id: BoardId, title: &str, connector: Option<&str>, archived: bool, deleted: bool| {
                let epoch = BoardClocks::epoch();
                state.boards.insert(
                    id,
                    Board {
                        id,
                        remote: connector.map(|c| RemoteBoardBinding {
                            board: taskboard_domain::RemoteBoardId::deck(1),
                            connector: ConnectorId::from(c),
                        }),
                        title: title.into(),
                        color: taskboard_domain::Color::new(""),
                        archived,
                        deleted,
                        clocks: BoardClocks {
                            title: epoch,
                            color: epoch,
                            archived: epoch,
                            deleted: epoch,
                        },
                        authors: taskboard_domain::entities::BoardAuthors::default(),
                        remote_seen: None,
                    },
                );
            };
        make(live, "home", Some("work"), false, false);
        make(dead, "gone", None, false, true);
        make(archived, "shelved", None, true, false);

        let boards = boards_of(&state);
        assert_eq!(boards.len(), 1, "the tombstone and the archive are out");
        assert_eq!(boards[0].id, live.as_uuid());
        assert_eq!(boards[0].name, "home");
        assert_eq!(
            boards[0].bound_connector.as_deref(),
            Some("work"),
            "the binding's connector round-trips"
        );
    }

    /// The inbox count (chunk 10 D9): the global Inbox board's live,
    /// open, anchor-filtered tasks; zero with no inbox; archived
    /// boards and tombstoned/done tasks never count.
    #[test]
    #[allow(clippy::too_many_lines)] // the fixture is the length; the assertions are three
    fn inbox_counts_the_global_inbox_only() {
        let mut state = AppState::default();
        assert_eq!(inbox_count(&state), 0, "no inbox, no count");
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let other = BoardId::from_uuid(uuid::Uuid::now_v7());
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: None,
                title: "Inbox".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        state.boards.insert(
            other,
            Board {
                id: other,
                remote: None,
                title: "beta".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        let inbox_stack = StackId::from_uuid(uuid::Uuid::now_v7());
        let other_stack = StackId::from_uuid(uuid::Uuid::now_v7());
        state.stacks.insert(
            inbox_stack,
            Stack {
                id: inbox_stack,
                remote: None,
                board,
                title: "Inbox".into(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    order: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::StackAuthors::default(),
                remote_seen: None,
            },
        );
        state.stacks.insert(
            other_stack,
            Stack {
                id: other_stack,
                remote: None,
                board: other,
                title: "todo".into(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    order: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::StackAuthors::default(),
                remote_seen: None,
            },
        );
        let mk = |stack: StackId, title: &str, done: bool, deleted: bool| {
            let mut t = Task {
                id: TaskId::from_uuid(uuid::Uuid::now_v7()),
                remote: None,
                label_memberships: std::collections::BTreeMap::new(),
                title: title.to_owned(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack,
                order: 0,
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,

                archived: false,
                deleted,
                clocks: taskboard_domain::TaskClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    description: taskboard_domain::BoardClocks::epoch(),
                    duedate: taskboard_domain::BoardClocks::epoch(),
                    scheduled: taskboard_domain::BoardClocks::epoch(),
                    done: taskboard_domain::BoardClocks::epoch(),
                    position: taskboard_domain::BoardClocks::epoch(),
                    effort: taskboard_domain::BoardClocks::epoch(),
                    importance: taskboard_domain::BoardClocks::epoch(),
                    urgency: taskboard_domain::BoardClocks::epoch(),
                    recur: taskboard_domain::BoardClocks::epoch(),
                    kind: taskboard_domain::BoardClocks::epoch(),

                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            };
            if done {
                t.done = Some(taskboard_domain::BoardClocks::epoch());
            }
            t
        };
        // "one" sits later in the stack than "two": the reading
        // orders by canonical `(order, id)`, never by insertion.
        // "one" sits later in the stack than "two": the reading
        // orders by canonical `(order, id)`, never by insertion. The
        // rows are keyed by their own ids (the one reading lists
        // `task.id`, the map's key here).
        let mut one = mk(inbox_stack, "one", false, false);
        one.order = 5;
        let one_id = one.id;
        state.tasks.insert(one_id, one);
        let two = mk(inbox_stack, "two", false, false);
        let two_id = two.id;
        state.tasks.insert(two_id, two);
        let done_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        state
            .tasks
            .insert(done_id, mk(inbox_stack, "done", true, false));
        let dead_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        state
            .tasks
            .insert(dead_id, mk(inbox_stack, "dead", false, true));
        let anchor_id = TaskId::from_uuid(uuid::Uuid::now_v7());
        state.tasks.insert(
            anchor_id,
            mk(
                inbox_stack,
                taskboard_domain::ANCHOR_CARD_TITLE,
                false,
                false,
            ),
        );
        state.tasks.insert(
            TaskId::from_uuid(uuid::Uuid::now_v7()),
            mk(other_stack, "elsewhere", false, false),
        );
        assert_eq!(
            inbox_count(&state),
            2,
            "live ∧ open ∧ inbox-stacked ∧ not anchor"
        );
        assert_eq!(
            inbox_tasks(&state),
            vec![two_id, one_id],
            "the one reading lists the same set in canonical stack order"
        );
    }

    /// The move-flow hint (chunk 10 D6): `"<board> › <stack>"` for a
    /// live target; empty on a dangling stack (degradation, not an
    /// error).
    #[test]
    fn move_hint_names_both_parts_and_degrades_empty() {
        let mut state = AppState::default();
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let stack = StackId::from_uuid(uuid::Uuid::now_v7());
        state.boards.insert(
            board,
            Board {
                id: board,
                remote: None,
                title: "personal".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: BoardClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    color: taskboard_domain::BoardClocks::epoch(),
                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        state.stacks.insert(
            stack,
            Stack {
                id: stack,
                remote: None,
                board,
                title: "todo".into(),
                order: 0,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: taskboard_domain::BoardClocks::epoch(),
                    order: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                authors: taskboard_domain::entities::StackAuthors::default(),
                remote_seen: None,
            },
        );
        assert_eq!(move_hint(&state, stack), "personal \u{203a} todo");
        let dangling = StackId::from_uuid(uuid::Uuid::now_v7());
        assert_eq!(move_hint(&state, dangling), "");
    }

    /// T4 — the down-override wins; the phases fold with the badge
    /// ordering; empty bindings → Idle.
    #[test]
    fn the_row_status_folds_bound_phases_and_the_down_override_wins() {
        let board = BoardId::from_uuid(uuid::Uuid::now_v7());
        let connector = ConnectorId::from("nextcloud");

        // No boards at all: attached, nothing to do.
        let empty = AppState::default();
        assert_eq!(
            connector_row_status(&empty, &connector, false),
            ConnectorStatus::Idle
        );

        // The down override wins over any fold.
        let mut state = bound_state("nextcloud", board, SyncPhase::Syncing);
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Syncing
        );
        assert_eq!(
            connector_row_status(&state, &connector, true),
            ConnectorStatus::Down
        );

        // The ordering: Failed beats Offline; Syncing beats Failed.
        set_stored(&mut state, board, SyncPhase::Offline);
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Offline
        );
        set_stored(
            &mut state,
            board,
            SyncPhase::Failed {
                last_error: taskboard_domain::SyncErrorKind::Network,
            },
        );
        assert_eq!(
            connector_row_status(&state, &connector, false),
            ConnectorStatus::Failed
        );
    }
    #[test]
    fn the_notice_expires_past_its_horizon() {
        let now = Utc::now();
        assert!(notice_visible(
            now,
            Some(now + chrono::TimeDelta::seconds(1))
        ));
        assert!(!notice_visible(now, Some(now)));
        assert!(!notice_visible(
            now,
            Some(now - chrono::TimeDelta::seconds(1))
        ));
        assert!(!notice_visible(now, None), "no notice is never visible");
    }

    #[test]
    fn the_resurrect_phrase_is_nonempty_and_deterministic() {
        let once = resurrect_notice_phrase("held work");
        assert!(!once.is_empty() && once.contains("held work"));
        assert_eq!(once, resurrect_notice_phrase("held work"));
    }
}

// ---- the recurring badge (DR-6/4, T13) ----------------------------------

#[cfg(test)]
mod recur_badge_tests {
    use super::*;
    use taskboard_domain::{
        DayCalendar, RecurBound, RecurPattern as Pattern, RecurRule, SeriesId, TaskClocks,
    };

    /// A local open head (the `tests::task` fixture is private to the
    /// sibling module; this needs only the badge-relevant fields).
    fn task() -> Task {
        Task {
            id: TaskId::from_uuid(uuid::Uuid::from_u128(1)),
            remote: None,
            label_memberships: std::collections::BTreeMap::new(),
            title: "badge head".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from_uuid(uuid::Uuid::from_u128(2)),
            order: 0,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: false,
            clocks: TaskClocks {
                title: taskboard_domain::BoardClocks::epoch(),
                description: taskboard_domain::BoardClocks::epoch(),
                duedate: taskboard_domain::BoardClocks::epoch(),
                scheduled: taskboard_domain::BoardClocks::epoch(),
                done: taskboard_domain::BoardClocks::epoch(),
                position: taskboard_domain::BoardClocks::epoch(),
                effort: taskboard_domain::BoardClocks::epoch(),
                importance: taskboard_domain::BoardClocks::epoch(),
                urgency: taskboard_domain::BoardClocks::epoch(),
                recur: taskboard_domain::BoardClocks::epoch(),
                kind: taskboard_domain::BoardClocks::epoch(),
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        }
    }

    fn calendar() -> DayCalendar {
        DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    /// The epoch instant of `d`'s UTC midnight.
    fn midnight(d: chrono::NaiveDate) -> DateTime<Utc> {
        chrono::Utc
            .timestamp_opt((d - date(1970, 1, 1)).num_days() * 86_400, 0)
            .unwrap()
    }

    fn rule(
        pattern: taskboard_domain::RecurPattern,
        anchor: chrono::NaiveDate,
        bound: RecurBound,
    ) -> RecurRule {
        RecurRule {
            series: SeriesId::from_uuid(uuid::Uuid::from_u128(1)),
            anchor,
            pattern,
            bound,
            continued_from: None,
            paused_until: None,
        }
    }

    #[test]
    fn the_paused_badge_names_the_pause_and_clears_with_it() {
        let day = calendar();
        let today = date(2026, 10, 21);
        let now = midnight(today);
        let mut t = task();
        let mut paused = rule(
            Pattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            date(2026, 10, 1),
            RecurBound::None,
        );
        paused.paused_until = Some(date(2026, 10, 25));
        t.recur = Some(paused.clone());
        // Inside the pause: the chip names the truth, not a next
        // member (the covered members do not exist).
        let badge = recur_badge(&t, day, now).expect("open + rule shows");
        assert_eq!(badge.paused_until, Some(date(2026, 10, 25)));
        let label = recur_badge_label(&badge, today);
        assert_ne!(label, String::new(), "the paused chip is never empty");
        // Structural determinism: same read, same phrase (no
        // wall-clock drift within a window).
        assert_eq!(recur_badge_label(&badge, today), label);
        // An expired pause is inert (the domain law): the chip is
        // gone and the next-member phrase is back.
        let expired_label = recur_badge_label(&badge, date(2026, 10, 26));
        assert_ne!(expired_label, label);
        assert_ne!(expired_label, String::new());
        // A pause past the Until bound: the badge renders the
        // pattern-only shape (schedule truth).
        paused.bound = RecurBound::Until(date(2026, 10, 20));
        t.recur = Some(paused);
        let badge = recur_badge(&t, day, now).expect("open + rule shows");
        assert_eq!(badge.next, None);
    }

    #[test]
    fn the_repair_target_answers_grace_and_quota_exactly() {
        use taskboard_domain::NonZeroCount;
        use taskboard_domain::habit::{Habit, HabitClocks, HabitTarget};
        let day = calendar();
        let today = date(2026, 10, 21);
        let now = midnight(today);
        let mut h = Habit {
            id: taskboard_domain::ids::HabitId::from(uuid::Uuid::from_u128(1)),
            board: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2)),
            title: "repair".into(),
            target: HabitTarget::PerDay(NonZeroCount::new(2).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: HabitClocks::all(chrono::Utc.timestamp_opt(100, 0).unwrap()),
            continued_from: None,
            deleted: false,
        };
        let mut log = std::collections::BTreeMap::new();
        // Under quota, in grace: the make-up points at yesterday's
        // window with its count plus one.
        let target = habit_repair_target(&h, &log, now, day).expect("under quota, in grace");
        assert_eq!(
            target.0.as_date(),
            taskboard_domain::habit::period_before(taskboard_domain::PeriodKind::Day, today)
        );
        assert_eq!(target.1, 1);
        // At quota: no repair (surplus logs are the +1's own business).
        log.insert(
            taskboard_domain::HabitLogKey {
                habit: h.id,
                window: target.0,
            },
            taskboard_domain::HabitLogEntry {
                count: 2,
                clock: now,
            },
        );
        assert_eq!(habit_repair_target(&h, &log, now, day), None);
        // Under quota but past the overflow grace: no repair. For a
        // daily habit the grace covers the whole day (the D-C8
        // arithmetic), so the out-of-grace leg rides a weekly habit
        // on its third day.
        h.target = HabitTarget::PerWeek(NonZeroCount::new(2).unwrap());
        let week = DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).unwrap(),
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        };
        let wednesday = chrono::NaiveDate::from_ymd_opt(2026, 10, 21).unwrap();
        let late = chrono::Utc
            .timestamp_opt(
                (wednesday - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days()
                    * 86_400
                    + 6 * 3_600,
                0,
            )
            .unwrap();
        let empty = std::collections::BTreeMap::new();
        assert_eq!(habit_repair_target(&h, &empty, late, week), None);
    }

    #[test]
    fn the_conversion_defaults_name_the_presets() {
        use taskboard_domain::recur::{EveryDays, EveryWeeks, MonthlyAnchor, RecurPattern};
        // Series → habit: the pre-selected chip is 1 per period.
        for pattern in [
            RecurPattern::Daily {
                every: EveryDays::new(3).unwrap(),
            },
            RecurPattern::Weekly {
                interval: EveryWeeks::new(2).unwrap(),
                on: taskboard_domain::WeekdaySet::new([chrono::Weekday::Mon]).expect("one weekday"),
            },
            RecurPattern::Monthly {
                on: MonthlyAnchor::OnDay(taskboard_domain::DayOfMonth::new(15).unwrap()),
            },
        ] {
            let target = convert_target_default(&pattern);
            assert_eq!(target.quota().get(), 1);
        }
        // Habit → series: the quota carries into the interval; the
        // anchors stay at the default.
        let per_day = taskboard_domain::habit::HabitTarget::PerDay(
            taskboard_domain::NonZeroCount::new(3).unwrap(),
        );
        let pattern = convert_pattern_default(per_day);
        assert!(matches!(
            pattern,
            RecurPattern::Daily { every } if every.get() == 3
        ));
        let per_week = taskboard_domain::habit::HabitTarget::PerWeek(
            taskboard_domain::NonZeroCount::new(2).unwrap(),
        );
        assert!(matches!(
            convert_pattern_default(per_week),
            RecurPattern::Weekly { interval, .. } if interval.get() == 1
        ));
    }

    #[test]
    fn visibility_follows_rule_and_openness() {
        let day = calendar();
        let now = midnight(date(2026, 10, 21));
        let mut t = task();
        // No rule → None.
        assert_eq!(recur_badge(&t, day, now), None);
        // A rule on an open head → Some.
        t.recur = Some(rule(
            Pattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            date(2026, 10, 1),
            RecurBound::None,
        ));
        let badge = recur_badge(&t, day, now).expect("open + rule shows");
        assert!(!badge.stale);
        assert_eq!(badge.next, Some(date(2026, 10, 22)));
        // A done head shows none (the successor carries the series).
        t.done = Some(now);
        assert_eq!(recur_badge(&t, day, now), None);
    }

    #[test]
    fn the_stale_head_badges_the_roll_target_with_the_overdue_decision() {
        let day = calendar();
        let now = midnight(date(2026, 10, 21));
        let mut t = task();
        t.scheduled = Some(midnight(date(2026, 10, 19)));
        t.recur = Some(rule(
            Pattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            date(2026, 10, 1),
            RecurBound::None,
        ));
        let badge = recur_badge(&t, day, now).expect("stale head shows");
        assert!(badge.stale);
        assert_eq!(badge.next, Some(date(2026, 10, 21)));
        // The dateless sibling is never stale.
        let mut dateless = task();
        dateless.recur = Some(rule(
            Pattern::Daily {
                every: taskboard_domain::EveryDays::new(1).unwrap(),
            },
            date(2026, 10, 1),
            RecurBound::None,
        ));
        let badge = recur_badge(&dateless, day, now).expect("dateless shows");
        assert!(!badge.stale);
    }

    #[test]
    fn an_exhausted_until_bound_renders_the_pattern_only_badge() {
        let day = calendar();
        let now = midnight(date(2026, 10, 21));
        let mut t = task();
        t.recur = Some(rule(
            Pattern::Weekly {
                interval: taskboard_domain::EveryWeeks::new(1).unwrap(),
                on: taskboard_domain::WeekdaySet::new([chrono::Weekday::Mon]).unwrap(),
            },
            date(2026, 10, 5),
            RecurBound::Until(date(2026, 10, 19)),
        ));
        let badge = recur_badge(&t, day, now).expect("exhausted head still shows");
        assert_eq!(badge.next, None);
        // The pattern-only label is non-empty and deterministic.
        let label = recur_badge_label(&badge, date(2026, 10, 21));
        assert!(!label.is_empty(), "the pattern-only label renders");
        assert_eq!(label, recur_badge_label(&badge, date(2026, 10, 21)));
    }

    #[test]
    fn the_label_is_day_stable_within_a_window_and_deterministic() {
        let day = calendar();
        let mut t = task();
        t.recur = Some(rule(
            Pattern::Monthly {
                on: taskboard_domain::MonthlyAnchor::OnDay(
                    taskboard_domain::DayOfMonth::new(15).unwrap(),
                ),
            },
            date(2026, 10, 15),
            RecurBound::Count(taskboard_domain::OccurrenceCount::new(3).unwrap()),
        ));
        // Two `now`s inside one window (the 21st): identical labels —
        // day granularity keeps re-pushes diff-stable.
        let morning = midnight(date(2026, 10, 21));
        let evening = morning + chrono::TimeDelta::try_hours(20).unwrap();
        let a = recur_badge(&t, day, morning).expect("badge");
        let b = recur_badge(&t, day, evening).expect("badge");
        assert_eq!(a, b, "same window, same badge");
        let label_a = recur_badge_label(&a, day.today(morning));
        assert!(!label_a.is_empty(), "the label renders");
        assert_eq!(label_a, recur_badge_label(&b, day.today(evening)));
        // The day bucket arithmetic the phrase rides: the next member
        // (Nov 15) sits 25 days out from the 21st.
        assert_eq!(
            duration_bucket(
                chrono::TimeDelta::try_days(
                    a.next
                        .expect("next")
                        .signed_duration_since(date(2026, 10, 21))
                        .num_days()
                )
                .expect("25d"),
            ),
            DurationBucket::Days(25)
        );
    }
}

// ---- The habits section (DR-6/6, §4.6) --------------------------------------

/// One today-view tracker row (the row source the section model and
/// the id table build from): the same shape the CLI's list projection
/// speaks — count/quota/streak derive from the domain functions, the
/// chip is presentation-only, `met` restyles but never disables.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HabitRow {
    /// The habit's id (the id table's entry).
    pub habit: taskboard_domain::HabitId,
    /// Display title.
    pub title: String,
    /// The quota phrase (`3/day`).
    pub target_phrase: String,
    /// The current window's stored count.
    pub count: u32,
    /// The quota (≥ 1).
    pub quota: u32,
    /// The streak (the conversion seam unioned in).
    pub streak: u32,
    /// The display chip; in no ordering path anywhere (D-C13).
    pub chip: Option<chrono::NaiveTime>,
    /// The make-up affordance's mount flag (issue #506, plan D-7):
    /// `habit_repair_target` answered `Some` — the button mounts (in
    /// the edit cluster's gate), the dispatch carries the tuple.
    pub repair: bool,
    /// The restyle flag.
    pub met: bool,
}

/// The today view's habit rows (pure): the active board's **live**
/// habits in stored manual `(order, id)` order (D-C13 — the negative
/// `anchor_time` property extends to this layer), the count read from
/// the current window's row, the streak the domain walk unioned
/// through the conversion seam.
#[must_use]
pub fn habit_rows(
    state: &taskboard_domain::AppState,
    board: taskboard_domain::BoardId,
    now: DateTime<Utc>,
    day: taskboard_domain::DayCalendar,
) -> Vec<HabitRow> {
    let today = day.today(now);
    let mut live: Vec<&taskboard_domain::Habit> = state
        .habits
        .habits
        .values()
        .filter(|h| h.is_live() && h.board == board)
        .collect();
    live.sort_by_key(|h| (h.order, h.id));
    live.iter()
        .map(|habit| {
            let window = taskboard_domain::WindowStart::of_period(habit.target.kind(), today);
            let key = taskboard_domain::HabitLogKey {
                habit: habit.id,
                window,
            };
            let count = state.habits.log.get(&key).map_or(0, |e| e.count);
            let quota = habit.target.quota().get();
            HabitRow {
                habit: habit.id,
                title: habit.title.clone(),
                target_phrase: habit_target_phrase(habit.target),
                count,
                quota,
                streak: taskboard_domain::habit_streak_unioned(
                    habit,
                    &state.habits.log,
                    &state.tasks,
                    now,
                    day,
                ),
                chip: habit.anchor_time,
                repair: habit_repair_target(habit, &state.habits.log, now, day).is_some(),
                met: count >= quota,
            }
        })
        .collect()
}

/// The quota phrase (`3/day`, `1/week`, `12/month`).
#[must_use]
pub fn habit_target_phrase(target: taskboard_domain::HabitTarget) -> String {
    use taskboard_domain::PeriodKind;
    let unit = match target.kind() {
        PeriodKind::Day => "day",
        PeriodKind::Week => "week",
        PeriodKind::Month => "month",
    };
    format!("{}/{}", target.quota().get(), unit)
}

/// The make-up affordance's target (issue #506, plan D-7): `Some`
/// while the previous window is under quota **and** still within its
/// 24 h overflow grace (the D-C8 arithmetic, read-side only — the
/// same constants [`taskboard_domain::habit::log_window`] composes;
/// the domain function is called, never edited). The tuple is the
/// exact `AdjustHabitLog` payload the button dispatches: the previous
/// window and its count plus one — what you click is what increments,
/// no re-derivation at dispatch. The attribution decision itself
/// stays the domain's.
#[must_use]
pub fn habit_repair_target(
    habit: &taskboard_domain::habit::Habit,
    log: &std::collections::BTreeMap<
        taskboard_domain::HabitLogKey,
        taskboard_domain::HabitLogEntry,
    >,
    now: DateTime<Utc>,
    day: taskboard_domain::days::DayCalendar,
) -> Option<(taskboard_domain::WindowStart, u32)> {
    let kind = habit.target.kind();
    let quota = habit.target.quota().get();
    let natural = taskboard_domain::WindowStart::of_period(kind, day.today(now));
    let prev_date = taskboard_domain::habit::period_before(kind, natural.as_date());
    let grace_end =
        taskboard_domain::habit::period_end(kind, prev_date, day) + TimeDelta::hours(24);
    if now >= grace_end {
        return None;
    }
    let prev = taskboard_domain::WindowStart::of_period(kind, prev_date);
    let count = log
        .get(&taskboard_domain::HabitLogKey {
            habit: habit.id,
            window: prev,
        })
        .map_or(0, |entry| entry.count);
    (count < quota).then_some((prev, count + 1))
}

/// The conversion flow's default quota (issue #506, plan D-2): the
/// series pattern's pre-selected chip — `Daily(n) → PerDay(1)`,
/// `Weekly{..} → PerWeek(1)`, `Monthly{..} → PerMonth(1)`. A starting
/// point, never silent: the confirm strip names the mapping.
#[must_use]
/// # Panics
///
/// Only via the constant mint's `expect` (`1` is in the mint's
/// domain by construction) — never an input condition.
pub fn convert_target_default(
    pattern: &taskboard_domain::recur::RecurPattern,
) -> taskboard_domain::habit::HabitTarget {
    use taskboard_domain::habit::HabitTarget;
    use taskboard_domain::recur::RecurPattern as P;
    let one = taskboard_domain::NonZeroCount::new(1).expect("1 is in the mint's domain");
    match pattern {
        P::Daily { .. } => HabitTarget::PerDay(one),
        P::Weekly { .. } => HabitTarget::PerWeek(one),
        P::Monthly { .. } => HabitTarget::PerMonth(one),
    }
}

/// The reverse default (issue #506, plan D-2): the quota's series
/// mapping — `PerDay(n) → Daily(n)`, `PerWeek(n) → Weekly on
/// Monday`, `PerMonth(n) → Monthly on the 1st`. The quota has no
/// series analog (a member is a completion, not a quota) — the
/// weekday/on-day anchors stay at the default, editable later through
/// the repeat surface.
#[must_use]
/// # Panics
///
/// Only via the constant/default mints' `expect`s (`1` and a
/// quota-carried count are in the mints' domains by construction) —
/// never an input condition.
pub fn convert_pattern_default(
    target: taskboard_domain::habit::HabitTarget,
) -> taskboard_domain::recur::RecurPattern {
    use taskboard_domain::recur::{
        DayOfMonth, EveryDays, EveryWeeks, MonthlyAnchor, RecurPattern, WeekdaySet,
    };
    match target {
        taskboard_domain::habit::HabitTarget::PerDay(n) => RecurPattern::Daily {
            every: EveryDays::new(u16::try_from(n.get()).unwrap_or(u16::MAX))
                .expect("the quota carried a minted count"),
        },
        taskboard_domain::habit::HabitTarget::PerWeek(_) => RecurPattern::Weekly {
            interval: EveryWeeks::new(1).expect("1 is in the mint's domain"),
            on: WeekdaySet::new([chrono::Weekday::Mon]).expect("one weekday is a set"),
        },
        taskboard_domain::habit::HabitTarget::PerMonth(_) => RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(DayOfMonth::new(1).expect("1 is in the mint's domain")),
        },
    }
}

/// The row's progress phrase ("water 5/8"): a presentation-side
/// derivation from the stored `count`/`quota` — never a state column.
#[must_use]
pub fn habit_progress_phrase(row: &HabitRow) -> String {
    format!("{}/{}", row.count, row.quota)
}

/// The kiosk gate (D-P13/D-T16): the gate empties the ACTION, never
/// the model — the rows render on the appliance (passive progress
/// visibility IS the section's point), the +1/reorder affordances do
/// not.
#[must_use]
pub const fn habit_editing_visible(kiosk: bool) -> bool {
    !kiosk
}

/// The reorder's drop-slot order: the **floor midpoint of the drop
/// slot's free interval** over the row source's orders (the
/// `insertion_order_after` convention, D-C13 — the UI computes
/// midpoints, the planner stamps the absolute value). `prev`/`next`
/// are the orders bracketing the slot (`None` at either end).
#[must_use]
pub fn habit_drop_order(prev: Option<i64>, next: Option<i64>) -> i64 {
    match (prev, next) {
        (None, None) => 0,
        (None, Some(next)) => next - 1,
        (Some(prev), None) => prev + 1,
        (Some(prev), Some(next)) => prev + (next - prev) / 2,
    }
}

#[cfg(test)]
mod habit_section_tests {
    use super::*;
    use taskboard_domain::{
        AppState, Habit, HabitClocks, HabitLogEntry, HabitLogKey, HabitTarget, NonZeroCount,
        PeriodKind, WindowStart,
    };

    fn ts(secs: i64) -> DateTime<Utc> {
        chrono::Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn day() -> taskboard_domain::DayCalendar {
        taskboard_domain::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).unwrap(),
        }
    }

    fn habit(raw: u128, board: u128, order: i64, chip: Option<chrono::NaiveTime>) -> Habit {
        Habit {
            id: taskboard_domain::ids::HabitId::from(uuid::Uuid::from_u128(raw)),
            board: taskboard_domain::BoardId::from(uuid::Uuid::from_u128(board)),
            title: format!("habit {raw}"),
            target: HabitTarget::PerDay(NonZeroCount::new(3).unwrap()),
            order,
            anchor_time: chip,
            clocks: HabitClocks::all(ts(100)),
            continued_from: None,
            deleted: false,
        }
    }

    /// T-18: stored `(order, id)` order; the `anchor_time` perturbation
    /// negative property at the presentation layer; the kiosk gate
    /// empties the actions, not the rows.
    #[test]
    fn habit_rows_render_stored_order_and_kiosk_gates_the_actions_only() {
        let board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
        let mut state = AppState::default();
        let a = habit(1, 2, 5, None);
        let mut b = habit(
            2,
            2,
            9,
            Some(chrono::NaiveTime::from_hms_opt(7, 0, 0).unwrap()),
        );
        b.title = "b".into();
        let foreign = habit(3, 3, 0, None);
        for h in [b, foreign, a.clone()] {
            state.habits.habits.insert(h.id, h);
        }
        let now = ts(1_700_000_000);
        let rows = habit_rows(&state, board, now, day());
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec!["habit 1", "b"],
            "stored (order, id) ascending; the foreign board never renders"
        );
        // The negative pin: perturbing anchor_time leaves the row order
        // and every rendered field but the chip unchanged.
        let mut perturbed = state.clone();
        let mut chiped = a.clone();
        chiped.anchor_time = Some(chrono::NaiveTime::from_hms_opt(23, 59, 0).unwrap());
        perturbed.habits.habits.insert(chiped.id, chiped);
        let chip_rows = habit_rows(&perturbed, board, now, day());
        for (x, y) in rows.iter().zip(chip_rows.iter()) {
            assert_eq!(x.title, y.title);
            assert_eq!(x.count, y.count);
            assert_eq!(x.streak, y.streak);
            assert_eq!(x.met, y.met);
        }
        assert!(chip_rows[0].chip.is_some());
        // The count derives from the domain row read (0 — nothing logged).
        assert_eq!(rows[0].count, 0);
        assert!(!rows[0].met);
        // The kiosk gate empties the actions, never the rows.
        assert!(habit_editing_visible(false));
        assert!(!habit_editing_visible(true));
        assert_eq!(habit_rows(&state, board, now, day()).len(), 2);
    }

    /// T-18 (continued): count/quota/streak/met derive from the domain
    /// functions — a logged window moves the count and `met`.
    #[test]
    fn habit_rows_derive_progress_from_the_domain_row() {
        let board = taskboard_domain::BoardId::from(uuid::Uuid::from_u128(2));
        let mut state = AppState::default();
        let a = habit(1, 2, 0, None);
        let today = day().today(ts(1_700_000_000));
        state.habits.habits.insert(a.id, a.clone());
        state.habits.log.insert(
            HabitLogKey {
                habit: a.id,
                window: WindowStart::of_period(PeriodKind::Day, today),
            },
            HabitLogEntry {
                count: 3,
                clock: ts(200),
            },
        );
        let rows = habit_rows(&state, board, ts(1_700_000_000), day());
        assert_eq!(rows[0].count, 3);
        assert_eq!(rows[0].quota, 3);
        assert!(rows[0].met);
        assert_eq!(rows[0].streak, 1, "today is met: the walk counts it");
        assert_eq!(habit_progress_phrase(&rows[0]), "3/3");
    }

    /// T-19 (the order law): the drop slot's floor midpoint — one law
    /// with the CLI's `--order` path at the planner.
    #[test]
    fn habit_drop_order_takes_the_floor_midpoint_of_the_free_interval() {
        assert_eq!(habit_drop_order(None, None), 0);
        assert_eq!(habit_drop_order(None, Some(10)), 9);
        assert_eq!(habit_drop_order(Some(10), None), 11);
        assert_eq!(habit_drop_order(Some(10), Some(20)), 15);
        assert_eq!(
            habit_drop_order(Some(10), Some(11)),
            10,
            "a tied gap floors to the lower bound"
        );
    }

    // -- the concentration overlay (#496): pure derivations ------------

    use taskboard_domain::{ids::TaskId, tracking::TimeEntry};

    fn overlay_entry(
        kind: taskboard_domain::tracking::TimeEntryKind,
        start: DateTime<Utc>,
    ) -> TimeEntry {
        TimeEntry {
            id: taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            task: TaskId::from_uuid(uuid::Uuid::now_v7()),
            task_title: "the subject".into(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
            kind,
        }
    }

    fn overlay_state(
        kind: taskboard_domain::tracking::TimeEntryKind,
        start: DateTime<Utc>,
    ) -> AppState {
        let entry = overlay_entry(kind, start);
        AppState {
            tracking: taskboard_domain::TrackingState {
                entries: [(entry.id, entry)].into_iter().collect(),
            },
            ..AppState::default()
        }
    }

    fn overlay_timing() -> FocusTiming {
        FocusTiming::try_new(
            TimeDelta::minutes(25),
            TimeDelta::minutes(5),
            TimeDelta::minutes(15),
            4,
        )
        .expect("positive timings")
    }

    const OVERLAY_T0: DateTime<Utc> =
        chrono::DateTime::<Utc>::from_timestamp(1_792_000_000, 0).expect("valid instant");

    #[test]
    fn focus_overlay_derives_phase_and_remaining() {
        let timing = overlay_timing();

        // Idle: nothing open.
        let overlay = focus_overlay(&AppState::default(), &timing, OVERLAY_T0);
        assert_eq!(overlay.phase, FocusPhase::Idle);
        assert_eq!(overlay.remaining_text, "");
        assert_eq!(overlay.task_title, "");

        // Focus: the work block's remaining time, boundary buckets.
        let state = overlay_state(taskboard_domain::tracking::TimeEntryKind::Work, OVERLAY_T0);
        let overlay = focus_overlay(&state, &timing, OVERLAY_T0 + TimeDelta::seconds(1));
        assert_eq!(overlay.phase, FocusPhase::Focus);
        assert_eq!(overlay.remaining_text, "24:59");
        assert_eq!(overlay.task_title, "the subject");
        let overlay = focus_overlay(&state, &timing, OVERLAY_T0);
        assert_eq!(overlay.remaining_text, "25:00", "the full block at t0");
        // Past the deadline: floor-clamped, never negative.
        let overlay = focus_overlay(&state, &timing, OVERLAY_T0 + TimeDelta::hours(2));
        assert_eq!(overlay.remaining_text, "00:00");

        // Break: the respite block arms its own length.
        let state = overlay_state(
            taskboard_domain::tracking::TimeEntryKind::Respite,
            OVERLAY_T0,
        );
        let overlay = focus_overlay(&state, &timing, OVERLAY_T0 + TimeDelta::seconds(1));
        assert_eq!(overlay.phase, FocusPhase::Break);
        assert_eq!(overlay.remaining_text, "04:59");
    }

    /// A walk-seeded state: `rows` are (kind, start, end) minutes
    /// relative to `OVERLAY_T0` (end `None` = open), plus the given
    /// open entry.
    fn overlay_state_seeded(
        rows: &[(taskboard_domain::tracking::TimeEntryKind, i64, Option<i64>)],
        open_kind: taskboard_domain::tracking::TimeEntryKind,
        open_start_min: i64,
    ) -> AppState {
        use taskboard_domain::ids::{TaskId, TimeEntryId};
        let mut entries = std::collections::BTreeMap::new();
        for (kind, start, end) in rows {
            let e = TimeEntry {
                id: TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
                task: TaskId::from_uuid(uuid::Uuid::now_v7()),
                task_title: "the subject".into(),
                start: OVERLAY_T0 + TimeDelta::minutes(*start),
                end: end.map(|mins| OVERLAY_T0 + TimeDelta::minutes(mins)),
                created_at: OVERLAY_T0,
                updated_at: OVERLAY_T0,
                kind: *kind,
            };
            entries.insert(e.id, e);
        }
        let mut open = TimeEntry {
            id: TimeEntryId::from_uuid(uuid::Uuid::now_v7()),
            task: TaskId::from_uuid(uuid::Uuid::now_v7()),
            task_title: "the subject".into(),
            start: OVERLAY_T0 + TimeDelta::minutes(open_start_min),
            end: None,
            created_at: OVERLAY_T0,
            updated_at: OVERLAY_T0,
            kind: open_kind,
        };
        open.created_at = open.start;
        open.updated_at = open.start;
        entries.insert(open.id, open.clone());
        AppState {
            tracking: taskboard_domain::TrackingState { entries },
            ..AppState::default()
        }
    }

    #[test]
    fn focus_overlay_flags_the_long_break_and_derives_its_deadline() {
        let timing = overlay_timing(); // 25 / 5 / 15 / every 4

        // Cold break: position 0 ⇒ short, unchanged.
        let state =
            overlay_state_seeded(&[], taskboard_domain::tracking::TimeEntryKind::Respite, 0);
        let overlay = focus_overlay(&state, &timing, OVERLAY_T0 + TimeDelta::seconds(1));
        assert!(!overlay.long_break);
        assert_eq!(
            overlay.remaining_text, "04:59",
            "the cold break keeps the short length"
        );

        // Three closed works (short breaks between) put the open
        // break at cadence position 4 ⇒ long: remaining counts to
        // start + 15m, and the structural flag carries it.
        let rows = [
            (
                taskboard_domain::tracking::TimeEntryKind::Work,
                0i64,
                Some(25i64),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Respite,
                25,
                Some(30),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Work,
                30,
                Some(55),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Respite,
                55,
                Some(60),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Work,
                60,
                Some(85),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Respite,
                85,
                Some(90),
            ),
            (
                taskboard_domain::tracking::TimeEntryKind::Work,
                90,
                Some(115),
            ),
        ];
        let state = overlay_state_seeded(
            &rows,
            taskboard_domain::tracking::TimeEntryKind::Respite,
            115,
        );
        let overlay = focus_overlay(
            &state,
            &timing,
            OVERLAY_T0 + TimeDelta::minutes(115) + TimeDelta::seconds(1),
        );
        assert!(
            overlay.long_break,
            "position 4: the break is structurally long"
        );
        assert_eq!(overlay.phase, FocusPhase::Break);
        assert_eq!(
            overlay.remaining_text, "14:59",
            "the long break counts to start + long_break, not respite"
        );

        // The same history under an open WORK entry: the flag stays
        // false and the work length holds (the walk never touches the
        // focus path's deadline).
        let state =
            overlay_state_seeded(&rows, taskboard_domain::tracking::TimeEntryKind::Work, 115);
        let overlay = focus_overlay(
            &state,
            &timing,
            OVERLAY_T0 + TimeDelta::minutes(115) + TimeDelta::seconds(1),
        );
        assert!(!overlay.long_break);
        assert_eq!(overlay.remaining_text, "24:59");
    }

    #[allow(clippy::too_many_lines)] // the task fixture dominates the count
    #[test]
    fn focus_switcher_rows_are_live_not_done_in_board_order_bounded() {
        let timing = overlay_timing();
        let _ = timing;
        let mut state = AppState::default();
        let board_a = taskboard_domain::BoardId::from_uuid(uuid::Uuid::now_v7());
        let stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::now_v7());
        let now = taskboard_domain::BoardClocks::epoch();
        state.boards.insert(
            board_a,
            taskboard_domain::Board {
                id: board_a,
                remote: None,
                title: "a".into(),
                color: taskboard_domain::Color::new(""),
                archived: false,
                deleted: false,
                clocks: taskboard_domain::BoardClocks {
                    title: now,
                    color: now,
                    archived: now,
                    deleted: now,
                },
                authors: taskboard_domain::entities::BoardAuthors::default(),
                remote_seen: None,
            },
        );
        state.stacks.insert(
            stack,
            taskboard_domain::Stack {
                id: stack,
                remote: None,
                board: board_a,
                title: "s".into(),
                order: 1,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::StackClocks {
                    title: now,
                    order: now,
                    deleted: now,
                },
                authors: taskboard_domain::entities::StackAuthors::default(),
                remote_seen: None,
            },
        );
        let mk = |title: &str, order: i64, done: bool| {
            let mut task = taskboard_domain::entities::Task {
                id: TaskId::from_uuid(uuid::Uuid::now_v7()),
                remote: None,
                title: title.into(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack,
                order,
                label_memberships: std::collections::BTreeMap::new(),
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,
                archived: false,
                deleted: false,
                clocks: taskboard_domain::entities::TaskClocks {
                    title: now,
                    description: now,
                    duedate: now,
                    scheduled: now,
                    done: now,
                    position: now,
                    effort: now,
                    importance: now,
                    urgency: now,
                    recur: now,
                    kind: now,
                    archived: now,
                    deleted: now,
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            };
            if done {
                task.done = Some(OVERLAY_T0);
            }
            task
        };
        let done = mk("done", 1, true);
        let done_id = done.id;
        state.tasks.insert(done_id, done);
        let second = mk("second", 2, false);
        let second_id = second.id;
        state.tasks.insert(second_id, second);
        let first = mk("first", 1, false);
        let first_id = first.id;
        state.tasks.insert(first_id, first);

        let rows = focus_switcher_rows(&state);
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            ["first", "second"],
            "reading order (order asc, id tie-break), done filtered"
        );
        assert_eq!(rows[0].task, first_id);

        // The bound: an 11-row pool caps at nine.
        for i in 0..9 {
            let t = mk(&format!("filler-{i}"), 10 + i, false);
            state.tasks.insert(t.id, t);
        }
        let rows = focus_switcher_rows(&state);
        assert_eq!(rows.len(), FOCUS_SWITCHER_CAP);

        // The empty pool: no live not-done tasks anywhere.
        let empty = focus_switcher_rows(&AppState::default());
        assert_eq!(empty, [] as [FocusSwitcherRow; 0]);
    }
}

// ---- The tracking surfaces' pure layer (r4 chunk 10, slice 1) ----------

#[cfg(test)]
mod track_surfaces_tests {
    use super::*;
    use chrono::{FixedOffset, LocalResult, Timelike};
    use taskboard_domain::TaskClocks;
    use taskboard_domain::tracking::TimeEntry;

    fn tz() -> FixedOffset {
        FixedOffset::east_opt(2 * 3600).expect("fixed +02:00")
    }

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(raw: u128, task: TaskId, start: i64, end: Option<i64>) -> TimeEntry {
        TimeEntry {
            id: taskboard_domain::TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task,
            task_title: "t".into(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(0),
            updated_at: ts(0),
            kind: taskboard_domain::tracking::TimeEntryKind::Work,
        }
    }

    fn respite(raw: u128, task: TaskId, start: i64, end: Option<i64>) -> TimeEntry {
        TimeEntry {
            kind: taskboard_domain::tracking::TimeEntryKind::Respite,
            ..entry(raw, task, start, end)
        }
    }

    #[test]
    fn entry_rows_render_newest_first_with_the_running_and_plus_day_marks() {
        let zone = tz();
        let task = TaskId::from(uuid::Uuid::from_u128(7));
        let mut state = AppState::default();
        // 10:00–11:00 local (+02:00) on the 8th; end rolls past local
        // midnight? No — a same-day span first, then a crossing one.
        let same_day = entry(1, task, 1_000, Some(1_900));
        // 23:30 local to 00:30 local next day (21:30Z → 22:30Z the
        // next day).
        let day = zone
            .with_ymd_and_hms(2026, 11, 8, 23, 30, 0)
            .single()
            .expect("resolvable wall-clock");
        let end = day + TimeDelta::hours(1);
        let crossing = TimeEntry {
            start: day.with_timezone(&Utc),
            end: Some(end.with_timezone(&Utc)),
            ..entry(2, task, 0, Some(0))
        };
        let running = entry(3, task, 5_000, None);
        for row in [&same_day, &crossing, &running] {
            state.tracking.entries.insert(row.id, row.clone());
        }
        // A respite row renders honestly (history is history)…
        let respite_row = respite(4, task, 2_000, Some(2_400));
        let respite_id = respite_row.id;
        state.tracking.entries.insert(respite_id, respite_row);
        // …and another task's row never renders here.
        let stranger = entry(
            5,
            TaskId::from(uuid::Uuid::from_u128(9)),
            9_000,
            Some(9_100),
        );
        state.tracking.entries.insert(stranger.id, stranger);

        let now = ts(6_000);
        let rows = track_entry_rows(&state, task, now, &zone);
        assert_eq!(rows.len(), 4, "all of the subject's rows, kinds included");
        // Newest-first by (start, id): crossing (2026) > running
        // (5000) > respite (2000) > same_day (1970).
        assert_eq!(rows[0].id, crossing.id);
        assert!(
            rows[0].end_other_day,
            "the crossing span carries the mark bit"
        );
        assert_eq!(rows[0].date, "2026-11-08");
        assert_eq!(rows[0].start, "23:30");
        assert_eq!(rows[0].end, "00:30");
        assert_eq!(rows[1].id, running.id);
        assert!(rows[1].running);
        assert_eq!(
            rows[1].end, TRACK_RUNNING_END,
            "the open end names its state"
        );
        assert_eq!(
            rows[1].spent, "16m 40s",
            "the phrase vocabulary is the summary row's"
        );
        assert_eq!(rows[2].id, respite_id, "a respite row is still history");
        assert_eq!(rows[3].id, same_day.id);
        assert!(!rows[3].end_other_day);
    }

    #[test]
    fn the_form_parse_maps_each_rejection_to_its_field() {
        let zone = tz();
        let today = NaiveDate::from_ymd_opt(2026, 11, 8).expect("today");
        // Unparseable start → start invalid, no draft.
        let got = parse_track_form("2026-11-08", "9:5", "10:00", false, today, &zone);
        assert_eq!(got.start_valid, Some(false));
        assert_eq!(got.rejected_by, Some(TrackField::Start));
        assert_eq!(got.draft, None);
        // Unparseable end → end invalid.
        let got = parse_track_form("", "09:00", "nope", false, today, &zone);
        assert_eq!(got.end_valid, Some(false));
        assert_eq!(got.rejected_by, Some(TrackField::End));
        assert_eq!(got.date_valid, None, "empty date stays neutral (today)");
        // Unparseable date → date invalid, named.
        let got = parse_track_form("2026-13-40", "09:00", "10:00", false, today, &zone);
        assert_eq!(got.rejected_by, Some(TrackField::Date));
        // The pair rejection lands on the END field; commit refused.
        let got = parse_track_form("", "10:00", "09:59", false, today, &zone);
        assert_eq!(got.start_valid, Some(true));
        assert_eq!(got.end_valid, Some(false));
        assert_eq!(got.rejected_by, Some(TrackField::End));
        assert_eq!(got.draft, None);
        // The empty span is the same typed rejection.
        let got = parse_track_form("", "10:00", "10:00", false, today, &zone);
        assert_eq!(got.rejected_by, Some(TrackField::End));
        // Seeded neutral: both times empty — no draft, no rejection.
        let got = parse_track_form("", "--:--", "", false, today, &zone);
        assert_eq!(
            got,
            TrackFormParse {
                date_valid: None,
                start_valid: None,
                end_valid: None,
                rejected_by: None,
                draft: None,
            }
        );
    }

    #[test]
    fn the_draft_resolves_local_to_store_utc_and_carries_the_mask_shape() {
        let zone = tz();
        let today = NaiveDate::from_ymd_opt(2026, 11, 8).expect("today");
        // Add: empty date resolves today; a typed end makes a closed
        // span at exact UTC instants (+02:00 → minus two hours).
        let got = parse_track_form("", "10:00", "11:30", false, today, &zone);
        let draft = got.draft.expect("the add commit is draft-ready");
        assert_eq!(
            draft.start,
            zone.with_ymd_and_hms(2026, 11, 8, 10, 0, 0)
                .single()
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            draft.end,
            Some(
                zone.with_ymd_and_hms(2026, 11, 8, 11, 30, 0)
                    .single()
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
        // Add with the end left empty: no draft (a LogTime span is
        // always closed), the field neutral.
        let got = parse_track_form("", "10:00", "", false, today, &zone);
        assert_eq!(got.draft, None);
        assert_eq!(got.end_valid, None);
        assert_eq!(got.start_valid, Some(true));
        // Edit with the end empty: the keep-open mask is draft-ready.
        let got = parse_track_form("", "10:00", "", true, today, &zone);
        let draft = got.draft.expect("the edit commit is draft-ready");
        assert_eq!(draft.end, None, "the mask's end is None (keep open)");
        // The lenient digit forms ride the same parser.
        let got = parse_track_form("", "930", "0945", false, today, &zone);
        let draft = got.draft.expect("lenient forms parse");
        assert_eq!(
            draft.start,
            zone.with_ymd_and_hms(2026, 11, 8, 9, 30, 0)
                .single()
                .unwrap()
                .with_timezone(&Utc)
        );
        assert_eq!(
            draft.end,
            Some(
                zone.with_ymd_and_hms(2026, 11, 8, 9, 45, 0)
                    .single()
                    .unwrap()
                    .with_timezone(&Utc)
            )
        );
    }

    /// A zone with a hard-coded spring-forward gap: `02:00–03:00`
    /// local on 2026-03-29 does not exist — the resolution refuses.
    #[derive(Clone, Copy)]
    struct GapZone(FixedOffset);
    impl TimeZone for GapZone {
        type Offset = FixedOffset;
        fn from_offset(offset: &Self::Offset) -> Self {
            GapZone(*offset)
        }
        fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> Self::Offset {
            self.0.offset_from_utc_datetime(utc)
        }
        fn offset_from_utc_date(&self, utc: &chrono::NaiveDate) -> Self::Offset {
            self.0.offset_from_utc_date(utc)
        }
        fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<Self::Offset> {
            let gap_day = NaiveDate::from_ymd_opt(2026, 3, 29).expect("gap day");
            if local.date() == gap_day && local.time().hour() == 2 {
                LocalResult::None
            } else {
                LocalResult::Single(self.0)
            }
        }
        fn offset_from_local_date(&self, _local: &chrono::NaiveDate) -> LocalResult<Self::Offset> {
            LocalResult::Single(self.0)
        }
    }

    #[test]
    fn a_dst_gap_wall_clock_is_a_typed_field_refusal() {
        let zone = GapZone(tz());
        let today = NaiveDate::from_ymd_opt(2026, 3, 29).expect("today");
        // 02:30 local does not exist on the gap day: the start field
        // takes the typed rejection (never a guessed instant).
        let got = parse_track_form("", "02:30", "04:00", false, today, &zone);
        assert_eq!(got.start_valid, Some(false));
        assert_eq!(got.rejected_by, Some(TrackField::Start));
        assert_eq!(got.draft, None);
        // The same wall-clock on a normal day resolves.
        let normal = NaiveDate::from_ymd_opt(2026, 6, 1).expect("normal");
        let got = parse_track_form("", "02:30", "04:00", false, normal, &zone);
        assert!(got.draft.is_some(), "a resolvable wall-clock drafts");
    }

    #[test]
    #[allow(clippy::too_many_lines)] // one fixture over four rulings
    fn tracked_today_rows_are_work_only_in_canonical_order_and_clip_to_the_window() {
        let task_a = TaskId::from(uuid::Uuid::from_u128(1));
        let task_b = TaskId::from(uuid::Uuid::from_u128(2));
        let task_c = TaskId::from(uuid::Uuid::from_u128(3));
        let mut state = AppState::default();
        // Canonical (map key) order: insert c, a, b — the output must
        // still read a, b, c.
        for id in [task_c, task_a, task_b] {
            let task = Task {
                id,
                remote: None,
                label_memberships: std::collections::BTreeMap::new(),
                title: "t".into(),
                description: String::new(),
                duedate: None,
                scheduled: None,
                done: None,
                stack: StackId::from(uuid::Uuid::from_u128(50)),
                order: 0,
                effort: None,
                importance: None,
                urgency: None,
                recur: None,
                kind: taskboard_domain::entities::TaskKind::Standard,
                archived: false,
                deleted: false,
                clocks: TaskClocks {
                    title: ts(0),
                    description: ts(0),
                    duedate: ts(0),
                    scheduled: ts(0),
                    done: ts(0),
                    position: ts(0),
                    effort: ts(0),
                    importance: ts(0),
                    urgency: ts(0),
                    recur: ts(0),
                    kind: ts(0),
                    archived: ts(0),
                    deleted: ts(0),
                },
                authors: taskboard_domain::entities::TaskAuthors::default(),
                remote_seen: None,
                remote_baseline: None,
            };
            state.tasks.insert(id, task);
        }
        // Window 08:00Z–16:00Z. task_a: a spanning entry 07:00–09:00
        // contributes its in-window hour only.
        state.tracking.entries.insert(
            entry(1, task_a, 7 * 3600, Some(9 * 3600)).id,
            entry(1, task_a, 7 * 3600, Some(9 * 3600)),
        );
        // task_b: work + respite — the respite contributes zero.
        let work = entry(2, task_b, 10 * 3600, Some(11 * 3600));
        state.tracking.entries.insert(work.id, work);
        let respite_row = respite(3, task_b, 12 * 3600, Some(15 * 3600));
        state.tracking.entries.insert(respite_row.id, respite_row);
        // An open entry accrues clamped at `now` (inside the window).
        let running = entry(4, task_c, 14 * 3600, None);
        state.tracking.entries.insert(running.id, running);
        // A tombstoned task's surviving history renders no row (the
        // today-section pattern's is_live filter — history outlives
        // the task, the today view does not).
        let task_d = TaskId::from(uuid::Uuid::from_u128(4));
        let tombstone = Task {
            id: task_d,
            remote: None,
            label_memberships: std::collections::BTreeMap::new(),
            title: "gone".into(),
            description: String::new(),
            duedate: None,
            scheduled: None,
            done: None,
            stack: StackId::from(uuid::Uuid::from_u128(50)),
            order: 0,
            effort: None,
            importance: None,
            urgency: None,
            recur: None,
            kind: taskboard_domain::entities::TaskKind::Standard,
            archived: false,
            deleted: true,
            clocks: TaskClocks {
                title: ts(0),
                description: ts(0),
                duedate: ts(0),
                scheduled: ts(0),
                done: ts(0),
                position: ts(0),
                effort: ts(0),
                importance: ts(0),
                urgency: ts(0),
                recur: ts(0),
                kind: ts(0),
                archived: ts(0),
                deleted: ts(0),
            },
            authors: taskboard_domain::entities::TaskAuthors::default(),
            remote_seen: None,
            remote_baseline: None,
        };
        state.tasks.insert(task_d, tombstone);
        let gone_work = entry(5, task_d, 9 * 3600, Some(10 * 3600));
        state.tracking.entries.insert(gone_work.id, gone_work);

        let window = (ts(8 * 3600), ts(16 * 3600));
        let now = ts(15 * 3600);
        let rows = tracked_today_rows(&state, window, now);
        assert_eq!(
            rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            [task_a, task_b, task_c],
            "canonical order, workless and tombstoned tasks absent"
        );
        assert_eq!(rows[0].1, TimeDelta::hours(1), "the spanning entry clips");
        assert_eq!(rows[1].1, TimeDelta::hours(1), "respite never counts");
        assert_eq!(rows[2].1, TimeDelta::hours(1), "open accrual clamps at now");
        // An empty day is the empty vec (the collapse's input). (An
        // open entry genuinely accrues into any later window — the
        // domain oracle's own semantics; the collapse pin is a state
        // without entries.)
        let empty = tracked_today_rows(
            &AppState::default(),
            (ts(90 * 3600), ts(98 * 3600)),
            ts(95 * 3600),
        );
        assert_eq!(empty, [] as [(TaskId, TimeDelta); 0]);
    }

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(32))]

        /// Reconciliation by construction (D-TS3): the overlay's rows
        /// sum (Work entries at lifetime scope) to exactly the summary
        /// row's total — one reduction, never a parallel one.
        #[test]
        fn overlay_row_sum_reconciles_with_the_summary_total(
            starts in proptest::collection::vec(0_i64..500_000, 0..6),
            spans in proptest::collection::vec(1_i64..20_000, 0..6),
            open in proptest::collection::vec(any::<bool>(), 0..6),
            now_secs in 500_000_i64..600_000,
        ) {
            let task = TaskId::from(uuid::Uuid::from_u128(42));
            let mut state = AppState::default();
            for (i, ((start, span), is_open)) in
                starts.iter().zip(&spans).zip(&open).enumerate()
            {
                let entry = TimeEntry {
                    start: ts(*start),
                    end: if *is_open { None } else { Some(ts(start + span)) },
                    ..entry(i as u128 + 1, task, *start, None)
                };
                state.tracking.entries.insert(entry.id, entry);
            }
            let now = ts(now_secs);
            let overlay = track_entry_rows(&state, task, now, &tz())
                .into_iter()
                .map(|row| row.spent)
                .collect::<Vec<_>>();
            let total = tracked_total(&state, task, now);
            // Structural reconciliation: the header total re-derives
            // from the same durations the rows phrase.
            let sum: i64 = state
                .tracking
                .entries
                .values()
                .filter(|entry| {
                    entry.task == task
                        && entry.kind == taskboard_domain::tracking::TimeEntryKind::Work
                })
                .map(|entry| entry.duration(now).num_seconds())
                .sum();
            let phrase = tracked_total_phrase(&total);
            prop_assert_eq!(
                i64::from(total.hours) * 3600
                    + i64::from(total.minutes) * 60
                    + i64::from(total.seconds),
                sum
            );
            prop_assert_eq!(
                overlay.len(),
                state.tracking.entries.values().filter(|e| e.task == task).count()
            );
            prop_assert!(phrase.split(' ').all(|seg| seg.ends_with('h')
                || seg.ends_with('m')
                || seg.ends_with('s')));
        }
    }
}

#[cfg(test)]
mod review_tracking_tests {
    use super::*;
    use taskboard_domain::{DayTotal, WindowRatings, WindowSplit, WorkBlockReading};

    fn secs(n: i64) -> chrono::TimeDelta {
        chrono::TimeDelta::try_seconds(n).unwrap()
    }

    fn day(work: i64, respite: i64) -> DayTotal {
        DayTotal {
            date: chrono::NaiveDate::from_ymd_opt(2026, 11, 5).unwrap(),
            split: WindowSplit {
                work: secs(work),
                respite: secs(respite),
            },
        }
    }

    fn reading(blocks: usize, total: i64) -> WorkBlockReading {
        WorkBlockReading {
            blocks,
            total: secs(total),
        }
    }

    // P-1: the phrases + the collapse flag. Empty → all phrases empty
    // + the flag set; non-empty → phrases observed, distinct inputs
    // give distinct forms (structure, never text matching).
    #[test]
    fn review_tracking_section_phrases_and_collapse_flag() {
        let empty = WindowSplit {
            work: chrono::TimeDelta::zero(),
            respite: chrono::TimeDelta::zero(),
        };
        let collapsed = review_tracking_section(&empty, &reading(0, 0));
        assert!(collapsed.empty);
        assert_eq!(collapsed.work, "");
        assert_eq!(collapsed.respite, "");
        assert_eq!(collapsed.blocks, "");

        let split = WindowSplit {
            work: secs(3_600),
            respite: secs(600),
        };
        let populated = review_tracking_section(&split, &reading(2, 7_200));
        assert!(!populated.empty);
        assert_ne!(populated.work, "");
        assert_ne!(populated.respite, "");
        assert_ne!(populated.blocks, "");
        // Distinct inputs, distinct forms (the phrases carry the
        // magnitudes, unlike the fractions).
        let longer = review_tracking_section(
            &WindowSplit {
                work: secs(7_200),
                respite: secs(600),
            },
            &reading(2, 7_200),
        );
        assert_ne!(populated.work, longer.work);
        // A zero count is an observed fact, phrased — the dash is
        // reserved for the collapsed section.
        let unblocked = review_tracking_section(&split, &reading(0, 0));
        assert_ne!(unblocked.blocks, "");
        assert_ne!(unblocked.blocks, populated.blocks);
    }

    // P-2: the fractions normalize to the strip maximum — the max
    // day's segment sum ≈ 1.0, other days scaled, all-zero → all
    // zeros (never NaN), lengths preserved, nothing above 1.0.
    #[test]
    fn review_bar_fractions_normalize_to_the_strip_maximum() {
        let series = [day(3_600, 0), day(7_200, 600), day(0, 0)];
        let bars = review_bar_fractions(&series);
        assert_eq!(bars.len(), series.len());
        let max_sum = bars[1].work + bars[1].respite;
        assert!(
            (max_sum - 1.0_f32).abs() < 1e-6,
            "the max day fills the strip"
        );
        assert!(bars[0].work > 0.0 && bars[0].respite == 0.0);
        assert!(
            bars[0].work < bars[1].work,
            "smaller days scale down proportionally"
        );
        assert!(
            bars.iter().all(|bar| bar.work <= 1.0 && bar.respite <= 1.0),
            "fractions clamp at 1.0"
        );
        assert!(
            bars.iter()
                .all(|bar| bar.work.is_finite() && bar.respite.is_finite()),
            "never NaN"
        );

        // The all-zero strip: all zeros, never NaN.
        let zeros = review_bar_fractions(&[day(0, 0), day(0, 0)]);
        assert!(
            zeros
                .iter()
                .all(|bar| bar.work == 0.0 && bar.respite == 0.0)
        );

        assert_eq!(review_bar_fractions(&[]), Vec::<ReviewBar>::new());
    }

    // ---- #623 slice B: the reflection phrases + fractions ----------------

    fn ratings(entries: usize, mood: (u32, usize), energy: (u32, usize)) -> WindowRatings {
        WindowRatings {
            entries,
            mood: taskboard_domain::RatingMean {
                total: mood.0,
                count: mood.1,
            },
            energy: taskboard_domain::RatingMean {
                total: energy.0,
                count: energy.1,
            },
            flags: taskboard_domain::FlagOccurrences::default(),
        }
    }

    // 10: the section collapses only on zero entries; entries without
    // ratings dash the rating cells while the flags fact still speaks.
    #[test]
    fn review_reflection_section_phrases_collapse_only_on_zero_entries() {
        let collapsed = review_reflection_section(&ratings(0, (0, 0), (0, 0)));
        assert!(collapsed.empty);
        assert_eq!(collapsed.mood, "");
        assert_eq!(collapsed.energy, "");
        assert_eq!(collapsed.flags, "");
        assert_eq!(collapsed.per_flag, "");

        let unrated = review_reflection_section(&ratings(2, (0, 0), (0, 0)));
        assert!(!unrated.empty, "entries exist: the skeleton stands");
        assert_eq!(unrated.mood, "", "unanswered is the dash");
        assert_eq!(unrated.energy, "");
        assert_ne!(unrated.flags, "", "zero occurrences is an observed fact");
        assert_eq!(unrated.per_flag, "", "no flags → no counts line");

        let rated = review_reflection_section(&ratings(2, (7, 2), (4, 1)));
        assert_ne!(rated.mood, "");
        assert_ne!(rated.energy, "");
        assert_ne!(rated.mood, rated.energy, "distinct means, distinct forms");
        let mut with_flags = ratings(2, (7, 2), (4, 1));
        with_flags
            .flags
            .curated
            .insert(taskboard_domain::HealthFlag::Cramps, 2);
        let flagged = review_reflection_section(&with_flags);
        assert_ne!(flagged.per_flag, "");
        assert_ne!(flagged.flags, unrated.flags, "the total moved");
    }

    // 11: the counts line's deterministic double sort — count desc,
    // then name asc, extra names verbatim among the curated.
    #[test]
    fn the_per_flag_line_sorts_count_desc_then_name_asc() {
        let mut flags = taskboard_domain::FlagOccurrences::default();
        flags
            .curated
            .insert(taskboard_domain::HealthFlag::PoorSleep, 2);
        flags
            .curated
            .insert(taskboard_domain::HealthFlag::Allergies, 2);
        flags
            .curated
            .insert(taskboard_domain::HealthFlag::GoodDay, 3);
        flags.extra.insert("solar-flare".to_owned(), 2);
        let counts = reflection_flag_counts(&flags);
        assert_eq!(
            counts,
            vec![
                ("good-day".to_owned(), 3),
                ("allergies".to_owned(), 2),
                ("poor-sleep".to_owned(), 2),
                ("solar-flare".to_owned(), 2),
            ]
        );
        assert_eq!(
            reflection_flag_counts(&taskboard_domain::FlagOccurrences::default()),
            Vec::<(String, usize)>::new()
        );
    }

    fn rated_day(mood: (u32, usize), energy: (u32, usize)) -> taskboard_domain::DayRatings {
        taskboard_domain::DayRatings {
            date: chrono::NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            ratings: ratings(1, mood, energy),
        }
    }

    // 12: fixed-scale normalization over the stacked pair — mean 4 →
    // 0.4, a 5/5 pair fills the plot, unrated → 0.0, the pair never
    // exceeds 1.0, never NaN, no strip-max rescaling.
    #[test]
    fn review_rating_fractions_normalize_by_the_stacked_rating_scale() {
        let series = [
            rated_day((4, 1), (0, 0)),
            rated_day((5, 1), (10, 2)),
            rated_day((0, 0), (0, 0)),
        ];
        let bars = review_rating_fractions(&series);
        assert_eq!(bars.len(), series.len());
        assert!(
            (bars[0].mood - 0.4_f32).abs() < 1e-6,
            "mean 4 of the pair's 10"
        );
        assert_eq!(bars[0].energy, 0.0, "unrated is the baseline tick");
        assert!(
            (bars[1].mood + bars[1].energy - 1.0_f32).abs() < 1e-6,
            "a 5/5 pair fills the plot exactly"
        );
        assert!(bars[2].mood == 0.0 && bars[2].energy == 0.0);
        // No strip-max rescaling: a lone mean-2 day stays at 0.2.
        let lone = review_rating_fractions(&[rated_day((2, 1), (0, 0))]);
        assert!((lone[0].mood - 0.2_f32).abs() < 1e-6);
        assert!(bars.iter().all(|bar| bar.mood + bar.energy <= 1.0 + 1e-6
            && bar.mood.is_finite()
            && bar.energy.is_finite()));
        assert_eq!(review_rating_fractions(&[]), Vec::<ReflectionBar>::new());
    }
}
