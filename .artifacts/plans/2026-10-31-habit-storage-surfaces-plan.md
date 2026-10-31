# [Plan] Habit trackers — storage, sync, surfaces (DR-6/6, #503) — stage-1 TDD implementation plan

- **Date**: 2026-10-31
- **Kind**: stage-1 chunk plan for dispatch chunk **DR-6/6** (the
  six-rounds batch dispatch,
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §13.1). TDD-first; no production code in this commit.
- **Branch / worktree**: `feat/habit-storage-surfaces` in
  `../taskboard-WT-feat-habit-storage-surfaces` (off `origin/main` @
  `4f8543e8`).
- **Preconditions verified (2026-10-31)**: DR-6/5 (#502, habit domain &
  engine — `2a1223ee`, `a88d8b75`, `02768ba3`, `e2fcda75`, `9f30e46f`,
  `8b80895a`) and DR-6/3 (#500, series storage & sync — `e66e71ec`,
  `6a1a58d3`, `96571de2`) are both in `origin/main`. The storage/fold
  lane is single-flight (no other chunk in flight).
- **Normative origins**: DR-6 design round
  `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §4.4 (storage DDL), §4.5 (sections, union, retention), §4.6 (today
  view + CLI surface), §4.7 (the conversion seam's read-side union —
  assigned to this chunk by `habit.rs`'s `continued_from` doc), §1
  fact 8 (`prune_absent_cards` is card-keyed) and fact 14 (mirror
  DR-5's retention ruling), §6 rulings D-C9/D-C13; dispatch plan §2
  (resource ladder + "next free at rebase" rule) and §13.1 (covers,
  acceptance, stage-2/3 prompts).

---

## 1. Mission & binding constraints

Land the second half of the habit tracker round: **persistence**
(migration + hydration), **sidecar roaming** (the board-level `habits`
and `habit-log` sections with embedded-clock union, tombstones,
verbatim relay, 180-day wire retention, and the board-level flush
trigger), the **CLI `habits` family**, and the **today-view Habits
section**. The domain/engine half already landed (DR-6/5): every
`StateCommand`, receipt, planner arm, and pure projection this chunk
needs exists — this chunk adds **zero `StateCommand` arms**.

Binding constraints (violating any is a review-blocking finding):

- **Zero Deck cards for habits** (D-C9): no habit row may enter any
  card push/pull path. Habit data rides the sidecar document only; the
  only outbox op this chunk introduces is a *board-level document
  flush trigger*, never a card op.
- **The additive-section recipe**: tolerant serde (one malformed row
  demotes to verbatim, never fails the document), verbatim relay for
  unknown shapes, tombstones competing by embedded clocks, union
  per-key LWW (§4.5; the `time-entries` section is the freshest
  precedent).
- **Retention mirrors DR-5's asymmetry exactly** (fact 14, D-T13):
  wire GC yes, local deletes no. The local database never drops
  history; the *document* is bounded; an aged row is never re-emitted
  and the union cannot resurrect it (overlay filter and lane GC share
  one constant).
- **`prune_absent_cards` gains no arm** (fact 8): board-level keys are
  invisible to it — habit history outlives everything.
- **D-C13 ordering**: the today section renders in stored manual
  `order`; `anchor_time` is a display chip only and appears in no
  ordering path anywhere (the DR-6/5 negative property extends to the
  new presentation layer).
- **days.rs is the only local-date vocabulary** (round binding): the
  retention boundary and the today section's window read through
  `DayCalendar`/`NaiveDate`, never a second timezone concept.
- A1 discipline: no new numeric types are minted here (`NonZeroCount`,
  `WindowStart` exist); hand-rolled types only, no derive crates.
- House test rules (AGENTS §5): no sleeps, no text assertions, no
  quality ceilings, one behavior one test at the lowest layer,
  `tokio::time::pause` for any clock work, typed outcomes only.
- SPDX headers: `MIT OR Apache-2.0` for domain/state/storage/sync
  files, `AGPL-3.0-only` for app/UI files; `#![forbid(unsafe_code)]`
  everywhere except `taskboard-ui-slint`.
- Gates (AGENTS §10) run through `scripts/run-quality-gates.sh`; the
  PR hands off green.

## 2. Verified seam facts (do not re-derive; verified 2026-10-31 against `4f8543e8`)

1. **Domain is done** (`crates/taskboard-domain/src/habit.rs`):
   `Habit`, `HabitState { habits, log }`, `HabitClocks { title,
   target, order, anchor_time, deleted }`, `HabitTarget`
   (`PerDay/PerWeek/PerMonth(NonZeroCount)`), `WindowStart` +
   `period_start/after/before`, `append_order_after`, `log_window`
   (attribution at log time, surplus-repairs), `streak` (walk with
   ≤ 2 pending skips). `HabitLogKey` serializes as the string
   `"<habit uuid>|<YYYY-MM-DD>"` (`wire_form`/`parse_wire_form`,
   strict). D-H12 leniency: non-canonical/former-kind rows decode and
   are analytically inert.
2. **Commands exist** (`messages.rs:357-419`): `CreateHabit`,
   `UpdateHabit(HabitChanges)`, `MoveHabit { habit, order: i64 }` (the
   UI computes midpoint orders — the planner stores the absolute
   value, `command.rs:2285`), `LogHabit`, `AdjustHabitLog` (typed
   `InvalidHabitWindow` on a non-canonical window), `DeleteHabit` (the
   log is never touched). `HabitChanges` (`messages.rs:514`) has
   `title/target/anchor_time/continued_from` — **no `order`**: order
   edits route `MoveHabit` only. Receipts: `HabitCreated`,
   `HabitLogged { habit, window, count }`, …
3. **Persistence actions exist** (`persistence.rs:250-256`):
   `UpsertHabit(Habit)`, `RecordHabitLog(HabitLogKey, HabitLogEntry)`;
   `apply_actions` is LWW (a strictly-older log row does not
   overwrite; equal/older clock keeps local — tests at 1213+).
