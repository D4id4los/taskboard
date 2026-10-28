# [Plan] Insistence flavour fields — `Insistence` across domain, storage, sync, CLI (#475, DR-3 chunk 2) — stage-1 implementation plan

- **Date**: 2026-10-28
- **Kind**: stage-1 TDD implementation plan for chunk DR-3/2 of the
  design-rounds batch dispatch. Zero production code in this commit;
  this file is the chunk's entire planning artefact. Implementation is
  stage 2's (dispatch prompt 2.2), review + shipping stage 3's
  (prompt 2.3).
- **Origin**: design round
  `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
  — §3 Part B (D-I2–D-I4, **normative item by item**), §1 (the
  urgency-collision anchor row), §5 (the chunk-2 test hooks), §7
  (chunk 2) — dispatched by
  `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1/§2/§9.1. DR-3 round binding notes (dispatch §3.2) apply: DR-1's R3
  settlement semantics are the written authority; the kiosk banner-only
  fence; the OS-delivery/visual plumbing stays with GUI review round 3.
- **Normative base**: `main` = `7355fcb` (the worktree's branch point,
  == `origin/main` at stage-1 time). Chunk gates: DR-3/1
  (`fix/reminder-settlement-retime`) **verified merged** — every branch
  commit has a patch-equivalent in `origin/main` (the rebase-merge
  rewrote `fc924e6`→`4c0911d`, `29a216f`→`4e9b703`; checked with
  `git log --cherry-pick --left-only`). This is a **shape-widener
  (w)** chunk: wave 6's single snapshot-regen slot is this chunk's.
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§5, D-1…D-13). The one dispatch
  deviation this planning pass records is **D-1**: the migration is
  **0013**, not the dispatch's assigned 0012 — `0012_task_label_memberships.sql`
  landed on `origin/main` before this chunk's base (DR-4/L merged ahead
  of its wave slot); per dispatch §1 rule 3 and §2 the chunk takes the
  next free number at its base and records the deviation.

## 1. Mission & binding constraints

Add the insistence flavour — **`Insistence { Normal, Alarm }`**, the
per-reminder "how loud" datum — to the reminder domain's three payload
types, its storage rows, the sidecar rules section, and the CLI create
verbs. Insistence is **data only** in this chunk: the evaluator decides
*when*, never *how loud*, and nothing consumes the level behaviorally
until chunk 4 (escalation) and chunk 3 (surfaces). Per the dispatch
§9.1:

1. The enum is **named `Insistence` — never `Urgency`** (`urgency` is
   an existing synced *task* field, `entities::Urgency` +
   `MODELED_TASK_FIELD_KEYS`; a second meaning under one name in one
   crate is the recorded misdirection bug — origin §1 anchor).
2. Fields on `ReminderRule`, `AdhocAlarm`, and `Notification` (the
   fire-time snapshot, the `title` precedent), all **set at creation,
   immutable per row** — no rule-edit verbs exist; changing a level
   re-creates the rule (remove + add), the D-R1 dedup-by-offset
   semantics unchanged.
3. The **implicit default reminder stays `Normal`** (no row to carry a
   field; v1 adds none).
4. The **evaluator stays insistence-agnostic**: `due_firings` /
   `active_notifications` signatures and logic get **zero hunks**;
   only `notifications_for` copies the field into the payload.
