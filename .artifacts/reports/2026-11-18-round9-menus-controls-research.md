# Round 9 GUI Review Research — Menus & Task-Card Controls

- **Date**: 2026-11-18
- **Scope**: the operator's four findings on the task context menu's
  quick-actions hot-bar, sub-menu indication arrows, dropdown reuse in
  task details, and the related open issues (#927, #938).
- **Method**: artefact archaeology (`.artifacts/plans/`, census),
  `gh pr view 933/940`, `gh issue view 927/938`, and a source read of
  `crates/taskboard-ui-slint` (`src/menu.rs`, `src/adapter.rs`,
  `ui/compositions/action-menu.slint`, `ui/widgets/dropdown.slint`,
  `ui/widgets/icon.slint`, `ui/widgets/icon-button.slint`,
  `ui/views/task-view.slint`, `ui/compositions/recur-editor.slint`,
  `ui/app-window.slint`). No production code written.

## 1. Hot-bar archaeology — what was specified vs delivered

### 1.1 What was specified

- **Origin plan** `.artifacts/plans/2026-11-16-task-card-controls-ux-plan.md`
  §3 **US-1** ("The quick actions bar", P1): icon-buttons at the top of
  the task context menu — schedule today / schedule tomorrow /
  unschedule / tracking start-stop twin — each with a keyboard shortcut
  routing the same `ActionId` (click ≡ key). The design reference is
  named explicitly at the plan header: "the operator's SuperProductivity
  screenshot … an icon quick-actions row atop the menu, ▸ submenus,
  per-row key badges." Wiring row **W-1** names
  `src/menu.rs` + `ui/compositions/action-menu.slint` + `src/adapter.rs`.
- **Chunk plan** `.artifacts/plans/2026-11-17-ui-r8-task-card-controls-plan.md`
  **D-2** ("The quick bar's menu-model section and popup rendering"):
  `QuickBarEntryModel` on the slint side, `quick_bar_entries: Vec<QuickBarEntry>`
  + the `on_quick_bar_invoked(int)` seam on the Rust side, walkthrough-graded.
- The round-8 dispatch plan chunk 5 and the round plan
  (`2026-11-16-gui-review-round8-plan.md` §41/§77, citing the
  SuperProductivity screenshot) both carry the same shape.

### 1.2 What was delivered — it IS implemented

PR **#933** ("feat(ui): the task quick-actions bar and menu grammar —
N-7/N-8/N-9/N-11/N-12/N-13, #811"), **merged 2026-11-18T01:02:29Z**,
delivered the bar end-to-end:

- `crates/taskboard-ui-slint/src/menu.rs:520–603`: `QuickBarIcon`
  (Calendar / CalendarPlus / CalendarMinus / Play / Stop),
  `QuickBarEntry { action, icon, enabled, visible }`, and
  `task_quick_bar(scheduled, tracked_here, tracking_elsewhere)` —
  today/tomorrow always shown, unschedule withheld on unscheduled
  subjects (C-4), the tracking twin's shown half flips with state,
  Start greyed while another task tracks.
- `ui/compositions/action-menu.slint:135–153`: a leading
  `HorizontalLayout` of `IconButton`s above the Flickable, mounted only
  while `quick-bar.length > 0` (every other menu consumer passes `[]`
  and renders bit-identical); routing via the
  `menu-quick-bar-invoked` seam resolved against the Rust vec.
- `src/adapter.rs` (`quick_bar_model` ~:866, `open_context_menu_with`
  ~:10194, the invocation seam ~:9075): the model is built once at open
  (the D-1/MenuKind pin covers it); two new icon assets
  (`calendar-plus.svg`, `calendar-minus.svg`) were vendored.
- The census (`docs/ux-surface-census.org` app.menu and
  task.scheduled_edit rows) already records the bar as evidence
  (UX-R8 chunk-5 US-1, journey `us1_s1_schedule_today_commits_the_exact_mask`).

**Verdict: the hot-bar was not silently ignored.** It was specified,
chunked, implemented, journey-tested, and merged — as the *last content
chunk but one* of round 8, at 01:02 UTC on 2026-11-18.

### 1.3 Why the operator saw nothing (most likely)

The operator's GUI pass almost certainly ran against a **pre-#933
build of main** (their daily driver): every earlier round-8 merge
(#862…#925) predates the bar; #933 landed hours before this research
ran. **First action for round 9: re-verify on a post-#933 build before
any rework is dispatched.** If the bar still does not show on a fresh
build, the code paths to re-audit are `open_task_context_menu` →
`task_context_page` → `quick_bar_model` → the
`KeyboardState.menu-quick-bar` global → the `if (root.quick-bar.length
> 0)` mount condition in `action-menu.slint`.

### 1.4 Genuine residual gaps (real, but narrow)

1. **The multi-selection branch has no bar.** In
   `open_task_context_menu` (`src/adapter.rs` ~:10159), when the
   right-clicked card is covered by the selection, the menu opens via
   `open_context_menu(window, x, y, &ids, Vec::new())` — an **empty
   bar**. The chunk plan's D-4 note records this as deliberate ("the
   multi grammar owns itself"), citing the origin §1 non-goal — but the
   origin §1 also says "the quick bar … composes with it — the bar's
   rows are multi-sensible and ride that menu (board-interaction W-5)".
   The two statements conflict; the implementation took the narrower
   reading. A multi-sensible bar (Complete / Delete / Schedule-today
   over the selection) is the one piece of the specified hot-bar that
   genuinely remains unbuilt.
2. **#938** — no `LabelManage` row on the card context menu (scoped to
   BOARD_CONTEXT by the tag-color plan's W-2; see §5.2 below).
3. **#927** — stale pinned rows when the subject vanishes mid-open (the
   MenuKind pin makes staleness persist by design; see §5.1).

## 2. Hot-bar specification (what remains to build)

Assuming re-verification confirms the single-card bar renders, the
remaining specified-but-unbuilt surface is:

- **Multi-selection quick bar**: when `selection_covers`, compose a bar
  over the selection's aggregate facts (tracking twin shown only when
  the selection is single-subject or uniformly tracked; schedule
  today/tomorrow/unschedule apply to every member — they already
  dispatch sparse `UpdateTask` masks that are selection-addressable via
  the existing multi-row routes). Bar buttons address the selection,
  not the focused card. Icon vocabulary: reuse the five existing
  `QuickBarIcon` glyphs — no new assets.
