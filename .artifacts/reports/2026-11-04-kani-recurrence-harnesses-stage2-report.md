# [Report] Kani recurrence harnesses (#505) — stage 2 implementation (chunk 1.4)

- **Date**: 2026-11-04 · **Branch**: `test/kani-recurrence-harnesses`
  · **Worktree**: `../taskboard-WT-test-kani-recurrence-harnesses`
- **Plan**: `.artifacts/plans/2026-11-03-kani-recurrence-harnesses-plan.md`
  (stage 1) · **Normative**: issue #505 · **Pilot precedent**: PR #456
- **Commits**: `c7837fb3` (R3 first green, B8), `23ab1c5a`-series final
  harness set — `git log test/kani-recurrence-harnesses`
- **Diff scope**: `crates/taskboard-domain/src/verify.rs` (+856) and
  the stage-1 plan artefact — **zero production diffs** vs the merge
  base `396bd6e3` (`git diff 396bd6e3 HEAD --stat`). No CHANGELOG
  (nothing user-observable), no push, no PR (stage 3 ships).

## 1. Outcome

**Final CI-shaped run (`cargo kani -p taskboard-domain`, the exact
weekly-job command, under the B7 6 GiB guard): 23 successfully
verified harnesses, 0 failures** — the pilot's 3 domain harnesses
(H2-Rung-C / H3 / H4, regression duty per D-9) plus all 20 shipped
recurrence harnesses. Peak RSS of the whole suite: 5.9 GiB
(within the 6 GiB guard; the runner envelope check — peak ≤ 10 GiB —
passes with headroom).

## 2. Three measured BMC walls (and the descents they forced)

The stage-1 plan's cost model did not survive contact; every descent
below is recorded where it applies (harness doc comments) and
summarized here.

1. **Symbolic date value range, not circuit size, drives SAT cost.**
   The plan's D-4 date minting (symbolic `i32` CE day in the
   2000–2100 window) never converged: ~11 h wall / ~9 h CBMC CPU at
   unwind 4 on the smallest harness, no verdict, memory flat (solver
   phase). A ±90-day band verified in ~50 s. → **Rung A date
   surface**: every harness mints its anchor as a symbolic ±730-day
   offset around a concrete base (2050-06-15: every month boundary,
   every weekday, a year turn, the 2052-02-29 leap day) and bands
   offsets to `u8` (stale ≤ 64, gap 1..=56, from ≤ 64, probe ±64).
   Span reach stays proptest-owned.
2. **Symbolic 64-bit divisors.** The closed forms divide by the
   interval (`rem_euclid`/`div_euclid` on `i64`); a symbolic `u16`
   divisor never converged. → interval mints are symbolic tags
   selecting among **concrete** values (B5's case-split rule):
   Daily parity 1..=30, Weekly parity 1..=8 (walk harnesses 1..=2).
3. **Cross-arm muxes and duplicated circuits.** The 4-arm pattern
   mux OOM-killed at the 6 GiB cap in every bundled shape while each
   arm verifies alone in seconds-to-minutes → **per-arm harness
   families** (B2 one law-set per harness, one level deeper). The
   determinism law (a duplicated `roll_target` circuit) and the
   anchor-member seed (a second pattern mint) each OOM'd bundled →
   split out.

## 3. Verdict table (base bound; wall = Kani-reported verify time)

| Harness (family / arm) | Laws | Base unwind | Doubling | Wall |
|---|---|---|---|---|
| gate/totality · daily `_proven` | totality + anchor gate | 4 | 8 ✅ 13 s | 6 s |
| gate/totality · weekly `_proven` | 〃 | 16 | 32 ✅ 48 s | 27 s |
| gate/totality · monthly-onday `_proven` | 〃 | 8 | 16 ✅ 5 s | 5 s |
| gate/totality · monthly-nth `_proven` | 〃 | 8 | 16 ✅ 6 s | 6 s |
| anchor-member · daily `_proven` | anchor ∈ seeded pattern | 4 | 8 ✅ 7 s | 5 s |
| anchor-member · weekly `_proven` | 〃 | 16 | 32 ✅ 51 s | 28 s |
| anchor-member · monthly-onday `_proven` | 〃 | 8 | 16 ✅ 4 s | 4 s |
| anchor-member · monthly-nth `_proven` | 〃 | 8 | 16 ✅ 8 s | 8 s |
| OnDay clamp `_proven` (R1c) | member day = min(n, dim) vs mirror | 8 | 16 ✅ 8 s | 4 s |
| roll Daily `_proven` (R3) | P4 laws 1–4 | 4 | 8 ✅ 373 s | 239 s |
| roll Daily determinism `_proven` (R3det) | P4 law 5 | 4 | 8 ✅ 55 s | ~45 s |
| roll Monthly-OnDay `_proven` | P4 laws 1–3 | 8 | 16 ✅ 463 s | 125 s |
| roll Monthly-Nth **k = 5** | P4 laws 1–3 | 8 | **16 ⛔ 6 GiB cap** (pilot H2 precedent: bound kept, cap recorded) | 473 s |
| strict/floor · daily `_proven` | P1 + floor sibling | 4 | 8 ✅ 49 s | 44 s |
| strict/floor · weekly | P1 + floor sibling | 8 | **16 ⛔ 6 GiB cap** | 269 s |
| strict/floor · monthly-onday `_proven` | 〃 | 8 | 16 ✅ 55 s | 25 s |
| strict/floor · monthly-nth `_proven` | 〃 | 8 | 16 ✅ 156 s | 63 s |
| chain gap · daily `_proven` | 2-hop advance + gap law | 4 | 8 ✅ 52 s | 45 s |
| chain gap · monthly-onday `_proven` | 〃 | 8 | 16 ✅ 157 s | 62 s |
| chain gap · monthly-nth `_proven` | 〃 (corrected 119/35) | 8 | 16 ✅ 436 s | 126 s |

20 shipped harnesses: 18 doubled-green (`_proven` suffix per B2), 2
kept at base bound with a capped doubling (pilot H2 precedent —
checklist re-attempt on a bigger runner).

**Dropped harnesses (honest termination, B7/D-7; disposition in the
module header)**: weekly chain law (4 kills across every mask/interval
rung), weekly roll law (3+ kills), k ≤ 4 NthWeekday roll (2 kills —
case structure subsumed by the OnDay roll + the k = 5 skip-walk
sibling). Their full-range laws were proptest-owned from the start;
the sampled P1/P4 laws remain the owner.

## 4. Counterexample triage (plan §6) — one finding, harness-correction class

`prop_next_after_chain_gap_bounded_monthly_nth_weekday` FAILED with
counterexamples on **both** gap asserts: `m1 − anchor ≤ 91` and
`m2 − m1 ≤ 91` falsified inside the documented band (real dates,
`NthWeekday` 1..=5, all weekdays). Classification per §6.1: genuine
counterexample inside the domain, not an unwinding-assertion failure.

- **Mint check (§6.2)**: the counterexamples are inside the
  documented domain — the harness encodes §2.2 correctly; the
  *specification constant* was wrong. The stage-1 plan's "91 (worst:
  Jan/Mar 5th-X straddling a memberless February)" derivation is
  false: 5th-weekday memberless runs reach **four** months (the
  30/31/31/28 shape around a non-leap February, e.g. Nov–Feb).
