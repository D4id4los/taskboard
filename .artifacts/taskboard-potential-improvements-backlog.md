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

## [2026-10-08] OAuth2 device/PKCE login flow

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` §2/§4
- **Target Area**: `crates/taskboard-app/`, `crates/taskboard-sync-nextcloud/`

### Context & Description
The roadmap decision (2026-10-08) is app password + OS keyring: the user
mints an app password in the Nextcloud web UI and pastes it at first run.
OAuth2 (device grant or PKCE) would remove that manual step and improve
revocation UX, but adds redirect handling, refresh-token scheduling, and
per-server OAuth app registration — significant work on top of an auth
model that is already decided, implemented, and tier-tested.

### Proposed Approach
If first-run login friction becomes a real complaint, add an OAuth2 flow
alongside the app-password path (keep both): browser-based authorization
with a loopback redirect, tokens in the same keyring entry, and refresh
scheduling in the sync actor. Registering a default OAuth client per
Nextcloud instance is an open problem to solve first.

## [2026-10-08] Daemon IPC for remote control (kiosk management)

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` §2/§4
- **Target Area**: `crates/taskboard-app/`

### Context & Description
The roadmap decision (2026-10-08) is a single `taskboard` binary whose
subcommands boot the core in-process. A long-running `daemon` owning the
core plus thin CLI subcommands over local IPC (e.g. unix domain socket)
would enable remote management of a kiosk box and avoid repeated core
bootstrapping per command. Deferred because the in-process flavor is the
simplest correct E2E story and IPC adds a protocol layer before anything
is usable.

### Proposed Approach
Design a small typed IPC protocol (versioned, length-framed) once a
kiosk deployment actually needs remote control; the CLI surface
(`login`, `tasks ...`, `sync`) stays unchanged, only the transport of
commands differs.

## [2026-10-08] Reminders/notifications subsystem

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` §4
- **Target Area**: `taskboard-domain/`, `taskboard-state/`, `taskboard-app/`

### Context & Description
Taskboard's identity is "task management **and reminder** system"
(AGENTS.md §1), but no reminder design exists yet: no due-date watching,
notification scheduling, kiosk banner triggering, or desktop-notification
integration appears in architecture.org or any plan. The roadmap focuses
on sync + persistence first; reminders need their own planning round and
should start early after M1/M2 while the state engine contracts are
still cheap to extend.

### Proposed Approach
Own plan covering: reminder rules as pure domain functions (proptest
scheduling), a scheduler actor on the injected clock (no sleeps),
notification output as a Port, and the kiosk overdue-banner path.

## [2026-10-08] Multi-board support

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` §3 (Phase 6)
- **Target Area**: `crates/taskboard-app/`, `crates/taskboard-state/`

### Context & Description
The MVP binds exactly one Deck board at first run (`boards select`).
Multiple boards complicate identity (remote ids are only unique per
board for stacks/cards), the outbox operation types, and every UI view.
Deferred until the single-board deployment proves the core.

### Proposed Approach
Lift to N boards by keying stacks/cards by (board_id, id) in storage and
adding a board selector to the UI; revisit before designing any feature
that bakes "one board" into the AppState shape.

