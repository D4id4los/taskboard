# [Plan] GUI Usability Pass — Hover, Tooltips, Hamburger, Menu IA & Sizing, Add-Stack

- **Date**: 2026-10-22
- **Kind**: implementation plan (stage 1 of the three-stage dispatch in
  `2026-10-22-gui-usability-pass-dispatch-prompts.md` — TDD-first spec
  for the stage-2 implementation agent). No production code in this
  commit.
- **Origin**: the user's GUI bug report of 2026-10-22 (dispatch §1 —
  the binding six-issue list); reconnaissance seeds (dispatch §2, all
  re-verified here); normative design origins: gui-phase7 design plan
  `2026-10-14-gui-phase7-design-plan.md` §3.1 (theme is the only style
  source), §3.4 (token lint), §4.2 (layering rules A–E), §5.9 (menus
  are projections, never hand-written), §6.1 (base-scale), §6.3–6.4
  (breakpoints + degeneration), §7.1 (every click affordance has an
  action equivalent), §7.2–7.3 (a11y discipline + keyboard journeys),
  §9.1 (contrast over token pairs), §10 (golden visuals);
  `2026-10-15-ui-action-keymap-plan.md` D1–D3/D15–D19 (registry,
  Category, menu projection, kbd-hint); `docs/testing_strategy.org` §7
  (the four UI test tiers); kiosk report
  `2026-10-21-netbook-kiosk-test-environment-report.md` (1024×600
  panel, winit scale guess 1.25 → **819×480 logical px** — the
  cut-off target; "Cosmetic" note at the end of the i686 lane
  section).
- **Target**: `crates/taskboard-ui-slint` only (markup, pure modules,
  its tests). No other crate; no domain/state changes.
- **Branch / worktree**: `feat/ui-usability-pass` in
  `../taskboard-WT-feat-ui-usability-pass`, off `main` @ `aac5097`
  (worktree created after `scripts/worktree-doctor.sh` passed: 103G
  free, floor 25G).
- **Goal**: all six reported issues fixed with the design's seams
  intact — a perceptible hover vocabulary built on one new token, a
  delayed tooltip mechanism whose text is the a11y label, the
  hamburger pinned square (root cause diagnosed and reproduced
  headlessly, §1.4–1.5), a section-grouped menu that stays a pure
  registry projection, a menu that fills only the available space and
  scrolls instead of being cut off (proven at the kiosk netbook's real
  geometry), and an on-board New-stack affordance whose click routes
  the same registry action as the keyboard — including the
  previously invisible stack-create flow surface (§1.7).
- **Status**: READY — stage 2 implements exactly this plan; deviations
  need a fresh recorded reason.

---

## 1. Context & verified facts (anchored — do not re-derive)

1. **The theme has no hover token** (dispatch §2 confirmed):
   `ui/theme.slint` palette is `background #14161c, surface #1d2028,
   surface-raised #262a35` plus text/border/focus colours. The only
   `has-hover` in the tree is `ui/widgets/icon-button.slint:27`
   (`touch.has-hover ? Theme.surface : transparent`) — on the toolbar
   and status bar, both `Theme.background: Theme.surface`, so the one
   hover that exists renders literally invisible. Every other
   interactive element (board-bar segments, compact switcher segments,
   column headers, task cards, menu rows, banner rows) has no hover
   state at all.
2. **The menu has no bound and no scroll** (dispatch §2 confirmed):
   `ui/compositions/action-menu.slint` pins `min-width: 320px*scale`
   and `min-height: 240px*scale`, no max constraint; the host in
   `ui/app-window.slint` mounts it at `x/y: Theme.space-8` under
   `if (KeyboardState.menu-open)`. A ~25-row menu cannot fit the
   kiosk netbook's viewport (§1.3).
3. **The kiosk reality** (kiosk report): 1024×600 LVDS panel, winit
   guessed scale factor **1.25** → the window is **819×480 logical
   px**. Both geometries (raw 1024×600 and logical 819×480) are in
   this plan's test matrix; 819×480 is the binding one for cut-off.
