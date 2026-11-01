# [Plan] GUI Review Round 4 — Fix Round (board surface & column resizing, icon assets, hover machinery, task-view layout & focus, date/time entry, task reminders, timer surfaces, tracking surfaces, settings connections)

- **Date**: 2026-11-01
- **Status**: PROPOSED — written from the user's 2026-11-01 GUI review
  round 4 (29 findings, R4-F1…R4-F29 below) plus **four recorded
  decisions pending the user's confirmation** (§2). Companion dispatch
  plan: `.artifacts/plans/2026-11-01-gui-review-round4-implementation-dispatch-plan.md`
  — **11 chunks**, serial default with named parallel lanes.
- **Scope**: `taskboard-ui-slint` (most chunks) + `taskboard-domain` /
  `taskboard-state` / `taskboard-storage-sqlite` (chunk 8 only — the
  at/in reminder kind, one migration) + `taskboard-app` (expected
  empty; the per-second repaint condition lives in the ui crate's
  driver, and notification delivery reuses the existing sinks).
- **Queue relationship**: the in-flight backlog round-2 dispatch
  (`.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`)
  chunks **2.5 (#352 timers surface)** and **3.4 (#490/#491
  tracked-time surfaces)** overlap this round's findings 13/17/23 —
  §2 decision 4 absorbs them (pending user confirmation); every other
  backlog chunk is untouched. Rebase friction with the backlog
  render-lane chunks (2.1 #464, 4.1 #461 — `models.rs`) is expected
  and handled by the standard rebase dance.

## 1. The user's round-4 findings (numbered for reference)

1. Notifications: "Dismiss Notification" and "Mark task done" share
   one icon; find distinct icons, and evaluate icon sourcing options
   (icon font vs custom SVGs) with a proposal.
2. Today/board task titles truncate to ~7–8 chars + ellipsis far
   short of the card width; text must cut at the task box's edge.
3. Board column resizing: minimum width is far larger than content;
   dragging the right edge grows the column in both directions.
   Redesign to the standard divider interaction (Excel/Calc,
   Explorer/Dolphin, Qt): a divider between two columns redistributes
   the space between exactly those two neighbours; other dividers do
   not move.
4. Keyboard-shortcut hints were missing from hover boxes early in the
   session, appeared later — investigate.
5. Hover boxes render an empty line when the description is empty —
   don't.
6. Right-click outside an open context menu must close it (left-click
   already does).
7. Left-click on the empty parts of a stack's background card must
   select the stack (as right-click already routes the context menu).
8. "Add task" button needs two clicks on the active stack, three on a
   non-active stack, to open the new-task title input; Ctrl+O works
   first try. One click must reliably open the input.
9. Edit view: "Back to board" sits on its own row — put it in line
   with the task title.
10. Edit view: the title row is far too tall (reads as two empty
    lines around it); any element that eats leftover vertical space
    must be the editable description field, not padding.
11. Edit view: move "Start tracking" into the title row, right side.
12. Edit view: Tab does not cycle all interactable elements; fix for
    every preset where Tab cycles; emacs C-p/C-n/C-f/C-b must
    navigate interactables as on the board; the task title shows no
    active indication while tabbing — fix.
13. Today view: a mouse-operable GUI widget for setting timers,
    planned so the text-box interaction survives for keyboard-only
    users.
14. A hover message survived the board→edit-view transition (cleared
    only on the next hover); view transitions must not leave ghost
    hover messages.
15. Add a "Mark task done/undone" checkbox to the edit view, like the
    board/today cards have.
16. Edit view: a section showing total tracked time on this task,
    seconds being the finest unit shown.
17. Edit view: a button opening a detailed tracked-times list — date,
    start time, end time, time spent per entry, full total at the top
    — plus interfaces to add new entries (forgotten tracking) and
    edit past records (correct start/end), fully operable by mouse
    only, keyboard only, and mixed; may share widgets with the
    timer-setting widget (13).
18. Date pickers flip above↔below the field while paging months —
    investigate and fix.
19. Date picker: +/- steppers next to the time do nothing while the
    field is empty; only work after a time exists.
20. Date picker text fields: typing non-conforming text and clicking
    away onto the card background leaves the text — the interaction
    must close on click-away.
21. Emacs mode: C-p/C-n/C-f/C-b should navigate the date picker while
    the text field is not focused; selection changes must be reflected
    in the field text; desired flow: walk to due date, Enter, choose a
    day with C-p/n/f/b, type a time, Enter to finish.
22. Mouse interaction for the time part of the date picker: survey
    other apps/frameworks and recommend a pattern (done — §2
    decision 3).
23. Timers set in the edit task view don't count down; they must show
    (a) in how many d/h/m/s they will fire and (b) the fixed fire
    time in ISO `YYYY-MM-DD HH:MM:SS` form.
24. Manual reminder input: in one test Enter did nothing (checkmark
    button required); in another, Enter in the custom-reminder field
    started editing the task title. Investigate and fix so this class
    of confusion is architecturally unlikely to return.
25. Manually set reminders on tasks never fire and remain in the list
    forever — investigate and propose a fix.
26. Many edit-view elements have no hover messages — fix.
27. Add a syntax check to all date and reminder offset input fields in
    the edit view, reusing the settings-menu validation machinery.
28. Settings: scrap the scale slider's instant preview (the slider's
    position under the cursor shifts with each resize, feeding a
    flickering resize loop).
