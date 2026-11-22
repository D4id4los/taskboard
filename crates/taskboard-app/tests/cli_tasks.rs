// SPDX-License-Identifier: AGPL-3.0-only

//! The task-lifecycle CLI verb family, grouped into one binary (#1070,
//! plan §3.2): editing/undo/journal/track/timers/reminders/repeat/
//! habits verb ceremonies. Members are verbatim moves living under
//! `tests/cli/`. The shared `common` module resolves once at this root.

// The shared helpers module, declared exactly once per merged binary:
// a per-member `mod common` copy (each member's old standalone
// preamble) trips `clippy::duplicate_mod` under `--all-targets -D
// warnings` and multiplies any process-global helper state. Members
// consume it via `use crate::common;`.
#[path = "common/mod.rs"]
mod common;

#[path = "cli/cli_editing.rs"]
mod cli_editing;
#[path = "cli/cli_exit_codes.rs"]
mod cli_exit_codes;
#[path = "cli/cli_habits.rs"]
mod cli_habits;
#[path = "cli/cli_journal.rs"]
mod cli_journal;
#[path = "cli/cli_reminders.rs"]
mod cli_reminders;
#[path = "cli/cli_repeat.rs"]
mod cli_repeat;
#[path = "cli/cli_timers.rs"]
mod cli_timers;
#[path = "cli/cli_track.rs"]
mod cli_track;
#[path = "cli/cli_undo.rs"]
mod cli_undo;
