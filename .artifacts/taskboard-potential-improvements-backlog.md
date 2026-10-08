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
- **Status**: ✅ DONE (implemented 2026-10-08 as a hand-rolled matrix
  workflow — decision: cargo-dist deferred to ≥ 1.0.0, see the
  [2026-10-08] entry below — via `.artifacts/plans/2026-10-08-release-pipeline-and-ci-followups-plan.md`,
  PRs #10–#12; dry-run + first release validation pending post-merge)

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
- **Status**: ✅ DONE (implemented 2026-10-07/08: `benchmark` job tracks
  criterion results from `main` pushes via the Bencher CLI with the
  `BENCHER_API_TOKEN` secret; tolerant until first `[[bench]]` targets
  exist — PR #7)

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
- **Status**: ✅ PARTIALLY DONE — SonarCloud implemented 2026-10-07/08
  (`sonar-project.properties` + CI job, `SONAR_TOKEN` secret; PRs #6/#9;
  skipped for Dependabot actors, non-gating by design). Snyk stays
  skipped/backlog.

### Context & Description
Optional third-party quality/security services (free for public repos) that
add long-term code-quality tracking (SonarCloud) and automated vulnerable-
dependency PRs (Snyk) on top of clippy + cargo-deny.

### Proposed Approach
Defer until the repository is public and the team decides the added PR
noise is worth it; then add `SonarSource/sonarcloud-github-action` and/or
connect Snyk via the GitHub app.

## [2026-10-08] cargo-dist & installers (≥ 1.0.0)

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-release-pipeline-and-ci-followups-plan.md`
- **Target Area**: `.github/workflows/`

### Context & Description
The hand-rolled matrix release workflow (2026-10-08) covers plain archives
for the first-class targets. Installer UX (shell/PowerShell installers),
artifact attestations, and auto-update channels are a ≥ 1.0.0 topic, when
end-user distribution actually matters.

### Proposed Approach
Adopt cargo-dist (v0.33+, actively maintained) when approaching 1.0.0. It
consumes the existing `v*` tags, so adoption stays cheap: the hand-rolled
workflow can be retired or kept as a fallback.

## [2026-10-08] Second-priority release targets: macOS aarch64 + Linux/Windows aarch64

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-release-pipeline-and-ci-followups-plan.md`
- **Target Area**: `.github/workflows/release.yml`

### Context & Description
The release matrix currently covers only the two first-class targets
(x86_64 Linux & Windows). macOS and aarch64 are second priority per
AGENTS.md §1 and need a cross toolchain plus Slint system-lib answers
(revisit when the Slint UI lands).

### Proposed Approach
Add matrix legs for macOS aarch64 (native runner) and Linux/Windows
aarch64 (cross toolchain) to `release.yml`; verify the packaging script's
`--target` path on each before the first tagged release that includes them.

## [2026-10-08] Nextcloud major-version matrix for the docker test tier

- **Category**: `Testing`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md`
- **Target Area**: `docker/nextcloud-test/compose.yaml`, `.github/workflows/ci.yml`

### Context & Description
Tier 2 (dockerized Nextcloud) pins a single stable major, so protocol drift
between supported Nextcloud/Deck versions goes unnoticed until it hits the
user's real deployment (Tier 3, manual-only).

### Proposed Approach
Once the client surface grows beyond the board slice, add a matrix leg for
the oldest supported Nextcloud major (a separate compose file or an
overridable image tag) to the `Nextcloud Integration (docker)` CI job.

## [2026-10-08] Scheduled live-tier runs via a self-hosted LAN runner

- **Category**: `Testing`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md`
- **Target Area**: `.github/workflows/`, GitHub runner setup

### Context & Description
Tier 3 (private production server) is deliberately manual/local-only so no
Nextcloud secret — not even the server address — exists on GitHub, whose CI
logs are public. The trade-off: nothing verifies the real deployment
periodically.

### Proposed Approach
If scheduled live checks become wanted, register a self-hosted GitHub
runner on the LAN (its logs are not publicly readable), move the Tier 3
suite + `.env` secrets there, and run it on a schedule like `miri.yml`.
Keep the zero-secrets-on-GitHub property intact for all hosted jobs.

## [2026-10-08] Dedicated `BoardColor` type in the Deck client

- **Category**: `Refactoring`
- **Originating Plan/Report**: Chat review of `.artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md` outcomes
- **Target Area**: `crates/taskboard-sync-nextcloud/`
- **Status**: ✅ DONE — generalized to `DeckColor` (covers labels too) in
  `crates/taskboard-sync-nextcloud/src/color.rs`
  (`.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md` §5,
  PR A)

### Context & Description
Deck API colors (boards now; labels, cards, and stacks later) are passed
through as bare strings everywhere in the crate. Modelling them as a
dedicated validated type, converted to string only at the request
boundary, was deferred because the current plan scopes the full Deck
surface out (§1) and the change would reopen the public
`create_board` signature across three reviewed branches.

### Proposed Approach
Design the type once for the whole Deck surface in the upcoming
client-surface plan: hex-string newtype (`#[serde(rename_all =
"camelCase")]` wire format), validation rules (six hex digits, no `#`),
proptest for the validation, and decide whether it belongs
crate-internal or in `taskboard-domain` alongside the other Deck
payload types.

## [2026-10-08] Deck sync actor, conflict resolution, and DTO→domain mapping

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md` §9
- **Target Area**: `crates/taskboard-sync-nextcloud/`, `crates/taskboard-domain/`, `crates/taskboard-app/`

### Context & Description
The client-surface plan deliberately delivers only the Deck API access
point. The actor that drives it — poll scheduling on top of the
conditional reads, offline write queue for kiosk mode, conflict
resolution, channel wiring to the state engine, and the canonical
domain entities (`Task`, board ports) plus the pure DTO→domain mapping
functions — remains unbuilt.

### Proposed Approach
Its own plan once the client surface has landed: define domain entities
and a Deck backend port in `taskboard-domain`, implement pure mapping
functions in the sync crate (proptest-able), then build the actor over
`mpsc`/`broadcast`/`oneshot` per `docs/architecture.org`.

## [2026-10-08] Deck attachment writes and content download

- **Category**: `Refactoring`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md` §9
- **Target Area**: `crates/taskboard-sync-nextcloud/`

### Context & Description
The client surface covers attachment *metadata* only (list + embedded
`extendedData`). Uploading (`multipart/form-data`), updating, restoring,
deleting, and downloading file content would require the `multipart`
reqwest feature and a non-JSON response path, none of which a taskboard
kiosk needs today.

### Proposed Approach
Add the `multipart` feature to the workspace `reqwest` pin, model the
upload body, and route content download as raw bytes rather than
`send_json`. Trigger: an actual product feature that consumes
attachments.

## [2026-10-08] Deck comments and administrative endpoints

- **Category**: `Refactoring`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md` §9
- **Target Area**: `crates/taskboard-sync-nextcloud/`

### Context & Description
Deck API groups intentionally left unbuilt because a single-account
taskboard client has no use for them: card comments (OCS endpoints, the
API's only pagination), ACL/participant management, card user
assignment/unassignment, board import/export, collaborative sessions,
and Deck config endpoints.

### Proposed Approach
Model and test each group the same tiered way when a consumer appears;
comments first if task discussions ever matter to the UI.
