# [Plan] GUI Review Round 2 — Chunk 3: Action registry round-up & object context menus (D16–D21)

- **Date**: 2026-10-24 (stage-1 run; filename follows the round's
  dispatch naming)
- **Kind**: stage-1 chunk plan (TDD-first; no implementation in this
  commit). Branch `feat/ui-r2-actions-context-menus`, cut at
  `origin/main` `cb02a38` (chunk 2 `fix/ui-r2-interaction-repairs`
  merged — precondition verified via `git fetch origin --prune` +
  `git log origin/main --oneline`; worktree created with
  `git worktree add -b` after a green `scripts/worktree-doctor.sh`,
  101G free / 25G floor).
- **Origin** (normative): design round
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` §4
  (D16–D21), §1.4 (F-E1–F-E4), §1.2 F-B7 (the context-menu chain
  bug), §1.6 (domain command coverage — expected empty domain diff),
  §10 (testing strategy), §14-R3 (keymap-collision risk); dispatch
  plan `.artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md`
  §1 (shared execution model + code-quality mandate) and §4 (this
  chunk's acceptance criteria (a)–(i)).
- **Non-goals** (dispatch §4): no domain/state/sync-crate changes
  (every command this chunk dispatches exists, design §1.6); no
  drag-to-move (chunk 8); no reminder-action bindings beyond the
  D16–D21 set (the `reminder.*` unbound rows stay recorded debt);
  no delete-confirmation modal (D18: "no new confirmation modal this
  round"); no due-date/task-view work (chunk 4); no settings work
  (chunks 5–6). Expected crate diff: `taskboard-ui-slint` only.

## 1. Verified facts (anchored; line numbers are post-chunk-2 `main` = `cb02a38`)

- **F-1 Registry shape.** `ActionId` (`src/actions.rs:150-185`, 34
  variants) ↔ `REGISTRY` rows (`src/actions.rs:302-648`) pinned
  one-to-one by `registry_covers_every_action_exactly_once`
  (`src/actions.rs:675-716`). Every non-reserved row must be
  `enabled: true` (`only_reserved_actions_stay_disabled`); every row
  carries a non-empty `description`
  (`every_row_carries_a_description_and_spaced_top_ranks` — the
  rank-spacing clause binds only `menu_group == None` rows). Adding a
  variant without a row, a row without router coverage, or a row
  without a description is a compile/test error by construction.
- **F-2 Presets & contracts.** `STANDARD` (`src/keymap/presets.rs:62-108`),
  `EMACS` (114-166), `VI` (173-215). Gates that every new binding
  must pass **unweakened** (`src/keymap/mod.rs:426-605`):
  `every_preset_covers_the_core_verbs` (426-467),
  `every_preset_resolves_the_board_verbs_in_the_view_chain` (475-486),
  the stack-width mirror (492-503), the settings-close mirror
  (509-520), `presets_are_intra_context_conflict_free` (538-548, T7),
  `presets_are_prefix_shadow_free` (554-573, T8),
  `vi_preset_shape_matches_the_mode_model` (578-595: vi Global is
  **empty**, ViInsert binds exactly one row),
  `presets_mask_only_the_menu` (600-605).
- **F-3 The resolver is exact-match-first.** `KeymapResolver::feed`
  (`src/resolver.rs:60-82`): an accumulated sequence that
  **exact-resolves dispatches immediately** (69-72) — the
  `has_prefix` pending test (73-75) only runs when nothing
  exact-matched. Consequence for D16: a lone-`Ctrl+g` binding in the
  **same context** as emacs `app.redo`'s `["Ctrl+g","Ctrl+/"]`
  (`presets.rs:118`) would exact-match first and make the redo
  sequence permanently unreachable — "registration ordering" cannot
  fix this (the binding map is a `BTreeMap`, row order is inert).
  §3.2 records the working resolution.
- **F-4 Move-cancel is structurally missing** (F-E2): no cancel
  action exists; Escape resolves to nothing on a board-view chain in
  every preset (Escape rows live only in Menu/ViInsert/Settings);
  emacs lone C-g is a pending prefix of redo; and the interpret
  table's discard arm covers only `Flow::Create | Flow::EditTitle` —
  `(Flow::Move, SetMode(Normal))` falls through to the default arm
  (`src/adapter.rs:444-446, 503-508`). `view_state::cancel_move`
  (`src/view_state.rs:803-809`) is already flow-agnostic (it ends
  whatever flow is active) — only the routing to it is missing.
- **F-5 The interpret table and its leaving rule**
  (`src/adapter.rs:400-512`): leaving effects (NextView/PrevView/
  FocusBoard/BoardNext/BoardPrevious/BoardSelect) commit edit flows
  and **cancel move flows first** (418-430); the `(flow, effect)`
  match (432-510) routes move-flow FocusNext/FocusPrev/Activate/
  RequestMove, edit-flow SetMode(Normal)/Activate, and defaults for
  everything else. `apply_outcomes` (276-316) sends
  `FlowOutcome::Command`s over the mpsc, applies SetMode/RefreshNow,
  and hands focus back to the view when the flow ended (313-315).
- **F-6 Context menu today** (round-1 D13): `open_task_context_menu`
  (`src/adapter.rs:1432-1448`) opens the shared menu pipeline over
  chain `[TaskDetail, Global]` with `menu::context_entries` = the
  flat Task-group page (`src/menu.rs:113-118`), anchored at
  `MenuAnchor::Pointer(x, y)`. Board/stack card right-clicks focus
  the card first (`activate_card`, `src/adapter.rs:1452-1494`) —
  right-click selects like left-click. **F-B7**: rows project
  `hint: first_binding_display(entry.id, chain)` (`src/menu.rs:50-61`)
  but the task verbs' bindings live in the **Board** context, so over
  `[TaskDetail, Global]` every task-verb hint renders empty (F9).
- **F-7 Keymap chain.** `KeymapShell::chain` (`src/shell.rs:54-68`):
  menu → `[Menu]`; insert → `[ViInsert, Global]`; otherwise
  `[view, ViNormal, Global]`. `first_binding_display`
  (`src/keymap/mod.rs:119-138`) walks the visible chain, skipping
  unbound contexts — a Board-context binding is invisible to a chain
  without a Board member.
- **F-8 Stroke grammar** (`src/keymap/stroke.rs`): named keys include
  Home/End/Delete/PageUp/PageDown/Escape/Return/Space/F1–F12/arrows;
  printables are single chars, case-carrying (`"U"` ≠ `"u"`); bare
  printables fold shift into the char, chorded printables keep shift
  as a modifier; `Alt+<` / `Alt+>` / `Ctrl+Home` / `F2` all parse.
- **F-9 Domain commands all exist** (`crates/taskboard-domain/src/messages.rs`):
  `SetTaskDone { id, done }` (38-43 — the flag already carries
  reopen), `CreateStack` (59-66), `RenameStack` (68-73), `DeleteStack`
  (75-78), `MoveStack { id, order }` (81-86), `CreateBoard { title,
  color }` (125-130 — always mints), `RenameBoard` (149-154),
  `DeleteBoard` (177-180 — cascades; repeat is an accepted no-op).
  `Color` is a lenient newtype (`entities.rs:23-24`); test fixtures
  and UI-side code use `Color::new("")` (e.g.
  `taskboard-app/src/ui_requests.rs:637`).
- **F-10 Focus/selection machinery.** `step_focus`
  (`src/view_state.rs:434-505`) walks reading order (no wrap outside
  compact; wrap at the compact switcher via `wrapped_index`, 127-131);
  `select_stack` (510-514) sets selection and lands focus on the new
  stack's first task; `keyboard_resize_target` (626-630) is the
  established focused→selected→first stack resolution;
  `select_board` (382-389) resets selection/focus onto the new board.
  No first/last jump exists (W4).
- **F-11 Right-click pattern** (`ui/widgets/task-card.slint:85-91`):
  `pointer-event(event)` filtering
  `PointerEventKind.down && PointerEventButton.right`, emitting
  window-coordinate `(absolute-position + mouse-x/y)` floats. The
  stack header TouchArea (`ui/compositions/board-layout.slint:179-194`)
  and board-bar segment TouchArea
  (`ui/compositions/board-bar.slint:79-96`) have clicks/hover only;
  the compact switcher segments (`board-layout.slint:408-410`)
  likewise.
- **F-12 Snapshots.** `tests/keymap_snapshots.rs` produces
  `registry` + `resolved_preset_keymaps` snapshots
  (`tests/snapshots/keymap_snapshots__*.snap`) — regeneration is an
  explicit reviewed `INSTA_UPDATE=always` run recorded in the commit
  body. Board goldens are paint-pinned; this chunk must not touch
  them (any golden diff = finding, chunk-1 precedent).
- **F-13 vi chain exposure.** vi binds everything in `ViNormal`,
  which sits in every non-menu/non-insert chain — including
  `[Settings, ViNormal, Global]`. Vi-style g-verbs can therefore fire
  from the settings view today (e.g. `o` → BeginCreate → the
  Settings-view `create_target_stack` returns `None` → inert). The
  new vi bindings inherit this exposure; flow-opening verbs must be
  similarly inert-or-sane from odd views (§5).
- **F-14 Model-push hint slots** (`src/models.rs:141-152, 290-362`):
  six `[string]` hint models pushed per refresh from
  `first_binding_display` over the visible chain — the established
  way any new affordance gets its binding hint.

## 2. D16–D21 → the registry rows (decision: 12 new actions)

All rows are `enabled: true` (none reserved; F-1's post-flip test
enforces this), each with a one-sentence description (what + effect).
`contexts` are **menu-availability** contexts; binding contexts are
§3's concern.

| ActionId | id | label | category | contexts | menu_group | menu_rank |
|---|---|---|---|---|---|---|
| `TaskMoveCancel` | `task.move.cancel` | Cancel move | Task | `GLOBAL` | Task | 95 |
| `StackNext` | `stack.next` | Next stack | Navigation | `GLOBAL_VIEW_NAV` | Stack | 60 |
| `StackPrev` | `stack.prev` | Previous stack | Navigation | `GLOBAL_VIEW_NAV` | Stack | 70 |
| `StackDelete` | `stack.delete` | Delete stack | Navigation | `[Board, StackDetail]` | Stack | 130 |
| `StackRename` | `stack.rename` | Rename stack | Navigation | `[Board, StackDetail]` | Stack | 90 |
| `StackMove` | `stack.move` | Move stack | Navigation | `[Board, StackDetail]` | Stack | 100 |
| `BoardCreate` | `board.create` | New board | Navigation | `GLOBAL_VIEW_NAV` | Board | 90 |
| `BoardDelete` | `board.delete` | Delete board | Navigation | `GLOBAL_VIEW_NAV` | Board | 110 |
| `BoardRename` | `board.rename` | Rename board | Navigation | `GLOBAL_VIEW_NAV` | Board | 100 |
| `TaskReopen` | `task.reopen` | Reopen task | Task | `[Board, TaskDetail]` | Task | 85 |
| `NavFirstItem` | `nav.first-item` | Focus first item | Navigation | `VIEW_NAV` | ViewNav | 105 |
| `NavLastItem` | `nav.last-item` | Focus last item | Navigation | `VIEW_NAV` | ViewNav | 115 |

Ids `stack.next`/`stack.prev` follow D17/F-E1's literal naming
(`board.previous`'s longer form is round-1 legacy; the design names
the short form for stacks).

**Descriptions (recorded so stage 2 writes them once):**
`task.move.cancel` — "Cancels the active move flow without moving
anything (Escape; under emacs also Ctrl+g)."; `stack.next`/`prev` —
"Moves the stack selection to the next/previous stack (wraps) and
focuses its first task."; `stack.delete` — "Deletes the focused or
selected stack and everything in it; the engine refuses typed when
the target is gone."; `stack.rename` — "Opens the stack title editor
for the focused or selected stack."; `stack.move` — "Starts a move
flow for the focused or selected stack: pick an insertion slot, then
confirm to reorder."; `board.create` — "Opens the new-board field and
creates the board on commit."; `board.delete` — "Deletes the current
board and all its content (no confirmation step); typed refusals are
the engine's."; `board.rename` — "Opens the board title editor for
the current board."; `task.reopen` — "Marks the focused completed
task as not done."; `nav.first-item`/`last` — "Moves the focus to the
first/last card in reading order."

**Menu groups (D21): `MenuGroup::Stack` and `MenuGroup::Board` are
added, and `BoardStack` is SPLIT.** Declaration order (the root
page's parent-row order) becomes `Task, Stack, Board, ViewNav, Mode,
Help`; labels "Task actions", "Stack", "Board", "View & navigation",
"Mode", "Help". Rows move: `StackCreate`/`StackWider`/`StackNarrower`
(from BoardStack) + the three new stack rows → **Stack**;
`BoardNext`/`BoardPrevious`/`BoardSelect` (from BoardStack) + the
three new board rows → **Board**. Stack-group ranks renumber to
next 60, prev 70, create 80 (was 90), rename 90, move 100, wider 110
(was 100), narrower 120 (was 110), delete 130; board-group: next 60,
previous 70, select 80 (all unchanged), create 90, rename 100,
delete 110. Recorded reason for the split: D21 adds Stack and Board
as object groups; leaving a third "Board & stack" parent alongside
them would double-list every row's grouping and incohere the two-tier
menu. `menu_group` is presentation-only (keymap resolution never
reads it, `src/menu.rs:1-10`), so this is zero-risk to bindings; the
registry + root-page snapshots and the menu tests T-1/T-5 are
extended accordingly (recorded decisions, not weakenings — the group
inventory changed by design).

## 3. The binding table (decision; every preset binds every new action — R3)

### 3.1 Final table

Contexts: std/emacs view verbs bind in **Board** (the board-surface
chain head); vi binds in **ViNormal** (F-2's vi shape; its Global
must stay empty). `nav.first/last-item` follow the `nav.next-item`
family: bound in **Board, StackDetail, TaskDetail** for std/emacs.

| action | standard | emacs | vi |
|---|---|---|---|
| `task.move.cancel` | `Escape` (Global) | `Escape` (Global) **+** `Ctrl+g` (Board) | `Escape` (ViNormal — §3.4) |
| `stack.next` | `Ctrl+Shift+Right` (Board) | `Ctrl+f` (Board) | `g l` (ViNormal) |
| `stack.prev` | `Ctrl+Shift+Left` (Board) | `Ctrl+b` (Board) | `g h` (ViNormal) |
| `stack.delete` | `Ctrl+Delete` (Board) | `Ctrl+c Delete` (Board) | `g D` (ViNormal) |
| `stack.rename` | `F2` (Board) | `Ctrl+c r` (Board) | `g r` (ViNormal) |
| `stack.move` | `Ctrl+Alt+M` (Board) | `Ctrl+c m` (Board) | `g v` (ViNormal) |
| `board.create` | `Ctrl+Shift+N` (Board) | `Ctrl+c Ctrl+b` (Board) | `g n` (ViNormal) |
| `board.delete` | `Ctrl+Shift+Delete` (Board) | `Ctrl+c Ctrl+d` (Board) | `g d` (ViNormal) |
| `board.rename` | `Ctrl+F2` (Board) | `Ctrl+c b` (Board) | `g R` (ViNormal) |
| `task.reopen` | `Ctrl+Space` (Board) | `Ctrl+c Ctrl+Space` (Board) | `U` (ViNormal) |
| `nav.first-item` | `Ctrl+Home` (Board/StackDetail/TaskDetail) | `Alt+<` (same 3) | `H` (ViNormal) |
| `nav.last-item` | `Ctrl+End` (Board/StackDetail/TaskDetail) | `Alt+>` (same 3) | `G` (ViNormal) |
| `stack.create` (emacs gap, F-E1) | — (has `S`) | `Ctrl+c s` (Board) | — (has `O`) |

**Deviations from the design's concrete keys, with reasons:**

1. **standard `stack.next/prev` = `Ctrl+Shift+Right/Left`, not the
   design's `Ctrl+Alt+Right/Left`.** Ctrl+Alt+Left/Right are the
   stock workspace-switch shortcuts on GNOME and KDE — on the dev
   platform (x86_64 Linux, first-class release target) the WM
   consumes them and the app would never see the event. The
   design's stated intent ("families with the stack.wider pair
   without colliding" — `Ctrl+Right/Left`) is preserved exactly,
   one modifier up.
2. **vi `nav.first-item` = `H`, not the design's `g g`.** `g g` is
   `nav.focus-board` since round-1 (`presets.rs:179`) — T7
   (intra-context conflict) rejects the design's proposal verbatim,
   and re-binding `nav.focus-board` would deviate further.
   `H`/`L` are vim's top/bottom-of-screen motions; `H` = first
   item reads authentically beside the design's own `G` = last.
3. **The design's "(board chain)"/"(stack chain)" parentheticals for
   D18's deletes are read as prose about the verb family, not
   binding contexts.** Binding stack/board verbs in the literal
   StackDetail context would make them unresolvable in the board
   view (the chain head) and invisible to every menu-hint chain in
   §7 — defeating the chunk's own hint goal. All board-surface
   verbs bind in Board (std/emacs) / ViNormal (vi), the established
   family shape (F-2's board-verb contract).
4. **`stack.move`/`stack.rename`/`board.rename` keys are stage-1
   picks** (the design names the entries, not their chords): vi
   extends the existing `g`-family with the `d`/`D` delete-pair
   convention for `r`/`R` renames and `v` for the move; std pairs
   `F2`/`Ctrl+F2` (the CUA rename convention) and `Ctrl+Alt+M`
   beside `M` (task move); emacs extends the org-mode `C-c` family
   — bare second stroke for stack verbs (`s`, `m`, `r`, `Delete`),
   chorded for board verbs (`Ctrl+b`, `Ctrl+d`), matching the
   design's own D18 picks in that family.

### 3.2 The C-g-vs-`app.redo` resolution (D16; recorded)

The design's "resolves by ordering" cannot work: the resolver is
exact-match-first (F-3) and binding order is inert — a lone
`Ctrl+g` bound in **Global** would dispatch `task.move.cancel` on
the first stroke in every chain and emacs `app.redo`
(`["Ctrl+g","Ctrl+/"]`, Global) would become permanently
unreachable, tripping the spirit of T8 while weakening redo.

**Resolution: emacs binds `task.move.cancel` to `Ctrl+g` in the
Board context** (Escape stays the all-preset Global binding).
Behaviour, pinned by tests (§9.2):

- Board view (chain `[Board, ViNormal, Global]`): lone `Ctrl+g`
  exact-resolves in Board → `Action(TaskMoveCancel)` — cancels an
  active move flow, inert without one. Move flows only ever live in
  the board view under emacs (the only leaving effects that change
  the view cancel the flow first, F-5), so "a lone C-g during a move
  flow cancels" holds exactly.
- Every other chain (`[TaskDetail, ViNormal, Global]`,
  `[StackDetail, …]`, `[Settings, …]`, `[ViInsert, Global]`): no
  Board member → `Ctrl+g` is still a pending prefix of redo;
  `Ctrl+g Ctrl+/` still resolves `Action(AppRedo)` ("completing the
  sequence elsewhere still redoes").
- T7/T8 pass: `["Ctrl+g"]` (Board) and `["Ctrl+g","Ctrl+/"]`
  (Global) are in different contexts; no Board binding extends
  `Ctrl+g`; no Global single `Ctrl+g` exists.

Accepted consequence (recorded): under emacs, redo's chord is
unreachable **from the board view** (C-g always cancels there).
`app.redo` is a reserved disabled row (backlog #353) dispatching
nothing today; when redo ships, its emacs binding gets re-picked in
that round. The add-stack tooltip's empty-hint case (r2 D6) also
closes: emacs `stack.create` = `Ctrl+c s` makes the hint non-empty.

### 3.3 Conflict/shadow run-through (mentally executed per candidate)

- **T7 (intra-context uniqueness), std Board additions:**
  `Ctrl+Shift+Right/Left` (vs `Ctrl+Right/Left` wider/narrower —
  distinct strokes, F-8), `Ctrl+Delete` (vs `Delete` = task.delete),
  `Ctrl+Shift+Delete`, `Ctrl+Shift+N` (vs `Ctrl+Shift+M` = app.menu
  in Global — different context; in-Board unbound), `Ctrl+Space`
  (vs `Space` = task.complete), `F2`, `Ctrl+F2`, `Ctrl+Alt+M`,
  `Escape` (Global; std Global has no Escape), `Ctrl+Home/End`
  ×3 contexts (all unbound). All unique.
- **T7, emacs:** Board gains `Ctrl+g`, `Ctrl+f`, `Ctrl+b`, and the
  `Ctrl+c`-family extensions `s`, `m`, `r`, `b`, `Delete`,
  `Ctrl+b`, `Ctrl+d`, `Ctrl+Space` — the family's second strokes
  (`Ctrl+t`, `Ctrl+w` existing; new ones listed) are pairwise
  distinct; no collisions with existing Board rows. Global gains
  only `Escape` (unbound there; the Settings-context Escape is a
  different context). `Alt+<`/`Alt+>` ×3 view contexts, unbound.
- **T7, vi:** ViNormal gains `g l`, `g h`, `g D`, `g r`, `g v`,
  `g n`, `g d`, `g R`, `U`, `H`, `G` — all distinct from the
  existing `g t`, `g T`, `g g`, `g m`, `d d`, `Z *` families and
  case-distinct where paired (`g d`/`g D`, `g r`/`g R`, `u`/`U`).
  vi's `task.move.cancel` Escape lives in **ViNormal**, not Global —
  the vi-shape carve-out of §3.4.
- **T8 (prefix-shadow):** all new multi-stroke rows extend unbound
  prefixes (`Ctrl+c` alone is bound nowhere in Board; `g` alone
  nowhere in ViNormal); no new single-stroke row is a prefix of an
  existing longer row (`Ctrl+g` in Board has no Board extension;
  Escape has no extension in any context it joins — the
  Menu/Settings/ViInsert Escapes are other contexts, other chains).
  Clean.
- **Cross-context shadowing (legitimate, D11):** std
  `Ctrl+Shift+N` (Board) shadows nothing (app.menu is
  `Ctrl+Shift+M`); the vi g-family is additive. Emacs Board's
  `Ctrl+c`-family makes `Ctrl+c`-then-`c` from the board view still
  resolve Global's `capture.quick` (Board has no `Ctrl+c c`) —
  unchanged behaviour.

### 3.4 vi-shape carve-out (recorded decision)

`vi_preset_shape_matches_the_mode_model` asserts vi's **Global is
empty**. The all-preset `Escape` (Global) binding for
`task.move.cancel` would violate it. Resolution: vi binds
`task.move.cancel` to `Escape` in **ViNormal** (vi's "everything
lives in ViNormal" model — the vi-shape test's Global-emptiness is
preserved verbatim; the contract test is **extended**, not weakened,
to additionally assert vi resolves the cancel verb over the
`[ViNormal, Global]` chain). Escape in ViNormal conflicts with
nothing (ViNormal has no Escape; ViInsert's Escape is another
context and still wins while typing, chain head first).

## 4. Router & effects (decision)

Ten new `UiEffect` variants (`src/router.rs`): `CancelMove`,
`StackNext`, `StackPrev`, `StackDelete`, `StackRename`, `StackMove`,
`BoardCreate`, `BoardDelete`, `BoardRename`, `FocusFirst`,
`FocusLast` (eleven — `FocusFirst`/`FocusLast` separate). New arms:

- `TaskMoveCancel`, `StackNext`, `StackPrev`, `StackMove`,
  `StackDelete`, `StackRename`, `BoardCreate`, `BoardDelete`,
  `BoardRename`, `NavFirstItem`, `NavLastItem` → the corresponding
  unparameterized `UiEffect`s (target resolution is the view
  state's, §5 — the `StackWider`/`BoardNext` precedent; pure
  effects, never `Command`s, the dumb-client pin).
- `TaskReopen` → `Command(StateCommand::SetTaskDone { id, done:
  false })` from `(ctx.focused_task, ctx.focused_task_done)`,
  `NoTarget` otherwise — the exact `TaskComplete` machinery
  inverted (dispatch plan 3.3 trap (e)).
- **`DispatchCtx` gains one field**: `focused_task_done: bool`,
  synced in `sync_view_context` from the focused task's `done`
  stamp (the router stays pure; no other ctx change — context-menu
  targets resolve via select-on-right-click, §7.3, so no one-shot
  target plumbing is needed).
- `shell::apply` and the adapter's effect-exhaustiveness match arms
  grow the same variants as inert shell state (the BoardNext
  precedent).

## 5. View state (decision)

### 5.1 Cancel plumbing (D16)

- Interpret-table arms (adapter §6): `(Flow::Move{..} |
  Flow::MoveStack{..}, CancelMove | RequestMove | SetMode(Normal))`
  → `cancel_move()` (already flow-agnostic, F-4); `(Flow::None,
  CancelMove)` falls to the default inert arm. This closes F-E2's
  structural half: Escape/C-g now *reach* `cancel_move` from every
  preset, and `SetMode(Normal)`-during-move no longer falls through
  leaving the flow intact.

### 5.2 `stack.next`/`stack.prev` (D17)

`step_stack(&mut self, state, direction: i32) -> Vec<FlowOutcome>`:
normalize, `stack_ids(state)`; `None` selection enters at the
direction-appropriate end (the `step_focus` switcher rule); else
`wrapped_index(current, direction, len)`; then **`select_stack`**
(selection + focus on the new stack's first task — acceptance (d)).
Inert (`Vec::new()`) with zero stacks. **Wrap** is the recorded
board-level decision (D17: "wrap, matching `board.next` semantics"
— note `board.next` itself does NOT wrap; the design's sentence
means the compact walk's wrap rule; the recorded semantics: stacks
wrap like the compact switcher, D8's segmented-control convention).

### 5.3 `jump_focus` (D20)

`pub enum FocusEdge { First, Last }` +
`jump_focus(&mut self, state, edge) -> Vec<FlowOutcome>`: the same
domain choice as `step_focus` (compact → the effective stack's
tasks; else `reading_order`) — factored into a private
`focus_domain(state, compact)` helper both walkers share (one
behaviour, one home). First/last sets `focused_task` and returns
`RefreshNow`; empty domain → inert. Idempotent at the target
(acceptance (g): jumping from the end to Last stays put — same
state, one `RefreshNow`).

### 5.4 New flows (D18/D21)

`Flow` gains `CreateBoard`, `RenameStack { stack: StackId }`,
`RenameBoard { board: BoardId }`, `MoveStack { stack: StackId, slot:
usize }` (slot = insertion position, `0..=len`).

- `begin_create_board()` — always available (the zero-board
  first-run escape; the `begin_create_stack` refusal does not
  apply), buffer cleared, `SetMode(Insert)` + `RefreshNow`.
- `begin_rename_stack(state)` / `begin_rename_board(state)` —
  resolve the target (`keyboard_resize_target` / current board);
  seed the buffer from the current title (the `edit_subject`
  pattern); inert without a target.
- `delete_stack(state)` — `keyboard_resize_target` resolution →
  `[Command(DeleteStack { id }), RefreshNow]`; inert without.
  Selection repair is `reconcile`'s on the next refresh.
- `delete_board(state)` — current board →
  `[Command(DeleteBoard { board }), RefreshNow]`; inert with none.
  The engine owns every refusal (last-board/mid-sync per its
  rules); the UI dispatches and renders nothing new.
- `begin_move_stack(state)` — resolve the moving stack (same
  resolution); `slot` starts at its current position; `RefreshNow`.
- `cycle_move_stack_target(state, direction)` — wrap `slot` over
  `0..=len` (`wrapped_index` over `len + 1` slots); `RefreshNow`.
- `confirm_move_stack(state)` — **re-normalization semantics**
  (recorded): compute the desired final stack order (the current
  display order with `stack` reinserted at `slot`) and dispatch the
  **minimal set** of `MoveStack { id, order }` commands with target
  orders `0..len−1` — only for stacks whose order changes. One
  absolute-order command can never express a dense insertion
  (F-9); renumbering the permutation is deterministic, idempotent,
  and tie-free forever. Flow ends; `RefreshNow`.
- `commit_edit` arms: `CreateBoard` (non-empty title) →
  `Command(CreateBoard { title, color: Color::new("") })` (the
  lenient empty color — the fixture convention, F-9; a colour
  picker is future work, recorded debt); `RenameStack` →
  `Command(RenameStack { id, new_title })`; `RenameBoard` →
  `Command(RenameBoard { board, title })`. Empty buffer commits
  nothing and still ends the flow (the `commit_edit` rule).
- The discard/leaving arm set (`Flow::Create | CreateStack |
  EditTitle`) extends to all three new edit flows; leaving effects
  commit them first (the `leaving` match), a `MoveStack` flow
  cancels on leaving like `Flow::Move`.
- `reconcile` arms: `RenameStack` on a vanished stack, `RenameBoard`
  on a vanished board, and `MoveStack` on a vanished moving stack
  all end the flow silently (the `Flow::Move` precedent).

### 5.5 Move-stack visuals

`push_board`'s `move_target` (models.rs:459-466) additionally
derives from `Flow::MoveStack { slot, .. }`: highlight the stack at
`min(slot, len−1)` (the insertion point; `slot == len` highlights
the last stack = "append after it"). The status-bar flow hint
(`push_status_bar`, models.rs:676-685) extends to `Flow::MoveStack`
using `first_binding_display(StackMove, chain)`.

## 6. Interpret table (complete new arms; adapter `src/adapter.rs:400-512`)

```
(Flow::Move{..} | Flow::MoveStack{..}, CancelMove)            -> cancel_move()
(Flow::Move{..} | Flow::MoveStack{..}, RequestMove)           -> cancel_move()      [existing arm, extended]
(Flow::Move{..} | Flow::MoveStack{..}, SetMode(Mode::Normal)) -> cancel_move()      [new: F-E2's fall-through]
(Flow::Move{..} | Flow::MoveStack{..}, FocusNext/FocusPrev)   -> cycle target       [extended]
(Flow::Move{..} | Flow::MoveStack{..}, Activate)              -> confirm            [extended]
(Flow::None, CancelMove)                                      -> default inert arm
(_, StackNext/StackPrev)       -> step_stack(state, ±1)
(_, FocusFirst/FocusLast)      -> jump_focus(state, edge)
(_, StackDelete)               -> delete_stack(state)
(_, StackRename)               -> begin_rename_stack(state)
(_, StackMove)                 -> match flow { None => begin_move_stack(state),
                                               MoveStack{..} => cancel_move(),   // toggle, the task-move rule
                                               _ => inert }
(_, BoardCreate)               -> begin_create_board()
(_, BoardDelete)               -> delete_board(state)
(_, BoardRename)               -> begin_rename_board(state)
```

The edit-flow SetMode(Normal)/Activate arms extend to
`CreateBoard | RenameStack | RenameBoard` (discard/commit); the
`leaving` pre-match extends likewise (commit edit flows, cancel
move flows — `MoveStack` joins `Move`). One asymmetry recorded: the
existing `(Flow::None | Flow::Move{..}, SetMode(Mode::Insert))`
arm (vi's `i`/`a` open a create flow mid-move) is **not** extended
to `Flow::MoveStack` — vi `i` during a stack move falls to the
default arm (a plain mode flip, the flow surviving until
Escape/confirm). Keeping `i`-during-move out of the new flow avoids
opening a task-create row while a stack move targets an insertion
slot; the cost is a mode flip the user escapes the same way F-E2's
fall-through allowed before this chunk fixed it for tasks.

## 7. Context menus (D21; the F-B7/F9 fix)

### 7.1 One chain for every object menu (recorded decision)

The design's per-object chains ("task → [Board, Global]" — the
dispatch-prompt fix; "stack → [StackDetail, Global]";
"board → [Global]-appropriate") are unified to **`[Board, Global]`
for all three**. Recorded reason: every stack/board verb's bindings
live in the Board context (§3.1) and their registry availability is
Board-tagged (`stack.wider`'s contexts are `[Board]`) — over a
`[StackDetail, Global]` or `[Global]` chain those rows are not even
*visible* to `chain_visible` (`src/menu.rs:39-48`), so the design's
own entry lists (Wider/Narrower on the stack menu) would render
nothing and every hint would stay empty — the exact F-B7 bug this
chunk exists to fix. `[Board, Global]` makes every §2 row visible
and every §3 binding hint resolve.

### 7.2 Projection shape (menu.rs)

`context_entries` generalizes from "the Task group page" to an
explicit per-object action list projected through the existing
`row()` (hints included):

```rust
pub fn context_entries(keymap: &Keymap, chain: &[ContextId],
                       ids: &[ActionId]) -> Vec<MenuEntry>
```

- `TASK_CONTEXT` — unchanged behaviour, new spelling: the Task
  group's rows in rank order (T-4's equality-with-group-page pin is
  kept by asserting against `group_page(Task)`); now joined by
  `task.reopen` (85) and `task.move.cancel` (95) automatically.
- `STACK_CONTEXT = [TaskCreate, StackMove, StackRename, StackWider,
  StackNarrower, StackDelete]` — D21's set ("Move stack, Rename
  stack, Delete stack, Wider/Narrower, New task"), destructive
  last, no parent rows, no Quit pin.
- `BOARD_CONTEXT = [TaskCreate, StackCreate, BoardCreate,
  BoardRename, BoardDelete]` — D21's set ("New task, New stack,
  New board, Rename board, Delete board").

Enablement gating (dispatch prompt's "which entries are
enablement-gated"): **none are statically gated** — registry rows
stay `enabled: true` and gating is dispatch-time, the established
`NoTarget`/inert machinery: `New task` on a zero-stack board is
inert (`create_target_stack` → `None`); `Delete board` with no
current board is inert; `Reopen task` on a not-done task is
`NoTarget` (§4). Recorded: this matches `task.complete`-on-no-focus
exactly (the design's "same enablement machinery").

### 7.3 Open seams & select-on-right-click (decision)

`open_task_context_menu` generalizes to
`open_context_menu(window, x, y, ids)` (one method; `MenuAnchor::
Pointer(x, y)`, `MenuState` reset, `sync_menu_models` + `sync` —
the existing pipeline, D12's sync included). Right-click **selects
first**, the card precedent (F-6):

- **Stack header** (`board-layout.slint` header TouchArea, both the
  regular header and the compact switcher segments — the same
  object class): `pointer-event` right-down (the task-card pattern,
  F-11) emits a new `header-context(int, float, float)` callback →
  window seam `on_board_header_context_requested(column, x, y)` →
  adapter: resolve `tables.stack_ids[column]` (stale index absorbed
  silently), `select_column` (selection + focus, the existing
  header-click seam), open `STACK_CONTEXT` at the pointer. The
  compact switcher reuses the same callback with
  `effective-selected()` addressing.
- **Board-bar segment** (`board-bar.slint` segment TouchArea):
  `segment-context(int, float, float)` →
  `on_board_segment_context_requested(index, x, y)` → adapter:
  resolve `tables.board_ids[index]`, `select_board` (the segment's
  click path — recorded: right-clicking a segment switches to that
  board, consistent with right-click-selectes everywhere), open
  `BOARD_CONTEXT` at the pointer.
- Callback signatures carry window-coordinate floats exactly as
  `task-card.slint:85-91` computes them
  (`absolute-position + mouse-x/y`), so the anchor rule is
  identical to the task menu's.

### 7.4 Field surfaces for the new flows (Slint)

- **Board create/rename fields**: window-level slot (single mount in
  `app-window.slint`, always visible while its flow is open — covers
  every breakpoint, the empty-board leg, and vi-from-settings
  exposure (F-13) with one site). Properties: `board-create-open:
  bool`, `board-rename-open: bool`, `board-flow-initial: string`
  (seeded buffer, the `TaskDetailModel.initial_text` mount-only
  rule). The `TextField` mirrors via `set_buffer`, Return commits
  (`commit_text`), Escape routes `mode.normal` (the discard arm).
- **Stack rename field**: per-column, inside the locked-height
  header (`StackColumnModel` gains `rename_open: bool` +
  `initial_title: string`; the header swaps its title row for the
  field while open — the height lock (r2 D3) is preserved). Compact
  leg: the field mounts atop the selected column (no header exists
  there).
- **Move-stack highlight**: §5.5 (model-derived, no new surface).

## 8. Snapshot regeneration list (same commit as the rows; eyeballed, recorded in the commit body)

1. `tests/snapshots/keymap_snapshots__registry.snap` — 12 new rows,
   group split/re-rank of 6 moved rows.
2. `tests/snapshots/keymap_snapshots__resolved_preset_keymaps.snap`
   — 46 new binding rows (§3.1: standard 16 = 9 Board verbs +
   Escape + first/last ×3 contexts; emacs 18 = those + the `Ctrl+g`
   cancel alternate + the `stack.create` fill; vi 12 = 11 ViNormal
   rows + the ViNormal Escape of §3.4).
3. No golden regen (F-12 — any golden diff is a finding).

## 9. Test catalogue (TDD; design §10 discipline — typed outcomes, no text assertions)

### 9.1 Keymap/registry layer (`src/keymap/mod.rs` tests, `src/actions.rs` tests)

- `registry_covers_every_action_exactly_once` extended to the 12 new
  ids (46 total).
- `every_preset_covers_the_core_verbs`: core list extended with
  `StackCreate` (the emacs gap becomes contract) + all 12 new
  actions — the R3 "every preset binds every new action" gate.
- New `every_preset_resolves_the_move_cancel_in_the_board_chain`
  (the dispatch (b) contract): `first_binding_display(TaskMoveCancel,
  [Board, ViNormal, Global])` is `Some` for all three presets.
- New mirrors: stack-nav verbs resolve over the board chain
  (extends the width-verbs contract shape) for
  `StackNext/StackPrev/StackDelete/StackRename/StackMove`;
  board-lifecycle verbs likewise.
- **The C-g pin** (dispatch (c)): resolver test — emacs: board
  chain, lone `Ctrl+g` → `Action(TaskMoveCancel)`; task-detail
  chain, `Ctrl+g` → `Pending`, then `Ctrl+/` → `Action(AppRedo)`.
- T7/T8 run unweakened over the grown tables (no edits needed —
  §3.3 verified; if stage 2 finds a slip, the binding is re-picked,
  never the contract).

### 9.2 Router layer (`src/router.rs` tests)

- `TaskReopen`: done+focused → exact `SetTaskDone { done: false }`;
  not-done → `NoTarget`; unfocused → `NoTarget` (the enablement
  pin, dispatch (f)).
- The eleven new arms route to their named effects (the
  `view_intents_route_to_effects` table extended).

### 9.3 View-state layer (`src/view_state.rs` tests)

- `step_stack`: wraps both directions; `None` selection enters at
  the direction end; focus lands on the new stack's first task;
  zero stacks inert.
- `jump_focus`: first/last over the reading order from both ends;
  idempotent at the target; empty board inert; compact domain.
- `begin/commit/discard` for `CreateBoard`/`RenameStack`/
  `RenameBoard`: seeded buffers, exact typed commands on commit,
  empty-buffer commits nothing, discard clears.
- `delete_stack`/`delete_board`: exact commands with the resolution
  chain; inert without targets.
- Move-stack flow: begin at current slot, cycle wraps over
  `0..=len`, confirm dispatches the minimal `MoveStack` re-normalization
  set (assert the exact command vec for a mid-board move — e.g.
  [A,B,C,D] move B after C → B:2, C:1 — and that an in-place
  confirm dispatches nothing), cancel dispatches nothing,
  reconcile ends the flow on vanish.

### 9.4 Menu layer (`src/menu.rs` tests)

- T-1/T-5 extended for the two new groups (six parents on the root
  page, declaration order).
- New: `context_entries` over each object list — flat, no Quit,
  hints **non-empty for every row whose §3.1 binding is
  Board/ViNormal-visible over `[Board, Global]`** (the F-B7 fix
  pin, structural: `!row.hint.is_empty()` for bound rows — never
  hint-string assertions); the task menu still equals the Task
  group page (T-4 preserved).

### 9.5 Journeys (`tests/journeys.rs`; the harness patterns at
`the_context_menu_focuses_and_routes_the_card` / `stack_create_has_
a_visible_click_equal_flow`)

- **Move cancel per preset** (dispatch (c)): for each preset, start
  a move (`M` / `Ctrl+c Ctrl+w` / `g m`), assert the flow visible
  (`move_target ≥ 0`), press `Escape` (all presets) / `Ctrl+g`
  (emacs) → flow gone, **zero commands drained**. Extra pin:
  emacs outside a move, board view, `Ctrl+g` → still zero commands
  (inert cancel), and `Ctrl+g Ctrl+/` from the **task-detail**
  chain resolves (pending then action) — the journey-level echo of
  the C-g pin.
- **Stack wrap** (dispatch (d)): at the last stack, `stack.next`
  lands on the first with focus on its first task; the reverse for
  `stack.prev`; per-preset strokes.
- **Dispatch-pinning journeys** (dispatch (e)/(f)): board create
  via keyboard and via the board menu (click ≡ key): field opens,
  typed title + Return drains exactly `CreateBoard { title, color:
  Color::new("") }`; board delete drains `DeleteBoard` for the
  selected board; stack delete (keyboard + header menu) drains
  `DeleteStack` for the selected stack; reopen on a completed
  focused task drains `SetTaskDone { done: false }` and is a no-op
  dispatch on an open task.
- **First/last** (dispatch (g)): from each end, the jump lands on
  the far end's card (`emphasized_card`), repeat is a stay.
- **Stack-move flow**: begin (context menu + key), cycle, confirm
  drains the exact `MoveStack` set; Escape mid-flow drains nothing.
- **Rename flows**: header menu Rename → field mounts seeded
  (model pin), commit drains `RenameStack`; board rename likewise;
  Escape discards (no command).
- **Hint presence** (dispatch (h)): after opening each object menu,
  the entries model carries non-empty hints for the bound rows
  (structural loop over the pushed `menu_entries`).
- **Menu-open journeys** (dispatch (h)): header right-click
  (`invoke_board_header_context_requested`) opens the pointer-
  anchored menu with exactly `STACK_CONTEXT`'s ids (no Quit, no
  parents); segment right-click likewise with `BOARD_CONTEXT`;
  invoking entries routes (e.g. stack-menu Wider drains nothing but
  steps the width request — reuse the resize-jney assertion shape).
- Existing journeys that change shape (recorded, not weakened):
  `the_context_menu_focuses_and_routes_the_card` — the task menu
  now also contains `task.reopen`/`task.move.cancel` rows (its
  presence assertions still hold verbatim; any exact-list
  assertions extend).

## 10. Commit sequence for stage 2 (each standalone, rebase-merge hygiene)

1. `feat(ui-slint): the r2 registry round-up rows + three-preset
   bindings (D16-D21)` — actions.rs, presets.rs, contracts, both
   snapshots regenerated in-commit.
2. `feat(ui-slint): the router arms for the r2 verbs (D16-D21)` —
   effects, reopen gate, ctx field + tests.
3. `feat(ui-slint): move-cancel + stack nav + first/last in the
   view state (D16/D17/D20)` — interpret arms, step_stack,
   jump_focus + tests.
4. `feat(ui-slint): board/stack lifecycle flows (D18/D21)` — flows,
   commit/discard/reconcile arms, move-stack re-normalization +
   tests.
5. `feat(ui-slint): object context menus with rendered hints
   (D21/F9)` — menu.rs lists/groups, chain fix, adapter seams,
   models + tests.
6. `feat(ui-slint): right-click seams + flow fields on the board
   surface (D21)` — slint changes + journeys.
7. `docs(changelog): the r2 actions round-up` — Unreleased entries
   (stage-2 obligation, dispatch prompt 3.2).

## 11. Code inventory & estimated size

`src/actions.rs` (+~140), `src/keymap/presets.rs` (+~90),
`src/keymap/mod.rs` tests (+~90), `src/router.rs` (+~110),
`src/view_state.rs` (+~420), `src/adapter.rs` (+~260),
`src/menu.rs` (+~110), `src/models.rs` (+~70),
`src/shell.rs` (+~15), `ui/compositions/board-layout.slint`
(+~90), `ui/compositions/board-bar.slint` (+~15),
`ui/app-window.slint` (+~35), `tests/journeys.rs` (+~700),
`tests/snapshots/*` (regen), `tests/keymap_snapshots.rs` (0).
**Estimated total ≈ 2,300–2,900 added lines**; size class L, in
line with the dispatch table.

## 12. Risks (round §14-R3 + chunk-specific)

- **R-a Keymap collisions** (R3): every candidate run through T7/T8
  mentally (§3.3); two design proposals re-picked with recorded
  reasons (Ctrl+Alt-arrows WM seizure; vi `g g` collision). The
  contracts run unweakened in CI — any implementation slip is a
  re-pick, never a weakening.
- **R-b The C-g trade-off**: emacs redo is unreachable from the
  board view (accepted; redo is disabled debt #353 — re-pick when
  it ships). Pinned at resolver level so the semantics can't drift
  silently.
- **R-c Event delivery of design-picked chords** (Tier-4, user
  runs): `Ctrl+Space` (std reopen) is the IME toggle on
  ibus/fcitx desktops; `Alt+<`/`Alt+>` (emacs first/last) may
  deliver as the base key with Alt on some backends. The
  resolver/snapshot pins are delivery-independent; if Tier-4 finds
  seizure, the follow-up re-pick is a one-row preset change +
  snapshot regen.
- **R-d Move-stack re-normalization bursts** a command per reordered
  stack on a bound board (each an outbox op, F-9). Boards carry few
  stacks (tens); a mid-board move renumbers ≤ the suffix. Accepted;
  if the outbox chatter matters, a domain-side compound command is
  backlog material (`backlog:` issue per AGENTS §7 if observed).
- **R-e Select-on-right-click switches boards** (segment menu):
  right-clicking a non-selected segment selects it — heavier than
  the card precedent but consistent ("the menu addresses what you
  clicked"); recorded as the deliberate semantics.
- **R-f Destructive deletes without confirmation** (D18's explicit
  scope cut): Delete board/stack act immediately; undo does not
  exist (#356). The engine's typed refusals are the only guard.
  Design-owned; restated to the user in the PR's Tier-4 list.
- **R-g Snapshot churn**: the group split re-ranks six existing
  rows — the registry snapshot diff is bigger than the additive
  minimum; reviewed in the commit body per the regen discipline.

## 13. Tier-4 manual checklist (handed to the user at PR time)

1. Emacs: start Move Task, press `C-g` — cancels; press `C-g C-/`
   outside the board view — still resolves redo (disabled no-op
   today, so: no crash, no stray dispatch).
2. Standard: `Ctrl+Space` on a completed task reopens it (if the
   desktop IME seizes the chord, report — fallback chord is a
   one-row change).
3. Emacs: `Alt+<` / `Alt+>` jump to first/last card (same
   delivery caveat).
4. Right-click a stack header, a compact switcher segment, and a
   board-bar segment: menus open at the pointer, entries carry
   binding hints, invokes act on the clicked object.
5. Move-stack flow feel: highlight legibility, wrap at both ends,
   confirm reorders as previewed.
6. Rename-stack field feel inside the locked header; board
   create/rename field at the window slot; Escape discards each.
7. Delete stack/board feel without a confirmation dialog (design
   D18's cut — flag if it feels dangerous, a confirmation modal is
   a future round).

## 14. Bookkeeping (dispatch §10)

Stage 3 posts `✅ DONE` on **#229** (task reopen) and **#232**
(first/last-item navigation) after the PR is green, with one-line
notes + the PR link. No other issue closes; out-of-scope ideas
found during stages go to `backlog:` issues per AGENTS §7 (e.g. a
board colour picker for the create-board flow, R-d's compound
stack-move command if the outbox chatter shows up).
