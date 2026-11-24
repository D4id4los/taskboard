# B3 — The Move Geometry Trio (implementation plan)

- **Date**: 2026-11-24
- **Kind**: implementation chunk plan (chunk B3 of round 6, phase 1).
- **Origin**: scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §B (normative); dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §1/§9 (normative); issue **#1247** (the p2 sibling of the habit move
  no-op). Reused input: A1's landed geometry diagnosis
  (`.artifacts/plans/2026-11-24-today-geometry-aggregation-plan.md`,
  landed as f77a66cb/81136cd3/e4b0b72f) and the round-5 #1198
  diagnosis (`.artifacts/plans/2026-11-22-today-habit-interaction-plan.md`
  §2.1, §3 D1 — normative for the reorder law). Round laws: typed pins
  (the midpoint rule is typed, not pixel-fudged); diagnosis-first;
  out-of-scope findings become `backlog:` issues immediately.
- **Branch**: `fix/move-geometry-trio`
- **Status**: READY — every open question resolved below with a
  recorded decision.

## 0. Verified diagnosis — the shared-geometry verdict (stage-1, code-read 2026-11-24 at origin/main 11771219)

#1247 claims the task compact-move pair and the board drag-drop share
the habit move geometry bug. **Verdict: the *disease* is shared, the
*patient count is two, not three* — the habit consumer already
carried the fix (round-5 #1198 chunk A1 D1 landed the engine-side
`HabitReorder` with dense renumber). The trio is therefore: two
broken consumers + one landed fix that is the reused *pattern*, not a
patient.** One arithmetic defect class underlies both patients:

> **The midpoint-slop law**: any move resolved as the floor midpoint
> of the free interval bracketing the drop gap cannot express a move
> across an *adjacent* (gap-1) neighbour — the width-1 interval has
> no free integer, so the mint lands on an occupied order (or the
> mover's own order, the silent no-op). On dense integer orders
> (creation appends `last + 1`), *every* adjacent pair is gap-1, so
> every adjacent compact move is dead.

Per consumer:

1. **`StateCommand::TaskMoveUp`/`TaskMoveDown`** (`command.rs`
   ~1389–1455) — BROKEN (confirmed; #1247's facts verified). The arm
   enumerates the stack's live rows in canonical `(order, id)` order
   (the correct row source), then resolves the slot through
   `floor_midpoint_order` — and *explicitly pins the width-1 case to
   `task.order`* (the commented no-op arm): the up-arm's raw midpoint
   would collide with the neighbour, so both arms pin to the mover's
   own order and the `order == task.order` branch returns
   `no_op()`. On a dense stack an adjacent compact move can **never
   swap the rows**. Its own fixtures confess it:
   `us14_s1_task_move_up_resolves_the_floor_midpoint_slot` moves 10→5
   with the sibling at 0 — the mover stays *behind* the sibling in
   canonical order — and
   `us14_task_move_adjacent_gap1_tie_is_the_accepted_no_op` pins the
   no-op as "accepted" (a limitation pin, per the no-pins-of-today's-
   behavior law).
2. **Board drag-drop slot arithmetic** `view_state::slot_order`
   (`view_state.rs` ~822) — BROKEN (confirmed). The gap-between-ends
   arm mints `a + (b - a) / 2`; for an adjacent pair this equals
   `a.order` — an occupied order — and the `(order, id)` canonical
   sort breaks the collision by UUIDv7 mint order (`now_v7`): the
   drop lands above or below the neighbour by creation-order luck,
   documented as the "leniency tie". Both construction sites ride it:
   the keyboard confirm (`move_flow_confirms_the_aimed_slot_order`)
   and the pointer drop (`append_move_command`/`slot_order` at
   `view_state.rs` ~4300/4409).
3. **The habit move** — FIXED (round-5 landed). `HabitReorder
   { habit, target: usize }` takes a **gap index**, the engine does
   remove/insert/renumber dense `0..n` on the section's canonical
   row source, the width-adjacent case is expressed (the mover is
   placed beyond the crossed neighbour), undo is the multi-row
   `InverseClass::HabitReorder`. `floor_midpoint_order`'s doc
   already records that the UI's habit reorder "no longer delegates
   here".

**Shared?** Not one shared *computation* — the two patients compute
independently (the engine arm calls `floor_midpoint_order`; the UI
`slot_order` inlines its own midpoint) — but they share the
*arithmetic habit* and the exact failure mode, and the landed habit
fix is the sanctioned cure. They are fixed together, on one law.

## 1. Decisions (recorded, binding for stage 2)

**D-1 — fix level: promote the landed habit law to THE task-order
law (one gap-index reorder rule, all consumers).** Not "one midpoint
rule all three consume" (the midpoint is the disease) and not three
per-consumer patches: the engine gains `StateCommand::TaskReorder
{ task: TaskId, target: usize }` mirroring `HabitReorder` verbatim —
`target` is a gap index into the **stack's** live rows in canonical
`(order, id)` order; clamp `0..=len`; the in-place gaps
(`target == position || target == position + 1`) are the accepted
`no_op`; otherwise remove/insert (`insert_at = target <= position ?
target : target - 1`) and **renumber the stack's live rows dense
`0..n`**, upserting only changed rows (each stamping
`clocks.position`? — no: **`clocks.order`** semantics follow the
`MoveTask` stamp the stack uses today; stage 2 keeps the existing
`MoveTask` stamp field verbatim) plus the stack's board flush.

- Rationale for renumber-everything over a two-row swap: the #1198
  D1 decision record already ruled the alternative (sparse
  re-spacing heuristics = a second arithmetic and the collision
  class again) out for habits; the same ruling applies. Task stacks
  are the same size class as habit sections (dozens); LWW cost is
  bounded and deterministic. **One reorder law, two entity kinds.**
- `TaskMoveUp`/`TaskMoveDown` **keep their command shapes** (keymap,
  journal strings, one-shot ids unchanged) but their engine arms
  **delegate** to the same resolution: up = target gap `position`,
  down = target gap `position + 2` (the habit pair's superseded
  minting law, `habit_move_slots`, verbatim — the gap *after* the
  crossed neighbour); boundary gaps (no such position) are the
  typed `TaskAtBoundary` refusal, unchanged. The width-1 no-op arm
  and the `floor_midpoint_order` call in the pair retire.
- **Dense-stack visibility invariant** (the acceptance core): after
  a compact move across an adjacent row, the canonical order shows
  the mover and neighbour **swapped** — pinned typed by row sequence,
  never by pixel or order-value equality.

**D-2 — the board drag-drop consumer mints gap indices.**
`slot_order`'s between-ends midpoint retires from the Move flow: the
`Move` flow confirm (`view_state.rs` ~4300/4409) mints
`StateCommand::TaskReorder { task, target: gap }` for **same-stack**
drops (the gap grammar it already speaks — `0..=len` verbatim).
Cross-stack and bare-surface aims keep `MoveTask` + `append_order`
(absolute order across stacks is a *different* act — insertion into
another stack's sparse tail, not a same-stack reorder; the midpoint
there has room by construction of the append, and multi-stack
insertion is out of this chunk's law).

**D-3 — `slot_order` retirement scope: the between-ends midpoint arm
is deleted; the end arms stay.** Gap-0 (`first - 1`) and the append
gap (`last + 1`) remain as `MoveTask` inputs for cross-stack/first
position minting — stage 2 re-signs the function or moves the end
arms inline and deletes the between arm + its "leniency tie" doc
comment. `floor_midpoint_order` itself **stays**: its remaining
callers (`insertion_order_after` for the completion spawn — a
new-row insertion, not a crossing move — and gap-0/append minting)
are legitimately midpoint-shaped; its doc comment is updated to
record that *no move path* delegates to it anymore.

**D-4 — undo: the task pair/reorder joins the multi-row inverse
class.** The swap-expressing move changes *two or more* rows' orders;
the single-pre-image inverse the pair rides today
(`undo.rs` ~553: `TaskMoveUp`/`TaskMoveDown` grouped with
`MoveTask`) cannot express it. New `InverseClass::TaskReorder`
mirroring `InverseClass::HabitReorder` (~1181/2228): the stack's
touched rows' pre-move orders journaled, restored as one batch of
`MoveTask { id, order: pre.order }` with per-row order watermarks —
`MoveTask` stays the CLI verb and the cross-stack verb untouched.
Requirement pin: move, undo → the stack's canonical sequence equals
the pre-move sequence.

**D-5 — fake-engine parity.** `FakeEngine::apply_compact_move`
(`tests/support/fake_engine.rs` ~71) mirrors the engine: it
re-resolves through `floor_midpoint_order` today (the same slop) and
is rewritten to the same gap-index resolution — fake ≡ engine by
construction, pinned by the existing fake-vs-engine journey posture.
The fake's `slot_order` uses (if any) ride D-2.

**D-6 — typed pin shape (round law (a)).** The gap-index payload is
already typed (`usize` grammar, clamp at the engine — no raw `i64`
order crosses the move path anymore). The slot law is pinned typed:
a `TaskReorder` result is asserted by canonical row sequence
(`Vec<TaskId>` equality after the move), the boundary refusal by
`matches!(err, CommandError::TaskAtBoundary(..))`, the no-op by
`CommandOutcome` — never by order arithmetic values, never by text.

**D-7 — A1-unregression check (round law (c)).** A1's landed pins
must stay green after this chunk: stage 2 runs, at minimum,
`sections_never_overlap_at_any_content_volume`,
`first_frame_geometry_is_declared` (both
`crates/taskboard-ui-slint/tests/probes/virtualisation.rs`),
`the_today_habit_rows_aggregate_all_live_boards` (`models.rs`),
`today_rows_walk_aggregates_all_live_boards_habits`
(`view_state.rs`), and the us14 habit-reorder pins — plus the full
workspace suite (quality gates, §5). B3's diffs are confined to the
task-order seams; no Today render path is touched, so unregression
is expected by construction — but the round law requires it
*verified*, and the stage-2 report records the run.

## 2. Re-record list (dated 2026-11-24 supersessions, never silent
weakenings)

The pins below pin "today's behavior" (the slop) and are superseded
by this plan as authority:

1. `us14_s1_task_move_up/down_resolves_the_floor_midpoint_slot`
   (`command.rs` tests): re-recorded — the mover lands **beyond**
   the crossed neighbour; new pins for the dense-adjacent swap.
2. `us14_task_move_adjacent_gap1_tie_is_the_accepted_no_op`:
   superseded — the gap-1 stroke swaps; the *in-place* gaps
   (`target == position || position + 1` in the reorder grammar) are
   the remaining accepted no-ops, and the name becomes
   `..._currently_pins_...`-free: the new contract is the desired
   one.
3. `us14_task_move_property_permutes_one_row_within_the_bracket`
   (proptest): the property's oracle is re-stated over the reorder
   semantics (a move permutes the canonical sequence by one removal +
   insertion; all other rows keep relative order — renumber changes
   order *values*, not sequence).
4. `slot_order_spans_the_between_ends_and_empty_grammar` and
   `move_flow_confirms_the_aimed_slot_order` (`view_state.rs`
   tests): re-recorded over the gap-index mint (D-2/D-3).
5. The "leniency tie" doc comments (`slot_order`, `insertion_order_after`,
   `floor_midpoint_order`, `messages.rs` `TaskMoveUp` doc): rewritten
   to the reorder law with the dated note.
6. `undo.rs` tests exercising the single-pre-image task-move inverse
   (`undo/tests.rs` ~1308: "midpoint 5" fixtures): re-recorded to the
   `TaskReorder` inverse class.

## 3. Test catalogue (typed/structural only; every new pin red
pre-fix per TDD)

Domain (`command.rs`/`undo.rs` tests):

- T1 `task_move_up_across_dense_adjacent_neighbour_swaps`: dense
  stack (orders 0,1), `TaskMoveUp` on the lower row → canonical
  sequence `[moved, former-upper]`; symmetric down pin.
- T2 `task_move_boundary_refusal_survives_the_reorder_law`: first/last
  row strokes refuse `TaskAtBoundary` (typed).
- T3 `task_reorder_gap_grammar_clamps_and_no_ops_in_place`: past-end
  clamp to append; `position`/`position+1` are accepted no-ops
  (`CommandOutcome` asserted).
- T4 `task_reorder_renumbers_dense_and_sparse_alike`: a sparse stack
  (0, 10, 20) reorders and renumbers to `0..n`; only changed rows
  upsert (action count asserted).
- T5 `task_move_undo_restores_the_pre_move_sequence` (D-4).
- T6 `task_move_pair_and_reorder_share_one_resolution`: structural —
  the `TaskMoveUp`/`Down` arms and `TaskReorder` resolve through one
  function (no second arithmetic; the finding named in
  `floor_midpoint_order`'s doc retires for move paths).
- T7 proptest (re-stated §2-3): arbitrary live stacks — any legal
  reorder preserves the multiset of row ids and every non-moved row's
  relative sequence.

UI (`view_state.rs`/flow tests + a journey):

- U1 the Move flow same-stack confirm mints `TaskReorder` with the
  aimed gap (typed `StateCommand` assertion).
- U2 drop on an adjacent pair swaps — the `#1247` user-visible
  symptom pinned at the flow level.
- U3 the keyboard confirm and the pointer drop build equivalent
  commands from the same gap (the r2 chunk-8 D-F
  one-construction-site law, carried over).

## 4. Acceptance (verbatim from the dispatch plan §9, annotated)

- The three consumers' move geometry pinned — D-6: typed, not
  pixel-fudged (T1–T7, U1–U3).
- The A1 pins unregressed (D-7, recorded in the stage-2 report).
- Diffs confined to: `crates/taskboard-domain/src/command.rs`,
  `messages.rs`, `journal.rs` (string arm), `undo.rs`,
  `crates/taskboard-ui-slint/src/view_state.rs`,
  `tests/support/fake_engine.rs`, and the re-recorded test files. No
  Today render path, no storage/sync schema (orders stay `i64`;
  renumber is plain row upserts — sync-transportable as-is).
- Full quality-gate suite green (pre-commit hooks + flock-wrapped
  nextest per the dispatch discipline).

## 5. Risks

- **LWW/sync churn from renumber**: a same-stack reorder now touches
  up to the whole stack's rows (each stamping its order clock). The
  #1198 D1 record accepted this for habit sections; stacks are the
  same size class. Residual risk: concurrent-device edits to other
  rows in the stack lose to the renumber more often than under
  single-row moves — accepted, recorded; escalate only if a sync
  pin breaks (then the diagnosis-first law applies on the branch).
- **`MoveTask` stays absolute-order**: cross-stack moves and the CLI
  keep `MoveTask`; its insertion into a *sparse target* is
  midpoint-shaped legitimately. The adjacent-collision class survives
  only where the target is dense — the multi-move block-splice
  fallback (`multi_move_commands`, documented append fallback) is a
  named out-of-scope sibling (backlog, §6).
- **Undo watermark breadth**: `InverseClass::TaskReorder` journals
  every touched row; a refused watermark on any row refuses the
  whole batch (the HabitReorder precedent, carried verbatim).
- **`journal.rs`/one-shot id stability**: command variants keep
  their names/ids; the new `TaskReorder` adds a journal string —
  additive per the O-2 law.

## 6. Backlog issues (standing authorization exercised)

Filed at stage 1 (numbers in the final report):

- The multi-move block-splice adjacent fallback shares the slop
  class (documented append fallback; not this chunk's law).
- The CLI `MoveTask`/`MoveHabit` absolute-order verbs cannot express
  a same-stack adjacent crossing on dense orders (the UI-reorder
  law's CLI twins).

## 7. Estimated size

~300–450 lines: domain ~120–180 (new command + engine arm + undo
class), UI ~40–80 (slot_order retirement + flow mint), fake engine
~20–40, tests ~120–180, doc/comment re-records ~20.
