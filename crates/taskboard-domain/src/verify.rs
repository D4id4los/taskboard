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
// **Rung A date surface (measured, recorded deviation from the plan's
// §2.2 domain table).** The binding BMC cost for this arithmetic is
// the *value range* of symbolic dates, not circuit size: a
// full-2000–2100-window Daily harness with full-parity offsets did
// not converge in ~11 h wall / ~9 h CBMC CPU at unwind 4 (and two
// harnesses bundling two symbolic pattern mints were OOM-killed at
// the 6 GiB cap), while a ±90-day band verifies in ~50 s and the
// ±730-day band in minutes (stage-2 report, measurement table).
// Every harness below therefore mints its anchor as a **symbolic
// ±730-day offset around a concrete base** (2050-06-15: the band
// sweeps 731 anchor positions covering every month boundary, every
// weekday, a year turn, and the 2052-02-29 leap day), and bands its
// stale/gap/from offsets to `u8` (≤ 64 / 1..=56 / ≤ 64) — the plan's
// own R4 Rung-A values, applied chunk-wide. Within the band every
// case split of every law is exhaustively covered (catch-up vs
// skip-ahead, clamped Feb, memberless-month walks); what is *lost*
// vs the proptests is span reach (20 000/2 000-day gaps, the full
// 101-year window), and the full-range laws stay proptest-owned
// (P1–P4, 256 samples each) — the pilot's Rung-D disposition shape,
// now documented at each harness (B5's honest-claim rule).

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

/// `base` advanced by a concrete signed day count (the harness-side
/// date arithmetic wrapper; representability is by construction on
/// the band surfaces every harness uses).
fn date_plus_days(base: NaiveDate, days: i64) -> NaiveDate {
    base.checked_add_signed(TimeDelta::try_days(days).expect("day delta"))
        .expect("a banded offset stays representable")
}

/// Mints the symbolic anchor: recur_base() ± 730 days, full band, no
/// assume beyond the `i16` band itself (the band *is* the documented
/// rung, not a silent narrowing — see the module header).
fn mint_anchor() -> NaiveDate {
    let offset: i16 = kani::any();
    kani::assume((-730..=730).contains(&offset));
    date_plus_days(recur_base(), i64::from(offset))
}

/// Mints a `stale` reference at proptest parity structure: ≥ anchor,
/// ≤ 64 days past it (`u8` full type range is 0..=255, so this one
/// assume is the band).
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

/// Mints the weekday set from a 7-bit mask (bits 0..=6 = Mon..Sun);
/// the non-empty assume is the mint's own constructor validity, and
/// `WeekdaySet::new` cannot fail over it.
fn mint_weekday_set(mask: u8) -> WeekdaySet {
    kani::assume(mask & 0b1000_0000 == 0);
    kani::assume(mask != 0);
    let days: Vec<Weekday> = (0..7)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| mint_weekday(bit))
        .collect();
    WeekdaySet::new(days).expect("a non-empty mask mints a non-empty set")
}

/// Daily `every`, parity 1..=30 — a symbolic tag selects among
/// **concrete** values (B5's case-split rule). The recurrence closed
/// forms divide by the interval (`rem_euclid`/`div_euclid` on `i64`),
/// and a *symbolic* 64-bit divisor is a measured BMC wall (the
/// original full-`u16` symbolic `every` run never converged; stage-2
/// report). Concrete arms keep each branch's division
/// constant-folded; the full-`u16` law stays proptest-owned.
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

/// Weekly `interval`, parity 1..=8, with the same concrete-case split
/// as [`mint_every_days`] (the weekly `rem_euclid(interval)` has the
/// same symbolic-divisor wall).
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

/// Mints a fully symbolic `RecurPattern` over one `u8` tag (0 Daily,
/// 1 Weekly, 2 Monthly-OnDay, 3 Monthly-NthWeekday), per-arm fields
/// per the rung-A surface (module header).
fn mint_pattern() -> RecurPattern {
    let tag: u8 = kani::any();
    kani::assume(tag <= 3);
    match tag {
        0 => RecurPattern::Daily {
            every: mint_every_days(),
        },
        1 => RecurPattern::Weekly {
            interval: mint_every_weeks_parity(),
            on: mint_weekday_set(kani::any()),
        },
        2 => RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(mint_day_of_month()),
        },
        _ => {
            let weekday_index: u8 = kani::any();
            kani::assume(weekday_index <= 6);
            RecurPattern::Monthly {
                on: MonthlyAnchor::NthWeekday(mint_nth_week(), mint_weekday(weekday_index)),
            }
        }
    }
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

