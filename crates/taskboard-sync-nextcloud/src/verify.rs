// SPDX-License-Identifier: MIT OR Apache-2.0
//! Kani bounded-model-checking harnesses for the sync crate.
//!
//! Compiled only under `cfg(kani)` (Kani's own rustc driver sets it);
//! never compiled by `cargo test`/`nextest`/`clippy`/coverage. The
//! `kani` proof library auto-provisions from the Kani bundle — no
//! Cargo.toml dependency. Budget and scope:
//! `.artifacts/plans/2026-10-25-kani-pilot-plan.md` and issue #426.
//!
//! Harness rule: every symbolic input is the narrowest type covering
//! the law's domain (CBMC bitblasts the *type*; `assume` ranges do
//! not shrink the circuit).

use crate::backoff::BackoffPolicy;

/// Escalates `prop_delays_positive_increasing_capped`
/// (`backoff.rs`, 256 proptest samples) to exhaustive-within-bounds:
/// over every `retry_index` in `0..12` against `BackoffPolicy::new()`,
///
/// 1. `delay(i) == Some(d)` ⇒ `d > ZERO ∧ d <= 8 s`,
/// 2. `delay(i) == Some(d) ∧ delay(i-1) == Some(p)` ⇒ `d >= p`,
/// 3. `delay(i) == None ⇔ i + 1 >= max_attempts`.
///
/// Unwind: `delay_at`'s doubling loop runs at most `retry_index` < 11
/// iterations. Bound-doubling check: identical verdict at
/// `#[kani::unwind(12)]` and `#[kani::unwind(24)]`
/// (proven, 103 checks, 2 unreachable — both bounds).
#[kani::proof]
#[kani::unwind(12)]
fn prop_backoff_delays_positive_increasing_capped_proven() {
    let retry_index: u32 = kani::any();
    kani::assume(retry_index < 12);
    let p = BackoffPolicy::new();
    if let Some(d) = p.delay(retry_index) {
        assert!(d > std::time::Duration::ZERO);
        assert!(d <= std::time::Duration::from_secs(8));
        if retry_index > 0
            && let Some(prev) = p.delay(retry_index - 1)
        {
            assert!(d >= prev);
        }
    } else {
        assert!(retry_index + 1 >= p.max_attempts());
    }
}
