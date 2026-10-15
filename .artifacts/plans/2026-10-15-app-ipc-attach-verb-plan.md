# [Plan] Daemon-Side Attach IPC Verb (P5)

- **Date**: 2026-10-15
- **Kind**: stage-1 implementation plan (chunk P5 of the
  parallel-during-GUI dispatch), TDD-first. No production code in this
  commit.
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-14-parallel-during-gui-dispatch-plan.md`
  §1 (execution model, §1.3 code-quality mandate, §1.4 GUI no-go zones)
  and §6 P5; backlog entry "[2026-10-14] Daemon-side attach verb:
  connect a running daemon without restarting it"
  (`.artifacts/taskboard-potential-improvements-backlog.md`); ADR 0014
  decision 7 (the future-work note this chunk closes) and its
  DetachConnector pattern; ADR 0012 (IPC envelope conventions, the
  additive-variant wire policy, WI-H's detach nudge).
- **Target**: `taskboard-app` only — `src/ipc/protocol.rs`,
  `src/ipc/client.rs`, `src/ipc/server.rs` (one relay arm),
  `src/session_ceremonies.rs` (the frontend-agnostic daemon-side
  construction), `src/commands/connect.rs`, `src/commands/mod.rs` (the
  one-shot `connect_command` wiring only), `src/commands/output.rs`,
  `src/commands/status.rs` (the §2.6 companion slice), plus the test
  tiers (`src/ipc/protocol.rs` units, `tests/ipc.rs` IP-series,
  `tests/cli_ipc.rs` E-series). **No `bootstrap.rs`, `main.rs`,
  `config.rs`, `ui-slint`, CI, or version diff** (dispatch §1.4; all
  required seams — `App::attach_connector`, `wait_for_boot_pull` (both
  `pub(crate)`/`pub`), `BasicSession { store, config_path }`,
  `AppConfig::load` — already exist read-only).
- **Branch / worktree**: `feat/app-ipc-attach-verb`,
  `../taskboard-WT-feat-app-ipc-attach-verb`, off `main` @ `fda8009`.
- **Goal**: a running daemon attaches a connector from persisted state
  on request — the client sends identity only, the daemon re-reads its
  own config, loads its own credential store, and calls the merged
  `App::attach_connector` with the REPL's settle — and the one-shot
  `connect` triggers it best-effort, replacing today's passive
  "restart the daemon" warning. Closes ADR 0014 decision 7's
  daemon-side follow-up.
- **Status**: READY for stage 2 (every open question resolved in §3
  with a recorded decision).

---

## 1. Context & verified facts (anchored on `main` @ `fda8009`; do not re-derive)

1. **The additive-verb pattern is fully established by
   `DetachConnector`** (`ipc/protocol.rs:46-53`, `:84-90`):
   `Request::DetachConnector { v, server_url, username }` /
   `Response::DetachConnector { v, outcome: DetachWireOutcome }`;
   `DetachWireOutcome` is a `Copy` kebab-case enum (`detached` /
   `no-connector` / `identity-mismatch` / `timed-out`,
   protocol.rs:118-130). `PROTOCOL_VERSION = 1` stays 1 for additive
   variants (ADR 0012 amendment: additive variants never bump;
   mixed-version window degrades exactly one verb — an old daemon
   cannot decode the variant and drops the connection → the client
   sees typed `ipc-transport`). Wire pins live in
   `wire_shapes_are_pinned`, `envelopes_round_trip`,
   `version_accessor_skips_only_version_mismatch`
   (protocol.rs:141-302).
2. **The server dispatch seam** (`ipc/server.rs`): `handle(app:
   &App, session: &BasicSession, req)` executes serially on the
   dispatcher task (the only `&App` holder, ADR 0012 §2.8). The
   detach arm (:384-416) is the template: standalone-config short
   circuit → identity match against the daemon's config →
   `app.detach_connector()` → outcome mapping. `request_version`
   (:352-359) is an exhaustive match over `Request` — a new variant is
   a compile error until wired. The Exec-lane ceremony refusal
   (:484-497) refuses argv `connect`/`login`/`logout` with code 2
   **before** dispatch — untouched by this chunk.
3. **The client relay seam** (`ipc/client.rs`):
   `Connection::detach(self, server_url, username)` (:209-230) wraps
   the request in `tokio::time::timeout(DETACH_BUDGET = 30 s, …)` and
   maps transport/version failures to `CommandError::IpcTransport` /
   `IpcVersion`. `try_connect(db_path)` answers `None` on concrete
   refusals (no daemon) with no latency.
4. **`App::attach_connector(&self, client: DeckClient)`**
   (`bootstrap.rs:263-314`) is merged and untouched by this chunk:
   `AlreadyAttached` guard → fresh command pair → engine push + FIFO
   ack (`AttachError::Engine` / `AttachError::Draining` on refusal) →
   actor spawn over the parked reports sender's clone → register (a
   lost vacancy unwinds and answers `AlreadyAttached`).
5. **The settle is the `pub(crate)` boot-pull helper
   `wait_for_boot_pull(app, timeout)`** (`bootstrap.rs:682-710`):
   bounded, offline-first, degrade-to-local. The session ceremony
   calls it with `Duration::from_secs(app.config.app.
   shutdown_timeout_secs.max(1))` (`session_ceremonies.rs:121`);
   `shutdown_timeout_secs` defaults to 10 (`config.rs:42`).
6. **The daemon holds everything the handler needs in
   `BasicSession`** (`commands/mod.rs:883-900`): `store: Arc<dyn
   CredentialStore>` (the boot-picked production store — `main.rs`
   constructs it and passes the same `Arc` into bootstrap and the
   session, `main.rs:285-291`) and `config_path: Option<PathBuf>` (the
   daemon's `--config` answer). `AppConfig::load(Option<PathBuf>)`
   (`config.rs:408`) is the full validated figment resolution
   (defaults → file → `TASKBOARD_*` env) the session refresh uses
   (`session_ceremonies.rs:169`); env overlays resolve against *the
   daemon's own process environment*. **No `App` field, `bootstrap`,
   or `main` change is needed** — the store handle rides the session.
7. **The REPL's client construction is `build_client(&ReplSession)`**
   (`session_ceremonies.rs:187-205`): identity off the refreshed
   config (`CommandError::NotConfigured` when absent), secret off
   `session.store` (`CommandError::Credential`), `DeckClient::new`
   (`map_deck_error`). ReplSession-typed and private — the
   frontend-agnostic core is steps (identity → store load → client),
   which this chunk factors out (§2.3).
8. **The one-shot `connect` already probes for a daemon and warns**:
   `connect_command` (`commands/mod.rs:743-775`) captures
   `db_path = raw.db_path()` from the pre-ceremony raw config (:751),
   and on ceremony success runs the D11 heads-up
   `connect::daemon_warning(connect::daemon_serves(&db_path).await)`
   (:760-764) — "keeps running without the new connection until
   restarted". `login_command` delegates with a narrower flag bundle
   (:723-734), inheriting the heads-up. The ceremony's config overlay
   (`write_config_overlay`, connect.rs:469-501) writes only
   `nextcloud.server_url`/`username` (+ the backend word iff
   switched) — **`storage.db_path` is never touched, so the
   pre-ceremony `db_path` remains the correct socket derivation**.
9. **The WI-H logout nudge is the client-side template**
   (`commands/logout.rs:98-172`): `NudgeOutcome { NoDaemon,
   Answered(DetachWireOutcome), Transport, Version }` → pure
   `map_nudge` → `DaemonConnectorState { Dropped, None, Mismatch,
   Unreachable, Unknown }` → `nudge_warning(state) ->
   Option<&'static str>` (only degraded arms warn; `NoDaemon` is
   silent, D5) → receipt field `daemon_connector:
   Option<DaemonConnectorState>` (`skip_serializing_if`), exit code
   unchanged. The nudge is bounded client-side by the 30 s read
   budget; the daemon completes regardless.
