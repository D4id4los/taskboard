# Chunk Plan — P9 build-cache evaluation (#890)

- **Date**: 2026-11-18
- **Kind**: stage-1 chunk plan — research chunk (evidence report +
  ruling doc; operator-gated, mirroring the #708/#891 pattern).
- **Origin**: tier-2 dispatch plan
  `.artifacts/plans/2026-11-18-tier2-dispatch-plan.md` §1/§10; the
  recommendation report §Tier-2 (#890); issue #890; the OOM incident
  report L4 (`.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`);
  the worktree disk report
  (`.artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md`).
- **Branch**: `docs/build-cache-evaluation`; worktree
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-docs-build-cache-evaluation`
  (branch created off `origin/main` @ `5d6bd327` via
  `git worktree add -b`, doctor passed: 89G free / 22066 MiB avail).
- **Hard boundary — zero build-config code**: no `config.toml`, no
  `CARGO_TARGET_DIR` export in any script, no sccache installation or
  wiring, no CI change, no pre-commit-hook edit lands in this chunk.
  The diff is exactly: the chunk plan (this file), the evidence
  report, the ruling doc. Any scratch harness needed for measurement
  lives only in the worktree's untracked scratch space and is deleted
  before handoff (evidence is excerpts inside the report — the
  evidence & artefact mandate). A rollout, if recommended, is its own
  follow-up chunk after the operator gate.

## 1. Problem statement (from the recorded evidence)

- Each parallel worktree carries an isolated `target/`: measured
  36–55G each (disk report §1, 2026-10-18, `du -sh` evidence); debug
  builds are debuginfo-dominated (30G/23G of `target/debug`), with
  `llvm-cov-target` a further 13–25G when left behind.
- Seven concurrent cold clippy builds (pre-commit hooks, unflocked)
  were the main memory source of the 2026-11-17 OOM (incident report
  §1); L4 names the per-worktree cold build as the structural cost —
  zero cache sharing between worktrees.
- Current mitigations already landed: the memory gate (L1), the agent
  cap (L2), the doctor memory floor (L3), worktree teardown (R1), the
  trailing `llvm-cov-target` rm (R2). None of them share build
  artifacts; they only serialize/limit. #890 asks whether sharing is
  worth its risks.

## 2. Candidate schemes to evaluate

Four schemes; each gets the §3 measurement battery and the §4 risk
axes applied. The baseline (A) is the status quo.

- **A — baseline / status quo**: per-worktree `target/`, no shared
  cache. What today's law already assumes.
- **B — sccache (compiler-cache server)**: a per-machine `sccache`
  server fronting rustc; each worktree keeps its local `target/` for
  linking/incremental, but compilation units are served/ filled by the
  shared server. Expected shape: kills *duplicate* cold builds across
  worktrees; does not shrink per-worktree `target/` much (linking
  artifacts remain); adds a server daemon (its own memory footprint —
  must be measured) and cache-dir disk.
- **C — shared `CARGO_TARGET_DIR` per machine**: one `target/` for all
  worktrees of the repo. Expected shape: maximal disk saving (one
  target dir instead of N) and maximal warm-cache reuse; highest risk
  — concurrent cargo invocations from two worktrees share lockfile
  semantics (cargo takes a build-dir lock; contention or, worse,
  inter-worktree rebuild churn when `Cargo.toml`/feature state
  differs), stale-artifact semantics across branches, and every
  memory-gate serialization assumption must be re-derived.
- **D — per-branch target dirs** (the L4 wording: "per-worktree-hash"):
  `CARGO_TARGET_DIR` keyed by branch/chunk (e.g. a stable per-branch
  name), so a recreated worktree for the same chunk reuses its cache
  while unrelated branches stay isolated. Middle ground: disk drops
  for the recreate/resume case (the doctor-teardown → re-`worktree
  add` loop), cross-branch poisoning stays bounded by branch.
- **E — hybrid**: B + D (sccache across all worktrees + per-branch
  target dirs), or C plus a `cargo clean`-on-handoff law. Evaluate the
  combination only if B and C individually show complementary wins
  (e.g. B solves cross-worktree compile reuse, D solves disk for
  resume).

For each candidate the report records: mechanism, configuration
surface (what a rollout would change — env var, config.toml, hook,
CI), measured A/B numbers, risk-axis evidence, disk delta, and
operational cost (daemon lifecycle, cache eviction, debugging opacity
when a cache question masks a real build bug).

## 3. Measurement battery — the controlled A/B (method)

**Laws binding on every run** (from the round law [L](a) and the [D]
discipline): every number carries the exact command + environment box
(context below); benchmark builds run through
`scripts/memory-gate.sh`; runs are BACKGROUND with short interleaved
polls and useful work between polls; landed work is committed BEFORE
any wait.

**Environment block (captured once per session, quoted in the
report)**: kernel, CPU cores, total RAM, `MemAvailable` at run start,
free disk, rustc/cargo version, `sccache --version` if installed, and
the concurrency context (which other agents' builds are in flight —
see sequencing below).

### 3.1 Worktree configuration

- All measurement worktrees live at
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-bench-*`
  (scratch worktrees, **not** git worktrees of record — created with
  `git worktree add -b bench/...` naming so the doctor can see them,
  and removed after the battery; or, cheaper, the evaluation reuses
  the chunk's own worktree plus one extra bench worktree). Decision
  for stage 2: minimize worktree count; at most two build sites exist
  at once (needed for the concurrency risk axis).
- Measured commands (the four that dominate agent life, per the
  incident evidence):
  1. **cold full clippy** — `cargo clippy --workspace --all-targets
     -- -D warnings` in a worktree whose `target/` is empty (the
     pre-commit hook's peak);
  2. **warm clippy** — same command immediately re-run (the
     stage-2→stage-3 re-run shape);
  3. **cold test build** — `cargo nextest run --workspace --no-run`
     from empty;
  4. **branch switch rebuild** — commit a one-line change in a leaf
     crate, rebuild (the inter-chunk incremental shape).
- Metrics per run: wall time (`/usr/bin/time -v`), peak RSS of the
  build tree (`/usr/bin/time -v` maximum resident set size, plus
  `MemAvailable` delta sampled before/after), and resulting
  `target/` size (`du -sh`). sccache additionally reports its own
  hit/miss/size (`sccache --show-stats`).

### 3.2 Load control — the confounding problem

Other agents are running concurrently on this box (the ≤2-cap beside
r9), so raw wall times are noisy. Controls, in order of strength:

1. **The memory-gate law does the serialization**: every benchmark
   build routes through `scripts/memory-gate.sh`, so benchmark builds
   cannot overlap other gated heavy builds — the gate's flock is the
   sequencing mechanism. The report records gate hold/wait events per
   run (a deferred start is recorded, never hidden).
2. **Pairs are measured back-to-back inside one gate hold** where the
   scheme permits: A-runs and candidate-runs alternate (A, B, A, B,
   ...) rather than "all A then all B", so slow drift in background
   load affects both arms equally (blocking on order effects).
3. **MemAvailable is sampled at run start and end**; a run whose
   start MemAvailable deviates >20% from its paired run's is marked
   `LOAD-FLAGGED` in the evidence table and the affected comparison
   is re-run, not averaged away.
4. **Timing comparisons use medians of 3 runs** per arm per command
   (3 cold runs is expensive but cold builds are the whole point;
   warm/incremental arms get 3 cheap runs). Memory peaks are less
   load-sensitive (RSS of one build) — single run per arm is
   acceptable there, start-MemAvailable still recorded.
5. **Concurrency risk-axis runs are exempt from load control** — they
   are deliberately about contention, not clean timing; their metric
   is outcome (correctness, lock behavior), not seconds.

### 3.3 What the A/B must answer

- Does the scheme reduce cold-build wall time across worktrees
  (compile reuse), and by how much, at what peak memory?
- Does the scheme reduce per-worktree disk (only C/D/E plausibly)?
- What is the scheme's own fixed cost (sccache server RSS + cache
  dir size; shared-dir lock wait)?
- What breaks (the §4 axes) and how loudly?

## 4. Risk axes to evidence

1. **Cache poisoning across branches** — stale incremental state,
   feature-flag divergence (a worktree building `--all-features` vs
   default), a reverted commit leaving artifacts newer than sources.
   Evidence method: build branch X, switch the *shared* scheme to
   branch Y with different feature state, then re-build X and verify
   correctness (the build must be identical to a clean X build —
   compare artifact fingerprint/`cargo build --message-format` or
   simply a nextest run green + a known-behavior test, not text).
   For sccache the poisoning surface is different (content-addressed
   compiler inputs — safe in theory; verify the actual hit/miss behavior
   on a feature-flag flip).
2. **Concurrent-access semantics** — two worktrees building
   simultaneously against the shared scheme. Actually exercised
   (dispatch-plan trap (b): not asserted from documentation):
   two `cargo build`s from two worktrees against scheme C's one
   target dir; two `sccache`-fronted builds against scheme B's
   server. Record: cargo's build-dir lock behavior (serialize?
   fail?), sccache server behavior, correctness of both outcomes.
3. **CI parity** — CI runners are ephemeral and one-suite-per-runner;
   the report records whether each scheme is CI-invisible (A, B with
   a GH-actions sccache action + GHA cache), CI-neutral-recommended
   (D), or CI-hazardous (C), and the ruling states CI's standing
   configuration explicitly either way. No CI change lands in this
   chunk.
4. **Operational/behavioral risks**: debugging opacity (a wrong
   cached artifact's blast radius), eviction policy (cache growth is
   bounded by what?), the doctor/teardown interaction (a shared dir
   must NOT be removed at worktree teardown — the teardown law's
   wording would need the rollout chunk to amend AGENTS §8/§10;
   recorded, not changed).

## 5. Report & ruling skeletons

### 5.1 Evidence report — `.artifacts/reports/2026-11-19-build-cache-evaluation-report.md`

1. **Header**: kind (evidence report), origin (#890, L4, dispatch
   plan §10), the environment block, the method (§3 of this plan,
   as-executed including deviations).
2. **Baseline characterization**: today's numbers (cold/warm/incremental
   from §3.1's battery, arm A) + the disk picture.
3. **Candidate results**: one section per scheme B/C/D/E — mechanism,
   configuration surface, measurement table (command, run, wall,
   peak RSS, start MemAvailable, target size, sccache stats where
   applicable, LOAD-FLAG marks), risk-axis evidence per §4.
4. **Concurrency exercise log**: the two-worktree runs verbatim
   (commands + observed outcomes).
5. **Analysis**: per-axis comparison; what each scheme buys vs costs
   on this box (30G RAM, the ≤3-agent cap, the memory gate).
6. **Recommendation**: exactly one (options B–E + keep-A with
   trade-offs all stated; one chosen; consequences for later work;
   the rollout shape).
7. **Appendix**: full command transcripts (paths of scratch, since
   removed — noted), the gate/deferral log.

### 5.2 Ruling doc — `.artifacts/plans/2026-11-19-build-cache-ruling.md`

One page for the operator: the question, the one recommendation +
its three decisive numbers, what a rollout chunk would change
(files/scripts/law text), what stays operator-coordinated (any
AGENTS §8/§10/§5 law amendment, CI config, daemon lifecycle policy),
and the explicit gate handoff (accept → a rollout chunk gets planned;
reject → status quo stands and #890 closes on the evidence). The
ruling carries no new evidence — it cites the report.

## 6. Stage-2 execution order (for the dispatcher's sequencing)

1. Land this plan (already done by stage 1); read it + the mandate.
2. Capture the environment block; build the baseline arm A numbers
   (background + polls; commit nothing yet — the report is written
   incrementally in the worktree, committed after each completed
   arm so a quota-cut leaves evidence).
3. Install/verify sccache (scratch-local if a system install is
   needed — record the version; installation into the user env is
   measurement setup, not build-config code; nothing lands).
4. Arms B, C, D in that order (E only if complementary), interleaved
   per §3.2(2); the concurrency exercise per §4(2).
5. Write the report + ruling; delete all scratch (bench worktrees,
   scratch config, sccache cache dir if scratch-local); pre-commit
   hooks gate; no push.
6. Backlog issues for any out-of-scope findings; report numbers.

**Stage-2 gate**: docs-only chunk — pre-commit hooks are the gate;
the dispatcher runs the full §10 suite (dispatch plan §1 shared
model). Benchmark runs themselves route through the memory gate and
defer per the flock-deferral rule when contended.

## 7. Open questions resolved at stage 1

- **Which scheme gets measured?** All four (B–E) plus baseline A;
  E conditionally (only on complementary B/C results). Decision: the
  dispatch plan's acceptance names all four shapes.
- **Where do bench worktrees live and how are they cleaned?** Scratch
  worktrees named `bench/...`, removed in stage 2 step 5, never left
  for the doctor to decide — measured-heavy dirs must not linger
  (36–55G law). Decision: explicit scratch lifecycle, recorded in the
  report.
- **Is sccache installation allowed?** Yes as measurement setup
  (nothing lands in the repo), No as a repo/config change (the hard
  boundary). Decision recorded here so stage 3's "diff is exactly the
  artefacts" check is unambiguous.
- **Does the ruling gate cover CI adoption?** No — CI stays as-is in
  this chunk; the ruling states the recommended CI posture for the
  operator to decide with the rollout.
- **Timing vs memory primacy?** The incidents make memory/peak and
  the *number of concurrent cold builds* the primary metric; wall
  time is the throughput bonus. The recommendation must win on the
  incident axis first.

## 8. Risks to the chunk itself

- **Benchmark noise from concurrent agents** — mitigated by §3.2's
  gate serialization, pairing, LOAD-FLAG + re-run; residual risk:
  the box's non-gated load (desktop, browsers) still varies.
  Accepted; the report flags it.
- **The concurrency exercise (two simultaneous builds) deliberately
  violates the one-heavy-build-at-a-time spirit** — bounded: exactly
  two sites, exactly one exercise set, through the memory gate where
  possible (the gate's own serialization may itself *be* the finding
  for scheme C — record either way).
- **Disk pressure during the battery** — the doctor floor (25G) is
  checked before each bench worktree creation; battery target dirs
  are cleaned per-arm (scheme A/C dirs can be 55G each).
- **A shared-scheme build corrupting a real worktree's target/** —
  the battery never points scheme C/D at any worktree of record;
  only bench worktrees are used. The chunk's own worktree stays
  baseline.
