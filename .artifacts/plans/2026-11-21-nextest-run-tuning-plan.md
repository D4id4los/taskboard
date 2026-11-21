// SPDX-License-Identifier: MIT OR Apache-2.0

# Chunk plan — S2a: the nextest run-side wall regression tuning (#1090)

- **Date**: 2026-11-21
- **Round**: backlog-reduction round 4, chunk S2a; branch
  `perf/nextest-run-tuning` (this plan's landing branch).
- **Normative inputs**: the scope report
  `.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`
  §B-S2; the dispatch plan
  `.artifacts/plans/2026-11-21-backlog-reduction-round4-dispatch-plan.md`
  §1/§4 (Prompt S2a.x); issue #1090; the R4 stage-2 measurement
  `.artifacts/reports/2026-11-19-consolidate-test-binaries-stage2-report.md`
  §6 (the before/after numbers).
- **Acceptance**: the `cargo nextest run --workspace` wall regression
  recovered to ≤ the 86.9 s baseline band (measured with the identical
  R4 method), **or** a recorded residual accepted with evidence
  (CPU-flat proof + the CI-per-job argument), feeding S2b's
  go/no-go (#1087).
- **Scope guard**: tuning only. Zero test-case renames (round law (b));
  no production/test code changes except, under lever L3 only, the
  mechanical un-merging of at most two merged roots (§4).

## 1. The problem, with numbers

The R4 test-binary consolidation (98 → 47 targets) halved the compile
side but the nextest run wall doubled on an idle box, CPU flat:

| Metric | before (98 binaries) | after (47 binaries) |
|---|---|---|
| `nextest run --workspace` wall | 86.9 s | 152.7 s |
| plain `cargo test --all-targets` wall | 358.6 s | 662.4 s |
| plain-cargo CPU (user+sys) | 272.5 + 48.9 s | 268.6 + 37.9 s |

For plain `cargo test` the mechanism is exact: cargo schedules per
binary, so halving binaries halves the scheduling units and the long
serial members fill the tail. For nextest the mechanism is **not
proven**: nextest runs one process per test with global concurrency
(`test-threads`, default = logical CPUs = 16 here), under which binary
count should not change packing. The R4 report's reading ("cargo's
across-binary parallelism has 47 scheduling units") is an inference
carried over from the plain-cargo case. Therefore stage 2 **measures
before it tunes**: the diagnosis phase (§4 Phase A) locates the tail
empirically, and the implement lever is chosen by evidence, not by the
inference.

## 2. Seams inventory (measured at `origin/main` @ `f0c85359`)

- **`.config/nextest.toml`** (the only config seam): one
  correctness-only override — `test-groups` `docker-tier` with
  `max-threads = 1` for `test(it_nextcloud_docker)`. No profile-level
  `test-threads`, no slow-timeout, no retries. CI picks this file up
  automatically (nextest auto-discovers `.config/nextest.toml`); the
  CI Test Execution step is plain
  `cargo nextest run --workspace --no-tests=warn`
  (`.github/workflows/ci.yml`), so **config-only levers reach CI
  without any workflow edit**.
- **Tooling**: `cargo-nextest 0.9.148` locally; CI installs nextest
  via taiki-e/install-action (also 0.9.148-class). `test-groups`,
  profile `test-threads`, and overrides are all stable in this
  version.
- **Box**: 16 logical CPUs; memory gate floor 8192 MiB
  (`scripts/memory-gate.sh`).
- **Binary layout after R4's consolidation** (`cargo nextest list
  --workspace`, fresh capture on this branch's base, 4897 test
  entries): 8 crates; the merged roots are
  `taskboard-app::{app_core, app_services, cli_tasks, cli_sync,
  cli_output, it_cli, it_docker, mcp_smoke}` and
  `taskboard-ui-slint::{journeys, journey_suites, structural,
  probe_composition, probe_pixel, pin_guard}` (members live under
  `tests/core/`, `tests/cli/`, `tests/services/`, `tests/journeys/`,
  `tests/probes/` as `#[path]` modules, per the R4 §2 shared-module
  mechanics). Counts of the tail-heavy binaries (test entries):
  app_core 140, cli_tasks 112, cli_sync 88, app_services 73,
  journey_suites 121, probe_composition 103, journeys 414 (its own
  binary, untouched by R4). Six crates' lib unit-test binaries
  (domain 1329, sync-nextcloud 172, sync-files 146, sqlite 71,
  state 194 across its binaries) keep full binary granularity.
- **Constraint seams**: `docs/agent_dispatch.org` long-command
  discipline; `scripts/run-quality-gates.sh` (flock) is the gate
  wrapper; `scripts/memory-gate.sh` routes expected-heavy commands.

## 3. Measurement method (identical to R4 §6 — the law-(b) comparison basis)

Every wall number reported by this chunk uses the R4 method verbatim:

1. Suites pre-built once (`cargo nextest build --workspace`-equivalent
   warmup; numbers are run-dominated, not build-dominated).
2. `/usr/bin/time`-wrapped, executed **sequentially** on an idle box;
   any run overlapping another suite or gate lock holder is discarded
   and repeated.
3. `cargo nextest run --workspace --no-tests=warn` (the CI
   invocation), wall and CPU captured.
4. Baseline band: 86.9 s ± measurement noise as recorded in R4 §6
   (single-digit percent). Acceptance band: **≤ ~90 s wall, green
   (0 failed)**, with the pass count ≥ the R4 branch's 4730/4730-class
   (current main's count is larger; counts are compared, never
   asserted as an upper bound).

Additionally, for the diagnosis phase only (Phase A), one instrumented
run per lever candidate captures per-test durations via
`--message-format junit -o <file>`; the junit times are aggregated per
binary (this is a *diagnostic*, not the acceptance measurement — the
acceptance measurement stays the R4 method).

## 4. Stage-2 procedure

### Phase A — diagnosis (measure first)

1. R4-method baseline re-run on the branch base: confirm the ~152 s
   wall reproduces (if the box/environment drifted, record the fresh
   pair and use the fresh before/after discipline: every lever is
   compared against the *fresh* un-tuned base, not against the stale
   86.9/152.7 pair).
2. Junit per-test capture on the un-tuned run. Aggregate: per-binary
   wall contribution (sum of test times per binary vs the run wall),
   the tail profile (last-finishing tests), and idle-CPU estimate
   (wall × 16 − CPU).
3. `--test-threads` sweep, R4 method, on the un-tuned layout:
   N ∈ {16 (baseline), 24, 32, 48} — `cargo nextest run --workspace
   --test-threads=N` (CLI flag; the landed lever, if any, goes into
   `.config/nextest.toml`, never into CI flags). Each sweep point
   routes through `scripts/memory-gate.sh` and is watched for new
   failures: oversubscription can surface port/PTY/lockfile races
   (#697's pty-gap class). A sweep point with a failure that the
   baseline does not reproduce is recorded and **disqualifies** that
   N (the tuning may not trade correctness for wall; a genuinely
   exposed flaky test is filed per AGENTS §7 and noted, and the
   failing N is excluded from the lever decision).
4. Decision point: pick the Phase B lever per §5's rules.

### Phase B — implement the chosen lever (evidence-gated)

Apply exactly one primary lever (L1 or L3 below); re-run the R4
method; record before/after. Name-set identity check (§6) after any
code-touching lever.

### Phase C — verdict + report

- If the wall is back in the acceptance band: lever landed, green run
  recorded, report states the recovery and its shape (wall + CPU +
  pass counts), #1090 closure recommendation for the dispatcher.
- If not: the residual is recorded with evidence (fresh before/after
  pair, CPU-flat proof, the CI-per-job argument: CI runners schedule
  one suite per runner, so the runner-side wall is bounded by the
  same job either way) and the verdict **binds S2b**: "consolidation
  is net-positive on compile, run-side residual accepted" or
  "phase-2 consolidation proceeds only for crates whose run-side cost
  is measured benign". #1087's wontfix-with-evidence path is the
  dispatcher's, per dispatch plan §4.

## 5. Levers — chosen, ordered; rejected alternatives

### L1 (chosen primary): profile `test-threads` oversubscription in `.config/nextest.toml`

- **What**: land `profile.default.test-threads = <N>` where N is the
  sweep's best point (expected N = 24 or 32, i.e. 1.5–2× CPUs), plus
  a comment recording the sweep evidence and the memory floor
  interaction.
- **Why first**: the diagnosis shows the box idles (86.9 s wall at
  ~3.7× effective parallelism on 16 CPUs; 152.7 s at ~2.1×). The
  suite is fixture/process-heavy (daemon boots, PTYs, sqlite pools,
  wiremock) — wall-clock bound with CPU to spare. Oversubscription
  fills idle CPU with zero code movement, zero build cost, zero
  rename risk, and reaches CI through the existing config seam.
- **Risk**: memory (floor 8192 MiB; 32 concurrent fixture processes)
  → memory-gate'd sweep; a point that OOMs or trips the gate floor is
  excluded. Flaky-exposure risk → the §4 Phase A.3 disqualification
  rule. Locally-heavy but CI-runner-scaled (4 vCPU runners): an N of
  32 on a 4-CPU runner is oversubscription ×8 — record the
  runner-CPU count during the PR and, if runner wall regressed on
  the first PR run, cap N via a documented `NEXTEST_TEST_THREADS`
  env override in the CI step (the only workflow edit this chunk
  ever makes, and only with measured evidence).

### L2 (rejected as primary): per-binary test-groups in `.config/nextest.toml`

- `test-groups` + `binary()` filter overrides to pin slow binaries
  into serial groups. **Rejected**: it *reduces* concurrency; it is a
  correctness knob (already used for the docker tier), not a wall
  recovery knob. Only reconsider if Phase A shows the tail is
  contention-driven *within* the docker tier specifically — not the
  #1090 signature.

### L3 (conditional fallback, evidence-gated): split the merged hot roots back out

- **Trigger**: only if Phase A shows the tail is dominated by one or
  two merged binaries' long serial members (their summed test time >
  ~25% of wall) *and* L1's sweep fails to reach the acceptance band
  — i.e. the regression really is granularity-driven.
- **What**: un-merge at most the two worst roots (expected candidates:
  `cli_tasks`, `app_core`) by restoring their member files as
  standalone `tests/` targets — verbatim moves, the shared
  `common`/`support` module resolution per target (the R4 §2
  mechanics in reverse).
- **Name-set identity handling**: a split removes the member-module
  prefix from nextest strings (`cli_editing::<fn>` → the member's
  flat path). This is exactly the R4 stage-2 report §3 erratum class
  ((a) the member-module prefix, inherent to grouping/un-grouping).
  The identity guarantee is therefore stated at function level:
  a programmatic bijection proof (canonical function names before ==
  after, prefix stripped), residual 0, recorded in the report — the
  same erratum discipline R4 used. Zero function renames; law (b)
  holds.
- **Cost**: build-locality loss for those members (the trade-off R4
  accepted in reverse); mechanical diff ~200 lines.

### L4 (the recorded residual): accept with evidence

- The fallback verdict path (Phase C). Not a "lever" that lands code:
  the acceptance is the §4 Phase C evidence bundle plus the S2b
  binding. This is the honest outcome if both L1 and L3 miss the
  band — CPU flat means the merge costs wall but not work, and CI
  pays it once per runner.

### Rejected alternatives (recorded)

- **Retries/slow-timeout tuning**: addresses flakiness, not wall.
- **nextest `archive` + `--archive` / partitioned CI shards**: a
  CI-workflow restructure, out of scope for #1090's "run-side
  regression" and changes the CI invocation the measurement law pins.
- **Reverting the R4 consolidation wholesale**: destroys the landed
  compile-side win and the shared-module mechanics fix; S2b's
  phase-2 consolidation would re-merge the six small crates, not
  un-merge app/ui.
- **`[profile.default] retries`-style flake masking**: none needed;
  the suite is nextest-green on main.

## 6. Open questions — recorded decisions

- **Q1 — Which lever first?** Decision: L1, because the diagnosis
  signature (CPU flat, wall doubled, box idle in the tail) matches
  oversubscription and it carries zero rename/build risk. L3 is not
  attempted unless L1 measurably fails AND the tail-concentration
  trigger holds (both conditions, not either).
- **Q2 — Is the #1090 mechanism (binary count ⇒ nextest packing)
  accepted as fact?** Decision: no. It is accepted for plain
  `cargo test` (scheduling is per binary) and treated as a hypothesis
  for nextest (global one-process-per-test). Phase A exists to test
  it; if disproven, the finding is commented onto #1090 (the issue's
  mechanism statement gets an erratum note in the chunk report).
