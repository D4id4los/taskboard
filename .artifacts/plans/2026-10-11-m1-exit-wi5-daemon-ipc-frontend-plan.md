# [Plan] M1-Exit WI-5 — Daemon IPC Frontend (Local-Socket Command Transport)

- **Date**: 2026-10-11
- **Target**: `taskboard-app` (new `src/ipc/` module family, `main.rs`,
  `cli.rs`, `commands/mod.rs`, `repl.rs` one mechanical refactor,
  `commands/output.rs`, root `Cargo.toml` + `crates/taskboard-app/Cargo.toml`
  for one new workspace dep; tests: new IP-series + binary E2E suite +
  wiremock daemon row; new `contrib/taskboard.service`). **No changes** to
  `taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`, `taskboard-ui-slint` — IPC is a new transport
  over the existing core (ADR 0008 "one bootstrap, N frontends").
- **Goal**: implement M1-exit report item 5 (direction D): while the daemon
  runs, every core verb (`tasks`, `stacks`, `labels`, `boards …`, `sync`) is
  served over a local-socket IPC channel instead of failing
  `instance-locked` or re-booting the core — killing the daemon-mutex
  friction and paying per-command boot cost only when no daemon serves the
  database. Two new operator verbs ride the channel: `taskboard status`
  (live sync status, never boots a core, never triggers a cycle) and
  `taskboard daemon stop` (graceful remote shutdown — the kiosk management
  primitive). Rides along: the systemd unit template (existing backlog
  entry) and the first versioned wire envelope (the versioned-JSON-schema
  backlog entry's stated trigger).
- **Status**: CONFIRMED — user review 2026-10-11: all §2 decisions
  approved as proposed, including the standalone-gate relaxation (the
  ADR 0010 decision-9 amendment, §2.2). Implementation may proceed in
  §6 order.
- **Parent roadmap**:
  `.artifacts/reports/2026-10-11-m1-exit-pre-gui-analysis-report.md` §4D +
  §7 row 5; backlog entries "[2026-10-08] Daemon IPC for remote control
  (kiosk management)", "[2026-10-10] `taskboard status` without triggering
  a cycle", "[2026-10-10] Systemd/journald packaging", "[2026-10-10]
  Versioned `--json` schema".
- **Predecessors**: WI-0 through WI-4 merged (`main` @ `8e1b5b1`).
  WI-4's consumer-handoff map named this work's inputs: the `*_on(&App)`
  seam + `dispatch_line`, and `CommandIo` as the socket sinks.
- **Branch / worktree**: `feat/app-daemon-ipc`, worktree
  `../taskboard-WT-feat-app-daemon-ipc` (created from `main` @ `8e1b5b1`).
  One PR for the plan + implementation (phase 4/5/6/6b/WI-3/WI-4
  precedent).

---

## 1. Context & Known Facts (do not re-derive; verified on `main` @ `8e1b5b1`)

- **The seam WI-4 paid forward is complete.** Every core verb has an
  `*_on(&App, …)` body; `dispatch_line(session: &ReplSession, app: &App,
  command, io) -> u8` routes any parsed `Command` against a booted core
  (commands/mod.rs:720-1009) and never exits. `CommandIo { out, err, json }`
  already carries `&mut dyn Write` sinks — its doc comment says verbatim:
  "the future daemon-IPC frontend (WI-5) will inject socket sinks"
  (commands/mod.rs:54-62). IPC is a third caller of this table, not a new
  dispatch layer.
- **The daemon today is signal-only.** `daemon_lifecycle` (main.rs:142-189)
  boots the `App`, spawns `spawn_system_event_logger`, then blocks in
  `wait_for_signal()`; SIGTERM/ctrl_c → graceful `app.shutdown()`. Nothing
  serves requests while it runs. It **refuses** the resolved `standalone`
  backend at entry with typed `sync-disabled` (main.rs:153-158, "The
  daemon's entire purpose is the connector" — ADR 0010 decision 9).
- **The line-parse layer is nearly reusable as-is.** `repl::resolve_line`
  (repl.rs:225-258) does: shell-word split → `Cli::try_parse_from` →
  REPL-level rules, returning `LineAction::{Execute(Command, json),
  Refuse(LineRefusal), PrintClap(clap::Error), Exit, Reprompt}`. The split
  step is the only REPL-specific part; an argv-taking sibling for IPC is a
  mechanical extraction (§4.6). `LineRefusal` (repl.rs:165-203) already
  covers `daemon`/`login`/`logout`/`config`/`--config` — exactly the verbs
  IPC must also refuse.
- **One-line refusals in `dispatch_line`**: `Command::{Daemon, Login,
  Logout, Config}` are `unreachable!` (commands/mod.rs:728-732) — callers
  must intercept before routing. The IPC server inherits this contract.
- **Exit-code and payload contract** (ADR 0009): `dispatch`/`dispatch_line`
  return `0` success / `1` runtime failure; clap usage errors are `2`
  (printed by clap at process level today, never inside dispatch);
  `emit_to` writes payload-or-text lines on `out`, `report_to` writes
  `{"error":{"kind","detail"}}` on `err` under `--json` (commands/mod.rs:
  1044-1076). Wire fidelity = relaying these three things (code, out, err)
  verbatim.
- **The instance lock** (lockfile.rs): `InstanceLock::acquire(&db)` takes
  an OS advisory exclusive lock on `<db>.lock` (sidecar naming:
  `taskboard.db` → `taskboard.lock`, lockfile.rs:134-141), writes the
  holder PID into the body (diagnostic only, best-effort read via
  `read_holder_pid`, lockfile.rs:144-148), held for process lifetime,
  released by the OS on death. `bootstrap()` acquires it before opening
  the db (bootstrap.rs:263-269) and `App` owns it — so daemon ⇒ no second
  core on that db, by construction. **The socket sidecar follows this
  precedent: `<db>.sock`** (§4.4).
- **Engine surface is all the server needs**: `EngineHandle` (state actor
  core, actor.rs:73-77) exposes `execute` (durable receipt), `dispatch`
  (fire-and-forget), `shared_state` (ArcSwap load), `subscribe`
  (EngineSignal broadcast) — all reached through `&App`. `SyncStatusView`
  (commands/output.rs:79-109) is the existing JSON projection of sync
  phase/last_success/pending_ops/error_kind; `sync_on` (commands/sync.rs:
  72-86) and `wait_for_sync_result` (sync.rs:99-138) are `&App`-shaped
  already.
- **`ReplSession`** (repl.rs:152-160) = `{ config: AppConfig, store:
  Arc<dyn CredentialStore>, json_default: bool }` — the cross-line state
  `dispatch_line` needs. The IPC server needs the same {config, store};
  only `json_default` is REPL-specific (§4.5 refactor).
- **`App` is not `Clone` and `shutdown(self)` consumes it**
  (bootstrap.rs:127-143, 178-200); `EngineHandle`/`StorageHandle`/broadcast
  senders inside are cloneable, but the shutdown discipline is
  consume-once. The daemon lifecycle owns `App`; the IPC server must share
  it across tokio tasks ⇒ `Arc<App>` at the daemon layer only (§4.7).
- **Config → db → socket is a pure derivation**: `AppConfig::config_path`
  (config.rs:283-291) resolves `--config` > `TASKBOARD_CONFIG` > XDG-if-
  present > `./taskboard.toml`; `db_path()` (config.rs:402) is filled by
  `extract_raw` (config.rs:308-310). A client that resolves config the
  same way derives the same socket path as the daemon — per-db names,
  worktrees and CI never collide (the lock sidecar's argument verbatim).
- **`wiremock` is already an app-crate dev-dependency**
  (crates/taskboard-app/Cargo.toml `[dev-dependencies]`) — a hermetic
  connector-backend daemon (auth-failing stub server) costs no new test
  deps.
- **Test-series letters in use**: L/V/K/O (hermetic handler suites),
  ST (standalone journeys), D (dispatch routing), R (REPL), J (docker
  binary journeys), pty suite in `cli_tty.rs`. New suites take **IP**
  (in-process IPC) and a binary E2E file (§6 steps 5-6).
- **Dependencies**: transport needs cross-platform local sockets
  (first-class Linux **and** Windows releases, AGENTS §1). `tokio` UDS is
  unix-only; the workspace-standard answer is **`interprocess` 2.2**
  (MIT OR Apache-2.0, `tokio` feature: async local sockets = UDS on unix,
  named pipes on Windows). Both licenses are already in `deny.toml`'s
  allow list (deny.toml:10-11) — **no deny.toml change needed** (contrast
  rustyline's BSL exception, commit b81bbda). Framing is hand-rolled
  (serde_json is in-tree); no other new deps.
- **Coverage floor**: re-measure app-crate line coverage at branch point
  (M1-exit snapshot was 80.33% pre-WI-2; WI-3/4 raised it) and treat that
  as the floor the IP-series must not regress (§9).

## 2. Product Decisions (all confirmed at review, 2026-10-11)

1. **Confirmed — Transparent auto-routing for core verbs.** When a `taskboard
   <core-verb>` invocation finds a daemon serving that db's socket, the
   request is served over IPC; when no daemon answers, the invocation
   boots the core in-process exactly as today. Core verbs: `tasks *`,
   `stacks *`, `labels *`, `boards select|create|rename|publish`, `sync`.
   Pre-core verbs (`login`, `logout`, `config print`, `boards list`) stay
   client-side always (no lock, no core, unchanged behavior — `logout`
   while a daemon runs is permitted and documented: the daemon keeps its
   booted credentials until restart). Routing is silent in both human and
   JSON modes (`tracing::debug` only): the observable contract (exit code,
   stdout payload, stderr envelope) is byte-identical to the in-process
   path. Rationale: the backlog entry's own framing — "the CLI surface
   stays unchanged, only the transport of commands differs".
2. **Confirmed — Relax the daemon's `standalone` refusal** (amends ADR 0010
   decision 9; the refusal lives at main.rs:153-158). Recommended: **yes**
   — with IPC the daemon's purpose is "own the core, serve commands",
   which is meaningful under both backends; a standalone daemon gives
   scripters the same no-boot-per-command latency the connector profile
   gets, and it is what makes the binary E2E tier hermetic (§6 step 5).
   The old comment's premise ("idling without a connector") stops being
   true the moment the socket listener exists. Rejected alternative:
   keep the gate and test the daemon tier only against a wiremock
   auth-failing server (works — wiremock is a dev-dep — but tests the
   feature on the exotic path, not the common one). Confirmed at review
   (2026-10-11): the gate is removed (§6 step 5); the wiremock row
   (§6 step 6) additionally proves the connector-backend daemon.
3. **Confirmed — Two new operator verbs ride the channel**:
   - `taskboard status` — answers *who serves this db and how is sync*:
     with a daemon, `{daemon: true, version, backend, pid, sync:
     SyncStatusView}` read live from the engine (no cycle triggered);
     without, `{daemon: false, holder_pid?}` from the lock sidecar's
     diagnostic PID (never boots a core, never probes the lock — the
     backlog entry's "no silent network I/O" requirement). Exit 0 in both
     cases (an observation, not a failure); `--json` follows the global
     flag.
   - `taskboard daemon stop` — sends the graceful-shutdown control
     request; the daemon acks (response code 0), then runs its normal
     signal-path shutdown (drain IPC → socket removed → `app.shutdown()`).
     Without a daemon: typed failure `no-daemon`, exit 1. CLI shape:
     `Daemon` grows an optional subcommand (`daemon [stop]`; bare `daemon`
     still runs the daemon — §4.9).
4. **Confirmed — Wire protocol: one versioned JSON envelope per request,
   length-framed; argv in, rendered text out.** A request carries the raw
   argv words (stable wire — immune to clap type churn); the *server* is
   the single parse authority (`Cli::try_parse_from`, exactly like the
   REPL). The response carries `{code, out, err}` — the exact bytes the
   in-process dispatch would have printed and the exact exit code — so the
   client is a dumb relay and the ADR 0009 contract survives transport
   unchanged. v1 is **one request per connection** (connect → frame →
   response frame → close; a CLI process is one command; UDS setup is
   ~free); keep-alive/pipelining is a protocol v2 concern.
5. **Confirmed — Envelope versioning now, payload versioning stays
   deferred.** The wire envelope carries `"v": 1` in both directions and a
   mismatched client gets a typed `VersionMismatch` response (§4.2) —
   this satisfies the versioned-JSON-schema entry's *trigger* (first IPC
   consumer) at the layer that actually crosses a version boundary. The
   `--json` payload shapes inside `out` remain explicitly unversioned
   (same-binary client; external consumers are the payload-versioning
   trigger, unchanged). Backlog entry gets a status note, not DONE.
6. **Confirmed — The socket is derived from the db, hardened to owner-only,
   and self-healing.** Path: `<db>.sock` sidecar (unix); namespaced name
   `taskboard-<fnv1a64(abs-db-path)>.sock` (Windows named pipes, §4.4).
   Unix: chmod 0600 after bind (single-user-per-db is already the lock
   model); stale socket file (crashed daemon) is recovered at next daemon
   boot (bind fails → try connect → nobody answers → remove file → rebind
   once); graceful shutdown removes the file. Namespaced names cannot go
   stale (no filesystem artifact) — first live listener wins.
7. **Confirmed — OS object permissions are the whole auth story in v1.**
   No tokens, no handshake auth: a local socket restricted to the db's
   owner is exactly as trusted as the `taskboard.toml` + db + lock files
   beside it (same user, same machine). Kiosk = single-user by design.
   Documented in ADR 0012; a multi-user daemon knob is backloged (§7).
8. **Confirmed — Requests execute serially, like every other actor.** The
   engine is a single-servant loop; IPC requests queue behind it
   (dispatcher task owns `&App`, one in-flight request). Concurrent
   *connections* are accepted (a second client waits, it does not error).
   Consequence documented: one long `sync --timeout-secs N` holds the
   queue. This is the same fairness model the engine already gives the
   REPL loop and the sync actor.
9. **Confirmed — REPL unchanged; mutex story unchanged.** The REPL boots
   its own core and holds the lock (ADR 0011); it does not attach over
   IPC in this phase. `daemon stop`/`status`/IPC refusals inside a REPL
   session: `daemon` (any form) is already refused by `resolve_line`;
   `status` inside the REPL routes like any other line against the
   session core (client-side answer shape; the session *is* the daemon —
   `daemon: false, holder_pid: <own pid>` is honest and cheap).
10. **Confirmed — In-session `--config` over IPC is refused server-side**
    (code 2, guidance text on `err` — same class as the REPL's
    `ConfigFlag` refusal): the socket *is* the config binding; a daemon
    serves exactly the core it booted. The client never needs to police
    this — a `--config other.toml` invocation resolves *other.toml*'s db,
    derives *that* socket, finds no daemon there, and runs in-process.
    The refusal only catches the degenerate same-db-different-file case.

## 3. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Protocol | `src/ipc/protocol.rs`: versioned `Request`/`Response` enums, `PROTOCOL_VERSION = 1`, typed mismatch arm; serde-tagged shapes frozen by unit tests |
| Codec | `src/ipc/codec.rs`: u32-LE length-prefixed JSON frames, size caps (request 64 KiB / response 16 MiB), truncation/garbage refusal; unit-tested round-trips |
| Naming | `src/ipc/mod.rs`: socket-name derivation (unix fs sidecar `<db>.sock`; Windows namespaced `taskboard-<fnv1a64>.sock`), hand-rolled FNV-1a (stable across compiler versions — `DefaultHasher` is not), unit tables |
| Server | `src/ipc/server.rs`: bind (+ stale-file recovery, unix perms), accept loop, per-connection read → dispatcher queue → respond; dispatcher owns `Arc<App>` + session; `Status`/`Shutdown` control arms; graceful drain handle |
| Client | `src/ipc/client.rs`: connect-with-budget, request round-trip, typed transport failures (`CommandError::{NoDaemon, IpcTransport, IpcVersion}`) |
| Routing | `commands/mod.rs`: `route_for_ipc(&Command) -> Route` pure table + `main.rs` auto-detect wiring (raw argv captured pre-clap); argv-to-action extraction shared with the REPL (§4.6) |
| Verbs | `cli.rs`: `Status` subcommand; `Daemon { cmd: Option<DaemonCmd::Stop> }`; `commands/status.rs` client- and server-side payload + render; D-series routing rows |
| Refactor | `ReplSession { config, store, json_default }` → `CoreSession { config, store }` + REPL wrapper; `dispatch_line(&CoreSession, &App, …)` (behavior-preserving, zero existing-test changes) |
| Gate | Remove the daemon's standalone refusal (confirmed §2.2; ADR 0010 amendment) |
| Tests | IP-series (`tests/ipc.rs`, hermetic standalone in-process server+client); binary E2E (`tests/cli_ipc.rs`: real daemon + real client, fallback row, stop journey); wiremock connector-daemon row; codec/naming/routing unit suites |
| E2E audit | Coverage-matrix pass over every user-facing journey × test layer once the new surface lands; fill genuine binary-level gaps (process/transport/lifecycle glue only, AGENTS §5); audit report in `.artifacts/reports/` (§6 step 7) |
| Packaging | `contrib/taskboard.service` systemd user-unit template + install notes |
| Docs | ADR 0012; `architecture.org` (ipc module + frontend + tech-stack rows); README (daemon, status, stop, auto-routing, systemd); CHANGELOG; backlog bookkeeping (4 entries); roadmap amendment |

**Out of scope** (backlog §7): keep-alive/pipelined connections; event/
push subscriptions over IPC (Phase 7 UI reads `ArcSwap` in-process; a
remote-UI event stream is its own design); multi-user/socket-path config
knobs; graceful Ctrl-C cancellation of in-flight IPC requests (signal-
escalation entry stays separate); IPC over TCP/ssh (remote *network*
management is explicitly not v1 — "local" is the security boundary);
`boards list` over IPC; REPL-attaches-to-daemon mode; payload-schema
versioning; Windows CI rows (validation rides the release pipeline).

### 3.1 Entry surface after this phase

```
# core verbs — transport is auto-negotiated per invocation:
taskboard tasks add "x"        # daemon running? → served over IPC (no boot)
                               # no daemon?      → in-process boot (today's path)
taskboard sync                 # over IPC: nudges the *daemon's* sync actor

taskboard status [--json]      # NEW: daemon/backend/pid/live SyncStatusView,
                               #          or {daemon:false, holder_pid?}; never boots
taskboard daemon               # unchanged (plus: now serves IPC while running)
taskboard daemon stop          # NEW: graceful remote shutdown (no-daemon → exit 1)
taskboard daemon --no-ipc      # NOT in v1 (always-on; backlog knob)

# daemon lifecycle (either backend): boot → bind <db>.sock → serve →
#   SIGTERM | `daemon stop` → drain in-flight → remove socket → app.shutdown()
```

## 4. Design Decisions

1. **Relay-rendered-text, not typed payloads, crosses the wire.** The
   server runs the real routing stack (`dispatch_line` into injected
   `Vec<u8>` buffers) and returns `{code, out, err}` strings. One wire
   shape covers every verb — including clap usage errors (rendered, code
   2), help (code 0), refusal guidance (code 2), and `--json` payloads
   (they are just `out` lines) — with zero duplication of the projection
   layer. The client writes `out`→stdout, `err`→stderr, `exit(code)`.
   Byte-identical contract to in-process is *load-bearing*: it is why
   auto-routing (§2.1) can be silent.
2. **Protocol shapes** (serde, internally tagged by `"type"`):

   ```rust
   pub const PROTOCOL_VERSION: u32 = 1;

   #[derive(Serialize, Deserialize)]
   #[serde(tag = "type", rename_all = "kebab-case")]
   pub enum Request {
       /// Run one CLI invocation: argv words *after* the binary name,
       /// exactly as `std::env::args().skip(1)` produced them.
       Exec   { v: u32, argv: Vec<String> },
       /// Live status snapshot (never mutates, never syncs).
       Status { v: u32 },
       /// Graceful shutdown request (the `daemon stop` backend).
       Shutdown { v: u32 },
   }

   #[derive(Serialize, Deserialize)]
   #[serde(tag = "type", rename_all = "kebab-case")]
   pub enum Response {
       /// Exec outcome: process-shaped (0/1/2) + rendered streams.
       Exec   { v: u32, code: u8, out: String, err: String },
       /// Status answer (server side, live).
       Status { v: u32, status: StatusView },
       /// Shutdown ack — the daemon begins its drain after sending.
       Shutdown { v: u32 },
       /// Any request whose `v` ≠ PROTOCOL_VERSION gets this, verbatim
       /// and parseable by any client that knows *any* version.
       VersionMismatch { server_v: u32 },
   }
   ```

   `StatusView` (new in `commands/output.rs`, serde all-`Option` with
   `skip_serializing_if`): `{ daemon: bool, version?: String, backend?:
   "standalone"|"nextcloud", pid?: u32, sync?: SyncStatusView,
   holder_pid?: Option<u32> }` — one shape for both answers, `daemon`
   discriminating. Version handling is asymmetric on purpose: the
   *response* is version-tagged so an old client talking to a new daemon
   gets a typed `ipc-version` failure, never a misparse.
3. **Framing**: `u32` little-endian byte length, then exactly that many
   UTF-8 JSON bytes; one frame each way; writer flushes after the frame;
   reader enforces the caps and treats over-length/truncated/undecodable
   as a transport failure (drop connection — there is nothing salvageable
   in a garbage stream). Caps: request 64 KiB (argv), response 16 MiB
   (a `tasks list` of a very large board). No compression (local socket).
4. **Socket naming & lifecycle** (mirrors the lock sidecar argument):

   ```rust
   /// unix: <db-dir>/<stem>.sock   (taskboard.db → taskboard.sock)
   /// windows: namespaced "taskboard-<fnv1a64(abs-db)>.sock"
   pub fn socket_name(db_path: &Path) -> interprocess::local_socket::Name;
   ```

   Bind sequence (unix): create listener → on address-in-use: attempt one
   client connect; refused/absent ⇒ stale artifact ⇒ remove file, rebind
   once; connect succeeds ⇒ impossible while the lock discipline holds
   (a live server implies a live lock holder, and we hold the lock) —
   surface as typed `IpcBind` error, never loop. After bind: chmod 0600
   (`std::os::unix::fs::PermissionsExt`; brief pre-chmod window
   documented). Graceful shutdown: stop accepting → drain → remove the
   file. Windows: namespaced names bind-or-fail, no artifacts, nothing
   to clean. FNV-1a 64 is implemented in-module (~4 lines, unit-pinned
   against fixed vectors) because `std::hash::DefaultHasher` makes no
   stability guarantee.
5. **`CoreSession` extraction** (the one refactor to existing code):
   `dispatch_line` takes `&ReplSession` today; the IPC dispatcher needs
   the same two fields. Move `{ config, store }` to
   `commands::CoreSession`; `ReplSession { core: CoreSession,
   json_default: bool }` (repl.rs keeps its public shape by re-exporting
   accessors — `session.config` call sites in dispatch_line move to
   `session.core.config`; ~10 mechanical edits, all inside the crate).
   Gate: zero changes to existing test files.
6. **Argv-to-action parsing shared with the REPL**: extract
   `resolve_words(words: &[String], json_default: bool) -> LineAction`
   (everything in `resolve_line` after the `shell_words::split` step);
   `resolve_line` becomes split + `resolve_words`. The IPC server calls
   `resolve_words(argv, false)` and maps: `Execute(cmd, json)` → capture
   writers + `dispatch_line`; `Refuse(kind)` → code 2 + `kind` text on
   `err`; `PrintClap(err)` → clap's routing (help/version → `out`, code
   0; usage → `err`, code 2); `Exit`/`Reprompt` are REPL-only and
   unreachable from argv (`exit` is not a subcommand — `Cli::parse` of
   `["exit"]` is a `PrintClap` usage error; unit-pinned).
7. **Server topology — accept concurrently, execute serially**:

   ```rust
   pub struct IpcServer {
       app: Arc<App>,                 // daemon-lifecycle Arc (§1)
       session: CoreSession,
       // internal: listener task; mpsc<InboundRequest> dispatcher task;
       // shutdown trigger; stop_requested signal
   }
   pub struct IpcServerHandle {
       pub fn request_shutdown(&self),            // graceful drain
       pub async fn stopped(&self),               // resolves on Shutdown
                                             // request OR external stop
       pub async fn join(self),                   // bounded by caller
   }
   struct InboundRequest { req: Request, reply: oneshot::Sender<Response> }
   ```

   The listener task accepts connections, reads one frame, and forwards
   `(req, reply-oneshot)` to the dispatcher channel; it writes the reply
   frame and closes. The dispatcher task is the *only* `&App` holder: it
   dequeues, executes (`dispatch_line` with `Vec<u8>` sinks; `Status`
   reads `app.engine.shared_state()`; `Shutdown` acks then fires
   `stopped`), and answers — engine-fairness by construction (§2.8).
   Buffer→`String` is lossy-free (all writers are UTF-8 by construction).
   `Arc<App>` exists only in `daemon_lifecycle`: server tasks hold
   clones; at shutdown the lifecycle joins the server first, then
   `Arc::try_unwrap(app)` (refcount back to 1 — the server is joined and
   dropped) → `app.shutdown()` — the consume-once discipline preserved.
8. **Client budget policy**: connect budget 1 s (local sockets answer or
   refuse instantly; the budget only guards pathological mounts — on
   expiry: fall through to in-process, which will surface
   `instance-locked` if a daemon really is wedged — an honest signal,
   not a hang). No client read deadline in v1: every verb self-bounds
   (`sync --timeout-secs`, `boards select --timeout-secs`, engine
   receipts); a daemon-side hang would surface as a wedged *daemon*, which
   `status`/`daemon stop` + supervision exist to handle (backlog: §7).
9. **CLI surface deltas** (`cli.rs`): new `Command::Status`; `Daemon`
   becomes `Daemon { #[command(subcommand)] cmd: Option<DaemonCmd> }`
   with `DaemonCmd::Stop` — bare `taskboard daemon` (and the mode-
   resolution path) still means *run*; `main.rs`'s `explicit_daemon`
   check becomes "run arm = `Daemon { cmd: None }`". `daemon stop` never
   reaches `dispatch` in-process except as the typed `no-daemon` report
   (main only falls back there when the IPC probe found no daemon).
   Global `--json`/`--config` behavior is untouched by transport.
10. **New `CommandError` arms** (kinds are the stable contract):
    `NoDaemon` → `"no-daemon"` (stop/status-with-exppectation paths),
    `IpcTransport` → `"ipc-transport"` (mid-request connection death,
    frame garbage), `IpcVersion { server_v }` → `"ipc-version"`.
    Existing kinds untouched. Text is UX; tests assert kinds.
11. **Raw argv capture**: `main` records `std::env::args().skip(1)` before
    clap parses (clap's `Command` cannot faithfully reconstruct quoting),
    and hands the slice to the IPC path. The server re-parses argv with
    the *same binary version's* clap — argv is the stable wire (§2.4).

## 5. Normative Shapes (signatures binding; bodies indicative)

```rust
// src/ipc/mod.rs
pub mod client; pub mod codec; pub mod protocol; pub mod server;

pub fn socket_name(db_path: &Path) -> interprocess::local_socket::Name;
// unix: fs name  <db-dir>/<stem>.sock
// windows: ns name "taskboard-{:016x}.sock" of fnv1a64(abs db path)
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64;

// src/ipc/protocol.rs  (§4.2 verbatim)
pub const PROTOCOL_VERSION: u32 = 1;
pub enum Request { Exec { v, argv }, Status { v }, Shutdown { v } }
pub enum Response { Exec { v, code, out, err }, Status { v, status },
                    Shutdown { v }, VersionMismatch { server_v } }

// src/ipc/codec.rs
pub const MAX_REQUEST_BYTES: u32  = 64 * 1024;
pub const MAX_RESPONSE_BYTES: u32 = 16 * 1024 * 1024;
pub async fn write_frame<W: AsyncWrite + Unpin>(
    w: &mut W, bytes: &[u8], cap: u32) -> Result<(), codec::Error>;
pub async fn read_frame<R: AsyncRead + Unpin>(
    r: &mut R, cap: u32) -> Result<Vec<u8>, codec::Error>;
// Error: { TooLarge, Eof, Io, Utf8 } — typed, kind-mapped client-side.

// src/ipc/server.rs
pub struct ServerConfig { pub socket_perms_readonly_hint: () } // v1: none
pub async fn spawn_server(
    app: Arc<crate::App>,
    session: CoreSession,
    db_path: &Path,
) -> Result<IpcServerHandle, IpcBindError>;   // { Create, AddrInUse, Io }
impl IpcServerHandle {
    pub fn request_shutdown(&self);           // idempotent
    pub async fn stopped(&self);              // fires on Shutdown request
    pub async fn join(self);                  // drains, removes socket file
}

// src/ipc/client.rs
pub async fn try_connect(db_path: &Path) -> Option<client::Connection>;
impl Connection {
    pub async fn exec(self, argv: &[String]) -> Result<ExecOutcome, CommandError>;
    pub async fn status(self) -> Result<StatusView, CommandError>;
    pub async fn shutdown(self) -> Result<(), CommandError>;
}
pub struct ExecOutcome { pub code: u8, pub out: String, pub err: String }

// commands/mod.rs
pub struct CoreSession { pub config: AppConfig,
                         pub store: Arc<dyn CredentialStore> }
pub enum IpcRoute { Local, IpcOrLocal, IpcOnly }   // per-Command table
pub fn route_for_ipc(command: &Command) -> IpcRoute;
// status payload shared by both answers (commands/status.rs):
pub struct StatusView { /* §4.2 fields, all serde-Option'd */ }
pub fn render_text(view: &StatusView) -> String;
```

```rust
// main.rs — the dispatch arm becomes:
let raw_argv: Vec<String> = std::env::args().skip(1).collect();  // pre-clap
if let Some(command) = cli.command.take() && !run_arm(&command) {
    // 1. resolve config (ok) ⇒ derive db ⇒ route_for_ipc(&command):
    //    IpcOrLocal: try_connect → exec(raw_argv) → relay(out, err, code)
    //                no daemon ⇒ dispatch(command, ctx)            (today)
    //    IpcOnly   : try_connect → shutdown()/…  else report(NoDaemon)
    //    Local     : dispatch(command, ctx)                          (today)
    // config error ⇒ dispatch (typed report, unchanged)
}
// daemon_lifecycle: Arc::new(app) → spawn_server(Arc clone, CoreSession)
//   → select { wait_for_signal(), server.stopped() }
//   → server.request_shutdown(); server.join()
//   → Arc::try_unwrap(app).expect("server joined").shutdown()
```

## 6. Worktree, Branch & TDD Implementation Order

Branch `feat/app-daemon-ipc`, worktree
`../taskboard-WT-feat-app-daemon-ipc` (already created from `main` @
`8e1b5b1`). One PR. TDD order — commit per step, every commit passing the
full gate (§9). Step 2 is behavior-preserving with a
**zero-existing-test-changes** gate; the rest are additive.

1. **Protocol + codec + naming (pure unit layer)**: `ipc/protocol.rs`,
   `ipc/codec.rs`, `fnv1a64` + `socket_name`; deps wired (`interprocess`
   in root `[workspace.dependencies]` + app manifest). Unit tests:
   frame round-trip; cap enforcement both directions; truncated frame →
   typed error; garbage bytes → typed error; request/response serde
   round-trips incl. the `"type"` tags and kebab-case; version field on
   every variant (serde assert, not text); fnv1a64 fixed vectors; unix
   name `<stem>.sock` sibling table; windows name shape (pure fn over
   injected path — no cfg needed for the hash itself).
2. **`CoreSession` extraction** (behavior-preserving): move
   `{config, store}` out of `ReplSession`, update `dispatch_line`
   signature and ~10 in-crate call sites; REPL tests stay green
   byte-for-byte. (This is the commit the Phase 7 UI will also thank.)
3. **Server + IP-series** (`src/ipc/server.rs`, `tests/ipc.rs`):
   standalone core via `bootstrap` (tempdir db, `env_lock` where env is
   touched), `spawn_server(Arc::new(app), …)`; client = in-process
   `client::Connection`. Hermetic journeys (structural asserts only —
   kinds, codes, parsed `out` JSON, engine state via follow-up reads):
   - IP1 happy path: `boards create` → `stacks add` → `tasks add` →
     `tasks list --json` over one server; responses carry code 0 and the
     receipt lines; durability re-read from a second client.
   - IP2 typed failure: unknown-task selector → code 1; parse `err` as
     the error envelope, assert `kind == "unknown-task"`.
   - IP3 clap surface: garbage verb → code 2, `err` non-empty;
     `--help` → code 0, `out` non-empty.
   - IP4 refusals: `daemon`, `login`, `logout`, `config print`, and a
     `--config x` argv → code 2 (usage class), session healthy after.
   - IP5 version mismatch: hand-built `v: 0` frame → `VersionMismatch`
     response (assert `server_v == PROTOCOL_VERSION`).
   - IP6 `Status` request: standalone server → `daemon: true`, backend
     `"standalone"`, `sync.phase == "idle"` (no cycle: phase stays idle
     the whole test — the backlog entry's core assertion).
   - IP7 `Shutdown` request: ack response; `stopped()` resolves; `join`
     completes; socket file gone (unix).
   - IP8 serialization: two concurrent `Connection`s (`tokio::join!`);
     both answered; state consistent (both receipts present in a
     follow-up `tasks list`).
   - IP9 stale socket: write a junk file at the socket path, then
     `spawn_server` → binds successfully (recovery), server serves.
4. **Client routing + CLI verbs + main wiring** (`ipc/client.rs`,
   `route_for_ipc`, `cli.rs` `Status`/`Daemon{cmd}`, `commands/status.rs`
   client-side answer, `main.rs` auto-detect + raw-argv capture; D-series
   rows for the new dispatch arms; `LineAction`-from-argv extraction
   `resolve_words` + its unit table incl. `["exit"]` → PrintClap).
   Unit: routing table per `Command` variant (exhaustive match — the
   compiler forces every future verb to pick a lane).
5. **Standalone-gate relax + binary E2E** (`tests/cli_ipc.rs`, real
   binary both sides, standalone config in tempdir, socket-readiness =
   poll-for-file+connect with deadline): daemon up → `status --json`
   (`daemon: true`); `tasks add` **while the daemon holds the lock**
   exits 0 (the headline: today it is `instance-locked`); `daemon stop`
   → daemon child exits 0, socket gone; `status` → `daemon: false`;
   `tasks add` after stop → in-process fallback exits 0; a
   no-daemon-from-the-start row proves fallback again; lock-sidecar PID
   reported in `status` while a daemon runs matches the child's pid
   (structural, not text).
6. **Wiremock connector-daemon row** (`tests/ipc_wiremock.rs` or fold
   into IP file): config → wiremock answering 401 on everything; daemon
   boots (credential store = env password), sync cycle fails auth in the
   background — `status` over IPC shows `backend: "nextcloud"` with a
   terminal sync phase (`failed`/`offline`), and `tasks add` still
   serves over IPC. Proves the daemon-under-connector shape and that IPC
   does not depend on sync health.
7. **E2E coverage audit** (user-mandated pass, review 2026-10-11): now
   that a full binary-level surface exists (one-shots, REPL pty, daemon
   + IPC + fallback + `daemon stop`), walk the E2E tier against what
   the product actually promises and close the gaps.
   - Build the matrix: rows = user-facing journeys (every §3.1 entry
     arm and every §9 runbook step); columns = the layer covering each
     (unit / L-V-K-O-ST-D-R handler suites / IP-series / binary E2E /
     pty / docker tier); empty cells = gaps.
   - Fill each gap at the **lowest layer that can honestly test it**
     (AGENTS §5): E2E rows assert process-level facts only — exit
     codes across process boundaries, stdout/stderr routing, real
     locks and sockets, signals, the IPC-vs-in-process byte-identity
     silence contract; verb semantics stay in the handler suites.
   - Seed candidates to adjudicate (verify, don't assume they're
     missing): byte-identity for `--help` and exit-2 usage lines (IPC
     vs in-process, beyond the receipt row of step 5); `status` while
     a REPL session holds the lock (`holder_pid` at binary level);
     stale-socket recovery after `kill -9` (automatable: kill the
     daemon child, restart, serve again — runbook step 11's
     counterpart); `daemon stop` with a request in flight (drain
     visibility — binary row or documented in-process-only, decide and
     record); concurrent CLI clients at binary level vs IP8's
     in-process row; REPL error-line and `--json`-session pty
     journeys beyond the one happy-path script; docker-tier `sync`
     over IPC (runbook step 12's automated counterpart, `#[ignore]`-
     gated like the J-series).
   - Output: an audit report in `.artifacts/reports/` (named
     `<audit-date>-wi5-e2e-coverage-audit-report.md`) — the matrix,
     gaps filled (test ids), gaps deliberately left uncovered with
     reasons, and any backlog appends the audit spawns. The report and
     any new rows land in this PR; §9 lists the audit among the
     acceptance criteria.
8. **Packaging + docs + bookkeeping**: `contrib/taskboard.service`
   (user service, `Type=simple`, `Restart=on-failure`, `ExecStart=
   %h/.local/bin/taskboard daemon`, comment block pointing at
   `taskboard.toml`; sd_notify stays out per the backlog entry); ADR
   0012; architecture.org (crate-roles paragraph gains `ipc`, tech-stack
   table gains `interprocess`, frontend list gains the transport); README
   (daemon section rewrite: auto-routing, status, stop, systemd install);
   CHANGELOG; backlog notes on the four entries; roadmap amendment;
   §9 runbook handed to the user.

## 7. Out of Scope → Backlog (entries appended with this plan)

- **Keep-alive / pipelining / event streams over IPC**: one-request-per-
  connection is v1; a subscribe-style push channel (UI repaints over the
  socket) is a Phase-7-adjacent design of its own.
- **IPC disable knob** (`daemon --no-ipc` / `[ipc] enabled = false`):
  always-on v1; minimal-image hardening can add the flag when asked for.
- **Multi-user / socket-path overrides**: OS-perms auth (§2.7) assumes
  single-user-per-db; a knob arrives with a real second-user story.
- **Client read deadlines / request cancellation**: verbs self-bound
  (§4.8); a hard cap interacts with user-set `--timeout-secs` — design
  with the signal-escalation entry, not ad hoc.
- **`logout` while a daemon runs**: permitted and documented today; a
  "daemon holds booted credentials" warning/error is a UX decision for
  the settings-work round.
- **TCP/ssh remote management**: explicitly not v1 (local socket = the
  security boundary); would need real auth design.

## 8. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0012 — "Daemon IPC: the local-socket command transport"**:
  third frontend over the shared seam (CLI one-shot, REPL, IPC client);
  auto-routing table and its silence contract; argv-on-the-wire +
  rendered-text relay rationale (contract preservation over typed
  payloads); one-request-per-connection v1; serialization discipline
  (engine fairness); naming/lifecycle (sidecar derivation, stale
  recovery, perms); versioning policy (envelope now, payloads deferred —
  cross-version behavior: old client → `ipc-version`); the standalone-
  gate relaxation amending ADR 0010 decision 9; security scope (OS perms,
  local-only). Status: Accepted (post-review).
- **`docs/architecture.org`**: taskboard-app crate paragraph gains the
  `ipc` module (server + client + codec + naming); the inter-actor
  protocol table gains "CLI ⇄ daemon (local socket)" noting it as
  transport-only reuse of `dispatch_line`; tech-stack table gains
  `interprocess` (MIT OR Apache-2.0, tokio feature).
- **README.org**: "The daemon" section — start/stop (`daemon`, `daemon
  stop`), `status`, transparent routing (when commands are served by the
  daemon, incl. the no-boot latency note), REPL/daemon mutex note update
  (verbs now route instead of failing), systemd template install snippet.
- **CHANGELOG** `[Unreleased]` → Added: daemon IPC (auto-routing,
  `status`, `daemon stop`, systemd template); Changed: the daemon no
  longer refuses `standalone` (serves IPC instead); `taskboard <verb>`
  now prefers a running daemon.
- **Backlog**: annotate the four source entries (daemon-IPC →
  implemented by this plan; `status`-without-cycle → DONE; systemd
  packaging → template DONE, sd_notify still conditional; versioned-JSON
  → envelope DONE, payloads deferred) and append §7.

## 9. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo nextest run --workspace`, `cargo test --doc --workspace`,
  `cargo deny check` (`interprocess` must land green — MIT/Apache both
  allow-listed; any transitive surprise stops the step, rustyline-BSL
  precedent), `SQLX_OFFLINE=true cargo llvm-cov --workspace` with
  app-crate line coverage ≥ the branch-point measurement.
- Tier 2 (docker J-series): existing journeys stay green untouched.
  Optional manual row: daemon + dockerized Nextcloud, `sync` over IPC
  (runbook step 12).
- **E2E coverage audit delivered** (§6 step 7): the matrix report
  exists in `.artifacts/reports/`; every binary-level gap it identifies
  is either filled by a passing test or recorded with a reason; any new
  E2E rows pass the same full gate as every other commit.
- **Runbook (phase exit, user-run)**, clean machine/profile, no keyring,
  standalone first: (1) `taskboard daemon` in one terminal → logs show
  the socket path; (2) `taskboard status --json` → `daemon: true`,
  backend `standalone`, phase `idle`; (3) `taskboard tasks add "first"`
  while the daemon runs → receipt, **no `instance-locked`**, daemon log
  shows the served request; (4) `taskboard tasks list` → the task;
  (5) `taskboard sync` → typed `sync-disabled` over IPC (standalone), as
  a one-shot would; (6) second terminal `taskboard status` → same
  daemon; (7) `taskboard daemon stop` → exit 0, daemon exits 0, socket
  file gone; (8) `taskboard status --json` → `daemon: false`;
  (9) `taskboard tasks list` (in-process fallback) still shows the data;
  (10) `taskboard` on a TTY with the daemon running → REPL boot fails
  typed `instance-locked` (mutex unchanged, documented); (11) kill -9 a
  daemon, start a new one → stale socket recovered, serving again.
  With network + keyring (nextcloud profile): (12) `taskboard daemon`,
  web-UI-created card appears via `taskboard tasks list` after a cycle
  (served, no boot); `taskboard sync` over IPC → fresh cycle outcome;
  (13) systemd: `systemctl --user import` the template → daemon under
  supervision, `daemon stop` vs `systemctl stop` both graceful.
  Acceptance: user confirms.

## 10. Consumer Handoff Map

| Consumer | Takes from WI-5 |
|---|---|
| Phase 7 UI | The in-process core story is unchanged (UI boots its own `App`), but `status`/`daemon stop` become the supervision surface; a remote-UI event stream has a wire protocol + server topology to grow from |
| Reminders subsystem | A long-lived daemon that can be queried without booting a core is where due-date watchers will live; `StatusView` grows reminder fields |
| Kiosk deployments | `contrib/taskboard.service` + `daemon stop` + `status` = the remote-management minimum; future TCP/ssh rides after auth design |
| External scripters | Envelope versioning precedent; `--json` payloads still unversioned — their versioning (when demanded) copies the envelope pattern |
| Generic file-sync connector work | The daemon is now backend-agnostic; a second connector changes nothing in IPC |

## 11. Risks & Mitigations

- **`interprocess` API/version drift** (2.x reworked names once): pin
  `interprocess = "2.2"`; confine every type import to `ipc/` modules
  (server/client/naming); framing is hand-rolled so the crate surface we
  touch is create/connect/read/write only. Windows named-pipe behavior
  is validated by the release pipeline (CI is Linux); the namespaced-
  name derivation is unit-pinned pure code.
- **Wire contract accidental mutation** (payload drift between transport
  and in-process paths): the binary E2E fallback row (step 5) runs the
  *same command* with and without the daemon and asserts identical
  stdout bytes — the silence contract is tested, not hoped for.
- **Stale-socket races** (two daemons racing recovery on one db):
  impossible while the lock discipline holds (the second boot fails
  `instance-locked` before it ever binds); the recovery path's
  connect-probe is belt-and-braces, unit-tested via IP9.
- **Serialization head-of-line blocking** (one slow `sync` stalls
  `status`): accepted v1 semantics (§2.8), same fairness as the engine
  itself; `status` after a hung request remains available via a fresh
  connection once the queue drains — if operational pain appears, the
  dispatcher gains a read-only fast lane (documented alternative, not
  built).
- **Argv quoting fidelity** (client platform shell already split args;
  we relay `env::args` verbatim): no re-splitting ever happens
  server-side (§4.6 consumes `Vec<String>` directly) — the REPL's
  `shell-words` path is the only splitter in the codebase and stays
  REPL-only.
- **`Arc::try_unwrap` failure at shutdown** (a leaked server task still
  holding a clone): the server exposes `join` and the lifecycle joins it
  before unwrapping; a residual holder is a bug the `expect` surfaces
  loudly in dev runs (bounded by `shutdown_timeout_secs` overall).
- **Scope creep** (keep-alive, TCP, auth, event streams): triaged to §7;
  the deliverable is transport + `status` + `daemon stop` + packaging.
- **Rebase exposure**: none — WI-5 is the last of the four M1-exit work
  items; nothing is stacked behind this branch (per AGENTS §8 the PR is
  standalone against `main`).
