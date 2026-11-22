#!/usr/bin/env bash
# Gate-lock holder liveness (#1110) — surface-first, never act.
#
# Both gate locks (memory-gate.lock, quality-gates.lock) release on
# holder DEATH (flock is kernel-owned) but say nothing about a holder
# that hangs while ALIVE (the 2026-11-20 incident: a pid at ~0.1% CPU
# for 55+ minutes, every queued pre-commit blind). This helper is
# sourced by both gates and classifies a lock holder from the record
# the gates already write (`pid=… branch=… worktree=… started=…`):
#
#   alive       the holder's cumulative CPU time (utime+stime,
#               /proc/<pid>/stat fields 14+15) advanced since the
#               caller's previous sample
#   likely-hung zero CPU-time delta across the caller's poll window
#   gone        the pid no longer exists (the kernel releases the
#               lock; purely informational)
#
# The LAW (docs/agent_dispatch.org, the two-mode law amendment): gate
# lock liveness reporting is DETECTION-ONLY. No gate may kill, signal,
# or steal a lock from a live holder regardless of its CPU profile —
# a legitimate long holder is indistinguishable from a wedged one by
# CPU time alone; triage is the dispatcher's job and the safe release
# path remains flock-on-kill. This helper therefore only classifies;
# it never sends signals, never removes lock files, never touches the
# holder process.
#
# Linux-only by design (/proc/<pid>/stat): the dev/target platform is
# x86_64 Linux. Unreadable proc fails closed to an unclassified
# result — the caller keeps its existing wait message, never masks
# the free/held contract.
#
# Usage (sourced):
#   gate_holder_pid <lock_file>          -> holder pid (empty rc 1)
#   gate_holder_cpu <pid>                -> utime+stime jiffies (rc 1 if gone)
#   gate_holder_liveness <lock_file> [<prev_cpu>]
#                                        -> alive|likely-hung|gone|unknown
#
# Usage (direct): scripts/gate-lock-liveness.sh --self-test
set -uo pipefail

gate_holder_pid() { # gate_holder_pid <lock_file> -> pid | rc 1
  local lock_file=$1
  [[ -f $lock_file ]] || return 1
  local pid
  pid=$(sed -n 's/^pid=\([0-9][0-9]*\).*/\1/p' "$lock_file" 2>/dev/null | head -n 1)
  [[ -n $pid ]] || return 1
  printf '%s\n' "$pid"
}

gate_holder_cpu() { # gate_holder_cpu <pid> -> jiffies | rc 1
  local pid=$1 stat rest
  local -a f=()
  [[ -n $pid ]] || return 1
  [[ -r /proc/$pid/stat ]] || return 1
  IFS= read -r stat <"/proc/$pid/stat" 2>/dev/null || return 1
  # comm (field 2) may contain spaces and parentheses — parse after
  # its closing ')': what follows starts at field 3 (state), so
  # utime (field 14) is index 11 and stime (field 15) index 12.
  rest=${stat##*) }
  read -ra f <<<"$rest"
  if (( ${#f[@]} < 13 )); then return 1; fi
  printf '%s\n' "$(( f[11] + f[12] ))"
}

gate_holder_liveness() { # gate_holder_liveness <lock_file> [<prev_cpu>]
  local lock_file=$1 prev=${2:-} pid cur
  pid=$(gate_holder_pid "$lock_file") || {
    echo unknown
    return 0
  }
  if ! cur=$(gate_holder_cpu "$pid"); then
    echo gone
    return 0
  fi
  if [[ -z $prev ]]; then
    echo unknown
    return 0
  fi
  if (( cur == prev )); then
    echo likely-hung
  else
    echo alive
  fi
  return 0
}

# ---------------------------------------------------------------------------
# Self-test: the idle/spinning/vanished fake-holder triplet, run against
# a temp lock file. Asserts classifications only (typed outcomes — the
# classification vocabulary IS the typed outcome here), self-cleans.
# ---------------------------------------------------------------------------
self_test() {
  local pid_idle pid_spin prev cur rc=0
  ST_DIR=$(mktemp -d) # global: the EXIT trap outlives this function's locals
  tmp=$ST_DIR
  trap 'rm -rf "$ST_DIR"' EXIT

  # 1. Idle holder: a sleep-backed pid never accumulates CPU — two
  #    samples must classify likely-hung.
  sleep 300 & pid_idle=$!
  printf 'pid=%d branch=self-test worktree=/tmp started=%s\n' \
    "$pid_idle" "$(date -Is)" >"$tmp/idle.lock"
  prev=$(gate_holder_cpu "$pid_idle")
  cur=$(gate_holder_cpu "$pid_idle")
  if [[ $(gate_holder_liveness "$tmp/idle.lock" "$prev") == likely-hung ]]; then
    echo "self-test 1 (idle sleep holder classifies likely-hung on zero delta): PASS"
  else
    echo "self-test 1 (idle sleep holder classifies likely-hung on zero delta): FAIL"
    rc=1
  fi
  kill "$pid_idle" 2>/dev/null || true
  wait "$pid_idle" 2>/dev/null || true

  # 2. Spinning holder: a while-loop pid accumulates CPU between two
  #    real samples — must classify alive.
  (while :; do :; done) & pid_spin=$!
  printf 'pid=%d branch=self-test worktree=/tmp started=%s\n' \
    "$pid_spin" "$(date -Is)" >"$tmp/spin.lock"
  prev=$(gate_holder_cpu "$pid_spin")
  sleep 0.2
  cur=$(gate_holder_cpu "$pid_spin")
  if [[ -n $prev && -n $cur ]] && (( cur > prev )) \
     && [[ $(gate_holder_liveness "$tmp/spin.lock" "$prev") == alive ]]; then
    echo "self-test 2 (spinning holder accumulates CPU and classifies alive): PASS"
  else
    echo "self-test 2 (spinning holder accumulates CPU and classifies alive): FAIL (prev=$prev cur=$cur)"
    rc=1
  fi
  kill "$pid_spin" 2>/dev/null || true
  wait "$pid_spin" 2>/dev/null || true

  # 3. Vanished holder: a pid that no longer exists classifies gone.
  printf 'pid=999999999 branch=self-test worktree=/tmp started=%s\n' \
    "$(date -Is)" >"$tmp/vanished.lock"
  if [[ $(gate_holder_liveness "$tmp/vanished.lock" 42) == gone ]]; then
    echo "self-test 3 (vanished pid classifies gone): PASS"
  else
    echo "self-test 3 (vanished pid classifies gone): FAIL"
    rc=1
  fi

  # 4. Unclassifiable: no pid record (foreign lock content) fails
  #    closed to unknown — the caller keeps its plain wait message.
  printf 'someone else wrote this file\n' >"$tmp/foreign.lock"
  if [[ $(gate_holder_liveness "$tmp/foreign.lock" 42) == unknown ]]; then
    echo "self-test 4 (foreign lock record fails closed to unknown): PASS"
  else
    echo "self-test 4 (foreign lock record fails closed to unknown): FAIL"
    rc=1
  fi

  return $rc
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then
  if [[ "${1:-}" == "--self-test" ]]; then
    self_test
    exit $?
  fi
  echo "usage: source scripts/gate-lock-liveness.sh  |  scripts/gate-lock-liveness.sh --self-test" >&2
  exit 64
fi
