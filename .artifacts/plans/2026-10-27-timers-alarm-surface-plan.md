# [Plan] Timers & Alarms Surface — today-view section, status-bar chip, anchor-aware notification rows

- **Date**: 2026-10-27
- **Kind**: stage-1 implementation plan for dispatch chunk **DR-3/3**
  (`2026-10-26-design-rounds-batch-implementation-dispatch-plan.md` §6.2,
  issue #476, branch `feat/timers-alarm-surface`).
- **Origin (normative)**: the design round's §3 **Part D** decision by
  decision (D-I6–D-I8), §5's UI test hooks, §7 chunk 3 —
  `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`;
  ADR 0015's dumb-presentation-client rules; DR-3 binding notes at
  dispatch §3.2 (R3 settlement semantics are the written authority; the
  kiosk banner-only fence; OS-delivery plumbing stays with GUI review
  round 3).
- **Precondition verified**: DR-3/1 landed on `origin/main` as `4c0911d`
  (*fix(reminders): settle alarm rows and snooze links on dismissal, not
  delivery*) — the local `fix/reminder-settlement-retime` branch retains
  the pre-rebase SHA `fc924e6` with identical content (AGENTS §8's
  rebase-merge consequence; `merge-base --is-ancestor` alone is the
  wrong test here). This branch is cut off `origin/main` @ `2a801c1`
  (the local `main` ref was 25 commits stale; the worktree was reset to
  the fetched tip before planning).
- **Status**: PLANNED — awaiting stage-2 implementation.
- **Branch / worktree**: `feat/timers-alarm-surface` in
  `../taskboard-WT-feat-timers-alarm-surface` (worktree-doctor green:
  245G free against the 25G floor).

---

## 1. Context & verified anchors (read on this branch @ `2a801c1`, 2026-10-27)

Symbols are the stable keys; line numbers are this branch's.

| Fact | Anchor | Evidence |
|---|---|---|
| The pure-presentation module and its phrase precedents | ui-slint `presentation.rs` | `duration_bucket` :98 (Now/Minutes/Hours/Days, clamps negative to `Now`); `due_phrase` :118 (`"in {q}"` / `"{q} overdue"` / `"now"`); `scheduled_phrase` :147; `notification_phrase` :407 (past-direction, the R3 chunk C precedent); module header :3–14 states the extraction seam — no slint types, no IO |
| The notification rows derivation + kiosk gate | ui-slint `presentation.rs` :378, `models.rs` :421 | `notification_rows` filters the anchor card and maps `active_notifications` into `NotificationRow { task: Option<TaskId>, reminder, occurrence, title }` — `task: None` ⇔ alarm-anchored; `push_notifications` empties the model on `kiosk` (R3.5.3 banner-only) and caps at `NOTIFICATION_ROW_CAP` :42 |
| The notification surface shows all four buttons regardless of anchor (the Part D repair target) | ui-slint `ui/compositions/notification-surface.slint` :40–81 | every row renders Dismiss/Done/Snooze/Mute IconButtons unconditionally; the adapter's done-seam guards `row.task` (:3466 `let Some(task) = row.task else { return }`) — the presentation never hides the button |
| The seams' dispatch convention | ui-slint `adapter.rs` | `install_notification_seams` :3441 (each button = exactly one engine command); `notification_row` :3428 resolves an index against `tables.notifications`; `notification_target` :3508 mirrors the ref into a `ReminderTarget` anchor; `route` :319 copies the adapter's one-shot target fields into `DispatchCtx` and clears them |
| The one-shot target + routed-verb pattern | `router.rs` :220–254, `actions.rs` :740–777 | `ReminderSnooze`/`ReminderMute`/`ReminderRuleAdd`/`ReminderRuleRemove` — registry rows carry `contexts: GLOBAL`, `Category::Task`, `MenuGroup::Task`, no preset binding; the router arm matches the ctx target, `NoTarget` without one |
| The grammar-field precedent (parse at the seam, absorbed miss) | `adapter.rs` :3187–3205 | `on_rule_add_custom_accepted` parses via `ReminderOffset::parse_text_form`; a miss returns `false` (field keeps its text), a hit stashes `rule_add_offset` and routes — the UI owns no error chrome |
| The origin/main router now ranges a flow-settle matrix over `UiEffect` | `router.rs` :127–204 (new since the stage-1 prompt was written) | `EffectClass::class()` is exhaustive over `UiEffect`; `Adapter::resolve` matches every class. **Consequence: any new `UiEffect` needs a class row + resolve handling. This chunk adds none.** |
| The today view's structure | `ui/views/today-view.slint` | `TodayModel { sections, empty }`; sections loop in a Flickable `content` VerticalLayout; empty state `if (root.today.empty)`; empty sections collapse (`visible: section.count > 0`); section bodies are inline in the delegates (#373) |
| The status bar's shape | `ui/compositions/status-bar.slint`, `models.rs` `push_status_bar` :1017 | banner slot (capped rows + overflow badge) over the indicator row (sync badge, startup findings, flow hint); models are dumb strings assembled Rust-side |
| The minute repaint is the UI's only clock | `lib.rs` `start_repaint_driver` :374 | drains `EngineSignal`/`SystemEvent` (data changes) and fires a refresh when `chrono::Utc::now().timestamp()/60` changes (time passage) at `REPAINT_CADENCE_MS = 100`; `Adapter::refresh` :449 re-derives every model from the ArcSwap snapshot |
| The domain reads the rows derive from | domain `reminders.rs` | `state.reminders.alarms: BTreeMap<AdhocAlarmId, AdhocAlarm>` :431 (`{ id, label, fire_at, created_at }` :272, map membership = liveness); `alarm_occurrence` :673 (fire_at base + `walk_chain` snooze walk, cycle-bounded); `active_notifications` :783 (delivered ∧ ¬dismissed at the effective occurrence, `(occurrence, reminder)` order — the DR-3/1 re-time makes fired alarm rows reachable here); `ReminderRef::Adhoc` |
| The commands this chunk dispatches | domain `messages.rs` :285–299, `command.rs` :1559–1606 | `CreateAdhocAlarm { label, when: AdhocWhen }` — **`AdhocWhen { At(DateTime<Utc>) | In(TimeDelta) }`, `In ≤ 0` is a typed rejection (`InvalidAdhocWhen`), a past `At` is legal data (fires next tick)**; `CancelAdhocAlarm { alarm }` — unknown id is a typed rejection, terminal-by-deletion |
| The `in`/`at` grammar to mirror | app `cli.rs` `parse_at_flag` :550, `commands/timers.rs` `timer_rows` :196 | `at` accepts RFC-3339 | `YYYY-MM-DD` (host-local midnight) | `HH:MM[:SS]` (host-local today, **rolled to tomorrow when already past**); durations ride `ReminderOffset::parse_text_form` (`30s|5m|1h30m|2d`, unsigned, whole seconds); the CLI list projects rows in `(fire_at, id)` order reading the effective occurrence per row |
| The test tiers this chunk extends | ui-slint `tests/` | pure unit tests in `presentation.rs`/`router.rs` (no slint run loop); component tier `usability_components.rs` (mounted window, token-colour pixel proofs); `journeys.rs` (typed command capture via `FakeEngine.drain()`, kiosk gate J-K2 :5812 asserts `get_notification_model().row_count() == 0`); golden board-view matrix with an alarm-free fixture (no notification-surface goldens exist — the R3 chunk C precedent) |
| The test-support gap | `tests/support/` | no fixture helper mints alarm rows; `FakeEngine::settle` has no `CreateAdhocAlarm`/`CancelAdhocAlarm` arms (they fall into the log-and-keep `other` arm) — both must grow (§5, D-P13) |

## 2. Scope & non-goals

**In scope**: the timers section composition in the today view; the
status-bar chip; the pure presentation layer (rows, countdown phrase,
canonical order, chip, awaiting pointer, the `in`/`at` grammar parser);
the anchor-aware notification-row button repair; `ActionId::TimerCreate`
/ `TimerCancel` + router arms + adapter seams; the kiosk model-side
gate; the test-support growth (fixture alarms, fake-engine settle arms).

**Non-goals (binding)**:

- **The alarm window** — DR-3/4's (dispatch §2 ruling). No
  always-on-top overlay, no attention requests, no re-toast cadence, no
  sound. The awaiting rows this chunk surfaces in the notification
  panel are exactly the rows DR-3/4's window will list later.
- **The insistence picker** — deferred to DR-3/2 (see D-P1; a recorded
  deviation from dispatch §6.2's covers line).
- **Recurring alarms, snooze presets, per-run sound/theme pickers**
  (design §2 non-goals); no new view family (R1); no UI-held timer
  state machine (remaining time is derived per refresh; no new
  `slint::Timer` — the minute repaint is the only clock).
- **The OS-delivery plumbing queue** (GUI review round 3's fence) and
  **any domain/state/storage diff** — this chunk is ui-slint-only plus
  its test support. A `taskboard-domain` hunk is a fence violation;
  the crate diff must be exactly `taskboard-ui-slint`.
- **The CLI** — `timers` verbs are untouched (the UI mirrors their
  grammar; it does not change them).

## 3. Design

### Part A — the pure presentation layer (`presentation.rs`, TDD: tests first, no slint types)

Every function below is pure over domain data, total on arbitrary
input, and unit-tested in `presentation.rs`'s existing `mod tests`
(structural assertions only — buckets, orders, counts, typed parses;
never label text per AGENTS §5).

1. **`countdown_phrase(effective: DateTime<Utc>, now) -> String`** —
   the active row's remaining-time phrase, the `due_phrase` future
   direction over `duration_bucket`: `Now` → `"now"`, `Minutes(n)` →
   `"in {n}m"`, `Hours(n)` → `"in {n}h"`, `Days(n)` → `"in {n}d"`.
   Negative deltas (a fired-but-undelivered transient, D-P17) clamp
   through `duration_bucket` to `"now"` — total, no panic path.
   *Tests*: one per bucket boundary (59s/60s, 59m/60m, 23h/24h),
   the `"now"` clamp on past input, monotonic bucketing (the
   `notification_phrase` bucket property shape).
2. **`timer_rows(state: &AppState, now) -> Vec<TimerRow>`** — the
   section's active (unfired) rows: iterate `state.reminders.alarms`,
   read each row's effective occurrence via
   `taskboard_domain::alarm_occurrence` (the walk, never re-derived),
   keep **`effective > now`** rows (D-P17), sort by
   **`(effective, id)`** — the design's canonical order (D-P6) —
   and project `TimerRow { alarm: AdhocAlarmId, label: String,
   effective: DateTime<Utc> }` (label carried; the phrase is the push
   pass's `countdown_phrase(effective, now)` so one refresh derives
   one phrase per row).
   *Tests*: order over three alarms (effective first, id breaking
   ties — construct a base `fire_at` ordering that inverts under a
   snooze chain to pin that the *walked* occurrence orders); the
   fired-undismissed alarm (`effective ≤ now`) is absent from the
   rows; the not-yet-delivered past-effective transient stays a row
   (totality); label snapshot is carried verbatim.
3. **`awaiting_alarm_count(state, defaults, now) -> usize`** — the
   compact pointer's number: the Adhoc-anchored rows of the
   presentation's own `notification_rows` derivation
   (`notification_rows(state, defaults, now)` filtered to
   `matches!(row.reminder, ReminderRef::Adhoc(_))` — one definition,
   the surface's exact rows, no second derivation to drift).
   *Tests*: the D-I7 derivation script as three unit steps on one
   constructed `ReminderState` — fired+delivered ⇒ 1; after a
   snooze link `A→B` ⇒ 0 (the delivery keys the old occurrence; the
   effective walked to B; the row leaves by derivation); after the
   delivery at B ⇒ 1 again. Plus: a dismissed alarm ⇒ 0; task-anchored
   notifications never count.
4. **`timer_chip(state, now) -> Option<TimerChip>`** — the chip's
   pure decision: `None` when `state.reminders.alarms` is empty
   (hidden); else `TimerChip { alarm, label, effective }` for the
   **nearest** row — min by `(alarm_occurrence, id)` over ALL live
   alarms (awaiting included; the chip says "Tea · now", the surface
   owns the row — D-P5). Push assembles `"label · {phrase}"`; the
   pure fn carries the parts so tests assert selection and
   visibility, never assembled text.
   *Tests*: empty map ⇒ `None`; nearest selection with an id
   tiebreak; an awaiting-only map still yields a chip.
5. **`parse_timer_when(raw: &str, now: DateTime<Utc>, tz: impl
   TimeZone) -> Option<AdhocWhen>`** — the form's timing grammar,
   the D-R9 CLI ruling mirrored (D-P7/D-P8). Prefix-dispatched on
   the first whitespace token:
   - `"in <dur>"` — `<dur>` rides
     `ReminderOffset::parse_text_form` (the one duration grammar:
     unsigned `<int><unit>` components `s|m|h|d`); the magnitude is
     negated into a positive `TimeDelta`; **`≤ 0` (i.e. `"0s"`)
     parses to `None`** — the domain's `InvalidAdhocWhen` rejected
     at the seam so a miss is absorbed, never a routed error.
   - `"at <t>"` — the `parse_at_flag` grammar: RFC-3339 (any
     offset) → exact instant; `YYYY-MM-DD` → that date's midnight
     host-local; `HH:MM[:SS]` → today host-local at that time,
     **rolled to tomorrow when already past** (today derived from
     `now` converted through `tz` — `now` is data, the timezone is
     the injected seam; the adapter call site passes
     `chrono::Local` as untested glue, exactly the CLI's
     "only the edge reads Local" shape). DST-ambiguous or
     unresolvable local times (`.single()` misses) fold to `None`.
   - Anything else — empty, missing prefix, bare `"3m"`, unknown
     prefix — is `None`.
   *Tests* (with a fixed `FixedOffset` tz, `now` constructed):
   each accepted form's exact `AdhocWhen` value; the roll-to-
   tomorrow rule (09:30 with now at 22:00 ⇒ tomorrow 09:30; now at
   08:00 ⇒ today); `"in 0s"` ⇒ `None`; each rejection class;
   `In` positivity property (`parse_timer_when("in X", …)` is
   always `In(d > 0)` or `None` — proptest over generated
   magnitudes).
6. **`awaiting_row_buttons(anchor: ReminderAnchor) -> AwaitingButtons`**
   — the anchor-aware per-row visibility decision (D-P11): a pure
   total function answering `{ dismiss, done, snooze, mute, cancel }`
   — `dismiss`/`snooze` always `true`; task anchors add `done` +
   `mute`, alarm anchors add `cancel`. The slint row template
   renders buttons conditionally from the model's projected bools;
   the decision lives here so the repair is a reviewed table, not
   markup drift.
   *Tests*: the two anchor rows of the table; exhaustiveness of the
   bool set.

### Part B — the timers section composition (slint + push)

- **New file `ui/compositions/timers.slint`** (AGPL header):
  `component TimersSection` rendering, top-to-bottom: the header row
  ("Timers" caption + count + the awaiting pointer badge
  `"\{awaiting-count} awaiting"`, non-interactive — D-P15), one row
  per `TimerRowModel { label: string, countdown: string }` (ListRow
  with the countdown in the meta slot + a cancel IconButton), and
  the creation form: two `TextField`s (label, timing placeholder
  naming the grammar, e.g. `"in 10m · at 09:30"`), commit on the
  timing field's `accepted`. Row cancel: `callback cancel-clicked(int)`.
  Form commit: `callback create-accepted(string, string) -> bool`
  (the rule-add seam's return convention: `false` keeps the text).
  Accessibility: `accessible-role: region`, label "Timers"; rows are
  list-items; buttons carry labels (the a11y discipline, ADR 0015
  decision 5).
- **Mounted inside `ui/views/today-view.slint`**, top of the page's
  content layout — above the empty state and the four task sections
  (D-P18): the section is the chip's click target and the
  time-critical surface. The view forwards the two callbacks up to
  app-window. Desktop-only is enforced model-side: the window-level
  `timers-visible` flag is `false` on kiosk and the composition
  renders `if (root.timers-visible)` — the `push_notifications`
  gate precedent (the model is emptied, never rendered).
- **Empty-section discipline (D-P3)**: the *rows* collapse when
  there are none, but the header + form always render on the today
  view — the creation affordance needs a permanent home, and the
  task sections' `count > 0` collapse rule must not hide it. The
  task-side "Nothing due today" empty state keeps its semantics
  (it describes the task sections).
- **No virtualisation for v1** (D-P16): plain rows in the page's
  Flickable content — a user-scale set, the whole page scrolls; the
  #373 section-bound machinery is for the task sections' viewport
  economics. Debt note: virtualise if counts ever warrant.
- **`models.rs` `push_timers(window, state, defaults, now, kiosk,
  tables, models) -> usize`**, called from `push` after
  `push_notifications`: derives `timer_rows`, caps nothing (v1),
  pushes the row model through a `PushModels.timers` persistent
  handle (the positional-diff discipline), sets
  `timers-visible = !kiosk`, `timers-awaiting` from
  `awaiting_alarm_count` (0 on kiosk), and fills
  `IdTables.timers: Vec<AdhocAlarmId>` (index-aligned with the
  pushed model — the `notifications` table precedent; the cancel
  seam's resolution base).

### Part C — the status-bar chip

- **`ui/compositions/status-bar.slint`** gains `in property <bool>
  timers-visible`, `in property <string> timers-chip` (the assembled
  `"label · phrase"`), and `callback timers-chip-clicked()`; the chip
  renders in the indicator row (before the spacer) as a Badge-toned
  interactive element while `timers-visible`.
- **Click routing (D-P2)**: app-window forwards
  `timers-chip-clicked` to the adapter, which routes
  **`ActionId::ViewToday`** — the existing action, the existing
  `UiEffect::FocusToday`, **no new `UiEffect`** and therefore no
  `EffectClass` matrix row on the new router code (§1 anchor). The
  timers section sits top-of-page (D-P18), so FocusToday lands it
  visibly; deep-scroll is explicitly out (the section is the first
  thing on the page).
- **Push**: `push_status_bar` computes `timer_chip(state, now)` and
  sets the flag + text; kiosk forces `false`/empty model-side
  (D-I6's desktop-only, the same gate as the section).

### Part D — the anchor-aware notification rows (the surface repair)

- **`NotificationRowModel` grows three bools**: `can-done`,
  `can-mute`, `can-cancel` (dismiss/snooze stay unconditional).
  `push_notifications` projects them from
  `awaiting_row_buttons` per row — the model stays dumb data; the
  decision is the Part A pure function.
- **`notification-surface.slint`** renders Done/Mute only when
  `can-done`/`can-mute`, and a new Cancel IconButton only when
  `can-cancel`; the cancel button carries its own label
  ("Cancel timer"). Task rows are visually unchanged (all four as
  today); alarm rows become Dismiss / Snooze / Cancel — no Mark
  Done (no task), no Mute (mute is task-shaped), exactly D-I7.
- **The cancel seam**: `on_notification_cancel_clicked(index)`
  resolves `notification_row(index)` (the `tables.notifications`
  convention), requires `matches!(row.reminder,
  ReminderRef::Adhoc(id))` (a stray/task row degrades to a no-op —
  the done-seam's guard shape), stashes
  `adapter.timer_target = Some(id)` and routes
  `ActionId::TimerCancel`. The row disappears by derivation at the
  next refresh (cancel deletes the row; `active_notifications`'
  alarm arm loses it).

### Part E — the action layer: two ActionIds, router arms, adapter seams

- **Registry** (`actions.rs`): `ActionId::TimerCreate`
  (`"timer.create"`) and `ActionId::TimerCancel`
  (`"timer.cancel"`), mirroring the reminder one-shot family
  (D-P9): `contexts: GLOBAL`, `Category::Task`,
  `menu_group: Some(MenuGroup::Task)`, `enabled: true`,
  `menu_rank` 150/160 (after `ReminderRuleRemove`'s 140), honest
  descriptions; **no preset bindings** — pointer-driven one-shot
  verbs, the `reminder.*` family precedent. The exhaustive `ALL`
  registry test's const grows; `REGISTRY.len()` stays pinned.
- **`DispatchCtx`** (`router.rs`) grows two one-shot target fields
  (the `reminder_target` precedent — copied in by `Adapter::route`
  from adapter fields, consumed and cleared on dispatch):
  `timer_draft: Option<TimerDraft>` and
  `timer_target: Option<AdhocAlarmId>`, where
  `TimerDraft { label: String, when: AdhocWhen }` is a small
  `Debug + Clone + PartialEq` struct beside `DispatchCtx`.
- **Router arms** (exhaustive match — compile-forced):
  - `TimerCreate` ⇒ `ctx.timer_draft` is
    `Some(TimerDraft { label, when })` ⇒
    `Command(CreateAdhocAlarm { label, when })`; `None` ⇒
    `NoTarget`.
  - `TimerCancel` ⇒ `ctx.timer_target` is `Some(alarm)` ⇒
    `Command(CancelAdhocAlarm { alarm })`; `None` ⇒ `NoTarget`.
  *Tests* (router `mod tests`): both arms route the exact typed
  commands from their targets; `NoTarget` without (the
  `rule_verbs_route_from_their_targets` pattern).
- **Adapter**: fields `timer_target`, `timer_draft`; `route()`
  copies + clears them (one more instance of the one-shot block);
  **`install_timer_seams`** wires:
  - `on_timers_create_accepted(label, when_text) -> bool`: absorb
    on a whitespace-empty label (D-P14 — the raw label dispatches
    unmodified, no trimming); parse `parse_timer_when(when_text,
    self.day_ctx().now, chrono::Local)`; a miss ⇒ `false` (the
    timing field keeps its text, the rule-add convention); a hit ⇒
    stash `timer_draft`, route `TimerCreate`, `true` (the
    composition clears both fields).
  - `on_timers_cancel_clicked(index)`: resolve
    `tables.timers[index]` ⇒ `timer_target` ⇒ route
    `TimerCancel`; a stale index is a no-op (the
    `notification_row` guard shape).
  - The Part D notification cancel seam.

### Part F — the creation form's dispatch, end to end

`accepted("Tea", "in 3m")` → pure parse ⇒ `AdhocWhen::In(180s)` ⇒
`timer_draft = Some(TimerDraft { label: "Tea", when })` ⇒ route
`TimerCreate` ⇒ `DispatchOutcome::Command(CreateAdhocAlarm { label:
"Tea", when: In(180s) })` ⇒ `commands.try_send` ⇒ the engine stamps
`fire_at = now + 180s` (the planner's `In` arm; saturation, never
panic). `accepted("Tea", "at 09:30")` ⇒ `AdhocWhen::At(local
instant)` — the parse pins the absolute instant at commit time; a
past instant is legal data (fires next tick) and is NOT re-rolled at
dispatch (the roll happened in the parser against the commit-moment
`now`). The `at`-form's `now` is the adapter's `day_ctx().now` — the
same refresh clock every push reads, so commit-time and render-time
agree.

### Part G — kiosk gate (R3.5.3 fence)

One gate, model-side, three surfaces: `push_timers` empties the row
model and zeroes the awaiting count on kiosk; `push_status_bar`
hides the chip on kiosk; `timers-visible = !kiosk` removes the
composition entirely. No kiosk branch ever renders a timer surface —
the banner stays the appliance's only reminder surface (the J-K2
journey extends, §5).

## 4. Decision summary

| ID | Question | Decision |
|---|---|---|
| D-P1 | §6.2's covers line includes the creation form's *insistence picker*, but `AdhocAlarm.insistence` and the `CreateAdhocAlarm` field do not exist at this chunk's base (DR-3/2 is wave 6; this chunk is wave 3) | **Deviation, recorded**: the picker defers to DR-3/2. A picker with no command field to carry it is dead UI (violates the dumb-client rule and "no UI-held state"); growing the domain command here would collide with DR-3/2's own diff (planner arm + migration 0012 + sidecar). The form is built so the picker drops in beside the timing field when the field lands. |
| D-P2 | The chip's click affordance | Routes the existing `ActionId::ViewToday` (`UiEffect::FocusToday`). No new `UiEffect` ⇒ no `EffectClass` matrix row (the origin/main router constraint). Deep-scroll: none — the section is top-of-page (D-P18). |
| D-P3 | The today view's empty sections collapse — what keeps the create form reachable? | The timers header + form always render on the today view (desktop); only the rows collapse. The task-side empty state is untouched. |
| D-P4 | Countdown phrase vocabulary | `duration_bucket`-driven: `"in {n}m/h/d"`, `"now"` at/past (clamped, total). Assembles from the same bucket the `due_phrase`/`notification_phrase` contracts are tested over. |
| D-P5 | Chip visibility & nearest | Visible iff any live alarm row exists (awaiting included — "while any timer exists"); nearest = min `(alarm_occurrence, id)`. |
| D-P6 | Canonical row order | `(effective occurrence, id)` per the design's normative sentence. Noted: the CLI's `timer_rows` sorts `(fire_at, id)` — a divergence only visible through snooze chains; the CLI is not this chunk's diff (crate boundary), recorded as a follow-up candidate, not silently copied. |
| D-P7 | `at` purity vs host-local | Pure `parse_timer_when(raw, now, tz: impl TimeZone)`; the adapter seam passes `chrono::Local` (untested glue, the CLI edge precedent); tests inject `FixedOffset`. |
| D-P8 | The form's timing grammar | Prefix-required `"in <dur>"` / `"at <t>"` (the design's literal examples); `in` reuses `ReminderOffset::parse_text_form` with `≤ 0` ⇒ `None`; `at` mirrors `parse_at_flag` (RFC-3339 \| date \| `HH:MM[:SS]`, past-bare-time ⇒ tomorrow, ambiguous ⇒ `None`). |
| D-P9 | Registry/menu shape for the new actions | The `reminder.*` one-shot family precedent: `GLOBAL` contexts, `Category::Task`, `MenuGroup::Task`, enabled, unbound in presets. Rejected: a new `Category::Timers` — vocabulary growth for two rows against an established precedent (the description column disambiguates). |
| D-P10 | How the form's parse meets the router | `DispatchCtx.timer_draft: Option<TimerDraft>` + `timer_target: Option<AdhocAlarmId>`; adapter fields, copied and cleared in `route()` (the one-shot pattern). |
| D-P11 | The anchor-aware button rule | Pure `awaiting_row_buttons(anchor)` table in presentation; `NotificationRowModel` grows `can-done`/`can-mute`/`can-cancel`; task rows unchanged, alarm rows become Dismiss/Snooze/Cancel. |
| D-P12 | Golden visuals | None change and none are added: the fixture stays alarm-free, so the chip is invisible in all six board goldens (zero churn); the timers surface follows the notification-surface precedent (component tier, no goldens). "Extend per the harness" is satisfied vacuously and honestly; a today-view golden matrix is GUI-review debt if wanted. |
| D-P13 | Test-support gap | `fixture` gains an alarm helper (rows + delivery/dismissal builders); `FakeEngine::settle` gains `CreateAdhocAlarm` (insert with a minted id, `when`→`fire_at` mirroring the planner; the `Utc::now()` precedent is the existing `SetTaskDone` arm) and `CancelAdhocAlarm` (remove). |
| D-P14 | Label validation | Whitespace-empty label ⇒ absorbed (`false`, nothing dispatched); the raw label dispatches unmodified (no trim — presentation never rewrites user data). |
| D-P15 | The awaiting-count pointer | Non-interactive badge (count only). The notification surface is already mounted above the status bar on every view while rows exist — a clickable pointer would duplicate its visibility, not improve it. |
| D-P16 | Row virtualisation / cap | Neither for v1: plain rows in the page Flickable, user-scale sets; the #373 machinery is the task sections'. Debt noted. |
| D-P17 | Active-row eligibility | Rows are alarms with `effective > now` (unfired). Awaiting (delivered ∧ ¬dismissed at effective) rows are the surface's; the transient fired-but-undelivered tick boundary stays an active row rendering `"now"` — total, no third list. |
| D-P18 | Section placement | Top of the today page, above the empty state and the task sections: the chip's click target must land visibly, and timers are the time-critical surface. |

## 5. Test plan (TDD-first order)

Stage 2 writes tests before the code they pin, in this order; every
commit is standalone (builds green, rebase-merge hygiene).

1. **Commit 1 — `feat(ui-slint): the timers presentation functions`.**
   Tests first in `presentation.rs::tests`: `countdown_phrase`
   buckets + clamps; `timer_rows` order (the snooze-inversion
   construction), eligibility, label carry; `awaiting_alarm_count`
   three-step derivation script (fire → snooze → refire) + the
   dismissed/task-anchored negatives; `timer_chip` visibility +
   nearest; `parse_timer_when` accepted forms, roll-to-tomorrow,
   rejections, the `In > 0` proptest; `awaiting_row_buttons` table.
   Then the functions. Diff ≈ 250–300 lines.
2. **Commit 2 — `feat(ui-slint): the timers section and chip
   compositions`.** `timers.slint` (new, AGPL header),
   `today-view.slint` mount + forwarding, `status-bar.slint` chip,
   `app-window.slint` properties/callbacks; `models.rs`
   `push_timers` + `push_status_bar` chip + `IdTables.timers` +
   `PushModels.timers`; the kiosk gate. Structural verification via
   the component tier (§3) lands with commit 5's harness; this
   commit's tests are the pure-layer ones already green plus the
   models-level diff discipline (unchanged-pass no-op behaviour is
   the existing `push_rows` contract).
3. **Commit 3 — `feat(ui-slint): the timer create and cancel
   actions`.** `actions.rs` (ids, registry rows, `ALL` const),
   `router.rs` (`TimerDraft`, ctx fields, arms + tests), `adapter.rs`
   (fields, `route()` copy-clear, `install_timer_seams`, form
   dispatch). Router tests: exact commands from targets, `NoTarget`
   without.
4. **Commit 4 — `feat(ui-slint): the anchor-aware notification
   rows`.** `NotificationRowModel` bools, surface template
   conditionals, `push_notifications` projection, the notification
   cancel seam. Component-tier assertions: an alarm row renders
   exactly three buttons (structural: count/kinds via accessible
   labels as addresses), a task row four.
5. **Commit 5 — `test(ui-slint): the timers journeys and kiosk
   gate pins`.** Fixture alarm helper + fake-engine settle arms
   (D-P13); journeys: J-T1 desktop create→row→cancel (drain
   asserts the exact `CreateAdhocAlarm` then `CancelAdhocAlarm`;
   terminal model shape between); the awaiting-pointer journey
   (delivered alarm ⇒ notification row + `timers-awaiting == 1` ⇒
   dismiss ⇒ 0); J-K2 extended (kiosk: `timers` model empty,
   `timers-visible` false, chip hidden, across the rotation flip);
   component-tier pixel/structure proofs for the section and chip.
6. **Gates** (AGENTS §10, via `scripts/run-quality-gates.sh`):
   fmt, clippy `-D warnings`, nextest, doc tests, deny, the
   coverage three-command sequence. **No golden regeneration** — the
   fixture is alarm-free (D-P12); a golden diff in the PR is a
   finding.

Assertion discipline (binding): typed/structural outcomes only —
drained `StateCommand` variants, model row counts, `Option`/enum
values, table bools; no label/log text, no sleeps (the journeys'
mock clock + `settle` refresh), no TTY, no slint run loop in unit
tests.

## 6. Risks & mitigations

- **The `EffectClass` matrix is new on origin/main** (the stage-1
  prompt predates it): any tempted `UiEffect` addition drags a
  matrix row + resolve arm with it. This chunk adds none (D-P2) —
  the review anchor is "no `UiEffect` variants in the diff".
- **The insistence-picker deviation** (D-P1) must survive stage-3
  review against §6.2's covers line: this section is the recorded
  reason; the PR description should quote it.
- **`chrono::Local` in the UI crate**: `parse_timer_when` itself is
  tz-injected; only the adapter call site reads `Local`. Verify the
  workspace chrono features already enable it (the app crate does;
  ui-slint's `Utc::now()` usage implies the `clock` feature) — a
  compile-time question settled in commit 3's first build.
- **TextField focus interplay in the today view**: the form's two
  fields become tab stops inside a FocusScope view; the just-landed
  L1 focus-loss seam settles text flows on focus loss. The journeys'
  create flow (commit 5) pins commit-then-refresh; the L1 machinery
  is reused, not modified.
- **`alarm_occurrence` cost per row per refresh**: the walk is
  O(chain) map lookups over a `BTreeMap` of live alarms — kiosk-scale
  fine by construction (the CLI's `list_on` reads the same walk per
  row per invocation).
- **Chip text assembly** lives in the push pass (string building is
  trivial glue over `TimerChip` parts, the `due_phrase` convention);
  tests never see assembled strings (AGENTS §5).

## 7. References

- Normative: `2026-10-26-reminders-surface-pair-design-round-plan.md`
  §3 Part D (D-I6–D-I8), §5 (UI test hooks), §7 (chunk 3);
  `2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §1 (execution model, code-quality mandate), §2 (alarm-window
  ruling), §3.2 (DR-3 binding notes), §6.2.
- Architecture: ADR 0015 (dumb presentation client, action registry,
  token discipline); ADR 0013 as amended by DR-1 (the R3 settlement
  model this chunk's awaiting rows render); AGENTS §2 (crate
  boundaries — ui-slint cannot depend on IO crates), §5 (test
  directives), §10 (gates).
- Precedents in-tree: `notification_phrase`/`due_phrase`
  (pure phrase fns); `rule_add_custom_accepted` (grammar seam,
  absorbed miss); `reminder.*` registry family (one-shot routed
  verbs); `push_notifications` (model-side kiosk gate, row cap,
  IdTables); `banner_rows` (chip's nearest-row selection shape);
  J-K2 (the kiosk-gate journey).
