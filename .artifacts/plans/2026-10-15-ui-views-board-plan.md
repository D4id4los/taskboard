# [Plan] UI Component Library & Board Views (GUI Phase 7, Chunk 3)

- **Date**: 2026-10-15
- **Kind**: implementation plan (chunk 3, stage 1 of the GUI Phase 7
  dispatch — TDD-first spec for the stage-2 implementation agent). No
  production code in this commit.
- **Origin**: accepted design round
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` (Status:
  ACCEPTED 2026-10-14; normative here: §4 all subsections — layers,
  rules A–E, presentation helpers, component inventory; §6.3–6.4
  responsive composition and board degeneration; §7.1–7.2 focus
  architecture and a11y discipline; §2.1 shipped surface, §2.3–2.4
  scope fence; §12 row 3; decisions Q4 (one view tree, no kiosk mode
  now), Q16 (unscaled breakpoints compact <720 / regular /
  expansive ≥1440), Q23 (overdue banner in v0.1.0, snooze/mute via
  actions)); GUI requirements brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md` §3
  (the feature surface this chunk must NOT build); dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1 (execution model & mandates, binding) and §4 (chunk-3
  covers/acceptance/non-goals — binding); reminders design round
  `.artifacts/plans/2026-10-12-reminders-design-plan.md` §7 (kiosk
  overdue-banner path) and §8 (the AppState/EngineSignal surface
  Phase 7 reads); chunk-1 plan
  `.artifacts/plans/2026-10-14-ui-shell-theme-plan.md` and chunk-2
  plan `.artifacts/plans/2026-10-15-ui-action-keymap-plan.md`
  (landed seams + their recorded decisions, esp. D18's text-input
  contract and D5's staged enabled bits).
- **Target**: `taskboard-ui-slint` only (component library, views,
  presentation/view-state/repaint modules, registry/router/shell
  amendments, slint markup). No other crate changes except the two
  insta snapshot files that regenerate from registry/preset edits.
- **Branch / worktree**: `feat/ui-views-board` in
  `../taskboard-WT-feat-ui-views-board`, off `main` @ `7aa6e93`
  (chunks 1 and 2 merged and verified: chunk-1 tip `20e3ca3`,
  chunk-2 tip `7aa6e93`).
- **Goal**: the v0.1.0 visible surface lands — the L1/L2 component
  library under the §4 layering rules, exactly three views (board,
  stack detail, task detail) plus the sync-status indicator and the
  persistent overdue banner, responsive board degeneration per Q16,
  the focus architecture and a11y discipline of §7.1–7.2, and the
  pure presentation helpers of §4.3 — all reading `ArcSwap<AppState>`
  and broadcasting signals without mutating anything, all verbs
  routing through the chunk-2 action layer. Every open question is
  settled by a recorded decision (§3); deviations from the design
  plan are enumerated with reasons (§11, marked **[DEV]**).
- **Status**: READY — stage 2 implements exactly this plan;
  deviations from *this* plan need a fresh recorded reason.

---

## 1. Context & verified facts (anchored — do not re-derive)

1. **Chunks 1–2 are merged** (`main` @ `7aa6e93`). The UI crate has:
   `ui/theme.slint` (`Theme`/`Typography`/`Semantic` globals,
   `base-scale`, spacing scale, `RoleText`), `ui/app-window.slint`
   (root `key-root` FocusScope with capture-phase interception, the
   `ActionMenu` conditional, `KeyboardState`/`UiStatus` globals),
   `ui/widgets/kbd-hint.slint` (L1 seed),
   `ui/compositions/action-menu.slint` (L2 seed), the pure modules
   `actions`/`keymap`/`resolver`/`router`/`menu`/`shell`, the slint
   `adapter` (single key path; `install()` owns shell+resolver+
   commands), `mount`/`run`/`quit`, `UiSettings { scale, keymap,
   startup }`, bundled fonts, `test_platform::on_slint_thread` (null
   platform), `token_tests`, and the token lint gate
   (`scripts/check_slint_tokens.py`, three rules, no escape hatch).
2. **The chunk-3 seams chunk 2 left are real and named**:
   - `UiHandle::set_view_context(Option<ContextId>)` and
     `UiHandle::set_focused_entities(Option<TaskId>,
     Option<StackId>)` (adapter.rs, "the chunk-3 focus seam").
   - `UiStatus.startup-keymap-findings` — pushed once at mount; "the
     Q15 startup-warning state the status bar renders (chunk 3)".
   - `UiHandle { _state: Arc<ArcSwap<AppState>>, _signals:
     broadcast::Receiver<EngineSignal>, _system:
     broadcast::Receiver<SystemEvent> }` — the underscore fields are
     this chunk's to consume.
   - Registry rows `nav.*`, `task.create`, `task.edit`, `task.move`
     are present but `enabled: false` ("chunk-3 staged", chunk-2 D5;
     pinned by `chunk3_staged_actions_are_disabled`).
   - **D18's recorded contract (binding on this chunk)**: "when a
     text input holds focus the chain must be a text context
     (`ViInsert`/`TaskDetail`-class) in which printable strokes do
     not resolve — typing passes through untouched … the contract
     above is recorded as chunk 3's focus-architecture obligation
     (its review checks it)."
3. **Preset binding facts** (presets.rs, verified): `standard` binds
   printables in the Board context (`N`=task.create, `M`=task.move,
   `Space`=task.complete, `Return`=task.edit) and arrows in all view
   contexts; `emacs` binds only Ctrl-chords/`Return` in view
   contexts; `vi` binds printables in `ViNormal` only, and
   `ViInsert` binds exactly one stroke (`Escape`→`mode.normal`).
   `standard`/`emacs` have **no** `ViInsert` bindings today.
4. **Domain surfaces the views read** (verified signatures):
   - `AppState { boards, stacks, tasks, labels, reminders:
     ReminderState, sync: SyncStatus, last_updated }`; helpers
     `live_tasks()`, `tasks_in_stack(StackId)` — canonical order
     `(order, id)`. **There is no domain/engine helper ordering
     stacks** (verified by grep across `taskboard-domain` and
     `taskboard-state`); the CLI lists stacks ad hoc.
   - `Task { id, title, description, duedate: Option<DateTime<Utc>>,
     done: Option<DateTime<Utc>>, stack, order, labels, archived,
     deleted, … }`; `is_live()`, `is_open()`.
   - `SyncStatus { phase: SyncPhase, last_success, pending_ops }`;
     `SyncPhase { Idle, Syncing, Offline, Failed { last_error:
     SyncErrorKind } }`.
   - Reminders (reminders.rs): `overdue_tasks(&BTreeMap<TaskId,
     Task>, now) -> Vec<&Task>` and `due_soon(tasks, now, horizon)`
     — canonical `(order, id)` order; `occurrence_of(rule,
     duedate)`; `ReminderState { rules, deliveries, snoozes, muted }`.
   - Commands (messages.rs): `StateCommand::{CreateTask, UpdateTask,
     SetTaskDone, MoveTask, DeleteTask, SnoozeReminder { reminder,
     occurrence, postpone }, SetTaskRemindersMuted { task, muted },
     …}` — everything the views dispatch already exists. `EngineSignal`
     has exactly one variant (`StateUpdated`); `SystemEvent` is
     `{NetworkLost, NetworkRestored, Shutdown, StorageFailed}`.
   - `AppState` carries no reminder *defaults* (`default_before` is
     engine-side config) — the UI cannot compute effective
     occurrences for `ReminderRef::Default` targets (fact that
     shapes D12).
5. **Reminders §7 banner contract (binding)**: the banner reads
   `ArcSwap` lock-free, calls pure `overdue_tasks(&state.tasks,
   now)` with `now` from the UI's own clock, repaints via
   `EngineSignal::StateUpdated` **plus a minute-granularity repaint
   timer** (time passing never mutates state), works from cache
   offline, renders before the first tick, cannot spam, and
   **ignores mute/snooze by contract** (board truth, not nagging
   policy). §8: `StateUpdated` is the only repaint signal; no new
   signal or `SystemEvent` variant.
6. **Slint 1.18 facts the design rests on** (stress report §3.1/
   §3.5/§3.6): FocusScope has `focus()`/`has-focus`/
   `focus-on-tab-navigation`/`focus-on-click`; capture-phase
   interception precedes the focused element; `slint::Timer` exists
   for the repaint driver; accessible-role/label/value are the a11y
   property surface; `slint::Window::dispatch_event(WindowEvent::
   KeyPressed)` injects keys for component tests (the chunk-2
   pattern — `i-slint-backend-testing` stays chunk 4's).
7. **Crate mechanics**: `uuid` is currently a **dev-dependency** of
   the UI crate only; `TaskId::from_uuid` exists but string parsing
   needs the uuid crate as a regular dep (avoided by D2). The UI
   crate already depends on `tokio` for channel types only, on
   `chrono` transitively via domain — presentation helpers need
   `chrono` types (already reachable through `taskboard-domain`
   public API; add `chrono.workspace = true` only if direct imports
   require it, preferring re-use of domain-exported types).
8. **House rules that bite here**: token lint forbids literal
   colours/fonts/spacing-literals outside `ui/theme.slint`
   (proportional/relative expressions exempt); AGENTS §5 — no text
   assertions (labels are element *addresses*; model *data* like
   task titles is fixture payload and assertable), typed outcomes,
   fakes over mocks, no quality ceilings; every commit standalone.

## 2. Scope

**In** (dispatch §4 "covers", restated): the §4.1 component library
(`ui/widgets/`, `ui/compositions/`, `ui/views/`) under rules A–E; L1
inventory `task-card` (+decoration slots), `badge`, `kbd-hint`
(formalised), `icon-button`, `text-field`, `list-row`, `status-dot`,
`empty-state`; L2 `board-layout`, `toolbar`, `status-bar`
(sync + overdue-banner slot) — **not** `overlay-host`; L3 exactly
board/stack/task views + the sync-status indicator + the persistent
overdue banner; presentation helpers (§4.3); the `Responsive` global
+ unscaled breakpoints + panel demotion + board degeneration
(§6.3–6.4); focus architecture + a11y discipline (§7.1–7.2);
flipping the staged `enabled` bits and wiring their handlers; the
two Q23 banner actions (D4); the repaint/state-read driver (D1).

**Out** (non-goals, dispatch §4 verbatim plus plan-level cuts):
nothing from design §2.3 (no search palette, capture input, today
view, overlays, toasts, kiosk rotation, telemetry hooks of any
kind); no `overlay-host`; no pointer drag-and-drop (D8, [DEV]);
no new engine/domain code; no due-date editing UI (D13); no
description editing UI (D13); no stack create/rename/delete UI
(D13); no second window, no runtime theme switching, no live re-map,
no `i-slint-backend-testing` (chunk 4), no goldens (chunk 4), no
journeys beyond the adapter-level component tests (chunk 4), no
ADR/docs amendments (chunk 5), no backlog ✅ edits (chunk 5; this
chunk only *appends* new out-of-scope ideas per AGENTS §7, §11).

## 3. Decisions (each binding for stage 2)

Decisions are numbered D1–D22 continuing the house style. [DEV]
items are deviations from the design plan's letter — all are
collected again in §11 with reasons.

### D1 — State reads: a slint-Timer repaint driver, full re-derive

The UI never mutates and never blocks. One repeating `slint::Timer`
(owned by `UiHandle`, cadence `REPAINT_CADENCE = 100 ms`, a named
const) does three things per tick on the slint thread:

1. drain `signals` (`EngineSignal`) and `system` (`SystemEvent`)
   receivers with `try_recv` until `Empty`/`Lagged` (`Lagged` is
   treated as "changed" — chunk 1's recorded semantics);
2. detect a wall-minute boundary change of `Utc::now()` (the
   reminders §7 minute-granularity time repaint);
3. if anything fired (signal, system event, minute tick, or a
   pending first-refresh flag), run `refresh()`.

`refresh()` is a public, deterministic, timer-free function on
`UiHandle` (also callable from tests): `ArcSwap::load` (lock-free
full snapshot) → build display models (D3/D10) → push one-way onto
window globals/properties. Full re-derivation per update at
kiosk-scale `BTreeMap` sizes is the deliberate cost model (recorded;
revisit only with profiling evidence). Edit buffers are never
touched by `refresh()` (D9 rule 4). The timer itself is thin glue
(untested by class, like `run`); `refresh` and everything it calls
is tested directly. `SystemEvent::Shutdown` reaching the UI is inert
here (the app-side exit watcher already owns quit); `NetworkLost`/
`Restored`/`StorageFailed` only matter as refresh triggers — sync
truth renders from `AppState.sync` (D11).

*Rationale*: the broadcast receivers cannot be awaited on the slint
thread (no runtime there); `slint::invoke_from_event_loop` bridging
would require an app-side tokio task and split the UI's state reads
across crates for no gain. A non-blocking drain on a UI-owned timer
keeps the crate IO-free and the whole repaint path testable.

### D2 — The slint boundary carries display models and indices, never ids

Views receive display-model structs (titles, due phrases, tones,
flags) built Rust-side. Interaction callbacks return **indices**
(column, row, field), which the adapter resolves against the id
tables (`Vec<TaskId>` etc.) it built when pushing the models — same
thread, rebuilt together, always consistent. No uuid strings cross
the seam; `uuid` stays a dev-dependency. Entity ids appear in
accessible labels only where the screen-reader address benefits
(task title is the label; no raw uuids in labels).

### D3 — New pure modules: `presentation`, `view_state`, `responsive`, `models`

- `src/responsive.rs` — pure: `Breakpoint { Compact, Regular,
  Expansive }`, `breakpoint_for_width(logical_px: f32) -> Breakpoint`
  (Q16: `< 720`, `720..=1439`, `≥ 1440`, unscaled consts).
- `src/presentation.rs` — pure display-domain mapping over domain
  types, **no slint types, nothing generated** (§4.3; full contracts
  in §4).
- `src/view_state.rs` — pure navigation/selection/flow state
  machine: current `ViewId`, board focus, compact stack selection,
  edit flows, move flow; produces the inputs for `shell.view` /
  focused entities and consumes `UiEffect`s (D7–D9).
- `src/models.rs` — the slint-struct assembly (generated types):
  builds the window model values from presentation output + view
  state. Thin by construction; exercised through component tests.

### D4 — Registry grows `reminder.snooze` / `reminder.mute` (Q23's "via actions")

Q23 (accepted) says the banner does "snooze/mute **via actions**",
and §7.1 requires every click affordance to have an action
equivalent — banner buttons that bypassed the registry would fail
the §7.1 registry-vs-handlers diff. The dispatch non-goal "no new
actions beyond the chunk 2 registry" cannot be read literally
against the accepted Q23; **Q23 wins** (the dispatch plan itself
cites Q23 as covered). Two rows are added:

| id | label | contexts | category | enabled |
|---|---|---|---|---|
| `reminder.snooze` | "Snooze reminder" | Global | Task | yes |
| `reminder.mute` | "Mute reminders" | Global | Task | yes |

No preset binds them (menu-reachable via `app.menu`, which satisfies
§7.1's keyboard-equivalence bar); the preset-coverage core-verb
list is unchanged. `DispatchCtx` grows `reminder_target:
Option<ReminderTarget>` (`{ task, reminder, occurrence }`) set by
the adapter when a banner row is activated; `postpone` is the const
`SNOOZE_POSTPONE = 1 h` in the router. Semantics recorded honestly
in §8/D12: banner snooze records the user intent as a snooze link
(reminder = first explicit rule with `occurrence_of(rule, duedate)`
when rules exist, else `ReminderRef::Default(task)` with the due
anchor — engine-legal, `AppState`-visible, and the *nag* effect
lands with plan-5 toasts); banner mute is one-way
(`muted: true`; unmute stays CLI/POST-1.0).

### D5 — Editing = insert mode; insert mode masks the view context (D18's contract, honoured by chain discipline)

Entering any edit flow (create row, task-title edit) applies
`SetMode(Insert)` **and** the shell's chain rule changes: while
`mode == Insert`, `KeymapShell::chain()` returns
`[ViInsert, Global]` — the view context is masked, exactly as the
open menu masks today. Consequences per preset (fact §1.3):

- **vi**: unchanged behaviour (`ViInsert` binds only `Escape`).
- **standard**: `[ViInsert, Global]` resolves no printables — typing
  flows to the `TextInput`; `Return`/`Escape` are unbound there and
  reach the field's native `accepted` callback / cancel path. To give
  standard and emacs the same exit key vi has, **both presets gain
  one `ViInsert` binding `Escape → mode.normal`** (an existing
  action; resolved-keymap snapshot regenerates). Cancel-edit rides
  the `SetMode(Normal)` effect: the adapter, on
  `SetMode(Normal)`-while-editing, discards the buffer and returns
  focus to the view scope.
- **emacs**: same as standard; known wrinkle recorded — `Ctrl+g` is
  a chord prefix (redo) and will show `PrefixPending` while editing;
  `Escape` is the reliable cancel.

Title commit rides the `TextInput`'s native single-line `accepted`
callback (the §5.1 text-input exception class — text fields receive
text natively; this is the field's own activation, not a view key
handler). While editing, arrows/Ctrl-chords that resolve in the
chain still act (deliberate: `Ctrl+Q` must keep working mid-edit).

`Mode::Insert` is meaningful only while a text field is focused:
in vi, `i`/`a` in Board opens the create row (D8) rather than
flipping mode into the void.

### D6 — The staged bits flip; the router grows UI effects, not domain knowledge

`nav.next-view`, `nav.prev-view`, `nav.focus-board`,
`nav.next-item`, `nav.prev-item`, `task.create`, `task.edit`,
`task.move` flip to `enabled: true`. New `UiEffect` variants:
`FocusNext`, `FocusPrev`, `NextView`, `PrevView`, `FocusBoard`,
`BeginCreate`, `BeginEdit`, `BeginMove`. The router stays pure and
exhaustive: these actions map to the effects; `reminder.snooze`/
`reminder.mute` map to `Command(...)` built from
`ctx.reminder_target` (`NoTarget` without one). The **adapter**
resolves effects against `view_state` (focus maths needs the model,
which the router never sees — chunk-2 D16's seam pattern extended,
not bent). Chunk-2's `chunk3_staged_actions_are_disabled` test is
rewritten (not weakened) into `only_reserved_actions_stay_disabled`
— its contract changes role from "staged" to "reserved-only", which
is the honest post-flip assertion; the reserved set itself is still
pinned by `reserved_globals_are_present_and_disabled`.

### D7 — View state: one `ViewId` cycle, model-driven focus

`ViewId { Board, StackDetail, TaskDetail }` (Rust-side; mirrored as
a slint enum for the conditional mounting). `nav.next-view`/
`prev-view` cycle Board → StackDetail → TaskDetail → Board.
StackDetail opens on the currently focused stack (Board) or the
task's stack (from TaskDetail); TaskDetail opens on the focused
task; entering a view snapshots its subject (stack id / task id) so
later focus moves in the background view don't retarget it.
`nav.focus-board` switches to Board and focuses its scope.
Refresh-time reconciliation (D1) re-validates subjects: a deleted
task pops TaskDetail → Board; a deleted stack pops StackDetail →
Board and re-falls-back the compact selection; an empty board
renders `empty-state`. Board focus: `BoardFocus { task:
Option<TaskId> }`; `focused_stack` is always derived (the focused
task's stack; `None` when no focus). `nav.next-item`/`prev-item`
walk cards in reading order — columns in stack order, tasks in
canonical order — **without wrap** (stops at the ends; vim-faithful,
recorded). Compact reading order is the selected stack's tasks only.

### D8 — Board degeneration: `BoardMode` + stack switcher + collapsed decorations

`board-layout` reads `Responsive.current-breakpoint` and renders:

- **regular/expansive**: one column per live stack (order: D10),
  horizontal scroll when overflowing, full-decoration cards.
- **compact**: a stack switcher (segmented row of stack titles, the
  first focusable element of the board) + the selected stack's single
  column; task-cards render with `compact: true`, which collapses
  tertiary (rule-D) decorations; the due chip stays (secondary).

Stack switching (compact): the switcher is a focusable container —
when it holds focus, `nav.next-item`/`nav.prev-item` move the
switcher selection (selection-follows-focus, one keypress per
switch); clicking a segment does the same via the same callback.
On switch, board focus resets to the first task of the new stack.
The switcher is hidden in regular/expansive (its duty is the column
row itself). **Create flow** (all modes): `task.create` opens an
inline footer row ("new task") in the focused stack (compact: the
selected stack) containing a single-line `text-field`, auto-focused,
insert-mode per D5; `Return` commits
`CreateTask { title, stack, order: append }` (append = max canonical
order + 1, computed from the state slice), `Escape` discards.
**Move flow** (all modes): `task.move` starts `MoveFlow { task,
target: focused_stack }`; while active, `nav.next-item`/`prev-item`
cycle the **target stack** (in compact that is exactly the switcher
highlighting — one mechanism), and `task.move` pressed again cancels
(toggle — zero new bindings). Confirm rides `Return`: it is bound to
`task.edit` in Board (standard/vi), and a move flow interprets the
`BeginEdit` effect as **confirm** (D9's flows-interpret-effects
rule), dispatching `MoveTask { id, stack: target, order: append }`. The
status bar shows the flow state with a `kbd-hint` projection of the
`task.move` binding (D11). **Pointer drag is [DEV]-deferred** (D8 in
§11): the move flow is the complete, keyboard-first path at every
breakpoint; Slint 1.18 has no widget-level DnD, and hand-rolling it
is disproportionate to the chunk (see §11 for the full reason).

### D9 — Flows interpret effects; the adapter is the only interpreter

`view_state` exposes `apply_effect(&mut self, effect) -> Vec<
FlowOutcome>` where `FlowOutcome` is `{ Command(StateCommand),
SetMode(Mode), Focus(Field), RefreshNow, None }`-shaped: edit and
move flows reinterpret `FocusNext/FocusPrev/BeginEdit/BeginMove/
SetMode` contextually (move flow: FocusNext cycles targets, BeginMove
toggles cancel, BeginEdit confirms; edit flow: SetMode(Normal)
discards, anything else commits-then-proceeds only for view-leaving
effects — recorded rule: **leaving the edit field by effect first
commits the buffer** except cancel). Rules that keep this safe:

1. Text buffers live in the slint `TextInput`s and in a Rust mirror
   the adapter updates on every keystroke callback (`edited(string)`
   on text-field) — commit reads the mirror, so no slint-state
   dependency.
2. Effect application is single-threaded on the slint thread
   (callbacks + timer all land there); no locking anywhere.
3. One flow is active at a time (edit XOR move); starting one ends
   the other per its rules.
4. `refresh()` never pushes text properties while its view's flow is
   active (buffers are user-owned until commit/discard).

### D10 — Presentation display-ordering (verified: domain owns task order only)

`stacks_in_board(state, board) -> Vec<&Stack>`: live stacks of the
board, `(order, id)` ascending — **presentation's own display
ordering**; the domain defines no stack-order helper (fact §1.4,
verified by grep; the task comparator `AppState::sorted_tasks` is
the precedent convention). `live_board(state) -> Option<&Board>`:
the single non-deleted, non-archived board (MVP 0..1). Due
classification is **set-membership over the domain functions** (§4)
— no predicate reimplementation anywhere.

### D11 — Status bar (L2): sync badge + startup warning + banner slot + flow hint

A bottom chrome row (chrome lives in the window shell per Q4's
mitigation note), declared:

```
[ status-dot + sync phase label + pending badge ] [ startup-warning
  indicator (count badge, warning tone, non-interactive) ]
[ move-flow hint (kbd-hint + conditional label, only while moving) ]
— and above it the overdue-banner slot (D12)
```

Sync badge states map from `SyncPhase` via the pure
`sync_badge(&SyncStatus) -> SyncBadge` (§4): the phase enum and
pending count travel to the markup, where conditionals derive tone
(Idle→neutral, Syncing→`sync-active`, Offline→`offline`,
Failed→`danger`) and the label text from a typed `SyncPhaseUi`
("last synced" recency only as a `caption` when `last_success` is
recent); `pending_ops > 0` renders a `badge` count.
Labels are English v0.1.0 (no i18n — recorded). The sync indicator
is informational (not focusable, role `text`, stable label
"Sync status"). The startup-warning surface renders
`UiStatus.startup-keymap-findings > 0` as a warning-toned count —
non-interactive (the settings deep-link is the POST-1.0 Q15
journey). The move-flow hint is a `kbd-hint` of the current preset's
`task.move` binding (a `menu::`-style projection of keymap data,
reusing D19's `first_binding_display`) plus a conditional phrase.

### D12 — Overdue banner: truth list with per-row actions, riding state + minute tick

The banner slot renders when `overdue_tasks(&state.tasks, now)` is
non-empty (domain fn, canonical order): one `list-row` per overdue
task (title + due phrase in `overdue` tone) with two trailing
`icon-button`s (snooze, mute). Row activation (click or keyboard
focus via Tab into the banner rows) sets `focused entities` +
`ctx.reminder_target` (D4) — the target for rule-having tasks is
the first explicit rule with `occurrence_of(rule, duedate)`;
rule-less tasks use `ReminderRef::Default(task)` with the duedate
anchor (occurrence is opaque link data by engine contract; the nag
effect of these links lands with plan-5 toasts — recorded honestly,
the v0.1.0-observable effect is the snooze link in
`AppState.reminders` and the muted set for mute). Visibility never
consults mute/snooze/deliveries (reminders §7 contract). The banner
must not flood the layout: the slot renders at most
`BANNER_ROW_CAP = 3` rows plus a "…and N more" `badge` (layout cap
only — the truth is the count; recorded as presentation cap, not a
domain claim). The banner is chrome (visible in every view), part of
`status-bar`.

### D13 — What the views deliberately do NOT edit (recorded scope cuts)

- **Due date**: displayed (mono role + due phrase), not editable —
  date parsing/validation UX is its own surface; CLI covers it in
  v0.1.0. [scope cut, backlog]
- **Description**: displayed read-only (body role, plain text — no
  markup rendering, R10/org is POST); editing deferred (the journey
  bar needs title only). [scope cut, backlog]
- **Stacks**: create/rename/delete are CLI-only in v0.1.0 — no
  registry actions exist for them and adding them would exceed even
  D4's Q23-mandated growth. Column headers are display + activation
  only. [scope cut]
- **Labels**: not rendered on cards in v0.1.0 (decoration slots
  arrive empty; label chips are a W1-era decoration — recorded so
  the omission is visibly deliberate).

### D14 — Task detail = title-editing surface + metadata display

`task.edit` (or `Return` on a card, or card double-activation —
same action) opens TaskDetail with the title `text-field` focused
and selected: `Return` commits `UpdateTask { changes: TaskChanges {
title: Some(..), ..default } }` and stays (focus returns to the
view scope), `Escape` discards. Displays: stack title (caption),
due phrase (mono, tone), description (body, read-only), completion
state; `task.complete`/`task.delete` act on the viewed task from
every context that offers them. `task.delete` from TaskDetail pops
to Board after dispatch.

### D15 — Focus architecture (§7.1): scope map, tab order, rings

One root scope (chunk 2, unchanged — capture interception is
focus-independent by tunneling order). Per-view scopes:

```
AppWindow (forward-focus: key-root)
key-root FocusScope                     — capture only; NOT a tab stop
└─ VerticalLayout
   ├─ toolbar: menu icon-button                      [tab 1]
   ├─ view-stack area (conditional mount):
   │   BoardView FocusScope                           [tab 2]
   │     ├─ stack switcher segments (compact only)    [tab 3..k]
   │     └─ create-row text-field (while open)        [tab k+1]
   │   StackDetailView FocusScope                     [tab 2']
   │     └─ create-row text-field (while open)
   │   TaskDetailView FocusScope                      [tab 2'']
   │     └─ title text-field
   └─ status-bar:
       └─ banner snooze/mute icon-buttons             [tab last]
```

Tab order is declaration order (§7.1); only genuinely focusable
elements carry `focus-on-tab-navigation`: the toolbar button, view
scopes, switcher segments, open text fields, banner buttons. Task
cards are **not** tab stops — their navigation is `nav.next-item`
(action layer) and their keyboard indicator is the selection ring
(not Slint focus). Every FocusScope renders its ring while
`has-focus` (border width/colour from `Theme.focus-ring-width`/
`Semantic.focus-ring` — always rendered, no keyboard-detection
heuristics). The selected card renders the same ring tokens
(selection is view state, always visible). Programmatic focus uses
the `.slint` `focus()` seam only (each view's scope focuses itself
on mount — conditional mounting creates the element, `init` grabs
focus; chunk 2's root `init => key-root.focus()` stays as the
pre-view fallback). Mouse-click discipline: every `TouchArea`
handler either (a) sets focus/selection state only, or (b) routes a
registry action (after preparing ctx) — the stage-3 review diffs
handlers against the registry per §7.1.

### D16 — a11y inventory (§7.2): role + label on every interactive element and information-bearing region

Slint 1.18's surface is role/label/value-centric (fact §1.6). The
inventory (labels are the chunk-4 journey addresses — stable
strings, English):

| Element | accessible-role | accessible-label |
|---|---|---|
| toolbar menu button | button | "Command menu" (registry label) |
| board view scope | group | "Board" |
| stack column | list | stack title |
| task card | listitem | task title (+ due phrase) |
| stack switcher | tablist / tab (per segment) | "Stacks" / stack title |
| create-row field | textinput | "New task title" |
| stack detail scope | group | "Stack: " + title |
| task detail scope | group | "Task: " + title |
| title field | textinput | "Task title" |
| description region | text | "Description" |
| due-date region | text | "Due date" |
| sync indicator | text | "Sync status" (value = phase enum) |
| startup warning | text | "Keyboard configuration warnings" |
| banner region | group | "Overdue tasks" |
| banner row | listitem | task title |
| snooze button | button | "Snooze reminder" (registry label) |
| mute button | button | "Mute reminders" (registry label) |
| empty-state | text | the empty-state text |
| action menu rows | (chunk 2's menu: already addressed) | registry labels |

Compound widgets expose semantic children only (cards/rows hide
internal layouts; the switcher exposes tabs, not rectangles). The
inventory table is copied into the plan's test catalogue (§6) as
the stage-3 review checklist; chunk 4's ElementQuery-by-role/label
makes any gap mechanically visible then.

### D17 — `Responsive` wiring: Rust owns the mapping, slint owns the reading

`ui/responsive.slint`: `export enum Breakpoint { compact, regular,
expansive }` + `export global Responsive { in-out property
<Breakpoint> current-breakpoint: Breakpoint.regular; }`. The window
pushes logical width on change: `changed width =>
root.window-width-changed(self.width / 1px)` (a pure Rust-installed
callback) → `responsive::breakpoint_for_width` → the generated
`Responsive` setter; the initial push happens in `mount` after
window creation using `preferred-width` (and the first `changed`
fires on real layout). Guards: widths ≤ 0 are ignored (the null
platform reports 0×0 — the global keeps its default), and the
mapping consts live only in `responsive.rs` (unscaled, Q16).
Compositions read `Responsive.current-breakpoint`, never window px
(§6.3). Panel demotion declarations (the §6.3 "one rule" table):

| Composition | primary | secondary | tertiary |
|---|---|---|---|
| board-layout | task columns (compact: single column + switcher) | column headers (compact: merged into switcher) | per-card decorations (compact: collapsed) |
| toolbar | the row itself | — | — |
| status-bar | sync + banner | startup warning | flow hint |
| stack-view | task list | header | metadata |
| task-view | title field | description | metadata row |

Demotion under compact follows this order uniformly (conditionals on
`Responsive` inside each composition; the table is the design
declaration the review diffs against).

### D18 — `Tone` vocabulary (rule A's property surface)

`export enum Tone { neutral, info, success, warning, danger, overdue,
sync-active, offline }` in `ui/theme.slint` (a theme-level closed
vocabulary alongside the text roles; adding a tone is a theme
decision). Primitives map `Tone` → `Semantic` colours internally via
conditionals (3–8 branches each — token-only by construction, lint
clean); views never name colours (rule A). The due-kind/phase enums
map onto `Tone` in the same markup conditionals (presentation stays
colour-free, §4).

### D19 — Icon strategy: geometric primitives, no icon font

`icon-button` takes `in property <IconKind>` (closed enum: `menu`,
`snooze` (clock), `mute` (bell-off), `warn` (triangle)); icons are
tiny compositions of `Rectangle`/`Ellipse`/`Path` inside the
`icon-button` file, coloured from `Tone`→`Semantic`. No icon font is
bundled (font additions are theme-level decisions with licence
record steps; geometric shapes are golden-deterministic and
offline-proof). If a richer icon set is wanted later it is a
theme-level decision.

### D20 — Component file layout (§4.1) — final tree

```
ui/
  theme.slint                 (+ Tone enum, D18)
  responsive.slint            (D17)
  app-window.slint            (chrome: toolbar / views / status-bar; width push)
  widgets/                    L1
    kbd-hint.slint            (chunk 2 seed, unchanged)
    badge.slint  status-dot.slint  empty-state.slint
    icon-button.slint  text-field.slint
    list-row.slint  task-card.slint
  compositions/               L2
    action-menu.slint         (chunk 2, unchanged)
    toolbar.slint  status-bar.slint  board-layout.slint
  views/                      L3
    board-view.slint  stack-view.slint  task-view.slint
```

`view-stack` is **not** a separate file: a generic L2 stack cannot
take views as children in Slint without referencing L3 from L2 (rule
C violation) — its substance (navigation + focus hand-off) is the
`view_state` cycle + conditional mounting in `app-window.slint`
([DEV], §11). `overlay-host` is not built (dispatch §4). Rule C/D/E
conformance: no widget imports a composition; no composition imports
a view; compositions hold layout only (domain-shaped *display
models* are their inputs — rule B's compositions clause); every
future-surface decoration rides `task-card`'s slot property (D21).

### D21 — task-card decoration slots (rule D, the fence against forks)

```slint
export struct TaskDecoration {
    kind: DecorationKind,   // importance | urgency | habit | reminder
    label: string,
    tone: Tone,
}
```

`task-card` exposes `in property <[TaskDecoration]> decorations: []`
and `in property <bool> compact: false`. Internally each decoration
renders as a `badge`-class chip after the due chip; the kind→tier
table lives inside `task-card` (v0.1.0: **all rule-D decorations are
tertiary** — `compact` collapses them; the W1 importance/urgency
wave revisits per-kind tiers as part of its design round). v0.1.0
call sites pass `[]` everywhere — the slot exists, nothing
speculative is built behind it (no pre-made decoration renderers
beyond the generic chip the badge primitive already provides).

### D22 — Per-component property contracts (rule B)

Rule B applied per layer — primitives take domain-meaningful display
data, compositions take layout parameters + display models, views
take state slices + action dispatchers (callbacks). Full table:

| Component | Layer | Properties (in unless noted) | Callbacks |
|---|---|---|---|
| `task-card` | L1 | `model: TaskCardModel { title, due-phrase, due-kind: DueKind, done, emphasis: bool }`, `decorations: [TaskDecoration]`, `compact: bool` | `activated()` |
| `badge` | L1 | `text: string`, `tone: Tone` | — |
| `kbd-hint` | L1 | `keys: [string]` | — |
| `icon-button` | L1 | `icon: IconKind`, `tone: Tone`, `label: string` (a11y) | `clicked()` |
| `text-field` | L1 | `placeholder: string`, `label: string` (a11y), `initial-text: string`, `selected-on-focus: bool`; in-out `text: string` | `edited(string)`, `accepted(string)`, `cancelled()` |
| `list-row` | L1 | `text: string`, `meta: string`, `tone: Tone`, `emphasis: bool` | — |
| `status-dot` | L1 | `tone: Tone`, `label: string` (a11y) | — |
| `empty-state` | L1 | `text: string`, `hint: string` | — |
| `toolbar` | L2 | `title: string`, `menu-hint: [string]` | `menu-clicked()` |
| `status-bar` | L2 | `sync: SyncBadgeModel`, `startup-findings: int`, `banner: [BannerRowModel]`, `flow-hint: FlowHintModel` | `snooze-clicked(int)`, `mute-clicked(int)`, `row-activated(int)` |
| `board-layout` | L2 | `stacks: [StackColumnModel]`, `selected-stack: int`, `move-target: int` (stack index, −1 = none) | `task-activated(int,int)`, `task-clicked(int,int)`, `header-activated(int)`, `switcher-activated(int)`, `create-committed(int,string)` |
| `board-view` | L3 | `board: BoardModel` (title + `stacks` + focus/flow fields) | the board-layout callbacks + `create-cancelled()` |
| `stack-view` | L3 | `stack: StackDetailModel` | `task-activated(int)`, `task-clicked(int)`, `create-committed(string)`, `create-cancelled()` |
| `task-view` | L3 | `task: TaskDetailModel` | `title-committed(string)`, `title-cancelled()` |

Slint struct/model types are declared next to their consumer
(`MenuEntryModel` precedent) and constructed only by `models.rs`.
No component takes colours, font attributes, or raw px (rule A +
lint); no component holds key handlers (§5.1); `text-field` wraps
`TextInput` (the §5.1 exception, contained in one primitive).

## 4. Presentation helpers (§4.3) — signatures and reuse

`src/presentation.rs`, pure, slint-free, `#[must_use]` throughout:

```rust
pub enum DueKind { None, Done, Later, Soon, Overdue }

/// Due classification inputs, computed ONCE per refresh by calling
/// the domain functions collection-side (no predicate duplication):
pub struct DueSets { overdue: BTreeSet<TaskId>, soon: BTreeSet<TaskId> }
pub fn due_sets(state: &AppState, now: DateTime<Utc>) -> DueSets
    // = id-keys of overdue_tasks(&state.tasks, now) and
    //   due_soon(&state.tasks, now, DUE_SOON_HORIZON).
    // The overdue/due-soon semantics exist ONLY in the domain fns;
    // these sets are their outputs, keyed for membership asks.

pub fn due_kind(task: &Task, sets: &DueSets) -> DueKind
    // done → Done; no duedate → None; overdue-set → Overdue;
    // soon-set → Soon; else Later. Set membership, no reimplementation.

pub enum DurationBucket { Now, Minutes(u32), Hours(u32), Days(u32) }
pub fn duration_bucket(delta: chrono::TimeDelta) -> DurationBucket
    // the numeric bucketing behind due phrases (typed → testable);
    // string assembly is trivial glue over the bucket.

pub fn due_phrase(kind: DueKind, duedate: Option<DateTime<Utc>>, now) -> String
    // display string (mono role); tests assert kinds/buckets, not
    // these strings (AGENTS §5 no-text rule).

pub struct SyncBadge { pub phase: SyncPhaseUi, pub pending: u32 }
pub enum SyncPhaseUi { Idle, Syncing, Offline, Failed }
pub fn sync_badge(sync: &SyncStatus) -> SyncBadge

pub struct BannerRow { pub task: TaskId, pub kind: DueKind,
                       pub target: ReminderTarget }
pub fn banner_rows(state: &AppState, now: DateTime<Utc>) -> Vec<BannerRow>
    // overdue_tasks order; snooze target per D12 (first rule +
    // occurrence_of, else Default(task) + duedate anchor).

pub fn stacks_in_board(state: &AppState, board: BoardId) -> Vec<&Stack> // D10
pub fn live_board(state: &AppState) -> Option<&Board>                    // D10
```

`DUE_SOON_HORIZON = 24 h` — a presentation **const**, not config:
the domain comment anticipates "user config" but Q22 fixes the
v0.1.0 `[ui]` surface at scale + keymap; recorded here as the
overruling decision (tunable const, one line). `DueKind`/
`SyncPhaseUi` are **defined in `presentation.rs` and are slint-free**
(D3's purity claim holds literally — the module imports nothing
generated). The markup needs enum-valued properties for its
conditionals, so `models.rs` maps the presentation enums 1:1 onto
the generated slint enums (`DueKindUi`, `SyncPhaseUi` in slint) and
the *tone* mapping (phase/due-kind → `Tone` → `Semantic`) lives in
markup conditionals inside the primitives (D18) — presentation never
carries colours.

**Proptest catalogue** (time-boundary formatting, per dispatch):

1. `due_sets` boundaries: for `offset ∈ [-48 h, +48 h]` around a
   generated `now` (1 s granularity): `duedate = now + offset` →
   membership matches `< now` (strict) and `(now, now+24h]`
   (inclusive right edge) exactly — pinned against independently
   computed predicates *in the test* (test-side recomputation for
   verification is the AGENTS-sanctioned direction; production has
   one implementation).
2. `duration_bucket` monotonicity + bucket edges (59 s→Minutes,
   60 s→Minutes(1)…; hours at 3600 s; days at 24 h; saturation at
   `TimeDelta` extremes — no panics, total on arbitrary input).
3. `banner_rows`: live+open+overdue only (tombstoned/done/future
   excluded), canonical `(order, id)` order, targets well-formed
   (rule tasks → `Rule` ref with `occurrence_of`; rule-less →
   `Default` ref with duedate anchor — proptest over generated
   states with rules/no-rules mix).
4. `stacks_in_board`: filters other boards + deleted stacks; order
   `(order, id)` asc (proptest vs sorted model in test).
5. `sync_badge`: exhaustive variant mapping (unit, not proptest).

## 5. Implementation steps (TDD order; every commit standalone)

Each step is red→green; commits carry `feat(ui-slint)`/`test(ui-slint)`
subjects with bodies; the pure modules land before the slint markup
that consumes them (chunk-2's proven ordering).

1. **`responsive.rs` + `ui/responsive.slint`**: TDD the mapping
   (boundaries, ≤0 guard); the global + `changed width` push in
   `app-window.slint` + adapter callback; component test: setter
   round-trip on the null platform.
2. **`presentation.rs`**: due_sets/due_kind/duration_bucket/
   due_phrase/sync_badge/stacks_in_board/live_board/banner_rows —
   full unit + proptest catalogue (§4) first, then impl.
3. **Registry/router/shell/menu edits (D4–D6)**: reminder rows +
   enabled flips + new effects + `reminder_target` ctx + chain
   masking (`mode == Insert` → `[ViInsert, Global]`) + preset
   `Escape → mode.normal` (standard/emacs `ViInsert`); rewrite
   `chunk3_staged_actions_are_disabled` → `only_reserved_actions_
   stay_disabled`; regenerate both insta snapshots (reviewed);
   router/shell/menu unit tests extended.
4. **`view_state.rs`**: ViewId cycle, board focus walk (reading
   order, no wrap), compact stack selection, reconciliation rules,
   edit flows (open/commit/cancel/buffer mirror rules), move flow
   (target cycle, toggle-cancel, confirm, append order); pure unit
   tests with fixture `AppState`s (domain `test_support` strategies
   where useful).
5. **L1 widgets**: `badge`, `status-dot`, `empty-state`,
   `icon-button` (+IconKind shapes), `text-field` (TextInput wrap,
   edited/accepted/cancelled), `list-row`, `task-card`
   (+`TaskDecoration` slot, `compact`). Markup-only (rendered shape
   reviewed via the chunk-3 component tests' model pushes; token
   lint runs per commit).
6. **L2 compositions**: `toolbar`, `status-bar` (sync badge +
   startup warning + banner slot + flow hint), `board-layout`
   (responsive columns/switcher, demotion per D17 table).
7. **L3 views + window chrome**: `board-view`, `stack-view`,
   `task-view`; `app-window.slint` grows toolbar/views/status-bar
   conditional mounting + view-scope focus-on-init + the width
   callback; a11y roles/labels per D16 on every element as it is
   written.
8. **`models.rs` + adapter/refresh wiring (D1–D3)**: model builders
   from `AppState` + `view_state`; `UiHandle::refresh()`; the
   repaint Timer; adapter grows the view callbacks (indices → ids →
   state changes/effects/commands), `shell.view` tracking,
   `reminder_target` management, edit-buffer mirroring, flow
   interpretation (D9); `_state`/`_signals`/`_system` become real
   fields (`state`, `signals`, `system`).
9. **Component tests** (§6): fake-state render asserts, breakpoint
   demotion asserts, editing-chain safety (typing dispatches
   nothing; Escape exits), banner action end-to-end (exact
   `StateCommand`s at the mpsc), view-switch focus/init asserts at
   the property level.
10. **Gate suite** green (fmt, clippy `-D warnings`, nextest,
    doc-tests, deny, llvm-cov) + token lint + contrast checks over
    the new markup; stage-2 report lists the landed inventory and
    deviations.

## 6. Test catalogue (TDD ledger)

**Rust unit/proptest** (modules as §5): responsive boundaries (1);
presentation catalogue §4 items 1–5 (3 proptests, 2 unit sets);
view_state: reading-order walk fixtures, cycle semantics,
reconciliation (focused-task-deleted, stack-deleted fallback,
empty-board), edit flow (commit produces `CreateTask`/
`UpdateTask` with append order; cancel discards; buffer mirror),
move flow (target cycle, toggle cancel, confirm produces `MoveTask`
append, flow-exclusive rules); router: every flipped action →
effect; snooze/mute → exact commands given target, `NoTarget`
without; reserved set still disabled; shell chain: insert-mode
masks view context (updated T23-class test); menu: board chain
offers the now-enabled verbs (unit; snapshot covers the dump).

**Component tests** (`test_platform::on_slint_thread`, chunk-2's
`dispatch_event` pattern — no sleeps, typed asserts, model data is
fixture payload):

1. `refresh` from a fake `AppState` (two stacks × tasks, one
   overdue, mixed sync phase): window getters assert column count,
   per-column card counts, banner row count, sync badge phase —
   structural facts.
2. Breakpoint demotion: set `Responsive.current-breakpoint` through
   the generated setter → switcher visibility, single-column
   rendering selector, `compact` propagation (board-layout exposes
   `compact-active: bool` as a read-back property for this assert —
   a rendering fact, not test-only surface).
3. Editing safety (standard preset): open create row via `N`;
   inject `h` → no command, no feedback, buffer mirror advanced;
   inject `Escape` → flow closed, mode normal; inject `Ctrl+Q`
   path stays live (quit effect — asserted via a `quit-requested`
   counter seam on the adapter, not by killing the platform).
4. Banner end-to-end: overdue fake state; activate row 0; invoke
   snooze → exact `SnoozeReminder` at the mpsc (typed fields incl.
   `postpone`); mute → exact `SetTaskRemindersMuted`.
5. Move flow: begin via `M`; arrows retarget (assert via
   `move-target` read-back); `Return` confirms → exact `MoveTask`;
   `M` again cancels (no command).
6. View cycle: `Ctrl+PageDown` chain → view enum property advances
   Board→StackDetail→TaskDetail; deleted-focused-task refresh pops
   TaskDetail→Board (property-level).
7. Registry/menu surface: menu entries for the Board chain include
   the enabled task verbs and the reminder actions (row-count +
   enabled-bit asserts against `menu::entries` — the snapshot
   already covers the dump).

**A11y inventory checklist** (review-time, D16 table): stage 3 (and
stage 2 self-check) diff every `TouchArea`/`TextInput`/FocusScope/
information-bearing `Text` in the new markup against the D16 table
— an interactive element missing `accessible-role`+`accessible-
label` is a finding; chunk 4 turns this mechanical.

## 7. File manifest (end state)

```
crates/taskboard-ui-slint/
  src/{responsive,presentation,view_state,models}.rs     (new)
  src/{actions,router,shell,adapter,lib,menu}.rs          (edited)
  ui/responsive.slint                                     (new)
  ui/theme.slint                                          (+Tone)
  ui/app-window.slint                                     (chrome+views)
  ui/widgets/{badge,status-dot,empty-state,icon-button,
              text-field,list-row,task-card}.slint        (new)
  ui/compositions/{toolbar,status-bar,board-layout}.slint (new)
  ui/views/{board-view,stack-view,task-view}.slint        (new)
  tests/keymap_snapshots.rs + snapshots                   (regen)
```

Estimated diff: ~2 500–3 500 added lines (≈60% Rust incl. tests,
≈40% slint), 6 edited Rust files, 1 edited slint file, 2
regenerated snapshots.

## 8. Acceptance mapping (dispatch §4 → this plan)

| Dispatch acceptance | Where |
|---|---|
| three views + two indicators render from injected `AppState` (no engine) | D1 `refresh`, §6 component tests 1 |
| board degenerates per breakpoint; mapping + demotion unit-asserted | D17, §5 step 1, §6 tests 1–2 |
| every interactive element focus-reachable, declared-order tab, role+label | D15 scope map, D16 inventory, §6 checklist |
| every verb routes through the action layer; no key handlers in views | D5/D15, rule "clicks prepare ctx or route actions" |
| drag affordances have action equivalents | no drag exists ([DEV] D8); move flow is the universal path |
| presentation helpers pure, reuse domain overdue/due-soon | §4 set-membership design; grep-clean |
| token lint + contrast green over new markup | §5 step 10; `Tone`→Semantic only |
| banner rides signal path, dispatches typed `StateCommand`s | D1/D4/D12, §6 test 4 |
| gates green; fence holds (§2.2/§2.3 diff) | §2 scope, §11 deviations list |

## 9. Risks & fallbacks

1. **Text-input starvation (the D18 trap)** — standard's Board
   printables would eat typing if the chain kept the view context.
   Mitigation: D5's insert-mode masking + §6 test 3. Residual:
   emacs `Ctrl+g` prefix-pending wrinkle (Escape is the documented
   cancel). Fallback if masking proves insufficient in review:
   extend the early-reject in `intercept` for `editing && no
   modifiers` — a recorded amendment, not drift.
2. **Focus-order traps** — conditional view mounting clears focus
   (1.18 clears focus on invisible items); each view's `init`
   focus-grab covers the switch; the root `init` fallback covers
   the pre-view window. Risk: `focus()` during `init` of a
   conditionally-created element racing the unmount of the old view
   — slint serialises instance creation on the event loop; if a
   flake appears, move the grab to a `changed current-view`
   callback in `app-window.slint` (one-line fallback, recorded).
   Second trap: Space/Return on a focused button natively
   "activating" it — our interception consumes them first (they are
   board verbs); this is by design (actions own keys), recorded in
   the a11y inventory notes so nobody "fixes" it later.
3. **Banner-signal traps** — (a) expecting a dedicated overdue
   signal: none exists or may be added (§8 of the reminders round);
   the banner is state + minute tick (D1/D12). (b) Making banner
   visibility mute-aware: contract violation (board truth). (c)
   Snooze link inertness for rule-less tasks: recorded honestly
   (D4/D12); the nag effect is plan-5's. (d) The banner flooding
   small windows: `BANNER_ROW_CAP` + count badge.
4. **Model-rebuild clobbering edit buffers** — refresh skips
   text-field pushes during active flows (D9 rule 4); test 3 covers
   (signal arrives mid-edit via forced `refresh()`).
5. **Enabled-bit flip churn** — chunk-2 snapshot/tests change
   meaning; the rewrite of `chunk3_staged_actions_are_disabled` is
   a planned contract change (D6), called out so review sees intent
   rather than a weakened test.
6. **Breakpoint float edges + null platform 0×0** — mapping consts
   tested at exact boundaries; ≤0 widths ignored (D17).
7. **Registry growth pushback** — D4's authority chain (Q23 > the
   dispatch summary line) is recorded; if review disagrees, the
   fallback is removing the two buttons (banner truth-only) — worse
   against Q23, so the default stands.
8. **Scope creep gravity** ("just a date picker…") — D13 records
   the deliberate cuts; the §2.4 escalation rule applies to any
   §2.3 temptation.

## 10. Verification of this round

- Every dispatch-prompt topic is settled: file layout + contracts
  (D20/D22), decoration slots (D21), presentation helpers +
  proptest catalogue (§4), Responsive + demotion (D17), board
  degeneration (D8), focus maps + a11y inventory (D15/D16),
  status bar (D11/D12), state subscription without mutation
  (D1/D2), test catalogue (§6).
- Every design-plan normative section cited in the origin is
  traceable to a decision; deviations are exclusively the §11 list,
  each with a reason and (where applicable) a backlog entry for the
  deferred substance.

## 11. Deviations from the design plan ([DEV], consolidated)

1. **[DEV] No pointer drag-and-drop** (design §6.4/§7.1 imply drag
   exists at regular+ with `task.move` as its equivalent): Slint
   1.18 has no widget-level DnD primitive (stress report verified
   nothing there); hand-rolling cross-column drag is disproportionate
   to this chunk and untestable by the keyboard journeys that are
   the acceptance bar. The move flow (D8) is the single, universal,
   keyboard-complete path at every breakpoint. Backlog entry:
   "Pointer drag-to-move on the board (equivalence: task.move)".
2. **[DEV] No `view-stack` L2 file** (design §4.4 names it): a
   generic stack cannot host L3 children without an L2→L3 reference
   (rule C violation); the substance (navigation + focus hand-off)
   is delivered by `view_state` + conditional mounting in the
   window shell. Recorded so the inventory diff shows intent.
3. **[DEV] Registry grows two reminder actions** vs the dispatch
   non-goal's "no new actions" reading — Q23's accepted "snooze/
   mute via actions" + §7.1's equivalence rule override (D4).
4. **[DEV] Due-soon horizon is a const, not config** — the domain
   comment anticipated config; Q22 fixes the v0.1.0 config surface
   (§4).
5. **[DEV] Scope cuts recorded in D13** (due-date editing,
   description editing, stack management UI, label chips) — none
   are design-plan violations (the design names the views, not
   their field editors); recorded + backlog entries for the two
   editing cuts.
6. **[DEV] `list-row` consumed by the banner** rather than deferred:
   the §4.4 inventory seeds it, and the banner rows are a real
   v0.1.0 consumer — landing it consumed avoids both dead code and
   a deferred-primitive hole.

Stage 2 appends the two backlog entries ([DEV] 1 and 5's editing
cuts) to
`.artifacts/taskboard-potential-improvements-backlog.md` per the
AGENTS §7 rule (this stage-1 commit stays plan-only).
