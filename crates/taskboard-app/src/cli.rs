// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI surface (ADRs 0008/0009): clap types only — dispatch lives in
//! `commands::dispatch` (the lib); `main` is a thin process shell.

use std::path::PathBuf;

use clap::Subcommand;

/// The `taskboard` command line.
#[derive(Debug, clap::Parser)]
#[command(name = "taskboard", version = crate::VERSION, about = "Taskboard: offline-first task sync for desktop and kiosk")]
pub struct Cli {
    /// Path to the config file. Resolution order: this flag >
    /// `TASKBOARD_CONFIG` > `$XDG_CONFIG_HOME/taskboard/taskboard.toml`
    /// (when it exists) > `./taskboard.toml` (absent tolerated).
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Machine-readable output on stdout; errors as JSON on stderr.
    #[arg(long, global = true)]
    pub json: bool,
    /// The subcommand; `None` resolves `[app] mode` (the daemon until
    /// the UI phases land).
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommand catalogue.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Connect this installation to a Nextcloud server: verify the
    /// credentials against the server, then persist the connection (the
    /// non-secrets to the config, the secret to the credential store).
    Connect {
        /// Server URL (prompted when omitted).
        #[arg(long)]
        server: Option<String>,
        /// Login name (prompted when omitted).
        #[arg(long)]
        user: Option<String>,
        /// Rewrite an explicit `sync.backend = "standalone"` to
        /// `nextcloud` — the scripted loud flip. Without it a terminal
        /// asks (y/N); a non-interactive run refuses typed.
        #[arg(long)]
        switch_backend: bool,
        /// Re-point an already-configured connection to a different
        /// server/user without the interactive confirm (the old
        /// identity's secret is deleted after the new one verifies).
        #[arg(long)]
        force: bool,
    },
    /// Deprecated alias of `connect` (removal revisited at v0.1.0):
    /// the same ceremony under the old name, without the
    /// `--switch-backend`/`--force` escapes — an explicit `standalone`
    /// backend or a configured different identity refuses typed,
    /// naming `connect`.
    Login {
        /// Server URL (prompted when omitted).
        #[arg(long)]
        server: Option<String>,
        /// Login name (prompted when omitted).
        #[arg(long)]
        user: Option<String>,
    },
    /// Board discovery, stand-alone board creation, and the sync
    /// binding/publish ceremonies.
    Boards {
        #[command(subcommand)]
        cmd: BoardsCmd,
    },
    /// Stack verbs over the in-process core.
    Stacks {
        #[command(subcommand)]
        cmd: StacksCmd,
    },
    /// Task verbs over the in-process core.
    Tasks {
        #[command(subcommand)]
        cmd: TasksCmd,
    },
    /// Label verbs over the in-process core (uuid | exact title selectors).
    Labels {
        #[command(subcommand)]
        cmd: LabelsCmd,
    },
    /// Fuzzy title search across all live boards, stacks, and tasks
    /// (done included); prints ranked rows — an exact title first, then
    /// prefixes, word starts, substrings, subsequences. The scripting
    /// bridge over the same ranking the GUI palette uses.
    Find {
        /// The query: case-insensitive, trimmed; an empty (or
        /// whitespace-only) query matches nothing.
        query: String,
        /// Restrict the results to one entity kind. An unknown word is
        /// a clap usage error, never a runtime string compare.
        #[arg(long = "type")]
        kind: Option<FindType>,
        /// Cap the ranked output (applied after ranking).
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
        limit: Option<u64>,
    },
    /// Quick capture (#347): lands the thought in the global Inbox
    /// (created unbound on first capture); with `--board`, in that
    /// board's Backlog stack (created on demand). Never errors on the
    /// happy path.
    Capture {
        /// The captured thought, verbatim (the engine does not
        /// validate titles; the UI trims at its edge).
        title: String,
        /// Target board (exact uuid | exact title among the live
        /// boards). Omit for the global Inbox.
        #[arg(long)]
        board: Option<String>,
    },
    /// Standalone timer/alarm verbs over the in-process core: fire once
    /// at `--in`/`--at`, die by settlement. Device-local — never synced.
    Timers {
        #[command(subcommand)]
        cmd: TimersCmd,
    },
    /// Time tracking over the in-process core (D-T14): start/stop the
    /// running timer, log retroactive blocks, adjust/delete history
    /// (ADR 0017 — the open entry *is* the timer; nothing ticks).
    Track {
        #[command(subcommand)]
        cmd: TrackCmd,
    },
    /// Habit trackers (DR-6/6): add/list/log/adjust/update/rm over the
    /// in-process core. Trackers are board-scoped today-view items;
    /// `anchor_time` is a display chip and never an ordering input.
    Habits {
        #[command(subcommand)]
        cmd: HabitsCmd,
    },
    /// Export time entries to stdout as data (D-T15): CSV by default,
    /// JSON optional. Pipes are the contract — no file writing, no
    /// clipboard — and the payload deliberately bypasses the `--json`
    /// receipt envelope (the recorded D-T15 exception).
    Export {
        /// Output format (csv by default).
        #[arg(long, value_enum)]
        format: Option<ExportFormat>,
        /// Only entries starting on/after this UTC date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        since: Option<chrono::NaiveDate>,
        /// Only entries starting before the day after this UTC date —
        /// inclusive of the named day (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        until: Option<chrono::NaiveDate>,
        /// Only entries of this task (uuid | unique ≥8-char uuid
        /// prefix | exact live title).
        #[arg(long)]
        task: Option<String>,
        /// Only entries whose task's stack belongs to this board
        /// (exact uuid | exact title among the live boards).
        #[arg(long)]
        board: Option<String>,
    },
    /// Delete the stored Nextcloud credentials (and, under the auto
    /// backend, the config's server/user keys). Idempotent; the board
    /// binding and local db are untouched.
    Logout,
    /// Run one full sync cycle now and report the status.
    Sync {
        /// Return immediately after nudging the sync actor.
        #[arg(long)]
        no_wait: bool,
        /// Scope the reported status (and the wait's watched aggregate)
        /// to this connector's bound boards; the nudge stays broadcast.
        /// The default (first) connector when omitted.
        #[arg(long)]
        connector: Option<String>,
        /// Wait budget for the cycle, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Effective-configuration diagnostics.
    Config {
        #[command(subcommand)]
        cmd: ConfigCmd,
    },
    /// Who serves this database and how is sync (never boots a core,
    /// never triggers a sync cycle). With a running daemon: the live
    /// view over IPC; without: the lock sidecar's diagnostic PID.
    Status,
    /// Run the headless sync daemon (M1) — boots the core, binds the
    /// per-database IPC socket, and serves commands until a signal or
    /// `daemon stop`.
    Daemon {
        #[command(subcommand)]
        cmd: Option<DaemonCmd>,
    },
    /// Run the desktop UI in this process: explicit frontend selection
    /// (the `daemon` verb's mirror) — boots the core and opens the
    /// desktop window regardless of `[app] mode` and of TTY. A running
    /// instance on the same database fails typed `instance-locked`.
    Gui,
}

/// The `find --type` values: the one entity kind a result row may name.
/// The workspace's first clap `ValueEnum` — validation is the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum FindType {
    /// Task cards.
    Task,
    /// Stacks (columns).
    Stack,
    /// Boards.
    Board,
}

/// `daemon` subcommands.
#[derive(Debug, Subcommand)]
pub enum DaemonCmd {
    /// Gracefully stop a running daemon over its IPC socket (typed
    /// `no-daemon` failure, exit 1, when nothing serves this database).
    Stop,
}

