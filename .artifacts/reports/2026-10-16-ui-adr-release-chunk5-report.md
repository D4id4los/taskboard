# [Report] Chunk 5 — UI ADR, Docs Amendments & v0.1.0 Release (stage 2)

- **Date**: 2026-10-16
- **Branch**: `docs/ui-adr-release` (worktree
  `../taskboard-WT-docs-ui-adr-release`), based on `origin/main`
  @ `2e9d824`. No push, no PR (stage 3 ships), no tag, no
  `bump_version.sh` execution.
- **Plan**: `.artifacts/plans/2026-10-16-ui-adr-release-plan.md`
  (executed as specified; no deviations).

## Commits (each standalone, pre-commit fmt/clippy/token-lint green)

1. `8a491b2` `docs(adr): formalise adr 0015 slint ui architecture` —
   `docs/adr/0015-slint-ui-architecture.org` (new, 100 lines). ADR
   number re-verified next-free before writing (`docs/adr/` holds
   0001–0012 + 0014). The §13.2 draft carried as Accepted with the
   five recorded amendments A1–A5 (decision 6 restated against the
   landed manifest — default renderer features + additive
   `unstable-fontique-011`; the 0013 numbering-gap context sentence;
   the kiosk software-renderer consequence; the Q11 licensing
   paragraph; the explicit per-bump pin-upgrade runbook).
2. `e66a428` `docs(architecture): the landed slint ui crate and desktop
   entry` — six-section amendment set per plan §3; the Versioning
   section kept verbatim (recorded non-amendment).
3. `25f1ea7` `docs(testing): the ui test tiers and slint pin
   discipline` — §7 rewritten and retitled "UI Testing: Tiers & Pins
   (ADR 0015)"; the fictional `slint::testing` reference removed; the
   four landed tiers + house rules + pin discipline.
4. `6a61ad7` `docs(backlog): slint dependency and ui forwarder done,
   adr 0013 and bump script follow-ups` — see below.
5. `9e75c20` `docs(changelog): the v0.1.0 gui release notes` — five
   Added bullets in `[Unreleased]`; `changelog.py self-test` green
   before the commit.

## Backlog edits (`.artifacts/taskboard-potential-improvements-backlog.md`)

- "[2026-10-07] Add the `slint` dependency to taskboard-ui-slint" →
  ✅ DONE (2026-10-16) with a one-line note; the kiosk-renderer
  *Update 2026-10-16* paragraph retained (complementary).
- "[2026-10-09] UI command forwarder bootstrap task" → ✅ DONE
  (2026-10-16): `spawn_ui_command_forwarder`,
  `crates/taskboard-app/src/ui.rs` (`CommandSink` over
  `EngineHandle::dispatch`, hermetic fake-sink test).
- New entry "[2026-10-14] Formalise ADR 0013 (reminders)…" inserted
  chronologically after the last [2026-10-14] entry (Push-abort
  `Retry-After` floor), before the [2026-10-15] block.
- New entry "[2026-10-16] `bump_version.sh`: adapt the release cut to
  branch-protected `main`" appended at end of file.
- Everything else untouched, including "[2026-10-14] LLM/MCP
  UI-debugging driver tooling" and all [2026-10-15]/[2026-10-16]
  entries.

## Release-readiness evidence (plan §6.1 checklist)

