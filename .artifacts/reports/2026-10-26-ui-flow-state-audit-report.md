# [Report] UI flow-state audit — GUI review round 3, W-SM-a (chunk 1)

- **Date**: 2026-10-27 (evidence re-anchored on the chunk branch).
- **Branch**: `fix/ui-r3-flow-state-machine` @ `4c0911d` — every
  `file:line` anchor below is from this branch (the design round cited
  pre-kiosk-wave numbers, which drifted).
- **Inputs**: design round `2026-10-26-gui-review-round3-plan.md`
  §3.1 (W-SM-a…d), §1 findings 7/8/9/10/18/26; stage-1 plan
  `2026-10-26-ui-r3-flow-state-machine-plan.md` §1 (outline) and §3.2
  (the normative target matrix, mirrored as §6 here).
- All paths relative to `crates/taskboard-ui-slint/`.

## 1. Flow inventory

The 13 `Flow` variants (`src/view_state.rs:161-204`), their subject
data and their end paths:

| Flow | Subject/buffer | End paths |
|---|---|---|
| `None` | — | target of every end path; inert (idempotence required) |
| `Create { stack }` | buffer | commit `commit_edit` `view_state.rs:1094`; discard `discard_edit` `:1171`; leaving-commit `adapter.rs:545-597`; reconcile drop |
| `CreateStack` | buffer | same as `Create` (arms via `begin_create_stack` `view_state.rs:1079`) |
| `EditTitle { task }` | buffer (title mirror) | commit/discard as above; arms via `activate_focused` `:941` (auto-arm) and `edit_subject` `:912` (vi `i`/`a`) |
| `EditDue { task }` | adapter `due_buffer`/`due_echo`/`due_picker` (`adapter.rs:123-125`) | the one commit funnel `commit_due` `adapter.rs:3345`; clean close `end_edit_due` `view_state.rs:934` (leaving rule, `adapter.rs:585-590`); discard on Escape `adapter.rs:690-696` |
| `Move { task, target }` | — | confirm `confirm_move` → `append_move_command`; cancel `cancel_move` `view_state.rs:1457`; leaving-cancel |
| `MoveStack { stack, slot }` | — | same family as `Move` |
| `DragMove { task, target }` | model-pushed ghost | confirm `confirm_drag_move` `view_state.rs:1447`; cancel + native teardown (`teardown_runtime_drag` `adapter.rs:848`); reconcile vanish-drop `view_state.rs:2019` |
| `Palette { selected }` | buffer (query) | `close_palette` `view_state.rs:1274`; jump ends it (`palette_jump`) |
| `Capture` | buffer | `commit_capture` (keeps the flow) / `close_capture` `:1236` |
| `CreateBoard` | buffer | `commit_edit` arm; `begin_create_board` |
| `RenameStack { stack }` | buffer | `commit_edit` arm |
| `RenameBoard { board }` | buffer | `commit_edit` arm |

`reconcile`'s vanish-drops (`view_state.rs:1617`, drag case `:2019`)
are the background-refresh safety net.

## 2. The six breakage mechanisms (findings F7/F8/F9/F10/F18/F26)

