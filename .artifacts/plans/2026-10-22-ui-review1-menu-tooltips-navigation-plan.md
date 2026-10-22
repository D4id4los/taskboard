# [Plan] GUI Review Round 1, Chunk 1 — menu, tooltips, navigation, window stability

- **Date**: 2026-10-22
- **Branch / worktree**: `feat/ui-review1-menu-tooltips-navigation` in
  `../taskboard-WT-feat-ui-review1-menu-tooltips-navigation` (off `main`
  @ `66b65ed`; `scripts/worktree-doctor.sh` run first: 83G free, floor
  25G — pass).
- **Status**: stage-1 chunk plan for dispatch prompt 1.1. Normative
  inputs: the accepted design round
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md`
  (§1 facts F1–F11, §2 D1–D7, §3 D8–D11, §4 D12–D14, §5 D15, §7
  D20b–D23, §9 D10, §10 testing, §14 risks R1–R3) and the dispatch plan
  `.artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md`
  §1/§2. Every open question is settled below with a recorded decision;
  deviations from the design round carry a recorded reason (§9).
- **Scope guard**: this chunk touches **only `taskboard-ui-slint`**
  (plus regenerated goldens). The cross-crate diff
  (`taskboard-domain`, `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`) must be empty. Non-goals (dispatch §2):
  no stack widths / `UiRequest` lane (chunk 2), no settings view, no
  enabling of `ActionId::AppSettings` (chunk 3 — the row stays
  disabled), no keymap-resolution semantics changes, no horizontal
  board scroller.

## 1. Probe results (recorded; Slint 1.18.1, testing backend)

Probed with a throwaway crate at `/tmp/slint-probe` (never committed;
raw output preserved in this section). These results are chunk inputs,
not folklore — stage 2 may cite them.

| # | Question | Result |
|---|---|---|
| P1 | Sibling hit-test order (D2) | The **later-declared sibling is hit-tested first**. A full-window `TouchArea` declared *before* a panel receives exactly the clicks the panel does not absorb ("layer" only outside, "panel" only inside; no double delivery). The D2 dismissal layer works as designed in markup order `views → dismissal layer → menu panel`. |
| P2 | Flickable-hosted free-child panel width (R2/F3) | Reproduced the overflow: `min-width` only ⇒ panel takes the **parent's full width** (400 on a 400 window; at `x: 64` it overflows by 64). `max-width` alone is **ignored** (the in-file caveat is real: still 400). A **definite `width:` binding is honored** (`width: max(100px, min(preferred-width, 260px))` ⇒ 260 while `preferred-width` reports the natural 545). **R2 is closed: no re-host needed.** Additionally: Slint rejects declaring `width` and `min-width` on the same element — the floor must be folded into the width expression. |
| P2b | Narrow fallback of the full formula | `width: min(root.width − x − margin, max(floor, min(preferred, cap)))` clamps correctly: at a 120-px window the panel realized 88 = 120 − 24 − 8. |
| P3 | Right-button `pointer-event` (R3/D13) | `TouchArea.pointer-event` **fires** on a dispatched right press: `kind` (down/up) and `mouse-x`/`mouse-y` (correct local coordinates) are delivered, **but the testing backend reports `button == PointerEventButton.other`** for `PointerEventButton::Right`. Right-button identity is **not observable headlessly**. Consequence: the D13 journey drives the context menu through its Rust seam callback; real right-click behavior is a Tier-4/winit probe (stage 2). R3 stays open until that probe; the design's `button == right` seam is kept. |
| P4 | Window `min-width/min-height` vs testing backend | `set_size(300×200)` on a window declaring `min-width: 500px / min-height: 400px` is **not clamped** (size stays 300×200). Headless Tier-2 tests **cannot** assert the floors; the floors are a markup invariant + Tier-4/winit item. R1 needs the real-backend probe (§6.15). |
| P5 | Wrapped tooltip sizing (D9) | `wrap: word-wrap` does **not** change `preferred-width` (391 either way) — the cap math can use the natural width safely. `preferred-height` **reflects wrapping only when the Text's width is explicitly bound** (99 px at explicit `width: 120px`; 74 px at a bound 150-px panel width); a free implicit child evaluates `preferred-height` at its natural width (41 px — wrong for a wrapped panel). Pattern settled: the panel hosts a `VerticalLayout`; panel `width = min(layout.preferred-width, cap)`; the wrapped `RoleText` binds `width: root.width − 2·pad` explicitly; panel `height = layout.preferred-height`. |

## 2. Design settlements — menu system (design §2)

### 2.1 D1 — hamburger toggles; entries recomputed on every open

`install_toolbar_seam` (`adapter.rs:488-495`) becomes a stateful
toggle, still routing actions (every click affordance has an action
equivalent, §7.1):

```rust
window.on_toolbar_menu_clicked(move || {
    let Some(window) = weak.upgrade() else { return };
    let mut a = adapter.borrow_mut();
    if a.shell.menu.open {
        a.route(&window, ActionId::MenuDismiss); // → CloseMenu, full close
    } else {
        a.route(&window, ActionId::AppMenu);     // → OpenMenu
    }
});
```

Consequence (F1's root cause removed): `apply_effect`'s OpenMenu arm
can now only fire while the menu is closed, so `self.shell.chain()` at
that moment **is** the pre-menu chain — the "entries computed for the
pre-menu chain" comment (`adapter.rs:313-315`) becomes true on every
open, and the second open reproduces the first open's entries
deterministically (same chain ⇒ same projection). The keyboard path is
unchanged: Global is masked while Menu is open (`presets.rs:52-57`),
so `app.menu` can never re-fire from keys. **Decision**: no defensive
guard inside the OpenMenu arm — the toggle is the mechanism; the
journey pins the same-entries property.

### 2.2 D2 — outside-click dismissal layer (probe-verified)

Markup order inside `key-root` (`ui/app-window.slint`), after the
chrome `VerticalLayout`:

1. `if (KeyboardState.menu-open): MenuDismissal` — a full-window
   `TouchArea` (`width: 100%; height: 100%`), `clicked =>`
   a new pure callback `menu-dismiss-requested()`. No visual.
2. the existing `if (KeyboardState.menu-open): ActionMenu { ... }`.
3. the existing tooltip host stays last (tooltip has no TouchArea; it
   never steals clicks).

P1 verified this exact sibling order: the layer receives exactly the
clicks the panel misses; the panel keeps its own clicks. The Rust seam
`menu-dismiss-requested()` routes `ActionId::MenuDismiss`
(`CloseMenu`, full close — a click outside is a dismissal, not a
level-up). A stolen hamburger click closes via the layer, a
toolbar-delivered one via D1 — both converge on CloseMenu, per design.
The layer takes an `init => TooltipState.unhover();` like the panel.

### 2.3 D3 — `menu_path_stays_live_mid_edit` reconciliation (the precise reading)

The test (`lib.rs:1313-1335`) currently asserts exactly two things:
(i) `get_menu_open()` after `invoke_toolbar_menu_clicked()` while a
create flow holds insert mode, and (ii) `quit_requests > 0` after
`invoke_menu_entry_invoked(exit_index)`.

**The real invariant it guards**: *the action layer stays live and
effective while an edit flow holds* — the menu opens over the flow and
a row invocation still dispatches over the flow. It does **not**
actually assert the menu stays open after the mouse invoke (F2's
"pins current behavior" is implicit: the test simply never looked).
Nothing in it needs weakening.

Reconciliation (same test, same name — it remains truthful):

1. Keep (i) and (ii) verbatim.
2. **Add** after (ii): `assert!(!keyboard.get_menu_open())` — pins D3's
   menu-closes-after-mouse-invoke (the F2 fix).
3. **Add**: `assert!(create_open …)` — the menu lifecycle left the
   underlying edit flow intact (the flow survives open/invoke/close;
   refresh rule 4 territory, now pinned).

The keyboard-close pin (`lib.rs:718-720`,
`injected_keys_open_navigate_and_invoke_the_menu`, final
`assert!(!menu_open)` after Return) is **extended, not weakened**: the
curation moves `app.exit` off index 0 (§3.1), but that test only
asserts labels are non-empty and invoke closes — both stay true; no
change required beyond what §3.1's snapshot order demands. The ten-step
journey reconciliation is §5 item J-8.

### 2.4 D4 — panel bounded on all four edges (probe-settled formula)

`ActionMenu` instantiation in `app-window.slint` (replacing the free
`x/y` + `available-height` host):

- **Anchor**: `Toolbar` exposes the hamburger button's window rect as
  out properties (`menu-anchor-x/y/width/height`, bound to the button's
  `absolute-position`/size — the same window-coordinate space the
  tooltip trigger already uses).
- **Preferred position**: right-align the panel's right edge under the
  button: `x0 = anchor-x + anchor-width − panel-width`,
  `y0 = anchor-y + anchor-height + space-1`.
- **Width** (P2/P2b-verified shape; one expression — `width` and
  `min-width` cannot coexist):

  ```slint
  width: min(root.width - 2 * Theme.space-2,
             max(320px * Theme.base-scale,
                 min(self.preferred-width, 560px * Theme.base-scale)));
  ```

  floor = today's `min-width` (320·scale); cap = 560·scale (above the
  measured natural 545 at scale 1, so current content is uncapped; the
  cap only bounds pathological rows). The leading `min` is the narrow
  fallback: when even the floor cannot fit, the panel spans window
  width minus `space-2` margins. **No horizontal clipping, ever.**
- **x/y clamp** (all four edges, status bar included):
  `x = max(m, min(x0, root.width − panel-width − m))`,
  `y = max(m, min(y0, status.y − panel-height − space-1))` with
  `m = Theme.space-2`; the existing
  `height: min(preferred-height, available-height)` binding (e911e16)
  is kept and `available-height` continues to derive from the final
  `self.y`.
- Test observation seam: `ActionMenu` exposes
  `out property <length> panel-width / panel-x / panel-y` as rendering
  facts (the `compact-active` precedent) so journeys can assert
  containment at several `set_window_size` sizes.

### 2.5 D5/D7 — registry metadata shape

`ActionEntry` gains two presentation-only fields; keymap resolution
(code paths and data) never reads them — `Keymap` is built from preset
tables only, so the "resolution order untouched" guarantee is
structural:

```rust
pub struct ActionEntry {
    // ... id, label, contexts, category, enabled (unchanged) ...
    /// Presentation-only display priority (D5): lower = earlier.
    /// Spaces of 10 leave room for future insertions.
    pub menu_rank: u16,
    /// One or two sentences: what the function does and its effect;
    /// disabled rows state that they are not yet available (D10).
    /// Rendered as the row tooltip (D8 trigger) and a11y description.
    /// Empty is allowed by the type; the registry review requires
    /// every row to carry one.
    pub description: &'static str,
    /// The submenu that owns this row in the two-tier menu (D6/D10);
    /// `None` = top-level. Presentation-only.
    pub menu_group: Option<MenuGroup>,
}
```

```rust
/// The two-tier menu's submenu vocabulary (D6/D10; stage-1
/// finalization of "grouping stage 1 finalizes").
pub enum MenuGroup { Task, BoardStack, ViewNav, Mode, Help }
```

with `label()`: "Task actions", "Board & stack", "View & navigation",
"Mode", "Help", and a declaration-order rank used only to order parent
rows.

**MenuEntry / model changes** (`src/menu.rs`,
`ui/compositions/action-menu.slint`):

```rust
pub struct MenuEntry {
    pub action: Option<ActionId>,   // None for a synthesized parent row
    pub label: &'static str,
    pub description: &'static str,
    pub enabled: bool,
    pub hint: Vec<String>,
    /// Some(group) for a submenu-parent row (activating it opens the
    /// group's page; it never dispatches).
    pub submenu: Option<MenuGroup>,
}
```

- `section_title` and its category-boundary derivation are
  **superseded** by the submenu mechanism (the design's own
  replacement for "one long list with headers"); the field is removed
  from `MenuEntry` and from the slint `MenuEntryModel`. The existing
  section-derivation tests are replaced by the group-projection tests
  (§5 T-4) — a recorded supersession, not a weakening (the guarded
  property "the menu is grouped, derived, never hand-written" is
  re-expressed by the submenu tests).
- The slint `MenuEntryModel` becomes
  `{ action: string, label: string, description: string, enabled: bool,
  hint: [string], submenu: bool }` (`action` is `""` for parents;
  ids remain the tests' addresses).

### 2.6 D6 — two-tier MenuState, both presentation modes

`MenuState` (`src/shell.rs`) extension:

```rust
pub struct MenuState {
    pub open: bool,
    pub selected: usize,      // the cursor on the ACTIVE page
    pub len: usize,           // the ACTIVE page's entry count
    /// Some(group): a submenu page is active (replace/fly-out);
    /// None: the root page.
    pub active_group: Option<MenuGroup>,
    /// The root page's cursor, preserved across a submenu round-trip.
    pub root_selected: usize,
}
```

**Decision (recorded deviation)**: no fly-out anchor lives in
`MenuState` — the fly-out's anchor is presentation geometry (the
activated row's position inside the panel), derivable in the markup;
Rust state would duplicate it.

Projection (`src/menu.rs`):

- `pub fn root_page(keymap, chain) -> Vec<MenuEntry>` — top-level
  quick rows in `menu_rank` order (their `menu_group` is `None`),
  then submenu-parent rows in group order — **a parent renders only if
  its group's page is non-empty for the chain** — then `AppExit`
  pinned last regardless of `menu_rank` (D5).
- `pub fn group_page(keymap, chain, group) -> Vec<MenuEntry>` — the
  group's chain-visible rows in `(menu_rank, registry order)`.
- `pub fn context_entries(keymap, chain) -> Vec<MenuEntry>` — the D13
  card menu: rows whose contexts intersect the chain **and**
  `menu_group == Some(Task)`, flat (no parents), no Quit pin.
- `Keymap::first_binding_display` stays the hint source; section
  titles are gone.

Keyboard semantics (shell + adapter):

- `UiEffect` gains **`MenuBack`** (recorded decision): the router maps
  `MenuDismiss → UiEffect::MenuBack` (level-aware), while
  `CloseMenu` remains the *full* close used by the dismissal layer and
  close-on-invoke. `shell.apply(MenuBack)`:
  `active_group.is_some()` ⇒ clear `active_group`,
  `selected = root_selected` (cursor back on the parent row); else ⇒
  full close (MenuState::default).
- Activating a parent row (keyboard `InvokeSelected` or click):
  the adapter sees `entry.submenu = Some(group)`, sets
  `root_selected = selected`, `selected = 0`,
  `active_group = Some(group)`, computes `group_page` into the
  submenu model, and syncs. A parent row **never dispatches**.
- `MenuNext`/`MenuPrevious` operate on the active page's len (the
  adapter passes the active page's len to `shell.apply`).
- Presentation (existing `Responsive` breakpoint):
  - **regular/expansive ⇒ fly-out**: the root panel stays; a second
    panel mounts at the root panel's right edge
    (`x = root-panel.x + root-panel.width + space-1`, `y` aligned to
    the activated parent row, clamped by the §2.4 formula against the
    window rect), rendering the group page. The activated parent row
    carries a static highlight (`menu-active-group` index), not the
    cursor.
  - **compact ⇒ replace**: only the active page renders, with a back
    row at the top — an `AccessibleRole.button` labelled "Back"
    (address, not assertion text), `clicked =>` a
    `menu-level-back()` pure callback → adapter applies `MenuBack`.
    The back row is chrome above the list; the cursor only ever
    addresses entries.
  - Which mode: the composition reads `Responsive.current-breakpoint`
    directly (the `compact-active` precedent).
- Escape at root closes (unchanged); Escape at a submenu returns one
  level (`MenuBack`) — the level-by-level acceptance item.
- Invoke/outside-click close from any level (full close; the root page
  model is cleared with the state).

`KeyboardState` additions (`sync` + imperative pushes at page
switches): `menu-active-group: int` (−1 = root, else the root-page
index of the open parent row), `menu-submenu-entries:
[MenuEntryModel]` (empty at root). `menu-entries` keeps holding the
root page. `menu-selected-index` keeps being the active page's cursor.

### 2.7 Curation (D10) — the full table

Top level, in order (disabled rows stay visible and greyed with
descriptions saying they are not yet available):

| # | Row | Registry id | State |
|---|---|---|---|
| 1 | Open settings | `app.settings` | disabled (chunk 3 enables) |
| 2 | Quick capture | `capture.quick` | disabled (backlog #347) |
| 3 | Undo | `app.undo` | disabled (backlog #356) |
| 4 | Redo | `app.redo` | disabled (backlog #353) |
| 5 | ▸ Task actions | — | submenu (Task) |
| 6 | ▸ Board & stack | — | submenu (BoardStack) |
| 7 | ▸ View & navigation | — | submenu (ViewNav) |
| 8 | ▸ Mode | — | submenu (Mode) |
| 9 | ▸ Help | — | submenu (Help) |
| 10 | Quit taskboard | `app.exit` | enabled, **pinned last** |

Group membership (rows not listed at top level; within a group:
`menu_rank`, then registry order):

| Group | Rows |
|---|---|
| Task | `task.create`(60), `task.edit`(70), `task.complete`(80), `task.move`(90), `task.delete`(100), `reminder.snooze`(110), `reminder.mute`(120) |
| Board & stack | `board.next`(60), `board.previous`(70), `board.select`(80), `stack.create`(90) |
| View & navigation | `palette.search`(50, disabled reserved), `nav.next-view`(60), `nav.prev-view`(70), `nav.focus-board`(80), `nav.next-item`(90), `nav.prev-item`(100) |
| Mode | `mode.insert`(60), `mode.normal`(70) |
| Help | `app.menu`(60), `app.help`(70, disabled reserved) |

Top-level `menu_rank`s: settings 10, quick capture 20, undo 30, redo
40; every other row's rank orders within its group. `app.exit` carries
`menu_group: None` and rank 1000, but the last-row guarantee is the
projection rule, not the number (Tier-1 test).

**Recorded stage-1 curation decisions** (D10 delegates "grouping
stage 1 finalizes so no submenu is a long list again"): five groups —
the two `mode.*` rows get their own 2-row group rather than
overloading Help; `palette.search` joins View & navigation (a
view-level tool) so the top level stays exactly D10's four quick rows
+ parents + Quit. Group pages over the Board chain are 2–7 rows — no
submenu is a long list.

## 3. Design settlements — tooltips (design §3)

### 3.1 D8 — generalized trigger

`TooltipState` (`ui/widgets/tooltip.slint`) API becomes:

```slint
public function hover(label: string, description: string,
                      keys: [string],
                      x: length, y: length, w: length, h: length)
