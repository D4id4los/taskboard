# [Plan] Undo tracking & habit arms (undo v2 widening) — the six
# mechanically-invertible arms join the inverse-synthesis catalogue

- **Date**: 2026-10-31 (dispatch slot; written 2026-11-01)
- **Kind**: stage-1 implementation plan for dispatch chunk **1.9** of
  backlog round 2 (#555, branch `feat/undo-tracking-habit-arms`).
  TDD-first; no production code in this commit.
- **Origins** (normative, in precedence order):
  GitHub issue **#555** (the six-arm list and the exclusion list,
  verbatim); `.artifacts/plans/2026-10-30-undo-journal-plan.md` **§3**
  (the 44-arm classification table — rows 33–43 — and **decision D20**,
  which scheduled exactly this widening); **ADR 0019** (the journal's
  rules: single-advance, the pushed-edit rule, D7's watermark scope,
  R-U6 refuse-and-name); **ADR 0017** (why `StartTracking`/
  `StopTracking` are structurally non-invertible and why a history edit
  cannot clear an end); the round-2 dispatch plan §1/§2/§3 chunk 1.9.
- **Branch / worktree**: `feat/undo-tracking-habit-arms` in
  `../taskboard-WT-feat-undo-tracking-habit-arms`, branched off
  `origin/main` @ `e8a919cd` (2026-11-01; local `main` was 20 commits
  behind, so — the undo/1 precedent — every seam below was read **at
  the worktree base**, which is the dispatch's "then-current `main`").
  `scripts/worktree-doctor.sh` exit 0 (235G free, floor 25G).
- **Status**: **READY** — dispatched by the round-2 dispatch plan §3
  chunk 1.9; preconditions verified (§2).

---

## 1. Mission & binding constraints

Widen the undo v1 catalogue (ADR 0019) by exactly the six
mechanically-invertible arms issue #555 names — `LogTime`,
`AdjustTimeEntry`, `CreateHabit`, `UpdateHabit`, `MoveHabit`,
`AdjustHabitLog` — reusing **only** existing verbs as inverses, the
existing journal substrate, and the existing refusal vocabulary. No
new command surface, no storage schema change, no engine wiring
change: the whole production diff lands in `taskboard-domain`
(`journal.rs` shapes, `undo.rs` classification + capture + synthesis).

Binding, restated and re-verified on this branch (§2):

1. **Single-advance rule untouched** (ADR 0006/0019): this chunk adds
   zero new `PersistenceAction` variants and zero apply-arm changes —
   the widened facts ride the existing `RecordJournal` payload.
2. **The pushed-edit rule untouched** (ADR 0019 decision 2): every new
   inverse plans through `plan_command` and inherits
   `plan_undo`'s two op behaviours verbatim (§9). No new op handling.
3. **`plan_command`'s catalogue is NOT edited** (R-U8, held by undo/1):
   pre-images and watermarks are captured after planning by the
   wrapper's arm-agnostic pass; the synthesis re-plans existing verbs.
   The planner arms at `command.rs:2061-2406` stay byte-identical.
4. **R-U6 refuse-and-name, R-U7 checked-scope, D7 per-arm write set**:
   the widened arms obey the same watermark discipline as the 13 v1
   arms; the three dynamic exclusions this chunk records (§7) journal
   `NotUndoable(reason)` at the classification site — the same
   shape `LogHabit` already has.
5. **AGENTS §5 discipline**: typed outcomes via `matches!`, no text
   assertions, fixed/stepping clocks, one behavior one test at the
   lowest layer, no quality ceilings.
6. **The exclusion set is frozen** (dispatch hard rule): `StartTracking`,
   `StopTracking`, `LogHabit` stay `NotUndoable` with their reasons —
   silently shrinking it is a deviation to report, never make. So do
   `DeleteTimeEntry` and `DeleteHabit` (restore-class shape — no
   un-delete verb exists; §4 rows 7–8).

## 2. Verified preconditions, base, and resource ladder

| Check | Result |
|---|---|
| Issue #555 | OPEN, `architecture`+`p3`, no milestone — this chunk's normative arm list |
| undo/1 landed | ADR 0019 accepted; journal + synthesis + engine + storage live at the base; `NotUndoableReason` already carries `TrackingSurface`/`HabitSurface` |
| undo/2 surfaces (#510) | landed (CLI/IPC surfaces); the engine contract is what this chunk widens — no surface edits needed (labels exist, `journal.rs:339-347`) |
| Migration in flight? | **No migration in this chunk at all** (shape-widening is serde-additive inside the existing JSON payload, D22) |
| ADR 0019 | untouched by production code; a dated one-line amendment note only (§10, the acceptance clause) |
| `worktree-doctor.sh` | exit 0; worktree created `git worktree add -b feat/undo-tracking-habit-arms ../taskboard-WT-feat-undo-tracking-habit-arms origin/main` |
| Snapshot corpus at base | tracking/habit scenarios **s36b, s45a, s45b** carry `journal:` facts that widen (§11.8) — 3 pairs, one deliberate regeneration pass |
| v1 pins that name the widened arms | `not_undoable_heads_refuse_and_name_the_reason` (domain) asserts LogTime/CreateHabit heads refuse — **updated, not weakened** (§11.6, D30); the e2e habit pin survives (its head is `LogHabit`) |

## 3. What the planners already do (the seam facts this plan builds on)

Re-derived at the base; these facts decide every design below.

| Arm | Planner (`command.rs`) | Batch actions | Op in batch? | Pre-image today? |
|---|---|---|---|---|
| `LogTime` :2150 | mints `TimeEntry` (`new_time_entry_id`), strict `ClosedSpan`, never touches the open set | `UpsertTimeEntry` + `EnqueueOp(UpdateAppData(task))` | yes | **yes** — `PreImage::TimeEntry { id: minted, before: None }` via the existing `UpsertTimeEntry` walk (`undo.rs:301`) |
| `AdjustTimeEntry` :2175 | change mask (`start`/`end`, `None` = unchanged; **no payload clears an end**), identical mask = no-op, entry-only resolution | `UpsertTimeEntry` + op | yes | yes — `before: Some(entry)` |
| `DeleteTimeEntry` :2213 | absent = accepted no-op; hard remove + wire tombstone at flush | `DeleteTimeEntry(id)` + op | yes | no (delete action; stays excluded) |
| `CreateHabit` :2229 | mints `Habit` (`new_habit_id`), appends order (D-H11), `anchor_time: None`, `continued_from: None` | `UpsertHabit` + `EnqueueOp(FlushBoardAppData(board))` | yes | **yes** — `PreImage::Habit { id: minted, before: None }` (`undo.rs:294`) |
| `UpdateHabit` :2264 | sparse `HabitChanges { title, target, anchor_time, continued_from }`; first three stamp their `HabitClocks` slot **only when the value changed**; **`continued_from` stamps no clock** (D-H7a); identical mask = no-op | `UpsertHabit` + flush op | yes | yes — `before: Some(habit)` |
| `MoveHabit` :2304 | order only, `clocks.order = now`, identical order = no-op | `UpsertHabit` + flush op | yes | yes — `before: Some(habit)` |
| `LogHabit` :2320 | +1 into the attribution-picked window, never a no-op | **`RecordHabitLog(key, row)`** + flush op | yes | no (`Record*`, D19) — stays excluded |
| `AdjustHabitLog` :2341 | canonical-window gate (current target kind), identical count = no-op, count 0 legal (row kept) | **`RecordHabitLog(key, row)`** + flush op | yes | **no — the one real gap; §5 closes it** |
| `DeleteHabit` :2384 | tombstone only; **the log map is never touched** (history outlives the tracker); repeat = no-op | `UpsertHabit` + flush op | yes | no (stays excluded) |
| `StartTracking`/`StopTracking` :2061/:2128 | close-all-open + mint / close-all | `UpsertTimeEntry` per touched row + dismissal rows + ops | yes | yes — **irrelevant**: stays excluded (ADR 0017: the inverse would have to re-open closed entries; no command expresses that) |

The two structural facts everything hinges on:

- **The habit log map lives outside the habit entity** —
  `HabitState { habits: BTreeMap, log: BTreeMap<HabitLogKey,
  HabitLogEntry> }` (`habit.rs:293-298`). `PreImage::Habit` therefore
  does **not** carry any window counts, and `RecordHabitLog` writes
  only the log map (`persistence.rs:388`) — no `UpsertHabit` rides an
  adjust batch. The journal today holds nothing an `AdjustHabitLog`
  inverse could read.
- **`HabitLogEntry { count, clock }` carries its own LWW write clock**
  (`habit.rs:274-280`) — the per-window watermark exists at the row
  level without touching `HabitClocks` (which deliberately has no log
  slots, `habit.rs:148-164`).

## 4. The widened classification table (this chunk's §3 rows)

Continuing the undo/1 plan §3's numbering. "Watermark scope" = exactly
the inverse's write set (D7) — what the R-U7 check compares.

| # | Arm | Class (new) | Inverse (synthesis) | Pre-image source | Watermark scope | Reason / notes |
|---|---|---|---|---|---|---|
| 35′ | `LogTime` | create-class | `DeleteTimeEntry { id }` on the minted id | journal entry: `PreImage::TimeEntry { id, before: None }` — **the minted id is journaled, never re-derived** (the stage-3 trap) | the created entry's `updated_at` stamp (its single clock covers span + title snapshot): any later adjust ⇒ refuse — the CreatedTask content-stamp precedent ("any later edit ⇒ refuse — protects the edits the delete would destroy") | Task deletion since does **not** block: `DeleteTimeEntry` resolves entry-only (R-10). Open-set untouched by `LogTime` ⇒ no side effects to grandfather |
| 36′ | `AdjustTimeEntry` | same-verb invertible **unless it closed an entry** (§7.1) | `AdjustTimeEntry { id, start: pre_start?, end: pre_end? }` — mask of the divergent fields only | journal entry: `PreImage::TimeEntry { before: Some }` | the entry's `updated_at` stamp — one check covers both mask fields; also structurally guards the close case at undo time | The close case (`pre.end = None → post.end = Some`) is **inexpressible**: the mask cannot clear an end (ADR 0017 §4). Refused at record time (D23), not at undo time. Start-only adjusts of an open entry undo fine (the entry stays open; no span re-validation fires) |
| 38′ | `CreateHabit` | create-class | `DeleteHabit { id }` on the minted id | journal entry: `PreImage::Habit { id, before: None }` — the minted id rides the entry | the created habit's content clocks `{ title, target, order, anchor_time }`: any later editor ⇒ refuse | **No occupancy guard, unlike `CreateStack`** (D26): `DeleteHabit` cascades nothing — the log map outlives the tombstone by construction (`command.rs:2394`), so a post-create `LogHabit` cannot be destroyed by the undo. A post-create seam link (`continued_from`) is stranded honestly with the tombstone (D26). A manual delete since makes the undo an accepted no-op repeat (the `DeleteTask` tombstoned-repeat precedent, `command.rs` DeleteTask arm) |
| 39′ | `UpdateHabit` | same-verb invertible **unless the seam is addressed** (§7.3) | `UpdateHabit { habit, changes }` — `HabitChanges` diffed pre vs current over `{ title, target, anchor_time }` (the `task_changes_from_pre` pattern, `undo.rs:973`); double-option `anchor_time` round-trips (cleared-then-undone re-clears) | journal entry: `PreImage::Habit { before: Some }` | per divergent field, its `HabitClocks` slot (`title`/`target`/`anchor_time`) — the v1 UpdateTask mechanics with three fields | `continued_from` is **outside** the write set and outside the check: no clock slot exists (D-H7a), so a divergence could neither be verified nor protected — refused at record time instead (D25). Target-kind changes undo cleanly: former-kind rows were never deleted (D-H12), the restore re-activates them |
| 40′ | `MoveHabit` | same-verb invertible | `MoveHabit { habit, order: pre.order }` | journal entry: `PreImage::Habit { before: Some }` — **the pre order position** comes from the pre-image entity's `order` field, never from the command payload | the habit `order` clock | Identical-order repeats journal nothing (planner no-op ⇒ empty-batch rule) |
| 42′ | `AdjustHabitLog` | same-verb invertible **unless the window row was absent** (§7.2) | `AdjustHabitLog { habit, window, count: pre_count }` — **the pre count comes from the journal entry's `PreImage::HabitLogRow`** (new shape, §5), never from the change payload | journal entry: `PreImage::HabitLogRow { key, before: Some(row) }` + `EntityStamps::HabitLog(post_row)` | the touched window's post-write row `(count, clock)` — compared as a whole: any later write to that window (another adjust; a `LogHabit` landing there) moves the clock ⇒ refuse | Absent-window adjusts (creating a row where none stood) are **lossy to invert**: the inverse would leave an explicit count-0 row — a different stored fact than "no row" (`HabitState.log` distinguishes them; no log-row delete action exists) — the exact reason `LogHabit` is excluded. Refused at record time (D24). Pre row present with count 0 inverts cleanly (row existed, row restored) |

**Stay excluded — the reasons restated at the classification site**
(`undo.rs:145-159`, where the groups live after this chunk):

| Arm | Reason (restated) |
|---|---|
| `StartTracking` | Structurally non-invertible (ADR 0017): the start closes every foreign open entry and mints one; the inverse would have to re-open closed entries — no command expresses that (no payload clears a stamped end; `AdjustTimeEntry` cannot un-close; `DeleteTimeEntry` cannot reopen). A synthesis outside the catalogue is forbidden. Also dismissal side effects (D-T5). `TrackingSurface` |
| `StopTracking` | Same structural reason: its closes cannot be re-opened. `TrackingSurface` |
| `LogHabit` | Lossy to invert: undoing the +1 needs the pre-row shape — a pre-log *absent* window would become an explicit count-0 row, a different stored fact (the domain deliberately keeps "emptied window keeps its row" distinct from "no row"), and no log-row delete action exists. `HabitSurface` |
| `DeleteTimeEntry` | No un-delete verb exists (`LogTime` mints fresh ids — resurrect-by-re-log is identity churn); restore-class shape (#356 family for v2). Journals `TrackingSurface` at the code site today — **left as-is**; the variant's own doc already names it restore-class (a pre-existing doc/code divergence this chunk does not touch) |
| `DeleteHabit` | No `RestoreHabit` verb; tombstone-only delete. `HabitSurface` at the code site today — left as-is, same note |

## 5. Journal shape widening (D22) — serde-additive, zero migration

All in `crates/taskboard-domain/src/journal.rs`; every touched enum
already derives `Serialize + Deserialize` and rides the
`command_journal.payload` JSON blob (`D13`), so **new variants are
additive payloads, not schema**: the SQL tables, the head table, and
hydration are untouched — the dispatch's "no storage schema change"
rule holds by construction.

```rust
// PreImage gains (after TimeEntry — append-only keeps Ord stable):
HabitLogRow {
    key: HabitLogKey,
    before: Option<HabitLogEntry>,   // None = the window had no row;
},                                   // unreachable when journaled undoable (D24),
                                     // kept for the enum's `before` convention
// JournalEntity gains:
HabitLog(HabitLogKey),               // Ord via declaration order (appended last)
// EntityStamps gains:
HabitLog(HabitLogEntry),             // the post-write row (Copy) — count AND clock
// UndoField gains:
HabitTarget, HabitAnchorTime, HabitLogCount,
// (TimeEntry refusals reuse the existing UndoField::UpdatedAt — the
// entry's one clock IS the compared stamp; Title/Order are reused for
// habits, the JournalEntity in the refusal payload disambiguates.)
```

The wrapper's two capture passes gain one match arm each
(`undo.rs:261-359`):

- `pre_images`: `PersistenceAction::RecordHabitLog(key, _)` ⇒
  `PreImage::HabitLogRow { key, before: app.habits.log.get(&key).copied() }`.
- `watermark_of`: `RecordHabitLog(key, post)` ⇒
  `JournalEntity::HabitLog(key)` → `EntityStamps::HabitLog(*post)`.

`touched_board` (`undo.rs:245-249`) and `command_label`
(`journal.rs:342-347`) already cover all six arms — no changes.

## 6. Wrapper classification (`record_entry`) — three dynamic rules

The v1 classification is per-arm static. Three of the six arms need a
**record-time dynamic** rule — inspected against the pre-state and the
plan's own upsert payload, which `record_entry` already holds. Each
rule journals `NotUndoable(reason)` exactly when the inverse is
inexpressible, so every `PreImages` entry's undo restores the pre-state
literally (the dispatch acceptance), and every refusal is the existing
R-U6 typed shape — zero new `CommandError` variants.

```rust
// undo.rs record_entry — the tracking group splits:
StateCommand::LogTime { .. } | StateCommand::CreateHabit { .. }
| StateCommand::MoveHabit { .. } => PreImages (unconditional),
StateCommand::AdjustTimeEntry { id, .. } => {
    // §7.1: the plan closed an open entry?
    closed = pre.end.is_none() && plan's UpsertTimeEntry(id).end.is_some()
    closed => NotUndoable(TrackingSurface), else PreImages
}
StateCommand::UpdateHabit { changes, .. } => {
    // §7.3: the seam addressed?
    changes.continued_from.is_some() => NotUndoable(HabitSurface), else PreImages
}
StateCommand::AdjustHabitLog { habit, window, .. } => {
    // §7.2: the window row absent?
    !app.habits.log.contains_key(&key) => NotUndoable(HabitSurface), else PreImages
}
// StartTracking | StopTracking | LogHabit | DeleteTimeEntry  => TrackingSurface (LogHabit⇒HabitSurface)
// DeleteHabit                                                => HabitSurface   (unchanged)
```

The synthesis-side `command_class()` (`undo.rs:555`) mirrors: the six
arms move from the defensive `Err` arm into new `InverseClass` rows —
`CreatedTimeEntry`, `AdjustTimeEntry`, `CreatedHabit`, `UpdateHabit`,
`MoveHabit`, `AdjustHabitLog` — safe because `plan_undo` consults the
facts first: a dynamically-`NotUndoable` entry refuses before its class
is read. The `no-wildcard` lockstep stays exhaustive in both matches.

## 7. The dynamic rules, each with its recorded reason

1. **`AdjustTimeEntry` that sets an end on an open entry ⇒
   `NotUndoable(TrackingSurface)`.** The inverse needs
   `end: None`, and the change mask structurally cannot clear an end —
   "a history edit cannot resurrect a running timer" is ADR 0017 §4's
   own rule, now enforced at the classification site. Refuse-and-name
   at record time beats journaling facts that claim an invertibility
   the verb cannot deliver; the undo-time refusal is the existing
   `TargetNotUndoable(TrackingSurface)` (D23).
2. **`AdjustHabitLog` onto a window with no pre-existing row ⇒
   `NotUndoable(HabitSurface)`.** The inverse `count: 0` would leave an
   explicit count-0 row where the pre-state had none — the exact
   stored-fact lossiness that keeps `LogHabit` excluded (§4). No
   log-row delete action exists; inventing one is a new command surface
   (forbidden). Pre row present (any count, 0 included) inverts
   exactly (D24).
3. **`UpdateHabit` with `changes.continued_from.is_some()` ⇒
   `NotUndoable(HabitSurface)`.** The §4.7 conversion seam carries no
   clock slot by design (D-H7a, "conversion-seam data, not contested
   content — same treatment as `Task.remote`"), so the watermark could
   neither verify the field unchanged since (R-U7 needs a compared
   stamp) nor protect a later seam write from a blind pre-image
   restore. Restoring the clocked content fields while silently
   keeping a mis-issued link would print "Undone" over an unrestored
   field; refusing names the truth (D25).

## 8. The synthesis rows (`synthesize_undo` additions)

Mechanically the v1 patterns, restated per arm for the stage-2 agent:

- **`CreatedTimeEntry`** (LogTime): minted id from
  `PreImage::TimeEntry { id, before: None }` (missing ⇒
  `NothingToUndo`, the defensive shape); guard: current row exists and
  `current.updated_at == recorded` else
  `TargetNoLongerInvertible { entity: TimeEntry(id), field: UpdatedAt }`;
  inverse `StateCommand::DeleteTimeEntry { id }`.
- **`AdjustTimeEntry`**: pre from `before: Some`; one guard
  (`updated_at` stamp) covers both fields; mask built by diffing
  pre vs current — `start: (pre.start != cur.start).then(pre.start)`,
  `end: (pre.end != cur.end).then(pre.end)` (a `pre.end: None`
  divergence is unreachable — the close case never journals `PreImages`;
  commented as such). The inverse re-plans through the catalogue, whose
  identical-mask no-op and span validation apply.
- **`CreatedHabit`** (CreateHabit): minted id from
  `PreImage::Habit { id, before: None }`; guard: the four content
  clocks equal the recorded stamps (field-named refusals: `Title`,
  `HabitTarget`, `Order`, `HabitAnchorTime`; missing row ⇒ refuse);
  inverse `StateCommand::DeleteHabit { id }`.
- **`UpdateHabit`**: pre from `before: Some`; per-field diff with
  per-field clock checks (`title`, `target`, `anchor_time` — the
  `task_changes_from_pre` shape, three fields, `HabitChanges::default()`
  base, `continued_from: None` always); inverse `UpdateHabit { changes }`.
- **`MoveHabit`**: pre from `before: Some`; guard: `clocks.order`;
  inverse `MoveHabit { habit, order: pre.order }`.
- **`AdjustHabitLog`**: key from the stored command payload
  (`AdjustHabitLog { habit, window, .. }`); pre count from
  `PreImage::HabitLogRow { before: Some(row) }` (`before: None` ⇒
  refuse — unreachable per D24, defensive); guard:
  `app.habits.log.get(&key) == Some(&recorded_post_row)` (whole-row
  compare — `EntityStamps::HabitLog`), field `HabitLogCount`; inverse
  `AdjustHabitLog { habit, window, count: pre.count }`.

No new occupancy guard in `plan_undo` (D26: `DeleteHabit` cascades
nothing). The `CreateStack` guard stays the only one.

## 9. Line mechanics — inherited wholesale, stated once

- **Pushed-edit rule**: every new inverse enqueues (the delete/flush
  ops its catalogue verb plans), so `plan_undo`'s two behaviours apply
  verbatim: un-pushed head ⇒ the inverse's enqueues dropped +
  `FailOp(entry.ops)` retraction (the server never knew); pushed head ⇒
  the inverse's ops ride out as a new edit. Zero new op code.
- **Redo** re-plans the forward intent: `AdjustTimeEntry`,
  `UpdateHabit`, `MoveHabit`, `AdjustHabitLog` redo exactly (same ids,
  fresh stamps); `LogTime`/`CreateHabit` redo **re-mint fresh ids** —
  the v1 create-redo precedent (the e2e pin: "the old id stays
  retired"). A follow-up undo of such a redo refuses typed (the old
  id's watermark row is gone) — same as `CreateTask`'s v1 behaviour.
- **Tombstone-blocking keeps the widened arms safe**: any
  `DeleteHabit`/`DeleteTimeEntry` interposed after one of the six is
  itself a `NotUndoable` head — the line cannot retreat past it (R-U6
  never skips), so no inverse ever meets a tombstoned or deleted
  target through undo. A property, not a guard.
- **Dynamic-`NotUndoable` entries are unreachable as redo targets**
  (undo refuses at them; D5's defensive `TargetNotUndoable` covers the
  impossible) — unchanged machinery.
- **Repeat/no-op edges**: identical-order `MoveHabit`, identical-count
  `AdjustHabitLog`, unchanged-mask `AdjustTimeEntry`, unchanged-mask
  `UpdateHabit` all journal nothing (planner no-ops ⇒ the empty-batch
  rule) — pinned by one assertion line each in the capture test.

## 10. ADR 0019 + CHANGELOG (docs, per the acceptance clause)

- **ADR 0019**: one dated amendment line under the Decision §6
  paragraph (the classification reference moved): "2026-11-01 (chunk
  #555): the mechanically-invertible tracking/habit rows joined the
  undoable catalogue — `LogTime`, `AdjustTimeEntry` (unless it closes
  an entry), `CreateHabit`, `UpdateHabit` (unless the conversion seam
  is addressed), `MoveHabit`, `AdjustHabitLog` (unless the window row
  was absent); `StartTracking`/`StopTracking`/`LogHabit` and the
  delete verbs stay excluded with their recorded reasons." Nothing
  else in the ADR moves.
- **CHANGELOG** Unreleased/Added: one entry — undo now covers time-log
  and habit bookkeeping (the six verbs, the two/three honest
  exclusions named).

## 11. Test catalogue (TDD-first; AGENTS §5 throughout)

Build order = test-first order; every commit standalone green (§13).
Lowest layer first: the domain suite (`undo/tests.rs`, the `Harness`
pure loop) owns semantics; the engine suite owns routing + the
published-state round-trips the dispatch acceptance names; storage owns
the codec.

1. **Shapes + capture** (`journal/tests.rs`, `undo/tests.rs`):
   - serde round-trip of a `JournalEntry` carrying
     `PreImage::HabitLogRow` (both `before` shapes) +
     `EntityStamps::HabitLog` + the new `UndoField`s
     (`journal_entry_round_trips_through_serde_all_facts_variants`
     gains the variants).
   - `pre_images`/`watermark_of` walk `RecordHabitLog`: an
     `AdjustHabitLog` onto an existing window journals
     `PreImage::HabitLogRow { before: Some }` and
     `EntityStamps::HabitLog(post)`; the no-op repeats (identical
     count/order/mask) journal nothing.
2. **Per-arm round-trips — domain** (one per arm; the
   `verb → undo → pre-state values equal (fresh stamps) → redo →
   post-state values equal` shape of
   `undo_of_update_restores_pre_image_values_with_fresh_clocks`):
   - `log_time_round_trips`: LogTime ⇒ undo deletes the entry
     (retraction: `FailOp(entry.ops)`, zero enqueues) ⇒ redo re-mints
     (fresh id — structural equality of task/span/title, old id gone).
   - `adjust_time_entry_round_trips` (closed entry, end moved):
     undo restores pre start+end; redo re-applies the exact mask.
   - `adjust_open_entry_start_round_trips`: start-only adjust of an
     open entry undoes and stays open (no span validation fires).
   - `create_habit_round_trips`: CreateHabit ⇒ undo tombstones ⇒ redo
     re-mints at the appended order.
   - `update_habit_round_trips`: title+target+anchor-clear in one
     changeset ⇒ undo restores all three (the double-option clear
     round-trips) ⇒ redo re-applies.
   - `move_habit_round_trips`: undo restores the **pre order**
     (asserted against the pre-image, not the payload) ⇒ redo re-applies.
   - `adjust_habit_log_round_trips`: 1→5 on an existing row ⇒ undo
     restores count 1 (row kept, fresh clock) ⇒ redo sets 5.
   - Also: `log_time_undo_after_task_delete_still_deletes` (entry-only
     resolution) and `create_habit_undo_after_log_keeps_the_log`
     (the log map outlives the tombstone — D26 pinned).
3. **Dynamic refusals** (one per rule, §7): closing adjust ⇒ entry
   journals `NotUndoable(TrackingSurface)` and `Undo` refuses typed;
   absent-window adjust ⇒ `NotUndoable(HabitSurface)`; seam-addressed
   `UpdateHabit` (mixed with a title change, to pin the whole-edit
   refusal) ⇒ `NotUndoable(HabitSurface)`. Each: the head never moved.
4. **Watermark refusals** (per new stamp scope): created-habit edited
   since ⇒ `TargetNoLongerInvertible` (field-named); logged entry
   adjusted since ⇒ refuse (`UpdatedAt`); log window written since (a
   second adjust on the same window: first undo pops the newest, the
   second refuses — the v1 covered-field-edit echo).
5. **v1 pin updates (planned, not weakening — D30)**:
   `not_undoable_heads_refuse_and_name_the_reason` swaps its
   tracking/habit representatives to `StartTracking` (TrackingSurface)
   and `LogHabit` (HabitSurface) — LogTime/CreateHabit are undoable
   now, which is the chunk's purpose; the e2e
   `a_habit_log_journals_its_flush_trigger_but_stays_not_undoable`
   survives untouched (its head is `LogHabit`).
6. **e2e per arm** (`state/tests/undo.rs`, the `Rig` pattern; the
   dispatch acceptance's round-trip contract, asserted on published
   state + persisted journal agreement — fine-grained op/watermark
   assertions stay in the domain suite per AGENTS §5): six compact
   round-trips — `LogTime`, `AdjustTimeEntry`, `CreateHabit`,
   `UpdateHabit`, `MoveHabit`, `AdjustHabitLog` — plus one pushed-
   boundary case (a completed push flips a `LogTime` entry; its undo
   enqueues the delete op instead of retracting).
7. **Storage** (`journal_storage.rs`): the JSON-payload round-trip test
   gains the `HabitLogRow`/`HabitLog` shapes; corrupt-payload and
   hydration paths need nothing (shape-blind).
8. **Insta**: exactly **s36b, s45a, s45b** regenerate — the diff is
   `journal:` facts widening from `NotUndoable` to `PreImages` on
   tracking/habit entries (and the new `HabitLogRow` rows), reviewed
   hunk-by-hunk as "the widening, nothing else" (the undo/1 §16.3
   discipline, 3 pairs this time).
9. **proptest**: unchanged (D29).
10. **Completeness smoke** `every_arm_journals_entry_or_nothing`
    survives untouched (it asserts entry-or-nothing, which holds for
    dynamic facts too) — it is the canary that the widened
    classification kept the wrapper's batch contract.

## 12. Decision record (chunk-level; one per open question)

| # | Question | Decision |
|---|---|---|
| D21 | Scope | Exactly #555's six arms join; the exclusion list (`StartTracking`, `StopTracking`, `LogHabit` + the two delete verbs) stays `NotUndoable` with the restated reasons (§4); `DeleteTimeEntry`/`DeleteHabit`'s code-site reason families are left untouched (pre-existing doc/code divergence noted, not fixed) |
| D22 | How the `AdjustHabitLog` pre count reaches the journal | Widen the shapes, not the storage: `PreImage::HabitLogRow { key, before }` + `JournalEntity::HabitLog` + `EntityStamps::HabitLog(row)` + `UndoField::{HabitTarget, HabitAnchorTime, HabitLogCount}`, captured by one new match arm in each capture pass over `RecordHabitLog`. Serde-additive inside the existing JSON payload — no migration, no `PersistenceAction`, no apply-arm change (the dispatch's schema rule holds) |
| D23 | The closing `AdjustTimeEntry` | Record-time dynamic classification ⇒ `NotUndoable(TrackingSurface)` (§7.1): the mask cannot clear an end (ADR 0017 §4), so the inverse is inexpressible; refuse-and-name at record time; zero new error variants; the entry's coarse `updated_at` watermark doubles as the undo-time guard |
| D24 | The absent-window `AdjustHabitLog` | Record-time dynamic classification ⇒ `NotUndoable(HabitSurface)` (§7.2): inverting would forge an explicit count-0 row — the same stored-fact lossiness that excludes `LogHabit`; no log-row delete action exists and inventing one is out of bounds. `PreImage::HabitLogRow.before` is therefore always `Some` in journaled-undoable entries |
| D25 | `UpdateHabit` and the conversion seam | `changes.continued_from.is_some()` ⇒ `NotUndoable(HabitSurface)` (§7.3): no clock slot (D-H7a) ⇒ nothing to watermark against (R-U7) ⇒ a blind restore could destroy a later seam write. The undoable scope is the three clocked content fields; the diff never touches `continued_from` |
| D26 | `CreateHabit` occupancy | No guard, unlike `CreateStack`: `DeleteHabit` cascades nothing and never touches the log map, so post-create logs survive the undo by construction; a post-create seam link strands honestly with the tombstone (D-H7a data on a tombstoned row); a manual delete since makes the undo an accepted no-op repeat (the `DeleteTask` precedent) |
| D27 | Created-entity watermark scopes | `LogTime` ⇒ the minted entry's `updated_at`; `CreateHabit` ⇒ the four content clocks (the `deleted` clock excluded — matching the CreatedTask scope, which also excludes `deleted`). Consequence (accepted, v1-consistent): adjust-then-undo-then-undo-the-log refuses typed — the fresh restore stamp diverges from the recorded one; same mechanic v1 pinned for tasks |
| D28 | Redo semantics per arm | Same-verb arms redo exactly; `LogTime`/`CreateHabit` redo re-mints fresh ids (the v1 create-redo precedent); the round-trip tests assert structural equality modulo the fresh id, and "the old id stays retired" |
| D29 | proptest scope | The randomized field-line proptests stay task-scoped; the six arms are pinned by deterministic per-arm round-trips at both layers. Widening the strategies (habit entities, windows, entries) is #353-family v2 material — recorded, not scheduled |
| D30 | The v1 pins that name the widened arms | Updated deliberately, never weakened: the domain refusal-pins swap representatives to `StartTracking`/`LogHabit`; the e2e habit pin needs no edit; the snapshot regeneration is the 3-pair pass (§11.5/§11.8) |

## 13. Risks

1. **Pre-image fidelity is THE hazard** (the stage-3 trap): every
   inverse must read the *pre* image (journal entry), never the change
   mask's post values — an inverse built from post values passes its
   own round-trip and is still wrong. Mitigation: the round-trip tests
   assert the pre-state from the pre-image's own captured values, and
   `move_habit_round_trips` asserts the pre order explicitly.
2. **The minted id must be journaled, not re-derived**: `LogTime`'s
   inverse reads `PreImage::TimeEntry { id, before: None }`; there is
   no "the id LogTime minted" lookup anywhere else. Pinned by the
   capture test (§11.1) and the round-trip.
3. **Dynamic classification drift**: the record-time rules inspect the
   plan's upsert payload; if a future planner change alters the
   payload shape the rules must follow. Bounded: the rules live in one
   function next to the classification table, and the dynamic-refusal
   tests (§11.3) fail loudly on drift.
4. **Snapshot churn beyond the three pairs**: any scenario whose line
   carries one of the six arms widens; the corpus scan names exactly
   s36b/s45a/s45b today — if the suite grows before stage 2, the
   regeneration stays one audited pass.
5. **Reason-family cosmetic divergence** (`DeleteTimeEntry`/
   `DeleteHabit` journaled under `TrackingSurface`/`HabitSurface`
   while the variant docs say restore-class): deliberately untouched —
   re-classifying would change typed refusal payloads surfaces may
   match on, for no functional gain; noted for the #356 family.
6. **Concurrent round-2 chunks**: 1.9 is domain/state-exclusive
   (dispatch §1 lane note); the stage-3 rebase dance absorbs
   Cargo.lock/CHANGELOG adjacency.

## 14. Suggested commit spine (stage 2; every commit standalone green)

1. `feat(domain)`: the shape widening (D22) + the capture arms + the
   dynamic classification + serde/capture tests.
2. `feat(domain)`: the six synthesis rows + `InverseClass`/`command_class`
   widening + the domain round-trip/refusal suite + the v1 pin updates
   (D30) + the 3-pair snapshot regeneration.
3. `test(state,storage)`: the e2e per-arm round-trips + pushed-boundary
   case + the storage codec extension.
4. `docs`: ADR 0019 dated amendment + CHANGELOG entry.

Gates: the full AGENTS §10 suite via `scripts/run-quality-gates.sh`
from the worktree. Handoff: stage 3 per the dispatch protocol. No
push, no PR, no issue edits before stage 3.

## 15. Estimated diff size

~+900/−80 lines across ~12 files: `journal.rs` ~+30; `undo.rs`
~+250 (classification ~60, capture ~20, synthesis ~170); domain tests
~+450 (incl. pin updates); state e2e ~+120; storage ~+30; snapshots 3
pairs (small hunks); CHANGELOG/ADR ~+12. Production code ≈ 280 lines.

## 16. References

- Issue #555 (the arm list; the exclusion reasons, verbatim).
- `.artifacts/plans/2026-10-30-undo-journal-plan.md` §3 (rows 33–43),
  §4 (shapes), §5–§7 (wrapper/watermark/line mechanics), D7/D17/D19/D20.
- ADR 0019 (decisions 1–6; §6 the classification lockstep), ADR 0017
  (decisions 1–4: open entries, single-global intent, history editing).
- Code anchors (base `e8a919cd`): `messages.rs:306-415` (the arms),
  `:516-541` (`HabitChanges`); `command.rs:2061-2406` (the planner
  arms), `:70-183` (`CommandError` incl. the undo refusals);
  `undo.rs:56-185` (wrapper + classification), `:261-359` (capture),
  `:406-518` (`plan_undo`/`plan_redo`), `:525-612` (classes),
  `:621-1029` (synthesis); `journal.rs:96-217` (the widened enums),
  `:303-352` (labels); `habit.rs:148-298` (clocks, entity, log map),
  `tracking.rs:37-66`; `persistence.rs:244-260` (the action arms),
  `:385-390` (apply).
- Tests: `undo/tests.rs` (the `Harness`, the v1 round-trip/refusal
  patterns, the smoke), `state/tests/undo.rs` (the `Rig`, the habit
  pin), `journal/tests.rs:234` (serde), `journal_storage.rs:196`
  (codec), snapshots `s36b/s45a/s45b`.
