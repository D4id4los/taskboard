# Today Two-Column UX Journey Plan — today-two-column

<!--
Written from .artifacts/templates/ux-journey-plan.md. The guidance
comment STAYS in the copy; only <...> placeholders go unfilled.
-->

# [Plan] UX Journey Plan — Today two-column layout at large widths (#1436)

- **Date**: 2026-11-25
- **Kind**: UX journey plan — user stories, journeys, flows, wiring
  contract for the Today view's two-column large-width layout (#1436).
  No implementation decisions beyond the command contract.
- **Origin**: round-8 scope report §C-C3 (2026-11-25-backlog-reduction-round8-scope-report.md); issue #1436 (normative body); the A1 aggregation contract (plan `2026-11-24-today-geometry-aggregation-plan.md` §2/D-3 — the Today row sources are the all-live-boards aggregates `presentation.rs::today_habit_rows` and the aggregated task sets); the severing guard (`Theme.layout-severing-max-width` at the Today root) and the board layout's width-breakpoint machinery (the study target #1436 names).
- **Status**: ACCEPTED with amendments (2026-11-25 — see the
  amendment log below; unblocked C3a)

## 0. Amendment log (dated, binding)

- **2026-11-25 — operator acceptance with amendment** (recorded on
  issue #1436,
  https://github.com/D4id4los/taskboard/issues/1436#issuecomment-7172600684):
  the kiosk mount is **NOT** special-cased — kiosk renders
  two-column too, whenever space allows; the same breakpoint rules
  apply on every mount and the layout logic carries **no
  kiosk/non-kiosk branch**. This SUPERSEDES US-19 ("kiosk stays
  single-column"), step US-19/S1, ruling R-2, and the "Kiosk
  two-column" non-goal. US-19-as-amended pins the amended semantics:
  a kiosk-mode window at/above the D-2 breakpoint mounts the grid
  arm; below it, the single column — the width test alone decides.
- **2026-11-25 — dispatch ruling (stage-2 clarification, operator
  answered via the dispatch channel)**: the Today model renders
  **four** task sections (Overdue, Scheduled, Due today, Done) while
  D-1 names only the two the issue titled. Ruled: the left column
  carries **all four task sections** in display order (D-1's "the
  task sections, urgency first" vs "the meta sections" is a
  grouping-by-kind), the right column Habits + Day Review; D-1's
  "exactly four sections" reads as the issue's four *named* surfaces
  (Overdue, Scheduled, Habits, Day Review) joining the grid
  explicitly, with the remaining task sections joining their kind
  group. D-3's column-major walk then needs **no successor
  reordering at all**: the existing walk domain (task sections in
  display order → aggregated habits → break-rule roster) already
  reads left-column-then-right-column; the column wrap is
  Done-last → Habits-first, and the break-rule roster keeps its
  existing walk position (the full-width band is not a column).
- **Surfaces touched**: `crates/taskboard-ui-slint/ui/views/today-view.slint` (layout arm only); its goldens (`tests/journeys/golden_visuals.rs` wide variants); no adapters, no commands.
- **Platform scope**: desktop (the two-column arm); kiosk (single column, unchanged — per story)

## 1. Scope & goals

What the user gains: on a desktop viewport wider than the recorded
breakpoint, the Today view reflows into **two columns** — left:
Overdue, then Scheduled; right: Habits, then Day Review — so a large
monitor sees the whole day without scrolling. Below the breakpoint the
single-column layout is byte-identical to today's; kiosk narrow
displays always get the single column.

The section row sets are **untouched**: every section continues to
render the A1-aggregated all-live-boards rows (`today_habit_rows`,
the aggregated task sections per plan
`2026-11-24-today-geometry-aggregation-plan.md` §2/D-3 — "see = sum"
holds on both columns). Two columns reflow *placement*, never
membership or ordering-within-section.

**Decisions (recorded, binding for C3a):**

- **D-1 — column assignment**: left column = Overdue, Scheduled (the
  task sections, urgency first); right column = Habits, Day Review
  (the meta sections). Timers and the tracked-day summary band stay
  **full-width above the two-column grid** (the D-P18 posture:
  timers first on the page; a timer firing must stay visible
  regardless of column reading order). Exactly the #1436 request's
  four sections move; nothing else joins the grid.
- **D-2 — the breakpoint**: one new `Theme` layout token
  (`layout-today-two-column-min-width`, logical px) set equal to the
  board layout's existing large-width breakpoint value — one shared
  constant, no second number to keep in sync. Below it: single
  column, byte-identical DOM of sections.
- **D-3 — walk order is column-major**: the masked-walk/focus
  machinery (logical row focus, app-window.slint:2213-2217 idiom)
  walks Overdue → Scheduled (left column top-to-bottom), then wraps
  to Habits → Day Review (right column). Column-major preserves the
  existing semantic section order (timers → habits → task sections →
  review) as the *reading* order does not match the *visual* left-to-
  right sweep, so the walk is documented as: vertical sweep per
  column, left column first. Rationale: a mixed row-major interleave
  would jump the user between unrelated section kinds every row.
  Keyboard parity law (ux_planning §2.7) is satisfied: every row
  keeps its existing chords; only the *next/prev* successor mapping
  changes at the column wrap.
- **D-4 — equal-height columns**: the two columns are two independent
  scroll regions inside the existing Today scroll viewport? No —
  decision: the **whole page scrolls as one** (the existing scroll
  container), the grid is a two-column arrangement inside it;
  columns top-align, unequal heights leave the shorter column's
  space below empty. No nested scrolling (nested scroll regions were
  the #1310 convergence-transient class — not reopened).
- **D-5 — severing guard composition**: the two-column grid replaces
  the single column *inside* the existing
  `max-width: Theme.layout-severing-max-width` root cap — the grid
  activates between D-2's breakpoint and the severing cap; above the
  cap the centred capped width carries the same two-column grid. The
  E1 severing census gains the grid arm (C3a trap (c)).

**Non-goals (with homes):**

- Resizable/proportional column widths (50/50 fixed) — backlog issue
  filed with this plan.
- Three-column or user-arrangeable sections — backlog issue (same
  family; filed once).
- Kiosk two-column — deliberately absent (R-2): kiosk displays are
  narrow by definition; the breakpoint makes it unreachable without
  a ruling.
- Any change to row content, chips, or section semantics — this plan
  is placement-only.

## 2. Capability × surface matrix

<!-- Fixed header — parser contract. The layout is display-only: no
verb gains or loses a surface. Rows are the touched display
capabilities; the edit verbs on task/habit rows are pre-existing and
unchanged (not re-listed — the matrix lists what this feature
touches). -->

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| today.layout | — (R-1) | — (R-1) | D (US-17) | — (R-1) | — (R-1) | — (R-1) | — (R-1) |
| today.walk_order | — (R-1) | — (R-1) | D (US-18) | — (R-1) | — (R-1) | — (R-1) | — (R-1) |
| today.section_aggregate_display | — (R-1) | — (R-1) | D (pre-existing, A1 — pinned by US-17 AC-3) | — (R-1) | — (R-1) | — (R-1) | — (R-1) |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | No verb cells outside Today — the feature is a Today-view layout arm | No entity verb is created, edited, or removed; rows keep their existing surfaces | This plan §1 |
| R-2 | No kiosk two-column arm | The D-2 breakpoint exceeds kiosk display widths by construction; if a wide kiosk ever exists it inherits the desktop arm via the same width test | This plan §1 (platform scope) |
| R-3 | No Settings toggle for layout | The breakpoint is a width fact, not a preference (consistency with the board layout's breakpoint, which has no toggle) | Backlog issue filed with this plan if the operator later wants an override |

## 3. User stories

### US-17 — a wide desktop shows Today in two columns

- **Story**: As a desktop user with a large monitor I want the Today
  view to split into two columns (Overdue+Scheduled left, Habits+Day
  Review right) so that I can see the whole day without scrolling.
- **Platforms**: desktop (kiosk/narrow: single column, unchanged).
- **Entry points**: opening the Today view at a viewport width ≥ the
  D-2 breakpoint (no explicit affordance — width-driven).
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given a viewport ≥ the breakpoint and non-empty Overdue and
    Habits sections, When the Today view renders, Then Overdue and
    Scheduled share the left column and Habits and Day Review the
    right, with timers + tracked-day summary full-width above —
    verified by journey test (golden wide variant + structural fold).
  - AC-2: Given a viewport below the breakpoint, When the Today view
    renders, Then the single-column layout is unchanged — journey
    test (golden narrow variant byte-comparison).
  - AC-3: Given multiple live boards with tasks in several sections,
    When either column renders, Then each section shows the same
    aggregated all-boards row set as the single-column layout (A1
    contract composes; placement only) — fold test over the
    presentation row sources.

### US-18 — keyboard walk follows the columns

- **Story**: As a keyboard user I want the Today walk order to follow
  the two-column layout (column-major: left column top-to-bottom,
  then right) so that focus movement stays predictable.
- **Platforms**: desktop (single column below the breakpoint keeps
  the existing linear walk).
- **Entry points**: `ActionId::NavNextItem` / `ActionId::NavPrevItem`
  (the existing walk chords) while the Today view holds the walk.
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given the two-column layout, When the walk reaches the last
    row of Scheduled (left column bottom), Then the next step lands
    on the first row of Habits (right column top) — journey test.
  - AC-2: Given the walk on any row, When NavNextItem/NavPrevItem is
    dispatched, Then every row in both columns is reachable and the
    existing per-row chords (toggle, move, context) are unaffected —
    journey test (successor mapping coverage).

### US-19 — a kiosk display keeps the single column

- **Story**: When my kiosk renders the Today view I want the single-
  column layout regardless of anything so that the tuned low-power
  display posture is untouched.
- **Platforms**: kiosk.
- **Entry points**: opening the Today view on a kiosk-mode window.
- **Priority**: P1
- **Acceptance**:
  - AC-1: Given mode = Kiosk at any viewport width, When the Today
    view renders, Then the single-column arm mounts (the grid arm is
    unreachable — R-2) — fold test over the layout-arm predicate.

## 4. Journey specs

### US-17 — wide desktop renders two columns

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-17/S1 | Today view root (today-view.slint), viewport width crossing the D-2 breakpoint | resize/open the window wide (no explicit control — width-driven) | — display only (pure layout arm; no command, no request) | The section blocks re-parent into the two-column grid per D-1; the section viewport geometry channel (#1310/#1304) reports per-column extents | Below-breakpoint width → the single-column arm mounts (AC-2); a rendering/geometry regression cannot strand rows — every section block mounts in exactly one arm (fold-pinned) |
| US-17/S2 | The two-column grid, non-empty sections | scroll the page | — display only | The existing single scroll container scrolls both columns together (D-4); section collapse behaviour unchanged | Column content overflow → the page scrolls (no nested scroll region per D-4) |
| US-17/S3 | Either column's sections, multi-board state (A1 aggregation) | none — observe the rows with tasks from several live boards | — display only | Each section renders the same aggregated all-live-boards row set as the single-column layout (placement only, membership untouched) | A board going offline/empty mid-session → the aggregated row set re-folds (the existing A1 re-aggregation), columns re-flow placement only |

### US-18 — keyboard walk follows the columns

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-18/S1 | The Today walk (masked logical focus), two-column layout | press the walk-next chord | ActionId::NavNextItem | The successor mapping follows D-3: column-major; wrap Scheduled-last → Habits-first | At the walk's ends the existing clamping/wrap law applies unchanged (no new end-of-walk behavior) |
| US-18/S2 | The Today walk, any row in either column | press walk-prev / row chords | ActionId::NavPrevItem (and pre-existing per-row ActionIds) | Prev mapping mirrors Next (right column back into left); row verbs dispatch exactly as in the single column | NoTarget on a transiently empty successor slot → the typed stale-absorb posture (existing walk law) |

### US-19 — kiosk keeps the single column

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-19/S1 | Today view root, kiosk mode | none — open the view | — display only | Single-column arm mounts at any width (R-2) | n/a — the unchanged posture is the specified outcome |

## 5. Interaction flows

```mermaid
flowchart TD
  open[Open/resize Today view] --> arm{viewport width}
  arm -- ">= D-2 breakpoint ∧ desktop" --> grid["Two-column grid (D-1/D-5):
  full-width timers+tracked band,
  left: Overdue→Scheduled,
  right: Habits→Day Review"]
  arm -- "< breakpoint ∨ kiosk" --> single["Single column (unchanged)"]
  grid -- "US-17/S1" --> walk[Masked walk armed]
  single -- "US-19/S1" --> walkLin[Linear walk (unchanged)]
  walk -- "US-18/S1 column-major successor" --> walk
  walkLin -- "US-18 existing linear mapping" --> walkLin
```

No overlays, editors, or modals are touched — no stateDiagram: the
layout arm is a pure function of (mode, viewport width).

## 6. Wiring inventory

<!-- Fixed header — parser contract. The feature is display-only:
no new slint callback, no new ActionId, no command. Every row carries
"—" with the reason in Journey steps, per the template law. The
tests are the journey tests (per-plan file per the naming law) plus
the golden wide variants. -->

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | crates/taskboard-ui-slint/ui/views/today-view.slint | the section content VerticalLayout → two-column grid (pure Slint layout) | — (no callback — pure width-driven layout arm, display only) | — (no adapter seam — the arm reads only `Theme` tokens and the existing width inputs; the UI stays id-less and logic-free: the predicate is a layout expression, not state mutation) | — (no ActionId — nothing is invoked) | — (no StateCommand/UiRequest — display only) | n/a — placement of existing section blocks, no new rows | US-17/S1, US-17/S2, US-17/S3 | crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us17_s1_wide_viewport_mounts_the_two_column_grid |
| W-2 | crates/taskboard-ui-slint/ui/theme.slint | `global Theme` layout token | — (no callback — a token constant, display only) | — (no seam — the Rust→Theme injection pattern is not needed; a const token like the severing cap) | — | — | n/a | US-17/S2 | crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us17_s2_narrow_viewport_keeps_the_single_column |
| W-3 | crates/taskboard-ui-slint/src/presentation.rs (walk successor mint) / adapter walk route | the masked-walk successor mapping (pre-existing) | — (no new callback — the existing walk route's successor order is re-derived for the grid arm) | the walk route's ordering site (the adapter's existing Today walk seam — column-major successor order per D-3) | ActionId::NavNextItem | — (no new command — the existing walk route) | ItemAddr row kind (pre-existing) — the walk addresses (section, row) pairs as today | US-18/S1, US-18/S2 | crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us18_s1_walk_wraps_left_column_to_right |
| W-4 | crates/taskboard-ui-slint/tests/journeys/golden_visuals.rs | golden corpus | — (no callback — test corpus) | — (no seam) | — | — | n/a | US-17/S1, US-17/S2 | crates/taskboard-ui-slint/tests/journeys/golden_visuals.rs::the_today_two_column_wide_golden_variant_exists |

## 7. Command-contract delta

<!-- Fixed header — parser contract. Empty by design: a layout arm
dispatches nothing. Engine work items: none — C3a is pure UI
presentation + a walk-ordering derivation in the presentation layer.
That is exactly why this plan gates a UI chunk, not an engine chunk. -->

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|
| — (none) | — (no new StateCommand, UiRequest, or ActionId) | — (the walk's existing NavNextItem/NavPrevItem routes are re-ordered for the grid arm, not re-typed) | US-18 (uses pre-existing ActionIds only) |

## 8. Verification plan

- **Journey tests** (`crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs`, step-ID-named):
  - `us17_s1_wide_viewport_mounts_the_two_column_grid` (US-17/S1, AC-1)
  - `us17_s2_narrow_viewport_keeps_the_single_column` (US-17/S2, AC-2)
  - `us17_s3_aggregated_row_sets_unchanged_in_both_columns` (US-17/S3, AC-3)
  - `us18_s1_walk_wraps_left_column_to_right` (US-18/S1, AC-1)
  - `us18_s2_prev_mapping_mirrors_and_row_chords_hold` (US-18/S2, AC-2)
  - `us19_s1_kiosk_mounts_the_grid_when_space_allows` (US-19/S1, AC-1,
    the 2026-11-25 amendment: the width test alone rules every mount;
    the pre-amendment `us19_s1_kiosk_stays_single_column` is
    superseded) — plus the posture twin
    `us19_s1_kiosk_posture_mounts_the_grid_when_space_allows`
    (kiosk UiSettings mount).

  Step→test bounds (the #768 grammar; the §6 Test column nominates
  one test per wiring row, these bind the remaining steps):

bound-test: US-17/S3 -> crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us17_s3_aggregated_row_sets_unchanged_in_both_columns
bound-test: US-18/S2 -> crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us18_s2_prev_mapping_mirrors_and_row_chords_hold
bound-test: US-19/S1 -> crates/taskboard-ui-slint/tests/journeys/today_two_column_journeys.rs::us19_s1_kiosk_mounts_the_grid_when_space_allows
- **Seam tests**: none needed — no new seams; W-3's ordering is
  asserted through the journey test's successor sequence (typed
  row-kind sequence, not label text).
- **Fold/presentation tests**: the layout-arm predicate (mode, width)
  folds to the mounted arm — `us19_s1` is that fold; AC-3's
  aggregation pin is a fold over the presentation row sources.
- **Kiosk gates**: US-19/S1 (single column at any kiosk width).
- **Manual**: the visual balance of 50/50 columns and the empty-
  column whitespace at very large widths — walkthrough protocol at
  stage 3.

## 9. Definition of done

- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0
- [ ] Every P1 story's journey completes end-to-end including escapes
  (US-17–US-19)
- [ ] Walkthrough protocol executed at stage 3; findings resolved or
  homed
- [ ] All matrix absences carry rulings (R-1…R-3); census updated
  (the Today layout rows note the two-column arm + breakpoint token)
- [ ] Non-goals filed as backlog issues (resizable widths; section
  arrangement; layout override — filed with this plan)