- **Q3 — name-set identity under L3?** Decision: the R4 §3
  function-level bijection erratum precedent applies (§5 L3); the
  literal byte-identity reading is unsatisfiable under any
  un-grouping, and the report records the programmatic proof.
- **Q4 — CI workflow edit?** Decision: none by default. CI reads
  `.config/nextest.toml`; the lever lands there. The only sanctioned
  workflow touch is the documented `NEXTEST_TEST_THREADS` override in
  the Run Tests step, and only with measured runner-side regression
  evidence (§5 L1 risk).
- **Q5 — What binds S2b (#1087)?** Decision: Phase C's verdict, one
  of: (i) band recovered → S2b proceeds under the same levers;
  (ii) residual accepted → S2b proceeds (phase-2 consolidation of the
  six small crates is compile-side-positive and their binaries are
  short-tail), with the run-side cost of each newly merged root
  measured before landing; (iii) L3 un-merge landed → S2b's plan
  respects the un-merged members. Recorded in the chunk report for
  the dispatcher.
- **Q6 — Is the docker-tier serialization touched?** Decision: no.
  `docker-tier max-threads = 1` is a correctness property of the
  single-server Nextcloud fixture; every sweep point and landed
  config keeps it byte-identical.

## 7. Risks

1. **Oversubscription exposes races** (fixed ports, PTY sessions,
   sqlite pools): Phase A.3's disqualification rule keeps the landed
   N green; any genuinely exposed flaky test is observed-and-filed,
   never fixed opportunistically (AGENTS §7 flaky law).
2. **Memory floor breach** on the sweep or under the landed N on the
   8192 MiB floor: memory-gate routing + sweep-point exclusion; the
   landed N is capped by the lowest passing point, not the fastest.
3. **Measurement noise / box contention** invalidates a comparison:
   the R4 method's idle-box + discard rule; a contended run is
   re-run, and gate-lock holders are respected via
   `scripts/run-quality-gates.sh --status` before timing work.
4. **CI-runner scale mismatch** (4 vCPU vs 16 local): the landed
   profile is validated against the PR's own CI run before handoff;
   §5 L1 risk names the sanctioned override path.
5. **L3 un-merge churn** if triggered: verbatim-move discipline
   (R4's mechanics in reverse), function-level bijection proof, and
   the pre-commit `duplicate_mod` gate as the tripwire — each split
   target re-declares the shared module exactly once.

## 8. Estimated diff size

- **L1-only (expected)**: `.config/nextest.toml` +8–15 lines (profile
  test-threads + evidence comment); report + plan docs. No code.
- **L3 triggered**: additionally ~200 mechanical lines across
  `crates/taskboard-app/tests/` (two roots un-merged into standalone
  targets) — no test-body changes.
- Reports: `.artifacts/reports/2026-11-21-nextest-run-tuning-report.md`
  (stage 2/3 write; this plan does not pre-create it).

## 9. Stage-3 additions

Standard review block (dispatch plan §4 Prompt S2a.x): rebase dance,
identity check re-run, R4-method gate evidence re-runnable, PR body
references #1090 (closure is the dispatcher's at merge), no CI watch,
worktree teardown after PR-green per the lifecycle law.
