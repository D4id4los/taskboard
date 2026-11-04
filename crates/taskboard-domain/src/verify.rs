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

// ---- Chunk 1.4 (#505): recurrence-arithmetic harnesses ----
//
// Escalates the DR-6 §3.9 recurrence proptests (P1–P4,
// `recur.rs` `#[cfg(test)]`) to exhaustive-within-bounds at the
// chunk's containment rung. Plan:
// `.artifacts/plans/2026-11-03-kani-recurrence-harnesses-plan.md`;
// normative item: issue #505. Same module per the pilot's
// one-harness-module-per-crate convention (plan D-2).
//
// **Rung A surface (measured, recorded deviations from the plan's
// §2.2 domain table and §4 single-mux harness shapes).** Three walls
// were measured on this arithmetic, each forcing one descent
// (documented at the harnesses; details and measurement table in the
// stage-2 report):
//
// 1. **Symbolic date value range** (not circuit size) drives SAT
//    cost: a full-2000–2100-window harness with full-parity offsets
//    did not converge in ~11 h wall / ~9 h CBMC CPU at unwind 4,
//    while a ±90-day band verifies in ~50 s. Every harness therefore
//    mints its anchor as a symbolic ±730-day offset around a
//    concrete base (2050-06-15: the band sweeps 731 anchor positions
//    covering every month boundary, every weekday, a year turn and
//    the 2052-02-29 leap day), and bands stale/gap/from offsets to
//    `u8` (≤ 64 / 1..=56 / ≤ 64) — the plan's own R4 Rung-A values,
//    applied chunk-wide. Span reach (20 000/2 000-day gaps, the full
//    101-year window) stays proptest-owned.
// 2. **Symbolic 64-bit divisors**: the closed forms divide by the
//    interval (`rem_euclid`/`div_euclid` on `i64`); interval mints
//    are symbolic tags selecting among **concrete** values (B5's
//    case-split rule), keeping each branch's division
//    constant-folded. Daily parity 1..=30, Weekly parity 1..=8; the
//    full-`u16` domains stay proptest-owned.
// 3. **Pattern-tag mux of arms** (each arm's concrete-case mux
//    multiplied through one circuit): OOM-killed at the 6 GiB cap in
//    every bundled shape, while every arm verifies alone in seconds
//    to minutes. Harness families are therefore **per pattern arm** —
//    B2's one-law-set-per-harness applied one level deeper; the union
//    of a family carries the law over all four arms.
//
// Within the band every case split of every law is exhaustively
// covered (catch-up vs skip-ahead, clamped Feb, memberless-month
// walks); what is lost vs the proptests is span reach and the
// symbolic-mask/multi-day-set shape, both proptest-owned (P1–P4,
// 256 samples each).
//
// **Dropped harnesses (honest termination per B7/D-7, recorded
// disposition).** Two weekly walk harnesses never fit the 6 GiB
// local envelope at any rung (every mask shape × interval band ×
// unwind tried was OOM-killed):
//
// - *weekly chain law* (2-hop strict-advance + gap over the weekly
//   walk): 4 kills (full 7-bit mask; 2-symbolic-day mask; singleton
//   at interval 1..=8; singleton at interval 1..=2). The 1-hop
//   weekly walk evidence lives in
//   `prop_next_after_strict_member_weekly`; the 2-hop weekly chain
//   law stays proptest-owned (P1/DR-6 §3.9 item 2).
// - *weekly roll law* (P4 catch-up/skip-ahead over the weekly
//   backward walk + its forward walk): 3+ kills (full mask at 64/56;
//   singleton at 64/56 unwind 12; singleton at 32/32 interval 1..=2
//   unwind 8). The Daily (R3) and Monthly-OnDay/Nth5 roll harnesses
//   carry the roll case structure for their arms; the weekly roll
//   law stays proptest-owned (P4, D-7's recorded disposition).
// - *k ≤ 4 monthly-NthWeekday roll law*: 2 kills (k-mux; k ≤ 4
//   split). Its case structure is the memberless-free backward
//   month-walk — covered structurally by the Monthly-OnDay roll
//   harness and the k = 5 sibling (which adds the skip walk); the
//   k ≤ 4 roll instances stay proptest-owned.

