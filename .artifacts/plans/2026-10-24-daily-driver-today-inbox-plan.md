# [Plan] Daily-Driver Chunk 10 — Today View + Global Inbox + Quick Capture + Move Ring

Stage-1 plan for chunk 10 of the daily-driver implementation round
(dispatch §11): the pure `today_sections` core in `days.rs`, the
`ViewId::Today` desktop-default view with its re-commit verb, the
v2 app-global Inbox (a designated real board) with
`StateCommand::CaptureTask`, the capture overlay and the `capture`
CLI/REPL verb, the flat cross-board move ring, and the inbox chip.
Closes #346 + #347 (stage 3 writes both `Closes` lines on the PR).

Normative input, in MD-7 rank: implementation plan §2 DDF-15 +
DDF-11 (the consumers contract), §3 chunk 10 (files, behaviours,
non-goals), §4–§6; design doc
`2026-10-19-views-finding-design-plan.md` Parts A/B (§3.1–§3.7,
D-V1–D-V7 with D-V5 as the **v2 rewrite** + its ruling), §4's
decision record incl. the riders (D-V4 in-wave + confirmation-free;
D-V5 sub-levers: unbound creation + flat ring), §6 chunk mapping,
§7's reconciliation items 1/2/3/5/6/7/8/10; dispatch plan §1
(execution model, MD rules) + §11 (covers/acceptance/traps). No
ACCEPTED ruling is deviated from; every decision below is either a
plan-day resolution of a detail the designs left open or an
engineering consequence of D-V1–D-V7, with the two prose-vs-prose
tensions (D1: the "done or not" sentence; D3: boards have no
`order` field) settled inside the rulings' own precedence, MD-7
style.

Precondition (MD-9) verified 2026-10-24, all on `origin/main`:
chunk 1 (the four `Task` fields + clocks at `entities.rs:410–424`,
`TASK_FIELDS` at `entities.rs:252`, migration `0007`), chunk 2
(`days.rs::day_window`, pure + proptested), chunk 3
(`ANCHOR_CARD_TITLE` at `appdata.rs:36`, re-exported `lib.rs`).
Chunks 4–9 are **all merged** — including the two named colliders:
chunk 5 (decorations on `TaskCardModel`, the anchor filter
`presentation::is_anchor_task`) and chunk 9 (`Flow::Palette`,
`ContextId::Palette`, `search-palette.slint`, top-level `find`).
No collision remains; this chunk is the round's last implementation
chunk (11 = docs follows).

**Base moved mid-planning (recorded for stage 2/3).** The plan's
first pass read the seams at `6b18100` (the chunk-9 tip); a
concurrent agent's round (r2 action round-up: object context menus,
board/stack lifecycle flows, move-cancel/wrap-walk, registry rows)
landed on `origin/main` and its worktree-doctor sweep removed this
chunk's freshly-cut worktree (clean tree, zero unique commits —
correctly classified disposable). The branch was reset to the new
tip `aba316e` and the worktree recreated
(`../taskboard-WT-feat-ui-today-inbox`); the r2 round's touched
seams were re-read (Flow enum grew `CreateBoard`/`RenameStack`/
`RenameBoard`/`MoveStack`; `ActionId` grew `TaskReopen`,
`TaskMoveCancel`, `Stack*`/`Board*` lifecycle verbs,
`NavFirstItem`/`NavLastItem`; the move flow's candidate walk and
`commit_edit` arms were confirmed re-located). Stage 3 re-verifies
the base and does the rebase dance if `origin/main` moved again
(the concurrent agent's worktree was still live at plan time).

---

## 1. Verified baseline (symbols re-located at this plan's start; lines are plan-day references @ aba316e)

Domain (`crates/taskboard-domain`):

- `days.rs` — chunk 2's module: `day_window(now, NaiveTime,
  FixedOffset) -> (start, end)` :36 with the consumers-contract doc
  (bootstrap resolves the pair; consumers receive `(NaiveTime,
  FixedOffset)` explicitly; nothing reads figment mid-flight); the
  proptest house pattern (256 cases, `timestamp_strategy`) :133.
- `entities.rs` — `Task { duedate, scheduled, done: Option<
  DateTime<Utc>>, stack, order, … }` :396 with `is_live` :439 /
  `is_open` :446; `Board { id, remote, title, color, archived,
  deleted, clocks, remote_seen }` :351 (**no `order` field**);
  `Stack { board, title, order, … }` :373.
- `reminders.rs` — `overdue_tasks(tasks, now)` :643: live ∧ open ∧
  `duedate < now`, canonically `(order, id)`-sorted via private
  `filter_sorted` :676; `due_soon` :654 the sibling.
- `state.rs` — `AppState { boards, stacks, tasks, labels:
  BTreeMap<Id, _>, reminders, sync, sidecar }` :25; canonical
  comparators `live_tasks`/`tasks_in_stack` :407/:413.
- `messages.rs` — `StateCommand` :20 (`CreateTask {title, stack,
  order}` :22, `UpdateTask{id, changes}` :31, `MoveTask` :44,
  `CreateStack{board,title,order}` :59, `CreateBoard{title,color}`
  :125 — mints unbound, **no outbox op**); `TaskChanges` with the
  `duedate`/`scheduled` double options :287/:293.
- `command.rs` — `plan_command(app, outbox, command, ids, now) ->
  Result<CommandPlan, CommandError>` :316; `CommandPlan {actions,
  outcome}` :45; `CommandOutcome` (incl. `CreatedTask`, 
  `CreatedStack`, `CreatedBoard`) :118; `CommandError` :57;
  helpers `live_board_of` :202, `live_stack` :233, `enqueue` :1286,
  `all_task_clocks` :1387, `no_op` :1404; the `CreateTask`/
  `CreateStack`/`CreateBoard` handlers :324/:485/:703.
- `appdata.rs` — `ANCHOR_CARD_TITLE: &str = "[taskboard]
  app-data"` :36 (the presentation-side exact-title filter's
  constant home, D-V §7 item 5, resolved).
- `ids.rs` — the `local_id!` newtypes all derive `Ord`; BTreeMap
  iteration = canonical id order.

UI (`crates/taskboard-ui-slint`):

- `view_state.rs` — `ViewId { Board(default), StackDetail,
  TaskDetail, Settings }` :27 (`Settings` cycle-excluded);
  `step_view` order `[Board, StackDetail, TaskDetail]` :322;
  `enter` subject validation :338; **`Flow` grew the r2 lifecycle
  flows** :41 (`Create{stack}`, `CreateStack`, `EditTitle{task}`,
  `Move{task, target: StackId}`, `Palette{selected}`,
  `CreateBoard`, `RenameStack`, `RenameBoard`, `MoveStack{stack,
  slot}`); `ViewState::new()` starts `Board` :141; `append_order`/
  `append_stack_order` :110/:121; `select_board` :394 (sets board +
  first-stack selection + first-task focus); `normalize_board`
  (live = !deleted ∧ !archived) :374; `step_focus` domain selection
  (compact → stack; desktop → `reading_order`) :483; **the move
  flow**: `start_move` :949, `cycle_move_target` :965 (**walks
  `self.stack_ids(state)` — the selected board only; the
  generalization site**), `confirm_move` :985, `cancel_move` :1002;
  `commit_edit`'s flow arms :784+ (empty title commits nothing);
  `reconcile` :985-region (flows on vanished entities end; text
  buffers never touched — D9 rule 4); `open_palette`/`palette_jump`
  the chunk-9 modal precedent.
