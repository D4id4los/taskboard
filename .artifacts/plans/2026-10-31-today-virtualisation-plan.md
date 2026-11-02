# [Plan] Today virtualisation (#461) — chunk 4.1 plan (stage 1)

- **Date**: 2026-10-31 (dispatch prompt's fixed name; written 2026-11-02)
- **Kind**: stage-1 TDD plan for chunk 4.1 of backlog round 2, branch
  `perf/ui-today-virtualisation` (off `origin/main` = `9f3508ee`).
  Zero production code in this commit; implementation is stage 2's
  (dispatch prompt 4.1.b), review + shipping stage 3's (4.1.c).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md` §1/§6
  chunk 4.1, dispatching GitHub issue #461, which rests on the design
  round `.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`
  (§2 routes, §3c scroll semantics, §4 what moves, §6 recommendation
  step 3), the view-pinned re-measure
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`,
  and the #373 implementation pair
  `.artifacts/plans/2026-10-26-listview-virtualisation-plan.md` +
  `.artifacts/reports/2026-10-26-listview-virtualisation-stage2-report.md`.
- **Gate**: chunk 3.1 (issue #462, branch `fix/kiosk-board-i16-overflow`,
  PR **#582** — the dispatch table's "#462 PR" is the issue number;
  the PR carrying the branch is #582) — merged and verified:
  `gh pr list --state merged` shows #582 merged 2026-11-02T19:25Z, and
  the branch's content commits (`16e58511` plan, `b068a6cc` render pin,
  `65caaef2` changelog, `a41edb93` upstream report) are ancestors of
  `origin/main` (rebase-rewritten SHAs). The render lane is serially
  clear. Doctor: exit 0 (225 GiB free, floor 25 GiB); worktree created
  with `-b` off `origin/main`.

## 0. The premise correction (read this first, stage 2)

The dispatch prompt (and #461's body) describe the Today section rows
as an **uncapped, unvirtualised `for`** at `today-view.slint:54` whose
draw cost is list-extent-scale. That premise is **superseded**: the
#373 virtualisation chunk (PR #511, merged 2026-10-26; commit
`d57b95be` "the today sections and stack detail through the ListView")
already mounted all four Today sections on the in-house `ListView`,
landed the first anchor-on-removal pin, re-measured the lanes, and the
dispatch plan's own Covers line reflects it ("via the **(landed)**
#373 ListView mechanism"). The evidence is on the record:

- The sections' row loops run inside `row-viewport := ListView`
  instances today (`today-view.slint:165-242`), with the
  content-sized-below-a-window-derived-bound viewport shape (the
  stage-2 report's §2.2 decoupling).
- The decoupling is **measured**: `draw-full/today` 19.9 → 1.44 ms and
  `draw-partial/today` 903 → 5.75 ms at N = 10 000; every draw lane
  N-independent (stage-2 report §4, before/after table).
- The first anchor pin
  `a_removal_above_the_today_viewport_leaves_the_rows_resting`
  (virtualisation.rs:537) landed with the chunk and is commented on
  #461 (2026-10-26).

**Consequence**: chunk 4.1's honest remaining scope is the two
missing anchor-semantics pins (#461's acceptance names the catalogue;
one of three cases is pinned), the recorded decisions this plan
makes, and the fresh bench evidence on this branch. **It is not** a
re-implementation of the mechanism: stage 2 must not touch the Today
markup or `push_today` semantics. "The per-section mechanism" of
dispatch prompt 4.1.b resolves to **verification of the landed
mechanism** (§5 M3), not new markup — a deviation from the prompt's
literal sequence, recorded here as the reason (re-implementing a
landed, pinned, N-independent mechanism would regress it; AGENTS §5:
a failing new test may be a production bug, but a passing one is not
an invitation to rewrite the production side).

## 1. Mission & binding constraints

Pin the Today view's anchor-on-removal semantics completely (the
three-case catalogue of #461: removal above the viewport, insertion
above the viewport, removal of the anchored row — the first landed,
two land here), record what the bench lanes measure after
virtualisation and keep the list-extent cost visible per the design
round's contract (§3d item 3), and leave every today journey, walk
pin, and golden green **unmodified**.

Binding, verbatim force:

1. **The today journeys pass unmodified** (dispatch 4.1.b: "a silent
   focus/walk change is a deviation to revert"). This chunk's
   production-diff budget is **empty** — expected `git diff
   origin/main` is tests + docs only.
2. **Anchor semantics**: "removal reflow keeps the user's place per
   the pins (a jump is a bug in this chunk, not a slint quirk to
   accept)". The pins encode the **pinned actual** repeater behavior
   (stage-3 4.1.c's trap: "a pin that encodes the DESIRED semantics
   rather than the pinned actual ones will fail in the field"). The
   semantics statement of §3 D-2 is the contract the two new pins
   verify on the real runtime; if a pin comes out red, that is a
   finding against the semantics statement (or a real slint
   regression), not something to weaken (AGENTS §5).
3. **Bench contract** (design round §3d): a reference that keeps
   list-extent cost visible survives. This plan discharges it with
   the structural engagement probes + the recorded baselines and
   rejects a live unvirtualised lane — the recorded decision D-4,
   with reasons.
4. AGENTS §5 across the new tests: no sleeps, no text assertions, no
   quality ceilings (the engagement probes' `×4` ratios are failure
   detectors of the eager shape, never success ceilings — strictly
   better virtualisation passes), deterministic by construction, one
   behavior one test at the lowest layer, no "today's behavior" pins.
5. No crate-boundary change; no diff/push semantic change; no
   IdTables shape change; `taskboard-ui-slint` stays AGPL-3.0-only
   headered, and the crate keeps its `forbid(unsafe_code)`-exempt
   status (no new production .rs files at all).

## 2. Verified baseline — numbered facts

All verified on this branch's base (`origin/main` = `9f3508ee`);
file:line anchors are that tree's.

**F1 — the landed component.** `ui/widgets/listview.slint`: the
in-house `ListView inherits Flickable` carries the compiler's nominal
five-property contract (name + `content-y/height/width`,
`visible-height/width` — `i-slint-compiler-1.18.1/object_tree.rs:2891-2905`;
a rename silently renders eager, which the engagement probes fail on
purpose), the focus seek (`request-row/-nonce/-total-rows`,
`scroll-row-into-view` top-anchoring on the runtime's cached average),
and the round-3 paging seam (`page-request/-nonce`, `page`). The
Today view binds both seams per section (`today-view.slint:187-202`:
`FocusScrollHint.target-a == sindex` gates the seek; `ScrollPageHint`
family 2 pages the focused section).

**F2 — the landed Today viewport shape.** Each section body is
inlined into the section loop's delegate (`today-view.slint:9-14,130`):
a model may not flow through a child component's `in property` on the
pinned slint (stage-2 report §2.3) — `TodaySection` was dissolved for
exactly this reason. The viewport is `row-viewport-clip := Rectangle`
(clip host, content-sized height **assigned** by `changed
content-height` / `changed watch-viewport-height` handlers — a
declarative `min(content-height, cap)` is a statically detected
binding loop) over `row-viewport := ListView` whose `visible-height`
is the window-derived bound. A fitting section keeps up to a
bound-scale window live (bounded, never N-scale — the markup
documents this consequence). All four sections mount the mechanism;
empty sections collapse (`visible: section.count > 0`).

**F3 — the landed engagement + anchor pins** (`tests/virtualisation.rs`):
`the_today_sections_virtualise` (:410) mounts `build_state(600)`
(~200 overdue rows) and asserts viewport-scale live rows (`rows.len()
* 4 < 600`, a failure detector, not a ceiling);
`a_removal_above_the_today_viewport_leaves_the_rows_resting` (:537)
drives 30 `NavNextItem` strokes (the walk enters at row 0, so
position 29 = the 30th overdue row = `bench_label(87)` — overdue is
`(order, id)`-ordered over orders {0,3,6,…}, `days.rs:182-186`), then
flips overdue row 0's `done` through the state store + `refresh()`,
and asserts the focused row's `absolute_position().y` unchanged.

**F4 — push_today and its tables** (`models.rs:1670-1755`): builds
the **full** filtered row list per section (no cap anywhere), diffs
rows into the **persistent** per-section handles
(`models.today_sections[index]`, the D9 discipline — never a
`ModelRc` swap, so the virtual repeater never takes its `reset()`
path), fills `tables.today_tasks` (the flattened display order) and
`tables.today_section_rows` (prefix sums), from which the additive
reverse helpers `today_row(section, row)` and `today_position(task)`
(`models.rs:120-152`) resolve the UI's `(sindex, rindex)` seams and
the focus-hint writer. The kiosk board filter is inside the same loop
(`:1707-1714`).

**F5 — the walk/focus machinery (state-side, virtualisation-blind).**
The keyboard walk is `step_focus` → `step_flat_domain` over
`today_domain` (four sections concatenated in display order,
`view_state.rs`); focus is a `TaskId` in `ViewState`; emphasis rides
the models. After a focus move the adapter resolves the new focus
through `IdTables::today_position` and edge-triggers the
`FocusScrollHint` global (`adapter.rs:1375-1400`), which the matching
section's ListView consumes (F1). Paging rides `ScrollPageHint`
family 2. The walk journey `the_today_view_walks_completes_and_recommits`
(`journeys.rs:6716`) pins display order, emphasis, selection-follows,
`SetTaskDone` dispatch, done-section placement, and the re-commit
verb — all green under the virtualised sections since #511.

**F6 — the done-flip journey is the anchor case.** A done flip
removes the task from Overdue (`is_open()` goes false — a
filtered-list removal, `days.rs:183` / `reminders.rs` predicate) and
— when the `done` stamp falls inside today's window — inserts it into
Done. The bench's `PARTIAL_STAMP_SECS` is a fixed 2024 stamp
(`refresh_draw.rs:97`), so the lanes' flip is a **pure removal**; the
journeys' `SetTaskDone` path pins the dispatch, not the geometry. The
insertion class (§3 D-2 case 2) is therefore exercised in tests by a
direct state mutation + `refresh()` — the same lever the landed
removal pin uses — because no desktop verb inserts a row above a
scrolled viewport deterministically (captures land at the list head
of *Overdue*, which is above the viewport when deep — a synthetic
earlier-overdue insert is exactly that shape, made deterministic).

**F7 — the bench's view-pinned lanes** (`benches/refresh_draw.rs`):
`VIEW_PINS = [("today", false), ("board", true)]` (:92) via the
mount's kiosk lever; lanes `draw-full/{today,board}/{n}` and
`draw-partial/{today,board}/{n}` over `N_SIZES = [100, 1_000, 10_000]`;
`refresh/{n}` unpinned (the pass derives every view's models). The
partial lanes rotate over the pinned view's rendered head rows
(`support::today_flip_targets` — the Overdue head six; the board's
flip is an in-place card change — the two pins deliberately separate
the damage shapes), with the visible-flip preamble guard (a zero
region panics: the 0×0 anomaly's lesson) and the max/last region
carrier. Post-#511 reference numbers (stage-2 report §4): today
1.42/1.43/1.44 ms full, 4.81/4.87/5.75 ms partial at the three N;
board 2.78/2.74/2.78 full, 2.99/4.78/1.76 partial. Pre-#511
baselines (re-measure report §2): today 1.37/2.08/19.9 ms full,
8.95/88.0/903 ms partial — the removal-reflow evidence #461 cites.

**F8 — the goldens.** The today golden matrix
(`today-{0.5,3.0}-{600,1000,1500}.png`, `golden_visuals.rs:193-239`,
fixture `today_overdue_state`) was regenerated at #511
(`d57b95be` touched all six) and pins the virtualised section
geometry, including the corrected card heights the goldens caught
(stage-2 report §3). Regen protocol: `TASKBOARD_REGEN_GOLDENS=1`,
PNG diff is the review artefact.

**F9 — the repeater's anchor mechanics** (the #373 plan's F2, verified
against `i-slint-core-1.18.1/model/repeater.rs:531-616`):
`row_changed` off-viewport is a no-op; `row_added`/`row_removed`
shift the repeater's offset keeping a row-anchored position — the
live instances keep their bound model rows and screen positions when
the change is **above** the viewport window; a change **within** the
window re-binds the slots after the change point (rows below shift by
the changed row's extent); `content-y` is ≤ 0 and the runtime
re-anchors it itself after offset shifts; `reset()` re-instantiates
the whole viewport (unreachable here — persistent handles, F4).

**F10 — the i16 ceiling is not a Today risk.** The eager Today view
rendered ~N/3 overdue rows at ~183k px of list depth without
panicking (re-measure §5: the per-path difference vs the unclipped
board columns is upstream's to explain); the virtualised Today
instantiates a viewport-scale window, so its geometry is bounded by
construction. `a_board_beyond_the_i16_pixel_ceiling_renders` (PR
#582) pins the board half of this; no Today-side i16 pin is needed.

## 3. The decisions

### D-1 — per-section virtualisation: uniform `ListView` on all four sections, ratified

The landed shape mounts the mechanism on every section (Overdue,
Scheduled, Due today, Done) through the single section-loop delegate
(F2). Ratified per section, with the counterfactual recorded:

- **Overdue** — the N-scaling offender (N/3 rows in the bench
  fixture; unbounded by neglect). Virtualisation is load-bearing.
- **Done** — the section that grows on every productive flip (the
  done-today membership of F6). Any given day is small, but the bound
  is user-behavior, not design; virtualisation costs nothing and
  removes the ceiling. Uniformity also keeps the done-flip's
  removal+insertion pair mechanically symmetric across sections.
- **Scheduled / Due today** — small in practice, but a plain `for`
  here would (a) fork the section delegate into per-section markup
  branches (the loop is one delegate; a conditional eager path inside
  it is exactly the speculative generality the code-quality mandate
  rejects), (b) save nothing measurable — an empty section collapses
  and a fitting section already keeps only a bound-scale window live
  (F2), and (c) break the uniform engagement pin
  (`the_today_sections_virtualise` asserts the mechanism per mounted
  section). Rejected: per-section demotion.

**Decision**: keep the uniform mechanism; no markup change.

### D-2 — anchor-on-removal semantics: the pinned contract and the three-case catalogue

**The semantics (what the user's scroll position means).** The
virtual repeater is **row-anchored, never pixel-anchored** (F9):

1. A membership change **above** the viewport (removal *or*
   insertion) leaves every on-screen row at its exact position: the
   repeater shifts its internal offset; `content-y` is not scrolled.
   The user's place does not move. (A pixel-anchored repeater would
   shift the rows by the changed row's extent and fail.)
2. A removal **within** the viewport (the anchored row is the walked-to
   row — the seek top-anchors it, so focus position = window top)
   reflows only the rows **below** it: each moves up by exactly the
   removed row's extent; rows above the removed row (none visible, or
   off-window) do not move; the page does not scroll. The click that
   flipped the row is where the reflow starts — the list closes up
   under the cursor, it does not jump.
3. There is no scroll-position reset on any refresh (persistent
   handles, F4 — the repeater never `reset()`s).

The user-facing reading, for the record: a done flip anywhere in the
Overdue list keeps the viewport where the user put it; only the rows
visibly below a removed row close up, by exactly that row's height.

**The catalogue** (one behavior, one pin, all through the mounted
Today view at `build_state(600)` — the landed pin's fixture and
mount, 1000×900 window):

| # | Case | State | Landed? |
|---|---|---|---|
| 1 | Removal above the viewport (flip overdue row 0 after a 30-deep walk) | rows resting (focused `absolute_position().y` unchanged) | ✓ `a_removal_above_the_today_viewport_leaves_the_rows_resting` — kept **untouched** |
| 2 | Insertion above the viewport | rows resting + the new head row **not** instantiated | **new** (§4 T-1) |
| 3 | Removal of the anchored row | rows below close up by the removed row's extent; the page does not scroll | **new** (§4 T-2) |

Case 2's assertions: after inserting a synthetic task whose duedate
is strictly older than the fixture's oldest overdue (it lands at
Overdue index 0 — above the viewport), (a) the focused
`bench_label(87)`'s `absolute_position().y` is unchanged, (b) the new
row's label is **not** findable (index 0 stays outside the live
window — a regression that re-seeks or grows the window fails), (c)
the section header's position is unchanged (no page scroll).

Case 3's assertions: after flipping the focused row's own `done`
(position 29 = `bench_label(87)`, which is also the window's top
live row), (a) the row that was directly below it (`bench_label(90)`,
position 30, previously the window's second live row) now sits at
exactly the removed row's old y — the reflow boundary is the removed
row, and the extent is exact by F9's actual-height layout walk; (b)
the removed label is gone; (c) the section header's position is
unchanged (the page did not scroll). Focus follow-through after the
removal (the done task leaves `today_tasks`; reconcile keeps focus
honest by id) is **deliberately not asserted here** — the walk
journey and the state-side focus pins own that behavior; this pin is
the geometry contract.

Both new pins are pinned-actual pins: they encode F9's offset-shift
behavior as observed through the driver's element seams
(`find_by_label`, `absolute_position`) — the same instrument class as
the landed pin — never slint internals, never pixel numbers.

### D-3 — the walk/focus story under virtualisation: landed precedents, preserved

The board round's adjudications carry over verbatim (design round
§3a/§3b): the walk is state-side (`TaskId` domains — viewport
nonexistence cannot break it); the focus-into-view seam is the
`FocusScrollHint` nonce seek (F1), resolved through `IdTables`
reverse helpers (F4) with global model indices (no viewport-relative
index anywhere); UI row indices are model indices where the tables
are declared. The Today-specific wiring (seek gated per `sindex`,
paging on family 2) is exercised by every catalogue pin's walk setup
(30 strokes ride the seek; a lost hint binding fails the pins loudly
at "the walked-to card is live"). **Decision**: no new seek/jump pin
for Today — the seek *mechanism* is pinned at the probe level
(`the_seek_makes_a_deep_row_live_and_visible`), the board jump pin
covers full-visibility, and a Today duplicate would re-test the same
behavior through a second mount (AGENTS §5). The journeys and walk
pins stay byte-identical.

### D-4 — the bench lanes: what N measures now, and the kept reference

**What N measures after virtualisation.** N still seeds the fixture
(`build_state(n)`) and still drives the model-side extent: the
refresh pass derives and diffs all N rows per section (`push_today`
is uncapped, D-5) regardless of the viewport. The **draw** lanes
raster a viewport-scale scene at every N — their N-independence is
precisely the chunk's acceptance ("today draw cost decouples from
list extent"), already measured (stage-2 report §4) and re-run on
this branch as stage-2 evidence (§5 M3). The partial lane's flip
remains the removal class (F6's pure-removal stamp), and its region
carrier keeps the re-measure's corrected semantics: max = honest
worst case, last iteration for warm-up separation, per-iteration
readings meaningful only because the visible-flip guard underwrites
them.

**The kept reference (design round §3d item 3) — decision: no live
unvirtualised lane; the reference is structural probes + recorded
baselines.** The contract's intent is that list-extent cost stays
*visible* — not silently lost. It is discharged by:

1. **The structural engagement probes** (`the_today_sections_virtualise`,
   plus the M1 probe pair and the board/stack/compact pins): any
   silent de-virtualisation — the ListView name/property contract
   break (F1), a model routed through a child property (F2), a
   per-refresh `ModelRc` swap (F4) — fails a named test loudly.
   These carry the *detection* duty a live eager lane would carry,
   at unit-test cost and without a parallel markup path.
2. **The recorded baselines** (F7's before/after tables — the
   re-measure §2 eager numbers and the stage-2 report §4 decoupled
   numbers, both PR-visible artefacts): they carry the *magnitude*
   duty — what the eager class costs at list scale (19.9 ms full /
   903 ms partial at N = 10 000) versus viewport scale (1.44 /
   5.75 ms).

A live unvirtualised Today lane is **rejected**, with reasons: (i) it
requires keeping a parallel eager markup path (an uncapped plain-`for`
variant of the section body) that ships in the binary for no user —
dead code under the code-quality mandate; (ii) a "size-scaled
viewport" variant (a window tall enough to hold all N rows, so the
virtualised viewport equals the eager extent) is physically
impossible at the contract's N = 10 000 — 10 000 rows × ~52–77 px ≈
0.5–0.8 M px of raster height, past the software renderer's i16
space (#462's recorded ceiling) and a multi-GiB framebuffer — and at
small N it degenerates to the ordinary draw lanes; (iii) the board
half of the same contract was resolved identically when #511 landed
(M4: lanes un-skipped, names/shapes/guards unchanged, no eager lane
kept) and re-confirmed by #582's render pin — Today follows the
precedent, and consistency across the two surfaces is itself
documentation.

**Lanes stage 2 re-runs (the evidence protocol)**: `draw-full/today`
and `draw-partial/today` at all three N, plus `refresh/{n}` for
context — on a quiet box, times recorded as dev-machine references,
never gates (the house rule; regions are deterministic). The board
lanes are **not** re-run as a requirement: this chunk does not touch
the board mechanism, and #511/#582's numbers stand; a stage-2 run may
include them for free context, not as evidence.

### D-5 — `push_today` stays uncapped

Recorded because the dispatch prompt names it ("what it caps today:
nothing") and the answer is a decision, not an omission: (i)
**correctness** — `tables.today_tasks` is the walk domain; a cap
would hide rows from the keyboard walk and break the
complete-and-recommit journey's contract (the walk reaches every
task); (ii) **economics** — with the sections virtualised, render
cost is viewport-scale; the residual N-cost is the derive+diff per
refresh, which is the refresh lane's own recorded finding (#390,
referenced, not absorbed); (iii) **product** — a cap is a
user-visible policy (which rows drop? what "show more" surface?)
with no mandate anywhere in the round. Rejected: any cap.

### D-6 — the goldens: untouched, and regen-forbidden

The chunk's production-diff budget is empty (§0), so the six today
goldens (F8) must pass **byte-identical**. A golden diff during
stage 2 is a bug in the chunk, not something to regenerate — record
it as a deviation and stop. (The goldens already pin the virtualised
geometry; nothing in this chunk changes a rendered pixel.)

### D-7 — the CHANGELOG: no entry

Dispatch prompt 4.1.b's boilerplate says "one performance entry";
resolved against it, with reason: the user-visible performance entry
for section virtualisation already landed with #511 (stage-2 report
§7: "CHANGELOG: added (virtualised task lists, user-visible)"), and
this chunk's honest content (pins + evidence + decisions) changes no
user-observable behavior — AGENTS' shared model gates CHANGELOG
entries on user-observability. If stage 2's invert-checks were to
surface a real bug requiring a production fix (§7 R1), that fix gets
the entry instead, at that commit.

## 4. TDD catalogue — stage 2's commits, in order

Every commit standalone-green (rebase-merge hygiene). The "red" step
for semantic pins of landed behavior is the **invert-check**
protocol, not a production fix: after each pin is green, verify it
fails under the broken shape, and record the transcript in the
stage-2 report (the #373 D-3 and #582 pin precedents). Two
invert-checks per pin, each discriminating a different failure:

- *de-virtualisation*: a scratch consistent rename
  `ListView`→`ListViev` in a scratch checkout (never committed) —
  the seek dies with the contract, the pins fail at "the walked-to
  card is live" (they cannot even set up), which is the
  probe-carried detection of D-4 item 1 demonstrated on the new pins.
- *pixel-anchoring*: for the removal pin, a scratch change that
  re-solves positions from pixel offsets is impractical; instead
  demonstrate the pin's discriminating power against the **eager
  shape** — run the same pin body against a scratch Today variant
  with the section's `ListView` swapped for a plain clipped `for` in
  the scratch checkout: removal/insertion above the viewport now
  shifts every row below (the layout re-solves from the top), and
  each pin fails at its resting assertion. Record which assertion
  failed and with what delta.

- **T-1 (commit 1, `test(ui-slint): the today anchor holds under an
  insertion above the viewport`)**: the D-2 case-2 pin. Fixture
  lever: clone the shape of overdue row 0's task (its stack id, live,
  open, no scheduled) with a fresh `TaskId`, `order = -1` (i64; it
  lands at Overdue index 0 by the `(order, id)` order — the
  fixture's orders are `0..n-1`), and a duedate exactly one second
  older than row 0's own stamp (clone it and subtract
  `chrono::TimeDelta::seconds(1)` — strictly older than every
  fixture stamp by construction, never wall-clock); insert into
  `next.tasks`, store, `refresh()`, advance the mock clock.
  Assertions per D-2 case 2 (a)(b)(c).
- **T-2 (commit 2, `test(ui-slint): removing the anchored today row
  closes the list up without scrolling`)**: the D-2 case-3 pin. Walk
  30 strokes (same setup as the landed pin — reuse its comment's
  position arithmetic: position 29 = `bench_label(87)`), record the
  pre-removal y of `bench_label(90)` and the header, flip
  `bench_label(87)`'s `done` via the state store, assert per D-2
  case 3 (a)(b)(c). Comment must carry the F9 semantics citation
  (repeater.rs offset shift; actual-height walk) so the pin's
  expectation is legible.
- **T-3 (commit 3, `docs(bench): the today lanes' N semantics after
  virtualisation`)**: the bench doc block
  (`refresh_draw.rs:17-33`'s lane narrative) gains the explicit
  post-virtualisation reading — what N measures now (D-4's first
  paragraph), and a pointer to the kept reference (the probes + the
  recorded baselines). Comment-only; no lane shape change (D-4's
  decision needs no code). If the existing block already states a
  clause verbatim, extend, don't duplicate.
- **M3 evidence (stage-2 report, not a commit)**: the lane re-run per
  D-4 (today full/partial ×3 N + refresh), the invert-check
  transcripts, the journeys/goldens green confirmation, per-gate
  results, and the deviations list (expected: "none beyond the
  stage-1 premise correction").
- **M4 gates**: the full AGENTS §10 suite green
  (`scripts/run-quality-gates.sh` from the worktree — the flock
  wrapper; the coverage three-command sequence with quarantine +
  trailing `rm`).

No stage-2 commit touches `today-view.slint`, `listview.slint`,
`models.rs`, `adapter.rs`, or `view_state.rs`. The branch's final
diff versus `origin/main` is: two test functions + their comments in
`tests/virtualisation.rs`, the bench doc block, this plan, and the
stage-2 report — **≈ 200–300 lines, tests + docs only**.

## 5. Milestones (plan structure, not commit boundaries)

- **M1 — the anchor catalogue complete** (T-1, T-2): the three-case
  semantics of D-2 fully pinned; the landed case-1 pin untouched.
- **M2 — the bench story on the record** (T-3 + the lane re-run):
  N's post-virtualisation meaning documented at the lanes; fresh
  decoupling evidence recorded on this branch.
- **M3 — the green sweep**: journeys (incl.
  `the_today_view_walks_completes_and_recommits`, the kiosk-rotation
  and today-mode goldens), model-identity and repetition probes, the
  goldens byte-identical (D-6), full gate suite (M4).

## 6. Acceptance checklist (maps 1:1 to the dispatch plan's)

- [ ] The anchor-on-removal pins: all three catalogue cases present,
      pinned-actual, invert-checked with recorded transcripts.
- [ ] Today draw cost decoupled from list extent: fresh lane run
      recorded (viewport-bounded at N = 10 000), the kept-reference
      decision documented at the lanes (D-4).
- [ ] All today journeys/goldens green **unmodified**; zero
      production diffs.
- [ ] Every open question resolved with a recorded decision (D-1 …
      D-7; §7's Q-list).
- [ ] Stage 3 closes #461 with a self-contained comment (the pins,
      the lane evidence, the D-4 disposition — the design round's
      §6 step-3 recommendation discharged).

## 7. Risks and resolved questions

- **R1 — a new pin comes out red** (the semantics statement wrong
  about the actual repeater, or a real regression since #511). Do
  not weaken (AGENTS §5): if the runtime genuinely re-anchors
  visibly, the pin stays red, the chunk reports the finding with the
  transcript, and the semantics statement gets the correction — the
  user-facing consequence ("a done flip can jump the list") is then
  a bug to file, not a quirk to accept (dispatch 4.1.b, verbatim).
- **R2 — walk-position arithmetic drift**: the pins hardcode
  `bench_label(87)`/`bench_label(90)` with the `(order, id)` × 3
  derivation in comments (F3); any fixture/ordering change fails
  them loudly at setup, which is the correct failure.
- **R3 — the insertion pin's synthetic task** must not perturb other
  sections (scheduled/due-today membership): the lever clones an
  overdue row's shape (duedate set, no scheduled) with an *older*
  duedate, so it lands in Overdue only; assert nothing about other
  sections.
- **R4 — determinism**: all fixtures are 2024-stamped; the pins
  never read wall time and never sleep; the mock clock advances
  synchronously (the landed pin's discipline).
- **R5 — bench times on a busy box** are references, never gates
  (house rule); the lane run goes through
  `scripts/run-quality-gates.sh` if heavy, or standalone `cargo
  bench` outside the gates window — recorded either way.
- **Q: timers/habits sections?** Out of scope — they are separate
  compositions whose rows are capped by their domains (few live
  timers/habits), the #373 non-goal's "row-capped, not N-task"
  class; no ListView there, ratified.
- **Q: compact/kiosk Today leg?** None exists — kiosk mounts the
  pinned board (`adapter.rs` seed; the today kiosk filter is inert
  on the single-board fixture); the section mechanism has one leg.
- **Q: does `the_today_sections_virtualise` need a deep-Done
  variant?** No — engagement is per-mechanism, not per-section
  (D-1); the catalogue supplies depth through Overdue, and Done's
  growth class shares the mechanism.
- **Q: should stage 2 also pin `content-y` stability directly?** No
  — the driver's element seams are the sanctioned instrument; the
  header-position + row-position joint assertions pin the same
  property structurally (D-2), without poking slint internals.

## 8. Acceptance per the stage-1 prompt (self-check)

- Per-section decisions with reasons: D-1.
- Anchor-on-removal semantics + explicit repeater-offset pinning +
  the three-case catalogue: D-2, §4.
- Walk/focus story under virtualisation, board-round precedents: D-3.
- Bench lanes update, what N measures, kept/scaled reference per the
  design report's contract: D-4.
- The goldens: D-6.
- Every open question resolved with a recorded decision: D-1…D-7,
  §7; the stage-1 premise correction itself is §0 with its reason.