use crate::recur::{
    DayOfMonth, EveryDays, EveryWeeks, MonthlyAnchor, NthWeek, RecurPattern, WeekdaySet,
    next_after, occurs_on, roll_target,
};
use chrono::{Datelike, Months, NaiveDate, TimeDelta, Weekday};

/// The concrete base date every harness anchors its band on (chosen
/// mid-window so ±730 days stays inside the proptest window and hits
/// a leap day; any in-window date gives the same circuit class).
fn recur_base() -> NaiveDate {
    NaiveDate::from_ymd_opt(2050, 6, 15).expect("the band base is a real date")
}

/// `base` advanced by a signed day count (the harness-side date
/// arithmetic wrapper; representability holds by construction on the
/// band surfaces every harness uses).
fn date_plus_days(base: NaiveDate, days: i64) -> NaiveDate {
    base.checked_add_signed(TimeDelta::try_days(days).expect("day delta"))
        .expect("a banded offset stays representable")
}

/// Mints the symbolic anchor: recur_base() ± 730 days, full band —
/// the band *is* the documented rung, not a silent narrowing (see
/// the module header).
fn mint_anchor() -> NaiveDate {
    let offset: i16 = kani::any();
    kani::assume((-730..=730).contains(&offset));
    date_plus_days(recur_base(), i64::from(offset))
}

/// Mints `stale` ≥ anchor, ≤ 64 days past it.
fn mint_stale(anchor: NaiveDate) -> NaiveDate {
    let offset: u8 = kani::any();
    kani::assume(offset <= 64);
    date_plus_days(anchor, i64::from(offset))
}

/// Mints `today` a symbolic 1..=56-day gap after `stale` (strictly
/// after — `roll_target`'s precondition shape; 56 covers the longest
/// weekly case window of the interval-8 band).
fn mint_today(stale: NaiveDate) -> NaiveDate {
    let gap: u8 = kani::any();
    kani::assume((1..=56).contains(&gap));
    date_plus_days(stale, i64::from(gap))
}

/// Mints a `d` probe within ±64 days of the anchor (both gate sides,
/// the month/leap boundary classes of the band).
fn mint_probe(anchor: NaiveDate) -> NaiveDate {
    let offset: i16 = kani::any();
    kani::assume((-64..=64).contains(&offset));
    date_plus_days(anchor, i64::from(offset))
}

fn mint_weekday(index: u8) -> Weekday {
    match index {
        0 => Weekday::Mon,
        1 => Weekday::Tue,
        2 => Weekday::Wed,
        3 => Weekday::Thu,
        4 => Weekday::Fri,
        5 => Weekday::Sat,
        _ => Weekday::Sun,
    }
}

/// Daily `every`, parity 1..=30 — a symbolic tag selecting among
/// **concrete** values (wall 2 in the module header). The full
/// `u16` domain stays proptest-owned.
fn mint_every_days() -> EveryDays {
    let tag: u8 = kani::any();
    kani::assume((1..=30).contains(&tag));
    match tag {
        1 => mk_every_days(1),
        2 => mk_every_days(2),
        3 => mk_every_days(3),
        4 => mk_every_days(4),
        5 => mk_every_days(5),
        6 => mk_every_days(6),
        7 => mk_every_days(7),
        8 => mk_every_days(8),
        9 => mk_every_days(9),
        10 => mk_every_days(10),
        11 => mk_every_days(11),
        12 => mk_every_days(12),
        13 => mk_every_days(13),
        14 => mk_every_days(14),
        15 => mk_every_days(15),
        16 => mk_every_days(16),
        17 => mk_every_days(17),
        18 => mk_every_days(18),
        19 => mk_every_days(19),
        20 => mk_every_days(20),
        21 => mk_every_days(21),
        22 => mk_every_days(22),
        23 => mk_every_days(23),
        24 => mk_every_days(24),
        25 => mk_every_days(25),
        26 => mk_every_days(26),
        27 => mk_every_days(27),
        28 => mk_every_days(28),
        29 => mk_every_days(29),
        _ => mk_every_days(30),
    }
}

