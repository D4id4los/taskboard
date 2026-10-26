# [Plan] Time tracking — domain & engine (DR-5/1, #486) — stage-1 TDD implementation plan

- **Date**: 2026-10-26
- **Kind**: stage-1 TDD implementation plan for chunk **DR-5/1** of the
  six-rounds dispatch (wave 1). Zero production code in this commit;
  this file is the chunk's entire planning artefact. Implementation is
  stage 2's (dispatch prompt 1.2), review + shipping stage 3's
  (prompt 1.3).
- **Origin**: the accepted design round
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  (§3 Parts A+B and D-T18 are normative decision by decision; §1's
  anchors; §5's test hooks; §7's chunk-1 row), dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§3.4/§4.1 (the Covers/Acceptance list and the code-quality
  mandate are binding). Issue: **#486**. The follow-ons #487 (storage
  & roaming), #488 (CLI & export), #489 (UI) are *other* chunks — not
  a line of them lands here.
- **Normative base**: `origin/main` = `27343ad` (the worktree's branch
  point; verified 2026-10-26 via `git fetch origin --prune`). Wave 1
  has no upstream gates (DR-5/1 is the first chunk of the
  command-surface lane; the migration/ADR ladder assigns ADR 0017 to
  this chunk and no migration to it).
- **Branch / worktree**: `feat/tracking-domain-engine` in
  `../taskboard-WT-feat-tracking-domain-engine` (created 2026-10-26
  after `scripts/worktree-doctor.sh` passed: 87 G free, floor 25 G).
- **Milestones** (plan structure, not commit boundaries — stage 2
  lands TDD pairs as standalone green commits): M1 the entity, its
  state home, and the id (shape-widener core); M2 the five verbs
  (planner arms + receipts/errors); M3 the `SetTaskDone`/`DeleteTask`
  side-effects; M4 the pure projections + properties; M5 the engine
  tier (crash-resume) + snapshot regen; M6 ADR 0017 + the full gate
  suite. Every milestone is plan structure; commits follow the TDD
  pairs of §7.

---

## 1. Mission & binding constraints

The tracking core: a `TimeEntry` entity whose **open entry (`end:
null`) is the running timer** (D-T1), housed in `TrackingState` nested
like `ReminderState` (D-T3); five `StateCommand` verbs with typed
receipts and errors (D-T4); planner semantics — `StartTracking`
closes-all-then-opens and settles the task's notifications via the
exact `SetTaskDone` dismissal machinery (D-T5), `StopTracking` closes
all, `SetTaskDone`/`DeleteTask` close the task's open entries at their
stamps (D-T6), `LogTime`/`AdjustTimeEntry`/`DeleteTimeEntry` give v1
manual history editing with adjust able to close but structurally
unable to reopen (D-T7); the single-global soft invariant pinned by
properties (D-T8); the `tracked_in_window` day-window projection; and
ADR 0017 (D-T18).

Binding, from the dispatch plan §3.4 + §4.1 and the origin plan
(verbatim force):

1. **Nothing ticks.** No clock-driven task, no cached elapsed, no
   `running` field anywhere: the open set is *derived*
   (`entries.values().filter(|e| e.end.is_none())`), elapsed is always
   `now − start` at read, and an end is stamped only by a command at
   planner `now`. A `running` field or a ticking anything is the
   design's cardinal violation (stage-3 grep gate).
2. **`now` is data.** Domain purity holds: every planner entry point
   takes `now: DateTime<Utc>`; nothing in `taskboard-domain` reads a
   clock (the `clock.rs` rule).
3. **The storage crate carries no schema, migration, codec, or decode
   work in this chunk.** Migration 0013 is chunk DR-5/2's (wave 7,
   dispatch §2 ladder). The one forced exception — the storage repo's
   no-wildcard match must grow the two new action arms or the workspace
   does not compile — is resolved by decision **R-11** below and is
   the chunk's single recorded deviation from dispatch prompt 1.2's
   letter.
4. **Engine tests run on the existing repo tiers** (`InMemoryRepository`
   + the engine harness seams), injected clocks, oneshot replies /
   poll-with-deadline waits only, no sleeps anywhere.
5. **Typed outcomes only** — receipts, `CommandError` variants,
   structural facts; no text assertions on any path (AGENTS §5); no
   quality ceilings; one behavior, one test, lowest layer.
6. **A1 discipline** (AGENTS §2): the span predicate is enforced at
   the planner trust boundary through a hand-rolled private-field
   newtype with a fallible mint (`ClosedSpan`, R-4); no derive crates;
   deliberate leniency documented at the type; no validate-and-forget.
7. **Shape-widener lane**: this chunk is wave 1's one `(w)` chunk —
   the shared insta dump suite grows the `tracking` section in **one
   deliberate regeneration pass, diff-audited**; no other shape
   widener may be in flight.
8. SPDX headers per tier (`MIT OR Apache-2.0` for
   `taskboard-domain`/`taskboard-state`); `#![forbid(unsafe_code)]`
   holds (no new unsafe anywhere); no new dependencies (`cargo deny`
   must stay untouched).

## 2. Verified seam facts (do not re-derive; verified 2026-10-26 against `27343ad`)

