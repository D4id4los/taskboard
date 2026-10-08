# [Plan] Growth Roadmap Phase 1 — Domain Model, Contracts & Conflict Policy

- **Date**: 2026-10-08
- **Target**: `taskboard-domain` (whole crate; no other crate gains code)
- **Goal**: implement the roadmap's Phase 1 — canonical entities with
  offline-first identity, the real `AppState`, inter-actor message enums,
  the `TaskRepository` Port (+ clock/id seams), the outbox operation type,
  and the pure merge/conflict-resolution policy with heavy property tests.
- **Status**: ACCEPTED (planning session with user; decisions §3 and
  conflict-policy choices confirmed 2026-10-08)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 1. Exit criterion: "entities + ports + policy functions,
  proptests green."
- **Predecessor surface**: `.artifacts/plans/2026-10-08-sync-nextcloud-deck-client-surface-plan.md`
  (Deck client, merged — its wire model constrains §7).

---

## 1. Context & Known Facts (do not re-derive)

Current state of the crate: a single `lib.rs` holding only the `AppState`
stub (`last_updated: Option<DateTime<Utc>>`, derives
`Debug, Clone, Default, Serialize`) plus doc comments whose intra-doc links
(`[`StateCommand`]`, `[`TaskRepository`]`) point at types that do not exist
yet — this plan creates them. Manifest already pins exactly `serde`,
`chrono`, `uuid` (+ dev-dep `proptest`); there are no tests in the crate.

Hard constraints (normative, from AGENTS.md / architecture.org / ADRs):

- **Purity**: `taskboard-domain` allows zero network, UI, and async-runtime
  dependencies; architecture.org caps its deps at "minimal (`serde`,
  `chrono`, `uuid`)". Consequences:
  - **No `tokio`, no `async-trait`.** Async ports are authored exactly like
    the sync crate's `RetrySleep` seam: object-safe traits returning
    `Pin<Box<dyn Future + Send>>` via a hand-rolled `BoxFuture` alias
    (`crates/taskboard-sync-nextcloud/src/client.rs` is the precedent).
  - **Message enums cannot name `tokio::sync::oneshot::Sender`.** See the
    payload/envelope split (decision D8).
- **Lints**: workspace `clippy::all` + `clippy::pedantic` at `warn`,
  promoted to `-D warnings` in CI. House style (sync crate): `# Errors`
  doc section on every fallible public fn, `#[must_use]` on pure getters,
  SPDX `MIT OR Apache-2.0` header, `#![forbid(unsafe_code)]`, documented
  public items, flat re-exports from `lib.rs`.
- **Weekly gates target this crate**: `cargo miri test -p taskboard-domain`
  and `cargo mutants -p taskboard-domain`. Policy functions must be total,
  deterministic, and their tests must kill boundary mutants (e.g. the
  tie-break tests exist precisely to kill `>` vs `>=` mutants).
- **Deck facts that shape the policy** (established by the client-surface
  plan): no write-side `If-Match`; `lastModified`/`deletedAt` are
  epoch-second integers (**entity-level** granularity, seconds resolution);
  cards have **no** `deletedAt` (deletion = absence from listings; DELETE
  responses are authoritative); boards/stacks are soft-deleted with a real
  `deletedAt` stamp; card PUT is full round-trip; `reorder` returns no
  usable echo and Deck's cache can lag on cross-stack moves; sparse PUTs
  reset omitted fields to server defaults.
- **Identity fact** (architecture.org): local ids are UUIDv7 generated
  client-side before remote sync. Remote Deck ids (`u64`) are only unique
  per board for stacks/cards/labels (backlog multi-board entry).

## 2. Scope

**In scope** — the crate grows from stub to contracts:

| Area | Deliverable |
|---|---|
| Entities | `Board`, `Stack`, `Task`, `Label` + id newtypes + per-field clocks + remote refs |
| State | Real `AppState` (task tree + `SyncStatus` + `last_updated`), `SyncPhase`, `SyncErrorKind` |
| Messages | `StateCommand`, `SystemEvent`, `EngineSignal`, `SyncCommand`, `SyncReport` + query payloads |
| Ports | `TaskRepository`, `Clock`, `IdGenerator`, `BoxFuture`, `RepositoryError` |
| Outbox type | `LocalOp`, `PendingOp`, `OpId` (persisted by Phase 2, consumed by Phase 4) |
| Policy | Pure merge/conflict functions + the `apply_sync_report` pipeline |
| Tests | In-module unit + proptest suite; `test-support` feature (strategies, `InMemoryRepository`, repository contract harness) |
| Docs | ADR 0004 (conflict policy), architecture.org entity/message refresh, CHANGELOG, backlog appends |

**Out of scope** (owned by later phases): any engine/actor logic (Phase 3),
SQL schema/migrations/tombstone retention (Phase 2), DTO↔domain mapping
(Phase 4 — mapping functions live in the sync crate), board-local-edit
commands (MVP binds an existing remote board), reminders, multi-board.

## 3. Design Decisions

