# [Plan] GUI Review Round 2 — Design Round (board surface, interaction repairs, actions & context menus, task-view ergonomics, settings rework, connection management, visual identity, drag-to-move)

- **Date**: 2026-10-23
- **Status**: PROPOSED — written from the user's 2026-10-23 GUI review
  findings (32 items, §1.0) plus four remediation-wave-1 leftovers the
  user explicitly pulled into this round (#226, #227, #229, #232).
  Dispatchable once the user accepts this round. The companion dispatch
  plan
  (`.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`)
  chunks it into **8 PRs** (serial default; the user may direct a
  partial-parallel schedule at dispatch time, §12).
- **Scope guard**: this round touches `taskboard-ui-slint` and
  `taskboard-app` (+ config overlay writers). **`taskboard-domain` is
  touched only if a stage-1 agent verifies a missing StateCommand** —
  every lifecycle command this round needs already exists (F-set §1.6:
  `CreateBoard`/`DeleteBoard`/`CreateStack`/`DeleteStack`/
  `BindBoard`/`UnbindBoard`,
  `crates/taskboard-domain/src/messages.rs:125-250`), so the expected
  domain diff is **empty**; `taskboard-state` and
  `taskboard-sync-nextcloud` stay untouched. Reviewers assert the empty
  diff over those three crates.
- **Round-1 relationship**: this round *completes* round-1 decisions
  that shipped half-applied (R1-D15 window floors, R1-D21b tooltips,
  R1-D22 add-stack relocation) and **reverses two recorded round-1
  positions**: R1-R5 ("tones, never message text" in settings — the
  user now explicitly wants readable feedback text, F21/F14) and the
  tab-segment board selector staying visually plain (F32 asks for the
  tab abstraction; #411's *dropdown scalability* redesign stays
  deferred).

## 1. Context & verified facts (anchored, do not re-derive)

### 1.0 The user's findings (2026-10-23 review, topic list)

Screenshots referenced below live in
`/home/viktoria/Pictures/Screenshots/` (user-provided evidence; not in
the repo).

1. Default stack column width far too small ("fit for only 3 dots");
   defaults should consume the available horizontal window space.
   (Screenshot_20261023_010059.png)
2. The "New Stack" plus glyph is elongated vertically — no longer
   reads as a plus. (010059)
3. A 0-task stack's title takes far more vertical space than other
   stack titles; title rendering must not differ between 0 and >0
   tasks. (010059)
4. Selecting a 0-task stack shows no visible selection indicator.
5. Hover text no longer appears on the "New Stack" button.
6. Wants "Add Task" buttons at the bottom of every stack (mouse
   affordance), like the "New Stack" button.
7. Clicking main-menu items that open sub-menus does nothing;
   keyboard navigation works.
8. Double-click to open the Edit Task view still does not work;
   right-click context menu works.
9. Task right-click context menu lacks keyboard shortcut hints.
   (014154)
10. Stack-title and board-title buttons should get the same kind of
    right-click context menu as tasks (all object-specific functions).
11. Stack-column mouse-drag resize does not work (cursor changes,
    dragging does nothing); keyboard and main-menu resize work.
12. Adding a task to an empty stack resized the stack to the smallest
    possible width. (011015)
13. Hover text froze: clicking "Open settings" while a tooltip was
    visible left it on screen over the settings view; clicking
    non-interactive elements does not close it; hovering the hamburger
    does. (011149)
14. Selecting the Emacs or Vi keymap: (a) the selected-button
    indication does not move; (b) a green something (button? toast?)
    with no text and no apparent use appears. (011447)
15. The keymap preset chooser should be a dedicated grouped element
    ("the user forgot the name" — a **segmented control** / radio
    group) making the three-options-one-choice relationship and the
    current selection visually clear.
16. UI scale would be nicer as a slider (≈0.1 steps) plus a text box
    for keyboard users.
17. The settings view has no scroll area; a too-high scale makes it
    impossible to set the scale back within the UI.
18. Connection management presumes a single board and a single sync
    connection — both assumptions are now false (recent multi-board /
    named-connector commits). Rework: choose the local board, then
    add/remove sync connections for it; prepare the UI for future
    non-Nextcloud connector kinds.
19. Settings elements lack help hover text; proposes the classic "i"
    in a circle with descriptive hover text (what is a Nextcloud app
    password and where to create one; is the connection Name free or
    Nextcloud-dependent; etc.).
20. Settings text fields grow leftward while typing (right edge fixed,
    whole field slides/grows). (012844 → 013005)
21. Save on a new sync connection does nothing visible and logs
    nothing at info. Expected: clear visual feedback, a spinner while
    the initial connection test runs, then success or a typed error
    (bad URL format, no Nextcloud instance at URL, timeout, …), and
    the connection actually established.
22. Switching views ("Next View" menu item or shortcut) still changes
    the window size. (013619)
23. At the smallest viewport the "New Stack" button takes an entire
    vertical row for itself; it should be hidden there or inline at
    the end of the stack-title row. (013912)
24. (Emacs preset) neither Escape nor C-g cancels the "Move Task"
    action.
25. Missing Actions + keybindings for moving between stacks (next /
    previous stack); user proposes C-f / C-b for emacs.
26. Missing "New Board" and "Delete Board" actions and bindings.
27. Missing "Delete Stack" action and bindings.
28. Binding hints are not part of hover-frame sizing ("Back to board"
    tooltip too narrow for its Ctrl+X Ctrl+B chips). (014912)
29. Default reminder preset buttons need more white-space around the
    text.
30. The custom reminder offset field should share the preset buttons'
    row (unused horizontal space).
31. Commit-style text fields (reminder offset, task title, …) should
    get a checkmark at the right edge to submit with a mouse click.
32. Visual redesign: stacks get distinct card-like backgrounds with a
    strip of board background around and between them; boards render
    as tabs — active tab with rounded top corners flowing into the
    board background colour, inactive boards as distinct tab markers.

**Carried in at the user's request** (remediation-wave-1 leftovers,
`ui,p3`):

- W1 = #226 pointer drag-to-move on the board.
- W2 = #227 due-date editor widget for the task detail view.
- W3 = #229 task reopen action in the UI.
- W4 = #232 first/last-item focus navigation.

### 1.1 Facts — board surface, stack widths, headers, add-affordances

- **F-A1 Auto width is a sentinel with stretch 1; user widths pin
  absolute pixels forever.** `stack_width_px()` pushes `−1.0` for
  never-resized stacks (`crates/taskboard-ui-slint/src/models.rs:204-219`);
  the markup arm gives sentinel columns `min-width: 0` and
  `horizontal-stretch: 1`, fixed columns exact width and stretch 0
  (`ui/compositions/board-layout.slint:115-128`). Stored widths are
  keyed `(board, stack)` in logical px with **no window-size
  dimension** (`crates/taskboard-app/src/config.rs:351-357`), so a
  width captured in a small window (the floor is 96 logical px,
  `src/widths.rs:13,43-46`) renders 96·scale px on any window — at
  scale 0.5 that is 48 px ≈ three characters (F1, F12 mechanism;
  layout math fact, narrow-render arithmetic INFERENCE).
- **F-A2 `STACK_WIDTH_DEFAULT_PX = 280` is only the keyboard-resize
  seed**, never a layout default (`src/widths.rs:17`,
  `src/view_state.rs:637-639`); Theme has no column-width token.