4. **Storage write stubs exist and fix the concrete schema**
   (`repo.rs:1407-1466`, runtime-checked `sqlx::query` because the
   tables do not exist yet): `habits(habit_id, board_id, title,
   target, sort_order, anchor_time, ck_title, ck_target, ck_order,
   ck_anchor_time, ck_deleted, deleted, continued_from)` — `target`
   is **serde_json TEXT of `HabitTarget`** (one column, not the
   design's `target_kind`/`target_n` pair), stamps are rfc3339 TEXT,
   ids hyphenated-UUID TEXT; `habit_log(habit_id, window_start,
   count, clock)` with PK `(habit_id, window_start)` and the LWW guard
   `WHERE excluded.clock > habit_log.clock`. The load side stubs
   `HabitState::default()` (`repo.rs:967`) with a comment that already
   names **0017** as this chunk's migration.
5. **Migration ladder**: `0015_time_tracking.sql` (DR-5/2),
   `0016_command_journal.sql` (undo/1 took 0016 at its landing —
   dispatch §2 assigned it to DR-6/6, but the ladder rule is "next
   free at rebase"; `merge.rs`'s plan §4 recorded the same
   renumber-forward expectation). **0017 is next free**; no remote
   branch claims it. `connect.rs` pins `applied == 16`
   (`reopening_applies_no_new_migrations`); 0016 has an upgrade-pin
   test to mirror (`4f8543e8`).
6. **Appdata document** (`appdata.rs`): `AppDataSections` registry
   (§67-95) — `time-entries` is the board-level precedent: keys are
   not card ids, so `prune_absent_cards` (§940-967, touches only
   `tasks`/`reminder_rules`/`dismissals`) can never see them (fact 8).
   The envelope is the strict boundary; everything else demotes to
   verbatim, never fails.
7. **Union machinery** (`appdata.rs:742-933`): `union()` is per-section
   dispatch; `union_time_entries` is the freshest flavor — whole-row
   LWW, `bc > oc` keeps base (**ties resolve to the overlay** — the
   D-T11 flush-side ruling), `Verbatim` is never ordered against and
   is additive from base. (`union_rules_entry`/`union_dismissals_entry`
   use `bc >= oc` — base wins ties; the time-entries flavor supersedes
   for new sections.)
8. **DR-5 retention precedent** (`appdata.rs:1147-1241`):
   `TRACKING_SYNC_RETENTION_DAYS: i64 = 180` (named constant, never a
   config knob), one private window-start helper shared by the
   emission filter (`time_entries_for`: aged rows never re-emitted)
   and the post-union strip (`lane_gc_old_time_entries`: `start` for
   live rows, `clock` for tombstones, `Verbatim` passes, emptied
   section → `None`); **wire-only** — nothing deletes local rows;
   `derive_time_entry_tombstones` exists because time entries *can* be
   deleted locally (D-T12's fail-closed conjunction).
9. **Flush chain** (`sync-nextcloud/src/flush.rs`): per-card overlay
   loop → board-level time-entries overlay + tombstone derivation
   (§230-278) → `union(&base, &overlay)` → `lane_gc_scheduled` →
   `lane_gc_old_time_entries` → `prune_absent_cards` → upload.
   `FlushCtx` (§66-91) carries `tasks/stacks/rules/dismissals/
   entries`; constructed at `actor.rs:655`.
10. **Flush triggers** (`flush.rs:136` `has_flush_work`): a deferred
    document group exists, OR (rebuild evidence ∧
    `board_has_local_app_data` — the D12 lazy gate at §97, which
    today knows tasks/rules/entries only). `plan_pushes`
    (`push.rs:363`) buckets `LocalOp::UpdateAppData(task)` into
    `document_ops` by resolving task→stack→board, then
    `plan_document_groups` coalesces per board into ONE
    `PushTarget::Document(BoardId)` group, ranked strictly last;
    document groups are stashed by the actor (`actor.rs:515-521`) and
    executed post-pull as the flush.
11. **Board scoping of outbox ops**: `LocalOp::owning_board(&tables)`
    (`outbox.rs:121+`) resolves per variant; the actor filters
    `board_ops` by it (`actor.rs:482-487`). The codec's
    `op_to_columns`/decode round-trips ops through the outbox table
    (task_id nullable, board_id present since migration 0003).
12. **Fold call site** (`pipeline.rs` `apply_sync_report` §5b,
    `1007-1027`): the board-level time-entries fold runs once per
    document after the per-card loop — copy-on-write
    `folded_tracking`, `merge::fold_time_entries` (parse key → adopt
    when strictly newer / delete on newer tombstone → `changes`
    struct → `PersistenceAction`s), pre-R3 card map so history
    outlives a same-cycle card deletion. §5c (series dedup) runs
    after. Both reports currently clone `habits` through untouched
    (`1397-1400`, `1610-1613`) and a pin test asserts "no lane in
    either cycle yet" (`2706`).
13. **Habit writes enqueue nothing today** (`command.rs:2236`): "No
    outbox op (D-H9): the board-level sidecar flush vocabulary is
    chunk 6's." Every other document producer (task edits, rules,
    dismissals, time entries) triggers the flush through an
    `UpdateAppData` op keyed by a task — habits have no task, so this
    chunk must add the task-less trigger.
14. **CLI recipe** (`cli.rs:892` `TrackCmd` +
    `commands/track.rs`): one-shot wrappers over
    `super::tasks::with_core`, `*_on` seams on a booted core, one
    `collapse()` site (`track.rs:592`), typed receipts in
    `commands/output.rs` read back from the published snapshot (never
    input echoes). `resolve_time_entry` (`track.rs:494`) is the
    selector precedent: exact uuid → unique ≥8-char prefix (a
    collision names both candidates, typed) → typed miss.
    `route_for_ipc` (`commands/mod.rs:1980`) routes `Track`/`Export`
    onto the mutation lane (a one-shot local boot would write a
    *different* database's data when a daemon serves the db).
15. **UI recipe**: the timers section (DR-3/3) is the today-view
    composition precedent — `ui/compositions/timers.slint`, view
    props + callbacks (`today-view.slint:56-66`), adapter seam
    (`adapter.rs:4646` `install_timer_seams`) resolving
    `IdTables.timers` (`models.rs:81`) by global index →
    `ActionId::TimerCancel` → `router.rs:406`
    `DispatchOutcome::Command(StateCommand::…)`. The today model is
    pushed by `push_today` (`models.rs:1431`) which also fills the
    id tables; stale indices degrade to no-ops. Kiosk gates empty the
    ACTION, never the model (`presentation.rs:560-565`, D-T16); the
    timers' mount gate is model-side (`timers-visible` false on
    kiosk).
16. **Undo absorbs ops generically** (`undo.rs:175`): journal facts
    record `EnqueueOp` ids; the pushed-boundary/redo machinery is
    op-id-generic. The habit commands are already in the
    inverse-synthesis catalogue (undo/1 landed against the final
    command surface; this chunk adds no arms).
17. **Recur arithmetic** (`recur.rs`): `occurs_on` (789), `next_after`
    (804), `roll_target` (985), `schedule(rule, from, to)` (1005) —
    the §4.7 read-side union needs no new schedule arithmetic.
    `roll_reference_date(task, day)` (695) is the house way to name a
    done occurrence's member date.

## 3. The design, decision by decision

### R-1 — Migration 0017 (renumber deviation recorded)

Dispatch §2 assigned "0016" to this chunk; undo/1 took 0016 at its
landing. Per dispatch §1 lane 3 ("the chunk takes the next free number
at its rebase and records the deviation") this chunk ships
**`0017_habit_trackers.sql`** — `repo.rs:963`'s comment already
anticipates exactly this. The issue body's number correction (0016 →
0017) is stage 3's bookkeeping (dispatch §15 annotations pattern).

DDL — **exactly the schema the landed write stubs already speak**
(fact 4); the design round's §4.4 sketch (`target_kind`/`target_n`,
`created_at`/`updated_at`) is superseded by the stub's concrete
columns (`target` as one serde_json TEXT column, per-field `ck_*`
clocks carrying the stamps, `continued_from`):

