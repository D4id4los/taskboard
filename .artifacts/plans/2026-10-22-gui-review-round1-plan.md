# [Plan] GUI Review Round 1 — Design Round (menu, tooltips, navigation, window stability, stack sizing, settings)

- **Date**: 2026-10-22
- **Status**: PROPOSED — written from the user's 2026-10-22 GUI review
  findings (17 items, reproduced verbatim in §1.0); dispatchable once
  the user accepts this round. The companion dispatch plan
  (`.artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md`)
  chunks it into **exactly 3 PRs** (user decision: minimize PR count
  this round).
- **Scope guard**: this round touches only `taskboard-ui-slint` and
  `taskboard-app` (+ config). **No `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, or `taskboard-sync-nextcloud` changes** —
  §6 deliberately routes persistence around the synced domain shape.
  Reviewers assert the empty diff over the other four crates.

## 1. Context & verified facts (anchored, do not re-derive)

### 1.0 The user's findings (2026-10-22 review, verbatim topics)

1. Settings menu including sync connection management (the biggest item).
2. Clicking outside the opened main menu does not close it.
3. Main menu is cut off at the window's right edge.
4. Categories help but the menu is still one long list; need a split
   into top-level items (undo, redo, open settings, quick capture, quit)
   vs. category sub-menus (fly-out to the side on wide windows,
   replace-contents-with-back-button on narrow ones).
5. Menu needs user-visible ordering by priority (Quit Taskboard last, not first).
6. Second hamburger click *changes the menu's entries* instead of closing it.
7. Hover-text element is huge, overflows the window right/bottom; must
   size to content, support line breaks, stay inside the window, and
   position relative to the hovered element.
8. Selecting some menu functions (e.g. Edit Task) does not close the menu.
9. Edit-task view reachable only via shortcut/menu; should also open on
   task double-click and a task right-click context menu.
10. Edit-task view has no visible back navigation; want a "<" button left
    of the task title with hover text.
11. Switching views can change the app window size; only explicit
    window-management actions may ever resize the window.
12. Stack columns need horizontal resize (mouse + keyboard), saved
    per-board across restarts.
13. The tall "Welcome to Nextcloud Deck!" button — is it the board
    selector? Too much vertical whitespace, no hover text.
14. "+ New Stack" button sits in the board-title row; belongs in the
    stack-title row.
15. Hover texts lack keyboard-shortcut information.
16. Menu items need descriptive hover texts explaining what each does.

Answer to 13: **yes, it is the board selector** — `BoardBar` renders one
tab-segment per live board (F7); the redesign keeps segments (D22–D25).

Answer to 9 (partial): **double-click already exists in code**
(F9) — it is pinned by a test and manually verified in chunk 1 rather
than re-implemented (D14).

### 1.1 Facts — menu

- **F1 Hamburger open is not a toggle.** The toolbar button routes the
  `AppMenu` action directly through `install_toolbar_seam`
  (`crates/taskboard-ui-slint/src/adapter.rs:488-495`) →
  `ActionRouter` maps `AppMenu → UiEffect::OpenMenu`
  (`src/router.rs:123`) → `apply_effect`'s OpenMenu arm
  (`src/adapter.rs:312-338`). That arm projects the entries from
  `self.shell.chain()`; with the menu already open, `chain()` returns
  `[ContextId::Menu]` (`src/shell.rs:44-47`), so the second click
  *regenerates the panel with the Menu-context control rows*
  (`src/actions.rs:408-435`) and stays open. The keyboard path cannot
  reproduce this (Global is masked while the Menu context is open,
  `src/keymap/presets.rs:52-57`) — only the button can.
- **F2 Mouse invoke does not close the menu.** Keyboard
  `menu.invoke` → `UiEffect::InvokeSelected` closes first, then
  re-dispatches (`src/adapter.rs:340-347`). The row-click path
  `invoke-entry` → `menu-entry-invoked` → `invoke_entry`
  (`src/adapter.rs:371-377,440-449`) only routes the action; comments
  claiming closure are wrong (`ui/app-window.slint:190-192`,
  `ui/compositions/action-menu.slint:3-5`).
  `menu_path_stays_live_mid_edit` (`src/lib.rs:1313-1335`) pins
  current behavior on this path and must be reconciled (D3); the
  keyboard-close invariant is pinned at `src/lib.rs:718-720`.
- **F3 Menu overflows the right edge.** The panel is a free child of
  the root FocusScope at `x: Theme.space-8`, `y: Theme.space-8`, with
  only `available-height` constrained
  (`ui/app-window.slint:157-168`). It binds `min-width: 320px *
  base-scale` and `height: min(preferred-height, available-height)` but
  **no width / max-width** (`ui/compositions/action-menu.slint:33,41`);
  as a free child it takes the host's full width → overflows the right
  edge by `Theme.space-8`. The in-file comment records that
  max-constraints are not honoured for the Flickable-hosted panel
  (`action-menu.slint:28-31`); content is a Flickable-hosted
  VerticalLayout (`action-menu.slint:43-46`). Commit e911e16 already
  bounded the height (scroll + indicator, `action-menu.slint:112-132`).
- **F4 No outside-click dismissal.** The panel mounts conditionally on
  `KeyboardState.menu-open` (`ui/app-window.slint:157`); nothing
  catches clicks off-panel.
- **F5 Entry generation & ordering.** `menu::entries(keymap, chain)`
  (`src/menu.rs:33-78`) filters the const registry `REGISTRY`
  (`src/actions.rs:236-450`; rows: id, label, contexts, category,
  enabled) by context-chain intersection, stable-sorts by
  `(Category, registry order)`, and derives `section_title` at category
  boundaries; category labels at `src/actions.rs:89-99`. Menu state is
  `KeymapShell.menu: MenuState { open, selected, len }`
  (`src/shell.rs:13-20`), mirrored onto `KeyboardState.menu-open`
  (`src/adapter.rs:76-82`). There is **no display-priority metadata and
  no submenu concept**; "Quit taskboard" lands early purely by registry
  order. Reserved-but-disabled rows already render greyed:
  undo/redo (`src/actions.rs:251-264`), quick capture
  (`src/actions.rs:279-285`), search/help, settings
  (`src/actions.rs:265-271`).

### 1.2 Facts — tooltips

- **F6 Tooltip system.** `TooltipState` global + `TooltipPanel` in
  `ui/widgets/tooltip.slint`; empty labels never show
  (`tooltip.slint:31-34`). The **only** trigger is `IconButton`
  (`ui/widgets/icon-button.slint:150-156`: `changed has-hover` →
  `TooltipState.hover(a11y label)`; leave/click → `unhover`). Arming is
  a 600 ms one-shot Rust timer, invocation-based so re-hover restarts
  it (`src/lib.rs:198-232`). The panel is a padded single `RoleText`
  (`Typography.caption`) with **no wrap, no overflow, no max-width, no
  explicit width** (`tooltip.slint:54-71`) — preferred width is the
  full single-line text width. Position: centred on the anchor with
  `x: max(space-2, min(center − width/2, root.width − width − space-2))`
  (`ui/app-window.slint:177-183`): once the tooltip is wider than the
  window the clamp degenerates (negative bound, `max` wins) and the
  panel runs past the right edge; vertically only the status bar is
  guarded (`ui/app-window.slint:179-182`), never the window bottom/top.
  Line breaks/wrapping are unsupported today.

### 1.3 Facts — board bar, columns, views, window

- **F7 Board bar.** `BoardBar` (`ui/compositions/board-bar.slint`) is
  the board selector: one `AccessibleRole.tab` FocusScope per live
  board; click routes `board.select` through the id tables
  (`src/adapter.rs:502-519`, one-shot `board_target` pattern
  `src/adapter.rs:63-64,128-131,505-518`). Segments carry the board
  title as a11y label (`board-bar.slint:44`) but no tooltip wiring.
  The bar fixes no height (`board-bar.slint:27`); segments stretch to
  whatever the outer VerticalLayout gives them
  (`ui/compositions/board-layout.slint:79-89`) — the tall whitespace.
  "+ New stack" is a right-aligned `IconButton` in the same bar
  (`board-bar.slint:79-84` → `board-new-stack-clicked` → `StackCreate`,
  `src/adapter.rs:524-532`); a second instance exists on the empty
  board (`board-layout.slint:65-77`). The whole bar is hidden in
  compact mode (`board-layout.slint:40-43`).
- **F8 Stack columns.** Regular/expansive renders one VerticalLayout
  per stack in a single HorizontalLayout
  (`board-layout.slint:91-147`); `StackColumnModel` is
  {title, tasks, create-open} (`board-layout.slint:21-26`). **No
  per-column width state exists anywhere** (`src/view_state.rs:60-79`);
  width is pure layout distribution, dominated by the header `RoleText`
  (`Typography.heading`, `horizontal-stretch: 1`, no wrap, no elide,
  `board-layout.slint:115-120`) — a long stack name widens its column.
  Card titles elide (`ui/widgets/task-card.slint:80`); headers do not.
  The columns row is not horizontally scrollable; no resize affordance.
- **F9 Edit-task entry points.** Double-click is **already wired**:
  `TaskCard.double-clicked → activated` (`task-card.slint:57-60`) →
  `board/stack-task-activated` → `TaskEdit`
  (`src/adapter.rs:563-571,666-673`). Also `Return` (`task.edit`,
  `src/keymap/presets.rs:81,133,170` → `Activate` →
  `view_state.activate_focused`, `src/view_state.rs:486-495`,
  `src/adapter.rs:257-261`) and the menu row
  (`src/actions.rs:363-369`). **Right-click is not handled anywhere in
  the crate** (no `pointer-event`/`PointerEventButton` use).
- **F10 No back navigation in the task view.** Only keyboard:
  `nav.focus-board` (Alt+1 standard / `gg` vi) → `focus_board`
  (`src/view_state.rs:299-302`); Escape only ends the edit flow
  (`src/view_state.rs:594-598`). Stale-subject refresh reconciliation
  pops back to Board (`src/view_state.rs:688-704`). Title block at
  `ui/views/task-view.slint:54-65`.
- **F11 Window sizing.** Only `preferred-width/height` are set, bound
  to `Theme.window-default-width/height` (1024/640 × base-scale;
  `ui/app-window.slint:58-62`, `ui/theme.slint:60-61`). **No
  min-width/height, no max, no fixed size, no geometry persistence**
  (only kiosk fullscreen at mount, `src/lib.rs:160-162`). Views mount
  conditionally in one tree (`ui/app-window.slint:35-39,68,115-140`);
  with the winit backend the window is resized when the live layout's
  min size outgrows it, and each view's computed min/preferred differs —
  that is the reported view-switch resizing. The Responsive breakpoint
  push fires on every width change (`ui/app-window.slint:204-205`,
  `src/responsive.rs:34-45`, `src/adapter.rs:455-465`), re-rendering
  compact/regular mid-switch and compounding the effect.

### 1.4 Facts — keymap, dispatch, config, backend seams

- **F12 Keymap.** Single key path: root FocusScope → `intercept-key`
  (`ui/app-window.slint:88-98,189`) → `Adapter::intercept`
  (`src/adapter.rs:87-119`) → `KeymapResolver` (`src/resolver.rs`).
  Central tables: `REGISTRY` (`src/actions.rs:236-450`; insta snapshot
  `tests/snapshots/keymap_snapshots__registry.snap` documents it) and
  preset binding tables `STANDARD`/`EMACS`/`VI`
  (`src/keymap/presets.rs:62-192`) merged into `Keymap`
  (`src/keymap/mod.rs:21-40`). A `kbd-hint` widget exists
  (`ui/widgets/kbd-hint.slint`).
- **F13 Dispatch pipeline.** `ActionRouter::dispatch` (pure,
  exhaustive; `src/router.rs:112-185`) → `DispatchOutcome::{Command(
  StateCommand), UiEffect(UiEffect), Disabled, NoTarget}`
  (`src/router.rs:76-87`); `UiEffect`s (`src/router.rs:30-72`) applied
  in `Adapter::apply_effect`/`interpret` (`src/adapter.rs:212-365`).
  UI→backend is a `tokio::sync::mpsc::Sender<StateCommand>` handed to
  `mount` (`src/lib.rs:139-145`), `try_send`-with-drop-on-full
  (`src/adapter.rs:134-140`), drained into the engine by
  `spawn_ui_command_forwarder` (`crates/taskboard-app/src/ui.rs:39-47,
  70-80`). No engine handle ever reaches the UI crate. Quit =
  `AppExit → UiEffect::Exit` (`src/adapter.rs:308-311`,
  `src/lib.rs:379-382`).
- **F14 UI is IO-free; typed settings hand-over.** `UiSettings`
  (`src/lib.rs:52-77`: scale, keymap, startup, kiosk) is resolved
  app-side (`crates/taskboard-app/src/config.rs:337-363,952-953`;
  `resolve_ui_settings` `crates/taskboard-app/src/ui.rs:186-215`) and
  handed over at mount; `Theme.base-scale` is pushed once
  (`src/lib.rs:152-155`). No UI state is ever written back today.
- **F15 State/persistence shape (why we route around it).**
  `AppState` lives in the domain crate
  (`crates/taskboard-domain/src/state.rs:25-45`) and is
  sync/merge-aware; the persisted shape is `PersistedState`
  (`crates/taskboard-domain/src/persistence.rs:77-105`); the engine
  writes through synchronously before publishing
  (`crates/taskboard-state/src/actor.rs:584-635`); every shape change
  breaks the S-series insta snapshots
  (`crates/taskboard-state/tests/snapshots.rs`). Device-local
  "memory-only overlay" precedent: `SyncStatus.counters`
  (`state.rs:57-60`). DF-2: UI-only selection never enters the domain
  (`docs/architecture.org` "Board Selection is Caller-Owned").
- **F16 Config seams.** `AppConfig` tables are
  `#[serde(deny_unknown_fields)]` and secret-free by construction
  (`crates/taskboard-app/src/config.rs:4-9,66-100`). `[ui]` section =
  {scale 0.5–3.0 validated, keymap} (`config.rs:342-351`);
  `[preferences]` documents a reserved future device-local `settings`
  territory (`config.rs:94-98`). Figment chain defaults →
  `taskboard.toml` → `TASKBOARD_*` (`config.rs:1119-1121`). A
  comment-preserving overlay writer already exists:
  `write_config_overlay` via `toml_edit`
  (`crates/taskboard-app/src/commands/connect.rs:523`).
