# [Plan] Respite marking + break reminders (#493 + #495, backlog round 2 chunk 3.5) — stage-1 implementation plan

- **Date**: 2026-11-02 (written into the mandated
  `2026-10-31-respite-break-reminders-plan.md` slot of the round-2
  dispatch table).
- **Kind**: stage-1 TDD implementation plan for chunk **3.5** of the
  backlog round-2 dispatch. Zero production code in this commit; this
  file is the chunk's entire planning artefact. Implementation is
  stage 2's, review + shipping stage 3's.
- **Origin**: the round-2 dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` (batch
  3, chunk 3.5, branch `feat/respite-break-reminders`); the normative
  fence issues **#493** (respite marking — "an additive entry kind with
  default-off fold on the wire", "consumed by break-reminder triggers
  and review statistics") and **#495** (break reminders — "a different
  trigger source — accumulated tracked-work duration … rides the landed
  reminders pipeline (rules/evaluator/sink) with a new pure trigger fn
  over time entries; respite-marked entries feed the reset condition");
  the accepted DR-5 design round
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md` —
  §7's vocabulary reservation is the binding fence text: *"a `kind`/flags
  field and a `user` field fold onto `TimeEntry` and its wire row as
  default-carrying additions — nothing in v1's schema or wire shape
  precludes either"* — plus §1's seams table, §5's test hooks, and §6's
  risks, all of which this chunk consumes as landed fact.
