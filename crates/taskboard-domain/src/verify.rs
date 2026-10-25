// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kani bounded-model-checking harnesses for the pure domain core.
//!
//! Compiled only under `cfg(kani)` (Kani's own rustc driver sets it);
//! never compiled by `cargo test`/`nextest`/`clippy`/coverage. The
//! `kani` proof library auto-provisions from the Kani bundle — no
//! Cargo.toml dependency. Budget and scope:
//! `.artifacts/plans/2026-10-25-kani-pilot-plan.md` and issue #426.
//!
//! Harness rule: every symbolic input is the narrowest type covering
//! the law's domain (CBMC bitblasts the *type*; `assume` ranges do
//! not shrink the circuit). `DateTime`s are minted from `i32`
//! seconds-since-1970 via `DateTime::from_timestamp` (nanos concrete
//! zero — the folds compare whole instants); `String`-valued fields
//! are never symbolized (the known CBMC memory-bomb class, measured
//! in the pilot plan §1).

use crate::state::{SyncCounters, SyncErrorKind, SyncPhase, SyncStatus, aggregate_rows};

/// Harness-local reference rank mirroring `state::phase_rank` (the
/// production fn is private; the mirror is the law's *specification*,
/// and a drift between the two is exactly what the proof would
/// surface as a counterexample).
fn spec_phase_rank(phase: SyncPhase) -> u8 {
    match phase {
        SyncPhase::Idle => 0,
        SyncPhase::Offline => 1,
        SyncPhase::Failed { .. } => 2,
        SyncPhase::Syncing => 3,
    }
}

/// Mints a symbolic `SyncPhase` from a `u8` index: 0 Idle, 1 Syncing,
/// 2 Offline, 3..=9 `Failed` with the seven `SyncErrorKind`s (narrow
/// enum tags; no wide arithmetic anywhere in the fold path).
fn mint_phase(tag: u8) -> SyncPhase {
    match tag {
        0 => SyncPhase::Idle,
        1 => SyncPhase::Syncing,
        2 => SyncPhase::Offline,
        3 => SyncPhase::Failed {
            last_error: SyncErrorKind::Network,
        },
        4 => SyncPhase::Failed {
            last_error: SyncErrorKind::Auth,
        },
        5 => SyncPhase::Failed {
            last_error: SyncErrorKind::Forbidden,
        },
        6 => SyncPhase::Failed {
            last_error: SyncErrorKind::Server,
        },
        7 => SyncPhase::Failed {
            last_error: SyncErrorKind::BadRequest,
        },
        8 => SyncPhase::Failed {
            last_error: SyncErrorKind::LocalData,
        },
        _ => SyncPhase::Failed {
            last_error: SyncErrorKind::NoBoard,
        },
    }
}

fn mint_row(
    phase_tag: u8,
    last_success_secs: i32,
    last_success_some: bool,
    pending_ops: u32,
    counters: SymCounters,
    last_cycle_some: bool,
) -> SyncStatus {
    SyncStatus {
        phase: mint_phase(phase_tag),
        last_success: if last_success_some {
            chrono::DateTime::from_timestamp(i64::from(last_success_secs), 0)
        } else {
            None
        },
        pending_ops,
        counters: SyncCounters {
            cycles: u64::from(counters.cycles),
            pushes: u64::from(counters.pushes),
            pulls: u64::from(counters.pulls),
            failures: u64::from(counters.failures),
            offline_episodes: u64::from(counters.offline_episodes),
            last_cycle: if last_cycle_some {
                Some(std::time::Duration::from_secs(u64::from(counters.last_cycle)))
            } else {
                None
            },
        },
        // Not read by the fold — never symbolized what the law cannot
        // see (plan §3.3).
        sidecar: Default::default(),
        features: Default::default(),
    }
}

#[derive(Debug, Clone, Copy)]
struct SymCounters {
    cycles: u32,
    pushes: u32,
    pulls: u32,
    failures: u32,
    offline_episodes: u32,
    last_cycle: u32,
}

/// Escalates the `aggregate_rows` fold's unit/proptest coverage to
/// exhaustive-within-bounds over a fixed array of 3 symbolic rows:
///
/// 1. **Phase dominance**: `aggregated.phase` carries the maximal
///    `phase_rank`, and among max-rank rows the *first* wins (the
///    `Failed`-tie-keeps-lowest-board-id contract through
///    sorted-rows feeding).
/// 2. **`last_success` = row-wise max** (`None < Some`).
/// 3. **`pending_ops` = saturating sum** (never overflows).
/// 4. **Counters**: saturating sums; `last_cycle` = max.
/// 5. **Panic-freedom** of the whole fold (implicit: Kani checks
///    every reachable panic/overflow on the symbolic path).
///
/// Unwind: the fold's `for` loop over 3 rows. Bound-doubling check:
/// identical verdict at `#[kani::unwind(4)]` and
/// `#[kani::unwind(8)]` (proven, both bounds).
#[kani::proof]
#[kani::unwind(4)]
fn prop_aggregate_rows_dominance_max_saturating_proven() {
    let rows: [SyncStatus; 3] = [
        mint_row(
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            SymCounters {
                cycles: kani::any(),
                pushes: kani::any(),
                pulls: kani::any(),
                failures: kani::any(),
                offline_episodes: kani::any(),
                last_cycle: kani::any(),
            },
            kani::any(),
        ),
        mint_row(
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            SymCounters {
                cycles: kani::any(),
                pushes: kani::any(),
                pulls: kani::any(),
                failures: kani::any(),
                offline_episodes: kani::any(),
                last_cycle: kani::any(),
            },
            kani::any(),
        ),
        mint_row(
            kani::any(),
            kani::any(),
            kani::any(),
            kani::any(),
            SymCounters {
                cycles: kani::any(),
                pushes: kani::any(),
                pulls: kani::any(),
                failures: kani::any(),
                offline_episodes: kani::any(),
                last_cycle: kani::any(),
            },
            kani::any(),
        ),
    ];

    let aggregated = aggregate_rows(rows.iter());

    // Law 1: dominance + first-max-wins, against a reference fold.
    let mut expected_phase = SyncPhase::Idle;
    for row in &rows {
        if spec_phase_rank(row.phase) > spec_phase_rank(expected_phase) {
            expected_phase = row.phase;
        }
    }
    assert_eq!(aggregated.phase, expected_phase);

    // Law 2: last_success = row-wise max.
    let mut expected_last_success: Option<chrono::DateTime<chrono::Utc>> = None;
    for row in &rows {
        expected_last_success = std::cmp::max(expected_last_success, row.last_success);
    }
    assert_eq!(aggregated.last_success, expected_last_success);

    // Law 3: pending_ops = saturating sum.
    let mut expected_pending: u32 = 0;
    for row in &rows {
        expected_pending = expected_pending.saturating_add(row.pending_ops);
    }
    assert_eq!(aggregated.pending_ops, expected_pending);

    // Law 4: counters — saturating sums and max last_cycle, per field.
    let mut expected_counters = SyncCounters::default();
    for row in &rows {
        expected_counters.fold_into(&row.counters);
    }
    assert_eq!(aggregated.counters, expected_counters);
}
