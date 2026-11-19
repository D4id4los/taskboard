# Build-Cache Ruling — P9 (#890) — for operator acceptance

- **Date**: 2026-11-19. **Status**: awaiting operator gate.
- **Evidence**: `.artifacts/reports/2026-11-19-build-cache-evaluation-report.md`
  (this ruling carries no new evidence; § references below are that
  report's).

## The question

Does sharing build artifacts across worktrees (sccache / shared
`CARGO_TARGET_DIR` / per-branch dirs / hybrid) beat the per-worktree
`target/` status quo on this box — 30G RAM, ≤3 agents, the memory
gate — without correctness or operational regressions?

## The recommendation (exactly one)

**Adopt scheme D — per-branch target dirs**:
`CARGO_TARGET_DIR=/var/tmp/taskboard-target/<branch-slug>` for agent
worktrees. No sccache, no shared machine-wide dir, no daemon.

### The three decisive numbers

1. **Resume after teardown: 0 s vs 145 s** — a recreated worktree for
   an existing branch reuses its dir fully (D-resume, §4) vs today's
   full cold clippy (A-cold median, §1). This is the R1 lifecycle's
   recurring tax, gone.
2. **Switch-back after another branch's build: 1 s vs 106 s** —
   branch isolation has no ping-pong churn (D-back-switch, §4) vs
   scheme C's measured per-switch tax (§3).
3. **Incremental leaf-edit rebuild: 27–28 s vs 63–69 s** (§4 vs §1),
   and disk 3.7–5.6G per *active branch* vs 30G per *worktree* (§8
   table).

Correctness under divergence and two-worktree concurrency: clean
(§6–§7; 1313 tests pass through a churned shared dir; cargo's locks
serialize concurrent builds).

## What a rollout chunk would change

- AGENTS §8: agent worktrees export the branch-keyed
  `CARGO_TARGET_DIR`; the branch dir survives worktree teardown.
- AGENTS §10: the env var becomes part of the gated command text.
- `scripts/worktree-doctor.sh`: sweep branch dirs whose branch is
  fully contained in `origin/main` (eviction; ~4–6G each).
- Optionally a `scripts/target-dir-for-branch.sh` helper.
- CI: unchanged (per-runner targets; the env var simply is not
  exported there).

## What stays operator-coordinated

- The AGENTS §8/§10 law amendments themselves.
- The `/var/tmp` disk budget and the sweep discipline.
- Explicitly rejected by evidence: scheme C (churn tax 106 s/switch,
  disk churn, CI divergence), sccache B (daemon, no disk/resume win,
  nextest +21%, and structurally incompatible with D — hybrid E
  measured 0.00% Rust hit rate because `--out-dir` is hashed, §5),
  status quo A (pays 145 s on every recreate).

## The gate

Issue **#890 stays open through this gate**. On acceptance, a rollout
chunk is planned from this ruling (the chunk's diff is the scripts +
law text above). On rejection, the status quo stands and #890 closes
on the evidence report.
