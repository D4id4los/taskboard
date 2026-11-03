# [Plan] GUI Review Round 4, Chunk 8 — Task Reminders That Fire

- **Date**: 2026-11-03
- **Status**: PROPOSED — stage-1 plan for chunk 8 of the round-4
  dispatch. Branch `feat/ui-r4-task-reminders`, cut from `origin/main`
  (3aa7cb96; `scripts/worktree-doctor.sh` passed pre-creation, 228 G
  free against the 25 G floor). Both gates verified: chunk 6 (insert
  engagement, `198fa27e`) is on `origin/main`, and **no** migration-
  carrying chunk is in flight (zero open PRs; the backlog round-2
  chunks that touched adjacent domain files — 1.9
  `feat/undo-tracking-habit-arms`, 3.5 `feat/respite-break-reminders`,
  4.2 `feat/tracker-task-kind` — have all rebase-merged; the local
  branch tips are pre-rebase originals, the #415 leftover pattern, not
  in-flight work). Decision gate: round-4 §2 decision 2 (at/in reminder
  kind) **CONFIRMED by the user 2026-11-01** (the origin plan's §2
  header records all six decisions confirmed; tripwire passed).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.8 (D29–D31), §2 decision
  2, §1 findings R4-F23-part/F25, §7-R5; dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§9 (acceptance a–g; code-quality mandate). Fences read: the
  backlog round-2 dispatch §1 rule 4 (migrations) and §1 rule 3
  (same-crate gates).
- **Migration number amendment (binding)**: the design §3.8 and the
  dispatch table say "migration 0018 — the next free number per the
  backlog dispatch's fence". The fence itself rules the number is
  taken **at the chunk's rebase** as "the next free number". Since the
  plan was written, migrations 0018_respite_break_rules (chunk 3.5)
  and 0019_tracker_task_kind (chunk 4.2) have landed on `origin/main`.
  This chunk's migration is therefore **0020** — same content, shifted
  number; every mention of "0018" below reads "0020".
- **Scope**: the round's only crate-crossing chunk —
  `taskboard-domain` (kind, planner, evaluator, settlement, document
  fold), `taskboard-storage-sqlite` (migration 0020 + codec/repo),
  `taskboard-sync-nextcloud` (only if a fold/flush pin needs touching;
  expected near-empty), `taskboard-state` (the paused-clock journey
  tests only — the tick lanes are kind-agnostic), `taskboard-ui-slint`
  (grammar, rows, un-mute), `taskboard-app` (mechanical CLI fold).
  Non-goals (dispatch §9): GUI insistence picker (D31c — backlog
  issue), escalation ladder changes, timer surfaces (chunk 9 owns
  D32–D34 — but the shared countdown/ISO formatter lands HERE per
  D30/D32), tracking surfaces (chunk 10). CLI at/in grammar: not in
  the acceptance list — the CLI stays due-offset only (§7).
- **All `file:line` anchors are this worktree at 3aa7cb96** (post
  chunks 6/7). The dispatch prompt's anchors predate those chunks and
  drifted (e.g. `AddReminderRule` planner 1928 → 1978; enqueue pin
  6356 → 6458; `parse_timer_when` 788 → 1023; `countdown_phrase`
  472 → 531; mute-path 384 → 412). Every anchor below was read, not
  carried over.

## 0. Findings → mechanisms audit (what the code confirms)

