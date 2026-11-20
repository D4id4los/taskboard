# Journal Decode Robustness — Chunk B2 Plan (#1039 + #1038)

- **Date**: 2026-11-20
- **Kind**: stage-1 chunk plan (dispatch plan
  `.artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md` §7)
- **Branch**: `fix/journal-decode-robustness`
- **Status**: **GATE-PAUSED — the boot policy below is data-loss-adjacent
  and awaits operator confirmation before stage 2** (dispatch plan §7).
  Stage 2 implements exactly the recorded recommendation once the gate
  clears.
- **Scope**: `RepositoryError::Corrupted` gains typed decode context
  (#1038); journal-row quarantine with a typed degraded-boot signal;
  entity-table decode stays abort (with the new context). The healthy
  boot path is untouched (round law c).

## 1. Seams read (stage-1 evidence)

- **Domain port**: `crates/taskboard-domain/src/persistence.rs` —
  `RepositoryError` is a two-variant, `Copy` unit enum (`Unavailable`,
  `Corrupted`); `TaskRepository::load` → `PersistedState`.
- **Storage decode**: `crates/taskboard-storage-sqlite/src/repo.rs`
  `load_inner` (~line 971) hydrates entity tables, then habits, then the
  command journal (~1365–1395) in rowid order. ~45 `Corrupted`
  construction sites: the `decode::{uuid, clock, author}` helpers
  (~284–320), the enum-from-text helpers (~440–485), per-row struct
  decoders (reminders ~690–708, habits ~750–855, validators ~1288),
  and the journal loop (`journal_payload_from_json(...).ok_or(Corrupted)`
  at ~1384 — the site that discarded serde's field-name error, the
  2026-11-19 boot failure).
- **Error mapping**: `crates/taskboard-storage-sqlite/src/error.rs`
  `repo_error` maps sqlx shape errors onto `Corrupted` (constraint /
  column-decode) vs `Unavailable` — no row context exists there either.
- **Boot surfacing**: `EngineStartupError::Load(#[from] RepositoryError)`
  (`crates/taskboard-state/src/error.rs:33`) →
  `BootstrapError::Engine` (`crates/taskboard-app/src/bootstrap.rs:301`)
  — the app refuses to start; the user sees only the string
  "repository data corrupted".
- **Journal role**: `crates/taskboard-domain/src/journal.rs` — undo
  metadata (ADR 0019): entries form a parent-chained line, head pinned in
  `command_journal_head`; the authoritative task state lives in the
  entity tables. Retention cap 1000 (R-U11). The specific H1 serde
  incompatibility from 2026-11-19 is already fixed upstream by the
  `HabitClocks` `#[serde(default)]` leniency (habit.rs ~237); this chunk
  is the general-robustness remainder, not that regression.
- **Signal channel**: the engine's `broadcast::Sender<SystemEvent>`
  (`bootstrap.rs` system feed) exists and is the house primitive for
  global status notifications (AGENTS §3).

## 2. Recorded decision — boot policy (GATE-PAUSED)

**Recommendation: hybrid, per the dispatch frame — abort-with-typed-context
for entity tables; quarantine-with-signal for journal rows only.**

The frame's safe default (abort) holds for everything authoritative;
the frame's carve-out (quarantine with an explicit surfaced typed
signal, never silent) is exercised exactly once, for the one table that
is metadata:

| Data | Failure outcome | Why |
|---|---|---|
| Entity tables (boards, stacks, tasks, labels, outbox, validators, reminders, tracking, habits, habit_log) | **Abort** with `Corrupted(CorruptionDetail)` | Authoritative state. A skipped row is silent user-data loss; abort loses nothing the disk doesn't already hold. |
| `command_journal` rows | **Quarantine**: skip the bad row and every descendant (children whose `parent` chain crosses the gap), re-pin `command_journal_head` to the last good entry, `tracing::warn!` with the typed detail, and emit a typed degraded-boot signal | Undo metadata only — the tasks remain reachable and correct; the undo line degrades instead of the whole app (#1039's exact proposal). |

### 2.1 Data-loss analysis (what a skip loses, what a user sees)

- **What a quarantined journal row loses**: the user's ability to undo
  (and redo past) that one command — and, by tail truncation, everything
  after it on the undo line. Nothing in the task/board/habit state is
  lost: undo only *applies* pre-images when invoked; a dropped entry
  cannot apply anything. Cap-bounded blast radius: the journal holds at
  most 1000 entries / 90 days (R-U11), so worst-case loss is "recent
  history" — bounded, non-authoritative, and strictly smaller than the
  status quo ("the app never starts").
- **What the user sees**: boot proceeds; the app is usable. The
  degraded-boot signal (§4.3) is typed and user-facing at the app layer
  (log + broadcast; a UI badge is a separate user-visible capability and
  out of scope here — the JSON-serializable typed event makes it
  addable later without another decode-path change).
- **What abort-with-context changes for entity tables**: nothing about
  the outcome (still refuses to start — correct for authoritative data)
  but the operator no longer bisects rows: the detail names table, row
  key, decode stage, and reason kind (#1038's acceptance).
- **Asymmetry check**: quarantine can only be entered when the failed
  row is provably non-authoritative. Any journal decode failure that
  suggests *entity* corruption (e.g. a payload naming a task that no
  longer decodes elsewhere) is not reachable here — journal decode
  failure is self-contained payload decoding; entity decode failures
  abort independently. No false-quarantine path exists.

### 2.2 Alternatives rejected

- **Abort everywhere with context** (frame's pure default): maximally
  conservative, but a single stale undo row again bricks the boot — the
  exact 2026-11-19 outage — and the journal being metadata makes the
  loss asymmetric: it trades a bounded undo tail for total unavailability.
- **Quarantine everywhere**: silent user-data loss on authoritative
  tables; violates the frame outright.
- **Truncate-from-first-bad-row only (no per-row skip)**: simpler, but
  discards still-decodable later entries; per-row quarantine preserves
  the maximal good prefix-and-suffix? — no: the line is parent-chained,
  so a skipped middle row orphans its descendants anyway; the
  descendant drop is *forced* by the chain invariant, not an extra
  policy. Quarantine = drop the minimal violating sub-forest.

## 3. Typed context on `Corrupted` (#1038)

### 3.1 The detail type (domain crate, hand-rolled per house A1)

```rust
/// Which persisted table failed. Closed enum — every hydratable table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersistedTable {
    Boards, Stacks, Tasks, Labels, Outbox, Validators,
    Reminders, Tracking, Habits, HabitLog, CommandJournal, CommandJournalHead,
}

/// Where in the decode the failure fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeStage { Column, RowBuild, Payload, Invariant }

/// Why the value did not decode. Closed enum — no string payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeReason {
    MalformedUuid, MalformedClock, MalformedDate,
    UnknownEnumVariant, SerdePayload, OutOfRange,
    ConstraintViolation, MissingColumn,
}

/// Typed decode context carried by `RepositoryError::Corrupted`.
/// `row` is the row's key (UUID text where the schema has one) — the
/// bisect-killer; `Option` because singleton rows have none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CorruptionDetail {
    pub table: Option<PersistedTable>,
    pub row: Option<uuid::Uuid>,
    pub stage: Option<DecodeStage>,
    pub reason: Option<DecodeReason>,
}
```
(Serde derives so `SystemEvent::JournalDegraded` stays
round-trip-serializable like the rest of the system feed; `Default` is
the all-`None` `UNKNOWN` of D-B2-4. The enums derive Serde likewise.)

**Recorded decisions on the shape:**

- **D-B2-1 (row identity)**: `row: Option<Uuid>` — raw `uuid::Uuid`,
  not a `String` dump. All decoded rows key on UUID text; the singleton
  head row is `None`. If a UUID itself fails to parse, the *raw text* is
  not carried (it is untrusted garbage; the warn log may print it,
  the typed context stays a typed `None` row with the uuid-stage reason).
- **D-B2-2 (no strings)**: no field is a formatted string (round law a).
  The `#[error]` Display of `Corrupted` may render the detail for humans;
  the facts themselves are typed.
- **D-B2-3 (port stability)**: `RepositoryError::Corrupted` becomes
  `Corrupted(CorruptionDetail)`; `Copy` is dropped from
  `RepositoryError` (`Clone`/`PartialEq` retained — the ~232 usage sites
  are construction and matching, none copy-dependent; the error.rs tests'
  `assert_eq!` still compile). The enum's variant set is unchanged —
  external port shape stable, payload added.
- **D-B2-4 (context-free construction)**: a
  `RepositoryError::corrupted(detail)` constructor plus
  `CorruptionDetail::UNKNOWN` (all-`None`) for the `repo_error` sqlx
  shape mapping (no row in hand there). Mechanical rewrite of the ~45
  `.ok_or(...)`/`map_err(|_| ...)` sites.

### 3.2 Context capture per construction site

| Site | table | row | stage | reason |
|---|---|---|---|---|
| `decode::uuid` (repo.rs ~306) | caller-supplied | caller-supplied (where the caller has one) | `Column` | `MalformedUuid` |
| `decode::clock` (~317) | caller-supplied | caller-supplied | `Column` | `MalformedClock` |
| `decode::author` | caller-supplied | caller-supplied | `Column` | `UnknownEnumVariant` |
| enum-from-text helpers (~440–485) | caller-supplied | caller-supplied | `RowBuild` | `UnknownEnumVariant` |
| remote-ref decoders (~380–440) | caller-supplied | caller-supplied | `RowBuild` | `OutOfRange` / `MalformedUuid` / `UnknownEnumVariant` |
| reminders per-row (~690–708) | `Reminders` | reminder rule id | `RowBuild` | per-arm |
| habit-log loop (~1419–1437) | `HabitLog` | habit id | `RowBuild` | `MalformedUuid`/`MalformedDate`/`OutOfRange` |
| validator key/value (~1288–1304) | `Validators` | validator key | `RowBuild`/`Payload` | per-arm |
| `repo_error` (error.rs) | `None` | `None` | `Column` or `Invariant` | `ConstraintViolation`/`MissingColumn` by arm |
| journal loop (~1370–1395) | `CommandJournal` | the row's journal id text parsed to `Uuid` (id-column parse failures carry `None` row + `MalformedUuid`) | `Column` (id/parent/board/created_at) / `Payload` (serde) / `Invariant` (chain break during quarantine) | per-arm; the payload serde arm maps serde's error into `DecodeReason::SerdePayload` **and the warn log prints serde's field-level message** (triage text in logs, typed fact in the error) |

To avoid ~45 hand-threaded tuples, the `decode` helpers gain sibling
signatures taking `(PersistedTable, row: Option<Uuid>)` and returning
`Result<T, RepositoryError>` with the detail assembled internally;
row-build sites call a `fail(table, row, stage, reason)` closure-style
helper. No helper signature churn beyond the storage crate.

## 4. Journal quarantine mechanics (#1039)

- **D-B2-5 (quarantine algorithm)**: hydrate journal rows in rowid order
  into `Vec<JournalEntry>` as today; on any row decode failure:
  1. Record the typed `CorruptionDetail` + `warn!` (row id, table,
     stage, reason, serde message at the payload arm).
  2. Continue decoding subsequent rows (a *later* row independent of the
     gap — `parent: None` lineage or a chain rooted before the gap —
     still hydrates; parent-chained descendants of the bad row are
     dropped as orphaned, D-B2-2 alternative's minimal sub-forest).
  3. Collect all quarantine details; re-pin the head to the highest
     rowid of a surviving entry (matching the existing
     `command_journal_head` write shape).
  4. Emit one degraded-boot signal per load (not per row).
- **D-B2-6 (signal)**: new domain variant
  `SystemEvent::JournalDegraded { details: Vec<CorruptionDetail>,
  dropped: u32, head: Option<JournalId> }` on the existing system
  broadcast. The storage actor cannot broadcast directly (it owns no
  sender); the details ride `PersistedState`? — **No: recorded decision
  is a new field-free channel — `load()`'s signature stays stable;
  instead the storage actor replies with the details appended to a new
  `StorageCommand`-layer load result used only by the engine's boot
  hydration call site, which then broadcasts.** Concretely:
  `TaskRepository::load` keeps `Result<PersistedState, RepositoryError>`;
  the sqlite actor emits the details through a crate-internal
  `watch::Sender<LoadDiagnostics>` that the engine reads right after a
  successful boot load and re-broadcasts as `SystemEvent::JournalDegraded`.
  (Alternative considered — a `PersistedState` field — rejected: it
  perturbs the snapshot corpus and every fake; healthy-path unchanged,
  round law c.)
- **D-B2-7 (head re-pin)**: the re-pin is a best-effort `UPDATE` after
  the successful load transaction boundary; a failed re-pin leaves the
  stale head — the next load's quarantine pass re-pins (idempotent), and
  the invariant `head ∈ surviving entries ∪ {None}` is what tests pin.
- **D-B2-8 (undo/redo line invariants)**: after quarantine, the undo
  planner sees a chain with a gap-free surviving prefix per lineage;
  redo of a pre-gap entry behaves as today (re-plan from head). The
  undo line is degraded, never panicked-on. Pinned by an engine-level
  test that undoes past a quarantined gap.
- **Healthy-data path untouched**: no corruption ⇒ zero behavioral delta
  (same rows, same head, no event, no warn).

## 5. Test matrix (deterministic; typed outcomes; no text assertions)

Storage-crate unit/integration tests (fixtures hand-built SQLite with
one injected bad value; assertions on `matches!(err,
RepositoryError::Corrupted(d))` detail fields, or on the surviving row
set — never on message text):

1. **Entity abort, head**: bad `boards.title` enum text on the first
   board row → `Err(Corrupted(d))` with `table = Boards`, `row = Some`,
   `reason = UnknownEnumVariant`; state not returned.
2. **Entity abort, middle**: bad habit-log `window_start` date mid-table
   → `table = HabitLog`, `reason = MalformedDate`.
3. **Entity abort, tail**: malformed UUID on the last validator row →
   `table = Validators`, correct row `None`-if-unparseable.
4. **Journal payload, head row**: first journal row's payload not JSON →
   load succeeds; surviving set = later entries; head re-pinned;
   `JournalDegraded` details carry `table = CommandJournal`,
   `stage = Payload`, `dropped ≥ 1` (floor, not ceiling — no quality
   ceiling law).
5. **Journal, middle**: bad row mid-line with two descendants → the two
   descendants are dropped (orphaned), pre-gap entries survive.
6. **Journal, tail**: last row undecodable → only that row dropped,
   head re-pinned to its parent.
7. **Journal, multi-row multi-arm**: one bad id-column row + one bad
   payload row in the same load → both quarantined, both detailed.
8. **Chain-root survival**: a later entry whose `parent` chain roots
   before the gap survives (D-B2-5 step 2).
9. **Head singleton**: `command_journal_head` row missing entirely →
   abort with `table = CommandJournalHead`, `row = None` (the
   fail-closed ~1176 path).
10. **Engine routing**: fake-repo boot with a quarantined load (storage
    seam faked at the diagnostics watch) → engine boots, emits
    `SystemEvent::JournalDegraded` once, state healthy.
11. **Healthy delta pin**: existing boot corpus tests unchanged and
    green (no event, no warn) — the healthy-path law.
12. **serde field triage**: a payload missing a struct field → warn
    recorded (structured, via `tracing_test`-style capture if present in
    tree, else the typed detail only) naming the field; assertion on the
    typed `DecodeReason::SerdePayload`, not the text.

Property-test (proptest, domain or storage): a journal with one
randomly injected undecodable row among N valid chained rows ⇒
surviving entries are exactly the valid rows whose parent chains avoid
the bad row; head ∈ survivors; load returns `Ok`. Pure and
deterministic (fixed seed corpus construction, no clock).

## 6. Open questions — all resolved

| Question | Decision |
|---|---|
| Abort vs quarantine | Hybrid: abort entity tables, quarantine journal (§2) — GATE-PAUSED |
| Where does the context live | Payload on `Corrupted` (§3) |
| Strings in context | None (D-B2-2); serde detail text → warn logs only |
| Row identity shape | `Option<Uuid>` (D-B2-1) |
| Signal transport | storage-internal watch → engine broadcast `SystemEvent::JournalDegraded` (D-B2-6) |
| `PersistedState` changes | None |
| `TaskRepository` port signature | Unchanged |
| UI badge for degraded boot | Out of scope → backlog issue (the typed event makes it addable) |
| Undo behavior across a gap | Degraded line, planner tolerant, pinned by test 8/D-B2-8 |
| Head re-pin failure | Idempotent retry next load (D-B2-7) |

## 7. Risks

- **`Copy` removal ripple**: ~232 `RepositoryError` mentions; mechanical
  but wide. Mitigation: the `corrupted()` constructor makes each site a
  one-line change; clippy `-D warnings` catches stragglers.
- **Quarantine correctness under chains**: the descendant-drop rule must
  match the undo planner's assumptions; tests 5/8 + D-B2-8 pin both
  sides.
- **Hidden journal consumers**: `load_inner` is the only journal reader
  at boot; rewind/test-support paths construct journals synthetically
  and are unaffected, but the in-memory fake must stay signature-stable.
- **Snapshot corpus**: untouched by design (no `PersistedState` field);
  verify corpus stays byte-stable in the gate run.

## 8. Estimated diff

Medium (dispatch-plan sizing M): ~350–500 lines — domain detail/enums +
system event (~80), storage decode helper signatures + ~45 site rewrites
(~180), quarantine + head re-pin + diagnostics watch (~80), engine
broadcast hop (~20), tests ~120–150. Confined to the storage/replay seam
+ the error type (dispatch acceptance); no UI, no sync.

## 9. Stage-2 handoff

Implement exactly §2–§5 once the operator clears the gate. Round laws
apply: typed outcomes everywhere, healthy path untouched, any
out-of-scope discovery → immediate `backlog:` issue reported in the
stage report. Gate: pre-commit hooks + `scripts/run-quality-gates.sh
cargo nextest run --workspace`; flock-deferral rule applies verbatim.
