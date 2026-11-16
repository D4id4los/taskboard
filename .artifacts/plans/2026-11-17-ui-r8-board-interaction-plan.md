# [Plan] ui-r8 chunk 4 — board interaction: reorder, compact movement, multi-select

- **Date**: 2026-11-17
- **Kind**: TDD-first implementation chunk plan (stage 1 of the
  three-stage dispatch; no code in this commit).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-16-board-interaction-ux-plan.md` (ALL
  sections normative — §2 matrix + rulings R-1..R-4, §3 US-1..US-7
  with their ACs, §4 step tables, §6 W-1..W-7, §7 delta, §8 test
  names) and the dispatch plan
  `.artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md`
  §1/§5. This chunk plan resolves every seam name, test name, and
  open design question the origin left as an assumption; each
  resolution is a recorded decision (D-1..D-12). Deviations from the
  origin at stage 2 need a recorded reason in the implementer's
  report.
- **Branch**: `feat/ui-r8-board-interaction` (off `origin/main`
  e7a720dc — chunk 2's focus-restoration PR landed as e7a720dc).
- **Files**: `crates/taskboard-ui-slint/src/view_state.rs`,
  `src/adapter.rs`, `src/menu.rs`, `src/models.rs`,
  `src/presentation.rs`, `ui/compositions/board-layout.slint`,
  `ui/compositions/board-bar.slint`, `ui/widgets/task-card.slint`,
  `ui/app-window.slint`, the test suite under
  `crates/taskboard-ui-slint/tests/`.

## 0. Reading baseline (the seams as they exist at e7a720dc)

- **Move flow** (`view_state.rs`): `Flow::Move { task, target:
  StackId }` — stack-only aim; `start_move` arms, `cycle_move_target`
  walks the flat cross-board ring (`move_ring` = every live board's
  stacks in `(order, id)` order), `confirm_move` dispatches
  `MoveTask` through `append_move_command` — **the one construction
  site, append-only today**. `Flow::DragMove { task, target }` is the
  pointer twin (`begin_drag_move` / `drag_target` /
  `confirm_drag_move`, same append site). `cancel_move` /
  `settle_by_rule` (move/drag flows cancel) are the escape paths.
- **Drop resolver** (`adapter.rs` ~9154–9240): `board-card-drag-armed`
  resolves (column,row) via `tables.board_task` (stale absorbed),
  `board-drag-hovered` re-asserts the target stack,
  `board-card-dropped` re-asserts then confirms (stale column →
  **settle, never guess** — the round-3 F8 law),
  `board-card-drag-finished` settles whatever survived (the
  `try_borrow_mut` echo law). The slint side is index-only (the r2
  D-E invariant: no column geometry crosses to Rust).
- **Compact leg** (`board-layout.slint` ~787–970): `compact-active`
  breakpoint; the switcher is a segmented row of `FocusScope`s +
  TouchAreas (`switcher-activated`), **no drop targets**; the single
  selected column's cards are `TaskCard`s in a bare ListView (no
  DragArea in compact — the compact viewport has no drag handling
  today; the DropAreas/DragAreas are regular-leg only).
- **Board bar** (`board-bar.slint`): strip + shared `Dropdown`
  (`bar-dd`, flyout-site 1, always mounted, strip-mode visible only
  while `BoardBarFacts.switcher-request`), the roster dropdown
  (`roster-dd`, flyout-site 3, mounted when archived-rows non-empty,
  field-text = the count, `field-enabled` = `archived-enabled`).
  The board bar does **not mount in the compact leg** today (the
  layout's compact branch renders the stack switcher only).
- **Context menu** (`adapter.rs` `open_task_context_menu` ~9328 +
  `menu.rs` `TASK_CONTEXT`): the tracker hide-set precedent (#492 —
  subtract the impossible verb); `extra: &[ActionId]` extends for the
  calling surface.
- **Focus machinery** (chunk 2, merged): the one-shot
  `focus_origin: Option<FocusOrigin>` fact
  (`adapter.rs` ~495, ~1214–1508) + `FocusFacts` global — every
  overlay exit restores the invoking element or its container anchor.
  The board-bar dropdown is census site 1; the context menu rides the
  same funnel. **Composition rule for this chunk**: no new overlay
  bypasses it — the compact dropdown reuses the bar's existing
  site-1 grammar (nothing new to record), the multi menu reuses
  `open_context_menu` (restoration for free). New seams that mount
  controls (gap DropAreas, segment drop targets) are not overlays —
  no origin facts needed.
- **FocusScrollHint** (`ui/focus-scroll.slint`): the keyboard-walk
  scroll hint — `target-a` = column index, `target-b` = row, nonce
  edge-trigger; each ListView filters by its own index pair. This is
  the compact scroll behavior US-4 drops need nothing from (see
  D-3), but US-1's ring walk needs the extension in D-2.
- **Id tables** (`models.rs` `Tables`): `board_ids`,
  `archived_board_ids`, `stack_ids`, `board_tasks` (per-column task
  id vecs) — every seam resolves indices against one pass's tables;
  stale absorbed.
- **Order engine truth** (`command.rs:1225`): `MoveTask` stores
  `order` verbatim (i64; `tasks_in_stack` sorts `(order, id)` asc,
  `command.rs` no-ops a same-(stack,order) write). No
  renormalization — the insertion math is presentation-side and the
  id tie-break keeps collisions deterministic.

## 1. Decisions (binding for stage 2)

### D-1 — The position-slot grammar (US-1)

`Flow::Move` gains the slot as variant data:
`Move { task: TaskId, target: StackId, slot: usize }` where `slot` is
the gap index into `tasks_in_stack(target)` — `0..=len` (gap `i` =
before task `i`, gap `len` = after the last). One flow carries both
aim coordinates; the append-only `append_move_command` splits into:

- `slot_order(state, stack, gap) -> i64` — the insertion-order
  computation, pure presentation math over the engine's sort orders:
  empty stack or the stack's only task → `0`; gap before the first →
  `first.order - 1`; gap after the last → `last.order + 1` (the
  existing `append_order`); between `a` and `b` →
  `a.order + (b.order - a.order) / 2` (i64 midpoint, saturating —
  `i128`-free arithmetic on non-negative orders cannot overflow in
  practice, the saturating form is the documented leniency; the id
  tie-break keeps a degenerate collision deterministic).
- **Slot enumeration** (the walk order): board reading order —
  stacks in `move_ring()` order (unchanged: flat cross-board),
  positions top→bottom within each stack, **N+1 slots per stack**
  (US-1/AC-3: a single-task stack offers exactly two). The flattened
  `(stack, gap)` sequence is computed on demand from the live state
  (never stored — refresh-survival by re-derivation, the ring's
  existing law).
- **Arm**: `start_move` seeds `slot = own gap + 1` (the slot after
  the task's current position in its own stack; a vanished task
  leaves the flow unarmed — the existing `focused_task` guard).
- **Walk**: `cycle_move_target` advances one slot per stroke through
  the flattened sequence (wrapping via the existing
  `wrapped_index`); the ring highlight moves both the target column
  (`move-target` index) and the gap (the indicator row, D-2).
- **Confirm**: `confirm_move` dispatches
  `MoveTask { id, stack: target, order: slot_order(...) }` —
  **in-place confirm (the task's own gap, i.e. the re-armed slot
  never equals the task's occupied position by construction, but a
  walked-back slot that maps to the identical order as the task's
  current one) dispatches the command and the engine's
  same-(stack,order) no-op absorbs it** (`command.rs:1227` returns
  `no_op()`; the flow still ends — typed, no special case
  presentation-side). AC-2's escape: Escape (`TaskMoveCancel`) and
  foreign settles ride the existing `cancel_move` /
  `settle_by_rule` move-arm unchanged.

### D-2 — The gap drop target model + the indicator's data path (US-2, W-1)

- **Slint-side targets** (the r2 D-E invariant holds — indexes only):
  in the regular leg's delegate loop, each card's `drag-zone`
  DragArea gains a full-size child **`gap-zone` DropArea** whose
  `can-drop` computes the gap edge from pointer geometry *inside the
  delegate*: `event.position.y < drag-card.height / 2` → gap
  `task-index`, else gap `task-index + 1`. The existing column
  DropArea's bare-space drop (below the last card) becomes gap
  `len` (append). New callbacks, each with the `// slint: <callback>`
  comment (#782 fold):
  `callback board-gap-hovered(int, int)` — (column, gap), and
  `callback board-gap-dropped(int, int)`.
- **Adapter seams**: `on_board_gap_hovered` resolves (stack, gap)
  through the tables and calls a new
  `view_state.drag_gap(state, stack, gap)` (updates
  `Flow::DragMove`'s target **and** the armed gap — the flow gains
  `gap: usize`, `None` = column-only aim for the legacy seams);
  `on_board_gap_dropped` re-asserts (stack, gap) then confirms via
  the shared construction site. **In-place no-op** (US-2/AC-2): the
  confirmed gap equals the dragged card's own position →
  `confirm_drag_move` dispatches nothing and ends the flow (the
  same-absorb shape the engine's no-op gives the keyboard flow,
  enforced presentation-side so the *gesture* is silent too — a
  release back on the card must not land a ghosting refresh).
  **Staleness** (US-2/S2's failure cell): an unresolvable (column,
  gap) on hover is absorbed silently; on drop it settles the flow
  (the `board-card-dropped` F8 law verbatim). The existing
  `board-card-dropped`/`board-drag-hovered` seams stay (column-level
  aim remains the empty-stack/below-last path) and their drop
  becomes gap `len`.
- **The indicator's data path** (US-2/AC-3, US-5): the aimed
  (column, gap) is model-carried — `StackColumnModel` gains
  `aim-gap: int` (−1 = not aimed) and `move-aim: bool` (D-6); the
  regular leg renders the insertion indicator as a conditional
  hairline row *inside the viewport at the aimed gap* (a delegate
  paints the line when `task-index == column.aim-gap`, plus an
  after-last marker on the bare surface when `aim-gap == len`).
  Geometry stays slint-local (the line is a delegate property, not a
  window-level overlay). Keyboard flow (US-1) drives the same model
  fields — one data path for every aim gesture (US-5/S1's premise).

### D-3 — The compact segment drop (US-3, W-2)

- Each compact switcher segment's TouchArea gains a sibling DropArea
  wrapper (the segment is both a click target and a drop target —
  the DropArea wraps the segment's Rectangle, `can-drop` accepts only
  while a drag is live, keeping clicks untouched). New callbacks with
  the `// slint:` comments: `callback switcher-drop-hovered(int)`,
  `callback switcher-card-dropped(int)`.
- **Highlight**: the hovered segment derives from the existing
  `move-target` property (the segment paint already reads it,
  `board-layout.slint:807-812`) — the adapter's
  `on_switcher_drop_hovered` routes through
  `view_state.drag_target`, so highlight + clear is model-driven
  (AC-1's "highlight clears" = the flow ending).
- **Append + no-op guard**: the adapter's
  `on_switcher_card_dropped` resolves the segment's stack via
  `stack_ids`, then guards: **the resolved stack equals the dragged
  task's own stack → settle the flow, dispatch nothing** (AC-2; the
  same-stack segment is the selected stack's segment in the compact
  one-column leg — R-3's append-only rule makes a same-stack segment
  drop meaningless, so it is a no-op, never an append-to-self).
  Otherwise `confirm_drag_move` dispatches
  `MoveTask { id, stack, order: append_order }` (R-3 verbatim).
  Stale segment index absorbed per the id-table law (settle, never
  guess).
- **Compact drag source**: the compact leg's cards gain the regular
  leg's DragArea wrapper (`allow-move: !root.flow-open`, the ghost
  size capture verbatim) — today the compact viewport has no drag
  handling at all and US-3's story requires the card to lift. The
  arm seam is the same `board-card-drag-armed`; the compact viewport
  keeps `mouse-drag-pan-enabled: false`.
- **US-4 drops' compact scroll note** (the prompt's FocusScrollHint
  question): a compact drop *into* the selected stack needs no seek —
  the dropped card lands in the only rendered column and the refresh
  re-renders it; a drop into a *non-selected* stack removes the card
  from view (nothing to scroll to). The hint machinery serves the
  **keyboard ring walk instead** (US-1): `cycle_move_target` writes
  the existing `FocusScrollHint` (family 1 semantics — `target-a` =
  aimed column, `target-b` = the gap's clamp row: `min(gap, len-1)`,
  nonce re-edge) so the aimed column's viewport seeks the aim point.
  The hint already filters per-ListView (both legs read it); no new
  hint family.

