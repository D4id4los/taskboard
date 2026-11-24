// SPDX-License-Identifier: AGPL-3.0-only

//! The journey-driver suites, grouped into one binary (#1070, plan §3.1):
//! every member drives journeys through `tests/support/driver.rs` with
//! `headless_renderer: true`, so a merged binary keeps the one-platform
//! invariant (options are per test binary). Members are verbatim moves;
//! the files live under `tests/journeys/` (subdirectories are not
//! auto-discovered as targets).

// The shared harness module, declared exactly once per merged binary:
// `driver.rs`'s platform dispatch runs off process-global `OnceLock`s,
// so a per-member `mod support` copy (each member's old standalone
// preamble) would multiply the platform init and deadlock plain
// `cargo test` on this target. Members consume it via `crate::support`.
#[path = "support/mod.rs"]
mod support;

/// The one-platform mode this merged binary standardizes on: the
/// `support` self-tests read it so their `DriverOptions` agree with
/// the members sharing the process (`driver::run`'s guard asserts one
/// platform per process; see #1016).
pub(crate) const HEADLESS_RENDERER: bool = true;

#[path = "journeys/board_ceremonies_journeys.rs"]
mod board_ceremonies_journeys;
#[path = "journeys/board_move_journeys.rs"]
mod board_move_journeys;
#[path = "journeys/detail_focus_journeys.rs"]
mod detail_focus_journeys;
#[path = "journeys/detail_shortcuts_journeys.rs"]
mod detail_shortcuts_journeys;
#[path = "journeys/export_journeys.rs"]
mod export_journeys;
#[path = "journeys/focus_coverage.rs"]
mod focus_coverage;
#[path = "journeys/focus_law_journeys.rs"]
mod focus_law_journeys;
#[path = "journeys/golden_visuals.rs"]
mod golden_visuals;
#[path = "journeys/habit_family_journeys.rs"]
mod habit_family_journeys;
#[path = "journeys/habit_surface_journeys.rs"]
mod habit_surface_journeys;
#[path = "journeys/habits_journey.rs"]
mod habits_journey;
#[path = "journeys/keymap_case_seam.rs"]
mod keymap_case_seam;
#[path = "journeys/small_gated_journeys.rs"]
mod small_gated_journeys;
#[path = "journeys/status_export_journeys_command_journeys.rs"]
mod status_export_journeys_command_journeys;
#[path = "journeys/status_export_journeys_preview_journeys.rs"]
mod status_export_journeys_preview_journeys;
#[path = "journeys/status_export_journeys_support.rs"]
mod status_export_journeys_support;
#[path = "journeys/status_surface_journeys.rs"]
mod status_surface_journeys;
#[path = "journeys/tracker_journeys.rs"]
mod tracker_journeys;
#[path = "journeys/undo_family_browse_parity_journeys.rs"]
mod undo_family_browse_parity_journeys;
#[path = "journeys/undo_family_restore_clear_surfaces_journeys.rs"]
mod undo_family_restore_clear_surfaces_journeys;
#[path = "journeys/undo_ui_branch_picker_journeys.rs"]
mod undo_ui_branch_picker_journeys;

#[path = "journeys/undo_ui_reminder_undo_journeys.rs"]
mod undo_ui_reminder_undo_journeys;
#[path = "journeys/usability_components.rs"]
mod usability_components;
