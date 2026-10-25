# [Report] True-partial re-measure — refresh+draw bench after r2 D9 (chunk 7)

- **Date**: 2026-10-26 (runs 00:31–00:44 CEST)
- **Branch**: `docs/bench-true-partial-remeasure` (backlog-triage dispatch
  chunk 7; dispatch plan
  `.artifacts/plans/2026-10-25-backlog-triage-dispatch-plan.md` §8,
  stage-1 plan `.artifacts/plans/2026-10-25-bench-true-partial-remeasure-plan.md`)
- **Base commit**: `438aa13` (worktree HEAD; branch off fetched
  `origin/main` = `fc2c52d` + the stage-1 plan commit — plan D1)
- **Machine**: dev box (same as both baselines) — AMD Ryzen 9 5900HX,
  16 threads, 30 GiB, Linux 7.0.0-22-generic, `performance` governor
  (recorded, never changed), AC powered, rustc 1.99.0
  (b940084d7 2026-09-28), criterion 0.8.2.
- **Outcome (one line)**: the D9 row-diff push removed the ~97 %
  whole-model dirt — but the region instrument, as built, samples only
  the final iteration, which post-D9 is a *zero-change* iteration, so it
  reports `0×0` and the one-card partial region remains unmeasured; the
  chunk-6 cost inversion (partial ≥ full) resolves only at N=100; the
  refresh lane itself regressed 2–8.5× vs the 2026-10-20 baseline (real,
  reproduced at low load) — **#390's true-partial verdict is still
  unmeasured; recommended disposition: keep #390 open** (plan §6.2
  branch, not the §6.1 close branch).

Every number in this report is a **dev-machine reference, never a
gate**. The on-device (D250 kiosk) verdict belongs to the kiosk wave.
The 100 ms repaint cadence (C2/F13-09) floors composed latency
regardless.

## 1. Method (reproducible)

Commands (stage-1 plan §4 runbook, run verbatim in
`../taskboard-WT-docs-bench-true-partial-remeasure`):

```sh
# T1 — build evidence at the measured commit
git rev-parse HEAD            # 438aa13791b955d816c3e0eab045865b65176666
cargo bench --no-run -p taskboard-ui-slint --bench refresh_draw
# → Finished `bench` profile [optimized] target(s) in 2m 32s; only the
#   pre-existing .slint viewport-y deprecation warnings (plan §2)

# T2 — primary run, all 3 groups × 3 Ns, criterion defaults
cargo bench -p taskboard-ui-slint --bench refresh_draw -- --save-baseline true-partial \
    2>&1 | tee /tmp/refresh_draw_remeasure.log

# T3 — confirmation re-run, draw groups only (no --save-baseline)
cargo bench -p taskboard-ui-slint --bench refresh_draw -- draw \
    2>&1 | tee /tmp/refresh_draw_remeasure_confirm.log
```

Criterion configuration (identical methodology to both baselines): the
crate's defaults for N ∈ {100, 1 000} (sample size 100, 3 s warm-up,
5 s measurement); the bench's own `configure_group` tiering for
N = 10 000 (`sample_size(10)`, 15 s measurement). The partial lane
measures one render per iteration (`iter_batched`,
`BatchSize::PerIteration`); the toggle→store→refresh setup is
unmeasured. Medians quoted from criterion's `estimates.json` and the
run logs. One **extra confirmation run beyond the plan** (recorded
deviation, §7): `-- refresh` at a quiet box, to separate a suspected
load effect on the refresh lane from a real regression.

**Machine-state record (plan D5)**:

| Run | Wall clock | Load avg (1/5/15) | Concurrent load |
|---|---|---|---|
| T2 primary | 00:31 CEST | 8.42 / 8.47 / 8.32 | a `cargo kani` verification (`verify::prop_reminder_offset`) was running on this box (parallel dispatch chunk) |
| T3 confirm | 00:37 CEST | 0.81 / 3.50 / 6.10 | none (`pgrep cargo` clean; the Kani run had finished) |
| refresh confirm | 00:44 CEST | 0.95 / 2.61 / 5.39 | none |