- **F17 Connection-management building blocks all exist.**
  `[[sync.connectors]]` → `ConnectorCfg { name, backend, server_url,
  username }` (`config.rs:242-253`; legacy singleton table mutually
  exclusive, `config.rs:627-633`); secrets resolve via the
  `CredentialStore` port (keyring | env | file, `config.rs:141-155`)
  and never live in the TOML. Runtime reconfiguration is supported:
  `App::attach_connector` / `detach_connector(_named)`
  (`crates/taskboard-app/src/bootstrap.rs:314-391,205,265`),
  `SyncCommand::Detach`
  (`crates/taskboard-domain/src/messages.rs:357-366`). The REPL
  `connect`/`logout` ceremonies already do verify → config rewrite →
  detach/attach (ADR 0014); the connection test is `verify()` = build
  `DeckClient`, call `boards()`
  (`crates/taskboard-app/src/commands/connect.rs:394-404`). Live
  status: per-board `SyncPhase`
  (`crates/taskboard-domain/src/state.rs:106-119`) +
  `SystemEvent::ConnectorLost/Restored(ConnectorId)`
  (`messages.rs:331-334`).
- **F18 Threading/repaint.** The Slint event loop runs on the process
  main thread inside `tokio::block_on` (`run_desktop`,
  `crates/taskboard-app/src/ui.rs:265-322`); the UI drain is a repeated
  Slint Timer ticking over the broadcast receivers
  (`crates/taskboard-ui-slint/src/lib.rs:239-250`) → `refresh()`
  (`src/adapter.rs:195`). Sync `try_send` from UI callbacks and
  `try_recv`-style drains are the established non-blocking patterns.