- `actions.rs` — `ContextId` :26 (Global, Board, StackDetail,
  TaskDetail, Settings, Menu, Palette, ViNormal, ViInsert);
  `ActionId` :156 incl. r2's additions; `REGISTRY` rows with
  contexts/category/enabled/menu_rank/description/menu_group;
  `CaptureQuick` :163 reserved-disabled ("capture.quick", GLOBAL,
  Category::Capture); `reserved_globals_are_present_and_disabled`
  + `only_reserved_actions_stay_disabled` pin the reserved set
  (CaptureQuick in it — the flip updates both, deliberately).
- `router.rs` — `UiEffect` :30 (incl. `OpenPalette`, `BeginCreate`,
  `BeginCreateStack`, `RequestMove`, board effects);
  `DispatchOutcome` :94; `DispatchCtx { focused_task,
  focused_stack, reminder_target, board_target, rule_target,
  rule_add_offset }` :112 (all `Option` one-shots set by the
  adapter's `route`); the exhaustive dispatch table :146
  (`CaptureQuick` still in the `Disabled` arm :231).
- `models.rs` — `push(window, state, vs, keymap, chain, now,
  models) -> IdTables` :336 (the one deterministic pass);
  `IdTables` :46 (board_ids, stack_ids, board_tasks, stack_tasks,
  banner, task_rules, palette); `push_palette` :210 (the anchor
  filter at the row source, `PALETTE_ROW_CAP`, indices-back);
  `task_card_model` :860 (`due_phrase`/`scheduled_phrase`/
  `effort_phrase` + `decorations` handle); `push_status_bar` :742
  (banner cap, `FlowHintModel { keys, visible }` :799 — the hint
  carrier); `diff_rows`/`PushModels` (r2 D9 stable-identity
  discipline — every new repeated surface keeps a persistent
  handle).
- `presentation.rs` — `due_sets`/`due_kind`/`due_phrase`/
  `scheduled_phrase` :50/:69/:117/:146; `is_anchor_task` :176;
  `banner_rows` (anchor-filtered) :315; `stacks_in_board` (order,
  id) :357; `live_boards` (!deleted ∧ !archived, BTreeMap order)
  :372; `live_board` :385.
- `adapter.rs` — `Adapter { keymap, shell, resolver, view_state,
  state, commands, requests, one-shot targets, tables, models, … }`
  :69; `route` builds `DispatchCtx` from the one-shots :244;
  `interpret` — the `(flow, effect)` table :442 with the
  leaving-commit rule; `refresh` :355 (`now = Utc::now()`, one
  `push`); `install(...)` :1276 the mount hand-over (constructs
  `ViewState::new()`, seeds widths); `Adapter::sync` pushes
  `palette_open` etc. :130.
- `lib.rs` — `UiSettings { scale, keymap, startup, kiosk, … }`
  :57 (**`kiosk: bool` already rides it — "derives from the run
  mode"**); `mount(...)` :161 → `install` → first `refresh`;
  `start_repaint_driver` :317 (100 ms cadence + **wall-minute
  boundary detection** — the rollover mechanism, reused verbatim).
- keymap `presets.rs` — `(ActionId, ContextId, &[strokes])` rows;
  **`(CaptureQuick, Global, ["Ctrl+K"])` already bound** :71 (the
  reserved action's pre-wired stroke); insert/Return discipline:
  ViInsert masks all but Escape; the palette binds its cursors in
  its own head context.
- slint — `View` enum (board, stack-detail, task-detail, settings)
  app-window.slint:45 + the conditional view mounts :172+;
  overlay pattern `if (root.palette-open): SearchPalette` :341;
  `views/` + `widgets/empty-state.slint` + `widgets/task-card.slint`;
  toolbar.slint (title + menu hint — the chip's home);
  status-bar.slint `FlowHintModel { keys, visible }` :41.
- tests — `journeys.rs` (`Harness::mount`, `press(ActionId)`,
  `type_text`, `assert_commands(&[StateCommand])`, window getters;
  the ten-step session :247 asserts `View::Board` **at launch**;
  the palette leg :2907 the template); `keymap_snapshots.rs` +
  `snapshots/keymap_snapshots__*.snap`; `golden_visuals.rs` (board
  matrix 2 scales × 3 breakpoints, fixed-height captures,
  `TASKBOARD_REGEN_GOLDENS=1` regen discipline); `support/`
  (driver, fake_engine, fixture, guard).

App (`crates/taskboard-app`):

- `cli.rs` — `CliCommand` :20 (top-level `Find` :89, `Timers` :103
  the recent siblings; no `Capture` yet); `TasksCmd::Add { title,
  stack: Option<String> }` :669, `Move { task, stack, order }`
  :686.
- `commands/tasks.rs` — `resolve_stack` (uuid | exact title,
  typed ambiguity) :64; `resolve_target` :103; `add_on` :200 (the
  two-lane `*_on` precedent + `MutationReceipt`).
- `commands/boards.rs` — `DEFAULT_BOARD_COLOR: &str = "0082c9"`
  :26 (app-side; the Inbox creation's default-color precedent).
- `commands/find.rs` — chunk 9's newest command module (the
  two-lane + typed-row shape `commands/capture.rs` mirrors).
- `commands/output.rs` — `MutationReceipt` :304 (id, title,
  stack_id, stack_title, order, done, labels — capture's receipt).
- `repl.rs` — `resolve_line` → clap reuse (a new clap verb parses
  for free); ceremony refusals are an explicit small set.
- `main.rs` — the kiosk/desktop mount hand-overs (:484+): 
  `resolve_ui_settings(&app.config.ui)` → `ui_settings.kiosk =
  true` on the kiosk path (the desktop path leaves it false).
- `config.rs` — `AppMode` (Daemon/Desktop/Kiosk); the r2 UI handover
  `resolve_ui_settings` (app/src/ui.rs) — the `day_start` pair's
  resolution site; `PreferencesSection.day_start_time` :338 (typed
  validation exists — chunk 2).

## 2. Shape of the chunk

Eight pieces, TDD-first, in commit-ladder order (each commit
standalone, rebase-merge hygiene):

1. **Domain core** — `days.rs::today_sections` + `TodaySections` +
   the proptest catalogue (pure, ids-only, one definition).
2. **Domain capture** — `StateCommand::CaptureTask` + the
   resolution helpers as pure `AppState` functions + the
   `plan_command` handler + the engine-snapshot corpus (this
   chunk's state-crate insta pass) + the default-board-color const
   promotion.
3. **Registry/router/view-state scaffolding** — `ContextId::Today`,
   `ViewId::Today` + cycle + default-view constructor, the three
   actions + `CaptureQuick` flip + context widenings, `UiEffect`
   growth + `DispatchCtx.day_window`, router arms.
4. **The move-ring generalization** — the cross-board candidate
   walk + the `board › stack` hint + the regenerated move-flow
   tests (the deliberate behavior change).
5. **The today view surface** — `DayCtx`, the window custody in
   the adapter, `push_today` + `IdTables.today_tasks` +
   `today-view.slint` + the today focus domain +
   selection-follows-focus + the kiosk filter + `task.due-today`
   wiring + the anchor filter.
6. **The capture overlay** — `Flow::Capture` + `BeginCapture`
   interpretation + `capture-overlay.slint` + the commit semantics.
7. **The inbox chip + `view.inbox`** — `inbox_count`, the toolbar
   badge, the jump action's quiet no-op.
8. **CLI/REPL + journeys + regen** — `capture` verb +
   `commands/capture.rs`, the `tasks add` Backlog default, 
   `tasks move --board`, the today + triage journey legs, the
   ten-step journey's default-view regen, the registry/keymap
   snapshot + golden-visual passes, the CHANGELOG line.

## 3. Decisions (every open question settled here)

### D1 — `today_sections`: exact section rules (and the "done or not" tension settled)

`days.rs` grows, per D-V1:

```rust
pub struct TodaySections {
    pub overdue: Vec<TaskId>,     // overdue_tasks reused; (order, id)
    pub scheduled: Vec<TaskId>,   // (scheduled, order, id)
    pub due_today: Vec<TaskId>,   // (order, id)
    pub done_today: Vec<TaskId>,  // (done, id) ascending
}

pub fn today_sections(
    tasks: &BTreeMap<TaskId, Task>,
    now: DateTime<Utc>,
    window: (DateTime<Utc>, DateTime<Utc>),
) -> TodaySections
```

Per-task classification, in first-match order — the predicates make
the sections disjoint by construction:

1. **Overdue** — `overdue_tasks(tasks, now)` reused verbatim
   (live ∧ open ∧ `duedate < now`), ids in its canonical
   `(order, id)` order.
2. **Scheduled** — `is_live() ∧ is_open() ∧ scheduled ∈ [start,
   end)`, ordered `(scheduled, order, id)`.
3. **Due-today** — `is_live() ∧ is_open() ∧ duedate ∈ [now, end)`,
   ordered `(order, id)`. Lower bound `now` (the ruling: a duedate
   between `start` and `now` is already Overdue — the illustration
   comment's `duedate ∈ [start, now)` at doc line 166 is a stale
   artifact; §3.1's item-3 bullet and DDF-15 both rule `[now, end)`
   and are normative).
4. **Done** — `is_live() ∧ done ∈ [start, end)`, ordered `(done,
   id)`.

A task matching Scheduled ∧ Due-today lands in Scheduled **only**
(first-match: the classification short-circuits per task — a task
placed in an earlier section is not re-tested for a later one).
Deleted tasks appear nowhere; done tasks whose `done` falls outside
the window appear nowhere (open-only sections exclude them by
`is_open`; Done excludes them by the window).

**The prose tension, recorded**: §3.1 item 2's "a task scheduled
earlier today but already past stays in the section — … done or
not" reads as if completed scheduled tasks stay in Scheduled; the
same ruling's first-match bullet ("a completed scheduled task moves
to Done") and its named proptest ("done-placement: completing a
scheduled task moves it to `done_today`, out of `scheduled`") say
the opposite. The bullet + proptest govern: **Scheduled and
Due-today require `is_open()`**; the "done or not" sentence is read
as time-passage (the slot's time having passed does not unplace the
task — an open 09:00 task stays in Scheduled at 15:00). This is
intra-ruling precedence (the specific contract over the loose
phrase), not a deviation: DDF-15 restates the four sections with
Done as its own section, and the partition + done-move proptests
are part of the ACCEPTED recommendation.

The doc illustration's per-section comment ordering (`due_today`
"see below") and the struct above are the contract; callers read
entities from the map (ids only, serde-free — the `overdue_tasks`
shape). No importance/urgency/effort input anywhere (fields stay
dumb); no board parameter (the function is board-agnostic by
construction — D-V3's cross-board desktop is a caller property).

**Proptest catalogue** (in `days.rs`'s test module, the chunk-2
house pattern; 256 cases; strategies derive from
`test_support::task_strategy` with constrained timestamps):

- P1 **partition** — every live task appears in ≤1 section
  (deleted tasks in none).
- P2 **first-match** — a task with `scheduled ∈ window` ∧ `duedate
  ∈ [now, end)` is in `scheduled` only.
- P3 **done-move** — completing a task scheduled in-window moves it
  from `scheduled` to `done_today` (the paired-state construction).
- P4 **window bounds** — `duedate` one second before `now` →
  overdue; at `now` → due-today (not overdue — `d < now` is the
  domain predicate); at `end` → neither (half-open); `done` one
  second before `start` → nowhere.
- P5 **orderings** — scheduled sorted by `(scheduled, order, id)`;
  done by `(done, id)`; overdue/due-today by `(order, id)`
  (asserted over generated multi-task fixtures).
- P6 **totality/determinism** — equal inputs → equal outputs
  (BTreeMap canonicality).

### D2 — `ViewId::Today`, the cycle, and the desktop-only default

- `ViewId::Today` joins the enum (declaration before `Board`; the
  derived `Default` stays on `Board`); the cycle array becomes
  `[Today, Board, StackDetail, TaskDetail]` (`step_view` :322);
  `enter(Today)` sets the view and returns `true` unconditionally
  (subjectless — no validation, `ViewId::Board`'s arm inverted);
  `context()` maps it to `ContextId::Today` (name `"today"`,
  declared between `Global` and `Board` in the enum — the cycle's
  order; snapshot regen covers the position).
- `nav.focus-board` remains the one-keystroke escape (unchanged).
- **The default is desktop-only and reaches `ViewState` through
  `UiSettings.kiosk`**: `ViewState::new()` keeps its const
  `Board` default (every existing caller unchanged); a new `const
  fn new_starting(view: ViewId) -> Self` is the constructor with a
  start view; `adapter::install` passes
  `if settings.kiosk { ViewId::Board } else { ViewId::Today }`.
  The `kiosk` bit already rides `UiSettings` ("derives from the run
  mode — never a config key"), so no new `UiSettings` field and no
  `AppMode` dependency enters the ui crate — the ruled scoping
  ("the daemon has no UI and the kiosk's surface is the kiosk
  wave's") is carried by the one bit that already distinguishes the
  two UI-bearing modes. `settings_return` interplay: Settings
  opened from Today restores Today (the existing snapshot
  mechanism, no change).

### D3 — the actions, contexts, and their routing

- **`ActionId::ViewToday`** — `"view.today"`, label "Go to today",
  Category::Navigation, contexts GLOBAL, enabled, `menu_rank` 90,
  `MenuGroup::ViewNav`, description per D-V2. Router → `UiEffect::
  FocusToday`; the adapter's interpretation → `view_state.
  focus_today()` (sets view = Today, `RefreshNow` — the
  `focus_board` sibling; performs no engine command).
- **`ActionId::ViewInbox`** — `"view.inbox"`, label "Go to inbox",
  Navigation, GLOBAL, enabled, rank 100, ViewNav. Router →
  `UiEffect::OpenInbox`; interpretation resolves read-side (D7
  below) — **quietly no-ops with no Inbox board** (an empty inbox
  is triage-done; `Vec::new()`, no repaint, no board creation —
  read-write separation, the D-V7 ruling).
- **`ActionId::TaskDueToday`** — `"task.due-today"`, label "Due
  today (re-commit)", Category::Task, contexts `[Today]`, enabled,
  Task group. Router arm:

  ```rust
  ActionId::TaskDueToday => match (ctx.focused_task, ctx.day_window) {
      (Some(id), Some((_, end))) => DispatchOutcome::Command(
          StateCommand::UpdateTask { id, changes: TaskChanges {
              duedate: Some(Some(end)), ..default } }),
      _ => DispatchOutcome::NoTarget,
  }
  ```

  Confirmation-free (the D-V4 rider); a fresh `Some(Some(_))`
  write that stamps the `duedate` clock (reconciliation item 8);
  `NoTarget` without focus **or window** (the design's exact
  wording). No `snooze_target` logic touched (reconciliation item
  7 — the banner is reminder-side, this is planning-side).
- **`CaptureQuick` flips enabled** — description loses the
  "Not yet available" clause (states the overlay: "Captures a
  thought into the global inbox from anywhere; Enter commits and
  continues, Escape closes."); the router arm moves from the
  `Disabled` set to `UiEffect::BeginCapture`. The two registry
  tests drop `CaptureQuick` from the reserved set — a deliberate,
  design-ordered test change (the flip is D-V6's own instruction),
  noted for stage 3 so it is not read as a weakening.
- **Context widenings** — `task.complete`, `task.reopen` (r2's
  verb), `task.edit`, `task.move`, `task.delete` add `Today` to
  their contexts (D-V2 names the task-verb rows; `task.reopen`
  follows its complete-sibling — same row family); `nav.next-item`/
  `nav.prev-item` (the `VIEW_NAV` table) and `nav.first-item`/
  `nav.last-item` (r2's edges) add `Today` (the today journey walks
  rows with `nav.next-item`, D-V2). `task.create`/`stack.create`/
  `stack.*`/`board.*` stay off Today (no create row on the today
  view — capture is the create path there).

### D4 — the day-window custody: `DayCtx`, the adapter's pair, `push`, and `DispatchCtx`

- The **adapter holds the pair** (`day_start:
  (chrono::NaiveTime, chrono::FixedOffset)` — an `Adapter` field
  set by `install` from a new `UiSettings.day_start` field). The
  app resolves it once at hand-over: `resolve_ui_settings`
  (app/src/ui.rs) parses `config.preferences.day_start_time`
  (already typed-validated by chunk 2) and resolves the device
  offset via `chrono::Local` at derivation time — the consumers
  contract verbatim (nothing reads figment mid-flight; nothing
  mutates `AppState` for it). `UiSettings::default` uses
  midnight + UTC.
- A new value type in `view_state.rs` (the pass's time bundle):

  ```rust
  pub struct DayCtx {
      pub now: DateTime<Utc>,
      pub window: (DateTime<Utc>, DateTime<Utc>),
  }
  ```

- **`models::push` takes `DayCtx`** (replacing the bare `now`
  param) and a `kiosk: bool` flag: `push(window, state, vs, keymap,
  chain, day: DayCtx, kiosk, models) -> IdTables`. Time is data —
  the rollover rides the existing minute-boundary repaint because
  `refresh` recomputes `DayCtx` (`day_window(now, …)` from the held
  pair) on every pass; **no new timer** (D-V4). Direct `push`
  callers (adapter, models/lib tests, goldens, journeys via
  `handle.refresh`) construct the bundle.
- **`DispatchCtx` grows `day_window: Option<(DateTime<Utc>,
  DateTime<Utc>)>`**; `shell.dispatch_ctx()` leaves it `None`;
  the adapter's `route` sets it per dispatch from the held pair
  (`day_window(now, …)` with a fresh `now`), exactly the
  one-shot-target pattern's placement (state-derived, not
  consumed — the ruling's "the adapter sets it per dispatch,
  exactly as it sets `reminder_target` today" in mechanism, minus
  the consumption). `Copy` holds (two `DateTime<Utc>`).

### D5 — `CaptureTask`: the handler, the resolution helpers, the receipts

- `messages.rs`:

  ```rust
  /// Quick capture (#347, D-V5 v2): `board: None` lands in the
  /// app-global Inbox (the designated board, created unbound on
  /// first capture); `Some(board)` lands in that board's Backlog
  /// stack (created on demand). Never errors on the happy path;
  /// title validation stays at the edges.
  CaptureTask { title: String, board: Option<BoardId> },
  ```

- **Resolution helpers as pure functions over `AppState`** (public
  in `state.rs` — the UI's read side reuses them; one rule, the
  `due_sets` layering instinct):
  - `AppState::inbox_board(&self) -> Option<BoardId>` — the
    canonical-first (**smallest `BoardId`**, BTreeMap order) live
    board titled exactly `Inbox`. "Live" = `!deleted ∧ !archived`
    (`live_boards`' definition — the one canonical live-board set:
    board bar, walk, ring, find all use it; an archived
    `Inbox`-titled board is not adopted). **Boards carry no
    `order` field**, so the design's "canonical `(order, id)`
    first" degenerates to id order for the board dimension; the
    `(order, id)` pair applies to the stack dimension, where
    `order` exists. Recorded as the plan-day reading of D-V5's
    phrase (a deviation from data shape, not from the ruling).
  - `AppState::canonical_first_stack(&self, board) ->
    Option<StackId>` — smallest `(order, id)` among the board's
    `!deleted` stacks.
  - `AppState::backlog_stack(&self, board) -> Option<StackId>` —
    canonical-first among the board's `!deleted` stacks titled
    exactly `Backlog`.
- **The `plan_command` handler** (compose-in-one-batch —
  `plan_command` is pure over `AppState`, so the creates ride the
  same `CommandPlan`):
  - `board: None`: `inbox_board()` → `Some(b)` uses `b`; `None`
    **creates**: `ids.new_board_id()` + `ids.new_stack_id()`, the
    batch gains `UpsertBoard` (title `Inbox`, the default color —
    see below, **`remote: None` = unbound by construction**;
    `CreateBoard`'s handler semantics verbatim, no outbox op for
    the board) + `UpsertStack` (title `Inbox`, order 0) +
    `enqueue(LocalOp::CreateStack(sid))` (the op strands honestly
    until the user binds — the ruled unbound semantics; **never
    auto-bound**).
  - `board: Some(b)`: `live_board_of(b)` (typed
    `UnknownBoard`/`BoardDeleted` — not the happy path);
    `backlog_stack(b)` → `Some(s)` uses `s`; `None` creates the
    stack (title `Backlog`, order = the `append_stack_order`
    arithmetic over the board's live stacks, replicated
    engine-side) + `enqueue(LocalOp::CreateStack(sid))`.
  - Then the task: order = max canonical order + 1 over
    `tasks_in_stack(target)` (0 when empty — the `append_order`
    arithmetic, engine-side), `UpsertTask` (the `CreateTask`
    handler's fresh-task shape) + `enqueue(LocalOp::CreateTask(
    tid))`.
  - **Receipt**: `CommandOutcome::CreatedTask(task_id)` — reused,
    not a new variant (capture's essence is a created task; the
    landing context is read back by the CLI's receipt projection
    and by tests against `AppState`, which is where the
    board/stack creation is asserted anyway — the design's engine
    tests are snapshot tests of state, not of receipts).
  - Action order: board upsert, stack upsert, task upsert, then
    the enqueues (the `CreateTask`/`CreateStack` handlers' shape
    scaled up; persistence applies them in order).
  - Empty title: accepted (the engine does not validate titles
    today — `CreateTask` accepts any string; "title validation
    stays at the edges", v1's rule carried). A whitespace-only
    title is likewise accepted verbatim (the CLI/UI trim at their
    edges — the UI's `commit` trims, the CLI takes the arg as
    given; same as `tasks add` today).
- **Default board color**: the Inbox creation needs a color and
  `CaptureTask` carries none. `DEFAULT_BOARD_COLOR` ("0082c9",
  Deck's UI blue) is **promoted to the domain** (a `pub const` in
  `entities.rs`); `commands/boards.rs` consumes it (its local
  const removed) — one home, no second default. The color value is
  presentation-lenient data (the `Color` newtype), never validated
  domain-side.
- **Engine snapshots** (this chunk's state-crate insta pass):
  first capture creates board+stack+task unbound (the board row
  has `remote: None`, one `CreateStack` op + one `CreateTask` op
  queued, **no board op**); second capture appends to the same
  stack; `Some(board)` with no Backlog creates it appended;
  `Some(board)` with two Backlog stacks lands canonical-first; an
  existing `Inbox`-titled live board is adopted (not duplicated);
  an archived `Inbox`-titled board is not adopted (a fresh one is
  created); `Some(unknown)` is the typed rejection. Sync is
  native-lane throughout (the created task rides `CreateTask` —
  nothing inbox-shaped appears in the app-data document; the
  outbox ops are the ordinary ones).

### D6 — the move-ring generalization (the deliberate behavior change)

- `cycle_move_target`'s candidate list widens from
  `self.stack_ids(state)` (the selected board's stacks) to the
  **flat cross-board ring**: `presentation::live_boards(state)`
  (canonical BTreeMap order) flat-mapped with each board's
  `stacks_in_board` order `(order, id)` — all live boards' stacks,
  wrapping (the `wrapped_index` primitive unchanged). `start_move`
  keeps its initial target (the task's own stack); `confirm_move`
  keeps its `MoveTask` append (cross-board targets are already
  legal engine-side — `live_stack` is board-agnostic; the native
  lane carries the move; the app-field gap is exactly #350's
  tracked scope, and fresh captures are native-only so triage of
  just-captured items never touches it).
- **No Inbox special-casing** (the trap list: the ring is uniform;
  the Inbox board's stacks appear like any board's).
- **The hint**: `FlowHintModel` grows `text: string` — while a
  `Flow::Move` holds, the label reads `"<board> › <stack>"` (the
  target's board title, ` › `, the stack title — D-V5's literal
  vocabulary, always both parts; uniform, no conditional
  abbreviation). A new pure helper `presentation::move_hint(state,
  target: StackId) -> String` (empty on a dangling stack — data
  degradation, not an error) feeds `push_status_bar`.
- `BoardModel.move_target` keeps its meaning (the index into the
  **focused board's** columns; a cross-board target resolves to
  `-1` — no column highlighted, the hint carries the position).
- **The behavior change, owned**: ordinary same-board moves' ring
  now continues across the board edge. The affected view-state
  move-flow tests and any journey step pinning the old ring
  regenerate **in this chunk** (reconciliation item 10: deliberate,
  not drift); the CHANGELOG line states it explicitly. The r2
  round's `TaskMoveCancel`/wrap-walk semantics ride unchanged (the
  cancel arm and the wrap primitive are orthogonal to the
  candidate set).

### D7 — the today view surface

- **`today-view.slint`** (L2, `views/` sibling): four section
  groups in display order (Overdue, Scheduled, Due today, Done) —
  each a header (label + count; Done's count is the P8
  accumulation caption) and a list of rows; **empty sections
  collapse**; an all-empty day renders the existing `EmptyState`
  widget with a capture hint line. Rows are the `task-card`
  primitive (R1) inside a new `TodayRowModel { card:
  TaskCardModel, context: SharedString }` — the context line is
  the stack title, or `"<board> › <stack>"` when
  `live_boards(state).len() > 1` (D-V3: stack title always, board
  badge when >1 live board — a deterministic state-derived
  presentation helper decides, `today_row_context`). Compact
  breakpoint: single column (responsive layout in the composition;
  no board-badge removal on desktop-compact — that is kiosk-only).
  Section rows ride `PushModels` persistent handles (the r2 D9
  row-diff discipline — a rollover refresh must not re-instantiate
  cards).
- **`push_today`** (models.rs, called from `push` when the view is
  Today): computes `today_sections(&state.tasks, day.now,
  day.window)`, applies the **kiosk filter** (below) and the
  **anchor filter** (below), builds the section models +
  `TodayModel { sections, done_count, empty }`, pushes
  `window.set_today_model`, and fills `IdTables.today_tasks` —
  **the four filtered sections concatenated in display order**
  (D-V3: the focus walk's domain).
- **The kiosk filter** (the one Q8 departure, reconciliation item
  6): with `kiosk: true` the pass drops rows whose task's stack
  board ≠ `vs.board()` (the focused board's subset) and suppresses
  the board part of row contexts (no badges) — one presentation
  filter parameter on the pass, **no kiosk code paths** (no mode
  branches outside the filter); done-count and the overdue section
  still render (glanceable status). Desktop: cross-board by
  construction (the domain function takes the whole map).
- **The anchor filter** (reconciliation item 5's consumption):
  `is_anchor_task` filtered at the row source inside `push_today`
  (the `push_board`/`push_palette` precedent) — rows and
  `today_tasks` and the done-count all follow (the filter sits
  before anything derived). The domain function stays
  convention-blind.
- **The focus domain + selection-follows-focus**: `step_focus`
  grows the Today branch — its domain is the concatenated
  `today_sections` display order, derived in `view_state` from
  `(state, DayCtx)` (the adapter passes a fresh `DayCtx` into
  `advance_focus`/`retreat_focus` — time is data; other views
  ignore it). After each focus step (and `activate_card`) on the
  Today view, a new `follow_board` seam retargets
  `ViewState.board` to the row's board and `selected_stack` to the
  row's stack, **keeping `focused_task`** — a lighter touch than
  `select_board` (which resets focus to the board's first task and
  would break the walk); this is D-V3's "selection-follows-focus
  retargets `ViewState.board` (Q8's `select_board` mechanics)"
  read as the retarget-without-focus-reset, which is what keeps
  `task.move`'s ring and detail navigation board-local (the
  design's stated purpose). No-op when the board already matches.
- **`nav.focus-board`** escapes to the kanban view (unchanged);
  the D-V2 journey shape holds.

### D8 — the capture overlay (`Flow::Capture` + `capture-overlay.slint`)

- `Flow::Capture` (subject-free — `reconcile` leaves it and its
  buffer alone, the `Palette` precedent). **`begin_capture()`**
  (view_state): ends any in-progress flow **without committing**
  (its buffer is discarded — the overlay is the fire-drill escape;
  the design's "beginning capture ends any in-progress flow, the
  existing XOR rule" — the XOR rule replaces the flow, it does not
  commit it), then `flow = Capture`, buffer cleared,
  `[SetMode(Insert), RefreshNow]` — "exactly `begin_create`'s
  opening". The interpretation arm is `(_, UiEffect::BeginCapture)`
  (any flow — the GLOBAL action, P1's "from any context").
- **Commit semantics** (the one deliberate difference from the
  create row): Return on a non-empty (trimmed) buffer dispatches
  `StateCommand::CaptureTask { title, board: None }` — **always
  the global Inbox, never the focused board** (the session
  correction of v1) — clears the buffer, and **keeps the flow +
  Insert mode** (commits-and-continues, the rapid-fire braindump);
  Return on an empty buffer **closes** (flow ends, `SetMode(
  Normal)`); Esc (`ModeNormal` in the flow) closes discard-and-close
  (the `discard_edit` shape). A dedicated `commit_capture()` — not
  a `commit_edit` arm (commit_edit's contract is
  "the flow always ends"; capture's non-empty commit deliberately
  does not).
- **`capture-overlay.slint`** (L2, ActionMenu's sibling): one text
  field + a hint line — nothing else (no navigation, no fields).
  Conditional mount `if (root.capture-open)` in app-window.slint
  with a modal scrim (the palette's mount pattern verbatim);
  `capture-open` is a window property pushed from `Adapter::sync`
  (`matches!(flow, Flow::Capture)` — no new shell seam, no new
  `ContextId`: typing rides `ViInsert`, exactly the create row's
  discipline). The field's `accepted` callback routes the commit;
  Escape rides the action layer (`ModeNormal`).
- **No at-create scheduling** (D-V6's ruling of doc 2 §7's
  question): the overlay takes a title and nothing else; no flags,
  no overlay fields, no `--scheduled` anywhere near capture.
- **Kiosk**: no capture surface — the composition is not mounted
  in kiosk chrome... concretely: the overlay is input chrome; the
  kiosk never opens it because no kiosk surface advertises or
  binds it (the action stays enabled for an operator keyboard,
  Q8's stance; nothing kiosk-specific lands — if an operator
  triggers it, it functions; the design's "no chrome advertises
  it" is about default surfaces, and the kiosk renders none).

### D9 — `view.inbox` and the inbox chip

- **`view.inbox`** interpretation (the adapter, read-side only):
  `state.inbox_board()` — `None` → quietly no-op; `Some(b)` →
  `select_board(b)` + `subject_stack = canonical_first_stack(b)` +
  `view = StackDetail` (the resolution rule reused read-side,
  D-V7). Never creates (read-write separation).
- **`inbox_count`** (presentation.rs, pure): the count of live,
  open, anchor-filtered tasks whose stack belongs to the global
  Inbox board (`inbox_board()`; zero when none) — one inbox, one
  number, the `due_sets` template.
- **The toolbar chip**: `Toolbar` grows `in property <int>
  inbox-count` + a `inbox-clicked()` callback (routed to
  `ActionId::ViewInbox` — every click has an action equivalent);
  rendered as a `Badge`; **desktop only** — hidden at the compact
  breakpoint (the chrome-dropping path) and not pushed on kiosk
  (the adapter passes the count only when `!kiosk`; the property
  carries 0 otherwise). The chip renders at zero too (the triage
  journey asserts it reads zero after the move — a structural
  read, not text).
- The `toolbar_title` on the Today view: the toolbar shows the
  selected board's title today; on Today it renders the view's own
  caption ("Today") — `push_chrome`'s title selection grows the
  Today arm (a fixed label, presentation data; copy is polish).

### D10 — the CLI/REPL surface

- **`taskboard capture <TITLE> [--board <BOARD>]`** — a top-level
  `CliCommand::Capture { title: String, board: Option<String> }`
  (the `Find`/`Timers` sibling shape). No flag = the global Inbox
  (`CaptureTask { board: None }`); `--board` = the board-directed
  variant (`Some`). The `--board` selector: exact uuid | exact
  title among live boards, typed `AmbiguousBoard`/
  `UnknownBoard`-family errors at the app layer (the ADR 0009
  cascade's shape; `tasks move`'s selector below shares the
  resolver — one helper in `commands/mod.rs`).
- **`commands/capture.rs`** (new, the `find.rs`/`timers.rs`
  two-lane shape): `capture(config, store, title, board)` boots a
  core; `capture_on(app, …)` resolves the board selector, executes
  `CaptureTask`, and projects the receipt — **`MutationReceipt`
  reused** (the `add_on` shape: the engine's `CreatedTask` reply
  names the id; the receipt reads the landing stack/board context
  back from state; never re-derive by scanning titles).
- **The `tasks add` Backlog default**: with `--stack` given, the
  path is unchanged (`resolve_stack`). With `--stack` omitted the
  verb becomes board-directed: exactly one live board ⇒
  `CaptureTask { title, board: Some(board) }` (its Backlog,
  created on demand — the deliberate default-landing change; a
  single-board setup with a differently-titled only stack now gets
  a `Backlog` stack instead of appending to it — CHANGELOG); zero
  live boards ⇒ the existing typed board-less refusal; **multiple
  live boards ⇒ typed `AmbiguousBoard`** naming the candidates
  (a strictly better error than today's all-stacks
  `AmbiguousStack`; the user names a stack or uses `capture
  --board`). No new flag on `tasks add` (flag parity stays the
  open backlog item, doc 4 §8).
- **`tasks move --board <BOARD>`** (D-V5's CLI-triage detail, the
  ADR 0009 home): an optional board pre-filter on the `--stack`
  selector — when stack titles collide across boards the filter
  scopes the candidate set (uuid | exact title within the named
  board); ambiguity without the filter errors as today. The
  `--board` selector is the shared resolver from `capture`.
- **REPL**: `capture <TITLE> [--board b]` parses through
  `resolve_line` (clap reuse — free); no ceremony interception;
  `completion.rs` grows the verb (the existing completion
  machinery's family shape).
- No `tasks today` verb, no PDF (backlog — doc 4 §8).

### D11 — keymap preset rows (existence + resolvability)

- `capture.quick` is **already bound** `(Global, ["Ctrl+K"])` in
  the standard preset (:71); the emacs/vi preset tables gain the
  same row if absent (the reserved action's pre-wired stroke
  decided it — no new invention).
- `view.today`: bound in **all three** presets at `(Global,
  ["Ctrl+T"])` (verified free at plan time; P10's "one action from
  anywhere" is the one-stroke promise this carries).
- `view.inbox` and `task.due-today`: **no default strokes** —
  menu-reachable in their contexts (ViewNav / the Today task
  group) plus the chip click for the inbox; existence +
  resolvability is the pinned requirement (the StackCreate
  precedent: the preset table is the tunable).
- Registry/keymap snapshot regen is this chunk's deliberate insta
  pass (`keymap_snapshots__registry.snap` +
  `resolved_preset_keymaps.snap`): the three new rows, the
  context widenings, and `CaptureQuick`'s enabled bit + new
  description all land there.

### D12 — the journeys (two new legs + the deliberate regens)

- **The today leg** (`the_today_view_walks_completes_and_recommits`):
  a fixture variant seeding two live boards with (a) a task
  scheduled inside a fixed window on board A, (b) an overdue task
  on board B, (c) a task done inside the window; mount (desktop ⇒
  starts at Today — asserted); `nav.next-item` walks the
  `today_tasks` display order with the emphasis following; the
  walk retargets `vs.board` to each row's board
  (selection-follows-focus, asserted structurally via the board
  bar's selected index); `task.complete` on the scheduled row
  (assert `SetTaskDone`); focus the overdue row, `task.due-today`
  via the menu path (assert the exact `UpdateTask` command with
  `duedate = Some(Some(window_end))` — the harness drives with a
  known `DayCtx`); `nav.focus-board` escapes. Rollover is **not**
  in the journey (it is the models-test's stepped-`now` push
  proof — no sleeps anywhere).
- **The triage leg** (`the_inbox_triage_moves_across_the_board_edge`):
  a fixture variant seeding the Inbox board (title `Inbox`, one
  stack, one open task) + a second board with stacks; `view.inbox`
  jumps (assert view `StackDetail` + subject = the inbox stack);
  `nav.next-item` focuses the row; `task.move` starts the flow;
  `nav.next-item` cycles the ring **across the board edge**
  (assert the target lands on the other board's stack — the
  ring's new behavior, structural on the view state via the
  hint/flow); Activate confirms (assert `MoveTask`); the harness
  applies the moved state through the fake-engine seam; the chip
  reads **zero** (`window.get_toolbar_inbox_count() == 0` — the
  structural assertion the acceptance names).
- **The deliberate regens**: the ten-step session's launch step
  flips to `View::Today` (desktop default) and gains a
  `nav.focus-board` step before the board assertions (its move
  steps are single-board — the ring assertions survive unchanged);
  every mount-time `View::Board` assertion in the component/journey
  suites that runs the desktop profile flips or escapes the same
  way (enumerated at implementation; a kiosk-profile mount test,
  if none exists, is added to pin Board-default on kiosk — the
  ruling's scope). The palette journey is unaffected (it opens
  from `Flow::None` on any view).
- The guard (`support/guard.rs`) is untouched — no new `Verb`
  (capture's commit is the create shape, `N + x + Return`; the
  guard's budgets are the r2 round's concern).

### D13 — golden-visual scope

- **New**: a today-view golden matrix mirroring the board's —
  `today-<scale>-<breakpoint>.png` for scale ∈ {0.5, 3.0} ×
  breakpoint ∈ {600, 1000, 1500} (six PNGs). Determinism: the
  fixture seeds **phrase-stable rows only** relative to a fixed
  `DayCtx` passed through the harness (`push` takes `DayCtx` as
  data — the goldens' `now` is frozen by construction, which the
  board matrix could not do for the banner); sections with
  wall-relative phrases are avoided unless frozen-`now` phrases
  are confirmed stable in review. The `TASKBOARD_REGEN_GOLDENS=1`
  discipline applies; the PR's PNG diff is the review artefact.
- **Regenerated**: the six `board-*.png` goldens — the toolbar
  grows the inbox chip (desktop surfaces render it), so the board
  captures change. If the chip's zero-state renders visually
  null after review, stage 2 may hide-at-zero instead — a
  presentation polish call recorded either way in the PR body;
  the journey's chip-zero assertion is structural and holds under
  both.

### D14 — bookkeeping

- CHANGELOG `Unreleased`: one line for the today view + re-commit
  verb; one for capture + the global Inbox/Backlog convention
  (**naming the two deliberate behavior changes**: ordinary
  moves' ring crosses board edges; `tasks add` without `--stack`
  lands in Backlog; plus the desktop default view). No tracker
  edits from stage 2 (MD-4); stage 3 writes `Closes #346` +
  `Closes #347` and the per-issue progress annotations.
- Every new `.rs` file (`commands/capture.rs`) carries the AGPL
  header; domain files MIT OR Apache-2.0; `#![forbid(unsafe_code)]`
  holds everywhere (the ui crate excepted as ever).

## 4. Implementation inventory (file-by-file)

Domain (`crates/taskboard-domain`):

- `src/days.rs` — `TodaySections` + `today_sections` + the P1–P6
  proptest catalogue (D1).
- `src/messages.rs` — the `CaptureTask` variant (D5).
- `src/command.rs` — the `CaptureTask` handler (resolve-or-create
  compose, the enqueue set, `CreatedTask` receipt) + the
  engine-side append-order arithmetic (D5).
- `src/state.rs` — `inbox_board` / `canonical_first_stack` /
  `backlog_stack` (pure resolution, public for the UI read side).
- `src/entities.rs` — `DEFAULT_BOARD_COLOR` promoted from the app
  (D5).
- `tests/`/state crate (`crates/taskboard-state`) — the
  `CaptureTask` engine-snapshot corpus (this chunk's state insta
  pass; D5's scenario list).

UI (`crates/taskboard-ui-slint`):

- `src/view_state.rs` — `ViewId::Today` + cycle + `new_starting`;
  `Flow::Capture` + `begin_capture`/`commit_capture`/
  `close_capture`; `focus_today`; the Today focus domain +
  `follow_board`; `DayCtx`; the ring generalization in
  `cycle_move_target`; `open_inbox` (D2/D4/D6/D7/D8).
- `src/actions.rs` — `ContextId::Today`; `ViewToday`/`ViewInbox`/
  `TaskDueToday`; the `CaptureQuick` flip; the context widenings;
  the reserved-set test updates (D3).
- `src/router.rs` — `UiEffect::FocusToday`/`BeginCapture`/
  `OpenInbox`; `DispatchCtx.day_window`; the `TaskDueToday` arm;
  the `CaptureQuick` arm (D3/D4).
- `src/models.rs` — `DayCtx` consumption in `push` (+ `kiosk`);
  `IdTables.today_tasks`; `push_today` + the section/row models +
  the persistent handles; the chip push; `FlowHintModel.text`
  (D4/D6/D7/D9).
- `src/presentation.rs` — `today_row_context` (+ the badge rule),
  `inbox_count`, `move_hint` (D6/D7/D9).
- `src/adapter.rs` — the `day_start` pair + `kiosk` custody;
  `DayCtx` in `refresh`/focus paths; `ctx.day_window` in `route`;
  the interpretation arms (`FocusToday`, `OpenInbox`,
  `BeginCapture`, the capture commit/close seams, the Today
  focus-follow); `capture-open` in `sync` (D4/D7/D8).
- `src/lib.rs` — `UiSettings.day_start`; `install` wiring
  (D2/D4).
- `src/keymap/presets.rs` — the D11 rows.
- `ui/app-window.slint` — `View.today`; the capture overlay mount
  (+ scrim); the today view mount.
- `ui/views/today-view.slint` (new) + `ui/compositions/
  capture-overlay.slint` (new); `ui/compositions/toolbar.slint`
  (the chip); `ui/compositions/status-bar.slint` (the hint text
  field).
- `tests/journeys.rs` — the two legs + the ten-step regen (D12);
  `tests/golden_visuals.rs` + `tests/goldens/` (D13);
  `tests/keymap_snapshots.rs` snapshots (D11); `tests/support/
  fixture.rs` (the today/triage fixture variants).

App (`crates/taskboard-app`):

- `src/cli.rs` — `CliCommand::Capture`; the `tasks add` default's
  help text; `tasks move --board` (D10).
- `src/commands/capture.rs` (new) — the two-lane handler +
  `MutationReceipt` (D10).
- `src/commands/tasks.rs` — the Backlog default in `add_on`;
  `move`'s board pre-filter (D10).
- `src/commands/boards.rs` — consume the domain color const.
- `src/commands/mod.rs` — the shared live-board selector helper
  (D10).
- `src/ui.rs` — `resolve_ui_settings` resolves the `day_start`
  pair (D4).
- `src/repl.rs` + `src/completion.rs` — the verb lands for free +
  completion.

Root: `CHANGELOG.md` (D14).

## 5. Test matrix (TDD order — each test written first, watched fail, then the code)

1. `days.rs` proptests P1–P6 + the D1 boundary units (overdue-at
   `now−1s`, due-at-`now`, neither-at-`end`, done-window edges).
2. `state.rs` resolution units (adoption, canonical-first over
   duplicate titles, archived non-adoption, Backlog picks).
3. `command.rs` handler units (the D5 batch shapes — actions in
   order, the unbound board row, the enqueued ops) + the state
   snapshot corpus (the insta pass).
4. Router units: `TaskDueToday` → the exact `UpdateTask` with the
   ctx window end; `NoTarget` (no focus / no window);
   `CaptureQuick` → `BeginCapture`; `ViewToday`/`ViewInbox`
   effects.
5. `actions.rs` registry units: the three rows' shape; the
   reserved-set tests' deliberate update; the context widenings.
6. `view_state` units: the cycle with Today (enter-always, wrap,
   prev-from-Board); `new_starting`; the capture flow's three
   exits (commit-and-continue, empty-Enter close, Esc close) —
   commands asserted exactly; the ring across boards (canonical
   order + wrap at the last board's last stack); `follow_board`;
   `open_inbox`'s no-op/jump pair.
7. `models` units: `push_today` section assembly + the kiosk
   filter + the anchor filter + `today_tasks` concat + the chip
   count (structural, `0` after the filtered state);
   `FlowHintModel.text`; rollover by two `push` calls with a
   stepped `now`/window (no sleeps).
8. `presentation` units: `today_row_context`'s badge rule;
   `inbox_count`; `move_hint`.
9. App units: `capture_on` two-lane (receipt, `--board`
   resolution typed errors); `tasks add` default matrix (one
   board / zero / many); `tasks move --board` scoping; REPL line
   table row for `capture`.
10. Journeys: the today leg; the triage leg (chip reads zero);
    the ten-step regen; the kiosk-mount default pin.
11. Regen passes: registry/keymap snapshots; board goldens; the
    six today goldens.

## 6. Non-goals (dispatch §11 restated + this plan's)

No time-ruler/day-shape layout (sections are lists); no capture
overlay fields beyond the title (at-create scheduling is ruled
out — doc 2 §7's question stays answered "no"); no `tasks today`
verb, no PDF export; no kiosk chrome or rotation machinery (the
view is rotation-ready by not owning any); no cross-board
app-field repair (#350); no new undo semantics; no capture
session count ("N captured" polish); no `ContextId::Capture` (the
overlay rides `ViInsert`); no new guard verbs; no debounce; no
inbox-into-document anything (native lane only); no live-swap or
config growth (`day_start_time` exists; nothing new).

## 7. Risks, residual windows, interpretation flags

- **The base may move again** — a concurrent agent's worktree was
  live at plan time. Stage 3 re-verifies and does the rebase
  dance; stage 2 works purely on this branch. The r2 round touched
  the same files (view_state/actions/router/presets/journeys) —
  its shape was re-read at `aba316e` and the plan's inventory is
  stated against it; another round landing before this chunk's PR
  rebase-dances per AGENTS §8.
- **The default-view churn is wider than the design's one
  sentence**: every desktop-profile mount assertion of
  `View::Board` flips or escapes. The inventory names the
  mechanism (`new_starting` + the ten-step regen); stage 2
  enumerates the sites mechanically (`View::Board` at mount in
  ui tests) — the churn is mechanical but large; risk of
  missing one is caught by the suite, not by review.
- **`push`'s signature churn** (`DayCtx` + `kiosk`) touches every
  direct caller; contained to the ui crate + its tests.
- **The done-today golden determinism** depends on the frozen
  `DayCtx` reaching the goldens' `push` (it does — time is a
  parameter); if the today fixture's phrases prove unstable in
  review, phrase-free rows are the fallback (D13's rule).
- **`CommandOutcome::CreatedTask` reuse** means the CLI cannot
  distinguish capture from plain create in the receipt —
  intentional (the landing context is read back from state); if
  stage 3 wants a dedicated receipt, that is a finding to raise,
  not a silent change.
- **The `Some(board)` capture on a board with no live stacks**
  (all deleted): `backlog_stack` → None ⇒ the Backlog is created
  (order 0 over an empty board — the `append_stack_order` shape);
  no special case exists or is needed.
- **Outbox growth on first capture**: the Inbox creation queues
  one `CreateStack` op + one `CreateTask` op (board has none —
  unbound). On an unbound Inbox these strand honestly until the
  user binds (the ruled semantics; `plan_pushes` already
  noop-completes unbound boards' groups). No fence change.

## 8. Acceptance checklist (stage 2/3 verify against this)

- [ ] The section proptests green; no "today" derived bit stored
      anywhere (every question routes through `day_window` +
      `today_sections`).
- [ ] Rollover proven by stepped-`now` `push` calls (no sleeps).
- [ ] Re-commit writes `duedate = Some(Some(window_end))`
      confirmation-free via `DispatchCtx.day_window`; `NoTarget`
      without focus or window.
- [ ] First capture creates the unbound Inbox board + stack +
      task (snapshots); adoption of an existing exact-titled
      board; `Some(board)` lands in Backlog (canonical-first,
      created on demand); never errors on the happy path.
- [ ] The move ring crosses board edges with `board › stack`
      hints; no Inbox special-casing; the affected journeys
      regenerated in-chunk (deliberate).
- [ ] Desktop launches on Today; kiosk still launches on Board;
      the daemon is untouched.
- [ ] The chip counts to zero after a triage move (structural).
- [ ] Kiosk = the focused-board subset as a presentation filter;
      anchor task invisible in today rows/counts.
- [ ] `capture` CLI/REPL + `tasks add` Backlog default +
      `tasks move --board` round-trip typedly.
- [ ] Registry/keymap snapshots + board goldens + today goldens
      regenerated (the one deliberate pass); no `.pending`
      snapshots ship.
- [ ] Full AGENTS §10 gate suite green.

## 9. References

- Implementation plan
  `.artifacts/plans/2026-10-19-daily-driver-wave-implementation-plan.md`
  §2 DDF-15/DDF-11, §3 chunk 10, §4–§6.
- Design doc `.artifacts/plans/2026-10-19-views-finding-design-plan.md`
  Parts A/B (§3.1–§3.7), §4 record + riders, §6, §7 items
  1/2/3/5/6/7/8/10.
- Dispatch plan
  `.artifacts/plans/2026-10-19-daily-driver-implementation-dispatch-plan.md`
  §1 (MD-2/4/6/7/8/9), §11 (covers/acceptance/traps).
- Sibling stage-1 plans for house shape:
  `2026-10-24-daily-driver-find-plan.md` (chunk 9, the palette
  precedent), `2026-10-23-daily-driver-adhoc-alarms-plan.md`.
- Issues #346 (today view), #347 (inbox + capture) — milestone
  `wave-daily-driver`; #350 (the cross-board app-field gap this
  chunk tracks but does not fix).
