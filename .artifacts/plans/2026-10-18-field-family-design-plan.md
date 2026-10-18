# [Plan] Daily-Driver Wave — Field Family + Preferences Design (round doc 2 of 5)

- **Date**: 2026-10-18
- **Kind**: design doc for the daily-driver design round, question 2
  (framing plan §5.2). Designs the Wave-1 domain field family —
  `scheduled` (start-vs-deadline split), `effort`, `importance`,
  `urgency` — plus the #244 per-field lockstep seam, the additive
  storage migration, the CLI/UI surface growth, the native-`startdate`
  ruling, and the `UserPreferences` home (day-start-time semantics,
  device-local until D5). Each point carries re-verified anchors,
  exactly one recommendation, and a decision-table row; §5 is the
  decision-table row; §5 is the decision record, including the
  user's session record of 2026-10-18 (framing D-D6's process
  completed).
- **Origin**: framing plan
  `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`
  (§5 Q2 normative; §2 scope incl. #208, #209, #244, #345). Normative
  inputs: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  (§3.1 field shapes and fit; §3.4 day-start-time; §4.1 D1 — dedicated
  fields for semantics, labels only for taxonomy, the binding bias),
  `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`
  (§3.1 wire evidence; §6 per-field verdicts incl. the
  evaluate-native-`startdate`-first instruction; §5 merge-policy
  BINDING via doc 1), the audit findings **F3-05** (crate-domain
  report) and **F16-01/F16-02** (arch-fit report) behind issue #244,
  and the accepted multi-board design
  (`.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`)
  with its implementation plan's design facts DF-1…DF-10
  (`2026-10-18-multi-board-wave-implementation-plan.md` §2).
- **Round context**: doc 1 of 5 —
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
  (commit `2b1a657`, recommendations ACCEPTED 2026-10-18) — is the
  sidecar this family rides; its §7 explicitly assigns this doc the
  field shapes, the manifest test (naming `PendingTaskIntent`'s
  app-field flags and the `UpdateAppData` enqueue sites), the storage
  columns (0007+), and the native-`startdate` ruling. This doc
  consumes doc 1's D-S2 document schema (`TaskEntry` field keys,
  `{v, clock}` shape, `null` = cleared, no tombstones in `tasks`),
  D-S5's op, and D-S6's guard widening as settled interfaces.
