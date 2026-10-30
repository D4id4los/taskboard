// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard track` (D-T14) and `taskboard export` (D-T15): the
//! time-tracking verb family and the raw-payload export verb — the
//! timers two-lane recipe: one-shot wrappers over
//! [`super::tasks::with_core`], `*_on` seams on a booted session core,
//! one [`collapse`] definition site consumed by both lanes. The CLI
//! only parses and dispatches: duration phrases, CSV/JSON renders, the
//! day-window projection, and the filters are pure functions;
//! receipts are state truth read back from the published snapshot,
//! never input echoes; export is raw stdout data (pipes are the
//! contract — no file writing, no clipboard) and deliberately bypasses
//! the `--json` receipt envelope (D-T15's recorded exception, D-TC10).

use std::path::PathBuf;
use std::sync::Arc;

use chrono::{DateTime, NaiveDate, TimeDelta, Utc};

use taskboard_domain::{
    AppState, CommandOutcome, DayCalendar, StateCommand, TimeEntryId, TrackingState,
};

use crate::cli::{ExportFormat, TrackCmd};
use crate::commands::CommandError;
use crate::commands::output::{
    ExportRow, TrackAdjustReceipt, TrackDeleteReceipt, TrackEntryRow, TrackListView,
    TrackLogReceipt, TrackStartReceipt, TrackStatusView, TrackStopReceipt,
};
use crate::config::AppConfig;
use crate::secrets::CredentialStore;

/// The header row of the CSV payload (D-TC11's fixed column contract).
const EXPORT_CSV_HEADER: &str = "entry_id,task_id,task_title,start,end,duration_seconds";

