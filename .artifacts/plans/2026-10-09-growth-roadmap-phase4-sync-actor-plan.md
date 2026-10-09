# [Plan] Growth Roadmap Phase 4 — Sync Actor: Push Cycle, Conditional Pull, Offline Reporting

- **Date**: 2026-10-09
- **Target**: `taskboard-sync-nextcloud` (the sync actor: `actor.rs`, push
  planner, DTO→domain mapping, poll scheduler), additive domain modules in
  `taskboard-domain` (`push.rs` planner, `SyncReport`/`SyncErrorKind`
  extensions, `SyncStateReader` port), small engine extensions in
  `taskboard-state` (`EngineCommand::ReadState`, report-ingestion updates),
  and a storage codec addition (`ValidatorKey::ArchivedStacks` tag).
- **Goal**: implement the roadmap's Phase 4 — the sync actor inside
  `taskboard-sync-nextcloud`: a scheduled cycle that pushes the storage
  outbox to Deck (coalesced, dependency-ordered, fetch-before-write),
  pulls the bound board via conditional reads into a complete
  `RemoteBoardSnapshot`, and reports `SyncReport`s to the State Engine —
  with offline handling (`SystemEvent::NetworkLost`, failure backoff,
  outbox retained), DTO↔domain mapping (proptest-able), the wiremock
  actor suite (`tokio::time` paused scheduling), and dockerized two-way
  sync scenarios as the exit demonstration.
- **Status**: ACCEPTED (user review 2026-10-09; rev 2 rebases onto the
  merged phase 3 and records the §0 reconciliation outcome)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 4. Exit criterion: "two-way sync demonstrated against
  dockerized Nextcloud."
- **Predecessors**: Phase 1 (merge policy R1–R9 + remote views, merged),
  Phase 2 (outbox + validators persistence, merged), Phase 3 (engine +
  nudge contract, **merged to `main` as `93ab5fb`; §0 records the
  reconciliation against it**).

---

## 0. Phase 3 Reconciliation Gate — executed (2026-10-09, against `main` @ `93ab5fb`)

