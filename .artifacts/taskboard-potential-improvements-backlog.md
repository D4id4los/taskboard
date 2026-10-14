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

## Flaky Test Watchlist

A standing collection of tests observed failing (or hanging) without a
code cause — timing races, environment sensitivity, instrumentation
artifacts, real-server jitter. Append an entry each time a flake is
observed; **do not fix opportunistically inside unrelated work**.
Periodically (e.g. when the list grows to ~5 entries or before a
release), review the list in one dedicated pass: reproduce, diagnose,
and either fix the root cause or document the tolerated flakiness and
its mitigation.

Entry format per observation: date, test, where it failed (gate /
runner / invocation), symptom, evidence for the flake verdict
(re-runs, isolation), current suspicion, and any mitigation applied.

---

### [2026-10-11] `actor_wiremock` fails under the instrumented llvm-cov run

- **Test**: `taskboard-sync-nextcloud --test actor_wiremock` (all 12
  tests in the binary, exact case not captured from the aggregate
  output)
- **Where**: local full-gate run `cargo llvm-cov --workspace`
  (instrumented target dir `target/llvm-cov-target`), on branch
  `feat/app-cli-e2e` @ `b22721f`
- **Symptom**: the workspace llvm-cov run exited 101 with
  `error: test failed ... --test actor_wiremock`; the crate was not
  touched by the branch's changes.
- **Flake evidence**: the same test binary passes under llvm-cov in
  isolation (12/12), and the immediate full-workspace llvm-cov re-run
  is green. Not reproduced since.
- **Suspicion**: timing under instrumentation (wiremock request/response
  handling with coverage probes and the instrumented target dir) or
  parallel-test resource contention specific to the coverage build;
  no diagnosis attempted — the gate was re-run instead.
- **Mitigation applied**: none (re-run). If it recurs, capture the
  failing test name and the `--nocapture` output before re-running.
- **Cross-reference (2026-10-13)**: investigated as part of the
  coverage-flake dispatch; the confirmed CI root cause (corrupt profraw
  aborting the profile merge) is a different symptom family — it fails
  *after* a green suite, never a test exit 101 — so this occurrence is
  **separate, unexplained**, and stays on the watchlist.

### [2026-10-13] CI `Code Coverage` job: corrupt profraw on merge (×3, all tests green)

- **Test**: the whole-workspace instrumented run inside the CI
  `Code Coverage` job — **no test fails**; the job dies merging
  profiles after a fully green test suite.
- **Where**: three GitHub Actions runs of `ci.yml`: `main`
  @ run `38275173071` (2026-10-12 16:09Z), PR #47
  (`docs/reminders-design`) @ run `38275544869` first attempt
  (2026-10-12 16:12Z), `docs/connect-flow-design` @ run
  `38399239683` (2026-10-13 12:44Z).
- **Symptom**: identical in all three — exactly one profraw is bad,
  then the merge aborts:
  `taskboard-<pid>-<run>_N.profraw: invalid instrumentation profile
  data (file header is corrupt)` → `error: no profile can be merged`
  → `llvm-profdata merge -sparse -f …` exits 1. Test summaries:
  612/612 and 624/624 passed, 25 skipped.
- **Flake evidence**: all three runs passed on re-run/re-attempt
  (PR #47: rerun of the failed job green in 2m16s). The corrupt file
  differs per run (`_0`, `_1`, `_2` suffixes, PID-stamped names);
  never the same file twice; never more than one per run.
- **Suspicion**: a per-process profile-file collision — the
  PID-stamped profraw naming suggests spawned child processes
  (taskboard-app's pty REPL tests, IPC daemon/CLI subprocess tests)
  inherit the parent's `LLVM_PROFILE_FILE` and write the same file,
  interleaving/truncating it. Unverified. First CI occurrence
  (2026-10-12 16:09Z on `main`) postdates the pty-completion test
  landing (c1db45e) by ~2h; the 2026-10-11 local occurrence predates
  it — the two symptom families (test-fail vs merge-fail) may or may
  not share a root cause.
- **Mitigation applied**: none (re-run). Escalated per the watchlist
  policy: dedicated dispatch plan
  `.artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md`
  (investigation + fix, three-stage).
- **✅ Verdict (2026-10-13, stage-2 investigation)**: root cause
  confirmed empirically. The coverage build instruments every spawned
  binary child (daemons, CLI clients, REPLs); the tests legitimately
  kill such children mid-flight, and a SIGKILL landing inside a
  process's profile-write window leaves a partially written profraw
  whose header is invalid — reproduced deliberately at ~1 in 60 kills
  of the instrumented binary in its first 12 ms, byte-identical error
  string. `llvm-profdata merge -sparse -f` aborts the entire merge on
  one invalid input, so a fully green suite still failed the job.
  Eliminated along the way: cross-process `%m` file sharing (files are
  pid-unique; slot = pid mod core count), stale/cached profraws
  (cargo-llvm-cov scrubs the dir pre-run — junk-file probe), tool
  version drift (CI floated 0.9.1 / nextest 0.9.148 = local), and the
  "drop `%m`" naming fix (counterfactual: exit-write kills tear files
  the same way — the write window is unfixable at our layer). **Fix**:
  the coverage job is split into test phase (`--no-report`) →
  corrupt-profraw quarantine (`scripts/coverage-quarantine-profraw.sh`,
  every dropped file logged; a torn file holds no usable coverage) →
  report (`cargo llvm-cov report`); plus `workflow_dispatch` on
  `ci.yml` and pinned `cargo-llvm-cov@0.9.1,nextest@0.9.148`. No test
  changed, no gate downgraded. Evidence: first run on the fix caught
  and quarantined a real corrupt file that would have failed the job
  (`taskboard-4749-…_1.profraw`, run 38407784220); **five consecutive
  green `Code Coverage` runs** on the final config: 38407784220,
  38408165262, 38408595714, 38408947750, 38409304159. Full evidence:
  `.artifacts/reports/2026-10-13-coverage-flake-investigation-report.md`.
  Escalation resolved.

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

- **Status**: ✅ DONE (implemented 2026-10-12: `msrv` / "MSRV Check" job
  in `ci.yml` compiles `--workspace --all-features --locked` on the
  toolchain read from the root `Cargo.toml`; ADR 0003 amendment fixes
  the policy — the MSRV does not float, it moves only by explicit
  manifest bump + ADR update in one commit).
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

> **Status (2026-10-11): IMPLEMENTED** by M1-exit WI-5 (ADR 0012,
> plan `2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md`) —
> transparent auto-routing over a per-database local socket, `status`,
> `daemon stop`, systemd template. Remaining follow-ups live in the
> appended WI-5 entries below (keep-alive, auth knobs, TCP).

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

> **Status**: ✅ DONE (implemented 2026-10-14..2026-10-15, WI-I plans
> 1–4: `reminders.rs` domain model with proptests, engine tick arm +
> snooze/mute commands, sqlite migration 0004 + repo, app wiring with
> `[reminders]` config, `LogSink`, `tasks remind` verbs, and the
> overdue/due-soon marker — design
> `.artifacts/plans/2026-10-12-reminders-design-plan.md`, ADR 0013).
> Plan 5 (desktop toasts + interactive snooze) remains future work.

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

> **Status note (2026-10-10, Phase 5)**: decided in ADR 0008. The
> `CredentialStore` port shipped with the keyring store (default) and
> the *env-var* store as the sanctioned headless escape hatch
> (`credential_store = "env"` + `TASKBOARD_APP_PASSWORD` — explicit
> opt-in, never a silent fallback). Remaining future work for this
> entry: the encrypted-file store below.

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
- **Status**: ✅ PARTIALLY DONE — title half landed 2026-10-11 (WI-3: `boards rename`, `BoardClocks.title`, migration 0002, field-wise title merge, ADR 0004 amendment); color/archive verbs and their clocks remain.

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
- **Status**: ✅ DONE (implemented 2026-10-12, WI-F: `tracing` added with
  the workspace pin verbatim — `log` feature per coding guidelines §4, so
  the "no features" phrasing here was resolved against the guideline;
  architecture.org cap amended rather than an ADR; 78 instrumentation
  sites across `merge.rs`/`pipeline.rs`; logging arms deliberately
  uncovered per the deck-client precedent — no log-text assertions).

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
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: `SystemEvent::StorageFailed`, edge-triggered at the engine storage reply site, daemon logger).

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

## [2026-10-09] Miri + proptest isolation friction — ✅ DONE (SUBSUMED 2026-10-12)

**Resolved** by the Miri CI tuning item: the repo-wide policy landed as
"isolation stays on" — no `-Zmiri-disable-isolation` anywhere. The
proptest persistence abort is handled by
`PROPTEST_DISABLE_FAILURE_PERSISTENCE=1` forwarded into the interpreter
via `-Zmiri-env-forward` in the weekly job (env vars are invisible under
isolation — this is why the env gate alone never worked), wall-clock
tests carry scoped `#[cfg_attr(miri, ignore)]` gates, and the policy is
recorded in `docs/testing_strategy.org` §10.

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

## [2026-10-09] Reduce per-message state cloning in the State Engine

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase3-state-engine-plan.md` (decision 15, accepted trade-off) / Phase 3 review
- **Target Area**: `crates/taskboard-state/` (`core.rs`, `actor.rs`)

### Context & Description
Every accepted message currently deep-clones the entity maps several
times: `EngineCore::app()` clones four `BTreeMap`s per call, `interpret`
calls it twice (before/after comparison for publish-iff-changed) plus
once more inside `publish`, and the actor clones the action batch once
more to hand the repository its own copy. Irrelevant at personal
taskboard scale, but the project targets low-power kiosk hardware and
this is the per-interaction hot path.

### Proposed Approach
Analyse only if profiling shows a problem. The `AppState` projection is
already shared with the UI as an `Arc` inside the `ArcSwap`; the same
shape could apply to the internal message path — e.g. keep the working
state behind a cheaply cloneable handle (or share the maps as
`Arc<BTreeMap>` with copy-on-write semantics) so `interpret`'s
changed-detection compares cheap handles and `publish` swaps the very
`Arc` it built, instead of cloning out of the core again. The action
batch given to the repository could likewise be `Arc<[PersistenceAction]>`
or drained from the plan rather than cloned. Requires care to keep ADR
0006's single-advance rule (memory advances only by interpreting the
persisted batch) and the P2/P3/P4 equivalence pins intact.

## [2026-10-09] Miri CI tuning: verify and time-cap the weekly job — ✅ DONE (2026-10-12)

**Resolved** on `ci/miri-tuning`: the weekly job now runs four parallel
per-family jobs (domain core / pipeline / remote+state properties /
state crate) with `PROPTEST_CASES=16` caps and the persistence env gate
forwarded through `-Zmiri-env-forward` (isolation stays on). Green
in-budget full run of the final config:
<https://github.com/D4id4los/taskboard/actions/runs/38312493624>
(55 min wall, all jobs ≤ 55 min vs the 120-min timeout; plan §10 of
`docs/testing_strategy.org` records the exact scope). Key finding: env
vars do not reach the interpreted process under Miri's default
isolation, so the caps must be forwarded with `-Zmiri-env-forward` —
and the true interpreter-monsters are the plain `pipeline::` tests
(≈28 s native → ≈25 min interpreted), not the properties.

## [2026-10-09] Miri CI tuning: verify and time-cap the weekly job

- **Category**: `Testing` / `DX`
- **Originating Plan/Report**: Phase 3 review fix-up (this branch) / prior entry "Miri environment limits"
- **Target Area**: `.github/workflows/miri.yml`, proptest configs

### Context & Description
The Phase 3 review fixed the *aborts* the weekly Miri job would hit:
proptest failure-persistence is now disabled under `cfg!(miri)` via
`test_support::proptest_config` (the `getcwd` isolation abort at runner
startup), and `sqlite_smoke` (tempdir `mkdir` + sqlite FFI) is
`#[cfg_attr(miri, ignore)]`-gated like the insta S-series. Locally
verified: the A-series runs 12/12 under Miri (~9 s interpreted, plus
several minutes of compile), the smoke test is skipped, and the exact
aborting scenarios are gone. What is NOT verified is the full CI
invocation end-to-end: proptest properties are extremely slow under the
interpreter (one P-equivalence property at a *single* case exceeded a
9-minute local budget), so at 256 cases the job almost certainly blows
its 120-minute timeout — the job has never actually run green on this
suite.

### Proposed Approach
A dedicated agent (own branch, e.g. `ci/miri-tuning`, own checkout so
other work continues) drives `workflow_dispatch` on the weekly job and
iterates on config until green within budget: cap cases for the Miri job
via `PROPTEST_CASES` env in `miri.yml` (proptest reads it natively),
consider splitting domain/state into parallel jobs, and if interpreter
throughput still cannot fit, scope the job's test filter (unit + A-series
+ P2/P3 at reduced cases) with the scope documented in
testing_strategy §10. Landing state must keep the property: every
reduction is an environment/speed gate, never a weakened assertion.

