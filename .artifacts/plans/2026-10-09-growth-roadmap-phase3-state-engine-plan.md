# [Plan] Growth Roadmap Phase 3 — State Engine: Command Transitions, Actor Loop, ArcSwap Publishing

- **Date**: 2026-10-09
- **Target**: `taskboard-state` (whole crate, stub → engine), a new pure
  command-semantics module in `taskboard-domain` (§4, decision 1), the
  shared action-application function in the domain's persistence module
  (decision 2), a `StorageHandle` port adapter in
  `taskboard-storage-sqlite` (§6), and a counting-id fake promoted into
  the domain's feature-gated `test_support` (decision 12).
- **Goal**: implement the roadmap's Phase 3 — the StateEngine actor: a
  command loop over pure transition functions, `ArcSwap<AppState>`
  publishing + `EngineSignal` broadcast, oneshot replies (command receipts +
  flush barrier), persistence dispatch through the domain port, ingestion of
  `SyncReport`s via the Phase 1 pipeline, and `SyncStatus` tracking — with
  the insta snapshot suite that defines the engine's contract.
- **Status**: ACCEPTED (user review 2026-10-09; rev 2 incorporated the
  agility doctrine — §1 working agreement, roadmap §2.5 — and the
  resulting decision changes: 1 flipped, 2 extended, 10/12/16 amended)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 3. Exit criterion: "snapshot suite defines the engine's
  contract."
- **Predecessors**: Phase 1
  (`.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md`,
  merged; its §15 handoff row for Phase 3 is the input contract) and
  Phase 2 (`.artifacts/plans/2026-10-09-growth-roadmap-phase2-storage-sqlite-plan.md`,
  merged; its §14 handoff row for Phase 3 is the input contract).

---

## 1. Context & Known Facts (do not re-derive)

- **Working agreement (user, 2026-10-09): no crate is frozen after its
  phase.** When a later phase finds that changing an earlier phase's
  output — a different crate — yields a better architecture, cleaner
  responsibilities, or better testability, that change belongs in the
  later phase's PR, without ceremony. The full test suite (contract
  harness, proptests, snapshots, quality gates) is the safety net that
  keeps such changes confident. This plan exercises the doctrine:
  decisions 1, 2, and 12 each reach into `taskboard-domain` on
  architecture merit alone.
- **Phase 1 is merged and supplies every payload the engine moves.**
  `StateCommand` (14 variants incl. `RequestSync`), `SystemEvent`
  (`NetworkLost`/`NetworkRestored`/`Shutdown`), `EngineSignal
  (::StateUpdated)`, `SyncCommand` (`SetBoard`/`SyncNow`), `SyncReport`
  (`Completed { snapshot, pushes }` / `Failed { kind }`), all in
  `crates/taskboard-domain/src/messages.rs`. The clock and identity seams
  (`Clock`/`SystemClock`, `IdGenerator`/`UuidV7Generator`) exist in
  `clock.rs`/`idgen.rs`. Proptest strategies (`app_state_strategy`,
  `persisted_state_strategy`, `snapshot_strategy`, `push_outcome_strategy`,
  …) exist behind the domain's non-default `test-support` feature.
- **The sync-ingestion half of the engine already exists as a pure
  function**: `pipeline::apply_sync_report(current, outbox, snapshot,
  pushes, ids, now) -> (AppState, Vec<PersistenceAction>)`
  (`crates/taskboard-domain/src/pipeline.rs`) — push outcomes first, then
  board → stacks → labels → tasks, presence reconciliation, stack/board
  cascades, sync-status transition. It emits entity upserts and
  `CompleteOp`/`FailOp` transitions; it never emits `UpsertSyncStatus`
  and never touches validators.
- **The local-command half does not exist anywhere.** There is no
  `apply_state_command`/transition function in any crate (verified by
  search): designing the command semantics catalogue (§4) is this phase's
  core intellectual work, exactly as the merge rules were Phase 1's.
- **`AppState` deliberately omits the outbox and validators**
  (`state.rs`: boards/stacks/tasks/labels/sync/`last_updated` only — it is
  the UI-facing projection). The engine therefore needs its own internal
  working state (`EngineCore`, §5.1) hydrated from `PersistedState`
  (which does carry `outbox` + `validators`).
- **Phase 2 is merged and supplies the persistence mechanics.** The port is
  `TaskRepository::{load, apply}` with `RepositoryError::{Unavailable,
  Corrupted}`; `PersistenceAction` covers entity upserts, op
  enqueue/complete/fail, `UpsertValidators`, and `UpsertSyncStatus` (phase +
  last success; `pending_ops` derived at load from outbox depth —
  `InMemoryRepository` implements the identical derivation). The storage
  actor (`StorageCommand::{Apply, Load}` + `StorageHandle`, oneshot replies,
  FIFO + reply-after-commit) exists in
  `crates/taskboard-storage-sqlite/src/actor.rs`.
- **Crate state**: `taskboard-state/src/lib.rs` is doc-comments +
  `#![forbid(unsafe_code)]` only, but its manifest already pins exactly the
  intended dependencies (`taskboard-domain`, `arc-swap`, `tokio`,
  `thiserror`, `tracing`; dev-deps `insta`, `proptest`, `tokio` with
  `test-util`) — and `architecture.org` §"Crate Roles" confirms the
  boundary that matters: its dependency list for this crate does not
  include `taskboard-storage-sqlite`. **`taskboard-state` must not depend
  on `taskboard-storage-sqlite`** (and `taskboard-ui-slint` must not
  depend on `taskboard-state`: the UI gets `Arc<ArcSwap<AppState>>` + a
  domain-typed channel from the Phase 5 bootstrap, never the handle —
  §12).
- **CI already targets this crate for the deep gates**: `miri.yml` runs
  `cargo +nightly miri test -p taskboard-domain -p taskboard-state` and
  `mutation-testing.yml` runs cargo-mutants against the same pair — both
  currently no-op over the state stub and start counting the moment this
  phase lands. `testing_strategy.org` §10 scopes Miri to "pure logic,
  serialization, and state-transition tests".
