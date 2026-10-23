#!/usr/bin/env bash
# Disk-hygiene sweep for agent worktrees (2026-10-18 disk-usage report:
# .artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md).
#
# Usage: scripts/worktree-doctor.sh   (from the main checkout, or set
# WORKTREE_DOCTOR_ROOT). Run it before creating a new worktree and any
# time disk headroom is in doubt.
#
# What it does:
#   1. removes sibling worktrees whose checked-out HEAD is fully merged
#      into origin/main and that have no uncommitted changes — a worktree
#      whose branch carries any commit absent from origin/main (or that
#      sits on a primary branch, or is locked) is never touched;
#   2. deletes target/llvm-cov-target from surviving worktrees (the
#      coverage tree is spent once lcov.info exists; separate from the
#      incremental cache, so nothing else is invalidated) — skipped when
#      an llvm-cov run appears active;
#   3. exits nonzero when free space on the checkout's filesystem is
#      below the floor, so callers stop instead of filling the disk.
#
# Env overrides:
#   WORKTREE_DOCTOR_ROOT       anchor checkout (default: this repo root)
#   WORKTREE_DOCTOR_MIN_FREE_G free-space floor in GiB (default: 25)
set -euo pipefail

MIN_FREE_G="${WORKTREE_DOCTOR_MIN_FREE_G:-25}"
ROOT="$(cd "${WORKTREE_DOCTOR_ROOT:-$(dirname "$0")/..}" && pwd)"

git -C "$ROOT" fetch origin --prune --quiet

for wt in "$ROOT"-WT-*; do
  [ -d "$wt" ] || continue
  branch="$(git -C "$wt" branch --show-current)"
  if [ -z "$branch" ] || [ "$branch" = main ]; then
    echo "KEEP   (no branch or primary branch): $wt"
    continue
  fi
  if [ -n "$(git -C "$wt" status --porcelain)" ]; then
    echo "KEEP   (uncommitted changes): $wt"
    continue
  fi
  # Safe predicate: the worktree's HEAD itself (not merely the branch
  # name's PR history) must be fully merged into origin/main, and must
  # not BE the origin/main tip — a worktree at the tip is either fresh
  # (just created, nothing committed yet) or a resume on a landed
  # branch, both of which look identical to "done" to any sweep and
  # are therefore kept (2026-10-23 active-worktree deletions, #415).
  head_commit="$(git -C "$wt" rev-parse HEAD)"
  if [ "$(git -C "$ROOT" rev-parse origin/main)" = "$head_commit" ]; then
    echo "KEEP   (HEAD at origin/main tip — fresh or resume): $wt"
    continue
  fi
  if ! git -C "$ROOT" merge-base --is-ancestor "$head_commit" origin/main; then
    echo "KEEP   (in flight):           $wt"
    continue
  fi
  # Re-check liveness immediately before removal: status was sampled
  # above, so anything written since would be destroyed by --force.
  if [ -n "$(git -C "$wt" status --porcelain)" ]; then
    echo "KEEP   (changed during sweep): $wt"
    continue
  fi
  if git -C "$ROOT" worktree remove "$wt" 2>/dev/null; then
    echo "REMOVE (HEAD merged):         $wt"
  else
    echo "KEEP   (remove refused — likely locked or in use): $wt"
  fi
done

if pgrep -f llvm-cov >/dev/null 2>&1; then
  echo "SKIP   coverage scrub (an llvm-cov run looks active)"
else
  for wt in "$ROOT"-WT-*; do
    if [ -d "$wt/target/llvm-cov-target" ]; then
      rm -rf "$wt/target/llvm-cov-target"
      echo "SCRUB  coverage tree:         $wt"
    fi
  done
fi

free_g=$(( $(df --output=avail -k "$ROOT" | tail -n 1) / 1024 / 1024 ))
echo "free: ${free_g}G (floor: ${MIN_FREE_G}G)"
[ "$free_g" -ge "$MIN_FREE_G" ]
