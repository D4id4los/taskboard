#!/usr/bin/env bash
# shellcheck source-path=SCRIPTDIR
# Cross-worktree serialization for full quality-gate runs (AGENTS.md §10).
#
# Parallel agents in sibling worktrees OOM the box when several of them
# run the full test/coverage suite at the same time — no per-agent -j cap
# helps, because the suites themselves overlap. This script makes them
# take turns: every invocation takes an exclusive flock on a lock file
# inside the MAIN checkout's git dir (`git rev-parse --git-common-dir`
# resolves there from any worktree), so all worktrees of this clone
# contend on one file and exactly one suite runs at a time while the
# others block, printing who currently holds the lock.
#
# flock is chosen over mkdir/pidfile locks because the kernel releases
# it when the holder's process dies — including an OOM kill — so a
# crashed agent can never leave a stale lock behind.
#
# CI never needs this (one suite per runner); it exists for local runs.
#
# Usage (run from the worktree root, like the §10 commands themselves):
#   scripts/run-quality-gates.sh                full §10 gate sequence
#   scripts/run-quality-gates.sh <cmd> [args…]  run <cmd> under the same lock
#                                               (e.g. a lone cargo nextest run)
#   scripts/run-quality-gates.sh --status       report the current holder
#
# Two-mode cross-reference (2026-11-17): this script serializes but does
# not floor-check-with-wait — the floor check here is a one-shot
# pre-flight refusal BEFORE the lock is acquired, so a starved box is a
# refusal, not a queued 10–60-minute llvm-cov start. Build-vs-build
# routing (short/heavy ⇒ hold, long self-capped ⇒ preflight-only) is the
# memory gate's law: scripts/memory-gate.sh (docs/agent_dispatch.org §2).
# Evidence: the 2026-11-17 OOM incident
# (.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md).
#
# Env:
#   GATES_LOCK_TIMEOUT  max seconds to wait for the lock before giving up
#                       with a nonzero exit (default: 14400; 0 = forever)
#   GATES_LOCK_POLL     max seconds between "still waiting" messages
#                       (default: 30; also bounds reacquisition latency
#                       after the holder releases)
#   MEMORY_GATE_MIN_AVAIL_MIB
#                       MemAvailable pre-flight floor in MiB (default:
#                       8192) — the SAME constant scripts/memory-gate.sh
#                       uses (one budget, one dial); deliberately not a
#                       new env var. Duplicated, not sourced (shell, no
#                       shared config; the doctor's floor keeps its own
#                       WORKTREE_DOCTOR_MIN_AVAIL_MIB name).
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/run-quality-gates.sh                # full AGENTS.md §10 gate sequence
       scripts/run-quality-gates.sh <cmd> [args…]  # run <cmd> under the same lock
       scripts/run-quality-gates.sh --status       # who holds the lock right now
env:   GATES_LOCK_TIMEOUT  max wait for the lock in seconds (default 14400, 0 = forever)
       GATES_LOCK_POLL     max seconds between waiting messages (default 30)
EOF
}

command -v flock >/dev/null || {
  echo "run-quality-gates: flock (util-linux) not found in PATH" >&2
  exit 1
}

# Holder liveness (#1110): detection-only classification of the lock
# record — alive / likely-hung / gone. Sourced, never executed; the
# helper carries its own --self-test. Law: no gate kills, signals, or
# steals from a live holder regardless of CPU profile
# (docs/agent_dispatch.org, the two-mode law amendment).
# shellcheck source=gate-lock-liveness.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/gate-lock-liveness.sh"

common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
  echo "run-quality-gates: not inside a git repository" >&2
  exit 1
}
lock_file="${common_dir%/}/quality-gates.lock"

# Pre-flight memory floor — SAME env var + default as scripts/memory-gate.sh
# (cross-referencing comment in its header; one budget, one dial).
floor="${MEMORY_GATE_MIN_AVAIL_MIB:-8192}"

avail_mib() {
  awk '/^MemAvailable:/ { print int($2 / 1024); found=1; exit }
       END { exit found ? 0 : 1 }' /proc/meminfo 2>/dev/null
}

