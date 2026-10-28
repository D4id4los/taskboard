# [Plan] Habit trackers — domain & engine (DR-6/5, #502) — stage-1 TDD implementation plan

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan for dispatch chunk **DR-6/5**
  (`feat/habit-domain-engine`, wave 7 of the six-rounds dispatch).
  Normative inputs: `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
  §4.1–§4.3 + §4.7 (decision by decision), §6 rulings D-C8/D-C9/D-C10/
  D-C13, §3.9 properties 5–6 (by analogy), §7 chunk 5; the dispatch plan
  §1/§10.1 (acceptance + code-quality mandate). Deviations from the
  origin plan are recorded decisions below, each with its reason.
- **Base**: `origin/main` @ `f67c9bc0` (precondition verified: DR-6/2
  merged as PR #534, merge commit `a61cdd17` — the command-surface lane
  order DR-5/1 → DR-6/2 → **DR-6/5** → undo/1 holds).
- **Status**: READY for stage 2.

## 1. Mission & binding constraints

Add the habit-tracker half of DR-6 as **pure domain + planner**:
`HabitState` (the third nested state home), the `Habit` entity with its
A1 mints, the log keyed `(habit, window)` (the `(reminder, occurrence)`
fact shape), six `StateCommand` verbs with typed receipts/errors, the
window-attribution and streak projections, and the `continued_from`
conversion seam's habit-side field. **Nothing ticks** — never-done is
native: no spawn, no roll, no done stamp, no `day_tick` involvement, no
`running`-style derived cache. A window's state *is* its log row.

Fences (dispatch §10.1 + §7 chunk 5):

- **No storage migration, no sync sections, no surfaces** — chunk 6's
  (`feat/habit-storage-surfaces`, mig 0016). The storage crate shows
  only the R-11-shaped dead arms (§3.8); the CLI/UI show no diff.
- **No reminder machinery** (D-C11): no `ReminderRef::Tracker`, no
  pings, no evaluator changes.
- **No ADR**: ADR 0018 was written by DR-6/2 and already covers the
  round's shape; this chunk records nothing new in `docs/adr/`.
- A1 discipline: hand-rolled private-field newtypes with fallible mints;
  no derive crates; deliberate leniency documented at the type (AGENTS
  §2, `coding_guidelines.org` §2.1).
- `days.rs` is the only "local date" vocabulary; all period instants
  derive through `DayCalendar`.
- No outbox ops are planned by this chunk's planner arms (§3.7, D-H9).
- The chunk is a **shape-widener (w)**: `AppState`/`PersistedState`
  gain the `habits` section — one deliberate, diff-audited insta
  regeneration pass (§5.5).

## 2. Verified seam facts (do not re-derive; verified 2026-10-28 against `f67c9bc0`)

1. **Nested-state pattern** (`state.rs`): `AppState` carries
   `reminders: ReminderState` and `tracking: TrackingState` between
   `labels` and `sync`; both are `BTreeMap`-based, `Default`, and
   serialize deterministically. `AppState` derives
   `Debug, Clone, Default, PartialEq, Serialize` (no `Deserialize` —
   hydration goes through `PersistedState`).
2. **The mirror** (`persistence.rs`): `PersistedState` mirrors the
   entity sections and `From<PersistedState> for AppState` maps them
   (`last_updated: None`). `apply_actions` is the executable batch
   contract shared by the engine's memory advance, the fake repo, and
   the sqlite repo.
3. **The fact shape** (`reminders.rs`): deliveries/dismissals are
   keyed `(reminder, occurrence)` with ≤1 row per key, enforced by
   `apply_actions` and pinned by property. The habit log applies the
   same shape with a count payload (D-H10).
4. **Planner catalogue style** (`command.rs`): `plan_command` is pure,
   total, clock/id-injected; returns `CommandPlan { actions, outcome }`;
   rejections are typed `CommandError` variants; *terminal* intents
   repeated are accepted `NoOp`s; *edit*-style intents against
   tombstones are errors. Resolve helpers `live_*`/`existing_*`.
5. **The tracking precedent** (DR-5/1, commits `2c41d92e`…`3e94f744`):
   entity+home+actions in one commit, verbs+receipts next, projections+
   strategies next, engine tier last, insta pass alongside. Receipts
   carry post-stamp rows (`TrackingStarted { entry, closed }`).
6. **R-11 storage deviation** (recorded in `2c41d92e`'s message): the
   sqlite `execute_batch` partition match over `PersistenceAction` is
   exhaustive by policy (no wildcard) — new actions force arms in the
   same commit, written as runtime-checked `sqlx::query` against the
   storage chunk's planned DDL, dead (no producer exercises them on the
   real repo) until the migration lands; `load_inner` keeps the new
   section at `Default` with a comment naming the hydrating chunk.
7. **Id families** (`ids.rs`, `idgen.rs`): `HabitId` and `SeriesId`
   already exist (DR-6/1) in the roaming family;
   `IdGenerator` gains one method per id kind — implementors:
   `UuidV7Generator`, `test_support::CountingIds`, and any bench/test
   doubles (compile-enforced lockstep; DR-5/1 also touched
   `taskboard-ui-slint/benches/support/mod.rs` for this).
8. **`RecurRule.continued_from: Option<HabitId>` exists** (DR-6/1,
   `recur.rs:389`, serde-tested `null` round-trip). This chunk adds the
   other direction only: `Habit.continued_from: Option<SeriesId>`.
9. **MoveTask midpoint machinery** (`command.rs:988`): identical
   `(stack, order)` → `NoOp`; the write stamps one composite
   `clocks.position`. The successor-spawn midpoint helper
   `insertion_order_after` (`command.rs:457`) documents itself as "the
   single source the later habit-drag work (D-C13) reuses" — but its
   stack-scoped signature (`t.stack == head.stack`) does not generalize
   to board-scoped habits; the habit analogue is written once in
   `habit.rs` (D-H11) with the same gap/midpoint law.
10. **`DayCalendar`** (`days.rs:93`): `today(now)` (the window-date,
    night-owl rule), `calendar_date(t)` (plain date), `instant(date,
    tod)`; free fn `window_start(date, day_start_time, offset)` gives a
    local date's window-opening instant. This is the period-boundary
    vocabulary (D-H4).
11. **Engine tier** (`taskboard-state/tests/`): `common/mod.rs` provides
    `FixedClock`, `seed_board_state`; `tracking.rs` is the harness
    template (spawn over `InMemoryRepository`, oneshot replies,
    poll-with-paused-time-deadline, crash-resume leg). The actor
    routes commands generically — no exhaustive `StateCommand` match
    exists outside the domain planner (`actor.rs:630` is an
    `if let`). The UI crate matches `StateCommand` only in
    non-exhaustive construction sites — no compile impact expected
    (gates verify).
12. **Insta corpus** (`state/tests/snapshots/`): `snapshots__sN_persisted_state.snap`
    (+ `_published_tail`); DR-5/1's pass added the empty `tracking`
    section to every persisted dump and new s36a/b scenarios for the
    populated shape. The same two-part pass repeats for `habits`.

## 3. Design

### 3.1 The entity, its mints, and the state home (D-H1–D-H3, D-H12)

New module `crates/taskboard-domain/src/habit.rs` (SPDX
`MIT OR Apache-2.0`; the crate already forbids unsafe), mirroring
`tracking.rs` in shape and doc discipline:

```rust
/// Hand-rolled A1 count: private field, fallible mint rejecting zero.
/// Decode is strict (a stored `0` cannot deserialize — the recur.rs
/// posture: mints decode strictly, leniency lives one layer up).
pub struct NonZeroCount(u32);
impl NonZeroCount {
    pub fn new(n: u32) -> Result<Self, InvalidHabitTarget>;
    pub fn get(self) -> u32;   // clamped-name `get` per house A1 style
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodKind { Day, Week, Month }

pub enum HabitTarget {          // serde externally tagged
    PerDay(NonZeroCount), PerWeek(NonZeroCount), PerMonth(NonZeroCount),
}
impl HabitTarget {
    pub fn kind(&self) -> PeriodKind;
    pub fn quota(&self) -> NonZeroCount;
}

/// A canonical period-start local date. Private field; the infallible
/// mint floors to the period start, so command-path keys are canonical
/// by construction (D-H3).
pub struct WindowStart(NaiveDate);
impl WindowStart {
    pub fn of_period(kind: PeriodKind, date: NaiveDate) -> Self;
    pub fn as_date(self) -> NaiveDate;
}

pub struct HabitClocks {        // per-field LWW (§4.1's list, verbatim)
    pub title: DateTime<Utc>, pub target: DateTime<Utc>,
    pub order: DateTime<Utc>, pub anchor_time: DateTime<Utc>,
    pub deleted: DateTime<Utc>,
}

pub struct Habit {
    pub id: HabitId,
    pub board: BoardId,
    pub title: String,
    pub target: HabitTarget,
    pub order: i64,                       // manual list order (D-C13)
    pub anchor_time: Option<NaiveTime>,   // display chip only (D-C13)
    pub clocks: HabitClocks,
    pub continued_from: Option<SeriesId>, // §4.7 seam, habit side
    pub deleted: bool,                    // tombstone, resurrection-safe
}

pub struct HabitLogKey { pub habit: HabitId, pub window: WindowStart }
pub struct HabitLogEntry { pub count: u32, pub clock: DateTime<Utc> }

pub struct HabitState {         // Default, PartialEq, Eq, Serialize+Deserialize
    pub habits: BTreeMap<HabitId, Habit>,
    pub log: BTreeMap<HabitLogKey, HabitLogEntry>,
}
```

Wiring: `state.rs` gains `pub habits: crate::habit::HabitState` between
`tracking` and `sync`; `persistence.rs` mirrors it in `PersistedState`
and maps it in `From<PersistedState> for AppState`; `lib.rs` exports the
public types. `idgen.rs` gains `new_habit_id()` (UUIDv7), with every
implementor updated in the same commit (fact 7).

**D-H12 (target-kind change)**: `UpdateHabit` may change the target
freely (count *and* kind). Rows keyed by a former kind's windows remain
in the map (they sync as facts in chunk 6) but are **analytically
inert**: every projection enumerates canonical windows of the *current*
kind only. Documented leniency at `HabitState`, pinned by a test.

### 3.2 Period arithmetic (D-H4) — one parameterized vocabulary

All in `habit.rs`, pure and total over the proptest domain (mirroring
`recur.rs`'s date-arithmetic posture; chrono-extreme saturation
documented at each function, strategies bounded to a safe window):

```rust
pub fn period_start(kind: PeriodKind, date: NaiveDate) -> NaiveDate;
    // Day: date itself; Week: the ISO week's Monday (chrono
    // `week(Weekday::Mon).first_day()`); Month: the month's 1st
    // (`with_day(1)`, infallible for day 1).
pub fn period_after(kind: PeriodKind, start: NaiveDate) -> NaiveDate;
    // +1 day / +7 days / checked_add_months(1) — saturating at the
    // chrono bound, documented.
pub fn period_before(kind: PeriodKind, start: NaiveDate) -> NaiveDate;
```

Period *end instants* — needed by the +24 h overflow — go through the
day-window vocabulary, never a second timezone concept:

```rust
fn period_end(kind, start, day: DayCalendar) -> DateTime<Utc>
    // = days::window_start(period_after(kind, start),
    //                      day.day_start_time, day.offset)
```

Laws (proptest §5.1, catalogue item 6): `period_start` idempotent;
`period_after(start) > start` and its `period_start` round-trips;
`period_before`/`period_after` are inverses; Week starts are Mondays;
Month starts are day-1.

### 3.3 Command payloads (messages.rs, D-H7)

`StateCommand` gains the family (serde: the enum rides the IPC wire —
round-trip tested like the tracking verbs):

```rust
CreateHabit { board: BoardId, title: String, target: HabitTarget },
UpdateHabit { habit: HabitId, changes: HabitChanges },
MoveHabit { habit: HabitId, order: i64 },
LogHabit { habit: HabitId },
AdjustHabitLog { habit: HabitId, window: WindowStart, count: u32 },
DeleteHabit { habit: HabitId },
```

```rust
pub struct HabitChanges {          // sparse, TaskChanges discipline
    pub title: Option<String>,
    pub target: Option<HabitTarget>,
    /// `Some(None)` clears the chip, `None` = untouched (double-Option
    /// helper, like `TaskChanges::scheduled`).
    pub anchor_time: Option<Option<chrono::NaiveTime>>,
    /// The §4.7 conversion seam's write path: `Some(Some(series))`
    /// links, `Some(None)` unlinks, `None` = untouched.
    pub continued_from: Option<Option<SeriesId>>,
}
```

**D-H7a**: `continued_from` is commandable *only* here — set at mint
time never (CreateHabit carries no link; a conversion links the
already-created pair), immutable in the planner beyond this field, and
carries **no clock slot** (§4.1's clock list omits it; it is seam data,
not contested content — same treatment as `Task.remote`).

### 3.4 Receipts and errors (command.rs)

`CommandOutcome` gains:

```rust
HabitCreated { habit: HabitId },
/// The +1 landed here: the attribution decision is user-visible
/// feedback (the repair case must be observable, not silent).
HabitLogged { habit: HabitId, window: WindowStart, count: u32 },
/// The audited correction's post-write state.
HabitLogAdjusted { habit: HabitId, window: WindowStart, count: u32 },
```

`UpdateHabit`/`MoveHabit`/`DeleteHabit` return `Applied`/`NoOp` per the
catalogue rules (MoveTask/DeleteTask precedent).

`CommandError` gains:

```rust
UnknownHabit(HabitId),      // no habit with this id
HabitDeleted(HabitId),      // tombstoned; edit-style intents refuse
InvalidHabitWindow { habit: HabitId, window: WindowStart },
    // AdjustHabitLog with a window that is not the canonical period
    // start of the habit's current target kind
```

`HabitTarget` zero is unrepresentable (the mint), so no
`InvalidHabitTarget` planner error exists; `AdjustHabitLog` accepts any
`u32` count including 0 (a correction may empty a window; the row stays
with count 0 — analytically identical to absent, honestly clocked).

### 3.5 Planner arms — exact semantics (command.rs)

All arms resolve the habit first via two new helpers in the catalogue's
style: `live_habit` (edit-style: unknown → `UnknownHabit`, tombstoned →
`HabitDeleted`) and `existing_habit` (terminal-style: only absence is
an error).

- **CreateHabit**: `live_board_of` first (unknown/deleted board → typed
  error). Mint `HabitId` (v7), clocks all `now`, `deleted: false`,
  `continued_from: None`, `anchor_time: None`.
  **D-H11 (order)**: the payload has no `order` (§4.2 verbatim), so the
  planner appends: `max(live habits of the board).order + 1`, else 0 —
  the CaptureTask append shape, board-scoped. The gap/midpoint law of
  `insertion_order_after` (fact 9) is written once in `habit.rs` as
  `append_order_after(habits_of_board, last_order)` — the same
  `gap == 1` tie rule — for `CreateHabit`'s append and any later
  batch-order affordance (D-C13's "sort by time is just a batch of
  MoveHabit edits"). Actions: `UpsertHabit`. No op (D-H9).
- **UpdateHabit**: `live_habit`; apply `HabitChanges` field-wise; stamp
  only the matching clock for `Some` entries (`clocks.title/target/
  anchor_time` — `continued_from` stamps nothing, D-H7a).
  **NoOp rule**: a changeset naming only unchanged values plans zero
  actions (the MoveTask-identical precedent — no clock churn onto the
  future sync wire).
- **MoveHabit**: `live_habit`; identical `order` → `NoOp`; else set
  `order`, stamp `clocks.order`, `UpsertHabit`. Mirrors MoveTask
  exactly (the UI computes midpoints on drag — D-C13).
- **LogHabit**: `live_habit`; window =
  `log_window(habit, &state.habits.log, now, day)` (§3.6); new row
  count = stored count for that key + 1, `clock: now` (fresh LWW
  clock); actions: `RecordHabitLog`. **No NoOp branch** — every
  invocation is exactly one increment, by design (count habits); the
  receipt's `count` is the post-increment value. Monotone under a
  monotone clock (property §5.1 item 1).
- **AdjustHabitLog**: `live_habit`; validate the window is canonical
  for the habit's *current* target kind (`window ==
  WindowStart::of_period(kind, window.as_date())`), else
  `InvalidHabitWindow`; set `count`, `clock: now` (the audited fresh
  stamp); identical count stored → `NoOp` (no clock churn); actions:
  `RecordHabitLog`. This is the explicit correction/backdating valve —
  analysis functions read its results but no automatic path uses it.
- **DeleteHabit**: `existing_habit` (terminal intent); already
  tombstoned → `NoOp`; else `deleted = true`, `clocks.deleted = now`,
  `UpsertHabit`. **The log is never touched** — history outlives the
  tracker (the review gate: DeleteHabit tombstones, it does not purge).
  Resurrection-safe on sync by the same LWW-clock argument as tasks
  (the fold that adopts a tombstone is chunk 6's).

Batch shape of every arm: entity/log actions only, no `EnqueueOp` tail
(D-H9).

### 3.6 Attribution and streaks (habit.rs, D-H5 + D-H6 — the recorded core)

**D-H5 — attribution happens at LogHabit time.** The design's §4.3
sentence ("a period counts iff quota met by period end + 24 h
overflow (window attribution; no knob)") is implemented as the +1's
*target-window choice*, because the normative log shape (§4.1: a count
per window, one LWW clock, **no per-completion instants**) makes
read-time attribution undefinable — row counts cannot be split by time
after the fact. Pure function:

```rust
pub fn log_window(habit: &Habit, log: &HabitState.log, now, day)
    -> WindowStart
```

1. `natural = WindowStart::of_period(kind, day.today(now))` — the
   window-date (night-owl rule: a 02:00 log with a 05:00 day start is
   still yesterday), floored to the period.
2. `prev = WindowStart::of_period(kind, period_before(kind,
   natural.as_date()))`; `prev_end = period_end(kind, prev.as_date(),
   day)`; `in_grace = now < prev_end + 24 h` (the fixed overflow; no
   knob, D-C8).
3. **Surplus-repairs rule**: return `prev` iff `in_grace` ∧
   `count(prev) < quota` ∧ `count(natural) >= quota`; else `natural`.

Why surplus-repairs (recorded, because it refines the one-liner): the
literal series-side rule ("late completions attribute to the earlier
member", §3.9 prop 5) applied unconditionally makes *every routine log*
land in the previous period (a 9 AM log on day D falls inside day D−1's
24 h overflow), lagging the whole streak one period and breaking
"streak = consecutive counting periods **ending at the current one**".
Guarding the repair on "today already met" keeps routine behavior
exactly natural and makes the overflow a *make-up* mechanism: doubling
up after meeting today heals yesterday (the D-C8 ADHD-forgiveness
intent), and `AdjustHabitLog` remains the explicit valve for everything
else. Per-kind reach of the repair falls out of the arithmetic: daily
habits can repair yesterday all day (prev_end + 24 h = end of today);
weekly/monthly habits only within the first 24 h of the new period —
the +24 h is uniform, the period lengths are not.

**D-H6 — streak with pending-transparent periods.** Pure and total:

```rust
pub fn streak(habit: &Habit, log: &HabitState.log, now, day) -> u32
```

Walk periods backward from `natural` (as in D-H5). For each period `p`:

- `count(p) >= quota` → **Counts**: `streak += 1`, continue backward.
- else if `now < period_end(p) + 24 h` → **Pending**: transparent —
  skip (neither counts nor breaks), continue backward. At most two
  periods can be pending for any `now` (the current one, plus the
  previous during its first 24 h — derivable from `period_end`
  arithmetic; pinned in the hand-model property).
- else → **Missed**: stop.

Return the accumulated count. Behavior contract: the morning of an
unmet today still shows yesterday's run (today is pending); a missed
period breaks the chain exactly when its grace closes (before that it
is pending, and a surplus log can still repair it, restoring the
chain); a brand-new habit with an empty log has streak 0 (today
pending, yesterday missed). Periods before the log's earliest row are
empty ⇒ missed ⇒ the walk terminates naturally — **no creation
boundary is needed** (a pre-creation row can only arrive via the
explicit `AdjustHabitLog` valve, which is allowed to count: audited
data).

Both functions read rows keyed by canonical windows of the current
kind only (D-H12); non-canonical decoded rows are inert.

### 3.7 The conversion seam, both directions (D-C12, D-H13)

- `Habit.continued_from: Option<SeriesId>` — **this chunk** (field +
  `UpdateHabit` write path + serde round-trip).
- `RecurRule.continued_from: Option<HabitId>` — already exists (DR-6/1,
  fact 8); no change here.
- **D-H13**: the read-side history-union view ("streak/history views
  union both sides at read time via the link", §4.7) lands with its
  consumer in chunk 6's surfaces. No speculative helper is built now —
  the fields are the seam; the union is presentation arithmetic.

### 3.8 Persistence (persistence.rs + storage dead arms, D-H10)

`PersistenceAction` gains exactly two variants:

```rust
UpsertHabit(crate::habit::Habit),
RecordHabitLog(crate::habit::HabitLogKey, crate::habit::HabitLogEntry),
```

`apply_actions` arms (the executable contract, doc-commented in the
semantics table):

- `UpsertHabit` — replace by id (entity upsert discipline).
- `RecordHabitLog` — keyed set-union: insert if the `(habit, window)`
  key is absent; on collision keep the row with the **strictly newer
  clock** (count LWW under the row clock, §4.5); equal clock keeps the
  stored row (deterministic tie; sqlite-upsert parity note as in
  `EnqueueOp`). Planner-written rows always carry a fresh `now`, so
  single-device sequences are monotone (property §5.1 item 1); the
  cross-device cost (concurrent same-window increments can lose one
  under LWW) is the design's accepted §4.5 semantics and becomes chunk
  6's fold documentation — recorded here so it is decided, not
  discovered.

No `DeleteHabit` action and no log-deletion action exist in this chunk
(tombstone = upsert; the log outlives; retention is chunk 6's
document-side GC).

**Storage crate (R-11 shape, fact 6)**: `repo.rs`'s exhaustive
partition match gains the two arms; the writes are runtime-checked
`sqlx::query` against §4.4's DDL (`habits` / `habit_log`, migration
0016) — dead until chunk 6's migration, with the R-11 comment naming
the hydrating chunk; `load_inner` keeps `habits:
HabitState::default()` with the same comment. The sqlite smoke tier
gains no habit commands (they would hit the missing tables).

### 3.9 Engine wiring (state crate)

None beyond the test tier: the actor routes `StateCommand` generically
(fact 11), `day_tick`/`DayRollConfig` is untouched (habits never
appear in it), and no `SystemEvent`/`NotifierMessage` changes exist.
The engine tier `state/tests/habits.rs` mirrors `tracking.rs`:
`FixedClock` + `InMemoryRepository` harness; create → log → publish
(memory ≡ disk), adjust shows in the published snapshot, delete
tombstones and the log row survives the publish, and the crash-resume
leg — a fresh engine over the same repository sees the log rows
(persisted intent). Poll-with-paused-time-deadline, oneshot replies,
no sleeps.

## 4. Decision summary (every open question, resolved)

| ID | Question | Decision |
|---|---|---|
| D-H1 | Module home | New `domain/src/habit.rs` (tracking.rs mirror); `AppState.habits` between `tracking` and `sync`; `PersistedState` mirrors; `From` maps |
| D-H2 | Count mint | `NonZeroCount(u32)`, private field, `new()` rejecting 0; strict decode (0 cannot deserialize; recur.rs posture) |
| D-H3 | Window key type | `WindowStart(NaiveDate)`, private field, canonicalizing mint `of_period(kind, date)`; serde transparent over the date — decoded non-canonical rows are documented-lenient and analytically inert; `AdjustHabitLog` rejects non-canonical explicitly (`InvalidHabitWindow`) |
| D-H4 | Period vocabulary | `PeriodKind` from the target; `period_start/_before/_after` pure fns (ISO Monday weeks; day-1 months; checked month add); boundary instants via `days::window_start` — `DayCalendar` is the only local-date concept |
| D-H5 | Where attribution lives | At LogHabit time (the +1's window choice) — the normative count-per-window shape has no instants for read-time attribution; **surplus-repairs** targeting: repair `prev` iff `now < prev.end + 24 h` ∧ prev unmet ∧ natural met, else natural. Refines §4.3's one-liner (reason in §3.6); flag in the PR description for the user's review |
| D-H6 | Streak contract | Walk from the current period: met counts, unmet-but-grace-open is transparent (≤ 2 skips, pinned), missed stops. Morning shows yesterday's run; a miss breaks exactly after its grace-free window; a grace-window repair restores it |
| D-H7 | Receipts/errors | `HabitCreated` / `HabitLogged { window, count }` / `HabitLogAdjusted { window, count }`; `UnknownHabit` / `HabitDeleted` / `InvalidHabitWindow`; edits use `Applied`/`NoOp` |
| D-H7a | `continued_from` writes | Settable only via `UpdateHabit`'s double-Option field; no clock slot (seam data, not contested content); CreateHabit never mints a link |
| D-H8 | Delete semantics | Tombstone upsert (`deleted` + `clocks.deleted`), log untouched, repeat `NoOp`, edit-style intents on tombstones refuse |
| D-H9 | Outbox lane | Zero ops planned by this chunk: the board-level sidecar flush trigger (`UpdateAppData` is task-keyed; habits have no task) is chunk 6's vocabulary; chunk 6 amends the arms' (stable) action tails |
| D-H10 | Log action semantics | `RecordHabitLog`: keyed set-union, strictly-newer-clock LWW, equal clock keeps stored; single-device monotonicity is planner-guaranteed; concurrent-increment loss under LWW is the design's accepted §4.5 cost, documented for chunk 6's fold |
| D-H11 | CreateHabit order | Append: `max(live order)+1` on the board, else 0 (payload has no order, §4.2); the gap/midpoint law written once as `habit.rs::append_order_after` |
| D-H12 | Target-kind change | Allowed; former-kind rows stay (sync facts) but are analytically inert — projections read current-kind canonical windows only |
| D-H13 | History-union view | Chunk 6 (lands with its consumer); this chunk ships only the fields |
| D-H14 | NoOp shapes | UpdateHabit unchanged-only mask → NoOp; MoveHabit identical order → NoOp; AdjustHabitLog same count → NoOp; LogHabit has no NoOp (each call is +1) |
| D-H15 | ResetBoard cascade | Not in this chunk: `ClearBoardContent`'s contract is chunk-6-amendable together with the storage half; a memory-only cascade would desync. Handoff note §8 |
| D-H16 | Ticks | None — never-done is native; `DayRollConfig`/`day_tick` untouched; no clock-driven anything |

## 5. TDD test plan

### 5.1 Proptest catalogue (domain, `habit.rs` tests + `test_support/habit_strategies.rs`)

Strategies: generated `(PeriodKind, quota 1..=50, date in a ±10 y
window, log rows over consecutive canonical periods, now)` — mirroring
recur.rs's bounded-domain posture.

1. **Log monotonicity** (acceptance: "the log is keyed set-union with
   monotone counts under clocks"): over generated command sequences
   (LogHabit at non-decreasing `now`, arbitrary AdjustHabitLog
   interleaved): after each LogHabit, the touched window's count
   increased by exactly 1 relative to its pre-command value, and the
   receipt's count equals the stored count.
2. **Attribution partition** (acceptance: "attribution partitions
   completions to exactly one period"): over generated sequences,
   Σ(window counts) = initial Σ + number of LogHabit commands (each
   +1 lands in exactly one window); each chosen window ∈
   {natural(now), prev(natural)}; a repair choice implies (prev unmet
   before the write ∧ natural met ∧ `now` in grace) — the D-H5 rule
   pinned constructively.
3. **Streak hand-model** (acceptance: "streaks match a hand model on
   generated sequences; a single miss breaks the streak exactly after
   its grace-free window"): `streak` equals a reference walk over the
   generated rows (the hand model, implemented plainly in the test);
   a single unmet period M among met periods yields, for `now` inside
   M's grace, a chain that still spans M's neighbors (M pending), and
   for `now` past M's grace, the chain broken at M; at most two
   pending skips occur for any `now`.
4. **anchor_time never sorts** (acceptance: "anchor_time never
   influences order (pinned negatively)"): two habit sets differing
   only in `anchor_time` values produce identical canonical
   `(order, id)` orderings; no habit ordering/sorting surface reads
   `anchor_time` (the canonical-order helper is the single ordering —
   written so the sort key structurally omits the field).
5. **Serde round-trip**: `Habit`, `HabitState`, `HabitLogKey/Entry`,
   `HabitChanges` round-trip losslessly; `NonZeroCount`/`HabitTarget`
   reject zero at decode (typed failure); a non-canonical
   `WindowStart` date decodes and is inert (reads as absent for every
   projection).
6. **Period laws** (D-H4): idempotent `period_start`;
   `after`/`before` inverses; Week ⇒ Monday; Month ⇒ day-1;
   `after(start) > start`.

### 5.2 Planner cases (command.rs tests, injected `CountingIds` + constructed state)

- CreateHabit: happy path (action shape, clocks all `now`, append
  order on empty and non-empty boards); unknown board; deleted board.
- UpdateHabit: each field; unchanged-only mask → `NoOp` (zero
  actions); anchor clear (`Some(None)`) vs untouched; continued_from
  link/unlink stamping nothing.
- MoveHabit: move stamps `clocks.order`; identical → `NoOp`; unknown/
  deleted → typed errors.
- LogHabit per kind: PerDay → today's window-date (plus the night-owl
  case: a 02:00 log with a 05:00 day start targets *yesterday*'s
  window); PerWeek → this ISO week's Monday; PerMonth → the month's
  1st; the repair case (yesterday unmet, today met, in grace) lands in
  yesterday and says so in the receipt; outside grace or today unmet →
  natural; unknown/deleted → typed errors; monotone under an advancing
  clock.
- AdjustHabitLog: set/overwrite/backdate; count 0; same-count →
  `NoOp`; non-canonical window → `InvalidHabitWindow`; unknown/
  deleted → typed errors.
- DeleteHabit: tombstone shape; log rows untouched in the same
  `apply_actions` result; repeat → `NoOp`; unknown → `UnknownHabit`.
- Absence-of-machinery pins: no `day_tick`/roll path reads habits
  (structural: `plan_day_rolls` filters tasks; a habit command's plan
  contains zero `EnqueueOp`s — D-H9's lane fence pinned).

### 5.3 Projection unit tests (habit.rs)

`log_window`/`streak` deterministic cases per kind: the morning case,
the grace case, the repair-restores case, the two-pending-skip case,
empty-log case, inert foreign-kind rows, quota change across kinds.

### 5.4 Engine tier (`state/tests/habits.rs`)

Create → log → adjust → delete journey over the live engine
(published snapshot assertions), the log outlives the tombstone in the
published state, and the crash-resume leg (fresh engine, same repo,
log intact). `FixedClock` advances drive all stamps.

### 5.5 Insta corpus ((w) lane)

One deliberate, diff-audited pass: every persisted dump gains the
empty `habits:` section (tails unchanged); a new s-series scenario
pair (populated habits + log, created/logged/adjusted/deleted through
the engine) joins the corpus like s36 did for tracking. Audit rule:
"the widening, nothing else" for the touched dumps.

## 6. Stage-2 execution order (TDD pairs → standalone green commits)

1. `feat(domain): the habit entity, state home, and persistence actions`
   — habit.rs entity + mints + period arithmetic (with 5.1 item 6 +
   serde tests); state/persistence wiring; `apply_actions` arms;
   `PersistenceAction` variants **with the storage R-11 arms in the
   same commit** (standalone-build rule, fact 6); idgen + implementor
   lockstep.
2. `feat(domain): the six habit verbs with typed receipts` — messages,
   receipts/errors, planner arms + §5.2 suite.
3. `feat(domain): the habit attribution, streaks, and strategies` —
   `log_window`/`streak` + §5.1 items 1–4 + §5.3 + strategy module.
4. `test(state): the habit engine tier` — §5.4.
5. `test(state): the habits insta corpus` — §5.5 regeneration.

Every commit: `cargo fmt --all -- --check` + clippy `-D warnings` +
nextest slice clean; the full AGENTS §10 gate suite green before
handoff (via `scripts/run-quality-gates.sh`).

## 7. Files touched & estimated diff size

| File | Change | ~Lines |
|---|---|---|
| `crates/taskboard-domain/src/habit.rs` | new: entity, mints, periods, projections + tests | +750 |
| `crates/taskboard-domain/src/messages.rs` | 6 commands + `HabitChanges` | +90 |
| `crates/taskboard-domain/src/command.rs` | helpers, receipts, errors, 6 arms + tests | +450 |
| `crates/taskboard-domain/src/persistence.rs` | mirror field, 2 actions, `apply_actions` arms | +80 |
| `crates/taskboard-domain/src/state.rs` | `habits` field | +5 |
| `crates/taskboard-domain/src/{ids,idgen,lib}.rs` | id kind already exists; generator method; exports | +30 |
| `crates/taskboard-domain/src/test_support/` | `habit_strategies.rs` + `CountingIds` arm | +170 |
| `crates/taskboard-storage-sqlite/src/repo.rs` | R-11 dead arms | +55 |
| `crates/taskboard-state/tests/habits.rs` | engine tier | +260 |
| `crates/taskboard-state/tests/snapshots/` | widening + new scenarios | +400 |
| `crates/taskboard-ui-slint/benches/support/mod.rs` | IdGenerator double arm | +2 |
| **Total** | | **≈ 2 300 insertions** (M, matching DR-5/1's 2 475) |

## 8. Risks & mitigations

- **D-H5 refines the origin one-liner** (attribution at log time,
  surplus-repairs): the reasoning is recorded (§3.6) and the surface is
  two pure functions + their properties — a pivot, if the user rules
  otherwise at PR review, is contained and cheap. Stage 3 must quote
  the deviation in the PR description.
- **chrono API**: `week(Weekday::Mon)`/`checked_add_months` behavior at
  the pinned version — verified by the period-law properties on the
  first TDD pass; strategies are bounded so saturation paths are
  unit-pinned, not luck.
- **(w) lane**: wave 7 contains exactly one shape-widener (this chunk);
  the regeneration must still be one pass — snapshot churn from other
  in-flight branches (e.g. `feat/series-surfaces`, disjoint files) is
  rebase-dance territory per AGENTS §8, not a reason to re-run the pass.
- **IdGenerator lockstep**: the new trait method compile-breaks every
  implementor — enumerate via grep (fact 7) in commit 1, never with a
  wildcard catch-all.
- **The storage dead arms execute real SQL against missing tables**:
  documented dead (no producer + smoke tier avoids habit commands until
  chunk 6), exactly as R-11 shipped.

## 9. Handoff notes

- For **chunk 6** (`feat/habit-storage-surfaces`): migration 0016 is
  the *next free number at its rebase* (dispatch §2 rule — re-verify;
  0013/0014 landed, 0015 is undo/1's); the sidecar sections + the
  board-level flush-trigger op vocabulary that D-H9 deferred; the
  `ClearBoardContent` habit cascade (D-H15); the history-union read
  view (D-H13); the retention GC.
- For **undo/1** (wave 9): the habit verbs join `StateCommand` before
  its inverse-synthesis catalogue is written — six more arms it must
  cover (the command-surface lane order exists precisely for this).
- For **stage 3**: review gates per dispatch §10.1 — the log key is
  `(habit, window)` set-union (a count overwrite that can lose an
  increment is LWW misapplication *within* a device sequence — pinned);
  no grace knob anywhere; `anchor_time` in no ordering path (grep);
  MoveHabit mirrors MoveTask; mints reject zero; DeleteHabit does not
  purge the log; the diff contains no storage migration, no CLI, no UI.
