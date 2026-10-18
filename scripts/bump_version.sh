#!/usr/bin/env bash
# Cut a release through a PR: bump the workspace's single global semver
# on a release branch, then leave the merge + tag to the human.
#
# Usage: ./scripts/bump_version.sh <major|minor|patch> [--no-push]
#
# `main` is branch-protected (AGENTS §8), so nothing here writes to it.
# Steps:
#   1. abort on an unclean tree; verify origin/main and tag freedom
#   2. cut chore/release-v<major>.<minor>.<patch> off fetched origin/main
#   3. increment [workspace.package] version in the root Cargo.toml
#   4. refresh Cargo.lock and promote the CHANGELOG.md release entry
#   5. stage & commit all changed files on the release branch
#   6. unless --no-push: push the branch and open the release PR
# The tag is NEVER created here: it must name the rebased post-merge
# main commit, which does not exist until the human merges. The script
# prints those steps; running the script is the user's push instruction
# (agents run it only with --no-push).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"
CHANGELOG="$ROOT/CHANGELOG.md"

usage() {
    echo "usage: $0 <major|minor|patch> [--no-push]" >&2
    exit 2
}

[ $# -ge 1 ] && [ $# -le 2 ] || usage
KIND="$1"
NO_PUSH=0
case "$KIND" in
    major | minor | patch) ;;
    *) usage ;;
esac
if [ $# -eq 2 ]; then
    [ "$2" = "--no-push" ] || usage
    NO_PUSH=1
fi

cd "$ROOT"

# --- 1. Preconditions -------------------------------------------------------
[ -z "$(git status --porcelain)" ] || {
    echo "error: git tree is not clean; commit or stash first." >&2
    exit 1
}
git fetch origin --prune
git rev-parse -q --verify origin/main >/dev/null || {
    echo "error: origin/main not found; is a remote configured?" >&2
    exit 1
}

CURRENT="$(sed -n 's/^version = "\(.*\)"$/\1/p' "$MANIFEST" | head -n1)"
[ -n "$CURRENT" ] || { echo "error: could not read version from $MANIFEST" >&2; exit 1; }
MAJOR="$(cut -d. -f1 <<<"$CURRENT")"
MINOR="$(cut -d. -f2 <<<"$CURRENT")"
PATCH="$(cut -d. -f3 <<<"$CURRENT")"

case "$KIND" in
    major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
    minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
    patch) PATCH=$((PATCH + 1)) ;;
esac
NEW="$MAJOR.$MINOR.$PATCH"
TAG="v$NEW"
BRANCH="chore/release-v$NEW"

# After the fetch above a remote tag is also a local ref; check the
# remote first so the refusal names the right place.
if git ls-remote --tags origin "refs/tags/$TAG" | grep -q .; then
    echo "error: tag $TAG already exists on origin." >&2
    exit 1
fi
if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    echo "error: tag $TAG already exists locally." >&2
    exit 1
fi
if git rev-parse -q --verify "refs/heads/$BRANCH" >/dev/null; then
    echo "error: branch $BRANCH already exists locally." >&2
    exit 1
fi
if git ls-remote --heads origin "$BRANCH" | grep -q .; then
    echo "error: branch $BRANCH already exists on origin." >&2
    exit 1
fi

echo "bumping version: $CURRENT -> $NEW (on $BRANCH off origin/main)"

# --- 2. Release branch ------------------------------------------------------
# Always cut from fetched origin/main, never from whatever HEAD is.
git switch -c "$BRANCH" origin/main

# --- 3. Bump the version ----------------------------------------------------
# Only the first `version = "..."` line matches: [workspace.package] precedes
# every other occurrence in the root manifest.
python3 - "$MANIFEST" "$CURRENT" "$NEW" <<'PY'
import re, sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path, encoding="utf-8").read()
patched, count = re.subn(
    rf'^(version = "){re.escape(old)}"$', rf'\g<1>{new}"', text, count=1, flags=re.M
)
if count != 1:
    sys.exit(f"error: expected exactly one version bump, patched {count}")
open(path, "w", encoding="utf-8").write(patched)
PY

# --- 4. Refresh lockfile & changelog ----------------------------------------
cargo update --workspace --quiet

DATE="$(date +%Y-%m-%d)"
if grep -q "^## \[Unreleased\]" "$CHANGELOG"; then
    # changelog.py promotes the Unreleased content into a versioned section
    # (no-op with a warning when Unreleased is empty — the bump still stands).
    python3 "$ROOT/scripts/changelog.py" promote "$NEW" "$DATE" "$CHANGELOG"
else
    echo "warning: no '## [Unreleased]' section in $CHANGELOG; skipping changelog edit." >&2
fi

# --- 5. Commit ---------------------------------------------------------------
git add "$MANIFEST" "Cargo.lock" "$CHANGELOG"
git commit -m "chore(release): bump workspace version to $NEW

Single global semver bump of the workspace version (was $CURRENT).
All member crates inherit via version.workspace = true.
Regenerated Cargo.lock; cut the CHANGELOG.md release entry.
Cut on the release branch: main is branch-protected, the bump
lands through a PR."

# --- 6. Ship -----------------------------------------------------------------
HUMAN_STEPS="Release branch ready. After the PR merges (rebase merge), tag the
merged main commit and push the tag — that triggers the Release workflow:

  git fetch origin --prune
  git tag -a $TAG -m \"taskboard $NEW\" origin/main
  git push origin $TAG"

if [ "$NO_PUSH" -eq 1 ]; then
    echo "$HUMAN_STEPS"
    echo "push + PR left to you (--no-push):"
    echo "  git push -u origin $BRANCH"
    echo "  gh pr create --title 'chore(release): v$NEW' --fill"
    exit 0
fi

git push -u origin "$BRANCH"
# The PR body is the human's release review: the promoted section's
# notes, not just the diff. Promotion can have been skipped (empty or
# missing [Unreleased]); say so instead of failing the already-pushed cut.
RELEASE_NOTES="$(python3 "$ROOT/scripts/changelog.py" extract "$NEW" "$CHANGELOG" 2>/dev/null || true)"
if command -v gh >/dev/null && gh pr create \
    --title "chore(release): v$NEW" \
    --body "Release $NEW.

${RELEASE_NOTES:-"(no promoted CHANGELOG section for $NEW)"}

$HUMAN_STEPS"; then
    echo "$HUMAN_STEPS"
else
    echo "error: 'gh pr create' failed or gh is missing; the branch was pushed." >&2
    echo "open the PR by hand, then:" >&2
    echo "$HUMAN_STEPS" >&2
    exit 1
fi
