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
    /// Delete the stored Nextcloud credentials (and, under the auto
    /// backend, the config's server/user keys). Idempotent; the board
    /// binding and local db are untouched.
    Logout,
    /// Run one full sync cycle now and report the status.
    Sync {
        /// Return immediately after nudging the sync actor.
        #[arg(long)]
        no_wait: bool,
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
    List,
    /// Bind this installation to one board (refuses a rebind; migrates a
    /// local board with `--bind-local-board`; pins the local board with
    /// `--board` when several are live).
    Select {
        /// The remote Deck board id (from `boards list`).
        board: u64,
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
        /// Wait budget for the settling first sync cycle, in seconds
        /// (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
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

/// `--due` grammar: set (RFC-3339 or bare `YYYY-MM-DD` → midnight UTC)
/// or clear (`none`).
#[derive(Debug, Clone, PartialEq)]
pub enum DueSpec {
    /// Set the due date.
    Set(chrono::DateTime<chrono::Utc>),
    /// Clear the due date.
    Clear,
}

/// Parses the `--due` value: RFC-3339, a bare date (midnight UTC), or
/// the literal `none`. A malformed value is a clap usage error (exit 2).
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

/// The `tasks remind --before` flag: the same `[reminders]
/// default_before` grammar ([`crate::config::parse_before`]). A
/// malformed value is a clap usage error (exit 2), the `parse_due`
/// precedent.
fn parse_before_flag(raw: &str) -> Result<chrono::TimeDelta, String> {
    crate::config::parse_before(raw)
        .ok_or_else(|| "expected <int><unit> components (s|m|h|d), e.g. 10m or 1h30m".to_owned())
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
        /// when omitted.
        #[arg(long)]
        stack: Option<String>,
        /// Explicit sort order; default appends to the end of the target
        /// stack.
        #[arg(long)]
        order: Option<i64>,
    },
    /// Edit task fields (at least one flag required).
    Edit {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: String,
        /// New title.
        #[arg(long)]
        title: Option<String>,
        /// New description.
        #[arg(long)]
        description: Option<String>,
        /// RFC-3339, YYYY-MM-DD (midnight UTC), or `none` (clears).
        #[arg(long, value_parser = parse_due)]
        due: Option<DueSpec>,
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
    /// Manage a task's reminder rules: set/replace with `--before`,
    /// enumerate with `--list`, remove with `--clear`, mute/unmute the
    /// task's firings with `--off`/`--on`. Exactly one mode per line.
    #[group(required = true, multiple = true)]
    Remind {
        /// The task (uuid | unique ≥8-char uuid prefix | exact title).
        task: Option<String>,
        /// The new rule's offset before due (`30s|5m|1h30m|2d`); the
        /// `[reminders] default_before` config default applies when
        /// omitted. List rendering glyphs: `!` marks overdue and `~`
        /// due-soon in `tasks list`.
        #[arg(long, requires = "task", value_parser = parse_before_flag)]
        before: Option<chrono::TimeDelta>,
        /// List the board's reminder rules (pure read).
        #[arg(long, conflicts_with_all = ["task", "clear", "off", "on"])]
        list: bool,
        /// Remove all of the task's reminder rules.
        #[arg(long, requires = "task", conflicts_with_all = ["before", "off", "on", "list"])]
        clear: bool,
        /// Mute all firings of the task (the overdue banner is
        /// unaffected).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "on", "list"])]
        off: bool,
        /// Unmute the task's firings (inverse of `--off`).
        #[arg(long, requires = "task", conflicts_with_all = ["before", "clear", "off", "list"])]
        on: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn parse_due_rejects_garbage_as_a_usage_error() {
        for raw in ["tomorrow", "2026-13-40", ""] {
            assert!(parse_due(raw).is_err(), "{raw} must be rejected");
        }
    }

    #[test]
    fn parse_before_flag_reuses_the_config_grammar() {
        assert_eq!(
            parse_before_flag("10m"),
            Ok(chrono::TimeDelta::try_minutes(10).expect("10m"))
        );
        assert!(parse_before_flag("5min").is_err());
    }
}
