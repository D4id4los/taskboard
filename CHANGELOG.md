# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The workspace carries a single global version (see `docs/architecture.org`);
`scripts/bump_version.sh <major|minor|patch>` cuts release entries.

## [Unreleased]

### Added

- Growth roadmap Phase 5 (ADR 0008): the `taskboard-app` bootstrap
  library — `AppConfig` (figment: defaults → `taskboard.toml` →
  `TASKBOARD_*` env, strict tables enforcing no-secrets-in-TOML), the
  `CredentialStore` port with `KeyringStore` (OS keyring, default) and
  `EnvCredentialStore` (`TASKBOARD_APP_PASSWORD` escape hatch,
  config-selected, never a silent fallback), the redacting
  `AppPassword` newtype, and `bootstrap()` wiring the full actor graph
  into an `App` handle with a graceful bounded shutdown sequence
  (`App::shutdown()`, `run_until_signal` with ctrl_c/SIGTERM). The
  `taskboard` binary is now lib+bin: `main.rs` is thin glue, the
  library is importable by tests and every later frontend.
- Docker-tier app tests (`it_nextcloud_docker_*` in `taskboard-app`):
  the daemon pull/push/shutdown journey against the dockerized
  Nextcloud — the automated Phase 5 exit demo; wired into
  `nextcloud-integration.yml` alongside the sync crate's tier.
- Growth roadmap Phase 4 (ADR 0007): the sync actor in
  `taskboard-sync-nextcloud` — `spawn_sync_actor` (scheduled push-first
  cycle: read via the `SyncStateReader` port → push → pull → report),
  the push executor (binding overlay, fetch-before-write, typed
  `DeckError` → `PushResult` classification), the pull assembler
  (three conditional reads, both stack listings, per-card detail
  refresh), the offline FSM (`NetworkLost`/`NetworkRestored`
  ownership, failure backoff, cycle-in-flight `SyncNow` dedupe,
  `SetBoard` consumption with binding-by-adoption), and the D-series
  two-way sync demonstration against dockerized Nextcloud (the phase
  exit).
- Domain additions feeding the above: `push::plan_pushes` (the pure
  push-coalescing planner with proptest partition/order laws), the
  `SyncStateReader` port (+ `EngineCommand::ReadState`, engine and
  fake implementations), `apply_push_report` (the pipeline's push-only
  composition for evidence-then-verdict `Failed` reports), the
  `SyncReport` reshape (`Completed { validators, read_at }`, `Failed {
  pushes, read_at }`), `SyncErrorKind::NoBoard`, and the storage
  codec's `ValidatorKey::ArchivedStacks` tag.
- The self-clobber guard (ADR 0007): sync reports carry `read_at`, and
  the merge/echo adoption protects fields covered by outbox ops queued
  after the cycle's read — a local edit landing between the cycle's
  state read and its push can no longer be silently wiped by the
  pull's whole-card stamps (observed end-to-end in the D6 docker
  scenario).
- Doc-test usage examples on `spawn_sync_actor` and `plan_pushes`.
- Growth roadmap Phase 3 (ADR 0006): the State Engine in
  `taskboard-state` — `EngineCore` (working state advancing only by
  interpreting persisted batches through the shared `apply_actions`),
  the `spawn_state_engine` actor loop (biased `select!` over
  `EngineCommand::{Execute, Flush}`, `SyncReport`s, and `SystemEvent`s),
  `EngineHandle` (`execute` receipts, fire-and-forget `dispatch`, the
  `flush` durability barrier, `subscribe`, `shared_state`),
  apply-before-interpret failure semantics (no rollback: memory never
  advances on storage failure), the never-persisted `Syncing` transient,
  and the `SyncNow` nudge contract — plus the S-series insta snapshot
  suite defining the engine's behavioral contract, the A-series routing
  tests, and the P-series architectural proptests.
- Domain additions feeding the above: `command::plan_command` (the full
  local-command semantics catalogue with typed `CommandError`
  rejections, idempotent no-ops, bound/unbound delete split, and the
  `DeleteStack` cascade), `persistence::apply_actions` (the port
  contract's batch semantics as one production function, now also
  delegated to by the `InMemoryRepository` fake), and the promoted
  `test_support::CountingIds` + `state_command_strategy`.
- `taskboard-storage-sqlite`: `impl TaskRepository for StorageHandle`
  (the port adapter letting the state engine drive the storage actor
  without a crate dependency).
- Growth roadmap Phase 2 (ADR 0005): the SQLite local persistence crate
  `taskboard-storage-sqlite` — normalized schema (`boards`/`stacks`/
  `tasks`/`labels`, `task_labels`, `outbox`, `sync_metadata`,
  `sync_status`) with embedded boot-time migrations, WAL +
  `synchronous=NORMAL` on a single connection, `SqliteTaskRepository`
  implementing the `TaskRepository` port with compile-time checked
  `query!` SQL (one transaction per batch, FK-safe fixed order, derived
  `pending_ops`, retained tombstones), the `StorageCommand` actor with
  reply-carrying `StorageHandle`, and the offline sqlx workflow
  (`scripts/sqlx-prepare.sh`, committed `.sqlx/`, `SQLX_OFFLINE` +
  freshness check in CI).
