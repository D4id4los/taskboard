#!/usr/bin/env bash
# The dead-man's switch for the weekly scheduled lanes (#1413).
#
# Three evidence-backed lanes sat dead 5-6 weeks before anyone noticed
# (#1401/#1402/#1403). This script holds the WHOLE classification core,
# so it runs locally (a lane watcher that only exists in CI dies
# silently — the trap this design exists to kill) and is unit-testable
# without touching GitHub (`--self-test` over synthetic run-JSON
# fixtures, typed verdict/exit-code assertions only).
#
# Watched lanes (workflow-file + its timeout cap, for the timeout-shape
# rule). lane-watch.yml watches ITSELF (#1413 failure-path law): two
# dead watcher weeks are reported by the next live run.
#
# Classification per lane, over the `event == scheduled` COMPLETED runs
# (newest first, as `gh run list` returns them):
#   stale         — no completed scheduled run, or the newest is older
#                   than 8 days (one missed weekly cycle).
#   timeout-shape — ≥2 consecutive scheduled runs whose conclusion is
#                   not `success`, ALL of them `cancelled` within ~2
#                   min of the lane's cap (the exact #1401/#1402 tell).
#   streak        — ≥2 consecutive scheduled runs not `success`, not
#                   timeout-shaped (`failure` counts red regardless of
#                   duration).
#   fresh         — otherwise (green, or fewer than 2 red runs).
# A SHORT `cancelled` (concurrency supersede) is skipped and never
# counted; runs still `in_progress`/`queued` are skipped too. Scheduled
# events cannot practically supersede (one run/week) — the duration
# rule is belt-and-braces.
#
# Verdict output (typed, one row per lane):
#   `<workflow-file> <verdict> <reason>` with verdict in {green, red},
#   reason in {fresh, stale, streak, timeout-shape}.
#
# Exit codes (never conflate "all green" with "could not check"):
#   0 — every lane green
#   1 — at least one lane red
#   2 — tooling error (gh missing / unauthenticated / API failure)
#
# The script NEVER comments itself: the lane-watch.yml wrapper owns the
# search-first comment step and consumes these verdict rows. --dry-run
# is the first-dispatch guard: identical output plus an explicit
# no-comments banner, safe to run by hand.
#
# Usage:
#   scripts/lane-watch.sh             classify the watch list (live gh)
#   scripts/lane-watch.sh --dry-run   same, with the no-comments banner
#   scripts/lane-watch.sh --self-test run the fixture tests, exit
#                                     nonzero on any failure
# Env seams (tests only):
#   LANE_WATCH_NOW  epoch seconds assumed "now" (injected clock seam)
#   GH              gh binary override (self-test uses a fake)
set -euo pipefail

# The watch list: <workflow-file> <cap-minutes>.
watch_list=(
  "miri.yml 120"
  "mutation-testing.yml 240"
  "kani.yml 120"
  "nextcloud-integration.yml 30"
  "benchmark.yml 45"
  "lane-watch.yml 10"
)

# Classification core: pure, no gh. args: <cap_minutes> <now_epoch>
# <runs-json>. Prints "<verdict> <reason>" — the single contract every
# caller (live path, self-test) reads.
classify_lane() {
  python3 - "$1" "$2" "$3" <<'PYEOF'
import json, sys
from datetime import datetime

cap_min = int(sys.argv[1])
now = float(sys.argv[2])
runs = json.loads(sys.argv[3])
STALE_SECS = 8 * 24 * 3600      # one missed weekly cycle
GRACE_SECS = 2 * 60             # "within ~2 min of the cap"

def ts(r):
    return datetime.fromisoformat(r["createdAt"].replace("Z", "+00:00")).timestamp()

sched = [r for r in runs
         if r.get("event") == "schedule" and r.get("status") == "completed"]

if not sched:
    print("red stale")
    raise SystemExit

if now - ts(sched[0]) > STALE_SECS:
    print("red stale")
    raise SystemExit

red_streak = 0          # consecutive scheduled runs that are not success
timeout_shape = 0       # of those, cap-shaped cancels
verdict, reason = "green", "fresh"
for r in sched:
    concl = r.get("conclusion")
    if concl == "success":
        break
    if concl == "cancelled":
        dur = datetime.fromisoformat(
            r["updatedAt"].replace("Z", "+00:00")).timestamp() - ts(r)
        if dur >= cap_min * 60 - GRACE_SECS:
            red_streak += 1
            timeout_shape += 1
        else:
            continue   # short cancelled (supersede): skipped, never counted
    elif concl is not None:
        red_streak += 1   # failure (or any other non-success verdict)
    else:
        continue          # null conclusion: not yet a verdict, skip
    if red_streak >= 2:
        if timeout_shape == red_streak:
            verdict, reason = "red", "timeout-shape"
        else:
            verdict, reason = "red", "streak"
        break
print(verdict, reason)
PYEOF
}