10. **`ConnectReceipt`** (`commands/output.rs:405-428`) already carries
    `attached: Option<bool>` (session ceremony only; absent for the
    one-shot — byte-shape pinned, connect.rs:589-604). Pre-v0.1.0
    additive JSON fields are sanctioned (P4 precedent).
11. **Status gating** (`commands/status.rs:64-82`):
    `StatusView::server(app)` computes `connector_attached` through
    `connector_flag(app.config.sync_backend(), app.connector_attached())`
    — `Standalone => None` (:21-26) — against the **boot-frozen**
    `app.config` (ADR 0012: the daemon never rewrites its in-memory
    config). The presence contract is pinned by
    `connector_attached_presence_is_pinned` (protocol.rs:308-334):
    daemon+nextcloud carries the flag, standalone omits it.
12. **Test harness facts**: `tests/ipc.rs` — `served` /
    `served_nextcloud(dir, db, server_url)` boot a real core + server
    over a tempdir db (`FakeCredentialStore`, `config_path: None`,
    DRAIN_BUDGET, `bounded()` wrapper, `exec_once`, `json_lines`,
    `detach_once`); IP13-15 pin the detach outcomes. `tests/cli_ipc.rs`
    — the `Install` harness (standalone `taskboard.toml` with
    `credential_store = 'env'`, `shutdown_timeout_secs = 5`,
    `poll_interval_secs = 1`, `write_test_config`,
    tests/common/mod.rs:445-468), `spawn_daemon` (children inherit the
    test process env), `run` one-shots, `wait_ready`; **e8**
    (:503-538) pins today's D11 contract: connect-while-daemon-serves
    succeeds, receipt unchanged, stderr non-empty (wiremock boards,
    piped empty password). `tests/common/mod.rs` — env mutex +
    `set_env`/`EnvOverride` (edition-2024 unsafe under lock),
    `FakeCredentialStore::with(server, user, pw)`. The J-series
    (`tests/repl.rs` j1/j6) pins the session ceremonies incl. the
    attach settle. `EnvCredentialStore.load` reads
    `TASKBOARD_APP_PASSWORD` live (`secrets.rs:162-170`); the env
    store is read-only (`store` → `Unsupported` → receipt
    `stored: "env"`).
