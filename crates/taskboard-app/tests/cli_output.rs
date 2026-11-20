// SPDX-License-Identifier: AGPL-3.0-only

//! The output/tty/export/lock/login/ipc-pipe CLI family, grouped into
//! one binary (#1070, plan §3.2). Members are verbatim moves living
//! under `tests/cli/` (the member named `cli_output` rides under this
//! root of the same name); each keeps its own `common` include.

// The shared helpers module, declared exactly once per merged binary:
// a per-member `mod common` copy (each member's old standalone
// preamble) trips `clippy::duplicate_mod` under `--all-targets -D
// warnings` and multiplies any process-global helper state. Members
// consume it via `use crate::common;`.
#[path = "common/mod.rs"]
mod common;

#[path = "cli/cli_export_pdf.rs"]
mod cli_export_pdf;
#[path = "cli/cli_ipc.rs"]
mod cli_ipc;
#[path = "cli/cli_lock.rs"]
mod cli_lock;
#[path = "cli/cli_login.rs"]
mod cli_login;
#[path = "cli/cli_output.rs"]
mod cli_output;
#[path = "cli/cli_tty.rs"]
mod cli_tty;
