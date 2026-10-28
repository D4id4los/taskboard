# [Plan] UI R3 chunk 6 — navigation, palette & menu curation

- **Date**: 2026-10-28
- **Kind**: stage-1 implementation plan (TDD-first) for chunk 6 of
  GUI review round 3.
- **Branch**: `feat/ui-r3-navigation-menu` (worktree
  `../taskboard-WT-feat-ui-r3-navigation-menu`, cut from `origin/main`
  at `ee746ca`).
- **Origin**: design round
  `.artifacts/plans/2026-10-26-gui-review-round3-plan.md` §3.6
  (D22–D29, normative), §1 findings 27/32/33/34/35/36/37/38/39/40,
  §7-R3 (structural pins only — never label text), §4 menu test
  strategy; dispatch plan §1/§7. Chunk-3 boundary record:
  `2026-10-26-ui-r3-taskview-dates-plan.md` §6 ("preset-adapted
  picker bindings, the control-nav walk, and the emacs-mode arrow
  conflict are chunk 6's registry work").
- **Precondition**: verified. Chunks 1 (`a430f80`…`5ab0fd1`, flow
  machine) and 3 (`6291a3c`…`dd73019`, task view & dates) are merged
  into `origin/main`; 2, 4, 5 also merged. Chunk 6 may proceed.
- **Status**: READY for stage 2.

## 1. Scope & non-goals

In: toolbar view switcher (D22), `nav.show-task` (D23), palette
layout (D24), control-nav walk (D25), scroll actions (D26), menu
curation (D27), emacs C-g C-g (D28), #231 snooze/mute preset rows
(D29).

Out (fenced): #411 board dropdown redesign (deferred); the tracking
half of #210 (DR-5); DR-3 settlement (the reminder prune cascade —
untouched); no domain/state/storage/sync diffs (empty-diff gate over
those crates); no snooze-*duration* presets (DR-3 fence); the item
focus walk (`nav.next/prev-item`) semantics are unchanged — this
chunk adds a *second*, control-level walk beside it.

## 2. D22 — the toolbar view switcher (R3-F37)

Option A per the design: a two-segment `SegmentedControl` in the
toolbar. The widget already exists (`ui/widgets/segmented-control.slint`,
round-2 D26) with exactly the needed shape: caller-owned
`selected-index`, a `picked(int)` callback, per-segment enablement
unused here (all enabled).

### 2.1 Mount & geometry

- `toolbar.slint`: the switcher sits between the title and the inbox
  chip: `title-text | SegmentedControl | inbox chip | [kbd hint] |
  menu button`. Segment labels are markup-owned presentation
  constants `"Board"`, `"Today"` — index-order contract **`[Board,
  Today]`**, documented at the use site (the r2 D26 index-contract
  precedent; the settings picker uses `[Standard, Emacs, Vi]`).
- Visible at every breakpoint and on kiosk (no gate): it is chrome
  driven by the same two actions the keyboard already has; the
  compact path drops the inbox chip, never the switcher.
- Width: the segments size to their labels (`horizontal-stretch: 0`
  on the control; the title keeps `stretch: 1` and elides as today).

### 2.2 Wiring — model ↔ view_state, both directions