- **F19 Testing tiers** (`docs/testing_strategy.org`): Tier 1 pure-Rust
  unit tests without Slint types (menu projection, rect math belong
  here); Tier 2 component journeys over `i-slint-backend-testing` with
  the engine faked at the forwarder boundary
  (`tests/support/fake_engine.rs`) — journeys assert **routing only**;
  Tier 3 golden visuals require reviewed regeneration; Tier 4 MCP/manua
  smoke. House rules: no sleeps (mock/pause time), a11y labels are
  addresses, **never assert label/message text**, typed outcomes at
  stable seams. `#![forbid(unsafe_code)]` everywhere except
  `taskboard-ui-slint`; SPDX headers per AGENTS §6.

## 2. Design A — Menu system

- **D1 (hamburger toggles).** The toolbar seam
  (`adapter.rs:488-495`) consults `shell.menu.open`: open ⇒ apply
  `CloseMenu`, closed ⇒ `OpenMenu`. Consequence of the toggle: the
  OpenMenu arm's "entries computed for the pre-menu chain" comment
  (`adapter.rs:313-315`) becomes true on every open, because OpenMenu
  can only fire while closed. Keyboard behavior is unchanged (the
  resolver already masks Global while Menu is open, F1).
- **D2 (outside click closes).** When `KeyboardState.menu-open`, mount
  a full-window dismissal `TouchArea` as a sibling **below** the menu
  panel in z-order (Slint hit-tests later siblings first) and above the
  view content; any click on it applies `CloseMenu`. Whether it sits
  above or below the toolbar is immaterial for correctness: a stolen
  hamburger click closes via the layer, a toolbar-delivered one closes
  via D1 — both paths converge on CloseMenu. Stage 1 verifies the
  actual hit-test order on the pinned Slint version and pins it with a
  Tier-2 journey.
