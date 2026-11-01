# [Plan] GUI Review Round 4, Chunk 2 — Column-Resize Divider Model

- **Date**: 2026-11-01
- **Status**: READY — stage-1 plan for dispatch chunk 2
  (`feat/ui-r4-column-resize`); gate verified: chunk 1's PR #571
  merged into `origin/main` 2026-11-01T19:30Z, this branch starts at
  `4d8be313`.
- **Origin**: design round
  `.artifacts/plans/2026-11-01-gui-review-round4-plan.md` §3.2
  (D5–D8, normative), §2 decision 6 (total width == viewport; dividers
  redistribute two neighbours; proportional slack; mins-clamp + clip
  fallback), §1 finding 3, §7-R1; dispatch plan §1/§3 (acceptance
  (a)–(f), prompt 2.1).
- **Scope**: `crates/taskboard-ui-slint` only (`widths.rs`,
  `adapter.rs`, `view_state.rs`, `models.rs`, `board-layout.slint`,
  `board-view.slint`, `app-window.slint`, tests, goldens). Empty diff
  over domain/state/storage/sync/app (stage 3 greps). Non-goals per
  dispatch §3: no horizontal scrolling; no persistence-lane changes
  beyond semantics (the `StackWidthsChanged` shape and the app-side
  debounce are untouched).
