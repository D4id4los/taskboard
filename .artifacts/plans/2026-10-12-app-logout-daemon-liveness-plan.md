# [Plan] WI-H — `logout` vs Running Daemon: Credential-Liveness Semantics

- **Date**: 2026-10-12
- **Target**: `taskboard-app` (IPC protocol/server/client, `commands/{logout,status,sync,mod}`,
  `bootstrap.rs`) plus, **only under option (b)**, `taskboard-domain`
  (`messages.rs`) and `taskboard-sync-nextcloud` (`actor.rs`). Docs: ADR 0012
  amendment, backlog bookkeeping, CHANGELOG. No new workspace dependencies
  under either option.
- **Goal**: dispatch plan §4 WI-H — resolve the gated semantics of `logout`
  while a daemon serves the database. Today the daemon keeps its booted
  credentials until restart (ADR 0012 decision 2's documented carve-out):
  honest but surprising. Two candidate semantics are designed below to
  decision-ready depth; **one** is implemented by stage 2 after the user
  picks at plan review.
- **Status**: DRAFT — **GATED**. Recommendation: **option (b)** (§6). The
  user picks at plan review by naming the approved option here:
  **Approved option: ______________** (stage 2 STOPs unless this names (a)
  or (b)).
- **Origin**: backlog "[2026-10-11] `logout` while a daemon runs:
  credential-liveness warning"; dispatched by
  `.artifacts/plans/2026-10-12-backlog-batch-dispatch-plan.md` §2/§4 WI-H.
- **Predecessors**: WI-5 (daemon IPC, ADR 0012), WI-3 (`logout` itself)
  merged; verified on `main` @ `c1db45e`.
- **Branch / worktree**: `feat/app-logout-daemon-liveness`, worktree
  `../taskboard-WT-feat-app-logout-daemon-liveness` (created from `main` @
  `c1db45e`).

---

> ## GATE — one semantics, picked at plan review
>
> - **(a) Observe** — `status` surfaces `credentials_revoked: true` when
>   the daemon's store load now fails. Observation only; the daemon keeps
>   its connector and keeps syncing until restart.
> - **(b) Act** — `logout` nudges the daemon over IPC to drop its
>   connector: the sync actor stops, the in-memory credential
>   (`DeckClient`) dies, the daemon degrades to connector-less serving.
>
> **Recommended: (b)** — logout means logout, especially on a kiosk; see
> §6. Do not implement both: their status-field additions collide and (b)
> makes (a)'s probe redundant (§6.3).

---

## 1. Context & Verified Facts (do not re-derive; `main` @ `c1db45e`)

- **`logout` today** (`commands/logout.rs:25-68`): Local-lane, pre-core,
  no lock (ADR 0012 lane table). Requires a configured identity
  (`server_url` + `username`, typed `not-configured` otherwise) → deletes
  the secret (`NotFound` = idempotent success) → strips
  `nextcloud.server_url`/`username` from the config file **iff** the
  backend auto-resolved; an explicit `backend = "nextcloud"` is never
  rewritten (ADR 0010 loudness). The env store refuses `delete` with typed
  `Unsupported`. The board binding and local db are untouched.
- **The daemon's credential is boot-time and in-memory only**
  (`bootstrap.rs:275-302`): `bootstrap` resolves
  `credentials.load(server, username)` exactly once; the password is
  consumed by `DeckClient::new` (the only `AppPassword::expose` call,
  `secrets.rs:14-15`) and lives in the client for the process lifetime.
  The store is never consulted again. `App` does **not** retain the
  store; the IPC server's `BasicSession` does (`commands/mod.rs:777-794`).
- **A keyring deletion does not revoke anything server-side.** Deleting
  the entry removes the *local* copy; the app password stays valid at
  Nextcloud until revoked there. So a daemon that keeps its booted
  client after `logout` does not merely *hold* a dead credential — it
  keeps **syncing successfully** with it. "Credential liveness" here is a
  local-store statement, never a server-side one.
- **The sync actor has exactly two exit paths**
  (`taskboard-sync-nextcloud/src/actor.rs:183-231`): the command inbox
  closing (`None`, "command inbox closed") and `SystemEvent::Shutdown` on
  the broadcast. The inbox-close path is **unreachable while the engine
  lives**: the engine holds its own clone of the commands sender
  (`bootstrap.rs:319`, engine seam `sync_out`), so the receiver only
  closes when the engine itself is gone.
- **`SystemEvent::Shutdown` cannot serve as a connector-only stop.** The
  engine subscribes to the same broadcast and enters drain mode on
  `Shutdown` (`taskboard-state/src/actor.rs:263-273, 303-310`): it
  declines new work (`EngineGone`) and exits once the reports channel
  closes. That is `App::shutdown`'s whole-core sequence — broadcasting it
  from `logout` would kill the daemon's core, not just its connector.
  **This corrects the backlog's "reuses the sync actor's shutdown path"
  sketch**: the reusable pieces are the actor's graceful return path and
  the engine's *existing* reports-closed arm, not the Shutdown event.
- **The engine already survives connector death.** When the actor
  returns, its `reports` sender drops; the engine logs "sync report
  channel closed; continuing without sync" and keeps serving
  (`taskboard-state/src/actor.rs:330-340`). Engine nudges then fail
  closed: `RequestSync` → `forward_sync_now` fails → the engine answers
  `CommandOutcome::NoOp` **without** marking `Syncing`
  (`taskboard-state/src/actor.rs:406-425`) — the exact standalone-like
  world the engine's loop already handles.
- **One dishonesty already exists post-connector-death**: `sync_on`
  ignores the `NoOp` outcome (`commands/sync.rs:72-86`) — the verb would
  nudge a dead channel and wait to `TimedOut` ("did not finish in time")
  instead of saying there is no connector. Any (b) implementation must
  close this (D6).
- **The connector ceremony verbs are already honest**: `boards select` /
  `publish` preludes re-load credentials from the session store per call
  (`commands/mod.rs:845-903`) → after `logout` they fail typed
  `Credential(NotFound)`. The resident sync actor is the *only* surface
  that keeps using the booted credential.
- **REPL is out of scope by construction**: the session loop refuses
  `logout` mid-session (fixed-for-epoch, `repl.rs:193-196, 288`), and the
  REPL holds the instance lock, so it never coexists with a daemon on the
  same db. WI-H is exactly: *one-shot CLI `logout` vs a serving daemon*.
- **The wire today** (`ipc/protocol.rs`): `PROTOCOL_VERSION = 1`;
  requests `Exec{v, argv}` / `Status{v}` / `Shutdown{v}`; responses
  `Exec` / `Status{v, StatusView}` / `Shutdown{v}` /
  `VersionMismatch{server_v}`. An *undecodable* frame (unknown variant
  included) drops the connection before any version check
  (`ipc/server.rs:316-321`); the client maps that to typed `IpcTransport`
  (`ipc/client.rs:41-72`). `serde` on both ends tolerates unknown JSON
  fields (no `deny_unknown_fields` anywhere on the envelopes).
- **Client plumbing is ready**: `try_connect(db) -> Option<Connection>`
  (None = no daemon answers); request methods consume the connection.
- **Test tiers**: the IP-series boots in-process daemons via
  `bootstrap` + `spawn_server` + `BasicSession` with a **shared fake
  credential store** (`tests/ipc.rs:38-66`) — hermetic nextcloud daemons
  are possible (ip11 uses env store + wiremock; the harness can equally
  pass a `HashMap` store since `serve_with_config` shares `store` between
  bootstrap and session). The binary E2E (`tests/cli_ipc.rs`) runs the
  real daemon process + real client processes, but its header pins the
  tier contract: "process-level facts only … verb semantics stay in the
  handler suites and the IP-series".

## 2. The gate, precisely

The question is what `logout` **does to a daemon** that holds the
database:

- Under **(a)**: nothing. The daemon keeps the connector, keeps cycling
  with the deleted-but-still-valid app password; `status` gains a field
  that *says so* (probe the store at status time).
- Under **(b)**: after the local logout work succeeds, the one-shot
  client best-effort asks the daemon to stop its sync actor; the
  credential dies in memory; the daemon keeps serving local editing,
  connector-less.

Both designs below are complete enough for stage 2 to implement either
without further decisions.

## 3. Option (a) — `status` surfaces `credentials_revoked` (observe only)

### 3.1 Semantics

The daemon-side `status` answer gains an optional field reporting whether
the credential store still holds the secret the daemon booted with.
Nothing else changes: no lane moves, no new verbs, no daemon behavior
change. The operator reads the field and restarts/stops the daemon
themselves.

### 3.2 Mechanism

- `StatusView` gains
  `credentials_revoked: Option<bool>` (`skip_serializing_if = "is_none"`,
  like every optional field there).
- Presence/semantics (decision D8):
  - daemon side + `nextcloud` backend:
    - probe = `session_store().load(server, username)` where
      `(server, username)` is the **daemon's booted identity**
      (`app.config`), not the caller's;
    - `Ok(_)` → `Some(false)` (live);
    - `Err(NotFound)` → `Some(true)` (the store no longer holds what the
      daemon booted with — a boot-time-live daemon can only get here via
      logout or a manual keyring deletion);
    - `Err(Backend | Unsupported)` → `None` (inconclusive: a broken
      backend must never be reported as "revoked").
  - daemon side + `standalone` backend → absent (no credentials exist);
  - every client-side answer (`no_daemon`, `in_session`) → absent.
- Env-store daemon: `load` reads the daemon's own env snapshot → always
  `Some(false)` post-boot. Consistent: env-store `logout` is already the
  typed `Unsupported` refusal, so no revocation exists to observe.
- The keyring `load` is a blocking syscall on the serial dispatcher task
  — wrap the probe in `tokio::task::spawn_blocking` so a slow Secret
  Service cannot delay queued `Exec` requests.
- `StatusView::server` gains the probe result parameter (call site:
  `ipc/server.rs:371-374`, which already holds `session`); the daemon
  answers the same field whether `status` arrives as the `Status`
  request or as relayed `Exec` (`--json status` over IPC) — both read
  `StatusView::server`.

### 3.3 Wire impact

Additive optional JSON field on the `Response::Status` payload and on
`--json status` stdout. No new request/response variants;
`PROTOCOL_VERSION` stays 1. Old client ↔ new daemon: field ignored by
serde; new client ↔ old daemon: field absent → `None`. Extend
`wire_shapes_are_pinned` (`ipc/protocol.rs:100`) with the field. The
`--json` payload change is sanctioned pre-v0.1.0 (M1 payloads are
explicitly unversioned).

### 3.4 Client surface

`render_text` gains a clause when `credentials_revoked == Some(true)`
(presentation text, never asserted beyond fact survival, per the existing
status tests). Exit code unchanged (0). No `logout` change at all — the
surprise stays, now labeled.

### 3.5 Tests (TDD)

1. Unit, probe mapping (`commands/status.rs`): a `HashMap`-backed fake
   store — live entry → `Some(false)`; deleted → `Some(true)`;
   `Backend`-returning fake → `None`; standalone config → field absent.
   Typed/structural asserts only.
2. Unit, shapes: daemon view carries the field, client views don't;
   optional-skip serialization pinned.
3. IP journey (`tests/ipc.rs`, new ipN): boot a **nextcloud** daemon via
   `serve_with_config` with the shared fake store (boardless — cycles
   skip silently); `status` → `credentials_revoked == Some(false)`;
   delete the fake store's entry; `status` → `Some(true)`; then
   `stacks add` over IPC still code 0 (observation never mutates).
4. Binary E2E (`tests/cli_ipc.rs`): nextcloud daemon (env store +
   wiremock, ip11-style) → `status --json` shows
   `credentials_revoked: false` (presence pinned; `true` is not
   reachable hermetically at the binary tier — env store can't be
   deleted — and stays pinned at the IP tier).

### 3.6 Docs & bookkeeping

ADR 0012 amendment: replace the "stays permitted … keeps its booted
credentials until restart" sentence with the status-liveness contract
(field, tri-state, probe identity = booted identity). Backlog entry ✅
DONE. CHANGELOG line.

### 3.7 Risks

- **The original complaint survives, labeled**: the kiosk keeps syncing
  with the old account until someone restarts it; the fix is operator
  knowledge, not behavior.
- Probe cost on every daemon-side `status` (one keyring round-trip,
  `spawn_blocking`); status pollers pay it repeatedly.
- Tri-state `None` (backend broken) is honest but easy to misrender as
  "fine".
- Naming: `revoked` overstates — nothing is revoked server-side; the
  plan records the name as "no longer present in the store" (D8).
- Env-store daemons can never report `true` (structurally consistent,
  but a reader may expect otherwise).

### 3.8 Commit plan (stage 2, if (a) approved)

1. `feat(app): status credential-liveness probe` — probe seam + field +
   unit tests (3.5.1–2) + wire pin extension.
2. `feat(app): daemon-side status probe over ipc` — `spawn_blocking`
   wiring + IP journey + binary E2E presence pin.
3. `docs(app): ADR 0012 credential-liveness amendment` — ADR + backlog ✅
   + CHANGELOG.

Estimated diff: ~200–300 lines, `taskboard-app` only.

## 4. Option (b) — `logout` nudges the daemon to drop its connector

### 4.1 Semantics

The local logout work remains the act (secret deleted first, auto keys
stripped, idempotent). **After it succeeds**, the one-shot client
notifies the daemon serving the same database: the sync actor stops
gracefully, the `DeckClient` (the in-memory credential) is dropped, and
the daemon continues as a connector-less server — local editing keeps
working offline-first, exactly the closed-channel world the engine
already handles. The daemon is **not** flipped to standalone (config is
never rewritten in memory — ADR 0010 loudness: backend flips stay
explicit, on disk, by commands); it is a nextcloud-backend daemon with
no attached connector.

### 4.2 Mechanism

**(1) Domain: `SyncCommand::Detach`** (`taskboard-domain/src/messages.rs`,
additive `Copy` variant). The actor's inbox arm
(`taskboard-sync-nextcloud/src/actor.rs:187-190` grows a `Some(
SyncCommand::Detach)` arm): log + `return`. Because the select loop runs
one arm at a time and cycles run inline, `Detach` is processed strictly
*between* cycles — an in-flight cycle completes first (D4: graceful over
abort; a wedged cycle is bounded by the join budget below, which aborts
— the credential dies either way). The actor's return drops the reports
sender → the engine's existing "continuing without sync" arm fires; the
engine's own `sync_out` sends then fail closed.

**(2) App: `detach_connector`** (`bootstrap.rs`). `App.sync` becomes
`sync: std::sync::Mutex<Option<SyncActor>>` (interior mutability because
the IPC dispatcher holds `&App`). AGENTS §3 forbids `Mutex` *for
cross-actor communication* — this mutex is lifecycle bookkeeping inside
`App` and is never held across `.await`: take the actor out (lock
released), `send(SyncCommand::Detach).await` on the taken sender,
bounded-join via the existing private `join_bounded` with the same
`shutdown_timeout_secs` budget. Outcome enum:

- `NoConnector` — standalone backend or already detached (idempotent);
- `Detached` — actor joined within budget;
- `TimedOut` — aborted at budget (credential still destroyed with the
  task).

`App::sync_commands()` already returns a dead channel once `sync` is
`None` (`bootstrap.rs:159-167`) — post-detach nudges fail closed with no
further change. `App::shutdown` reads the mutex once; behavior unchanged
(the sync join is skipped exactly when the actor is gone).

**(3) Wire: `DetachConnector`** (`ipc/protocol.rs`, additive):

- `Request::DetachConnector { v, server_url, username }` — the identity
  the client just logged out;
- `Response::DetachConnector { v, outcome: DetachWireOutcome }` with
  kebab-case `"detached" | "no-connector" | "identity-mismatch" |
  "timed-out"`;

`PROTOCOL_VERSION` **stays 1** (decision D2, with recorded rationale):
client and daemon ship as one binary, so mixed versions are the
post-upgrade window (old daemon still running, new CLI). A bump to 2
would make *every* verb fail `VersionMismatch` in that window; the
no-bump cost is exactly this one verb degrading — old daemon can't parse
the variant → undecodable → connection dropped → client sees typed
`IpcTransport` → warning path (4.4). ADR 0012 amendment records the
policy: **additive variants are minor wire changes (no bump); changing
an existing variant's shape bumps.**

**(4) Server arm** (`ipc/server.rs::handle`): compare the request's
`(server_url, username)` against the session config's booted identity;
mismatch → `identity-mismatch`, **no detach** (the §2.10 degenerate
same-db-different-config guard's analog: never drop a connector whose
credential was not the one revoked). Match → `app.detach_connector()`
mapped to the wire outcome. Runs on the serial dispatcher — serialized
with every other request by construction.

**(5) Client method** (`ipc/client.rs`): `Connection::detach(self,
server_url, username) -> Result<DetachWireOutcome, CommandError>`,
mirroring `shutdown()`; plus a bounded client-side read budget (constant,
≈ the connect budget family) so a slow daemon join cannot hang `logout` —
on client timeout the daemon still completes its detach; the client
degrades to the warning.

**(6) `logout` wiring** (`commands/mod.rs::logout_command`, becomes
`async`; dispatch arm `mod.rs:330` awaits): after `logout::run` succeeds —

- derive the db path from the raw config (the same config logout used);
- `try_connect(db)`: `None` → nothing to do (no daemon holds
  credentials; **no warning** — logout without a daemon is not
  degraded);
- `Some(conn)` → `conn.detach(server, username)` bounded:
  - `Detached` → receipt note "daemon connector dropped";
  - `NoConnector` → nothing (standalone daemon);
  - `IdentityMismatch` → stderr warning (daemon serves a different
    identity; its connector keeps that identity until restart);
  - `Err(_)` (transport — including a pre-`DetachConnector` daemon —
    or version) → stderr warning "the daemon could not drop its
    connector; restart it or run `taskboard daemon stop`".
- **Exit code stays 0** whenever the local logout succeeded (the secret
  *is* deleted; D5); warnings ride stderr. Under `--json`, `LogoutReceipt`
  gains optional `daemon_connector:
  "dropped"|"none"|"mismatch"|"unreachable"|"unknown"` (additive,
  pre-v0.1.0-sanctioned). The mapping `nudge result → note/field` is a
  pure function, unit-tested (no socket needed).

**(7) Post-detach honesty riders** (both required by (b), D6):

- `sync_on` maps `CommandOutcome::NoOp` from its `RequestSync` to a new
  typed `CommandError::SyncUnavailable` (`"sync-unavailable"`, exit 1 —
  "no connector attached"): post-detach `sync` refuses immediately
  instead of lying via `TimedOut`.
- `StatusView` gains `connector_attached: Option<bool>`, present only in
  daemon-side answers under the `nextcloud` backend (`true` live actor /
  `false` detached); `render_text` appends a "connector detached"
  clause. Without it, a detached daemon would answer
  `backend: "nextcloud"` + phase `Idle` — structurally true, operationally
  misleading.

### 4.3 Wire impact summary

Two new envelope variants (additive), no version bump, one optional
`StatusView` field, one optional `LogoutReceipt` field. Extend
`wire_shapes_are_pinned` + round-trip tests. Cross-version matrix
recorded in the ADR amendment: new client → old daemon = typed transport
failure → warning; old client → new daemon = unaffected (never sends the
verb); new ↔ new = full semantics.

### 4.4 Tests (TDD)

1. **Domain**: `SyncCommand::Detach` round-trips/serializes alongside
   the existing variants' tests.
2. **Sync actor** (sync crate harness, fake state reader, no server —
   boardless cycles skip silently): `Detach` → the actor task finishes;
   a `SyncNow` queued *before* `Detach` completes first (ordering proof);
   the reports sender closes after the last evidence.
3. **Engine**: (exists conceptually via reports-closed arm; add if
   unpinned) drop the reports sender mid-life → engine still executes a
   `TaskAdd` — the "continuing without sync" contract.
4. **App level**: nextcloud boot with fake store (boardless) →
   `detach_connector()` = `Detached`; second call = `NoConnector`;
   `RequestSync` after = `NoOp`; `shutdown()` after detach completes
   without the sync join path.
5. **`sync` verb**: post-detach `sync_on` → typed
   `CommandError::SyncUnavailable` (structural assert); pre-detach
   behavior unchanged.
6. **Wire**: shape pins + round-trips for both new variants;
   `connector_attached` serialization presence rules.
7. **IP journeys** (`tests/ipc.rs`, shared fake store, boardless):
   - nudge → `detached` → `status` shows `connector_attached == Some(false)`
     and relayed `--json sync` answers envelope `sync-unavailable`
     (code 1) over the socket; mutations still serve (code 0);
   - identity mismatch: request carries another identity →
     `identity-mismatch`, actor still attached (`Some(true)`), evidence
     untouched;
   - idempotency: second `DetachConnector` → `no-connector`;
   - standalone daemon → `no-connector`.
8. **Degradation seam (unit)**: the pure `nudge-result → note/field`
   mapping — every arm (dropped/none/mismatch/transport/version/timeout).
9. **Binary E2E** (`tests/cli_ipc.rs`): a real nextcloud daemon (env
   store + wiremock) + the test speaking a hand-rolled `DetachConnector`
   frame over the daemon's real socket (ip5/ip12 precedent) → `detached`
   → real `status --json` client process shows
   `connector_attached: false`. The full binary `logout` → nudge path is
   **not** binary-testable hermetically (logout needs a deletable store
   = keyring) and stays pinned at tiers 7–8; this split matches
   cli_ipc.rs's own tier contract.

### 4.5 Docs & bookkeeping

ADR 0012 amendment: replace the logout carve-out sentence with the
detach contract — verb, identity matching, additive-variant versioning
policy, post-detach daemon behavior (engine continues; `sync`
`sync-unavailable`; `status` `connector_attached`), the client
degradation matrix, and the explicit statement that a detached daemon is
**not** a standalone flip (config untouched; ADR 0010 loudness
preserved). Backlog entry ✅ DONE. CHANGELOG.

### 4.6 Risks

- **Widest diff of the two options**: three code crates
  (domain, sync-nextcloud, app) + docs; the `App.sync` mutex refactor
  touches `shutdown()` (regression surface bounded: existing shutdown
  tests + new tier-4 tests).
- **Mixed-version window** (old daemon + new CLI): the nudge degrades to
  a warning — i.e. (a)'s world, transiently, until the daemon restarts.
  Self-healing; D2 records why the bump alternative is worse.
- **Graceful join means one last cycle may complete** with the deleted
  credential (queued `SyncNow` or in-flight cycle before `Detach`):
  bounded by the join budget; the alternative (immediate abort) loses
  cycle evidence the engine expects. Recorded as D4; review may flip it.
- **Detached daemon still answers `backend: "nextcloud"`** (config is
  deliberately untouched) — mitigated by `connector_attached`; a reader
  of `backend` alone can be misled.
- Kiosk semantics flip side: after detach the kiosk silently stops
  syncing (that *is* the requested semantics — logout means logout — but
  the status field is the only surface saying why).

### 4.7 Commit plan (stage 2, if (b) approved)

1. `feat(domain,sync): SyncCommand::Detach stops the sync actor` —
   variant + actor arm + tiers 1–2.
2. `feat(app): App::detach_connector + post-detach sync refusal` — mutex
   refactor, detach, `SyncUnavailable`, tiers 3–5.
3. `feat(app): ipc DetachConnector verb + status connector field` —
   protocol, server arm, client method, status field, tiers 6–7.
4. `feat(app): logout nudges the daemon on success` — wrapper, pure
   degradation mapping, receipt field, tiers 8–9.
5. `docs(app): ADR 0012 detach-connector amendment` — ADR + backlog ✅ +
   CHANGELOG.

Each commit standalone-green (rebase-merge hygiene). Estimated diff:
~600–800 lines.

## 5. Comparison on the dispatch axes

| Axis | (a) Observe | (b) Act |
|---|---|---|
| Wire | additive status field; no verbs; no version question | two additive variants + recorded no-bump policy + degradation matrix |
| Test surface | ~1 crate, 4 clusters, all hermetic | 3 crates, ~9 clusters, all hermetic; binary E2E for daemon side |
| Kiosk semantics | link stays factually alive until restart; operator must act | link dies at logout; daemon becomes connector-less; no operator knowledge needed |
| Credential hygiene | password stays in daemon memory | password destroyed at detach |
| Honesty gaps fixed | the surprise is *labeled* | the surprise is *removed*; `sync`/`status` riders close the adjacent gaps |
| Diff size | ~200–300 lines, 1 crate | ~600–800 lines, 3 crates |

## 6. Recommendation & rationale

**Recommend option (b).** The dispatch axes, weighed:

- **Kiosk semantics is the decisive axis.** A kiosk daemon is
  weeks-lived, and `logout` is its decommission verb. Under (a) the
  cloud link stays factually alive — the app password remains valid
  server-side after a keyring deletion — so a "logged-out" kiosk keeps
  pulling and pushing the old account for as long as nobody restarts it;
  (a) makes that visible but keeps it behaviorally intact, i.e. the fix
  is operator discipline. Under (b) logout means logout the moment it
  runs: the credential is destroyed in memory, cycles stop, the daemon
  degrades to exactly the connector-less serving shape the engine
  already handles for standalone. On the hardware this project targets,
  that is the only semantics that does not require the operator to know
  something the machine could have done itself.
- **Credential hygiene**: (b) is the only option that actually kills the
  in-memory secret — the stated concern of the origin entry
  ("credential-liveness").
- **Wire and test cost are real but bounded and mostly one-time**: the
  additive-verb policy (D2) keeps the mixed-version window benign, and
  every new test cluster is hermetic with existing harness patterns
  (shared fake store, boardless silent-skip, hand-rolled frames).
- **(b) with its honesty riders also fixes two adjacent gaps for free**
  (post-detach `sync` lying via `TimedOut`; detached daemons answering
  `backend: nextcloud` with no marker), which (a) leaves untouched.

**Why not (a)**: it is the right shape only if the goal is a warning
label at minimum cost. It leaves the operative behavior — a logged-out
machine still syncing — completely in place, and every kiosk operator
must still know to restart. **Do not implement both**: (b) makes (a)'s
probe redundant (post-detach status reports the connector directly), and
their `StatusView` field additions would collide.

**Corrected detail vs the backlog sketch** (important for review): the
backlog says (b) "reuses the sync actor's shutdown path". Literally, it
cannot — the `Shutdown` broadcast drains the *whole engine*
(§1). What (b) actually reuses: the actor's graceful-return path (via a
new `Detach` inbox command) and the engine's existing reports-closed and
fail-closed-nudge arms. The semantic is the backlog's; the mechanism is
one new domain variant.

