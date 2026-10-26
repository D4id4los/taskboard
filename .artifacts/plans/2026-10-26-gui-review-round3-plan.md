# [Plan] GUI Review Round 3 — Fix Round (UI state machine, board surface, task view & dates, settings polish, legacy migration, navigation & menu, reminder delivery)

- **Date**: 2026-10-26
- **Status**: PROPOSED — written from the user's 2026-10-26 GUI review
  round 3 (41 findings, R3-F1…R3-F41 below) plus the user's explicit
  work item: **analyse the UI state machine and re-architect state
  transitions so invalid states are impossible or very unlikely**
  (work item W-SM, §3). Companion dispatch plan:
  `.artifacts/plans/2026-10-26-gui-review-round3-implementation-dispatch-plan.md`
  — **7 PRs**, serial default.
- **Scope guard**: `taskboard-ui-slint` + `taskboard-app` (+ config
  migration in `taskboard-app/src/config.rs`). `taskboard-domain`
  expected empty (start date = existing `Task.scheduled`, time
  component = existing `DateTime<Utc>` instants, default-Kanban =
  existing `CreateStack` commands) — stage-1 asserts per chunk.
  **Coordination fences** (user-directed 2026-10-26): DR-3
  reminders-surface-pair, DR-5 time tracking, DR-6 recurring, DR-2
  signal, DR-4 sync-merge, and undo v1 are separate agents' rounds —
  this round does not enter their territories (§2).
- **Round-2 relationship**: round 2's chunks landed; several round-3
  findings are round-2 completions/regressions (ghost sizing R2-D38,
  picker behaviour R2-D25, slider R2-D29, status line R2-D28,
  InfoTip/R2-D31, tooltip width R2-D14).

## 1. The user's round-3 findings (numbered for reference)

