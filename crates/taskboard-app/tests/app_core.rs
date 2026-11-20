// SPDX-License-Identifier: AGPL-3.0-only

//! The boot/config/runtime-core suites, grouped into one binary
//! (#1070, plan §3.2): bootstrap, config, daemon signals, dispatch
//! routing, lockfile, repl, secrets. Members are verbatim moves living
//! under `tests/core/`; each keeps its own `common` include.

#[path = "core/bootstrap.rs"]
mod bootstrap;
#[path = "core/config.rs"]
mod config;
#[path = "core/daemon_signals.rs"]
mod daemon_signals;
#[path = "core/dispatch_routing.rs"]
mod dispatch_routing;
#[path = "core/lockfile.rs"]
mod lockfile;
#[path = "core/repl.rs"]
mod repl;
#[path = "core/secrets.rs"]
mod secrets;