/// Mints a pattern *seeded at the anchor* — the chain laws' base case,
/// where `occurs_on(pattern, anchor, anchor)` must hold. Weekly gets
/// the anchor's weekday OR-ed into the symbolic mask; OnDay takes the
/// anchor's own day; NthWeekday derives the k for which the anchor is
/// the k-th weekday of its month (input construction from calendar
/// arithmetic — never an assume of the property under test, B5).
fn mint_anchor_member_pattern(anchor: NaiveDate) -> RecurPattern {
    let tag: u8 = kani::any();
    kani::assume(tag <= 3);
    match tag {
        0 => RecurPattern::Daily {
            every: mint_every_days(),
        },
        1 => {
            let extra: u8 = kani::any();
            let anchor_bit = 1u8 << anchor.weekday().num_days_from_monday();
            RecurPattern::Weekly {
                interval: mint_every_weeks_parity(),
                on: mint_weekday_set(extra | anchor_bit),
            }
        }
        2 => RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(
                DayOfMonth::new(u32::from(anchor.day()))
                    .expect("a real date's day number is 1..=31"),
            ),
        },
        _ => {
            let first =
                NaiveDate::from_ymd_opt(anchor.year(), anchor.month(), 1).expect("a real month");
            let offset = (anchor.weekday().num_days_from_monday() + 7
                - first.weekday().num_days_from_monday())
                % 7;
            let k = (anchor.day() - 1 - offset) / 7 + 1;
            RecurPattern::Monthly {
                on: MonthlyAnchor::NthWeekday(
                    NthWeek::new(u32::from(k)).expect("the anchor is the 1st..=5th weekday"),
                    anchor.weekday(),
                ),
            }
        }
    }
}

/// Harness-local specification of the largest gap two consecutive
/// members (and the anchor → first member) can span, per arm:
/// Daily `every`; Weekly `7·interval`; Monthly OnDay 31 (the clamp
/// keeps a member in or adjacent to every month); Monthly NthWeekday
/// 91 (the memberless Jan/Feb/Mar run: (31−31)+31+28+30-day and
/// (31−30)+31+29+30-day walks both top out at 91 with member days
/// ≥ 29). A counterexample here falsifies the derivation — that is
/// the harness working (plan §6), not a nuisance.
fn spec_max_gap(pattern: &RecurPattern) -> i64 {
    match pattern {
        RecurPattern::Daily { every } => i64::from(every.get()),
        RecurPattern::Weekly { interval, .. } => 7 * i64::from(interval.get()),
        RecurPattern::Monthly {
            on: MonthlyAnchor::OnDay(..),
        } => 31,
        RecurPattern::Monthly {
            on: MonthlyAnchor::NthWeekday(..),
        } => 91,
    }
}

/// R1a — `occurs_on` totality + anchor gate, exhaustive within the
/// rung-A band (plan §4.1 laws 1–2):
///
/// 1. **Totality/panic-freedom** over the fully symbolic pattern
///    (implicit — every reachable overflow/assert on the symbolic
///    path, incl. chrono internals).
/// 2. **Anchor gate**: `d < anchor ⇒ !occurs_on(..)`, over a ±255-day
///    `i16` d-offset (the gate is order-only; every path pair-class
///    fits inside the band).
///
/// *Split from the plan §4.1's single R1 harness* (recorded
/// deviation): the bundled version minted two symbolic patterns in
/// one circuit and was OOM-killed twice at the 6 GiB cap — the
/// two-kill rule (B7) descends by law-scope, per the pilot's
/// containment convention; B2's one-law-set rule restored.
///
/// Unwind 8: the production path is loop-free; the binding need is
/// the mint's 7-bit mask-expansion loop (7 trips) and the BTreeSet
/// walk/insert (≤ 8). (The plan §4.1's original "4" derivation
/// miscounted the 7-trip mask loop; corrected per B6.) Doubling run:
/// `#[kani::unwind(16)]`, identical verdict — recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_total_and_anchor_gated() {
    let anchor = mint_anchor();
    let pattern = mint_pattern();
    let d_offset: i16 = kani::any();
    let d = date_plus_days(anchor, i64::from(d_offset));
    if d < anchor {
        assert!(!occurs_on(&pattern, anchor, d));
    }
}