- **D3 (invoke closes, both input paths).** `invoke_entry`
  (`adapter.rs:440-449`) applies `CloseMenu` before routing, mirroring
  `InvokeSelected` (`adapter.rs:340-347`). Reconcile
  `menu_path_stays_live_mid_edit` (`lib.rs:1313-1335`): first identify
  the real invariant it guards; the menu-stays-open aspect is a pin of
  today's behavior (AGENTS §5) and is replaced by
  menu-closes-after-activation; anything else it genuinely guards
  (e.g. edit-flow liveness across refresh) stays, under a truthful
  name. The keyboard-close pin (`lib.rs:718-720`) is extended, not
  weakened.
- **D4 (menu bounded on all sides).** The panel gets a definite
  width — `min(preferred-width, root.width − x − margin)` with a
  max-width cap — which also resolves the Flickable
  constraints-ignored caveat (F3: constraints are honoured once the
  width is definite). Anchor at the hamburger button geometry; clamp
  x/y against the window rect minus the status bar on **all four**
  edges. Narrow-window fallback: if even `min-width` cannot fit, the
  panel spans window width minus margins. No horizontal clipping, ever.
- **D5 (explicit display priority).** Registry rows gain
  `menu_rank` metadata (u16) used **only** by the menu projection;
  keymap resolution order is untouched. Projection order: top-level
  quick rows in their fixed rank order, then submenu-parent rows by
  category rank (D6), with `AppExit` pinned last regardless of rank.