4. **Hamburger root cause — diagnosed and reproduced** (AGENTS §5
   both-sides evaluation done; this is a production bug, not a test
   bug — the committed goldens themselves contain the stretched bar,
   so no existing test contradicts it). `IconButton` declares only
   `min-width/min-height: Theme.space-6` — no maximum — so in the
   toolbar's `HorizontalLayout` it is growable without a cap. The
   title's explicit `horizontal-stretch: 1` does not protect it:
   Slint 1.18.1's space distribution (`i-slint-core layout.rs::
   adjust_items`) grows every uncapped item whose `can_grow > 0`,
   sharing the leftover across stretchy items and then feeding the
   rest to whatever still has room; the title (an elided `Text`)
   saturates at its preferred width, after which the button absorbs
   the remainder. The icon-box `VerticalLayout` then stretches the
   three bars across the full button width (cross-axis stretch
   default) — the giant hamburger.
   Measured hamburger-bar widths (uniform runs of the icon colour
   `Theme.text-secondary #a6abbd`, the only long monochrome runs in
   the toolbar row):
   - committed golden `board-0.5-1000.png`: **447 px** of a 1000 px
     row (≈45 %);
   - committed golden `board-3.0-1000.png`: **183 px** (61 logical);
   - headless repro (journey harness, populated board, scale 1.0):
     **394 px** at 1000×900, **406 px** at 1024×600, **304 px** at
     819×480 (37 % of the row) — the reported bug, reproduced at the
     failing kiosk geometry.
   The golden-derived and live-measured numbers agree exactly, so the
   repro is exact. Fix validated in the same scratch harness:
   `max-width: Theme.space-6` on the primitive yields **8/16/48 px**
   bars at scale 0.5/1.0/3.0 (exactly `space-6 − 2×space-2`, the
   token-derived square) at every tested geometry, with the title
   taking the freed space. The toolbar itself needs no change.
5. **The stack-create flow has no presentation surface** (new
   finding, deepens issue 6): `stack.create` is registered
   (`actions.rs`: label "New stack", contexts Board/StackDetail,
   enabled) and bound in presets (standard `S` in Board, vi `O` in
   ViNormal), and the router maps it to `UiEffect::BeginCreateStack`;
   `view_state::begin_create_stack` opens `Flow::CreateStack` and
   `commit_edit` commits `StateCommand::CreateStack` from the shared
   buffer. But `models.rs` exposes `create-open` only for
   `Flow::Create` (task create) and the StackDetail row — no model
   field, no `TextField`, and no window callback exists for the
   stack-create flow. Pressing `S` (or invoking the menu row) opens a
   flow that renders *nothing*: no field mounts, nothing mirrors into
   the buffer, and Return commits an empty buffer (a silent no-op).
   The menu path the user called "reachable" opens an invisible flow.
   Both the on-board button (issue 6) and the keyboard path need the
   flow's visible surface landed with the button.
6. **Hover is not headlessly drivable**: `i-slint-backend-testing`
   1.18.1 exposes `send_mouse_click` but no mouse-move API, so
   `TouchArea::has-hover` cannot be turned on in a test. Hover
   *rendering* is therefore proven at the token level (§3.1's
   distinctness/contrast guards) plus review; the golden tier captures
   rest state only. Everything else in this plan (geometry, menu
   scroll, tooltips, clicks) is headlessly testable: `Timer` elements
   fire under the testing backend's mock clock (`advance_time`),
   `Window::set_size` gives real layout (test_platform /
   driver `set_window_size`), elements are addressable by
   accessible label, and `ElementHandle::
   invoke_accessible_default_action()` delivers a real click to a
   button-role element — the click-equivalence seam for §7.1.
7. **Slint 1.18.1 facts used below** (version-pinned, verified in the
   vendored sources): `absolute-position` is a reserved **output**
   property on every item (window-logical `Point` — typeregister.rs
   line 337; the compiler's own lookup tests exercise
   `header.absolute-position.y`); `Timer` is a builtin element
   (`interval`/`running`/`triggered`); `Flickable` is a builtin with
   `viewport-x/y/width/height`; `ElementHandle` offers no public
   geometry accessor, so component tests assert geometry through
   pixels (the golden-compare `Rgba8Image` snapshot) and state through
   generated getters.
8. **Token/contrast machinery** (token_tests.rs): the extractor
   asserts the exact declared token inventory, and every declared
   colour token must appear in a contrast pair or a recorded
   exemption — a new palette token mechanically touches
   `extractor_finds_every_declared_theme_token` and the pair table.
   The token lint (`scripts/check_slint_tokens.py`) forbids literal
   colours/fonts anywhere but the theme and px literals bound to
   layout properties unless the expression references a theme token —
   the new max-height/scroll arithmetic below references tokens and
   window geometry only.
9. **Test suites that must not break**: `tests/journeys.rs` (menu
   index arithmetic — sections must not renumber rows; command
   boundary asserts — the stack-create journey only *adds* commands
   inside its own new test), `tests/golden_visuals.rs` (six goldens
   change *content* but not the comparison protocol — regen is the
   explicit step, §3.8), `tests/keymap_snapshots.rs` (untouched: no
   registry/keymap/preset changes — §3.5), `src/token_tests.rs`
   (extended, never weakened — §3.1), `tests/pin_guard.rs` (untouched:
   no manifest changes).

## 2. Scope

