# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
The workspace carries a single global version (see `docs/architecture.org`);
`scripts/bump_version.sh <major|minor|patch>` cuts release entries.

## [Unreleased]

### Added
- Local board editing: `boards color <hex>`, `boards archive`,
  `boards unarchive`, `boards delete [--confirm]`, and
  `boards reset [--confirm]` (#132, #171, #177). Edits stamp per-field
  clocks and sync through the existing full-send board PUT; delete
  cascades locally and pushes one board delete; reset is a local-only
  repair that also clears sync validators so the next pull
  re-materializes. The destructive verbs ask for `--confirm` and
  print what would be destroyed otherwise.
- GUI stack creation on the live board (#304): a "New stack" action
  (standard preset `S`, vi preset `O`) opens the inline create row on
  an empty or populated board and appends the stack after the
  existing ones.

### Changed
- A bare `taskboard` launch in `daemon` mode (the default) without a
  terminal but with a graphical session (a file-manager double-click)
  now boots the desktop UI instead of silently starting a headless
  daemon; ssh/systemd/cron contexts (no TTY, no display) keep the
  daemon lifecycle, and the explicit `daemon` subcommand stays the
  headless entry (#236).
- The engine inbox envelope enum re-exported by `taskboard-state` is
  renamed to `EngineEnvelope` — a mechanical rename so the type name
  matches the payload/envelope vocabulary the docs already use; no
  variant, field, or behavior change (#302).
- The `connect` ceremony and daemon bootstrap warn once per process
  when the Nextcloud server URL is cleartext `http`, and the connect
  receipt records a `cleartext_transport` flag (#261).
- Release builds enable overflow-checks: arithmetic overflow in the
  shipped binary panics loudly instead of wrapping silently (dev and
  test builds already checked) (#303).

### Fixed

- A push raced by another client's move no longer loses the local
  intent or re-adopts the card as a twin: a 404 at the stale stack
  binding re-resolves the card once against the fresh listings and
  retries at the fresh location — update/move re-fetch before
  classifying (#258), delete retries at the fresh stack (#319); a
  card absent from every stack still converges `RemoteMissing`, so
  the local tombstone stays correct (ADR 0007).
- Label-delta outcomes on task updates are decided per operation: a
  failing `assignLabel`/`removeLabel` sub-call no longer completes
  the primary edit and its other label ops with the primary's
  result — ops whose write landed stay `Applied`, the failing
  label's ops carry the classified outcome per op, and an
  unresolvable label reaches `Rejected { LocalData }` instead of
  being silently shadowed (#310, #314).
- A stray shared reference to the running core no longer panics the
  daemon mid-shutdown: the shutdown tail logs the advisory refcount,
  runs the ownership-free salvage steps (engine flush, `Shutdown`
  broadcast), and exits through the regular error path (#257).
- Server URLs carrying credentials (`https://user:password@host`)
  are rejected with a typed error at config validation and client
  construction, instead of validating and carrying the credential
  through resolved-config logs and the client's base URL (#260).
- Files taskboard creates are owner-only (`0600`): the SQLite
  database with its WAL/SHM siblings, the daemon lock sidecar, and
  the config file rewritten by `connect` and `logout`; pre-existing
  permissive copies are tightened on the next run (#297).
- REPL history is written through taskboard's own `0600` writer
  instead of rustyline's save, whose process-global umask window
  could strip permission bits from files sibling threads created
  concurrently (#289).
- The state engine exits when every input source is closed: the run
  loop no longer pins its own system-event sender, which had made
  the all-sources-closed exit structurally unreachable (#316).

## [0.1.0] - 2026-10-16

### Added
- The desktop GUI (ADR 0015): `mode = "desktop"` boots a Slint
  application — kanban board, stack detail, and task detail views
  driven by the same headless core as the CLI (lock-free state reads;
  no business logic in the UI). Bare `taskboard` routes by the
  configured app mode.

- Keyboard-first operation: every verb is an action with a rebindable
  keybinding — `standard`, `emacs`, and `vi` presets plus per-action
  overrides under `[ui.keymap]` (invalid overrides are dropped with a
  startup warning, never blocking); a generated action menu lists the
  current context's verbs with their key hints.

- Display scaling and responsiveness: `[ui] scale` (0.5–3.0) applies
  as a live token multiplier, with `SLINT_SCALE_FACTOR` as the OS-DPI
  escape hatch; unscaled responsive breakpoints (compact below 720 px,
  regular, expansive at 1440 px and up) degenerate the board for
  narrow windows.

- Sync status and overdue banner: a persistent status-bar sync
  indicator, and an overdue-task banner whose snooze/mute actions ride
  the reminders command path.

- Accessibility and quality machinery: a WCAG contrast-checked dark
  theme, screen-reader labels on every control, and a headless
  keyboard-journey test suite that keeps every core verb within its
  keypress budget.

- REPL completion disambiguates hard title collisions: when two live
  stacks, tasks, or labels share a title, the menu replaces the
  (guaranteed-`Ambiguous*`) title row with one `title - uuid` row per
  entity that inserts the full uuid; unambiguous titles keep the plain
  row.

- File-backed REPL history: submitted session lines persist in
  `<data-dir>/taskboard/history` (0600, following the db-path data-dir
  precedent) and are recalled in later sessions; persistence is
  best-effort — a missing, corrupt, or unwritable history file degrades
  to in-memory recall with a warning and self-heals on the next
  recorded line, and prompted ceremony answers (secrets, confirms)
  never enter it.

- `logout` detaches a running daemon's connector (WI-H, ADR 0012
  amendment): after the local logout succeeds, the client best-effort
  asks the daemon serving the same database to drop its sync actor —
  the booted credential dies in memory and the daemon keeps serving
  local edits connector-less (not a standalone flip; the config is
  never rewritten). The nudge never changes the exit code: the receipt
  gains the optional `daemon_connector` field, degradations are stderr
  warnings, and no daemon at all is no warning. The verb is additive
  on the protocol-v1 wire (recorded policy: additive variants never
  bump; shape changes do), matched against the daemon's booted
  identity (`identity-mismatch` otherwise), and idempotent. Honesty
  riders: `sync` on a detached engine refuses typed
  `sync-unavailable` instead of lying via `sync-timed-out`, and
  daemon-side `status` reports `connector_attached` so a detached
  daemon no longer masquerades as a syncing one.

- REPL tab-completion (WI-G): inside the interactive session, Tab
  completes verbs, subverbs, long flags, and the live stack/task/label
  titles at the cursor — including the attached `--stack=<Tab>` value
  form and quoting of multi-word titles. Candidates are derived from
  the static clap surface plus a lock-free `ArcSwap<AppState>` read on
  the editor thread (strictly read-only), parity the selector
  semantics; the in-session-refused verbs (`daemon`, `login`,
  `logout`, `config`) and `--config` are never offered.

- Daemon IPC (ADR 0012, WI-5): while `taskboard daemon` runs it serves
  a per-database local socket, and every core verb (`tasks`/`stacks`/
  `labels`, `boards select/create/rename/publish`, `sync`) is
  transparently served over it — no per-command core boot, no
  `instance-locked` friction; without a daemon the invocation boots
  in-process exactly as before (byte-identical output, silent
  routing). New operator verbs: `taskboard status [--json]` (live
  daemon/backend/pid/sync view, or the lock holder pid — never boots a
  core, never triggers a cycle) and `taskboard daemon stop` (graceful
  remote shutdown; typed `no-daemon`, exit 1, when none serves). A
  versioned wire envelope (protocol v1) crosses the socket; the
  systemd user-unit template ships in `contrib/taskboard.service`.

- The interactive REPL frontend (ADR 0011): bare `taskboard` with a
  TTY on stdin opens a parted-style session over one booted core —
  each line is one CLI invocation (`shell-words` quoting, per-line
  `--json`), `exit`/`quit`/Ctrl-D shut the core down gracefully, and
  failed lines never abort the session. `daemon`, `login`, and
  per-line `--config` are refused in-session; the per-database
  instance lock keeps the session and the daemon mutually exclusive.

- The full CLI editing surface (WI-3): `tasks edit/delete/label/
  unlabel`, `stacks rename/delete/move`, `labels list/add/edit/
  delete`, and `boards rename <title>` (title-only, both backends —
  the board title pushes when the board is bound and rides the next
  `boards publish` otherwise). `boards rename` carries
  `BoardClocks.title` (migration 0002, epoch default: pre-existing
  rows keep the remote-wins merge behavior bit-for-bit) and the
  field-wise board-title merge (ADR 0004 amendment).

- The cascading task selector: exact uuid → unique ≥8-char uuid
  prefix → exact title; ambiguity is the typed `ambiguous-task` error
  listing candidate titles.

- `logout`: deletes the stored secret and — under the auto backend —
  strips the config's server/user keys (an explicit
  `backend = "nextcloud"` is never rewritten; ADR 0010).

- `config print`: a decode-only diagnostic that always prints; the
  payload carries `valid` plus the typed `invalid_kind` (exit 0 unless
  the file is undecodable).

- XDG config discovery: `--config` > `TASKBOARD_CONFIG` >
  `$XDG_CONFIG_HOME/taskboard/taskboard.toml` (when present) >
  `./taskboard.toml`.

- The `StorageFailed` system event: edge-triggered at the state
  engine's storage-failure reply site (one event per failure epoch);
  the daemon logs it, the Phase 7 UI banner will consume it.

- The J5 tier-2 journey (`it_nextcloud_docker_cli_editing_push`): the
  editing surface lands server-side after one sync.

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

- The reminders subsystem (ADR 0013): due-anchored reminder rules on
  duedated tasks, evaluated by the state engine's tick (the first tick
  is immediate, so reminders left undelivered before a shutdown fire
  on the next boot) with durable-then-notify delivery — delivery
  markers persist before any notification is sent, and a failed batch
  fires nothing. Every duedated task carries an implicit default
  reminder at the configured offset; explicit rules replace it.

- `tasks remind`: set or replace a task's reminder with `--before`,
  list the board's rules with `--list`, remove them with `--clear`,
  and mute/unmute a task's firings with `--off`/`--on` (the overdue
  banner is unaffected by mute).

- The `[reminders]` config table: `enabled`, `tick_secs`, and
  `default_before` (validated; a bad duration is a typed
  `invalid-duration` rejection).

- Reminder delivery v1 logs one line per notification through the
  `NotificationSink` port (`LogSink`); desktop toasts are a follow-up
  behind the same port.

### Fixed
- The interactive REPL no longer leaves the terminal in raw mode when
  the session ends via `exit`/`quit`: the session saves stdin's line
  discipline at boot and restores it on exit (previously, a shutdown
  racing the editor thread's fresh `readline()` could exit the process
  mid-raw-mode, and the pane's shell then read input without echoing
  it — invisible typing until `stty sane`). A signal kill
  mid-`readline` remains unrestoreable (no destructors run).

- Label assignment on an *existing* card now actually lands: Deck 1.19
  rejects card-update PUTs carrying a non-empty `labels` field (HTTP
  400), so the push executor strips the label list from the field PUT
  and applies the desired set through the dedicated
  `assignLabel`/`removeLabel` sub-endpoints (tier-2 verified by the J5
  journey; previously the op dead-lettered as `bad-request`).

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

### Changed
- The daemon no longer refuses the `standalone` backend (ADR 0010
  decision 9 amended): it owns the core and serves commands over IPC
  under both backends; under `standalone` the sync actor is not
  spawned at all and no credential store is consulted.

- `taskboard <core verb>` now prefers a running daemon (served over
  its socket) and boots in-process only when none answers.

- `tasks done/move` accept prefixes and exact titles (exact uuid stays
  first, so existing scripts keep working); task resolution now
  happens in-core against the published state.

- The `MutationReceipt` JSON payload gains the additive `labels` key
  (the attached label titles after the mutation).

- Growth roadmap Phase 6b (ADR 0010): *stand-alone-first* — the app
  works fully stand-alone with no server configured, and Nextcloud sync
  becomes the first optional connector selected by `[sync] backend =
  "standalone" | "nextcloud"` (absent = auto: `nextcloud` iff
  `[nextcloud] server_url` is present, so a half-configured file stays
  loud). Fresh installs boot and author with zero network and zero
  keyring access; existing `[nextcloud]` configs behave exactly as
  before.

- `boards create <title> [--color <hex>]` and a `stacks` group with
  `stacks add <title> [--order <n>]`: local board/stack authoring
  (the engine's `CreateBoard`/`BindBoard` domain commands, no outbox
  op for board creation; `stacks add` appends by default).

- The attach ceremonies for migrating stand-alone data onto a remote
  board: `boards publish` (create a fresh Deck board from the local
  one, bind, and let the first cycle drain the outbox) and
  `boards select --bind-local-board` (bind onto an existing board;
  union-by-identity semantics — see ADR 0010).

- The typed `sync-disabled` refusal (`--json` kind `sync-disabled`,
  exit 1) for the connector-only verbs — `sync`, `daemon`, `boards
  list/select/publish`, and `login` under an *explicit* `standalone`
  backend — enforced at handler entry before any core boot or client
  construction.

- The human `tasks list` stand-alone footer (`stand-alone: N changes
  waiting for a sync connector`); presentation-only, the `--json`
  payload is unchanged.

- The J4 tier-2 journey (`it_nextcloud_docker_cli_standalone_publish`):
  stand-alone authoring → login → publish → server-side verification
  → local ids preserved.

- Bare `taskboard` with a TTY on stdin now opens the interactive REPL
  instead of resolving `[app] mode` (the daemon); non-TTY invocations
  are unchanged, and every explicit subcommand behaves exactly as
  before.

- Fresh installs default to the `standalone` backend: `[sync]
  backend` is optional and `[nextcloud] server_url`/`username` are
  required only when the resolved backend is `nextcloud` (a present
  but malformed value is still a loud typed rejection). Configs with
  `[nextcloud] server_url` are unchanged (auto resolves `nextcloud`).

- Growth roadmap Phase 6 (ADR 0009): the operator CLI over the
  in-process core — `login` (verify-then-persist: flags/prompted
  server+user, app password via `rpassword` on a terminal or piped
  stdin, non-secrets overlaid into `taskboard.toml` comment-preserving
  via `toml_edit`, the secret into the credential store; the env store
  reports `stored: "env"` with nothing written to any file), `boards
  list/select` (discovery plus the one-time single-board binding with
  pre-validation, rebind refusal, and an adoption wait), `tasks
  list/add/done/move` (exact selectors, append-by-default orders,
  `NoOp` idempotency), and `sync` (bounded wait, honest
  Succeeded/Failed/TimedOut outcome, `--no-wait`). A global `--json`
  flag switches every command to typed JSON payloads on stdout with
  kebab-case error envelopes on stderr; exit codes are 0 success /
  1 runtime failure / 2 clap usage / 130 second-signal force exit.

- The per-database instance lock: an OS advisory exclusive lock on a
  `<db>.lock` sidecar (fd-lock), acquired in `bootstrap()` before the
  database opens and held for the process lifetime — a second core on
  the same database fails with the typed `InstanceLocked` (db path +
  holder pid), machine-distinguishable as `instance-locked`; released
  automatically on process death.

- The J-series binary-driving E2E tier (`it_nextcloud_docker_cli_*`):
  the real `taskboard` binary against the dockerized Nextcloud —
  login (stdin-piped password) → boards select → tasks add → sync
  with server-side push/pull verification, and the bad-token rejection
  journey; picked up by the existing `nextcloud-integration.yml`
  filter with no workflow change.

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

- `login` replaces the manual `secret-tool` dance of the Phase 5 M1
  quickstart: the ceremony verifies against the server before anything
  is persisted and stores the app password itself.

- The sync merge fixes a duplicate-adoption bug: a card created and
  pulled within one sync cycle was adopted twice (the create's echo
  binding was invisible to the pull's card-to-local matching map,
  which read pre-echo state), wedging subsequent update pushes.

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
