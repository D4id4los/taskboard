# [Report] ListView virtualisation design round — board columns (#373), with the §0 dirty-region probe

- **Date**: 2026-10-26
- **Branch**: `docs/listview-virtualisation-analysis` (off `origin/main`
  = `a1bc03d`, the instrument-fix report's merge). Design round: the
  deliverable is this report, not an implementation.
- **Machine**: dev box (AMD Ryzen 9 5900HX, 30 GiB, rustc 1.99.0,
  slint 1.18.1, software renderer, 1366×768) — probe runs were
  interactive, single-process, no concurrent heavy load; every time
  below is a **dev-machine reference, never a gate** (the on-device
  verdict belongs to the kiosk wave; the 100 ms repaint cadence
  (C2/F13-09) floors composed latency regardless). Region findings are
  deterministic and load-independent.
- **Outcome (one line)**: the §0 probe resolves the 0×0 anomaly —
  **neither criterion nor coalescing: the bench mounts on the *Today*
  view (not the board), where most flipped tasks are not rendered at
  all, so 0×0 is honest zero-damage, and the 88 % "one-card" region is
  a filtered-list *removal* reflow**; on that corrected ground the
  design round returns **analyse-positive** — implement ListView on
  the board columns, bench-instrument first, implementation gated on
  the kiosk wave's on-device D250 verdict per the standing decision.

Every claim below carries its evidence: verbatim probe output, file
and line anchors, or the recorded reports it builds on. Probe runs
reproduced once (two independent runs of E0–E3); structural facts
identical, times within run-to-run noise.

## 1. §0 — the 0×0 probe: verdict and what it licenses

### 1.1 Method

A scratch example (`crates/taskboard-ui-slint/examples/
probe_dirty_region.rs`, **never committed** — deleted before this
report's commit; the evidence lives here as excerpts) mounting the
bench's own fixture and seam (`benches/support/mod.rs` `build_state`,
`src/test_platform.rs` `NullPlatform`/`NullWindowAdapter`), driving the
partial lane's **identical** setup→render cycle (`done` flipped on its
current value — a guaranteed one-model-row change; `ArcSwap` store;
`UiHandle::refresh()`; one `ReusedBuffer` render) in a **plain loop
with no criterion**, recording per-iteration (flipped task index,
region, render time). Because everything but criterion is
bench-identical, any effect that reproduces here is independent of
candidate (a).

Experiments: **E0** no-change control; **E1** the bench's rotation
(task 0, 1, 2, … cycling) with every iteration recorded, N=100
(220 iterations ≈ 2 rotations) and N=1 000 (120); **E2** a *fixed*
flip target (task 1) 40×; **E3** a fixed low-board flip (task 500,
N=1 000; 12×, N=10 000); **E4** pixel-truth (full `NewBuffer` renders
diffed pixel-wise around each flip); **E5** positive controls (title
flip vs done flip vs overdue-member flips).

### 1.2 The experiments (verbatim excerpts)

Run 2 (the complete decisive run; run 1 reproduced E0–E3 with the same
structures):

```
E0 no-change control N=100
  SUMMARY N=100: no-change region 0x0
E1 rotation-mirror N=100 iterations=220
  it=  0 task=   0 col=0 row=  0 region=1342x688 (12929 us)
  it=  1 task=   1 col=1 row=  0 region=0x0 (459 us)
  it=  2 task=   2 col=2 row=  0 region=0x0 (249 us)
  it=  3 task=   3 col=3 row=  0 region=1342x688 (11877 us)
  it=  4 task=   4 col=0 row=  1 region=0x0 (335 us)
  ...
  SUMMARY N=100: 145/220 iterations 0x0; zero-flip task idx range Some(1)..Some(19); first non-zero after first zero: Some((3, 1342, 688))
E1 rotation-mirror N=1000 iterations=120
  (same pattern: nonzero exactly at task 0, 3, 6, 9, ...; region always 1342x688, 98–141 ms; zeros 2.9–9.2 ms)
  SUMMARY N=1000: 80/120 iterations 0x0; zero-flip task idx range Some(1)..Some(119); first non-zero after first zero: Some((3, 1342, 688))
E2 fixed-visible N=1000 task=1 iterations=40
  it=0..39 ZERO region (40/40)
  SUMMARY N=1000: 40/40 zero regions; median render 1360 us; min 1037 us; max 2690 us
E3 fixed-off-window N=1000 task=500 iterations=40
  SUMMARY N=1000: 40/40 zero regions; median render 1235 us; min 1012 us; max 2449 us
E3 fixed-off-window N=10000 task=500 iterations=12
  SUMMARY N=10000: 12/12 zero regions; median render 30226 us; min 26759 us; max 34452 us
```

