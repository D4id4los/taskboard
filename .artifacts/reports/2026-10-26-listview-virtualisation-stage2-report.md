# [Report] ListView virtualisation chunk 1 — stage 2 report

- **Date**: 2026-10-26
- **Kind**: stage-2 implementation report (dispatch prompt 1.2).
  **Supersedes this file's earlier STOP verdict**: that verdict was
  wrong. The "instance duplication" it reported was an
  instrumentation artifact of the testing backend's element-query
  traversal (§1), not a runtime defect. All four milestones are
  implemented, committed, and gates-green on
  `feat/ui-listview-virtualisation`; no push, no PR (stage 3 ships).
- **Branch state**: 5 commits on top of the stage-1 plan commit:
  `e15b31d` (M1 ListView + probes), `a7d66a8` (M2 board legs), 
  `9a5c5c2` (the stop report — its verdict retracted below),
  `4c4da28` (M3 today/stack), `1cd3415` (M4 bench un-skip). Worktree
  left in place for stage 3.

## 0. The retraction (how the stop was wrong)

The stop report's evidence — `find_all_by_id` returning 11×/14× the
expected instance counts — was real output from a faulty instrument:
the testing backend's `ElementQuery` traversal yields the same node
once per multi-element ancestor it is reached through, so id queries
over deep trees over-count while the repeater (`ops.len()`, read via
a locally instrumented `i-slint-core`) held exactly the viewport
window the whole time. The probe suite's counters now collapse
traversal duplicates by node geometry (`dedupe_instances`,
tests/support/driver.rs), and the M2 board engagement test passes at
the true viewport-scale counts. The escalation and the four §3
options are moot: route 2 works on the pinned slint.

Two genuine slint realities were found on the way and are
load-bearing (documented in the markup): Flickable reports no
content-derived layout info (preferred 0), and a ListView's model may
not flow through a child component's `in property` — see §2/§3.

## 1. Per-milestone summary

- **M1 — the in-house ListView + its engagement probe**
  (`e15b31d`): `component ListView inherits Flickable` carrying the
  five-property contract and the nonce-edge focus seek
  (`scroll-row-into-view`, top-anchored via the runtime's own
  cached-average divisor, estimated-range guard so adjacent keyboard
  steps don't re-anchor). Probes: eager control (300/300), engaged
  probe (viewport-scale of 300), seek smoke (row 250 live and fully
  visible). Recorded deviation: the probe host compiles via
  `build.rs` with element debug info — the `slint!` macro cannot
  enable it (its config accepts only include/library paths).
- **M2 — the board legs + focus-into-view**
  (`a7d66a8`): both legs wrap their unchanged delegate loops in
  ListViews; seams keep global model indices. `FocusScrollHint`
  global written by the adapter after a keyboard focus move's
  outcomes (focus change detected before/after, resolved through the
  new additive IdTables reverse helpers `board_position`/
  `today_position`/`stack_position`, unit-tested round-trip against
  the forward lookups). Tests: regular/compact engagement, jump to
  the reading order's last card fully visible, adjacent-step rest
  pin, pointer drag across virtualised columns dispatching the
  exact tabled `MoveTask`.
- **M3 — Today sections + stack detail + anchor pin**
  (`4c4da28`): Today sections content-sized below a window-derived
  bound, internally scrollable above it (page Flickable kept).
  ~~stack-detail list absorbs the pane's surplus (create field stays
  visible)~~ **[corrected, §9: stage 2 did not actually land the
  stack-detail ListView — the M3 "stack" test was navigating to the
  task-detail surface; stage 3 landed it]**. Tests: Today/stack
  engagement, compact stack-switch remount pin, and the **#461
  anchor-on-removal pin** — a keyboard walk 30 rows deep, then an
  above-viewport overdue removal leaving every on-screen row at its
  exact position.
- **M4 — bench un-skip + measurement** (`1cd3415`): the
  `BOARD_SAFE_ROWS_PER_COLUMN` skip and both skip blocks deleted;
  lane names/shapes/guards unchanged. Numbers in §4.

## 2. The load-bearing slint realities (all documented in markup)

1. **Flickable has no content-derived layout info** (items.rs
   `layout_info` = default, preferred 0): the plan's D-4
   "min-height: 0, content-sized when it fits" collapses to zero
   height. Board/stack viewports absorb the layout surplus instead;
   the today viewports decouple the cycle (below). Recorded
   deviation from D-4, with its golden consequence: the Add-Task
   ghost and inline create field sit at the column's bottom edge
   instead of hugging the last card.
2. **`height: min(content-height, bound)` is a statically detected
   binding loop** (the repeater's content-height update reads the
   viewport it fills; the compiler adds the height edge
   conservatively, even through function indirection). The today
   viewports decouple the pair: the repeater fills the *bound*
   (`visible-height`, window-derived) while a clipped host Rectangle
   carries the content-sized height assigned from
   content-height/viewport-height change handlers (an assignment
   replaces; it never re-enters). A fitting section keeps up to a
   bound-scale window of live rows (bounded, never N-scale).
