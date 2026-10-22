// SPDX-License-Identifier: AGPL-3.0-only
//! `taskboard-app` bootstrap library.
//!
//! Everything the `taskboard` binary does except process-glue lives
//! here so integration tests, doc tests, and every future frontend
//! (Phase 6 CLI subcommands, Phase 7 UI) can boot the same in-process
//! core (ADR 0008):
//!
//! - [`config`]: figment-backed, secret-free-by-construction
//!   [`AppConfig`](config::AppConfig).
//! - [`secrets`]: the [`CredentialStore`](secrets::CredentialStore)
//!   port, keyring + env stores, and the redacting
//!   [`AppPassword`](secrets::AppPassword).
//! - [`bootstrap`]: the actor-graph wiring and the [`App`] handle with
//!   its graceful [`shutdown`](bootstrap::App::shutdown) sequence.
//! - [`lockfile`]: the per-database instance lock (one core per
//!   database; OS-released on process death).
//! - [`commands`]: the operator-facing command handlers (ADR 0009) —
//!   `login`, `boards`, `tasks`, `sync` — with the `--json` output
//!   contract; the binary is dispatch + exit codes.
//! - [`repl`]: the interactive session frontend (ADR 0011) — one booted
//!   core, one line per invocation.
//! - [`cli`]: the clap surface.
//!
//! This is internal API (the workspace is `publish = false`): no
//! stability promises across crates. Tracing is initialized by the
//! binary; the lib only *emits* spans (subscriber-less tests make the
//! macros no-ops).

#![forbid(unsafe_code)]

pub mod authority;
pub mod bootstrap;
pub mod cli;
pub mod commands;
mod completion;
pub mod config;
pub mod connections;
pub mod connectors;
pub mod ipc;
pub mod lockfile;
pub mod notifications;
pub mod repl;
mod repl_history;
pub mod secrets;
mod session_ceremonies;
pub mod ui;
pub mod ui_requests;
pub mod web;

/// Application version, taken from the shared workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use bootstrap::{App, BootstrapError, ShutdownOutcome, bootstrap, run_until_signal};