(The nonzero set in E1 is exactly `order % 3 == 0` — the fixture's
`has_duedate` pattern (`support/mod.rs:44`), i.e. the tasks that are
**overdue**; see §1.3. Run 1's E2/E3 medians were 2 827 / 2 155 /
42 665 µs — same structures, reference-grade spread.)

E4/E5 (run 3; the E4 "due task0" line is void — see §8, the flip was a
stamp-over-stamp no-change; E5(e) panicked on a probe-bug index after
its four controls completed):

```
E4 pixel truth N=100
  no-due task1 flip, FULL render: region=1366x768; actual changed pixels=0 bbox=None
  flip-back FULL render: changed pixels vs pristine = 0 (expect 0)
  no-due task1 flip, REUSED render: region=0x0
  STALE-PIXEL CHECK: reused-vs-truth changed pixels=0 bbox=None (partial region was 0x0)
E5 positive controls N=100
  title-only flip (task 1): changed pixels=0 bbox=None
  done-only flip (task 1, no due): changed pixels=0 bbox=None
  done-only flip (task 3, due): changed pixels=120810 bbox=Some((12, 79, 1353, 738))
  done-only flip (task 12, due, row 3): changed pixels=177687 bbox=Some((12, 79, 1353, 738))
```

### 1.3 The mechanism (every link code-anchored)

1. **The bench never renders the board.** `mount` seeds the view from
   the kiosk bit: `ViewState::new_starting(if kiosk { Board } else {
   Today })` (`src/adapter.rs:2015-2020`). The bench and every journey
   harness use `UiSettings::default()` → `kiosk: false` → **the
   mounted window shows the Today view**. One view is instantiated at
   a time; the board composition of `board-layout.slint` is never
   created on the bench scene.
2. **Which flipped tasks are visible is a section-membership lottery.**
   The Today view's rows are the uncapped, unvirtualised
   `for row[rindex] in root.section.rows`
   (`ui/views/today-view.slint:54`) over `push_today`'s full filtered
   lists (`src/models.rs:1130+`, no cap on section rows). The fixture's
   only populated section is **Overdue**: `overdue_tasks` = live ∧
   open ∧ `duedate < now` (`taskboard-domain/src/reminders.rs:782-786`;
   `is_open()` = `done.is_none()`, `entities.rs:451`). With
   `has_duedate(order) = order % 3 == 0` and all fixture dates in 2024,
   every third task is overdue and visible; the rest are in no section
   (their `scheduled` dates also fall outside today's window,
   `days.rs:130-137`) and are **not rendered anywhere**.
3. **A done flip is a removal, not an in-place change, for overdue
   tasks.** `None → Some(done)` makes the task not-open → it leaves the
   Overdue list → `VecModel` row removal → the repeater marks all
   subsequent instances dirty (`i-slint-core-1.18.1/model/repeater.rs:580-610`)
   → the partial renderer repaints the repeater's whole on-screen
   extent. Measured: the full content area, 1342×688 — the reported
   "88 % of the frame" — for **every** overdue flip, including
   below-the-fold ones (E1, task 57 = overdue row 19: region 1342×688).
   E5's in-place controls bracket it: the further down the list the
   removed row sits, the more shifted rows repaint (task 3 → 120 810 px,
   task 12 → 177 687 px, same bbox).
4. **For every other task the flip changes no rendered pixel.** Its
   only model change lands in the *board* view's per-column models —
   which are pushed every refresh (`models::push` derives all views)
   but whose repeater is not instantiated. E4's stale-pixel check
   confirms the 0×0 is **honest**: after the 0×0 partial render, the
   reused buffer is pixel-identical to a fresh full render of the same
   scene (0 stale pixels). Even a **title** flip on such a task renders
   zero pixel change (E5a) — the change is real in the models, merely
   not on screen.

