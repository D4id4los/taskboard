# [Plan] Growth Roadmap Phase 2 — SQLite Persistence: Schema, Outbox, Repository, Actor

- **Date**: 2026-10-09
- **Target**: `taskboard-storage-sqlite` (whole crate), plus two small
  additive touches to `taskboard-domain`'s `test-support`/`PersistenceAction`
  (§8) and CI workflow wiring (§7). No other crate gains code.
- **Goal**: implement the roadmap's Phase 2 — a normalized sqlite schema with
  sqlx migrations, the durable outbox, `SqliteTaskRepository` implementing the
  Phase 1 `TaskRepository` port with compile-time-checked queries, the storage
  actor wrapper (mpsc loop), and the offline `query!` workflow wired into CI.
- **Status**: ACCEPTED (user review 2026-10-09; §3 decisions confirmed, plus
  the review amendments: §3.3 in-app timestamp sorting, §3.5 migration
  immutability note, §3.15 observability deliverable)
- **Parent roadmap**: `.artifacts/plans/2026-10-08-growth-roadmap-plan.md`
  §3 Phase 2. Exit criterion: "repository passes the domain contract tests
  against sqlite."
- **Predecessor**: `.artifacts/plans/2026-10-08-growth-roadmap-phase1-domain-model-plan.md`
  (merged; its §15 handoff row for Phase 2 is the input contract).

---

## 1. Context & Known Facts (do not re-derive)

- **Phase 1 is merged and shapes everything here.** The port is exactly two
  methods — `TaskRepository::load() -> PersistedState` and
  `apply(Vec<PersistenceAction>)` — with `RepositoryError::{Unavailable,
  Corrupted}` as the only failure classes (`crates/taskboard-domain/src/persistence.rs`).
  `PersistedState` = entity maps + `outbox: Vec<PendingOp>` +
  `validators: BTreeMap<ValidatorKey, SyncValidators>` + `sync: SyncStatus`.
  The contract harness (`test_support/contract.rs`) and referentially
  consistent strategies (`persisted_state_strategy()`) exist behind the
  domain's non-default `test-support` feature.
- **Integration gap 1 — the contract harness cannot drive real IO.**
  `assert_task_repository_contract` awaits via a hand-rolled `block_on` that
  polls **once** with a no-op waker and panics on `Pending`. That is correct
  for `InMemoryRepository` (ready futures, Miri-clean) but a sqlite future
  needs a reactor. §8 adds an async form of the same clauses; the sync entry
  stays for the fake.
- **Integration gap 2 — nothing can persist `SyncStatus`.**
  `PersistenceAction` has entity upserts, op transitions, and validator
  upserts — **no action writes `PersistedState.sync`**. A kiosk that reboots
  must still know "last successful sync: 3 days ago", and Phase 3's engine has
  no way to store it. §8 adds `PersistenceAction::UpsertSyncStatus`.
- **Crate state**: `taskboard-storage-sqlite/src/lib.rs` is doc-comments +
  `#![forbid(unsafe_code)]` only; its manifest already pins
  `taskboard-domain`, `sqlx`, `chrono`, `uuid`, `tokio`, `thiserror`,
  `tracing` (all `.workspace = true`; sqlx 0.9 with `sqlite`,
  `runtime-tokio`, `chrono`, `uuid`, `migrate` features — see root
  `Cargo.toml`). The crate doc already promises `query!` compile-time
  checking; this plan honors that.
- **sqlx 0.9 workflow facts** (verified against the official macro docs):
  - `query!`/`query_as!` require `DATABASE_URL` at **build time** pointing at
    a database that *already has the schema*; the macros do **not** run
    migrations, and `sqlite::memory:` gets a fresh empty database per macro
    expansion, so it does not work for compile-time checking.
  - Offline mode: `cargo sqlx prepare` writes the `.sqlx/` directory **at the
    workspace root**; builds then need `DATABASE_URL` unset (or
    `SQLX_OFFLINE=true`). CI is encouraged to run `cargo sqlx prepare
    --check`. The `sqlx-cli` major/minor must match the crate (0.9) — the
    offline data format changed across versions.
  - The macros also read a `.env` via dotenvy; our git-ignored `.env` (tier-3
    creds) contains no `DATABASE_URL`, so no interference — but CI sets
    `SQLX_OFFLINE=true` explicitly so an ambient `DATABASE_URL` can never
    make a CI build hit the network.
