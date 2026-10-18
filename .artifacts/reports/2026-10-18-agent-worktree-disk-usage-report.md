# [Report] Agent worktree disk usage — evidence & cleanup recommendations

- **Date**: 2026-10-18
- **Kind**: report — measured analysis of SSD consumption by dispatch-plan
  agent worktrees, with recommendations for the dispatch-plan template,
  AGENTS.md, and a new hygiene script. No code in this commit.
- **Origin**: user question of 2026-10-18 (dispatch agents fill the SSD with
  cargo build artefacts; how should the template change so agents clean up?).
- **Status**: accepted by the user 2026-10-18 — applied the same day on
  branch `chore/worktree-disk-hygiene` (doctor script, AGENTS §8/§10
  amendments, dispatch-plan template amendments, and the two in-flight
  dispatch plans, in one PR). §5's one-off cleanup was executed by the
  doctor's first production run (evidence at the end of §5).

## 1. Measured state (2026-10-18, all evidence from this machine)

Disk: `/dev/nvme0n1p4 428G total, 70G free (83% used)` (`df -BG`).
`~/.cargo` is 2.4G — the registry is not a factor. There is no
`.cargo/config.toml` (repo or home), no `sccache`; `cargo-sweep` is
installed but referenced nowhere in the repo.

`git worktree list` plus `du -sh` per worktree:

| Worktree | Branch | PR | `target/` | `target/llvm-cov-target` | `target/debug/incremental` |
|---|---|---|---|---|---|
| `-WT-fix-remediation-wave1-followups` | `fix/remediation-wave1-followups` | none open | 55G | 25G | 7.0G |
| `-WT-docs-multi-board-impl-plan` | `docs/multi-board-impl-plan` | none open | 36G | 13G | 3.2G |
| `-WT-docs-multi-board-detailed-design` | `docs/multi-board-detailed-design` | #337 MERGED | — | — | — |
| `-WT-docs-wave-multi-board-design` | `docs/wave-multi-board-design` | #333 MERGED | — | — | — |

Supporting facts:

- The two 45G+ worktrees are **in-flight chunks, not finished ones**: both
  have uncommitted tracked changes and no PR yet
  (`git status --porcelain`, `gh pr list --state all --head <branch>` → `[]`).
  `fix-remediation-wave1-followups` has already produced `lcov.info`, so
  its 25G coverage dir is spent output.
- The two MERGED-PR worktrees are pure leftovers (docs-only chunks, no
  `target/`), polluting `git worktree list` but costing ~0 bytes.
- `target/debug` alone is 30G / 23G per worktree (debuginfo-dominated:
  incremental is only 7.0G / 3.2G of it).
- Nothing in AGENTS.md, the dispatch template, or `scripts/` removes a
  worktree or deletes `target/llvm-cov-target`
  (`grep -rn 'worktree remove\|llvm-cov-target\|CARGO_INCREMENTAL\|cargo clean'`
  over AGENTS.md, scripts/, .github/, docs/ → only the quarantine-script
  invocation in the §10 gate and ci.yml).

## 2. Analysis: "cargo clean more often" is the wrong knob

Disk usage here tracks **unterminated worktree lifecycles, not build
frequency**: 91G lives in two worktrees whose chunks stalled before the
stage-3 handoff, and 38G of that is one-shot coverage output that nobody
was ever told to delete. A rule that runs `cargo clean` on a schedule or
per-gate-run would (a) destroy the incremental cache agents legitimately
reuse between the §10 gate runs of stages 2 and 3, slowing every chunk,
and (b) not fix the actual leak, because the dirs simply regrow while the
worktree lives on. The space is reclaimed exactly once per chunk — at
handoff — plus continuously for the one dir that is single-use
(`llvm-cov-target`).

The dispatch template's lifecycle today has a creation step ("Every chunk
works in `../taskboard-WT-<branch>`", stage 1 "creates the worktree") and
even a recreate rule (stage 2: "recreates it on the branch if missing"),
but **no teardown step and no pre-flight check**. That asymmetry is the
bug.