T3's draw numbers match T2 within ~4 % despite the load delta, so the
concurrent Kani run did not materially distort the box; the refresh
lane was nevertheless re-confirmed at low load (see §2, Table 1 note).

**What the measured tree contains beyond the baselines** (plan §5
skeleton §1, risk 6 — honest-by-labelling):

- **The subject**: r2 D9 (`70c812d`) — stable model identity via
  row-diff push; `models::push` no longer replaces any `ModelRc`
  (persistent handles, positional `diff_rows`, `set_row_data` only
  where a row differs; zero notifications on an unchanged refresh).
- **Named background drift**: R3 chunks A/B/C + the in-app
  notification surface (`fc2c52d`, landed *after* the baselines;
  +661 lines under `crates/taskboard-ui-slint/`). The notification
  surface follows the same persistent-handle discipline and is empty
  on the bench fixture (no reminder rules in `build_state`), but its
  reconcile work is in the refresh pass.
- **Bench-code drift** (instrument structurally unchanged):
  `8015b6b` (+12 lines in `support/mod.rs`: new task fields, all
  `None`/pattern-fixed in the fixture) and `c0ece3e` (+3 lines in
  `refresh_draw.rs`: the `UiRequest` lane's mount-signature
  accommodation).

## 2. Fresh numbers (criterion medians [range], both runs)

**Table 1 — primary run (T2)**, 1366×768 deterministic fixture:

| Lane | N=100 | N=1 000 | N=10 000 |
|---|---|---|---|
| `refresh` (load → reconcile → diff-push) | 60.4 µs [59.95–60.90] | 807 µs [803–812] | 22.15 ms [21.68–22.56] |
| `draw-full` (NewBuffer) | 1.386 ms [1.374–1.400] | 2.324 ms [2.273–2.384] | 22.55 ms [22.25–23.01] |
| `draw-partial` (ReusedBuffer) | 125.0 µs [124.1–126.0] | 13.71 ms [11.67–15.74] | 155.2 ms [141.1–169.7] |
| partial region (W×H, share of frame) | 0×0 (0 %) | 0×0 (0 %) | 0×0 (0 %) |

Region lines, verbatim from the captured stderr:

```
[refresh_draw] draw-partial/100: region 0×0 (0 px = 0% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
[refresh_draw] draw-partial/1000: region 0×0 (0 px = 0% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
[refresh_draw] draw-partial/10000: region 0×0 (0 px = 0% of the full frame; 100% = the whole-VecModel-replacement dirt, the D6 recorded finding)
```

**Confirmation runs.** T3 (draw lanes, quiet box): draw-full
1.359 ms / 2.154 ms / 21.72 ms; draw-partial 118.8 µs / 13.36 ms /
150.2 ms; identical `0×0` region lines. The extra refresh-only run
(quiet box): 55.5 µs / 793 µs / 21.99 ms — within noise of T2, so the
refresh-lane reading below is **not** a load artefact. Run-to-run
spread across the two runs is ≤ 4 % on every cell except
draw-partial/1000 and /10000, whose wide criterion ranges
(11.7–15.7 ms; 141–170 ms) reflect the mixed iteration population
described next.

**The `0×0` region is an instrument-semantics finding, not a
zero-dirt verdict.** The region carrier (`refresh_draw.rs:219–247`)
holds only the *last* measured iteration's render. The setup toggles
`done = TOGGLE.get().then(…)` with `TOGGLE` alternating per iteration
and the fixture's `done` starting as a fixed pattern (every 10th task
done) — so every even-numbered iteration sets `done = None` on a task
that is (with 9/10 probability) already `None`: a **zero-change**
state. Pre-D9 that still dirtied ~97 % of the frame (whole `VecModel`
replacement). Post-D9 a zero-change refresh diffs to zero rows and —
correctly — dirties nothing, which is exactly what the last iteration
(even in count: 100, 100, 10) reports: `0×0`. The instrument therefore
now measures and *confirms* D9's unchanged-refresh-zero-dirt property,
but **cannot report the one-card-change partial region**; the partial
lane's *times* average over a mixed population of one-card-change and
zero-change iterations.

