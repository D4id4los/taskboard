// SPDX-License-Identifier: AGPL-3.0-only

//! The geometry/composition probe suites, grouped into one binary
//! (#1070, plan §3.1): every member mounts a generated probe window
//! (`include!(OUT_DIR/<probe>.rs)`; component names are probe-unique)
//! and drives it with `headless_renderer: false`, so a merged binary
//! keeps the one-platform invariant (options are per test binary).
//! Members are verbatim moves living under `tests/probes/`.

#[path = "probes/action_menu_bar_probe_tests.rs"]
mod action_menu_bar_probe_tests;
#[path = "probes/board_bar_probe_tests.rs"]
mod board_bar_probe_tests;
#[path = "probes/dropdown_probe_tests.rs"]
mod dropdown_probe_tests;
#[path = "probes/editor_viewport_probe_tests.rs"]
mod editor_viewport_probe_tests;
#[path = "probes/focus_seam_probe_tests.rs"]
mod focus_seam_probe_tests;
#[path = "probes/overlay_migration_probe_tests.rs"]
mod overlay_migration_probe_tests;
#[path = "probes/plan_window_max_probe_tests.rs"]
mod plan_window_max_probe_tests;
#[path = "probes/slider_grab_probe_tests.rs"]
mod slider_grab_probe_tests;
#[path = "probes/virtualisation.rs"]
mod virtualisation;
#[path = "probes/walk_probe_tests.rs"]
mod walk_probe_tests;
#[path = "probes/widget_framework_probe_tests.rs"]
mod widget_framework_probe_tests;
#[path = "probes/window_max_probe_tests.rs"]
mod window_max_probe_tests;
