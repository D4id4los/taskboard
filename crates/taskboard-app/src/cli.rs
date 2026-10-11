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
    /// Verify and store Nextcloud credentials (first-run command).
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
    /// Run the headless sync daemon (M1).
    Daemon,
}

/// `boards` subcommands.
#[derive(Debug, Subcommand)]
pub enum BoardsCmd {
    /// List the server's boards (remote ids feed `boards select`).
    List,
    /// Bind this installation to one board (refuses a rebind; migrates a
    /// local board with `--bind-local-board`).
    Select {
        /// The remote Deck board id (from `boards list`).
        board: u64,
        /// Adoption wait budget, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
        /// Bind the existing local board onto the remote board
        /// (migration path for stand-alone data).
        #[arg(long)]
        bind_local_board: bool,
    },
    /// Create the local board (stand-alone first-run command; refuses
    /// when a board already exists).
    Create {
        /// The board title.
        title: String,
        /// Board color as six hex digits (no `#`); a default applies
        /// when omitted.
        #[arg(long)]
        color: Option<String>,
    },
    /// Rename the local board's title (both backends; the rename pushes
    /// when the board is bound, and rides the next `publish` otherwise).
    Rename {
        /// The new board title.
        title: String,
    },
    /// Publish the local board as a new Nextcloud board and bind it
    /// (the migration path for stand-alone data; sync-only).
    Publish {
        /// Wait budget for the settling first sync cycle, in seconds
        /// (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
}

/// `stacks` subcommands. The stack selector is the uuid or the exact
/// live title (prefixes are a task-only affordance).
#[derive(Debug, Subcommand)]
pub enum StacksCmd {
    /// Create a stack on the board (appended by default).
    Add {
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

/// Six lowercase hex digits or a usage error — labels only (an invalid
/// label color is a permanent push rejection; board colors stay lenient).
fn parse_color(raw: &str) -> Result<String, String> {
    let lower = raw.to_lowercase();
    if lower.len() == 6 && lower.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(lower);
    }
    Err("expected exactly six hex digits (e.g. 31cc7c, no '#')".to_owned())
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

/// `config` subcommands.
#[derive(Debug, Subcommand)]
pub enum ConfigCmd {
    /// Print the effective config (secret-free by construction).
    Print,
}

/// `labels` subcommands.
#[derive(Debug, Subcommand)]
pub enum LabelsCmd {
    /// List the board's labels (with live-task assignment counts).
    List,
    /// Create a label (color defaults to the Deck default when omitted).
    Add {
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
}