1. **Per-field last-writer-wins via per-field write clocks.** Each editable
   field of an entity carries a `DateTime<Utc>` write time (`TaskClocks`
   etc.). Local edits stamp `Clock::now()` on touched fields only; adopting
   a remote version stamps all clocks with the remote `lastModified`.
   Merge rule per field: **remote wins iff
   `remote.last_modified > field_clock` (strictly); ties keep local.**
   This is the only way to honor the roadmap's "last-writer-wins per field"
   given that Deck timestamps whole entities: the local side is per-field,
   the remote side is entity-granular. Documented limitation (§7.4): a
   remote entity-touch newer than a local field edit reverts that field
   even if the remote change touched a different field.
   *Alternative rejected*: single entity-level mtime (whole-entity winner
   drops the other side's entire edit — materially worse for a task app).
2. **Ties keep local, remote must be strictly newer.** Deterministic
   tie-break; Deck's second-resolution timestamps then naturally lose to
   sub-second local edits made in the same second. This exact boundary is
   what the tie-break property tests pin (and what kills `>=` mutants).
3. **Deletion is a timestamped fact where Deck provides one; cards fall
   back to delete-wins.** (User-confirmed.) Boards/stacks carry a real
   remote `deletedAt` → pure LWW against the local tombstone clock. Cards
   have no remote delete timestamp → an observed remote absence **always**
   beats concurrent local edits (tombstone). Rationale: absence has no
   timestamp to compare against, and automatic resurrection would create
   zombie tasks that reappear after every sync.
4. **Labels merge as one field (whole-set LWW).** (User-confirmed.) The
   label set is a single field; the newer write replaces the set. Add-wins
   OR-set merging goes to the backlog (§12).
5. **Position is one composite field.** `stack` + `order` share a single
   clock; a move is one user intent, so concurrent move+reorder resolve as
   a unit (never stack-from-A-order-from-B chimeras). Concurrent
   interleaving of two devices' orderings is a documented limitation.
6. **The engine executes the merge.** The sync actor never computes merged
   state; it fetches, pushes, maps DTOs, and reports raw observations plus
   push outcomes (`SyncReport`). The state engine runs the pure policy
   against its authoritative current state, so user edits landing during a
   sync run cannot be lost to a stale-state race. This also keeps the
   policy 100% inside `taskboard-domain` (roadmap §2.1).
7. **Tombstones live on entities.** `deleted: bool` + the `deleted` clock;
   a tombstoned task keeps its remote ref cleared and stays in the maps
   (filtered out of live views by pure helpers). Retention/purge is Phase 2
   storage design (roadmap §5).
8. **Payload/envelope split for oneshot queries.** Domain owns all *pure*
   payloads; the channel-bearing envelopes (e.g. a `Flush { reply:
   oneshot::Sender<()> }` persistence barrier for CLI exit) are declared by
   the actor crates that own the channels (Phase 3 `taskboard-state`,
   Phase 5 `taskboard-app`), which do depend on tokio. This honors the
   purity cap without generifying every enum over a reply-sender type
   (`StateCommand<R>` infects all consumers).
9. **One-method batched write side on the port.** `TaskRepository::apply(Vec<PersistenceAction>)`
   instead of per-entity methods: the engine already computes actions, the
   batch is the transaction unit (entity+outbox pairs land atomically in
   Phase 2), and the contract-test surface stays small and stable.
10. **`test-support` cargo feature.** (User-confirmed.) Phase 2/3 tests
    need the entity strategies, the in-memory fake, and the contract
    harness; dev-dependencies cannot be shared across crates, so these
    ship as a non-default feature. `proptest` moves from dev-dep to
    optional dep (already workspace-pinned — no new external deps).
11. **Deterministic by construction**: public domain types use
    `BTreeMap`/`BTreeSet` exclusively (no `HashMap` in public shapes), so
    serialization, snapshot output, and merge results are deterministic
    for equal inputs — required by the convergence property and by insta
    snapshots in Phase 3. Lookup-only indexes may use `HashMap` privately.
12. **`Color` is a minimal lenient newtype** over `String` (construction
    passes anything through; `Display` round-trips). Validation stays in
    the sync adapter (`DeckColor`), per the client plan's YAGNI note.

## 4. Entities (`ids.rs`, `entities.rs`)

### 4.1 Identifiers

```rust
// Local identity: UUIDv7 (time-ordered, collision-resistant offline-first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BoardId(Uuid);   // + StackId, TaskId, LabelId same shape
// Construction via From<Uuid> + as_uuid() getter; generation via the
// IdGenerator seam (§6.3) so tests stay deterministic.

// Remote identity: Deck u64 ids, typed so board/stack/card numbers cannot be swapped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RemoteBoardId(pub u64);     // + RemoteStackId, RemoteCardId, RemoteLabelId

// Composite remote refs (remote ids are only unique per board):
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCardRef  { pub board: RemoteBoardId, pub stack: RemoteStackId, pub card: RemoteCardId }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteStackRef { pub board: RemoteBoardId, pub stack: RemoteStackId }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteLabelRef { pub board: RemoteBoardId, pub label: RemoteLabelId }
```

### 4.2 Task (the central entity)

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: TaskId,
    /// Remote binding; `None` until the pending create is pushed (R2).
    pub remote: Option<RemoteCardRef>,
    pub title: String,
    pub description: String,
    pub duedate: Option<DateTime<Utc>>,
    /// Completion timestamp (`None` = open). Set via `SetTaskDone`.
    pub done: Option<DateTime<Utc>>,
    /// Position = stack + order, ONE mergeable field (decision 5).
    pub stack: StackId,
    pub order: i64,
    pub labels: BTreeSet<LabelId>,
    pub archived: bool,
    /// Tombstone flag; the `clocks.deleted` timestamp is the tombstone's
    /// write time (decision 7).
    pub deleted: bool,
    pub clocks: TaskClocks,
    /// Remote `lastModified` at last adoption; `None` = never synced.
    /// Fast-path guard for R4 and the R3 baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// Per-field write timestamps (decision 1). Local edits touch only the
/// edited fields; remote adoption stamps everything with the remote ts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskClocks {
    pub title: DateTime<Utc>,
    pub description: DateTime<Utc>,
    pub duedate: DateTime<Utc>,
    pub done: DateTime<Utc>,
    pub position: DateTime<Utc>,
    pub labels: DateTime<Utc>,
    pub archived: DateTime<Utc>,
    pub deleted: DateTime<Utc>,
}
```

### 4.3 Stack, Board, Label

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    pub id: StackId,
    pub remote: Option<RemoteStackRef>,
    pub board: BoardId,
    pub title: String,
    pub order: i64,
    pub archived: bool,
    pub deleted: bool,
    pub clocks: StackClocks,          // { title, order, deleted }
    pub remote_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Board {
    pub id: BoardId,
    pub remote: Option<RemoteBoardId>,
    pub title: String,
    pub color: Color,
    pub archived: bool,
    pub deleted: bool,
    // No per-field clocks MVP: no local board-edit commands exist; board
    // edits arrive only via remote adoption (uniform clocks arrive with
    // the first local board command, multi-board era).
    pub remote_seen: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub id: LabelId,
    pub remote: Option<RemoteLabelRef>,
    pub board: BoardId,
    pub title: String,
    pub color: Color,
    pub deleted: bool,
    pub clocks: LabelClocks,          // { title, color, deleted }
    pub remote_seen: Option<DateTime<Utc>>,
}

/// Lenient color newtype (decision 12). Validation lives in the adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(String);
```

