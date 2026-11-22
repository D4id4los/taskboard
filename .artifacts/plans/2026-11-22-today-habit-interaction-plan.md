# Today Habit Interaction (#1198, #1200) — Chunk A1 Implementation Plan

- **Date**: 2026-11-22
- **Kind**: dispatch chunk plan (stage 1), diagnosis-first, TDD
- **Branch**: `fix/today-habit-interaction` (worktree
  `../taskboard-WT-fix-today-habit-interaction`, cut off `origin/main`
  0946b220)
- **Normative sources**: scope report
  `.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`
  (#1198 p1, #1200 p2 rows); dispatch plan
  `.artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md`
  §1/§2 (chunk A1, acceptance + traps); issues #1198, #1200 (verbatim
  diagnosis-first mandates); the landed r9 base —
  `.artifacts/plans/2026-11-19-habit-keyboard-parity-plan.md` (H3:
  D4 keyboard reorder, D5 drag gap-drop) and
  `.artifacts/plans/2026-11-18-habit-parity-ux-plan.md` (H2 row
  rework; US-4 AC-1, US-3 AC-2).
- **Round laws honoured here**: typed outcomes only (never text
  assertions); diagnosis-first — every survey seam was read, and the
  survey's suspected legs are REFUTED (see §2.0); out-of-scope
  findings became backlog issues (§7).

## 1. Scope

Close #1198 (habit drag-n-drop and context-menu Move up/down never
move a row) and #1200 (mouse click cannot select a habit row;
keyboard can). Both fixes compose with the r9 keyboard reorder pair
and the task-row DnD/multi-select grammars, which must stay
unregressed.

Non-goals: the task compact-move pair's own slot geometry (systemic
finding — backlog issue §7-B1); the board's task drag-drop midpoint
tie-lottery (same issue); CLI `habits update --order` (absolute-order
`MoveHabit` semantics preserved verbatim); kiosk posture (HAB-14
gates survive untouched); D3a's habit row redesign.

## 2. Diagnosis (verified against the worktree at 0946b220)

### 2.0 The survey's suspected seams are REFUTED — the dispatch chain is intact end-to-end

Every leg the survey (and both issues) suspected was read and
exercises correctly:

- **Slint wiring**: `habits.slint:226` drag-zone (DragArea,
  `allow-move: root.editing-enabled`, non-empty
  `DragState.payload`) and `:264` gap-zone (DropArea,
  `drop-asked(row, gap)`) → `today-view.slint:244` HabitsSection
  forwards → `app-window.slint:675/681` (`habits-row-activated`,
  `habits-drop-asked`) → adapter seams
  (`adapter.rs:11727 on_habits_row_activated`,
  `:11748 on_habits_drop_asked`). Same DragArea+child-DropArea shape
  as the working board delegate (`board-layout.slint:561/618`).
- **Adapter → router**: the drop seam mints `a.habit_move` and routes
  `ActionId::HabitMove`; `route_consumes` declares
  `HABIT_MOVE`/`HABIT_MOVE_UP`/`HABIT_MOVE_DOWN` (`router.rs:1388–
  1390`); `route()` copies the one-shots into the ctx
  (`adapter.rs:1815–1822`); the router arms dispatch
  `StateCommand::MoveHabit` (`router.rs:1635–1656`).
- **Menu**: `HABIT_CONTEXT` carries the pair (`menu.rs:454–455`);
  `habit_menu_entries` pre-mints both slots at menu open
  (`adapter.rs:7359–7366`).
- **Engine**: `StateCommand::MoveHabit` applies — live-habit guard,
  `order` equality no-op, `clocks.order` stamp, `UpsertHabit` +
  `board_flush` (`command.rs:2927–2945`). The models pass re-sorts by
  `(order, id)` (`live_habits_in_order`, `presentation.rs:7802`), so
  an applied move would re-render.

An unhandled ActionId, a dropped event, a settle cancellation, and a
silent engine rejection are all ruled out **for the menu path
verified statically**. The real mechanisms are below.

### 2.1 #1198 — the exact mechanism: the order mint cannot express a move over dense integer orders