## [2026-10-10] Duplicate-create crash window reconciliation

- **Category**: `Architecture` / `Reliability`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8
- **Target Area**: `crates/taskboard-sync-nextcloud/`, `crates/taskboard-domain/src/pipeline.rs`

### Context & Description
Deck has no idempotency keys: a crash after a create POST but before the
outcome reaches the engine re-POSTs on the next cycle, producing a
duplicate card. The evidence-then-verdict reporting (ADR 0007) narrows
the window to one oneshot hop; it cannot close it.

### Proposed Approach
Reconcile duplicate titles+timestamps at pull time (detect same-title
cards created within the crash window and adopt one binding), triggered
only when an observed duplicate exists.

## [2026-10-10] `Retry-After` handling in the poll backoff

- **Category**: `Performance` / `Reliability`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8 (client plan §9 leftover)
- **Target Area**: `crates/taskboard-sync-nextcloud/src/poll.rs`, `src/client.rs`

### Context & Description
The client's retry policy ignores `Retry-After` on 429s and the poll
backoff (`poll_backoff`) is a pure doubling curve; a rate-limiting server
cannot tell the actor how long to wait.

### Proposed Approach
Parse the header into `poll_backoff`'s floor: the actor's next deadline
is at least the advertised delay. Keep the pure seam so paused-time
tests stay deterministic.

## [2026-10-10] Filtered deleted-boards listing fallback

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8 (decision 11 follow-up)
- **Target Area**: `crates/taskboard-sync-nextcloud/src/actor.rs`

### Context & Description
Board absence from the `GET /boards` listing is a tombstone confirmed by
a detail read (decision 11, ADR 0007). If Deck's listing ever hides
soft-deleted boards in a way that defeats the confirm-GET (e.g. 403
confirmations under rate limiting), the delete-wins fallback would fire
on a false alarm.

### Proposed Approach
If the confirm-GET becomes unreliable, add an unfiltered/deleted-boards
listing when (if) the Deck API grows one — never speculate against a
documented endpoint.

## [2026-10-10] Listing→detail field-parity cache

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8
- **Target Area**: `crates/taskboard-sync-nextcloud/src/actor.rs`

### Context & Description
The pull already refreshes every *bound* card's detail per cycle (tier-2
verified: the archive flag never advances `last_modified`, and listings
lag Deck's cache). That is O(bound cards) unconditional GETs per cycle —
negligible at kiosk scale but the first scaling wall.

### Proposed Approach
Per-card *conditional* detail fetches (ETags) for changed cards only;
seeded from the card stamps already in the pull cache.

## [2026-10-10] Incremental pull (per-stack validators / per-card ETags)

- **Category**: `Performance` / `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8
- **Target Area**: `crates/taskboard-domain/src/persistence.rs`, `crates/taskboard-sync-nextcloud/src/actor.rs`

### Context & Description
`ValidatorKey` is per-board-listing today; any card change anywhere on
the board invalidates the whole stack listing. Post-MVP scaling path for
large boards.

### Proposed Approach
Per-stack (later per-card) validators stored under finer `ValidatorKey`
tags; the pull fetches only listings whose validators changed.

## [2026-10-10] Push pipeline parallelism

- **Category**: `Performance`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8
- **Target Area**: `crates/taskboard-sync-nextcloud/src/push_exec.rs`

### Context & Description
Push groups execute serially in dependency order — correct and simple.
Independent entity groups (e.g. unrelated label groups) could be in
flight concurrently on slow links.

### Proposed Approach
Parallelize only on measured need; keep the dependency direction
(stacks → labels → tasks) and the abort-on-transport semantics.

## [2026-10-10] Sync-actor metrics for the kiosk dashboard

- **Category**: `DX` / `Observability`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` §8
- **Target Area**: `crates/taskboard-sync-nextcloud/src/actor.rs`

### Context & Description
The actor logs cycle start/end, offline transitions, and per-endpoint
conditional outcomes via `tracing`, but no machine-readable metrics
exist for the kiosk dashboard (cycle duration, push/pull counts, retry
depth, backoff streak).

### Proposed Approach
Fold into the observability backlog entry: a `metrics`-shaped
`SystemEvent` or an `AppState` extension the UI can read lock-free.

## [2026-10-10] Per-card pull detail refresh is O(bound cards) per cycle

- **Category**: `Performance` / `Scaling`
- **Originating Plan/Report**: Review of `.artifacts/plans/2026-10-09-growth-roadmap-phase4-sync-actor-plan.md` (finding F4; ADR 0007's pull description)
- **Target Area**: `crates/taskboard-sync-nextcloud/src/actor.rs` (`pull_snapshot`)

### Context & Description
Tier 2 showed Deck's archive flag never advances the card's
`last_modified` and its listings lag the server cache, so the pull
refreshes every *bound* card's detail (one unconditional GET per bound
card per cycle) to observe the flag truthfully. Fine at kiosk scale;
4 + N requests per cycle grows linearly with the bound-card count.

### Proposed Approach
Trigger: a board with hundreds of bound cards or measured cycle cost.
Mitigation: conditional (`If-None-Match`) per-card detail fetches for
changed cards only, or refresh details only when the listing's
`lastModified` batch actually moved.

## [2026-10-10] XDG config discovery

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase5-app-bootstrap-plan.md` §8
- **Target Area**: `crates/taskboard-app/src/config.rs`
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: presence-checked XDG candidate between `TASKBOARD_CONFIG` and the CWD default; the write-target flip stays a backlog entry below).

### Context & Description
Phase 5's config path resolution is `--config` > `TASKBOARD_CONFIG` >
`./taskboard.toml` — a CWD-relative default only. Desktop deployments
expect `$XDG_CONFIG_HOME/taskboard/taskboard.toml` to be consulted
before falling back to the CWD file.

### Proposed Approach
Insert an XDG step between the env var and the CWD default in
`AppConfig::load`; document the final order. Cheap, self-contained.

## [2026-10-10] Encrypted-file credential store

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase5-app-bootstrap-plan.md` §3.3 (resolves the design half of the "[2026-10-08] Keyring fallback" entry)
- **Target Area**: `crates/taskboard-app/src/secrets.rs`