| # | Check | Evidence (verified 2026-10-16 on this branch) |
|---|---|---|
| 1 | Release binaries carry the UI | `scripts/package_release.py` line 56: `cargo build --locked --release -p taskboard-app`; `crates/taskboard-app/Cargo.toml` `[dependencies]` includes `taskboard-ui-slint.workspace = true` → the binaries carry the UI by construction. Invocation: `release.yml` "Package Release" step. |
| 2 | GUI links on the Linux release leg | `release.yml` build job, "Install system deps" step (Linux): installs `libgl1-mesa-dev`, `libegl1-mesa-dev`, `libxkbcommon-dev`, `libwayland-dev`, `libfontconfig1-dev` (plus `libdbus-1-dev`/`pkg-config` for keyring). |
| 3 | MCP feature off in the release profile | Two assertions, both `! cargo tree -e features -p taskboard-app \| grep -q 'feature "mcp"'`: `ci.yml` ~line 66 ("default resolution") and `release.yml` ~line 103 (build job, runs per matrix target). The `mcp` feature is non-default and dev-gated (`crates/taskboard-ui-slint/Cargo.toml`: `mcp = ["slint/mcp"]`). |
| 4 | Tag/version match | `Cargo.toml` line 13: `[workspace.package] version = "0.1.0"`; `release.yml` validate job "Tag Version Matches Workspace Version" asserts tag == workspace version; tag format regex `^v[0-9]+\.[0-9]+\.[0-9]+$`. Zero git tags exist locally (`git tag` empty). |
| 5 | Release notes exist at the tag | This chunk added the GUI bullets to `[Unreleased]`; the user's promote creates `[0.1.0]`. Dry-check today: `changelog.py extract 0.1.0 CHANGELOG.md` exits 1 ("no '## [0.1.0]' section found") — expected pre-promotion; the validate job turns exactly this into a release-blocking error. `changelog.py self-test` green (promote/extract round-trip). |
| 6 | Workflow-on-main precondition | `release.yml` header comment: "The workflow executes in the tagged commit, so it must be merged to main before the first release tag." The workflow is already on `main`; trivially satisfied — the tag must be cut on `main` *after* the release PR merges (rebase-merge rewrites SHAs). |
| 7 | Optional pre-tag validation | `workflow_dispatch` dry-run exists (`release.yml` lines 11–16: builds + packages both targets, uploads artifacts, never creates a Release; notes check relaxed under dry-run). |

No pipeline gap found — checklist green; nothing to escalate.

## Gates run

`cargo fmt --all -- --check` clean; `cargo clippy --workspace
--all-targets -- -D warnings` clean; `cargo nextest run --workspace`:
959 passed, 28 skipped (the live-tier/sandbox skips), 0 failed;
`cargo test --doc --workspace` ok; `cargo deny check` all four
categories ok; `check_slint_tokens.py --self-test` + lint clean
(pre-commit hook on every commit).

## The post-merge release command sequence (user-executed, verbatim)

```sh
# 0) after the docs/ui-adr-release PR merges — cut the release branch:
git checkout main
git pull --ff-only origin main
git status --short                  # must be empty before branching
git switch -c chore/release-v0.1.0

# 1) promote the changelog (substitute the actual release date):
python3 scripts/changelog.py promote 0.1.0 2026-10-XX CHANGELOG.md

# 2) commit the release cut (mirrors bump_version.sh's commit shape):
git add CHANGELOG.md
git commit -m "chore(release): cut the v0.1.0 changelog section

Promote [Unreleased] into the [0.1.0] release entry. The single global
semver is already 0.1.0 (initial manifest); the release rides this
branch + PR because main is branch-protected (bump_version.sh
adaptation tracked in the backlog)."

# 3) push the branch and open the release PR (required checks must pass):
git push -u origin chore/release-v0.1.0
gh pr create --base main --title "chore(release): cut the v0.1.0 changelog section" \
  --body "Release cut for v0.1.0 (roadmap Phase 7 / M2 exit). After merge: fetch main, tag v0.1.0 on the merged commit, push the tag — the Release workflow takes over."

# 4) after CI is green and the PR is merged on GitHub (rebase-merge
#    rewrites SHAs — the tag MUST be cut on main, never on the branch):
git checkout main
git pull --ff-only origin main
git tag -a v0.1.0 -m "taskboard 0.1.0"
git push origin v0.1.0             # the tag push triggers the Release workflow
```

Optional pre-branch dry run: GitHub → Actions → Release → Run workflow
(dry-run). Post-tag verification: `gh run watch`, then
`gh release view v0.1.0`. No version bump is needed — the workspace
version has been 0.1.0 since the initial manifest, and
`bump_version.sh` only increments (it would cut v0.1.1/v0.2.0 and
fails under branch protection anyway; adaptation tracked in the
backlog).
