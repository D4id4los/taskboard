# taskboard Potential Improvements Backlog

A living document of improvement ideas that were noticed but are not (yet)
part of an active implementation plan. Ideas are archived/removed once
implemented. Append entries using the template below.

## Entry Template

<!--
## [YYYY-MM-DD] Short Descriptive Title

- **Category**: `Architecture` | `Refactoring` | `Testing` | `Performance` | `UI` | `DX`
- **Originating Plan/Report**: `.artifacts/plans/YYYY-MM-DD-topic-plan.md` (or Chat/Context)
- **Target Area**: Crate or module affected (e.g., `crates/taskboard-state/`)

### Context & Description
Why this matters and why it was out-of-scope at the time of discovery.

### Proposed Approach
Optional high-level notes for a future iteration.
-->

## [2026-10-07] Add the `slint` dependency to taskboard-ui-slint

- **Category**: `UI` / `DX`
- **Originating Plan/Report**: Repository bootstrap (ADR 0003)
- **Target Area**: `crates/taskboard-ui-slint/`

### Context & Description
The UI crate ships dependency-free so the GUI/OpenGL system-library stack
(libxkbcommon and friends) stays out of headless builds and CI until UI
work starts.

### Proposed Approach
When UI work begins, add `slint` to `[workspace.dependencies]` (decide
backend features: renderer-skia vs renderer-femtovg for low-power kiosk
hardware), wire the crate into `taskboard-app`, and evaluate CI system
package requirements on ubuntu-latest.

## [2026-10-07] Release pipeline for binary distributions

- **Category**: `DX`
- **Originating Plan/Report**: Repository bootstrap
- **Target Area**: `.github/workflows/`

### Context & Description
The README promises GitHub Releases binaries (Linux & Windows first-class;
macOS/ARM second priority), but no release workflow exists yet.

### Proposed Approach
Evaluate `cargo-dist` for multi-target binary release automation vs a
hand-rolled matrix workflow; trigger it from `v*` tags created by
`scripts/bump_version.sh`.

## [2026-10-07] MSRV policy and CI job

- **Category**: `DX` / `CI`
- **Originating Plan/Report**: Repository bootstrap (ADR 0003)
- **Target Area**: `Cargo.toml`, `.github/workflows/`

### Context & Description
`rust-version = "1.96"` is declared per crate, but nothing verifies it —
accidental use of newer stdlib/stabilizations would only fail on older
user toolchains.

### Proposed Approach
Add a `check (msrv)` CI job using `dtolnay/rust-toolchain@1.96` +
`cargo check --workspace --all-features`, and define whether the MSRV may
float (e.g., track stable minus N).

## [2026-10-07] Continuous benchmark tracking via Bencher

- **Category**: `Performance`
- **Originating Plan/Report**: Repository bootstrap
- **Target Area**: `.github/workflows/ci.yml`

### Context & Description
The `benchmark` job runs criterion and discards output; historical trend
tracking (regression detection across commits) needs an external tracker.
Also, no `[[bench]]` targets exist yet.

### Proposed Approach
Once criterion benches exist for hot paths (SQLite caching actor, task
sorting), add the `bencherdev/bencher-action` step and a
`BENCHER_API_TOKEN` secret.

## [2026-10-07] SonarQube Cloud / Snyk integration

- **Category**: `Testing`
- **Originating Plan/Report**: Repository bootstrap
- **Target Area**: `.github/workflows/`

### Context & Description
Optional third-party quality/security services (free for public repos) that
add long-term code-quality tracking (SonarCloud) and automated vulnerable-
dependency PRs (Snyk) on top of clippy + cargo-deny.

### Proposed Approach
Defer until the repository is public and the team decides the added PR
noise is worth it; then add `SonarSource/sonarcloud-github-action` and/or
connect Snyk via the GitHub app.
