// SPDX-License-Identifier: AGPL-3.0-only

//! The boards/sync CLI verb family, grouped into one binary (#1070,
//! plan §3.2): wiremock ceremonies for the attach/break/connect/
//! standalone/verbs grammars. Members are verbatim moves living under
//! `tests/cli/`. The shared `common` module resolves once at this root.

// The shared helpers module, declared exactly once per merged binary:
// a per-member `mod common` copy (each member's old standalone
// preamble) trips `clippy::duplicate_mod` under `--all-targets -D
// warnings` and multiplies any process-global helper state. Members
// consume it via `use crate::common;`.
#[path = "common/mod.rs"]
mod common;

#[path = "cli/cli_attach.rs"]
mod cli_attach;
#[path = "cli/cli_breaks.rs"]
mod cli_breaks;
#[path = "cli/cli_connect.rs"]
mod cli_connect;
#[path = "cli/cli_standalone.rs"]
mod cli_standalone;
#[path = "cli/cli_verbs.rs"]
mod cli_verbs;
