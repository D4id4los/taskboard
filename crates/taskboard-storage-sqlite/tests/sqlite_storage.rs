// SPDX-License-Identifier: MIT OR Apache-2.0

//! The repository-semantics integration family, grouped into one
//! binary (#1087, plan §3.2): the actor, board adoption, repository
//! contract, corrupt-row matrix, fixture hygiene, journal storage,
//! reflection storage, schema, and bench-fixture contract pins over
//! one harness. Members are verbatim moves under `tests/suite/`
//! (subdirectories are not auto-discovered as targets); zero test
//! renames — the only nextest-string delta is the inserted
//! member-module segment. The proptest + slow-tail `roundtrip`
//! binary stays standalone by design (its regression-file position
//! and the suite's longest test stay a distinct scheduling unit).

// The shared harness module, declared exactly once per merged binary.
// Members consume it via the `use crate::common;` line that replaced
// each member's standalone `mod common;` preamble (edition-2024
// uniform paths resolve the bare `use common::…` against the root).
#[path = "common/mod.rs"]
mod common;

#[path = "suite/actor.rs"]
mod actor;
#[path = "suite/bench_fixtures.rs"]
mod bench_fixtures;
#[path = "suite/board_adoption.rs"]
mod board_adoption;
#[path = "suite/contract.rs"]
mod contract;
#[path = "suite/corrupt_row_matrix.rs"]
mod corrupt_row_matrix;
#[path = "suite/fixture_hygiene.rs"]
mod fixture_hygiene;
#[path = "suite/journal_storage.rs"]
mod journal_storage;
#[path = "suite/reflection_storage.rs"]
mod reflection_storage;
#[path = "suite/schema.rs"]
mod schema;
