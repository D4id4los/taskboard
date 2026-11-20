// SPDX-License-Identifier: AGPL-3.0-only

//! The pixel/golden-compare probe suites, grouped into one binary
//! (#1070, plan §3.1): every member compares a software-rendered frame
//! against a committed golden through the `support::golden_compare`
//! harness with `headless_renderer: true` — the software-renderer
//! group stays separate from the desktop-renderer probes.
//! Members are verbatim moves living under `tests/probes/`
//! (`virtualisation.rs` included: its deep-board golden capture needs
//! the software renderer, so the strict headless partition puts it
//! here rather than with the desktop-renderer composition probes).

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

#[path = "probes/icon_probe_tests.rs"]
mod icon_probe_tests;
#[path = "probes/stale_remount_probe_tests.rs"]
mod stale_remount_probe_tests;
#[path = "probes/stat_bars_severing_probe_tests.rs"]
mod stat_bars_severing_probe_tests;
#[path = "probes/task_card_hover_probe_tests.rs"]
mod task_card_hover_probe_tests;
#[path = "probes/task_card_title_probe_tests.rs"]
mod task_card_title_probe_tests;
#[path = "probes/tooltip_layout_probe_tests.rs"]
mod tooltip_layout_probe_tests;
#[path = "probes/virtualisation.rs"]
mod virtualisation;