/// R1b — the anchor-membership law (plan §4.1 law 3): the anchor is a
/// member of its own *seeded* pattern, for every arm — the chain
/// laws' base case. The seed is input construction from calendar
/// arithmetic (Weekly OR-s the anchor's weekday into the symbolic
/// mask; OnDay takes the anchor's day; NthWeekday derives the k for
/// which the anchor is its month's k-th weekday) — never an assume of
/// the property under test (B5).
///
/// Unwind 8: same derivation as R1a. Doubling run at 16: identical
/// verdict recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_occurs_on_anchor_is_member() {
    let anchor = mint_anchor();
    let seeded = mint_anchor_member_pattern(anchor);
    assert!(occurs_on(&seeded, anchor, anchor));
}

/// R1c — the OnDay clamp structure (plan §4.1 law 4, rides P3): an
/// OnDay member's day number is `min(n, days-in-month)`, against the
/// harness-local chrono-based `spec_days_in_month` mirror
/// (`checked_add_months` stepping vs. the production
/// `next_month`-plus-predecessor shape — a drift between the two is
/// exactly what this law surfaces).
///
/// Unwind 8: the Monthly-OnDay production path is loop-free; the
/// bound is kept uniform with R1a/R1b for the doubling protocol (no
/// mask loop on this arm). Doubling run at 16 recorded in the
/// stage-2 report.
#[kani::proof]
#[kani::unwind(8)]
fn prop_monthly_onday_clamp_structure() {
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

/// R3 — escalates P4's Daily laws to exhaustive-within-bounds at the
/// rung-A band (plan §4.3; Daily is the one pattern with loop-free
/// `next_after` *and* `prev_on_or_before`):
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
/// Symbolic surface: rung-A band anchor; `every` parity 1..=30 via
/// concrete case-split; stale ≤ 64 d; gap 1..=56 d.
///
/// Unwind 4: both Daily paths are closed-form; no mask loop, no
/// BTree on this arm. Doubling run at 8 recorded in the stage-2
/// report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_roll_target_daily_laws() {
    let anchor = mint_anchor();
    let every = mint_every_days();
    let pattern = RecurPattern::Daily { every };
    let stale = mint_stale(anchor);
    let today = mint_today(stale);

    let target = roll_target(&pattern, anchor, stale, today);

    assert!(target > stale);
    if target <= today {
        assert!(occurs_on(&pattern, anchor, target));
        assert!(next_after(&pattern, anchor, target) > today);
    } else {
        assert_eq!(target, next_after(&pattern, anchor, stale));
    }
    if every.get() == 1 {
        assert_eq!(target, today);
    }
}