1. Checkboxes on task cards to mark done/undone (time-tracking
   play/pause button explicitly deferred with #210 → DR-5 #489).
2. Fix the deprecation warnings in Slint code.
3. Password field: toggle (not hold-to-show) plaintext eye button.
4. Drag&drop ghost is whole-window sized; must be the card's size.
5. Plus icons on New Stack / New Task too large (~halve).
6. Right-click on a stack's empty card space should open the stack
   context menu.
7. Creating a task, typing nothing, clicking out → app stuck (cannot
   create/move tasks) until re-enter + Enter.
8. Drag&drop between stacks can leave a mode needing Escape before
   further interaction, though the move itself applied.
9. New Stack / New Task buttons inconsistent: sometimes nothing,
   sometimes flash-then-nothing; occasionally stuck until Escape.
10. Empty board (no stacks): New Stack button never works; menu's
    "New Stack" entry missing; one click-combo made it appear but
    clicking still did nothing.
11. Clicking a newly created empty board's tab lands in a view
    without board tabs (only board title + hamburger).
12. New Task buttons lack hover text.
13. Due-date field in edit-task view renders as static, unclickable
    text; needs the interactive visual grammar + hover.
14. Start Date missing from the task edit view.
15. Date fields need an optional time component.
16. Picker should open by default when due-date editing is entered;
    render as overlay (like the main menu), not push content down.
17. Clicking the due field deletes the set value; want the value kept
    with a small "x" clear button inside the field (same system as
    the round-2 checkmark; maybe reset buttons on saved settings
    fields too).
18. While due-date editing is open, switching to Settings silently
    fails — editing modes must either allow or cancel-then-allow
    actions, never fail silently.
19. Info-"i" hover windows sized from the short title ("Name") — far
    too narrow.
20. Settings status display must sit outside the scroll area (user
    thought Save was dead because the status was scrolled away).
21. Fields with validity rules need instant feedback: active border
    red when invalid, green when valid, plus an error line below the
    field (hover-style box).
22. Legacy single-server config: automatic migration to the
    multi-server form in the background, not a typed rejection.
23. Scale: text field and slider must stay in sync both directions.
24. Slider drag breaks after one tick (instant preview refresh kills
    the gesture).
25. During scale change, keep the scale setting visible (anchor
    scroll); downward changes currently leave an empty-looking
    screen.
26. Task edit view auto-starts title editing; after closing once the
    title can never be edited again; title box too narrow. Want
    on-demand editing (pen button / double-click), always re-armable,
    full-width field.
27. Emacs-style C-n/C-p/C-f/C-b navigation on all views (arrows for
    vertical/horizontal neighbours where layout-aware, else tab
    order when not editing a text box), including the date chooser
    (which currently hardbinds arrows even in emacs mode).
28. Date picker: displayed month must follow the arrow-key cursor;
    month prev/next buttons must be keyboard-operable with
    preset-adapted bindings.
29. Reminder list rows show too-detailed target/created times
    (rfc3339); humanize, at most to the second.
30. Manually set reminders never show in-app, and system notifications
    never reach KDE (Wayland); overdue auto-rows do show in-app. This
    is the OS-delivery/visual plumbing the DR-3 plan explicitly
    queues for **this round**.
31. Board → today view shrinks the window (only when the today view
    has content).
32. Scroll areas need keybindings: C-v / M-v (emacs); standard
    probably PageUp/PageDown; vi equivalent to be looked up.
33. The always-visible hamburger shortcut chip should become hover
    text like every other element.
34. Search input should be a normal-height box with results below,
    not a full-screen field.
35. Search result activation must actually navigate (select board +
    stack, show the task); propose a reusable "ShowTask"-style
    action.
36. Emacs convention note: double `C-g C-g` where a single C-g is
    ambiguous.
37. Needs a mouse affordance to switch today ↔ board; asks for
    **options** (§3.9 answers with three options + recommendation).
38. Submenu items need priority sorting (go-to-board/today above
    next/prev view in the view submenu).
39. Menu entries irrelevant to the active preset (vi mode switches)
    should not be shown at all.
40. Menu entries inapplicable in context (Cancel Move with no move)
    should be greyed, not silently dead.
41. Empty-board page: centered labelled "New Stack" button below the
    hint text, plus "Create Default Kanban Setup" producing
    Backlog/To Do/In Progress/Done; keep the preset mechanism
    extensible.

## 2. Fold-in recommendation (open issues; DR-covered ones excluded)

**Recommend folding in** (user to confirm; assumed in the chunking):

| Issue | Why now | Lands in |
|---|---|---|
| #230 configurable due-soon horizon | one presentation+config knob on the date surfaces this round already reworks | chunk 3 |
| #231 preset bindings for `reminder.snooze`/`reminder.mute` | pure registry/preset rows (verbs exist; DR-3's non-goal is snooze *duration* presets, not keybindings) — clears recorded debt at `actions.rs` | chunk 6 |
| #228 description editor widget | the task view is already open for surgery (title/dates); a multi-line editable description with the round-2 TextField commit pattern is self-contained — stage-1 verifies `TaskChanges` carries description; if the domain lacks the field, STOP and defer | chunk 3 (drop-on-verify) |
| task-card checkbox half of #210/#489 | pure UI (`SetTaskDone` toggle, `task.reopen` exists since round 2); the tracking half stays with DR-5 | chunk 2 |
| Slint deprecation warnings (#2 above) | rider on the chunk touching the most Slint files; acceptance = warning-free Slint build | chunk 2 |

**Not folded, with homes**: #474 settlement re-time (DR-3 chunk 1 —
the in-app half of R3-F30 is *caused* by it; §8 records the fence),
#444 native toast buttons (gated on DR-3 escalation + platform
tests), #210/#489 tracking UI (DR-5), #412 window-geometry
persistence (still deferred — R3-F31 is within-session D15
regression work, not persistence), #411 board dropdown (deferred),
#463/#464 perf follow-ups (perf track), #351 free-form toml editor
(deferred — but its *migration* half folds in as chunk 5), #461
already landed (ListView).

## 3. Design

Anchored facts are cited inline as `file:line` (current `main`,
post-round-2). Flow-machine facts: `Flow` enum
`view_state.rs:58-101`; interpret table `adapter.rs:589-783`;
`settle_flows_on_leaving` `adapter.rs:532-584` (leaving-effect-only);
commit/discard `view_state.rs:991-1072`.

### 3.1 W-SM — the state-machine work item (R3-F7/8/9/10/18/26; the user's explicit ask)