- Domain additions feeding the above: async contract harness
  (`assert_task_repository_contract_async`), `PersistenceAction::
  UpsertSyncStatus` (with derived-`pending_ops` parity in the
  `InMemoryRepository` fake).
- Growth roadmap Phase 1 (ADR 0004): the canonical domain model in
  `taskboard-domain` — `Board`/`Stack`/`Task`/`Label` entities with
  per-field write clocks, typed local (UUIDv7) and remote (Deck) ids with
  board-scoped composite refs, the real `AppState` with sync status, the
  inter-actor message payloads (`StateCommand`, `SystemEvent`,
  `EngineSignal`, `SyncCommand`, `SyncReport` + changesets), the outbox
  operation types (`OpId`, `LocalOp`, `PendingOp`), the async-shaped
  `TaskRepository` port (`BoxFuture`, no runtime dependency) with
  `RepositoryError`, `PersistedState`, and batched `PersistenceAction`s,
  the `Clock`/`IdGenerator` seams, and the pure sync conflict policy
  (`apply_sync_report` + R1–R9 per-entity merge primitives) with a
  property-test suite (idempotence, strict-newness tie-break, clock
  monotonicity, convergence, no-panic bombardment).
- Non-default `test-support` cargo feature on `taskboard-domain`:
  property strategies for all entities and remote views, an
  `InMemoryRepository` fake, and the repository contract harness
  (`assert_task_repository_contract`) that Phase 2's sqlite adapter will
  re-run verbatim.
- ADR 0004 "Sync conflict policy" documenting the per-field LWW model,
  tie-to-local bias, deletion rules (timestamped soft deletes for
  boards/stacks/labels, delete-wins fallback for cards), unconditional
  echo adoption, and the engine-executes-merge split.

### Changed

- The `taskboard` binary now accepts `daemon` (subcommand), a global
  `--config <path>`, and `--version`; no arguments still runs the
  daemon, per `[app] mode` (ADR 0008).
- The State Engine's `Shutdown` semantics: the engine no longer exits
  immediately — it enters a shutdown drain (new work declined with
  `EngineGone`, flush still answers) and keeps ingesting sync reports
  until the reports channel closes, so the sync actor's final cycle
  evidence lands before the process exits (the graceful-shutdown half
  of the duplicate-create window, ADR 0007/0008).
- `DeckClient`'s `Debug` impl redacts the credentials field (a
  `?client` log line used to print the app password).

### Fixed