/// `boards` subcommands.
#[derive(Debug, Subcommand)]
pub enum BoardsCmd {
    /// List the server's boards (remote ids feed `boards select`).
    List {
        /// The connector whose server is listed; the default (first)
        /// connector when omitted.
        #[arg(long)]
        connector: Option<String>,
    },
    /// Bind this installation to one board (refuses a rebind; migrates a
    /// local board with `--bind-local-board`; pins the local board with
    /// `--board` when several are live).
    Select {
        /// The remote Deck board id (from `boards list`). The internal
        /// clap id is `remote_board` (completion routes it as server
        /// state); the displayed syntax is unchanged.
        #[arg(id = "remote_board", value_name = "board")]
        board: u64,
        /// The connector whose listing pre-validates the id and whose
        /// binding tag the bind mints; the default (first) connector
        /// when omitted.
        #[arg(long)]
        connector: Option<String>,
        /// The local board (uuid) to bind; the single live board when
        /// omitted, a freshly minted one on a board-less install.
        #[arg(long, value_parser = parse_board_id)]
        local_board: Option<taskboard_domain::BoardId>,
        /// Adoption wait budget, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        /// Bind the existing local board onto the remote board
        /// (migration path for stand-alone data).
        #[arg(long)]
        bind_local_board: bool,
    },
    /// Create a local board (stand-alone first-run command; always
    /// mints — titles are not uniqueness keys).
    Create {
        /// The board title.
        title: String,
        /// Board color as six hex digits (no `#`); a default applies
        /// when omitted.
        #[arg(long)]
        color: Option<String>,
    },
    /// Rename a local board's title (both backends; the rename pushes
    /// when the board is bound, and rides the next `publish` otherwise).
    Rename {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The new board title.
        title: String,
    },
    /// Publish a local board as a new Nextcloud board and bind it
    /// (the migration path for stand-alone data; sync-only).
    Publish {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The connector whose server the remote board is created on
        /// and whose binding tag the bind mints; the default (first)
        /// connector when omitted.
        #[arg(long)]
        connector: Option<String>,
        /// Wait budget for the settling first sync cycle, in seconds
        /// (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Set the session's board selection (REPL-only): later board-scoped
    /// lines pre-fill `--board` from it. Without a uuid, answers the
    /// current selection. One-shot and daemon-socket invocations refuse.
    Use {
        /// The local board (uuid); omit to query the selection.
        #[arg(value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Set a board's color (pushes as a full-send when the board is
    /// bound).
    Color {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Board color as six hex digits (no `#`).
        #[arg(value_parser = parse_color)]
        color: String,
    },
    /// Archive a board (archived boards stay commandable; the GUI
    /// stops listing them).
    Archive {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Unarchive a board.
    Unarchive {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Delete a board: tombstone-cascades its stacks, tasks, and
    /// labels, cancels the pending ops of the doomed entities, and
    /// queues the remote delete (a bound board's remote content is
    /// destroyed server-side on the next cycle).
    Delete {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the destruction (the refusal without it is typed and
        /// states what would be destroyed).
        #[arg(long)]
        confirm: bool,
    },
    /// Reset a board's local content (stacks, tasks, labels, outbox,
    /// validators, reminder facts) in one local-only repair; the board
    /// row and binding survive. A bound board re-materializes from the
    /// next pull.
    Reset {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the repair (the refusal without it is typed and
        /// states what would be cleared).
        #[arg(long)]
        confirm: bool,
    },
    /// Detach the board's remote binding: content and queued ops stay
    /// (the ops strand honestly and drain on a re-bind); the binding
    /// and the board's sync-status row go. Repeat is an idempotent
    /// no-op.
    Unbind {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the unbind (the refusal without it is typed and
        /// states what will strand).
        #[arg(long)]
        confirm: bool,
    },
    /// Deterministically merge same-title duplicate entities the bind
    /// ceremony's pull adopted alongside pre-existing local rows
    /// (survivors keep the local ids; duplicate rows neutralize).
    /// Idempotent.
    Reconcile {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// Confirm the merge (the refusal without it is typed and
        /// states the match counts).
        #[arg(long)]
        confirm: bool,
    },
}

/// `stacks` subcommands. The stack selector is the uuid or the exact
/// live title (prefixes are a task-only affordance).
#[derive(Debug, Subcommand)]
pub enum StacksCmd {
    /// Create a stack on a board (appended by default).
    Add {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The stack title.
        title: String,
        /// Explicit sort order; default appends to the end of the board.
        #[arg(long)]
        order: Option<i64>,
    },
    /// Rename a stack.
    Rename {
        /// The stack (uuid | exact title).
        stack: String,
        /// The new title.
        title: String,
    },
    /// Delete a stack and tombstone-cascade its tasks (no confirmation;
    /// repeats are idempotent).
    Delete {
        /// The stack (uuid | exact title).
        stack: String,
    },
    /// Reorder a stack within the board (no-op when unchanged).
    Move {
        /// The stack (uuid | exact title).
        stack: String,
        /// The new sort position.
        #[arg(long)]
        order: i64,
    },
}

/// The default label color: Deck's UI default green. `labels add`
/// applies it when `--color` is omitted.
pub const DEFAULT_LABEL_COLOR: &str = "31cc7c";

/// Six lowercase hex digits or a usage error — labels and boards (an
/// invalid color is a permanent push rejection; the lenient domain
/// value stays for adopted remote content, new input gets this boundary
/// validation).
fn parse_color(raw: &str) -> Result<String, String> {
    let lower = raw.to_lowercase();
    if lower.len() == 6 && lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(lower);
    }
    Err("expected exactly six hex digits (e.g. 31cc7c, no '#')".to_owned())
}

/// A local board uuid or a usage error — the `--board` flag of the
/// board-scoped verbs. A malformed value is a clap usage error (exit 2),
/// the `parse_color` precedent.
fn parse_board_id(raw: &str) -> Result<taskboard_domain::BoardId, String> {
    uuid::Uuid::parse_str(raw)
        .map(taskboard_domain::BoardId::from_uuid)
        .map_err(|_| "expected a board uuid".to_owned())
}

#[test]
fn parse_board_id_accepts_uuids_and_rejects_garbage() {
    let good = "00000000-0000-0000-0000-000000000001";
    assert_eq!(
        parse_board_id(good),
        Ok(taskboard_domain::BoardId::from_uuid(
            uuid::Uuid::parse_str(good).unwrap()
        ))
    );
    for bad in ["", "not-a-uuid", "3"] {
        assert!(parse_board_id(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn parse_color_normalizes_six_hex_digits() {
    assert_eq!(parse_color("31CC7C"), Ok("31cc7c".to_owned()));
    assert_eq!(parse_color("AbCdEf"), Ok("abcdef".to_owned()));
}

#[test]
fn parse_color_rejects_everything_else() {
    for bad in ["#fff", "fff", "ffffff0", "zzzzzz", "31cc7", ""] {
        assert!(parse_color(bad).is_err(), "{bad} must be rejected");
    }
}

/// `--due` grammar: set (RFC-3339 with an explicit offset, normalized
/// to UTC, or bare `YYYY-MM-DD` → midnight UTC) or clear (`none`).
/// Due dates are stored and displayed in UTC only; offset-less
/// timestamps are refused.
#[derive(Debug, Clone, PartialEq)]
pub enum DueSpec {
    /// Set the due date.
    Set(chrono::DateTime<chrono::Utc>),
    /// Clear the due date.
    Clear,
}

/// Parses the `--due` value: RFC-3339 with an explicit offset
/// (normalized to UTC), a bare `YYYY-MM-DD` (midnight UTC), or the
/// literal `none`. Due dates are stored and displayed in UTC only;
/// offset-less timestamps are refused. A malformed value is a clap
/// usage error (exit 2).
fn parse_due(raw: &str) -> Result<DueSpec, String> {
    if raw == "none" {
        return Ok(DueSpec::Clear);
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| DueSpec::Set(dt.with_timezone(&chrono::Utc)))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map(|date| {
                DueSpec::Set(
                    date.and_hms_opt(0, 0, 0)
                        .expect("midnight is valid")
                        .and_utc(),
                )
            })
        })
        .map_err(|_| "expected RFC-3339 (2026-10-24T09:00:00Z), YYYY-MM-DD, or 'none'".to_owned())
}

/// `--effort` grammar: set (the domain text form) or clear (`none`) —
/// the [`DueSpec`] shape, because clap's derive wraps `Option<Option<T>>`
/// one layer too many for a wrapper returning `Option`.
#[derive(Debug, Clone, PartialEq)]
pub enum EffortSpec {
    /// Set the effort.
    Set(taskboard_domain::Effort),
    /// Clear the effort.
    Clear,
}

/// `--importance` grammar: set (a vocabulary level) or clear (`none`).
#[derive(Debug, Clone, PartialEq)]
pub enum ImportanceSpec {
    /// Set the level.
    Set(taskboard_domain::Importance),
    /// Clear the level.
    Clear,
}

/// `--urgency` grammar: set (a vocabulary level) or clear (`none`) —
/// distinct from [`ImportanceSpec`] by the ruled type separation.
#[derive(Debug, Clone, PartialEq)]
pub enum UrgencySpec {
    /// Set the level.
    Set(taskboard_domain::Urgency),
    /// Clear the level.
    Clear,
}

/// Parses the `--effort` value over the domain text form
/// ([`taskboard_domain::Effort::parse_text_form`]): bare digits are
/// minutes, `s`/`m`/`h` components compose (`1h30m`). `none` clears;
/// anything unparsable is a clap usage error (exit 2).
fn parse_effort(raw: &str) -> Result<EffortSpec, String> {
    if raw == "none" {
        return Ok(EffortSpec::Clear);
    }
    taskboard_domain::Effort::parse_text_form(raw)
        .map(EffortSpec::Set)
        .ok_or_else(|| {
            "expected <n><unit> components (s|m|h; bare digits = minutes), e.g. 30 or 1h30m, or 'none'"
                .to_owned()
        })
}

/// Parses `--importance` over the domain's hyphenated level vocabulary;
/// `none` clears. Anything unmatched is a clap usage error (exit 2).
fn parse_importance(raw: &str) -> Result<ImportanceSpec, String> {
    if raw == "none" {
        return Ok(ImportanceSpec::Clear);
    }
    taskboard_domain::Importance::parse_text_form(raw)
        .map(ImportanceSpec::Set)
        .ok_or_else(|| "expected very-low|low|normal|high|very-high|none".to_owned())
}

/// Parses `--urgency` over the domain's hyphenated level vocabulary;
/// `none` clears. Anything unmatched is a clap usage error (exit 2).
fn parse_urgency(raw: &str) -> Result<UrgencySpec, String> {
    if raw == "none" {
        return Ok(UrgencySpec::Clear);
    }
    taskboard_domain::Urgency::parse_text_form(raw)
        .map(UrgencySpec::Set)
        .ok_or_else(|| "expected very-low|low|normal|high|very-high|none".to_owned())
}

/// Parses `--insistence` over the reminder rung vocabulary
/// (`normal`|`alarm`; no `none` — a rule is created at one rung, the
/// default folds at the collapse boundary). Anything unmatched is a
/// clap usage error (exit 2), the `parse_urgency` shape.
fn parse_insistence_flag(raw: &str) -> Result<taskboard_domain::Insistence, String> {
    taskboard_domain::Insistence::parse_text_form(raw)
        .ok_or_else(|| "expected normal|alarm".to_owned())
}

/// The `tasks remind --before` flag: the same `[reminders]
/// default_before` grammar ([`crate::config::parse_before`]). A
/// malformed value is a clap usage error (exit 2), the `parse_due`
/// precedent.
fn parse_before_flag(raw: &str) -> Result<chrono::TimeDelta, String> {
    crate::config::parse_before(raw)
        .ok_or_else(|| "expected <int><unit> components (s|m|h|d), e.g. 10m or 1h30m".to_owned())
}

/// The `--weekly` value: the empty default-missing value is the bare
/// flag's task-weekday default; anything else goes through the
/// domain's weekday-set text form — one parser, one taxonomy (the
/// `parse_effort` shape).
fn parse_weekdays_flag(raw: &str) -> Result<WeekdaysFlag, String> {
    if raw.is_empty() {
        return Ok(WeekdaysFlag::TaskWeekday);
    }
    taskboard_domain::WeekdaySet::parse_text_form(raw)
        .map(WeekdaysFlag::Days)
        .map_err(|err| err.to_string())
}

/// The `--monthly` value: the empty default-missing value is the bare
/// flag's task-day default; anything else is a
/// [`taskboard_domain::MonthlyAnchor`] text form.
fn parse_monthly_flag(raw: &str) -> Result<MonthlyFlag, String> {
    if raw.is_empty() {
        return Ok(MonthlyFlag::TaskDay);
    }
    taskboard_domain::MonthlyAnchor::parse_text_form(raw)
        .map(MonthlyFlag::Anchor)
        .map_err(|err| err.to_string())
}

/// Wraps the domain's day-interval text form (`3` | `3d`).
fn parse_every_days_flag(raw: &str) -> Result<taskboard_domain::EveryDays, String> {
    taskboard_domain::EveryDays::parse_text_form(raw).map_err(|err| err.to_string())
}

/// Wraps the domain's week-interval text form (`2` | `2w`).
fn parse_every_weeks_flag(raw: &str) -> Result<taskboard_domain::EveryWeeks, String> {
    taskboard_domain::EveryWeeks::parse_text_form(raw).map_err(|err| err.to_string())
}

/// Wraps the domain's occurrence-count text form (bare digits, ≥ 1).
fn parse_count_flag(raw: &str) -> Result<taskboard_domain::OccurrenceCount, String> {
    taskboard_domain::OccurrenceCount::parse_text_form(raw).map_err(|err| err.to_string())
}

/// The `tasks repeat --until` bound: an inclusive **local** calendar
/// date (`YYYY-MM-DD`) — a pure date, deliberately without
/// `parse_due`'s UTC-midnight semantics (a bound is not an instant).
/// Anything else is a clap usage error (exit 2).
fn parse_until_flag(raw: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map_err(|_| "expected a local date YYYY-MM-DD".to_owned())
}

/// The `--weekly` flag's value shape: the bare flag (the empty
/// `default_missing_value`) names the task's own weekday — resolved
/// handler-side, the calendar does not exist at parse time — and an
/// explicit value carries the weekday set.
#[derive(Debug, Clone, PartialEq)]
pub enum WeekdaysFlag {
    /// Bare `--weekly`: the task's own weekday.
    TaskWeekday,
    /// `--weekly mon,wed,fri`.
    Days(taskboard_domain::WeekdaySet),
}

/// The `--monthly` flag's value shape: the bare flag names the task's
/// own day-of-month (handler-side resolution), an explicit value is a
/// [`taskboard_domain::MonthlyAnchor`] text form.
#[derive(Debug, Clone, PartialEq)]
pub enum MonthlyFlag {
    /// Bare `--monthly`: the task's own day-of-month.
    TaskDay,
    /// `--monthly 15` | `--monthly 2nd-mon`.
    Anchor(taskboard_domain::MonthlyAnchor),
}

/// The `tasks repeat` pattern/mode flags: exactly one of the four
/// (`--daily`/`--weekly`/`--monthly` set a pattern, `--off` clears the
/// rule) — clap-enforced by the `repeat-mode` arg group (`multiple =
/// false`, `required = true`).
#[derive(Debug, Clone, clap::Args)]
#[group(id = "repeat-mode", multiple = false, required = true)]
pub struct RepeatModeFlags {
    /// Every N days (bare flag = 1).
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "1",
        value_parser = parse_every_days_flag
    )]
    pub daily: Option<taskboard_domain::EveryDays>,
    /// On the given weekdays (`mon,wed,fri`) of every
    /// `--every-weeks`-th week; bare flag = the task's own weekday.
    /// A month never lacks a weekday — no monthly-style clamp question.
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "",
        value_parser = parse_weekdays_flag
    )]
    pub weekly: Option<WeekdaysFlag>,
    /// Monthly on a day-of-month (`15`, clamped in short months) or an
    /// nth weekday (`2nd-mon`; a month lacking it contributes no
    /// member — legal grammar, legal rule). Bare flag = the task's own
    /// day-of-month.
    #[arg(
        long,
        num_args(0..=1),
        default_missing_value = "",
        value_parser = parse_monthly_flag
    )]
    pub monthly: Option<MonthlyFlag>,
    /// Stop repeating (clear the rule; the head stays an ordinary
    /// task). Exclusive with every set flag and bound.
    #[arg(long, conflicts_with_all = ["every_weeks", "until", "count"])]
    pub off: bool,
}

/// The `tasks repeat` flags (the pattern family above, plus bounds) —
/// a flattened `Args` struct so the bounds can require the pattern by
/// the group id.
#[derive(Debug, Clone, clap::Args)]
pub struct RepeatArgs {
    /// The task (uuid | unique ≥8-char uuid prefix | exact title).
    pub task: String,
    /// Exactly one of `--daily`/`--weekly`/`--monthly`/`--off`.
    #[command(flatten)]
    pub mode: RepeatModeFlags,
    /// The weekly interval (≥ 1); requires `--weekly`.
    #[arg(
        long,
        requires = "weekly",
        conflicts_with_all = ["daily", "monthly"],
        value_parser = parse_every_weeks_flag
    )]
    pub every_weeks: Option<taskboard_domain::EveryWeeks>,
    /// No member after this inclusive local date; requires a pattern.
    #[arg(
        long,
        requires = "repeat-mode",
        conflicts_with_all = ["count"],
        value_parser = parse_until_flag
    )]
    pub until: Option<chrono::NaiveDate>,
    /// At most N completed occurrences (≥ 1); requires a pattern.
    #[arg(
        long,
        requires = "repeat-mode",
        conflicts_with_all = ["until"],
        value_parser = parse_count_flag
    )]
    pub count: Option<taskboard_domain::OccurrenceCount>,
}