### Context & Description
The env store is fine for CI/SSH but wrong for long-lived desktop use:
env vars are process-scoped and visible to same-user tooling (`ps -E` on
some platforms). Deployments that refuse env vars *and* lack a Secret
Service (minimal kiosk images) currently have no sanctioned store.

### Proposed Approach
A third `CredentialStore` impl selected by `[nextcloud] credential_store
= "file"`: encrypted at rest, key management is the open design
question (passphrase-derived via argon2 with an interactive unlock, or
a generated 0600 key file for unattended kiosks). The port makes this a
drop-in addition.

## [2026-10-10] Systemd/journald packaging

> **Status (2026-10-11): PARTIAL — template DONE** (ADR 0012 ships
> `contrib/taskboard.service`, a `Type=simple` user unit with
> `Restart=on-failure`; tracing already logs to stderr → journald).
> Still open: `Type=notify` / sd_notify readiness (conditional on a
> deployment actually needing it).

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase5-app-bootstrap-plan.md` §8
- **Target Area**: packaging / `taskboard-app`

### Context & Description
Kiosk deployments supervise the daemon with systemd; the SIGTERM-graceful
shutdown (ADR 0008) is the hard part and is done, but a unit-file
template, `Type=notify` readiness, and a journald log destination are
still manual.

### Proposed Approach
Ship a `contrib/taskboard.service` template (user service, `Restart=
on-failure`, `SIGTERM` default) and note that the fmt subscriber's
stderr is journald-visible as-is for M1; sd_notify integration only if
start-up ordering actually needs it.

## [2026-10-10] Config doctoring / `taskboard config print`

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase5-app-bootstrap-plan.md` §8
- **Target Area**: `crates/taskboard-app`
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: decode-only `config print` with `valid`/`invalid_kind` in the payload, `ConfigError::kind()`).

### Context & Description
Support conversations need the *effective* config (post-defaults, post-
env) without secrets. Phase 5 keeps `AppConfig` secret-free by
construction, so a redacted dump is nearly free — but there is no CLI
surface for it until Phase 6's subcommands.

### Proposed Approach
Phase 6: a `config print` subcommand serializing the loaded `AppConfig`
(it is `Serialize` and secret-free); a `--json` flag matches the
planned output convention.

## [2026-10-10] Signal-escalation policy

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase5-app-bootstrap-plan.md` §8
- **Target Area**: `crates/taskboard-app/src/main.rs`

### Context & Description
The second Ctrl-C force-exits (`exit(130)`), which is standard daemon
UX but flat: no distinct exit codes for "aborted actor" vs "clean",
no flush-then-force ladder for kiosk supervision that wants a graded
response.

### Proposed Approach
If kiosk supervision ever needs it: a signal ladder (second signal =
abort actors immediately, third = `_exit`) with distinct exit codes,
documented in the runbook.

## [2026-10-10] Task selectors beyond exact uuid

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/commands/`
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: the cascading task selector — exact uuid → unique ≥8-char prefix → exact title, typed ambiguity; stacks/labels keep the two-tier shape by design).

### Context & Description
M1 `tasks done/move` take the exact local `TaskId` uuid (scripting-first).
Humans must copy it from `tasks list --json` first; prefix matching,
`--title`, or ordinal selectors would round out the UX.

### Proposed Approach
A shared `resolve_task(&AppState, &str)` that tries exact id, then a
unique id prefix (≥8 chars), then an exact-title match — mirroring
`resolve_stack`, returning the same `UnknownTask`/ambiguity errors.

## [2026-10-10] `tasks edit/delete`, `stacks`/`labels` subcommands

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/commands/`
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: `tasks edit/delete/label/unlabel`, `stacks rename/delete/move` — `stacks move` grew the domain `MoveStack` — and `labels list/add/edit/delete`).

### Context & Description
The domain commands exist (`UpdateTask`, `DeleteTask`, the full
stacks/labels verb set) but the M1 CLI deliberately ships only
`tasks list/add/done/move`. The outbox/push machinery needs no changes
for the missing verbs.

### Proposed Approach
Grow the `Tasks`/new `Stacks`/`Labels` clap groups one verb at a time;
each is a selector resolution + one `StateCommand` + a receipt,
reusing the Phase 6 handler pattern.

## [2026-10-10] `logout` subcommand

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/commands/`
- **Status**: ✅ DONE (implemented 2026-10-11, WI-3: secret delete always, config strip iff auto backend; the open backend-interaction question is decided in the ADR 0009 amendment).

### Context & Description
`CredentialStore::delete` shipped in Phase 5 unused; rotating away a
stale keyring entry currently means `secret-tool` by hand.

### Proposed Approach
`taskboard logout`: load config (typed errors if unconfigured), delete
the `user@host` entry, print a receipt; a `--forget-board` variant can
additionally drop the binding. A `--purge` variant for the local db is
a separate, destructive design question.

## [2026-10-10] `taskboard status` without triggering a cycle

> **Status (2026-10-11): DONE** — `taskboard status` (ADR 0012) reads
> the daemon's live engine state over IPC (never a cycle trigger,
> pinned by IP6/IP11) or the lock sidecar's holder pid client-side.

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` §7
- **Target Area**: `crates/taskboard-app/`

### Context & Description
Under the uniform-core doctrine every core boot fires an immediate sync
cycle, so a "just show me SyncStatus" read cannot exist as a plain
subcommand — it would silently do network I/O.

### Proposed Approach
Prefer daemon IPC: a `status` query over the IPC surface, drafting
from the `--json` payload contract (see the Phase 6 plan's consumer
handoff map). Note: once the stand-alone-first work lands,
`backend = "none"` boots already cycle-free, so the remaining hard
case is a configured connector — exactly what daemon IPC answers.

## [2026-10-10] Versioned `--json` schema

> **Status (2026-10-11): ENVELOPE DONE, payloads deferred** — the
> stated trigger (first IPC consumer) is served by the versioned wire
> envelope (`"v": 1` per variant, typed `VersionMismatch`/
> `ipc-version`, ADR 0012). The `--json` payload shapes inside `out`
> stay explicitly unversioned; when an external consumer appears,
> payload versioning copies the envelope pattern.

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/commands/output.rs`

### Context & Description
The Phase 6 JSON payloads and error kinds are stable within M1 but
carry no compatibility guarantee; external scripts will accrete around
them anyway.

### Proposed Approach
When real consumers appear: a schema version field (or `--json=v2`
gate), serde renames frozen, and payload diffs covered by snapshot
tests in the app crate.

## [2026-10-10] Stand-alone-first product shape: optional, pluggable sync connectors

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-10-growth-roadmap-phase6-cli-and-e2e-plan.md` (user decision, 2026-10-10)
- **Target Area**: `crates/taskboard-app/` (bootstrap/config), `taskboard-sync-nextcloud` (becomes the first connector), `taskboard-domain` (connector port), `docs/architecture.org`

### Context & Description
The M1 milestone couples the core to Nextcloud: `bootstrap()` requires a
validated `server_url`/`username` and an app password before it will
spawn anything, so no command runs on a never-logged-in machine. The
**intended final configuration at M2 (v0.1.0)** — per the user's
2026-10-10 decision — is an app that **works fully stand-alone** (local
task management with no server configured) and treats Nextcloud sync as
an **optional connector**, with room for future connectors of the same
shape: a plain-file backend on a synchronized folder (Dropbox, OneDrive,
ownCloud/Nextcloud desktop client), etc.

### Proposed Approach
Implement shortly after Phase 6, while the architecture change is still
minor. Sketch:

1. Introduce a sync-connector **port** in `taskboard-domain` (the
   existing `SyncCommand`/`SyncReport`/`SyncStateReader` vocabulary is
   already connector-agnostic — ADR 0007's engine contract needs no
   change) and make `taskboard-sync-nextcloud` its first implementation.
2. Make the connector optional in config: `[sync] backend = "none" |
   "nextcloud"` (default `none` for fresh installs; a config carrying a
   `[nextcloud]` table keeps today's behavior). `bootstrap` spawns the
   sync actor only for a real connector; `none` means local-only mode
   with `SyncPhase::Idle` forever, no credentials required, and
   `boards select` absent from the ceremony.
3. The state engine, storage, outbox, and UI layers are already
   connector-blind and should need zero or near-zero changes — the
   outbox simply stays the local write-ahead queue that a connector
   drains when one is configured (adding a connector to an existing
   local database must drain the accumulated outbox safely; verify the
   merge pipeline's behavior for ops authored with no remote bindings).
4. Each new connector is a new `taskboard-sync-<name>` crate; the
   domain stays Deck-agnostic (already true since Phase 1).

This effectively re-frames the roadmap's "offline-first" from
"sync-capable-when-offline" to "stand-alone by default, sync when
configured", and should be reflected in `docs/architecture.org` and a
roadmap amendment when picked up.

## [2026-10-11] `parted`-style interactive REPL mode

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md` (user idea, 2026-10-11)
- **Target Area**: `crates/taskboard-app/` (`commands/`, `cli.rs`, `main.rs`)