## [2026-10-08] Keyring fallback for headless Linux (no Secret Service)

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md` §5
- **Target Area**: `crates/taskboard-app/`

### Context & Description
The `keyring` crate on Linux talks to the Secret Service (gnome-keyring/
KWallet), which is absent on headless servers, SSH sessions, and minimal
kiosk images. The roadmap stores the Nextcloud app password in the OS
keyring; deployments without a secret service need a defined behavior —
a clear typed error at minimum, likely a 0600 file fallback.

### Proposed Approach
In the Phase 5 (app bootstrap) plan: wrap keyring access behind a small
Port with two impls (keyring, encrypted-file), select via config, and
document the trade-off. Never silently fall back to plaintext.

## [2026-10-08] Deck client coverage pass: endpoint logging arms

- **Category**: `Testing`
- **Originating Plan/Report**: Codecov patch-coverage flag on the
  deck-client-surface PRs (`.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md`)
- **Target Area**: `crates/taskboard-sync-nextcloud/src/client.rs`

### Context & Description
Every `DeckClient` endpoint method ends in a
`match &result { Ok => tracing::info!, Err => tracing::warn! }` pair;
these arms (roughly 30 lines) are the bulk of the crate's remaining
uncovered code. Deliberately not covered in the surface PRs: asserting
them would require asserting log text, which the testing rules forbid,
and per-arm copy-paste tests would be metric-chasing, not behavior
testing. The model-layer wire-drift behavior (CardLabel, Participant)
was covered in the surface PR itself.

### Proposed Approach
One parameterized test that iterates the endpoint methods (success and
error case each) and asserts only structural facts — no panic, call
observed on the mock — driving every logging arm without text
assertions. Or accept the gap explicitly with a lint/coverage ignore
annotated at the logging match, if the team decides logs are not worth
harnessing. Trigger: next time coverage is formally gated, or when the
client surface next changes anyway.

## [2026-10-09] Field-level merge refinement to avoid granularity clobbering

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` (§7.4 example 2, "granularity trap") + user request
- **Target Area**: `crates/taskboard-domain/src/merge.rs` (policy layer; may need a persisted edit-history structure in Phase 2 storage)

### Context & Description
Deck's `lastModified` is entity-level, so the Phase 1 policy lets a
remote entity-touch that is merely *newer* than a local field edit
overwrite that field with its (unchanged) remote value — even when the
remote change actually touched a different field. This is the classic
per-object sync flaw where one field's update silently clobbers
unrelated fields the remote never modified. The current design accepts
and documents this loss; the user explicitly wants mitigation explored:
"always hate it when a per-object sync clobbers individual fields
because of bad sync implementations". Any decision under incomplete
information is probabilistic, not provably correct — the goal is to
make wrong guesses rare and bounded, not to eliminate them.

### Proposed Approach
Ideas to evaluate (design spike first, then a plan):

1. **Local edit history.** Persist per-field edit records
   `(field, previous_value, new_value, edited_at)` locally (Phase 2
   storage, bounded/compacted). On merge, when
   `ts_r > clocks[field]`, diff the remote snapshot against the
   previously observed remote state (`remote_seen` baseline) to infer
   *which fields the remote edit actually touched*, and apply remote
   values only to those fields — instead of blanket entity-level LWW.
2. **Field-change inference heuristics.** When the inference is
   ambiguous (e.g. remote value equals our previous local value, or no
   prior baseline exists), decide between "remote deliberately
   reverted/rewrote this field" vs "sync granularity artifact" using
   signals like value equality with history, whether the remote entity
   has any *other* changed field, and recency distance. Uncertain cases
   fall back to today's strict-LWW behavior.
3. **Conflict surfacing instead of silent choice.** For fields the
   heuristic scores as genuinely contested, consider surfacing a
   user-visible conflict (kept copy + current copy) rather than
   auto-picking — possibly only for high-stakes fields like
   title/description.
4. **Upstream check (cheap first step).** Re-verify against current
   Nextcloud Deck sources whether any write-side version vector,
   `If-Match`, or per-field `lastModified` exists on newer Deck
   versions (the client-surface plan found none documented); if one
   appears, prefer it over all heuristics.

Trigger: revisit after M1 real-server use if field-clobbering is
observed or complained about; the Phase 1 policy primitives (per-field
clocks, `remote_seen`) are already the substrate this builds on.

## [2026-10-09] Add-wins (OR-set) label merge

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` §12
- **Target Area**: `crates/taskboard-domain/src/merge.rs`

### Context & Description
Labels currently merge as one field (whole-set LWW, phase-1 decision 4):
two devices adding different labels concurrently lose one side's addition.

### Proposed Approach
Per-label timestamps; removes win only when newer than the opposing add.
Replaces the whole-set comparison in `merge_task`'s label branch and adds
a `LabelClocks`-like per-membership clock.

## [2026-10-09] Resurrect-as-new-card policy for remote card deletes

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` §12
- **Target Area**: `crates/taskboard-domain/src/merge.rs` (R3)