Pure helpers on entities (all `#[must_use]`): `Task::is_live()` (not
deleted), `Task::is_open()`, plus `state.rs` tree helpers (§5).

## 5. AppState & Sync Status (`state.rs`)

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AppState {
    /// MVP holds 0..1 entries; the map shape keeps multi-board additive
    /// (backlog warning: never bake "one board" into AppState).
    pub boards: BTreeMap<BoardId, Board>,
    pub stacks: BTreeMap<StackId, Stack>,
    pub tasks: BTreeMap<TaskId, Task>,
    pub labels: BTreeMap<LabelId, Label>,
    pub sync: SyncStatus,
    /// Kept from the stub; now = time of the last engine mutation from
    /// ANY source (local command or sync ingestion), stamped via `Clock`.
    pub last_updated: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncStatus {
    pub phase: SyncPhase,
    pub last_success: Option<DateTime<Utc>>,
    /// Outbox depth for the UI badge / CLI status.
    pub pending_ops: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncPhase {
    Idle,
    Syncing,
    Offline,
    Failed { last_error: SyncErrorKind },
}

/// Deck-agnostic sync failure classification; the sync actor maps
/// `DeckError` into this (Phase 4). No text payloads (AGENTS §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncErrorKind {
    Network,     // DeckError::Transport / RateLimited / Unavailable
    Auth,        // Unauthorized
    Forbidden,   // Forbidden
    Server,      // Server(u16) / Conflict / PreconditionFailed
    BadRequest,  // BadRequest (our write rejected)
    LocalData,   // e.g. repository unavailable during sync
}
```

Derived view helpers (pure, `#[must_use]`, in `state.rs`):
`live_tasks(&self)`, `tasks_in_stack(stack)`, `sorted_tasks(...)`
(order asc, tie-break id asc — the single canonical ordering every UI and
test uses).

## 6. Ports & Seams (`clock.rs`, `persistence.rs`)

### 6.1 Async-port shape (the `RetrySleep` pattern, no deps)

```rust
/// Hand-rolled: domain may not depend on futures/tokio/async-trait.
pub type BoxFuture<'a, T> =
    std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub trait TaskRepository: Send + Sync + std::fmt::Debug {
    /// Full hydration for boot. Empty/default state on a fresh database.
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;
    /// Apply a batch atomically (entity upserts + outbox transitions land
    /// or fail together — Phase 2 wraps this in one transaction).
    fn apply(&self, actions: Vec<PersistenceAction>)
        -> BoxFuture<'_, Result<(), RepositoryError>>;
}
```

```rust
#[derive(Debug, Error)]
pub enum RepositoryError {
    /// Backend unreachable/locked/busy — transient, caller may retry.
    #[error("repository unavailable")]
    Unavailable,
    /// Persisted data failed integrity/decoding — not retryable.
    #[error("repository data corrupted")]
    Corrupted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedState {
    pub boards: BTreeMap<BoardId, Board>,
    pub stacks: BTreeMap<StackId, Stack>,
    pub tasks: BTreeMap<TaskId, Task>,
    pub labels: BTreeMap<LabelId, Label>,
    pub outbox: Vec<PendingOp>,
    /// Opaque conditional-read validators (mirror of the sync crate's
    /// `Validators`; persisted so polls survive restarts).
    pub validators: BTreeMap<ValidatorKey, SyncValidators>,
    pub sync: SyncStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ValidatorKey { Boards, Stacks(RemoteBoardId) }

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncValidators {
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PersistenceAction {
    UpsertBoard(Board),
    UpsertStack(Stack),
    UpsertTask(Task),
    UpsertLabel(Label),
    EnqueueOp(PendingOp),
    CompleteOp(OpId),
    FailOp(OpId),
    UpsertValidators(ValidatorKey, SyncValidators),
}
```

### 6.2 Clock seam

```rust
pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
}
/// Production: `Utc::now()` via chrono's `clock` feature (already on).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;
```

### 6.3 Id-generator seam

```rust
pub trait IdGenerator: Send + Sync + std::fmt::Debug {
    fn new_board_id(&self) -> BoardId;
    fn new_stack_id(&self) -> StackId;
    fn new_task_id(&self) -> TaskId;
    fn new_label_id(&self) -> LabelId;
    fn new_op_id(&self) -> OpId;
}
/// Production: `Uuid::now_v7()` per id kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7Generator;
```

Injection everywhere follows the sync crate: `Arc<dyn Trait>` fields,
`with_*` consuming builders on the future actors (Phase 3), test fakes
recording/returning fixed values. **Merge functions never read the clock
or generate ids — time and ids are always data passed in** (Miri/mutants
determinism).

## 7. Conflict-Policy Semantics (`remote.rs`, `merge.rs`) — normative

### 7.1 Remote observation views (`remote.rs`)

Domain-owned, Deck-agnostic views of what the server returned; the sync
crate's DTO→view mapping lands in Phase 4 and is proptest-ed there.

```rust
pub struct RemoteBoardSnapshot {
    pub board: RemoteBoard,
    pub stacks: Vec<RemoteStack>,
    pub tasks: Vec<RemoteTask>,   // flattened from all stacks
    pub labels: Vec<RemoteLabel>,
}
pub struct RemoteTask {
    pub id: RemoteCardRef,
    pub title: String,
    pub description: String,
    pub duedate: Option<DateTime<Utc>>,
    pub done: Option<DateTime<Utc>>,
    pub stack: RemoteStackId,
    pub order: i64,
    pub labels: BTreeSet<RemoteLabelId>,
    pub archived: bool,
    /// Entity-level seconds-resolution timestamp — the ONLY remote
    /// versioning input (may be epoch 0 on old servers → treat as the
    /// minimum `DateTime`, i.e. loses to everything).
    pub last_modified: DateTime<Utc>,
}
pub struct RemoteBoard { /* id: RemoteBoardId, title, color, archived,
                            deleted_at: Option<DateTime<Utc>>, last_modified */ }
pub struct RemoteStack { /* id: RemoteStackRef, title, order, archived,
                            deleted_at: Option<DateTime<Utc>>, last_modified */ }
pub struct RemoteLabel { /* id: RemoteLabelRef, title, color,
                            deleted_at: Option<DateTime<Utc>>, last_modified */ }
```

**Snapshot completeness contract** (binding on the Phase 4 actor): the
`RemoteBoardSnapshot` in `SyncReport::Completed` is the actor's complete
view of the board at sync time — a resource absent from it is absent on
the server. The actor satisfies this by fetching the full stacks list
(validatorsgate it: unchanged stacks may be filled from its cached copy)
whenever anything reports changed. Absence detection (R3) is only as
correct as this contract.

### 7.2 Pull rules

Let `ts_r` = `remote.last_modified`. Per field `f` (including `deleted`
and the composite `position`; labels are one field — decision 4):

> **remote value wins ⟺ `ts_r > clocks.f`; ties and older keep local.**

- **R1 — new remote task** (no local task with this `RemoteCardRef`):
  adopt. New local `TaskId`, all clocks = `ts_r`, `remote_seen = ts_r`.
  Same for new stacks/labels. (Two devices creating "the same" task
  produce two tasks — accepted, documented.)
- **R2 — local-only, never pushed** (`remote: None`, absent remotely):
  unchanged. The pending `CreateTask` op stays queued; nothing to merge.
- **R3 — remote absence of a bound task** (`remote: Some`, not in
  snapshot): **card delete-wins fallback** (decision 3). Tombstone
  (`deleted = true`, `clocks.deleted = now` — the observation time,
  passed in), clear `remote`, `remote_seen = None`; cancel the task's
  pending ops (`UpdateTask`/`MoveTask`/label ops → dropped; a pending
  `DeleteTask` completes trivially). Concurrent local edits are lost —
  documented, backlogged alternative: resurrect-as-new-card (§12).
- **R4 — bound, remote unchanged** (`ts_r <= remote_seen`): keep local
  entirely (fast path; local edits ride on top and stay queued).
- **R5 — bound, remote changed** (`ts_r > remote_seen`): per-field LWW
  with the rule above; adopted fields get `clocks.f = ts_r`; then
  `remote_seen = ts_r`. The `deleted` field participates like any other:
  - local tombstone with `clocks.deleted < ts_r` → **resurrect**: adopt
    remote content, rebind `remote`, `deleted = false`, drop the pending
    `DeleteTask` op (the remote edit was newer);
  - local tombstone with `clocks.deleted >= ts_r` → stays tombstoned; the
    pending delete re-pushes and overwrites the remote edit.
- **R6 — boards/stacks/labels with real `deleted_at`**: the deletion is a
  timestamped fact — LWW it against `clocks.deleted` exactly like R5 (no
  fallback needed). A *final* stack tombstone cascades R3 to the stack's
  tasks; cards moved out before the delete survive via presence in the
  target stack's listing. A board tombstone cascades to all stacks/tasks.
- **R7 — labels field**: whole-set LWW (decision 4); remote label ids are
  resolved to local `LabelId`s through the index (§7.5) *before* the
  comparison — an unmapped remote label (new since the snapshot slice)
  is adopted as part of the set (R1 for the label entity itself).
- **R8 — position**: `stack`+`order` adopted or kept as one unit.

### 7.3 Push rules (`PushOutcome` ingestion)

```rust
pub struct PushOutcome { pub op: OpId, pub result: PushResult }
pub enum PushResult {
    /// Server accepted; echo = response body as a remote view where the
    /// endpoint returns one (create/update/delete do; reorder does not).
    Applied { echo: Option<RemoteEcho> },
    /// 404/403-shaped "gone" — falls through to the R3/R6 delete rules.
    RemoteMissing,
    Rejected { kind: SyncErrorKind },   // op stays queued (except BadRequest → dead-letter)
}
pub enum RemoteEcho { Task(RemoteTask), Stack(RemoteStack), Label(RemoteLabel) }
```

- **R9a — create pushed** (`Applied`): bind the returned `RemoteCardRef`,
  adopt the echo: values = echo values **unconditionally** (not LWW!) and
  `clocks = max(local clocks, echo.last_modified)`. Rationale: the server
  normalizes what we just wrote (trims, reformats, truncates timestamps to
  whole seconds); LWW with a seconds-resolution echo would let the local
  sub-second values win forever and the normalization never lands.
  `remote_seen = echo.last_modified`. Complete the op.
- **R9b — update pushed**: same unconditional echo adoption; complete op.
- **R9c — delete pushed**: finalize the tombstone (keep it), clear
  `remote`, complete op. Deck's DELETE response is authoritative — if the
  next pull still lists the card, R5's tombstone clock (`>= ts_r`) keeps
  the tombstone (cache-lag safe by construction).
- **R9d — move pushed** (`reorder`, no echo): complete the op; leave the
  baseline alone; the next pull reconciles (Deck's cache lag on
  cross-stack moves is an established fact).
- **R9e — `RemoteMissing`**: apply the corresponding delete rule (R3 for
  tasks, R6 for stacks/labels) — this is also the "update pushed to a
  since-deleted card" path.
- **R9f — `Rejected`**: op stays queued for retry; `Rejected { kind:
  BadRequest }` marks the op dead-lettered (server will never accept it;
  surfaced via `SyncPhase::Failed`).

### 7.4 Worked examples (canonical; become the docker-tier scenarios' expected outcomes in Phase 4)

Baseline everywhere: `remote_seen = 10:00:00Z`, all clocks `10:00:00`.

1. **Disjoint field edits both survive.** Local edits title at
   `10:05:30.500`; remote edits description with `ts_r = 10:03:00`.
   Merge: title keeps local (`10:03:00 < 10:05:30.5`); description adopts
   remote (`10:03:00 > 10:00:00`). Both edits live.
2. **Granularity trap (documented loss).** Same, but remote's description
   edit has `ts_r = 10:06:00` (after the local title edit). The remote
   title — unchanged on the server — overwrites the local title
   (`10:06:00 > 10:05:30.5`). Entity-level remote granularity; the
   Phase 4 docker scenario pins this exact outcome.
3. **Remote card delete vs concurrent local edit.** Local edits title at
   `10:05`; sync at `11:00` finds the card absent → R3: tombstone at
   `11:00`, remote ref cleared, pending `UpdateTask` cancelled. The
   `10:05` edit is lost (decision 3).
4. **Local delete vs remote edit, both directions.** Local deletes at
   `10:05` (`clocks.deleted = 10:05`); remote edits description at
   `10:03` → tombstone stands, delete pushes over the remote edit.
   Remote edit at `10:06` → resurrect with remote content, pending
   `DeleteTask` dropped.
5. **Stack soft delete (real timestamp).** Remote stack
   `deleted_at = 10:07`; local rename at `10:05` → delete wins (LWW),
   cascade R3 to the stack's tasks. Local rename at `10:08` → rename
   wins, stack stays live, push retries; if the server then answers the
   rename with `RemoteMissing`, R6 falls through to delete-wins.
6. **Echo normalization.** Offline create at `10:59:59.700`; pushed;
   server echoes `last_modified = 11:00:00` with trimmed title. R9a
   adopts the trimmed title and stamps all clocks `11:00:00` (max) —
   the next pull (`ts_r = 11:00:00`, not `>`) changes nothing.

### 7.5 Merge API (sketch — signatures stable, bodies TDD'd)

```rust
/// Lookup-only index derived from current entities (rebuilt per sync,
/// O(n); incremental maintenance is backlogged).
pub struct RemoteIndex {
    pub task_by_ref:  HashMap<RemoteCardRef, TaskId>,
    pub stack_by_ref: HashMap<RemoteStackRef, StackId>,
    pub label_by_ref: HashMap<RemoteLabelRef, LabelId>,
}
pub fn remote_index(state: &AppState) -> RemoteIndex;

// Per-entity primitives (pure, total, clock-free):
pub fn adopt_remote_task(remote: &RemoteTask, id: TaskId) -> Task;              // R1
pub fn merge_task(local: &Task, remote: &RemoteTask, ctx: &RemoteIndex) -> Task; // R4/R5/R7/R8
pub fn tombstone_task(local: &Task, observed_at: DateTime<Utc>) -> Task;        // R3
pub fn adopt_after_push(local: &Task, echo: &RemoteTask, ctx: &RemoteIndex) -> Task; // R9a/R9b
pub fn finalize_pushed_delete(local: &Task) -> Task;                           // R9c
// + merge_stack / merge_board / merge_label / stack cascade helpers (R6)

/// The pipeline the engine calls once per SyncReport::Completed.
/// Processing order is fixed: push outcomes first (they establish new
/// baselines), then board → stacks → labels → tasks (R1 before R7
/// resolution), then presence reconciliation (R3) for bound-but-absent
/// entities, then sync-status transition.
pub struct SyncApplication {
    pub boards: BTreeMap<BoardId, Board>,
    pub stacks: BTreeMap<StackId, Stack>,
    pub tasks:  BTreeMap<TaskId, Task>,
    pub labels: BTreeMap<LabelId, Label>,
    pub sync: SyncStatus,
    /// Persistence batch for the storage actor (upserts + op transitions).
    pub actions: Vec<PersistenceAction>,
}
pub fn apply_sync_report(
    current: &AppState,
    outbox: &[PendingOp],
    snapshot: &RemoteBoardSnapshot,
    pushes: &[PushOutcome],
    now: DateTime<Utc>,
) -> SyncApplication;
```

### 7.6 Clock-skew disclaimer (normative text for ADR 0004)

All comparisons are cross-machine wall-clock (`DateTime<Utc>`); no NTP
alignment is assumed. The strict-`>`-with-tie→local bias is the documented
deterministic behavior: within-skew misordering can revert a just-made
local edit to a slightly-"newer" remote value, and cannot be fixed without
server preconditions Deck does not offer. Property tests use logical times
and are skew-free by construction.

## 8. Outbox Operation Type (`ops.rs`)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OpId(pub Uuid);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LocalOp {
    CreateTask(TaskId), UpdateTask(TaskId), MoveTask(TaskId), DeleteTask(TaskId),
    CreateStack(StackId), RenameStack(StackId), DeleteStack(StackId),
    CreateLabel(LabelId), UpdateLabel(LabelId), DeleteLabel(LabelId),
    AssignLabel(TaskId, LabelId), UnassignLabel(TaskId, LabelId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingOp { pub op_id: OpId, pub op: LocalOp, pub queued_at: DateTime<Utc> }
```

Id-scoped, no field sets: `TaskClocks` already carries field knowledge,
and every Deck write is full-send anyway (client-surface plan decision 4).
No board ops — MVP binds an existing remote board. Op **coalescing**
(update+delete → delete, etc.) is Phase 2/4 storage/actor logic built on
these types; `MoveTask`'s remote semantics get tier-2 verification in
Phase 4 before the push side trusts it (roadmap §5).

## 9. Message Protocols (`messages.rs`)

```rust
/// UI/CLI → State Engine (tokio mpsc; payload type — see decision D8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StateCommand {
    CreateTask   { title: String, stack: StackId, order: i64 },
    UpdateTask   { id: TaskId, changes: TaskChanges },
    SetTaskDone  { id: TaskId, done: bool },
    MoveTask     { id: TaskId, stack: StackId, order: i64 },
    DeleteTask   { id: TaskId },
    CreateStack  { title: String, order: i64 },
    RenameStack  { id: StackId, new_title: String },
    DeleteStack  { id: StackId },
    CreateLabel  { title: String, color: Color },
    UpdateLabel  { id: LabelId, changes: LabelChanges },
    DeleteLabel  { id: LabelId },
    AssignLabel  { task: TaskId, label: LabelId },
    UnassignLabel{ task: TaskId, label: LabelId },
    /// User-visible refresh request; engine forwards to the sync actor.
    RequestSync,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskChanges {
    pub title: Option<String>,
    pub description: Option<String>,
    /// `Some(None)` = clear the due date; `None` = untouched.
    pub duedate: Option<Option<DateTime<Utc>>>,
    pub archived: Option<bool>,
}
// LabelChanges { title: Option<String>, color: Option<Color> } likewise.

/// Global system notifications (tokio broadcast).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SystemEvent { NetworkLost, NetworkRestored, Shutdown }

/// Engine → UI repaint trigger (tokio broadcast).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EngineSignal { StateUpdated }

/// Engine/app → sync actor (tokio mpsc; payload type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncCommand {
    /// First-run board binding (CLI `boards select`); MVP single board.
    SetBoard(RemoteBoardId),
    /// Nudge: run a cycle ASAP (used after local edits and by `sync`).
    SyncNow,
}

/// Sync actor → engine (tokio mpsc; payload type).
#[derive(Debug, Clone, PartialEq)]
pub enum SyncReport {
    Completed { snapshot: RemoteBoardSnapshot, pushes: Vec<PushOutcome> },
    Failed { kind: SyncErrorKind },
}
```

Channel-bearing envelopes (e.g. `taskboard-state`'s inbox wrapping a
`Flush { reply: oneshot::Sender<()> }` barrier for CLI exit, or the sync
actor's `RunOnce` handle) are Phase 3/5 crate types; they embed these
payloads. Every enum here is pure data — `SystemEvent`/`EngineSignal`/
`SyncCommand` are `Copy` and go on broadcast/mpsc channels verbatim.

## 10. Module Layout, `test-support`, Implementation Order

```
crates/taskboard-domain/
├── Cargo.toml        # deps unchanged; proptest → optional; [features] test-support = ["dep:proptest"]
└── src/
    ├── lib.rs        # crate docs, #![forbid(unsafe_code)], flat re-exports (sync-crate style)
    ├── ids.rs        # id newtypes, remote refs                        (~120 lines)
    ├── clock.rs      # Clock, SystemClock, IdGenerator, UuidV7Generator (~90)
    ├── entities.rs   # Board/Stack/Task/Label, clocks, Color, helpers   (~400)
    ├── state.rs      # AppState, SyncStatus, SyncPhase, view helpers    (~150)
    ├── remote.rs     # RemoteBoardSnapshot + views, RemoteIndex          (~200)
    ├── ops.rs        # OpId, LocalOp, PendingOp                          (~90)
    ├── persistence.rs# TaskRepository, PersistenceAction, PersistedState,
    │                 # RepositoryError, BoxFuture, SyncValidators        (~200)
    ├── messages.rs   # §9 enums + changesets                            (~220)
    ├── merge.rs      # §7 policy + apply_sync_report                    (~500 + tests)
    └── test_support/ # #[cfg(feature = "test-support")] public module
        ├── mod.rs    # prop strategies for all entities/views (bounded strings,
                      # sane timestamp ranges, referentially-consistent trees)
        ├── memory.rs # InMemoryRepository (std Mutex, ready futures, no tokio)
        └── contract.rs # assert_task_repository_contract(&impl TaskRepository)
```

TDD order (one PR, `feat/domain-canonical-model`; stacked split
entities/policy only if review size demands — roadmap §3):

1. `ids.rs` + `clock.rs` (seams first; unit tests for v7 shape/uniqueness
   are generator-level only, not timing).
2. `entities.rs` + strategies in `test_support` (serde round-trip proptests
   land with the types).
3. `state.rs`, `remote.rs`, `ops.rs` (+ strategy coverage).
4. `persistence.rs` + `InMemoryRepository` + contract harness
   (self-run against the fake — both get tested at once).
5. `messages.rs` (data-only; compile-level tests + serde round-trips).
6. `merge.rs` primitives R1–R9 (each rule: unit example from §7.4 first,
   then properties).
7. `apply_sync_report` pipeline + processing-order tests.
8. Docs: ADR 0004, architecture.org refresh, backlog appends, CHANGELOG.

## 11. Test Plan (property catalogue)

All in-module `#[cfg(test)]` + `proptest!` (sync-crate conventions; 256
cases default like `backoff.rs`). Assertions are typed/structural — never
text (AGENTS §5).

