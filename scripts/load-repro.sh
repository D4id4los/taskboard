#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# PID-recorded, trap-cleaned load-reproduction runner (#607).
#
# Pattern contract for future repro recipes: every background child has
# its PID recorded in a file at spawn time, and cleanup kills *those
# PIDs* from an EXIT/INT/TERM trap — never `kill %N`, whose job specs
# are a no-op in non-interactive shells and leak spinners past the
# script's exit. Load work is bounded (loop-count deadline), so a
# cleanup that never fires cannot spin forever either.
#
# Usage:
#   scripts/load-repro.sh [SPIN_STEPS]   # spin for SPIN_STEPS busy
#                                        # iterations per worker, then exit
#   scripts/load-repro.sh --self-test    # prove the EXIT trap kills
#                                        # every recorded PID (typed
#                                        # PASS/FAIL, nonzero exit on
#                                        # any survivor)

set -euo pipefail

WORKERS="${WORKERS:-4}"
# Scratch lands under /var/tmp by default, never host tmpfs /tmp
# (docs/agent_dispatch.org §2, the #1043 ENOSPC law); an operator-set
# TMPDIR pointing somewhere other than /tmp is honored.
SPIN_DIR="$(mktemp -d "${TMPDIR:-/var/tmp}/load-repro.XXXXXX")"
SELF_TEST=0

# Kill every recorded PID, then — in self-test mode — verify from the
# trap context that each one is actually gone before the pid files go
# away. `kill -0` is polled with a bounded wait: SIGTERM delivery is
# asynchronous, and a script has no asynchronous runtime to await.
# shellcheck disable=SC2329 # registered via `trap cleanup EXIT INT TERM` below; shellcheck does not count trap registration as invocation
cleanup() {
    for pid_file in "$SPIN_DIR"/*.pid; do
        # Glob unmatched: no children were recorded.
        [ -f "$pid_file" ] || continue
        kill "$(cat "$pid_file")" 2>/dev/null || true
    done
    if [ "$SELF_TEST" -eq 1 ]; then
        local survivors=0 total=0
        for pid_file in "$SPIN_DIR"/*.pid; do
            [ -f "$pid_file" ] || continue
            local pid
            pid="$(cat "$pid_file")"
            total=$((total + 1))
            local waited=0
            while kill -0 "$pid" 2>/dev/null && [ "$waited" -lt 100 ]; do
                sleep 0.05
                waited=$((waited + 1))
            done
            if kill -0 "$pid" 2>/dev/null; then
                kill -9 "$pid" 2>/dev/null || true
                survivors=$((survivors + 1))
                echo "FAIL: spinner pid $pid survived past the trap"
            else
                echo "ok: spinner pid $pid is dead (trap killed it)"
            fi
        done
        if [ "$survivors" -eq 0 ]; then
            echo "PASS: all $total recorded spinner pids were killed by the cleanup trap"
        else
            echo "FAIL: $survivors/$total recorded spinner pids survived the cleanup trap"
            SELF_TEST_FAILURE=1
        fi
    fi
    rm -rf "$SPIN_DIR"
}
SELF_TEST_FAILURE=0
trap cleanup EXIT INT TERM

# One bounded busy-loop worker: `steps` iterations of nothing — a CPU
# spinner with a hard deadline, per #607's "bounded spinner" shape.
SPINNER_SEQ=0
spawn_spinner() {
    local steps="${1:-6000000}"
    (
        local n=0
        while [ "$n" -lt "$steps" ]; do
            n=$((n + 1))
        done
    ) &
    SPINNER_SEQ=$((SPINNER_SEQ + 1))
    echo "$!" >"$SPIN_DIR/spinner-$SPINNER_SEQ.pid"
}

self_test() {
    SELF_TEST=1
    # Long enough that no spinner can finish on its own before the
    # trap fires — a "dead" verdict then proves the trap, not a
    # natural exit.
    local long_steps=2000000000
    for _ in $(seq 1 "$WORKERS"); do
        spawn_spinner "$long_steps"
    done
    # Let the workers actually start (shell script, not a Rust test —
    # the hermetic-test clock laws do not reach here).
    sleep 1
}

case "${1:---run}" in
--self-test) self_test ;;
*)
    for _ in $(seq 1 "$WORKERS"); do
        spawn_spinner "${1:-6000000}"
    done
    wait
    ;;
esac
exit "$SELF_TEST_FAILURE"
