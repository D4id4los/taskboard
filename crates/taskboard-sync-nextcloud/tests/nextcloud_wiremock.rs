// SPDX-License-Identifier: MIT OR Apache-2.0

//! The wiremock/hermetic API integration family, grouped into one
//! binary (#1087, plan §3.3): the actor wiremock suite, the Deck API
//! contract pins, the hermetic end-to-end suite, the actor-detach
//! facts, and the mapping bench-fixture pins — all driving the mock
//! Deck server through the shared `common` harness. Members are
//! verbatim moves under `tests/suite/` (subdirectories are not
//! auto-discovered as targets); zero test renames — the only
//! nextest-string delta is the inserted member-module segment. The
//! `#[ignore]` tier-2/3 binaries (`it_docker`, `it_live`) stay
//! standalone so the tier invocations and the docker-tier
//! serialization story stay zero-diff.

// The shared harness module, declared exactly once per merged binary.
// Members consume it via the `use crate::common;` line that replaced
// each member's standalone `mod common;` preamble (edition-2024
// uniform paths resolve the bare `use common::…` against the root).
#[path = "common/mod.rs"]
mod common;

#[path = "suite/actor_detach.rs"]
mod actor_detach;
#[path = "suite/actor_wiremock.rs"]
mod actor_wiremock;
#[path = "suite/contract.rs"]
mod contract;
#[path = "suite/e2e_hermetic.rs"]
mod e2e_hermetic;
#[path = "suite/mapping_bench_fixtures.rs"]
mod mapping_bench_fixtures;
#[path = "suite/move_marker.rs"]
mod move_marker;
#[path = "suite/timeout_wiremock.rs"]
mod timeout_wiremock;
