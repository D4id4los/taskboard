# [Plan] ListView virtualisation across the N-task surfaces (#373) — chunk 1 implementation plan (stage 1)

- **Date**: 2026-10-26
- **Kind**: stage-1 TDD implementation plan for the single chunk of
  the #373 dispatch — the in-house Slint `ListView` component applied
  to every surface that draws N tasks, the focus-into-view seam, and
  the bench un-skip. Zero production code in this commit; this file
  is the chunk's entire planning artefact. Implementation is stage 2's
  (dispatch prompt 1.2), review + shipping stage 3's (prompt 1.3).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-26-listview-virtualisation-dispatch-plan.md`
  §2 (the Covers/Acceptance list is binding), which dispatches the
  accepted design round
  `.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`
  (§2 route 2 + compiler contract + runtime mechanics; §3 the four
  adjudicated trade-offs; §5 the D9 trichotomy; §6 the recommendation
  order) and the view-pinned re-measure
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`
  (§2-§3 the before-numbers and the guard/skip instrument; §5 the board
  i16 render ceiling). Issues: #373 (the ask), #461 (Today lists +
  anchor-on-removal), #462 (the crash), #460 (the instrument — landed
  by the carrying PR).
- **Normative base**: `origin/main` = `d58e0b2` (the worktree's
  branch point; carries all three origin documents).
- **Milestones** (fixed order, from the dispatch prompt): M1 the
  in-house ListView + its engagement probe; M2 the board legs
  (regular + compact) + focus-into-view + journey/golden updates; M3
  Today sections + stack detail + anchor-on-removal pins; M4 the bench
  board-cell un-skip + measurement. Milestones are plan structure, not
  commit boundaries — stage 2 lands TDD pairs as standalone green
  commits (§8).

## 1. Mission & binding constraints

One in-house `ListView` component (route 2 of the design round) that
engages the slint compiler's virtual repeater layout, applied to:
the board columns' regular leg
(`board-layout.slint:386-409`), the compact leg's single column
(`board-layout.slint:603-610`), the Today view's four section lists
(`today-view.slint:54`), and the stack-detail task list
(`stack-view.slint:57`). Plus: the focus-into-view seam (a keyboard
focus move to an off-viewport card scrolls it fully into view), and
the bench's board cells at N = 10 000 un-skipped and measured (the
#373 Ask-2 success instrument; eager N = 10 000 is the #462 i16
panic, virtualised N = 10 000 is a measurement).

Binding, from the dispatch plan §2 and prompts (verbatim force):

1. The component element must be **named exactly `ListView`** and
   carry all five length properties — `content-y`, `content-height`,
   `content-width`, `visible-height`, `visible-width` — or the
   compiler silently renders the `for` eager
   (`i-slint-compiler-1.18.1/object_tree.rs:2891-2905`, verified §2
   fact F1). M1's engagement probe guards this forever.
2. The delegate is the existing card pair **moved inside unchanged**;
   callbacks and IdTables keep **global model indices** — nothing
   viewport-relative may leak into any seam (design round §3b).
3. **Never swap a `ModelRc` per refresh** — D9's persistent handles
   are load-bearing (design round §5; a `reset()` would re-instantiate
   the viewport every pass and forfeit the win).
4. No existing test weakened: goldens updated (regen), never deleted;
   journeys extended, never relaxed. The goldens lane, the keyboard
   journeys (F8 double-click = `a_double_click_across_a_refresh_...`,
   F11-family drag = `the_drag_survives_interleaved_...` and the
   resize-drag journeys), the model-identity journeys
   (`consecutive_refreshes_keep_the_board_model_identity`), the
   repetition-semantics probes (the TouchArea-survival mechanism those
   two journeys pin), and `tests/bench_fixture_contract.rs` must all
   stay green unmodified except where §5 names an expected, reviewed
   delta (goldens regen).
5. Diff shape: no `models.rs` diff/push **semantic** change, no
   `IdTables` **shape** change (additive read-only helpers are fine —
   §3.9), no crate-boundary change, no detect-side work (#463/#464),
   no renderer/upstream work, no palette/banner/notification surfaces
   (row-capped, not N-task).
6. AGENTS §5 across all tests: no sleeps, no text assertions, no
   quality ceilings on success metrics, deterministic by construction
   (poll a predicate with a deadline / mock clock), one behavior one
   test at the lowest layer. AGPL-3.0-only SPDX header on every new
   file; `#![forbid(unsafe_code)]` on any new .rs (none anticipated —
   no new production .rs files).

## 2. Verified baseline — numbered facts

