# [Plan] Growth Roadmap Phase 6 — CLI Subcommands + App-Level E2E (M1 Exit)

- **Date**: 2026-10-10
- **Target**: `taskboard-app` (new `commands/` module tree, `lockfile.rs`,
  grown `cli.rs`, thin `main.rs`; new workspace deps `rpassword`, `fd-lock`,
  `toml_edit`, `serde_json` promoted to a real dep), the app crate's test
  suite (L/V/K/O-series hermetic + J-series docker-tier binary-driving E2E),
  docs (ADR 0009, testing strategy, README M1 quickstart), backlog appends.
  No changes to other crates are currently identified (the roadmap §2.5
  agility agreement applies if implementation proves otherwise).
- **Goal**: implement the roadmap's Phase 6 — the operator-facing CLI
  (`login`, `boards list/select`, `tasks list/add/done/move`, `sync`,
  `daemon`) over the Phase 5 in-process core, with `--json` output for
  scripting/tests, an instance lockfile (one core per database), and a new
  app-level E2E test tier that drives the **real binary** against the
  tier-2 dockerized Nextcloud. Exit (M1): a deployable, real-server-
  validated headless taskboard.
- **Status**: DRAFT (rev 1, awaiting user review)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 6, §6 M1 acceptance.
- **Predecessors**: Phases 1–5 all merged. Authored against merged `main`
  @ `4b45902`. Residual duty: re-verify the §1 spawn/shape citations if
  anything lands on `main` before the branch starts.
- **Branch / worktree**: `feat/app-cli-e2e`,
  `../taskboard-WT-feat-app-cli-e2e` (created from `main` @ `4b45902`).
  One PR for the plan (phase 4/5 precedent: plan revisions and
  implementation commits land on the same branch).

---

## 1. Context & Known Facts (do not re-derive)

- **Phase 5 shipped the in-process core** (`crates/taskboard-app/src/
  bootstrap.rs`): `bootstrap(config: AppConfig, credentials: Arc<
  dyn CredentialStore>) -> Result<App, BootstrapError>` spawns storage →
  engine → sync actor; `App` exposes `config`, `engine: EngineHandle`,
  `system: broadcast::Sender<SystemEvent>`, `sync_commands() -> mpsc::
  Sender<SyncCommand>` (the `SetBoard` seam, bootstrap.rs:134), and
  `shutdown(self) -> ShutdownOutcome` (flush → `Shutdown` broadcast →
  bounded joins, bootstrap.rs:147). `wait_for_signal()` and
  `run_until_signal` exist. `main.rs:54` (`run`) is thin: load config →
  pick store → bootstrap → daemon lifecycle with second-signal force-exit.
- **The clap surface is a skeleton**: `cli.rs:26` `Command` has only
  `Daemon`; its doc comment says Phase 6 grows `Login`, `Boards`, `Tasks`,
  `Sync` "without touching `main`'s dispatch shape" (§3.2 refines this:
  handler *bodies* move into the lib; `main` keeps a one-arm dispatch).
- **Config**: `AppConfig::load(explicit_path)` (config.rs:218) resolves
  the file as `--config` > `TASKBOARD_CONFIG` > `./taskboard.toml`
  (absent default = defaults-only; explicit missing = `ConfigError::
  FileNotFound`), then **validates** — a not-yet-logged-in machine has no
  `server_url`, so `login` must not go through `load()`+validate
  (§3.4). Sections are strict (`deny_unknown_fields` per table);
  `credential_store = "keyring" (default) | "env"`. Provider chain
  `figment()` at config.rs:347. `sync_actor_config()` maps seconds →
  `SyncActorConfig`. `shutdown_timeout_secs` (default 10) bounds actor
  joins at shutdown.
- **Secrets**: `CredentialStore` port (`load/store/delete`, secrets.rs)
  with `KeyringStore` (`service = "taskboard"`, entry =
  `credential_key(server, username) = "{username}@{host}"`) and
  `EnvCredentialStore` (reads `TASKBOARD_APP_PASSWORD`, secrets.rs:22;
  `store`/`delete` → `CredentialError::Unsupported`). `AppPassword`
  redacts in `Debug`. `FakeCredentialStore` exists in
  `tests/common/mod.rs:73`.
