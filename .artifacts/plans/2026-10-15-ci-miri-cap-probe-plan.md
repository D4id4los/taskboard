# [Plan] Miri Cap Raise Probe — a Measured `PROPTEST_CASES` Ratchet

- **Date**: 2026-10-15
- **Kind**: stage-1 plan (chunk P8 of the 2026-10-14
  parallel-during-GUI dispatch,
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md` §9) —
  CI + docs only; **no production code, no test files**. The deliverable
  is a *measured* cap raise: stage 2 converges on it by driving
  `workflow_dispatch` runs from the pushed branch.
- **Target**: `.github/workflows/miri.yml`, `docs/testing_strategy.org`
  §10, `.artifacts/taskboard-potential-improvements-backlog.md` (final
  commit).
- **Branch / worktree**: `ci/miri-cap-probe` in
  `../taskboard-WT-ci-miri-cap-probe`, off `main` @ `dce5009`.
- **Goal**: raise the state-crate job's `PROPTEST_CASES` severalfold and
  the pipeline job's from 1 to 2, keep exactly what measures green and
  ≤ 90 min per job, and record caps + measurements in
  `testing_strategy.org` §10. Every change stays an interpreter-only
  environment/speed gate — the dispatch's hard rule: **zero test-file
  changes**.
- **Status**: DRAFT — awaiting user review; stage 2 implements only
  after approval.

---

## 1. Context & Findings (verified 2026-10-15 against `main` @ `dce5009`)

1. **Current caps, verified in `miri.yml`** (the dispatch warned the
   backlog's numbers might drift — they did): domain core **16**
   (`miri.yml:48`), domain pipeline **1** (`miri.yml:76`), domain
   remote/state properties **16** (`miri.yml:86`), state **16**
   (`miri.yml:105`); every job `timeout-minutes: 120`. These match §10's
   recorded caps. The backlog's "raise 32→64" misread the state cap: 32
   is the number of *interpreted cases* at cap 16 (2 properties × 16),
   not the cap itself — "severalfold" therefore means 16→64 as the
   first candidate.
2. **Run history** (per-job walls from
   `gh api …/actions/runs/<id>/jobs`, `started_at → completed_at`):

   | Job (cap) | 2026-10-12, run 38312493624 | 2026-10-15, run 38786976628 |
   |---|---|---|
   | domain core (16) | 55 m 18 s | **1 h 21 m 10 s** |
   | domain pipeline (1) | 24 m 26 s | **1 h 12 m 10 s** |
   | domain props (16) | 20 m 42 s | 17 m 59 s |
   | state (16) | 8 m 10 s | 13 m 00 s |

   Run 38786976628 is P7's verification dispatch on
   `test/domain-deterministic-stamps` — i.e. the current config *and*
   the current post-P7 test scope, green, three days old. It is the
   baseline this probe measures against.
3. **The variance finding (this probe's dominant risk)**: the pipeline
   job ran the *identical* 51 tests, all green, at the identical cap in
   both runs — yet its "Run Miri" step went 24 m → 1 h 12 m (~3×).
   P7's diff (`outbox.rs`, `test_support/contract.rs`,
   `test_support/memory.rs`, docs only) does not touch the
   `pipeline::` family; the drift is rolling-`nightly` interpreter
   changes plus runner variance. Consequences:
   - the backlog's "cap 2 projects ≈ 35–50 min" is stale — at the
     Oct-12 curve cap 2 projects ≈ 45–65 min (comfortable), at the
     Oct-15 curve ≈ 90–110+ min (likely a miss). Only a measurement
     decides which curve is current;
   - a single ≤ 90-min measurement is necessary but weak evidence of
     weekly repeatability → the headroom preference rule (D4).
4. **Core has no headroom and is not a candidate**: 1 h 21 m against
   the 90-min bar, most of the +26 m vs Oct-12 being P7's legitimately
   newly-interpreted families (PendingOp roundtrip, read-port contract,
   repository-contract harness now run interpreted instead of gated).
   Neither the dispatch nor the backlog names core for a raise. Its
   refreshed wall gets *recorded* in §10; if drift ever pushes the
   weekly core job past the 120-min kill, that is a finding to report,
   not something this chunk fixes.
5. **State job anatomy**: setup ≈ 20 s; the single "Run Miri" step
   (compile + tests together) took 7 m 49 s (Oct-12) / 12 m 36 s
   (Oct-15) at cap 16. The p2/p3 equivalence pins are cheap per case —
   ≤ ~12 m covers compile plus 32 interpreted cases — so 16→64
   projects well under the bar, but the projection is not the evidence;
   the rung measures it.
6. **Pipeline job anatomy**: 51 tests, two of them properties —
   `equal_replicas_converge` (cheap) and
   `apply_push_report_equals_the_pipeline_push_phase`, the family's cost
   dominator at ≈ 15–25 min per interpreted case (§10's subtraction).
   Cap 1→2 adds exactly one extra case of each property, so the
   expected delta is ≈ +15–30 min over the cap-1 wall of that day's
   curve.
7. **Iteration mechanics** (unchanged from the 2026-10-12 tuning round,
   `.artifacts/plans/2026-10-12-ci-miri-tuning-plan.md` finding 8):
   `workflow_dispatch` executes the workflow definition *from the named
   ref*, so `gh workflow run miri.yml --ref ci/miri-cap-probe`
   exercises each rung pushed to the branch; `miri.yml` has no
   push/pull_request triggers, so branch pushes trigger nothing else;
   the per-ref concurrency group isolates branch dispatches from
   `main`'s weekly cron. Dispatch §1.3/§9 authorizes stage 2 to push
   this branch (never a PR) — the WI-C precedent.
8. **Measurement extraction**: per-job wall from the Actions API
   (`started_at → completed_at`; step-level for attribution). **Never**
   libtest's `finished in` figure — under isolation Miri emulates the
   monotonic clock from interpreted work (run 38786976628's pipeline
   job printed `finished in 4454.18s` against a 1 h 12 m wall). §10
   already legislates this; the plan inherits it.
9. **Non-time failure protocol is pre-mandated** by the dispatch
   (P8-2): a candidate that fails for a non-time reason is a finding
   to report — never to be hidden by lowering a cap, and never
   "fixed" with a test-file change. More interpreted cases reach more
   code paths; a property failure at a higher cap may be a real bug
   the probe found (AGENTS §5's bug-vs-test rule applies in full).

## 2. Design Decisions

| # | Decision | Choice & rationale |
|---|---|---|
| D1 | **Scope fence** | Only the two named caps move: state and pipeline. Core/props stay at 16 (core has no headroom — finding 4; props is not named by the dispatch). `timeout-minutes: 120` unchanged; no new jobs, no `ci.yml`, zero test files. The final diff is `miri.yml` + `testing_strategy.org` + backlog only, exactly the dispatch's acceptance shape. |
| D2 | **Ladder shape** | Rung 1 bundles state 16→**64** and pipeline 1→**2** in one dispatch: the four jobs run in parallel and are measured independently, so one run yields both candidate measurements plus two untouched control jobs; bundling halves runner time. Rung 2 (conditional): state 64→**128** only if rung 1's state measurement both fits and leaves D4 headroom. Pipeline has no intermediate rung between 1 and 2 — it keeps or reverts, nothing in between. |
| D3 | **Keep rule (the dispatch's, verbatim)** | A candidate is kept iff its run is **green** and that job's measured wall is **≤ 90 min**. Otherwise revert: state drops back one rung (64→32 if 64 missed, per finding 5's projection an unlikely branch, but the ladder covers it), pipeline drops back to 1. A revert is a valid probe outcome, not a failure of the chunk — the measurement itself re-anchors §10. |
| D4 | **Headroom preference** | Where rungs exist (state), prefer one measuring ≤ ~68 min (25% under the bar). A keep in the 68–90 band is admissible only where no intermediate rung exists (pipeline) and must carry an explicit variance note in §10. Rationale: the 90-min bar was sized in the tuning round for ±10–20% runner variance; the observed same-config drift reached ~3× (finding 3) — the bar alone no longer buys its intended margin. |
| D5 | **No dedicated baseline rung** | Run 38786976628 is the baseline (current config + current scope, green, Oct-15). Rung 1's untouched core/props jobs double as a free same-day control group: their walls vs 38786976628 quantify same-week drift at zero extra runner cost. |
| D6 | **Cap-honored tripwire** | Nothing prints the effective case count (established in the tuning round). The rung's own delta is the proof: state at 64 must show a materially higher "Run Miri" step than the ≤ 13-min baseline; a flat state wall means the env did not take (typo'd value — proptest ignores malformed `PROPTEST_CASES` with a stderr note only). |
| D7 | **Non-time red = stop and report** | Per finding 9 / the P8-2 mandate: triage via `gh run view --log-failed`; interpreter aborts or property failures at higher caps are findings with a proposed fix prompt, never a lowered cap hiding them, never a gate added in this chunk (a scoped `#[cfg_attr(miri, ignore)]` would be its own chunk with its own justification). |
| D8 | **Green-run provenance** | Stage 2 iterates with interim pushed commits, then squashes to the clean final set — which rewrites the SHA the green runs came from. After the squash (and after any stage-3 config fix), re-dispatch once on the final SHA and link *that* run in the PR. Content-identical is not enough; SHA-identical costs one run. |
| D9 | **§10 bookkeeping** | The caps bullet's numbers and the measured-walls sentence refresh to the converged config with the run dates; the pipeline paragraph is rewritten to whatever the evidence showed (kept at 2 with the variance note, or stays at 1 with the measurement that killed 2 — both are honest outcomes); the interpreted-case arithmetic refreshes (state: 2 × cap); one new sentence records the observed same-config week-over-week drift and the ≥ 25%-headroom-where-rungs-exist preference. |
| D10 | **Commit hygiene** | Interim ladder commits accumulate freely during iteration and are squashed before handoff (AGENTS §8 rebase-merge hygiene — no "try 64 cases" noise lands on `main`); final set per §6, each standalone. |

## 3. The Probe Ladder

Each rung = one pushed (interim) commit + one `workflow_dispatch` run +
per-job wall extraction. Escalation is evidence-driven; no rung runs on
a projection alone.

### Rung 1 — state 64 + pipeline 2 (bundled)

`miri.yml`: state job `PROPTEST_CASES: "64"`, pipeline job
`PROPTEST_CASES: "2"`; core/props untouched. Dispatch, watch, record
all four jobs' walls. Decision matrix:

- **state ≤ 90 min** (projection: ~15–35 min): keep 64; compute the
  per-case slope from the (16, 64) pair — `t(N) ≈ t₀ + k·N` — and
  evaluate rung 2. **> 90 min**: revert to 32 (the fallback rung;
  finding 5 makes this branch unlikely but the ladder owns it), same
  keep rule there.
- **pipeline ≤ 90 min**: keep 2 (if it lands in the 68–90 band, the
  D4 variance note is mandatory in §10). **> 90 min or timeout kill**:
  revert to 1 and record the measurement — this re-anchors §10's
  pipeline justification on current data instead of the stale
  "≈ 35–50 min" projection, which is itself deliverable value.
- **core / props walls** vs 38786976628: the same-week drift factors,
  recorded in the probe log (they parameterize how much trust each
  keep deserves).

A red *run* (e.g. pipeline timed out at 120 min while state finished
green at 30 min) still yields rung-1 measurements for every job that
completed; only the final config needs a fully green run (D8).

### Rung 2 — state 128 (conditional)

Runs only if rung 1 kept 64 **and** measured it ≤ ~68 min. Same
protocol, same keep rule. Expected ~25–50 min at either observed curve;
a miss reverts to 64 with the measurement recorded.

### Convergence

Revert whatever the measurements rejected, squash to the final commit
set (§6), push `--force-with-lease`, re-dispatch once on the final SHA,
confirm green + ≤ 90 per job — that run is the PR's linked evidence.

## 4. Validation Protocol (every rung)

```sh
# in ../taskboard-WT-ci-miri-cap-probe, on ci/miri-cap-probe
git push origin ci/miri-cap-probe        # dispatch §1.3 P8 exception; never a PR
gh workflow run miri.yml --ref ci/miri-cap-probe
gh run list --workflow miri.yml --limit 1   # grab the run id
gh run watch <run-id> --exit-status        # blocks until ALL jobs finish
gh api repos/{owner}/{repo}/actions/runs/<run-id>/jobs \
  --jq '.jobs[] | "\(.name): \(.started_at) → \(.completed_at) \(.conclusion)"'
```

Per rung, record: run URL, per-job walls + conclusions, the D3 verdict
per candidate, slope updates, and the control jobs' drift factors.
These form the probe log that stage 2 reports (candidate → run URL →
timings → keep/revert) and the PR's evidence trail.

Sanity checks folded into every rung: `--exit-status` conclusion; walls
≤ 120 min (a timeout kill is an automatic miss for that job);
`--log-failed` triage for any non-time red (D7 — stop and report); the
D6 delta check whenever a cap changed. Never re-dispatch while a run of
the same ref is still live — the per-ref concurrency group would cancel
it mid-measurement.

## 5. What `testing_strategy.org` §10 Records

The final commit updates §10's "weekly job's contract" block; exact
wording follows the converged config, content per D9:

1. The per-job caps list, refreshed to whatever was kept, with the
   measured wall times *at those caps* and the measurement dates.
2. The pipeline sentence rewritten on the probe's evidence: either
   "cap 2, measured X min (run Y, Z % headroom)" or "cap stays 1 —
   cap 2 measured Z min / timed out (run Y) at the then-current
   nightly curve".
3. The state arithmetic refreshed (2 properties × cap = interpreted
   cases) and its cost profile updated from the measured slope.
4. One sentence on the observed same-config week-over-week wall drift
   (runs 38312493624 vs 38786976628: pipeline ~3×) on the rolling
   nightly, and the standing preference for ≥ 25% headroom where cap
   rungs exist.
5. The existing policy sentences stand unchanged: caps are budget
   statements, never coverage statements; moving one requires a
   measured budget justification recorded in the PR that moves it.

## 6. Commit & Bookkeeping Plan (stage 2)

Interim ladder commits (e.g. `ci(workflows): miri probe rung 1 —
state 64 / pipeline 2`) accumulate during iteration, then squash to:

1. `ci(workflows): miri — raise state/pipeline proptest caps on a
   measured probe` — the final `miri.yml` (pipeline's raise included
   iff kept).
2. `docs(testing): record measured miri cap raises in testing_strategy
   §10` — §5's content; separate because it is reviewable prose, not
   config.
3. `docs(backlog): miri cap probe done` — marks "[2026-10-12] Raise
   the Miri pipeline/state case caps on a measured probe" ✅ DONE with
   a one-line note naming the final green run URL.

After the squash: `git push --force-with-lease origin
ci/miri-cap-probe`, one fresh dispatch on the final SHA (D8), hand off
to stage 3. Stage 3 ships the PR (description links the green in-budget
run(s)); the merge stays the user's, as always.

## 7. Risks

- **Same-config variance up to ~3×** (observed, finding 3) makes any
  single-run keep weak evidence of weekly repeatability. Mitigations:
  D4's headroom preference, the control jobs, and the standing
  weekly-red triage posture (a drift-driven red is re-triaged on the
  next nightly; if a kept cap proves chronically over budget, lowering
  it is a recorded follow-up, never a quiet one).
- **Pipeline cap 2 plausibly misses** at the current curve (projection
  ≈ 90–110 min). The probe still wins — the measurement replaces a
  stale projection in §10 — but a timeout costs up to 120 min of one
  job's wall before `gh run watch` returns; stage 2 plans the wait.
- **Core sits at 1 h 21 m with ~11% margin to the 120-min kill** on
  the post-P7 scope. Not this chunk's change and not fixable within it
  (no core authorization); a drift-driven core red during the probe
  window is a report-and-stop finding, per D7's spirit.
- **Nightly churn / env footguns** (rolling nightly moves ~daily; a
  malformed `PROPTEST_CASES` is ignored with a stderr note only):
  D6's delta check is the tripwire; pinning a dated nightly stays
  rejected (the tuning round's D11 — the weekly job *wants* fresh
  Miri).
- **The first scheduled weekly run** (Monday 2026-10-19 03:00 UTC)
  exercises whatever sits on `main` when it fires; until this PR
  merges that is the old caps. Different refs — no interaction with
  the probe's dispatches either way.

## 8. Out of Scope

- Core/props cap changes, `timeout-minutes` changes, new jobs, any
  `ci.yml` edit (dispatch non-goals, restated).
- Nightly pinning / `rust-cache` for `miri.yml` (rejected in the
  tuning round; unchanged).
- Any test-file change, including scoped Miri gates for code paths a
  higher cap might newly reach — a D7 event goes to a report with a
  proposed fix prompt, not into this diff.
- Migrating the raw `Config::with_cases` blocks to
  `test_support::proptest_config` (standing §9 backlog item).

---

**Estimated diff**: `miri.yml` ± 2 lines (two env values, plus the
env-comment's cap mention if wording shifts), `testing_strategy.org`
§10 ± 15 lines, backlog + ~5 lines. The chunk's mass is runner time
(2–3 dispatch runs × up to ~2 h wall each, bounded by the slowest job),
not diff size — as with the tuning round.
