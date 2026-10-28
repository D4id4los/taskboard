# [Plan] Time tracking — entry storage & sidecar roaming (DR-5/2, #487) — stage-1 TDD implementation plan

- **Date**: 2026-10-28
- **Kind**: stage-1 TDD implementation plan for chunk **DR-5/2** of the
  six-rounds dispatch (wave 7). Zero production code in this commit;
  this file is the chunk's entire planning artefact. Implementation is
  stage 2's (dispatch prompt 2.2), review + shipping stage 3's
  (prompt 2.3).
- **Origin**: the accepted design round
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  (§3 Parts C+D and D-T9–D-T13 are normative decision by decision;
  §1's sidecar-substrate anchors — the dismissals-wave 8-step recipe,
  `prune_absent_cards` card-keyedness, `lane_gc_scheduled`; §5's
  storage/sidecar test hooks; §7's chunk-2 row), dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§3.4/§10.2 (the Covers/Acceptance list and the code-quality
  mandate are binding). Issue: **#487**. Siblings #486 (landed), #488
  (CLI, later wave), #489 (UI, landed) are *other* chunks — not a line
  of them lands here.
- **Precondition verification (recorded 2026-10-28)**: DR-5/1 (#486)
  merged as PR #519 (2026-10-27); DR-4/R (#484) merged as PR #544
  (2026-10-28) — both verified CLOSED via `gh issue view` and present
  in `git log origin/main` after `git fetch origin --prune`. The
  merge/pipeline lane is single-flight and this chunk follows DR-4/R
  directly. **Migration check**: the dispatch assigned **0013**, but at
  this chunk's base 0013 = `0013_reminder_insistence.sql` (DR-3/2,
  landed first) and 0014 = `0014_outbox_ref_delete_payload.sql`
  (DR-4/R, landed out of wave order). Next free at the base is
  **0015** — this chunk takes 0015; deviation from dispatch §2's table
  recorded per its own landing-order rule (§4 D-1).
- **Normative base**: `origin/main` = `f67c9bc0` (the worktree's branch
  point; the local `main` mirror was fast-forwarded 23 commits to it
  before the worktree was cut). Gates re-checked against this tip.
- **Branch / worktree**: `feat/tracking-storage-roaming` in
  `../taskboard-WT-feat-tracking-storage-roaming` (created 2026-10-28
  after `scripts/worktree-doctor.sh` passed: 245 G free, floor 25 G;
  the doctor's stale-fetch WARN is refetch noise — the fetch had
  succeeded at session start and all verification used it).
- **Milestones** (plan structure, not commit boundaries — stage 2
  lands TDD pairs as standalone green commits): M1 migration 0015 +
  the storage codec (the table the dead writes await); M2 the sidecar
  section + row enum with tolerant serde; M3 the union arm; M4 the
  overlay builder + tombstone derivation guard; M5 lane GC + flush
  integration; M6 the fold + its pipeline call site; M7 the outbox
  comment fix + the full gate suite.

---

## 1. Mission & binding constraints

The durability and roaming half of tracking: entries survive daemon
restarts in SQLite (Part C), and roam between devices as a **board-level
`"time-entries"` sidecar section** keyed by entry UUID (Part D) —
history **outlives task deletion** (`prune_absent_cards` gains **no
arm**; an arm here is a blocking stage-3 finding), deletes roam as
derived tombstones while retention aging never leaves the wire, and the
local DB never drops a row.

Binding, verbatim from the origin plan and dispatch:

- **D-T9**: migration adds `time_entries`; no FKs (entries outlive soft
  deletes); **no intent table — the open row is the intent** (the
  already-written storage writes implement exactly this; M1 lands the
  table they await). New `PersistenceAction` variants already exist
  (chunk 1); the exhaustive storage match already forces the arms.
- **D-T10**: section `time_entries`, keyed by entry UUID;
  `TimeEntryRecord { Entry, Tombstone, Verbatim }` after `RuleEntry`;
  `task` is the **decimal remote card id**; **unbound-task entries are
  never emitted**; old devices relay the section verbatim (mixed
  fleets fold nothing, lose nothing).
- **D-T11**: per-key embedded-clock LWW; newer `clock` wins, ties →
  overlay; `Verbatim` never ordered; tombstones compete by the same
  clock; a remote stop is just a newer write — no special case.
- **D-T12**: tombstone derivation reuses the truncated-view guard —
  **only base-present live rows, within the retention window, whose
  card resolves locally**; a narrower view can never erase another
  device's history; unresolvable entries age out instead. Fold resolves
  card → `TaskId`, applies per-entry LWW against local `updated_at`,
  imports live rows, applies strictly-newer tombstones as **local
  removal** (deletes are global; aging is not — asymmetric by design),
  skips unresolvable. Fold called **after `fold_reminder_dismissals`**
  in the pipeline pass.
- **D-T13**: `TRACKING_SYNC_RETENTION_DAYS = 180`, a named constant (no
  `[tracking]` config — fenced); document-side GC at flush in the
  `lane_gc_scheduled` shape; the overlay builder filters by the same
  window; **the local DB never drops history**; stale-device re-emit of
  aged rows is the accepted one-version tolerance.
- **Flush trigger**: every tracking mutation enqueues
  `LocalOp::UpdateAppData(TaskId)` — **already landed in chunk 1**
  (verified §2); this chunk wires the flush to consume it for the new
  section. The stale `outbox.rs` doc comment at the `UpdateAppData` op
  is fixed in passing.
- **Nothing ticks**; domain purity holds; no new dependencies (`cargo
  deny` untouched); typed outcomes everywhere — no text assertions
  (AGENTS §5); SPDX `MIT OR Apache-2.0` on domain/storage/sync files;
  `#![forbid(unsafe_code)]` holds crate-wide.
