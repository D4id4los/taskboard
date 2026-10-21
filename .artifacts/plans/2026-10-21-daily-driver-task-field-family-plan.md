# [Plan] Daily-Driver Chunk 1 — Domain Field Family + Lockstep Manifest + 0007 + Native `startdate`

- **Date**: 2026-10-21
- **Kind**: stage-1 TDD implementation plan (dispatch plan §2, prompt 1.1;
  MD-6 path). Chunk 1 of the daily-driver implementation round; branch
  `feat/domain-task-field-family`; **closes #208**, annotates #209/#244.
- **Origin (normative, in MD-7 rank)**: implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-1/2/3, §3 chunk 1, §4, §5 (the interim rule), §6; design docs
  `.artifacts/plans/2026-10-18-field-family-design-plan.md` §3.1 D-F1,
  §3.2 D-F2 (branch A, RULED), §3.3 D-F3 (assertions 1–2), §3.4 D-F4,
  §5 ACCEPTED record; `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
  §3.2 D-S2 (`TaskEntry` value encodings this chunk's serde shapes must
  match), §3.6 D-S6 (the guard flags); dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1 (MD-1…MD-10), §2 (acceptance + traps).
- **Status**: precondition verified 2026-10-21 — milestone
  `wave-multi-board` has **no open issues** (round gate MD-1/MD-9 green),
  the dispatch plan is on `origin/main` (`3f4f586`),
  `scripts/worktree-doctor.sh` green (116G free; one empty leftover
  sibling dir `taskboard-WT-ci` removed by `rmdir` to unblock the
  doctor's glob — recorded deviation, zero content lost). Worktree
  `../taskboard-WT-feat-domain-task-field-family` created off
  `origin/main` (local `main` was 3 commits stale; the dispatch plan
  mandates "off the then-current `main`" = `origin/main`).

---

## 0. Vocabulary relocations (MD-8 — symbols are the stable keys)

The multi-board round moved/renamed three seams the plan-day docs name;
this plan uses today's names throughout:

| Plan-day name | Today's symbol (re-located, line refs this chunk's) |
|---|---|
| `StateCommand::EditTask` | `StateCommand::UpdateTask { id, changes }` (domain `messages.rs:31`, handler `command.rs:328`) |
| `RemoteTask` (sync `model.rs`) | **domain** `remote.rs:74` `RemoteTask` (multi-board moved the merge inputs into the domain); the sync crate's wire DTO is `model.rs:103` `Card` |
| `parse_due`/CLI flags | unchanged, but **out of scope** (chunk 5) |

Every other symbol kept its name; the full re-located baseline follows.

## 1. Verified baseline (re-located on `origin/main` @ `3f4f586`)

| Seam | Symbol (file) | Lines | Notes |
|---|---|---|---|
| Task shape | `Task` (domain `entities.rs`) | 165–194 | 14 fields; `duedate: Option<DateTime<Utc>>` :176 is the mirror target; `clocks` :190 |
| Eight clocks | `TaskClocks` (entities.rs) | 49–67 | title/description/duedate/done/position/labels/archived/deleted |
| Epoch clock | `BoardClocks::epoch` (entities.rs) | 107–115 | `0001-01-01T00:00:00+00:00`-equivalent instant (`-62_135_596_800`); storage twin is the 0005 SQL default string |
| Newtype precedent | `Color` (entities.rs) | 22–43 | private inner, `new`/`as_str`, lenient by design — `Effort`'s structural model |
| Sparse edit | `TaskChanges` (messages.rs) | 252–269 | 4 slots; `duedate`'s serde attrs :261–265 are the double-option template; struct doc :250–251 is the stamping contract |
| Double option | `deserialize_double_option` (messages.rs) | 273–280 | generic, reused verbatim |
| Edit handler | `StateCommand::UpdateTask` arm (command.rs) | 328–354 | per-slot `if let Some` + clock stamp; `applied_task` (:1231) = upsert + `LocalOp::UpdateTask` enqueue, unconditional today |
| Guard struct | `PendingTaskIntent` (pipeline.rs) | 90–101 | 8 bools, private to `pipeline` (must become `pub(crate)` for assertion 1) |
| Guard build | `pending_task_intents` (pipeline.rs) | 104–152 | `queued_at > read_at`; `UpdateTask` arm sets title/description/duedate/done/archived :131–138 |
| Guard apply | `keep_task_intent` (pipeline.rs) | 158–206 | value **and** clock copy per flag |
| Native merge | `merge_task` (merge.rs) | 170–262 | the `duedate` verdict :192–196 is the template; `remote_wins` :71; debug verdict event :234–247 |
| Adopt/echo | `adopt_remote_task` :91 (+`clocks_from` :128), `adopt_task_after_push` :291 | — | struct literals that must grow the new fields |
| Merge inputs | `RemoteTask` (domain remote.rs) | 74–97 | `duedate` :82; `startdate` appears **nowhere** in the repo (grep-verified) |
| Push shapes | `NewCardShape`/`CardShape` + `from_task` (push.rs) | 143–159 / 164–179 / 181–207 | consumed by sync `push_exec/execute.rs` (`create_task` :328, `update_task` :579) |
| Rules model | `ReminderRule` (reminders.rs) | 108–120 | `created_at` doc :117–119 reserves the write clock — chunk 4 consumes it; **read-only here** |
| Storage row | `TaskRow`/`decode_task`/tasks upsert (repo.rs) | 75–97 / 251–315 / 905–952 | `query_as!` macros ⇒ `.sqlx` offline cache must be regenerated after SQL edits |
| Whole-state batch | `load_inner`/`apply_inner`/`execute_batch` (repo.rs) | 438+ / 696+ / 725+ | tasks SELECT :485–500, INSERT :906 |
| Op tags | `op_kind_to_text` (codec.rs) | 41–59 | additive-tag discipline; **no new tag this chunk** (`update_app_data` is chunk 3's) |
| Migrations | `0001_init.sql` (:40–62 tasks), `0005` (:8–10 epoch defaults), `0004` (:12–17 `reminder_rules`) | — | 0006 is multi-board's; this wave numbers from 0007 |
| Wire DTO | `Card` (sync model.rs) | 101–146 | `duedate` :121–123 (`default`, `alias = "dueDate"`, `ser_opt_datetime`, no skip) |
| Create payload | `NewCard` (sync client.rs) | 124–141 | `duedate` :131–137 (`rename`, `skip_serializing_if`, `ser_opt_datetime`) |
| Wire mapping | `map_card`/`new_card_from`/`apply_card_shape` (sync mapping.rs) | 76 / 212 / 267 | the three `duedate` sites a `startdate` twins |
| Contract tier 1 | `tests/contract.rs` (wiremock) | 706–767 | create exact-payload + update full-round-trip patterns; inline `card_json` helper :554; fixtures `tests/fixtures/deck/` |
| Proptest corpus | domain `test_support/{entities,messages,remote,state}.rs` | — | `task_strategy` :64, `task_changes_strategy` (messages :9), `remote_task_strategy` (remote :315), `snapshot_strategy` (remote :228, `SnapshotTaskRow` :55) |
| Insta corpus | `taskboard-state/tests/snapshots.rs` → `snapshots/s{1..10}_{persisted_state,published_app_state}.snap` | — | 20 files; **the** regeneration pass |
| Storage round-trips | `taskboard-storage-sqlite/tests/roundtrip.rs` (+`common/`) | — | `task()`/:118, `task_clocks()`/:32 builders; proptest `persisted_state_roundtrips_through_sqlite` :147 |
| Offline SQL cache | `.sqlx/` (repo root) | — | `query_as!`/`query!` against changed SQL ⇒ `cargo sqlx prepare` regenerates; CI builds offline |

## 2. Decisions (every open question settled; D-numbers are this plan's)

- **D1 — types live in `entities.rs`**, next to `Color` (D-F1's own
  anchor). `TASK_FIELDS` + `FieldFamily` also in `entities.rs` (the
  design's "(or a tiny task_fields.rs)" option declined: the manifest's
  doc comment names `Task`/`TaskClocks` members; colocating keeps the
  #244 ceremony visible at the struct definitions, and a 30-line module
  would only fragment it).
- **D2 — `Effort` mirrors `Color`'s encapsulation**: private `u64`
  inner, `Effort::new(u64)`, `Effort::seconds() -> u64` accessor;
  `#[serde(transparent)]` (bare integer — exactly D-S2's
  `"effort": { "v": 900 }` value shape); derives
  `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize,
  Deserialize`. No `Default` (unset is `Option`, not a zero). No cap on
  the type; the `≤ i64::MAX` fence is the storage codec's (D-F1).