Habit creation appends `append_order_after` = max live order + 1
(`habit.rs:812`, `command.rs:10641`) → the section's live orders are
**dense integers** (0, 1, 2, …). Every UI reorder path mints an
absolute order through one shared arithmetic —
`Adapter::habit_move_slots` (`adapter.rs:7383`, used by the menu open
AND the keyboard walk's pre-mint) and `on_habits_drop_asked`
(`adapter.rs:11748`) — via `habit_drop_order` →
`floor_midpoint_order(prev, next)` (`command.rs:748`).

Two failure arms, both deterministic on a dense list:

1. **Down / drop-below**: the slot is the gap at `position + 1`, its
   bracket is (mover's own order, lower neighbour's order). Floor
   midpoint of a width-1 interval = the mover's **own** order → the
   engine's `habit.order == order` branch returns `no_op()`
   (`command.rs:2931`). The move never lands. (Geometrically: the
   slot minted IS the gap the mover already sits at the bottom edge
   of.)
2. **Up / drop-above**: the slot bracket is (upper neighbour, mover)
   → midpoint floors to the **upper neighbour's occupied order**. The
   mover is written with an order it does not own; the canonical
   `(order, id)` sort (`live_habits_in_order`) breaks the tie by
   UUIDv7 mint order (`idgen.rs:85`, `now_v7`) — the mover stays put
   whenever it was created after the neighbour (the common case), and
   the section now carries a duplicate order.

With no free integer anywhere in a dense list, **every** slot is
either the mover's own order (no-op) or an occupied order (id-sort
lottery). That is why the row "never moves regardless of where it is
released" (#1198) and both context-menu directions are no-ops. The
keyboard reorder pair (H3 D4) shares the mint and is broken the same
way. The engine's own `TaskMoveUp`/`TaskMoveDown` arm knows this
class — its width-1 pin comment (`command.rs:1384–1396`) — but pins
to `task.order`, i.e. the accepted no-op, and never crosses the
neighbour either (systemic finding, §7-B1; its US-14 fixture
`us14_s1_task_move_up_resolves_the_floor_midpoint_slot` — mover 10 →
5 with the sibling at 0 — shows the mover remaining BEHIND the
sibling in canonical order).

### 2.2 #1200 — the exact mechanism: the click seam mints the wrong selection truth

The habit row's visual selection is the card emphasis ring, pushed by
the models pass from the **ViewState cursor**:
`emphasis: vs.focused_habit() == Some(row.habit)` (`models.rs:942`).
The keyboard walk writes that cursor (`view_state.rs:2354
focus_today_row`, exclusivity law) — which is why keyboard selection
works. The pointer seam `on_habits_row_activated`
(`adapter.rs:11727`) writes ONLY the adapter's menu-subject field
`a.focused_habit` (`adapter.rs:11734`) — never the ViewState cursor,
no `apply_outcomes`/`finish_mutation` refresh. The click mints the
menu verbs' subject but paints no selection: exactly the reported
"keyboard works, pointer does not".

The task-row precedent is the fix's shape: the task pointer path
settles the pointer conflict, seats the ViewState cursor
(`view_state::activate_today_row` → `activate_card`/`set_focused`),
and applies the outcomes (`adapter.rs:13333`, `:13402 focus_today_row`
— "click ≡ key, §7.1"). The habit seam has no ViewState twin for
habits (`activate_today_row` clears `focused_habit` and only focuses
tasks). Secondary consequence of the same defect: after a keyboard
landing on habit B, a click on habit A leaves the walk cursor on B —
one click, two selection truths (the stage-3 trap (c)).

## 3. The fixes (decisions recorded)

### D1 (#1198): engine-side reorder with section renumber — `StateCommand::HabitReorder`

The mint-side arithmetic cannot be patched (no free integers exist to
mint into; the `(order, id)` tiebreak is mint-order luck). Decision:
**the engine resolves and expresses the move**, the UI mints a target
index only.

- **New domain command** `StateCommand::HabitReorder { habit:
  HabitId, target: usize }` — `target` is a **gap index into the
  section's current live sequence** (the slint gap grammar verbatim,
  `0..=len`).
