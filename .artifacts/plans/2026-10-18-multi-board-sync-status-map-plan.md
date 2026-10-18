# [Plan] Multi-board Chunk 1 — SyncStatus Per-Board Map + Migration 0006 (Whole)

- **Date**: 2026-10-18
- **Kind**: stage-1 TDD-first work plan (dispatch plan §2, prompt 1.1) for
  chunk 1 of the multi-board implementation round, branch
  `feat/state-sync-status-map`.
- **Origin** (normative rank per dispatch MD-7): implementation plan
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md`
  §2 (DF-1 status slice, DF-3, DF-4, DF-9, DF-10) + §3 chunk 1 + §4/§5/§6;
  design `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`
  §3.2 (option (a), D-W9 ruled; wire-compat consequence), §3.9 (0006
  sketch, decode home, snapshot-regeneration rule, rollback story), §3.3
  (`ConnectorId` minting/typing); dispatch plan §1/§2 (covers,
  acceptance, traps).
- **Status**: READY for stage 2. Every open question the prompt named is
  settled by a recorded decision (§3, C1–C11); two deviations from
  plan-prose are recorded with reasons (C4 decode-home mechanics, C7's
  `boards.rs`/`output.rs` file-list corrections) — neither touches an
  ACCEPTED §5 decision.

## 1. Scope

**In** (impl plan §3 chunk 1): migration
`0006_multi_board_sync_status.sql` **whole** — (a) `boards.connector TEXT
NULL` and (b) the `sync_status` rebuild to a per-board PK with the
legacy-singleton backfill into the ≤1 bound live board, no seed,
board-less singleton dropped; `AppState.sync` →
`SyncStates(BTreeMap<BoardId, SyncStatus>)` with `get`/`aggregate`;
global `last_updated` **unchanged**; `ConnectorId` newtype; decode/write
home for the connector tag in the storage crate; engine mutators
re-keyed (`mark_syncing(board)`, ingest by the report's board,
`go_offline` over bound boards); app + UI surfaces keep their shapes via
the `aggregate()` fold; **snapshot pass 1** (the 20-file insta corpus).

**Non-goals** (restated, binding): no plural connectors (chunk 5); no
`Board.remote` widening — it stays `Option<RemoteBoardId>` until chunk 4
(the codec writes the spawn-side default id, never a domain field); no
`StateCommand` shape changes (chunk 2); no `SyncCommand` changes (chunk
4); no `[[sync.connectors]]` config (chunk 5); no per-board surface
rendering (chunk 6) or selected-board badge read (chunk 7); no kebab
validation of connector names (chunk 5's config load, per DF-3).

## 2. Verified baseline (symbols re-located 2026-10-18; paths are the stable keys)

- **Domain**: `AppState.sync: SyncStatus` + `SyncStatus { phase,
  last_success, pending_ops }` + `SyncPhase` + `SyncErrorKind`
  (`crates/taskboard-domain/src/state.rs`); id newtype macros
  (`ids.rs` — `local_id!` wraps `Uuid` (Copy), `remote_id!` wraps `u64`);
  `PersistedState.sync: SyncStatus` + `From<PersistedState> for
  AppState` (`persistence.rs`); `PersistenceAction::UpsertSyncStatus(
  SyncStatus)` with the apply arm setting phase + `last_success` only
  (`persistence.rs`, `apply_actions`) and the closing derivation
  recomputing `sync.pending_ops` from outbox depth; the two pure
  pipelines `apply_sync_report` / `apply_push_report`
  (`pipeline.rs` — each constructs the merged `sync` singleton with
  `pending_ops = remaining_ops`, `last_success = Some(now)` /
  carry-over-on-rejection); `SyncReport::Completed { snapshot:
  Box<RemoteBoardSnapshot>, .. }` / `Failed { kind, pushes, read_at }`
  (`messages.rs` — no board field; `RemoteBoardSnapshot.board.id` is the
  remote identity); `LocalOp` entity-addressed ops (`outbox.rs`);
  `InMemoryRepository` (`test_support/memory.rs` — `load` derives
  `pending_ops` from outbox depth; `apply` delegates to `apply_actions`).
- **State engine**: `EngineCore { state: PersistedState,
  last_updated }`, `app()`, `sync_status()` getter, `mark_syncing()`
  memory-only transient (`core.rs`); actor mutators
  `execute_command` (RequestSync arm marks syncing; post-apply nudge
  marks syncing when ops enqueued), `ingest_sync_report` (Completed →
  `apply_sync_report` + `append_status_if_changed`; Failed-no-pushes →
  one direct `UpsertSyncStatus`; Failed-with-pushes →
  `apply_push_report` + `append_status_if_changed`),
  `append_status_if_changed` (compares phase/`last_success` only),
  `go_offline` (idempotence guard on `phase == Offline`, one
  `UpsertSyncStatus(Offline)`) (`crates/taskboard-state/src/actor.rs`).
- **Storage**: `sync_status` table `CHECK (id = 1)` + idle seed
  (`migrations/0001_init.sql`, conventions block: UUID TEXT ids, chrono
  TEXT timestamps never in ORDER BY, no triggers, logic in Rust); 0005's
  in-migration re-tag precedent + its round-trip pin
  `migration_0005_upgrades_a_pre_edit_verb_database`
  (`crates/taskboard-storage-sqlite/src/connect.rs` — run shipped
  migrations, strip the newest migration's effects + bookkeeping row,
  seed legacy rows, re-`open`, assert decode); `open(path)` /
  `open_memory()` construct `SqliteTaskRepository { pool }` after running
  the embedded `MIGRATOR` (`connect.rs`); load's singleton read
  `WHERE id = 1` with `.ok_or(RepositoryError::Corrupted)?` and
  `pending_ops` derived from hydrated outbox (`repo.rs`, `load_inner`);
  the status upsert loop `INSERT … VALUES (1, …) ON CONFLICT(id)` and
  the board upsert (`repo.rs`, `execute_batch`); `spawn_storage_actor(
  repo: Arc<SqliteTaskRepository>)` (`actor.rs`); `TaskRepository` port
  adapter on `StorageHandle` (`port.rs`).
- **App surfaces**: `StatusView::server` feeds
  `SyncStatusView::new(&…load_full().sync)` (`commands/status.rs`);
  `SyncStatusView::new(&SyncStatus)` — a projector over one status
  (`commands/output.rs`); `sync` verb wait decision table +
  `wait_for_sync_result` compare on whole `SyncStatus` values
  (`commands/sync.rs`); `boards.rs` wait loops + `SyncStatusView::new(
  &state.sync)` at the bind/reset ceremonies (`commands/boards.rs` —
  three sites); `wait_for_boot_pull` reads `state.sync.phase` /
  `.last_success` (`bootstrap.rs`); the storage spawn
  `spawn_storage_actor(repo)` right after `open(&db_path)`
  (`bootstrap.rs`).
- **UI**: `sync_badge(&SyncStatus) -> SyncBadge` (`presentation.rs`);
  caller `models.rs` (`sync_badge(&state.sync)` + recency caption over
  `state.sync.last_success`); test fixture mutating `state.sync.phase`
  (`lib.rs`).
- **Corpora that must not break**: state-engine insta S-series s1–s10 ×
  {`persisted_state`, `published_app_state`} = 20 snaps
  (`crates/taskboard-state/tests/snapshots/`); storage suites
  (`tests/roundtrip.rs`, `tests/contract.rs` — fake-vs-sqlite parity
  over `apply_actions`, `tests/actor.rs`, `tests/fixture_hygiene.rs`,
  `tests/bench_fixtures.rs`, `benches/hot_path.rs`); state
  `tests/sqlite_smoke.rs` (asserts `hydrated.sync.pending_ops == 2`);
  domain pipeline/persistence unit suites; `open`/`open_memory` has ~46
  call sites workspace-wide (blast radius of C4).
- **Verified absent**: the sync actor never reads `persisted.sync` (the
  `SyncStateReader` contract forbids branching on `sync.phase` —
  `persistence.rs` doc); no IPC/CLI code reads `.sync` outside the
  surfaces above; `SyncCommand::SetBoard` exists and stays (chunk 4).

## 3. Recorded decisions

### C1 — Final 0006 SQL (design §3.9's sketch, assumptions verified)

The sketch's assumptions hold against the live schema: `boards` has
`remote_board_id INTEGER NULL` and `deleted INTEGER`; the legacy table is
`sync_status (id INTEGER PK CHECK (id = 1), phase, last_error,
last_success)` with the `'idle'` seed (0001:102–109); sqlx runs
multi-statement files in one per-file transaction (0005 precedent) and
the house has no down-migrations. Final file
`crates/taskboard-storage-sqlite/migrations/0006_multi_board_sync_status.sql`,
design-verbatim apart from the two noted tailoring points:

```sql
-- 0006_multi_board_sync_status: the SyncStatus singleton becomes a
-- per-board row (F16-22 three-layer widening, wave chunk 1); board
-- bindings gain their connector tag (connect-flow §4.5).
-- Convention notes: timestamps stay chrono TEXT (never ORDER BY);
-- connector attribution is NOT stored in sync_status (derived from the
-- board's binding at read — one source of truth); no triggers — the
-- backfill is plain SQL inside the file's transaction.

-- 1. Binding connector tag: NULL = the legacy default connector
--    (resolved at decode time in the repository; see C4). Pre-migration
--    rows keep their behavior bit-for-bit: the one binding belonged to
--    the one configured connector, which is the default.
ALTER TABLE boards ADD COLUMN connector TEXT NULL;

-- 2. Status rebuild: one row per bound board. A v0.1.x database has at
--    most one bound live board (the single-board invariant), so the
--    singleton's phase/last_success backfills that board's row; a
--    board-less singleton (standalone installs) is dropped — its
--    content is vacuous (the recorded, priced deviation from 0005's
--    bit-for-bit bar, which governs merge behavior, not status
--    cosmetics).
CREATE TABLE sync_status_new (
    board        TEXT PRIMARY KEY NOT NULL REFERENCES boards (id),
    phase        TEXT NOT NULL,
    last_error   TEXT NULL,
    last_success TEXT NULL
);
INSERT INTO sync_status_new (board, phase, last_error, last_success)
SELECT b.id, s.phase, s.last_error, s.last_success
  FROM boards b
  JOIN sync_status s ON s.id = 1
 WHERE b.remote_board_id IS NOT NULL AND b.deleted = 0;
DROP TABLE sync_status;
ALTER TABLE sync_status_new RENAME TO sync_status;
```

Tailoring vs the sketch (both cosmetic, recorded): column alignment
only; the sketch's `board TEXT PRIMARY KEY NOT NULL REFERENCES boards
(id)` kept verbatim (0001 writes FKs inline on `stacks`/`labels` the
same way). No seed follows the rebuild — a fresh install loads an empty
map (0001's seed dies with the dropped table). Bound-but-deleted boards
are excluded by the backfill's `deleted = 0` — same priced class as the
standalone drop (their singleton content is unreachable status
cosmetics). `connector` has no CHECK/constraint: validity is chunk 5's
config-load concern; NULL is the legacy relic marker per DF-4.

### C2 — `SyncStates` newtype: exact API + the aggregate fold rule

Lives in `crates/taskboard-domain/src/state.rs` next to `SyncStatus`:

```rust
/// Per-board sync status, keyed by local board id. Rows exist for every
/// board the state knows (the derivation materializes defaults); the
/// connector attribution is derived from bindings, never stored here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SyncStates(BTreeMap<BoardId, SyncStatus>);

impl SyncStates {
    #[must_use] pub fn new() -> Self;                    // empty
    #[must_use] pub fn get(&self, board: BoardId) -> Option<&SyncStatus>;
    #[must_use] pub fn aggregate(&self) -> SyncStatus;   // the fold, below
    pub fn iter(&self) -> impl Iterator<Item = (&BoardId, &SyncStatus)>; // tests + chunk 6
    fn set_stored(&mut self, board: BoardId, phase: SyncPhase,
                  last_success: Option<DateTime<Utc>>);  // pub(crate): UpsertSyncStatus arm + mark path
}
```

- Derives mirror the domain style; **not** `Copy` (heap map). `Eq` holds
  (`SyncStatus: Eq`). `#[serde(transparent)]` serializes as the plain
  map — `AppState`'s YAML snapshots render `sync: {<uuid>: {…}}`
  deterministically (BTreeMap) and `PersistedState`'s serde round-trip
  keeps working.
- **Aggregate fold rule** (the back-compat contract; pinned by unit +
  proptest):
  - `phase`: precedence **`Syncing` > `Failed` > `Offline` > `Idle`**;
    `Failed` ties break by lowest `BoardId` (BTreeMap order —
    deterministic). Empty map → `SyncPhase::default()` (`Idle`).
    Rationale: an in-flight cycle is the operationally dominant fact —
    `wait_for_boot_pull`, `wait_for_sync_result`, and the boards.rs
    wait loops key on `Syncing` to detect settle, so a `Failed`
    aggregate while another cycle is mid-flight would terminate a wait
    with stale evidence; `Failed` outranks `Offline` because an
    actionable failure is more informative than an environmental one.
  - `last_success`: `max` over rows (`None` when no row has one).
  - `pending_ops`: **saturating sum** over rows.
- In every one-board world (the whole of chunk 1: `CreateBoard` still
  refuses a second live board) the fold over the single row is today's
  singleton bit-for-bit; the empty-map fold equals
  `SyncStatus::default()` — the fresh-install/standalone display today.

### C3 — `ConnectorId` newtype (derives, serde, validation split)

`crates/taskboard-domain/src/ids.rs`, hand-written (not the `local_id!`
macro — it wraps a `String`, not a `Uuid`, and must stay `Copy`-free):

```rust
/// Config-minted connector name (`[[sync.connectors]]` key; the legacy
/// scalar synthesizes "nextcloud") — a human-meaningful name, not a
/// UUID. Kebab-case + uniqueness are validated at config load (chunk 5).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConnectorId(String);

impl ConnectorId {
    #[must_use] pub fn as_str(&self) -> &str;
}
impl From<&str> for ConnectorId;   // and From<String>
impl std::fmt::Display for ConnectorId;
```

`Ord`/`Hash` now (chunk 5 registry keys need them; deriving costs
nothing and avoids a breaking derive-add later). Serde serializes as the
plain string (tuple-struct default). **No validation lands in chunk 1**
— DF-3 assigns kebab + uniqueness to config load as a typed
`ConfigError` (chunk 5); a constructor that cannot fail here also keeps
`From` total. `Board.remote` stays `Option<RemoteBoardId>` (non-goal
fence).

### C4 — Decode-home plumbing (recorded deviation: the parameter lives on `open`, not `spawn_storage_actor`)

Design §3.9 / DF-4 / impl plan all name `spawn_storage_actor`'s
parameter as the resolution home. Structurally the connector is consumed
inside the repository, not the actor:

- the **write** side (`execute_batch`'s board upsert writes non-NULL)
  runs behind the `TaskRepository` port (`repo.apply(actions)`), whose
  trait signature cannot carry the connector, and the action itself
  (`UpsertBoard(Board)`) has no field for it in chunk 1 (the domain
  binding widening is chunk 4's);
- the **read** side (`decode` NULL → default) runs inside
  `load_inner`, same constraint.

Threading a spawn-time parameter into those paths would need either
interior mutability (a set-once `OnceLock` dance + `expect` on every
decode) or the actor rewriting action batches — both worse than the
constructor. **Decision**: `SqliteTaskRepository` gains a
`default_connector: ConnectorId` field, and the constructor takes it —
`open(path, default_connector)` / `open_memory(default_connector)`
(`connect.rs`); `spawn_storage_actor`'s signature is **unchanged** (the
actor adds no policy; its doc contract says exactly that). Blast radius:
~46 mechanical `open`/`open_memory` call sites (tests pass a fixed id
constant; bootstrap passes C10's value). The design's actual invariants
all hold and are pinned by tests: **one** resolution home (the
repository, config-derived at bootstrap), NULL decodes as the default,
every upsert writes non-NULL, and **no boot-time UPDATE pass** (the
rejected alternative — legacy NULLs are read-only relics until their
board's next upsert). Recorded as a prose-mechanics deviation under
dispatch MD-7; the ACCEPTED row D-W9 (option (a) keying) is untouched.

### C5 — Bootstrap derivation of `default_connector`

Chunk 1 has no connector config. Per DF-3's mint-chain head, the legacy
scalar world has exactly one connector name: `"nextcloud"` — it is the
default for a resolved `nextcloud` backend **and** the inert tag written
for standalone installs' boards (unbound boards' tags are never read:
attribution derives from bindings). **Decision**: `bootstrap.rs`
computes `let default_connector = ConnectorId::from("nextcloud");`
unconditionally and passes it to `open(&db_path, default_connector)`.
Chunk 4's synthesis-of-one refines the derivation (resolved
`standalone` ⇒ zero connectors; tags sourced from the domain field);
chunk 5 adds validation. Recorded so stage 3 does not read the
standalone tag as plural-connector scaffolding.

### C6 — Per-board `pending_ops` derivation: one shared projection, rows materialized per board

Today `pending_ops` is global outbox depth, derived after every batch
(`apply_actions`), at sqlite `load`, and at the memory fake's `load`.
The wave's honest-interim signal (chunk 2's "per-board `pending_ops`
climbs on the unsynced boards") requires unbound boards' pending depth
to be **visible** — but the SQL table only ever holds rows for boards
the engine has written transitions for (C1 backfills the bound one;
"No seed"). **Decision**: one pub domain function is the single
derivation home, called by all three consumers:

```rust
/// Recomputes the per-board sync projection: materializes a default
/// row for every board, then sets each row's pending_ops to the count
/// of outbox ops attributed to that board through the entity graph.
/// Stored phase/last_success are preserved; pending_ops is never
/// stored (the outbox is the single source of truth).
pub fn derive_sync_states(
    boards: &BTreeMap<BoardId, Board>, stacks: …, tasks: …, labels: …,
    outbox: &[PendingOp], sync: &mut SyncStates,
);
```

- **Attribution**: `UpdateBoard(b)`/`DeleteBoard(b)` → `b`; stack ops →
  the stack's board; task ops (incl. `MoveTask`/`AssignLabel`/
  `UnassignLabel`) → the task's stack's board; label ops → the label's
  board — the same filter `read_scope` implements app-side
  (`commands/boards.rs`), now in the domain where the derivation lives.
- **Materialization**: every board in `boards` gets a row (default
  `SyncStatus::default()` when none stored). This preserves today's
  display bit-for-bit in *every* one-board world, including
  standalone-with-queued-ops (today the singleton shows depth N; a
  rows-only-for-bound-boards map would show 0 — a silent regression and
  a quality-ceiling violation). Tombstoned boards keep rows (their
  `DeleteBoard` op counts to them; tombstones are retained forever).
  The projection is a **display derivation, like `pending_ops` today** —
  materialized rows are never written to SQL.
- `apply_actions` calls it after every batch; `repo.rs`'s `load_inner`
  calls it on the hydrated state (replacing the singleton read + global
  depth); `InMemoryRepository::load` calls it (parity).
- **Proptest pin**: for generated well-formed states, every outbox op
  attributes to exactly one board and the per-board counts sum to the
  outbox depth (the no-ceiling property: no op is silently uncounted).

### C7 — Surface-by-surface aggregate adoption list (exhaustive; two file-list corrections)

| Surface | Change |
|---|---|
| `commands/status.rs` | `StatusView::server`: `SyncStatusView::new(&state.sync.aggregate())`. JSON byte-shape unchanged for the single-board install (the D-W6 wire-compat pin). |
| `commands/output.rs` | **No change** (recorded correction to the impl file list): `SyncStatusView::new(&SyncStatus)` is a single-status projector and stays; the adoption is at its call sites. |
| `commands/sync.rs` | `run`'s startup snapshot + `wait_for_sync_result`: compare/project `state.sync.aggregate()` (whole-value `SyncStatus` comparisons keep their decision-table semantics unchanged). |
| `commands/boards.rs` | **Added to the adoption list** (compile-forced; absent from the impl file list): the bind/reset ceremony wait loops and two `SyncStatusView::new(&state.sync)` sites read `state.sync.aggregate()`. |
| `bootstrap.rs` | `wait_for_boot_pull` matches on `state.sync.aggregate().phase/.last_success`; passes C5's `default_connector` into `open` (C4). |
| ui `presentation.rs` | `sync_badge(&SyncStates)` — reads the fold (impl plan's wording); internally aggregates. Signature change ripples to `models.rs` (`sync_badge(&state.sync)` stays call-compatible; recency caption reads `aggregate().last_success`), the `presentation.rs` unit tests, and the `lib.rs` test fixture. Selected-board read is chunk 7's. |

### C8 — Mutator re-keying plan (state crate)

- `EngineCore::sync_status(board: BoardId) -> Option<&SyncStatus>`
  (getter re-keyed; `None` for an unknown board is the caller's
  no-row case).
- `EngineCore::mark_syncing(board: BoardId) -> bool` — same memory-only
  transient contract (never persisted, idempotent per board), now
  per-row.
- **Which boards the actor marks**: chunk 1 has no board-scoped commands
  (chunk 2), so both nudge sites derive the set engine-side — new pure
  helper `AppState::bound_boards() -> Vec<BoardId>`
  (`remote.is_some()`, **regardless of `deleted`** — a tombstoned
  board's finalizing push is a sync fact; deterministic BTreeMap
  order). `RequestSync` marks every bound board; the post-apply nudge
  (ops enqueued) marks every bound board. In chunk 1's world (≤1 board,
  ≤1 bound) this is today's semantics bit-for-bit, and it avoids
  per-command board plumbing chunk 2 immediately rewrites.
- **Ingest re-key** (`ingest_sync_report`):
  - `Completed`: the report's board is `snapshot.board.id` resolved
    through the post-merge bindings (the pipeline already computes
    `board_id`); `apply_sync_report` writes that board's row transition
    into the cloned map, then runs `derive_sync_states` over its final
    maps + the outbox param (pending values there are advisory — the
    engine publishes from `core.app()` post-`interpret`, where the
    authoritative derivation ran; the merged map feeds only
    `append_status_if_changed`'s phase/`last_success` comparison).
    `append_status_if_changed(board)` appends
    `UpsertSyncStatus(board, …)` only when that board's stable part
    changed.
  - `Failed { pushes: non-empty }`: `apply_push_report` has no snapshot;
    the boards come from the pushes' ops via the same attribution
    helper; those boards' rows get the `Failed` transition.
  - `Failed { pushes: empty }`: no board identity exists anywhere in
    the report (`SyncReport` is unchanged — a non-goal). **Recorded
    chunk-1 simplification**: mark **every bound board** `Failed { kind
    }`, each carrying **its own** row's `last_success`. Chunk 1's world
    makes this exact (≤1 bound board); chunk 4's per-connector
    attribution scopes it to the reporting connector's boards (the
    successor is named, not built here). Corollary, priced like C1's
    singleton drop: a board-less failure (`SyncErrorKind::NoBoard`,
    pre-bind cycles) vanishes from the map — aggregate stays `Idle`.
    The design already prices board-less singleton content as "not
    behavior worth preserving"; recorded so stage 3 does not read it as
    a regression.
- **`go_offline`**: derives `bound_boards()`; appends one
  `UpsertSyncStatus(board, Offline, own last_success)` per bound board
  **not already `Offline`** (per-board idempotence; the batch is empty
  iff every bound board is already offline → no-op, matching today's
  guard). `NetworkLost` with zero bound boards writes nothing
  (aggregate stays `Idle` — same priced class as above; unreachable in
  standalone anyway, since only the sync actor emits `NetworkLost`).
- `ClearBoardContent` semantics unchanged — the board row survives, so
  its status row (stored or materialized) survives with it; its
  per-board pending drops to 0 when the ops are cleared (derivation).

### C9 — Action + persistence re-key

`PersistenceAction::UpsertSyncStatus(BoardId, SyncStatus)` (carrying
phase + `last_success`; `pending_ops` still ignored/stored-never).
`apply_actions`' arm sets that board's stored part; the C6 projection
runs as today's closing derivation. Storage's upsert loop becomes
`INSERT INTO sync_status (board, phase, last_error, last_success)
VALUES (?, ?, ?, ?) ON CONFLICT(board) DO UPDATE …` keyed by the board
uuid text; `load_inner`'s singleton read (`WHERE id = 1` +
`.ok_or(Corrupted)`) is replaced by a whole-table read
(`SELECT board, phase, last_error, last_success FROM sync_status`,
decode uuid per row → malformed uuid stays `Corrupted`; **zero rows is
success**, the empty map) followed by the C6 projection (which also
replaces the global depth line). The fake, the sqlite repo, and
`apply_actions` share the one projection — `tests/contract.rs`'s parity
suite enforces the lockstep. `SyncStatusRow` gains `board: String`.
Boards SELECT/decode ignore the `connector` column entirely in chunk 1
(no domain consumer; selecting-but-ignoring would be dead reads) — the
repo's `default_connector` field exists now for the write side and
chunk 4's decode consumes it.