- **Phase 2 constraints that bind the engine**: mpsc FIFO + oneshot
  reply-after-commit is the flush barrier (Phase 2 decision 13 / test A2);
  `pending_ops` derivation is contract-pinned (Phase 2 decision 9); the
  `apply` batch is one transaction (atomic: engine may treat a batch as
  all-or-nothing); tombstones must survive restarts (Phase 2 §4.3).
- **Domain facts that shape command semantics**: local ids are UUIDv7;
  every editable field carries a per-field write clock; position
  (`stack`+`order`) shares one clock (`ck_position`); labels are one
  whole-set field (`ck_labels`); tombstones are `deleted` + the `deleted`
  clock, and a locally-deleted *bound* entity keeps its remote binding
  until the push confirms (R9c finalizes); `LocalOp`s are id-scoped with
  no field sets; op coalescing is explicitly Phase 4 push-side logic
  (Phase 1 §8, Phase 2 §11) — the engine must not do it.
- **House rules that bite this crate**: `#![forbid(unsafe_code)]`, SPDX
  `MIT OR Apache-2.0` header, `# Errors` doc sections on fallible public
  fns, no `unwrap`/`expect` in non-test library code, `tracing` spans
  across `.await`, typed outcome assertions only (no text), no test sleeps
  (reply/join-handle/predicate-with-deadline are the completion signals),
  one behavior one test at the lowest layer (pure transition tests come
  before engine-thread routing tests; routing tests don't re-assert
  semantics), insta for `StateEngine` state trees (AGENTS §5).

## 2. Scope

**In scope** — the workspace grows the running core:

| Area | Deliverable |
|---|---|
| Command semantics | Pure `plan_command` transition function + typed `CommandOutcome`/`CommandError` in `taskboard-domain` (`command.rs`), full §4 catalogue |
| Working state | `EngineCore` (a `PersistedState` + `last_updated`) + the single-advance `interpret`, delegating to the domain's shared `apply_actions` (§5.1) |
| Actor | `EngineCommand` envelopes (Execute with optional oneshot reply, Flush barrier), `EngineHandle`, `spawn_state_engine` select-loop over three inboxes (§5.3–5.5) |
| Sync ingestion | `SyncReport::{Completed, Failed}` handling via `apply_sync_report`; sync-status machine incl. `SystemEvent` handling (§5.4) |
| Persistence | `Arc<dyn TaskRepository>` dispatch, apply-then-interpret ordering, storage-failure semantics (§5.1, decision 5/15) |
| Storage adapter | `impl TaskRepository for StorageHandle` in `taskboard-storage-sqlite` + routing tests (§6) |
| Tests | pure unit suite, proptest laws, insta snapshot scenarios, async routing suite, one sqlite-actor integration smoke (§8) |
| Docs | ADR 0006, architecture.org state bullet + channel table refresh, CHANGELOG, backlog appends (§10) |

**Out of scope** (owned by later phases): any sync-actor logic — poll
loop, outbox *consumption*, coalescing, DTO mapping, validator
production (Phase 4; the engine only leaves the ingestion door open,
§12); config/paths/bootstrap/channel-graph ownership (Phase 5); CLI
subcommands and app-level E2E (Phase 6); UI (Phase 7); write batching /
pipelining, query-envelope variants beyond Execute/Flush, a
storage-failure `SystemEvent` (backlog, §9).

## 3. Design Decisions

1. **Local command semantics live in `taskboard-domain` as a pure module
   (`command.rs`), mirroring `pipeline.rs`.** The engine executes both
   halves of its transition logic from one place: the sync side from
   `pipeline::apply_sync_report`, the command side from
   `command::plan_command`. Rationale: `StateCommand`'s payload and its
   semantics co-locate (one crate answers "what does this command
   mean"); the domain is the project's home for pure, clock/id-injected
   transitions over domain types, with the proptest strategies and the
   Miri/mutants CI pair already there; and the engine crate stays a thin
   orchestrator (channels, ArcSwap, sequencing — nothing else), the
   cleanest possible review and mutation-testing split. Rev 1 of this
   plan had argued for `taskboard-state` on "engine vocabulary, single
   consumer" grounds plus an aversion to touching a merged crate; the
   freeze half of that argument is withdrawn (working agreement, §1) —
   and the single-consumer argument never applied to `apply_sync_report`,
   which lives in the domain although only the engine runs it. Symmetry
   wins. *Rejected*: keeping the catalogue in `taskboard-state` (splits
   `StateCommand`'s shape and meaning across crates, and re-declares
   strategies and fakes the domain already owns behind `test-support`).
2. **Single-advance interpret loop — memory advances only by
   interpreting the very action batch that was persisted — with the
   action-application semantics implemented exactly once.** The engine
   never applies a mutation to memory except via
   `EngineCore::interpret(&actions, now)`, and `interpret` runs **only
   after** `repo.apply(actions)` returned `Ok`. The domain grows one
   pure function, `persistence::apply_actions(&mut PersistedState,
   &[PersistenceAction])` — upserts, op transitions, validator upserts,
   status writes, and the post-batch `pending_ops` derivation; the port
   contract made executable — and both consumers delegate to it: the
   engine's `EngineCore` (§5.1) and `test_support::InMemoryRepository`,
   whose inline match moves out of the feature-gated fake into
   production domain code. Consequences (all load-bearing):
   - memory ≡ disk by construction: the same `Vec<PersistenceAction>`
     lands in both through literally the same semantics;
   - storage failure needs **no rollback machinery**: a failed `apply`
     means `interpret` never ran, so the in-memory state (and the
     published `AppState`) simply never advanced — the command is
     reported as failed and the next attempt re-plans against unchanged
     state;
   - crash windows are safe: a crash after `apply` but before
     `interpret` loses only in-process memory, which the next boot
     re-hydrates from the authoritative disk state;
   - the fake is no longer an independent re-implementation that can
     drift from the engine — the drift the originally conceived
     "interpret ≡ repository" property guarded against cannot exist.
     Its correctness against the port clauses is pinned by
     `apply_actions`' own inline unit tests, and the Phase 2 contract
     harness re-run pins sqlite parity; the behavior-preserving fake
     refactor is this plan's showcase of the §1 working agreement.
   `apply_sync_report`'s returned `AppState` is therefore *redundant for
   the engine* (the engine interprets the batch instead) — the
   redundancy is pinned by proptest P3 (§8) so the two can never drift.
   *Rejected*: advancing memory from the pipeline's returned state and
   maintaining the outbox separately (two bookkeeping paths; drift
   becomes possible instead of impossible-by-construction).
3. **The engine consumes the domain port, hexagonally:
   `Arc<dyn TaskRepository>`.** Production wiring (Phase 5) hands the
   engine the sqlite repository *through its actor* via a small
   `impl TaskRepository for StorageHandle` adapter (§6) — the actor's
   oneshot round-trips become the port's `BoxFuture`s. Tests use
   `InMemoryRepository` (and a failing wrapper, §8 A9). This honors
   architecture.org's dependency list for the crate (no
   `taskboard-storage-sqlite` edge) *and* the roadmap's "persistence
   dispatch to the storage actor" topology. *Rejected*:
   `taskboard-state` depending on `taskboard-storage-sqlite` (couples the
   engine to one adapter; breaks the testing strategy §1
   inject-dependencies rule).