- **D3 — the two enums are hand-written twins, not a macro**: each is
  `VeryLow | Low | Normal | High | VeryHigh` with
  `#[serde(rename_all = "snake_case")]` (unit names `very_low`…`very_high`
  = the storage TEXT encoding, D-F4) and derives including
  `PartialOrd, Ord` (declaration order = severity order — the ruled
  sorting contract; derived field-less `Ord` compares discriminants).
  Derives: `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
  Serialize, Deserialize`. Two distinct types is the ruling (cross-assignment
  is a type error); a shared macro would re-couple them for zero lines
  saved.
- **D4 — field placement**: `Task.scheduled` immediately after
  `duedate`; `effort`/`importance`/`urgency` after `labels` (the AppData
  family grouped, before `archived`); `TaskClocks` in the same relative
  order. Manifest rows follow the same order. (Serde key *sets* are
  order-free; declaration order only fixes where the insta diffs show
  the new keys.)
- **D5 — `TaskChanges` slots** copy `duedate`'s serde attribute set
  verbatim (`default, deserialize_with = "deserialize_double_option",
  skip_serializing_if = "Option::is_none"`). Handler rule (citation):
  the struct's own contract, messages.rs:250–251 — *"Stamp the matching
  field clock only for `Some` entries"*; `Some(None)` writes the value
  `None` **and** stamps the clock (a cleared field is a write — D-F1's
  tie to D-S2's `{v: null, clock}`).
- **D6 — the interim enqueue split (recorded, MD-10)**: today any
  non-empty `UpdateTask` changeset returns `applied_task` (upsert +
  `UpdateTask` enqueue). Chunk 1 splits on family: if **any native slot**
  is set (title/description/duedate/archived/**scheduled**) →
  `applied_task` unchanged; if **only app slots** are set
  (effort/importance/urgency) → upsert only, **no enqueue**
  (`UpdateAppData` does not exist yet; enqueueing `UpdateTask` for an
  app-only edit would full-send a card with no native change — a no-op
  push). `scheduled` is native (D-F2 branch A) and rides `UpdateTask`.
  The chunk-4 fold/enqueue chunk reverses exactly the app-slot half.
- **D7 — `PendingTaskIntent` widens per D-S6**: five new bools
  (`scheduled, effort, importance, urgency, reminder_rules`), gains
  `Serialize, Deserialize` derives (assertion 1's need) and becomes
  `pub(crate)` (private today; no *public* API grows — the struct stays
  unnameable outside the crate). `pending_task_intents`' `UpdateTask`
  arm grows `scheduled = true` (native full-send set); **no arm sets the
  app-field or rules flags this chunk** (that is chunk 4's
  `UpdateAppData` arm). `keep_task_intent` grows **only the
  `scheduled`** value+clock copy — the app-field copies live inside
  `fold_task_app_data` (chunk 4, D-S6's ruling), and a copy written now
  would be unreachable dead code. The guard's `tracing::debug!` event
  gains the five flags mechanically.
- **D8 — manifest** exactly as D-F3's table (12 rows; `position` →
  `["stack", "order"]` composite; `labels` whole-set; `scheduled` row 9
  `Native`; the last three `AppData`). `FieldFamily` derives
  `Debug, Clone, Copy, PartialEq, Eq`. Doc comment records the
  connector-independence ruling (the `family` is *the Deck connector's*
  lane assignment, never an intrinsic field property — so a future
  connector never forks `Task`/`TaskClocks`).
- **D9 — assertion 1 mechanics** (domain test, entities.rs `#[cfg(test)]`):
  key sets via `serde_json::to_value(x).as_object().keys()` collected
  into `BTreeSet<&str>`; expected clock set from `TASK_FIELDS`.
  (a) populated `TaskClocks` (all twelve literal epoch stamps) key set
  **==** manifest clock set (exact equality);
  (b) all-true `PendingTaskIntent` key set: `intent \ clocks == {
  "reminder_rules" }` **and** `clocks ⊆ intent` — the rules flag is
  D-S6's deliberate non-clock member and is pinned as exactly that
  difference (the design's "equal exactly" wording predates the flag;
  this is its faithful reading, not a deviation);
  (c) populated `Task` (via `test_support::task_strategy()` sample) key
  set ⊇ union of all manifest `task_fields`;
  (d) fully-populated `TaskChanges` (every slot `Some(...)`, so
  `skip_serializing_if` hides nothing) key set ⊆ manifest clock set.
- **D10 — assertion 2 mechanics** (new storage integration test
  `tests/schema.rs`): `FileDb` (fully migrated), then
  - `PRAGMA table_info(tasks)` → column-name set: the `ck_*` columns
    found must **equal** `{ "ck_" + clock : clock ∈ manifest }` exactly
    (no stray clocks, none missing);
  - one value column per manifest `task_field`, carrying the three
    non-verbatim mappings in the test: `"stack" → "stack"`,
    `"order" → "sort_order"`, `"labels" →` the `task_labels` link table
    (asserted to exist with exactly columns `task`,`label` via
    `PRAGMA table_info(task_labels)`); all other names verbatim
    (`title, description, duedate, done, archived, deleted, scheduled,
    effort, importance, urgency`); every expected column ∈ the tasks set.
- **D11 — 0007 SQL is final** (§4 below): one file, plain `ALTER`s; the
  sqlx migrator wraps each migration file in one transaction (the
  acceptance's "one file, one transaction" is satisfied by the
  migrator's own semantics; no `-- no-transaction` marker).
- **D12 — storage codec**: `TaskRow` +8 fields
  (`scheduled: Option<DateTime<Utc>>`, `effort: Option<i64>`,
  `importance/urgency: Option<String>`, four `ck_*: String`); SELECT,
  INSERT, and `ON CONFLICT` grow symmetrically; `decode_task` parses the
  four clock TEXTs and maps the enums. Enum text mapping = two small
  `pub(crate)` fns in `codec.rs` (`importance_to_text`,
  `importance_from_text`, urgency twins; `from_text` returns
  `Option<...>` over the five serde names — the one-representation
  discipline: storage TEXT **is** the serde unit name). Effort decode:
  `u64::try_from(i64)` (negative ⇒ `Corrupted`); encode:
  `i64::try_from(effort.seconds())` (overflow past `i64::MAX` ⇒
  `Corrupted` — D-F1's typed bound check at the only boundary that has
  an i64). **`reminder_rules.updated_at` is untouched**: no
  `ReminderRuleRow` field, no SELECT/INSERT column — plain INSERTs
  inherit the epoch default (DDF-1; a wiring diff here is chunk-4 scope
  creep, per the trap list).
- **D13 — merge verdict**: `merge_task` gains the `duedate`-verdict twin
  (`remote_wins(ts_r, local.clocks.scheduled)` → take
  `remote.scheduled`, stamp `ts_r`) + the `scheduled_remote_wins` field
  on the R4 debug event; `adopt_remote_task`/`adopt_task_after_push`/
  `clocks_from` literals grow the fields (`adopt_*` take values as-is;
  echo clocks `max(local, lm)` — the existing shape covers the new
  clock). `tombstone_task` is clone-based — nothing.
- **D14 — push shapes**: `NewCardShape` + `CardShape` gain
  `scheduled: Option<DateTime<Utc>>` after `duedate`; both `from_task`
  sites populate it. No `MaterializedOp` change.
- **D15 — sync crate twins**: `model.rs Card.startdate` mirrors
  `duedate`'s attrs exactly (`default`, `alias = "startDate"`,
  `serialize_with = "ser_opt_datetime"`; camelCase `rename_all` makes
  the wire key `startdate`); `client.rs NewCard.startdate` mirrors its
  `duedate` (`rename = "startdate"`, `skip_serializing_if =
  "Option::is_none"`, `ser_opt_datetime`); `mapping.rs`: `map_card`
  gains `scheduled: card.startdate`, `new_card_from` gains
  `startdate: shape.scheduled`, `apply_card_shape` gains
  `card.startdate = shape.scheduled`. Null-tolerance both directions:
  decode — `default` + `Option` accept absent and explicit `null` ⇒
  `None`; encode — the PUT body carries `"startdate": null` when unset
  (Card has no skip — the full round-trip body), while `NewCard` omits
  the key when unset (server default applies, duedate's create
  behavior).
- **D16 — contract pins (tier 1, wiremock)** — §6 has the catalogue:
  the existing `create_card_posts_exact_payload_including_iso_duedate`
  doubles as the create-omission pin for free (its `NewCard` literal
  gains `startdate: None`; the exact-body assertion already proves the
  key absent); one new create test pins the `+00:00` ISO write; the
  update test pins the PUT body carrying `startdate`; one new inline
  JSON test pins explicit-`null` decode; **no harvested fixture file is
  edited** — one new modeled fixture `card_startdate.json` (the
  `card_due_null.json` shape + a `startdate` value) carries the read
  pin, and `card_detail.json` (no `startdate` key) pins absence
  tolerance via the existing detail test growing one assertion.
- **D17 — strategies** (§5): `effort_strategy` (bounded `0..=10^9`
  seconds), `importance_strategy`/`urgency_strategy` (the five variants
  via `prop_oneof`/index map), in `test_support/entities.rs`, re-exported
  in `mod.rs`; `task_clocks_strategy` 8→12 tuple; `task_strategy` four
  `option::of(...)` entries; `task_changes_strategy` four
  `option::of(option::of(...))`; `remote_task_strategy` +
  `SnapshotTaskRow` + `snapshot_strategy` gain `scheduled`.
- **D18 — insta pass = exactly the 20 state S-series snapshots**
  (`snapshots__s{1..10}_{persisted_state,published_app_state}.snap`),
  regenerated with `INSTA_FORCE_UPDATE=1`/`cargo insta accept`. Review
  rule for the accepted diff: every hunk is the four new value keys and
  four new clock keys appearing on task rows (values `null`/populated
  per scenario, clocks = whatever the scenarios stamp — epoch-styled
  defaults only where a row was adopted/persisted without a local
  write); **nothing else** — no id, ordering, reminder, outbox, or
  status churn. Web (`taskboard-app/src/web/snapshots`) and ui-slint
  snapshots must **not** change (nothing renders the fields); a change
  there is a finding, never a regen. No `.pending`/`.snap.new` ships.
- **D19 — `.sqlx` cache**: every changed `query!`/`query_as!` (tasks
  SELECT + INSERT) invalidates cache entries; regenerate with
  `cargo sqlx prepare` (a live sqlite via `DATABASE_URL` or the
  workspace's existing prepare flow) and commit the diff — CI builds
  offline; a stale cache is a red required check.
- **D20 — CHANGELOG**: one `### Added` Unreleased entry (domain fields
  + native `startdate` sync + the #244 manifest + 0007), house voice.

No deviation from an ACCEPTED ruling exists in this plan. D6 and D7's
"copies deferred" readings are the design docs' own stated positions
(impl-plan §5 interim; D-S6's fold-position ruling), recorded here as
decisions because the prompts call them out explicitly.

## 3. The exact type shapes (illustrative, not a diff)

```rust
// entities.rs — after Color, before TaskClocks.
/// Estimated effort in whole seconds; `None` on `Task` = unestimated.
/// Unsigned by construction (negative effort is unrepresentable); the
/// storage codec rejects values beyond `i64::MAX` at its boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Effort(u64);

impl Effort {
    #[must_use]
    pub const fn new(seconds: u64) -> Self { Self(seconds) }
    /// Whole seconds.
    #[must_use]
    pub const fn seconds(self) -> u64 { self.0 }
}

/// Importance level for later decision support; `None` = unset (a task
/// outside the matrix must not read as "Normal"). Declaration order =
/// severity order: the derived `Ord` is the ruled sorting contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Importance { VeryLow, Low, Normal, High, VeryHigh }

// Urgency: the identical shape, a distinct type (cross-assignment is a
// type error; the later matrix view grows them independently).

/// Which sync lane the Deck connector carries a task field on (#244).
/// The lane is the *connector's* mapping, not an intrinsic property of
/// the field — the local data model stays one construct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldFamily { Native, AppData }

/// The mergeable task-field manifest: the single source the lockstep
/// assertions pin every structure against (#244). One row per
/// `TaskClocks` slot; `.1` names the `Task` members the clock guards
/// (composites name both); `.2` is the Deck connector's lane.
pub const TASK_FIELDS: &[(&str, &[&str], FieldFamily)] = &[
    ("title",       &["title"],             FieldFamily::Native),
    ("description", &["description"],       FieldFamily::Native),
    ("duedate",     &["duedate"],           FieldFamily::Native),
    ("done",        &["done"],              FieldFamily::Native),
    ("position",    &["stack", "order"],    FieldFamily::Native),
    ("labels",      &["labels"],            FieldFamily::Native),
    ("archived",    &["archived"],          FieldFamily::Native),
    ("deleted",     &["deleted"],           FieldFamily::Native),
    ("scheduled",   &["scheduled"],         FieldFamily::Native), // D-F2 branch A
    ("effort",      &["effort"],            FieldFamily::AppData),
    ("importance",  &["importance"],        FieldFamily::AppData),
    ("urgency",     &["urgency"],           FieldFamily::AppData),
];
```

`Task` gains `scheduled` (after `duedate`), `effort`/`importance`/
`urgency` (after `labels`); `TaskClocks` gains the four clocks in the
same relative order. `TaskChanges`:

```rust
pub struct TaskChanges {
    // …existing 4 slots…
    /// New scheduled start; `Some(None)` clears it, `None` = untouched.
    #[serde(default, deserialize_with = "deserialize_double_option",
            skip_serializing_if = "Option::is_none")]
    pub scheduled: Option<Option<chrono::DateTime<chrono::Utc>>>,
    // effort: Option<Option<Effort>>, importance: Option<Option<Importance>>,
    // urgency: Option<Option<Urgency>> — the same attribute set.
    pub archived: Option<bool>, // position unchanged; order per D4
}
```

## 4. Migration 0007 (final, against the live 0001–0006 schema)

```sql
-- 0007_task_field_family: the Wave-1 domain field family (#208, #209)
-- plus the reminder-rule write clock doc 1 D-S9 reserved (chunk 4 wires
-- the domain field; until then the column sits at its default).
-- Conventions: value columns NULL = unset (0001's duedate precedent);
-- clock columns epoch-defaulted NOT NULL (0005's mechanism) so
-- pre-migration rows read as "never locally written" — every remote
-- stamp / document clock wins bit-for-bit. Additive only (ADR 0005).
ALTER TABLE tasks ADD COLUMN scheduled     TEXT NULL;    -- rfc3339, duedate's twin
ALTER TABLE tasks ADD COLUMN effort        INTEGER NULL; -- Effort seconds (u64, codec-checked ≤ i64::MAX)
ALTER TABLE tasks ADD COLUMN importance    TEXT NULL;    -- serde unit names, five levels
ALTER TABLE tasks ADD COLUMN urgency       TEXT NULL;    -- serde unit names, five levels
ALTER TABLE tasks ADD COLUMN ck_scheduled  TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_effort     TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_importance TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_urgency    TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE reminder_rules ADD COLUMN updated_at TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
```

## 5. TDD ordering (test first at every step; commits standalone)

Each numbered step is one commit-shaped unit; every test below is a
typed-outcome test (no text assertions, no sleeps; the domain needs no
runtime here at all).

1. **Domain types + manifest (red → green)**: write the failing tests
   first — `effort_roundtrips_through_serde` (transparent integer),
   enum serde unit-name round-trips for both types, the `Ord` contract
   test (`sorted_all_variants_equals_declaration_order`, both enums),
   and assertion 1 (D9 a–d; needs the widened structs to compile, so
   the skeleton lands in the same commit — the assertion is the commit's
   proof it landed whole). Then: `Effort`/`Importance`/`Urgency`,
   `FieldFamily` + `TASK_FIELDS`, `Task`/`TaskClocks` widening,
   `PendingTaskIntent` `pub(crate)` + serde derives + five bools,
   `TaskChanges` slots. Fix the crate's struct literals
   (`clocks_from`, strategies) minimally.
2. **Handler semantics (red → green)**: failing command tests first —
   set-and-stamp for all four slots; `Some(None)` clears **and** stamps
   (each new slot); untouched clocks unchanged; app-only edit → actions
   == `[UpsertTask]` (named `app_field_edits_stay_local_until_the_
   appdata_lane_lands`, citing impl-plan §5); scheduled-only edit →
   `[UpsertTask, EnqueueOp(UpdateTask)]`. Then the `UpdateTask` arm's
   four `if let` blocks + the D6 native/app enqueue split.
3. **Guard (red → green)**: failing pipeline tests — a post-`read_at`
   `UpdateTask` op protects `scheduled` (value + clock survive a merge
   that would otherwise win) via the existing guard-test pattern; then
   the `pending_task_intents` arm + `keep_task_intent` copy + debug
   fields.
4. **Merge + push + remote DTO (red → green)**: failing merge tests —
   `merge_task` scheduled LWW both directions (remote newer ⇒ adopt;
   local newer ⇒ stand; the entity-stamp basis, duedate's twin);
   `adopt_remote_task` carries `scheduled`; `adopt_task_after_push`
   takes the echo value with clock `max`. Then: `RemoteTask.scheduled`,
   the verdict, the literals, `NewCardShape`/`CardShape` +
   `from_task`.
5. **Strategies + corpus green**: extend the five strategies (D17); the
   existing round-trip proptests now cover the fields for free.
6. **Storage (red → green)**: failing tests first — `tests/schema.rs`
   (assertion 2, D10); the default-inheritance test (raw-SQL INSERT
   omitting the family columns ⇒ loads with epoch clocks);
   effort-overflow ⇒ `Err(Corrupted)`; negative/garbage TEXT in DB ⇒
   `Corrupted` decode; round-trip with populated family values. Then:
   `0007_task_field_family.sql`, `TaskRow`/SELECT/INSERT/`decode_task`,
   codec enum helpers, builder updates in `tests/` — and
   `cargo sqlx prepare` (D19) in the same commit (CI-offline honesty).
7. **Sync startdate (red → green)**: failing tests first — the D16
   contract pins (create-with-startdate exact body; create-without
   omits; update PUT carries it; explicit-`null` decode; absent-key
   decode; `map_card` carries it; `apply_card_shape`/`new_card_from`
   apply it). Then: `Card.startdate`, `NewCard.startdate`, the three
   mapping sites, the new fixture.
8. **Insta pass (the one deliberate regeneration, D18)**: run the state
   suite, accept exactly the 20 S-series snapshots, review the diff
   per the rule, ensure web/ui snapshots untouched.
9. **CHANGELOG + gate suite**: D20 line; then the full AGENTS §10 gate
   set green, including the CI-identical three-command llvm-cov
   sequence and `rm -rf target/llvm-cov-target`.

## 6. Typed-outcome test matrix (behaviour → test → layer)

| # | Behaviour | Test (layer) |
|---|---|---|
| 1 | `Effort` serde is a bare integer; accessors hold | proptest round-trip + unit (domain entities tests) |
| 2 | Enum serde unit names `very_low`…`very_high` | round-trip + explicit-name assertions via `serde_json::to_value` equality on unit values (domain) |
| 3 | Declaration order = `Ord` rank (both enums) | sort-all-variants equality test (domain) |
| 4 | Manifest ↔ structures bijection (assertion 1, D9 a–d) | `task_fields_manifest_pins_the_domain_structures` (domain entities tests) |
| 5 | `Some(Some(v))` sets value + stamps clock, per slot | `update_task_sets_and_stamps_the_field_family` (domain command tests) |
| 6 | `Some(None)` clears **and** stamps (clearing is a write) | `update_task_clearing_stamps_the_clock` (domain command tests) |
| 7 | Untouched fields keep clocks | extended touched/untouched assertions in #5's test |
| 8 | App-only edit enqueues nothing (interim, MD-10) | `app_field_edits_stay_local_until_the_appdata_lane_lands` — actions == `[UpsertTask]` |
| 9 | Scheduled-only edit enqueues `UpdateTask` (native lane) | `scheduled_edit_rides_the_native_update_op` |
| 10 | Post-`read_at` `UpdateTask` protects `scheduled` value+clock | guard test in pipeline tests (existing pattern) |
| 11 | `merge_task` scheduled LWW both directions, entity-stamp basis | merge tests, `remote_wins` twin |
| 12 | `adopt_remote_task` / `adopt_task_after_push` carry `scheduled` | merge tests (values-as-is, clock max) |
| 13 | Push shapes carry `scheduled` from the task | `from_task` unit assertions (push.rs tests / mapping-level) |
| 14 | Wire read: `startdate` decodes (value / explicit null / absent) | contract: new fixture test + inline-null test + `card_detail.json` absence assertion |
| 15 | Wire write: create posts `+00:00` ISO `startdate`; omits when `None` | contract: new create test (exact body); existing exact-body test (omission) |
| 16 | Wire write: update PUT carries `startdate` (null when unset) | extended `update_card_puts_full_round_trip_body` |
| 17 | `map_card`/`new_card_from`/`apply_card_shape` thread `scheduled`↔`startdate` | mapping unit tests (extend existing) |
| 18 | 0007 applies; columns match the manifest (assertion 2, D10) | `tests/schema.rs` PRAGMA assertions |
| 19 | INSERT without the family columns inherits epoch defaults | raw-SQL insert → load → epoch clocks |
| 20 | Effort > `i64::MAX` ⇒ typed `Corrupted` on upsert | storage apply error test |
| 21 | Negative effort / unknown level TEXT in DB ⇒ `Corrupted` decode | raw-SQL corrupt → load tests |
| 22 | Family values survive the whole-state round-trip | existing round-trip proptest (strategy-widened) + storage corpus |
| 23 | Reminder-rule clock column exists yet no codec reads/writes it | the schema test asserts the column; grep-level absence is review's job (trap list) |
| 24 | Engine corpora stay coherent after widening | 20-snapshot insta pass (D18 review rule) |

## 7. Risks & mitigations

1. **`.sqlx` cache staleness** (highest CI risk): mitigated by D19 —
   prepare runs in the storage commit; a missed entry is caught by the
   offline CI build before review ends.
2. **Struct-literal ceremony blast radius** (~20 files construct
   `TaskClocks`/`RemoteTask`/`CardShape`/`TaskChanges`/`NewCard`
   literals: state/ui-slint tests, benches, mapping, outbox, fake
   engines): all mechanical field additions; clippy `-D warnings` plus
   the exhaustive literals make a half-run ceremony a compile error —
   the compiler is the first manifest here, assertions 1–2 the second.
3. **Snapshot drift beyond the widening**: the D18 review rule bounds
   the accepted diff; web/ui snapshot changes are findings.
4. **Wire-shape assumption** (`startdate` camelCase key + `startDate`
   alias): evidenced by the roundtrip report's 26-key payload (read
   path) and pinned at tier 1 here; the *write* path's behavioral proof
   on live Deck 1.17.5 rides chunk 3's gate (DDF-10) — this chunk's
   pins are contract-level, by design.
5. **`reminder_rules.updated_at` drift into scope**: the schema test
   asserts the column's existence; any repo.rs diff touching
   `ReminderRuleRow`/`reminder_rules` SQL is stage-3 scope-creep
   material (trap list, MD-10).
6. **Effort/enum decode corrupt-path wording**: typed `Corrupted`
   outcomes only; no error-message assertions anywhere (house rule).

## 8. Estimated diff size

~28–35 files, ~1,000–1,300 changed lines:
domain (`entities/messages/pipeline/merge/push/remote/command` +
`test_support` ×4) ≈ 550–700; storage (0007 + repo + codec + tests +
`.sqlx` cache entries) ≈ 300–400; sync (model/client/mapping/contract +
1 fixture) ≈ 130–180; CHANGELOG ≈ 12; plus 20 regenerated state
snapshots (mechanical). Consistent with the plan's L rating.

## 9. Out of scope (chunk boundaries, restated)

No `LocalOp::UpdateAppData` arm, no enqueue for app fields (MD-10
interim), no document/`TaskEntry` keys (chunk 3), no
`ReminderRule.updated_at` domain field or codec wiring (chunk 4), no
CLI/UI surfaces (chunk 5), no preferences (chunk 2), no fence-comment
amendments (chunk 4 — the v1 comments stand verbatim this chunk), no
tracker edits (MD-4; stage 3 posts the PR annotations).
