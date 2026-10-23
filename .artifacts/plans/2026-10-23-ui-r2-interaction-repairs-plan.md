# [Plan] GUI Review Round 2 — Chunk 2: Interaction repairs (D9–D15)

- **Date**: 2026-10-23
- **Kind**: stage-1 chunk plan (TDD-first; no implementation in this
  commit). Branch `fix/ui-r2-interaction-repairs`, cut at `main`
  `addda00` (chunk 1 `feat/ui-r2-board-surface` merged — precondition
  verified; `git worktree add -b` after a green
  `scripts/worktree-doctor.sh`, 101G free / 25G floor).
- **Origin** (normative): design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §3 (D9–D15),
  §1.2 (F-B1–F-B7), §10 (testing strategy), §14-R1/R2/R6 (risks);
  dispatch plan
  `.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`
  §1 (shared execution model + code-quality mandate) and §3 (this
  chunk's acceptance). Issue **#390** read in full — its fix direction
  is D9's mandate; its cited kiosk/netbook bench reports are accepted
  background and are **not** re-derived here; its true-partial bench
  re-measurement stays with the issue (stage 3 annotates, never
  closes). Issue **#373** (ListView virtualisation) stays out of scope;
  §2.6 records why this design does not preclude it.
- **Non-goals** (dispatch §3): no drag-semantics changes beyond what
  D9 makes work; no settings scroll (chunk 5 D30); no new actions
  (chunk 3); no bench work; empty diff over `taskboard-app` and
  `taskboard-domain` (expected: this chunk touches only
  `taskboard-ui-slint`).

## 1. Verified facts (anchored; line numbers are post-chunk-1 `main`)

- **F-1 The refresh pass rebuilds every model.** `models::push`
  (`src/models.rs:109-158`) and its four sub-pushers assign fresh
  `VecModel`s wrapped in fresh `ModelRc`s on **every** refresh:
  board bar segments (130-138), board columns + per-column cards
  (`push_board`, 235-297), stack-detail cards (300-331), task-detail
  rules/presets (336-399), banner rows (435-449), the six `[string]`
  hint models (`push_chrome`, 171-209), and the settings rows
  (`adapter.rs:931`). The repetition at `board-layout.slint:135`
  (`for column … `) and the nested card repeaters (267, 440) are
  destroyed and re-instantiated wholesale — the TouchArea that saw the
  first press dies. This is F-B1, the shared mechanism behind F8
  (double-click) and F11 (drag), and the mechanism #390 documents as
  "~97 % dirty region per refresh".
- **F-2 Refreshes happen more often than gestures.** The repaint
  driver (`src/lib.rs:317-383`) refreshes on every engine signal or
  system event **and unconditionally at every wall-minute boundary**
  (373-377) — an idle app still refreshes once a minute, so a
  double-click slower than a minute is still cross-cut by F-1. The
  drag seam (`src/adapter.rs:1327-1363`) refreshes **per tick**
  (`set_stack_width` at 1361 + `a.refresh` at 1362), killing the
  pressed `resize-handle` TouchArea (`board-layout.slint:200-220`)
  after the first move.
- **F-3 Slint 1.18.1 repetition semantics — stage-1 probe results**
  (run in this worktree on the null platform with an `init`-counter
  probe component; the test to land is §2.7): on the pinned Slint,
  for a `for`-repeater bound to a model property:
  1. Initial mount instantiates each row once; an unchanged re-render
     adds nothing.
  2. `set_row_data(i, …)` on the installed model — **equal or changed
     content** — updates the row's bindings in place and
     re-instantiates **nothing**.
  3. `insert` adds exactly one instance; `remove` adds none.
  4. Assigning the model property a **different** `ModelRc` (even with
     byte-equal content) **resets the repetition** — every row
     re-instantiates. This is F-1's exact damage.
  5. Re-assigning the model property the **current** handle is a
     no-op (no reset).
  6. Re-assigning a **struct** property that carries the model field
     with the same handles inside (the per-refresh `set_board_model`
     shape) re-instantiates **nothing**.
  7. Nested models: an inner `set_row_data` reaches the inner repeater
     with zero churn and **without any outer write**; an outer
     `set_row_data` (equal or changed scalars) that keeps the same
     inner handle re-instantiates nothing inner.
  8. `ModelRc` implements `PartialEq` by handle identity (clones of
     one handle equal, distinct handles unequal); the generated slint
     structs derive `PartialEq`.
- **F-4 Double-click wiring is intact and window-counted.**
  `task-card.slint:80` wires `double-clicked => root.activated()`; in
  i-slint-core 1.18.1 the **window** counts clicks
  (`input.rs` `ClickState::check_repeat`: same button, within
  `click_interval`, within 10 px²) using `animations::Instant` — the
  mock clock the testing backend controls — and the TouchArea that
  receives an odd-count release fires `double_clicked`
  (`items/input_items.rs:158-163`). So a synthesized double-click
  journey is feasible headlessly (§3), with one validity caveat (§3,
  decision rule).
- **F-5 Menu pointer paths skip the sync and lie about pages.**
  Both mouse invokers resolve pages Rust-side
  (`invoke_root_entry` → `root_entries`, `invoke_entry` →
  `menu_entries`; `src/adapter.rs:606-640`) but only `route`'s
  trailing sync (256-257, `KeyboardState` push at 126-154) updates the
  globals — `activate_parent` (593-600) ends at `sync_menu_models`
  only, so `menu-active-group` stays −1 and neither the wide fly-out
  (mount condition `app-window.slint:295`) nor the compact replace
  page (`action-menu.slint:51-56`) ever appears: F7's "clicking
  parent items does nothing". Second divergence: the root panel's
  rows always emit `invoke-entry` (`action-menu.slint:151` a11y,
  166 click) while `invoke-submenu-entry` is declared (45), wired
  (`app-window.slint:284`) and **never emitted** — in compact replace
  mode the panel renders the **submenu** page (`page` at 51-52) but
  the click resolves against `root_entries`: the F-B3 index-space
  mismatch. Leaf clicks work only because `apply_close_menu`
  (578-588) syncs explicitly.
- **F-6 Tooltip hide paths are trigger-local.** Hides happen on
  `has-hover` flips, `IconButton.clicked`
  (`ui/widgets/icon-button.slint:203-204`), and `init` of the
  dismissal layer / menu host (`app-window.slint:238`, 246). Menu-row
  clicks do not unhover (`action-menu.slint:166`), no view-switch
  cleanup exists, and the adapter never touches `TooltipState` — a
  visible/pending tip survives the anchor unmounting (F13). The
  Rust-callable dismiss primitive already exists: the global's
  `public function unhover()` (clears pending/visible/placed and stops
  the timer via `dismiss-requested`; the usability tests invoke it
  from Rust as `tooltip.invoke_unhover()`).
- **F-7 Tooltip width measures the caption only.**
  `ui/widgets/tooltip.slint:88-89`:
  `min(caption.preferred-width + 2·space-2, min(window·0.6, 420·scale))`;
  the `KbdHint` row (124-126, a `HorizontalLayout` whose preferred
  width is its keycaps' sum) is outside the computation (F28; height
  includes it). The panel reports its size via `size-observed`
  (96-98) into the placement hand-off (`src/lib.rs:230-273`).
- **F-8 No per-view window sizing exists.** The Window keeps constant
  preferred 1024×640·scale and floors 480×400·scale, with the "no
  width/height binding" invariant in force (`app-window.slint:79-90`).
  R1-D15's stage-2 probe (which mounted view's content min-constraints
  exceed the window) was never run — D15 runs it (§8).