All verified in the worktree at `d58e0b2` (file:line anchors are this
commit's). Compiler/runtime facts verified in the pinned crate sources
(`~/.cargo/registry/src/index.crates.io-.../i-slint-{compiler,core}-1.18.1`).

**The compiler contract (F1).** `object_tree.rs:2891-2905`
(`from_repeated_node`): a `for`'s parent gets the virtual ListView
layout iff the parent's base type stringifies to `"ListView"` **and**
`content-y`, `content-height`, `content-width`, `visible-height`,
`visible-width` all resolve as `LogicalLength` properties. Two
consequences read directly off the same function: (a) if the user
**explicitly binds** `content-width`/`content-height` (or their
deprecated `viewport-*` aliases), the compiler drops them from the
runtime's `ListViewInfo` — the runtime then stops driving them, so the
component must never bind them; (b) `passes/repeater_component.rs:84-99`:
for a ListView `for`, an unset delegate `height` binds to the
delegate's own `preferred-height` and an unset `width` binds to the
listview's `visible-width` — the existing explicit delegate bindings
(`width: 100%`, `height: drag-card.preferred-height`,
`board-layout.slint:387-388`) survive unchanged inside the delegate.

**The runtime mechanics (F2).** `i-slint-core-1.18.1/model/repeater.rs:263-446`
(`update_visible_instances`): only viewport instances live (± the
scroll window); off-screen instances splice out; a jump beyond 1.5
screens random-seeks with `offset = floor(-content-y / element_height)`
where `element_height` is the runtime's cached per-item average — and
that cached average is exactly what the runtime writes into
`content-height` (`cached_item_height × row_count`, repeater.rs:427).
Therefore an external seek computed as `content-y = -(row ×
content-height/row_count)` divides by the runtime's own divisor: the
seek's floor lands the window **exactly at `row`** — the design
round's "approximate average-height estimate" (§3a) is in fact exact
at making the target row live. `row_changed` on an off-viewport row is
a complete no-op; `reset()` re-instantiates the whole viewport;
insertions/removals shift the offset keeping a row-anchored position
(repeater.rs:531-616). `content-y` is ≤ 0 (negative downward), and the
runtime re-anchors it itself after offset shifts.

**The std widget shape (F3).** The built-in `Flickable` natively
declares `content-width/height` (in, shadowable) and `content-x/y`
(in-out, shadowable) with `viewport-*` as deprecated aliases
(`builtin_elements.rs:1494-1520`); std `ScrollView` merely aliases
`content-* <=> flickable.content-*`, `visible-* <=> flickable.width/height`,
and places `@children` inside the flickable (`widgets/cosmic/
scrollview.slint:100-131`). Std `ListView` is a ScrollView subclass
(`widgets/common/listview.slint:6-10`); its `bring-into-view` is
component-internal and assumes uniform heights
(`listview.slint:57-71`, `item-y(index) = index × content-height/
model.length`). Route 2's component can therefore be a thin
`component ListView inherits Flickable` adding only the two
`visible-*` properties — verified against the exact property set F1
checks.

**The board regular leg (F4).** The task loop
(`board-layout.slint:386-409`) sits inside the task-area
`VerticalLayout` (`alignment: start`, `vertical-stretch: 1`,
`board-layout.slint:369-374`), which sits inside
`column-body → card (height: 100%) → drop-zone (DropArea)` inside the
columns `HorizontalLayout` — i.e. the column's height is already
window-bounded; only the task list overflows it (unclipped, the #462
mechanism). The delegate carries explicit `width: 100%` +
`height: drag-card.preferred-height` (survive per F1b), the DragArea
seams fire `(column-index, task-index)` (model indices), and the
create field + add-task ghost are siblings **after** the loop inside
the same start-aligned layout. The compact leg
(`board-layout.slint:585-645`) has no DragArea wrapper (delegates are
bare `TaskCard`s, `compact: true`), addresses everything through
`effective-selected()`, and its outer leg has no `width: 100%` fill
(the regular leg needed one per recorded deviation D1-b,
`board-layout.slint:158-165`) and no bounded task area.

**Today + stack (F5).** The Today page is one outer `Flickable` whose
content is a `VerticalLayout` of four `TodaySection`s, each
content-sized (`vertical-stretch: 0`, "the walk never stretches a
section into leftover window space") with the uncapped row loop at
`today-view.slint:54`. `StackView`'s task loop (`stack-view.slint:57`)
is a plain `for` of `TaskCard`s in a start-aligned `VerticalLayout`
between a fixed heading and the conditional create field.

**State-side focus (F6).** The keyboard walk is entirely TaskId-based
state: `step_focus` → `step_flat_domain` over `focus_domain`
(reading order on the board, effective stack in compact) or
`today_domain` (four sections concatenated; `view_state.rs:548-654,
638-654, 693-717`). Focus moves return `FlowOutcome::RefreshNow`; the
adapter's effect interpreter maps `FocusNext`/`FocusPrev`/
`FocusFirst`/`FocusLast` onto `advance_focus`/`retreat_focus`/
`jump_focus` (`adapter.rs:694-735`) and applies outcomes via
`apply_outcomes` → `refresh` (`adapter.rs:767, 394, 436`). The mount
seeds Board on kiosk, Today otherwise (`adapter.rs:2014-2022`). Focus
is pushed to the cards through `emphasis` in the models — the walk
never touches slint items (design round §3a).

**IdTables (F7).** Forward-only today: `board_task(column, row)`,
`stack_task(row)`, `today_row(section, row)` (`models.rs:87-112`);
`board_tasks: Vec<Vec<TaskId>>`, `stack_tasks: Vec<TaskId>`,
`today_tasks` + `today_section_rows: [usize; 4]` prefix sums
(`models.rs:52-85`). No reverse lookup exists — the seam needs one
(additive method, §3.9 D-6). The persistent handles
(`models.rs:143-201`): `columns`, per-stack `column_tasks`,
`detail_tasks`, `today_sections: [Option<Rc<VecModel<_>>>; 4]` —
`push`/`push_board` never replace a `ModelRc` (the columns row is a
struct re-assignment; `models.rs:717-724`), the D9 discipline the
virtual repeater requires.

**The bench (F8).** `benches/refresh_draw.rs`: the two skip sites are
the `if kiosk && n.div_ceil(STACK_COUNT) > BOARD_SAFE_ROWS_PER_COLUMN`
blocks in `draw_full_lanes` (`:258-268`) and `draw_partial_lanes`
(`:311-319`); `BOARD_SAFE_ROWS_PER_COLUMN = 400` (`:96`) with the #462
rationale comment (`:82-95`). `STACK_COUNT = 4`, `N_SIZES =
[100, 1_000, 10_000]` (`benches/support/mod.rs`). The partial lanes'
visible-flip guard (`:345-357`) rotates over the pinned view's
rendered head rows (`support::board_flip_targets` = first rows of
every column — in-viewport at scroll 0, so the guard survives
virtualisation). The `draw-full` warm-up guard asserts a full-frame
`NewBuffer` render (`:274-282`) — a repaint-type property, independent
of instantiation count (design round §2).

**The test instruments (F9).** The journey driver's
`find_all_by_id(id)` returns **one entry per repetition instance**
(`tests/support/driver.rs:250-261`; qualified ids like
`BoardLayout::add-col-hit` are already asserted at
`journeys.rs:1447`) — a live-instantiation counter needing zero
production-markup hooks. `find_by_label` + `Element::
absolute_position()/size()` give on-screen geometry for full-visibility
assertions. The testing backend (exact dev-pin, `pin_guard.rs`) owns
the platform per driver thread; nextest's process-per-test isolates
platforms. The goldens lane regenerates under `TASKBOARD_REGEN_GOLDENS=1`
and fails the regen run afterwards (`golden_visuals.rs:34-40, 87-90`) —
the PNG diff is the review artefact. The bench fixture
(`benches/support::build_state`) is deterministic and already consumed
from a test via `#[path]` (`tests/bench_fixture_contract.rs:7-10`) —
the deep fixture (N = 1 000 → 250 cards/column) is available to the
new tests. `slint::slint!` macro imports resolve **relative to the
invoking .rs file** (`slint-macros-1.18.1/lib.rs:357`), so a
test-local probe component can `import` the real in-house ListView
source. `slint` is a normal dependency of the crate; the testing
backend and `i-slint-core` are dev-dependencies (`Cargo.toml`).

**TaskCard heights (F10).** Card preferred-height is layout-solved
(title role body↔caption + conditional due/scheduled/effort chip rows
+ the compact-collapsed decoration row, `task-card.slint:111-177`) —
no closed-form height function exists or should be created (one source
of truth: the layout). Heights vary ~48-70 px at base scale; the
fixture's worst-case stride is ~77 px (bench comment, F8).

## 3. The design, milestone by milestone

### 3.1 M1 — the in-house ListView component + its engagement probe

**D-1 — the component** (`ui/widgets/listview.slint`, new):

```slint
// SPDX-License-Identifier: AGPL-3.0-only
// The in-house virtualised list (the #373 design round, route 2).
// The element NAME and the five length properties are the slint
// compiler's nominal ListView contract (object_tree.rs:2891-2905):
// rename any of them and the `for` below silently renders eager —
// tests/virtualisation.rs's engagement probe fails on purpose.
component ListView inherits Flickable {
    in-out property <length> visible-height: self.height;
    in-out property <length> visible-width: self.width;
    // The focus-into-view seam (design round §3a): the consumer
    // binds request-row/-nonce/-total-rows; a nonce change seeks.
    in property <int> request-row: -1;
    in property <int> request-nonce: 0;
    in property <int> total-rows: 0;
    changed request-nonce => {
        if (self.request-row >= 0) {
            self.scroll-row-into-view(self.request-row);
        }
    }
    // Top-anchors the row using the runtime's own cached average
    // (content-height/total-rows) — the exact divisor the runtime's
    // 1.5-screen random seek inverts (repeater.rs update_visible_
    // instances), so the seek lands the row live at the window top.
    // Guard: no-op while the first layout is pending (avg == 0) and
    // when the estimated range already holds the row.
    public function scroll-row-into-view(row: int) {
        if (row < 0 || self.total-rows <= 0) { return; }
        avg-est := self.content-height / self.total-rows;
        if (avg-est <= 0) { return; }
        top := row * avg-est;
        if (top < -self.content-y
                || top + avg-est > -self.content-y + self.visible-height) {
            self.content-y = -top;
        }
    }
}
```

(Stage 2 adjusts pure-slint mechanics — e.g. exact `changed`-handler
syntax, `pure` qualification — freely; the **binding** parts are the
name, the five properties, no `content-height`/`content-width`
binding anywhere, and the seek formula. The doc comment above stays
with the file.)

Decisions folded in, with their rejections:

- **Inherits `Flickable` directly** (F3), not a ScrollView copy:
  the built-in already owns `content-x/y/width/height`; a ScrollView
  clone would re-import scrollbar plumbing route 2 exists to avoid.
  Alternative rejected: a bare `component ListView {}` with an inner
  Flickable — the `for` must be the ListView element's direct child
  for the contract, and children of the inner Flickable would not be.
- **Never bind `content-height`/`content-width`** (F1a): an explicit
  binding removes the runtime's driver and silently kills the
  estimate — the exact silent-failure mode the probe exists for.
- **No scrollbar, no chrome (D-2, recorded against the theme
  ladder)**: the theme ladder (background < surface < surface-raised,
  hover-guarded, `theme.slint:30-45`) has no scrollbar track/thumb
  token; introducing one is a visual-identity round of its own (the
  D35 discipline), and the std ScrollView chrome is precisely the
  visual-identity cost route 2 avoids. The thumb would be an
  approximation anyway (avg-height content estimate, F2 — documented
  slint behaviour with variable heights). The kiosk — the primary
  surface — navigates by keyboard and wheel; drag-pan stays enabled
  (std's `mouse-drag-pan-enabled: false` is a scrollbar-era choice we
  do not copy). If a scrollbar is ever wanted it is an additive child
  consuming `content-height`/`visible-height` — a separate, later
  round.
- **`scroll-row-into-view` top-anchors flush** (margin 0), the std
  `bring-into-view` shape: any margin m shifts the seek's floor
  (`floor((row×avg − m)/avg) = row−1`) and admits a top-clip of up to
  (avg − min-height) − m under height variance; flush anchoring keeps
  the focused row the window's first instance (its on-screen top is
  exactly its own height deviation from avg, a few px). The estimated
  range guard makes adjacent-step navigation a no-op when the next
  card is already visible — the keyboard walk does not re-anchor the
  view on every step.
- **The exact-trim fallback (named, deliberately NOT built)**: if the
  acceptance test exposes a variance case the flush seek misses (the
  est-range guard misjudging by more than a card height), the
  recorded fallback is a delegate-anchored trim — the focused
  delegate corrects `content-y` from its real geometry in a `changed`
  handler. Not built now: no speculative generality (the code-quality
  mandate), and F2's divisor identity says the seek lands the row
  live; the test decides.

**D-3 — the virtualisation-engagement observable.** The failure mode
this probe must catch (dispatch prompt, verbatim force): a silent
non-virtualising ListView renders fine on small fixtures and forfeits
every win — goldens and journeys cannot see it. The observable is the
**live-instantiation count**: engaged, a deep model renders a
viewport-sized window of instances (F2 splices everything else out);
eager, all N instances exist. Instrument: the journey driver's
`find_all_by_id` (one entry per live repetition instance, F9) — no
production-markup hooks, no counting callbacks in shipped markup.

M1's probe (`tests/virtualisation.rs`, new) proves the instrument's
discriminating power first, then the component's engagement:

1. **The eager control** — a test-local `slint::slint!` component
   (imports resolve relative to the .rs file, F9) with a plain `for`
   of 300 sized delegates in a 400 px viewport: assert the counted
   instances == 300 (the probe's counting path works and the eager
   shape is what we think).
2. **The engaged probe** — the same test-local component with the
   `for` placed inside the real `ListView` (imported from
   `ui/widgets/listview.slint`): assert counted instances ≥ 1 and
   `instances × 4 < 300` (a detector of the eager failure, not a
   success ceiling: strictly better virtualisation still passes; a
   lost name or any lost property fails it — F1). Assert also that at
   least one delegate is findable by label (guards the degenerate
   "engaged but renders nothing" breakage).
3. **The seek smoke** — the engaged probe component: drive
   `request-row`/`request-nonce` to a deep row, advance the mock
   clock, assert the delegate at that row is now instantiated (the
   seek made it live) and its `absolute_position()` sits inside the
   viewport.

The M1 commit is red-green by construction: the tests are written
first against the not-yet-existing component, the component lands in
the same commit. No AppWindow/leg changes in M1 — the component is
exercised through the test-local host exactly because the contract is
per-`for`-parent and must be proven before any leg depends on it.

### 3.2 M2 — the board legs (regular + compact), focus-into-view, journeys

**D-4 — the leg swap (regular).** Inside the task-area VerticalLayout
(`board-layout.slint:369-446`), the `for task[task-index] in
column.tasks` loop is wrapped — unchanged, same model, same delegate
markup — as the direct child of a `ListView` instance:

- `lv := ListView { min-height: 0; for task[task-index] in
  column.tasks: drag-zone := DragArea { ... } }` — the delegate
  subtree is byte-identical to today's (F1b: the explicit
  width/height bindings survive; the DragArea seams keep
  `(column-index, task-index)` = model indices).
