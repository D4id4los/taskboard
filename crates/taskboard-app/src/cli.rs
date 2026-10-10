// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI surface (ADRs 0008/0009): clap types only — dispatch lives in
//! `commands::dispatch` (the lib); `main` is a thin process shell.

use std::path::PathBuf;

use clap::Subcommand;

/// The `taskboard` command line.
#[derive(Debug, clap::Parser)]
#[command(name = "taskboard", version = crate::VERSION, about = "Taskboard: offline-first task sync for desktop and kiosk")]
pub struct Cli {
    /// Path to the config file (default: ./taskboard.toml if present;
    /// `TASKBOARD_CONFIG` as the environment fallback).
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
    /// Board discovery and the one-time single-board binding.
    Boards {
        #[command(subcommand)]
        cmd: BoardsCmd,
    },
    /// Task verbs over the in-process core.
    Tasks {
        #[command(subcommand)]
        cmd: TasksCmd,
    },
    /// Run one full sync cycle now and report the status.
    Sync {
        /// Return immediately after nudging the sync actor.
        #[arg(long)]
        no_wait: bool,
        /// Wait budget for the cycle, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Run the headless sync daemon (M1).
    Daemon,
}

/// `boards` subcommands.
#[derive(Debug, Subcommand)]
pub enum BoardsCmd {
    /// List the server's boards (remote ids feed `boards select`).
    List,
    /// Bind this installation to one board (refuses a rebind).
    Select {
        /// The remote Deck board id (from `boards list`).
        board: u64,
        /// Adoption wait budget, in seconds (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
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
    /// Complete (or with `--undo`, reopen) a task by exact local id.
    Done {
        /// The task's local uuid (from `tasks list`).
        task: String,
        /// Reopen the task instead of completing it.
        #[arg(long)]
        undo: bool,
    },
    /// Move a task to a stack and/or position (one composite intent).
    Move {
        /// The task's local uuid (from `tasks list`).
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
}
