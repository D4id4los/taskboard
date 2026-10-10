# [Plan] Growth Roadmap Phase 5 — `taskboard-app`: Bootstrap Library, Config & Secrets (M1 Checkpoint)

- **Date**: 2026-10-10
- **Target**: `taskboard-app` (lib+bin split: `config.rs`, `secrets.rs`,
  `bootstrap.rs`, `cli.rs`, thin `main.rs`; new workspace deps `keyring`,
  `clap`, `dirs`), a small semantic addition in `taskboard-state` (engine
  shutdown linger-drain, §3.6), a hygiene fix in `taskboard-sync-nextcloud`
  (`DeckClient` Debug redaction, §3.7), root `Cargo.toml`
  `[workspace.dependencies]`, CI (`nextcloud-integration.yml` picks up the
  app crate's docker tier), and docs (ADR 0008).
- **Goal**: implement the roadmap's Phase 5 — refactor `main` into a
  reusable `bootstrap`: figment config → credential resolution → channel
  graph → actor spawns → a typed `App` handle, shared by the daemon mode
  now and every Phase 6 CLI subcommand later (the "in-process core"
  decision, roadmap §2.4). figment `Config` for
  `[nextcloud]`/`[storage]`/`[sync]`/`[app]` with **no secrets in TOML**
  (mechanically enforced), app-password secrets via the OS keyring (env
  escape hatch for headless/CI), and graceful signal shutdown with
  pending-op flush.
- **Status**: DRAFT (rev 1, awaiting user review)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 5. Exit criterion: "headless daemon runs against the real
  server" (M1 checkpoint).
- **Predecessors**: Phase 1 (domain, merged), Phase 2 (storage, merged),
  Phase 3 (engine, merged as `93ab5fb` + fixes), Phase 4 (sync actor,
  merged to `main` @ `0ef6d00` — the full series
  `3465d62..0ef6d00`). Authored against merged `main` @ `0ef6d00`; no
  reconciliation gate needed. Residual duty: re-verify the spawn
  signatures cited in §1 if anything lands on `main` before the branch
  starts.

---

## 1. Context & Known Facts (do not re-derive)

- **Working agreement (roadmap §2.5)**: later phases may reshape earlier
  crates without ceremony; the full test suite is the safety net. This
  plan exercises it twice: the engine gains shutdown drain semantics
  (§3.6) and `DeckClient` loses its secret-leaking derived `Debug`
  (§3.7). Both land in this phase's PR with their own tests.
- **The actor spawn surfaces are merged and final-for-now**:
  - `taskboard_state::spawn_state_engine(repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>, clock: Arc<dyn Clock>, sync_out:
    mpsc::Sender<SyncCommand>, sync_reports: mpsc::Receiver<SyncReport>,
    system: broadcast::Receiver<SystemEvent>) -> Result<(EngineHandle,
    JoinHandle<()>), EngineStartupError>`. Boot hydrates via
    `repo.load()` **before** the loop starts; `EngineStartupError::Load`
    is the only startup failure. The engine owns its command inbox
    (`EngineCommand::{Execute, Flush, ReadState}`), its
    `EngineSignal` broadcaster, and the `Arc<ArcSwap<AppState>>`.
  - `EngineHandle`: `execute` (typed receipt; a reply implies durable
    persistence), `dispatch` (fire-and-forget), `flush` (durability
    barrier — the CLI exit path), `subscribe`, `shared_state`, and
    `impl SyncStateReader for EngineHandle` (the sync actor's read port).
  - `taskboard_sync_nextcloud::spawn_sync_actor(client: DeckClient,
    state: Arc<dyn SyncStateReader>, commands: mpsc::Receiver<
    SyncCommand>, reports: mpsc::Sender<SyncReport>, system:
    broadcast::Sender<SystemEvent>, cfg: SyncActorConfig) ->
    JoinHandle<()>` with `SyncActorConfig { poll_interval,
    backoff_initial, backoff_max }` — its doc comment literally says
    "Knobs the Phase 5 bootstrap wires from figment's `[sync]` table".
    The actor **publishes** `NetworkLost`/`NetworkRestored` on the
    injected broadcast and **consumes only `Shutdown`** from it (its own
    subscription); it also exits when its command inbox closes. A cycle
    is never aborted mid-flight — `Shutdown` lands between cycles.
  - `taskboard_storage_sqlite::spawn_storage_actor(repo: Arc<
    SqliteTaskRepository>) -> (StorageHandle, JoinHandle<()>)`;
    `impl TaskRepository for StorageHandle` already exists (the port
    adapter). `open(path)` / `open_memory()` apply embedded migrations
    at boot (`OpenError::{Connect, Migrate}`); the pool is a single
    connection (WAL, `synchronous=NORMAL`, `busy_timeout` 5 s,
    `create_if_missing`). The actor loop exits when the last
    `StorageHandle` drops.
- **Engine loop discipline (matters for §3.6)**: biased `select!` —
  commands, then sync reports, then system events; `SystemEvent::
  Shutdown` **returns immediately** today. If the sync-report channel
  closes, the engine logs a warning and *keeps running* (it is a
  daemon). `SystemEvent::{NetworkLost, NetworkRestored, Shutdown}` is
  the whole broadcast vocabulary; no actor other than the sync actor
  ever produces system events today.
- **Sync-report evidence semantics (ADR 0007)**: `Completed`/`Failed`
  carry the pushes that landed (`Failed`) plus `read_at`; the engine
  ingests them through the merge pipeline. A report that is *sent* after
  the engine stopped is dropped by the actor with a `warn!` — its ops
  stay queued and re-push next boot. Deck has **no idempotency keys**:
  a create re-POSTed after a lost report duplicates the card server-side
  (the backlogged duplicate-create crash window).
- **Client construction**: `DeckClient::new(base_url: &str, user: &str,
  token: &str) -> Result<Self, DeckError>` — parses and normalizes the
  URL (`InvalidBaseUrl` on parse failure / non-http(s) / hostless),
  Basic auth `user:token`, `OCS-APIRequest: true`. **`DeckClient`
  derives `Debug` over a `credentials: String` field** — a
  `tracing::debug!(?client)` would print the app password (§3.7 fix).
- **`main.rs` today** (68 lines): `color_eyre::install`, `init_tracing`
  (fmt + `EnvFilter`, or `console-subscriber` behind the
  `tokio-console` feature), a placeholder `ArcSwap<AppState>`, a
  multi-thread runtime, empty `async_main`. The app crate is
  **binary-only** (`[[bin]] taskboard`); `tests/` cannot import from it.
  Its manifest already depends on every workspace crate plus `figment`,
  `color-eyre`, `tracing-subscriber`, `arc-swap`, `chrono`, `uuid`,
  `tokio`.
- **Workspace dep facts**: `tokio` features already include `signal`;
  `figment` is pinned with `toml` + `env` features; all shared versions
  live only in root `[workspace.dependencies]`. No
  `[target.'cfg(...)'.dependencies]` exist anywhere yet — this phase
  introduces the first (platform keyring features, §3.9). The tokio
  workspace feature list is shared by every crate; adding per-target
  deps does not disturb it.
- **Config precedence is a settled house rule** (AGENTS.md §4,
  architecture.org): defaults → `taskboard.toml` → `TASKBOARD_*` env.
  With figment's `Env::prefixed("TASKBOARD_").split("__")`, nested keys
  map as `TASKBOARD_NEXTCLOUD__SERVER_URL` → `nextcloud.server_url`.
  Tier-3 test env vars (`TASKBOARD_IT_NEXTCLOUD_*`) must coexist with
  the `Env` provider (§3.2 guard).
- **Backlog handoff (decided here)**: the backlog entry "[2026-10-08]
  Keyring fallback for headless Linux (no Secret Service)" says: "In
  the Phase 5 (app bootstrap) plan: wrap keyring access behind a small
  Port with two impls (keyring, encrypted-file), select via config, and
  document the trade-off. Never silently fall back to plaintext." This
  plan makes that decision (§3.3): port + config-selected store, with
  the *env-var* store as the second impl for M1 (the encrypted-file
  store stays backlogged). The roadmap's risk §5 item is thereby
  resolved.
- **Testing assets & rules**: tier 2 docker
  (`scripts/nextcloud_it_setup.sh up [--ci]` + env
  `TASKBOARD_IT_DOCKER_{URL,USER,TOKEN}`, filter
  `test(it_nextcloud_docker)`, run in
  `.github/workflows/nextcloud-integration.yml` — `workflow_dispatch` +
  weekly, deliberately out of the per-PR path); tier 3 live (`.env`:
  `TASKBOARD_IT_NEXTCLOUD_{URL,USER,TOKEN}`, filter
  `test(it_nextcloud_live)`, `#[ignore]` + env guard, **skip never
  fail**). Per-PR CI (ci.yml): fmt, clippy `-D warnings`, nextest
  `--workspace --all-targets`, doc tests, `cargo deny check`, llvm-cov
  nextest `--all-features`. House rules: no sleeps in tests (poll a
  predicate with a deadline), typed outcome assertions only (no text
  assertions on errors/logs), one behavior one test at the lowest
  layer, fakes over mocks, `#![forbid(unsafe_code)]` (app crate: on),
  SPDX `AGPL-3.0-only` headers in `taskboard-app`, `MIT OR Apache-2.0`
  in the touched library crates.
- **`deny.toml` license allow-list**: MIT, Apache-2.0, Apache-2.0 WITH
  LLVM-exception, BSD-3-Clause, ISC, Unicode-3.0, Zlib,
  CDLA-Permissive-2.0 (+ the workspace's own AGPL). `clap`, `dirs`,
  `keyring` and their expected transitives (windows-sys,
  security-framework, the Secret Service/zbus stack, RustCrypto
  primitives) are MIT/Apache-2.0-class; `cargo deny check` is the
  gate, run before handoff (§10).
- **Seeding seam for app-level tests**: the storage crate's public
  surface (`open` + `StorageHandle::apply`) can pre-seed a database
  with `PersistenceAction::UpsertBoard(Board { remote: Some(
  RemoteBoardId(n)), .. })` before `bootstrap()` opens it — the
  engine then hydrates a live board binding, and a `RequestSync`
  triggers a real cycle against a (wiremock or docker) server. `Board`
  carries `remote: Option<RemoteBoardId>`; board-scoped commands
  require exactly one live board with a remote binding.

## 2. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| App crate shape | lib+bin split: `taskboard_app` lib (modules below) + thin `main.rs`; `[[bin]]` name unchanged (§3.1) |
| Config | `config.rs`: `AppConfig` + section structs, figment provider chain (defaults → toml → env), typed `ConfigError`, validation, per-table `deny_unknown_fields` (mechanically no secrets in TOML), `SyncActorConfig` mapping, default db path via `dirs` (§3.2, §4.1) |
| Secrets | `secrets.rs`: `CredentialStore` port, `KeyringStore`, `EnvCredentialStore`, `credential_key` derivation, redacting `AppPassword`, typed `CredentialError` (§3.3, §4.2) |
| Bootstrap | `bootstrap.rs`: `bootstrap(config, credentials) -> App`; channel graph ownership; spawn order; partial-failure cleanup; `BootstrapError` (§3.4, §4.3) |
| Shutdown | `App::shutdown()` graceful sequence (flush → Shutdown broadcast → actor join bounded → engine join → storage join); engine linger-drain semantics in `taskboard-state` (§3.5–§3.6, §5); `run_until_signal` (ctrl_c + SIGTERM on unix) |
| CLI skeleton | `cli.rs`: clap `Parser` — `--version`, global `--config <path>`, optional subcommand `daemon`; `[app] mode` resolution with typed not-implemented for `desktop`/`kiosk` (§3.8) |
| Sync-crate hygiene | manual redacting `Debug` for `DeckClient` (§3.7) |
| Workspace deps | `keyring = 3` (platform features via target-specific deps), `clap = 4` (derive), `dirs = 6`; `rpassword` **deferred to Phase 6** (§3.9) |
| Tests | C-series (config), S-series (secrets), G-series (engine drain, state crate), B-series (hermetic bootstrap E2E: real figment + real sqlite + wiremock), docker daemon exit demo (`it_nextcloud_docker_*` in the app crate), tier-3 manual runbook (§7) |
| CI | extend `nextcloud-integration.yml` to also run the app crate's docker tier (§9) |
| Docs | ADR 0008; `architecture.org` app-crate bullet + channel-table ownership rows + dependency list refresh; CHANGELOG; backlog updates (keyring-fallback entry resolved-note, new appends §8); README M1 daemon quickstart (§9) |

**Out of scope** (Phase 6+ unless the agility doctrine demands a touch):
the `login`/`boards`/`tasks`/`sync` subcommands and `rpassword`
(Phase 6); the instance lockfile (Phase 6 — §3.13 notes the interim
multi-process stance); app-level E2E *driving the real binary* through
the CLI (Phase 6's new test tier); UI mounting and `[app] mode`
consumption beyond validation (Phase 7); OAuth2 (backlog); daemon IPC
(backlog); the encrypted-file credential store (backlog, §8); XDG
config discovery (backlog, §8); systemd/journald packaging (backlog,
§8); MSRV CI (existing backlog entry).

## 3. Design Decisions

1. **Lib+bin split of `taskboard-app`.** The crate grows a lib target
   (`src/lib.rs`, crate `taskboard_app`) exposing `config`, `secrets`,
   `bootstrap`, `cli`, and `shutdown`-facing items; `main.rs` becomes
   a thin binary (clap parse → color-eyre → dispatch). Rationale:
   integration tests and doc tests cannot import from a `[[bin]]`;
   Phase 6's subcommands all call the same `bootstrap` in-process; the
   lib keeps `#![forbid(unsafe_code)]` + AGPL headers. The lib is
   internal API (`publish = false` workspace) — no stability promises
   across crates. *Rejected*: keeping everything in `main.rs` modules
   (untestable from `tests/`, and Phase 6 would restructure it again).

2. **figment config: strict tables, typed errors, env nesting.**
   Provider chain: `Figment::from(Serialized::defaults(
   AppConfig::default()))` → `Toml::file(path)` →
   `Env::prefixed("TASKBOARD_").split("__")`. The toml *path* resolves
   as `--config` flag > `TASKBOARD_CONFIG` env > `./taskboard.toml`
   (an absent default-path file contributes nothing; an explicitly
   requested path that does not exist is a typed
   `ConfigError::FileNotFound` — checked before figment runs). Section
   structs carry `#[serde(deny_unknown_fields)]` **per table, not on
   the root**: an unknown key inside `[nextcloud]` (e.g. a `token`)
   fails decoding, while foreign top-level env profiles
   (`TASKBOARD_IT_NEXTCLOUD_*` → `it.nextcloud.*`) stay ignored — the
   tier harnesses set those in-process, so a root-level deny would
   break every tier test that also loads config. Required
   no-default fields (`nextcloud.server_url`, `nextcloud.username`)
   are `Option` in serde and checked by `AppConfig::validate()` —
   figment's dynamic error strings are never pattern-matched (AGENTS
   §5); decode failures collapse into one typed
   `ConfigError::Decode { source }` variant. Durations are plain
   `u64` seconds fields (`poll_interval_secs`, …) converted at the
   `SyncActorConfig` mapping — no string duration parsing, no new
   deps. Defaults: poll 30 s, backoff 5 s → 300 s (the values the
   `spawn_sync_actor` doc example already uses); `db_path` default
   `dirs::data_dir()/taskboard/taskboard.db` (parent dirs created at
   bootstrap; `~` is *not* expanded — documented). Unknown
   `[app] mode` values are decode errors; `desktop`/`kiosk` decode but
   fail at bootstrap with `BootstrapError::ModeNotImplemented` (§3.8).

3. **Credential resolution: a port, two impls, config-selected, keyring
   default — deciding the backlog entry.** `trait CredentialStore`
   (§4.2) with `KeyringStore` (production) and `EnvCredentialStore`
   (read-only escape hatch). Selection is explicit:
   `[nextcloud] credential_store = "keyring"` (default) `| "env"`. No
   silent fallback of any kind; on a headless box without a Secret
   Service the keyring store returns typed `CredentialError::Backend`
   whose `Display` names the env alternative (UX text, never
   test-asserted), and the operator opts into `credential_store =
   "env"` explicitly. The env store reads **`TASKBOARD_APP_PASSWORD`**
   (deliberately *not* nested-`__` shaped: it is a secret read by the
   secrets module, not config consumed by figment — different
   namespaces, visually distinct); it implements `store`/`delete` as
   `CredentialError::Unsupported`. `KeyringStore` keys entries
   `service = "taskboard"`, entry name = `credential_key(server_url,
   username) = "{username}@{host}"` — the server host is part of the
   identity, so changing `server_url` in config cannot silently reuse
   another server's password; Phase 6's `login` stores through the
   same pure function. `store`/`delete` ship now (thin, needed by
   Phase 6) even though no Phase 5 command calls them. Secrets are
   carried in a redacting `AppPassword` newtype (§4.2) — `Debug`
   prints `[redacted]`, no `Display`, access via `expose()`. *Rejected*:
   always-on env override beating the keyring (implicit precedence is
   exactly the "silent fallback" the backlog forbids); a plaintext/0600
   file store (worse than env for the CI use case; the encrypted-file
   store stays backlogged); deferring the port and calling `keyring`
   directly from bootstrap (CI and tier tests could not exercise
   resolution, and the headless error would be an untyped string).

4. **`bootstrap` owns the channel graph and returns an `App` handle.**
   One async fn (§4.3): validate config → resolve credentials → build
   `DeckClient` → `open` sqlite (parent dirs first) → spawn storage
   actor → wrap `StorageHandle` as `Arc<dyn TaskRepository>` → create
   the system broadcast (capacity 64), sync-commands mpsc (32),
   sync-reports mpsc (32) → spawn the state engine (injecting
   `UuidV7Generator`, `SystemClock`, the sync sender, the reports
   receiver, a system subscription) → spawn the sync actor (client,
   `Arc::new(engine.clone()) as Arc<dyn SyncStateReader>`, the command
   receiver, the report sender, the system **sender**, figment-mapped
   `SyncActorConfig`) → return `App`. Ordering rationale: the
   failure-prone pure steps (config, secret, client URL) run before
   anything spawns; the only failing spawn is the engine's boot
   hydration (`EngineStartupError::Load`), whose cleanup is dropping
   the storage handle and awaiting its join. Capacities: nudges are
   engine `try_send` fire-and-forget where "full" already means
   "coalesced" (ADR 0006), and reports are one-per-cycle — 32 is
   generous at kiosk scale. The `App` struct (§4.3) keeps the engine
   handle, the system sender, a retained sync-command sender clone
   (the Phase 6 `SetBoard` seam — keeping it open is also why actor
   shutdown keys on `Shutdown`, never on inbox close), the storage
   handle (dropping it is part of shutdown), and both join handles.
   `bootstrap` is generic over nothing: `Arc<dyn CredentialStore>` is
   the seam tests inject (the fake: a `HashMap`-backed store, fakes
   over mocks).

5. **Graceful shutdown is an explicit sequence on `App`, signals are a
   thin wrapper.** `App::shutdown()` (§5) implements: engine `flush()`
   → broadcast `Shutdown` → await the sync actor's join with a bounded
   timeout (`[app] shutdown_timeout_secs`, default 10; on timeout,
   `abort()` and continue) → await the engine join (bounded; abort as
   last resort) → drop the storage handle and await its join (the
   storage actor drains its inbox and exits) → return a
   `ShutdownOutcome { actor_aborted }`. `run_until_signal` (unix:
   `ctrl_c()` **or** SIGTERM — container stop and systemd send
   SIGTERM; windows: `ctrl_c()`) awaits one signal and calls
   `shutdown()`; a **second** signal during the drain force-exits
   (standard daemon UX; one `select!` in `main`, deliberately
   untested-thin). Roadmap wording "pending-op flush" maps to: the
   engine flush (every accepted command durable), plus §3.6's
   linger-drain (every completed push cycle's evidence landed) — the
   *outbox itself is durable by construction* (Phase 2); shutdown
   never blocks on the network beyond the bounded actor timeout.
   *Rejected*: attempting a final sync cycle at shutdown (unbounded
   network latency in the shutdown path; the outbox makes it
   unnecessary); dropping `App` without an explicit sequence (joins
   would dangle and the storage actor could be killed mid-transaction
   on runtime drop).

6. **Engine linger-drain: `Shutdown` stops new work, not report
   ingestion (the one cross-crate semantic change).** Today the engine
   returns immediately on `SystemEvent::Shutdown`; a sync actor
   finishing a push cycle concurrently would send its report into a
   closed engine — the evidence is dropped, and next boot re-POSTs
   those creates (duplicates: Deck has no idempotency keys). New
   semantics in `taskboard-state`: on `Shutdown` the engine enters
   **drain mode** — it answers every subsequent `EngineCommand::
   Execute` with `Err(ExecuteError::EngineGone)` and every `Flush`
   with an immediate reply (FIFO: the pre-drain prefix is already
   applied, so the barrier semantics hold), while **continuing to
   ingest `SyncReport`s** (full pipeline: apply → interpret → publish)
   until the reports channel *closes* — which happens exactly when the
   sync actor task exits, because it owns the sender. Then the engine
   returns. Bootstrap ordering (§5) makes this safe and bounded: the
   actor is joined (or aborted, which also drops the sender) before
   the engine join is awaited, so drain always terminates; a hung
   engine is still covered by the bounded engine join + abort. This
   closes the *graceful-shutdown* half of the duplicate-create window;
   only the hard-crash half remains (existing backlog entry).
   *Rejected*: broadcasting shutdown only to the actor and stopping the
   engine by closing its inbox (the broadcast channel is shared by
   design — ADR 0001 — and the engine's own command inbox must stay
   open for the drain replies); accepting the lost-report window and
   backlogging it (evidence-then-verdict exists precisely to avoid
   duplicate re-POSTs; the fix is engine-local and testable — G-series).

7. **`DeckClient` Debug redaction (sync-crate hygiene).** `DeckClient`
   currently derives `Debug` over `credentials: String` (the
   `user:app-password` pair); any `?client` in a log line leaks the
   secret. Replace with a manual `Debug` that prints every field except
   a `credentials: "[redacted]"` marker. The regression test asserts
   the formatted output of a client built with a known password does
   not contain it — a leak *floor* on sensitive output, in the same
   sanctioned class as garbage-input ceilings (AGENTS §5 litmus: code
   getting strictly better keeps it passing), not a message-text pin.
   The same pattern guards `AppPassword` (§4.2).

8. **clap skeleton: one subcommand, config-driven default.** `Cli`
   (§4.4): `--version` (workspace version), global `--config <path>`,
   `#[command(subcommand)] command: Option<Command>` with the single
   variant `Daemon`. `None` resolves `[app] mode`: `daemon` (the
   default) runs the daemon; `desktop`/`kiosk` return
   `BootstrapError::ModeNotImplemented` (Phase 7 replaces the arm, the
   error type stays). Phase 6 adds `Login`, `Boards`, `Tasks`, `Sync`
   variants on this enum without touching `main`'s dispatch shape.
   *Rejected*: deferring clap to Phase 6 (roadmap lists it here;
   `--config`/`--version` are wanted now; Phase 6 would restructure
   `main` a second time); making `daemon` the clap *default* without
   `[app] mode` (the config key is roadmap-mandated and Phase 7 needs
   it to select the UI shape).

9. **New workspace dependencies, pinned once, platform features via
   target-specific deps.** Root `[workspace.dependencies]`: `clap =
   { version = "4", features = ["derive"] }`; `dirs = "6"`; `keyring =
   { version = "3", default-features = false }`. The app manifest adds
   the first target-specific deps in the workspace:
   `[target.'cfg(target_os = "linux")'.dependencies] keyring = {
   workspace = true, features = ["sync-secret-service", "crypto-rust"]
   }` (synchronous Secret Service with the pure-Rust crypto backend —
   no openssl in the graph), `[target.'cfg(windows)'.dependencies] …
   features = ["windows-native"]`, `[target.'cfg(target_os =
   "macos")'.dependencies] … features = ["apple-native"]` (features
   union across declarations; other targets compile the mock store and
   are outside the release matrix). Implementation duty: verify exact
   feature names against keyring 3.x docs at add time (v3 moved every
   platform store behind features; names have drifted across minors)
   and prove the linkage story with `cargo tree -p taskboard-app` +
   `cargo deny check` — if the Secret Service feature drags in a
   system libdbus (C) that breaks the release pipeline's aarch64
   cross-builds, that is the canary: either document the linkage for
   desktop targets or switch the linux feature set; the release
   workflow build is part of §10. **`rpassword` is deferred to Phase
   6** (roadmap deviation, recorded): Phase 5 contains no interactive
   prompt — shipping an unused dependency now buys nothing and trips
   the unused-dep hygiene gate later.

10. **Config instance hygiene: `AppConfig` is `Clone + Debug` and
    secret-free by construction.** The secret never enters `AppConfig`
    (the tables deny unknown keys, so a `token`/`password` key cannot
    even decode); it lives only in the `AppPassword` produced by the
    `CredentialStore` and is consumed once by `DeckClient::new`.
    Startup logs the resolved config (server URL, username, store
    kind, db path, mode, sync knobs) — never the secret. `AppConfig::
    validate()` checks: URL parses via `DeckClient`-grade rules
    (http/https, has host — same normalization), non-empty username,
    positive durations with `backoff_initial <= backoff_max`, non-empty
    db path. All failures are typed variants; nothing matches on
    figment/serde message text.

11. **Bootstrap-level tests use the real seams, not a fake
    repository.** The B-series (§7) runs the *actual* production
    graph: real figment (tempdir toml + process-scoped env — tests
    serialize env-mutating cases or use a per-test figment build that
    injects providers directly), real sqlite on a tempdir file, real
    engine + storage actor, and wiremock as the Deck server
    (`server_url` points at the mock). Board binding is pre-seeded
    through the storage crate's public API (§1 seeding seam) so
    `RequestSync` triggers genuine cycles. The only injected fake is
    the `CredentialStore` (or the env store). This is the highest
    layer that is still hermetic; Phase 6's new tier then drives the
    real binary end-to-end.

12. **The docker tier proves the phase exit; the live tier is the
    user-run checkpoint.** `it_docker.rs` in the app crate (§7 T-series)
    boots the in-process daemon against the dockerized Nextcloud with
    `credential_store = "env"` and the tier env password: pull
    (server-created card appears in `AppState`), push
    (`CreateTask` through the engine handle appears on the server),
    graceful `shutdown()` with clean joins and a reopened database.
    This automates "headless daemon runs against a real Nextcloud
    server" in CI-dispatch; the literal M1 exit ("the real server")
    is the tier-3 runbook (§10) executed by the user against the live
    deployment with a real keyring entry. CI wiring:
    `nextcloud-integration.yml` grows the app crate to its nextest
    invocation (same dispatch + weekly schedule; per-PR CI stays
    hermetic by design).

13. **Interim multi-process stance (documented, not solved here).**
    Nothing in Phase 5 prevents two `taskboard` processes opening one
    database; the single-connection WAL pool plus `busy_timeout`
    makes concurrent *writers* queue rather than corrupt, which is
    tolerable until Phase 6's instance lockfile lands (its design
    must account for worktrees/CI where several test processes share
    a machine but never a db path). Recorded here so Phase 6 designs
    with eyes open; no code in this phase.

14. **Observability is a deliverable** (phase 2–4 discipline):
    `info!` — resolved config summary (minus secret), database path
    opened, engine hydrated counts (existing engine log), actors
    spawned/stopped, shutdown sequence steps and its outcome; `warn!`
    — credential backend failures (typed class only), shutdown
    timeouts preceding an abort; `error!` — bootstrap failures with
    their typed class. No secret or user content above `trace`.
    Logging arms are not text-asserted (AGENTS §5); G/B-series assert
    typed outcomes and join states instead.

## 4. Normative Shapes (signatures are binding; bodies indicative)

### 4.1 `config.rs`

```rust
/// Fully-resolved, validated application configuration. Secret-free by
/// construction (§3.10); `Debug` is safe.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AppConfig {
    #[serde(default)] pub app: AppSection,
    #[serde(default)] pub nextcloud: NextcloudSection,
    #[serde(default)] pub storage: StorageSection,
    #[serde(default)] pub sync: SyncSection,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSection {
    #[serde(default)] pub mode: AppMode,               // default: Daemon
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppMode { Daemon, Desktop, Kiosk }

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NextcloudSection {
    pub server_url: Option<String>,      // required — validate() turns None into Missing
    pub username: Option<String>,        // required — same
    #[serde(default)] pub credential_store: CredentialStoreKind, // default: Keyring
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CredentialStoreKind { Keyring, Env }

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageSection {
    pub db_path: Option<std::path::PathBuf>, // default resolved at load: dirs::data_dir()
}                                            //   /taskboard/taskboard.db

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncSection {
    #[serde(default = "default_poll_interval")]    pub poll_interval_secs: u64,    // 30
    #[serde(default = "default_backoff_initial")]  pub backoff_initial_secs: u64,  // 5
    #[serde(default = "default_backoff_max")]      pub backoff_max_secs: u64,      // 300
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration incomplete")]  Missing { field: &'static str },
    #[error("explicit config file not found")] FileNotFound(std::path::PathBuf),
    #[error("configuration failed to decode")] Decode(#[source] figment::Error),
    #[error("server URL invalid")] InvalidServerUrl,
    #[error("username empty")] EmptyUsername,
    #[error("duration invalid")] InvalidDuration { field: &'static str },
    #[error("storage path empty")] EmptyDbPath,
}

impl AppConfig {
    /// defaults → toml (§3.2 path resolution) → `TASKBOARD_*` env, then
    /// `validate()`. Never pattern-matches provider error text.
    pub fn load(explicit_path: Option<std::path::PathBuf>) -> Result<Self, ConfigError>;
    pub fn validate(&self) -> Result<(), ConfigError>;
    /// The resolved database path (default filled in).
    #[must_use] pub fn db_path(&self) -> &std::path::Path;
    /// The `[sync]` table mapped onto the actor's knobs.
    #[must_use] pub fn sync_actor_config(&self) -> taskboard_sync_nextcloud::SyncActorConfig;
    /// The validated server URL (validate() ran).
    #[must_use] pub fn server_url(&self) -> &str;
}
```

figment provider chain (binding): `Figment::from(Serialized::defaults(
AppConfig::default()))`, then `Toml::file(resolved_path)` (absent
default file → skipped silently; explicit missing path → checked
beforehand, `FileNotFound`), then `Env::prefixed("TASKBOARD_").split(
"__")`. Extract via `figment.extract()` and map any error to
`ConfigError::Decode`.

### 4.2 `secrets.rs`

```rust
/// The Nextcloud app password. Redacting `Debug`, no `Display`; the only
/// read access is `expose()`. Never logged, never serialized.
#[derive(Clone)]
pub struct AppPassword(String);

impl AppPassword {
    #[must_use] pub fn expose(&self) -> &str { &self.0 }
}
impl std::fmt::Debug for AppPassword { /* prints "AppPassword([redacted])" */ }

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("no credential stored")] NotFound,
    #[error("credential backend unavailable")] Backend(#[source] keyring::Error),
    #[error("operation not supported by this store")] Unsupported,
}

/// OS-side credential storage seam (fakes over mocks: tests use a
/// `HashMap` store; production selects via `[nextcloud] credential_store`).
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    fn load(&self, server: &str, username: &str)
        -> Result<AppPassword, CredentialError>;
    fn store(&self, server: &str, username: &str, secret: AppPassword)
        -> Result<(), CredentialError>;      // Phase 6 `login`; shipped now
    fn delete(&self, server: &str, username: &str)
        -> Result<(), CredentialError>;
}

/// `keyring` adapter: `Entry::new("taskboard", &credential_key(..))`.
/// Error mapping: `keyring::Error::NoEntry` → `NotFound`; everything
/// else → `Backend(source)` — matched on variant shape, never message
/// text.
#[derive(Debug, Default)]
pub struct KeyringStore;

/// Read-only escape hatch (headless/CI): loads `TASKBOARD_APP_PASSWORD`;
/// `store`/`delete` → `Unsupported`.
#[derive(Debug, Default)]
pub struct EnvCredentialStore;

/// Entry identity: `"{username}@{host-of-server}"` — server-scoped, so a
/// config edit pointing at another server cannot reuse a stale password.
#[must_use]
pub fn credential_key(server: &str, username: &str) -> String;
```

### 4.3 `bootstrap.rs`

```rust
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    #[error("configuration rejected")] Config(#[from] ConfigError),
    #[error("credential resolution failed")] Credential(#[from] CredentialError),
    #[error("storage open failed")] Storage(#[from] taskboard_storage_sqlite::OpenError),
    #[error("state engine failed to start")] Engine(#[from] taskboard_state::EngineStartupError),
    #[error("deck client construction failed")] Client(#[from] taskboard_sync_nextcloud::DeckError),
    #[error("app mode not implemented yet")] ModeNotImplemented(AppMode),
}

/// A running taskboard core: every Phase 6 subcommand and the Phase 7 UI
/// start here. Not `Clone` (the shutdown sequence consumes it once).
pub struct App {
    pub config: AppConfig,
    pub engine: taskboard_state::EngineHandle,
    pub system: tokio::sync::broadcast::Sender<taskboard_domain::SystemEvent>,
    sync_commands: tokio::sync::mpsc::Sender<taskboard_domain::SyncCommand>, // Phase 6 SetBoard seam
    storage: taskboard_storage_sqlite::StorageHandle,
    engine_join: tokio::task::JoinHandle<()>,
    sync_join: tokio::task::JoinHandle<()>,
}

impl App {
    /// A second sender into the sync actor's inbox (`SetBoard`, Phase 6).
    #[must_use] pub fn sync_commands(&self) -> tokio::sync::mpsc::Sender<taskboard_domain::SyncCommand>;

    /// The graceful shutdown sequence (§5). Bounded; never panics.
    async fn shutdown_inner(self) -> ShutdownOutcome;   // split for testability, pub(crate)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownOutcome {
    /// The sync actor was aborted at the timeout instead of joining.
    pub actor_aborted: bool,
}

/// Wires the full graph (§3.4 order) and spawns every actor onto the
/// current runtime. Failing steps abort with typed errors and leak
/// nothing (the only mid-sequence failure is engine hydration, whose
/// cleanup drops the storage handle and joins it).
pub async fn bootstrap(
    config: AppConfig,
    credentials: std::sync::Arc<dyn CredentialStore>,
) -> Result<App, BootstrapError>;

/// Daemon lifecycle: await ctrl_c / SIGTERM (unix), then `shutdown()`.
/// The second-signal force-exit lives in `main`, not here.
pub async fn run_until_signal(app: App) -> ShutdownOutcome;
```

Channel capacities (binding): system broadcast 64, sync commands 32,
sync reports 32 (§3.4).

### 4.4 `cli.rs` + `main.rs`

```rust
#[derive(clap::Parser)]
#[command(name = "taskboard", version = crate::VERSION)]
pub struct Cli {
    /// Path to the config file (default: ./taskboard.toml if present).
    #[arg(long, global = true)]
    pub config: Option<std::path::PathBuf>,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(clap::Subcommand)]
pub enum Command {
    /// Run the headless sync daemon (M1).
    Daemon,
}
```

`main`: `color_eyre::install` → `init_tracing` (unchanged, incl. the
`tokio-console` feature path) → parse `Cli` → `AppConfig::load(cli.
config)` → pick the store from config → `bootstrap` → run (subcommand
`Daemon` or `[app] mode`; `Desktop`/`Kiosk` → the typed
`ModeNotImplemented` report) → `run_until_signal` with the
second-signal force-exit select → exit 0. Tracing initialization stays
in the binary (a global side effect; the lib only *emits* spans —
tests run subscriber-less, where macros are no-ops).

## 5. Graceful Shutdown Sequence (normative)

```
signal (ctrl_c / SIGTERM)
  1. engine.flush().await          — every accepted command is durable
  2. system.send(Shutdown)          — engine enters drain (§3.6),
                                      actor finishes its in-flight cycle
  3. await sync_join, bounded by [app] shutdown_timeout_secs (10 s)
       ├─ joined  → its final report was already ingested (drain)
       └─ timeout → sync_join.abort()   // sender drops → reports close
  4. await engine_join, bounded (abort as last resort)
                                      — drain ended when reports closed
  5. drop(storage handle); await storage_join
                                      — actor drains its inbox, exits
  6. log + return ShutdownOutcome { actor_aborted }
```

Why the order is safe: the engine's drain keeps the report channel
serviced while the actor finishes, so step 3's join is the moment the
last evidence lands (step 4 then observes the reports channel closed
and the engine exits on its own); aborting the actor in step 3 is
equivalent to a crash *between* cycles from the outbox's perspective
(never mid-transaction: the storage actor owns the transaction, and it
is only stopped in step 5 after the engine stopped issuing batches).
Storage failure during drain ingestion follows existing engine
semantics (log, discard report — the next boot re-reads server truth).

## 6. Module Layout & TDD Implementation Order

```
crates/taskboard-app/
├── Cargo.toml            # lib+bin; keyring target-specific features; dev-deps
│                         #   wiremock, tempfile, dotenvy, base64, proptest
├── src/
│   ├── lib.rs            # NEW: module roots, #![forbid(unsafe_code)]
│   ├── config.rs         # NEW: §4.1
│   ├── secrets.rs        # NEW: §4.2
│   ├── bootstrap.rs      # NEW: §4.3 (channel graph, spawn order, App)
│   ├── cli.rs            # NEW: §4.4 (clap surface only; dispatch in main)
│   └── main.rs           # thin: parse → tracing → bootstrap → run
└── tests/
    ├── config.rs         # C-series
    ├── secrets.rs        # S-series
    ├── bootstrap.rs      # B-series (hermetic E2E; tests/common/mod.rs helpers)
    └── it_docker.rs      # T-series (docker tier, env-gated, it_nextcloud_docker_*)
crates/taskboard-state/
└── src/actor.rs          # drain-mode semantics (§3.6) + tests
crates/taskboard-sync-nextcloud/
└── src/client.rs         # redacting Debug (§3.7) + test
Cargo.toml                # +clap, +dirs, +keyring (workspace.dependencies)
```

Branch: **`feat/app-bootstrap`** (this plan's branch; worktree
`taskboard-WT-feat-app-bootstrap` created from `main` @ `0ef6d00`).
One PR for the plan, following the phase 4 precedent (plan revisions
and implementation commit to the same branch). TDD order:

1. Workspace deps + app manifest + lib/bin split (mechanical; `main`
   behavior unchanged; quality gates green on the skeleton).
2. `config.rs`: structs + `load`/`validate` with C-series first
   (defaults, precedence, env nesting, unknown-key rejection incl. the
   secret-in-TOML case, required-missing, invalid values, explicit
   missing file).
3. `secrets.rs`: `AppPassword` redaction, `credential_key`, env store,
   keyring error mapping (constructible variants), fake store in
   `tests/common`; S-series.
4. `DeckClient` Debug redaction (+ its leak-floor test).
5. Engine drain semantics in `taskboard-state` (G-series: drain replies
   `EngineGone`/flush-ok; late report after `Shutdown` is ingested;
   reports-close ends drain; already-closed reports exit immediately;
   engine does not exit on `Shutdown` while reports are open).
6. `bootstrap.rs` + `App::shutdown` (B-series hermetic: boot/hydrate/
   teardown; seeded-binding cycle against wiremock with auth-header
   proof; shutdown mid-cycle; shutdown with a hung actor → bounded
   abort; persistence survives restart).
7. `cli.rs` + `main.rs` daemon wiring + `run_until_signal` (thin;
   second-signal force-exit documented as untested glue).
8. `it_docker.rs` T-series exit demo; extend
   `nextcloud-integration.yml` to run it.
9. Docs (ADR 0008, architecture.org, CHANGELOG, backlog, README note)
   + full quality gates + `cargo tree`/`cargo deny` verification.

## 7. Test Plan (catalogue)

Deterministic by construction: no sleeps — predicate polls with
deadlines (`tokio::time::timeout` guards), join-handle completion as
the terminal signal, typed outcome assertions only, wiremock
per-mock verifications for request *shapes* (Basic-auth header
computed in-test). Env-mutating config tests either build figment
providers directly (unit) or serialize via a process-wide env mutex
(integration) — never parallel env races. The full-graph tests use
real sqlite files under `tempfile` dirs and wiremock Deck endpoints
with inline canned payloads (boards listing, both stack listings,
card detail — the merged Phase 4 cycle surface; the sync crate's
`tests/common` harnesses are test-internal and not importable, so the
app crate carries its own minimal `tests/common/mod.rs`).

- **C-series (config, `tests/config.rs` + inline units)**: C1 defaults
  parse with no file (path absent → defaults; db default resolves
  under the data dir); C2 toml → env precedence (`TASKBOARD_SYNC__
  POLL_INTERVAL_SECS` beats the file; `--config` path is used);
  C3 required-missing (`server_url`/`username` absent →
  `ConfigError::Missing{field}`, one per field); C4 unknown key in a
  table → `Decode` — including the normative case `[nextcloud]
  token = "…"` (secrets in TOML are mechanically impossible);
  C5 foreign top-level env profile (`TASKBOARD_IT_NEXTCLOUD_URL` set)
  does not break extraction; C6 invalid values (bad URL →
  `InvalidServerUrl`; `poll_interval_secs = 0` → `InvalidDuration`;
  `backoff_initial > backoff_max` → `InvalidDuration`); C7 explicit
  missing `--config` path → `FileNotFound`; C8 `sync_actor_config()`
  mapping equals the actor's documented example values; C9 serde
  round-trip of `AppConfig` (defaults survive; kebab-case enums).
- **S-series (secrets, `tests/secrets.rs` + inline units)**: S1
  `AppPassword` Debug redaction (leak-floor: formatted output lacks
  the secret); S2 `credential_key` shape (username@host; hostless and
  garbage server inputs still produce a stable key — pure fn, no
  panics); S3 env store load hit/miss (`TASKBOARD_APP_PASSWORD`
  present/absent → value/`NotFound`); S4 env store `store`/`delete` →
  `Unsupported`; S5 keyring error mapping table
  (`NoEntry → NotFound`; constructible backend variants → `Backend`)
  — shape-matched, no text; S6 fake store (tests/common) round-trip
  via the port; S7 `#[ignore]`d local keyring round-trip
  (`TASKBOARD_IT_KEYRING=1` guard; exercises `KeyringStore` against
  the developer's real Secret Service — run in the tier-3 runbook).
- **G-series (engine drain, `crates/taskboard-state` — extends the
  phase 3 suite)**: G1 `Shutdown` while the reports channel is open →
  engine keeps ingesting (a report sent after `Shutdown` lands in
  `AppState`/persistence); G2 drain-mode `Execute` →
  `Err(EngineGone)`, drain-mode `Flush` replies promptly (no hang);
  G3 reports channel closes after `Shutdown` → engine task exits
  (JoinHandle); G4 `Shutdown` with reports already closed → immediate
  exit; G5 pre-`Shutdown` pending reports still ingested before drain
  begins (biased-order regression pin); G6 engine ignores a second
  `Shutdown` during drain; G7 (regression) `NetworkLost`/`Restored`
  semantics unchanged outside drain.
- **B-series (hermetic bootstrap E2E, `tests/bootstrap.rs`; wiremock +
  real sqlite; `credential_store = "env"`)**: B1 boot on an empty temp
  database → engine hydrated (`shared_state` default), storage actor
  alive, `RequestSync` executed without a binding → `SyncPhase::
  Failed { NoBoard }` observed in `shared_state` (the actor reported
  through the graph — proof the whole wiring is live); B2 seeded
  binding (storage-API pre-seed) + scripted cycle → `last_success`
  set, remote task merged into `AppState`; **auth proof**: the
  boards mock's request verification asserts the Basic-auth header
  decodes to `username:TASKBOARD_APP_PASSWORD` — the secret provably
  reached the client through config+store+bootstrap; B3 graceful
  shutdown on an idle graph → all three joins complete, database
  reopens, hydrated state equals the last published projection;
  B4 shutdown during an in-flight cycle (first Deck endpoint hangs)
  → bounded sequence completes (`actor_aborted = true`), database
  reopens with the outbox intact (queued ops neither lost nor
  double-applied); B5 bootstrap failure paths: bad URL → `Client`,
  missing credential → `Credential(NotFound)`, unreadable db
  directory → `Storage`, engine hydration on a corrupted db →
  `Engine` (typed variant per row, nothing spawned leaks — the
  storage join is awaited in the cleanup path); B6 config with
  `mode = "desktop"` → `ModeNotImplemented`; B7 restart continuity:
  B2's database rebooted in a fresh process (new bootstrap in the
  same test) → validators/binding hydrate and a `RequestSync` cycle
  304s against the same wiremock validators (mirrors phase 4's I5,
  now through the bootstrap seam).
- **T-series (docker tier 2, `tests/it_docker.rs`;
  `it_nextcloud_docker_*`, env-gated, skip-never-fail, run-id
  isolation + teardown, wall-clock timeouts — the phase exit demo)**:
  T1 the daemon journey — create board+stack+card server-side via a
  raw `DeckClient`, storage-seed the board binding, `bootstrap` with
  `credential_store = "env"` + tier creds, `RequestSync`, predicate-
  poll `AppState` until the card appears (pull), `CreateTask` through
  the engine handle, predicate-poll the server until the card title
  appears (push), `shutdown()` → clean joins, reopen db → outbox
  drained (`pending_ops == 0`); T2 shutdown-mid-cycle against the
  real server (SIGTERM-timeout path exercised via a tight
  `shutdown_timeout_secs` during a cycle) → clean bounded shutdown,
  outbox consistent on reopen.
- **Doc-tests**: `bootstrap` usage sketch (mirroring the state/sync
  crates' spawn doc-tests), `AppConfig::load`, `CredentialStore`
  fake-store example.

Coverage: llvm-cov review target = config, secrets, bootstrap,
shutdown sequence. Miri/mutants scope is unchanged (domain+state
only) — the engine drain lands inside `taskboard-state` and inherits
the gates for free; the app crate's async tests are outside their
scope (testing strategy §10).

## 8. Out of Scope → Backlog (append with this plan)

- **XDG config discovery**: search
  `$XDG_CONFIG_HOME/taskboard/taskboard.toml` (then the CWD file)
  when no `--config` is given; today only `./taskboard.toml` is read.
- **Encrypted-file credential store** (updates the existing
  "[2026-10-08] Keyring fallback" entry, whose decision this plan
  makes): a config-selectable third `CredentialStore` impl for
  deployments that refuse env vars but lack a Secret Service — key
  management (passphrase-derived via argon2, or a generated key file)
  is the open design question. Env-store is the sanctioned M1
  fallback; recorded trade-off: env vars are process-scoped, visible
  to same-user tooling and `ps -E` on some platforms — acceptable for
  CI/SSH escape hatch, not for long-lived desktop use (keyring
  remains the default).
- **Systemd/journald packaging**: unit file template, `Type=notify`
  readiness, journald log destination for kiosk deployments (the
  fmt subscriber's stderr is fine for M1 foreground use).
- **Config doctoring / `taskboard config print`**: redacted effective-
  config dump for support conversations (Phase 6 CLI surface makes it
  a one-liner then).
- **Signal-escalation policy**: today the second Ctrl-C force-exits;
  a "flush-then-force" ladder with distinct exit codes for kiosk
  supervision is a future hardening.

## 9. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0008 — "Application bootstrap: in-process core, config
  hierarchy, secret resolution"**: the lib+bin split and why; channel-
  graph ownership consolidated in `bootstrap` (closes the "Phase 5
  bootstrap owns the system broadcast sender" note in ADR 0007's
  handoff); the strict-tables/no-secrets-in-TOML rule; the
  `CredentialStore` port, config-selected stores, env escape hatch
  and its threat-model trade-off (backlog cross-ref); the graceful
  shutdown sequence with the engine linger-drain rationale
  (evidence-then-verdict preserved across shutdown); dependency
  additions with platform-feature strategy. Status: Accepted.
- **architecture.org**: rewrite the `taskboard-app` bullet (bootstrap
  library, config/secrets, daemon mode, shutdown; Phase 7 UI mount
  stays projected); channel table — mark the system broadcast and all
  inter-actor channels as **bootstrap-created** (the ownership note
  phase 4 left for this plan); dependency list of the app crate
  refreshed to the manifest (figment, clap, keyring, dirs, color-eyre,
  tracing, tokio, all workspace crates).
- **CHANGELOG** `[Unreleased]` → Added: bootstrap library + config +
  secrets + daemon + shutdown; the engine drain semantics; the
  `DeckClient` Debug redaction; Changed: `taskboard` binary now
  accepts `daemon`/`--config`/`--version` (no-args default unchanged
  in spirit: it runs the daemon per `[app] mode`).
- **Backlog**: append §8 entries; update the keyring-fallback entry
  with a status note (env escape hatch shipped in Phase 5;
  encrypted-file store remains the entry's future work).
- **README.org**: a short M1 quickstart — configure `taskboard.toml`,
  provide the app password (keyring via a one-liner `secret-tool`
  note until Phase 6's `login` exists, or the env store), run
  `taskboard daemon`.
- **CI**: `nextcloud-integration.yml` — add `-p taskboard-app` (T-series)
  to the docker-tier nextest invocation, same guards as the sync
  crate's tier.

## 10. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets -- -D warnings` (CI sets
  `SQLX_OFFLINE=true`; no new queries ship, the committed `.sqlx/`
  cache suffices); `cargo nextest run --workspace`; `cargo test --doc
  --workspace`; `cargo deny check`; `cargo llvm-cov --workspace`.
- Per-crate: `-p taskboard-app` C/S/B series green; `-p
  taskboard-state` phase 3 suite + G-series green (drain is additive);
  `-p taskboard-sync-nextcloud` re-run green (Debug change only).
- Dependency verification (§3.9): `cargo tree -p taskboard-app` shows
  no openssl; the release matrix's cross-build check (or a local
  `cargo check --target aarch64-unknown-linux-gnu`) proves the linux
  keyring feature set links; `cargo deny check` passes with the new
  graph.
- Tier 2: `eval "$(scripts/nextcloud_it_setup.sh up)" && cargo nextest
  run -p taskboard-app --run-ignored only -E 'test(
  it_nextcloud_docker)'` green (T1 is the automated phase-exit demo);
  in CI, dispatch `nextcloud-integration.yml` on the PR branch.
- **Phase 5 exit (roadmap M1 checkpoint)** — user-run tier-3 runbook
  against the live server: (1) configure a minimal `taskboard.toml`
  (server URL, username); (2) store the app password in the desktop
  keyring under `taskboard` / `user@host` (e.g. `secret-tool set
  --service taskboard --account user@host` until Phase 6's `login`
  exists) or export `TASKBOARD_APP_PASSWORD` with
  `credential_store = "env"`; (3) `taskboard daemon`; (4) observe a
  full cycle in the logs (`state engine hydrated`, sync `Completed`);
  (5) add a task offline (network down / server unreachable), watch
  `SyncPhase::Offline`, restore the network, observe the task in the
  Nextcloud web UI; (6) Ctrl-C → the §5 sequence completes in the
  logs and the process exits 0; (7) `cargo nextest run -p
  taskboard-app --run-ignored only -E 'test(it_nextcloud_live)'`
  (S7's keyring round-trip included). Record any known-server matrix
  additions in `testing_strategy.org`.

## 11. Consumer Handoff Map (what later phases import)

| Consumer | Takes from Phase 5 |
|---|---|
| Phase 6 CLI | `taskboard_app::bootstrap` + `App` (each subcommand boots the core in-process, works, `shutdown()`), `EngineHandle::execute/flush/shared_state` for `--json` output (`AppState: Serialize`), `App::sync_commands()` sender for `boards select` (`SyncCommand::SetBoard`), `CredentialStore::{store,delete}` for `login`/logout, the clap `Command` enum as the growth point, `rpassword` lands with `login` |
| Phase 7 UI | `App.engine.shared_state()` + `subscribe()` for Slint mounting, `[app] mode` = `desktop`/`kiosk` replacing the `ModeNotImplemented` arm, bootstrap unchanged (UI mounts beside the daemon loop) |
| Kiosk deployments | SIGTERM-graceful shutdown for containerized/supervised operation; the config surface (`[sync]` knobs, `[storage] db_path`) as the tuning point; the env-store escape hatch for keyless images |
| Everyone | the "in-process core" pattern (roadmap §2.4) — one bootstrap, N frontends; the redacted-`Debug` discipline for anything carrying secrets |

## 12. Risks & Mitigations

- **keyring platform features / CI runners**: ubuntu-latest runners
  have no Secret Service — `KeyringStore` cannot run there. Mitigated
  by design: CI paths use `EnvCredentialStore`/fakes; the real
  keyring is exercised only by the guarded S7 test and the tier-3
  runbook. Feature-name drift across keyring 3.x minors is an
  implementation-time verification duty (§3.9, §10).
- **Linux keyring linkage in cross-builds**: the Secret Service
  feature may pull C linkage (libdbus) or rustls-incompatible crypto
  backends. Mitigated: `crypto-rust` chosen; `cargo tree` +
  aarch64 `cargo check` are explicit §10 gates with a documented
  escape (switch the linux feature set; the port makes the store
  swappable without touching bootstrap).
- **Shutdown deadlocks**: every await in the §5 sequence is bounded
  (config timeouts), every abort is followed by a join; G/B-series
  cover both the idle and hung-actor paths; the second-signal
  force-exit is the operator's escape hatch. The one theoretical
  hang — drain ingestion blocked on a storage apply while the storage
  actor is healthy — is impossible by construction (single-writer
  inbox, 5 s busy timeout, bounded apply).
- **Env-mutating tests racing**: config tests that set `TASKBOARD_*`
  can race nextest's process-parallel peers. Mitigated: unit-layer
  tests build figment providers directly (no process env); the few
  integration env cases serialize behind a shared mutex and restore
  prior values on drop.
- **figment dynamism vs typed errors**: all semantic failures
  (missing/invalid) surface through `Option` fields + `validate()`,
  never by inspecting provider error text; decode failures collapse
  into one `Decode` variant whose *source* is logged, not matched.
- **linger-drain changes engine semantics mid-stream**: additive and
  engine-local; the phase 3 insta snapshot suite re-runs untouched
  (drain is post-Shutdown behavior, absent from every existing
  snapshot), and G-series pins the new contract in both directions
  (no early exit, no hang).
- **Two processes, one database** (pre-lockfile window): WAL +
  single-writer + busy-timeout queue writes safely; the lockfile is
  Phase 6 scope with this phase's §3.13 note as its design input.
  Tests never share db paths (per-test tempdirs), so CI cannot trip
  it.
- **Scope creep via "just add `login`"**: resisted — the store *API*
  ships (Phase 6 needs the exact seam) but no interactive UX, no
  verification call, no `rpassword` until Phase 6's plan sizes them.
