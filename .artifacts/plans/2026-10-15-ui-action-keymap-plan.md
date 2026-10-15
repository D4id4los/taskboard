# [Plan] UI Action-Dispatch & Keymap Layer (GUI Phase 7, Chunk 2)

- **Date**: 2026-10-15
- **Kind**: implementation plan (chunk 2, stage 1 of the GUI Phase 7
  dispatch — TDD-first spec for the stage-2 implementation agent). No
  production code in this commit.
- **Origin**: accepted design round
  `.artifacts/plans/2026-10-14-gui-phase7-design-plan.md` (Status:
  ACCEPTED 2026-10-14; normative here: §5 all subsections — pipeline,
  registry/namespace, keymap-as-data, presets, Rust-side module
  decision, Slint integration, conflict detection, reserved actions,
  R8 menu projection; §10 unit/snapshot tier rows; §12 row 2;
  decisions Q1, Q2 (with §16 extraction trigger), Q9, Q14, Q15
  (**redirected** in §16: drop-and-warn, never block start), Q20,
  Q22 keymap half); stress-test report
  `.artifacts/reports/2026-10-14-slint-gui-stress-test-report.md`
  §3.2 (interception seam: root FocusScope + `forward-focus`,
  capture-phase tunneling, `KeyBinding` single-stroke-only caveat);
  dispatch plan
  `.artifacts/plans/2026-10-14-gui-phase7-implementation-dispatch-plan.md`
  §1 (execution model & mandates, binding) and §3 (chunk-2
  covers/acceptance/non-goals — binding).
- **Target**: `taskboard-ui-slint` (action registry, keymap data
  model + presets + validation, resolver, router, menu generation,
  root FocusScope adapter, kbd-hint + action-menu components,
  insta snapshots), `taskboard-app` (`[ui.keymap]` config surface,
  keymap resolution + tracing surfacing, typed hand-over). No other
  crate.
- **Branch / worktree**: `feat/ui-action-keymap` in
  `../taskboard-WT-feat-ui-action-keymap`, off `main` @ `9b79aa1`
  (chunk 1 merged at `20e3ca3`, verified ancestor; `9b79aa1` adds
  only orthogonal sync/attachment work).
- **Goal**: the action-dispatch keybinding layer lands — a pure,
  Slint-free `actions`/`keymap`/`resolver`/`router`/`menu` module set
  with the three presets, Q15 drop-and-warn validation, and the
  generated `app.menu` projection — wired to a single root
  `FocusScope` interception path, with registry/keymap insta snapshots
  as "documentation as a test" and the full pure-module suite
  (unit + proptest) green before any Slint-side code. Every open
  question is settled by a recorded decision (§3); the deviations
  from the design plan are enumerated with reasons (§3, marked
  **[DEV]**).
- **Status**: READY (rev 1 after user review 2026-10-15: D6 numpad
  answer added; D8 emacs/vi tables reworked to source-editor-authentic
  bindings per the review's tmux principle; D9 add-new rule clarified
  and `MaskedContext` semantics defined; D10 mode machinery guards
  removed — mode is ordinary shell state, frankenstein presets work)
  — stage 2 implements exactly this plan; deviations from *this*
  plan need a fresh recorded reason.

---

## 1. Context & verified facts (anchored — do not re-derive)

1. **Chunk 1 is merged** (`main` @ `9b79aa1`, chunk-1 tip
   `20e3ca3`): `crates/taskboard-ui-slint` has the slint 1.18.1
   dependency (+ `unstable-fontique-011`), `ui/theme.slint`
   (`Theme`/`Typography`/`Semantic` globals, `base-scale` root
   multiplier, spacing scale), `ui/app-window.slint` (a bare Window —
   no content, no focus scopes), `mount()`/`run()`/`quit()` with
   `UiSettings { scale }`, `UiHandle { window, _state, _signals,
   _system, _commands }`, bundled fonts, `src/test_platform.rs`
   (`on_slint_thread` — process-global null platform on a dedicated
   worker thread), and `src/token_tests.rs`.
2. **The command boundary exists**: `mount()` receives
   `mpsc::Sender<StateCommand>` (capacity 64, app side); the app's
   forwarder (`crates/taskboard-app/src/ui.rs::
   spawn_ui_command_forwarder`) drains it into `EngineHandle` — a
   `CommandSink` trait makes it hermetically testable. `run_desktop`
   is the lifecycle glue (thin-main-glue class, untested by design);
   `SLINT_SCALE_FACTOR` is observed before any slint call (D13).
3. **`[ui]` config landed**: `crates/taskboard-app/src/config.rs` has
   `UiSection { scale }` with `#[serde(deny_unknown_fields)]`, the
   typed `ConfigError::InvalidScale` with a stable `kind()`
   discriminant, and `unknown_key_in_the_ui_table_is_a_decode_error`
   — the config surface rejects unknown keys structurally. Adding the
   keymap table means extending `UiSection`, not a new top-level
   table.