/// Totality helper for the concrete-tag mints (the tag table only
/// names constructor-valid values; the mint's own check is the guard).
fn mk_every_days(n: u16) -> EveryDays {
    EveryDays::new(n).expect("the tag table only names valid intervals")
}

/// Weekly `interval`, parity 1..=8, with the same concrete-case
/// split as [`mint_every_days`].
fn mint_every_weeks_parity() -> EveryWeeks {
    let tag: u8 = kani::any();
    kani::assume((1..=8).contains(&tag));
    match tag {
        1 => mk_every_weeks(1),
        2 => mk_every_weeks(2),
        3 => mk_every_weeks(3),
        4 => mk_every_weeks(4),
        5 => mk_every_weeks(5),
        6 => mk_every_weeks(6),
        7 => mk_every_weeks(7),
        _ => mk_every_weeks(8),
    }
}

fn mk_every_weeks(n: u16) -> EveryWeeks {
    EveryWeeks::new(n).expect("the tag table only names valid intervals")
}

/// Weekly `interval`, parity 1..=2 — the walk-harness sub-rung (the
/// 1..=8 band over the weekly week-walk was OOM-killed at the 6 GiB
/// cap at every mask shape; the walk length drives the circuit, and
/// the interval band multiplies it). The 1..=8 domain stays
/// proptest-owned.
fn mint_every_weeks_le2() -> EveryWeeks {
    let n: u8 = kani::any();
    kani::assume((1..=2).contains(&n));
    mk_every_weeks(u16::from(n))
}

fn mint_day_of_month() -> DayOfMonth {
    let d: u8 = kani::any();
    kani::assume((1..=31).contains(&d));
    DayOfMonth::new(u32::from(d)).expect("1..=31")
}

fn mint_nth_week() -> NthWeek {
    let n: u8 = kani::any();
    kani::assume((1..=5).contains(&n));
    NthWeek::new(u32::from(n)).expect("1..=5")
}

/// Harness-local specification of an `OnDay` member's day number:
/// `min(n, days-in-month)` with the days-in-month mirror written as an
/// *independent* calendar formulation (`checked_add_months` stepping
/// vs. the production `next_month`-plus-predecessor shape) — a drift
/// between the two is exactly what the clamp law would surface.
fn spec_days_in_month(year: i32, month: u32) -> u32 {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("a real month's first day");
    let next_month_first = first
        .checked_add_months(Months::new(1))
        .expect("an in-window month has a successor month");
    next_month_first
        .pred_opt()
        .expect("a first-of-month has a predecessor")
        .day()
}

/// Harness-local specification of the largest gap two consecutive
/// members (and the anchor → first member — the anchor may itself be
/// a member day) can span: Daily `every`; Weekly `7·interval`;
/// Monthly OnDay 31 (the clamp keeps a member in or adjacent to
/// every month); Monthly NthWeekday: **35** for k ≤ 4 (a k ≤ 4
/// weekday exists in every month; the longest member-to-member hop is
/// a Feb month boundary, 28 + 7) and **119** for k = 5.
///
/// **Triage record (plan §6.5)**: the stage-1 plan's constant (91,
/// "memberless Jan/Feb/Mar run") was falsified by a Kani
/// counterexample on 2026-11-04 (both hops of
/// `prop_next_after_chain_gap_bounded_monthly_nth_weekday`, inside
/// the documented band): 5th-weekday memberless runs reach four
/// months (the 30/31/31/28-month shape around a non-leap February,
/// e.g. Nov–Feb), and the true worst member-to-member hop is 119
/// days (e.g. a Aug 31st-X → Nov 29th-X hop around that run). The
/// corrected constants come from an independent brute-force
/// enumeration of every weekday × 40 years (2030–2070): k = 5 → 119,
/// k ≤ 4 → 35, maxima stable across all weekdays. **No production
/// bug**: `next_after`'s enumeration satisfies the corrected bound
/// (proven below); the drift was entirely in the spec derivation.
fn spec_max_gap(pattern: &RecurPattern) -> i64 {
    match pattern {
        RecurPattern::Daily { every } => i64::from(every.get()),
        RecurPattern::Weekly { interval, .. } => 7 * i64::from(interval.get()),
        RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(..),
        } => 31,
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(k, ..),
        } if k.get() == 5 => 119,
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(..),
        } => 35,
    }
}

