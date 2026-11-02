# [Plan] Kiosk board i16 overflow (#462, rider #536) — chunk 3.1 implementation plan (stage 1)

- **Date**: the artefact name carries the dispatch date (2026-10-31,
  per the dispatch prompt's mandated path); **written 2026-11-02** on
  the chunk's worktree.
- **Kind**: stage-1 TDD implementation plan for chunk 3.1 of backlog
  round 2. Zero production code in this commit; this file is the
  chunk's planning artefact. Implementation is stage 2's (dispatch
  prompt 3.1.b), review + shipping stage 3's (prompt 3.1.c).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §1 (the batch table's render-lane seriality) + §2 chunk 3.1 (the
  Covers/Acceptance list is binding), dispatching the backlog issues
  **#462** (the crash: mechanism facts, empirical bracket, Today
  contrast, fix levers) and **#536** (the rider: the testing
  backend's empty-element-query limitation), on the evidence base of
  the ListView design-round report
  `.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`,
  the view-pinned re-measure
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`,
  and the landed #373 round's stage-2 report
  `.artifacts/reports/2026-10-26-listview-virtualisation-stage2-report.md`
  (with its §9 stage-3 corrections).
- **Normative base**: `origin/main` = `67c0b531` (the worktree's
  branch point; carries the landed #373 mechanism, the round-4
  divider/icon chunks, and the render lane's chunk 2.1 row-diff
  mirror `a3987d11`).
- **Gate (verified)**: chunk 2.1 is merged — the dispatch prompt says
  "PR #464", but no PR #464 exists in this repository; the chunk is
  identified by its branch and title, `perf/ui-row-diff-mirror`, and
  that is **PR #574, merged 2026-11-01T23:06Z** (commit `a3987d11`
  in `origin/main`). The render lane (1.3 → 2.1 → 3.1 → 4.1,
  strictly serial) is unblocked; chunk 4.1 must not start before this
  chunk's PR lands.
- **The premise finding that shapes this plan** (§3.0): #462's
  structural fix lever — the #373 board-column virtualisation — is
  **already landed on `main`** (2026-10-26, five days before the
  dispatch plan was cut), bench-proven at N = 10 000 by its stage-2
  round. Chunk 3.1 is therefore a **proof, pin, and documentation
  chunk**: gate the survival property in CI, refresh the bench
  evidence on the current tree, land the mandated upstream report and
  CHANGELOG entry — not a mechanism-implementation chunk. The dispatch
  prompt 3.1.b step 2 wording ("the board columns onto the ListView
  mechanism") predates that discovery; the deviation is resolved at
  D-1/D-10 below.

## 1. Mission & binding constraints

The dispatch plan §2 chunk 3.1 acceptance list, each item mapped to
its state on the branch base:

| Acceptance item | State at `67c0b531` | This chunk's work |
|---|---|---|
| A ≥2.5k-task board renders on the software renderer (bench evidence at the formerly-panicking scale) | Proven once, by the #373 M4 run (stage-2 report §4: `draw-full/board/10000` = 2.78 ms; N = 10 000 = 2 500 rows/column ≫ the bracket's 650-rows/column panic point) — but only in a report, and on a five-day-old tree | **Re-prove on the final tree**: the full bench matrix re-run (M-B) + a CI-gated render pin at the exact recorded panic scale (M-A) |
| No panic at the N = 10 000 draw lanes | The lanes exist un-skipped (M4 deleted the skip); last recorded 2026-10-26 | Re-run them (M-B), record verbatim output |
| The bench's 400-rows/column loud-skip bound updated | **Done at M4**: the constant and both skip blocks deleted; the bench docs state the ceiling is "unreachable by construction" (`benches/refresh_draw.rs:30-31, 90`) | Verify (grep record + a matrix output with zero SKIPPED lines); fix the one stale comment the deletion left behind (F10 → M-B) |
| Board journeys/goldens updated honestly | **Done by #373**: six board goldens regenerated with diagnosed diffs (the D-4 deviation's geometry); journeys pass | Verify green unmodified; no new golden (D-4) |
| The upstream report lands (diff = code + that report only) | Not started | **M-C** — the chunk's central deliverable |
| CHANGELOG: one bug-fix entry (the kiosk panic) | The Added entry (#373, virtualised task lists) already names #462 as the eager board's ceiling | **M-D** — the short `### Fixed` entry the prompt mandates |

Binding constraints:

