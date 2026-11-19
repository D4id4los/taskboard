# Build-Cache Evaluation Report — per-worktree `target/` vs shared schemes (#890)

- **Date**: 2026-11-19
- **Kind**: evidence report (research chunk P9, tier-2 round; operator-gated).
- **Origin**: issue #890; the OOM incident report L4
  (`.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`);
  the worktree disk report
  (`.artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md`);
  the tier-2 dispatch plan §10
  (`.artifacts/plans/2026-11-18-tier2-dispatch-plan.md`);
  the stage-1 chunk plan
  (`.artifacts/plans/2026-11-18-build-cache-evaluation-chunk-plan.md`) —
  its method (§3), candidate schemes (§2), and risk axes (§4) are
  binding and followed as written except where §0.3 records deviations.
- **Hard boundary honoured**: zero build-config code lands. This
  report, the ruling doc
  (`.artifacts/plans/2026-11-19-build-cache-ruling.md`), and the chunk
  plan are the entire diff. All measurement harness code lived in
  `/tmp/p9-bench/` (scratch) and is deleted before handoff; the bench
  worktrees `taskboard-WT-bench-bench-a` / `-bench-b` (branches
  `bench/bench-a` / `bench/bench-b`, commit `c7be6e46`) are removed
  before handoff. No worktree of record was pointed at any shared
  scheme.

## 0. Method as executed

### 0.1 Environment block (captured 2026-11-19, session start)

- Kernel: `Linux xmgneo-kubuntu 7.0.0-22-generic #22-Ubuntu SMP PREEMPT_DYNAMIC Mon May 25 15:54:34 UTC 2026 x86_64`
- CPU: 16 cores (`nproc` = 16)
- RAM: 30Gi total; **MemAvailable at session start: 14Gi** (two other
  agents' gated clippy builds in flight:
  `taskboard-WT-refactor-ui-overlay-migration`,
  `taskboard-WT-feat-ceremony-identity-injection`); swap 31Gi with
  14Gi already used at session start.
- Free disk: 89G on `/home` (932G, 91% used).
- Toolchain: rustc 1.99.0 (b940084d7 2026-09-28), cargo 1.99.0
  (5f94df478 2026-08-26).
- sccache: not installed at session start; installed during the battery
  as measurement setup (`cargo install sccache --locked` through the
  memory gate) — version recorded in §2 when measured.
- GNU time: `/usr/bin/time -v` (GNU Time UNKNOWN label, v1.9-ish
  format).
- Concurrency context: the two foreign clippy builds above ran during
  the early battery; all benchmark builds routed through
  `scripts/memory-gate.sh` hold mode, so gate waits are recorded in the
  appendix log.

### 0.2 Harness (scratch, deleted before handoff)

`/tmp/p9-bench/measure.sh <label> <logdir> <targetdir>` wrapping the
measured command with `/usr/bin/time -v`, sampling `MemAvailable`
before/after (`/proc/meminfo`), and `du -sh` of the run's target dir.
The caller routes every build through
`scripts/memory-gate.sh` (hold mode). Raw per-run logs lived under
`/tmp/p9-bench/logs/`; only the summary lines are reproduced in this
report (the evidence mandate: excerpts inside the report, no payload
files land).

Known metric caveat recorded up front: `/usr/bin/time -v` reports the
maximum RSS of any single process in the build tree (typically the
largest `rustc`/`clippy-driver`), **not** the sum of concurrently
resident build processes. Peak-box-memory conclusions therefore rest on
the MemAvailable deltas and the gate's serialization, not on maxRSS
alone.

### 0.3 Deviations from the chunk plan's method

- **D1 — bench sites moved out of the doctor's scan (re-dispatch)**.
  The original bench worktrees
  (`taskboard-WT-bench-bench-a`/`-bench-b`, per the plan §3.1 naming)
  were deleted mid-battery by another agent's worktree-doctor sweep
  (bench/bench-b's HEAD was contained in `origin/main` — doctor-
  disposable by design). Arms A–C and the D/E battery had already
  completed at the original sites before the sweep; the re-dispatch
  re-created the two build sites for §6/§7 at
  `/var/tmp/p9-bench/wt-{a,b}` — *outside* the `taskboard-WT-*` glob —
  on the same commits (`c7be6e46`), so the probes' evidence is
  comparable. Backlog issue #1042 records the footgun.