/// The `timers add --at` flag (D-R9's host-local ruling, applied at the
/// CLI edge — this is the only place the app reads `chrono::Local`).
/// Accepted, in order: (1) full RFC-3339 with offset → absolute instant;
/// (2) `YYYY-MM-DD` → that date's midnight **host-local**, resolved to
/// UTC (deliberately diverging from `parse_due`'s UTC bare-date reading:
/// a wake-up alarm is a wall-clock promise); (3) `HH:MM[:SS]` → today
/// host-local at that time, rolled to **tomorrow** when already past (a
/// "09:30" set at 22:00 means tomorrow 09:30). DST-ambiguous local
/// times resolve to one offset (chrono's local resolution); the alarm
/// is absolute thereafter. Anything else is a clap usage error
/// (exit 2), the `parse_due` failure shape.
fn parse_at_flag(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    use chrono::TimeZone as _;
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(raw) {
        return Ok(dt.with_timezone(&chrono::Utc));
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        let midnight = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
        return chrono::Local
            .from_local_datetime(&midnight)
            .single()
            .map(|local| local.with_timezone(&chrono::Utc))
            .ok_or_else(|| "that local midnight does not resolve to one instant".to_owned());
    }
    let time = chrono::NaiveTime::parse_from_str(raw, "%H:%M")
        .or_else(|_| chrono::NaiveTime::parse_from_str(raw, "%H:%M:%S"));
    if let Ok(time) = time {
        let today = chrono::Local::now().date_naive();
        let mut local = chrono::Local
            .from_local_datetime(&today.and_time(time))
            .single()
            .ok_or_else(|| "that local time does not resolve to one instant".to_owned())?;
        if local <= chrono::Local::now() {
            let tomorrow = today.succ_opt().expect("tomorrow exists");
            local = chrono::Local
                .from_local_datetime(&tomorrow.and_time(time))
                .single()
                .ok_or_else(|| "that local time does not resolve to one instant".to_owned())?;
        }
        return Ok(local.with_timezone(&chrono::Utc));
    }
    Err("expected RFC-3339 (2026-10-24T09:00:00Z), YYYY-MM-DD, or HH:MM[:SS]".to_owned())
}