29. Settings connections: no hover feedback on the board-selector
    buttons; clicking a non-active board appears to do nothing;
    replace the buttons with a dropdown ("Select Board to
    Connect/Disconnect:" left of it when space allows, above it
    otherwise) carrying a connected/not-connected indicator —
    combinable with the board-view board-selector dropdown issue
    (#411).

## 2. Recorded decisions (PROPOSED — the user confirms or redirects at review; each names its alternative)

1. **Icon sourcing (F1)** — **vendor ~30–60 monochrome Lucide SVGs
   (ISC) into `ui/assets/icons/` and render them through a thin
   `Icon inherits Image` component (`@image-url` +
   `Image.colorize`)**, keeping `IconKind` as the single closed
   vocabulary gate. This amends D19's "shapes composed from
   primitives" clause to "vendored monochrome SVG assets, tinted via
   colorize"; the "no icon font" clause stands. Verified: SVG support
   is default-on for native desktop backends (bundled resvg, no cargo
   feature needed); `colorize` uses the rasterized alpha channel as a
   mask, so one monochrome asset tints per theme token — Slint's own
   documented icon pattern (`demos/usecases/ui/widgets/icon.slint`);
   assets are compile-time-embedded (offline kiosk-safe, ~20 KB for
   60 icons). Alternatives recorded: Tabler SVGs (MIT — same
   mechanism, denser style; purely aesthetic), icon fonts
   (maintainer-discouraged, tofu-risk, ligature-unsupported —
   rejected), hand-drawn Paths (status quo; rejected as the source of
   the duplicate-icon bug class), the `lucide-slint` crate (viable
   fallback; whole-set binary inclusion, single maintainer).
2. **Manual reminder semantics (F25)** — **add task-anchored "at/in"
   reminders**: the task view's reminder section gains a second kind
   that fires at an absolute instant ("at 2026-11-02 09:30", "in 10m")
   with or without a due date and settles after fire+ack like adhoc
   alarms. Due-anchored rules stay, but their rows show the armed
   state and effective fire time (root cause of F25: rules are
   due-anchored offsets — `ReminderOffset` ≤ 0 before the due date —
   so a rule on a task without a due date can never fire, and rules
   are only ever removed manually; the UI never communicates this).
   Domain extension + migration 0018 (chunk 8). Alternative recorded:
   keep due-anchored only and fix the communication (armed/not-armed
   rows + set-due shortcut) — cheaper, but "10m" keeps meaning "10
   min before due", not "in 10 minutes".
3. **Time-of-day selection (F22)** — **a hybrid "time combobox"**: one
   typable HH:MM field (lenient parse: `9`, `930`, `9:45`) with
   embedded up/down steppers (±15 min with midnight rollover, wheel
   support where the backend provides it) plus a dropdown quick-list
   of quarter-hour slots (:00/:15/:30/:45) that scrolls to the
   current value; Enter commits, Escape closes. Survey basis: desktop
   frameworks converge on the typed field + steppers (Qt `QTimeEdit`,
   MD3 input variant, `<input type=time>`) and Google Calendar
   desktop / USWDS add the quarter-hour quick list; radial dials are
   touch-first (large in-popup, mouse-imprecise, AM/PM-prone) —
   rejected. The widget is shared by the date picker's time part, the
   timer setter (F13), and the tracking-entry editor (F17).
   Alternatives recorded: steppers-only (rejected: common values cost
   many clicks); radial dial (rejected).
4. **Backlog queue overlap (F13/17/23 vs backlog chunks 2.5/3.4)** —
   **round 4 absorbs the two backlog chunks**: chunk 9 owns all
   timers-surface work incl. #352's remainder (its escalation/alarm
   window half already landed; stage 1 enumerates what's left against
   the DR-3 rulings), chunk 10 owns the tracked-time surfaces
   (#491 task-detail entry history + editor; #490 today totals rides
   along). Bookkeeping: after the user confirms, a small docs commit
   amends the backlog round-2 dispatch plan to drop chunks 2.5/3.4
   with pointers here; issues #352/#490/#491 close via chunks 9/10.
   Alternative recorded: keep the backlog queue and gate round-4
   chunks 9/10 on those merges (deferred deltas only) — rejected as
   it serializes two plans over the same files and delays the user's
   reported fixes.
5. *(Decided here, no confirmation needed)* **Per-second UI ticking**:
   the timers round's fence "no UI-held timer state machine — the
   minute repaint is the only clock" is amended: the repaint driver's
   dirty-check (`src/lib.rs:534-540`, already a 100 ms repeated
   timer) gains a condition — a refresh is also due when any mounted
   seconds-precision surface (live tracked-elapsed, countdown rows)
   has crossed a second boundary. No new `slint::Timer`, no
   UI-held timer state; kiosk keeps today's behaviour (timers section
   already desktop-only, `models.rs:482`).
6. *(Decided here)* **Column-resize model** (F3): the columns row
   keeps **total width == viewport** (no horizontal scroll — the
   status quo clips overflow; the user-cited Excel/Dolphin/Qt model
   redistributes a fixed total). Dividers redistribute space between
   exactly two neighbours; when Σ stored widths ≠ viewport (window
   resize, stack added/removed), the delta is distributed
   proportionally; if Σ content-minimums exceeds the viewport,
   columns clamp to minimums and the row clips (recorded fallback).

## 3. Design

Anchored facts cite `file:line` on current `main` (post round 3 +
timers/escalation/undo/habits rounds). Investigation sources: the four
round-4 exploration/research passes (summarised inline; no separate
report artefacts — the anchors below are the evidence).

### 3.1 Chunk 1 — board surface repairs (R4-F2, F6, F7, F8)

- **D1 (title truncation, F2).** The card title is
  `RoleText { overflow: elide }` with no width or `horizontal-stretch`
  of its own (`task-card.slint:166-171`), inside an inner
  `VerticalLayout` also without stretch (`task-card.slint:159-163`)
  beside a fixed 16 px done-checkbox (`task-card.slint:128-130`).
  Under the pinned Slint (1.18.1), an elided text's layout minimum is
  the width of a single "…" glyph (`i-slint-core-1.18.1/items/text.rs:686-688`),
  so layout squeezes land entirely on the title — the committed
  goldens show checkbox + bare "…" cards
  (`tests/goldens/board-0.5-1500.png`). Fix: the inner text column
  gets `horizontal-stretch: 1` and a token-based `min-width` (a
  readable-characters floor, e.g. ~12 chars at body size — stage 1
  picks the token), so the title fills the card to its right edge and
  only elides there. On the board, the delegate's explicit
  `width: 100%` (`board-layout.slint:531-532`) resolves against the
  Flickable content width — stage 1 verifies the card fills the
  column (the structural difference from the today delegate, which
  the compiler binds to the ListView width
  (`i-slint-compiler-1.18.1/passes/repeater_component.rs:95-100`)).