- **D6 (two-tier menu, size-dependent presentation).** Categories with
  more than a trivial member count become submenu parents. Top level =
  quick items (D10) + submenu parents + Quit. Menu state
  (`MenuState`, `shell.rs:13-20`) gains the active level
  (`active_category: Option<CategoryId>` plus a fly-out anchor when
  applicable); keyboard `menu.next/previous/invoke` operate on the
  active level's entries. Presentation by the existing Responsive
  breakpoint: **regular/expansive ⇒ fly-out**: activating a parent row
  (click or keyboard) opens a second panel to the right of the main
  one, clamped like D4; **compact ⇒ replace**: activation swaps the
  panel contents, with a back row ("Back", a11y-labelled, tooltip D8)
  at the top. Escape at a submenu returns to root; Escape at root
  closes (Menu-context preset already binds dismiss). Both modes
  closed on invoke/outside per D1–D3.
- **D7 (descriptions).** Registry rows gain an optional
  `description: &'static str` (one or two sentences: what the function
  does and its effect); rendered as the row's tooltip (via D8's
  trigger) and exposed as a11y description. The registry insta
  snapshot (`keymap_snapshots__registry.snap`) is regenerated in the
  same commit.

## 3. Design B — Tooltips

- **D8 (generalized trigger).** Extract the hover→`TooltipState`
  wiring from `IconButton` (`icon-button.slint:150-156`) into a
  reusable tooltip mechanism in `ui/widgets/tooltip.slint` (wrapper
  element or global API) taking (label, description, anchor geometry).
  Adopters: IconButton, board-bar segments (D21), menu rows (D7), task
  cards, the back button (D12), stack headers (D17/D19). Empty label
  ⇒ suppressed (existing rule). Delay/timer machinery unchanged
  (`lib.rs:198-232`); menu mount still dismisses a pending tooltip.
- **D9 (size to content, wrap).** Tooltip text gets
  `wrap: word-wrap` (explicit `\n` honored); panel width =
  `min(natural text width, max-cap)` with max-cap ≈ 60% window width
  and an absolute px cap (stage 1 tunes); height = content height
  (multi-line). The panel hugs its text — no fixed large size.
- **D10 (position relative to the anchor, inside the window).**
  Because D9 caps width, the degenerate clamp (F6) cannot occur;
  re-derive placement defensively anyway: horizontal centre on the
  anchor clamped to `[margin, root.width − width − margin]`; vertical
  below the anchor, flip above when it would cross the status bar or
  window bottom and room exists above, else clamp inside. Invariant:
  the tooltip rect ⊆ window rect, always (property-tested, §10).
- **D11 (shortcut hints).** For elements backed by an `ActionId`, the
  tooltip gains a second row showing the action's **current binding**
  in the active preset, via a pure `binding_hint(keymap, action_id) →
  Option<String>` helper (Tier-1 testable) rendered with the existing
  `kbd-hint` styling. Shown only when a binding exists. No text
  assertions in tests — structural facts only (AGENTS §5/F19).

## 4. Design C — Navigation & task interaction

- **D12 (back button).** `ui/views/task-view.slint` gains an
  IconButton (chevron-left) left of the title block
  (`task-view.slint:54-65`), a11y label + tooltip description
  ("returns to the board"), routing the existing `focus_board`
  outcome (`view_state.rs:299-302`) — no new view-state semantics;
  reconciliation already guards stale subjects (`view_state.rs:688-704`).
- **D13 (task context menu).** Task cards gain right-button handling
  (`TouchArea::pointer-event`, `PointerEventButton::right` — verify on
  the pinned Slint version, R3): it opens a context menu **anchored at
  the pointer**, reusing the generated-menu projection restricted to
  Task-context actions for that card (context chain `[Task]`) — the
  same `MenuState`/entries pipeline with a position anchor, **not** a
  second menu implementation. Closes on invoke/outside/Escape per
  D1–D3, clamped inside the window like D4.
- **D14 (double-click pinned, not re-implemented).** Double-click
  already routes `TaskEdit` (F9). Chunk 1 pins it with a Tier-2
  journey (single click focuses, double click opens the task view) and
  verifies it manually; if it proves broken in practice on a real
  pointer, that is a chunk-1 bug fix, not new scope.

## 5. Design D — Window size stability