- **F-9 The tests that must not break silently**:
  `tests/journeys.rs` context menu (1113-1163), drag seam
  (1304-1352), auto-width sentinel (1359-1382 — the never-persist-on-
  refresh invariant), keyboard resize (1243-1299); `tests/
  usability_components.rs` tooltip delay/dismissal/restart (130-230),
  hand-off (236-297), menu-row hover (303-341), menu geometry
  (387-436); the whole lib/journey/golden/keymap suite. Any edit to a
  test that pinned buggy behaviour carries the recorded-decision
  comment naming this plan (dispatch 2.2 hard rule).

## 2. D9 — Stable model identity via row-diff push (implements #390's fix direction)

### 2.1 Mechanism decision: row-diff on push, persistent handles (not caller-side dirty flags)

**Recorded decision**: the push pass keeps building the *complete
desired* rows exactly as today, then applies them to **persistent
`Rc<VecModel<T>>` handles** through a diff. Caller-side dirty flags
were rejected: the pass's inputs (ArcSwap state, view_state, keymap
chain, clock) change for many reasons across many seams; flags would
have to be maintained at every mutation site and any missed
invalidation is silent stale UI. A full re-derive + diff is
self-contained in `models.rs`, keeps push *complete-state* (the
stage-3 trap (a): reconcile-on-vanish and every other correctness
path is untouched — only the transport becomes granular), and the
probe (F-3) proves the diff's no-op path is a renderer no-op.