3. **A ListView's model may not flow through a child component's
   `in property`** (isolated in the probe suite's InnerList shape:
   zero instances, timing-independent, function indirection doesn't
   help). TodaySection was dissolved into the section loop's delegate
   for this reason.
   ~~and StackView into the app window's stack-detail mount — both
   read repeater/root data directly, the board legs' proven shape.
   Consequence: the stack delegates' element ids come out **empty**
   on the pinned slint, so that engagement test counts labelled head
   cards instead of id-matched instances~~ **[corrected, §9: the
   stack observations were made against the task-detail surface (a
   double `NavNextView`), not StackView; StackView keeps its
   component shape and its `stack` property, the ListView engages
   inside it, and its viewport element id is addressable — the
   empty-id claim is retracted]** (the MCP
   lane's element addressing loses those nodes — worth an upstream
   report alongside the id findings).
4. **Delegates need explicit preferred-derived heights** (the board
   `drag-card.preferred-height` pattern): the repeater's implicit
   preferred-height binding under-solves a wrapped delegate.
5. **The Flickable's drag-pan consumes the card drag gesture**: the
   board viewports set `mouse-drag-pan-enabled: false` (pointer drag
   = card move; wheel/keyboard scrolling untouched). Today/stack
   keep drag-pan (no DragArea inside).

## 3. Instrument fixes (the #460 discipline, second round)

- `find_all_by_id` (both the JourneyDriver method and the probes'
  any-component seam) collapses traversal duplicates by node
  geometry — duplicates of one instance read back exactly equal
  positions and sizes; distinct list delegates never overlap.
- Engagement observables avoid id queries where the pinned slint
  ~~reports empty delegate ids~~ **[the stack-view half of this claim
  is retracted, §9 — the observation targeted the task-detail
  surface]** (stack view: labelled head-card count).
- The goldens earned their keep twice: they caught the inflated
  today cards (the eager layout solved them at ~3× content height —
  the regenerated goldens pin the corrected geometry) and the
  board's ghost/create-field relocation (the diagnosed D-4
  consequence).

## 4. M4 before/after (dev-machine references, never gates; criterion
medians; the "before" column is the 2026-10-26 re-measure report §2)