### 1.4 Candidate verdicts

- **(a) criterion `iter_batched`/`BatchSize::PerIteration` — EXCLUDED.**
  The full phenomenon (scattered 0×0, full-content regions on every
  third flip, the run-dependent last-iteration reading) reproduces
  verbatim in a plain loop with no criterion (E1/E2/E3). The bench's
  last-iteration reading is simply `last_iteration_index mod N` landing
  inside or outside the overdue set — a 1-in-3 vs 2-in-3 coin per run,
  which is why the N=100 last iteration read 88 % in one run and 0×0 in
  the stage-3 spot-check, and why the anomaly shows up at every N (the
  original report's "at N ≥ 1 000" framing was small-sample
  coincidence, not an N-threshold).
- **(b) refresh-notify coalescing — EXCLUDED.** `UiHandle::refresh()`
  is a synchronous direct call (`src/lib.rs:428-432`); the same harness
  renders full 1342×688 regions immediately after a flip that changes
  the rendered scene, and E0 re-confirms the zero-change → 0×0
  property at the render layer.
- **(c) something else — CONFIRMED, and it is the instrument's premise,
  not the renderer**: *"a guaranteed one-row change per iteration
  should never render zero damage"* conflates a **model-row** diff with
  a **rendered** change. On the view the bench actually mounts, most
  guaranteed row changes are not rendered anywhere; their zero damage
  is correct. There is no renderer bug and no lost damage: nothing to
  fix in production code (the thread is not stopped), but the partial
  lane's instrument needs a view pin and a visible-flip guarantee
  (§3d, §7 — a bench-code change, #460, not committed here).

### 1.5 What per-iteration region semantics are licensed (feeds §3)

- Per-iteration (and last-iteration) region readings are meaningful
  **only for flips that produce a rendered change on the measured
  view**. The max-region carrier stays the honest worst case, but its
  *meaning* is now precise: it is the **removal-reflow** region of the
  Overdue list, not a per-card-change region.
- The steady-state claim in the instrument-fix report ("at N=100 the
  last iteration still reports 88 %, so this is steady state, not
  warm-up") keeps its conclusion (the max reproduced exactly across
  runs and thousands of iterations) but its *label* changes: the 88 %
  is steady-state **list-removal** damage on the Today view.
- The D9 verdicts are untouched: zero notifications on unchanged
  refresh and exactly-one-`set_row_data` on change are model-layer
  properties (E0 re-confirmed the render-layer side), independent of
  which view is mounted.

### 1.6 What this does to the recorded evidence base

- **#390's open half keeps its verdict but changes its referent.** The
  STILL-DIRTY measurement (88–91 % per one-card change; partial costs
  more than full-frame at every N: 1.57/56/567 ms vs 1.39/2.3/22.6 ms)
  is internally valid and now *explained*: it is the Today view's
  unvirtualised, uncapped Overdue list (N/3 rows) paying a full-list
  reflow per membership change, plus the per-item damage walk over all
  live rows. **The board's own per-card-change damage region is
  unmeasured** — no recorded number speaks about
  `board-layout.slint`'s kanban columns, because the bench never
  renders them.
- **The draw lanes' N-scaling is Today-view economics.** `draw-full`
  at N=10 000 (22.6 ms) paints ~3 334 Overdue cards in one list — not
  4 kanban columns of 2 500. The board view's eager-instantiation cost
  is unmeasured (same markup family, so the *class* of the problem
  transfers; the numbers do not — see §4).
- **#373's motivation survives intact, its instrument does not.** Ask
  2's success instrument ("the partial lane's reported dirty-region
  area") cannot see a virtualised board at all until the lanes pin the
  view (§3d). The refresh-lane regression remains #390's side finding,
  referenced here, not absorbed.

## 2. Feasibility on this markup

**What would change**: the inner per-column card loop
`for task[task-index] in column.tasks: drag-zone := DragArea { … }`
(`ui/compositions/board-layout.slint:386-409`) becomes a ListView
whose delegate is the existing DragArea+TaskCard pair; the outer
`for column` stays a plain repeater (columns are few, fixed by stack
count, and already width-managed by the HorizontalLayout — no
horizontal virtualisation is needed or asked for). The compact leg's
single-column loop (`board-layout.slint:603-610`) is the same change
in miniature.

**Two implementation routes** (the project imports no std-widgets
today — the UI is an all-in-house primitive set, `build.rs` uses
default compiler config):

1. **Import std-widgets `ListView`** (a `ScrollView` subclass,
   `i-slint-compiler-1.18.1/widgets/common/listview.slint:6-10`).
   Works, but drags the style's scrollbar/viewport machinery into a
   design system that paints its own ladder (theme.slint surface
   steps) — a visual-identity cost the goldens would have to absorb.
2. **A custom in-house `ListView` component.** The compiler's
   virtual-layout contract is *nominal*: a `for` whose parent element
   is named `ListView` and carries `content-y`, `content-height`,
   `content-width`, `visible-height`, `visible-width` as length
   properties gets the runtime virtual layout — *"Custom 'ListView' is
   OK, but it must have these properties"*
   (`i-slint-compiler-1.18.1/object_tree.rs:2891-2905`). A thin
   in-house component (Flickable + the five properties, the project's
   own scrollbar-less chrome) satisfies it and keeps the design system
   closed. This is an undocumented-but-stable compiler convention in
   the pinned 1.18.1; the risk is a future slint bump changing it —
   acceptable under the existing pin-together upgrade discipline, and
   worth a one-line probe in the implementation round's first commit.

**Runtime mechanics the implementation inherits**
(`i-slint-core-1.18.1/model/repeater.rs:263-446`,
`update_visible_instances`): only viewport instances exist (± the
scroll-time window); off-screen instances are spliced out; a jump
beyond 1.5 screens re-seeks from an average-height estimate;
`content-height` is `avg(item height) × row_count` — an estimate, so
the scrollbar thumb is approximate with variable heights (our cards
vary by chips: ~48–70 px at base-scale 1 — near-uniform; acceptable,
documented slint behaviour). Per-item heights are supported natively:
the delegate's `height` defaults to its own `preferred-height`
(`passes/repeater_component.rs:84-99`) and the layout walks actual
instance heights.

**Specific markup concerns, checked:**

- *Per-card heights*: supported (above). The DragArea wrapper's
  `height: drag-card.preferred-height` moves inside the delegate
  unchanged.
- *The D6 full-frame warm-up guard*: survives. The guard asserts the
  `NewBuffer` warm-up render covers the full window
  (`benches/refresh_draw.rs:174-181`) — that is a repaint-type
  property, independent of how many items the scene instantiates.
  What changes is the scene's *content* (§3d).
- *Index-only seams*: the card callbacks pass `(column-index,
  task-index)`; under ListView the delegate's `task-index` remains the
  **model index** (global, board-relative), not a viewport-relative
  one — the runtime lays out a window of instances over the same
  index space (`repeater.rs` `ensure_updated(i, idx)` pairs instance
  slots with model rows). No seam re-indexing.
- *Drag & drop*: DragArea arms/drops carry the same indices; DropAreas
  live on the outer column repeater, untouched. Drag re-orders mutate
  the model (user-move scale) — the virtual repeater's
  `row_added`/`row_removed` offset-shift logic handles mid-list
  changes with viewport-local churn.

**Feasibility verdict**: feasible on this markup with either route;
route 2 (custom ListView component) is the one that fits the design
system. The work is presentation-layer only — no crate-boundary or
actor change.

## 3. The four trade-offs, adjudicated

**(a) Focus walk & focus restoration (`view_state.rs`) — no breakage;
one new obligation.** The keyboard walk is entirely state-side:
`step_focus` → `step_flat_domain` walks `TaskId` domains derived from
`AppState` (`reading_order` = stacks in stack order × tasks in
canonical order, `view_state.rs:328-334, 548-654`); focus *is* a
`TaskId` in `ViewState`; the ring is pushed through the models
(`emphasis`, `models.rs:1117`). None of that touches slint items, so
"items outside the viewport don't exist" cannot break it — including
restoration across model changes (reconcile's normalisation keeps
`focused_task` honest by id). The new obligation: a focus move to an
off-viewport card must scroll it into view. The raw ListView has no
public `bring-into-view` (the std `StandardListViewBase`'s is
component-internal and assumes uniform heights,
`listview.slint:57-71`); the implementation computes the target
`content-y` itself — exact by walking the id table's row heights, or
approximately via the runtime's own average-height estimate.
Verdict: **adjudicated, cheap** — state-side walk unchanged; one
scroll-into-view seam, in-UI, presentation-only.

**(b) IdTables index→id mapping — board-relative survives as-is.**
`IdTables::board_task(column, row)` resolves against full-board
tables rebuilt each pass (`models.rs:52-112`), and the ListView keeps
model indices global (§2). The viewport window changes *which rows
are instantiated*, never *what the indices mean*: callbacks, the
drag seams, and the D2 index-only convention are unchanged.
Viewport-relative indexing appears nowhere. Verdict: **non-issue** —
document the invariant (UI row indices are model indices) where the
tables are declared.

**(c) Scroll-position semantics across refreshes — the D9 world
already solved the hard part.** The recorded worry ("whole-model
replacement resets/rescrolls") died with D9: `models::push` never
replaces a `ModelRc` — the per-column handles are persistent,
keyed by stack (`models.rs:654-695`), and `StackColumnModel.tasks`
carries the same handle every pass, so the outer column row only
fires on real scalar changes. A ListView repeater under a persistent
handle never takes the `reset()` path (full viewport re-instantiation,
`repeater.rs:612-615`) on refresh; in-viewport instances survive and
`content-y` is never written by the refresh pass — position survives
refreshes **by construction**. What genuinely moves the anchor:
insertions/removals above the viewport (the runtime shifts its offset
and keeps a row-anchored position, `repeater.rs:549-610`) and the
average-height content estimate drifting the thumb. On the board,
mid-column membership changes are user-move-scale; on the **Today
view's Overdue list** (see issue 2, §7) every done flip is a removal —
there the anchor story is real work, which is exactly why the Today
lists are a separate follow-up, not silent scope. Verdict:
**adjudicated, no board-side work**; the anchor-on-removal semantics
are named acceptance criteria for any later Today-view round.

**(d) The bench contract — reshaped by §1, and now the critical path.**
N's meaning under a virtualised board ("the draw lanes would measure
viewport scale") was the recorded concern; the probe adds the prior
problem: **today the lanes measure the Today view**, so virtualising
the board moves no recorded number. The contract for the
implementation round becomes:

1. **Pin the view per lane.** Board-view lanes (the actual subject)
   and Today-view lanes (the desktop default, and the current numbers'
   true referent) — the bench needs a view seam (a mount-time
   `kiosk`-style setting already exists as the lever:
   `kiosk: true` mounts on Board).
2. **Guarantee the flip is visible on the measured view** (flip a task
   that the pinned view renders; assert it: E5's title-flip control is
   the pattern — a lane whose guaranteed change renders zero pixels is
   mis-instrumented, not quiet).
3. **Keep an unvirtualised reference.** With view-pinned lanes, the
   unvirtualised board is the `kiosk: false`-equivalent fixture of the
   same lane pair; N keeps its board-scale meaning in that lane, and
   the virtualised lane reports viewport-scale costs alongside. The
   region carrier gains the §1.5 semantics (max = worst case;
   per-iteration readings only for visible flips).

Verdict: **adjudicated — the instrument work is a precondition, not a
follow-up** (Ask 2's success instrument is unmeasurable without it).

## 4. Expected win, per lane (measured numbers; references, never gates)

| Cost term | Measured today (dev box) | Under board-column virtualisation |
|---|---|---|
| `draw-full` as recorded (Today view, ~N/3 Overdue rows) | 1.39 / 2.32 / 22.6 ms at N = 100 / 1 000 / 10 000 | **unchanged by board virtualisation** — the board is not rendered; moves only if the Today lists virtualise (issue 2: toward viewport scale, ~10–16 rows) |
| Board-view `draw-full` (unmeasured; same eager-`for` family) | unmeasured — the implementation round's first baseline | N cards → 4 × viewport (~40–64 cards) at any N; the measured per-card paint constant (~6.8 µs/card at N=10 000) transfers as an order of magnitude, subject to the round's own baseline |
| `draw-partial`, visible-change case as recorded (Today removal reflow) | 56.2 / 566.5 ms at N = 1 000 / 10 000 | the live set shrinks to the viewport, so both the reflow extent and the per-item damage walk shrink with it — viewport-scale at any N |
| Zero-damage partial render (the walk paid anyway) | **~30–43 ms at N=10 000 even for 0×0** (E3, two runs) — per-item `compute_dirty_regions` bookkeeping over all ~N live items, ~1.3–2× the full paint per item | **deleted**: the walk visits only live (viewport) instances — this is the cleanest measured statement that virtualisation attacks the dominant residual term |
| `refresh` lane | 60.4 µs / 807 µs / 22.15 ms | **unchanged** — the diff pass walks the full desired model regardless of what the renderer instantiates (the lane's regression is #390's side finding, referenced, not absorbed) |

Reading: the only lever in-house that changes the **scaling class**
(board-size → viewport-size) is virtualisation; upstream per-row
damage tracking improves constants, not the class. On kiosk-class
silicon (the row-3 envelope: ≤ 50 ms full / ≤ 20 ms partial at
1366×768), the measured dev-box constants put any-N≥1 000 board draw
outside the envelope by ~an order of magnitude before the hardware
ratio is even applied — consistent with the 2026-10-21 netbook
context (D250 slot 2). The on-device verdict stays the kiosk wave's;
implementation remains gated on it (standing decision).

## 5. Interaction with the D9 row-diff push

The three model-notification paths map cleanly onto the virtual
repeater (`i-slint-core-1.18.1/model/repeater.rs:531-616`):

- **`row_changed` (the D9 diff's in-place path)**: for an **in-viewport**
  row, the live instance is updated in place (`comp.update(row, …)`,
  line 536-546) — the same stable-identity semantics the repetition
  probe pinned for plain repeaters (no re-instantiation; TouchAreas
  survive). For an **off-viewport** row: `instances.get_mut(row −
  offset)` misses → **a complete no-op** — no instantiation, no damage,
  zero cost. D9's per-row push and ListView compose; they are
  complementary, not competing.
- **`row_added`/`row_removed`** (moves, creates): offset-shift with
  viewport-local instance churn — user-move-scale, fine.
- **`reset`** (whole-model swap): clears all instances — full viewport
  re-instantiation per refresh. This is why **D9's persistent handles
  are load-bearing for virtualisation**: with the pre-D9
  fresh-`VecModel`-per-refresh push, a virtualised board would still
  re-instantiate its viewport every refresh. The composition is: D9
  makes ListView worthwhile; ListView makes D9's savings *visible* at
  the renderer.

**Does ListView shrink the renderer's damage region (#390's open
half)?** Yes, by construction of the live set: the probe showed the
coarse case is *repeater-extent* damage (removals repaint the whole
list extent regardless of the removed row's position — E1 task 57),
and in-place changes are per-item (`partial_renderer.rs:543-653`
tracks each item's own box). A virtualised repeater's extent *is* the
viewport, so even today's coarse marking lands at viewport scale.
Upstream per-row damage tracking remains the right lever for
board-scale repeaters, but it is **not a precondition** for
virtualisation's win.

## 6. Recommendation (one)

**Analyse-positive: implement, in this order, implementation still
gated on the kiosk wave's on-device D250 verdict (standing decision).**

1. **Bench instrument first** (bench-code only; #460): view-pinned
   lanes (Board + Today via the existing `UiSettings.kiosk` lever), a
   visible-flip guarantee per lane, and the §1.5 region semantics in
   the carrier. This un-freezes Ask 2's success instrument and gives
   the implementation its board-view baseline (the unmeasured column
   of §4's table).
2. **Implement ListView on the board columns** — route 2, the custom
   in-house `ListView` component (§2), the inner card loop per column
   (regular + compact legs), the focus-into-view seam (§3a),
   IdTables untouched (§3b), persistent handles untouched (§5).
   Board-view partial/full lanes move toward viewport scale; the
   unvirtualised reference lane stays (§3d).
3. **Then the Today view's section lists** (out of #373's board scope;
   #461) — the same mechanism on the surface the current bench numbers
   actually measure, and the desktop default view, where
   removal-reflow damage is a per-done-flip occurrence rather than a
   user-move rarity.

What would change this answer (the analyse-negative triggers, for the
record): an on-device D250 verdict showing the row-3 envelope met at
the kiosk's real board scales (then both levers are theoretical), or
the implementation round's route-2 probe finding the custom-ListView
compiler contract broken on the pinned slint (then route 1's style
cost becomes the decision).

## 7. Consequences for later work

Filed as backlog issues (out-of-scope here by #373's board-columns
framing; the report is the permalink target once merged):

1. **#460 — bench instrument: view-pinned lanes + visible-flip
   guarantee.** The §0 probe's permanent home: the guard that would
   have caught the 0×0 (a lane whose "guaranteed change" renders zero
   pixels asserts, not shrugs), the Board/Today lane split, and the
   board-view damage baseline the implementation round needs.
2. **#461 — Today-view section-list virtualisation.** The
   unvirtualised, uncapped Overdue list is the measured N-scaling
   offender (§4 row 1) and sits on the desktop default view; it
   inherits §3c's anchor-on-removal acceptance criteria.

Referenced, not absorbed (their own homes): the refresh-lane
regression and the super-linear diff step (#390's side finding,
2026-10-25 report §4); upstream per-row damage tracking (#390's open
half — sharpened here by the zero-damage-walk measurement, §4/§5, for
whichever round next talks to it).

## 8. Deviations / scope notes

- The probe was a scratch example, deleted before commit; its logs
  live in `/tmp` (never committed). Everything load-bearing is
  transcribed verbatim in §1.2. Production code was never edited.
- E5(e) ("title flip on off-window task 400") panicked on a probe-bug
  index (N=100 has 100 tasks; the experiment meant N=1 000). It is
  redundant — E1's non-overdue flips at rows 0–14 (on-window in board
  terms) and E5(a) already establish the class — recorded here for
  honesty, not re-run.
- E4's "due task0" line is void (a stamp-over-stamp assignment on an
  already-done task — no model change, correctly 0×0). The due-class
  evidence comes from E5(c)/(d), which toggle properly.
- Region findings are deterministic (reproduced exactly across runs);
  times are single-process interactive references, reproduced once
  (§1.2), never gates.
- The evidence-base correction (§1.6) changes the *interpretation* of
  recorded numbers, not the numbers; both superseded readings (88 % as
  a board-region fact; the draw lanes as board measurements) are
  flagged for #390/#373 readers. No issue body was edited from this
  round — disposition happens at stage 3 / by the user.
- Gates: this branch's only commit is this report (docs-only); fmt
  canary run locally (tree clean); the full §10 gate suite is CI's on
  the PR.

## 9. Acceptance checklist

- [x] §0 probe run in scratch; experiments, verbatim outputs, and a
      candidate verdict recorded (§1); production code untouched;
      nothing probe-shaped committed; the permanent-home proposal is
      issue 1 (§7).
- [x] AGENTS.md, #373, #390, the four evidence reports, the wave plan
      sections, architecture.org and coding_guidelines §2.1 (via
      AGENTS §2) read before writing.
- [x] The code the analysis is about read: board-layout.slint,
      task-card.slint, view_state.rs, models.rs (IdTables, push,
      diff_rows), the bench, plus the pinned slint sources the
      feasibility claims rest on (§2, §5).
- [x] The four Ask-1 trade-offs adjudicated concretely (§3); expected
      win quantified per lane with measured numbers (§4); D9
      interaction stated (§5).
- [x] Exactly one recommendation (§6), with the standing D250 gate
      kept and the analyse-negative triggers named.
- [x] Out-of-scope ideas filed as backlog issues (§7); the
      refresh-lane regression referenced, not absorbed.
- [x] Single standalone `docs(report)` commit on
      `docs/listview-virtualisation-analysis`; no push (handoff:
      stage 3 ships the PR).