| Fact | Anchor | Evidence |
|---|---|---|
| `plan_command` is the pure, total, clock/id-injected catalogue | domain `command.rs` | `plan_command(app, outbox, command, ids, defaults, now) -> Result<CommandPlan, CommandError>` :322-329; `CommandPlan { actions, outcome }` :47-53; helpers `live_task`/`existing_task` :220-233, `enqueue` :1532, `no_op` :1650, `applied_task` :1670 |
| `SetTaskDone` is the implicit-side-effect precedent | domain `command.rs` | :546-592 — `live_task` → done-flip no-op check → stamp → `applied_task` → **if `*done`**: `active_notifications(...)` filtered to `ReminderAnchor::Task(id)` → `RecordReminderDismissal` rows → one `UpdateAppData(id)` op when rows non-empty |
| `DeleteTask` cascade precedent | domain `command.rs` | :612-640 — `existing_task` → tombstone no-op check → `UpsertTask(tombstoned)` → `task_rule_deletes` (:291-297, the per-task cascade-helper shape) → bound ? `DeleteTask` op : unbound `FailOp` sweep |
| Receipt enum & typed errors | domain `command.rs` | `CommandOutcome` :120-198 (`NoOp` :195 the idempotent-repeat precedent; bare unit error variants `InvalidAdhocWhen` :111, `InvalidSnoozePostpone` :115 are the no-payload style); `CommandError` :59-116 is `Copy` |
| `StateCommand` is a serde enum, nothing matches it exhaustively in-tree | domain `messages.rs`, workspace | the enum :20-301 (arm docs are the wire contract); grep confirms no exhaustive `match` over it anywhere (the undo/1 catalogue will be the first — dispatch §1 lane 2); app/IPC/UI only *construct* arms |
| `AppState` nesting + determinism doc | domain `state.rs` | :24-45 — field order = snapshot order (`boards, stacks, tasks, labels, reminders, sync, last_updated`); `Serialize` only; the :4-8 doc pins `BTreeMap` determinism ("a time-entry map follows" — this chunk is that map) |
| `PersistedState` mirrors `AppState`, serde both ways | domain `persistence.rs` | :82-102; `From<PersistedState> for AppState` :104-116; `apply_actions` :264+ is the one exhaustive semantics table both interpreters share |
| `PersistenceAction` is matched with **no wildcard** in storage | storage `repo.rs` | :852-856 comment: "No wildcard arm: a new `PersistenceAction` variant must fail this crate's build (E0004) until storage persists it"; `execute_batch` partitions per kind then writes in fixed order |
| Migrations 0001-0009 exist; **0013 is DR-5/2's** (wave 7) | storage `migrations/`, dispatch §2 | this chunk creates no table; entries must be producible before the table exists (R-11) |
| Ids: one macro, two families; two `IdGenerator` implementors | domain `ids.rs`/`idgen.rs` | `local_id!` :13-39; roaming family :41-60; `IdGenerator` :16-31; implementors = `UuidV7Generator` :37 and `test_support::CountingIds` (grep-verified: exactly two) |
| The engine command path is generic over `plan_command` | state `actor.rs` | `execute_command` :582-675 — plan → apply → `interpret` → publish; `plan.actions.is_empty()` short-circuits as the accepted no-op (:633-639); zero per-arm engine code exists or is needed |
| Memory ≡ disk by construction | state `core.rs` | `EngineCore.state: PersistedState`; `interpret` delegates to domain `apply_actions` :58-63; `app()` maps field-by-field :68-87 |
| Engine harness idioms | state `tests/engine.rs`, `common/mod.rs` | `FixedClock` (logical, advanceable), `CountingIds`, `seed_board_state`, `Harness`/`Spawner` :134-197, `eventually()` poll-with-deadline :216-228, oneshot reply = completion signal; `spawn_state_engine(...).await` returns post-boot |
| Snapshot suite is the `(w)` surface | state `tests/snapshots.rs` | `settle()` :111-134 — per-field memory ≡ disk asserts (boards/stacks/tasks/labels/reminders) + `assert_yaml_snapshot!` of `PersistedState` (19 persisted dumps + 19 tails = 38 `.snap` files; scenarios s1-s35b); **tracking needs one assert line + regen + a new scenario** |
| The arbitrary-command / arbitrary-action strategies enumerate arms | domain `test_support/messages.rs`, state `tests/proptest.rs` | `state_command_strategy` :49-149 (`prop_oneof!` list, grows per arm); `arbitrary_action` ("one strategy arm per action class"); `persisted_state_strategy` feeds P4 |
| `sqlite_smoke.rs` (I1) is the sqlite tier | state `tests/sqlite_smoke.rs` | real tempdir DB + two-boot hydration idiom; **untouched here** — its tracking leg rides chunk 2 (with migration 0013) |
| Day windows are pure, `(day_start_time, offset)` fed | domain `days.rs` | `day_window(now, day_start_time, offset) -> (start, end)` :41-56; consumers pass window data in — `tracked_in_window` takes the window, never config |
| `active_notifications` is the one dismissal source | domain `reminders.rs` | :768-773 `(tasks, reminders, defaults, now) -> Vec<Notification>`; `Notification.anchor`/`reminder`/`occurrence` feed the rows; the actor's `withdrawals_for` picks `RecordReminderDismissal` out of any applied batch (toast withdrawal on Start is free) |
| Kani harness is cfg(kani)-only, sync-phase scoped | domain `verify.rs` | untouched by this chunk |

## 3. Design

### 3.1 The entity and its state home (D-T1, D-T2, D-T3)

New module `crates/taskboard-domain/src/tracking.rs` (`pub mod
tracking;` in `lib.rs` + re-exports):

```rust
/// One settled block of tracked work. The open entry (`end: None`)
/// IS the running timer (ADR 0017); nothing ticks — elapsed is always
/// derived at read.
pub struct TimeEntry {
    pub id: TimeEntryId,           // local_id! — roaming family
    pub task: TaskId,              // reference survives soft delete
    pub task_title: String,        // immutable snapshot at start (D-T2)
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>, // None = open = the running timer
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>, // the LWW mutation clock
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrackingState {
    pub entries: BTreeMap<TimeEntryId, TimeEntry>,
}
```