- **D15 (window never resizes itself).** Root Window gains explicit
  scale-aware `min-width`/`min-height` floors (compact layout must
  remain usable at the floor; exact numbers stage 1) alongside the
  existing preferred bindings; **no code path binds `width`/`height`**
  (code-review invariant). Every view must lay out at ≤ the floors:
  stack headers elide (D17/D19 → §6), board bar height capped (D20),
  long texts elide/wrap at min. Stage 2 verifies empirically on the
  pinned Slint version that min-size floors stop the winit
  content-driven growth (F11) across all view-switch pairs; the
  fallback, if growth persists, is per-view fixed preferred sizes
  equal to the Theme constants — decided after the probe, recorded in
  the chunk plan. The Responsive breakpoint push stays: it changes
  layout *within* the window, never the window itself.

## 6. Design E — Stack column sizing & device-local persistence

- **D16 (model).** `ViewState` gains a per-board map
  `StackId → width (u32, logical px)`; absent entries = auto layout
  (today's behavior). UI-local state; **never enters the domain**
  (DF-2, F15).
- **D17 (mouse resize).** Each stack header gains an edge drag
  affordance at the column's right header edge (pointer-resize cursor
  if the pinned Slint supports it, else a visible grip); drag sets the
  width live, clamped [min ≈ 96–120 px·scale, remaining-viewport
  share]; release persists (D19/D18 lane).
- **D18 (keyboard resize).** New registry actions
  `stack.wider`/`stack.narrower` bound in all three presets (e.g.
  Ctrl+Left/Right standard/emacs, an analogous vi pair), operating on
  the focused stack header (headers join the focus walk) in fixed
  steps; discoverable via the header tooltip incl. binding hint (D11).
  Registry snapshot regenerates.