- **Icon sourcing**: unchanged — vendored Lucide monochrome SVGs,
  `IconKind` closed vocabulary, `Image.colorize` tint (r4 D10/D19
  rules). `chevron-right` etc. already exist for §3's arrow work.
- **Tooltip / shortcut display**: `IconButton` already carries
  `label`/`description`/`hint-keys` into `TooltipState` and the a11y
  tree; the bar populates `label` from the registry. Gap: the bar
  buttons currently pass no `hint-keys` — the preset stroke for each
  action should ride the tooltip (registry-generated hints, the
  keymap-parity chunk's machinery). Small, widget-feed-only change.
- **Menu-grammar interaction**: the bar coexists with the MenuKind::Object
  pin (built once at open, D-1) — a multi bar must be composed at open
  time from the same frozen facts, so it inherits the pin and the #927
  disarm question unchanged. The C-4 withheld rule applies per bar row
  exactly as the single-card bar implements it.

## 3. Sub-menu indication audit + general fix

**Audit (confirmed at the shared component level).**
`MenuEntryModel` carries `submenu: bool` ("True for a submenu-parent
row (round-1 D6): activating it opens the group's page") — but the row
renderer in `ui/compositions/action-menu.slint` (lines 256–274) draws
only `RoleText { label }` + `KbdHint`. **Nothing distinguishes a
parent row from a leaf.** Affected surfaces, all through this one
component:

- the **main menu** root page (hamburger → Task/Board/… group parents),
- the **task context menu**'s four family parents
  (Convert ▸ / Reminders ▸ / Timers ▸ / Entries ▸, chunk-5 D-5),
- the compact replace-mode page and the fly-out instance (same
  component, same rows),
- every other object context menu that ever carries a parent row.

**Proposed general fix (one place, not per-instance).** In the
`ActionMenu` row's trailing `VerticalLayout`, after the `KbdHint`,
render the existing right-pointing glyph when the row is a parent:

```slint
if entry.submenu: Icon { kind: IconKind.chevron-right; }
```

- Uses the already-vendored `chevron-right.svg` — zero new assets, no
  vocabulary growth decision needed (it is an affordance cue, not a new
  action glyph; if the house wants it recorded as a theme decision, one
  census line covers it).
- A11y: parent rows' `FocusScope` keeps `AccessibleRole.button`; the
  description already says the row opens a subgroup (round-1 D7 data).
  Optional refinement: `has-submenu` accessible state when Slint 1.18's
  role set exposes one — it currently does not (recorded limitation).
- Compact replace mode's hand-written "Back" row may symmetrically gain
  `chevron-left` (optional, walkthrough-graded).
- Tests: presentation/golden only (the flag is already in the model —
  the change is pure rendering); re-baseline the menu goldens.

## 4. Dropdown reuse inventory

### 4.1 The existing widget's capabilities (`ui/widgets/dropdown.slint`, r4 D39 + round-5 D6)

- Pushed model `[DropdownRow]`: label, indicator glyph
  (none/link/link-off), tone (RowDot's five-colour language),
  selected, busy; closed-field text/label/description, busy pulse,
  recognition glyph (archive), empty-rows caption, platform-gate
  enablement.
- Full keyboard ladder widget-internal (Return/Space open, arrows +
  C-n/C-p clamped highlight, Return select, Escape close-only, Tab
  close-and-reject), open-time anchor on the selected row, focus
  hand-back to the field.
- Window-level flyout: the widget mirrors open state, anchor geometry,
  and ladder state into the `DropdownFacts` global keyed by an **int
  `site` discriminator**; `app-window.slint` mounts one
  `DropdownFlyout` + dismissal underlay **per hardcoded site**
  (currently 0 = settings board picker, 1 = board-bar switcher,
  2 = board-bar strip-mode twin, 3 = archived-boards roster).
- Callbacks: `row-activated`, optional `row-context` (right-click seam),
  `row-hovered/unhovered` (consumer-owned tooltip policy);
  adapter-driven `open-requested` (#794 flow arming).
- Mounted today in: settings-view (`RemotePicker`,
  `DeckPublishPicker`), board-bar (two sites).

### 4.2 Task-details surfaces that should use it (or its successor)

| Surface | Current implementation | Dropdown fit |
|---|---|---|
| Priority picker (UX R-3 D-1, #788) | bespoke `PrioritySlotRow` overlay | high — closed six-value list, single select, exactly the widget's shape |
| Labels picker (UX R-3 D-2, #789) | bespoke chip toggles | medium — needs multi-toggle semantics the widget lacks |
| Time combobox (r4 D26) | `TimeField`/`TimeSlotRow` (its own flyout grammar, `TimeFieldFacts`) | low — time slots have their own ladder; converging is optional |
| Recur-editor weekday pick (round-7 D-7.1) | `SegmentedControl`, with a **recorded deviation** naming why Dropdown was rejected | the deviation documents the blockers (below) |
| Stack/board chooser on the detail view (if ever added) | — | high |

### 4.3 Gaps that block reuse (the recorded deviation + this audit)

1. **Flyout-site coupling**: every new consumer needs a new integer
   site id, a per-site `DropdownFlyout` mount arm in `app-window.slint`
   (4 copies of near-identical geometry code already), and a
   capture-tunnel exception so the ladder keys reach the field. This is
   therecur-editor deviation's "flyout-site coupled to the window-level
   flyout panels and the keyscope ladder — hostile inside a total-mask
   modal".
2. **Single-open global**: `DropdownFacts` holds one open site — two
   mounted dropdowns cannot coexist statefully (fine today, limiting
   inside multi-picker detail views).
3. **Single-select semantics**: no toggle/multi-select rows (labels),
   no separators/section headers, no search over long lists.
4. **Successor direction (for the operator to ratify)**: replace the
   int-site discriminator with a self-registering mount (the widget
   declares its flyout window-side from its own facts, as `ActionMenu`
   already does via `MenuPanelFacts` without a site enum), or at
   minimum generate the per-site mount arms from one parameterized
   component. That unblocks priority + labels + any future detail-view
   picker without touching app-window each time.

## 5. Related open issues

### 5.1 #927 — stale pinned object-menu rows

The MenuKind::Object pin (chunk-5 D-1) guarantees the built-once row
set; a subject deleted mid-open leaves dead-but-harmless rows
(dispatch-time gating covers every row). The issue proposes a
reconcile-driven disarm closing an object menu whose subject vanished.
Fold-in for round 9: any quick-bar/multi-bar work composes with this —
the bar is part of the pinned model, so a disarm closes bar + rows
together; implement the disarm in the same chunk as the multi bar if
dispatched together, else leave #927 homed as-is.

### 5.2 #938 — Labels row on the card popup

`LabelManage` is deliberately absent from TASK_CONTEXT (tag-color W-2
scoped it to BOARD_CONTEXT + detail toggles). Round-9 option: add the
row to the task context menu's core set (routes `LabelManage` with the
card's board as subject, roster-only mount). It is a menu-row addition,
not a hot-bar button (it opens a manager, not a quick verb). Cheap
discoverability win; needs a one-line UX-plan amendment rather than a
new plan.

## 6. Open questions for the operator

1. Was the "hot-bar missing" observation taken on a **post-#933**
   build? (Merged 2026-11-18 01:02 UTC — if the pass predates it, the
   finding dissolves and only §1.4's residuals stand.)
2. Should the **multi-selection** context menu carry the quick bar
   (origin §1's "multi-sensible" reading), or is the chunk plan's
   narrower D-4 reading the ruling? A one-line decision; the wider
   reading is a small chunk.
3. Sub-menu arrow: plain `chevron-right` glyph on `entry.submenu`
   rows — confirm the walkthrough-graded aesthetic (size/tint follow
   the icon-button neutral tone; no new theme decision assumed).
4. Dropdown successor: ratify the de-site-ing direction (§4.3.4) before
   any task-details picker work chunks, or accept per-site mounts for
   one more consumer (priority picker only)?
5. #938: add `LabelManage` to the card context menu this round, or
   leave demand-gated as the issue says?

## 7. Suggested chunk boundaries

- **Chunk A — verification + sub-menu arrow** (S): re-verify the bar on
  a post-#933 build; the `action-menu.slint` ▸ fix + golden re-baseline
  + hint-keys on bar tooltips. One branch, no plan amendment needed
  beyond a census line.
- **Chunk B — multi-selection quick bar + #927 disarm** (M): the
  multi-sensible bar composed at open over frozen selection facts, and
  the reconcile-driven disarm riding the same MenuKind seam. Needs the
  operator's §6.2 ruling; folds #927's fix.
- **Chunk C — dropdown de-siting + priority picker** (M–L): the
  successor mount grammar (§4.3.4) plus the first task-details consumer
  (priority), labels picker if multi-toggle semantics are added.
  Depends on §6.4's ruling; #938's row can ride either this chunk or A.
