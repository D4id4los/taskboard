# [Report] Miri Cap Raise Probe — chunk P8, parallel-during-gui dispatch

- **Date**: 2026-10-16
- **Branch**: `ci/miri-cap-probe` @ `f53b379` (worktree
  `../taskboard-WT-ci-miri-cap-probe`; stage 3 ships the PR, never
  merges).
- **Deliverable**: measured cap probe per
  `.artifacts/plans/2026-10-15-ci-miri-cap-probe-plan.md` (stage 1).
- **Final config** (`miri.yml`): domain core 16, domain `pipeline::`
  **2** (raised from 1), domain remote/state properties 16,
  **state 128** (raised from 16). Zero test-file changes; diff is
  `miri.yml` + `testing_strategy.org` §10 + backlog only.

## Probe log (candidate → run URL → timings → verdict)

Walls are per-job `started_at → completed_at` from the Actions API.
Windows: a *day window* (fast) and a *night window* (≈ 1.5–2× slower)
appeared within hours of each other; variance ended up dominating
every cap decision.

### Baselines (plan §1, no dispatch spent)

| job (cap) | run 38312493624 (10-12) | run 38786976628 (10-15) |
|---|---|---|
| core (16) | 55 m 18 s | 1 h 21 m 10 s |
| pipeline (1) | 24 m 26 s | 1 h 12 m 10 s |
| props (16) | 20 m 42 s | 17 m 59 s |
| state (16) | 8 m 10 s | 13 m 00 s |

### Rung 1 — state 64 + pipeline 2 (bundled), run [38804930006](https://github.com/D4id4los/taskboard/actions/runs/38804930006)

pipeline (2) 58 m 48 s · state (64) 39 m 16 s · core (ctrl) 1 h 18 m ·
props (ctrl) 14 m 41 s. Both candidates ≤ 90 → rung 2 triggered
(state ≤ ~68 min headroom gate).

### Rung 2 — state 128, run [38814917035](https://github.com/D4id4los/taskboard/actions/runs/38814917035)

state (128) 71 m 34 s · pipeline (2) 1 h 44 m 41 s · core 1 h 22 m ·
props 14 m 35 s.

### Final-SHA runs

1. [38826477952](https://github.com/D4id4los/taskboard/actions/runs/38826477952)
   — green; pipeline (2) 1 h 45 m, state (128) 91 m, core 68 m,
   props 14 m (night window).
2. [38836835837](https://github.com/D4id4los/taskboard/actions/runs/38836835837)
   — state job died at 1 m on a crates.io download flake (`HTTP2
   framing layer`); transient infra, re-dispatched same SHA. Other
   three green; pipeline 1 h 51 m (slowest observed).
3. [38846463650](https://github.com/D4id4los/taskboard/actions/runs/38846463650)
   — **the acceptance run**: all four jobs green and ≤ 90 —
   pipeline (2) 55 m 03 s, state (128) 71 m 39 s, core 66 m 09 s,
   props 15 m 38 s.
4. [38853294730](https://github.com/D4id4los/taskboard/actions/runs/38853294730)
   — green; pipeline 1 h 42 m, state 93 m, core 46 m, props 14 m.
5. [38862752532](https://github.com/D4id4los/taskboard/actions/runs/38862752532)
   — the `f53b379` provenance run: green; pipeline 91 m 25 s,
   state 77 m 20 s, core 82 m 13 s, props 18 m 01 s.

## Why both raises were kept (and the earlier reverts un-done)

- **state 16 → 128**: measured walls 71/91/72/93/77 min. state 64's
  two samples were 39 (day) and 91 (night) — the worst walls of both
  rungs are ≈ equal because runner variance, not case count, sets
  this job's wall. 128 doubles coverage at no measurable wall cost;
  its worst observed wall (93 m) keeps ≈ 22% margin under the
  120-min timeout.
- **pipeline 1 → 2**: cap 2 walls across six dispatches
  {55, 59, 102, 105, 106, 111} min vs cap 1's own pre-probe
  {24, 72} min. Cap 2 adds ≈ one dominator case (≈ 15–30 min) in the
  same window; the slow windows push *every* cap past the 90-min
  soft bar (core at unchanged cap 16 hit 82 m), so reverting to 1
  would have bought no budget and cost coverage. Kept on the green
  in-budget run 38846463650 (55 m).
- Mid-probe I briefly reverted both caps (planned "pipeline stays 1 /
  state 64" commits); that verdict rested on misattributed data (a
  stale-index commit meant the "cap 1" and "state 64" runs I read
  were actually cap 2 / state 128) and was un-done once the commit
  contents were audited. The final §10 records the corrected
  attribution.

## Findings

1. **Runner/nightly variance swamps the caps** — same-config walls
   moved up to ~4× within hours (pipeline cap 2: 55 ↔ 111 min). The
   90-min bar is exceeded in night windows at *any* cap, including
   caps this chunk never touched. §10 now says a single in-budget
   measurement is weak evidence and headroom is judged against the
   worst-observed wall.
2. **The weekly cron (Mon 03:00 UTC) fires inside the slow window.**
   If the night curve persists on Mondays, re-sizing the core and
   pipeline budgets (or the 90-min bar) is a recorded follow-up —
   flagged in §10, not fixable in this chunk.
3. **Crates.io flake** (run 38836835837): transient download failure,
   resolved by re-dispatch; no workflow hardening applied (out of
   scope, one-off).

## Deviations from the stage-1 plan

- **Final caps differ from the plan's projection**: the plan expected
  "state 64, pipeline stays 1 or 2 by measurement"; the measurement
  kept state 128 and pipeline 2. The plan's D4 headroom preference
  ("prefer the rung measuring ≤ ~68 min") assumed monotone cost, which
  the data falsified; §10 records the corrected rule (worst-observed
  wall basis).
- **More dispatches than planned** (five final-config runs, not one):
  the miscommit audit, one infra flake, and night-window sampling.
  Re-rolling for a fast window was capped deliberately — the last run
  (pipeline 91 m) was accepted and recorded rather than chased.
- **Process incident (recovered)**: a malformed `git checkout`
  detached HEAD mid-convergence, so one squash committed a stale
  index and one push shipped the wrong tip briefly. Both were caught
  by content audits (`git show <sha>:<path>`), corrected, and
  force-pushed with explicit leases. Final branch contents were
  verified commit-by-commit.
- No test-file changes, no PR, no main contact, core/props/timeout
  untouched.