- **DR-4 boundary**: this chunk's fold is purely additive-section — it
  must not touch task-field merge semantics (`merge_task`,
  `remote_baseline`, the intent guard machinery it does consume
  read-only).

## 2. Verified seam facts (do not re-derive; verified 2026-10-28 against `f67c9bc0`)

| Fact | Anchor | Evidence |
|---|---|---|
| The storage write side is **already written, dead** | storage `repo.rs` | the exhaustive match's `UpsertTimeEntry`/`DeleteTimeEntry` arms partition into `entry_upserts`/`entry_deletes` (:940-941, :974-975) and the INSERT…ON CONFLICT / DELETE statements exist (:1236-1267) as **runtime-checked** `sqlx::query` "because the `time_entries` table arrives with DR-5/2's migration"; the R-11 comment explicitly predicts this chunk renumbers "again at its rebase" |
| The load side is the gap | storage `repo.rs` | `load_inner` hydrates `tracking: TrackingState::default()` with the comment "Chunk DR-5/2's migration hydrates this map" (:847-849); every other table loads via compile-checked `query_as!` + a `decode_*` fn (e.g. `decode_reminder_rule` :804) |
| Migration runner + offline cache | storage `connect.rs`, `scripts/sqlx-prepare.sh` | `sqlx::migrate!("./migrations")` :16; CI runs `scripts/sqlx-prepare.sh --check` (ci.yml :75-76) with `SQLX_OFFLINE=true` — **any new compile-checked query requires regenerating `.sqlx/` and committing it with the change** |
| Migration conventions | `migrations/0009_reminder_dismissals.sql` | hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no FKs, header comment stating the design; upgrade backfills only where semantics demand (none here) |
| D-T9's DDL is legal verbatim | SQLite (verified) | `CREATE TABLE time_entries (… start TEXT NOT NULL, end TEXT, …)` executes unquoted — SQLite admits `end` as a column identifier; the already-written INSERT binds these exact column names |
| The section registry and its recipe | domain `appdata.rs` | `AppDataSections { tasks, reminder_rules, dismissals, unknown }` :68-86 (serde `rename`/`default`/`skip_serializing_if` per field); `RuleEntry { Rule, Tombstone, Verbatim }` :360-378 with hand-written `deserialize_verbatim` :393-420 (tolerant, never fails) + matching `Serialize` :423-444; the dismissal twin :540-582 |
| `union()` is the one merge point | domain `appdata.rs` | per-section arms :621-668 — each `(Some, Some)` clones base then merges overlay entries via a `union_*_entry` helper; task fields resolve ties to the **overlay** (:691-698), rules/dismissals ties keep the **base** (:719-720) — see D-7; unknown envelope keys + sections additive throughout |
| `prune_absent_cards` is card-keyed only | domain `appdata.rs` | :769-796 filters exactly `tasks`/`reminder_rules`/`dismissals` by the present-card set — a board-level section keyed by entry UUID is invisible to it **by absence of an arm** |
| Overlay builders + derivation | domain `appdata.rs` | `task_entry_for` :805, `rules_entry_for` :852, `dismissals_entry_for` :893 (fail-closed emission filter precedent), `derive_rule_tombstones(base, local, clock)` :930-946 — derives only for base-*present live* rows absent locally, stamps the caller's clock, tombstones/verbatim pass through |
| Lane GC shape | domain `appdata.rs` | `lane_gc_scheduled(document, drop)` :954-962 — clone-and-strip one modeled lane, decision (the flag) lives at the sync layer |
| The fold pattern | domain `merge.rs` | `RuleFoldChanges { upserts, deletes }` :678-683; `fold_reminder_rules` :702-767 — `clock >= local.updated_at` adopts (doc wins pull-side ties), tombstone `> local.updated_at` removes, verbatim never folds, unparseable keys skipped; `fold_reminder_dismissals` :790-812 is the guard-immune twin |
| The pipeline fold pass | domain `pipeline.rs` | §5b :944-1004 — only `Document` evidence folds; per-card loop over `snapshot.tasks` resolving via `task_by_board_card` (:961, built :497, retained to the pulled board :553; board id = `snapshot.board.id` :519); deltas become `PersistenceAction`s and the same deltas feed the memory view (`folded_rules`/`folded_dismissals` :951-957, batch extend :1220-1221, final state :1271-1272 — where `tracking: current.tracking.clone()` :1275 is this chunk's swap point) |
| The `PersistenceAction` variants exist | domain `persistence.rs` | `UpsertTimeEntry`/`DeleteTimeEntry` :236-239; `apply_actions` already interprets both into the memory view (:345-348); the engine applies to storage before interpreting (the crash-resume order) |
| The flush consumes `UpdateAppData` | sync `flush.rs`, `actor.rs` | `FlushCtx` :69-86 (tasks/stacks/rules/dismissals refs); overlay assembly :205-258 (per-card loop + derived rule tombstones :230-239); GC chain `union → lane_gc_scheduled → prune_absent_cards` :261-269; construction site `actor.rs` :653-667 from the loaded `PersistedState`; lazy-anchor gates `board_has_local_app_data` :92-118 / `has_flush_work` :123-139 (called `actor.rs` :633) |
| Chunk 1's flush-trigger enqueues | domain `command.rs` | `StartTracking` :1950, `StopTracking` :2017, `LogTime` :2039 (+ :2059), `AdjustTimeEntry` :2064 (+ :2097), `DeleteTimeEntry` arm, and the done/deleted-close arms all enqueue `UpdateAppData` — the trigger needs **no new enqueue code** |
| The stale comment | domain `outbox.rs` | `UpdateAppData(TaskId)` doc: "…Nothing enqueues it yet — the flush lane is chunk 4's." (:79-81) — six-plus call sites exist since |
| `ClearBoardContent` is entry-blind | domain `persistence.rs` :241-243, storage `repo.rs` | the repair clears stacks/tasks/labels/outbox/validators/reminder facts — time entries are not in its contract (see D-15) |
| Test infrastructure ready | domain `test_support/`, storage `tests/` | `time_entry_strategy()`/`tracking_state_strategy()` (valid spans by construction); `app_state_strategy` already generates referentially-consistent tracking rows (:410-447); `persisted_state_strategy` carries them; storage `tests/common/actions_from_state` **does not yet emit entry actions** and the P1 roundtrip proptest does not yet assert `tracking` — both are M1 test-side extensions; migration-upgrade test pattern: `migration_0004_upgrades_a_pre_0004_database` (`connect.rs` :783+) |

## 3. Design

### 3.1 Migration 0015 (storage `migrations/0015_time_tracking.sql`)

```sql
-- 0015_time_tracking: settled time entries (DR-5, design D-T9; ADR
-- 0017). Additive only (ADR 0005). Conventions as in 0004/0008:
-- hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no foreign keys —
-- entries outlive soft-deleted tasks by design (history survives
-- deletion; the reference is the entry's data, not a join).
--
-- There is no intent table and no running-timer row: the open entry
-- (`end` NULL) IS the running timer, persisted like any other row — a
-- daemon restart rehydrates it and the timer "resumes" because it
-- never stopped. `updated_at` is the row's LWW mutation clock for the
-- sidecar lane.
CREATE TABLE time_entries (
    entry_id   TEXT PRIMARY KEY NOT NULL,
    task_id    TEXT NOT NULL,
    task_title TEXT NOT NULL,
    start      TEXT NOT NULL,
    end        TEXT,                -- NULL = open (the running timer)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

Exactly D-T9's DDL — the dead writes at `repo.rs` :1236-1267 bind these
column names. (`end` unquoted is legal SQLite — verified; keep it
unquoted to match the already-written statements.) No backfill: the
table is new and no producer could have written rows before this
migration.

### 3.2 Storage codec: the load path (storage `repo.rs`)

- **Row struct + query**: `TimeEntryRow { entry_id, task_id,
  task_title, start, end, created_at, updated_at }` read via a
  compile-checked `query_as!` (`SELECT … FROM time_entries ORDER BY
  entry_id`, `DateTime<Utc>` casts as in the sibling loads) inside
  `load_inner`'s read transaction, beside the reminder loads.
- **`decode_time_entry(row) -> Result<(TimeEntryId, TimeEntry),
  RepositoryError>`** beside `decode_reminder_rule`:
  `codec::uuid_from_text` for both ids; `DateTime::parse_from_rfc3339`
  for the four stamps; `end` column `NULL` → `None`; any failure →
  `RepositoryError::Corrupted`. **Strict on load, tolerant on the
  wire**: SQLite rows are trusted-written (validated at their
  originating planner, ADR 0017's leniency boundary); a corrupt row is
  a corrupted database, the same contract every other table's decode
  upholds. Span validity is *not* re-checked here (the type documents
  decoded rows as trusted-written; read-side projections clamp).
- **Assemble**: build `BTreeMap<TimeEntryId, TimeEntry>` and replace
  the `TrackingState::default()` placeholder at :849.
- **Convert the dead writes to compile-checked macros**: the R-11
  runtime-checked `sqlx::query` pair (:1236-1267) becomes `query!`/
  `query_as!` style — its stated reason (the table not existing) is
  removed by M1 — and the R-11 partition comment (:933-939) and the
  "dead until chunk DR-5/2's migration" note (:1230-1235) shrink to the
  one fact that remains true (deletes run after upserts so a batch
  carrying both for one id mirrors the sequential last-write-wins).
- **`.sqlx/` regeneration is part of the milestone**: run
  `scripts/sqlx-prepare.sh` and commit the refreshed cache **in the
  same commit** as the query changes (CI fails the freshness check
  otherwise; sqlx-cli must be 0.9 sqlite per the script header).

### 3.3 The sidecar section + row enum (domain `appdata.rs`)

Registry field (the recipe's step 3, board-level — **no per-card
wrapper struct exists because the section has no card grouping**):

```rust
/// Board-level synced time entries, keyed by entry UUID (design
/// D-T10) — the one section whose keys are not card ids: history
/// outlives task deletion, so `prune_absent_cards` must never see it.
#[serde(rename = "time-entries", default, skip_serializing_if = "Option::is_none")]
pub time_entries: Option<BTreeMap<String, TimeEntryRecord>>,
```

Row enum (the recipe's step 1, after `RuleEntry`/`DismissalEntry`):

```rust
pub enum TimeEntryRecord {
    /// `{ "task": "<decimal card>", "title": str, "start": rfc3339,
    /// "end": rfc3339|null, "created": rfc3339, "clock": rfc3339 }`.
    Entry {
        task: Option<String>,   // decimal card id; None decodes tolerant, never emits
        title: String,
        start: DateTime<Utc>,
        end: Option<DateTime<Utc>>,
        created: DateTime<Utc>,
        clock: DateTime<Utc>,
    },
    /// `{ "deleted": true, "clock": rfc3339 }` — a global delete.
    Tombstone { clock: DateTime<Utc> },
    /// A shape this version does not model: preserved verbatim.
    Verbatim(Value),
}
```

- **`clock()` accessor** (`Option<DateTime<Utc>>`, `None` for verbatim)
  — the recipe's step 2, mirroring `RuleEntry::clock`.
- **Decode rules** (hand-written `deserialize_verbatim(raw)`, the
  tolerant boundary — never an error, mirroring `RuleEntry`):
  1. non-object → `Verbatim(raw)`;
  2. `deleted == true` and `clock` parses → `Tombstone`; `deleted ==
     true` with an unparseable clock → `Verbatim`;
  3. otherwise all of `title` (string), `start`, `created`, `clock`
     (rfc3339) must decode → `Entry`; `task` absent/`null` → `None`,
     present string → `Some` (not validated as decimal here — the fold
     treats a non-decimal string as unresolvable), present non-string →
     `Verbatim`; `end` absent/`null` → `None`, present rfc3339 →
     `Some`, present unparseable → `Verbatim` (whole row);
  4. **span validity is not a decode input** — synced rows fold as-is
     (§6's skew ruling; the projections clamp); any residual mismatch
     → `Verbatim`.
- **Serialize** emits exactly the D-T10 key set per arm (`task` always
  emitted by *this* version's rows — decode-only tolerance admits
  `None`; `end` as rfc3339 or `null`), `Verbatim` verbatim.
- **Strictness boundary unchanged**: the envelope plus the known
  section map shapes. A non-object body for the known `time-entries`
  key fails the document exactly like a non-object `reminder-rules`
  body does today — the one-version tolerance every known section
  already has (recorded, D-5).
- Export the new names from `domain/lib.rs` beside the other appdata
  re-exports.

### 3.4 Union (domain `appdata.rs`, the recipe's step 4)

`union()` grows the fourth section arm (`(Some, Some)` clone-and-merge
shape, `(Some, None)/(None, Some)/(None, None)` passthroughs) delegating
to:

```rust
fn union_time_entries(base: &mut BTreeMap<String, TimeEntryRecord>,
                      overlay: &BTreeMap<String, TimeEntryRecord>)
```

Per-key embedded-clock LWW (D-T11): overlay row absent in base → take;
both present → verbatim on either side keeps the slot untouched;
otherwise newer `clock` wins, **equal clocks resolve to the overlay**
(see D-7 — deliberate divergence from `union_rules_entry`'s tie-keeps-
base, matching `union_task_entry` and D-T11's normative text). No
helper struct needed — the section map is flat.

### 3.5 Overlay builder + window filter + unbound skip (domain `appdata.rs`)

```rust
pub fn time_entries_for(
    entries: &BTreeMap<TimeEntryId, TimeEntry>,
    tasks: &BTreeMap<TaskId, Task>,
    board: RemoteBoardId,
    now: DateTime<Utc>,
) -> BTreeMap<String, TimeEntryRecord>
```

Emission filter, all of it fail-closed by construction:

- entry must be **within the retention window**: `entry.start >= now −
  TRACKING_SYNC_RETENTION_DAYS` (aged rows are never re-emitted —
  overlay and lane GC agree on the one window, D-T13);
- `tasks.get(entry.task)` must exist **and** bind to `board`
  (`task.remote` is `Some` with `.board == board`): unbound entries are
  device-local and never emitted; entries of other boards' tasks are
  not this document's; a dangling task id (possible after
  `ClearBoardContent`, D-15) emits nothing;
- **`task.deleted` does not filter**: a soft-deleted-but-still-bound
  task's entries keep flowing so post-deletion adjusts/deletes roam
  (the design's flush-trigger note — soft-deleted tasks keep local rows
  precisely so their ops stay resolvable);
- emitted row: `task: Some(card.get().to_string())` (decimal), `title`
  snapshot, `start`/`end`/`created` verbatim, `clock: entry.updated_at`
  (the LWW mutation clock — the remote stop is just a newer write).

### 3.6 Tombstone derivation — the truncated-view guard (domain `appdata.rs`)

```rust
pub fn derive_time_entry_tombstones(
    base: &BTreeMap<String, TimeEntryRecord>,
    local: &BTreeMap<TimeEntryId, TimeEntry>,
    resolvable_cards: &BTreeSet<String>,
    now: DateTime<Utc>,
    clock: DateTime<Utc>,
) -> BTreeMap<String, TimeEntryRecord>
```

The `derive_rule_tombstones` shape plus the two D-T12 guard clauses.
Derive `Tombstone { clock }` for a base key **iff every clause holds**
(fail-closed conjunction — a narrower view can never erase another
device's history):

1. the base row decodes as a live `Entry` (base tombstones pass through
   untouched — no re-stamp; `Verbatim` is never ordered against);
2. the key parses as a UUID (an unparseable key is not an entry
   identity);
3. the **base row's own `start` is within the retention window** (a row
   the GC is about to strip is not tombstoned — overlay, derivation and
   GC agree on the one window; guard on what the base row itself
   carries, not on the local copy, so the check survives a
   locally-unknown row);
4. the row's `task` card **resolves locally**: present in
   `resolvable_cards` (the flush assembles the set of card ids of
   `board`-bound local tasks from `ctx.tasks` — unresolvable ⇒ neither
   imported by the fold nor deleted here; it ages out instead);
5. **no local entry with that UUID exists** (absence-from-base is never
   an input — only base-present rows missing locally tombstone, so a
   truncated foreign view can never cause a deletion).

The flush stamps derivations with the same stamp the rules derivation
uses (the scoped op stamp — group primary's `queued_at`, or `read_at`
in a rebuild).

### 3.7 Lane GC (domain `appdata.rs`)

```rust
pub const TRACKING_SYNC_RETENTION_DAYS: i64 = 180;

pub fn lane_gc_old_time_entries(document: &AppDataDocument, now: DateTime<Utc>) -> AppDataDocument
```

The `lane_gc_scheduled` shape: clone the document; strip `time_entries`
rows whose `start` (live `Entry` rows) or `clock` (`Tombstone` rows)
falls before `now − TRACKING_SYNC_RETENTION_DAYS`; `Verbatim` rows in
the section pass untouched (never ordered against); an emptied section
becomes `None`. **Wire-only**: nothing here or anywhere else deletes a
local row — `export` reads full local history (D-T13). Open entries get
no exemption (D-11): a 180-day-open entry is out of service by
definition; its local truth is unaffected.

### 3.8 Flush integration (sync `flush.rs`, `actor.rs`)

- `FlushCtx` gains `entries: &'a BTreeMap<TimeEntryId, TimeEntry>`
  (constructed from `persisted.tracking.entries` at `actor.rs` :653).
- **Overlay assembly**, once per flush after the per-card loop (the
  section is board-level — it does not join the card-scoped loop):
  `time_entries_for(...)` into `overlay_time`, then
  `derive_time_entry_tombstones(base.sections.time_entries or empty,
  ctx.entries, &resolvable_cards, ctx.read_at, stamp)` merged in; the
  overlay's `time_entries` field is `Some` iff non-empty.
  `resolvable_cards` = the card ids of `ctx.target`-bound tasks (built
  once from `ctx.tasks`; includes soft-deleted-but-bound).
- **GC chain**: `union → lane_gc_scheduled →
  lane_gc_old_time_entries → prune_absent_cards` (the tracking GC
  beside the scheduled GC; both precede the card prune, order among
  them irrelevant — disjoint sections).
- **The lazy-anchor gates grow the tracking clause**
  (`board_has_local_app_data`, `has_flush_work` — one extra parameter
  pair `entries`/`now`): a board whose only app-data is tracking
  entries must be able to (re)build its anchor when evidence is
  `Absent`/foreign even with no queued ops. Clause: any live board task
  card-bound on `board` with a within-window entry (D-12).

### 3.9 The fold + pipeline call site (domain `merge.rs`, `pipeline.rs`)

```rust
pub struct TrackingFoldChanges {
    pub upserts: Vec<TimeEntry>,
    pub deletes: Vec<TimeEntryId>,
}

pub fn fold_time_entries(
    tracking: &mut TrackingState,
    rows: &BTreeMap<String, TimeEntryRecord>,
    cards: &BTreeMap<String, TaskId>,   // decimal card key → TaskId, board-scoped
) -> TrackingFoldChanges
```

Per row (LWW against local `updated_at`, D-T12):

- **`Entry`**: parse the key UUID (unparseable → skip, kept verbatim in
  the document); resolve `task` (`None` or not in `cards` → skip —
  unresolvable rows are neither imported nor deleted, D-T12); then:
  local absent → adopt the full row (`updated_at = clock`);
  `clock >= local.updated_at` → overwrite `task`/`task_title`/`start`/
  `end`, `updated_at = max(local, clock)` (doc wins pull-side ties —
  the `fold_reminder_rules` comparison, the mirror of the flush's
  tie-to-overlay); older → local stands. Every adopt/update pushes the
  row to `upserts`.
- **`Tombstone`**: parse UUID; if the local row exists and `clock >
  local.updated_at` → remove + report in `deletes` (a delete after an
  adjust wins; a re-created row carries a newer clock and stands);
  tombstones for locally-unknown ids leave no trace.
- **`Verbatim`** never folds.

**No `protected` guard** (D-13): the rules fold's intent flag protects
local edits during the copy-over collapse window; entries have no such
window — they are independent rows, and the dismissals fold's
guard-immunity precedent applies (though with LWW, because entries are
editable).

**Call site** (pipeline §5b, inside `if let Document`, **after** the
per-card rules/dismissals loop — the design's "after dismissals"):

- build `board_cards: BTreeMap<String, TaskId>` from
  `task_by_board_card` filtered to `snapshot.board.id` (only when the
  section is present);
- `folded_tracking: Option<TrackingState>` seeded
  `get_or_insert_with(|| current.tracking.clone())`, exactly the
  `folded_rules` pattern;
- push `PersistenceAction::UpsertTimeEntry`/`DeleteTimeEntry` per delta;
- extend the §7 batch beside `rule_actions`/`dismissal_actions` (batch
  and memory view are the same deltas by construction);
- final state assembly: `tracking:
  folded_tracking.unwrap_or_else(|| current.tracking.clone())`
  (replacing the `current.tracking.clone()` carry-through).

Position notes: the fold runs pre-R3 like the rest of §5b and feeds R3
nothing; resolution uses the pre-R3 card map, so an entry whose card R3
tombstones *this same cycle* still folds — intended (history outlives
the card; the task lands soft-deleted, the entry's reference intact).

### 3.10 Flush-trigger verification + the outbox comment fix

- **Enqueues need no code**: chunk 1's five planner arms plus the
  `SetTaskDone`/`DeleteTask` close side-effects already enqueue
  `UpdateAppData` (verified §2). Stage 2 confirms via the existing
  planner tests — no new enqueue test here ("one behavior, one test,
  lowest layer": the planner tests own it).
- **`outbox.rs` :79-81**: replace "Nothing enqueues it yet — the flush
  lane is chunk 4's." with the reality — every task-touching planner
  (task verbs, reminder verbs, the tracking verbs) enqueues it for each
  touched task, and the flush coalesces the board's ops into the single
  sidecar document upload (D-S5). Comment-only; no behavior.

## 4. Decision summary (every open question, resolved)

| ID | Question | Decision | Status |
|---|---|---|---|
| D-1 | Migration number | **0015** (dispatch's 0013 taken by DR-3/2's insistence, 0014 by DR-4/R's ref-delete payload; next free at base `f67c9bc0`, verified by directory listing). Dispatch §2's landing-order rule explicitly overrides its own table; downstream note for the dispatch owner: DR-6/3 (claimed 0014) and undo/1 (claimed 0015) re-verify at their own bases per the same rule | **recorded deviation** |
| D-2 | DDL shape | D-T9's DDL verbatim (the dead writes bind these column names; `end` verified legal unquoted); no FKs, no backfill, header comment carries the open-row-is-the-intent rationale | recorded |
| D-3 | Load-side strictness | Strict decode → `RepositoryError::Corrupted` on any unparseable stored row; span validity not re-checked (trusted-written rows, ADR 0017's leniency boundary; projections clamp). The tolerant layer is the sidecar, not the DB | recorded |
| D-4 | Query style + offline cache | Convert DR-5/1's runtime-checked time-entry writes to compile-checked macros (their reason evaporates) and load via `query_as!`; regenerate `.sqlx/` with `scripts/sqlx-prepare.sh` **in the same commit** (CI freshness gate) | recorded |
| D-5 | Section body tolerance | `time_entries` is a bare `Option<BTreeMap<String, TimeEntryRecord>>` (board-level, no wrapper struct — there is no sibling-key namespace to preserve); row level fully tolerant via `deserialize_verbatim`; a non-object section body fails the document exactly like every existing known section (the envelope remains the strict boundary; one-version tolerance as for `reminder-rules`) | recorded |
| D-6 | Row decode rules | §3.3's matrix: tolerant, whole-row demotion to `Verbatim` on any unmodelable field; `task` absence tolerated at decode (never emitted); span validity is not a decode input (skewed synced rows fold as-is; §6 ruling) | recorded |
| D-7 | Union tie-break | **Ties resolve to the overlay** per D-T11's normative text and `union_task_entry`'s precedent — a deliberate divergence from `union_rules_entry`/`union_dismissals_entry` (ties keep base); convergence holds under either uniform choice, and the flush's "just-stamped local writes" rationale (D-S4) favors overlay; comment states the divergence | recorded |
| D-8 | `prune_absent_cards` | **No arm** — the section's keys are entry UUIDs, invisible to card-keyed pruning; history outlives deletion. An arm here is the stage-3 blocking finding | **normative (D-T11)** |
| D-9 | Emission filter | §3.5: within-window ∧ card-bound on the document's board; `task.deleted` does not filter (post-deletion adjusts/deletes must roam); unbound/foreign/dangling → skip (device-local or not ours) | recorded |
| D-10 | Tombstone derivation guard | §3.6's five-clause fail-closed conjunction (live ∧ UUID ∧ within-window(base row) ∧ card-resolvable ∧ locally-absent); base tombstones pass through; stamp = the scoped op stamp like rules | recorded |
| D-11 | Retention window edge | Uniform window, **no open-entry exemption** from the GC/overlay filter: a 180-day-open entry is out of service by definition; local truth unaffected; uniformity is what makes overlay and GC agree (the mixed-fleet stale-device re-emit stays the accepted one-version tolerance) | recorded |
| D-12 | Flush gates | `FlushCtx` carries the entries map; the GC chains after `lane_gc_scheduled`; `board_has_local_app_data`/`has_flush_work` grow the tracking clause so a tracking-only board can (re)build its anchor | recorded |
| D-13 | Fold guard | **No `protected` parameter** — no copy-over concept exists for entries; LWW against `updated_at` is the whole story (dismissals' guard-immunity precedent, edits reintroduce the clock) | recorded |
| D-14 | Fold call site | Pipeline §5b, inside `if let Document`, after the per-card loop (board-level, once per document, after dismissals); board-scoped card map built on demand; deltas ride the §7 batch and the memory view symmetrically | recorded |
| D-15 | `ClearBoardContent` | Does **not** prune `time_entries` — the repair's contract stays as landed; entries are board-less rows whose identity is the entry UUID + task reference, and dangling task refs are already representable (history outlives every deletion path, D-T13's local side) | recorded |
| D-16 | Flush-trigger enqueues | Already landed in chunk 1 (§2 verification table); no new enqueue code or duplicate tests — the planner tests own the behavior | recorded |
| D-17 | Aged-row import at fold | The fold imports what the document carries, window-free: a stale device's aged row pulls in, joins local history (which never drops), and the next round-device flush GCs it from the wire — no resurrection loop (the overlay never re-emits aged rows) | recorded |
| D-18 | Insta lane | Not a shape-widener: no `AppState`/`PersistedState` field changes (chunk 1 widened the shape); existing fixtures carry no `time-entries` section, so zero snapshot churn is expected — stage 2 verifies rather than regenerates | recorded |
| D-19 | R-11 stub comments | Removed/reworded once the table exists — the "dead until DR-5/2" notes become the one true fact (delete-after-upsert ordering) | recorded |

## 5. TDD test plan (origin §5's hooks, milestone-ordered)

Every test obeys AGENTS §5: typed/structural outcomes, no text or log
assertions, no sleeps, proptests via `test_support` strategies, floors
over garbage input only, one behavior per test at its lowest layer.

**M1 — migration + storage codec** (storage `connect.rs` tests,
`tests/roundtrip.rs`):

- `migration_0015_upgrades_a_pre_tracking_database`: the
  `migration_0004_upgrades_a_pre_0004_database` pattern — build the
  pre-0015 schema (real migrations 0001-0014), `open()` replays 0015,
  `PRAGMA table_info(time_entries)` names exactly the seven columns
  with `entry_id` the primary key, and `load()` folds an empty table
  into an empty `TrackingState`.
- `time_entry_rows_round_trip_including_the_open_row`: repository
  apply → load with a closed and an open (`end: NULL`) entry; loaded
  map equals the applied rows (typed equality; the open row's `end`
  column reads back `None`).
- `a_batch_carrying_upsert_then_delete_for_one_entry_lands_the_delete`:
  both actions in one `apply` — the delete-after-upsert ordering pins
  the last-write-wins contract (extends the dead-write comment's claim
  into a test).
- Extend `actions_from_state` (tests/common) with the entry-upsert loop
  and the P1 proptest with `prop_assert_eq!(loaded.tracking,
  state.tracking)` — the shared strategy already generates
  referentially-consistent rows, so the full-state roundtrip covers
  arbitrary entries in one go.
- Run `scripts/sqlx-prepare.sh`; commit `.sqlx/` with the query changes.

**M2 — section + row serde** (domain `appdata.rs` tests):

- `time_entry_rows_round_trip_and_unknown_shapes_stay_verbatim` (the
  dismissal twin's shape): decode matrix pins §3.3 — full Entry,
  null-task Entry, open row (`end: null`), tombstone, and the demotions
  (missing title, bad clock, bad end, non-object row, `deleted: true`
  with bad clock) each land as `Verbatim` with value equality across
  decode→encode.
- `the_time_entries_section_serializes_exactly_the_modeled_keys`:
  emitted Entry rows carry exactly
  `task/title/start/end/created/clock`, tombstones exactly
  `deleted/clock`.
- The generic unknown-section and envelope proptests already cover
  mixed-fleet relay of the *whole* section by old devices — no
  duplicate (one behavior, one test).

**M3 — union** (domain `appdata.rs` unit + proptest):

- `union_time_entries_picks_newer_clocks_ties_to_the_overlay_and_keeps_verbatim`:
  newer-wins both directions, **tie → overlay** (the D-7 pin),
  `Verbatim` never displaced, base-only rows survive, all four
  `(Some/None)` section arms.
- `union_time_entry_tombstones_compete_by_the_same_clock`: a newer
  tombstone beats a live row; a later-dated live row beats an older
  tombstone (both directions of the delete-vs-adjust race).
- Proptest `time_entry_union_lww_laws` (the `dismissal_union_lww_laws`
  shape, arbitrary row maps): idempotence
  (`union(d, union(d, o)) == union(d, o)`), convergence
  (union is associative-commutative up to the tie rule's overlay bias —
  assert the standard three-document identity the dismissal laws use),
  open→closed transitions riding as newer writes.

**M4 — overlay builder + derivation** (domain `appdata.rs` unit +
proptest):

- `time_entries_emission_is_card_bound_only_and_window_filtered`: emits
  card-bound entries keyed by UUID with decimal `task` strings;
  unbound/foreign-board/dangling-task entries never emitted;
  soft-deleted-but-bound emitted; out-of-window `start` never emitted;
  the boundary instant itself is inside (inclusive).
- `tombstone_derivation_tombstones_exactly_the_locally_absent_live_entries`
  plus the guard clauses: base tombstone untouched, verbatim untouched,
  out-of-window base row not tombstoned, unresolvable card not
  tombstoned, locally-present not tombstoned.
- Proptest `tombstone_derivation_never_deletes_from_a_truncated_view`:
  for arbitrary base maps and resolvable sets, every derived tombstone's
  key decodes as a live base row, parses as a UUID, lies within the
  window, resolves, and is locally absent — and
  `no_local_or_unbound_ids_reach_the_time_entries_wire` (the
  `no_local_ids_reach_the_dismissals_wire` tripwire twin): every
  emitted key parses as a UUID; every emitted `task` is a decimal
  string that names a `board`-bound card.

**M5 — lane GC + flush** (domain `appdata.rs` + sync `flush.rs` tests):

- `lane_gc_drops_only_aged_time_entries`: fresh live rows and fresh
  tombstones survive; aged ones (both arms) go; verbatim passes;
  other sections untouched (the `gc_keeps_present_cards…` shape); an
  emptied section becomes `None`; boundary inclusive.
- Proptest `gc_touched_only_time_entries`: for arbitrary documents,
  GC changes exactly the `time_entries` section.
- `the_overlay_never_re_emits_what_the_gc_strips`: builder + GC over
  the same `now` agree — no aged row in either output; union of an
  aged base row with a fresh overlay GCs the row (the stale-device
  tolerance self-heals in one flush).
- Flush assembly tests (the sync crate's existing flush fixtures):
  the overlay carries the section; derived tombstones join it; the GC
  chain runs; `has_flush_work` fires the tracking clause for an
  otherwise app-data-less board with bound entries.

**M6 — fold + pipeline** (domain `merge.rs` + `pipeline.rs` tests):

- `fold_time_entries_imports_updates_and_tombstones_by_the_clock`:
  adopt-unknown, update-newer (`>=` ties to the document — the pull
  side), local-stands-older, strictly-newer tombstone removes,
  older-tombstone leaves the re-created row, unknown-id tombstone
  leaves no trace, verbatim/null-task/unresolvable-card rows skip
  without appearing in the changes.
- `the_pipeline_folds_time_entries_after_dismissals`: one document
  carrying a dismissal row and entry rows — both land (the position
  pin); the resulting batch carries the `UpsertTimeEntry`/
  `DeleteTimeEntry` actions and the memory view equals the folded
  deltas (batch and view identical by construction).
- `an_entry_whose_card_vanishes_this_cycle_still_folds`: resolution
  against the pre-R3 card map (the §3.9 position note, pinned).
- `a_null_task_wire_row_is_skipped_entirely` — neither imported nor
  deleted (D-T12's fail-closed fold side).

**M7 — comment fix + gates**: the outbox comment change carries no test
(comment-only); run the full AGENTS §10 suite through
`scripts/run-quality-gates.sh`; `cargo deny` untouched (no new deps).

## 6. Files touched & estimated diff size

| File | Change | ~LOC |
|---|---|---|
| `crates/taskboard-storage-sqlite/migrations/0015_time_tracking.sql` | new | +18 |
| `crates/taskboard-storage-sqlite/src/repo.rs` | `TimeEntryRow` + load query + `decode_time_entry`; writes → macros; R-11 comments reworded | +55 / −25 |
| `crates/taskboard-storage-sqlite/src/connect.rs` | upgrade test | +55 |
| `crates/taskboard-storage-sqlite/tests/roundtrip.rs` + `tests/common/mod.rs` | P1 extension + targeted codec/order tests | +90 |
| `.sqlx/` | regenerated cache (mechanical) | ± |
| `crates/taskboard-domain/src/appdata.rs` | section field, `TimeEntryRecord` + serde, union arm + helper, `time_entries_for`, `derive_time_entry_tombstones`, `TRACKING_SYNC_RETENTION_DAYS` + `lane_gc_old_time_entries`, tests/proptests | +340 |
| `crates/taskboard-domain/src/lib.rs` | re-exports | +6 |
| `crates/taskboard-domain/src/merge.rs` | `TrackingFoldChanges` + `fold_time_entries` + tests | +140 |
| `crates/taskboard-domain/src/pipeline.rs` | §5b call site, batch/final-state wiring, tests | +85 |
| `crates/taskboard-domain/src/outbox.rs` | stale comment fix | −3 / +5 |
| `crates/taskboard-sync-nextcloud/src/flush.rs` | `FlushCtx.entries`, overlay assembly, GC chain, gate clause | +55 |
| `crates/taskboard-sync-nextcloud/src/actor.rs` | `FlushCtx` construction + `has_flush_work` args | +8 |
| `crates/taskboard-sync-nextcloud` tests | flush assembly/gate tests | +70 |

**Estimated total: ≈ 900 added / 30 removed lines** (tests ≈ half), M-size
as the dispatch table says. No new dependencies; `persistence.rs`,
`tracking.rs`, `messages.rs`, `command.rs`, the UI crate: zero diff.

## 7. Risks & mitigations

- **`.sqlx` freshness breaks CI** (the most likely stage-2/3 failure):
  any compile-checked query change requires `scripts/sqlx-prepare.sh`
  (sqlx-cli 0.9 sqlite) with the cache committed in the same commit.
  Fallback if the toolchain is unavailable locally: keep the load query
  runtime-checked like the writes (D-4's weaker sibling) and say so in
  the handoff — CI still passes, the style conversion lands later.
- **Union tie-break divergence (D-7) read as a bug**: the divergence
  from the rules/dismissals helpers is deliberate and pinned by test;
  the comment names it so stage 3 reviews the decision, not the
  surprise.
- **Fold resurrects aged rows?** No: fold imports what the wire
  carries (D-17); the wire is bounded by the GC on every current
  writer; a stale-device row self-heals in one round-device flush. The
  proptest pair (M5) pins both halves.
- **Tombstone guard regression to delete-on-absence** (the exact bug
  class the dismissals wave fixed): the five-clause conjunction plus
  `…_never_deletes_from_a_truncated_view` is the stage-3 focus; an
  unguarded derivation is a blocking finding.
- **`board_has_local_app_data` signature churn** touches two call
  sites and their tests — mechanical, but the sync crate's flush
  fixtures must grow the entries argument; keep the clause one
  readable predicate.
- **Migration collision drift** (undo/1 also eyes 0015): recorded in
  D-1; their stage-1 re-verifies next-free at its own base — no
  coordination action needed from this chunk beyond the record.

## 8. Stage-2 execution order (TDD pairs → standalone green commits)

1. M1 storage: migration + upgrade test → codec/load + roundtrips →
   write-side macro conversion + `sqlx-prepare` (one commit per pair;
   the migration commit is standalone-green by itself).
2. M2–M3 appdata: section + serde matrix → union arm + laws.
3. M4: builder + derivation + guard proptests.
4. M5: lane GC → flush integration + gate clause.
5. M6: fold + changes struct → pipeline call site + position pins.
6. M7: outbox comment; full gate suite through
   `scripts/run-quality-gates.sh`.

Each commit standalone (rebase-merge hygiene): correct build, meaningful
`<type>(<module>)` subject. Suggested subjects: `feat(storage):
migration 0015 time entries and the load codec`, `feat(domain): the
time-entries sidecar section`, `feat(domain): the time-entry union and
tombstone derivation`, `feat(domain): time entry retention aging`,
`feat(sync): the flush carries the time-entries section`,
`feat(domain): the time-entry document fold`, `docs(outbox): the
UpdateAppData enqueue reality`.

## 9. Handoff notes

- Stage 2 implements per this plan; deviations get recorded decisions
  here-inheriting IDs (D-20…) in the PR description. Do not push; do
  not open the PR (stage 3 ships).
- Stage 3's functional focus is dispatch §10.2 verbatim, plus this
  plan's D-7/D-8/D-10/D-15 as the review-tripwire decisions.
- The worktree stays for stage 2 (no teardown at stage-1 handoff).
- Issue #487 stays open for stage 3 to close with the PR.
