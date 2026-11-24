// SPDX-License-Identifier: AGPL-3.0-only

//! The structural/data inventory suites, grouped into one binary
//! (#1070, plan §3.1): pure source- and data-shape assertions with no
//! platform init (no `driver::run` call), so any grouping is
//! behavior-neutral — merged to stop each member paying a full slint
//! link as a standalone binary. Members are verbatim moves living
//! under `tests/inventory/` (the `include_str!` depth adjusted).

// The bench fixture module, declared once at the binary root so the
// member's bare `use support::…` paths resolve to this module ahead of
// the extern crate (edition-2024 uniform paths) and only one copy of
// the fixture is instantiated. Members consume it via `crate::support`.
#[path = "../benches/support/mod.rs"]
mod support;

#[path = "inventory/alarm_window_structural.rs"]
mod alarm_window_structural;
#[path = "inventory/bench_fixture_contract.rs"]
mod bench_fixture_contract;
#[path = "inventory/icon_mapping.rs"]
mod icon_mapping;
#[path = "inventory/keymap_snapshots.rs"]
mod keymap_snapshots;
#[path = "inventory/seam_inventory.rs"]
mod seam_inventory;
#[path = "inventory/view_mount_inventory.rs"]
mod view_mount_inventory;
