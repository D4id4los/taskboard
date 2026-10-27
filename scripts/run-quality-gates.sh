#!/usr/bin/env bash
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
# Env:
#   GATES_LOCK_TIMEOUT  max seconds to wait for the lock before giving up
#                       with a nonzero exit (default: 14400; 0 = forever)
#   GATES_LOCK_POLL     max seconds between "still waiting" messages
#                       (default: 30; also bounds reacquisition latency
#                       after the holder releases)
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

common_dir="$(git rev-parse --path-format=absolute --git-common-dir 2>/dev/null)" || {
  echo "run-quality-gates: not inside a git repository" >&2
  exit 1
}
lock_file="${common_dir%/}/quality-gates.lock"

if [[ "${1:-}" == "--status" ]]; then
  if flock -n "$lock_file" true 2>/dev/null; then
    echo "run-quality-gates: lock is FREE: $lock_file"
  else
    echo "run-quality-gates: lock is HELD: $lock_file"
    echo "  holder info (best effort — the lock itself lives in the kernel):"
    sed 's/^/    /' "$lock_file" 2>/dev/null || true
  fi
  exit 0
fi

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

timeout="${GATES_LOCK_TIMEOUT:-14400}"
poll="${GATES_LOCK_POLL:-30}"

holder_info() { tr '\n' ';' <"$lock_file" 2>/dev/null || true; }

exec 9>>"$lock_file" # creates if missing; this fd carries the lock
start_ts="$(date +%s)"
while ! flock -w "$poll" 9; do
  now="$(date +%s)"
  if (( timeout > 0 )) && (( now - start_ts >= timeout )); then
    echo "run-quality-gates: gave up after ${timeout}s — ${lock_file} still held by: $(holder_info)" >&2
    exit 1
  fi
  echo "run-quality-gates: waiting for the gates lock, held by: $(holder_info)" >&2
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
