#!/usr/bin/env bash
# Accepted-survivor checker for the mutation-testing lane (#1480).
#
# Compares mutants.out/missed.txt (one surviving-mutant name per line,
# as written by cargo-mutants) against scripts/mutation-accepted.txt
# (the accepted baseline: one exact mutant name per line, trailing
# #-comments allowed, blank lines allowed). The operator's 2026-11-28
# ruling on #1480, point 3: a survivor on the accepted list does not
# fail the lane; a NEW survivor fails the shard loudly. Real coverage
# gaps are never accepted — they stay off the list and keep the lane
# red until their issue is fixed.
#
# Exit codes:
#   0  every survivor is on the accepted list (or there are none)
#   1  at least one unaccepted (NEW) survivor — printed under a
#      NEW-SURVIVOR banner and, when stdout is a CI annotation context,
#      a ::error:: per survivor
#   2  malformed input: a missing/unreadable file, or a duplicate
#      accepted entry (a comment-only accepted file is valid: no
#      accepted mutants yet)
#   64 usage error
#
# Stale accepted entries (on the list, not in missed.txt) are reported
# informationally and never affect the exit code — a killed "accepted"
# mutant is good news; pruning is editorial.
#
# Usage:
#   scripts/mutation-accepted-check.sh <missed.txt> [accepted.txt]
#   scripts/mutation-accepted-check.sh --self-test
#
# <missed.txt> may be absent or empty (a fully-caught run) — that is
# exit 0. <accepted.txt> defaults to scripts/mutation-accepted.txt
# next to this script; a missing accepted file is exit 2 (infra), not
# a silent all-new pass.
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
usage: scripts/mutation-accepted-check.sh <missed.txt> [accepted.txt]
       scripts/mutation-accepted-check.sh --self-test

  <missed.txt>    mutants.out/missed.txt from the cargo-mutants run
                  (may be missing or empty: no survivors, exit 0)
  [accepted.txt]  the accepted baseline
                  (default: scripts/mutation-accepted.txt beside this
                  script); missing is exit 2

exit codes: 0 all accepted; 1 NEW survivor(s); 2 malformed input;
            64 usage
EOF
}