**In** (dispatch §1's six issues, each mapped in §3):

- The `surface-hover` token, contrast pairs, and the
  distinctness/perceptibility guards (issue 1).
- Hover feedback on every interactive element (issue 1) — inventory
  in D2.
- The delayed tooltip mechanism + icon-button wiring (issue 2).
- The hamburger fix: `max-width` pin on the primitive + the geometry
  contract test (issue 3).
- Menu IA: section grouping derived from `Category` (issue 4).
- Menu sizing: available-space bound + `Flickable` scroll area +
  scroll indicator + cursor-follows-selection (issue 5).
- The on-board New-stack affordance (board-bar button + empty-state
  button) and the stack-create flow's visible field (issue 6).

**Out** (non-goals, each a recorded fence):

- No theme switching machinery, no light/high-contrast themes
  (POST-1.0, Q13); exactly one new palette token.
- No icon font (D19 discipline): the new `plus` glyph is geometric,
  composed of primitives like the existing four `IconKind`s.
- No domain/state/storage changes; no new `StateCommand`s; the
  adapter only routes existing actions (`stack.create` already
  dispatches `CreateStack` via the existing flow).
- No registry/keymap/preset changes: `keymap_snapshots.rs` and the
  preset contracts stay byte-identical; `Category` gains only a
  display-label function, not a variant.
- No submenus, no context menus, no menu search — section headers
  keep the one-modal-context keyboard model (keymap plan D3) intact.
- No std-widgets adoption (ScrollView et al.): the scroll indicator
  is hand-built from tokens like every other primitive.
- No tooltips beyond the label-carrying controls (§3.3's scope rule);
  no tooltip theming variants; no hovertext for elements whose label
  is already visible text.
- No stack *management* UI (rename/delete/reorder — CLI-only per the
  chunk-3 D13 scope cut); the button creates, nothing else.
- No compact-breakpoint board-bar: the strip stays hidden < 720 px
  (keyboard `S` remains the path there, §6.4's R4-holds-at-every-size
  rule); the empty-state button covers both breakpoints.

## 3. Decisions (each binding for stage 2)

### D1 — One new palette token, `Theme.surface-hover: #303545`

Colours live only in the theme (design §3.1); hover is an interaction
*surface*, so it sits in the `Theme` palette next to its siblings
(the `Semantic` global stays the state-colour vocabulary it is —
hover is not a state colour). The value is chosen against measured
floors, computed with the same WCAG math as `token_tests.rs`:

| pair | ratio | floor |
|---|---|---|
| `surface` → `surface-hover` | 1.335 | ≥ the `surface`→`surface-raised` step (1.136) — the hover cue is at least as visible as the existing selection language |
| `surface-raised` → `surface-hover` | 1.175 | ≥ the same 1.136 floor — visible on raised parents (menu panel, selected segments) |
| `text-secondary` on `surface-hover` | 5.34 | ≥ 4.5 Body (icons render in this colour) |
| `text-primary` on `surface-hover` | 10.15 | ≥ 4.5 Body |

One token, always *lighter* than everything it can be painted over —
no per-parent variants, no context-dependent hover arithmetic. The
icon-button's `pressed` state merges with hover
(`pressed || has-hover ? Theme.surface-hover : transparent`): press
feedback is the click itself; a darker-than-hover pressed state would
read as a regression.

**Guards (token_tests.rs, extending — never weakening):**

- `extractor_finds_every_declared_theme_token`: expected list grows
  `Theme.surface-hover` (the exact-inventory assertion stays exact).
- `PAIRS` grows `(text-primary, surface-hover, Body)` and
  `(text-secondary, surface-hover, Body)` — the completeness rule
  (every token in a pair or recorded exemption) then holds without an
  exemption entry.
- New test `hover_token_is_distinct_from_its_parents`: extracted hexes
  assert `surface-hover ≠ {background, surface, surface-raised}`, and
  `contrast(surface, surface-hover) ≥ contrast(surface,
  surface-raised)` and `contrast(surface-raised, surface-hover) ≥
  contrast(surface, surface-raised)` — the structural lock against
  the old invisible-hover bug (hover == parent colour) and against an
  imperceptible step. Pure data assertions over the same extractor.

### D2 — The hover inventory (issue 1: "almost no elements change")

Every interactive element responds through the same rule — *rest
background becomes `Theme.surface-hover` while hovered* — applied to
the `TouchArea` each of these already owns (no new input elements,
no hover-only widgets):

| element (file) | rest bg | hover bg | notes |
|---|---|---|---|
| `IconButton` (icon-button.slint) | transparent | `surface-hover` | toolbar menu button + banner snooze/mute; icon colour for `Tone.neutral` lifts `text-secondary` → `text-primary` while hovered (non-neutral tones keep their semantic colour — the background carries the cue) |
| board-bar segments (board-bar.slint) | transparent / `surface-raised` (selected) | `surface-hover` | selected+hover still lightens; selection stays marked by the raised rest state + emphasized text |
| compact switcher segments (board-layout.slint) | same as above | same | identical segment pattern (D8's one mechanism) |
| board column headers (board-layout.slint) | transparent / `surface-raised` (move target) | `surface-hover` | `header-hit` TouchArea drives it |
| task cards (task-card.slint) | `surface` / `background` (done) | `surface-hover` | hover stays distinct from the emphasis ring (focus ≠ hover, both may coexist) |
| action-menu rows (action-menu.slint) | transparent / `surface` (selected, inset) | `surface-hover` | selection keeps its inset + ring; hover lightens over the raised panel |
| banner rows (status-bar.slint) | transparent over `Theme.background` | `surface-hover` | the row-box Rectangle behind `ListRow` |
| text fields (text-field.slint) | `surface-raised` + focus ring | **none** | recorded exclusion: the field's affordance is its focus/insert state; a hover bg change would fight the ring (and `TextInput` focus is the keyboard story) |

Disabled rows (menu `enabled: false`) never hover: their `TouchArea`
is disabled, so `has-hover` cannot engage — correct by construction.
Rendered hover is not headlessly assertable (§1.6): the tests for D2
are D1's token guards plus review that each `TouchArea` above feeds
its background binding.

### D3 — Tooltips: a window-level host, the a11y label as the only text source (issue 2)

Slint has no native tooltip; the mechanism is one global + one widget
+ one timer, all in the window crate:

- **`global TooltipState`** (declared in `ui/app-window.slint` beside
  `KeyboardState`, exported like it): `text: string`, `visible: bool`,
  `anchor-x/y/width/height: length`.
- **Trigger, inside `IconButton`** (the only icon-only — and thus
  self-explanation-needing — controls: toolbar menu button, banner
  snooze/mute, the new-stack buttons of D7): the existing `TouchArea`
  gains `changed has-hover` — on enter it calls
  `TooltipState.hover(root.label, self.absolute-position.x,
  self.absolute-position.y, self.width, self.height)`; on leave and
  on `clicked` it calls `TooltipState.unhover()`. `hover` with an
  empty label is a no-op (a control without a label never shows a
  tooltip).
- **The delay** is a `Timer` element in `AppWindow`
  (`interval: 600ms`, `running: TooltipState.pending` — the global
  carries a `pending` bit set by `hover`/cleared by `unhover`;
  `triggered => TooltipState.visible = true`). The Timer fires under
  the testing backend's mock clock, so tests are deterministic
  (§1.6); the delay is a duration, not a layout dimension — outside
  the token lint's rule 3.
- **Placement** (window-level host, `if (TooltipState.visible)` after
  the menu host — later siblings paint on top, the menu-over-tooltip
  order is right; the menu's `init` calls `unhover()`): default
  *below* the anchor (`anchor-y + anchor-height + Theme.space-1`),
  flipped *above* (`anchor-y − Theme.space-1 − panel-height`) when
  the below position would cross the window bottom minus the status
  bar's top; x centred on the anchor and clamped to
  `Theme.space-2` margins. The panel is display-only: no `TouchArea`
  (clicks pass through), no focusability — it can never steal the
  keyboard (§5.1's single key path is untouched; no `PopupWindow` is
  used, precisely because a popup would create a second focus scope).
- **Text source**: `root.label` — the *same property* that feeds
  `accessible-label` (D16). Tooltips introduce no new strings and can
  never carry information unavailable elsewhere (the a11y tree, the
  generated menu, and — for `stack.create` — the visible button all
  carry the label). Non-icon controls (segments, headers, cards)
  show their labels as visible text and get **no** tooltip — recorded
  exclusion, not an oversight.
- **Keyboard/touch story**: none by design. A keyboard user reaches
  every labelled control through focus + a11y (§7.2) and every action
  through the keymap (§7.1); touch has no hover and loses nothing
  (the tooltip is redundant eye-candy, never the only carrier).

**Tests** (`tests/usability_components.rs`, Tier 2): invoke the
generated global's `hover` with a probe label → `advance_time(600)` →
`TooltipState.visible` is true; `unhover` → false; an empty label
never becomes visible; re-hover inside the delay window restarts it
(visible still false at 599 ms-equivalent — advance in two steps).
Typed state only; the label-binding identity (tooltip text ≡ a11y
label) is structural — one property reference in markup — and
review-checked.

### D4 — The hamburger fix: the primitive owns its canonical size (issue 3)

Per §1.4's diagnosis: `IconButton` grows
`max-width: Theme.space-6` beside its `min-width` (the square is the
primitive's declared size class — Rule E: hosts need no layout
knowledge; the status-bar's banner buttons are unaffected — they
already render at their minimum). `min-height` stays and the vertical
cross-axis keeps stretching (a 32×48 hover target at scale 1.0 is a
better target than a strict square; the icon itself stays centred).
The title keeps `horizontal-stretch: 1` and now receives the freed
space; `toolbar.slint` needs no change.

**Test** (`tests/usability_components.rs`, the geometry contract):
render the populated fixture, snapshot, and measure the longest
uniform run of the icon colour in the toolbar rows — a pure-pixel
scan (no element-geometry API exists, §1.7). Assert the run equals
`space-6 − 2×space-2` at the window's scale, ±1 px for rounding, at:

- 1024×600 (the kiosk panel, raw),
- 819×480 (the kiosk panel through winit's 1.25 guess — the reported
  failing geometry),
- 1000×900 at scale 1.0, and the matrix extremes 1000×900 at scale
  0.5 and 3.0.

This is a layout *contract* (an exact token-derived value), not a
quality ceiling — AGENTS §5's no-ceilings rule targets caps on
success; an exact geometry pin is the same species as the §9.2
keypress ceilings the session already accepted.

### D5 — Menu IA: section headers derived from `Category`, zero new actions (issue 4)

The menu stays a pure registry/keymap projection (§5.9) and the
keymap layer stays untouched:

- `Category` (actions.rs) grows `section_label() -> &'static str`
  ("Application", "Capture", "Search", "Navigation", "Task", "Menu",
  "Mode") — the declaration order remains the documented menu order.
  No new categories, no registry edits: the registry snapshot and
  preset snapshots stay byte-identical.
- `menu::entries` annotates rows: `MenuEntry` grows
  `section-title: &'static str`, set to the row's category label when
  it differs from the previous row's category (first row always) —
  the section boundaries are *derived data*, computed in the same
  stable `(category, registry order)` sort the menu already has.
- `MenuEntryModel` (action-menu.slint) grows `section-title: string`;
  the adapter's `OpenMenu` mapping copies it. The composition renders
  a header (`RoleText`, `Typography.caption`,
  `Theme.text-secondary`) above a row when its `section-title` is
  non-empty. Headers are not rows: `menu-selected-index` still
  indexes *entries*, `menu.next/previous` still step entries, and
  `menu.invoke` can never land on a header — the D3 one-modal-context
  keyboard model, the presets' hints, and the journeys' menu index
  arithmetic are all untouched by construction.
- Section labels are data-derived, never hand-written rows (§5.9
  verbatim); adding an action to the registry automatically files it
  under its category's section.

**Tests**: unit tests in `menu.rs` — entries over the Board chain
carry non-empty `section-title` exactly at category boundaries (and
the first row); the count of distinct sections equals the count of
distinct categories present. The existing T24 tests keep passing
unchanged (field addition, no ordering change).

### D6 — Menu sizing: fill only the available space, scroll beyond it (issue 5)

- **Bound**: the host binding in `app-window.slint` gives the menu
  `max-height: root.height − self.y − status.height −
  Theme.space-2` (toolbar bottom = `self.y = Theme.space-8`; the
  named `status` element's height already accounts for banner rows).
  The panel's `min-height: 240px*scale` is **removed** — a short
  menu (a Global-only chain) must shrink to its content, a long menu
  must cap at the bound. All terms reference tokens/window geometry:
  token-lint clean.
- **Scroll area**: the entry list moves into a `Flickable` (builtin;
  wheel + drag scrolling) inside the panel; when content fits, the
  Flickable is inert and the panel is exactly content-sized. A thin
  scroll indicator (track in `Theme.surface`, thumb in
  `Theme.border`, `Theme.space-1` wide at the panel's inner edge)
  renders only when `viewport-height > flickable-height` — an
  affordance, not decoration.
- **Cursor follows selection**: the keyboard path must stay
  complete at any size (§7.3 step 8). Each row carries
  `changed root-selected-index => { if selected == self: adjust
  viewport-y so the row is fully inside the viewport }` (below →
  scroll down, above → scroll up; a no-op when everything fits) —
  `menu.next/previous` at the kiosk geometry walks the whole list
  with the selected row always visible.
- **Proof** (`tests/usability_components.rs`): at 819×480 open the
  menu (the journey harness's `press(ActionId::AppMenu)` path) and
  assert by pixel scan that the panel's focus-ring border forms a
  closed box *above* the status bar's top edge (no cut-off); walk
  `menu.next` to the last entry and assert the selected row's ring
  stays inside the panel's border box (the follow logic); repeat the
  pair at 1024×600 and at the golden matrix's scale extremes
  (600×900 @ 0.5 and 3.0) — small-window small-scale is the other
  cut-off corner. The `min-width: 320px*scale` stays (width is not
  the failing axis).

### D7 — The on-board New-stack affordance — and the flow's missing surface (issue 6)

- **Registry action**: `stack.create` ("New stack"), already enabled,
  already bound (standard `S` / vi `O`) — the button adds no new
  action, it *is* the click equivalent (§7.1 verbatim).
- **Placement**:
  - *Populated board*: an `IconButton` (`IconKind.plus` — new
    geometric glyph, a recorded theme-level icon-vocabulary addition
    per D19's comment; `label: "New stack"` — the registry label)
    right-aligned in the board bar after a stretching spacer. The bar
    is regular/expansive-only today; at the kiosk netbook's real
    logical width (819 ≥ 720) the bar renders — compact (< 720)
    keeps the keyboard `S` path (recorded in non-goals).
  - *Empty board* (`stacks.length == 0`): the EmptyState branch grows
    the same `IconButton` (label "New stack") below the hint text —
    EmptyState itself stays a pure text primitive (Rule E: the
    composition owns the affordance). The branch's hint text changes
    from "Add the first column: menu › New stack" to reference the
    on-board button (no test asserts label text — AGENTS §5; the
    goldens render a populated board and are unaffected by this
    string).
  - Exactly one button per state — no duplicated affordance.
- **The flow's visible surface** (the §1.5 finding — without it both
  the button and the `S` key open an invisible flow): `BoardModel`
  grows `stack-create-open: bool` (`models.rs`:
  `matches!(flow, Flow::CreateStack)`); `board-layout` renders a
  `TextField` (label/placeholder "New stack name", auto-focus per the
  existing `TextField::init`) at the end of the columns row in the
  regular branch and below the switcher in the compact branch when
  open. New window callbacks `board-stack-create-edited(string)` /
  `board-stack-create-committed(string)` wire the exact
  mirror/commit seams the task-create rows use (`set_buffer` /
  `commit_text` — the `Flow::CreateStack` commit path already
  dispatches `StateCommand::CreateStack` and returns to normal mode).
  Escape still discards (mode.normal → discard, the existing D5/D9
  rule — the view-graph walk already covers the flow's Escape).
- **Adapter**: the button's click callback routes
  `ActionId::StackCreate` through the same `route()` the resolver
  feeds (the `install_toolbar_seam` precedent); a11y: the
  IconButtons' FocusScopes carry `accessible-role: button` +
  `accessible-label` (the D15/D16 discipline, existing in the
  primitive).
- **Tests**: `tests/journeys.rs` grows one journey —
  `stack_create_has_a_visible_click_equal_flow`: (a) keyboard:
  `press(ActionId::StackCreate)` → `board-model.stack-create-open`
  is true → `type_text("s")` → Return → the boundary captured exactly
  `StateCommand::CreateStack { title: "s", .. }` and the flag
  cleared; (b) click equivalence: `driver.click_by_label("New
  stack")` (new `JourneyDriver` method wrapping
  `ElementHandle::invoke_accessible_default_action` — no coordinate
  arithmetic) opens the same flow state, then the same commit path
  yields the same command shape — click ≡ key at the observed
  boundary. The keypress guard is not extended (stack-create is not
  a §9.2 core verb; its count equals Create's ≤ 3 anyway).

### D8 — Test catalogue, goldens, and gates (issue coverage ledger)

- **New Tier-2 file `tests/usability_components.rs`**: D4's geometry
  contract; D6's sizing/scroll proofs; D3's tooltip state machine.
  Shares `tests/support/` (driver + fixture + fake_engine) — no new
  harness concepts beyond `click_by_label`.
- **`tests/journeys.rs`**: + the D7 stack-create journey. Existing
  journeys unchanged: menu indices survive D5 (rows, not headers,
  are indexed); the `Escape`-discards-create walk already covers
  Flow::CreateStack's Escape via the same mode machinery.
- **`src/token_tests.rs`**: + D1's pairs/inventory/distinctness
  guards. The contrast gate stays green with the new token by
  construction (measured floors in D1).
- **`tests/keymap_snapshots.rs`, `tests/pin_guard.rs`**: untouched.
- **Goldens**: all six PNGs change exactly twice — the toolbar's
  hamburger region (D4, commit 2) and the board bar's plus button
  (D7, commit 7). Hover (rest state in headless), tooltips (hidden
  without hover), menu sections/scroll (menu closed), and the
  empty-board string (goldens render a populated board) leave the
  rest state pixel-identical per commit. Regen protocol verbatim:
  `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
  --test golden_visuals` — the run **fails afterwards by design**;
  never hand-edit PNGs; the PR's cumulative PNG diff is the review
  artefact, and the plan's expectation for it is: toolbar bar +
  right-edge of the board bar differ, nothing else.
- **Gates**: the full AGENTS §10 suite per commit ladder; the token
  lint (`scripts/check_slint_tokens.py`) stays clean — the only new
  literals are the hover hex (in `theme.slint`, allowed) and the
  600 ms timer interval (a duration, not a layout binding).

## 4. Implementation steps (TDD order; every commit standalone)

1. **Hover token** — failing first: extend `token_tests.rs`
   (inventory + pairs + `hover_token_is_distinct_from_its_parents`),
   watch them fail, then add `Theme.surface-hover: #303545`.
   Commit: `feat(ui-slint): surface-hover token with contrast guards`.
2. **Hamburger** — failing first: the D4 geometry contract in
   `tests/usability_components.rs` (measure the bar, assert the
   token-derived width — fails at 394/447/… px), then
   `max-width: Theme.space-6` in `icon-button.slint`. Regen the six
   goldens in the same commit (toolbar region). Commit:
   `fix(ui-slint): pin the icon-button square against layout stretch`.
3. **Hover inventory** — apply D2's table across the seven element
   files (no test churn: goldens capture rest state). Commit:
   `feat(ui-slint): perceptible hover on every interactive element`.
4. **Tooltips** — `TooltipState` global + `ui/widgets/tooltip.slint`
   + AppWindow host/Timer + `IconButton` trigger; the D3 component
   tests first (they fail on the missing global). Commit:
   `feat(ui-slint): delayed hover tooltips from a11y labels`.
5. **Menu sections** — `Category::section_label` + `menu.rs`
   annotation + unit tests first, then the model field + composition
   headers + adapter mapping. Commit:
   `feat(ui-slint): section-grouped generated action menu`.
6. **Menu sizing** — the D6 component tests first (they fail: the
   panel has no bound), then max-height binding, `Flickable`,
   indicator, cursor-follow. Commit:
   `fix(ui-slint): bound the action menu and scroll the overflow`.
7. **Add-stack** — the D7 journey first (fails: no model field, no
   buttons), then models field, board-bar/empty-state buttons,
   stack-create `TextField`s, callbacks, adapter seams,
   `driver.click_by_label`. Regen the six goldens in this commit
   (board-bar region). Commit:
   `feat(ui-slint): on-board new-stack affordance and its create flow`.
8. **Full gate suite** (AGENTS §10) green on the branch tip; the
   cumulative `git diff origin/main...HEAD -- '*.png'` reviewed as
   the golden artefact.

Steps 2 and 7 are the only visual-regen points; each commit builds
and passes its gates standalone (rebase-merge hygiene).

## 5. Test catalogue (TDD ledger)

| # | Test | Tier / file | Locks |
|---|---|---|---|
| G1 | hover token inventory + contrast pairs extend exactly | unit — `src/token_tests.rs` | D1 |
| G2 | `hover_token_is_distinct_from_its_parents` (hex ≠ parents; step ≥ raised step, both parents) | unit — `src/token_tests.rs` | D1 |
| G3 | hamburger bar == `space-6 − 2×space-2` (±1 px) at 1024×600, 819×480, 1000×900 × {0.5, 1.0, 3.0} | component pixel — `tests/usability_components.rs` | D4, issue 3 |
| G4 | tooltip: hover → delay → visible; unhover/click → hidden; empty label never visible; re-hover restarts the delay | component — `tests/usability_components.rs` | D3, issue 2 |
| G5 | menu sections: non-empty `section-title` exactly at category boundaries; one per present category | unit — `src/menu.rs` | D5, issue 4 |
| G6 | menu panel's border box closes above the status bar at 819×480 and 1024×600; also 600×900 @ 0.5/3.0 | component pixel — `tests/usability_components.rs` | D6, issue 5 |
| G7 | `menu.next` to the last entry keeps the selected row's ring inside the panel (cursor-follow) at 819×480 | component pixel — `tests/usability_components.rs` | D6, §7.3 |
| G8 | stack-create: `S` → visible flow → commit → exact `CreateStack` command; flag clears | journey — `tests/journeys.rs` | D7, issue 6 |
| G9 | `click_by_label("New stack")` opens the same flow and commits the same command shape (click ≡ key) | journey — `tests/journeys.rs` | D7, §7.1 |
| G10 | the six goldens hold after the explicit regen; PNG diff = toolbar + board-bar regions only | golden — `tests/golden_visuals.rs` | D8 |

Untouched by design: `tests/keymap_snapshots.rs` (D5 changes no
registry/keymap data), `tests/pin_guard.rs`, the existing journeys'
assertions, the state crate's suites.

## 6. File manifest (end state)

```
crates/taskboard-ui-slint/ui/theme.slint                    (+3: token)
crates/taskboard-ui-slint/ui/widgets/icon-button.slint      (~+20: max-width, hover, tooltip trigger; IconKind.plus)
crates/taskboard-ui-slint/ui/widgets/tooltip.slint          (new, ~45)
crates/taskboard-ui-slint/ui/app-window.slint               (~+50: TooltipState, Timer, host, 3 callbacks)
crates/taskboard-ui-slint/ui/compositions/action-menu.slint (~+70 net: sections, Flickable, indicator, follow)
crates/taskboard-ui-slint/ui/compositions/board-bar.slint   (~+20: plus button)
crates/taskboard-ui-slint/ui/compositions/board-layout.slint (~+50: empty-state button, stack-create fields ×2, threading)
crates/taskboard-ui-slint/ui/views/board-view.slint         (~+8: callback threading)
crates/taskboard-ui-slint/src/actions.rs                    (~+18: Category::section_label)
crates/taskboard-ui-slint/src/menu.rs                       (~+30: section annotation + tests)
crates/taskboard-ui-slint/src/adapter.rs                    (~+45: mapping + 3 click seams)
crates/taskboard-ui-slint/src/models.rs                     (~+6: stack-create-open)
crates/taskboard-ui-slint/src/token_tests.rs                (~+45)
crates/taskboard-ui-slint/tests/support/driver.rs           (~+15: click_by_label)
crates/taskboard-ui-slint/tests/usability_components.rs     (new, ~230)
crates/taskboard-ui-slint/tests/journeys.rs                 (~+90)
crates/taskboard-ui-slint/tests/goldens/*.png               (6 regenerated, twice)
```

Estimated diff: **≈ 700–850 added/changed lines** across 16 text
files (+ the 6 PNGs) — an M chunk. No new dependencies; no manifest
changes.

## 7. Acceptance mapping (dispatch §1 → this plan)

| # | Reported issue | Where |
|---|---|---|
| 1 | hover missing/invisible | D1 (token + guards), D2 (inventory), G1/G2 |
| 2 | no hovertext | D3 (mechanism), G4 |
| 3 | hamburger stretches | D4 (+ §1.4 diagnosis), G3 |
| 4 | menu flat/willy-nilly | D5 (sections from Category), G5 |
| 5 | menu cut off at kiosk | D6 (bound + scroll + follow), G6/G7 |
| 6 | no add-stack button | D7 (buttons + the flow's missing surface), G8/G9 |

## 8. Risks & fallbacks

- **Flickable wheel behaviour**: plain `Flickable` may deliver wheel
  scrolling inconsistently across platforms in 1.18.1. Fallback: keep
  the `Flickable` (touch/drag) and add the keyboard path's
  cursor-follow as the guaranteed scroll driver (it assigns
  `viewport-y` directly); verify wheel manually in the release pass —
  the kiosk's input device (touchpad) works through drag either way.
- **`absolute-position` semantics**: window-relative logical
  coordinates per the 1.18.1 sources (§1.7); if a backend quirk
  surfaced on Windows/macOS (release-chunk platforms), the tooltip's
  placement binding is one expression — re-clamped or reduced to
  below-anchor-only — without touching the mechanism.
- **Geometry pixel-scans vs renderer drift**: the scans target
  uniform token colours (`text-secondary`, `focus-ring`), which are
  stable constants; the ±1 px tolerance absorbs half-scaling
  rounding. If the software renderer's antialiasing shifts an edge,
  the assertion relaxes on the *tolerance*, never on the contract.
- **`status.height` in the max-height binding**: a binding-cycle
  would be a compile error (caught immediately); the menu and the
  status bar share no properties, so none is expected.
- **Goldens churn beyond the two expected regions**: any extra
  region differing in the PNG diff is a finding for the stage-3
  review (the plan's expectation is recorded in D8).
- **The stack-create field's focus hand-off** mirrors the existing
  task-create rows (the D15 refocus-on-flow-end seam already handles
  unmount); the view-graph walk's Escape leg covers the new flow
  automatically once it renders.

## 9. Verification of this round

- Every dispatch §1 issue maps to a decision and a ledger row (§7);
  no issue is silently narrowed (the §1.5 finding *widens* issue 6's
  fix and is recorded).
- The §2 reconnaissance seeds were all re-verified against the worktree
  sources; two were corrected/extended by this round: the menu's
  `min-height` (240px, not 240px-flat — it is scale-multiplied) and,
  materially, the invisible stack-create flow (§1.5), which the seeds
  did not know about.
- The hamburger root cause was reproduced headlessly at the failing
  geometry (394/406/304 px measured live; 447/183 px in the committed
  goldens) and the fix was validated in the same harness (8/16/48 px)
  before this plan was written; the scratch harness was removed and
  the tree is clean.
- Per AGENTS §5, the failing-new-test question was evaluated both
  ways for the hamburger: the goldens contain the stretched bar and
  no existing test asserts toolbar geometry, so the bug is
  production-side; the fix changes production, the new test locks it.
- No normative origin is violated: §5.9 (menu stays generated data —
  sections are derived, zero hand-written rows), §7.1 (the button
  routes the registry action; G9 proves the equivalence), §7.2/D15-D16
  (a11y labels are the tooltip source; new controls are labelled),
  §3.1/§3.4 (one token in the theme; lint stays clean), §9.1
  (contrast pairs extend), §10 (regen protocol verbatim), keymap plan
  D3/D16/D19 (no new actions, no key handlers in the menu, hints
  untouched — snapshots byte-identical).
- Worktree doctor passed before creation (103G ≥ 25G floor); the
  branch/worktree exist off `main` @ `aac5097`; nothing is pushed.