### C10 — Round-trip migration test design (mirrors the 0005 pin's shape)

New test in `crates/taskboard-storage-sqlite/src/connect.rs`'s test
module, next to `migration_0005_upgrades_a_pre_edit_verb_database`,
same construction recipe (shipped migrations → strip 0006's effects:
`ALTER TABLE boards DROP COLUMN connector`, drop + rebuild the legacy
`sync_status (id CHECK(id=1), …)` with its seed row, restore the
singleton content, `DELETE FROM _sqlx_migrations WHERE version = 6` →
seed rows → re-`open` → assert):

1. **Bound-board backfill**: one live bound board (`remote_board_id`
   set, `deleted = 0`), singleton at a non-idle phase (e.g. `offline`,
   `last_success` set) → after `open`: `load` succeeds; the board's
   row decodes with the singleton's phase + `last_success` (typed
   equality, no text); the map has exactly that one stored row.
2. **Singleton gone**: SQL-level `SELECT` for the legacy shape fails
   typed / `PRAGMA table_info(sync_status)` shows the `board` key and
   no `id` column (structural assertion, not text).
3. **Standalone drop**: a db with the seeded singleton and **no** bound
   live board → `open` + `load` succeed with an empty stored map
   (aggregate `Idle` — today's fresh-hydrate world).
4. **Legacy NULL tag is a read-only relic**: the migrated board row's
   `connector` is NULL (SQL assert); after an `UpsertBoard` round-trip
   through `apply`, the column is non-NULL and equals the repo's
   `default_connector` (the DF-4 write rule pinned end-to-end).
5. **Fresh install**: new db, no seeds → empty map, `load` OK (the
   `Corrupted`-on-missing-singleton path is dead).

Plus the storage-suite companions: per-board upsert/load round-trip
(two stored rows survive reload), and `tests/contract.rs` parity
extends to the re-keyed action (fake vs sqlite agree on two boards).

### C11 — Snapshot pass 1: catalogue + review rule

- **Catalogue**: all 20 snaps regenerate —
  `snapshots__s{1..10}_{persisted_state,published_app_state}.snap`
  (`crates/taskboard-state/tests/snapshots/`). Every file's `sync:`
  field changes shape (singleton object → board-keyed map; seeded
  scenarios with a board show the materialized row; s6's offline cycle
  shows the bound board's `offline` row; s2's queue accumulation shows
  the climbing per-board count). The command sequences, outcomes, and
  every non-`sync` field in each snap are code-driven and must be
  byte-identical.
- **Rule for the accepted diff** (impl plan's wording, made concrete):
  per file, the diff must contain **only** the `sync:` value's
  shape change; any drift in commands, entities, outbox ops, reminders,
  validators, or `last_updated` is a finding (a value change that is
  not the map widening). One deliberate `cargo insta accept` pass
  inside the switch commit; **no `.pending` snapshots ship**; the pass
  is reviewed file-by-file against this catalogue before handoff.
- Compile-adjacent (not snapshot) churn, same commit: `seed_board_state`
  keeps using `PersistedState::default()` (empty map), pipeline/persistence
  unit asserts on `.sync.pending_ops`/`.sync.phase` become
  `.sync.get(&board)…` or `.sync.aggregate()…` per what each test pins
  (semantics preserved, typed outcomes kept), `sqlite_smoke`'s
  `hydrated.sync.pending_ops == 2` becomes the aggregate/per-board
  read, `test_support` fixtures follow the new shapes.

## 4. TDD-first implementation sequence (commits; each standalone-green)

1. `feat(domain): add ConnectorId newtype and SyncStates aggregate map`
   — C3 + C2 types + `derive_sync_states`/attribution helper (C6) +
   `AppState::bound_boards` + their unit/proptest suites. Purely
   additive; nothing consumes them yet; all gates green.
2. `feat(state): key sync status per board with migration 0006` — **the
   switch, one motion** (the workspace cannot compile half-switched):
   0006 SQL (C1); `AppState.sync`/`PersistedState.sync` → `SyncStates`;
   `UpsertSyncStatus(BoardId, …)` + `apply_actions` arm + projection
   (C6, C9); pipelines' per-board transitions (C8); `EngineCore`
   getters/`mark_syncing(board)` + actor mutators (C8); storage
   `open(…, default_connector)` + load/upsert re-key + `SyncStatusRow`
   (C4, C9); bootstrap C5 + `wait_for_boot_pull`; app surfaces (C7);
   ui `sync_badge(&SyncStates)` + models; all affected tests updated
   (C10's new pins land here, written first); the **one deliberate
   insta accept** (C11); CHANGELOG `Unreleased` line. Full gate suite
   green.
3. (Optional, only if review granularity wants it)
   `test(storage): tighten per-board status parity` — additional
   contract-parity cases; folded into commit 2 if it stays small.

TDD order within each commit: the failing typed-outcome tests are
written first in the working tree (C10's migration pin before the SQL;
the fold/attribution proptest before the impl; the engine re-key pins
before the mutator edits), then the production code, then the single
insta pass. House rules throughout: typed outcomes (no text asserts),
no sleeps (existing poll-with-deadline seams unchanged), fakes over
mocks, one behavior one test at the lowest owning layer (the fold is
pinned in the domain, not through the engine; the migration pin in the
storage crate; the surfaces keep their existing routing-level tests).

## 5. Test matrix (typed outcomes per AGENTS §5)

- **Domain (new)**: `SyncStates::aggregate` precedence/tie-break/max/
  saturating-sum/empty-default (unit); aggregate determinism + sum
  conservation over generated maps (proptest); attribution — every op
  of a well-formed generated state attributes to exactly one board and
  counts sum to depth (proptest, the no-ceiling pin);
  `derive_sync_states` materializes rows for boards without stored
  rows, preserves stored phase/`last_success`, recomputes pending
  (unit, two-board fixtures seeded directly — engine-possible shapes);
  `apply_actions` `UpsertSyncStatus(board, …)` touches only that
  board's stored part (unit); `apply_sync_report`/`apply_push_report`
  update only the report board's / pushed boards' rows, other boards'
  rows unchanged (unit, two-board seeds);
  `AppState::bound_boards` ordering + tombstoned-bound inclusion
  (unit); `ConnectorId` serde round-trip + `as_str`/`Display` (unit).
- **Domain (updated)**: `persistence.rs` serde round-trip
  (`PersistedState` with a populated map); `state.rs` serialization
  determinism; pipeline suites re-keyed per C11 (typed equality on
  rows/aggregate, no text).
- **Storage (new)**: C10's five migration pins + per-board upsert/load
  round-trip + contract parity over two boards + DF-4 write rule
  (upsert writes non-NULL default; NULL relic survives untouched reads).
- **Storage (updated)**: `roundtrip.rs`/`contract.rs`/`actor.rs`/
  `port.rs`/`fixture_hygiene.rs`/`bench_fixtures.rs`/`benches` call
  sites + any singleton-shaped asserts re-keyed.
- **State engine (new)**: `RequestSync` marks every bound board
  `Syncing` (memory-only: no `UpsertSyncStatus` in the repo's action
  log — the existing fake-log seam); post-enqueue nudge same;
  `go_offline` marks every bound board `Offline`, per-board idempotent
  (second `NetworkLost` is a no-action no-op), zero bound boards ⇒ no
  actions; `Failed`-report ingest marks bound boards with own-row
  `last_success` carry-over; `Completed`-report ingest re-keys only the
  report's board (two-board seed: the other board's row untouched).
- **State engine (updated)**: S-series snapshot regeneration (C11);
  `sqlite_smoke` per-board/aggregate reads; engine.rs routing tests
  compile-shape updates only.
- **Surfaces (updated, routing-level only)**: `StatusView::server`
  byte-shape pin (existing serialization test) keeps passing on the
  aggregate path; `sync`/`boards` wait-table tests keep their typed
  `SyncOutcome` outcomes; ui `sync_badge` maps every phase (existing
  unit, re-keyed to the map input) + `token_tests`/fixtures compile.
- **No upper bounds anywhere**: the only ceilings are on garbage input
  (malformed uuid → `Corrupted`), floors elsewhere.

## 6. Gates (AGENTS §10, run by stage 2 and re-run by stage 3)

`cargo fmt --all -- --check` → `cargo clippy --workspace --all-targets
-- -D warnings` → `cargo nextest run --workspace` → `cargo test --doc
--workspace` → `cargo deny check` (zero new deps — `ConnectorId` uses
nothing new) → the CI-identical three-command llvm-cov sequence +
quarantine script → `lcov.info` → `rm -rf target/llvm-cov-target`.
SPDX headers on touched files (storage/domain/state: MIT OR
Apache-2.0; app/ui: AGPL-3.0-only); `#![forbid(unsafe_code)]` holds
outside ui.

## 7. Risks & recorded interactions

- **The decode-home deviation (C4)** is the likeliest stage-3 flag: the
  traps text says "resolution home is only spawn_storage_actor's
  parameter". The plan records the structural reason (trait seam,
  constructor enforcement, single home, no boot UPDATE pass — every
  invariant the design actually states is pinned by a test). If the
  reviewer still rules it a deviation too far, the fallback is the
  `OnceLock`-on-repo variant with `spawn_storage_actor` taking the
  parameter verbatim; the tests in C10 are written against behavior, so
  they survive either plumbing.
- **Materialized rows (C6)** are design-grey (the SQL table stays
  bound-rows-only per C1; the domain projection completes rows). They
  are load-bearing for chunk 2's honest interim (unsynced boards'
  climbing counts must be visible) and for standalone-with-ops parity;
  removing them silently regresses both. Recorded as the chunk's own
  decision, not a design quote.