The handle set lives in a new `models::PushModels` struct owned by the
`Adapter` (one per mounted window):

| Slot | Rows | Keyed by |
|---|---|---|
| `bar_segments` | `BoardSegmentModel` | positional |
| `columns` | `StackColumnModel` | positional + a `positions: Vec<StackId>` mirror |
| `column_tasks` | `TaskCardModel` per column | `StackId` |
| `card_decorations` | `TaskDecoration` per card | `TaskId` (pruned to the pass's tasks) |
| `detail_tasks` | `TaskCardModel` (stack view) | positional |
| `task_rules`, `task_rule_presets` | `RuleRowModel` / `SharedString` | positional |
| `banner` | `BannerRowModel` | positional |
| `settings_rows` | settings connection rows (`adapter.rs:931`) | positional |
| six hint slots | `SharedString` (`toolbar-menu-hint`, `nav-back-hint`, `board-segment-hint`, `task-card-hint`, `stack-header-hint`, `stack-create-hint`) | positional |

`card_decorations` is **required, not optional**: `TaskCardModel`
compares by `PartialEq`, and its `decorations` `ModelRc` field compares
by handle identity (F-3.8) — with a fresh decorations `VecModel` per
card per pass every card row would differ on every refresh and every
push would `set_row_data` everything (correct but zero budget win).
Keying decorations by `TaskId` and pruning to the pass's task set
makes unchanged cards compare fully equal.

### 2.2 The diff primitive

```rust
/// Applies `desired` onto the persistent model; returns the number of
/// row mutations (tracing::trace!-logged per pass).
fn diff_rows<T: PartialEq + Clone>(model: &VecModel<T>, desired: &[T]) -> usize
```

Positional compare over the common length (`set_row_data` only where
`row_data(i) != desired[i]`), then truncate the tail (`remove`), then
append (`insert`). No LCS: a mid-list reorder costs a set_row_data
cascade over the suffix — in-place binding updates, zero
re-instantiation (F-3.2), and reorders are user-move-scale, not
refresh-scale. One generic helper `push_rows(slot, install, desired)`
does get-or-create (installing the handle into its window property
exactly once via the existing setter), then diffs.

### 2.3 Push shape after the change

`models::push` gains a `&mut PushModels` parameter (the `Adapter`
passes `&mut self.models`; `push_settings` becomes `&mut self`).
Every sub-pusher keeps its current code that computes the desired
`Vec<…Model>` rows; the tail of each changes from
"`window.set_…(ModelRc::new(VecModel::from(rows)))`" to
"`push_rows(…)`, then set the scalar struct fields as today".
Per F-3.5/6 the per-refresh struct pushes (`set_board_model` with the
persistent `stacks` handle inside, same for `stack-model`,
`task-model`, `board-bar-model`, `SettingsModel.rows`) are renderer
no-ops for their repetitions while keeping the scalar fields live —
**no .slint struct splits are needed**.

Column-level rule (the F-3.7 consequence): the desired
`StackColumnModel` for stack *S* carries `ModelRc::from(handle(S))`
after diffing that column's cards; the outer `set_row_data(column_i, …)`
fires only when the row actually differs — scalars **or** the inner
handle (board/stack switches rebind exactly the rebinding columns;
unchanged refreshes are zero-mutation). The `positions: Vec<StackId>`
mirror makes the reorder case explicit.

The refresh pass (`adapter.rs:322-331`) keeps its exact sequence —
`sync_view_context` → ArcSwap load → `view_state.reconcile` →
`models::push` → `push_settings`. The `IdTables` rebuild is unchanged.

### 2.4 What each repair gets out of it

- **F11 (drag)**: the per-tick `set_stack_width` + `refresh` stays as
  shipped (D11 keeps live updates). The tick now lands as one
  `set_row_data` on the dragged column — in-place (F-3.2/7) — so the
  pressed `resize-handle` TouchArea survives the whole gesture.
- **F8 (double-click)**: the first click's focus refresh now produces
  one `set_row_data` for the emphasised card (emphasis is a row
  field); the clicked card's TouchArea survives and receives the
  second release.
- **#390's first leg**: unchanged refreshes become zero-notification;
  small changes touch exactly one row — the render budget's dirty
  region collapses from ~97 % toward the true partial. The bench
  re-measurement itself stays with #390 (stage 3 annotates).

### 2.5 #373 non-preclusion (structural constraint, normative)

The design does not preclude #373 (ListView virtualisation): the
`for`-repeater markup stays untouched; every repeated surface keeps a
single stable per-surface model with stable row addressing, which is
exactly the model shape a `ListView` consumes later; the id tables and
index-based seams are unchanged. **A comment at the push seam points
at #373 as the next lever on the same render budget** (required by
design D9; review checks for it).