## 3. Comparison vs the baselines (plan D6 — no cherry-picking)

**Table 2 — primary comparison: 2026-10-20 chunk-6 stage-2 report §2**
(same box, closest methodology), with delta:

| Lane | N | 2026-10-20 | 2026-10-26 (T2) | Δ |
|---|---|---|---|---|
| refresh | 100 | 27.2 µs | 60.4 µs | +122 % |
| refresh | 1 000 | 229 µs | 807 µs | +252 % |
| refresh | 10 000 | 2.60 ms | 22.15 ms | +752 % |
| draw-full | 100 | 2.82 ms | 1.386 ms | −51 % |
| draw-full | 1 000 | 6.24 ms | 2.324 ms | −63 % |
| draw-full | 10 000 | 44.0 ms | 22.55 ms | −49 % |
| draw-partial | 100 | 21.9 ms | 125.0 µs | −99.4 % |
| draw-partial | 1 000 | 162.8 ms | 13.71 ms | −91.6 % |
| draw-partial | 10 000 | 1.57 s | 155.2 ms | −90.1 % |
| region | all N | 1351×760 ≈ 97 % | 0×0 (last-iter; see §2) | instrument semantics changed |

**Table 3 — variance envelope: 2026-10-21 dev references** (same box,
different day) and the fresh numbers against it:

| Lane | N | 2026-10-21 ref | 2026-10-26 (T2) | Inside envelope? |
|---|---|---|---|---|
| refresh | 100 | 30 µs | 60.4 µs | no (2.0×) |
| refresh | 1 000 | 247 µs | 807 µs | no (3.3×) |
| refresh | 10 000 | 5 150 µs | 22 150 µs | no (4.3×) |
| draw-full | 100 | 3.34 ms | 1.386 ms | no (0.41×) |
| draw-full | 1 000 | 8.61 ms | 2.324 ms | no (0.27×) |
| draw-full | 10 000 | 60.5 ms | 22.55 ms | no (0.37×) |
| draw-partial | 100 | 25.3 ms | 0.125 ms | no (0.005×) |
| draw-partial | 1 000 | 197.9 ms | 13.7 ms | no (0.069×) |
| draw-partial | 10 000 | 1 851 ms | 155.2 ms | no (0.084×) |

Reading: the *draw-full* lane moved by −49…−63 % vs the 2026-10-20
baseline — far outside the stage-3 re-run lesson's same-machine drift
yardstick (draw-full/10000 44.0 → 54.2 ms, +29 %, 2026-10-20 → the
direction here is *down*, by half). The markup family did not shrink
(the fixture is unchanged, and the structural full-frame warm-up guard
passes at every N), so the most plausible carriers are the shared
drift named in §1 (r2 round-2 markup, incl. D9 making unchanged row
rendering cheaper to traverse on the NewBuffer path) plus
compiler/rustc drift since 2026-10-20 (rustc 1.99.0 now). Absolute
levels moved; the *shape* (roughly linear in N, ~2.2 µs/card at
N=10 000) did not. No delta here is attributed to D9 alone.

**D250 slot-2 numbers (quoted context-only, plan D6)**: the 110.8 ms /
659.0 ms / 6 141 ms pre-D9 partial column is an upper bound under the
old whole-model dirt, measured on a different CPU (1.5 GHz in-order
i686 netbook). It is **not** a like-for-like delta target and was not
re-measured (plan D2: on-device session is a dispatch non-goal). The
one clean sentence it supports: the pre-D9 dirt penalty it embodies
(3.6× the full-frame cost at N=100) has been removed at the source —
the dev partial lane now sits at 0.09× of full-frame at N=100.

