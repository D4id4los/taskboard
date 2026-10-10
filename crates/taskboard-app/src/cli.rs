// SPDX-License-Identifier: AGPL-3.0-only
//! The CLI surface (ADR 0008): clap types only — dispatch lives in the
//! binary. Phase 6 grows this enum (`Login`, `Boards`, `Tasks`, `Sync`)
//! without touching `main`'s dispatch shape.

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
    /// The subcommand; `None` resolves `[app] mode` (the daemon until
    /// the UI phases land).
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The subcommand catalogue.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run the headless sync daemon (M1).
    Daemon,
}
