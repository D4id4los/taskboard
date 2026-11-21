// SPDX-License-Identifier: MIT OR Apache-2.0

//! The feature-family integration suite (#1087, plan §3.1): the
//! habits, reminders, reminders-deadline, and reflection engines over
//! the same shared harness. Members are verbatim moves under
//! `tests/suite/` (subdirectories are not auto-discovered as targets);
//! zero test renames — the only nextest-string delta is the inserted
//! member-module segment.

// The shared harness module, declared exactly once per merged binary.
// Members consume it via the `use crate::common;` line that replaced
// each member's standalone `mod common;` preamble (edition-2024
// uniform paths resolve the bare `use common::…` against the root).
#[path = "common/mod.rs"]
mod common;

#[path = "suite/habits.rs"]
mod habits;
#[path = "suite/reflection.rs"]
mod reflection;
#[path = "suite/reminders.rs"]
mod reminders;
#[path = "suite/reminders_deadline.rs"]
mod reminders_deadline;
