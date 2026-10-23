# [Plan] GUI Review Round 2, Chunk 1 — Board Surface & Stack Width Semantics

- **Date**: 2026-10-23
- **Kind**: chunk plan (dispatch stage 1) — TDD-first implementation
  spec for chunk 1 of the GUI review round 2 design round. No code in
  this commit.
- **Origin** (normative): design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §2 (D1–D8),
  §1.1 (F-A1–F-A11), §10 (testing), §11 (inventory), §12 (slicing),
  §14-R2; dispatch plan
  `.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`
  §1 (execution model) + §2 (chunk 1 scope, acceptance a–j).
- **Status**: PROPOSED — awaiting the stage-2 agent. Branch
  `feat/ui-r2-board-surface`, worktree
  `../taskboard-WT-feat-ui-r2-board-surface`, cut from `main` at
  `72182d3`. `scripts/worktree-doctor.sh` passed (101 G free, floor
  25 G; it suggests removing the already-merged
  `taskboard-WT-feat-domain-adhoc-alarms` worktree — left for its
  owner/§8 hygiene pass, not this chunk's call).
- **Scope**: `taskboard-ui-slint` only. Expected **empty diff** over
  `taskboard-app`, `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud` (reviewers
  assert it). No drag-gesture changes (chunk 2), no new actions or
  context menus (chunk 3), no TextField work (chunk 4), no
  paint/token ladder changes (chunk 7 owns card chrome, tabs, and the
  token ladder — every surface this chunk paints already exists).

## 1. Recorded decisions at a glance

| # | Decision | Mechanism (one line) |
|---|---|---|
| D1 | Widths are floors; every column stretches | `horizontal-stretch: 1` unconditionally; `min-width` keeps the conditional stored-floor arm; probe-recorded solver rule (§3.1) |
| D2 | Floor 96→160 logical px + load-time heal | `widths::reclamp_width` applied inside `ViewState::seed_stack_widths` (the single load point); never at reconcile, never at push |
| D3 | Header = one locked control row | `min-height` == `max-height` == `Theme.space-6 + 2·Theme.space-1` (the D20b board-bar expression, inline); task-list gains `vertical-stretch: 1` |
| D4 | Regular-mode selection paint | header arm keyed on the raw `selected-stack`; model pushes the **true** selection (`−1` when none — the `move-target` convention); compact consumes `max(selected-stack, 0)` |
| D5 | Square plus glyph | `icon-box` square-locked (min==max `space-6` × both axes) and centred; extracted `PlusGlyph` widget shared with D8 |
| D6 | Add-stack tooltip | standard `changed has-hover` trigger on `add-col-hit`; hint = new `stack-create-hint` projection of `stack.create` |
| D7 | Compact hides the regular affordance, inlines its own | regular add-col stays inside the (already `!compact`) branch, unchanged; compact `IconButton` moves **inside** the switcher row (trailing, inline) |
| D8 | Per-column Add Task ghost | trailing row per column (regular + compact); click = `select_column` then `route(ActionId::TaskCreate)` — the keyboard's exact path |
| G  | Goldens **will** be regenerated once | D3/D5/D7/D8 are design-mandated visual changes; recorded deviation from dispatch acceptance (j) — §4 |

## 2. Where each seam changes

Files (all `crates/taskboard-ui-slint/` unless noted):

- `ui/widgets/plus-glyph.slint` — **new** (D5/D8 shared glyph).
- `ui/compositions/board-layout.slint` — D1 stretch line, D3 header +
  task-list, D4 header arm, D5 glyph, D6 trigger, D7 compact button
  move, D8 ghost rows, `effective-selected()`, new
  `stack-create-hint` in-property + `add-task-activated(int)` callback.
- `ui/views/board-view.slint` — thread the new property/callback;
  `BoardModel.selected-stack` doc gains the `−1` convention.
- `ui/app-window.slint` — declare/thread `stack-create-hint` and
  `board-add-task-activated` (the `stack-header-hint` /
  `board-header-activated` patterns, app-window.slint:102/168).
- `src/widths.rs` — floor constant, `reclamp_width`, tests.
- `src/view_state.rs` — heal inside `seed_stack_widths` (+doc).
- `src/models.rs` — `selected_stack` faithful push (`unwrap_or(-1)`),
  `set_stack_create_hint` in `push_chrome`.
- `src/adapter.rs` — `on_board_add_task_clicked` handler in
  `install_board_seams` (reuses `select_column` + `route`).
- `tests/support/driver.rs` — extend the `Element` wrapper with
  `size()`/`absolute_position()` (+ a find-all-by-id/label query for
  repetition instances); dev-only, confined to the churn-isolation
  seam.
- Tests: `tests/journeys.rs`, `src/widths.rs`, `src/view_state.rs`
  inline; `tests/goldens/*.png` (one regen commit).

No theme.slint change: the D3 constant and the glyph size are
expressions over existing spacing tokens (board-bar.slint:31-32
precedent), deliberately not new tokens — chunk 7 owns the token
ladder.

## 3. Decisions in detail

### 3.1 D1 — widths become floors; the stretch rule (probe-recorded)

**Markup**: in the columns row
(`ui/compositions/board-layout.slint:115-128`) the stretch binding
becomes unconditional and the min-width arm is unchanged:

```slint
min-width: column.width-px > 0 * Theme.space-1
    ? column.width-px
    : 0 * Theme.space-1;
horizontal-stretch: 1;
```

The `width-px` model value, `stack_width_px` (models.rs:204-221), and
the `−1.0` sentinel are **unchanged** — the sentinel test
(`stack_width_px_scales_set_columns_and_sends_the_auto_sentinel`,
models.rs:545) and the auto-board journey
(`never_resized_board_pushes_auto_widths_and_no_requests`) pass
verbatim. For a sentinel board the constraint set is byte-identical
to today's (min 0 / stretch 1), so D1 alone drifts no golden.

**Probe — the pinned solver's distribution rule** (Slint `=1.18.1`,
read from the vendored source at
`~/.cargo/registry/src/index.crates.io-*/i-slint-core-1.18.1/layout.rs`):

