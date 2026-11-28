# [Plan] UI clip fixes — chunk plan (#1491, #1504, #1461, #1464)

- **Date**: 2026-11-28
- **Kind**: chunk plan — one branch (`fix/ui-clip-fix-waves`), one PR,
  three stages (this plan → implementation → review) for four UI-layout
  backlog issues: the kiosk-scale timers-editor overflow (#1491), the
  task-detail bottom-edge clip at 1.5×/900 px (#1504), the empty-board
  CTA full-bleed (#1461 — **found stale**), and the `visible:false`
  slot-leak sweep (#1464). No production code in this commit.
- **Origin**: the four issues (bodies normative); the clip-sentinel law
  plan `.artifacts/plans/2026-11-25-clip-sentinel-law-plan.md` (§3 fix
  shapes, §1 the C1 rubric — the classifier's `real-clip` grades are
  consumed, not re-derived); the widget-framework base plan
  `.artifacts/plans/2026-11-18-widget-framework-base-plan.md` §5 (the
  geometry-probe assertion catalogue — the test spine pattern); the
  wrap + click-out plans
  `.artifacts/plans/2026-11-27-focus-remount-wrap-plan.md` and
  `.artifacts/plans/2026-11-27-board-click-out-plan.md` (the board
  invariants this chunk must not regress); `docs/ux_planning.org`
  (the trigger rule) and `docs/testing_strategy.org` §6.1–6.2 (clock
  discipline for goldens).
- **Status**: READY — no gates, no dependencies; dispatchable now.
  Not dispatched here: the #1365 board-columns overflow regime
  (accepted legacy pending #1490's wrap), #1416's shadow-blur
  disposition (checked, not re-litigated), #1430's broader habit-editor
  density work (only the dead-slot mechanism is in scope).

## 1. Recorded reality (file:line at origin/main `3fc4764b`)

### #1491 — the timers editor row overflows at kiosk scale (p2)

- The row: `crates/taskboard-ui-slint/ui/compositions/timers.slint`
  ~line 388-425 — inside `if (!root.absence-hint): VerticalLayout`, a
  `HorizontalLayout { spacing: space-2 }` mounting `label-field :=
  TextField` (`horizontal-stretch: 1`), `when-field := TextField`
  (`horizontal-stretch: 1`) **and** the insistence
  `SegmentedControl { options: ["Normal", "Alarm"] }` — all three on
  one row.
- The measured red (the findings report,
  `.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md`,
  §`notification-3.0-1000` / §`today-3.0-1000`, classifier output is
  the oracle): at `base-scale = 3.0` (window 1000 physical ⇒ ~333
  logical px of viewport) the row's min-content width — two text
  fields' minimums plus the segmented control's two labelled halves —
  exceeds the viewport, so the row paints past the right canvas edge:
  `x=999 y=209 h=133` (extent 133 px, real-clip fix-now) on both
  goldens, plus `x=999 y=429 h=106` (today) — the second flush band —
  and `x=0 y=209 h=206` (notification) — the panel's ring/shadow flush
  at the **left** edge. The issue's geometry claim reproduces: this is
  a **min-content overflow**, not a full-bleed composition — no inset
  can heal it (a bound still paints past the frame; the issue says so
  and the mechanism agrees).
- The surface is the today page's timers block
  (`today-view.slint:365 TimersBlock inherits TimersSection`), mounted
  from both the wide arm (`:652`) and the narrow arm (`:805`) — the
  same composition, so one fix serves both arms. The notification
  surface's right-edge run shares the signature class but the
  notification panel's left-edge 206 px run is the #1416
  shadow-blur/ring disposition's territory — stage 2 checks #1416's
  recorded disposition before touching the notification panel; if the
  disposition already covers the left-edge half, only the timers row
  changes here.

### #1504 — the 1.5×/900 px task-detail fit clips the description viewport (p3)

- The chain: #1435 (round 8 B4) added the **vertical legs** of the
  detail-editor severing inset (`task-view.slint:512-528` —
  `padding-top/bottom: Theme.space-2` beside the L1 horizontal legs;
  the comment records the vertical legs join at #1435's residual, the
  AlarmWindow recipe: rows flushed the window's top/bottom edges too).
  The vertical legs consume `2 × space-2` of the window's height, and
  the detail root has **no outer scroll** — the flexible
  `description-section := VerticalLayout` (`task-view.slint:1484`,
  `vertical-stretch: 1`) is the only absorber. At the
  `task-view-1.5-900` fit the absorber's leftover height shrinks past
  the description viewport's needs, and the viewport's clip boundary
  lands at the window's bottom edge — the two C1 real-clip signatures
  (44 px / 106 px runs) the round-8 B4 amendment records in
  `scripts/layout_review.py`'s residual explanation
  (`layout_review.py:1236`). Content is scrollable *inside* the
  viewport (`widgets/multi-line-editor.slint` — the Flickable viewport
  law, `min-height: 3 * line-height + 2 * space-2` at `:129`), so this
  is scroll-viewport geometry, not a severed widget — but the law's
  classification keeps it `real-clip`, and "the content scrolls" is
  today's behaviour, not a contract (AGENTS §5: no pins of today's
  behaviour).
- **Report drift noted**: the committed findings report's residual
  header still reads "eight remaining" (the round-8 L1 text) while the
  script's `layout_review.py` explanation (amended round 8 B4) reads
  "ten remaining" including the two `task-view-1.5-900` rows — the
  report file on main predates the B4 regeneration. Stage 2's
  determinism law (clip-sentinel plan §3): the regenerated report
  lands in the same change as any fix, healing this drift as a side
  effect.
- The probe twins already on main:
  `tests/probes/editor_inset_probe_tests.rs` (pins the inset's
  horizontal off-the-canvas law — must stay green) and
  `tests/probes/editor_viewport_probe_tests.rs` (the MultiLineEditor
  viewport contract — must stay green).

### #1461 — the empty-board CTAs render full-bleed (p3) — **STALE**

- **Already fixed on main.** Commit `a92b6ad1` ("fix(ui): the
  detail-editor severing inset + the empty-board stub bound; the
  clip-focus absorb seam + refocus nonce") added exactly the issue's
  proposed direction: the empty leg's CTAs
  (`ui/compositions/board-layout.slint:277-300`) sit in
  `HorizontalLayout { alignment: center }` containers and each
  `TextButton` carries `max-width: Theme.dialog-min-width`
  (`theme.slint:177`, `280px * base-scale`), with the recorded
  severing-bound comment naming the B-empty signature verbatim.
- The typed pin exists: `tests/probes/board_empty_stub_probe_tests.rs`
  — `the_empty_board_ctas_stay_off_the_canvas_edges` asserts
  `pos.x > 0` and `pos.x + width < window-width` for both CTAs, green
  on main. The issue's measured evidence (`board-empty-1.0-1000.png`,
  two 28 px-tall bands bordered at x=0/x=999) describes the pre-fix
  golden.
- **Action**: no code. Stage 2 comments on the issue with the commit
  and probe evidence; the dispatcher closes it at merge (AGENTS §7:
  closure is the archive). If the issue re-reopens with a fresh
  measurement, that is a new finding, not this chunk.

### #1464 — `visible:false` slot leaks at six named sites (p3)

The §5 law (`docs/coding_guidelines.org` §5, and the #1047/#1419
footgun record): `visible: false` keeps the layout slot; when the slot
is not wanted, gate with an `if` mount. Site-by-site at `3fc4764b`
(the issue's line numbers have drifted; the sites are all still there —
none already fixed):

1. **today-view.slint:174** — `component TodaySectionBlock` (the
   delegate used by both arms' `for section[sindex] in …` repeaters at
   `:718`/`:899`): `visible: sec.count > 0` — an empty section keeps
   its header+viewport slot (~21 px dead band per empty section; the
   D7 comment even *says* "empty sections collapse" while the
   mechanism only hides). The narrow arm's own comment
   (`today-view.slint:797`) records that a per-delegate
   `visible: false` collapse "keeps the twins mounted" — the same law,
   already learned one block below. **Fix shape**: gate the delegate's
   *contents* with `if (sec.count > 0)` inside the component (a
   repeater delegate cannot carry a bare conditional at its root per
   the recorded slint constraint — the gate goes one level down, on the
   inner block, so the empty delegate contributes zero height).
2. **task-view.slint** — the task-only rows' `visible:
   !root.task.is-habit` mounts: the header row (`:809`), the metadata
   row (`:945`), the priority/importance row (`:1044`), the effort row
   (`:1140`), the tracked row (`:1271`), the rule list (`:1336`), the
   description section (`:1485`) — plus the habit block's nudge
   preview (`:782`, `visible: root.habit.nudge-preview.visible`, the
   "hidden while the draft refuses" neutral state). On the habit
   editor variant this is the compounding dead column above the save
   row — the "wasted vertical space" #1430 names. **Fix shape**:
   convert each `visible: !root.task.is-habit` row to an `if
   (!root.task.is-habit):` mount (the file already uses this idiom
   everywhere else — `:865`, `:877`, `:1242`, `:1291`), and the nudge
   preview to `if (root.habit.nudge-preview.visible):`.
   - **Mechanical consequence, recorded**: `effort-row` and
     `description-section` are referenced by the probe-geometry out
     properties (`:438-442`, `effort-rect-*` / `description-rect-*`);
     unmounted elements read as zero — acceptable (the geometry facts
     are consumed on the task variant), but the probe/tests reading
     them must run the task variant, and any pin asserting their
     existence on the habit variant is wrong today.
   - **Layout consequence, recorded**: `description-section` is
     `vertical-stretch: 1` — today its *hidden* slot absorbs the
     habit variant's leftover height. After the `if` conversion the
     habit editor's own rows pin to the top with the leftover at the
     bottom; that reclaimed band **is** #1430's ask. The habit-variant
     goldens re-baseline (determinism law) and the shift is the
     intended user-visible win, ruled in R-2 below.
3. **action-menu.slint:162** — the quick-bar delegate
   `Rectangle { visible: bar-entry.visible; … }` inside the
   `quick-bar-row` HorizontalLayout: a withheld row keeps its 32 px
   slot + spacing. The C-4 comment (`:19-20`) says "the bar renders
   only the shown half" — the mechanism contradicts the stated
   intent. **Fix shape** (per the issue's own alternative): the
   position-stability reading is *not* claimed anywhere — convert to
   `if (bar-entry.visible):` inside the delegate body (a `for`
   delegate's children may carry conditionals; the row element itself
   stays the repeater's child, so the model contract — resolve by
   index — is untouched), OR record the reserved-slot intent in place.
   Default: **`if` mount**, matching the stated "renders only the
   shown half" contract.
4. **search-palette.slint:96,107** — the overflow caption
   (`visible: root.overflow > 0`) and the empty-state caption
   (`visible: root.rows.length == 0`): the palette always carries up
   to two dead caption bands at the bottom. **Fix shape**: `if` mounts
   for both (the palette is a transient overlay — position stability
   across its own states has no consumer; the empty-state caption's
   own comment says it is "judged at the walkthrough; never asserted
   by tests", so no text oracle exists to disturb).

## 2. Decisions

### D1 — #1491: stack the insistence picker onto its own row at narrow widths (a width-fact branch, not a measurement hack)

The row's three children cannot shrink below their min-content at
kiosk scale, and slint has no measurement-conditional wrap. The house
idiom for width-dependent composition is the pushed facts/breakpoint
pattern (`responsive.slint` — Rust owns the width→breakpoint mapping;
`TodayLayoutFacts.grid-arm` is the same-chunk precedent). Fix shape:

- TimersSection gains a `in property <bool> stacked-insistence :=
  false` (or reads `Responsive.current-breakpoint` directly if the
  kiosk timers leg already maps to a breakpoint — stage 2 picks
  whichever fact the timers' host window already carries; the
  notification surface and today page both render at kiosk scale, so
  the fact must be per-surface width, not a global breakpoint guess —
  prefer a width fact the host pushes, mirroring
  `TodayLayoutFacts`).
- Narrow fact ⇒ the composition mounts `VerticalLayout` with row one
  = {label-field, when-field} and row two = the SegmentedControl;
  wide ⇒ today's single row. Both branches carry the identical
  bindings/commit seams (the Return-commit, the validity caption, the
  insistence rung) — a placement-only branch like the
  TodaySectionBlock extraction precedent ("placement-only refactor"
  comment, `today-view.slint:160-166`).
- Why this over capping `min-width`: truncating the timer-label
  placeholder at kiosk (the issue's option 2) sacrifices readability
  on the exact surface kiosks stare at; the wrap keeps every glyph.
  Why not restructuring per "the kiosk timers design" (option 3): no
  accepted design round asks for one — out of scope.
- The left-edge 206 px run (notification panel ring/shadow): stage 2
  first checks #1416's recorded disposition; only if the disposition
  does not cover it does it become an inset fix here — otherwise it
  stays with #1416 and is **not** touched (scope wall).

### D2 — #1504: heal the absorber, keep the #1435 inset intact

The vertical inset legs are load-bearing (they heal #1435's
top/bottom flush rows — removing them regresses a landed fix; not an
option). The tight fit is the *absorber* running out of height. Fix
shape, smallest-first decision tree, stage 2 executes the first that
measures green:

- **(a) default — outer page scroll on the task detail root**: wrap
  the detail composition's scrollable body in the existing
  `ui/scroll-page.slint` substrate (the today page's scroll idiom —
  the precedent the seek-page/`focus-scroll.slint` machinery already
  serves), with the description section returning to a
  content-serving min-height. No content is ever flush at the floor
  by construction; a scrollbar thumb, when it appears, classifies
  `accept-as-legacy thumb` (the law's own disposition). This heals
  both C1 signatures for every scale, not just 1.5×.
- **(b) fallback** — if (a) collides with the focus/walk machinery on
  the detail surface (the overlay law, the ActivationStop grammar) in
  a way stage 2 cannot resolve cheaply: raise the description
  viewport's guaranteed budget at tight fits only (a
  min-height bump on the flexible section at sub-regular
  breakpoints), accepting that the clip run shrinks but may not reach
  zero — **this fallback requires a fresh operator ruling** (it
  re-grades a `real-clip fix-now` as partial).
- Stage 2 first writes the probe (`T2` below) that pins the current
  44/106 px geometry as the red, then lands the fix.

### D3 — #1464: `if` mounts at all six sites; no reserved-slot carve-outs

Per §1's per-site findings: all six sites convert to `if` mounts. No
site records a reserved-slot intent instead — none of the six has a
stated position-stability consumer (the C-4 comment's stated intent
*contradicts* its mechanism; the palette is transient; the today
section's comment already claims collapse). The one deliberate
`visible:` that stays is `today-view.slint:356`
(`context-line: visible: row.context != ""`) — a per-row caption
inside a fixed-height card row where the slot IS the row's height
stability; the issue does not name it and it is out of scope.

### D4 — #1461: stale — evidence comment, no code

As §1 records: fixed by `a92b6ad1`, pinned by
`board_empty_stub_probe_tests.rs`. Stage 2 comments the evidence on
the issue; the dispatcher closes it at merge. The chunk's diff
contains zero #1461 code.

### D5 — UX-plan decision per issue (docs/ux_planning.org trigger rule cited)

The trigger rule: a plan that "adds or changes a user-visible
capability — any new verb, badge, view, or setting a human can
perceive or act on" needs an accepted UX journey plan; pure repair of
an existing surface is exempt.

- **#1491 — no journey plan; bug fix restoring an accepted contract.**
  The no-clip contract is the clip-sentinel law plan's accepted §3
  fix-shape table (N-today group) — the ownership artefact for this
  geometry class. The narrow-width stack adds no verb/badge/view/setting
  (the same three controls, re-flowed). The look change is handled as
  ruling R-1 below, not as a journey plan.
- **#1504 — no journey plan under default (a) is *not* clean: the
  outer scroll adds a perceivable/actable affordance (a scrollbar) to
  the task detail.** Decision: default (a) **amends the owning journey
  plan** for the task-detail surface. The owning plan is the
  task-detail/detail-editor journey lineage (the H4 habit-editor plan
  and the #1435 inset's owning chunk,
  `.artifacts/plans/2026-11-25-ui-small-inventory-plan.md` group C).
  **Amendment text** (stage 2 appends to the owning journey plan's
  rulings section before implementation): *"R-x (2026-11-28): the
  task-detail composition gains the workspace's outer page scroll
  (ScrollPage substrate) so the flexible description section is no
  longer the only vertical absorber; the #1435 vertical inset legs
  stay. New affordance: the detail page scrolls when content exceeds
  the viewport; the description viewport's internal scroll remains
  the nested surface. Journey impact: no story step changes — the
  detail's verbs, badges and fields are untouched; S-steps that
  assert on-screen presence of below-fold content now assert
  scroll-then-visible. Coverage: no W-row changes; the geometry probe
  T2 is the wiring evidence."* Under fallback (b), no journey plan
  (pure inset repair) — but (b) needs ruling R-2 instead.
- **#1464 — no journey plan; the §5 gating-law sweep restores the
  recorded intent at each site** (each site's own comment already
  states the post-fix behaviour: "empty sections collapse", "the bar
  renders only the shown half", the palette's caption gates). No new
  verbs; dead bands removed. The habit-variant density win is
  #1430's ask landing by mechanism, not a new capability — noted on
  #1430 at merge, not journey-planned.
- **#1461 — n/a** (stale; nothing ships).

## 3. Invariants (stage 2 must hold, stage 3 must verify)

1. **Board columns wrap / seek probe**: the #1543-landed Flickable
   wrap and `tests/probes/columns_seek_probe_tests.rs` pins stay
   untouched and green — this chunk touches no board-columns file.
   `board-layout.slint` is *not* in the diff (the empty-board CTAs
   are already fixed; #1461 contributes no code).
2. **Dead-zone click-out layers** (#1543): `dead-zone-click-out`
   (`board-layout.slint:226-236`) untouched; the journey
   `a_dead_zone_click_settles_the_create_flow`
   (`tests/journeys.rs`) green, assertions byte-identical.
3. **Clip-sentinel law**: the classifier's tests and the corpus
   reconciliation stay green and **stricter-or-equal** — after this
   chunk, the regenerated findings report must show the #1491
   N-today fix-now rows and the two #1504 rows **gone** (healed), the
   board-columns accepted-legacy rows **unchanged** (still recorded),
   and no new fix-now row on any touched golden.
4. **#1435 inset intact**: `editor_inset_probe_tests.rs` green
   unchanged; the vertical padding legs stay on the detail root.
5. **MultiLineEditor viewport contract**: `editor_viewport_probe_tests.rs`
   green unchanged (the widget is not modified; if D2(a) is chosen,
   the host changes, not the widget).
6. **UI purity (AGENTS §2)**: `taskboard-ui-slint` gains no IO crate
   dependency; the width fact is a pushed presentation fact, no
   business logic moves into the UI.
7. **The commit-contract seams survive the #1464 sweep verbatim**:
   the quick-bar index routing (`invoke-quick-bar(bar-index)` with
   the Rust-side index resolver), the timers Return-commit seam, the
   habit save/cancel row, the today section callbacks — bindings
   byte-identical across the `visible:`→`if` conversions.
8. **Golden determinism law**: every re-recorded golden is
   deterministic per the round-7 G1 law; the regenerated findings
   report lands in the same commit as its healing change.

## 4. Test catalogue (lowest layer, AGENTS §5 — typed/structural, no text assertions, no clock dependence)

- **T1 (#1491, probe)** — extend/register a timers probe
  (the `today_window_max_probe` rig or a new
  `timers_row_probe.slint` + `tests/probes/timers_row_probe_tests.rs`
  per the widget-framework §5 facts/probe pattern): at kiosk scale
  (base-scale 3.0, ~333 logical viewport), assert the row's children
  all satisfy `x ≥ 0 ∧ x + w ≤ viewport-width` (typed geometry, the
  `board_empty_stub_probe` idiom) in **both** fact arms, and that the
  stacked arm mounts the SegmentedControl below (not beside) the
  fields — `segmented-y > fields-y` and `segmented-x == fields-x`.
- **T2 (#1504, probe)** — a `task_detail_fit_probe` assertion: at the
  1.5×/900 px fit, no description-viewport content paints past the
  window floor (typed: the viewport's clip edge `y + h ≤ window
  height − Theme.space-2`, and under D2(a) the page scroll's content
  extent exceeds the viewport iff the section's content demands it —
  the scroll affordance exists exactly when needed). Red first on
  main (the 44/106 px signatures), green after.
- **T3 (#1464, probe per surface)** — structural mount assertions:
  (i) today: an empty section contributes zero height (the
  section delegate's rect height == 0 with `count == 0`; header
  unmounted — element lookup by id returns no match); (ii) task view:
  on the habit variant none of the seven task-only row ids resolve
  (`find_all_by_id_on` is empty) and the save row's band above
  shrinks (typed y-position delta); on the task variant all ids
  resolve exactly once (the geometry facts still publish);
  (iii) action menu: a model with one withheld row renders exactly
  one icon button (the shown half) and `invoke-quick-bar` on the
  shown index routes (the index contract's twin);
  (iv) palette: with no overflow and rows present, neither caption
  id resolves.
- **T4 (goldens, per the determinism law)** — expected re-records
  with per-PNG justification: `today-3.0-1000` +
  `notification-3.0-1000` (the timers row healed; the flush runs
  gone), `task-view-1.5-900` (+ any other task-view scale the absorber
  change shifts: `task-view-1.5-500`, `task-view-3.0-*`),
  habit-variant goldens and today page goldens carrying empty
  sections (the #1464 dead-band removals). Every re-recorded PNG
  lands in the same commit as its cause; the regenerated findings
  report rides along (invariant 8).
- **T5 (existing pins that must not weaken)** —
  `board_empty_stub_probe_tests.rs` (the #1461 evidence, stays green
  untouched), `columns_seek_probe_tests.rs`, the #1490/#1543 journeys,
  `editor_inset_probe_tests.rs`, `editor_viewport_probe_tests.rs`,
  the StateEngine snapshots (untouched — no engine file in the diff).

## 5. Chunk split

**One chunk, one branch (`fix/ui-clip-fix-waves`), one PR.** Rationale:
all four issues are presentation-layer layout repairs verified by the
same golden corpus + findings report regeneration, which is a single
deterministic pass (two chunks would regenerate the same artifacts
twice and fight over the report); #1461 contributes no code. The
natural commit sequence inside the chunk keeps each commit standalone
(rebase hygiene): (1) #1464 the gating sweep, (2) #1491 the timers
stack, (3) #1504 the absorber heal + findings/goldens regeneration.
Estimated size: **M** — four files of slint edits, two new probe
assertions, ~8-12 golden re-records, the report regeneration; ~400-600
changed lines including goldens.

## 6. Verification commands (expected outcomes)

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint   # probes + journeys green
scripts/run-quality-gates.sh cargo nextest run --workspace       # the flock-safe gate
python3 scripts/layout_review.py --regenerate                    # the findings report heals (invariant 3)
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

(Stage 2 runs the full gate per the discipline block; the coverage
trio never runs inside a subagent — the dispatcher owns it.)

## 7. Rulings requested (proceed on defaults)

- **R-1 (#1491) — the kiosk timers row stacks.** The insistence
  SegmentedControl moves onto its own row when the surface is narrow;
  at kiosk scale the timers editor visibly re-flows (two rows instead
  of one). Alternatives: cap the fields' min-widths so they truncate
  (rejected D1: placeholder readability on the kiosk's primary
  surface). **Recommended default: the stack (D1).** Silent if
  unruled.
- **R-2 (#1504) — the task detail gains outer page scroll (D2(a)).**
  The detail page becomes a scrolling page when content exceeds the
  viewport; the description section stops being the only absorber; a
  scrollbar thumb appears exactly when needed. This visibly changes
  the task-detail surface (the ruling also settles the journey-plan
  amendment per D5). Fallback (b) — a taller guaranteed description
  budget at tight fits, leaving a smaller residual clip — needs this
  ruling explicitly and re-grades a fix-now finding; **recommended
  default: (a).**
- **R-3 (#1464, habit variant) — the reclaimed dead column stays
  reclaimed.** After the `if` conversion the habit editor's rows
  top-align with the leftover band at the bottom (today a hidden
  stretch absorber holds the space). This is #1430's "wasted vertical
  space" cut landing; the habit-variant goldens shift. Alternative:
  give the habit block its own `vertical-stretch: 1` tail element to
  keep the exact current pinned-to-bottom look. **Recommended
  default: reclaim (no tail spacer) — the AGENTS §5 "no phantom
  rows" posture (F9's recorded kill of the old trailing-spacer
  shape).**
- **R-4 (#1461 closure)** — close as stale at merge with the
  `a92b6ad1` + probe evidence comment. **Recommended default: yes.**

## 8. Stage 2 prompt (IMPLEMENTATION) — paste-ready, verbatim

```text
You are the stage-2 IMPLEMENTATION agent for the "UI clip fixes" chunk (branch fix/ui-clip-fix-waves), taskboard repo, GitHub D4id4los/taskboard.

**Long-command discipline** (binding — docs/agent_dispatch.org; restated verbatim from the dispatch law): a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the test helpers plant `TASKBOARD_TEST_SANDBOX`). The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines — derive tables from ladders/enums, share scaffolding, do not copy-paste. If you run `scripts/ux-coverage.sh`, pass `--root /home/viktoria/src/taskboard-WT-fix-ui-clip-fix-waves`.

Worktree: /home/viktoria/src/taskboard-WT-fix-ui-clip-fix-waves (branch fix/ui-clip-fix-waves, off origin/main 3fc4764b). Recreate it on the branch if missing (git worktree add ../taskboard-WT-fix-ui-clip-fix-waves fix/ui-clip-fix-waves); never touch /home/viktoria/src/taskboard (the main checkout) or any other taskboard-WT-* directory.

1. Read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-28-ui-clip-fix-waves-plan.md (normative: §1 recorded reality, §2 decisions D1–D5, §3 invariants, §4 test catalogue, §7 rulings — the plan's defaults are in force), the clip-sentinel law plan .artifacts/plans/2026-11-25-clip-sentinel-law-plan.md (§1 rubric, §3 fix shapes, the determinism law), and the widget-framework base plan §5 (the probe assertion pattern). Skim docs/coding_guidelines.org §5 (the gating law) and docs/testing_strategy.org §6.1–6.2 (clock discipline for goldens).

2. Implement exactly per the plan, in this commit order, every commit standalone (rebase-merge hygiene, `<type>(<module>): <desc>` + body):

   a. **#1464 — the `visible:`→`if` gating sweep** (plan §1 site list is exhaustive — exactly these six, nothing else):
      - crates/taskboard-ui-slint/ui/views/today-view.slint:174 (TodaySectionBlock: gate the inner block with `if (sec.count > 0)`, NOT a bare root conditional — the recorded slint constraint: a repeater delegate cannot carry one; the D7 comment already claims this behaviour),
      - crates/taskboard-ui-slint/ui/views/task-view.slint: :782 (nudge preview → `if (root.habit.nudge-preview.visible)`), :809/:945/:1044/:1140/:1271/:1336/:1485 (the seven task-only rows → `if (!root.task.is-habit):`; the file's existing idiom at :865/:877 is the pattern). The probe-geometry out properties (:438-442) keep their bindings; the geometry facts are consumed on the task variant.
      - crates/taskboard-ui-slint/ui/compositions/action-menu.slint:162 (withheld quick-bar row → `if (bar-entry.visible)`; the `for` delegate stays, index routing `invoke-quick-bar(bar-index)` byte-identical),
      - crates/taskboard-ui-slint/ui/compositions/search-palette.slint:96/:107 (both captions → `if` mounts).
      Bindings and commit seams survive verbatim (invariant 7). Do NOT touch today-view.slint:356 (context-line — its slot IS the row height stability).
      Tests: plan §4 T3 (structural mount assertions per surface, `find_all_by_id_on`-style, no text assertions). Re-record the shifted goldens (determinism law: same commit as the cause; habit-variant and empty-section goldens shift — per-PNG justification in the commit body).

   b. **#1491 — the timers editor row stacks at narrow width** (plan D1): crates/taskboard-ui-slint/ui/compositions/timers.slint — the `if (!root.absence-hint)` block's HorizontalLayout gains the width-fact branch: narrow ⇒ row one {label-field, when-field}, row two the insistence SegmentedControl; wide ⇒ today's single row. Push the fact from the host (the TodayLayoutFacts pattern — per-surface width fact, not a global breakpoint guess). Both arms carry identical bindings/commit seams (placement-only). Check #1416's recorded shadow-blur disposition first: the notification panel's left-edge 206 px run is #1416's unless the disposition does not cover it — default: do NOT touch the notification panel.
      Tests: plan §4 T1 (typed geometry: all children inside the viewport at kiosk scale in both arms; stacked arm: segmented-y > fields-y). Re-record today-3.0-1000 / notification-3.0-1000.

   c. **#1504 — the task-detail absorber heal** (plan D2, default (a)): measure first — write T2 red on main's geometry, then wrap the task-detail composition's body in the ScrollPage substrate (crates/taskboard-ui-slint/ui/scroll-page.slint — the today page's idiom) with the description section back to a content-serving min-height. The #1435 inset legs (task-view.slint:525-528) stay. If (a) collides with the detail surface's focus/walk/ActivationStop machinery in a way you cannot resolve cheaply, STOP that item, leave T2 red-but-recorded, and report — fallback (b) needs an operator ruling (plan R-2); do not improvise a third shape.
      Amend the owning journey plan per plan D5's verbatim amendment text (the task-detail lineage plan under .artifacts/plans/ — the #1435 owner, 2026-11-25-ui-small-inventory-plan.md group C, is the citation anchor). Re-record task-view goldens; regenerate the findings report in the same change (layout_review.py — background + poll if slow; commit before any wait).

   d. **#1461 — stale, no code** (plan D4): comment on the issue (`gh issue comment 1461 --repo D4id4los/taskboard`) with the evidence: fixed by a92b6ad1, pinned by tests/probes/board_empty_stub_probe_tests.rs::the_empty_board_ctas_stay_off_the_canvas_edges; note "dispatcher closes at merge". board-layout.slint must not appear in your diff.

3. Gates: pre-commit hooks run automatically. Then `scripts/run-quality-gates.sh cargo nextest run --workspace` per the discipline above (background + poll; flock-deferral per the rule). Keep green: columns_seek_probe_tests.rs, the #1490/#1543 journeys (a_dead_zone_click_settles_the_create_flow — assertions byte-identical), editor_inset_probe_tests.rs, editor_viewport_probe_tests.rs, board_empty_stub_probe_tests.rs (invariants 1-5). The regenerated findings report must show the #1491 and #1504 fix-now rows gone and the board-columns accepted-legacy rows unchanged (invariant 3). No text assertions, no sleeps, no quality ceilings anywhere (AGENTS §5). SonarCloud new-code duplication ≤ 3 %.

4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

5. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results (or the flock-deferral line), the golden re-record list with per-PNG justification, T2's red→green evidence, any deviation from the plan and why.
```

## 9. Stage 3 prompt (REVIEW) — paste-ready, verbatim

```text
You are the stage-3 REVIEW agent for the "UI clip fixes" chunk (branch fix/ui-clip-fix-waves), taskboard repo, GitHub D4id4los/taskboard.

**Long-command discipline** (binding — docs/agent_dispatch.org; restated verbatim from the dispatch law): a subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) **Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`** — nothing bare. (5) *Stage prompts restate this discipline verbatim.* **Never commit with `--no-verify` or any hook bypass.** Never let a test or binary you run touch the user's real taskboard data (isolated config/data dir under /var/tmp; the test helpers plant `TASKBOARD_TEST_SANDBOX`). The required SonarCloud quality gate fails a PR whose NEW code has more than 3 % duplicated lines — treat a duplication finding as a finding and fix it on the branch. If you run `scripts/ux-coverage.sh`, pass `--root /home/viktoria/src/taskboard-WT-fix-ui-clip-fix-waves`.

Worktree: /home/viktoria/src/taskboard-WT-fix-ui-clip-fix-waves. Your scope ends at PR-open: you do NOT remove the worktree — the dispatcher owns teardown and at-merge issue closures (it will also close #1461 as stale per the plan's R-4; verify your PR body notes that).

1. BEGIN by rebasing onto main: git fetch origin --prune; rebase the branch onto origin/main (AGENTS §8's rebase dance — drop already-merged duplicate commits, resolve only real conflicts); push --force-with-lease if history was rewritten.

2. Read AGENTS.md, the stage-1 plan .artifacts/plans/2026-11-28-ui-clip-fix-waves-plan.md, the clip-sentinel law plan (§1/§3), and the widget-framework base plan §5; then review the entire branch diff versus the post-rebase base (git diff origin/main...HEAD) against the plan's acceptance criteria.

3. Functional focus — this chunk's specific traps (from the plan's invariants §3; not generic advice):
   - The `visible:`→`if` sweep must be EXACTLY the plan's six sites; a seventh conversion (especially today-view.slint:356 context-line) is a finding. Bindings and commit seams byte-identical across conversions — diff the binding blocks, not just the gating lines (invariant 7). The quick-bar index routing (`invoke-quick-bar(bar-index)`), the timers Return-commit seam, the habit save/cancel row must be verbatim.
   - No board file in the diff: columns_seek_probe_tests.rs and the #1490/#1543 journeys (a_dead_zone_click_settles_the_create_flow) must be untouched and green — the tests that must not have been weakened (invariants 1-2). board_empty_stub_probe_tests.rs untouched (#1461 evidence).
   - The #1435 vertical inset legs (task-view.slint padding-top/bottom) still present; editor_inset_probe_tests.rs and editor_viewport_probe_tests.rs unmodified and green (invariants 4-5).
   - The regenerated findings report shows the #1491 N-today fix-now rows and the two #1504 rows GONE, the board-columns accepted-legacy rows unchanged, no new fix-now row (invariant 3) — the classifier stays stricter-or-equal. Every re-recorded golden sits in the same commit as its cause (invariant 8).
   - No text assertions, no sleeps, no fixed-clock dependencies in the new tests; typed/structural geometry assertions only (AGENTS §5).
   - SonarCloud new-code duplication ≤ 3 %.

4. **Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — the UI is a presentation client; (c) a single level of abstraction per function body. Findings are fixed on the branch.

5. Re-run the gates per the discipline above (pre-commit hooks + `scripts/run-quality-gates.sh cargo nextest run --workspace`; background + poll; flock-deferral per the rule). Any failure is a finding.

6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).

7. Ship: push the branch, open the PR to main with gh (body: the four issues, the per-issue decisions, the stale-#1461 note for the dispatcher's at-merge closure, the golden re-record list). Then STOP — do not watch checks; the dispatcher owns `gh pr checks --watch` and merge-state verification. Do not remove the worktree.

8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, the gate evidence (or the flock-deferral line). Never merge; never touch main; do not remove the worktree.
```

## 10. Completion bookkeeping

- **Closes at merge** (dispatcher): #1491, #1504, #1464; **#1461 closed
  as stale** with the evidence comment (R-4). **#1430** gets a
  comment: the dead-column mechanism of its ask lands here; its
  broader density scope stays open. **#1416**: if stage 2 confirms
  the disposition covers the notification left-edge run, a comment
  records the check.
- **Deliberately not dispatched here**: the #1365 board-columns
  overflow regime (accepted legacy pending #1490's wrap); #1430's
  full density design; the kiosk timers redesign (no accepted round
  asks for one).
- Follow-up: the findings report's residual explanation shrinks by
  six rows after this chunk; the next layout-review round should
  re-baseline its residual count in its own regeneration.