/// `timers` subcommands: the standalone alarm family (D-R9).
#[derive(Debug, Subcommand)]
pub enum TimersCmd {
    /// Create a standalone alarm: fires once at the stamped instant,
    /// then dies by settlement. Exactly one of `--in`/`--at`.
    Add {
        /// The notification label (no task to snapshot a title from).
        /// The clap id is `timer_label` — `label` is the task-label
        /// selector positional's id, and the completion walk types by
        /// id (a collision would offer task-label titles here).
        #[arg(value_name = "label")]
        timer_label: String,
        /// Fire this long after now (`30s|5m|1h30m|2d`); must be
        /// positive. Exactly one of `--in`/`--at`.
        #[arg(
            long,
            name = "in",
            required_unless_present = "at",
            conflicts_with = "at",
            value_parser = parse_before_flag
        )]
        in_: Option<chrono::TimeDelta>,
        /// Fire at this instant (RFC-3339 | YYYY-MM-DD | HH:MM[:SS]
        /// host-local; a bare time already past means tomorrow).
        /// Exactly one of `--in`/`--at`.
        #[arg(
            long,
            required_unless_present = "in",
            conflicts_with = "in",
            value_parser = parse_at_flag
        )]
        at: Option<chrono::DateTime<chrono::Utc>>,
        /// How loudly the alarm insists (`normal|alarm`).
        #[arg(long, value_parser = parse_insistence_flag)]
        insistence: Option<taskboard_domain::Insistence>,
    },
    /// List the standalone alarms in canonical `(fire_at, id)` order
    /// (pure read).
    List,
    /// Cancel a standalone alarm (uuid | unique ≥8-char uuid prefix |
    /// unique case-folded label prefix). Cancel deletes the row, so a
    /// repeat is a typed unknown-alarm refusal.
    Cancel {
        /// The alarm selector.
        selector: String,
    },
}