- **Domain facts that shape the schema**: local ids are UUIDv7 (`Uuid`);
  remote ids are per-board-scoped `u64` composites (`RemoteCardRef` =
  board+stack+card); every entity carries per-field write clocks
  (`TaskClocks` ×8, `StackClocks`/`LabelClocks` ×3) plus `remote_seen`;
  tombstones are `deleted: bool` + the `deleted` clock and **must survive
  restarts** (ADR 0004's R3/R5/R6 compare tombstone clocks across boots);
  `Color` is a lenient string newtype.
- **Roadmap-designated duties**: the **outbox** is the crash-safe offline
  write queue (durable in sqlite); **tombstone retention is designed here**,
  not improvised in the Phase 4 actor (roadmap §5); op *coalescing* is actor
  logic built on these types (Phase 4).
- **Backlog constraints already on record**: multi-board storage keys stacks/
  cards by `(board_id, id)` someday (satisfied by uuid PKs — additive); the
  field-level-merge-refinement entry may later want "a persisted edit-history
  structure in Phase 2 storage" (additive future table + migration; not now).
- **House rules that bite this crate**: `#![forbid(unsafe_code)]`, SPDX
  `MIT OR Apache-2.0` header, `# Errors` doc sections on fallible public fns,
  no `unwrap`/`expect` in non-test library code, `tracing` spans across
  `.await`, typed outcome assertions only (no text), no test sleeps, quality
  gates incl. `cargo deny check --workspace` (new transitive deps:
  `libsqlite3-sys` bundled, dev-only `tempfile`).

## 2. Scope

**In scope** — the crate grows from stub to working persistence:

| Area | Deliverable |
|---|---|
| Schema | `migrations/0001_init.sql`: boards/stacks/tasks/labels, `task_labels` join, `outbox`, `sync_metadata`, `sync_status` singleton (§4) |
| Repository | `SqliteTaskRepository: TaskRepository` with `query!`-checked SQL, one transaction per `apply` batch (§5) |
| Connection | `open(path)` / `open_memory()` with WAL + pragmas + embedded migrations run at boot (§5.1) |
| Errors | `OpenError` (thiserror) + `sqlx::Error → RepositoryError` mapping (§5.4) |
| Actor | `StorageCommand` inbox, `StorageHandle`, `spawn_storage_actor` mpsc loop (§6) |
| Domain additions | async contract-harness variant, `PersistenceAction::UpsertSyncStatus`, contract clauses, `InMemoryRepository` parity (§8) |
| Tests | contract harness vs sqlite (memory + file), proptest round-trips, atomicity/corruption/ordering suites, actor routing tests (§10) |
| Workflow/CI | `scripts/sqlx-prepare.sh`, committed `.sqlx/`, `SQLX_OFFLINE` in CI, `prepare --check` step, `.gitignore` entries (§7) |
| Observability | `tracing` statements across the crate so a debug-level log attached to a bug report localizes the failure (§3.15) |
| Docs | ADR 0005, architecture.org storage refresh, CHANGELOG, backlog appends (§12) |

**Out of scope** (owned by later phases): any state-engine logic (Phase 3),
outbox consumption/push/coalescing (Phase 4 — storage only stores and
orders), DTO↔domain mapping (Phase 4), config/paths for the db file and
bootstrap (Phase 5 — `open()` takes a path, config decides which), tombstone
*purge* implementation (designed §4.3, mechanism backlogged), criterion
benches (backlogged; `cargo bench` infra already exists), read pools /
concurrent loads (single-writer MVP, §3.4).

## 3. Design Decisions

1. **Fully normalized relational schema; no JSON columns anywhere.** One
   column per domain field; `LocalOp` decomposes into an `op_kind` tag column
   plus the id columns it addresses. Rationale: every column is
   compile-time-checked by `query!`; decoding failures map cleanly onto the
   typed `RepositoryError::Corrupted`; tombstone purge (§4.3) is plain SQL;
   the crate avoids a second serialization layer (serde_json) entirely.
   *Rejected*: JSON payload columns for clocks/ops (schema-stable but
   bypasses `query!` checking, invites serde drift, and mixes two decoding
   disciplines). *Rejected*: document-store layout (one `payload` column per
   entity) — same objections, and squanders sqlite.
2. **Identity as hyphenated UUID `TEXT` primary keys**, converted explicitly
   (`Uuid::to_string()` / `Uuid::parse_str`) in the row codecs. Debuggable in
   the `sqlite3` CLI — a kiosk bug report can be localized by hand. If sqlx
   0.9's `Uuid` Sqlite impl round-trips as TEXT, the codecs may bind `Uuid`
   directly; the schema and tests do not change either way. *Rejected*:
   `BLOB(16)` (marginally smaller, undebuggable; irrelevant at MVP scale).
3. **Timestamps as `TEXT` in sqlx's chrono encoding** (`DateTime<Utc>`
   implements `Encode`/`Decode<Sqlite>` via the workspace `chrono` feature);
   `Option<DateTime<Utc>>` ↔ nullable columns. `query!` needs the
   type-override syntax (`as "duedate: DateTime<Utc>"`) because sqlite TEXT
   affinity infers strings — adopted crate-wide. Caveat recorded: TEXT
   timestamps must never be used in `ORDER BY` (fractional-digit lengths
   vary); ordering is always by `sort_order`, `op_seq`, or id. This is a
   non-constraint at our scale, not a lurking limit: any "sort by time" is
   derived view logic that belongs in Rust (the domain already owns the
   canonical orderings, e.g. `AppState::live_tasks`), and at < 10 000 tasks
   an in-app sort is microseconds. If SQL-level time ordering is ever
   genuinely needed, add a dedicated sortable-encoded column then — do not
   start overloading the chrono TEXT form.
