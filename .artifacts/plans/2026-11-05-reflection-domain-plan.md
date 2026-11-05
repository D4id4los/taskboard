# [Plan] Reflection domain & engine — DR-7 chunk 1 (#636) — stage-1 TDD implementation plan

- **Date**: 2026-11-05
- **Kind**: stage-1 implementation plan for dispatch chunk 1 of the
  v0.2 completion round (`feat/reflection-domain`, issue **#636**).
  Normative inputs: the accepted DR-7 round
  (`.artifacts/plans/2026-11-05-journal-review-design-round-plan.md`)
  §1 anchors, §3 Parts A–D (D-J1–D-J7, D-J15) **including every dated
  in-session amendment** (the amended D-J3 flag set, the amended
  four-prompt D-J4 core, the D-J9 shape-B note — the amendments, not
  the originals), §4 decision table, §5 test hooks; the binding
  cross-milestone contract (round-1 implementation plan §3, quoted in
  DR-7 §3-A); the dispatch plan §2 (acceptance + the code-quality
  mandate); the completion implementation plan §4.1 (ADR is 0021, the
  guards hold, the amended sets are the ruled content). Deviations
  from the origin sketches are recorded decisions below, each with its
  reason.
- **Base**: `main` @ `8ea54d77` (worktree
  `../taskboard-WT-feat-reflection-domain`, created off `main` after
  `scripts/worktree-doctor.sh` passed — 170G free, floor 25G).
- **Status**: READY for stage 2.

## 1. Mission & binding constraints

Add the reflection half of DR-7 as **pure domain + planner + engine
tier** — no storage migration, no CLI, no UI:

- New module `taskboard-domain/src/reflection.rs`: `ReflectionEntry`
  (roaming-family v7 id, cadence + day label + **stored** window
  instants, `created_at`/`updated_at` LWW clocks, **tombstone-in-place**
  delete), `ReflectionAttributes` (`Rating` A1 mint 1–5, curated
  `HealthFlag` + `extra_flags` tolerant parking), `ReviewCadence`,
  the `ReviewWindow` A1 mint + the pure `review_window` derivation
  (composing the **one** `habit::period_start`/`period_after` flooring
  with `days.rs::window_start`), the `window_misses`/`window_done`
  pure projections, and the curated `DEFAULT_TEMPLATES` const.
- `ReflectionState` BTreeMap home on `AppState.reflection` +
  `PersistedState` (D-J5).
- `RecordReflection` / `UpdateReflection` / `DeleteReflection` —
  windows are data, the planner stays calendar-blind and validates
  structurally through `ReviewWindow::try_new` (D-J6); change-mask
  edits, flag delta, typed receipts/errors, `NoOp` idempotence.
- Undo: `NotUndoableReason::ReflectionSurface` + the `command_label`
  arms (D-J7).
- One new `PersistenceAction` (`UpsertReflectionEntry`, the R-11
  shape — decision **P-2** below); storage shows only dead arms.
- Properties, planner/engine/insta-snapshot tests; **ADR 0021**;
  CHANGELOG entry.

Fences (dispatch §2 + DR-7 §2, binding):

- **No migration** — 0022 is chunk 2's (`feat/reflection-storage-cli`,
  #637); this chunk creates no table (the R-11 dead arms are the
  recorded exception, decision P-2).
- **No `journal`-spelled code symbols** outside the undo substrate's
  existing homes, no `journal_entries`-spelled storage anywhere (the
  D-J1 guard rules, enforced by test §5.6). The undo types are touched
  additively only (`NotUndoableReason` + `command_label`).
- **Zero `UpdateAppData` enqueue** — the §3-A contract, pinned by a
  planner test: reflection plans enqueue no outbox ops of any kind.