## 3. Recommendations

Exactly one primary recommendation (R1–R3, adopted together as one
template+AGENTS change), plus one optional workspace-config follow-up (R4).

### R1 — dispatch template: teardown at handoff (stage 3)

Stage 3's ship sequence ends with the handoff contract "PR open + checks
green + merge state `CLEAN`". Append a final teardown task to it:

> 7. **Teardown**: the branch now lives on `origin` and the PR is green —
> the worktree is disposable. From the project root (not the worktree):
> `git worktree remove --force ../taskboard-WT-<branch>`. `--force` is
> correct here because everything precious is pushed; local-only state at
> this point is regenerable build output. If a later round resumes the
> chunk (post-review fixes, rebase after an upstream merge), recreate it:
> `git fetch origin --prune && git worktree add ../taskboard-WT-<branch>
> <branch>`. The stage-3 report states the worktree was removed (or why
> not).

Mirror one sentence in AGENTS.md §8 next to the worktree-naming rule:
"Worktrees are disposable — the agent handing off at PR-green removes
its worktree; a resuming chunk recreates it on the branch." Also add
"worktree removed" to the stage-3 report fields in the template's
Prompt N.3 closing line.

Effect per chunk: frees the full 36–55G `target/` plus the checkout, at
the one moment it is guaranteed stale. This *is* the "cargo clean often
enough" rule — with the correct frequency: once, at handoff.

### R2 — AGENTS.md §10: delete the coverage dir after each coverage pass

The llvm-cov sequence writes to `target/llvm-cov-target`, a **separate**
dir from the normal incremental cache, and its only product is
`lcov.info`. Observed cost: 13–25G per worktree per gate cycle, regrown
by every full-gate re-run (stage 2, stage 3, and again after each
finding-fix round). Append one line to the §10 block:

```sh
rm -rf target/llvm-cov-target   # local hygiene only; CI runners are ephemeral
```

Zero-downside: it never touches the normal build cache (separate dir),
so mid-chunk incremental builds are unaffected; a later coverage re-run
rebuilds it, which it does anyway for a fresh measurement. Every dispatch
prompt already says "run the full gate suite from AGENTS.md §10", so the
template inherits this without per-prompt edits.

### R3 — pre-flight: `scripts/worktree-doctor.sh` + stage-1 gate

Covers the "forgetting to clean up after them" case (stalled/abandoned
chunks like the two currently on disk). New script, called by stage 1
before creating a worktree:

```sh
#!/usr/bin/env bash
# worktree-doctor.sh — sweep handed-off worktrees, scrub coverage dirs,
# and fail below a disk-headroom floor. Read-only towards in-flight work.
set -euo pipefail
MIN_FREE_G="${WORKTREE_DOCTOR_MIN_FREE_G:-25}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
git -C "$ROOT" fetch origin --prune --quiet

for wt in "$ROOT"-WT-*; do
  [ -d "$wt" ] || continue
  branch="$(git -C "$wt" branch --show-current)"
  if [ -n "$(git -C "$wt" status --porcelain)" ]; then
    echo "KEEP (uncommitted changes): $wt [$branch] $(du -sh "$wt" 2>/dev/null | cut -f1)"
    continue
  fi
  if git -C "$ROOT" merge-base --is-ancestor "$branch" origin/main 2>/dev/null \
     || [ -n "$(gh pr list --state merged,closed --head "$branch" --json number --limit 1)" ]; then
    git -C "$ROOT" worktree remove --force "$wt"
    echo "REMOVED (merged/closed): $wt [$branch]"
  else
    echo "KEEP (in flight): $wt [$branch] $(du -sh "$wt" 2>/dev/null | cut -f1)"
  fi
done

# Coverage output is spent once lcov.info exists; scrub it everywhere.
for wt in "$ROOT"-WT-*; do
  [ -d "$wt" ] && rm -rf "$wt/target/llvm-cov-target"
done

free_g=$(( $(df --output=avail -k "$ROOT" | tail -1) / 1024 / 1024 ))
echo "free: ${free_g}G (floor ${MIN_FREE_G}G)"
[ "$free_g" -ge "$MIN_FREE_G" ]
```

