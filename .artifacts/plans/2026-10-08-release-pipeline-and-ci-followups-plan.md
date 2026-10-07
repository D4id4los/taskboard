# [Plan] CI Follow-ups & Binary Release Pipeline

- **Date**: 2026-10-08
- **Target**: `.github/workflows/`, `scripts/`, `CHANGELOG.md`, `.artifacts/` —
  minimal source-code impact (packaging metadata only)
- **Branches**: one feature branch per phase (§3); nothing lands on `main`
  without the user's merge order (double-rebase policy, AGENTS.md §8)
- **Goal**: Close out the defects and housekeeping left open after the
  external-services setup, then implement the hand-rolled matrix release
  pipeline (decision: cargo-dist deferred to ≥ 1.0.0, see backlog) with a
  local Linux packaging script for low-latency testing of release issues.
- **Status**: DRAFT — awaiting user review before any branch is created.

---

## 1. Context & Findings (verified 2026-10-08)

State checked against `origin/main` (1690df2) and the live GitHub repo:

Done and healthy: remote pushed, CI green on `main` (6 jobs), secrets
`CODECOV_TOKEN` / `SONAR_TOKEN` / `BENCHER_API_TOKEN` set, `protect-main`
ruleset active (PR required, 0 approvals, rebase merge, force-push/deletion
blocked, 5 required checks), Miri + Mutation Testing smoke-tested, hooks
enabled, auto-delete-head-branches on.

Open defects found:

