# [Plan] UI Round 5, Chunk 7 — The Keybind Context Stack (F11/F18, D4 = W5a)

- **Date**: 2026-11-07
- **Branch / worktree**: `refactor/ui-slint-keybind-scopes` in
  `../taskboard-WT-refactor-ui-slint-keybind-scopes` (off fetched
  `origin/main` @ 6c3d353b; chunk 2 `feat/ui-slint-multi-line-editor`
  verified merged — commits 307b9723/dc229e4f).
- **Origin (normative)**: round plan
  `2026-11-05-gui-review-round5-plan.md` §2 (F11, F18), §3 D4 (W5a),
  §4.7; dispatch plan `2026-11-05-gui-review-round5-implementation-dispatch-plan.md`
  §1/§8; flow audit `2026-10-26-ui-flow-state-audit-report.md` §6 (the
  settle matrix — NOT touched by this chunk: no flow outcomes change).
- **Kind**: refactor + feature (the scope stack replaces boolean chain
  reconstruction; the editor scope is new machinery). TDD throughout.
- **Non-goals** (dispatch §8): hostable sub-surfaces (W5b), new flows,
  preset binding content changes beyond the editor tables.

All file/line references below were verified against this branch's
tree (the dispatch prompt's line numbers had drifted; the current
anchors are recorded so stage 2/3 don't re-derive them).

## 1. What exists today (the port surface)

### 1.1 The intercept funnel — `src/adapter.rs`

- `to_stroke` (adapter.rs:3887) — `SharedString`+`KeyboardModifiers`
  → `Option<Stroke>` via `Stroke::from_event`; rejects multi-char
  texts and modifier-only/control events.
- `Adapter::intercept` (adapter.rs:630-738) — in order:
  1. walk-re-entrancy guard (`walk_reentry` flag, :636);
  2. the D20 mode pulls (EditingFacts `text-focus-count` ↔
     `Mode::Insert`, :642-656 — NOT a special case, stays);
  3. **special case A** — timer-setter Escape (:665-670): explicit
     dismissal ahead of the resolver;
  4. **special case B** — due Down-arm (:679-693): `Flow::EditDate` +
     bare `Down` opens the picker grid-focused / performs the
     field→grid focus hand-over;
  5. `sync_view_context` + `self.shell.chain()` (:696-697);
  6. **special case C** — activation reservation (:698-714):
     `activation_focus_count > 0` + bare Return/Space → decline
     `Action`/`Unhandled` answers to the focused cell;
  7. `resolver.feed(stroke, &chain, &self.keymap)` and the four
     `Resolution` arms (:715-737).
- `on_intercept_key` closure (adapter.rs:5915-5970) — ahead of
  `intercept`: the walk guard check, then **special case D** — the
  dropdown ladder decline: `SettingsBoardFacts.open ||
  BoardBarFacts.open` → decline arrows/`C-n`/`C-p`/Escape/Return
  (:5939-5950); `TimeFieldFacts.open` → decline
  arrows/Escape/Return (:5956-5968).

### 1.2 The chain — `src/shell.rs` `KeymapShell::chain` (83-153)

Boolean reconstruction from `menu.open`, `palette`, `focus_overlay`,
`track_entries`, `review_walk` (mirrored from view_state by
`sync_view_context`), `mode`, `date_picker_grid` (mirrored by
`install_picker_grid_seam`, adapter.rs:7398-7407), `view`. Nine
distinct chain shapes (enumerated in §4's port checklist).

### 1.3 Resolution — `src/keymap/mod.rs`, `src/resolver.rs`

`Keymap::resolve/has_prefix/visible_chain` walk a `&[ContextId]`
chain with first-match-wins and masking stops (`masks` = Menu,
Palette, FocusMode, TrackEntries, ReviewWalk — pinned by
`presets_mask_only_the_modal_contexts`). `KeymapResolver::feed`
accumulates prefixes, resets on chain change. The C-g contract lives
at keymap/mod.rs:614-641 (`emacs_c_g_cancels_on_the_board_chain_…`)
and :774-817 (double C-g) — pins that must survive byte-identically.

### 1.4 Stroke grammar — `src/keymap/stroke.rs`

`Stroke::from_event` (:142-179): named keys (NAMED_KEYS table, :18-46)
accepted with modifiers verbatim; then control chars rejected; then
non-printables rejected; bare printables fold shift into the case-
carrying char and clear all mods; chorded printables keep mods.
`is_printable` (:208) = `!is_control && not a named char`.

### 1.5 Focus facts — `ui/editing-facts.slint`

`text-focus-count` (+1/−1 with the generation re-baseline latch —
see the replica in `ui/widgets/multi-line-editor.slint:217-250`),
`activation-focus-count`, `generation`. Adapter reset seams zero the
counts and bump `generation` (adapter.rs:976-980; e.g. :1735 for the
picker close, :1890 comment for the stale-absorb discipline).

### 1.6 The editor surface (chunk 2) — `ui/widgets/multi-line-editor.slint`

Callbacks: `edited(string)`, `accepted(string)`, `focus-lost()`;
fact mirrors: `buffer`, `input-has-focus`, `content-*`, `scroll-y`,
`cursor-in-view`; `grab-focus()`/`focus-token`. Key grammar
(:279-285): only Shift+Return handled; everything else `reject`ed to
native. The header comment explicitly reserves this chunk's scope
attachment and the focus-publication dedup decision.

### 1.7 The pinned journeys (acceptance pins — must pass UNCHANGED)

| Journey | Pin |
|---|---|
| `the_control_walk_reaches_fields_buttons_and_the_picker` (journeys.rs:7210) | Down-in-field hands focus to the grid; context arms; Escape resolves ModeNormal; clean close |
| `the_due_picker_journeys_reach_the_boundary` (:8430) | the two-phase ladder: field → grid → Tab to month button → Return |
| `the_combobox_is_the_only_time_path` (:8193) | typed filter/quick-pick/commit funnel |
| `the_bar_dropdown_ladder_declines_the_action_layer_under_every_preset` (:12548) | special case D under all three presets |
| `the_setter_dismissal_layers_close_without_committing` (:11036) + `the_setter_composes_the_custom_row_grammar` (:10936) | special case A |
| `the_focused_cell_activates_on_return_and_space_under_any_preset` (:6559) | special case C |
| flow_table_tests.rs walk re-entry (:983-1015 context; the flow×effect pair table) | Tab traversal under every flow — untouched |
| keymap/mod.rs C-g tests (:614, :774) | C-g exact-match vs prefix semantics |
| `move_cancel_routes_through_escape_and_emacs_c_g` (journeys.rs:5717) | the session-level C-g contract |
| `the_entries_overlay_walks_rows_and_the_escape_ladder_steps_back` (:12349) | modal TrackEntries chain |

`flow_table_tests` and the audit §6 settle matrix are untouched: no
flow outcomes, no `resolve_conflict` rows, no settle behavior change.

## 2. Decision D-KS1 — the KeyScope data model

**A scope is a declarative registration row, evaluated (pulled) at
intercept time from already-mirrored state — not an imperative
push/pop stack.** The P5-class trap (a widget unmounts and its scope
registration survives) is avoided by construction: every scope's
arming condition is a fact the adapter already re-reads per
keystroke (the D20 "pull-at-intercept is self-healing" doctrine) or a
focus count that already carries the generation discipline.

```rust
// src/keyscope.rs (new module in taskboard-ui-slint)
/// One registered key scope: which context it contributes to the
/// stack, how it arms, and what it does with a stroke it owns.
pub struct KeyScope {
    pub context: ContextId,       // e.g. ContextId::DatePicker
    pub mask: MaskSemantics,      // Masking | Shadowing (must match preset.masks())
    pub arming: ScopeArming,
    pub verbs: ScopeVerbs,
}
pub enum ScopeArming {
    /// True exactly while the view-state flow matches (surface scope).
    Flow(FlowKind),               // Palette, FocusMode, TrackEntries, ReviewWalk, EditDate
    /// True while the shell menu is open.
    MenuOpen,
    /// True while a facts global reports open (widget ladder scope).
    FactsOpen(FactsGlobal),       // SettingsBoard | BoardBar | TimeField | TimerSetter
    /// True while the EditingFacts count is > 0 (element scope).
    FocusCount(FocusCounter),     // TextFocus | ActivationFocus | EditorFocus (new)
    /// Always present (the tail of the stack).
    Always,
}
pub enum ScopeVerbs {
    /// Contributes bindings via the keymap resolver chain (ordinary context).
    Resolve,
    /// Declines the listed stroke classes outright (returns Unhandled
    /// from intercept so the focused widget's own handler wins).
    Decline(DeclineSet),          // arrows+confirm chords per special cases C/D
    /// Routes to the editor command table (the Editor scope).
    EditorTable,
}
```

**Ordering** (round plan §4.7): focused-element scopes (innermost
first) → enclosing surface scopes (innermost first) → view → mode →
global. The assembly function is pure:

```rust
pub fn scope_stack(shell: &KeymapShell, view_state: &ViewState,
                   facts: &FocusFactsSnapshot) -> Vec<ContextId>
```

`KeymapShell::chain` retires; its callers (`intercept`, kbd-hint
lookups) switch to `scope_stack`. `chain`'s unit tests are ported to
`scope_stack` assertions (§4).

**Registration/unregistration lifecycle (explicit ruling):**
- *Surface scopes* (menu, five modals, EditDate's picker): armed by
  `sync_view_context`-mirrored state — the flow's settle/unmount IS
  the unregistration (the audit's "a flow can no longer outlive its
  affordance" invariant carries the stale-scope guarantee).
- *Element scopes*: armed by EditingFacts counts. The grid seam
  (`date_picker_grid`) becomes the `FocusCount`-class arming read at
  intercept (the seam write stays as-is — it feeds `KeyboardState`
  for the journeys' `grid_armed` assertions; do not remove it).
- *Editor scope*: a NEW `EditingFacts.editor-focus-count` (+ the
  same generation latch replica) published by
  `MultiLineEditor`'s `TextInput` exactly like `text-focus-count`.
  The dedup decision chunk 2 reserved: **replicate, don't extract**
  (a shared component still cannot reach another component's
  TextInput; extraction is W5b material). The monopoly predicate
  keys on `text-focus-count` (not the editor count) so every text
  field keeps its printables; the editor *command table* keys on
  `editor-focus-count`.
- *Ladder scopes* (the retired special cases A and D): armed by the
  existing facts globals' `open` properties, read at intercept —
  the popover's close already clears the fact; no new unmount event
  is invented.

**The generation problem, answered**: no scope registration can
outlive its surface because no scope state is stored at all beyond
facts that already reset (counts zeroed + generation bumped at the
adapter reset seams :976-980; facts `open` cleared by their close
seams). Stage 3 cross-checks every EditingFacts reset site against
the new editor counter.

## 3. Decision D-KS2 — the printable monopoly

In `intercept`, immediately after the mode pulls and BEFORE any
decline/resolver step except the re-entrancy guard:

```
monopoly := facts.text_focus_count > 0
         && !stroke.ctrl && !stroke.alt && !stroke.meta
         && is_monopolizable(stroke)
→ return false (Unhandled; the focused field receives the key)
```

`is_monopolopolizable` (a new `Stroke` method, `pub(crate)`):
`self.named_name()` is `None` **or** equals `"Space"`. Rationale:

- aligned with `Stroke::from_event`: control chars never reach here
  (rejected at :154), named keys are exactly the NAMED_KEYS table;
- **Space is printable text** — a typing user's space must reach the
  field even where a scope binds it (vi binds `Space` =
  `TaskComplete` in ViNormal; today insert-mode masking protects it —
  the monopoly must not regress that). Space is the ONE named key
  reclassified as printable;
- the remaining named keys (Return, Escape, Tab, Backspace, Delete,
  Insert, Home/End, PageUp/Down, arrows, F1–F12) are **chord-class**:
  scopes may bind them. This preserves Tab traversal, Return/Escape
  commit/discard in every field, and vi insert's `Escape`→ModeNormal.
- Backspace reaching the field: no preset binds Backspace anywhere;
  the monopoly does not protect it, but nothing consumes it — same
  falls-through-to-native result as today (pinned by a new unit
  test, T-M3).

Chord-class strokes (any of ctrl/alt/meta set, or a named
non-Space key) resolve against the stack **including surface scopes
armed while the field holds focus** — this is F11's construction:
the picker-open DatePicker scope serves C-p/n/f/b to the calendar
while the field keeps every printable.

**Mode pulls unchanged**: the D20 block stays first; under monopoly
the mode is already Insert whenever `text_focus_count > 0`, so the
monopoly and the insert chain agree (an assertion in T-M1).

## 4. The port checklist — exact chain equality

Every state below must produce the IDENTICAL `Vec<ContextId>` from
`scope_stack` as today's `KeymapShell::chain` (a table-driven unit
test, T-S1, enumerates all of them; the two F11 deltas are the only
intentional behavioral diffs and get their own journey):

| # | State | Today's chain | scope_stack |
|---|---|---|---|
| 1 | menu open | `[Menu]` | `[Menu]` |
| 2 | palette flow | `[Palette, ViInsert, Global]` | same |
| 3 | focus overlay | `[FocusMode, ViInsert, Global]` | same |
| 4 | track entries | `[TrackEntries, ViInsert, Global]` | same |
| 5 | review walk | `[ReviewWalk, ViInsert, Global]` | same |
| 6 | insert, grid focus | `[DatePicker, ViInsert, Global]` | same |
| 7 | insert, no grid | `[ViInsert, Global]` | same |
| 8 | normal, grid focus, view V | `[DatePicker, V, ViNormal, Global]` | same |
| 9 | normal, view V | `[V, ViNormal, Global]` | same |

**The two intentional deltas (F11 only):**

- **D-δ1**: while `Flow::EditDate` holds AND the picker is open AND
  the FIELD has focus (grid does not), the DatePicker surface scope
  is armed NON-masking at the chain head:
  `[DatePicker, ViInsert, Global]` — but the monopoly keeps every
  printable with the field. Under vi this also means h/j/k/l while
  typing are the field's (printables), and after Esc/Down (grid
  focus) the grid scope owns them. Note under vi the
  field-focus-phase chain binds nothing new (vi's picker verbs are
  arrows/uniform) — the delta is observable under emacs/standard
  only (C-p/n/f/b move the calendar mid-typing).
- **D-δ2**: while `editor-focus-count > 0`, an `Editor` scope rides
  the head: `[Editor, …unchanged tail…]` (non-masking; its verbs are
  the editor command table, consulted before the resolver, not
  keymap rows).

Everything else — including special cases A/C/D, which become
decline-scope declarations that fire BEFORE the resolver exactly
where they fire today — is behavior-preserving. The C-g, C-x family,
double-C-g, mask, and insert-shadowing pins (§1.7) must pass without
editing a single assertion line.

## 5. Decision D-KS3 — the editor command tables and target seam

**Command model** (`src/keyscope.rs`):

```rust
pub enum EditorCommand {
    CursorBackward, CursorForward,     // C-b / C-f
    LineUp, LineDown,                  // C-p / C-n
    LineStart, LineEnd,                // C-a / C-e
    KillToLineEnd,                     // C-k
    DeleteForward,                     // C-d
    SetMark,                           // C-Space
    CutRegion, CopyRegion,             // C-w / M-w
    Paste,                             // C-y
    Commit,                            // Shift+Return (already native) + C-c C-c
    Discard,                           // C-c C-k (Escape keeps the existing
                                        //  ModeNormal funnel — NOT rebound)
}
pub struct EditorTable(BTreeMap<Vec<Stroke>, EditorCommand>);
```

- **emacs table** (dispatch §8 verbatim): C-p/n/f/b, C-a/e, C-k,
  C-d, C-Space, C-w, Alt+w (M-w), C-y, Shift+Return ≡ Commit (the
  widget's existing seam — the table entry routes to the same
  `accepted` funnel), the prefix pair `C-c C-c` → Commit /
  `C-c C-k` → Discard.
- **vi table** (the stripped normal ladder, editor-local): while the
  editor scope is armed AND the shell mode is Normal (the editor
  keeps its own mode arithmetic OUT of scope — the vi ladder rides
  the shell mode: after the field's Escape → ModeNormal, h/j/k/l/0/
  $/x/etc. drive the editor): `h/l` cursor, `j/k` line, `0/$` line
  edges, `x` delete-forward, `d d` kill-line prefix… **stripped to:
  h, l, j, k, 0, $, x, i (back to insert)** — recorded as the
  deliberate strip; nothing more ships in this chunk (no yank/
  register grammar). Under vi, printables while typing (mode
  Insert) are the field's by the monopoly; the ladder arms only in
  Normal.
- **Commit/Discard semantics** map onto the existing settle funnel:
  Commit ≡ `accepted(buffer)` (the checkmark ≡ Shift+Return path);
  Discard ≡ the flow's discard (same outcome as Escape today —
  reuse the effect, do not invent a new settle).
- **Standard preset**: an empty table (native editing only) —
  second-class-free by the monopoly, not by chords.

**Target seam**: the adapter holds `editor_commands:
Option<EditorTarget>` — `EditorTarget { kind: EditorTargetKind }`
where the single kind today is `Description(task)`; the
`MultiLineEditor` mount that grabs editor scope publishes through a
new window callback `invoke-editor-command(int)` plus two new out
mirrors `selection-anchor: int` / `selection-position: int`
(two-way), so Rust-side commands map to Slint `TextInput` APIs
inside the widget:

- cursor/line/kill/delete/set-mark → the widget applies them to its
  `TextInput` via `set-selection-offsets` (mark = anchor; motion
  commands reposition cursor by selecting anchor==position) and
  text surgery through the existing `edited` mirror;
- cut/copy read `buffer` + the selection mirrors into the adapter
  clipboard slot, then move the cursor;
- paste writes the clipboard slot into the selection.

The widget gains `public function apply-command(code: int)`; the
adapter never touches TextInput internals directly (the L1 boundary
chunk 2 set). Cursor-up/down over wrapped lines is visual-line
based (`TextInput` cursor APIs only give offsets — v1 moves by
logical line via `\n` scan; documented leniency at the command, a
backlog issue records visual-line motion).

**Clipboard** (D4's ruling): one adapter-side slot
`editor_clipboard: Option<String>` — single-slot, no kill-ring;
the leniency is documented at the field. Cut/copy overwrite; paste
replays. No OS clipboard interaction in this chunk (backlog issue;
journeys assert the slot, not the platform).

## 6. Decision D-KS4 — retiring the four special cases

Each retirement is a `ScopeVerbs::Decline`/ladder row whose arming
fact is exactly today's condition; each keeps its journey as the
acceptance pin (run FIRST, stays green through the refactor):

1. **Timer-setter Escape** (adapter.rs:665-670) → `FactsOpen(
   FactsGlobal::TimerSetter)` scope with `Decline(Escape → close)`:
   the decline arm routes to `clear_timer_setter` — the scope table
   maps the declined stroke to the existing dismissal call, not to a
   keymap action. Pin: `the_setter_dismissal_layers_close_without_
   committing`, `the_setter_composes_the_custom_row_grammar`.
2. **Due Down-arm** (adapter.rs:679-693) → the DatePicker scope's
   ladder rule: while `Flow::EditDate` + picker open + field focus,
   bare `Down` performs the existing open/hand-over sequence
   verbatim (the `due_focus_steal`/`FocusHandover::Grid` mechanics
   move into the scope's arm, unchanged). Pin:
   `the_control_walk_reaches_fields_buttons_and_the_picker`,
   `the_due_picker_journeys_reach_the_boundary`.
3. **Activation reservation** (adapter.rs:698-714) →
   `FocusCount(ActivationFocus)` scope, `Decline({bare Return,
   bare Space} → cell)`. NOTE: Space is printable under D-KS2 — the
   monopoly must NOT steal Space from the activation cell: the
   reservation check runs BEFORE the monopoly (an activation stop is
   not a text field; `text-focus-count` is 0 there, so the two rules
   are disjoint by the facts — asserted in T-M4). Pin:
   `the_focused_cell_activates_on_return_and_space_under_any_preset`.
4. **Dropdown ladder decline** (adapter.rs:5928-5968) → two
   `FactsOpen` scopes (SettingsBoard/BoardBar with arrows +
   C-n/C-p + Escape + Return; TimeField with arrows + Escape +
   Return), evaluated inside `intercept` (the closure body
   collapses to guard + `intercept` call). Pin:
   `the_bar_dropdown_ladder_declines_the_action_layer_under_every_
   preset` (all three presets), `the_combobox_is_the_only_time_path`.

After retirement, `intercept` contains ONLY: the re-entrancy guard,
the mode pulls, the monopoly rule, the scope-stack walk (decline
arms and ladder rules), and the resolver feed. Stage 3 grep-verifies
no special-case logic survives outside `keyscope.rs` declarations.

## 7. TDD stage order (stage 2's sequence)

Tests FIRST at each stage; the F11 session journey is written in
stage 0 and stays red until stage 4:

- **Stage 0 — the F11 journey (red)**: `the_picker_shares_chords_
  while_the_field_keeps_printables` — both presets with chords
  (emacs/standard: C-p/n/f/b move the cursor mid-typing; vi: arrows
  move it, h/j/k/l type), both picker phases (default-open
  field-focus and grid-focus after Down), printables insert
  throughout, Escape closes clean. Plus the editor-scope journey
  `the_description_editor_gains_the_preset_command_grammar`
  (emacs mark/cut/copy/paste via the clipboard slot; vi ladder
  after Esc) — red.
- **Stage 1 — `keyscope.rs` skeleton + T-S1 port checklist**: the
  table test of §4 (all nine rows byte-equal) + property test:
  for arbitrary (flow, menu, mode, grid, view) tuples,
  `scope_stack == legacy_chain` except the two declared deltas.
  `chain()` is kept during this stage and deleted at stage 5.
- **Stage 2 — monopoly (T-M1–T-M4)**: unit tests on the predicate
  (printables + Space monopolized; named chord-class keys not;
  control chars impossible — from_event already rejects; vi Space
  under ViNormal while typing reaches the field; activation Return/
  Space disjointness). Wire into `intercept`; all existing journeys
  green.
- **Stage 3 — special-case retirement**: one at a time, each gated
  by its §6 pin journey passing UNCHANGED. Order: D (pure decline),
  A, C, then B (B is the only one moving stateful hand-over
  mechanics). `flow_table_tests` green untouched after each.
- **Stage 4 — editor scope**: EditingFacts `editor-focus-count` +
  the widget's latch replica; the command tables (unit tests per
  preset: table→command mapping, prefix accumulation for C-c
  prefixes through the existing resolver or a local accumulator —
  decision: a LOCAL prefix accumulator in `keyscope.rs`, the
  keymap resolver is not reused for editor commands; documented);
  the `EditorTarget` seam and clipboard slot (fake-target unit
  tests, then the description-mount journey). The
  MultiLineEditor's `key-pressed` Shift+Return arm stays (commit
  ≡ native grammar); the table's Commit entry routes to the same
  `accepted` seam.
- **Stage 5 — delete `KeymapShell::chain`**, port its unit tests to
  `scope_stack`, delete the boolean reconstruction; full gates.

Every commit standalone (AGENTS §8); AGPL header in new files;
typed assertions only (no text matching); no sleeps (poll/deadline
or the harness's `advance_time`).

## 8. Alt/Meta finding (recorded, per the dispatch prompt)

`M-w` maps through `KeyboardModifiers.alt` (adapter.rs:3896 →
`Stroke::from_event`'s chord arm, stroke.rs:160-168 keeps
`alt: true`, shift preserved): on the Linux (winit/femtovg) and
Windows backends the physical Alt key reports `alt=true`, so
`Alt+w` parses and resolves identically to the spelled `"Alt+w"`;
macOS Option reports `alt` (Slint maps Option→alt, Command→meta),
so emacs M- convention = Option on macOS — consistent. **Recorded
limitations**: (a) AltGr on European layouts reports `ctrl+alt` on
some backends — an AltGr-w would surface as `Ctrl+Alt+w`, which the
editor table does not bind (safe: no misfire, but M-w is
unreachable while AltGr is held — a layout-level leniency,
documented at the emacs table); (b) the testing backend synthesizes
strokes directly (`harness.stroke(parse("Alt+w"))`), so the
journeys pin the grammar, not per-backend delivery; stage 3
verifies M-w resolves on the testing backend and records this
finding as the coverage boundary. No code change needed for M-w
itself.

## 9. Risks (honest: the round's highest-risk chunk)

1. **Chain-equality regressions** — nine chain shapes × three
   presets × masking/first-match interactions; a single ordering
   slip breaks the C-g or insert-shadow pins. Mitigated by T-S1's
   exhaustive table + the property test + untouched journey pins.
2. **The monopoly/insert-mask seam** — today ViInsert masking IS
   the printable protection; the monopoly must reproduce its
   coverage exactly (including Space-under-vi and the flow-armed
   insert holds). T-M1–T-M4 pin the seams; the D20 comment's
   "convergence" argument must survive verbatim.
3. **Stale-scope trap (P5-class)** — mitigated by construction
   (§2's pull model: no stored registrations), but the NEW
   `editor-focus-count` is a fresh count with the latch replica —
   a mis-transcribed latch is exactly the P5 shape. The chunk-2
   replica (multi-line-editor.slint:217-250) is the template;
   stage 3 cross-checks every reset seam against it.
4. **The Down-arm state move** (special case B) — the hand-over
   carries one-shot flags (`due_focus_steal`, `focus_handover`)
   with subtle ordering against `refresh_forced`; moving it risks
   the focus-steal/settle funnel (r4 D24). It moves LAST in stage 3
   for this reason.
5. **Slint TextInput selection APIs** — `set-selection-offsets`
   availability/behavior on the bundled Slint version is assumed
   (design §4.7 names it); stage 4 starts with a probe test against
   the real widget — if the API cannot express mark/region, the
   fallback is cursor-only commands + a recorded negative and a
   backlog issue (cut/copy then ship selection-less, à la the
   chunk-2 fence precedent).
6. **Scope creep gravity** — the retirement invites "fixing" the
   ladders (e.g. vi h/j/k/l in the picker field phase). Stage 3
   must reject any diff beyond the two declared deltas.

## 10. Estimated diff

- `src/keyscope.rs` (new): ~450 lines incl. tests.
- `src/adapter.rs`: −260 (four special cases + chain plumbing) /
  +220 (scope wiring, monopoly, editor seam) — net ≈ −40.
- `src/shell.rs`: −70 (chain + its tests; the shell keeps mode/
  menu/flags as scope arming inputs).
- `src/keymap/stroke.rs`: +25 (the monopolizable predicate).
- `src/actions.rs`: +2 ContextId variants (`Editor`, plus none
  else — ladder scopes decline, they don't resolve; TimerSetter/
  SelectorLadder need NO ContextId unless stage 3 finds a resolve-
  shaped one — recorded as open, default no).
- `ui/editing-facts.slint`: +6 (editor-focus-count).
- `ui/widgets/multi-line-editor.slint`: +60 (latch replica,
  apply-command, selection mirrors).
- `tests/journeys.rs`: +350 (two new journeys); unit tests in
  keyscope.rs (+~300 incl. above).
- **Total ≈ 1,100–1,400 lines** across ~10 files. Size L, as
  dispatched.