5. **Migration 0013** (this dispatch's renumber, D-1): one migration,
   both columns (`reminder_rules`, `adhoc_alarms`), `TEXT NOT NULL
   DEFAULT 'normal'`; codec arms + roundtrips + upgrade test.
6. Sidecar `RuleEntry` additive field, **default-on-missing fold**;
   emission writes it; the mixed-fleet verbatim-relay fixture extends.
7. CLI `--insistence` flags on `tasks remind` and `timers add` (+ list
   rendering, receipts carrying the level, **typed unknown-value
   errors** per the `KeymapPreset` precedent).
8. **Non-goals (fences)**: no dismissal/delivery row changes
   (insistence never keys suppression); no config knobs (the ladder is
   per-reminder data, not `[reminders]` config); no rodio / escalation
   driver / alarm window (chunk 4); no UI surfaces or insistence
   picker (chunk 3 — the UI router's mechanical default in D-8 is a
   compile fix, not a surface); no kiosk work (banner-only fence);
   the OS-delivery plumbing bugs stay with GUI review round 3.

## 2. Verified anchors (branch point `7355fcb`; symbols are the stable keys)

| Fact | Anchor | Evidence |
|---|---|---|
| The three payload types | domain `reminders.rs` | `ReminderRule` :230 (`id, task, offset, created_at, updated_at`); `AdhocAlarm` :272 (`id, label, fire_at, created_at`); `Notification` :473 (`reminder, anchor, title, occurrence` — doc: "Title is a snapshot at fire time") |
| The payload assembler is the only copy site | domain `reminders.rs` | `notifications_for` :516–540 — Task arm resolves the title from `tasks`, Alarm arm the label from `reminders.alarms`; already takes `reminders: &ReminderState` (no signature change needed) |
| The evaluator fence | domain `reminders.rs` | `due_firings` :712, `active_notifications` :783, `effective_occurrence` :631, `occurrence_of` :620, `compact_snooze_chains` :558, `prune_settled_alarms` :598 — none read insistence; canonical orders `(occurrence, reminder)` / `(fire_at, id)` |
| The commands | domain `messages.rs` | `AddReminderRule { task, offset }` :244; `CreateAdhocAlarm { label, when }` :285 |
| The planner arms | domain `command.rs` | `AddReminderRule` :1810 (dedup-by-offset survivor :1817–1830; fresh rule stamped :1834–1840); `CreateAdhocAlarm` :1890 (alarm stamped :1908–1913) |
| The sidecar entry + its discipline | domain `appdata.rs` | `RulesEntry` :302; `RuleEntry::Rule { offset, created, clock }` :362; `deserialize_verbatim` :388 (unmodelable → `Verbatim`, never folded); emission `Serialize` :413; `rules_entry_for` :829; positive-offset demotion precedent test :1254 |
| The sidecar fold | domain `merge.rs` | `fold_reminder_rules` :697–759 — adopt (:729–739), clock-gated update (:722–727), tombstone (:742), Verbatim never folds (:755) |
| Storage rows | storage `repo.rs` | `ReminderRuleRow` :156, `AdhocAlarmRow` :165; `sqlx::query_as!` SELECTs :655/:699; INSERTs :1372/:1426 — **macros**: the `.sqlx` offline cache changes with the query text |
| The codec discipline | storage `codec.rs` | `importance_to_text/:_from_text` :22/:35 — "storage TEXT *is* the serde encoding"; unknown text → `Corrupted` |
| Migration conventions | storage `migrations/` | 0004 (`reminder_rules`), 0007 (`ALTER TABLE … ADD COLUMN … NOT NULL DEFAULT` precedent :16), 0008 (`adhoc_alarms`), 0012 (most recent house style); **next free at base: 0013** (0010/0011/0012 taken — D-1) |
| Upgrade-test precedent | storage tests | `pre_membership_rows_decode_as_fresh_adds_at_the_epoch` (roundtrip.rs :1001 — raw pre-migration-shape INSERT → load → assert the fold); corruption precedent `unknown_level_text_in_the_db_is_a_typed_corruption` (schema.rs :271) |
| The typed unknown-value precedent | app `config.rs` | `KeymapPreset` :420 + the ruling comment :276/:1707 — "an unknown enum value has no meaningful drop: it fails the decode, never defaults"; the CLI flag form is the clap value-parser (`parse_urgency` cli.rs :524 — `parse_text_form(..).ok_or_else(..)` → usage error, exit 2) |
| The CLI verbs | app `cli.rs`, `commands/` | `TimersCmd::Add` :586 (`--in`/`--at` group); `TasksCmd::Remind` :786 (`--before` + mode-flag coherence sets); `reminders.rs` `collapse`/`add_on`/`reminder_rows`; `timers.rs` `collapse`/`add_on`/`timer_rows`; receipts in `output.rs` :454–542 (Serialize structs → the `--json` envelope via `emit_result`) |
| The UI dispatcher (mechanical fix) | ui-slint `router.rs` | `AddReminderRule` :352, `CreateAdhocAlarm` :364 — variant widening breaks compile; the arms gain `Insistence::Normal` (chunk 3 owns any picker) |
| The fixtures the sidecar extension builds on | domain tests | appdata.rs rules roundtrip ~:1240; `a_positive_rule_offset_demotes_to_verbatim` :1254; the verbatim-preservation proptest :1977; merge.rs rules fixtures :2924/:3196/:3239; pipeline.rs rules fixtures :1865–:2034 |
| The snapshot suite (the (w) regen) | state tests | `snapshots__s*_persisted_state.snap` — every scene carrying rules/alarms widens (`reminders.rules`/`alarms` entries gain the field) |

## 3. The design, exactly

### 3.1 The type (D-I2)

New in domain `reminders.rs` (beside `ReminderRule`; added to the
`pub use reminders::{…}` list in `lib.rs` :116):

```rust
/// How loudly a reminder insists when it fires — per-reminder flavour
/// data stamped at creation. Naming: deliberately not `Urgency`, which
/// is an existing synced task field (`entities::Urgency`); the origin
/// round records the rename (D-I2). The evaluator never reads this —
/// it decides *when*, never *how loud*; the escalation driver (chunk 4)
/// is the behavioral consumer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
         Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Insistence {
    /// Today's behavior: one passive toast + the in-app row.
    #[default]
    Normal,
    /// The wake-up rung: the alarm window, re-toasts, and the sound
    /// ramp (chunk 4 owns the behavior; this chunk only carries the
    /// datum).
    Alarm,
}
```

plus the `parse_text_form`/`text_form` sibling pair of
`entities::Urgency` (CLI + rendering vocabulary; `"normal"`/`"alarm"`).

**Ordering decision (D-I2's either/or, pinned):** *keep declaration
order and sort explicitly.* Declaration order is the ladder's
severity-ascending order (`Normal`, then `Alarm` — the
`entities::LEVELS` discipline, `#[default]` first), so derived `Ord`
reads "higher = more insistent" (`Alarm > Normal`). **No code in this
chunk sorts by insistence**: canonical orders stay `(occurrence,
reminder)` for notifications and `(fire_at, id)` for timers. A future
insistent-first surface sort (chunks 3/4's call, D-I7) states
`Reverse(insistence)` explicitly at its sort site — ordering by
side-effect of declaration order is exactly what this pin forbids.

### 3.2 The three field placements (D-I3)

Field order is snapshot order (semantic payload before stamps, the
existing reading order):

- `ReminderRule`: `id, task, offset, insistence, created_at,
  updated_at`.
- `AdhocAlarm`: `id, label, fire_at, insistence, created_at`.
- `Notification`: `reminder, anchor, title, insistence, occurrence`
  (beside `title` — both are fire-time snapshots copied by
  `notifications_for`).

**The snapshot copy (`notifications_for`, only logic change):**
- Alarm arm: `insistence: alarm.insistence` (the row is already
  resolved for the label).
- Task arm, `ReminderRef::Rule(id)`:
  `reminders.rules.get(&rule_id).map(|r| r.insistence).unwrap_or_default()`.
- Task arm, `ReminderRef::Default(_)`: `Insistence::Normal` — D-I3's
  ruling: the implicit default has no row and stays `Normal`.

The `unwrap_or_default` leniency is documented at the call site: through
the engine the rule row exists (the evaluator just read it in the same
message); a hand-constructed firing whose rule row is missing degrades
to `Normal` instead of dropping a notification the evaluator deemed due
— the function's total-on-arbitrary-input stance (its doc already
skips unresolvable *anchors*; an unresolvable *ref row* is a different,
milder case).

### 3.3 The immutability rule (D-I3/D-5)

No rule-edit verbs exist; none are added. The only path to a different
level is **re-create**: `RemoveReminderRule` + `AddReminderRule` (the
add's dedup-by-offset applies on the re-add exactly as today).
`CancelAdhocAlarm` remains the alarm off-switch regardless of level.
Dedup stays keyed by **offset alone** — `(task, offset)` still fully
determines *firing* semantics (occurrence arithmetic is pure in
`(offset, duedate)`); insistence is flavour, not firing semantics, so
the dedup's soundness rationale stands and an add carrying a different
level than an existing same-offset rule reports the survivor
(`added: false`) without changing it. The `AddReminderRule` variant doc
gains one sentence saying so.

### 3.4 Migration 0013 (D-I4 + D-1, one migration, both columns)

`migrations/0013_reminder_insistence.sql`:

```sql
-- 0013_reminder_insistence: the insistence flavour columns (DR-3/2,
-- #475; design D-I4). Additive only (ADR 0005). Conventions as in
-- 0007/0012: stored TEXT is the domain serde unit name; unknown text
-- decodes Corrupted at the codec (no CHECK re-legislates the enum).
-- Numbering: the dispatch table assigned 0012, superseded by landing
-- order — 0012_task_label_memberships.sql landed first; 0013 is the
-- next free number at this chunk's base (dispatch §1 rule 3).
ALTER TABLE reminder_rules ADD COLUMN insistence TEXT NOT NULL DEFAULT 'normal';
ALTER TABLE adhoc_alarms   ADD COLUMN insistence TEXT NOT NULL DEFAULT 'normal';
```

**Upgrade test** (the S12/S3 raw-INSERT precedent, roundtrip.rs): raw
pre-migration-shape INSERTs — one `reminder_rules` row and one
`adhoc_alarms` row without the new column → `repo.load()` → both decode
`Insistence::Normal` ("existing rows fold `normal`", the acceptance
criterion).

### 3.5 Storage codecs

- `codec.rs`: `insistence_to_text(Insistence) -> &'static str` /
  `insistence_from_text(&str) -> Option<Insistence>` — the
  importance/urgency siblings, same one-representation discipline
  (`"normal"`/`"alarm"`; single words, so kebab-case serde and the TEXT
  form coincide).
- `repo.rs`: `ReminderRuleRow`/`AdhocAlarmRow` gain `insistence:
  String`; the two `query_as!` SELECTs and two INSERTs gain the column;
  decode maps through the codec, unknown text →
  `RepositoryError::Corrupted` (the schema.rs :271 precedent).
- **`.sqlx` offline cache**: the changed macro queries get new hashes —
  regenerate with `scripts/sqlx-prepare.sh` against a migrated scratch
  db and commit the refreshed `.sqlx/` entries (CI runs
  `SQLX_OFFLINE=true` against the committed cache).

### 3.6 The sidecar additive fold (D-I4)

- `RuleEntry::Rule { offset, created, clock, insistence }` — the
  variant gains a field.
- Decode (`deserialize_verbatim`): `insistence` is
  **default-on-missing** → `Normal` (an old document folds; the
  additive-section precedent DR-1 landed for every rules field). An
  **unknown insistence value demotes the whole entry to `Verbatim`**
  (the D-S2 materialization rule; the positive-offset precedent
  :1254): a future rung is unmodelable content — never silently folded
  down to `Normal`, preserved verbatim so a client that knows the rung
  folds it and the union relays it.
- Emission (`Serialize` + `rules_entry_for`): writes `"insistence"`
  unconditionally on a clocked `Rule` entry (fully-owned shape, like
  `offset`/`created`/`clock`).
- Fold (`fold_reminder_rules`): the `Rule` arm carries `insistence`
  into both the adopt arm and the clock-gated update arm (mechanical
  LWW). No same-clock field conflict is constructible: no edit verbs
  exist, creation stamps differ (origin D-I4's recorded note). A
  pre-round device's re-created rule carries no field → folds `Normal`
  → that *is* what its creator asked for (a device that cannot know
  levels asked for the default) — accepted, recorded, fixture-pinned.

### 3.7 The CLI surface (D-I4)

- `cli.rs`: `parse_insistence_flag(raw) -> Result<Insistence, String>`
  — `Insistence::parse_text_form(raw).ok_or_else(|| "expected
  normal|alarm".to_owned())`; the clap usage-error form (exit 2) is the
  typed-unknown-value precedent in its CLI shape (the domain parse is
  typed; the engine never sees a string; the `KeymapPreset` rule — no
  silent default at the config/decode layer either).
- `TasksCmd::Remind` gains `--insistence <normal|alarm>`:
  `#[arg(long, value_parser = parse_insistence_flag, requires = "task",
  conflicts_with_all = ["clear", "remove", "off", "on", "list"])]`
  (mirrors `--before`'s coherence set — the flag composes with the Add
  mode only). `TimersCmd::Add` gains the same flag (no conflicts — Add
  is the subcommand's only creating mode).
- `RemindInvocation::Add { task, before, insistence }` /
  `TimersInvocation::Add { label, when, insistence }`; `collapse` maps
  the flag; a `None` folds to `Insistence::Normal` at the collapse
  boundary (create-time-only flag; no "unspecified" state is stored).
- Receipts carry the level, **read from post-state** (the
  `fire_at`/`task_title` read-back precedent — never an input echo):
  `ReminderAddReceipt.insistence` (so a dedup-hit receipt reports the
  *survivor's* level), `TimerAddReceipt.insistence`, and the list rows
  `ReminderRow.insistence` / `TimerRow.insistence`. `Serialize`
  derives put the field into the `--json` envelope automatically.
- Renderers: an ` [alarm]` suffix when the level is `Alarm`, nothing
  for `Normal` (the ` [muted]`/` [overdue]` marker precedent; the
  quiet default renders quiet). Tests assert the typed structs, never
  the rendered text (AGENTS §5).

### 3.8 Mechanical widening fall-out (compile fixes, not surfaces)

- ui-slint `router.rs` :352/:364 arms pass `Insistence::Normal` — the
  picker is chunk 3's surface (D-I7); the UI stays a dumb client.
- ui-slint `tests/journeys.rs` rule/alarm fixture literals gain the
  field; `AddReminderRule` command assertions gain it.
- domain `test_support/messages.rs` command fixture gains the field.
- state `tests/snapshots.rs` fixture literals gain the field; the
  s-series `.snap` files regenerate (one deliberate `cargo insta
  accept` pass — dispatch prompt 2.2 step 3).
- storage/domain/pipeline test literals constructing the widened
  structs gain the field.

## 4. Recorded decisions (every open question resolved)

| ID | Question | Decision |
|---|---|---|
| D-1 | Migration number | **0013**, recorded deviation from the dispatch's 0012 — `0012_task_label_memberships.sql` landed on `origin/main` before this chunk's base (DR-4/L merged ahead of its wave slot); dispatch §1 rule 3/§2 rule "next free at the base, record the deviation". Stage 3 re-verifies at rebase; if another migration lands in between, rename (filename + test refs only — the migration is unpushed until stage 3) |
| D-2 | Ordering (D-I2's either/or) | Keep declaration order, sort explicitly: `Normal` declared first (`#[default]`), `Alarm` second; derived `Ord` (higher = more insistent); **no sort site in this chunk orders by insistence** — canonical orders unchanged; a future insistent-first sort writes `Reverse(insistence)` at its site |
| D-3 | Where the enum lives | `reminders.rs` beside `ReminderRule`; re-exported in `lib.rs` :116; `parse_text_form`/`text_form` siblings of `entities::Urgency` |
| D-4 | Snapshot copy semantics | `Rule` ref → the rule row's insistence (`unwrap_or_default`, documented leniency: an evaluator-consistent firing's row exists; a hand-built firing with a missing row degrades `Normal` rather than dropping a due notification); `Default` ref → `Normal` (D-I3); `Alarm` anchor → the alarm row's |
| D-5 | Dedup vs insistence | Dedup stays keyed by offset alone; a differing-level add reports the survivor `added: false`; the level changes only via remove→re-add (re-create semantics, D-I3). The variant doc states the rationale extension: `(task, offset)` determines *firing* semantics; insistence is flavour |
| D-6 | Unknown sidecar insistence value | The whole entry demotes to `Verbatim` (D-S2 / positive-offset precedent) — never a silent `Normal` downgrade; verbatim relay preserves it for a rung-aware client |
| D-7 | Missing sidecar `insistence` key | Default-on-missing fold → `Normal` (the origin's explicit ruling; the DR-1 additive precedent) |
| D-8 | UI router widening | Mechanical `Insistence::Normal` at the two dispatch arms; the picker is chunk 3's (D-I7); no UI surface, no badge, no rendering in this chunk |
| D-9 | Receipt provenance | Receipt/list levels read from **post-state** (the read-back precedent), so a dedup-hit receipt carries the survivor's level, never the requested echo |
| D-10 | CLI unknown value | Typed at every layer: domain `parse_text_form → Option`, clap value-parser → usage error (exit 2) before any engine contact; no stringly failure anywhere; no config knob |
| D-11 | Field positions | Semantic payload before stamps: `ReminderRule` after `offset`; `AdhocAlarm` after `fire_at`; `Notification` after `title` (the snapshot pair). Snapshot regen is wholesale either way |
| D-12 | `.sqlx` cache | The changed `query_as!` SELECT/INSERT hashes regenerate via `scripts/sqlx-prepare.sh` and land in the implementation commit (CI is `SQLX_OFFLINE=true`) |
| D-13 | `notifications_for` signature | Unchanged — it already takes `&ReminderState`; the copy is internal. The evaluator fence (§1 item 4) is diff-proven by stage 3 |

## 5. Test plan (TDD-first; milestones are plan structure — stage 2 lands red-green pairs as standalone commits)

Every test: typed/structural assertions only (AGENTS §5 — no message
text, no label substrings); deterministic (fixed instants, injected
clocks where the seam exists; no sleeps); one behavior per test at the
lowest layer that owns it.

**M1 — domain type & serde** (`reminders.rs` tests + entities-style
text-form tests)

- T1 `insistence_serde_roundtrips_and_rejects_unknown`: both variants
  roundtrip through `serde_json`; `from_str::<Insistence>("\"critical\"")`
  is a typed `Err` (structural `is_err()`/`matches!`, no text match).
- T2 `insistence_text_form_roundtrips`: `parse_text_form`/`text_form`
  both variants + rejection (`None`) — the `Effort`/`Urgency`
  text-form test siblings.
- T3 `notifications_for` snapshot arms: a `Rule`-ref firing carries the
  rule's `Alarm`; an `Adhoc` firing carries the alarm's; a `Default`
  firing carries `Normal`; a hand-built `Rule`-ref firing with no rule
  row degrades `Normal` (D-4's leniency pinned as contract). Extends
  the existing `notifications_for` tests (reminders.rs :1833 area).
- T4 default-derives: `Insistence::default() == Normal`;
  declaration-order `Ord` (`Normal < Alarm`) pinned once — the D-2
  contract future sort sites lean on.

**M2 — command surface & planner** (`command.rs` tests, the :5958+
siblings — lowest layer that owns stamping)

- T5 `add_rule_stamps_insistence`: `AddReminderRule { insistence:
  Alarm }` plans a rule row carrying `Alarm`.
- T6 `create_alarm_stamps_insistence`: `CreateAdhocAlarm { insistence:
  Alarm }` plans an alarm row carrying `Alarm`.
- T7 `dedup_ignores_insistence_and_names_the_survivor`: existing
  `Normal` rule at offset O; add `{ O, Alarm }` → `added: false`,
  survivor unchanged (`Normal`); remove + re-add `{ O, Alarm }` → fresh
  row `Alarm`. Pins D-5 (the re-create semantics) — this is the
  "immutability" contract test.
- T8 snapshot regen (the (w) pass): s-series scenes carrying
  rules/alarms widen; one deliberate `cargo insta accept` after M1–M3
  land.

**M3 — storage** (`roundtrip.rs` / `schema.rs` / codec tests)

- T9 `pre_insistence_rows_decode_normal` (the upgrade test, §3.4): raw
  pre-0013-shape INSERTs into both tables → load → `Normal` (the S12
  raw-INSERT precedent).
- T10 `insistence_round_trips_through_sqlite`: rule + alarm with
  `Alarm` survive apply/load identically (column = serde name).
- T11 `unknown_insistence_text_is_a_typed_corruption`: corrupt the
  `reminder_rules` column to `'wake_up'` → `RepositoryError::Corrupted`
  (schema.rs :271 sibling); the `adhoc_alarms` decode arm shares the
  codec, covered at codec level.
- T12 codec unit tests: `insistence_to_text/:_from_text` both variants
  + unknown → `None` (the codec module's existing layout).

**M4 — sidecar** (`appdata.rs` / `merge.rs` / `pipeline.rs` tests)

- T13 `rule_entry_roundtrips_insistence`: a `Rule` entry with
  `insistence: "alarm"` roundtrips; emission includes the key
  (extends the ~:1240 roundtrip).
- T14 `a_missing_insistence_key_folds_normal`: old-shape JSON (the
  exact pre-round shape: `offset`/`created`/`clock` only) → decodes
  `Rule { insistence: Normal }` (D-7).
- T15 `an_unknown_insistence_value_demotes_to_verbatim`:
  `insistence: "critical"` → `Verbatim(_)` (the :1254 sibling; D-6).
- T16 `fold_carries_insistence`: adopt arms an unknown uuid with the
  document's level; a clock-newer entry overwrites a local row's level;
  clock-older leaves it (merge.rs :3196/:3239 siblings).
- T17 `mixed_fleet_old_shape_folds_and_relays` (the origin §5
  mixed-fleet fixture extension): a section holding one old-shape rule
  entry + one unknown sibling key → folds `Normal` locally AND the
  union re-emits the unknown key verbatim (extends the existing
  fixture; the verbatim-preservation proptest :1977 stays green
  untouched).
- T18 pipeline rules-fold fixtures gain the field where they construct
  `RuleEntry::Rule` (:1865–:2034) — wiring stays pinned by the existing
  scenes.

**M5 — CLI** (`cli.rs` unit + `tests/cli_verbs.rs`)

- T19 `parse_insistence_flag_matches_the_domain_vocabulary`: accepts
  `normal`/`alarm`, rejects anything else (`is_err()`; the
  `parse_before_flag` test precedent).
- T20 receipts carry the level: `tasks remind <t> --before 5m
  --insistence alarm` → `ReminderAddReceipt.insistence == Alarm`;
  `timers add "tea" --in 2m --insistence alarm` →
  `TimerAddReceipt.insistence == Alarm`; omitted flag → `Normal`
  (typed struct assertions via the existing app-test harness).
- T21 list rows carry the level: `--list`/`timers list` rows expose
  the typed field.
- T22 dedup-hit receipt reports the survivor's level (D-9): add
  normal, then add alarm at the same offset → receipt
  `added == false ∧ insistence == Normal`.
- T23 `--json` envelope: the field appears in the serialized receipt
  (structural JSON key/value assertion — the machine envelope *is* the
  contract there, the existing JSON tests' form).

**Gates**: full AGENTS §10 suite via `scripts/run-quality-gates.sh`;
`cargo insta` clean after the single deliberate accept;
`scripts/sqlx-prepare.sh` refreshed (D-12); no clippy warnings; doc
tests green.

## 6. Risks & mitigations

- **Migration collision in flight** (D-1): DR-5/2 holds the dispatch's
  0013 slot and could land first. Mitigation: the recorded re-verify at
  stage-3 rebase; renaming an unmerged migration is filename + test
  refs. The plan's T9/T10 reference "the insistence migration" by
  behavior, not number, beyond the file name.
- **Snapshot-regen breadth** (the (w) lane): every rule/alarm-carrying
  scene widens. Mitigation: one deliberate accept pass after M1–M3;
  wave 6 guarantees no sibling (w) chunk in flight; a mid-flight
  sibling merge resolves by re-rebasing + regenerating.
- **Dedup-vs-insistence UX wrinkle**: a user adding `--insistence
  alarm` over an existing same-offset `normal` rule gets a quiet
  survivor (`added: false`). Mitigated: the receipt carries the
  survivor's level (D-9) and `--list` renders it; the louder warning
  is an out-of-scope candidate improvement (the round's issue trail
  owns it — stage 2/3 do not open issues; flagged here for the user).
- **Mixed-fleet semantics**: a pre-round device's re-created rule folds
  `Normal` on post-round devices — recorded as correct (the creating
  device could not express a level), fixture-pinned by T17.
- **Evaluator-fence drift**: the chunk's most tempting slip is
  "just one early-sort by insistence". Mitigation: D-2/D-13 + the
  stage-3 diff proof; `due_firings`/`active_notifications` must show
  zero hunks.
- **`.sqlx` staleness**: a forgotten prepare regen fails CI
  (`SQLX_OFFLINE=true` against the committed cache) — the gate sequence
  catches it locally before push.

## 7. Estimated diff size

| Area | Production | Tests |
|---|---|---|
| domain `reminders.rs` (enum, fields, copy, text forms) | ~90 | ~130 |
| domain `messages.rs` + `command.rs` + `lib.rs` | ~35 | ~90 |
| domain `appdata.rs` + `merge.rs` + `pipeline.rs` (sidecar) | ~45 | ~160 |
| storage: migration, `codec.rs`, `repo.rs`, `.sqlx` | ~90 | ~170 |
| app: `cli.rs`, `reminders.rs`, `timers.rs`, `output.rs` | ~120 | ~180 |
| ui-slint (router + fixture literals, mechanical) | ~10 | ~40 |
| state fixtures | ~10 | ~30 |
| insta `.snap` regen | — | churn in the rule/alarm-carrying scenes |

**Total: roughly 400 lines of production code and 800 of tests plus
snapshot churn — an M chunk** (matches the dispatch's size column).
Commit shape: TDD pairs as standalone commits (M1→M5), each green;
`docs(plan)` this commit; no pushes.

## 8. References

- Origin: `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
  §3 Part B (D-I2–D-I4), §1 (urgency-collision anchor), §5 (test
  hooks), §7 (chunk 2 = #475).
- Dispatch: `.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (wave 6, the (w) lane), §2 (the migration ladder + the D-1
  supersession rule), §9.1 (this chunk's prompts + acceptance).
- Authority chain: DR-1 R3 semantics
  (`2026-10-25-reminder-rule-sync-design-round-plan.md` §4-R3) — the
  settlement model this chunk must not disturb; the sidecar lane's
  shape discipline lives in the DR-1 stage-2 doc's D-1/D-4/D-9.
- Sibling chunk: `2026-10-26-reminder-settlement-retime-plan.md`
  (DR-3/1, merged) — the house stage-1 style this plan follows.
- Issue: #475 (`backlog: DR-3 chunk 2 — insistence flavour fields`) —
  stage 3 closes it with the PR.
