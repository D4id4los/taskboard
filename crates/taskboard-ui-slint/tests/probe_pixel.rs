// SPDX-License-Identifier: AGPL-3.0-only

//! The pixel/golden-compare probe suites, grouped into one binary
//! (#1070, plan §3.1): every member compares a software-rendered frame
//! against a committed golden through the `support::golden_compare`
//! harness with `headless_renderer: true` — the software-renderer
//! group stays separate from the desktop-renderer probes.
//! Members are verbatim moves living under `tests/probes/`.

#[path = "probes/icon_probe_tests.rs"]
mod icon_probe_tests;
#[path = "probes/stat_bars_severing_probe_tests.rs"]
mod stat_bars_severing_probe_tests;
#[path = "probes/task_card_hover_probe_tests.rs"]
mod task_card_hover_probe_tests;
#[path = "probes/task_card_title_probe_tests.rs"]
mod task_card_title_probe_tests;
#[path = "probes/tooltip_layout_probe_tests.rs"]
mod tooltip_layout_probe_tests;
