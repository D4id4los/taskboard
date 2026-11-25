# The Severing/Geometry Completions Chunk Plan (round 7, chunk B1)

- **Date**: 2026-11-25
- **Branch**: `fix/severing-geometry-completions` (worktree
  `../taskboard-WT-fix-severing-geometry-completions`, cut from fetched
  `origin/main` per #1185)
- **Covers** (dispatch plan §6, normative): #1378, #1334, #1365, #1320,
  #1353
- **Chunk class**: bug fixes riding existing surfaces — **no UX journey
  plan** (dispatch plan §1 table: "no UX plan (bug fixes)").
- **Round laws**: [L](a) every fix pinned by a test, never prose;
  [L](c) #1334 is golden-truth — root-cause the offset, fix it, and
  re-record the golden as evidence-carried; [L](d) out-of-scope
  findings become `backlog:` issues immediately.
- **Long-command discipline**: pre-commit hooks + flock-wrapped
  `scripts/run-quality-gates.sh cargo nextest run --workspace`; every
  cargo call through `scripts/memory-gate.sh`; commit landed work
  before any wait; if the gate lock is held and no useful work
  remains, commit and report "gate deferred: lock held by <holder>".

## 0. Landed base (read during stage 1)

- **E1 census guard**: `crates/taskboard-ui-slint/tests/inventory/
  severing_census.rs` — filesystem source-scan over `ui/views/`;
  boundary note in its header already names `ui/app-window.slint` as
  the unsevered out-of-boundary window ("backlog issue" = #1378).
  Out-of-boundary probes are admitted via `OUT_OF_BOUNDARY_PROBES`
  (today: `alarm_window_max_probe` only).
- **Probe pattern**: `build.rs` `*_window_max_probe` array + probe
  `.slint` + asserted tests module under `tests/probes/` (e.g.
  `today_window_max_probe_tests.rs`, asserting the root's horizontal
  `layout_info().max >= 99_999.0` after `ensure_item_tree_instantiated`).
- **Focus-scroll contract (#1302)**: global `FocusScrollHint`
  (`ui/focus-scroll.slint`: `target-a`/`target-b`/`nonce`);
  `adapter.rs::push_focus_scroll_hint` resolves the landing through
  `IdTables` (global model indices) and edge-triggers; the owning
  container filters (`target-a == sindex`) and seeks. The A5a page-seek
  precedent (`seek-page`, commit 83e5f7af) lives in
  `today-view.slint` (synthetic slots 4/5).
- **Declarative today rework**: `today-view.slint` —
  `TodayViewport.height` imperative assignment severs the
  window-height↔cap loop; `TodayClipFacts` global carries the
  per-section measured extents (`e0..e3`, adapter push pass) in and
  mirrors realized heights (`h0..h3`) out via a pure callback; clip
  heights bind to `TodayClipFacts.section-height(sindex, …)`.
- **Overflow regime**: `src/widths.rs::resolve_row` renders explicit
  column widths summing to the row; when Σ minimums > row width every
  column resolves to its minimum and "the row clips" (decision 6's
  recorded fallback). `ui/compositions/board-layout.slint` ~302: a
  plain `HorizontalLayout`, no `Flickable`, no `content-x`.
- **PlanNextDay**: `adapter.rs` `push_focus_scroll_hint` returns the
  typed inert for `ViewId::PlanNextDay` ("fixed surface, #622");
  `ui/views/plan-next-day-view.slint` ~88 wraps the panes in a
  `Flickable` that can overflow; `models.rs` already carries
  `plan_position` / `plan_row` (`plan_rows` flat table + placed-head
  count).
- **Alarm drag strip**: `ui/alarm-window.slint` `drag-strip :=
  Rectangle` — `background: strip-ta.pressed ? Theme.surface :
  transparent`, grab/grabbing cursor only; no at-rest affordance.
- **Golden infra**: `tests/journeys/golden_visuals.rs` captures
  `kiosk-today-0.5-1000.png` (scale 0.5, width 1000, one forced
  rotation dwell) and upserts `tests/goldens/manifest.toml`
  (`golden_manifest`, bijection test).

## 1. Item #1378 — AppWindow carries no severing max-width

**Root cause**: `ui/app-window.slint`'s root (`AppWindow inherits
Window`, line 168 in the round-7 base) states `min-width`/`min-height`
floors and `preferred-*` but no `max-width:
Theme.layout-severing-max-width`. Every view it hosts is itself a
severed census root, so the composition may currently be safe — but
the Window's own layout merge is the last unguarded merge point of
exactly the class #778/#1197/#1251 collapsed through: any future
row/section whose max reaches the root's merge clamps the OS window's
max size once mounted. The E1 guard fails closed only inside
`ui/views/`, so nothing would catch it.

**Fix shape**:
1. Add `max-width: Theme.layout-severing-max-width;` to the root
   property block of `AppWindow` (the second, real component at line
   168 — there is also a small forward-declared `AppWindow` stub above
   it; only the exported Window root gets the sever).
2. Mint the twin: probe `.slint` `tests/app_window_max_probe.slint`
   (mirrors `today_window_max_probe.slint` — mounts the real
   `AppWindow`), a `build.rs` array entry, and an asserted tests
   module `tests/probes/app_window_max_probe_tests.rs` with the same
   `h.max >= 99_999.0` invariant.
3. Extend the census guard: add `"alarm_window_max_probe"`'s sibling —
   `OUT_OF_BOUNDARY_PROBES` gains `"app_window_max_probe"`, and the
   census header's boundary paragraph flips from "carries no sever
   today (backlog issue)" to "severed + twin, witnessed by
   `app_window_max_probe_tests`" (the witnessed-safety-analysis ask in
   the issue is discharged by the probe itself, stronger than prose).

**Pin**: structural — the probe test (fails on the pre-fix root if a
bounded-max row ever reaches the merge) + the census `OUT_OF_BOUNDARY_
PROBES` entry (a deleted sever/twin fails the inventory loudly via the
existing stale-twin machinery).

**File scope**: `ui/app-window.slint`, `build.rs`,
`tests/app_window_max_probe.slint`,
`tests/probes/app_window_max_probe_tests.rs`,
`tests/inventory/severing_census.rs`, `tests/probe_composition.rs`
(mod registration).

**Decision D-1378**: sever + twin, not exemption. An exemption would
need a witnessed analysis re-proven at every future view addition; the
sever is one line and the twin reuses the landed probe pattern. Note
#1382 (the `plan_window_max_probe` name drift) is B3's chunk — the new
twin uses the derived-stem law (`app_window_max_probe`), consistent
with either resolution of that drift.

## 2. Item #1334 — kiosk-today golden's ~one-row vertical spacing offset (golden truth)

**Root cause hypothesis space** (stage 2 confirms before touching
anything; the issue's two suspects plus one added by this read):
1. **Extent-unit mismatch at scale 0.5** (this plan's primary): the
   adapter's `TodayClipFacts` extent push pass measures row/section
   extents in physical px while the kiosk mount renders at
   `base_scale = 0.5` (or vice versa — logical lengths written where
   physical are read). At scale 1.0 the two agree (the desktop goldens
   hold, exactly the observed asymmetry); at 0.5 a per-section
   round-up of `ceil(extent / 0.5)` or an unscaled gap inflates each
   section viewport by a residual that accumulates across the
   summary→Overdue boundary (~75 px ≈ one half-scale row + gap, ~48 px
   ≈ header-to-row gap at 0.5 scale — consistent with a per-section
   constant inflation, not a one-off).
2. The #592 row-anchored compensation residual re-ordering against the
   new synchronous extent push (pipeline pass ordering on the forced
   rotation flip: `advance_time(1_000)` flips mode before the first
   extent push settles).
3. A kiosk-only layout-info path: the kiosk mount's
   `TodayViewport.height` init assignment lands before the flip's
   model push, and the first-frame extent read caches a pre-flip
   value.

**Fix shape** (stage 2 procedure, decisions pre-recorded):
- **D-1334a (verdict law)**: the offset is a *regression* if any
  spacing at kiosk scale is computed from a unit-mismatched or stale
  extent; it is *acceptable re-spacing* only if every gap provably
  comes from the intended `Theme.space-*` ladder at scale 0.5. A
  "the pixels moved but nothing is wrong" verdict requires naming the
  exact binding whose value legitimately changed — otherwise fix.
- **D-1334b (fix locus)**: fix in the extent pipeline
  (`adapter.rs` push pass / `today-view.slint` `TodayClipFacts`
  consumers), never in the golden tolerance and never in the capture
  harness. The golden is evidence, not the adjustable.
- **D-1334c (re-record)**: after the fix, re-record
  `kiosk-today-0.5-1000.png` via the regen flag; the manifest row's
  `state`/`fixture` text stays; the dated census note (the issue's
  ask) goes into `severing_census.rs`'s sibling inventory doc or the
  golden manifest's note field — wherever the landed manifest schema
  keeps notes (stage 2 checks `support::golden_manifest`; if no note
  field exists, the verdict is recorded in this plan's stage-2 report
  and the census header comment).
- **D-1334d (structural pin)**: beyond the golden, pin the unit law
  where it was violated: a typed test asserting the extent push pass
  writes extents in the same unit the clip-height binding consumes at
  a non-1.0 scale (e.g. set kiosk scale 0.5 on the probe mount, push a
  known row set, assert `TodayClipFacts.h0 ==` the expected
  logical-length sum — no image diffing in the unit pin; the golden
  stays the visual evidence).

**Interaction with G1 (binding note)**: chunk G1
(`test/golden-determinism-rebase`) is **not merged** as of this
stage-1 (no PR open; the worktree sits with uncommitted changes on the
re-base). Per the dispatch order (wave 2 runs G1 ∥ B1), B1 plans its
re-record **against current main's golden corpus** and observes:
- If G1 lands first: rebase this branch onto the re-based corpus
  before re-recording kiosk-today — the enriched fixture changes the
  kiosk-today golden's content, and re-recording against the stale
  fixture would force a second re-record and a confusing delta.
- If B1 lands first: G1's re-base re-records kiosk-today again on top
  of this fix; G1's law (c) already assigns geometry-truth deltas to
  B1, so a residual spacing delta after G1's re-base traces here.
- Either way: **never** re-record with a tolerance change; the
  comparator tolerance is untouched (L(c)).
Stage 2 must re-check G1's merged state before the re-record commit
and note which ordering held.

**File scope**: `crates/taskboard-ui-slint/src/adapter.rs` (extent
push pass, likely locus), `ui/views/today-view.slint` (consumers, only
if the fix lands binding-side), `tests/goldens/kiosk-today-0.5-1000.png`
(re-record), `tests/goldens/manifest.toml` (upsert), one new typed pin
test (probe-side, file per the landed probe layout).

## 3. Item #1365 — board columns row has no horizontal scroll container

**Root cause**: `resolve_row` (decision 6's recorded fallback) emits
explicit widths summing to the row width; the columns container
(`board-layout.slint` ~302, plain `HorizontalLayout`) has no
`Flickable`/`content-x`, so in the overflow regime (Σ minimums > row
width) columns past the right edge are clipped and unreachable by
pointer *and* keyboard — a focused card in a clipped column cannot be
scrolled into view (the A5a D-4 residual).

**Fix shape**:
1. Wrap the columns `HorizontalLayout` in a horizontal `Flickable`
   whose `content-width` is the resolved Σ-minimums when the row is in
   the overflow regime, and == row width in the fitting regime (no
   dead scroll range when everything fits). The row-width oracle for
   `resolve_row` stays the viewport width (the Flickable's own
   viewport), not the content width — the resolver's contract is
   unchanged.
2. Keyboard reachability: extend the `FocusScrollHint` consumption to
   the board columns container — `push_focus_scroll_hint`'s `Board`
   arm already emits `(column, row)`; the Flickable (or its viewport
   wrapper) consumes `target-a == column` and seeks the column into
   view (`seek-page` precedent, structural seek by index, never
   pixel-text).
3. Divider-drag geometry: the drag seams capture
   `DragState.pointer-x` from `absolute-position`, which survives
   `Flickable` translation (verified in the issue body); stage 2
   confirms the divider-drag hit path under a scrolled viewport and
   re-records the width-resolver test fixtures only if the oracle
   input actually changed (D-1365a: **it should not** — the resolver
   reads viewport width; if stage 2 finds the oracle reading the
   clipped row width instead, that is a second bug → fix + pin
   separately).
4. Overflow-regime scrollbar affordance per the widget grammar
   (B2's centralized scrollbar — #1359 tracks the widget; use the
   current grammar's scrollbar spelling, do not hand-roll a new one).

**Pin**: structural scroll-container pin — a probe/structural test
asserting the columns row's container is a scroll container with
`content-width >= viewport-width + Σ(overflow)` in the overflow
regime and `content-width == viewport-width` in the fitting regime,
plus a focus-law journey assertion that a keyboard move into an
overflow-clipped column changes the container's `content-x` (the #1302
pin pattern at the new container). No text assertions; typed
properties only.

**File scope**: `ui/compositions/board-layout.slint`,
`ui/views/board-view.slint` (hint consumption wiring, if the
consumption lands view-side), possibly `src/adapter.rs` (only if the
Board hint arm needs the column-resolution tweak), new structural test
module, `tests/journeys/focus_law_journeys.rs` (the keyboard-reach
journey).

**Decision D-1365b**: scroll container, not reflow/wrap. Wrapping
columns breaks the divider-drag width law (a wrapped column's width is
not row-relative) and contradicts the board metaphor; the issue's
proposed shape (scroll container + hint consumer) is adopted.

## 4. Item #1320 — PlanNextDay's pane Flickable never scrolls focus into view

**Root cause**: `push_focus_scroll_hint` returns the typed inert for
`ViewId::PlanNextDay` (the #622 "fixed paging surface" decision), but
the surface's panes live in a `Flickable`
(`plan-next-day-view.slint` ~88) whose content overflows with many
placed boxes / a long pool, and the J3 R-5 pane walk can move focus
below the fold. The #1302 law (focused row always scrolled into view)
is violated on this surface.

**Fix shape**:
1. Remove `PlanNextDay` from the inert arm in
   `push_focus_scroll_hint`; resolve the landing through the existing
   tables — `models.rs::plan_position(task) -> Some((pane, row))` —
   and emit `(pane, row)` on the hint. The pane convention: `target-a`
   = pane index, `target-b` = row within the pane (the #622 fixed-page
   decision is *not* reopened — the surface keeps no paging hint;
   only the focus-into-view hint is added; D-1320a records this as the
   narrowed exception: #622 D-1 stays authoritative for paging, the
   focus law wins for focus visibility).
2. `plan-next-day-view.slint`: each pane's `Flickable` consumes
   `FocusScrollHint.target-a == pane-index` and seeks
   `target-b`'s row into view (row-index arithmetic on the pane's
   repeater geometry, the Today section ListView's consumption
   pattern — `seek-nonce` edge-trigger + structural seek).
3. The stale-hint guard (`rotation_tick`'s nonce reset) already
   covers the global; no adapter-side guard changes.

**Pin**: the #1302 pin pattern — a focus-law journey on the planner
surface: seed enough placed boxes to overflow the pane, keyboard-walk
past the fold, assert the pane's viewport-y moved so the focused row's
index is inside the viewport (structural index-in-viewport assertion,
not pixel/text). Plus a unit pin that `push_focus_scroll_hint` on a
`PlanNextDay` landing emits the `(pane, row)` pair (typed assertion on
the global's properties after the walk).

**File scope**: `src/adapter.rs` (the inert arm), 
`ui/views/plan-next-day-view.slint`, `src/models.rs` (only if
`plan_position` needs the pane split exposed — it already returns
`(pane, row)`; likely zero change), new journey + unit pins.

**Decision D-1320b**: seek, not restrict. The alternative recorded in
the issue ("the pane walk must not enter off-viewport rows") would
break the walk's totality law (R-5's ends-hold) and degrade the
keyboard model; the seek reuses a landed contract and touches two
files.

## 5. Item #1353 — alarm drag strip has no at-rest visual cue

**Root cause**: `drag-strip := Rectangle`
(`ui/alarm-window.slint` ~101) — `background: strip-ta.pressed ?
Theme.surface : transparent`, `height: Theme.space-2`; the only rest
affordance is the grab cursor on hover. Recognition-over-recall
failure: a first-time user cannot see the top edge moves the window.

**Fix shape** (of the issue's three directions, D-1353a adopts the
grip affordance; the one-time hint and the #1321 persisted-placement
fold are deferred — a one-time hint needs a persistence seam out of
scope, and #1321 is its own chunk):
- A subtle grip dot row at rest: a centered row of small dots
  (`Theme.surface-variant`-grade token, per the widget grammar's
  hairline/dot tokens — stage 2 picks the exact token against the
  Theme ladder; no raw colors, per the B2 token law) rendered inside
  `drag-strip`, dimming on press into the existing
  `Theme.surface` tint (the pressed state stays; the dots fade so the
  two cues don't stack).
- The alarm-window structural test already pins the window's shape;
  the cue rides inside the existing strip element — no layout-mass
  change (the strip keeps `height: Theme.space-2`; the dots are
  painted children, zero new geometry claims).

**Pin**: structural — extend the alarm window's structural test
(`alarm_window_structural`-adjacent module; stage 2 locates the
landed file) asserting the drag strip contains the at-rest cue element
with a non-transparent rest color and the pressed-state transition
(typed property assertions on the instantiated element — the probe
mount pattern; no screenshot, no text).

**File scope**: `ui/alarm-window.slint`, the alarm structural test
module. *(If the token ladder lacks a suitable dot token, add the
token to the Theme file — a token, never a raw color.)*

**Decision D-1353b**: grip dots, not first-mount hint. Cheapest
visible affordance, no persistence seam, no new state.

## 6. Execution order & phases

1. **Phase 1 — #1378** (independent, no golden interaction): sever +
   probe twin + census extension. Commit.
2. **Phase 2 — #1334** (the diagnostic-heavy item): reproduce the
   offset via the regen path, instrument `TodayClipFacts` at kiosk
   scale, verdict per D-1334a, fix per D-1334b, typed pin per
   D-1334d, re-record per D-1334c, census note. Commit. (Re-check G1's
   merged state first — the interaction note in §2.)
3. **Phase 3 — #1320** (adapter + one view; smallest of the two hint
   items): inert-arm removal + pane consumption + pins. Commit.
4. **Phase 4 — #1365** (the largest geometry change): Flickable wrap +
   hint consumption + structural/journey pins + drag-seam
   verification. Commit.
5. **Phase 5 — #1353** (cosmetic + structural pin). Commit.
6. **Gate**: `scripts/run-quality-gates.sh cargo nextest run
   --workspace` (+ fmt/clippy via the pre-commit hooks on each
   commit). If the lock is held at the end: commit everything, report
   "gate deferred: lock held by <holder>", end the stage.

Rationale for the order: #1334 before #1365 because the analyzer (G3)
fires on golden spacing first and #1334 feeds G3 directly; #1320
before #1365 because both touch hint consumption and the planner's is
the simpler container; #1353 last as it is cosmetic and independent.

## 7. Risks

- **#1334's verdict**: if stage 2's verdict is "acceptable re-spacing"
  rather than a unit bug, D-1334a requires naming the exact legitimate
  binding change — a vague verdict is not shippable; escalate to the
  dispatcher rather than re-recording with prose justification.
- **#1365 drag seams**: the divider-drag hit path under a scrolled
  viewport is the one place the issue's own "survives translation"
  claim could fail (the handle's `absolute-position` arithmetic); if
  it fails, the fix is in-scope (the drag geometry is this chunk's
  subject) but grows the diff — budget an extra commit.
- **#1365 × #1302**: the board hint consumption adds a second consumer
  filtering on `target-a == column`; the one-view-tree law (Q4) keeps
  consumers unambiguous, but the stage-2 pin must assert the board
  consumption does not double-fire with the card-level seek (the
  board cards' existing consumption, if any — stage 2 inventories the
  board's current hint consumers before wiring).
- **G1 ordering**: the §2 interaction note; re-record kiosk-today only
  on the corpus this branch will be merged against.
- **Golden determinism**: the re-record must be byte-stable across two
  regens (G1's [L](a) law applied locally even before G1 lands).

## 8. Estimated diff size

| Item | Code | Tests/pins | Goldens |
|---|---|---|---|
| #1378 | ~5 (sever + build.rs) | ~90 (probe .slint + tests + census) | — |
| #1334 | ~10–60 (verdict-dependent) | ~60 (typed unit pin) | 1 PNG + manifest |
| #1320 | ~40 (adapter + view) | ~80 (journey + unit) | — |
| #1365 | ~80–140 (Flickable + consumption) | ~120 (structural + journey) | — |
| #1353 | ~25 (dots + token) | ~40 (structural) | — |

Total: ~450–600 lines, one PR (one coherent deliverable; the five
commits ride one branch per dispatch plan §1).

## 9. Decisions index

- D-1378: sever + twin, not exemption (§1).
- D-1334a: regression-vs-acceptable verdict law (§2).
- D-1334b: fix locus = the extent pipeline, never the tolerance (§2).
- D-1334c: re-record against the corpus this branch merges against;
  G1 state checked at stage 2 (§2).
- D-1334d: typed unit-law pin alongside the golden (§2).
- D-1365a: `resolve_row`'s oracle input must not change; an oracle
  reading the clipped width is a second bug (§3).
- D-1365b: scroll container, not reflow/wrap (§3).
- D-1320a: #622's fixed-page paging decision stands; only the
  focus-into-view hint is added (§4).
- D-1320b: seek, not walk restriction (§4).
- D-1353a: grip dots, not first-mount hint / #1321 fold (§5).
- D-1353b: cue token from the Theme ladder, never a raw color (§5).