// ---- Gate/totality family (R1a, plan §4.1 laws 1–2) ----
//
// Per arm: 1. **Totality/panic-freedom** of `occurs_on` over the
// arm's symbolic surface (implicit — every reachable overflow/assert
// on the symbolic path, incl. chrono internals); 2. **Anchor gate**:
// `d < anchor ⇒ !occurs_on(..)` over a ±64-day d band (the gate is
// order-only — `d < anchor` is the same fact at any distance; span
// reach stays proptest-owned). *Split from the plan §4.1's single
// muxed harness*: the 4-arm mux (and even a 3-arm one) was
// OOM-killed at the 6 GiB cap; the per-arm harnesses carry the same
// laws with the union covering all arms (wall 3, module header).
//
// Unwind: per arm below; doubling runs recorded in the stage-2
// report.

/// Daily arm of the gate/totality family.
///
/// Unwind 4: the Daily production path is loop-free; no mask loop,
/// no BTree on this arm. Doubling at 8 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_occurs_on_total_anchor_gated_daily_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Daily {
        every: mint_every_days(),
    };
    let d = mint_probe(anchor);
    if d < anchor {
        assert!(!occurs_on(&pattern, anchor, d));
    }
}

/// Weekly arm of the gate/totality family. The mask is the singleton
/// {weekday(anchor)}: the gate law is mask-blind (`d < anchor ⇒
/// !occurs_on` regardless of alignment), so the narrowing carries no
/// loss of strength for the law. Recorded descent: the symbolic 7-bit
/// mask over the weekly arm was OOM-killed at the 6 GiB cap twice
/// (bundled 4-arm shape, and per-arm) — the multi-day mask's set
/// shape stays proptest-owned (P1 samples masks 1..=127).
///
/// Unwind 16: the mint's mask-expansion loop (≤ 7 trips), the
/// BTreeSet walk/insert (≤ 8), the weekly alignment path — 16 ≥
/// every binding need. Doubling at 32 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(16)]
fn prop_occurs_on_total_anchor_gated_weekly_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Weekly {
        interval: mint_every_weeks_parity(),
        on: WeekdaySet::new([anchor.weekday()]).expect("a singleton weekday set is non-empty"),
    };
    let d = mint_probe(anchor);
    if d < anchor {
        assert!(!occurs_on(&pattern, anchor, d));
    }
}

/// Monthly-OnDay arm of the gate/totality family.
///
/// Unwind 8: the production path is loop-free; the bound covers the
/// BTree-class needs only. Doubling at 16 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_total_anchor_gated_monthly_onday_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(mint_day_of_month()),
    };
    let d = mint_probe(anchor);
    if d < anchor {
        assert!(!occurs_on(&pattern, anchor, d));
    }
}

/// Monthly-NthWeekday arm of the gate/totality family.
///
/// Unwind 8: the production path is loop-free; the bound covers the
/// BTree-class needs only. Doubling at 16 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_total_anchor_gated_monthly_nth_weekday_proven() {
    let anchor = mint_anchor();
    let weekday_index: u8 = kani::any();
    kani::assume(weekday_index <= 6);
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::NthWeekday(mint_nth_week(), mint_weekday(weekday_index)),
    };
    let d = mint_probe(anchor);
    if d < anchor {
        assert!(!occurs_on(&pattern, anchor, d));
    }
}

// ---- Anchor-membership family (R1b, plan §4.1 law 3) ----
//
// Per arm: the anchor is a member of its own *seeded* pattern — the
// chain laws' base case. Seeding is input construction from calendar
// arithmetic, never an assume of the property under test (B5): the
// Weekly seed mints the singleton mask {weekday(anchor)} — other
// mask members cannot participate in *anchor* membership, so the
// narrowing carries no loss of strength for this law (the full-mask
// weekly membership evidence rides the gate/totality weekly harness
// and the R2/R5 weekly harnesses over the full mask); OnDay takes
// the anchor's day; NthWeekday derives the k for which the anchor is
// its month's k-th weekday.