/// R3det — P4's determinism law, Daily arms, at the rung-A band:
/// `roll_target(pattern, anchor, stale, today)` called twice on
/// identical inputs gives identical targets — the no-hidden-state
/// claim, proven structurally by BMC over the duplicated circuit.
///
/// Unwind 4: same derivation as [`prop_roll_target_daily_laws`].
/// Doubling run at 8 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(4)]
fn prop_roll_target_daily_determinism() {
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

/// R4 — the P4 law-set of R3 over **all four arms** at the rung-A
/// band (plan §4.4, D-7). Within the band every P4 case split is
/// reachable for every arm (Daily gaps ≤ 30, weekly ≤ 56, monthly
/// ≤ 31 ≤ the offset span) — the case structure is exhaustively
/// covered; what is lost vs P4 is span reach. The full-range law
/// stays proptest-owned regardless of this harness's outcome
/// (recorded disposition, D-7).
///
/// Laws: R3's 1–3 (the `every == 1 ⇒ target == today` law is
/// Daily-only and lives in R3; the determinism law is Daily-only
/// too — the all-arms bundled version exceeds the 6 GiB cap, recorded
/// deviation; purity is type-level: `roll_target` takes only `Copy`
/// arguments and no global state exists in the domain crate).
///
/// Unwind 20: weekly backward week-walk ≤ (64+56)/7 ÷ interval(1)
/// + 1 = 18 trips (binding); monthly backward month-walk ≤
/// (120)/28 + 2 = 6; forward walks ≤ interval(8) + 1 = 9; mint ≤ 8 →
/// 20 ≥ binding need 18 + 2 slack. Doubling run at 40 recorded in
/// the stage-2 report. Resource ladder: a second kill at this rung
/// records the envelope and stops (B7/D-7) — no cap-raising.
#[kani::proof]
#[kani::unwind(20)]
fn prop_roll_target_all_arms_laws_rung_a() {
    let anchor = mint_anchor();
    let pattern = mint_pattern();
    let stale = mint_stale(anchor);
    let today = mint_today(stale);

    let target = roll_target(&pattern, anchor, stale, today);

    assert!(target > stale);
    if target <= today {
        assert!(occurs_on(&pattern, anchor, target));
        assert!(next_after(&pattern, anchor, target) > today);
    } else {
        assert_eq!(target, next_after(&pattern, anchor, stale));
    }
}

/// R2 — escalates P1 and its `from < anchor` sibling to
/// exhaustive-within-bounds, both sides in one harness (plan §4.2;
/// bundled per the pilot's H3 multi-law precedent):
///
/// 1. `from ≥ anchor` side: `next > from` (strictness) ∧
///    `occurs_on(pattern, anchor, next)` (membership).
/// 2. `from < anchor` side: `next ≥ anchor` (the least-member floor)
///    ∧ membership.
/// 3. **Totality/panic-freedom** across all four arms (implicit; the
///    `expect`s in `next_after` are unreachable inside the band — a
///    path to one is a real triage finding, plan §6).
///
/// Symbolic surface: rung-A band anchor; fully symbolic pattern;
/// forward offset 0..=64 / back offset 1..=64 (`u8` — the plan's
/// 40 000/4 000-day parities are span reach and stay proptest-owned,
/// module header).
///
/// Unwind 16: weekly outer week-walk ≤ interval(8) + 1 = 9 trips
/// (the first aligned week index is within `interval − 1` of the
/// start); weekly inner weekday scan ≤ 7; Monthly OnDay ≤ 2; Monthly
/// NthWeekday ≤ 4 (n ≤ 4 exists everywhere, n = 5 misses at most a
/// 3-month run); mint/BTree ≤ 8 → 16 ≥ every binding need. Doubling
/// run at 32 recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(16)]
fn prop_next_after_strict_after_member_and_anchor_floor() {
    let anchor = mint_anchor();
    let pattern = mint_pattern();
    let forward: bool = kani::any();
    let (from, floor_side_only) = if forward {
        let offset: u8 = kani::any();
        (date_plus_days(anchor, i64::from(offset)), false)
    } else {
        let offset: u8 = kani::any();
        kani::assume(offset >= 1);
        (date_plus_days(anchor, -i64::from(offset)), true)
    };

    let next = next_after(&pattern, anchor, from);

    if floor_side_only {
        assert!(next >= anchor);
    } else {
        assert!(next > from);
    }
    assert!(occurs_on(&pattern, anchor, next));
}

/// R5 — escalates DR-6 §3.9 item 2 (repeated `next_after` enumerates
/// the schedule) in its bounded core (plan §4.5; the full
/// chain-enumeration law stays proptest-owned because `schedule`
/// allocates a `Vec` and scans — B4-adjacent, recorded D-8):
///
/// 1. Seed at the anchor: `anchor < m1 < m2` where
///    `m1 = next_after(pattern, anchor, anchor)` and
///    `m2 = next_after(pattern, anchor, m1)` — strict advance over
///    two hops (repeated application terminates and moves forward).
/// 2. `occurs_on(pattern, anchor, m1)` ∧ `occurs_on(.., m2)` — the
///    enumeration stays on the schedule.
/// 3. **No-skipped-member gap law** (the claim sampling cannot
///    support): `m1 − anchor ≤ spec_max_gap(pattern)` ∧
///    `m2 − m1 ≤ spec_max_gap(pattern)`.
///
/// Symbolic surface: rung-A band anchor; fully symbolic pattern; no
/// `from` symbol (seeded at the anchor).
///
/// Unwind 16: the weekly/monthly forward-walk derivation as R2
/// (weekly ≤ 9 each), ×2 hops (CBMC unwinds each loop instance
/// separately) — 16 ≥ binding need 9 with slack. Doubling run at 32
/// recorded in the stage-2 report.
#[kani::proof]
#[kani::unwind(16)]
fn prop_next_after_chain_gap_bounded_members() {
    let anchor = mint_anchor();
    let pattern = mint_pattern();

    let m1 = next_after(&pattern, anchor, anchor);
    let m2 = next_after(&pattern, anchor, m1);

    assert!(anchor < m1);
    assert!(m1 < m2);
    assert!(occurs_on(&pattern, anchor, m1));
    assert!(occurs_on(&pattern, anchor, m2));
    assert!((m1 - anchor).num_days() <= spec_max_gap(&pattern));
    assert!((m2 - m1).num_days() <= spec_max_gap(&pattern));
}
