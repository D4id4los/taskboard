#!/usr/bin/env bash
# Disk-hygiene sweep for agent worktrees (2026-10-18 disk-usage report:
# .artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md).
#
# Usage: scripts/worktree-doctor.sh   (from the main checkout, or set
# WORKTREE_DOCTOR_ROOT). Run it before creating a new worktree and any
# time disk headroom is in doubt.
#
# What it does:
#   1. removes registered worktrees whose checked-out HEAD is fully
#      merged into origin/main and that have no uncommitted changes — a
#      worktree whose branch carries any commit absent from origin/main
#      (or that sits on a primary branch, or is locked) is never touched;
#   2. deletes target/llvm-cov-target from surviving worktrees (the
#      coverage tree is spent once lcov.info exists; separate from the
#      incremental cache, so nothing else is invalidated) — skipped when
#      an llvm-cov run appears active;
#   3. exits nonzero when free space on the checkout's filesystem is
#      below the floor, so callers stop instead of filling the disk.
#
# The sweep enumerates via `git worktree list --porcelain` (#395): only
# registered worktrees enter the loop, at whatever path they live at.
# Unregistered `*-WT-*` directories are warned about, never touched —
# their KEEP protections are unevaluable, so removing them would be
# rm -rf by pattern match. Every failure (fetch, enumeration, per-entry
# git calls, df) is contained: the ONLY nonzero exit is the final floor
# test (free space below floor, or unmeasurable, which counts as below).
#
# Env overrides:
#   WORKTREE_DOCTOR_ROOT       anchor checkout (default: this repo root)
#   WORKTREE_DOCTOR_MIN_FREE_G free-space floor in GiB (default: 25)
set -euo pipefail

MIN_FREE_G="${WORKTREE_DOCTOR_MIN_FREE_G:-25}"
ROOT="$(cd "${WORKTREE_DOCTOR_ROOT:-$(dirname "$0")/..}" && pwd)"

if ! git -C "$ROOT" fetch origin --prune --quiet 2>/dev/null; then
  echo "WARN  fetch failed — merge decisions use stale origin/main"
fi

if porcelain="$(git -C "$ROOT" worktree list --porcelain 2>/dev/null)"; then
  mapfile -t REGISTERED < <(sed -n 's/^worktree //p' <<<"$porcelain" | sort)
  mapfile -t WORKTREES < <(sed -n 's/^worktree //p' <<<"$porcelain" | grep -vxF "$ROOT")
else
  echo "WARN  cannot enumerate worktrees"
  REGISTERED=()
  WORKTREES=()
fi

# Safe predicate: the worktree's HEAD itself (not merely the branch
# name's PR history) must be fully merged into origin/main, and must
# not BE the origin/main tip — a worktree at the tip is either fresh
# (just created, nothing committed yet) or a resume on a landed
# branch, both of which look identical to "done" to any sweep and are
# therefore kept (2026-10-23 active-worktree deletions, #415).
# Resolved once; if it fails the sweep is report-only (all KEEP).
origin_main=""
if ! origin_main="$(git -C "$ROOT" rev-parse origin/main 2>/dev/null)"; then
  echo "WARN  origin/main unresolvable — sweep is report-only"
  origin_main=""
fi

for wt in "${WORKTREES[@]}"; do
  if [ ! -d "$wt" ]; then
    echo "STALE (registered, dir gone): $wt"
    continue
  fi
  if ! branch="$(git -C "$wt" branch --show-current 2>/dev/null)"; then
    echo "SKIP  (unreadable registered worktree): $wt"
    continue
  fi
  if [ -z "$branch" ] || [ "$branch" = main ]; then
    echo "KEEP   (no branch or primary branch): $wt"
    continue
  fi
  if ! status="$(git -C "$wt" status --porcelain 2>/dev/null)"; then
    echo "SKIP  (unreadable registered worktree): $wt"
    continue
  fi
  if [ -n "$status" ]; then
    echo "KEEP   (uncommitted changes): $wt"
    continue
  fi
  if ! head_commit="$(git -C "$wt" rev-parse HEAD 2>/dev/null)"; then
    echo "SKIP  (unreadable registered worktree): $wt"
    continue
  fi
  if [ -z "$origin_main" ]; then
    echo "KEEP   (origin/main unresolvable — report-only): $wt"
    continue
  fi
  if [ "$origin_main" = "$head_commit" ]; then
    echo "KEEP   (HEAD at origin/main tip — fresh or resume): $wt"
    continue
  fi
  if ! git -C "$ROOT" merge-base --is-ancestor "$head_commit" "$origin_main" 2>/dev/null; then
    echo "KEEP   (in flight):           $wt"
    continue
  fi
  # Re-check liveness immediately before removal: status was sampled
  # above, so anything written since would be destroyed by --force.
  if ! status="$(git -C "$wt" status --porcelain 2>/dev/null)"; then
    echo "SKIP  (unreadable registered worktree): $wt"
    continue
  fi
  if [ -n "$status" ]; then
    echo "KEEP   (changed during sweep): $wt"
    continue
  fi
  if git -C "$ROOT" worktree remove "$wt" 2>/dev/null; then
    echo "REMOVE (HEAD merged):         $wt"
  else
    echo "KEEP   (remove refused — likely locked or in use): $wt"
  fi
done

# Unregistered `*-WT-*` siblings (e.g. a plain dir left by a naming
# deviation) never enter the sweep — warn so a human can decide.
while IFS= read -r orphan; do
  [ -n "$orphan" ] || continue
  echo "WARN  not a registered worktree (left alone): $orphan"
done < <(comm -23 \
           <(ls -d "$ROOT"-WT-* 2>/dev/null | sort) \
           <(printf '%s\n' "${REGISTERED[@]}" | sort) )

if pgrep -f llvm-cov >/dev/null 2>&1; then
  echo "SKIP   coverage scrub (an llvm-cov run looks active)"
else
  for wt in "${WORKTREES[@]}"; do
    if [ -d "$wt/target/llvm-cov-target" ]; then
      rm -rf "$wt/target/llvm-cov-target"
      echo "SCRUB  coverage tree:         $wt"
    fi
  done
fi

if free_k="$(df --output=avail -k "$ROOT" 2>/dev/null | tail -n 1)"; then
  free_g=$(( free_k / 1024 / 1024 ))
  echo "free: ${free_g}G (floor: ${MIN_FREE_G}G)"
else
  echo "free: ? (floor: ${MIN_FREE_G}G) — unmeasurable counts as below"
  free_g=$(( MIN_FREE_G - 1 ))
fi
[ "$free_g" -ge "$MIN_FREE_G" ]