/// Daily arm: every interval aligns the anchor to itself.
///
/// Unwind 4 (loop-free arm). Doubling at 8 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_occurs_on_anchor_is_member_daily_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Daily {
        every: mint_every_days(),
    };
    assert!(occurs_on(&pattern, anchor, anchor));
}

/// Weekly arm: singleton seed over symbolic interval.
///
/// Unwind 16 (same derivation as the gate/totality weekly harness).
/// Doubling at 32 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(16)]
fn prop_occurs_on_anchor_is_member_weekly_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Weekly {
        interval: mint_every_weeks_parity(),
        on: WeekdaySet::new([anchor.weekday()]).expect("a singleton weekday set is non-empty"),
    };
    assert!(occurs_on(&pattern, anchor, anchor));
}

/// Monthly-OnDay arm: the anchor's own day as the clamp anchor.
///
/// Unwind 8. Doubling at 16 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_anchor_is_member_monthly_onday_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(
            DayOfMonth::new(u32::from(anchor.day())).expect("a real date's day is 1..=31"),
        ),
    };
    assert!(occurs_on(&pattern, anchor, anchor));
}

/// Monthly-NthWeekday arm: the anchor is its month's k-th weekday for
/// exactly one k in 1..=5 — derived, minted, then asserted.
///
/// Unwind 8. Doubling at 16 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_anchor_is_member_monthly_nth_weekday_proven() {
    let anchor = mint_anchor();
    let first = NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).expect("a real month");
    let offset =
        (anchor.weekday().num_days_from_monday() + 7 - first.weekday().num_days_from_monday()) % 7;
    let k = (anchor.day() - 1 - offset) / 7 + 1;
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::NthWeekday(
            NthWeek::new(u32::from(k)).expect("the anchor is the 1st..=5th weekday"),
            anchor.weekday(),
        ),
    };
    assert!(occurs_on(&pattern, anchor, anchor));
}

/// R1c — the OnDay clamp structure (plan §4.1 law 4, rides P3): an
/// OnDay member's day number is `min(n, days-in-month)`, against the
/// harness-local chrono-based `spec_days_in_month` mirror
/// (`checked_add_months` stepping vs. the production
/// `next_month`-plus-predecessor shape — a drift between the two is
/// exactly what this law surfaces).
///
/// Unwind 8: the Monthly-OnDay production path is loop-free; the
/// bound is kept uniform with the family for the doubling protocol.
/// Doubling at 16 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_monthly_onday_clamp_structure_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(mint_day_of_month()),
    };
    let probe = mint_anchor();
    if let RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(day),
    } = &pattern
        && occurs_on(&pattern, anchor, probe)
    {
        assert_eq!(
            probe.day(),
            u32::from(day.get()).min(spec_days_in_month(probe.year(), probe.month()))
        );
    }
}

// ---- Shared law bodies ----
//
// The per-arm harness families assert identical law sets; the bodies
// live here once (Sonar's new-code duplication gate flags repeated
// harness bodies). Kani's unwind bound covers loops in callees
// identically to inline bodies, so the proofs are unaffected.

/// The strict/floor family's banded `from` probe: a symbolic side
/// bool picks the forward band (0..=255 days past the anchor) or the
/// back band (1..=255 days before it) — input-domain minting only,
/// never an assume of a law (B5).
fn mint_banded_from(anchor: NaiveDate) -> (NaiveDate, bool) {
    let forward: bool = kani::any();
    if forward {
        let offset: u8 = kani::any();
        (date_plus_days(anchor, i64::from(offset)), false)
    } else {
        let offset: u8 = kani::any();
        kani::assume(offset >= 1);
        (date_plus_days(anchor, -i64::from(offset)), true)
    }
}

