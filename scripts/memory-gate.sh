#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
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
# Two-mode routing law (docs/agent_dispatch.org §2, issue #923 corollary
# of 2026-11-17 ~21:20):
#   - Short/heavy builds (clippy, cargo check, one-shot test builds —
#     anything real but bounded by its own completion) run gated-HOLD
#     (the default mode): lock held for the command.
#   - Long self-capped runs (cargo-mutants --jobs N under timeout,
#     miri batches — anything with BOTH its own job cap and a wall-clock
#     cap) run gated-PREFLIGHT only: `--preflight` floor-checks once and
#     never holds the lock. Holding the gate lock across a multi-hour
#     run is an availability collapse (every other agent's gated
#     pre-commit clippy queues behind it — observed live 2026-11-17
#     ~21:20), not a safety measure; the self-caps are the protection.
#   - Preflight is a pre-check by design: the floor can be eaten between
#     the reading and the command's build phase (TOCTOU). The residual
#     risk is covered by the resource law's stagger/cap (§2) — not
#     "solved" here.
#
# Route through this gate: any agent command expected to be a real build
# (>~1 min: cargo build, cargo test subsets, clippy re-runs) and the
# pre-commit hook's clippy step. The full §10 suite keeps
# scripts/run-quality-gates.sh (its own lock, unchanged).
#
# CI never needs this (one suite per runner); it exists for local runs.
#
# TMPDIR redirection (both modes, before exec): if TMPDIR is unset or
# canonicalizes to /tmp (or under it — host /tmp is tmpfs, RAM-backed),
# it is redirected to /var/tmp so scratch does not consume host memory;
# an explicitly chosen non-/tmp TMPDIR is honored untouched. Best-effort:
# an unwritable /var/tmp only warns — the gate must not turn a
# disk-permission quirk into a refusal.
#
# Usage:
#   scripts/memory-gate.sh [--] <cmd> [args…]          run <cmd> under the gate (hold mode)
#   scripts/memory-gate.sh --preflight [--] <cmd> [args…]
#                                                      floor-check once, no lock, exec <cmd>
#   scripts/memory-gate.sh --status                    report lock state + MemAvailable
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
# cross-reference the evidence when it changes. Since the two-mode law
# (2026-11-17) it is ALSO the pre-flight floor in
# scripts/run-quality-gates.sh — one budget, one dial.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/memory-gate.sh [--] <cmd> [args…]          # run <cmd> under the gate (hold mode)
       scripts/memory-gate.sh --preflight [--] <cmd> [args…]  # one floor check, no lock, exec
       scripts/memory-gate.sh --status                    # lock state + current MemAvailable
env:   MEMORY_GATE_TIMEOUT       max wait for lock/floor in seconds (default 3600, 0 = forever)
       MEMORY_GATE_POLL          max seconds between polls/messages (default 30)
       MEMORY_GATE_MIN_AVAIL_MIB MemAvailable floor in MiB (default 8192)
       (also the pre-flight floor in scripts/run-quality-gates.sh)
EOF
}

command -v flock >/dev/null || {
  echo "memory-gate: flock (util-linux) not found in PATH" >&2
  exit 1
}

# Holder liveness (#1110): detection-only classification of the lock
# record — alive / likely-hung / gone. Sourced, never executed; the
# helper carries its own --self-test. Law: no gate kills, signals, or
# steals from a live holder regardless of CPU profile
# (docs/agent_dispatch.org, the two-mode law amendment).
# shellcheck source=gate-lock-liveness.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/gate-lock-liveness.sh"

floor="${MEMORY_GATE_MIN_AVAIL_MIB:-8192}"
timeout="${MEMORY_GATE_TIMEOUT:-3600}"
poll="${MEMORY_GATE_POLL:-30}"

avail_mib() {
  awk '/^MemAvailable:/ { print int($2 / 1024); found=1; exit }
       END { exit found ? 0 : 1 }' /proc/meminfo 2>/dev/null
}