- Deck client wire-model fixes found by the live tiers: explicitly `null`
  collections (e.g. a stack's `cards` once empty) decode as empty vectors;
  card `labels` accept both bare ids and inlined label objects (writes
  re-emit ids); `Participant` reads accept an id string or a full object,
  and writes re-emit the id string (Deck's card PUT requires the owner as
  the bare user id); `reorder_card` no longer decodes the response body
  (Deck versions return a card object, an array, or nothing) and returns
  `()`, reading back via `card()` when needed.
- CI: coverage job uses `cargo llvm-cov nextest` (the `--nextest` flag was
  removed in llvm-cov 0.9) with `--no-tests=warn`; benchmark job runs plain
  `cargo bench` (`--output-format bencher` is a nightly-only libtest option).
- CI: use the valid `dtolnay/rust-toolchain@stable` ref (the previous `@v1`
  ref does not exist and would have failed every toolchain job on first run).
- CI: mutation-testing workflow reports surviving mutants via artifacts
  instead of failing the scheduled job.
- `LICENSE-MIT` copyright holder corrected to the project author; repository
  URL placeholder (`your-username`) replaced with the real remote in
  `Cargo.toml` and the README badge.
- CI: SonarCloud job is skipped for Dependabot PR runs (they receive no
  repository secrets, so the scan could only fail with "Not authorized");
  main-push analysis remains the authoritative record.
- CI: replaced the deprecated `SonarSource/sonarcloud-github-action@v5`
  with its successor `SonarSource/sonarqube-scan-action@v8`.
- `scripts/bump_version.sh`: the changelog promotion no longer corrupts
  `### ` subsections (stray `#` lines, H2-ified `### Fixed`), nests the
  promoted section under Unreleased, or crashes on an empty Unreleased
  section — reimplemented on `scripts/changelog.py` with golden self-tests
  wired into the CI `check` job.

### Added

- Conditional reads in the Deck client (`taskboard-sync-nextcloud`):
  `fetch_boards`/`fetch_stacks`/`fetch_card` take opaque `Validators`
  (ETag / `Last-Modified`, re-emitted verbatim) and return `Fetch<T>`;
  `304 Not Modified` surfaces as `data: None` — mapped before any body
  decoding, never an error. Tier 1 pins the 304-with-empty-body contract
  and a mocked fetch→304→mutate→refetch cycle; Tier 2/3 gain a real
  `conditional_read_cycle`.
- Deck client write surface in `taskboard-sync-nextcloud`: board
  update/restore/clone (sparse `BoardChanges`, all-false-default
  `CloneOptions`), stack create/update/delete, card create/update (full
  round-trip)/delete/archive/reorder-move/label assign-remove, and label
  create/update/delete — every `PUT`/`POST` body pinned by contract tests.
  Tier 2/3 gain a `full_tree_lifecycle` live suite behavior and the fixture
  harvest example now builds the same run-id tree and writes per-resource
  `*_live.json` fixtures.
- Deck client read surface in `taskboard-sync-nextcloud`: full wire model
  (`Board`, `Stack`, `Card`, `Label`, `Acl`, `Participant`, `Attachment`,
  `BoardPermissions`, `StackFilter`) with typed read endpoints for boards,
  stacks (active/archived), cards, labels, and attachment metadata; ISO-8601
  `duedate`/`done` decode via chrono. `DeckError` gains `BadRequest`,
  `Conflict`, and `PreconditionFailed` for a complete typed status matrix;
  Tier 0 fixtures and Tier 1 contract tests cover every endpoint.
- `DeckColor` validated type in `taskboard-sync-nextcloud`: six hex digits
  (no `#`), case-normalized on construction, proptest-validated; reads stay
  lenient so a malformed server color cannot fail a whole listing. The
  `create_board` color parameter and `Board.color` now carry the type.
- Nextcloud sync testing build-out: `DeckClient` boards slice (list/create/
  delete over the Deck OCS REST API) with typed errors and bounded retries,
  plus all four test tiers — Tier 0 fixtures, Tier 1 `wiremock` contract
  tests (hermetic, default suite), Tier 2 secretless dockerized Nextcloud
  (`scripts/nextcloud_it_setup.sh`, new `Nextcloud Integration (docker)` CI
  job), and Tier 3 live-server tests loaded from a git-ignored `.env`
  (`.env.example` template; `examples/harvest_deck_fixtures.rs` refreshes
  the recorded fixtures).
- Testing strategy: tiered integration-test model for the Nextcloud sync
  client (Deck OCS REST API) — recorded fixtures, `wiremock` contract
  tests, a secretless dockerized-Nextcloud tier in CI, and a local-only
  live tier against a private server; no Nextcloud secrets exist in CI by
  design, so the server address cannot leak into public CI logs.
- SonarCloud static analysis: `sonar-project.properties` and a
  `SonarCloud Analysis` CI job (token via the `SONAR_TOKEN` secret).
- Binary release pipeline: tag-triggered (`v*`) matrix workflow building
  `x86_64-unknown-linux-gnu` (tar.gz) and `x86_64-pc-windows-msvc` (zip)
  archives plus SHA256 checksums, publishing a GitHub Release whose body
  is the version's CHANGELOG section; `workflow_dispatch` dry-run mode
  for pipeline validation without a Release.
- `scripts/package_release.py`: shared packaging script used identically
  by CI and locally (byte-identical artifacts) — builds `taskboard-app`,
  stages binary + licenses + README + changelog into `target/dist/`,
  archives, and writes checksums; `--check` verifies existing archives.
- `scripts/changelog.py`: Keep-a-Changelog tooling (`promote`, `extract`,
  `self-test`) used by `bump_version.sh` and the release workflow.
- Bencher continuous benchmark tracking: the `benchmark` job records
  criterion results from `main` pushes via the Bencher CLI
  (`BENCHER_API_TOKEN` secret); tolerant until first benches exist.
- Cargo workspace with six crates: `taskboard-domain`, `taskboard-state`,
  `taskboard-sync-nextcloud`, `taskboard-storage-sqlite`,
  `taskboard-ui-slint`, `taskboard-app` (edition 2024, single global semver).
- Dual-tier licensing: `MIT OR Apache-2.0` for library crates,
  `AGPL-3.0-only` for application/UI crates.
- CI pipeline (fmt/clippy, cargo-deny, nextest + doc tests, llvm-cov
  coverage, criterion benchmarks) plus weekly mutation-testing and Miri
  workflows.
- Project documentation: `docs/architecture.org`,
  `docs/coding_guidelines.org`, `docs/testing_strategy.org`, and ADRs
  0001–0003; `README.org` and `AGENTS.md` agent guidance.
- Release tooling: `scripts/bump_version.sh`, `CHANGELOG.md`.
- Repository hygiene: `.githooks/pre-commit` (fmt + clippy), Dependabot
  config, issue & PR templates, `.editorconfig`, `.gitattributes`.
