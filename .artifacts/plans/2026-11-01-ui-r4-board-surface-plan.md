# [Plan] GUI Review Round 4, Chunk 1 — Board Surface Repairs

- **Date**: 2026-11-01
- **Status**: PROPOSED — stage-1 plan for chunk 1 of the round-4
  dispatch. Branch `fix/ui-r4-board-surface`, cut from `main`
  (7bce3173; `scripts/worktree-doctor.sh` passed pre-creation, 223 G
  free against the 25 G floor).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.1 (D1–D4), §1 findings
  R4-F2/F6/F7/F8, §4 testing, §7 risks; dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§2 (acceptance a–e; code-quality mandate per §1).
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`).
  `taskboard-domain`, `taskboard-state`, storage, sync, app: **empty
  diff**. Non-goals per dispatch §2: no column-width model change
  (chunk 2), no icon changes (chunk 3), no tooltip machinery (chunk
  4). The stack-width gesture seams (`stack-width-press/drag/release`,
  the adapter `width_drag` buffer, the header resize handle) stay
  **byte-identical** — this chunk must not pre-empt chunk 2.
- **All `file:line` anchors are this worktree at 7bce3173**; the
  design round's anchors were re-derived (several drifted ~60 lines
  in `adapter.rs`); every one below was read, not carried over.

## 0. The F8 repro audit — what the code confirms (context for D4)

The user's symptom: the Add-task ghost needs two clicks on the active
stack, three on a non-active one; Ctrl+O works first try. Static audit
of the current action layer (all anchors read):

- The ghost handler (`adapter.rs:4006-4019`) already runs
  `select_column` (which settles, `:4093-4106` →
  `settle_pointer_conflict`, `:4067-4070`) + a second idempotent
  settle + `route(TaskCreate)`. The router arms `BeginCreate`
  unconditionally (`router.rs:488`); the table arms it under
  `Flow::None` (`adapter.rs:1047-1053`); under a foreign flow the
  resolver's `SettleThenRun` settles and re-runs
  (`adapter.rs:1537-1546`). `settle_flow` handles every `Flow` kind
  (`view_state.rs:1624-1656`) and returns `false` only on
  `Flow::None` — which `resolve_conflict` already early-returns on
  (`:1510-1515`). So **every settle-then-run re-dispatch fires
  today**, and the audit's four D4 mechanisms reduce, at the action
  layer, to the **targeting** defect: `create_target_stack`
  (`view_state.rs:1072-1088`) resolves focused→selected→first and
  never hears which column was clicked.
- The existing ghost journey (`tests/journeys.rs:2093`) drives
  `invoke_default_action()` — the a11y path straight into
  `add-task-activated`. It never exercises **physical pointer
  delivery** (press/hover/release against the real hit tree). The
  user's repro lives below the a11y seam; today's suite cannot see
  it. This is the journey gap §6 closes: a `click_at`-driven ghost
  journey becomes the F8 harness, and per AGENTS §5 a red-first
  failure there is a production bug, chased with evidence — not
  papered over.
- Consequence for the fix: the targeted payload (§4) plus
  select-on-click make the action layer correct *by construction* for
  every origin; the physical journey pins the observable behaviour;
  if the headless driver cannot reproduce the eaten click, the probe
  verdict is recorded (§8 R-A) and the Tier-4 manual checklist keeps
  it.

## 1. D1 — the card title fills the card to its edge (R4-F2)

**Facts**: the title `RoleText { overflow: elide }`
(`task-card.slint:166-171`) sits in an inner `VerticalLayout`
(`:159-163`) beside the fixed 16 px done-box (`:128-130`) in the
card-root `HorizontalLayout` (`:120`). Under the pinned Slint 1.18.1
an elided text's layout minimum is the width of one "…" glyph, so
layout squeezes land entirely on the title — the committed goldens
show checkbox + bare "…" cards (`tests/goldens/board-0.5-1500.png`).
The widget is shared: the board delegate (`drag-card := TaskCard
{ width: 100% }`, `board-layout.slint:552-554`), the compact leg's
`compact-task` (`:806-814`), and the today delegate
(`today-view.slint:216-231`, no explicit width — the compiler binds
it to the ListView width) all inherit one fix.

**Decision — stretch + a named min-width token, both in
`task-card.slint` + one new theme token.**

- The inner `VerticalLayout` gains `horizontal-stretch: 1;` — the
  text column claims the card's surplus instead of surrendering it.
- The title `RoleText` gains `min-width: Theme.card-title-min-width;`
  — the elide floor. Min-width (not stretch) is what defeats the
  ellipsis-minimum squeeze; stretch (not min-width) is what makes the
  title *fill* to the card's right edge. Both halves are required.
- **New theme token** (theme.slint, the spacing-class block beside
  `space-15`):
  `out property <length> card-title-min-width: 84px * base-scale;`
  Derivation (recorded in the token comment): ≈12 characters of
  `Typography.body` — Inter's average lowercase advance at 14 px is
  ≈0.5 em ≈ 7 px, 12 × 7 = 84. It is a readability *floor* for the
  elide, not a measurement; role-independent (a done card's caption
  role still gets the same layout floor). Name and value are stage-1
  decisions; stage 2 verifies visually at the golden matrix (0.5/3.0
  × 600/1000/1500) and may tune ±12 px **inside the golden-regen
  commit** with the derivation comment updated — never without it.
- The due/scheduled/effort chip rows below the title are untouched;
  they keep their content preferred width inside the stretched
  column.
- **Chunk-2 interplay — noted, not implemented**: the board
  delegate's `width: 100%` chain (`drag-card` → `drag-zone` → the
  `DropArea`, whose `min-width` binding comes from `width-px`,
  `board-layout.slint:290-294`) resolves against whatever width the
  column row allocates. Chunk 2 replaces the allocation model
  (stretch → explicit widths summing to the row, content minimums);
  the `100%` chain and this D1 fix are allocation-agnostic — the
  title keeps filling whatever width the column gets. No hunk here
  pre-figures chunk 2.

## 2. D2 — a right-click outside an open overlay closes it (R4-F6)

**Facts**: all four full-window dismissal layers listen to `clicked`
only — date-picker (`app-window.slint:441-446`), menu (`:476-481`),
palette (`:570-574`), capture (`:575-581`). A right-click press lands
on the layer (later sibling = hit-tested first) and is swallowed: the
runtime never fires `clicked` for it, and nothing beneath is reached —
so the overlay stays open. The codebase's right-click idiom is the
card's `pointer-event` arm (`task-card.slint:104-110`): fire on
`down && button == right`.

**Decision — one `pointer-event` arm per layer, calling that layer's
own dismiss callback:**

```slint
pointer-event(event) => {
    if (event.kind == PointerEventKind.down
            && event.button == PointerEventButton.right) {
        root.<date-picker|menu|palette|capture>-dismiss-requested();
    }
}
```

- **Why `down`, not a right-release**: `clicked` is exactly the seam
  that swallows right-button releases today; closing on the down
  half is also the no-flash behaviour stage 3 checks (dispatch §2
  review item d) — the menu is gone before any release processing.
- **No reopen from the same event**: each layer is hit-tested before
  every context seam beneath it (the later-sibling rule the layers
  themselves exist for), and a TouchArea that grabs the press does
  not propagate it — the right-down closes, full stop. No other seam
  fires.
- The left-click `clicked` arms stay untouched (the existing
  journeys keep their meaning); middle-click stays out of scope.
- The four arms are the same three-line shape — no shared component
  is extracted for four call sites (the r3 D4 "shared arm" precedent
  lives in Rust below the callbacks, not in slint).

## 3. D3 — a left-click on a column's empty body selects the stack (R4-F7)

**Facts**: the bare-surface seam exists for right-click only:
`body-hit := TouchArea` (`board-layout.slint:305-316`) is the FIRST
child of the column `card` (lowest z), firing `body-context` on
right-down; the header's `header-hit` already pairs
`clicked => header-activated` (`:363-364`) with the right-click arm
(`:369-376`). D3 makes the body's left-click symmetric with the
header's left-click.

**Decision — `clicked => root.body-activated(column-index)` on
`body-hit`, threaded exactly like `body-context`.**

- New callback `body-activated(int)` on `BoardLayout` (beside
  `body-context`, `:98`), threaded `board-layout` → `board-view` →
  `app-window` (`:282` pattern) → a new adapter handler that calls
  **`select_column`** (`adapter.rs:4093-4106`) — the identical seam
  the header and switcher use (`:3991-4004`).
- **Touch hierarchy — why the body cannot steal card clicks**:
  `body-hit` is declared before (and is hit-tested after) the
  header, the viewport's `drag-zone` DragAreas + `TaskCard`
  TouchAreas, the create field, and the ghost — they keep every
  click they ever got. `body-hit` sees only the bare surface: an
  empty stack's body, the space below the last card, the inner
  padding gutters. The below-last-card region's reachability is
  proven by the r3 D4 journey
  (`the_body_right_click_selects_and_opens_the_stack_menu`,
  `journeys.rs:1385` — the ListView passes un-grabbed pointers
  through with `mouse-drag-pan-enabled: false`).
- **`settle_pointer_conflict` interaction**: none new —
  `select_column` settles first (`:4094`), so a body click under a
  foreign flow settles-then-selects through the one per-kind rule
  (round-3 R1); the compact leg has no bare-body surface (the single
  column's viewport fills the leg) — out of scope, recorded.
- `clicked` is left-release semantics on the pinned runtime (that is
  precisely why D2's right-click needs `pointer-event`) — so the new
  arm is left-click only by construction, mirroring the header.

## 4. D4 — one click opens the create field in the clicked column (R4-F8)

**Decision (the shape, settled): `UiEffect::BeginCreate` gains a
payload — `BeginCreate { target: Option<StackId> }` — delivered via a
one-shot `DispatchCtx.create_target`.** Not a new effect variant.

- **Why payload-over-variant (one call, recorded)**: the house
  precedent for targeted effects is data-carrying variants with one
  class — `BoardSelect(board)`, `PickerKey(key)` (`router.rs:159-235`).
  A `BeginCreateAt(StackId)` variant would duplicate all seven
  `EK::BeginCreate` pair-table rows, the `class()` arm, the
  `apply_effect` arm, and `key_of` for zero semantic gain: the
  payload changes *where* the flow opens, never the
  (flow → flow-after) outcome, so the matrix stays payload-blind.
  `Option<StackId>` keeps `UiEffect: Copy`.
- **Delivery through the action layer** (click ≡ key by
  construction, the r2 D8 rule): `DispatchCtx` gains
  `create_target: Option<StackId>` (the eighth one-shot target —
  `reminder_target`/`board_target`/`timer_target` pattern,
  `router.rs:266-320`, moved/consumed in `route()`,
  `adapter.rs:472-486`). `router.rs:488` becomes
  `ActionId::TaskCreate => BeginCreate { target: ctx.create_target }`.
  The ghost handler (`adapter.rs:4006-4019`) resolves the clicked
  column through the id tables (`tables.stack_ids` — the D2
  index-only convention; a stale index yields `None`, never a wrong
  stack), sets `create_target`, and keeps `select_column` +
  `route(TaskCreate)`.
- **Target resolution** in `interpret_begin` (`adapter.rs:1367-1381`):
  `Some(stack)` → `begin_create(stack)` directly (the clicked column
  wins, `create_target_stack` bypassed); `None` → today's
  focused→selected→first resolution verbatim — **Ctrl+O is
  byte-identical** (acceptance e). `interpret_insert`'s vi-`i` arm
  (`:1353-1361`) keeps `create_target_stack` (keyboard origin,
  `target: None`).
- **Select-on-click**: `select_column` stays in the ghost handler —
  one click = settle + select + focus-first-task + open (design D4a).
  With the target carried, selection and field placement can never
  diverge again even if a future refactor reorders them.
- **The settle-nothing skip dies (design D4b, scoped ruling)**:
  `Resolve::SettleThenRun` (`adapter.rs:1537-1546`) drops the
  `if self.settle_flow(window, state)` gate — settle, then re-run
  unconditionally. Rationale, recorded: the gate is provably dead
  today (`resolve_conflict` early-returns on `Flow::None`, and
  `settle_flow` returns `false` nowhere else), so the removal is
  outcome-neutral — and it retires the *only* shape that could
  silently eat a settle-then-run re-dispatch if the kept-arm set
  ever shifts. That is F8's ruling applied at the arm's one home
  rather than per-effect. The flow table's observed outcomes are
  unchanged (the harness proves it — §6).
- **Flow-table totality impact, enumerated**: (i) `key_of`'s arm
  becomes `UiEffect::BeginCreate { .. } => EK::BeginCreate`
  (payload-blind, the `BoardSelect(_)` pattern,
  `flow_table_tests.rs:153`); (ii) `class()` unchanged — `Begin`
  (`router.rs:220`); (iii) the class table and the seven pair rows
  (`:358, :379, :390, :403, :414, :420, :428`) keep their outcomes —
  no edits; (iv) `all_effects` (`:600-645`) lists
  `BeginCreate { target: None }` **and** `BeginCreate { target:
  Some(fx.stack_b) }` — the driver runs both payloads through every
  flow, pinning payload-blindness by construction instead of
  convention; (v) no new wildcard arm anywhere (stage-3 review
  item e).

## 5. Testing catalogue (TDD-first; red where the behaviour is new)

House rules carry: typed outcomes, no text assertions, deterministic
by construction, one behaviour/one test at the lowest layer.

1. **D1 title fill** — the golden matrix regenerates
   (`TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
   --test golden_visuals`; 14 PNGs: board + today × 0.5/3.0 ×
   600/1000/1500 + the two kiosk goldens). Expectation: at 1000/1500
   the fixture titles render across the full card width; the 600
   compact goldens still elide **at the card's right edge** (the
   min-width floor never elides mid-card). The PNG diff is the
   review artefact. Optional structural probe (stage 2's call): a
   driver read of the title element's width vs the card width on one
   fixture card — goldens remain the gate.
2. **D2 dismissal arms** — four journeys, one per layer, each
   asserting the typed closed state (never text): (a) context menu
   open → `right_press_at` outside it → `menu-open` false (mirrors
   `an_outside_click_dismisses_the_menu`, `journeys.rs:627`, using
   the `right_press_at` helper, `support/driver.rs:258`); (b) palette
   flow open → right-press outside → flow closed (typed `Flow`
   check); (c) capture overlay open → right-press outside → closed;
   (d) task detail with the date picker open → right-press outside →
   `date-picker-open` false. One guard journey: a right-press on the
   bare board with the menu open closes it **and opens nothing**
   (no context menu — the layer ate the press).
3. **D3 body select** — (a) empty stack: `click_at` on the bare body
   → `board_model.selected_stack` == that column and the focused task
   == the stack's first task — **the exact state the header journey
   pins** (`mount_pushes_no_selection_and_a_header_click_moves_it`,
   `journeys.rs:1850`, mirrored assertion-for-assertion); (b)
   populated stack: click below the last card selects it (the
   pass-through region); (c) regression: the existing card
   click/double-click/context/drag journeys stay green unchanged
   (`the_context_menu_focuses_and_routes_the_card`,
   `a_double_click_across_a_refresh_opens_the_task_detail`,
   `the_drag_survives_interleaved_refreshes_and_engine_changes`) —
   they are the not-stealing harness.
4. **D4 targeted create** — (a) **the F8 harness**: a *physical*
   pointer click on a ghost (`click_at` its centre — press+release,
   not `invoke_default_action`) opens the create row in the clicked
   column and selects it, under `Flow::None` (red-first on main if
   the eaten click reproduces headlessly — then it is a production
   bug per AGENTS §5; the fix lands either way); (b) ghost click on a
   non-active column → field in that column + selection moved (one
   click, not two); (c) ghost click under a foreign flow (arm
   `Flow::RenameStack` via the keyboard route) → settles, then opens
   in the clicked column (settle-then-run pinned, no eaten click);
   (d) ghost click while another column's create field is open and
   focused → the old flow settles, the clicked column's field opens;
   (e) Ctrl+O unchanged: `press(TaskCreate)` with no selection →
   first stack; with selection → the selected stack (the existing
   journey (b) at `journeys.rs:2138-2158` extends with the no-
   selection leg); (f) the flow-table driver passes with both
   payloads (§4.iv) — every pair on-matrix; (g) the a11y ghost
   journey (`:2093`) stays green unchanged.
5. **CHANGELOG Unreleased**: one user-visible line per D1–D4.

## 6. Commit slices (stage 2; standalone, rebase-clean)

1. `fix(ui-slint): the card title fills the card to its edge`
   (D1 + the theme token + the golden regen)
2. `fix(ui-slint): a right-click outside an open overlay closes it`
   (D2, all four layers + journeys)
3. `feat(ui-slint): the column body left-click selects the stack`
   (D3)
4. `fix(ui-slint): the add-task ghost opens the create field in its
   column` (D4: payload + ctx target + ghost rework + the
   SettleThenRun gate drop + flow-table payload pass)
   — CHANGELOG updated across 1–4; order 2–4 freely permutable
   after 1.

## 7. Risks & recorded probes

- **R-A the F8 eaten click may not reproduce headlessly** (the
  driver's synthesized press/hover/release stream differs from the
  real backend's — hover ordering, the 600 ms tooltip timer).
  Mitigation: §5.4a is the probe; a green-on-main verdict is
  recorded in the stage-2 report and the Tier-4 manual checklist
  keeps the live repro. The D4 fix is normative regardless (the
  targeting defect and the dead-gate ruling stand on their own).
- **R-B the golden matrix regenerates wholesale** (title fill shifts
  every card in all 14 PNGs) — expected drift; the PNG diff is the
  review artefact; stage 3 checks the 600-width goldens still elide
  at the card edge (dispatch review item a).
- **R-C the body left-click could regress card interactions** if the
  ListView pass-through assumption shifts at some runtime — the
  existing card/drag journeys are the harness, run before the D3
  commit lands.
- **R-D the payload widens `UiEffect`'s surface** — `Copy` holds;
  every match site is compiler-exhaustive; the flow table pins both
  payloads; no wildcard arm.
- **R-E the SettleThenRun gate removal is outcome-neutral today**
  (proven by the early-return + `settle_flow`'s contract) but
  widens the re-dispatch class if a future kept-arm shifts — the
  pair driver runs every `(Flow, effect)` through the table, so a
  behaviour change cannot land silently.
- **R-F chunk-2 rebase friction** on `board-layout.slint` (the
  columns row, the DropArea constraints, the resize handle are all
  chunk-2 territory) — expected; standard rebase dance. This chunk
  touches none of those hunks.

## 8. Tier-4 checklist (manual, on the reference machine)

- **Resize-gesture feel untouched**: drag a column's right edge —
  the gesture must behave exactly as today (chunk 1 owns no width
  hunk; stage 3's grep-diff confirms zero hunks touch the
  `stack-width-*` seams).
- **Click feel**: body left-click is release-semantics like the
  header (no select-on-press flicker); the dismissal right-click
  closes on press with no menu-flash and no reopen; the ghost opens
  its field on one click with no perceptible delay; title elide
  reads at the card's edge at 0.5 and 3.0 scale, and the F8 repro
  (two/three clicks) is gone live.

## 9. Bookkeeping

- Closes on merge: nothing (F2/F6/F7/F8 are review findings, not
  GitHub issues).
- No new issues planned; stage discoveries file as `backlog:` per
  AGENTS §7.
- What opens next: chunk 2 (`feat/ui-r4-column-resize`) is gated on
  this chunk's merge (dispatch §1: "2 on 1 (board files)").