# TMPDIR redirection (issue #923 comment thread, binding): host /tmp is
# tmpfs (RAM-backed), so scratch written there eats MemAvailable directly.
# Both modes call this before exec. An explicitly chosen non-/tmp TMPDIR
# is honored untouched; unwritable /var/tmp only warns (best-effort).
redirect_tmpdir() {
  local resolved=""
  if [[ -n "${TMPDIR:-}" ]]; then
    resolved="$(realpath -m -- "${TMPDIR}" 2>/dev/null)" || resolved=""
  fi
  # unset, or under /tmp (including exactly /tmp) → redirect
  if [[ -z "${TMPDIR:-}" || "${resolved}" == "/tmp" || "${resolved}" == /tmp/* ]]; then
    if [[ -d /var/tmp && -w /var/tmp ]]; then
      export TMPDIR=/var/tmp
      echo "memory-gate: TMPDIR redirected to /var/tmp (host /tmp is tmpfs; scratch must not consume MemAvailable)"
    else
      echo "memory-gate: WARNING: TMPDIR unset or under tmpfs /tmp but /var/tmp is not writable — continuing with TMPDIR=${TMPDIR:-unset}" >&2
    fi
  fi
}

# Preflight mode (two-mode law): one MemAvailable reading, fail closed,
# no waiting, no lock — fd 9 is never opened, so nothing can be held
# across the command's duration by construction. A long self-capped run
# wants a go/no-go now, not a queue; the bounded-wait loop deliberately
# does not apply here. The caller re-runs when the box frees up.
if [[ "${1:-}" == "--preflight" ]]; then
  shift
  if [[ "${1:-}" == "--" ]]; then
    shift
  fi
  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]] || [[ "$#" -eq 0 ]]; then
    usage
    [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]
    exit $?
  fi
  if ! avail="$(avail_mib)" || (( avail < floor )); then
    echo "memory-gate: preflight refusing — MemAvailable ${avail:-unreadable} MiB below floor ${floor} MiB (no lock was acquired)" >&2
    exit 1
  fi
  echo "memory-gate: preflight MemAvailable ${avail} MiB (floor ${floor} MiB) — running: $*"
  redirect_tmpdir
  exec "$@"
fi

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
    # #1110: one short liveness probe — prev CPU sample now, the
    # helper's internal cur sample ~1s later. The "lock is FREE/HELD"
    # lines above are the machine-readable contract and are unchanged;
    # this is an addition, report-only.
    status_pid=$(gate_holder_pid "$lock_file" 2>/dev/null) || status_pid=""
    status_cpu_prev=""
    if [[ -n $status_pid ]]; then
      status_cpu_prev=$(gate_holder_tree_cpu "$status_pid" 2>/dev/null) || status_cpu_prev=""
    fi
    sleep 1
    case $(gate_holder_liveness "$lock_file" "$status_cpu_prev" 2>/dev/null || echo unknown) in
      likely-hung)
        echo "  holder liveness: likely-hung (no CPU time in the ~1s probe — triage that lane)" ;;
      gone)
        echo "  holder liveness: gone (the pid no longer exists; flock releases on death)" ;;
    esac
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
holder_field() { # holder_field <key> — one field from the holder record
  grep -oE "(^|[[:space:]])${1}=[^[:space:]]*" "$lock_file" 2>/dev/null \
    | head -n 1 | sed "s/.*${1}=//"
}

# Liveness tick (#1110): sample the holder record, keep the previous
# CPU sample across calls, count consecutive zero-delta polls. Sets
# globals: holder_class, holder_pid_now, hung_polls. Detection-only —
# the classification changes the WAIT MESSAGE, never behavior.
holder_pid_now=""
holder_class="unknown"
hung_polls=0
liveness_tick() {
  holder_pid_now=$(gate_holder_pid "$lock_file" 2>/dev/null) || holder_pid_now=""
  local cur_cpu=""
  if [[ -n $holder_pid_now ]]; then
    cur_cpu=$(gate_holder_tree_cpu "$holder_pid_now" 2>/dev/null) || cur_cpu=""
  fi
  holder_class=$(gate_holder_liveness "$lock_file" "${prev_cpu:-}" 2>/dev/null || true)
  if [[ $holder_class == likely-hung ]]; then
    hung_polls=$((hung_polls + 1))
  else
    hung_polls=0
  fi
  prev_cpu=$cur_cpu
}
hung_note() { # the #1110 hung-holder wait line (branch/started from the record)
  echo "memory-gate: held by a likely-hung pid ${holder_pid_now:-?} (branch $(holder_field branch), started $(holder_field started); no CPU time in ~$((hung_polls * poll))s — triage that lane)" >&2
}

start_ts="$(date +%s)"
deadline() {
  now="$(date +%s)"
  (( timeout > 0 )) && (( now - start_ts >= timeout ))
}

# Phase 1: acquire the build lock (bounded).
exec 9>>"$lock_file" # creates if missing; this fd carries the lock
prev_cpu=""
while ! flock -w "$poll" 9; do
  if deadline; then
    echo "memory-gate: gave up after ${timeout}s — ${lock_file} still held by: $(holder_info)" >&2
    exit 1
  fi
  liveness_tick
  # (#1110) 3 consecutive zero-CPU-delta polls (~90s at the default
  # 30s poll) surface the holder as likely-hung; report-only — wait
  # duration, timeout and exit codes unchanged.
  if (( hung_polls >= 3 )); then
    hung_note
  else
    echo "memory-gate: waiting for the memory-gate lock, held by: $(holder_info)" >&2
  fi
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
  liveness_tick
  # (#1110) the phase-2 holder is this gate process itself ($$), and a
  # sleeping waiter trivially shows a zero CPU delta — the hung line
  # is surfaced only when the record names a foreign pid (defensive
  # against a racing overwrite), never against self.
  if [[ -n $holder_pid_now && $holder_pid_now != "$$" ]] && (( hung_polls >= 3 )); then
    hung_note
  fi
  echo "memory-gate: MemAvailable ${avail:-?} MiB below floor ${floor} MiB — waiting" >&2
  sleep "$poll"
done

echo "memory-gate: running under the gate (MemAvailable ${avail} MiB, floor ${floor} MiB): $*"
redirect_tmpdir
exec "$@" # exec keeps fd 9 open, so the lock lives exactly as long as the command
