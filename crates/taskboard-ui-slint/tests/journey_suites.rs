// SPDX-License-Identifier: AGPL-3.0-only

//! The journey-driver suites, grouped into one binary (#1070, plan §3.1):
//! every member drives journeys through `tests/support/driver.rs` with
//! `headless_renderer: true`, so a merged binary keeps the one-platform
//! invariant (options are per test binary). Members are verbatim moves;
//! the files live under `tests/journeys/` (subdirectories are not
//! auto-discovered as targets).

#[path = "journeys/board_move_journeys.rs"]
mod board_move_journeys;
#[path = "journeys/focus_coverage.rs"]
mod focus_coverage;
#[path = "journeys/focus_law_journeys.rs"]
mod focus_law_journeys;
#[path = "journeys/golden_visuals.rs"]
mod golden_visuals;
#[path = "journeys/habits_journey.rs"]
mod habits_journey;
#[path = "journeys/keymap_case_seam.rs"]
mod keymap_case_seam;
#[path = "journeys/usability_components.rs"]
mod usability_components;