fetch_runs() { # fetch_runs <workflow-file> — runs JSON on stdout, or fail
  "${GH:-gh}" run list --workflow "$1" --limit 15 \
    --json event,status,conclusion,createdAt,updatedAt
}

now_epoch() { echo "${LANE_WATCH_NOW:-$(date +%s)}"; }

# ---- self-test: synthetic fixtures, typed assertions only ----
self_test() {
  local fails=0
  local now=1800000000   # fixed "now" (2027-01-15T00:00:00Z)

  check() { # check <name> <cap> <fixture-json> <expected "verdict reason">
    local got expected="$4"
    got="$(classify_lane "$2" "$now" "$3" 2>/dev/null || echo "CRASH")"
    if [[ $got == "$expected" ]]; then
      echo "self-test $1: PASS"
    else
      echo "self-test $1: FAIL (got '$got', want '$expected')"
      fails=$((fails + 1))
    fi
  }

  d() { date -u -d "@$((now - $1))" +%Y-%m-%dT%H:%M:%SZ; }
  local fresh_1d mid_3d old_9d
  fresh_1d="$(d $((1 * 86400)))"
  mid_3d="$(d $((3 * 86400)))"
  old_9d="$(d $((9 * 86400)))"
  # cap-shaped cancels: started a full cap before their end
  local t240s t240e t10s t10e t119s t119e
  t240s="$(d $((3 * 86400 + 240 * 60)))"; t240e="$(d $((3 * 86400 + 40)))"
  t10s="$(d $((3 * 86400 + 10 * 60)))";   t10e="$(d $((3 * 86400 + 5 * 60)))"
  t119s="$(d $((3 * 86400 + 119 * 60)))"; t119e="$(d $((3 * 86400 + 30)))"

  ok() {   # ok <created> <updated> — a successful scheduled run
    printf '{"event":"schedule","status":"completed","conclusion":"success","createdAt":"%s","updatedAt":"%s"}' "$1" "$2"
  }
  fail_() { # a failed scheduled run
    printf '{"event":"schedule","status":"completed","conclusion":"failure","createdAt":"%s","updatedAt":"%s"}' "$1" "$2"
  }
  cancel() { # a cancelled scheduled run starting <start> ending <end>
    printf '{"event":"schedule","status":"completed","conclusion":"cancelled","createdAt":"%s","updatedAt":"%s"}' "$1" "$2"
  }

  # 1. green fresh
  check 1-green-fresh 120 "[$(ok "$mid_3d" "$fresh_1d")]" "green fresh"
  # 2. stale: newest scheduled run 9 days old
  check 2-stale 120 "[$(ok "$old_9d" "$old_9d")]" "red stale"
  # 3. no scheduled runs at all (only a push run) -> stale
  check 3-no-scheduled-runs 120 \
    "[{\"event\":\"push\",\"status\":\"completed\",\"conclusion\":\"success\",\"createdAt\":\"$fresh_1d\",\"updatedAt\":\"$fresh_1d\"}]" \
    "red stale"
  # 4. two consecutive failures -> streak (failure counts, any duration)
  check 4-two-failures-streak 120 \
    "[$(fail_ "$mid_3d" "$fresh_1d"),$(fail_ "$old_9d" "$old_9d")]" \
    "red streak"
  # 5. timeout-shaped cancel pair (cap 240, both ran ~239 min) -> timeout-shape
  check 5-timeout-shape 240 \
    "[$(cancel "$t240s" "$t240e"),$(cancel "$t240s" "$t240e")]" \
    "red timeout-shape"
  # 6. the same shape at cap 120 (miri's tell) -> timeout-shape
  check 6-timeout-shape-120 120 \
    "[$(cancel "$t119s" "$t119e"),$(cancel "$t119s" "$t119e")]" \
    "red timeout-shape"
  # 7. short cancelled does NOT count: 10-min cancel over successes -> green
  check 7-short-cancel-neutral 240 \
    "[$(cancel "$t10s" "$t10e"),$(ok "$mid_3d" "$fresh_1d"),$(ok "$old_9d" "$old_9d")]" \
    "green fresh"
  # 8. single failure is not a streak (needs 2)
  check 8-single-failure-green 120 \
    "[$(fail_ "$mid_3d" "$fresh_1d"),$(ok "$old_9d" "$old_9d")]" \
    "green fresh"
  # 9. failure + short cancel: cancel skipped, one red run -> green
  check 9-cancel-skipped-single-failure 120 \
    "[$(fail_ "$mid_3d" "$fresh_1d"),$(cancel "$t10s" "$t10e"),$(ok "$old_9d" "$old_9d")]" \
    "green fresh"
  # 10. in-progress scheduled runs are skipped entirely -> judged on history
  check 10-inprogress-skipped 120 \
    "[{\"event\":\"schedule\",\"status\":\"in_progress\",\"conclusion\":null,\"createdAt\":\"$fresh_1d\",\"updatedAt\":\"$fresh_1d\"},$(fail_ "$mid_3d" "$fresh_1d"),$(fail_ "$old_9d" "$old_9d")]" \
    "red streak"

  # 11-13. live-path exit contract through a fake gh over fixture files
  # (tmp is deliberately global: the EXIT trap fires at top level, where
  # a function-local would be unbound under set -u)
  local code
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT
  cat >"$tmp/fake-gh" <<FAKE
#!/usr/bin/env bash
if [[ \${1:-} == auth ]]; then exit 0; fi
cat "$tmp/fixture.json"
FAKE
  chmod +x "$tmp/fake-gh"

  echo "[$(ok "$fresh_1d" "$fresh_1d")]" >"$tmp/green.json"
  echo "[$(fail_ "$mid_3d" "$fresh_1d"),$(fail_ "$old_9d" "$old_9d")]" >"$tmp/red.json"

  run_live() { # run_live -> runs this script's live path under the fake gh
    GH="$tmp/fake-gh" LANE_WATCH_NOW="$now" "$0" 2>/dev/null
  }

  cp "$tmp/green.json" "$tmp/fixture.json"
  code=0; run_live >/dev/null || code=$?
  if (( code == 0 )); then echo "self-test 11-all-green-exit-0: PASS"; else
    echo "self-test 11-all-green-exit-0: FAIL (exit $code, want 0)"; fails=$((fails+1)); fi

  cp "$tmp/red.json" "$tmp/fixture.json"
  code=0; run_live >/dev/null || code=$?
  if (( code == 1 )); then echo "self-test 12-any-red-exit-1: PASS"; else
    echo "self-test 12-any-red-exit-1: FAIL (exit $code, want 1)"; fails=$((fails+1)); fi

  # API failure: fixture file missing -> cat fails, fake gh exits nonzero
  rm -f "$tmp/fixture.json"
  code=0; run_live >/dev/null 2>&1 || code=$?
  if (( code == 2 )); then echo "self-test 13-gh-error-exit-2: PASS"; else
    echo "self-test 13-gh-error-exit-2: FAIL (exit $code, want 2)"; fails=$((fails+1)); fi

  if (( fails == 0 )); then
    echo "lane-watch self-test: ALL PASS"
    return 0
  fi
  echo "lane-watch self-test: $fails FAILURES"
  return 1
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
  exit $?
fi

# ---- live path ----
command -v "${GH:-gh}" >/dev/null 2>&1 || {
  echo "lane-watch: gh not found" >&2
  exit 2
}
if ! "${GH:-gh}" auth status >/dev/null 2>&1; then
  echo "lane-watch: gh not authenticated" >&2
  exit 2
fi

if [[ "${1:-}" == "--dry-run" ]]; then
  echo "lane-watch: DRY RUN — the caller must not post comments from this output"
fi

now="$(now_epoch)"
any_red=0
tool_error=0
rows=()
for entry in "${watch_list[@]}"; do
  read -r wf cap <<<"$entry"
  if ! json="$(fetch_runs "$wf")" || [[ -z $json ]]; then
    tool_error=1
    continue
  fi
  read -r verdict reason < <(classify_lane "$cap" "$now" "$json")
  rows+=("$wf $verdict $reason")
  [[ $verdict == red ]] && any_red=1
done

if (( tool_error )); then
  printf '%s\n' "${rows[@]}" >&2   # report what WAS checked, then fail
  echo "lane-watch: gh query failed — could-not-check is exit 2, never green" >&2
  exit 2
fi

printf '%s\n' "${rows[@]}"
if (( any_red )); then
  exit 1
fi
exit 0