- **`min-height: 0` is load-bearing**: a Flickable's in-layout
  min/preferred derive from content; without the explicit floor the
  column's layout minimum explodes to the content height (the D19
  elide lesson) and the solver cannot shrink the viewport. With it,
  the start-aligned task area compresses the ListView to
  (available − ghost − create-field − spacings) exactly when content
  overflows, and leaves it content-sized when content fits —
  **fitting-content pixels are unchanged**, which is what the goldens
  pin. The ghost and create field stay siblings after the ListView,
  hugging the content for short columns (today's r2 D8 language) and
  sitting at the viewport's bottom edge for deep ones (where today
  they render 30 k px down, off-window).
- The seek bindings, fed by the global hint carrier (D-5):
  `request-row: FocusScrollHint.target-a == column-index ?
  FocusScrollHint.target-b : -1;` `request-nonce:
  FocusScrollHint.target-a == column-index ? FocusScrollHint.nonce :
  0;` `total-rows: column.tasks.length;`

**D-4b — the compact leg.** Same swap around the bare-`TaskCard`
`for` (`board-layout.slint:603-610`), addressed through
`effective-selected()` exactly like today's callbacks. Additionally
the compact leg gains the regular leg's recorded D1-b fill
(`width: 100%` on the leg's outer VerticalLayout, vertical-stretch on
the column block) — without a bounded viewport there is nothing to
virtualise; fitting-content pixels stay start-stacked (goldens
verified at regen). Delegate id: name the compact card
(`compact-task := TaskCard`) so `find_all_by_id` can count it.

