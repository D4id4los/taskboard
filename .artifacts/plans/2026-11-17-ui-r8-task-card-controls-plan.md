# [Plan] ui-r8 chunk 5 — Task card controls & the context menu grammar

- **Date**: 2026-11-17
- **Kind**: dispatch chunk plan (stage-1 of the three-stage protocol) —
  TDD-first implementation spec for chunk 5 of the ux-r8 round. No code
  in this commit.
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-16-task-card-controls-ux-plan.md` (ALL
  sections normative — §2 matrix, §3 US-1..US-7 + rulings R-1..R-4,
  §4 journey steps, §6 W-1..W-7, §7 delta, §8 tests, §9 DoD) and the
  dispatch plan `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1/§6. Cross-chunk inputs, consumed as landed code on this branch's
  base (33322afd): chunk 1's preset rows (`presets.rs` — the tracking
  pair F9/F10 at TaskDetail+Today, the detail editors' F5–F8 run, the
  emacs C-c chord inventory), chunk 4's selection branch
  (`TASK_CONTEXT_MULTI`, `BoardSelection`, `arm_multi_delete`,
  `DeleteTarget::Tasks`, the card's `clicked-modifiers` seam), chunk 3's
  day facts (`days.rs` `day_window`/`day_window_end`/`next_day_window`,
  the adapter's `day_ctx()`/`calendar()` pair).
- **Status**: READY for stage 2.
- **Branch**: `feat/ui-r8-task-card-controls` (worktree
  `../taskboard-WT-feat-ui-r8-task-card-controls`).
- **Surfaces**: `crates/taskboard-ui-slint/src/menu.rs`,
  `src/adapter.rs`, `src/router.rs`, `src/actions.rs`,
  `src/shell.rs`, `src/keymap/presets.rs`, `src/models.rs`,
  `src/view_state.rs`,
  `ui/compositions/action-menu.slint`, `ui/compositions/board-layout.slint`,
  `ui/widgets/task-card.slint`, `ui/widgets/icon.slint`, `ui/app-window.slint`.

## 1. Scope

The chunk lands: the quick-actions bar atop the task context menu
(three new schedule ids + the tracking twin, each click ≡ key); the
applicable-row filter (withheld-not-greyed, C-4) + the four submenus
(Convert/Reminders/Timers/Entries ▸) with the Escape ladder; the N-11
context-popup content-swap fix (reproduction test FIRST, pinned);
the card play/pause control; the create-focus handoff (one-shot,
scroll-into-view); the done-checkbox hover flag; the compact add-task
field height fix; the #882 armed-multi-delete display half. Non-goals
stand per the origin §1: `TaskDueToday`'s Today-only scope untouched
(R-1), no inline editing (T-26), no batch verb, no CLI twins (R-2).

## 2. Settled decisions (each a recorded design; deviations need a
   reason in stage 2's report)

### D-1 — The N-11 root cause (verified against the code at 33322afd)

The hypothesis in the dispatch prompt is **confirmed**, and it is not a
timer-of-its-own: `Adapter::refresh_forced` (`adapter.rs:2343–2353`)
re-derives the menu pages *whenever `self.shell.menu.open`*, with no
discrimination between the main menu and an object/context menu:

```rust
if self.shell.menu.open {
    ...
    self.root_entries = menu::root_page(&self.keymap, &self.menu_chain, &states);
    self.menu_entries = match self.shell.menu.active_group { ... None => self.root_entries.clone() };
    self.shell.menu.len = self.menu_entries.len();
    self.sync_menu_models(window);
}
```

`open_task_context_menu` → `open_context_menu` (`adapter.rs:9604–9619`)
opens the popup with `menu_chain = [Board, Global]` and
`root_entries = object_entries(..., TASK_CONTEXT…)`, `active_group:
None`. Any later materialized refresh pass — the 100 ms repaint tick's
guarded pass hitting its minute-boundary/forced lane, an engine sync
reply, `FlowOutcome::RefreshNow` from any flow — swaps `root_entries`
to `menu::root_page` over `[Board, Global]`, i.e. the **main menu's**
quick rows + parent rows, and pushes them into the live popup's
`menu_entries`/`menu_submenu_entries` globals (`sync_menu_models`,
`adapter.rs:788–792`). The menu the user reads becomes the menu the
next click fires. The `menu_entries`/`root_entries` pair IS the swap
surface — the "model globals" named in the dispatch prompt.

**Fix**: a typed menu-kind discriminator, never an anchor proxy
(`MenuAnchor::Pointer` would also cover future anchored main-menu
variants). `Adapter` gains

```rust
enum MenuKind { Main, Object }   // adapter-private
menu_kind: MenuKind,             // reset to Main at apply_close_menu
```

`open_context_menu` sets `MenuKind::Object`; every other open path
(the hamburger/AppMenu route, keyboard) keeps `Main` (set at the open
funnels, defaulted). The refresh-time re-derivation guard becomes
`if self.shell.menu.open && self.menu_kind == MenuKind::Main { ... }`
— the object popup's model is built once at open and nothing
re-derives it while open (the origin US-3/AC-1 pin verbatim). The main
menu's own round-3 D27c re-grey behaviour is untouched (US-3/AC-2: the
existing menu tests stand).

**Clock law ruling**: the UI refresh cadence is slint-timer-driven
(`REPAINT_CADENCE_MS`), not tokio — the deterministic reproduction
rides the journey driver's mock clock (`JourneyDriver::advance_time`,
`tests/support/driver.rs`), the tool that matches this assertion's
oracle (AGENTS §5 clock discipline). No sleeps; no tokio pause needed
for this test. The round-8 law's "tokio pause/advance" applies where a
tokio timer is the oracle — none is in this chunk.

