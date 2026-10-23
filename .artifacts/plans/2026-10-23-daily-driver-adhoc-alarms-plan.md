# [Plan] Daily-Driver Chunk 7 — Standalone Timers & Alarms + 0008

- **Date**: 2026-10-23.
- **Kind**: stage-1 implementation plan (TDD-first; this commit is plan
  only — no production code).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§8 (MD-1…MD-10, the code-quality mandate); wave implementation
  plan `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-13 (timers & alarms half), §3 chunk 7, §4–§6; reminders-surfaces
  design doc `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md`
  Part B §3.5 (D-R5), §3.6 (D-R6, the ripple site table — this chunk's
  checklist), §3.7 (D-R7), §3.8 (D-R8), §3.9 (D-R9), §4 (ACCEPTED
  record), §5 (test-strategy hooks), §7 (chunk mapping: 6 → 7 lands the
  `ReminderRef` ripple once). All ACCEPTED rulings are binding (MD-7);
  every decision below stays inside them — the interpretation calls are
  flagged in §7.
- **Branch / worktree**: `feat/domain-adhoc-alarms` in
  `../taskboard-WT-feat-domain-adhoc-alarms`, off `main` @ `72182d3`
  (= current `origin/main` HEAD). **MD-9 verified 2026-10-23**: chunk 6
  (`feat/state-reminder-rule-verbs`) is merged — its ladder (plan
  `e2ba080` → verbs `dc301d5`/`c75183c`/`c7ee9b2` → UI `78b5d28` →
  changelog `e61f427`) is on `origin/main`; the remote branch is
  deleted post-merge; `SetReminderRule` has no matches in the tree and
  `AddReminderRule` is present. No stacking (MD-2).
  `scripts/worktree-doctor.sh` exited 0 (101 G free, floor 25 G).
- **Goal**: task-independent standalone alarms: `ReminderRef::Adhoc`
  ripples through every evaluation site once (the D-R6 table, walked
  arm by arm), `CreateAdhocAlarm`/`CancelAdhocAlarm` give it a
  lifecycle that ends by settlement (the post-batch prune, ordered
  after snooze compaction, property-pinned), migration 0008 persists
  it device-locally, and `taskboard timers add/list/cancel` surfaces it
  CLI+REPL-first (no UI — #352). Closes #211.

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references)

Domain (`crates/taskboard-domain`):

- `reminders.rs:138-144` — `ReminderRef { Rule(ReminderRuleId),
  Default(TaskId) }`; `:150-171` — `text_form`/`parse_text_form`
  (`"rule:<uuid>"` / `"default:<uuid>"`; one definition site the
  snooze-key serde `:231-257` and the storage codec both delegate to).
- `reminders.rs:282-296` — `ReminderState { rules, deliveries,
  snoozes, muted }`; the doc comment pins "field order is the snapshot
  order" — `alarms` slots in **after `muted`** (D-R5).
- `reminders.rs:269-277` — `ReminderFiring { reminder, task: TaskId,
  occurrence }` (Copy, serde); `:325-336` — `Notification { reminder,
  task: TaskId, title, occurrence }`; `:343-358` — `notifications_for`
  (title snapshot from the task map; a firing whose task is missing is
  skipped).
- `reminders.rs:369-380` — `compact_snooze_chains` (the compaction
  primitive; `apply_actions` calls it in the post-batch pass);
  `:398-418` — `effective_occurrence` (chain walk with the `visited`
  cycle bound); `:420-436` — `base_occurrence` (private; `Rule`/`Default`
  arms); `:448-485` — `due_firings` (iterates live+open+dued tasks,
  rules-replace-default, mute filter, fires iff `effective ≤ now ∧
  undelivered`, sorted by `(occurrence, reminder)`).
- `command.rs:1074-1109` — the `SnoozeReminder` arm: ref match
  (`Rule` → rule lookup → `UnknownReminderRule`; `Default` → task id) →
  `live_task` guard → `postpone ≤ 0` → `InvalidSnoozePostpone` →
  saturating `now.checked_add_signed` → `RecordReminderSnooze`.
- `command.rs:904-923` — the `ResetBoard`/`ClearBoardContent` planner's
  `board_reminders` no-op predicate: a match on `d.reminder` with
  `Default(t) => board_tasks.contains(&t)` / `Rule(_) => false`.
- `command.rs:1125-1204` — the chunk-6 verb handlers (the shape
  precedents): `AddReminderRule` (dedup → zero-action receipt naming
  the survivor; fresh mint + `UpsertReminderRule` + one
  `UpdateAppData` enqueue), `RemoveReminderRule` (existence-only
  lookup → `DeleteReminderRule` + enqueue), `ClearReminderRules`.
- `command.rs:57-107` — `CommandError`; `:102` `UnknownReminderRule`;
  `:106` `InvalidSnoozePostpone` (the `dur ≤ 0` sibling's shape);
  `:111-179` — `CommandOutcome` (the receipt vocabulary).
- `persistence.rs:177-226` — `PersistenceAction` (`UpsertReminderRule`/
  `DeleteReminderRule` at `:204-207`, `ClearBoardContent` at `:225`);
  `:249-464` — `apply_actions` (the one semantics table; the
  `DeleteReminderRule` cascade `:298-308`; the `ClearBoardContent`
  arm `:333-446`; the post-batch pass `:449-463`: `compact_snooze_chains`
  then `derive_sync_states`).
- `persistence.rs:82-102` — `PersistedState` (carries `reminders:
  ReminderState` wholesale; `From<PersistedState> for AppState`
  `:104-116` maps it — alarms ride both for free).
- `ids.rs:41-60` — the `local_id!` registry (`ReminderRuleId` is the
  newest member); `idgen.rs:15-53` — `IdGenerator` +
  `UuidV7Generator`; `test_support::CountingIds` (public for
  downstream crates) is the test generator.
- `test_support/reminders.rs:30-98` — the row strategies
  (`reminder_ref_strategy` at `:41-46`); `:102-171` — `ReminderRow`
  (kinds `0=rule, 1=delivery, 2=snooze, 3=mute`); `:179-232` —
  `build_reminders` (the referential binder); `:236-248` — `pick_ref`.
- `reminders.rs` tests: `expected_firings` `:614-674` (the P2
  independent re-derivation — a `Rule`/`Default` match with the
  hand-rolled snooze walk); P1/P2/P7/P8 `:737-876`; the serde
  roundtrip battery `:1132-1222`.

State (`crates/taskboard-state`):

- `actor.rs:1055-1083` — `reminder_tick` (clock read →
  `core.due_reminder_firings` → `delivery_batch` → `repo.apply` →
  `notifications_for(&firings, core.tasks())` → `interpret`/publish →
  `send_notifications`); `:1087-1101` — `delivery_batch` (one
  `RecordReminderDelivery` per firing; touches `firing.reminder`/
  `occurrence` only — the anchor widening passes through untouched).
- `core.rs:114-122` — `due_reminder_firings` (delegates to the domain
  with `&self.state.tasks` + `&self.state.reminders`); `:124-128` —
  the `tasks()` accessor (notifications_for's current title source).
- `tests/reminders.rs` — the engine tick suite: paused runtime,
  `FixedClock` + `advance_both` (`:215-240`), the immediate-first-tick
  arm, delivery/no-fire-on-storage-failure/quiet-tick arms.

Storage (`crates/taskboard-storage-sqlite`):

- `migrations/0004_reminders.sql` — the four reminder tables; the
  conventions comment (hyphenated-UUID TEXT ids, rfc3339 TEXT
  timestamps, additive only, no CHECK re-legislation, no FKs on
  deliveries/snoozes — the command-layer cascade prunes dependents
  first). `0007_task_field_family.sql` — the wave's additive slot is
  now at 0008 (the rule-clock column already landed in 0007, so
  **chunk 7's migration is purely the `adhoc_alarms` table** — D-R8's
  "one migration, both concerns, whichever lands first" resolved by
  chunk 1 having landed).
- `codec.rs:310-323` — `reminder_ref_to_text`/`reminder_ref_from_text`
  delegate to `ReminderRef::text_form`/`parse_text_form`; the
  deliveries/snoozes columns are opaque text — `"alarm:<uuid>"` rides
  unchanged (D-R8).
- `repo.rs:492-710` — `load_inner` (per-table sqlx queries folded into
  `PersistedState`); `:755+` — `apply_inner` (actions grouped into
  typed vectors: `rule_upserts`/`rule_deletes`/deliveries/snoozes/
  mutes); `:1282-1329` — the `ClearBoardContent` SQL deletes
  deliveries/snoozes `WHERE reminder IN ('rule:'‖id … 'default:'‖id)`
  — an `'alarm:'`-prefixed text never matches either arm, so **the
  storage clear is already alarm-safe with zero changes**.

App (`crates/taskboard-app`):

- `cli.rs:536-668` — `TasksCmd` incl. the `Remind` group (one mode per
  line, `value_parser` flags, the `parse_due`/`parse_effort`/
  `parse_before_flag` edge-parse precedents `:440-485`); `:29+` — the
  top-level `Command` enum (`Boards`/`Tasks`/`Config` groups).
- `commands/reminders.rs` — the two-lane pattern in full: one-shot
  wrappers (`with_core`) + `*_on` session seams (`:111-311`),
  `resolve_rule` (`:198-238`: exact uuid → unique ≥8 lowercased prefix
  with `AmbiguousReminderRule { candidates }` → offset text form),
  `RemindInvocation` + `collapse` (`:424-492`), renderers
  (`:354-419`), the two dispatch switches (`remind_command`/
  `remind_line`).
- `commands/mod.rs:115+` — the app `CommandError` (typed, with
  machine-readable codes `:398-481`); `:982-987` — the one-shot
  dispatch arm; `:1325+/:1654-1656` — `dispatch_line`'s session arm.
- `notifications.rs:44-58` — `LogSink::deliver` (reads
  `notification.reminder/task/title/occurrence` as log fields);
  `:65+` — `spawn_notifier` (the port consumer; the port itself does
  **not** move — D-R6.5/D-R11).
- `completion.rs` — static verb/subcommand completion tables
  (`remind` at `:1168-1194`).

UI (`crates/taskboard-ui-slint`):

- `presentation.rs:254-259` — `ReminderTarget { task: TaskId,
  reminder: ReminderRef, occurrence }`; `:305-334` — `BannerRow` +
  `banner_rows` (task-only by contract); `:340-350` — `snooze_target`
  (rule → `Rule(id)`, else `Default(task)` fallback); `:731-737`,
  `:924-927` — the presentation tests (a `Rule`/`Default` match in the
  target proptest).

Snapshots / CHANGELOG:

- `crates/taskboard-state/tests/snapshots/snapshots__s{1..10}_*
_{published_app,persisted}_state.snap` — every snapshot renders
  `ReminderState`; the `alarms` field widens all of them (the chunk's
  one deliberate insta pass).
- `CHANGELOG.md` — `## [Unreleased]` carries the round's per-chunk
  lines.