- **W-SM-a (audit, analysis artefact).** Chunk 1 stage 1 produces
  `.artifacts/reports/2026-10-26-ui-flow-state-audit-report.md`: the
  full Flow × effect table, every start/end/discard path, the
  pointer-path holes (settle only runs on leaving *effects*, never
  pointer clicks), and the six breakage mechanisms:
  click-out with empty buffer leaves `Flow::Create`+Insert so the
  `[ViInsert, Global]` chain masks view contexts (TextField exposes
  only `accepted`/`edited`, `text-field.slint:87`);
  stale-column drop returns without settle (`adapter.rs:2398-2407`,
  `drag_finished` early-out `:2430-2432`);
  `BeginCreate*` matched only under `Flow::None` (`adapter.rs:686-692`)
  so a lingering flow makes buttons dead (`(_, _) => Vec::new()`
  `:770`); empty-board create flow opens *invisibly* (fields live in
  `stacks.length > 0` branches, `board-layout.slint:538,616`);
  `(Flow::EditDue, _)` swallows all effects before the `OpenSettings`
  arm (`adapter.rs:677-683` vs `:756`); title re-arm needs
  `ModeInsert` which only vi binds (`presets.rs:339-340`) and the
  closed title has no pointer seam (`task-view.slint:112-117`).
- **W-SM-b (rules).** The redesign records five rules, then enforces
  them in code: **R1 focus-loss settles** — any pointer click outside
  an active text flow commits (non-empty) or discards (empty) it;
  the TextField gains a focus-loss seam. **R2 gestures always
  settle** — `drag_finished`/drop-miss cancels cleanly; no early-out
  can leave `Flow::DragMove` armed. **R3 flows yield** — an action
  that doesn't conflict with the active flow either commits-then-
  runs (view switches, settings open, palette) or is explicitly
  queued; the swallow-all arms die; each non-yielding combination is
  named in the audit report. **R4 dead buttons cannot exist** — a
  `Begin*` effect under a foreign flow surfaces (status feedback) or
  auto-settles per R3; the empty-board create flow gets a visible
  field (with R5). **R5 flows are visible** — an active flow always
  has a rendered affordance (field, highlight, or status hint); the
  audit lists every flow's affordance.
- **W-SM-c (modelling).** Keep the enum-with-data `Flow` (house A1
  style; ids are minted newtypes) but make invalid combinations
  unrepresentable where cheap: the interpret table becomes
  total-by-construction over `(Flow, effect)` with explicit
  yield/settle arms (no `(_, _) => Vec::new()` default), and the
  adapter's stray one-shot buffers (`due_buffer`, `width_drag`,
  `scale_drag`) move under flow/mode invariants so they cannot
  outlive their flow. Typestate is deliberately not introduced
  (A2 stays out per AGENTS §2.1 unless the audit shows a shape the
  enum cannot express).
- **W-SM-d (title editing, R3-F26).** Task-detail entry never
  auto-arms EditTitle; the displayed title gains the interactive
  grammar (hover + click-to-edit, and the task context menu keeps
  Edit Task); re-arming is always possible (mouse seam + a bound
  edit action in all presets). Field uses the row's full width.

### 3.2 Chunk 2 design — board surface & empty board (R3-F1-half, 2, 4, 5, 6, 9-residue, 10, 11, 12, 41)

- **D1 (ghost).** `DragState` gains the dragged card's logical size
  (captured at drag-arm, `adapter.rs:2336`); the ghost TaskCard binds
  width/height to it (`app-window.slint:482-491` currently unbound →
  fills the window).
- **D2 (glyphs).** `PlusGlyph` shrinks to the IconButton visual
  (16 px-class inside `space-6` chrome) or the buttons adopt
  IconButton outright — one decision, applied to New Stack and both
  New Task ghosts.
- **D3 (tooltips).** Add Task ghosts adopt the generalized tooltip
  trigger (New Stack's wiring is the template,
  `board-layout.slint:505-515`; ghosts lack it, `:470-472, 684-686`).
- **D4 (empty-space context menu).** The column body gets a
  right-click seam routing the stack context menu with the column's
  index (headers already have it, `board-layout.slint:264-277`).