- `solve_box_layout` (line 1229) clamps every cell's preferred size
  into its bounds first: `pref: preferred.min(max).max(min)` — so a
  column with `min-width: W` enters distribution at
  `pref = max(content_preferred, W)`.
- Under the default `Stretch` alignment (the columns row has no
  `alignment:` — default; board-layout.slint:113) the solver calls
  `grid_internal::layout_items` (line 270), which seeds every item at
  its `pref` and then runs `adjust_items::<Grow>` (line 208):
  surplus is distributed as `grow × stretch` per iteration, where
  `grow = surplus / Σstretch` — **equal stretch factors receive equal
  shares of the surplus**, and the loop re-runs as items hit their
  `max` caps. Items whose `max` is unbounded (our columns) simply
  each get `surplus / N`. The `Shrink` arm is the mirror: items
  shrink proportionally toward their `min`s, which always hold.
- The solver's own unit test (`test_layout_items`, line 293)
  demonstrates exactly our shape: three items, unequal mins
  (100/50/50), equal stretch, container 300 → all equal (100/100/100);
  container 200 → the mins hold (100/50/50).

**Observed rule (recorded)**: *equal `horizontal-stretch: 1` across
min-width-ed columns gives each column `max(content_pref, floor) +
surplus/N` — equal shares, mins never violated.* No fallback to
proportional weights is needed. The stage-2 fill journey (§5, T-4)
re-verifies this on the real window via the new geometry oracle; if
it somehow disagrees, the recorded fallback is
`horizontal-stretch: column.width-px > 0 ? column.width-px / Σ(width-px>0) : 1`
weights — a markup-only change.

Consequences pinned by the rule: a fully-"resized" board still fills
the window (F1); no refresh can render a column below its stored
floor (F12's symptom class — the min constraint is solver-enforced);
the drag seam's press base is now the *rendered* width (floor +
share), so the live-drag journey's model-level assertions still hold
(§5, T-2 keeps passing unchanged) while the *feel* narrows only once
the pointer crosses below the floor+share point — Tier-4 item.

Round-1 probe caution honoured: no `max-width` binding is introduced
anywhere (the sentinel-arm solver perturbation recorded at
board-layout.slint:120-124 was about an explicit `max-width` — D1
adds none).