13. **Completion's `REFUSED_VERBS`** is `["daemon", "config"]`
    (`completion.rs:234`); `connect`/`login`/`logout` already complete.
    `dispatch_line`'s `Command::Connect` arm is an `unreachable!`
    routing invariant (mod.rs:920-933) — untouched.

## 2. Design

### 2.1 Wire shapes (decision D1, D2)

Additive envelope variants, `PROTOCOL_VERSION` stays 1, kebab-case
tags — `DetachConnector` verbatim:

```rust
// Request — identity only; no field can carry a secret.
AttachConnector {
    v: u32,
    server_url: String,
    username: String,
}

// Response
AttachConnector { v: u32, outcome: AttachWireOutcome }
```

```rust
/// The attach-connector answer classes, kebab-case on the wire.
enum AttachWireOutcome {
    Attached,            // "attached"
    AlreadyAttached,     // "already-attached"
    IdentityMismatch,    // "identity-mismatch"
    NotConfigured,       // "not-configured"
    CredentialUnavailable, // "credential-unavailable"
    Engine,              // "engine"
    Draining,            // "draining"
}
```

Arm semantics (each is reachable and distinct-in-remedy except the
noted guard):

- `attached` — the daemon built the client from its own persisted
  state and attached; answered after the bounded settle (§2.4).
- `already-attached` — the primitive's guard. **Unreachable from this
  handler by construction** (the dispatcher is serial and the handler
  swaps, §2.3; a second queued request re-swaps); carried so the wire
  can report every primitive refusal honestly if dispatch ever
  widens. The client folds it into the generic warning arm.
- `identity-mismatch` — the re-read config names a *different*
  identity than the request's; nothing changed. Rationale mirrors WI-H
  D3: the client is about to tell the operator "connected + daemon
  attached" *for identity X*; attaching the persisted Y under that
  claim would be dishonest.