```sql
-- 0017_habit_trackers: habit trackers and their count log (DR-6/6;
-- design §4.4/§4.5). Additive only (ADR 0005). Conventions as in
-- 0015: hyphenated-UUID TEXT ids, RFC 3339 TEXT stamps, no foreign
-- keys — the log outlives its tracker by design (DeleteHabit never
-- touches the log), so a habit_log row is data, not a join.
CREATE TABLE habits (
    habit_id       TEXT PRIMARY KEY NOT NULL,
    board_id       TEXT NOT NULL,
    title          TEXT NOT NULL,
    target         TEXT NOT NULL,      -- serde_json HabitTarget
    sort_order     INTEGER NOT NULL,
    anchor_time    TEXT,               -- "HH:MM:SS", NULL = no chip
    ck_title       TEXT NOT NULL,
    ck_target      TEXT NOT NULL,
    ck_order       TEXT NOT NULL,
    ck_anchor_time TEXT NOT NULL,
    ck_deleted     TEXT NOT NULL,
    deleted        INTEGER NOT NULL,   -- 0/1 tombstone
    continued_from TEXT                -- UUID, NULL = unlinked
);
CREATE TABLE habit_log (
    habit_id     TEXT NOT NULL,
    window_start TEXT NOT NULL,        -- canonical period-start "YYYY-MM-DD"
    count        INTEGER NOT NULL,
    clock        TEXT NOT NULL,        -- the row's LWW clock
    PRIMARY KEY (habit_id, window_start)
);
```

`connect.rs`: bump the `applied` pin 16 → 17; add the upgrade pin
`migration_0017_upgrades_a_pre_habit_database` (the 0016 test's
shape): run migrations, `DROP TABLE habit_log`, `DROP TABLE habits`,
delete the `_sqlx_migrations` row, reopen — load yields the default
empty `HabitState`, then an applied `UpsertHabit` +
`RecordHabitLog` round-trips through a reopen.

### R-2 — Storage load side (hydrate what the stubs could not)

- Two hydration queries in `load` (the time-entries block's shape,
  ordered before `PersistedState` assembly), decoding through
  `codec::` helpers: `habit_log.window_start` parses strictly
  (`NaiveDate::parse_from_str` → `WindowStart` — a malformed date is
  `RepositoryError::Corrupted`: an undecodable *key* is corruption,
  the `HabitLogKey::parse_wire_form` posture); `habit.target`
  decodes through `serde_json` with `InvalidHabitTarget` →
  `Corrupted` (the mint's strict decode is load-bearing — a stored 0
  cannot exist); `anchor_time` `None`-able; `continued_from` UUID.
  Replace the `HabitState::default()` stub at `repo.rs:967`.
- Convert the two runtime-checked write stubs (`repo.rs:1407-1466`)
  to `query!`/`query_as!` macros — the reason for runtime checking
  ("the tables arrive with chunk DR-6/6's migration") evaporates with
  0017 in the tree.
- Run `scripts/sqlx-prepare.sh` (sqlx-cli 0.9) and commit the
  refreshed `.sqlx/` **together with** the change (CI runs
  `--check`; DR-5/2 shipped its own 0015 with macros exactly this
  way). Fallback if sqlx-cli is unavailable in the environment: keep
  runtime-checked queries with the journal's comment shape and record
  the deviation in the stage-2 report.
- Storage round-trip tests (the `96571de2` shape): apply
  `UpsertHabit` + `RecordHabitLog` → `load` returns the entity
  faithfully (clocks, target, anchor chip, `continued_from`,
  tombstone); the log LWW guard is exercised at the SQL layer (an
  older-clock `RecordHabitLog` batch leaves the newer row).

### R-3 — The two sidecar sections (row enums + registry)

`AppDataSections` gains two board-level sections (the `time-entries`
registration shape — serde rename, `default`, skip-if-none; *not* card
-keyed, hence invisible to `prune_absent_cards` by construction, fact
8):

```rust
/// Board-level habit tracker definitions, keyed by habit UUID (DR-6
/// §4.5) — the RuleEntry row pattern.
#[serde(rename = "habits", default, skip_serializing_if = "Option::is_none")]
pub habits: Option<BTreeMap<String, HabitRecord>>,
/// Board-level habit count facts, keyed by [`HabitLogKey::wire_form`]
/// (§4.5); 180-day wire retention (D-H14).
#[serde(rename = "habit-log", default, skip_serializing_if = "Option::is_none")]
pub habit_log: Option<BTreeMap<String, HabitLogRecord>>,
```

```rust
pub enum HabitRecord {
    Live {
        title: String,
        target: HabitTarget,             // strict decode (the mint)
        order: i64,
        anchor_time: Option<chrono::NaiveTime>,
        clocks: HabitClocks,             // all five embedded clocks
        continued_from: Option<SeriesId>,
    },
    Tombstone { clock: DateTime<Utc> },
    Verbatim(Value),
}
pub enum HabitLogRecord {
    Entry { count: u32, clock: DateTime<Utc> },
    Verbatim(Value),
}
```

Shape rulings:

- **Custom serde like `RuleEntry`**: a malformed `Live` body
  (non-decoding `target`, wrong shapes) demotes the row to `Verbatim`
  — one bad row never fails the document. `Tombstone` is
  `{"deleted": true, "clock": …}` (the `RuleEntry` wire shape).
- **No board on the wire** (D-11): the document belongs to one remote
  board; the fold stamps the locally-bound board. `habit.board` is a
  local identity, like time entries' resolved task.
- **No `Tombstone` arm on `HabitLogRecord`** (D-5): no command deletes
  a log row (`DeleteHabit` keeps the log — fact 2), so nothing can
  emit one. A future delete shape decodes as `Verbatim` and relays —
  the documented leniency boundary, never a decode failure.
- **No tombstone *derivation* for either section** (D-7): DR-5 derives
  tombstones because a deleted time entry leaves no local row; a
  deleted habit leaves a tombstoned row the overlay emits directly,
  and log rows are never deleted. The D-T12 machinery has no
  counterpart here — its absence is pinned by the "delete roams"
  journey test, not by a silent gap.

### R-4 — Union (per-key whole-row LWW, ties → overlay)