# Strip a trailing comment and blank lines; keep one mutant name per
# line. A line whose comment-stripped remainder is empty is data loss
# (an entry without a name) — the caller decides whether that is an
# error (accepted list: yes — fail loud, never silently skip).
parse_entries() { # file -> stdout, one name per line
  LC_ALL=C awk '
    {
      sub(/^[[:space:]]+/, "")              # leading whitespace
      sub(/([[:space:]]|^)#.*$/, "")        # comment: after whitespace or at line start
      sub(/[[:space:]]+$/, "")              # trailing whitespace
      if ($0 != "") print
    }
  ' "$1"
}

run_check() { # missed accepted
  local missed="$1" accepted="$2"

  if [ ! -e "$accepted" ] || [ ! -r "$accepted" ]; then
    echo "ACCEPTED-CHECK: accepted-baseline file missing or unreadable: $accepted" >&2
    return 2
  fi
  local empty_list=0
  if [ ! -e "$missed" ]; then
    empty_list=1
  elif [ ! -r "$missed" ]; then
    echo "ACCEPTED-CHECK: missed.txt exists but is unreadable: $missed" >&2
    return 2
  fi

  local accepted_names stale_duplicates
  accepted_names="$(mktemp)"
  parse_entries "$accepted" > "$accepted_names"
  # Duplicates on the accepted list are a maintenance error: two lines
  # meaning to record two different reasons for one mutant read as one
  # entry and one lost reason. Fail loud (exit 2), never silently skip.
  stale_duplicates="$(LC_ALL=C sort "$accepted_names" | uniq -d)"
  if [ -n "$stale_duplicates" ]; then
    echo "ACCEPTED-CHECK: duplicate entries in $accepted:" >&2
    printf '%s\n' "$stale_duplicates" >&2
    rm -f "$accepted_names"
    return 2
  fi

  local status=0
  local new_names
  new_names="$(mktemp)"
  if [ "$empty_list" -eq 0 ] && [ -s "$missed" ]; then
    # Survivors NOT on the accepted list. comm -23: lines only in
    # missed.txt. The accepted list is parsed+sorted the same way so
    # comment/whitespace differences cannot mask a real match.
    comm -23 \
      <(parse_entries "$missed" | LC_ALL=C sort) \
      <(LC_ALL=C sort -u "$accepted_names") > "$new_names" || status=$?
  fi

  local stale_names
  stale_names="$(comm -13 \
    <(parse_entries "$missed" 2>/dev/null | LC_ALL=C sort || true) \
    <(LC_ALL=C sort -u "$accepted_names") || true)"

  if [ -s "$new_names" ]; then
    status=1
    echo "ACCEPTED-CHECK: NEW surviving mutant(s) not on the accepted baseline ($accepted):"
    while IFS= read -r line; do
      printf 'NEW-SURVIVOR: %s\n' "$line"
      printf '::error::NEW surviving mutant not on the accepted baseline: %s\n' "$line"
    done < "$new_names"
    echo "ACCEPTED-CHECK: triage each survivor: fix the test (typed outcome), or accept only if genuinely unkillable/equivalent (real gaps get a backlog: issue and stay OFF the list)."
  elif [ "$status" -eq 0 ]; then
    if [ "$empty_list" -eq 1 ] || [ ! -s "$missed" ]; then
      echo "ACCEPTED-CHECK: no surviving mutants — all caught."
    else
      echo "ACCEPTED-CHECK: all surviving mutants are on the accepted baseline."
    fi
  fi

  if [ -n "$stale_names" ]; then
    echo "ACCEPTED-CHECK: informational — accepted entries with no matching survivor (prune editorially):"
    printf '%s\n' "$stale_names"
  fi

  rm -f "$accepted_names" "$new_names"
  return "$status"
}

self_test() {
  local tmp
  tmp="$(mktemp -d)"

  local failed=0
  check_case() { # <what> <want> <missed-content> <accepted-content>
    # <missed-content> of the literal token ABSENT removes the file.
    local what="$1" want="$2" got=0
    if [ "$3" = ABSENT ]; then
      rm -f "$tmp/m"
    else
      printf '%b' "$3" > "$tmp/m"
    fi
    printf '%b' "$4" > "$tmp/a"
    run_check "$tmp/m" "$tmp/a" > "$tmp/out" 2>&1 || got=$?
    if [ "$want" != "$got" ]; then
      printf 'self-test FAIL: %s: want exit %s, got %s\n' "$what" "$want" "$got" >&2
      failed=1
    fi
    if [ "$want" -eq 1 ] && ! grep -q '^NEW-SURVIVOR:' "$tmp/out"; then
      printf 'self-test FAIL: %s: NEW-SURVIVOR line not printed\n' "$what" >&2
      failed=1
    fi
  }

  check_case "absent missed file" 0 ABSENT 'mutant-a # reason\n'
  check_case "empty missed file" 0 '' 'mutant-a # reason\n'
  check_case "all survivors accepted" 0 \
    'crates/x/src/a.rs:1:1: replace f with ()\ncrates/x/src/b.rs:2:2: rhs of "g" -> false\n' \
    'crates/x/src/a.rs:1:1: replace f with ()\ncrates/x/src/b.rs:2:2: rhs of "g" -> false\n'
  check_case "one new survivor" 1 \
    'crates/x/src/a.rs:1:1: replace f with ()\ncrates/x/src/NEW.rs:9:9: delete match arm "Zed"\n' \
    'crates/x/src/a.rs:1:1: replace f with () # accepted reason\n'
  check_case "comments and blanks ignored" 0 \
    'crates/x/src/a.rs:1:1: replace f with ()\n' \
    '\n# header comment\ncrates/x/src/a.rs:1:1: replace f with ()   # trailing reason #1480\n'
  check_case "accepted superset is fine (stale informational)" 0 \
    'crates/x/src/a.rs:1:1: replace f with ()\n' \
    'crates/x/src/a.rs:1:1: replace f with ()\ncrates/x/src/old.rs:3:3: replace old with ()\n'
  check_case "partial overlap is a new survivor" 1 \
    'crates/x/src/a.rs:1:1: replace f with ()\ncrates/x/src/c.rs:5:5: replace <= with <\n' \
    'crates/x/src/a.rs:1:1: replace f with ()\n'
  check_case "duplicate accepted entries" 2 \
    'crates/x/src/a.rs:1:1: replace f with ()\n' \
    'crates/x/src/a.rs:1:1: replace f with () # reason one\ncrates/x/src/a.rs:1:1: replace f with () # reason two\n'
  check_case "whitespace/comment-stripped duplicates collide" 2 \
    '' \
    'mutant-x # one\nmutant-x   # two\n'
  check_case "comment-only accepted file is valid (nothing accepted)" 0 \
    '' \
    '# only a comment\n'
  check_case "empty accepted file accepts nothing: survivor is NEW" 1 \
    'crates/x/src/a.rs:1:1: replace f with ()\n' \
    ''

  if [ "$failed" -ne 0 ]; then
    echo "self-test: FAILED" >&2
    rm -rf "$tmp"
    return 1
  fi
  rm -rf "$tmp"
  echo "self-test: all cases passed"
  return 0
}

main() {
  if [ "${1:-}" = "--self-test" ]; then
    self_test
    return $?
  fi
  if [ "${1:-}" = "" ] || [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
    usage
    return 64
  fi
  if [ "$#" -gt 2 ]; then
    usage
    return 64
  fi
  local script_dir accepted
  script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  accepted="${2:-$script_dir/mutation-accepted.txt}"
  run_check "$1" "$accepted"
}

main "$@"