**D-5 — the focus-into-view seam: carrier and resolution.** API
shape decision (dispatch prompt names the two candidates; the
per-column-property shape wins):

- **Carrier: a new global `FocusScrollHint`** (`ui/focus-scroll.slint`,
  imported + re-exported in `app-window.slint` beside `DragState`):
  `in property <int> target-a; in property <int> target-b; in
  property <int> nonce;` — `target-a` is the surface's primary index
  (board column / today section), `target-b` the row; the stack-detail
  surface reads `target-b` alone (a = −1). One view is instantiated at
  a time (the Q4 one-view-tree), so the three surfaces cannot
  disagree about who consumes a hint. The nonce makes identical
  re-moves (same column/row twice) edge-triggered.
- **Rejected: per-column `in` properties pushed from Rust** — Rust
  would have to materialise a per-column array on every focus move
  (state-shaped plumbing for an event) and BoardLayout would re-diff
  it; the global hint is one property write consumed by exactly the
  matching ListView instance.
- **Rejected: Rust calling a slint function per column** — functions
  on repeated instances are not addressable from the generated API;
  it devolves to the property-diff mechanism above.
- **Resolution: Rust-side, through the IdTables (the dispatch
  acceptance's "computed through the id tables")**: the adapter
  resolves the newly focused `TaskId` to its (column, row) /
  (section, row) / row **via the IdTables' new reverse helpers**
  (D-6), then writes the global — after `apply_outcomes` of a
  focus-changing effect, so the models and the hint land in the same
  deterministic order (hint last, `adapter.rs:767` region).
- **Trigger set: the four keyboard focus moves only**
  (`FocusNext`/`FocusPrev`/`FocusFirst`/`FocusLast`, plus the compact
  switcher's land-on-first-task path that `step_focus` folds into the
  same outcomes): detect a focus change by comparing
  `view_state.focused_task()` before/after the outcomes. Pointer
  selections (`activate_card`, header/switcher clicks) are visible by
  construction and hint nothing.
- **Acceptance semantics**: "the focused card is **fully visible**
  after the move" — asserted through real geometry
  (`find_by_label` → `absolute_position()/size()` within the window),
  never an exact `content-y` pin (heights vary, F10).

**D-6 — the IdTables reverse helpers (additive, not a shape
change).** New read-only methods on `IdTables`
(`models.rs`, beside the forward ones):

- `board_position(&self, task: TaskId) -> Option<(usize, usize)>` —
  a scan over `board_tasks` (O(N) per keyboard event, trivial next to
  the 20 ms refresh it accompanies),
- `today_position(&self, task: TaskId) -> Option<(usize, usize)>` —
  scan over `today_tasks` + `today_section_rows` prefix sums,
- `stack_position(&self, task: TaskId) -> Option<usize>` — scan over
  `stack_tasks`.

No new fields, no semantic change — the non-goal ("no IdTables shape
change") is respected; the design round §3b's documentation duty (UI
row indices are model indices) lands as a doc comment on the struct.

**The M2 tests** (TDD: each written red before the change that greens
it; all in `tests/virtualisation.rs` unless named):

1. **Engagement, board regular**: mount `build_state(1000)` via the
   bench fixture (`#[path]` precedent, F9), escape to the board
   (`NavFocusBoard`, the goldens' idiom), window 1000×900,
   `advance_time(0)`, then `find_all_by_id("BoardLayout::drag-zone")`:
   ≥ 1 and `count × 4 <` total cards (250/column; eager would be
   ≈ 1000). Also one visible head card findable by label.
2. **Engagement, board compact**: same fixture, window 600×900,
   count `BoardLayout::compact-task` likewise.
3. **Focus-into-view, board regular**: same mount; `FocusFirst`
   (card 0 — assert visible at its rest position), then `FocusLast`
   (the reading order's last card — far off-viewport): after the
   mock-clock pass, the last card's element exists, and
   `0 ≤ top ∧ top + height ≤ window height` — fully visible. Guard
   first: assert the card was **not** fully visible before the seek
   (a move that starts visible proves nothing).
4. **Adjacent steps do not scroll away**: from the top, one
   `FocusNext`; card 0's successor becomes focused; card 0 remains at
   its rest position (the est-range guard's no-op path).
5. **Drag seams keep model indices**: the F11-family drag journey
   (`the_drag_survives_interleaved_refreshes_and_engine_changes` and
   the resize-drag journeys) runs **unmodified** — green before and
   after; plus one new drag arm/drop across a **virtualised** column
   pair at deep N: arm on a head card of column 0, drop on column 1,
   assert `IdTables::board_task` resolves the same card before and
   after (via the task-move request the FakeEngine captures — the
   house fake, not a mock).
6. **D9 survival**: `consecutive_refreshes_keep_the_board_model_
   identity` and the double-click journey run unmodified and green
   (the persistent handles guarantee no `reset()`); the diff adds no
   `ModelRc` swap (stage-3 check h).
7. **Goldens**: run the golden lane; expected **no-op** (fitting
   content is pixel-identical, D-4). Any drift is diagnosed before a
   regen (D-8) — the regen PNG diff is the review artefact.

### 3.3 M3 — Today sections + stack detail, anchor-on-removal (#461)

**D-7 — the Today section swap.** Each `TodaySection`'s row loop
(`today-view.slint:54-78`) becomes a ListView instance wrapping the
unchanged row delegate (named `today-row :=` for the count probe);
the row callbacks keep `(section-index, rindex)` model indices. The
page keeps its outer Flickable (page-level scrolling between
sections). The section ListView is content-sized below a bound and
viewport-bounded above it: the bound arrives as
`in property <length> viewport-height` on `TodaySection` (bound from
`TodayView`'s own height — the page viewport), and the ListView caps
at `max-height: viewport-height − 2*Theme.space-3` (the page
padding; the coarse bound is deliberate — it only bites on overflow,
and fitting-content pixels are untouched). Consequence recorded: a
section taller than the page scrolls **internally**; wheel chaining
at the inner boundary is slint's Flickable behaviour (rejects when
unscrollable → the outer page scrolls) — verified interactively at
stage 2, pinned by no test (deterministic tests only; no journey
depends on scroll feel). The seek bindings mirror D-4 with
`target-a == sindex`; `total-rows: section.rows.length`; the hint
carrier is the same global (one-view-at-a-time, D-5).

**Stack detail**: the task loop (`stack-view.slint:57-62`) gets the
same swap with `vertical-stretch: 1` + `min-height: 0` (it is the
only stretchable child between the fixed heading and the conditional
create field — the create field stays **below** the viewport, always
visible when opened, where today a deep list pushes it off-window).
Delegate named `detail-task :=`; seek bindings read `target-b`
alone.

**The M3 tests**:

1. **Engagement, Today + stack**: deep overdue fixture (see 2),
   count `TodayView::today-row` and `StackView::detail-task`
   instances — viewport-scale, the D-3 detector ratios.
2. **Anchor-on-removal, row-anchored (the #461 pin)**: a state with
   enough overdue tasks to overflow a section viewport at the test
   window (synthesize via the `fixture.rs` helpers; ~40 overdue rows
   × ~60 px ≫ the 900 px bound — the goldens' `today_overdue_state`
   if already sufficient, else an extension). Drive the section deep
   **by keyboard** (the Today walk over `today_domain` — N
   `FocusNext` strokes; each seek top-anchors, so the viewport ends
   far from row 0 — this doubles as Today's focus-into-view
   exercise). Then flip one **above-viewport** overdue task's `done`
   through the state store + refresh (the bench's `flip` shape, F8;
   the removal leaves the Overdue list). Assert: the on-screen rows
   did **not** move — the focused card's `absolute_position()` is
   unchanged (within float equality of the same read) across the
   removal. That is row-anchoring's user-visible contract: a
   pixel-anchored repeater (no offset shift, repeater.rs:549-610)
   shifts every on-screen row up by the removed row's height and
   fails the assertion — the test fails if the anchor behaviour is
   removed, per stage-3 check (d).
3. **Compact stack-switch remount (r2 D8 preserved)**: compact
   window, two non-empty stacks; switch stacks by keyboard; assert
   the new stack's head card is findable at the column's rest
   position and the old stack's card labels are gone — the model
   swap re-mounts the single column's viewport cleanly (scroll at
   top, no stale instances), never a mid-scroll half-state.
4. The goldens lane again: expected no-op (both Today and stack
   changes only bite on overflow).

### 3.4 M4 — the bench un-skip + measurement mechanics

**D-9 — the skip lift.** Remove `BOARD_SAFE_ROWS_PER_COLUMN` and both
skip blocks (`refresh_draw.rs:96, 258-268, 311-319`): under
virtualisation the board's live raster geometry is viewport-scale at
every N — the i16 ceiling (#462) is unreachable by construction (the
Today view's 183 k px list-depth precedent, re-measure §5). The lane
doc comments (F8's `:16-29`) are rewritten to the virtualised reality;
the visible-flip guard and the full-frame warm-up guard stay
untouched (board flip targets are head rows, in-viewport at scroll 0,
F8). **The lanes keep their names and shapes** — only the skip
disappears.

**The measurement mechanics** (dev-machine references, never gates;
quiet-box discipline per the re-measure's §2 header): run the four
board cells (`draw-{full,partial}/board/{1000,10000}` — 100 was never
skipped) plus a `today` and `refresh` spot-check to prove the Today
lanes and the guard are untouched by the un-skip. Record the
before/after pair: **before** = the re-measure report §2 (board/10000
SKIPPED, #462 panic; board/1000 partial 14.18 ms vs full 5.96 ms —
the inversion), **after** = this run's numbers + region lines.
Expected shape (a reading guide, not an acceptance number): board
draw-full collapses toward viewport scale at every N (the eager
per-card paint constant ~6.8 µs/card, design round §4, no longer
multiplies by N), draw-partial's region collapses from the 83 %
full-columns-reflow toward card-scale, and the N = 10 000 cells
measure instead of panicking. If the partial region stays large (the
re-measure §3.5 column-re-solve bimodality), that is a **finding to
record**, not a plan failure — the instrument reports it.

The before/after pair goes into stage 2's report → stage 3's PR body
and a short addendum section in the re-measure report (stage 3's
choice per the dispatch). The refresh lane stays unpinned and
unchanged (its regression is #390's side finding, referenced, not
absorbed).

**D-10 — no in-tree unvirtualised reference lane (recorded deviation
from design-round §3d point 3).** The design round asked for an
unvirtualised reference alongside the virtualised lanes; the user's
widened scope then virtualised **every** N-task surface, so an
in-tree eager reference would require a parallel non-virtualised
markup path — a second code path existing only to be benchmarked
(dead code / speculative generality, against the code-quality
mandate). The eager before-numbers are preserved in-repo (the two
2026-10-26 reports; this plan's §2 F8) and the M4 addendum completes
the pair. Recorded: if a future regression hunt needs live eager
numbers, `git revert` of the leg commits on a scratch branch
reconstructs the eager bench exactly.

### 3.5 Bookkeeping the chunk owns

- `models.rs`: the §3b invariant doc comment (UI row indices are
  model indices) at the IdTables declaration + the three reverse
  helpers (D-6). Nothing else in `models.rs`.
- No CHANGELOG line for the bench change (the instrument-fix
  precedent, re-measure §7); the ListView itself is user-visible —
  stage 2 adds the CHANGELOG entry for the UI change (scrolling
  columns/lists) per the release conventions.
- Issue annotations (stage 2 reports text, stage 3 posts — dispatch
  prompt 1.2 rule): #461 (anchor-on-removal landed), #462
  (virtualisation is the structural fix; the board renders at
  N = 10 000 again). #373 stays open for stage 3's PR to close.

## 4. Decision record

| # | Decision | Alternatives rejected (why) |
|---|---|---|
| D-1 | `component ListView inherits Flickable` carrying only `visible-*`; the five-property contract satisfied through the built-in's own `content-*` (F3) | ScrollView clone (re-imports std chrome); bare component + inner Flickable (the `for` wouldn't be the ListView's direct child — contract dead) |
| D-2 | No scrollbar/chrome; wheel + drag-pan + focus-into-view are the affordances | std chrome (route 1's rejected cost); a themed scrollbar (no ladder token; approximate thumb; a visual round of its own — named as the later additive option) |
| D-3 | Engagement observable = live-instantiation count via `find_all_by_id` (+ a test-local probe with an eager control proving discrimination) | render-geometry facts (cannot distinguish engaged-but-broken); timing (never a gate); a11y-role counts (same mechanism, less precise); prod debug hooks (rejected on principle) |
| D-4 | Legs: wrap the unchanged `for` in a `min-height: 0` ListView inside the untouched start-aligned task area; fitting content stays pixel-identical | vertical-stretch ListView (short columns: ghost pinned to column bottom — gratuitous golden churn for all 12 shots); max-height arithmetic (duplicates the layout's spacing math); alignment-removal (same churn) |
| D-4b | Compact leg gains the D1-b fill + a bounded task ListView; delegates named for the probe | leaving compact unvirtualised (it is an N-task surface per the commissioning decision) |
| D-5 | Focus-into-view: global `FocusScrollHint {target-a, target-b, nonce}` set by the adapter after the refresh of a keyboard focus move; each ListView filters by its own index pair | Rust-pushed per-column properties (state plumbing for an event); Rust→slint function calls (not addressable on repeated instances); adapter-side pixel math (heights are slint facts, F10) |
| D-6 | IdTables gains three additive read-only reverse-scan methods | a reverse HashMap field (shape change, rebuild cost per pass — the non-goal); resolving slint-side (the id tables are the resolution base by D2 convention) |
| D-7 | Today: per-section ListView, content-sized below the viewport bound, outer page Flickable kept; bound = `viewport-height − 2*space-3` threaded as an explicit property | section always-viewport-height (destroys the page model — a visual round); dropping the outer Flickable (four fixed panes — same objection); unbounded sections (no virtualisation) |
| D-8 | Goldens: expected no-op; any drift is diagnosed before regen; no deep-content goldens added | silent regen (drift = a regression, not a new baseline); deep-fixture goldens (the golden tier pins fitting-content identity; deep behaviour is the probe's) |
| D-9 | Skip lifted by deleting the constant + both blocks; guards untouched; lanes unchanged in name/shape | keeping the skip for "reference" (it would keep skipping the virtualised board's honest measurements); renaming lanes (breaks the before/after comparability) |
| D-10 | No in-tree unvirtualised reference lane | a parallel eager markup path (dead code); an env-gated markup fork (same, plus a second golden/bench matrix) |
| — | Seek: flush top-anchor via `content-y/total-rows` (the runtime's own divisor — F2's identity); est-range guard for adjacent steps | delegate-anchored exact trim (named fallback, built only if the acceptance test demands); margin-anchored seek (admits a top-clip under variance, F2 analysis); Rust-side height ledgers (presentation state in Rust, two sources of truth) |

## 5. Testing strategy (per AGENTS §5)

- **Layer discipline**: engagement + seek + anchor are
  presentation-layer facts — they live in `tests/virtualisation.rs`
  (one new file) and never duplicate through the StateEngine (which
  knows nothing of views). The IdTables reverse helpers get
  state-side unit tests in `models.rs`'s existing test module (pure
  table lookups; one behavior one test at the lowest layer).
- **Determinism**: every test drives the mock clock
  (`advance_time(0)` after property writes — the goldens' idiom) and
  asserts terminal facts (element presence, on-screen geometry,
  counts). No sleeps; no transient intermediate asserts; no wall
  clock. The anchor test asserts the focused card's position is
  **identical across a removal** — a terminal relation.
- **No text assertions**: visibility/geometry via
  `absolute_position()/size()`; identity via labels-as-addresses and
  ids; model effects via the FakeEngine's captured requests (typed).
- **No quality ceilings**: the engagement ratios are failure
  detectors on garbage (eager instantiation), explicitly not success
  ceilings — the litmus holds (strictly better virtualisation still
  passes). Bench numbers are recorded references, never asserted.
- **The probe is permanent**: the M1 tests must never be
  `#[ignore]`d or fixture-starved — they are the slint-bump guard for
  the undocumented compiler contract (the `pin_guard.rs` pattern,
  applied at runtime instead of manifest level). A future slint bump
  that breaks the nominal contract fails CI here, not in a kiosk.
- **Journey/golden updates**: none anticipated. The named journeys
  (F8/F11-family, model-identity, repetition probes) run unmodified;
  the golden lane runs and is expected to be a no-op (D-8). If stage
  2 finds a real journey breakage, that is a finding against this
  plan's design (report it; do not weaken the test — AGENTS §5).
- **Coverage**: the new slint file is markup (the coverage gate's
  ui-slint crate handles it as today); the new Rust surface is the
  three helpers + the adapter hint block — covered by the unit and
  virtualisation tests above, no exclusions.

## 6. Risks

1. **The nominal contract is undocumented and version-pinned** (F1):
   a future slint bump can break it silently. Mitigation: the
   permanent M1 probe (§5) + the exact-pin bump discipline
   (`pin_guard.rs`). Residual: a bump could also change the runtime's
   seek/anchor mechanics (F2) — the anchor pin and seek smoke catch
   behavioural drift; treat both as bump-gate tests.
2. **Layout-min explosion**: without `min-height: 0`, Flickable
   content-derived minimums could blow the column/page layouts (the
   D19 class of failure). The explicit floor is in the markup;
   the goldens + the ten-step session catch any residue.
3. **Nested Flickables (Today page + sections)**: no drag-pan
   chaining at the inner boundary; wheel chaining is Flickable
   behaviour to verify interactively (D-7). No test depends on it;
   if it proves unusable the fallback is the same bound with the
   outer Flickable's sections replaced by stacked fixed-height
   sections — a visual round, escalated to the user, not silently
   shipped.
4. **Seek races the runtime's re-anchor**: our `content-y` write and
   the repeater's own re-anchor are single-threaded and lazily
   re-evaluated; tests assert terminal geometry only (§5). Residual:
   an animation era could interleave — none exists today.
5. **Height-variance seek misses** (stale cached average across
   windows with mixed chips): the acceptance property is
   full visibility, and the flush top-anchor keeps deviations at
   one-card scale; the named fallback (delegate-anchored trim) is
   built only if the test exposes a real miss.
6. **The M4 numbers may still miss budget row 3's partial leg on
   kiosk silicon** — expected as a possible outcome (the dispatch's
   completion bookkeeping routes it to the upstream
   damage-tracking conversation "armed with per-view numbers");
   the instrument records it, nothing here gates on it.
7. **Goldens drift beyond the expected no-op** — diagnose-then-regen
   (D-8); the PNG diff is the review artefact either way.

## 7. References

- Dispatch plan: `.artifacts/plans/2026-10-26-listview-virtualisation-dispatch-plan.md` (§1-§2; the code-quality mandate is stage 3's, binding on stage 2 by construction)
- Design round: `.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md` (§2, §3, §5, §6)
- Re-measure: `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md` (§2-§3, §5)
- Issues: #373 (ask + scope), #461 (Today + anchor acceptance), #462 (the i16 ceiling), #460 (the instrument, landed)
- Pinned sources: `i-slint-compiler-1.18.1/object_tree.rs:2891-2905`, `passes/repeater_component.rs:84-99`, `builtin_elements.rs:1494-1520`, `widgets/cosmic/scrollview.slint`, `widgets/common/listview.slint`; `i-slint-core-1.18.1/model/repeater.rs:263-446, 531-616`; `slint-macros-1.18.1/lib.rs:357`
- AGENTS.md §2 (crate boundaries), §5 (testing), §8 (worktree/PR), §10 (gates)

## 8. Suggested commit sequence (stage 2's; each standalone + gates-green)

1. `feat(ui-slint): the in-house ListView and its engagement probe` —
   `ui/widgets/listview.slint`, `tests/virtualisation.rs` (probe +
   eager control + seek smoke).
2. `feat(ui-slint): the board columns through the ListView` —
   board-layout both legs, `ui/focus-scroll.slint` +
   `app-window.slint` wiring, adapter seam + IdTables helpers (+ unit
   tests), board engagement/seek/drag tests.
3. `feat(ui-slint): the today sections and stack detail through the
   ListView` — today-view, stack-view, Today/stack engagement +
   anchor pin + compact remount pin.
4. `fix(bench): measure the board cells at every N` — skip lift +
   lane docs; the numbers go in the report, not the commit.
   (Goldens regen rides whichever commit first shifts a pixel —
   expected none; any regen is its own diagnosed commit. CHANGELOG
   entry with the first user-visible commit.)

## 9. Estimated diff size

≈ 550-700 lines touched: ~130 slint production (new ListView ~45, new
global ~15, board ~45, today ~25, stack ~15, app-window ~3), ~90 Rust
(adapter seam ~45, IdTables helpers + docs ~35, CHANGELOG ~5), ~350
new tests (`tests/virtualisation.rs`), ~−40 bench (skip lift) + doc
rewrites, goldens expected no-op. Size L, single PR, no stacking —
per the dispatch plan §1.