- `not-configured` — the daemon's re-read config did not yield a
  usable identity+client: standalone install, missing identity keys,
  an unreadable/invalid file, or a URL `DeckClient` rejects. Merged
  bucket (recorded): every arm has the same operator remedy (the
  daemon's own config is the problem — fix it and restart); the daemon
  logs the precise cause for diagnosis.
- `credential-unavailable` — the daemon's store could not load the
  secret (the env store without `TASKBOARD_APP_PASSWORD` in the
  *daemon's* environment; a keyring miss). This is the documented
  env-store caveat's typed surface: attach works for the keyring
  store; for the env store the daemon's own environment governs
  (restart-with-env stays that store's operating model).
- `engine` / `draining` — `AttachError::Engine` / `AttachError::
  Draining` mapped (a raced shutdown).

**The refused-secret invariant is structural and pinned**: the request
payload type has exactly three fields, serde serializes exactly those,
and a new protocol unit test asserts the serialized JSON object's key
set equals `{"type","v","server_url","username"}` (and the response's
`{"type","v","outcome"}`) — any future field addition fails the pin
and forces a conscious decision. No `String` in the payload is a
secret: `server_url`/`username` are the config-file non-secrets the
ceremony already persists in cleartext (WI-H ships the same pair).

`request_version` (server) and `Response::version` gain the arms;
`wire_shapes_are_pinned`, `envelopes_round_trip`, and
`version_accessor_skips_only_version_mismatch` extend additively.

### 2.2 Client relay (decision D4)

`Connection::attach(self, server_url, username) ->
Result<AttachWireOutcome, CommandError>` — `detach`'s mirror with
`ATTACH_BUDGET: Duration = 30 s` (covers the daemon-side worst path:
serial-queue wait + detach join + settle under the 10 s default
shutdown timeout, and the 5 s E2E installs, with margin; on expiry the
daemon still completes its attach and the client degrades to the
warning — WI-H's reading).

### 2.3 Daemon-side construction & orchestration (decision D3)

A frontend-agnostic ceremony function in `session_ceremonies.rs`
(ADR 0014's "verb bodies stay frontend-agnostic functions"; the
dispatch names this file as where the client construction lives):

```rust
pub(crate) async fn attach_from_persisted(
    app: &App,
    config_path: Option<&Path>,
    store: &Arc<dyn CredentialStore>,
    server_url: &str,
    username: &str,
) -> AttachWireOutcome
```

Step order (named steps, one level of abstraction; each early-return
maps to its wire arm, cause `tracing::warn!`-ed server-side):

1. **Re-read**: `AppConfig::load(config_path)` — the daemon's own boot
   config path, env overlays resolved against the daemon's
   environment. Failure → `not-configured` (§2.1's merged bucket).
2. **Identity**: absent keys → `not-configured`; a pair ≠ the
   request's → `identity-mismatch`.
3. **Build before any detach**: share the construction with the REPL —
   factor the identity→store-load→`DeckClient::new` core out of
   `build_client(&ReplSession)` into
   `deck_client_from(config: &AppConfig, store: &Arc<dyn
   CredentialStore>) -> Result<DeckClient, CommandError>`
   (`session_ceremonies.rs`; the REPL builder delegates, behavior
   unchanged). The daemon maps `Err(CommandError::Credential(_))` →
   `credential-unavailable`, every other error → `not-configured`
   (the identity was already extracted, so the remainder is
   malformed-persisted-state; the cause is logged). Building before
   detaching means the only post-swap failures are `engine`/
   `draining` (a raced shutdown) — the daemon is never left detached
   by a failure its own state could have prevented.
4. **Swap-first**: `if app.connector_attached() {
   app.detach_connector().await; }` — ADR 0014's detach-first session
   rule applied daemon-side, for the same recorded reason: a
   same-identity re-connect stores a *new* secret, and only detach +
   re-attach rebuilds the client. Without it, secret rotation leaves
   the daemon syncing on a dead credential while reporting attached —
   the dishonesty class this project keeps killing. The WI-H detached
   state (connector-less, still serving) is the recoverable midpoint.
5. **Attach**: `app.attach_connector(client)` → `Ok(())` proceeds,
   `AttachError` maps to `already-attached` / `engine` / `draining`.
6. **Settle**: `wait_for_boot_pull(app, Duration::from_secs(
   app.config.app.shutdown_timeout_secs.max(1)))` — the REPL's exact
   mirror (fact 5) — then answer `attached`.

The server arm (`ipc/server.rs::handle`) is a two-line relay — call
`attach_from_persisted(app, session.config_path.as_deref(),
&session.store, …)`, wrap in `Response::AttachConnector` — keeping
IPC transport-only (mandate: no business logic in the relay; the
`detach` arm's inline precedent is *not* extended, it is superseded by
the ceremony-home placement for the new, longer body).

Known/accepted behaviors (recorded, not bugs): the spawned actor's
tuning (`sync_actor_config()` off the boot-frozen `app.config`) rides
the restart — identical to the REPL path; the daemon's in-memory
config is never rewritten (ADR 0012 preserved); the store *instance*
is the boot-picked one (re-picking the kind from the re-read file
would only matter for a mid-session `credential_store` hand-edit — a
restart-class change; both store kinds read their backing live at
`load()`).

### 2.4 Settle semantics (decision D5)

Mirror the REPL exactly: bounded `wait_for_boot_pull`, offline-first,
degrade-to-local; `attached` is answered regardless of settle
degradation (freshness rides later poll cycles / `taskboard sync`).
The attach arm may hold the serial dispatcher up to ≈ 2×
`shutdown_timeout_secs` (swap join + settle) — the same accepted
ADR 0012 consequence class as one long `sync` holding the queue.

### 2.5 Client-side surface: the existing `connect` verb, no flag (decision D6)

**The one-shot `connect` nudges unconditionally after a successful
ceremony** (and `login` inherits it by delegation) — the D11 passive
probe+warning is *replaced* by the active attach nudge:

- `connect::AttachNudgeOutcome { NoDaemon, Answered(
  AttachWireOutcome), Transport, Version }` and
  `connect::nudge_daemon_attach(db_path, server, username)` —
  logout's `NudgeOutcome`/`nudge_daemon` mirrored onto the attach
  verb.
- Pure `map` → new `DaemonAttachState { Attached, None, Mismatch,
  Unreachable, Unknown }` (`commands/output.rs`, kebab, beside
  `SessionConnectorState`): `NoDaemon → None` (silent, D5);
  `Answered(Attached) → Attached` (silent); `Answered(IdentityMismatch)
  → Mismatch`; `Transport → Unreachable` (covers the pre-attach-verb
  daemon, which drops the link); everything else (`NotConfigured`,
  `CredentialUnavailable`, `Engine`, `Draining`, `AlreadyAttached`,
  `Version`) `→ Unknown`. Warnings only on `Mismatch`/`Unreachable`/
  `Unknown` — the connect *did* succeed; the warning tells the
  operator the daemon did not pick it up (restart / `taskboard daemon
  stop` guidance). Exit code stays 0 in every arm.
- `ConnectReceipt` gains `daemon_connector: Option<DaemonAttachState>`
  (`skip_serializing_if` — absent when no daemon was consulted,
  preserving the pure-local shape; pre-v0.1.0 additive). `render_text`
  appends a suffix on `Attached` (UX text, never asserted).
- `connect_command` wiring: the existing success-path block
  (mod.rs:756-765) swaps probe+warning for nudge+map+warning+receipt
  field; `db_path` stays the pre-ceremony capture (fact 8's
  invariant). The REPL's `connect_line` is untouched (the session
  holds the lock — no daemon can serve its database, D10).

Recorded rejection — a flag (`connect --attach-daemon`): leaves the
headline bug (backlog: "the running daemon never attaches … until
restart") alive on the default path; breaks the WI-H symmetry (logout
nudges unconditionally); adds surface for no operator story. Recorded
rejection — a new verb (`taskboard daemon attach` / `attach`):
attach-from-persisted-state only makes sense immediately after a
local connect verified+persisted that state; a standalone verb would
either re-run the ceremony (that *is* `connect`) or blindly trust
stale state with no verification story. The D11 probe already firing
unconditionally at this exact spot is the third argument for
no-flag: P5 upgrades probe → nudge in place, adding no UX surface.

### 2.6 Companion slice: status reports the live flag (decision D7)

P5 creates a state the status projection misreports: a
standalone-*booted* daemon that attached from persisted state answers
`backend: "standalone"` with `connector_attached` **absent** (the
gate at status.rs:69-70 reads the boot-frozen config) while a live
actor syncs — the field's own doc condemns this shape ("structurally
true, operationally misleading"). Fix in the daemon projection only:
`StatusView::server` answers `connector_attached:
Some(app.connector_attached())` unconditionally; `connector_flag`
stays for the in-session caller (the session refreshes its config
view at every ceremony, so its gating tracks reality). The field doc
and the protocol presence pin
(`connector_attached_presence_is_pinned`) update to the new contract:
**daemon-side answers always carry the live flag** (boot-standalone +
no connector → `Some(false)`, post-attach → `Some(true)`;
boot-nextcloud rows unchanged); `backend` remains the boot-shaped
config word — the pair reads "booted standalone, connector live",
which is the true story (the WI-H mirror: `backend: "nextcloud"` +
`Some(false)` after logout's nudge). Revertable as one commit; called
out for the plan review.

### 2.7 Untouched invariants (decision D8)

- The three ceremony verbs stay refused over the Exec lane
  (server.rs:484-497) — the new verb is a dedicated envelope variant,
  never an argv path; `dispatch_line`'s `unreachable!` arm,
  `route_for_ipc` lanes (`connect` stays `Local`), and completion's
  `REFUSED_VERBS` are untouched.
- No envelope version bump; no multi-connector lift (the
  `AlreadyAttached` guard and the v1 ≤ 1 invariant stand); no GUI
  no-go file in the diff.

### 2.8 ADR 0014 additive note (decision D9)

One dated completion paragraph appended under decision 7 (append-only,
no renumbering, no new ADR): the daemon-side attach shipped as
`Request::AttachConnector` on the WI-H additive pattern — identity
only on the wire, the daemon re-reads its config and loads its own
credential store, swap-first per the detach-first session rule, the
one-shot `connect` nudges best-effort exactly as `logout` nudges
detach, and the env-store caveat is the `credential-unavailable`
outcome (the daemon's environment governs).

## 3. Open questions → recorded decisions

| # | Question | Decision |
|---|---|---|
| D1 | Request/response shape | `AttachConnector { v, server_url, username }` / `{ v, outcome }`; `PROTOCOL_VERSION` stays 1 (ADR 0012 additive policy); kebab tags; identity-only proven by the key-set pin test (§2.1) |
| D2 | Outcome vocabulary | 7 arms (§2.1) — the dispatch's four (`attached`/`already-attached`/`engine`/`draining`) plus the precondition arms the daemon-side construction requires (`identity-mismatch`, `not-configured` incl. read-failure, `credential-unavailable`); every arm reachable-in-principle and distinct-in-remedy except the guard, which is carried for wire honesty |
| D3 | Daemon-side client construction | `attach_from_persisted` in `session_ceremonies.rs` over `BasicSession`'s `config_path`+`store`; re-read → identity match → shared `deck_client_from` → swap-first → attach → settle; build-before-swap so pre-swap failures never strand the daemon detached (§2.3) |
| D4 | Client budget | `ATTACH_BUDGET = 30 s` mirroring `DETACH_BUDGET`; expiry degrades to the warning, daemon completes regardless |
| D5 | Settle semantics | `wait_for_boot_pull` with the REPL's exact timeout; `attached` answered regardless of settle degradation; serial-queue hold ≈ 2× shutdown timeout is the accepted ADR 0012 consequence class (§2.4) |
| D6 | Which client surface relays it | The existing one-shot `connect` (and `login` by delegation), unconditionally, replacing the D11 probe+warning in place — no flag, no new verb; rejections recorded in §2.5 |
| D7 | Status after a standalone-boot attach | Daemon projection answers the live flag unconditionally (§2.6); presence pin + docs updated; in-session gating unchanged |
| D8 | Ceremony refusals / lanes / completion | Untouched (§2.7) |
| D9 | ADR 0014 | Additive completion note under decision 7 only (§2.8) |
| D10 | Store instance vs re-picked kind | The boot-picked `BasicSession.store` (both kinds read live at `load()`; a mid-session kind hand-edit is restart-class — §2.3) |

## 4. TDD test plan (write-first order)

### Tier 1 — protocol units (`src/ipc/protocol.rs`, in-module `tests`)

1. Extend `wire_shapes_are_pinned`: the attach request serializes
   `type: "attach-connector"`, `v: 1`, both identity fields; the
   response serializes `type: "attach-connector"` and a kebab outcome
   (e.g. `"credential-unavailable"`).
2. **New** `the_attach_request_carries_identity_only`: serialize the
   request; assert the JSON object's key set is exactly
   `{"type","v","server_url","username"}` (collect + compare as a
   sorted set — structural, not text); same for the response's
   `{"type","v","outcome"}`. This is the refused-secret invariant's
   pin.
3. Extend `envelopes_round_trip` and
   `version_accessor_skips_only_version_mismatch` with the new pair.
4. Update `connector_attached_presence_is_pinned` to D7's contract
   (rides the §2.6 commit): daemon-side carries the flag under both
   backends; standalone-pre-attach pins `Some(false)`.

### Tier 2 — client mapping units (`src/commands/connect.rs`, in-module)

5. `the_attach_nudge_mapping_covers_every_arm` (logout's
   `detach_tests` mirror, socket-free): every `AttachNudgeOutcome` arm
   maps to its `DaemonAttachState`; warnings exist exactly on
   `Mismatch`/`Unreachable`/`Unknown` (`None`/`Attached` silent — the
   no-daemon row is D5's "not a degradation").
6. `the_daemon_connector_field_serializes_kebab_case_or_is_absent`:
   the receipt field kebab (`"attached"`, `"none"`, `"unknown"`, …)
   and absent when `None`.

### Tier 3 — IPC-level, hermetic (`tests/ipc.rs`, IP-series; real
bootstrap + server over a tempdir db, `bounded()` wrapper)

7. New harness `served_with_config(dir, db_name, config_body) ->
   (handle, db, config_path, store: Arc<FakeCredentialStore>)`: boots
   a standalone daemon whose `BasicSession.config_path` is `Some(`a
   real tempdir file`)` — **`config_path` must never be `None` in
   these tests** (`AppConfig::load(None)` would resolve against the
   CWD/XDG — a hermeticity rule, fact 12). Tests mutate the config
   file and the fake store's entries to simulate what a `connect`
   ceremony just persisted.
8. `ip16` attach-from-persisted-state: standalone boot → write the
   identity into the config file + store entry → `attach_once` →
   `Attached`; `status` answers `connector_attached: Some(true)`
   (D7) with `backend: "standalone"` (boot-shaped); a relayed
   `--json sync` serves (code 0/cycle outcome, not `sync-unavailable`);
   local edits still serve; argv `connect` over the Exec lane still
   refuses code 2 (the untouched guard, one `exec_once` row).
9. `ip17` swap-first (the daemon-side detach-first rule): attach again
   (same identity, rotated store entry) → still `Attached`; `status`
   stays `Some(true)`; a second attach with a *different* store entry
   proves the rebuilt client (the fake store's current entry is what
   the new actor loaded — assert via the store's recorded loads if
   the fake records them, else via the `Attached` + status pair).
10. `ip18` typed refusals: foreign identity → `IdentityMismatch`
    (config untouched, nothing attached); identity removed from the
    file → `NotConfigured`; store entry removed →
    `CredentialUnavailable`; each leaves `connector_attached`
    unchanged from the pre-request state.

### Tier 4 — binary E2E (`tests/cli_ipc.rs`, E-series; real daemon
process + wiremock)

11. **Update e8** (the D11 contract flip — intended behavior change,
    not a weakening): env mutex → set `TASKBOARD_APP_PASSWORD` →
    spawn daemon (inherits) → wiremock connect (piped password) →
    exit 0; receipt `daemon_connector: "attached"`; **stderr empty**
    (success is now silent); `--json status` → `daemon: true`,
    `connector_attached: true`, `backend: "standalone"`; `daemon
    stop` exits 0. The old `stderr`-non-empty assertion is replaced
    by the attach pins; the exit-0 and receipt-shape rows carry over.
12. **New e9** (degradation, env-store caveat live): daemon spawned
    *without* the env var → connect (env store, piped password)
    succeeds → receipt `daemon_connector: "unknown"`, stderr
    non-empty (structural), exit 0; the daemon keeps serving
    (`status` `daemon: true`, `connector_attached: false`).

### Tier 5 — regression neighbors

13. `tests/cli_connect.rs` C-series and `tests/dispatch_routing.rs`
    need no weakening: in-process runs have no daemon → `NoDaemon` →
    silent `None` receipt field (the `try_connect` concrete-refusal
    path costs no latency). Add one structural row if a journey
    already pins the receipt's full JSON shape (extend, don't
    rewrite).
14. `tests/repl.rs` J-series: untouched (the session path is
    unchanged; j1/j6 keep pinning it).

## 5. Implementation slices (stage 2; every commit standalone-green)

1. `fix(app): status reports the live connector flag for daemons` —
   §2.6 + tier-1 items 4 (presence pin) — independently correct.
2. `feat(app-ipc): daemon-side attach verb` — wire pair + outcome
   enum, `Connection::attach` + budget, `attach_from_persisted` +
   `deck_client_from` refactor, server arm; tiers 1 (items 1-3) + 2 +
   3.
3. `feat(app): connect nudges a serving daemon to attach` — nudge
   mapping/wiring, receipt field, `render_text`; tier 4 + 5.
4. Final bookkeeping: `docs(adr): 0014 daemon-side attach completion
   note` + backlog entry ✅ DONE with a one-line note (dispatch §6
   acceptance; ADR append-only per D9).

## 6. Risks & mitigations

- **D7 touches a just-shipped P4 surface** (status presence contract):
  isolated as slice 1, revertable alone; flagged here for the plan
  review. Without it, P5 ships a known misleading status — the worse
  trade.
- **e8's assertion flip** reads like test-weakening out of context:
  §4.11 frames it as the intended contract change with strictly more
  pins (receipt field + live status + silent stderr); stage 3 re-reads
  this section.
- **Serial-queue hold** during a settle-bound attach (≈ 2× shutdown
  timeout): accepted ADR 0012 consequence class, documented in §2.4;
  no fast lane (same "if pain appears" stance as ADR 0012 §2.7).
- **Hermeticity**: `AppConfig::load(None)` resolving against CWD/XDG —
  the tier-3 harness rule (item 7) forbids `config_path: None`.
- **E2E env inheritance**: the daemon must inherit
  `TASKBOARD_APP_PASSWORD` set under the env mutex *before*
  `spawn_daemon`; children snapshot the parent env at spawn (fact 12).
  e9 deliberately withholds it.
- **Cross-version window**: old daemon + new client → undecodable
  variant → dropped link → `Unreachable` warning (WI-H's identical
  matrix; no action).
- **CI/runtime**: no new deps, no workspace pin changes, no config
  surface — `cargo deny` unaffected; sqlx offline unaffected.

## 7. Estimated diff size

≈ 850-950 lines incl. tests (dispatch table's "M"): protocol ~120,
client ~45, server ~15, session_ceremonies ~90, connect ~150, mod
~15, output ~40, status ~15, tests/ipc ~200, tests/cli_ipc ~120,
ADR+backlog ~16.