- **F7 click-out with empty buffer** — a pointer click outside the
  create row dispatches *no effect* (the view roots are FocusScopes;
  the row's own seams are `edited`/`accepted` only), so the armed
  `Flow::Create` holds `Mode::Insert` and `KeymapShell::chain`
  (`src/shell.rs:62-82`) masks the view context with
  `[ViInsert, Global]` — every view key dies. `text-field.slint`
  exposes only `edited`/`accepted` (`ui/widgets/text-field.slint:35-36`,
  seam list `:86-87`): there is no focus-loss event at all. The
  leaving rule (`adapter.rs:545-597`) runs on *effects*, never on
  pointer clicks.
- **F8 stale-column drop** — `on_board_card_dropped` early-returns on
  an out-of-tables column (`adapter.rs:2490`) *without settling*;
  `on_board_card_drag_finished` then early-returns on
  `completed == true` (`adapter.rs:2515-2517`), so the armed
  `Flow::DragMove` survives a completed drop whose confirm was
  swallowed by the stale-column race.
- **F9 dead buttons under a flow** — `BeginCreate`,
  `BeginCreateStack`, `Activate`, `RequestMove` are matched only under
  `Flow::None` (`adapter.rs:753-758`); under any other flow they fall
  to the wildcard `(_, _) => Vec::new()` (`adapter.rs:780`): the New
  Task / New Stack affordances flash and do nothing.
- **F10 invisible empty-board flow** — `begin_create_stack`
  (`view_state.rs:1079`) arms `Flow::CreateStack` + Insert regardless
  of stack count, but the field renders only in the populated legs
  (`ui/compositions/board-layout.slint:538`, `:616`); the empty-board
  leg (`:147-157`) has no field — the flow opens with no affordance
  (R5 violation).
- **F18 due-editor swallow** — the `(Flow::EditDue, effect)` arm
  (`adapter.rs:690-696`) catches *every* effect before the global
  `OpenSettings` arm (`adapter.rs:769-777`): switching to Settings
  under the due editor silently does nothing. The same swallow kills
  emacs `C-g` (`CancelMove`) inside the due editor.
- **F26 title re-arm impossible** — task-detail entry auto-arms via
  `activate_focused` (`view_state.rs:941-950`, reached from
  `interpret_begin`'s Activate arm `adapter.rs:877`); after one close,
  re-arming needs `ModeInsert` (`interpret_insert` → `edit_subject`),
  i.e. vi-only `i`/`a` (`keymap/presets.rs:339-340`) — emacs and
  standard have no path — and the closed title
  (`ui/views/task-view.slint:113-117`) has no pointer seam.

## 3. Pointer-path hole map

The settle rule runs only on *leaving effects* (`adapter.rs:611-626`
computing `leaving`; `settle_flows_on_leaving :545-597`) — never on
pointer seams. Helpers that bypass `interpret` entirely (each becomes
an L2 settle site):

- `focus_board_card` `adapter.rs:2968` — card click;
- `select_column` `adapter.rs:2984` — header/switcher click;
- the due editor's open seam `adapter.rs:3214-3222` — calls
  `begin_edit_due` directly, silently replacing any live flow;
- the context-menu seams `adapter.rs:2621`, `:2645`, `:2682`, `:2736`,
  `:2755` (`open_context_menu :2580`);
- `today_row_clicked/activated` `adapter.rs:2656+`;
- `board_add_task_activated` `adapter.rs:2937`;
- `board_new_stack_clicked` `adapter.rs:2867`;
- `board_bar_segment_activated` `adapter.rs:2845`;
- `commit_text` `adapter.rs:3001` — the text seams' commit path.

## 4. Buffer-lifetime map

- `due_buffer` / `due_echo` / `due_picker` (`adapter.rs:123-125`):
  set at `on_task_due_edited` `:3231-3236`; wiped **only** in
  `commit_due` `:3360-3362`. They survive discard, leaving-settle and
  reconcile drops — violating *non-empty ⇔ `Flow::EditDue` armed*.
- `width_drag` (`adapter.rs:136`): set `:2337`, taken only at release
  `:2390` — survives a settle that unmounts the header.
- `scale_drag` (`adapter.rs:139`): set `:2022`, taken only at
  commit/revert `:2047` — survives settings close paths that do not
  pass its own release.

## 5. Menu/system effects: an application-layer fact (audit correction)

The stage-1 plan (§1.2 F9) expected `OpenMenu` to die under a flow via
the wildcard. On this branch the menu/system effects are applied in
`apply_effect` (`adapter.rs:958-1040`: `Exit :958`, `OpenMenu :962`,
`InvokeSelected :977`, `CloseMenu :992`, `MenuBack :1001`,
`MenuNext/Previous :1020`) *before* the interpretation table — the
table's fall-through arm routes only the remaining effects. So the
hamburger does open under a flow today; what it does **not** do is
settle the armed flow first (the menu opens over an armed text flow,
whose `Mode::Insert` keeps masking keys after dismiss). The fix keeps
the application-layer home and adds the settle step there; inside the
table the MENU/SYSTEM classes are typed inert (unreachable by
construction, recorded in the matrix below).

## 6. Target-state matrix (normative, mirrored from plan §3.2)

Effect classes — **NAV** = NextView, PrevView, FocusBoard, BoardNext,
BoardPrevious, BoardSelect, FocusToday, OpenInbox†, OpenSettings,
FocusNext, FocusPrev, FocusFirst, FocusLast, StackNext, StackPrev,
StackWider, StackNarrower · **LIFECYCLE** = StackDelete, StackRename,
StackMove, BoardCreate, BoardDelete, BoardRename · **BEGIN** =
BeginCreate, BeginCreateStack, RequestMove · **MODAL-OPEN** =
OpenPalette, BeginCapture · **EDIT** = Activate, SetMode(Normal),
SetMode(Insert) · **CANCEL** = CancelMove · **MENU** = OpenMenu,
CloseMenu, MenuBack, MenuNext, MenuPrevious, InvokeSelected ·
**SYSTEM** = Exit · **TITLE** = BeginTitleEdit (new, W-SM-d).
† OpenInbox is unlisted in the plan's NAV enumeration; it is a pure
read-side view jump like FocusToday and is classified NAV here
(recorded decision).

| Flow ↓ / class → | NAV | LIFECYCLE | BEGIN | OpenPalette | BeginCapture | EDIT | CANCEL | MENU | SYSTEM | TITLE |
|---|---|---|---|---|---|---|---|---|---|---|
| `None` | run | run | begin | open | replace (D8) | begin/open | inert | run (application layer) | run | open-or-inert (needs a viewed subject) |
| TEXT | **S** | **S** | **S** (F9 dies) | **S** | **X** | commit/discard (kept) | **I** | OpenMenu **S**; rest run | run | **S** |
| DUE | **S** (F18 dies) | **S** | **S** | **S** | **X** (picker dropped) | SetMode(Normal) kept discard; Activate/SetMode(Insert) **I** (the `accepted`→`commit_due` funnel owns Return) | **K** (emacs C-g lives) | OpenMenu **S**; rest run | run | **S** |
| MOVE | **S** | **S** | **S** | **S** | **X** | kept `interpret_move` arms | kept cancel | OpenMenu **S**; rest run | run | **S** |
| DRAG | **S** (+teardown) | **S** (+teardown) | **S** (+teardown; kept RequestMove = cancel) | **S** (+teardown) | **X** (+teardown) | Activate = confirm; SetMode(Normal) = cancel (kept); FocusNext/Prev **I** (kept: the pointer owns targeting) | kept cancel+teardown | OpenMenu **S**; rest run | run | **S** (+teardown) |
| PALETTE | **S** (latent overlay bug dies) | **S** | **S** | inert (already open) | **X** | **I** | **K** | run | run | **S** |
| CAPTURE | **S** | **S** | **S** | **I** (one overlay) | inert (kept: re-open is inert) | Activate = commit, SetMode(Normal) = close (kept) | **K** | run | run | **S** |

Legend: **S** = SettleThenRun · **K** = CancelOnly · **X** = Replace ·
**I** = Inert · plain text = today's kept explicit arms (unchanged).

Named decisions: R4 auto-settle (no status feedback; r2 D28
untouched); no queued effects in chunk 1; the capture replace-without-
commit exception kept (chunk-10 D8); menu-internal effects run at the
application layer (§5); `settle_flows_on_leaving` refactors onto the
same pure `settle_flow` the L1/L2 triggers use — one taxonomy, three
triggers; `Exit` runs everywhere (flows die with the window).

## 7. Affordance table (R5)

| Flow | Visible affordance (pinned) |
|---|---|
| `Create { stack }` | the inline row's TextField in the stack column |
| `CreateStack` | the board-level field — wide `board-layout.slint:538`, compact `:616`, **empty-board leg `:147` gains the field in this chunk (the F10 minimal fix; the full centered-CTA page stays chunk 2)** |
| `CreateBoard` | the window-level board-name field |
| `EditTitle` | the title TextField (`task-view.slint:112`) |
| `EditDue` | the due editor surface (typed field; picker when toggled) |
| `Move` | the moving card's dim + target highlight (model-pushed) |
| `MoveStack` | **audit item resolved**: r2 D21 renders the insertion indicator via the model push (`src/models.rs:789-792` derives per-stack insert-before flags from `Flow::MoveStack { slot }`); no status-line fallback needed |
| `DragMove` | the ghost (`DragState`) + source dim + target highlight |
| `Palette` | the palette overlay panel |
| `Capture` | the capture overlay |

With R4 auto-settling, a flow can no longer outlive its affordance.

## 8. Coverage map (stage-1 plan §10 catalogue → dispatch acceptance a–j)

| Acceptance | Where |
|---|---|
| (a) this report | §2 mechanisms, §7 affordances |
| (b) total table, no wildcard | `resolve_conflict` + exhaustive `EffectClass` (compiler-enforced), pair-table test |
| (c) R1 click-out commit/discard | focus-loss seam + L2 settle sites, journeys |
| (d) R2 drop-miss | `on_board_card_dropped`/`_drag_finished` settle, journey |
| (e) R3 settle-then-run | matrix rows S, EditDue→Settings journey |
| (f) R4 buttons under any flow | matrix BEGIN rows, journey |
| (g) R5 visibility | §7 table + empty-board field + pins |
| (h) title re-edit | `TaskTitleEdit` registry row, pointer seam, full-width field, journeys |
| (i) empty-board visible flow | empty-board leg field |
| (j) recorded decisions | §5 (menu layer), † (OpenInbox), MoveStack affordance; test rewrites carry in-code comments naming the plan sections |