### Context & Description
Bare `taskboard` currently resolves `[app] mode` (the daemon until
Phase 7). A `parted`-style REPL — no arguments drops the user into a
shell where each line is parsed and dispatched against one booted
core, `exit`/Ctrl-D leaves — would make daily driving and SSH/kiosk
administration far cheaper than one-process-per-command. The seam
already exists: every verb wraps an `AsyncFnOnce(&App)` body in
`with_core`; extracting those bodies is most of the work and is
shared with the daemon-IPC entry below.

### Proposed Approach
Extract the `&App`-taking handler bodies; loop `rustyline` (new MIT
workspace dep) lines → `Cli::try_parse_from(shell-split(line))` →
dispatch, printing clap usage errors in-loop instead of exiting.
Decide the no-argument default (TTY-sniff → REPL vs explicit
subcommand vs parted-style always-REPL) — nothing ships pre-v0.1.0,
so changing the default is cheapest now. A REPL session holds the
instance lock for its lifetime (stop the daemon first). Sequence
after the CLI-completeness work so the verb surface justifies
entering the shell.

### [2026-10-11] Title-match reconciliation for board attach

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-growth-roadmap-phase6b-standalone-first-plan.md` §7
- **Target Area**: `taskboard-domain` (merge/pipeline), `taskboard-app` (`commands/boards.rs`)

#### Context & Description
Attach is identity-based (ADR 0010): binding a local board onto a
remote board that already holds similarly-named content *unions* both
sets — the outbox re-creates the local entities remotely while the
pull adopts the remote ones. Accepted and documented in Phase 6b
because `boards publish` onto a fresh board avoids the situation
entirely.

#### Proposed Approach
Only if real usage hits it: a reconciliation pass at bind time that
matches local-only entities to remote ones by normalized title and
adopts the remote identity instead of enqueueing the create op.

### [2026-10-11] Board reset/delete ceremony

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-growth-roadmap-phase6b-standalone-first-plan.md` §7
- **Target Area**: `taskboard-domain` (new `DeleteBoard` command + cascade), `taskboard-app`

#### Context & Description
There is no `boards delete` with cascade + confirmation; the documented
reset is "new db path". A destructive ceremony needs its own design
round (tombstone semantics, remote cascade, confirmations).

### [2026-10-11] CLI commands to add a Nextcloud connection

- **Category**: `DX`
- **Originating Plan/Report**: Chat/Context (follow-up to Phase 6b, ADR 0010)
- **Target Area**: `crates/taskboard-app` (`commands/login.rs`, `commands/mod.rs`)

#### Context & Description
Since Phase 6b, `login` refuses with the typed `sync-disabled` while
`[sync] backend = "standalone"` is explicit, and under the auto rule a
stand-alone install has no `server_url` for `login` to verify against
without first hand-editing `taskboard.toml`. There is no CLI path that
*introduces* a Nextcloud connection to an existing stand-alone install
the way `boards create`/`stacks add` introduce local content.

#### Proposed Approach
Connector-management verbs (e.g. `taskboard connect add` / `connection
set <server> <user>` or a relaxed `login` that accepts a server
argument under auto): write `server_url` (and switch `backend` when
explicitly standalone) into the config, then run today's
verify-then-persist ceremony. Must respect ADR 0010's loudness rule —
no silent backend flips without an explicit command — and reuse the
L-series ceremony + test harness. Related open question already
tracked for WI-3: `logout`'s interaction with `backend`.