- **Status**: recommendations **ACCEPTED** — the user's decision
  session (2026-10-18) accepted all of §5's recommendations, with one
  amendment (D-F1: importance/urgency ruled to **five** levels; enums
  retained for exhaustive matching; ordering comparability required),
  one rationale addition (D-F3: the local state and data model is a
  single construct independent of what sync connectors implement
  internally), and one ordered tracker edit (a GUI settings editor,
  recorded as backlog issue #351, §7). Record in §5. Zero production
  code beyond that tracker edit — this file is the chunk's entire
  diff.

---

## 1. Mission & binding constraints

Land the Wave-1 field family so that every field is typed, per-field
merged, day-one synced, and dumb (no derived semantics in-wave), with
the #244 seam making the multi-site ceremony impossible to half-run —
and land the day-start-time preference its dependents (today view, doc
4) consume, in the home that survives until D5 without over-building.

Binding, not re-arguable:

- **D1 bias** (future-directions §4.1, accepted): dedicated fields for
  anything with semantics (dates, scores, kinds); labels only for
  free-form taxonomy. The fields land as struct fields with clocks,
  not labels.
- **Fields stay dumb** (§3.1 rows; framing §5.2): derived priority,
  the importance×urgency matrix *view*, effort-fit scoring are
  decision support — out of this wave. This wave ships values,
  clocks, storage, sync, and list decorations only.
- **Doc 1 interfaces are settled**: the app-data document's
  `TaskEntry` carries per-field `{v, clock}` entries keyed
  `effort`/`importance`/`urgency`-style strings; `null` = cleared;
  the tasks section has no tombstones (D-S2); the op is
  `LocalOp::UpdateAppData(TaskId)` (D-S5); the guard widens with one
  bool per app field plus `reminder_rules` (D-S6); the fold is
  embedded-clock LWW, clock = `max`, absence never deletes (D-S3).
- **Roundtrip §5 discipline** for any *native* mapping: per-field LWW
  on Deck's entity stamp via `merge_task`'s existing verdict shape;
  the self-clobber guard covers it through the existing
  `PendingTaskIntent` mechanism.
- **Multi-board world**: designs target the post-multi-board code
  (per-board `SyncStates`, `ConnectorId`, derived board sets); the
  implementation round is sequenced after those chunks land (framing
  §7). DF-1's vocabulary facts bind: task-scoped commands stay
  id-addressed, outbox ops resolve the owning board through the
  entity graph.
- **Storage**: additive migrations only, numbered from **0007**
  (0006 is multi-board's, in flight — doc 1 §2); the epoch-default
  clock-column mechanism (0005) is the precedent for new clocks.
- **TDD** (AGENTS §5): the fields land test-first — `task_strategy`
  round-trips, `TaskChanges` serde, merge/fold verdicts, and the
  manifest test of §3.3 all land with the code, never after.

## 2. Verified baseline (symbols are the stable keys; lines are this chunk's)

Worktree base: branch `docs/daily-driver-design-round` @ `2b1a657`
(= `main` `b809809` + doc 1; the delta from the framing's `070885c`
is docs-only). Locating method: `grep -n` for the symbol, then read
the body. Every line below was re-read for this doc.

| Seam | Symbol (file) | Lines | Notes |
|---|---|---|---|
| Task shape | `Task` (domain `entities.rs`) | 163–192 | 14 fields; `duedate: Option<DateTime<Utc>>` :174 is the mirror target; `labels` whole-set :182; `clocks` :188 |
| Eight clocks | `TaskClocks` (domain `entities.rs`) | 49–66 | title/description/duedate/done/position/labels/archived/deleted — one `DateTime<Utc>` per mergeable field; composite position documented :58, whole-set labels :61 |
| Epoch clock | `BoardClocks::epoch` (domain `entities.rs`) | 106–114 | "never locally written" sentinel; storage twin in 0005 |
| Sparse edit | `TaskChanges` (domain `messages.rs`) | 208–225 | 4 slots today (title/description/duedate/archived); `duedate` is the double-option `Option<Option<DateTime<Utc>>>` :217–222; `deserialize_double_option` :229–236 is generic and reusable |
| Edit command | `StateCommand::EditTask { task, changes }` (domain `messages.rs`) | 30–35 | the one sparse-edit verb; untouched fields keep clocks (doc :206–207) |
| Guard struct | `PendingTaskIntent` (domain `pipeline.rs`) | 90–101 | eight bools, one per clock, private; `#[allow(clippy::struct_excessive_bools)]` documents the one-flag-per-field intent |
| Guard build | `pending_task_intents` (domain `pipeline.rs`) | 104–152 | op→flag map keyed `op.queued_at > read_at` :110; `UpdateTask` sets title/description/duedate/done/archived :131–138 (position/labels/deleted have dedicated ops) |
| Guard apply | `keep_task_intent` (domain `pipeline.rs`) | 158–206 | copies protected **values and their clocks** back, field by field :173–205 |
| Native merge | `merge_task` (domain `merge.rs`) | 170–262 | per-field `remote_wins(ts_r, local.clocks.X)` (helper :71) — the verdict template a native `scheduled` copies |
| Push shapes | `NewCardShape` / `CardShape` + `from_task` (domain `push.rs`) | 143–159 / 164–179 / 182–207 | the full-send card write fields (domain-side); `duedate` rides both :151/:170/:187/:200 — a native `scheduled` extends exactly these |
| Remote DTO | `RemoteTask.duedate` (sync `model.rs`) | 123 | `startdate` appears **nowhere** in any crate (repo-wide grep, zero matches) — the native slot is unmapped end-to-end (framing §3's own verification, re-confirmed) |
| Wire evidence | Deck card payload key list (roundtrip §3.1) | :103 | 26 keys incl. `startdate` — "free native slot, see §6" |
| Storage | `tasks` table (storage `0001_init.sql`) | 40–62 | value columns NULL-able (`duedate TEXT NULL` :48); clock columns NOT NULL (`ck_duedate` :55) |
| Epoch default | 0005 (storage `0005_board_edit_verbs.sql`) | 8–10 | `ADD COLUMN ck_* TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00'` — "pre-migration rows keep the remote-wins merge behavior bit-for-bit" |
| Row codec | row struct `ck_duedate` / `decode_task` / upsert / batch (storage `repo.rs`) | 84 / 236 / 864 / 423+673+702 | the per-column ceremony: row field, decode, INSERT/upsert, `load_inner`/`apply_inner`/`execute_batch` |
| Op tags | `op_kind_to_text` (storage `codec.rs`) | 41 | additive-tag discipline (`update_app_data` lands with doc 1's chunk 3) |
| CLI | `TasksCmd::Edit` flags (app `cli.rs`) | 384–398 | `--title/--description/--due/--archive/--unarchive`; `DueSpec` :267 + `parse_due` :276 (RFC-3339, bare date → midnight UTC, `none` clears) is the hand-rolled parser precedent |
| CLI render | `TaskRow` (app `commands/output.rs`) | 89–108 | the list-row projection incl. `due_marker` — where decorations surface |
| REPL | command handlers (app `commands/tasks.rs`) | 398 / 412 | `edit` / `edit_on` — one handler path feeds both frontends |
| Config | `AppConfig` + sections (app `config.rs`) | 63, 97–169+ | `[app]`/`[nextcloud]`/`[storage]`/`[sync]`/`[ui]` (+keymap)/`[reminders]`; typed validation precedent: `UI_SCALE_MIN/MAX` enforced, never silently clamped (:49–53) |
| UI row model | `push` (ui `models.rs`) | 101, 250, 330 | builds slint-facing rows; `due_phrase` wiring at :250/:330 — the decoration seam |
| UI classification | `DueKind`/`due_sets` (ui `presentation.rs`) | 23–51 | presentation constants + set-membership over **domain** functions (`overdue_tasks`/`due_soon`, domain `reminders.rs` :420/:431) — semantics live in domain, mapping in UI |
| Time seam | `Clock` (domain `clock.rs`) | 11, 16–19 | "Nothing in this crate calls `Utc::now()` directly" — time is data; the UTC-instant end-to-end verdict is the time/DST audit's headline |
| AppState | `AppState` (domain `state.rs`) | 24–40 | boards/stacks/tasks/labels/reminders/sync — no preferences home exists |
| Proptest | `task_strategy` round-trip (domain `entities.rs`) | 246–254 | the strategy extension covers every new field for free |

F16-01's ceremony count — "≥10 named sites in 5 crates" per field —
enumerates against this baseline in §3.3's table.

## 3. The design, point by point

### 3.1 D-F1 — Field shapes (the dumb values)

**Anchors.** `Task`/`TaskClocks` (entities.rs :163/:49); `duedate`'s
full stack as the mirror (entities :174, messages :217–222 double
option, merge :192–196 verdict, 0001 :48/:55 columns, cli :267/:395,
output :98, models :330); future-directions §3.1 rows (scheduled "M",
effort "S–M", importance/urgency "S–M + UI, keep the fields dumb");
doc 1 D-S2's `TaskEntry` (value = "the domain field's serde encoding
— shapes owned by doc 2").

**Decision D-F1 (shapes).** Four fields, one clock each, mirroring
`duedate`'s stack exactly where the mirror applies:

```rust
// entities.rs — illustration, not a diff.
pub struct Task {
    // …existing fields…
    /// Scheduled start; `None` = unscheduled. The "when do I intend
    /// to begin" half of the deadline split (`duedate` is "when it
    /// must be finished").
    pub scheduled: Option<DateTime<Utc>>,
    /// Estimated effort; `None` = unestimated.
    pub effort: Option<Effort>,
    /// Importance for later decision support; `None` = unset.
    pub importance: Option<Importance>,
    /// Urgency for later decision support; `None` = unset.
    pub urgency: Option<Urgency>,
}

pub struct TaskClocks {
    // …eight existing clocks…
    pub scheduled: DateTime<Utc>,
    pub effort: DateTime<Utc>,
    pub importance: DateTime<Utc>,
    pub urgency: DateTime<Utc>,
}
```

- **`scheduled: Option<DateTime<Utc>>`** — the exact `duedate` shape
  (future-directions §3.1: "New `scheduled: Option<DateTime<Utc>>` +
  clock, mirrors `duedate` exactly (sparse changeset, merge policy,
  storage column, outbox)"). No timezone attachment: domain is
  UTC-instant end-to-end (time/DST audit headline); "which local
  day" is a derivation (§3.6).
- **`Effort`** — a newtype over **u64 seconds** (`Effort(u64)`,
  `#[serde(transparent)]`, next to `Color`'s wrapper precedent in
  entities.rs). Rationale: Deck has no effort slot so there is no
  wire constraint; seconds-as-integer is the canonical unit with
  human units parsed at the CLI edge (§3.5); unsigned-by-construction
  rejects negative effort at the type level (a `chrono::TimeDelta`
  would admit negatives and drag nanosecond precision nobody asked
  for); the document encoding is the plain integer doc 1's
  illustration already drew (`"effort": { "v": 900, "clock": … }`).
  One range note: the storage codec writes it into a SQLite INTEGER
  (i64) — the codec validates `≤ i64::MAX` and errors typed
  otherwise; no artificial cap on the domain type itself.
- **`Importance` / `Urgency`** — two distinct **five-level** enums
  (`VeryLow | Low | Normal | High | VeryHigh`, serde unit variants
  `"very_low"/"low"/"normal"/"high"/"very_high"`), **amended from the
  drafted three levels by the session (2026-10-18)**. Two types, not
  one reused: cross-assignment becomes a type error and the later
  matrix view can grow them independently. `Option<…>` wrapper:
  `None` = unset — a task not participating in the matrix must not
  render as "Normal" (the same presence distinction
  `duedate: Option<…>` makes). Enums, not scores (the session's
  explicit choice): named levels carry vocabulary and exhaustive
  matches; a numeric score carries range validation and "what does 4
  mean". **Ordering is part of the contract (ruled)**: both enums
  derive `PartialOrd`/`Ord` with declaration order = severity order,
  so sorting tasks by importance or urgency is a plain sort key on
  the enum (derived field-less `Ord` compares discriminants — the
  variant order *is* the rank); a proptest pins that ordering a mixed
  collection by the enum is monotonic in the variant index. Five is
  now the ruled granularity; re-leveling beyond five is decision
  support's re-modeling decision (Wave 2–3), not this wave's.
- **Out of wave, recorded**: derived priority, matrix quadrants,
  effort-fit scoring, calibration loops (framing §2 out-list). No
  `TaskKind`/respite/recurrence fields land (doc 1 D-S2 reserves the
  document key space; nothing serializes them yet).
- **`TaskChanges` grows four sparse slots** — all four clearable, so
  all four use the double-option shape with the existing generic
  `deserialize_double_option` (messages.rs :229):

```rust
pub struct TaskChanges {
    // …existing slots…
    pub scheduled: Option<Option<DateTime<Utc>>>,
    pub effort: Option<Option<Effort>>,
    pub importance: Option<Option<Importance>>,
    pub urgency: Option<Option<Urgency>>,
}
```

  The handler rule is the struct's own doc contract (messages.rs
  :206–207): stamp the matching field clock only for `Some` entries;
  a `Some(None)` clear writes the value `None` *and* stamps the clock
  (a cleared field is a write, exactly like doc 1 D-S2's
  `{v: null, clock}` document semantics).
- **Serde/strategy consequences**: `test_support::task_strategy` and
  `task_changes_strategy` grow the fields (entities' round-trip
  proptest :250 and messages' :339 then cover them for free);
  `Effort`/`Importance`/`Urgency` get proptest strategies (bounded
  seconds; the three enum values exhausted).

**DF conformance**: pure data in the domain crate, serde/chrono only;
no vocabulary change (`EditTask` widens its payload, stays
id-addressed — DF-1's "task-scoped commands stay id-addressed").
**Topology untouched.**

### 3.2 D-F2 — THE native-`startdate` ruling (open for the session)

**Anchors.** Roundtrip §4's consequence ("Deck cards carry a native
`startdate` … a free, LWW-merging slot the field-family plan should
evaluate for scheduled-start *before* defaulting it into the sidecar")
and §6's verdict row (same instruction; "Either way: day one, no
migration"); the wire fact itself (roundtrip §3.1 :103 — `startdate`
in the 26-key card payload; `RemoteTask` maps `duedate` at model.rs
:123 and `startdate` maps to nothing anywhere in the repo); F16-02's
cost table (arch-fit :159–173): native = the F16-01 ceremony + one
DTO field + one `merge_task` verdict line; sidecar = the ceremony +
the document machinery (second evidence input, fold step, push-op
variant, flush-order insertion, anchor ceremony, format-version —
i.e. everything doc 1 just designed); doc 1 §7 ("a native ruling
simply means no sidecar key for it"); doc 1 §4's live-tier risk
(Deck 1.17.5 flush reliability unknown until the probe gate — the
sidecar is *expected-degraded* there); framing §5.4 (the today view
is *driven by* the scheduled field).

**Recommendation D-F2 (exactly one): map Deck's native `startdate`
for scheduled-start — branch A.** Concretely: `RemoteTask` gains
`startdate: Option<DateTime<Utc>>` (model.rs, duedate's twin);
`merge_task` gains the verdict pair
(`remote_wins(ts_r, local.clocks.scheduled)` → take
`remote.startdate`, stamp `ts_r` — merge.rs :192–196 copied);
`NewCardShape`/`CardShape` gain `scheduled` and their `from_task`
sites populate it (push.rs :151/:170/:187/:200); the sync client's
card write encoding carries it; the contract fixtures pin the wire
key. The document's `tasks` section **never gains a `scheduled` key**
(doc 1 §7's anticipated consequence).

Why native wins, in order of weight:

1. **Cross-client coherence.** Deck's own card UI renders
   `startdate`; a household running taskboard *and* Deck's web/mobile
   UI sees and edits one truth. Under the sidecar branch, our
   scheduled-start is invisible in Deck, and — worse — a Deck user
   editing the *native* startdate of a card gets silently ignored by
   us: two truths for the same concept on the same card. The field
   family's whole point (D1) was *semantics*; a semantic only Deck's
   UI can't see is half-delivered.
2. **Live-tier resilience.** Doc 1 §4 records that sidecar *flush* on
   Deck 1.17.5 is unknown until the dockerized probe gate passes —
   expected-degraded. Scheduled-start is the wave's most
   consumer-critical value (the today view, framing §5.4; "plan for
   next day", future-directions §3.4). Native mapping syncs it day
   one through the path that has been carrying `duedate` since v0.1.0,
   independent of sidecar health.
3. **It is the cheaper branch** (F16-02's own arithmetic): ceremony +
   DTO field + one verdict + push materialize. The sidecar branch's
   extra machinery exists anyway for the other three fields — but
   scheduled would still pay a document key, fold/guard entries, and
   TaskEntry codec surface it does not need.
4. **Semantics match.** Deck's `startdate` is the card timeframe's
   start — the same "when work on this begins" our split wants
   (future-directions §3.1). No meaning is being bent to fit a slot.

Trade-offs, recorded against the recommendation:

- **Coarser merge basis**: native LWW compares against the card's
  entity stamp, which *any* card write advances — the document's
  embedded clock is field-private. Consequence: a Deck-side title
  edit advances the stamp and can make the (unchanged) remote
  `startdate` win over a concurrent older local scheduled write…
  except the value adopted is the one Deck already holds, so the
  observable loss window is exactly `duedate`'s today — the proven
  discipline, not a new hazard. The self-clobber side is guarded by
  `PendingTaskIntent.scheduled` (§3.3).
- **A second sync discipline in the family**: three fields fold on
  embedded clocks, one merges on the entity stamp. Cost is one more
  row in the manifest (§3.3) and the two lists the companion tests
  pin. The alternative — uniformity for its own sake — buys nothing
  the user can feel and costs interop + resilience (points 1–2).
- **Wire caution, gated**: the payload key is evidenced on reads
  (roundtrip §3.1); the *write* path (create/update accepting
  `startdate`) is covered by extending doc 1's live-tier
  contract-fixture gate — the probe already pins card create/update
  basics; one assertion per direction is added there. Deck's ISO-8601
  null-tolerant decode mirrors `duedate`'s treatment (model.rs :7
  module doc).
- **Deck-side edits may normalize the value**: Deck's own card UI
   is expected to write date-granularity values (its picker exposes
   dates; not re-verified for this doc — the roundtrip measured API
   shapes, not the web UI), which would truncate a datetime to
   midnight on a Deck-side edit. Taskboard-side writes keep full
   precision; a Deck-side edit is a deliberate user write and wins
   by LWW as it should. Recorded, not designed around — the field's
   sync contract is "last writer, Deck's normalization adopted",
   identical to `duedate`'s seconds-resolution normalization story
   (merge.rs :284–289 R9a doc).

**Ruled (2026-10-18): ACCEPTED — branch A.** Deck's native
`startdate` maps scheduled-start; the document's `tasks` section never
gains a `scheduled` key. The doc's downstream sections stand as
written (branch-A-native throughout); the inlined branch-B deltas in
§3.3/§3.5 are retained for provenance only — they are no longer
live re-scope instructions.

**DF conformance**: DTO + push-shape widening inside the existing
per-board cycle; `UpdateTask`-class op (already board-resolving per
DF-1). **Topology untouched.**

### 3.3 D-F3 — The #244 lockstep seam (field-manifest test)

**Anchors.** F3-05 (crate-domain report :566): three parallel
structures must grow in lockstep, "nothing marks the seam"; F16-01
(arch-fit :143–158): ≥10 named sites in 5 crates per field; the
compiler forces enum arms but nothing forces struct members to agree
— "a half-landed field fails silently (defaulted clock / false guard
/ absent slot)"; the finding's proposed action is exactly this
design's brief ("names the app-fields home [D-F1: extend
`Task`/`TaskClocks` — settled] and, if field-per-field, adds one
compile-or-test lockstep seam (a field-manifest test asserting the
structures agree)"); doc 1 §7 extends the brief ("must now also name
`PendingTaskIntent`'s app-field flags and the `UpdateAppData`
enqueue sites"); framing §5.2 ("landing with the *first* field").

**Decision D-F3 (seam).** One exported manifest constant in the
domain crate + four thin assertions that consume it, landing in the
same chunk as the fields (framing §6 chunk 1 — "Domain field family +
lockstep manifest test"; "with the first field" means with the family
chunk, never deferred to a surface chunk):

```rust
// domain entities.rs (or a tiny task_fields.rs) — illustration.
/// The mergeable task-field manifest: the single source the lockstep
/// tests pin every structure against (#244). One row per `TaskClocks`
/// slot; `.1` names the `Task` members the clock guards (composites
/// name both); `.2` says which sync lane carries it.
pub enum FieldFamily { Native, AppData }

pub const TASK_FIELDS: &[(&str, &[&str], FieldFamily)] = &[
    // clock        task_fields                         family
    ("title",       &["title"],                          FieldFamily::Native),
    ("description", &["description"],                    FieldFamily::Native),
    ("duedate",     &["duedate"],                        FieldFamily::Native),
    ("done",        &["done"],                           FieldFamily::Native),
    ("position",    &["stack", "order"],                 FieldFamily::Native), // composite (R8)
    ("labels",      &["labels"],                         FieldFamily::Native), // whole set (R7)
    ("archived",    &["archived"],                       FieldFamily::Native),
    ("deleted",     &["deleted"],                        FieldFamily::Native),
    ("scheduled",   &["scheduled"],                      FieldFamily::Native), // D-F2 branch A
    ("effort",      &["effort"],                         FieldFamily::AppData),
    ("importance",  &["importance"],                     FieldFamily::AppData),
    ("urgency",     &["urgency"],                        FieldFamily::AppData),
];
```

The **home ruling F3-05 asks for** is hereby made: extend
`Task`/`TaskClocks` — *not* a parallel app-fields map (D-F1). A
parallel map would fork every consumer (`TaskChanges` still needs
slots; the guard still needs flags; storage still needs columns) while
additionally breaking struct-pattern ergonomics; F16-01's ceremony is
the same size either way, so the seam, not the shape, is the fix.
**Ruled rationale addition (2026-10-18)**: more than one sync
connector is planned, and the local state and data model is therefore
to be viewed as **a single construct independent of what sync
connectors implement internally to do the syncing**. Consequences,
stated once: the domain field shapes carry no lane information; the
manifest's `family` field records *the Deck connector's* lane
assignment for a field (today: entity-stamp native vs app-data
document), not an intrinsic property of the field — a future
connector that carries `scheduled` differently (or not at all) is a
connector-side mapping concern, never a reason to fork `Task`/
`TaskClocks`.

The four assertions (each is one small test; together they are *the*
manifest test):

1. **Domain bijection** (`taskboard-domain`): serde-serialize a
   populated `TaskClocks` and a `PendingTaskIntent` and assert both
   key sets equal `TASK_FIELDS`' clock set exactly; serialize a
   populated `Task` and assert its key set ⊇ the union of
   `task_fields`; serialize a populated `TaskChanges` and assert its
   key set ⊆ the clock set (every sparse slot names a mergeable
   field — the double-option slots must serialize `Some(_)` values
   for this, `skip_serializing_if = "Option::is_none"` hides `None`).
   Mechanics: `PendingTaskIntent` (pipeline.rs :92, private) gains
   `Serialize, Deserialize` derives — it is private, so no API grows;
   its serde exists for exactly this assertion. A missing clock, a
   clock without a field, a guard flag without a clock, or a
   `TaskChanges` slot naming nothing fails here, at the layer that
   owns all four structures.
2. **Storage columns** (`taskboard-storage-sqlite`): against a
   migrated file DB, assert `PRAGMA table_info(tasks)` contains
   exactly the manifest's clocks as `ck_<clock>` columns (plus the
   documented non-mergeable columns: ids, binding, `remote_seen`, …)
   and one value column per manifest row's `task_fields` — the
   assertion carries the three name mappings that are not verbatim
   (`stack` → the `stack` FK column, `order` → `sort_order`,
   `labels` → the `task_labels` link table, 0001 :42/:50/:80–83), so
   a new field without columns fails.
3. **Document keys** (`taskboard-sync-nextcloud`, landing with doc 1's
   chunk 3/4): assert the `TaskEntry` codec's modeled field-key set
   equals the manifest's `AppData` rows exactly — under branch A,
   `{effort, importance, urgency}` and *no* `scheduled` key; a
   `scheduled` document key appearing anyway (the half-run ceremony
   in its most likely form) fails here.
4. **Op routing** (`taskboard-state`): for each manifest row, one
   engine round: `EditTask` carrying only that field's slot enqueues
   `UpdateTask` for `Native` rows and `UpdateAppData` for `AppData`
   rows (mixed slots enqueue both — full-send covers both lanes).
   This pins the enqueue sites doc 1 §7 asked the seam to name; the
   engine snapshot tests per verb remain the *behavioral* coverage
   (clock advance, value persistence) — the manifest test is the
   structural tripwire, not a replacement for them.

**Why one hand-written constant is the fix and not
test-duplication**: the manifest is written *once* and every
structure is checked *against* it — the rot-prone copies (five
hand-lists in five tests) are exactly what the design avoids. The
ceremony table F16-01 counts, restated as the checklist a field
author walks (each row is a named site in §2's table): `Task` field +
`TaskClocks` clock + `TaskChanges` slot + `PendingTaskIntent` flag +
`pending_task_intents` arm + `keep_task_intent` copy + (Native:
`merge_task` verdict + `RemoteTask` DTO + push shapes ×2 + client
encode | AppData: `TaskEntry` key + fold entry) + storage value
column + `ck_` column + row struct + decode + upsert + CLI flag +
parser + handler + `TaskRow` + UI row model + phrase helper. The
manifest test cannot force the *copy sites* (they are if-blocks, not
declarations) — the per-field engine snapshot tests cover those; what
it forces is that no field exists anywhere without its declarative
skeleton, which is the failure mode F16-01 observed (defaulted clock /
false guard / absent slot — all declarative drift).

**Branch-B delta** (if the session rules sidecar for `scheduled`):
row 9 moves to `AppData`; assertions 3 grows `scheduled`, and
assertion 4 routes it to `UpdateAppData`. Nothing else changes.

**DF conformance**: a const + tests across the existing crates; no
runtime vocabulary. **Topology untouched.**

### 3.4 D-F4 — Additive storage migration shape

**Anchors.** 0001's `tasks` table (:40–62 — value NULL, clock NOT
NULL); 0005's epoch defaults (:8–10 — `DEFAULT
'0001-01-01T00:00:00+00:00'`, "pre-migration rows keep the
remote-wins merge behavior bit-for-bit"); doc 1 §2 ("this wave's
migrations number from 0007") and §7 ("rule clock column + field
columns ride docs 2/3's additive migration (0007+)"); repo.rs row
ceremony (:84/:236/:864).

**Decision D-F4 (migration).** One migration, **`0007_task_field_family.sql`**,
landed by the field-family chunk (framing §6 chunk 1) — including the
reminder-rule write clock column doc 1 D-S9 reserved, so doc 3's
chunks need no migration of their own:

```sql
-- 0007_task_field_family: Wave-1 domain field family (#208, #209)
-- plus the reminder-rule write clock (doc 1 D-S9's reserved slot).
-- Value columns are NULL = unset (0001's duedate precedent); clock
-- columns are epoch-defaulted NOT NULL (0005's mechanism) so
-- pre-migration rows read as "never locally written" — every remote
-- stamp / document clock wins bit-for-bit.
ALTER TABLE tasks ADD COLUMN scheduled   TEXT NULL;    -- rfc3339, duedate's twin
ALTER TABLE tasks ADD COLUMN effort      INTEGER NULL; -- Effort seconds (u64, codec-checked ≤ i64::MAX)
ALTER TABLE tasks ADD COLUMN importance  TEXT NULL;    -- "very_low"|"low"|"normal"|"high"|"very_high" (serde unit names)
ALTER TABLE tasks ADD COLUMN urgency     TEXT NULL;    -- "very_low"|"low"|"normal"|"high"|"very_high"
ALTER TABLE tasks ADD COLUMN ck_scheduled   TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_effort      TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_importance  TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE tasks ADD COLUMN ck_urgency     TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
ALTER TABLE reminder_rules ADD COLUMN updated_at TEXT NOT NULL DEFAULT '0001-01-01T00:00:00+00:00';
```

- **Why TEXT for the enums**: the serde unit-variant names are
  self-describing in the DB and round-trip the domain encoding
  verbatim (the storage codec's one-representation discipline); an
  INTEGER discriminant would invent a second encoding to keep in
  sync with serde for zero benefit at 3 values.
- **No data migration anywhere** (roundtrip §6's "local-only period
  ≡ zero"): the fields land together with their sync lanes; existing
  rows are simply unset.
- **Row codec**: `TaskRow` grows the eight task columns; `decode_task`
  (:236) and the upsert (:864) grow them symmetrically; the
  whole-state batch (`load_inner`/`apply_inner`/`execute_batch`)
  covers them without structural change — 0004's reminder round-trip
  through the batch (commit `f3fd3d4`) is the test precedent to
  extend.
- **Snapshots**: `AppState` widens (Task fields) — per DF-9's rule
  the widening chunk owns its one deliberate insta regeneration pass;
  the storage round-trip and engine-snapshot corpora regenerate in
  the field-family chunk, reviewable as `--force-update` diffs.
- **Branch-B delta**: none — storage is family-invariant (doc 1 §7
  already noted the columns ride regardless of the `startdate`
  ruling).

**DF conformance**: additive columns on the existing single-user
schema; numbering after multi-board's 0006. **Topology untouched.**

### 3.5 D-F5 — CLI verb + UI list-decoration surface growth

**Anchors.** `TasksCmd::Edit` (cli.rs :384–398) and `--due`'s full
path as the pattern (`DueSpec` :267, `parse_due` :276 — RFC-3339,
bare date = midnight UTC, `none` clears; hand-rolled, no new crate);
`commands/tasks.rs::edit`/`edit_on` (:398/:412) feed both frontends;
`TaskRow` + `due_marker` (output.rs :89–108); the UI seam
(`models.rs::push` :101 with `due_phrase` wiring :250/:330;
`presentation.rs` constants + `due_sets` :23–51 wrapping the domain's
`overdue_tasks`/`due_soon`, reminders.rs :420/:431); framing §5.2
("CLI verbs + UI list decorations"; the matrix *view* is out).

**Decision D-F5 (surfaces).** No new top-level verbs — the family
rides `tasks edit` flags, exactly as `--due` does (scripting-first
symmetry; `tasks add` takes no `--due` today and takes no new fields
either):

- `--scheduled <WHEN>` — parsed by the *same* vocabulary as `--due`
  (reuse `parse_due`'s spec; bare date = midnight UTC, `none`
  clears). One parser, two flags.
- `--effort <EFFORT>` — new hand-rolled `parse_effort` (the
  `parse_due` precedent, no new dependency): a bare number is
  **minutes** (the human default for task effort — "this takes 30"),
  explicit suffixes `s`/`m`/`h` compose (`1h30m`), `none` clears.
  Parsed to `Effort` seconds; overflow past `u64` is a clap usage
  error (exit 2), matching `parse_due`'s failure shape.
- `--importance <LEVEL>` and `--urgency <LEVEL>` —
  `<very-low|low|normal|high|very-high|none>`; the parser maps the
  hyphenated CLI spelling to the serde variant name, `none` clears
  (unset). Unknown levels are clap usage errors.
- REPL mirrors the flags (it shares `commands/tasks.rs::edit_on`;
  completion tables grow the flag names).
- `TaskRow` (output.rs) grows `scheduled`, `effort`, `importance`,
  `urgency`; the list renders them compactly — scheduled as its
  timestamp, effort in the CLI's own duration vocabulary (`30m`,
  `1h30m` — one formatter shared with the parser), importance and
  urgency as their level names. Exact column layout is the
  implementation chunk's polish; the *fields on the row* are the
  contract.
- **UI list decorations**: the row model (`models.rs::push`) grows
  `scheduled_phrase`, `effort_phrase`, and slint-safe
  importance/urgency carriers (strings from the presentation layer);
  `presentation.rs` grows the pure phrase helpers mirroring
  `due_phrase`, and the decoration *visuals* (weight/color tokens for
  the five levels) use the existing token system — no new component
  family, no slint-side logic (the dumb-client rule; `due_sets'`
  set-membership-over-domain-functions pattern :50 is the template).
  Classification stays out of slint: anything semantic ("starts
  today", "overdue by effort-fit") is a domain function or a
  presentation pure function over domain output — never view code.
- **Today view / search are doc 4's** (framing §5.4); this doc's UI
  scope stops at list decorations. Doc 4 consumes `scheduled` +
  §3.6's day window through the same presentation seam.

**DF conformance**: app/UI-layer growth only; presentation logic
reads `ArcSwap<AppState>` as today. **Topology untouched.**

### 3.6 D-F6 — The `UserPreferences` home ruling + day-start-time semantics

**Anchors.** Issue #345 (the question, verbatim: "resurrect the
Phase-1-deferred `UserPreferences` domain entity vs app-config
sections … device-local for now"); the Phase-1 deferral record
(growth-roadmap phase 1 §13: "the entity list (`Task`, `Project`,
`UserPreferences` → the canonical `Board`/`Stack`/`Task`/`Label` set;
… `UserPreferences` deferred)"); future-directions §3.4 :134 ("many
consumers … land it *before* the features that depend on it");
config.rs's section set + typed-validation precedent (:45–53, :97+);
`AppState` (state.rs :24–40 — no preferences home); the time/DST
audit headline (UTC-instant end-to-end by representation; the `Clock`
seam's "nothing calls `Utc::now()` directly", clock.rs :11);
`ArcSwap` read model (AGENTS §3); doc 1 D-S2's reserved `settings`
section (the D5 vehicle).

**Decision D-F6 (home): a config section — `[preferences]` — not a
domain entity. The Phase-1 deferral stands.**

```toml
[preferences]
# Local wall-clock time-of-day at which "today" begins (night-owl
# rollover). Device-local; never synced until the D5 settings
# section exists.
day_start_time = "00:00"   # default; "05:00" for the night-owl case
```

- **Why config wins for this wave**: (1) *Device-local by
  construction* — #345's own fence until D5; a per-device file is the
  honest home for a per-device value, and zero ceremony rides with it
  (no `AppState` growth, no storage table, no engine command, no
  snapshot churn, no merge semantics to design). (2) *The consumers
  are not engine consumers*: today view (doc 4, UI), CLI list
  markers (app) — both live above `ArcSwap` reads or in the app
  layer, where figment-resolved config already flows (the
  `[ui] scale` precedent is consumed by the very same UI layer;
  `[reminders] tick_secs` shows config reaching engine actors at
  bootstrap when one eventually needs it). (3) *Nothing downstream is
  precluded*: when D5's settings sync lands, the reserved `settings`
  document section (doc 1 D-S2) is keyed by preference name and
  agnostic to where the device's local value lived — migration is
  "seed the document from config at first D5 sync", not a data move.
- **Ruled addition (2026-10-18)**: config's one real cost — no
  in-app editing route (a preference change means editing
  `taskboard.toml` or an env var, then restarting) — gets its relief
  as **backlog issue #351**, a GUI settings editor riding the
  config-write machinery the logout flow already owns
  (`AppConfig::write_path`, config.rs :26–30). No domain entity, no
  in-wave scope.
- **The resurrection trigger, recorded**: re-open the domain-entity
  question only if a future feature needs *engine-merged* preferences
  (cross-device conflict resolution on the preference itself, or
  engine-internal logic that must observe preference changes as
  state). Nothing in Waves 1–3 as scoped does (habit boundaries and
  daily generation are later waves and can take bootstrap-injected
  config the way `tick_secs` does). F3-05-style lockstep cost is the
  tax a domain entity would charge; this wave declines to pay it.
- **Day-start-time semantics (incl. DST), stated as a contract**:
  *"today" began at the most recent instant whose local wall-clock
  time-of-day equals `day_start_time`, computed in the device's
  current UTC offset.* Concretely, a pure domain function (new small
  module `days.rs`, chrono-only, `Clock`-seam-clean — takes time as
  data):

  ```rust
  /// The 24h window [`start`, `start` + 24h) containing `now`, where
  /// `start` is the most recent `day_start_time` in the fixed offset
  /// `offset`. Pure, total, deterministic (#345's contract; the
  /// fixed-offset discipline below is the DST ruling).
  pub fn day_window(
      now: DateTime<Utc>,
      day_start_time: chrono::NaiveTime,
      offset: chrono::FixedOffset,
  ) -> (DateTime<Utc>, DateTime<Utc>);
  ```

  Computation: convert `now` to `offset`, take its date, set
  `day_start_time`, convert back; if that candidate is after `now`,
  step back one day. **DST ruling — fixed-offset arithmetic**: the
  window is computed in the consumer-supplied *current* offset, so
  the instant math is total and gap-free by construction (a fixed
  offset has no transitions; the nonexistent-local-time case cannot
  arise). The cost, accepted and documented: on a transition day the
  wall-clock label of the boundary can differ from the tz-database
  answer by the DST delta (≤1h, at most twice a year, and only for
  non-midnight starts). This mirrors the audit's
  UTC-instant-by-representation posture — the instants are always
  unambiguous; only the "which local day" label wobbles on the two
  edge days, which no Wave-1 consumer observes. The alternative —
  tz-database-aware boundaries in the domain — would pull
  transition-table lookups (env-dependent, like `Utc::now()`) into
  pure functions and break the crate's determinism rule (clock.rs
  :11) for a once-a-year hour nobody has asked to feel.
- **Consumers contract (who reads what)**: app bootstrap parses
  `[preferences]` once (figment → `NaiveTime`, typed validation:
  must parse as a time-of-day, else the config error path — never
  silently defaulted) and resolves the *device offset* via
  `chrono::Local` at derivation time (a device concern, app layer —
  the same layer that renders wall-clock times today). Consumers —
  UI today view, CLI markers, later engine features via bootstrap
  injection — receive `(day_start_time, offset)` explicitly and call
  `day_window`; nothing reads figment mid-flight, nothing mutates
  `AppState`. Proptest target: `start ≤ now < start + 24h`; stepping
  `now` across `start` flips the window; the window advances exactly
  24h per day in fixed offset.
- **Device-local until D5, restated**: no sync lane, no document key,
  no `settings` serialization in this wave (doc 1 D-S2 keeps the
  section reserved); kiosk/desktop each keep their own preference —
  correct today, and the D5 design decides later whether preferences
  sync per-device-class or per-user.

**DF conformance**: config-section + pure domain function; no
vocabulary, no channels, no state growth. **Topology untouched.**

## 4. Coherence map

- **One clock home**: `TaskClocks` holds the local write clock for
  every mergeable field, native *and* app-data alike — the
  *comparison basis* differs (entity stamp for the nine native
  fields, `merge_task` :182–231; document-embedded clock for the
  three app fields, doc 1 D-S3's fold) but the local slot is shared,
  which is precisely what lets one guard struct (doc 1 D-S6) and one
  manifest (§3.3) cover both families.
- **One edit verb**: everything user-facing flows through
  `EditTask { changes }` — CLI flags, REPL, and (via actions) UI
  edits all compose a `TaskChanges`; the engine stamps clocks, routes
  ops (`UpdateTask` for native slots, `UpdateAppData` for app slots),
  and persists. No field-specific verbs to drift.
- **One document, three keys**: under branch A the document models
  exactly `{effort, importance, urgency}`; `scheduled` never appears
  (D-F2/D-F3 assertion 3); `task-kind`/`respite`/`recurrence` stay
  reserved names (doc 1 D-S2).
- **Clearing is a write, everywhere**: `Some(None)` in `TaskChanges`
  stamps the clock (messages.rs :206–207); `{v: null, clock}` in the
  document beats by clock (doc 1 D-S2); `none` at the CLI clears
  (`--due`'s vocabulary, D-F5).
- **Fields dumb, derivations parameterized**: the today view's
  "today" is not a field — it is `day_window(now, preference,
  offset)` over `scheduled`/`duedate` (D-F6); decision-support math
  (Wave 2–3) consumes `importance`/`urgency`/`effort` as plain
  values. Nothing derived is stored, so nothing derived can desync.
- **Conformance to the binding inputs**: D1's dedicated-fields bias
  (D-F1), roundtrip §6's day-one verdicts (all four fields sync on
  their lanes day one; migration-free), F16-01/F3-05's seam (D-F3
  answers both halves — home ruling + manifest), #345's home
  question (D-F6), DF-1…DF-10 (every section's note).

## 5. Decision record — recommendations + open points

| ID | Point | Recommendation (exactly one) | Status |
|---|---|---|---|
| D-F1 | Field shapes | `scheduled: Option<DateTime<Utc>>` (duedate's twin), `Effort` newtype over u64 seconds, `Importance`/`Urgency` as two distinct **five-level** enums (`VeryLow…VeryHigh`) under `Option` with derived `Ord` as the sorting contract; all four + clocks on `Task`/`TaskClocks`, all four as double-option `TaskChanges` slots; derived priority out of wave | **ACCEPTED (ruled 2026-10-18)** — amended: five levels (not the drafted three), enums retained, ordering comparability required; effort confirmed (seconds + suffix-with-minutes-fallback) |
| D-F2 | Native `startdate` | **Map Deck's native `startdate` for scheduled-start** (DTO + `merge_task` verdict + push shapes + client encode; no sidecar key). Trade-offs recorded (§3.2): entity-stamp merge basis, Deck-side normalization, write-path wire gate folded into doc 1's contract-fixture | **ACCEPTED (ruled 2026-10-18)** |
| D-F3 | #244 lockstep seam | Home = extend `Task`/`TaskClocks` (no parallel map); one exported `TASK_FIELDS` manifest (clock / task_fields / family) + four assertions: domain bijection (serde key sets), storage `PRAGMA` columns, document key set = AppData rows, engine op routing per family — landing with the field-family chunk | **ACCEPTED (ruled 2026-10-18)** — with the connector-independence rationale recorded (§3.3) |
| D-F4 | Storage migration | `0007_task_field_family.sql`: 4 value columns (NULL = unset) + 4 epoch-defaulted clock columns + the D-S9 rule clock column; TEXT enums (five serde names), INTEGER effort; row codec/batch/snapshot regen in the same chunk; branch-invariant | **ACCEPTED (ruled 2026-10-18)** |
| D-F5 | CLI + UI surfaces | `tasks edit` flags `--scheduled/--effort/--importance/--urgency` (no new verbs; `parse_effort` hand-rolled, bare number = minutes); `TaskRow` + UI row model grow phrase/level fields; visuals via existing tokens; no slint-side logic | **ACCEPTED (ruled 2026-10-18)** |
| D-F6 | UserPreferences home | `[preferences] day_start_time` config section (device-local; typed validation; consumers get `(NaiveTime, FixedOffset)` explicitly); domain `days.rs::day_window` pure fixed-offset contract (DST delta ≤1h twice-yearly accepted); Phase-1 deferral stands, D5 `settings` section is the later vehicle | **ACCEPTED (ruled 2026-10-18)** — both sub-points (config section; device-offset computation); ruled addition: GUI settings editor → issue #351 |

**Decision session record (2026-10-18)** — the user ruled on every
point (framing D-D6's list, this doc's entries):

1. **D-F2 (the headline)**: **accept** — native `startdate` maps
   scheduled-start (branch A; §3.2's ruled paragraph).
2. **D-F1, amended**: importance/urgency at **five levels**, encoded
   as enums (exhaustive matching), with an ordering comparison so
   sorting by importance/urgency is easy — recorded as the derived
   `Ord` contract (§3.1). Effort confirmed: stored seconds, CLI
   explicit suffixes with bare-number = minutes fallback.
3. **D-F3**: **accept** — extend `Task`/`TaskClocks`; the user added
   the connector-independence rationale (planned multi-connector
   support ⇒ the local data model is one construct, never forked by
   connector internals) — recorded in §3.3.
4. **D-F6**: **accept both** — `[preferences]` config section, and
   the fixed-offset day-boundary computation from the device's UTC
   offset. Ordered tracker edit: a way to edit `taskboard.toml`
   settings from the GUI, recorded as backlog issue **#351**
   (unscheduled; §3.6/§7).
5. **D-F4, D-F5**: **accept as designed**.

## 6. Consequences for the wave implementation plan

Against framing §6's chunk sketch:

- **Chunk 1 (domain field family + manifest test)** lands D-F1 +
  D-F3 + D-F4: the four fields/clocks/slots, `TaskChanges`
  extension, `UpdateAppData` guard flags per doc 1 D-S6, migration
  0007 (incl. the rule clock column so doc 3's chunks are
  migration-free), `TASK_FIELDS` + the four assertions (assertions
  3–4's sync/state halves land with doc 1's chunks 3–4 if those land
  first — the manifest const itself is chunk 1), strategy/snapshot
  regeneration, and (branch A) the `RemoteTask` DTO + `merge_task`
  verdict + push shapes with contract-fixture wire pins.
- **Chunk 2 (UserPreferences + day-boundary)** lands D-F6:
  `[preferences]` section + validation, `days.rs::day_window` +
  proptests. No storage, no state.
- **Chunk 5 (field surfaces)** lands D-F5: CLI flags + parsers,
  REPL completion, `TaskRow`, UI row-model decorations.
- **Doc 1's chunks 3–4** consume this doc's shapes verbatim:
  `TaskEntry` keys (`effort`/`importance`/`urgency`, `{v, clock}`,
  integer seconds / enum names / `null`-clear), the D-S6 guard flag
  names (one bool per manifest AppData row + `reminder_rules`), and
  the enqueue contract (EditTask slots → `UpdateAppData`).
- **Doc 4** consumes `scheduled` (or its branch-B twin), the phrase
  helpers, and `day_window` — and inherits the anchor-task hiding
  ruling (doc 1 §7) into the same list surfaces D-F5 extends.

## 7. Backlog candidates (out-of-scope ideas; the user promotes)

- `tasks add` flag parity for the new fields (today `add` takes no
  `--due` either; revisit if quick-capture UX (#347) wants
  at-create scheduling).
- Re-leveling importance/urgency beyond the ruled five levels —
  decision support's re-modeling decision, not the field family's.
- Tz-database-aware day boundaries if a consumer ever observes the
  ≤1h twice-yearly wobble (D-F6's fixed-offset contract).
- `UserPreferences` domain entity + settings sync — D5's design
  round owns the resurrection question with a real consumer in hand.
- GUI settings editor (view/edit `taskboard.toml` from the app) —
  **tracked as issue #351** (ruled addition 2026-10-18;
  `AppConfig::write_path` is the write-machinery precedent).
- Effort calibration feedback loop (#214) — out per framing §2.

## 8. References

- Daily-driver framing plan (normative §5 Q2; §2 scope; §6 chunks;
  D-D6): `.artifacts/plans/2026-10-18-daily-driver-wave-design-plan.md`.
- Round doc 1, app-data sidecar (ACCEPTED; interfaces consumed):
  `.artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md`
  (commit `2b1a657`).
- Roundtrip strategy report (normative): §3.1 wire key list, §4
  consequences, §5 merge discipline (BINDING via doc 1), §6
  per-field verdicts — `.artifacts/reports/2026-10-14-deck-roundtrip-strategy-report.md`.
- Future-directions analysis (normative bias): §3.1, §3.4, §4.1 D1 —
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`.
- Audit findings behind #244: F3-05
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-crate-domain-report.md`
  :566), F16-01/F16-02
  (`.artifacts/reports/2026-10-16-v0.1.0-audit-theme-arch-fit-report.md`
  :143/:159).
- Time/DST audit (UTC-instant posture):
  `.artifacts/reports/2026-10-16-v0.1.0-audit-theme-time-dst-report.md`.
- Multi-board design + DF-1…DF-10:
  `.artifacts/plans/2026-10-18-multi-board-detailed-design-plan.md`,
  `.artifacts/plans/2026-10-18-multi-board-wave-implementation-plan.md` §2.
- Issues: #208 (scheduled split), #209 (effort + importance/urgency
  decorations), #244 (lockstep seam), #345 (day-boundary home).
- Code anchors @ `2b1a657` (§2's table; symbols are the stable keys).