- **The #373 round's pinned semantics are binding and must hold
  unchanged** (dispatch prompt 3.1.b hard rule): the reading-order
  walk (state-side, `view_state.rs` `step_flat_domain` over
  `AppState` — virtualisation cannot touch it), focus restoration by
  id, the IdTables global-model-index convention
  (`crates/taskboard-ui-slint/src/models.rs:52-57`, the documented
  invariant), viewport-scale instantiation on both board legs, the
  drag seams' index-only convention (D2), and scroll-position
  survival across refreshes (persistent handles; the row-diff mirror
  keeps `reset()` off the refresh path). The pins that enforce these
  are enumerated in F5; stage 2 runs them, stage 3 reviews against
  them.
- **AGENTS §5 testing rules**: no text assertions; no sleeps;
  deterministic by construction; no quality ceilings (the engagement
  ratios are failure detectors, never success ceilings); one
  behavior, one test, lowest layer; a new test failing may be a
  production bug.
- **The evidence & artefact mandate** (dispatch plan §1, binding for
  the upstream report): every factual claim carries evidence (source
  cites, verbatim output, or the recorded reports); the method
  reproducible; exactly one decision-grade recommendation; scratch
  never lands — the PR diff is exactly the chunk's artefacts (code
  commits + the one report + this plan).
- **The bench's instrument honesty rules** (the #460/#460-second-round
  discipline): a lane whose guaranteed change renders nothing panics,
  never measures nothing; skip lines are loud eprintlns, never hidden
  cells; times are dev-machine references, never gates.
- **Lane & territory**: the render lane is serial (gate verified);
  the UI-territory rule gives this chunk the board territory
  (`board-layout.slint` / `push_board`) — chunk 3.2–3.6 and 4.x must
  not co-touch it while this branch is in flight.

## 2. Verified baseline — numbered facts