1. **Idempotence**: `merge_task(merge_task(t, r, ctx), r, ctx') == merge_task(t, r, ctx)`
   for unchanged index; re-applying the same snapshot twice = no-op
   (second `apply_sync_report` emits no actions).
2. **Strict newness / tie-break**: for every field, remote wins iff
   `ts_r > clock`; at `ts_r == clock` local survives (boundary pair
   proptest — kills `>=` and `>` mutants).
3. **Clock monotonicity**: no field clock ever decreases through any
   merge/adoption path (bombard arbitrary sequences).
4. **Convergence**: two replicas with equal state applying the same report
   produce identical entity maps.
5. **R3 outcome facts**: tombstone set, `remote` cleared, ops for the task
   dropped from the resulting outbox actions.
6. **R5 resurrect/stay-tombstoned**: both directions of the `deleted`
   clock vs `ts_r`, delete-op dropped vs kept.
7. **R6 cascade**: stack board-tombstone final → tasks tombstoned; moved-
   out card survives in the target stack.
8. **R9a adoption**: post-push values equal the echo exactly; clocks ≥
   `max(prior, echo.last_modified)` (unconditional adoption — the
   trimmed-title example is a unit test).
9. **Pipeline determinism/order**: push outcomes applied before snapshot
   merge; R1 stacks/labels resolve before R7 label-set mapping.