- No sidecar section, no board attachment, no sync lane; no
  `appdata.rs` change; `ViewId::Review` and every ui-slint surface
  untouched (chunk 3's).
- Nothing reads the clock or the environment: `now` and the
  `DayCalendar` are data everywhere (`clock.rs`'s rule; the planner is
  calendar-blind per D-J6).
- A1 discipline (AGENTS §2): hand-rolled private-field newtypes with
  fallible mints at trust boundaries; no derive crates; deliberate
  leniency documented at the type; no validate-and-forget on write
  paths.
- `review_due` is **not** in this chunk (chunk 3's derived-at-read
  card, D-J10); this chunk ships only the projections D-J8 names.
- No kiosk, no surfacing, no walk (chunks 3's), no export (chunk 2's).

## 2. Verified seam facts (do not re-derive; verified 2026-11-05 against `8ea54d77`)

1. **ADR numbering**: `docs/adr/` ends at **0020**
   (`0020-files-sync-substrate-path-ownership-current-state-v1.org` —
   taken by the files-sync substrate; the DR-7 §1 anchor saying "next
   free is 0020" is stale exactly as the dispatch plan §2 records).
   This chunk writes **0021**. Migrations end at `0021_break_rule_board.sql`
   — the next free **0022** is chunk 2's; this chunk adds none.
2. **The vocabulary collision** (`journal.rs`, `ids.rs`): the undo
   command journal owns `JournalEntry`/`JournalId`/`JournalState`,
   `JournalState { entries, head }`, `UndoView`/`UndoLabel`,
   `command_label` (journal.rs), retention constants, and
   `JournalId` **defined in `ids.rs`** (a local_id! family member).
   `NotUndoableReason` lives in journal.rs :96-121; the classification
   sites live in `undo.rs` (`record_entry`'s facts match, the
   `command_class` lockstep match, `touched_board`).
3. **Nested-state pattern** (`state.rs` :25-55, `persistence.rs`):
   `AppState` carries `reminders`/`tracking`/`habits` as nested
   `BTreeMap`-based sub-states; `PersistedState` mirrors every
   persisted map and `From<PersistedState> for AppState` maps
   field-by-field; `AppState` is **Serialize-only**; `PersistedState`
   serde both ways. Field order = snapshot order.
4. **The engine's only write path into `AppState` is
   `apply_actions`** (`state/core.rs` :60-87): `interpret` delegates to
   the domain's `apply_actions(PersistedState)` and `app()` maps the
   fields one-by-one into a **fresh exhaustive `AppState` literal**
   (core.rs :78 — no `..default()` spread; it breaks compile on a new
   field and must grow the field). Consequence: a planner arm that
   mutated state without emitting a `PersistenceAction` could never
   publish — the action variant is forced by the single-advance rule.
5. **`PersistenceAction` is matched with no wildcard twice**: the
   domain `apply_actions` (the shared batch semantics, memory ≡ disk
   by construction) and the storage `repo.rs::execute_batch`
   partition match (:1290-1348, "no wildcard arm … until storage
   persists it"). Adding a variant forces both crates' arms in the
   same commit — the R-11 situation (the tracking DR-5/1 precedent,
   `2026-10-26-tracking-domain-engine-plan.md` §2 fact 3 + decision
   R-11: dead runtime-checked writes + `load` default-empty, the
   hydrating chunk named).
6. **`IdGenerator` has exactly two implementors** (`idgen.rs`
   `UuidV7Generator`, `test_support/mod.rs` `CountingIds`) — a new
   `new_reflection_id()` method breaks both, plus every exhaustive
   `match` over `StateCommand` inside `plan_command`/`record_entry`/
   `command_class`/`touched_board`/`command_label`.
7. **Strategy enumeration sites** that grow per arm:
   `test_support/messages.rs::state_command_strategy` (prop_oneof per
   command), `test_support/state.rs::persisted_state_strategy`
   (explicit field lists at :404/:523), `state/tests/proptest.rs::
   arbitrary_action` (one arm per action class).
8. **Entity precedent** (`tracking.rs`): `TimeEntry` with
   `created_at`/`updated_at` LWW clocks, `#[serde(default)]`
   back-compat kind enum, `ClosedSpan::try_new` trust-boundary mint
   (planner maps `SpanRejected` → `CommandError::InvalidSpan`), the
   `TrackingState::open_entries` derived-set precedent, pure
   window projections composed from `days.rs` vocabulary.
9. **Curated-enum + A1 precedents** (`tracking.rs` `TimeEntryKind`
   `parse_text_form`/`text_form` kebab-case unit names;
   `habit.rs` `NonZeroCount` — `#[serde(try_from = "u32")]` strict
   decode, "mints decode strictly, leniency lives one layer up";
   `Habit.deleted` tombstone-in-place).
10. **Period vocabulary** (`habit.rs`): `PeriodKind { Day, Week,
    Month }`, `period_start` (Day = date, Week = Monday-start via
    chrono week arithmetic, Month = day 1), `period_after`/`period_before`
    (checked, saturating at `NaiveDate::MAX`/`MIN` — documented
    leniency). **One flooring implementation**; DR-7 sanctions moving
    it to `days.rs` only if the cross-module call reads poorly at
    implementation time (decision P-8 keeps it in `habit.rs`).
11. **Day-window machinery** (`days.rs`): `window_start(date, 
    day_start_time, offset)`, `day_window(now, …)` (24h, contains
    `now`), `DayCalendar { day_start_time, offset }` (`today`/
    `calendar_date`/`instant`), `today_sections` (first-match,
    four disjoint sections, canonical orders), the D-F6 consumer
    contract (`(day_start_time, offset)` parsed once at the app layer,
    passed as data), the `dst_boundary_instants()` battery in
    `test_support`, and the totality posture ("total for every
    representable `now` at least one day inside chrono's date range").
12. **Planner shape** (`command.rs`): `plan_command`'s catalogue is
    exhaustive over `StateCommand`; `CommandError` is a `Copy`
    thiserror enum of typed rejections; `CommandOutcome` carries
    post-write rows (the `TimeLogged { entry }` shape); the `LogTime`
    arm is the mint-at-planner exemplar (validate → build row with
    `created_at/updated_at = now` → upsert + receipt); `AdjustTimeEntry`
    is the change-mask-no-churn exemplar (a mask naming only unchanged
    values is the accepted no-op without touching clocks);
    `no_op()` plans zero actions (the journal's empty-batch rule then
    journals nothing).
13. **Engine harness** (`state/tests/`): `FixedClock` + 
    `InMemoryRepository` + `CountingIds`, oneshot replies as
    completion signals, `eventually()` poll-with-paused-deadline (no
    sleeps); `tracking.rs`/`habits.rs` are the per-family engine-tier
    exemplars (publish → memory ≡ disk → crash-resume over the same
    repo); `snapshots.rs` is the S-series (persisted `PersistedState`
    YAML dumps + memory-only tails; a new section = one assert line +
    regen + a new scenario); `sqlite_smoke.rs` gains **no** reflection
    commands (the table does not exist until 0022).
14. **`AppState` literal sites** beyond core.rs use the
    `..AppState::default()` spread (habit.rs :784,
    merge_testutil, pipeline, app tests) and survive the new field
    untouched — verified; only `core.rs::app()` and
    `From<PersistedState>` are exhaustive.
15. **Source-scan test precedent**: ui-slint's
    `tests/icon_mapping.rs` reads `.slint` sources via
    `CARGO_MANIFEST_DIR`-relative paths — the shape the vocabulary
    guard test mirrors (§5.6).

## 3. Design

### 3.1 The vocabulary ruling applied (D-J1) + the guard test

Module `taskboard-domain/src/reflection.rs`; symbols `ReflectionEntry`,
`ReflectionId`, `ReflectionState`, `ReflectionTemplate`,
`ReflectionAttributes`, `HealthFlag`, `Rating`, `ReviewCadence`,
`ReviewWindow`, `DEFAULT_TEMPLATES`, `review_window`, `window_misses`,
`window_done`. The user-facing `journal` naming belongs to chunks 2–3;
nothing in this chunk's surfaces exists yet, so the guard is pure code
hygiene, enforced by test (§5.6). The `Review` collision is UI-side
(chunk 3's `ReviewWalk` discipline); this chunk only records it in the
ADR.

### 3.2 The entity and its state home (D-J2, D-J5) — shapes verbatim

```rust
pub struct ReflectionEntry {
    pub id: ReflectionId,            // local_id!, Uuid v7 — ROAMING family (contract §3.2)
    pub cadence: ReviewCadence,      // Day | Week | Month — which walk/verb produced it
    pub day: NaiveDate,              // the window-date label (DayCalendar::today of window_start)
    pub window_start: DateTime<Utc>, // the reviewed window's opening instant
    pub window_end: DateTime<Utc>,   // the window's closing instant (exclusive)
    pub text: String,                // the free-text body (may be empty)
    pub attributes: ReflectionAttributes, // mood/energy + health flags (D-J3)
    pub created_at: DateTime<Utc>,   // row creation stamp (the minting planner's now)
    pub updated_at: DateTime<Utc>,   // the LWW mutation clock (contract §3.2, embedded)
    pub deleted: bool,               // the in-place tombstone (contract §3.4, D-J6)
}
```

- `ReflectionId` joins `ids.rs`'s `local_id!` family with the roaming
  doc (the `TimeEntryId` wording: "roaming family — the future
  files-sync section's wire key"). It never syncs in v0.2; the
  *family* is the roaming-readiness (D-J5).
- Tombstone-in-place: `deleted: bool` rides the row; delete flips it
  and bumps `updated_at`; the row survives in the map (the
  `Habit.deleted` precedent — the contract's "tombstone-shaped rows …
  never silent absence" read literally). `is_live()` = `!deleted`
  (the `Habit::is_live` shape).
- `ReflectionState { entries: BTreeMap<ReflectionId, ReflectionEntry> }`
  mounted as `AppState.reflection` (field placed after `habits`,
  before `sync`) and mirrored on `PersistedState` + the
  `From<PersistedState>` mapping. `ReflectionState::live_entries()`
  is the one derived live-read seam (the `TrackingState::open_entries`
  precedent): tombstoned rows stay in the map; projection sites read
  through `live_entries()` so "reads filter `!deleted`" has a single
  home.

### 3.3 Attributes and the curated sets (D-J3, D-J4) — ruled content verbatim

```rust
pub struct ReflectionAttributes {
    pub mood: Option<Rating>,          // 1..=5, A1 mint (`Rating::try_new`)
    pub energy: Option<Rating>,        // 1..=5, A1 mint
    pub flags: BTreeSet<HealthFlag>,   // curated enum v1 (R-5)
    pub extra_flags: BTreeSet<String>, // unknown names parked verbatim (tolerant decode)
}
```

**`Rating`** — one hand-rolled A1 newtype (private `u8`, fallible
`try_new` rejecting 0 and ≥6) shared by both fields, the
`NonZeroCount` mirror including **strict decode**: 
`#[derive(…, Serialize, Deserialize)] #[serde(try_from = "u8")]` — a
stored 0 or 6+ cannot deserialize (data error, never a clamp). The
mint's error is `InvalidRating`. The type doc carries D-J3's leniency
note: decoded rows are trusted-written; every write path mints through
`try_new` (planner) or strict decode (serde), so no validate-and-forget
path exists (decision P-3).

**`HealthFlag`** — curated serde enum, kebab-case unit variants,
`Debug/Clone/Copy/PartialEq/Eq/PartialOrd/Ord/Hash/Serialize/
Deserialize`. **The member set is the AMENDED ruling (2026-11-05,
user)** — enum order is declaration order:

1. `migraine`
2. `minor-headache`
3. `medium-headache`
4. `cold-or-flu`
5. `allergies`
6. `cramps`
7. `poor-sleep`
8. `overwhelmed`
9. `good-day` (the positive marker)

(`low-medication-day` was removed by the user; the two headache
grades added; `migraine` stays distinct as the severe case. The
original proposal list is history — do not resurrect it.) Extension is
**append-only** (R-5): the serialized form is the name string; never
rename or renumber. The mint `HealthFlag::try_new(name: &str) ->
Result<Self, UnknownHealthFlag>` accepts exactly the nine kebab names
(exact match; `"Migraine"` is a rejection — case matters). 
**`UnknownHealthFlag` lives at the mint, not in `CommandError`**
(decision P-4): the command payloads carry typed
`BTreeSet<HealthFlag>` (D-J6's table), so the planner structurally
cannot see an unknown name; the typed rejection is the mint's error,
consumed by chunk 2's CLI `--flag` parse. DR-7 §5's "UnknownHealthFlag
is typed" hook is satisfied at the mint's unit tier.

**Serde tolerance** (`ReflectionAttributes`): `#[derive(Serialize)]`
(both sets serialize as sorted JSON arrays — `BTreeSet` order is
deterministic) + **hand-written `Deserialize`** that decodes the
`flags` array as names and partitions: a known name mints into
`flags`, an **unknown name parks verbatim into `extra_flags`** (the
`AppDataSections.unknown` tolerance pattern applied to a set field —
the contract's "unknown-field tolerance in any serialized form").
Unknown *struct keys* are tolerated by serde's default ignore
behavior — pinned, not configured. `extra_flags` never merges back
into `flags` on encode; decode→encode is byte-stable (§5.1 item 10).

**Curated templates** (D-J4, the amended four-prompt core verbatim —
`ReflectionTemplate { id: &'static str, name: &'static str, cadence:
ReviewCadence, prompts: &'static [&'static str] }`; decision P-1 for
the slice field):

| Template | `id` / `name` | Prompts |
|---|---|---|
| Day | `daily-close` / `Daily close` | `What did I get done today?` · `What am I grateful for today?` · `What would I do differently next time?` · `What's still on my mind?` |
| Week | `weekly-close` / `Weekly close` | `What did I get done this week?` · `What am I grateful for this week?` · `What would I do differently next week?` · `What's still on my mind?` |
| Month | `monthly-close` / `Monthly close` | `What did I get done this month?` · `What am I grateful for this month?` · `What would I do differently next month?` · `What's still on my mind?` |

`pub const DEFAULT_TEMPLATES: &[ReflectionTemplate]` — compiled-in
curated data, no stored rows, no config section (P7); the type shape
is the roaming-ready seam for #639's later stored templates. The set's
laws, pinned by test: exactly one template per cadence; ids unique;
the four prompt texts byte-identical to the table above (a golden
assertion on the const — data, not UI text).

### 3.4 Windows: the `ReviewWindow` mint and the `review_window` derivation (D-J6's structural half + D-J8)

**`ReviewCadence { Day, Week, Month }`** — its own enum in
reflection.rs (not `habit::PeriodKind`: separate concern, separate
doc), kebab-case serde (`"day"`/`"week"`/`"month"` — the future
storage column values), plus `parse_text_form`/`text_form` (the
`TimeEntryKind` precedent, chunk 2's CLI seam).

**`ReviewWindow`** — the A1 mint the calendar-blind planner validates
through (private fields, `Copy`):

```rust
pub struct ReviewWindow { cadence: ReviewCadence, day: NaiveDate,
                          start: DateTime<Utc>, end: DateTime<Utc> }
impl ReviewWindow {
    pub fn try_new(cadence, day: NaiveDate, start, end)
        -> Result<Self, InvalidReviewWindow>;
    pub const fn cadence/day/start/end accessors;
}
```

Validation (structural only — **calendar-blind**, the D-F6 consumer
contract): `end > start` always; **Day** requires `end − start ==
24h` exactly; **Week** requires `end − start == 168h` exactly;
**Month** requires only monotonicity (month length varies, nothing
else is checkable calendar-free). `day` is **taken on trust as mint
input** — the mint documents the leniency: it enforces nothing about
the label (a non-Monday label on a week window is tolerated, the
`WindowStart::from_stored`/D-H12 posture), because the label is data
the caller owns. Rejection: `InvalidReviewWindow` (reflection.rs's
mint error; the planner maps it to its own `CommandError` variant,
the `SpanRejected`→`InvalidSpan` shape).

**`review_window(cadence, reference_date: NaiveDate, calendar:
DayCalendar) -> ReviewWindow`** — the pure derivation, total in the
`days.rs` posture's bounded domain (documented at the fn, mirroring
`day_window`'s totality note). One flooring implementation, two
consumers (habits, review):

- **Day** → `[window_start(ref, dstime, offset), window_start(ref + 1d, …))`
  — the window whose window-date label is `ref`. (D-J8's
  "`day_window(reference)`" compositionally: `day_window` takes an
  instant; `review_window` takes the label, so the pieces are
  `window_start` per day — the same pieces `day_window` itself is
  built on. The law `review_window(Day, calendar.today(now)) ==
  day_window(now)` is pinned as a property.)
- **Week** → `[window_start(period_start(Week, ref)), window_start(period_after(Week, period_start(Week, ref))))`
  — Monday-anchored 168h.
- **Month** → the calendar-month bounds the same way:
  `[window_start(day 1), window_start(day 1 of next month))`.

The derived output satisfies `try_new` by construction (pinned);
`calendar.calendar_date(w.start())` equals the floored period-start
date for every cadence (the label law, pinned). Fixed offsets have no
transitions, so the arithmetic is gap-free — the DST battery (fact 11)
composes through unchanged (the `trailing_days_window` test's shape).

**Decision P-8**: `period_start`/`period_after` **stay in `habit.rs`**
(this plan's reading of D-J8's sanction: the move to `days.rs` is
available *only if* the call reads poorly at implementation time; the
DR-6 modules already import `days.rs` the same direction, and moving
the flooring rewrites habit call sites in the same commit — churn the
sanction exists to avoid, not require). If stage 2 finds the call
genuinely unreadable, the move is still sanctioned — as its own
commit, mechanical, no semantic diff.

### 3.5 The window projections (D-J8) — exact predicates and orders

Both live in reflection.rs, are pure, and take windows as data. They
are the week/month parameterisation's read (the day walk reads
`today_sections` verbatim — chunk 3); the consistency pins (§5.1
items 11–12) guarantee no second classification.

```rust
pub fn window_misses(tasks: &BTreeMap<TaskId, Task>,
                     now: DateTime<Utc>,
                     window: (DateTime<Utc>, DateTime<Utc>)) -> Vec<TaskId>
pub fn window_done(tasks: &BTreeMap<TaskId, Task>,
                   window: (DateTime<Utc>, DateTime<Utc>)) -> Vec<TaskId>
```

- **`window_misses`**: live ∧ open ∧ **any anchor qualifies**, where
  an anchor qualifies iff it fell inside the window and before `now`:
  `(scheduled ∈ [start, end) ∧ scheduled < now) ∨ (duedate ∈ [start, end) ∧ duedate < now)`.
  A task carrying both fields qualifies through either (DR-7's "the
  task's anchor instant — scheduled or due"). **Decision P-5**: the
  result orders by `(anchor, order, id)` where `anchor` is the
  earliest qualifying instant of the task — a chronological
  carry-forward list (the scheduled section's `(scheduled, order, id)`
  precedent generalized over the mixed anchor set). Not-missed shapes:
  anchors at exactly `now` (the `< now` conjunct), anchors outside the
  window (half-open), done tasks (the open gate), tombstoned tasks.
- **`window_done`**: live ∧ `done ∈ [start, end)`, ordered
  `(done, id)` ascending — the `today_sections.done_today` shape
  verbatim (its first-match context is irrelevant to a standalone
  done-scan: a done task never enters the open-gated sections, which
  is exactly what the day-cadence equality pin §5.1 item 12 checks).

Neither projection stores or derives anything: ids only, the
`TodaySections` discipline (the caller reads entities from its own
map).

### 3.6 Command payloads (`messages.rs`, D-J6)

Three new `StateCommand` arms, appended after `DeleteHabit` (family
grouping; serde is name-keyed so arm order is wire-neutral; journal
decode is additive by construction — new commands only ever appear in
*new* journal leaves):

```rust
/// Record one reflection entry for the caller-derived window. The
/// planner is calendar-blind: it validates the window structurally
/// (`ReviewWindow::try_new`) and stores the caller's window verbatim.
/// An empty record (no text ∧ no attributes) is the accepted no-op.
RecordReflection {
    cadence: ReviewCadence,
    day: NaiveDate,
    window: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    text: String,
    mood: Option<u8>,             // the mint input; the planner mints through Rating::try_new
    energy: Option<u8>,
    flags: std::collections::BTreeSet<HealthFlag>,
},
/// Edit one entry. Every slot is a change mask (`None` = unchanged);
/// the rating slots are double-Options (`Some(None)` = clear,
/// `Some(Some(v))` = set, minted at the planner — bad numbers are
/// typed rejections, never clamps). Flags move by delta
/// (`flags_add`/`flags_remove`), never whole-set replace — a
/// fat-fingered edit cannot wipe the flag set. Text `Some("")` is the
/// legal clear. All-unchanged is the accepted no-op (clocks untouched).
UpdateReflection {
    id: ReflectionId,
    text: Option<String>,
    #[serde(default, deserialize_with = "deserialize_double_option",
            skip_serializing_if = "Option::is_none")]
    mood: Option<Option<u8>>,
    #[serde(default, deserialize_with = "deserialize_double_option",
            skip_serializing_if = "Option::is_none")]
    energy: Option<Option<u8>>,
    flags_add: std::collections::BTreeSet<HealthFlag>,
    flags_remove: std::collections::BTreeSet<HealthFlag>,
},
/// Tombstone one entry in place (the row survives; D-J2). A repeat on
/// an already-deleted entry is the accepted no-op; an unknown id is a
/// typed rejection (decision P-6).
DeleteReflection { id: ReflectionId },
```

The `deserialize_double_option` helper already exists in messages.rs
(the `HabitChanges.anchor_time` precedent — reuse it, never duplicate).
`flags_add`/`flags_remove` ride `#[serde(default)]` (additive-field
discipline for old journal payloads, which cannot contain them anyway
— but the empty-set default keeps the payload's absence decode-clean).

### 3.7 Receipts and errors (`command.rs`)

`CommandOutcome` grows three variants (the `TimeLogged`/`EntryDeleted`
post-write-row shapes):

- `ReflectionRecorded { entry: ReflectionEntry }`
- `ReflectionUpdated { entry: ReflectionEntry }` (the post-write row)
- `ReflectionDeleted { id: ReflectionId }`

`CommandError` grows three variants (the `UnknownHabit(HabitId)` /
`InvalidSpan` shapes):

- `UnknownReflection(ReflectionId)` — no row with this id exists.
- `InvalidReviewWindow` — the window failed the structural mint.
- `InvalidRating` — a rating input outside 1..=5 (the planner minted
  it; never a clamp).

### 3.8 Planner arms — exact semantics (`command.rs`, D-J6)

Validation precedes no-op detection in every arm (a malformed payload
is a typed rejection even if the record would have been empty — the
`LogTime` order).

**`RecordReflection`**:
1. `ReviewWindow::try_new(cadence, day, window.0, window.1)` →
   `InvalidReviewWindow` on rejection.
2. `mood.map(Rating::try_new)` / `energy.map(Rating::try_new)` →
   `InvalidRating` on rejection.
3. Empty-record check: `text.is_empty() ∧ mood_none ∧ energy_none ∧
   flags.is_empty()` → `no_op()` (plans zero actions; the journal's
   empty-batch rule then journals nothing — an empty record was never
   an event).
4. Mint the row: `id = ids.new_reflection_id()`, `created_at =
   updated_at = now` (the mutation clocks are the planner's `now`,
   never window data — the `LogTime` R-14 rule), `deleted = false`,
   window/day stored verbatim from the minted `ReviewWindow`, flags
   from the payload, `extra_flags` empty (write paths never park).
5. `PersistenceAction::UpsertReflectionEntry(entry)` + receipt.
   **No `EnqueueOp` of any kind** (the §3-A contract; pinned).

**`UpdateReflection`**:
1. `app.reflection.entries.get(id)` → `None` → `UnknownReflection(id)`.
2. Row tombstoned → `no_op()` (idempotent repeat; D-J6).
3. Validate mask ratings (`Some(Some(v))` slots) → `InvalidRating`.
4. Compute prospective values (mask → stored-or-new; flag delta →
   stored ∪ add ∖ remove). If the row would be unchanged → `no_op()`
   **without** touching clocks (the `AdjustTimeEntry` mask-no-churn
   rule: a mask naming only unchanged values cannot bump `updated_at`).
5. Else write: `updated_at = now`, all masked fields applied,
   `UpsertReflectionEntry` + `ReflectionUpdated { entry }` (post-write).

**`DeleteReflection`**:
1. Unknown id → `UnknownReflection(id)`. **Decision P-6**: a typed
   rejection, deliberately unlike `DeleteTimeEntry`'s accepted no-op —
   reflection rows are device-local with no sync fold that could have
   removed a row underneath a selector, so an unknown id is a user
   error worth naming (D-J6 rules it explicitly).
2. Already tombstoned → `no_op()` (repeat delete, D-J6).
3. Else `deleted = true`, `updated_at = now`, `UpsertReflectionEntry`
   (delete *is* an upsert — the one-action shape, D-J13) +
   `ReflectionDeleted { id }`.

All three arms classify `NotUndoable(ReflectionSurface)` in the
journal (§3.10) and ride `plan_journaled_command` unchanged — accepted
mutating commands journal, no-ops don't.

### 3.9 Persistence — the R-11 shape (decision P-2)

`PersistenceAction` gains **exactly one** variant:
`UpsertReflectionEntry(crate::reflection::ReflectionEntry)` — the
tombstone-in-place ruling makes delete an upsert, so the
exhaustive-match forcing needs a single arm (D-J13).

- **Domain `apply_actions` arm** (this chunk): replace by id —
  `state.reflection.entries.insert(entry.id, entry.clone())` (the
  entity-upsert discipline; no cascade exists — nothing references a
  reflection entry). This arm is what makes the chunk-1 engine hooks
  implementable at all: `apply_actions` is the engine's only write
  path into `AppState` (fact 4), so a planner arm without an action
  variant could never publish, and the crash-resume leg needs the
  fake repo's persistence.
- **Storage `repo.rs`** (forced by the no-wildcard match, fact 5 —
  the R-11 exception, the tracking DR-5/1 precedent verbatim): the
  partition match grows the arm buffering into `reflection_upserts`,
  written by a **runtime-checked** `sqlx::query`
  (`INSERT … ON CONFLICT(entry_id) DO UPDATE SET …`) against D-J13's
  `reflection_entries` DDL — **dead until chunk 2's migration 0022**,
  with the R-11 comment naming #637 as the hydrating chunk. If a
  reflection-bearing batch ever reached sqlite before 0022 it fails
  loudly at runtime ("no such table") and the batch rolls back —
  honest failure, never silent loss; nothing in this chunk's tests
  reaches it (`sqlite_smoke.rs` gains no reflection commands).
  `load_inner` keeps `reflection: ReflectionState::default()` with
  the same comment (sqlite cannot carry reflection data before the
  table exists, so default-empty is truthful, not lossy).
- This is the chunk's single recorded deviation from dispatch §2's
  letter ("No migration (storage is chunk 2)"): no migration lands
  here — the *dead write arms* do, exactly as R-11 ruled for DR-5/1
  and the habit DR-6/5 chunk repeated. Chunk 2 converts the writes to
  compile-checked macros and adds the load codec + migration.

### 3.10 The undo arm (D-J7)

- `NotUndoableReason` grows **`ReflectionSurface`** (journal.rs,
  additive enum arm; serde name is the variant name, consistent with
  `TrackingSurface`/`HabitSurface`). The doc comment carries the
  three recorded reasons: (i) the inverse of a record is a tombstone —
  a *different stored fact* than row-absence, exactly the lossiness
  that made `LogHabit` `HabitSurface`; (ii) un-delete would resurrect
  with a minted-then-inverted clock — the tracking close/reopen
  asymmetry (ADR 0017) in mirror; (iii) the family's own
  `UpdateReflection`/`DeleteReflection` verbs are the recovery
  surface — cheaper than undo for self-authored text, and P9's undo
  everywhere is already served for the *task* operations the walk
  dispatches (those ride existing classified commands).
- **`undo.rs::record_entry`** grows the classification arm: the three
  commands → `UndoFacts::NotUndoable(ReflectionSurface)`.
- **`undo.rs::command_class`** (the lockstep match): the three arms
  join the not-undoable tail list (never consulted — heads refuse
  first; carried for compilation).
- **`undo.rs::touched_board`**: the three arms → `None` (not
  board-scoped).
- **`journal.rs::command_label`** grows three arms: `Record
  reflection` / `Edit reflection` / `Delete reflection`
  (presentation-only; never test-asserted — the `head_label`
  contract).

### 3.11 Engine wiring (state crate)

None beyond the test tier: the actor routes `StateCommand` generically
through `plan_journaled_command` (fact 13); `day_tick`/`DayRollConfig`
untouched (D-J10 declined the tick ride-in); no `SystemEvent`/
`NotifierMessage` changes; `core.rs::app()` grows the
`reflection: self.state.reflection.clone()` line (the exhaustive
literal, fact 4). The shape-widener lane: every persisted-insta dump
grows the `reflection` key — **one deliberate, diff-audited regen
pass** in the shape-widening commit (§4 step 4); the audit asserts
only the `reflection:` line (empty map) differs on existing snapshots.

## 4. Decision summary (every open question, resolved)

| ID | Question | Decision | Reason |
|---|---|---|---|
| P-1 | `ReflectionTemplate.prompts` type | `&'static [&'static str]`, not `Vec` | DR-7's sketch cannot compile: a `const` cannot populate `Vec` fields (const-context allocation). Content unchanged — the four-prompt table verbatim. |
| P-2 | The persistence boundary with "no migration" | The `UpsertReflectionEntry` variant + `apply_actions` arm + storage **dead runtime-checked write arm** + `load` default-empty land in this chunk; migration 0022 + codec decode + compile-checked macros are chunk 2's | Forced by construction: `apply_actions` is the engine's only write path (fact 4) — without the variant the publish and crash-resume hooks (DR-7 §5 chunk-1 engine tier) are unimplementable; the storage crate's no-wildcard match breaks compile otherwise (fact 5). This is the R-11 exception, the DR-5/1 and DR-6/5 precedent verbatim; dispatch §2's "no migration" holds. |
| P-3 | `Rating` serde decode | Strict (`#[serde(try_from = "u8")]`); 0/6+ are data errors | The `NonZeroCount` posture: mints decode strictly, leniency lives one layer up. No legacy rows exist (new family), so strictness risks nothing; it guarantees decoded rows are planner-clean. |
| P-4 | Where `UnknownHealthFlag` lives | At the mint (`HealthFlag::try_new`), **not** as a `CommandError` variant | The D-J6 payloads carry typed `BTreeSet<HealthFlag>`; the planner structurally cannot see an unknown name. A `CommandError` variant would be dead code. DR-7 §5's hook is the mint's unit tier. |
| P-5 | `window_misses` ordering | `(anchor, order, id)`, anchor = earliest qualifying instant | A chronological carry-forward list; generalizes the scheduled section's order over the mixed anchor set. (DR-7 leaves the order open; chunk 3's walk consumes it.) |
| P-6 | `DeleteReflection` on unknown id | Typed `UnknownReflection`, not a no-op | D-J6 rules it; unlike time entries there is no sync fold that can remove a row under a selector (device-local), so absence is a genuine user error. |
| P-7 | Validation vs no-op order | Validate first (window, ratings), then empty/all-unchanged checks | A malformed payload is a rejection even when the record would have been empty (the `LogTime` order). |
| P-8 | Where `period_start`/`period_after` live | Stay in `habit.rs`; the `days.rs` move remains sanctioned-but-not-exercised | D-J8's sanction is conditional ("if the call reads poorly"); the DR-6 modules already import `days.rs` vocabulary, and the move would rewrite habit call sites in the same commit — churn the sanction exists to avoid, not require. Re-ruled by stage 2 only if genuinely unreadable, as its own mechanical commit. |
| P-9 | `review_window`'s signature | `(cadence, reference_date: NaiveDate, calendar: DayCalendar) -> ReviewWindow`, infallible in the bounded domain | D-J8's wording takes the label (the CLI's `--day` is a reference *date*, D-J11); totality documented like `day_window`'s ("at least one day inside chrono's date range"). |
| P-10 | The three `StateCommand` arms' position | After `DeleteHabit`, before `Undo` | Family grouping; serde/journal decode is name-keyed, so wire-neutral. |

## 5. TDD test plan

### 5.1 Property catalogue (domain, `reflection.rs` test modules)

1. **Day windows**: `review_window(Day, ref, cal)` — `end − start ==
   24h` exactly; `start == window_start(ref, …)`; the label law
   `cal.calendar_date(start) == ref`; contains the reference (the
   reference's own window-start instant ∈ `[start, end)`). Over
   `timestamp_strategy()` × day-start × offset strategies (the
   days.rs property shapes).
2. **Week windows**: 168h exactly; `start ==
   window_start(period_start(Week, ref))`; `calendar_date(start)` is
   a Monday; mid-week references floor to the same window (equality
   across references within one week).
3. **Month windows**: `start == window_start(day 1)`; `end ==
   window_start(day 1 of next month)`; `calendar_date(start)`'s
   day-of-month == 1; `end > start` across month lengths (28–31) and
   leap years.
4. **Cadence-uniform pins**: the derived window satisfies
   `ReviewWindow::try_new` (composition law); equal inputs equal
   outputs (determinism); `review_window(Day, cal.today(now), cal) ==
   day_window(now)` (no second day-window definition).
5. **DST battery**: every `dst_boundary_instants()` fixture composes
   through the fixed-offset rule — bounds equal the plain
   `window_start` composition (the `trailing_days_window` test's
   shape).
6. **`ReviewWindow` mint rejections** (units + table): `end ≤ start`
   (all cadences), Day ≠ 24h, Week ≠ 168h, Month monotonic-only
   (a 400-day "month" span is *accepted* — the documented
   calendar-blindness); the `day` label is never validated (a
   non-Monday label on a week window is accepted — the documented
   trust).
7. **`Rating`**: mint rejects 0 and 6..=255, accepts 1..=5;
   serde strict decode rejects 0 and ≥6 as data errors; roundtrip
   1..=5.
8. **`HealthFlag`**: `try_new` accept table (the nine ruled names) +
   reject table (`""`, `"Migraine"`, `"migraines"`, `"low-medication-day"`
   — the retired name must parse *no*); serde unit names kebab-case
   pinned for all nine; `BTreeSet` ordering is declaration order
   (the `Ord` derive's declaration order — deterministic snapshots).
9. **Attributes serde tolerance**: for JSON carrying unknown flag
   names interleaved with known: decode parks unknowns into
   `extra_flags` verbatim, keeps knowns in `flags`; decode→encode→
   decode is byte-stable (`to_json → from_json → to_json` identical);
   unknown *struct keys* are ignored (tolerated); `extra_flags` never
   leaks back into `flags`; a tombstoned entry's full row roundtrips
   (`deleted: true` shape included).
10. **`window_misses`/`window_done` over generated task sets** (the
    `today_sections` test-module strategy shape: generated stamps
    wandering ±1 day around the window): predicate membership per
    §3.5 (both anchor conjuncts, the `≥ now` and outside-window
    exclusions, the open gate, the live gate); the orderings
    (`(anchor, order, id)` / `(done, id)`); determinism.

Consistency pins (the "no second classification" guarantees, DR-7 §5):

11. For the day window: `window_misses(tasks, now, day_window) ==
    (overdue ∪ scheduled sections of `today_sections(tasks, now,
    day_window)`) ∩ anchor-qualifying` — the two-sided composition:
    every miss is classified by `today_sections` (no third
    classification exists), and every anchor-qualifying member of
    those two sections is a miss.
12. For the day window: `window_done(tasks, day_window) ==
    today_sections(tasks, now, day_window).done_today` exactly
    (predicate and order).

### 5.2 Planner cases (`command.rs` tests, injected `CountingIds` + constructed state, `FixedClock` instants)

- Record: receipt carries the minted id + `now`-stamped clocks; the
  row stores the caller's window **verbatim** (byte-equal instants);
  empty record (text-only-empty, attributes-only-empty, both) →
  `NoOp` with zero actions; invalid window (Day ≠ 24h, inverted,
  Week ≠ 168h) → `InvalidReviewWindow`; rating inputs 0 and 6 →
  `InvalidRating` (never a clamp — the row is not written); flags
  round-trip into the row; `flush_targets(&plan)` / `EnqueueOp`
  scan is **empty** (the §3-A contract pin).
- Update: change-mask matrix — outer `None` untouched; `Some(None)`
  clears; `Some(Some(v))` sets after minting; text `Some("")` clears;
  flag delta adds and removes without replacing the untouched rest;
  all-unchanged → `NoOp` **with `updated_at` unmoved**; unknown id →
  `UnknownReflection`; tombstoned target → `NoOp`; bad masked rating →
  `InvalidRating` even when the rest of the mask is unchanged.
- Delete: tombstones in place (row present, `deleted: true`,
  `updated_at` moved); repeat → `NoOp`; unknown → `UnknownReflection`.
- Journal leg (with `plan_journaled_command`): accepted records carry
  a journal entry whose facts are `NotUndoable(ReflectionSurface)`;
  no-ops journal nothing (the empty-batch rule).

### 5.3 Undo tier (`journal/tests.rs` + `undo/tests.rs`)

- Classification pin: the journaled reflection entry's facts are
  typed `NotUndoable(ReflectionSurface)` (structural, no text).
- `plan_undo` over a reflection head → `Err(TargetNotUndoable(
  ReflectionSurface))` (typed `matches!`, never a message).
- `command_label` grows the three arms (compilation is the lockstep;
  the labels themselves are presentation-only and unasserted).

### 5.4 Engine tier (`state/tests/reflection.rs`, the `tracking.rs` harness shape)

- Record → row visible in the published snapshot (memory ≡ disk).
- Update → the post-write row publishes; the mask-no-churn no-op
  publishes nothing (state hash unchanged).
- Delete → tombstone visible in `app.reflection.entries`, absent from
  `live_entries()`; the row survives the publish.
- Crash-resume: a fresh engine over the same `InMemoryRepository`
  lists the entry (deterministic; join/predicate waits only).
- Undo of a reflection head refuses typed
  (`TargetNotUndoable(ReflectionSurface)`).
- Paused-time deadlines, oneshot replies, no sleeps; Miri
  environment-gate header like the sibling suites.

### 5.5 Insta corpus ((w) lane)

- One new scenario (record → update → delete journey) grows the
  reflection section; **one regen pass** over the existing persisted
  dumps — the audit rule: only the `reflection:` key (empty map) may
  differ on pre-existing snapshots. The memory-only tail snapshots do
  not change (`reflection` is persisted, not memory-only).

### 5.6 The vocabulary-guard test (D-J1, decision — enforced by test, not hope)

`crates/taskboard-domain/tests/vocabulary_guards.rs` (integration
test; SPDX header; `#[cfg_attr(miri, ignore)]` — it reads the
filesystem, the tracking.rs environment-gate posture):

- Walks `src/**/*.rs` under `CARGO_MANIFEST_DIR` and asserts **zero**
  `Journal`-prefixed identifiers (regex `\bJournal[A-Za-z]`) outside
  the undo substrate's existing homes: `journal.rs`, `journal/`,
  `undo.rs`, `undo/`, `ids.rs` (where `JournalId` legitimately lives
  today), and `lib.rs` (the re-export lines). The new `reflection.rs`
  must be clean — as must every future file, which is the drift
  guard.
- Asserts the string `journal_entries` appears **nowhere** in the
  crate's sources (the table-name guard's domain-side half; chunk 2
  re-pins it against the storage crate).
- Mirrors the `icon_mapping.rs` source-scan shape; failure output
  names the offending file and line.

### 5.7 Gates

Insta snapshots grow the section (persisted dumps only); `cargo deny`
untouched (zero new dependencies — everything is hand-rolled per the
house stack); no log/UI text assertions anywhere; deterministic tests
(no sleeps, fixed stamps, `tokio::time::pause` where the engine tier
needs the clock advanced); full suite green through
`scripts/run-quality-gates.sh`.

## 6. ADR 0021 outline (D-J15, house org format)

`docs/adr/0021-reflection-family-device-local-roaming-ready.org`:

- **Title**: "ADR 0021: The Reflection Family — Device-Local by
  Ruling, Roaming-Ready by Construction".
- **Header block**: Date 2026-11-05; Status: Accepted (lands with the
  v0.2 completion dispatch chunk 1, #636); Depends on: ADR 0019 (the
  undo journal and its classification), ADR 0017 (the close/reopen
  asymmetry the undo refusal mirrors); Carriers: #636 (this chunk),
  #637/#638 (the later chunks), the DR-7 round.
- **Context**: #621's journal mandate and the R-1 device-local ruling
  (options report option C); the VF-3 vocabulary collision; the
  binding cross-milestone contract (§3-A quoted — the files-sync
  import fold is the first consumer); R-2 boundary-generic; R-5
  curated flags; R-7 gentle surfacing; P9/P12/P13.
- **Decision** (records D-J1/J2/J6/J7/J10 per D-J15): 1. the
  vocabulary split (code = `reflection`, surfaces = `journal`; the
  guard rules; `Review` taken by statistics → `ReviewWalk`); 2. the
  entity (roaming-family v7 id, stored window instants — never
  re-derived at read, LWW clocks, tombstone-in-place); 3. curated
  enums behind A1 mints + tolerant serde (the ruled flag set, the
  four-prompt template core as compiled-in data); 4. the command
  family (windows are data, the calendar-blind planner, change-mask +
  delta edits, `NoOp` idempotence, zero `UpdateAppData`); 5. the undo
  arm (the three recorded reasons); 6. the boundary-generic review
  primitive (`ReviewCadence` + `review_window` — one flooring
  implementation, two consumers); 7. derived-at-read gentle surfacing
  (`review_due`, one card, no machinery, the tick ride-in and
  reminders-pipeline alternatives declined) — recorded here, shipped
  by chunk 3.
- **Consequences**: device-local v0.2 with the import-fold readiness
  statement (rows read exactly as stored; the union keys on entry
  UUID; tombstones are rows); retention/GC named as future work (the
  scale sanity: ~0.5–2 KB/day); the fence list (#639–#642, #623 slice
  B, the `nextcloud-files-sync` milestone); the D-J15 numbering note
  (0020 collision resolved by re-verify-at-base; renumbering is the
  sanctioned mechanical response).

## 7. Stage-2 execution order (TDD pairs → standalone green commits)

Every commit standalone (rebase-merge hygiene): correct build,
meaningful `<type>(<module>)` subject. Tests are written *before*
their production code inside each step's working tree; the commit is
the green checkpoint.

1. `feat(domain): the reflection vocabulary and entity mints` —
   reflection.rs skeleton (`ReflectionId` in ids.rs + idgen +
   CountingIds + lib re-exports), `ReviewCadence`, `Rating`,
   `HealthFlag`, `ReflectionAttributes` (serde), `ReflectionEntry`,
   `ReflectionTemplate`/`DEFAULT_TEMPLATES`, `ReflectionState` +
   `live_entries`; §5.1 items 7–10 green.
2. `feat(domain): the review windows and their flooring derivation` —
   `ReviewWindow::try_new` + `review_window`; §5.1 items 1–6 green.
3. `feat(domain): the window misses and done projections` —
   `window_misses`/`window_done`; §5.1 items 10–12 green.
4. `feat(domain): the reflection state home and persistence arm` —
   the shape-widener commit: `AppState`/`PersistedState`/`From` +
   `PersistenceAction::UpsertReflectionEntry` + `apply_actions` arm +
   `core.rs::app()` field + strategies growth (fact 7) + storage
   R-11 dead arms + the one audited insta regen (§5.5's empty-map
   lines; the new scenario lands in step 7).
5. `feat(domain): the reflection commands and planner semantics` —
   messages.rs arms + receipts/errors + the three planner arms +
   §5.2 green.
6. `feat(domain): the reflection undo arm` — `NotUndoableReason::
   ReflectionSurface` + the three undo.rs sites + `command_label` +
   §5.3 green.
7. `test(state): the reflection engine tier` — `state/tests/
   reflection.rs` + the snapshot scenario (+ regen if the scenario
   adds dumps); §5.4 green.
8. `test(domain): the reflection vocabulary guards` — §5.6 green.
9. `docs(adr): the reflection family is device-local and roaming-ready` —
   ADR 0021 per §6.
10. `docs(changelog): the reflection domain entry` — the Unreleased
    entry (Added: the reflection family, the three commands, the
    undo arm, the state home; storage/CLI/UI explicitly chunk 2–3).

Do not close #636 (the PR body carries "Closes #636", stage 3).
No push, no PR from stage 2.

## 8. Files touched & estimated diff size

| File | Change | Est. |
|---|---|---|
| `crates/taskboard-domain/src/reflection.rs` | new — types, mints, templates, windows, projections + tests | ~+700 |
| `crates/taskboard-domain/src/ids.rs` | `ReflectionId` | +8 |
| `crates/taskboard-domain/src/idgen.rs` | `new_reflection_id` (trait + impl) | +9 |
| `crates/taskboard-domain/src/test_support/mod.rs` | `CountingIds` arm | +4 |
| `crates/taskboard-domain/src/test_support/messages.rs` | 3 strategy arms | +25 |
| `crates/taskboard-domain/src/test_support/state.rs` | 2 field lines | +4 |
| `crates/taskboard-domain/src/lib.rs` | module + re-exports + module-map line | +12 |
| `crates/taskboard-domain/src/messages.rs` | 3 command arms | +65 |
| `crates/taskboard-domain/src/command.rs` | errors, outcomes, 3 planner arms + tests | +320 |
| `crates/taskboard-domain/src/journal.rs` | `ReflectionSurface`, 3 label arms | +16 |
| `crates/taskboard-domain/src/journal/tests.rs` | serde name pin | +12 |
| `crates/taskboard-domain/src/undo.rs` | 3 sites | +30 |
| `crates/taskboard-domain/src/undo/tests.rs` | classification + refusal pins | +40 |
| `crates/taskboard-domain/src/state.rs` | `AppState.reflection` | +8 |
| `crates/taskboard-domain/src/persistence.rs` | action variant + apply arm + `PersistedState`/`From` | +25 |
| `crates/taskboard-domain/tests/vocabulary_guards.rs` | new | +75 |
| `crates/taskboard-state/src/core.rs` | the literal's field | +2 |
| `crates/taskboard-state/tests/reflection.rs` | new engine tier | ~+220 |
| `crates/taskboard-state/tests/snapshots.rs` | scenario + assert line | +45 |
| `crates/taskboard-state/tests/proptest.rs` | `arbitrary_action` arm | +10 |
| `crates/taskboard-state/tests/snapshots/*.snap` | regen (audited) | ~38 files, +1 line each |
| `crates/taskboard-storage-sqlite/src/repo.rs` | R-11 partition + dead write | +30 |
| `docs/adr/0021-….org` | new | ~+110 |
| `CHANGELOG.md` | Unreleased entry | +15 |

Production diff ≈ **1.8–2.2k lines** including tests and the snapshot
regen — consistent with the dispatch's **M** size.

## 9. Risks & mitigations

- **The R-11 dead-write window**: a sqlite-backed daemon that
  executed a reflection command between this chunk's merge and chunk
  2's would hit the missing table and roll the batch back (loud,
  typed, no loss). Mitigation: the dispatch runs chunks 1→2 serially
  and chunk 2 converts the arms within days; `sqlite_smoke.rs` gains
  no reflection commands; the R-11 comments name #637.
- **Consistency-pin disagreement**: if §5.1 items 11–12 fail at
  implementation, the predicate reading — not the pin — is suspect
  (a new test failing can be a production bug, AGENTS §5): re-derive
  against `today_sections`' first-match semantics and fix the
  projection, never weaken the pin. The two-sided composition in
  item 11 is the exact statement of DR-7's "where both apply".
- **Insta regen breadth**: every persisted dump grows the
  `reflection:` key. Mitigation: the regen is one commit with the
  audit rule (only that key may differ); the memory-only tails must
  not change — a tail diff is a finding.
- **Numbering drift** (ADR/migration — 0020 already collided once):
  re-verified at this base (fact 1); stage 3 re-verifies at rebase
  and records any collision (the standing rule).
- **Vocabulary drift**: the guard test fails the build on any new
  `Journal*` symbol outside the allowlist; stage 3 additionally
  greps the diff (dispatch §2 trap (a)).
- **Strategy-growth churn**: growing `state_command_strategy`/
  `arbitrary_action` shifts existing proptest distributions — expect
  the regression files to absorb new cases; no assertion text changes.
- **Acceptance slippage**: chunks 2–4 consume this entity; the shapes
  above are ruled and verbatim, so the exposure is execution, not
  design.

## 10. Handoff notes

- The binding contract (impl-plan round-1 §3) is satisfied by
  construction and pinned: device-local (no sync lane anywhere in the
  diff), roaming-ready rows (roaming-family v7 id, embedded clock,
  tombstone rows, tolerant serde, A1 mints), zero `UpdateAppData`
  (§5.2's planner pin).
- Stage 3's trap list (dispatch §2): guards enforced by test; the
  contract; the curated sets match the **amended** rulings exactly (a
  resurrected `low-medication-day` or a three-prompt core is a
  finding — check against the dated amendment notes, i.e. this plan's
  §3.3 tables); the undo arm typed and joined into `command_label`;
  ADR numbered **0021**; window derivations proptested at
  cadence/calendar edges (DST battery included).
- The two shapes stage 2 must not "fix": the `&'static [&'static str]`
  prompts (P-1 — the `Vec` sketch cannot compile in a const) and the
  strict `Rating` decode (P-3 — leniency lives one layer up, never in
  the mint).
- `review_due`, the walk, the card, the CLI, the export, the
  migration: all out of scope here — chunks 2–3's plans inherit from
  the landed code, not from this document's sketches.
