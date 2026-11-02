# [Plan] GUI Review Round 4, Chunk 6 — Task-View Focus & Insert Engagement

- **Date**: 2026-11-01
- **Status**: PROPOSED — stage-1 plan for chunk 6 of the round-4
  dispatch. Branch `fix/ui-r4-taskview-focus`, cut from `origin/main`
  (d9988860; `scripts/worktree-doctor.sh` passed pre-creation, 195 G
  free against the 25 G floor; gate chunk 5 verified merged — PR #585,
  rebase-merged 2026-11-02).
- **Origin** (normative): design round
  `2026-11-01-gui-review-round4-plan.md` §3.6 (D18–D21), §1 findings
  R4-F12/F24/F26, §4 (focus/insert testing), §7-R3; dispatch plan
  `2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  §1/§7 (acceptance a–f; blanket acceptance rule; code-quality
  mandate per §1).
- **Scope guard**: `taskboard-ui-slint` only (slint + its `src/`).
  `taskboard-domain`, `taskboard-state`, storage, sync, app: **empty
  diff** (review focus f). Non-goals per dispatch §7: no date-picker
  phase machinery (chunk 7 — this chunk lands D20's masking rule and
  records the seam contract chunk 7 builds on), no reminder-row
  content (chunk 8), no reminder grammar/validity work (chunk 7), no
  IconButton activation changes outside the enumerated cells.
- **All `file:line` anchors are this worktree at d9988860**; the
  dispatch prompt's anchors predate chunk 5's task-view rework and
  were re-derived — every one below was read, not carried over.

## 0. Findings → mechanisms audit (what the code confirms)

- **F12 (Tab does not cycle all interactables; no active indication)**
  — the task view's interactive surface splits three ways after
  chunk 5. (a) Real tab stops already: the header IconButtons
  (`focus-surface` overlay, `icon-button.slint:45-72`), the done
  checkbox's `done-focus` FocusScope with Return/Space activation and
  the ring (`task-view.slint:205-224`, the closed-cell grammar chunk 5
  landed), and every TextField (native input focus). (b) Dead stops —
  hover+click but no FocusScope, no key activation, no ring: the
  closed title (`task-view.slint:278-302`), the closed due cell
  (`:354-375`), the closed scheduled cell (`:394-413`), the closed
  description (`:516-538`), the rule preset chips
  (`rule-list.slint:81-105`). (c) Non-element: the tracked summary is
  data, deliberately not a control (`task-view.slint:444` "not itself
  clickable"). The walk machinery itself is preset-complete —
  `nav.next-control/prev-control` resolve in all five view contexts
  (`presets.rs:184-205` standard, `:400-415` emacs, `:533-554` vi),
  route to `ControlNext/Prev` (`router.rs:469-470`), and re-inject a
  synthetic Tab into the runtime traversal
  (`adapter.rs:1271-1275`, `dispatch_traversal` `:1410-1430`,
  `walk_reentry` guards re-entry `:424-426`). The gap is purely that
  the walk has nothing to land on for (b).
- **F24 (reminder-field Enter confusion)** — root cause confirmed.
  The rule-list custom offset field has no engagement path at all:
  `rule-list.slint:106-118` forwards only `accepted`; there is no
  `edited`→engage seam (the timers form has one,
  `timers.slint:33-37/106/113`; the settings fields have one,
  `adapter.rs:2620-2666`). While typing there, `shell.mode` stays
  `Normal`, the chain stays `[TaskDetail, ViNormal, Global]`, and
  emacs Return resolves `task.title-edit`
  (`presets.rs:325-328`) → arms title editing mid-typing. The two
  per-mount engage seams that exist are keystroke-triggered
  (`edited`), not focus-triggered — a field that is focused but not
  yet typed into still resolves view bindings (Return under emacs
  hijacks before the first character). The flow-driven editors (title
  `view_state.rs:1074-1081`, description `:1032-1039`, dates
  `:1010-1019`) arm `Mode::Insert` via flow outcomes and are therefore
  masked correctly today — which is exactly why the bug is
  wiring-shaped: a field whose wiring was forgotten (the reminder
  field) falls out of insert engagement entirely. The class fix is the
  widget publishing focus facts (D20), not a third seam.
- **F26 (hover messages missing)** — tips exist on the done checkbox
  (`task-view.slint:241-255`), the tracked total (`:463-479`), and
  every IconButton (back, tracking, rule remove — the widget's own
  arm). Missing: the four closed cells, the preset chips, and all
  TextField-class fields in the view (title/description/custom
  offset/date editors — `TextField` has label/description for a11y
  only, no hover arm; the settings fields' tips come from the
  `LabeledField` wrapper, which the task view does not use).
- **Slint runtime facts the mechanism relies on** (verified against
  the pinned `i-slint-core-1.18.1` source): a focused element that
  unmounts (a conditional `if` subtree) never fires its
  `changed has-focus` handler — the window's focus weak-upgrade
  fails and no event reaches the destroyed element
  (`window.rs:846`, `:1304`); the adapter's `refocus-view` seam
  (`app-window.slint:693-695`, driven `adapter.rs:654`) exists
  precisely because flow ends unmount focused fields. The singleton
  needs explicit reset seams (§3.4c). Capture-phase interception
  visits the focused item's ancestors (`app-window.slint:246-248`),
  so `intercept-key` sees every keystroke regardless of what holds
  focus — both the D20 masking and the cell reservation are single
  seams at that one doorway.

## 1. D18 — the focus-stop inventory and the closed-cell grammar

### 1.1 The inventory (task view; the chunk's deliverable)

Walk order = tree order (the runtime traversal). "Shape" is the
FocusScope carrier; "fact" is what the element publishes into the
editing-facts singleton (§3).

| # | Element | Shape | Activation | Fact | State |
|---|---|---|---|---|---|
| 1 | Back to board | `IconButton.focus-surface` | Return/Space element-level (existing) | none | exists |
| 2 | Done checkbox | `done-focus` FocusScope (done-box grammar) | Return/Space → `task-done-toggled` | activation | exists (chunk 5) |
| 3 | Closed title cell | NEW FocusScope overlay (done-box grammar) | Return/Space → `title-edit-opened` | activation | **add** |
| 4 | Tracking start/stop | `IconButton.focus-surface` | element-level (existing) | none | exists |
| 5 | Closed due cell | NEW FocusScope overlay | Return/Space → `due-edit-opened` | activation | **add** |
| 6 | Closed scheduled cell | NEW FocusScope overlay | Return/Space → `scheduled-edit-opened` | activation | **add** |
| 7 | Tracked total | — not a control (a11y text + tip only, chunk 5) | — | — | stays out |
| 8 | Rule rows: remove | `IconButton.focus-surface` | element-level (existing) | none | exists |
| 9 | Preset chips (one per chip) | NEW FocusScope overlay in the chip body | Return/Space → `add-preset-clicked(i)` | activation | **add** |
| 10 | Custom offset field | `TextField` (native input focus) | Return → `rule-add-custom-accepted` | text | exists |
| 11 | Closed description | NEW FocusScope overlay | Return/Space → `description-edit-opened` | activation | **add** |
| 12 | Open editors (title/description/dates) | `TextField` | Return commits | text | exists |
| 13 | Date-picker grid (while open) | its own FocusScope + the `DatePicker` context (round-3 D25 seams) | picker keys | — | untouched |

The FocusScope shape is the **done-box grammar verbatim** (the
chunk-5 precedent, `task-view.slint:190-261`): a full-size child
`FocusScope` with `focus-on-tab-navigation: true`, the
`accessible-role: button` + label/description/`accessible-action-default`
triple carried over from the cell's existing a11y strings, a
`key-pressed` handler accepting exactly bare Return/Space, and the
ring rendered by the parent cell's border driven by
`has-focus`. The ring visual reuses the D15 tokens —
`Semantic.focus-ring` at `Theme.focus-ring-width`, width 0 when
unfocused (the `IconButton` idiom, so the hover surface stays the
rest-state differentiator; done-box's always-on border stays as is).
The chips factor into a small local `PresetChip` component inside
`rule-list.slint` (the `for` body otherwise triples); the chip keeps
its token floor pin (`journeys.rs:5772`).

The title cell mounts its FocusScope on the **closed** branch only;
the open `TextField` replaces it (the display/field pair in one cell,
chunk 5's shape). Walk reaches the field, not two stops.

### 1.2 Other views — no new stops this chunk

D18's scope is the task view (acceptance a enumerates its elements).
The other surfaces' inventories are recorded here as the baseline the
structural test guards, all already real: board (cards, column
headers, board-bar segments, add-task ghosts, create/rename fields),
today (rows, timers form + cancel buttons, habits controls), stack
detail (rows, create field), settings (rows, fields, scale), the menu
(cursor-driven, no tab stops by design), palette/capture (field +
rows), date picker (grid FocusScope). The structural stop test (§6.A)
asserts the task view's sequence only; the others are untouched by
this diff and pinned by the existing walk journeys
(`the_control_walk_reaches_fields_buttons_and_the_picker`,
`journeys.rs:5854`, extends to cover the new stops).

## 2. Closed-cell activation seams (Enter/Space/click → existing effects)

- **Click** already routes the actions (`title-edit-opened` /
  `due-edit-opened` / `scheduled-edit-opened` /
  `description-edit-opened` / `add-preset-clicked`) — no new effects,
  no new callbacks; the new FocusScopes call the same window seams the
  TouchAreas call.
- **Return/Space element-level**: the new cells' `key-pressed`
  handlers fire where the runtime delivers — which is the crux. The
  key-root capture (`app-window.slint:241-244`) sees every key first;
  any view-context binding on bare Return/Space would consume the
  stroke before the focused cell's handler runs. Today's table: under
  **standard**, Return/Space are unbound at `[TaskDetail, ViNormal,
  Global]` → cells already receive them (this is why chunk 5's
  done-box works); under **emacs**, Return resolves `task.title-edit`
  (`presets.rs:328`) → hijack; under **vi**, Return resolves
  `task.edit` → `Activate` (re-opens the subject) and Space resolves
  `task.complete` → toggles done — both consumed, the cell never sees
  them. This is not a corner: chunk 7's two-phase emacs flow (F21:
  *walk to due → Enter*) is unreachable unless Enter on the focused
  due cell reaches the cell under emacs.
- **The mechanism (D18's reservation)**: the activation-focus fact
  (§3, second property of the singleton) arms a reservation in
  `intercept` beside the existing EditDate Down arm
  (`adapter.rs:438-449` — the recorded precedent for stroke-specific
  early arms). While an activation stop holds focus and the stroke is
  bare Return/Space (no modifiers), the resolver is still fed first
  (prefix semantics preserved): a `Pending`/`Aborted` answer consumes
  as today (an emacs pending prefix aborts on Return exactly as
  now); an `Action` answer or `Unhandled` yields — `intercept`
  returns false and the focused cell's `key-pressed` activates.
  Effects: emacs Return on any cell opens that cell's editor (the
  two-phase entry); vi Space on a cell opens the cell instead of
  toggling done (the done toggle stays on the checkbox row itself and
  the action's own binding when no cell holds focus); standard is
  unchanged in effect. **Recorded runtime delta**: vi/emacs
  Return/Space while a task-view *cell* holds focus now activate the
  cell — a deliberate resolution-semantics change bound to element
  focus (snapshots unaffected; the binding tables do not move). No
  shipped preset binds bare Return/Space in `Global`, so no global
  verb is shadowed; a future Global binding on those strokes would be
  shadowed while a cell holds focus — recorded as a known constraint.
- **Scope of the reservation**: only the task view's cells publish
  the activation fact (§1.1 rows marked "activation" — including the
  existing done-box, for uniformity of its Space path under vi). The
  board cards / today rows deliberately do NOT publish: their
  Space/Return semantics are the action layer acting on model focus
  (the house pattern) and must not change. IconButtons do not publish
  — their Return under emacs/vi stays action-shadowed (recorded
  limitation; F12 asked for the walk + visuals, and the header verbs
  are key-reachable through their own bindings). The reservation arm
  also checks `menu.open`/`palette` are closed (shell state) — both
  reset the fact anyway (§3.4c), the check is the belt to the reset's
  braces.

## 3. D20 — the editing-facts singleton (the systemic insert rule)

### 3.1 The carrier decision: `Mode::Insert` as the mask, focus as the trigger

The design's contract is "the shell chain masks the view context
whenever any text field holds focus — the insert-mode chain
`[ViInsert, Global]` + non-masking heads". Two mechanisms satisfy it:

- **(A) chosen** — the focus fact drives `shell.mode`:
  `Mode::Insert` while any TextField holds focus. `chain()`
  (`shell.rs:66-99`) is **unchanged** — masking falls out of the
  existing insert branch, the D5 contract ("insert masks the view
  context") is verbatim, the `ViInsert`-shadows-without-masking rule
  (`presets.rs:52-58`, Ctrl-chords keep working while typing) is
  verbatim, and the `KeyboardState.vi-insert` mirror
  (`adapter.rs:348`) stays truthful. Mode widened to "a text field
  holds focus OR vi insert intent" is already the status quo — the
  settings/timers engage seams set `Mode::Insert` for every preset
  today.
- (B) rejected — a `KeymapShell::text_field_focused` fact consulted
  in `chain()`: two masking sources that can disagree, a lying
  `vi-insert` mirror while typing, chain-test churn, and a second
  Escape story to design. Recorded for history.

### 3.2 The singleton (declaration home + publish)

- **Declaration home**: new file `ui/editing-facts.slint` — the
  single-purpose globals-file precedent (`drag-state.slint`,
  `settings-facts.slint`, `menu-facts.slint`), so neither
  `text-field.slint` nor `task-view.slint` owns cross-view state.
  Exported through `app-window.slint`'s export list
  (`app-window.slint:51` pattern) so the generated Rust exposes it.
  Consumed by `text-field.slint` (publish), `task-view.slint` +
  `rule-list.slint` (activation publish), `adapter.rs` (consult +
  reset).

```slint
export global EditingFacts {
    // Mounted TextFields whose inner input holds slint focus.
    // +1 on gain, −1 on loss; the adapter resets to 0 at the seams
    // where a focused field can unmount without a loss event (§3.4c).
    in-out property <int> text-focus-count: 0;
    // The task view's closed-cell activation stops (§2).
    in-out property <int> activation-focus-count: 0;
}
```

- **The publish** (`text-field.slint`, inside the existing
  `changed input-has-focus` block, `:79-84`):

```slint
changed input-has-focus => {
    EditingFacts.text-focus-count += self.input-has-focus ? 1 : -1;
    if (!self.input-has-focus) { root.focus-lost(); }
}
```

  A **count, not a bool**: focus moving field A → field B fires the
  two `changed` handlers in unspecified order; a bool can land B's
  `true` before A's `false` and stay false forever (stuck-unmasked),
  while the count is order-independent. The activation cells publish
  symmetrically on their FocusScopes' `has-focus`.
  **Field** (the type-level statement of why the field cannot
  forget): the publish lives in the one widget every text input
  composes — a new field anywhere is masked by construction, zero
  per-mount wiring. The probe class for the count lifecycle exists
  (`focus_seam_probe_tests.rs`, `walk_probe_tests.rs` P2).

### 3.3 How the shell consults it (pull at the one doorway)

`intercept` (`adapter.rs:418-473`) gains two pulls before resolver
feed — no push callback, no changed-watcher on the global (Slint
globals don't notify Rust; pull-at-intercept is self-healing by
construction since every keystroke re-reads):

- **Engage pull**: `text-focus-count > 0 && mode != Insert` →
  `shell.apply(SetMode(Insert))` (+ `sync` for the mirror). The
  engage is then idempotent against the flow outcomes
  (`begin_edit_title` etc. already set Insert).
- **Disengage pull**: `text-focus-count == 0 && mode == Insert &&
  !date_picker_grid` → `SetMode(Normal)`. The grid exclusion is
  load-bearing: during the picker phase the flow holds Insert while
  the *field* lost focus to the grid — flipping Normal there would
  degrade the chain to `[DatePicker, ViNormal, Global]` and hand
  Escape to vi's move-cancel instead of the clean close (the existing
  journey `journeys.rs:5930-5938` pins the clean close; it stays
  green).
- **Reservation arm**: `activation-focus-count > 0` (and menu/palette
  closed) + bare Return/Space per §2 — after the resolver feed, per
  the answer table there.
- Hint projections read the same shell: a refresh while a field holds
  focus projects hints over the masked chain (a field's tooltip never
  advertises view verbs mid-typing). No code change needed — the
  projections already walk `shell.chain()`.

### 3.4c Lifecycle: which seams die, which stay, and the reset seams

**Die** (their only job was mode symmetry for TextFields; focus does
it now):

- `timers_form_engage` + its disengage (`adapter.rs:4959-4975`) and
  the whole `timers-form-edited` callback chain
  (`timers.slint:33-37/106/113` → `today-view.slint:67/115` →
  `app-window.slint:317/859` → `adapter.rs:5018-5022`) — removed
  through all layers.
- `settings_field_insert_engage` (`adapter.rs:2649-2656`) and the
  mode flip in `settings_field_committed` (`:2660-2666`) — the buffer
  mirroring in `settings_field_edited` stays. If
  `settings-field-committed`'s window callback ends up empty, it is
  removed too (grep-verifiable: no per-mount `edited`-or-commit seam
  may write mode afterwards — review focus a's grep).

**Stay** (non-TextField inputs and non-mode seams):

- The flow-driven `SetMode` outcomes (title/description/date/create/
  rename/capture/palette arming and ending) — they own flow
  transitions, not engagement; they converge with the focus fact
  idempotently.
- The date-picker **grid** focus seam (`date_picker_grid_focus_changed`
  → the `DatePicker` chain head) — a non-TextField input with its own
  context, exactly the "heads that stay" class.
- The settings scale slider's gesture seams (no mode involvement).
- `text-flow-focus-lost` (commit/discard on focus steal — commit
  semantics, orthogonal to masking).
- IconButton keyboard activation (element-level, untouched).

**Reset seams** (the unmount hazard, verified against the pinned
runtime in §0): a focused field that unmounts fires no loss event —
the count would stick at 1 and mask the shell forever. The adapter
resets **both counts to 0** at the seams where unmounts happen,
which are exactly the seams that already know the tree changed:

1. **Flow ends** — the settle/commit/close funnel that drives
   `invoke_refocus_view` (`adapter.rs:654`): every field-unmounting
   commit/discard lands here (title, description, dates, create,
   rename, capture, palette close).
2. **View swaps** — the enumerate-once view-swapping effect arms
   chunk 4 landed for the tooltip total-dismiss (the same list: view
   switches, settings open/close, board select, task open/close).
3. **Menu/palette open** — the modal surfaces refocus; the
   reservation must not outlive the surface that armed it (belt for
   §2's check).

Self-healing: a field that mounts focused (`autofocus`) fires
`changed` true on its `init` focus grab, re-publishing after any
reset. A field that commits but stays mounted and focused (settings
form, timers form, custom offset field) never hits a reset seam
before the user's next keystroke — and correctly stays masked: under
D20 its Return is a commit every time (today, vi's second Return on
the committed timers form hijacks to `task.edit`→`Activate` and can
open a task view from the today form — D20 kills that too; recorded
delta).

### 3.5 The enumerated exceptions

- **The search palette** (the named one): its chain branch precedes
  the mode check (`shell.rs:70-75`) and `Palette` masks
  (`presets.rs:52-58`) — `[Palette, ViInsert, Global]` regardless of
  the focus fact; Return resolves `MenuInvoke` at the head and the
  capture consumes it before the field's `accepted`
  (`presets.rs:167-173` comment). The palette query field participates
  in the count (it is a TextField) — harmless: the palette branch
  wins either way. Pinned by journey (§6.C7).
- The **menu**: no field; masked modal chain `[Menu]`.
- The **date-picker grid**: its own non-masking head context; Escape
  stays the clean close (§3.3's exclusion pins it).
- Everything else — title, description, due/scheduled editors,
  custom offset, timers form, settings fields, board/stack
  create/rename, capture — has field-commit Return semantics and is
  *covered*, not exempt: that is the rule.

### 3.6 Chunk 7 seam contract (recorded, not implemented)

1. The `DatePicker` chain head stays **non-masking** (never enters
   `Preset::masks()`); its Escape contract (clean close through
   `ModeNormal` from `ViInsert`) is pinned today and chunk 7 must
   keep it.
2. The picker's **field-phase masking is exactly D20** — the mounted
   date field is a TextField, publishes into the count, and masks the
   view context with zero new wiring. Chunk 7 must not add any
   per-field engage seam; a new engage seam after this chunk is a
   review finding (grep clause, §6.F).
3. The **picker phase** keeps its head-over-insert chain
   (`[DatePicker, ViInsert, Global]`) and the disengage pull's grid
   exclusion (§3.3) — chunk 7's two-phase ladder arms on
   `date_picker_grid`, not on new mode writes.
4. The two-phase entry (walk to due → Enter) is **this chunk's
   activation reservation**: emacs Return on the focused due cell
   reaches `due-edit-opened` (§2). Chunk 7's acceptance journey
   starts at a seam that exists after this chunk.

## 4. D19 — the emacs C-n/C-p rows (conflict-contract analysis)

Emacs today: C-n/C-p bind `NavNextItem/NavPrevItem` in Board
(`presets.rs:299-300`), Today (`:313-314`), StackDetail
(`:369-370`), TaskDetail (`:375-376`); the control walk binds
C-f/C-b in Today/StackDetail/TaskDetail/Settings (`:400-415`) —
Board keeps C-f/C-b for `stack.next/stack.prev` (`:331-334`).

| Context | C-n/C-p today | Verdict |
|---|---|---|
| Board | live task/stack walk | **untouched** (design mandate; the round-3 contract `emacs_control_walk_preserves_the_board_chords`, `keymap/mod.rs:647-675`, extends to pin this) |
| Today | live row walk | **excluded** — occupied by a live action; not conflict-clean |
| StackDetail | live task-list walk | **excluded** — same |
| TaskDetail | `NavNextItem` walks the *hidden board* under the open view (`step_focus` has no TaskDetail arm, `view_state.rs:716-766`) — semantically dead for the visible surface | **re-purposed**: the two rows move from `NavNextItem/PrevItem` to `NavNextControl/PrevControl`. The intra-context conflict contract (`presets_are_intra_context_conflict_free`, `keymap/mod.rs:813`) forbids a second action on the same (context, stroke) — this is a move, not an add. F12's four-chord requirement (C-n/p/f/b all walk interactables) is met. |
| Settings | unbound | **added**: `NavNextControl/PrevControl` rows on C-n/C-p (the design's "non-Board contexts where they are conflict-contract-clean") |
| Menu/Palette | `MenuNext/MenuPrev` C-n/C-p | untouched — modal masked chains, no contract contact |
| DatePicker | `PickerDown/Up` C-n/C-p (chain head) | untouched — chain-head priority shadows the walk strokes while the grid focuses; different contexts, no contract contact (the recorded round-3 shadowing pattern, `presets.rs:432`) |

Contract impacts, all verified against the suite: the move keeps
`presets_are_intra_context_conflict_free` (the duplicate would fail
it); no new prefix relations (C-n/C-p are single strokes, prefixes of
no emacs sequence) so `presets_are_prefix_shadow_free`
(`keymap/mod.rs:829`) holds; `every_preset_covers_the_core_verbs`
(`:426-491`) holds — `NavNextItem` stays bound in
Board/Today/StackDetail; the vi-shape contract (`:856`) and the C-g
contracts (`:615`, `:733`) are untouched (vi and standard rows do not
move — acceptance d: snapshots diff only the intended emacs rows).
`NavFirstItem/NavLastItem` Alt+</Alt+> rows at TaskDetail stay (harmless,
unchanged semantics). Snapshot: `resolved_preset_keymaps` regenerates;
`registry` does not change (no registry rows move).

## 5. D21 — the hover sweep (task-view inventory; presentation constants)

Every interactive task-view element arms the tooltip with
label + description + hint keys where bound (empty `[]` where the
routed action has no preset binding). Labels/descriptions reuse the
cell's a11y strings verbatim (the house rule: the tooltip never says
what the a11y tree doesn't). All copy is presentation constants;
hint projections are keymap data pushed per refresh, never
hand-written (`task-done-hint` precedent, `app-window.slint:137`).

| Element | Label | Description | Keys |
|---|---|---|---|
| Closed title | "Edit title" | "Arms the title editor for the viewed task." | `task-title-edit-hint` (NEW projection over the view chain — emacs Return / standard F2; vi unbound → empty) |
| Done checkbox | done/not-done verb pair | "Toggles the task's done state." | exists (chunk 5) |
| Back / tracking | existing | existing | exists |
| Closed due cell | "Edit due date" | "Opens the due-date editor." | `[]` (no opener action) |
| Closed scheduled cell | "Edit start date" | "Opens the start-date editor." | `[]` |
| Closed description | "Edit description" | "Opens the description editor." | `[]` |
| Preset chips | the chip's preset label | "Adds this reminder offset." | `[]` (`ReminderRuleAdd` has no preset rows — menu-only) |
| Custom offset field | "Custom reminder offset" | the field's a11y description line (NEW: one constant, e.g. "Adds a reminder the given time before the due date.") | `[]` |
| Remove buttons | existing | existing | `[]` (already so) |
| Tracked total | exists (chunk 5) | exists | `[]` |

Mechanism for the **fields**: `TextField` grows an optional tip arm —
three `in` properties (`tip-label`/`tip-description`/`tip-keys`,
default empty = no arm anywhere else) plus a hover TouchArea that
calls `TooltipState.hover/unhover` (the IconButton arm's shape,
`icon-button.slint:107-124`; TouchAreas coexist with the input — the
eye/commit affordance precedent). This is the one widget-level
mechanism; the task view's four field sites (title, description,
custom offset, both date editors via `DueEditor`) opt in. The closed
cells arm via their TouchAreas (the done-box pattern, extended from
hover-only to hover+ring on the new cells). The structural sweep test
counts distinct tip spots per element id (the
`every_settings_field_carries_its_info_tip` pattern,
`journeys.rs:4022-4070`).

## 6. Test catalogue (TDD; red where the behaviour is new)

### A. Structural — stop inventory (acceptance a)

- **`the_task_view_stop_inventory_walks_in_reading_order`**: the new
  focused-stop facts global (each stop's `changed has-focus` writes
  its stop id — the `WalkProbeFacts` pattern) asserts the §1.1
  sequence under repeated `NavNextControl`, including wrap, with the
  open-editor branches (field replaces cell, one stop) and the
  date-picker grid interposed while open.
- **Title-focus journey** (F12's wording): the active-element fact is
  non-empty at every walk position; one golden with a focused closed
  cell pins the ring visual (regenerates with the sweep).

### B. Contracts & snapshots (acceptance b; review focus d)

- Extend `emacs_control_walk_preserves_the_board_chords`
  (`keymap/mod.rs:647-675`) →
  `emacs_four_chords_walk_controls_on_the_detail_chain`: C-n/C-p/C-f/C-b
  all resolve `NavNextControl/PrevControl` over
  `[TaskDetail, ViNormal, Global]`; Board C-n/C-p still resolve
  `NavNextItem/PrevItem`; Today/StackDetail unchanged; Settings
  resolves the walk on both C-f/C-b and C-n/C-p.
- The generic contracts (`intra_context_conflict_free`,
  `prefix_shadow_free`, `core_verbs`, `vi_shape`, C-g pair) stay
  green unmodified — a failure is a finding, never a weakened test.
- Snapshot regen: `keymap_snapshots__resolved_preset_keymaps.snap`
  only (INSTA_UPDATE=always run); the diff must contain exactly the
  emacs TaskDetail move + emacs Settings additions.

### C. Journeys — D20 (acceptance c/d; §7-R3's gate)

- **The Return-never-hijacks matrix** (per preset × field, the
  `press_preset` helper `journeys.rs:5046` switches presets):
  fields = the custom reminder offset field (F24's repro verbatim),
  the title field, the description field, the timers when-field.
  Each cell: open the view, focus the field (`click_by_label`),
  stroke Return, assert (i) no title-edit flow armed
  (`task_model.edit_open` false where applicable), (ii) the field's
  commit landed (`assert_commands` on the exact typed command, or
  the flow's commit), per the field's own semantics. 12 cells; the
  emacs column is the one red today.
- **The palette exception pin**: palette open → Return resolves the
  jump (view/board change), never a field commit, under all three
  presets (`the_palette_searches_and_jumps_across_boards`,
  `journeys.rs:4670`, extends with the explicit assertion).
- **The engage-focus delta pin**: focusing a settings field by click
  alone (no keystroke) masks the chain — `vi_insert` mirror true
  with zero `edited` events (the old seams could not do this).
- **The count-lifecycle probe**: field A focused → field B focused
  (count stays 1 through the unspecified handler order); commit a
  flow field via Return (count resets to 0 at the flow end; the next
  Return resolves view bindings normally — the unmask pin); a
  settings field click-away (loss event, count −1 without any reset
  seam). Probe-level facts on the generated global.
- **Existing field journeys stay green unweakened** (review focus f):
  the rule-list boundary (`:4139`), parse-missed custom offset
  (`:5704`), timers form (`:8174`), title edits (`:8026`), commit
  affordances (`:5591`, `:5674`), due/scheduled editors
  (`:6208`, `:6339`), the picker clean-close (`:5930`), settings
  save/buzz (`:7226`). Any red is diagnosed, not relaxed (§7-R3).

### D. Journeys — activation reservation (§2; chunk 7's entry seam)

- **Emacs: Return on the focused due cell opens the due editor**
  (walk → NavNextControl onto the cell → Return →
  `date_picker_open`/flow armed; the two-phase entry F21 needs).
- **The cell-activation matrix** (standard: all four cells + a chip;
  emacs: Space variants; vi: Return on due + Space on due — the
  recorded deltas asserted as the new semantics, plus Return on the
  done-box toggling done).
- **The reservation's prefix discipline**: with an emacs pending
  prefix (C-x) Return aborts (consumed), the cell does not activate —
  pins the feed-first ordering.

### E. Hover sweep (acceptance e)

- **`every_task_view_element_carries_its_info_tip`**: distinct tip
  spots per element id across the closed cells, chips, fields, and
  the existing controls (the settings sweep's mechanism); runs per
  the closed/open editor branches.

### F. Review greps (stage-3 clauses, listed for the implementer)

No per-mount mode write outside the flow outcomes after this chunk
(`grep -n "SetMode" src/` → flow arms/ends + the intercept pulls
only); no `form-edited` residue; the singleton has exactly two
properties; empty diff over domain/state/storage/sync/app.

## 7. Commit slices (stage 2; standalone, rebase-clean)

1. **D20 core**: `editing-facts.slint` + the `TextField` publish +
   the intercept pulls/resets + the engage-seam removals (timers
   chain, settings flips) + the count-lifecycle probe + the
   Return-never-hijacks matrix + the palette pin + the focus-engage
   delta pin. Existing field journeys green in this commit.
2. **D18 stops + reservation**: the four cell FocusScopes + the chip
   component + the activation publish + the reservation arm + the
   activation/reservation journeys + the stop inventory + the
   focused-cell golden + the title-focus journey.
3. **D19 rows**: the emacs move + Settings additions + the extended
   contracts + the snapshot regen (same commit as the rows).
4. **D21 sweep**: the `TextField` tip arm + the cell/chip arms + the
   projections (`task-title-edit-hint`) + the structural sweep +
   CHANGELOG Unreleased entry (Fixed: F12/F24/F26 phrasing).

## 8. Risks & recorded probes

- **R3 (origin §7)** — D20 changes key resolution globally: the
  contract suite + snapshots + the 12-cell matrix are the gate; any
  existing field journey regression is a finding (never a reweakened
  test). The palette exception is pinned explicitly.
- **The unmount stuck-bit** (runtime-verified, §0) — mitigated by
  the reset seams + the lifecycle probe; the probe is the tripwire
  (a future field mounted outside the reset seams' coverage fails
  the unmask pin).
- **The disengage pull's grid exclusion** — without it the picker
  phase degrades (Escape → move-cancel); the existing clean-close
  journey is the pin. Chunk 7 inherits this as seam-contract item 3.
- **The vi/emacs cell-activation delta** (§2) — deliberate, recorded,
  snapshot-invisible; stage 3 reviews it as focus item (c)'s runtime
  companion, not as a regression.
- **Handler-order dependence** — the count semantics remove it; the
  probe pins the A→B transition.
- **`Theme`/token purity** — the ring reuses `Semantic.focus-ring` /
  `Theme.focus-ring-width`; no new tokens.
- **Kiosk**: no kiosk-gated surface changes (the task view is
  desktop-only; the timers form lives behind the existing
  desktop-only gate) — the kiosk pins (`:6880`) stay green.

## 9. Tier-4 checklist (manual, on the reference machine)

- Tab walk over the whole task view under all three presets: ring
  visible at every stop, no dead positions, wrap behaves.
- Emacs: C-n/C-p/C-f/C-b walk; walk to due → Enter opens the editor
  (the two-phase entry); type into the custom reminder field and
  press Return — commits, never arms the title.
- The click-away commit on a settings field then immediate typing in
  another field — no dead keys between fields.
- Palette under emacs: type, arrows, Return jumps; Escape closes.
- Hover sweep: every cell/chip/field tips on hover; no empty-line
  tips (chunk 4's fix holds).
- Scale 0.5 / 3.0: rings and chip hit-targets stay legible.

## 10. Bookkeeping

- Estimated diff: **~800–1000 lines** including tests and the snap
  regen (slint ~350, adapter/shell ~150, presets/contracts ~120,
  journeys/probes ~350, snap ~30).
- No new issues foreseen beyond the recorded IconButton-activation
  limitation (§2) — file as `backlog:` ui/p3 if stage 3 confirms.
- The CHANGELOG entry lands in slice 4 (stage 2).
