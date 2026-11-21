// SPDX-License-Identifier: MIT OR Apache-2.0

//! The engine-core + persistence/config integration family, grouped
//! into one binary (#1087, plan §3.1): engine routing (A-series),
//! tracking, undo, journal-degraded resilience, manifest routing,
//! settings, and the sqlite storage smoke leg of the engine story
//! (`docs/testing_strategy.org` §579). Members are verbatim moves
//! under `tests/suite/` (subdirectories are not auto-discovered as
//! targets); zero test renames — the only nextest-string delta is the
//! inserted member-module segment.

// The shared harness module, declared exactly once per merged binary.
// Members consume it via the `use crate::common;` line that replaced
// each member's standalone `mod common;` preamble (edition-2024
// uniform paths resolve the bare `use common::…` against the root).
#[path = "common/mod.rs"]
mod common;

#[path = "suite/engine.rs"]
mod engine;
#[path = "suite/journal_degraded.rs"]
mod journal_degraded;
#[path = "suite/manifest_routing.rs"]
mod manifest_routing;
#[path = "suite/settings.rs"]
mod settings;
#[path = "suite/sqlite_smoke.rs"]
mod sqlite_smoke;
#[path = "suite/tracking.rs"]
mod tracking;
#[path = "suite/undo.rs"]
mod undo;