4. **WAL, `synchronous=NORMAL`, `foreign_keys=ON`, `busy_timeout`,
   single-connection pool.** `SqliteConnectOptions` sets
   `create_if_missing(true)`, `journal_mode(Wal)`, `synchronous(Normal)`,
   `foreign_keys(true)`, `busy_timeout(...)`; `SqlitePoolOptions::
   max_connections(1)`. The actor is the only writer, so one connection
   eliminates the `SQLITE_BUSY` class entirely and — required anyway — keeps
   `:memory:` test databases per-connection-consistent. Durability stance:
   WAL+NORMAL survives **app** crashes without loss (the outbox's
   "crash-safe" promise); OS crash / power loss may drop the newest commits —
   an accepted kiosk trade-off. *Rejected*: `synchronous=FULL` (fsync cost on
   low-power hardware) — revisit only if power-loss field reports show lost
   edits.
5. **Migrations embedded in the binary and run by `open()`.**
   `sqlx::migrate!()` (default `./migrations` next to the crate manifest)
   embeds the SQL at compile time; `open()` applies pending migrations before
   returning. Production boots need no `sqlx-cli`, no filesystem access to
   migration sources, and releases carry their schema. Discipline:
   **migrations are immutable once shipped** — each database records the
   versions it has applied and receives only *new* numbered files on boot;
   editing an already-released migration corrupts that bookkeeping (sqlx
   checksums applied files and fails on mismatch). Schema changes are always
   additive-new-file. sqlite's limited `ALTER TABLE` means some future
   changes need the copy-table-and-rename pattern — fine, but write it
   deliberately.
6. **One `apply` batch = one transaction, executed in a fixed order that
   satisfies FKs**: boards → stacks → labels → tasks (+ their `task_labels`
   rows) → outbox ops → validators → sync status. Entity writes are
   `INSERT … ON CONFLICT(id) DO UPDATE` upserts. **Never `INSERT OR
   REPLACE`**: REPLACE deletes-then-inserts, which would fire
   `ON DELETE CASCADE` and silently destroy the task's `task_labels` rows —
   this pitfall gets its own test (§10 T5).
7. **Task label sets are table rows, replaced wholesale inside the
   transaction** (`DELETE FROM task_labels WHERE task = ?` then re-`INSERT`)
   — the domain models labels as one whole-set field (Phase 1 decision 4),
   and set sizes are small. The `task_labels` table exists because it is the
   one genuinely relational shape (roadmap: "join tables") and it makes
   label→task lookups and future queries possible.
8. **Outbox order is a monotonic `op_seq INTEGER PRIMARY KEY
   AUTOINCREMENT`**; `op_id` is `UNIQUE`. `load()` returns ops in `op_seq`
   ASC order (queue order must not depend on timestamp collisions —
   `queued_at` has seconds-resolution duplicates in practice).
   `EnqueueOp` appends; `CompleteOp`/`FailOp` `DELETE` by `op_id`, and an
   absent id is a no-op (existing contract clause). No FKs from outbox rows
   to entities — ops are addressed by `OpId` only and may legitimately
   coexist with any entity state.
9. **`sync.pending_ops` is never stored; it is derived at load** from
   `COUNT(*)` on the outbox (single source of truth; a stored counter can
   drift). `UpsertSyncStatus` persists `phase` + `last_success` + the failed
   error kind only. Both the sqlite repo and `InMemoryRepository` implement
   this identical derivation so the contract clauses (§8) pin it once.
10. **`SyncPhase`, `SyncErrorKind`, `LocalOp`, and `ValidatorKey` map to
    tagged `TEXT` by hand-written codecs** (pure functions, unit-tested),
    mirroring the domain's serde wire forms (`"boards"`, `"stacks:<u64>"`;
    snake_case op tags like `create_task`, `assign_label`). Unknown tags or
    missing id columns at decode time → `RepositoryError::Corrupted`. This
    keeps the crate serde-free and every stored byte inspectable.