/// The strict/floor laws, shared verbatim by the four per-arm
/// harnesses: `from ≥ anchor ⇒ next > from` (strictness),
/// `from < anchor ⇒ next ≥ anchor` (the least-member floor), and
/// membership of `next` on both sides.
fn assert_next_after_strict_or_floor(
    pattern: &RecurPattern,
    anchor: NaiveDate,
    from: NaiveDate,
    floor_side_only: bool,
) {
    let next = next_after(pattern, anchor, from);
    if floor_side_only {
        assert!(next >= anchor);
    } else {
        assert!(next > from);
    }
    assert!(occurs_on(pattern, anchor, next));
}

/// The roll laws 1–3 shared by the roll-family harnesses: the head
/// moves forward; catch-up lands on the latest member ≤ today;
/// skip-ahead equals the stale head's next member. Returns the
/// target so an arm-specific law can assert on it without re-running
/// the circuit.
fn assert_roll_target_laws(
    pattern: &RecurPattern,
    anchor: NaiveDate,
    stale: NaiveDate,
    today: NaiveDate,
) -> NaiveDate {
    let target = roll_target(pattern, anchor, stale, today);
    assert!(target > stale);
    if target <= today {
        assert!(occurs_on(pattern, anchor, target));
        assert!(next_after(pattern, anchor, target) > today);
    } else {
        assert_eq!(target, next_after(pattern, anchor, stale));
    }
    target
}

/// The chain laws shared by the chain-family harnesses: two seeded
/// hops strictly advance, stay on the schedule, and never span more
/// than the pattern's maximal member gap.
fn assert_chain_gap_laws(pattern: &RecurPattern, anchor: NaiveDate) {
    let m1 = next_after(pattern, anchor, anchor);
    let m2 = next_after(pattern, anchor, m1);
    assert!(anchor < m1);
    assert!(m1 < m2);
    assert!(occurs_on(pattern, anchor, m1));
    assert!(occurs_on(pattern, anchor, m2));
    assert!((m1 - anchor).num_days() <= spec_max_gap(pattern));
    assert!((m2 - m1).num_days() <= spec_max_gap(pattern));
}

// ---- roll_target Daily family (R3, plan §4.3 = P4 Daily) ----

/// R3 — P4's Daily laws at the rung-A band:
///
/// 1. `target > stale` (the head always moves forward).
/// 2. `target ≤ today ⇒ occurs_on(..) ∧ next_after(target) > today`
///    (catch-up lands on the latest member ≤ today).
/// 3. `target > today ⇒ target == next_after(stale)` (skip-ahead).
/// 4. `Daily { every: 1 } ⇒ target == today`.
///
/// *Law 5 (determinism) lives in its own harness*
/// ([`prop_roll_target_daily_determinism`], recorded deviation): the
/// assertion duplicates the whole `roll_target` circuit — with it
/// bundled, this harness exceeded the 6 GiB cap (CBMC OOM); split
/// per B2's one-law-set rule, every law still proven, in smaller
/// circuits.
///
/// Unwind 4: both Daily paths are closed-form; no mask loop, no
/// BTree on this arm. Doubling at 8 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_roll_target_daily_laws_proven() {
    let anchor = mint_anchor();
    let every = mint_every_days();
    let pattern = RecurPattern::Daily { every };
    let stale = mint_stale(anchor);
    let today = mint_today(stale);

    let target = assert_roll_target_laws(&pattern, anchor, stale, today);

    if every.get() == 1 {
        assert_eq!(target, today);
    }
}

/// R3det — P4's determinism law, Daily arms, at the rung-A band:
/// `roll_target(pattern, anchor, stale, today)` called twice on
/// identical inputs gives identical targets — the no-hidden-state
/// claim, proven structurally by BMC over the duplicated circuit.
///
/// Unwind 4 (same derivation as R3). Doubling at 8 recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_roll_target_daily_determinism_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Daily {
        every: mint_every_days(),
    };
    let stale = mint_stale(anchor);
    let today = mint_today(stale);
    assert_eq!(
        roll_target(&pattern, anchor, stale, today),
        roll_target(&pattern, anchor, stale, today)
    );
}