Template change, §1 dispatch mechanics + Prompt N.1 (new step before
"Create the worktree"): "Run `scripts/worktree-doctor.sh` from the
project root. If it exits nonzero (headroom below the floor), STOP and
report — do not start the chunk." The script never deletes anything with
uncommitted tracked changes, so an interrupted chunk's work is safe; its
worst case on a collision is failing one concurrent llvm-cov run, which
that agent simply re-runs (torn-profraw handling already exists via the
quarantine script).

### R4 — optional follow-up PR: shrink per-worktree artifacts

`[profile.dev] debug = "line-tables-only"` (root `Cargo.toml`) typically
halves the ~23–30G `target/debug` (debuginfo dominates; incremental is
only 3–7G of it). Keeps backtraces with file:line — coverage and the §10
gates are unaffected. Trade-off: variable inspection in a debugger gets
worse. This is a workspace-wide dev-experience change, so it deserves
its own small PR and the user's call, not a rider on the template fix.

## 4. Alternatives considered and rejected

- **Frequent/scheduled `cargo clean` mid-chunk** — the user's framing.
  Slows agents (incremental reuse between gate runs is real value: 3–7G
  per worktree of cache that pays for itself), and the dirs regrow while
  the worktree lives; the leak is the lifecycle, not the build count.
- **Shared `CARGO_TARGET_DIR` across worktrees** — dedupes dependency
  builds, but cargo flocks the target dir: parallel agents serialize, and
  a queued agent experiences a build that "hangs" — poison for agent
  workflows. Rejected.
- **`sccache`** — saves compile *time* across worktrees, not disk: each
  worktree still stores its full artifacts, plus sccache's own cache.
  Not the tool for a space problem (and not currently installed).
- **`cargo-sweep`** (installed) — bounds *stale* artifacts inside a
  long-lived checkout; the main checkout has no `target/` at all and the
  problem is whole worktrees, not old files within one. Could be added
  to the doctor later; not the fix.

## 5. Immediate one-off recovery available today (no template change)

1. `git worktree remove --force` the two MERGED-PR leftovers
   (`docs-wave-multi-board-design`, `docs-multi-board-detailed-design`) —
   ~0G, but restores a truthful `git worktree list`.
2. `rm -rf` the 25G `target/llvm-cov-target` in
   `-WT-fix-remediation-wave1-followups` — `lcov.info` is already
   produced; the dir is spent. Safe while the chunk stays in flight.
3. Same for the 13G one in `-WT-docs-multi-board-impl-plan` — that chunk
   has not completed a coverage pass yet (no `lcov.info`), so its next
   full-gate run rebuilds coverage artifacts anyway.

Recovers ~38G now (≈ back to the ~108G baseline); the remaining ~53G
(`target/debug` of the two in-flight chunks) is actively in use and
should wait until those chunks ship — at which point R1 would reclaim it
automatically.

**Executed 2026-10-18** by the doctor's first production run
(`WORKTREE_DOCTOR_ROOT=/home/viktoria/src/taskboard bash
scripts/worktree-doctor.sh`): both MERGED-PR worktrees removed, both
in-flight worktrees kept (one dirty, one clean-but-unshipped — both
correct verdicts), both coverage trees scrubbed, free space
70G → **107G** (`df` before/after).

## 6. Consequences for later work

- Dispatch-plan template gains its missing lifecycle half (teardown +
  pre-flight); every future dispatch plan copies it automatically.
- AGENTS.md §10 becomes self-cleaning locally; the "CI-identical
  three-command sequence" claim is untouched (the `rm` is post-gate
  hygiene, explicitly local-only).
- In-flight chunks that stall keep their disk until either the doctor
  sweeps them post-merge or the user intervenes — bounded by the 25G
  headroom floor at every new chunk start.
- R4, if adopted, compounds: fewer bytes per worktree × shorter-lived
  worktrees.