4. **Slint 1.18.1 key-event facts** (verified against the vendored
   sources, all version-pinned):
   - `KeyEvent` carries `text: string` (one character — printable
     keys arrive as their literal char; special keys as private-use
     chars) + `modifiers: KeyboardModifiers { alt, control, shift,
     meta }` (bools). `FocusScope` exposes
     `capture-key-pressed(event) -> EventResult`,
     `key-pressed(event) -> EventResult`, focus callbacks,
     `focus()`/`clear-focus()`, `enabled`, click/tab focus flags.
   - **Key Event Delivery** (compiler `builtin_elements.rs`,
     "Key Event Delivery"): before delivery to the focused element,
     `capture-key-pressed` is called Window→downward (tunneling);
     any `accept` **stops** delivery there. Otherwise the focused
     element's `key-pressed` fires, and rejected events walk
     **upward** to parents.
   - The `Key` enum is **char-based** (`i-slint-common/key_codes.rs`):
     printables are their literal char; named specials map to chars
     (Backspace `\u{8}`, Tab `\u{9}`, Return `\u{a}`, Escape `\u{1b}`,
     Delete `\u{7f}`, Space `\u{20}`, arrows `\u{F700}`–`\u{F703}`,
     F1–F12 `\u{F704}`–`\u{F70F}`, Insert/Home/End/PageUp/PageDown
     `\u{F727}`/`\u{F729}`/`\u{F72B}`/`\u{F72C}`/`\u{F72D}`).
   - The winit backend maps keys via `logical_key`
     (`to_slint_key`): `Key::Character(str)` passes through verbatim,
     so **Ctrl+J delivers text `"j"` + ctrl=true** (no legacy
     control-char collision on our dev platform) and **Shift+J
     delivers text `"J"`** (case carries the shift for printables).
     (Windows AltGr handling exists backend-side; not our concern.)
   - `KeyBinding` (declarative, single-stroke, compile-time keys,
     duplicate-diagnosis at compile time) exists but is **not used**
     for any rebindable action (design Q9).
   - `slint::Window::dispatch_event(WindowEvent::KeyPressed { text })
    ` is **public** — key events can be injected into a mounted
     window on the null platform, which is how chunk 2's adapter
     tests drive the interception path end-to-end without
     `i-slint-backend-testing` (that harness is chunk 4's).
5. **insta + proptest are workspace dependencies** (`insta` with the
   `yaml` feature, `proptest` 1); the state crate's convention is a
   `tests/` integration file using `assert_yaml_snapshot!` with
   scenario-unique names, snapshots in `tests/__snapshots__/`.
6. **`StateCommand` vocabulary** (domain `messages.rs`): the task
   verbs map cleanly — `SetTaskDone { id, done }`,
   `DeleteTask { id }`, `MoveTask { id, stack, order }`,
   `CreateTask { title, stack, order }`, `UpdateTask { id, changes }`.
   All need entity ids from UI-side focus state; none are
   context-free.
7. **Workspace hygiene**: UI crate is AGPL-3.0-only, no
   `forbid(unsafe_code)` (Slint exception), lints workspace-inherited;
   every commit must stand alone (rebase merges); full gate suite per
   AGENTS §10 before handoff.

## 2. Scope

**In** (dispatch §3 chunk-2 covers, verbatim mapping):

- `actions` module — `ActionId`, `ContextId`, categories, the const
  registry, reserved actions disabled (§5.2, §5.8, Q20).
- `keymap` module — stroke parsing, sequence encoding, the three
  preset tables, preset ⊕ override merge, typed-findings validation
  with Q15 drop-and-warn semantics (§5.3, §5.7, Q15, Q14).
- `resolver` — pure prefix-accumulation state machine over
  (stroke, context-chain) events; masking; prefix-abort + status
  signal (§5, §5.7).
- `router` + shell state — ActionId → ui-internal effect |
  `StateCommand` | disabled; the vi mode state; the focused-entity
  seam (§5 pipeline).
- Slint adapter — root `FocusScope` + `forward-focus`, capture-phase
  interception, `KeyBinding` unused (§5.6, Q9), applied one-way at
  UI start.
- App side — `[ui.keymap]` config surface (`preset` + `overrides`,
  Q22), resolution + `tracing` surfacing, typed hand-over into the UI
  crate.
- `app.menu` generated menu + `kbd-hint` primitive seed (§5.9, §4.4).
- Insta snapshots: registry dump + resolved keymaps (§10).
- Test catalogue: unit (parse/merge/validate), proptest (resolver
  sequences incl. chords/prefixes/modes/masking), preset coverage
  over the core verbs, preset conflict-freeness.

**Out** (non-goals, dispatch §3 verbatim + clarifications):

- No views beyond the menu projection; no focus architecture beyond
  the root scope (chunk 3 owns both — the `ContextId` variants
  `Board`/`StackDetail`/`TaskDetail` exist as *data* now; nothing
  drives them).
- No conflict-resolution UI, no settings page, no live re-map during
  a session (POST-1.0; §5.6).
- No new config surface beyond Q22's keymap half.
- No `i-slint-backend-testing` journeys (chunk 4); no status bar
  rendering of the feedback/warning state (chunk 3 reads the state
  this chunk pushes).
- No backlog edits (stage 3's shipping task; the [2026-10-09]
  forwarder bookkeeping already landed with chunk 1's report).

## 3. Decisions (each binding for stage 2)

Deviations from the accepted design plan carry a **[DEV]** marker
with the reason; everything else implements the design verbatim or
settles a point the design left open.

### D1 — `ActionId` is a field-less enum; the registry is a const table

`ActionId`: field-less `enum` with one variant per v0.1.0 action
(D2). The registry is a `const REGISTRY: &[ActionEntry]` with
`ActionEntry { id, label: &'static str, contexts: &'static [ContextId],
category: Category, enabled: bool }`. String ↔ enum conversion
(for config override keys and the menu model) goes through the
registry table — an unknown string is a typed `UnknownAction`
finding, and the router's `match` is exhaustive, so a new action
cannot be added without touching every dispatch site (the
compiler-checked alternative to a string newtype's open set).

### D2 — The v0.1.0 registry table (ids, labels, contexts, categories, enabled)

Contexts (`ContextId`, closed enum): `Global`, `Board`,
`StackDetail`, `TaskDetail`, `Menu`, `ViNormal`, `ViInsert`.
Registry `contexts` = **menu-availability contexts** (where the
generated menu offers the action): `Global`-tagged actions appear in
every context's menu; `Board`/`StackDetail`/`TaskDetail` are the
view contexts chunk 3 drives. `ViNormal`/`ViInsert` are **binding
contexts only** (the vi preset binds verbs there, D10) — no registry
row lists them, because menu availability is a *view* question
("is there something to act on?"), while mode contexts are chain
members that carry bindings (rev 1: availability lists view contexts
only, so a view-less window never offers task verbs — under any
preset). *Binding* contexts (which context a preset binds an action
in) are a preset-table concern, not a registry concern — except the
add-new-override rule (D9).

| id | label | contexts | category | enabled @ chunk 2 |
|---|---|---|---|---|
| `app.exit` | Quit taskboard | Global | Application | **yes** |
| `app.menu` | Command menu | Global | Application | **yes** |
| `app.undo` | Undo | Global | Application | no — reserved (D4) |
| `app.redo` | Redo | Global | Application | no — reserved (D4) |
| `app.settings` | Open settings | Global | Application | no — reserved (D4) |
| `app.help` | Open help | Global | Application | no — reserved (D4) |
| `capture.quick` | Quick capture | Global | Capture | no — reserved (D4) |
| `palette.search` | Search | Global | Search | no — reserved (D4) |
| `nav.next-view` | Next view | Global | Navigation | no — chunk 3 (D5) |
| `nav.prev-view` | Previous view | Global | Navigation | no — chunk 3 (D5) |
| `nav.focus-board` | Go to board | Global | Navigation | no — chunk 3 (D5) |
| `nav.next-item` | Focus next item | Board, StackDetail, TaskDetail | Navigation | no — chunk 3 (D5) |
| `nav.prev-item` | Focus previous item | Board, StackDetail, TaskDetail | Navigation | no — chunk 3 (D5) |
| `task.create` | New task | Board, StackDetail | Task | no — chunk 3 (D5) |
| `task.edit` | Edit task | Board, TaskDetail | Task | no — chunk 3 (D5) |
| `task.complete` | Complete task | Board, TaskDetail | Task | **yes** (D16) |
| `task.move` | Move task | Board, TaskDetail | Task | no — chunk 3 (D5) |
| `task.delete` | Delete task | Board, TaskDetail | Task | **yes** (D16) |
| `menu.next` | Next menu entry | Menu | Menu | **yes** |
| `menu.previous` | Previous menu entry | Menu | Menu | **yes** |
| `menu.invoke` | Invoke menu entry | Menu | Menu | **yes** |
| `menu.dismiss` | Dismiss menu | Menu | Menu | **yes** |
| `mode.insert` | Enter insert mode | Global | Mode | **yes** |
| `mode.normal` | Return to normal mode | Global | Mode | **yes** |

`enabled` means "dispatching performs the action's effect in this
build" — reserved actions dispatch a disabled no-op and **read as
disabled in the dumped registry** (§5.8: honest, keeps menu
generation total). The registry is data; chunk 3 flips the chunk-3
rows in this same table when the views land.

**[DEV] D3 — two scopes beyond the design's four: `menu.*` and
`mode.*`.** Design §5.2 names "four top-level scopes" (and already
counts `capture.quick`/`palette.search` as extra top-level ids).
This chunk adds: (a) `menu.*` — the generated menu must be
keyboard-completable through the action layer (§7.3 step 8: open the
menu, invoke an entry **by keyboard**), and a key handler inside the
menu composition would violate §5.1's single-key-path rule, so menu
cursor movement is four context-scoped actions; (b) `mode.*` — the
vi mode-flip actions (§5.4 names them) need registry ids to be
bindable, menu-visible, and kbd-hintable. Both are one-line registry
rows; retrofit cost (the brief's "extreme") is avoided. This is the
recorded reason; if the session objects, folding `menu.*` under
`nav.*` is the fallback (mechanical rename, no semantics change).

### D4 — Reserved globals: registry rows now, no-op dispatch (Q20 verbatim)

`app.undo`, `app.redo` (D4 undo round), `capture.quick` (W1 inbox,
ADHD P1), `palette.search` (W1 fuzzy search), `app.settings`
(POST-1.0 view), `app.help` (R10) — all present with labels, all
`enabled: false`. Presets still **bind** them (D8): a disabled
action with a visible binding in the menu documents the reservation
("Undo  Ctrl+Z — disabled" is honest design documentation).

### D5 — Staged `enabled` bits for chunk-3 handlers

`nav.*`, `task.create`, `task.edit`, `task.move` are implemented by
chunk 3's views (create/edit need text-entry flows; move needs a
target-chooser flow; nav needs the views to switch). They ship in
the registry + presets **bound but disabled**; chunk 3 flips the
bits. Exception — D16.

### D6 — The stroke grammar (the R5 core)

A **stroke** = at most one non-modifier key plus modifier state.

```text
stroke    := key | mod-list key
mod-list  := mod "+" (accepted in any order; canonical display
             order Ctrl, Alt, Shift, Meta; duplicates malformed)
mod       := "Ctrl" | "Alt" | "Shift" | "Meta"
key       := printable | named
printable := a single Unicode scalar, excluding control chars and
             ' ' — CASE-SENSITIVE ("j" ≠ "J", "/" , "," , "." , ":")
named     := "Space" | "Return" | "Escape" | "Tab" | "Backspace" |
             "Delete" | "Insert" | "Home" | "End" | "PageUp" |
             "PageDown" | "Up" | "Down" | "Left" | "Right" |
             "F1" … "F12"
sequence  := stroke ("," stroke)*     — in TOML: array of strings
```

Rules:

- **Shift-folding normalization**: for a *printable* key, shift is
  expressed by the character's case/spelling and the `Shift`
  modifier is normalized away — `"Shift+A"` ≡ `"A"` ≡ the event
  text `"A"`. For *named* keys, `Shift` is a real modifier
  (`"Shift+Up"` ≠ `"Up"`). This matches how Slint delivers events
  (fact §1.4: printables arrive case-carrying, so `j` and `J` are
  distinct keys and vi's `j`/`J` split works by construction).
- `Ctrl`/`Alt`/`Meta` are boolean modifiers from
  `KeyboardModifiers` (`control`/`alt`/`meta`); note the documented
  macOS mapping (`control` = ⌘) — a platform-tuning concern recorded
  for the release chunk, not a grammar concern.
- A modifier-only stroke (`"Ctrl"`, `"Shift"`), an empty string, an
  unknown token, or a control char is **malformed** (typed
  `MalformedStroke` finding at validation; parse fn returns
  `Result<Stroke, StrokeParseError>` — no panics on user input).
- Internally: `Stroke { key: char, ctrl: bool, alt: bool, meta:
  bool, shift: bool }` (post-normalization) with the named-key
  chars mapped per fact §1.4. `Display` renders the canonical form
  (mods in canonical order + key) — the kbd-hint text source.
  `parse`/`Display` round-trip idempotently (proptest D22).
- **Numpad (review question, rev 1)**: there are no numpad spellings
  in the grammar because Slint 1.18.1 cannot distinguish them — key
  events carry only the logical `text` char, and the winit adapter
  uses `KeyLocation` solely for left/right modifier disambiguation
  (verified: no keypad entries in `i-slint-common/key_codes.rs`).
  With NumLock on, numpad digits deliver the same logical char as
  the top row (`"1"`), so a `"1"` binding fires for both; with
  NumLock off, the numpad delivers the named arrows/Home/End keys.
  **Additions are mechanical**: a named key is one grammar spelling
  + one enum variant + one char-mapping row; Slint's `Key` is
  `#[non_exhaustive]`, so upstream additions don't break us either.

### D7 — Sequence encoding: array of stroke strings; bare-string sugar

In TOML, a binding is an **array** of stroke strings
(`["Ctrl+x", "Ctrl+c"]`, design §5.3's example); a bare string
(`"Ctrl+Q"`) is accepted as the single-stroke degenerate case
(serde untagged `String | Vec<String>`, normalized to `Vec`). A
chord like `C-x C-c` is a two-stroke **sequence** — no special
syntax (design §5.3 verbatim). Internally `Binding = Vec<Stroke>`
(non-empty, else malformed). Multiple sequences may map to the same
action in the same context (**alternates**, e.g. `Escape` *and*
`Ctrl+G` for `menu.dismiss` in emacs) — the map shape gives this
for free.

### D8 — The three preset tables (Q14; strokes session-tunable, contracts enforced by test)

Each preset is a const table of `(ActionId, ContextId, sequence)`
rows plus per-context `masks_below` flags (D11). The **enforced
contracts** (D21) are: every core verb bound in every preset (the
coverage list below), intra-context conflict-freeness, and
prefix-shadow-freeness (the `PrefixShadow` finding of D13); the
exact strokes below are a starting draft the session/review may
retune freely (tuning re-runs only the contract tests and the
keymap snapshots).

**Binding-design principle (review round 1, the tmux standard)**:
wherever a source editor has a convention, a preset binding matches
what that binding *means* in the editor — exactly where the default
is famous (`C-x C-c`, `C-h`, `M-x`, `u`, `Ctrl+r`, `ZQ`, `/`), in
spirit where our domain has no editor analog (`C-c c` capture =
org-capture's canonical binding, not an invented `C-x y`). Prefix
families are used the way the editor uses them (`C-x` = emacs's
file/buffer/window prefix; `C-c` = emacs's mode/user prefix;
`g`/`d`/`Z` = vim's goto/delete/quit prefixes) — never as a
one-size-fits-all app prefix. Directional navigation prefers the
editor's home-row motions over arrow keys (emacs `C-n`/`C-p`; vim
`j`/`k`) — arrows appear only in the CUA standard preset, where
they *are* the convention.

**Core-verb coverage list** (const in the coverage test; §7.3 verbs
+ undo/redo/capture/menu, dispatch §3): `app.undo`, `app.redo`,
`app.menu`, `app.exit`, `capture.quick`, `nav.next-item`,
`nav.prev-item`, `nav.next-view`, `nav.prev-view`, `task.create`,
`task.edit`, `task.complete`, `task.move`, `task.delete`,
`menu.next`, `menu.previous`, `menu.invoke`, `menu.dismiss` — plus,
for the vi preset only, `mode.insert` and `mode.normal` (§7.3-8
requires the keyboard-complete menu in *every* preset, hence the
`menu.*` rows in the list; `palette.search` is deliberately not
core — it is W1-reserved).

Draft tables (tunable):

- **standard** (CUA — arrows *are* this preset's convention) —
  Global: `app.undo` Ctrl+Z, `app.redo` Ctrl+Shift+Z, `app.menu`
  Ctrl+Shift+M, `app.exit` Ctrl+Q, `app.settings` Ctrl+`,` ,
  `app.help` F1, `capture.quick` Ctrl+K, `palette.search` Ctrl+F,
  `nav.next-view` Ctrl+PageDown, `nav.prev-view` Ctrl+PageUp,
  `nav.focus-board` Alt+1. Board: `nav.next-item` Down,
  `nav.prev-item` Up, `task.complete` Space, `task.create` N,
  `task.edit` Return, `task.move` M, `task.delete` Delete.
  StackDetail: `nav.next-item`/`prev-item` Down/Up, `task.create`
  N. TaskDetail: `nav.next-item`/`prev-item` Down/Up. Menu:
  `menu.next` Down, `menu.previous` Up, `menu.invoke` Return,
  `menu.dismiss` Escape.
- **emacs** (rev 1 rework: default-emacs-authentic; the review's
  examples — C-x C-c exit, C-h help — kept; C-x o/C-x b re-homed to
  commands that match their emacs meanings) — Global: `app.exit`
  [Ctrl+x, Ctrl+c] (= `save-buffers-kill-terminal`, exact),
  `app.undo` [Ctrl+x, u] (= `undo`, exact), `app.redo` [Ctrl+g,
  Ctrl+/] (= Emacs 28's `undo-redo`, exact), `app.menu` Alt+x
  (= `M-x` `execute-extended-command` — the command menu *is*
  emacs's command surface), `app.help` Ctrl+h (= the help prefix,
  exact; becomes a sequence family if help pages arrive),
  `app.settings` [Ctrl+x, c] (mnemonic `M-x customize`; `C-x c` is
  unbound in default emacs), `capture.quick` [Ctrl+c, c]
  (org-capture's canonical binding — `C-c` is emacs's mode/user
  prefix, our domain verbs live under it), `palette.search` Ctrl+s
  (= `isearch-forward`, exact), `nav.next-view` [Ctrl+x, ]] and
  `nav.prev-view` [Ctrl+x, [] (= `forward-page`/`backward-page` —
  pages ≈ views, exact bindings), `nav.focus-board` [Ctrl+x,
  Ctrl+b] (= `list-buffers`, the everything-overview — the board is
  ours). Board: `nav.next-item` Ctrl+n, `nav.prev-item` Ctrl+p
  (= `next-line`/`previous-line`, exact), `task.complete` [Ctrl+c,
  Ctrl+t] (= org-mode's `org-todo`, exact), `task.create` Ctrl+o
  (= `open-line` — a new entry below point), `task.edit` Return
  (= dired's RET — open the item at point), `task.move` [Ctrl+c,
  Ctrl+w] (= org-mode's `org-refile`, exact), `task.delete` Ctrl+d
  (= `delete-char` — delete at point). StackDetail:
  `nav.next-item`/`prev-item` Ctrl+n/Ctrl+p, `task.create` Ctrl+o.
  TaskDetail: same nav pair. Menu: `menu.next` Ctrl+n,
  `menu.previous` Ctrl+p, `menu.invoke` Return, `menu.dismiss`
  Ctrl+g (= `keyboard-quit`, exact) + Escape (cross-preset
  alternate).
- **vi** (rev 1 rework: vim-authentic) — **ViNormal carries
  everything; Global is empty** — the honest vim model: normal-mode
  keys mean nothing in insert mode, so nothing is bound where both
  modes would see it (in insert mode, as in vim, only Escape acts).
  ViNormal: `nav.next-item` j, `nav.prev-item` k (= line motions,
  exact), `nav.next-view` [g, t], `nav.prev-view` [g, T] (= vim's
  `gt`/`gT` tab switching, exact — views ≈ tabs), `nav.focus-board`
  [g, g] (= `gg` goto-top — the board is the root view),
  `task.complete` Space (the checkbox-toggle convention; vim leaves
  Space free), `task.create` o (= `open-line`-below, exact),
  `task.edit` Return (open the thing at point), `task.delete`
  [d, d] (= `dd`, exact — and our prefix accumulation reproduces
  vim's operator-waits-for-motion behaviour for free), `task.move`
  [g, m] (`gm` is an obscure middle-of-line motion in vanilla vim;
  the `g` goto-family reads "go move" — the one invented binding,
  refile has no vanilla vim key), `mode.insert` i and a (=
  `insert`/`append`, exact), `app.undo` u, `app.redo` Ctrl+r
  (exact), `app.exit` [Z, Q] (= `ZQ` quit-without-saving, exact),
  `app.settings` [Z, s] and `capture.quick` [Z, c] (the `Z`
  prefix's free letters — vim only binds `ZZ`/`ZQ`), `app.menu` ":"
  (= vim's command line: type-to-select a command — our menu is its
  wildmenu), `palette.search` "/" (= exact), `app.help` F1.
  ViInsert: `mode.normal` Escape — the **only** insert binding.
  Menu: `menu.next` j, `menu.previous` k, `menu.invoke` Return,
  `menu.dismiss` Escape. (Design §6.4's "vi J/K friendly" compact
  stack switcher: when chunk 3's compact board exists it binds
  `J`/`K` as `Board`-context bindings of the nav actions — `J` =
  vim's join is deliberately left unused here so it stays free.)

Masking declarations (D11) ship with every preset: `Menu` masks
below; `ViInsert` does **not** (shadow only).

### D9 — Merge semantics: flat per-action overrides (design §5.3's own shape)

`overrides` is a flat map `ActionId-string → sequence(s)`. An
override **replaces every binding of that action in every context**
(the user says "task.complete is now X", everywhere it was bound;
the binding *contexts* are inherited from the preset). This first
rule already covers every action the preset binds. The one case it
cannot cover is an action the preset leaves **unbound** — there is
nothing to replace — and the design's upgrade model requires such
overrides to *work*, not to be rejected: "new actions in an upgrade
simply appear (unbound unless the preset binds them)" means a user
must be able to bind them from their delta file. So an override on
a preset-unbound action **adds** a binding in `Global` — and this
is where `MaskedContext` earns its keep: if the action's registry
availability does not include `Global`, the added binding can never
fire in any context it would land in (a `menu.*` cursor action
bound outside the masked `Menu` context, say — cursor movement with
no menu open), and a config entry that does nothing is dropped with
the finding instead of being silently inert. Semantically,
`MaskedContext` says exactly that: *"this override can never fire
where the merge would place it"* — the same never-silently-no-op
spirit as `UnknownAction`, applied to placement instead of identity.
Overrides never touch masking declarations or add contexts. Because
the user file carries only deltas, preset upgrades never clobber
customisations (design §5.3's merge-on-upgrade resolution,
verbatim).

### D10 — vi modes: contexts + ordinary shell state, zero special machinery (rev 1)

Modes are keymap **contexts** (design §5.4 verbatim): the vi preset
binds its verbs in `ViNormal` and only `mode.normal` in `ViInsert`.
The mode **state** is two plain fields of shell state (D17) —
`mode: Mode` (initially `Normal`, the vi convention; untouched
while the menu is open, which masks) — and the chain **always**
contains the current mode context (D11): an empty `ViNormal`/
`ViInsert` contributes no bindings and therefore no behaviour, so
mode only *matters* when the keymap binds something there. There is
deliberately **no activation guard, no derived "uses vi modes"
flag, and no registry restriction on `mode.*`** (rev 1: the review
rejected them): `mode.insert`/`mode.normal` are ordinary
ui-internal actions, globally available in the registry (D2),
whose dispatch effect is `UiEffect::SetMode(Mode)` applied by the
shell — exactly the design §5 pipeline ("ui-internal actions → …
mode flips"). A user who binds `mode.insert` in the emacs preset
gets a working — if frankenstein — mode layer for free: the chain
gains `ViInsert`, their bindings there shadow, and nothing else
changes. Building the guard would have been extra effort for no
gain; the resolver never learns that modes exist at all (D12).

### D11 — Context chains, shadowing, and masking-as-data

Resolution walks an ordered **context chain**, first match wins; a
higher context's binding *shadows* a lower context's same-sequence
binding (legitimate — that is the point of contexts). A context may
declare `masks_below` in the keymap data (design §5.2: masking is
declared in the data, not hardcoded): when the chain walk *reaches*
a masking context, lower contexts are not consulted at all, even
for strokes the masking context does not bind. Chain construction
(shell state, D17):

- menu open → `[Menu]` (pure modal: `Menu` masks below; even
  `app.exit` waits until dismissal — modal discipline, and the one
  shipped use of masking in v0.1.0);
- else → `[view-context?]`, `[ViNormal | ViInsert — always the
  current mode, no activation flag (D10)]`, `[Global]` — `ViInsert`
  shadows (its single Escape binding beats lower contexts' Escape)
  but does not mask, so Ctrl-modified globals keep working while
  typing (P9-forgiving); with no vi-context bindings in the keymap
  the mode context is simply an empty set member and changes
  nothing.

### D12 — Resolver: a pure prefix-accumulation state machine

`KeymapResolver::new(&Keymap)` holds only the pending prefix (+
the chain it accumulated under). `feed(stroke, chain) -> Resolution`
where `Resolution ∈ { Action(ActionId), Pending, Aborted,
Unhandled }`:

1. Extend the pending sequence with the stroke (if the stroke
   arrives under a **different chain** than the pending was
   accumulated under, implicitly reset first — deterministic under
   context/mode flips mid-sequence; the adapter additionally resets
   on focus/context changes).
2. If any visible binding's sequence **equals** the pending
   sequence → `Action` (highest context wins; same-context
   duplicates are impossible — conflict-freeness), pending clears.
3. Else if any visible binding's sequence **starts with** the
   pending sequence → `Pending` (the stroke is consumed; a pending
   indicator is the D18 feedback).
4. Else if pending was non-empty → `Aborted` — **prefix-abort is a
   defined transition**, the design's status signal (§5.7): the
   adapter traces it debug-level and pushes the `PrefixAbort`
   feedback (D18); pending clears. No timeout on prefixes in
   v0.1.0 (emacs-style; recorded — a timeout is a settings-view
   feature).
5. Else → `Unhandled` (nothing anywhere in the chain matches this
   stroke even as a prefix) — the adapter **rejects** the event so
   Slint continues delivery (Tab keeps doing focus traversal).

Visibility = the chain with `masks_below` applied (D11). No
`tokio::time`, no IO, no slint types — proptestable (D22).

### D13 — Validation: typed findings, drop-and-warn, preset fallback (Q15 redirect verbatim)

`keymap::build(raw: &RawKeymap) -> BuiltKeymap { keymap: Keymap,
findings: Vec<Finding> }` — pure, total, never errors. `Finding {
kind: FindingKind, action: Option<ActionId>, context: ContextId,
stroke: String }`; `FindingKind ∈ { UnknownAction, MalformedStroke,
MaskedContext, Conflict, PrefixShadow }` — typed enum values only;
**no text is ever matched in tests** (AGENTS §5).

- The first four kinds **drop the offending override** and the
  action's preset bindings survive **verbatim** ("the preset
  binding wins for every dropped action" — assert the fallback, not
  just the drop): `UnknownAction` (id not in registry),
  `MalformedStroke` (D6), `MaskedContext` (D9's never-fires rule),
  `Conflict` (the override's sequence collides intra-context with
  another effective binding; user-vs-preset → the override drops;
  user-vs-user → **both** drop, preset wins for both — deterministic,
  no last-wins).
- `PrefixShadow` (§5.7: a same-context binding that is a strict
  prefix of another) is computed on the **effective** map, is a
  **warning only** — nothing is dropped (usable, just lossy) — and
  the shipped presets are additionally asserted shadow-free (D21).
- Findings are **exhaustive**: every offender is listed, one pass.
- A hostile keymap **never blocks UI start** (Q15): `build` cannot
  fail; the app logs each finding (`tracing::warn!`, structured
  fields: kind/action/context/stroke — surfacing, not asserting)
  and hands the keymap + a `StartupWarnings { keymap_findings: u32 }`
  summary to the UI (D18), which chunk 3's status bar renders. The
  end-goal settings-page journey is POST-1.0 (recorded design
  target, not built).

**Boundary — what still fails fast**: structural config errors stay
config errors (chunk-1 class): an unknown `preset` value or a
wrongly-typed `overrides` entry is a typed `ConfigError::Decode`
(not a finding) — the line is *structure vs semantics*: serde-shape
problems are the config layer's fail-fast contract (like
`InvalidScale`), value-level keymap problems are findings. An
unknown preset has no meaningful "drop" (silently substituting
`standard` would ignore the user's intent wholesale); Q15's redirect
names *overrides* as the drop-and-warn class. **[DEV]** (a reading
the session should confirm): this keeps one config typo blocking
start — consistent with every other config field, and the findings
class covers everything the design listed in §5.3.

### D14 — `[ui.keymap]` config surface (Q22) and the typed hand-over

```toml
[ui]
scale = 1.0
[ui.keymap]
preset = "standard"          # Standard | Emacs | Vi (serde enum)
[ui.keymap.overrides]
"task.complete" = ["Ctrl+Space"]
"app.exit" = "Ctrl+Q"        # bare-string sugar, D7
```

`UiSection` grows `keymap: KeymapSection { preset: KeymapPreset
(default Standard), overrides: BTreeMap<String, OverrideSpec> }`
(serde defaults keep old configs valid; `deny_unknown_fields`
stays). The config layer stores **raw** strings (structure only).
Resolution happens at UI start in `taskboard-app` (design §5.3:
"validation at load, app-side, where figment lives"; the UI crate
stays IO-free): a small pure step
`ui::resolve_ui_settings(&UiSection) -> (UiSettings, Vec<Finding>)`
(unit-tested without slint) calls `keymap::build`, and
`run_desktop` logs the findings and passes
`UiSettings { scale, keymap, startup }` to `mount`. (`UiSettings`
loses `Copy`, gains `Clone` — mount takes it by value already.)

### D15 — Keymap data model

`Keymap { bindings: BTreeMap<ContextId, BTreeMap<Vec<Stroke>,
ActionId>>, masks: ContextId-set of masking contexts }` (rev 1: no
derived `uses_vi_modes()` — mode activation is not a keymap
property, D10). `BTreeMap` ordering makes snapshots and the
menu's first-binding display deterministic. Alternates (D7) are two
entries mapping to one action. Lookup helpers: visible bindings for
a chain, bindings of an action in a context (kbd-hint source).

### D16 — Router: exhaustive, pure, with a focused-entity seam

`ActionRouter::dispatch(id: ActionId, ctx: &DispatchCtx) ->
DispatchOutcome`, pure (no slint, no channels):

- `DispatchOutcome ∈ { Command(StateCommand), UiEffect(UiEffect),
  Disabled, NoTarget }`.
- `DispatchCtx { focused_task: Option<TaskId>, focused_stack:
  Option<StackId> }` — the **chunk-3 focus seam**: chunk 2's shell
  has no views, so it is `None` in real sessions (keyboard task
  verbs resolve to `NoTarget`, a harmless absorbed no-op) and is
  set through a `UiHandle::set_focused_entities(..)` test/mount API
  that chunk 3's focus architecture will drive for real. This is a
  seam, not a selection subsystem.
- Mappings in chunk 2: `app.exit` → `UiEffect::Exit`;
  `app.menu` → `UiEffect::OpenMenu`; `menu.next`/`menu.previous` →
  cursor effects; `menu.invoke` → handled by the adapter (close,
  then one re-dispatch of the selected entry — bounded: the menu is
  closed before the re-dispatch, no recursion); `menu.dismiss` →
  `CloseMenu`; `mode.*` → `SetMode`; **`task.complete` →
  `SetTaskDone { id, done: true }` and `task.delete` →
  `DeleteTask { id }` are enabled now** — their mapping is complete
  without views (focused task suffices), which gives chunk 2 its
  end-to-end "keys → resolver → router → mpsc → forwarder" path
  (dispatch §1.2: "chunk 2's resolver dispatches real commands
  through chunk 1's forwarder"); `task.create`/`task.edit`/
  `task.move`/`nav.*` stay disabled (D5 — their flows are views);
  registry `enabled: false` → `Disabled` (before any target check).
- The adapter sends `Command` outcomes with `try_send`
  (non-blocking, human-input-scale capacity 64; a drop logs — the
  forwarder convention) and applies `UiEffect`s to the shell state.

### D17 — Shell state (pure Rust, adapter-adjacent)

`KeymapShell { mode: Mode, menu: MenuState, view: Option<ContextId>,
focused: DispatchCtx }` — no slint types; owns chain construction
(D11), effect application (menu open/close/cursor, mode flips,
exit→`quit()`), and the resets on context change (D12 rule 1).
Chunk 2's real sessions run with `view: None` (no views exist), so
both driver inputs are settable from outside the crate through
`UiHandle`: `set_view_context(Option<ContextId>)` (chunk 3's views
own this for real) and `set_focused_entities(..)` (D16) — the
component tests use them to build a Board-context chain with a
focused task before injecting strokes. The slint glue (lib.rs
adapter) syncs shell state ↔ window properties and is the only
code that touches both worlds.

### D18 — Slint adapter: single key path, capture-phase (§5.6, Q9)

`ui/app-window.slint` grows the root interception scope (the only
key-event handler in the tree, §5.1):

```slint
forward-focus: key-root;
key-root := FocusScope {
    capture-key-pressed(event) => {
        root.intercept-key(event.text, event.modifiers) ? accept : reject
    }
}
```

- `intercept-key(text: string, modifiers: KeyboardModifiers) ->
  bool` is a Rust-installed callback — the **only** place slint
  types meet keymap types, converting `(text, modifiers)` →
  `Stroke` (D6 normalization) and returning accept iff the resolver
  consumed the stroke (`Action`/`Pending`/`Aborted`); `Unhandled`
  rejects so delivery continues (Tab traversal survives).
  Modifier-only events (text ∈ the modifier control chars) are
  rejected outright.
- **Capture-phase semantics, interpreted and recorded**: capture
  fires *before* the focused element, so "only for events text
  fields do not consume" (design §5.1) is realized by **chain
  discipline**, not by the phase: the resolver consumes a stroke
  only when the active chain resolves it, and when a text input
  holds focus the chain must be a text context (`ViInsert`/
  `TaskDetail`-class) in which printable strokes do not resolve —
  typing passes through untouched. Chunk 2 contains the risk: no
  `TextInput` exists in the window yet (views are chunk 3); the
  contract above is recorded as chunk 3's focus-architecture
  obligation (its review checks it). This is the design's intent
  under the documented delivery order (fact §1.4), not a behaviour
  change.
- `KeyBinding` is **unused** for anything (Q9) — the review greps
  for it; root-scope interception is the single path.
- The keymap is applied one-way at UI start (§5.6): `mount` builds
  the resolver from `UiSettings.keymap`; no live re-map machinery.
- Window-state surface pushed for chunk 3 (all one-way out):
  `global KeyboardState { vi-mode, menu-open, menu-selected-index,
  last-feedback: KeyFeedback { none, prefix-pending, prefix-abort } }`
  (the prefix-pending indicator and the §5.7 status signal) and
  `global UiStatus { startup-keymap-findings: int }` (the Q15
  startup-warning state the status bar will render) — pushed from
  shell state; nothing renders them in chunk 2.

### D19 — `app.menu` projection + `kbd-hint` seed (§5.9)

- `menu::entries(registry, keymap, chain) -> Vec<MenuEntry>` (pure):
  registry filtered to entries whose availability contexts intersect
  the chain (`Global`-tagged always), ordered by category then
  registry order; each entry carries label, `enabled`, and the
  **first** chain-visible binding rendered as display strings (D6
  `Display`) — menus, hints, and the future visualiser are
  projections of keymap data, never hand-written (§5.9 verbatim).
  Disabled entries are included (greyed) — menu generation stays
  total and documents the reservations (§5.8).
- `ui/widgets/kbd-hint.slint` (L1 seed, §4.4 inventory): pure
  presentational `keys: [string]` → keycap row, `mono` role +
  theme tokens only (Rule A; no literal colours/fonts/spacing — the
  chunk-1 token lint gate applies). `ui/compositions/
  action-menu.slint` (L2): renders the model + selected index +
  open flag; **no key handlers** (menu cursor movement is actions,
  D3); mouse click on an entry invokes the same Rust callback the
  `menu.invoke` action reaches (every click affordance has an action
  equivalent, §7.1). Layer rules C/E respected (no view references;
  layout only).

### D20 — Insta snapshots ("documentation as a test", §10)

`crates/taskboard-ui-slint/tests/keymap_snapshots.rs` (insta yaml,
state-crate conventions): (a) **registry dump** — id, label,
contexts, category, enabled, one row per action, ordered — this
snapshot *is* the action documentation (§5.2) and must show the
reserved actions disabled; (b) **resolved keymaps** — the effective
map of each shipped preset, per context, sequences sorted — the
preset documentation; the standard map doubles as "the resolved
default keymap" (§10: default preset + empty overrides ≡ the preset
map, T16). Snapshots are
**reviewed, not blindly accepted** (stage-2 duty); regeneration is
an explicit `INSTA_UPDATE=always` run recorded in the commit
message.

### D21 — Preset contracts (tests, enforced)

For each shipped preset: (a) **core-verb coverage** — every id in
D9's list has ≥ 1 binding somewhere (+ `mode.*` for vi); (b)
**intra-context conflict-freeness** — no two actions share a
sequence in one context; (c) **prefix-shadow-freeness** — no
same-context binding is a strict prefix of another (D13); (d)
deterministic shape (const tables, BTreeMap-ordered). These replace
the compiler-side duplicate detection `KeyBinding` would have given
(§5.7a) and keep no preset a second-class citizen (Q14).

### D22 — Test strategy (tier mapping, §10 unit row; AGENTS §5)

- **Unit (pure, no slint)**: stroke parse/display (canonical order,
  case-sensitivity, named keys, punctuation, malformed classes);
  merge/validate per D13 (each finding kind; fallback-not-just-drop;
  both-drop user-vs-user; exhaustive listing; empty overrides ≡
  preset map); resolver transitions per D12; router per D16; menu
  generation per D19. Typed outcomes only; `matches!(finding.kind,
  FindingKind::Conflict)` style — **no text assertions**.
- **proptest**: (a) parse∘Display∘parse idempotence over generated
  strokes; (b) resolver sequence property — random stroke streams ×
  random chains (incl. menu open, vi modes, masking): invariants:
  exactly one `Resolution` per stroke; state returns to idle after
  `Action`/`Aborted`; any preset-bound sequence under a constant
  chain yields exactly its `Action`; strokes only bound below a
  masking context never resolve; identical streams produce
  identical outputs (determinism).
- **Component (slint thread)**: mount with a preset; inject
  `WindowEvent::KeyPressed` via `window.dispatch_event` (public
  API, fact §1.4) through the null platform: `app.menu` key opens
  the menu (property assert); `menu.next`/`menu.invoke` navigate +
  dispatch; a `task.complete` stroke with focused entities set
  sends the exact `StateCommand` to the mpsc receiver
  (end-to-end through the forwarder boundary, fake receiver —
  fakes over mocks); prefix-abort pushes the `PrefixAbort`
  feedback; an unbound key dispatches nothing. Assert terminal
  state via drained receivers/properties; no sleeps, no wall
  clock, no transient asserts.
- **App-side unit**: `[ui.keymap]` decode (defaults, old configs
  unaffected, bare-string sugar); unknown preset → typed
  `ConfigError` kind `"decode"`; `resolve_ui_settings` returns
  findings typed and `StartupWarnings` counted; valid overrides
  land in the handed-over `UiSettings.keymap` (structural asserts
  on the built map, not strings).

### D23 — Module layout (Q2: inside `taskboard-ui-slint`, Slint-free public API)

```
crates/taskboard-ui-slint/src/
  actions.rs          # ActionId, ContextId, Category, REGISTRY (D1–D5)
  keymap/mod.rs       # Keymap, Binding, RawKeymap, build() (D9, D13, D15)
  keymap/stroke.rs    # Stroke, parse, Display, named-key chars (D6, D7)
  keymap/presets.rs   # standard/emacs/vi tables, coverage lists (D8)
  resolver.rs         # KeymapResolver (D12)
  router.rs           # ActionRouter, UiEffect, DispatchCtx (D16)
  menu.rs             # menu::entries (D19)
  shell.rs            # KeymapShell: chain, effects, mode (D17)
  lib.rs              # mount/run glue + the slint adapter (D18)
crates/taskboard-ui-slint/ui/
  app-window.slint    # root FocusScope + menu host + state globals
  widgets/kbd-hint.slint
  compositions/action-menu.slint
crates/taskboard-ui-slint/tests/keymap_snapshots.rs
crates/taskboard-app/src/{config.rs,ui.rs}
```

All pure modules' **public APIs contain no slint types** (the §5.5
extraction seam, Q2's recorded trigger: extraction to its own crate
when a second consumer appears). SPDX AGPL-3.0-only headers; no
`unsafe`; no new dependencies (workspace set suffices — `insta`,
`proptest` as dev-deps of the UI crate).

## 4. Implementation steps (TDD order; every commit standalone)

Each step: failing tests first, then the module, then gate-green
commit. Steps 1–6 are pure Rust (no slint code touched); the Slint
side lands last (dispatch §3: "adapter last, after the pure modules
are green").

1. `actions.rs` — ActionId/ContextId/Category/REGISTRY + registry
   unit tests + the registry insta snapshot (D20a). Commit:
   `feat(ui-slint): action registry with contexts and reserved actions`.
2. `keymap/stroke.rs` — grammar, normalization, Display, named-key
   chars; unit tests + parse/display round-trip proptest. Commit:
   `feat(ui-slint): keymap stroke grammar`.
3. `keymap/presets.rs` + `keymap/mod.rs` (data model only) — the
   three tables; preset contract tests (D21) + preset snapshots
   (D20b). Commit: `feat(ui-slint): standard emacs and vi keymap presets`.
4. `keymap/mod.rs::build` — merge + validation + findings (D9, D13);
   the drop-and-warn unit battery (fallback assertions). Commit:
   `feat(ui-slint): keymap merge with drop-and-warn validation`.
5. `resolver.rs` — the state machine (D12) + transition units +
   sequence proptests (D22). Commit:
   `feat(ui-slint): keymap resolver state machine`.
6. `router.rs` + `shell.rs` — dispatch outcomes, effects, chain
   construction (D16, D17) + units. Commit:
   `feat(ui-slint): action router and keymap shell state`.
7. `menu.rs` + `ui/widgets/kbd-hint.slint` +
   `ui/compositions/action-menu.slint` + menu-generation units.
   Commit: `feat(ui-slint): generated action menu and kbd hint primitive`.
8. lib.rs adapter — `UiSettings` growth, root FocusScope,
   `intercept-key`, state globals, `set_focused_entities`; component
   tests via `dispatch_event` injection (D18, D22). Commit:
   `feat(ui-slint): root focus scope key interception`.
9. App side — `KeymapSection` config + `resolve_ui_settings` +
   `run_desktop` wiring + units (D14). Commit:
   `feat(app): ui keymap config surface and typed handover`.
10. Full gate suite (AGENTS §10), snapshot review pass, risk
    re-read; fix-ups as amends. Optional final commit:
    `test(ui-slint): keymap snapshot review notes` (only if the
    review adds non-generated artefact).

## 5. Test catalogue (TDD ledger)

| # | Test | Tier | Locks |
|---|---|---|---|
| T1 | registry snapshot = action documentation; reserved rows read disabled | snapshot | D2/D4/D20 |
| T2 | preset snapshots: effective maps per context, sorted | snapshot | D8/D20 |
| T3 | stroke parse: canonical, any-mod-order, case-sensitivity, named, punctuation | unit | D6 |
| T4 | stroke malformed classes (modifier-only, unknown token, control char, empty, duplicate mod) → typed `StrokeParseError` variants | unit | D6 |
| T5 | parse∘Display∘parse idempotence | proptest | D6 |
| T6 | preset core-verb coverage ×3 (+mode flips for vi) | unit | D8/D9/D21 |
| T7 | preset intra-context conflict-freeness ×3 | unit | D21 |
| T8 | preset prefix-shadow-freeness ×3 | unit | D13/D21 |
| T9 | merge: override replaces all bindings of the action | unit | D9 |
| T10 | add-new binding: global action → Global; non-global → `MaskedContext` drop | unit | D9/D13 |
| T11 | `UnknownAction`, `MalformedStroke` findings drop only the offender; others survive | unit | D13 |
| T12 | conflict user-vs-preset: override dropped, preset binding verbatim (fallback assert) | unit | D13 |
| T13 | conflict user-vs-user: both dropped, preset wins for both | unit | D13 |
| T14 | `PrefixShadow` on effective map: warning, nothing dropped | unit | D13 |
| T15 | findings exhaustive (multiple offenders all listed) | unit | D13 |
| T16 | empty overrides ≡ preset map | unit | D9 |
| T17 | resolver: single-stroke, chord, prefix accumulation, prefix-abort → idle | unit | D12 |
| T18 | resolver: chain order first-match; shadowing across contexts | unit | D11/D12 |
| T19 | resolver: masking (`Menu` masks globals; `ViInsert` shadows only) | unit | D11 |
| T20 | resolver: pending-chain mismatch resets and reprocesses | unit | D12 |
| T21 | resolver sequence properties (idle-return, bound-sequence-yields-action, mask isolation, determinism) | proptest | D12/D22 |
| T22 | router: command mappings, Disabled, NoTarget, enabled table | unit | D16 |
| T23 | shell: chain construction per mode/menu; effects; resets | unit | D17 |
| T24 | menu::entries: filtering, ordering, first-binding display, disabled included | unit | D19 |
| T25 | component: injected keys → menu open/navigate/invoke | component | D18 |
| T26 | component: task.complete keystroke → exact `StateCommand` at mpsc receiver | component | D16/D22 |
| T27 | component: prefix-abort pushes `PrefixAbort` feedback; unbound key no-op | component | D12/D18 |
| T28 | config: `[ui.keymap]` decode, defaults, old-config compat, sugar | unit (app) | D14 |
| T29 | config: unknown preset → typed decode error kind | unit (app) | D13/D14 |
| T30 | `resolve_ui_settings`: findings typed + StartupWarnings count + handover | unit (app) | D13/D14 |

No text assertions anywhere (labels appear only *inside* snapshots,
which are data); no fixed sleeps (component tests are
dispatch/drain-based on the slint thread); one behaviour one test —
the resolver's semantics live in T17–T21, component tests assert
routing only.

## 6. File manifest (end state)

```
crates/taskboard-ui-slint/src/actions.rs              (new, ~200 ln + tests)
crates/taskboard-ui-slint/src/keymap/mod.rs           (new, ~250 ln + tests)
crates/taskboard-ui-slint/src/keymap/stroke.rs        (new, ~250 ln + tests)
crates/taskboard-ui-slint/src/keymap/presets.rs       (new, ~220 ln + tests)
crates/taskboard-ui-slint/src/resolver.rs             (new, ~180 ln + tests)
crates/taskboard-ui-slint/src/router.rs               (new, ~150 ln + tests)
crates/taskboard-ui-slint/src/menu.rs                 (new, ~100 ln + tests)
crates/taskboard-ui-slint/src/shell.rs                (new, ~150 ln + tests)
crates/taskboard-ui-slint/src/lib.rs                  (grow: adapter, ~250 ln)
crates/taskboard-ui-slint/ui/app-window.slint         (grow: root scope, menu host)
crates/taskboard-ui-slint/ui/widgets/kbd-hint.slint   (new)
crates/taskboard-ui-slint/ui/compositions/action-menu.slint (new)
crates/taskboard-ui-slint/tests/keymap_snapshots.rs   (new + 4 .snap files)
crates/taskboard-ui-slint/Cargo.toml                  (dev-deps: insta, proptest)
crates/taskboard-app/src/config.rs                    (grow: KeymapSection + tests)
crates/taskboard-app/src/ui.rs                        (grow: resolve_ui_settings + wiring + tests)
```

Estimated diff: **~2,900–3,400 added lines** across ~15 files
(≈60% tests/snapshots) — consistent with the dispatch plan's M–L
(8–12 d).

## 7. Acceptance mapping (dispatch §3 → this plan)

| Dispatch acceptance line | Where |
|---|---|
| `keymap`/`actions` public APIs contain no Slint types (§5.5) | D23, T-suite purity |
| preset-coverage green ×3 over core verbs | D8/D9/D21, T6 |
| shipped presets conflict-free | D21, T7 |
| validation drop-and-warn; hostile keymap never blocks start (Q15); preset fallback asserted | D13, T11–T16 |
| findings are typed enum values, no text assertions | D13, D22 |
| registry snapshot shows reserved actions disabled | D4/D20, T1 |
| root scope is the single key path; no view binds keys; `KeyBinding` unused (Q9) | D18 (grep + review) |
| resolver prefix-abort is a defined transition | D12 rule 4, T17 |
| text input never starved (capture only unconsumed) | D18 capture-semantics note + chunk-3 contract (no `TextInput` exists in chunk 2) |
| resolver proptested over chord/prefix/mode/masking sequences | D22, T21 |
| `app.menu` generated menu + kbd-hint seed under layering rules | D19 |
| non-goals respected (no views/conflict UI/settings/live-remap) | §2 |

## 8. Risks & fallbacks

- **Capture-phase vs text input (the big one)**: capture fires
  before the focused element; typing is protected by chain
  discipline, which chunk 3 must implement (text contexts resolve
  no printables). Mitigation: recorded as chunk 3's contract (D18);
  chunk 2 ships no `TextInput`, so the exposure is zero until then;
  the chunk-3 review checklist includes it. Fallback if Slint's
  delivery proves awkward with real fields: move the root
  interception to the bubble phase (`key-pressed` on the root
  scope) — mechanically the same callback, one-line change,
  re-run the same component tests.
- **Cross-platform stroke delivery**: verified on winit/Linux
  (Ctrl+letter → letter char + ctrl; shift in case). Windows
  AltGr/`text_without_modifiers` handling and macOS ⌘/ctrl mapping
  are release-chunk tuning (D6 note); first-class platforms beyond
  Linux get a manual pass in the release chunk, not here.
- **Preset stroke choices are drafts**: the enforced contracts
  (coverage, conflict-free, shadow-free) hold for any retuning;
  snapshots regenerate mechanically. Human review of the keymap
  snapshot is the intended tuning forum (D8/D20).
- **`UiSettings` growth breaks `Copy`**: callers pass by value
  (chunk 1's `mount` already does); mechanical.
- **Enabled-bit churn in chunk 3** (`nav.*`, create/edit/move flip
  on): one table edit + snapshot regen by design (D5); the
  dispatch acceptance only requires *reserved* rows disabled.
- **Menu navigation without focus traversal**: menu entries are not
  tab-focusable in chunk 2 (cursor is action-driven); if the
  session/testing wants tab-in-menu later, it is a chunk-3/4
  addition that composes (actions still own the keyboard).

## 9. Verification of this round

- Precondition verified: chunk-1 seams present on `main` (slint
  dep, theme module, window shell, forwarder, `[ui]` table) at
  `20e3ca3`, an ancestor of the branch base `9b79aa1`.
- Every design-plan normative line this chunk covers (§5.1–§5.9,
  §10 unit/snapshot rows, §12 row 2, Q1/Q2/Q9/Q14/Q15-redirected/
  Q20/Q22-keymap) maps to a decision (§3) or an acceptance row
  (§7); deviations are exactly two and both carry reasons ([DEV]
  D3, D13-boundary).
- Every Slint fact cited is version-pinned to the vendored 1.18.1
  sources (§1.4); nothing was re-derived against a different
  version.
- The test catalogue (§5) covers every decision with a lock; the
  TDD ledger + standalone-commit ladder (§4) satisfies the rebase
  hygiene and "pure modules before slint" mandates.
- **Review round 1 (2026-10-15)**: all 23 decisions reviewed; D1–D5,
  D7, D11–D23 accepted as written; D6 accepted with the numpad
  answer added (Slint 1.18.1 cannot distinguish numpad — logical
  char only; additions mechanical); D8 emacs/vi tables reworked to
  the review's binding-design principle (source-editor-authentic,
  the tmux standard; home-row motions over arrows outside the CUA
  preset); D9's add-new rule clarified — the replace-all rule
  covers every preset-bound action, the add-in-Global rule exists
  for the preset-unbound case the design's upgrade model requires
  to work, and `MaskedContext` = "this override can never fire
  where the merge would place it"; D10 stripped of all mode
  machinery (`uses_vi_modes` gate and registry restrictions
  removed — mode is two plain shell-state fields, the chain always
  carries the mode context, frankenstein mode-in-emacs presets work
  with no guards). Consistency sweep done: registry availability
  lists view contexts only (D2), chain construction (D11), keymap
  model (D15) all match the guard-free model.