- **D5 (empty board).** The empty branch gains: the board bar /
  tabs rendered even with zero stacks (fixes R3-F11 — today the bar
  lives only in the `stacks.length > 0` leg, `:160-181`), a centered
  labelled "New Stack" button **with a working create field** (the
  invisible-flow bug), and **"Create Default Kanban Setup"**: an
  extensible board-preset table (name → ordered stack titles; exactly
  one preset "default-kanban" = Backlog/To Do/In Progress/Done)
  dispatching the existing `CreateStack` per entry — no domain
  change; future presets are table rows.
- **D6 (card checkbox).** Each task card gains a done/undone
  checkbox (left edge), dispatching `SetTaskDone` with the toggled
  value (click ≡ `task.complete`/`task.reopen`); state from the
  card model's done flag; done cards keep the existing strike/dim
  treatment.

### 3.3 Chunk 3 design — task view, dates, description (R3-F13, 14, 15, 16, 17, 28-part, 29 + #230, #228)

- **D7 (due field grammar).** The closed due editor gets hover
  styling + cursor and opens on click (seam exists,
  `task-view.slint:146-160`); it reads as interactive.
- **D8 (start date).** A scheduled-date editor beside due — the
  domain already carries `Task.scheduled` and
  `TaskChanges.scheduled` (`entities.rs:413-416`,
  `messages.rs:320-326`); the UI never touches it today. Same
  widget, same picker.
- **D9 (time component).** The date grammar grows an optional
  `HH:MM` part (`due_edit.rs` parse; values are already full
  `DateTime<Utc>` instants — today's date-only commits pin 00:00
  UTC, `due_edit.rs:42-45`); absent time keeps 00:00; the picker
  gets optional time steppers (stage-1 decides exact widget); echo
  shows time when set. Local-vs-UTC display follows the existing
  presentation rules.
- **D10 (picker behaviour).** The picker mounts as an **overlay**
  anchored to the field (the main-menu overlay pattern,
  `app-window.slint:348-369`) instead of pushing content
  (`due-editor.slint:79-181` inline today); entering due-edit opens
  it by default; closing the editor closes it.
- **D11 (value preservation + clear).** Opening the editor seeds the
  buffer with the set value (today it seeds empty and an immediate
  Enter clears, `view_state.rs:818-827`, `due_edit.rs` parse-empty);
  the TextField's trailing-affordance mechanism
  (`text-field.slint:30-34, 90-110`) gains a clear-"x" variant used
  by the due field; settings fields get the same reset-"x" when a
  saved value differs from default (cheap, same widget).
- **D12 (picker keyboard).** `move_days` syncs the displayed month
  to the cursor (`due_edit.rs:129-138` deliberately desyncs today);
  the month IconButtons become keyboard-activatable
  (`icon-button.slint:57-66` has no key handling) and the picker's
  hardcoded `Key.*` handling moves to preset-adapted bindings
  (chunk 6's registry work supplies them; until then arrows/Enter/
  Escape keep working).
- **D13 (reminder row times).** Rule rows drop the rfc3339 created
  column (`models.rs:923-926`) for a humanized presentation (offset
  in d/h/m as authored, created date short-form, at most
  second-precision) via `presentation.rs` helpers.
- **D14 (#230).** Due-soon horizon becomes config (`[ui]`
  `due_soon_days`, defaulting to the current constant) consumed by
  the presentation `DueSets`.