- **D2 (right-click closes menu, F6).** The dismissal layer listens
  to `clicked` only (`app-window.slint:476-481`); right-clicks are
  swallowed (the layer hit-tests first) and the menu stays open. Fix:
  a `pointer-event` arm on that TouchArea closing on
  `down && button == right` (the codebase's right-click idiom,
  `task-card.slint:104-110`). The same arm lands on the date-picker,
  palette, and capture dismissal layers (`app-window.slint:441-446,
  570-581`) for consistency.
- **D3 (background selects stack, F7).** The column body already
  right-clicks into the stack context menu (round-3 D4,
  `board-layout.slint:308-315`); the left-click seam is added the
  same way: a click on the body's empty areas routes
  `header-activated(column-index)` (the existing select seam,
  `board-layout.slint:363-364` → `select_column`,
  `adapter.rs:4035-4048`). Cards and their drag zones sit above the
  body TouchArea and keep their own clicks; the click also runs the
  R1 pointer-settle (it already does — `select_column` calls
  `settle_pointer_conflict`, `adapter.rs:4009-4012`).
- **D4 (single-click add task, F8).** Button and key dispatch the
  same action (`TaskCreate` → `UiEffect::BeginCreate`,
  `router.rs:488`), but the button prepends selection+settle
  (`adapter.rs:3953-3961`), `BeginCreate` arms only under
  `Flow::None` (`adapter.rs:994-1000`) with a settle-gated re-dispatch
  that is skipped when the settle changed nothing
  (`adapter.rs:1484-1493`), and the target resolves
  focused→selected→first (`view_state.rs:1072-1088`) so the field can
  open in a column the user is not looking at. Fix: (a) the ghost's
  click carries its column: `BeginCreate` gains the stack index (or a
  targeted variant) so `create_target_stack` is bypassed for pointer
  origin — the field opens in the clicked column, and the click also
  selects+focuses that stack (one click: select + open); (b) the
  pointer-originated `BeginCreate` under a foreign flow always
  settles-then-runs (the settle-nothing skip is dropped for this
  arm). Stage 1 records the exact effect-shape change (a `BeginCreate`
  carrying `Option<StackId>` vs a new effect) with the flow-table
  test updated for totality.

### 3.2 Chunk 2 — column-resize divider model (R4-F3)

- **D5 (explicit-width row).** Today every column is
  `horizontal-stretch: 1` with `min-width` from the stored width (or
  160 px floor) in one non-scrolling `HorizontalLayout`
  (`board-layout.slint:264-294`); the floor is the readability
  constant `STACK_WIDTH_MIN_PX = 160` logical px
  (`widths.rs:11-15`), content-independent — hence "minimum way
  larger than needed". Growing one column's `min-width` forces the
  stretch redistribution to shrink the *other* columns, so the
  dragged column's left edge moves left while its right edge follows
  the pointer — the "grows in both directions" effect. Redesign: the
  row renders explicit pixel widths that always sum to the row width;
  the stretch machinery dies on this row.
- **D6 (dividers).** A divider handle mounts at every column boundary
  (the existing header-edge handle `board-layout.slint:396-415` is
  re-anchored as the boundary between column *i* and *i+1*, with
  `ew-resize` cursor and hover affordance). Dragging moves the
  boundary: column *i* takes ±delta, column *i+1* takes ∓delta, each
  clamped by its content minimum; no other column changes. The
  press/drag/release callback chain (`board-layout.slint:129-131` →
  `board-view.slint:118-120` → `app-window.slint:271-273`) and the
  adapter's `width_drag` buffer (`adapter.rs:3239-3316`) are reworked
  to the two-column redistribution semantics in the pure module
  `src/widths.rs` (property-tested: redistribution preserves the sum;
  clamps at both minimums; no-op at exact minimum).
- **D7 (content minimums + slack).** `STACK_WIDTH_MIN_PX` is replaced
  by a content-derived minimum: the column header's preferred width
  plus card chrome (checkbox + paddings), floored by a small
  readability constant (stage 1 derives the formula and the floor;
  expect ~96–120 px, well under 160). When Σ widths ≠ row width
  (window resize, stack added/removed, mount seed), the delta
  distributes proportionally across columns (§2 decision 6); if Σ
  minimums exceeds the row width, columns clamp to minimums and the
  row clips (recorded fallback, status quo). Persistence rides the
  existing snapshot lane unchanged (`UiRequest::StackWidthsChanged`,
  `adapter.rs:3293-3316`; app debounce `ui_requests.rs:91-114`).
- **D8 (keyboard sibling).** The ±32 px keyboard resize
  (`widths.rs:22`, `view_state.rs:1160-1179`) now adjusts the focused
  column's boundary against its right neighbour (same redistribution,
  same clamps) — stage 1 records the edge case at the last column
  (left neighbour instead).

### 3.3 Chunk 3 — icon assets (R4-F1)

- **D9 (asset pipeline).** Vendor the chosen set (§2 decision 1:
  Lucide ISC) into `crates/taskboard-ui-slint/ui/assets/icons/`, one
  monochrome SVG per icon, plus `ui/assets/icons/LICENSE-lucide`.
  `slint-build` embeds them (`@image-url`); no build script, no Rust
  dependency, no codegen.
- **D10 (Icon component + migration).** A thin `Icon inherits Image`
  component (`width/height` from `Theme` icon metrics,
  `colorize: <tone color>`) replaces the per-kind geometry blocks in
  `icon-button.slint:104-309`; `IconKind` (`icon-button.slint:14-36`,
  13 kinds) remains the public vocabulary and maps enum→asset in one
  pure function. Every existing call site keeps its `IconKind` API;
  only the rendering changes. The tone→color map
  (`icon-button.slint:89-102`) is untouched — per-window light/dark
  keeps working because `colorize` re-evaluates with the theme token.