### 2.6 Known residuals (recorded, accepted)

- The compact leg's single-column repeater is fed by
  `[root.selected-column()]` (`board-layout.slint:432`), a binding
  re-evaluated whenever the stacks model notifies a row change; an
  outer `set_row_data` therefore re-instantiates the compact column.
  No pointer gesture exists in compact (no resize handle) and the
  behaviour is strictly better than today's wholesale rebuild.
  Accepted; noted so nobody chases it as a regression.
- A refresh that changes a column's row while its inline create field
  is open re-instantiates that column (and the field). Pre-existing
  (today every refresh rebuilds everything); typing itself causes no
  pushes (the buffer is view_state-side), so this only bites on a
  concurrent content change. Accepted residual.
- Menu page models (`adapter.rs:159-190`) keep wholesale replacement —
  they change only on menu transitions where a fresh page is the
  intent. Untouched.

### 2.7 Proof tests (land in this chunk)

1. **The probe, checked in** (`src/lib.rs` `#[cfg(test)]` module; the
   stage-1 scratch version ran green in this worktree): an inline
   `slint::slint!` component with a global `init` counter over a
   `VecModel`, asserting the F-3 table — `set_row_data` (equal and
   changed) and same-handle property re-assignment and same-handle
   struct re-assignment re-instantiate nothing; a fresh `ModelRc`
   with equal content re-instantiates everything; `insert` adds one;
   `ModelRc`/struct `PartialEq`. This pins the mechanism on the
   pinned Slint and is the canary for future Slint bumps.
2. **`diff_rows` unit tests** (`models.rs` tests): equal rows → 0
   mutations; one changed row → 1; append → 1; truncate → N; reorder
   → positional sets; the `PushModels` column rule skips
   `set_row_data` when scalars and handle are unchanged.
3. **Identity pin journey** (`tests/journeys.rs`): mount the
   populated fixture, `refresh()` twice unchanged —
   `get_board_model().stacks` is the same `ModelRc` (`PartialEq`),
   and a title edit lands through row update with the handle still
   stable across that refresh.
4. The full existing suite passes **unchanged** — including the
   sentinel journey's "refreshes alone never persist" and the
   mid-edit-refresh pins (`refresh_pushes_the_board_structure…`,
   `editing_dispatches_nothing…`: a refresh with zero mutations must
   not close the create row — with the diff it no longer even
   notifies the repetition).

## 3. D10 — Double-click: synthesized journey first, Tier-4 as the fallback

**Decision**: attempt the real regression journey; the recorded
decision rule decides between it and the Tier-4 item.

