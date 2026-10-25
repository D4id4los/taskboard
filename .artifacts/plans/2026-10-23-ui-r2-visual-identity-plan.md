# [Plan] GUI Review Round 2 — Chunk 7: Visual identity — stack cards & board tabs (D35–D37)

- **Date**: 2026-10-25
- **Kind**: stage-1 chunk plan (paint-only; TDD-first adapted to a
  paint chunk — baseline-green proof first, goldens as the reviewed
  acceptance artefacts; no implementation in this commit). Branch
  `feat/ui-r2-visual-identity`, cut at `main` `abc5aa7` (dispatch §1
  precondition verified: chunk 1 `feat/ui-r2-board-surface` — golden
  regen `addda00` — and chunk 4 `feat/ui-r2-taskview-textfields` —
  changelog `deffcca` — are both ancestors of the tip; chunks 2, 3, 5,
  6 landed between them per the serial default). Worktree created with
  `git worktree add -b` after a green `scripts/worktree-doctor.sh`
  (101G free / 25G floor).
- **Origin** (normative): design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §8
  (D35–D37), §1.1 (F-A9/F-A10), §1.0 finding 32 (F32), §10 (testing
  strategy, goldens rule), §14-R2 (Slint layout defaults fight back);
  dispatch plan
  `.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`
  §1 (shared execution model + code-quality mandate) and §8 (this
  chunk's acceptance + prompts 7.1/7.2/7.3).
- **Non-goals** (dispatch §8): zero Rust diffs (an `adapter.rs`/`models.rs`
  diff is a finding — stage 2 stops and records); every journey from
  chunks 1–6 passes **unchanged** — the acceptance is the unchanged
  journey suite; no tooltip/menu paint; no settings paint; the #411
  dropdown/many-board scalability redesign stays deferred; the compact
  switcher's paint is untouched (D-10).

## 0. Decisions index (the dispatch prompt's six settlements)

| # | Decision | One-line form |
|---|---|---|
| D-1/D-2 | Token ladder | Existing 4-step ladder, re-anchored: `background` (board strip) < `surface` (**column card**) < `surface-raised` (**task card**, moved up from `surface`) < `surface-hover` (hover). **No new colour token**; recorded deviation from D35's named card token (§2.1). |
| D-3 | Badge on the ladder | `Badge` neutral tone `surface` → `surface-raised` (header count chip must read on the card). |
| D-4 | New dimension token | `Theme.hairline-width: 1px·scale` — baseline rule height + inactive-tab border width. The only theme addition; no colour ⇒ `token_tests.rs` byte-identical. |
| D-6 | Gutter | **Keep `space-3`** (D35's default); bump to `space-4` only if the stage-2 eyeball probe says cramped — that fallback carries the one permitted non-golden test edit (the glyph test's `+12.0`). |
| D-7/D-8/D-9 | Card anatomy | The regular leg's per-column `column-body` wraps in a `Rectangle` card: `surface` bg, `radius-medium`, `space-2` inner padding, no border, no clip; columns row gains `space-3` side/bottom padding (the visible strip). Header, task cards, ghost row keep every token binding — they re-read against the new context. |
| D-12–D-15 | Tab strip anatomy | Baseline rule (`Theme.border`, `hairline-width`) under the tabs; active tab = full-height `Theme.background` paint, top-corner-only `radius-medium`, no border — covers the rule (it breaks) and merges into the board background; inactive tabs = `hairline-width` `Theme.border` marker, paint box ending on the rule, transparent bg, `surface-hover` lift; focus swaps the frame to `focus-ring`/`focus-ring-width` on both states. Max-width 160·scale, elide, tooltips, D20b height, tab-list/tab roles: untouched. |
| §5 | Golden regen discipline | ONE dedicated commit (`chore(ui)`), all 12 matrix PNGs listed with their visible diffs in the body; matrix and test file unchanged. |
| §6 | Tier-4 checklist | Tab look at min viewport; card contrast on the user's monitor; hover/focus walk. |

## 1. Verified facts (anchored; line numbers are post-chunk-6 `main` `abc5aa7`)

- **F-1 Column chrome does not exist (F-A9 restated).** Columns are
  bare `VerticalLayout`s in a `HorizontalLayout` with `spacing:
  Theme.space-3` (`ui/compositions/board-layout.slint:147-149`); the
  only painted surfaces are the at-rest-transparent header
  (background `board-layout.slint:253-256`) and the task cards; the
  gutters are raw window background `Theme.background` (#14161c,
  `ui/app-window.slint:88`). The F32 redesign starts from a clean
  slate — nothing to preserve.
- **F-2 Board-bar segments (F-A10 restated).** Each segment is a
  `FocusScope` wrapping a paint `Rectangle`: `radius-small`,
  hover → `surface-hover` / selected → `surface-raised` / rest
  transparent, focus ring as the only border
  (`ui/compositions/board-bar.slint:56-64`), 160·scale max-width +
  elide (54, 78), full tooltip wiring (82-97), right-click (99-106),
  D20b locked height `space-6 + 2·space-1` (39). The F32 restyle
  alters paint only — every seam around the paint is load-bearing
  and stays.
- **F-3 The surface token map (usage survey, all of `ui/`).**
  `background` #14161c: window shell, done task cards, status-bar
  wells. `surface` #1d2028: **task card at-rest**
  (`ui/widgets/task-card.slint:56-58`), toolbar, status bar,
  segmented-control track, kbd-hint chips, neutral badge
  (`ui/widgets/badge.slint:13`), action-menu page-cursor row,
  settings rows. `surface-raised` #262a35: **text-field**
  (`ui/widgets/text-field.slint:38`), action-menu/search-palette/
  capture-overlay/tooltip panels, segmented-control selected thumb
  (`ui/widgets/segmented-control.slint:45,56`), slider fill, board
  selected-segment/selected-header paints. `surface-hover` #303545:
  every hover. The board's existing container language is literally
  the segmented control's: `surface` track, `surface-raised` thumb.
- **F-4 The colour-token lock.** `src/token_tests.rs` extracts every
  `out property <color>` from `ui/theme.slint` and enforces: the
  **closed 14-token set** (`extractor_finds_every_declared_theme_token`
  asserts exact membership *and* count, :282-286), the WCAG PAIRS
  table with role-class minimums (:146-179, :327-352), **pair-or-
  exemption completeness** for every declared colour token
  (:354-368), the hover distinctness guard against parents
  `[background, surface, surface-raised]` (:289-317). Adding any
  colour token ⇒ three coordinated test edits; changing no colour ⇒
  the file is byte-identical.
- **F-5 The geometry pixel pins that constrain the gutter.**
  `the_add_stack_glyph_is_square` (`tests/journeys.rs:1559-1618`)
  computes the add-column strip's left edge as
  `header_b.x + header_b.width + 12.0` — the `space-3` gutter as a
  literal (:1576) — and classifies "background" pixels as
  `#14161c ± 12` (:1597-1600) inside a scan band confined to the
  40-px-wide strip. A `space-4` gutter bumps the literal; a card
  painted under the scan band would poison the bounding box. With
  the gutter kept and the strip outside the cards, the test is
  untouched.
- **F-6 TaskCard is shared.** `task-card.slint` renders in the board
  regular leg (`board-layout.slint:307`), the compact leg (:505),
  stack view (`ui/views/stack-view.slint:57`), and today view
  (`ui/views/today-view.slint:57`). Its at-rest token swap
  re-surfaces all four, so the **today golden series diffs too** —
  6 of the matrix's 12 PNGs, not just the board's 6.
- **F-7 The bar-to-content gap is background-coloured.** The regular
  leg is `VerticalLayout { spacing: Theme.space-1 }` (BoardBar, then
  the columns row; `board-layout.slint:127-147`) with no
  intermediate paint — an active tab painted `Theme.background`
  merges seamlessly across the gap.
- **F-8 Slint 1.18.1 supports per-corner radii (R2 probe answered
  statically).** The pinned compiler's builtin `Rectangle` carries
  `border-top-left-radius` / `border-top-right-radius` /
  `border-bottom-left-radius` / `border-bottom-right-radius`
  (`i-slint-compiler-1.18.1/builtin_elements.rs:628-636`, "To target
  specific corners with different values use the following
  properties"); the pin is exact (`=1.18.1`, guarded by
  `tests/pin_guard.rs`). Rectangle borders remain uniform — there is
  no per-side border-width — so an "open-bottom" inactive tab must
  be built by sizing its paint box, not by omitting one border side.
- **F-9 The control-row constant is triplicated inline.**
  `space-6 + 2·space-1` appears as the board-bar height
  (`board-bar.slint:39`), the header height
  (`board-layout.slint:189-190`; chunk 1's comment there explicitly
  deferred the token decision to "chunk 7's ladder"), and the ghost
  rows (:329-330 regular, :525-526 compact).
- **F-10 The golden matrix.** 12 PNGs in `tests/goldens/`: `board-*`
  and `today-*` × scale {0.5, 3.0} × width {600, 1000, 1500}, height
  900. 600 ⇒ the compact leg (board bar absent, stack switcher
  present); 1000/1500 ⇒ regular/expansive (bar + columns row).
  Regen is env-gated (`TASKBOARD_REGEN_GOLDENS=1`) and **self-fails**
  the regen run (`tests/golden_visuals.rs:87-90, 165-178`); CI only
  compares; the PR's PNG diff is the review artefact.
- **F-11 The compact leg has no card and needs none.** The compact
  breakpoint renders the switcher + one full-width column
  (`board-layout.slint:408-548`); dispatch §8 review item (c) asks
  only that the compact switcher "still reads correctly against the
  new tokens".
- **F-12 The new-colour-token path is arithmetically boxed (the
  ladder's decision driver; WCAG math per `token_tests.rs`, computed
  2026-10-25).** Luminances: bg 0.00805, surface 0.01452, raised
  0.02328, hover 0.03603; the guard's step `surface→raised` = 1.136.
  A new card token X between raised and hover, once added to the
  hover guard's parent list (hover paints over task cards), must
  satisfy both `raised→X ≥ 1.136` ⇒ L(X) ≥ 0.03325 and
  `X→hover ≥ 1.136` ⇒ L(X) ≤ 0.02573 — **contradictory**. Concretely,
  X = #2f3440 clears raised→X (1.151) but fails X→hover (≈1.02).
  Keeping hover at #303545, no perceptible raised-again step exists;
  a perceptible one forces an app-wide hover re-tune (L(hover) ≥
  0.0445, ≈ +24 % luminance) plus the F-4 test edits.

## 2. D35 — Stack columns as cards

### 2.1 The token ladder (decision D-1/D-2; the recorded deviation)

**The column card paints `Theme.surface`, not `Theme.surface-raised`
as D35's first clause names.** The ladder is the existing four steps,
re-anchored:

| Step | Token | Paints after this chunk |
|---|---|---|
| board strip | `background` #14161c | the visible strip around/between/below cards; window shell (unchanged) |
| container | `surface` #1d2028 | **the column card** (new use); toolbar, status bar, tracks, chips (unchanged) |
| content | `surface-raised` #262a35 | **the task card** (moved up from `surface`); text-fields, panels, selected paints (unchanged) |
| lift | `surface-hover` #303545 | every hover (unchanged) |

Why the deviation (the reason dispatch §1 requires, recorded):

1. **The container reading D35 asks for already exists one step
   lower.** Task cards on `surface-raised` step up from their
   `surface` container by 1.136 — numerically the same step the old
   cards took off the bare board (1.110) — and it is exactly the
   segmented control's established track/thumb language (F-3).
   Cards read as containers; nothing else in the app changes.
2. **D35's literal path is boxed by the hover invariant (F-12).**
   Card = `surface-raised` + a new raised-again token for task cards
   has no valid value under the current hover; the escape is a global
   hover re-tune plus three coordinated `token_tests.rs` edits —
   palette churn in every hover surface in the app and a violation
   of dispatch acceptance (d)'s "ONLY golden-snapshot updates as
   allowed test diffs".
3. **Done cards improve.** A done card paints `Theme.background`
   (`task-card.slint:57`) — inside a `surface` card it becomes a
   recessed well; on today's bare board the same paint is nearly
   invisible.

D35's mechanism clause ("task cards on `surface-raised` switch to a
raised-again token") is the conditional machinery attached to its
named card token; taking the other existing step achieves the same
intent — "cards read as containers" — with a closed token set. The
swap itself is one line: `task-card.slint:56-58` at-rest arm
`Theme.surface` → `Theme.surface-raised` (hover and done arms
untouched). It flows through all four TaskCard call sites (F-6).

**Consequences audited**: text on the new card surface is locked
PAIRS already (text-primary 11.92:1, text-secondary 6.27:1 on
raised); due-tone text on the re-stepped card surface stays above
the 4.5 body floor (§4). Text-fields (raised) inside the card
(surface) sit one step up — the create/rename fields remain visible
transiently, delineated by their focus rings. `kbd-hint` chips
(surface) only ever render inside raised panels — unchanged.

### 2.2 Badge on the ladder (D-3)

The header's task-count `Badge` (neutral tone, `badge.slint:13`) is
`surface` — it would dissolve into the `surface` card. **Neutral tone
→ `Theme.surface-raised`.** Audited against every other Badge call
site: status-bar badges are all translucent semantic tones
(`status-bar.slint:148-217`) — unaffected; the toolbar's inbox badge
takes neutral only at count 0 (`toolbar.slint:59`), where it blends
into the `surface` toolbar **today** — the bump un-blends it, a
recorded latent fix, golden-arbitrated. No PAIRS impact: the paint
moves between two existing tokens (text-secondary on raised is
locked, 6.27:1).

### 2.3 Card anatomy (D-7/D-8/D-9)

- **Wrap target**: the regular leg's per-column
  `column-body := VerticalLayout` (`board-layout.slint:149`) becomes
  the child of a `card := Rectangle` — `background: Theme.surface`,
  `border-radius: Theme.radius-medium`, `padding: Theme.space-2` on
  all sides, **no border, no `clip`** (padding insets everything, so
  nothing reaches the rounded corners; the header's hover/selected
  box is an inset rounded chip inside the card, not a full-bleed
  strip).
- **The width-floor bindings move with the wrap**: `min-width` (the
  user-width floor, :160-162), `horizontal-stretch: 1` (:163), and
  `max-width: root.width` (:168) transfer from `column-body` to
  `card` — the card is the row's layout child now. The a11y list
  role/label stay on `column-body` (the card is an unnamed wrapper;
  the accessible tree shape is unchanged for the driver and ATs).
- **The visible strip**: the columns row `HorizontalLayout`
  (:147-148) gains `padding-left/right/bottom: Theme.space-3` (top
  stays 0 — the tab strip's baseline defines the board's top edge;
  §2.4 gives the row breathing room below the bar). Spacing between
  cards stays `space-3` (D-6). Card inner padding (`space-2`) is
  deliberately smaller than the outer strip (`space-3`) — content
  groups tighter than the group itself.
- **Header (D-8)**: every binding unchanged — rest transparent
  (reads as card surface), hover `surface-hover`, selected and
  move-target `surface-raised` + focus-ring border (:253-260). On
  the card these read as: title row, lift, one-step-up + ring.
- **Ghost row and add-column affordance (D-9)**: the per-column
  Add-Task ghost keeps transparent-rest/`surface-hover`-hover inside
  the card; the trailing add-column strip stays **outside** the
  cards on the background — it is not a stack, and its hover/tooltip
  seams (the pixel test's scan band, F-5) are untouched.
- **Solver caution (R2)**: the row padding is a static reduction of
  distributable width; equal-stretch + width-floor arithmetic is
  unaffected, and no new max-width binding appears (the round-1
  caution stands). Verified by the keyboard-resize journey and the
  1000/1500 goldens' column symmetry after the wrap.

### 2.4 Rhythm

The leg `VerticalLayout` spacing (`space-1`, :135) sets the
bar↔columns-row and columns-row↔stack-create-field gaps. **Default:
bump to `space-2`** so the cards do not hang flush under the
baseline; the eyeball probe may keep `space-1` if the attached look
reads better. Same probe discipline as D-6, zero test impact (the
pixel test reads live positions, F-5).

### 2.5 Gutter decision (D-6)

**Keep `space-3`.** D35 names `space-3` as the default and reserves
`space-4` for a cramped probe. The whole ladder scales uniformly, so
proportionality is scale-invariant — the question is purely
aesthetic at stage-2 regen time. The fallback's cost is recorded:
bumping :148 to `space-4` **requires** updating
`the_add_stack_glyph_is_square`'s `+12.0` → `+16.0`
(`journeys.rs:1576`) — the chunk's single permitted non-golden test
edit, listed in the handoff report and the PR body if taken. Default
expectation: no test edit at all.

### 2.6 Compact leg (D-10) and empty board (D-11)

**No card wrap in the compact leg.** A single full-width column has
no surrounding stacks — the strip-of-board-background metaphor has
nothing to frame; a card there is chrome without information.
Dispatch (c)'s compact concern is satisfied: the switcher's paints
(selected `surface-raised`, move-target `surface`, hover
`surface-hover` — :427-430) all stay distinct on the background, and
the TaskCard re-step still lifts compact cards off the background.
The empty-board leg (:113-125) is untouched.

## 3. D36 — The board bar as a tab strip

### 3.1 Anatomy (D-12/D-13/D-14)

```
BoardBar (Rectangle, height = space-6 + 2·space-1, unchanged)
├─ baseline := Rectangle        // declared FIRST (paints underneath)
│    y: parent.height - height; width: 100%
│    height: Theme.hairline-width; background: Theme.border
└─ HorizontalLayout (zero-segment guard unchanged; declared after ⇒ tabs paint over the rule)
   └─ tab := FocusScope (height 100%, max-width 160·scale, elide,
        tooltips, right-click, tab-list/tab roles: ALL untouched)
      └─ paint := Rectangle
         - ACTIVE:   height 100%; background Theme.background;
                     border-top-left/right-radius Theme.radius-medium;
                     bottom corners square; no border.
                     Covers the rule beneath itself (it "breaks");
                     merges across the space-1 gap into the board
                     background (F-7). Hover: no change.
         - INACTIVE: height 100% - Theme.hairline-width (the paint
                     box ends ON the rule — sides run down to it;
                     no mask rectangle needed, F-8's uniform-border
                     constraint handled by geometry);
                     background transparent; border-width
                     Theme.hairline-width; border-color Theme.border;
                     top corners radius-medium, bottom square.
                     Hover: background Theme.surface-hover, border
                     persists ("hover lifts per the existing hover
                     token").
```

Text roles are unchanged (active `body-emphasized`/`text-primary`,
inactive `body`/`text-secondary`). Hit targets stay the FocusScope's
full height in both states — tooltip anchors, hover wiring, click and
context seams see identical geometry (the paint box shrink is
paint-only).

### 3.2 Focus ring interplay (D-15)

`tab.has-focus` ⇒ frame swaps to the ring on both states:
`border-width: Theme.focus-ring-width`,
`border-color: Semantic.focus-ring` (inactive: the ring *replaces*
hairline+border-colour; active: the merged background tab gains its
ring — keyboard focus stays visible on a paint that is otherwise
invisible against the content, which is the a11y point). This
preserves D15/D20b semantics: the bar remains one tab stop per
board, selection follows the action layer, `bar.selected` is
untouched — the board-bar journeys assert model state, not paint.

### 3.3 The hairline token (D-4)

`Theme.hairline-width: 1px * base-scale` joins the radii block in
`theme.slint` (naming per §3.3: a dimension role, not a hue). It is
a spacing-class dimension, so the theme is its only legal home (the
house rule the theme's header states); no view-local `1px` literal.
**No colour token is added** — the extractor only parses `<color>`
properties, so `token_tests.rs` is byte-identical (F-4). Consumed by:
baseline rule height, inactive-tab border width.

Contrast: the rule and the inactive border paint `Theme.border` on
`Theme.background` — a locked NonText pair, 3.47:1 ≥ 3.0.

At scale 0.5 the hairline renders 0.5 logical px — the software
renderer anti-aliases; the 0.5-scale goldens arbitrate readability
(R-3), the Tier-4 monitor check confirms real-GPU rendering.

## 4. D37 — Contrast sanity & goldens

### 4.1 Contrast at both scale breakpoints

WCAG ratios are scale-independent (colours constant), so the
`token_tests.rs` lock covers every scale; the golden matrix's luma
budget (8/255 per cell) covers the rendered extremes. The changed
rendered pairs, with numbers (computed 2026-10-25, WCAG 2.x per
`token_tests.rs`):

- text on the new task-card surface (raised): text-primary 11.92:1,
  text-secondary 6.27:1 — locked PAIRS, pass unchanged.
- text on the new card container (surface): text-primary 13.54:1,
  text-secondary 7.12:1 — locked PAIRS.
- due-tone text on the re-stepped card surface: success 8.17,
  warning 8.43, overdue 7.05, **danger 4.92** (down from 5.53 on
  today's `surface` — above the 4.5 body floor; recorded residual,
  monitored at Tier-4). Extending PAIRS with tone-on-raised entries
  is a theme-level decision outside this chunk (§8).
- the strip and the rule: `border` on `background` 3.47:1 (locked
  NonText pair).

### 4.2 Golden regen discipline

1. Paint commits land **first**; the journeys run green after each
   (the goldens are still old — they are *not* in the journey suite).
2. **ONE dedicated commit** (D37; chunk 1's precedent `addda00`):
   `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
   --test golden_visuals` (the run self-fails by design), then the
   compare run green, then commit **all 12 PNGs** (F-6: today diffs
   too, not just board) as
   `chore(ui): regenerate goldens for the r2 visual-identity chunk`,
   body listing every diffed PNG with its visible change — the
   reviewed acceptance evidence for D35/D36. The matrix, the test
   file, and the comparison budget are untouched.
3. The PR's PNG diff is the review artefact (GitHub renders images);
   stage 3 reviews it against §2/§3's anatomy.

## 5. TDD-first discipline (paint-chunk adaptation) & commit slicing

No new tests are in scope — acceptance (d) permits only golden
diffs (with D-6's single recorded exception if the gutter fallback
fires). TDD-first therefore binds stage 2 to this order:

1. **Baseline-green proof**: run the full AGENTS §10 gate suite on
   the pristine branch before any paint. An existing failure here is
   a stop-and-report, never a pre-existing-failure carry.
2. **Commit 1** — `feat(ui-slint): stack columns render as cards on
   the re-stepped surface ladder (r2 D35)`: `task-card.slint` (1
   line), `badge.slint` (1 line), `board-layout.slint` (card wrap,
   row padding, rhythm), CHANGELOG entry. Journeys green.
3. **Commit 2** — `feat(ui-slint): the board bar renders as a tab
   strip (r2 D36)`: `theme.slint` (`hairline-width`),
   `board-bar.slint`, CHANGELOG entry. Journeys green.
4. **Commit 3** — the golden regen commit (§4.2). Stage-2 performs
   the D-6/D-7 eyeball probe on the regen candidates *before*
   committing (cramped gutter? attached rhythm?) — the fallbacks
   fire here or never.
5. **Full gate suite green** (fmt, clippy `-D warnings`, nextest,
   doc-tests, deny, the three-command llvm-cov sequence + quarantine
   + local `rm`), CHANGELOG Unreleased carrying both entries.

R2's probe ledger: per-corner radius — answered statically (F-8);
card-wrap solver perturbation — verified by journeys + golden
symmetry after commit 1 (a perturbation is a stop-and-record);
gutter cramp + rhythm — the stage-2 eyeball at step 4.

## 6. Tier-4 manual checklist (user runs; goes in the PR body)

- **T-1 Tab look at min viewport**: a window just above the compact
  floor (~730 logical px) with 3+ boards, long titles — active tab
  merges and breaks the baseline, inactive markers elide at the
  160·scale cap, keyboard focus ring visible on both tab states.
- **T-2 Card contrast on the user's monitor**: the `surface` card vs
  `background` strip step is subtle by design (1.11:1); the done-card
  wells and the lifted task cards must read on real hardware. If the
  user's monitor flattens it, the recorded fallback is a one-value
  palette nudge of `surface` upward (R-1).
- **T-3 Hover/focus walk**: over the bar (active + inactive),
  a column card header (hover/selected), the ghost row, and the
  add-column strip, at scale 1.0 and a high scale — every lift must
  be visible against its new parent.

## 7. Code inventory & estimated diff size

| File | Change | Size |
|---|---|---|
| `ui/theme.slint` | `hairline-width` token + comment | +~8 |
| `ui/compositions/board-layout.slint` | card wrap, floor-binding move, row padding/rhythm | ~+25/−6 |
| `ui/compositions/board-bar.slint` | baseline + tab paint/focus swap | ~+30/−12 |
| `ui/widgets/task-card.slint` | at-rest token | 1 |
| `ui/widgets/badge.slint` | neutral tone token | 1 |
| `CHANGELOG.md` | two Unreleased entries | +~6 |
| `tests/goldens/*.png` | 12 regenerated binaries | (binary) |

**~70–90 source lines across 5 `.slint` files + changelog + 12
PNGs.** Zero `.rs` diffs anywhere (asserted by stage 3; a Rust diff
is a finding per prompt 7.2). `taskboard-domain`, `taskboard-state`,
`taskboard-storage-sqlite`, `taskboard-sync-nextcloud`,
`taskboard-app`: empty diffs.

## 8. Risks

- **R-1 The card step is subtle** (surface vs background, 1.11:1):
  the strip reads but quietly. Tier-4 T-2 decides; fallback is a
  one-value `surface` nudge (PAIRS-safe — text-on-surface ratios
  only improve), taken as a follow-up commit on the branch, never
  silently.
- **R-2 Solver perturbation from the wrap** (§14-R2's named D35
  case): padding shrinks distributable width; a broken equal-stretch
  or width-floor shows as asymmetric goldens or a failed resize
  journey — stop-and-record, never a tolerance bump.
- **R-3 The 0.5-scale hairline** (0.5 logical px, AA-rounded) and
  80-px elide caps: the 0.5-scale goldens arbitrate; unreadable ⇒
  record and consider `focus-ring-width`-based rule in a follow-up,
  not a silent constant.
- **R-4 Golden churn beyond the board series** (today's 6 PNGs via
  shared TaskCard, F-6) may surprise a reviewer expecting "board
  goldens" — the regen commit body lists all 12 with reasons.
- **R-5 Danger-tone margin** on the re-stepped card (4.92 vs 4.5
  floor): above the floor; recorded, watched at Tier-4; a future
  theme-level PAIRS extension (tone-on-raised) is the structural
  fix, out of this chunk.

## 9. Deferred (recorded, out of scope)

- **Hoisting the control-row constant** (F-9, four sites incl. the
  ghost rows) into a Theme token: a pure refactor with zero visual
  delta — not paint, so not this chunk despite chunk 1's pointer;
  candidate for a `backlog:` issue at round exit.
- **PAIRS extension for tone-on-raised**: the completeness rules
  don't force it; numbers recorded in §4.1; theme-level decision.
- **#411 board-selector scalability**: D36 defers explicitly; the
  tab strip changes paint only.