Every fact below was verified on the branch base `67c0b531` by this
planning round (file + line anchors are the worktree's, i.e.
origin/main's line numbers).

- **F1 — the i16 mechanism (from #462 + the re-measure §5, recorded
  2026-10-26)**: slint 1.18.1's software renderer's entire geometry
  space is `i16` physical pixels
  (`i-slint-renderer-software-1.18.1/lib.rs:63-67`:
  `type PhysicalLength = euclid::Length<i16, PhysicalPx>`;
  `PhysicalPoint`/`PhysicalSize` over `i16` likewise). With the
  workspace's deliberate `overflow-checks = true` in the release
  profile (the #303 standing decision, "overflow panics stay loud"),
  a rendered item whose geometry add exceeds ±32 767 px aborts the
  process. Recorded panic chain: `euclid::Length<i16>::add` ←
  `BasicBorderRectangle::render` ← `Repeater<…drop_zone_98>::visit`
  (the regular leg's drop-zone repeater). Empirical bracket
  (1366×768, dev-profile scratch run, four-column round-robin
  fixture): **N = 2 000 (500 rows/column) renders; N = 2 600 (650
  rows/column) panics**.
- **F2 — production reachability**: the kiosk appliance runs the
  linuxkms backend on this software renderer; `Board` is the kiosk
  default view; the release profile ships overflow-checks — a kiosk
  board accumulating ~2.5k+ tasks aborts on first board render.
  Desktop (winit/femtovg) uses a different renderer and was never
  affected.
- **F3 — the structural fix is landed**: the in-house `ListView`
  (`ui/widgets/listview.slint` — the five-property nominal compiler
  contract plus the `scroll-row-into-view`/`page` nonce seams) wraps
  the board's card loops on **both legs**: regular
  `board-layout.slint:489` (`lv`, delegate `drag-zone` at :513) and
  compact :819 (`task-viewport`, delegate `compact-task` at :841).
  Landed 2026-10-26: `2411f305` (component + engagement probe),
  `85f0cd85` (the board columns through the ListView), Today/stack
  surfaces in the same round; the stage-3 review round landed the
  stack-detail leg and corrected the stage-2 report (§9). The outer
  per-column `drop-zone := DropArea` repeater
  (`board-layout.slint:273`) is untouched — the drop target stays
  column-granular.
- **F4 — the board already measures at the former crash scale**: the
  #373 M4 run (stage-2 report §4, dev-box references):
  `draw-full/board/10000` = 2.78 ms, `draw-partial/board/10000` =
  1.76 ms — the cells that hard-aborted before (re-measure §2:
  SKIPPED; §5: the panic). Every draw lane became N-independent;
  the stage-2 report records "#462 closed by construction".
- **F5 — the binding pins exist** (`tests/virtualisation.rs`): the
  eager control (300/300 instances — the counter works and the eager
  shape is what we think); `the_list_view_engages_the_virtual_repeater`
  (the ×4 failure-detector ratio; guards the nominal contract by
  runtime); `the_board_regular_leg_virtualises` and
  `the_board_compact_leg_virtualises` (viewport-scale at N = 1 000);
  `a_keyboard_jump_to_the_last_card_makes_it_fully_visible` (the
  seek) and `an_adjacent_step_leaves_the_view_resting` (the est-range
  guard); `a_pointer_drag_across_virtualised_columns_moves_the_
  tabled_task` (drag seams resolve `board_task(0, 0)` exactly at
  virtualised N = 1 000); the Today anchor-on-removal pin
  (`a_removal_above_the_today_viewport_leaves_the_rows_resting`,
  #461's acceptance, landed early by #373).
- **F6 — IdTables**: the invariant is documented at the type
  (`models.rs:52-57`): every row index a view reports or consumes is
  a **global model index** into the pass's tables; the ListView
  repeater changes which rows are instantiated, never what the
  indices mean. `board_task(column, row)` and the additive reverse
  helpers (`board_position`/`today_position`/`stack_position`) are
  unit-tested round-trip.
- **F7 — the push path composes** (post-2.1 row-diff mirror,
  `models.rs` `push_board` ~:983): persistent per-stack
  `MirroredRows` handles diffed against mirrors (`diff_rows`), the
  outer column row firing only on real scalar/handle change — a
  virtualised repeater under a persistent handle never takes the
  `reset()` full re-instantiation path on refresh, so scroll position
  survives refreshes by construction (design round §3c/§5).
- **F8 — goldens**: the six board goldens
  (`tests/goldens/board-<scale>-<breakpoint>.png`) pin the
  post-virtualisation geometry (the Add-Task ghost and inline create
  field at the column's bottom edge — the recorded D-4 deviation);
  the golden tier renders the clean two-stack fixture at scale
  extremes × breakpoints, never over-deep content. Regen protocol:
  `TASKBOARD_REGEN_GOLDENS=1 … nextest --test golden_visuals`,
  fail-after, PNG diff is the review artefact.
- **F9 — the bench instrument** (`benches/refresh_draw.rs` +
  `benches/support/mod.rs`): `N_SIZES = [100, 1_000, 10_000]`,
  `STACK_COUNT = 4` → rows/column = N/4 = 25/250/**2 500**;
  `VIEW_PINS = [("today", false), ("board", true)]` (the mount's
  kiosk lever); the partial lanes rotate over rendered head rows
  (`FLIP_HEAD_ROWS = 6`) with the preamble visible-flip guard (a
  zero-region flip panics the lane); region carriers print loudly
  (max = honest worst case, last iteration alongside); the 10 000
  tier runs sample_size 10 × 15 s per cell. The file header (:29-31)
  and `VIEW_PINS` doc (:88-92) already document the ceiling as
  unreachable under virtualisation and the skip as gone.
- **F10 — one live-wrong bench comment**: `benches/support/mod.rs:206-210`
  (`board_flip_targets` doc) still says "The board renders all N
  tasks eagerly" — false since F3. The flip-shape claim itself (a
  board head-row done flip is an *in-place* card change) remains
  true. This is the bench-bound acceptance item's residual.
- **F11 — the survival property is not CI-gated today**: the
  engagement tests instantiate at N = 1 000 (250 rows/column ≈ 15k px
  estimated content — below the ceiling) with
  `headless_renderer: false` (no rasterization); the golden tier
  rasterizes but at fixture scale; the bench rasterizes at scale but
  is a criterion target outside the AGENTS §10 gate suite. So the
  property "a board beyond the i16 ceiling renders" is
  report-documented and bench-documented, but no `cargo nextest` run
  would fail if the ListView disengaged *and* something else also
  regressed. (The engagement probes alone do gate disengagement at
  N = 300 — F5 — so the realistic re-entry is already guarded; the
  deep pin closes the remaining composition gap: fixture + mount +
  virtualised board + raster at the recorded crash scale, end to end.)
- **F12 — the #536 facts** (the rider): with a second conditional
  control mounted in the chrome row (the tracking chip), the testing
  backend's element-traversal query for `SettingsView` returns empty;
  `an_info_tip_hover_arms_the_generalized_tooltip`
  (`tests/journeys.rs` ~:4034) works around it by clearing the
  fixture's open tracking entry, with a comment naming the
  limitation. The #373 round add: the same traversal yields one node
  once per multi-element ancestor path (the retracted stop report's
  11×/14× instance counts — explained by the stage-2 report §0;
  `find_all_by_id` now dedupes by node geometry,
  `tests/support/driver.rs`). **Retracted and excluded from upstream**
  (D-7): the "empty delegate ids behind components" claim (stage-2
  §9 correction 2 — the observation had targeted the wrong surface).
- **F13 — genuine slint 1.18.1 behavior findings from the #373 round**
  (stage-2 report §2, all code-anchored, none retracted): Flickable
  reports no content-derived layout info (`items.rs` `layout_info`
  default, preferred 0); a ListView's model may not flow through a
  child component's `in property` (zero instances, timing-independent,
  isolated in the probe suite's InnerList shape); the compiler's
  conservatively-detected `height` ↔ `content-height` binding loop
  (statically rejected even through function indirection; the Today
  viewports decouple it via change-handler assignment).
- **F14 — the driver's raster seam for the pin**:
  `DriverOptions { headless_renderer: true }` embeds the software
  rasterizer so `Window::take_snapshot` renders real pixels
  (`tests/support/driver.rs:46-50, 86, 276`); the renderer-presence
  assertion is a hard failure, never a skip (the golden tier's
  convention, `golden_visuals.rs:110-118`).
- **F15 — CHANGELOG**: the Unreleased `### Added` entry "Virtualised
  task lists (#373)" already names #462 ("the eager board at that
  scale could not render at all — the #462 i16 ceiling"); no
  `### Fixed` entry for the kiosk panic exists.

## 3. The design, milestone by milestone

### 3.0 The lever decision (the premise resolution)

#462's levers, adjudicated against the landed tree:

1. **The #373 structural route — virtualised columns: LANDED;
   adopted as the fix of record.** Viewport-only instantiation bounds
   the live raster geometry to the viewport window (± the repeater's
   scroll-time window), so no rendered item's geometry can approach
   ±32 767 px regardless of column depth; the estimated content
   height (avg × rows) lives in f32 logical properties, never in
   renderer geometry. Proven at the former crash scale by F4; this
   chunk re-proves and CI-gates it.
2. **`clip: true` on the column task area: REJECTED — no clip
   anywhere in this chunk.** Recorded reasons: (a) it is unnecessary
   — the live geometry is already viewport-bounded; (b) it would not
   even have prevented the recorded panic: `clip` is the raster
   stage's scissor, not a geometry bound — the recorded backtrace
   aborted in `BasicBorderRectangle::render`'s i16 add for eager
   content sitting below the fold, i.e. in exactly the region a clip
   would have scissored *after* its geometry was computed; (c) its
   only observable effect on the healthy tree would be masking the
   engagement probe's failure symptom (cut content instead of a loud
   eager-shape failure). The golden story for over-deep columns is
   therefore **viewport-only, unclipped**: the head cards render, the
   rest exists only as the estimated scroll range (D-4).
3. **Upstream slint: the report artefact (M-C)** — the i16 facts and
   the Today contrast are upstream's mechanism question; the
   testing-backend limitation (#536) rides the same draft. Posting
   upstream is the user's act; the artefact is in-repo only.

**Consequence for stage 2 (deviation D-10)**: dispatch prompt 3.1.b
step 2's "the board columns onto the ListView mechanism (adapting
drop zones, walk, and indexing per the plan)" is **satisfied by the
landed #373 round** — drop zones untouched (the DropArea is the outer
column repeater's, F3), the reading-order walk is state-side and
untouched, IdTables indexing is global-model and pinned (F5/F6).
Stage 2 implements the *residual* scope below and must not re-touch
the mechanism; a diff inside `ui/widgets/listview.slint`,
`ui/compositions/board-layout.slint`'s `lv`/`task-viewport` blocks,
or `view_state.rs`'s walk is a deviation to revert, not to make.

### 3.1 M-A — the CI deep render pin (the TDD core)

New test in `tests/virtualisation.rs` (the file already owns the
board mounting machinery — `mount_board` over
`benches/support::build_state` — and the engagement assertions):

- **Name** (the desired semantics, per the house debt convention's
  positive form): `a_board_beyond_the_i16_pixel_ceiling_renders`.
- **Fixture**: `build_state(2_600)` — four columns × 650 rows, the
  recorded bracket's exact panic point (F1). Window 1366×768 — the
  bracket's recorded geometry (and the row-3 budget's own).
- **Renderer**: `DriverOptions { headless_renderer: true }` (F14);
  the renderer-presence assertion fires first (hard failure on a
  backend without the embedded rasterizer — the golden tier's
  convention; there is no skip path).
- **Assertions** (all structural, no text, no ceilings):
  1. One `take_snapshot` capture succeeds. This is the property
     itself: the former failure mode was process abort inside the
     raster pass; a captured frame is the typed outcome. (The mount
     + first refresh already derive and push the full board model at
     N = 2 600; the capture adds the raster pass over the
     virtualised scene.)
  2. The live `BoardLayout::drag-zone` instance count stays
     viewport-scale at N = 2 600 — the same ×4 failure-detector
     ratio the engagement tests use (`instances.len() * 4 < 2_600`),
     never a ceiling: strictly better virtualisation still passes.
  3. The first card of column 0 renders
     (`find_by_label(&bench_label(0))`) — the degenerate
     "engaged but renders nothing" guard, matching the engagement
     probe's labelled-element guard.
- **Why N = 2 600 and not 10 000**: the bracket's panic point is the
  tightest recorded reproduction, and it keeps the pin cheap (one
  mount, one derive/push pass, one capture). The 10 000 tier (2 500
  rows/column, 3.8× deeper than the panic point) stays the bench
  lanes' subject (M-B) — scale headroom is bench evidence, the crash
  point is CI contract (D-5).
- **Why the regular leg only**: the recorded crash is the regular
  leg's drop-zone repeater (F1's backtrace); the compact leg's
  viewport-scale engagement is already pinned at N = 1 000 (F5), and
  one behavior (deep-board raster survival) gets one test at the
  lowest layer that exercises it (AGENTS §5).
- **TDD order and the load-bearing proof**: the pin lands **first**
  and passes on the landed tree (the fix is in — this is a regression
  pin, not a red-green for new code). Its constraining power is then
  proven by the invert-check, recorded in the stage-2 report:
  temporarily break the nominal ListView contract in the scratch tree
  (rename the component `ListView` → `ListViev` in
  `board-layout.slint`'s import/use — the compiler's contract is
  nominal, so the `for` silently renders eager; the mechanism is
  documented in `ui/widgets/listview.slint:2-6`), run the pin, and
  observe it **fail** — the engagement ratio assertion fires (and the
  eager raster at 650 rows/column would abort at the i16 add, the
  recorded mechanism, had the ratio not caught it first). Revert the
  scratch rename; the transcript (command + outcome) goes in the
  stage-2 report. This is the AGENTS §5 / review-mandate discipline:
  a guard whose failure you have not observed is decoration.

### 3.2 M-B — the bench comment fix + the full-matrix re-run

- **The comment fix (F10)**: rewrite `board_flip_targets`' doc in
  `benches/support/mod.rs` to state the scene honestly: the board's
  rendered head rows sit inside the viewport window (virtualised),
  and a done flip on one is an *in-place* card change — the damage
  shape the board pin separates from the Today pin's removal. No
  code change; the fixture and targets are unchanged.
- **The matrix re-run**: all ten cells — `refresh/{100,1000,10000}`,
  `draw-full/{today,board}/{100,1000,10000}`,
  `draw-partial/{today,board}/{100,1000,10000}` — on the chunk's
  final tree, run through
  `scripts/run-quality-gates.sh cargo bench -p taskboard-ui-slint`
  (the heavy-run wrapper; the lock discipline applies on this box).
  Record verbatim: criterion medians, the visible-flip guard lines,
  and the region/max/last-iteration lines (the re-measure report §2
  format) in the stage-2 report.
- **The acceptance mapping**: the `board` cells at N = 10 000 are the
  bench-level formerly-panicking-scale proof (2 500 rows/column ≫
  650); every cell measuring (no SKIPPED line anywhere) is the
  loud-skip acceptance item's verification; times stay dev-machine
  references — **no budget assertion, no ceiling** (the 100 ms
  cadence floors composed latency regardless; the on-device verdict
  remains the kiosk wave's).
- Expected shape (from F4, not a gate): board draw lanes
  N-independent, low-single-digit ms; Today lanes unchanged by this
  chunk (it touches nothing they render); refresh unchanged.

### 3.3 M-C — the upstream report (the rider's deliverable)

Path (mandated by dispatch prompt 3.1.b step 2):
`.artifacts/reports/2026-10-31-slint-upstream-i16-and-testing-backend-report.md`.

Skeleton (stage 2 fills; every claim cites the pinned 1.18.1 sources
re-verified locally, the recorded reports, or fresh verbatim output):

1. **Header**: date; kind = *upstream draft — in-repo artefact,
   posting upstream is the user's decision*; the pinned slint crate
   versions from `Cargo.lock` (re-read, not assumed); the evidence
   mandate statement.
2. **Finding 1 — the software renderer's i16 geometry space**: the
   source cites (`i-slint-renderer-software-1.18.1/lib.rs:63-67` —
   `PhysicalLength = euclid::Length<i16, PhysicalPx>`, the
   `i16`-based point/size types); the overflow-checks amplification
   (a consumer-profile choice turning out-of-space geometry into a
   process abort — cite `Cargo.toml`'s profile + the #303 decision);
   the verbatim recorded backtrace (re-measure §5); the empirical
   bracket with its method (F1). The ask for upstream: where the
   geometry space should widen or saturate (i32/f32 conversion
   stage), and whether the class is shared by the other backends.
3. **Finding 2 — the Today contrast (the mechanism question)**: the
   same card widget family rendered ~183k px of eager list depth on
   the Today view at N = 10 000 without panicking
   (`draw-full/today/10000` = 19.9 ms) while the eager board column
   scene aborted at ~35k px (650 cards × ~55 px); the board's
   overflowing path rasterized through the unclipped column stack
   (the drop-zone repeater's visit); the Today list sat inside the
   page-Flickable/section clip chain. The open question for upstream:
   is the difference the clip scissor's culling stage (items outside
   the scissor dropped before geometry math) or per-item-type
   arithmetic? Recorded against 1.18.1; both surfaces are virtualised
   in our tree since #373, so the contrast is reproducible upstream
   in principle with a plain eager `for`.
4. **Finding 3 — the testing backend's empty-element-query
   limitation (#536)**: the reproduction (a second conditional
   control in the chrome row → the `SettingsView` element query
   returns empty; the journeys workaround comment quoted verbatim,
   `journeys.rs` ~:4034-4041); the traversal-multiplicity observation
   from the #373 round (one node yielded once per multi-element
   ancestor path — the 11×/14× counts the retracted stop report
   measured; the dedupe-by-geometry workaround in
   `tests/support/driver.rs`). The ask: the `ElementQuery` traversal
   contract for conditional/multi-ancestor trees.
5. **Annex — further 1.18.1 behavior observations from the ListView
   round** (short; each citing the stage-2 report §2 as its
   evidence): the Flickable layout-info reality (F13a), the
   model-through-property non-instantiation (F13b), the conservative
   height↔content-height loop edge (F13c). **Explicitly excluded**:
   the retracted empty-delegate-id claim (stage-2 §9) — D-7.

Bookkeeping split (binding): the report is the artefact; stage 3
posts nothing upstream. Stage 3's #536 close comment states the draft
is filed in-repo and posting is the user's act; stage 3's #462 close
comment carries the pin + bench evidence + the report path.

### 3.4 M-D — CHANGELOG

One `### Fixed` entry in the Unreleased section (F15): the kiosk
board no longer aborts at large task counts (#462) — one or two
sentences naming the software-renderer/linuxkms surface and pointing
at the "Virtualised task lists (#373)" Added entry for the mechanism
(the eager board's columns exceeded the renderer's i16 pixel space at
~2.5k+ tasks and aborted the process on first board render; the
landed virtualisation bounds the live geometry to the viewport, so
deep boards render). User-observable, kiosk-facing — distinct in role
from the feature entry, as the dispatch prompt mandates.

## 4. Decision record

| # | Decision | Alternatives rejected (why) |
|---|---|---|
| D-1 | The lever is the #373 structural route, **already landed** — adopt as the fix of record; the chunk proves, pins, and documents | Re-implementing the mechanism on this branch (duplicate of landed work; a diff there is a revert-class deviation, D-10); the clip lever (D-2); waiting for the kiosk wave's D250 verdict (the design round's implementation gate applied to the *performance* motivation — the mechanism shipped anyway via #373's accepted dispatch; this chunk's scope is the crash's proof, which needs no on-device verdict: the recorded crash reproduced on the dev box's software renderer, the same renderer class the kiosk runs) |
| D-2 | No `clip: true` anywhere; over-deep columns are viewport-only by construction | Clip on the column body (unnecessary — viewport-bounded live geometry; ineffective against the recorded panic — the scissor runs after geometry math, the abort was in the below-the-fold add; masks the engagement probe's failure symptom — a visual-design change with no surviving benefit). If a future regression makes eager rendering reachable again, the engagement probe must fail loudly, not a scissor hide it |
| D-3 | The CI pin: one test, regular leg, N = 2 600, 1366×768, headless software renderer, capture + ×4 ratio + head-card presence | N = 10 000 in CI (bench-scale evidence is M-B's job; 4× the pin's cost for headroom, not contract); a compact-leg twin (the recorded crash is the regular leg; compact engagement already pinned); pixel-diff assertions (the golden tier owns visual facts; depth is invisible in viewport-only pixels, D-4); a new test file (the board-mount machinery lives in `tests/virtualisation.rs`) |
| D-4 | No new golden; over-deep columns' look is viewport-only — pixel-identical to a shallow column at the same viewport except the count badge's digit (fixture-dependent) | A deep-fixture golden (pins a fixture-dependent badge digit for no visual fact; the existing six goldens already pin the post-virtualisation geometry incl. the D-4 ghost/create-field deviation); regenerating any existing golden (nothing this chunk changes renders — a golden diff would be a bug to diagnose, never a baseline to advance) |
| D-5 | Bench: no new N cell; the standard matrix re-run on the final tree; N = 2 600 lives in the CI pin | A permanent N = 2 600 criterion cell (an ad-hoc N pollutes the recorded lane matrix and the ≥10 000 budget config keys off the tier boundary); a one-off scratch bench at 2 600 (scratch never lands; the pin is the permanent record of that exact scale) |
| D-6 | The 400-rows/column skip: verified **deleted** (M4, `1cd3415` + stage-3 comment rewrite); this chunk verifies zero SKIPPED output + fixes the stale doc the deletion left (F10) | Re-adding a bound "for safety" (it would skip the virtualised board's honest measurements — the exact M4 verdict); renumbering lanes (breaks before/after comparability) |
| D-7 | The upstream report covers the mandate (i16 facts + Today contrast + #536) plus a short annex of the #373 round's non-retracted 1.18.1 findings | Folding the retracted empty-delegate-id claim in (retracted with cause, stage-2 §9 — an upstream report carrying it would waste upstream's time and ours); splitting into multiple reports (the dispatch plan mandates one artefact; the annex keeps it one) |
| D-8 | One short `### Fixed` CHANGELOG entry despite the Added entry already naming #462 | Amending the Added entry (the prompt mandates a bug-fix record; the panic's user-facing story — "the app died" — is distinct from the feature's); a longer entry (the mechanism detail lives in the Added entry and the report) |
| D-9 | The branch bases off `origin/main` (`67c0b531`), not the 40-commits-stale local `main` | Local `main` as base (the render lane is serial precisely so this chunk builds on 2.1's landed row-diff mirror `a3987d11`; a stale base would cite pre-mirror line numbers and re-fight stage 3's rebase); fast-forwarding the primary checkout's `main` (unnecessary mutation of the shared checkout; branching off `origin/main` is the AGENTS §8 re-branch discipline) |
| D-10 | Stage 2 does **not** re-implement the mechanism — dispatch prompt 3.1.b step 2's mechanism wording is satisfied-by-landing; the residual scope is M-A..M-D | Reading the prompt literally and re-wrapping the board columns (duplicating `85f0cd85`'s diff would at best be a no-op and at worst regress the pinned semantics) — the premise drift is the dispatch plan's, written before this planning round verified the landing; the recorded-reason rule covers the deviation |

## 5. Testing strategy (per AGENTS §5)

- **Layer discipline**: deep-board raster survival is a presentation
  fact — it lives in `tests/virtualisation.rs` beside the other
  board engagement pins and never duplicates through the engine (the
  engine knows nothing of renderers) or the bench (which stays
  evidence, not contract). One behavior, one test, lowest layer.
- **The pin's assertions are structural**: capture success (the
  former abort is the failure mode), the ×4 failure-detector ratio
  (a ceiling would violate the no-quality-ceilings rule; a smaller
  live window is strictly better and passes), label presence (the
  degenerate-render guard). No text assertions; no timing
  assertions anywhere.
- **Determinism**: the fixture is the bench support's pure function
  of N (`build_state`); the driver's mock clock (`advance_time`)
  drives instantiation; no sleeps, no wall reads; the capture is
  CPU-deterministic for the pinned slint (the golden tier's
  determinism argument applies verbatim).
- **The load-bearing proof**: the invert-check transcript (scratch
  rename → pin fails → revert) recorded in the stage-2 report — the
  same discipline the review mandate applies to the refresh guard
  ("temporarily invert and confirm"), without which the pin is
  decoration.
- **A new test failing can be a bug**: if the pin fails on the landed
  tree, the mechanism regressed somewhere between `67c0b531` and the
  pin's commit — treat production code as the likely suspect (the
  engagement probes and goldens would usually have caught it first;
  a pin-only failure means a *composition* regression, which is
  exactly the gap F11 names).

## 6. Risks

- **R1 — pin runtime cost**: mount + derive/push + one capture at
  N = 2 600 with the embedded rasterizer. Expected low-single-digit
  seconds (the derive pass is sub-ms at this N; the capture is one
  1366×768 raster). Stage 2 measures and records the cost; if it
  lands >5 s, investigate before accepting (the suspicion would be
  an accidental eager instantiation the ratio somehow missed — which
  is a finding, not a tuning problem).
- **R2 — renderer presence in CI**: the headless rasterizer is
  embedded by the test harness (a dev-feature of the driver, F14) —
  no CI leg lacks it; the presence assertion is a hard failure by
  design. No skip path exists or may be added.
- **R3 — the pin proves the recorded scale, not all scales**: deeper
  columns (the estimated content height grows unboundedly with N in
  f32 properties) are unrecorded territory; the chunk's contract is
  the recorded crash scale (F1's bracket), and the next ceiling, if
  any, is upstream's question (M-C's finding 1 ask). Scope note, not
  a gap.
- **R4 — bench re-run duration**: the 10 000 tier runs 10 samples ×
  15 s per cell across five 10 000-cells plus the lower tiers —
  order 15-25 min wall. Run through
  `scripts/run-quality-gates.sh` (the exclusive-flock wrapper) so a
  parallel worktree's coverage suite cannot OOM the box; never paste
  the raw sequence beside concurrent agents.
- **R5 — premise drift at stage 2**: the dispatch prompt's step 2
  reads as mechanism-implementation. The binding text is this plan
  (D-10); stage 3's diff review must reject any
  `listview.slint`/board-`lv`/`view_state.rs` production diff.
- **R6 — upstream-report citation drift**: the slint source cites
  must be re-verified against the locally-pinned 1.18.1 registry
  sources at write time, not copied from the reports (the reports
  cite the same pinned version, but the evidence mandate makes the
  citation stage 2's own claim).
- **R7 — `origin/main` moves during the chunk**: stage 3's rebase
  dance per the shared model handles it; the UI-territory rule
  (§1) keeps co-edits off the board seams while the branch is in
  flight.

## 7. References

- Dispatch plan: `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §1 (batches/lanes), §2 chunk 3.1 (the three prompts; the code
  -quality and evidence mandates).
- Issues: #462 (the crash — mechanism, bracket, levers), #536 (the
  testing-backend limitation), #373 (the landed mechanism), #460
  (the bench instrument), #461 (Today virtualisation — the *next*
  render-lane chunk, blocked on this one), #390 (the damage-region
  half — referenced, not absorbed), #303 (overflow-checks).
- Reports: the design round (2026-10-26), the view-pinned re-measure
  (2026-10-26), the #373 stage-2 report with §9 corrections
  (2026-10-26).
- Landed commits: `2411f305` (ListView + engagement probe),
  `85f0cd85` (board columns through the ListView), `1cd3415` (bench
  un-skip), `a3987d11` (the row-diff mirror, chunk 2.1), `7aeb5cd3`
  (the paging seam), `bf7cf88f` (the divider chunk — the neighboring
  board diff this branch must not tangle with).
- Seams: `ui/widgets/listview.slint`, `ui/compositions/board-layout.slint`
  (:273 drop-zone repeater, :489 `lv`, :819 `task-viewport`),
  `src/models.rs` (:52-57 the index invariant, ~:310 `diff_rows`,
  ~:983 `push_board`), `ui/focus-scroll.slint` + `ui/scroll-page.slint`
  (the hint globals), `tests/virtualisation.rs`,
  `tests/support/driver.rs`, `benches/refresh_draw.rs`,
  `benches/support/mod.rs`, `tests/golden_visuals.rs`.

## 8. Suggested commit sequence (stage 2's; each standalone + gates-green)

1. `test(ui-slint): the board renders beyond the i16 pixel ceiling` —
   the M-A pin (test only; the invert-proof transcript goes in the
   stage-2 report, not the commit).
2. `docs(ui-slint): the board flip targets state the virtualised scene` —
   the M-B comment fix in `benches/support/mod.rs`.
3. `docs(changelog): the kiosk board i16 panic gets its Fixed entry` —
   M-D.
4. `docs(report): slint upstream i16 and testing-backend draft` —
   M-C's artefact (the chunk's last commit; the diff to review is
   then exactly code + the one report + CHANGELOG + this plan).

The bench matrix re-run (M-B) is evidence gathering — its verbatim
output lands in the stage-2 report, not in a commit.

## 9. Estimated diff size

≈ 120-160 lines of branch diff: ~70-90 test lines (the pin + its
doc comment), ~4 bench doc lines, ~10 CHANGELOG lines, plus the
report artefact (~250-350 lines, not code). No production slint or
Rust diffs. Size S/M, single PR, no stacking.