- **D19 (elide).** Header `RoleText` gets `overflow: elide`; the full
  name is available via tooltip (D8). Elide engages under user-set
  width; never-resized boards keep today's auto distribution. No
  horizontal board scroller in this round (adjacent backlog #373).
- **D20 (persistence home = device-local config, not the domain).**
  Widths persist in the `[ui]` section of `taskboard.toml` —
  `UiSection` gains an optional `stack_widths` map
  (board-uuid → stack-uuid → px, serde-defaulted so old configs parse;
  deny_unknown_fields stays additive-strict). **Rejected**: a new
  `PersistenceAction` + migration + engine commands — the persisted
  shape is sync/merge-aware territory (F15) and a device-local
  preference does not justify opening it; the config file already
  anticipates device-local settings (F16); kiosk = one device per
  config. Consequence: **zero domain/state/storage diff this round.**
- **D21 (UI→app request lane).** `mount` gains a second typed channel:
  `mpsc::Sender<UiRequest>` (same try_send drop-on-full discipline,
  F13). `UiRequest` (defined in the UI crate, handled app-side) starts
  with `StackWidthsChanged { board, widths: Vec<(StackId, u32)> }` —
  full per-board snapshots, idempotent. The app-side forwarder task
  debounces (quiet window via injected time; `tokio::time::pause` in
  tests, never sleeps) and merges into `taskboard.toml` via the
  existing comment-preserving `write_config_overlay`
  (`commands/connect.rs:523`), pruning entries for boards/stacks no
  longer live (read-only look at `engine.shared_state()`). Load path:
  `UiSettings` carries the map to the UI at mount (F14). The UI crate
  stays IO-free — sending on a channel handle is the established
  pattern. **All overlay writes are owned by this one forwarder task**
  (the REPL ceremonies keep their own path; a single-writer rule for
  overlay merges is recorded in the chunk plan, R6).

## 7. Design F — Board bar & add-stack placement

- **D20b (fixed height).** The board bar row gets a fixed
  single-control height (≈ IconButton height + padding); segments fill
  that row, long board titles elide (full title via tooltip). This
  removes the uncapped layout share that caused the tall whitespace
  (F7).
- **D21b (segment hover text).** Segments adopt the generalized
  trigger (D8): label = board title (already the a11y label), a11y
  description + tooltip describing what clicking does (wording stage 1).
- **D22 (add-stack moves to the columns row).** The board-bar
  "+ New stack" button is removed; the trailing slim add-column
  affordance (unified with the existing empty-board instance,
  `board-layout.slint:65-77`) becomes the single regular/expansive
  entry point, sitting after the last stack — the "+" visually joins
  the stack-title row. Compact mode (no columns row): the affordance
  lives in the stack-detail header (exact placement stage 1; must
  remain reachable). The board bar keeps only the board segments.
- **D23 (selector stays segments in v1).** `BoardBar` tab-segments
  remain the board selector; a dropdown/popup redesign for many-board
  scalability is deferred (§13 backlog candidate).

## 8. Design G — Settings view & sync connection management

- **D24b (scope v1).** A new Settings surface, reachable from the menu
  row and the reserved `Ctrl+,` binding; **outside** the
  next/prev-view cycle (stage 1 models it as an overlay view or a
  `ViewId` excluded from cycling — constraint: view cycling never
  passes through it). Contents v1: (1) **Sync connections** — list of
  configured connectors (name, server, username, live status from
  `SyncPhase` + `ConnectorLost/Restored`), add / edit / remove, a
  test button; (2) **Keymap preset** (standard/emacs/vi); (3) **UI
  scale** (0.5–3.0, same validation as config.rs:342-351). Explicitly
  not the free-form `taskboard.toml` editor (backlog #351).
- **D25b (wiring over the request lane; ceremony reuse).** Settings
  operations ride the D21 lane with replies per the AGENTS §3
  query pattern (oneshot inside the mpsc payload; UI side polls via
  its existing timer drains, F18):
  `TestConnection { profile, reply }`, `SaveConnection { profile,
  reply }`, `RemoveConnection { name, reply }`, `SetKeymapPreset`,
  `SetScale`. App-side, the REPL `connect`/`logout` ceremony core
  (verify → `write_config_overlay` → `attach_connector`/`detach`,
  ADR 0014, F17) is **extracted into callable app-layer functions**
  shared by the REPL and the request forwarder — extract, don't
  rewrite; the connect/logout E2E suites staying green is the gate.
  Live status flows to the UI through the existing drains (no new
  state plumbing).
- **D26b (secrets).** The token field is write-only (never echoed,
  never persisted); the ceremony hands it to the configured
  `CredentialStore` (F17). `taskboard.toml` stays secret-free
  (config.rs:4-9).
- **D27b (apply semantics).** Keymap preset and scale apply **live**
  (UI-local: `Theme.base-scale`, `Keymap` rebuild) and persist via the
  lane; connection changes apply live via attach/detach. Nothing in v1
  requires a restart; if that changes, the UI must say so.
- **D28b (enable the entry points).** `ActionId::AppSettings`
  (`actions.rs:265-271`) flips to enabled with a router arm → new
  `UiEffect::OpenSettings`; `Ctrl+,` un-reserved in the standard
  preset; the menu row (top-level per D10) becomes live; Escape / the
  back affordance closes Settings onto the previous view.

## 9. Menu content curation (initial assignment)

- **D10 (which items where — the user's split, mechanism = D5/D6).**
  Top-level inline, in order: **Open settings** (live after chunk 3;
  rendered disabled until then), **Quick capture** (reserved-disabled,
  backlog #347), **Undo** and **Redo** (reserved-disabled, backlog
  #356/#353) — disabled rows stay visible with descriptions stating
  they are not yet available; then submenu parents (grouping stage 1
  finalizes so no submenu is a long list again): Task actions, Board &
  stack, View & navigation, Help; then **Quit Taskboard pinned last**
  (D5). The mechanism is data-driven (`menu_rank` + category
  metadata) so later rounds re-curate without layout code.

## 10. Testing strategy (normative; AGENTS §5 / F19)

- **Layer rule.** Pure logic → Tier-1 unit tests without Slint types:
  menu projection (ranking, curation, submenu tree, both presentation
  modes), tooltip rect math, `binding_hint`, width clamp math
  (proptest invariants: tooltip/column rects ⊆ window rect). Routing
  → Tier-2 journeys over `tests/support/fake_engine.rs`: hamburger
  toggle (second click closes; third click restores the same entries —
  pins F1's fix), outside-click closes, mouse-invoke closes,
  submenu navigation both modes, back button routes to board,
  right-click opens the task menu and invoke routes, double-click
  opens edit (D14). App-side: the D21 forwarder (debounce under
  `tokio::time::pause`, overlay merge preserves comments and
  unrelated keys, prunes stale ids) and config round-trips.
- **No text assertions.** Tooltips, descriptions, and menu labels are
  asserted structurally only (visible/hidden, non-empty, rect within
  bounds, typed outcomes). A11y labels are addresses, not expectations.
- **Window stability** is not CI-automatable against a real window
  manager: it is a manual/Tier-4 checklist item in chunk 1's
  acceptance plus the code invariant (no `width`/`height` bindings).
- **Existing tests to reconcile, not weaken:** `menu_path_stays_live_
  mid_edit` (`lib.rs:1313-1335`, D3), the keyboard-close pin
  (`lib.rs:718-720`), the registry snapshot
  (`keymap_snapshots__registry.snap` — regenerated with D7/D18/D28b in
  the same commits that change the registry).
- **Cross-crate guard:** chunks assert
  `git diff main...HEAD -- crates/taskboard-domain crates/taskboard-state
  crates/taskboard-storage-sqlite crates/taskboard-sync-nextcloud`
  is empty (the S-series snapshots must not move).

## 11. Code inventory (files this round touches)

- **UI crate** (`crates/taskboard-ui-slint`): `src/actions.rs`
  (registry: menu_rank, description, stack.wider/narrower,
  AppSettings enable), `src/menu.rs` + `src/shell.rs` (two-tier
  state, ranking, anchor), `src/adapter.rs` (toggle seam, invoke
  closes, OpenSettings, request lane senders), `src/router.rs`
  (UiEffect::OpenSettings, stack-width actions), `src/view_state.rs`
  (settings surface, stack widths map), `src/models.rs` /
  `src/lib.rs` (mount signature + lane, widths load), `src/keymap/
  presets.rs` (new bindings, Ctrl+,),
  `ui/compositions/action-menu.slint` (bounds, submenu/back/fly-out),
  `ui/compositions/board-bar.slint` + `board-layout.slint` (height,
  hover, add-column affordance, drag handles, elide),
  `ui/views/task-view.slint` (back button),
  `ui/views/stack-view.slint` (compact add-stack),
  `ui/widgets/tooltip.slint` + `icon-button.slint` (generalized
  trigger, wrap/cap/clamp), `ui/widgets/kbd-hint.slint` (reuse),
  `ui/app-window.slint` (dismissal layer, window floors, settings
  mount), `ui/theme.slint` (tokens as needed), new settings view
  files, plus Tier-1/Tier-2 tests.
- **App crate** (`crates/taskboard-app`): `src/config.rs`
  (`UiSection.stack_widths`, settings round-trips),
  `src/ui.rs` (request forwarder + `resolve_ui_settings` extension),
  new `src/ui_requests.rs` (or similar: debounce + overlay merge +
  prune), `src/commands/connect.rs` (ceremony extraction, unchanged
  REPL surface).
- **Untouched by design**: `taskboard-domain`, `taskboard-state`,
  `taskboard-storage-sqlite`, `taskboard-sync-nextcloud` (asserted
  empty diff).

## 12. Implementation slicing

Exactly **3 PRs** (user decision: minimize PR count), dispatched
serially, each branch off the then-current `main` after the previous
PR merged — no stacks, no rebase dance:

1. **Chunk 1 — `feat/ui-review1-menu-tooltips-navigation`** (L):
   §2, §3, §4, §5, §7, §9 — menu system (toggle/dismissal/close/bounds/
   ranking/two-tier/descriptions/curation), tooltips (trigger/wrap/
   clamp/shortcut hints), back button + task context menu (+ double-
   click pin), window floors, board bar rework + add-stack relocation.
   UI crate only.
2. **Chunk 2 — `feat/ui-stack-width-persistence`** (M): §6 — stack
   column resize (mouse + keyboard + elide) and the `UiRequest` lane
   with device-local `[ui]` persistence. UI + app crates.
3. **Chunk 3 — `feat/ui-settings-sync-connections`** (L): §8 —
   settings surface, connection management over the extracted
   ceremonies, enable `AppSettings`/`Ctrl+,`. UI + app crates.

Chunk 2 strictly after chunk 1 (shared UI seams: adapter, app-window,
theme); chunk 3 strictly after chunk 2 (it extends the chunk-2 lane).
Full prompts: dispatch plan
`.artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md`.

## 13. Backlog interactions & out-of-scope

- Existing issues this round deliberately does **not** close: #347
  (quick capture impl), #356/#353 (undo/redo v2), #351 (GUI toml
  editor — settings v1 is narrower by design), #373 (ListView
  virtualisation — horizontal board scrolling stays out), #226
  (drag-to-move), #363 (kiosk ambient rotation).
- **New backlog candidates to file after this plan merges** (one
  issue per idea, `backlog:` prefix, permalink to this file):
  board-selector dropdown/popup for many boards (D23); window-geometry
  persistence across restarts (D15 intentionally scopes to
  within-session stability).
- Round 2 of GUI review sessions intake continues per the user's
  expectation of "many more such sessions".

## 14. Risks

- **R1 Window-growth semantics are Slint/winit-version-specific**
  (F11/D15): probe empirically in chunk 1 before committing to the
  floors; fallback = fixed per-view preferred sizes.
- **R2 Flickable-hosted panel constraints** (F3/D4): a definite width
  binding must be verified to actually bound the panel; else re-host
  the panel.
- **R3 Right-button pointer events / pointer-cursor support on the
  pinned Slint version** (D13/D17): probe early; a missing cursor
  affordance is cosmetic, a missing right-button event would force a
  recorded design deviation.
- **R4 Ceremony extraction perturbs REPL flows** (D25b): extract
  without rewriting; connect/logout E2E green is the gate.
- **R5 Text-bearing UI additions invite text assertions**: reviewers
  reject any assertion on label/description/tooltip text (F19).
- **R6 Two overlay writers** (REPL ceremonies + new request
  forwarder): single-owner rule for width writes + serialized overlay
  merges recorded in the chunk-2 plan; never hold a lock across
  `.await`.