- **Engine surface** (the CLI's primary object): `execute(StateCommand)
  -> Result<CommandOutcome, ExecuteError>` (a reply implies durable
  persistence), `dispatch`, `flush`, `subscribe() -> broadcast::Receiver<
  EngineSignal>` (a signal follows every publish), `shared_state() ->
  Arc<ArcSwap<AppState>>`, and `SyncStateReader::read_state` (full
  `PersistedState` incl. outbox). `CommandOutcome`: `CreatedTask(TaskId)`,
  `CreatedStack`, `CreatedLabel`, `Applied`, `NoOp`, `SyncRequested`.
  `ExecuteError`: `Rejected(CommandError)`, `Storage(RepositoryError)`,
  `EngineGone`. `CommandError`: `UnknownTask`, `TaskDeleted`,
  `UnknownStack`, `StackDeleted`, `UnknownLabel`, `LabelDeleted`,
  `NoBoard`.
- **Relevant `StateCommand`s** (`taskboard-domain/src/messages.rs:19`):
  `CreateTask { title, stack: StackId, order: i64 }`, `SetTaskDone { id,
  done }` (repeat = `NoOp`), `MoveTask { id, stack, order }` (one intent:
  stack+order share a clock), `RequestSync` (special-cased in the engine:
  forwards `SyncNow`, marks the memory-only `Syncing` transient, replies
  `SyncRequested` immediately — *not* a cycle-completion barrier).
  **There is no command that creates a Board** — boards only appear via
  sync adoption; board-scoped commands need a live board.
- **Reads**: `AppState` (domain state.rs:24) is `Serialize` with
  `BTreeMap` tables (deterministic key order). `live_tasks()` (canonical
  order: `order` asc, id asc), `tasks_in_stack(StackId)`. `SyncStatus {
  phase: SyncPhase, last_success: Option<DateTime<Utc>>, pending_ops:
  u32 }`; `SyncPhase`: `Idle` (default), `Syncing` (memory-only
  transient), `Offline`, `Failed { last_error: SyncErrorKind }`;
  `SyncErrorKind`: `Network | Auth | Forbidden | Server | BadRequest |
  LocalData | NoBoard`.
- **Sync actor behavior** (`taskboard-sync-nextcloud/src/actor.rs`):
  the **first cycle fires immediately at spawn** (actor.rs:181); poll
  ticks between cycles are silent `NoBoard` skips, **commanded** cycles
  (`SyncNow`/`SetBoard`) report `Failed { NoBoard }` (actor.rs:294–320).
  `SetBoard(id)` sets the in-memory target and commands a cycle; the
  durable binding is the **adopted board row** (`boards` table with
  `remote: Some(RemoteBoardId)`) — a restarted actor resumes its target
  from `read_state()` (actor.rs:294–304). `Shutdown` is processed only
  **between** cycles; `App::shutdown`'s bounded join **aborts mid-cycle
  at the timeout** (equivalent to a crash between pushes from the
  outbox's perspective; the duplicate-create hard-crash window is an
  existing backlog entry).
- **Single-board invariant**: domain `live_board()` (command.rs:99) picks
  `min(BoardId)` among live boards — with two adopted boards, commands
  would silently target the wrong one. The MVP is single-board
  (roadmap §4); `boards select` must therefore refuse to rebind
  (§3.5).
- **Deck client**: `DeckClient::new(base_url, user, token)` (URL rules:
  http/https + host, else `InvalidBaseUrl`), `boards() -> Vec<Board>` is
  the credential-verification call (roadmap:149). `DeckError`:
  `Unauthorized` (401) / `Forbidden` (403) are the bad-credential
  signals — tier-3 evidence shows some servers answer 403 for bad
  tokens, so both map to the same typed login failure. `classify_read`
  (mapping.rs:187) sends `NotFound` → `SyncErrorKind::Server` — a
  bogus board id in a `SetBoard` cycle surfaces as a generic `Server`
  failure, which is why `boards select` pre-validates against a fresh
  `boards()` listing (§3.5).
- **Lockfile design input** (phase 5 plan §3.13): two processes on one
  database queue writes safely (WAL, single writer, busy-timeout) but
  must be prevented anyway; the lockfile design "must account for
  worktrees/CI where several test processes share a machine but never a
  db path" → the lock is **scoped to the database file** (sidecar), not
  global. Tests use per-test tempdir dbs, so CI cannot trip it.
- **Test-tier harness (reusable as-is)**:
  - Tier 2 env: `TASKBOARD_IT_DOCKER_{URL,USER,TOKEN}` minted by
    `scripts/nextcloud_it_setup.sh up [--ci]` (compose file
    `docker/nextcloud-test/compose.yaml`, `nextcloud:35-apache`,
    loopback-only, `occ`-minted app password); teardown `down`.
  - Naming/filters: docker-tier tests are `#[ignore]`-marked, named
    `it_nextcloud_docker_*`, run with `--run-ignored only -E
    'test(it_nextcloud_docker)'`; `.config/nextest.toml` serializes them
    (`docker-tier` group, `max-threads = 1`).
  - App-crate helpers (`crates/taskboard-app/tests/common/mod.rs`):
    `env_lock()`/`EnvOverride` (process-env mutation guard, :18–66),
    `FakeCredentialStore` (:73), wiremock fixture builders
    (`boards_json`, `board_json`, `stacks_json`, `card_json`, :135–215),
    `live_docker_config()` skip-never-fail guard (:227), `run_id()`
    (`taskboard-it-<uuid8>` prefix, :242), `with_deadline` (300 s),
    `retry` (Deck's sporadic write-path `Server(500)`s, ≤4 attempts).
    `it_docker.rs` contributes `create_board_and_stack` (:256),
    `seed_board_binding` (:282 — pre-seeds a db with an adopted board
    via the storage API), `live_config` (:308 — `AppConfig` with
    `credential_store = Env`).
  - CI: `nextcloud-integration.yml` (weekly cron + `workflow_dispatch`)
    already runs `-p taskboard-app --run-ignored only -E
    'test(it_nextcloud_docker)'` after `up --ci` — **J-series tests are
    picked up by the existing invocation with zero YAML changes**
    (verify the binary builds under that step, §9).
  - **Driving the real binary needs no new test dep**: integration
    tests of the app package get `env!("CARGO_BIN_EXE_taskboard")`
    (cargo sets it for `[[bin]]` targets of the same package) —
    `std::process::Command::new(env!("CARGO_BIN_EXE_taskboard"))`.
- **Dependency facts**: `serde_json` (1.0.151) and `toml_edit` (0.22.27)
  are already in `Cargo.lock` transitively (promoting them to direct
  workspace deps adds no graph weight); `serde_json` is currently a
  *dev*-dependency of the app crate only. `rpassword` and `fd-lock` are
  new (both MIT OR Apache-2.0, tiny; `fd-lock` pulls only
  `libc`/`windows-sys` target deps, both already in the allow-list
  class via keyring). `cargo deny check` is the gate.
- **House rules that shape this phase**: no text assertions (assert
  typed outcomes / JSON structure), no sleeps (predicate polls with
  deadlines; wall-clock deadlines are sanctioned in the live tiers),
  one behavior one test at the lowest layer (handler tests hermetic via
  wiremock; the docker J-series tests only the wiring), fakes over
  mocks (`FakeCredentialStore`, `FakePrompts`), `#![forbid(unsafe_code)]`
  + AGPL headers in the app crate, commit hygiene for rebase merges.

## 2. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| clap surface | `cli.rs`: `--json` global flag; subcommands `login`, `boards {list, select}`, `tasks {list, add, done, move}`, `sync`, `daemon` (§3.1) |
| Command handlers | new `commands/` module tree in the `taskboard_app` lib: typed handlers returning typed outcomes; `main.rs` stays thin dispatch + exit-code mapping (§3.2) |
| Pre-core commands | `login` (prompt/flags → verify via `boards()` → persist config + secret; `rpassword`), `boards list` (direct `DeckClient`, no core boot, no lock) (§3.3–§3.5) |
| Core commands | `boards select` (pre-validate + `SetBoard` + adoption wait), `tasks list/add/done/move`, `sync` (trigger + bounded wait + status) — all boot the in-process core per roadmap §2.4 (§3.5–§3.7) |
| Output contract | `commands/output.rs`: text + `--json` projections (serde types, kebab-case discriminators), error JSON on stderr, exit codes 0/1/2/130 (§3.8) |
| Instance lockfile | `lockfile.rs`: `fd-lock` exclusive advisory lock on `<db_path>.lock`, acquired in `bootstrap` before the db open, held in `App` for the process lifetime; typed `BootstrapError::InstanceLocked` (§3.9) |
| Workspace deps | `rpassword = 7`, `fd-lock = 4`, `toml_edit = 0.22`, `serde_json` promoted to a real dependency (§3.10) |
| Hermetic tests | L-series (login), V-series (verbs via wiremock + real sqlite), K-series (lockfile, incl. a cross-process hermetic binary test), O-series (output shapes) (§6) |
| E2E tier | J-series `it_nextcloud_docker_cli_*`: binary-driving journeys (login→select→add→verify-remote; remote-create→pull→list; bad-credentials) reusing the tier-2 harness (§3.11, §6) |
| CI | verify the existing `nextcloud-integration.yml` app-crate step picks up J-series (binary built under nextest); no per-PR CI change (§8) |
| Docs | ADR 0009; `testing_strategy.org` app-level E2E section; `architecture.org` app-crate bullet; README M1 quickstart rewrite (login → select → add → sync → daemon); CHANGELOG; backlog appends (§8) |

**Out of scope** (backlog, §7): `tasks edit/delete`, `stacks`/`labels`
subcommands; `logout`; short-id/prefix task selectors; a no-sync `status`
read; `config print` (existing backlog entry); JSON schema stability
guarantees; daemon IPC (existing entry); OAuth2 (existing entry);
multi-board (existing entry); any UI work (Phase 7).

## 3. Design Decisions

1. **Command catalogue and clap shape.** `Command` grows to
   `Login { … }`, `Boards(BoardsCmd)` with `List` / `Select { board:
   u64 }`, `Tasks(TasksCmd)` with `List { stack: Option<String> }` /
   `Add { title: String, stack: Option<String> }` / `Done { task:
   String, undo: bool }` / `Move { task: String, stack: Option<String>,
   order: Option<i64> }`, `Sync { no_wait: bool, timeout_secs: u64 }`,
   `Daemon` (unchanged). A global `--json` flag on `Cli`
   (`#[arg(long, global = true)]`) switches every output-producing
   command from human text to the §3.8 payloads; `login` takes
   `--server <URL>` / `--user <NAME>` so scripted and interactive use
   share one code path (prompt only for what is missing). `taskboard`
   with no subcommand keeps the Phase 5 semantics (`[app] mode`
   resolution; daemon until Phase 7). *Rejected*: per-subcommand
   `--json` flags (inconsistent combinatorics for no benefit);
   `SyncStatus`-style nested `sync status` subcommand (see §3.7: a
   status read without a cycle is a fiction under the uniform-core
   doctrine; backlog).

2. **Handlers live in the lib; `main` is dispatch + exit codes.**
   `commands/` (one module per subcommand group + `output.rs`) exposes
   `pub async fn dispatch(command: Command, ctx: CommandCtx) ->
   Result<CommandOutcome, CommandError>`-style handlers, where
   `CommandCtx` carries `json: bool`, the resolved config path, and the
   credential-store choice. Hermetic tests call handlers directly
   (V/L/K/O-series); only `main.rs` maps `CommandError` → stderr report
   + exit code. This refines the phase-5 "dispatch lives in the binary"
   comment the way it anticipated: the *dispatch shape* in `main` stays
   one arm per subcommand, the bodies are testable lib code. Exit
   codes: `0` success (incl. `NoOp` idempotent repeats — they are
   successes); `1` runtime failure (any `CommandError`); `2` clap usage
   errors (clap default); `130` second-signal force exit (existing).
   `CommandError` is a typed `thiserror` enum (§4.2) wrapping
   `ConfigError`, `BootstrapError`, `CredentialError`, `ExecuteError`,
   plus CLI-specific variants (`InvalidCredentials`, `UnknownBoard`,
   `BoardAlreadyBound`, `UnknownTask`, `AmbiguousStack`, `NoStacks`,
   `SyncFailed(SyncErrorKind)`, `SyncTimedOut`, `Login(io)`,
   `ConfigWrite(io)`). Nothing matches on `Display` text.

3. **Two command classes: pre-core and core-booting.** `login` and
   `boards list` never boot the actor graph — they need only config +
   a `DeckClient` (`login` before a valid config exists at all;
   `boards list` is a stateless remote listing for the first-run
   selection UX). They take **no lock** and touch no database. Every
   other command (`boards select`, `tasks *`, `sync`, `daemon`) boots
   the full in-process core exactly per roadmap decision §2.4 — one
   process, bootstrap, work, `shutdown()`, exit. Consequence, accepted:
   booting spawns the sync actor whose **first cycle fires
   immediately**, so every core command does one pull+push round-trip
   before exit (offline: the cycle fails, `SyncPhase::Offline` is set,
   local data still serves the command — offline-first is the product
   intent). Worst-case wall time of a core command is bounded by
   `[app] shutdown_timeout_secs` (the bounded actor join/abort), which
   tests tune to 2–5 s. *Rejected*: a "local-only boot mode" flag
   (second bootstrap shape, stale reads, doctrine divergence — backlog
   instead); read-only commands opening the repository directly
   (bypasses the single-writer engine discipline and the freshness the
   product wants).

4. **`login`: prompt → verify → persist non-secrets to TOML, secret to
   the store.** Flow: resolve the config *path* (same rule as `load`:
   `--config` > `TASKBOARD_CONFIG` > `./taskboard.toml`); a missing file
   at that path is **not** an error for `login` (it is the first-run
   bootstrap command and creates the file — other commands keep the
   `FileNotFound` behavior). Read the existing file, if any, **without
   validation** (new `pub(crate) AppConfig::extract_raw(path)` +
   `pub(crate) config_path(explicit)` extracted from `load`; a
   not-yet-valid config must not block login). Determine
   `server_url`/`username` from flags, else prompt interactively
   (plain stdin lines); prompt for the app password via the prompt seam
   (§4.4: `rpassword::read_password` in production — stdin-based, so
   piped E2E works; verify tty echo behavior at implementation time —
   `FakePrompts` in tests). Verify: `DeckClient::new(&server, &user,
   password)` + `boards()`; `Unauthorized | Forbidden` → typed
   `InvalidCredentials` (nothing persisted); `Transport` →
   `Unreachable`; other `DeckError`s pass through by variant. Persist:
   overlay `server_url`/`username` into the config via **`toml_edit`
   read-modify-write** (preserves comments, formatting, and unknown
   tables — the file is operator territory), `AppConfig::validate()`
   the result, write atomically (temp file + rename). Then the secret:
   `CredentialStore::store(server, username, password)` — the keyring
   path overwrites silently (re-login rotates; a server/username
   change writes a *new* `user@host` entry, the old one is inert). With
   `credential_store = "env"`, `store` returns `Unsupported`: login
   succeeds (verified + config written) and reports
   `stored: "env"` — the human text instructs exporting
   `TASKBOARD_APP_PASSWORD`; **no secret is ever written to any file**.
   The success payload is `{"server_url", "username", "verified": true,
   "stored": "keyring"|"env"}`. *Rejected*: writing the secret into the
   TOML (mechanically impossible since Phase 5 — right); storing the
   password in a local cache file (backlog's encrypted-file store owns
   that); refusing `env`-store logins (CI/E2E needs exactly this path).

5. **`boards`: list is a direct remote read; select is the one-time
   binding ceremony.** `boards list` builds a `DeckClient` from
   config+credentials (same construction as bootstrap; extract a
   `pub(crate) client_for(config, &store)` helper so `bootstrap`,
   `boards list`, and `login` share it), calls `boards()`, renders
   `remote-id | title | archived/deleted markers` (`--json`: an array
   of `{id, title, archived, deleted}`; ids are the **remote** Deck
   ids — that is what `select` consumes). `boards select <id>`:
   (a) pre-validate the id against a fresh `boards()` listing — an
   unknown, archived, or deleted id is a typed `UnknownBoard(u64)`
   **before** any core boot (a bogus id inside a sync cycle would only
   surface as the generic `Failed { Server }` via `classify_read` —
   §1); (b) boot the core; (c) if local state already has a live board
   bound to a **different** remote id → typed `BoardAlreadyBound
   { current: u64 }` (the single-board invariant, §1 — domain
   `live_board()` would otherwise silently target the wrong board);
   re-selecting the same id is a `NoOp` success; (d) send
   `SyncCommand::SetBoard(RemoteBoardId)` on `App::sync_commands()`;
   (e) wait for adoption — predicate-poll `shared_state` (driven by
   `EngineSignal::StateUpdated` via `subscribe()`, deadline-bounded,
   §3.7 helper) until some board has `remote == Some(id)`, or a sync
   `Failed` status lands, or the deadline (default 60 s,
   `--timeout-secs`) expires; (f) print the adopted board summary +
   sync status, `shutdown()`. The adoption itself persists the binding
   (the board row) — a later daemon resumes it from `read_state()`
   (§1). *Rejected*: allowing rebind with data wipe (destructive, not
   roadmap MVP); pre-seeding the binding via the storage API like the
   tests do (production ceremony goes through the real sync path).

6. **`tasks` verbs: exact selectors, append-by-default orders, engine
   receipts as output.** All verbs boot the core, resolve selectors
   against `shared_state()`, `execute()` the `StateCommand`, print,
   `shutdown()`. Selectors: a task is its **exact local `TaskId`**
   (uuid string; `tasks list --json` is the id source — scripting-first
   M1); a stack is `--stack <exact-uuid | exact-title>` — id match
   wins, then exact title; multiple live stacks sharing the title →
   typed `AmbiguousStack`; `tasks add`/`move` with `--stack` omitted
   requires **exactly one** live stack (else `NoStacks`/
   `AmbiguousStack`). Orders: `add` and `move` append by default
   (`max(order) + 1` in the target stack, 0 when empty — computed from
   the canonical `tasks_in_stack` order); `move` accepts an explicit
   `--order`. `tasks list` renders stacks in board order with their
   tasks in canonical order (`[done]`-style markers in text mode);
   optional `--stack` filter; the JSON payload embeds the current
   `SyncStatus` footer (§3.8). `tasks done <id>` sets done (repeat →
   `NoOp`, still exit 0, payload reflects state); `--undo` clears it.
   Mutation commands print the new/updated projection (task id, title,
   stack, order, done) — in `--json` mode machine-readable for the
   E2E's chaining. Durability contract, documented in `--help` and
   README: the `execute` reply means **durable locally** (outbox
   semantics); the boot cycle usually pushes immediately but the
   guarantee is "pushed by the next `sync`/daemon run" — the E2E chains
   an explicit `sync` where a push must be deterministic (§3.7).
   *Rejected*: prefix/short-id selectors (backlog); auto-`--wait` on
   mutations (doubles every command's latency; `sync` exists).

7. **`sync`: trigger, bounded wait, honest status.** Boot the core;
   snapshot the startup status `S0` (hydrated from the db — may be a
   stale `Idle`/`Failed`); `execute(RequestSync)` (belt-and-braces
   nudge; the boot cycle coalesces); wait via a shared helper —
   `wait_for_sync_result(&app, s0, deadline) -> SyncOutcome` — that
   awaits `EngineSignal::StateUpdated` (falling back to short-interval
   polling only if signals lag) and decides: `Succeeded` when
   `last_success > s0.last_success`; `Failed(SyncErrorKind)` when the
   observed phase is `Failed` *after* having seen a non-`S0` status (a
   transition into `Failed`, or any `Failed` differing from `S0`);
   `TimedOut` at the deadline (default 60 s, `--timeout-secs`).
   Documented honesty clause: a fresh failure **identical** to a stale
   persisted `S0` (same kind, no `last_success` advance) publishes no
   state change and is indistinguishable from "cycle still running" —
   it resolves as `TimedOut` (or `Failed` at deadline if the phase is
   `Failed`); the payload always shows the final status either way.
   `--no-wait` skips the wait (nudge + current status + exit 0). Exit
   code: `0` for `Succeeded`; `1` for `Failed`/`TimedOut` (payload on
   stdout, error kind on stderr per §3.8). The same helper serves
   `boards select`'s adoption wait (§3.5). *Rejected*: an
   `EngineCommand`-level "await next report" extension to the engine
   (cross-crate change for a CLI-presentational concern; the signal +
   status contract suffices and is tested at this layer).

8. **Output contract: typed projections, JSON on stdout, errors as
   JSON on stderr under `--json`.** `commands/output.rs` defines
   serde `Serialize` projection structs — never leaking domain serde
   casing — with kebab-case discriminators: `BoardsListEntry { id:
   u64, title, archived, deleted }`; `TaskRow { id, title, done,
   order, duedate, stack_id, stack_title, labels: Vec<String> }`;
   `TaskList { stacks: Vec<StackGroup { id, title, order, tasks:
   Vec<TaskRow> }>, sync: SyncStatusView }`; `MutationReceipt { id,
   title, stack_id, stack_title, order, done }`; `SyncStatusView {
   phase: "idle"|"syncing"|"offline"|"failed", last_success:
   Option<DateTime<Utc>>, pending_ops: u32, error_kind: Option<String>
   }`; `LoginReceipt { server_url, username, verified, stored }`;
   `SelectReceipt { board: BoardsListEntry-ish adopted summary, sync:
   SyncStatusView }`. Text mode renders the same data humanly (tables,
   one-line receipts). Under `--json`, runtime failures print
   `{"error":{"kind":"<kebab-case CommandError discriminator>"}}` to
   **stderr** and exit 1 — the kind string is the serialized typed
   variant (e.g. `invalid-credentials`, `unknown-task`, `board-
   already-bound`, `instance-locked`, `sync-failed` + nested kind),
   stable within M1, explicitly *not* versioned-guaranteed (backlog).
   Tests assert **structure** (parse with `serde_json::Value`, check
   kinds/fields/types) — never message text (AGENTS §5). `daemon`
   ignores `--json` (logs are its interface).

9. **Instance lockfile: OS advisory lock on a db sidecar, held by the
   core, not the CLI.** `lockfile.rs`: `InstanceLock::acquire(db_path)
   -> Result<InstanceLock, LockError>` opens/creates `<db_path>.lock`
   and takes an exclusive `fd-lock` write lock (`try_lock` — fail
   fast, never block), writing the holder PID into the file body
   (diagnostic only; the OS lock is authoritative and is **released
   automatically on process death** — no stale-lock recovery logic
   exists to get wrong, the classic O_EXCL pitfall). `bootstrap()`
   acquires it right after config validation and **before the db
   open** (fail before spawning anything); the guard lives in `App`
   and drops at `shutdown()`/process exit. Failure is a new
   `BootstrapError::InstanceLocked { db: PathBuf, holder_pid:
   Option<u32> }` (read best-effort from the file). Scope: any core
   boot locks (`daemon`, `boards select`, `tasks *`, `sync`); pre-core
   commands (`login`, `boards list`) do not. Per-database scoping
   answers the phase-5 §3.13 design input: worktrees/CI processes with
   distinct db paths never contend. Documented caveat: advisory locks
   on NFS/network filesystems are unreliable — sqlite on NFS is
   already unsupported territory; the error message points at the
   holder PID. *Rejected*: O_EXCL lockfile + staleness heuristics
   (racy, PID-reuse hazards); locking the db file itself (sqlite owns
   it; sidecar keeps concerns separate); a global lock in the data dir
   (breaks multi-db test isolation on one machine).

10. **New workspace dependencies, pinned once.** Root
    `[workspace.dependencies]`: `rpassword = "7"` (stdin-based password
    read, no tty requirement), `fd-lock = "4"` (cargo's own advisory
    locking crate; `libc`/`windows-sys` target deps only),
    `toml_edit = "0.22"` (already in the graph transitively — comment-
    preserving config read-modify-write), and `serde_json` promoted to
    a full dependency of the app crate (currently dev-only). All
    MIT OR Apache-2.0; `cargo deny check` + `cargo tree -p
    taskboard-app` verification are §9 gates. No `unsafe` enters our
    code (it lives in the deps; the app crate keeps
    `#![forbid(unsafe_code)]`).

11. **The E2E tier drives the real binary; hermetic tests drive the
    handlers.** Layering (one behavior, one test, lowest layer —
    AGENTS §5): handler logic is proven hermetically (L/V/K/O-series,
    wiremock + real sqlite tempdirs, fake stores/prompts); the
    J-series docker journeys prove only what only the binary can prove
    — arg parsing, stdin prompting, config-file discovery, env
    resolution, process exit codes, and the multi-process login →
    select → add → sync choreography against a real Deck server. J
    mechanics: `Command::new(env!("CARGO_BIN_EXE_taskboard"))` with
    `--config <tempdir>/taskboard.toml` (never `TASKBOARD_CONFIG`),
    per-child env via `Command::env` (`TASKBOARD_APP_PASSWORD` =
    tier token) and explicit `env_remove` for every `TASKBOARD_*`
    override the harness may have set in the test process
    (`TASKBOARD_CONFIG`, `TASKBOARD_NEXTCLOUD__*`, `TASKBOARD_SYNC__*`,
    …) so children cannot inherit foreign config; config content:
    `credential_store = "env"`, tempdir db, `poll_interval_secs = 1`,
    `shutdown_timeout_secs = 5`; stdin pipes the password for `login`.
    Assertions: exit codes + parsed `--json` structure + server-side
    truth via a raw `DeckClient` (as `it_docker.rs` does today).
    Isolation: `run_id()` board/card titles, per-test tempdirs,
    best-effort teardown, `with_deadline(300 s)`, `retry` for Deck's
    sporadic write-path 500s; the `docker-tier` nextest group
    serializes them with the existing tests. CI: the existing
    `nextcloud-integration.yml` app step matches
    `it_nextcloud_docker_cli_*` via its `-E 'test(it_nextcloud_docker)'
    filter — **no YAML change**; the §9 checklist verifies nextest
    builds the `taskboard` bin for `CARGO_BIN_EXE_` resolution (add a
    `cargo build -p taskboard-app --bin taskboard` step only if that
    proves necessary). Per-PR CI stays hermetic (the Docker-Hub
    rate-limit rationale stands).

12. **Observability.** `info!` — each command's start (name, resolved
    config path, json flag), login verification result (variant only,
    never the secret), lock acquisition, core boot/shutdown reuse of
    the phase-5 spans, `sync` wait outcome (`SyncOutcome` variant +
    final status); `warn!` — lock contention (holder PID), `TimedOut`
    conclusions; `error!` — command failures with typed kinds. Nothing
    secret or user-content above `trace`; no log text is ever
    test-asserted.

## 4. Normative Shapes (signatures are binding; bodies indicative)

### 4.1 `cli.rs` (additions)

```rust
#[derive(Debug, clap::Parser)]
pub struct Cli {
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    /// Machine-readable output on stdout; errors as JSON on stderr.
    #[arg(long, global = true)]
    pub json: bool,
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Verify and store Nextcloud credentials (first-run command).
    Login {
        /// Server URL (prompted when omitted).
        #[arg(long)]
        server: Option<String>,
        /// Login name (prompted when omitted).
        #[arg(long)]
        user: Option<String>,
    },
    /// Board discovery and the one-time single-board binding.
    Boards(BoardsCmd),
    /// Task verbs over the in-process core.
    Tasks(TasksCmd),
    /// Run one full sync cycle now and report the status.
    Sync {
        /// Return immediately after nudging the sync actor.
        #[arg(long)]
        no_wait: bool,
        /// Wait budget for the cycle (default 60).
        #[arg(long, default_value_t = 60)]
        timeout_secs: u64,
    },
    /// Run the headless sync daemon (M1).
    Daemon,
}

#[derive(Debug, Subcommand)]
pub enum BoardsCmd {
    /// List the server's boards (remote ids feed `boards select`).
    List,
    /// Bind this installation to one board (refuses a rebind).
    Select { board: u64 },
}

#[derive(Debug, Subcommand)]
pub enum TasksCmd {
    List { #[arg(long)] stack: Option<String> },
    Add { title: String, #[arg(long)] stack: Option<String> },
    Done { task: String, #[arg(long)] undo: bool },
    Move { task: String, #[arg(long)] stack: Option<String>,
           #[arg(long)] order: Option<i64> },
}
```

`boards select` also accepts `--timeout-secs` (shared wait budget,
default 60).

### 4.2 `commands/mod.rs`

```rust
/// Everything a handler needs; built by `main` once per invocation.
pub struct CommandCtx {
    pub json: bool,
    pub config_path: Option<PathBuf>,   // resolved by `config_path()`
}

/// Typed failure of one CLI invocation. `Display` is UX text (never
/// asserted); the `--json` discriminator comes from `error_kind()`.
#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("configuration rejected")]        Config(#[from] ConfigError),
    #[error("core bootstrap failed")]         Bootstrap(#[from] BootstrapError),
    #[error("credential store failed")]       Credential(#[from] CredentialError),
    #[error("engine rejected the command")]   Engine(#[from] ExecuteError),
    #[error("credentials rejected by the server")] InvalidCredentials,
    #[error("server unreachable")]            Unreachable,
    #[error("no such board")]                 UnknownBoard(u64),
    #[error("a different board is already bound")] BoardAlreadyBound { current: u64 },
    #[error("no such task")]                  UnknownTask(String),
    #[error("stack selector matches several stacks")] AmbiguousStack,
    #[error("no stacks to target")]           NoStacks,
    #[error("sync cycle failed")]             SyncFailed(SyncErrorKind),
    #[error("sync did not finish in time")]   SyncTimedOut,
    #[error("config file could not be written")] ConfigWrite(#[source] std::io::Error),
    #[error("prompt read failed")]            Prompt(#[source] std::io::Error),
}

impl CommandError {
    /// The stable kebab-case discriminator for the `--json` error shape.
    #[must_use] pub fn error_kind(&self) -> &'static str { /* … */ }
}

/// One subcommand run to completion. Returns the process exit code.
pub async fn dispatch(command: Command, ctx: CommandCtx) -> u8;
```

`dispatch` is the only item `main` calls (besides the daemon arm, which
reuses `bootstrap` + the existing signal lifecycle). Handlers internally:
`login::run`, `boards::{list, select}`, `tasks::{list, add, done,
move_}`, `sync::run`, sharing `wait_for_sync_result` and
`resolve_stack` helpers. Exit code `2` stays clap's; `dispatch` returns
`0|1`.

### 4.3 `lockfile.rs`

```rust
/// Exclusive advisory lock on `<db_path>.lock`, held for the process
/// lifetime (dropped with `App`). OS-released on death; the PID in the
/// file body is diagnostic only.
pub struct InstanceLock { /* fd-lock guard, path */ }

#[derive(Debug, thiserror::Error)]
pub enum LockError {
    #[error("another taskboard instance holds this database")]
    Held { holder_pid: Option<u32> },
    #[error("lock file could not be opened")]
    Io(#[source] std::io::Error),
}

impl InstanceLock {
    /// `try_lock` semantics: fail fast, never block.
    pub fn acquire(db_path: &Path) -> Result<Self, LockError>;
}
```

`BootstrapError` grows `InstanceLocked { db: PathBuf, holder_pid:
Option<u32> }` (constructed from `LockError::Held` at the bootstrap
call site, which knows the db path).

### 4.4 Prompt seam (`commands/login.rs`)

```rust
/// Input seam for interactive values (fakes over mocks; the E2E pipes
/// stdin through the production impl).
pub trait PromptReader: Send + Sync {
    /// One trimmed line (server URL / username).
    fn read_line(&mut self) -> Result<String, std::io::Error>;
    /// A secret (rpassword::read_password in production).
    fn read_secret(&mut self) -> Result<String, std::io::Error>;
}

pub struct StdinPrompts;                    // production
// tests/common: FakePrompts(Vec<String>)   // scripted answers
```

### 4.5 `commands/sync.rs` wait helper

```rust
/// Outcome of awaiting one fresh sync cycle after `S0`.
#[derive(Debug, Clone, PartialEq)]
pub enum SyncOutcome {
    Succeeded(SyncStatusView),   // last_success advanced past S0
    Failed(SyncStatusView),      // a fresh Failed phase observed
    TimedOut(SyncStatusView),    // deadline; final status reported
}

/// Event-driven (EngineSignal::StateUpdated) predicate poll with a
/// wall-clock deadline. Shared by `sync` and `boards select`.
pub async fn wait_for_sync_result(
    app: &App, startup: &SyncStatus, deadline: Duration,
) -> SyncOutcome;
```

## 5. Worktree, Branch & TDD Implementation Order

Branch `feat/app-cli-e2e`, worktree `../taskboard-WT-feat-app-cli-e2e`
(already created from `main` @ `4b45902`). One PR. TDD order — commit
per step, every commit passing the full gate (§9):

1. **Deps + skeleton**: root `Cargo.toml` (`rpassword`, `fd-lock`,
   `toml_edit`, `serde_json`), app manifest (promote `serde_json`),
   `commands/mod.rs` + `output.rs` empty shells, clap surface grown
   (§4.1) with handlers stubbed to `unimplemented!`-free typed errors;
   `main.rs` dispatch arms. Gates green (no behavior change yet).
2. **O-series**: projection types + `error_kind()` mapping table;
   serialization tests (structure, kebab-case, duedate/uuid formats).
3. **Lockfile** (K1, K2, K4 in-process) + `bootstrap` integration +
   `BootstrapError::InstanceLocked`; K3 cross-process hermetic binary
   test.
4. **`login`** (L-series): prompt seam, `extract_raw`/`config_path`
   refactor of `config.rs` (pure extraction — C-series must stay
   green), verify + persist + store paths, `--json` receipts, bad-
   credential and env-store arms.
5. **`boards list/select`** (V7–V9 + list): `client_for` helper,
   pre-validation, single-board refusal, `SetBoard` + adoption wait via
   `wait_for_sync_result` (build the helper here).
6. **`tasks` verbs** (V1–V4): selectors, append orders, receipts,
   `NoOp` semantics, list payload with sync footer.
7. **`sync`** (V5, V6): wait helper outcomes Succeeded/Failed/TimedOut,
   `--no-wait`, exit codes.
8. **J-series docker journeys** (J1–J3) + CI verification (§8/§9).
9. **Docs & bookkeeping** (§8): ADR 0009, testing strategy,
   architecture, README M1 quickstart, CHANGELOG, backlog appends;
   full gates + `cargo tree`/`cargo deny` + tier-2 run + tier-3
   runbook handoff (§9).

## 6. Test Plan (catalogue)

Deterministic by construction: hermetic tests use wiremock + real
sqlite tempdirs + `FakeCredentialStore`/`FakePrompts`, predicate polls
with deadlines, typed/structural assertions only. Live-tier tests use
wall-clock deadlines (sanctioned), `run_id` isolation, teardown
best-effort, and the `retry` helper. Env-mutating cases hold
`env_lock()` / use per-child `Command::env`.

- **O-series (output contract, `tests/cli_output.rs` + inline units)**:
  O1 payload serializations are structurally correct (task list nesting,
  stack groups in board order, canonical task order; uuid/ISO-8601
  formats); O2 `SyncStatusView` phase/error-kind casing table; O3
  `CommandError::error_kind()` covers every variant (exhaustive match —
  the compiler enforces new variants get kinds); O4 no payload type
  contains a secret-bearing field (compile-time by construction; a
  leak-floor test formats a receipt built from a known password and
  asserts the password is absent — same sanctioned class as the
  phase-5 redaction tests).
- **L-series (login, `tests/cli_login.rs`; wiremock boards endpoint)**:
  L1 happy path — `boards()` 200 with one board → exit 0; config file
  written at the resolved path containing server/user and **not** the
  password (read file, assert password substring absent — leak floor);
  `FakeCredentialStore` recorded `store(user@host, password)`; L2 401
  and 403 (separate tests) → `InvalidCredentials`, **no** file write,
  **no** store call (fake records prove absence); L3 unreachable
  (wiremock drop) → `Unreachable`; L4 env store → exit 0,
  `stored == "env"`, store untouched; L5 read-modify-write: pre-existing
  file with `[sync] poll_interval_secs` override and a comment → both
  survive, `nextcloud` keys updated; L6 missing `--server` prompts in
  order via `FakePrompts`; L7 login creates the file when the explicit
  `--config` path does not exist (typed contrast to `FileNotFound` for
  other commands); L8 invalid server string → client-construction
  failure kind, nothing persisted.
- **V-series (verbs, `tests/cli_verbs.rs`; wiremock full-cycle surface +
  real sqlite tempdir + seeded bindings via `seed_board_binding`)**:
  V1 `tasks add` with no board → `NoBoard` kind, exit 1; V2 seeded
  board+stack → `add` → receipt id; fresh bootstrap (second "process",
  same db) sees the task (durability across invocations); `--stack` by
  uuid and by exact title both target it; V3 `done` then `done --undo`
  → receipts reflect both; repeat `done` → `NoOp`-shaped success
  (exit 0); V4 `move` to a second seeded stack appends (`order` =
  target max + 1; explicit `--order` honored); V5 `sync` happy cycle
  (scripted wiremock pulls + pushes) → `Succeeded`, `pending_ops == 0`;
  the boards mock's Basic-auth header decodes to
  `user:TASKBOARD_APP_PASSWORD` (the phase-5 B2 proof, now through the
  CLI seam); V6 `sync` failure: wiremock 500s → `Failed(server)`; an
  unroutable URL + `--timeout-secs 2` → `TimedOut`; V7 `boards select`:
  pre-validation hit + `SetBoard` + adoption observed + `SelectReceipt`
  (the wiremock board + stacks are in local state); V8 `select` unknown
  id → `UnknownBoard`, **no** `SetBoard` sent (mock records absence);
  V9 `select` with an existing different binding → `BoardAlreadyBound`
  (current id in payload); same id → no-op success; V10 ambiguous stack
  title → `AmbiguousStack`; zero stacks + `add` without `--stack` →
  `NoStacks`.
- **K-series (lockfile, `tests/lockfile.rs` + `tests/cli_lock.rs`)**:
  K1 second `bootstrap` on the same db while the first `App` lives →
  `InstanceLocked`; K2 after `App::shutdown`, bootstrap succeeds; K3
  **cross-process, hermetic**: spawn the real binary (`daemon`, env
  store, tempdir config, unreachable server — boots fine offline), then
  a second `tasks list` invocation → exit 1, kind `instance-locked`
  (stderr JSON); SIGTERM the daemon, wait, retry succeeds; K4 the
  sidecar is `<db>.lock` (structural); K5 two bootstraps with different
  db paths coexist.
- **J-series (docker tier 2, `tests/it_cli.rs`; `#[ignore]`,
  `it_nextcloud_docker_cli_*`, skip-never-fail, LIVE_DEADLINE = 300 s,
  serialized by the existing nextest group)**:
  - **J1 `…_login_select_add_verify`** (the M1 journey in miniature):
    create board + stack server-side via raw `DeckClient`
    (`create_board_and_stack`); `login --server --user` with the tier
    token piped on stdin → exit 0, config exists in the tempdir;
    `boards list --json` → the board id is present (structure);
    `boards select <id> --json` → exit 0; `tasks add "<run>-task"
    --stack <title> --json` → receipt; `sync --json` → `Succeeded`,
    `pending_ops == 0`; **verify remote**: raw `DeckClient` sees the
    card with the title (push proof); `tasks list --json` → the task
    present, `done == false`, sync footer healthy; `tasks done <id>
    --json`; `sync`; verify the done state survived the round-trip per
    the Deck mapping (remote card read → re-pull → still done);
    final `tasks list --json` consistent.
  - **J2 `…_remote_create_pull_list`**: server-side card via raw client
    → `sync --json` (Succeeded) → `tasks list --json` contains it with
    the right stack (pull proof) → `boards list` unaffected.
  - **J3 `…_login_rejects_bad_token`**: `login` with a garbage password
    piped → exit 1, stderr JSON kind `invalid-credentials`; the tempdir
    contains **no** config file and no lock sidecar (nothing booted).
- **Doc-tests**: `commands::dispatch` usage sketch (build a `CommandCtx`,
  call a handler against a fixed store), `InstanceLock::acquire`
  acquire/conflict sketch, `wait_for_sync_result` contract doc.

Coverage: llvm-cov target = `commands/`, `lockfile.rs`, `output.rs`.
Note (documented, not solved): binary-driving tests (K3, J-series) run
the work in subprocesses llvm-cov does not instrument — the hermetic
handler tests carry the coverage duty for that logic; K3/J exist to
prove wiring, not to move the coverage number. Miri/mutants scope
unchanged (domain + state only).

## 7. Out of Scope → Backlog (entries appended with this plan)

- **Task selectors beyond exact uuid** (`tasks done 018f…-abc` prefix
  match, `--title`, ordinals) — DX; exact ids are scripting-sufficient
  for M1.
- **`tasks edit/delete`, `stacks`/`labels` subcommands** — the domain
  commands exist (`UpdateTask`, `DeleteTask`, stacks/labels verbs);
  the CLI surface is deliberately minimal for M1.
- **`logout`** — `CredentialStore::delete` ships; the subcommand is a
  one-liner once anyone needs it.
- **`taskboard status` without triggering a cycle** — impossible under
  the uniform-core doctrine (boot = immediate cycle); needs either a
  no-sync boot mode or daemon IPC (existing entry).
- **JSON schema stability/versioning** — payloads are internal-but-
  scriptable; a `--json=versioned` contract is premature until
  consumers exist.
- (Existing entries stay; the signal-escalation entry from phase 5
  remains adjacent to — but separate from — the exit-code mapping
  shipped here.)

## 8. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0009 — "CLI surface, instance lock, and the binary-driving E2E
  tier"**: the two command classes (pre-core vs core-booting) and why
  every core command rides one bootstrap (roadmap §2.4); the login
  ceremony (verify-then-persist, toml_edit preservation, env-store
  semantics — no secret ever hits a file); the single-board invariant
  and `BoardAlreadyBound`; the output contract (stdout payloads,
  stderr error JSON, exit codes, kebab-case kinds) and its
  explicitly-non-guaranteed stability; the lockfile decision
  (db-scoped sidecar, OS-released, no staleness heuristics, NFS
  caveat); the J-series tier rationale (what only the real binary can
  prove) and its place in the testing strategy. Status: Accepted.
- **`docs/testing_strategy.org`**: new subsection under §8 —
  *App-level E2E (binary-driving, tier-2 reuse)*: the
  `it_nextcloud_docker_cli_*` filter, `CARGO_BIN_EXE_taskboard`,
  per-child env hygiene (`Command::env` + `env_remove` of
  `TASKBOARD_*`), tempdir config isolation, and the hermetic-vs-live
  layering rule (handlers hermetic; journeys prove wiring only).
- **`docs/architecture.org`**: app-crate bullet gains the `commands/`
  tree + lockfile; the dependency list gains `rpassword`, `fd-lock`,
  `toml_edit`, `serde_json`.
- **README.org**: replace the phase-5 placeholder-style M1 quickstart
  with the real ceremony: `taskboard login` (interactive) →
  `taskboard boards list` / `select` → `tasks add` → `sync` →
  `daemon`; the env-store variant for headless boxes; the
  `--json` scripting note.
- **CHANGELOG** `[Unreleased]` → Added: the CLI subcommands, `--json`
  contract, instance lockfile, J-series E2E tier; Changed: `login`
  replaces the manual `secret-tool` dance from the phase-5 quickstart.
- **Backlog**: append §7 entries.
- **CI**: expected **no workflow change** (J-series matches the
  existing filter); if the §9 check shows nextest does not build the
  bin for `CARGO_BIN_EXE_`, add a `cargo build -p taskboard-app --bin
  taskboard` step to `nextcloud-integration.yml` only.

## 9. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`;
  `cargo clippy --workspace --all-targets -- -D warnings`; `cargo
  nextest run --workspace`; `cargo test --doc --workspace`; `cargo deny
  check`; `cargo llvm-cov --workspace`. (`SQLX_OFFLINE=true` as CI
  does; no new sqlx queries ship.)
- Dependency verification: `cargo tree -p taskboard-app` shows
  `fd-lock`/`rpassword`/`toml_edit` with no unexpected weight; `cargo
  deny check` green with the new graph.
- Hermetic suites: O/L/V/K green under `cargo nextest run -p
  taskboard-app` (plus phase-5 C/S/B/T suites untouched-green — the
  `config.rs` extraction refactor is covered by them).
- **Tier 2 (automated phase-exit)**: `eval "$(scripts/
  nextcloud_it_setup.sh up)" && cargo nextest run -p taskboard-app
  --run-ignored only -E 'test(it_nextcloud_docker)'` green — now
  including the three J-series journeys; confirm the nextest invocation
  resolved `CARGO_BIN_EXE_taskboard` (else add the §8 build step);
  dispatch `nextcloud-integration.yml` on the PR branch as the CI
  proof; `scripts/nextcloud_it_setup.sh down` afterwards.
- **Phase 6 exit = M1 (roadmap §6, user-run tier-3 runbook against the
  live server)**: on a clean machine/profile: (1) `taskboard login`
  (real keyring — prompts only; verify the `secret-tool` replacement
  works end to end); (2) `taskboard boards list` shows the real
  account's boards; `boards select <id>`; (3) with the network
  **down**: `tasks add "offline task"` → exit 0 (durable locally);
  (4) network up: `taskboard sync` → the task appears in the Nextcloud
  web UI; (5) reverse direction: create a card in the web UI →
  `taskboard sync` → `tasks list` shows it; (6) `taskboard tasks done
  <id>` → `sync` → web UI shows it done; (7) start `taskboard daemon`,
  verify a second `taskboard tasks list` fails with
  `instance-locked`, Ctrl-C the daemon; (8) record any known-server
  matrix notes in `testing_strategy.org`. M1 acceptance is reached
  when the user confirms this runbook passes.

## 10. Consumer Handoff Map

| Consumer | Takes from Phase 6 |
|---|---|
| Phase 7 UI | The CLI becomes the second frontend proving the "one bootstrap, N frontends" pattern; `login`/`boards select` have already done first-run by then, so the UI mounts on a bound, synced core; `commands/output.rs` projections double as the UI's view-model seed |
| Daemon IPC (backlog) | The `--json` payload + error-kind contract is the natural wire format draft; exit-code semantics map onto IPC result codes |
| Scripting/kiosk tooling | `--json` on every verb; lock errors are machine-distinguishable (`instance-locked`) for supervisors |
| Release v0.1.0 | M1 exit unblocks the Phase 7 → `bump_version.sh` path; no version bump ships in this phase |

## 11. Risks & Mitigations

- **Docker-tier flakiness (Deck write-path 500s, PHP single-server
  contention)**: `retry` helper, `run_id` isolation, the existing
  `max-threads = 1` group, 300 s deadlines, teardown best-effort — all
  inherited from the proven T-series mechanics.
- **Child-process env leakage** (a sibling test's `TASKBOARD_*` in the
  shared process env reaching the binary): every J/K spawn sets its
  config via `--config` (file), the password via per-child
  `Command::env`, and explicitly `env_remove`s the `TASKBOARD_*`
  surface; hermetic env tests keep using `env_lock()`.
- **`rpassword` tty behavior on piped stdin**: we use the stdin-based
  read (not the tty-opening prompt APIs); J1 pipes the password and is
  itself the regression proof; implementation duty to confirm echo
  suppression on a real terminal during the tier-3 runbook.
- **Every core command pays a network cycle** (immediate boot cycle +
  bounded shutdown join): offline is a first-class outcome (Offline
  status, local data serves, exit 0); the wait is bounded by
  `shutdown_timeout_secs`; a mid-cycle abort at the timeout is the
  already-tolerated crash window (outbox re-pushes next boot). UX
  documented; tests tune the timeout down.
- **`sync`'s stale-identical-failure ambiguity** (fresh failure ==
  persisted S0 publishes nothing): resolved honestly as `TimedOut`
  with the final status always reported; the contract is pinned by V6
  tests rather than papered over with an engine change.
- **`fd-lock` cross-platform semantics** (LockFileEx vs flock):
  same-crate abstraction cargo itself relies on; Linux is the only CI
  platform that *runs* it, the release matrix only *compiles* the
  Windows/macOS arms; K3 exercises the real contention path on Linux.
- **Lockfile vs long-running daemon UX** (`boards select` while the
  daemon runs → `instance-locked`): typed, documented in `--help` and
  the runbook — stop the daemon first; the alternative (queued
  commands) is daemon IPC, a backlog entry.
- **Scope creep** (edit/delete/labels/logout/selectors/status): each
  was deliberately triaged to §7; the domain commands exist, so later
  phases are additive.