- **Normative base**: branch point = `origin/main` @ `aa252c4b` (the
  worktree's base). **Chunk gate verified** (2026-11-02): chunk 2.2's
  PR **#576** (`chore/audit-cleanups-domain-state`, closing issues
  #270 + #271) is **MERGED** (2026-11-01T22:56:58Z) and its merge
  commit `b535b989` is an ancestor of `origin/main`
  (`git merge-base --is-ancestor` checked). Batch-3 siblings verified
  at planning time: 3.1/3.2 branches exist; **3.3
  (`feat/delivery-proof-pruning`) is IN FLIGHT with four commits
  touching `reminders.rs` (+352) and `persistence.rs` (+131)** — see
  §10 R-1, the lane coordination note, and §11's migration claim.
- **Status**: **READY** — every open question the seams raised is
  resolved with a recorded decision (§7, D-1…D-22). The two findings
  this planning pass contributes beyond the fence issues are the
  **storage correction** (D-12: the dispatch's "expect no schema
  impact" expectation is *refuted* — the `time_entries` table is
  strictly columnar with no spare column, so the kind needs an
  additive `ALTER TABLE`; the *sidecar wire row* needs none) and the
  **occurrence ruling** (D-8: a duration trigger's occurrence key is
  the *threshold-crossing instant* — the only stable key that composes
  with the `(reminder, occurrence)` set-union suppression without a
  per-tick double-fire).

## 1. Mission & binding constraints

Two features, one chunk, sharing one data lane:

1. **Respite marking (#493)**: a `kind` datum on time entries whose
   `Respite` value marks an entry as *break activity* rather than
   work. It roams (the wire row grows additively, default-off), it
   persists (one additive sqlite column), and it is marked at the
   `track` surface. Its only in-chunk consumer beyond the wire is the
   break trigger's reset condition plus the work-only
   tracked-today projection.
2. **Break reminders (#495)**: reminder rules whose trigger source is
   *accumulated tracked-work duration since the last respite* ("you've
   tracked 90m without a respite-marked entry") instead of a task due
   date. They ride the **landed** reminders pipeline unchanged — the
   evaluator decides WHEN, the delivery/withdraw machinery, the
   in-app surface, snooze chains, and the escalation driver all
   consume the firing as they consume any firing.

Binding constraints (every one verified against the tree at the base):

- **The evaluator stays urgency-agnostic and decides WHEN only** (the
  `Insistence` doc is the written contract, reminders.rs :228-239):
  the new trigger pass reads no insistence, no urgency; the payload's
  insistence is a fire-time snapshot from the rule row
  (`notifications_for` precedent).
- **No disturbance of the existing evaluator arms** (issue #495's
  fence): the rule/default/alarm passes in `due_firings` and
  `active_notifications` keep their shapes; the break trigger enters
  as a new pass composed alongside them (D-10), with the existing
  unobservability/totality properties extended to hold with break
  rules present.
- **The suppression guard is set-union over `(reminder, occurrence)`
  keys and stays exactly that** (reminders.rs :338-351): the duration
  trigger must produce a *stable* occurrence instant per firing
  episode or it double-fires every tick (D-8 rules out the tick-stamp
  alternative explicitly).
- **"Nothing ticks"** (ADR 0017 / DR-5 §3): no new timer, no new tick
  arm, no `[reminders]` config key (D-16). The crossing is derived at
  tick time from entries + `now` by a pure fn; the break evaluation
  rides the existing 30 s tick when `[reminders] enabled`.
- **Dumb-client UI**: no UI-held state; the notification row is the
  domain's own `active_notifications` output (presentation.rs :475);
  the break delta in `taskboard-ui-slint` is two compiler-forced
  exhaustive-match arms (D-19).
- **No new dependencies**: `cargo deny` untouched; everything is
  std + chrono + serde + clap + uuid already in the tree.
- **Type-driven A1 policy** (AGENTS §2): the threshold and the entry
  kind are hand-rolled newtypes/enums with fallible mints at the
  trust boundary (`WorkThreshold::try_new`, the `ClosedSpan`
  precedent); no derive crates; deliberate leniency documented at the
  type (D-5, D-2).
- **Non-goals (fences)**: review statistics/charts beyond the
  tracked-today correction (Wave-2 follow-ons); tracker task kind
  (#492, chunk 4.2 — a *task* kind, deliberately not conflated with
  the entry kind); pomodoro (#496); per-user attribution; roaming
  break-rule config or roaming break dismissals (D-15 records the
  fence); a `[tracking]`/`[reminders]` config knob for thresholds
  (thresholds are per-rule data; the DR-5 knobs fence stands);
  snooze presets; any change to the task-anchored rule grammar.

## 2. The respite kind (#493) — D-1…D-3, D-11

### D-1 — a kind enum, not a bool flag

```rust
/// What a time entry counts as. `Work` is the default and the
/// overwhelming case; `Respite` marks break activity — consumed by the
/// break-reminder trigger's reset condition and by review statistics.
/// Deliberately an enum, not a bool: the DR-5 §7 vocabulary reservation
/// names *kind*s (plural, forward-extensible) and a second bool would
/// fork the fold/wire/export plumbing per flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TimeEntryKind {
    #[default]
    Work,
    /// Break activity: counts *against* the work streak, never toward
    /// it. The marker is data, not behavior — nothing auto-fires from
    /// it; the break-reminder trigger is the consumer.
    Respite,
}
```

Lives in `tracking.rs` beside `TimeEntry`; the struct gains
`pub kind: TimeEntryKind` with `#[serde(default)]` on the *field*
(so old journal payloads and old receipt JSON decode total — the
`Insistence` additive-enum precedent, DR-3/2). Constructors of
`TimeEntry` in the planner fill `kind` from the command payload
(D-3); every existing construction site (`start_on`/`log_on`
planners, `fold_time_entries` adoption) names it explicitly — no
`..Default::default()` blinding.

### D-2 — the wire row: additive field, default-off decode, no silent downgrade

`TimeEntryRecord::Entry` (appdata.rs :616-623) gains
`kind: TimeEntryKind`. The rules, each pinned by test:

- **Old data (no `"kind"` key) decodes `Work`** — the default-off
  fold the issue mandates. `deserialize_verbatim` reads
  `map.get("kind")`: absent/null → `Work`.
- **A modeled value round-trips**: `"work"` / `"respite"` (the serde
  unit names — the 0013 insistence-column convention: stored text is
  the domain serde unit name).
- **A present-but-unmodeled value demotes the whole row to
  `Verbatim`** — never a silent downgrade to `Work` (the
  `RuleEntry` doc's written rule, appdata.rs :384-386; D-S2's
  materialization rule). A future fleet member's new kind therefore
  *stops folding* here rather than corrupting history; it ages out by
  the 180-day retention lane (D-T13) — the same one-version tolerance
  every section already has, recorded as risk R-3.
- **Emit always writes the key** (`Serialize` arm +
  `time_entries_for` overlay builder, appdata.rs :1464-1495).
  Consequence, recorded honestly: a pre-kind device decoding our
  respite row gets a modeled `Entry` (it ignores unknown keys) and
  re-emits it *without* the kind — a stale device can silently
  downgrade a respite mark on re-flush. Same one-version tolerance
  class as insistence's landing; R-3.
- **`union_time_entries` and `lane_gc_old_time_entries` need no
  code change**: rows compete wholesale by clock under derived
  `PartialEq`, and GC matches on `start`/`clock` — the field rides
  along. `fold_time_entries` (merge.rs :859-938) gains exactly two
  line-shaped changes: the LWW-overwrite copies `kind`, and the
  adopted row carries it. Tombstone derivation
  (`derive_time_entry_tombstones`) and `prune_absent_cards` are
  untouched (board-level section — the load-bearing D-T10 fact).

### D-3 — the CLI marking surface: `--kind` on start, log, adjust

The marking rides the existing `track` verbs — no new verb, no new
family for marking:

| Verb | Flag | Semantics |
|---|---|---|
| `track start <task> --kind respite` | `TimeEntryKind` (default `Work`) | opens the *running break* — a respite-marked open entry; `track stop` closes it like any open entry |
| `track log <task> … --kind respite` | `TimeEntryKind` (default `Work`) | retroactive break block |
| `track adjust <entry> --kind <work\|respite>` | change mask: **omit = unchanged** | the retroactive marking/remarking of history; rides `AdjustTimeEntry`'s existing change-mask semantics and its undo pre-image |

Why a value flag over a `--respite` bool: `adjust` needs the
tri-state (unchanged / set / unset-to-work) a bool cannot express,
and one flag keeps the parse, the receipts, and the forward
vocabulary single-shaped. The value parses via the clap value-parser
seam (`parse_kind_flag`, the `parse_date_flag` precedent) — an
unknown kind is a boundary parse rejection, never a domain error
variant. `StartTracking`/`LogTime` payloads gain
`kind: TimeEntryKind` (`#[serde(default)]` for old journal
tolerance); `AdjustTimeEntry` gains `kind: Option<TimeEntryKind>`
(the change mask). Receipts (`TrackStartReceipt`, `TrackLogReceipt`,
`TrackAdjustReceipt`) and `track list` rows carry the kind datum;
`track status` renders the open entry as-is (the chip already shows
whatever runs — a running break is honest data on the kiosk, D-18).

### D-11 — the tracked-today correction

`track status`'s "tracked today" total becomes **work-only**:
`work_in_window(entries, window, now)` — filter `kind == Work`, then
delegate to the unchanged `tracked_in_window` (whose overlap-sum
clipping oracle stays the single definition of window arithmetic).
Leaving respite in the total would make the marking cosmetic — the
issue's "counts tracked time as break activity" is exactly this
filter. `export` keeps *per-row* durations (rows now carry `kind`;
no aggregate changes, CSV column in D-20).

## 3. The break-reminder rule family (#495) — D-4…D-7, D-13…D-15

### D-4 — trigger-source discriminator: a dedicated row family, NOT a grammar extension of `ReminderRule`

**Decision**: break rules are their own typed row family, entering the
model as a new `ReminderRef`/`ReminderAnchor` variant — not as a
`trigger:` field on the existing `ReminderRule`.

Reasons, each anchored in the verified tree:

1. **`ReminderRule` is task-anchored by construction**: `task:
   TaskId` is a required field feeding `occurrence_of`'s due-date
   arithmetic (reminders.rs :700-702), the card-keyed `reminder-rules`
   sidecar section, `rules_entry_for`'s per-card emission, and the
   fold's card→task resolution. A break rule has no task. A
   grammar extension forces `task: Option<TaskId>`, which touches
   every one of those arms — precisely the disturbance issue #495
   forbids ("without disturbing the existing arms").
2. **The when-only contract reads better typed**: the evaluator
   dispatches on trigger *source*; a dedicated family keeps each
   pass's inputs honest — the due pass consumes `tasks`, the break
   pass consumes `tracking.entries`. One struct pretending to be two
   triggers needs internal re-validation ("is this rule a due rule?
   then it must have a task") — invariants a sum type carries for
   free.
3. **Identity composes for free**: deliveries, dismissals, and
   snoozes are keyed by `ReminderRef` (the set-union guard). A new
   `ReminderRef::Break(BreakRuleId)` variant slots into `BTreeSet`
   keys, `text_form` ("`break:<uuid>`" — one new prefix arm in
   `text_form`/`parse_text_form`, the storage codec delegating to
   them), the ≤1-row-per-key invariant, and the withdraw direction
   (`withdrawals_for` is ref-generic) with zero semantic change.
   Under the grammar extension, the same `Rule(id)` ref would
   silently mix two trigger families in one map — the exact blur the
   discriminator avoids.
4. **The compile-break discipline works for us**: every exhaustive
   `ReminderRef`/`ReminderAnchor` match (dismiss planner
   command.rs :1877-1903, snooze planner, the fail-closed emission
   filter appdata.rs :1344-1395, `notifications_for`, the UI's
   `notification_rows`) breaks the build until it *explicitly rules*
   on the new variant — the additive-section mechanism that keeps
   the suppression boundary fail-closed (D-15).

**Shape** (reminders.rs, the `AdhocAlarm` precedent):

```rust
pub struct BreakRule {
    pub id: BreakRuleId,          // device-local id family (D-5)
    pub label: String,            // the notification title snapshot source
    pub threshold: WorkThreshold, // positive-duration newtype (D-6)
    pub insistence: Insistence,   // fire-time snapshot datum; evaluator never reads it
    pub created_at: DateTime<Utc>,
}
```

No `updated_at`: no rule-edit verbs (the `ReminderRule` ruling —
re-create to change), and device-local rows never compete under LWW.
Home: `ReminderState.break_rules: BTreeMap<BreakRuleId, BreakRule>`
(mirrored by `PersistedState` through `ReminderState` itself). New
commands: `CreateBreakRule { label, threshold, insistence }` →
`BreakRuleCreated { rule }`; `DeleteBreakRule { id }` →
`BreakRuleDeleted { id }`, repeat → accepted `NoOp`, stranger →
typed `UnknownBreakRule` (every named-target command's law). The
planner's delete cascades the ref-matched deliveries, dismissals,
and snooze links — the `remove_alarm` shape (reminders.rs :657-663)
extracted into a `remove_break_rule` sibling so the cascade has one
definition site. **No `UpdateBreakRule`** — no rule-edit verbs exist
anywhere; adding one here is a fence.

### D-5 — `BreakRuleId` joins the device-local id family

The `AdhocAlarmId` precedent ("never synced"): `local_id!` in the
device-local block of ids.rs. The alarms family is the standing
proof that a per-device reminder surface is coherent (its
deliveries/dismissals/snoozes are already per-device facts by R3's
two-fact split). **Roaming is fenced, not forgotten** — see D-15.

### D-6 — `WorkThreshold`, the A1 threshold newtype

```rust
/// A break rule's accumulated-work threshold. Private field; the
/// fallible mint is the command-layer trust boundary — a zero or
/// negative threshold can never enter a row (it would fire
/// immediately and permanently).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkThreshold(chrono::TimeDelta);
```

`try_new(TimeDelta) -> Result<Self, InvalidWorkThreshold>` for
strictly positive durations; `as_duration()`, and
`text_form()`/`parse_text_form()` in the `ReminderOffset` mold
("`90m`", "`1h30m`" — the existing duration-phrase grammar the CLI
already parses for `timers in`). Storage column
`threshold_seconds INTEGER` (the 0004 `offset_seconds` convention;
the domain newtype guarantees positivity, no CHECK re-legislates
it).

### D-7 — commands ride the existing lanes

`CreateBreakRule`/`DeleteBreakRule` are ordinary `StateCommand` arms
planned by `plan_command` (pure, `now` as data, v7 ids via
`IdGenerator` — the `CreateAdhocAlarm` shape). Undo: they join the
`NotUndoable(ReminderSurface)` arm (undo.rs :137-145 — the
rule/alarm family's existing disposition; device-local config rows,
the alarm ruling). The respite marking itself needs **no new undo
work**: `AdjustTimeEntry`'s kind change rides the existing closed-
entry pre-image (undo.rs :153-163 — the inverse `UpsertTimeEntry`
carries the old row including its old kind, free),
`StartTracking`/`LogTime` keep their `TrackingSurface`
dispositions.

## 4. The trigger fn and the occurrence semantics — D-8…D-10

### D-8 — the occurrence of a duration trigger is the threshold-crossing instant

The suppression guard keys on `(reminder, occurrence)`. For a
duration trigger the occurrence **cannot** be the tick stamp (every
tick would mint a fresh key → the delivery receipt could not
suppress the next tick → a toast per 30 s: the double-fire hazard,
unacceptable by construction). It also cannot be the streak start
(fires at the streak's first instant — nonsense). It is the
**crossing instant**:

> Walking time entries in `(start, id)` order, the crossing instant
> is the point inside a work span where the cumulative tracked work
> since the current streak's boundary first reaches the rule's
> threshold.

Properties that make this the right key, each pinned by test (§8):

- **Stable while history is stable**: the crossing instant lies in
  the past once crossed and does not move as `now` advances (open
  entries accrue to `now`, but the crossing point is fixed the
  moment cumulative work reaches the threshold). Tick 2 sees the
  same `(Break(id), crossing)` key the delivery receipt settled at
  tick 1 → suppressed → **at-most-once per episode**.
- **Respite re-arms legitimately**: a respite entry resets the
  accumulator; the next crossing is a *new* instant → a new key →
  the reminder fires again after another full threshold of work.
  This is the desired semantics and it composes with set-union
  additivity for free.
- **Dismissal is permanent for the episode** (the house law: no
  un-dismiss): the dismissal row settles `(Break(id), crossing)`;
  while the crossing instant stands, it suppresses everywhere.
- **History edits move the base honestly**: an adjust/delete/log
  that changes the accumulation moves or removes the crossing
  instant — the old key's rows orphan, the new base re-arms. This is
  the documented semantics of moving a due date (reminders.rs
  :338-342: "moving the due date … creates a new occurrence and
  re-arms"); no new mechanism is invented here. Orphaned
  delivery rows are bounded by edit frequency; GC is fenced to the
  delivery-pruning lane (R-4).

**Streak boundary semantics** (the reset condition the respite kind
feeds), precisely: walking entries in `(start, id)` order, each
`Respite` entry sets the boundary — a *closed* respite to its `end`
("since your last break ended"), an *open* respite (the running
break, D-3) to its `start`. Accumulated work at `now` is
`Σ work-span ∩ (boundary, now]`, clamped at zero — the
`tracked_in_window` overlap-sum convention (overlapping work
entries each contribute; rows are never physically split). An open
respite must anchor at its *start*, not at `now`: a moving boundary
would break accumulation monotonicity (law L-1). The boundary walk
and the crossing walk are one pass (`break_crossing`); a
`accumulated_work(entries, now)` helper exposes the accumulator for
the laws and for tests.

### D-9 — the pure trigger surface

In `tracking.rs` (projections live there — the `tracked_in_window`
home):

```rust
pub fn accumulated_work(entries: impl IntoIterator<Item = &TimeEntry>,
                        now: DateTime<Utc>) -> chrono::TimeDelta;
pub fn break_crossing(entries: impl IntoIterator<Item = &TimeEntry>,
                      threshold: WorkThreshold,
                      now: DateTime<Utc>) -> Option<DateTime<Utc>>;
```

In `reminders.rs` (the evaluator's home — it consumes
`ReminderState`):

```rust
pub fn break_due_firings(break_rules: &BTreeMap<BreakRuleId, BreakRule>,
                         tracking: &TrackingState,
                         reminders: &ReminderState,
                         now: DateTime<Utc>) -> Vec<ReminderFiring>;
```

Per rule (id order): `break_crossing(&tracking.entries,
rule.threshold, now)`; fire iff `Some(c) && c <= now &&
!is_delivered(&Break(id), &c) && !is_dismissed(&Break(id), &c)` —
exactly the task-pass eligibility shape, no new suppression
concept. `due_firings` and `active_notifications` compose it as a
third pass after the alarms pass (or the core concatenates — one
composition site, decided at implementation by whichever keeps the
existing signatures honest; the constraint is: **one** definition of
eligibility, and the `(occurrence, reminder)` total-order sort
preserved across the concatenation). Their signatures grow
`tracking: &TrackingState` — a compiler-forced sweep of the four
call sites (state `core.rs` :124-133, app `escalation.rs` :260, ui
`presentation.rs` :476, domain `persistence.rs` property tests).
The existing properties (compaction/settled-prune unobservability in
`due_firings`/`active_notifications`, persistence.rs :1596-1736)
extend mechanically: break rules present ⇒ still unobservable
(the new pass reads neither snooze-compacted nor alarm-pruned
state beyond the ref-keyed sets the properties already cover).

**A break rule created after the work already happened fires at the
first tick** — crossing ≤ now, undelivered. Intended ("you have
90m unbroken tracked work *now*"), recorded: creation is not a
reason to suppress an honestly-crossed threshold.

### D-10 — when-only, end to end

The break pass reads no `Insistence` (D-4's binding constraint);
`notifications_for`'s new `ReminderAnchor::Break` arm snapshots the
label from the rule row and the insistence from the rule row —
with a missing rule row *skipping* the payload (anchor unresolvable
→ skip: the function's documented totality rule, reminders.rs
:569-574). The escalation driver treats a Break anchor exactly like
an alarm anchor for affordances: no task, no Done/Mute target, the
insistence ladder applies as-is (app `escalation.rs` :537
precedent).

## 5. Storage — D-12…D-14

### D-12 — migration 0018 (the dispatch's no-schema-impact expectation, refuted and corrected)

Verified finding: the `time_entries` table (0015) is strictly
columnar — `entry_id, task_id, task_title, start, end, created_at,
updated_at` — with no extensions/JSON column; the local sqlite store
is the rehydrate path, so the kind must survive restarts there. The
wire row needs no migration (D-2); the local row does. **Migration
`0018_respite_break_rules.sql`**, additive only (ADR 0005), the 0013
precedent verbatim:

```sql
ALTER TABLE time_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'work';
CREATE TABLE break_rules (
    rule_id           TEXT PRIMARY KEY NOT NULL,
    label             TEXT NOT NULL,
    threshold_seconds INTEGER NOT NULL,
    insistence        TEXT NOT NULL DEFAULT 'normal',
    created_at        TEXT NOT NULL
);
```

The `DEFAULT 'work'` backfill **is** the default-off decode of old
local data (every pre-0018 row reads `Work`; pinned by the
migration-upgrade test). Stored text = the serde unit name; unknown
text decodes `Corrupted` at the codec (the 0013 rule — local rows
are trusted-written, a corrupt enum is a corrupted database, the
`decode_time_entry` strictness comment applies). `break_rules`
conventions = 0004/0008: hyphenated-UUID TEXT id, INTEGER seconds
(newtype-guaranteed positive), rfc3339 TEXT stamps, no FKs, no
CHECK re-legislating the newtype, no extra indexes (PK is the
lookup path at kiosk scale).

**Numbering claim + collision note**: 0018 is the next free number
at this chunk's base (migrations end at 0017_habit_trackers) and is
**unclaimed** by the in-flight batch-3 branches (verified:
`origin/feat/delivery-proof-pruning` carries no migration files;
3.1/3.2 are UI/config-surface chunks). The implementing stage
re-verifies at its own base and records any collision — the DR-5
§1 rule.

### D-13 — persistence actions

`PersistenceAction::UpsertBreakRule(BreakRule)` /
`DeleteBreakRule(BreakRuleId)` — the exhaustive storage match forces
the arms by design (repo.rs :1200-1245). The delete arm removes the
row only; the *planner's* cascade (`remove_break_rule`, D-4)
already removed the ref-matched delivery/dismissal/snooze rows in
the same batch — the 0004 no-FK convention (planner cascades, batch
mirror). `load()` reads `break_rules` into `ReminderState`. The
`reminder`-keyed tables (`deliveries`/`dismissals`/`snoozes`) store
the ref via `text_form` — the `break:` prefix rides the existing
TEXT columns, **no schema change** for the suppression tables
themselves.

### D-14 — rehydrate and restart

The open respite entry (D-3) is just an open row — the ADR 0017
crash-safety property covers it verbatim: a daemon restart
rehydrates the running break and the accumulator derives from
entries at the next tick. `track status` shows it (the chip's
newest-open pick is kind-blind — deliberate, D-18).

## 6. Surfaces — D-15…D-21

### D-15 — sync posture: device-local v1, roaming fenced

Break rules roam **not at all** in this chunk: `BreakRuleId` is
device-local (D-5), and the fail-closed dismissals filter gains
`| ReminderRef::Break(_) => None` (appdata.rs :1364-1370 — the new
variant must be *ruled off* the wire explicitly; the
`no_local_ids_reach_the_dismissals_wire` proptest extends to cover
it). Time entries themselves keep roaming — so every device
*evaluates* crossings from shared history, but only devices with
the rule configured fire it, and dismissal stays per-device.
Recorded consequence (honest, alarm-class): on a fleet, the kiosk
without the rule stays silent and desktop dismissal doesn't stop a
kiosk that also has the rule. Coherent roaming (a board-level
`break-rules` section + the D-4 widening for `break:` dismissal
keys) is a real follow-up but a section-recipe-sized one — fenced
as an issue idea for stage 3 to open, never snuck in here.

### D-16 — no new timers, no new config

The break evaluation rides the existing reminder tick
(`reminder_tick` → `due_reminder_firings`, actor.rs :1140-1173);
the durable-then-notify order, the storage-failure no-fire path,
and the withdraw direction are ref-generic and cover the new pass
unchanged. `[reminders] enabled=false` disables break reminders
with everything else (one switch, no new key).

### D-17 — the `breaks` CLI family (top-level, the timers recipe)

Break rules are standing device-local reminder rows — the
`timers` family's exact situation — so they get the same shape of
home rather than a hanging flag off `track` (entries) or a
task-anchored arm off `tasks remind` (which cannot express a
task-less rule without the D-4 disturbance):

```
taskboard breaks add --after <dur> [--label <text>] [--insistence normal|alarm]
taskboard breaks remove <id|≥8-char prefix>
taskboard breaks list
```

`--after 90m` mints `WorkThreshold` at the command layer (the
parse boundary); `add` prints the typed receipt (row + id);
`remove` resolves the selector like `resolve_alarm` (exact uuid →
unique prefix); `list` renders the rules table (label, threshold
phrase, insistence, id). Implementation = the two-lane recipe
(oneshot `with_core` wrappers + session `*_on` seams + one
`collapse()` definition site + typed receipts in output.rs +
`--json` envelopes). `route_for_ipc`: the new `Breaks` command →
`IpcOrLocal` (the daemon's engine owns the rows; a one-shot local
boot would orphan them — the timers precedent, byte for byte).

### D-18 — UI: two compiler-forced arms, no slint files

`presentation.rs::notification_rows` (ui :474-507) matches
`ReminderAnchor` exhaustively twice: the anchor filter arm —
`Break(_) => true` (shown; the anchor-card filter is about task
cards, a break row has no card to filter) — and the task mapping —
`Break(_) => None` (no Done/Mute buttons; Dismiss/Snooze carry the
ref + occurrence themselves, the alarm-row precedent, struct doc
:445-452). The row's title is the snapshot the domain assembled
(rule label) — carried, never re-derived. The status chip is
kind-blind (a running break *is* the honest running state; the
chip renders whatever `newest_open` picks). No slint markup
changes; the pure-fn unit tests extend.

### D-19 — kiosk posture

Unchanged fence: the notification surface is model-side gated on
kiosk (banner/rows per the R3 ruling); break notifications ride
that gate as every notification does. No kiosk-specific break
behavior; configuring rules remains a desktop/REPL surface.

### D-20 — export gains the kind column

CSV (default): `kind` appended as the **final** column
(`entry_id, task_id, task_title, start, end, duration_seconds,
kind`) — the additive-tail policy for positional consumers; golden
fixtures update (they are data, and this is a deliberate, recorded
contract extension). JSON rows gain `"kind"` naturally. Filters and
ordering untouched.

### D-21 — receipts and output

`TrackStartReceipt`/`TrackLogReceipt`/`TrackAdjustReceipt` grow the
kind datum; `BreakRuleCreated`/`BreakRuleDeleted`/`BreaksList`
receipts land in output.rs beside their timers siblings. Text
renders extend; the `--json` envelopes stay typed. **No text
assertions in tests anywhere** — structural/typed assertions only
(AGENTS §5).

## 7. Decision record

| ID | Question | Decision | Status |
|---|---|---|---|
| D-1 | Flag or kind on `TimeEntry` | **`TimeEntryKind` enum** (`Work` default, `Respite`), `#[serde(default)]` field — the §7 vocabulary reservation names kinds plural; a bool forks fold/wire plumbing per flag | recorded |
| D-2 | Wire decode of the kind | Absent key → `Work` (default-off); modeled value round-trips; **unmodeled value → whole row `Verbatim`** (never a silent downgrade); emit always writes the key; union/GC need no code change; fold copies kind in overwrite + adoption | recorded |
| D-3 | Which track verb gains the marking | **`--kind <work\|respite>` on `start`, `log`, `adjust`** (adjust: omit = unchanged); clap value-parser rejection at the boundary; receipts/list rows carry the datum; `start --kind respite` opens the *running break* | recorded |
| D-4 | Rule surface: grammar extension vs discriminator | **Trigger-source discriminator — a dedicated `BreakRule` family** entering via `ReminderRef::Break`/`ReminderAnchor::Break`; NOT a `trigger:` field on `ReminderRule` (task-anchored by construction; extension forces `Option<TaskId>` through every arm — the forbidden disturbance) | **decided (this plan's central ruling)** |
| D-5 | Break rule identity home | `BreakRuleId` in the **device-local** id family (the `AdhocAlarmId` precedent); `ReminderState.break_rules` map; no `updated_at` (no rule-edit verbs, no LWW competition) | recorded |
| D-6 | Threshold typing | `WorkThreshold` private-field newtype, `try_new` (> 0) at the command boundary, `InvalidWorkThreshold` typed rejection, text-form grammar = the existing duration phrases | recorded |
| D-7 | Undo posture | `CreateBreakRule`/`DeleteBreakRule` → `NotUndoable(ReminderSurface)` (the rule/alarm arm); adjust-kind rides the existing closed-entry pre-image free; no new undo machinery | recorded |
| D-8 | Occurrence key of a duration trigger | **The threshold-crossing instant** — stable while history is stable, re-arms on respite reset, composes with the `(reminder, occurrence)` set-union; tick-stamp ruled out (per-tick double-fire), streak-start ruled out (nonsense). Streak boundary: closed respite → its `end`, open respite → its `start`; accumulation = overlap-sum over `(boundary, now]`; history edits move the base honestly (due-date-move precedent) | **decided (the double-fire guard's load-bearing ruling)** |
| D-9 | Trigger fn surface | `accumulated_work` + `break_crossing` (pure, `tracking.rs`) + `break_due_firings` (pure, `reminders.rs`) composed into `due_firings`/`active_notifications` as a third pass; signatures grow `tracking: &TrackingState` (four compiler-forced call sites); eligibility shape identical to the task pass; `(occurrence, reminder)` total order preserved | recorded |
| D-10 | When-only contract | The break pass reads no insistence/urgency; payload label + insistence snapshot in `notifications_for`'s new anchor arm; missing rule row → payload skipped (totality rule) | recorded |
| D-11 | Tracked-today semantics | **Work-only**: `work_in_window` filters `Work` then delegates to the unchanged `tracked_in_window`; `track status` switches to it; export keeps per-row durations | recorded |
| D-12 | Local schema | **Migration 0018** (additive; the dispatch's no-impact expectation refuted — the table is strictly columnar): `ALTER TABLE time_entries ADD COLUMN kind TEXT NOT NULL DEFAULT 'work'` (the backfill IS the default-off decode) + `CREATE TABLE break_rules`; numbering unclaimed by the in-flight siblings (verified), re-verify at implementation base | **decided, with the expectation correction recorded** |
| D-13 | Persistence actions | `UpsertBreakRule`/`DeleteBreakRule` (exhaustive match forces the arms); planner-side cascade via `remove_break_rule` (the `remove_alarm` shape); suppression tables ride existing TEXT `reminder` columns via the `break:` text-form prefix — no schema change there | recorded |
| D-14 | Restart | Nothing new: the open respite row is the ADR 0017 crash-safe intent verbatim; accumulator derives at tick time | recorded |
| D-15 | Sync posture | **Device-local v1**: rules never roam; dismissals filter gains the explicit `Break ⇒ None` fail-closed arm + proptest extension; entries keep roaming so crossings evaluate everywhere; fleet consequence recorded; coherent roaming fenced as a follow-up issue idea | **decided** |
| D-16 | Timers & config | No new tick, no new timer, no `[reminders]` key — rides the existing tick under `enabled`; "nothing ticks" preserved (crossing derived at tick time) | recorded |
| D-17 | Rule CLI | **Top-level `breaks` family** (`add --after <dur> [--label] [--insistence]`, `remove <sel>`, `list`), the timers two-lane recipe, `IpcOrLocal` | recorded |
| D-18 | UI delta | Two compiler-forced exhaustive-match arms in `notification_rows` (shown; `task: None`); title carried from the snapshot; chip kind-blind; no slint markup | recorded |
| D-19 | Kiosk | Existing notification gate untouched; rule config stays desktop/REPL | recorded |
| D-20 | Export contract | CSV gains `kind` as the final column (additive-tail policy, golden fixtures updated); JSON gains the field | recorded |
| D-21 | Output | Receipts grow kind / new break receipts in output.rs; typed/structural assertions only | recorded |
| D-22 | Chunk 3.3 lane overlap | 3.3 (in flight) touches `reminders.rs`/`persistence.rs` heavily but carries **no migration**; 0018 is unclaimed. Stage 2 gates on the lane rule (never two domain/state chunks in flight) and rebases onto 3.3's landed PR if it lands first (§10 R-1) | recorded |

## 8. Test catalogue (TDD-first: each phase opens with its failing tests)

**Phase N.1 — domain: kind, wire, fold, projections**

- `time_entry_kind_serde_unit_names_roundtrip` — kebab unit names are
  the wire/storage text (structural).
- `old_entry_json_without_kind_decodes_work` — the default-off
  entity-decode pin (old journal/receipt tolerance).
- `old_wire_row_without_kind_decodes_entry_with_work` /
  `respite_wire_row_roundtrips` /
  `unmodeled_kind_string_demotes_the_whole_row_to_verbatim` — the
  D-2 wire pins (the no-silent-downgrade pin is the load-bearing
  one).
- `overlay_emits_the_kind_key` + fold pins:
  `lww_overwrite_carries_kind`, `adopted_row_carries_kind`; union
  pin: `kind_participates_in_clock_competition`.
- `work_in_window_matches_the_kind_filtered_oracle_prop` (proptest:
  oracle = `tracked_in_window` over `Work`-filtered entries) +
  `respite_only_window_is_zero`.
- **The mandated proptest laws over the trigger fn**:
  - **L-1 accumulation monotonicity**: fixed entries, `t1 ≤ t2` ⇒
    `accumulated_work(t1) ≤ accumulated_work(t2)`.
  - **L-2 respite resets to zero**: work entries wholly before a
    closed respite, no work after ⇒ `accumulated_work(after) = 0`
    (strict form with disjoint fixtures; the overlap-sum general
    form is the oracle property below).
  - **L-3 no firing below the threshold**: `accumulated_work(now) <
    threshold` ⇒ `break_crossing(...) = None` ⇒
    `break_due_firings` empty for that rule.
  - L-4 crossing stability: `crossing(t) = Some(c)` ⇒
    `crossing(t') = Some(c)` for `t' ≥ t` (fixed entries).
  - L-5 accumulated-vs-crossing oracle: for any entries/threshold,
    `crossing = Some(c)` ⇔ `accumulated_work(c) ≥ threshold` and
    `accumulated_work(c − ε) < threshold` (the exact first-touch
    point; ε via a sub-second probe — determinism by construction,
    no sleeps).
  - L-6 window-oracle composition: `accumulated_work(now)` equals
    the `tracked_in_window` oracle over `(boundary-walk, now]` with
    kind filtering (one definition of clipping, two doors).
- Deterministic units: closed-respite boundary = its end; open
  respite boundary = its start (the monotonicity-protecting
  anchor); multiple respites walk-order semantics; retroactive
  respite moves/retracts the crossing; inverted/degenerate spans
  contribute zero (the clamp); crossing inside an open entry.

**Phase N.2 — domain: break rule family + evaluator composition**

- `below_threshold_no_firing` / `at_threshold_single_firing_with_crossing_occurrence`
  / `second_tick_does_not_refire_the_crossing` (**the double-fire
  guard pin**) / `respite_reset_then_recross_fires_the_new_occurrence`
  (the re-arm pin) / `dismissed_crossing_never_refires` /
  `snoozed_crossing_moves_via_the_chain` /
  `two_rules_fire_independently_in_total_order`.
- `empty_break_rules_leaves_due_firings_unchanged` (the existing
  property suite keeps passing untouched — the extension pin) +
  the compaction/settled-prune unobservability properties re-run
  with break rules present.
- `notifications_for_break_anchor_snapshots_label_and_insistence` /
  `missing_break_rule_row_skips_the_payload` (totality).
- Planner: `create_break_rule_mints_and_stamps` /
  `zero_or_negative_threshold_rejected_typed` /
  `delete_break_rule_cascades_ref_matched_rows` /
  `delete_repeat_is_no_op` / `delete_stranger_is_unknown_break_rule` /
  `dismiss_and_snooze_resolve_break_refs` (planner arms, typed
  assertions) / `adjust_kind_rides_the_existing_pre_image` /
  `break_rule_commands_are_not_undoable` (structural
  `UndoFacts` assertion).
- Fail-closed: `break_dismissals_never_reach_the_wire` (extends the
  existing P2/pins; structural — the emission map contains no
  `break:` keys for arbitrary local state).

**Phase N.3 — storage**

- Migration upgrade: a pre-0018 database (the 0008 recipe's
  fixture shape) opens; entries decode `Work`; `break_rules` loads
  empty (**the local default-off pin**).
- Codec roundtrips: entry `Work`/`Respite`; break-rule row
  (threshold seconds, insistence); reminder-ref text roundtrip
  including `break:` through deliveries/dismissals/snoozes rows.
- `apply_actions` upsert/delete break rules in one transaction;
  the ≤1-delivery-per-`(reminder, occurrence)` property extends
  over `Break` refs.
- Rehydrate: engine restart over a repo holding an open respite
  entry + a break rule reproduces the firing state (deterministic,
  join/predicate waits — no sleeps).

**Phase N.4 — app: CLI marking + breaks family**

- `track start/log --kind respite` → typed receipts carry the kind;
  `adjust --kind` change-mask semantics (omit = unchanged) —
  hermetic tempdir boots, the cli_track.rs shape.
- `--kind sabbatical` is a boundary parse rejection (structural
  exit/outcome, no text assertions).
- `breaks add/remove/list`: collapse total for direct calls; typed
  receipts; `--json` envelopes; the IPC relay leg asserts the lane
  (`Breaks` → `IpcOrLocal` — structural route assertion).
- `status` tracked-today excludes respite (the D-11 pin through
  the CLI view); `list` rows carry kind; export golden CSV/JSON
  fixtures updated for the `kind` column (golden files are data).

**Phase N.5 — ui/escalation + gates**

- `notification_rows` Break anchor: shown, `task: None`, title =
  rule label snapshot (pure-fn units, structural).
- Escalation driver: a Break-anchored `alarm` row re-toasts/
  sounds per the ladder with no task affordance (structural).
- Insta snapshots grow (`break_rules` map, entry `kind` fields,
  receipts) — persisted dumps only.
- Gates: the full AGENTS §10 sequence **through
  `scripts/run-quality-gates.sh`** (flock wrapper — 3.3 may hold
  the box); `cargo fmt --check`, clippy `-D warnings`, nextest,
  doc tests, `cargo deny`, the three-command coverage sequence
  with the quarantine step, trailing `rm -rf
  target/llvm-cov-target`.

## 9. Phasing & commit plan (one branch, N.1 → N.5 in order)

| Phase | Delivers | Commit shape |
|---|---|---|
| N.1 | `TimeEntryKind` + wire/fold/overlay + `work_in_window` + `accumulated_work`/`break_crossing` + all six laws | `feat(domain): the respite kind and the work-streak projections` |
| N.2 | `BreakRule` family, ref/anchor variants, planner arms, `break_due_firings` composition, fail-closed arm | `feat(domain,state): break rules and the duration trigger` |
| N.3 | Migration 0018, codec, actions, rehydrate | `feat(storage): respite column and break-rule table` |
| N.4 | `--kind` marking, `breaks` family, receipts, export column, IPC lane | `feat(app): respite marking and the breaks surface` |
| N.5 | UI/escalation arms, snapshots, full gates | `feat(ui-slint): break notification rows` (+ fixups, none with duplicate subjects — commit hygiene for the rebase merge) |

TDD order inside each phase: laws/pins first (red), then the
production change (green), then the property/roundtrip sweep.
`AdjustTimeEntry`'s `kind` mask and the wire row land in N.1 so the
fold and the marking never drift; the CLI flag lands in N.4
against the finished mask.

## 10. Risks & mitigations

- **R-1 — lane collision with chunk 3.3** (in flight:
  `reminders.rs` +352, `persistence.rs` +131, the stale-delivery
  sweep): both chunks grow the evaluator's neighborhood. Mitigation:
  the dispatch's same-crate lane rule (3.3/3.5 never in flight —
  stage 2 verifies before writing code); if 3.3 lands first, rebase
  onto it (its sweep prunes *unconsulted receipts by age* —
  orthogonal to the crossing-key semantics; the property suites
  compose); if 3.5 lands first, 3.3 rebases. **Never** a semantic
  merge of the two passes' predicates.
- **R-2 — the double-fire hazard**, mitigated by construction (D-8):
  the occurrence is a derived stable instant, not the tick stamp;
  the pin is `second_tick_does_not_refire_the_crossing`. The
  residual: history edits legitimately re-arm under a new key —
  documented semantics, not a defect.
- **R-3 — mixed-fleet kind downgrade**: a pre-kind device re-emits
  a respite row without the key (its decode drops what it cannot
  model); a round-device's newer-clock write re-establishes the
  mark. One-version tolerance, the insistence precedent; recorded,
  accepted; the follow-up (verbatim relay) is every-section-wide,
  not this chunk's.
- **R-4 — orphaned suppression rows** after history edits or rule
  deletion-after-fire: bounded by edit frequency; rule deletion
  cascades (D-4); age-based GC is exactly chunk 3.3's lane —
  fenced there, not duplicated here.
- **R-5 — the open-respite boundary choice** (start, not end) is
  the one place a "reasonable alternative" exists; it is forced by
  law L-1 (a moving boundary breaks monotonicity) and pinned by a
  deterministic unit — if review prefers different streak
  semantics, it is one fn's boundary clause, isolated.
- **R-6 — insta snapshot churn** across the suite (PersistedState
  grows a map; entries grow a field): mechanical, persisted dumps
  only, updated in N.3/N.5, never hand-waved as "expected diff".

## 11. Size, handoff, coordination

- **Estimated diff**: ≈ 1.4–1.8k lines including tests — domain
  ~700 (tracking.rs ~340 incl. laws, reminders.rs ~450, appdata.rs
  ~170, merge/ids/messages/command/persistence/undo ~250), storage
  ~230 (0018.sql + repo + codec), app ~450 (cli.rs + new
  commands/breaks.rs + track.rs + output.rs + routes), ui/escalation
  ~40, fixtures/snapshots ~a dozen files. **M** — consistent with
  the dispatch table's sizing.
- **Dependencies**: none outside the workspace (deny untouched).
  Gates run under the flock wrapper.
- **Handoff**: stage 2 implements N.1–N.5 on
  `feat/respite-break-reminders` in
  `../taskboard-WT-feat-respite-break-reminders`; stage 3 reviews,
  runs the gates, opens the PR closing #493 + #495, and opens the
  two fenced issue ideas (roaming break-rule config + dismissals;
  nothing else). Worktree removal is stage 3's final act (doctor
  rules; `--force` per the lifecycle doc). **No push without the
  user's instruction during planning** — this commit stays local.