## 2. Shape of the chunk

Seven slices, TDD, each landing as its own standalone commit (§4 has
the ladder):

1. **Domain anchor**: `AdhocAlarmId`, `AdhocAlarm`,
   `ReminderRef::Adhoc` (after `Default`), text form, `alarms` map,
   `base_occurrence` arm, `ReminderAnchor` widening,
   `notifications_for` growth. Everything compiler-forced ripples in
   the same commit (the D-R6 walk).
2. **Domain lifecycle**: `AdhocWhen` + `CreateAdhocAlarm`/
   `CancelAdhocAlarm` + the `SnoozeReminder` `Adhoc` arm +
   `ClearBoardContent`'s `Adhoc(_) => false` + `IdGenerator` growth.
3. **Domain settlement prune**: `prune_settled_alarms` +
   `PersistenceAction::{Upsert,Delete}AdhocAlarm` + the post-batch
   pass position + the property set.
4. **Storage 0008**: migration + repo rows/queries + codec round-trip
   pins.
5. **State engine**: tick integration tests (paused time), the
   `EngineCore::reminders()` accessor, the insta pass.
6. **App CLI/REPL**: `TimersCmd`, `commands/timers.rs` (two lanes),
   selector cascade, receipts, completion.
7. **UI widening**: `ReminderTarget` anchor + the presentation test
   arms (no new surface).

## 3. Decisions (every open question settled here)

### D1 — `AdhocAlarmId`, `AdhocAlarm`, `ReminderState.alarms` (D-R5 verbatim)

- `ids.rs`: `local_id!(AdhocAlarmId)` after `ReminderRuleId` —
  portable UUIDv7 like every entity id.
- `idgen.rs`: `IdGenerator::new_adhoc_alarm_id()` grows the trait;
  `UuidV7Generator` returns `AdhocAlarmId::from(Uuid::now_v7())`;
  `test_support::CountingIds` grows the same method (compile-forced —
  downstream crates' fixtures pick it up).
- `reminders.rs`: the entity exactly as D-R5 sketches it — `pub struct
  AdhocAlarm { id: AdhocAlarmId, label: String, fire_at: DateTime<Utc>,
  created_at: DateTime<Utc> }` with D-R5's doc comment (fires once at
  `fire_at` plus snooze-chain movement; cancel is the off-switch; no
  mute). Derives: `Debug, Clone, PartialEq, Eq, Serialize, Deserialize`
  (the `ReminderRule` derive set; no `Copy` — the `label: String`
  forbids it; no `Hash`/`Ord` — map-keyed by id like `ReminderRule`).