### Context & Description
Observed remote absence of a bound card always wins over concurrent local
edits (delete-wins fallback): the local edit is lost when someone deletes
the card remotely while it is being edited offline.

### Proposed Approach
Keep the local content and re-push it as a new card (fresh remote id)
instead of tombstoning; needs a duplicate-title UX policy and an outbox
op for "recreate".

## [2026-10-09] Incremental `RemoteIndex` maintenance

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` §12
- **Target Area**: `crates/taskboard-domain/src/merge.rs`

### Context & Description
The remote-ref index is rebuilt O(n) per sync from scratch. Fine at MVP
scale; irrelevant until boards grow large or syncs become frequent.

### Proposed Approach
Maintain the index incrementally inside the state engine when entities
change, or memoize per `AppState` version counter.

## [2026-10-09] Local board-edit commands and `Board` clocks

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` §12
- **Target Area**: `crates/taskboard-domain/src/entities.rs`, `messages.rs`

### Context & Description
`Board` has no per-field clocks and there are no local board-edit
commands (MVP binds an existing remote board); board content arrives only
via remote adoption.

### Proposed Approach
Add `BoardClocks { title, color, archived, deleted }` plus
`RenameBoard`/`SetBoardColor` commands when multi-board or local board
editing lands.

## [2026-10-09] Clock-skew mitigation for sync comparisons

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md` §12
- **Target Area**: `crates/taskboard-domain/src/merge.rs`, ADR 0004

### Context & Description
All LWW comparisons are cross-machine wall clock; within-skew
misordering can revert a just-made local edit to a slightly "newer"
remote value (documented in ADR 0004).

### Proposed Approach
Configurable remote-age bias (subtract N seconds from remote timestamps
before comparison) if real-server use (M1) shows revert-on-sync symptoms.

## [2026-10-09] Logging statements for taskboard-domain

- **Category**: `DX`
- **Originating Plan/Report**: Chat/Context 2026-10-09 (user request during
  the Phase 2 plan review; forgotten in the initial Phase 1 commits)
- **Target Area**: `crates/taskboard-domain/`

### Context & Description
The domain crate shipped without `tracing` statements, so a debug log
attached to a bug report cannot show which merge primitive or pipeline step
produced a surprising state. Coding guidelines §4 expects "sufficient
logging statements ... that a user's default log output can localise a bug
report, and a `debug` or `trace` level log can pinpoint the location of most
bugs" — the pure core is currently the biggest hole in that story.

### Proposed Approach
Add a pass instrumenting the domain's public entry points: `trace!` at the
top of each merge primitive and `apply_sync_report` (inputs' identities and
chosen outcome per rule R1–R9 — typed values only, no text assertions in
tests, user content only at `trace`), `debug!` for rule decisions (e.g.
"R5 resurrect" as the enum/variant facts, not prose). Two open design
questions to settle first:
1. Domain purity: the crate's dependency cap is `serde`/`chrono`/`uuid`/
   `thiserror` with zero incidental deps; adding `tracing` (workspace-pinned,
   no features) relaxes the documented cap and deserves a one-line ADR 0004/
   architecture.org amendment rather than a silent addition.
2. Alternatively (if the cap stays), instrument at the Phase 3 engine
   boundary where the policy is invoked, accepting coarser attribution.
Trigger: before or during Phase 3 work, so engine-level debugging benefits
from the start.

## [2026-10-09] Tombstone purge mechanism

- **Category**: `FEATURE`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md` §4.3/§11
- **Target Area**: `taskboard-domain` (new `PersistenceAction::PruneTombstones { older_than }`), `taskboard-storage-sqlite` (execution), actor/CLI surface

### Context & Description
MVP retains tombstones forever (correct for clock comparison across
boots, ADR 0004 R3/R5/R6). Local db growth is bounded by real single-board
usage, but long-lived kiosks eventually need pruning.

### Proposed Approach
`DELETE FROM tasks WHERE deleted = 1 AND ck_deleted < ?` (stacks/labels/
boards likewise; `task_labels` follows via `ON DELETE CASCADE`). The
schema already supports this as pure SQL — no migration needed. Trigger:
local db growth observed in real use (M1).