- **Independent brute force** (Python, every weekday × 40 years,
  2030–2070): k = 5 → **119** days max member-to-member hop, k ≤ 4 →
  **35** days, stable across all weekdays.
- **Corrected spec** (`spec_max_gap`: 119 / 35 by k) **re-proven**:
  the same harness verifies at its base bound (0 of 1253) *and* at
  the doubled bound (0 of —, 436 s). `next_after`'s enumeration
  satisfies the corrected bound — **no production bug** (AGENTS §5
  both-sides check: the drift was entirely in the spec derivation;
  the strictness/floor family independently proves the enumeration
  properties).
- Per §6.5 the triage evidence is posted as a dated comment on issue
  #505.

## 5. Deviations from the stage-1 plan (each with recorded reason)

| Plan | Reality | Reason (measured) |
|---|---|---|
| D-4/§2.2 symbolic CE-day window | concrete base ± 730-day band; `u8` offsets | wall 1 (value range) |
| D-5 "full `u16` Daily domain, loop-free arm rides free" | concrete case-split parity 1..=30 | wall 2 (symbolic divisor) |
| §4 single muxed harnesses R1–R5 | per-arm families (R1 → 4+4+1, R2/R5 → 4 each, R4 → 3 arms) | wall 3 (mux multiplication, OOM ×2 per shape) |
| R1 unwind 4 derivation | 8 (mask loop is 7 trips — plan miscount) | B6 correction |
| R3 determinism inside R3 | separate harness | duplicated-circuit OOM |
| R4 rung ladder D-7 | sub-rungs per arm as documented; weekly roll dropped | OOM envelope |
| k ≤ 4 vs k = 5 split of the NthWeekday roll | split after 2 kills | OOM |

## 6. Normal-suite isolation (dispatch acceptance)

- `cargo nextest -p taskboard-domain`: **951/951 passed** — the
  `#[cfg(kani)]` module is invisible to the default build.
- Full gate suite via `scripts/run-quality-gates.sh`: see §7.
- `git diff 396bd6e3 HEAD --stat`: verify.rs + plan artefact only.
- kani.yml untouched: the Monday 04:00 UTC job runs the crate
  unfiltered, so the 20 harnesses ride it automatically. Every
  shipped harness was proven under the *local* 6 GiB guard at its
  shipped bound; the full-suite peak (5.9 GiB) fits the 16 GiB
  runner envelope. The two capped doublings are local-cap notes, not
  CI risks (CI runs base bounds).

## 7. Gates

`scripts/run-quality-gates.sh` (fmt, clippy `-D warnings`, nextest,
doc tests, deny, coverage triple): **GREEN** — full log in the
handoff notes; pre-commit (fmt/clippy/slint lint) green on both
commits.

## 8. Handoff

Stage 3: re-verify harnesses independently (pilot §8 shape), rebase
onto the fetched `origin/main` (it has drifted — merge base
`396bd6e3`), open the PR, watch the SonarCloud duplication gate (B9
mints are factored; the two spec mirrors are independent
formulations), and post/verify the #505 triage comment.
