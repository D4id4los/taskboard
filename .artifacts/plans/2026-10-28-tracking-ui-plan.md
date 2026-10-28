# [Plan] Tracking UI affordances (DR-5/4) — status chip, card badge + context arms, detail button, notification Start Task, kiosk gate

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan for chunk **DR-5/4** of the
  design-rounds batch dispatch
  (`.artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md`
  §8.3; wave 5, issue **#489**). TDD-first; no implementation in this
  commit.
- **Origin (normative)**: the accepted design round
  `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md` —
  §3 **D-T16** (the four affordances, the two ActionIds, the kiosk
  gate) and **D-T17** (the pure presentation catalogue) decision by
  decision; §1's UI anchors; §5's UI test hooks; §7 chunk 4. The
  dispatch plan §3.4 fences bind: no `[tracking]` config, nothing
  ticks, #490–#496 stay fenced.
- **Branch / worktree**: `feat/tracking-ui` in
  `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-feat-tracking-ui`
  (canonical sibling path; `/home/viktoria/src/taskboard` is a symlink
  view of the main checkout), branched off `origin/main` @ `22b5f5d`.
- **Precondition verified (2026-10-28)**: DR-5/1 has merged —
  `2c41d92` (entity + TrackingState), `ab34153` (the five verbs),
  `9d17d7d` (projections), `3e94f74` (engine tier). Storage rides
  whatever exists at base per the origin's own sequencing note
  (§7): DR-5/2 is wave 7, after this chunk.
- **Status**: **READY FOR STAGE 2** — every open question is resolved
  in §4 (D-TU1–D-TU12); no rulings outstanding.

---

## 1. Verified anchors (2026-10-28 against the base `22b5f5d`; symbols are the stable keys, line numbers are THIS base's)

| Fact | Anchor | Evidence |
|---|---|---|
| The verbs exist; `StopTracking` is payload-free | domain `messages.rs` | `StartTracking { task }` :306, `StopTracking` :312 — receipts `TrackingStarted/TrackingStopped` (command.rs :167/:174) |
| Start's semantics the UI must not duplicate | domain `command.rs` | planner arm :1617: resolve → close-all → open → **dismissals in the same batch** (the SetTaskDone machinery) → flush ops; **no done-gate** ("a completed task is trackable, R-8"); idempotent repeat → `NoOp`. `StopTracking` :1684: total, none-open → `NoOp` |
| The read side the UI derives from | domain `state.rs`, `tracking.rs` | `AppState.tracking` :41; `newest_open()` (tracking.rs :135) — the D-T8 reader pick (most-recently-started, id tie-break); `TimeEntry.task_title` is the immutable snapshot (history outlives deletion) |
| The kiosk gate precedent is model-side and empties | ui-slint `models.rs` | `push_notifications` :474 (gate :483 — `Vec::new()` on kiosk), `push_timers` :430; **timers chip** built in `push_status_bar` :1128-1142 — kiosk forces the chip `None` (display hidden — **the tracking chip deliberately deviates**: display stays, action empties, D-T16) |
| The chip affordance precedent in the chrome | ui-slint `status-bar.slint` | timers chip :226-242 (`TouchArea` + `Badge` + `timers-chip-clicked()`); props :52-60; wired in `app-window.slint` :361-373 |
| Cards carry NO per-row buttons | ui-slint `task-card.slint` | the whole widget — interactions are done-toggled/click/context-requested only; the **decorations slot** :12-25 (`DecorationKind { importance, urgency, habit, reminder }`, rule-D: "future surfaces ride this slot"), rendered :199-218, **tertiary** (compact collapses: `if (!root.compact)` :199) |
| The notification row's affordance table | ui-slint `presentation.rs`, `notification-surface.slint` | `AwaitingButtons` :462-468 ("the five-bool shape is the deliberate table") + `awaiting_row_buttons` :599-616 (task anchors: done/mute true, cancel false; alarms: cancel only); surface renders gated `IconButton`s :64-95 |
| The detail view's State cell | ui-slint `task-view.slint` | metadata row :146-211 — Stack / Due date / **State** :199-210; `TaskDetailModel` pushed at `models.rs` :1076 from `vs.subject_task()` |
| The context menu is generated, ungated | ui-slint `menu.rs` | `TASK_CONTEXT` :124-139 (curated list; timers rows added by DR-3/3 with the note "pointer-driven, menu-reachable, no preset bindings"); `object_entries` :172 — "**No static enablement gating**: gating is dispatch-time"; every context seam focuses the card first (adapter.rs :2872-2916 `activate_card` then open) so the focused task IS the addressed task at invocation |
| The reservation to resolve (comment, not code) | ui-slint `adapter.rs`, `app-window.slint` | adapter.rs :3964-3968 and app-window.slint :679-683 both carry "No Start Task button — deferred to the time-tracking subsystem (issue #210)". Both go in this chunk — **removed, not orphaned** (a stage-3 finding if left) |
| Action-layer growth pattern | ui-slint `actions.rs`, `router.rs` | `ActionId` :159-212; registry rows :349+ (timers rows :796-818: `contexts: GLOBAL`, `Category::Task`, `menu_group: Task`, ranks 150/160, enabled, descriptions mandatory); `ALL` coverage test :930-955 pins `REGISTRY.len()`; `DispatchCtx` :237-274 (one-shot targets: `rule_target`/`timer_target`…), `dispatch` :286-433 (timer arms :355-367 the exact pattern); `route()` consumes one-shot targets per dispatch (adapter.rs :350-361) |
| Wall-minute repaint is free | ui-slint `lib.rs` | the D1 driver re-pushes on the wall-minute boundary :449-451 — minute-bucketed elapsed re-derives with **no driver change** |
| Icons are a closed slint enum | ui-slint `icon-button.slint` | `IconKind` :14-30 — no play/stop glyphs exist yet |
| The test tiers | ui-slint `tests/` | pure-fn units (presentation.rs `#[cfg(test)]`, no run loop); router unit tests; journeys (`Harness::mount_with_settings`, headless testing backend, `FakeEngine` command sink — the kiosk pins :5105-6130); golden matrix (14 PNGs, `golden_visuals.rs`; wall-clock-derived text is established — the sync recency caption already rides it); component pixel-scan tier (`usability_components.rs`) |

## 2. Scope & non-goals

**In scope**: the status-bar chip + its kiosk gate; the card tracking
badge + the context-menu arms; the task-detail Start/Stop button; the
notification surface's Start Task button (anchor-aware, exactly one
command); `ActionId::TaskTrackStart`/`TaskTrackStop` + router arms +
registry rows; the pure presentation catalogue (D-T17); the
`adapter.rs`/`app-window.slint` reservation comments resolved; tests
per §5; golden + registry-snapshot regeneration.

