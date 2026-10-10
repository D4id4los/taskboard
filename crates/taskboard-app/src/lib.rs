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
//! - [`cli`]: the clap surface (dispatch lives in the binary).
//!
//! This is internal API (the workspace is `publish = false`): no
//! stability promises across crates. Tracing is initialized by the
//! binary; the lib only *emits* spans (subscriber-less tests make the
//! macros no-ops).

#![forbid(unsafe_code)]

pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod secrets;

/// Application version, taken from the shared workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use bootstrap::{App, BootstrapError, ShutdownOutcome, bootstrap, run_until_signal};