`union()` gains two arms (the `union_time_entries` flavor verbatim,
fact 7 — including **ties resolve to the overlay**, the D-T11
flush-side ruling; the rules/dismissals `bc >= oc` flavor is not
followed):

- `union_habits(base, overlay)`: per habit-UUID key; competition
  clock = the row's **effective clock** — `max` of the five embedded
  clocks for `Live`, the tombstone's `clock` for `Tombstone`;
  `base_clock > overlay_clock` keeps base, else overlay wins;
  `Verbatim` on either side keeps the slot untouched (never ordered
  against), unknown keys extend additively.
- `union_habit_log(base, overlay)`: per `wire_form` key; whole-row
  LWW on `clock` (`bc > oc` keeps base); `Verbatim` never displaced.
  Set-union semantics in practice: one write site per key per device
  (the planner's fresh `now`), LWW degenerating to union exactly as
  the dismissals doc argues.

Concurrent different-field edits on one habit (rename here, reorder
there) are **whole-row LWW: one edit wins** — the accepted RuleEntry
semantics the design round chose for §4.5; the per-field
`HabitClocks` still ride the row (they are the row) and feed the
effective clock. Recorded as accepted semantics, pinned by the race
properties (§5 P-2), revisited only on evidence.

### R-5 — The board-level flush trigger (the D-H9 vocabulary)

New outbox variant, the task-less document trigger:

```rust
/// Flush one board's sidecar document (habit writes have no task to
/// key an `UpdateAppData` op — the board-level lane's trigger). The
/// flush uploads the whole merged document, so a stale or duplicated
/// trigger is harmless bookkeeping, never a delta.
FlushBoardAppData(BoardId),
```

- `owning_board(&tables)` → `Some(*board)` directly (no entity-graph
  resolution — simpler than `UpdateAppData`).
- `plan_pushes`: buckets straight into `document_ops` (its board is
  carried on the op); `plan_document_groups` coalesces it with any
  `UpdateAppData` ops of the same board into the ONE flush group.
  The no-wildcard rule: every existing exhaustive `LocalOp` match
  gains the arm (`outbox.rs`, `push.rs`, the sync crate's member
  resolution, the storage codec's `op_to_columns`/decode — kind text
  + `board_id` set, `task_id` NULL).
- Habit planner arms: all six (`Create/Update/Move/Log/Adjust/Delete`)
  append one shared-helper enqueue (`enqueue_board_flush(board, ids,
  now)`, the touched-tasks helper's shape, `command.rs:500`) to their
  action lists. No new `StateCommand` arms; the undo journal absorbs
  the op id generically (fact 16) — the undo suite runs against
  habit commands as a verification item (§5 T-12), not new
  machinery.
- Why not the alternatives (recorded): a cadence rebuild would need a
  "document dirty" watermark or burn bandwidth against the lazy
  D-S1/D-S8 posture; piggybacking on a task op is impossible (habits
  have no task) and a fabricated task id is exactly what the D-T10
  fail-closed filters guard against.

### R-6 — Flush wiring (overlay builders + lane GC)

`FlushCtx` gains `habits: &'a HabitState`, `board: BoardId` (the
local board whose document this is — the actor's loop variable) and
`today: NaiveDate` (device-local, resolved from the same `DayCalendar`
the cycle already passes to `apply_sync_report` — no second local-date
vocabulary). In `run_flush`, beside the time-entries block:

```rust
let mut overlay_habits = habits_for(ctx.habits, ctx.board);
let mut overlay_log = habit_log_for(ctx.habits, ctx.today);
```

- `habits_for(habits, board)`: emits **every** habit of that board,
  live rows as `Live` and tombstoned rows as `Tombstone { clock:
  clocks.deleted }`, keyed by UUID. Definitions carry **no
  retention** (D-10) — the log is the volume; tombstone rows are tiny
  and must roam forever so late-adopting devices learn the deletion.
- `habit_log_for(habits, today)`: the fail-closed emission filter,
  the D-T10 shape — a log row emits only if (a) its habit exists in
  the local map (an orphaned fact never emits; ClearBoardContent
  fallout included), (b) that habit's `board == board` (other
  boards' facts are not this document's), and (c) the window is
  **in-retention**: `window.as_date() >= cutoff(today)`. Aged rows
  are never re-emitted (D-14's agreement property).
- After the union: `let merged = lane_gc_old_habit_log(&merged,
  ctx.today);` — strips `Entry` rows with `window < cutoff`;
  `Verbatim` passes (never ordered against — we cannot date it);
  an emptied section becomes `None`. **Wire-only**: nothing here or
  anywhere else deletes a local row.
- `board_has_local_app_data` (the D12 lazy gate) gains the habits
  clauses — a board whose users touch *only* habits must still grow
  the anchor: `|| habits.habits.values().any(|h| h.is_live() && h.board == board)`
  `|| habits.log.keys().any(in-window row of a board habit)`. The
  retention window applies here too (a long-empty board's aged log
  does not hold the anchor up on its own).
- Order in the chain: the habits/log arms join the overlay document's
  sections; `lane_gc_old_habit_log` runs beside
  `lane_gc_old_time_entries` (before `prune_absent_cards`, which
  cannot see these sections anyway — pinned, R-9).

### R-7 — Retention (the 180-day constant, the local-date boundary)

```rust
/// The habit-log wire retention in days (design §4.5, mirroring
/// DR-5's D-T13 ruling): a named constant, never a config knob.
/// The overlay builder and the lane GC filter by this one window so
/// an aged row is never re-emitted and the union cannot resurrect
/// it. The local database never drops history.
pub const HABIT_LOG_SYNC_RETENTION_DAYS: i64 = 180;
```

- Boundary arithmetic in the **local-date vocabulary** (D-8):
  `cutoff(today) = today.checked_sub_days(180)` (saturating at
  `NaiveDate::MIN`, the `period_before` leniency), comparison
  `window.as_date() >= cutoff`. The window start is the row's
  *semantic* instant — the D-T13 shape (time entries age on `start`,
  not on mutation clock): the document carries the recent ~180 days
  of counted periods. A fresh `AdjustHabitLog` on an ancient window
  re-enters the document only while the window itself is in-range —
  ancient corrections stay local truth, exactly DR-5's asymmetry.
- No UTC conversion anywhere: `WindowStart` is a local `NaiveDate`;
  `today` arrives as a `NaiveDate` (R-6). The day-window offset is
  applied once by the caller that resolves `today` from the
  `DayCalendar`.

### R-8 — The fold (pull half)

`merge.rs` gains the two fold functions (the `fold_time_entries`
shape — parse, resolve, adopt-when-newer, `changes` struct):

```rust
pub struct HabitFoldChanges { pub upserts: Vec<Habit> }
pub fn fold_habits(habits: &mut HabitState,
                   rows: &BTreeMap<String, HabitRecord>,
                   board: BoardId) -> HabitFoldChanges;
pub struct HabitLogFoldChanges { pub records: Vec<(HabitLogKey, HabitLogEntry)> }
pub fn fold_habit_log(habits: &mut HabitState,
                      rows: &BTreeMap<String, HabitLogRecord>) -> HabitLogFoldChanges;
```

`fold_habits` semantics:

- Key parses as a habit UUID, else skip (an unparseable key is not an
  identity).
- `Live` row: local exists → adopt **wholesale** (all fields and all
  five clocks) iff the row's effective clock (R-4) is strictly newer
  than local's; else local stands. Local missing → adopt as a new
  habit with `board` = the document's locally-bound board (the
  parameter; the fold stamps it, D-11). Emits `UpsertHabit`.
- `Tombstone { clock }`: only a **live local** habit with
  `clock > local.clocks.deleted` tombstones (set `deleted`, stamp
  `clocks.deleted`); an already-deleted local ignores it (no
  re-stamp); an **unknown habit ignores it entirely** — no shell
  materialization (D-6; a bare tombstone carries nothing to adopt).
- `Verbatim` → skip (the fold imports nothing it cannot order).

`fold_habit_log` semantics:

- Key parses via `HabitLogKey::parse_wire_form`, else skip.
- `Entry { count, clock }`: local row absent → adopt; local `clock`
  strictly older → adopt **wholesale** (a lower count with a newer
  clock is a legitimate audited correction from another device);
  equal/older → local stands. Emits `RecordHabitLog` (whose SQL and
  `apply_actions` layers are independently LWW — belt and braces).
- **No habit-existence check** (D-6): orphan facts import and are
  analytically inert (no projection reads keys of unknown habits),
  retention-bounded, and honest — "the log outlives the tracker"
  roams as facts, not as reconstructed entities.
- `Verbatim` → skip.

Pipeline call site — §5b's board-level block, immediately after the
time-entries fold (`pipeline.rs:1027`), same copy-on-write pattern:
`folded_habits.get_or_insert_with(|| current.habits.clone())`, both
sections fold against the same copy, actions push
`UpsertHabit`/`RecordHabitLog`; the folded state lands in the sync
report's result. The push path stays hands-off (habits cloned through
`apply_push_report` untouched — the local state is the overlay's
source; there is no push-side fold, mirroring time entries).

The pin test `the_sync_and_push_reports_carry_the_habits_section_
through` (`pipeline.rs:2706`) is **rewritten** (D-16): the sync half
becomes the new-semantics test (a document with habit rows folds them;
a document without folds nothing); the push half stays a pin under its
current name, reworded to name the permanent rule (a push cycle never
folds — it *is* the overlay's source) instead of the "yet" debt.

### R-9 — `prune_absent_cards` untouched (the tripwire pin)

No arm added. Pin structurally: a document whose `habits` and
`habit_log` sections carry rows, pruned against an **empty**
`present_cards`, loses nothing from the two board-level sections
(while its card-keyed sections empty) — so a future arm that starts
pruning board keys fails the pin loudly. This is fact 8's property,
stated for the new sections.

### R-10 — The CLI family (`taskboard habits`)

`cli.rs` gains `Habits { #[command(subcommand)] cmd: HabitsCmd }` +
`HabitsCmd`:

| Verb | Flags | Command dispatched | Notes |
|---|---|---|---|
| `add <title>` | `--daily [N]` / `--weekly [N]` / `--monthly [N]` (exactly one, default N = 1), `--board <sel>`, `--at <HH:MM[:SS]>` | `CreateHabit` | the target grammar mirrors the `tasks repeat` flag style (fact 10 of DR-6/4's plan); `--board` omits to the single live board (the capture precedent); zero-quota is unrepresentable (clap range + the mint) |
| `list` | `--board <sel>` | read-only projection | canonical `(order, id)` order — the D-C13 order, never clock-sorted; columns: title, target phrase (`3/day`), progress in the current window, streak, chip |
| `log <habit>` | — | `LogHabit` | the receipt names the window the increment landed in and the post-increment count (`HabitLogged`) — user-visible, never silent repair |
| `adjust <habit> --window <YYYY-MM-DD> --count <n>` | | `AdjustHabitLog` | a non-canonical window is the typed `invalid-habit-window` refusal passed through structurally |
| `update <habit>` | change-mask: `--title`, the target flags, `--at <t>` / `--at none` (clear), `--order <n>` | `UpdateHabit` / `MoveHabit` | `--order` routes `MoveHabit` (`HabitChanges` has no order — fact 2); a zero-flag update is the typed no-op; `--continued-from <series-sel>` rides `UpdateHabit` (the D-H7a write path) |
| `rm <habit>` | — | `DeleteHabit` | tombstone; the log survives (the receipt says so structurally: the row's history stays queryable) |

- `commands/habits.rs` (new): the track.rs recipe verbatim — one-shot
  wrappers over `with_core`, `*_on` seams, **one `collapse()` site**
  over a `HabitsInvocation` enum, receipts projected from the
  published snapshot (never input echoes). `resolve_habit` mirrors
  `resolve_time_entry`: exact uuid → unique ≥8-char prefix (collision
  names both candidates, typed `UnknownHabit`) → exact live title.
- `commands/output.rs`: `HabitAddReceipt`, `HabitListView`,
  `HabitLogReceipt { habit, window, count }`,
  `HabitAdjustReceipt`, `HabitUpdateReceipt`, `HabitDeleteReceipt` —
  typed, `--json`-enveloped like the track family (no export-style
  exception here).
- `route_for_ipc`: the whole family joins the `Track` mutation lane —
  a one-shot local boot would write a different database when a daemon
  serves the db (fact 14's D-TC1 argument). The `route_for_ipc` unit
  table gains the rows.
- `main.rs`: the dispatch arm (collapse → invoke), the same shape as
  track's.
- Tests: hermetic boots in tempdirs (the track suite's harness —
  `c2b2ea52`'s journey style), typed receipts asserted structurally
  (`matches!`), no text assertions.

### R-11 — The today-view Habits section

- **Composition** `ui/compositions/habits.slint` (the timers
  precedent): `HabitsSection` with `in property <[HabitRowModel]>
  habits`, callback `plus-clicked(int)`, callback `reorder-dragged(int, int)`
  (row, drop-slot). `HabitRowModel { title, chip, count: int, quota:
  int, streak: int, met: bool }` — the progress phrase ("water
  5/8") is a presentation-side derivation from `count`/`quota`, the
  streak decoration renders from `streak` (a flame-free count badge;
  no new theme tokens beyond the existing semantic set).
- **View** `today-view.slint`: the section sits with the timers
  block, above the task sections' empty state (D-P18's page-order
  logic: timers, then habits, then tasks) — habits are today-view
  items (§4.6), and the section renders whenever the active board has
  live habits (no collapse-when-empty *model*; the empty section just
  has zero rows — the timers mount-gate shape).
- **Presentation** (`presentation.rs`): `habit_rows(state, board,
  now, day) -> Vec<HabitRow>` — the board's **live** habits in
  canonical `(order, id)` order (D-C13; `append_order_after`'s
  vocabulary), `count` = the current window's log row
  (`WindowStart::of_period(target.kind(), day.today(now))`),
  `streak` = the domain `streak` (R-12's unioned variant when
  `continued_from` is set), `met = count >= quota`. The negative
  property extends here: perturbing `anchor_time` leaves the row
  order and every rendered field but the chip unchanged (DR-6/5's
  §5.1 item 4 at the presentation layer). Kiosk gate:
  `habit_editing_visible(kiosk) -> !kiosk` (the D-T16 shape — the
  gate empties the **actions**; the rows render on kiosk: passive
  progress visibility is the section's point, edits are not).
- **Adapter/models**: `IdTables.habits: Vec<HabitId>`;
  `push_today` (or a sibling pass it calls) fills the table and
  pushes the section model; `install_habit_seams` resolves
  `plus-clicked(i)` → `tables.habits[i]` (stale index → no-op, the
  notification-row guard shape) → `ActionId::HabitLog` → router →
  `StateCommand::LogHabit`.
- **Reorder**: a drag past a neighbor's midpoint previews and on
  release dispatches `ActionId::HabitMove` with the **absolute
  floor-midpoint order of the drop slot's free interval** (the
  `MoveTask` convention, `command.rs:551`; `MoveHabit` stores the
  absolute value — fact 2) computed from the row source's orders —
  the UI computes midpoints, the planner stamps. No board drag
  machinery is reused (different layout context); the row source is
  the same `habit_rows` order, so what you see is what reorders.
- **The +1 button never disables at/after quota** (D-14): surplus
  logs are first-class (the D-C8 surplus-repairs rule *feeds* on
  them); `met` only restyles the row.
- **Undo/menu/keymap**: nothing — no new global actions beyond the
  two `ActionId`s (section-scoped, like the timers' seams); the
  enabled-set exclusions and keymap presets are untouched.

### R-12 — The §4.7 read-side seam union (assigned to this chunk)

`habit.rs`'s `continued_from` doc assigns "the read-side history
union" to chunk 6 (§4.7: "streak/history views union both sides at
read time via the link — no fabricated done tasks, no collapsed
logs"). One pure function, the decoration's input:

```rust
/// The habit's streak continued through its conversion seam (§4.7):
/// the habit's own `streak` walk, then — when the walk breaks on a
/// genuinely missed period and `continued_from` names a series —
/// consecutive *completed* series members counted backward from
/// that break period.
pub fn habit_streak_unioned(habit: &Habit,
    log: &BTreeMap<HabitLogKey, HabitLogEntry>,
    tasks: &BTreeMap<TaskId, Task>, now: DateTime<Utc>,
    day: DayCalendar) -> u32;
```

Law: run the ordinary `streak` walk but capture its **break period**
`b` (the first period that is neither counting nor pending — the walk
already computes it; a saturation termination means no seam). While
`habit.continued_from == Some(series)` and the break exists: take the
series' rule from the **most recent done occurrence task** carrying
`recur.series == series` (the rule is identical down the chain);
enumerate `schedule(rule, …)` members backward from `b`'s date over a
bounded window (≤ 366 days per step is never needed — stop at the
first gap); a member counts iff a done task exists with that series
and `roll_reference_date(task, day) == member`. Stop at the first
uncounted member; sum with the habit's own walk.

Recorded simplification (D-15): **no grace crossing the seam** — the
habit side applies its own 24 h overflow within its log; the series
join uses strict member completion. Refinement is F-2's (the UX
follow-up), not this chunk's. Properties: `None`/no-break ⇒ equals
`streak` exactly; one extra completed member at the break extends by
exactly one; a gap at the break contributes zero; permutation/order
independence over the task map. The today view's decoration and
`habits list`'s streak column render the unioned value.

## 4. What does NOT change

- **No `StateCommand` arm, no receipt variant, no planner signature**
  — the DR-6/5 surface is final for this chunk; only the six arms'
  action lists gain the flush trigger (R-5).
- **No `ModeledValue` arm, no `tasks`-section decode change** —
  habit facts live in their own sections; the per-task lane is
  untouched.
- **No new migration consumer, no schema rewrite** — 0017 is two
  plain `CREATE TABLE`s; the stubs already speak it.
- **`prune_absent_cards`, `lane_gc_scheduled`,
  `lane_gc_old_time_entries`, `derive_time_entry_tombstones`** —
  untouched bodies; only the flush chain gains one call.
- **The engine's ticks** — nothing ticks; habits have no day-boundary
  arm (DR-6/5's "never-done is native").
- **The push path's fold** — `apply_push_report` keeps cloning
  habits through (D-16's kept half).
- **No ADR** — ADR 0018 was chunk 2's; this chunk carries none
  (dispatch §15's docs list has no entry for DR-6/6 beyond the plan
  itself).
- **Backlog issues** — stage 3 closes #503 (and #212 with the round's
  completion comment); stage 2 touches no issues.
- **Config** — no `[habits]` table; the retention constant is named
  code (D-T13's posture).

## 5. Test plan (TDD order)

Lowest layer first; one behavior, one test; typed outcomes only;
proptest strategies from `test_support` (`habit_fixture_strategy`,
`habit_strategy` exist from DR-6/5).

**Domain — appdata.rs**

- **T-1**: section serde — the two sections ride the envelope;
  exact modeled keys serialize (`{"habits": …}` / `{"habit-log": …}`);
  a malformed `Live` body demotes to `Verbatim` without failing the
  document; `HabitLogRecord` has no tombstone shape (a future
  `{"deleted":true}` log row decodes `Verbatim` — D-5 pinned).
- **T-2 (P-1 union)**: `union_habits`/`union_habit_log` — newer wins
  whole-row; tie → overlay (both sections); verbatim never displaced
  and additive from base; idempotence
  (`union(union(b,o),o) == union(b,o)`); permutation invariance over
  row insertion order.
- **T-3 (P-2 tombstone races)**: tombstone(newer) over live(older)
  → deleted survives; live(newer) over tombstone(older) → resurrected
  (the resurrection-safe property); effective-clock tie between a
  tombstone and a live row → overlay wins; the fold-side twin
  (strictly-newer tombstone only, unknown-habit tombstone ignored —
  covered with T-6's fixture or here, one home, not both).
- **T-4 (P-3 verbatim relay)**: unknown row shapes in both sections
  survive union + a full serde round-trip value-identically.
- **T-5 (P-4 retention agreement)**: proptest — for generated
  `(habits, log, today)`: `habit_log_for` emits exactly the
  in-window, board-matching, habit-present rows; after
  `union(base_with_aged, overlay)` + `lane_gc_old_habit_log` the
  document's log rows are exactly the in-window set (aged base rows
  stripped, never re-emitted); `Verbatim` rows always survive the GC;
  the local maps are untouched by every function in the chain (the
  asymmetry, pinned).
- **T-5b**: `habits_for` emits live **and** tombstoned definitions,
  no retention on definitions; foreign-board habits never emit.

**Domain — merge.rs**

- **T-6 (P-5 fold)**: `fold_habits` — adopt-when-strictly-newer
  (whole row, clocks included), local stands on equal/older, unknown
  habit adopts with the stamped board, tombstone semantics per R-8;
  `fold_habit_log` — adopt/stand per clock, lower-count-newer-clock
  adopts (audited correction roams), orphan facts import.
- **T-7 (P-5 permutation)**: the fold result is invariant under row
  order; two conflicting documents folded in either order converge
  to the same state (LWW totals).

**Domain — pipeline.rs**

- **T-8**: the §5b call site — a sync report whose document carries
  habit rows emits the `UpsertHabit`/`RecordHabitLog` actions and the
  result state carries the folded habits; a document without them is
  a no-op; the reworked pin (D-16): the sync half asserts the new
  semantics, the push half keeps "never touches habits".

**Domain — habit.rs / outbox.rs / push.rs / command.rs**

- **T-9 (P-6 zero card traffic)**: proptest over the six habit
  commands on generated states — every plan's actions contain no
  card `LocalOp` (create/update/move/delete card), only
  `FlushBoardAppData` + persistence actions; the D-C9 tripwire.
- **T-10**: `plan_pushes` — habit ops coalesce with `UpdateAppData`
  ops of the same board into ONE document group (primary/subsumed
  partition: every op id exactly once, the doc-lane property);
  `owning_board` resolves directly.
- **T-11**: `habit_streak_unioned` — the R-12 law: no link/no break ⇒
  equals `streak`; one completed member at the break extends by one;
  a gap contributes zero; determinism under task-map permutation.

**Storage**

- **T-12 (acceptance: migration upgrade test)**: R-1's upgrade pin +
  the `applied == 17` bump.
- **T-13**: repo round-trip (R-2): habit + log survive
  apply→load; LWW guard at the SQL layer; the undo suite re-run
  against `LogHabit`/`MoveHabit` with ops now enqueued (fact 16's
  verification) — undo retracts/keeps the pending flush op exactly
  as for task ops.

**Sync crate**

- **T-14 (acceptance: union convergence + journeys)**: flush-level —
  a flush over a base document with foreign + aged + fresh rows
  uploads the merged document whose habit sections are the converged
  set (the union/verbatim/retention properties at the chain level);
  a habit-only edit (no task ops) produces `has_flush_work == true`
  through the new group and grows the anchor on a habit-only board
  (`board_has_local_app_data`); the wiremock journey (the DR-6/3
  `96571de2` shape): a habit and its log roam to a second device via
  the sidecar alone — the wire fixture carries zero card bodies for
  habits.

**App**

- **T-15**: `collapse` totality + the `route_for_ipc` table rows.
- **T-16**: `resolve_habit` — exact uuid, unique prefix, collision
  names both (typed), exact live title, typed miss.
- **T-17**: hermetic verb journeys (tempdir boots): `add` receipt
  round-trips through `list`'s projection in stored order; `log`
  receipt names window + count and the snapshot's log row moved;
  `adjust` on a non-canonical window is the typed refusal; `update
  --order` routes `MoveHabit` (the clock slot `ck_order` moves,
  `ck_title` does not); `rm` keeps the log rows queryable.

**UI**

- **T-18 (acceptance: order + routing pinned)**: `habit_rows` —
  stored `(order, id)` order; the `anchor_time` perturbation
  negative property; count/quota/streak/met derive from the domain
  functions (no re-derivation in the UI); kiosk gate empties the
  actions, not the rows.
- **T-19**: adapter seam — `plus-clicked(valid)` routes
  `LogHabit` for `tables.habits[i]`; stale index no-ops;
  reorder dispatches `MoveHabit` with the midpoint order computed
  from the row source (equivalence with the CLI's `--order` path at
  the planner, one law, two surfaces).

**Acceptance map (dispatch §13.1 → tests)**: migration upgrade test
→ T-12; section union convergence + tombstone + verbatim properties
→ T-2/T-3/T-4/T-14; retention GC strips + overlay never re-emits →
T-5/T-14; `prune_absent_cards` untouched → R-9's pin (T-1's document
fixture asserts it); today-view stored order + `+1 → LogHabit`
routing → T-18/T-19; CLI hermetic with typed receipts → T-15–T-17;
gates green → the §10 suite through
`scripts/run-quality-gates.sh`.

## 6. Implementation order (stage-2 guidance)

Standalone green commits, each buildable and clippy-clean:

1. `feat(domain): the habit sections ride the sidecar document` —
   T-1…T-5b (rows, serde, unions, retention pair, overlay builders,
   const).
2. `feat(domain): the board flush trigger for task-less lanes` —
   T-9…T-10 (outbox variant, owning_board, plan_pushes bucketing,
   the six planner arms' enqueue, codec round-trip in storage's
   codec).
3. `feat(domain): the habit fold and its pipeline arm` — T-6…T-8
   (merge folds, §5b call sites, the reworked pin).
4. `feat(storage): migration 0017 hydrates the habit state` — T-12…
   T-13 (DDL, hydration, macro conversion, `scripts/sqlx-prepare.sh`,
   `.sqlx` committed together, upgrade pin, undo-suite re-run).
5. `feat(sync): the habits sections join the flush chain` — T-14
   (FlushCtx fields, overlay assembly, lane GC call, lazy-gate
   clauses, wiremock journey).
6. `feat(app): the habits CLI family` — T-15…T-17.
7. `feat(ui-slint): the today view's habits section` — T-18…T-19 +
   `habit_streak_unioned` if not landed with commit 1 (it belongs in
   1's crate; land it there).
8. `docs(changelog): the habit storage and surfaces round entry` —
   one Unreleased entry (D-17).

## 7. Decision record (every open question, resolved)

| ID | Question | Ruling |
|---|---|---|
| D-1 | Migration number | **0017**, not the dispatch table's 0016 — undo/1 took 0016 at its landing; dispatch §1 lane 3's renumber-at-rebase rule; `repo.rs:963` already anticipates it. Stage 3 corrects #503's body |
| D-2 | §4.4's DDL sketch vs the landed stubs | The stub schema wins: `target` one serde_json TEXT column (not `target_kind`/`target_n`), per-field `ck_*` stamps (no `created_at`/`updated_at`), plus `continued_from`/`deleted`. The stubs are the contract the code already speaks |
| D-3 | sqlx macros vs runtime-checked queries | Macros + `scripts/sqlx-prepare.sh`, `.sqlx` committed together (0015's self-shipped-migration precedent); convert the two write stubs too. Fallback: runtime-checked with the journal's comment, deviation recorded |
| D-4 | Row competition clock; tie rule | Effective clock = max of the five embedded clocks (Live) / the tombstone clock; ties resolve to the **overlay** — the `union_time_entries` (D-T11) flavor, superseding rules/dismissals' base-wins-ties for new sections |
| D-5 | Tombstone arm on `HabitLogRecord`? | **No** — no command deletes a log row; a future delete shape decodes `Verbatim` and relays (documented leniency), never a decode failure |
| D-6 | Fold policy for orphans | Orphan log facts **import** (inert per D-H12's projection law, retention-bounded, honest history); unknown-habit tombstones **ignore** (no shell materialization) |
| D-7 | Tombstone derivation (the D-T12 counterpart) | **None** — habit deletes leave state rows the overlay emits; log rows are never deleted. The delete-roams journey test pins the behavior the derivation would have provided |
| D-8 | Retention instant: row clock vs window date | The **window's local date** vs device-local `today − 180d` (the semantic instant, D-T13's `start`-based shape); pure `NaiveDate` arithmetic, no calendar round-trip in the flush |
| D-9 | The flush trigger for task-less writes | New `LocalOp::FlushBoardAppData(BoardId)` enqueued by all six habit arms via one helper; cadence-rebuild and task-piggyback rejected (lazy-gate posture; no task exists and fabricating one is the D-T10 hazard) |
| D-10 | Definitions retention | `habits` rows carry **no retention** (the log is the volume; deletions must roam forever); retention is `habit-log`-only, mirroring §4.5's text |
| D-11 | Board on the wire | Not carried — the document belongs to one remote board; the fold stamps the locally-bound board (the time-entries resolution shape, minus the card hop) |
| D-12 | CLI verb set | Six verbs `add/list/log/adjust/update/rm`; the design's five + `update` (issue #503's "+ order/anchor-time flags on add/update"); `--order` routes `MoveHabit` (`HabitChanges` has no order field); target grammar `--daily/--weekly/--monthly [N]` mirroring the repeat family |
| D-13 | Kiosk posture | Section **renders** on kiosk (passive progress visibility); the +1 and drag actions are gated (`habit_editing_visible(kiosk) -> !kiosk`) — the D-T16 shape: the gate empties the ACTION, never the model |
| D-14 | +1 at/after quota | The button **stays enabled** — surplus logs are first-class (D-C8's surplus-repairs feed on them); `met` restyles, never disables |
| D-15 | The §4.7 seam union's precision | `habit_streak_unioned`: the walk's break period seeds a backward member enumeration via `schedule()`; strict member completion joins (no cross-seam grace — F-2's refinement); the rule comes from the latest done occurrence of the series |
| D-16 | The "no lane yet" pin test | Rewritten: the sync half asserts the new fold semantics; the push half stays a pin, reworded from "yet" debt to the permanent rule (a push cycle is the overlay's source, never a folder) |
| D-17 | CHANGELOG | A one-line Unreleased entry rides the final commit (the round-entry precedent) |
| D-18 | Which habits the today section shows | The **active board's** live habits (the board-bar selection drives the today view on both postures; consistent with the kiosk task filter's board scoping) |

## 8. Risks

- **The flush-trigger edit touches the push lane's exhaustive
  matches** (outbox, push planner, sync member resolution, storage
  codec). Mitigation: the no-wildcard rule makes the compiler find
  every site; T-10 pins the grouping partition; the op is
  board-carried so no entity-graph resolution can strand it (and the
  orphan-ops path already tolerates unresolvable ops safely).
- **Undo interplay with the new op**: the journal machinery is
  op-id-generic (fact 16), but undo/1's suite never saw a habit op.
  Mitigation: T-13 re-runs the undo journeys against `LogHabit`/
  `MoveHabit`; a failure here is a production bug (AGENTS: a new
  test failing can be a bug) — fix on the branch, not a test
  weakening.
- **`cargo sqlx prepare` availability** (sqlx-cli 0.9 pinned).
  Mitigation: D-3's recorded fallback keeps the chunk unblocked
  without weakening CI.
- **Whole-row LWW loses concurrent different-field habit edits**
  (D-4): accepted design-round semantics, race pinned by T-3;
  revisited only on evidence (the §5.5 honesty pattern).
- **The seam union's member enumeration** could over-scan for sparse
  monthly patterns. Mitigation: the scan is bounded by the first gap
  and by the schedule window; a decoration-scale cost at human
  habit counts (the §5.5 "fine at human scale" posture).
- **Slint drag-reorder on the today view** has no house precedent
  (the board's drag lives in a different layout context).
  Mitigation: the timers-composition shape for structure, the row
  source as the single order truth; if the pinned slint version
  fights the pointer-drag gesture, the keyboard-pair fallback
  (move-up/move-down dispatching the same `MoveHabit` midpoint law)
  satisfies the acceptance ("reorder persists and survives sync
  round-trip") with the deviation recorded.
- **Wave soft lanes**: this wave is alone (dispatch §13) — cli.rs and
  ui-slint are uncontended; undo/2 (wave 11) touches
  router/actions/menu after this lands.

## 9. Estimated diff size

~2,300–2,600 lines including tests (implementation ~1,100–1,300):
domain ~850 (appdata ~450, merge ~180, pipeline ~60, habit ~120,
outbox/push/command ~65), storage ~215 + `.sqlx` artifacts, sync
~110, app ~815 (habits.rs ~550 new, cli ~140, output ~80, routing
~45), ui-slint ~430 (composition ~120 new, models ~70,
presentation ~120, adapter/actions/router ~90, view ~30),
CHANGELOG ~4. Size **M/M+**, matching the dispatch table.

## 10. References

- DR-6 design round (normative §4.4–4.6, §4.7, §1 facts 8/14, §6
  D-C1–D-C13): `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
- Dispatch plan §1/§2/§13.1/§15:
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
- DR-5 retention ruling (D-T13) and section precedent:
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`;
  the landed `time-entries` lane in `appdata.rs:1147-1241` and
  `flush.rs`
- The storage/fold lane's previous chunk:
  `.artifacts/plans/2026-10-29-series-storage-sync-plan.md`
- Issue: #503 (chunk), #212 (the round's origin gate); follow-ups
  #504 (F-1 tracker reminders), #506 (F-2 habit UX) stay fenced
