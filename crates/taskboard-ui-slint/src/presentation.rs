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

use chrono::{DateTime, NaiveTime, TimeDelta, Utc};
use taskboard_domain::{
    AdhocAlarmId, AppState, Board, BoardId, Effort, ReminderRef, SidecarHealth, Stack, StackId,
    SyncPhase, Task, TaskId,
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

/// The ` · HH:MM` time suffix (round 3 D9/F15): empty for an absent
/// or midnight instant — a midnight instant renders exactly the
/// date-only shape it always did — else the UTC wall-clock time. One
/// place: both date phrases compose it; the tint/direction language
/// is untouched.
#[must_use]
pub fn at_time_suffix(instant: Option<DateTime<Utc>>) -> String {
    match instant {
        Some(instant) if instant.time() != NaiveTime::MIN => {
            format!(" \u{b7} {}", instant.format("%H:%M"))
        }
        _ => String::new(),
    }
}

/// The display phrase for a due classification (mono role), assembled
/// from the bucket with the direction taken from the sign of
/// `duedate - now`.
#[must_use]
pub fn due_phrase(kind: DueKind, duedate: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(duedate) = duedate else {
        return String::new();
    };
    let suffix = at_time_suffix(Some(duedate));
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
pub fn scheduled_phrase(scheduled: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let Some(scheduled) = scheduled else {
        return String::new();
    };
    let suffix = at_time_suffix(Some(scheduled));
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
/// rules-replace-default derivation). Empty for a rule-less task.
#[must_use]
pub fn rule_list_hint(rule_count: usize) -> String {
    if rule_count == 0 {
        String::new()
    } else {
        "Explicit rules replace the task's default reminder: only these rules fire.".to_owned()
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
    now: DateTime<Utc>,
) -> Vec<NotificationRow> {
    taskboard_domain::active_notifications(&state.tasks, &state.reminders, defaults, now)
        .into_iter()
        .filter(|note| match note.anchor {
            taskboard_domain::ReminderAnchor::Task(task) => {
                state.tasks.get(&task).is_none_or(|t| !is_anchor_task(t))
            }
            taskboard_domain::ReminderAnchor::Alarm(_) => true,
        })
        .map(|note| NotificationRow {
            task: match note.anchor {
                taskboard_domain::ReminderAnchor::Task(task) => Some(task),
                taskboard_domain::ReminderAnchor::Alarm(_) => None,
            },
            reminder: note.reminder,
            occurrence: note.occurrence,
            title: note.title,
        })
        .collect()
}

/// The active timer row's remaining-time phrase (D-I7): the
/// `due_phrase` future direction over [`duration_bucket`] — `Now`
/// clamps (the fired-undelivered transient is total, never a panic).
#[must_use]
pub fn countdown_phrase(effective: DateTime<Utc>, now: DateTime<Utc>) -> String {
    match duration_bucket(effective - now) {
        DurationBucket::Now => "now".to_owned(),
        DurationBucket::Minutes(n) => format!("in {n}m"),
        DurationBucket::Hours(n) => format!("in {n}h"),
        DurationBucket::Days(n) => format!("in {n}d"),
    }
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

/// One active timer row (D-I7): the alarm plus its walked effective
/// occurrence, carried so the push pass derives the countdown phrase
/// once per refresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimerRow {
    /// The alarm the cancel affordance dispatches against.
    pub alarm: taskboard_domain::AdhocAlarmId,
    /// The label snapshot, carried verbatim.
    pub label: String,
    /// The effective occurrence (snooze walk included).
    pub effective: DateTime<Utc>,
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
    now: DateTime<Utc>,
) -> usize {
    notification_rows(state, defaults, now)
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
        "at" => {
            let rest = rest.trim();
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(rest) {
                return Some(taskboard_domain::AdhocWhen::At(
                    dt.with_timezone(&chrono::Utc),
                ));
            }
            if let Ok(date) = chrono::NaiveDate::parse_from_str(rest, "%Y-%m-%d") {
                let midnight = date.and_hms_opt(0, 0, 0)?;
                return tz.from_local_datetime(&midnight).single().map(|local| {
                    taskboard_domain::AdhocWhen::At(local.with_timezone(&chrono::Utc))
                });
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
            Some(taskboard_domain::AdhocWhen::At(instant))
        }
        _ => None,
    }
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

/// The global inbox's open count (chunk 10 D9): live, open,
/// anchor-filtered tasks whose stack belongs to the global Inbox
/// board — one inbox, one number, the `due_sets` template. Zero when
/// no Inbox board exists (an empty inbox is triage-done).
#[must_use]
pub fn inbox_count(state: &AppState) -> usize {
    state.inbox_board().map_or(0, |inbox| {
        state
            .tasks
            .values()
            .filter(|task| {
                task.is_live()
                    && task.is_open()
                    && !is_anchor_task(task)
                    && state
                        .stacks
                        .get(&task.stack)
                        .is_some_and(|stack| stack.board == inbox)
            })
            .count()
    })
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
    now: DateTime<Utc>,
) -> Vec<AlarmRow> {
    taskboard_domain::active_notifications(&state.tasks, &state.reminders, defaults, now)
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
    use chrono::TimeZone as _;
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

            archived: false,
            deleted: false,
            clocks: task_clocks(),
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
        assert!(due_phrase(DueKind::None, None, now).is_empty(), "undated");
        assert!(
            !due_phrase(DueKind::Later, Some(now + TimeDelta::hours(3)), now).is_empty(),
            "dated future"
        );
        assert!(
            !due_phrase(DueKind::Overdue, Some(now - TimeDelta::hours(3)), now).is_empty(),
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
            offset: ReminderOffset::before(TimeDelta::minutes(-30)).expect("valid offset"),
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
                offset: ReminderOffset::at_due(),
                created_at: now,
                updated_at: now,
            },
        );
        // No delivery yet: not active (the surface is fired ∧ not
        // dismissed — the domain view's contract, not a UI rule).
        assert_eq!(
            notification_rows(&state, &defaults(), now),
            [] as [NotificationRow; 0]
        );

        let occurrence = now - TimeDelta::hours(1);
        state.reminders.deliveries.insert(ReminderDelivery {
            reminder: ReminderRef::Rule(rule_id),
            occurrence,
            delivered_at: now,
        });
        let rows = notification_rows(&state, &defaults(), now);
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
            notification_rows(&state, &defaults(), now),
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
            notification_rows(&state, &defaults(), now),
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

    #[test]
    fn countdown_phrase_buckets_the_remaining_time_and_clamps() {
        let now = Utc::now();
        assert_eq!(countdown_phrase(now + TimeDelta::seconds(59), now), "now");
        assert_eq!(countdown_phrase(now + TimeDelta::seconds(60), now), "in 1m");
        assert_eq!(
            countdown_phrase(now + TimeDelta::minutes(59), now),
            "in 59m"
        );
        assert_eq!(countdown_phrase(now + TimeDelta::minutes(60), now), "in 1h");
        assert_eq!(countdown_phrase(now + TimeDelta::hours(23), now), "in 23h");
        assert_eq!(countdown_phrase(now + TimeDelta::hours(24), now), "in 1d");
        // Past input (the fired-undelivered transient) clamps, total.
        assert_eq!(countdown_phrase(now - TimeDelta::hours(3), now), "now");
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
        assert_eq!(awaiting_alarm_count(&state, &defaults(), now), 0);

        // Fired + delivered: one awaiting row.
        deliver_alarm(&mut state, id, base);
        assert_eq!(awaiting_alarm_count(&state, &defaults(), now), 1);

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
        assert_eq!(awaiting_alarm_count(&state, &defaults(), now), 0);

        // Delivered at the new target: awaiting again at that target.
        deliver_alarm(&mut state, id, now + TimeDelta::minutes(10));
        let later = now + TimeDelta::minutes(11);
        assert_eq!(awaiting_alarm_count(&state, &defaults(), later), 1);

        // Dismissed at the effective: gone for good.
        state
            .reminders
            .dismissals
            .insert(taskboard_domain::ReminderDismissal {
                reminder: ReminderRef::Adhoc(id),
                occurrence: now + TimeDelta::minutes(10),
                dismissed_at: later,
            });
        assert_eq!(awaiting_alarm_count(&state, &defaults(), later), 0);
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
                offset: ReminderOffset::at_due(),
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
            notification_rows(&state, &defaults(), now).len(),
            1,
            "the task row renders on the surface"
        );
        assert_eq!(
            awaiting_alarm_count(&state, &defaults(), now),
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

        let rows = alarm_window_rows(&state, &defaults(), now);
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
        let rows = alarm_window_rows(&state, &defaults(), now);
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

    #[test]
    fn scheduled_phrase_is_empty_when_absent_and_directional_when_set() {
        // Structural facts only (AGENTS §5), the `due_phrase` precedent:
        // the words are presentation wording, the direction is the
        // behavior — pinned as future ≠ past at the same bucket.
        let now = chrono::TimeZone::timestamp_opt(&Utc, 1_700_000_000, 0)
            .single()
            .expect("ts");
        assert_eq!(scheduled_phrase(None, now), "");
        assert!(
            !scheduled_phrase(Some(now), now).is_empty(),
            "the boundary bucket renders"
        );
        let future = scheduled_phrase(Some(now + TimeDelta::hours(2)), now);
        let past = scheduled_phrase(Some(now - TimeDelta::hours(2)), now);
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
        assert_eq!(at_time_suffix(None), "");
        let midnight = Utc.with_ymd_and_hms(2030, 1, 31, 0, 0, 0).unwrap();
        assert_eq!(at_time_suffix(Some(midnight)), "");
    }

    #[test]
    fn at_time_suffix_carries_the_utc_clock_and_both_phrases_compose_it() {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2030, 1, 31, 12, 0, 0).unwrap();
        let timed = Utc.with_ymd_and_hms(2030, 2, 2, 14, 30, 0).unwrap();
        assert_eq!(at_time_suffix(Some(timed)), " \u{b7} 14:30");
        // Midnight instants render exactly the date-only shape.
        let midnight = Utc.with_ymd_and_hms(2030, 2, 2, 0, 0, 0).unwrap();
        let timed_phrase = due_phrase(DueKind::Later, Some(timed), now);
        let midnight_phrase = due_phrase(DueKind::Later, Some(midnight), now);
        assert!(timed_phrase.ends_with(" \u{b7} 14:30"));
        assert_eq!(midnight_phrase.find('\u{b7}'), None);
        let sched_timed = scheduled_phrase(Some(timed), now);
        let sched_midnight = scheduled_phrase(Some(midnight), now);
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
                        offset: ReminderOffset::before(TimeDelta::minutes(-15))
                            .expect("valid offset"),
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
                    ReminderRef::Adhoc(_) => {
                        panic!("banner rows never carry alarm refs");
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
                    board: taskboard_domain::RemoteBoardId(1),
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

                    archived: taskboard_domain::BoardClocks::epoch(),
                    deleted: taskboard_domain::BoardClocks::epoch(),
                },
                remote_seen: None,
                remote_baseline: None,
            };
            if done {
                t.done = Some(taskboard_domain::BoardClocks::epoch());
            }
            t
        };
        state.tasks.insert(
            TaskId::from_uuid(uuid::Uuid::now_v7()),
            mk(inbox_stack, "one", false, false),
        );
        state.tasks.insert(
            TaskId::from_uuid(uuid::Uuid::now_v7()),
            mk(inbox_stack, "two", false, false),
        );
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
                archived: taskboard_domain::BoardClocks::epoch(),
                deleted: taskboard_domain::BoardClocks::epoch(),
            },
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
        use chrono::TimeZone as _;
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
        }
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
    use chrono::TimeZone as _;
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
}