Authored against `feat/state-engine-actor` @ `726a765`; Phase 3 merged to
`main` as one squash (`93ab5fb` "Phase 3 review fixes — Miri gates,
single publish, coherence"). Gate findings, each re-verified in the
merged tree:

- **`spawn_state_engine` — unchanged** (same six injected parameters);
  §5.1's wiring assumptions hold verbatim.
- **`EngineCommand` — unchanged** (`Execute`/`Flush` only); the §4.1
  `ReadState` addition lands exactly as planned.
- **`SyncReport` — untouched** (`messages.rs` is not in the merge
  diff); the §4.2 reshape applies as written. The storage crate is
  likewise untouched — the §4.4 `ArchivedStacks` codec tag lands on the
  same code.
- **Adopted change — `persisted_view()` divergence**: the review fixes
  documented that the memory-only `Syncing` transient appears in
  `EngineCore::persisted_view()` even though no repository `load()`
  could ever return it. The §4.1 `ReadState` reply inherits that
  divergence; §4.1 now pins how a `SyncStateReader` consumer must treat
  it.
- **Adopted change — single publish per message**: the engine now emits
  at most one swap + signal per mutating message (entity change and the
  `Syncing` transient in one publish, with the nudge sent before the
  publish). Engine-internal; A2/I-series expectations are unaffected,
  and the in-code guarantee the change documents — "the report it
  triggers cannot be ingested until this message finishes" — is exactly
  the ordering the I-series E2E tests ride on.
- **Noted, irrelevant here**: `plan_command` hardening (existence
  guards before changeset guards; an outbox-independence proptest) and
  the new Miri `cfg_attr` environment gates touch no phase 4 contract.
- **architecture.org**: phase 3 refreshed the state-crate bullet and
  the channel table, but the `taskboard-sync-nextcloud` dependency list
  is **still stale** relative to its manifest — §9's refresh duty
  stands.
- Residual duty at implementation time: re-run this checklist once more
  if anything else lands on `main` before the branch starts.

## 1. Context & Known Facts (do not re-derive)

- **Working agreement (roadmap §2.5)**: later phases may reshape earlier
  crates without ceremony; the test suite is the safety net. This plan
  exercises it: §4.2 changes `SyncReport` (Phase 1's type), §4.1 adds an
  `EngineCommand` variant (Phase 3's envelope) — the phase 3 plan §12
  explicitly reserved both changes for Phase 4.
- **The client surface is complete** (`crates/taskboard-sync-nextcloud/src/client.rs`):
  `DeckClient::new(base, user, token)` (app password, Basic auth,
  `OCS-APIRequest: true`), unconditional reads (`boards`, `board(id)`,
  `stacks(board, StackFilter)`, `card(b,s,c)`, `labels(board)` — the
  latter 405s on Nextcloud 35; board reads carry labels inline), **full
  write surface** (`create/update/delete_board`, `create/update/
  delete_stack`, `create_card`, `update_card` (full round-trip PUT),
  `delete_card`, `archive/unarchive_card`, `reorder_card` (destination
  stack in the **body** — tier-2 verified; returns `()` and no usable
  echo), `assign_label`/`remove_label`, `create/update/delete_label`),
  and **conditional reads** `fetch_boards(&Validators)`,
  `fetch_stacks(board, StackFilter, &Validators)`,
  `fetch_card(b,s,c,&Validators)` returning `Fetch<T> { data: Option<T>,
  validators: Validators }` — `304` maps to `data: None` before decoding,
  validators are opaque strings the caller threads (the client is
  stateless). Write-param structs: `NewCard` (title/order/description/
  duedate; **no labels, no done, no archived**), `StackChanges` (title +
  order, full-send), `LabelChanges` (title + color, full-send),
  `BoardChanges` (full-send).
- **Retry reality**: every call rides `send_with_retries` — 3 attempts,
  500 ms doubling, 8 s cap, retrying only `Transport`/`RateLimited`/
  `Unavailable`; `RetrySleep` is an injected seam. **No `Retry-After`
  handling** (backlog). Deck has **no write-side `If-Match`** and no
  idempotency keys: a retried or duplicated create POST re-applies — the
  design must minimize duplicate-create windows (§3 decision 5) and the
  merge layer tolerates echoes unconditionally (R9a).
- **Typed error matrix** (`error.rs`): `Transport(_)` (network lost),
  `Envelope(serde_json)` (2xx body failed to decode — **only reachable on
  2xx**, so the write succeeded), `Unauthorized`/`Forbidden`/`NotFound`/
  `BadRequest`/`Conflict`/`PreconditionFailed`/`Server(u16)`/`RateLimited`
  /`Unavailable`. "Network lost" = `Transport` or exhausted
  `RateLimited`/`Unavailable`.
- **The policy half already exists and is merged**: `pipeline::
  apply_sync_report(current, outbox, snapshot, pushes, ids, now) ->
  (AppState, Vec<PersistenceAction>)` with the fixed order push outcomes
  (R9) → board → stacks → labels → tasks → presence reconciliation (R3)
  → cascade (R6) → sync-status transition. `RemoteBoardSnapshot { board,
  stacks, tasks, labels }` carries a **binding completeness contract**
  (remote.rs): absence from the snapshot means absence server-side —
  "Enforced in Phase 4 actor tests; a partial snapshot would silently
  tombstone live tasks." `PushOutcome { op: OpId, result: PushResult }`;
  `PushResult::{Applied { echo: Option<RemoteEcho> }, RemoteMissing,
  Rejected { kind }}` — `Rejected { BadRequest }` dead-letters, others
  stay queued. `RemoteIndex` maps remote refs → local ids from bindings.
- **The R3 hazard that forces cycle order**: presence reconciliation
  tombstones every bound entity absent from the snapshot. A snapshot
  fetched **before** this cycle's pushes therefore reports freshly created
  cards as absent → tombstoned. **The cycle must push first, then pull.**
  Verified in `pipeline.rs` §6 (no `!local.deleted` guard on tasks).
- **Validators persistence exists end-to-end except the report**:
  `ValidatorKey::{Boards, Stacks(RemoteBoardId)}` (serde tags
  `"boards"` / `"stacks:<u64>"`), `SyncValidators { etag, last_modified }`,
  `PersistedState.validators: BTreeMap<ValidatorKey, SyncValidators>`,
  `PersistenceAction::UpsertValidators`, storage table `sync_metadata`,
  `apply_actions` replaces by key, engine's `interpret` handles it. The
  **only missing link is `SyncReport` itself** — the phase 3 §12 known
  gap: "validators must flow engine-ward for persistence, so Phase 4
  should change `SyncReport` in the domain — new field or envelope
  variant — whichever its plan finds cleanest."
- **Engine facts**: `spawn_state_engine(repo, ids, clock,
  sync_out: mpsc::Sender<SyncCommand>, sync_reports: mpsc::Receiver<
  SyncReport>, system: broadcast::Receiver<SystemEvent>)`; the engine
  forwards `SyncNow` nudges fire-and-forget (`try_send`; a full inbox
  counts as coalesced-success) iff a batch enqueued ops, on `RequestSync`,
  or on `NetworkRestored` — **cycle coalescing is this actor's job**
  (ADR 0006). `SyncCommand::{SetBoard(RemoteBoardId), SyncNow}` —
  `SetBoard` is defined but **never consumed anywhere** (first-run
  binding is unimplemented; this plan consumes it, §3 decision 10). The
  published `AppState` deliberately omits the outbox and validators;
  `EngineCore::outbox()` is unreachable from outside the loop. ADR 0005:
  "Phase 4 (sync) reads outbox order **via the engine, never direct SQL**."
  The engine's `Failed { kind }` ingestion writes
  `SyncStatus { phase: Failed, last_success: kept }`; `NetworkLost` →
  persisted `Offline` (idempotent); `NetworkRestored` → nudge only.
- **Outbox facts** (Phase 2): ops are `PendingOp { op_id: OpId, op:
  LocalOp, queued_at }`, `LocalOp` is id-scoped (12 variants, no field
  sets), queue order = `op_seq` (monotonic), addressed by `OpId` only;
  `CompleteOp`/`FailOp` delete by id (absent id = no-op). **Op coalescing
  is explicitly push-side Phase 4 logic** (Phase 1 §8, Phase 2 §11) —
  storage stores and orders only. Tombstones are retained forever (MVP).
- **Deck wire facts that shape the mapping**: entity-level
  `last_modified` (epoch **seconds**, i64) and `deleted_at` (0 = live) on
  boards/stacks/labels; **cards have no `deletedAt`**; remote ids are u64
  unique per board (refs carry board context); `Card.labels` accepts
  id-or-object (`CardLabel` → bare id on serialize); empty stacks send
  `cards: null`; NC35 returns bare JSON for `Accept: application/json`
  (envelope fallback handled by the client); the known-server matrix
  records Deck cache lag after mutations (echo/lastModified may lag).
- **Testing assets**: Tier 0 fixtures (`tests/fixtures/deck/*.json` +
  `examples/harvest_deck_fixtures.rs`), Tier 1 wiremock suite
  (`RecordingSleeper`, `SequenceResponder`, `last_body`) in
  `tests/contract.rs`, Tier 2 dockerized Nextcloud
  (`scripts/nextcloud_it_setup.sh up [--ci]`, env
  `TASKBOARD_IT_DOCKER_{URL,USER,TOKEN}`, run via
  `cargo nextest run -p taskboard-sync-nextcloud --run-ignored only -E
  'test(it_nextcloud_docker)'`; **CI runs the tier in
  `.github/workflows/nextcloud-integration.yml` — `workflow_dispatch` +
  weekly (Wednesdays 04:00 UTC), deliberately out of the per-PR path
  (Docker Hub pull-rate 429s made a required check flaky; per-PR CI
  keeps the wiremock contract suite only)**), Tier 3 live (`.env`:
  `TASKBOARD_IT_NEXTCLOUD_{URL,USER,TOKEN}`). The sync crate's dev-deps
  already include `tokio/test-util` (pause/advance), `wiremock`,
  `proptest`, `dotenvy`. Domain `test-support` provides
  `snapshot_strategy`, `push_outcome_strategy`, `persisted_state_strategy`,
  `InMemoryRepository`, `CountingIds`. CI: miri.yml and
  mutation-testing.yml target only domain+state — **this phase starts
  neither gate** (scoped extension is a deliberate change, not accidental).
- **House rules**: `#![forbid(unsafe_code)]`, SPDX `MIT OR Apache-2.0`,
  `# Errors` doc sections, no `unwrap`/`expect` in non-test code, tracing
  spans across `.await`, typed outcome assertions only, no sleeps in tests
  (pause/advance, injected `RetrySleep`/clock seams, reply/JoinHandle
  completion signals), one behavior one test at the lowest layer (pure
  mapping/planner/coalescing tests precede actor routing tests), fakes
  over mocks.

## 2. Scope

**In scope**:

| Area | Deliverable |
|---|---|
| Domain: read port | `SyncStateReader` trait (persistence.rs) + `EngineCommand::ReadState` + `impl SyncStateReader for EngineHandle` + `impl` for `InMemoryRepository` (§4.1) |
| Domain: report shapes | `SyncReport::{Completed + validators, Failed + pushes}`, `SyncErrorKind::NoBoard`, `apply_push_report` pure fn (§4.2) |
| Domain: push planner | `push.rs`: pure coalescing `plan_pushes(&[PendingOp]) -> Vec<PushGroup>` (§4.3) |
| Domain: mapping | `mapping` responsibilities land in the sync crate as pure functions DTO → `Remote*` views + `DeckError → SyncErrorKind`/`PushResult` classification (§5.2–5.3) — **not** in the domain (they are Deck-specific by definition) |
| Storage: codec | `ValidatorKey::ArchivedStacks(RemoteBoardId)` variant + serde tag `"archived_stacks:<u64>"` (§4.4; no SQL change) |
| Engine | `ReadState` routing + validators/pushes-aware report ingestion (§5.5) |
| Sync actor | `spawn_sync_actor`, cycle state machine (idle/nudge/backoff/offline), push executor with binding overlay + fetch-before-write, pull assembler (both stack listings), snapshot assembly, report emission (§5) |
| Tests | pure mapping/planner/classification suites (U/P), wiremock actor suite A-series (paused time), hermetic E2E I-series (engine+actor in-process), docker D-series two-way sync scenarios (§7) |
| Docs | ADR 0007, architecture.org refresh, CHANGELOG, backlog appends (§9) |

**Out of scope** (later phases unless the agility doctrine demands a
touch): config keys and channel-graph ownership (`[sync] poll_interval/
backoff` land in Phase 5's figment config — this phase takes them as
injected `SyncActorConfig` values); CLI (`SetBoard` senders, login —
Phase 6); UI; reminders; multi-board; attachments/comments/ACL writes;
daemon IPC. Also out: retrying writes beyond the client's built-in
policy, `Retry-After` support, tombstone pruning, engine-side debounce
(all backlogged, §8).

## 3. Design Decisions

1. **The sync crate consumes state only through a domain port:
   `SyncStateReader`.** ADR 0005 mandates the outbox is read "via the
   engine, never direct SQL"; architecture.org's dependency list forbids
   `taskboard-sync-nextcloud` → `taskboard-state`/`-storage-sqlite`. The
   domain therefore grows a second port beside `TaskRepository`:
   `trait SyncStateReader { fn read_state(&self) -> BoxFuture<'_,
   Result<PersistedState, RepositoryError>>; }` — implemented (a) for
   `EngineHandle` inside `taskboard-state` over a new
   `EngineCommand::ReadState { reply }` envelope (the engine replies with
   `EngineCore::persisted_view()` — the full persisted shape including
   outbox and validators, which `AppState` deliberately omits), and (b)
   for `test_support::InMemoryRepository` (returns its snapshot) so every
   test layer shares one fake. The engine processes `ReadState` in its
   sequential loop, so a reply reflects the settled post-interpret state —
   same durability story as `Flush`. *Rejected*: an outbox-dump
   `EngineCommand` variant returning only the outbox (narrower, but the
   actor equally needs bindings and validators; a per-field API would
   grow variant-by-variant — backlogged query-envelope entry is superseded
   by this decision); reading sqlite directly (violates ADR 0005 and the
   crate dependency list).
2. **Push before pull — normative cycle order: read → push → pull →
   report.** Push outcomes are reported together with the snapshot of the
   **post-push** server state: R9 echo adoption establishes baselines
   before the snapshot merge (the pipeline's documented order assumes
   exactly this), and the R3 hazard (§1) is structurally avoided — a card
   created and pushed this cycle is present in the snapshot. *Rejected*:
   pull-first (tombstones fresh creates); separate reports for pushes and
   pulls (two engine transitions per cycle, and the Failed-path window
   between them re-introduces duplicate creates).
3. **Evidence-then-verdict: `SyncReport::Failed` carries the cycle's
   successful `pushes`.** A cycle that pushed five ops and then lost the
   network during the pull reports `Failed { kind: Network, pushes:
   [five outcomes] }` — the completed pushes are applied (ops complete,
   create echoes bind) and only the pull is retried next cycle. Without
   this, every network flap during the pull re-POSTs completed creates →
   duplicate Deck cards (no idempotency keys exist to save us). The
   domain grows the matching pure function `apply_push_report(current,
   outbox, pushes, ids, now) -> (AppState, Vec<PersistenceAction>)` =
   the pipeline's step 1 + step 7 (push outcomes, then status
   transition), reusing `apply_push_outcomes` — no new policy, a narrower
   composition of the existing one. The engine's `Failed` arm uses it iff
   `pushes` is non-empty; the empty-pushes `Failed` keeps today's single
   `UpsertSyncStatus` path. *Rejected*: reporting pushes immediately as
   mini-`Completed`-shaped messages without snapshots (a `Completed`
   report without a snapshot would violate the engine's ingestion
   contract; a new envelope variant multiplies shapes for no semantic
   gain).
4. **`SyncReport::Completed` grows `validators:
   BoardPullValidators`.** Closing the phase 3 §12 gap with a struct on
   the variant (not a new envelope): `BoardPullValidators { boards:
   SyncValidators, stacks: SyncValidators, archived_stacks:
   SyncValidators }` — exactly the three conditional endpoints one cycle
   touches, keyed by the bound board (the engine derives
   `ValidatorKey::{Boards, Stacks(board), ArchivedStacks(board)}` from
   its own binding when appending `UpsertValidators` actions — the actor
   never reasons about storage keys). The engine appends these actions
   into the ingestion batch exactly like its `UpsertSyncStatus` append
   (iff the stored value differs — cheap O(1) compare against
   `EngineCore`'s validators). On `Failed { .., validators: None }` the
   engine writes nothing validator-related. This makes the actors
   restart-cheap: the next boot re-reads persisted validators via
   `read_state` and resumes conditional polling.
5. **Fetch-before-write for every op touching an existing bound card**:
   one unconditional `GET card` immediately before the card's write
   group. Rationale: the full round-trip `update_card` PUT is built from
   the **fetched** card with local fields applied — pushing a locally
   cached card from before a remote change would clobber; and the
   fetched `stack_id` is the fresh location, protecting `reorder`/
   `assignLabel`/`delete` from stale-binding 404s that would be misread
   as `RemoteMissing` (a false delete-wins). Creates and stack/label
   full-sends skip the pre-flight (nothing to round-trip). Cost: one GET
   per touched card per cycle — negligible at kiosk scale. *Rejected*:
   trusting the local binding without a pre-flight (false
   `RemoteMissing` is a data-destroying misclassification; the GET is
   the cheap insurance).
6. **Coalescing is a pure domain planner, executed by the actor.**
   `domain::push::plan_pushes(&[PendingOp]) -> Vec<PushGroup>` groups ops
   per entity (queue order preserved) and computes, per group, the
   materialized client intent plus which subsumed op ids ride on it.
   Pure ⇒ proptest laws (§7 P-series); the actor maps groups → client
   calls using the binding overlay. Per-entity rules (normative table in
   §4.3): a delete subsumes everything; a create subsumes updates/moves
   (the create carries current state) while label assignments ride along
   post-create; otherwise the group materializes one full-card PUT (from
   current state; covers `SetTaskDone`, repeated edits, and label-set
   changes via the PUT's whole label array) plus at most one reorder
   (only if the final position differs from the pre-flight GET). Stacks
   and labels: create subsumes later updates of the same entity; a
   delete subsumes all. **Ordering across groups**: stacks, then labels,
   then tasks — the dependency direction of `CreateTask`-into-`CreateStack`
   and label resolution; within a kind, queue order. Subsumed ops get
   **synthetic `Applied { echo: None }` outcomes only when the
   materialized op succeeds**; on failure they stay queued with their
   materialized sibling (one outcome per failed group is not emitted —
   only op ids whose pushes happened get outcomes).
7. **Binding overlay, not rebinding by search.** The actor builds an
   in-cycle `RemoteIndex` from the read state, then extends it as create
   echoes return (`CreateStack` echo → `stack_by_ref`, etc.). A group
   whose entity is unbound and whose dependencies are unresolved after
   the earlier groups ran reports `Rejected { kind: LocalData }` for its
   materialized op (stays queued — R9f keeps it; the next cycle retries
   after the dependency landed). This is the only path that produces
   `LocalData`. *Rejected*: resolving by title/order search (silent
   mis-binding risk).
8. **Push classification `DeckError → PushResult`** (pure fn, table in
   §5.3): success-with-body → `Applied { echo: Some }`; success-with-
   unusable-body (`reorder`, and **`Envelope` decode failures on write
   echoes — the status was 2xx, so the write landed**) → `Applied { echo:
   None }`; `NotFound` on an existing-entity op → `RemoteMissing`;
   `Forbidden` on delete ops → `RemoteMissing` (treat as gone;
   tier-2-verify, §7 D-series) — otherwise `Forbidden`/`Unauthorized` →
   `Rejected { kind }` (stays queued; never fabricate deletes from an
   ACL problem); `BadRequest` → `Rejected { BadRequest }` (dead-letters
   via R9f); transport/exhausted → **no outcome at all** (the push
   aborted; the cycle fails, nothing is reported for unattempted/
   ambiguous ops — decision 3's evidence rule only reports what
   completed).
9. **Pull = three conditional reads + cache fill; both stack listings.**
   Per cycle: `fetch_boards` → locate the bound board →
   `fetch_stacks(board, Active)` **and** `fetch_stacks(board, Archived)`
   (user decision 2026-10-09: archived cards sync; otherwise the
   completeness contract would tombstone archived cards and unarchive
   would resurrect them as new local tasks). A `304` on any endpoint is
   filled from the actor's in-memory cache of the last decoded listing;
   a cache miss (cold boot — mitigated by seeding the cache from the
   persisted validators' *presence*, never their content, so a warm
   validator with a cold cache forces one unconditional fetch) cannot
   produce a partial snapshot: if any listing is unavailable, the cycle
   fails with `Failed { kind, pushes }` and **no snapshot is reported**
   (never pair a fresh push list with a stale/partial snapshot —
   R3 hazard). Validators from all three responses travel in the report
   (decision 4). `fetch_card` is not part of the pull loop (listings
   carry full cards; tier-2-verify field parity, §7).
10. **`SetBoard(RemoteBoardId)` is consumed by the actor: it sets the
    pull target, clears the in-memory caches (not the persisted
    validators — they are per-board-keyed), and triggers an immediate
    cycle.** The *local* board binding is not written by the actor at
    all: the next successful pull adopts the remote board via R1
    (`adopt_remote_board` in the pipeline binds by remote id). A
    re-`SetBoard` to the same id is a no-op; to a different id swaps the
    target (MVP is single-board; the swap path is a documented
    consequence, not a supported flow). If no board is set when a cycle
    fires, the actor reports `Failed { kind: NoBoard, pushes: [] }` once
    per triggered cycle (not on poll ticks) so a pre-binding `RequestSync`
    surfaces instead of leaving the engine's `Syncing` badge stuck — this
    is why `SyncErrorKind` grows `NoBoard`.
11. **Board absence from the listing is a tombstone, verified in two
    steps.** The board not appearing in `fetch_boards`' data → one
    unconditional `GET board(id)`: if it returns the board (Deck may
    list or hide deleted boards — tier-2-verify), its `deleted_at`
    travels in the snapshot and the pipeline's cascade handles the rest;
    if `NotFound`/`Forbidden`, the actor synthesizes a `RemoteBoard`
    with `deleted_at = Some(now)` (delete-wins fallback, consistent with
    R3/R9e semantics; **all three reads of a dead board are expected to
    404 too — the synthesized board + empty listings snapshot is valid**).
    A filtered-listing false alarm (server hiding live boards) would
    cascade-destruct — hence the two-step confirm and the D-series
    docker scenario; a fallback to `fetch_boards`-unfiltered (if Deck
    grows one) is backlogged, not speculative.
12. **Scheduling: one `tokio::select!` over commands and a computed
    deadline; cycle-in-flight dedupe; failure backoff.** Loop state:
    `running: bool`, `rerun_requested: bool`, `failure_streak: u32`,
    `offline: bool`. `SyncNow` while a cycle runs sets
    `rerun_requested` (the ADR 0006 absorber — nudge storms collapse);
    `SyncNow` while idle starts a cycle immediately. After a cycle, the
    next deadline = `poll_interval` (success) or `backoff(streak)`
    (doubling from an injected initial value to a cap — same shape as the
    crate's `BackoffPolicy`, new pure fn `poll_backoff(streak) -> Option<
    Duration>` on an injected `RetrySleep`-style seam so `pause()/
    advance()` tests it deterministically). Any command receipt
    preempts the wait. Transport-classified cycle failure → if not
    already offline: broadcast `SystemEvent::NetworkLost`, report
    `Failed { Network, pushes }`, advance the streak; first success after
    offline → report `Completed` first, **then** broadcast
    `NetworkRestored` (the resulting engine nudge lands on a running
    cycle's dedupe flag — one benign extra coalesced rerun, documented).
    Non-transport failures (`Auth`, `Server`, `LocalData`, `NoBoard`)
    also back off but never broadcast network events. `SystemEvent::
    Shutdown` (or channel close) exits the loop cleanly.
13. **The actor is stateless across restarts except the injected
    client.** All durable facts (outbox, bindings, validators, board
    binding) live in the engine's persistence and arrive via
    `read_state`; the actor's in-memory cache is a rebuildable
    optimization. Consequence: no actor-side persistence, no new
    storage schema, and `spawn_sync_actor` takes everything by
    parameter (the Phase 5 bootstrap owns the channel graph, mirroring
    `spawn_state_engine`'s injection pattern). Signature (§5.1):
    `spawn_sync_actor(client: DeckClient, state: Arc<dyn SyncStateReader>,
    commands: mpsc::Receiver<SyncCommand>, reports: mpsc::Sender<
    SyncReport>, system: broadcast::Sender<SystemEvent>, cfg:
    SyncActorConfig) -> JoinHandle<()>` with `SyncActorConfig {
    poll_interval, backoff_initial, backoff_max }` (Phase 5 wires
    figment values; tests inject tiny/paused values).
14. **The engine never learns about network events from the sync
    crate's internals — only via the documented channels.** The actor
    produces `SyncReport`s (mpsc) and `SystemEvent`s (broadcast sender,
    injected); the engine consumes reports and its injected system
    receiver. No shared state, no callbacks, ADR 0001 topology
    verbatim. The system broadcast **sender** half is owned by the
    Phase 5 bootstrap (this phase's tests create their own).
15. **Observability is a deliverable** (Phase 2/3 discipline): `error!`
    — a cycle aborted mid-push without any reportable evidence;
    `warn!` — push groups rejected (`LocalData`), dead-lettered ops,
    snapshot-cache misses forcing unconditional fetches, report send
    failures; `info!` — actor start/stop, cycle start/end with op and
    entity counts, offline transitions; `debug!` — per-group push
    results, per-endpoint conditional outcomes (fresh/304); `trace!` —
    per-op detail (op kind, entity id). No user content above `trace`.
    Logging arms are not text-asserted in tests (AGENTS §5).
16. **Miri/mutants scope is unchanged** (weekly CI targets domain+state
    only) — but the pure domain additions (`push.rs`,
    `apply_push_report`) land inside the already-covered
    `taskboard-domain` and inherit the gates for free; the actor's async
    tests are `start_paused`-based and need no Miri coverage
    (testing strategy §10 scope note).

## 4. Domain Additions (normative)

### 4.1 `SyncStateReader` port (`persistence.rs`) + engine envelope

```rust
/// The sync actor's read-only view of the engine's persisted state —
/// outbox, bindings, validators included. ADR 0005: sync reads via the
/// engine, never direct SQL.
pub trait SyncStateReader: Send + Sync + std::fmt::Debug {
    fn read_state(&self)
        -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;
}
```

`taskboard-state` adds `EngineCommand::ReadState { reply: oneshot::
Sender<Result<PersistedState, RepositoryError>> }`; the loop arm replies
`Ok(core.persisted_view())`; a closed inbox maps to
`RepositoryError::Unavailable` in the `impl SyncStateReader for
EngineHandle` (mirroring `execute`'s `EngineGone` semantics, expressed in
the port's error type). **Divergence note (merged phase 3, §0)**:
`persisted_view()` carries the memory-only `Syncing` transient that no
repository `load()` can ever return, so a `ReadState` reply may advertise
`phase: Syncing` that was never persisted. Accepted deliberately, under
one pin: **`SyncStateReader` consumers must never branch on
`sync.phase`** — the phase is engine/UI territory, and the actor's cycle
(§5.2) reads only the outbox, bindings, and validators (an A-series test
asserts identical actor behavior for both phase spellings of an equal
state). `impl SyncStateReader for InMemoryRepository` returns
`Ok(self.snapshot())` — with the contract harness extended to
assert both impls return the post-apply persisted shape (cheap, one
routing test per impl; semantics live in `apply_actions`, already
tested).

### 4.2 `SyncReport` reshaped (breaking change inside the workspace)

```rust
pub enum SyncReport {
    Completed {
        snapshot: RemoteBoardSnapshot,
        validators: BoardPullValidators,
        pushes: Vec<PushOutcome>,
    },
    Failed {
        kind: SyncErrorKind,
        /// Successful push outcomes from the aborted cycle (decision 3).
        pushes: Vec<PushOutcome>,
    },
}

/// Conditional-read validators for one board pull (decision 4); the
/// engine maps these onto `ValidatorKey`s against its own binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardPullValidators {
    pub boards: SyncValidators,
    pub stacks: SyncValidators,
    pub archived_stacks: SyncValidators,
}
```

`SyncErrorKind` gains `NoBoard` (deck-agnostic; storage codec tag
`no_board`, §4.4-adjacent codec table update for `sync_status.last_error`).
New pure fn in `pipeline.rs`:

```rust
/// The pipeline's push-only composition (steps 1 + 7): consumed by the
/// engine for `Failed { pushes: non-empty }` reports.
pub fn apply_push_report(
    current: &AppState, outbox: &[PendingOp], pushes: &[PushOutcome],
    ids: &dyn IdGenerator, now: DateTime<Utc>,
) -> (AppState, Vec<PersistenceAction>);
```

Behavior contract: identical to `apply_sync_report` with an empty
snapshot *except* that no presence reconciliation, cascade, or board
merge runs (an empty snapshot would tombstone everything — this fn
exists precisely to avoid that); phase transition: `Idle`/`Failed {
BadRequest }` by the same dead-letter rule, `last_success = Some(now)`
only if no rejection occurred, else unchanged. Its equivalence to the
pipeline's push phase is proptest-pinned (§7 P2).

### 4.3 `push.rs` — the coalescing planner (pure)

```rust
pub struct PushGroup {
    /// Which entity the group materializes against.
    pub target: PushTarget,                 // Task(TaskId) | Stack(StackId) | Label(LabelId)
    /// The surviving client intent for this cycle.
    pub materialized: MaterializedOp,
    /// Op ids that succeed/fail together with `materialized`
    /// (subsumed), in queue order.
    pub subsumed: Vec<OpId>,
    /// The materialized op's own id (an outcome is emitted for it).
    pub primary: OpId,
}

pub enum MaterializedOp {
    CreateTask { task: TaskId, stack: StackId, new_card: NewCardShape },
    UpdateTask { task: TaskId, card: CardShape },        // full-send PUT fields
    MoveTask { task: TaskId, to: (StackId, i64) },       // reorder call
    DeleteTask { task: TaskId },
    CreateStack { stack: StackId }, RenameStack { stack: StackId },
    DeleteStack { stack: StackId },
    CreateLabel { label: LabelId }, UpdateLabel { label: LabelId },
    DeleteLabel { label: LabelId },
    /// Unbound entity whose group contains its create and delete (or a
    /// delete of a never-pushed entity): nothing to send; every op in
    /// the group completes synthetically.
    Noop,
}
pub fn plan_pushes(outbox: &[PendingOp]) -> Vec<PushGroup>;
```

`NewCardShape`/`CardShape` are **domain-side mirrors** of the client's
`NewCard`/write fields (title, description, duedate, order, archived,
label ids as `BTreeSet<LabelId>`, done) so the planner stays in the
domain; the sync crate maps shapes → client param structs (the reverse
direction of the read mapping). Normative grouping table (ops in
`op_seq` order within an entity):

| Group contains | Materialized | Subsumed |
|---|---|---|
| any `Delete*` | `Delete*` (or `Noop` if the entity is unbound — the server never knew it) | every other op of the entity |
| `Create*` (+ any updates/moves) | `Create*` from **current** state; pending label ops on a created task ride as `assignLabel` calls post-create (their own outcomes) | updates/moves; label ops resolved in-overlay (unresolvable → `Rejected { LocalData }` for that label op only) |
| else (edits only) | one `UpdateTask` (full-send from current state; label-set changes ride the PUT) + ≤1 `MoveTask` **iff** final position ≠ pre-flight GET's position; stack/label edits → `Rename*`/`UpdateLabel` | all other edit ops of the entity |

The planner never sees remote bindings (pure over the outbox); the
*executor* decides `Noop` (unbound) and resolves dependencies via the
overlay. Cross-entity ordering: stacks → labels → tasks; within a kind,
group order follows first-op queue position. Proptest laws (§7): groups
partition the outbox (every op id appears exactly once — in `primary` or
`subsumed`); group order respects stacks<labels<tasks; a group's
materialized op is one of its member op kinds; deletes subsume
everything.

### 4.4 Storage codec: `ValidatorKey::ArchivedStacks(RemoteBoardId)`

One variant, serde tag `"archived_stacks:<u64>"`, codec unit tests
mirroring the existing two keys; `sync_metadata` needs **no migration**
(keys are free-form TEXT). The domain's `persisted_state_strategy`
already generates validators — extend it to the new key. The Phase 2
contract harness round-trips it via the shared codec test.

## 5. The Actor (`crates/taskboard-sync-nextcloud/src/actor.rs`)

### 5.1 Construction & loop

```rust
#[derive(Debug, Clone)]
pub struct SyncActorConfig {
    pub poll_interval: Duration,
    pub backoff_initial: Duration,
    pub backoff_max: Duration,
}

pub fn spawn_sync_actor(
    client: DeckClient,
    state: Arc<dyn SyncStateReader>,
    commands: mpsc::Receiver<SyncCommand>,
    reports: mpsc::Sender<SyncReport>,
    system: broadcast::Sender<SystemEvent>,
    cfg: SyncActorConfig,
) -> tokio::task::JoinHandle<()>;
```

Loop skeleton (single task; all awaits are cancellable at message
boundaries only — a cycle is never aborted mid-flight by a nudge, only
by `Shutdown` between cycles):

```
state: { target: Option<RemoteBoardId>, cache: PullCache,
         running, rerun_requested, failure_streak, offline }
select! {
    cmd = commands.recv() => match cmd {
        SetBoard(id) => { target = Some(id); cache.clear(); start_cycle() }
        SyncNow       => if running { rerun_requested = true } else { start_cycle() }
        None          => exit
    },
    _ = sleep_until(deadline) => start_cycle(),
    _ = system shutdown subscriber? => exit,        // see below
}
```

Shutdown source: the actor subscribes to the same `SystemEvent`
broadcast it publishes on (its own receiver; bootstrap-owned channel) —
`Shutdown` exits; `NetworkLost`/`NetworkRestored` from other actors are
ignored (this actor *is* the network authority). Channel-closed on the
broadcast receiver: `warn!` and continue on commands/deadline (mirrors
the engine's degradation).

### 5.2 Cycle: read

`state.read_state()` → on `Err(Unavailable)` (engine mid-restart) treat
as a transport-class failure (backoff, no report — the engine will
re-nudge or the poll tick retries); `Err(Corrupted)` → `error!`, park
the actor (retry on next tick; a corrupted store is not sync's to fix).
From the persisted state take: the outbox (`plan_pushes` input), the
board binding (the single live board with `remote == Some(target)` —
its *presence* gates push dependency resolution), existing bindings for
the overlay seed, and the persisted validators (cache *warmth* signal
only, decision 9).

### 5.3 Cycle: push

1. `plan_pushes(outbox)` → groups in §4.3 order. Empty outbox ⇒ skip to
   pull.
2. Execute groups in order. Per group:
   - Resolve binding via the overlay (seeded from read state, extended
     by create echoes). Unresolved dependency → `Rejected { LocalData }`
     outcome for `primary`, group's `subsumed` untouched.
   - Unbound-entity delete/create+delete pair → `Noop`: synthetic
     `Applied { echo: None }` for every member op (decision 6).
   - Card edits: pre-flight `GET card` (decision 5) → build the PUT from
     fetched fields overridden by current local state (labels resolved
     through the overlay; a locally-set label with no remote binding is
     omitted from this PUT — its op will land it later; a PUT whose
     label list changed only by unresolvable labels skips the PUT and
     defers the whole group to the next cycle with no outcomes).
     `reorder` iff the final `(stack, order)` differs from the GET's.
   - Send via the client; classify (decision 8 table) → outcomes.
   - On a transport-classified send failure: **stop pushing**; the
     accumulated outcomes become the `Failed` report's `pushes`; jump
     to reporting (no pull — the snapshot would predate the unfinished
     pushes anyway; R3 safety).
3. Transport-failure exit path and normal path converge at §5.4/5.5.

`DeckError → PushResult` classification table (pure fn `classify_push`):

| `DeckError` | `PushResult` |
|---|---|
| (2xx, body decoded) | `Applied { echo: Some(mapped) }` |
| (2xx, no usable body: `reorder`; `Envelope` on a write echo) | `Applied { echo: None }` |
| `NotFound` on an existing-entity op | `RemoteMissing` |
| `Forbidden` on a delete op | `RemoteMissing` (tier-2 verify) |
| `Unauthorized`/`Forbidden` (other) | `Rejected { Auth / Forbidden }` |
| `BadRequest` | `Rejected { BadRequest }` (dead-letters) |
| `Conflict`/`PreconditionFailed`/`Server(_)` | `Rejected { Server }` |
| `Transport`/`RateLimited`/`Unavailable` (retries exhausted) | abort cycle; no outcome |

### 5.4 Cycle: pull

Assemble the snapshot per decision 9/11:

1. `fetch_boards(&validators.boards)` → find `target` in the data.
   Absent → confirm-and-synthesize (decision 11).
2. `fetch_stacks(target, Active)` and `fetch_stacks(target, Archived)`,
   each conditional, `304` → fill from cache; cache miss → one
   unconditional refetch (warmth rule).
3. Map DTOs → `Remote*` views (§5.6); tasks = union of both listings
   (the archived listing's cards carry `archived = true` — if the live
   listing also lists a card, the live entry wins and the archived
   entry is deduped by card id).
4. Snapshot = board + merged stacks (both listings, deduped) + tasks +
   labels (from the board payload's inline labels). Validators captured
   from all three responses.

Any pull read failing after retries → `Failed { kind, pushes }` (kind
per the classification: `Transport`-class → `Network` + offline
transition; `Envelope` on a read → `Server`; `Unauthorized` → `Auth`;
etc.) — **no snapshot is ever reported from a partial pull**.

### 5.5 Cycle: report + engine-side ingestion

- Success: `reports.send(Completed { snapshot, validators, pushes })`
  (a closed reports channel is `warn!`-logged and the cycle's work is
  discarded — the next cycle re-reads; engine restarts are the expected
  cause).
- The engine's `Completed` arm additionally appends one
  `UpsertValidators` action per key derived from the report's
  `BoardPullValidators` **iff the stored value differs**; `Failed`
  arm: `pushes` non-empty → `apply_push_report` path (decision 3);
  empty → today's single-status-action path. `NoBoard` kind flows into
  `SyncStatus.phase = Failed { last_error: NoBoard }` (storage codec
  tag `no_board`).

### 5.6 DTO → domain mapping (pure, `mapping.rs`)

- `map_board(&DeckBoard) -> RemoteBoard` (i64 epoch-seconds
  `last_modified` → `DateTime<Utc>` via `timestamp_opt`; `deleted_at ==
  0` → `None`; color passed through as the raw wire string — the domain
  view is color-string based by design).
- `map_stack(&DeckStack, board: RemoteBoardId) -> RemoteStack`
  (ref synthesis).
- `map_card(&DeckCard, board: RemoteBoardId) -> RemoteTask`
  (`labels: Vec<CardLabel>` → `BTreeSet<RemoteLabelId>` via
  `CardLabel::id`; `stack_id` + board → ref context; `done`/`duedate`
  pass through; `archived` flag from `Card.archived`).
- `map_label(&DeckLabel, board) -> RemoteLabel`.
- Reverse direction for push shapes: `task → CardShape` (labels resolved
  via overlay), `task → NewCardShape`, `stack → StackChanges`,
  `label → LabelChanges` (all pure over domain types + a resolved-ref
  lookup closure).
- Property tests: round-trip each view's *owned fields* through the
  mapping with generated DTOs (ids are contextual, so laws assert field
  preservation, timestamps, and deleted_at semantics, not id equality);
`serde` round-trips already exist in the domain for the views.

## 6. Module Layout & TDD Implementation Order

```
crates/taskboard-domain/
├── src/push.rs          # NEW: plan_pushes, PushGroup/MaterializedOp, shapes
├── src/pipeline.rs      # + apply_push_report (reuses apply_push_outcomes)
├── src/persistence.rs   # + SyncStateReader; ValidatorKey::ArchivedStacks
├── src/messages.rs      # SyncReport reshape, SyncErrorKind::NoBoard
├── src/remote.rs        # + BoardPullValidators
└── src/test_support/    # + strategies for PushGroup, BoardPullValidators;
                         #   impl SyncStateReader for InMemoryRepository
crates/taskboard-state/
├── src/actor.rs         # + EngineCommand::ReadState + routing
└── src/port.rs-adjacent # impl SyncStateReader for EngineHandle (new port.rs or in actor.rs)
crates/taskboard-storage-sqlite/
└── src/codec.rs         # archived_stacks key tag + tests
crates/taskboard-sync-nextcloud/
├── Cargo.toml           # no new prod deps (tokio time/sync already on)
└── src/
    ├── actor.rs         # NEW: spawn_sync_actor, loop, cycle, offline FSM
    ├── push_exec.rs     # NEW: group executor (overlay, pre-flight, classify)
    ├── mapping.rs       # NEW: DTO ↔ domain views, classify_push
    ├── poll.rs          # NEW: poll_backoff pure fn + seam
    └── lib.rs           # re-exports
tests/ (sync crate)
├── actor_wiremock.rs    # A-series (paused time + wiremock)
├── e2e_hermetic.rs      # I-series: engine + actor + InMemoryRepository
└── it_docker.rs         # + D-series two-way sync scenarios
```

Branch `feat/sync-nextcloud-actor` (this plan's branch, stacked on
`feat/state-engine-actor`), one PR for the plan. TDD order:

1. Domain `ValidatorKey::ArchivedStacks` + codec tag + strategy
   extension (smallest ripple first; storage harness re-run green).
2. Domain `push.rs` planner: laws first (P-series), then the table in
   §4.3 case-by-case test-first.
3. Domain `apply_push_report` + P2 equivalence proptest.
4. `SyncReport` reshape + `NoBoard` + `BoardPullValidators`; fix all
   compile sites (engine ingestion) with the §5.5 semantics; phase 3
   suite re-run green.
5. `SyncStateReader` port + `EngineCommand::ReadState` + both impls +
   routing tests.
6. Sync crate `mapping.rs` (U + P series; Tier 0 fixtures extended with
   archived-listing + NC35 bare-JSON samples).
7. `push_exec.rs` against a wiremock-recorded client (pure-ish unit
   layer: classification, overlay, pre-flight).
8. `actor.rs` + `poll.rs`; A-series wiremock actor suite (paused time).
9. I-series hermetic E2E (engine + actor + in-memory fake).
10. D-series docker scenarios (§7) + docs (§9) + full quality gates.

## 7. Test Plan (catalogue)

Deterministic by construction: `#[tokio::test(start_paused)]` for every
awaiting-timer test (`pause`/`advance`; no sleeps), `RecordingSleeper`
for retry timing, oneshot/JoinHandle/`try_recv` completion signals,
typed outcomes only, wiremock per-mock verifications for request shapes.
The engine half of E2E tests uses `InMemoryRepository` + `CountingIds` +
a fixed/step clock from the domain's `test-support`; the actor half gets
`Arc<InMemoryRepository>` as its `SyncStateReader` — the *same* fake, so
memory ≡ disk invariants hold across the pair.

- **U-series (pure, inline)**: mapping table cases per §5.6 (epoch 0,
  deleted_at 0, label id-or-object, NC35 bare JSON fixture); `classify_push`
  every row of §5.3's table; `poll_backoff` (doubling, cap, reset);
  planner §4.3 rows incl. Noop/unbound, create+label-rides,
  multi-edit collapse, move-only, delete-subsumes; `apply_push_report`
  vs pipeline push phase.
- **P-series (proptest, domain)**: P1 planner partition/order laws (§4.3);
  P2 `apply_push_report(current, outbox, pushes) ≡ apply_sync_report`
  restricted to the push phase (same action prefix modulo status
  derivation, on `persisted_state_strategy` + `push_outcome_strategy`);
  P3 mapping field-preservation laws over generated DTOs; P4
  `BoardPullValidators` serde round-trip (extends existing remote-view
  round-trips).
- **A-series (wiremock actor, one behavior each)**: A1 nudge → cycle →
  `Completed` report with a full snapshot (two-way: local create pushed
  then pulled back bound); A2 poll-tick cycle with unchanged server →
  304s → `Completed` with empty diffs and **no** entity upserts (the
  engine's publish-iff-changed keeps the signal count at zero — asserted
  via the engine in I-series instead); A3 cycle-in-flight `SyncNow`
  dedupe (exactly one rerun after completion — asserted by wiremock
  request counts under advanced time); A4 push failure mid-outbox
  (transport after two successes) → `Failed { Network, pushes: [2] }`,
  outbox retains the rest, no snapshot sent; A5 `NotFound` on update →
  `RemoteMissing` outcome → engine tombstones (I-series asserts the
  visible state); A6 `BadRequest` → dead-letter surfaces as
  `Failed { BadRequest }` phase via the pipeline; A7 offline transition:
  transport failure → `NetworkLost` broadcast exactly once, backoff
  doubling observed via `advance`, success → `Completed` then
  `NetworkRestored`; A8 `SetBoard` before any binding → cycle reports
  `Failed { NoBoard }` and the engine's `Syncing` clears; A9 archived
  listing merge (card archived remotely → pulled as
  `archived = true`, **not** tombstoned); A10 304-with-cold-cache forces
  unconditional refetch (request-count assertion); A11 board absent
  from listing → confirm-GET → synthesized tombstone snapshot → engine
  cascade (I-series asserts tasks tombstoned); A12 `Shutdown` exits the
  loop (JoinHandle, deadline guard); A13 dependency ordering: create
  stack + create task in its stack → stack POST precedes card POST
  (wiremock expectation ordering).
- **I-series (hermetic E2E, engine + actor in one test, in-memory
  fake)**: I1 the roadmap's offline journey — create task offline
  (engine only), nudge absorbed, actor started later → push + pull
  round-trip, receipt of the remote binding in `AppState`; I2 remote
  edit wins per LWW (server-modified card vs stale local edit);
  I3 concurrent edit → policy outcome (local newer field wins; remote
  newer field adopts) — the Phase 1 §7.4 worked examples as executable
  scenarios; I4 full two-way including move + label assign + delete;
  I5 restart continuity: drop the actor, respawn with a fresh client —
  persisted validators resume conditional polling (304s observed).
- **D-series (docker tier 2, `it_nextcloud_docker_*`, reusing
  `tests/common`)**: D1 the exit demo — login-equivalent (client from
  tier env), create task via the engine+actor pair, verify the card on
  the server via a raw `DeckClient` read, and the reverse: create a
  card server-side via `DeckClient`, cycle, assert it in `AppState`;
  D2 cross-stack reorder semantics re-verified through the executor
  (path-vs-body fact pinned at the actor level); D3 archived listing
  behavior on the real server (archive a card via client, cycle,
  assert `archived = true` locally; unarchive → resurrects in place);
  D4 deleted-board listing behavior (delete a board via client, cycle,
  assert cascade) — feeds decision 11's fallback; D5 label-change
  visibility through the board payload (guard against the NC35 `labels()`
  405 path); D6 `update_card` round-trip honoring `done`/`duedate`/
  `archived` field writes (feeds §5.3's PUT construction). All D-series
  under the run-id isolation + teardown discipline of the existing
  harness; **skip, never fail** without the tier env.
- **Doc-tests** on `spawn_sync_actor` and `plan_pushes` usage.

Coverage: llvm-cov review target = mapping + planner + executor + loop;
the weekly mutants/miri pair picks up the new *domain* modules for
free (decision 16). Surviving mutants → `.artifacts/reports/`.

## 8. Out of Scope → Backlog (append with this plan)

- **Duplicate-create crash window**: a crash *after* a create POST but
  *before* the outcome reaches the engine re-POSTs on the next cycle
  (no idempotency keys exist). Mitigation today: the window is one
  oneshot hop wide; a future mitigation could reconcile duplicate
  titles+timestamps at pull time. Trigger: observed duplicate.
- **`Retry-After` handling** in the poll backoff (client plan §9
  leftover): parse the header into `poll_backoff`'s floor.
- **Filtered deleted-boards fallback**: if Deck's `GET /boards` is
  confirmed to hide soft-deleted boards in a way that defeats decision
  11's confirm-GET (e.g. rate-limited confirmations), add an
  unfiltered/deleted-boards listing if the API grows one.
- **Listing→detail field-parity cache**: if tier 2 shows stack-listing
  cards missing fields the snapshot needs, introduce per-card
  conditional detail fetches for *changed* cards only.
- **Incremental pull** (per-stack validators / per-card ETags): the
  domain's `ValidatorKey` is per-board-listing today; per-card
  conditional pulls are the known scaling path post-MVP.
- **Push pipeline parallelism** (independent entity groups in flight
  concurrently): serial groups are correct and simple; parallelize only
  on measured need.
- **Sync-actor metrics** (cycle duration, push/pull counts, retry
  depth) for the kiosk dashboard — with the observability backlog entry.

## 9. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0007 — "The Sync Actor: push-first cycle, evidence-then-verdict
  reporting"**: cycle order and the R3 rationale; `SyncReport` shape
  (validators on `Completed`, pushes on `Failed`) and the engine-side
  ingestion rules; `SyncStateReader` as the second port and the
  read-via-engine rule; coalescing as a pure domain planner; offline
  FSM (NetworkLost/Restored ownership, backoff, dedupe); `SetBoard`
  consumption and the binding-by-adoption model. Status: Accepted.
- **architecture.org**: refresh the `taskboard-sync-nextcloud` bullet
  (actor cycle, push executor, pull assembler, ports consumed) and fix
  its stale dependency list to the manifest's actual set; add the
  actor's rows to the channel table (SyncNow consumption, SyncReport
  production, SystemEvent production, SyncStateReader port).
- **CHANGELOG**: `[Unreleased]` → Added: sync actor + domain push
  planner + report reshaping + read port; Changed: `SyncReport` shape
  (workspace-internal breaking change).
- **Backlog**: append §8 entries via the file's template.

## 10. Verification & Acceptance

- Full quality gate (AGENTS §10): fmt; clippy `-D warnings` (with
  `SQLX_OFFLINE=true`); `cargo nextest run --workspace` (hermetic tiers
  only, Docker stopped, no `.env`); doc tests; `cargo deny check`;
  `cargo llvm-cov --workspace`.
- `cargo nextest run -p taskboard-sync-nextcloud` green: U/P/A/I series;
  `-p taskboard-domain --all-features`: planner, `apply_push_report`,
  port + strategies; `-p taskboard-state`: `ReadState` routing +
  reshaped ingestion (phase 3 suite re-run); `-p
  taskboard-storage-sqlite`: codec addition + contract harness re-run.
- Tier 2: `eval "$(scripts/nextcloud_it_setup.sh up)" && cargo nextest
  run -p taskboard-sync-nextcloud --run-ignored only -E 'test(
  it_nextcloud_docker)'` green including D1–D6 locally; in CI, trigger
  `nextcloud-integration.yml` via `workflow_dispatch` on the PR branch
  (it also runs weekly thereafter) — per-PR CI covers the contract
  suite only, by design (§1).
- **Phase 4 exit (roadmap)**: two-way sync demonstrated against
  dockerized Nextcloud — D1 is the demo; D2–D6 retire the flagged
  tier-2 verifications (reorder semantics, archived listing, deleted
  boards, label visibility, card PUT field writes).
- Tier 3 (manual, user-run): the same two-way journey against the live
  server per the existing runbook; record the known-server matrix
  additions in `testing_strategy.org`.

## 11. Consumer Handoff Map (what later phases import)

| Consumer | Takes from Phase 4 |
|---|---|
| Phase 5 app bootstrap | `spawn_sync_actor` wiring (client, `Arc<dyn SyncStateReader>` from `EngineHandle` clone, channel halves, `SyncActorConfig` from figment `[sync]`), the system broadcast ownership (bootstrap creates it; engine subscribes, actor publishes + subscribes for Shutdown), shutdown ordering (stop the actor before the engine; engine `flush()` before exit) |
| Phase 6 CLI | `SyncCommand::SetBoard` for `boards select`; the `sync` command's status watch (predicate poll on `shared_state()`); login stores the app password the client consumes |
| Phase 7 UI | nothing directly — sync state reaches it through `AppState.sync` (phase, `last_success`, `pending_ops`) via the ArcSwap; the offline indicator is `SyncPhase::Offline` |
| Everyone | the push-first/evidence-then-verdict reporting model (ADR 0007) — any future sync transport (CalDAV, etc.) reuses the cycle skeleton, planner, and report shapes |

## 12. Risks & Mitigations

- **Partial-snapshot tombstoning** (the R3 hazard): structurally
  excluded — push-before-pull, all-or-nothing pull (a failed read means
  no snapshot at all), cache-fill-or-fail, D/I/A tests assert the
  negative (no tombstone without a completed three-read pull). The
  single remaining gap — a server that *lies* by omitting live
  resources from a 200 listing — is outside the completeness contract's
  reach and covered only by decision 11's confirm-GET for the board
  case; stack/label listings lying is accepted (Deck's own consistency
  domain), noted in ADR 0007.
- **Duplicate creates** (no idempotency keys): minimized by
  evidence-then-verdict (decision 3) and abort-on-transport (decision
  8); the residual crash window is backlogged (§8). The retry policy's
  3 attempts per call is the other exposure — `Envelope`-on-write
  classified as success (decision 8) avoids the worst re-POST class.
- **Stale-binding 404s misread as deletes**: fetch-before-write
  (decision 5) plus `Forbidden`-on-delete classification (decision 8,
  tier-2-verified in D2) keep `RemoteMissing` conservative.
- **`update_card` clobbering concurrent remote changes**: the
  pre-flight GET narrows the window to sub-second; Deck offers no
  `If-Match` to close it — per-field LWW (R5) repairs the field-level
  damage on the next pull, and the granularity limitation is a recorded
  roadmap-level accepted trade-off (ADR 0004, backlog entry exists).
- **Nudge storms / cycle cost**: dedupe flag + coalesced planner bound
  the work per cycle; poll backoff bounds the failure path; measured
  need owns parallelism (backlog).
- **`SyncReport` reshaping ripples**: the change is workspace-internal,
  compile-time enforced, and lands with the ingestion semantics in the
  same PR (TDD step 4); the phase 3 suite is the regression net.
- **Engine `ReadState` clone cost** (full `PersistedState` per cycle):
  O(entities) clone per cycle at kiosk scale is microseconds-to-millis;
  backlogged narrower read envelopes if measurements disagree.
- **Docker-tier flakiness** (Deck cache lag vs assertions): D-series
  assertions use the same deadline-poll discipline as the existing
  suite; no fixed sleeps; tier-2-only behaviors (D4's listing behavior)
  are asserted as *observed-and-pinned* facts with the fallback path
  backlogged.
