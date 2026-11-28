# The Habit Event SQLite Migration Chunk Plan — Chunk 3 of the Habit Event-Log Dispatch

- **Date**: 2026-11-28
- **Kind**: stage-1 (planning) artefact for chunk 3 ("sqlite migration",
  branch `feat/storage-habit-event-migration`) of the #1519 habit
  event-log dispatch.
- **Origins (normative)**:
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` §0 (Rulings A,
  F, I — all accepted on defaults 2026-11-27), §6 (schema,
  load-then-remap, marker, rewind); the dispatch plan
  `.artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md` §1/§4;
  ADR 0005 (shipped migrations immutable once shipped, checksummed by
  sqlx); the granularity audit §F-1
  (`.artifacts/reports/2026-11-27-source-of-truth-granularity-audit.md`);
  the UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md` (US-4/S2,
  token `us4_s2`); the chunk-2 plan
  `.artifacts/plans/2026-11-27-habit-event-commands-plan.md` (D-C the
  derived mirror, D-P the one `UpsertHabitEvent` action + the inert
  sqlite partition arm, D-R the remap contract);
  `docs/testing_strategy.org` §6 (clock discipline).
- **Operator rulings recorded on issues** (2026-11-27): #1549 option 3 —
  the over-quota audit limit is accepted and the **migration
  consequence** (Ruling A1's synthesized rows inherit the limit) must be
  documented (§3 M-1549 below writes it); #1542 — the derived legacy
  mirror is temporary, removed in chunk 5 (this chunk keeps it alive on
  disk; §1 D-F).
- **Worktree**: `/home/viktoria/src/taskboard-WT-feat-storage-habit-event-migration`
  on branch `feat/storage-habit-event-migration` at `origin/main`
  `192b9f72` (worktree-doctor passed; created by the dispatcher per
  AGENTS §8; chunk 2 landed via PR #1556). The landed code on
  `origin/main` supersedes the origin plans where they differ — cited
  accordingly.
- **Scope**: migration `0032`; the one-shot load-then-remap pass with
  the migration marker; flush/load/rewind on `habit_event`; the uuid
  `v5` feature add; the deterministic legacy-id recipe (domain, single
  source for chunk 4's shim). **Non-goals** (dispatch §4): no wire work
  (chunk 4); no UI/CLI; no domain *command* changes; the legacy
  `habit_log`/`habit_day_log` **tables** stay on disk (Ruling F; their
  mirror writes and load reads also stay — §1 D-F, the one recorded
  deviation from design §6/F's "load stops reading" sentence, with
  reason).

---

## 0. Rulings requested (operator approves this chunk's merge personally)

This chunk is a persisted-data migration: everything below is decided
and the plan is dispatchable on the recorded defaults; the operator can
overrule any item before stage 2 without invalidating the rest. Each
item names what changes if overruled.

- **RQ-1 — what "dormant" means for the legacy tables (Ruling F's
  execution)**. *Decision (recommended default)*: from 0032 on, sqlite
  **still persists the mirror** (`RecordHabitLog`/`RecordHabitDayLog`
  flush arms stay) and load **still reads** the legacy tables into
  `HabitState.log`/`day_log`, exactly as today; the event table is
  added beside them; nothing ever consults the legacy tables *as the
  events source* after the marker. Reason: the mirror's readers (UI
  heatmap/badges, CLI reads) are chunk 5's re-key, and the chunk-2
  keystone law (`log` ≡ `derive_window_counts`) must survive a restart
  on any intermediate build — stopping the mirror writes now would
  break the keystone law across a restart (the legacy rows on disk
  would freeze while the derived truth moves). Chunk 5 deletes the
  mirror writes, the load reads, and (per Ruling F's text) the *code*
  references — the tables themselves stay as inert schema history
  forever (ADR 0005). *If overruled* (hard-stop now): the restart
  keystone break must be accepted and chunk 5's re-key pulled forward;
  the dispatch plan's chunk boundaries change.
  - **Rewind's treatment of the legacy tables**: a rewind-to-31 fixture
    drops `habit_event` + the marker table (derived strips) and the
    legacy tables keep their rows — pre-upgrade rows replay the remap
    on re-open; post-upgrade mirror rows (derived from events) replay
    a remap that synthesizes their events *again* — but identically?
    No: post-upgrade mirror rows carry fresh `mint_log_clock` stamps,
    so their v5 ids differ from the live v7 events' ids. **Resolved by
    the marker's own semantics**: a rewind *below* 32 that keeps
    post-upgrade legacy rows is a fixture-only shape (real downgrades
    never un-apply migrations); the rewind fixture strips the legacy
    tables' post-upgrade rows is NOT needed — the rewind test corpus
    uses pre-upgrade legacy rows only, and the plan records that a
    real downgrade (never un-applying) keeps the marker row, so the
    remap never re-runs. No action beyond the tests.

- **RQ-2 — the marker is set on first load with this binary, even when
  there are no legacy rows**. *Decision (recommended default)*: the
  one-shot pass runs whenever the marker row is absent; it synthesizes
  iff legacy rows exist, and **always** inserts the marker. A fresh
  database (never had habit rows) gets the marker with zero events —
  the pass then costs one `SELECT`-exists per boot forever after, and
  a user whose first-ever habit log happens *after* the upgrade can
  never be mistaken for legacy data. *If overruled* (marker only when
  legacy rows existed): the "user logs mid-upgrade" window widens from
  "between two loads" to "between upgrade and first load", with the
  same guard (empty event store + marker absent); strictly weaker —
  rejected.

- **RQ-3 — the legacy `skip` bit becomes one `Skip` mark event**. The
  design §6.2 names only count rows; Ruling B (skip is user-asserted
  intent, not derivable) and Ruling F-2's dissolution require carrying
  it. *Decision (recommended default)*: a legacy window row with
  `skip = 1` synthesizes its N completion events **plus one**
  `HabitEventKind::Skip { window }` mark stamped at the same instant
  the completions stamp (twin-clock refinement applies to the mark
  too), id = the v5 recipe's mark leg (`…|skip`, no ordinal). `skip = 0`
  synthesizes no `Unskip` (absence ≡ unskipped, the derived posture's
  law). *If overruled*: skip postures die at migration — user-visible
  loss; rejected on Ruling B's own grounds.

- **RQ-4 — twin rows contribute instants, never counts**. The
  `habit_day_log` overlay is a display projection of the same
  completions (the transactional-twin law kept them in lockstep).
  *Decision (recommended default)*: for a Day-kind habit's window row
  with count N, an existing twin row for the same calendar day
  changes only the **stamp instant** (the twin's clock per Ruling A1's
  refinement); the window row is the sole count authority — exactly N
  events synthesize, never 2N. A twin whose day differs from the
  window's start date, a twin for a non-Day-kind habit, and an orphan
  twin (no window row) contribute **nothing** (each is a recorded
  outcome with a test — §4). *If overruled*: any twin-as-count reading
  double-counts by construction (the twin counted the same
  completions); no alternative exists.

- **RQ-5 — the remap folds legacy rows without joining `habits`**
  (deleted habits included). *Decision (recommended default)*: every
  `habit_log` row synthesizes regardless of whether its habit row
  still exists or is tombstoned — 0017's own law ("the log outlives
  its tracker; a habit_log row is data, not a join") and the D-H6
  fail-closed posture (foreign/orphan events are inert at derivation)
  make this the faithful choice, and it keeps the remap a pure row
  fold. *If overruled*: habit-filtered synthesis loses data on
  tombstoned habits (restore would find nothing); rejected.

- **RQ-6 — #1549's migration consequence (already ruled, option 3)**:
  documented, not re-opened. The full statement and where each copy
  lives: §3 M-1549. Listed here because the operator asked the
  Rulings-requested section complete: **the ruling stands** — the only
  ask is the documentation, which this chunk delivers in three places
  plus a chunk-5 pointer.

---

## 1. Recorded decisions

### D-32 — Migration `0032_habit_events.sql`

Verified: the latest shipped migration is **`0031_habit_time_entries.sql`** —
`0032` is genuinely the next free number. DDL (design §6.1 verbatim,
plus the marker table; the rewind grammar's recognized shapes only —
see D-RW):

```sql
-- 0032_habit_events: the per-completion habit event log (#1519
-- chunk 3; design §6.1). Additive only (ADR 0005 — never edit a
-- shipped migration). One row per completion or skip-intent mark;
-- uuid primary key; no LWW columns — the union fold arbitrates
-- nothing and the state engine holds the write side.
--
-- MIGRATION CONSEQUENCE (operator ruling on #1549, option 3,
-- 2026-11-27): the load-time remap synthesizes events from the
-- legacy habit_log rows at each row's clock (Ruling A1). Under the
-- event grammar those synthesized events re-derive at read time, so
-- a legacy row whose audited count exceeds the habit's quota
-- (the over-quota AdjustHabitLog valve, #1549) migrates to a derived
-- count capped at the quota, with the surplus re-attributed into the
-- unmet predecessor window — the limit the event grammar accepted on
-- #1549 now applies to migrated data. Recorded here, at the remap
-- site, in the chunk-3 plan; chunk 5 carries the user-facing note
-- (see .artifacts/plans/2026-11-28-habit-event-migration-plan.md §3).
CREATE TABLE habit_event (
    id         TEXT PRIMARY KEY NOT NULL,  -- event uuid (hyphenated)
    habit_id   TEXT NOT NULL,
    kind       TEXT NOT NULL,              -- 'completion' | 'skip' | 'unskip'
    window     TEXT,                       -- mark provenance date; NULL for completion
    at         TEXT NOT NULL,              -- rfc3339 (UTC, nanos form)
    au_by      TEXT,                       -- author (0023/0026 column grammar)
    deleted_at TEXT,                       -- tombstone (Ruling C2) — rides the row
    deleted_by TEXT
);
CREATE INDEX idx_habit_event_habit_at ON habit_event (habit_id, at);

-- The one-shot remap marker (design §6.2): a row here means "the
-- load-then-remap pass ran with a 0032-aware binary" — the pass
-- never re-runs after the user logs new events mid-upgrade. The
-- migration creates the table empty; the remap inserts the row (data
-- belongs to Rust, not to an immutable SQL file).
CREATE TABLE habit_event_migration (
    done INTEGER NOT NULL CHECK (done = 1)
);
```

Notes:
- Both `CREATE TABLE` statements are in the rewind grammar's
  invertible shape (derived `DROP TABLE` strips); the `CREATE INDEX`
  classifies `Unstrippable` — the `rewind_table_covers_the_ledger`
  canary's pinned unstrippable-carrier list gains `(32, 1)` (a planned,
  cited test edit; D-RW).
- No foreign keys (the 0017 posture verbatim: events outlive their
  habit row by design; deleted-habit rows are data, RQ-5).
- Column spellings follow the existing conventions: `au_by` (0023's
  author-column grammar), rfc3339 TEXT stamps, hyphenated-uuid TEXT.
- The tombstone rides the row (Ruling C2; no separate deletion family
  — chunk 2's `UpsertHabitEvent` carries `deleted: Option<_>`
  verbatim).

### D-ID — the deterministic legacy-id recipe (Ruling I; domain, single-sourced)

 uuid `v5` feature lands in the root `Cargo.toml` workspace pin (its
 own commit):

```toml
uuid = { version = "1", features = ["v4", "v5", "v7", "serde"] }
```

The recipe lives in `crates/taskboard-domain/src/habit_event.rs` (pure
— v5 is `sha1` + name, no IO; the domain-crate discipline holds; uuid's
`v5` feature pulls only `sha1_smol`), as the **single source** chunk 4's
wire shim imports (the chunk-4 plan's "import it, never re-declare"):

```rust
/// The fixed namespace for every deterministically synthesized
/// legacy habit event (the sqlite remap here, the wire legacy fold in
/// a later chunk). Version-tagged recipe: bump the suffix to change
/// the recipe once, deliberately.
pub const LEGACY_HABIT_EVENT_NAMESPACE: Uuid =
    Uuid::from_u128(0x7461_736b_6261_6f72_6c65_6761_6379_0001); // "taskboard|legacy|1"

/// Ruling I: `Uuid::new_v5(ns, "legacy-habit-log|<habit uuid>|<window
/// date YYYY-MM-DD>|<at rfc3339 nanos UTC>|<ordinal>")` for
/// completions; the mark leg appends `|skip` instead of an ordinal
/// (RQ-3). The `at` wire form is fixed at
/// `to_rfc3339_opts(SecondsFormat::Nanos, true)` — always UTC, always
/// 9 fractional digits — so the key string is byte-stable across
/// platforms and clock grammars (legacy clocks are whole-second per
/// the #1095 invariant; the nanos form is deterministic for any
/// input). Mint-at-trust-boundary (A1): the remap and the wire shim
/// are the only callers; `HabitEventId::from(Uuid)` is the
/// sanctioned constructor (the landed type's doc names this exact
/// use).
pub fn legacy_habit_event_id(habit: HabitId, window_date: NaiveDate,
    at: DateTime<Utc>, ordinal: Option<u32>) -> HabitEventId
```

**Cross-device consequence (explicit, per the design's intent)**: two
devices holding the *same* legacy row mint **the same** id — that is
the design saying so (Ruling I: "deterministic across devices"), and it
is what makes the chunk-4 union fold converge instead of double-counting
identical history. The flip side, recorded as a risk (R2): legacy rows
whose *clocks* differ across devices (possible once chunk 4's shim folds
wire rows that won an LWW duel differently per device) mint distinct ids
for "the same" completions — the union keeps both and the derived count
is inflated until the ≤ 180 d retention ages the legacy rows out
(Ruling D1's accepted transitional semantics; sqlite itself is
device-local and never has this problem — it is a chunk-4 wire-shim
risk that this recipe's contract text must name).

**Idempotence proof** (the property the tests pin): the id is a pure
function of recorded facts (habit uuid, stored window date, stored
clock, ordinal); the same input rows yield the same key strings, hence
the same v5 UUIDs, hence the same `HabitEvent` values — the remap run
twice over an unchanged database is byte-identical; and re-execution is
additionally made a no-op by (a) `INSERT OR REPLACE` by primary key and
(b) the marker making a second run unreachable (D-MK). The one
non-pure input would be iteration order — eliminated: rows are read
`ORDER BY habit_id, window_start` / `ORDER BY habit_id, day` and the
ordinal is assigned in that order.

### D-REM — the remap algorithm (Ruling A1)

At `load_inner`, after migrations, inside the load transaction's read
scope; implementation lives in `crates/taskboard-storage-sqlite/src/repo.rs`
(a private `remap_habit_events_if_needed` + the decode helpers): the
storage crate owns its migration, and the fold needs no habit config —
it is a pure row fold plus the domain recipe (SRP: no domain logic
moves; the derivation fold never leaves the domain crate).

1. **Gate**: `SELECT EXISTS(SELECT 1 FROM habit_event_migration)` — a
   marker row present → return (never re-consult legacy again). Absent
   → the pass runs (RQ-2: it runs on every 0032-aware first load,
   legacy rows or not).
2. **Read** `habit_log ORDER BY habit_id, window_start` and
   `habit_day_log ORDER BY habit_id, day` (the existing load queries'
   shape; unchecked queries per the 0030 precedent).
3. **Synthesize** per window row `(habit_id, window_start, count N,
   clock, au_by, skip)`:
   - stamp instant: the row's `clock`, decoded strict (the existing
     `decode::clock`) — **unless** the twin refinement applies (step
     4);
   - N `Completion` events (`N = count`; N = 0 synthesizes zero events
     — the count-0 row is inert, recorded), ordinals `0..N` in read
     order, `by = decode::author(au_by)`, `deleted = None`;
   - `skip = 1` → plus one `Skip { window: WindowStart::from_stored(
     stored date) }` mark at the same stamp instant, the recipe's mark
     leg (RQ-3). The stored window decodes as-is, never floored (the
     D-H12 decode-seam posture the load path already applies — former
     kind rows migrate as data and re-derive under the current config
     at read time; the id key uses the **stored** date verbatim).
4. **Twin refinement (Ruling A1 + RQ-4)**: applies only when the
   habit's current target kind is Day — but the remap is a pure row
   fold with no habit config (RQ-5's no-join law). **Resolution (recorded
   deviation from design §6.2's letter, same behavior)**: the refinement
   is applied by *day geometry*, not by joining the habit's config: a
   Day-kind habit's canonical window row has `window_start` equal to a
   single calendar day, and its twin row has `day` equal to that same
   date. So: if a twin row `(habit_id, day == window_start)` exists,
   its clock is the stamp instant for that row's events and its skip
   mark; Week/Month rows (whose twin-day equality never coincides — a
   week's start date is not a row the overlay writes for that window)
   and twin-less Day rows stamp at the row clock. Consequences named:
   (a) a Week row that *coincidentally* has a twin with `day ==
   window_start` cannot occur (the overlay writes only
   `day.today(log-now)` values — a week start date can collide only if
   the user logged at exactly a week boundary *and* the window is the
   current week; the collision is accepted: the twin clock is recorded
   reality about those completions' day in that case too — the
   refinement degrades gracefully). (b) No `habits` join = a Day-kind
   habit is detected by its data, not its config; a former-Day habit's
   legacy rows refine the same way (their twins exist) — harmless: the
   day attribution the twin carries stays true.
5. **Twin-orphan and twin-divergence outcomes** (each a test, §4): a
   twin with no matching window row contributes nothing (instants
   without a count authority); a twin whose `day != window_start`
   contributes nothing; the twin's `count` column is **never** read
   (RQ-4: the overlay is a projection of the same completions).
6. **Write**, in one transaction: `INSERT OR REPLACE` every
   synthesized event into `habit_event` (by uuid), then
   `INSERT INTO habit_event_migration (done) VALUES (1)`. The
   transaction commits atomically — events-without-marker is an
   unreachable persisted state (crash = rollback = both absent = the
   pass re-runs idempotently, D-ID's proof).
7. **Load continues**: the synthesized events decode through the same
   path as step D-LD and enter `HabitState.events` on this very load
   (no restart needed to see them).

**No-data-loss invariant (§4's tests pin it)**: every legacy window row
of count N becomes exactly N live completion events (+ ≤ 1 mark), or
lands in one of the recorded no-mint outcomes (count-0 row; orphan or
mismatched twin — instants-without-count, not lost counts). The legacy
rows themselves are never deleted or mutated by the remap (Ruling F:
inert history).

### D-MK — marker discipline (never re-runs; downgrades)

- **Mid-upgrade logging**: the pass runs at load; between the marker
  commit and the next load the user may log — those are v7 events
  written through `UpsertHabitEvent`, distinct ids, never re-synthesized
  (the marker forbids the pass; even a hypothetical second pass would
  mint the same v5 ids for the same legacy rows and `INSERT OR REPLACE`
  would fold them harmlessly — belt to braces).
- **Mirror rows after the marker** (RQ-1's dormant semantics): the
  mirror keeps writing `habit_log`/`habit_day_log` rows derived from
  events; those rows are **never** re-consulted by the remap (marker)
  and never read as the events source. They exist solely to keep
  `HabitState.log` truthful across restarts until chunk 5 (the D-C
  keystone law), exactly as on a chunk-2 main.
- **Downgrade/rollback**: a database with the marker, `habit_event`
  rows, and an *older* binary (no un-apply — sqlx migrations never
  reverse): the old binary ignores `habit_event` (unknown tables are
  invisible to it), reads/writes the legacy tables as it always did —
  functional with data as-of-upgrade plus any post-upgrade mirror
  writes; on re-upgrade the marker forbids a second remap, and every
  post-upgrade completion already exists as an event (each mirror write
  rode its event's `UpsertHabitEvent`). No duplicate, no loss. The
  plan records this as the contract; no code implements downgrade
  behavior (it falls out of the two invariants).

### D-LD — load on the event table

`load_inner` gains (after the `habit_time_entries` block, same
unchecked-query posture — "a fresh table need not re-seed the sqlx
offline metadata", the `habit_day_log`/`roaming_settings` precedent):

```rust
type HabitEventRow = (String, String, String, Option<String>, String,
                      Option<String>, Option<String>, Option<String>);
let habit_event_rows: Vec<HabitEventRow> = sqlx::query_as(
    r"SELECT id, habit_id, kind, window, at, au_by, deleted_at, deleted_by
        FROM habit_event ORDER BY id",
)...
```

Decode: uuid via `codec::uuid_from_text` (`MalformedUuid` on failure,
`PersistedTable::HabitEvent` row-key attachment — the new table joins
the `PersistedTable` enum additively with a doc line); kind string →
`Completion`/`Skip`/`Unskip` (unknown kind string = a corrupt row:
`DecodeStage::RowBuild`, typed `DecodeReason` — fail-closed, never
demote to a completion); `window` decodes `from_stored` (provenance
only); `at` via `decode::clock`; tombstone legs →
`Option<HabitEventTombstone>` (both present or both absent — one
without the other is corrupt); build `HabitEvent` with
`HabitEventId::from(Uuid)` (the landed trust-boundary constructor,
whose doc already names the migration). Insert into `habits.events` —
keyed by id; a duplicate id in the table cannot exist (PRIMARY KEY).

**Order of load steps**: read event rows → read legacy rows (mirror) →
run the remap if the marker is absent (its synthesized events are
inserted into the same in-memory `habits.events` map) → build
`PersistedState`. The remap runs before `PersistedState` is assembled
so the first load after upgrade carries the synthesized events.

### D-FL — flush on the event table; the whole-second guards' fate

- The chunk-2 inert partition arm (`repo.rs:2323–2326`) becomes real:
  the collected `habit_event_rows` flush as

```rust
for event in habit_event_rows {
    sqlx::query(
        r"INSERT INTO habit_event (id, habit_id, kind, window, at, au_by,
                                     deleted_at, deleted_by)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(id) DO UPDATE SET
                   kind = excluded.kind, window = excluded.window,
                   at = excluded.at, au_by = excluded.au_by,
                   deleted_at = excluded.deleted_at,
                   deleted_by = excluded.deleted_by",
    )...
}
```

  i.e. `INSERT OR REPLACE`-by-uuid semantics spelled as an explicit
  `ON CONFLICT(id) DO UPDATE` (full-column replace; identical uuid =
  identical fact; the tombstone update is a plain column write —
  design §6.2). **No clock predicate anywhere**: the whole-second LWW
  guards (`repo.rs:2905–2921`, `:2941–2952`, #1095) are row-map LWW
  machinery for the *mirror* tables and stay until chunk 5 deletes the
  mirror writes with them; the event table never has guards — "the
  guards you retire" in the dispatch's words are the guards that would
  otherwise have been copied onto the new table. The remap's own
  inserts use the same statement (they cannot conflict in practice;
  the idempotence belt costs nothing).
- The `at`/`deleted_at` write form: `to_rfc3339_opts(SecondsFormat::
  Nanos, true)` — the same fixed grammar as the id key (D-ID), so the
  stored text round-trips byte-stably. No whole-second normalization
  on this table (events carry sub-second instants natively; no string
  comparison guards exist to break — the #1095 constraint is
  mirror-table-scoped).
- `tracing::trace!` per row, the lane's shape.

### D-RW — rewind

- `rewind.rs` needs **no logic change**: 0032's `CREATE TABLE`s derive
  `DROP TABLE IF EXISTS` strips automatically; the `CREATE INDEX`
  classifies `Unstrippable` (already a recognized shape). The canary
  `rewind_table_covers_the_ledger`'s pinned unstrippable-carrier list
  gains `(32, 1)` — the only rewind edit, a planned citation.
- Fixtures rewinding past 32 need **no `ExtraStrip`s**: the marker and
  event tables drop with the derived strips; the legacy tables keep
  their rows by design (pre-upgrade corpora) so the replayed 0032 +
  re-open re-runs the remap — which is the rewind-safety proof: the
  re-synthesized ids are identical (D-ID determinism) *and* identical
  to the pre-rewind bytes (the byte-identical test, §4).
- "Rewind is event-grammar from then on" (design §6.2): after the
  marker, nothing re-consults the legacy tables for events — pinned by
  the mid-upgrade test (§4).

### D-1549 — the #1549 migration-consequence documentation (option 3's delivery)

Written in four places, one text, three copies + a pointer:

1. **The migration file's comment** (D-32's header — verbatim block
   above).
2. **This plan** (§1 D-REM's derivation consequence — restated): a
   legacy window row whose audited count exceeds the habit's quota
   migrates as N events at one instant; `derive_window_counts`
   re-attributes them under the event grammar, the surplus routes into
   the unmet predecessor window, and the derived count of the audited
   window caps at the quota — the accepted #1549 limit now applies to
   migrated data. The legacy row on disk still shows the audited count
   (inert history); only the derived truth moved.
3. **The remap function's doc comment** in `repo.rs` (a condensed
   copy with the #1549 reference).
4. **The chunk-5 pointer**: the plan's §5 stage prompts tell stage 3 to
   comment the user-facing-note obligation on #1549 (beside #1550 per
   the ruling's text); the dispatcher closes #1549 only after chunk 5
   lands that note.

### D-UX — the `us4_s2` token's home

The UX plan's W-10 row names
`crates/taskboard-domain/src/appdata.rs::us4_s2_verbatim_never_folds`
(chunk 4's fold test); the dispatch plan's chunk-3 row assigns the
`us4_s2` token to this chunk's migration test. Both share the token
prefix; the gate matches exact `fn <name>` at the named path, so:
this chunk lands `us4_s2_migration_synthesizes_a1_events` in
`crates/taskboard-storage-sqlite/src/repo.rs`, and **W-10 stays MISSING
on the gate until chunk 4 lands its appdata.rs test** — reported
honestly (the chunk-2 plan's CLI-half precedent). Stage 2 runs
`scripts/ux-coverage.sh … --gate --root <worktree>` and reports the
steady state; the claimed-row evidence is the new test's FOUND status
in the nextest output, not the W-10 gate row.

### D-TS — the test-support seam (SonarCloud 3 % duplicate gate)

The fixture-story catalogue (§4) shares one scaffolding module —
`crates/taskboard-storage-sqlite/src/test_support.rs` (the crate's
existing test-support file; extend, don't spawn a parallel one): a
`legacy_habit_fixture(pool)` helper that writes `habit_log`/
`habit_day_log` rows from a plain struct list, and one
`event_table_dump(pool) -> Vec<u8>` byte-dump helper (the
determinism/rewind oracle). No copy-pasted INSERT blocks across tests.

---

## 2. Implementation order (TDD)

Each numbered step is at least one standalone commit
(`<type>(<module>): <desc>` + body; red/green within the commit).

1. **feat(storage): migration 0032 — the habit event and marker
   tables** — D-32; the migration file; the rewind canary's pinned
   list gains `(32, 1)`; `scripts/sqlx-prepare.sh` run and `.sqlx`
   committed with the migration (or verified a no-op — all new queries
   are unchecked, but the gate item runs regardless; §5); the uuid v5
   feature is NOT in this commit.
2. **feat(domain): the legacy habit event id recipe** — D-ID; the
   namespace const + `legacy_habit_event_id` in `habit_event.rs` with
   determinism/immortality unit tests (same inputs → same id; distinct
   ordinals → distinct ids; the mark leg distinct from the completion
   leg); the uuid `v5` feature lands here (workspace pin single-sourced,
   AGENTS §4; its own commit per the dispatch's Prompt 3.2 rule).
3. **feat(storage): load decodes the habit_event table** — D-LD;
   decode + `PersistedTable::HabitEvent`; round-trip test
   (`persist → load → identical event map`); corrupt-row tests
   (bad uuid, bad kind, half tombstone — typed rejections).
4. **feat(storage): flush persists UpsertHabitEvent rows** — D-FL;
   the inert partition arm becomes real; upsert-by-id test (insert,
   tombstone-ride replace, no clock predicate — a newer-lower-`at`
   row replaces, pinning the guards' absence).
5. **feat(storage): the load-then-remap pass** — D-REM + D-MK; the
   `us4_s2_migration_synthesizes_a1_events` main test; the marker
   discipline tests; the #1549 consequence pinned by a test (an
   over-quota legacy row migrates to N events whose derived count caps
   at quota — asserting the derived map, not text).
6. **test(storage): the fixture-story catalogue** — §4's remaining
   rows through the shared D-TS scaffolding.
7. **test(storage): rewind** — the rewind-to-31-and-back byte-identity
   test; the ledger canary edit rides commit 1, this commit proves the
   loop.

Then the fast gate (§5) and the ux-coverage gate.

---

## 3. The test catalogue (fixture-story)

All in `crates/taskboard-storage-sqlite` (`repo.rs` tests +
`test_support.rs`), fixtures built through `rewind_to`/direct DDL —
never the user's real data dir (the sandbox guard posture; scratch DBs
under `/var/tmp` via the existing fixture helpers). Tokens per D-UX.

| # | Test | Pins |
|---|---|---|
| 1 | `us4_s2_migration_synthesizes_a1_events` | legacy Week row (count 3, clock C, au_by A, skip 0) → exactly 3 live `Completion` events at C, `by` = A, ordinals 0–2, ids = the v5 recipe over the recorded facts; run the whole open-load twice → byte-identical table dumps |
| 2 | `remap_stamps_day_rows_at_the_twin_clock` | Day-geometry row + twin (`day == window_start`) → all events + the skip mark stamp at the twin's clock; the same row twin-less stamps at the row clock |
| 3 | `remap_synthesizes_skip_marks_from_the_skip_bit` | `skip = 1` row → N completions + exactly one `Skip { window }` mark (the mark leg id); `skip = 0` → no mark |
| 4 | `remap_sets_the_marker_even_without_legacy_rows` | empty legacy tables → zero events, marker row present; a second load never re-runs |
| 5 | `count_zero_legacy_row_migrates_to_zero_events` | `count = 0` row → no events, no mark (recorded inert outcome; the no-data-loss ledger records "count 0 ≡ zero events") |
| 6 | `marker_prevents_rerun_after_mid_upgrade_log` | remap; then insert an extra legacy row (simulating post-marker mirror writes); re-open → event table unchanged (the extra row never synthesizes); then a real `LogHabit`-shaped upsert adds its v7 event and survives a third open |
| 7 | `former_kind_row_migrates_as_data` | a row whose stored window date is not a canonical window of the habit's current kind (D-H12 luggage) → N events mint with the stored date in the id key; derivations re-attribute under the current config (assert via `derive_window_counts` on the loaded map) |
| 8 | `deleted_habit_rows_still_migrate` | legacy rows whose habit is absent/tombstoned → events synthesized (inert at derivation — the D-H6 posture) |
| 9 | `orphan_twin_and_mismatched_twin_contribute_nothing` | twin without a window row → nothing; twin with `day != window_start` → nothing; the twin `count` column read by nothing (the window row's count alone governs) |
| 10 | `over_quota_legacy_row_derives_capped_after_migration` | count 5 on a quota-3 Day habit → 5 events mint (no loss at the table) but `derive_window_counts` on the loaded state yields the quota-capped/surplus-spilled map — the #1549 consequence as a typed assertion (D-1549) |
| 11 | `event_round_trips_through_persist_and_load` | a state with synthesized + hand-built events (tombstones included) → `apply` → `load` → identical event map (the dispatch acceptance's round-trip) |
| 12 | `upsert_replaces_by_id_with_no_clock_guard` | same id upserted twice (second with a tombstone and an *earlier* `at`) → the row is replaced (pinning the whole-second guards' absence on this table) |
| 13 | `rewind_to_0031_and_back_reproduces_identical_events` | `rewind_to(pool, 31)` → re-open (0032 replays, remap re-runs) → the event-table dump equals the pre-rewind dump byte-for-byte |
| 14 | `corrupt_event_rows_are_typed_rejections` | bad uuid / unknown kind string / half tombstone → `RepositoryError` attached to `PersistedTable::HabitEvent` (fail-closed, no silent demotion) |
| 15 | determinism oracle | the shared `event_table_dump` byte comparison used by tests 1 and 13 (structural: ordered rows, fixed text grammar) |

Existing suites re-based, none weakened: the sqlite crate's load/flush
tests, the rewind fixtures (zero edits expected — D-RW), the
`taskboard-state` snapshots (no snapshot churn expected — this chunk
touches no domain state shape; if any snapshot changes it is a bug,
AGENTS §5).

---

## 4. Invariants

- **No data loss**: every legacy `habit_log` row of count N becomes
  exactly N live completion events (+ ≤ 1 skip mark), or lands in a
  recorded no-mint outcome (count-0 row; orphan/mismatched twin —
  instants without a count authority, never lost counts). Legacy rows
  are never mutated or deleted.
- **No wire work**: no `appdata.rs`/`merge.rs`/connector changes
  (chunk 4). The domain delta is the additive id recipe only — no
  domain command, message, journal, or state-shape change.
- **No UI/CLI changes** (chunk 5).
- **Shipped migrations immutable** (ADR 0005): 0032 is new; nothing
  edits 0001–0031; the remap is Rust at the repository boundary
  (design §6.2's reason: synthesis needs domain logic, not SQL).
- **Determinism**: two runs over the same database produce
  byte-identical `habit_event` content (D-ID; tests 1/13).
- **One-shot**: the marker makes the remap unreachable after its first
  successful run, on every future load, forever (D-MK).

## 5. Gates & verification (long-command discipline — binding for stages 2 and 3)

Identical to the chunk-2 plan §5 (verbatim origin: dispatch plan §1
shared execution model; `docs/agent_dispatch.org`), plus the chunk
specifics:

- **The sqlx prepare trap (explicit gate item)**: migration 0032 +
  any macro query touching the new tables invalidate the committed
  `.sqlx` offline cache; CI runs `cargo sqlx prepare --check` and a PR
  that forgot the refresh fails there. Stage 2 runs
  `scripts/memory-gate.sh scripts/sqlx-prepare.sh` after the migration
  lands and commits the `.sqlx` delta **with** the migration (never
  separately — the script's own law). This plan's queries are
  unchecked (`sqlx::query_as`/`query`, the 0030 precedent), so the
  delta may be empty — the script runs regardless and its output is
  reported.
- `scripts/run-quality-gates.sh cargo nextest run --workspace` (the
  fast gate; backgrounded with polls; commit before waits; flock
  deferral per the discipline).
- `scripts/memory-gate.sh` on every cargo invocation; the coverage
  trio never runs inside a stage.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md
  --gate --root <worktree>` — ALWAYS `--root` (the #1531 law).
- Focused pre-gate loop: `scripts/memory-gate.sh cargo nextest run -p
  taskboard-storage-sqlite -p taskboard-domain`.
- Never commit with `--no-verify` or any hook bypass; never touch
  `~/.local/share/taskboard` or `~/.config/taskboard` — fixtures live
  under the test helpers' isolated dirs (/var/tmp for large scratch).

## 6. Risks

- **R1 — the sqlx prepare trap**: the only CI-failing forget-mode;
  gated as an explicit §5 item and restated in both stage prompts.
- **R2 — the cross-device id question (explicit)**: identical legacy
  facts mint identical v5 ids (convergence — intended); differing
  legacy clocks across devices mint distinct ids for "the same"
  completions, and the chunk-4 union inflates the derived count until
  the ≤ 180 d retention ages the legacy rows out (Ruling D1's accepted
  transition). The recipe's contract text names it; sqlite itself
  never sees the phenomenon (device-local). Chunk 4's plan owns the
  wire-side test.
- **R3 — the twin-geometry refinement** (D-REM step 4) deviates from
  design §6.2's "the habit's current target kind is Day" letter to
  preserve the pure row fold (no `habits` join). The behavior is
  identical for every shape that can occur; the week-boundary corner
  is analyzed in the decision and accepted (the twin clock is recorded
  reality there too). Stage 3 re-derives this analysis (focus (b)).
- **R4 — the remap runs inside the load transaction**: a large legacy
  corpus lengthens one load; kiosk-scale corpora (hundreds of rows)
  are trivial; no chunking designed (the pass runs once, ever).
- **R5 — `PersistedTable::HabitEvent` is a new enum variant**: verify
  serde/decode stability of `PersistedTable` (if persisted anywhere,
  additive law); stage 2 verifies, same posture as the chunk-2 R4
  archaeology.
- **R6 — RQ-1's dormant semantics is the plan's one recorded deviation
  from design §6/Ruling F's "load stops reading" sentence**, with the
  restart-keystone reason (§0 RQ-1). Operator-overridable; overruling
  moves chunk-5 work forward.

## 7. Estimated diff size

- `0032_habit_events.sql`: ~40 lines. `repo.rs`: ~180 (remap + decode +
  flush loop) + ~420 test lines. `habit_event.rs`: ~50 (recipe) +
  ~60 tests. `rewind.rs`: ~1 (canary list). `Cargo.toml`: 1 line.
  `.sqlx`: 0–small. **Total ~750–850 lines**, one crate pair
  (`taskboard-storage-sqlite` bulk, `taskboard-domain` recipe),
  no UI/app/state churn expected.

---

## 8. Stage-2 (IMPLEMENTATION) prompt — verbatim, paste-ready

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the habit event-log dispatch ("sqlite migration"). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-storage-habit-event-migration (branch feat/storage-habit-event-migration); every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard (it resolves to /home/viktoria/nextcloud/viktoria/programming/taskboard); never touch another taskboard-WT-* directory.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (3) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (4) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings A, F, I)/§6, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§4, ADR 0005 (docs/adr/0005 — shipped migrations immutable), the chunk-2 plan .artifacts/plans/2026-11-27-habit-event-commands-plan.md (D-C/D-P/D-R), and the stage-1 plan .artifacts/plans/2026-11-28-habit-event-migration-plan.md (all in the worktree) — then implement exactly per the stage-1 plan's §1 decisions (D-32, D-ID, D-REM, D-MK, D-LD, D-FL, D-RW, D-1549, D-UX, D-TS) and §2 order. If the worktree is missing, recreate it: git -C /home/viktoria/src/taskboard worktree add -b feat/storage-habit-event-migration ../taskboard-WT-feat-storage-habit-event-migration origin/main; if that branch's PR already landed, use feat/storage-habit-event-migration-v2 per AGENTS §8 (never `git worktree add <path> main`, never a slash-suffixed worktree path).
2. TDD, every commit standalone (rebase-merge hygiene). Hard rules: NEVER edit a shipped migration (ADR 0005 — 0032 is new; 0001–0031 untouched); the uuid v5 feature lands as its own commit with the workspace pin single-sourced (root Cargo.toml [workspace.dependencies], AGENTS §4); SPDX headers on every new .rs file; #![forbid(unsafe_code)] (crate-wide already); no tokio::time::sleep in tests; no text assertions (typed matches!/structural facts only); no weakening a correct test (AGENTS §5: a failing new test may be a production bug — evaluate both, leave it failing and report if the fix is out of scope); scratch fixtures never land; shared test scaffolding lives in the existing test-support module, not copy-pasted (the required SonarCloud gate fails a PR whose NEW code exceeds 3% duplicated lines); run scripts/sqlx-prepare.sh through scripts/memory-gate.sh after the migration lands and commit the .sqlx refresh WITH the migration (never separately) — this is a named gate item, report its output; the recorded deviations (D-REM's twin-geometry refinement; RQ-1's dormant = still-written mirror) and the six RQ defaults stand as the plan records them — do not re-litigate them, implement them.
3. Scope fences (dispatch §4 non-goals + the plan): no wire work (appdata.rs/merge.rs/connectors untouched); no UI/CLI changes; no domain command/message/journal changes (the additive habit_event.rs id recipe is the only domain delta); the legacy habit_log/habit_day_log tables stay on disk AND keep their mirror writes and load reads (RQ-1); the whole-second LWW guards stay on the mirror tables until chunk 5; the event table gets NO guards (D-FL).
4. Run the fast gate per the long-command discipline above: pre-commit hooks automatic; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls, useful work between polls, all work committed BEFORE any wait; every cargo invocation through scripts/memory-gate.sh; the coverage trio never runs in this session; flock deferral = commit + "gate deferred: lock held by <holder>" + end.
5. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate --root /home/viktoria/src/taskboard-WT-feat-storage-habit-event-migration (ALWAYS --root: invoked from a worktree without it the script silently scans the main checkout — issue #1531). The chunk's claimed evidence is us4_s2_migration_synthesizes_a1_events in crates/taskboard-storage-sqlite/src/repo.rs; the W-10 gate row (appdata.rs::us4_s2_verbatim_never_folds) stays MISSING until chunk 4 — report the full honest steady state, never paper over a MISSING.
6. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
7. Backlog issues per the discipline above; list the issue numbers in your report.

Report: per-gate results, the sqlx-prepare output, remap idempotence evidence (the byte-identical double-run test), any deviation from the plan and why.
```

## 9. Stage-3 (REVIEW) prompt — verbatim, paste-ready

```text
You are the stage-3 REVIEW agent for chunk 3 of the habit event-log dispatch (branch feat/storage-habit-event-migration). Work ONLY in the worktree /home/viktoria/src/taskboard-WT-feat-storage-habit-event-migration; every command by absolute path; never modify the main checkout /home/viktoria/src/taskboard; never touch another taskboard-WT-* directory.

**Long-command discipline (binding — docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, the `mergeStateStatus CLEAN` verification, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh (the 2026-11-23 systemd-oomd law) — nothing bare. (5) Out-of-scope findings become `backlog:` GitHub issues (or comments on existing ones) the moment they are discovered, without asking (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.**

1. **Begin by rebasing onto main**: git fetch origin --prune, then rebase the branch onto origin/main (AGENTS §8's rebase dance: drop already-merged duplicate commits; resolve only real conflicts), push --force-with-lease if the rebase rewrote history. Then read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings A, F, I)/§6, the dispatch plan §1/§4 (the code-quality mandate below applies to you), ADR 0005, the chunk-2 plan (D-C/D-P/D-R), and the stage-1 plan .artifacts/plans/2026-11-28-habit-event-migration-plan.md; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD) in the worktree.
2. Functional focus, named from the origin: (a) the remap is deterministic and idempotent — two runs byte-identical, the marker forbids re-runs, and a user logging between upgrade passes must never duplicate events (verify test #6 really inserts a post-marker legacy row and asserts the event table unchanged); (b) Ruling A1 attribution exactly — row-clock stamps, the twin-clock refinement ONLY in the day-geometry shape the stage-1 plan D-REM step 4 records (re-derive its week-boundary corner analysis yourself; if it breaks, that is a finding); the window row is the sole count authority (the twin `count` column read by nothing — try to construct a fixture where reading it would double-count and confirm the code cannot); (c) the v5 recipe is single-sourced in taskboard-domain (chunk 4 will import it — no recipe text duplicated in repo.rs), the namespace is a fixed documented constant, and the id key's clock grammar is the fixed nanos-UTC form; (d) no shipped migration edited (git diff the migrations/ dir: only 0032 added); the sqlx cache is fresh — run scripts/memory-gate.sh scripts/sqlx-prepare.sh --check yourself; (e) the retired whole-second guards leave NO stale callers ON THE EVENT TABLE — and the mirror-table guards survive untouched until chunk 5 (a guard deleted from habit_log/habit_day_log is a scope violation); (f) rewind round-trips — the canary's pinned list edit is present and the rewind-to-31-and-back byte-identity test passes; (g) network IO still absent from this crate (AGENTS §2); no RwLock/Mutex shortcuts (AGENTS §3); (h) the D-1549 documentation exists in all three places (migration comment, remap doc comment, stage-1 plan) and the chunk-5 pointer is carried; (i) no weakened tests; the #1549 consequence is pinned by a typed derived-map assertion, not text.
3. **Data-safety review duty (this is a persisted-data migration — try to break it)**: actively construct (or verify a catalogue test constructs) a legacy database for each of: empty legacy tables; count-0 rows; skip-marked rows; former-kind rows (the D-H12 luggage, stored window not a canonical current-kind window); rows for a deleted or tombstoned habit; twin rows without a window row; window rows without a twin; two devices remapping the same legacy rows (same facts → same ids — and the differing-clock inflation consequence named in the plan's R2); a marker present with a non-empty event table (the mid-upgrade shape); a crash between the event inserts and the marker insert (transaction atomicity — events-without-marker must be an unreachable persisted state). Every such case must have a test with a recorded outcome — a case you can construct that the remap loses, double-counts, or non-deterministically mints is a BLOCKER fixed on the branch (a new test + the fix), never a report note.
4. **Code-quality mandate**: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crate; the derivation fold never leaves the domain crate; the UI never computes habit semantics; the remap stays in the storage crate with only the pure id recipe imported from domain); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Also verify the shared test scaffolding kept the new-code duplication within the 3% SonarCloud gate (no copy-pasted fixture INSERT blocks). Findings are fixed on the branch like any other finding.
5. Re-run the fast gate per the long-command discipline (pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace, backgrounded with polls, work committed before any wait; flock deferral rule applies; every cargo invocation through scripts/memory-gate.sh). The coverage trio never runs in this session.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship — **your scope ends at PR-open; do not remove the worktree**: push the branch (--force-with-lease only, never plain --force), open the PR to main with gh, then report the URL — do NOT run gh pr checks --watch; the dispatcher owns CI watch, merge-state verification, teardown of this worktree, and at-merge issue closures. This is a persisted-data migration: in the PR body, state plainly that the operator has asked to approve this merge personally, and link the stage-1 plan's §0 Rulings-requested section. Never merge; never touch main.
8. Backlog issues per the discipline above; list the issue numbers in your report.

Report: verdict, findings (functional + data-safety + mandate), what you fixed, PR URL. Never merge; never touch main; do not remove the worktree.
```

---

## 10. Stage-2 landing note (2026-11-28)

Implemented on `feat/storage-habit-event-migration`; the six RQ
defaults (§0) were implemented as recommended, each cited at its
code site:

- **RQ-1** (dormant = mirror still written and read): the untouched
  mirror flush arms and load decode in
  `crates/taskboard-storage-sqlite/src/repo.rs`; no RQ-1 comment was
  added there because the behavior is the pre-existing code (the
  marker/remap is the only delta) — the ruling lives here and in the
  remap doc comment.
- **RQ-2** (marker set on every 0032-aware first load, legacy rows
  or not): the gate in `remap_habit_events_if_needed` (`repo.rs`,
  "RQ-2" comment); test `remap_sets_the_marker_even_without_legacy_rows`.
- **RQ-3** (skip bit → one `Skip` mark, mark leg id, no `Unskip`):
  the `skip` branch in `remap_habit_events_if_needed` ("RQ-3"
  comment); test `remap_synthesizes_skip_marks_from_the_skip_bit`.
- **RQ-4** (twin rows contribute instants, never counts): the
  `twin_clocks` map in `remap_habit_events_if_needed` ("RQ-4"
  comments); tests `remap_stamps_day_rows_at_the_twin_clock`,
  `orphan_twin_and_mismatched_twin_contribute_nothing`.
- **RQ-5** (no `habits` join): the pure row fold over `habit_log`
  rows only ("RQ-5" comment in the remap doc);
  test `deleted_habit_rows_still_migrate`.
- **RQ-6** (#1549 consequence documented, not re-opened): migration
  header, remap doc comment, this plan §1 D-1549; test
  `over_quota_legacy_row_derives_capped_after_migration`.

Recorded deviations from the plan's letter (behavior-preserving or
forced by the domain types):

1. **D-LD's "both tombstone legs present or both absent"** became:
   the tombstone exists iff `deleted_at` is set; `deleted_by` is the
   tombstone's optional author (the domain's `HabitEventTombstone.by`
   is `Option<AuthorId>` — chunk 2's type law; the strict both-legs
   rule would make every authorless tombstone undecodable). The
   corrupt direction tested is `deleted_by` present with `deleted_at`
   absent (`corrupt_event_rows_are_typed_rejections`).
2. **§2 steps 3–7 share one repo.rs commit** (the decode, flush,
   remap, and catalogue edits are interleaved in one file and the
   pre-commit hook compiles the whole worktree, so partial-file
   staging could not prove each intermediate state); steps 1, 2 (the
   uuid-pin rule) and the rewind proof remained separate commits.
3. The `connect.rs::reopening_applies_no_new_migrations` pinned
   shipped-migration literal moved 31 → 32 (a planned consequence of
   shipping 0032; the assertion's substance — reopen idempotence —
   unchanged).

D-UX steady state: `us4_s2_migration_synthesizes_a1_events` landed in
`crates/taskboard-storage-sqlite/src/repo.rs`; W-10
(`us4_s2_verbatim_never_folds` in `appdata.rs`) stays MISSING until
chunk 4, as §1 D-UX records.