- Derives: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` on
  `TimeEntry` (the `PersistedState` mirror needs serde both ways; no
  floats, so `Eq` holds). No `Ord`/`Hash` on the struct; the map key
  (`TimeEntryId`) carries `Ord` from the `local_id!` macro.
- `AppState.tracking: TrackingState` — declared **between `reminders`
  and `sync`** (field order is snapshot order, `state.rs` :24-45);
  `Default` fills empty. Mirrored as `PersistedState.tracking` at the
  same position; `From<PersistedState> for AppState` and
  `EngineCore::app()` grow the field mapping (one line each).
- **No `running` field** — the open set is derived. Accessors on
  `TrackingState` (§3.5).

`TimeEntryId` joins the roaming family: `local_id!(TimeEntryId)` in
`ids.rs` (doc: "Local identity of a time entry (`UUIDv7`, generated
client-side)" — roaming like `ReminderRuleId`, *not* device-local like
`AdhocAlarmId`), `IdGenerator::new_time_entry_id()` (required method —
both in-tree implementors grow it: `UuidV7Generator` mints
`Uuid::now_v7()`, `CountingIds` the next counter uuid), exported from
`lib.rs`'s `pub use ids::{...}`.

**A1 span validation seam (R-4).** The entity's `start`/`end` are
plain data — rows decode tolerantly everywhere (persistence, and
chunk 2's sidecar fold folds synced rows as-is). Validity is enforced
at the only *input-adjacent* construction point, the planner, through
a minted newtype in `tracking.rs`:

```rust
/// A closed span the planner has validated (`end > start`). Private
/// fields; the fallible mint is the planner-side trust boundary.
pub struct ClosedSpan { /* start, end private */ }
impl ClosedSpan {
    pub fn try_new(start: DateTime<Utc>, end: DateTime<Utc>)
        -> Result<Self, SpanRejected>;
}
/// The span predicate failed (`end <= start`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("time span invalid")]
pub struct SpanRejected;
```

The planner mints `ClosedSpan` for every user-authored closed span
(`LogTime`, `AdjustTimeEntry`-to-closed) and builds the row *from the
minted values* — validated data flows, nothing validate-and-forget.
`command.rs` maps `SpanRejected` → `CommandError::InvalidSpan`. The
leniency boundary is documented at `TimeEntry`: **planner-written
spans are predicate-clean; decoded rows are trusted-written** (they
were validated at their originating device's planner; the read-side
projections clamp degenerate shapes anyway, §3.5) — the
`ConnectorId` doc pattern (`ids.rs` :100-115).

**Close stamps are exempt from the strict predicate (R-5).** A close
(`StopTracking`, `StartTracking`'s close-all, done/deleted-close,
adjust-set-end) stamps `end = Some(now); updated_at = now`
unconditionally — the planner does not run the `end > start`
predicate on closes. Reason: `StopTracking` must stay total (one
zero-length entry from an instant start-stop, or a skewed
remote-stop close, must not reject the whole command — the origin
plan's risk table already rules "span validation still applies to
adjusts", i.e. to user-authored spans); a skewed close may produce a
zero/negative raw span, which no projection amplifies (`duration` and
`tracked_in_window` clamp negative overlaps to zero, §3.5). Recorded
as deliberate leniency at the type.

### 3.2 The verb payloads (messages.rs — D-T4)

Five new `StateCommand` arms, appended after `CancelAdhocAlarm`
(before `RequestSync`), each with a wire-contract doc comment:

```rust
/// Start tracking the named live task: every open entry closes at the
/// engine-stamped `now` (the single-global intent — starting anywhere
/// stops what any device left running), a fresh open entry is minted
/// (title snapshotted), and the task's currently-active reminder
/// occurrences are dismissed exactly as `SetTaskDone` dismisses them.
/// Idempotent repeat: exactly one open entry exists and it is this
/// task's — the accepted no-op. Unknown/deleted task is a typed
/// rejection.
StartTracking { task: TaskId },
/// Close every open entry at the engine-stamped `now`. Nothing open
/// is the accepted no-op.
StopTracking,
/// Log a retroactive, closed entry for the live task. The span must
/// validate (`end > start`) or the command is a typed rejection.
/// Never touches the open set (logging yesterday's block while
/// today's timer runs is legal). Overlaps with other entries are
/// legal by design.
LogTime { task: TaskId, start: DateTime<Utc>, end: DateTime<Utc> },
/// Adjust an entry's span. Each field is a *change mask*: `None` =
/// unchanged; there is deliberately no way to clear an `end` (a
/// history edit can never resurrect a running timer — ADR 0017).
/// Setting an end on an open entry closes it. The resulting span
/// must validate; a mask naming only unchanged values is the
/// accepted no-op. Entries of soft-deleted tasks stay adjustable
/// (history outlives the task).
AdjustTimeEntry {
    id: TimeEntryId,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
},
/// Delete one entry — globally (chunk 2 drives a wire tombstone).
/// An id naming no entry is the accepted no-op: a synced tombstone
/// may have folded the row away already, making the local repeat
/// redundant rather than wrong.
DeleteTimeEntry { id: TimeEntryId },
```

Note the deliberate contrast with `TaskChanges`' double-Option
(`messages.rs` :319): `AdjustTimeEntry.end` is a plain `Option` — the
change-mask *is* the fence. Document that at the arm (done above);
the stage-3 gate greps for any reopen path.

### 3.3 Receipts and errors (command.rs — D-T4)

`CommandOutcome` grows (full rows ride the receipts so every surface
reads the stamped values without re-reading state — §5 "receipts
carry the stamped values"):

```rust
/// The command opened the entry and closed the named ones (entry-id
/// ascending).
TrackingStarted { entry: TimeEntry, closed: Vec<TimeEntryId> },
/// The command closed these entries, post-stamp (entry-id ascending).
TrackingStopped { entries: Vec<TimeEntry> },
/// The command logged the entry.
TimeLogged { entry: TimeEntry },
/// The command adjusted the entry (post-adjust row).
EntryAdjusted { entry: TimeEntry },
/// The command deleted the named entry.
EntryDeleted { id: TimeEntryId },
```

`CommandOutcome` is `Eq` — `TimeEntry` is fieldwise `Eq`, so the
derive holds. `CommandError` grows two variants in the established
style (`UnknownAdhocAlarm` payload shape; bare-unit for the predicate,
like `InvalidAdhocWhen`):

```rust
/// No time entry with this id exists.
#[error("unknown time entry")]
UnknownTimeEntry(TimeEntryId),
/// A user-authored span must satisfy `end > start`.
#[error("time span must be positive")]
InvalidSpan,
```

### 3.4 Planner arms (command.rs) — exact semantics

Two shared helpers (the `task_rule_deletes` shape, `command.rs`
:291-297), both in `command.rs` next to their one call-site family:

```rust
/// Closes every open entry at `now` (close stamps are exempt from the
/// span predicate — R-5). Returns the upsert rows entry-id ascending
/// and the distinct touched task ids ascending.
fn close_all_open(app: &AppState, now: DateTime<Utc>)
    -> (Vec<PersistenceAction>, Vec<TaskId>);

