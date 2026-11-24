# A5a — Focus-Out-of-View Scrolls Into View (implementation plan)

- **Date**: 2026-11-24
- **Kind**: implementation chunk plan (chunk A5a of round 6, "p1 GUI
  wave").
- **Origin**: scope report
  `.artifacts/reports/2026-11-24-backlog-reduction-round6-scope-report.md`
  §A-A5 (normative); dispatch plan
  `.artifacts/plans/2026-11-24-backlog-reduction-round6-dispatch-plan.md`
  §1/§4 (normative); issue **#1302** (p1, ui, bug). Round laws: typed/
  structural pins only (row index / position within the visible
  window, never pixel-text); diagnosis-first (the incomplete path's
  inventory is a stage-1 deliverable, §1); out-of-scope findings →
  backlog issues immediately (§6: #1320).
- **Branch**: `fix/scroll-to-focused`
- **Status**: READY — every open question resolved below with a
  recorded decision.

## 1. Verified diagnosis & the inventory (stage-1 code-read, 2026-11-24, branch tip `dc96c034`)

The issue's premise holds: the scroll-to-focused seam exists
(`FocusScrollHint { target-a, target-b, nonce }`, ui/focus-scroll.slint;
the adapter edge-triggers it after a keyboard focus landing via
`push_focus_scroll_hint`, adapter.rs ~6085–6127; the matching container
consumes it through `ListView::scroll-row-into-view`,
widgets/listview.slint ~53–66), but it is incomplete on three axes.

**Inventory — every view family that hosts focusable rows:**

| Surface | Hint pushed? | Consumed? | Gap |
|---|---|---|---|
| **Today, task rows** | yes — `(section, row)` via `tables.today_position` (models.rs ~196) | yes — each section's inner ListView (today-view.slint ~378) | (a) the **outer page `Flickable`** (today-view.slint ~187) is never adjusted by a focus move — a focused section below the fold seeks *off-screen*; (b) the inner seek's `visible-height` is bound to the window-derived cap (~371), not the section clip — correctness rides **A1's geometry fix** (D-5 of A1's plan); (c) `avg-est` seek arithmetic stays estimate-based — acceptable once A1 aligns the window |
| **Today, habit rows** | **no** — `push_focus_scroll_hint` resolves `focused_task` only; the walk's `focused_habit` landing (view_state.rs `focus_today_row`, ~2410) arms no table position and fires no hint | — | the entire habits section is dark to the seam |
| **Today, roster rows** (break rules) | **no** — same: `break_rule_cursor` landings never hint | — | same darkness |
| **Board, desktop cards** | yes — `(column, row)` via `tables.board_position` (adapter ~6098, consumed board-layout.slint ~544) | inner per-column ListView only | the **columns row scrolls horizontally** (board-layout.slint ~3, ~302) and is never brought along: walking into an off-screen column leaves the focused card outside the window |
| **Board, compact leg** | yes — same seam (~910) | selected column's viewport | the switcher-level landing (`selected_stack` change) swaps the whole visible column, so visibility is structural — **no gap**; verify with a pin (S-6) |
| **StackDetail** | yes — `(−1, row)` (adapter ~6106; stack-view.slint ~83) | the row viewport | none found — the single-column surface fills the viewport; mark complete, pin unregression (S-7) |
| **TaskDetail** | no — typed inert (adapter comment) | — | correct by decision: no row-list container; the detail sheet is fixed-height with internal widget scrollers |
| **Review** | no — typed inert (D-8: no focusable rows) | pages via `ScrollPageHint` family 4 | correct by decision: a read-only ledger |
| **Settings** | no hint; adapter-side facts paging (`page_settings`, round-3 D26) | facts two-way viewport | correct by decision: no task-row walk |
| **Status** | no — typed inert (OP3) | — | correct by decision: read-only panel |
| **PlanNextDay** | no — typed inert per #622 D-1 | pane `Flickable` (plan-next-day-view.slint ~88) can overflow; `plan_position`/`plan_row` tables exist | **out-of-scope gap — filed as backlog #1320**; this chunk keeps the #622 fixed-surface decision |

**Kiosk posture**: `KioskMode` maps only Board/Today onto the *same*
compositions (`KioskMode::view`, view_state.rs ~98–110); the kiosk walk
runs through the same `step_focus` (`kiosk` parameter). There is no
kiosk-specific view family — fixing the desktop seams fixes kiosk by
construction. Kiosk notes recorded in §3 (K-1/K-2).

**The stop ladder the scroll must not fight**: the hint lives on the
*focus-landing* path only (pushed after the walk's `RefreshNow`
outcomes; `focused.is_none() || focused == before` re-arms nothing —
the walk's ends hold). Pointer selections hint nothing. Ladder-
consumed keys (dropdown/abort ladders, adapter.rs ~1435–1465) never
reach the walk, hence never hint. The extension must preserve all
three properties: **the scroll reacts to a focus landing, never to a
key**, and a landing that changed nothing scrolls nothing.

## 2. Decisions (recorded, binding for stage 2)

**D-1 — fix level: one seam, extended — no second mechanism.**
`FocusScrollHint` remains the single scroll-into-view contract. The
gaps are coverage gaps (rows with no table position, containers with
no consumer), not a seam-design failure. No new global, no new hint
family beyond the existing `target-a`/`target-b` pair convention.

**D-2 — Today hint coverage widens to every walkable row.** The id
tables gain positions for the currently-dark rows against the *same*
`(section, row)` coordinate space the view already consumes:

- `tables.today_habit_position(habit) -> Option<(usize, usize)>` and
  `tables.today_roster_position(rule) -> Option<(usize, usize)>`
  (naming stage 2's call; the contract is fixed): section index =
  the habits / break-rules section's index in the pushed
  `TodayModel.sections` (or the roster's own coordinate — stage 2
  matches what the view's consumers filter on), row index = the row
  within that section. Built during `push_today`'s table pass, the
  same place `today_tasks`/`today_section_rows` are filled — one
  derivation, walk == render coordinates by construction (A1's D-2
  law).
- `push_focus_scroll_hint` resolves the walk subject via
  `walk_subject()` (task **or** habit, view_state.rs ~1457) plus the
  roster cursor — the `focused.is_none() || focused == before`
  guard generalizes to the subject tuple (task, habit, roster) so a
  habit landing whose habit did not change re-arms nothing.
- Consumers: the habits section root (HabitsSection has no inner
  scroll — it is content-sized, see D-3) binds the hint for its own
  section index and delegates the seek entirely to the page level;
  the break-rules section likewise (its `rule-list` host consumes
  only if it has an inner scroller — stage 2 verifies; if not, page
  only).

**D-3 — the outer page scroll is the primary Today fix.** With A1's
declarative geometry, fitting sections are content-sized (no inner
scroll to seek) and only the window cap forces inner scrolling; the
dominant failure — "focused section below the fold" — lives at the
page `Flickable`. The page gains a hint consumer: on a `FocusScrollHint`
nonce edge, it seeks so the *target section* is within the visible
window, using structural geometry (the section's layout position /
offset among sections — the #592 `TodayAnchorFacts` extent channel is
the sanctioned measurement source; **no pixel-text assertions and no
hard-coded offsets** in code or pins). Inner ListViews keep their
existing consumption unchanged — outer and inner seek compose (outer
brings the section into the window; inner brings the row into the
section's viewport).

**D-4 — Board gains a column-into-view seek.** The columns row's
horizontal scroll container gains the same consumption pattern at the
column granularity: on a hint edge, seek so the target `column-index`
is within the visible column window (structural: column index vs
window, never pixels). The per-column inner `request-row` consumers
stay as-is; outer column seek + inner row seek compose exactly like
D-3's two levels.

**D-5 — typed inerts stay typed inerts.** TaskDetail, Review,
Settings, Status keep their recorded decisions (§1 table) — no hint,
no consumer, and the acceptance's "all views" reads as *all views
with a walkable row list*. PlanNextDay stays inert in this chunk
(#622's decision stands); the residual overflow gap is backlog
#1320.

**D-6 — sequencing law honored (A1's D-5, binding).** This chunk's
**Today-view journey-level pins stage after A1
(`fix/today-geometry-aggregation`) merges**: aggregation changes the
habits section's row count and A1's declarative heights change the
viewport semantics the seek rides. Stage 2 order: (1) the adapter/table
widening + Board column seek + their pins (A1-independent) land first
on this branch; (2) the Today page/section consumers are implemented
against A1's recorded contract (D-1's declarative shape), but the
Today journey-level pins (T-1..T-4 of §4) are written and gated only
after rebasing onto a fetched `origin/main` that contains A1 — the
rebase dance per AGENTS §8 if this branch was already pushed; (3) if
A1 has not landed when stage 2 otherwise completes, the PR waits
(hand-off requires the Today pins to exist and pass). The Board,
compact, stack-detail, and adapter pins are not A1-gated.

**D-7 — the walk's ends, the pointer law, and the ladder stay
untouched.** No hint on pointer selection; no hint when a landing
changes nothing (the generalized subject-tuple guard, D-2); no
scroll write on the key path (ladders consume before the walk runs).
This is asserted, not assumed — S-1/S-2 pins.

**D-8 — kiosk rides the same seams (K-posture notes).**
- K-1: no kiosk-specific code path — the Board/Today compositions are
  shared, so the fix is kiosk-complete by construction; the kiosk pins
  run the same journeys under the kiosk flag where the harness
  already mounts kiosk postures.
- K-2: kiosk is a passive display — the fix must never *proactively*
  scroll (no timers, no rotation-triggered seeks). The stale-hint
  analysis: a hint fired pre-rotation finds no matching consumer index
  after rotation (different column/section set) and decays unused;
  the nonce edge is consumed only by an index match, so a stale hint
  cannot misfire — verified by construction, no extra guard needed
  (S-8 pins the decay).

## 3. Test catalogue (structural pins only — row/column index,
position within the visible window; never pixel-text)

Adapter/table level (A1-independent; unit tests in models/adapter):

- S-1 `habit_and_roster_landings_arm_the_hint`: walking onto a habit
  row and onto a roster row edges `FocusScrollHint` with their
  `(section, row)`; identical re-landings re-arm nothing (nonce
  stable).
- S-2 `pointer_and_ends_rearm_nothing`: a pointer selection, and
  vertical walks into both walk ends, leave the nonce unchanged (the
  ends-hold law, now pinned for the widened subjects).
- S-3 `positions_agree_with_the_pushed_sections`: the new positions'
  section/row indices resolve to exactly the pushed model's rows
  (walk == render coordinates).

Container level:

- S-4 `board_column_seek_brings_the_column_into_the_window`: with
  more columns than fit, walking into the last column scrolls the
  columns row so that column is the visible window's last (index
  arithmetic, not pixels); walking back up brings the first column
  into view. Both directions per the acceptance.
- S-5 `compact_switcher_landing_swaps_the_visible_column` (no gap
  pin): the selected stack's landing shows that stack's column
  (structural: the selected index drives the rendered column).
- S-6 `stack_detail_seek_unregressed`: existing `(−1, row)` seek
  behavior pinned against the touched seam (guard pin, not new
  behavior).
- S-7 `inert_surfaces_stay_inert`: TaskDetail/Review/Settings/Status/
  PlanNextDay landings arm no hint and write no container offset
  (typed outcome: the globals' nonce/index unchanged).

Today journey-level pins (**A1-gated per D-6**; probe file
`tests/probes/virtualisation.rs` or a sibling — the driver already
exposes scroll/viewport structure):

- T-1 `today_walk_past_the_fold_brings_the_section_into_view`:
  populate sections beyond the window; walking down past the fold
  scrolls the page so the focused section is within the visible
  window; walking up past the top does the same in reverse.
- T-2 `habit_row_focus_becomes_visible`: with the habits section
  below the fold, walking onto a habit row brings the section into
  the window.
- T-3 `roster_row_focus_becomes_visible`: same for a break-rule row.
- T-4 `inner_and_outer_seek_compose`: a tall capped section (A1's
  cap semantics) with the focused row below *its* fold scrolls both
  levels — the section into the window and the row into the
  section's viewport.
- T-5 `stale_hint_decays_after_remount` (K-2): a hint fired before a
  surface re-mount misfires nothing (nonce consumed only by an index
  match).

Existing suites re-checked, never weakened: `tests/journeys.rs`
(the `FocusScrollHint` pins ~4847, ~22266), the virtualisation
probe's no-scroll-on-insert/remove pins (~771–1063 — the seek must
not fight the content-anchored contract; if a new seek breaks one,
diagnose per the never-weaken law: those pins encode the insertion
must not scroll *the page* law, which D-3 must respect — a focus
landing scrolls, a content mutation does not).

## 4. Acceptance (verbatim from #1302, annotated)

- Moving focus (walk, and chord-driven jumps — the jumps share
  `focus_domain`/`focus_today_row`, so they inherit the hint) in
  every view with a walkable row list (Today, Board desktop+compact,
  StackDetail) scrolls the containers — outer page scroll *and*
  inner row list (T-1..T-4, S-4) — so the focused element is
  visible.
- Works in both directions (down past the fold, up past the top)
  (T-1, S-4).
- The incomplete path's inventory recorded (§1 — this plan is the
  record).
- The N-3 focus contract composes (dispatch plan): the hint's
  `(target-a, target-b)` convention is unchanged; only coverage and
  consumers widen (S-3/S-7).
- Kiosk postures behave identically by construction (K-1, T-5).

## 5. Risks & size

- **A1 coupling (D-6)**: the Today page seek measures section
  geometry through the #592 extent channel; if A1 changes that
  channel's shape, the Today pins re-base on the merged contract.
  Mitigated by staging the Today pins after A1's merge.
- **Content-anchored scroll fights**: the page `Flickable`'s
  content-y is contract-pinned by the no-scroll-on-insert/remove
  tests; the seek writes only on a hint edge, never on refresh —
  the same edge discipline the inner ListViews already survive
  under. Risk low, but S-2/T-guard pins must run against insertion
  storms (the probe's campaign cases).
- **Roster cursor generality**: `break_rule_cursor` is a cursor, not
  a subject in `walk_subject()`; D-2's guard must compare it too
  without breaking the K4 arming law (US-9/W-10). Stage 2 reads the
  arming tests before touching the guard.
- **Estimated diff**: ~450–600 lines — adapter.rs ~60–90 (subject
  guard + hint resolution), models.rs ~60–90 (the two position
  tables + table pass), today-view.slint ~60–90 (page consumer +
  habit/roster section consumption), board-layout.slint ~30–50
  (column seek), tests ~250–330, no domain change (crate boundary:
  all diffs in `taskboard-ui-slint`).

## 6. Backlog issues

- Stage 1 filed: **#1320** — `backlog:` PlanNextDay's pane Flickable
  never scrolls the keyboard focus into view (#622's typed inert
  stands; the residual overflow gap, D-5).