- **F-A3 No width write exists on task commit.** `set_stack_width` has
  exactly two callers — the drag seam (`src/adapter.rs:1361`) and
  `resize_focused_stack` (`src/view_state.rs:625-643`); a task create
  emits only `SetMode(Normal)` + `RefreshNow`
  (`src/view_state.rs:678`). F12's collapse must therefore be a
  *render* consequence of F-A1's pinned widths under refresh, not a
  width write (stage-1 must reproduce headlessly anyway).
- **F-A4 The add-stack plus is two crossing Rectangle bars with an
  uncapped height**: `icon-box` height = `parent.height − 2·space-2`
  inside a full-height flex column
  (`ui/compositions/board-layout.slint:238-273`) — the vertical bar
  spans nearly the whole board height (F2 mechanism). `IconButton`
  locks its square with min/max constraints
  (`ui/widgets/icon-button.slint:45-47`) — the pattern to copy.
- **F-A5 The add-stack affordance has no tooltip** — its TouchArea
  (`board-layout.slint:242-244`) has no `changed has-hover` /
  `TooltipState.hover` wiring, unlike headers
  (`board-layout.slint:146-155`), icon buttons
  (`ui/widgets/icon-button.slint:196-202`), and board-bar segments
  (`ui/compositions/board-bar.slint:84-94`). A11y label "New stack"
  exists (249-251).
- **F-A6 The stack header has no height binding at all**; the task
  area is `alignment: start` (content height), so in the equalised
  columns row a 0-task column's surplus height is absorbed by the
  header Rectangle (`board-layout.slint:136-214`; height outcome
  INFERENCE from stretch distribution — no test pins it).
- **F-A7 Regular mode has no selected-stack visual at all.**
  `selected-stack` is consumed only by the compact switcher
  (`board-layout.slint:80-82, 305-321`); the regular header background
  is hover / move-target / transparent (183-185). Clicking a header
  does select the stack (`src/adapter.rs:1604-1617` →
  `src/view_state.rs:510-514`) — the state exists, nothing paints it
  (F4 root cause, and it is not 0-task-specific).
- **F-A8 Compact mode renders a separate add-stack `IconButton` after
  the switcher** (`board-layout.slint:345-349`); the regular
  full-height affordance stays mounted whenever the columns row is
  (F23 surface).