/// `close_all_open` narrowed to one task's entries (the
/// done/deleted-close shape).
fn task_tracking_closes(app: &AppState, task: TaskId, now: DateTime<Utc>)
    -> Vec<PersistenceAction>;
```

**`StartTracking { task }`** — order of operations (D-T5; the
stage-3 gate: resolve → close-all → open → dismiss →
`UpdateAppData`, a dismissal before the close or a missed op is a
finding):

1. `live_task(app, task)?` — `UnknownTask`/`TaskDeleted` typed. No
   done-gate: tracking a completed task is legal (R-8).
2. **NoOp guard**: if the open set is exactly one entry and
   `entry.task == task` → `no_op()`. Any other open shape (a foreign
   open entry, or a transient double-open on the same task) proceeds
   to close-all — the convergence rule of R-6; the ruling's "*the*
   open entry" is read literally.
3. `close_all_open(app, now)` — rows entry-id ascending; collect
   `closed: Vec<TimeEntryId>`.
4. Mint the new entry: `ids.new_time_entry_id()`, `task`,
   `task_title = task.title.clone()` (the D-T2 snapshot),
   `start = now`, `end = None`, `created_at = updated_at = now`.
   Push its upsert after the close rows.
5. Dismissals — the exact `SetTaskDone` machinery: `active_notifications(...)`
   filtered to `anchor == task`, mapped to
   `PersistenceAction::RecordReminderDismissal(ReminderDismissal {
   reminder, occurrence, dismissed_at: now })`, extended after the
   entry upserts. (The actor's `withdrawals_for` then withdraws the
   OS toasts for free — no engine change.)
6. `UpdateAppData` ops: one `enqueue(LocalOp::UpdateAppData(t))` per
   distinct touched task — closed entries' tasks ascending by task
   id, **the target's op last and always** (a start always mints a
   row the sidecar must flush). Ops ride regardless of the board's
   binding (the flush trigger is content-free; the `DismissReminder`
   precedent enqueues unconditionally).
7. Receipt `TrackingStarted { entry, closed }`.

**`StopTracking`**: open set empty → `no_op()`; else
`close_all_open` at `now`, upserts entry-id ascending, one
`UpdateAppData` op per distinct task ascending, receipt
`TrackingStopped { entries }` (post-stamp rows).

**`LogTime { task, start, end }`**: `live_task` → `ClosedSpan::try_new`
(mapped `InvalidSpan` on `SpanRejected`) → mint a **closed** entry
(`id` fresh, `created_at = updated_at = now` — the span is historical
data, the mutation clock is the planner's `now`, which is what keeps
LWW sane for retroactive rows) → upsert + `UpdateAppData(task)` →
`TimeLogged { entry }`. Never touches the open set; no overlap
validation (R-9).

**`AdjustTimeEntry { id, start, end }`** — the change-mask-never-value
rule (D-T7):

1. `app.tracking.entries.get(&id)` → absent is `UnknownTimeEntry(id)`.
   **No task resolution**: the entry's task may be soft-deleted;
   history stays editable (R-10).
2. Both masks `None` → `no_op()`.
3. Compose the resulting span: `new_start = start.unwrap_or(e.start)`,
   `new_end = end.unwrap_or(e.end)` — `end: None` = unchanged, so
   *clearing is unexpressible*; there is no payload that reopens.
4. Resulting entry equals the stored one → `no_op()` (the
   `MoveTask` identical-position precedent :595 — an adjust that
   changes nothing must not churn `updated_at` onto the sync wire).
5. `new_end` known → `ClosedSpan::try_new(new_start, new_end)` or
   `InvalidSpan`. (An adjust of an *open* entry setting only `start`
   has no end to validate — accepted by construction; the entry stays
   open. Setting `end` on an open entry closes it — the
   forgotten-timer remedy.)
6. Stamp `updated_at = now`; upsert + `UpdateAppData(e.task)`;
   `EntryAdjusted { entry }`.

**`DeleteTimeEntry { id }`**: absent → `no_op()` (R-7's recorded
deviation from `CancelAdhocAlarm`'s typed rejection: chunk 2's fold
applies synced tombstones as local removals, so a local repeat can
legitimately target an already-gone row; ids are copied from
`track list`, never typed from memory). Present →
`PersistenceAction::DeleteTimeEntry(id)` + `UpdateAppData(e.task)`
(the flush trigger chunk 2's tombstone derivation rides) + receipt
`EntryDeleted { id }`.

**`PersistenceAction`** grows two variants (with doc comments, after
`DeleteAdhocAlarm`):

```rust
/// Insert or replace a time entry.
UpsertTimeEntry(crate::tracking::TimeEntry),
/// Remove a time entry (chunk 2 derives the wire tombstone at flush).
DeleteTimeEntry(TimeEntryId),
```

`apply_actions` grows the two arms (`insert` by id / `remove` by id)
and its semantics doc the two sentences. These variants are domain
currency — the engine and `InMemoryRepository` need nothing else (both
interpret through `apply_actions`).

**`SetTaskDone` insertion point (D-T6)** — the tracking close is a
sibling of the dismissal block, *before* it (closes precede
dismissals in the batch, matching the StartTracking order the stage-3
gate enforces): after `applied_task` and only under `if *done`,

```text
if *done {
    let closes = task_tracking_closes(app, *id, now);   // entry-id asc
    if !closes.is_empty() {
        plan.actions.extend(closes);
        plan.actions.push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
    }
    /* the existing dismissal block, unchanged */
}
```

Each block enqueues its own conditional op — two `UpdateAppData` ops
when both fire is correct under the catalogue rule ("every accepted
intent enqueues its own op"; coalescing is push-side, `push.rs`
:371-377). Reopening (`done = false`) closes nothing (a dismissal is
permanent; so is a stamped end).

**`DeleteTask` insertion point (D-T6)** — the close rides the
tombstone ceremony: after `task_rule_deletes(app, *id)` and before
the bound/unbound op split,

```text
let closes = task_tracking_closes(app, *id, now);
if !closes.is_empty() {
    actions.extend(closes);
    actions.push(enqueue(LocalOp::UpdateAppData(*id), ids, now));
}
```

Entries are **closed, not deleted** — history outlives the task by
ruling (D-T10's load-bearing fact); the `UpdateAppData` op enqueues
regardless of `task.remote` (same unconditional-op reasoning as
above; a soft-deleted task keeps its local row, so the op stays
resolvable — the origin plan's flush-trigger note).

**Cascade scope (R-12, the one scope ruling the origin plan left
open):** `DeleteStack`'s cascade (:703-745), `DeleteBoard`'s, and
`ResetBoard`'s paths are **not** grown in this chunk. The origin
plan's D-T6 is definite — "two existing planners grow side-effects" —
and the dispatch Covers line names exactly `SetTaskDone`/`DeleteTask`.
The residual exposure (a stack/board delete orphans the running entry
until the next tracking write) is bounded by the soft invariant (the
next `StartTracking`/`StopTracking` *anywhere* closes all open
entries) and the frozen title keeps the orphaned chip readable; it is
recorded in ADR 0017's consequences. Generalizing the close to the
cascades is a shape-neutral follow-up any later chunk can add.

### 3.5 Pure projections (tracking.rs)

```rust
impl TimeEntry {
    /// `end.is_none()`.
    pub fn is_open(&self) -> bool;
    /// Duration as of `now`: `end.unwrap_or(now) − start`, clamped at
    /// zero (a skew-produced inverted span contributes nothing).
    pub fn duration(&self, now: DateTime<Utc>) -> chrono::TimeDelta;
}

