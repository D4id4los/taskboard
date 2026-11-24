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
#      a branch whose commits exist nowhere but this machine (no
#      origin/<branch> ref AND HEAD not contained in origin/main) is
#      kept as never pushed;
#   2. deletes target/llvm-cov-target from surviving worktrees (the
#      coverage tree is spent once lcov.info exists; separate from the
#      incremental cache, so nothing else is invalidated) — skipped when
#      an llvm-cov run appears active;
#   3. exits nonzero when free space on the checkout's filesystem is
#      below the floor, so callers stop instead of filling the disk;
#   4. applies the same fail-closed contract to MEMORY: exits nonzero
#      when /proc/meminfo MemAvailable is below
#      WORKTREE_DOCTOR_MIN_AVAIL_MIB (default 8192 — same constant and
#      evidence base as MEMORY_GATE_MIN_AVAIL_MIB in
#      scripts/memory-gate.sh; measured 2026-11-17: one clippy build
#      peaks at ~3.9 GiB, floor = peak + ~2 GiB desktop headroom + ~2 GiB
#      margin). A worktree created into a memory-starved box dies at its
#      first pre-commit hook anyway — and an under-provisioned box is
#      exactly the 2026-11-17 global-OOM shape
#      (.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md).
#      Unreadable MemAvailable counts as below the floor.
#
# The sweep enumerates via `git worktree list --porcelain` (#395): only
# registered worktrees enter the loop, at whatever path they live at.
# Unregistered `*-WT-*` directories are warned about, never touched —
# their KEEP protections are unevaluable, so removing them would be
# rm -rf by pattern match. Every failure (fetch, enumeration, per-entry
# git calls, df) is contained: the ONLY nonzero exit is the final floor
# test (free space below floor, or unmeasurable, which counts as below).
#
# The never-pushed spare errs toward KEEP by design: a deliberately
# abandoned scratch branch — never pushed, with or without unique
# commits — is pruned by hand (`git worktree remove --force`, then
# `git branch -D`), not by the doctor; the doctor is not its tool.
#
# Known limitation (#1185): a worktree that is never pushed, has zero
# commits, a clean tree, and sits strictly below the *fetched*
# origin/main tip is indistinguishable from a landed-PR leftover whose
# remote ref was pruned. Mitigations: the tip guard covers the
# same-tip window; a GRACE WINDOW (WORKTREE_DOCTOR_GRACE_HOURS, default
# 2) keeps a contained, zero-unique-commit worktree whose branch was
# born recently — the stale-local-main fresh-worktree shape; uncommitted
# or untracked files make removal refused (KEEP); branch off
# `origin/main` (not the local main ref) when creating a worktree. A
# scratch worktree older than the grace window in that state is pruned
# by hand (see the never-pushed stance above).
#
# Env overrides:
#   WORKTREE_DOCTOR_ROOT       anchor checkout (default: this repo root)
#   WORKTREE_DOCTOR_MIN_FREE_G free-space floor in GiB (default: 25)
#   WORKTREE_DOCTOR_MIN_AVAIL_MIB MemAvailable floor in MiB (default: 8192)
#   WORKTREE_DOCTOR_GRACE_HOURS grace window in hours for a contained,
#                               zero-unique-commit worktree whose branch
#                               was born recently (default: 2; 0 disables)
set -euo pipefail

MIN_FREE_G="${WORKTREE_DOCTOR_MIN_FREE_G:-25}"
MIN_AVAIL_MIB="${WORKTREE_DOCTOR_MIN_AVAIL_MIB:-8192}"
ROOT="$(cd "${WORKTREE_DOCTOR_ROOT:-$(dirname "$0")/..}" && pwd)"

if ! git -C "$ROOT" fetch origin --prune --quiet 2>/dev/null; then
  echo "WARN  fetch failed — merge decisions use stale origin/main"
fi

if porcelain="$(git -C "$ROOT" worktree list --porcelain 2>/dev/null)"; then
  mapfile -t REGISTERED < <(sed -n 's/^worktree //p' <<<"$porcelain" | LC_ALL=C sort)
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
    if git -C "$ROOT" show-ref --verify --quiet "refs/remotes/origin/$branch" 2>/dev/null; then
      echo "KEEP   (in flight):           $wt"
    else
      echo "KEEP   (never pushed — local-only branch): $wt"
    fi
    continue
  fi
  # Grace window (#1185): containment already proved zero unique
  # commits, which is exactly the fresh-worktree-off-a-stale-local-main
  # shape AND the landed-leftover-whose-remote-was-pruned shape. If the
  # branch was born recently, keep it — an in-flight fresh worktree
  # must not die because local main lagged the fetched tip. An
  # unreadable reflog birth counts as not-fresh (fall through; KEEP is
  # the doctor's posture but the contained+clean case is the removal
  # case, so only readable evidence can trigger the grace).
  grace_hours="${WORKTREE_DOCTOR_GRACE_HOURS:-2}"
  if [ "$grace_hours" -gt 0 ] \
     && birth="$(git -C "$ROOT" log -g --format=%at "refs/heads/$branch" 2>/dev/null | tail -n 1)" \
     && [ -n "$birth" ]; then
    now="$(date +%s)"
    if [ "$now" -ge "$birth" ] && [ $(( (now - birth) / 3600 )) -lt "$grace_hours" ]; then
      echo "KEEP   (fresh — grace window): $wt"
      continue
    fi
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
# (SC2012 waived for the ls below: the glob is the agent-controlled flat WT-suffix from AGENTS §8, not arbitrary user data)
# shellcheck disable=SC2012
while IFS= read -r orphan; do
  [ -n "$orphan" ] || continue
  echo "WARN  not a registered worktree (left alone): $orphan"
# (#1277) both comm inputs sort in the C locale: comm expects byte
# order, and under a locale collation (en_US.UTF-8 — case-insensitive
# primary weights, punctuation deweighted) the case-mixed and
# non-ASCII WT-suffix names this layout produces sort differently,
# spewing `comm: file N is not in sorted order` on every run. The pin
# makes comm's expectation match the collation regardless of the
# ambient locale.
done < <(comm -23 \
           <(ls -d "$ROOT"-WT-* 2>/dev/null | LC_ALL=C sort) \
           <(printf '%s\n' "${REGISTERED[@]}" | LC_ALL=C sort) )

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
[ "$free_g" -ge "$MIN_FREE_G" ] || exit 1

if avail_mib="$(awk '/^MemAvailable:/ { print int($2 / 1024); found=1; exit } END { exit found ? 0 : 1 }' /proc/meminfo 2>/dev/null)"; then
  echo "avail: ${avail_mib} MiB (floor: ${MIN_AVAIL_MIB} MiB)"
else
  echo "avail: ? MiB (floor: ${MIN_AVAIL_MIB} MiB) — unmeasurable counts as below"
  avail_mib=$(( MIN_AVAIL_MIB - 1 ))
fi
[ "$avail_mib" -ge "$MIN_AVAIL_MIB" ]
