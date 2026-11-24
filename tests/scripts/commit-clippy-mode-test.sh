#!/usr/bin/env bash
# Fixture test for scripts/commit-clippy-mode.sh + the pre-commit hook's
# clippy admission branch (#1274).
#
# Typed outcomes only (no text assertions beyond the mode helper's
# two-verdict contract, which IS the machine interface): the helper's
# verdict is asserted exactly, and the end-to-end proof (test 5)
# asserts completion of a docs-only commit's clippy step through
# `memory-gate.sh --preflight` while the memory-gate lock is HELD —
# AC-1 of the D2 plan. Time-manipulation rule honored: completion is
# awaited by a poll-with-deadline loop, never sleeps-then-hope.
#
# Usage: tests/scripts/commit-clippy-mode-test.sh  (repo root or anywhere)
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
helper="$root/scripts/commit-clippy-mode.sh"

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# A throwaway repo mirroring the layout the hook expects (scripts/,
# .githooks/) — the hook cd's to the toplevel and invokes everything by
# its repo-relative path.
new_fixture_repo() { # new_fixture_repo <name> -> prints repo path
  local repo="$scratch/$1"
  git init --quiet --initial-branch=main "$repo"
  git -C "$repo" config user.email test@example.invalid
  git -C "$repo" config user.name test
  mkdir -p "$repo/scripts" "$repo/.githooks" "$repo/docs"
  cp "$helper" "$repo/scripts/"
  cp "$root/scripts/memory-gate.sh" "$root/scripts/gate-lock-liveness.sh" \
    "$repo/scripts/"
  cp "$root/.githooks/pre-commit" "$repo/.githooks/"
  cp "$root/scripts/check_slint_tokens.py" "$repo/scripts/"
  printf '# docs\nhello\n' >"$repo/docs/readme.md"
  git -C "$repo" add -A
  git -C "$repo" commit -q -m init
  echo "$repo"
}

# ---------------------------------------------------------------------------
# 1. Docs-only staged set -> preflight.
# ---------------------------------------------------------------------------
repo1="$(new_fixture_repo repo1)"
printf 'more\n' >>"$repo1/docs/readme.md"
printf 'notes\n' >"$repo1/docs/notes.org"
git -C "$repo1" add docs/
verdict="$(git -C "$repo1" rev-parse --show-toplevel >/dev/null; cd "$repo1" && bash "$repo1/scripts/commit-clippy-mode.sh")"
[[ $verdict == preflight ]] || fail "1: docs-only staged set gave '$verdict', want preflight"
echo "ok 1 - docs-only staged set -> preflight"

# ---------------------------------------------------------------------------
# 2. One staged *.rs among docs -> gated.
# ---------------------------------------------------------------------------
repo2="$(new_fixture_repo repo2)"
printf 'fn main() {}\n' >"$repo2/src_stub.rs"
printf 'more\n' >>"$repo2/docs/readme.md"
git -C "$repo2" add docs/ src_stub.rs
verdict="$(cd "$repo2" && bash "$repo2/scripts/commit-clippy-mode.sh")"
[[ $verdict == gated ]] || fail "2: rs-among-docs staged set gave '$verdict', want gated"
echo "ok 2 - staged *.rs among docs -> gated"

# ---------------------------------------------------------------------------
# 3. Staged Cargo.toml / Cargo.lock (no .rs) -> gated.
# ---------------------------------------------------------------------------
repo3="$(new_fixture_repo repo3)"
printf '[workspace]\n' >"$repo3/Cargo.toml"
printf '[]\n' >"$repo3/Cargo.lock"
git -C "$repo3" add Cargo.toml Cargo.lock
verdict="$(cd "$repo3" && bash "$repo3/scripts/commit-clippy-mode.sh")"
[[ $verdict == gated ]] || fail "3: staged Cargo.toml/Cargo.lock gave '$verdict', want gated"
echo "ok 3 - staged Cargo.toml/Cargo.lock -> gated"

# ---------------------------------------------------------------------------
# 4. Empty stage (nothing to commit path) -> gated (conservative default).
# ---------------------------------------------------------------------------
repo4="$(new_fixture_repo repo4)"
verdict="$(cd "$repo4" && bash "$repo4/scripts/commit-clippy-mode.sh")"
[[ $verdict == gated ]] || fail "4: empty stage gave '$verdict', want gated"
echo "ok 4 - empty stage -> gated (conservative default)"

# ---------------------------------------------------------------------------
# 5. The proof-of-law test (#1274, AC-1): with the memory-gate lock HELD
#    by a foreign flock holder, a docs-only commit's clippy step (mode =
#    preflight) completes end to end through the real hook — preflight
#    floor-checks and never waits for the lock. A `gated` verdict here
#    would queue behind the holder and blow the deadline.
# ---------------------------------------------------------------------------
repo5="$(new_fixture_repo repo5)"
# Fake cargo: every subcommand (fmt, clippy) logs and succeeds — the
# assertion is the hook's admission path, not a real build.
mkdir -p "$repo5/bin"
cat >"$repo5/bin/cargo" <<'STUB'
#!/usr/bin/env bash
printf 'cargo-stub invoked: %s\n' "$*" >>"${CARGO_STUB_LOG:?}"
exit 0
STUB
chmod +x "$repo5/bin/cargo"
lock_file="$(git -C "$repo5" rev-parse --path-format=absolute --git-common-dir)/memory-gate.lock"
# Hold the lock for the whole window; released when the subshell dies.
flock "$lock_file" -c 'sleep 60' &
holder_pid=$!
cleanup_holder() { kill "$holder_pid" 2>/dev/null || true; wait "$holder_pid" 2>/dev/null || true; }
trap 'cleanup_holder; rm -rf "$scratch"' EXIT
sleep 0.2 # let the holder take the flock (kernel-guaranteed by flock return)
flock -n "$lock_file" true 2>/dev/null && fail "5: could not acquire test hold on $lock_file"

printf 'held-lock probe\n' >"$repo5/docs/probe.md"
git -C "$repo5" add docs/probe.md
verdict="$(cd "$repo5" && bash "$repo5/scripts/commit-clippy-mode.sh")"
[[ $verdict == preflight ]] || fail "5: docs-only mode gave '$verdict', want preflight (the docs-only proof requires the preflight branch)"

log="$repo5/cargo-stub.log"
rm -f "$log"
hook_ok=0
deadline=$(( $(date +%s) + 30 ))
( cd "$repo5" && PATH="$repo5/bin:$PATH" CARGO_STUB_LOG="$log" \
    bash "$repo5/.githooks/pre-commit" ) >"$repo5/hook.out" 2>&1 &
hook_pid=$!
while kill -0 "$hook_pid" 2>/dev/null; do
  if (( $(date +%s) >= deadline )); then
    kill "$hook_pid" 2>/dev/null || true
    sed 's/^/    /' "$repo5/hook.out" >&2
    fail "5: docs-only clippy step did not complete within 30s while the memory-gate lock was HELD (gated-mode queueing?)"
  fi
  sleep 0.5
done
wait "$hook_pid" && hook_ok=1 || true
[[ $hook_ok -eq 1 ]] || {
  sed 's/^/    /' "$repo5/hook.out" >&2
  fail "5: the hook exited nonzero while the lock was held"
}
grep -q '^cargo-stub invoked: clippy --workspace' "$log" || fail "5: the stubbed clippy step never ran (log: $(cat "$log" 2>/dev/null))"
echo "ok 5 - docs-only clippy step completed through --preflight while the memory-gate lock was HELD"

echo "commit-clippy-mode fixture: 5/5 ok"
