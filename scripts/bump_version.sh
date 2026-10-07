#!/usr/bin/env bash
# Bump the workspace's single global semver and cut a release commit + tag.
#
# Usage: ./scripts/bump_version.sh <major|minor|patch>
#
# Steps (per docs/coding_guidelines.org §7):
#   1. abort on an unclean git tree or a non-release branch
#   2. increment [workspace.package] version in the root Cargo.toml
#   3. refresh Cargo.lock and write the CHANGELOG.md release entry
#   4. stage & commit all changed files
#   5. tag v<major>.<minor>.<patch> (GitHub-parseable)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$ROOT/Cargo.toml"
CHANGELOG="$ROOT/CHANGELOG.md"

usage() {
    echo "usage: $0 <major|minor|patch>" >&2
    exit 2
}

[ $# -eq 1 ] || usage
KIND="$1"
case "$KIND" in
    major | minor | patch) ;;
    *) usage ;;
esac

cd "$ROOT"

# --- 1. Preconditions -------------------------------------------------------
git diff --quiet && git diff --cached --quiet || {
    echo "error: git tree is not clean; commit or stash first." >&2
    exit 1
}
BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "$BRANCH" != "main" ]; then
    echo "error: version bumps run on 'main' only (current branch: $BRANCH)." >&2
    exit 1
fi

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

if git rev-parse -q --verify "refs/tags/$TAG" >/dev/null; then
    echo "error: tag $TAG already exists." >&2
    exit 1
fi

echo "bumping version: $CURRENT -> $NEW"

# --- 2. Bump the version ----------------------------------------------------
# Only the first `version = "..."` line matches: [workspace.package] precedes
# every other occurrence in the root manifest.
python3 - "$MANIFEST" "$CURRENT" "$NEW" <<'PY'
import re, sys
path, old, new = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(path, encoding="utf-8").read()
patched, count = re.subn(
    rf'^(version = ){re.escape(old)}$', rf'\g<1>"{new}"', text, count=1, flags=re.M
)
if count != 1:
    sys.exit(f"error: expected exactly one version bump, patched {count}")
open(path, "w", encoding="utf-8").write(patched)
PY

# --- 3. Refresh lockfile & changelog ----------------------------------------
cargo update --workspace --quiet

DATE="$(date +%Y-%m-%d)"
if grep -q "^## \[Unreleased\]" "$CHANGELOG"; then
    # changelog.py promotes the Unreleased content into a versioned section
    # (no-op with a warning when Unreleased is empty — the bump still stands).
    python3 "$ROOT/scripts/changelog.py" promote "$NEW" "$DATE" "$CHANGELOG"
else
    echo "warning: no '## [Unreleased]' section in $CHANGELOG; skipping changelog edit." >&2
fi

# --- 4. Commit ---------------------------------------------------------------
git add "$MANIFEST" "Cargo.lock" "$CHANGELOG"
git commit -m "chore(release): bump workspace version to $NEW

Single global semver bump of the workspace version (was $CURRENT).
All member crates inherit via version.workspace = true.
Regenerated Cargo.lock; cut the CHANGELOG.md release entry."

# --- 5. Tag ------------------------------------------------------------------
git tag -a "$TAG" -m "taskboard $NEW"

echo "done: $NEW committed and tagged as $TAG"
echo "remember: pushing commits/tags requires explicit user direction."