**Non-goals (fenced, binding)**:
- **Today-view totals section (#490) and task-detail entry history
  (#491)** — fenced backlog issues; the detail view gains ONE button,
  no list.
- No `LogTime`/`AdjustTimeEntry`/`DeleteTimeEntry` affordances — v1
  UI has no entry-listing surface to host them (the CLI owns those
  verbs, DR-5/3).
- No `[tracking]` config; no repaint-driver change (the wall-minute
  re-push suffices); no keymap preset bindings (the timers precedent:
  pointer-driven, menu-reachable); no menu enablement machinery
  (dispatch-time gating only, `menu.rs` doctrine); no UI-held timer
  state (elapsed is derived per frame from the ArcSwap snapshot).
- **Not a shape-widener**: zero `AppState`/`PersistedState` changes —
  the shared insta dump suite is untouched. Only the ui-crate's own
  registry snapshot (`keymap_snapshots`) and the golden PNGs
  regenerate.
- No new crate dependencies (`cargo deny` untouched).

## 3. The pure presentation catalogue (D-T17, TDD phase A)

All in `presentation.rs` (the `notification_phrase`/
`countdown_phrase`/`timer_chip` precedents — pure, `now` as data,
structural unit tests, no slint types, no run loop):

```rust
/// The chip's parts (the TimerChip precedent): the newest-open entry's
/// task, its title SNAPSHOT (survives rename/deletion), its start.
pub struct TrackingChip { pub task: TaskId, pub title: String, pub start: DateTime<Utc> }

/// `None` (hidden) while no open entry exists; else the D-T8 reader
/// pick over `state.tracking.newest_open()` — chip and badge share
/// this one derivation so they can never disagree under the
/// transient double-open ruling.
pub fn tracking_chip(state: &AppState) -> Option<TrackingChip>

/// The shared open-entry task (badge visibility's input).
pub fn tracking_open_task(state: &AppState) -> Option<TaskId>

/// Minute buckets, past direction, over the existing `duration_bucket`:
/// `{n}m` / `{n}h` / `{n}d`; the `Now` bucket (sub-minute) renders
/// `"0m"` — an honest zero, not a countdown-style "now".
pub fn open_elapsed_phrase(start: DateTime<Utc>, now: DateTime<Utc>) -> String

/// The kiosk gate (D-T16): the gate empties the ACTION, never the
/// display. Named so the ruling reads at the call site.
pub const fn tracking_chip_action_visible(kiosk: bool) -> bool

/// Badge visibility: the card's task holds the shared open-entry pick.
pub fn tracking_badge_visible(open_task: Option<TaskId>, card_task: TaskId) -> bool

/// The detail button's verb: `Stop` when the subject holds the open
/// entry, `Start` when a live subject exists without it, `None` with
/// no subject. No done-gate — the engine's R-8 ruling owns it.
pub enum TrackDetail { Start, Stop }
pub fn detail_tracking_verb(state: &AppState, subject: Option<TaskId>) -> Option<TrackDetail>
```

`AwaitingButtons` gains **`track_start: bool`** (task anchors `true`,
alarm anchors `false`) — the deliberate-table allow(clippy) comment
stands, now six bools; `awaiting_row_buttons` is the only producer.

## 4. Design — every open question, resolved

**D-TU1 — The chip: composition, placement, kiosk gate.**
`status-bar.slint` gains `in property <bool> tracking-chip-visible`,
`<string> tracking-chip`, `<bool> tracking-chip-action-visible`,
`callback tracking-stop-clicked()`; the chip renders in the bottom
chrome row beside the timers chip (same `Badge` language, tone
`info`): `"{title} · {elapsed}"` assembled in `push_status_bar` (the
timers chip's assembly pattern, models.rs :1128-1142). The Stop
`IconButton` (`IconKind.stop`, a11y label "Stop tracking") renders
only under `tracking-chip-action-visible`. **Kiosk**: the model-side
gate keeps the DISPLAY (`tracking_chip` runs regardless of kiosk —
the appliance shows what is being worked on) and empties the ACTION
(`set_tracking_chip_action_visible(chip.is_some() && !kiosk)`).
This is the deliberate deviation from the timers chip (hidden on
kiosk); D-T16 rules it and the stage-3 review pins it ("a hidden chip
on kiosk is a finding").

**D-TU2 — Chip scope is global, not board-scoped.** Tracking is a
single global intent (D-T8); the chip names the newest-open entry
wherever its task lives, with the entry's title snapshot (a deleted
task's entry still names itself — the D-T2 history ruling). Recorded:
no board filter anywhere in the chip derivation.

**D-TU3 — The badge rides the existing decorations slot.**
`DecorationKind` gains `tracking`; the models pass appends
`TaskDecoration { kind: tracking, label: "tracking", tone: info }`
for the card whose task matches `tracking_open_task(state)`. Static
label — **the badge carries no elapsed**: one moving number exists in
the product (the chip), and a per-minute-changing badge would churn
card-decoration diffs on every wall-minute re-push for zero
information the chip doesn't already show. `info` tone matches the
timers family's informational tone (running-work is a state, not a
verdict). Implementation: `field_decorations(task)` grows a
sibling parameter (the open-entry task, computed once per push pass
and threaded through `decorations_model` — all call sites have
`state` in scope; the ghost copy at models.rs :853 included, so the
drag ghost badges too). The slot's existing semantics stand: the
badge is **tertiary** (compact collapses it — the chip covers
compact).

**D-TU4 — The context-menu arms: two registry rows, no gating.**
`ActionId::TaskTrackStart` ("task.track-start", "Start tracking") and
`TaskTrackStop` ("task.track-stop", "Stop tracking"): `contexts:
GLOBAL`, `Category::Task`, `enabled: true`, `menu_rank` **170/180**
(after the timers rows' 150/160), `menu_group: Some(MenuGroup::Task)`,
descriptions in the registry-review shape; appended to `TASK_CONTEXT`
(the DR-3/3 note applies verbatim: pointer-driven, menu-reachable, no
preset bindings) and to the `ALL` coverage test. Both rows always
render; **no static enablement** — `menu.rs` doctrine ("gating is
dispatch-time"): Start on an already-tracked task is the planner's
idempotent `NoOp`; Stop with nothing open is the planner's total
`NoOp`. The engine owns the refusals.

**D-TU5 — Router arms; the Start target's two honest sources.**
- `TaskTrackStop => DispatchOutcome::Command(StateCommand::StopTracking)`
  — payload-free, **unconditional** (no target check exists to make;
  nothing-open is the engine's accepted `NoOp`).
- `TaskTrackStart` routes `StartTracking { task }` from
  **`ctx.track_target.or(ctx.focused_task)`**: `DispatchCtx` gains
  `track_target: Option<TaskId>` (one-shot, consumed in `route()` —
  the `timer_target` precedent, adapter.rs :361). The fallback serves
  the context menu and the detail button, whose seams do not resolve
  ids themselves: every context seam focuses the card before the menu
  opens (adapter.rs :2872-2916), so `ctx.focused_task` IS the
  addressed task; the detail view's subject is the focused task. The
  explicit target serves the notification row, whose task is table
  addressing, not focus. `NoTarget` when neither is set (the menu
  invocation with no focused card). Recorded alternative rejected:
  overwriting the shell's focused task from the notification seam
  would move real selection state as a side effect.

**D-TU6 — The detail button: one slot, two verbs.**
`TaskDetailModel` gains `can-track-start: bool` / `can-track-stop:
bool` (the notification row's `can-*` naming); exactly one is true
when a subject exists (`detail_tracking_verb`), both false without.
`task-view.slint` renders one `IconButton` in the metadata row beside
the State cell (`IconKind.play`/`stop`, a11y labels "Start tracking"/
"Stop tracking"), forwarding two new callbacks
(`task-track-start-clicked`/`task-track-stop-clicked`) that route the
matching ActionIds (focused-task path, D-TU5). No elapsed text, no
history list — #491 stays fenced. **No kiosk gate here**: kiosk
gating exists only on delivery surfaces (notifications/timers) and
now the chrome chip; no verb family gates the context menu or detail
view on kiosk (a kiosk user can already reach `task.delete` there) —
introducing kiosk machinery for exactly these two verbs would be new
doctrine, not this chunk's to write. Recorded with reasoning.

**D-TU7 — The notification Start Task button: anchor-aware, one
command, the reservation resolved.**
- Visibility: `can-track-start` rides `awaiting_row_buttons` — task
  anchors only; **an Adhoc row never renders it** (the D-P11 table,
  unit-pinned). `notification-surface.slint` gains the gated
  `IconButton` (D-T16 order: after Mark Done, before Snooze — the
  task-shaped cluster stays together).
- Dispatch: the seam resolves `notification_row(&a, index)` (the
  done-seam's stale-index guard), takes `row.task` (a `None` task
  degrades to a no-op — unreachable behind the anchor table, guarded
  anyway), sets `a.track_target = Some(task)`, and routes
  `ActionId::TaskTrackStart` — **exactly one command leaves the UI**.
  Implicit dismissal lives in the planner (D-T5's same-batch
  dismissals, landed in DR-5/1 at command.rs :1617); a UI-side
  `DismissReminder` alongside would duplicate that machinery — the
  stage-3 review's blocking finding, prevented here by construction
  (the seam sends nothing directly; `route()` sends the one outcome).
- The reservation: the `adapter.rs` (:3964-3968) and
  `app-window.slint` (:679-683) "No Start Task button — deferred"
  comments are **deleted** in the same commits that add the button —
  removed, not orphaned.

**D-TU8 — Icons.** `IconKind` gains `play` and `stop` (closed slint
enum, additive arms with `Path` glyphs in the icon-button renderer —
the `check` glyph's pattern). No theme-token changes.

**D-TU9 — Elapsed granularity and bucketing.** Minute buckets v1
(D-T17) over the existing `duration_bucket` — no new bucket math, no
sub-second rendering, no repaint change (the wall-minute re-push,
lib.rs :449-451, re-derives chip text and nothing else needs to
move). The sub-minute `Now` bucket maps to `"0m"` (D-T17 catalogue)
— chosen over the countdown's `"now"` because elapsed-zero and
imminent-fire are different displays.

**D-TU10 — what `push()` touches, and what it doesn't.**
`push_status_bar` gains the three chip property pushes;
`decorations_model`/`field_decorations` gain the badge parameter;
`push_task_detail` (models.rs :1076) gains the two verb bools;
`push_notifications` maps the extended button table. `IdTables` grows
**nothing** (Stop is target-free; Start's targets resolve from
existing tables: focus, notification rows). No new push pass, no new
persistent row handle — the chip is two scalar properties and a bool,
not a model.

**D-TU11 — Adapter seams (complete list).**
`on_tracking_chip_stop_clicked` → route `TaskTrackStop`;
`on_task_track_start_clicked`/`on_task_track_stop_clicked` (detail) →
route the matching id (focused path); `on_notification_track_start_
clicked(index)` → resolve row → set `track_target` → route
`TaskTrackStart`. The context-menu arms need **no new seam** (the
generic menu invocation already re-dispatches registry entries over
the focused task). Every new seam ends in `finish_mutation` via
`route()` — no direct sends outside `route()`'s outcome arm.

**D-TU12 — Test fixtures & goldens.** `fixture.rs` gains
`add_open_entry(state, task, title, start) -> TimeEntryId` (the
`add_alarm` helper's shape, D-P13). The golden fixture
(`golden_state`) seeds one open entry (`start = Utc::now() − 12min`)
so the chip and badge appear in the matrix; **all 14 PNGs regenerate
once** (`TASKBOARD_REGEN_GOLDENS=1`, the harness's reviewed-regen
protocol) — the PNG diff is the review artefact: chip in the chrome
across scales/breakpoints, badge on the seeded card, kiosk variants
pinning the **read-only** chip (button absent). Wall-clock text in
goldens is established (the sync recency caption); fixture
construction and capture share the same process-instant, the same
determinism class the timers countdowns already live in — a
minute-boundary straddle is the recorded residual risk (§6).

## 5. TDD plan (stage-2 execution order; every phase lands green)

- **Phase A — pure fns first** (presentation.rs `#[cfg(test)]`, no
  run loop, structural assertions): `tracking_chip` picks
  `newest_open` (incl. the id tie-break) and hides on empty/closed;
  `open_elapsed_phrase` buckets (0m/59m→"59m"/60m→"1h"/negative clamp
  →"0m"); `tracking_chip_action_visible(!kiosk)`; `tracking_badge_
  visible`; `detail_tracking_verb` (holds→Stop, live→Start,
  no-subject→None, **done task→Start — R-8 is the engine's, not a UI
  gate**); `awaiting_row_buttons` anchor table (task rows gain
  `track_start`, alarm rows never).
- **Phase B — action layer**: registry rows (the `ALL` coverage test
  extended; unique dotted ids); router arms assert the exact
  `StateCommand`: Start with `track_target` set wins over focus;
  Start falls back to `focused_task`; `NoTarget` with neither;
  **Stop routes the exact payload-free command unconditionally** —
  the two-ActionIds-are-all growth is pinned by the exhaustive
  dispatch match (the compiler forces it; the registry test pins the
  count).
- **Phase C — models pass over the mounted harness** (journeys tier,
  headless, `FakeEngine` sink, no sleeps — poll property reads
  directly): chip props pushed for an open entry and cleared after a
  closed-only state; **the kiosk pin**: mount `kiosk: true` with an
  open entry → `tracking-chip-visible` true, chip text set,
  `tracking-chip-action-visible` **false**; badge decoration only on
  the open entry's card (board + today surfaces); detail verbs per
  subject; notification `can-track-start` true on a task row, false
  on an alarm row; **the one-command pin**: drive the notification
  Start seam → the sink holds exactly one command, matching
  `StartTracking { task }` (a `DismissReminder` alongside is the
  blocking bug); stale-index and alarm-row seams degrade to zero
  commands.
- **Phase D — slint compositions + seams** (the smallest mechanical
  phase; the behaviour above is already pinned): status-bar chip row,
  task-card enum arm, task-view button, notification-surface gated
  button, app-window callback/prop forwarding, icon glyphs, adapter
  seams, reservation-comment removal.
- **Phase E — regenerations + gates**: registry snapshot
  (`keymap_snapshots`) for the two rows; the 14 goldens (D-TU12);
  then the full AGENTS §10 suite through
  `scripts/run-quality-gates.sh`.

## 6. Risks & mitigations

- **Golden churn + minute-boundary flake** (D-TU12): all 14 PNGs
  change (intended — the chip is chrome); a fixture→capture minute
  straddle flips the elapsed phrase and fails a compare. Mitigation:
  same-process instants (the harness's established class); a straddle
  is a re-run, and if it ever bites CI it is triaged under the
  flaky-test issue protocol, never fixed in passing.
- **Double-open transient renders two badges?** No — chip and badge
  share `tracking_open_task` (the `newest_open` pick), so exactly one
  card badges and the chip names the same entry; the other open entry
  is visible nowhere in the UI (the CLI `track status` lists all —
  D-T8's readers rule). Recorded honestly: the UI shows the pick.
- **Start Task on a kiosk-reachable notification row?** The kiosk
  empties the notification model wholesale (models.rs :483) — the
  button cannot render there; no extra gate exists to build.
- **A11y/label regressions**: new buttons follow the IconButton
  contract (a11y label = the routed action's registry label); no text
  assertions anywhere — the journeys pin properties and typed
  commands only (AGENTS §5).
- **Drift against waves 5–6 siblings**: DR-6/2 (wave 5) edits the
  `SetTaskDone` planner arm — domain-side, disjoint from every file
  this chunk touches (ui-slint only); DR-4/D (wave 5) is app/daemon
  side. Within the ui-slint soft lane this chunk is the single
  holder (dispatch §1 rule 5). Rebase onto a moved `origin/main`
  re-verifies the anchors of §1 (symbols, not lines).

## 7. Estimated diff size

~15 files, ≈ **+850 / −60** lines: presentation.rs ~+90; models.rs
~+70; router.rs ~+40 (+ tests ~+70); actions.rs ~+40 (+ ALL rows);
adapter.rs ~+60 (seams + comment removal); menu.rs +2;
status-bar.slint ~+30; task-view.slint ~+25;
notification-surface.slint ~+12; task-card.slint +1;
icon-button.slint ~+16; app-window.slint ~+14; fixture/journeys
~+220; goldens 14 PNGs (binary); keymap snapshot ~2 rows. Consistent
with the dispatch's size **M**.

## 8. References

- Normative: the design round §3 D-T16/D-T17, §1 (UI anchors), §5
  (UI test hooks), §7 (chunk 4 + fences #490/#491); the dispatch plan
  §1 (lanes + stage protocol), §3.4 (DR-5 bindings), §8.3 (chunk
  brief + the stage-3 review's blocking findings — planned against
  throughout).
- Landed substrate: DR-5/1's domain surface (§1 table); DR-3/3's
  timers UI (chip, section, one-shot targets, kiosk pins — the
  pattern family this chunk extends in four places).
- ADR 0017 (DR-5/1): the open-entry model this UI derives from;
  ADR 0015: the dumb-client rule the four affordances preserve.