impl TrackingState {
    /// The derived open set — the running timer has no stored home.
    pub fn open_entries(&self) -> impl Iterator<Item = &TimeEntry>;
    /// The reader's single-open pick (D-T8): the most-recently-started
    /// open entry, ties broken by entry id.
    pub fn newest_open(&self) -> Option<&TimeEntry>;
}

/// Total tracked time inside `window` as of `now`. Each entry's span
/// ([start, end), or [start, now) while open) clips to the window;
/// overlaps with the window are summed — the entry is conceptually
/// split at each boundary and only the in-window part counts; rows
/// are never physically split (the days.rs no-derived-bit rule).
/// `now` clamps open accrual at `min(window.end, now)`.
pub fn tracked_in_window<'a>(
    entries: impl IntoIterator<Item = &'a TimeEntry>,
    window: (DateTime<Utc>, DateTime<Utc>),
    now: DateTime<Utc>,
) -> chrono::TimeDelta;
```

`tracked_in_window` takes the window as data (`days.rs` consumer
contract — the CLI chunk passes `day_window(now, day_start_time,
offset)`), and `now` because an open entry's accrual must stop at the
evaluation instant, never run into the window's future.

### 3.6 Engine wiring (state crate)

**Zero new engine code.** `execute_command` is generic over
`plan_command`; the no-op short-circuit (:633-639) gives the idempotent
repeats their "nothing persisted, published, or signaled" behavior for
free. `EngineCore::app()` grows the one field mapping. The notifier's
withdraw direction picks StartTracking's dismissal rows out of the
batch unchanged.

## 4. Decision summary (every open question, resolved)

| ID | Question | Decision |
|---|---|---|
| R-1 | Module & state home | `tracking.rs`; `TrackingState { entries: BTreeMap }`; `AppState.tracking` / `PersistedState.tracking` between `reminders` and `sync`; `From`/`EngineCore::app` mappings; no `running` field |
| R-2 | Entity shape/derives | Exactly D-T1's seven fields; `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`; no `Ord`/`Hash` |
| R-3 | Id family & generation | `TimeEntryId` via `local_id!` (roaming); `IdGenerator::new_time_entry_id` required; both implementors grow (`UuidV7Generator`, `CountingIds`) |
| R-4 | A1 seam | `ClosedSpan` private-field newtype, `try_new` fallible mint at the planner; `SpanRejected` mapped to `CommandError::InvalidSpan`; entity decodes tolerantly, leniency documented at the type |
| R-5 | Close-stamp validity | Closes stamp `end = now` unconditionally (predicate exempt); projections clamp degenerate spans to zero; `StopTracking` stays total |
| R-6 | StartTracking NoOp rule | NoOp iff the open set is exactly one entry ∧ its task is the target; any foreign/double-open shape closes all and reopens |
| R-7 | DeleteTimeEntry repeat | Absent id → accepted no-op (deviation from `CancelAdhocAlarm`'s typed rejection; synced tombstones make local absence benign) |
| R-8 | Done-gate on start | None — a completed task is trackable (`live_task` gates deletion only) |
| R-9 | Overlap validation | None in v1 — overlapping/retroactive entries are legal; per-entry duration math only |
| R-10 | Adjust vs dead tasks | Adjust/DeleteTimeEntry resolve the *entry* only; entries of soft-deleted tasks stay editable (history ruling) |
| R-11 | **Storage-crate boundary** | The two `PersistenceAction` variants + `apply_actions` arms are **chunk 1's** (the planner cannot emit rows otherwise, and the crash-resume acceptance requires them); `repo.rs` grows exactly the E0004-forced arms + partition/write lines, written against D-T9's DDL and **dead until chunk 2's migration 0013** (no producer exists before chunk 3's CLI). Recorded deviation from dispatch prompt 1.2's "storage crate shows NO diff": stage 3's gate reads as *no migration, no codec, no decode, no schema work* — the match arms are the no-wildcard rule's own tax (`repo.rs` :852-856). Chunk 2's stage-1 must be told the arms exist |
| R-12 | Cascade closes | `SetTaskDone` + `DeleteTask` only (the normative letter); stack/board/reset cascades deliberately not grown; orphan exposure bounded by the soft invariant, recorded in ADR 0017 |
| R-13 | Batch ordering | Closes (entry-id asc) → new-entry upsert → dismissal rows → `UpdateAppData` ops (distinct tasks asc, target last/always for start; per-block conditional for done/delete) |
| R-14 | LWW clocks | `created_at`/`updated_at` = planner `now` always (never span data) — retroactive rows stay newest-write |
| R-15 | Projections | `open_entries`, `newest_open` (max by `(start, id)`), `TimeEntry::duration(now)` clamped, `tracked_in_window(entries, window, now)` with open accrual clamped at `min(window.end, now)` |
| R-16 | Strategy growth | `state_command_strategy` + the five arms; `arbitrary_action` + the two action arms; `persisted_state_strategy` + `tracking_state_strategy` (valid spans only — rejection paths are unit tests) |
| R-17 | Engine tier | New `tests/tracking.rs` (A-series style, Miri-clean): start publishes, stop stamps+publishes, crash-resume over a shared `Arc<InMemoryRepository>`; sqlite leg rides chunk 2 |
| R-18 | Snapshot regen | `settle()` gains the tracking memory ≡ disk assert; new scenarios `s36a` (open row) / `s36b` (stamped + logged rows); one diff-audited regen pass over the 19 persisted dumps (tails unchanged) |
| R-19 | ADR 0017 | `docs/adr/0017-time-tracking-open-entries-single-global-intent.org` — draft in §6, stage 2 lands it with the final commits |
| R-20 | Kani / outbox comment | `verify.rs` untouched (sync-phase scope); the stale `outbox.rs` :55-59 doc comment is chunk 2's in-passing fix |

## 5. TDD test plan

**Domain planner tables (`command.rs` `mod tests`, fixtures: `ts()`,
`now()`, `CountingIds`, the existing app-builder)** — each pair lands
test-first:

1. `start_tracking_opens_with_title_snapshot_and_flush_op` — receipt
   fields (`start == T0`, `end: None`, `task_title`), action list
   `[UpsertTimeEntry, EnqueueOp(UpdateAppData)]`.
2. `start_tracking_closes_the_foreign_open_entry_first` — seeded open
   entry on task B; start A: `closed == [b_entry]`, action order per
   R-13, post-apply open set == the new entry.
3. `start_tracking_on_the_only_open_entry_is_a_no_op` — zero actions.
4. `start_tracking_over_a_double_open_closes_and_reopens` — two open
   entries on A: not a NoOp; both stamped; one fresh row.
5. `start_tracking_emits_dismissals_exactly_for_active_occurrences` —
   mirrors `set_task_done_emits_dismissals_exactly_for_active_occurrences`
   (:2109): the active occurrence dismissed, the other task's and the
   already-dismissed one not; `UpdateAppData` rides; the no-occurrences
   case emits no dismissal rows but still flushes the target.
6. `start_tracking_rejects_unknown_and_deleted_tasks` —
   `matches!(err, …)` both shapes, zero actions.
7. `stop_tracking_closes_all_and_reports_stamped_rows` (two open
   entries, distinct tasks; ops ascending) /
   `stop_tracking_with_nothing_open_is_a_no_op`.
8. `log_time_mints_closed_entry_without_touching_the_open_set` —
   incl. `created_at == updated_at == T0` while the span is
   yesterday; overlap with the open entry accepted.
9. `log_time_rejects_inverted_and_empty_spans` — `end < start` and
   `end == start` → `InvalidSpan`, zero actions.
10. `adjust_sets_fields_stamps_and_can_close_an_open_entry` — table:
    start-only on open (stays open); end-set on open (closes);
    both-set on closed (moves the span).
11. `adjust_structurally_cannot_reopen` — `start: Some(x), end: None`
    on a closed entry leaves `end` at its stamped value (the mask
    pin).
12. `adjust_no_op_shapes` — both-`None`; values-equal mask; zero
    actions, `updated_at` untouched.
13. `adjust_rejects_inverted_resulting_spans` — new start after the
    kept end → `InvalidSpan`; `adjust_unknown_entry_is_typed`.
14. `entries_of_deleted_tasks_stay_adjustable_and_deletable` —
    tombstone the task, then adjust + delete succeed.
15. `delete_time_entry_removes_rows_and_enqueues_flush` /
    `delete_time_entry_of_absent_id_is_a_no_op`.
16. `set_task_done_closes_open_entries_at_the_done_stamp` (+ reopen
    closes nothing) and `delete_task_closes_open_entries_but_keeps_the_rows` —
    the side-effect insertion points, incl. the conditional-op and
    close-before-dismissal ordering asserts.
17. `rename_never_rewrites_the_title_snapshot` — start → `UpdateTask`
    title → adjust → `task_title` unchanged.
18. **Property** `single_global_convergence_prop` (proptest, 256
    cases): arbitrary `TrackingState` (0..=4 entries, valid spans,
    arbitrary open subset) × target from a small task pool → plan
    `StopTracking`/`StartTracking`, interpret via `apply_actions`,
    assert open count 0 / exactly 1 (or the literal NoOp
    precondition). Convergence leg: two arbitrary single-global
    states (≤ 1 open each) unioned by a **test-local per-key
    newer-`updated_at`-wins oracle** (documented stand-in for chunk
    2's `union_time_entries`), then stop → 0, start → 1.

**Pure projections (`tracking.rs` `mod tests`)**:

19. `tracked_in_window_matches_the_boundary_split_oracle_prop` —
    proptest over windows × entries (closed spanning/adjacent/
    disjoint + open with `now` beyond the window): the function equals
    the oracle that splits each entry at both boundaries and sums the
    inside segment; plus the once-per-boundary pin (a window fully
    inside a closed span yields exactly the window length) and the
    clamps (open accrual stops at `now`; inverted spans contribute
    zero).
20. Table tests: `newest_open` tie-break (equal starts → greater id),
    empty-map `None`, `duration` clamp.
21. `tracking_state_serde_roundtrip` — incl. an `end: None` row.

**Persistence semantics (`persistence.rs` `mod tests`)**:

22. `upsert_time_entry_replaces_by_id` /
    `delete_time_entry_removes_the_row` — the two `apply_actions`
    arms at their lowest layer.

**Engine tier (`crates/taskboard-state/tests/tracking.rs`, new file,
A-series style — Miri-clean, `FixedClock` + `CountingIds` +
`InMemoryRepository`, oneshot replies as completion signals, no
sleeps)**:

23. `t1_start_publishes_the_open_row_memory_equals_disk` — boot,
    `CreateStack`+`CreateTask`+`StartTracking`, receipt is
    `TrackingStarted`; `shared_state().tracking` holds exactly the
    open entry; `repo.snapshot().tracking` equals it.
24. `t2_stop_stamps_and_publishes` — advance the `FixedClock`,
    `StopTracking`: receipt rows stamped at the advanced instant,
    published + persisted agree.
25. `t3_fresh_engine_over_the_same_repo_sees_the_running_entry` —
    the crash-resume: one `Arc<InMemoryRepository>`; engine 1 boots,
    seeds, starts tracking; graceful shutdown (`SystemEvent::Shutdown`
    + join await, the engine.rs idiom); engine 2 boots over the same
    repo: the open entry hydrates with `start == T0`; `StopTracking`
    on engine 2 closes it — the intent survived process death. (The
    sqlite analogue of this test is chunk 2's, extending the I1
    smoke.)

**Snapshot suite (`snapshots.rs`)**:

26. `settle()` grows `assert_eq!(app.tracking, persisted.tracking,
    "memory ≡ disk: tracking");`.
27. New scenario `s36a` (start → settle: one open row in the
    persisted dump) and `s36b` (start → advance → stop + `LogTime` →
    settle: stamped rows) — the serialized entry shape enters the
    disk contract. Then **one** diff-audited regeneration pass
    (`cargo insta test` → review the 19 persisted-dump diffs (each
    gains `tracking: entries: {}` or the scenario rows) → accept);
    tails must be unchanged.

**Strategy growth (R-16)**: `test_support::tracking_state_strategy`
+ `time_entry_strategy` (spans valid by construction: start instant +
positive offset; open subset arbitrary); `persisted_state_strategy`
carries the new field; `state_command_strategy` grows the five arms
(`LogTime` pairs valid-only). `proptest.rs::arbitrary_action` grows
the two arms (no referential consistency — P2 compares two
interpreters of one batch). P4 then exercises tracking commands
against arbitrary states for free (its rejected/no-op-change-nothing
assertion covers the typed rejections); P5's op-freshness holds by
construction (`UpdateAppData` ops address the touched tasks, which
exist whenever a command is accepted).

## 6. ADR 0017 draft (stage 2 lands as `docs/adr/0017-time-tracking-open-entries-single-global-intent.org`)

```org
#+title: ADR 0017: Time Tracking — Settled Open Entries, Single-Global Intent, Board-Section Roaming
- *Date:* 2026-10-26
- *Status:* Accepted (core domain & engine land with chunk DR-5/1; storage & roaming DR-5/2, CLI & export DR-5/3, UI DR-5/4 follow)
- *Depends on:* ADR 0006 (state-engine interpret loop), ADR 0009/0012 (daemon + IPC), ADR 0013 (the reminders sidecar precedent this design extends), ADR 0004 (sync conflict policy)