10. **No-panic bombardment**: arbitrary `(Task, RemoteTask)` pairs through
    every public merge fn; invariant: referenced `StackId`/`LabelId`s
    exist under referentially-consistent generators.
11. **Serde round-trips** for every entity, `PersistedState`, changesets,
    and message payloads (`yaml` ↔ value identity).
12. **Contract harness** green on `InMemoryRepository` (and re-used
    verbatim by Phase 2 against sqlite).
13. **Snapshot determinism**: `AppState` YAML serialization is stable
    across insertion-order permutations (BTree discipline).

No `tokio::time::sleep` anywhere (nothing async to wait on — the only
async surface is the fake's ready futures). Miri runs the suite clean by
construction (no unsafe, no time reads). Mutation testing: the catalogue
above is the mutant-killing surface; any survivor in `merge.rs` is a bug
in the tests, not license to weaken them.

## 12. Out of Scope → Backlog (append with this plan)

- Add-wins (OR-set) label merge — per-label timestamps, removes only win
  when newer than the opposing add (replaces whole-set LWW, decision 4).
- Resurrect-as-new-card policy for R3 (remote card delete vs concurrent
  local edit) — keep the local content, re-push as a new card.
- Incremental maintenance of `RemoteIndex` (currently rebuilt O(n) per
  sync — fine at MVP scale).
- Local board-edit commands (`RenameBoard`, board colors) + the matching
  `Board` clocks.
- Clock-skew mitigation (e.g. configurable remote-age bias) — revisit if
  real-server use (M1) shows revert-on-sync symptoms.

## 13. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0004 — "Sync conflict policy"**: LWW-per-field model, local
  per-field vs remote entity-level granularity, strict-`>` tie→local,
  delete-as-timestamped-fact + card delete-wins fallback, whole-set
  labels, composite position, unconditional echo adoption, skew
  disclaimer (§7.6). Status: Accepted.
- **architecture.org**: refresh the entity list (`Task`, `Project`,
  `UserPreferences` → the canonical `Board`/`Stack`/`Task`/`Label` set;
  `Project` is superseded by `Board`, `UserPreferences` deferred) and the
  message-protocol table (payload/envelope split; engine-executes-merge).
- **CHANGELOG**: `[Unreleased]` → Added section per repo convention.
- **Backlog**: append §12 entries using the file's entry template.
- Fix the now-dangling intra-doc links in the domain crate docs.

## 14. Verification & Acceptance

- Full quality gate green (AGENTS §10): `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo nextest run --workspace`, `cargo test --doc --workspace`,
  `cargo deny check --workspace`.
- `cargo nextest run -p taskboard-domain --all-features` (covers the
  `test-support` feature build).
- Phase 1 exit (roadmap): entities + ports + policy functions exist, all
  property tests green, contract harness proven against the in-memory
  fake.
- Downstream compile check (no code changes outside domain):
  `cargo check --workspace` still green — the new types break no existing
  consumer (only additive; the `AppState` stub's shape change is
  snapshot-breaking by design, and nothing snapshots it yet).

## 15. Consumer Handoff Map (what later phases import)

| Consumer | Takes from Phase 1 |
|---|---|
| Phase 2 `storage-sqlite` | `TaskRepository`, `PersistenceAction`, `PersistedState`, `PendingOp`, `SyncValidators`, `RepositoryError`, contract harness, strategies |
| Phase 3 `state` | `StateCommand`, `EngineSignal`, `apply_sync_report`, `Clock`/`IdGenerator`, `InMemoryRepository`, `AppState` (+insta), `SystemEvent` |
| Phase 4 sync actor | `RemoteBoardSnapshot` views (DTO→view mapping targets), `PushOutcome`, `SyncErrorKind` (from `DeckError`), `SyncCommand`, `SyncReport`, outbox ops |
| Phase 5/6 app/CLI | `SystemEvent::Shutdown`, `SyncStatus`/`SyncPhase` for status output, `SyncCommand::SetBoard` |

## 16. Risks & Mitigations

- **Granularity data loss** (§7.4 ex. 2): inherent to Deck's entity-level
  timestamps; documented in ADR 0004 and pinned by a Phase 4 docker-tier
  scenario so the behavior is at least intentional and observable.
- **Absence-detection correctness** rests on the snapshot-completeness
  contract (§7.1) — enforced in Phase 4 actor tests (partial snapshots
  would silently tombstone live tasks).
- **Seconds-resolution echo truncation** — mitigated by unconditional
  echo adoption (R9a); the failure mode without it (eternal
  renormalization loops) is a unit test.
- **`AppState` snapshot noise** from per-field clocks: Phase 3 insta
  snapshots use injected fixed clocks, so values are deterministic.
- **Outbox growth / op storms**: coalescing and retention are Phase 2/4;
  Phase 1 only guarantees the types can express them.
- **Test-mass creep in one PR**: the stacked split (entities vs policy)
  remains available per roadmap §3 if review load demands it.