- **Anchors**: all `file:line` citations are this worktree at
  `4d8be313`, read for this plan (the design round's anchors predate
  chunk 1's board-file edits and have drifted).

## 0. The current model, audited — what stays, what dies

Today (round-1 D16/D17/D18 + r2 D2): every column's `DropArea` sets
`min-width` from its stored width (or 0 when the `−1.0` auto sentinel
is pushed) plus `horizontal-stretch: 1` and `max-width: root.width`
(`board-layout.slint:294-298`). One non-scrolling `HorizontalLayout`
(spacing/padding `space-3`) stretches the columns to the row; a stored
width is a **floor**, never a pin. Growing a dragged column's
`min-width` forces the stretch solver to shrink every *other* column —
the dragged column's left edge moves while its right edge follows the
pointer (finding 3's "grows in both directions"). The
`STACK_WIDTH_MIN_PX = 160` floor (`widths.rs:15`) is content-blind.

The full inventory of what this chunk touches:

| Element | Today | Fate |
|---|---|---|
| `DropArea` sizing | conditional `min-width` + `stretch: 1` + `max-width` | **explicit `width: column.width-px`**; the three constraints die |
| Resize handle | 10 px strip inside the header, header height (`board-layout.slint:400-425`) | **re-homed full-height** at each boundary (§2) |
| `stack-width-press/drag/release` | `(int, float)/(int, float)/(int)` | same three seams, re-typed (§2) — chain `board-layout.slint:133-135` → `board-view.slint:120-122` → `app-window.slint:271-273` |
| `widths.rs` | `STACK_WIDTH_MIN_PX`, `STACK_WIDTH_DEFAULT_PX`, `STACK_WIDTH_STEP_PX`, `reclamp_width`, `clamp_width`, `width_ceiling` | rewritten (§3): step + heal stay, the rest is replaced by the row resolver + pair math |
| adapter `width_drag` buffer | `(StackId, f32)` (`adapter.rs:213`) | `(StackId, StackId, f32)` — left, right, base (§5) |
| `stack_width_ceiling[_excluding]` | `adapter.rs:745-798` | **die** — the pair clamp subsumes the ceiling (§3) |
| `view_state::resize_focused_stack` | base ± 32 against `[floor, ceiling]` (`view_state.rs:1160-1179`) | **pair step** against the neighbour (§6) |
| `models::stack_width_px` | stored × scale or `−1.0` (`models.rs:860-872`) | **dies** — the row resolver replaces it (§1, §5 sentinel) |
| `on_window_width_changed` | breakpoint only (`adapter.rs:3187-3197`) | **gains a forced refresh** (§1.4) |
| Persistence lane | release-only full snapshot, debounce app-side (`ui_requests.rs:91-114`) | **unchanged** (§7: one semantic widening, recorded) |

Everything else — the compact leg (single full-width column, no
handle), the kiosk leg (renders the regular row), the tooltip seams,
the drag-to-move `DropArea` filter — is untouched.

## 1. D5 — the explicit-width row

### 1.1 Units and the one conversion rule

All pure math is **rendered slint px** (`u32`/`i64`; the space
`window-w`, deltas and the resolver share it — the widths.rs doc's
existing rule). The persisted overlay (`ViewState::stack_widths`,
`view_state.rs:272`) stays **logical px**. Conversion happens at
exactly three seams, ×`base-scale` on the way in (models push), ÷
`base-scale` rounded on the way out (drag tick, keyboard step) — the
drag seam's existing convention (`adapter.rs:3345-3358`). This also
fixes a latent scale bug: today's keyboard verb mixes a logical base
and step with a rendered ceiling (`view_state.rs:1172-1176`); at
scale ≠ 1 the clamp bounds are in different units. The journeys run at
scale 1 so nothing pins the mixed behaviour — safe to replace.

### 1.2 The resolve pipeline (one home)

A single helper assembles the row every consumer needs
(`models.rs`, beside `push_board` — it reads `window` globals and the
view state, like the rest of the push pass):

```rust
pub(crate) struct BoardRow {
    pub stacks: Vec<StackId>,   // tables order
    pub widths: Vec<u32>,       // resolved rendered px, Σ == row_w (§3)
    pub mins: Vec<u32>,         // content minimums, rendered px (§4)
}
pub(crate) fn board_row(window: &AppWindow, state: &AppState, vs: &ViewState) -> BoardRow
```

`row_w = window_w − chrome`, with chrome **derived from the Theme
globals, not hardcoded**: `2·space-3` (the row's side padding,
`board-layout.slint:270-271`) + `n·space-3` (n columns plus the
add-column affordance ⇒ n inter-child gaps) + `(space-6 + space-2)`
(the affordance's fixed width, `board-layout.slint:647`). Recorded
correction: the old ceiling arithmetic (`adapter.rs:769`: `12·(n−1) +
40`, no side padding) was inexact on both counts; the r2 T-4 journey
(`journeys.rs:1800-1807`) already used the correct form
(`window − 4×12` for two columns + the affordance counted separately)
— this chunk makes the code match the journey's arithmetic. Reading
the Theme global lengths (`get_space_3()/1px` etc.) replaces the
hardcoded `12.0/32.0/8.0` literals; floor `row_w` at 0.

Consumers: `push_board` (models push), the drag press handler, and the
keyboard route — one assembly, three readers, no second derivation.

### 1.3 The slint binding

`board-layout.slint` columns row: the `DropArea` keeps its place in
the `HorizontalLayout` (spacing/padding unchanged) and gains

```slint
width: column.width-px;
```

`min-width`/`horizontal-stretch`/`max-width` (`:294-298`) are deleted
— stage 3 greps `horizontal-stretch` over the column code and must
find nothing (dispatch §3 review focus (b)). `StackColumnModel`
(`:62-77`) keeps the `width-px: length` field; its doc changes to
"the resolved rendered width of this column for this pass — always ≥
its content minimum, never a sentinel". The card's
`width: 100%` (`:299-300`) and chunk 1's title-fill chain are
allocation-agnostic (chunk-1 plan §1 recorded exactly this hand-off).

**Degenerate fallback** (decision 6): when Σ content-minimums exceeds
the row, `resolve_row` returns the minimums (§3 case 4), the row's
children overflow the layout, and the columns-row `HorizontalLayout`
gains `clip: true` so the overflow clips at the row instead of
painting over the affordance — the recorded "row clips" fallback, no
scrollbar.

### 1.4 Window resize must re-resolve

Under stretch, a window resize re-laid-out in-slint and the adapter's
`on_window_width_changed` (`adapter.rs:3187-3197`) only remapped the
breakpoint. Under explicit widths the pushed numbers go stale the
moment the window moves a pixel, so the handler gains a
`refresh_forced(&window)` (window width is an input outside the state
`Arc` — exactly the forced set's charter, `adapter.rs:648-650`). The
`refresh()` `Arc::ptr_eq` guard cannot see width changes; the forced
path is mandatory, not an optimisation choice. Cost: one forced diff
pass per width-change event during a live window drag; the diff only
carries `width_px` scalars (the persistent-handles machinery,
`models.rs:918-924`, fires per changed scalar). Accepted; recorded as
risk §11.1 with the kiosk note (kiosk windows do not resize in
service). The null-platform guard (`width <= 0` ignored) stays.

## 2. D6 — the divider element

**Placement — full-height, not header-only.** The handle leaves the
header (`board-layout.slint:400-425`) and becomes a direct child of
the `DropArea`, declared *after* `card` so it hit-tests first. It
spans the column's full height (the Excel/Calc, Explorer/Dolphin,
Qt tableview hit model — the user's citation) and straddles the
boundary: `x: parent.width − self.width/2`, `width:
Theme.resize-handle-width` (10 px, token stays), `height: 100%`. With
the 12 px `space-3` gap this puts 5 px over the dragged column's edge,
5 px in the gap, and leaves the neighbour's remaining 7 px (gap
+ its 5 px strip belongs to the *next* boundary's handle — one handle
per boundary, `n−1` handles, none after the last column:
`if (column-index < root.stacks.length − 1)`).

**Recorded trade**: the handle's 5 px strip overlays the card
surface's right edge, so card clicks/context on that strip start a
resize instead (the body-hit/cards are hit-tested *under* it). This is
the standard table-editor behaviour and is preferred over a
header-only handle, which forces pixel-precise aiming at the header
row — the exact "hard to grab" class finding 3 complains about. The
`can-drop` filter forwarding (D-H, `:281-286`) is untouched; a child
`TouchArea` hit-tests before the `DropArea` parent, so card
drag-to-move keeps working everywhere else.

**Affordance**: `mouse-cursor: ew-resize` (carried over) plus a
visible cue — a 2 px vertical accent line inside the strip:
`Semantic.focus-ring` at hover, full opacity while `pressed` (the
D15 focus-ring token reuse precedent; no new theme token). No
tooltip and no tab stop on the divider — recorded deviation from the
blanket interactive-element rule with rationale: chunk 2 is not in
dispatch §1's blanket list, and the operation is fully keyboard-owned
through `stack.wider`/`stack.narrower` on the focused column (§6);
the divider is a pointer accelerator over an existing accessible verb,
like the tableview pattern it imitates.

**Callbacks — the boundary is addressed by its left column.** The
three seams keep their names and chain positions, re-typed:

- `stack-width-press(int left-index)` — the `float base` parameter
  dies: Rust resolves the row itself (§1.2), and press additionally
  **materializes** the resolved widths into the view-state overlay
  (§7.2) so the gesture runs on a total, sum-exact basis.
- `stack-width-drag(int left-index, float delta)` — delta-since-press
  (idempotent-against-dropped-events rule carried over).
- `stack-width-release(int left-index)` — unchanged semantics: one
  full-snapshot request, release only.

The adapter buffer keys `(left_stack, right_stack, base_left_f32)` by
StackIds; the stale-index guard (`adapter.rs:3336-3341`) carries over
verbatim for the left index (a mid-drag tables reorder is absorbed;
the right StackId rides in the buffer and needs no re-check).

## 3. The redistribution math — `widths.rs` rewrite

The module stays pure, slint-free, proptestable (its doc's contract).
New API (rendered px in/out; the names are the stage-2 contract):

```rust
pub const STACK_WIDTH_READABILITY_MIN_PX: u32 = 120;  // §4, replaces 160
pub const STACK_WIDTH_STEP_PX: u32 = 32;              // kept, logical

/// The row resolver: rendered widths that satisfy Σ == row_w and
/// w_i ≥ mins[i] (when Σ mins ≤ row_w; else every w_i = mins[i] and
/// the row clips — decision 6's fallback).
pub fn resolve_row(row_w: u32, stored: &[Option<u32>], mins: &[u32]) -> Vec<u32>;

/// The divider drag: move the boundary between `boundary` (left) and
/// `boundary + 1` (right) by `delta` (left takes +delta). Sum- and
/// others-exact; clamps at both minimums (the boundary sticks).
pub fn drag_boundary(widths: &[u32], boundary: usize, delta: i64, mins: &[u32]) -> Vec<u32>;

/// The keyboard sibling: ±step on column `index` against its right
/// neighbour, or its left neighbour when `index` is the last column.
pub fn step_column(widths: &[u32], index: usize, direction: i32, step: i64, mins: &[u32]) -> Vec<u32>;

/// The load-time heal (kept, new floor).
pub fn reclamp_width(px: u32) -> u32;

/// The content minimum (§4).
pub fn content_min_px(title_chars: usize, base_scale: f64) -> u32;
```

Dying: `STACK_WIDTH_MIN_PX` (160), `STACK_WIDTH_DEFAULT_PX`
(equal-share seeding supersedes the keyboard seed — §6),
`clamp_width`, `width_ceiling` (the pair clamp subsumes the ceiling:
`left + δ ≤ left + right − min_right` bounds the dragged column by the
*neighbour's* minimum with the rest of the row untouched, which is the
stronger, local guarantee the ceiling approximated globally).
`STACK_WIDTH_STEP_PX` keeps its name, value and "logical px" meaning.

**`resolve_row` — the enumerated cases:**

1. `n == 0` → empty vec. `n == 1` → `[row_w]` (clamped at `mins[0]` —
   a single column takes the row).
2. **Seed**: an absent entry (`None` — never touched) seeds at the
   equal share `row_w / n` (integer floor; the fit repairs rounding).
   A never-resized board resolves to equal shares — the rendered
   image of today's `stretch: 1` distribution.
3. **Fit (surplus, Σ seed < row_w)**: distribute `d = row_w − Σ`
   proportionally to current widths, largest-remainder rounding
   (deterministic tie-break by index) so Σ out == row_w exactly.
   No clamps can engage (widths only grow).
4. **Fit (deficit, Σ seed > row_w)**: take `−d` proportionally, but
   no column goes below its minimum — water-filling: clamp at-min
   columns, redistribute their un-takeable share over the columns
   with slack, iterate (≤ n rounds, bounded). Degenerate Σ mins >
   row_w: every column = its minimum; Σ > row_w; the row clips (§1.3).
5. **Identity**: `Σ stored == row_w` (the post-gesture steady state,
   §7.2) resolves to the input unchanged — the stability property the
   drag anchoring rides on.

**`drag_boundary`** (the load-bearing invariant set): δ clamped into
`[min_left − left, right − min_right]`; out = input with
`(left + δ, right − δ)`, everything else bit-identical. Sum-exact by
construction (one i64 subtraction); no-op at exact minimum (the
dispatch's property); "anchored" = within the clamp the boundary
tracks the pointer exactly (acceptance (f)); past the clamp it sticks
— Excel behaviour, and the hover line going full-opacity while
`pressed` makes the stick legible.

**`step_column`** — the case table (§6): wider with a right neighbour
= `drag_boundary(b = index, +step)`; wider at the last column =
`drag_boundary(b = index − 1, −step)` (grow me by shrinking my left
neighbour); narrower mirrors both; `n == 1` → identity (no boundary
exists — the handle conditional already hides it).

**Property suite** (replaces the existing three proptests; the heal
proptest carries over against the new floor):

- `resolve_row`: Σ == row_w whenever Σ mins ≤ row_w; every w ≥ min;
  output-stable (same input twice → same vec); proportional ±2 for
  any two unclamped columns; degenerate returns the mins exactly;
  absent-seed determinism (seeded shares never depend on *which*
  entries are absent).
- `drag_boundary`: Σ preserved for any δ (−10⁶..10⁶), any mins; only
  indices `b, b+1` differ; monotone in δ; double-clamp idempotent
  (δ past the min twice == once); clamp-then-reverse returns the
  exact input.
- `step_column`: last-column equivalence
  (`step_column(last, +1) == drag_boundary(n−2, −step)`); `n == 1`
  identity.
- `content_min_px`: ≥ floor·scale; non-decreasing in `title_chars`;
  monotone in scale.

## 4. D7 — the content-minimum formula

`content_min_px(title_chars, base_scale) =
max(READABILITY·scale, ceil(title_chars × HEADING_CHAR · scale) +
HEADER_CHROME · scale)` with three named constants:

- **`STACK_WIDTH_READABILITY_MIN_PX = 120`** (logical) — the floor,
  replacing 160. Derivation, not taste: it is the task card's own
  min-content width — card padding `2·space-2` (16) + done checkbox
  (16) + row spacing (`space-1`, 4) + chunk 1's
  `Theme.card-title-min-width` (84, `theme.slint:70`) = 120. A column
  narrower than this would force chunk 1's title-fill chain to
  overflow the card it lives in; the floor is exactly where card and
  column agree. In the design's expected 96–120 band, taken at the
  top.
- **`HEADING_CHAR_WIDTH_PX: f64 = 9.0`** (logical) — the header
  title's average glyph advance. `Typography.heading` is 16 px weight
  600 (`theme.slint:105`); ~0.56 em average for a humanist sans at
  weight 600 ⇒ ~9 px.
- **`HEADER_CHROME_PX: u32 = 64`** (logical) — header inner padding
  `2·space-2` (16) + count-badge reserve (32: two digits at badge
  proportions) + the card body's `2·space-2` (16).

So a short header (`"a"`, `"b"` — the golden fixture) floors at
`max(120, 3×9+64=91) = 120` **— the acceptance-(b) unit test's
"demonstrably smaller than today's 160"** — and a 20-character header
resolves `max(120, 180+64) = 244`, growing the minimum so the title
never starts elided-by-minimum.

**Measurement source — recorded decision.** The estimator is
Rust-side because the *reconcile* needs the mins (water-filling, the
degenerate case, the pair clamp): a slint-side measurement (a hidden
no-elide `Text`'s preferred width, or pushing measured mins back to
Rust through a callback) is either unavailable to the resolver or
one-pass-late/stale across font and scale changes — rejected. The
estimator is a documented approximation: underestimate ⇒ a wide-glyph
title elides a few characters at its minimum (the header already
`clip: true` + `overflow: elide`, `board-layout.slint:371,471` —
graceful); overestimate ⇒ a column slightly wider than its content
could be (harmless). The drag path and the keyboard path read the
same mins, so clamping is consistent everywhere. Alternative recorded:
slint-side `min-width` measurement with a Rust floor-only reconcile —
rejected because the explicit-width row would then overflow silently
whenever the content minimum binds during a *drag* (the clamp the
user feels must match the clamp the resolver applies).

**Slack distribution on stack add/remove** (decision 6, acceptance
(c)): a new stack arrives with no stored entry → case 2 seeds it at
the current equal share, the fit distributes the (negative) residual
proportionally over all columns — the others make room, the new
column joins at roughly their average. Removal is the mirror: the
row re-fits over the survivors; nothing is persisted for the removed
StackId (liveness pruning at write time, unchanged). Both directions
are covered by `resolve_row` properties (any `n`, any `stored`
pattern) plus the §9 journey.

## 5. The auto-sentinel's fate — KILLED (recorded)

The `−1.0` sentinel (`models.rs:866-871`, "≤ 0 = auto",
`board-layout.slint:71-76`) is **removed**. Rationale: the new model
has no "unresized" rendering state to express — every column renders
an explicit resolved width every pass, and a never-touched board's
equal share *is* its resolved width (case 2). Keeping the sentinel
would keep the stretch machinery alive for auto columns inside the
same row as pinned ones, violating D5 ("the stretch machinery dies on
this row") and making a divider between an auto and a pinned column
semantically undefined. Redefining it (materialize every absent entry
at mount) was the recorded alternative and is rejected on two
grounds: it bakes mount-time geometry into the overlay before any
user gesture, and it fattens the first persistence snapshot beyond
what the user expressed. The `never_resized_board_pushes_auto_widths_
and_no_requests` journey adapts to assert the new contract: every
column pushes a positive width, equal shares within rounding, and
refreshes still never persist. `arm_width_drag`
(`adapter.rs:907-909`, the flow-table test seam at
`flow_table_tests.rs:1048`) re-shapes with the buffer tuple.

## 6. D8 — the keyboard sibling

`StackWider`/`StackNarrower` keep their route
(`adapter.rs:1084-1093` → `view_state.rs:1160-1179`) and their target
resolution (focused → selected → first, `keyboard_resize_target`,
`view_state.rs:1147-1151`). The verb's body becomes: resolve the
board row (§1.2), find the target's index, `step_column(widths,
index, direction, STEP·scale, mins)` (§3), write back the two changed
columns as logical px (÷ scale, rounded), return
`[StackWidthsChanged, RefreshNow]` as today. The ceiling parameters
and the `STACK_WIDTH_DEFAULT_PX` seed die: the target always has a
resolved width, so "first step on a never-touched board" is now
`equal_share ± 32` — the existing journey's `280 + 32` expectation
(`journeys.rs:1636`) updates to the fixture's equal share ± 32
(adapted, not weakened: it still pins exact step arithmetic, exact
floor clamping, and exactly one typed snapshot per press).

**Last-column edge** (the dispatch's named case): `wider` on the last
column drags the boundary to its *left* (steal from the left
neighbour); `narrower` pushes that neighbour wider. Both via
`step_column`'s case table, property-pinned (§3). `n == 1`: the verb
is a no-op returning no outcomes (no boundary exists to move;
`keyboard_resize_target` still resolves, `step_column` returns the
input). Compact leg: the verb still runs (state-level) but nothing
visible depends on widths there — status quo, untouched.

## 7. Persisted widths — migration, materialization, lane

### 7.1 The heal (existing persisted widths)

`seed_stack_widths` (`view_state.rs:1099-1112`) keeps its single
load-point heal, now against 120: every persisted width is ≥ 160
today (the old `reclamp_width` guaranteed it), so the new floor makes
the heal a near-no-op for existing data — **no migration, no config
rewrite**, and the "heals reach config only on the next drag/step"
rule carries over. The *semantics* of a stored value change by
design (recorded): it was a floor under stretch; it is now the
column's basis width under proportional fit. A user's old 160-wide
column keeps its basis and re-spaces with the row — exactly decision
6's model; no legacy value can violate the new floor, and the heal
proptest (idempotent, monotone, floored) re-pins the boundary for
hand-edited or downlevel config.

### 7.2 Press materialization (the anchoring guarantee)

At `stack-width-press`, the handler materializes: for every live
stack of the board, `set_stack_width(stack, round(resolved/scale))`.
View-state only — nothing is sent (release still sends; refreshes
still never persist). From that moment `stored` is total and
Σ stored·scale == row_w (case 5 identity), so every drag tick is the
pure pair edit — **untouched columns cannot wiggle mid-gesture and
the released boundary cannot jump when the next derive re-fits the
row**, which is acceptance (f) by construction. Without this, a
sparse overlay (absent neighbours seeding at equal share) would
re-fit the residual against the dragged pair and shift
non-adjacent columns during the gesture. The keyboard step gets the
same materialize-then-step treatment (one code path: materialize,
resolve, step, write back).

### 7.3 The lane (semantic-only change, recorded)

`StackWidthsChanged` keeps its shape (board + `Vec<(StackId, u32)>`
full snapshot, `BTreeMap` order, replace semantics app-side,
`ui_requests.rs:91-114,121-131` — untouched, existing debounce tests
green). One recorded widening: because press materializes, a board's
*first* gesture persists widths for **all** of its live stacks, not
only the dragged one — the snapshot is the honest total state (and
matches the lane's full-snapshot contract). Downlevel configs
(widens-less) load fine: absent entries seed at equal share (case 2).
Unknown boards/stacks kept, write-time liveness pruning: unchanged.

## 8. The probe — gesture feel

Stage 1 has no reference-machine access; per dispatch prompt 2.1 the
fallback applies: **Tier-4 checklist** (§12), executed by the
stage-2/stage-3 agents or the user on the reference machine. The
headless journeys pin the *mechanics* (anchoring, clamps, isolation,
proportionality); the Tier-4 pass judges the *feel* (stick at the
min, hover-line legibility, strip-steal acceptability, keyboard
rhythm at 0.5/3.0 scale). No probe commit is staged in stage 1.

## 9. Testing catalogue (TDD-first; red where the behaviour is new)

**Unit — `widths.rs` (rewrite the module's test block):**

- `content_min_px`: short header floors at 120 (< 160 — acceptance
  (b)); grows with `title_chars`; scales with `base_scale`.
- `resolve_row` table: identity / surplus / deficit with slack /
  deficit into the mins / degenerate (Σ mins > row_w) / absent-seed /
  n = 0 / n = 1.
- `drag_boundary` table: exact move both directions; clamp at left
  min; clamp at right min; no-op at exact minimum; sum and others
  exact.
- `step_column` table: right pair both directions; last-column left
  pair; n = 1 identity; step into both mins.
- `reclamp_width`: carried over, new floor.

**Property — `widths.rs` proptests:** the §3 invariant set (sum
preservation, mins, stability, proportionality ±2, degenerate = mins,
last-column equivalence, double-clamp idempotence, heal idempotence).

**View-state unit tests** (`view_state.rs` tests ~3556-3660, adapted
to the verb's new inputs): exact ±32 arithmetic at scale 1; floor
clamp; the snapshot outcome pair `[StackWidthsChanged, RefreshNow]`;
seed-heal at 120.

**Journeys** (`tests/journeys.rs`):

1. `keyboard_resize_steps_clamps_and_requests_persistence` — adapted
   (§6): equal-share seed, exact steps, floor clamp, one request per
   press.
2. `drag_seam_resizes_live_and_persists_on_release` — adapted: press
   without base, live re-render, stale tick absorbed, release-only
   single request; **extended**: the right neighbour's width moves by
   exactly −δ and the snapshot carries both columns.
3. NEW `divider_drag_moves_exactly_two_neighbouring_columns` —
   acceptance (a): a **3-stack** fixture (new
   `golden_state_with_stacks(n)` helper in `fixture.rs`; the current
   fixtures hardcode two stacks, `fixture.rs:269-283`); drag the
   middle boundary; assert columns 0/1 change and column 2 is
   bit-stable (±1 px rounding).
4. NEW `window_resize_distributes_delta_proportionally` — acceptance
   (c): drag col 0 narrow, enlarge the window, every column grows and
   Σ + chrome fills the row (the corrected chrome arithmetic of §1.2
   pinned); replaces `stored_widths_fill_the_window_as_floors`
   (floors semantics are gone; the journey's fill-the-row arithmetic
   is inherited, not weakened).
5. NEW `stored_widths_rebalance_when_a_stack_joins` — mount a
   2-stack board with dragged widths, remount with a third stack
   seeded (the §4 add case at the integration layer): all columns
   re-fit proportionally, the new column lands near the average, no
   request fires without a gesture.
6. `never_resized_board_pushes_auto_widths_and_no_requests` →
   renamed/adapted (§5): positive equal shares, zero requests on
   refresh.

**Structural/other:** `flow_table_tests.rs` width-buffer arm update;
grep pins: no `horizontal-stretch` in the column row, no
`STACK_WIDTH_MIN_PX`/`clamp_width`/`width_ceiling`/`-1.0` sentinel
residue (dispatch §3 review focus (b)/(c)); goldens regenerate —
`board-0.5-1000/1500`, `board-3.0-1000/1500`, `kiosk-board-0.5-1000`:
the auto case must be pixel-near-identical to `main` (equal shares ≙
stretch-equal), and any drift beyond ±1 px rounding is a finding, not
a silent regen (risk §11.4).

## 10. Commit slices (stage 2; standalone, rebase-clean)

1. `feat(ui-slint): the column row resolver (pure, property-tested)`
   — widths.rs rewrite + the full unit/proptest suite + the new
   floor/heal.
2. `feat(ui-slint): the divider handle owns the explicit-width row` —
   slint: handle re-home + affordance, `width: column.width-px`,
   stretch/max-width/min-width removal, callback re-typing through
   the chain, row `clip: true`.
3. `feat(ui-slint): press materializes the row; drag moves the pair`
   — adapter seams, `BoardRow` assembly in models, window-width
   forced refresh, ceiling fns removed.
4. `feat(ui-slint): the keyboard step joins the pair model` —
   view_state verb reshape + flow-table seam.
5. `test(ui-slint): divider journeys and regenerated goldens` (+ the
   CHANGELOG Unreleased rows: Added — divider handles between board
   columns; Changed — column widths redistribute between neighbours
   and re-space proportionally on window/stack changes).

## 11. Risks

1. **Window-resize refresh cadence** (§1.4): one forced diff pass per
   width event; bounded by scalar-only diffs, and the unchanged-
   refresh guard's fast path is deliberately bypassed (width is
   invisible to it). Kiosk windows are static in service. If the
   perf lane (#463/#464) ever flags it, the fallback is reconciling
   on breakpoint crossings + a trailing debounce — recorded, not
   built.
2. **Rounding drift** between logical stored and rendered resolved
   (±1 px per ÷scale round-trip): gestures are exact within a
   session (base + absolute delta); a remount can show a boundary
   ±1 px from where release left it. Accepted; largest-remainder fit
   keeps Σ exact so no drift accumulates.
3. **The header-min estimator vs true text width** (§4): wide-glyph
   titles can elide at their column's minimum. Graceful (clip +
   elide exist); the floor dominates the common short-title case.
4. **Golden drift in the auto case** (§9): stretch-equal and
   equal-share must render the same image; a beyond-rounding drift
   means the chrome arithmetic (§1.2) or the fit disagrees with the
   layout — a finding to fix on the branch, never a regen to hide it.
5. **The 5 px strip over cards** (§2): card clicks in the strip start
   a resize. Excel-parity trade, recorded; the Tier-4 pass vetoes if
   it reads as stealing.
6. **Rebase friction**: none against chunk 1 (merged before this
   branch); `models.rs` collisions with the backlog render lane
   (2.1/4.1) are the expected standard-dance class if those land
   mid-chunk.

## 12. Tier-4 checklist (manual, reference machine)

- Drag the middle divider of ≥ 3 columns: the boundary tracks the
  pointer 1:1; both edges of every other column hold still; the
  boundary sticks at either minimum and the hover line reads it.
- Drag into the min hard and release, then drag the same boundary the
  other way: no jump, symmetric recovery.
- Hover the divider: cursor + accent line; the line holds while
  pressed; gone on release.
- A click on a card's rightmost ~5 px starts a resize (accepted
  trade) — confirm it does not read as a stolen click.
- Keyboard: focus a middle column, `stack.wider` ×3, `stack.narrower`
  ×5 — neighbour seesaws, floor clamp, exactly one persist per press
  (status line / next-mount width). Repeat on the last column (left
  neighbour) and on a 1-column board (inert).
- Resize the window across breakpoints: columns re-space
  proportionally, nothing overflows at 320 px with long headers
  except the recorded mins-clamp + clip case.
- Scale extremes 0.5 / 3.0: handle grab size, hover line, cursor,
  keyboard rhythm all coherent (rendered math).

## 13. Bookkeeping

- No round issues close via this chunk; nothing to post.
- CHANGELOG Unreleased rows land in stage 2's final slice (§10.5).
- If the Tier-4 pass surfaces a feel finding (strip width, stick
  behaviour, hover token), it is a follow-up commit on this branch or
  a `backlog:` issue — never a silent plan deviation.
- Plan committed as `docs(plan): divider column resizing`; no
  implementation, no push (dispatch §1 stage-1 contract).