**Stage-2 recorded deviations D1-b/D1-c (empirical correction of this
section's premise)**: the T-4 fill journey failed against the plan's
markup alone, and the *committed goldens* prove the premise wrong:
today's auto board does **not** fill the window — the columns row sits
at min-content (91/39 px at a 1000 px window). Two compiler facts,
both verified against the pinned 1.18.1 sources and the running
window:

- **D1-b — the conditional leg receives no default fill geometry.**
  The compiler's default-geometry pass early-returns on *repeated*
  elements (`i-slint-compiler-1.18.1/passes/default_geometry.rs`:
  `if elem.borrow().repeated.is_some() { return None; }`), and Slint
  compiles `if` elements as single-iteration repetitions — the
  board-leg `if (...) : VerticalLayout` was never sized to its parent.
  Fix: `width: 100%;` on the regular leg (compact leg untouched).
- **D1-c — content caps the column's growth.** A `Text` child reports
  `max == preferred` in its layout info, and a max-capped item
  receives no growth ("space a max-capped item cannot take stays
  free" — the solver's own comment). Each column's layout max was its
  text's preferred width. Fix: `max-width: root.width;` on the
  column (percent is rejected on `max-width` by the compiler; a
  parent-width bound never over-constrains — a column can never want
  more than the row).

With both, the observed rule of §3.1 holds exactly on the real window:
equal stretch → `max(content_pref, floor)` + equal shares of the
surplus (measured 746.5/689.5 from prefs 96/39.5 at a 1500 px window —
both +650). T-4 asserts fill + floors, not share equality (R-c). The
D1 goldens therefore drift *more* than §4 predicted (the columns
spread out), all attributable to D1-b/D1-c; the regen commit's diff
list records it.

### 3.2 D2 — floor raise + load-time re-clamp

- `STACK_WIDTH_MIN_PX`: 96 → **160** logical px (widths.rs:13). At
  scale 0.5 that is 80 rendered px — the design's readability floor.
  `STACK_WIDTH_DEFAULT_PX` (280) and `STACK_WIDTH_STEP_PX` (32) stay;
  `width_ceiling`'s floor follows the constant automatically, as does
  the drag seam's rendered min
  (`STACK_WIDTH_MIN_PX * scale`, adapter.rs:1348).
- New pure function `widths::reclamp_width(px: u32) -> u32` returning
  `px.max(STACK_WIDTH_MIN_PX)` — named for the load-time heal, the
  widths-module home for the rule.
- **Hook point (recorded decision)**: the heal runs inside
  `ViewState::seed_stack_widths` (view_state.rs:575-577) — the single
  place persisted widths load into the view state
  (`Adapter` mount's uuid→id conversion feeds exactly this,
  adapter.rs:1161-1181). Explicitly **not**:
  - *not* in `reconcile` — the T-4 pin
    (`seed_and_snapshot_round_trip_and_reconcile_leaves_widths_alone`,
    view_state.rs:1820-1844) requires reconcile to leave the map
    alone; that invariant is load-bearing (liveness pruning is the
    app forwarder's write-time job) and stays untouched;
  - *not* at push time — `models::push` stays a dumb ferry, and the
    drag/keyboard seams already clamp new writes.
- The heal never writes config: no `UiRequest` on load (the
  never-persist-on-refresh invariant). Healed values reach the
  overlay only when the user next drags/steps (the full-snapshot
  write path carries the healed map). The app-side overlay writer is
  untouched (empty app diff).
- Unknown boards/stacks in the map are healed too (they reappear
  already-floored — harmless and consistent).

### 3.3 D3 — header height fixed; the task area absorbs surplus

- The column header Rectangle (board-layout.slint:136) gains
  `min-height: Theme.space-6 + 2 * Theme.space-1;` **and**
  `max-height:` the same expression — the D20b one-control-row
  constant, spelled inline exactly as board-bar.slint:31-32 spells it
  (recorded: no new theme token; chunk 7 owns the ladder). `clip:
  true` already present stays.
- The task-list VerticalLayout (board-layout.slint:212) gains
  `vertical-stretch: 1` (its `alignment: start` keeps cards
  top-aligned inside the now-taller cell).
- **Why this fixes F3 (mechanism, from the same solver rule)**:
  today the column's children both have stretch 0, so the
  all-stretch-0 fallback (`grow = surplus / count`, layout.rs:240-244)
  hands a 0-task column's surplus **equally** to header and task-list
  — the header grows tall (F-A6). With the header locked and the list
  at stretch 1, all surplus flows to the list; title rendering is
  identical for 0 and >0 tasks because the header's height no longer
  depends on column content.
- Pin (acceptance c) — geometry oracle, §5 T-5: every `header`
  instance measures exactly the constant, equal for a 0-task and a
  populated column.

### 3.4 D4 — selection paints in regular mode (the `−1` push)

**Mechanism (recorded decision — the load-bearing one):**

1. `models::push_board` (models.rs:270-275) flattens `None` to **`−1`**
   instead of `0` — the same convention `move-target` already uses in
   the same struct (board-view.slint:16). The model now carries the
   *true* selection: `Some(stack)` → index, `None` → `−1`.
2. The **regular** header background arm (board-layout.slint:183-185)
   becomes the compact switcher's selected language with
   move-target precedence:

   ```slint
   background: header-hit.has-hover ? Theme.surface-hover
       : column-index == root.move-target ? Theme.surface-raised
       : column-index == root.selected-stack ? Theme.surface-raised
       : transparent;
   border-width: column-index == root.selected-stack
       ? Theme.focus-ring-width : 0;
   border-color: Semantic.focus-ring;
   ```

   Selected = raised fill **+ focus ring** (the ring is the selection
   marker, per compact 305-311); move-target keeps today's distinct
   arm (raised fill, no ring) and **wins when both apply** — during a
   move flow the target cue replaces the selection cue, and the
   selection ring returns when the flow ends. `clip: true` keeps the
   ring inside the rounded rect.
3. The **compact** leg consumes the field through a new pure function
   `effective-selected() -> int { Math.max(root.selected-stack, 0) }`
   at every current consumption site (switcher paint 306/316/319,
   `selected-column()` 81, task/create callbacks 365-374) — compact's
   behaviour and pixels are identical to today (`−1` at mount renders
   and paints stack 0 exactly as the old `0` fallback did; every
   real selection is `≥ 0` and unaffected).

**Why `−1` and not seeding the selection at reconcile** (the
alternative, rejected with evidence): seeding `None → first stack` in
`reconcile` would keep the model unchanged but changes walk
behaviour — the compact switcher-level walk's `None` arm
(view_state.rs:443-455) is observable today: the existing journey
`the_switcher_level…` (journeys.rs:998-1057) presses `NavNextItem`
from an unselected mount and lands on stack **a** via the `None ⇒
stacks[0]` arm; with a seeded selection the same press would advance
to stack **b** and break the assertions at 1019-1023. `−1` changes
no view-state behaviour at all — only the model flattening — at the
cost of **one** recorded test edit (§6, E-1). It also keeps the
regular goldens free of a mount-time painted column-0 header (the
goldens drift only from D3/D5/D7/D8, §4).

Acceptance (d)'s "driven by the already-pushed selected-stack, no new
state" holds: the field, its type, and its source are unchanged; only
the flattening policy of `None` became faithful.

### 3.5 D5 — the plus glyph is square

- New L1 widget `ui/widgets/plus-glyph.slint` (SPDX AGPL-3.0-only):
  a Rectangle locked `min-width`/`max-width`/`min-height`/
  `max-height` = `Theme.space-6` carrying the two crossing bars at
  `Theme.focus-ring-width`, colour via an in-property — the extracted
  add-col glyph (board-layout.slint:253-272), square-locked the
  IconButton way (icon-button.slint:43-47 — but both axes capped;
  IconButton caps only width + min-height because its parent row
  bounds it, the add-col's parent does not).
- The add-col `icon-box` becomes a centred `PlusGlyph`:
  `x/y: (parent − self)/2`, locked square. The affordance stays a
  full-height hit target (the enclosing Rectangle and `add-col-hit`
  are untouched — hover surface, a11y FocusScope, width
  `space-6 + space-2`).
- `IconButton`'s inline plus (icon-button.slint:173-188) is **not**
  refactored onto `PlusGlyph` — its square comes from the button's own
  constraints and touching the shared widget in this chunk is risk
  without a finding (recorded; unification can ride a later chunk
  that has a reason to touch IconButton).
- Pin: geometry oracle — the glyph element measures `space-6 ×
  space-6` at scale 1 (§5 T-6).

### 3.6 D6 — the add-stack tooltip

- `add-col-hit` (board-layout.slint:242-244) adopts the standard
  trigger verbatim (icon-button.slint:196-202 /
  board-layout.slint:146-155):

  ```slint
  changed has-hover => {
      if (self.has-hover) {
          TooltipState.hover("New stack", "Adds a new stack to this board.",
              root.stack-create-hint,
              self.absolute-position.x, self.absolute-position.y,
              self.width, self.height);
      } else {
          TooltipState.unhover();
      }
  }
  ```

  The description string is an inline markup literal, consistent with
  every existing label/description in this file and board-bar's
  "Switches to this board." (board-bar.slint:86-90) — the chunk-3
  presentation-constants rule governs the registry surfaces it adds,
  not this file's existing inline-string practice (recorded).
- Hint source: new window property `stack-create-hint` (the
  `stack-header-hint` pattern, app-window.slint:102 → board-view →
  board-layout), pushed in `push_chrome` (models.rs:195-201
  neighbourhood) as `first_binding_display(ActionId::StackCreate,
  chain)`. Empty renders as today (emacs's `stack.create` gap is
  chunk 3's F-E1 fold-in; the tooltip's empty-hint rendering is
  established R1-D21b behaviour).
- Pins: the model-side projection (§5 T-7 — non-empty under the
  standard preset, empty under emacs; structural emptiness, no text
  assertions). The hover trigger wiring itself is not
  mouse-drivable headless (usability drives `TooltipState` directly,
  usability_components.rs:144-160) — markup review + Tier-4 item.

### 3.7 D7 — compact mounts exactly one add-stack entry, inline

- The regular full-height add-col already unmounts in compact: it
  lives inside the `stacks.length > 0 && !compact-active` branch
  (board-layout.slint:101). D7's regular clause is affirmed, not
  changed (recorded — no edit needed).
- The compact `IconButton` (board-layout.slint:345-349) moves from
  its own row **into** the switcher `HorizontalLayout` (295) as a
  trailing inline item — F23's "no own row at the smallest viewport".
  Same `icon`/`label`/`clicked` wiring; the journey
  `the_compact_add_stack_affordance_opens_the_flow`
  (journeys.rs:1169) passes unchanged (find-by-label and the flow are
  position-independent). The stack-create TextField stays below the
  switcher (an input row, not a button row).
- Pin: §5 T-8 — at 600 px the regular add-col element is absent and
  exactly one "New stack" accessible entry exists; at 1000 px the
  add-col element is present.

### 3.8 D8 — the per-column Add Task ghost

- **Anatomy** (regular leg, after the create-open TextField at
  board-layout.slint:225-230; compact leg likewise after 370-375 —
  the compact ghost addresses `effective-selected()`):

  ```slint
  add-task := Rectangle {
      min-height: Theme.space-6 + 2 * Theme.space-1;
      max-height: Theme.space-6 + 2 * Theme.space-1;
      border-radius: Theme.radius-small;
      background: add-task-hit.has-hover ? Theme.surface-hover : transparent;
      add-task-hit := TouchArea {
          clicked => root.add-task-activated(column-index);
      }
      // FocusScope overlay: accessible button "Add task", real tab
      // stop, default action = the same callback (the add-col
      // pattern, board-layout.slint:245-252).
      PlusGlyph { … centred … }
  }
  ```

  One control row tall (the D3/D20b rhythm — recorded choice; the
  design fixes the visual language, not the height), the add-col's
  hover surface + square plus + radius, label "Add task" for the a11y
  tree. **No tooltip** in this chunk — D8's text names label and
  click only (recorded; a hint can ride chunk 3's hint work if the
  user asks).
- **Exact dispatch path (named functions)**: new BoardLayout callback
  `add-task-activated(int)` → board-view threads → app-window
  callback `board-add-task-activated(int)` → new handler in
  `install_board_seams` (adapter.rs:1577):

  ```rust
  window.on_board_add_task_clicked(move |column| {
      let mut a = add_task.borrow_mut();
      select_column(&mut a, &window, column);      // existing: tables → select_stack → apply
      a.route(&window, ActionId::TaskCreate);       // the keyboard's exact action
  });
  ```

  (`select_column`, adapter.rs:1657; `route`, the same dispatch the
  keyboard `task.create` takes; the `on_board_new_stack_clicked`
  handler at 1554-1559 is the template.) After `select_column`, the
  view state holds `selected_stack = clicked`, `focused_task` = that
  stack's first task, so `create_target_stack` (Board arm,
  view_state.rs:553-558) resolves to the clicked stack — the
  `BeginCreate` interpret arm (adapter.rs:453-458) opens
  `begin_create(clicked)` exactly as the keyboard would in the same
  state. Empty stacks: `select_stack` leaves focus `None`, the
  `or(self.selected_stack)` arm anchors the flow — F6's affordance
  works on empty stacks. Click ≡ key holds by construction (one
  action, one target-resolution rule); the journey (§5 T-9) proves it
  at the boundary.

## 4. Decision G — the goldens regenerate once (recorded deviation)

Dispatch acceptance (j) says this chunk must not need golden regen;
design §10/D37 reserves regen for chunk 7. **Both cannot hold**: D3,
D5, D7, D8 are design-mandated *visual* changes to the board surface
(that is their point — F2/F3/F6/F23 are visual findings), and
`fixture::golden_state` (unequal columns: 2 tasks vs 1) guarantees
drift:

- **D3** trips first and alone: today the shorter column's surplus
  splits equally between its header and task-list, so stack b's title
  renders vertically centred in a taller header; locked, the title
  moves up by the surplus share — text pixels cross many luma cells,
  far past the 8/255 per-cell budget.
- **D5** replaces a near-full-height bar with a 32 px glyph in the
  trailing column of every regular/expansive golden; **D7** moves the
  compact button inline (compact goldens); **D8** adds a ghost row to
  every column.
- D1/D2/D4/D6 are constructed to drift **nothing** (sentinel
  constraint sets identical; `−1` keeps mount-state pixels identical
  in both modes; tooltips don't render at rest) — the regen diff must
  show changes attributable to D3/D5/D7/D8 only, and stage-2 verifies
  that attribution while reviewing the PNG diff.

**Decision**: run the golden suite unchanged first (the failure output
is the evidence); then regenerate **once**, in a single dedicated
commit (`chore(ui): regenerate board goldens for the r2 board-surface
chunk` + body listing each visual diff), mirroring round-1's
in-round precedent `9a147b6` ("regenerate board goldens for the bar +
the changelog line"). This is the recorded deviation from (j)/§10-D37,
reason: D3/D5/D7/D8 are in-scope design mandates, not chunk-7 paint
leakage; deferring them would split their TDD journeys from their
implementation and push F2/F3/F6/F23 fixes out of the chunk that owns
them. Chunk 7's D37 regen then re-baselines again over the card/tab
ladder — expected and harmless.

## 5. Test catalogue (TDD order)

Stage-2 writes each test **first** (red where a production change is
its subject), then implements. Harness prerequisite (T-0) lands as
its own commit first: `tests/support/driver.rs`'s `Element` wrapper
gains `size() -> LogicalSize`, `absolute_position() -> LogicalPosition`,
and a find-all query (by element id and by accessible label) over the
pinned testing backend's `ElementHandle` (search_api.rs:876-893 —
`size()`/`absolute_position()` exist in `=1.18.1`; this *supersedes*
round-1 F-B2's "no element geometry" note for everything except
pointer-event synthesis, which chunk 2 still owns). driver.rs is the
designed churn seam — the only file naming the backend.

| # | Layer | Test | Proves |
|---|---|---|---|
| T-0 | harness | driver geometry passthrough compiles + one smoke read | the oracle exists (dev-only) |
| T-1 | widths unit | `clamp_width_saturates_at_both_bounds` (existing, unchanged); `width_ceiling_subtracts_others_and_chrome` — the `1000 − 192 − 80` literal becomes `1000 − 2·STACK_WIDTH_MIN_PX − 80` (**recorded edit E-2**) | the ceiling follows the raised floor |
| T-2 | widths proptest | `reclamp_width_is_idempotent_monotone_and_floored` (px ∈ 0..5000: result ≥ 160, idempotent, identity above the floor) | D2's heal math |
| T-3 | view_state | `seed_stack_widths_heals_below_floor_entries` (a 96-px entry seeds as 160; a 240-px entry untouched); existing T-4 reconcile-leaves-alone test green **unchanged** | the hook point is seed, not reconcile |
| T-4 | journey | `stored_widths_fill_the_window_as_floors` — journey_state, drag column 0 to a narrow floor, release, `set_window_size(1500, …)`, then via `match_id("column-body")` sizes: every column ≥ its floor·scale and Σ(sizes + gaps + add-col) == window width (±1 px rounding) | D1 fill + floors (the on-window stretch-rule confirmation; fallback per §3.1 if red) |
| T-5 | journey | `stack_headers_are_one_control_row_regardless_of_content` — fixture with a 0-task stack beside a populated one; every `header` instance measures exactly `space-6 + 2·space-1` at scale 1 | D3 (acceptance c) |
| T-6 | journey | `the_add_stack_glyph_is_square` — glyph element `space-6 × space-6`; the add-col element remains full column height | D5 |
| T-7 | journey/models | `the_add_stack_button_carries_the_stack_create_hint` — `get_stack_create_hint()` non-empty under standard, empty under emacs (structural emptiness only) | D6's hint source |
| T-8 | journey | `compact_mounts_exactly_one_add_stack_entry` — at 600 px: no add-col element, exactly one "New stack" accessible entry; at 1000 px: add-col present | D7 (acceptance g) |
| T-9 | journey | `the_add_task_ghost_is_the_click_equivalent_of_task_create` (the journeys.rs:1193 template): click column 2's "Add task" → create row opens in column 2 (`create-open` in the model) → type + Return → `CreateTask { stack: b, … }` at the boundary; keyboard run from the same selection state lands the identical command; empty-stack variant (0-task stack's ghost opens the flow anchored there); compact variant (ghost addresses the effective-selected stack) | D8 (acceptance h) |
| T-10 | journey | `mount_pushes_no_selection_and_a_header_click_moves_it` — fresh mount: `board_model.selected_stack == −1`; `click_by_label(<stack title>)` → index; (the existing 1007 assertion becomes this test's mount half — recorded edit E-1) | D4's model semantics |
| T-11 | goldens | run unchanged (expect drift, capture evidence) → one regen commit with the reviewed diff list (§4) | decision G |

Existing tests that must pass **verbatim** (checked against every
decision): `never_resized_board_pushes_auto_widths_and_no_requests`
(sentinel unchanged), `drag_seam_resizes_live_and_persists_on_release`
(model-level; the press base semantic is additive),
`keyboard_resize_steps_clamps_and_requests_persistence` (except
recorded edit E-3), `the_compact_add_stack_affordance_opens_the_flow`,
`stack_create_has_a_visible_click_equal_flow`,
`stack_width_px_scales_set_columns_and_sends_the_auto_sentinel`,
`resize_focused_stack_seeds_steps_and_clamps` (constant-relative
already), all view_state/settings journeys, usability_components,
keymap snapshots (no keymap change — no snapshot regen).

## 6. Recorded test edits (design-mandated, pre-recorded per the stage-2 rule)

- **E-1** — journeys.rs:1007-1011: mount-time
  `selected_stack == 0` → `== −1`, message updated to say no stack is
  explicitly selected at mount (compact still *renders* the first).
  Reason: D4's faithful push (§3.4); the old value pinned the `0`
  flattening artifact, and the alternative (seeding at reconcile)
  breaks the switcher-walk assertions at 1019-1023. Not a weakening —
  the semantics changed by design.
- **E-2** — widths.rs `width_ceiling_subtracts_others_and_chrome`:
  derived literal `1000 − 192 − 80` → arithmetic over
  `STACK_WIDTH_MIN_PX` (unpins stale 2·96 arithmetic; the relation was
  always the contract).
- **E-3** — journeys.rs:1286-1293 (keyboard resize): expected
  `.max(96)` → `.max(STACK_WIDTH_MIN_PX)` (imported constant). Reason:
  D2's normative floor raise; the journey then pins the relation
  rather than a number.

No other existing test changes. Any additional edit stage-2 discovers
a need for is a STOP-and-record event, not an in-flight tweak.

## 7. Implementation order (every commit standalone)

1. `test(ui-slint): journey-driver geometry reads over the testing backend` (T-0).
2. `feat(ui-slint): stack widths become stretch floors` — D1 markup + T-4 (+ E-3 rides the D2 commit instead if red-order demands; keep E-3 with D2).
3. `feat(ui-slint): raise the stack width floor and heal loads` — D2 + T-1/T-2/T-3 + E-2/E-3.
4. `feat(ui-slint): fixed-height stack headers` — D3 + T-5.
5. `feat(ui-slint): paint the selected stack in regular mode` — D4 (models `−1`, markup arm, `effective-selected`, board-view doc) + T-10 + E-1.
6. `feat(ui-slint): square plus glyph widget` — D5 (`plus-glyph.slint` + add-col) + T-6.
7. `feat(ui-slint): tooltip and inline placement for new-stack` — D6 + D7 + T-7/T-8.
8. `feat(ui-slint): per-column add-task ghost rows` — D8 (markup + callback threading + adapter handler) + T-9.
9. `chore(ui): regenerate board goldens for the r2 board-surface chunk` — decision G, diff list in the body.
10. CHANGELOG.md Unreleased entries ride the feature commits that
    cause them (width fill semantics + floor raise; fixed header
    height; selection highlight; square glyph + tooltip; inline
    compact new-stack; Add Task buttons).

## 8. Tier-4 manual checklist (for the PR body / the user)

1. **Header-height feel**: a 0-task stack's title sits at the same
   height as every other stack's; nothing stretches when a stack
   empties.
2. **Ghost-row feel**: every stack (empty ones too) ends in an Add
   Task row; hover lifts it; click opens the create row anchored to
   that stack; typing + Return creates the task there.
3. **Fill-the-window feel**: on a wide monitor, a board whose columns
   were all narrowed still consumes the full window width; no column
   ever renders narrower than its stored floor; dragging still tracks
   the pointer live (narrowing bites only below the floor+share
   point).
4. **Selection feel**: clicking a stack header (including a 0-task
   stack's) shows the raised + ring selection; during a move the
   target highlight replaces it and the selection returns after.
5. **Compact feel**: at the smallest viewport the New stack button
   sits inline at the end of the stack switcher (no own row); the
   plus glyph reads as a plus everywhere.
6. **Tooltip feel**: hovering the trailing add-column shows "New
   stack" + the binding hint (standard/vi; emacs shows none until
   chunk 3 binds it).

## 9. Risks

- **R-a (goldens)**: the §4 deviation may draw stage-3 fire despite
  the recording; mitigation is the evidence-first run (unchanged
  goldens fail *before* regen) and the D1/D2/D4/D6 no-drift
  attribution check inside the reviewed PNG diff.
- **R-b (−1 semantics)**: the compact leg's six consumption sites
  must all move to `effective-selected()`; a missed site breaks at
  mount only (−1 index) — T-9's compact variant and the unchanged
  compact journeys cover it; the regular goldens' mount state stays
  unpainted by construction.
- **R-c (solver rounding)**: the fill journey's Σ-width assertion
  allows ±1 px (physical rounding, not a quality ceiling); share
  *equality* is deliberately not pinned (fallback discriminator only,
  §3.1).
- **R-d (geometry API pinned to the backend version)**: confined to
  driver.rs, the designed churn seam; a Slint bump edits one file.
- **R-e (drag feel under floors)**: behaviour is model-identical but
  visually bites later; covered as Tier-4 item 3, and chunk 2's D11
  drag work re-tests the gesture seam on top of this chunk anyway.
- **R-f (ghost rows inflate compact column height)**: the compact
   single column gains one row; bounded (one control row) and covered
   by the compact golden regen review.

## 10. Out of scope (guardrails for stage 2/3)

No `theme.slint` edits, no app-crate or domain-crate diffs, no
keymap/preset/snapshot changes, no model-identity or refresh work
(chunk 2's D9 — `push_board` keeps building fresh VecModels per
refresh; D4's `−1` and D6's hint push ride the existing wholesale
push), no context menus or new actions (chunk 3), no TextField or
ghost-row checkmark (chunk 4), no card/tab chrome (chunk 7), no
quick-capture surface (#347 — D8 is per-stack Add Task only). Goldens
regenerate exactly once, in the dedicated commit.