- **F25 ("manually set reminders never fire and remain in the list
  forever")** — root cause confirmed as the design says: a rule is a
  due-anchored offset (`ReminderRule.offset`, `reminders.rs:294`;
  `occurrence_of` at `reminders.rs:869-871` adds the ≤ 0 offset to the
  task's due date) and `is_remindable` (`reminders.rs:1179-1181`)
  requires `duedate.is_some()` — so a rule on an undated task has no
  occurrence at all (`base_occurrence`'s `Rule` arm,
  `reminders.rs:934-937`, returns `None`), and the evaluator never
  sees it. Rules are only removed manually (`RemoveReminderRule` /
  `ClearReminderRules`, `command.rs:2025-2062`); no settlement exists
  for them. The GUI also has three aggravators the design folds into
  this chunk: mute is one-way from the GUI (every mute path sends
  `muted: true` — `router.rs:417`, `:879`; the un-mute command and the
  CLI `--off` lane, `app/src/commands/reminders.rs:291-299`, already
  exist), the rule list shows only the authored offset and the created
  date (`rule-list.slint:18-21`, `models.rs:1439-1446`) — never
  whether the rule is armed — and the derived-fact hint
  (`presentation.rs:396-404`) says nothing about needing a due date.
- **F23-part ("timers don't count down; show d/h/m/s and the fixed
  fire time in ISO `YYYY-MM-DD HH:MM:SS`")** — applies to the new
  `At` rule rows (D30). The existing `countdown_phrase`
  (`presentation.rs:531-543`) is minute-bucketed and text-shaped —
  wrong precision, wrong shape. D30/D32 require a structural d/h/m/s
  countdown + an ISO-ordered local fire time; the pure formatter lands
  here (chunk 9's D32 adopts it for timers).
- **The engine lanes are healthy and kind-agnostic** — D31b: the tick
  (`actor.rs:557-596`, 30 s cadence re-arm at `:595`), the
  durable-then-notify tick body (`reminder_tick`,
  `actor.rs:1196-1246`: evaluate → persist delivery markers → only
  then notify; a failed batch fires nothing), the post-batch settle
  (`persistence.rs:633-640`: `compact_snooze_chains` then
  `prune_settled_alarms`, caller order load-bearing), and the
  stale-delivery sweep (`reminders.rs:801-839`) are all generic over
  `ReminderRef`. Round-3 chunk 7 probe-verified them (2026-10-28); the
  chunk still pins the end-to-end `At` journey (§8 T6).
- **The fences the design names are landed facts** — backlog 3.5
  (respite/break rules, migration 0018) and 4.2 (tracker task kind,
  migration 0019) are merged; their code is in the base this chunk
  cuts from. No rebase friction remains from them; the fence that
  matters now is the migration one (one migration-carrying chunk in
  flight — this chunk is it; zero competitors).

## 1. Decisions (all settled here; alternatives recorded)

### D-1. The kind enum: enum-with-data over the rule row; `AtInstant` per house A1

`ReminderRule` keeps its struct shape (id, task, insistence,
created_at, updated_at — the LWW clock fields stay uniform across
kinds, which a rule-as-enum shape would duplicate) and its `offset`
field is **replaced by**:

```rust
/// How a rule decides its fire instant ...
pub enum ReminderKind {
    /// Today's shape: fire this far before the task's due date.
    DueOffset(ReminderOffset),
    /// Fire at this absolute instant — no due date involved.
    At(AtInstant),
}
```

(`reminders.rs:294` `pub offset: ReminderOffset` →
`pub kind: ReminderKind`.) `ReminderKind` re-exports the two arms' data
types verbatim; `ReminderOffset` itself is untouched (its mint, text
form, serde tuple, and `Default = at_due` all stand).

**`AtInstant`** — the hand-rolled private-field newtype (house A1, no
derive crates), mirroring `ReminderOffset`'s discipline exactly:

```rust
pub struct AtInstant(DateTime<Utc>);
```

- **Mint** `AtInstant::whole_second(dt: DateTime<Utc>) -> Result<Self,
  InvalidAtInstant>`: rejects sub-second instants
  (`dt.timestamp_subsec_nanos() != 0`). Rationale: the ReminderOffset
  rationale verbatim (`reminders.rs:24-31`) — every persistence lane
  (the sqlite epoch-seconds column, the document wire) stores whole
  seconds; a sub-second instant would diverge from its persisted form
  on reload and re-fire a delivered occurrence. `InvalidAtInstant`
  is a `thiserror` unit enum (one variant `SubSecond`), the
  `InvalidReminderOffset` shape.
- **Deliberate leniency, documented at the type**: past instants are
  legal data (fire on the next tick — the `AdhocWhen::At` doc,
  `reminders.rs:315-317`). No range guard: every `DateTime<Utc>` fits
  the i64-seconds column. No validate-and-forget: the mint is the only
  construction path and the serde decode re-mints (the offset's
  serde-lane precedent, `reminders.rs:166-178`).
- **Serde**: hand-written as `(i64,)` epoch seconds — the offset's
  custom-tuple precedent, pinning the wire shape against chrono
  default drift. (The whole `ReminderRule` is not serde-serialized in
  any production lane — the sqlite mirror uses columns, the document
  uses the hand-built `RuleEntry`, the journal stores commands — but
  the derive exists and stays honest.)
- Accessors: `as_datetime()`, `text_form()` (the ISO-ordered
  `YYYY-MM-DD HH:MM:SS` UTC form used by receipts/lists; UI-local
  rendering is the presentation layer's job, §6-D-6).

Alternatives recorded: rule-as-enum (`ReminderRule::DueOffset{..} |
At{..}`) — rejected: duplicates the clock/insistence fields and every
match down the codec; `At(DateTime<Utc>)` bare — rejected by the
design itself ("a minted instant newtype per the house A1 pattern"):
the bare type validates nothing and invites sub-second data no storage
lane can hold.

### D-2. Command surface: one verb, the add command grows an additive `at` input

**Grow `AddReminderRule`, do not add a verb.** The recorded reason:
add is already a one-call, dedup'd, flush-triggering, undo-invertible
arm (`command.rs:1978-2024`); a second verb would fork the dedup, the
enqueue pin, the undo arm, the document tombstone derivation, and the
CLI/IPC surface for no semantic gain.

The payload change is constrained by a hard fact the design round
could not see: **`StateCommand` is the undo journal's persisted payload**
(`journal.rs:55` `command: StateCommand`; the payload decodes at load,
`repo.rs:1147`, and a decode failure is `RepositoryError::Corrupted` —
a pre-change journal leaf with the old shape must keep decoding, or a
database load fails). The shape is therefore **additive, never a
rename**:

```rust
AddReminderRule {
    task: TaskId,
    /// Legacy field, kept for the journal/IPC wire: the due-anchored
    /// offset. Read iff `at` is None.
    offset: ReminderOffset,
    /// The at/in input (r4 §2 decision 2). Absent (the default) =
    /// the legacy due-offset shape.
    #[serde(default)]
    at: Option<AdhocWhen>,
    insistence: Insistence,
}
```

- `AdhocWhen` is reused, not duplicated (`reminders.rs:313-322`) —
  the CreateAdhocAlarm precedent: `At(DateTime<Utc>)` carries a minted
  instant; `In(TimeDelta)` lets the UI say "in 10m" **without reading
  a clock** ("absolute-vs-duration is a caller concern, never a domain
  concern — the planner stamps `In`").
- Planner (`command.rs:1978-2024`): normalize the input first —
  `None` → `ReminderKind::DueOffset(offset)`; `Some(At(t))` →
  `At(AtInstant::whole_second(t)?)` (a sub-second input is a new typed
  rejection `CommandError::InvalidReminderRuleAt` — no silent
  truncation); `Some(In(dur))` → `dur ≤ 0` is the same typed
  rejection; `now + dur` saturating at `MAX_UTC` (the Snooze/
  CreateAdhocAlarm precedent, `command.rs:1891-1895`, `:2075-2079`) →
  `At(...)` mint. **Then** the dedup: today's `(task, offset)` key
  (`command.rs:1986-2002`) becomes `(task, kind)` — the message doc
  (`messages.rs:239-256`) updates its soundness argument to
  "(task, kind) fully determines firing semantics" (an `At` rule's
  occurrence is pure in the instant; insistence stays flavour). The
  minted rule stores `kind`; `created_at = updated_at = now` (D9
  stamps unchanged). The enqueue rides unchanged (`command.rs:2017`) —
  the update pin test names the shape (§8 T2).
- Old payloads (journal leaves, IPC peers) decode with `at = None` →
  `DueOffset(offset)` — byte-compatible. New payloads on an old
  binary: not a supported matrix (single binary ships both sides).

Alternatives recorded: a new `AddReminderRuleAt` verb — rejected
(forked dedup/settle/undo/doc lanes, §1 opening); renaming `offset` →
`kind` — rejected: breaks every persisted journal leaf at load
(the Corrupted discipline, `repo.rs:1147`) and the IPC wire; reusing
`AdhocWhen` as the WHOLE command kind (dropping `offset`) — rejected
by the wire constraint, same as the rename.

### D-3. Evaluator: the `At` pass beside the alarms pass, not a task-loop restructure

The design says the `At` arm fires "independent of
`is_remindable`'s duedate requirement". The minimal-diff, byte-
compatible shape mirrors the break pass (`break_firings`,
`reminders.rs:976-998`) — a **third, dedicated pass** shared by both
readers via the `delivered: bool` parameter:

```rust
fn at_rule_firings(tasks, reminders, now, delivered: bool) -> Vec<ReminderFiring>
```

- **Gate**: the rule's task must be live, open, and unmuted — checked
  per rule via `tasks.get(&rule.task)` (a rule whose task is missing
  is skipped; `DeleteTask`'s D-c cascade makes that unreachable, the
  totality guard). **The due-date requirement is dropped** — that is
  F25's root cause. Live/open still gate: an `At` reminder on a done
  task is noise (the design drops only the "duedate requirement").
  Mute still applies: mute is task-shaped ("stop nagging") and D31's
  un-mute affordance exists precisely to reverse it.
- **Occurrence**: `walk_chain(rule.at_instant, Rule(id), snoozes)` —
  the kind-aware `base_occurrence` (`reminders.rs:934-937` grows the
  match on `rule.kind`: `DueOffset` → today's `occurrence_of`
  arithmetic; `At` → `Some(instant)`), so snoozes compose through the
  existing lane. `occurrence_of`'s doc updates ("given the task's due
  date — `At` rules ignore it").
- **Firing set**: `effective <= now && is_delivered == delivered &&
  !is_dismissed` — the family gate verbatim (`due_firings`'s
  task loop, `:1038-1050`). Delivery-set semantics are the alarm
  precedent, no new machinery: the receipt key is
  `(Rule(id), instant)`; the tick persists markers before notifying
  (durable-then-notify, `actor.rs:1231-1245`) so nothing double-fires;
  a repeated tick reads `is_delivered = true` and skips. **No-due-date
  independence** falls out of the pass's own gate.
- **Ordering**: the pass's firings are appended into the existing
  `sort_by_key((occurrence, reminder))` (`:1071`, `:1143`) — the total
  order is preserved; no ordering question with the offset pass
  survives the sort.
- **Call sites**: appended in `due_firings` (`:1070`-adjacent, next to
  the break extend) and `active_notifications` (`:1142`-adjacent).
  The engine consumes both unchanged (`core.rs:124`,
  `actor.rs:1212`).
- **Consulted-row parity**: `stale_deliveries`' `unconsulted_task`
  (`reminders.rs:845-862`) calls `is_remindable` — for an `At` rule's
  delivery row that would wrongly declare the row unconsulted (the
  task's missing due date) and age it out while the receipt still
  guards the toast. The `Rule` arm becomes kind-aware: `DueOffset`
  rules keep today's gate; `At` rules gate on live + open + unmuted
  only (the D-3 pass's gate, factored into one helper both call —
  one definition of At eligibility, the break pass's discipline).

Alternatives recorded: restructuring the per-task loop (drop
`is_remindable` to live+open, let occurrence arithmetic `None`-skip)
— rejected: it silently changes the gate for every family in both
readers plus `unconsulted_task` for zero behavior gain, and blurs
which gate protects which family; a per-task candidates split —
rejected: same blast radius with more branching.

### D-4. Settlement: planner-side delete at the settling dismiss (tombstone first), post-batch prune as backstop

The design's one-line settle — "fired+dismissed `At` rules prune (the
adhoc precedent `prune_settled_alarms`)" — hides a fork the adhoc
precedent does not have: **`At` rules are synced** (the document's
`rules` section, D-S9) while adhoc alarms are device-local. A bare
post-batch prune that follows `remove_alarm`'s cascade
(`reminders.rs:712-718`: row + deliveries + dismissals + snoozes) is
**unsound**: the document still carries the live rule entry, the next
fold adopts it unconditionally (`merge.rs:810-821`'s `None` arm), and
the pruned receipts are gone — so the re-adopted rule re-fires. The
settled decision, two mechanisms:

1. **The settling dismiss plans the removal.** In the
   `DismissReminder` planner arm (`command.rs:1920-1977`), when the
   ref is `Rule(id)`, the rule row is `At`-kinded, and the dismissed
   occurrence equals its current effective occurrence (the settle
   condition — a stale/base occurrence dismissal does NOT settle; the
   snooze-walk decides), the planner additionally plans
   `PersistenceAction::DeleteReminderRule(id)` +
   `enqueue(LocalOp::UpdateAppData(task))`. The flush then derives the
   document **tombstone** stamped with this instant (the
   RemoveReminderRule shape, `command.rs:2035-2043`) — the rule dies
   everywhere (a tombstone removes a local rule iff strictly newer,
   `merge.rs:824-836`), no device can re-adopt it, and the local
   receipt cascade is safe because the rule can never return. The
   dismissal itself still syncs through the document's dismissals
   section (unchanged) — belt and braces for any device that folds the
   dismissal before the tombstone packet.
2. **The post-batch backstop prunes row-only.** `prune_settled_at_rules`
   runs in `apply_actions` immediately after `prune_settled_alarms`
   (`persistence.rs:640` — the caller-order discipline holds:
   compaction first, alarms, then `At` rules). Condition mirrors
   `prune_settled_alarms` (`reminders.rs:746-761`): kind is `At` **and**
   the effective (snooze-walked) occurrence is delivered **and**
   dismissed. It removes **only the rule row** — deliberately NOT the
   receipts. Recorded rationale: the backstop exists for shapes the
   planner path can't have ordered (a synced-in dismissal folded by a
   batch, degraded/offline corners); keeping the receipts makes it
   re-fire-proof under any later re-adoption (`delivered ∧ dismissed`
   blocks the evaluator forever), and the leftovers age out through
   the existing sweeps (row-less refs are `unconsulted` for
   `stale_deliveries` after the 90-day horizon; `compact_snooze_chains`
   owns the snooze links). Idempotent by construction; property-pinned
   to never prune a live `At` rule or any `DueOffset` rule (§8 T4).

**What remains in the list, deliberately**: a `DueOffset` rule never
settles (it re-arms with due changes — the design's "persist by
design"); an `At` rule on a task that is marked done (or deleted —
the D-c cascade owns deletion) before its instant never fires, never
settles, and persists until manual removal or task deletion — the
same persist-by-design class, recorded here so review does not call
it a leak. The row's UI presentation (§6-D-6) makes both states
visible instead of silently stale (the F25 lesson).

Alternatives recorded: post-batch prune with the full cascade —
rejected as unsound above (the re-fire trace); post-batch prune
without cascade as the ONLY mechanism — rejected: every sync cycle
re-adopts the row and re-prunes it (observable churn, upsert report
noise); UI-hides-settled-rows without pruning — rejected: contradicts
§2 decision 2 ("settles after fire+ack like adhoc alarms") and leaves
the storage row forever (the F25 storage half); tombstone-only at
dismiss without the backstop — rejected: leaves folded-dismissal
corners unpruned and untested as a class.

### D-5. Snooze: the existing lane, verbatim

`SnoozeReminder` resolves `Rule(id)` → rule row → task →
`live_task` guard → `RecordReminderSnooze` (`command.rs:1850-1904`) —
all kind-blind. An `At` rule snoozes through it unchanged: the link
keys on `(Rule(id), dismissed-or-fired occurrence)`, the target is
planner-stamped (`now + postpone`, saturating), and the effective
occurrence walks the chain (D-3). The in-app snooze verb on a fired
`At` notification needs no new code (the banner/target machinery is
ref-keyed, `router.rs:404-411`). Settlement composes: snoozed, then
dismissed at the target → the settle condition (D-4.1) reads the
walked occurrence → settles. Recorded edge: snoozing an `At` rule
whose task has since been done/died is the existing typed rejection
(the `live_task` guard) — unchanged for both kinds.

### D-6. UI grammar: one shared prefix parse, extraction not duplication

The rule-add field grows from bare-offset-only to the three-form
grammar; the *at-instant* arm must have exactly one definition.
Where it lives: **`taskboard-ui-slint/src/presentation.rs`** — the
already-sanctioned home of `parse_timer_when` and the r4 D27 validity
probes; the domain must not learn the local-midnight/tomorrow-rollover
rules (they are presentation policy, and the ui crate owns
presentation).

- Extract the `at` arm of `parse_timer_when`
  (`presentation.rs:1035-1062`) verbatim into
  `fn parse_at_instant(rest: &str, now: DateTime<Utc>, tz:
  &impl TimeZone) -> Option<DateTime<Utc>>` (RFC-3339 any-offset →
  `YYYY-MM-DD` host-local midnight → `HH:MM[:SS]` today,
  rolled to tomorrow when past; DST-unresolvable → `None`).
  `parse_timer_when` calls it; nothing else changes in it — its tests
  keep passing untouched.
- New `pub fn parse_rule_when(raw, now, tz) -> Option<RuleWhen>` where

  ```rust
  pub enum RuleWhen { DueOffset(ReminderOffset), At(AdhocWhen) }
  ```

  prefix-dispatched like `parse_timer_when` (`:1028`): `"in <dur>"`
  → `At(In(dur))` (positive duration — the domain parser's ≤ 0
  magnitude is `None`, the same seam refusal as `:1030-1034`);
  `"at <t>"` → `At(At(parse_at_instant(...)))`; **anything else**
  → `DueOffset(ReminderOffset::parse_text_form(trimmed))` (bare
  `"10m"` stays due-anchored — the design's parity rule). Empty →
  `None`.
- The **exactly-one claim, stated for review focus (d)**: one
  at-instant parser (`parse_at_instant`), one in/bare duration
  grammar (the domain's `ReminderOffset::parse_text_form`), one
  prefix dispatcher per consumer grammar (`parse_timer_when` for
  timers, `parse_rule_when` for rules — they differ in the bare
  fallback, which is the feature). The grep contract: no second
  `"%H:%M"` / RFC-3339 / `parse_text_form` call site outside these.
- The custom field's validity rides chunk 7's wiring verbatim:
  `offset_validity` (`:1086-1091`) → replaced by
  `rule_when_validity(text, now, tz)` (the `timer_when_validity`
  shape, `:1097-1106`), pushed through the existing
  `offset-validity`/`offset-error-hint` properties
  (`rule-list.slint:100-102`, `adapter.rs:946-947`) — no new
  plumbing; the hint copy becomes the three-form hint (a constant
  beside `OFFSET_SYNTAX_HINT`, `:1077`).
- The preset chips stay due-offset (`rule_preset_labels`,
  `presentation.rs:381-393`) — bare forms, the feature's parity
  anchor. No `At` preset chips this round (recorded: the natural
  presets are personal, the custom field carries the grammar).

### D-7. Rows per kind, the shared formatter, and the needs-due-date affordance

`RuleRowModel` (`rule-list.slint:18-21`) grows: `before` becomes the
kind-specific primary line, plus `armed: bool` (the needs-due-date
state renders from it — **derived in the projection, never stored**;
review focus (e): `task.duedate.is_none() && kind.is_due_offset()` at
push time, `models.rs:1439-1446`).

- **`At` rows**: live countdown ("in 0d 2h 3m 41s") + the fixed local
  fire time ("@ 2026-11-02 09:30:00"). **The formatter lands here,
  shared with chunk 9's D32**:
  - `pub struct CountdownParts { days, hours, minutes, seconds }` +
    `countdown_parts(effective, now) -> CountdownParts` (clamping at
    zero, the `countdown_phrase` clamp precedent) and its text
    renderer — structural output (segment counts, round-trip via
    re-summing seconds), never label text (the no-text-assertions
    rule); the journey pins the shape.
  - `pub fn iso_local_fire_time(effective, tz) -> String` — the
    `YYYY-MM-DD HH:MM:SS` local form (colon-separated per ISO 8601,
    the user's F23 spec), tz-injected like the parse seam.
  Chunk 9's D32 adopts both for timer rows and the status chip;
  nothing double-lands (D32 says "the pure formatter is shared with
  D30").
- **`DueOffset` rows, armed** (task has a due date): the authored
  offset text form plus the effective fire instant
  ("fires 2026-11-02 09:20:00" via `iso_local_fire_time` of
  `occurrence_of` — the armed state made visible, F25's
  communication half).
- **`DueOffset` rows, unarmed** (no due date): the explicit
  "needs a due date — not armed" state (`armed: false` row; the copy
  is a presentation constant), with a **jump affordance**: a row
  IconButton (the `IconKind.calendar`-class glyph from chunk 3's
  vendored set; stage 2 picks from `IconKind`) whose callback routes
  to the **existing** `due-edit-opened()` seam (`task-view.slint:125`)
  — the button opens the due editor for the subject task
  (`Flow::EditDate { field: Due }`), chunk 6's focus machinery arms
  from there. No new focus plumbing; the callback is a new
  `RuleList` callback wired through `task-view.slint` to the same
  handler the due metadata row uses.
- **Per-second tick**: `compute_seconds_surfaces_active`
  (`adapter.rs:845-874`) grows one term — task-detail mounted AND the
  subject has at least one `At`-kind rule (the only seconds-changing
  text in the section). This is the doc-comment's named extension
  point (`:146-149`, "THIS term is the mandated extension point");
  kiosk stays frozen by the existing three-layer exclusion. The
  refresh pass re-projects rule rows each second while active
  (the existing derive-diff pass owns the push; rows re-render only
  when the projected strings change).
- **The muted indicator + un-mute (D31a)**: the rules section hint
  (`rule_list_hint`, `presentation.rs:396-404`) grows the muted
  variant (a `muted: bool` input; the hint names the muted state —
  the cause of "never fires" made visible, the F25 lesson). The
  section gains an un-mute affordance next to the hint (an
  IconButton), dispatching a new `ActionId::ReminderUnmute` →
  `SetTaskRemindersMuted { task, muted: false }` — the CLI's `--off`
  lane as the affordance precedent (`app/src/commands/reminders.rs:
  289-309`). The router arm mirrors `ReminderMute`'s
  (`router.rs:412-428`); the one-shot `reminder_target` machinery is
  NOT needed (the section always names its subject task — the focused
  task is the target, `ctx.focused_task`). Round-trip journey: mute →
  indicator visible → un-mute → indicator gone (§8 T9). The banner's
  one-way mute stays one-way there (banner real estate is the
  notification moment; the section is the state surface — recorded,
  not an oversight).

### D-8. Migration 0020: additive columns, the 0013/0019 recipe

`migrations/0020_reminder_rule_kind.sql` — additive only (ADR 0005),
the 0019 comment block's conventions verbatim:

```sql
ALTER TABLE reminder_rules ADD COLUMN kind TEXT NOT NULL DEFAULT 'due_offset';
ALTER TABLE reminder_rules ADD COLUMN at_epoch_seconds INTEGER;
```

- The `kind` DEFAULT backfill IS the legacy decode: every existing
  row reads `DueOffset` (the 0019 recipe; "upgrade pin, no loss").
  Stored TEXT is the domain serde unit name (`due_offset` / `at`);
  unknown text decodes `Corrupted` at the codec — no CHECK
  re-legislates the enum (0013's discipline).
- `at_epoch_seconds` is deliberately **nullable** (NULL ⇔ due-offset
  rows; NOT NULL, epoch seconds, iff kind = `at`) — the
  `time_entries.end` precedent (`repo.rs:192-202`), not the
  epoch-default trick (that exists so remote stamps win clocks; here
  NULL is honest absence, not a lost race). Decode discipline
  (trusting-write, the offset decode's strictness,
  `repo.rs:583-606`): `kind='at'` with NULL epoch → `Corrupted`;
  `kind='due_offset'` → the offset column decodes through the domain
  mint as today; a stray non-NULL epoch on a due-offset row →
  `Corrupted` (the row is self-contradictory).
- The mirror: `ReminderRuleRow` (`repo.rs:160-167`) gains both
  columns; `decode_reminder_rule` maps them onto `ReminderKind`; the
  upsert (`repo.rs:1872-1877`) writes them. The deliveries/snoozes
  joins (`repo.rs:2083`, `:2097` — `'rule:' || r.rule_id`) are
  kind-blind (the ref text form is unchanged — no `ReminderRef`
  change; D-S9 keys stay stable).
- **The document lane is a second migration surface and needs NO
  schema migration** — the additive-key precedent is already in the
  file: `RuleEntry::Rule` (`appdata.rs:400-418`) gains the kind the
  way insistence did (`:443-457`, default-on-missing → `DueOffset`;
  a present-but-unmodelable kind demotes the whole entry to
  `Verbatim`, never a silent downgrade). Serialize: a `DueOffset`
  rule emits **today's shape verbatim** (no new key — documents
  carrying only due-offset rules stay byte-identical, and old-reading
  clients are unaffected); an `At` rule emits
  `{ "at": <rfc3339>, "created", "clock", "insistence" }` — an old
  reader misses `offset`, `decode_offset` fails, the entry demotes to
  `Verbatim` and is preserved (D-S2's materialization rule working as
  designed, both directions safe). `rules_entry_for`
  (`appdata.rs:1374-1395`) and `fold_reminder_rules`
  (`merge.rs:771-841`) grow the kind mechanically; the fold's clock
  comparison is kind-blind.

## 2. Crate-by-crate work list (stage 2's build order)

1. **domain — reminders.rs**: `AtInstant` + `ReminderKind` (D-1);
   `ReminderRule.kind`; kind-aware `occurrence_of`/`base_occurrence`;
   `at_rule_firings` + the eligibility helper (D-3);
   `prune_settled_at_rules` (D-4.2) + the settle-condition helper the
   planner shares; unit + property tests (§8 T1/T4). SPDX
   MIT OR Apache-2.0, `#![forbid(unsafe_code)]` (already crate-wide).
2. **domain — messages.rs / command.rs / undo.rs / journal.rs**: the
   additive `at` field + doc updates (D-2); the planner's
   normalize → dedup-by-kind → mint path; the new typed rejection;
   the `DismissReminder` settle arm (D-4.1); undo arms are
   payload-wildcarded today (`undo.rs:283`) — verify, extend only if
   the inverse needs the kind (it removes by id, it does not);
   journal label unchanged. Pin tests (§8 T2/T5).
3. **domain — appdata.rs / merge.rs**: `RuleEntry` kind key
   (D-8 document half); `rules_entry_for`; `fold_reminder_rules`;
   round-trip + demote + fold tests (§8 T3).
4. **storage**: migration 0020; row/decode/upsert (D-8 sqlite half);
   codec kind-text fns beside `insistence_to_text` (`codec.rs:79`,
   `:136`); upgrade-pin + round-trip tests (§8 T7).
5. **state**: no production change expected (the tick is
   kind-agnostic) — the paused-clock journey (§8 T6) proves it; if
   the journey forces a production edit, that edit is a finding to
   record, not a silent fix.
6. **ui-slint**: `parse_at_instant` extraction + `parse_rule_when` +
   validity + hint constants (D-6); `CountdownParts`/`iso_local_fire_time`
   (D-7); `RuleRowModel`/projection/`IdTables` unchanged indices
   (`models.rs:1425-1446`); `rule-list.slint` row model + jump button
   + un-mute affordance; `task-view.slint` callback wiring;
   `router.rs` `ReminderRuleAdd` payload (`:432-441`, `:823`) +
   `ActionId::ReminderUnmute` (`:412` shape); adapter draft/target
   fields (`rule_add_offset: Option<ReminderOffset>` → the `RuleWhen`
   draft, `adapter.rs:196-202`, `:4859-4896`); seconds-surface term
   (D-7). Journeys + flow-table validity tests (§8 T8/T9).
7. **app (mechanical)**: the CLI's `--before` add passes `at: None`;
   `--list` rows and receipts render kind text (due-offset forms
   unchanged; an `At` rule — only reachable via the GUI — renders its
   UTC `text_form`). `reminders.rs:130-155`, `:183`, `:245`, `:343`.

## 3. Test catalogue (T-numbers referenced above)

Domain (pure, `reminders.rs` tests + `persistence.rs` + `merge.rs` +
`appdata.rs`):

- **T1** `AtInstant`: mint accepts whole-second, rejects sub-second
  (`matches!` on the error — no text); serde tuple round-trip;
  past-instant leniency. `ReminderKind` serde round-trip.
- **T2** planner: `at: None` plans the byte-identical legacy shape
  (the identity pin — `add_rule_on_a_rule_less_task_plans_mint_and_one_enqueue`,
  `command.rs:6458-6486`, gains the None arm and keeps its enqueue
  count); `At(t)` mints; `In(dur)` stamps `now + dur` on the injected
  clock (saturation pin); `In(≤ 0)` and sub-second `At` are the typed
  rejection; dedup hits per kind (`DueOffset` equality; `At` equality;
  a `DueOffset` and an `At` rule never dedup against each other) —
  extends `add_rule_at_an_existing_offset_plans_nothing_and_names_the_survivor`
  (`:6488-6505`).
- **T3** document: pre-kind entry (no key) folds `DueOffset` (the
  insistence-precedent pin); `At` entry round-trips (serialize →
  parse → same rule); an unknown kind value demotes to `Verbatim`
  and is preserved verbatim (never folded); an old-shape reader
  pin (a `DueOffset` serialize emits no kind key — byte-shape
  assertion on the JSON map, structural not textual); fold update /
  adopt paths carry the kind (`merge.rs:3879+` test family grows one
  leg); a moved card re-points the rule (the recent cross-board
  fold semantics, `merge.rs:800-806`, hold for `At` rules — the pin).
- **T4** evaluation + settlement property/unit: the `At` pass fires
  at `instant <= now` on a task with **no due date** (the F25
  regression core); delivered-then-quiet (no re-fire on repeated
  evaluation — idempotence); dismissed never returns; snooze-walked
  occurrence composes; mute/done/missing-task gates; `DueOffset`
  journeys byte-compatible (the existing P1-P8 suite is the guard —
  it must pass untouched); `prune_settled_at_rules` prunes exactly
  the delivered∧dismissed `At` rows (property: no live `At` rule and
  no `DueOffset` rule is ever in the pruned set; receipts survive the
  prune); the settle-condition helper: dismissed-occurrence ==
  effective ⇒ settle; a stale-occurrence dismissal does not.
- **T5** dismiss-settle planner: the `DismissReminder` arm on a
  settled `At` rule plans dismissal + `DeleteReminderRule` + exactly
  one enqueue (the RemoveReminderRule shape's pin); on a `DueOffset`
  rule or a stale occurrence it plans what today plans (the negative
  pin).

Storage:

- **T7** migration round-trip + upgrade pin: a raw pre-0020-shape
  INSERT (only the 0004/0007/0013 columns) loads as `DueOffset` with
  the authored offset (the S3/S12 precedent,
  `tests/roundtrip.rs:1151+`, `:1270+`); an `At` row round-trips
  epoch-exact; the corruption guards (`kind='at'` + NULL epoch, stray
  epoch, unknown kind text) decode `Corrupted`; full
  persist→load→persist byte-stability on mixed-kind state.

Engine (paused clock, `state/tests/reminders.rs` harness —
`advance_both`/`eventually`/`drain_notifications`):

- **T6** the F25 end-to-end journey: an `At` rule minted on an
  **undated** task → advance to T → the notification lands in-app
  (drained from the notifier) → dismiss → the rule is gone from
  `state.reminders.rules` (the settle) and stays gone across further
  advances. Mirrors `r9_alarm_fires_within_one_tick_and_survives_
  until_dismissal` (`:441+`) plus the settle leg; a second journey
  leg pins "quiet after delivery" on repeated ticks.

UI (`tests/journeys.rs`, `flow_table_tests.rs`):

- **T8** the rule-list grammar journeys: bare `"10m"` dispatches the
  due-offset shape (the existing journey at `:4251+` grows the
  at/in legs); `"in 2h"` dispatches `At(In)`; `"at 17:00"` dispatches
  `At(At)` with the injected-tz instant; garbage keeps the field and
  flips validity (the D27 wiring, structurally asserted via the
  dispatched-command `matches!`); a parse-missed commit absorbs
  (the D12 rule). The shared-parser grep contract (focus d) is a
  review step, not a test.
- **T9** rows + un-mute journeys: an `At` row renders the countdown
  parts (segment-shape assertions on the model, not label text) and
  survives a one-second repaint (the seconds-surface term active);
  an unarmed `DueOffset` row presents the needs-due-date state and
  the jump affordance opens the due editor (the flow lands in
  `Flow::EditDate { field: Due }`); mute → the section indicates →
  un-mute dispatches `SetTaskRemindersMuted { muted: false }` → the
  indication clears.

## 4. Risks & fences

- **R-A (the wire constraint, highest)** — the journal/IPC payload
  compatibility (D-2): any temptation to "clean up" the command shape
  into a kind enum corrupts old journals at load. The additive field
  is the whole trick; review focus reads the serde attrs first.
- **R-B (the settlement fork)** — the synced-row prune hazard (D-4):
  the naive adhoc-mirror prune re-fires via document re-adoption.
  Stage 3 traces the fold→prune→fold loop explicitly (the review
  focus (b)/(c) pair).
- **R-C (migration lane)** — this chunk is the one migration-carrying
  chunk in flight (fence satisfied); if a rebase lands another
  migration first, renumber to the then-next free number at rebase
  (the fence's own rule) and re-pin the upgrade test's raw INSERT.
- **R-D (rebase surface)** — chunks 9/10 (timer/tracking surfaces)
  sit behind this one per §6's serial default; this chunk's
  presentation.rs additions (`CountdownParts`, `iso_local_fire_time`,
  `parse_rule_when`) are the file region chunk 9 edits next — expect
  textual, not semantic, conflict; the shared formatter landing here
  is the design's de-confliction (D32 consumes, does not re-add).
- **R-E (the done-task edge)** — an `At` rule whose task is done
  before T never fires nor settles (D-4's recorded persist-by-design
  class); if review or the user reads that as a leak, the follow-up
  is a settle-at-done rule in a later round, recorded here as
  deliberately out of scope.
- **R-F (clock honesty)** — the UI never mints "in" instants: the
  command carries `AdhocWhen::In` and the planner stamps (D-2); the
  paused-clock journeys prove the stamping. A UI-side
  `Utc::now()` read is a review finding.

## 5. Commit plan (stage 2; every commit standalone)

1. `feat(domain): the at/in reminder kind and the evaluator's At pass`
   — D-1/D-2/D-3/D-4/D-5 + T1–T5.
2. `feat(storage): the reminder rule kind columns` — D-8 + T7
   (migration 0020).
3. `feat(domain): the rule entry's kind rides the document` — the
   document half of D-8 + T3 (split out so the wire fold is
   reviewable alone; lands with 1's kinds in place).
4. `feat(ui-slint): the rule grammar speaks at/in` — D-6 + T8.
5. `feat(ui-slint): armed rows, countdown, un-mute` — D-7 + T9.
6. `chore(app): the cli folds the rule kind` — §2.7 (may fold into 1
   if the workspace won't compile split — the command payload change
   forces the CLI's call sites in the same commit; in that case 1
   carries it and 6 is dropped).
7. `docs: CHANGELOG` (the release entry; the version bump is not this
   chunk's).

Order note: 3 depends on 1 (the kind type); 4/5 depend on 1's
command shape; the state journey (T6) rides whichever commit first
makes it compile — planned into 2 (storage lands before the journey
needs the column) — stage 2 may reorder 2/3 freely; 4/5 last.

## 6. Quality gates & bookkeeping

- Full gate suite through `scripts/run-quality-gates.sh` (the
  parallel-agents wrapper; AGENTS §10), including the coverage
  triple-command sequence and the trailing `rm -rf
  target/llvm-cov-target`. `cargo fmt`/`clippy -D warnings` clean or
  no handoff. `cargo test --doc` green (the planner doctests).
- Handoff at PR-green on `feat/ui-r4-task-reminders`; stage 3
  performs the rebase dance if anything landed beneath; the
  worktree goes with the handoff (§8 teardown).
- CHANGELOG: one entry under the unreleased heading, user-facing
  sentence ("Task reminders can now fire at an absolute time…").
- Bookkeeping on merge (stage 3): D31c files the GUI insistence
  picker as `backlog:` per §8 of the round plan (body links this
  plan's §1 design line; labels `ui` + `p3`).