4. **Two inbox kinds, both engine-owned envelopes; the rest of the graph
   is injected.** The engine creates and owns its command mpsc
   (`EngineCommand`), its `EngineSignal` broadcast, and the
   `Arc<ArcSwap<AppState>>`. Construction *takes* the external channels:
   `sync_reports: mpsc::Receiver<SyncReport>`, `system:
   broadcast::Receiver<SystemEvent>`, and `sync_out:
   mpsc::Sender<SyncCommand>` — the Phase 5 bootstrap owns the channel
   graph and keeps the matching sender/receiver halves for the sync
   actor and the system broadcaster. This is the payload/envelope split
   (ADR 0001/0004): domain payloads verbatim on the wire, channel-bearing
   envelopes declared here. Phase 1's plan explicitly named this crate's
   `Flush { reply: oneshot::Sender<()> }` envelope as the pattern's
   example — it lands here as `EngineCommand::Flush`.
5. **Sequential per-message processing; `repo.apply` is awaited inside
   the loop.** One message is planned, applied, interpreted, published,
   and replied before the next is polled. FIFO ordering makes outbox
   order trivially correct and "reply received ⇒ batch durable" the CLI
   flush semantics Phase 5/6 rely on (Phase 2 decision 13). A local
   sqlite apply is sub-millisecond; UI-dispatched commands are
   fire-and-forget so nothing UI-side blocks on the round-trip.
   *Rejected*: pipelining commands over in-flight writes (ordering and
   error-attribution complexity with no MVP payoff — backlogged, §9).
6. **Typed rejections; idempotent-accept no-ops.** Rejections are a
   `CommandError` enum (§4.2) asserted by variant, never text. Repeated
   *terminal* intents are accepted no-ops (`Ok(CommandOutcome::NoOp)`,
   zero actions, no publish, no signal, no persist): delete-already-deleted,
   assign-already-assigned, unassign-not-assigned, move-to-identical-
   position, rename-to-identical-title, set-done-to-current-state,
   empty `TaskChanges`. Rationale: a second delete after a lost UI
   refresh is redundant success, not an error; whereas *edit*-style
   intents against tombstones are errors (`TaskDeleted` etc.) because
   they cannot be meaningless-redundant. This split is a deliberate UX
   contract, pinned per-case in §8.
7. **Local `DeleteStack` cascades tombstones to the stack's live tasks**
   (mirroring R6's remote cascade so the local view is immediately
   consistent with what the server will do): each live task in the stack
   is tombstoned (`deleted` clock = now). Per task: if it has a remote
   binding, enqueue `DeleteTask(task)`; if it is unbound (never pushed),
   cancel its pending ops (`FailOp`) and enqueue nothing — the server
   never knew it. Sibling pending ops of *bound* cascaded tasks stay
   queued; dedupe is Phase 4 coalescing (explicitly not ours).
   `DeleteTask` on an unbound task follows the same unbound rule.