> **Status note (2026-10-12, WI-J design round)**: the design round
> landed as `.artifacts/plans/2026-10-12-connect-flow-design-plan.md`
> (draft ADR 0014 inside — a bare `taskboard connect` verb; `login`
> becomes its deprecated alias). Fact-check correction recorded there
> (§1.1): under the **auto** rule `login` already works on a fresh
> stand-alone install (it prompts for the server); the one-shot gap
> is explicit-`standalone` installs plus naming. Direction
> **approved** 2026-10-12 (round 2): D1 `connect` + deprecated
> `login` alias; D2 `--switch-backend` for scripts **and** a y/N
> confirm on a TTY; D3 re-point interactive by default with `--force`
> for scripts (old secret deleted after the new verify). Round 3
> (same day, post WI-H merge PR #48): D5 resolved by events — the
> detach half of connector liveness is on `main`
> (`SyncCommand::Detach`, `App::detach_connector`, the
> `DetachConnector` IPC verb with the additive-no-bump policy), so a
> future daemon-side attach verb rides that pattern with no envelope
> decision left to make.
>
> **Status**: ✅ DONE (implemented 2026-10-13/14, chunks 1–3 of the
> WI-J dispatch: `feat/connect-engine-attach` — the attach primitive,
> `feat/app-connect-verb` — the one-shot ceremony + deprecated alias,
> ADR 0014; the session half is the sibling entry below).

### [2026-10-14] In-session `status` should surface `connector_attached`

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` §4.6 (rider named at design round 3; recorded per AGENTS §7 during WI-J chunk 3)
- **Target Area**: `crates/taskboard-app/src/commands/status.rs`

#### Context & Description
ADR 0014 made the session's connector attach/detach dynamic (a
`connect`/`logout` line reshapes the running core), but the in-session
`status` view (`StatusView::in_session()`) still answers
`connector_attached: None` — it predates dynamic attachment. Today the
honesty rides `sync`'s typed refusals (`sync-disabled` after a logout,
`sync-unavailable` after a detach), which the journeys pin; a `status`
line just does not name the fact.

#### Proposed Approach
`dispatch_line`'s `Command::Status` arm reads `app.connector_attached()`
and threads it into the view (a one-line honesty improvement; needs a
session-status constructor that takes the flag). The JSON-shape
addition is pre-v0.1.0-sanctioned.

### [2026-10-14] Scoped SIGINT handler so Ctrl-C cancels an in-session prompt line instead of ending the process

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-14-app-connect-session-plan.md` (D7's pty-tier verification, outcome recorded in ADR 0014)
- **Target Area**: `crates/taskboard-app/src/repl.rs` (editor thread)

#### Context & Description
ADR 0011 gives Ctrl-C at the session prompt line-level semantics
(rustyline maps it to `Interrupted`; the session survives). Mid-ceremony
the outcome follows the environment's SIGINT disposition (ADR 0014's
recorded verification): with the default disposition, cooked prompt
reads and `rpassword`'s masked read (ISIG off, ^C re-raised) both end
the process mid-line; where SIGINT is ignored, the typed cancellation
path already survives. Accepted for the connect-flow round (nothing is
persisted mid-prompt, so death is safe), but the UX is asymmetric:
Ctrl-C one keystroke earlier or later means "clear the line" vs "quit
the app".

#### Proposed Approach
Around a ceremony's prompted reads on the editor thread, install a
scoped SIGINT handler that aborts the active read and delivers a
cancellation through the disposition oneshot (the typed
`CommandError::Prompt` path already exists); rustyline's own Ctrl-C
mapping must not fight it. Signal-handler machinery on the reader
thread — its own focused round.

### [2026-10-14] Daemon-side attach verb: connect a running daemon without restarting it

- **Category**: `Architecture`, `DX`
- **Originating Plan/Report**: `docs/adr/0014-connector-attach-session-login.org` decision 7; `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` §6
- **Target Area**: `crates/taskboard-app/src/ipc/` (protocol + server), `crates/taskboard-app/src/commands/` (client-side relay)

#### Context & Description
ADR 0014 gave the REPL (and future GUI) in-session attach/detach, but a
daemon's core stays connector-less until restart: a one-shot `connect`
run while a daemon serves persists the config and secret, yet the
running daemon never attaches (its boot already happened). The three
ceremony verbs are terminal-bound and refused over the socket (ADR
0014's IPC guard) — correctly so, since a socket cannot answer a masked
prompt. What the daemon needs is not the ceremony but its last step:
attach from already-persisted state.

#### Proposed Approach
An additive `Request::AttachConnector`-shape verb (no version bump,
WI-H's `DetachConnector` pattern verbatim): the client sends only the
identity (server URL, username — **never the secret**); the daemon
re-reads its config and loads the credential store itself, then calls
the merged `App::attach_connector` (with the same `wait_for_boot_pull`
settle the REPL uses). Typed outcomes mirror the detach vocabulary
(`attached`/`already-attached`/`engine`/`draining`). No envelope
decision outstanding — the pattern is established and the primitives
are merged. Note the env-store caveat: `TASKBOARD_APP_PASSWORD` is
process-local, so attach works for the keyring store; for the env store
the daemon's own environment governs (restart-with-env stays that
store's operating model).

## [2026-10-11] Undo/redo functionality

- **Category**: `Architecture`, `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi3-cli-editing-surface-plan.md` §7
- **Target Area**: `crates/taskboard-domain/` (inverse commands or a mutation journal), `crates/taskboard-app/src/commands/`

### Context & Description
Mutations are persistent (tombstones, outbox ops), so undo means either
inverse commands or a journaled mutation history — a domain-level
feature, not a CLI wrapper. Redo of an already-pushed edit is a *new*
edit remote-side (not a retraction), and undo before a sync push could
instead retract the queued op — the outbox is a natural seam either way.

### Proposed Approach
**Open decision for that plan: undo *stack* vs undo *tree*** — a linear
stack is simpler and matches most CLI conventions but discards the redo
branch when new input follows an undo; a tree preserves every branch
(the Phase 7 UI's natural model) at the cost of a navigation/selection
UX. Decide when planning the feature; record in an ADR.

## [2026-10-11] Board color/archive edit verbs and their clocks

- **Category**: `Feature`
- **Originating Plan/Report**: remainder of the "[2026-10-09] Local
  board-edit commands and `Board` clocks" entry; WI-3 delivered its title half
- **Target Area**: `crates/taskboard-domain/` (entities, messages,
  command, merge, push), `crates/taskboard-storage-sqlite/` (migration),
  `crates/taskboard-app/src/commands/boards.rs`

### Context & Description
`BoardClocks` has room for `color`/`deleted` clocks; the field-wise
title merge (ADR 0004 amendment) is the template. No local verbs exist
yet, so those fields remain adoption-governed.

## [2026-10-11] XDG write-target flip for `login`

- **Category**: `DX`
- **Originating Plan/Report**: WI-3 (reads shipped first, deliberately)
- **Target Area**: `crates/taskboard-app/src/commands/login.rs`
- **Status**: ✅ DONE (implemented 2026-10-12, WI-D: `AppConfig::write_path`
  truth table — first run with no config anywhere persists to the XDG
  candidate; an existing CWD config keeps its file via the existence
  guard)

### Context & Description
`login` on a machine with *no* config anywhere still creates
`./taskboard.toml` (the CWD default). Desktop nicety: write XDG-first
when no config exists anywhere. Cheap follow-up now that reads ship.

## [2026-10-11] `--forget-board` / board unbind

- **Category**: `Architecture`
- **Originating Plan/Report**: "[2026-10-10] `logout` subcommand" follow-up, triaged out of WI-3
- **Target Area**: `crates/taskboard-domain/` (new `UnbindBoard` command), `crates/taskboard-app/src/commands/boards.rs`

### Context & Description
No `UnbindBoard` domain command exists; logout deliberately leaves the
board binding and local db untouched. Own design question: what happens
to remote-bound content on unbind (keep local copies? re-adopt on
rebind?).

## [2026-10-11] Prefix selectors for stacks/labels

- **Category**: `DX`
- **Originating Plan/Report**: WI-3 scope decision (task-only prefixes)
- **Target Area**: `crates/taskboard-app/src/commands/tasks.rs` (`resolve_stack`), `commands/labels.rs` (`resolve_label`)

### Context & Description
The cascading prefix selector is a task-only affordance; stacks and
labels keep the two-tier uuid-or-exact-title shape. Lift the tier if
users ask.

## [2026-10-11] `StorageFailed` for the report-apply path

- **Category**: `Architecture`
- **Originating Plan/Report**: WI-3 scope note (command-reply path only)
- **Target Area**: `crates/taskboard-state/src/actor.rs` (`ingest_sync_report`)

### Context & Description
Persistence failures while applying sync reports (no reply channel)
still only log; extending the edge-trigger there needs a broadcast seam
in `ingest_sync_report`. Do it when Phase 7 actually needs it.

## [2026-10-11] `CommandError::Config` kind refinement

- **Category**: `DX`
- **Originating Plan/Report**: WI-3 (deferred to WI-5's wire-format pass)
- **Target Area**: `crates/taskboard-app/src/commands/mod.rs`
- **Status**: ✅ DONE (implemented 2026-10-12, WI-D: `error_kind` delegates
  to `ConfigError::kind()` — the envelope carries the specific stable
  discriminants instead of the flat `"config"`)

### Context & Description
Other commands could surface `ConfigError::kind()` instead of the flat
`"config"` kind in the JSON error envelope; defer so the wire format
changes once, together with WI-5's IPC payload pass.

## [2026-10-11] Outbox `board_id` column for board-scoped ops

- **Status**: ✅ DONE (implemented 2026-10-12, branch `fix/storage-outbox-board-id`: migration 0003 + backfill, codec/repo board column, zero BoardId↔StackId casts remain).
- **Category**: `Refactoring`
- **Originating Plan/Report**: WI-3 review (the `RenameBoard` codec workaround)
- **Target Area**: `crates/taskboard-storage-sqlite/` (migrations, codec.rs, repo.rs)

### Context & Description
The outbox ops table has task/stack/label id columns but no board
column. `LocalOp::RenameBoard` therefore rides `stack_id` under its own
`rename_board` op-kind tag (a `BoardId` cast through `StackId`). It is
documented, type-sound at the codec boundary, and roundtrip-tested, but
the cast is a hack a future reader can trip over — and the next
board-scoped op (color/archive verbs, unbind) would widen it.

### Proposed Approach
Migration 0003: add a nullable `board_id` column, backfill it from
`stack_id` where `op_kind = 'rename_board'`, and teach `op_to_columns`/
`op_from_columns` the proper column. One migration, mechanical codec
edits, existing roundtrip test extended.
### [2026-10-11] Org-mode markup fields + two-way local .org sync connector

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/reports/2026-10-11-org-mode-support-viability-report.md`
- **Target Area**: new `taskboard-org` + `taskboard-sync-org` crates; `taskboard-domain` (`SyncReport` payload generalization); `taskboard-app` (config/bootstrap); `taskboard-ui-slint` (rendering)

#### Context & Description
Analysis confirmed both org-mode features viable: (A) rendering
org-markup text fields (emphasis, headings, lists, blocks, drawers,
TAB folding) and (B) two-way sync with a local `.org` file (h1 =
boards, h2 = tasks, TODO keywords ↔ stacks, idempotent
read→parse→map→write loop). No maintained permissively-licensed Rust
crate combines lossless parsing with write-back (orgize 0.10-alpha is
closest but dormant since 2025-01); Slint rich-text display exists only
since 1.16 (`StyledText`) and rich-text editing is still open upstream.
Estimated 4–7 weeks for B, 2.5–5 for A, sharing one parser core.

#### Proposed Approach
Per the report §8: 2–4 day orgize spike behind a `taskboard-org`
wrapper (fork/vendor if adopted), else build the scoped two-layer
parser; phases = org read-only import → rendering MVP (StyledText +
custom block layout, source-edit toggle) → surgical write-back
(splice edits + canonical emitter with proptest idempotency fixpoint)
→ WYSIWYG deferred to Slint upstream. Requires a small ADR to
generalize the Deck-typed `SyncReport::Completed` snapshot for a
second connector. Out-of-scope mapping gaps surfaced by the analysis:
`SCHEDULED:` lines, priority cookies `[#A]`, and org CLOCK logbooks
have no domain fields (preserved verbatim for now).

## [2026-10-11] Emacs/org edit-backend crate + per-UI adapters

- **Category**: `UI` / `Architecture`
- **Originating Plan/Report**: `.artifacts/reports/2026-10-11-emacs-org-edit-backend-viability-report.md`
- **Target Area**: external `orgedit-core` crate (standalone repo); `crates/taskboard-ui-slint/` (future consumer)

### Context & Description
Viability confirmed for a framework-agnostic emacs/org editing backend
(buffer, point/mark/region, kill-ring + append rules, yank/yank-pop,
mark ring, chained undo, universal argument, chord keymap, org
folding/structure commands driven by the `taskboard-org` parser spans)
with thin egui/iced/Slint adapters — the seam drawn at byte offsets and
abstract events, never pixels. No existing Rust crate fills the
emacs-semantics gap (rustyline/reedline are line editors;
floem-editor-core is modal). Slint cannot host live-decorated editing
until upstream rich-text editing (#2723) lands, so its adapter is a
source-edit surface plus a rendered preview. Out of scope for taskboard
now: the crate is cross-project personal infrastructure and the UI
crate is still an empty shell.

### Proposed Approach
Build the crate standalone (MIT/Apache; report §4/§8 sequencing —
Emacs-manual conformance corpus first) when the org-mode work starts;
taskboard's editor fields then consume it as a thin `orgedit-slint`
widget (stock `TextEdit` editing surface with intercepted emacs keys,
cursor synced via `set-selection-offsets`, `StyledText` rendered
preview) instead of bespoke editing logic.

## [2026-10-11] `parted`-style interactive REPL mode — *implemented* (WI-4, ADR 0011; kept for the completion-history of its open sub-questions)

### [2026-10-11] `login` inside a running session (REPL / GUI)

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi4-repl-interactive-frontend-plan.md` §2.4/§7 + user decision at plan review (2026-10-11)
- **Target Area**: `crates/taskboard-app` (`repl.rs`, `commands/login.rs`, `bootstrap.rs`)

#### Context & Description
The REPL refuses `login` in-session (ADR 0011): credential +
`DeckClient` wiring happens in `bootstrap`, so a mid-session login
cannot activate the connector in the already-running core — the honest
v1 UX is "exit, `taskboard login`, re-enter". This is more than a CLI
nicety: the Phase 7 GUI will need *login while the app is running* for
good interaction flows around the Nextcloud login feature anyway (a
desktop app cannot ask users to quit and relaunch to connect), so the
design should be done once for both frontends.

#### Proposed Approach
A session-level connect flow: after credentials are verified and
persisted (the L-series ceremony, minus the process-exit shell), mutate
the *running* session — build a `DeckClient` from the live config +
store and attach a sync actor to the booted core (bootstrap currently
commits to `sync: Option<SyncActor>` at construction; an attach path
needs that to become settable, or a dedicated re-wiring of the sync
channel halves). Interacts with the "CLI commands to add a Nextcloud
connection" entry above (the persistence half is shared) and with
rustyline's terminal for the masked-secret prompt.

> **Status note (2026-10-12, WI-J design round)**: designed in
> `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` (draft
> ADR 0014 inside), direction **approved** 2026-10-12 (round 2);
> round 3 folds in WI-H's merge (PR #48). Attach resolves to an
> **additive engine rewire over a fan-out/fan-in topology**
> (command side: `Vec` of connector senders +
> `EngineCommand::AttachConnector` push + lazy pruning over *both*
> dead-entry paths — standalone boots and detached actors; report
> side: one permanent fan-in inbox, every actor clones the sender;
> `App::attach_connector(&self)` mirrors the merged
> `detach_connector(&self)` and spawns the actor, which self-targets
> from the persisted binding): parked command halves were rejected
> because a live-but-undrained receiver flips standalone nudge
> semantics into a perpetual `Syncing` transient
> (`forward_sync_now` treats `Ok` *and* `Full` as forwarded), and
> in-process re-bootstrap was rejected (lock-epoch hole). The masked
> prompt resolves to an editor-thread disposition handshake
> (`Line`/`Masked`/`Confirm`; never into history). The topology is
> deliberately multi-connector-ready (see the entry below). Per WI-H
> plan D7's handoff, this round also owns the **in-session
> `logout`** ceremony (local teardown + `detach_connector` + config
> refresh — plan §4.6), so both ADR 0011 session refusals die.
> Daemon-side attach over IPC remains future work riding WI-H's
> merged additive-verb pattern (no secret on the wire — the daemon
> re-reads its config and credential store).
>
> **Status**: ✅ DONE (implemented 2026-10-14, chunk 3
> `feat/app-connect-session`: the in-session connect/logout ceremonies
> over the editor-thread disposition handshake — config refresh +
> `attach_connector`/`detach_connector`, ADR 0014. The REPL half; the
> GUI/daemon halves remain future work riding the same
> frontend-agnostic ceremony functions).

### [2026-10-12] Multiple active sync connectors

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-connect-flow-design-plan.md` §4.5 (user mandate at WI-J plan review, round 2)
- **Target Area**: `crates/taskboard-app/` (bootstrap, config), `taskboard-domain/` (messages/projections), `taskboard-state/` (projection only)

#### Context & Description

ADR 0010 named future `taskboard-sync-<name>` connector crates (one
at a time); the user wants the *option of several simultaneously
active connectors* (e.g. two Nextcloud instances, or Nextcloud + a
future backend) kept as a product direction. The WI-J design round
(2026-10-12) was amended so v1 builds count-agnostic seams now — the
engine's additive fan-out registry, the permanent fan-in reports
inbox, `App.connectors: Vec` — while enforcing "at most one active
connector" purely at the app/config layer (`attach_connector`'s
`AlreadyAttached` guard + the scalar `[sync] backend` word).

#### Proposed Approach

The multi-connector round lifts the app-layer guard and owns the
deferred list in the WI-J plan §4.5: per-connector `SyncCommand`
routing keys, per-connector status projections (the `AppState.sync`
singleton becomes a map), per-connector board-ownership tags on
bindings, config `[[sync.connectors]]` array-of-tables with a
resolution rule against the legacy scalar, merged network authority
for `NetworkLost/Restored`, `connect <name>` / `connect list/remove`
verb growth, and possibly the connector trait promotion ADR 0010
deferred. Every item is additive; none re-opens the channel topology
the WI-J round fixed — that is the round's acceptance test.

### [2026-10-11] REPL history persistence (file-backed)

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi4-repl-interactive-frontend-plan.md` §2.7/§7
- **Target Area**: `crates/taskboard-app` (`repl.rs`)

#### Context & Description
v1 REPL history is in-memory only (rustyline session history). File
persistence needs the XDG data-dir story WI-3 owns (where the history
file lives, permissions); coupling WI-4 to it would have widened the
rebase for no functional loss.

#### Proposed Approach
rustyline's `with-file-history` once a data-dir decision exists;
load/save around `spawn_line_reader`'s editor construction.

### [2026-10-11] REPL tab-completion / hints

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi4-repl-interactive-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app` (`repl.rs`)
- **Status**: ✅ DONE (implemented 2026-10-12 via
  `.artifacts/plans/2026-10-12-app-repl-completion-plan.md`: a
  rustyline `Helper` over the clap shape + a lock-free
  `ArcSwap<AppState>` read on the editor thread; **hints/ghost text
  explicitly deferred** — the `Helper` seam makes them a contained
  follow-up)

#### Context & Description
No completion over verbs, selectors, or board/stack/task titles yet.
Worthwhile once the verb surface settles post-WI-3 (completing a moving
target doubles the churn).

#### Proposed Approach
rustyline `Helper` trait: verb/flag completion from the static CLI
shape; board/stack/title completion from the live `ArcSwap<AppState>`
(read lock-free on the editor thread).

### [2026-10-12] REPL completion: uuid candidates for selectors

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-app-repl-completion-plan.md` §7
- **Target Area**: `crates/taskboard-app` (`src/completion.rs`)

#### Context & Description
When two live tasks share a title, title completion yields one
deduplicated menu entry and the command answers with the honest
`Ambiguous*` error. Uuid candidates (display `title (uuid)`, replace
the uuid) would disambiguate directly.

#### Proposed Approach
A new `Kind` variant on the existing engine: extend the live-title
generator to emit uuid-disambiguated candidates for hard-colliding
titles only; the menu stays quiet otherwise.

### [2026-10-12] REPL completion: remote board ids for `boards select`

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-app-repl-completion-plan.md` §7
- **Target Area**: `crates/taskboard-app` (`src/completion.rs`)

#### Context & Description
`boards select <id>` takes a remote Deck board id that only exists in
`boards list` output — no local source to complete from today.

#### Proposed Approach
Needs a small cache design (the last `boards list` response, e.g. in
the session state); revisit once a remote-value cache exists.

### [2026-10-11] Graceful Ctrl-C line-cancellation in the REPL

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi4-repl-interactive-frontend-plan.md` §2.6/§7
- **Target Area**: `crates/taskboard-app` (`repl.rs`)

#### Context & Description
Ctrl-C while a REPL command runs keeps the default SIGINT disposition:
it kills the session. Safe by construction (state is durable per
`execute`; the OS releases the instance lock), but a long `sync` line
dying to a stray Ctrl-C is rough. v1 accepts this.

#### Proposed Approach
A SIGINT handler during dispatch that aborts the *line* (drop the
in-flight future), not the session. Design it together with WI-5's
process-signal story (the daemon's second-signal force exit) — one
signal design, once.

## [2026-10-11] IPC keep-alive / pipelining / event streams

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/ipc/`

### Context & Description
Wire protocol v1 is one request per connection (connect → frame →
response frame → close): right for a CLI process, wasteful for a
chatty client. A subscribe-style push channel (UI repaints or reminder
events over the socket) is the natural protocol-v2 design and has a
topology to grow from (the serial dispatcher + per-connection tasks).

### Proposed Approach
Version the framing negotiation into the existing envelope
(`PROTOCOL_VERSION = 2`), keep argv+rendered-text as one request class
and add a `Subscribe` variant; the dispatcher gains per-subscriber
fan-out beside the serial exec queue.

## [2026-10-11] IPC disable knob (`daemon --no-ipc` / `[ipc] enabled`)

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/{main.rs,ipc}`

### Context & Description
IPC is always-on in v1. Minimal-image hardening may want the daemon
bound to nothing; when asked for, a flag/config key skips
`spawn_server` (the rest of the lifecycle is unchanged).

### Proposed Approach
A `#[serde(default = "default_true")]` `[ipc] enabled` plus a
`--no-ipc` override; documented as reducing `status`/`daemon stop` to
typed `no-daemon`.

## [2026-10-11] Multi-user / socket-path overrides for IPC

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/ipc/`

### Context & Description
v1's whole auth story is OS object permissions under the
single-user-per-db model (same trust class as the config/db/lock
files). A second-user story (shared db, distinct operators) or a
socket-path override needs a real design: identity on the socket
(peercreds), tokens, or both.

### Proposed Approach
Start from `interprocess`'s peer-credentials support on unix
(`local_socket::peer_creds`); pair an allow-list with the existing
0600 default; keep "local socket = the security boundary" until then.

## [2026-10-11] Client read deadlines / request cancellation over IPC

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/ipc/{client,server}`

### Context & Description
v1 has no client read deadline: every verb self-bounds
(`--timeout-secs`, engine receipts), and a wedged daemon is a
supervision problem (`status`/`daemon stop`). A hard client cap
interacts with user-set timeouts and with Ctrl-C cancellation of
in-flight requests — design them together, not ad hoc.

### Proposed Approach
One signal/cancellation design round covering: the REPL's line
abort, the daemon's second-signal escalation, and an IPC
`Cancel`/deadline extension to the v1 envelope.

## [2026-10-11] `logout` while a daemon runs: credential-liveness warning

- **Status**: ✅ DONE (implemented 2026-10-12, option (b) "act" —
  `logout` nudges the daemon to drop its connector — per
  `.artifacts/plans/2026-10-12-app-logout-daemon-liveness-plan.md`,
  branch `feat/app-logout-daemon-liveness`; the ADR 0012 amendment
  records the detach contract. Note: the "reuses the sync actor's
  shutdown path" sketch below was corrected in the plan — the
  `Shutdown` broadcast drains the whole engine; the mechanism is a new
  `SyncCommand::Detach` inbox command instead)
- **Category**: `UX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-11-m1-exit-wi5-daemon-ipc-frontend-plan.md` §7
- **Target Area**: `crates/taskboard-app/src/commands/{logout,status}`

### Context & Description
`logout` stays permitted while a daemon serves the database (documented
in ADR 0012): the daemon keeps its booted credentials until restart, so
sync may continue with "deleted" credentials. Honest, but surprising.

### Proposed Approach
A settings-work-round UX decision: `status` could surface
`credentials_revoked: true` (the daemon's store load now fails), or
`logout` could nudge the daemon to drop its connector — the latter is
the cleaner semantic and reuses the sync actor's shutdown path.

## [2026-10-12] `error_detail` payload for `CommandError::Config`

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-app-config-dx-followups-plan.md` §8 (decision D2-b)
- **Target Area**: `crates/taskboard-app/src/commands/mod.rs` (`error_detail`)

### Context & Description
WI-D's D2 change surfaces the specific `ConfigError` kinds in the
`--json` error envelope's `kind`, but `error_detail` stays `None` for
`Config(_)` — minimal-diff discipline while no consumer needed more.
The natural payloads already exist on the variants: the `FileNotFound`
path and the `InvalidDuration` field. `config print` remains the
path-bearing diagnostic surface meanwhile.

### Proposed Approach
Map `ConfigError::FileNotFound(path)` to the path string and
`InvalidDuration { field }` to the field name in `error_detail`; do it
when the first external `--json` consumer actually needs the detail
(rides the "[2026-10-10] Versioned `--json` schema" trigger), not
speculatively.

## [2026-10-12] `taskboard config path` diagnostic verb

- **Category**: `DX`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-app-config-dx-followups-plan.md` §8
- **Target Area**: `crates/taskboard-app/src/commands/config.rs`

### Context & Description
After WI-D's D1, read resolution (`AppConfig::config_path`) and the
login write target (`AppConfig::write_path`) differ only in the
no-config-anywhere arm, and that difference is observable today only
by running `login` and seeing where the file lands. An operator asking
"which file am I reading / where would `login` write?" has no
side-effect-free verb.

### Proposed Approach
A pre-core `taskboard config path` beside `config print` (same `Local`
IPC lane): prints the resolved read path and the login write target,
JSON and text; pure reuse of `config_path`/`write_path`, one dispatch
arm plus a small view. Natural companion to any future
connector-management verb round that touches the config verb family.
||||||| parent of a66b9a5 (docs(backlog): signal-death line-discipline restore (guard can't cover it))
## [2026-10-12] Restore the line discipline on signal death mid-REPL

- **Category**: `DX`
- **Originating Plan/Report**: Chat/Context (the tmux no-echo breakage, fixed by the `TtyStateGuard` in `run_session`)
- **Target Area**: `crates/taskboard-app/src/repl.rs`

### Context & Description
The `exit`/`quit`/EOF paths now restore the terminal line discipline on
session end. A process death by signal (SIGINT while a verb runs, or an
external SIGTERM/SIGKILL while the editor thread parks in `readline()`)
still cannot restore: no destructors run. The pane then stays in raw
mode (invisible input echo until `stty sane`), same symptom as the
fixed race. Out of scope for the guard change because a correct fix
needs an actual signal handler chain, not a drop guard.

### Proposed Approach
Register the restore in the signal path: a minimal `signal-hook`-style
handler (or tokio's ctrl_c/SIGTERM listeners, which the process already
arms for the daemon) performs one `tcsetattr(TCSADRAIN, saved)` before
exiting. SIGKILL stays unrestoreable by definition; document
`reset`/`stty sane` as the user recovery for that case.

## [2026-10-12] Age-based pruning of reminder deliveries

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.4 D-c (WI-I design round)
- **Target Area**: `taskboard-domain/`, `taskboard-storage-sqlite/`

### Context & Description
The reminders design keys at-most-once suppression on
`(rule_id, occurrence)` delivery rows. Task deletion cascades its
rules and their deliveries away, but a long-lived live task on a kiosk
with a moving due date accumulates one row per re-arm forever. Rows
are tiny, so this is housekeeping, not a leak emergency — deliberately
out of scope for the first implementation plans.

### Proposed Approach
A pruning rule in the delivery interpreter path (e.g. drop deliveries
whose occurrence is older than N days at record time), defined in the
domain's shared `apply_actions` so fakes and sqlite stay in lockstep;
pick N when a real deployment's table sizes justify the churn.

## [2026-10-12] Sync of reminder rules and delivery proofs

- **Category**: `Architecture`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.4 D-a (WI-I design round; user direction)
- **Target Area**: `crates/taskboard-domain/`, `crates/taskboard-sync-nextcloud/`

### Context & Description
Syncing reminder rules and delivery proofs across devices is a
**desired property**, deliberately deferred from the first
implementation plans. V1 ships local-only because the Deck connector
has no native reminder model — but that is a fact about one backend,
not the design: future connectors may support server-side reminders
natively, and even Deck could carry taskboard-private data through
custom card attachments. The v1 model is built sync-ready so this
entry is a protocol design round, not a refactor.

### Proposed Approach
Own design round when picked up. The sync-readiness commitments to
preserve (the plan's §15 review checklist): portable UUID rule ids;
deliveries/snoozes as occurrence-keyed additive rows whose
cross-device merge is set-union (conflict-free by construction);
per-field write clocks on rules added in the `TaskClocks` pattern; a
connector capability seam for native server-side reminders; the Deck
custom-attachment transport candidate (touches the outbox, merge
policy R1–R9, and the Deck attachments API). Decide whether snoozes
and mutes roam (they are per-device UX facts in v1).

## [2026-10-12] Reminder insistence levels (notification vs alarm)

- **Category**: `UI`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-reminders-design-plan.md` §6.1 (WI-I design round; user direction)
- **Target Area**: `crates/taskboard-domain/`, `crates/taskboard-app/`, `crates/taskboard-ui-slint/`

### Context & Description
Reminders should eventually carry an "insistence" flavor. **Normal**
(the default): OS notification popup + taskbar-icon blink + an
in-app notification window the user can dismiss or snooze. **Alarm**:
wake-up-alarm behavior — demand attention, play a pleasant but
insistent sound that gets louder the longer it is ignored, steal
focus to open the alarm window on top of everything, until the user
interacts (dismiss or snooze). Out of scope for v1; the architecture
was checked to keep it additive (plan §6.1).

### Proposed Approach
Additive `urgency` field on `ReminderRule` and the `Notification`
payload — the evaluator stays urgency-agnostic (it decides *when*,
never *how loud*); the sink port signature is unchanged. Escalation
(louder while ignored) is a UI/notifier rendering loop driven by the
un-dismissed notification's presence in state; dismiss/snooze ride
the existing `StateCommand` path regardless of urgency.

## [2026-10-12] Multi-reminder UX surfaces (N rules per task)

- **Category**: `UI`
- **Originating Plan/Report**: `.artifacts/plans/2026-10-12-reminders-design-plan.md` §4.1 (WI-I design round; user direction)
- **Target Area**: `crates/taskboard-ui-slint/`, `crates/taskboard-app/`

### Context & Description
Setting **multiple** reminders on the same task (e.g. "2 days before"
and "at due") is an eventual product want. The v1 domain model
already supports it (`BTreeMap<ReminderRuleId, ReminderRule>`, N
rules per task; cascade, evaluator, and notification correlation are
all N-capable by construction), but no v1 surface needs to expose it
and other parts of the system should *know* the count is unbounded:
UI must render rule lists (not "the" rule), CLI `remind --list`
returns a collection, and `--clear` removes N.

### Proposed Approach
When picked up: management surfaces over the existing shapes — an
"add another reminder" affordance in the task editor, per-rule
snooze/dismiss correlation via `ReminderRef`, no domain changes
expected beyond UI work.

## [2026-10-12] Raise the Miri pipeline/state case caps on a measured probe

- **Category**: `Testing` / `CI`
- **Originating Plan/Report**: WI-C review analysis of green run
  38312493624 (chat context; plan
  `.artifacts/plans/2026-10-12-ci-miri-tuning-plan.md`)
- **Target Area**: `.github/workflows/miri.yml`

### Context & Description
The tuned weekly Miri job uses the interpreter's budget very unevenly:
the state job finishes in ≈ 8 min and the properties job in ≈ 21 min
against the ≈ 90-minute headroom bar, while the pipeline job runs its
two properties at `PROPTEST_CASES=1`. Notably, the state crate's p2/p3
equivalence pins turned out cheap under the interpreter — 32
interpreted cases fit inside an ≈ 8-minute job — so the plan-time fear
of a ">9 min per case" state property described the gated live-engine
family, not these. More interpreted coverage is available for free;
the caps were sized from priors, not from the runner's actual curve.

### Proposed Approach
One probe dispatch per candidate: raise the state cap severalfold
(e.g. 32→64) and the pipeline cap to 2 (its cost dominator is ≈ 15 min
per interpreted case, so cap 2 projects ≈ 35–50 min), keep whatever
measures ≤ 90 min, record the justification in the moving PR per
`docs/testing_strategy.org` §10's cap policy. Diminishing returns —
worth bundling with other CI work rather than as its own item.

## [2026-10-12] Deterministic stamps for clock-gated fixture builders

- **Category**: `Testing`
- **Originating Plan/Report**: WI-C review analysis of green run
  38312493624 (chat context)
- **Target Area**: `crates/taskboard-domain/src/` (tests)

### Context & Description
Three of the four realtime-clock Miri gates added in WI-C are
avoidable: they skip tests whose *fixture construction* reads the
clock, not the logic under test. `test_support/contract.rs` stamps
five `SystemClock.now()` outbox entries (the only nondeterminism in an
otherwise fully fixed harness — it already has `clock_at(secs)`
helpers), `outbox.rs`'s serde roundtrip stamps `Uuid::now_v7()` ×3 +
`Utc::now()`, and `test_support/memory.rs`'s read-port test stamps one
`Utc::now()`. Under Miri's default isolation (no
`clock_gettime(REALTIME)`) each aborts, so the PendingOp serde
roundtrip, the read-port routing contract, and the whole repository
contract harness currently get zero interpreted coverage. The
remaining two gates (`clock.rs` SystemClock, `idgen.rs`
UuidV7Generator) are inherent — production code itself reads the
clock.

### Proposed Approach
Replace fixture stamps with fixed, distinct, strictly increasing
timestamps (distinct and ordered so any timestamp-ordering semantics
stay exercised): `contract.rs` routes its five stamps through a
`clock_at`-style helper — the `Clock` trait seam already exists in
`clock.rs`, and the state crate's test-common already ships a concrete
`FixedClock`); `outbox.rs`/`memory.rs` use fixed `Utc.timestamp_opt`
values and `Uuid::from_u128` ids. Delete the three gates; keep the
two inherent ones. All affected tests live in the cheap core job, so
there is no budget impact. Re-verify with one `workflow_dispatch` of
`miri.yml`.

---

## [2026-10-14] Deck round-trip strategy for app-only fields

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md` (§4.1, decision D1 — the keystone)
- **Target Area**: `taskboard-sync-nextcloud/` (spike), `taskboard-domain/` (mapping policy)

### Context & Description
A large family of planned fields — scheduled-start, effort estimate,
importance/urgency, recurrence parameters, reminder rules — have no
native Deck representation, so they would be local-only until a
round-trip vehicle exists. The brainstorm proposes two candidates:
Deck custom fields, or attachments as a sidecar store. The choice
determines whether these fields can sync on the day they land, and a
wrong late guess means a data migration. The spike is cheap (days)
and unblocks the whole "daily driver" wave of the roadmap.

### Proposed Approach
Investigation spike against the dockerized Nextcloud tier: survey the
Deck API surface for custom fields (availability, permissions, type
system, sync semantics), prototype read+write of one scalar field on
a card both ways, and evaluate the fallback sidecar (a structured
JSON document per board stored as an attachment, merged
set-union-style). Deliverable: a short report with a recommendation
plus the merge-policy implications for `apply_sync_report`.

---

## [2026-10-14] Scheduled-start field split from deadline

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/2026-10-14-future-directions-analysis-report.md` (§3.1; Wave 1)
- **Target Area**: `crates/taskboard-domain/`, `crates/taskboard-storage-sqlite/`, `crates/taskboard-state/`, `crates/taskboard-app/`

### Context & Description
The brainstorm asks to split the concepts of a deadline and a
scheduled start time/date. `Task` carries a single `duedate` today;
today/organiser views, the planner, and habit-style workflows all
want both dimensions. The org-mode analysis already notes org
`SCHEDULED:` lines have no domain field to map onto — this split is
the prerequisite.

### Proposed Approach
Mirror the existing `duedate` seams exactly: `scheduled:
Option<DateTime<Utc>>` + its field clock, `TaskChanges` sparse entry
(double-option like duedate), merge-policy arm, storage migration,
CLI `tasks edit --scheduled`, and due/scheduled markers in list
output. Deck round-trip per the D1 spike outcome (local-only is an
acceptable v1 if D1 says so, but decide knowingly).

---

## [2026-10-14] Scheduling scalar fields: effort, importance, urgency

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/2026-10-14-future-directions-analysis-report.md` (§3.1 + §4.1; Wave 1)
- **Target Area**: `crates/taskboard-domain/`, `crates/taskboard-storage-sqlite/`, `crates/taskboard-app/`

### Context & Description
The brainstorm wants an effort-estimation field ("how long do I
expect this to take") and an importance–urgency matrix (possibly
tags, possibly fields — the report's recommendation: dedicated typed
fields for semantics, labels only for free-form taxonomy). These
fields feed task-list decorations, the matrix visualisation, and the
future decision-support ranking. All are Deck-unrepresented, hence
gated on the D1 spike answer.

### Proposed Approach
One field-family plan: `effort: Option<chrono::TimeDelta>`,
importance/urgency as small enums (or 1–5 scores — decide in the
plan), each with its own clock, changeset entries, migrations, and
CLI flags; pure helper functions (matrix quadrant, derived priority
candidate) proptest'ed in the domain. UI decorations and the matrix
view follow in their own GUI work.

---

## [2026-10-14] Time tracking subsystem

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/2026-10-14-future-directions-analysis-report.md` (§3.3; Wave 2 anchor)
- **Target Area**: `crates/taskboard-domain/`, `crates/taskboard-storage-sqlite/`, `crates/taskboard-state/`, `crates/taskboard-app/`

### Context & Description
The brainstorm wants time tracking per task with good export, a
never-"done" tracker task type ("Reading as Hobby", "Meditation"),
per-user attribution for shared team decks, and respite marking that
counts tracked time as break activity. None of this exists; it is
the anchor feature of the roadmap's focus/reflection wave and the
prerequisite of break reminders, pomodoro mode, and review
statistics.

### Proposed Approach
New `TimeEntry { id, task, start, end, user?, kind flags }` entity
+ StartTracking/StopTracking/AdjustEntry commands + storage tables.
The *running* timer stays app-runtime state (persisted only as an
in-flight intent) so engine truth remains settled writes; a
clock-driven app task stamps the end. Export as pure projections
(JSON/CSV). Tracker-type tasks and respite flags are follow-on
field/flag work once entries exist. Deck round-trip via D1.

---

## [2026-10-14] Standalone timers & alarms without tasks

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/2026-10-14-future-directions-analysis-report.md` (§3.2; Wave 1)
- **Target Area**: `crates/taskboard-domain/src/reminders.rs`, `crates/taskboard-state/`, `crates/taskboard-app/`

### Context & Description
The brainstorm: since reminder/notification/alarm machinery exists,
why should the user need another app for a 3-minute tea timer, a
meditation minimum, or a wake-up alarm? `ReminderRef` is task-anchored
today (`Rule(id) | Default(task)`), so task-independent firing needs
a new anchor kind — the rest of the occurrence/snooze/sink pipeline
is reusable as-is.

### Proposed Approach
Extend `ReminderRef` with an `Adhoc(AdhocAlarmId)` arm; add a small
`AdhocAlarm { id, label, fire_at | duration, insistence? }` entity
with create/cancel commands and storage. CLI `timers set/list/cancel`
style verbs first; GUI/overlay surfaces later. Sounds and escalation
levels ride the reminder-flavours backlog entry.

---

## [2026-10-14] Recurring tasks & habits — domain design round

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/2026-10-14-future-directions-analysis-report.md` (§3.1 + decision D3; Wave 3 gate)
- **Target Area**: `crates/taskboard-domain/` (design), then all crates

### Context & Description
The brainstorm wants repeatable tasks/habits as first-class citizens:
marked-done shows in history and today's schedule, future scheduled
days show them active; plus consistent history per occurrence (time
tracking, stack transitions, done timestamps), standalone-vs-repeating
identification, and habit-formation assistance grounded in research.
The analysis rates this the hardest pure-domain problem in the
roadmap (recurrence rules, materialisation policy, history identity)
and gates the habit features on a dedicated design round.

### Proposed Approach
A design-round plan (no implementation): choose materialisation
policy — the report recommends biasing toward
materialise-on-completion + re-arming rule (matches the reminders
pipeline's occurrence philosophy and keeps history = real tasks);
define occurrence identity and its interaction with reminders, time
tracking, and sync (Deck round-trip via D1); proptest strategy for
recurrence arithmetic. Habit streaks, pleasant reminders, and
tips/motivation content build on the outcome.

---

## [2026-10-14] Subtask hierarchy & task split — design round

- **Category**: `Architecture`
- **Originating Report**: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md` (§3.1; Wave 3 gate)
- **Target Area**: `crates/taskboard-domain/` (design), then all crates

### Context & Description
The brainstorm wants tasks to have subtasks with arbitrary recursion
depth, a user-facing split-into-subtasks function, and parent-state
tracking derived from subtask state. Deck has no sub-card concept,
so sync semantics need deciding alongside the tree semantics. The
broader hierarchy question (Projects containing Boards, recursive
Projects) is deliberately excluded — it is blocked on multi-board
and belongs to that epic instead.

### Proposed Approach
A design-round plan: `parent: Option<TaskId>` model, acyclicity and
depth policy, move/cascade semantics (does moving a parent move
children?), derived parent-state rules (pure functions, proptest'ed),
split-command interaction with titles/descriptions/labels/duedate,
and the Deck round-trip story (local-only vs D1 sidecar). UI tree
rendering and the split interface follow implementation.
