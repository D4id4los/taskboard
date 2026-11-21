# Chunk PR2 — walk affordance parity (#1030, #1031, #1041) — `feat/ui-walk-affordance-parity`

- **Date**: 2026-11-21
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`
  (US-13, US-14, US-15; W-14..W-16; R-8, R-9). Round-4 dispatch plan
  chunk PR2. Authorities: the walk-stop ladder owning the corrections
  (#1034's landed subsumption is the base), the K4 cursor state
  (`.artifacts/plans/2026-11-19-row-cursor-walk-plan.md` §9), the O-8
  strict keyboard–mouse parity law, chunk-8's habit compact-move
  (`presentation::habit_drop_order`'s shared midpoint arithmetic).
- **Status**: DRAFT — gated on the journey plan's operator acceptance.
- **Issues**: #1030, #1031, #1041.

## 1. Scope

1. **#1030 File-to keyboard activation** — the armed File-to picker's
   candidate rows become focusable accessible-role buttons activated
   by key; activation routes the same commit seam the row's click
   dispatches (US-13). The W3 `TextButton` centralization (#953) is
   the natural primitive adoption point; the K4 list-row cursor is
   the alternative — the recorded decision is **accessible-role
   button + key activation** (click ≡ key on the same seam), with the
   armed prev/next walk unchanged (US-13/S2).
2. **#1031 Today task compact-move** — a Today-scope per-step pair:
   new `StateCommand::TaskMoveUp` / `TaskMoveDown` one-shot twins of
   the habit pair, riding the shared midpoint arithmetic
   (`presentation::habit_drop_order`); Shift+Up/Shift+Down resolve by
   focused row kind (R-8) — task row → the task pair, habit row → the
   landed `HabitMoveUp`/`HabitMoveDown` (US-14). Boundary strokes
   refuse typed, no wrap. The board's Flow::Move ceremony is *not*
   generalized to Today (option 1 of #1031's proposal is the recorded
   decision; the ceremony stays board-scoped).
3. **#1041 planner in-pane cursor** — the planner's directional
   verticals arm and walk a K4 cursor over the focused pane's rows
   (`models.rs::plan_position` exposes the panes), replacing
   `adapter.rs::interpret_directional`'s PlanNextDay Up/Down typed
   inert (US-15). Left/Right pane switch (`view_state.rs::switch_plan_pane`)
   is unchanged and the cursor state follows the K4 clear/transfer
   law (US-15/S4).

## 2. Work items (journey-id mapped)

- W-14: File-to candidate buttons + activation seam. Tests:
  `us13_s1_candidate_key_activation_commits`,
  `us13_s2_candidate_rows_accessible_button` (structural).
- W-15: TaskMoveUp/TaskMoveDown commands + kind resolution. Tests:
  `us14_s1_task_move_up_dispatches`,
  `us14_s2_task_move_boundary_typed_refusal`,
  `us14_s3_habit_pair_unchanged_on_habit_row`.
- W-16: planner pane cursor. Tests: `us15_s1_planner_cursor_arms`,
  `us15_s2_planner_cursor_walks_clamped`,
  `us15_s3_planner_row_verb_uses_own_seam`,
  `us15_s4_pane_switch_preserves_cursor_law`.
- Engine-side: the two new StateCommand variants in
  `taskboard-domain::messages` + the state engine's Today-order move
  rule (additive; existing snapshots unchanged or per-diagnosis).
- Census slice: Today task compact-move cell; File-to rows;
  planner in-pane walk; R-8/R-9 recorded.

## 3. Traps (from the dispatch plan + rulings)

- **The walk-stop ladder owns the corrections** — #1034's landed
  subsumption (ControlNext/Prev Tab re-injection under the ladder) is
  the base this chunk reads, never modifies; a ladder change here is
  a finding.
- **#1041's in-pane cursor rides the K4 cursor state** — a second
  cursor mechanism is a finding (R-9); the planner cursor is an
  adopter, not a novel design.
- The **O-8 strict keyboard–mouse parity is the acceptance bar**:
  after this chunk, every pointer verb on the touched surfaces is
  keyboard-reachable (File-to activation, Today reorder, planner
  walk).
- The move pair shares the habit pair's midpoint arithmetic — a
  second, divergent order arithmetic is a finding; the state engine's
  move rule reuses the same presentation helper (engine-side
  ownership settled at stage 2's seam read).
- Drag and keyboard reorder must dispatch the same Move seam family
  (the r9 dispatch's parity trap, inherited).

## 4. Verification

- Journey tests in `tests/journeys/small_gated_journeys.rs`; seam
  tests for the move pair's exact commands; the boundary and
  no-wrap behaviors as typed refusals; the Σ/order property over the
  Today ordering follows the habit pair's established property
  pattern if the state engine's rule is new.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh`.

## 5. Definition of done

- [ ] Journey plan's US-13/US-14/US-15 wiring rows implemented and
      green
- [ ] Gate exits 0 on the journey plan
- [ ] #1030, #1031, #1041 close with the PR reference