- `ReminderState` grows `pub alarms: BTreeMap<AdhocAlarmId, AdhocAlarm>`
  **after `muted`** (D-R5's "after `muted`; snapshot order"). The doc
  comment's field-order sentence updates to "rules, deliveries,
  snoozes, muted, alarms".
- `ReminderRef` grows `Adhoc(AdhocAlarmId)` **declared after
  `Default`** (the ordering ruling — chronological-variant order keeps
  diffs readable; a violation is a stage-3 finding per the dispatch
  plan's trap list).

### D2 — the text form `"alarm:<uuid>"` (D-R6.1)

- `ReminderRef::text_form`'s match grows `Self::Adhoc(id) =>
  format!("alarm:{}", id.as_uuid())`; `parse_text_form` grows the
  `strip_prefix("alarm:")` arm (before the `default:` fallthrough, in
  arm order). One definition site: the `ReminderSnoozeKey` serde and
  the storage codec inherit the new form with zero changes — a snoozed
  tea timer's key is just `"alarm:<uuid>|<rfc3339>"`.
- Tests: the roundtrip/garbage battery (`reminders.rs:1061-1086`)
  grows `"alarm:<uuid>"` acceptance plus `"alarm:"`,
  `"alarm:not-a-uuid"`, and `"alarm:<uuid>|x"`-shaped garbage
  rejections; the storage codec's roundtrip test (`codec.rs:580-591`)
  grows the same case.

### D3 — `base_occurrence` + the chain walk (D-R6.2)

- `base_occurrence` grows `ReminderRef::Adhoc(id) =>
  reminders.alarms.get(&id).map(|a| a.fire_at)` — `fire_at` is
  absolute, no arithmetic, so the representable-range `None` case is
  only the missing-row one. The `effective_occurrence` chain walk,
  cycle bound, and delivery keying are untouched (D-R6.2).
- The prune (D9) needs the same walk for `Adhoc` refs without a
  `tasks`/`defaults` context. Decision: extract the walk body of
  `effective_occurrence` into a private `fn walk_chain(base,
  snoozes: &BTreeMap<ReminderSnoozeKey, DateTime<Utc>>) ->
  DateTime<Utc>` (pure motion — the `visited` set and break-on-repeat
  semantics move verbatim; P7 and the adversarial-chain test are the
  safety net), and add `pub fn alarm_occurrence(reminders:
  &ReminderState, id: AdhocAlarmId) -> Option<DateTime<Utc>>` (base
  from the alarms map, then `walk_chain`) as the alarms-side reading
  used by the prune, the CLI list rows, and `notifications_for`'s
  label lookup. `effective_occurrence` keeps its exact public
  signature and delegates to `walk_chain`.

### D4 — `due_firings`' alarms pass (D-R6.3)

- After the task loop, iterate `reminders.alarms.values()` **in map
  (id) order**: an alarm participates iff present in `alarms` (map
  membership is liveness — no done/tombstone/archived states), and
  fires iff `alarm_occurrence ≤ now ∧ !is_delivered(&Adhoc(id),
  &effective)`. **No mute check** (`muted: BTreeSet<TaskId>` is
  task-shaped; cancel is the alarm's off-switch — a mute check here is
  a ruling violation and a stage-3 trap). Each firing pushes
  `ReminderFiring { reminder: Adhoc(id), anchor: Alarm(id),
  occurrence: effective }`. The existing `sort_by_key(|f|
  (f.occurrence, f.reminder))` merges alarm firings into the task
  firings' total order — output stays fully deterministic
  (`ReminderRef: Ord` covers `Adhoc` by declaration order; D1's
  after-`Default` placement is what makes that order stable and
  readable).
- `due_firings`' signature is unchanged (`alarms` rides
  `&ReminderState`); its doc comment grows the alarms sentence
  (eligibility = map membership; mute does not apply).

### D5 — the anchor payload widening (D-R6.5)

- New domain enum (reminders.rs, next to `ReminderFiring`):

  ```rust
  /// What a firing/notification is anchored on: the task a reminder
  /// watches, or the alarm that owns its label. Alarms carry the typed
  /// anchor so consumers match without re-matching the ref.
  #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
  pub enum ReminderAnchor {
      /// A task-anchored reminder (explicit rule or implicit default).
      Task(TaskId),
      /// A standalone alarm (the label lives on the `AdhocAlarm` row).
      Alarm(AdhocAlarmId),
  }
  ```

  Derive set mirrors `ReminderRef` (the firing stays `Copy` + serde +
  `Ord` for the sort key).
- `ReminderFiring.task: TaskId` → `ReminderFiring.anchor:
  ReminderAnchor`; `Notification.task: TaskId` → `Notification.anchor:
  ReminderAnchor`. This is a serde shape change to port-visible
  payloads (`Notification` crosses the engine→notifier channel) —
  **the `NotificationSink` port signature does not move** (D-R6.5's
  last sentence, D-R11).
- `notifications_for` grows to `notifications_for(firings, tasks,
  reminders: &ReminderState)`: `Task` anchors read the task map's
  title (unchanged); `Alarm` anchors read
  `reminders.alarms[&id].label`. The skip rule is uniform — a firing
  whose anchor cannot resolve (task missing from the map, alarm
  missing from the alarms map) is skipped (total on arbitrary input;
  unreachable through the sequential tick). Decision for the
  parameter: `&ReminderState` (not a bare `&BTreeMap<AdhocAlarmId,
  AdhocAlarm>`) — it matches the design's "takes `&ReminderState`"
  option, parallels `effective_occurrence`'s shape, and stays stable
  when insistence (#203) adds fields.
- The engine tick (`actor.rs:1078`) calls
  `notifications_for(&firings, core.tasks(), core.reminders())`; a new
  `EngineCore::reminders() -> &taskboard_domain::ReminderState`
  accessor (the `tasks()` sibling) provides the state. `delivery_batch`
  needs no change (it keys on `firing.reminder`/`occurrence`).

### D6 — the D-R6 site table, walked (the chunk's checklist)

Every compiler-forced or named site, with its deliberate arm. Stage 3
walks this table against the diff — a site missing an arm, or an arm
that changes semantics beyond what the row states, is a finding.

| # | Site (today's anchor) | Deliberate arm |
|---|---|---|
| 1 | `ReminderRef::text_form`/`parse_text_form` (reminders.rs:150-171) | `"alarm:<uuid>"` (D2). Snooze-key serde + storage codec inherit. |
| 2 | `base_occurrence` (reminders.rs:420-436) | `Adhoc(id) => alarms.get(&id).map(fire_at)` (D3). |
| 3 | `due_firings` (reminders.rs:448-485) | the alarms pass after the task loop; membership = liveness; no mute; same sort (D4). |
| 4 | `SnoozeReminder` planner (command.rs:1074-1109) | `Adhoc(id)` → alarms-map lookup, `UnknownAdhocAlarm(id)` on miss; **no `live_task` call on this arm** (not a task); postpone/re-stamp logic shared (D8). |
| 5 | `ClearBoardContent` planner predicate (command.rs:904-915) | `Adhoc(_) => false` — alarms are device-global, not board content; the match grows the arm between `Default` and `Rule`. |
| 6 | `apply_actions` `ClearBoardContent` (persistence.rs:333-446) | **no change** — the rule/default retain-prunes never match `Adhoc` refs; alarms + their deliveries/snoozes survive a board clear (device-global). A pin test proves it. |
| 7 | `apply_actions` `DeleteReminderRule` cascade (persistence.rs:298-308) | **no change** — `!= ReminderRef::Rule(id)` comparisons never match `Adhoc`. |
| 8 | repo `ClearBoardContent` SQL (repo.rs:1282-1329) | **no change** — `'rule:'‖`/`'default:'‖` subqueries cannot match `'alarm:'` text (verified D10). |
| 9 | `notifications_for` (reminders.rs:343-358) | signature + label fallback + uniform skip (D5). |
| 10 | engine tick correlation (actor.rs:1078) | `core.reminders()` accessor; correlation unchanged — deliveries key on `(reminder, occurrence)` and `Adhoc` refs participate by text form (D5). |
| 11 | sink mapping (app/notifications.rs:49-58) | `LogSink` grows the anchor arm — typed match on `ReminderAnchor`; the `task=` log field renders only for `Task` anchors, an `alarm=` field for `Alarm` anchors (D13; chunk 8's `DesktopSink` maps the same enum). |
| 12 | `ReminderTarget` (ui/presentation.rs:254-259) | `task: TaskId` → `anchor: ReminderAnchor`; `banner_rows` constructs `Task(task.id)`; `snooze_target` is mechanically unaffected (it only produces rule/default refs); the presentation proptest (:924-927) grows the `Alarm` arm (D14). |
| 13 | proptest re-derivations (`expected_firings` reminders.rs:614-674; test_support pickers) | the alarms loop + `Adhoc` arms per D12; P1/P2/P7/P8 gain alarm variants. |
| 14 | `build_reminders_never_references_missing_tasks` (test_support/reminders.rs:256-289) | the `Adhoc` arms: `Adhoc(id)` refs must name existing alarm rows (D12). |
| 15 | engine tick tests (state/tests/reminders.rs) | alarm firing/settlement arms per D11 (routing only). |

Not compiler-forced but named by the design and included in the walk:
`compact_snooze_chains` needs no arm (it is ref-generic by key), and
`overdue_tasks`/`due_soon`/banner stay task-only by contract
(D-R9: kiosk shows nothing new).

### D7 — `CreateAdhocAlarm` / `CancelAdhocAlarm` (D-R7)

- `messages.rs`:

  ```rust
  /// Create a standalone alarm: fires once at the stamped instant,
  /// then dies by settlement (the post-batch prune). Device-local —
  /// no sync op, no sidecar section.
  CreateAdhocAlarm {
      /// The notification title (no task to snapshot a title from).
      label: String,
      /// Absolute instant or engine-stamped duration.
      when: crate::reminders::AdhocWhen,
  },
  /// Cancel a standalone alarm (its deliveries and snooze links are
  /// pruned by the persistence cascade). An unknown id is a typed
  /// rejection — cancel deletes the row, so a repeat targets nothing.
  CancelAdhocAlarm {
      /// The alarm to cancel.
      alarm: crate::ids::AdhocAlarmId,
  },
  ```

- `reminders.rs`:

  ```rust
  /// The `CreateAdhocAlarm` timing input: absolute-vs-duration is a
  /// CLI concern, never a domain concern — the planner stamps `In`.
  #[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
  pub enum AdhocWhen {
      /// Fire at this absolute instant (a past instant is legal data:
      /// it fires on the next tick).
      At(DateTime<Utc>),
      /// Fire `now + dur`, stamped by the engine's injected clock (the
      /// `SnoozeReminder` stamp precedent). `dur ≤ 0` is a typed
      /// rejection.
      In(chrono::TimeDelta),
  }
  ```

  (`Copy` — both arms are; serde externally tagged, the `StateCommand`
  default.)
- `command.rs` planner:

  - `CreateAdhocAlarm`: match `when` — `In(dur)` with `dur ≤
    chrono::TimeDelta::zero()` → `Err(CommandError::InvalidAdhocWhen)`
    (the `InvalidSnoozePostpone` sibling; named `InvalidAdhocWhen`,
    doc: "an alarm duration must be positive"); then
    `fire_at = now.checked_add_signed(dur).unwrap_or(MAX_UTC)`
    (saturating, the SnoozeReminder precedent — an unrepresentable
    target degrades to the chrono ceiling, never panics). `At(t)` →
    `fire_at = t` verbatim. Mint
    `id = ids.new_adhoc_alarm_id()`, `created_at = now`, plan
    `vec![PersistenceAction::UpsertAdhocAlarm(AdhocAlarm { .. })]`,
    outcome `CommandOutcome::AdhocAlarmCreated { alarm: id }`. **No
    enqueue** — device-local (an outbox op here is a ruling violation
    and a stage-3 trap).
  - `CancelAdhocAlarm`: existence-only alarms-map lookup →
    `Err(CommandError::UnknownAdhocAlarm(id))` on miss; plan
    `vec![PersistenceAction::DeleteAdhocAlarm(id)]`, outcome
    `CommandOutcome::AdhocAlarmCancelled { alarm: id }`. Mirrors
    `RemoveReminderRule` (terminal-by-deletion: a repeat is a typed
    rejection, not a `NoOp` — no tombstone exists; D-R7's "typed
    unknown-id rejection" reads exactly so).
- `CommandError` grows: `UnknownAdhocAlarm(AdhocAlarmId)` (doc "no
  adhoc alarm with this id exists"; the `UnknownReminderRule` shape)
  and `InvalidAdhocWhen`. `CommandOutcome` grows:
  `AdhocAlarmCreated { alarm: AdhocAlarmId }`,
  `AdhocAlarmCancelled { alarm: AdhocAlarmId }` (the D-R7 names).
- **Label policy** (interpretation call): no length/emptiness
  validation in the domain — the label is display data like a task
  title, and the task/board families are title-lenient (no
  `is_empty` guard exists in `plan_command`; the `SetBoardColor`
  "lenient domain value; boundary layers validate" comment is the
  house rule). The CLI accepts an empty label the same way `tasks
  add ""` lands today.

### D8 — the `SnoozeReminder` `Adhoc` arm (D-R6.4)

- The ref match grows `ReminderRef::Adhoc(id) => { let _ =
  app.reminders.alarms.get(&id).ok_or(CommandError::UnknownAdhocAlarm(id))?; }`
  — the lookup is the existence guard; the arm yields no task id, so
  the shared `live_task(app, task)?` below cannot apply to it.
  Restructure: the arm computes `Option<TaskId>` (`None` for `Adhoc`)
  and the `live_task` guard runs only on `Some(task)`; the
  `postpone ≤ 0` check and the saturating stamp stay shared. No
  `live_task` check on alarms is a dispatch-plan trap — an alarm is
  not a task and has no liveness states.
- Everything downstream is ref-generic: `RecordReminderSnooze` keys on
  the `Adhoc` text form; the evaluator walks the chain; settlement
  prunes (D9). Snoozing a tea timer reschedules exactly like a task
  reminder's snooze (D-R6.2's sentence).

### D9 — persistence: the two actions + the settlement prune (D-R7/D-R8)

- `PersistenceAction` grows (after `ClearBoardContent`, keeping the
  reminder actions together):

  ```rust
  /// Insert or replace a standalone alarm.
  UpsertAdhocAlarm(crate::reminders::AdhocAlarm),
  /// Remove an alarm **and prune its `Adhoc`-ref deliveries and
  /// snooze links** (the `DeleteReminderRule` cascade shape):
  /// nothing can re-fire from a pruned map row.
  DeleteAdhocAlarm(crate::ids::AdhocAlarmId),
  ```

- `apply_actions` grows the two arms between the rule cascade and
  `ClearBoardContent`: upsert-by-id into `state.reminders.alarms`;
  delete removes the row and retains out every
  `d.reminder == ReminderRef::Adhoc(id)` delivery and
  `key.reminder == Adhoc(id)` snooze link (verbatim cascade shape of
  `DeleteReminderRule`).
- The prune: a new pure function in reminders.rs, next to
  `compact_snooze_chains` (the compaction sibling):

  ```rust
  /// Prunes alarms whose effective occurrence is delivered: such an
  /// alarm can never fire again (the occurrence key is immutable)
  /// and carries no truth worth keeping. The `compact_snooze_chains`
  /// sibling — the caller orders it AFTER compaction so a just-
  /// settled chain reverts to its base before the delivered-ness
  /// check reads it (the composition: target settles → both chain
  /// endpoints delivered → link compacts → effective reverts → base
  /// delivered → alarm prunes). Pruning an alarm removes its
  /// deliveries and snooze links with it. Unobservable in
  /// `due_firings` (property-pinned).
  pub fn prune_settled_alarms(reminders: &mut ReminderState) { .. }
  ```

  Mechanics: collect the ids where
  `alarm_occurrence(reminders, id).is_some_and(|eff|
  reminders.is_delivered(&ReminderRef::Adhoc(id), &eff))`; for each,
  remove the row + cascade its deliveries/snoozes (one definition
  site — the same motion `DeleteAdhocAlarm`'s arm does, so the arm
  delegates to a shared private `fn remove_alarm(reminders, id)`).
- **Position**: in `apply_actions`' post-batch pass, the call lands
  **after** `compact_snooze_chains(&mut state.reminders)` and before
  `derive_sync_states` (the latter is independent; the position is
  fixed for a single canonical order). The before/after distinction
  is load-bearing, not stylistic: on a two-link chain
  (base→m→t, base+m delivered, t undelivered) compaction first drops
  the settled base→m link, the walk reverts to base, and the prune
  removes the alarm whose remaining m→t link is unreachable garbage;
  prune-first would read `effective = t` (undelivered) and keep the
  zombie forever. The property set (D10) pins both directions.
- The post-batch pass now runs compaction → alarm prune on **every**
  batch (same unconditional derivation discipline as today's doc
  comment states for compaction); `apply_actions`' doc-comment
  semantics table grows the two alarm sentences.

### D10 — the prune's property set (§5 test hooks, D-R7's pin)

1. **Unobservability proptest** (reminders.rs or persistence.rs —
   persistence, next to `compaction_is_unobservable_in_due_firings`):
   for arbitrary strategy states *with alarms* (D12),
   `due_firings(before, now) == due_firings(after
   prune_settled_alarms, now)` — the prune never changes what fires.
2. **The mis-order catcher** (unit): the D9 two-link shape via
   `apply_actions(&mut state, &[])` — after one post-pass the alarm
   row is gone. Fails if the prune is ordered before compaction or
   dropped (the zombie shape survives round one).
3. **The composition narrative** (unit, D-R7's sentence made
   concrete): create → fire → deliver base → snooze → deliver target
   → one more batch: link compacts, effective reverts, alarm prunes —
   all through `apply_actions` batches, no direct mutation.
4. **The cascade pin** (unit): `DeleteAdhocAlarm` removes the row +
   its `Adhoc`-ref deliveries/snoozes and nothing else's (the
   `reminder_rule_upsert_replaces_by_id_and_delete_prunes_dependents`
   shape, with a task-reminder row surviving).
5. **Cancel-then-quiet** (unit): after `CancelAdhocAlarm`'s action
   batch, `due_firings` never contains the id at any `now` — "nothing
   can re-fire from a pruned map row".

### D11 — engine integration (state crate)

- `EngineCore::reminders()` accessor (D5).
- Tick tests (paused runtime, `advance_both`), routing-level only per
  the one-behaviour-one-test rule (the *semantics* live in the domain
  suite):
  - **fires within one tick**: create via `CreateAdhocAlarm { when:
    In(30s) }` on the session harness, advance past the stamp, next
    tick → exactly one notification with an `Alarm` anchor, delivery
    row persisted, alarm row **pruned** (settled) — the tick's
    delivery batch and the prune land in the same `apply`.
  - **future alarm is quiet**: an alarm at `now + 1h` survives
    advances below its target untouched (row present, no
    notification).
  - **snoozed alarm re-fires at its target** (the engine-side
    routing of a snoozed `Adhoc` ref; the chain semantics are P7's).
  - the existing `LogSink`-consumer tests keep passing over the
    widened payload (D13).
- **The insta pass** (this chunk's one deliberate regeneration): the
  `alarms: {}` section (empty in every existing fixture — the
  fixtures mint no alarms) widens all `snapshots__s*_*` files. Review
  rule for the accepted diff: exactly the new `alarms` key on every
  `ReminderState` rendering, nothing else; no `.pending` snapshots
  ship.

### D12 — strategy extensions (§5: "referentially consistent alarm rows")

- `test_support/reminders.rs`:
  - `adhoc_alarm_id_strategy()` (the `reminder_rule_id_strategy`
    shape) and `adhoc_alarm_strategy()` (`label` from a short
    alphanumeric proptest string, `fire_at`/`created_at` from
    `timestamp_strategy()`).
  - `reminder_ref_strategy` grows the `Adhoc` arm
    (`prop_oneof!` weights stay even — three arms).
  - `ReminderRow.kind` grows `4 = alarm row`; the tuple grows an
    alarm-pick `usize` (delivery/snooze rows pick
    rule/default/alarm; alarm rows pick their id + stamps). The doc
    comment's kind table updates.
  - `build_reminders`: kind-4 rows insert into `state.alarms` and
    register the id; delivery/snooze rows may now resolve to
    `ReminderRef::Adhoc(existing alarm)`; compaction at the end
    unchanged. Alarm rows are referentially self-contained (no task
    anchor), so the alarm map never references tasks.
  - the invariant test grows: every `Adhoc(id)` in deliveries/snoozes
    names an existing alarm row.
- The domain proptest battery:
  - `eval_input()`/`persisted_state_strategy()` inherit alarms via
    `app.reminders` — no signature changes.
  - `expected_firings` (the P2 oracle) grows the alarms loop: for
    each alarm row, hand-roll base = `fire_at`, the same
    seen-set walk, the same `≤ now ∧ undelivered` gate, pushed into
    the expected vec (then sorted once). The task loop's
    `Rule`/`Default` match grows no arm (it never sees `Adhoc`).
  - P1 (settled never refire), P2, P7 (snoozed suppression — the
    `use_rule` bool becomes a three-way pick rule/default/alarm),
    P8 (mute: the fired-task projection only covers task firings;
    add the assertion that alarm firings ignore `muted`
    entirely — an alarm id can never equal a muted task id, so the
    existing all-fired-tasks-unmuted assertion holds with alarms in
    the mix; the alarm-specific pin is "muting every task does not
    silence any alarm").
  - the serde roundtrip battery grows `AdhocAlarm`, `AdhocWhen`,
    `ReminderAnchor` and the `ReminderFiring` fixture with an `Alarm`
    anchor.

### D13 — the sink mapping (app/notifications.rs, in-wave half of D-R6.5)

- `LogSink::deliver` matches `notification.anchor`:
  `Task(id)` → the existing `task = %id.as_uuid()` field;
  `Alarm(id)` → `alarm = %id.as_uuid()`. `reminder` (text form — now
  including `alarm:<uuid>`), `title`, `occurrence` render unchanged.
  No new domain text-form helper (the typed enum is the interface;
  `Display` on `ReminderAnchor` would be speculative generality —
  chunk 8's `DesktopSink` matches the same enum). The port, the
  channel, and `spawn_notifier` are untouched.

### D14 — the UI widening (D-R6.6, no surface)

- `ReminderTarget { anchor: ReminderAnchor, reminder: ReminderRef,
  occurrence }` (field rename `task` → `anchor` with the type
  change). `banner_rows` constructs `anchor:
  ReminderAnchor::Task(task.id)`; `snooze_target` is mechanically
  untouched (it only ever produced rule/default refs — its
  `Default(task)` fallback keeps a `TaskId`, wrapped as
  `ReminderAnchor::Task` by the construction site). No slint file
  changes; no alarm UI (the #352 ruling). The presentation proptest
  at :924-927 grows the `Adhoc` arm (an alarm-anchored target is
  unconstructible through `banner_rows` — the arm asserts the
  generated rows only ever carry `Task` anchors).

### D15 — storage 0008 (D-R8)

- `migrations/0008_adhoc_alarms.sql` (one file, one transaction
  wrapper per the 0001 convention, the 0004 comment style):

  ```sql
  -- 0008_adhoc_alarms: standalone task-independent alarms (#211,
  -- reminders-surfaces D-R8). Additive only (ADR 0005). Conventions
  -- as in 0004: hyphenated-UUID TEXT id, rfc3339 TEXT timestamps, no
  -- CHECK re-legislation, no foreign keys (alarms reference no
  -- entity; cancel/delete prunes dependents in the batch mirror).
  -- Deliveries/snoozes are untouched: an alarm is the text form
  -- "alarm:<uuid>" through the existing codec. Device-local: no
  -- outbox linkage, no sidecar section.
  CREATE TABLE adhoc_alarms (
      alarm_id   TEXT PRIMARY KEY NOT NULL,
      label      TEXT NOT NULL,
      fire_at    TEXT NOT NULL,
      created_at TEXT NOT NULL
  );
  ```

- `repo.rs`: `AdhocAlarmRow { alarm_id, label, fire_at, created_at }`
  + `decode_adhoc_alarm` (the `ReminderRuleRow`/`decode_reminder_rule`
  shape; rfc3339 via the shared codec time helpers); `load_inner`
  grows the `SELECT … FROM adhoc_alarms ORDER BY alarm_id` query
  folded into `persisted.reminders.alarms` (the reminders assembly
  site); `apply_inner` grows `alarm_upserts`/`alarm_deletes` groups
  (+ the prune needs no SQL — the cascade rides the two action
  groups); upsert `INSERT … ON CONFLICT(alarm_id) DO UPDATE SET` (the
  `reminder_rules` upsert shape); delete = the row delete + the two
  dependent-text deletes (`WHERE reminder = 'alarm:' || ?id` — the
  `rule:`-delete precedent at repo.rs:1196-1203).
- Roundtrip tests (the storage suite's existing whole-state
  roundtrip): an `adhoc_alarms` fixture row survives load→apply→load
  bit-for-bit, its `alarm:`-keyed delivery + snooze ride the text
  codec, and a board clear leaves them all in place (D6 row 8's pin,
  storage-side).

### D16 — CLI grammar: `taskboard timers` (D-R9 verbatim, resolved)

- `cli.rs` grows a top-level group (the `ConfigCmd`/`LabelsCmd`
  shape), on `Command::Timers { cmd: TimersCmd }`:

  ```text
  taskboard timers add <label> (--in <dur> | --at <datetime>)
  taskboard timers list
  taskboard timers cancel <selector>
  ```

  - `Add { label: String, #[arg(long)] in_: Option<TimeDelta>,
    #[arg(long, value_parser = parse_at_flag)] at:
    Option<DateTime<Utc>> }` — `--in`/`--at` conflict; neither given
    is a clap group error (`#[group(required = true, multiple =
    false)]` semantics via `conflicts_with` + required-group, the
    `Remind` group's mechanics). The Rust field is `in_` with
    `#[arg(name = "in")]` (`in` is a keyword; the rendered flag is
    `--in`).
  - `List {}` — pure read.
  - `Cancel { selector: String }`.