## 7. Recorded decisions (stage 2 implements only per the approved option)

- **D1 — connector-only stop mechanism**: new `SyncCommand::Detach`
  inbox variant; never the `Shutdown` broadcast (drains the engine).
- **D2 — no protocol version bump for (b)**: additive variants with
  typed degradation beat a bump that would fail *every* verb in the
  common post-upgrade window (old daemon, new CLI). Policy lands in the
  ADR 0012 amendment. *This refines the dispatch plan's anticipated
  "envelope version bump if (b) changes the wire" — recorded here for
  plan-review to veto.*
- **D3 — identity matching**: the verb carries `(server_url, username)`;
  the daemon detaches only on an exact match with its booted identity.
- **D4 — graceful, bounded detach**: an in-flight/queued cycle completes
  before `Detach` is processed; the join is bounded by
  `shutdown_timeout_secs`, abort at budget (credential dies either way).
- **D5 — logout exit code unchanged**: the nudge is best-effort; local
  success stays exit 0; degradation is a stderr warning (+ optional JSON
  receipt field). No daemon → no warning.
- **D6 — honesty riders are part of (b)**: `sync` post-detach → typed
  `sync-unavailable`; `status` → `connector_attached`.
- **D7 — REPL untouched**: refuses logout already; session-level
  login/attach is WI-J's design round.