### D-4 — The selection VM's typed shape (US-4/S1, W-4)

- **Home**: `view_state.rs` — a pure, adapter-owned field
  (`ViewState.selection: BoardSelection`), zero logic in `.slint`
  (the dispatch plan's stage-3 trap). Never synced, never journaled
  (R-2); it is presentation state like `focused_task`.
- **Shape**:
  ```rust
  pub struct BoardSelection {
      anchor: Option<TaskId>,          // the range origin
      members: BTreeSet<TaskId>,       // id-order storage; display
                                       // order is derived over board
                                       // reading order at use time
  }
  ```
  Empty = `anchor.is_none()` (the `BTreeSet` follows the anchor —
  the anchor is the invariant carrier). The **anchor order** used by
  every dispatch sequence = board reading order
  (columns in `stack_ids` order, tasks in `tasks_in_stack` order
  within a column) with the **anchor first**, then the remaining
  members in reading order — computed presentation-side at
  sequence-build time via the tables (`board_task`/`stack_ids`),
  member-death pruned against the same pass.
- **The range definition** (Shift+Click): members =
  reading-order span `[min(anchor_idx, clicked_idx),
  max(anchor_idx, clicked_idx)]` over the resolved board. Clicked
  card unresolvable (stale/foreign-surface) → absorbed silently.
  Ctrl+Click toggles the clicked card; on an empty selection it
  becomes the anchor. Plain Click resets (and keeps the existing
  open-on-click posture — the click seam already routes
  `activate_card`; selection uses the modifier gestures only).
  Shift+walk (`NavNextItem`/`NavPrevItem`): the keymap's named-key
  rows carry shift as a real modifier (`stroke.shift`); the
  resolver, when the resolved id is NavNextItem/NavPrevItem and the
  stroke's `shift` flag is set, routes to
  `view_state.extend_selection_walk(direction)` instead of the plain
  focus step (the selection becomes non-empty with the anchor at the
  focused card if it was empty; the walk extends to each newly
  focused card). **Recorded leniency**: a printable-bound walk row
  (j/k — shift folds away into the glyph per `stroke.rs`'s
  printable rule) cannot express Shift+walk; the extension works
  over the named-key rows (arrows) only — filed as a backlog issue
  for a dedicated extend id if the walkthrough wants printable
  parity.
- **Clear paths** (each dispatches nothing): Escape and the settle
  law — `settle_by_rule` gains a selection-clear arm alongside the
  flow settle (the selection is cleared by every foreign settle even
  when no flow level lives); plain Click; board/view change and
  member death ride `reconcile` (prune + clear-on-empty, the
  `focused_task` repair pattern); a surface change (the view
  identity tuple's swap) clears — selection never survives into
  another surface (origin §4 US-4/S1's failure cell).
- **Render fact**: `TaskCardModel` gains `selected: bool` and
  `anchor: bool` (the ring uses the focus tokens without holding
  focus — the card header's existing note); the models pass derives
  them from the selection over this pass's tables.

### D-5 — The modifier-click seam (W-4's slint half)

Slint 1.18's `PointerEvent` carries `modifiers` (verified:
`i-slint-common-1.18.1/builtin_structs.rs:79` — `KeyboardModifiers
{ shift, control, ... }` inside `PointerEvent`), so no keymap change
and no window-global poll is needed. `TaskCard`'s TouchArea stops
binding `clicked => root.clicked()` and instead emits from
`pointer-event` (kind Up, button Left):
- shift or control held → the new
  `callback clicked-modifiers(bool, bool)` (shift, control);
- otherwise the existing `clicked` callback (unchanged semantics for
  the stack-detail/today surfaces that bind it directly; the board
  leg's `task-clicked` seam keeps its plain-click route).
`double-clicked => activated` is untouched (the open posture
preserved). Board-layout passes the card's callback through as
`task-clicked-modifiers(column, task-index, shift, control)` (compact
leg: `effective-selected()` addressing verbatim); the adapter seam
resolves the card via `tables.board_task` (stale absorbed) and routes
to the selection VM. **No double emission**: exactly one of the two
callbacks fires per release (the branch is exhaustive in the
pointer-event handler).

### D-6 — The multi verb sequences (US-4/S2–S4, W-5's dispatch half)

- **Sequences** are built in the adapter
  (`sequence_moves(state, selection, aim) -> Vec<StateCommand>` /
  `sequence_done` / `sequence_scheduled`): one command per selected
  task in **anchor order** (D-4's order), **relative order
  preserved** within the moved set (the set's members are reinserted
  in their existing relative sequence at the aim point — cross-stack
  append: `append_order + i` per member; same-stack gap: the set is
  spliced as a block, members not in the set keep their orders).
  Per-task **absorb** (US-4/S2's failure cell): before dispatching
  each command the sequence re-checks the member against the live
  state (the id-table law); a vanished member's command is dropped
  and the sequence continues — no batch verb, no early abort.
- **Multi-drag**: a drag armed from any selected member arms the
  existing `DragMove` flow for the *set* — the flow's `task` field
  becomes the drag's origin task; the drop resolves the aim
  (regular: gap, D-2; compact: segment, D-3) and the sequence
  dispatches. Single-task selection behaves exactly as today.
- **The multi menu** (US-4/S3): `open_task_context_menu` gains the
  selection branch — when a selection exists and the invoked card is
  a member (non-member invocation opens the single-card menu
  unchanged), the ids come from a new named const slice in
  `menu.rs`, `TASK_CONTEXT_MULTI` (the named-const-slice pattern;
  never a re-pasted row block): **`TaskComplete`, `TaskDelete`** only
  in chunk 4 — single-target-only rows (tracking arms,
  series/habit verbs, open/entries, and chunk 5's quick schedule
  family) are **withheld, not greyed** (C-4; the BF-W-N2
  destructive-last rule is preserved by the row order — delete last).
  The count caption renders in the menu's caption slot (manual
  judgement at walkthrough, never asserted). Chunk 5 builds the menu
  grammar (the applicable-row filter + submenus); this chunk builds
  only the selection branch that hands it the multi row set — the
  composition point W-5 names.
- **The armed multi delete** (US-4/AC-3): `DeleteTarget` gains
  `Tasks(Vec<TaskId>)` (anchor-order snapshot at arm time; a member
  dying mid-arm prunes via `reconcile`, empty → disarm). Confirm
  dispatches one `DeleteTask` per member in anchor order (each a
  journal PreImage — `AppUndo` reverses stepwise, engine truth);
  Escape/Cancel disarm with nothing dispatched (the
  `ConfirmDelete` flow's existing grammar verbatim). From the preset
  strokes (US-4/S4): `ActionId::TaskComplete`/`TaskDelete` with a
  non-empty selection dispatch the same sequences; with an empty
  selection the existing single-target semantics stand, and the
  dispatch-time gate surfaces the typed `NoTarget` where there is no
  target (unchanged).
- **Selection persistence** (origin §4 US-4/S2): the selection
  survives completed sequences (so a follow-up verb reuses it) —
  sequences end, they do not clear; the clear paths stay D-4's.

### D-7 — The move-aim bit's model path (US-5, W-6)

The aim fact is derived once per models pass: the armed
`Flow::Move`/`Flow::DragMove`'s (target, gap) plus the drag hover
resolve to per-column `move-aim: bool` and `aim-gap: int` on
`StackColumnModel`. The count badge's tone flip stays the existing
`move-target` ternary (`board-layout.slint:489`); the badge gains the
labelled state — `Badge` gains an `in property <string>
accessible-label` (empty = the default count label), and the board
leg feeds it the derivation presentation-side: `move-aim` → the
aim-state label, else empty (the bit + the label derivation are the
presentation test's subjects — typed flags, never label text; the
Badge stays a dumb primitive). No aim → no badge carries the bit
(US-5/AC-2 — the derivation is the guard). Display-only row: no
callback, no command — the reason law carries it in the journey-steps
column (origin §6 W-6's shape).

### D-8 — The compact title dropdown's anchor (US-6, W-3)

The compact leg mounts the **existing `BoardBar`** (it currently
mounts in the empty-board page and the regular leg only), passing a
new `in property <int> dropdown-at` override of `0` for the compact
mount → `dropdown-mode` is true → the strip collapses to the shared
Dropdown's field showing the selected board's title; a click opens
the site-1 flyout (the always-mounted `bar-dd` + the
`switcher-request` edge already implemented); a row activation routes
the existing `board-bar-activated` seam (the same route the segment
click uses — stale index absorbed, inert with zero boards). The
keyboard twin (US-6/S3) is free: the r4 US-2/S4 target-free
`BoardSelect` stroke arms `BoardBarFacts.switcher-request`, and the
compact leg now has the mounted instance to receive the edge. The
roster indicator rides along (it mounts when the archive is
non-empty — the same affordance the regular bar shows; no new kiosk
instances, the board surface is desktop-only). The stack switcher
stays the first focusable element below the bar (the bar's segments
are tab stops in strip mode; in dropdown mode the field is the one
stop — the D15 order is the bar's existing grammar).

### D-9 — The roster glyph flag (US-7, W-7)

`BoardBarModel` gains `archived-cue: bool` (true exactly when the
archived set is non-empty — derived presentation-side per pass; the
flag exists so the kiosk-disabled affordance can render the same cue
grey, riding the existing `archived-enabled`), and the roster
Dropdown gains an `in property <bool> field-glyph` rendering the
archive icon beside the count (the icon-button glyph vocabulary; the
recognition cue is hover-independent). Display-only row (no
callback/command — reason in the journey-steps column). Presentation
test on the flag + the model-to-widget projection, never text.

### D-10 — Golden visuals re-baseline scope

The compact board goldens re-baseline (the compact leg gains the
board bar row above the switcher); the regular-leg goldens
re-baseline where the insertion indicator / selection ring can appear
(the goldens' fixture renders no aim and no selection — the idle
render is unchanged, so only the compact bar row drifts). Re-baseline
rides the commits that change the models (`cargo llvm-cov`-independent
— `REGEN_GOLDENS` the existing regen flag), each commit's body citing
the US/W ids.

### D-11 — Focus-contract composition (standing law (e))

No new overlay is introduced. The compact dropdown reuses the bar's
site-1 census entry; the multi menu reuses `open_context_menu`'s
funnel (chunk 2's restore seams apply verbatim); the gap/segment
DropAreas and the selection ring are in-surface controls, not
overlays. The only restoration-adjacent new behavior: opening the
context menu on a selected card (US-4/S3) does not clear the
selection (the menu's exit restores the invoking card per chunk 2 —
the selection then still drives the menu's grammar on reopen).

### D-12 — Census & coverage folding

Census cells (origin §9) ride the code commits citing the ids:
task.move Board evidence (US-1/2/3/4), the **task.select row minted**
with R-1/R-2, board.select compact evidence (US-6), the
stack.count_display move-aim note (closes #823), the board.archive
indicator evidence (closes #834). The gate's step tokens are the §8
test names verbatim (D-13). Every new slint seam carries the
`// slint: <callback>` comment; the pre-framework seams this chunk's
gate rows must resolve (`task-clicked-modifiers`, `board-gap-*`,
`switcher-*-drop`) get the comment at introduction (#782 fold).

## 2. Test catalogue (TDD-first; the §8 names are binding)

### Journey tests (`tests/journeys.rs`, step-ID-named)

| Test | Covers | Asserts (typed outcomes only) |
|---|---|---|
| `us1_s3_keyboard_reorder_commits_the_exact_move` | US-1/S1–S3 | arm (M), walk to a same-stack slot, confirm → exactly one `MoveTask { id, stack, order: slot_order }`; the flow ends |
| `us1_s2_flow_escape_dispatches_nothing` | US-1/S2 escape | arm, stroke, Escape → zero commands, the ring cleared |
| `us2_s2_gap_drop_commits_the_insertion_order` | US-2/S1–S2 | drag armed, gap hover, gap drop → one `MoveTask` with the between-A-and-B order; the in-place release dispatches nothing (AC-2) |
| `us3_s2_compact_segment_drop_moves_the_card` | US-3/S1–S2 | compact arm, segment hover+drop → one append `MoveTask`; the selected segment release dispatches nothing (AC-2) |
| `us4_s1_modifier_clicks_build_the_selection` | US-4/S1 | Ctrl+Click toggle, Shift+Click range (reading order), plain-click reset, Shift-walk extension |
| `us4_s2_selection_drag_moves_all_in_anchor_order` | US-4/S2 | N≥2 selection, gap drop → one `MoveTask` per member, anchor order, relative order kept; a mid-sequence vanished member absorbed |
| `us4_s3_multi_menu_withholds_single_target_rows` | US-4/S3 | the menu opened on a member carries the multi row set (structural: the armed rows' ActionIds), single-target rows absent; Complete → per-task `SetTaskDone`; Delete arm → per-task `DeleteTask` on Confirm, Escape disarms |
| `us4_s4_escape_clears_the_selection_without_dispatch` | US-4/S4 escape + AC-4 | selection, Escape → zero commands, the selection empty; foreign settle same |
| `us6_s2_compact_title_dropdown_switches_board` | US-6/S1–S2 | compact field click opens the flyout, row activation routes `BoardSelect`; Escape/outside closes with nothing dispatched |
| `us6_s3_target_free_stroke_opens_the_compact_flyout` | US-6/S3 | the target-free BoardSelect stroke opens the same flyout in the compact leg; walk + Return routes the same seam |

### Seam tests (adapter unit, in the existing seam-test modules)

- the in-place no-ops (US-2/AC-2, US-3/AC-2) — dispatch-empty typed;
- the stale-frame absorbs: stale (column, gap) hover silent, stale
  drop settles (the F8 twins); stale segment index settles;
- the selection VM's clear paths: Escape, foreign settle, plain
  click, board change, member death (each → empty, nothing
  dispatched);
- the per-task sequences: exact per-task `MoveTask`/`SetTaskDone`/
  `DeleteTask` vectors in anchor order; per-task absorb mid-sequence;
- the armed multi delete's prune (member death mid-arm → the strip
  follows, empty → disarm).

### Fold/presentation tests (`src/presentation.rs` / `models.rs` tests)

- `us1` slot-count grammar (AC-3): a single-task stack enumerates
  exactly two slots; the flattened walk order is stack-canonical,
  top→bottom (pure `ViewState` test);
- the indicator state per hovered gap (US-2/AC-3):
  `aim-gap` per column, −1 when idle;
- `us5_s1_the_move_aim_bit_drives_the_badge_state` — the `move-aim`
  bit + the Badge label derivation (typed: the bit is set exactly on
  the aimed column; empty derivation when idle — AC-2's guard);
- `us7_s1_the_roster_indicator_carries_the_glyph_flag` — the
  `archived-cue` flag true iff the archive is non-empty, projected
  onto the Dropdown's `field-glyph`;
- the multi-menu row-set composition (US-4/AC-2): the member-
  invocation branch returns the `TASK_CONTEXT_MULTI` ids, the
  non-member branch the single ids (structural, `matches!`).

### Regression guards (must not weaken)

- the keymap contract suite and keymap snapshots (no new preset rows
  in this chunk — D-4's shift walk rides `stroke.shift` on the
  existing named-key rows);
- the existing drag journeys (the column-level drop grammar's
  behavior for empty stacks stays; the append site's pin test —
  "one MoveTask construction site" — extends to cover `slot_order`
  as the second construction input, not a second site);
- the chunk-2 focus journeys (the menu and dropdown restoration
  journeys stay green untouched).

### Kiosk gates

None new — the board surface does not mount on kiosk (the census
grammar; origin §8 records the decided absence).

### Manual (walkthrough, never asserted)

The insertion indicator's readability at compact sizes; the multi-menu
count caption's wording.

## 3. Commit sequencing (stage 2; every commit standalone, `feat(ui): …` / `test(ui): …` subjects, census cells ride their code commit)

1. `test(ui): the position-slot move flow grammar` — D-1's
   `slot_order` + `Flow::Move.slot` + the walk, TDD (slot-count fold
   tests + the two us1 journeys first), census task.move Board
   evidence.
2. `feat(ui): the gap drop grammar and the insertion indicator` —
   D-2's seams + indicator model path (us2 journey + seam tests +
   aim-gap fold test), `// slint:` comments.
3. `feat(ui): the compact drag-and-drop onto the stack switcher` —
   D-3 (us3 journey + seam tests + the compact DragArea source).
4. `feat(ui): the board selection view-model and modifier clicks` —
   D-4/D-5 (us4_s1/s4 journeys + the clear-path seam tests + the
   TaskCardModel render fact).
5. `feat(ui): multi-move sequences and the multi-selection menu` —
   D-6 (us4_s2/s3 journeys + sequence seam tests + `TASK_CONTEXT_MULTI`
   + `DeleteTarget::Tasks`), census task.select row minted (R-1/R-2).
6. `feat(ui): the move-aim bit and the labelled count badge` — D-7
   (us5 journeys/fold tests), census stack.count_display note (closes
   #823).
7. `feat(ui): the compact board switcher dropdown and the roster glyph`
   — D-8/D-9 (us6 journeys + the glyph fold test), census
   board.select compact evidence + board.archive evidence (closes
   #834); the compact goldens re-baseline rides this commit.
8. Census/CHANGELOG sweep tail if any cell was missed.

Gate: pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest
run --workspace` after the final commit; `scripts/ux-coverage.sh
.artifacts/plans/2026-11-16-board-interaction-ux-plan.md --gate` every
claimed row FOUND.

## 4. Risks

- **Slint hit-testing of the nested gap DropArea** (a DropArea child
  of the DragArea delegate): the drag runtime designates the deepest
  accepting DropArea — the gap zone must not swallow the card's own
  DragArea arm (the arm fires on `dragging`, not on accept, so the
  risk is low but the compact leg's new DragArea-in-ListView needs a
  probe-style check; the regular leg's DragArea/DropArea nesting is
  the existing pattern).
- **PointerEvent.modifiers**: verified present in slint 1.18's
  builtin structs (evidence cited in D-5); if the runtime fails to
  populate modifiers on a specific backend, the fallback is the
  down-event capture (record in the implementer's report if used).
- **i64 midpoint degeneracy**: repeated same-gap inserts can converge
  orders (id tie-break keeps the rendering deterministic; the
  engine's Deck sync re-bases orders upstream). The saturating
  midpoint is the documented leniency; a renormalization verb is
  out-of-scope (AGENTS non-goal — no new order grammar).
- **Golden churn**: the compact leg's board bar changes the compact
  goldens; the fixture's compact width must render the bar — the
  re-baseline is contained to compact + idle-neutral regular.
- **Selection vs focus interplay**: the walk (focused_task) and the
  selection share the card grid; Shift+walk extends, plain walk
  keeps the selection but moves focus (the origin's US-4/S1 table
  implies plain walk does not clear — recorded as the chosen
  reading; a walkthrough finding may retune it).