- **`--in`** reuses the `parse_before_flag` grammar (`parse_before`:
  `<int><unit>` components `s|m|h|d`) — one duration grammar in the
  app, shared with `--before` (the `--due`/`--scheduled` one-parser-
  two-flags precedent). Positivity is the domain's typed rejection,
  not a CLI duplicate (the planner owns `dur ≤ 0`; the CLI surfaces
  it as the typed error path, exit 1 — a clap usage error would
  duplicate the check).
- **`--at`** (interpretation call, host-local at the edge per D-R9):
  `parse_at_flag` accepts, in order: (1) full RFC-3339 with offset →
  absolute instant; (2) `YYYY-MM-DD` → that date's midnight
  **host-local**, resolved to UTC (the `parse_due` bare-date shape,
  but local not UTC — D-R9's "host-local wall-clock" is the ruling;
  the divergence from `parse_due`'s UTC midnight is deliberate and
  recorded); (3) `HH:MM[:SS]` → today host-local at that time, rolled
  to **tomorrow** when already past (a "09:30" alarm set at 22:00
  means tomorrow 09:30 — the wake-up-alarm reading). (4) anything
  else → the clap usage error (exit 2), the `parse_due` failure
  shape. Timezone override stays a preferences question (D-R9;
  #352-adjacent backlog, not built). The host-local offset
  (`chrono::Local`) is read here and only here — the app
  boundary, never the domain.
- **Selector cascade** (`resolve_alarm(state, selector)` in
  `commands/timers.rs`, the `resolve_rule` shape): exact alarm UUID →
  unique ≥8-char lowercased uuid prefix (`AmbiguousAdhocAlarm {
  candidates }` on multi-hit) → unique case-folded label prefix
  (full-label equality is the zero-length-suffix case of the prefix;
  `AmbiguousAdhocAlarm` on multi-hit). Misses are
  `AppError::UnknownAdhocAlarm(String)` (the app-layer
  selector-string variant; the domain carries the typed id variant —
  the `UnknownReminderRule` pair precedent, app mod.rs:183/481).
  App `CommandError` grows `UnknownAdhocAlarm(String)` +
  `AmbiguousAdhocAlarm { candidates }` with their machine-readable
  codes (`unknown-adhoc-alarm` / `ambiguous-adhoc-alarm`).

### D17 — receipts, list rows, and the two lanes (D-R9)

- `commands/timers.rs` (new, SPDX AGPL, the two-lane shape):
  - one-shot wrappers `add/list/cancel` over `with_core`; session
    seams `add_on/list_on/cancel_on`.
  - `add_on`: label passthrough, `when` from the flag pair, execute
    `CreateAdhocAlarm`, project from the outcome +
    post-state (`fire_at` read back from the alarms map — the
    `ReminderAddReceipt` projection pattern; the stamped `In` value
    is state truth, not an input echo).
    `TimerAddReceipt { alarm_id: String, label: String, fire_at:
    DateTime<Utc> }`, text: `timer "<label>" set for <fire_at
    rfc3339> (alarm <uuid>)` — spelling at implementation, structure
    pinned here.
  - `cancel_on`: resolve → execute `CancelAdhocAlarm` → project the
    **pre-execute** row (the row is gone post-execute — the
    `TaskDeleteReceipt`/`ReminderRemoveReceipt` pre-state precedent).
    `TimerCancelReceipt { alarm_id: String, label: String }`.
  - `list_on` (pure read, no command): rows from
    `state.reminders.alarms` in canonical order `(fire_at, id)`
    (D-R9). Each row derives `effective =
    taskboard_domain::alarm_occurrence(&state.reminders, id)` and
    carries `TimerRow { alarm_id, label, fire_at, effective_at:
    DateTime<Utc>, overdue: bool }` with `overdue = effective ≤ now ∧
    !delivered`-**simplified to** `effective ≤ now` — a delivered,
    settled alarm cannot survive a batch (the prune removes it in
    the same `apply` that records the delivery), so a row visible in
    published state with `effective ≤ now` is exactly the
    fired-not-yet-processed shape D-R9's marker names; a snoozed
    alarm shows its moved target as `effective_at` and `remaining` =
    `effective_at − now` when positive. `now` is `chrono::Utc::now()`
    read at the command layer (the app boundary may read clocks; the
    domain stays pure). `TimersList { timers: Vec<TimerRow> }`.
  - `TimersInvocation` + `collapse(TimersCmd)` + the renderers + the
    two dispatch switches (`timers_command` one-shot /
    `timers_line` session), wired into `commands/mod.rs`'s one-shot
    arm and `dispatch_line`, the `remind` arm pattern verbatim.
  - `--json` rows follow the ADR 0009 envelope (serde projections on
    the receipt structs, kebab-case where enum-ish — none here; plain
    serde field naming for the structs, matching `ReminderAddReceipt`).
- `repl.rs`: `timers` joins the verb table; the REPL routes
  `Command::Timers` through `dispatch_line` (no REPL-specific
  behavior). `completion.rs`: the `timers` verb + static
  `add`/`list`/`cancel` subcommand completions; no dynamic
  label/id completion (selectors resolve typedly; the `remind`
  precedent completes static tokens only).

### D18 — CHANGELOG

- One `Unreleased → Added` line for the timers/alarms family (#211,
  chunk 7): `taskboard timers add/list/cancel`, alarms fire once and
  die by settlement, device-local, the one-tick precision bound
  stated honestly, banner/UI unchanged. (The `Instant`/`Utc` wording
  follows the existing lines' style.)

## 4. Implementation inventory (file-by-file, commit ladder)

Every commit standalone (rebase-merge hygiene): builds, gates-relevant
subsets green, meaningful subject, no fixups.

1. `feat(domain): the adhoc alarm anchor` — D1–D5, D6 rows 1–3/9–14:
   `ids.rs` (+1 id), `idgen.rs` (+trait method + both impls incl.
   `CountingIds`), `reminders.rs` (entity, ref arm, text form,
   base/alarm occurrence, due_firings pass, `ReminderAnchor`,
   `notifications_for`, `walk_chain` motion), `test_support/reminders.rs`
   (strategies + builder + invariant test), the domain test battery
   growth, plus the mechanical compile ripple in the dependent crates
   folded in so the workspace builds as one: `state/actor.rs`
   (`core.reminders()` + the `notifications_for` call), 
   `app/notifications.rs` (the LogSink anchor arm, D13), 
   `ui/presentation.rs` (the target widening, D14). Their behaviour
   pins/tests ride commits 5–7.
2. `feat(domain): the create and cancel alarm verbs` — D7, D8, D6
   rows 4–5: `messages.rs` (+2 commands + `AdhocWhen` import),
   `command.rs` (+2 planner arms, +2 errors, +2 outcomes, the
   Snooze restructure), the planner test block.
3. `feat(domain): alarm settlement prune` — D9, D10, D6 rows 6–7:
   `reminders.rs` (`prune_settled_alarms` + `remove_alarm`),
   `persistence.rs` (+2 actions, +2 arms, the post-pass call, doc
   comment), the property set + pins.
4. `feat(storage): the adhoc alarms table` — D15: `migrations/
   0008_adhoc_alarms.sql`, `repo.rs`, codec roundtrip pins, the
   storage board-clear pin.
5. `feat(state): alarm ticks on the paused clock` — D11: core
   accessor (if not folded into 1), the tick tests, **the insta
   pass** (one `cargo insta accept` over the widened `ReminderState`;
   diff reviewed as the widening only).
6. `feat(app): the timers verb family` — D16, D17, D18: `cli.rs`,
   `commands/timers.rs`, `commands/mod.rs` (error variants + both
   dispatch arms), `repl.rs`, `completion.rs`, the CLI/REPL test
   suites (typed outcomes throughout), CHANGELOG.
7. `test(ui): the reminder target anchor widening` — D14's remaining
   test arms (the compile mechanical landed in 1): presentation
   proptest arm + any unit fixtures. (Subject reflects test-only
   content; rename to `feat(ui-slint): …` if any construction-site
   motion remains.)

No migration-numbering, config, kiosk, sync, or docs-file changes
anywhere in the ladder.

## 5. Test matrix summary (layer → subject)

| Layer | Subject |
|---|---|
| domain unit | text form accept/reject; base/alarm occurrence; `AdhocWhen` stamp (In→now+dur saturating; At verbatim) + `dur ≤ 0` typed rejection; snooze arm (typed unknown, no live_task); clear-board `Adhoc(_) => false`; create/cancel planner outcomes + actions (no enqueue) |
| domain proptest | P1/P2/P7/P8 alarm variants; prune unobservability; serde roundtrips (entity, when, anchor, firing); strategies referentially consistent |
| persistence unit | upsert/delete arms; cascade pin; the composition narrative; the mis-order catcher; board-clear leaves alarms untouched |
| storage | 0008 roundtrip; codec text-form alarm case; SQL board-clear pin |
| state engine | fires-within-one-tick (paused); quiet-until-target; snoozed re-fire; insta pass = the widening |
| app CLI/REPL | add roundtrip (`--in`/`--at` forms, host-local edges); list rows canonical + overdue marker; cancel by all three selector kinds + typed ambiguous/unknown; two lanes; `--json` envelope |
| ui presentation | target widening arms; banner stays task-only |

## 6. Non-goals (dispatch plan §8 restated, plus this plan's)

- No UI surface of any kind (#352; `snooze_target`'s banner path
  mechanically unaffected — no banner behaviour change).
- No alarm sync: no `LocalOp` arm, no outbox enqueue, no sidecar
  section, no codec tag; the `adhoc-alarms` section name stays a
  D5-round note (D-R8).
- No sub-second timers; the one-tick bound is documented, not
  re-architected (`tick_secs` untouched).
- No insistence (#203), no repeat alarms, no retention/GC knob, no
  reminder center, no `timers` tz override (backlog candidates, the
  design's §8 — none promoted here; round-wide: no tracker edits).
- No `NotificationSink` port move (D-R11; chunk 8 owns the desktop
  implementor).
- No mute semantics for alarms, no done/archived states, no
  background GC.

## 7. Risks, residual windows, and interpretation flags

- **Interpretation calls (recorded; none departs an ACCEPTED
  ruling)**: `AdhocWhen`/error/outcome spellings (D7); the
  `walk_chain` extraction as pure motion (D3); `notifications_for`
  taking `&ReminderState` (D5, the design's own option B); the label-
  leniency policy (D7); `--at`'s three-form grammar incl. the
  local-midnight and tomorrow-rollover semantics, diverging from
  `parse_due`'s UTC bare-date reading as D-R9's host-local ruling
  requires (D16); the list row's `overdue = effective ≤ now` without
  a deliveredness conjunct (D17, justified by the same-batch prune);
  `LogSink`'s field spelling (D13); the list `now` read at the app
  layer (D17).
- **Enum ripple breadth** (the design's own risk): mitigated by the
  D6 table + the compiler; the danger arm is a wrong *deliberate*
  arm (mute applied to alarms, a `live_task` on the snooze arm, an
  enqueue on create) — all three are named stage-3 traps.
- **Prune ordering**: pinned by the mis-order catcher (D10.2), not by
  review attention alone.
- **`ReminderFiring`/`Notification` serde shape change**: in-tree
  only (payloads cross an in-process channel; nothing persisted
  carries them — deliveries key on the ref, not the firing). The
  insta corpus does not render firings; the serde roundtrip battery
  covers the new shape.
- **Snapshot churn**: bounded to the one deliberate pass (D11);
  `ReminderState` renders in every published/persisted snapshot, so
  the diff is wide but mechanical — the review rule is "the alarms
  key, nothing else".
- **`--at` local-time ambiguity** (DST fold/gap host-local): chrono's
  local resolution picks one offset for ambiguous wall-clock times;
  accepted at the CLI edge (an instant is chosen, the alarm is
  absolute thereafter) — documented in the flag's doc comment; not a
  domain concern (the domain sees UTC only).
- **Estimated diff size**: ~1.1–1.4k lines including tests
  (domain ~450, storage ~150, state ~120 + ~20 snapshot files, app
  ~350, ui ~40, migration ~15) — M as dispatched.

## 8. Acceptance checklist (stage 2/3 verify against this)

- The D-R6 site table (D6) walked arm by arm against the diff; every
  exhaustive `ReminderRef`/anchor match deliberate; no arm changes
  semantics beyond its row.
- `Adhoc` declared after `Default`; `alarms` after `muted`.
- Alarms ignore mute; no `live_task` on the alarm snooze arm; no
  outbox op / document key for alarms.
- The prune runs after snooze compaction; the unobservability
  property + the mis-order catcher land with it.
- `In(dur ≤ 0)` and unknown cancel/snooze ids are typed rejections.
- Timers fire within one tick in paused-time engine tests; settlement
  prunes.
- The anchor widening applied at `ReminderFiring` **and**
  `Notification` with the label fallback; the port unmoved.
- 0008 is one additive file, 0004 conventions, round-trip tested;
  deliveries/snoozes ride the text codec unchanged; board clears
  spare alarms (planner, persistence, and SQL all pinned).
- `timers` verbs round-trip typedly (all three selector kinds, both
  lanes, `--json` envelope).
- The insta pass is the widening only; no `.pending` files.
- Gates green (AGENTS §10, the CI-identical coverage sequence
  included); CHANGELOG line present; PR body carries `Closes #211`.

## 9. References

- Dispatch: `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1/§8 (chunk 7), MD-2/MD-4/MD-6/MD-7/MD-8/MD-9/MD-10, the
  code-quality mandate.
- Wave implementation plan: `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-13 (timers half), §3 chunk 7, §4 gates, §5 spine, §6
  bookkeeping (closes #211).
- Design: `.artifacts/plans/2026-10-19-reminders-surfaces-design-plan.md`
  Part B (§3.5–§3.9), §4 ACCEPTED record, §5 test hooks, §6 risks,
  §7 mapping, §8 backlog candidates.
- Lineage: `2026-10-12-reminders-design-plan.md` (D-c bounded growth,
  the port), `2026-10-13-reminders-domain-model-plan.md` (P1–P8, the
  compaction precedent §9.6).
- Stage-1 sibling: `.artifacts/plans/2026-10-22-daily-driver-reminder-rule-verbs-plan.md`
  (the chunk-6 shape this plan mirrors; its D5 text-form motion is
  why D2 here is one line).