- **D8 — (a) probe semantics**: tri-state `Option<bool>`, booted
  identity as the key, `Backend|Unsupported` → `None`, `spawn_blocking`,
  env-store daemon always "live".
- **AGENTS §3 note (b)**: the `std::sync::Mutex<Option<SyncActor>>` is
  lifecycle bookkeeping inside `App` (never held across `.await`), not
  cross-actor communication — actor messaging stays the typed `mpsc`
  `Detach` command.

## 8. Acceptance mapping (dispatch §4 WI-H)

- *Chosen semantics implemented and pinned by IPC-level tests* — §3.5.3
  / §4.4.7.
- *Envelope version bump if (b) changes the wire* — D2 records the
  no-bump additive policy (with rationale) in the ADR amendment instead;
  flagged for plan-review approval.
- *ADR 0012 amendment* — §3.6 / §4.5.
- *Backlog entry ✅ DONE* — final commit, either branch.
- Out of scope: session-level login/attach (WI-J), remote revocation at
  the Nextcloud server, `--forget-board`/unbind ceremonies, any GUI
  surface.

## 9. Gates (stage 2)

`cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --
-D warnings`; `cargo nextest run --workspace`; `cargo test --doc
--workspace`; `cargo deny check`; `cargo llvm-cov --workspace` — all
green before handoff, per AGENTS §10.