/// The export payload format. `Csv` is the default; a malformed value
/// is a clap usage error (exit 2) — the `FindType` `ValueEnum`
/// precedent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum ExportFormat {
    /// RFC-4180 CSV with a header row (the default).
    #[default]
    Csv,
    /// A single-line JSON array of entry objects (`jq` is the
    /// consumer).
    Json,
}

/// Span-stamp grammar (the `parse_due` shape minus `none`): RFC-3339
/// with an explicit offset (normalized to UTC) or bare `YYYY-MM-DD`
/// (that date's midnight UTC). No `none` — a span boundary is never
/// "clear" (there is no payload that reopens an entry, ADR 0017) — and
/// no bare `HH:MM` (D-TC7: a time-only boundary is day-ambiguous; its
/// wake-up-rollover reading is exactly wrong for retroactive history).
fn parse_span_stamp(raw: &str) -> Result<chrono::DateTime<chrono::Utc>, String> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|_| {
            chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map(|date| {
                date.and_hms_opt(0, 0, 0)
                    .expect("midnight is valid")
                    .and_utc()
            })
        })
        .map_err(|_| {
            "expected RFC-3339 with an offset (2026-10-24T09:00:00Z) or YYYY-MM-DD".to_owned()
        })
}

/// Strict `YYYY-MM-DD` — the UTC-day filter grammar of `track list
/// --since` and `export --since/--until`. A malformed value is a clap
/// usage error (exit 2).
fn parse_date_flag(raw: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").map_err(|_| "expected YYYY-MM-DD".to_owned())
}

/// A positive quota (`--daily/--weekly/--monthly [N]`): zero-quota is
/// unrepresentable in the domain, so the parse rejects it as a usage
/// error (the mint's law surfaced at the CLI boundary).
fn parse_quota_flag(raw: &str) -> Result<u32, String> {
    let n: u32 = raw.parse().map_err(|_| "expected a count".to_owned())?;
    if n == 0 {
        return Err("the count must be at least 1".to_owned());
    }
    Ok(n)
}

/// `HH:MM[:SS]` — the anchor-chip grammar.
fn parse_time_of_day_flag(raw: &str) -> Result<chrono::NaiveTime, String> {
    chrono::NaiveTime::parse_from_str(raw, "%H:%M:%S")
        .or_else(|_| chrono::NaiveTime::parse_from_str(raw, "%H:%M"))
        .map_err(|_| "expected HH:MM or HH:MM:SS".to_owned())
}

/// `--at <HH:MM[:SS]>` / `--at none` — the chip's change-mask value.
fn parse_anchor_edit(raw: &str) -> Result<AnchorEdit, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(AnchorEdit::Clear);
    }
    parse_time_of_day_flag(raw).map(AnchorEdit::Set)
}

/// `--continued-from <series uuid>` / `--continued-from none`.
fn parse_seam_edit(raw: &str) -> Result<SeamEdit, String> {
    if raw.eq_ignore_ascii_case("none") {
        return Ok(SeamEdit::Unlink);
    }
    let uuid =
        uuid::Uuid::parse_str(raw).map_err(|_| "expected a series uuid or 'none'".to_owned())?;
    Ok(SeamEdit::Link(taskboard_domain::SeriesId::from(uuid)))
}

#[test]
fn parse_span_stamp_normalizes_to_utc_and_rejects_bare_times() {
    let stamped = parse_span_stamp("2026-10-24T11:00:00+02:00").expect("explicit offset");
    let expected = chrono::DateTime::parse_from_rfc3339("2026-10-24T09:00:00Z")
        .expect("the same instant in UTC")
        .with_timezone(&chrono::Utc);
    assert_eq!(stamped, expected, "the offset normalizes to UTC");
    let bare = parse_span_stamp("2026-10-24").expect("bare date");
    let midnight = chrono::NaiveDate::from_ymd_opt(2026, 10, 24)
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .expect("valid midnight")
        .and_utc();
    assert_eq!(bare, midnight, "a bare date is that midnight UTC");
    // No `none`, no bare time-of-day: both are usage errors.
    for bad in ["none", "09:30", "tomorrow", "", "2026-10-24T09:00:00"] {
        assert!(parse_span_stamp(bad).is_err(), "{bad} must be rejected");
    }
}

#[test]
fn parse_date_flag_is_strict_ymd() {
    let date = parse_date_flag("2026-10-24").expect("ymd");
    assert_eq!(chrono::NaiveDate::from_ymd_opt(2026, 10, 24), Some(date));
    for bad in ["none", "2026-10-24T09:00:00Z", "24.10.2026", ""] {
        assert!(parse_date_flag(bad).is_err(), "{bad} must be rejected");
    }
}

/// `track` subcommands: the time-tracking family (D-T14).
#[derive(Debug, Subcommand)]
pub enum TrackCmd {
    /// Start tracking a live task: whatever runs closes at the
    /// engine-stamped now, a fresh open entry is minted (title
    /// snapshotted). Idempotent when this task is already tracked.
    Start {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
    },
    /// Close every open entry at the engine-stamped now. Nothing open
    /// is the typed no-op.
    Stop,
    /// The open entries (newest-first) plus the tracked-today total
    /// over the `[preferences] day_start_time` window — spanning
    /// entries contribute only their in-window part (the pure
    /// projection; rows are never physically split).
    Status,
    /// Time entries in canonical `(start, id)` order.
    List {
        /// Only entries of this live task.
        #[arg(long)]
        task: Option<String>,
        /// Only entries starting on/after this UTC date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        since: Option<chrono::NaiveDate>,
    },
    /// Log a retroactive, closed block: `--for <dur>` (ending now) or
    /// `--from/--to` absolute stamps — exactly one mode. An inverted
    /// or empty span is the typed `invalid-span` failure (the
    /// planner's law, not clap's).
    Log {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Length of the block ending now (`30s|5m|1h30m|2d`); must be
        /// positive. Exactly one of `--for`/`--from --to`.
        #[arg(
            long,
            name = "for",
            required_unless_present_all = ["from", "to"],
            conflicts_with_all = ["from", "to"],
            value_parser = parse_before_flag
        )]
        for_: Option<chrono::TimeDelta>,
        /// Span start (RFC-3339 with an explicit offset, or YYYY-MM-DD
        /// = midnight UTC). Requires `--to`.
        #[arg(long, requires = "to", value_parser = parse_span_stamp)]
        from: Option<chrono::DateTime<chrono::Utc>>,
        /// Span end, strictly after `--from`. Requires `--from`.
        #[arg(long, requires = "from", value_parser = parse_span_stamp)]
        to: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// Adjust an entry's span. Each flag is a *change mask*: omitted =
    /// unchanged; there is deliberately no way to clear an end (a
    /// history edit can never resurrect a running timer, ADR 0017).
    /// A zero-flag adjust is the typed no-op.
    Adjust {
        /// The entry (uuid | unique ≥8-char uuid prefix).
        entry: String,
        /// New span start.
        #[arg(long, value_parser = parse_span_stamp)]
        start: Option<chrono::DateTime<chrono::Utc>>,
        /// New span end (stamping one on an open entry closes it).
        #[arg(long, value_parser = parse_span_stamp)]
        end: Option<chrono::DateTime<chrono::Utc>>,
    },
    /// Delete one entry — explicit id, no confirmation. Repeats are
    /// the typed no-op (an unknown id is *not* terminal, unlike
    /// `timers cancel`: a synced tombstone may have folded the row
    /// already).
    Delete {
        /// The entry (uuid | unique ≥8-char uuid prefix).
        entry: String,
    },
}