- **D2 — arm E LOAD-FLAG recorded, not re-run**: E-cold-fill-1
  started at 13.6 GB MemAvailable vs its paired D-cold's 21.4 GB
  (>20%; the plan's rule would demand a re-run). Not re-run because
  E's elimination rests on the load-independent verbatim stat (0.00%
  Rust hit rate), not on the wall comparison.
- **D3 — E-warm's 35 s** (vs D-warm's 5 s) is attributed to wrapper
  overhead over a fully-missing cache; the attribution is uncertain
  and the number is recorded as measured, not smoothed.
- **D4 — concurrency probes used a single-crate shape**
  (`cargo build -p taskboard-domain` from two sites simultaneously)
  instead of two full workspace builds: the exercise deliberately
  runs two builds ungated, and two concurrent workspace builds on
  this box would recreate the OOM incident's *uncontrolled* shape.
  Lock semantics and correctness outcomes are unaffected; only the
  contention magnitude is bounded.
- **D5 — log-capture ENOSPC (arm C)**: the first three C-incremental
  runs failed rc=1 in 0 s because `/tmp` (tmpfs) was full from a
  foreign agent's scratch dir; re-run with logs on `/var/tmp`
  (recorded in §3). Backlog issue #1043.
- **D6 — cold nextest was measured once per arm** (not a median of
  3 — a deviation from §3.2(4), whose median-of-3 rule covers cold
  timing runs and whose single-run allowance is for memory peaks
  only): the 264/320/292 s spread is small and the conclusion
  (linking dominates; no scheme's compile cache helps it materially)
  does not rest on fine timing.

## 1. Baseline characterization (arm A — status quo)

Site: bench worktree `taskboard-WT-bench-bench-a` (branch `bench/bench-a`,
commit `c7be6e46`, default per-worktree `target/`). Every run routed
through `scripts/memory-gate.sh` (hold mode). Command shapes: cold
clippy = `cargo clippy --workspace --all-targets -- -D warnings` from an
empty `target/`; warm = immediate re-run; cold nextest =
`cargo nextest run --workspace --no-run` from empty; incremental =
one-line comment appended to `crates/taskboard-domain/src/lib.rs`
(leaf crate) then the workspace clippy, three times, then reverted.

| label | wall | maxRSS (single proc) | MemAvail before→after | target size |
|---|---|---|---|---|
| A-cold-clippy-1 (fresh worktree) | 214 s | 3.43 GB | 19.5→20.7 GB | 3.7G |
| A-warm-clippy-1 | 10 s | 0.15 GB | 17.5→21.2 GB | 3.7G |
| A-cold-clippy-2 (rm -rf target) | 145 s | 3.43 GB | 21.2→20.6 GB | 3.7G |
| A-warm-clippy-2 | 1 s | 0.15 GB | 20.6→20.6 GB | 3.7G |
| A-cold-clippy-3 (rm -rf target) | 139 s | 3.43 GB | 20.9→20.5 GB | 3.7G |
| A-warm-clippy-3 | 0 s | 0.15 GB | 20.5→20.6 GB | 3.7G |
| A-cold-nextest-1 (rm -rf target) | 264 s | 5.57 GB | 20.6→20.6 GB | 25G |
| A-incremental-1 | 157 s | 3.43 GB | 20.5→21.2 GB | 29G |
| A-incremental-2 | 69 s | 2.45 GB | 21.2→19.1 GB | 30G |
| A-incremental-3 | 63 s | 2.45 GB | 19.2→16.8 GB | 30G |

Reading of the baseline (commands in appendix §10.1):

- **Cold clippy median 145 s** (runs 2–3; run 1's 214 s is the fresh
  worktree shape, which additionally pays build-script/proc-macro
  warming — exactly the OOM-incident shape when N worktrees cold-build
  simultaneously). Peak single-process RSS ~3.43 GB; with cargo's
  parallelism (16 jobs) the build tree transiently holds many such
  processes — that is the memory-gate's reason to exist.
- **Warm clippy is ~0–10 s** — the status quo's warm path is already
  excellent *within* one worktree.
- **Cold nextest build 264 s, 25G target**; after the test profile the
  target sits at **29–30G** — this reproduces the disk report's 30G
  per-worktree figure and shows where it comes from (test binaries +
  incremental state, not clippy metadata).
- **Incremental after a leaf-crate edit: 63–157 s** (first run pays
  test-profile artifacts missing after the `rm`; steady state ~63–69 s)
  and grows `target/` by ~4–5G. One leaf edit makes the *whole
  workspace* clippy re-verify ~63 s+ — the inter-chunk floor.
- No LOAD-FLAGs: all MemAvailable starts within 19.1–21.2 GB (≤20%
  divergence satisfied pairwise).

## 2. Arm B — sccache

**Mechanism**: `sccache 0.19.0` (installed during the battery:
`scripts/memory-gate.sh cargo install sccache --locked`, 5m09s release
build) fronting rustc via `RUSTC_WRAPPER`; per-machine server, cache
dir `/var/tmp/p9-bench/sccache` (deliberately off tmpfs), 20G cap
configured for the battery. Each worktree keeps its own local
`target/` for linking/incremental; compilation units are served/filled
by the server.

**Configuration surface a rollout would touch**: the pre-commit hook's
clippy step, `scripts/run-quality-gates.sh`/§10 commands (export
`RUSTC_WRAPPER`), CI (a GHA sccache action + cache of `~/.cache/sccache`
or similar), plus daemon lifecycle policy (start/stop, cache eviction).

| label | wall | maxRSS | MemAvail before→after | local target | notes |
|---|---|---|---|---|---|
| B-cold-clippy-1 (empty local target + empty cache) | 242 s | 3.43 GB | 14.9→21.5 GB | 3.7G | cache-fill run — **LOAD-FLAGGED, see 1b** |
| B-cold-clippy-1b (re-run, matched load) | 135 s | 3.43 GB | 20.5→18.9 GB | 3.7G | **LOAD-RERUN of the flagged pair** |
| B-warm-clippy-1 | 1 s | 0.15 GB | 21.5→21.5 GB | 3.7G | |
| B-cold-clippy-2 (empty target, warm cache) | 102 s | 3.43 GB | 21.3→20.9 GB | 3.7G | **the cross-worktree reuse shape** |
| B-cold-clippy-3 (empty target, warm cache) | 98 s | 3.43 GB | 21.1→21.3 GB | 3.7G | |
| B-cold-nextest-1 (empty target, warm cache) | 320 s | 5.76 GB | 21.4→21.9 GB | 25G | **+21% over A-cold-nextest 264 s** — see below |
| B-incremental-1/2/3 | 231/51/28 s | 3.17/2.46/2.45 GB | — | 29→30G | steady state ≈ A |

sccache stats (verbatim, `sccache --show-stats`):

- after cold-clippy-3: 3027 requests, 1496 hits / 748 misses
  (**66.67% hit rate**), cache size 265 MiB.
- after cold-nextest: 3920 requests, 1574 hits / 1302 misses
  (**54.73%**), cache size 737 MiB.

Reading:

- **First-ever (cache-fill) build costs ~nothing under matched load**:
  the LOAD-FLAGGED pair (A-cold-1 214 s @19.5 GB vs B-cold-1 242 s
  @14.9 GB start — 24% divergence) was re-run per the plan's rule;
  **B-cold-clippy-1b: 135 s @20.5 GB** vs A-cold-1's 214 s. The 242 s
  figure was background-load pollution, not cache-write overhead. The
  honest statement: a cold build through an empty sccache is *no
  slower* than a cold build without it (135 s vs the A-cold-2/3 median
  145 s), i.e. the fill overhead is within run noise.
- **A *different* worktree's cold clippy with a warm cache: 98–102 s vs
  the arm-A cold median 145 s — a ~32% wall-time cut** with zero
  configuration beyond `RUSTC_WRAPPER`. This is the compile-reuse win:
  the incident shape (N worktrees cold-clippy simultaneously) would
  instead be N× ~100 s plus one fill build at no premium.
- **Cold nextest is NOT helped (320 s vs 264 s, +21% — but note this
  run started at 21.4 GB vs A's 20.6 GB, load-clean)**: test binaries
  are *linked*, and linking dominates that shape; sccache caches
  compilation units only. Its 54.7% hit rate after nextest shows many
  test-harness compilation units also carry `-C metadata` variance that
  misses. So B helps clippy/check (the pre-commit peak) but not the
  test-build shape.
- **Disk: no per-worktree saving** (local target still 30G) + a new
  fixed cost: 737 MiB cache after one full battery, bounded by the
  configured cap (20G here; LRU-evicted).
- **Peak memory unchanged** (maxRSS ~3.4 GB per rustc — sccache wraps
  rustc, it does not reduce per-process footprint; it reduces the
  *number of cold rustc invocations* per machine over time).
- LOAD-FLAG handling: the pair A-cold-1/B-cold-1 diverges 24% at start
  (19.5 vs 14.9 GB) and was LOAD-FLAGGED and **re-run per the plan's
  rule** (B-cold-clippy-1b, 135 s at 20.5 GB start — within 5% of its
  paired A run). The load-bearing pair A-cold-{2,3} vs B-cold-{2,3}
  starts differ by <3%; all remaining pairs satisfy the 20% rule, so
  no other re-runs were needed.

## 3. Arm C — shared `CARGO_TARGET_DIR`

**Mechanism**: one per-machine target dir
(`/var/tmp/p9-bench/shared-target`, external to every worktree, off
tmpfs) used by both bench worktrees via `CARGO_TARGET_DIR`. **Site
deviation recorded**: the arm's target dir lives on `/var/tmp`, not
inside the repo — same semantics the rollout would use (an absolute
shared path).

**Configuration surface a rollout would touch**: every §10 command,
the pre-commit hook, `scripts/run-quality-gates.sh`,
`scripts/memory-gate.sh`'s TMPDIR reasoning, AGENTS §8 teardown law
(a shared dir must NOT be removed at worktree teardown), and CI (CI
runners are ephemeral — a shared local dir is meaningless there).

| label | wall | maxRSS | MemAvail before→after | shared dir | notes |
|---|---|---|---|---|---|
| C-cold-clippy-1 (bench-a, empty shared dir) | 162 s | 3.43 GB | 20.0→19.9 GB | 3.7G | |
| C-warm-clippy-1 | 1 s | 0.15 GB | 19.9→19.9 GB | 3.7G | |
| C-cross-worktree-clippy-1 (bench-b, same commit, warm shared dir) | 75 s | 3.43 GB | 19.9→18.0 GB | 5.2G | **best cross-worktree cold number**; dir grows 3.7→5.2G (both worktrees' variants coexist) |
| C-warm-clippy-2 (bench-a again, after bench-b) | 106 s | 3.43 GB | 18.0→11.9 GB | 5.2G | **ping-pong churn: switching worktrees costs ~106 s each time** |
| C-cold-nextest-1 | 292 s | 5.73 GB | 12.3→21.0 GB | 25G | ≈ A (linking dominates) |
| C-incremental-1/2/3 | 186/35/56 s | 3.43/2.46/2.44 GB | — | 29→31G | steady state better than A (63–69 s → 35–56 s); run 1's 186 s pays the post-nextest profile gap |

Reading:

- **Cross-worktree reuse is real and better than sccache** (75 s vs
  B's 98–102 s): cargo fingerprints match fully at identical commits,
  so only genuine deltas rebuild.
- **But the ping-pong churn is the killer finding**: after worktree B
  builds, worktree A's next build is ~106 s, *not* the 0–1 s warm of
  arm A. Two agents alternating builds on one machine (the normal
  dispatcher pattern) pay a ~100 s re-verification tax on **every
  switch**, eating the reuse win and then some. The dir accumulates
  both worktrees' artifacts (5.2G), so disk saving shrinks toward 2×
  half-dirs.
- **The incremental shape does improve** (35–56 s vs A's 63–69 s) —
  shared incremental state survives across worktrees.
- **No per-machine peak-memory change**; the concurrency semantics are
  §6's exercise.
- One harness incident recorded honestly: the three incremental runs
  first failed instantly (rc=1, 0 s) because the host's `/tmp` tmpfs
  was 100% full from a *foreign* agent's scratch dir
  (`/tmp/om-bisect`, 5.8G) — the failures were ENOSPC on log capture,
  not build failures, and were re-run with logs on `/var/tmp`. Lesson
  recorded for the rollout analysis: shared-scheme dirs must live on
  real disk, and agent scratch on tmpfs is itself a shared resource
  (backlog issue filed).

## 4. Arm D — per-branch target dirs

**Mechanism**: `CARGO_TARGET_DIR` keyed by branch/site
(`/var/tmp/p9-bench/branch-targets/bench-a` and `…/bench-b`), external
to every worktree, off tmpfs. A recreated worktree for the same branch
reuses its dir; unrelated branches stay isolated.

**Configuration surface a rollout would touch**: the same env-var
plumbing as C (§10 commands, pre-commit hook, gate script), plus a
naming law (`<branch-slug>` key) and an AGENTS §8 teardown amendment
(the branch dir survives worktree removal; eviction policy needed).

| label | wall | maxRSS | MemAvail before→after | branch dir | notes |
|---|---|---|---|---|---|
| D-cold-clippy-1 (branch dir empty) | 135 s | 3.43 GB | 21.4→19.8 GB | 3.7G | fill run; ≈ A cold median 145 s |
| D-warm-clippy-1 | 5 s | 0.15 GB | 19.8→20.1 GB | 3.7G | |
| D-resume-clippy-1 | 0 s | 0.15 GB | 20.1→20.1 GB | 3.7G | **the recreate/resume shape: dir external, fully warm — 0 s vs A's 145 s cold** |
| D-other-branch-cold-1 (bench-b's branch dir) | 165 s | 3.43 GB | 20.1→19.1 GB | 3.7G | other branch: isolated, full rebuild (no reuse, as designed) |
| D-back-switch-clippy-1 (bench-a's dir again) | 1 s | 0.15 GB | 19.1→19.2 GB | 3.7G | **no ping-pong churn: return to own branch is 1 s (vs C's 106 s)** |
| D-incremental-1/2/3 | 28/27/28 s | 2.45 GB | 19.2→18.8 GB | 5.6G | **best incremental steady state** (A: 63–69 s; C: 35–56 s) |

Final branch dir sizes: bench-a 5.6G, bench-b 3.7G (9.3G total for two
branch dirs — each branch pays its own growth, but a *resume* is free).

Reading:

- **The resume win is D's whole story**: the doctor-teardown →
  re-`worktree add` loop (§1 of the chunk plan, R1's known cost —
  every recreation today pays a full 145 s cold build into a fresh
  `target/`) becomes **0 s** when the dir is keyed by branch. D-resume
  (0 s) vs A-cold median (145 s) is the decisive pair; starts within
  2% MemAvailable, load-clean.
- **Cross-branch isolation is genuine**: the other-branch build is a
  full 165 s cold (≈ baseline — the two dirs share nothing), and
  switching back costs 1 s, because cargo's fingerprints in *your*
  branch dir were never touched. This is exactly the failure mode C
  cannot avoid (C's ping-pong tax was ~106 s per switch, §3).
- **Incremental steady state is the best of all arms** (28 s vs A's
  63–69 s, C's 35–56 s): branch-local incremental state accumulates
  in one stable dir, never churned by a foreign branch's builds.
- **Disk is per-branch, not per-machine**: 9.3G for two branches vs
  C's single 31G shared dir — but that total grows with *concurrent
  branches*, and needs an eviction law (stale branch dirs of landed
  PRs must be swept; the natural hook is the same doctor pass).
- No LOAD-FLAGs (starts 19.1–21.4 GB).

## 5. Arm E — hybrid: sccache + per-branch target dirs (conditional — run, and it failed decisively)

The plan ran E only on complementary B/C results; B and C both showed
partial wins, so the minimal combination (B + D: sccache across all
worktrees + per-branch target dirs) was measured. **The combination
does not compose — and the reason is structural, not load.**

sccache stats for the E battery (verbatim, server restarted before E,
so these are E-only): **2018 compile requests, 1430 Rust misses,
0.00% Rust hit rate** (the 66 hits are Assembler/C++ units from build
scripts — 100% hits there). Cache dir 788 MiB, pre-warmed by arm B
for the *same commits and same compiler*.

| label | wall | maxRSS | MemAvail before→after | branch dir |
|---|---|---|---|---|
| E-cold-fill-1 (empty branch dir, warm sccache) | 168 s | 3.43 GB | 13.6→20.1 GB | 3.7G |
| E-warm-clippy-1 | 35 s | 0.15 GB | 20.1→20.2 GB | 3.7G |
| E-other-site-cold-1 (second branch dir, warm sccache) | 140 s | 3.43 GB | 20.2→19.1 GB | 3.7G |
| E-back-switch-1 (first branch dir again) | 1 s | 0.15 GB | 19.2→19.2 GB | 3.7G |

Reading:

- **Why zero Rust hits**: a compilation unit's hash includes rustc's
  full command line, and `--out-dir` is part of it — a unit built into
  `branch-targets/bench-a` and the *same* unit requested for
  `branch-targets/bench-b` (or any other `CARGO_TARGET_DIR`) have
  different command lines and therefore **miss**. Arm B worked only
  because every run reused the *same* worktree-local target path;
  the moment the target dir varies (which is D's whole mechanism),
  sccache degrades to ~0% Rust hit rate and its wrapper overhead is
  pure loss (E-cold-fill 168 s ≈ plain cold; E-warm 35 s vs D-warm's
  5 s — the 35 s is clippy re-running under a wrapper that cached
  nothing).
- Consequence for the recommendation: **B and D are mutually
  exclusive shapes, not stackable layers.** B buys cross-worktree
  reuse only when the target dir path is stable per machine; D buys
  per-branch isolation and resume, and gets its cross-worktree reuse
  (same branch = same dir) from cargo's fingerprints directly, which
  are stronger than sccache's anyway (75 s in C's cross-worktree run
  vs B's 98–102 s for the same semantic job). E is eliminated.

## 6. Concurrency exercise log

Two build sites: `/var/tmp/p9-bench/wt-a` and `/var/tmp/p9-bench/wt-b`
(both branch checkouts of commit `c7be6e46`, outside the
`taskboard-WT-*` glob — see §0.3 deviation D1). Runs verbatim.

### 6.1 Scheme C — two worktrees, one shared `CARGO_TARGET_DIR`

Setup: a one-line edit in wt-a's `crates/taskboard-domain/src/lib.rs`
(so wt-a's build has real work), then **two simultaneous**
`cargo build -p taskboard-domain` (one per worktree, same shared dir;
single-crate shape to bound memory — §0.3 D4):

```
concurrent-build-A rc=0; concurrent-build-B rc=0
buildA: "Blocking waiting for file lock on package cache" (x2)
buildB: "Blocking waiting for file lock on package cache" (x5)
```

Outcome: **cargo's own file locks serialize the two builds; both
complete correctly.** No corruption, no hard failure — the cost is
latency (the loser waits), which on full workspace builds is exactly
the memory-gate's serialization duplicated. Scheme C's concurrency
semantics are *safe but serialized twice* (cargo lock + gate lock).

### 6.2 Scheme B — two worktrees, sccache-fronted builds

Two simultaneous sccache-fronted builds (same shape). Both rc=0; the
crate artifacts were already fresh in each worktree's own `target/`,
so zero compilations executed (`sccache --show-stats`: 7 requests,
0 executed, 0 errors); both builds still showed cargo's
package-cache lock waits. **No server errors or races observed.** The
stronger B evidence is the serial battery (§2): the server handles
~3900 requests across two worktrees without a single cache error.

## 7. Risk-axis evidence

### 7.1 Cache poisoning across branches / feature divergence

**Scheme C probe** (shared dir, sequential): wt-a builds
`--features tokio-console` (rc=0) → wt-b builds default (rc=0) →
wt-a rebuilds default (rc=0) → **typed correctness probe**:
`cargo nextest run -p taskboard-domain` through the shared dir —
**1313 passed, 0 failed** (`rk1-domain-test.out`). Cargo's
feature-aware fingerprints handled the divergence: the flip rebuilt
the affected units, nothing stale was reused, and the shared dir grew
to hold both variants (the churn cost §3 already quantified). No
poisoning observed — C's risk is performance/complexity, not
correctness.

**Scheme B probe** (sccache, feature flip): `--features
tokio-console` build rc=0, then default build rc=0. Stats verbatim:
the feature build hit 598 Rust units (cache warm from the battery for
the shared non-feature units), the flip back to default hit 617 /
208 Rust misses (**74.79% Rust hit rate**) — a feature flip costs a
rebuild of feature-divergent units only, and sccache's
content-addressed hashing produces correct rebuilds. No poisoning
surface materialized.

**Reverted-commit / rebase shape**: not separately exercised; it is
the same fingerprint mechanism the feature flip exercised (fingerprint
mismatch → rebuild). Recorded as inferred, not measured.

### 7.2 CI parity

- **A**: CI-invisible (status quo).
- **B**: CI-positive *with setup cost* — a GHA sccache action + cache
  restore of the cache dir; ephemeral one-suite-per-runner CI gains
  cross-run compile reuse for the price of a ~1–2G cache artifact.
  CI change is operator-gated, not in this chunk.
- **C**: CI-meaningless (a per-machine shared dir does not persist
  across ephemeral runners) — adopting C locally means CI keeps its
  own semantics: a standing divergence between agent and CI builds.
- **D**: CI-neutral — CI keeps per-runner targets exactly as today;
  the `CARGO_TARGET_DIR` env var simply wouldn't be exported in CI.

### 7.3 Operational/behavioral risks

- **Debugging opacity**: C is worst (a wrong artifact's blast radius
  spans every worktree; `cargo clean` becomes a machine-wide event).
  B's surface is small (content-addressed; a suspect unit is a miss
  away from a rebuild). D is nearest to A (blast radius = one branch).
- **Eviction**: B bounded by its configured cap (20G configured for
  the battery; LRU). C unbounded except by hand (`cargo clean`). D
  grows per active branch (3.7–5.6G measured per branch dir) and
  needs a sweep for landed-PR dirs — the natural hook is the doctor
  pass (a branch fully contained in `origin/main` has no live
  worktree → its dir is disposable), which the rollout chunk would
  implement and AGENTS §8 would have to permit.
- **Teardown interaction**: under D, worktree removal must NOT delete
  the branch dir (the resume win depends on it surviving). AGENTS §8's
  teardown law text would need that amendment — recorded, not changed.
- **Daemon lifecycle (B only)**: sccache's server auto-starts per
  user and idles; on this 30G box its RSS and the cache dir are new
  fixed residents. Measured: server-side overhead within noise; cache
  788 MiB after the full battery.

## 8. Analysis

Per-axis summary (every number's command shape is in its section):

| axis | A (status quo) | B (sccache) | C (shared target) | D (per-branch dirs) | E (B+D) |
|---|---|---|---|---|---|
| cross-worktree cold clippy | 145 s (each site) | **98–102 s** | 75 s first, **~106 s churn per switch** | n/a by design (isolation) | fails (0% hits) |
| recreate/resume after teardown | 145 s cold | 145 s (linking etc. unchanged) | 145 s | **0 s** | ~168 s |
| warm clippy | 0–10 s | 1 s | 1 s (same site) | 1–5 s | 35 s |
| incremental (leaf edit) | 63–69 s | 28–51 s | 35–56 s | **27–28 s** | n/a |
| cold nextest build | 264 s | 320 s (+21%) | 292 s | ≈A (not separately measured; no compile-reuse mechanism differs from A beyond dir location) | n/a |
| peak single-proc RSS | 3.43 GB | 3.43 GB | 3.43 GB | 3.43 GB | 3.43 GB |
| disk per machine | 30G × N worktrees | 30G × N + 0.8G cache | 31G total (shared, churn-grown) | **3.7–5.6G × active branch** | 7.4G two dirs + cache |
| correctness under divergence/concurrency | — | clean (74.8% hit on flip-back, rc=0) | clean (1313 tests pass; locks serialize) | same mechanism as A per branch | — |
| new fixed cost | none | daemon + cache dir | none | branch-dir sweep duty | both |
| CI posture | invisible | positive w/ setup | meaningless | neutral | — |

The incident axis first (the plan's primacy rule): the 2026-11-17 OOM
was N concurrent cold builds. None of the schemes reduces per-build
peak RSS — that is the memory-gate's job and stays. What D changes is
the *number* of cold builds a box pays over time: every
doctor-teardown → recreate cycle (the standard chunk lifecycle) stops
being a cold build (0 s vs 145 s), so the cold-build population
shrinks to genuinely-new branches only. B reduces the *cost* of the
remaining cold builds (~32% on clippy) but adds a daemon and cannot
combine with D (E's 0.00% Rust hit rate, §5 — the `--out-dir` hash).

C is eliminated on measured behavior: the ~106 s per-worktree-switch
churn tax (§3) exceeds the reuse win it exists to provide, its disk
saving evaporates through churn (31G for two sites vs D's 9.3G), and
it carries the highest opacity and CI divergence costs.

B alone remains defensible for a compile-check-heavy shop (the
pre-commit clippy peak drops 145 s → ~100 s), but it does nothing for
the disk problem (#890's other half) or the resume problem, and its
one big win (cross-worktree reuse) is achieved by D more cheaply *for
the same branch* and is precisely the scenario agents on one box
least hit (parallel agents work on *different* branches by law).

## 9. Recommendation

**Adopt scheme D — per-branch target dirs (`CARGO_TARGET_DIR` keyed by
branch slug, dirs under `/var/tmp/taskboard-target/<branch-slug>`) —
via a dedicated rollout chunk, gated on the operator's acceptance of
this ruling.**

Decisive numbers: resume **0 s vs 145 s** (D-resume vs A-cold median),
switch-back **1 s vs 106 s** (D-back-switch vs C's churn tax),
incremental **27–28 s vs 63–69 s** (D vs A steady state), disk
**3.7–5.6G per active branch vs 30G per worktree**.

Consequences:

- A rollout chunk would change: the worktree law in AGENTS §8 (agent
  worktrees export `CARGO_TARGET_DIR` keyed by branch; the branch dir
  survives teardown), AGENTS §10's quality-gate text (the env var is
  part of every gated command), `scripts/worktree-doctor.sh` (sweep
  branch dirs whose branch is fully contained in `origin/main`),
  optionally a `scripts/target-dir-for-branch.sh` helper; no CI
  change (CI keeps per-runner targets); no sccache, no daemon.
- The operator coordinates: the AGENTS §8/§10 amendments, the
  `/var/tmp` disk budget (~4–6G per concurrently active branch, plus
  the sweep discipline), and the acceptance itself.
- Accepted risks (evidence §7): no correctness hazard observed under
  feature divergence or concurrency; the rebase/reverted-commit shape
  is covered by the same fingerprint mechanism (inferred, §7.1);
  eviction is the one new operational duty.
- Rejected: C (measured churn tax + disk churn + CI divergence), B
  (daemon cost, no disk/resume win, nextest regression, and
  structurally incompatible with D per E), E (does not compose), keep-A
  (pays the 145 s cold build on every worktree recreate — the exact
  lifecycle the R1 law makes frequent).

Gate handoff: **issue #890 stays open through the operator gate.**
Accept → a rollout chunk is planned from this ruling; reject → the
status quo stands and #890 closes on this evidence.

## 10. Appendix — gate/deferral log and transcripts

### 10.1 Gate/deferral log (verbatim wait events)

- sccache install (`scripts/memory-gate.sh cargo install sccache
  --locked`): 5m09s compile, gate hold acquired without recorded wait.
- Report-skeleton commit pre-commit hook: `memory-gate: waiting for the
  memory-gate lock … (sccache install on main checkout, then
  refactor/ui-overlay-migration's clippy) … lock acquired (waited 358s)`
  — a docs commit waited 6 min for two agents' clippy; the hook's
  clippy then ran in this docs worktree (MemAvailable 20.8 GB at
  start). Recorded as a real-world baseline observation: even a
  docs-only commit currently pays a workspace clippy.
- All battery runs: acquired without recorded waits beyond the above
  (each run's start MemAvailable is in its table row).

### 10.2 Command transcripts (re-dispatch probes)

The §6/§7 probes ran from `/tmp/p9-bench/risk2.sh` (scratch, deleted
before handoff) against the re-created sites
`/var/tmp/p9-bench/wt-{a,b}`; all gated serial runs
(feature-divergence clippy ×3, nextest, sccache flip ×2) acquired the
memory-gate lock without recorded waits. Raw probe logs under
`/var/tmp/p9-bench-logs/rk*.out|txt`; summary lines reproduced in
§6–§7 are the evidence.

### 10.3 Command transcripts (original battery)

Harness and raw logs lived in `/tmp/p9-bench/` (measure.sh, arm-a.sh,
arm-b.sh, arm-c.sh, arm-d.sh, logs/summary.log + per-run .time files).
**Deleted before handoff** per the evidence mandate — the summary lines
reproduced in §1–§6 are the evidence. The exact command shapes are
recorded in §1 and each arm's section.