- **D15 (#228, gated).** If `TaskChanges` carries description
  (stage-1 verifies): an editable description block (multi-line
  TextField variant, commit-on-Return/checkmark, same seams);
  otherwise STOP and defer with a `backlog:` note.

### 3.4 Chunk 4 design — settings polish (R3-F3, 19, 20, 21, 23, 24, 25)

- **D16 (password eye).** `TextField.secret` gains a toggle
  (open/closed eye IconButton in the trailing slot, state persists
  while the form lives; never logged/models — secret rules hold).
- **D17 (InfoTip width).** InfoTip passes its help text as the
  tooltip caption term (or TooltipState gains a width-from-
  description mode) so the frame sizes from the *help text*, not the
  short title (`info-tip.slint:57-65` feeds `about` as caption;
  `tooltip.slint:95-97` sizes from caption).
- **D18 (status line placement).** The status line moves outside the
  Flickable (pinned between header and scroll viewport), keeping the
  round-2 tone+text contract.
- **D19 (inline validation).** `TextField` gains a tri-state
  validity flavour (neutral/focus → invalid/valid) driving border
  colour, plus an inline error line below the field (hover-box
  styling) — fed by a per-field validator seam; the settings ladder
  (`settings_form.rs`) exposes per-field validators; the round-2
  no-text-assertions rule applies to the error *copy* (tests pin
  state/structure). Save keeps its gate; the error line is
  immediate, per keystroke.
- **D20 (scale sync + drag + scroll).** (a) The field echoes the
  live scale (model property written by preview/commit/revert).
  (b) The slider gesture survives the preview re-layout: the
  Flickable must not steal the grab mid-drag (stage-1 mechanism:
  disable scroll-touch while the slider is pressed, or host the
  slider row outside the Flickable's gesture priority; recorded
  probe per house rules) — instant preview is kept if the probe
  allows, else preview-on-release (recorded fallback, user-approved
  order). (c) On scale commit, the scroll position is corrected so
  the scale row stays in view (anchored scroll around the slider's
  row; downward changes must not leave a blank screen).

### 3.5 Chunk 5 design — legacy config migration (R3-F22)

- **D21.** On load, a legacy single-server config (`[nextcloud]`
  table, no `[[sync.connectors]]`) migrates automatically: synthesize
  one `ConnectorCfg` (the legacy-synthesized-name precedent,
  `connectors.rs:29`) from `nextcloud.server_url/username`, strip
  the legacy table, persist via the comment-preserving overlay
  writer (`connections.rs:182`, atomic `config.rs:1221`), log at
  info, and proceed — the settings view never sees the legacy
  rejection again. Secrets stay keyed by server+username in the
  store (unchanged). Failure to persist migrates in-memory only and
  warns (app still works; next successful overlay write completes
  it). The typed `LegacyConfig` rejection becomes unreachable dead
  path — removed with its tests, per the "no pins of today's
  behaviour" rule. Backups: the pre-migration file is copied
  alongside (`taskboard.toml.bak-legacy`, best-effort).

### 3.6 Chunk 6 design — navigation, palette, menu (R3-F27, 32, 33, 34, 35, 36, 37, 38, 39, 40 + #231)

- **D22 (view switcher — options for R3-F37).** Option A
  (**recommended**): a two-segment `SegmentedControl` (round-2
  widget) in the toolbar — Board | Today, always visible, drives the
  existing `ViewToday`/`NavFocusBoard` actions; smallest surface,
  reuses an existing widget, matches the toolbar's role. Option B:
  tabs-style entries in the board bar (Today as a pseudo-tab) —
  hides the affordance once boards grow and muddies the tab
  metaphor (#411 deferred redesign). Option C: status-bar icon
  toggle — least discoverable, collides with reminder rows. The
  chunk implements A unless the user redirects at review.
- **D23 (ShowTask).** New action `nav.show-task` (name per registry
  conventions) + `view_state.show_task(task)`: select board, select
  the task's stack, focus the task, request scroll-to-row — reused
  by palette activation (fixing `palette_jump`'s board-only
  retarget, `view_state.rs:1175-1186`) and available to future
  features. Also verify the wide-layout path and
  view-switching-from-search (R3-F35's second half).
- **D24 (palette layout).** The palette panel gets a fixed
  input-row height with results listed below (`search-palette.slint`
  currently height-unconstrained, `app-window.slint:464-475`).
- **D25 (control navigation).** New registry rows
  `nav.next-control` / `nav.prev-control`: emacs C-f/C-b +
  C-n/C-p (outside text-editing chains), standard arrow-consistent
  bindings, vi j/k+C-f/C-b-analogues per contract — walking
  interactive elements (fields, buttons, picker) in reading order,
  reusing the focus walk; the date picker joins the walk and drops
  its hardcoded arrow grabs. Exact per-preset keys settle in stage-1
  against the conflict contracts (C-f/C-b remain stack.next/prev on
  the board — control-nav is for the other views' chains).
- **D26 (scroll actions).** `view.scroll-up` / `view.scroll-down`
  rows bound emacs C-v/M-v, standard PageDown/PageUp, vi Ctrl+f/
  Ctrl+b (free in vi since stack-nav uses g l/g h) — scrolling the
  focused scroll area (settings Flickable, today lists, board
  columns' ListView).
- **D27 (menu curation).** (a) Submenu pages sort by the same
  `(menu_rank, order)` rule with the view submenu re-ranked
  (go-to-board/today above next/prev view). (b) Entries irrelevant
  to the active preset (`mode.normal`/`mode.insert` vi rows) are
  filtered at page build (the preset is known at build time).
  (c) Dynamic greying: `MenuEntry` gains adapter-supplied
  enabled-flags for state-dependent rows (Cancel Move only when a
  move/drag flow is live; others enumerated in the audit report) —
  static `enabled` stays for permanently-disabled rows.
  (d) The always-visible hamburger kbd chip becomes hover-only
  (`toolbar.slint:65-69`).
- **D28 (C-g C-g).** Emacs gains the double-C-g convention: during
  prefix-pending chains a second C-g aborts the prefix (today a lone
  C-g parks as pending, `keymap` prefix machinery); recorded in the
  emacs preset docs and snapshots.
- **D29 (#231).** `reminder.snooze` / `reminder.mute` preset
  bindings land (all three presets, conflict-contract clean),
  snapshots regenerate.

### 3.7 Chunk 7 design — reminder OS-delivery plumbing (R3-F30; DR-3 fence)

- **D30 (fence).** DR-3 chunk 1 owns the settlement re-time (the
  prune-cascade that kills fired alarms,
  `reminders.rs:585-598`/`persistence.rs:495-501` — the in-app half
  of R3-F30 heals there). This chunk owns **delivery to the OS and
  visibility of delivery**: `DesktopSink` (`notifications.rs:159-199`)
  failures are warn-and-swallow today.
- **D31 (diagnose, then fix).** Stage 1 runs a live probe on this
  machine (Kubuntu 26.04, Wayland): session-bus reachability from
  the app's launch environment, a minimal `notify-rust` toast, the
  xdg-desktop-portal question (KDE Wayland often needs the portal
  path; `notify-rust` speaks FDO directly). Fix per verdict: portal
  backend or env/transport fix, with typed failure surfaced to the
  settings/today status (never silent), plus a retry/fallback
  posture (recorded; D-R13 alignment). A LogSink-testable seam for
  delivery attempts/outcomes lands so CI can pin behaviour without
  a real daemon.
- **D32 (in-app visibility of manual reminders pre-#474).** Until
  DR-3's settlement lands, manual reminders still cannot persist as
  rows — this chunk adds **no** workaround; the fence comment in
  code + issue note records the dependency ordering (chunk 7 may
  land before or after DR-3 chunk 1; neither doubles the other's
  fix).

## 4. Testing strategy

Round-2 §10 rules carry (mock clock, no text assertions, typed
outcomes, one-behaviour-one-test). Round-3 additions:
- **State machine**: a flow-transition table test — every
  `(Flow, effect)` pair asserts its settle/yield/blocked outcome
  (the totality is compiler-enforced by removing the default arm);
  pointer-click-outside journeys for R1; drop-miss journey for R2;
  "buttons work under any flow" journey set for R3/R4; every flow's
  visibility affordance pinned (R5).
- **Dates**: grammar proptest (date, date+time, clear, invalid);
  picker month-follow + keyboard journeys at the seam; overlay
  mount geometry pin.
- **Migration**: legacy → migrated round-trip (config content +
  overlay write + secret untouched + backup file), in-memory
  fallback on persist failure.
- **Delivery**: LogSink-seam pins for DesktopSink attempts/outcomes;
  no real-daemon test in CI (Tier-4 manual checklist for the KDE
  toast on this machine).
- **Menu**: ordering/visibility/greying pins are structural
  (rank order, presence flags, enabled flags) — never label text.

## 5. Code inventory

`ui-slint/ui/`: `text-field.slint` (focus-loss, validity, eye, x),
`slider.slint` (grab), `settings-view.slint`, `info-tip.slint`,
`tooltip.slint`, `due-editor.slint` (overlay, keys), `due_edit.rs`,
`board-layout.slint` (ghost size, glyphs, empty board, right-click,
checkbox, tooltips), `app-window.slint` (ghost, palette, overlays),
`search-palette.slint`, `toolbar.slint` (switcher, chip),
`task-view.slint` (title, scheduled, description), `rule-list.slint`,
`notification/delivery` seams; `ui-slint/src/`: `view_state.rs`,
`adapter.rs`, `actions.rs`/`presets.rs`/snapshots, `menu.rs`,
`models.rs`, `presentation.rs`, `keymap` (C-g C-g);
`taskboard-app/src/`: `config.rs` + `connections.rs` (migration),
`notifications.rs` (delivery). Domain: expected empty.

## 6. Slicing (7 chunks; dispatch plan is normative)

| Chunk | Design | Branch | Theme | Size |
|---|---|---|---|---|
| 1 | W-SM, D-* of §3.1 | `fix/ui-r3-flow-state-machine` | audit + rules R1–R5 + title re-edit | L |
| 2 | §3.2 (D1–D6) | `feat/ui-r3-board-surface` | ghost, glyphs, tooltips, context menu, empty board + presets, card checkbox, slint deprecations | L |
| 3 | §3.3 (D7–D15) | `feat/ui-r3-taskview-dates` | due/scheduled editors, time, picker overlay/keyboard, reminder rows, horizon, description | L |
| 4 | §3.4 (D16–D20) | `feat/ui-r3-settings-polish` | eye, InfoTip width, status placement, inline validation, scale trio | L |
| 5 | §3.5 (D21) | `feat/ui-r3-legacy-config-migration` | automatic migration | M |
| 6 | §3.6 (D22–D29) | `feat/ui-r3-navigation-menu` | switcher, ShowTask, palette, control-nav, scroll, menu curation, C-g C-g, #231 | L |
| 7 | §3.7 (D30–D32) | `fix/ui-r3-reminder-delivery` | OS-delivery plumbing probe + fix | M |

Serial default 1→7. Gates: 2 and 3 gated on 1 (flows), 4 on 3
(shared widgets), 6 on 3 (picker keys) — 5 and 7 are independent of
1–4 (different files/actors) and may run in parallel lanes on the
user's direction.

## 7. Risks

- **R1 The flow-machine rework is invasive** (interpret-table
  totality, focus-loss seams): the transition-table test is the
  regression harness; existing flow journeys must pass unless a test
  pinned a broken behaviour (recorded-decision rule applies).
- **R2 The slider-drag fix and the overlay picker depend on
  Slint gesture/grab behaviour** — both carry recorded stage-1
  probes with the fallbacks named (preview-on-release; inline mount
  if overlay anchoring misbehaves at min viewport).
- **R3 Inline validation invites text assertions** — error copy is
  presentation; tests pin validator state and visibility only.
- **R4 The Wayland toast fix may not be reproducible in CI** — the
  LogSink seam pins logic; the real-daemon verdict is a Tier-4
  manual item on this machine, by design.
- **R5 Chunk 5 removes a typed error path** (`LegacyConfig`) —
  confirm no other caller depends on it before removal (grep-gate in
  stage 1).
- **R6 Six concurrent design rounds are in flight** (DR-2…DR-6,
  undo) — the fences in §2 are binding; any stage discovering
  overlap STOPs and reports instead of touching the fenced area.

## 8. Backlog interactions & bookkeeping

- **Closes on merge**: #228 (chunk 3, if the description verify
  passes), #230 (chunk 3), #231 (chunk 6).
- **Stays open**: #210/#486-#496 (DR-5 time tracking — the card
  play/pause rides #489), #411, #412, #444, #351 (free-form editor;
  its migration half lands as chunk 5 and the issue gets an
  annotation), #463/#464 (perf track), #474/#476/#477 (DR-3 — chunk
  7 annotates the delivery half).
- **New issues to file after this round's design merges**: none
  planned; stage discoveries go to `backlog:` per AGENTS §7.
- **What opens next**: GUI review round 4 over this round's merges.
