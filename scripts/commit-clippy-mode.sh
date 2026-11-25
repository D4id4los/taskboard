#!/usr/bin/env bash
# Commit-clippy admission mode (#1274): which memory-gate mode the
# pre-commit hook's clippy step uses for the staged diff.
#
# Reads the STAGED diff (not the worktree — partial staging is legal,
# R3): `git diff --cached --name-only --diff-filter=ACMR HEAD`, falling
# back to `--cached --name-only` against the empty tree when HEAD does
# not exist (a new-file-only first commit).
#
# Verdict:
#   preflight — NO staged path is Rust-relevant (docs-only commits) AND
#               this worktree's build cache is warm (a non-empty
#               worktree-local target/debug/.fingerprint). BOTH
#               premises must hold (#1424): the warm-cache premise was
#               implicit until a fresh worktree's docs-only commit ran
#               a cold `cargo clippy --workspace --all-targets`
#               un-gated and pushed MemAvailable through the floor.
#               On a warm cache clippy recompiles nothing (a
#               cache-validating walk in seconds), so it admits via
#               `scripts/memory-gate.sh --preflight`: the MemAvailable
#               floor is still enforced (fail closed), but the commit
#               does not wait behind a 40-minute mutants shard holding
#               the exclusive lock. Oomd-law compatibility (2026-11-23):
#               the exclusive lock exists to keep heavy-command
#               concurrency at 1; a zero-Rust clippy run is not a heavy
#               command and never skips the floor check.
#   gated     — at least one staged path is Rust-relevant (*.rs,
#               Cargo.toml, Cargo.lock, .cargo/*.toml), OR the staged
#               set could not be determined, OR the cache-warmth check
#               fails (fresh worktree, wiped target/, unreadable dir —
#               conservative default: never weaken the gate by
#               accident).
#
# Usage: scripts/commit-clippy-mode.sh   (inside a git repository)
# Exit: 0 with the verdict on stdout; 1 = not a git repository.
set -euo pipefail

if ! git rev-parse --git-dir >/dev/null 2>&1; then
  echo "commit-clippy-mode: not inside a git repository" >&2
  exit 1
fi

if git rev-parse --verify --quiet HEAD >/dev/null; then
  paths="$(git diff --cached --name-only --diff-filter=ACMR HEAD)"
else
  # New-file-only commit with no HEAD baseline: diff against the empty
  # tree (4b825dc6…) so staged additions are still enumerated.
  empty_tree="$(git hash-object -w -t tree --stdin </dev/null)"
  paths="$(git diff --cached --name-only --diff-filter=ACMR "$empty_tree")"
fi

if [[ -z $paths ]]; then
  # Nothing staged (amend-less empty commit path) or undeterminable:
  # conservative default.
  echo "gated"
  exit 0
fi

if grep -Eq '(^|/)\.cargo/[^/]*\.toml$|(^|/)Cargo\.(toml|lock)$|\.rs$' <<<"$paths"; then
  echo "gated"
  exit 0
fi

# Cache-warmth premise (#1424): preflight clippy is only cheap when this
# worktree has actually built before. The fingerprint dir is
# worktree-local (each worktree has its own target/), so a fresh
# worktree — or one whose target/ was wiped — fails this check and
# gates. "Non-empty" means at least one entry beyond . and ..; an
# unreadable or missing dir is undeterminable ⇒ gated.
fingerprint_dir="$(git rev-parse --show-toplevel)/target/debug/.fingerprint"
if [[ ! -d $fingerprint_dir ]] || ! [[ -n $(ls -A "$fingerprint_dir" 2>/dev/null) ]]; then
  echo "gated"
else
  echo "preflight"
fi