### D-2 — The quick bar's menu-model section and popup rendering

New typed row, `menu.rs`:

```rust
pub struct QuickBarEntry {
    pub action: ActionId,        // the routing id (click ≡ key)
    pub icon: IconKind,          // the slint icon vocabulary (imported via
                                 // a re-export or a parallel rust-side enum pushed as string)
    pub enabled: bool,
    pub visible: bool,           // false = withheld (C-4); the model still carries the row
}
```

Slint side (`action-menu.slint`): a `QuickBarEntryModel` struct
(`action: string`, `icon: IconKind`, `enabled: bool`,
`visible: bool`) and `in property <[QuickBarEntryModel]> quick-bar` on
`ActionMenu`, rendered as a leading icon-button HorizontalLayout above
the Flickable — **an empty model renders nothing**, so every other
menu consumer (main menu, stack/board contexts) passes `[]` and is
bit-identical. Bar buttons follow `invoke_entry`'s posture: a disabled
entry arms the T-19 disabled notice (its registry description), an
enabled one closes the menu first and routes (round-1 D3). Each new
seam carries the `// slint: <callback>` comment (#782).

Adapter side: `quick_bar_entries: Vec<QuickBarEntry>` held parallel to
`root_entries`, built **once at open** inside the extended
`open_task_context_menu`, pushed in `sync_menu_models`, resolved by
the new `on_quick_bar_invoked(int)` seam against the Rust vec (the
`root_entries` index-resolver pattern; stale index absorbed). The
object-menu pin (D-1) covers the bar — nothing re-derives it while
open.

Icon vocabulary (`icon.slint`): `play`/`stop` exist (the tracking
twin). `calendar` exists. Deliberately distinct glyphs per the F1
shared-icon lesson: add `calendar-plus` (schedule tomorrow) and
`calendar-minus` (unschedule) as new `IconKind` variants (Lucide
equivalents); schedule-today reuses `calendar`. The icon judgement is
walkthrough-graded (origin §8 Manual).

### D-3 — The three new ActionIds and their dispatch funnels

`actions.rs`: `TaskScheduleToday`, `TaskScheduleTomorrow`,
`TaskUnschedule` — Task-group `menu_group: Some(MenuGroup::Task)`
rows with `contexts: TASK_CONTEXTS` (the Board/Today/StackDetail/
TaskDetail slice, the `task.due-today` family's shape), ranks placed
above the reminder block (today/tomorrow/unschedule at the
reminder ranks' head; exact rank numbers chosen at stage 2 keeping
the 10-step spacing the registry test pins). Hints are
registry-generated; labels are the scheduled field's verbs (R-1: the
*due* twin stays `TaskDueToday`, Today-only, untouched).

`router.rs` arms (the `TaskDueToday` template at :1154, exact sparse
masks):

```rust
ActionId::TaskScheduleToday   => match (ctx.focused_task, ctx.day_window) {
    (Some(id), Some((_, end))) => Command(UpdateTask { id,
        changes: TaskChanges { scheduled: Some(Some(end)), ..default } }),
    _ => NoTarget },
ActionId::TaskScheduleTomorrow => match (ctx.focused_task, ctx.tomorrow_window_end) {
    (Some(id), Some(end)) => Command(UpdateTask { id,
        changes: TaskChanges { scheduled: Some(Some(end)), ..default } }),
    _ => NoTarget },
ActionId::TaskUnschedule       => match (ctx.focused_task, ctx.focused_task_scheduled) {
    (Some(id), true) => Command(UpdateTask { id,
        changes: TaskChanges { scheduled: Some(None), ..default } }),
    _ => NoTarget },   // inapplicable subjects are withheld upstream (D-4);
                       // the router NoTarget is the stale-absorb backstop
```

`DispatchCtx` gains one additive field `tomorrow_window_end:
Option<DateTime<Utc>>`, set by the adapter per dispatch from
`days::next_day_window(self.day_ctx().now, self.day_start.0,
self.day_start.1).1` (the landed days.rs law; **no** `+24h`
arithmetic anywhere). Today's end rides the existing `ctx.day_window`
end — the same instant `TaskDueToday` re-commits, so the scheduled
chip renders "today" under the uniform day-boundary law (origin §1).
Every mask is sparse-exact (only `scheduled` present) — the r3
exact-mask shape.

### D-4 — The per-subject withheld filter (C-4 per family)

`menu.rs` gains a typed subject fact and the curated context builder:

```rust
pub struct TaskMenuSubject {
    pub tracked_here: bool,          // the subject holds THE open entry
    pub tracking_elsewhere: bool,    // another task's open entry
    pub has_reminder_rules: bool,    // any rule/reminder state on the subject
    pub is_series: bool,             // recur.is_some()
    pub is_habit: bool,              // kind == Habit
    pub is_tracker: bool,            // kind == Tracker (the #492 filter, now derived here)
    pub has_armed_timer: bool,       // the subject holds an armed timer
    pub scheduled: bool,             // scheduled.is_some()
}
pub fn task_context_page(keymap, chain, subject: &TaskMenuSubject,
    extra: &[ActionId]) -> (Vec<QuickBarEntry>, Vec<MenuEntry>);
pub fn task_submenu_page(keymap, chain, subject: &TaskMenuSubject,
    family: TaskMenuGroup) -> Vec<MenuEntry>;
```

Derived in `open_task_context_menu` from the loaded `AppState` + the
subject (the adapter already loads it for the #492 tracker check —
that check moves into `TaskMenuSubject::from(state, subject)`). The
withheld/greyed verdict per row (C-4: state-inapplicable → withheld
(`visible: false` / row dropped); applicable-but-blocked → greyed
(`enabled: false`)):

| Family / row | Condition | Verdict |
|---|---|---|
| Quick bar schedule today / tomorrow | always | visible, enabled |
| Quick bar / row `TaskUnschedule` | `!scheduled` | **withheld**; else visible+enabled |
| Tracking twin (bar + rows `TaskTrackStart`/`Stop`) | `tracked_here` → Stop shown+enabled, Start **withheld**; `tracking_elsewhere` → Start shown+**greyed** (the singleton precondition, entry_states' existing grey), Stop **withheld**; neither → Start shown+enabled, Stop **withheld** | mixed (the origin US-1/AC-4 + US-2/AC-1 shown-half rule) |
| `TaskComplete` | `is_tracker` | **withheld** (the #492 hide-set moves here, semantics unchanged — the reopen no-op stays) |
| `ReminderUnmute` / `ReminderSnooze` / `ReminderRuleRemove` | `!has_reminder_rules` | **withheld** |
| `ReminderMute` / `ReminderRuleAdd` | always | visible; `Mute` greyed when `!has_reminder_rules` (applicable verb, precondition-blocked — the origin AC-2's "stay") |
| Series rows (`SeriesPause`/`SeriesResume`/`SeriesConvertArm`/`SeriesConvertToHabit`) | `!is_series` | **withheld** |
| `SeriesPauseCommit` | `!is_series` | **withheld**; on a series: visible, greyed unless the pause arm is live (the `TaskMoveCancel` flow-grey precedent via `entry_states`/states map) |
| Habit rows (`HabitRepair`/`HabitConvertArm`/`HabitConvertToSeries`) | `!is_habit` | **withheld** |
| `TimerCancel` | `!has_armed_timer` | **withheld** |
| `TimerCreate` | always | visible+enabled (NoTarget refusal remains the dispatch backstop) |
| `TaskTrackEntries` | always | visible+enabled (dispatch-time kiosk/subject refusals stand) |
| Core rows (create/edit/complete/reopen/move/move-cancel/delete) | unchanged enablement | `TaskMoveCancel` stays flow-greyed |

The multi-selection branch (chunk 4's, `adapter.rs:9653–9665`) hands
`TASK_CONTEXT_MULTI` unchanged — chunk 5's filter applies only to the
single-card menu (the origin §1 non-goal: the multi grammar owns
itself; the count caption is D-9).

### D-5 — The submenu grammar and the Escape ladder

New typed families, `menu.rs` (NOT `MenuGroup` variants — new
`MenuGroup` variants would leak parent rows onto the main root page,
whose `root_page` iterates `MenuGroup::all()`; the main menu's Task
page stays exactly as landed):

```rust
pub enum TaskMenuGroup { Convert, Reminders, Timers, Entries }
```

`task_context_page` composes: the bar (D-2) → the frequent core rows
(create, edit, complete/reopen, move, move-cancel, delete, the
schedule trio, the tracking twin) in rank order → one submenu-parent
row per family whose `task_submenu_page` is non-empty for the subject
(the `root_page`'s empty-page cascade, so a rule-less task renders no
Reminders ▸ parent at all when all its children withheld — the
parent-with-page rule) → destructive rows last (BF-W-N2). Parent rows
are the existing `MenuEntry { action: None, submenu: ? }` shape —
`MenuEntry.submenu` widens to `Option<Submenu>` where
`enum Submenu { Group(MenuGroup), Task(TaskMenuGroup) }` (additive;
`root_page`/`group_page` keep constructing `Submenu::Group`; their
tests untouched). `shell.rs` `MenuState` gains
`task_group: Option<TaskMenuGroup>` alongside `active_group`;
`activate_parent`/`invoke_entry`/`invoke_root_entry` branch on the
`Submenu` kind (a `Task` parent parks the cursor and mounts
`task_submenu_page` as `menu_entries`); the shell's level-up ladder
(shell.rs:168–173) consults `task_group` first, then `active_group`,
then closes — Escape 1 closes the submenu, Escape 2 closes the menu
(origin US-2/AC-4). Compact replace-mode and the fly-out ride the
existing two-panel machinery unchanged (the `menu-active-group`
int global gains a parallel `menu-task-group` push in `sync_menu_models`).

Child rows keep their existing `ActionId`s, hints, and shortcut
projections — `task_submenu_page` projects through the same `row()`
as every page (the T-3 hint contract holds per family).

### D-6 — The card play/pause control

`task-card.slint`: a new icon-button beside the decoration region
(pointer-grade, R-3 — no card-scoped key), shown when the card model
carries the new tracking fact:

```rust
// TaskCardModel additions
track-state: TrackCardState,   // enum { none, running-here } — the shown-half input
// slint: track-toggled
callback track-toggled();
```

`models.rs` derives `track-state` per card from
`presentation::tracking_chip(state)` (the open entry's subject vs the
card's task — the same fact `entry_states` uses). The button renders
`play` when `none`, `stop` when `running-here`; hidden entirely when
the subject is a tracker kind (the can-done hide posture). The seam
routes through the card's addressing chain (board / compact /
stack-detail / today rows) into `adapter` `route(ActionId::TaskTrackStart
| TaskTrackStop)` — the context-verb focus-first posture is already
guaranteed because every card interaction focuses first; the ids'
router arms are the existing ones (`router.rs:1001/1008`: Start needs
`track_target`/`focused_task`, Stop is the total no-op when nothing
is open). One-shot flip: the id is state-flip-gated by `entry_states`
(Start greyed while `tracking_live`) and the stale-absorb law — the
seam test pins no double dispatch (US-4/AC-3) via the
`apply_outcomes_for_test` seam asserting exactly one command per
state.

### D-7 — The create-focus handoff (one-shot, scroll-into-view)

Constraint honoured: no new `StateCommand`, no new `UiRequest`
(origin §7) — the created id is engine-minted (`CommandPlan::outcome
CreatedTask`, `command.rs:871–910`), and the UI's `commands` mpsc is
fire-and-forget (`adapter.rs:1796`), so the handoff derives the
target from the post-apply state diff:

1. `apply_outcomes`' `FlowOutcome::Command(StateCommand::CreateTask { stack, .. })`
   arm (it already matches commands for notices, `adapter.rs:1788–1795`)
   arms a one-shot
   `create_focus_arm: Option<CreateFocusArm { stack: StackId, before: BTreeSet<TaskId> }>`
   where `before` = the subject stack's live task ids at commit time.
   The empty-buffer settle never emits the command (view_state's
   `Flow::Create` guard) → arms nothing (US-5/AC-2 for free).
2. The next materialized refresh pass consumes the arm: the stack's
   live ids diffed against `before` — exactly one new id →
   `view_state` focus moves to it (the walk handoff's existing focus
   setter), then `push_focus_scroll_hint(window, before_focused)`
   edge-triggers the `FocusScrollHint` global (the machinery chunk 4
   extended; the board/compact ListViews already consume
   `request-row`/`request-nonce`), and the arm clears. Zero or >1 new
   ids (a refused create, a racing capture) → the arm clears with NO
   handoff — the one-shot law (a second apply never re-steals focus;
   the stale-absorb posture).

Test oracle: the focused-card model fact (`view_state.focused_task()`
== the created id) + the hint edge, never scroll geometry.

### D-8 — The done checkbox hover flag + the compact height fix

**Hover (US-6)**: `task-card.slint`'s `done-box` gains
`out property <bool> done-hover: done-hit.has-hover;` and the box's
`background`/`border-color` compose the house hover treatment from
the Theme's existing hover tokens (`Theme.surface-hover` step — the
r1 R-1 posture, no new primitive). The presentation probe
(`task_card_title_probe.slint` pattern) asserts the flag, never
styling text.

**Height (US-7)** — mechanism hypothesis, probe FIRST: the compact
leg's column loop (`board-layout.slint:996–1127`) wraps the
`task-viewport` (min-height: 0) and `add-task-compact`
(min=max=locked) in one VerticalLayout with **no `vertical-stretch`
assignment** — the regular leg's task area carries
`vertical-stretch: 1` (:568) and its viewport absorbs the column's
surplus; the compact leg's surplus therefore distributes into the
locked-height ghost / header band, the N-13 swallow. Fix: give the
compact leg's `task-viewport` `vertical-stretch: 1` (the regular
leg's task-area absorption verbatim). The presentation test writes
the constraint probe first (reads the realized heights of
`add-task-compact` vs `Theme.space-6 + 2 * Theme.space-1` at the
compact breakpoint), watches it fail, then the fix lands and the
golden suite re-baselines per the r2 R-3 mechanism. If the probe
shows the leak is NOT stretch-driven, the finding goes to the report
and the mechanism is re-derived from the probe's numbers — the probe
is the oracle either way.

### D-9 — The #882 armed-multi-delete display half

Chunk 4 landed the flow (`DeleteTarget::Tasks`, arm/confirm/prune,
`view_state.rs:3633/3605/3872`) but no visible strip. Chunk 5 adds
the display: `models.rs` pushes a window-level typed fact

```rust
window.set_task_multi_delete_armed(<member count as i32, −1 = disarmed>);
```

(the `board_delete_armed` boolean precedent, `models.rs:1984` — but
the count caption needs the number, so the fact is the count with −1
as the disarmed sentinel). `app-window.slint` mounts the armed-delete
strip pair (Confirm/Cancel, the #784 grammar's third instance beside
the stack/board strips) while the fact ≥ 0; the strip routes
`ActionId::TaskDeleteConfirm`/`TaskMoveCancel`-class arms exactly as
the existing strips do (stage 2 reuses the stack strip's seam pair;
the confirm path is chunk 4's `ConfirmDelete` resolution — the
button routes the same confirm action the keyboard's confirm stroke
reaches; NO new verb beyond what the strips already share). The
caption shows the member count (the manual walkthrough grades the
wording per the origin §8 note).

### D-10 — Per-preset key candidates (the contract suite is the oracle)

Contexts for all three ids: Board, Today, StackDetail, TaskDetail
(the `TASK_CONTEXTS` slice). Candidates, recorded as candidates —
the keymap contract suite + `keymap_snapshots` regenerate is the
oracle; any collision re-picks from the same context's free set and
is recorded, never a dropped row (C-5):

| Preset | TaskScheduleToday | TaskScheduleTomorrow | TaskUnschedule |
|---|---|---|---|
| Standard | `F11` (F5–F10 took the detail run in chunk 1; F11 free — verify) | `Ctrl+F11` | `Ctrl+Shift+F11`? — prefer the CUA `F11`-family continuation; free-set fallback `Ctrl+Shift+U` |
| Emacs | `C-c C-y` (yank-to-today mnemonic; verify free) | `C-c C-o` | `C-c C-u` |
| Vi | the vi preset's Task-context free set; candidate `F11` if vi carries function keys, else the leader-grammar successor | as today | as today |

Stage 2 duties: grep each preset's chord inventory for the candidate
strokes BEFORE binding (the emacs `C-c` chord space occupancy check
the origin §7 mandates — `C-c C-t` is the complete verb, `C-c C-d`
the due editor, `C-c t` the today re-commit, `C-c C-w` the move);
bind; regenerate snapshots; the contract suite green.

### D-11 — Preset rows / row-set const discipline

The three new ids join `TASK_CONTEXT`'s successor (D-5's curated
builder replaces the flat `TASK_CONTEXT` walk for the task menu —
`TASK_CONTEXT` itself stays as the registry-order source the builder
filters, so the T-12/T-3 tests' source of truth survives). New
preset rows land as named const slices appended in the three preset
tables (the chunk-2 review's dedup law: `TASK_QUICK_ROWS: &[(ActionId,
ContextId, &[&str])]` shared where the stroke is preset-common,
per-preset consts where strokes differ). `TASK_CONTEXT_MULTI` is
untouched.

## 3. Test catalogue (§8 of the origin, spelled)

Journeys (`tests/journeys.rs`, step-ID names exactly):

- `us1_s1_schedule_today_commits_the_exact_mask` — open a card's
  menu, click the today bar button; the fake engine receives
  `UpdateTask` with `changes == TaskChanges { scheduled:
  Some(Some(<today end>)), ..default }` (exact mask, typed).
- `us1_s2_schedule_tomorrow_commits_the_exact_mask` — same, tomorrow's
  `next_day_window` end.
- `us1_s3_unschedule_commits_the_clear_twin` — scheduled subject;
  `scheduled: Some(None)` exact.
- `us1_ac5_bar_key_and_click_route_identically` — bar click vs the
  preset stroke route the identical `ActionId` → identical command
  (structural equality of the recorded commands).
- `us2_ac4_escape_ladder_steps_submenu_then_menu` — open Convert ▸
  via click and via key; Escape 1 closes the submenu (the menu
  stays: `shell.menu.open == true`, `task_group == None`), Escape 2
  closes.
- `us3_s1_open_context_menu_survives_the_dwell_interval` — THE REPRO,
  written first and watched red: open a board card's context menu,
  `driver.advance_time(...)` past the repaint cadence ×several +
  minute boundary (and force a refresh via a settings-free engine
  sync), then read the popup's `menu_entries` global — must still be
  the context rows (typed: the row set's action ids, never labels).
  Red before the fix (the swap reproduces), green after. Companion:
  `us3_s2_main_menu_dwell_regrey_stands` — the main menu's
  round-3 D27c re-grey under the same advance (US-3/AC-2).
- `us4_s1_card_play_routes_starttracking` /
  `us4_s2_card_pause_routes_stoptracking` — the card control's flips;
  the running badge decoration flips with them.
- `us5_s1_create_focuses_the_new_card` — inline create commit → the
  created card is `focused_task` and the FocusScrollHint edge fired
  toward its (column,row).
- `us5_s2_empty_settle_hands_off_nothing` — empty-buffer settle: no
  command, no arm, focus unchanged.

Seam tests (adapter/router unit layer):

- the three schedule ids' exact masks incl. the `NoTarget` backstops;
- `tomorrow_window_end` == `days::next_day_window(...).1` for a fixed
  `frozen_now` (the day-law parity twin);
- the tracking ids' card-focused routes; the one-shot flip (US-4/AC-3);
- the create-focus one-shot: a second armed create (or a vanishing
  create) hands off nothing;
- `entry_states`' verdict table extended (D-4's greys: Start under
  `tracking_elsewhere`, `SeriesPauseCommit` under the live pause arm).

Fold/presentation tests (`tests/presentation`-style probes +
`menu.rs` unit tests):

- `us2_ac1_stop_tracking_withheld_on_non_tracking_task` and the
  swapped halves — typed row-set compositions over synthesized
  `TaskMenuSubject`s (withheld = absent, greyed = present+disabled —
  presence/enablement, never text);
- the rule-less / series-less / habit / tracker subjects' withheld
  sets (US-2/AC-2's full matrix);
- the submenu tree composition: core rows + four parents, children
  preserving ids/hints, destructive last (US-2/AC-3);
- the empty-family parent cascade (a rule-less subject renders no
  Reminders ▸ parent);
- `us6_s1_the_done_checkbox_carries_the_hover_flag`;
- `us7_s1_the_compact_add_task_field_holds_its_height` (the D-8 probe);
- the quick-bar model composition (visible/enabled per subject).

Keymap: the contract suite covers the three new ids per preset
(totality + conflict detection + snapshot regeneration).

Kiosk gates: none new (the origin §8 records the decided absence).

`tests/seam_inventory.rs` gains the new seams (`on_quick_bar_invoked`,
`track-toggled`, the multi-delete strip pair) — the #760 fold stage 3
cross-checks.

## 4. Commit sequencing (each commit standalone, hooks green)

1. `test(ui): reproduce the open context menu's content swap under the mock clock` —
   the D-1 repro journey, red (documents N-11's mechanism).
2. `fix(ui): pin the object context menu's model against refresh re-derivation` —
   the MenuKind discriminator + the scoped guard; both US-3 journeys green.
3. `feat(ui): the task quick actions' schedule ids and dispatch funnels` —
   the three ActionIds (registry, contexts, router arms, the
   `tomorrow_window_end` ctx field) + exact-mask seam tests.
4. `feat(ui): the task context menu's quick actions bar` —
   QuickBarEntry, the slint bar section + icon kinds, the adapter
   build-at-open + seam, `us1_*` journeys + `us1_ac5`.
5. `feat(ui): the applicable-row filter and the submenu grammar` —
   TaskMenuSubject, task_context_page/task_submenu_page, the
   Submenu/TaskMenuGroup + shell ladder; presentation matrix + the
   escape-ladder journey.
6. `feat(ui): per-preset keys for the schedule trio` — preset rows,
   snapshots, contract suite; `us1_ac5` proves click ≡ key.
7. `feat(ui): the card play/pause tracking control` — model fact +
   widget + seam + `us4_*`.
8. `feat(ui): focus lands on the created task's card` — the one-shot
   arm + hint; `us5_*`.
9. `fix(ui): the done checkbox's hover treatment` — flag + probe.
10. `fix(ui): the compact leg's viewport absorbs its surplus height` —
    the probe (landed red in the commit's history if split; else the
    failing probe precedes the one-line fix in the same commit's
    build) + golden re-baseline.
11. `feat(ui): the armed multi-delete's visible strip and count` — D-9.
12. `docs(census): task card controls evidence` — the census cells
    (below) ride the code commits where they become true where
    possible; a trailing census-only commit sweeps any stragglers.

Census cells (origin §9, citing US/W ids in the same commits):
`task.scheduled_edit` Board `U (TE-1)` → `E (US-1)`; `task.track_start/
stop` Board `U (TE-3)` → `E (US-1, US-4)` with the dated supersession
note on r6 D-6 (round plan C-1); `task.create` focus evidence;
`app.menu` task-context evidence. #811 closes with the PR (C-1);
issue #882's display half lands here (comment + close at merge is the
dispatcher's).

## 5. Gate steady state

`scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md --gate`
must report every §6 row (W-1..W-7) and every step token
(US-1/S1–S4, US-2/S1–S2, US-3/S1–S2, US-4/S1–S2, US-5/S1, US-6/S1,
US-7/S1) FOUND. The `—`-command rows (W-3, W-6, W-7) resolve via
their journey-steps reason (bug fix / display only). Any MISSING is a
plan-vs-code defect — fix code or the origin plan until FOUND; never
weaken a row.

## 6. Risks

- **R-a**: the D-8 height mechanism is a hypothesis — the probe-first
  protocol makes the finding cheap; if the surplus path differs, the
  probe's numbers re-derive it.
- **R-b**: `MenuEntry.submenu` widening touches every projection site;
  the additive `Submenu` enum keeps `root_page`/`group_page` tests
  bit-stable, but stage 2 must sweep `submenu: None` constructors.
- **R-c**: the context menu's keyboard walk over a *two-panel +
  bar* surface — the cursor state machine addresses entries only
  (chrome excluded, the replace-mode back row precedent); the bar is
  pointer/click + the preset keys, deliberately not a cursor stop
  (the r4 D7 prominence reasoning, R-3's posture).
- **R-d**: the create-focus diff can race a concurrent capture; the
  zero-or-multiple-new-ids branch disarms silently by design (the
  one-shot law) — documented in the seam test.
- **R-e**: emacs/vi candidates may collide in the contract suite; the
  C-5 re-pick rule governs, recorded per retune.

## 7. Estimated diff

Production ~900 lines (menu.rs ~350, adapter.rs ~250, router.rs ~80,
actions.rs ~80, shell.rs ~30, models.rs ~60, presets.rs ~40,
view_state.rs ~30, slint ~250); tests ~1300. Total ~2.2–2.5k.