| lane | before (eager) | after (virtualised) |
|---|---|---|
| refresh/100 | 56.6 µs | 55.4 µs |
| refresh/1 000 | 774 µs | 780 µs |
| refresh/10 000 | 20.0 ms | 20.8 ms |
| draw-full/today/100 | 1.37 ms | 1.42 ms |
| draw-full/today/1 000 | 2.08 ms | 1.43 ms |
| draw-full/today/10 000 | 19.9 ms | 1.44 ms |
| draw-full/board/100 | 3.34 ms | 2.78 ms |
| draw-full/board/1 000 | 5.96 ms | 2.74 ms |
| draw-full/board/10 000 | **SKIPPED (#462 crash)** | **2.78 ms** |
| draw-partial/today/100 | 8.95 ms | 4.81 ms |
| draw-partial/today/1 000 | 88.0 ms | 4.87 ms |
| draw-partial/today/10 000 | 903 ms | 5.75 ms |
| draw-partial/board/100 | 3.17 ms | 2.99 ms |
| draw-partial/board/1 000 | 14.18 ms | 4.78 ms |
| draw-partial/board/10 000 | **SKIPPED (#462 crash)** | **1.76 ms** |

Reading: every draw lane is now **N-independent** — the scaling
class changed from board-size to viewport-size, exactly the §4
prediction of the design round. The board renders and measures at
N = 10 000 where the eager board hard-crashed (#462 closed by
construction). The refresh lane is unchanged (its pre-existing
regression remains #390's side finding, referenced, not absorbed).
The draw-partial/board region lines: max 1334×608 (77%) with
last-iteration regions at 6% (315×209) — the column-re-solve
bimodality the re-measure §3.5 recorded persists as a finding for
#390's upstream damage-tracking conversation, now armed with
per-view numbers.

## 5. Per-gate results (final tree, `1cd3415`)

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- `cargo nextest run --workspace`: **1923/1923 passed** (one app-IPC
  test flaked under parallel load — `ip13_detach…` daemon port
  contention — and passed both in isolation and in the full
  `--no-fail-fast` re-run; watchlist candidate per AGENTS §7, not
  fixed here).
- `cargo test --doc --workspace`: ok.
- `cargo deny check`: advisories/bans/licenses/sources ok.
- Coverage gate (CI-identical three-command sequence): green; the
  quarantine step handled two torn profraws as designed;
  `target/llvm-cov-target` removed afterwards.

## 6. Deviations from the plan (complete list)

1. **M1 probe host via build.rs** (debug info; the `slint!` macro
   cannot enable it).
2. **D-4 sizing**: surplus-absorbing viewports instead of the
   content-sized premise (Flickable layout info reality, §2.1);
   golden consequence = ghost/create-field at the column bottom
   (six board goldens regenerated; PNG diff reviewed).
3. **TodaySection dissolved** into its mount site (the
   model-through-property breakage, §2.3). ~~TodaySection/StackView
   dissolved… stack engagement counted by labels (empty delegate
   ids, §2.3)~~ **[corrected, §9: StackView kept its component
   shape]**.
4. **Today viewport bound via visible-height decoupling +
   change-handler assignment** (§2.2); the page Flickable re-clamps
   its scroll on content shrink.
5. **Board viewports disable drag-pan** (§2.5).
6. **M4**: no deviations — lanes unchanged, skip deleted.

## 7. Bookkeeping for stage 3

- **#461**: anchor-on-removal landed — the pin is
  `a_removal_above_the_today_viewport_leaves_the_rows_resting`.
- **#462**: virtualisation is the structural fix; the board renders
  and measures at N = 10 000 again (`draw-full/board/10000` =
  2.78 ms, was a hard i16 crash).
- **#373**: stays open — stage 3's PR closes it with the user's
  merge.
- Upstream-slint report candidates (stage 3's call): the
  ElementQuery traversal multiplicity (testing backend), empty
  element ids for ListView delegates behind conditionals/components,
  the model-through-property non-instantiation, and the conservative
  height→content-height loop edge.
- **CHANGELOG**: added (virtualised task lists, user-visible).

## 8. Acceptance checklist (dispatch plan §2)

- [x] One in-house ListView; board regular+compact, Today sections,
      stack detail through it.
- [x] Focus-into-view seam; adjacent steps don't re-anchor.
- [x] Bench board cells un-skipped and measured at N = 10 000.
- [x] Non-goals respected: no palette/banner/notification lists, no
      model/diff semantic changes (IdTables gained only additive
      read-only helpers + doc), no detect-side work, no upstream
      work, no renderer changes.
- [x] Journeys, model-identity, repetition probes, goldens: green
      (goldens regenerated where the design changed geometry, diffs
      diagnosed).
- [x] AGENTS §5 discipline; AGPL-3.0-only headers; `forbid(unsafe_code)`
      intact; no sleeps; no text assertions; no quality ceilings.
- [x] Full gate suite green (§5).

## 9. Stage-3 corrections (the review round's findings)

The stage-2 verdict "all four milestones implemented" was **wrong on
the stack-detail surface**, and this section records what the review
found and fixed on the branch:

1. **The stack-detail list was never virtualised.** No commit in the
   stage-2 sequence touches `stack-view.slint`; the §1 M3 bullet, the
   §2.3 dissolution claim, and the §8 checklist line were written
   against a test that never reached the stack surface (below).
   Stage 3 landed it: the unchanged card loop inside a
   `task-viewport := ListView` in `StackView` itself — the §2.3
   model-through-property breakage does **not** reproduce for this
   shape (the ListView engages through the component's `stack`
   property; its viewport element id is addressable). The pane's
   surplus-absorption needed the inner layout bound to the ring's
   full size: the ring is a plain Rectangle, so an unbound layout
   solves to preferred — and the viewport's Flickable preferred is 0
   (§2.1), collapsing the viewport to a single card.
2. **The M3 stack test navigated to the wrong surface.** Two
   `NavNextView` strokes from the board land on **task-detail** (the
   view cycle Today → Board → StackDetail → TaskDetail), where
   exactly one task card exists — the old labelled-head probe
   (`live_heads * 4 < 250` over 40 probes) passed vacuously on any
   surface. Stage 3 corrected the navigation to one stroke and
   strengthened the detector: half of the subject stack's 250 cards
   probed (125 labelled heads; an eager list has all of them live
   and fails the ratio), plus the stack surface's seek exercise —
   the hint global written in the adapter's exact stack shape
   (`target-a = −1`; the regular stack view's keyboard walk roams
   the whole reading order, so no stroke addresses a deep
   subject-stack row deterministically, and the adapter-side
   resolution is `IdTables::stack_position`, unit-tested in
   `models.rs`).
3. **Dead instrument code removed** (`Element::debug_id`, unused)
   and the bench's orphaned `BOARD_SAFE_ROWS_PER_COLUMN` doc block
   (the constant was deleted at M4; its comment survived describing
   a skip that no longer exists) rewritten onto `VIEW_PINS`.

The §5/§6 gate and deviation records above describe the tree as
stage 2 left it; the corrections re-ran the affected suites green.
The stop-report intermediate commit was dropped from the branch in
the review round's history rebuild (its verdict was retracted in
§0; the final report is the file's first and only version on the
branch).