- **D11 (distinct notification icons, F1).** "Dismiss notification"
  and "Mark task done" both use `IconKind.check`
  (`notification-surface.slint:63-66, 70-75`). New kinds: dismiss =
  an X-in-circle (Lucide `circle-x`), done = `check` (stays); the
  vocabulary grows only by what the round's surfaces need (~5–10 new
  kinds total: dismiss, timer/alarm-clock, link/plug + link-off for
  the connections indicator (chunk 11), dropdown chevron-down,
  pencil for edit affordances) — the closed-vocabulary rule stands
  (growth = a recorded theme decision, as before, but now a vendored
  file + one enum row instead of hand-drawn geometry).
- **D12 (D19 amendment).** The ADR-level note in `icon-button.slint`
  and `docs/architecture.org` (where D19 lives) is amended: icons are
  vendored monochrome SVGs tinted via `Image.colorize`; no icon font
  (clause unchanged). Binary size and memory delta recorded in the
  chunk report (expected: tens of KB; the LRU caches decoded
  rasterizations).

### 3.4 Chunk 4 — hover machinery fixes (R4-F4, F5, F14)

- **D13 (empty description line, F5).** The tooltip panel
  always-mounts `description-text` (`visible: description != ""`) and
  `hint-row` (`visible: keys.length > 0`) inside its `VerticalLayout`
  (`tooltip.slint:152-168`); under the pinned runtime the hidden rows
  can reserve space (stage 1 records the probe). Fix: both rows become
  `if (condition)` conditional elements — invisible rows occupy no
  layout space by construction.
- **D14 (ghost tooltips on transitions, F14).** Tooltips clear on
  `unhover` and on a *named subset* of effects
  (`settle_flows_on_leaving` dismisses for
  NextView/PrevView/FocusBoard/FocusToday/BoardNext/BoardPrevious/BoardSelect
  — `adapter.rs:781-784, 877-892`; plus OpenSettings
  `adapter.rs:1066-1072`, OpenMenu `:1547`); the board→task-view
  transition is not in the set, and an anchor that unmounts while
  hovered never fires `unhover`. Fix, total-by-construction: a
  `dismiss_tooltip` call is added to **every** effect that changes the
  mounted main view (the enumerate-once list: open/close task, view
  switches, settings open/close, board select) — enforced by a
  structural test that walks the effect enum's view-swap arms and
  asserts each routes the dismiss (no default arm to hide behind).
- **D15 (hints missing at session start, F4).** Investigation item.
  Known facts: hints are synchronous projections of the keymap
  (`Keymap::first_binding_display(action, chain)`,
  `keymap/mod.rs:119-138`) and are chain-conditional — some are
  legitimately empty per preset/chain (e.g. the stack-create hint is
  documented empty under emacs, `models.rs:833-835`; an open menu
  masks the chain, `adapter.rs:1549-1551`; F-B7's fixed empty-hint
  bug is precedent, `menu.rs:210-213`). Stage 1 reproduces on a fresh
  boot (emacs preset, hover before any interaction) and discriminates
  the hypotheses: (a) chain masking at mount, (b) preset-unbound
  actions rendering keyless, (c) the keymap swapped by the settings
  reply after mount (config overrides — `resolve_ui_settings` runs
  app-side `ui.rs:386-392`, and the adapter may mount with a default
  keymap first), (d) tooltip hint-row visibility interacting with
  D13's layout. The fix lands per verdict; if the verdict is "empty
  by design under this preset", the hint row instead renders the
  action's menu-path hint or nothing — recorded decision either way.

### 3.5 Chunk 5 — task-view layout (R4-F9, F10, F11, F15, F16)

- **D16 (one header row, F9/F10/F11).** The back button's own
  ~40 px row (`task-view.slint:155-165`), the 12 px gaps, and the
  trailing `Rectangle { vertical-stretch: 1 }` spacer
  (`task-view.slint:398`) are the "two empty lines" effect. The view
  gains **one header row**: `[back IconButton][done checkbox][title
  (stretch)][Start/Stop tracking IconButton]` — back inline with the
  title (F9), tracking on the right (F11), the done/undone checkbox
  left of the title (F15) dispatching `SetTaskDone` exactly like the
  card checkbox (round-3 D6 precedent; state from the task model's
  done flag, strike/dim treatment consistent). The spacer dies; the
  **editable description field takes `vertical-stretch: 1`** (F10) —
  it is the element that eats leftover height. The metadata row
  (stack/due/scheduled/state) is unchanged except the state column
  losing the tracking button to the header.
