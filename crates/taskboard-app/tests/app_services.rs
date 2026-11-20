// SPDX-License-Identifier: AGPL-3.0-only

//! The long-running service surfaces, grouped into one binary (#1070,
//! plan §3.2): ipc, files-sync convergence, ui settings (cold + live),
//! the web dashboard. Members are verbatim moves living under
//! `tests/services/`. The shared `common` module resolves once at this root.

// The shared helpers module, declared exactly once per merged binary:
// a per-member `mod common` copy (each member's old standalone
// preamble) trips `clippy::duplicate_mod` under `--all-targets -D
// warnings` and multiplies any process-global helper state. Members
// consume it via `use crate::common;`.
#[path = "common/mod.rs"]
mod common;

#[path = "services/files_sync_convergence.rs"]
mod files_sync_convergence;
#[path = "services/ipc.rs"]
mod ipc;
#[path = "services/ui_live_settings.rs"]
mod ui_live_settings;
#[path = "services/ui_settings.rs"]
mod ui_settings;
#[path = "services/web_dashboard.rs"]
mod web_dashboard;