/// Boots the core and starts tracking the selected live task (the
/// one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn start(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task_selector: &str,
) -> Result<TrackStartReceipt, CommandError> {
    let selector = task_selector.to_owned();
    super::tasks::with_core(config, store, async |app| start_on(app, &selector).await).await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn stop(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
) -> Result<TrackStopReceipt, CommandError> {
    super::tasks::with_core(config, store, async |app| stop_on(app).await).await
}

/// The `track status` steps on a booted core. `day` travels as data
/// (the D-F6 consumer contract): the caller resolves it once per
/// invocation from the validated config
/// ([`AppConfig::resolved_day_calendar`]) — never mid-flight.
#[must_use]
pub fn status_on(app: &crate::App, day: DayCalendar) -> TrackStatusView {
    let state = app.engine.shared_state().load_full();
    status_view(&state.tracking, day, Utc::now())
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn status(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    day: DayCalendar,
) -> Result<TrackStatusView, CommandError> {
    super::tasks::with_core(config, store, async |app| Ok(status_on(app, day))).await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn list(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task_selector: Option<&str>,
    since: Option<NaiveDate>,
) -> Result<TrackListView, CommandError> {
    let selector = task_selector.map(str::to_owned);
    super::tasks::with_core(config, store, async |app| {
        list_on(app, selector.as_deref(), since)
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn log(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    task_selector: &str,
    mode: LogMode,
) -> Result<TrackLogReceipt, CommandError> {
    let selector = task_selector.to_owned();
    super::tasks::with_core(config, store, async |app| {
        log_on(app, &selector, mode).await
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn adjust(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    entry_selector: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<TrackAdjustReceipt, CommandError> {
    let selector = entry_selector.to_owned();
    super::tasks::with_core(config, store, async |app| {
        adjust_on(app, &selector, start, end).await
    })
    .await
}

/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn delete(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    entry_selector: &str,
) -> Result<TrackDeleteReceipt, CommandError> {
    let selector = entry_selector.to_owned();
    super::tasks::with_core(config, store, async |app| delete_on(app, &selector).await).await
}

/// Boots the core and projects the export payload (the one-shot lane).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn export(
    config: AppConfig,
    store: Arc<dyn CredentialStore>,
    query: ExportQuery,
) -> Result<ExportPayload, CommandError> {
    super::tasks::with_core(config, store, async |app| export_on(app, &query)).await
}

/// The `track start` steps on a booted core (the session seam):
/// resolve the task selector, execute, project the receipt from the
/// post-state — the entry is read back from the published snapshot,
/// never an echo of the input.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: both pinned
/// outcomes name an open entry of this task in the same-state
/// projection, so the read cannot miss.
pub async fn start_on(
    app: &crate::App,
    task_selector: &str,
) -> Result<TrackStartReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task = super::tasks::resolve_task(&state, task_selector)?;
    drop(state);
    let now = Utc::now();
    match app
        .engine
        .execute(StateCommand::StartTracking { task })
        .await?
    {
        CommandOutcome::TrackingStarted { entry, closed } => {
            let state = app.engine.shared_state().load_full();
            let entry = state
                .tracking
                .entries
                .get(&entry.id)
                .cloned()
                .expect("the start outcome names the minted entry");
            Ok(TrackStartReceipt {
                started: true,
                entry: entry_row(&entry, now),
                closed: closed.iter().map(|id| id.as_uuid().to_string()).collect(),
            })
        }
        // The idempotent repeat: exactly one open entry exists and it
        // is this task's (the payload contract) — the receipt projects
        // it with `started: false` and an empty `closed`.
        CommandOutcome::NoOp => {
            let state = app.engine.shared_state().load_full();
            let entry = state
                .tracking
                .entries
                .values()
                .find(|entry| entry.is_open() && entry.task == task)
                .cloned()
                .expect("the idempotent no-op names this task's open entry");
            Ok(TrackStartReceipt {
                started: false,
                entry: entry_row(&entry, now),
                closed: Vec::new(),
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `track stop` steps on a booted core: close everything open at
/// the engine's now, project the stamped rows from the outcome.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn stop_on(app: &crate::App) -> Result<TrackStopReceipt, CommandError> {
    let now = Utc::now();
    match app.engine.execute(StateCommand::StopTracking).await? {
        CommandOutcome::TrackingStopped { entries } => Ok(TrackStopReceipt {
            stopped: true,
            entries: entries.iter().map(|entry| entry_row(entry, now)).collect(),
        }),
        CommandOutcome::NoOp => Ok(TrackStopReceipt {
            stopped: false,
            entries: Vec::new(),
        }),
        _ => Err(outcome_mismatch()),
    }
}

/// The `track log` steps on a booted core: resolve the task selector,
/// stamp the span (`--for` ends at the command layer's `now`), execute.
/// An invalid span (empty or inverted, a zero `--for` included) is the
/// planner's typed [`taskboard_domain::CommandError::InvalidSpan`]
/// rejection — `invalid-span`, exit 1 — never a clap refusal (D-TC8).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: the log outcome
/// names the minted entry, so the same-state projection cannot miss.
pub async fn log_on(
    app: &crate::App,
    task_selector: &str,
    mode: LogMode,
) -> Result<TrackLogReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task = super::tasks::resolve_task(&state, task_selector)?;
    drop(state);
    let now = Utc::now();
    let (start, end) = match mode {
        LogMode::For(duration) => (now - duration, now),
        LogMode::Span(from, to) => (from, to),
    };
    match app
        .engine
        .execute(StateCommand::LogTime { task, start, end })
        .await?
    {
        CommandOutcome::TimeLogged { entry } => {
            let state = app.engine.shared_state().load_full();
            let entry = state
                .tracking
                .entries
                .get(&entry.id)
                .cloned()
                .expect("the log outcome names the minted entry");
            Ok(TrackLogReceipt {
                entry: entry_row(&entry, now),
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `track adjust` steps on a booted core: resolve the entry
/// selector, execute the change mask. The zero-flag mask is the
/// planner's accepted no-op (`changed: false`, the current row).
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the post-execute row read: resolution and the
/// no-op path both guarantee the entry exists in the projected state.
pub async fn adjust_on(
    app: &crate::App,
    entry_selector: &str,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<TrackAdjustReceipt, CommandError> {
    let state = app.engine.shared_state().load_full();
    let id = resolve_time_entry(&state, entry_selector)?;
    drop(state);
    let now = Utc::now();
    match app
        .engine
        .execute(StateCommand::AdjustTimeEntry { id, start, end })
        .await?
    {
        CommandOutcome::EntryAdjusted { entry } => {
            let state = app.engine.shared_state().load_full();
            let entry = state
                .tracking
                .entries
                .get(&entry.id)
                .cloned()
                .expect("the adjust outcome names the adjusted entry");
            Ok(TrackAdjustReceipt {
                changed: true,
                entry: entry_row(&entry, now),
            })
        }
        CommandOutcome::NoOp => {
            let state = app.engine.shared_state().load_full();
            let entry = state
                .tracking
                .entries
                .get(&id)
                .cloned()
                .expect("resolution guarantees the entry exists");
            Ok(TrackAdjustReceipt {
                changed: false,
                entry: entry_row(&entry, now),
            })
        }
        _ => Err(outcome_mismatch()),
    }
}

/// The `track delete` steps on a booted core: a full uuid is used
/// verbatim (no existence requirement — the honest repeat rides the
/// domain's accepted-absent no-op), a prefix resolves against the
/// current entries. Repeats project `deleted: false` (D-TC9), never
/// terminal-by-deletion.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub async fn delete_on(
    app: &crate::App,
    entry_selector: &str,
) -> Result<TrackDeleteReceipt, CommandError> {
    let id = if let Ok(uuid) = uuid::Uuid::parse_str(entry_selector) {
        // Full uuid: no existence requirement — the honest repeat rides
        // the domain's accepted-absent no-op.
        TimeEntryId::from_uuid(uuid)
    } else {
        let state = app.engine.shared_state().load_full();
        resolve_time_entry(&state, entry_selector)?
    };
    let entry_id = id.as_uuid().to_string();
    match app
        .engine
        .execute(StateCommand::DeleteTimeEntry { id })
        .await?
    {
        CommandOutcome::EntryDeleted { .. } => Ok(TrackDeleteReceipt {
            entry_id,
            deleted: true,
        }),
        CommandOutcome::NoOp => Ok(TrackDeleteReceipt {
            entry_id,
            deleted: false,
        }),
        _ => Err(outcome_mismatch()),
    }
}

/// The `track list` steps on a booted core: resolve the optional task
/// selector (live tasks only — `resolve_task`'s existing contract),
/// project the canonical rows.
///
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
pub fn list_on(
    app: &crate::App,
    task_selector: Option<&str>,
    since: Option<NaiveDate>,
) -> Result<TrackListView, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task = task_selector
        .map(|selector| super::tasks::resolve_task(&state, selector))
        .transpose()?;
    Ok(list_view(&state.tracking, task, since, Utc::now()))
}

/// The `export` steps on a booted core: resolve the optional
/// selectors, apply the inclusive UTC-day filters on `entry.start`,
/// project the payload rows. The rows carry their `duration_seconds`
/// derived at this instant (an open row accrues to the export
/// instant); rendering to bytes is the emit site's pure job.
///
/// # Errors
///
/// [`CommandError`] — typed, see the variant docs.
///
/// # Panics
///
/// Only via `expect` on the day successor — unreachable for any
/// representable filter date (the `day_start` comment).
pub fn export_on(app: &crate::App, query: &ExportQuery) -> Result<ExportPayload, CommandError> {
    let state = app.engine.shared_state().load_full();
    let task = query
        .task
        .as_deref()
        .map(|selector| super::tasks::resolve_task(&state, selector))
        .transpose()?;
    let board = query
        .board
        .as_deref()
        .map(|selector| super::resolve_live_board(&state, selector))
        .transpose()?;
    let since_stamp = query.since.map(day_start);
    let until_stamp = query
        .until
        .map(|date| date.succ_opt().expect("tomorrow exists"))
        .map(day_start);
    let now = Utc::now();
    let mut rows: Vec<&taskboard_domain::TimeEntry> = state
        .tracking
        .entries
        .values()
        .filter(|entry| since_stamp.is_none_or(|start| entry.start >= start))
        .filter(|entry| until_stamp.is_none_or(|end| entry.start < end))
        .filter(|entry| task.is_none_or(|task| entry.task == task))
        .filter(|entry| board.is_none_or(|board| stack_belongs_to_board(&state, entry, board)))
        .collect();
    rows.sort_by_key(|entry| (entry.start, entry.id));
    Ok(ExportPayload {
        rows: rows.iter().map(|entry| entry_row(entry, now)).collect(),
        format: query.format.unwrap_or_default(),
    })
}

/// Whether the entry's task's stack belongs to `board`. Entries whose
/// task row is absent (e.g. after a board reset) attribute nowhere and
/// are skipped under a `--board` filter (D-TC12); soft-deleted tasks
/// keep their stack ref, so attribution survives deletion.
fn stack_belongs_to_board(
    state: &AppState,
    entry: &taskboard_domain::TimeEntry,
    board: taskboard_domain::BoardId,
) -> bool {
    state
        .tasks
        .get(&entry.task)
        .and_then(|task| state.stacks.get(&task.stack))
        .is_some_and(|stack| stack.board == board)
}

/// `YYYY-MM-DD`'s midnight UTC — the shared UTC-day arithmetic of the
/// start-inclusive filters (D-TC12: deliberately plain UTC,
/// deterministic for scripted pipelines; the `day_start_time` window
/// stays a *display* concern on `status`).
///
/// # Panics
///
/// Never: midnight is a valid time-of-day for every date chrono
/// represents.
#[must_use]
pub fn day_start(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc()
}

/// Resolves an entry selector against the tracking map (D-TC4:
/// [`super::timers::resolve_alarm`] minus the label pass — entries
/// carry no title of their own, so a label-style pass would silently
/// match across tasks): exact uuid → unique ≥8-char lowercased uuid
/// prefix ([`CommandError::AmbiguousTimeEntry`] on multi-hit) → typed
/// miss. A shorter non-uuid selector is a miss, not an ambiguity.
///
/// # Errors
///
/// [`CommandError::UnknownTimeEntry`] / [`CommandError::AmbiguousTimeEntry`]
/// — typed, never text-matched in the engine.
pub fn resolve_time_entry(state: &AppState, selector: &str) -> Result<TimeEntryId, CommandError> {
    let entries = &state.tracking.entries;
    if let Ok(uuid) = uuid::Uuid::parse_str(selector) {
        let id = TimeEntryId::from_uuid(uuid);
        if entries.contains_key(&id) {
            return Ok(id);
        }
    }
    if selector.len() >= 8 {
        let needle = selector.to_lowercase();
        let hits: Vec<TimeEntryId> = entries
            .keys()
            .filter(|id| id.as_uuid().to_string().starts_with(&needle))
            .copied()
            .collect();
        match hits.as_slice() {
            [id] => return Ok(*id),
            [] => {}
            [_, ..] => {
                return Err(CommandError::AmbiguousTimeEntry {
                    candidates: hits.iter().map(|id| id.as_uuid().to_string()).collect(),
                });
            }
        }
    }
    Err(CommandError::UnknownTimeEntry {
        selector: selector.to_owned(),
    })
}

/// One resolved `track` invocation: the clap subcommand collapsed to
/// exactly one mode.
#[derive(Debug, Clone)]
pub enum TrackInvocation {
    /// `start <task>`.
    Start {
        /// The task selector.
        task: String,
    },
    /// `stop`.
    Stop,
    /// `status` (pure read + the day-window total).
    Status,
    /// `list [--task] [--since]` (pure read).
    List {
        /// The optional task selector.
        task: Option<String>,
        /// The inclusive UTC-day lower bound.
        since: Option<NaiveDate>,
    },
    /// `log <task>` in exactly one span mode.
    Log {
        /// The task selector.
        task: String,
        /// The collapsed span mode.
        mode: LogMode,
    },
    /// `adjust <entry> [--start] [--end]` (the change mask).
    Adjust {
        /// The entry selector.
        entry: String,
        /// New start, or unchanged.
        start: Option<DateTime<Utc>>,
        /// New end, or unchanged.
        end: Option<DateTime<Utc>>,
    },
    /// `delete <entry>`.
    Delete {
        /// The entry selector.
        entry: String,
    },
}

/// The collapsed `log` span mode: a length ending at the command
/// layer's `now`, or an absolute `[from, to)` pair. Span *validity*
/// (strictly positive) is the planner's law, not the collapse's
/// (D-TC8) — a zero `--for` is the typed `invalid-span`, never a
/// panic at the vacuous-unreachable branch.
#[derive(Debug, Clone)]
pub enum LogMode {
    /// A block of this length ending now.
    For(TimeDelta),
    /// An absolute span.
    Span(DateTime<Utc>, DateTime<Utc>),
}

/// Collapses the clap subcommand into exactly one invocation. One
/// definition site for both dispatch lanes (D-TC2). `None` is returned
/// only for the clap-unreachable vacuous log group — total for direct
/// calls (the [`super::timers::collapse`] comment).
#[must_use]
pub fn collapse(cmd: TrackCmd) -> Option<TrackInvocation> {
    match cmd {
        TrackCmd::Start { task } => Some(TrackInvocation::Start { task }),
        TrackCmd::Stop => Some(TrackInvocation::Stop),
        TrackCmd::Status => Some(TrackInvocation::Status),
        TrackCmd::List { task, since } => Some(TrackInvocation::List { task, since }),
        TrackCmd::Log {
            task,
            for_,
            from,
            to,
        } => {
            let mode = match (for_, from, to) {
                (Some(duration), _, _) => LogMode::For(duration),
                (None, Some(from), Some(to)) => LogMode::Span(from, to),
                // Unreachable through clap (the paired required group);
                // `None` keeps `collapse` total for direct calls.
                (None, _, _) => return None,
            };
            Some(TrackInvocation::Log { task, mode })
        }
        TrackCmd::Adjust { entry, start, end } => {
            Some(TrackInvocation::Adjust { entry, start, end })
        }
        TrackCmd::Delete { entry } => Some(TrackInvocation::Delete { entry }),
    }
}

/// The `export` flag bag: `Export` has no subcommands, so the flags
/// are the invocation (no collapse — D-TC2).
#[derive(Debug, Clone, Default)]
pub struct ExportQuery {
    /// The payload format (`Csv` when omitted).
    pub format: Option<ExportFormat>,
    /// Entries starting on/after this UTC day (inclusive).
    pub since: Option<NaiveDate>,
    /// Entries starting before the day after this UTC day (inclusive
    /// of the named day).
    pub until: Option<NaiveDate>,
    /// The optional live-task selector.
    pub task: Option<String>,
    /// The optional live-board selector.
    pub board: Option<String>,
}

/// The export payload: rows as data, rendered to bytes at the emit
/// site. Deliberately never `emit_to`'d — the payload bypasses the
/// `--json` receipt envelope (D-TC10's recorded exception).
#[derive(Debug, Clone)]
pub struct ExportPayload {
    /// The entry rows, canonical `(start, id)` ascending.
    pub rows: Vec<ExportRow>,
    /// The selected payload format.
    pub format: ExportFormat,
}

/// Renders the payload to its wire bytes: CSV (RFC-4180, header row,
/// RFC-3339 stamps, an open row's empty `end` cell) or a single-line,
/// newline-terminated JSON array (`"end": null` for open rows). Pure —
/// zero rows are header-only CSV / `[]`, success.
#[must_use]
pub fn render_payload(payload: &ExportPayload) -> String {
    match payload.format {
        ExportFormat::Csv => render_csv(&payload.rows),
        ExportFormat::Json => render_json(&payload.rows),
    }
}

/// The CSV rendering: the fixed header, then one row per entry, fields
/// quoted only when needed ([`csv_field`]).
#[must_use]
pub fn render_csv(rows: &[ExportRow]) -> String {
    let mut out = String::from(EXPORT_CSV_HEADER);
    out.push('\n');
    for row in rows {
        let cells = [
            csv_field(&row.entry_id),
            csv_field(&row.task_id),
            csv_field(&row.task_title),
            row.start.to_rfc3339(),
            row.end.map_or(String::new(), |end| end.to_rfc3339()),
            row.duration_seconds.to_string(),
        ];
        out.push_str(&cells.join(","));
        out.push('\n');
    }
    out
}

/// The JSON rendering: a single-line serde array of the same six
/// fields, newline-terminated — `jq` is the consumer.
///
/// # Panics
///
/// Never: the projection serialization is total.
#[must_use]
pub fn render_json(rows: &[ExportRow]) -> String {
    let mut line = serde_json::to_string(rows).expect("row serialization is total");
    line.push('\n');
    line
}

/// Minimal RFC-4180 quoting: wrap and double-quote only when the field
/// contains a comma, a double quote, a newline, or a carriage return.
/// Hand-rolled, no new dependency (the `cargo deny` fence).
#[must_use]
pub fn csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_owned()
    }
}

/// The human duration vocabulary: `<h>h <m>m` composition at minute
/// granularity (v1 is minute-granular by the round's non-goals) —
/// `90m` → `"1h 30m"`, `45s` → `"0m"`, zero → `"0m"`, hours-only
/// → `"1h 0m"`. Negative deltas clamp at zero (a degenerate span
/// renders as nothing, mirroring the domain's read clamp).
#[must_use]
pub fn duration_phrase(delta: TimeDelta) -> String {
    let minutes = delta.num_minutes().max(0);
    if minutes == 0 {
        return "0m".to_owned();
    }
    format!("{}h {}m", minutes / 60, minutes % 60)
}

/// The `track status` projection (pure — the lowest layer owning the
/// day-window behavior): every open entry newest-first
/// ([`taskboard_domain::TrackingState::newest_open`]'s ordering applied
/// over the whole open set), plus `tracked_in_window`'s split-at-the-
/// boundary total over the `day` window as of `now`. `now` is read at
/// the command layer — the app boundary may read clocks; this stays
/// pure.
#[must_use]
pub fn status_view(
    entries: &TrackingState,
    day: DayCalendar,
    now: DateTime<Utc>,
) -> TrackStatusView {
    let mut open: Vec<&taskboard_domain::TimeEntry> = entries.open_entries().collect();
    open.sort_by_key(|entry| std::cmp::Reverse((entry.start, entry.id)));
    let window = taskboard_domain::day_window(now, day.day_start_time, day.offset);
    let total = taskboard_domain::tracked_in_window(entries.entries.values(), window, now);
    TrackStatusView {
        open: open.iter().map(|entry| entry_row(entry, now)).collect(),
        tracked_today_seconds: u64::try_from(total.num_seconds()).unwrap_or(0),
    }
}

/// The `track list` projection (pure): canonical `(start, id)`
/// ascending over the filtered set. The task filter matches by id
/// (pre-resolved — only live tasks select); `since` keeps entries
/// starting on/after that day's midnight UTC.
#[must_use]
pub fn list_view(
    entries: &TrackingState,
    task: Option<taskboard_domain::TaskId>,
    since: Option<NaiveDate>,
    now: DateTime<Utc>,
) -> TrackListView {
    let since_stamp = since.map(day_start);
    let mut rows: Vec<&taskboard_domain::TimeEntry> = entries
        .entries
        .values()
        .filter(|entry| task.is_none_or(|task| entry.task == task))
        .filter(|entry| since_stamp.is_none_or(|start| entry.start >= start))
        .collect();
    rows.sort_by_key(|entry| (entry.start, entry.id));
    TrackListView {
        entries: rows.iter().map(|entry| entry_row(entry, now)).collect(),
    }
}

/// Projects one entry onto the six-column row; `duration_seconds` is
/// derived at this instant (clamped ≥ 0 by the domain read).
fn entry_row(entry: &taskboard_domain::TimeEntry, now: DateTime<Utc>) -> TrackEntryRow {
    TrackEntryRow {
        entry_id: entry.id.as_uuid().to_string(),
        task_id: entry.task.as_uuid().to_string(),
        task_title: entry.task_title.clone(),
        start: entry.start,
        end: entry.end,
        duration_seconds: u64::try_from(entry.duration(now).num_seconds()).unwrap_or(0),
    }
}

/// The planner's reply contract pins one outcome per tracking command
/// (plus `NoOp`); any other reply is an engine contract bug — surfaced
/// as the generic engine kind, never misreported as a user error.
fn outcome_mismatch() -> CommandError {
    CommandError::Engine(Box::new(taskboard_state::ExecuteError::EngineGone))
}

/// The human-readable `status` render: open rows (title, elapsed
/// phrase, entry id) and the tracked-today total in phrase and
/// whole-seconds forms — state truth read back, never an input echo.
#[must_use]
pub fn render_text_status(view: &TrackStatusView) -> String {
    use std::fmt::Write as _;
    let mut lines = Vec::new();
    if view.open.is_empty() {
        lines.push("nothing running".to_owned());
    }
    for row in &view.open {
        let phrase = duration_phrase(
            TimeDelta::try_seconds(i64::try_from(row.duration_seconds).unwrap_or(i64::MAX))
                .unwrap_or_default(),
        );
        let mut line = format!(
            "{} — {} (entry {}, started {})",
            row.task_title,
            phrase,
            row.entry_id,
            row.start.to_rfc3339()
        );
        if row.end.is_none() {
            let _ = write!(line, " [open]");
        }
        lines.push(line);
    }
    let total =
        TimeDelta::try_seconds(i64::try_from(view.tracked_today_seconds).unwrap_or(i64::MAX))
            .unwrap_or_default();
    lines.push(format!(
        "tracked today: {} ({}s)",
        duration_phrase(total),
        view.tracked_today_seconds
    ));
    lines.join("\n")
}

/// The human-readable `list` render.
#[must_use]
pub fn render_text_list(view: &TrackListView) -> String {
    if view.entries.is_empty() {
        return "no entries".to_owned();
    }
    view.entries
        .iter()
        .map(|row| {
            let phrase = duration_phrase(
                TimeDelta::try_seconds(i64::try_from(row.duration_seconds).unwrap_or(i64::MAX))
                    .unwrap_or_default(),
            );
            let end = row
                .end
                .map_or_else(|| "open".to_owned(), |end| end.to_rfc3339());
            format!(
                "{} … {}  {} — {} (entry {})",
                row.start.to_rfc3339(),
                end,
                row.task_title,
                phrase,
                row.entry_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `start` receipt (the stamped value, not an echo).
#[must_use]
pub fn render_text_start(receipt: &TrackStartReceipt) -> String {
    use std::fmt::Write as _;
    let phrase = duration_phrase(
        TimeDelta::try_seconds(i64::try_from(receipt.entry.duration_seconds).unwrap_or(i64::MAX))
            .unwrap_or_default(),
    );
    let mut line = format!(
        "tracking \"{}\" since {} ({} elapsed, entry {})",
        receipt.entry.task_title,
        receipt.entry.start.to_rfc3339(),
        phrase,
        receipt.entry.entry_id
    );
    if !receipt.started {
        let _ = write!(line, " [already running]");
    }
    if !receipt.closed.is_empty() {
        let _ = write!(line, " [closed {}]", receipt.closed.join(", "));
    }
    line
}

/// The human-readable `stop` receipt.
#[must_use]
pub fn render_text_stop(receipt: &TrackStopReceipt) -> String {
    if !receipt.stopped {
        return "nothing was running".to_owned();
    }
    receipt
        .entries
        .iter()
        .map(|row| {
            let phrase = duration_phrase(
                TimeDelta::try_seconds(i64::try_from(row.duration_seconds).unwrap_or(i64::MAX))
                    .unwrap_or_default(),
            );
            format!(
                "closed \"{}\" ({} elapsed, entry {})",
                row.task_title, phrase, row.entry_id
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The human-readable `log` receipt.
#[must_use]
pub fn render_text_log(receipt: &TrackLogReceipt) -> String {
    let row = &receipt.entry;
    let phrase = duration_phrase(
        TimeDelta::try_seconds(i64::try_from(row.duration_seconds).unwrap_or(i64::MAX))
            .unwrap_or_default(),
    );
    format!(
        "logged {} on \"{}\" ({} … {}, entry {})",
        phrase,
        row.task_title,
        row.start.to_rfc3339(),
        row.end
            .map_or_else(|| "open".to_owned(), |end| end.to_rfc3339()),
        row.entry_id
    )
}

/// The human-readable `adjust` receipt.
#[must_use]
pub fn render_text_adjust(receipt: &TrackAdjustReceipt) -> String {
    let row = &receipt.entry;
    let mut line = format!(
        "entry {} on \"{}\" ({} … {})",
        row.entry_id,
        row.task_title,
        row.start.to_rfc3339(),
        row.end
            .map_or_else(|| "open".to_owned(), |end| end.to_rfc3339()),
    );
    if !receipt.changed {
        line.push_str(" — nothing to change");
    }
    line
}

/// The human-readable `delete` receipt.
#[must_use]
pub fn render_text_delete(receipt: &TrackDeleteReceipt) -> String {
    if receipt.deleted {
        format!("deleted entry {}", receipt.entry_id)
    } else {
        format!(
            "entry {} is already absent (nothing to delete)",
            receipt.entry_id
        )
    }
}

/// The one-shot `track` dispatch: the mode switch over
/// [`super::run_core`], each mode with its own handler and renderer.
#[allow(clippy::too_many_lines)] // one arm per subcommand, kept flat for review
pub async fn track_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<PathBuf>,
    invocation: TrackInvocation,
) -> u8 {
    match invocation {
        TrackInvocation::Status => {
            super::run_core(io, config_path, render_text_status, |config, store| {
                let day = config.resolved_day_calendar();
                async move { status(config, store, day).await }
            })
            .await
        }
        TrackInvocation::List { task, since } => super::run_core(
            io,
            config_path,
            render_text_list,
            move |config, store| async move { list(config, store, task.as_deref(), since).await },
        )
        .await,
        TrackInvocation::Stop => {
            super::run_core(
                io,
                config_path,
                render_text_stop,
                |config, store| async move { stop(config, store).await },
            )
            .await
        }
        TrackInvocation::Start { task } => {
            super::run_core(
                io,
                config_path,
                render_text_start,
                move |config, store| async move { start(config, store, &task).await },
            )
            .await
        }
        TrackInvocation::Log { task, mode } => {
            super::run_core(
                io,
                config_path,
                render_text_log,
                move |config, store| async move { log(config, store, &task, mode).await },
            )
            .await
        }
        TrackInvocation::Adjust { entry, start, end } => {
            super::run_core(
                io,
                config_path,
                render_text_adjust,
                move |config, store| async move { adjust(config, store, &entry, start, end).await },
            )
            .await
        }
        TrackInvocation::Delete { entry } => {
            super::run_core(
                io,
                config_path,
                render_text_delete,
                move |config, store| async move { delete(config, store, &entry).await },
            )
            .await
        }
    }
}

/// The session-lane mode switch (`dispatch_line`): pure core reads and
/// mutations, no boot. The status total rides the session's resolved
/// day calendar, passed as data (D-TC5/D-TC13).
pub async fn track_line(
    app: &crate::App,
    day: DayCalendar,
    io: &mut crate::commands::CommandIo<'_>,
    invocation: TrackInvocation,
) -> u8 {
    match invocation {
        TrackInvocation::Status => {
            crate::commands::emit_result(io, Ok(status_on(app, day)), render_text_status)
        }
        TrackInvocation::List { task, since } => {
            crate::commands::emit_result(io, list_on(app, task.as_deref(), since), render_text_list)
        }
        TrackInvocation::Stop => {
            crate::commands::emit_result(io, stop_on(app).await, render_text_stop)
        }
        TrackInvocation::Start { task } => {
            crate::commands::emit_result(io, start_on(app, &task).await, render_text_start)
        }
        TrackInvocation::Log { task, mode } => {
            crate::commands::emit_result(io, log_on(app, &task, mode).await, render_text_log)
        }
        TrackInvocation::Adjust { entry, start, end } => crate::commands::emit_result(
            io,
            adjust_on(app, &entry, start, end).await,
            render_text_adjust,
        ),
        TrackInvocation::Delete { entry } => {
            crate::commands::emit_result(io, delete_on(app, &entry).await, render_text_delete)
        }
    }
}

/// The one-shot `export` dispatch. Not [`super::run_core`]: the payload
/// bypasses the receipt envelope — raw bytes on `out` regardless of
/// `--json` (D-TC10); failures still ride the typed envelope on `err`.
pub async fn export_command(
    io: &mut crate::commands::CommandIo<'_>,
    config_path: Option<PathBuf>,
    query: ExportQuery,
) -> u8 {
    match super::core_ctx(config_path) {
        Ok((config, store)) => match export(config, store, query).await {
            Ok(payload) => {
                let _ = io.out.write_all(render_payload(&payload).as_bytes());
                0
            }
            Err(err) => crate::commands::report_to(io, &err),
        },
        Err(err) => crate::commands::report_to(io, &err),
    }
}

/// The session-lane `export` (the mirror of [`export_command`]): raw
/// bytes on `out` regardless of `--json`; the daemon's captured-buffer
/// relay carries them byte-verbatim (ADR 0012 — no relay change).
pub fn export_line(
    app: &crate::App,
    io: &mut crate::commands::CommandIo<'_>,
    query: &ExportQuery,
) -> u8 {
    match export_on(app, query) {
        Ok(payload) => {
            let _ = io.out.write_all(render_payload(&payload).as_bytes());
            0
        }
        Err(err) => crate::commands::report_to(io, &err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;
    use taskboard_domain::TaskId;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    fn entry(
        raw: u128,
        task_raw: u128,
        title: &str,
        start: i64,
        end: Option<i64>,
    ) -> taskboard_domain::TimeEntry {
        taskboard_domain::TimeEntry {
            id: TimeEntryId::from(uuid::Uuid::from_u128(raw)),
            task: TaskId::from(uuid::Uuid::from_u128(task_raw)),
            task_title: title.to_owned(),
            start: ts(start),
            end: end.map(ts),
            created_at: ts(100),
            updated_at: ts(100),
        }
    }

    fn tracking_state(entries: &[taskboard_domain::TimeEntry]) -> TrackingState {
        let mut state = TrackingState::default();
        for e in entries {
            state.entries.insert(e.id, e.clone());
        }
        state
    }

    fn phrase_of_seconds(secs: i64) -> String {
        duration_phrase(TimeDelta::try_seconds(secs).expect("secs"))
    }

    // ---- duration_phrase --------------------------------------------------

    #[test]
    fn duration_phrase_composes_hours_and_minutes_at_minute_granularity() {
        assert_eq!(
            phrase_of_seconds(90 * 60),
            "1h 30m",
            "90m composes to 1h 30m"
        );
        assert_eq!(
            phrase_of_seconds(60 * 60),
            "1h 0m",
            "hours-only keeps the zero minutes"
        );
        assert_eq!(phrase_of_seconds(0), "0m", "zero is the honest 0m");
        assert_eq!(phrase_of_seconds(45), "0m", "sub-minute truncates to 0m");
        assert_eq!(phrase_of_seconds(-30), "0m", "negative clamps at zero");
    }

    // ---- csv_field ----------------------------------------------------------

    #[test]
    fn csv_field_quotes_only_when_a_special_byte_is_present() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field(""), "");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("a\"b"), "\"a\"\"b\"", "embedded quotes double");
        assert_eq!(csv_field("a\nb"), "\"a\nb\"");
        assert_eq!(
            csv_field("a\r\nb"),
            "\"a\r\nb\"",
            "CRLF quotes like any newline class"
        );
    }

    // ---- parse grammar lives beside the fns (cli.rs); the collapse is here —

    #[test]
    fn collapse_yields_every_subcommand_exactly_once() {
        use crate::cli::TrackCmd;
        assert!(matches!(
            collapse(TrackCmd::Start { task: "t".into() }),
            Some(TrackInvocation::Start { ref task }) if task == "t"
        ));
        assert!(matches!(
            collapse(TrackCmd::Stop),
            Some(TrackInvocation::Stop)
        ));
        assert!(matches!(
            collapse(TrackCmd::Status),
            Some(TrackInvocation::Status)
        ));
        assert!(matches!(
            collapse(TrackCmd::List {
                task: None,
                since: None
            }),
            Some(TrackInvocation::List {
                task: None,
                since: None
            })
        ));
        assert!(matches!(
            collapse(TrackCmd::Adjust { entry: "e".into(), start: None, end: None }),
            Some(TrackInvocation::Adjust { ref entry, start: None, end: None }) if entry == "e"
        ));
        assert!(matches!(
            collapse(TrackCmd::Delete { entry: "e".into() }),
            Some(TrackInvocation::Delete { ref entry }) if entry == "e"
        ));
    }

    #[test]
    fn collapse_folds_the_three_log_modes_and_keeps_the_vacuous_group_total() {
        use crate::cli::TrackCmd;
        let for_mode = collapse(TrackCmd::Log {
            task: "t".into(),
            for_: Some(TimeDelta::try_minutes(90).expect("90m")),
            from: None,
            to: None,
        });
        assert!(
            matches!(
                for_mode,
                Some(TrackInvocation::Log {
                    mode: LogMode::For(_),
                    ..
                })
            ),
            "--for collapses to the For mode"
        );
        let from = ts(1_000);
        let to = ts(2_000);
        let span_mode = collapse(TrackCmd::Log {
            task: "t".into(),
            for_: None,
            from: Some(from),
            to: Some(to),
        });
        let Some(TrackInvocation::Log {
            task: span_task,
            mode: LogMode::Span(span_from, span_to),
        }) = span_mode
        else {
            panic!("--from/--to collapse to the absolute span");
        };
        assert_eq!(span_task, "t");
        assert_eq!((span_from, span_to), (from, to));
        // The clap-unreachable vacuous group stays total for direct calls.
        assert!(
            collapse(TrackCmd::Log {
                task: "t".into(),
                for_: None,
                from: None,
                to: None
            })
            .is_none()
        );
    }

    // ---- resolve_time_entry -------------------------------------------------

    fn app_state_with(entries: &[taskboard_domain::TimeEntry]) -> AppState {
        AppState {
            tracking: tracking_state(entries),
            ..AppState::default()
        }
    }

    #[test]
    fn resolve_time_entry_hits_exact_uuid_then_unique_prefix() {
        // The u128 values are placed in the uuid's most-significant
        // hex digits so the first 8 chars of the textual form differ.
        let full = uuid::Uuid::from_u128(0xa1b2_c3d4_0000_0000_0000_0000_0000_0001);
        let other = uuid::Uuid::from_u128(0xfedc_ba09_0000_0000_0000_0000_0000_0002);
        let state = app_state_with(&[
            entry(
                0xa1b2_c3d4_0000_0000_0000_0000_0000_0001,
                900,
                "tea",
                1_000,
                None,
            ),
            entry(
                0xfedc_ba09_0000_0000_0000_0000_0000_0002,
                900,
                "toast",
                2_000,
                None,
            ),
        ]);
        let id = resolve_time_entry(&state, &full.to_string()).expect("exact uuid hit");
        assert_eq!(id.as_uuid(), full);
        let selector = full.to_string();
        let id = resolve_time_entry(&state, &selector[..8]).expect("unique 8-char prefix hit");
        assert_eq!(id.as_uuid(), full);
        // A <8-char non-uuid selector is a miss, not an ambiguity.
        let err = resolve_time_entry(&state, "aa").expect_err("too short is a miss");
        assert!(matches!(err, CommandError::UnknownTimeEntry { ref selector } if selector == "aa"));
        assert_eq!(err.error_kind(), "unknown-time-entry");
        let _ = other;
    }

    #[test]
    fn resolve_time_entry_names_both_candidates_on_a_prefix_collision() {
        // Minted v7 ids cannot be forced to collide hermetically —
        // hand-built twins sharing an 8-char prefix (the tracking.rs
        // test module's own Uuid::from_u128 pattern).
        let twin_a = uuid::Uuid::from_u128(0x1234_0000_0000_0001);
        let twin_b = uuid::Uuid::from_u128(0x1234_0000_0000_0002);
        let state = app_state_with(&[
            entry(0x1234_0000_0000_0001, 900, "a", 1_000, None),
            entry(0x1234_0000_0000_0002, 900, "b", 2_000, None),
        ]);
        let err = resolve_time_entry(&state, &twin_a.to_string()[..8]).expect_err("two hits");
        let CommandError::AmbiguousTimeEntry { candidates } = err else {
            panic!("the shared prefix is typed ambiguous");
        };
        assert_eq!(candidates.len(), 2);
        assert!(candidates.contains(&twin_a.to_string()));
        assert!(candidates.contains(&twin_b.to_string()));
        assert_eq!(
            resolve_time_entry(&state, &twin_a.to_string())
                .err()
                .map(|e| e.error_kind()),
            None,
            "the full uuid still hits exactly"
        );
    }

    #[test]
    fn resolve_time_entry_refuses_misses_typedly() {
        let state = app_state_with(&[entry(0xaaa1, 900, "tea", 1_000, None)]);
        let err = resolve_time_entry(&state, "ffffffff").expect_err("no hit");
        assert!(matches!(err, CommandError::UnknownTimeEntry { .. }));
        assert_eq!(err.error_kind(), "unknown-time-entry");
    }

    // ---- render goldens ------------------------------------------------------

    fn row(raw: u128, title: &str, start: i64, end: Option<i64>, duration: u64) -> ExportRow {
        let entry = entry(raw, 900, title, start, end);
        TrackEntryRow {
            duration_seconds: duration,
            ..entry_row(&entry, ts(0))
        }
    }

    /// The golden fixtures are data, constructed once, asserted
    /// verbatim (fixed timestamps — boot-stamped rows are never
    /// byte-golded).
    #[test]
    fn render_csv_golden_keeps_header_order_and_the_open_empty_end_cell() {
        let rows = vec![
            row(
                0x0011,
                "tea, with \"quotes\"\nand a newline",
                1_000,
                Some(4_600),
                3_600,
            ),
            row(0x0012, "open work", 5_000, None, 300),
        ];
        let csv = render_csv(&rows);
        let expected = concat!(
            "entry_id,task_id,task_title,start,end,duration_seconds\n",
            "00000000-0000-0000-0000-000000000011,00000000-0000-0000-0000-000000000384,",
            "\"tea, with \"\"quotes\"\"\nand a newline\",",
            "1970-01-01T00:16:40+00:00,1970-01-01T01:16:40+00:00,3600\n",
            "00000000-0000-0000-0000-000000000012,00000000-0000-0000-0000-000000000384,",
            "open work,1970-01-01T01:23:20+00:00,,300\n",
        );
        assert_eq!(csv, expected);
    }

    #[test]
    fn render_json_golden_is_a_single_line_array_with_null_end_for_open_rows() {
        let rows = vec![row(0x0012, "open work", 5_000, None, 300)];
        let json = render_json(&rows);
        assert_eq!(
            json,
            concat!(
                "[{\"entry_id\":\"00000000-0000-0000-0000-000000000012\",",
                "\"task_id\":\"00000000-0000-0000-0000-000000000384\",",
                "\"task_title\":\"open work\",",
                "\"start\":\"1970-01-01T01:23:20Z\",\"end\":null,\"duration_seconds\":300}]\n",
            )
        );
        assert_eq!(json.lines().count(), 1, "single line, newline-terminated");
        let empty = render_json(&[]);
        assert_eq!(empty, "[]\n", "zero rows export as an empty array");
        assert_eq!(
            render_csv(&[]),
            format!("{EXPORT_CSV_HEADER}\n"),
            "zero rows are header-only"
        );
    }

    // ---- list / status projections ------------------------------------------

    #[test]
    fn list_view_orders_canonically_and_filters_by_task_and_since() {
        let task_a = TaskId::from(uuid::Uuid::from_u128(900));
        // Two days before the epoch, the day before, and inside
        // 1970-01-01 — so the `--since` cut has a day boundary to bite.
        let early_a = entry(1, 900, "a early", -2 * 86_400, Some(-2 * 86_400 + 1_000));
        let late_a = entry(2, 900, "a late", 1_000, Some(2_000));
        let other_b = entry(3, 901, "b", -86_400, None);
        let state = tracking_state(&[late_a.clone(), early_a.clone(), other_b.clone()]);

        let view = list_view(&state, None, None, ts(9_999));
        assert_eq!(
            view.entries
                .iter()
                .map(|r| r.entry_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                early_a.id.as_uuid().to_string(),
                other_b.id.as_uuid().to_string(),
                late_a.id.as_uuid().to_string(),
            ],
            "canonical (start, id) ascending, not insertion order"
        );

        let view = list_view(&state, Some(task_a), None, ts(9_999));
        assert_eq!(view.entries.len(), 2, "the other task's entries vanish");
        assert!(
            view.entries
                .iter()
                .all(|r| r.task_id == task_a.as_uuid().to_string())
        );

        // `--since` cuts by start: midnight UTC of 1970-01-01 keeps
        // only the in-day row, drops the two earlier days' entries
        // (the unfiltered open row of task b is dropped by the day cut
        // too).
        let view = list_view(
            &state,
            None,
            Some(NaiveDate::from_ymd_opt(1970, 1, 1).expect("date")),
            ts(9_999),
        );
        assert_eq!(view.entries.len(), 1);
        assert_eq!(view.entries[0].entry_id, late_a.id.as_uuid().to_string());
    }

    #[test]
    fn status_view_lists_open_newest_first_and_splits_spanning_entries_at_the_window() {
        // Non-midnight day start, explicit fixed offset — host-TZ
        // independent by design (the D-F6 contract).
        let day = DayCalendar {
            day_start_time: chrono::NaiveTime::from_hms_opt(5, 0, 0).expect("05:00"),
            offset: chrono::FixedOffset::east_opt(2 * 3_600).expect("+02:00"),
        };
        // The window containing `now`: 05:00 local at +02:00 opens at
        // 03:00Z — but `now` = 03:46:40Z on 1970-01-01 is *before* that
        // opening, so the window stepped back a day:
        // [1969-12-31T03:00Z, 1970-01-01T03:00Z) = [-75_600, 10_800).
        let now = ts(10_000);
        let inside = entry(1, 900, "inside", 4_000, Some(5_000)); // fully in-window: 1000s
        // Spans the window start (−75_600 = 1969-12-31T03:00Z): only
        // [−75_600, −75_300] counts → 300s.
        let spanning = entry(2, 900, "spanning", -79_200, Some(-75_300));
        let open = entry(3, 900, "open", 6_000, None); // accrues [6_000, 10_000) → 4000s
        let later_open = entry(5, 900, "later open", 7_000, None); // newest open
        let closed_outside = entry(4, 900, "before the window", -90_000, Some(-89_000)); // 0
        let state = tracking_state(&[spanning, closed_outside, open.clone(), later_open.clone()]);
        let mut state = state;
        state.entries.insert(inside.id, inside.clone());

        let view = status_view(&state, day, now);
        // Newest-first open set: the later open entry, then the earlier.
        assert_eq!(view.open.len(), 2, "all open entries are shown honestly");
        assert_eq!(view.open[0].entry_id, later_open.id.as_uuid().to_string());
        assert_eq!(view.open[1].entry_id, open.id.as_uuid().to_string());
        // The exact split-at-the-boundary sum: 1000 (inside) + 300
        // (the spanning entry's in-window segment) + 4000 + 3000 (the
        // two open entries, accrued to `now`); the out-of-window closed
        // row contributes nothing.
        assert_eq!(view.tracked_today_seconds, 1000 + 300 + 4_000 + 3_000);
    }
}