// ---- roll_target Weekly/Monthly family (R4, plan §4.4 = P4 all
// arms) — per arm; the Daily arm is R3 above ----
//
// Laws per arm (R3's 1–3; law 4 is Daily-only, law 5 rides R3det —
// the all-arms bundled versions exceeded the 6 GiB cap, recorded
// deviation; purity is type-level: `roll_target` takes only `Copy`
// arguments and the domain crate holds no global state). Within the
// band every P4 case split is reachable for every arm (Daily gaps
// ≤ 30, weekly ≤ 56, monthly ≤ 31 ≤ the offset span); span reach
// stays proptest-owned (D-7's recorded disposition).

/// Monthly-OnDay arm of the roll family.
///
/// Unwind 8: the backward month-walk ≤ (120)/28 + 2 = 6 trips,
/// forward ≤ 2, no containers → 8 ≥ binding need. Doubling at 16
/// recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_roll_target_monthly_onday_laws_rung_a_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(mint_day_of_month()),
    };
    let stale = mint_stale(anchor);
    let today = mint_today(stale);
    assert_roll_target_laws(&pattern, anchor, stale, today);
}

// ---- next_after strictness/floor family (R2, plan §4.2 = P1 +
// its from<anchor sibling) — per arm, both sides via one symbolic
// bool ----

/// Daily arm of the strictness/floor family:
///
/// 1. `from ≥ anchor` side: `next > from` ∧ `occurs_on(.., next)`.
/// 2. `from < anchor` side: `next ≥ anchor` (least-member floor) ∧
///    membership.
/// 3. Totality/panic-freedom (implicit; the `expect`s in
///    `next_after` are unreachable inside the band — a path to one is
///    a real triage finding, plan §6).
///
/// Unwind 4 (loop-free arm). Doubling at 8 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_next_after_strict_member_daily_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Daily {
        every: mint_every_days(),
    };
    let (from, floor_side_only) = mint_banded_from(anchor);
    assert_next_after_strict_or_floor(&pattern, anchor, from, floor_side_only);
}

/// Weekly arm of the strictness/floor family. Same three laws. The
/// mask is the singleton {weekday(anchor)} and the interval band is
/// 1..=2 ([`mint_every_weeks_le2`]) — recorded descent ladder: the
/// full 7-bit symbolic mask was OOM-killed at the 6 GiB cap twice
/// (bundled shape, per-arm), the singleton at interval 1..=8 a third
/// time; the walk length × interval mux is the binding cost. The
/// within-week multi-candidate selection order and the 1..=8
/// interval band stay proptest-owned (P1); the weekly walk's
/// strictness/membership evidence is this sub-rung.
///
/// Unwind 8: forward week-walk ≤ interval(2) + 1 = 3 trips, weekday
/// scan ≤ 7, BTree ≤ 8 → 8 ≥ binding need. Bound-doubling run at 16
/// exceeded the 6 GiB cap (pilot H2 precedent: bound kept, cap
/// recorded, checklist re-attempt on a bigger runner — the stage-2
/// report carries the measurement).
#[kani::proof]
#[kani::unwind(8)]
fn prop_next_after_strict_member_weekly() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Weekly {
        interval: mint_every_weeks_le2(),
        on: WeekdaySet::new([anchor.weekday()]).expect("a singleton weekday set is non-empty"),
    };
    let (from, floor_side_only) = mint_banded_from(anchor);
    assert_next_after_strict_or_floor(&pattern, anchor, from, floor_side_only);
}

/// Monthly-OnDay arm of the strictness/floor family. Same three laws.
///
/// Unwind 8: month-walk ≤ 2 trips, no containers. Doubling at 16
/// recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_next_after_strict_member_monthly_onday_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(mint_day_of_month()),
    };
    let (from, floor_side_only) = mint_banded_from(anchor);
    assert_next_after_strict_or_floor(&pattern, anchor, from, floor_side_only);
}