- **D17 (tracked-time summary, F16).** A summary row (in the metadata
  area or under it — stage 1 places it) shows total tracked time on
  the task at second precision ("1h 23m 45s"), computed at refresh
  from the engine's tracking state (Σ `TimeEntry` durations incl. a
  running entry's elapsed; pure helper in `presentation.rs` beside
  `open_elapsed_phrase`, `presentation.rs:551-558` — which stays
  minute-bucketed for the status chip). While an entry is open the
  row live-ticks per second via the amended repaint condition (§2
  decision 5); otherwise it is refresh-static. This lifts the
  tracking-ui round's "no elapsed text" fence for the *task view*
  only (`task-view.slint:320-322` comment updated; the card badge
  stays fenced per #489).

### 3.6 Chunk 6 — task-view focus, navigation, insert engagement (R4-F12, F24, F26)

- **D18 (everything is a tab stop + focus visual, F12).** The closed
  title (`task-view.slint:181-205`), closed due/scheduled cells
  (`:235-256, :275-294`), closed description (`:363-384`), rule
  preset chips (`rule-list.slint:81-105`), and every checkbox gain a
  `FocusScope` with `focus-on-tab-navigation: true` and the D15 focus
  ring — the title now visibly indicates the active element (the
  today-board precedent: `today-view.slint:44-80`). The existing
  control walk (`nav.next-control/prev-control` → Tab re-injection,
  `adapter.rs:1254-1282`) then reaches every element in every preset
  where Tab cycles; standard/vi keep Tab/Shift+Tab
  (`presets.rs:184-205, 533-554`).
- **D19 (emacs four-key walk, F12).** Emacs binds C-f/C-b to the
  walk in TaskDetail (`presets.rs:395-415`); C-n/C-p join as aliases
  of next/prev-control in the non-Board contexts where they are
  conflict-contract-clean (on the Board they remain task/stack nav —
  unchanged). Snapshots regenerate; the conflict/shadow contracts
  must stay green.
- **D20 (focus-driven insert engagement — the systemic fix, F24).**
  Root cause of the Enter confusion: the reminder field never engages
  insert mode (RuleList forwards no `edited` seam,
  `rule-list.slint:106-118`; only the timers form does,
  `timers.slint:33-37`), so while typing, the chain stays
  `[TaskDetail, ViNormal, Global]` and emacs Return resolves
  `task.title-edit` (`presets.rs:328`) → arms title editing instead
  of committing the field. Fix systemically in the shared widget:
  `text-field.slint`'s inner input publishes `changed has-focus` into
  a global editing-facts singleton, and the shell chain masks the
  view context whenever **any** text field holds focus
  (`shell.rs:66-99` — the insert-mode chain `[ViInsert, Global]` +
  non-masking heads), so field-Enter always reaches the field, in
  every view, with zero per-mount wiring. Per-mount `edited`/engage
  seams (timers form) are replaced by or reconciled with the global
  rule (stage 1 records which die); the palette input is the named
  exception (it owns its chain already). This makes the F24 class —
  a field that forgot its wiring — architecturally impossible: the
  field cannot forget, the widget does it. The rule is pinned by a
  journey: focus any text field in any view → preset-bound Return
  never hijacks.
- **D21 (hover coverage, F26).** Every interactive element in the
  task view arms the tooltip (label + description + hint keys where
  bound): closed title/dates/description, preset chips, custom
  offset field, tracking start/stop, done checkbox, the new summary
  row (explains what the total covers). This is also a blanket
  acceptance rule for chunks 5–11: every new interactive element
  ships hover + walk-stop + action-layer dispatch.

### 3.7 Chunk 7 — date/time entry (R4-F18, F19, F20, F21, F22, F27)

- **D22 (flip fix, F18).** The window-level picker overlay flips
  above/below by a live `self.height` test (`app-window.slint:454-460`),
  and the panel height follows the grid's week-row count
  (`date-picker.slint:63`; the grid is 4/5/6 rows per month,
  `due_edit.rs:312-326`) — month paging changes the height, the test
  re-evaluates, the panel flips. Fix (primary): the grid reserves
  **six week-rows always** (constant panel height while open — the
  common calendar convention); fallback (recorded): freeze the
  above/below placement decision at open. Stage 1 probes which reads
  better at min viewport.
- **D23 (steppers from empty, F19).** `step_time` returns early when
  `self.time` is `None` (`due_edit.rs:247-250`), and the time is
  seeded only from a non-midnight stored anchor at open
  (`due_edit.rs:197-206`) — typed text never seeds it
  (`adapter.rs:4385-4398`). Fix: stepping an unset time seeds it
  (stage 1 records the seed — recommendation: 00:00, predictable and
  timezone-free; "now" recorded as the alternative) and then steps;
  the existing pin
  `date_only_picker_stays_midnight_until_a_time_exists`
  (`due_edit.rs:611-620`) is reconciled: display stays `--:--` until
  a time *exists*, and the first stepper click is exactly what makes
  one exist. Typing `HH:MM` also seeds the picker state live.
- **D24 (click-away closes the interaction, F20).** The DueEditor
  forwards no `focus-lost` seam (`due-editor.slint:43-60`; the R1/L1
  focus-loss settle never covered the date fields), and an
  `EditDate` settle discards rather than commits
  (`view_state.rs:1639`) — so junk text survives click-away. Fix:
  the editor gains the focus-loss seam wired to the round-3 R1 rule
  — non-empty *and parseable* commits, empty or non-conforming
  discards (restores the prior value, clears the field); clicking
  the card background both settles the editor and drops the picker
  overlay. The typed-grammar parse (`due_edit.rs:92-138`) gains a
  per-keystroke `FieldValidity` flavour (F27, below) so
  non-conforming text is visible *before* any exit.
- **D25 (two-phase emacs navigation + live sync, F21).** Today the
  `DatePicker` keymap context rides the chain only while the *grid*
  holds focus (`adapter.rs:4346-4359`; `shell.rs:76-98`), the
  default-open state focus starts on the field
  (`adapter.rs:4511-4515`), and grid cursor moves never write back
  to `due_buffer` (`adapter.rs:4523-4549` vs `:4508`). Redesign as an
  explicit two-phase flow inside `Flow::EditDate`: **picker phase**
  (grid holds focus; C-p/C-n/C-f/C-b move the cursor via the
  existing `picker.*` rows, `presets.rs:433-449`; month follows the
  cursor) → Enter hands focus to the field → **text phase** (typing;
  D20's field-focus masking applies, so C-f/C-b are caret movement;
  typed text seeds the picker state per D23) → Enter commits
  (`commit_date`, `adapter.rs:4585-4614`), Escape steps back a
  phase / cancels. In *both* phases the field text mirrors the
  picker selection (cursor moves write `due_buffer`; buffer parses
  write picker state — one pure bidirectional sync helper in
  `due_edit.rs`, property-tested for no-oscillation). This is
  exactly the user's described flow: navigate to due, Enter, pick a
  day, type a time, Enter.
- **D26 (time combobox, F22 — §2 decision 3).** New shared widget
  `ui/widgets/time-field.slint`: typable HH:MM with lenient parse
  (`9`, `930`, `9:45` → 09:45), embedded ± steppers (±15 min,
  midnight rollover — `step_time`'s wrapping math,
  `due_edit.rs:247-263`, generalised), and a dropdown quick-list of
  quarter-hour slots (anchored flyout like the date picker's,
  scrolled to the nearest value, filter-by-typing, Enter commits,
  Escape closes, full walk-stop coverage). 24-hour internally. The
  date picker's stepper row (`date-picker.slint:124-144`) is replaced
  by it; chunks 9 and 10 consume the same widget.
- **D27 (offset syntax validation, F27).** `ReminderOffset::parse_text_form`
  (`taskboard-domain/src/reminders.rs:108-135`) is pure, total, and
  already shared by three grammars; it becomes the validator for the
  custom-reminder offset field (`rule-list.slint`), the due editor's
  relative form, and the today-view timer form fields — per-keystroke
  tri-state `FieldValidity` (round-3 D19 machinery:
  `text-field.slint:21-25, 95-102`, error line below the field),
  exactly as `settings_form.rs` maps `UiScale` (`:209-235`). Copy is
  presentation constants; tests pin validity state, never text.
- **D28 (zone unification).** The due grammar commits UTC wall-clock
  (`due_edit.rs:12-30, 105, 111` — "UTC is the grammar's zone") while
  timers parse local (`adapter.rs:4745-4746`, `presentation.rs:800-826`)
  — for a UTC+1/+2 user every due-anchored reminder fires hours late
  (a ranked root cause of F25). The due grammar parses and echoes
  **local** time and converts at the seam; storage stays UTC
  instants. Journeys/goldens updated; DST edges ride the existing
  time-DST test seams.

### 3.8 Chunk 8 — task reminders that fire (R4-F25, F23-part)

- **D29 (at/in reminder kind — §2 decision 2).** Domain: reminder
  rules become enum-with-data — `DueOffset(ReminderOffset)` (today's
  shape, `reminders.rs:22-56`) and **`At(DateTime<Utc>)`** (a minted
  instant newtype per the house A1 pattern). Commands: the rule-add
  verb carries the kind (`AddReminderRule` grows the variant; the UI
  grammar "in 10m"/"at 17:00" mints `At`, bare "10m" stays
  `DueOffset` — parity with `parse_timer_when`'s `in`/`at` prefixes,
  `presentation.rs:788-830`). Evaluator: the firing pass
  (`due_firings`, `reminders.rs:792-845`) gains the `At` arm — fire
  at `instant <= now && !delivered && !dismissed`, *independent of
  `is_remindable`'s duedate requirement* (`reminders.rs:947-949`) —
  riding the same 30 s engine tick, delivery, and in-app surface
  lanes (`actor.rs:1145-1173`). Settlement: fired+dismissed `At`
  rules prune (the adhoc precedent `prune_settled_alarms`,
  `reminders.rs:678-693`), while `DueOffset` rules persist by design
  (they re-arm with due changes). Storage: migration **0018** (the
  next free number per the backlog dispatch's fence) carries the
  kind column. Fences: the backlog round-2 chunks 3.5
  (respite-break reminders) and 1.9/4.2 (tracking arms) touch
  adjacent domain files — rebase-dance friction only, no shared
  semantics.
- **D30 (armed-state rows + countdown, F23/F25-part).** Rule rows
  render per kind: `At` rows show the live countdown
  ("in 0d 2h 3m 41s") **and** the fixed fire time
  ("2026-11-02 09:30:00", local, ISO-ordered — the user's F23 spec,
  colon-separated per ISO 8601), ticking per second via §2 decision
  5; `DueOffset` rows show the authored offset plus the effective
  fire instant when armed, or an explicit **"needs a due date — not
  armed"** state (with a jump-to-due-field affordance) when the task
  has no due date — the F25 root cause made visible. Format helpers
  are pure functions in `presentation.rs`; tests pin structure
  (segment counts, round-trips), never label text.
- **D31 (repair items from the investigation).** (a) GUI mute is
  one-way (`router.rs:384, :803`; every GUI path mutes, none unmutes)
  — the task view's reminder section gains an un-mute affordance and
  a muted indicator (a ranked cause of "never fires"). (b) The
  engine tick, delivery, and settlement lanes are probe-verified
  healthy on main (round-3 chunk 7, 2026-10-28); the chunk pins the
  end-to-end journey anyway — an `At` reminder fires at T (paused
  clock), lands as an in-app row, and settles on dismiss — so the
  class has a regression harness. (c) `Insistence` stays
  `Normal`-hardcoded in the GUI (`router.rs:401, :417`) — recorded as
  backlog (the escalation ladder stays CLI-reachable); not this
  round's scope.

### 3.9 Chunk 9 — timer surfaces (R4-F13, F23; absorbs backlog 2.5/#352)

- **D32 (countdown + ISO, F23).** Timer rows (`timers.slint:76-93`)
  and the status-bar chip (`models.rs:1275-1289`) upgrade from the
  minute-bucketed phrase (`countdown_phrase`,
  `presentation.rs:472-479`) to the two-part display: d/h/m/s
  remaining + "@ YYYY-MM-DD HH:MM:SS" fire time, live per second (§2
  decision 5; kiosk keeps today's gates). The pure formatter is
  shared with D30.
- **D33 (mouse setter widget, F13).** The timers section header
  (`timers.slint:54-74` — the creation affordance's designated
  "permanent home") gains an IconButton opening an anchored popover:
  quick presets (1m/5m/10m/25m/1h), a label field, and a custom time
  composed from D26's time combobox (+ day offset for "tomorrow
  09:30" — stage 1 settles the minimal grammar against
  `parse_timer_when`). Commit dispatches the existing
  `TimerCreate`/`CreateAdhocAlarm` lane (`adapter.rs:4727-4757`); the
  inline text form stays for keyboard-only users (both affordances
  share the commit seam).
- **D34 (#352 remainder).** Stage 1 enumerates the DR-3 rulings
  against what landed (escalation + alarm window landed — commits
  `112bb6b5`, `3d7feb7b`; the active list, creation, snooze/cancel
  exist) and completes only what is genuinely missing; nothing
  double-lands. Issue #352 closes via this chunk (bookkeeping §8).

### 3.10 Chunk 10 — tracking surfaces (R4-F17; absorbs backlog 3.4/#490+#491)

- **D35 (entries list, F17).** A button beside D17's summary row
  opens an anchored overlay panel (the menu-overlay pattern,
  `app-window.slint:348-369`) listing the task's `TimeEntry` rows —
  date, start, end, spent per entry, the running entry marked — with
  the **full total at the top** (seconds precision, D17's helper).
  Data: the engine already carries the entries
  (`tracking.rs:36-66`); the UI needs a push section + an `IdTables`
  entry-id table (models-side only).
- **D36 (add + edit + delete, F17).** The panel's affordances
  dispatch the **existing** verbs — add: a form (date picker + two
  D26 time fields) → `LogTime { task, start, end }`
  (`messages.rs:321-333`); edit: row-activate opens the same form
  seeded → `AdjustTimeEntry { id, start, end }` (`:334-348`);
  delete → `DeleteTimeEntry` (`:349+`). Validation surfaces the
  planner's typed rejection (`ClosedSpan::try_new`,
  `tracking.rs:77-108`) as field validity (end > start), never as
  swallowed silence. Full input-mode support: every element a walk
  stop, Enter activates/commits, Escape backs out, hover tips
  throughout (D21 blanket rule).
- **D37 (#490 rider).** The today view gains the "Tracked" totals
  section per the issue's own scope (stage 1 reads #490 and follows
  it; `tracked_in_window`, `tracking.rs:141-166`, is the aggregator).
  If #490's scope proves larger than a section, the chunk lands the
  section and files the remainder — recorded, not silently dropped.

### 3.11 Chunk 11 — settings: scale + connections (R4-F28, F29, #411)

- **D38 (scrap instant preview, F28).** The live-apply arm dies:
  `settings_scale_preview` (`adapter.rs:2813-2833`) and its markup
  hook (`settings-view.slint:713-715`) are removed; `Theme.base-scale`
  moves only in `apply_scale_commit`/revert
  (`adapter.rs:2738-2766, 2863-2878`). During a drag the thumb
  follows the pointer (widget-internal), the app re-layouts once on
  release, and the field echo no longer tracks the thumb
  mid-gesture (the `:2828-2832` logic goes with the arm). The
  commit-path scroll anchoring (`:1984-1988`) stays.
- **D39 (connections dropdown, F29).** The per-board button list
  (`settings-view.slint:447-493`) is replaced by a hand-built
  dropdown widget (`ui/widgets/dropdown.slint`: field + chevron +
  anchored list; keyboard: Enter opens, C-n/C-p move, Enter selects,
  Escape closes; hover feedback on rows — fixing the missing hover
  feedback; walk stops throughout). The label "Select Board to
  Connect/Disconnect:" sits left of the dropdown when the responsive
  rule allows, above it otherwise (`responsive.slint`). Each option
  carries a connected/not-connected indicator (D11's link/link-off
  icons + status tone from `connector_row_status`,
  `adapter.rs:2383-2399`). Selection drives the existing
  `settings.selected_board` seam (`adapter.rs:2621-2627`) — which is
  Bind-only today (it does not and must not change the viewed board;
  the copy now says so). The board list refreshes on settings mount
  (today it is reply-stale, `adapter.rs:2105`). Connector rows gain
  hover feedback.
- **D40 (#411 adoption).** The board bar adopts the same dropdown as
  its board switcher per #411's ruling (many-boards case; stage 1
  reads the issue and follows — if its scope exceeds the widget +
  adoption, the widget lands and the remainder is filed/annotated).

## 4. Testing strategy

Round-2/3 rules carry (mock clock / pause+advance, typed outcomes, no
text assertions, one-behaviour-one-test at the lowest layer,
deterministic-by-construction). Round-4 additions:

- **Layout/geometry**: goldens for card-title fill (board + today at
  the fixture widths — the existing `board-0.5-1500.png` class
  regenerates with full titles), task-view header row (one row,
  description stretching), picker constant height across month pages.
- **Column resizing**: property tests in `widths.rs` (sum-preserving
  redistribution, clamps at both minimums, proportional slack
  distribution); journeys for drag press/move/release and the
  keyboard sibling; persistence round-trip (existing snapshot lane).
- **Focus/insert**: the D20 journey set — focus any field → preset
  Return never hijacks (all three presets); the walk reaches every
  declared stop in the task view (structural stop-inventory test per
  view); the two-phase picker flow (picker phase keys, text phase
  caret, sync no-oscillation property).
- **Reminders**: `At` firing journey on the paused clock (fire →
  in-app row → dismiss → pruned); `DueOffset` armed/not-armed
  presentation states; zone unification journeys (local↔UTC round
  trip, DST edges via the clock seam); un-mute round trip.
- **Timers/tracking**: per-second tick seam (dirty only when a
  seconds surface is mounted — kiosk pins unchanged); timer create
  via popover and via text form hit the same seam;
  `LogTime`/`AdjustTimeEntry`/`DeleteTimeEntry` journeys incl. the
  typed end≤start rejection surfaced as validity.
- **Icons**: structural (IconKind→asset mapping total; no unmapped
  kind), golden spot-checks of the new notification icons; no
  geometry-block residue (grep for the old `if (root.icon == …)`
  rectangles).
- **Investigation items** (D15 hints, D22 flip probe, D34 remainder):
  the probe method + verdict recorded in the chunk's stage-1 plan;
  the fix carries its own test per verdict.

## 5. Code inventory

`ui-slint/ui/`: `task-card.slint`, `board-layout.slint`,
`app-window.slint` (dismissal layers, overlays), `tooltip.slint`,
`icon-button.slint` + new `assets/icons/`, `time-field.slint`,
`dropdown.slint`, `date-picker.slint`, `due-editor.slint`,
`task-view.slint`, `rule-list.slint`, `timers.slint`,
`today-view.slint`, `settings-view.slint`, `toolbar/board-bar`;
`ui-slint/src/`: `adapter.rs`, `view_state.rs`, `widths.rs`,
`due_edit.rs`, `models.rs`, `presentation.rs`, `shell.rs`,
`actions.rs`, `keymap/presets.rs` + snapshots, `lib.rs` (repaint
driver), `tooltip.rs`; `taskboard-domain/src/reminders.rs`,
`messages.rs`, `command.rs` (chunk 8 only);
`taskboard-storage-sqlite` migration 0018 (chunk 8 only);
`taskboard-state` evaluator wiring (chunk 8 only). `taskboard-app`:
expected empty.

## 6. Slicing (11 chunks; dispatch plan is normative)

| Chunk | Design | Branch | Theme | Size |
|---|---|---|---|---|
| 1 | §3.1 (D1–D4) | `fix/ui-r4-board-surface` | title truncation, right-click menu close, background select, single-click add-task | M |
| 2 | §3.2 (D5–D8) | `feat/ui-r4-column-resize` | divider redistribution, content minimums, slack | L |
| 3 | §3.3 (D9–D12) | `feat/ui-r4-icon-assets` | SVG icon system + distinct notification icons | M |
| 4 | §3.4 (D13–D15) | `fix/ui-r4-hover-fixes` | empty line, ghost tooltips, hints-at-start investigation | M |
| 5 | §3.5 (D16–D17) | `fix/ui-r4-taskview-layout` | one header row, checkbox, description stretch, tracked summary + tick seam | L |
| 6 | §3.6 (D18–D21) | `fix/ui-r4-taskview-focus` | tab stops + focus visuals, emacs four-key walk, focus-driven insert engagement, hover sweep | L |
| 7 | §3.7 (D22–D28) | `feat/ui-r4-datetime-entry` | flip fix, empty steppers, click-away, two-phase emacs picker, time combobox, validation, zone unification | L |
| 8 | §3.8 (D29–D31) | `feat/ui-r4-task-reminders` | at/in reminder kind + migration 0018, armed rows + countdown/ISO, un-mute | L |
| 9 | §3.9 (D32–D34) | `feat/ui-r4-timer-surfaces` | countdown + ISO, mouse setter popover, #352 remainder | M |
| 10 | §3.10 (D35–D37) | `feat/ui-r4-tracking-surfaces` | entries list + add/edit/delete, #490 today totals | L |
| 11 | §3.11 (D38–D40) | `feat/ui-r4-settings-connections` | scrap scale preview, connections dropdown, #411 adoption | M |

Serial default 1 → 11. Gates: 2 on 1 (board files); 6 and 7 on 5
(task-view files); 8 on 6 (insert engagement); 9 and 10 on 7 (time
combobox) and on 3 (icons); 11 on 3 (icons). Independent pairs (may
run as parallel lanes on the user's direction, worktree budget
permitting): 3 ∥ 4; 11 ∥ 5–8; 9 ∥ 10 after their gates. Chunks 1–5
carry no §2 decision risk except 3 (decision 1); chunks 8 (decision
2), 9/10 (decision 4), and 7's D26 (decision 3) are gated on the
user's confirmation of those decisions at plan review.

## 7. Risks

- **R1 The divider redesign replaces the column sizing model**
  (stretch → explicit widths): the widths property suite and the
  persistence round-trip are the harness; goldens pin the visual;
  the 160 px floor's removal must not break the resize-handle
  gesture (probe recorded in stage 1).
- **R2 `Image.colorize` + SVG is runtime behaviour** — stage 1 of
  chunk 3 lands a probe first (one asset, one button, light/dark,
  both renderers used by CI) before the migration; the fallback is
  Tabler assets or the `lucide-slint` crate (recorded in §2
  decision 1).
- **R3 Focus-driven insert engagement changes key resolution globally**
  (D20): every preset's conflict/shadow contracts and snapshots are
  the gate; the palette exception is enumerated; a regression in any
  existing field journey is a finding, not a reweakened test.
- **R4 The zone unification (D28) changes committed instants** for
  existing date-only dues (00:00 UTC → local midnight): journeys and
  goldens regenerate deliberately; the migration question (rewrite
  stored instants?) is answered No — stored UTC instants are
  correct, only the grammar/echo zone changes — recorded in stage 1.
- **R5 Chunk 8 is the only crate-crossing chunk** (domain/state/
  storage migration 0018): the backlog dispatch's migration fence
  (only one migration-carrying chunk in flight; next free number)
  binds; rebase friction with backlog 3.5/1.9/4.2 expected.
- **R6 Absorbing backlog 2.5/3.4 (§2 decision 4) leaves the backlog
  plan stale** until the amendment commit lands — the dispatch plan's
  bookkeeping owns it; if the user redirects to the gated-deferral
  alternative, chunks 9/10 re-scope to deltas and the gates flip to
  the backlog merges.
- **R7 Per-second ticking on kiosk-class hardware**: the condition is
  mounted-surface-gated (kiosk pins today's minute-only behaviour);
  stage 1 of chunk 5 records the measured repaint cost at 1 Hz on
  the netbook-class reference if available; fallback = tick only
  while a seconds surface is visible (which is the design already).

## 8. Backlog interactions & bookkeeping

- **Closes on merge** (stage 3 posts ✅ DONE + PR link): #352
  (chunk 9, D34), #490 + #491 (chunk 10, D35–D37), #411 (chunk 11,
  D40 — if its full scope lands; else annotated).
- **Amends**: the backlog round-2 dispatch plan loses chunks 2.5/3.4
  (a docs commit after the user confirms §2 decision 4; pointer to
  this round's chunks 9/10).
- **Stays open, untouched**: #210/#489 (card play/pause — the
  tracking badge fence holds), #444, #463/#464 (perf lane), #412
  (window geometry — chunk 11 must not collide; stage 1 checks
  whether the backlog 2.4 PR landed first), all other backlog
  round-2 chunks.
- **New issues to file**: GUI insistence picker (D31c) as `backlog:`
  ui/p2; anything D15/D22/D34 investigations surface.
- **What opens next**: GUI review round 5 over this round's merges;
  the DR rounds' remaining surfaces per the roadmap.
