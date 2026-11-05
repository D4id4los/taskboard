# [Plan] GUI Review Round 5 — Fix Round (multi-line editing, anchored flyouts, focus coverage, live settings, deadline-driven reminders, keybind context stack, account-first connections)

- **Date**: 2026-11-05
- **Status**: ACCEPTED — written from the user's
  2026-11-05 GUI review round 5 (19 findings, R5-F1…R5-F19 below).
  Session rulings Q1–Q6 of 2026-11-05 are confirmed and recorded as
  §3 decisions D1–D8; **D7 (the account-first connection flow) was
  ruled ACCEPTED by the user on 2026-11-05** — every chunk is
  dispatchable, chunk 8 additionally gated on DR-8 chunk 2 (#630)
  or its config-detection fallback. Companion dispatch plan:
  `.artifacts/plans/2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  — **8 chunks**, serial default with named gates.
- **Scope**: `taskboard-ui-slint` (most chunks) + `taskboard-domain` /
  `taskboard-state` (chunks 5–6: the settings snapshot, the
  deadline-driven reminder loop) + `taskboard-app` (chunks 5, 8: the
  app-lane settings ordering, the account/bind request surface).
- **Queue relationship**: the in-flight filesync round
  (DR-8, `.artifacts/plans/2026-11-05-filesync-substrate-design-round-plan.md`)
  is untouched except chunk 8's coordination gate (§4.8). The journal
  review round (DR-7) is untouched; its stage-3 planner embedding is
  the named future consumer of the W5b design round this plan fences
  (§4.7.4).

---

## 1. The user's round-5 findings (numbered for reference)

1. Task view: the **Time Entries button does nothing** (mouse, Space,
   Enter).
2. Task view: the description display's leftmost character on every
   line is cut off.
3. Task view: switching the description to edit mode shrinks it to
   ~4 lines, stuck above the footer.
4. Task view: the description edit field has no scrollbar and no
   scroll-by-cursor-navigation.
5. Task view: the description display has no scroll function; it cuts
   off content when too small.
6. Task view: the description display does not auto-wrap at its
   horizontal border.
7. Task view: Enter confirms the description edit; a multi-line field
   must take newlines on Enter and confirm on Shift+Enter.
8. Task view: the **quick-time button** closes the entire date-time
   picker when no date was keyboard-selected first — useless to mouse
   users, flaky otherwise.
9. The quick-time menu renders at the **top-left of the window**
   instead of beneath the time field.
10. The quick-time menu, opened with a time already set, shows only
    the set time instead of the option list.
11. Date-time picker: emacs C-p/n/f/b do not navigate the calendar
    while typing; after Down-arrow they work but plain typing is then
    impossible. Desired: type into the field while chords move the
    calendar selection simultaneously.
12. All description fixes should become **one reusable multi-line
    text-edit widget** (future journalling reuses it); consider the
    Slint markdown viewer as its view mode.
13. C-p/n/f/b focus navigation (emacs mode; vi equivalent suspected)
    works only in board view — missing in Today, Settings, Task.
14. Settings: the **board-selection dropdown never opens** (click,
    Enter, Space) — connecting/disconnecting boards is impossible.
15. Settings: Tab focus around the new sections (General and down) is
    erratic — stops skipped, later included after scrolling. Want a
    systemic guarantee: interactive elements join the tab walk by
    construction, or a check that errors when they don't.
16. The default reminder tick frequency should be 1 s (countdown
    cadence argument).
17. Settings changes (General, Reminders, Sync Timing, Display) must
    take effect **immediately**; restart-only is an exception that
    must argue why live application is near-impossible, with a
    documented decision. Core/domain changes are authorized as
    required.
18. The state-machine/flow/keybind system hampers composability (the
    planning agent's walk-stage-3 answer; the picker's integration
    difficulties). Want: composable widgets with focus movement and
    key bindings mostly out-of-the-box; a possible binding stack;
    custom editor keybinds (emacs/vi) inside multi-line editors;
    picker typing + chord navigation simultaneously.
19. The pick-one-of-many widget (settings connection type/keymap,
    today timer type) shows **no focus indication** and cannot be
    activated by Enter/Space; the settings interface-scale **slider**
    likewise has no focus and no keyboard interaction.

## 2. Root-cause map (verified against `main` @ `58ae9c55`)

| Finding | Root cause | Anchor |
|---|---|---|
| F1 | The `TaskView` mount forwards ~30 callbacks but never `task-track-entries-clicked`; an unbound Slint callback is a silent no-op. The Rust seam, router, flow and panel all exist. Tests bypass the callback (`interpret_for_test`). | `ui/app-window.slint:403-440` vs `:1179`; seam at `src/adapter.rs:6948` |
| F2 | Display `RoleText` sits at `x: 0` inside a rounded rectangle with a 2 px focus ring — no inset, ink starts under the ring/corner. No compiler-level clip is involved. | `ui/views/task-view.slint:870-875` (contrast title `:418-419`) |
| F3/F4 | The "multiline" editor is the single-line `TextInput` primitive with wrap flipped; multiline is fenced to a fixed three-line window (`min-height` + `clip: true`) because bare-field `preferred-height` growth was a width-dependent layout cycle (recorded probe). No scroll container exists anywhere in the crate. | `ui/widgets/text-field.slint:133-139`, `:198-219`; fence note `task-view.slint:840-844` |
| F5/F6 | Display mode: a stretched `Rectangle`, no scroll; and the `Text` has `wrap: word-wrap` but no width (not in a layout → natural unwrapped width), so wrap never engages. | `task-view.slint:852-898` |
| F7 | On the pinned Slint, a single-line `TextInput` fires `accepted` on Return without inserting a newline (recorded probe). With `single-line: false` (never set today) Return inserts a newline natively. | `text-field.slint:64-69`, `:198` |
| F8 | Every picker-internal interaction that routes through the adapter sets the `due_focus_steal` latch (suppressing the field's focus-lost settle); the quick-times chevron toggles widget-local state only, the latch stays unset, and `settle_date_editor` closes the whole `EditDate` flow. Pressing Down first moves focus off the field, so no focus-lost fires — the asymmetry. | `ui/widgets/time-field.slint:126-133`; latch at `src/adapter.rs:6473-6475`, `:6590-6594` |
| F9 | The flyout's underlay is displaced to cover the window (`x/y: -anchor.absolute-position`), making underlay-local origin = window origin; the flyout's `x: 0; y: row-height` was authored in `TimeField`-local space. The `Dropdown` widget copied the grammar verbatim and shares the defect. | `time-field.slint:142-159`; `ui/widgets/dropdown.slint:256-273` |
| F10 | The quick-list model is filtered through the field's **value mirror** at open (`filter_digits("14:30")` matches exactly one slot). The filter exists for typed-digit narrowing; open-time seeding is the bug. | `src/adapter.rs:1055-1074`, `:6549-6555` |
| F11 | The `DatePicker` keymap context enters the chain only while the grid holds Slint focus; while the field has focus, insert-mode masking makes the chain `[ViInsert, Global]` where the chords are unbound (they die in native text input). After the Down hand-over the grid holds focus with no `key-pressed` handler and no path back except Return/Escape/Tab/click. Field focus and picker context are mutually exclusive by construction. | `src/shell.rs:112-130`; `src/adapter.rs:634-655`, `:590-604` |
| F13 | The walk machinery is view-agnostic (synthetic Tab over native traversal). Emacs binds the chords in TaskDetail/Settings (and r4 gave the task view stops) — but the walk *lands on invisible stops*: `SegmentedControl` never binds its visuals to `seg-focus.has-focus`, `SettingsButton` is a plain `Rectangle`+`TouchArea`, `Slider` has no `FocusScope`. On Today, C-n/C-p walk the Rust item model by design (board parity) — the perceived gap is the invisible/skipped stops, not absent bindings. | `ui/widgets/segmented-control.slint:56-58`, `:77-97`; `ui/widgets/slider.slint`; `settings-view.slint:230-259` |
| F14 | Three stacked causes: the shared flyout coordinate defect (F9); the flyout is clipped by the settings `Flickable`'s viewport (the widget "deliberately refuses a window overlay" — a recorded decision D6 overrides); and keyboard open is gated on `rows.length > 0` (empty local-board list → rejected, zero-height flyout). | `dropdown.slint:136`, `:180`, `:251-257`, `:273`; `settings-view.slint:594`, `:682-702` |
| F15 | Slint traversal accepts only visible (or Flickable-clipped) candidates; conditionally-mounted rows with unresolved layout are skipped until a scroll forces layout (the recorded P5 class); traversal order is tree order; and the new sections are full of non-stop widget classes (F13). All conspire. | `settings-view.slint:561-588`, `:594-663`; probe P5 `tests/walk_probe_tests.rs:151-178` |
| F16 | Countdowns already refresh at 1 Hz on the UI's own repaint timer, independent of the engine tick. What lags is **firing**: with the 30 s default tick, a due reminder fires up to 30 s late (per-tick engine work + DB-write-before-notify is why the cadence was bounded). | `src/lib.rs:628-654`; tick loop `taskboard-state/src/actor.rs:559-597`; default `taskboard-app/src/config.rs:57` |
| F17 | Every General/Reminders/Sync-Timing/Display setting is `SectionApply::OnRestart` ("no signal, no hot reload"); the engine loop, sync actors, notifier and the UI's own derivations capture boot-time copies; no `StateCommand` variant for settings exists. | `src/requests.rs:189-194`; `config.rs:1497-1511`; `taskboard-app/src/ui.rs:423-451` |
| F18 | One global key path; the chain is *reconstructed from shell booleans* per keystroke; widget-internal grabs can never win (the capture-tunnel rule), so every widget/key negotiation is a special case accumulating in `intercept` (dropdown ladder, timer-setter Escape, due Down, activation reservation — four today). The flow slot is XOR with no nesting; every new flow fans out across ~8 sites; hosted sub-state is flat adapter fields with hand-rolled invariants. | `src/adapter.rs:578-686`, `:4897-4919`; `src/view_state.rs:171-248`; audit `.artifacts/reports/2026-10-26-ui-flow-state-audit-report.md` §6 |
| F19 | `SegmentedControl` relies on `accessible-action-default` only (AT-invoked, not raw keys — the trap `IconButton` already documented and solved with a raw-key half); no visual binds to segment focus. `Slider` is pointer-only plus AT increment/decrement. | `segmented-control.slint:15-19`, `:85-97`; contrast `ui/widgets/icon-button.slint:53-64`; `slider.slint:40-60`, `:112-149` |

**Verified negative (recorded for history)**: Slint has **no
`MarkdownViewer`** — zero occurrences in the pinned 1.18.1 sources,
and none in the upstream changelog through 1.19-dev. What exists
since 1.16 is `StyledText` + the `@markdown(...)` macro, which parses
markdown **at compile time** from literals (runtime interpolations
insert as plain text runs — `i-slint-compiler-1.18.1/passes/resolving.rs:1249-1330`).
User-authored markdown bodies therefore cannot render through it.
D1's view-mode staging follows from this.

## 3. Recorded decisions (Q1–Q6 ruled by the user, 2026-11-05)

1. **D1 — the reusable multi-line editor (F3/F4/F5/F6/F7/F12, Q1)**:
   an in-house L1 widget **`MultiLineEditor`** with view/edit modes,
   built on `TextInput { single-line: false }` inside a scroll
   viewport (cursor-following scroll + scrollbar), **not** the
   std-widgets `TextEdit` (house theme/seams, preset-driven editor
   keybinds from D4, zero-std-widgets posture). Commit grammar:
   **Shift+Enter commits, Enter inserts a newline, Escape discards**
   (Vi preset: same chords; the vi editor-normal ladder is D4's).
   View mode v1 = plain text (wrapped, inset, scrollable). Stored
   descriptions (and future journal entries) are **treated as
   markdown going forward**; runtime markdown rendering is a separate
   gated chunk (D1b): a feasibility spike on constructing
   `styled-text` values from Rust (e.g. `pulldown-cmark`, which
   Slint itself uses) — if the spike fails, the negative is recorded
   and the view stays plain text until upstream ships a runtime
   markdown element. Licensing constraint: Slint's
   `widgets/common/textedit-base.slint` is GPL/royalty-free-licensed
   — it is the *reference grammar* (single-line flag, viewport
   content-height, cursor-position-changed scroll math), never copied
   code.
2. **D2 — reminder firing (F16, Q2 = option b)**: the engine loop
   becomes **deadline-driven** — `sleep_until(min(next due instant,
   now + tick))`, re-armed on every command batch that changes
   dues — plus **`DEFAULT_REMINDER_TICK_SECS` 30 → 1** so the tick is
   a cheap fallback sweep rather than the firing mechanism. Per-tick
   cost audit rides the chunk (the stale-delivery sweep moves to a
   coarser cadence or becomes event-driven if it touches the repo
   every second).
3. **D3 — live settings (F17, Q3 approved; no known restart-only
   settings)**: a typed **`SettingsSnapshot`** in `taskboard-domain`,
   owned by `AppState` (the engine becomes the single source of
   truth). Write path: UI commit → app-lane prove-and-write
   (unchanged overlay) → `StateCommand::UpdateSettings(snapshot)` →
   engine updates `AppState` and publishes (ArcSwap read side
   refreshes UI derivations) **plus a change notification that wakes
   sleeping actors** (sync loops re-arm poll, notifier re-resolves
   sink, the reminder loop re-reads tick/deadlines). Read side:
   actors read the snapshot per iteration instead of spawn-time
   copies. A per-setting **apply-semantics registry** makes live the
   default; `OnRestart` requires a written justification entry in
   that registry (the documented-decision mechanism the user asked
   for) — v1 ships with an empty exception list. Env-locked settings
   keep their existing lock semantics.
4. **D4 — the keybind composability system (F11/F18, Q4 approved)**:
   staged. **W5a (dispatched, chunk 7)**: a *registered context
   stack* — widgets/surfaces register key scopes (a `ContextId` +
   mask semantics) on mount/focus; the resolver consumes the ordered
   stack (focused-element scope → enclosing surfaces → view → mode →
   global) instead of a reconstructed boolean chain; the intercept
   special cases retire into scope declarations. Includes the
   **text-editor keybind scope**: a preset-provided editor command
   table (emacs: C-p/n/f/b cursor, C-a/e line edges, C-k kill-line,
   C-d forward-delete, **C-Space set-mark, C-w cut, M-w copy, C-y
   paste** per the user's addition, Shift+Enter/C-c C-c commit,
   C-c C-k/Esc discard; vi: the stripped normal-mode ladder) so any
   text widget gains editor keybinds by declaring the scope;
   **single-slot clipboard** (no kill-ring accumulation — documented
   leniency at the type). Governing rule — **the printable
   monopoly**: while a text field holds focus, printable characters
   always reach the field; non-editor scopes may consume only
   chord-class strokes (modifiers present or non-printable named
   keys). This yields F11's "type while chords move the calendar" by
   construction, and under the vi preset it means grid motion while
   typing is arrows-only (h/j/k/l are printables — reserved to the
   field); after Esc the DatePicker context owns them. **W5b (own
   future design round, not dispatched)**: hostable sub-surfaces for
   flows — a flow hosts a guest surface (guest owns interaction
   state + key scope; host sees guest-completed/guest-cancelled;
   settle composes guest-first). Named consumer: the journal review
   walk's stage-3 planner embedding.
5. **D5 — focus-coverage invariant (F15, Q5 = test first)**: a
   per-view structural test (a) walking the view via the synthetic-Tab
   mechanism and (b) asserting the enumerated stop inventory equals
   the inventory of interactive widgets (item-tree scan for
   click-bearing `TouchArea`s outside any stop — via the pinned
   `i-slint-core` APIs the probes already use), so an interactive
   element missing from the walk is a build failure. The on-screen
   debug focus-ring aid is **deferred** until the user needs manual
   debugging the test cannot express.
6. **D6 — anchored flyouts (F9/F14, Q6a)**: the `Dropdown` and
   `TimeField` flyouts convert to the **window-anchored overlay
   pattern** (facts-global geometry mirroring + window-level mount +
   dismissal underlay — the DatePicker/action-menu grammar). This
   **overrides the recorded "no window overlay" decision** at
   `dropdown.slint:251-257` (r4 D39's frugality clause); the
   coordinate-space defect (F9) is subsumed.
7. **D7 — account-first connections (F14, Q6b — ruled ACCEPTED by
   the user, 2026-11-05)**: see §4.8. The concept model is
   three-layer — **account** (server + username + credential; the
   keyring key `(server_url, username)` is already account-shaped) →
   **services** enabled per account (Deck boards = the connector;
   Files/journal roaming = `[files_sync]` per DR-8 D-SF10) →
   **bindings** (local board ↔ remote Deck board). Flow: sign in →
   enable services on the account card → the Deck service's mapping
   list shows **live remote boards**, each connectable to an unbound
   local board or spawnable as a new local board; the exact
   title-match heuristic retires. Board-first (the user's flow 1) is
   rejected as the primary flow because it cannot express a Files
   connection (no board to pick first) and forces filesync into a
   second-class path; it may return later as a shortcut from a board
   context menu. Alternatives recorded: board-first with
   create-remote option; the status quo (local-board picker +
   title-match adoption).
8. **D8 — dead-seam class (F1)**: beside the one-line fix, the
   Slint-side manifest — a source-scan test asserting every callback
   a view composition declares is forwarded by its mount site,
   complementing `tests/seam_inventory.rs` (which only covers the
   Rust install side).

## 4. Design per workstream

### 4.1 Chunk 1 — dead seams (F1, D8)

- Forward `task-track-entries-clicked()` in the `TaskView` mount
  (`ui/app-window.slint:403-440`).
- New test `tests/view_mount_inventory.rs` (working name): parse the
  `.slint` sources — for every component instantiation of a
  view/composition in `app-window.slint` (and the views' own mounts
  of compositions where applicable), every `pure callback` declared
  by the instantiated component must have a handler at the mount
  site. Source-scan (not compiler introspection) is deliberate: the
  failure mode is a missing line of markup, which a source assertion
  pins exactly. Journey: invoke `window.invoke_task_track_entries_clicked()`
  and assert the track-entries flow opens.

### 4.2 Chunk 2 — `MultiLineEditor` (F2–F7, F12, D1)

- New L1 widget `ui/widgets/multi-line-editor.slint`: view mode =
  wrapped, inset `RoleText` inside a scroll viewport; edit mode =
  `TextInput { single-line: false; wrap: word-wrap }` inside the same
  viewport class, with `content-height: max(visible-height,
  input.preferred-height)` and cursor-following scroll adjustment on
  `cursor-position-changed` (the reference grammar, re-implemented —
  D1's licensing note). A visible scrollbar (house tokens) appears
  iff content exceeds the viewport.
- **Probe first**: a probe test pinning that `preferred-height`
  inside a viewport (never as a bare layout constraint) grows and
  re-wraps on width change without the recorded panic — the fence at
  `text-field.slint:133-139` stays until the probe passes; if it
  fails, the fallback is a paged viewport (page up/down + cursor
  follow via scroll math only) and the negative gets recorded.
- Commit grammar: Shift+Enter → `accepted`; Enter → native newline;
  Escape → `discarded`; focus-lost → existing settle semantics. The
  single-line `TextField` stays for every other field.
- Task-view adoption: the description section swaps both branches for
  the widget; display inset/wrap (F2/F6) and both-mode scrolling
  (F4/F5) fall out. The `commit_description` funnel already
  round-trips newlines verbatim — no Rust change beyond the seam
  rename if any.
- Out of scope: editor keybind scope (chunk 7 adopts the widget),
  markdown view (D1b chunk).

### 4.3 Chunk 3 — anchored flyouts + quick-time fixes (F8/F9/F10, D6)

- Convert `Dropdown` and `TimeField` flyouts to the window-anchored
  overlay: a facts global per widget class mirroring anchor geometry
  (`changed absolute-position` handlers, the `DatePickerFacts`
  grammar), window-level conditional mount with clamp/flip, dismissal
  underlay ahead of the panel. The in-place displaced-underlay code
  (`time-field.slint:142-159`, `dropdown.slint:256-273`) is deleted,
  not repaired — D6 rules the overlay pattern.
- Quick-time chevron and the due-editor calendar toggle route through
  adapter seams that set the `due_focus_steal` latch (the same
  suppression every `apply_picker_key` interaction gets), fixing F8.
  The systemic focus-exit rule arrives with chunk 7 (a flow settles
  only when focus leaves the flow's whole scope subtree); the latch
  extension is the bounded interim.
- Quick-list seeding: the digit filter state resets to unfiltered at
  picker open; typed-digit narrowing (per keystroke) keeps its
  behavior. F10 fixed.
- Dropdown keyboard open stays gated on non-empty rows — but the
  empty state becomes *visible* (a disabled field with an
  "no boards yet" caption) instead of a dead control.
- Trap: the toolbar's `SegmentedControl`, the board-bar's `Dropdown`
  switcher, and the timer-setter/track-entries `TimeField`s all ride
  these widgets — the chunk's journeys must cover each mount site
  (position under anchor, dismissal, keyboard ladder unchanged).

### 4.4 Chunk 4 — focus coverage + the invariant (F13/F15/F19, D5)

- `SegmentedControl`: bind the focus ring to `seg-focus.has-focus`
  (the IconButton grammar); add the raw-key half (Return/Space →
  `picked(index)`), preserving `accessible-action-default`.
- `Slider`: gains a `FocusScope` (tab stop), focus ring, Left/Right =
  step (with the existing preview/commit ceremony), Up/Down alias,
  Home/End = min/max. AT increment/decrement unchanged.
- `SettingsButton` becomes a stop (the `ActivationStop` grammar).
- Settings view root: `focus-on-tab-navigation` + mount focus parity
  with the other views.
- The invariant (D5): `tests/focus_coverage.rs` — per view, (a) walk
  all stops via synthetic Tab from the view root (reading focused
  ids through a debug facts global, the `TaskViewWalkFacts`
  mechanism generalized), (b) scan the instantiated item tree for
  click-bearing `TouchArea`s not enclosed by a focus stop. (b) is the
  error-on-missing guarantee; (a) pins reading order. Newly added
  interactive elements fail the build unless they are stops.
- Trap: the visibility filter — stops inside `Flickable`s are
  eligible (the Flickable carve-out), but non-Flickable `clip: true`
  rects exclude their children from traversal (`today-view.slint:225`,
  `review-view.slint:166`, `board-layout.slint:370`,
  `text-field.slint:136`, `time-field.slint:167`, `dropdown.slint:280`
  are the current sites — the chunk audits each for stop exclusion).

### 4.5 Chunk 5 — live settings (F17, D3)

- `taskboard-domain`: `SettingsSnapshot` (the General/Reminders/Sync
  timing/Display fields; typed, A1 minting where values are
  constrained — tick seconds ≥ 1 reuses the existing validation) +
  `StateCommand::UpdateSettings { snapshot }`. `AppState` grows the
  settings field; boot seeds it from figment (the existing
  `resolve_*` accessors become snapshot constructors).
- `taskboard-state`: the engine applies the command (receipt +
  persistence action unchanged — settings persist via the app-lane
  file overlay, not sqlite), publishes through the existing
  `StateUpdated` signal, **and sends a new `SystemEvent::SettingsChanged`
  (or a dedicated `watch` channel — the chunk's plan settles which,
  bias: broadcast, no new channel class)** that sleeping actors
  select on.
- Consumers re-read per iteration: the reminder loop (enabled, tick,
  defaults, deadline re-arm), sync actors (poll/backoff — re-arm the
  sleep on `SettingsChanged`), notifier (sink re-resolution), the
  UI's day-start/due-soon derivations (already derive from the pushed
  model — now the model carries live values).
- `taskboard-app`: ordering — persist-then-command (a failed
  prove-and-write sends nothing); the reply snapshot now round-trips
  through the engine, keeping one source of truth.
- `taskboard-ui-slint`: the `SectionApply` badges flip to live; the
  registry (per-setting apply semantics + justification field for
  `OnRestart`) lives beside the settings form predicates; UI test
  asserts no `OnRestart` entries exist without justification text.
- Trap: env-locked settings (the `env_overridden` set) must remain
  un-writable and visibly locked — the existing absorption logic
  moves into the snapshot apply path.

### 4.6 Chunk 6 — deadline-driven reminders (F16, D2)

- Domain: a `next_due_instant(state, now)` query (pure; proptested
  against the firing evaluation so the two can never disagree).
- State: the run loop's tick arm becomes
  `sleep_until(min(next_deadline, now + tick))`; every command batch
  that can change dues (task due updates, reminders, board
  mutations) re-arms the deadline. First tick stays immediate.
- `DEFAULT_REMINDER_TICK_SECS` 30 → 1 (`config.rs:57`, the settings
  placeholder, the help copy, `taskboard-state/src/config.rs` docs).
- Per-tick cost audit: the stale-delivery sweep and any per-tick repo
  reads move to a coarser cadence (e.g. once per minute or on
  `SettingsChanged`) unless measured trivial — the chunk's plan
  records the measurement.
- Tests: `tokio::time` paused, advance-to-deadline journeys (fire at
  the instant, not at the next tick); the day-boundary roll keeps its
  existing arming.
- Trap: a 1 s tick multiplies engine wakeups — the audit must show
  the idle path is a clock read + a min() (no repo IO) or the sweep
  gating lands in this chunk.

### 4.7 Chunk 7 — the keybind context stack (F11/F18, D4 = W5a)

1. **Scope registration.** A `KeyScope` concept in the adapter: any
   surface registers `(ContextId, mask)` on mount and any focusable
   widget registers its scope on focus gain (the DatePicker's
   grid-focus seam generalized — `install_picker_grid_seam` becomes
   one instance of the mechanism). The resolver consumes the ordered
   stack: focused-element scope → enclosing surface scopes (innermost
   first) → view context → mode → global. The `KeymapShell::chain`
   boolean reconstruction retires; masks stay declarative (modal
   contexts keep total masks).
2. **The editor scope.** `ContextId::Editor` + a preset-provided
   command table (D4's emacs/vi sets). The `MultiLineEditor` (and
   optionally other fields) declares the scope while focused;
   commands route to the focused editor through a target seam.
   Slint `TextInput` selection APIs (`set-selection-offsets`) carry
   the mark/region; the clipboard is a single adapter-side slot.
3. **The printable monopoly.** In `intercept`: while
   `EditingFacts.text-focus-count > 0`, a stroke with no modifiers
   and a printable text is `Unhandled` before any non-editor scope
   sees it. Chord-class strokes resolve against the stack — so the
   DatePicker scope, registered by the picker surface while open,
   serves C-p/n/f/b to the calendar while the field keeps every
   printable (F11). Under vi, h/j/k/l while typing are the field's;
   after Esc the DatePicker scope owns them.
4. **Retire the special cases.** The dropdown ladder decline, the
   timer-setter Escape, the due Down-arm, and the activation
   reservation become scope declarations/ladder rules at their
   widgets; `intercept` keeps only the monopoly rule, the re-entrancy
   guard, and the resolver feed. Each retired special case keeps its
   journey test as the acceptance pin.
- Trap: ordering — the stack must reproduce every pinned journey
  (the C-g contract, the modal masks, insert-mode printables, the
  walk's Tab re-entry). The chunk's plan enumerates the journey
  matrix as the port checklist; `flow_table_tests` must stay green
  untouched.
- Out of scope: hostable sub-surfaces (W5b — own design round; the
  journal walk's stage-3 planner embedding is its first consumer),
  any new flow variants.

### 4.8 Chunk 8 — account-first connections (F14 rest, D7 — gated on the user's flow ruling + DR-8 chunk 2)

- **Gates**: (a) the user rules D7 (account-first) — this section is
  the recommendation; (b) DR-8 chunk 2 (#630, the files transport
  wiring) has landed or the chunk hides the Files toggle behind
  config detection.
- Config stays connector-array-shaped in v1: the UI **groups**
  connectors by `(server_url, username)` into account cards; a
  Deck service = one connector entry; the Files service = the
  `[files_sync]` table (D-SF10 shape, untouched). A `[[accounts]]`
  config refactor is fenced until multi-service config actually
  needs it (recorded; not this round).
- New app-lane request `ListRemoteBoards { connector }` → live
  `DeckClient::boards()` filter → typed failures (`NoSecretStored`,
  network). Bind goes explicit: `AttachConnector` carries the chosen
  `RemoteBoardId`; the exact title-match heuristic and its
  `NoRemoteMatch` adoption arm retire. "Create local board from
  remote" = `CreateBoard` + `BindBoard` (both exist). Engine commands
  unchanged.
- UI: the connections section becomes account cards (sign-in form →
  account list → per-account service toggles → the Deck mapping
  list: remote board rows with local-board picker or create-new).
  Full keyboard operability via the chunk-4 stop grammar; the
  local-board picker reuses the chunk-3 overlay dropdown. "Push local
  board to remote" is fenced (Deck board-creation API — backlog).
- Account removal guard: refuses while bindings exist (cascade-with-
  confirm is a backlog option).

## 5. Gated/deferred work (recorded so nobody re-derives it)

- **D1b — markdown view mode**: spike (Rust-side construction of
  `styled-text` values, e.g. via `pulldown-cmark`) then implement or
  record the negative. Chunk after 2; gated on nothing else.
- **W5b — hostable sub-surfaces design round**: the flow-nesting
  protocol (guest state ownership, settle composition, the pair-table
  extension). Opens before the journal walk's stage-3 work needs it.
- **Focus debug aid** (D5): only if the user needs manual debugging
  the invariant test cannot express.
- **Deck "push board to remote"** + **account-removal cascade**:
  backlog issues (chunk 8 opens them).
- **`[[accounts]]` config reshaping**: fenced until needed.

## 6. Test-strategy hooks

- Probes: viewport `preferred-height` growth (chunk 2's gate);
  flyout anchoring under flip/clamp (chunk 3); the monopoly rule
  (printable passthrough vs chord capture, both presets — chunk 7).
- Journeys: the track-entries button through the real callback
  (chunk 1); the picker's type-while-navigating session (chunk 7);
  the account→service→bind session over the fake engine (chunk 8).
- Engine (insta): `UpdateSettings` apply + signal; deadline firing at
  the instant (paused clock); tick-cost audit note in the chunk plan.
- The invariant: `focus_coverage.rs` per view (chunk 4) — floors not
  ceilings; the test must fail when a stop is removed and pass for
  any correct markup, not pin today's accidents.

## 7. Chunk mapping (dispatch plan carries the prompts)

| Chunk | Branch | Covers | Size |
|---|---|---|---|
| 1 — dead seams | `fix/ui-slint-dead-seams` | §4.1 | S |
| 2 — MultiLineEditor | `feat/ui-slint-multi-line-editor` | §4.2 | M |
| 3 — anchored flyouts | `refactor/ui-slint-anchored-flyouts` | §4.3 | M |
| 4 — focus coverage | `feat/ui-slint-focus-coverage` | §4.4 | M |
| 5 — live settings | `feat/app-live-settings` | §4.5 | L |
| 6 — deadline reminders | `feat/state-deadline-reminders` | §4.6 | M |
| 7 — keybind context stack | `refactor/ui-slint-keybind-scopes` | §4.7 | L |
| 8 — account connections | `feat/app-account-connections` | §4.8 (gated) | M |

Default order: 1 → 2 → 3 → 4 → 7 → 5 → 6 → 8. Independence: 1, 2, 3
are mutually independent (disjoint files except `app-window.slint`
mount blocks — rebase-light); 4 depends on 3 only for the overlay
dropdown reuse in later chunks (not for its own content) and may
parallel 3; 7 depends on 2 (the editor is the scope's first
consumer); 5 and 6 are serial (5 → 6, same engine loop); 8 is gated
(D7 ruling + DR-8 #630).

## 8. Risks

- **The viewport probe fails** (chunk 2): fallback is the paged
  viewport; the description editor still gains newlines, commit
  grammar, and cursor-follow — the growth fence stays with a
  recorded negative.
- **The stack reorder regresses a pinned journey** (chunk 7): the
  journey matrix is the port checklist; any behavioral diff is a
  finding, not an accepted change — the pinned semantics are the
  contract.
- **Live settings vs sleeping actors** (chunk 5): a `watch`-missed
  re-arm is the failure mode; the chunk's plan must pin "poll change
  visible within one re-arm" with a paused-clock test.
- **1 s tick cost** (chunk 6): measured in-chunk; the sweep gating is
  in scope if the idle path touches the repo.
- **Chunk 8 drift against the in-flight filesync work**: the config
  detection gate + disjoint files (connections UI vs sync-files
  crates) bound it; rebase friction with DR-8 chunks is expected and
  handled by the standard dance.
