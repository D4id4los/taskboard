# [Plan] Growth Roadmap — Deck Client → Deployable v0.1.0

- **Date**: 2026-10-08
- **Target**: whole workspace (ordering of crate development up to a basic
  usable, deployable version tested against a real Nextcloud server)
- **Goal**: a high-level, phase-ordered roadmap covering the entire
  development path from the (now-merged) Deck client surface to v0.1.0:
  domain model, SQLite persistence, state engine, sync orchestration,
  app bootstrap with auth + secrets, single-binary CLI, app-level E2E,
  and the Slint UI.
- **Status**: ACCEPTED (planning session with user; decisions §3)
- **This plan is a roadmap**: it defines order, scope boundaries, and
  exit criteria per phase. Each phase gets its **own detailed plan file**
  in `.artifacts/plans/` before implementation begins (AGENTS.md §7).

---

## 1. Context & Known Facts (do not re-derive)

- The Deck client surface is **complete and merged** (PRs A–D of
  `.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md`):
  full read/write surface, conditional reads (`Fetch<T>`,
  `If-Modified-Since`/ETags), typed error matrix, retry policy, and the
  four-tier test harness (fixtures → wiremock → dockerized Nextcloud →
  live server).
- Everything else is scaffolding: `taskboard-domain` holds only an
  `AppState` stub (no entities, commands, or Port traits);
  `taskboard-state`, `taskboard-storage-sqlite`, `taskboard-ui-slint`,
  and `taskboard-app` are doc-comment stubs. `taskboard-app/main` boots
  tracing and an empty async scaffold — no figment, no channels, no actors.
- **Dependency chain fact**: the hexagonal design (testing strategy §1)
  puts the canonical entities and Port traits (`TaskRepository`) in
  `taskboard-domain`. Therefore **domain comes before storage-sqlite** —
  the sqlite adapter and the state engine both consume those contracts.
  The originally imagined "storage first" order does not compile.
- **Sync orchestration fact**: Deck has no write-side `If-Match`
  optimistic concurrency; conflict policy must live app-side as pure,
  proptest-able functions. Offline-first identity (local UUIDv7 before
  remote sync) is already stated in `docs/architecture.org`.
- **Testing fact**: tier 2 (dockerized Nextcloud + `occ`-minted app
  password) and tier 3 (live server via `.env`) harnesses exist and are
  reusable at the app level for E2E.

## 2. Confirmed Product Decisions (user, 2026-10-08)

1. **No new orchestrator crate.** The documented topology stands
   (ADR 0001, AGENTS.md §3): the sync actor lives inside
   `taskboard-sync-nextcloud`; `taskboard-state` remains the central hub;
   merge/conflict policy is pure functions in `taskboard-domain`.
2. **Two milestones, CLI-first.** M1 = headless daemon + CLI deployed and
   validated against the real Nextcloud server. M2 = Slint UI on the same
   core, tagged v0.1.0. Real-world validation precedes UI investment.
3. **Auth = app password + OS keyring.** First-run `login` prompts for
   server URL, username, and an app password created in the Nextcloud web
   UI (the model already decided in testing strategy §8). The secret goes
   into the OS keychain (`keyring` crate); `taskboard.toml` holds only
   non-secrets. OAuth2 is a recorded future improvement, not now.
4. **Single-binary CLI, in-process.** One `taskboard` binary with clap
   subcommands; every non-daemon command boots the full core in its own
   process, does the job, flushes, exits. A lockfile prevents concurrent
   instances. Daemon IPC is a recorded future improvement.
5. **Agile cross-crate evolution (user, 2026-10-09).** No crate or
   module is frozen after its phase: when a later phase finds that
   changing an earlier phase's output yields a better architecture,
   cleaner responsibilities, or better testability, that change is made
   in the later phase's PR without ceremony. The full test suite
   (contract harness, proptests, snapshots, quality gates) is the
   safety net that keeps such refactors confident. Phase-plan "out of
   scope" sections state what a phase *delivers*, never which crates it
   may touch.

## 3. Development Order & Rationale

Dependency-driven chain:

```
domain → storage-sqlite → state → sync actor → app bootstrap → CLI →
app-level E2E → UI
```

Phases 2 and 3 are partially parallelizable (state can be developed
against a fake `TaskRepository`), but the recommended order is serial.
The roadmap's recommended PR granularity is **one plan/PR per phase**
(or stacked PRs where a phase splits naturally, e.g. domain entities vs.
merge policy).

### Phase 0 — Finish the Deck client ✅ (complete)
Client surface, conditional reads, and the four-tier harness are merged.
Attachments writes/comments/ACL writes remain backlog entries by design.
**Exit: reached.**

### Phase 1 — `taskboard-domain`: canonical model + contracts
- Canonical entities (`Board`, `Stack`, `Task`, `Label`) with offline-first
  identity: local UUIDv7 ids + optional remote Deck ids.
- Real `AppState`: task tree + `SyncStatus` + `last_updated`.
- Message enums: `StateCommand`, `SystemEvent`, `EngineSignal`, sync-actor
  commands, oneshot query payloads.
- Port traits: `TaskRepository` (+ clock seam where needed), following the
  `RetrySleep` seam pattern from the sync crate.
- Pure merge/conflict-resolution functions (last-writer-wins per field,
  delete-vs-edit rules — exact semantics designed in this phase's plan;
  no server preconditions to lean on).
- Heavy proptest. Domain stays Deck-agnostic; DTO→domain mapping lives in
  the sync crate (Phase 4).
- **Exit**: entities + ports + policy functions, proptests green.