## [2026-10-09] Criterion bench for the persistence hot path

- **Category**: `PERF`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md` §11
- **Target Area**: `taskboard-storage-sqlite` (`[[bench]]` target)

### Context & Description
`load()` hydration and one `apply` batch are the storage hot path; the
benchmark CI job exists and currently no-ops. The kiosk perf goal
(AGENTS §1) makes regressions here worth tracking.

### Proposed Approach
Criterion benches at N ∈ {100, 1 000, 10 000} tasks. Kept out of Phase 2
only to hold PR size.

## [2026-10-09] Read pool / concurrent loads

- **Category**: `PERF`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md` §11
- **Target Area**: `taskboard-storage-sqlite` (`connect.rs` pool shape)

### Context & Description
Phase 2 uses `max_connections(1)` (single-writer MVP, ADR 0005); reads go
through the actor. If Phase 3 ever wants parallel reads while the actor
writes, WAL readers can run on separate connections.

### Proposed Approach
Revisit `max_connections` + WAL reader semantics when a concrete
concurrent-read need exists; not before.

## [2026-10-09] Outbox compaction/coalescing at rest

- **Category**: `FEATURE`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md` §11
- **Target Area**: Phase 4 push side (storage exposes append/remove only)

### Context & Description
Storage deliberately stores and orders ops (plan decision 8); coalescing
update+delete → delete etc. is actor logic built on these types.

### Proposed Approach
Implement coalescing as a Phase 4 push-side policy over the outbox port
surface; no storage change required.

## [2026-10-09] sqlx error-class mapping refinement

- **Category**: `DESIGN`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md` §11
- **Target Area**: `taskboard-domain` port, `taskboard-storage-sqlite` (`error.rs`)

### Context & Description
Constraint violations map to `RepositoryError::Corrupted` (plan decision
12) — honest but coarse; a caller cannot distinguish "batch violated an
FK" from "stored data is damaged".

### Proposed Approach
If coarseness bites in practice, propose a port variant (`Rejected`?) with
an ADR rather than overloading `Corrupted` silently.

## [2026-10-09] Reconcile storage naming: `sync_metadata` table vs validator types; `TaskRepository` scope

- **Category**: `Architecture`
- **Originating Plan/Report**: review of `feat/storage-sqlite-repository`
- **Target Area**: `taskboard-storage-sqlite` (schema), `taskboard-domain` (`persistence.rs`)

### Context & Description
The table storing validator bundles is named `sync_metadata`, while every
type around it says "validator" (`ValidatorKey`, `SyncValidators`,
`PersistenceAction::UpsertValidators`). The mismatch is documented in the
code but persists in the schema. Separately, `TaskRepository` is named for
tasks while its surface persists whole `PersistedState` (boards, outbox,
sync status) — a name inherited from Phase 1 that this crate's first real
implementation makes visible.

### Proposed Approach
Both renames require an additive migration (`ALTER TABLE … RENAME` /
copy-table-and-rename per ADR 0005) plus a port rename rippling through
the domain, fake, and storage crates — bundle them with the first
migration that ships after Phase 2 to amortize the churn.

## [2026-10-09] Write batching / pipelining in the State Engine

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9, decision 5)
- **Target Area**: `crates/taskboard-state/` (actor loop)

### Context & Description
Phase 3 processes one message fully (plan → apply → interpret → publish)
before polling the next. CLI script throughput (many commands at once)
and kiosk disk wear would benefit from batching several commands into one
transaction, or overlapping applies with command processing. Local
sqlite applies are sub-millisecond, so this is a measured-need item, not
an MVP gap.

### Proposed Approach
Batch drain: after the loop wakes, drain the command channel into one
`Vec<CommandPlan>` (bounded), apply once, interpret once, publish once —
preserving FIFO and per-command receipts. Never reorder across sync
reports.

## [2026-10-09] Query-envelope variants on `EngineCommand`

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9, decision 17)
- **Target Area**: `crates/taskboard-state/` (`actor.rs`)

### Context & Description
Roadmap Phase 3's "oneshot queries" is realized as the Execute receipt
plus the Flush barrier; ArcSwap reads are the query surface. Typed
read-style variants (wait-for-idle, outbox dump, validators read for the
Phase 4 sync actor) have no consumer yet and were not built speculatively.

