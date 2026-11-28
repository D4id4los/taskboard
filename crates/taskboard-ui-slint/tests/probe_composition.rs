// SPDX-License-Identifier: AGPL-3.0-only

//! The geometry/composition probe suites, grouped into one binary
//! (#1070, plan §3.1): every member mounts a generated probe window
//! (`include!(OUT_DIR/<probe>.rs)`; component names are probe-unique)
//! and drives it with `headless_renderer: false`, so a merged binary
//! keeps the one-platform invariant (options are per test binary).
//! Members are verbatim moves living under `tests/probes/`.
//! (`virtualisation.rs` was mapped here by the plan but genuinely
//! needs the software renderer for one golden capture — it lives in
//! `probe_pixel.rs` instead, keeping the mode partition strict.)

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
pub(crate) const HEADLESS_RENDERER: bool = false;

#[path = "probes/action_menu_bar_probe_tests.rs"]
mod action_menu_bar_probe_tests;
#[path = "probes/alarm_window_drag_tests.rs"]
mod alarm_window_drag_tests;
#[path = "probes/alarm_window_max_probe_tests.rs"]
mod alarm_window_max_probe_tests;
#[path = "probes/alarm_window_ring_probe_tests.rs"]
mod alarm_window_ring_probe_tests;
#[path = "probes/app_window_max_probe_tests.rs"]
mod app_window_max_probe_tests;
#[path = "probes/armed_ring_probe_tests.rs"]
mod armed_ring_probe_tests;
#[path = "probes/board_bar_probe_tests.rs"]
mod board_bar_probe_tests;
#[path = "probes/board_empty_stub_probe_tests.rs"]
mod board_empty_stub_probe_tests;
#[path = "probes/board_window_max_probe_tests.rs"]
mod board_window_max_probe_tests;
#[path = "probes/columns_seek_probe_tests.rs"]
mod columns_seek_probe_tests;
#[path = "probes/dropdown_probe_tests.rs"]
mod dropdown_probe_tests;
#[path = "probes/editor_inset_probe_tests.rs"]
mod editor_inset_probe_tests;
#[path = "probes/editor_viewport_probe_tests.rs"]
mod editor_viewport_probe_tests;
#[path = "probes/focus_seam_probe_tests.rs"]
mod focus_seam_probe_tests;
#[path = "probes/gating_mount_probe_tests.rs"]
mod gating_mount_probe_tests;
#[path = "probes/habit_history_window_max_probe_tests.rs"]
mod habit_history_window_max_probe_tests;
#[path = "probes/menu_shell_probe_tests.rs"]
mod menu_shell_probe_tests;
#[path = "probes/overlay_migration_probe_tests.rs"]
mod overlay_migration_probe_tests;
#[path = "probes/plan_next_day_window_max_probe_tests.rs"]
mod plan_next_day_window_max_probe_tests;
#[path = "probes/plan_seek_probe_tests.rs"]
mod plan_seek_probe_tests;
#[path = "probes/quick_bar_gate_probe_tests.rs"]
mod quick_bar_gate_probe_tests;
#[path = "probes/review_window_max_probe_tests.rs"]
mod review_window_max_probe_tests;
#[path = "probes/settings_window_max_probe_tests.rs"]
mod settings_window_max_probe_tests;
#[path = "probes/slider_grab_probe_tests.rs"]
mod slider_grab_probe_tests;
#[path = "probes/stack_window_max_probe_tests.rs"]
mod stack_window_max_probe_tests;
#[path = "probes/task_window_max_probe_tests.rs"]
mod task_window_max_probe_tests;
#[path = "probes/timers_row_probe_tests.rs"]
mod timers_row_probe_tests;
#[path = "probes/today_window_max_probe_tests.rs"]
mod today_window_max_probe_tests;
#[path = "probes/walk_probe_tests.rs"]
mod walk_probe_tests;
#[path = "probes/widget_framework_probe_tests.rs"]
mod widget_framework_probe_tests;