### Phase 2 — `taskboard-storage-sqlite`
- Schema + sqlx migrations: boards/stacks/tasks/labels, join tables,
  `sync_metadata` (remote ids, etags/last-modified), and a durable
  **outbox** of pending local operations (crash-safe offline write queue).
- `SqliteTaskRepository` implementing the domain Port; `query!`
  compile-time-checked queries; round-trip tests (in-memory + file-backed)
  and proptest round-trips.
- Storage actor wrapper (mpsc loop commanded by the state engine).
- **Exit**: repository passes the domain contract tests against sqlite.

### Phase 3 — `taskboard-state`: the StateEngine
- Command loop → pure transition functions → `ArcSwap<AppState>` publish +
  `EngineSignal` broadcast; oneshot queries; persistence dispatch to the
  storage actor; ingestion of sync results; `SyncStatus` tracking.
- insta snapshot tests (command sequences → `AppState` trees) with a fake
  `TaskRepository`; injected clock; no sleeps (AGENTS.md §5).
- **Exit**: snapshot suite defines the engine's contract.

### Phase 4 — Sync actor (inside `taskboard-sync-nextcloud`)
- Poll loop over conditional reads (boards → changed stacks → cards) with
  etag/last-modified bookkeeping persisted in `sync_metadata`.
- Push side consuming the storage outbox; conflicts resolved via the
  Phase 1 pure policy; DTO↔domain mapping functions (proptest-able).
- Offline handling: transport failure → `SystemEvent::NetworkLost`,
  backoff, outbox retained in sqlite.
- Tests: wiremock actor tests with `tokio::time::pause()` for scheduling;
  dockerized two-way sync scenarios (local→push, remote→pull, concurrent
  edit→policy outcome) reusing the tier-2 harness.
- **Exit**: two-way sync demonstrated against dockerized Nextcloud.

### Phase 5 — `taskboard-app`: bootstrap library + config/secrets
- Refactor `main` into a reusable `bootstrap` function/module: figment
  config → channel graph → actor spawns → handles. Shared by the daemon
  mode and every CLI subcommand (the "in-process core" decision).
- figment `Config`: `[nextcloud] server_url/username`, `[storage] db
  path`, `[sync] poll_interval/backoff`, `[app] mode`. **No secrets in
  TOML.**
- `keyring` integration (new workspace deps: `keyring`, `clap`, and a
  prompt helper such as `rpassword`); typed error when the secret is
  missing; graceful shutdown (tokio signal) with pending-op flush.
- **Exit (M1 checkpoint)**: headless daemon runs against the real server.

### Phase 6 — CLI subcommands + app-level E2E (M1 exit)
- `taskboard login` (prompt URL/user/app-password, verify via `boards()`,
  store in keyring), `boards list/select` (first-run board binding; MVP is
  single-board), `tasks list/add/done/move`, `sync` (trigger + status),
  `daemon`; `--json` output for test assertions; instance lockfile.
- **New test tier: app-level E2E** driving the real binary against the
  dockerized Nextcloud (tier-2 compose + occ-minted app password):
  login→select→add→verify-remote and remote-create→pull→list flows; a
  live-server manual runbook reuses tier-3 creds.
- **Exit (M1)**: deployable, real-server-validated headless taskboard.

### Phase 7 — `taskboard-ui-slint` (M2 → v0.1.0)
- Add the `slint` dependency (renderer decision per the existing backlog
  entry: renderer-skia vs renderer-femtovg for kiosk hardware); board /
  stack / task views reading `ArcSwap` and dispatching `StateCommand`s;
  sync-status / offline indicator; `slint::testing` component tests.
- Ship v0.1.0 via `scripts/bump_version.sh` + the existing release
  pipeline. Kiosk rotation mode stays a later phase.
- **Exit (M2)**: tagged v0.1.0 with desktop UI, released binaries.

## 4. Out of Scope → Backlog (entries appended with this plan)

- OAuth2 device/PKCE login (revisit when app-password UX hurts).
- Daemon IPC / remote control (unix-socket protocol; kiosk management).
- Reminders/notifications subsystem — core to project identity, not yet
  designed; deserves its own planning round early post-M2.
- Multi-board support (MVP binds one board).
- Keyring fallback for headless Linux without Secret Service (SSH boxes).
- (Existing entries stay: attachments writes, comments/ACL endpoints,
  Nextcloud version matrix, kiosk mode, second-priority release targets.)

## 5. Risks & Mitigations

- **Conflict semantics without server preconditions**: design the policy
  as pure functions with property tests in Phase 1 *before* any sync
  wiring; docker-tier scenarios in Phase 4 encode the chosen outcomes.
- **Deck soft-delete vs local tombstones**: DELETE responses are
  authoritative (established fact); tombstone handling is designed in the
  Phase 2 (storage) plan, not improvised in the actor.
- **Card reorder path-vs-body stack semantics**: flagged by the client
  plan; verify at tier 2 during Phase 4 before building the outbox
  operation type for moves.
- **Keyring on headless servers**: Phase 5 must define typed behavior
  (clear error + config-file escape hatch decision) for boxes without a
  Secret Service; tracked in backlog either way.
- **Slint renderer choice for kiosk hardware**: defer to Phase 7's plan;
  the existing backlog entry owns the evaluation.

## 6. Verification & Acceptance (overall)

- Every phase passes the full quality gate (fmt/clippy -D warnings/
  nextest/doc tests/cargo-deny) before handoff.
- M1 acceptance: from a clean machine, `taskboard login` against the real
  server, add a task offline, bring the network up, and observe the task
  on the Nextcloud web UI; and the reverse direction from the web UI.
- M2 acceptance: v0.1.0 tag with released Linux + Windows binaries that
  pass the same E2E suite with the UI mounted.