```

Empty `label` ⇒ suppressed (existing rule, unchanged). Empty
`description` / empty `keys` ⇒ those parts simply do not render.
`unhover()` unchanged. The delay machinery (`lib.rs:198-232`,
invocation-restart semantics, `TOOLTIP_DELAY_MS = 600`) is untouched;
menu mount still dismisses a pending tooltip.

**`binding_hint` resolution path (recorded deviation)**: D11's named
helper already exists — `Keymap::first_binding_display(action,
visible_chain)` (`src/keymap/mod.rs:119-138`, Tier-1 tested). No
wrapper is added; a wrapper would be an untestable re-implementation
of production code (AGENTS §5). Resolution is per element:

- menu rows: the entry's own `hint` (already pushed per row);
- hamburger: the toolbar's existing `menu-hint` push, threaded into
  the IconButton;
- back button, board-bar segments, task cards: a `hint-keys`
  property on the element, filled from a **single window-level
  property per element class** pushed once per refresh
  (`push_chrome`-style: `nav-back-hint` = `nav.focus-board`,
  `board-segment-hint` = `board.select` (no preset binding ⇒ the row
  is absent ⇒ no hint row, per "shown only when a binding exists"),
  `task-card-hint` = `task.edit`) — resolved against the current
  visible chain at refresh time, never hand-written.

**Adopters** (D8): IconButton (new `description: string` and
`hint-keys: [string]` properties, empty defaults — existing call
sites unchanged), board-bar segments (D21b), menu rows (D7), task
cards (full title + task.edit hint), the back button (D12). Stack
headers are chunk 2's adopter (D17/D19); the API lands ready for them.

### 3.2 D9/D10 — size-to-content, wrap, in-window placement

- Panel structure (P5-settled): `TooltipPanel` hosts a
  `VerticalLayout` (padding `space-2`/`space-1` as today) containing
  the caption `RoleText` (`wrap: word-wrap`, explicit `\n` honored)
  and, conditionally, a `KbdHint` row. The text binds
  `width: root.width − 2·pad` explicitly.
- **Width cap numbers (stage-1 tuning)**:
  `width: min(layout.preferred-width, min(0.6 * root.width,
  420px * Theme.base-scale))` — 60 % of the window and an absolute
  420·scale cap, whichever is smaller. `height:
  layout.preferred-height` (wraps correctly per P5).
- **Placement algorithm** moves from markup expressions into a pure
  Rust module `src/tooltip.rs` (recorded decision: single-source,
  proptestable math per §10; the old markup formula had degenerated
  exactly because it was untestable):

  ```rust
  pub struct TooltipInput {
      pub anchor: (f32, f32, f32, f32), // x, y, w, h (window coords)
      pub tip: (f32, f32),              // w, h
      pub window: (f32, f32),
      pub status_top: f32,              // window-coords top of the bar
      pub margin: f32,
      pub gap: f32,
  }
  /// Returns the tooltip's top-left so the rect ⊆ window, always:
  /// horizontally centred on the anchor, clamped to
  /// [margin, window.0 − w − margin]; below the anchor
  /// (anchor.y + h + gap), flipped above when the below placement
  /// would cross status_top/window-bottom AND room exists above,
  /// else clamped inside [margin, window.1 − h − margin].
  pub fn place(input: TooltipInput) -> (f32, f32);
  ```

  Wiring: `TooltipState` gains `tip-x/tip-y: length`,
  `placed: bool`, and a callback `size-observed(length, length)`;
  `TooltipPanel` reports `changed width =>` /
  `changed height =>` → `size-observed`. Rust (the existing timer-fire
  closure and the new callback) computes `place(...)` and sets
  `tip-x/tip-y` + `placed = true`. The panel mounts on
  `TooltipState.visible && TooltipState.placed` — no first-frame
  flicker at the origin. The testing backend's mock clock delivers
  change handlers (the driver advances it), so this is deterministic
  in tests.

### 3.3 D11 — shortcut hints

Rendered as the panel's `KbdHint` row from the `keys` argument
(§3.1's resolution path); shown only when the model is non-empty; the
existing `kbd-hint.slint` styling is reused unchanged.

## 4. Design settlements — navigation & task interaction (design §4)

### 4.1 D12 — back button

- New `IconKind.chevron-left` (a theme-vocabulary decision, recorded:
  two mirrored bars like `plus`, composed from primitives).
- `TaskView` header: an `IconButton` left of the title block
  (HorizontalLayout wrapping the existing title area,
  `task-view.slint:54-65`), `label: "Back to board"`, description
  tooltip ("returns to the board" — stage 2 words it; never asserted),
  hint-keys from the `nav-back-hint` window property.
- Routing: `clicked =>` new pure callback `task-back-clicked()` →
  adapter routes `ActionId::NavFocusBoard` — the existing
  `focus_board` outcome (`view_state.rs:299-302`), no new view-state
  semantics; stale-subject reconciliation (`view_state.rs:688-704`)
  untouched.

### 4.2 D13 — task-card context menu (probe-informed)

- `TaskCard`'s `TouchArea` gains
  `pointer-event(event)`:
  `if (event.kind == PointerEventKind.down && event.button ==
  PointerEventButton.right): root.context-requested();` — P3 verified
  the callback fires with correct `mouse-x/y`; the `right` identity is
  a real-backend property (winit probe, stage 2; R3 stays open until
  then, and the design's seam is kept as-is).
- Card → window plumbing: `context-requested` callback on TaskCard;
  board-layout/stack-view pass the indices up (the `activated` 
  pattern); `AppWindow` gains
  `pure callback task-context-requested(int, int, float, float)`
  (column, row, pointer-x, pointer-y in window coords — the card
  computes `self.absolute-position + self.mouse-x/y`).
- Rust seam `install_task_context_seam`: resolve `(column, row)`
  against the id tables (stale index absorbed, the
  `board_bar_segment_activated` precedent); focus the card
  (`activate_card` — right-click selects like left-click, standard
  UX); open the menu with:
  - projection chain `[ContextId::TaskDetail, ContextId::Global]`
    (the card names a task; the menu speaks the task's context —
    design's "context chain [Task]"), projection `context_entries`
    (Task group only, flat);
  - the same `MenuState`/entries pipeline and the same
    `KeyboardState` models — **not** a second menu implementation;
    `shell.menu.open` behaves identically (modal chain `[Menu]`,
    Escape/outside/invoke all work);
  - an adapter-held anchor: `menu_anchor: enum { Hamburger,
    Pointer(f32, f32) }`, synced as
    `KeyboardState.menu-pointer-x/y` + `menu-pointer-anchored: bool`;
    the ActionMenu host positions from the hamburger rect or the
    pointer point (same clamp formula, §2.4);
  - invoke: the same `invoke_entry` path; commands route with the
    focused task set (step 1 focused it), so `task.complete` /
    `task.edit` resolve exactly as the keyboard path does.

### 4.3 D14 — double-click pinned, not re-implemented

The wiring exists (`task-card.slint:57-60` → `board_task_activated` →
`TaskEdit`). Pinning attempt: a Tier-2 journey dispatching two rapid
`PointerPressed/Released` pairs at the same position through the
driver (new `click_at`/`press_at` driver method). **Decision point
(recorded)**: if the testing backend's double-click detection does not
deliver `double-clicked` headlessly, the journey pins the single-click
focus half and the double-click half is verified manually (Tier-4
checklist, §8) with the outcome recorded in the stage-2 appendix —
this matches the design's "verifies it manually" provision. A real
pointer failure would be a chunk-1 bug fix (design D14), not new
scope.

## 5. Design settlements — window stability (design §5 / D15)

- **Floors (stage-1 numbers)**: on the root `Window`,
  `min-width: 480px * Theme.base-scale;` and
  `min-height: 400px * Theme.base-scale;` alongside the existing
  preferred bindings (1024×640·scale). Rationale: the compact layout
  (switcher row + single column of eliding cards + toolbar + status
  bar) must remain usable at 480·scale wide; the goldens exercise
  compact at 600; 400·scale height fits toolbar (64) + switcher (~40)
  + two cards + status bar. No `width:`/`height:` binding may appear
  anywhere on the Window (grep-verified in review; the code-review
  invariant).
- **Views must lay out at ≤ the floors.** Chunk-1-scope enablers:
  board-bar segments elide at a bounded height (§7.1); toolbar title
  already elides; task-card titles already elide; task-view and
  stack-view heading `RoleText`s gain `wrap: word-wrap`.
- **Recorded cross-chunk decision (D19 split)**: the board stack
  header `RoleText` gains only the mechanical `overflow: elide`
  property in this chunk — without it a long stack title forces the
  layout min above the floors and winit grows the window (F11), which
  D15 forbids in this chunk. D19's *conditional* engagement (elide
  only under user-set width, auto-layout preservation, header
  tooltip) remains chunk 2; chunk 2's "never-resized boards render
  byte-identically" baseline is therefore **post-chunk-1** rendering.
  The header tooltip is not added here (that is D19/D8 adopter work
  with the widths machinery).
- **R1 probe plan (stage 2, real winit backend)** — P4 showed the
  floors are not headlessly testable:
  1. Run the desktop app (real display or `xvfb-run`).
  2. For every view-switch pair (Board→StackDetail→TaskDetail→Board,
     both directions, including via the back button and menu
     invocations): record the window rect before/after (external
     `xdotool getwindowgeometry`; no probe code in the app).
  3. Resize to exactly the floors and to floors ± small deltas;
     confirm every view renders usable (compact checklist).
  4. Repeat at `[ui] scale` 0.5 and 3.0 once.
  5. **Fallback decision point**: if any switch still grows the
     window, apply the pre-approved fallback — per-view fixed
     preferred sizes equal to the Theme constants (1024×640·scale)
     with min floors unchanged — and record the outcome in the
     stage-2 appendix of this plan. No third mechanism is in scope.
- The Responsive breakpoint push stays as is (layout within the
  window, never the window itself).

## 6. Design settlements — board bar & add-stack (design §7)

### 6.1 D20b/D21b — fixed height, elide, segment tooltips

- `BoardBar` root: `height: Theme.space-6 + 2 * Theme.space-1`
  (40·scale — IconButton height + padding); the segment `FocusScope`s
  fill the row (`height: 100%` through the existing Rectangle). This
  removes the uncapped layout share that caused the tall whitespace
  (F7).
- Segment `RoleText`: `overflow: elide`, and the segment gets
  `max-width: 160px * Theme.base-scale` so many boards share the row.
- Segments adopt the generalized trigger (D8): hover arms a tooltip
  with label = board title (already the a11y label), description =
  what clicking does (stage-2 wording), hint-keys from the
  `board-segment-hint` window property (empty today — `board.select`
  has no preset binding — so no hint row renders; the mechanism is
  what ships).

### 6.2 D22 — add-stack relocation

- The `IconButton` is **removed** from `BoardBar` (the
  `create-activated` callback and the stretch spacer go with it); the
  board bar keeps only the board segments (D23: selector stays
  segments in v1).
- **Regular/expansive**: a trailing slim add-column affordance after
  the last stack in the columns `HorizontalLayout`
  (`board-layout.slint:91-147`): fixed
  `width: Theme.space-6 + Theme.space-2` (48·scale), full column
  height, `IconKind.plus`, `label: "New stack"` (the same address the
  existing journey clicks), `clicked => root.new-stack-activated()`.
  The empty-board branch (`board-layout.slint:65-77`) keeps its
  centered `EmptyState` + labelled button — that instance already
  exists and stays.
- **Compact** (no columns row): a trailing small `IconButton` (plus,
  same label) appended after the switcher segments in the compact
  switcher row (`board-layout.slint:159-205`) — the recorded compact
  placement; reachable, click-equivalent, and covered by a compact
  journey leg.
- All three instances route the same `stack.create` action; the
  existing `stack_create_has_a_visible_click_equal_flow` journey
  keeps passing (the label address is unchanged).

## 7. Test catalogue (TDD; AGENTS §5 / design §10)

Rules in force: no sleeps; no text assertions (a11y labels and action
ids are addresses; structural facts and typed outcomes only); the
registry insta snapshot regenerates **in the same commit** as the
registry change; higher layers test routing only; deterministic by
construction (the driver's mock clock advances timers).

### Tier-1 (pure Rust, no slint types)

| ID | Test | Pins |
|---|---|---|
| T-1 | `root_page` ordering: settings < quick < undo < redo < parents (group order) < quit; parents present iff their group page is non-empty for the chain | D5/D10 curation |
| T-2 | `app.exit` is the last root-page row for every chain (board, stack-detail, task-detail, global-only) | Quit pinned last |
| T-3 | `group_page(Task)` over `[Board, ViNormal, Global]` contains exactly the Task-group rows, enabled bits preserved, hints carried | group projection |
| T-4 | `context_entries` over `[TaskDetail, Global]` = Task group only, flat, no parents, no `app.exit` | D13 projection |
| T-5 | parent rows: `action == None && submenu == Some(group)`; label non-empty; group pages non-empty in the board chain | parent shape |
| T-6 | every `REGISTRY` row carries a non-empty `description` (structural non-empty check) | D7 totality |
| T-7 | `menu_rank`/`menu_group`/`description` appear in the regenerated `keymap_snapshots__registry.snap` (snapshot IS the test; regenerated in the registry commit) | D5/D7 documentation |
| T-8 | `tooltip::place` proptest: for adversarial anchors (corners/edges/centre) and tip sizes (incl. tip ≥ window), the placed rect ⊆ window rect (margin-aware); unit tests: flip-above when crossing `status_top` with room above; clamp when no room | D10 invariant |
| T-9 | shell: `MenuBack` at a submenu returns to root with `root_selected` restored; at root closes; `CloseMenu` always full-closes from any level | D6 levels |
| T-10 | router: `MenuDismiss → MenuBack`; menu cursor effects unchanged | effect split |
| T-11 | (reconciled) the old section-title tests are replaced by T-1/T-5 (supersession recorded in §2.5); `entries_filter_to_the_chain_and_global`, ordering/hints and the mode-context hint tests are re-pointed at `root_page`/`group_page` with the same assertions | projection rebase |

### Tier-2 journeys (`tests/journeys.rs`, driver + fake engine)

| ID | Journey | Pins |
|---|---|---|
| J-1 | hamburger toggle: open → second click closes → third click opens with the **same** entries model (row_count + per-row action-id equality — structural) | F1/D1 |
| J-2 | outside click closes: open, `click_at` a point off-panel → closed; the dismissal layer is the mechanism (z-order pinned by behavior, not comments); open again, click *on* the panel → menu state changes only via that row's semantics | D2/P1 |
| J-3 | mouse-invoke closes and routes: open on the board, click the "Edit task" row → `View::TaskDetail` **and** menu closed | D3/F2 |
| J-4 | panel containment at `set_window_size` 1000×800, 600×800, 320×400: `panel-x ≥ margin && panel-x + panel-width ≤ window − margin`; at 320 the narrow fallback spans window minus margins | D4/P2/P2b |
| J-5 | submenu navigation, both modes: open → cursor to a parent → invoke → `menu-active-group ≥ 0` + submenu model non-empty; regular/expansive: fly-out visible next to the root panel; `set_window_size(600, …)`: replace mode with the "Back" address present; `menu.next` walks the submenu; invoke a leaf → closed + routed; Escape walks level-by-level back | D6 |
| J-6 | back button: navigate to TaskDetail, `click_by_label("Back to board")` → `View::Board` | D12 |
| J-7 | context menu: `invoke_task_context_requested(0, 0, x, y)` on a populated board → menu open, entries = Task group (T-4's shape at runtime), invoke "Complete task" → `SetTaskDone { id: <card's task>, done: true }` at the boundary + menu closed | D13 |
| J-8 | (reconciled) ten-step session step 8: with the curated order, the cursor walks to the last row (Quit) and invokes it — the keyboard re-dispatch path stays pinned (close + quit counter increments; step 10's assertion becomes counter = 2) | curation order |
| J-9 | (reconciled) `Harness::press` resolves bindings via `keymap.first_binding_display` + registry `enabled` instead of the retired flat `menu::entries` (the harness gets *simpler*; no assertion changes) | harness |
| J-10 | double-click pin: rapid paired presses at a card → TaskDetail (decision point §4.3 if the backend refuses; single-click focus + Tier-4 manual fallback recorded) | D14 |
| J-11 | compact add-stack: `set_window_size(600, 800)` → `click_by_label("New stack")` opens the stack-create flow (the relocated compact affordance) | D22 |
| J-12 | (existing, reconciled) `stack_create_has_a_visible_click_equal_flow` — unchanged assertions; the button's new home resolves the same label. `the_ten_step_keyboard_session` and `the_view_graph_walks…` — only the step-8 target and nothing else change. `menu_path_stays_live_mid_edit` extended per §2.3 (in `lib.rs`, component-tier) | no regressions |

### Component-tier (lib.rs / usability_components.rs)

- The tooltip state-machine test keeps its assertions with the new
  `invoke_hover(label, description, keys, …)` signature (reconciled
  call sites); new: hint keys render iff non-empty (model read);
  `placed` precedes `visible` mounting; the size-observed → placement
  hand-off lands the panel inside the window for an over-wide label at
  a small window (structural read of `tip-x/tip-y`).
- Menu row tooltips: a hover on a row arms `TooltipState` with the
  entry's description non-empty (structural), keys = the entry's hint.

### Tier-3 goldens

The six `board-<scale>-<width>.png` goldens change with the board-bar
rework and the trailing add-column affordance; regenerate via the
existing reviewed-regen flag **in the board-bar commit**, and have the
new goldens reviewed (stage 3's visual pass) before shipping.

### Tier-4 manual checklist (recorded in the stage-2 plan appendix; copied to the PR body)

window-rect stability across all view-switch pairs at the floors and
at preferred size (R1 probe §5), resize below the floors rejected by
the WM, right-click context menu on a real pointer (R3), real
double-click, tooltip visual pass (wrap, cap, flip, hint row) at
scales 0.5/1/3, narrow-menu fallback, kiosk fullscreen unaffected,
compact add-stack reachability.

## 8. Implementation order (each commit standalone: builds + tests green)

1. `feat(ui-slint): curate the action registry with menu ranks, groups and descriptions` — `actions.rs` fields + values (§2.7), `menu.rs` projection split (root/group/context), snapshot regen, T-1…T-7, T-11, J-8, J-9.
2. `feat(ui-slint): hamburger toggles the action menu` — seam change (§2.1), J-1.
3. `feat(ui-slint): dismiss the action menu on outside clicks` — dismissal layer + `menu-dismiss-requested` (§2.2), J-2.
4. `feat(ui-slint): close the action menu on mouse invocation` — `invoke_entry` close-first (§2.3), `menu_path_stays_live_mid_edit` extension, J-3.
5. `feat(ui-slint): bound and anchor the action menu panel` — D4 formula + Toolbar anchor + observation properties + J-4.
6. `feat(ui-slint): two-tier action menu with fly-out and replace modes` — MenuState/`MenuBack`/page models/fly-out+back-row (§2.6), T-9, T-10, J-5.
7. `feat(ui-slint): generalized tooltip trigger with bounded placement` — TooltipState API, `src/tooltip.rs` + proptests, IconButton adopter, existing tooltip tests reconciled (§3).
8. `feat(ui-slint): describe and hint the menu rows` — descriptions + row tooltips + a11y descriptions (needs 7).
9. `feat(ui-slint): back button on the task view` — IconKind.chevron-left + routing + hint + J-6.
10. `feat(ui-slint): right-click task context menu` — pointer-event seam + anchor + J-7 (+ J-10 attempt).
11. `feat(ui-slint): single-height board bar with elided, hoverable segments` — D20b/D21b + add-stack relocation incl. compact + goldens regen + J-11.
12. `feat(ui-slint): window min floors and elide enablers` — D15 floors + stack-header elide enabler + view wraps; R1 winit probe executed (stage 2) and the checklist appended to this plan.

Gates before handoff (AGENTS §10, all green): fmt, clippy
`-D warnings`, nextest, doc-tests, deny, the three-command llvm-cov
sequence + quarantine script + `rm -rf target/llvm-cov-target`.
Cross-crate guard: `git diff main...HEAD -- crates/taskboard-domain
crates/taskboard-state crates/taskboard-storage-sqlite
crates/taskboard-sync-nextcloud` empty.

## 9. Deviations from the design round (all recorded, with reasons)

1. **No fly-out anchor in `MenuState`** (§2.6) — presentation
   geometry, derivable in the markup; Rust state would duplicate it.
2. **Five submenu groups, not four** (§2.7) — D10 delegates grouping
   finalization to stage 1; `mode.*` rows get their own group rather
   than overloading Help; `palette.search` joins View & navigation so
   the top level stays exactly D10's four quick rows + parents + Quit.
3. **`binding_hint` = the existing `Keymap::first_binding_display`**
   (§3.1) — the named D11 helper already exists; a wrapper would be a
   test-free re-implementation (AGENTS §5).
4. **`section_title` mechanism superseded by submenu groups** (§2.5) —
   the design's own replacement for the one-long-list problem; tests
   reconciled, not weakened.
5. **`UiEffect::MenuBack` added; `MenuDismiss` maps to it** (§2.6) —
   the dismissal layer and close-on-invoke need a full close while
   keyboard Escape must be level-aware; one effect cannot be both.
6. **Tooltip placement moves from markup expressions to pure Rust**
   (§3.2) — D10 requires the rect ⊆ window property test; single-source
   math in `src/tooltip.rs` with a `size-observed` hand-off.
7. **Board stack-header `overflow: elide` lands in chunk 1** (§5) —
   the floors cannot hold against unbounded titles without it; D19's
   conditional engagement/tooltip/auto-layout-preservation stay chunk
   2, whose byte-identical baseline becomes post-chunk-1 rendering.
8. **IconKind gains `chevron-left`** (§4.1) — the closed vocabulary
   grows by a theme decision, as its rule requires.
9. **J-8 moves the ten-step journey's invoke target to the Quit row**
   (§7) — the curated order moves `app.menu` off the early rows; the
   keyboard re-dispatch invariant stays pinned.
10. **P3/P4 probe consequences** (§1): right-button identity and
    window floors are not headlessly observable — context-menu and
    floors verification ride the callback seam (headless) plus the
    winit Tier-4 probe (stage 2). The design's seams are kept.

## 10. Risks

- **R1 (window growth, winit-specific)** — probe plan + fallback
  decision point fixed in §5; stage 2 records the outcome in the
  appendix before stage 3 ships.
- **R2 (Flickable constraints)** — **closed by probe P2**: the
  definite-width binding bounds the panel; no re-host.
- **R3 (right-button support)** — `pointer-event` fires headlessly
  (P3) but button identity needs the winit probe; a failure there is a
  recorded design deviation with a keyboard-accessible fallback
  decision, not a silent drop.
- **R5 (text-bearing additions invite text assertions)** — every new
  string (labels, descriptions, tooltips) is asserted only
  structurally (non-empty/visible/rect-bounded); the catalogue above
  contains no text equality on user-visible strings; action ids and
  a11y labels appear only as addresses.
- New: **tooltip placement hand-off** depends on change-handler
  delivery under the mock clock (driver `advance_time(0)`) — covered
  by the component tests in step 7 before any adopter lands on it.
- New: **golden regeneration** touches six reviewed artefacts — the
  regen commit is isolated (step 11) so review sees exactly the
  board-bar delta.

## 11. Estimated diff

~1.6–2.0k changed lines across ~20 files: `src/actions.rs` (+~130),
`src/menu.rs` (rewrite, ±150 net), `src/shell.rs` (+~70),
`src/adapter.rs` (+~140), `src/router.rs` (+~5),
`src/tooltip.rs` (new ~120), `src/lib.rs` (+~90),
`src/models.rs` (+~40), `ui/*.slint` (+~260/−~50 across app-window,
action-menu, toolbar, board-bar, board-layout, task-view, stack-view,
task-card, tooltip, icon-button, theme), tests (+~550), snapshot +
6 goldens regenerated. No app-crate, domain, state, storage, or sync
changes.

## Appendix A — probe raw output (Slint 1.18.1, testing backend, /tmp/slint-probe)

```text
P1  inside panel  -> last: "panel"          (panel's TouchArea only)
P1  outside panel -> last: "layer"          (full-window layer below the panel)
P3  right-button  -> last: "---oth:down:mx=50:my=30"   (button reported as `other`)
P4  size after set 300x200 on a min-500x400 window: PhysicalSize(300, 200)  (no clamp)
P2  bare   : w=400 pref=545 min=100 h=300   (min-width only ⇒ parent width: the overflow)
P2  capped : w=260 pref=545 min=0   h=300   (width: max(100, min(pref, 260)) honored)
P2  maxed  : w=400 pref=545 min=100 h=300   (max-width alone ignored)
P2b at 400 wide: panel w=260; at 120 wide: panel w=88 (= 120 − x 24 − margin 8)
P5  nowrap-pref-w=391 wrap-pref-w=391; free-implicit host h=41 (natural-width height)
P5  explicit-width text: prefh@120px=99; bound-width host2: w=150 h=74; layout host3: w=150 h=107
```

(Probe crate build notes: Slint rejects `width` + `min-width` on one
element; `PointerEventButton` has no `none` member in markup —
`other` exists. Both folded into the formulas above.)

## Appendix B — stage-2 fill-in section (left empty by this plan)

- R1 winit probe results (view-switch × size matrix, floors usable,
  fallback applied or not).
- R3 winit right-click result.
- D14 double-click journey outcome (or the recorded Tier-4 fallback).
- The manual/Tier-4 checklist results for the PR body.