- **The journey** (`tests/journeys.rs`): mount the populated fixture,
  resolve a card's geometry from its a11y label
  (`find_by_label(...).absolute_position()` + `size()` — the chunk-1
  geometry oracle), then **click at its centre, `handle.refresh()`,
  click again at the same centre — with no `advance_time` between the
  refresh and the second click**. Assert
  `get_current_view() == View::TaskDetail` (the `double-clicked =>
  activated()` wiring fired). The window-level `ClickState` counts the
  pair on the mock clock (F-4); the D9-fixed transport keeps the same
  card instance mounted across the refresh so the second press
  belongs to a card at all.
- **Validity probe (decision rule, stage 2, before keeping the
  journey)**: the journey counts as the regression proof only if it
  **fails against the old transport** — stage 2 verifies once by
  locally reverting `push_board` to the wholesale `ModelRc` swap
  (uncommitted) and running the journey: if the repetition's
  re-instantiation is deferred such that the stale tree still
  receives the second click, the journey cannot distinguish and is
  **demoted**: the headless proof for F8 remains §2.7's tests 1+3
  (instance survival is the mechanism), and the double-click lands as
  a **Tier-4 manual checklist item** ("double-click a card in the
  running app opens Edit Task" — handed to the user at PR time, the
  `c6414ea` compromise, now with the root cause fixed). Either way
  the Slint `double-clicked => activated()` wiring itself is
  **unchanged** (D10's constraint).
- A second Tier-4 item records the *feel* ("double-click at
  real-user speed opens Edit Task; single click still only focuses").

## 4. D11 — Drag survives refresh; live update kept

- **Kept as shipped**: press captures base, drag clamps
  base+delta into live bounds and re-renders per tick, release
  persists the full snapshot once (`adapter.rs:1309-1390`). D9 makes
  the gesture survive; nothing in the drag semantics changes.
- **New journey** (`tests/journeys.rs`, extends the drag-seam
  journey's shape): press column 0 → tick → `handle.refresh()` →
  tick → replace the state (a new task in another stack) + refresh →
  tick → release. Assert: each tick's live width is on the model
  (`column_width_px(0)`), the interleaved passes did not disturb the
  drag (`width_drag` still addresses column 0 — the tables guard at
  1341 still resolves), requests stay empty until release, and
  release emits exactly one `StackWidthsChanged` with the final
  width. This is the seam-level pin headless can make (dispatch
  acceptance b).
- **Tier-4**: "drag a header edge: the width tracks the pointer live
  through the whole gesture, however long" (F11's feel — the
  TouchArea survival is headless-proven by §2.7 test 1, the feel is
  the user's).

## 5. D12 — Menu pointer path syncs; page-aware index space

### 5.1 Shared post-mutation sync

Factor `route`'s trailing pair (`sync_view_context` + `sync`,
`adapter.rs:256-257`) into one helper,
`fn finish_mutation(&mut self, window: &AppWindow)`. Callers:
`route` (unchanged behaviour), `apply_close_menu` (its explicit
`sync` at 587 folds into the helper), **`activate_parent`** (new —
the F7 fix: the parent path finally pushes `menu-active-group` and
friends onto `KeyboardState`), and the `UiEffect::MenuBack` arm
(543-553, replacing its inline conditional sync). The resulting
double-syncs on the leaf path (`apply_close_menu` → `route`) are
idempotent global writes — accepted; the reviewer trap (c) is about
*behaviour*, and `an_outside_click_dismisses_the_menu` plus the leaf
journeys stay green unchanged.

### 5.2 The index-space rule (normative wording)

**A row's click seam names the page the row was rendered from, and
the Rust resolver resolves against that same page vector.**

- Slint side (`action-menu.slint`): rows stop emitting
  `invoke-entry` unconditionally. A pure function
  `invoke-row(index)` emits `invoke-submenu-entry(index)` when the
  panel is in compact replace mode (`root.compact &&
  root.active-group >= 0` — exactly the `page` property's condition
  at 51-52), else `invoke-entry(index)`. Both existing emit sites
  (the a11y default action at 151 and the TouchArea click at 166)
  call `invoke-row`. The fly-out instance (regular/expansive,
  `is-flyout`) keeps its host binding
  `invoke-entry → menu-submenu-entry-invoked`
  (`app-window.slint:311`) — it always renders the submenu page and
  its panel never satisfies the compact condition, so the rule
  routes it correctly with no per-instance special case.
- Rust side (unchanged resolvers, now correct by construction):
  `menu-entry-invoked(i)` → `invoke_root_entry(i)` → `root_entries`;
  `menu-submenu-entry-invoked(i)` → `invoke_entry(i)` →
  `menu_entries` (the active page). `activate_parent` rewrites
  `menu_entries` and pushes `active-group` in the same mutation
  (§5.1), so the displayed page, the emitted seam, and the resolving
  vector always agree in both breakpoints. `apply_close_menu` clears
  both pages, so no stale index survives a close.
- The declared-but-never-emitted `invoke-submenu-entry` (45) becomes
  live — the seam names stop lying (F-B3's second divergence).

### 5.3 Journeys

- **Wide parent click** (`tests/journeys.rs`): mount → open the menu
  → `click_by_label` of a parent row (the ViewNav group's parent) →
  assert `KeyboardState.menu-active-group >= 0` **and** the fly-out
  mounted (`MenuPanelFacts.flyout-width > 0`) → click the "Next
  view" leaf in the fly-out → `get_current_view()` advanced. (Labels
  are element addresses; the typed outcome is the view switch — no
  text assertions.)
- **Compact parent + page-resolved leaf**: `set_window_size(600,
  800)` → open → click the parent row → `menu-active-group >= 0` and
  the replace page's rows are addressable → `click_by_label("Next
  view")` → the view advanced. Pre-fix this click resolved against
  `root_entries` (wrong page — the index-space proof, dispatch
  acceptance d's compact leg).
- Existing context-menu journey (1113-1163: flat page, pointer
  anchor, typed command) passes unchanged.

## 6. D13 — Tooltip lifecycle centralized

### 6.1 The adapter-side dismiss

No new slint API: the global's existing `public function unhover()`
is the dismiss-all (F-6). The adapter gains one private helper,
`fn dismiss_tooltip(window: &AppWindow)` — recorded naming decision:
design D13's "Rust-side `dismiss_all`" is this adapter method (a
duplicate slint-side alias was rejected as dead surface). Invoked at
exactly these effect-application points:

1. `apply_effect`'s `UiEffect::OpenMenu` arm (`adapter.rs:520-531`) —
   every menu open.
2. `apply_close_menu` (578-588) — every menu close: the CloseMenu
   effect, the dismissal layer, the toolbar toggle, and both mouse
   leaf-invocation paths that share it.
3. `interpret`'s leaving block (418-430) — every view switch by
   effect: NextView / PrevView / FocusBoard / BoardNext /
   BoardPrevious / BoardSelect.
4. `interpret`'s `OpenSettings` arm (496-502).
5. `install_settings_back_seam` (1499-1510) — the settings view's own
   back affordance (a close that bypasses `interpret`).

The existing `init => TooltipState.unhover()` on the dismissal layer
and menu host (`app-window.slint:238`, 246) stay as belt-and-braces.

### 6.2 The ActionMenu row unhover (Slint-side)

`action-menu.slint`'s row TouchArea `clicked` (166) and the a11y
default action (151) call `TooltipState.unhover()` before invoking —
the `icon-button.slint:203-204` pattern. Rule: a tooltip never
outlives the element that spawned it.

### 6.3 Journeys (`tests/usability_components.rs`, mock clock)

- "A visible tooltip does not survive a view switch": hover a card
  (real `hover_at` through its a11y geometry), `advance_time(600)` →
  visible → `harness.press(ActionId::NextView)` → `advance_time(0)`
  → assert `!visible && !pending && !placed`.
- "A menu-row click unhovers": open the menu, hover a row (the scan
  pattern of the existing row-hover test) → pending → click a parent
  row → assert `!pending && !visible` (and the submenu opened —
  merging with §5.3's wide journey is allowed; one behaviour one
  test: keep the unhover assertion here, the submenu assertion in
  the D12 journey).

## 7. D14 — Tooltip width includes the hint row

**Formula** (`ui/widgets/tooltip.slint`, replacing 88-89):

```
width: min(max(caption.preferred-width, hint-natural()) + 2 * Theme.space-2,
           min(root.window-width * 0.6, 420px * Theme.base-scale));
pure function hint-natural() -> length {
    root.keys.length == 0 ? 0px : hint-row.preferred-width;
}
```

with `hint-row := KbdHint { … }` (the existing conditional element at
124-126, now named). The caption term keeps its wrap-probe comment as
the authority (the caption's unwrapped preferred width is the natural
term, P5); the hint row measures its own keycap-row preferred width
(the `HorizontalLayout` sums its caps). Caps unchanged. Placement is
untouched — it consumes the reported size via the existing
`size-observed` hand-off.

**Testability**: `TooltipState` gains two plain `in-out` mirrors,
`tip-width`/`tip-height`, written next to the `size-observed` report
(no new callbacks; the existing placement flow is untouched) — the
geometry oracle for tests.

**Geometry pin** (`tests/usability_components.rs`): relation-only
assertions — (a) hover with a short caption and no keys →
`tip-width = w1`; re-hover with the same caption plus a two-keycap
hint → `tip-width > w1` (the F28 relation, structural, no font
metrics); (b) the absurd-caption case keeps the cap:
`tip-width <= window·0.6 + ε`; (c) keys-less hover leaves the width
at the caption term (a hint-less tip is not widened by an empty
model).

## 8. D15 — Window-stability probe (R1-D15 stage 2 completed)

**Hypothesis (F-B6)**: view switches resize the window because a
mounted view's content min-constraints exceed the current window and
winit grows it. **Fix rule (normative)**: any proven offender is
fixed by capping its content min-width (elide/wrap at the window
floors) — **never** by binding Window width/height (the
`app-window.slint:79-90` invariant; stage 3 greps the diff, dispatch
acceptance g).

- **Instrumentation**: a stage-2 scratch instrumentation (reverted
  before handoff) logging `window.get_window_w()` /
  `window.get_window_h()` + the active `ViewId` after every view
  change and at the leave/enter boundaries (the `interpret` leaving
  block is the single choke point to hook). `Window::set_size` is the
  only geometry input in the harness; the real-resize evidence comes
  from the **real backend run** (winit), not the testing backend.
- **Run environment**: the dev box's real backend; if
  `$DISPLAY`/`$WAYLAND_DISPLAY` is absent, `xvfb-run`. The window is
  placed at the min floor for the scale under test
  (480×400·scale); each view switch is keyboard-driven.
- **Matrix**: scale ∈ {0.5, 1.0, 3.0} × views {board, stack-detail,
  task-detail} — settings is out of scope (chunk 5's D30 scroll is
  the settings-side fix; design D15).
- **Expected offenders** (F-B6): task view; possibly none on the
  board views (their long-text elements already elide).
- **Decision rule / escalation (R6, normative order)**: (a) an
  offender view at a matrix point → cap that content's min-width,
  re-run the matrix point, record; (b) resizes observed with **no**
  content-min violation → try R1-D15's recorded fallback (per-view
  preferred-size capping), re-run; (c) still resizing → **STOP**,
  report the full matrix to the user. Never Window bindings.
- **Results**: recorded in §10 below by stage 2 (design D15: "the
  matrix results in the chunk plan").
- **Tier-4**: "switch views at scale 3.0 in a minimum-sized window —
  the window never resizes" (F22).

## 9. Test catalogue (fix → test; design §10 discipline)

| Fix | Test | Layer |
|---|---|---|
| D9 mechanism | probe tests (F-3 table) | lib, null platform |
| D9 diff | `diff_rows` + column-skip unit tests | `models.rs` |
| D9 identity | consecutive-refresh ModelRc identity pin | journeys |
| D9 no-drift | existing suite green unchanged (sentinel, mid-edit refresh, goldens) | full suite |
| D10 double-click | synthesized click → refresh → click journey, gated by the §3 validity probe (fallback: Tier-4 only) | journeys (+ Tier-4) |
| D11 drag | refresh-interleaved drag journey (live ticks, release-only persistence) | journeys (+ Tier-4 feel) |
| D12 wide parent | parent click opens fly-out, leaf switches view | journeys |
| D12 compact page | parent click replaces page; leaf resolves the displayed page | journeys |
| D12 regressions | outside-click dismissal, context-menu, keyboard invoke journeys unchanged | existing |
| D13 view switch | visible tooltip does not survive NextView | usability |
| D13 menu invoke | row click unhovers (pending and visible cleared) | usability |
| D14 width | hint widens (relation), cap holds, keys-less unchanged | usability |
| D15 | matrix results in §10; grep pin: no Window width/height binding; `min-width` caps on proven offenders only | probe + review |
| Tier-4 (user) | double-click feel; drag feel; view-switch window stability at scale 3.0 | manual checklist |

Test rules carried from design §10 / AGENTS §5: no sleeps (the mock
clock only), no text assertions (labels and action ids are addresses;
typed outcomes and structure only), one behaviour one test at the
lowest layer that can observe it, and any existing-test edit carries
the recorded-decision comment.

## 10. D15 probe results (stage-2 fill-in)

- [ ] matrix: (scale × view) → window resized? content-min offender?
- [ ] fixes applied (min-width caps) and their re-run results
- [ ] escalation path taken, if any

## 11. Code inventory & estimated size

- `crates/taskboard-ui-slint/src/models.rs` — `PushModels`,
  `push_rows`/`diff_rows`, sub-pusher tails, tests (~+190).
- `crates/taskboard-ui-slint/src/adapter.rs` — `models` field,
  `push`/`push_settings` signatures, `finish_mutation`,
  `activate_parent`/`MenuBack` sync, `dismiss_tooltip` + 5 hook
  calls (~+55/−15).
- `crates/taskboard-ui-slint/src/lib.rs` — checked-in probe tests
  (~+120, test-only).
- `ui/compositions/action-menu.slint` — `invoke-row` rule, row
  unhover (~+10/−3).
- `ui/widgets/tooltip.slint` — width formula, `hint-row` id,
  `tip-width`/`tip-height` mirrors (~+12/−2).
- `ui/app-window.slint`, `ui/widgets/task-card.slint`,
  `ui/compositions/board-layout.slint` — expected **no diffs**
  (double-click wiring, seams, and repeaters stay).
- `tests/journeys.rs` — D10 journey + validity probe note, D11
  interleave, D12 ×2, D9 identity pin (~+170).
- `tests/usability_components.rs` — D13 ×2, D14 pins (~+110).
- `CHANGELOG.md` — Unreleased entries per dispatch 2.2.

Estimated total ≈ **+670 / −20 over ~8 files** (all in
`taskboard-ui-slint`; `taskboard-app` and `taskboard-domain` empty).

## 12. Risks

- **R1 (design) applies in full** — D9 underpins two user-visible
  fixes and touches every refresh path. Mitigations: the mechanism is
  probe-proven on the pinned Slint (F-3, checked in as tests), the
  full existing suite must pass unchanged, and the diff keeps push
  complete-state so no correctness path changes shape.
- **The double-click journey may not distinguish** (deferred instance
  destruction could hand the second click to the stale tree) — the §3
  validity probe decides; the fallback path is explicit.
- **Decorations persistence is easy to miss** — without it the diff
  silently degrades to per-push `set_row_data` on every card
  (correct transport, no budget win). Review checks §2.1's
  `card_decorations`.
- **Compact-leg churn** (§2.6) and **create-field re-creation on
  concurrent change** (§2.6) are accepted residuals — recorded so the
  review does not flag them as D9 regressions (both strictly better
  than today).
- **R2/R6**: D14's formula and D15's caps are layout-adjacent; both
  carry probe/settled rules above and the escalation path ends at
  the user, never at Window bindings.
- **#390 bookkeeping**: stage 3 posts the annotation (fix direction
  landed via this PR; the true-partial bench re-measurement stays
  open with the issue) — never closes it. No other backlog issue
  closes in this chunk.