- **Click → action (§7.1 click ≡ key)**: the toolbar grows two pure
  callbacks, `switcher-board-clicked()` and `switcher-today-clicked()`;
  `app-window.slint` forwards them and the adapter dispatches the
  *existing* registry actions through the same path the keyboard
  uses (`ActionRouter::dispatch(ActionId::NavFocusBoard / ViewToday)`
  — the hamburger's `app.menu` precedent). No new UiEffect, no new
  view_state verb: `focus_board()` / `focus_today()` already exist
  and the leaving-rule settle applies unchanged (`FocusBoard` is
  already a leaving effect; `FocusToday` **joins the `leaving` match**
  — it is a view switch and must settle-then-run under live flows
  exactly like R3 requires of the other switches; this also closes
  the gap where a flow was live while clicking Today).
- **State → model**: the models pass pushes `switcher-index` (new
  `in property <int>` on Toolbar) from a new pure projection:
  `view == Today → 1`; `view ∈ {Board, StackDetail, TaskDetail} → 0`
  (the deeper views are the board branch — the board segment stays
  lit, mirroring how `nav.focus-board` returns from them);
  `view == Settings → settings_return`'s projection
  (`unwrap_or(Board)`) — the segment that names where settings
  returns to. One home: a small pure function in `view_state.rs`
  (`switcher_selection(&self) -> usize`), unit-tested, consumed by
  the adapter's models pass.
- **Settings return**: no staleness path exists — `open_settings`
  overwrites `settings_return` on every open, and a switcher click
  while in Settings dispatches through the leaving rule (settles any
  flow, switches) leaving `settings_return` to be re-set at the next
  open. Pinned by a journey, not by new code.
- **Refresh sync**: every `FlowOutcome::RefreshNow` rebuilds the
  pushed index; the click's own dispatch ends in a refresh, so the
  selection echo is reply-sourced (the r2 D27 segmented-control
  discipline — the widget never owns state).

### 2.3 Journeys

Switcher click Board→Today→Board with model snapshots; keyboard
equivalence (`Ctrl+T`/`Alt+1` produce the same pushed index);
settings round-trip with the switcher reflecting the return view;
compact breakpoint; a live create-flow settled by a switcher click
(the leaving rule).

## 3. D23 — `nav.show-task` (R3-F35)

### 3.1 `ViewState::show_task`

```rust
pub fn show_task(&mut self, state: &AppState, task: TaskId) -> Vec<FlowOutcome>
```

- Resolve task → stack → board; a stale id (entity died between push
  and invoke) is **inert**: empty outcomes, no repaint — the
  palette_jump stale rule.
- Live target: `self.board = Some(board)`;
  `self.selected_stack = Some(task.stack)`;
  `self.focused_task = Some(task)`; `self.view = ViewId::Board`;
  returns `[RefreshNow]`.
- **The view is always Board** (the task's home): from Today, from a
  stack/task detail, from Settings, from compact — one semantics, no
  per-source branching. Done tasks have board cards (the r3 D6
  checkbox styling); the scroll lands on the done card. Cross-board
  targets select the task's board — same code path, no special case.
- Subjects (`subject_stack`/`subject_task`) are left alone — the
  refresh-time re-validation pops stale ones (existing reconcile
  rule).
- From Settings: `settings_return` stays as-is (harmless —
  overwritten at next open; same reasoning as §2.2).

### 3.2 The action, the effect, and the palette

- Registry row `ActionId::NavShowTask` → `"nav.show-task"`,
  `contexts: &[ContextId::Palette]`, `Category::Navigation`,
  `menu_group: Some(MenuGroup::ViewNav)`, `enabled: true`,
  rank 90 (§7 table). Palette-only contexts mean it *never renders
  in a menu* (the menu chain never contains the masked `Palette`
  context — the `menu.*` rows precedent) while remaining a first
  class row for config overrides, hints, and the snapshot
  documentation. Recorded decision: **no preset binding** — its
  trigger *is* the palette's Return, which is already bound as
  `menu.invoke` in the `Palette` context; a second chord would be a
  synonym, and the no-second-class gate is satisfied uniformly
  (unbound in all three presets), like `view.inbox`.
- New `UiEffect::ShowTask` (no payload), class `EffectClass::Nav`:
  the dispatch arm routes it through the palette table:
  `tables.palette[selected]` → a `SearchTarget::Task` arm calls
  `view_state.show_task`; stack/board targets keep today's
  retargets; a stale/absent cursor closes the palette (existing
  `close_palette`). The router's pure dispatch maps
  `NavShowTask → UiEffect::ShowTask` with `ctx` untouched —
  dispatching it outside a palette flow is **inert** (the walk's
  ends hold; the row description says it acts on the palette's
  selection).
- `apply_palette_invoke`'s Task arm (keyboard Return *and* row
  click) calls `view_state.show_task` — the exact function the
  effect arm calls. One implementation, two dispatch sites, no
  parallel path (the review's seam discipline).
- `palette_jump`'s `SearchTarget::Task` arm is **rewired** to
  `show_task` (it *is* the bug's fix: today it selects the board and
  focus but leaves `selected_stack` on the board's first stack and
  never changes the view or scrolls). Stack/board arms unchanged.

### 3.3 The scroll request

`push_focus_scroll_hint` currently runs only under the focus-walk
effects. The condition widens to `focus_walk || effect ==
ShowTask`; `before` reads the pre-effect focus so the retarget
always arms the hint (the guard `focused == before` stays for the
inert ends). Board-view hint = `(column, row)` from
`tables.board_position` — the column's `ListView` seeks (existing
consumer). Compact: the compact board leg's viewport consumes
`target-b` (the `stack-view` filter shape — verified during
implementation; if the compact leg needs its own `target-a == -1`
filter twin, it is a two-line markup addition in
`board-layout.slint`'s compact leg, pinned by the compact journey).
Stack-target palette jumps also emit their `(-1, row)` hint
(StackDetail view) — same widened condition, no extra code.

### 3.4 Journeys

The R3-F35 repro as a journey: open the palette **from Today**,
query a task that lives **on another board**, press Return → Board
view is mounted, the task's board is selected, `selected_stack` is
the task's stack, the card is focused, and the column ListView's
`request-nonce` advanced with the task's row (assert the pushed
`FocusScrollHint` global + the view/selection model — structural,
never text). Siblings: activation from Settings, from TaskDetail,
in compact; a done task; a stale target (delete the task under the
open palette via the fake engine, then Enter → close only).

## 4. D24 — the palette layout (R3-F34)

Root cause: `SearchPalette` inherits a bare `Rectangle`; its
`TextField` sets `min-height` but no preferred height, so the
vertical layout distributes the surplus into the field — a query row
that grows with the palette ("full-screen field").

- `text-field.slint` gains `out property <length> natural-height:
  placeholder-text.preferred-height + 2 * Theme.space-2;` — the
  single-line fact the widget already computes for `min-height`,
  exposed as a fact (tokens only, no hardcoded px; ≈40 logical px at
  scale 1).
- `search-palette.slint` binds the field `height: field.natural-height;`
  and pins every row and the caption `vertical-stretch: 0` — the
  input row is now fixed; results list below; the palette's height is
  content-driven and clamped above the status bar by the existing
  app-window expression (`PALETTE_ROW_CAP = 8` bounds it).
- Geometry pin (journey): with zero rows the palette's height equals
  `natural-height + 2 * Theme.space-2` (padding) — the full-screen
  field's regression pin; with 8 rows the palette stays inside the
  window at min viewport (the clamp journey exists for the menu; a
  palette twin joins it).

## 5. D25 — the control-navigation walk (R3-F27)

### 5.1 The walk's definition

- **What counts as a control**: the runtime's tab stops — every
  `focus-on-tab-navigation` FocusScope and text input mounted in the
  current tree, in the runtime's traversal (reading) order.
  Per-view inventory: toolbar (switcher segments, menu button; the
  inbox chip is a TouchArea with an a11y role but *not* a tab stop —
  recorded observation, unchanged); board: add-task ghosts, new-stack
  button, empty-board CTAs (the header/card TouchAreas are *item*
  territory, not stops); today: timers field + buttons; task view:
  back button, field-level icons, track buttons, the date-picker
  grid and its month/time buttons; settings: fields, eye, sliders,
  preset segments, row buttons. Cards and rows are **not** controls —
  the item walk owns them. The two walks compose, not overlap.
- **Interaction with text-editing chains — outside Insert**: the
  control rows bind **only in the five view contexts** (Today,
  Board, StackDetail, TaskDetail, Settings — a new `CONTROL_NAV`
  availability const). In insert mode the chain is
  `[ViInsert, Global]`; the view context is masked and no
  control-nav row exists there, so the emacs `C-f`/`C-b`/`C-n`/`C-p`
  caret motions of a focused field reach it natively — text editing
  is untouched (D25's parenthetical). The open menu/palette mask
  everything (modal), so the walk cannot run there either.
- **Wrap**: the runtime traversal wraps (tab convention) — recorded
  as deliberate, unlike the item walk's hard ends.
- **No auto-scroll**: the walk does not scroll the focused control
  into view (parity with today's native Tab). Tier-4 item records
  the observation; if it hurts, an ensure-visible seam is a backlog
  candidate, not this chunk.

### 5.2 Mechanism (probe-settled)

- `slint::Window` does **not** expose `focus_next_item` publicly
  (verified: the method lives on crate-private `WindowInner`,
  i-slint-core 1.17.1 `window.rs:1346`; `api.rs` has no public
  wrapper). The runtime's tab traversal runs inside its key handler
  on a received `"\t"` (`window.rs:1131`).
- Therefore: `UiEffect::ControlNext` / `ControlPrev` (class `Nav`)
  arms re-inject the traversal event —
  `window.window().dispatch_event(WindowEvent::KeyPressed { text:
  "\t" })`, wrapped in synthesized Shift down/up for the previous
  direction (the journey driver's exact modifier technique, and the
  `teardown_runtime_drag` synthetic-event precedent).
- **Re-entrancy guard**: Tab is *bound* (standard/vi), so the
  injected `"\t"` re-enters `intercept-key` and would resolve
  `nav.next-control` again — an infinite loop. The adapter grows a
  `walk_reentry: bool` flag: set around the synthetic dispatch;
  `intercept-key` rejects immediately while it holds (the runtime
  then consumes the Tab for traversal itself) and the flag clears
  when the dispatch returns. Pinned: one press = exactly one stop.
- **Probe P1 (stage 1 records, stage 2 first commit)**: on the
  testing backend, dispatching `KeyPressed { "\t" }` moves focus
  between two labelled stops, and the Shift-wrapped variant moves
  backwards. Fallback if P1 fails (recorded): per-view markup walk
  lists — each view reports its stop count and the adapter steps an
  index, with markup-side `focus()` arbitration. Only the probe's
  outcome picks between them; everything else in this plan is
  fallback-neutral.
- Scroll-into-view of the *focused control* is explicitly out (see
  5.1); the **item** walk's `FocusScrollHint` machinery is untouched.

### 5.3 The date picker joins the walk

- New `ContextId::DatePicker` (`"date-picker"`), **non-masking**,
  active exactly while the picker's grid FocusScope holds slint
  focus. The seam is the chunk-1 focus-loss mechanism A (probe-
  recorded, `focus_seam_probe_tests.rs` proves `changed has-focus`
  on the pinned Slint): `date-picker.slint`'s `grid-scope` grows
  `changed has-focus => root.grid-focus-changed(...)`; the adapter
  mirrors a `shell.date_picker_grid: bool`; `KeymapShell::chain`
  prepends `ContextId::DatePicker` while it holds:
  `[DatePicker, ViInsert, Global]` (mode is Insert during the date
  flow). Non-masking is required so Escape still resolves
  `ModeNormal` from `ViInsert` — the editor's clean close (chunk-3
  R-c semantics) survives; the mask test
  (`presets_mask_only_the_modal_contexts`) stays untouched.
- The picker's hardcoded `key-pressed` map (`date-picker.slint:80-111`
  — the last `Key.*` literal block) **dies**; its ten codes become
  registry actions in the `DatePicker` context (§9 table). The
  `picker-key(int)` seam *stays* for the click paths (day cells,
  month/time IconButtons — click ≡ key, not key literals). The
  adapter's `UiEffect::PickerKey(DatePickerKey)` arm performs the
  same `due_picker` mutations the `on_task_date_picker_key` closure
  performs today (the closure body factored into one function both
  call). The review's grep gate: no `Key.*` literals in
  `due-editor.slint` **or** `date-picker.slint`.
- Default-open keeps the field focused (chain has no `DatePicker`) —
  typing-first is untouched. Down-in-field transfers focus to the
  grid (chunk-3 `focus-on-mount`), which now also arms the context.
  Tab into the grid arms it identically — the walk *enters* the
  picker and the preset's keys drive it: the emacs-mode fix F27
  asks for.
- Time steppers stay button-only (today they are button-only; no
  regression, no invented bindings — recorded).

## 6. D26 — scroll actions (R3-F32)

- Rows `view.scroll-down` / `view.scroll-up` (Category::Navigation,
  ViewNav group, `CONTROL_NAV` contexts — the five view contexts,
  never `Global`: Global is chain-visible in insert mode and
  PageUp/PageDown are native text-field motions).
- **Container per view** (adapter resolves, structural):
  - Board (wide): the *focused task's column* ListView pages;
    without a focused task the action is inert (ends hold).
  - Board (compact) / StackDetail: the selected stack's viewport.
  - Today: the focused task's section ListView; inert without
    focus.
  - Settings: the one Flickable.
  - TaskDetail: no container → inert.
- **Mechanism**: a new `ScrollPageHint` global (the
  `FocusScrollHint` pattern): `{ family: int, index: int, page: int,
  nonce: int }`. Families: `0` settings, `1` board column,
  `2` today section, `3` stack/compact viewport. The in-house
  `ListView` gains `public function page(page: int)` scrolling by
  `visible-height - avg-est` (the same runtime average
  `scroll-row-into-view` uses — a page with one row of context),
  clamped to `[−content-height, 0]` on `content-y`; consumers bind
  `page-request`/`page-nonce` filtered on family+index like their
  `request-row` bindings. Settings pages **adapter-side** through
  `SettingsScrollFacts` (the chunk-4 anchor precedent: the facts
  global already carries `content-height`/`viewport-height`/two-way
  `viewport-y`; the adapter writes the clamped target and no markup
  change is needed).
- `page: +1` = scroll-down (content advances), `−1` = up. Journeys
  pin: settings paging reaches the save row and clamps at both
  ends; a board column pages and clamps (hint global + `content-y`
  probe); today sections page independently; TaskDetail dispatch is
  a typed no-op.

## 7. D27 — menu curation (R3-F33/38/39/40)

### 7.1 (a) Re-rank (the view submenu, R3-F38)

New ViewNav group order (rank, action) — "go-to-board/today above
next/prev view", spaced 10, no ties:

| rank | action | rank today |
|---|---|---|
| 50 | `palette.search` | 50 |
| 60 | `view.today` | 90 |
| 70 | `view.inbox` | 100 |
| 80 | `nav.focus-board` | 80 |
| 90 | `nav.show-task` (new) | — |
| 100 | `nav.next-view` | 60 |
| 110 | `nav.prev-view` | 70 |
| 120 | `nav.next-item` | 90 |
| 130 | `nav.prev-item` | 100 |
| 140 | `nav.first-item` | 105 |
| 150 | `nav.last-item` | 115 |
| 160 | `nav.next-control` (new) | — |
| 170 | `nav.prev-control` (new) | — |
| 180 | `view.scroll-down` (new) | — |
| 190 | `view.scroll-up` (new) | — |

Ranks are group-page presentation only (never read by resolution);
the quick-row spacing test filters `menu_group: None` rows and is
unaffected. Board/Stack/Task/Mode group orders unchanged. The
`menu.rs` ViewNav-order pin (T-ordering test) gets the new expected
array.

### 7.2 (b) Preset-irrelevant rows filtered (R3-F39)

- Rule: the **Mode group** renders only when the active keymap binds
  `mode.insert` in `ViNormal` — data-derived, no new plumbing:
  `keymap.bound_contexts(ActionId::ModeInsert).contains(&ViNormal)`
  is true exactly for the vi preset (standard/emacs bind no
  `mode.insert` anywhere; vi binds it in `ViNormal` twice). When
  false, the Mode parent row and its page are omitted at build
  time (`root_page`/group iteration), and `group_page(Mode)`
  returns empty — one filter site in `menu.rs`, applied at page
  build (the design's "the preset is known at build time").
- The group-page emptiness cascade already hides empty parents
  (`root_page` T-5), so the parent row disappears with the page.
- Structural pins: standard/emacs root pages carry no Mode parent;
  vi's does; `group_page(Mode)` empty for the first two.

### 7.3 (c) Dynamic greying (R3-F40)

- Plumbing shape: `menu.rs` page builders take an
  `enabled: &BTreeMap<ActionId, bool>` — adapter-supplied
  *state overrides*; `row()` applies
  `enabled: states.get(&id).copied().unwrap_or(entry.enabled)` —
  static registry `enabled` stays the default for permanently
  disabled rows (D27c's letter). The adapter builds the map
  (`fn entry_states(&self, state: &AppState, flow: &Flow)`) at menu
  open *and* in the models pass while the menu holds, so engine
  syncs re-grey a live menu.
- The enumerated state-dependent set (minimal, honest; everything
  else keeps dispatch-time `NoTarget` machinery):
  - `task.move.cancel` — a move/drag flow is live
    (`Flow::Move | MoveStack | DragMove`).
  - `task.complete` — a focused task exists and is not done.
  - `task.reopen` — a focused task exists and is done
    (`shell.focused_task_done` already exists).
  - `task.move`, `task.edit`, `task.delete` — a focused task exists.
  - `task.title-edit` — a subject task exists.
  - `task.track-start` — a focused task exists and nothing tracks;
    `task.track-stop` — tracking is live (the models pass already
    computes both facts for the chip; threaded into the map).
- `MenuEntryModel.enabled` already renders greyed rows (the
  reply-sourced echo work) — no markup change.
- Structural pins only (§7-R3): the map's values per fixture state;
  a journey asserting the menu row's `enabled` property flips when a
  move flow opens under an open menu. Never label text.

### 7.4 (d) Hamburger chip hover-only (R3-F33)

- `toolbar.slint:65-69` (the always-visible `KbdHint` column) is
  removed; `menu-button` gains `hint-keys: root.menu-hint` — the
  IconButton tooltip already renders `hint-keys` (`TooltipState.hover`,
  `icon-button.slint:317-323`), so the binding hint becomes hover
  text "like every other element". `toolbar-menu-hint` keeps flowing
  (it feeds the tooltip). No geometry change: the toolbar row keeps
  its height.

## 8. D28 — emacs double C-g (R3-F36)

- One preset row: `(ActionId::TaskMoveCancel, ContextId::Global,
  &["Ctrl+g", "Ctrl+g"])` in `EMACS`.
- Mechanism (why it is pure data): the resolver is exact-match-
  first; over the Board context the lone `C-g` still exactly resolves
  `task.move.cancel` (unchanged); everywhere else the first `C-g`
  parks as `Pending` (it is a prefix of both `C-g C-g` and `C-g
  C-/`), and the second `C-g` *exactly resolves* the new row →
  `TaskMoveCancel` dispatches. `C-g Ctrl+/` still resolves
  `app.redo`. Conflict scan (emacs Global): no equality, no prefix
  relation with `C-x C-c`, `C-x u`, `C-g C-/`, `C-x c`, `C-c c`,
  `C-x ]`, `C-x [`, `C-x C-b` — the contracts stay green untouched.
- Bonus (journey): `C-g C-g` cancels a move flow from contexts where
  the lone `C-g` never resolved (Today/TaskDetail) — the convention's
  actual point.
- Tests: a sibling of
  `emacs_c_g_cancels_on_the_board_chain_and_stays_redos_prefix_elsewhere`
  — task chain: `C-g` → `Pending`, `C-g C-g` → `Action(TaskMoveCancel)`,
  `C-g C-/` → `Action(AppRedo)`; board chain: lone `C-g` still
  cancels. Snapshot regen (§11).

## 9. D29 + the full new-row binding table

### 9.1 Registry additions

| id | contexts | category | group | enabled | bound? |
|---|---|---|---|---|---|
| `nav.show-task` | `[Palette]` | Navigation | ViewNav | yes | no (palette Return ≡ `menu.invoke`; recorded §3.2) |
| `nav.next-control` | `CONTROL_NAV` (5 view ctxs) | Navigation | ViewNav | yes | all presets (below) |
| `nav.prev-control` | `CONTROL_NAV` | Navigation | ViewNav | yes | all presets |
| `view.scroll-down` | `CONTROL_NAV` | Navigation | ViewNav | yes | all presets |
| `view.scroll-up` | `CONTROL_NAV` | Navigation | ViewNav | yes | all presets |
| `picker.left` / `picker.right` / `picker.up` / `picker.down` | `[DatePicker]` | Picker (new) | None | yes | all presets |
| `picker.prev-month` / `picker.next-month` | `[DatePicker]` | Picker | None | yes | all presets |
| `picker.month-start` / `picker.month-end` | `[DatePicker]` | Picker | None | yes | all presets |
| `picker.commit` | `[DatePicker]` | Picker | None | yes | all presets |

`Category::Picker` joins the enum (name/section label `"picker"` —
never rendered as a menu section; picker rows are never
chain-visible outside the `DatePicker` context, so they never render
in menus — same masking argument as the `menu.*` rows). Ranks 900
(cursor-family style, page-irrelevant). Every row carries a
description (the registry totality test enforces it).

New UiEffects: `ShowTask`, `ControlNext`, `ControlPrev`,
`ScrollUp`, `ScrollDown`, `PickerKey(DatePickerKey)` (a closed enum
of the ten codes). All classify `EffectClass::Nav` except
`PickerKey` — which is **not** a matrix member: like the menu
family, picker keys are applied before/aside the flow table (the
date flow is Insert; the picker mutates its own state and refreshes;
the existing `on_task_date_picker_key` already bypasses `interpret`).
The pair-table harness stays total without new flow rows — but the
resolver class list must still compile (`ShowTask` joins `Nav`'s
match arms).

### 9.2 The per-preset binding table (conflict-contract checked)

Legend: contexts — `T` Today, `B` Board, `S` StackDetail, `D`
TaskDetail, `G` Settings ("gear"), `DP` DatePicker, `VN` the five
view contexts, `ViN` ViNormal, `Gl` Global. Contracts
(`presets_are_intra_context_conflict_free`,
`presets_are_prefix_shadow_free`) verified per context below.

| action | standard | emacs | vi |
|---|---|---|---|
| `nav.next-control` | `Tab` in VN | `C-f` in T,S,D,G | `Tab` in VN |
| `nav.prev-control` | `Shift+Tab` in VN | `C-b` in T,S,D,G | `Shift+Tab` in VN |
| `view.scroll-down` | `PageDown` in VN | `C-v` in VN | `Ctrl+f` in ViN |
| `view.scroll-up` | `PageUp` in VN | `M-v` in VN | `Ctrl+b` in ViN |
| `picker.left` | `Left` DP | `C-b` DP | `h` DP |
| `picker.right` | `Right` DP | `C-f` DP | `l` DP |
| `picker.up` | `Up` DP | `C-p` DP | `k` DP |
| `picker.down` | `Down` DP | `C-n` DP | `j` DP |
| `picker.prev-month` | `PageUp` DP | `M-v` DP | `Ctrl+b` DP |
| `picker.next-month` | `PageDown` DP | `C-v` DP | `Ctrl+f` DP |
| `picker.month-start` | `Home` DP | `M-<` DP | `g g` DP |
| `picker.month-end` | `End` DP | `M->` DP | `G` DP |
| `picker.commit` | `Return` DP | `Return` DP | `Return` DP |
| `reminder.snooze` (#231) | `Ctrl+Shift+S` Gl | `C-c C-s` Gl | `s` ViN |
| `reminder.mute` (#231) | `Ctrl+Shift+B` Gl | `C-c C-b` Gl | `m` ViN |
| `task.move.cancel` (D28) | — | `C-g C-g` Gl (plus existing rows) | — |

Recorded decisions:

- **emacs `C-f`/`C-b` board preservation** (the design's explicit
  constraint): emacs control-nav binds T,S,D,G — never Board, where
  `C-f`/`C-b` stay `stack.next`/`stack.prev`. The board's controls
  keep native Tab (unbound → Unhandled → reject → traversal), an
  emacs-authentic behaviour. Standard/vi bind control-nav in all five
  contexts (Tab/Shift+Tab are free everywhere today).
- vi scroll rides **ViNormal once**, not five contexts — motions are
  modal (the vi model); ViNormal is absent from insert chains, so
  text is safe. standard/emacs bind per-view-context because their
  Global is chain-visible under insert.
- **emacs picker month keys = `C-v`/`M-v`** (calendar-scroll) —
  intentional shadowing of the Global scroll rows *within the
  DatePicker context* (chain-head priority; different contexts, no
  contract contact), and emacs-authentic (calendar mode).
- **#231 keys**: standard `Ctrl+Shift+S`/`Ctrl+Shift+B` (snooze/
  bell; `Ctrl+Shift+U` avoided — the IBus unicode composer; all
  free in their contexts); emacs joins the existing `C-c` domain
  family (`C-c C-s` snooze, `C-c C-b` bell-balance vs
  `C-c Ctrl+b` board-create — distinct sequences, no prefix
  relation; `C-c C-m` avoided — Ctrl+M is CR-ambiguous); vi bare
  `s`/`m` in ViNormal (free; no family growth). Vi **Global stays
  exactly one row** (the vi-shape contract pins it).
- Dispatch sanity for #231: both verbs dispatch today
  (`router.rs:325/333`) on the one-shot `reminder_target` armed by
  banner/notification row clicks — bindings make the verbs
  key-reachable exactly when a target is armed; without one they
  `NoTarget` (existing typed machinery).

Contract verification summary (per context, the test suite enforces
mechanically): every new sequence is unique intra-context; no new
sequence is a proper prefix of another in the same context (the
two-stroke additions `C-c C-s`/`C-c C-b`/`C-g C-g`/`g g` extend
families whose heads are unbound); `Tab`/`Shift+Tab` are unbound
today in all presets and contexts; `PageUp`/`PageDown`,
`C-v`, `M-v`, `Ctrl+f`, `Ctrl+b` (vi), `s`, `m` are unbound in their
target contexts today.

### 9.3 Coverage gate

`every_preset_covers_the_core_verbs` gains `NavNextControl`,
`NavPrevControl`, `ViewScrollUp`, `ViewScrollDown`,
`ReminderSnooze`, `ReminderMute` (all bound in every preset).
`NavShowTask` and the picker family stay out (uniformly unbound /
uniformly bound in all three respectively — no second-class preset;
the test comments name the recorded decisions).

## 10. Snapshot regen list

Regenerated in the *same commits* as their rows (dispatch §7 rule):

1. `tests/snapshots/keymap_snapshots__registry.snap` — new rows,
   rank edits (§7.1), `Category::Picker`.
2. `tests/snapshots/keymap_snapshots__resolved_preset_keymaps.snap`
   — all new bindings, all three presets.
3. `menu.rs` test expectation arrays (ViewNav order, Mode-group
   presence) — test code, not snapshots.
4. The visual goldens (`tests/goldens/`) — the toolbar band changes
   (switcher; the kbd chip leaves the row); regenerated with
   `INSTA_UPDATE=always cargo nextest run -p taskboard-ui-slint`,
   recorded in the commit message, reviewed by hand.

## 11. Test catalogue (TDD-first)

Unit / state machine (`view_state.rs` tests):

- `show_task_targets_the_tasks_board_stack_and_focus` (cross-board
  fixture — the `two_board_fixture` helper exists).
- `show_task_is_inert_on_a_stale_target`.
- `show_task_switches_to_the_board_from_any_view` (incl. Settings).
- `show_task_lands_on_a_done_task` (board retains done cards).
- `palette_jump_task_arm_matches_show_task` (equivalence pin).
- `switcher_selection_projects_views_and_settings_return`.
- `focus_today_joins_the_leaving_settle` (flow live + FocusToday →
  settled, then runs — pair-table harness row).

Keymap (`keymap/mod.rs` tests):

- `emacs_double_c_g_cancels_and_keeps_redo` (§8).
- Coverage list extended (§9.3); conflict/shadow contracts stay
  green unmodified (they iterate the tables).
- `picker_context_chains_and_never_masks` (chain construction +
  the masks test untouched).

Menu (`menu.rs` tests):

- `view_nav_page_ranks_jumps_above_the_cycle` (§7.1 array).
- `the_mode_group_renders_only_for_vi` (§7.2).
- `state_rows_grey_from_the_adapter_map` (§7.3: cancel/complete/
  reopen per fixture state; static rows unaffected).

Adapter / interpret:

- Pair-table rows: `ShowTask`/`ControlNext`/`ScrollUp` under each
  flow kind (settle-then-run via the resolver; totality preserved —
  `Nav` class).
- `show_task_pushes_the_board_scroll_hint` (global nonce/targets).
- `scroll_hint_targets_the_focused_container` (board column /
  today section / stack viewport / settings facts / task-detail
  none).
- `walk_reentry_rejects_exactly_one_tab` (the guard: one dispatch →
  one traversal).

Journeys (`tests/journeys.rs`):

- `the_switcher_drives_and_tracks_the_view` (§2.3).
- `a_palette_activation_shows_the_task` (§3.4 — the repro pin).
- `the_control_walk_reaches_fields_buttons_and_the_picker` (Tab
  into the grid arms the context; emacs `C-f` moves the cursor a
  day; Escape still closes the editor; standard arrows move the
  cursor).
- `scroll_actions_page_the_focused_area` (settings save-row
  reachability; board column clamp; TaskDetail no-op).
- `the_menu_curates_itself` (Mode group absent on standard; cancel
  move greyed → enabled when a move flow opens under the menu;
  ViewNav order).
- `emacs_double_c_g_cancels_from_the_task_detail` (move flow from
  detail, `C-g C-g`, nothing moved, flow gone).
- Palette geometry: `the_palette_input_row_stays_one_line` (§4 pin;
  zero-rows and 8-rows mounts).

Geometry pins are length/token expressions, never px literals;
assertions are structural (globals, model fields, element sizes) —
no text assertions anywhere (AGENTS §5; §7-R3).

## 12. Probe plan (stage 1 records; stage 2 executes first)

- **P1 — synthetic tab traversal**: host the real AppWindow on the
  testing backend; `dispatch_event(KeyPressed { "\t" })` must move
  focus between two labelled stops; the Shift-wrapped variant moves
  back. Evidence in the stage-2 report. Fallback (only on failure):
  per-view markup walk lists (§5.2) — recorded, not planned in
  detail.
- **P2 — focus seam on the picker grid** (mechanism A on
  `grid-scope`): `changed has-focus` fires on walk-entry and on
  close. The chunk-1 probe harness is the template; risk is low
  (the pattern is proven for TextField), but the picker grid is a
  different element class — probed before the context wiring lands.
- **P3 — ListView page arithmetic**: `page()` on the virtualised
  ListView with the runtime average (the `virtualisation.rs` probe
  hosts the same widget); pins the clamp and the one-row context
  overlap before journeys lean on it.

## 13. Commit slices (stage 2; standalone, rebase-clean)

1. `test(ui-slint): the show-task and switcher state pins` —
   view_state unit tests red first (show_task, switcher projection).
2. `feat(ui-slint): nav.show-task retargets board, stack, focus and
   scroll` — §3 (view_state + adapter hint widening + UiEffect +
   registry row + palette rewire).
3. `feat(ui-slint): the toolbar switcher` — §2 (segmented control in
   toolbar, callbacks, leaving-set addition, goldens regen).
4. `fix(ui-slint): the palette input row keeps its height` — §4.
5. `feat(ui-slint): the control walk` — §5 rows + mechanism + probe
   P1 evidence + re-entry guard test.
6. `feat(ui-slint): the date picker joins the walk and the registry`
   — §5.3 (context, focus seam, key-map removal, picker rows,
   snapshots).
7. `feat(ui-slint): scroll actions page the focused area` — §6
   (ScrollPageHint, ListView::page, settings facts paging).
8. `feat(ui-slint): the menu curates ranks, presets and state` — §7
   (+ snapshot + menu tests).
9. `feat(ui-slint): the emacs double C-g and the snooze/mute
   bindings` — §8 + §9.2 remaining rows + snapshots.
10. `docs(changelog): the navigation, palette & menu round entry` —
    CHANGELOG Unreleased; gates; report.

(Slices 5–7 are independent of 2–4 and may reorder; 6 depends on 5's
walk; 9 is independent.)

## 14. Risks & recorded fallbacks

- **R-A The synthetic-Tab mechanism fails P1** — unlikely (the
  journey driver rides the same path; `window.rs:1131` is the
  runtime's own handler) but load-bearing for control-nav bindings:
  fallback §5.2 recorded; the design's D25 contract (rows bound in
  all presets) survives either mechanism.
- **R-B Re-entrancy**: bound Tab re-entering `intercept-key` is a
  real hang risk — the guard lands in the same commit as the
  mechanism, with a hang-is-a-failure test (§11).
- **R-C `ContextId::DatePicker` leakage**: while the grid is
  focused, picker rows shadow Global (emacs `C-v` scrolls the
  calendar, not the view). Accepted and recorded (§9.2) — the grid
  *is* the focused area. The seam must be exact: a missed
  focus-loss leaves the context armed (symptom: printable `j`
  dead in the field). P2 + a journey pin the exactness; the
  focus-loss seam's once-per-change contract is already proven.
- **R-D Settings paging fights the chunk-4 scale anchor** —
  different trigger paths (facts writes vs the anchor's
  `anchor-requested` one-shot); journeys for both coexist in the
  same file; the anchor's capture/restore is untouched.
- **R-E Menu greying scope creep** — the enumerated set (§7.3) is
  closed; new state rows join the map only with a pin. Dispatch-time
  `NoTarget` remains the machinery for targetless verbs.
- **R-F Rank edits touch every ViewNav expectation** — mechanical,
  snapshot-visible, reviewed by hand; no resolution-path code reads
  `menu_rank`.

## 15. Tier-4 manual checklist (this machine, stage-2 report)

- **Switcher feel**: click both segments from every view incl.
  Settings and compact; the lit segment never disagrees with the
  mounted view; the settings-return segment reads sensibly.
- **Walk feel**: Tab/C-f walks read in reading order on board,
  today, task detail, settings; wrapping feels intentional; focus
  rings always visible (§7.1); walking into the picker grid and
  driving it with each preset's keys; no dead ends after flows.
- **Show-task feel**: palette-Enter on a far board "arrives" —
  the target card visibly scrolled into view, wide and compact.
- **Scroll feel**: PageDown/C-v/Ctrl+f cadence on settings/today/
  board; clamps without bounce; the focused column is the one that
  moves.
- **Menu feel**: greyed rows read as disabled (not hidden);
  preset-irrelevant Mode group truly absent on standard/emacs; the
  hamburger hint appears on hover only.
- **C-g feel**: `C-g C-g` from a pending state aborts with the
  prefix-abort status signal; lone `C-g` on the board still cancels
  instantly.

## 16. Estimated diff

~1.7–2.0k lines: registry/router/presets/menu ≈ 450, view_state/
adapter/shell ≈ 300, slint compositions/widgets ≈ 250, tests +
snapshots ≈ 800–1000. `taskboard-domain`, `taskboard-state`,
`taskboard-storage-sqlite`, `taskboard-sync-nextcloud`,
`taskboard-app`: **empty diffs** (the empty-diff gate).