- **F-A9 Column chrome does not exist**: columns are bare
  `VerticalLayout`s; the only painted surfaces are the transparent-at-
  rest header and the cards; gutters are raw window background
  `Theme.background` (#14161c) with `space-3` row spacing
  (`board-layout.slint:114-116`, `ui/theme.slint:30`). The F32
  redesign starts from a clean slate — nothing to preserve.
- **F-A10 Board-bar segments** are FocusScopes with `radius-small`,
  hover/selected surface backgrounds, focus ring, 160·scale max width,
  elide, and full tooltip wiring
  (`ui/compositions/board-bar.slint:36-94`) — the F32 tab restyle
  alters paint only.
- **F-A11 Nothing pins a minimum rendered width for auto columns, the
  add-col glyph geometry, regular-mode selection visuals, or 0-task
  header height** — all unpinned surface (journeys.rs pins listed in
  `tests/journeys.rs:1169-1380`; goldens
  `tests/golden_visuals.rs:73`).

### 1.2 Facts — interaction: models, clicks, drags, tooltips, menu, window

- **F-B1 Every refresh rebuilds every model.** `push_board` assigns a
  brand-new `ModelRc` to `board-model.stacks` on each refresh
  (`src/models.rs:266-267`); Slint re-instantiates the whole
  `for column … for task …` repetition
  (`board-layout.slint:115, 215`), destroying the TouchArea that saw
  the first press. This is the shared mechanism behind:
  - **double-click never firing** — the first click's TouchArea dies
    before the second click (`ui/widgets/task-card.slint:80` wires
    `double-clicked => activated()`, but the widget instance is
    replaced between clicks; INFERENCE from the rebuild mechanics,
    consistent with the user's report), and
  - **drag dying after the first tick** — the drag handler refreshes
    per tick (`src/adapter.rs:1327-1363`: `set_stack_width` +
    `refresh`), destroying the `resize-handle` TouchArea mid-gesture
    (cursor persists because it is hover-driven; INFERENCE for the
    exact feel).
  The lane itself is connected end-to-end
  (`src/lib.rs:328-341`, `src/ui_requests.rs` app-side) — only the
  live-gesture transport is broken.
- **F-B2 The double-click journey was never machine-tested.** The only
  `double-clicked` occurrence in the repo is the handler; commit
  `c6414ea` deferred it to the Tier-4 checklist because the testing
  backend has no element geometry; the pinned test covers the
  single-click focus half only
  (`tests/journeys.rs:1109-1163`). The drag journey likewise drives
  the seams directly, never the pointer path
  (`tests/journeys.rs:1301-1350`).
- **F-B3 Sub-menu pointer clicks skip the state sync.** Both paths
  call `activate_parent` (`src/adapter.rs:593-600`), but only the
  keyboard path returns through `route`'s trailing `self.sync(window)`
  (`src/adapter.rs:256-257`) which pushes `menu-active-group` onto
  `KeyboardState` (`src/adapter.rs:130-153`); the mouse handlers
  (`src/adapter.rs:1227-1248`) call `invoke_root_entry`/`invoke_entry`
  directly. `apply_close_menu` syncs explicitly
  (`src/adapter.rs:585-587`) — leaf rows work, parent rows do not
  (F7). Second divergence: `invoke-submenu-entry` is declared and
  wired but never emitted (`ui/compositions/action-menu.slint:45`,
  `ui/app-window.slint:280` vs the universal `invoke-entry` at
  `action-menu.slint:166`); in compact replace mode the root panel
  shows the submenu page while clicks resolve against
  `self.root_entries` (`src/adapter.rs:628`) — an index-space mismatch.
- **F-B4 Nothing unhovers the tooltip on element destruction.** Hide
  paths are: trigger `has-hover` flips, `IconButton.clicked`, and
  `init` on the dismissal layer / menu mount
  (`ui/app-window.slint:234, 242`; `ui/widgets/icon-button.slint:203-204`).
  Menu-row clicks do **not** unhover
  (`ui/compositions/action-menu.slint:166`), no view-switch cleanup
  exists, and the Rust adapter never touches `TooltipState` — so a
  visible/pending tip survives the anchor unmounting (menu close →
  settings mount, F13) until some other trigger hovers. The
  hamburger's own `clicked` unhover is why hovering it clears the
  frozen tip.
- **F-B5 Tooltip frame width measures the caption only** —
  `width: min(caption.preferred-width + 2·space-2, min(window·0.6,
  420·scale))` (`ui/widgets/tooltip.slint:88-89`); the `KbdHint`
  keycap row (124-126) is excluded from the width computation and can
  overflow (F28; height does include it).
- **F-B6 No per-view window sizing exists** — Window keeps constant
  preferred 1024×640·scale and floors 480×400·scale, with the
  "no width/height binding" invariant intact
  (`ui/app-window.slint:79-90`); view switching only swaps mounted
  branches (`app-window.slint:163-211`). Yet the user still sees
  resizes (F22): the R1-D15 stage-2 fallback (per-view fixed preferred
  sizes) was **not** applied, and the residual mechanism — winit
  growing the window when a mounted view's content min-constraints
  exceed the current size (e.g. settings or task view at high scale) —
  is the working hypothesis (INFERENCE; stage-2 of the repairs chunk
  probes it empirically, as R1-D15 prescribed).
- **F-B7 The task context menu gets hint data but shows none.** Rows
  are projected through the same `row()` as the main menu with
  `hint: first_binding_display(entry.id, chain)`
  (`src/menu.rs:56-58, 113-118`), but the context menu opens over
  chain `[TaskDetail, Global]` (`src/adapter.rs:1433-1437`) while the
  task verbs' preset bindings are declared in the **Board** context
  (e.g. `task.edit` Return in Board; §1.4) — `first_binding_display`
  finds nothing chain-visible and renders empty (F9; chain/context
  mismatch is fact, the empty-hint rendering is the observed result).

### 1.3 Facts — settings view, form controls, feedback

- **F-C1 No scroll container exists in the settings view** — one
  `VerticalLayout` under a FocusScope
  (`ui/views/settings-view.slint:122-136`); the only `Flickable` in
  the UI tree is inside the action menu
  (`ui/compositions/action-menu.slint:95`). At scale 3.0 content
  exceeds the viewport with no scroll (F17).
- **F-C2 The keymap picker's "selected" state is an inverted
  `enabled` flag, set once at mount and never echoed.** Buttons:
  `enabled: SettingsModel.preset-index != N`
  (`settings-view.slint:275-295`) — the selected preset renders as the
  *disabled* button; `preset-index` is written exactly once, at mount
  (`src/adapter.rs:1205-1212`), and neither the reply drain nor
  `push_settings` ever updates it (`src/adapter.rs:803-944`) — the
  visual selection cannot move (F14a).
- **F-C3 The green element is the typed outcome marker**: a bare
  `Rectangle` with `min-height: space-3`, no text ("R5: the tones,
  never message text"), no height cap, never reset to `none`
  (`settings-view.slint:323-332`, `src/adapter.rs:936-943`). With
  stretch-0 cells everywhere the runtime distributes leftover height
  to it (i-slint layout fallback-to-stretch-1), so it renders as a
  large empty green bar; after the first successful op of any kind it
  is permanently visible (F14b mechanism).
- **F-C4 Every field is the in-repo `TextField` primitive** wrapping a
  raw `TextInput` with **no width bindings and no horizontal-alignment
  setting** (`ui/widgets/text-field.slint:10-62`); the builtin
  `TextInput` reports a text-derived preferred width and native
  stretch 0 inside layouts, so the perceived leftward growth emerges
  from Slint layout defaults (F20 mechanism, INFERENCE from i-slint
  1.18.1 layout rules). The same widget serves task title editing
  (`ui/views/task-view.slint:86-93`), reminder custom offset
  (`ui/compositions/rule-list.slint:90-94`), create flows, and stack
  create — one fix fixes all (F31's hook point is here too).
- **F-C5 Commit is Return-only by design** (`accepted()`; D9 comment at
  `settings-view.slint:315-316`); title commits via the shared
  `commit_edit` (`src/view_state.rs:674-721`); the reminder offset has
  its own parse-and-route handler that silently absorbs parse misses
  (`src/adapter.rs:1789-1801`). A checkmark button (F31) can invoke
  the same seams.
- **F-C6 Scale commits on Return only**, live-applies
  `Theme.base-scale`, clamps 0.5–3.0, typed `Invalid` replies,
  reverts on error (`settings-view.slint:312-320`,
  `src/adapter.rs:1112-1137`,
  `crates/taskboard-app/src/ui_requests.rs:419-434`) — sound
  semantics; only the widget form (F16) and the scroll trap (F17) are
  wrong.
- **F-C7 Save can silently do nothing.** The Save button is disabled
  unless the draft passes the kebab/URL/password ladder
  (`src/adapter.rs:703-706`, `src/settings_form.rs:63-99`) and a
  disabled button absorbs the click with no callback
  (`settings-view.slint:77-83`); the seam's rejection path has no
  tracing call (`src/adapter.rs:992-1031`); the app-side success path
  logs nothing at info (`ui_requests.rs:291-354` — verify logs only on
  failure, `src/connections.rs:28-41`). The ceremony itself is real
  (verify → overlay upsert → store secret → detach/attach →
  `wait_for_boot_pull` → `Ok(rows)`) but its only feedback is the
  tone marker (F21 mechanism).
- **F-C8 The typed error path already exists end-to-end**:
  `SettingsError` (`src/requests.rs:67-95`) ← ceremony mappings
  (`ui_requests.rs:481-552`) ← `CommandError`/`DeckError`
  (`crates/taskboard-app/src/commands/mod.rs:115+`,
  `crates/taskboard-sync-nextcloud/src/error.rs:11-59`) — but the UI
  folds everything into five tones with no text (F21's "typed error
  text" needs a presentation mapping, not new plumbing).
- **F-C9 Secrets guarantees are in place** (redacting `AppPassword`,
  keyring/env/file `CredentialStore`, payload never on models, pinned
  by structural tests — `src/secrets.rs:111-138`,
  `src/requests.rs:25-32`, tests as listed in §1.7) — the rework must
  not disturb them.

### 1.4 Facts — connection/sync backend shape (post multi-board)

- **F-D1 Multiple named connectors exist in config**:
  `SyncSection.connectors: Option<Vec<ConnectorCfg>>` — an array
  (`[[sync.connectors]]`), first entry the default
  (`crates/taskboard-app/src/config.rs:205-211`,
  `src/connectors.rs:9-15`); `ConnectorCfg { name, backend,
  server_url, username }` with `SyncBackend { Standalone, Nextcloud }`
  and only Nextcloud implemented (`config.rs:183-190, 240-253`).
- **F-D2 Boards are a map and each board binds to a connector**:
  `AppState.boards: BTreeMap<BoardId, Board>`
  (`crates/taskboard-domain/src/state.rs:25-45`),
  `Board.remote: Option<RemoteBoardBinding>`,
  `RemoteBoardBinding { connector, board }`
  (`src/entities.rs:351-369`, `src/ids.rs:131-137`), created via
  `StateCommand::BindBoard` (`src/messages.rs:139-148`) — currently
  dispatched only from the REPL (`crates/taskboard-app/src/commands/boards.rs:707`).
  The settings view has **no board awareness at all**: one global
  draft keyed by Name (`src/settings_form.rs:42-46`), rows expose only
  Test/Remove (F18's "assumptions untrue" is exactly this gap).
- **F-D3 The runtime lifecycle capability exists**:
  `ConnectorControl { attach_connector, detach_connector_named,
  connector_commands }` (`src/bootstrap.rs:141-269+`) with typed
  `AttachError { ConnectorNameInUse, Engine, Draining, Credential,
  Client }`; sync actors derive their board set from bindings tagged
  with their connector id each cycle
  (`crates/taskboard-domain/src/messages.rs:361-376`); `SyncCommand`
  is `{ SyncNow, Detach }` only — there is **no test-connection actor
  message**; the settings Test runs `verify_credentials` directly
  (`ui_requests.rs:272-283`). Attach/detach/bind orchestration for the
  UI is new app-side handler code, not new domain surface.
- **F-D4 `UnbindBoard` exists** (`messages.rs:242-250`) — board↔
  connector detach has its domain command ready.

### 1.5 Facts — registry, keymap, focus model, task view, reminders

- **F-E1 Registry inventory (34 rows, pinned by
  `registry_covers_every_action_exactly_once`,
  `src/actions.rs:675-716`)**: app.exit/menu/undo/redo/settings/help,
  capture.quick, palette.search, nav.next-view/prev-view/focus-board/
  next-item/prev-item, board.next/previous/select, stack.wider/
  narrower, task.create/edit/complete/move/delete, reminder.snooze/
  mute/rule-add/rule-remove (unbound), menu.next/previous/invoke/
  dismiss, mode.insert/normal. **Absent** (all F24–F27, W3, W4):
  any move-cancel action, stack.next/stack.prev, board.create,
  board.delete, stack.delete, task.reopen, nav.first-item/nav.last-item.
  Also: `stack.create` is unbound in the emacs preset
  (`src/keymap/presets.rs:114-166`) — discovered during anchoring,
  folded into the registry chunk.
- **F-E2 Move-task cancel is structurally missing, not just unbound.**
  The flow `Flow::Move` cancels via: re-press of task.move (toggle),
  any leaving effect, reconcile-on-vanish
  (`src/adapter.rs:406-446`, `src/view_state.rs:736-795, 843`).
  Escape resolves to nothing during a move flow in **every** preset
  (Escape rows exist only for Menu/ViInsert/Settings contexts,
  `presets.rs:160-165, 207-214`); emacs C-g alone is a pending prefix
  of `app.redo` (`presets.rs:118`) and dispatches nothing; and the
  interpret table's discard arm covers only `Flow::Create |
  Flow::EditTitle` — `(Flow::Move, SetMode(Normal))` falls through to
  the default arm leaving the flow intact (`src/adapter.rs:444-446,
  503-508`). F24 needs both a bound cancel action **and** the
  interpret-table arm.
- **F-E3 Focus is view-state data, not Slint focus.**
  `step_focus` walks the reading order without wrap
  (`src/view_state.rs:434-505`); `select_stack` is the single
  stack-switch primitive (`src/view_state.rs:510-514`) — the natural
  hook for stack.next/prev; no first/last jump exists. Stack headers
  are plain Rectangles (not FocusScopes); cards are deliberately not
  tab stops (`ui/widgets/task-card.slint:5-7`).
- **F-E4 Hints are keymap projections everywhere** —
  `first_binding_display` feeds menu rows, tooltip `hint-keys`, and
  the "Back to board" button (`src/keymap/mod.rs:111-138`,
  `src/models.rs:177-183`); F28 is purely the tooltip's width math.
- **F-E5 Reminder presets row**: chips are borderless-when-idle
  Rectangles with `min-height: space-5`, **no min-width and no
  padding** — text touches the chip edges
  (`ui/compositions/rule-list.slint:69-88`); the custom offset field
  is a separate row below (90-94); presets are 5m/30m/1h/24h
  (`src/presentation.rs:264-288`). F29/F30 surface.
- **F-E6 Due date is display-only by recorded scope cut (R1-D13)**
  (`ui/views/task-view.slint:117-129`); the domain accepts
  `TaskChanges.duedate` today (`messages.rs:263-276`,
  `command.rs:358-360`); presentation helpers to reuse:
  `due_kind`, `due_phrase`, `DueSets`, `duration_bucket`
  (`src/presentation.rs:26-161`). W2 is UI-only.
- **F-E7 Complete is hard-coded `done: true`**
  (`src/router.rs:159-162`); no reopen id exists anywhere (W3).
- **F-E8 `KeyFeedback` is declared but never rendered**
  (`ui/app-window.slint:36-40, 71`; `src/adapter.rs:210-230`) — noted
  so nobody mistakes it for the green box; out of scope this round.

### 1.6 Facts — domain command coverage (scope-guard verification)

`StateCommand` variants present today
(`crates/taskboard-domain/src/messages.rs:22-251`): `CreateTask`,
`UpdateTask`, `SetTaskDone`, `MoveTask`, `DeleteTask`,
`CreateStack`, `RenameStack`, `DeleteStack`, `MoveStack`,
label verbs, `CreateBoard`, `BindBoard`, `RenameBoard`,
`SetBoardColor`, `SetBoardArchived`, `DeleteBoard`, `ResetBoard`,
reminder verbs, `UnbindBoard`, `ReconcileByTitle`. **Every action this
round dispatches already has its command** — the expected
`taskboard-domain` diff is empty.

### 1.7 Test surface inventory (what pins what today)

- `crates/taskboard-ui-slint/tests/journeys.rs` — context menu focus
  pin (1109), compact add-stack (1169), create-flow click≡key (1193),
  keyboard resize + persistence (1243), drag seam (1301), auto-sentinel
  pin (1357), settings journeys (1503-1931: open/close, save routing +
  redaction, row status, preset pick, rule list).
- `crates/taskboard-ui-slint/tests/usability_components.rs` — tooltip
  hover/unhover/delay + menu geometry under the mock clock (144-452).
- `crates/taskboard-ui-slint/tests/keymap_snapshots.rs` + snapshots —
  registry dump + per-preset resolved keymaps; coverage contracts in
  `src/keymap/mod.rs:426-605`.
- `tests/golden_visuals.rs` — board goldens at
  scale×width ∈ {0.5,3.0}×{600,1000,1500}; **no settings goldens**.
- `crates/taskboard-app/tests/ui_settings.rs` — save ceremony over
  wiremock (A6 114), failing verify persists nothing (197), dropped
  reply (241), legacy refusal (283), Test without secret (336),
  preset persist+reply (470), scale bounds (515).
- `crates/taskboard-app/tests/cli_connect.rs` — connect ceremony E2E
  (c13-c15, userinfo, cleartext receipt).
- Inline: `src/ui_requests.rs:564+` (mapping tables, prune, debounce),
  `src/connections.rs:326-508` (overlay writers),
  `taskboard-ui-slint/src/settings_form.rs:132-259`,
  `src/requests.rs:168-211` (redaction pins).

## 2. Design A — Stack width semantics & board surface behaviour (F1, F2, F3, F4, F5, F6, F12, F23)

- **D1 (widths become floors, never pins).** A user-set stack width
  keeps its meaning as "minimum width of this column" but stops
  pinning the column: every column keeps `horizontal-stretch: 1`, and
  spare row width distributes proportionally over the stretch-1
  columns (Slint distributes per stretch; equal stretch = equal
  shares of the surplus — stage-1 verifies the distribution rule on
  the pinned Slint and, if equal shares misbehave, falls back to
  proportional `horizontal-stretch: stored/total-stored` weights —
  recorded decision either way). A board that was fully "resized"
  therefore still fills the window (F1), and no refresh can collapse
  a column below its stored floor (F12's visible symptom).
- **D2 (floor raise + load-time re-clamp).** `STACK_WIDTH_MIN_PX`
  rises 96 → **160 logical px** (readable at scale 0.5: 80 rendered
  px); `widths.rs` gains a `reclamp_below_floor` pass applied where
  widths load into `ViewState` (mount/reconcile), so existing configs
  with 96-px entries heal on first launch; the app-side overlay writer
  keeps pruning only dead ids (unchanged). Keyboard steps and ceiling
  math stay as shipped.
- **D3 (header height is fixed).** The stack header gets an explicit
  `min-height` == `max-height` of one control row
  (`Theme.space-6 + 2·Theme.space-1`, the D20b board-bar constant)
  with `clip: true` already present; surplus column height flows to
  the task area (`vertical-stretch: 1`). Title rendering is identical
  for 0 and >0 tasks (F3).
- **D4 (selection is painted in regular mode).** The header background
  arm gains the selected state —
  `selected ? Theme.surface-raised + focus-ring border` (the compact
  switcher's selected language, `board-layout.slint:307-311`) —
  driven by the already-pushed `selected-stack`; move-target keeps its
  distinct arm and wins when both apply (F4, all stack sizes).
- **D5 (plus glyph is square).** The add-stack `icon-box` gets the
  IconButton treatment — `min-width`/`max-width`/`min-height`/
  `max-height` locked to `Theme.space-6`, bars stay
  `focus-ring-width` — the affordance remains a full-height hit
  target but draws a square centred plus (F2).
- **D6 (add-stack gets the generalized tooltip).** Its TouchArea
  adopts the standard trigger: label "New stack", a11y description +
  tooltip describing the action, `hint-keys` from
  `first_binding_display(stack.create)` (empty renders as today —
  emacs gets its binding in D15) (F5, completing R1-D21b).
- **D7 (compact mode hides the regular affordance).** The regular
  full-height add-column mounts only when the columns row does
  **and** the layout is not compact; the compact switcher's trailing
  `IconButton` stays the single compact entry point, sized inline
  with the switcher segments (F23: no own row at the smallest
  viewport; the stack-detail header placement from R1-D22 is not
  needed — compact already has the IconButton).
- **D8 (per-column Add Task affordance).** Each column's task list
  gains a trailing ghost row (the add-column's visual language: hover
  surface, plus glyph, radius-small) labelled for the a11y tree
  "Add task"; clicking it selects that stack (`select_stack`) and
  opens the existing create flow anchored to it — the same
  `task.create` action the keyboard uses (click ≡ action equivalence,
  R1 §7.1). Empty stacks thus show the affordance instead of blank
  space (F6); the create-flow click≡key journey pattern
  (`tests/journeys.rs:1193`) is the template for its test.

## 3. Design B — Interaction repairs (F7, F8, F11, F13, F22, F28)

- **D9 (model identity becomes stable — implement #390's fix
  direction).** `models::push` stops replacing every `ModelRc` with a
  fresh `VecModel` on each refresh: the board/stack/task models
  become persistent `VecModel`s updated by **row-level pushes**
  (insert/set/remove row) computed in the
  `ArcSwap` load → `view_state.reconcile` → `models::push` pass —
  the exact incremental-push direction issue #390 records (with its
  bench evidence: ~97 % dirty region per refresh today, partial lane
  costing more than full-frame). This is the root-cause fix shared by
  F8 and F11 *and* the first leg of #390's render-budget work.
  Stage-1 reads #390 (and its cited reports) and decides the diff
  mechanism (row-diff on push vs caller-side dirty flags), recording
  it. Two constraints are normative: (a) the design must **not
  preclude #373** (ListView virtualisation) — the `for`-repeater
  structure stays, per-column models stay addressable by stable
  indices, and a comment at the push seam points at #373 as the next
  lever on the same budget; (b) #390's *true-partial bench verdict*
  is explicitly out of scope here — chunk 2 annotates #390, the issue
  stays open for the re-measurement. Non-negotiable outcomes: (a) a
  Slint-side `double-clicked` survives an intervening refresh; (b) a
  drag delivers every `moved` tick across refreshes; (c) existing
  model pins (`never_resized_board_pushes_auto_widths…`) still pass.
- **D10 (double-click keeps its seam-level pin).** The Slint
  `double-clicked => activated()` wiring stays; because headless
  drivers cannot synthesize double clicks (F-B2), the chunk adds the
  seam-level pin it can (focus-half already pinned) **plus** a
  Tier-4 manual checklist entry ("double-click a card in the running
  app opens Edit Task") recorded in the chunk plan and handed to the
  user at PR time (same compromise the round-1 chunk recorded in
  `c6414ea`, now with the root cause actually fixed).
- **D11 (drag survives refresh; live update kept).** With D9 the
  per-tick `set_stack_width` + `refresh` flow stays as shipped; the
  repair adds a regression test that interleaves a refresh between
  press and move at the seam level, and a Tier-4 manual checklist
  entry ("drag a header edge: width tracks the pointer live through
  the whole gesture") (F11).
- **D12 (menu pointer path syncs).** `on_menu_entry_invoked` /
  `on_menu_submenu_entry_invoked` / `on_menu_level_back` route
  through the same post-mutation `self.sync(window)` the keyboard
  path gets (factor the trailing sync out of `route` into the shared
  helpers; `apply_close_menu`'s explicit sync folds into it). The
  compact index-space mismatch is fixed by resolving rows against the
  **page actually displayed** (root vs submenu page per
  `active-group` + compact flag), and ActionMenu emits
  `invoke-submenu-entry` for parent rows so the seam names stop
  lying (F7, both divergences of F-B3).
- **D13 (tooltip lifecycle is centralized).** `TooltipState` gains a
  Rust-side `dismiss_all` invoked by the adapter at: every menu
  open/close transition, every view switch (`NextView`/`PrevView`/
  `FocusBoard`/`OpenSettings`/… effect application), and every
  `menu-entry-invoked`; ActionMenu row clicks additionally call
  `unhover()` Slint-side (the icon-button pattern). Rule: a tooltip
  never outlives the element that spawned it, verified by a
  usability-components test "tooltip does not survive a view switch"
  (F13).
- **D14 (tooltip width includes the hint row).** Frame width becomes
  `max(caption-natural, hint-row-natural) + 2·space-2`, capped as
  today (`window·0.6`, `420·scale`); the caption-wrap probe comment
  stays authoritative for the caption term; the hint row measures via
  its own preferred width (F28; usability-components gains a
  wide-hint geometry pin under the mock clock).
- **D15 (window-stability probe completed, fallback applied where
  proven).** Finish R1-D15's stage 2: with `SLINT_SCALE_FACTOR` and
  in-app scale at the matrix {0.5, 1.0, 3.0} × min-floor window,
  instrument which mounted view's content min-constraints exceed the
  window (expected offenders per F-B6: settings, task view). Fix by
  capping offender min-widths (elide/wrap at the floors) — never by
  binding Window width/height. The settings scroll container (D30)
  is the settings-side fix; this chunk's probe covers board/stack/
  task views and records the matrix results in the chunk plan. Tier-4
  checklist: "switch views at scale 3.0 in a minimum window — the
  window never resizes" (F22).

## 4. Design C — Registry round-up & object context menus (F9, F10, F24, F25, F26, F27, W3, W4)

- **D16 (`task.move.cancel`).** New registry row (label "Cancel move",
  Task-group so it appears in the task context menu during a flow)
  routing `UiEffect` `CancelMove`; bindings: **Escape** in Global
  context for all three presets (no conflict — menu.dismiss's Escape
  is Menu-context; settings-close's is Settings-context) and **C-g**
  for emacs (Global; the `app.redo` prefix conflict resolves by
  ordering — `move.cancel` is registered before `app.redo`'s sequence
  so a lone C-g during a move flow cancels, completing the sequence
  elsewhere still redoes; stage-1 verifies the conflict contract
  tests still hold and records the resolution). The interpret table
  gains `(Flow::Move, CancelMove | SetMode(Normal)) → cancel_move`
  (F24; closes F-E2's structural half).
- **D17 (`stack.next` / `stack.prev`).** Registry rows routing
  `select_stack(selected ± 1)` with wrap (the compact walk already
  wraps; board-level wrap decision: wrap, matching `board.next`
  semantics); bindings: emacs **C-f / C-b** (user's proposal; both
  unbound today), standard **Ctrl+Alt+Right / Ctrl+Alt+Left**
  (families with the stack.wider pair without colliding), vi
  **g l / g h** (gt/gT are views). Stage-1 may re-pick standard/vi
  keys if the conflict contract disagrees; the recorded constraint is
  "every preset binds both directions, discoverable via header
  tooltips" (F25).
- **D18 (`board.create` / `board.delete` / `stack.delete`).** Registry
  rows routing the existing `StateCommand::CreateBoard` /
  `DeleteBoard` / `DeleteStack` (§1.6). Bindings: board.create —
  standard **Ctrl+Shift+N**, emacs **Ctrl+c Ctrl+b**, vi **g n**;
  board.delete — standard **Ctrl+Shift+Delete**, emacs
  **Ctrl+c Ctrl+d**, vi **g d** (board chain); stack.delete — joins
  the stack context menu and: standard **Ctrl+Delete**, emacs
  **Ctrl+c Delete**, vi **g D** (stack chain). Destructive deletes
  get a typed refusal when the target is the last board / a bound
  board mid-sync per engine rules — the UI renders the existing
  tone/flow paths, no new confirmation modal this round (F26, F27).
- **D19 (`task.reopen` — closes #229).** Registry row routing
  `SetTaskDone { done: false }` (the `SetTaskDone` command already
  carries the flag); bindings: standard **Ctrl+Space**, emacs
  **Ctrl+c Ctrl+Space**, vi **U**; enabled only on a completed
  focused task (same enablement machinery as task.complete). The
  single-toggle folding from #229's "consider" stays deferred until
  completed-task checkboxes exist (W3).
- **D20 (`nav.first-item` / `nav.last-item` — closes #232).**
  `view_state` gains `jump_focus(Edge)` beside advance/retreat;
  registry rows + bindings: emacs **Alt+< / Alt+>** (as the issue
  proposes), standard **Ctrl+Home / Ctrl+End**, vi **g g / G**;
  `every_preset_covers_the_core_verbs` extended (W4).
- **D21 (object context menus generalize; closes F9's fix too).**
  `menu::context_entries` parameterizes the chain per object: task →
  `[Board, Global]` (fixing F-B7 so existing hints actually render —
  F9), stack → `[StackDetail, Global]` gaining Move stack (existing
  `MoveStack`), Rename stack, Delete stack (D18), Wider/Narrower,
  New task; board → `[Global]`-appropriate chain gaining New task,
  New stack, New board, Rename board, Delete board (D18). The
  stack header and board-bar segment TouchAreas grow the same
  right-click `pointer-event` pattern task-card uses
  (`task-card.slint:85-91`) with anchors at pointer position. Menu
  groups: `MenuGroup::Stack` and `MenuGroup::Board` join `Task` in
  the registry's group metadata (F10).

## 5. Design D — Task-view & text-field ergonomics (F20, F29, F30, F31, W2)

- **D22 (`TextField` becomes a well-behaved layout citizen).** The
  primitive's inner `TextInput` stops reporting text-derived
  preferred width into ancestor layouts (wrap the input in a fixed
  geometry container / set explicit `width: 100%` on the input row
  and left alignment); the outer Rectangle fills its layout cell.
  All fifteen-odd call sites inherit the fix; the settings fields'
  leftward growth (F20) and any task-view variants die with it.
  Stage-1 probes the pinned Slint for the exact mechanism that
  neutralizes the TextInput preferred-width propagation and records
  it with a geometry pin in usability-components.
- **D23 (checkmark commit affordance).** `TextField` gains an
  optional trailing commit `IconButton` (check icon, same
  space-6 square language): visible while the field holds focus and
  a non-empty buffer, clicking it fires the same `accepted(text)`
  seam as Return. Enabled for: task title, reminder custom offset,
  create task/stack flows, settings fields (F31). No text assertions
  — tests pin visibility/enablement structure and that the click
  path equals the Return path.
- **D24 (reminder presets row absorbs the custom field).** Preset
  chips get `padding: Theme.space-1 Theme.space-2` + `min-width:
  Theme.space-6` (F29); the custom-offset `TextField` moves into the
  presets `HorizontalLayout` after the chips with
  `horizontal-stretch: 1` and a `min-width` floor, label collapsing
  to placeholder ("Custom offset, e.g. 45m") so the row reads as one
  controls row (F30). The parse-miss silent absorb stays (D12 of the
  reminders round) — but a parse miss now also clears the buffer so
  the field doesn't look stuck.
- **D25 (due-date editor — closes #227; amended by user 2026-10-23:
  calendar picker required).** The task-view metadata row's Due date
  cell becomes a proper date editor with **both interaction paths**:
  - **Typed entry**: a text field (grammar stage-1 settles — at least
    ISO `YYYY-MM-DD`, chrono-parsed) with live `due_phrase` echo;
    empty input clears (`duedate: Some(None)`), invalid input
    refuses typed and keeps the buffer.
  - **Calendar picker**: a popover month grid anchored to the field
    (prev/next month buttons, month/year caption, day cells with the
    today marker and the selected date highlighted; the
    presentation `DueKind` colours reused for overdue/due-soon
    tinting). Mouse path: click a day to commit. Keyboard path: the
    field's picker toggle opens the grid, arrows move the day
    cursor, PageUp/PageDown or dedicated keys switch months, Enter
    commits the cursor day, Escape closes without committing — the
    grid is fully operable without the mouse.
  - Both paths funnel through **one commit seam** (the same
    `UpdateTask { changes: TaskChanges { duedate: … } }` dispatch),
    so click ≡ key holds at the picker too (R1 §7.1 equivalence).
  A richer grammar (natural language like "next friday") stays
  deferred; the W1 today/organiser views reuse the widget later. The
  R1-D13 scope-cut comment is replaced with a pointer to this
  decision (W2).

## 6. Design E — Settings form, feedback, help (F14, F15, F16, F17, F19, F21-feedback)

- **D26 (segmented control widget).** New L1 primitive
  `SegmentedControl` (generic over N options): one grouped container
  (radius-small border, surface background), equal segments, exactly
  one selected at a time — selected renders `surface-raised` +
  `text-primary` + focus ring when focused, unselected
  `text-secondary`; keyboard accessible (a11y radio-group role).
  Consumed by the keymap chooser (F15). The inverted-`enabled`
  picker (F-C2) dies.
- **D27 (keymap selection echoes).** `apply_keymap_update` also
  writes `SettingsModel.preset-index` from the reply's keymap — the
  selection always reflects the **persisted** preset, whether the
  change came from this UI, the REPL, or a config reload (F14a).
- **D28 (status line replaces the tone bar — reverses R1-R5).** The
  bare outcome `Rectangle` becomes a one-row status line: tone dot +
  human text derived from the typed outcome via a presentation-layer
  mapping (`SettingsOutcome`/`SettingsError` → short message; every
  variant must map — exhaustive-match enforced in code, and the
  *mapping's existence/coverage* is what tests assert, never the
  strings). Fixed `min-height`, `max-height` one row, `vertical-stretch: 0`
  so it can never absorb layout height (the F-C3 mechanism), and an
  auto-clear timer (mock-clock, no real sleeps in tests) returning to
  hidden after N seconds of no new outcome. R1-R5's "tones, never
  message text" is recorded as reversed by user direction (F14b, F21).
- **D29 (scale = slider + field).** The scale row becomes an
  L1 `Slider` (0.5–3.0, step 0.1, a11y slider role) plus the existing
  `TextField` (exact entry, Return-commit semantics unchanged, D9 of
  round-1 preserved). Slider drags preview live via the existing
  `set_base_scale` path but commit on release (one `SetScale`
  request per gesture); the field stays authoritative (F16).
- **D30 (settings scrolls).** The settings content wraps in a
  `Flickable` + scrollbar (the action-menu's Flickable is the
  in-repo pattern) with the header row pinned outside the scroll;
  at any scale the scrollable extent reaches the status line and
  scale controls — acceptance: at scale 3.0 in a min-floor window,
  the scale field is reachable and the window does not resize
  (F17; also D15's settings-side obligation).
- **D31 (info affordance).** New tiny `InfoTip` widget (circle-"i",
  `space-3` square, hover = the generalized tooltip trigger) placed
  beside: Name, Server, Username, App password, Scale, Keymap, and
  each connection row. Help texts live as presentation constants
  (Name: free-form local kebab label, your choice; Server: the
  Nextcloud instance base URL; App password: what it is + created in
  Nextcloud → Personal settings → Security → Devices & sessions;
  per-row: test/remove semantics). Tests assert presence/structure
  and that each field's tip content is non-empty — never the text
  (F19).

## 7. Design F — Connection management rework (F18, F21)

- **D32 (board-scoped connection surface).** The Connections section
  becomes: a **board selector** (the `BoardBar` segment language or
  a compact list — stage-1 picks; every local board from
  `AppState.boards`, marking each as *bound to connector X* /
  *unbound*) above a **connector list** (name, backend badge,
  server, username, attachment/binding state, per-row Test / Remove /
  Bind-to-this-board), above the **add-connection form** (unchanged
  fields + a backend picker where Nextcloud is the only enabled
  option — `SyncBackend` rendered as a disabled "coming soon" row
  for future kinds). The `UiSettings` mount payload grows
  `boards: Vec<{ id, name, bound_connector }>`,
  `connectors: Vec<{ name, backend, server_url, username, attached }>`;
  every settings reply returns the refreshed pair (the
  `current_connectors` helper already exists,
  `ui_requests.rs:444-452`) so rows never go stale (F18).
- **D33 (attach/detach/bind requests).** New `UiRequest`s with
  oneshot replies (the established lane):
  `AttachConnector { board, connector, reply }` — app-side:
  verify (if no stored secret) → `ConnectorControl::attach_connector`
  → `StateCommand::BindBoard` through the engine command path (the
  same path the REPL's bind uses, `commands/boards.rs:707`) →
  settled via the existing `wait_for_boot_pull`;
  `DetachConnector { board, connector, reply }` — `detach_connector_named`
  + `StateCommand::UnbindBoard`. Typed refusals map through the
  existing `map_attach_error`/ceremony tables
  (`ui_requests.rs:481-552`). Remove stays config-level (overlay
  remove + detach), unchanged semantics. No new domain surface
  (§1.6) and no secret-path changes (F-C9 holds).
- **D34 (the save ceremony becomes visible).** Save/Test/Attach
  operations drive per-operation **busy states**: the form/row shows
  a spinner (indeterminate, mock-clock-friendly state machine — no
  wall-clock in tests); app-side gains `tracing::info!` at save start
  and success (name, server host only — never credentials; the
  redaction rules of §1.7 apply to logs too); the reply's typed
  outcome lands in the D28 status line with the mapped message
  (invalid URL format / unreachable / unauthorized / not a Deck
  instance / stored / attached). The silent-absorb paths (disabled
  Save button, invalid draft) become visible: a disabled Save shows
  *why* via its row's info tip / status line on click-through —
  stage-1 picks the mechanism; the recorded constraint is "every
  click on Save produces a visible reaction" (F21).

## 8. Design G — Visual identity: stacks as cards, boards as tabs (F32)

- **D35 (stack card chrome).** Each column wraps in a card
  `Rectangle`: `Theme.surface-raised` background,
  `radius-medium`, the existing `space-3` gutters become the visible
  board-background strip around/between stacks (gutter token bumps
  to `space-4` if the probe shows cramped); header, task cards and
  the D8 add-task ghost row sit inside the card and adjust surface
  steps so cards read as containers (task cards on `surface-raised`
  switch to a raised-again token — stage-1 defines the exact token
  ladder in `theme.slint` and records it) (F32 part 1).
- **D36 (board tabs).** The board bar becomes a tab strip: segments
  lose per-segment boxes; the **active** tab renders merged with the
  content below — background = the content surface, rounded top
  left/right corners, no bottom border (flows into the board
  background); **inactive** tabs are bordered tab markers
  (distinct border, `text-secondary`), hover lifts per the existing
  hover token; the strip gains a baseline rule the active tab breaks.
  Fixed height and elide behaviour from D20b/R1 are preserved; the
  #411 dropdown/many-board scalability redesign stays deferred (F32
  part 2).
- **D37 (goldens re-baselined deliberately).** The board goldens are
  regenerated once, in the same commit as the token changes, with the
  golden-matrix unchanged; the chunk plan lists the visual diffs as
  reviewed artefacts (they are the acceptance evidence for D35/D36).

## 9. Design H — Pointer drag-to-move (W1, #226)

- **D38 (DIY drag on the card TouchArea).** Press-and-hold-then-move
  on a task card lifts a ghost (the card's snapshot rectangle at
  reduced opacity following the pointer, mounted in the shell's
  overlay layer); column hit-testing uses the live column geometry
  the board layout already exposes to Rust (stage-1 records how
  geometry reaches the adapter — the `window-w` out-property pattern
  extended per column, or a Slint-side drop-target highlight driven
  by pointer position). Drop dispatches the **same `MoveTask`
  command** the move flow confirms (`click/gesture ≡ action`),
  after which the ghost unmounts; Escape (D16's cancel binding)
  and pointer release outside any column cancel. Re-check Slint's
  native DnD surface at the pinned version first; hand-roll only if
  absent (the issue's recorded approach). Depends on D9 (stable
  models — the ghost must survive refreshes) and D35 (column
  geometry) (W1).

## 10. Testing strategy (normative; AGENTS §5 / round-1 §10 still applies)

- All round-1 rules carry over: no `tokio::time::sleep` (mock clock /
  `pause`/`advance` — the tooltip delay and D28's auto-clear and
  D34's spinners are all mock-clock state machines); no text
  assertions (D28's message mapping and D31's help texts are pinned
  for **coverage/structure**, never content); typed outcomes over
  strings; one behaviour one test lowest layer; snapshot the engine
  only if a domain/state diff somehow appears (expected: none).
- **Regression-first for the repairs**: every F7/F8/F11/F13/F22/F28
  fix lands with the test that failed before it (seam-level where
  headless permits); what headless cannot reach (double-click feel,
  drag feel, spinner feel, window-resize feel) lands as an explicit
  **Tier-4 manual checklist** in the chunk plan and the PR body —
  the user is the Tier-4 runner, as round-1 established.
- **New journeys**: add-task ghost ≡ task.create; stack context menu
  routes delete/wider/narrower; board context menu routes
  create/delete; move.cancel via Escape (all presets) and C-g
  (emacs); stack.next/prev wrap; reopen only on done tasks;
  first/last jumps; due-date commit + clear + invalid-refused (typed
  path) and calendar-picker journeys (open, arrow navigation, month
  switch, Enter commits, Escape closes clean) — seam-level;
  segmented control echo after `KeymapUpdate`; attach/detach/bind
  choreography over wiremock (extend `ui_settings.rs` patterns);
  save-visible reactions (busy → typed outcome).
- **Property tests**: reclamp-below-floor monotonicity (extends
  `widths.rs` proptests); status-mapping totality (every
  `SettingsError` variant maps, proptest over variant generator).
- **Goldens**: regenerated only in D37's dedicated commit.

## 11. Code inventory (files this round touches)

- `crates/taskboard-ui-slint/ui/`: `theme.slint` (tokens: width
  floor constant, card/tab surfaces, gutter), `board-layout.slint`
  (widths arm, header height, selection paint, plus glyph, add-task
  ghost, card wrap), `board-bar.slint` (tabs), `tooltip.slint`
  (width), `text-field.slint` (layout fix, checkmark),
  `rule-list.slint` (reminder row), `task-view.slint` (due editor),
  `settings-view.slint` (scroll, segmented, slider, status line,
  info tips, connection rework), `action-menu.slint` (submenu
  emission, unhover), `app-window.slint` (menu seams, tooltip
  dismissal, overlay layer for ghost), new: `segmented-control.slint`,
  `slider.slint`, `info-tip.slint`.
- `crates/taskboard-ui-slint/src/`: `models.rs` (stable model
  identity, boards/connectors payload), `adapter.rs` (sync-on-menu,
  tooltip dismissal, attach/detach seams, settings echo),
  `view_state.rs` (jump_focus, cancel arm, stack next/prev),
  `actions.rs` + `keymap/presets.rs` + snapshots (new rows),
  `menu.rs` (object context entries, chains), `requests.rs`
  (new UiRequests), `widths.rs` (floor + reclamp), `presentation.rs`
  (status/help text mappings, due parse), `tooltip.rs` (unchanged
  math), new: `settings_status.rs` (mapping), `due_edit.rs` (parse).
- `crates/taskboard-app/src/`: `ui_requests.rs` (attach/detach/bind
  handlers, info logs, enriched replies), `connections.rs` (overlay
  writers as needed), `ui.rs` (UiSettings payload), `config.rs`
  (no shape change expected).
- `crates/taskboard-domain/`: **expected empty diff** (§1.6);
  `taskboard-state`, `taskboard-storage-sqlite`,
  `taskboard-sync-nextcloud`: empty diff, asserted by reviewers.
- Tests: the §1.7 inventory + new journeys/usability/geometry pins
  listed in §10.

## 12. Implementation slicing (8 chunks; dispatch plan is normative for order)

| Chunk | Design | Branch | Theme |
|---|---|---|---|
| 1 | A (D1–D8) | `feat/ui-r2-board-surface` | width semantics, header/selection/add affordances, per-column Add Task |
| 2 | B (D9–D15) | `fix/ui-r2-interaction-repairs` | incremental model push (#390's fix direction) → double-click + drag; menu sync; tooltip lifecycle/sizing; window-stability probe |
| 3 | C (D16–D21) | `feat/ui-r2-actions-context-menus` | registry round-up, move cancel, stack nav, board/stack lifecycle, reopen, first/last, object context menus |
| 4 | D (D22–D25) | `feat/ui-r2-taskview-textfields` | TextField layout fix + checkmark, reminder row, due-date editor (calendar picker + typed entry) |
| 5 | E (D26–D31) | `feat/ui-r2-settings-form` | segmented control + echo, slider, scroll, status line, info tips |
| 6 | F (D32–D34) | `feat/ui-r2-connection-management` | board-scoped connections, attach/detach/bind lane, visible ceremony |
| 7 | G (D35–D37) | `feat/ui-r2-visual-identity` | stack cards, board tabs, golden re-baseline |
| 8 | H (D38) | `feat/ui-r2-drag-to-move` | pointer drag-to-move |

Serial default 1→8 (shared seams: `adapter.rs`, `models.rs`,
`theme.slint`, `board-layout.slint`, `settings-view.slint`).
Permitted parallel lanes if the user accepts rebase cost (AGENTS §8
rebase dance applies): lane α = 1→2→7→8, lane β = 3 (after 2),
lane γ = 4→5→6 (after 2). Chunk 3 is gated on 2 (menu sync rewrite),
4 on 2 (TextField stability), 5 on 4 (widget reuse), 6 on 5 (status
line), 7 on 1+4, 8 on 7. Round-1's "minimize PR count" instruction
was for that round's scope; this round defaults to the 8-slice
reviewability, and the user may direct collapsing (e.g. 4+5) at
dispatch time.

## 13. Backlog interactions

- **Closes on merge** (one ✅ DONE comment per issue, from the chunk
  that ships the behaviour): #226 (chunk 8), #227 (chunk 4), #229
  (chunk 3), #232 (chunk 3).
- **Stays open, annotated**: #390 (chunk 2 lands its fix direction —
  the incremental row-level push — but the true-partial bench
  re-measurement that gives budget row 3 its verdict stays open with
  the issue), #351 (settings v1…v2 still narrower than free-form toml
  editing), #347 (quick capture — D8 is per-stack
  Add Task, not the inbox/quick-capture surface), #356/#353
  (undo/redo), #373 (horizontal scrolling/virtualisation — D9 is
  explicitly designed not to preclude it), #411 (board-selector
  *scalability* dropdown — D36 is the visual tab restyle only),
  #412 (window-geometry persistence across restarts — D15 completes
  R1-D15's *within-session* stability only).
- **New issues to file after this round's design merges**: none —
  every review finding above has a design home; anything discovered
  out-of-scope during stages goes to `backlog:` per AGENTS §7.
- **What opens next**: the user's next GUI review session (round 3)
  reviews this round's merges as its baseline.

## 14. Risks

- **R1 Stable model identity (D9) is the round's highest-risk
  change** — it underpins two user-visible fixes and touches every
  refresh path. Mitigation: the full existing journey suite must pass
  unchanged; new interleaving tests (refresh-between-press-and-move);
  any Slint-version-specific model-reuse behaviour gets probed in
  stage-1 before the approach is recorded.
- **R2 Slint layout defaults fight back** (D1 stretch distribution,
  D22 preferred-width neutralization, D30 Flickable, D35 card
  wrap): every one carries a recorded stage-1 probe on the pinned
  Slint 1.18.x with a fallback decision, mirroring round-1's probe
  discipline.
- **R3 Keymap collisions** (D16–D20 add ~12 bindings across three
  presets): the conflict/shadow contract tests
  (`keymap/mod.rs:538-605`) are the gate; any proposed binding that
  trips them is re-picked in stage-1 with the constraint recorded,
  never by weakening the contract.
- **R4 The connection rework (D32/D33) widens the app-side surface**
  (engine command plumbing from the settings lane for the first
  time): the REPL E2E suites are the unchanged-behaviour gate; the
  new choreographies extend `ui_settings.rs`'s wiremock patterns;
  secret/redaction pins (§1.7) must pass untouched.
- **R5 Text-bearing additions (status line, help texts, a11y labels)
  invite text assertions**: reviewers reject any assertion on the
  strings; coverage/structure pins only (round-1 R5's test rule
  survives even as its UI-tones-only rule is reversed).
- **R6 The window-stability probe (D15) may implicate Slint/winit
  version behaviour** beyond content-min constraints: if the probe
  exonerates the hypothesis, the fallback search order is (a) per-view
  preferred-size capping as R1-D15 recorded, (b) escalate to the user
  with the probe matrix — never a Window width/height binding.
- **R7 Eight serial chunks are merge-order sensitive**; the dispatch
  plan's precondition gates (previous chunk merged) are mandatory,
  and any parallelization the user directs inherits the AGENTS §8
  rebase-dance obligations.
