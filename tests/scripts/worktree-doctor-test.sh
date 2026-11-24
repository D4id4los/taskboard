#!/usr/bin/env bash
# Fixture test for scripts/worktree-doctor.sh — the #1185 grace window.
#
# Pattern per #1186's scripts-fixture precedent: a throwaway temp repo
# exercises the doctor's decision rows as typed outcomes (KEEP/REMOVE
# row prefixes), never text beyond the decision column. No wall-clock
# assertions: the grace check reads the branch reflog birth, and the
# test controls the outcome through the WORKTREE_DOCTOR_GRACE_HOURS
# env override (0 = aged out), not by sleeping or stamping clocks.
#
# Usage: tests/scripts/worktree-doctor-test.sh   (repo root or anywhere)
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
doctor="$root/scripts/worktree-doctor.sh"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

git -C "$scratch" init --quiet --initial-branch=main repo
cd "$scratch/repo"
git config user.email test@example.invalid
git config user.name test
echo one > f && git add f && git commit -q -m one
stale_tip="$(git rev-parse HEAD)"

# Fake fetched tip: origin/main strictly ahead of the local main ref —
# the stale-local-main shape a mid-round of landed PRs produces.
echo two > f && git add f && git commit -q -m two
fetched_tip="$(git rev-parse HEAD)"
git update-ref refs/remotes/origin/main "$fetched_tip"
git update-ref refs/heads/main "$stale_tip"

# Fresh worktree, zero commits, branched off the stale local main.
git worktree add -q -b fresh-agent "$scratch/WT-fresh-agent" 2>/dev/null

run_doctor() { # prints the doctor's decision rows (fixture repo only)
  (cd "$root" && WORKTREE_DOCTOR_ROOT="$scratch/repo" \
     WORKTREE_DOCTOR_MIN_FREE_G=0 WORKTREE_DOCTOR_MIN_AVAIL_MIB=0 \
     WORKTREE_DOCTOR_GRACE_HOURS="${1:-2}" \
     bash "$doctor") | grep -F "$scratch/"
}

# 1. Young branch + contained head + stale local main → grace KEEP.
decisions="$(run_doctor 2)"
grep -q '^KEEP' <<<"$decisions" || {
  echo "FAIL: fresh worktree was not kept under the grace window:" >&2
  echo "$decisions" >&2
  exit 1
}

# 2. Grace disabled (the aged-out shape) → the pre-existing REMOVE
#    decision (#1185's hole, now explicit and testable).
decisions="$(run_doctor 0)"
grep -q '^REMOVE' <<<"$decisions" || {
  echo "FAIL: aged zero-commit contained worktree was not removed:" >&2
  echo "$decisions" >&2
  exit 1
}

# 3. A branch with unique commits is never grace-removed (the
#    landed-PR-leftover false-positive guard): a second worktree whose
#    branch carries a commit, grace disabled, expect KEEP (in flight /
#    never pushed).
git worktree add -q -b unique-agent "$scratch/WT-unique-agent" 2>/dev/null
git -C "$scratch/WT-unique-agent" commit -q --allow-empty -m wip
decisions="$(run_doctor 0)"
grep -qF "$scratch/WT-unique-agent" <<<"$decisions" \
  && grep -q '^KEEP' <<<"$decisions" || {
  echo "FAIL: unique-commit branch was removed:" >&2
  echo "$decisions" >&2
  exit 1
}

# 4. #1277 locale sort: a case-mixed + non-ASCII WT-suffix pair must not
#    make comm spew `not in sorted order` under a UTF-8 collation. The
#    doctor's stderr is asserted STRUCTURALLY (no comm sort complaint),
#    unfiltered — the guard genuinely fails without the LC_ALL=C pin.
pick_utf8_locale() { # first available UTF-8 locale that collates non-byte-order
  local l
  for l in en_US.UTF-8 en_US.utf8 de_AT.UTF-8; do
    locale -a 2>/dev/null | grep -qiF "$l" && { echo "$l"; return 0; }
  done
  return 1
}
if utf8_locale="$(pick_utf8_locale)"; then
  git worktree add -q -b Zebra-agent "$scratch/WT-Zebra-agent" 2>/dev/null
  mkdir -p "$scratch/WT-apple-örphan" # unregistered orphan sibling
  doctor_err="$scratch/doctor.err"
  (cd "$root" && WORKTREE_DOCTOR_ROOT="$scratch/repo" \
     WORKTREE_DOCTOR_MIN_FREE_G=0 WORKTREE_DOCTOR_MIN_AVAIL_MIB=0 \
     WORKTREE_DOCTOR_GRACE_HOURS=0 LC_ALL="$utf8_locale" \
     bash "$doctor") >/dev/null 2>"$doctor_err"
  if grep -q "not in sorted order" "$doctor_err"; then
    echo "FAIL: comm emitted sort complaints under LC_ALL=$utf8_locale with case-mixed/non-ASCII WT names:" >&2
    sed 's/^/    /' <"$doctor_err" >&2
    exit 1
  fi
  echo "worktree-doctor fixture: 4/4 ok"
else
  # No non-byte-order collation installed: the pin is untestable here
  # (byte order == collation order) — report, don't lie with a vacuous
  # pass.
  echo "worktree-doctor fixture: 3/3 ok (locale test skipped: no UTF-8 collation locale installed)"
fi