### Proposed Approach
Additive `EngineCommand` variants with oneshot replies when Phase 4's
(outbox dump / validators read) and Phase 5's (wait-for-idle) plans name
their consumers.

## [2026-10-09] Storage-failure `SystemEvent`

- **Category**: `Architecture` / `UI`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9)
- **Target Area**: `crates/taskboard-state/`, `crates/taskboard-domain/` (`SystemEvent`)

### Context & Description
`ExecuteError::Storage` currently surfaces only as a command reply plus
an `error!` log. A kiosk running fire-and-forget UI dispatches never
sees persistence failures. A broadcast `SystemEvent` variant would let
the UI raise a persistent error banner.

### Proposed Approach
Bundle with Phase 5's error-UX plan; consider debouncing (a failing disk
fires per command otherwise).

## [2026-10-09] UI command forwarder bootstrap task

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9, §12)
- **Target Area**: `crates/taskboard-app/` (Phase 5 bootstrap)

### Context & Description
The UI dispatches domain-typed `StateCommand`s on a plain `mpsc` (no
`taskboard-state` dependency). Someone must adapt that channel onto
`EngineHandle::dispatch` — a tiny bootstrap task owned by Phase 5.

### Proposed Approach
A `tokio::spawn`ed forwarder in the Phase 5 bootstrap: `recv()` →
`dispatch()`, logging dropped commands.

## [2026-10-09] Engine metrics counters

- **Category**: `DX` / `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9)
- **Target Area**: `crates/taskboard-state/`

### Context & Description
Command/rejection/signal counters would feed the kiosk dashboard; the
logging arms are deliberately not asserted in tests (AGENTS §5), so
metrics are the durable observability path.

### Proposed Approach
Atomic counters behind a small `EngineMetrics` struct exposed via the
handle (or a query-envelope variant once those exist); bundle with the
existing observability backlog entry.

## [2026-10-09] Debounced nudging (engine-side sync coalescing)

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (§9, decision 8)
- **Target Area**: `crates/taskboard-state/` (actor loop)

### Context & Description
Every mutating command forwards a `SyncNow` nudge pre-coalescing; the
Phase 4 sync actor's cycle-in-flight dedupe is the designed absorber.
If measurements show the nudge traffic itself matters (channel churn on
burst edits), the engine could debounce.

### Proposed Approach
Only after Phase 4 lands and a benchmark shows need: collapse consecutive
nudges while one is already queued (an `AtomicBool` or a queued-marker in
the loop).

## [2026-10-09] Miri + proptest isolation friction

- **Category**: `Testing` / `DX`
- **Originating Plan/Report**: Phase 3 local verification (plan decision 16)
- **Target Area**: `.github/workflows/miri.yml`, domain/state test suites

### Context & Description
Under Miri's default isolation, `Utc::now()` (domain `SystemClock` test)
and proptest's `FileFailurePersistence` (`getcwd`) are unavailable — the
weekly CI job will need either `MIRIFLAGS=-Zmiri-disable-isolation` or
scoped `#[cfg_attr(miri, ignore)]` gates (the clock test carries one
already, with the testing-strategy scope quoted).

### Proposed Approach
Decide a repo-wide policy before the next weekly job: prefer
`-Zmiri-disable-isolation` for the whole job (simplest; the crates are
pure logic and isolation's guarantees matter less than coverage), or
audit every proptest/wall-clock test for gates.

**Update (Phase 3 local verification):** three environment limits found
and handled — (1) `Utc::now()` in the `SystemClock` test: scoped
`#[cfg_attr(miri, ignore)]`; (2) proptest's `getcwd`: run Miri with
`-Zmiri-disable-isolation`; (3) insta's socketpair: the S-series
snapshot tests are miri-gated (engine logic beneath them is Miri-covered
by the A-series). Also note: even at 16 proptest cases the domain suite
takes ~45 min under Miri — the weekly CI job (120 min budget) may need
`PROPTEST_CASES` capping for the miri job, or it will already be near
the limit.
