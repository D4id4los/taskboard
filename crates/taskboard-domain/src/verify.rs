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
                Some(std::time::Duration::from_secs(u64::from(
                    counters.last_cycle,
                )))
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
/// `#[kani::unwind(8)]` (proven, 690 checks, 5 unreachable — both
/// bounds).
/// Mints one fully symbolic row (`kani::any` mints per call site, so
/// each of the harness's three calls yields independent symbols).
fn mint_sym_row() -> SyncStatus {
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
    )
}

#[kani::proof]
#[kani::unwind(4)]
fn prop_aggregate_rows_dominance_max_saturating_proven() {
    let rows: [SyncStatus; 3] = [mint_sym_row(), mint_sym_row(), mint_sym_row()];

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

/// Escalates `window_contains_now` / `window_is_exactly_24_hours`
/// (`days.rs` proptests) to exhaustive-within-bounds over
/// [`crate::days::day_window`]:
///
/// 1. **Panic-freedom/totality** (primary; implicit — Kani checks
///    every reachable panic/overflow/assert on the symbolic path,
///    including chrono's internal debug asserts).
/// 2. `start <= now < end`.
/// 3. `end - start == 24 h`.
///
/// Symbolic surface (anchor-relative, narrowest-type): `now` = `i32`
/// seconds since 2020-01-01 (±68 y), nanos concrete zero;
/// `day_start_time` = `u32` in `0..=86_399` (the proptest's range —
/// keeping parity *is* the sampled→exhaustive escalation claim);
/// `offset` = `i32` in `±86_399` via `FixedOffset::east_opt`.
///
/// Unwind: chrono date math on these paths is loop-light.
/// Bound-doubling check: identical verdict at `#[kani::unwind(8)]`
/// and `#[kani::unwind(16)]` (447 checks, 0 failed, 8 unreachable —
/// both bounds).
#[kani::proof]
#[kani::unwind(8)]
fn prop_day_window_total_24h_contains_now_proven() {
    let now_secs: i32 = kani::any();
    // 2020-01-01T00:00:00Z
    let now = chrono::DateTime::from_timestamp(1_577_836_800_i64 + i64::from(now_secs), 0)
        .expect("i32 range keeps the anchor-relative instant representable");
    let start_secs: u32 = kani::any();
    kani::assume(start_secs <= 86_399);
    let day_start_time = chrono::NaiveTime::from_num_seconds_from_midnight_opt(start_secs, 0);
    kani::assume(day_start_time.is_some());
    let day_start_time = day_start_time.unwrap();
    let offset_secs: i32 = kani::any();
    kani::assume((-86_399..=86_399).contains(&offset_secs));
    let offset = chrono::FixedOffset::east_opt(offset_secs).unwrap();

    let (start, end) = crate::days::day_window(now, day_start_time, offset);

    assert!(start <= now);
    assert!(now < end);
    assert_eq!(end - start, chrono::TimeDelta::try_hours(24).unwrap());
}

// ---- H2: ReminderOffset canonical text round-trip (containment ladder) ----

use crate::reminders::ReminderOffset;

/// Harness-local specification mirror of `parse_text_form`'s semantics
/// — deliberately written as an *independent* single-pass state
/// machine (running digit accumulator vs. the production digit-run +
/// slice parse, so a transcription bug in either is exactly what the
/// proof would surface as a counterexample). The u32 digit
/// accumulator is exact on the harness's ≤4-byte surface; longer
/// digit runs fall outside the proven claim.
fn spec_parse_total(raw: &str) -> Option<i64> {
    if raw.is_empty() {
        return None;
    }
    let mut total: i64 = 0;
    let mut value: u32 = 0;
    let mut in_digits = false;
    for c in raw.chars() {
        let unit_secs: i64 = match (c.to_digit(10), in_digits) {
            (Some(digit), _) => {
                value = value.checked_mul(10)?.checked_add(digit)?;
                in_digits = true;
                continue;
            }
            (None, true) => match c {
                's' => 1,
                'm' => 60,
                'h' => 3_600,
                'd' => 86_400,
                _ => return None, // not a grammar unit (incl. non-ASCII)
            },
            (None, false) => return None, // a component without its number
        };
        total = total.checked_add(i64::from(value).checked_mul(unit_secs)?)?;
        value = 0;
        in_digits = false;
    }
    if in_digits {
        return None; // digits without a closing unit
    }
    Some(total)
}

/// Rung C of the H2 containment ladder (pilot plan §3.2): the real
/// `parse_text_form` over genuinely symbolic bytes. The `format!`
/// render side (Rung A, killed at the 6 GiB cap — the measured String
/// wall) is severed; what remains proven is the parse side:
///
/// 1. **Grammar totality**: never panics over any 4-byte input drawn
///    from the grammar's charset (digits + `s m h d`).
/// 2. **`Some` iff well-formed** and **value == Σ digit·unit** with
///    the non-positive sign discipline, against the spec mirror
///    (including the overflow ⇒ `None` path).
///
/// *What is lost* vs the full law (Rung D territory): no evidence
/// about the actual `format!`-rendered bytes — the canonical
/// round-trip stays with its proptest.
///
/// Unwind 12: the parse path's single char loop runs ≤ 4 iterations;
/// 12 ≥ 2× that need (the production parser's loops are equally
/// bounded by the 4-byte input). The 2×-bound doubling re-run at 24
/// exceeded the 6 GiB local cap and could not be completed (recorded
/// in the pilot report; bump checklist: re-attempt on a bigger
/// runner).
#[kani::proof]
#[kani::unwind(12)]
fn prop_parse_text_form_total_spec_conformant_rung_c() {
    let bytes: [u8; 4] = kani::any(); // rung C shrink: 4 bytes (one component class)
    for b in bytes {
        kani::assume(b.is_ascii_digit() || matches!(b, b's' | b'm' | b'h' | b'd'));
    }
    let raw = std::str::from_utf8(&bytes).expect("charset is ASCII");
    let parsed = ReminderOffset::parse_text_form(raw);
    let expected_secs = spec_parse_total(raw).and_then(|total| {
        chrono::TimeDelta::try_seconds(-total).and_then(|d| ReminderOffset::before(d).ok())
    });
    assert_eq!(parsed, expected_secs);
    if let Some(off) = parsed {
        assert!(off.as_duration() <= chrono::TimeDelta::zero());
    }
}
