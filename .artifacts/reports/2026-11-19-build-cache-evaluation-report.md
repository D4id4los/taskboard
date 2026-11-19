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

(To be filled as they occur — final list at the end of the battery.)

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

(To be filled.)

## 3. Arm C — shared `CARGO_TARGET_DIR`

(To be filled.)

## 4. Arm D — per-branch target dirs

(To be filled.)

## 5. Arm E — hybrid (conditional)

(To be filled or explicitly not run, with reason.)

## 6. Concurrency exercise log

(To be filled: two-worktree runs verbatim.)

## 7. Risk-axis evidence

(To be filled: poisoning / feature divergence / CI parity /
operational.)

## 8. Analysis

(To be filled.)

## 9. Recommendation

(Exactly one; consequences + rollout shape.)

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

### 10.2 Command transcripts

Harness and raw logs lived in `/tmp/p9-bench/` (measure.sh, arm-a.sh,
arm-b.sh, arm-c.sh, arm-d.sh, logs/summary.log + per-run .time files).
**Deleted before handoff** per the evidence mandate — the summary lines
reproduced in §1–§6 are the evidence. The exact command shapes are
recorded in §1 and each arm's section.