11. **Remote refs decompose into nullable id columns** with the invariant
    all-or-none NULL, enforced at the codec layer (a `CHECK` across three
    columns adds schema friction without protecting anything the codecs and
    round-trip tests don't already pin). Decomposed columns — not a JSON
    blob — keep `RemoteCardRef` joinable and purge-able.
12. **Constraint violations map to `Corrupted`.** `RepositoryError` has two
    variants by design (transient vs not-retryable); an FK/CHECK/UNIQUE
    violation is an integrity failure of the persisted dataset or the batch —
    not transient — so `Corrupted` is the honest bucket. Documented in
    `error.rs`; if Phase 3 ever needs finer classes, extend the port then.
13. **The actor replies; it never broadcasts errors.** Every `Apply` and
    `Load` carries a `oneshot` reply with the typed `Result`. mpsc FIFO
    ordering makes "await the reply of the last Apply" the natural flush
    barrier the CLI exit path needs (Phase 5), deterministically testable
    with zero sleeps. *Rejected*: fire-and-forget applies + error escalation
    via `SystemEvent` (the commander cannot correlate outcomes; ordering
    guarantees get implicit).
14. **Schema is forward-compatible by construction**: uuid PKs (multi-board
    keys stack on top), tombstones stay in-table (no deletes to unwind), and
    the backlogged edit-history table / purge mechanism are additive
    migrations.
15. **Observability is a deliverable, not an afterthought** (coding
    guidelines §4: a default log output must localize a bug report, and
    `debug`/`trace` must pinpoint it). Statements land with the code, at
    these levels:
    - `error!` — an `apply` batch rolled back, a `load` returning
      `Corrupted`, migration failure in `open()` (the "data may be damaged"
      class);
    - `warn!` — every `Err` reply the actor sends (with the typed variant
      name, never a formatted error string alone), a busy/locked retry;
    - `info!` — lifecycle: database opened (path, migrations applied count),
      actor started/stopped, batch applied (action count, kind histogram);
    - `debug!` — each `StorageCommand` received and completed, transaction
      begin/commit;
    - `trace!` — per-row detail on upserts and decodes (entity kind, id,
      remote binding) — enough to reconstruct exactly which rows a failing
      batch touched.
    Ids and enum variants are logged via `Display`/`Debug` of typed values;
    never user content at `info`+ (task titles etc. only at `trace`, if at
    all). Logging assertions stay out of tests (AGENTS §5 no-text rule) —
    coverage of the arms is accepted as a gap exactly like the sync crate's
    logging arms (existing backlog entry).

## 4. Schema (`migrations/0001_init.sql`)

### 4.1 DDL (sketch — column set normative, types as decided above)

```sql
CREATE TABLE boards (
    id              TEXT PRIMARY KEY,     -- hyphenated UUIDv7
    remote_board_id INTEGER NULL,
    title           TEXT NOT NULL,
    color           TEXT NOT NULL,        -- lenient raw string
    archived        INTEGER NOT NULL,     -- 0/1
    deleted         INTEGER NOT NULL,     -- tombstone flag
    remote_seen     TEXT NULL             -- DateTime<Utc> TEXT
);

CREATE TABLE stacks (
    id               TEXT PRIMARY KEY,
    board            TEXT NOT NULL REFERENCES boards (id),
    remote_board_id  INTEGER NULL,
    remote_stack_id  INTEGER NULL,
    title            TEXT NOT NULL,
    sort_order       INTEGER NOT NULL,
    archived         INTEGER NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,       -- per-field clocks
    ck_sort_order    TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);

CREATE TABLE tasks (
    id               TEXT PRIMARY KEY,
    stack            TEXT NOT NULL REFERENCES stacks (id),
    remote_board_id  INTEGER NULL,
    remote_stack_id  INTEGER NULL,
    remote_card_id   INTEGER NULL,
    title            TEXT NOT NULL,
    description      TEXT NOT NULL,
    duedate          TEXT NULL,
    done             TEXT NULL,           -- completion timestamp
    sort_order       INTEGER NOT NULL,    -- domain `order`; keyword-safe name
    archived         INTEGER NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,
    ck_description   TEXT NOT NULL,
    ck_duedate       TEXT NOT NULL,
    ck_done          TEXT NOT NULL,
    ck_position      TEXT NOT NULL,       -- composite stack+order clock
    ck_labels        TEXT NOT NULL,       -- whole-set clock
    ck_archived      TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);
CREATE INDEX tasks_by_stack ON tasks (stack);

CREATE TABLE labels (
    id               TEXT PRIMARY KEY,
    board            TEXT NOT NULL REFERENCES boards (id),
    remote_board_id  INTEGER NULL,
    remote_label_id  INTEGER NULL,
    title            TEXT NOT NULL,
    color            TEXT NOT NULL,
    deleted          INTEGER NOT NULL,
    ck_title         TEXT NOT NULL,
    ck_color         TEXT NOT NULL,
    ck_deleted       TEXT NOT NULL,
    remote_seen      TEXT NULL
);

CREATE TABLE task_labels (
    task  TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    label TEXT NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (task, label)
);

CREATE TABLE outbox (
    op_seq    INTEGER PRIMARY KEY AUTOINCREMENT,  -- stable queue order
    op_id     TEXT NOT NULL UNIQUE,
    op_kind   TEXT NOT NULL,                      -- 'create_task' | ...
    task_id   TEXT NULL,                          -- per-kind required columns
    stack_id  TEXT NULL,
    label_id  TEXT NULL,
    queued_at TEXT NOT NULL
);

CREATE TABLE sync_metadata (
    key           TEXT PRIMARY KEY,               -- 'boards' | 'stacks:<u64>'
    etag          TEXT NULL,
    last_modified TEXT NULL
);

CREATE TABLE sync_status (
    id           INTEGER PRIMARY KEY CHECK (id = 1),  -- singleton row
    phase        TEXT NOT NULL,                       -- 'idle'|'syncing'|'offline'|'failed'
    last_error   TEXT NULL,                           -- SyncErrorKind tag
    last_success TEXT NULL
);
INSERT INTO sync_status (id, phase) VALUES (1, 'idle');
```

`op_kind` ↔ required columns: task ops (`create_task`, `update_task`,
`move_task`, `delete_task`) need `task_id`; stack ops need `stack_id`; label
ops need `label_id`; `assign_label`/`unassign_label` need both `task_id` and
`label_id`. Decoding an op whose required columns are NULL → `Corrupted`.

### 4.2 Conventions

- `query!`/`query_as!` everywhere SQL appears in `src/` (the crate doc's
  promise); chrono columns read with `as "col: DateTime<Utc>"` overrides.
- Bools bind/decode as 0/1 INTEGER via sqlx. Nullable columns ↔ `Option`.
- Column names avoid SQL keywords (`sort_order`, not `order`).
- No stored triggers, views, or generated columns — all logic lives in Rust
  where it is typed and tested; the schema stays dumb data.

### 4.3 Tombstone handling & retention (the roadmap §5 duty)

- **Tombstones are ordinary rows with `deleted = 1`** in the same tables.
  They must persist across restarts: ADR 0004's R3 (card delete-wins), R5
  resurrect-vs-stay-tombstoned, and R6 (soft-deleted stacks/boards) all
  compare tombstone clocks against observations made days apart. Purging a
  tombstone early would resurrect a remotely-deleted entity on the next pull.
- **MVP retains tombstones forever.** Growth is bounded by real usage on a
  single board; nothing in the port surface expires rows.
- **Retention mechanism (designed, backlogged)**: when needed, a
  `PersistenceAction::PruneTombstones { older_than: DateTime<Utc> }` variant
  (domain addition) executes as `DELETE FROM tasks WHERE deleted = 1 AND
  ck_deleted < ?` (stacks/labels/boards likewise; `task_labels` follows via
  the existing `ON DELETE CASCADE`). The schema above already supports this
  as a pure-SQL statement — no migration needed when it lands.

## 5. `SqliteTaskRepository` (`repo.rs`, `connect.rs`, `codec.rs`, `error.rs`)

### 5.1 Construction

```rust
impl SqliteTaskRepository {
    /// Open (creating if absent) the database at `path`, apply pending
    /// migrations, return the repository.
    /// # Errors: OpenError::{Connect, Migrate}
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, OpenError>;
    /// In-memory variant for tests (single connection — see decision 4).
    pub async fn open_memory() -> Result<Self, OpenError>;
}
```

`open_memory()` = `SqliteConnectOptions::from_str("sqlite::memory:")` +
`max_connections(1)` + `min_connections(1)` + `idle_timeout(None)` so the
database (which is per-connection) survives between calls. The pragmas of
decision 4 apply to both (WAL is a no-op on `:memory:`, harmless).

### 5.2 `load()`

Six `SELECT`s (boards, stacks, labels, tasks, task_labels, outbox) +
`sync_metadata` + `sync_status`, row-mapped through private row structs into
domain entities (nested clock structs make `query_as!` on domain types
impossible; small `From<Row>` mapping functions keep it mechanical and
reviewable). Fresh database → `PersistedState::default()` except
`sync.pending_ops` still derived (0). Map key order is deterministic by
construction (`ORDER BY id` / `ORDER BY op_seq`; BTreeMap inserts don't care,
but stable ordering keeps proptest comparisons cheap). `pending_ops` =
outbox length.

### 5.3 `apply(actions)`

`pool.begin()` → execute each action in batch order (decision 6) → commit.
Any statement error aborts the transaction (dropped without commit) and the
whole batch is rolled back — the port's atomicity clause. Upserts, op
transitions, validator upserts, and `UpsertSyncStatus` as decided above.

### 5.4 Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("sqlite connect failed")]
    Connect(#[source] sqlx::Error),
    #[error("sqlite migration failed")]
    Migrate(#[source] sqlx::migrate::MigrateError),
}
```

`impl From<sqlx::Error> for RepositoryError` (crate-local, feeds the port):
decode/`ColumnDecode`/`UnexpectedNull`/constraint-class database errors →
`Corrupted`; everything else (io, busy, locked, unavailable) →
`Unavailable`. The mapping is a documented match on error shape, not on
message text. No `unwrap` in any non-test path (house rule).

## 6. Storage Actor (`actor.rs`)

```rust
/// Storage actor inbox (the channel-bearing envelope this crate owns —
/// domain payloads inside, per the payload/envelope split).
pub enum StorageCommand {
    Apply  { actions: Vec<PersistenceAction>,
             reply: oneshot::Sender<Result<(), RepositoryError>> },
    Load   { reply: oneshot::Sender<Result<PersistedState, RepositoryError>> },
}

/// Cloneable handle; async fns wrap the oneshot round-trip.
#[derive(Debug, Clone)]
pub struct StorageHandle { /* mpsc::Sender<StorageCommand> */ }
impl StorageHandle {
    pub async fn apply(&self, actions: Vec<PersistenceAction>)
        -> Result<(), RepositoryError>;       // send errors → Unavailable
    pub async fn load(&self) -> Result<PersistedState, RepositoryError>;
}

/// Spawn the actor loop onto the current runtime; returns handle + JoinHandle.
pub fn spawn_storage_actor(repo: Arc<SqliteTaskRepository>)
    -> (StorageHandle, tokio::task::JoinHandle<()>);
```

Loop: `while let Some(cmd) = rx.recv().await` — execute against the repo
under a `tracing` span, reply with the typed result, continue. Channel close
(= last handle dropped, or engine shutdown) drains naturally and the task
exits — the `JoinHandle` lets the future Phase 5 bootstrap await clean exit.
The actor adds no policy: repo semantics are the contract's, actor tests
cover routing only (one-behavior-one-test rule).

## 7. sqlx Offline Workflow & CI Wiring

- **`scripts/sqlx-prepare.sh`** (committed): create a throwaway
  schema-ready database, then regenerate the offline cache, then clean up:

  ```sh
  set -euo pipefail
  DB=sqlite://.sqlx-prepare.db?mode=rwc
  rm -f .sqlx-prepare.db*
  DATABASE_URL=$DB sqlx migrate run \
      --source crates/taskboard-storage-sqlite/migrations
  DATABASE_URL=$DB cargo sqlx prepare --workspace -- --all-targets
  rm -f .sqlx-prepare.db*
  ```

  Run it after touching any `query!` or migration; commit the refreshed
  `.sqlx/` with the change (never separately).
- **`.gitignore`**: `.sqlx-prepare.db*` (covers `-wal`/`-shm` sidecars).
- **`.github/workflows/ci.yml`**: add `SQLX_OFFLINE: "true"` to the `env` of
  the `check`, `test`, and `coverage` jobs (all of them compile the
  workspace), and a "sqlx offline cache freshness" step in `check` after
  clippy: install `sqlx-cli` 0.9 (sqlite-only, no default features) via
  `taiki-e/install-action`, then `cargo sqlx prepare --check --workspace --
  --all-targets` with the same throwaway-database dance — this fails the PR
  that forgot to refresh `.sqlx/`.
- **Docs**: the exact `cargo install sqlx-cli --version 0.9
  --no-default-features --features sqlite` pin goes in the crate docs and
  ADR 0005 (CLI/crate version skew corrupts the offline workflow — known
  sqlx failure mode).
- Note: `query!` reads `.env` via dotenvy at compile time; harmless here
  (no `DATABASE_URL` in it) and `SQLX_OFFLINE=true` forces precedence in CI.

## 8. Domain Additions (small, additive, in the same PR)

1. **`test_support/contract.rs`**: extract the clause body into
   `pub async fn assert_task_repository_contract_async<R: TaskRepository>
   (&R)`; the existing sync `assert_task_repository_contract` becomes a
   wrapper driving it with the no-op-waker `block_on` (still one poll — the
   in-memory fake stays ready). All Phase 1 clauses move verbatim.
2. **`PersistenceAction::UpsertSyncStatus(SyncStatus)`** (new variant,
   documented: "persist phase + last success; `pending_ops` is derived by
   implementations from the outbox"). `InMemoryRepository` gains the arm —
   it stores phase/last_success and **derives `pending_ops` at load** from
   the outbox length, exactly like sqlite (decision 9).
3. **New contract clauses** (both harness variants, hence pinning the
   in-memory fake and sqlite identically):
   - after `UpsertSyncStatus(s)` with a non-empty prior outbox,
     `load().sync.phase == s.phase && last_success == s.last_success &&
     pending_ops == outbox.len()` (the derivation, not the stored counter);
   - enqueueing/completing ops moves `pending_ops` accordingly.
4. **One Phase 1 test needs a semantic update**:
   `in_memory_repository_preserves_preloaded_state` currently pins
   `snapshot().sync.pending_ops == 7` from a hand-built state with an empty
   outbox. Under decision 9 that stored 7 is exactly the drift the design
   removes, so the test is updated to pin the new invariant (preloaded phase
   preserved; `pending_ops` re-derived from the outbox). This is a contract
   change made deliberately in this plan, not a weakening to dodge a bug —
   flagged here for reviewer attention.

## 9. Module Layout & TDD Implementation Order

```
crates/taskboard-storage-sqlite/
├── Cargo.toml        # deps unchanged; add dev-deps:
│                     #   taskboard-domain (features = ["test-support"])
│                     #   proptest, tempfile
├── migrations/
│   └── 0001_init.sql
├── scripts support:  # repo-root scripts/sqlx-prepare.sh (§7)
└── src/
    ├── lib.rs        # crate docs (offline workflow!), re-exports
    ├── error.rs      # OpenError, From<sqlx::Error> for RepositoryError
    ├── codec.rs      # TEXT <-> Uuid / LocalOp / SyncPhase / ValidatorKey
    ├── connect.rs    # connect options + pragmas, open(), open_memory()
    ├── repo.rs       # SqliteTaskRepository: load/apply + row mapping
    └── actor.rs      # StorageCommand, StorageHandle, spawn_storage_actor
tests/
├── contract.rs       # async harness vs :memory: AND file-backed db
├── roundtrip.rs      # proptest suite (§10 P1–P4)
└── actor.rs          # actor routing tests (§10 A1–A3)
```

Branch: `feat/storage-sqlite-repository`, one PR for the plan (roadmap §3
granularity; the §8 domain touches land as its first commits so the diff
reads "port prep → adapter"). Stacked split (`feat/domain-sync-status-action`
first) stays available if review size demands. TDD order:

1. **Domain prep** (§8): async harness, new action, fake parity, updated
   pin test — `cargo nextest run -p taskboard-domain --all-features` green.
2. **`error.rs` + `codec.rs`**: pure functions first — id/op/phase/key
   codecs with unit tests incl. every rejection path (`Corrupted` mapping is
   asserted as a typed outcome, never a message).
3. **`connect.rs` + `0001_init.sql`**: `open()`/`open_memory()`; tests: fresh
   file db loads `default`-shaped state; second `open()` on the same file
   applies zero migrations (idempotence); pragmas verified structurally
   (`PRAGMA journal_mode` query returns `wal` — value assertion, not text).
4. **`repo.rs` write+read per entity kind**: upsert then load round-trips
   each of board/stack/task/label (including remote `Some`/`None`, clocks,
   tombstones, label sets) — one test per kind at this layer, the proptest
   generalizes later.
5. **Batch semantics**: enqueue/complete/fail op transitions; validators
   overwrite; `UpsertSyncStatus`; upsert-keeps-`task_labels` (T5 pitfall);
   whole-batch rollback via a test-installed `RAISE(ABORT)` trigger (§10 T6).
6. **Contract harness** (async) against `:memory:` and a `tempfile` db.
7. **`roundtrip.rs` proptests** (§10 P-catalogue).
8. **`actor.rs`** + routing tests (§10 A-catalogue).
9. **Workflow/CI wiring** (§7): script, gitignore, workflow env, freshness
   step; run prepare; commit `.sqlx/`.
10. **Docs** (§12) + full quality gates + coverage pass.

## 10. Test Plan (catalogue)

Deterministic by construction everywhere: replies/join-handles are the
completion signals — no sleeps, no polling, no wall-clock assertions.
Typed outcomes only. In-memory for everything except the file-backed
contract run and the reopen test.

- **C1–C2 Contract**: `assert_task_repository_contract_async` green on
  `open_memory()` and on a `tempfile`-backed db (incl. the new §8 clauses).
- **P1 Round-trip identity** (proptest, `persisted_state_strategy`):
  state → one `apply` batch of upserts/enqueues/validator upserts/status →
  `load()` == input **modulo** `sync.pending_ops` (re-derived, asserted
  equal to outbox len) — 256 cases, matching the domain's convention.
- **P2 Outbox order**: enqueue N ops (with colliding `queued_at` seconds) →
  `load()` preserves enqueue order; complete/fail removes by id, order of
  the rest unchanged.
- **P3 Upsert overwrite**: applying a modified task/stack/label/board twice
  → second write wins everywhere (clocks included).
- **P4 Idempotent re-apply**: applying the same batch twice leaves the same
  state as once (no duplicate outbox rows — `op_id UNIQUE` in action).
- **T5 Upsert must not cascade**: task with labels upserted again (same id,
  changed title) → `task_labels` intact.
- **T6 Atomicity**: test db with a temporary `RAISE(ABORT)` trigger on
  outbox inserts; batch `[UpsertTask, EnqueueOp]` → `Err(Unavailable-class
  rollback outcome)` … asserted as `Err(_)` + **load shows neither** the
  task upsert nor the op (typed structural facts).
- **T7 Corruption mapping**: raw-inserted bad rows (unparseable uuid,
  unknown `op_kind`, missing required op column, unknown phase tag) →
  `load()` returns `Err(RepositoryError::Corrupted)` via `matches!`.
- **T8 Reopen persistence**: file db → apply → drop pool → reopen → load ==
  same state (WAL survives clean close; also exercises migration
  no-op-on-second-open).
- **A1 Actor Apply**: handle.apply(batch) → Ok; verify via load through a
  second handle/repo.
- **A2 Actor ordering barrier**: two applies from one handle; first reply
  resolves before the second's effects are observable (FIFO + reply-after-
  commit — the flush semantics Phase 5 relies on).
- **A3 Actor shutdown**: drop all handles → JoinHandle completes (awaited
  with `tokio::time::timeout` as a deadline guard, not a sleep).
- **Doc-tests** on `open`/handle usage compile-run.

Miri is not scoped to this crate (testing strategy §10 scopes it to
domain/state); `cargo mutants` likewise. Coverage: the §10 catalogue is the
review target for the llvm-cov patch (CI coverage job already runs
`--all-features`).

## 11. Out of Scope → Backlog (append with this plan)

- **Tombstone purge mechanism** — `PersistenceAction::PruneTombstones` +
  actor/CLI surface (schema already supports it, §4.3). Trigger: local db
  growth observed in real use (M1).
- **Criterion bench for the persistence hot path** — `load()` hydration and
  one `apply` batch at N ∈ {100, 1 000, 10 000} tasks; the benchmark CI job
  exists and currently no-ops. (Kiosk perf goal; kept out only to hold PR
  size.)
- **Read pool / concurrent loads** — if Phase 3 ever wants parallel reads
  while the actor writes, revisit `max_connections(1)` + WAL reader
  semantics.
- **Outbox compaction/coalescing at rest** — storage exposes append/remove
  only; compaction policy belongs to the Phase 4 push side.
- **`sqlx` error-class mapping refinement** — if `Corrupted` proves too
  coarse for constraint violations in practice, propose a port variant
  (`Rejected`?) with an ADR rather than overloading `Corrupted` silently.

## 12. Docs & Bookkeeping (part of this phase's PR)

- **ADR 0005 — "SQLite local persistence"**: normalized schema + TEXT
  ids/timestamps rationale, WAL/`NORMAL` durability stance (app-crash-safe
  outbox; power-loss trade-off), single-connection pool, derived
  `pending_ops`, tombstone retention policy (§4.3), offline `query!`
  workflow + sqlx-cli 0.9 pin. Status: Accepted.
- **architecture.org**: refresh the `taskboard-storage-sqlite` crate bullet
  (tables, actor inbox `StorageCommand`, boot-time migrations, offline
  prepare workflow pointer).
- **CHANGELOG**: `[Unreleased]` → Added entry for the crate + the two
  domain additions.
- **Backlog**: append §11 entries using the file's entry template.
- **Crate docs** in `lib.rs`: developer quickstart for the prepare workflow
  (when `query!` errors mention DATABASE_URL → run `scripts/sqlx-prepare.sh`).

## 13. Verification & Acceptance

- Full quality gate (AGENTS §10): `cargo fmt --all -- --check`; `cargo
  clippy --workspace --all-targets -- -D warnings` (with `SQLX_OFFLINE=true`
  so macros resolve offline); `cargo nextest run --workspace`; `cargo test
  --doc --workspace`; `cargo deny check --workspace`; `cargo llvm-cov
  --workspace`.
- `cargo nextest run -p taskboard-storage-sqlite` green including the
  contract harness against `:memory:` **and** file-backed databases, and
  `cargo nextest run -p taskboard-domain --all-features` still green after
  the §8 changes.
- CI's new `cargo sqlx prepare --check` step passes with a committed
  `.sqlx/`.
- Phase 2 exit (roadmap): the repository passes the domain contract tests
  against sqlite — C1/C2 are that criterion, verbatim harness clauses.
- Downstream compile check: `cargo check --workspace` green; no code changes
  outside `taskboard-storage-sqlite`, the two §8 domain files, CI, scripts,
  docs, and `.gitignore`.

## 14. Consumer Handoff Map (what later phases import)

| Consumer | Takes from Phase 2 |
|---|---|
| Phase 3 `state` | `StorageHandle::apply/load` (reply-based flush barrier), the `PersistenceAction` batching discipline (decision 6 order), derived `pending_ops` semantics |
| Phase 4 sync | outbox table semantics (queue order by `op_seq`, addressing by `OpId`) — read via the engine, never direct SQL; `sync_metadata` validators surviving restarts |
| Phase 5 app/CLI | `SqliteTaskRepository::open(path)` wired to `[storage] db path` from figment; migrations-free boot; drop-all-handles → actor exit as the shutdown path |
| Everyone | the offline build workflow (`scripts/sqlx-prepare.sh`, `.sqlx/`, `SQLX_OFFLINE`) |

## 15. Risks & Mitigations

- **Offline-cache friction** (`query!` build errors when `.sqlx/` is stale):
  mitigated by the prepare script, the CI `--check` step, and crate-doc
  guidance; failure mode is a loud compile error, not silent drift.
- **sqlx-cli / sqlx crate version skew** (offline data format changed across
  versions): pin 0.9 in install instructions and ADR 0005.
- **`:memory:` pool gotcha** (per-connection databases): `open_memory()`
  bakes the single-connection recipe; a doc note prevents cargo-culting a
  multi-connection memory pool that silently sees empty tables.
- **sqlite TEXT affinity inference in macros** (chrono columns infer as
  strings): the `as "col: DateTime<Utc>"` override convention is adopted
  crate-wide and exercised from the first repo test onward.
- **`INSERT OR REPLACE` cascade trap** (decision 6): named pitfall, pinned
  by T5.
- **`RepositoryError` two-variant squeeze** (constraint violations →
  `Corrupted`): documented mapping; backlogged refinement path (§11) with an
  ADR gate.
- **Power-loss durability of the outbox** (`NORMAL` sync): accepted and
  documented trade-off (decision 4); revisit trigger defined.
- **Domain-contract churn from §8**: kept additive; the one updated test is
  explicitly flagged (§8.4) so review confirms the semantic change rather
  than rubber-stamping it.