1. **SonarCloud fails on every Dependabot PR** (#1–#3). Dependabot PR runs do
   not receive repository secrets; the job log shows `SONAR_TOKEN` empty and
   the scanner exits `Not authorized` (exit 3). On `main` pushes it works.
2. **Deprecated action**: `ci.yml` uses `SonarSource/sonarcloud-github-action@v5`,
   which warns it is deprecated and delegates to
   `SonarSource/sonarqube-scan-action` (current major: **v8**, v8.3.0).
3. **`scripts/bump_version.sh` changelog rewrite is buggy** (verified by
   simulating its exact Python logic on the real CHANGELOG):
   - it splits at the next substring `"## "`, which first matches *inside*
     `### ` subsections → a stray `#` line is emitted and `### Fixed` is
     corrupted into an H2 `## Fixed`;
   - the promoted `## [X.Y.Z] - date` section is nested *under*
     `## [Unreleased]` instead of after it;
   - it crashes (`ValueError: substring not found`) when `## [Unreleased]` is
     empty and the only section — after `Cargo.toml`/`Cargo.lock` were already
     modified (half-done bump, dirty tree).
4. **Five open Dependabot PRs** (#1–#3 GitHub-Actions bumps; #4 sqlx 0.9.0,
   #5 console-subscriber 0.5.0). #4/#5 red only because they predate the
   `ci.yml` fixes on `main` — a `@dependabot rebase` refreshes them.
5. **Backlog drift**: Bencher and SonarCloud entries still listed as open
   ideas although both are implemented on `main`; the 2026-10-07 plan's
   status line still says "AWAITING USER TASKS".
6. **Local checkout** is 5 commits behind `origin/main`; four merged local
   branches can be deleted.

Release-body requirement (user): the GitHub Release text must come from the
version's `CHANGELOG.md` section. With defect 3 fixed, extraction is reliable
for both the CI publish job and local dry-runs.

## 2. Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Release tooling | Hand-rolled matrix workflow | Repo already owns release mechanics (bump script, changelog discipline); needs plain archives, not installers; zero new local tooling; every line reviewable as a normal `ci/` PR. cargo-dist's value (installers, auto-update) is a ≥ 1.0.0 topic — recorded in the backlog. |
| Packaging logic | One script shared by CI and local use (`scripts/package_release.py`, Python stdlib only) | Single source of truth: a local run is byte-identical to a CI leg, so the user can debug release issues locally with minimal round-trips. Runners and dev machines all have Python 3; repo already embeds Python in bash. |
| Changelog tooling | New `scripts/changelog.py` (`promote`, `extract`, `self-test`) used by `bump_version.sh` and the release workflow | The promotion bug and the extraction need the same parsing rules; a self-test subcommand guards both without adding a test framework. |
| SonarCloud vs. ruleset | Job skipped for `dependabot[bot]`; **not** added to required status checks | Rulesets cannot express "required except for Dependabot"; a skipped required check blocks Dependabot PRs forever. `main`-push analysis remains the authoritative quality record. |
| Sonar action | `SonarSource/sonarqube-scan-action@v8` | Drop-in replacement per the deprecation warning; env/`sonar-project.properties` unchanged; verify PR decoration on the first normal PR. |
| First end-to-end release test | The real first release (`v0.1.1`) | Tag-triggered runs execute the workflow file *in the tagged commit*, so the pipeline must be merged to `main` first; a pipeline dry-run (`workflow_dispatch`, artifacts but no release) validates packaging before that. No `-rc` tag grammar needed. |
| Targets (in scope) | `x86_64-unknown-linux-gnu` (ubuntu-latest), `x86_64-pc-windows-msvc` (windows-latest) | The two first-class platforms. macOS/aarch64 are second-priority backlog items (see §7) and would need a cross toolchain anyway. |
| Release visibility | `gh release create` publishes directly (non-draft) | Solo project; the tag itself is the deliberate act. Switching to draft-first is a one-flag change if wanted. |

**Security rule (unchanged from the 2026-10-07 plan):** no secret values are
handled. The release workflow uses the default `GITHUB_TOKEN` with
`permissions: contents: write` (job-scoped) — sufficient to create Releases
in the same repo; no new secrets required.

## 3. Phases & Branches

Independent: Phase 1, 2, 3. Phase 4 depends on 2 + 3. Phase 5 last.
Commit plan file with the first fix branch (same pattern as 2026-10-07).

### Phase 0 — Pre-flight (agent, no branch)

- [ ] `git pull --ff-only` (local `main` is 5 behind `origin/main`).
- [ ] Delete merged local branches (`fix/ci-action-refs-licenses`,
      `fix/ci-coverage-bench-invocations`, `ci/sonarcloud-integration`,
      `ci/bencher-integration`) and `git remote prune origin`.

### Phase 1 — SonarCloud repairs (`fix/ci-sonarcloud-dependabot-deprecation`)

- [ ] `ci.yml` `sonarcloud` job: add `if: github.actor != 'dependabot[bot]'`
      (job level, with a comment explaining that Dependabot PR runs get no
      secrets, so the scan can only fail there).
- [ ] Replace `SonarSource/sonarcloud-github-action@v5` →
      `SonarSource/sonarqube-scan-action@v8` (keep `fetch-depth: 0`,
      `SONAR_TOKEN` env, `sonar-project.properties` as-is).
- [ ] Open PR; verify on the PR run: job executes for a human actor, no
      deprecation warning in the log, SonarCloud shows the PR analysis.
- [ ] After merge: comment `@dependabot rebase` on PRs #1–#5; confirm all
      five go green (their `SonarCloud Analysis` check disappears by design;
      #4/#5 pick up the fixed coverage/benchmark jobs).
- [ ] Do **not** touch the ruleset's required checks (document in PR body why
      SonarCloud stays non-gating).

### Phase 2 — Changelog tooling & bump-script fix (`fix/bump-version-changelog`)

- [ ] New `scripts/changelog.py` (stdlib only, SPDX header not required for
      scripts — but keep the file header comment style consistent):
      - `promote <new-version> <date>`: replace the *content* of the
        `## [Unreleased]` block with a versioned `## [X.Y.Z] - <date>`
        section inserted **after** the (now empty) Unreleased heading.
        Match full-line `^## ` headings only; no-op with a warning when the
        Unreleased block is empty (never crash, never leave a half-done
        state — the caller decides whether an empty release is fatal).
      - `extract <version>`: print the section `## [X.Y.Z]` up to the next
        full-line `^## [` heading (release-body source for CI and local
        dry-runs).
      - `self-test`: golden in/out cases covering exactly the three defects
        (subsection corruption, nesting, empty-Unreleased) plus extraction
        round-trip; non-zero exit on any mismatch.
- [ ] Rewire `bump_version.sh` step 3 to call `python3 scripts/changelog.py
      promote "$NEW" "$DATE"` (delete the inline Python block).
- [ ] `ci.yml` `check` job: add a step `python3 scripts/changelog.py self-test`.
- [ ] Verify locally by simulating `promote` on scratch copies of the real
      CHANGELOG (both current state and post-promotion state).

### Phase 3 — Packaging script (`feat/release-packaging`)

- [ ] New `scripts/package_release.py` (stdlib only):
      - reads the version from `[workspace.package]` in the root `Cargo.toml`;
      - `cargo build --locked --release -p taskboard-app`
        (`--target <triple>` when given, else host);
      - stages into
        `target/dist/taskboard-<version>-<triple>/`: the `taskboard` binary
        (+ `.exe` on Windows), `LICENSE-MIT`, `LICENSE-APACHE`, `README.org`,
        `CHANGELOG.md`;
      - archives: `.tar.gz` by default, `--format zip` alternative;
      - writes `taskboard-<version>-<triple>.<ext>.sha256` next to the
        archive;
      - `--check` mode: rebuild nothing, verify an existing archive's
        contents + checksum (used by CI acceptance and locally).
- [ ] Optional (recommend, same branch): `[profile.release]` with
      `strip = "symbols"` and `lto = "thin"` in the root manifest — kiosk
      footprint goal; does this *before* the first release so artifact
      characteristics stay stable.
- [ ] Verify locally: run the script, `tar -tzf` the archive,
      `sha256sum -c`, run the binary (`--help` or its startup error is fine —
      assert the artifact, not log text).

### Phase 4 — Release workflow (`ci/release-workflow`)

- [ ] New `.github/workflows/release.yml`:
      - trigger: `push: tags: ["v*"]` **and** `workflow_dispatch` with a
        `dry_run` input (default true) that builds + packages + uploads
        artifacts but never creates a Release;
      - job `validate`: tag matches `^v[0-9]+\.[0-9]+\.[0-9]+$`; stripped
        version equals the root `Cargo.toml` version (fail fast otherwise);
        `scripts/changelog.py extract <version>` succeeds (release notes
        must exist);
      - job `build` (matrix, `needs: validate`): the two targets from §2;
        each leg checks out, `dtolnay/rust-toolchain@stable`,
        `Swatinem/rust-cache@v2`, runs
        `python3 scripts/package_release.py --target <triple>` (`--format
        zip` on Windows), uploads the archive + checksum via
        `actions/upload-artifact@v4` with per-leg names
        (`dist-<triple>`), `retention-days: 7`, `timeout-minutes: 45`;
      - job `publish` (`needs: build`,
        `permissions: contents: write`): `actions/download-artifact@v4`
        (majors of upload/download must match — Dependabot bumps them in
        tandem), reassemble `SHA256SUMS` over the final artifact set,
        `python3 scripts/changelog.py extract <version> > release_notes.md`,
        `gh release create "$TAG" --title "taskboard <version>"
        --notes-file release_notes.md <archives> SHA256SUMS`
        (skipped entirely when `dry_run`).
- [ ] `README.org` *Download* section: replace the "once the release pipeline
      lands" sentence with the Releases pointer + artifact naming + the local
      packaging script one-liner; mention the local script in *Dev Quickstart*.
- [ ] Verify: local script + `changelog.py self-test` green; on the PR only
      the workflow *syntax* is checkable (GitHub validates YAML on push);
      full validation happens post-merge via the `workflow_dispatch` dry-run
      (green run, artifacts uploaded, no Release created).
- [ ] First real release (user's order, after merge): run
      `./scripts/bump_version.sh patch` on `main` (→ v0.1.1, promotes the
      current Unreleased entries), user pushes `main` + tag, watch the run,
      confirm the Release text matches the CHANGELOG section byte-for-byte.

### Phase 5 — Housekeeping (`chore/backlog-housekeeping`, or folded into each phase's PR)

- [ ] Backlog (`taskboard-potential-improvements-backlog.md`):
      - mark the Bencher and SonarCloud entries **done** (implemented
        2026-10-07/08, PR refs);
      - mark the release-pipeline entry **done** (this plan) once Phase 4
        merges;
      - **add new entry** "cargo-dist & installers (≥ 1.0.0)": adopt
        cargo-dist (v0.33+, active) for shell/PowerShell installers,
        attestations and auto-update when the project approaches 1.0.0 and
        end-user distribution matters; consumes the existing tags, so
        adoption stays cheap;
      - add second-priority target entry: macOS aarch64 + Linux/Windows
        aarch64 matrix legs (needs cross toolchain; revisit when the Slint
        UI lands).
- [ ] Update the 2026-10-07 plan's status line: tasks complete (except
      optional §3.7 Snyk — stays skipped/backlog).
- [ ] `CHANGELOG.md [Unreleased]`: Fixed (SonarCloud Dependabot skip, action
      deprecation, bump-script changelog corruption), Added (release
      pipeline, packaging script, changelog tooling).
- [ ] Final report: PR/branch map, CI results, release/acceptance status.

## 4. Not Touched / Out-of-Scope

- cargo-dist, installers, auto-update, Homebrew/npm distribution — backlog
  entry, ≥ 1.0.0 (§3 Phase 5).
- macOS / aarch64 release targets — backlog entry (cross toolchain + Slint
  system-lib questions first).
- Code signing (macOS notarization, Windows Authenticode) — needs
  certificates; revisit with the ≥ 1.0.0 distribution work.
- crates.io publishing (`publish = false` stays), Snyk (skipped, backlog),
  MSRV CI job + `slint` dependency (separate backlog entries).
- Application runtime configuration (Nextcloud, `taskboard.toml`).

## 5. Verification & Acceptance

Done means, verifiably:

1. PRs #1–#5 rebased and fully green; Dependabot PRs show no
   `SonarCloud Analysis` check, normal PRs and `main` pushes do — and the
   SonarCloud log contains no deprecation warning.
2. `python3 scripts/changelog.py self-test` passes in CI (`check` job) and
   locally; `bump_version.sh` promotion simulated on scratch copies produces
   correct Keep-a-Changelog layout for: normal content, empty Unreleased,
   single-section file (no crash, no stray `#`, no nesting).
3. `python3 scripts/package_release.py` locally yields
   `target/dist/taskboard-<version>-x86_64-unknown-linux-gnu.tar.gz`
   containing the binary + LICENSE-MIT + LICENSE-APACHE + README.org +
   CHANGELOG.md, with a matching `.sha256`; `--check` validates it.
4. Post-merge `workflow_dispatch` dry-run of `release.yml` is green, uploads
   both legs' artifacts, and creates **no** Release.
5. First tagged release `v0.1.1` (user's order): Release body equals the
   CHANGELOG `## [0.1.1]` section; archives + `SHA256SUMS` attached;
   `sha256sum -c` passes.
6. Backlog and old-plan statuses updated; `[Unreleased]` entries present.