- Engine resolution: collect the board-scoped live habits in
  canonical `(order, id)` order (the same row source
  `live_habits_in_order` renders from — one row source, one truth);
  clamp `target` to `0..=len`; if the clamped target is the mover's
  own position or position+1 (the in-place gaps) → **accepted
  `no_op`** (the board's in-place-drop precedent, `confirm_drag_move`
  `view_state.rs:4237`); otherwise remove the mover, compute the
  insertion point `insert_at = if target <= position { target } else
  { target - 1 }`, insert, and **renumber the section's live rows
  `order = index (0..n)`** — every row whose order changed is
  upserted with `clocks.order = now` + the board's `board_flush`.
  Unknown habit → `UnknownHabit`; tombstoned → `HabitDeleted` (the
  `MoveHabit` guards verbatim).
- **Renumber-everything is the recorded decision** (alternative
  rejected: on-demand re-spacing/sparse heuristics = a second
  arithmetic and the collision class again). Habit sections are small
  (dozens); the LWW cost is bounded and deterministic, and the section
  stays gap-free forever, which is what makes every future slot
  expressible.
- **Undo**: `HabitReorder` gets its own inverse class recording the
  section's pre-move orders (every touched row) and restoring them as
  one journaled inverse step (a `MoveHabit` batch is acceptable —
  each stamps its own clock). Requirement pin: move, undo, the
  section's sequence equals the pre-move sequence; the existing
  `InverseClass::MoveHabit` single-pre-image shape cannot express it
  and stays for `MoveHabit` (the CLI's verb) untouched.
- **UI call sites** (all within the named seams; `actions.rs` and
  `menu.rs` ids unchanged):
  - `habit_move_slots` reworked to mint **target indices**: up →
    `position.checked_sub(1)` (None at the top — the one-shot stays
    unset → the existing typed `NoTarget` refusal posture); down →
    `Some(position + 1)` (None at the bottom); payload type becomes
    `Option<(HabitId, usize)>` for `habit_move_up`/`habit_move_down`.
    **Superseded by stage-2, 2026-11-22: down mints `position + 2`**
    (`None` when `position + 2 > len`). This line's `position + 1`
    contradicted §2.1's own diagnosis: the gap at `position + 1` is
    the mover's own bottom edge — the engine's accepted in-place
    no-op — so crossing the lower neighbour needs the gap after it,
    `position + 2`. The code matches the supersession
    (`presentation::habit_move_slots`, pinned by
    `habit_move_slots_mint_target_indices`).
  - `on_habits_drop_asked` mints `target = gap` (clamped by the
    engine) and routes `ActionId::HabitMove` with the index payload;
    the router's three arms (`router.rs:1635–1656`) dispatch
    `HabitReorder`. The kiosk doubling gate and the stale-index
    absorptions keep their shapes.
  - `ActionId::HabitMove` keeps its routing id and dotted string
    (keymap/menu churn avoided); its one-shot semantic changes from
    absolute order to target index — recorded in the one-shot field
    docs.
- `StateCommand::MoveHabit` (absolute order) is untouched: the CLI
  (`cli.rs:701`, `commands/habits.rs:701`) and the undo inverse keep
  their verbs. The UI stops minting raw orders entirely.

### D2 (#1200): the click seats the ViewState cursor — `activate_habit_row`

- New `ViewState::activate_habit_row(&mut self, habit)` — the walk
  landing write's twin at the pointer seam: `focused_habit =
  Some(habit)`, `focused_task = None` (the exclusivity law,
  `view_state.rs:2354`'s landing write verbatim), returns
  `vec![FlowOutcome::RefreshNow]` so the emphasis ring repaints.
- `on_habits_row_activated` becomes the task pointer path's shape:
  `settle_pointer_conflict(&mut a, &window)` first (the
  `today_row_activated`/`context_requested` discipline), resolve the
  id through `a.tables.habits`, seat the cursor via
  `activate_habit_row`, `apply_outcomes`, and keep the existing
  `a.focused_habit` mint (the menu verbs' subject — click ≡ key
  composes both directions). The seam opens no editor (habits have no
  click-to-open verb; H4's reserved verbs untouched).
- Kiosk: selection is render-side (the emphasis ring reads the cursor
  regardless of `editing-enabled`) — the keyboard walk selects on
  kiosk today, so the pointer twin matches (D-P13 posture).

### D3 (stage-2 verification duty — the one residual survey risk)

Stage-2's adapter-level pins invoke the callbacks synthetically and
therefore **cannot** prove the real pointer path fires through the
DragArea/DropArea overlay (`gap-zone := DropArea` is a full-size
sibling rendered above the card). The static contrast says this is
fine — the board's working delegate uses the identical shape — but
the diagnosis law requires the residual named: stage-2 runs a
journey-level or manual smoke of the REAL click and drag paths on the
desktop shell. If a click provably never reaches `TaskCard::clicked`,
the interception is a third mechanism — file it as a finding on the
issue and stop (do not blind-patch the z-order).

## 4. Tests (TDD; every pin fails pre-fix — red evidence per issue)

Typed/structural outcomes only; no text assertions anywhere.

### 4.1 Domain (`command.rs` / `undo.rs` tests)

- `habit_reorder_dense_orders_swaps_the_adjacent_upper_neighbour` —
  orders {0, 1, 2}; `HabitReorder { habit: B, target: 0 }` → Applied;
  canonical sequence `[B, A, C]`. (THE red pin: the pre-fix path —
  mint → `MoveHabit` — no-ops/collides here.)
- `habit_reorder_dense_orders_swaps_the_adjacent_lower_neighbour` —
  mirror; target `2` from position 1 → `[A, C, B]`.
- `habit_reorder_long_distance_lands_between_the_crossed_rows` —
  sparse and dense fixtures; the mover lands between the rows the
  gap names, others keep their relative order.
- `habit_reorder_in_place_gaps_are_the_accepted_no_op` — target ==
  position or position+1 → `NoOp`.
- `habit_reorder_clamps_the_past_end_gap` — target 9 over 3 rows →
  append.
- `habit_reorder_refuses_unknown_and_deleted_typed` —
  `UnknownHabit` / `HabitDeleted` (`matches!` on the typed error).
- `habit_reorder_stamps_and_flushes` — every order-changed row's
  `clocks.order == now`; `UpsertHabit` per row + `board_flush`.
- `habit_reorder_undo_restores_the_section` — move, undo, sequence ==
  pre-move (the inverse class pin).
- Property (`proptest`, the US-14 shape): for randomized order
  vectors and gap targets, an applied reorder yields exactly the
  requested permutation of one row, dense non-negative orders, no
  duplicates.

### 4.2 Presentation (`presentation.rs` tests)

- `habit_move_slots_mint_target_indices` — dense fixture: up at
  position 2 → `Some(1)`; up at 0 → None; down at last → None; else
  `Some(position + 1)`. (Red pre-fix: the current mint returns
  floor-midpoint orders.) **Superseded 2026-11-22 (stage-2): down is
  `position + 2`** — see the supersession note in §3 D1.
- `habit_drop_target_is_the_gap_verbatim` — the drop seam's mapping
  is the gap, un-reordered.

### 4.3 Adapter/router (`adapter.rs` unit tier — structural)

- Invoking `habits-row-activated(i)` → `view_state.focused_habit()
  == Some(id_i)` AND `focused_task` cleared; the pushed habits model's
  `emphasis` bit is true exactly at row i. (THE #1200 red pin: pre-fix
  the cursor stays None and no emphasis bit flips.)
- Click composes with the keyboard truth: keyboard landing on row 1,
  click row 0 → the cursor is row 0's habit (one selection truth),
  the task cursor cleared.
- Invoking `habits-drop-asked(1, 0)` on dense orders → the captured
  command is `HabitReorder { target: 0 }` (typed match on the
  variant), NOT `MoveHabit`. (Red pre-fix.)
- Invoking the habit menu's Move-up row with the walk cursor on row 1
  → `HabitReorder { target: 0 }`; kiosk refuses (the HAB-14 doubled
  gate: seam + mint both inert).
- The keyboard Shift+Up stroke under a habit landing mints the same
  index one-shot (the shared mint site; one arithmetic).

### 4.4 Journey (`tests/journeys/habits_journey.rs`)

- `a1_drop_moves_the_row_through_the_engine` — drive the fake engine:
  seed dense habits, invoke the drop seam at a gap, run the engine
  step, assert the refreshed section's habit-id sequence (structural:
  id order, never titles) shows the move, and the applied orders are
  dense.
- `a1_menu_move_moves_the_row` — open the habit context menu on a
  row, invoke the Move-up row, engine step, sequence swapped; undo
  restores.
- `a1_click_selects_the_row` — invoke `habits-row-activated`, refresh,
  assert the model's emphasis bits (structural).
- Unregression (stage-3): the board task DnD journey + the r9
  keyboard reorder journeys still green (their grammar untouched).

## 5. Open questions — all resolved

- **Q: keep `MoveHabit` for the UI?** No — D1. The absolute-order
  verb survives for the CLI only; the UI cannot express dense-list
  moves in absolute orders.
- **Q: renumber all rows or re-space on demand?** Renumber all (D1);
  on-demand spacing reintroduces the collision class.
- **Q: boundary posture for the keyboard pair?** Typed `NoTarget` at
  the mint (the existing posture — the one-shot stays unset), not an
  engine error; the DnD gap clamps (pointer grammar) with in-place
  gaps as accepted no-ops.
- **Q: does the click seam route an action?** No — selection only;
  no `TaskEdit` analogue for habits (H4 reserved).
- **Q: is the slint drop/click interception in scope?** Only as D3's
  verification duty; a proven interception is a new finding, not a
  z-order blind patch.
- **Q: UX journey plan?** None in origin (dispatch plan §1: bug
  fixes, "census note rides"); no new user-visible verb or surface —
  the reorder/selection grammar exists and is being repaired to its
  accepted r9 contract (US-4 AC-1, US-3 AC-2).

## 6. Risks

- **R1**: D1's renumber amplifies LWW clock stamps to the whole
  section per move — accepted (small n, deterministic); revisit only
  if the sync wire grows habit sections into the hundreds (backlog
  candidate if measured).
- **R2**: D3's residual interception risk — if real, #1200's fix is
  necessary but insufficient; the decision rule is written (stop +
  finding, no blind patch).
- **R3**: the `habit_move_*` one-shot payload type change touches the
  router arms and the ctx struct — mechanical, but the census pins
  (route_consumes totalness test) must stay green.
- **R4**: undo's new inverse class touches the pre-image plumbing —
  keep `MoveHabit`'s class untouched so the CLI undo path cannot
  regress.
- **R5**: habits diverge from tasks' compact-move geometry until §7-B1
  lands — accepted and recorded; the divergence is the bug fix, not
  the drift.

## 7. Backlog issues (standing authorization exercised)

- **B1** — the task compact-move pair and the board drag-drop share
  #1198's geometry: `TaskMoveUp/Down` never cross the neighbour
  (US-14's own fixture: mover 10 → 5 behind sibling 0; the width-1
  pin is an accepted no-op) and the board gap-drop's midpoint
  collision-tie is decided by UUIDv7 mint order. Body cites this
  plan. Filed after this plan commits (permalink requirement).
- **R1's measurement note** rides B1's issue if the renumber cost is
  ever measured — no separate issue until then.

## 8. Stage-2 execution order

1. Domain: `HabitReorder` + tests (§4.1) — red first on the dense
   swap pins.
2. Presentation: index mints + tests (§4.2).
3. Adapter/router: payload rework, `activate_habit_row`, seam fixes +
   structural pins (§4.3); view_state twin.
4. Journeys (§4.4) + D3's real-pointer smoke.
5. Per-issue commits (`fix(habits): … #1198`, `fix(habits): …
   #1200`); gate via `scripts/run-quality-gates.sh cargo nextest run
   --workspace` (background + poll; defer per the flock rule).
   Estimated diff: ~600–750 lines including tests (~250–300
   production): domain ~200 (impl+undo+tests), ui-slint ~350
   (presentation/adapter/view_state/tests/journeys),
   presentation.rs ~60, slint files **0** (no `.slint` change is
   expected — the callbacks and payload shapes already carry the
   gap/row indices verbatim).