* Context

Issue #210 asks for time tracking. The 2026-10-14 sketch kept the
running timer as app-runtime state: persisted only as an in-flight
intent, with a clock-driven app task stamping the end. Since then the
daemon + IPC architecture landed (ADR 0009/0012): a CLI `track start`
relays to the daemon and exits, so the intent must survive process
death as durable state regardless; and an open entry is itself a
settled write in the sketch's own sense — a row that exists, whose
end field is unterminated.

* Decision

1. *The open entry is the running timer.* A `TimeEntry` row with
   `end: null` is engine state: persisted, and (chunk DR-5/2) roaming
   via the synced-intent ruling. This deliberately deviates from the
   2026-10-14 sketch for the reason above. The sketch's actual
   invariant is preserved exactly: *nothing ticks* — elapsed is
   always derived at read (`now − start`), and an end is stamped only
   by a command (stop, start-close, done/deleted-close, adjust) at
   the acting device's planner `now`, from any device.
2. *Single-global intent, soft.* At most one open entry after any
   local tracking write, globally: starting anywhere closes what any
   device left running. Concurrent starts on two devices (before
   either pull sees the other) union into two open entries — the
   LWW merge cannot prevent it, and no reader breaks: the chip shows
   the most-recently-started open entry, `track status` lists all,
   and the next start or stop anywhere closes all. The invariant
   converges to zero-or-one at the next write; the domain properties
   pin exactly this. No device identity exists or is introduced
   (attribution stays fenced, Wave 4).
