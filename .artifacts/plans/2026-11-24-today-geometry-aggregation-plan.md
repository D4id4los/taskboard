# A1 — Today Geometry + Aggregation (implementation plan)

- **Date**: 2026-11-24
- **Kind**: implementation chunk plan (chunk A1 of round 6, "p1 GUI
  wave").
- **Origin**: scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §A (normative); dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §1/§2 (normative); issues **#1310** (section overlap), **#1304**
  (viewports don't grow), **#1301** (Today must aggregate all live
  boards). Round laws: typed/structural pins only (never pixel-text);
  diagnosis-first; the aggregation is a **projection-contract change**
  — every census/journey expectation that assumed board scoping gets a
  dated (2026-11-24) re-record; never weaken a correct test.
- **Branch**: `fix/today-geometry-aggregation`
- **Status**: READY — every open question resolved below with a
  recorded decision.

## 0. Verified diagnosis (stage-1, code-read 2026-11-24)

The filings' hypotheses were verified against
`crates/taskboard-ui-slint/ui/views/today-view.slint` (worktree at
branch tip `dc96c034`):

1. **Geometry root cause** — each Today section's
   `row-viewport-clip := Rectangle { clip: true }` (~line 356) has **no
   declarative height**. Its height is *imperatively assigned* only in
   two `changed` handlers (`changed content-height` ~372-377 and
   `changed watch-viewport-height` ~482-488) as
   `min(content-height, root.viewport-height - 2*Theme.space-3)` — an
   escape from a compiler-detected binding loop (the comment block at
   ~345-367 documents the compiler's conservative
   `height → content-height` edge on every `ListView`). Failure modes,
   all confirmed:
   - **No initial assignment**: Slint `changed` handlers do not fire
     for the value present at delegate instantiation. When Overdue and
     Scheduled materialize together (refresh from empty, day rollover),
     the clip height stays at layout-default; unlike every sibling
     layout child (`vertical-stretch: 0` at ~208/247/277/316/324), the
     clip sets no `vertical-stretch`/`min-height` — its geometry is
     *stretched*, not content-sized. Overdue's rows then paint across
     the slot boundary over Scheduled (#1310's screenshot symptom).
   - **Convergence trap**: the in-house virtualizer's `content-height`
     is an estimate that becomes accurate only once rows instantiate,
     and rows instantiate only for the viewport the clip allows. If
     the handler fires while the estimate is ~one row, the clip pins
     near one-row height and never re-fires: growing the height is
     what would change `content-height` (#1304's "stuck at one
     visible row").
   - **`visible-height` mismatch**: the inner `ListView`'s
     `visible-height` is bound to the window-derived cap
     (`root.viewport-height - 2*Theme.space-3`, ~line 371), not the
     clip's actual height — the virtualizer's live-row window and the
     painted viewport disagree; painted rows are bounded only by the
     (possibly stale/absent) clip.

2. **Aggregation scoping** — the Today *task* sections are already
   board-agnostic (`models.rs::push_today` calls
   `taskboard_domain::days::today_sections(&state.tasks, ..)`, and the
   domain fn is documented "board-agnostic by construction"). The
   board-scoped seams are:
   - `models.rs::push_habits` (~974) renders
     `presentation::habit_rows(state, vs.board(), ..)` — only the
     selected board's habits;
   - `presentation.rs::live_habits_in_order` (~7824) filters
     `h.board == board`;
   - `view_state.rs::today_rows` (~2536) — the focus walk — extends
     `TodayRow::Habit` from `self.board`'s habits only;
   - `presentation.rs::break_rule_rows` (~2262) is board-scoped and
     the Today surface reads it with `view_state.board()`
     (`adapter.rs:3235`);
   - `push_today`'s tracked-today rows are filtered to `vs.board()` on
     kiosk (~3605-3620);
   - `push_habits` gates the create mount on
     `!kiosk && board.is_some()` (~1034) and the habit-create draft
     mints into `view_state.board()`.

## 1. Decisions (recorded, binding for stage 2)

**D-1 — geometry fix level: container/viewport semantics, declarative
from the first frame.** The fix replaces the change-event-assigned
heights with a declarative, content-derived section height, at the
section-container level (not a per-section patch, not a virtualizer
rewrite). Constraints the concrete Slint shape must satisfy:

- (a) The section block's height is declaratively
  `min(content-derived height, window cap)` from the **first frame** —
  no `changed`-assigned heights anywhere in the section (the two
  handlers retire).
- (b) The binding-loop shape the compiler rejects (a direct
  `height: min(ListView.content-height, cap)` binding) must be escaped
  *declaratively*, not imperatively. Sanctioned shape: keep the
  two-property decoupling the file already documents (the repeater
  fills a bound `visible-height`, the element's layout height reads a
  content-derived value) but make **both sides bindings**, e.g. the
  clip's height reads the row layout's measured extent through a
  property the ListView does not feed back into (the existing
  `row-extent` extent-report channel, or a non-feedback measure
  property), so no cycle exists and Slint's dependency graph
  re-evaluates it on every content change — first materialization
  included, which fixes the no-initial-assignment mode by
  construction. Stage 2 picks the exact spelling; the acceptance is
  behavioral (§4 probes), not spelling-matching.
- (c) `visible-height` of the inner ListView binds to the clip's
  **actual** height (or the same declarative min-expression), so the
  virtualizer's live-row window equals the painted viewport — the
  mismatch mode dies.
- (d) The convergence trap dies *because* the binding is declarative:
  a content-height estimate that grows re-evaluates the height
  automatically; no handler can fail to fire.
- (e) The window cap (`viewport-height - 2*Theme.space-3`) semantics
  are preserved: a fitting section is content-sized (never stretched
  into leftover space); an overflowing section caps and scrolls
  internally; the outer page Flickable still scrolls correctly-sized
  sections.
- (f) The `watch-viewport-height` re-application retires with the
  handlers — a declarative binding tracks window resizes itself.

**D-2 — aggregation shape: board-canonical outer order, stored
`(order, id)` inner order, one shared derivation.**

- New single source:
  `presentation::today_habit_rows(state, now, calendar) ->
  Vec<HabitRow>` — concatenation over `live_boards(state)` in the
  canonical order (the same order the board bar speaks; the existing
  canonical-order test `live_boards_filters_to_live_boards_in_canonical_order`
  pins the outer order), each board's habits via the existing
  `live_habits_in_order(state, board)` (stored `(order, id)`, kept
  board-scoped — it remains the reorder arithmetic's inner reader).
  `habit_rows` either gains this wrapper or is re-signed; stage 2's
  call, but **one** derivation feeds both the render and the walk.
- `models.rs::push_habits` reads the aggregated rows; `vs.board()`
  disappears from the habit path.
- `view_state.rs::today_rows` extends `TodayRow::Habit` from the same
  aggregated derivation — walk == render by construction (the existing
  D2 law). Cross-board keyboard walk order is the render order.
- **Ordering across boards**: board-canonical outer, `(order, id)`
  inner. Rationale: board order is the only stable user-visible
  ordering that already exists; habit `order` values are only
  meaningful within a board (the floor-midpoint reorder arithmetic is
  per-board and stays so — a `HabitReorder` still resolves within its
  own board's row source, pinned by test).
- **Empty-board behavior**: zero live boards ⇒ no habit rows (the
  section collapses per the existing D7 empty-collapse law — no
  change); a live board with no habits contributes nothing. An
  **archived or deleted** board's habits never render (that is
  `live_boards`' existing filter — pinned).
- **Domain `Habit.board` stays.** #1301's open sub-question (drop the
  domain field) is answered **no**: storage, sync, CLI
  (`habits list --board`), and the reorder engine are legitimately
  board-scoped; the defect is presentation scoping only. The
  board-free property holds at the presentation layer; the domain
  field is the habit's home, not the Today view's scope. Re-open only
  if a future cross-board reorder feature demands it.

**D-3 — break rules and tracked-today ride the same board-free law.**

- The Today surface's break-rules roster reads
  `break_rule_rows(state, None)` (aggregate over all boards). Stage 2
  **verifies the rule-liveness filter** inside `break_rule_rows`
  (deleted/archived rules must be excluded independent of the board
  filter — the current body shows only a board filter; if liveness is
  missing that is a bug fixed on this branch, typed-pinned). The
  board view keeps its scoped call.
- The tracked-today kiosk filter (`filtered to the viewed board on
  kiosk`) **retires**: the Today view has no viewed board on kiosk
  either; what is shown is what is summed, over all live boards. The
  "what you see is what's summed" invariant is preserved by
  construction (the total folds the filtered set, which is now the
  full aggregated set). Census cell re-recorded (dated, §3).

**D-4 — habit create target board: first live board.**
`habits-create-mountable` becomes `!kiosk &&
!live_boards(state).is_empty()`; the create draft mints into the
**first live board** (canonical order — the same seed law
`normalize_board` uses today). The editor's title, which today shows
the board name (the #1303 family), must show the habit editor's own
title, not a board name — stage 2 verifies the draft's title path and
fixes the board-name leak if it fires from this mount (it is #1303's
territory; only the Today-mount instantiation belongs here). The
out-of-scope remainder (a target-board picker on create) is filed as a
backlog issue (stage 2 confirms the number; stage 1 filed it — see
§6).

**D-5 — A5a order of operations: A1 lands first.** The focused-row
scroll contract (#1302, A5a) reads
`scroll-row-into-view`/`FocusScrollHint` over the Today row set.
Aggregation grows the habits section's row count and the geometry fix
changes the viewport semantics the scroll estimates ride; A5a's
journey-level pin is therefore staged **after** A1 merges. Within A1:
stage 2 verifies the existing FocusScrollHint contract (targets are
`(section-index, row-index)` pairs — structurally unaffected by
aggregation) against the new row set and records the verification in
the stage-2 report; no scroll rework in this chunk.

## 2. Projection-contract re-record list (each with a dated note)

Every entry below gets a dated (2026-11-24) supersession/amendment
note naming this plan as authority. This is a projection-contract
change per the dispatch plan; no expectation is silently weakened —
each re-record states the new contract (aggregate over live boards)
and why the old one (board scoping) is retired by #1301.

1. `docs/ux-surface-census.org` habit-family Today cells —
   `habit.log`, `habit.reorder`, `habit.skip`, `habit.state_display`,
   `habit.convert_to_series` (each cites `presentation.rs::habit_rows`
   / `live_habits_in_order` and "the active board's" wording): the row
   source becomes `today_habit_rows` (aggregate); the cells keep their
   verbs and evidence anchors.
2. Census `task.location_display` Today cell (the "board › stack"
   context line, `today-view.slint` context-line): with aggregation
   the multi-board context line is the **norm** on Today, not a
   multi-board-desktop special case — note the semantics unchanged,
   the expectation re-recorded.
3. Census tracked-today cell / the `push_today` kiosk-filter comment:
   the kiosk board filter retires (D-3); the "see = sum" invariant
   re-recorded over the aggregated set.
4. `view_state.rs::today_rows` doc comment ("the active board's live
   habits", "Board-less days carry no habit rows") — rewritten to the
   aggregated contract; the zero-live-board sentence replaces it.
5. `presentation.rs::habit_rows` doc comment ("the active board's")
   — rewritten to the wrapper's contract.
6. `models.rs::push_habits` doc comment ("the active board's tracker
   rows") — rewritten; the create-mount gate sentence updated (D-4).
7. Journey expectations that assumed the scoped contract — re-checked,
   not weakened: `habits_journey.rs`, `habit_family_journeys.rs`,
   `habit_surface_journeys.rs`, `tracker_journeys.rs`,
   `small_gated_journeys.rs` (the us14 habit/task-move pair). Single
   fixture-board journeys pass unchanged by construction (one live
   board aggregates to the same row set — that equivalence is itself
   pinned, §3). Any journey that asserts a *second* board's rows are
   absent from Today (none found at stage 1; stage 2 greps
   `board` in the journey files) gets the dated re-record, never a
   deletion.
8. `tests/support/driver.rs:423`'s "distinct list delegates never
   overlap" *assumption*: the geometry fix makes it a **real
   invariant**; the virtualisation probe (§3) asserts it, and the
   driver comment gains the dated note pointing at the pin.

## 3. Test catalogue (structural pins only — row counts, positions,
orderings; never pixel-text, per the round law)

**Geometry (#1310/#1304) — `tests/probes/virtualisation.rs` (or a
sibling today-geometry probe file):**

- G1 `sections_never_overlap_at_any_content_volume`: combinations of
  (overdue, scheduled, habits) ∈ {0,1,5,20} rows each; structural
  assertion: the sections' viewport Y-extents from the driver are
  disjoint and ordered.
- G2 `viewport_grows_with_content`: 1/5/20 scheduled rows; the last
  row is reachable by the section's internal scroll (scroll offset can
  bring row N-1 into the viewport) — the "stuck at one row" trap
  pinned dead; the section height tracks content up to the cap.
- G3 `first_frame_geometry_is_declared`: populate Overdue and
  Scheduled **together from empty** (the refresh/rollover path that
  the `changed`-handler miss breaks); on the first rendered frame the
  clip heights are content-derived (both sections' rows visible or
  correctly capped) — no layout-default heights.
- G4 `visible_height_matches_painted_viewport`: the ListView's
  live-row window equals the clip height (structural: realized row
  count vs clip height ÷ row extent) — no rows painted outside the
  clip's own window.

**Aggregation (#1301) — presentation fold/unit tests +
journey-level pins:**

- A1 `today_habit_rows_aggregates_live_boards_in_canonical_order`:
  ≥2 live boards, habits interleaved by board; assertion: outer order
  is board-canonical, inner `(order, id)`.
- A2 `archived_and_deleted_boards_contribute_no_rows`: a habit on an
  archived board and one on a deleted board never appear.
- A3 `single_board_aggregation_equals_the_scoped_derivation`: the
  equivalence pin — one live board ⇒ aggregated rows ==
  `live_habits_in_order` rows (protects every existing single-board
  journey).
- A4 `walk_and_render_agree_on_the_multi_board_row_set`:
  `today_rows`' `TodayRow::Habit` sequence == the habits section's
  rendered ids (the by-construction law, now pinned across boards).
- A5 `reorder_resolves_within_the_habits_own_board`: a `HabitReorder`
  gap mint for a row on board B computes against B's row source, not
  the concatenated set (the floor-midpoint arithmetic stays
  board-scoped).
- A6 `zero_live_boards_renders_no_habit_rows_and_the_create_mount_is_unmountable`
  (typed mount flag, D4).
- A7 (kiosk, D-3): tracked-today sums over all live boards on kiosk;
  the retired filter's census note cites this test.

**Existing suites re-checked**: `habits_journey.rs` /
`habit_family_journeys.rs` / `tracker_journeys.rs` /
`small_gated_journeys.rs` must pass **unchanged** (A3's equivalence);
if one fails, it found a real contract change — diagnose per the
never-weaken law, re-record with a dated note only if the new
contract is the desired one.

## 4. Acceptance (verbatim from the dispatch plan, annotated)

- Sections never overlap at any content volume (G1, at 1/5/20-row
  sections).
- Viewports grow (G2) and are declared from the first frame (G3).
- The Today view aggregates all live boards (A1/A2: rows from ≥2
  boards render; an archived/deleted board's rows never render).
- The focused-row contract verified against the multi-board row set
  (D-5, recorded in the stage-2 report — no rework here).
- Census notes dated (§3's list, all 2026-11-24).
- Existing Today journeys re-checked with per-plan diagnosis for any
  expectation change (§3 tail).
- Diffs confined to the models/presentation/view seams
  (`today-view.slint`, `models.rs`, `presentation.rs`,
  `view_state.rs`, `adapter.rs` Today-mount sites only; domain
  untouched except the already-agnostic `today_sections` — no domain
  change).

## 5. Risks & size

- **Slint binding-loop shape** (D-1b): the declarative min-height may
  need the extent-report channel (`row-extent`) or a measure-property
  trick; if the compiler still rejects every declarative spelling, the
  fallback is a *single* assignment site that also runs on delegate
  instantiation (`init`/first-frame, not only `changed`) — recorded
  now so stage 2 does not improvise a second imperative path; G3 pins
  whichever shape lands.
- **Performance**: aggregation increases the habits section's row
  count on multi-board users; the virtualizer already bounds live
  rows, and D-1c re-aligns the window — watch the #592
  extent-report gate stays a no-op in steady state.
- **#1303 adjacency** (D-4): the create-mount board-name title leak —
  fix only the Today-mount instantiation; the editor's own title
  plumbing is #1303's chunk.
- **Kiosk tracked-today change** (D-3) is a visible behavior change on
  kiosk; flagged here and in the census note for the operator's PR
  review.
- **Estimated diff**: ~400–550 lines —
  `today-view.slint` ~60–100, `models.rs`/`presentation.rs`/
  `view_state.rs`/`adapter.rs` ~80–130, tests ~200–280, census doc
  notes ~40.

## 6. Backlog issues

- Stage 1 filed: **#1314** — `backlog:` habit create from the
  board-free Today view offers no target-board choice (first-live-board
  mint is the interim law, D-4).