8. **Nudge rule: forward `SyncCommand::SyncNow` iff the accepted batch
   enqueued ≥ 1 new op** (or the command was `RequestSync`). The engine
   does not debounce, delay, or dedupe nudges — the Phase 4 actor ignores
   nudges while a cycle is running (its plan's job). A nudge is
   fire-and-forget: a dead `sync_out` sender is a `debug!` event (no sync
   actor exists yet in Phase 3 tests unless one is wired).
9. **`SyncPhase::Syncing` is a published-but-never-persisted transient.**
   The engine sets it (memory + ArcSwap + signal) only when a
   `SyncNow`/nudge forward *succeeds* — otherwise a missing sync actor
   would leave the badge stuck. It is excluded from the persisted batch
   (a reboot mid-sync must not advertise a phantom in-flight cycle; the
   loaded phase shows the last stable state). All other phase
   transitions are persisted via an engine-appended
   `UpsertSyncStatus` — appended **iff phase or `last_success` changed**
   (local commands never append it: they change neither, and
   `pending_ops` is derived from the outbox the `EnqueueOp` actions
   already persist).
10. **`pending_ops` is always derived, never tracked.** The shared
    `apply_actions` re-derives `sync.pending_ops = outbox.len()` after
    every batch, and both repository implementations derive at `load()`
    (Phase 2 decision 9) — one invariant, expressed once per site it
    applies to, all contract-pinned.
11. **`last_updated` stamps once per message, from the same `now` as the
    entity clocks.** The loop reads `clock.now()` once per message and
    threads it through `plan_command`/`apply_sync_report` *and*
    `interpret`, so entity clocks, `last_updated`, and snapshots can
    never disagree at sub-read granularity (matters under the fixed test
    clock, where two reads return equal instants).
12. **Snapshot determinism by construction**: test fakes (a
    fixed/advancing `Clock` local to this crate; a sequence
    `IdGenerator` mapping u128 counters to ids — promoted from the
    domain's `cfg(test)`-private `merge_testutil::CountingIds` into the
    feature-gated `test_support` module, so downstream crates reuse one
    fake instead of re-declaring it) make insta YAML snapshots of
    `AppState` **and** of the fake repo's `PersistedState` fully
    deterministic — the outbox is thereby visible in snapshots (§8
    S-series).
13. **Observability is a deliverable** (same discipline as Phase 2
    decision 15): `error!` — a persisted batch lost to `RepositoryError`
    (the "command silently failed" class); `warn!` — command rejections
    (variant name, not text) and replies to a dropped engine; `info!` —
    engine start/stop, boot hydration counts (entities, outbox depth);
    `debug!` — each command received/settled, each sync report ingested,
    each nudge forwarded; `trace!` — per-action batch detail (entity
    kind, id). No user content (titles) above `trace`. Logging arms are
    not asserted in tests (AGENTS §5 no-text rule) — accepted gap,
    existing backlog entry.
14. **`dispatch` never blocks or panics; `execute` awaits.** UI-side
    dispatch (Slint callbacks are sync) uses `try_send` on a generous
    command buffer (256): on full, `error!` and drop — unreachable at
    human input rates, and a panic in a UI callback is never correct.
    CLI-side `execute` sends and awaits the oneshot reply.
15. **Publish-iff-changed, then signal.** After `interpret`, the engine
    compares the working `AppState` to the previously published one
    (O(board) equality on clone — micro-scale, accepted) and only then
    swaps the `ArcSwap` and broadcasts `EngineSignal::StateUpdated`.
    Under the fixed test clock this makes same-instant no-op syncs
    provably signal-free. Signal after swap, so a waking UI reads fresh
    state.
16. **Miri alignment**: the pure modules (domain `command.rs` +
    `apply_actions`, state `core.rs`) carry the Miri value; their tests
    are runtime-free. If
    tokio-based routing tests misbehave under Miri (unsupported timer
    paths), they get `#[cfg_attr(miri, ignore)]` with a comment quoting
    testing strategy §10's scope ("pure logic, serialization, and
    state-transition tests") — an environment gate on async plumbing,
    not a weakened assertion. Verified locally during this phase, not
    discovered by the weekly CI job.
17. **Roadmap interpretation notes** (veto points for review):
    - *"oneshot queries"* (roadmap §3 Phase 3) is realized as the
      Execute reply (typed `Result<CommandOutcome, ExecuteError>`) plus
      the `Flush` barrier. Read-style queries need nothing: the ArcSwap
      *is* the query surface (lock-free reads; the CLI's `sync` status
      watch is a predicate poll on it). Typed query *variants* (e.g.
      wait-for-idle, outbox dump for Phase 4) are additive envelope
      variants when their consumers exist — backlogged, not speculative.
    - *"persistence dispatch to the storage actor"* is realized through
      decision 3's port adapter, keeping the crate dependency graph of
      architecture.org intact.

## 4. Command Semantics Catalogue (`taskboard-domain` `command.rs`, normative)

`plan_command(&AppState, &[PendingOp], &dyn IdGenerator, now) ->
Result<CommandPlan, CommandError>` — pure, total, clock/id-injected
(matching the domain seams; the outbox slice lets delete/cascade paths
cancel a target's pending ops by id, exactly like `apply_sync_report`
takes it). `CommandPlan { actions: Vec<PersistenceAction>, outcome:
CommandOutcome }`. All clock stamps below are `now`. "Live" =
`!deleted`. The bound board = the single live board in `app.boards`
(MVP: 0..1 entries; if several ever coexist pre-multi-board, the
smallest `BoardId` wins deterministically — documented totality, not a
policy).

### 4.1 Per-command rules

| Command | Precondition | Effect (entity) | Actions enqueued | Outcome |
|---|---|---|---|---|
| `CreateTask { title, stack, order }` | stack exists & live | new `Task`: fresh id, all 8 clocks = now, `done: None`, `labels: ∅`, `archived: false`, `deleted: false`, no remote binding | `UpsertTask`, `EnqueueOp(CreateTask(id))` | `CreatedTask(id)` |
| `UpdateTask { id, changes }` | task exists & live; non-empty changeset | set each `Some` field; stamp `clocks.{title,description,duedate,archived}` per touched field only (position/labels/deleted clocks untouched) | `UpsertTask`, `EnqueueOp(UpdateTask(id))` | `Applied` |
| `SetTaskDone { id, done }` | task exists & live; `done` state actually changes | `done = Some(now)` / `None`; stamp `clocks.done` | `UpsertTask`, `EnqueueOp(UpdateTask(id))` | `Applied` |
| `MoveTask { id, stack, order }` | task exists & live; target stack exists & live; `(stack, order)` actually different | set `stack` + `order`; stamp `clocks.position` (composite intent, one clock) | `UpsertTask`, `EnqueueOp(MoveTask(id))` | `Applied` |
| `DeleteTask { id }` | task exists & live | `deleted = true`, stamp `clocks.deleted`; **keep** remote binding (R9c finalizes on push) | bound: `EnqueueOp(DeleteTask(id))`; unbound: `FailOp` for all its pending ops, no new op (decision 7) | `Applied` |
| `CreateStack { title, order }` | a live board is bound | new `Stack` on that board, 3 clocks = now | `UpsertStack`, `EnqueueOp(CreateStack(id))` | `CreatedStack(id)` |
| `RenameStack { id, new_title }` | stack exists & live; title differs | set title, stamp `clocks.title` | `UpsertStack`, `EnqueueOp(RenameStack(id))` | `Applied` |
| `DeleteStack { id }` | stack exists & live | stack tombstoned (`clocks.deleted`); **cascade**: every live task of the stack tombstoned per the `DeleteTask` bound/unbound rules (decision 7) | `UpsertStack`, per-task `UpsertTask`, `EnqueueOp(DeleteStack(id))` + per-bound-task `EnqueueOp(DeleteTask(..))` + per-unbound-task `FailOp`s | `Applied` |
| `CreateLabel { title, color }` | a live board is bound | new `Label` on that board, 3 clocks = now | `UpsertLabel`, `EnqueueOp(CreateLabel(id))` | `CreatedLabel(id)` |
| `UpdateLabel { id, changes }` | label exists & live; non-empty changeset | set `Some` fields; stamp `clocks.{title,color}` per touched field | `UpsertLabel`, `EnqueueOp(UpdateLabel(id))` | `Applied` |
| `DeleteLabel { id }` | label exists & live | label tombstoned; task label sets untouched (tombstone hides it; R7 reconciles remotely; the `task_labels` rows survive — tombstones are in-table per Phase 2) | bound: `EnqueueOp(DeleteLabel(id))`; unbound: `FailOp` its ops | `Applied` |
| `AssignLabel { task, label }` | task & label exist & live; label not already in the set | insert into set, stamp `clocks.labels` (whole-set clock) | `UpsertTask`, `EnqueueOp(AssignLabel(task, label))` | `Applied` |
| `UnassignLabel { task, label }` | task & label exist & live; label currently in the set | remove from set, stamp `clocks.labels` | `UpsertTask`, `EnqueueOp(UnassignLabel(task, label))` | `Applied` |
| `RequestSync` | — | no entity change; handled at the actor layer (forward `SyncNow`, set `Syncing` on success — decision 9) | none (actor-level `UpsertSyncStatus` for the phase is *not* persisted) | `SyncRequested` (or `NoOp` if the forward failed — actor-level) |

Idempotent no-ops (decision 6) return `Ok(CommandOutcome::NoOp)` with
empty actions: the "actually changes/differs" guards in the table. Note
`UpdateTask`/`UpdateLabel` with an all-`None` changeset and
`SetTaskDone`/`MoveTask`/`RenameStack` that would write identical values
are all no-ops — no push traffic for nothing.

### 4.2 Error & outcome types

```rust
// taskboard-domain::command — semantic rejection classes, semantic
// receipts. plan_command never fails for transport or lifecycle
// reasons; it cannot even name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error("unknown task")]
    UnknownTask(TaskId),
    #[error("task is deleted")]
    TaskDeleted(TaskId),
    #[error("unknown stack")]
    UnknownStack(StackId),
    #[error("stack is deleted")]
    StackDeleted(StackId),
    #[error("unknown label")]
    UnknownLabel(LabelId),
    #[error("label is deleted")]
    LabelDeleted(LabelId),
    #[error("no live board is bound")]
    NoBoard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    CreatedTask(TaskId), CreatedStack(StackId), CreatedLabel(LabelId),
    Applied, NoOp, SyncRequested,
}

// taskboard-state::error — engine-lifecycle failure classes wrap the
// semantic ones. The CLI still matches once, on this enum.
#[derive(Debug, thiserror::Error)]
pub enum ExecuteError {
    #[error("command rejected")]
    Rejected(#[from] CommandError),
    #[error("persistence failed; state unchanged")]
    Storage(#[from] RepositoryError),
    #[error("engine is stopped")]
    EngineGone,
}
```

Responsibility split: the domain owns what a command *means* (semantic
rejections, receipts); the engine crate owns what *running* the engine
means (storage round-trip failures, dead channels). `Storage` and
`EngineGone` are produced only by the actor layer (failed `apply`, dead
channel).

## 5. Engine Architecture

### 5.1 Working state + `interpret` (`core.rs`)

```rust
/// The engine's private working state: the persisted shape plus the one
/// field persistence never knew.
pub struct EngineCore {
    state: PersistedState,
    last_updated: Option<DateTime<Utc>>,
}

impl EngineCore {
    /// Hydrated at boot; `last_updated` starts None (unknown-at-boot).
    pub fn from_persisted(p: PersistedState) -> Self;
    /// Advance memory by exactly the batch that was persisted
    /// (`persistence::apply_actions`), stamp `last_updated`, and return
    /// whether the published projection changed.
    pub fn interpret(&mut self, actions: &[PersistenceAction], now: DateTime<Utc>) -> bool;
    /// The UI projection (entity maps + derived `pending_ops` + stamp).
    pub fn app(&self) -> AppState;
    /// Test/assertion view mirroring the persisted shape.
    pub fn persisted_view(&self) -> PersistedState;
}
```

`apply_actions` (domain, shared with the fake) owns the per-action
semantics: upserts replace by id; `EnqueueOp` appends (an in-place
replace on id collision mirrors today's fake); `CompleteOp`/`FailOp`
remove by `op_id`; `UpsertValidators` replaces the key;
`UpsertSyncStatus` sets phase + `last_success` only (never
`pending_ops`); a final derivation sets `sync.pending_ops` from
`outbox.len()`. `interpret` adds only the engine-only duties: the
`last_updated = Some(now)` stamp and decision 15's changed-detection
(compare the pre/post `app()` projections — clone + `PartialEq`).

### 5.2 The actor loop (`actor.rs`, sketch)

```rust
pub enum EngineCommand {
    Execute { command: StateCommand,
              reply: Option<oneshot::Sender<Result<CommandOutcome, ExecuteError>>> },
    Flush   { reply: oneshot::Sender<()> },
}

#[derive(Clone)]
pub struct EngineHandle { /* command tx, signal tx, Arc<ArcSwap<AppState>> */ }
impl EngineHandle {
    pub async fn execute(&self, c: StateCommand) -> Result<CommandOutcome, ExecuteError>;
    pub fn dispatch(&self, c: StateCommand);            // try_send; error! on full
    pub async fn flush(&self);                           // durability barrier
    pub fn subscribe(&self) -> broadcast::Receiver<EngineSignal>;
    pub fn shared_state(&self) -> Arc<ArcSwap<AppState>>; // UI-facing, no state-crate dep needed by readers
}

pub async fn spawn_state_engine(
    repo: Arc<dyn TaskRepository>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    sync_out: mpsc::Sender<SyncCommand>,
    sync_reports: mpsc::Receiver<SyncReport>,
    system: broadcast::Receiver<SystemEvent>,
) -> Result<(EngineHandle, tokio::task::JoinHandle<()>), EngineStartupError>;
```

Loop body (per message, one `now = clock.now()`):

- **Execute**: `plan_command` → `Err` ⇒ reply
  `Err(ExecuteError::Rejected(_))` (warn!). Empty actions ⇒ reply
  `Ok(outcome)` (no-op; nothing persisted, published, or signaled).
  Else `repo.apply(actions).await`:
  `Err(e)` ⇒ `error!`, reply `Err(ExecuteError::Storage(e))`, memory
  untouched (decision 2). `Ok` ⇒ `interpret(actions, now)`; if changed ⇒
  `ArcSwap::store(Arc::new(app))` + broadcast `StateUpdated`; if the
  batch enqueued ops ⇒ forward `SyncNow` (decision 8); reply
  `Ok(outcome)`. A dropped reply receiver is ignored (caller stopped
  caring; outcome already durable).
- **Flush**: reply immediately — command-channel FIFO guarantees every
  previously accepted command was applied, interpreted, and published
  (its own reply already implied the same; Flush additionally settles
  no-reply `dispatch`es and is the seam Phase 5's exit path uses).
- **SyncReport::Completed { snapshot, pushes }**:
  `apply_sync_report(&core.app(), core.outbox(), &snapshot, &pushes,
  ids, now)` (read accessors on `EngineCore`) → `(app, actions)`;
  append `UpsertSyncStatus(app.sync)` iff `(phase, last_success)`
  changed vs current (decision 9); empty batch ⇒ nothing happened (no
  apply, no publish); else `apply` → on `Err`, `error!` and **discard
  the report** (state unchanged; the next cycle re-delivers the same
  server truth); on `Ok`, `interpret` + publish + signal as above.
- **SyncReport::Failed { kind }**: build `[UpsertSyncStatus(SyncStatus {
  phase: Failed { kind }, last_success: unchanged, .. })]`, apply →
  interpret → publish → signal (a failed cycle is an engine mutation:
  `last_updated` advances).
- **SystemEvent::NetworkLost**: phase → `Offline` via the same
  one-action path (persisted — decision 9). Idempotent: already-Offline
  is a no-action no-op.
- **SystemEvent::NetworkRestored**: forward `SyncNow` only — no phase
  change (the next report is the evidence; a self-declared "idle" would
  be a guess).
- **SystemEvent::Shutdown**: break the loop; `info!` + JoinHandle
  resolves (the in-flight message, if any, completes first — sequential
  processing makes shutdown clean by construction).

`EngineStartupError` (thiserror): `Load(RepositoryError)` — the boot
hydration failed; the Phase 5 bootstrap decides retry-vs-abort (out of
scope here). The initial `AppState` is swapped into the `ArcSwap`
*before* the loop starts (UI paints instantly from cache; no boot
signal — subscribers cannot have existed yet).

`select!` over the three receivers with command-biased polling
(`biased;` — deterministic drain order in tests when several channels
are simultaneously ready: commands, then reports, then system events).

### 5.3 Shutdown & lifetime

Dropping the last `EngineHandle` closes the command inbox but sync
reports/system events keep the loop alive by design (the engine is a
daemon, not a request-scoped actor); `SystemEvent::Shutdown` is the stop
signal. The returned `JoinHandle` lets Phase 5 await clean exit. Channel
closes on the injected receivers (`sync_reports` sender dropped =
Phase 4 actor died) are `warn!` + loop continues on the remaining
sources (a dead sync actor must not take the UI down).

## 6. Storage Adapter (`taskboard-storage-sqlite`, additive)

```rust
impl TaskRepository for StorageHandle {
    fn load(&self) -> BoxFuture<'_, Result<PersistedState, RepositoryError>>;
    fn apply(&self, actions: Vec<PersistenceAction>)
        -> BoxFuture<'_, Result<(), RepositoryError>>;
}
```

Both forward through the existing actor commands (oneshot round-trip;
send/recv failures map to `Unavailable`, matching the handle's existing
error mapping). ~30 lines. Routing behavior is already pinned at the
layer that owns it — Phase 2's actor tests (A1–A3) cover the command
round-trips, and the state-crate smoke I1 (§8) drives the adapter
end-to-end — so the impl itself gets one delegation test per result
class (`Ok`, `Unavailable`) through a real in-memory sqlite repo, per
the one-behavior-one-test rule. With the engine consuming `Arc<dyn
TaskRepository>` (decision 3), this impl is also what lets Phase 5 wire
the real repository without `taskboard-state` ever seeing the storage
crate.

## 7. Module Layout & TDD Implementation Order

```
crates/taskboard-domain/
├── src/
│   ├── command.rs    # NEW: plan_command + §4 catalogue, CommandOutcome/CommandError
│   ├── persistence.rs# + apply_actions (the shared action-application semantics)
│   ├── lib.rs        # re-export the above
│   └── test_support/mod.rs  # + CountingIds (promoted from merge_testutil)
crates/taskboard-state/
├── Cargo.toml        # deps unchanged; add dev-deps:
│                     #   taskboard-domain { workspace = true, features = ["test-support"] }
│                     #   taskboard-storage-sqlite (I1 smoke only; no uuid dev-dep —
│                     #   CountingIds comes from test-support)
└── src/
    ├── lib.rs        # crate docs (architecture, invariants), re-exports, forbid(unsafe)
    ├── core.rs       # EngineCore over PersistedState, interpret (delegates to apply_actions)
    ├── error.rs      # ExecuteError (Rejected/Storage/EngineGone), EngineStartupError
    └── actor.rs      # EngineCommand, EngineHandle, spawn_state_engine, loop
tests/ (taskboard-state)
├── snapshots.rs      # insta scenario suite (S-series)
├── engine.rs         # async black-box routing suite (A-series)
├── proptest.rs       # property suite (P2–P5)
└── sqlite_smoke.rs   # I1: engine + real sqlite actor through the adapter
```

Unit tests live inline (`#[cfg(test)]` in each module — the lowest
layer): the §4 catalogue and `apply_actions` in the domain, `interpret`
and engine plumbing in the state crate. The counting `IdGenerator` fake
is the domain's feature-gated `test_support::CountingIds`; the fixed
`Clock` fake stays local to the state crate (trivial, single consumer).

Branch: `feat/state-engine-actor`, one PR for the plan (roadmap §3
granularity). TDD order:

1. **Domain `persistence::apply_actions`**: extract the action
   semantics from the fake into the production function, unit-test per
   action kind + derivation (inline); refactor `InMemoryRepository` to
   delegate; re-run the Phase 2 contract harness + full domain suite
   green (behavior-preserving).
2. **Domain `command.rs` skeleton**: outcomes/errors first (compile +
   variant tests), then the catalogue one command family at a time,
   test-first each: tasks (create/update/done/move/delete) → stacks
   (create/rename/delete+cascade) → labels
   (create/update/delete/assign/unassign) → `RequestSync`
   (outcome-only arm). Every rejection and no-op gets its own test
   before its impl.
3. **Domain P1** (plan totality/consistency proptest) + promote
   `test_support::CountingIds`.
4. **`core.rs`**: `from_persisted` + `interpret` over the shared
   function — unit tests for stamping, projection, changed-detection
   (fixed clock).
5. **state `error.rs` + `proptest.rs` P2–P3** (the wiring and pipeline
   equivalences — these gate the architecture).
6. **`actor.rs`**: envelopes + handle + spawn; A-series routing tests in
   dependency order (reply → signal → nudge → flush → sync ingestion →
   system events → shutdown → failure revert).
7. **`tests/snapshots.rs`** S-series scenarios over the finished engine.
8. **P4–P5** (sequence coherence, op freshness) — need the full actor.
9. **§6 adapter** in `taskboard-storage-sqlite` + its delegation test +
   I1 smoke.
10. **Docs** (§10) + full quality gates + a local
    `cargo +nightly miri test -p taskboard-domain -p taskboard-state`
    verification (decision 16) + on-demand cargo-mutants pass.

## 8. Test Plan (catalogue)

Deterministic by construction: oneshot replies, broadcast `try_recv`
after a awaited reply, JoinHandles, and predicate polls with deadlines
are the only completion signals. Typed outcomes only. The fake repo is
`InMemoryRepository`; A9 adds a `FlakyRepository` wrapper (fails the
first N applies with `Unavailable`, then delegates — non-mutating on
failure, matching transactional reality).

- **U-series (pure, inline)**: in the domain — each §4 table row's
  effect/actions/outcome, each rejection variant, each no-op, the
  unbound-vs-bound delete split, the cascade (bound and unbound tasks
  mixed, sibling ops preserved), empty-changeset handling, `NoBoard`,
  and per-action `apply_actions` semantics + derivation; in the state
  crate — `interpret` stamping, projection, changed-detection.
- **P1 plan_command totality & consistency** (domain, proptest,
  `app_state_strategy` + an arbitrary-command strategy over a consistent
  outbox): for any (state, outbox, command), `plan_command` returns
  without panicking; an `Ok` plan is self-consistent — every enqueued
  op's target entity exists in the post-batch state, `NoOp`/rejection
  outcomes carry no actions — 256 cases.
- **P2 interpret ≡ repository wiring** (state, `persisted_state_strategy`
  + action batches): `EngineCore::from_persisted(s)` then
  `interpret(batch)` yields the same entity maps, outbox, validators,
  and derived `pending_ops` as `InMemoryRepository::with_state(s)` +
  `apply(batch)` + `load()`. Cheap by construction now (shared
  `apply_actions`) — it guards the projection/derivation/stamping
  wiring, not duplicated semantics.
- **P3 interpret ≡ pipeline** (state, `snapshot_strategy` +
  `push_outcome_strategy` over a consistent state): interpreting the
  pipeline's returned actions (+ the engine-appended status action when
  applicable) onto the pre-state reproduces the pipeline's returned
  `AppState` — the decision-2 redundancy pin.
- **P4 sequence coherence** (proptest, generated command sequences over
  a seeded board): after every command the engine's `persisted_view()`
  equals the fake repo's `load()` (memory ≡ disk), rejected/no-op
  commands changed neither, and the number of `StateUpdated` signals
  equals the number of state-changing messages (floors on invariants,
  never ceilings on success).
- **P5 op freshness**: across any sequence, every enqueued `OpId` is
  unique and every op addresses an entity that exists in the post-state
  (or is a delete addressed by plan construction).
- **S-series (insta, named scenarios)**: `assert_yaml_snapshot!` of
  BOTH the engine's `AppState` and the fake's `PersistedState` after:
  fresh-boot authoring (stack → task → edit → done → label → assign →
  move → delete); offline queue accumulation (many edits, no sync);
  sync ingestion (a `Completed` report binding the board and pulling
  remote entities); sync resolution (push outcomes completing +
  dead-lettering ops; `pending_ops` draining); rejection sequence
  (stale-id commands leave state and outbox byte-identical);
  network-lost/restored cycle (phase transitions). Snapshot files under
  `tests/snapshots/` (insta default naming).
- **A-series (async routing, one behavior each)**: A1 execute reply
  carries `CreatedTask` and the task is queryable via `shared_state`;
  A2 dispatch (no reply) still persists + signals (signal observed by
  `try_recv` after a `flush()`); A3 flush barrier settles dispatches;
  A4 nudge forwarded iff ops enqueued (assert on an owned
  `mpsc::Receiver<SyncCommand>`); A5 `Completed` ingestion publishes
  (entity + `last_success` visible, no reply expected); A6 `Failed`
  sets the typed phase; A7 `NetworkLost`/`Restored` (phase vs forward
  only); A8 `Shutdown` stops the loop (JoinHandle with a timeout
  deadline guard, not a sleep); A9 storage failure (FlakyRepository):
  reply is `Err(ExecuteError::Storage(Unavailable))`, published state
  unchanged, no signal, subsequent command succeeds against unchanged
  state; A10 execute on a shut-down engine →
  `Err(ExecuteError::EngineGone)`;
  A11 boot hydration publishes pre-loop (state visible before any
  command); A12 `EngineStartupError::Load` surfaces a failing boot
  repo.
- **I1 sqlite smoke**: `spawn_state_engine` over
  `spawn_storage_actor(SqliteTaskRepository::open_memory())` (through
  the §6 adapter): one command round-trips durably (second engine boot
  on the same *file-backed* db hydrates the task) — engine ↔ adapter ↔
  actor ↔ repo in one test, routing only.
- **Doc-tests** on `EngineHandle` and `spawn_state_engine` usage.

Coverage: the catalogue is the llvm-cov review target. cargo-mutants on
demand for domain `command.rs`/`apply_actions` and state `core.rs` (the
CI weekly pair already covers both crates); surviving mutants →
`.artifacts/reports/`.

## 9. Out of Scope → Backlog (append with this plan)

- **Write batching / pipelining** — batch multiple commands into one
  transaction or overlap applies with command processing (CLI script
  throughput; kiosk disk wear). Trigger: measured need post-M1.
- **Query-envelope variants** — wait-for-idle, outbox dump, validators
  read for Phase 4 (additive `EngineCommand` variants when their
  consumers' plans exist).
- **Storage-failure `SystemEvent`** — surface `ExecuteError::Storage`
  to the UI as a broadcast (currently: reply + log only). Bundle with
  Phase 5 error UX.
- **UI command forwarder** — the Phase 5 bootstrap task adapting a
  domain-typed `mpsc::Sender<StateCommand>` (UI-compatible, no
  state-crate dep) onto `EngineHandle::dispatch` (§12 Phase 7 row).
- **Engine metrics** — command/rejection/signal counters for the
  kiosk dashboard; with the observability backlog entry.
- **Debounced nudging** — engine-side sync-request coalescing if the
  Phase 4 actor's own dedupe proves insufficient.

## 10. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0006 — "The State Engine: single-advance interpret loop"**:
  memory advances only by interpreting persisted batches (apply →
  interpret → publish); revert-free failure semantics; transitions in
  the state crate vs conflict policy in the domain (decision 1's line);
  sync-status machine incl. the never-persisted `Syncing` transient;
  the port-adapter bridge to the storage actor; nudge contract with the
  Phase 4 actor. Status: Accepted.
- **architecture.org**: refresh the `taskboard-state` bullet (EngineCore,
  `EngineCommand` inbox, Flush barrier, sync-report ingestion, port
  consumption) and add the State-Engine rows to the channel table
  (Execute/Flush oneshot replies; SyncNow forwarding).
- **CHANGELOG**: `[Unreleased]` → Added: `taskboard-state` engine +
  the storage-crate port adapter.
- **Backlog**: append §9 entries via the file's entry template.

## 11. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`; `cargo
  clippy --workspace --all-targets -- -D warnings` (with
  `SQLX_OFFLINE=true`); `cargo nextest run --workspace`; `cargo test
  --doc --workspace`; `cargo deny check --workspace`; `cargo llvm-cov
  --workspace`.
- `cargo nextest run -p taskboard-state` green: U/P/S/A/I suites; the
  domain suite green under this phase's domain additions
  (`-p taskboard-domain --all-features`: new command module, shared
  `apply_actions`, delegated fake, `CountingIds` promotion — the Phase 2
  contract harness re-run is the behavior-preservation proof); storage
  suite still green after the §6 adapter.
- Local `cargo +nightly miri test -p taskboard-domain -p
  taskboard-state` verified (decision 16); cargo-mutants run recorded
  if mutants survive.
- **Phase 3 exit (roadmap)**: the snapshot suite defines the engine's
  contract — S-series scenarios reviewed as the behavioral spec (insta
  diffs read as contract changes).
- Downstream compile check: no code changes outside `taskboard-state`,
  the domain additions (`command.rs`, `apply_actions`,
  `test_support::CountingIds`, lib re-exports), the §6 adapter file +
  its test in `taskboard-storage-sqlite`, docs, and backlog.

## 12. Consumer Handoff Map (what later phases import)

| Consumer | Takes from Phase 3 |
|---|---|
| Phase 4 sync actor | `sync_out` receiver (`SyncCommand::{SyncNow, SetBoard}` — the nudge contract, decision 8), the `SyncReport` sender (Completed reaches `apply_sync_report` verbatim; **known gap, Phase 4's to close in its own PR**: validators must flow engine-ward for persistence, so Phase 4 should change `SyncReport` in the domain — new field or envelope variant — whichever its plan finds cleanest; `interpret` already handles `UpsertValidators`), outbox visibility via a future query variant (backlogged) |
| Phase 5 app bootstrap | `spawn_state_engine` wiring (repo/ids/clock + the three injected channel halves), `EngineStartupError`, `flush()` as the CLI exit barrier, `shared_state()` handed to the UI, the UI command forwarder duty (§9), `SystemEvent::Shutdown` as the engine stop |
| Phase 6 CLI | `execute()` receipts (`CreatedTask(id)` for scripting, typed `ExecuteError` with the semantic `CommandError` behind `Rejected` for exit codes), the sync-status watch pattern (predicate poll on `shared_state()` with deadline), `dispatch` never blocking |
| Phase 7 UI | `subscribe()` + `shared_state()` reads + dispatch-only writes — no `taskboard-state` dependency (architecture.org's UI dep list holds) |
| Everyone | the memory ≡ disk invariant and its P-series pins (any future writer must go through actions + interpret or extend the property) |

## 13. Risks & Mitigations

- **Interpret/pipeline drift** (the pipeline computes a state the engine
  ignores): pinned by P3 as a proptest equivalence, not by convention —
  a pipeline change that stops emitting a needed action fails P3 loudly.
- **Miri × tokio in routing tests**: decision 16's local verification +
  scoped `cfg_attr(miri, ignore)` escape (documented scope, not silent).
- **Apply-in-loop latency** (a slow disk write delays the next command /
  report): accepted MVP trade-off (decision 5) — local sqlite applies
  are sub-ms; the backlog owns pipelining; the UI never blocks
  (fire-and-forget dispatch).
- **Nudge storms** (one `SyncNow` per mutating command pre-coalescing):
  the Phase 4 actor's cycle-in-flight dedupe is the designed absorber;
  engine-side debouncing backlogged if measurements demand it.
- **Error surface split** (7 semantic `CommandError` variants in the
  domain + 3 `ExecuteError` classes in the engine): every variant is
  matched-and-typed somewhere (tests, future CLI exit codes); merging
  into stringly or catch-alls would trade AGENTS §5 compliance for
  nothing. Watch at review.
- **Shared `apply_actions` couples the fake to production semantics**
  (the fake is no longer an independent oracle): acceptable because the
  semantics are unit-tested inline where they now live, the Phase 2
  contract harness keeps pinning sqlite against those semantics, and
  the coupling is the point — one executable definition of the port
  contract instead of two copies that can drift.
- **Snapshot churn discipline**: insta diffs are contract changes —
  `cargo insta review` per scenario, never blind `--accept-all`; the
  S-series names read as the spec's section titles.
- **Fixed-clock equal instants** masking publish-changes (decision 11's
  granularity note): P3's signal-count property runs under an advancing
  fake clock precisely to expose same-instant cases.
