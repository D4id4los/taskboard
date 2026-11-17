#!/usr/bin/env bash
# Memory gate: serialize build-vs-build peaks across worktrees and refuse
# to start a memory-heavy command on a starved box (2026-11-17 OOM incident:
# .artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md —
# 7 concurrent cold per-worktree clippy builds + 1 llvm-cov build drove a
# 30G box into global OOM).
#
# Two protections, one lock:
#   1. an exclusive flock on memory-gate.lock (in the MAIN checkout's git
#      dir, same mechanism as quality-gates.lock) so concurrent agents'
#      builds take turns instead of aligning their memory peaks;
#   2. a MemAvailable floor poll: while HOLDING the lock, the command only
#      starts when MemAvailable is at or above the floor — the lock
#      serializes the peaks, the floor checks the box's remaining budget.
#
# The lock is DISTINCT from quality-gates.lock on purpose: sharing would
# serialize every minutes-long pre-commit clippy behind the 10–60-minute
# full suite. Suite-vs-build windows are instead kept apart by the
# dispatcher's suite-free-window rule (docs/agent_dispatch.org §2, the
# 2026-11-17 concurrency law). Residual overlap is bounded at 2 heavy
# jobs — the box survives 2 (it died at 8).
#
# Route through this gate: any agent command expected to be a real build
# (>~1 min: cargo build, cargo test subsets, clippy re-runs) and the
# pre-commit hook's clippy step. The full §10 suite keeps
# scripts/run-quality-gates.sh (its own lock, unchanged).
#
# CI never needs this (one suite per runner); it exists for local runs.
#
# Usage:
#   scripts/memory-gate.sh [--] <cmd> [args…]  run <cmd> under the gate
#   scripts/memory-gate.sh --status            report lock state + MemAvailable
#
# Env:
#   MEMORY_GATE_TIMEOUT         max seconds waiting for lock OR floor before
#                               giving up with a nonzero exit (default: 3600;
#                               0 = forever)
#   MEMORY_GATE_POLL            max seconds between floor polls / waiting
#                               messages (default: 30)
#   MEMORY_GATE_MIN_AVAIL_MIB   MemAvailable floor in MiB (default: 8192)
#
# Floor provenance (measured 2026-11-17, this repo, full-workspace clippy
# re-check with a 2s MemAvailable sampler): MemAvailable fell from
# ~21,030 MiB to a minimum of 17,657 MiB — a peak draw of ~3.9 GiB for
# one clippy build. Floor = observed peak (~4 GiB) + ~2 GiB desktop
# headroom ⇒ ~6 GiB needed; 8192 MiB carries ~2 GiB of extra margin.
# The same constant is duplicated as WORKTREE_DOCTOR_MIN_AVAIL_MIB in
# scripts/worktree-doctor.sh (shell, no shared config) — tune BOTH or
# cross-reference the evidence when it changes.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/memory-gate.sh [--] <cmd> [args…]  # run <cmd> under the memory gate
       scripts/memory-gate.sh --status            # lock state + current MemAvailable
env:   MEMORY_GATE_TIMEOUT       max wait for lock/floor in seconds (default 3600, 0 = forever)
       MEMORY_GATE_POLL          max seconds between polls/messages (default 30)
       MEMORY_GATE_MIN_AVAIL_MIB MemAvailable floor in MiB (default 8192)
EOF
}

command -v flock >/dev/null || {
  echo "memory-gate: flock (util-linux) not found in PATH" >&2
  exit 1
}

floor="${MEMORY_GATE_MIN_AVAIL_MIB:-8192}"
timeout="${MEMORY_GATE_TIMEOUT:-3600}"
poll="${MEMORY_GATE_POLL:-30}"

avail_mib() {
  awk '/^MemAvailable:/ { print int($2 / 1024); found=1; exit }
       END { exit found ? 0 : 1 }' /proc/meminfo 2>/dev/null
}

if [[ "${1:-}" == "--status" ]]; then
  common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
    echo "memory-gate: not inside a git repository" >&2
    exit 1
  }
  lock_file="${common_dir%/}/memory-gate.lock"
  if flock -n "$lock_file" true 2>/dev/null; then
    echo "memory-gate: lock is FREE: $lock_file"
  else
    echo "memory-gate: lock is HELD: $lock_file"
    echo "  holder info (best effort — the lock itself lives in the kernel):"
    sed 's/^/    /' "$lock_file" 2>/dev/null || true
  fi
  if avail="$(avail_mib)"; then
    echo "memory-gate: MemAvailable ${avail} MiB (floor: ${floor} MiB)"
  else
    echo "memory-gate: MemAvailable unreadable (floor: ${floor} MiB)"
  fi
  exit 0
fi

if [[ "${1:-}" == "--" ]]; then
  shift
fi
if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] || [[ "$#" -eq 0 ]]; then
  usage
  [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]
  exit $?
fi

common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
  echo "memory-gate: not inside a git repository" >&2
  exit 1
}
lock_file="${common_dir%/}/memory-gate.lock"

holder_info() { tr '\n' ';' <"$lock_file" 2>/dev/null || true; }

start_ts="$(date +%s)"
deadline() {
  now="$(date +%s)"
  (( timeout > 0 )) && (( now - start_ts >= timeout ))
}

# Phase 1: acquire the build lock (bounded).
exec 9>>"$lock_file" # creates if missing; this fd carries the lock
while ! flock -w "$poll" 9; do
  if deadline; then
    echo "memory-gate: gave up after ${timeout}s — ${lock_file} still held by: $(holder_info)" >&2
    exit 1
  fi
  echo "memory-gate: waiting for the memory-gate lock, held by: $(holder_info)" >&2
done
waited="$(($(date +%s) - start_ts))"
branch="$(git branch --show-current 2>/dev/null || echo unknown)"
printf 'pid=%s branch=%s worktree=%s started=%s\n' \
  "$$" "${branch}" "$(git rev-parse --show-toplevel)" "$(date -Is)" >"$lock_file"
echo "memory-gate: lock acquired (waited ${waited}s)"

# Phase 2: wait for the memory budget while HOLDING the lock (bounded).
# Unreadable /proc counts as below the floor — fail closed, same contract
# as the disk floor in worktree-doctor.sh.
while ! avail="$(avail_mib)" || (( avail < floor )); do
  if deadline; then
    echo "memory-gate: gave up after ${timeout}s — MemAvailable ${avail:-?} MiB below floor ${floor} MiB" >&2
    exit 1
  fi
  echo "memory-gate: MemAvailable ${avail:-?} MiB below floor ${floor} MiB — waiting" >&2
  sleep "$poll"
done

echo "memory-gate: running under the gate (MemAvailable ${avail} MiB, floor ${floor} MiB): $*"
exec "$@" # exec keeps fd 9 open, so the lock lives exactly as long as the command
