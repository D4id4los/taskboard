// SPDX-License-Identifier: AGPL-3.0-only

//! The task-lifecycle CLI verb family, grouped into one binary (#1070,
//! plan §3.2): editing/undo/journal/track/timers/reminders/repeat/
//! habits verb ceremonies. Members are verbatim moves living under
//! `tests/cli/`; each keeps its own `common` include.

#[path = "cli/cli_editing.rs"]
mod cli_editing;
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