## 4. Verdict — perf-budget row 3, partial leg (plan D7/D8)

**(a) Region sub-verdict.** The ~97 %-of-frame whole-model dirt is
gone — that much is certain from both the region lines (zero-frame
dirt on a zero-change iteration; pre-D9 the *same* zero-change
iteration reported 1351×760) and the partial-lane cost collapse
(−90…−99 %). But the instrument, as built, samples the last iteration
and post-D9 that iteration is zero-change: **the one-card-change
partial region — the actual row-3 partial-leg instrument — was not
reported by this run.** This outcome sits outside the plan's D8
region decision table (which assumed a card-scale positive region);
per the plan's own framing it is reported as a finding, not squeezed
into a branch.

**(b) Cost-ordering sub-verdict.** The chunk-6 inversion (partial ≥
full) **resolves only at N=100** (125 µs vs 1.39 ms — partial is now
11× cheaper than full) and **persists at scale**: 13.7 ms vs 2.32 ms
at N=1 000, 155 ms vs 22.6 ms at N=10 000. A ReusedBuffer render that
touches one card still costs more than redrawing every card into a
fresh buffer at board scales ≥ 1 000 — the dirty-region *determination*
over ~N live items, not the dirty *paint*, is the dominant residual
cost. Mechanism hypothesis (plan D8-b's candidate, narrowed): per-item
damage tracking over an unvirtualised board traverses all N items per
partial render; a virtualised board (#373) removes exactly that term.

**(c) Budget-leg sub-verdict.** Budget row 3's partial leg (≤ 20 ms at
1366×768, software renderer, 1.5 GHz in-order kiosk core): **still
without a valid measured verdict.** The dev partial numbers straddle
the envelope in a scale-dependent way (13.7 ms at N=1 000 inside;
155 ms at N=10 000 far outside), the true one-card region was not
captured, and dev numbers are references only — the on-device verdict
belongs to the kiosk wave. The refresh lane is reported but asserts
nothing against a budget (row 3 speaks about repaint).

House caveats, verbatim from the plan: dev-machine **references,
never gates**; the on-device verdict belongs to the kiosk wave; the
100 ms repaint cadence (C2/F13-09) floors composed latency regardless.

**Additional finding (in scope for #390/#373 readers): the refresh
lane regressed 2–8.5× vs the 2026-10-20 baseline** (Table 2) and is
outside the 2026-10-21 envelope (Table 3), reproduced at load ≈ 1
(extra run, §2) — so it is code, not the concurrent Kani run. The
per-row diff pass does strictly more work than the old pointer
replacement, and the r2 surface grew the reconciled tree; the
super-linear step between N=1 000 and N=10 000 (0.79 → 22 ms, 28× for
10× the rows) suggests the diff is not a flat linear term. Whether
that is acceptable at kiosk scales, or worth a targeted look
(e.g. per-column short-circuiting when only one column changed), is a
separate question this report records but does not decide.

## 5. What survives of #373's motivation (plan D9 — decision-grade)

The question: analyse `ListView` virtualisation for board columns,
implement if viable. The re-measure changes the evidence under that
ask, not the ask.

**Options.**

1. **Analyse ListView now** (as #373 asks).
   *For*: the surviving motivation is strong and got *sharper*: D9
   removed the dirt argument for the status quo, but eager
   instantiation of all N cards remains, and the fresh numbers show
   its two costs precisely — full-frame draw scales with board size
   (22.6 ms at N=10 000 even on a desktop core; the ≤ 50 ms full-frame
   envelope has no headroom on a 1.5 GHz kiosk core), and the
   partial-lane residual (155 ms at N=10 000) is dominated by
   per-item damage tracking over all live items — exactly the term
   virtualisation deletes. Both budget-row-3 lanes would move toward
   viewport scale. *Against*: the four known trade-offs (#373 Ask 1:
   focus walk, index→id mapping, scroll semantics across refreshes,
   the bench contract) are real design work; and the kiosk wave's
   on-device numbers are still the decider for whether any of this is
   user-visible.
2. **Defer until the on-device kiosk numbers land.**
   *For*: cheap; the D250 session gives the real budget picture.
   *Against*: the on-device session measures the *status quo*; it
   cannot bound the virtualisation win, and the dev numbers already
   show the 10 k-tier full-frame cost is ~an order of magnitude from
   comfortable on kiosk-class silicon. Deferral risks the analysis
   (a design round, not implementation) simply being re-asked later.
3. **Drop the analysis.** Not defensible: both fresh cost signals
   (full-frame board-scale draw; partial damage-tracking residual)
   point at eager instantiation as the dominant remaining term.

**Recommendation: option 1 — run the ListView analysis design round
now, as #373 asks.** The re-measure strengthened the case: the dirt
half of the render-budget problem is fixed (D9), and what remains —
board-scale eager instantiation on both draw lanes — is precisely the
half virtualisation addresses. Keep the analysis as a design round
(adjudicate the four listed trade-offs; keep an unvirtualised bench
lane), with *implementation* gated on the kiosk wave's on-device
verdict. Separately (not #373's scope): the refresh-lane regression
(§4 finding) deserves its own targeted look before any kiosk-scale
deployment.

## 6. Issue updates applied

- **#390**: comment posted with the fresh numbers and the recommended
  disposition **keep open** (plan §6.2 branch — the true-partial
  verdict row 3 asks for is still unmeasured; §6.1's close branch did
  not apply because the region instrument's last-iteration semantics
  post-D9 report the zero-change case, not the one-card partial):
  <https://github.com/D4id4los/taskboard/issues/390#issuecomment-6405048920>.
- **#373**: body edited per plan §7 — the stale pre-D9 lane table and
  the "97 % dirty region" finding are marked superseded and pointed at
  this report; the analysis ask (Ask 1 verbatim, Ask 2's instrument
  refreshed with the post-D9 baseline). Issue stays open.
- Neither issue is closed; disposition happens at stage 3 / by the
  user (dispatch §8, stage-2 rule 3).

## 7. Deviations from the stage-1 plan

1. **One extra bench invocation beyond the T2/T3 runbook**: a
   refresh-only confirmation run (`-- refresh`) at load ≈ 1. Reason:
   T2 ran under a concurrent `cargo kani` verification (load 8.4), and
   the refresh lane read 2–8.5× baseline — the extra run separates
   load effect from real regression (it confirmed: real). T3's
   draw-only filter (plan D3) was extended by this one run only.
2. **The D8 decision table did not anticipate the observed region
   outcome.** All three branches assumed a positive region
   (card-scale / 10–50 % / ≥ 50 %); the run reported `0×0` because the
   instrument samples the last iteration, which post-D9 is a
   zero-change state. Per the plan's own framing (D8 pre-commits
   dispositions, no post-hoc wording), the disposition falls to the
   conservative reading: **keep #390 open** (the §6.2 branch), not the
   §6.1 close branch. No code was touched to change the instrument
   (freeze rule).
3. No other deviations. T1 green at the measured commit; no
   `--save-baseline` overwrite risk taken (T3/T4 without the flag);
   temp logs kept in `/tmp`, never committed.

## 8. Acceptance checklist (stage-1 plan §8, stage-2 items)

- [x] T1 build green in the worktree at the measured commit
      (`438aa13`, `Finished bench profile … in 2m 32s`).
- [x] Machine-state record captured (§1 table); primary run +
      confirmation re-run (+ one recorded extra refresh run); region
      lines transcribed verbatim (§2).
- [x] Report written per the §5 skeleton; no unresolved placeholders;
      every number labelled a dev-machine reference.
- [x] #390 commented (§6.2-branch text matching the D8 outcome);
      #373 body edited per §7; no issue closed.
- [x] `git status` clean except the report file; fmt canary green;
      single standalone `docs(report)` commit; no push.