if [[ "${1:-}" == "--status" ]]; then
  if flock -n "$lock_file" true 2>/dev/null; then
    echo "run-quality-gates: lock is FREE: $lock_file"
  else
    echo "run-quality-gates: lock is HELD: $lock_file"
    echo "  holder info (best effort — the lock itself lives in the kernel):"
    sed 's/^/    /' "$lock_file" 2>/dev/null || true
    # #1110: one short liveness probe — prev CPU sample now, the
    # helper's internal cur sample ~1s later. The "lock is FREE/HELD"
    # lines above are the machine-readable contract and are unchanged;
    # this is an addition, report-only.
    status_pid=$(gate_holder_pid "$lock_file" 2>/dev/null) || status_pid=""
    status_cpu_prev=""
    if [[ -n $status_pid ]]; then
      status_cpu_prev=$(gate_holder_cpu "$status_pid" 2>/dev/null) || status_cpu_prev=""
    fi
    sleep 1
    case $(gate_holder_liveness "$lock_file" "$status_cpu_prev" 2>/dev/null || echo unknown) in
      likely-hung)
        echo "  holder liveness: likely-hung (no CPU time in the ~1s probe — triage that lane)" ;;
      gone)
        echo "  holder liveness: gone (the pid no longer exists; flock releases on death)" ;;
    esac
  fi
  # Parity with scripts/memory-gate.sh --status: a status query never
  # refuses, whatever the reading.
  if avail="$(avail_mib)"; then
    echo "run-quality-gates: MemAvailable ${avail} MiB (floor: ${floor} MiB)"
  else
    echo "run-quality-gates: MemAvailable unreadable (floor: ${floor} MiB)"
  fi
  exit 0
fi

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

timeout="${GATES_LOCK_TIMEOUT:-14400}"
poll="${GATES_LOCK_POLL:-30}"

# One-shot pre-flight refusal (two-mode law, D4 of the C13 plan): check
# MemAvailable ONCE before the lock is touched — below floor or unreadable
# /proc fails closed, nonzero, no waiting loop. The bounded lock wait
# (GATES_LOCK_TIMEOUT) below stays as is; a starved box is a refusal,
# not a queued start. Same fail-closed contract as the memory gate and
# the doctor's disk floor.
preflight_mem_floor() {
  if ! avail="$(avail_mib)" || (( avail < floor )); then
    echo "run-quality-gates: pre-flight refusing — MemAvailable ${avail:-unreadable} MiB below floor ${floor} MiB (no lock was acquired)" >&2
    exit 1
  fi
  echo "run-quality-gates: pre-flight MemAvailable ${avail} MiB (floor ${floor} MiB)"
}

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
    cur_cpu=$(gate_holder_cpu "$holder_pid_now" 2>/dev/null) || cur_cpu=""
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
  echo "run-quality-gates: held by a likely-hung pid ${holder_pid_now:-?} (branch $(holder_field branch), started $(holder_field started); no CPU time in ~$((hung_polls * poll))s — triage that lane)" >&2
}

preflight_mem_floor # before the lock is touched — both the full-suite and the command path

exec 9>>"$lock_file" # creates if missing; this fd carries the lock
start_ts="$(date +%s)"
prev_cpu=""
while ! flock -w "$poll" 9; do
  now="$(date +%s)"
  if (( timeout > 0 )) && (( now - start_ts >= timeout )); then
    echo "run-quality-gates: gave up after ${timeout}s — ${lock_file} still held by: $(holder_info)" >&2
    exit 1
  fi
  liveness_tick
  # (#1110) 3 consecutive zero-CPU-delta polls (~90s at the default
  # 30s poll) surface the holder as likely-hung; report-only — wait
  # duration, timeout and exit codes unchanged.
  if (( hung_polls >= 3 )); then
    hung_note
  else
    echo "run-quality-gates: waiting for the gates lock, held by: $(holder_info)" >&2
  fi
done

waited="$(($(date +%s) - start_ts))"
branch="$(git branch --show-current 2>/dev/null || echo unknown)"
printf 'pid=%s branch=%s worktree=%s started=%s\n' \
  "$$" "${branch}" "$(git rev-parse --show-toplevel)" "$(date -Is)" >"$lock_file"
echo "run-quality-gates: lock acquired (waited ${waited}s)"

if [[ "$#" -eq 0 ]]; then
  echo "run-quality-gates: running the full quality-gate suite (AGENTS.md §10)"
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo nextest run --workspace
  cargo test --doc --workspace
  cargo deny check
  cargo llvm-cov nextest --workspace --all-features --no-tests=warn --no-report
  scripts/coverage-quarantine-profraw.sh target/llvm-cov-target
  cargo llvm-cov report --lcov --output-path lcov.info
  rm -rf target/llvm-cov-target
  echo "run-quality-gates: full suite passed"
else
  if [[ "${1:-}" == "--" ]]; then
    shift
  fi
  echo "run-quality-gates: running under the gates lock: $*"
  exec "$@" # exec keeps fd 9 open, so the lock lives exactly as long as the command
fi
