#!/usr/bin/env bash
# Disk-hygiene sweep for agent worktrees (2026-10-18 disk-usage report:
# .artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md).
#
# Usage: scripts/worktree-doctor.sh   (from the main checkout, or set
# WORKTREE_DOCTOR_ROOT). Run it before creating a new worktree and any
# time disk headroom is in doubt.
#
# What it does:
#   1. removes sibling worktrees whose branch is merged into origin/main
#      (or whose PR is MERGED/CLOSED) and that have no uncommitted
#      changes — in-flight worktrees are never touched;
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
  if [ -n "$(git -C "$wt" status --porcelain)" ]; then
    echo "KEEP   (uncommitted changes): $wt"
    continue
  fi
  if git -C "$ROOT" merge-base --is-ancestor "$branch" origin/main 2>/dev/null \
     || [ -n "$(gh pr list --state all --head "$branch" --json state \
                --jq 'map(select(.state == "MERGED" or .state == "CLOSED")) | length | select(. > 0)')" ]; then
    git -C "$ROOT" worktree remove --force "$wt" \
      || { git -C "$ROOT" worktree prune; rm -rf "$wt"; }
    echo "REMOVE (branch/PR merged):    $wt"
  else
    echo "KEEP   (in flight):           $wt"
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