- **Interim interaction (do not "fix" here)**: once chunk 2 makes
  multi-board engine-possible, `RequestSync` marking every bound board
  `Syncing` while one connector syncs its first board only leaves the
  other bound boards' rows `Syncing` until chunk 4's per-connector
  reports resolve them. That is the wave's recorded interim staying
  visible (impl plan §3 chunk 2's loud paragraph); chunk 1 must not
  add aggregation tricks to hide it.
- **`Failed`-report attribution (C8)** marks every bound board in chunk
  1 — vacuous now (≤1 bound board), scoped by chunk 4. A stage-2/3
  "improvement" that scopes it to the reporting connector early is
  scope creep into chunk 4 (it needs the connector identity the wave
  has not minted on reports yet).
- **Snapshot review burden**: 20 files regenerate; the C11 per-file
  rule ("the `sync:` shape change, nothing else") is the review
  contract; drift anywhere else is a finding.
- **Worktree note (process)**: this planning session observed the
  chunk's worktree (`../taskboard-WT-feat-state-sync-status-map`)
  externally disappear once mid-session; it was recreated on the branch
  per the lifecycle rule. Stage 2 begins by verifying/recreating it.
  Unrelated observation for the user:
  `taskboard-WT-docs-daily-driver-design-round` is stale (branch/PR
  merged; flagged by `worktree-doctor.sh`) — its owning round should
  have removed it; not touched by this chunk.

## 8. Bookkeeping (stage 2/3 boundaries)

Stage 2 adds the chunk's `CHANGELOG.md` `Unreleased` line (chunk 8
promotes it). Stage 3 ships: PR body carries `Closes #298` (merge
closes it), one progress annotation comment on #125 ("map + 0006
landed", PR URL) — dispatch MD-4; no other tracker edits, no new
issues. This plan's commit itself: `docs(plan): per-board sync status
map and migration 0006`, no push.

## 9. Estimated diff size

~25 source files + 20 regenerated snapshots + 1 new SQL file ≈ **1,300–
1,700 changed/added lines**, dominated by: migration + storage re-key
(~300 incl. tests), domain types/derivation/pipelines + tests (~500),
state engine mutators + tests (~250), surfaces + ui (~120), test/seed
compile-shape updates (~200). House size **M**, matching the dispatch
plan's estimate.