/// Monthly-NthWeekday arm of the strictness/floor family. Same three
/// laws.
///
/// Unwind 8: month-walk ≤ 4 trips within the band (n ≤ 4 exists
/// everywhere; the band's stale reach cannot straddle a memberless
/// 3-month run), no containers. Doubling at 16 recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_next_after_strict_member_monthly_nth_weekday_proven() {
    let anchor = mint_anchor();
    let weekday_index: u8 = kani::any();
    kani::assume(weekday_index <= 6);
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::NthWeekday(mint_nth_week(), mint_weekday(weekday_index)),
    };
    let (from, floor_side_only) = mint_banded_from(anchor);
    assert_next_after_strict_or_floor(&pattern, anchor, from, floor_side_only);
}

// ---- chain family (R5, plan §4.5 = DR-6 §3.9 item 2, the bounded
// core; the full chain-enumeration law stays proptest-owned because
// `schedule` allocates a `Vec` and scans — B4-adjacent, D-8) ----
//
// Per arm: 1. Seed at the anchor: `anchor < m1 < m2` where
// `m1 = next_after(pattern, anchor, anchor)` and
// `m2 = next_after(pattern, anchor, m1)`; 2. membership of both
// hops; 3. the **no-skipped-member gap law**: `m1 − anchor ≤
// spec_max_gap(pattern)` ∧ `m2 − m1 ≤ spec_max_gap(pattern)` (the
// constants carry the §6.5 triage record in
// [`spec_max_gap`]). A counterexample there falsifies the
// derivation — that is the harness working (§6), not a nuisance.

/// Daily arm of the chain family.
///
/// Unwind 4 (loop-free arm). Doubling at 8 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_next_after_chain_gap_bounded_daily_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Daily {
        every: mint_every_days(),
    };
    assert_chain_gap_laws(&pattern, anchor);
}

/// Monthly-OnDay arm of the chain family.
///
/// Unwind 8 (month-walk ≤ 2 per hop). Doubling at 16 recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_next_after_chain_gap_bounded_monthly_onday_proven() {
    let anchor = mint_anchor();
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::OnDay(mint_day_of_month()),
    };
    assert_chain_gap_laws(&pattern, anchor);
}

/// Monthly-NthWeekday arm of the chain family — the gap law here is
/// the chunk's sharpest new statement (the corrected 119-day k = 5 /
/// 35-day k ≤ 4 bounds, [`spec_max_gap`]'s triage record).
///
/// Unwind 8 (month-walk ≤ 4 per hop). Doubling at 16 recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_next_after_chain_gap_bounded_monthly_nth_weekday_proven() {
    let anchor = mint_anchor();
    let weekday_index: u8 = kani::any();
    kani::assume(weekday_index <= 6);
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::NthWeekday(mint_nth_week(), mint_weekday(weekday_index)),
    };
    assert_chain_gap_laws(&pattern, anchor);
}

/// Monthly-NthWeekday k = 5 arm of the roll family — the arm whose
/// backward walk can skip memberless months. Recorded descent: split
/// out of the k-muxed harness after its OOM kill (see the k ≤ 4
/// sibling above).
///
/// Unwind 8: the backward month-walk ≤ 6 trips within the band,
/// forward ≤ 4 → 8 ≥ binding need. Bound-doubling run at 16 exceeded
/// the 6 GiB cap (pilot H2 precedent: bound kept, cap recorded,
/// checklist re-attempt on a bigger runner — the stage-2 report
/// carries the measurement).
#[kani::proof]
#[kani::unwind(8)]
fn prop_roll_target_monthly_nth5_weekday_laws_rung_a() {
    let anchor = mint_anchor();
    let weekday_index: u8 = kani::any();
    kani::assume(weekday_index <= 6);
    let pattern = RecurPattern::Monthly {
        on: MonthlyAnchor::NthWeekday(mint_nth_week5(), mint_weekday(weekday_index)),
    };
    let stale = mint_stale(anchor);
    let today = mint_today(stale);
    assert_roll_target_laws(&pattern, anchor, stale, today);
}

/// `NthWeek` fixed to the 5th — the memberless-month arm.
fn mint_nth_week5() -> NthWeek {
    NthWeek::new(5).expect("5 is a valid nth-week position")
}
