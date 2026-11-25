# Chunk C3a — Today Two-Column at Large Widths (#1436) — Plan Skeleton

<!-- Skeleton written by the J-C2/C3 journey-stage agent (2026-11-25).
The stage-2 agent COMPLETES this plan at gate clearance. Do not
dispatch before the journey plan is accepted. -->

- **Branch**: `feat/today-two-column`
- **Origin**: journey plan
  `.artifacts/plans/2026-11-25-sound-today-colour-journeys-today-two-column-ux-plan.md`
  (status: GATE-PAUSED — operator acceptance pending); issue #1436.
- **Body reference**: #1436. Closes #1436 on merge.

## 1. Diagnosis (code base, journey-confirmed)

- The Today view (`crates/taskboard-ui-slint/ui/views/today-view.slint`)
  is single-column: a VerticalLayout of section blocks under one
  scroll container, with the #1310/#1304 per-section viewport
  geometry channel and the `Theme.layout-severing-max-width` root cap.
- The A1 aggregation contract (plan
  `2026-11-24-today-geometry-aggregation-plan.md` §2/D-3): Today row
  sources are the all-live-boards aggregates
  (`presentation.rs::today_habit_rows`, the aggregated task sets) —
  verified: two columns reflow placement only; the row sets and the
  multi-board context line semantics compose unchanged.
- The board layout's large-width breakpoint machinery is the study
  target for D-2's shared constant; the masked-walk machinery
  (app-window.slint:2213-2217 idiom) owns walk order.

## 2. Decisions (ruled by the journey plan — implement as written)

- D-1: left column Overdue→Scheduled; right column Habits→Day
  Review; timers + tracked-day summary stay full-width above the
  grid.
- D-2: one new `Theme` token `layout-today-two-column-min-width`,
  value = the board layout's existing large-width breakpoint.
- D-3: walk order column-major (left column top-to-bottom, then
  right); below the breakpoint the linear walk is unchanged.
- D-4: one scroll container; columns top-align; no nested scroll.
- D-5: the grid lives inside the existing severing cap; the E1
  severing census gains the grid arm.

## 3. Implementation outline

1. `theme.slint`: the D-2 token (const, like the severing cap).
2. `today-view.slint`: the width-driven layout arm re-parenting the
   four section blocks into the two-column grid (pure Slint; the
   section clip/viewport geometry channel keeps reporting per-section
   extents — verify the convergence transients still converge under
   the grid).
3. Walk successor order (presentation-layer derivation + adapter
   route unchanged kinds): column-major mapping for the grid arm.
4. Goldens: wide variants added to the breakpoint corpus
   (`golden_visuals.rs`; the wave-2 breakpoint corpus is the base).
5. Journey tests: `crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs`
   — the step-ID-named list in the journey plan §8.

## 4. Quality gates

- Pre-commit hooks + flock-wrapped full nextest (dispatcher owns the
  coverage trio). Goldens must pass both arms.
- `scripts/ux-coverage.sh <journey plan> --gate` green once the
  journey tests land.

## 5. Risks

- The #1310 geometry channel under re-parenting: section-height
  convergence is the fragile corner — keep the section block inner
  structure untouched; only the parent layout changes.
- Walk wrap correctness at empty sections (an empty left column):
  the successor mapping must skip empties per the existing walk law —
  covered by US-22/S2's coverage test.

## 6. Out-of-scope findings → backlog issues

Filed by the journey stage: resizable/proportional widths; section
arrangement; layout override toggle (R-3). New findings →
`backlog:` issues immediately.

## 7. Estimated diff

~150–300 lines: theme token, today-view layout arm, walk-order
derivation, goldens + journey tests. No domain/engine changes.

## 8. Stage-2 completion checklist (fill at gate clearance)

- [ ] Breakpoint value + source constant cited
- [ ] Grid arm line cites + geometry-channel verification note
- [ ] Walk successor mapping site cited; empty-section skip verified
- [ ] Golden wide variants enumerated (widths used)
- [ ] Full local hook/nextest evidence recorded