/// `habits` subcommands (D-12's six verbs). The target grammar
/// `--daily/--weekly/--monthly [N]` mirrors the `tasks repeat` family:
/// exactly one flag, the count defaulting to 1 (zero-quota is
/// unrepresentable — the [`taskboard_domain::NonZeroCount`] mint).
#[derive(Debug, Subcommand)]
pub enum HabitsCmd {
    /// Add a tracker to a board (appended last in manual order).
    Add {
        /// The display title.
        title: String,
        /// Count per day window (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            required_unless_present_any = ["weekly", "monthly"],
            conflicts_with_all = ["weekly", "monthly"],
            value_parser = parse_quota_flag
        )]
        daily: Option<u32>,
        /// Count per ISO week (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["monthly"],
            value_parser = parse_quota_flag
        )]
        weekly: Option<u32>,
        /// Count per calendar month (default 1).
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            value_parser = parse_quota_flag
        )]
        monthly: Option<u32>,
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The display anchor chip (`HH:MM[:SS]`) — presentation only,
        /// never an ordering input.
        #[arg(long, value_parser = parse_time_of_day_flag)]
        at: Option<chrono::NaiveTime>,
    },
    /// The board's trackers in stored manual `(order, id)` order, with
    /// current-window progress and streak (the conversion seam unions a
    /// linked series' completed members).
    List {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// +1 into today's window (surplus logs are first-class). The
    /// receipt names the window the increment landed in and the
    /// post-increment count — a surplus repair is user-visible, never
    /// silent.
    Log {
        /// The habit (uuid | unique ≥8-char uuid prefix | exact live
        /// title).
        habit: String,
    },
    /// Explicit count correction for one window (`YYYY-MM-DD`, the
    /// canonical period start of the habit's current target kind). A
    /// non-canonical window is the typed `invalid-habit-window`
    /// refusal, never a clap refusal.
    Adjust {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
        /// The window's period-start date (YYYY-MM-DD).
        #[arg(long, value_parser = parse_date_flag)]
        window: chrono::NaiveDate,
        /// The corrected count (0 is legal — an audited emptying).
        #[arg(long)]
        count: u32,
    },
    /// Edit a tracker's fields. Each flag is a change mask: omitted =
    /// unchanged; a zero-flag update is the typed no-op. `--order`
    /// routes `MoveHabit` (the manual reorder — the UI computes
    /// midpoints, this verb stores the absolute value).
    Update {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New per-day quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["weekly", "monthly"],
            value_parser = parse_quota_flag
        )]
        daily: Option<u32>,
        /// New per-week quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            conflicts_with_all = ["monthly"],
            value_parser = parse_quota_flag
        )]
        weekly: Option<u32>,
        /// New per-month quota.
        #[arg(
            long,
            value_name = "N",
            num_args = 0..=1,
            default_missing_value = "1",
            value_parser = parse_quota_flag
        )]
        monthly: Option<u32>,
        /// New anchor chip (`HH:MM[:SS]`), or `none` to clear it.
        #[arg(long, value_parser = parse_anchor_edit)]
        at: Option<AnchorEdit>,
        /// Absolute manual-order slot (the `MoveHabit` semantics).
        #[arg(long)]
        order: Option<i64>,
        /// Link the §4.7 conversion seam (series uuid), or `none` to
        /// unlink. The read-side streak/history views union the link.
        #[arg(long, value_parser = parse_seam_edit)]
        continued_from: Option<SeamEdit>,
    },
    /// Delete a tracker (tombstone; the log outlives it — history stays
    /// queryable, and the receipt names the surviving row count).
    Rm {
        /// The habit (uuid | prefix | exact live title).
        habit: String,
    },
}

/// The `habits update --at` value: a time-of-day, or `none` (clear).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorEdit {
    /// Set the chip.
    Set(chrono::NaiveTime),
    /// Clear the chip.
    Clear,
}

/// The `habits update --continued-from` value: a series uuid, or
/// `none` (unlink).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeamEdit {
    /// Link the series.
    Link(taskboard_domain::SeriesId),
    /// Unlink.
    Unlink,
}

/// `config` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print the effective config (secret-free by construction).
    Print,
    /// Print the resolved config read path and the connect/login write
    /// target — a side-effect-free resolution diagnostic: no decode,
    /// no core boot.
    Path,
}

/// `labels` subcommands.
#[derive(Debug, Subcommand)]
pub enum LabelsCmd {
    /// List a board's labels (with live-task assignment counts).
    List {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
    },
    /// Create a label (color defaults to the Deck default when omitted).
    Add {
        /// Target board (uuid); the single live board when omitted.
        #[arg(long, value_parser = parse_board_id)]
        board: Option<taskboard_domain::BoardId>,
        /// The label title.
        title: String,
        /// Six hex digits (no `#`); a default applies when omitted.
        #[arg(long, value_parser = parse_color)]
        color: Option<String>,
    },
    /// Edit a label (at least one flag required).
    Edit {
        /// The label (uuid | exact title).
        label: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New color (six hex digits).
        #[arg(long, value_parser = parse_color)]
        color: Option<String>,
    },
    /// Delete a label (detaches it from all tasks; idempotent).
    Delete {
        /// The label (uuid | exact title).
        label: String,
    },
}

