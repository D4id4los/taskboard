// SPDX-License-Identifier: AGPL-3.0-only

//! The boot/config/runtime-core suites, grouped into one binary
//! (#1070, plan §3.2): bootstrap, config, daemon signals, dispatch
//! routing, lockfile, repl, secrets. Members are verbatim moves living
//! under `tests/core/`.

// The shared helpers module, declared exactly once per merged binary:
// a per-member `mod common` copy (each member's old standalone
// preamble) trips `clippy::duplicate_mod` under `--all-targets -D
// warnings` and multiplies any process-global helper state. Members
// consume it via `use crate::common;`.
#[path = "common/mod.rs"]
mod common;

#[path = "core/bootstrap.rs"]
mod bootstrap;
#[path = "core/config.rs"]
mod config;
#[path = "core/daemon_signals.rs"]
mod daemon_signals;
#[path = "core/dispatch_routing.rs"]
mod dispatch_routing;
#[path = "core/find.rs"]
mod find;
#[path = "core/lockfile.rs"]
mod lockfile;
#[path = "core/repl.rs"]
mod repl;
#[path = "core/secrets.rs"]
mod secrets;