3. *History outlives tasks.* Entries live in a board-level
   `time-entries` sidecar section keyed by entry UUID (chunk DR-5/2);
   `prune_absent_cards` gains no arm for it, so deleting a task never
   deletes its tracked history. `task_title` is an immutable snapshot
   taken at start. Deliberate deletes roam via derived tombstones
   (global); retention aging (180 days, wire-only, constant) does not
   — asymmetric by design: the local DB never drops history.
4. *History editing can close but never reopen.* `AdjustTimeEntry`'s
   payload is a change mask: setting an end is expressible, clearing
   one is not — a history edit cannot resurrect a running timer. The
   runaway auto-close cap was considered and declined (session
   ruling, 2026-10-26): runaways are fixed by hand via
   `track adjust`; revisit only with poisoned-statistics evidence.
5. *Starting settles notifications.* `StartTracking` dismisses the
   task's currently-active reminder occurrences with the exact
   `SetTaskDone` machinery — the notification surface's Start Task
   button needs no second command (the #210 cross-link).

* Consequences

- A skewed stop stamps a slightly wrong duration, never an invalid
  one: close stamps are predicate-exempt and read-side projections
  clamp degenerate spans to zero; strict `end > start` validation
  applies to user-authored spans only (`log`/`adjust`).
- The soft invariant's transient double-open is observable between
  sync cycles; it is recorded honestly here rather than papered over
  with device identity.
- Task-tombstoning ceremonies close open entries only at
  `SetTaskDone` and `DeleteTask` (v1): a stack/board delete can
  orphan the running entry until the next tracking write closes it —
  bounded by the soft invariant, readable via the frozen title;
  generalizing the close to the cascades is shape-neutral follow-up.
- Fences (backlog, vocabulary reserved): tracker task type, respite
  marking, per-user attribution (Wave 4), break reminders, pomodoro,
  today-view totals section, task-detail history list.
```

## 7. Stage-2 execution order (TDD pairs → standalone green commits)

1. **M1**: `ids.rs`/`idgen.rs`/`test_support` id seam (test: the
   id-kind roundtrip extension) → `tracking.rs` entity + `TrackingState`
   + serde test → `state.rs`/`persistence.rs` field + `apply_actions`
   arms (+ their two tests) + `From`/`EngineCore::app` mappings +
   `PersistenceAction` variants → storage `repo.rs` forced arms (R-11)
   → `lib.rs` exports. Regenerate snapshots in this milestone's final
   commit (the shape lands whole; the s36 scenarios may wait for M5).
2. **M2**: the five planner pairs (tests of §5.1-9, 13-15, then arms),
   incl. `ClosedSpan`.
3. **M3**: the `SetTaskDone`/`DeleteTask` pairs (§5.16) + the
   dismissal-mirror test (§5.5).
4. **M4**: projections + properties (§5.17-21).
5. **M5**: engine tier `tests/tracking.rs` (§5.23-25) + `settle()`
   assert + `s36a`/`s36b` + the one diff-audited snapshot regen pass.
6. **M6**: ADR 0017; then the full AGENTS §10 gate suite
   (`fmt --check`, `clippy -D warnings`, `nextest`, `--doc`, `deny`,
   the three-command coverage sequence + quarantine script + trailing
   `rm`). Every commit standalone (rebase-merge hygiene); subjects
   `feat(domain): …` / `feat(state): …` / `docs(adr): …` style.

Stage-2 red lines, from the dispatch prompt: nothing ticks; no
`running` field; domain purity (`now` as data); typed outcomes only;
no sleeps; SPDX/forbid rules; no push, no PR, no backlog-issue edits.

## 8. Files touched & estimated diff size

| File | Change | ~Lines |
|---|---|---|
| `crates/taskboard-domain/src/tracking.rs` | new: entity, `TrackingState`, `ClosedSpan`, projections + tests | 420 |
| `crates/taskboard-domain/src/command.rs` | 5 arms, 2 helpers, `ClosedSpan` mapping, receipts/errors + ~18 tests/property | 560 |
| `crates/taskboard-domain/src/messages.rs` | 5 `StateCommand` arms + docs | 60 |
| `crates/taskboard-domain/src/persistence.rs` | 2 variants + `apply_actions` arms + docs + 2 tests | 60 |
| `crates/taskboard-domain/src/state.rs` | `AppState.tracking` field | 8 |
| `crates/taskboard-domain/src/ids.rs` | `TimeEntryId` | 7 |
| `crates/taskboard-domain/src/idgen.rs` | trait method + impl | 10 |
| `crates/taskboard-domain/src/lib.rs` | module + re-exports | 10 |
| `crates/taskboard-domain/src/test_support/{mod,messages,state}.rs` | `CountingIds` method; command strategy arms; tracking/persisted strategies | 70 |
| `crates/taskboard-state/src/core.rs` | `app()` field mapping | 3 |
| `crates/taskboard-state/tests/tracking.rs` | new: t1-t3 engine tier | 280 |
| `crates/taskboard-state/tests/snapshots.rs` | settle assert + s36a/s36b | 45 |
| `crates/taskboard-state/tests/snapshots/*.snap` | 19 persisted dumps grow the section + 4 new files (mechanical regen) | ~70 |
| `crates/taskboard-state/tests/proptest.rs` | 2 action arms | 30 |
| `crates/taskboard-storage-sqlite/src/repo.rs` | E0004-forced arms + partition/write lines (R-11) | 20 |
| `docs/adr/0017-time-tracking-open-entries-single-global-intent.org` | new | 110 |

Estimated total ≈ **1 700–1 900 lines including tests, snapshots, and
the ADR** (production code ≈ 550–600); size **M**, matching the
dispatch table. No new dependencies; `cargo deny` untouched.

## 9. Risks & mitigations

- **The storage-crate deviation (R-11)** is the chunk's one judgement
  call against the prompt's letter. Mitigated by: the in-tree no-
  wildcard rule leaves no alternative that keeps the workspace
  compiling and the crash-resume acceptance implementable; the arms
  are dead until chunk 2's migration (no producer exists before
  chunk 3); this plan is the recorded reason; chunk 2's stage-1 gets
  the handoff note.
- **Widening `state_command_strategy` widens P4's space**: a new
  property failure is a *bug candidate first* (AGENTS §5) — stage 2
  triages production code before touching the test; shrinking the
  strategy instead of fixing is a finding.
- **Close-stamp leniency (R-5)** can produce zero/negative raw spans
  under skew; projections clamp and the property oracle encodes the
  clamp, so no negative duration can escape to a surface.
- **Snapshot regen churn**: the (w) lane is single-flight (wave 1 has
  exactly this chunk); the regen is one audited pass; tails must show
  zero diff — a tail diff means the memory ≡ disk wiring is wrong.
- **`CommandOutcome` grows heavier** (full rows): receipts are
  in-process/IPC-local and `Eq`; the CLI chunk consumes them directly
  per the origin plan's receipt contract. No wire-shape concern (IPC
  relays argv, not receipts).
- **Migration-number drift**: not this chunk's concern (no
  migration); chunk 2 re-verifies 0013 at its own base per dispatch
  §2.

## 10. Handoff notes

- **Chunk DR-5/2's stage 1**: the two `PersistenceAction` variants and
  the `repo.rs` match arms already exist (dead until your migration);
  your codec must make the stored shape exactly what
  `apply_actions`/`InMemoryRepository` already round-trip (the
  domain type is the contract — D-T9's DDL is pinned in the origin
  plan §Part C); the stale `outbox.rs` :55-59 comment fix is yours.
- **Stage 3**: the storage diff reviews as R-11 (E0004 tax only);
  grep gates: no `running` field, no ticking task, no reopen path,
  `StartTracking` action order, done/deleted close stamps, empty
  `sqlite_smoke`/migration/codec diff.