/// `tasks` subcommands.
#[derive(Debug, Subcommand)]
pub enum TasksCmd {
    /// List the bound board's stacks and tasks.
    List {
        /// Restrict the listing to one stack (exact uuid or title).
        #[arg(long)]
        stack: Option<String>,
    },
    /// Create a task (appended to the end of the target stack).
    Add {
        /// The task title.
        title: String,
        /// Target stack (exact uuid or title); required to be unambiguous
        /// when omitted.
        #[arg(long)]
        stack: Option<String>,
    },
    /// Complete (or with `--undo`, reopen) a task.
    Done {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Reopen the task instead of completing it.
        #[arg(long)]
        undo: bool,
    },
    /// Move a task to a stack and/or position (one composite intent).
    Move {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// Target stack (exact uuid or title); required to be unambiguous
        /// when omitted. Collisions across boards resolve through
        /// `--board`.
        #[arg(long)]
        stack: Option<String>,
        /// Scope the `--stack` selector to this board (exact uuid |
        /// exact title among the live boards) when stack titles
        /// collide across boards.
        #[arg(long)]
        board: Option<String>,
        /// Explicit sort order; default appends to the end of the target
        /// stack.
        #[arg(long)]
        order: Option<i64>,
    },
    /// Edit task fields (a zero-flag edit is a typed no-op).
    Edit {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// RFC-3339 with an explicit offset (normalized to UTC),
        /// YYYY-MM-DD (midnight UTC), or `none` (clears). Due dates
        /// are stored and displayed in UTC only; offset-less
        /// timestamps are refused.
        #[arg(long, value_parser = parse_due)]
        due: Option<DueSpec>,
        /// Scheduled start — the `--due` vocabulary (RFC-3339,
        /// YYYY-MM-DD, or `none` clears). One parser, two flags.
        #[arg(long, value_parser = parse_due)]
        scheduled: Option<DueSpec>,
        /// Effort estimate — bare digits = minutes, `s`/`m`/`h`
        /// components compose (`1h30m`), `none` clears.
        #[arg(long, value_parser = parse_effort)]
        effort: Option<EffortSpec>,
        /// Importance level (`very-low|low|normal|high|very-high`;
        /// `none` clears).
        #[arg(long, value_parser = parse_importance)]
        importance: Option<ImportanceSpec>,
        /// Urgency level (`very-low|low|normal|high|very-high`; `none`
        /// clears).
        #[arg(long, value_parser = parse_urgency)]
        urgency: Option<UrgencySpec>,
        /// Archive the task.
        #[arg(long, conflicts_with = "unarchive")]
        archive: bool,
        /// Unarchive the task.
        #[arg(long)]
        unarchive: bool,
    },
    /// Delete (tombstone) a task; no confirmation (scripting-first —
    /// repeats are idempotent, the tombstone is permanent).
    Delete {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
    },
    /// Attach a label to a task (idempotent when already attached).
    Label {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// The label (uuid | exact title).
        label: String,
    },
    /// Detach a label from a task (idempotent when absent).
    Unlabel {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// The label (uuid | exact title).
        label: String,
    },
    /// Manage a task's reminder rules: add one with `--before`, remove
    /// one with `--remove`, remove all with `--clear`, enumerate with
    /// `--list`, mute/unmute the task's firings with `--off`/`--on`.
    /// Exactly one mode per line.
    #[group(required = true, multiple = true)]
    Remind {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: Option<String>,
        /// The new rule's offset before due (`30s|5m|1h30m|2d`); the
        /// `[reminders] default_before` config default applies when
        /// omitted. A rule at this offset is unique (repeats are no-ops).
        /// List rendering glyphs: `!` marks overdue and `~` due-soon in
        /// `tasks list`.
        #[arg(long, requires = "task", value_parser = parse_before_flag)]
        before: Option<chrono::TimeDelta>,
        /// How loudly the new rule insists (`normal|alarm`; the default
        /// reminder stays normal). Composes with the add mode only.
        #[arg(
            long,
            requires = "task",
            value_parser = parse_insistence_flag,
            conflicts_with_all = ["clear", "remove", "off", "on", "list"]
        )]
        insistence: Option<taskboard_domain::Insistence>,
        /// List the board's reminder rules (pure read).
        #[arg(long, conflicts_with_all = ["task", "clear", "off", "on"])]
        list: bool,
        /// Remove all of the task's reminder rules.
        #[arg(long, requires = "task", conflicts_with_all = ["before", "off", "on", "list"])]
        clear: bool,
        /// Remove one of the task's rules (rule uuid | unique ≥8-char
        /// uuid prefix | the offset text form, e.g. 30m).
        #[arg(
            long,
            requires = "task",
            conflicts_with_all = ["before", "clear", "off", "on", "list"]
        )]
        remove: Option<String>,
        /// Mute all firings of the task (the overdue banner is
        /// unaffected).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "on", "list"])]
        off: bool,
        /// Unmute the task's firings (inverse of `--off`).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "off", "list"])]
        on: bool,
    },
    /// Repeat a task on a schedule (`--daily`/`--weekly`/`--monthly`
    /// with optional `--until`/`--count` bounds) or stop repeating
    /// (`--off`). Exactly one mode per line; the anchor is the task's
    /// scheduled/duedate local date, else today.
    Repeat(RepeatArgs),
    /// Undo the last change (the journal line head, ADR 0019). Refuses
    /// typed when the line is empty, the head is not undoable, or a
    /// later write diverged. Zero flags: the line has no selectors.
    Undo,
    /// Redo the last undone change (the head's newest child, ADR 0019).
    /// Zero flags: the line has no selectors.
    Redo,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;

    #[test]
    fn parse_due_accepts_rfc3339_bare_date_and_none() {
        let set = parse_due("2026-10-24T09:00:00Z").expect("rfc3339");
        assert!(matches!(set, DueSpec::Set(_)));
        let bare = parse_due("2026-10-24").expect("bare date");
        match bare {
            DueSpec::Set(dt) => {
                let midnight = chrono::NaiveDate::from_ymd_opt(2026, 10, 24)
                    .and_then(|d| d.and_hms_opt(0, 0, 0))
                    .expect("valid midnight")
                    .and_utc();
                assert_eq!(dt, midnight, "bare date is midnight UTC");
            }
            DueSpec::Clear => panic!("bare date must set"),
        }
        assert_eq!(parse_due("none"), Ok(DueSpec::Clear));
    }

    #[test]
    fn parse_insistence_flag_matches_the_domain_vocabulary() {
        assert_eq!(
            parse_insistence_flag("normal"),
            Ok(taskboard_domain::Insistence::Normal)
        );
        assert_eq!(
            parse_insistence_flag("alarm"),
            Ok(taskboard_domain::Insistence::Alarm)
        );
        for raw in ["critical", "wake_up", "", "ALARM"] {
            assert!(
                parse_insistence_flag(raw).is_err(),
                "{raw} must be a usage error"
            );
        }
    }

    #[test]
    fn the_insistence_flags_parse_and_respect_the_coherence_sets() {
        // Remind: the flag composes with the add mode only.
        let Cli {
            command:
                Some(Command::Tasks {
                    cmd: TasksCmd::Remind { insistence, .. },
                }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "remind",
            "tea",
            "--before",
            "5m",
            "--insistence",
            "alarm",
        ])
        .expect("the add-mode flag parses")
        else {
            panic!("expected remind");
        };
        assert_eq!(insistence, Some(taskboard_domain::Insistence::Alarm));
        let parsed = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "remind",
            "tea",
            "--clear",
            "--insistence",
            "alarm",
        ]);
        assert!(parsed.is_err(), "--insistence conflicts with --clear");
        // Timers add: no conflicts.
        let Cli {
            command:
                Some(Command::Timers {
                    cmd: TimersCmd::Add { insistence, .. },
                }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "timers",
            "add",
            "tea",
            "--in",
            "2m",
            "--insistence",
            "alarm",
        ])
        .expect("the timers flag parses")
        else {
            panic!("expected timers add");
        };
        assert_eq!(insistence, Some(taskboard_domain::Insistence::Alarm));
    }

    #[test]
    fn parse_due_rejects_garbage_as_a_usage_error() {
        for raw in ["tomorrow", "2026-13-40", ""] {
            assert!(parse_due(raw).is_err(), "{raw} must be rejected");
        }
    }

    #[test]
    fn find_flags_parse_typedly_and_refuse_garbage() {
        // Well-formed combinations parse.
        let Cli {
            command: Some(Command::Find { query, kind, limit }),
            ..
        } = Cli::try_parse_from([
            "taskboard",
            "find",
            "needle",
            "--type",
            "board",
            "--limit",
            "3",
        ])
        .expect("find with flags parses")
        else {
            panic!("expected find");
        };
        assert_eq!(query, "needle");
        assert_eq!(kind, Some(FindType::Board));
        assert_eq!(limit, Some(3));

        // Defaults: no kind filter, no cap.
        let Cli {
            command: Some(Command::Find { query, kind, limit }),
            ..
        } = Cli::try_parse_from(["taskboard", "find", "needle"]).expect("bare find parses")
        else {
            panic!("expected find");
        };
        assert_eq!(query, "needle");
        assert_eq!(kind, None);
        assert_eq!(limit, None);

        // An unknown kind is a clap usage error, never a runtime compare.
        assert!(Cli::try_parse_from(["taskboard", "find", "needle", "--type", "bogus"]).is_err());
        // A zero limit is a usage error — "print nothing" is not a
        // meaningful invocation.
        assert!(Cli::try_parse_from(["taskboard", "find", "needle", "--limit", "0"]).is_err());
    }

    #[test]
    fn parse_before_flag_reuses_the_config_grammar() {
        assert_eq!(
            parse_before_flag("10m"),
            Ok(chrono::TimeDelta::try_minutes(10).expect("10m"))
        );
        assert!(parse_before_flag("5min").is_err());
    }

    #[test]
    fn parse_effort_none_clears_and_vocabulary_parses() {
        assert_eq!(parse_effort("none"), Ok(EffortSpec::Clear));
        assert_eq!(
            parse_effort("1h30m"),
            Ok(EffortSpec::Set(taskboard_domain::Effort::new(5400)))
        );
        assert_eq!(
            parse_effort("30"),
            Ok(EffortSpec::Set(taskboard_domain::Effort::new(30 * 60)))
        );
        assert!(
            parse_effort("1h30").is_err(),
            "bad grammar is a usage error"
        );
        assert!(parse_effort("").is_err());
    }

    #[test]
    fn parse_level_flags_none_clears_and_vocabulary_parses() {
        assert_eq!(parse_importance("none"), Ok(ImportanceSpec::Clear));
        assert_eq!(parse_urgency("none"), Ok(UrgencySpec::Clear));
        assert_eq!(
            parse_importance("very-high"),
            Ok(ImportanceSpec::Set(taskboard_domain::Importance::VeryHigh))
        );
        assert_eq!(
            parse_urgency("low"),
            Ok(UrgencySpec::Set(taskboard_domain::Urgency::Low))
        );
        for bad in ["medium", "very_low", ""] {
            assert!(parse_importance(bad).is_err(), "{bad} must be rejected");
            assert!(parse_urgency(bad).is_err(), "{bad} must be rejected");
        }
    }

    #[test]
    fn the_field_flags_land_on_the_edit_arm() {
        let cli = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--scheduled",
            "2026-10-24",
            "--effort",
            "1h30m",
            "--importance",
            "high",
            "--urgency",
            "very-low",
        ])
        .expect("field flags parse");
        let Some(Command::Tasks {
            cmd:
                TasksCmd::Edit {
                    scheduled,
                    effort,
                    importance,
                    urgency,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert!(matches!(scheduled, Some(DueSpec::Set(_))));
        assert_eq!(
            effort,
            Some(EffortSpec::Set(taskboard_domain::Effort::new(5400)))
        );
        assert_eq!(
            importance,
            Some(ImportanceSpec::Set(taskboard_domain::Importance::High))
        );
        assert_eq!(
            urgency,
            Some(UrgencySpec::Set(taskboard_domain::Urgency::VeryLow))
        );
    }

    #[test]
    fn the_field_flags_clear_on_none_and_error_on_garbage() {
        let cli = Cli::try_parse_from([
            "taskboard",
            "tasks",
            "edit",
            "draft",
            "--scheduled",
            "none",
            "--effort",
            "none",
            "--importance",
            "none",
            "--urgency",
            "none",
        ])
        .expect("none clears each field flag");
        let Some(Command::Tasks {
            cmd:
                TasksCmd::Edit {
                    scheduled,
                    effort,
                    importance,
                    urgency,
                    ..
                },
        }) = cli.command
        else {
            panic!("expected tasks edit");
        };
        assert_eq!(scheduled, Some(DueSpec::Clear));
        assert_eq!(effort, Some(EffortSpec::Clear));
        assert_eq!(importance, Some(ImportanceSpec::Clear));
        assert_eq!(urgency, Some(UrgencySpec::Clear));

        for bad in [
            ["--effort", "90min"],
            ["--importance", "medium"],
            ["--urgency", "very_high"],
            ["--scheduled", "tomorrow"],
        ] {
            let mut argv = vec!["taskboard", "tasks", "edit", "draft"];
            argv.extend(bad);
            assert!(
                Cli::try_parse_from(&argv).is_err(),
                "{bad:?} must be a clap usage error"
            );
        }
    }

    #[test]
    fn connector_flags_parse_on_the_targeting_verbs() {
        // `--connector` lands in the arm on boards list/select/publish
        // and sync (D6-5).
        let cli = Cli::try_parse_from(["taskboard", "boards", "list", "--connector", "beta"])
            .expect("boards list --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::List { connector },
        }) = cli.command
        else {
            panic!("expected boards list");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from([
            "taskboard",
            "boards",
            "select",
            "42",
            "--connector",
            "beta",
            "--bind-local-board",
        ])
        .expect("boards select --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Select {
                board, connector, ..
            },
        }) = cli.command
        else {
            panic!("expected boards select");
        };
        assert_eq!(board, 42);
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from(["taskboard", "boards", "publish", "--connector", "beta"])
            .expect("boards publish --connector parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Publish { connector, .. },
        }) = cli.command
        else {
            panic!("expected boards publish");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        let cli = Cli::try_parse_from(["taskboard", "sync", "--connector", "beta"])
            .expect("sync --connector parses");
        let Some(Command::Sync { connector, .. }) = cli.command else {
            panic!("expected sync");
        };
        assert_eq!(connector.as_deref(), Some("beta"));

        // Omission is `None` — the default (first) connector.
        let cli = Cli::try_parse_from(["taskboard", "boards", "list"]).expect("bare list");
        let Some(Command::Boards {
            cmd: BoardsCmd::List { connector },
        }) = cli.command
        else {
            panic!("expected boards list");
        };
        assert_eq!(connector, None);
    }

    #[test]
    fn boards_use_parses_with_and_without_a_uuid() {
        let good = "00000000-0000-0000-0000-000000000001";
        let cli = Cli::try_parse_from(["taskboard", "boards", "use", good]).expect("use parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Use { board },
        }) = cli.command
        else {
            panic!("expected boards use");
        };
        assert_eq!(
            board,
            Some(taskboard_domain::BoardId::from_uuid(
                uuid::Uuid::parse_str(good).unwrap()
            ))
        );
        let cli = Cli::try_parse_from(["taskboard", "boards", "use"]).expect("query parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Use { board },
        }) = cli.command
        else {
            panic!("expected boards use");
        };
        assert_eq!(board, None);
        // Malformed uuids are usage errors, like --board.
        assert!(Cli::try_parse_from(["taskboard", "boards", "use", "nope"]).is_err());
    }

    #[test]
    fn the_select_positional_id_rename_is_invisible_to_the_syntax() {
        // The internal clap id is `remote_board` (completion routes it
        // as server state); the displayed value name stays `board`.
        let cli = Cli::try_parse_from(["taskboard", "boards", "select", "42"])
            .expect("the positional still parses");
        let Some(Command::Boards {
            cmd: BoardsCmd::Select { board, .. },
        }) = cli.command
        else {
            panic!("expected boards select");
        };
        assert_eq!(board, 42);
    }

    #[test]
    fn gui_parses_as_a_first_class_subcommand_with_no_arguments() {
        let cli = Cli::try_parse_from(["taskboard", "gui"]).expect("gui parses");
        assert!(matches!(cli.command, Some(Command::Gui)));

        // Global flags compose with the verb in both orders.
        let cli = Cli::try_parse_from(["taskboard", "--json", "gui"]).expect("--json gui");
        assert!(cli.json);
        assert!(matches!(cli.command, Some(Command::Gui)));
        let config: &str = "some.toml";
        let cli =
            Cli::try_parse_from(["taskboard", "--config", config, "gui"]).expect("--config gui");
        assert_eq!(cli.config.as_deref(), Some(std::path::Path::new(config)));
        assert!(matches!(cli.command, Some(Command::Gui)));

        // The verb takes no arguments of its own.
        assert!(Cli::try_parse_from(["taskboard", "gui", "extra"]).is_err());
    }
}
