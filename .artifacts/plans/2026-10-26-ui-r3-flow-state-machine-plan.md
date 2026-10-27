# [Plan] UI R3 Chunk 1 — Flow State Machine (W-SM): audit, rules R1–R5, total interpret table, title re-edit

- **Date**: 2026-10-26 (stage-1 plan written 2026-10-27)
- **Branch**: `fix/ui-r3-flow-state-machine` (worktree
  `../taskboard-WT-fix-ui-r3-flow-state-machine`, cut from `origin/main`
  @ `4c0911d`; round-3 artefacts merge verified — `f29dde1`).
- **Normative inputs**: design round
  `2026-10-26-gui-review-round3-plan.md` §3.1 (W-SM-a…d) + §1 findings
  7/8/9/10/18/26 + §4 + §7-R1; dispatch plan §1/§2 (chunk 1, acceptance
  a–j). Non-goals (dispatch §2): no board-surface visuals (chunk 2), no
  due-picker changes (chunk 3), **no typestate introduction** (W-SM-c).
  Fences (design §2): DR-2…DR-6 and undo-v1 territories untouched; the
  empty diff over `taskboard-domain`/`taskboard-app` is acceptance (e).
- **All `file:line` anchors below are current-branch** (verified on
  `fix/ui-r3-flow-state-machine` @ 4c0911d; the design round cited the
  pre-kiosk-wave numbers, which have drifted).

## 0. TDD order (binding for stage 2)

1. **Transition-table test first** (§10.1): written against the audit
   matrix below, it must **fail on today's code** (the `(_, _) =>
   Vec::new()` default arm at `adapter.rs:783` classifies dozens of
   pairs as silent no-ops). Any existing test that pinned a broken
   behaviour is rewritten with a recorded-decision comment naming §-refs
   here (known rewrites: `view_state.rs:2796`
   `activate_focused_opens_the_title_edit_with_the_mirror_seeded`, the
   auto-arm half of `insert_intent_in_task_detail_edits_the_subject_in_place`
   `:2824`, and the Return-opens-title expectations inside the
   ten-step keyboard journey).
2. Then the focus-loss seam (§8), the total table (§4), the drag fix
   (§5), visibility (§6), title (§7), buffers (§9) — each red→green.
3. Full AGENTS §10 gate suite green; CHANGELOG Unreleased updated
   (stuck-state fixes + title-edit rework entries) — stage 2.

## 1. Deliverable 1 — the audit report (W-SM-a)

Stage 2 writes `.artifacts/reports/2026-10-26-ui-flow-state-audit-report.md`
as the **first commit** (docs), from the outline below. Evidence mandate
applies: every claim carries a `file:line` anchor from this branch.

1. **Flow inventory** — the 13 `Flow` variants (`view_state.rs:160-204`)
   with their subject/buffer data and their end paths (commit / discard
   / cancel / clean-close / reconcile-drop), each anchored:
   `commit_edit` `view_state.rs:1094-1168`, `discard_edit` `:1170-1175`,
   `end_edit_due` `:934-937`, `cancel_move` `:1457-1463`,
   `confirm_drag_move` `:1447-1454`, `close_palette`/`close_capture`,
   `reconcile`'s vanish-drops `:1655-1676`.
2. **The six breakage mechanisms** (findings F7/F8/F9/F10/F18/F26),
   anchored as verified on this branch:
   - F7 click-out with empty buffer: `commit_edit` only ends flows on
     Activate/SetMode(Normal)/leaving-effects; pointer clicks dispatch
     no effect (or land in the default arm). The armed flow holds
     `Mode::Insert`, and `KeymapShell::chain` (`shell.rs:62-82`) masks
     the view context with `[ViInsert, Global]` — every view key dies;
     `text-field.slint` exposes only `edited`/`accepted` (`:35-36`, the
     seam list at `:86-87`), so there is no focus-loss event at all.
   - F8 stale-column drop: `on_board_card_dropped` early-returns on an
     out-of-tables column (`adapter.rs:2490`) **without settling**;
     `on_board_card_drag_finished` then early-returns on
     `completed == true` (`:2515-2517`), so the armed `Flow::DragMove`
     survives a completed drop whose confirm was swallowed.
   - F9 dead buttons: `BeginCreate | BeginCreateStack | Activate |
     RequestMove` are matched only under `Flow::None`
     (`adapter.rs:699-705`); under any other flow they fall to
     `(_, _) => Vec::new()` (`:783`). The same arm kills `OpenMenu`
     under a flow (the hamburger routes through `interpret`), and
     emacs `C-g` (`CancelMove`) inside the due editor.
   - F10 invisible empty-board flow: `begin_create_stack` arms
     `Flow::CreateStack` and sets Insert regardless of stack count, but
     the field renders only in the `stacks.length > 0` legs
     (`board-layout.slint:538`, `:616`); the empty-board leg (`:146`)
     has no field — the flow opens with no affordance (R5 violation).
   - F18 due-editor swallow: the `(Flow::EditDue, effect)` arm
     (`adapter.rs:690-696`) eats everything before the `OpenSettings`
     arm (`:769`) — switching to Settings silently does nothing; same
     swallow kills `CancelMove` (emacs `C-g`) under EditDue.
   - F26 title: task-detail entry auto-arms via `activate_focused`
     (`view_state.rs:941-950`, reached from `interpret_begin`'s
     Activate arm `adapter.rs:877`); after one close, re-arming needs
     `ModeInsert` (vi-only `i`/`a`, `presets.rs:339-340`) — emacs and
     standard have no path — and the closed title
     (`task-view.slint:112-117`) has no pointer seam.
3. **Pointer-path hole map** — the settle rule runs only on
   *leaving-effects* (`adapter.rs:611-626`,
   `settle_flows_on_leaving :545-597`), never on pointer seams; the
   helper inventory that **bypasses `interpret` entirely** (each becomes
   an L2 settle site, §4.4): `focus_board_card :2968-2981`,
   `select_column :2984-2996`, `on_task_due_edit_opened :3214-3222`
   (calls `begin_edit_due` directly — silently replaces any live flow),
   the context-menu seams `:2608-2759`, `today_row_clicked/activated
   :2656-2675`, `board_add_task_activated :2937`, `board_new_stack_clicked
   :2867`, `board_bar_segment_activated :2845`, `commit_text :3001`.
4. **Buffer-lifetime map** (input to §9): every read/write of
   `due_buffer`/`due_echo`/`due_picker` (`adapter.rs:123-125`, set
   `:3231-3236`, wiped only in `commit_due :3360-3362`), `width_drag`
   (`:136`, set `:2337`, taken only at release `:2390`), `scale_drag`
   (`:139`, set `:2022`, taken only at commit/revert `:2047`).
5. **Target-state matrix** — the §4.2 table, as the normative
   classification the code table and the test matrix both mirror.
6. **Affordance table** (§6) and **coverage map** (§10 catalogue →
   acceptance a–j).

## 2. Deliverable 2 — settle rules per flow kind (R1 + R2)

One settle taxonomy, **two trigger families**:

- **L1 — focus-loss** (§8): the TextField's new `focus-lost()` seam.
  Covers clicks on any focus-grabbing surface (all four view roots are
  `FocusScope`s — `board-view.slint:26`, `today-view.slint:41`,
  `stack-view.slint:19`, `task-view.slint:40` — so *dead-zone* clicks
  move focus), Tab, and programmatic steals.
- **L2 — seam-settle**: every pointer/menu seam that bypasses
  `interpret` (the §1.3 inventory) plus the new effect table (§4) routes
  through one settle step before its own semantics.

**The per-kind settle rule** (identical for L1 and L2; one pure
`view_state` method, e.g. `settle_flow(&mut self, state) ->
Vec<FlowOutcome>`, so both triggers cannot diverge):

| Flow kind | Flows | Pointer-outside / settle rule |
|---|---|---|
| TEXT | `Create`, `CreateStack`, `CreateBoard`, `EditTitle`, `RenameStack`, `RenameBoard` | buffer trimmed non-empty → `commit_edit` (dispatches the create/rename); empty → `discard_edit`. **F7 dies**: typing nothing + clicking out ends the flow, `Mode::Normal` returns, the chain unmasks. |
| DUE | `EditDue` | clean close (`end_edit_due` + `due_picker = None`) — **never** commits on an unrelated click/focus-loss (the r2 D25 clean-close precedent; the only commit path stays the `commit_due` funnel). `due_buffer`/`due_echo` wiped (§9). |
| MOVE | `Move`, `MoveStack` | `cancel_move` — never dispatches a move on an unrelated click. |
| DRAG | `DragMove` | `cancel_move` **+ `teardown_runtime_drag`** (the native-gesture exit is the one public cancel, `adapter.rs:566-574` precedent). |
| MODAL | `Palette`, `Capture` | their own close paths (`close_palette` / `close_capture`). Note: pointer-outside barely applies (the overlay covers the window); L1 covers Tab/steals; the existing dismiss seams stay. |
| NONE | `Flow::None` | inert no-op (the settle call must be **idempotent** — this is the double-commit guard, §8.3). |

R2's "gestures always settle" is the DRAG row + §5's early-out kills;
the r2 gesture journeys (`the_drag_survives_interleaved_refreshes_and_engine_changes`,
`drag_seam_resizes_live_and_persists_on_release`) must stay green — the
fix must not settle a *live, on-tables* gesture.

## 3. Deliverable 3 — the yield/settle table that replaces the default arm (R3 + R4)

### 3.1 Shape (totality by construction)

`interpret` keeps its flat `match (&flow, effect)`; the `(_, _) =>
Vec::new()` default arm **dies**. The final arm becomes
`(_, effect) => self.resolve_conflict(&flow, effect, &state)` — a named
classifier whose own `match` is exhaustive over an `EffectClass` enum
(no silent default anywhere; the compiler enforces each `match`'s
totality). `resolve_conflict` returns one of:

```rust
enum Resolve {
    /// settle (the §2 rule for this flow kind), then re-run the effect
    /// once against the settled state (bounded one-level re-dispatch —
    /// the InvokeSelected precedent, `router.rs:52-54`). Re-dispatch is
    /// guarded: only if the settle actually changed the flow.
    SettleThenRun,
    /// settle only — the effect is consumed by the settle (CancelMove).
    CancelOnly,
    /// replace the flow without committing (explicit exceptions only).
    Replace,
    /// typed no-op — unreachable-by-construction or recorded-inert.
    Inert,
}
```

Because every arm resolves to a named class, **no `(Flow, effect)` pair
is silent**; the transition-table test (§10.1) pins every pair to the
audit matrix and fails when a new `Flow`/`UiEffect` variant lacks a row.

### 3.2 The matrix (normative — mirrored into the audit report)

Effect classes (all 37 `UiEffect` variants + the new
`BeginTitleEdit`, §7): **NAV** = NextView, PrevView, FocusBoard,
BoardNext, BoardPrevious, BoardSelect, FocusToday, OpenInbox,
OpenSettings, FocusNext, FocusPrev, FocusFirst, FocusLast, StackNext,
StackPrev, StackWider, StackNarrower · **LIFECYCLE** = StackDelete,
StackRename, StackMove, BoardCreate, BoardDelete, BoardRename ·
**BEGIN** = BeginCreate, BeginCreateStack, RequestMove · **MODAL-OPEN**
= OpenPalette, BeginCapture · **EDIT** = Activate, SetMode(Normal),
SetMode(Insert) · **CANCEL** = CancelMove · **MENU** = OpenMenu,
CloseMenu, MenuBack, MenuNext, MenuPrevious, InvokeSelected ·
**SYSTEM** = Exit · **TITLE** = BeginTitleEdit (new).

| Flow ↓ / class → | NAV | LIFECYCLE | BEGIN | OpenPalette | BeginCapture | EDIT | CANCEL | MENU | SYSTEM | TITLE |
|---|---|---|---|---|---|---|---|---|---|---|
| `None` (kept arms) | run | run | begin (`interpret_begin`) | open | replace (D8) | begin/open | inert | open/run | run | inert (no subject) |
| TEXT | **S** | **S** | **S** (F9 dies: New Task/New Stack always work) | **S** | **X** | commit/discard (kept arm) | **I** | OpenMenu **S**; rest run | run | **S** |
| DUE | **S** (F18 dies: OpenSettings works) | **S** | **S** | **S** | **X** (picker dropped) | SetMode(Normal) = kept discard arm; Activate **I** (named: the field's `accepted`→`commit_due` funnel owns Return); SetMode(Insert) **I** | **K** (emacs C-g lives again) | OpenMenu **S**; rest run | run | **S** |
| MOVE | **S** (generalizes today's leaving-only settle) | **S** | **S** | **S** | **X** | kept `interpret_move` arms | cancel (kept) | OpenMenu **S**; rest run | run | **S** |
| DRAG | **S** (+ native teardown) | **S** (+ teardown) | **S** (+ teardown) | **S** (+ teardown) | **X** (+ teardown) | kept arms: Activate = confirm; SetMode(Normal)/RequestMove = cancel; FocusNext/Prev = **I** (named: the pointer owns targeting mid-gesture) | cancel + teardown (kept) | OpenMenu **S**; rest run | run | **S** (+ teardown) |
| PALETTE | **S** (today: OpenSettings under a palette leaves the flow armed — latent overlay bug, named in the audit) | **S** | **S** | inert (already open) | **X** | **I** (row seams own activation) | **K** | run (chain head is `[Palette, ViInsert, Global]`) | run | **S** |
| CAPTURE | **S** | **S** | **S** | **I** (one overlay) | inert | Activate = `commit_capture`, SetMode(Normal) = close (kept arms) | **K** | run | run | **S** |

Legend: **S** = SettleThenRun · **K** = CancelOnly · **X** = Replace ·
**I** = Inert · plain text = today's kept explicit arms (unchanged).

### 3.3 Named decisions inside the matrix

- **R4 resolution — auto-settle, not status feedback**: `Begin*` under a
  foreign flow settles (per the flow's kind rule) then runs. The status
  line is *not* extended (r2 D28 contract untouched); the visible result
  of the settled-then-run action is the feedback. The alternative
  (surface + refuse) is rejected: it re-creates a "must press Escape
  first" world with extra words.
- **No queued effects in chunk 1**: W-SM-b R3's "explicitly queued"
  branch is reserved; the matrix chooses settle-then-run everywhere a
  queue could have been used. If a future effect genuinely cannot run
  after a settle, it lands in this matrix as an explicit new class —
  never in a wildcard.
- **The capture exception is kept and named** (chunk-10 D8's fire-drill
  replace-without-commit; the one **X** for `BeginCapture` everywhere).
- **Menu-internal effects** (CloseMenu/MenuBack/MenuNext/MenuPrevious/
  InvokeSelected) classify run-through: they only arrive while the menu
  is open, and an open menu masks flows (`chain` → `[Menu]`).
  `InvokeSelected` re-dispatches actions whose effects re-enter this
  table — the menu's "New Stack" row works under a flow via **S** (the
  F9 menu face).
- **Settle ordering with the leaving rule**: `settle_flows_on_leaving`
  (`adapter.rs:545-597`) stays for the leaving-effect set (it also
  dismisses the tooltip) but its per-flow arms are refactored to call
  the **same** pure `settle_flow` as L2/`resolve_conflict` — one
  taxonomy, three triggers (§2).
- **L2 — the pointer-helper settle** (§1.3 inventory): one
  `Adapter::settle_pointer_conflict(window, &state)` called at the top
  of `focus_board_card`, `select_column`, the context-menu seams,
  `on_task_due_edit_opened`, the today-row seams, the bar-segment and
  new-stack clicks, and `board_add_task_activated`. Same pure rule;
  idempotent on `Flow::None`.
- `Exit` runs everywhere (process teardown; flows die with the window).

## 4. Deliverable 4 — the drag settle fix (R2; F8)

Two early-outs **die**; one stays:

1. **`on_board_card_dropped`'s stale-column early-out dies**
   (`adapter.rs:2490`): `let Some(stack) = … else { return }` becomes
   *settle-cancel* — `cancel_move()` + `teardown_runtime_drag(window)` +
   the outcomes applied. A drop that cannot resolve its column leaves
   **no** armed flow and no ghost; the move dispatches nothing (a missed
   drop must not guess a target).
2. **`on_board_card_drag_finished`'s `if completed { return }` dies**
   (`adapter.rs:2515-2517`): the handler settles **whenever the flow is
   still armed**, `completed` or not. Semantics: `completed == true`
   with the flow still armed ⇒ the drop seam's confirm was swallowed
   (exactly the stale-column race) ⇒ cancel (the runtime said the
   gesture ended; nothing dispatched). `completed == false` ⇒ cancel
   (today's behaviour, kept). The `try_borrow_mut` echo-absorption
   **stays** (`:2518-2520`): the synthetic-teardown echo fires inside an
   `interpret` borrow with the flow already gone — absorption is correct
   there, and it cannot mask a legitimate settle (a legitimate settle is
   never called from inside that borrow).
3. The **flow guard stays** as the final backstop; after 1+2 it is
   unreachable except for the echo.

Regression harness: the r2 drag journeys (interleaved-refresh survival,
live resize, drop ≡ keyboard-confirm equivalence `view_state.rs:1973`)
plus the new drop-miss journey (§10.2). `reconcile`'s vanish-drop
(`view_state.rs:1664-1666`) is unchanged.

## 5. Deliverable 5 — the visible-flow rule per flow (R5; F10)

Every flow must render an affordance whenever armed; each row below
gets a structural visibility pin (§10.2). The one *implementation* gap
is the empty board:

| Flow | Visible affordance (pinned) |
|---|---|
| `Create{stack}` | the inline row's TextField in the stack column (board + stack-detail) |
| `CreateStack` | the field in **all three board legs** — the empty-board leg (`board-layout.slint:146`) gains the same field the wide (`:538`) and compact (`:616`) legs have. **This is the chunk's minimal F10 fix**; the full centered-CTA empty-board page stays chunk 2 (D5) |
| `CreateBoard` | the window-level board-name field |
| `EditTitle` | the title TextField (`task-view.slint:103`) |
| `EditDue` | the due editor surface (typed field; picker when toggled) |
| `Move` | the moving card's dim + target highlight (model-pushed classes) |
| `MoveStack` | the insertion indicator — **audit item**: verify what r2 D21 renders; if nothing, the fallback is a status-line hint pushed at arm/cycle/confirm (recorded decision in stage 2) |
| `DragMove` | the ghost (`DragState`) + source dim + target highlight |
| `Palette` | the palette overlay panel |
| `Capture` | the capture overlay |

Because R4 auto-settles, a lingering flow can no longer outlive its
affordance: any action that would unmount it settles first (§3).

## 6. Deliverable 6 — title-edit redesign (W-SM-d; F26)

- **Entry never arms**: `activate_focused` (`view_state.rs:941-950`)
  stops setting `Flow::EditTitle`/Insert — it becomes the plain
  "open the focused task's detail" (cycle-entry parity; the existing pin
  `cycle_entry_into_task_detail_never_opens_an_edit_flow :2200` already
  states the target semantics). Its four test call-sites rewrite under
  the recorded-decision rule (§0).
- **New registry row** (actions.rs, `ActionEntry` shape beside `TaskEdit`
  `:667-676`): `ActionId::TaskTitleEdit`, name `task.title-edit`, label
  "Edit title", contexts `[TaskDetail]`, `menu_group: Task`,
  `menu_rank` 72 (beside `task.edit`'s 70), description "Arms the title
  editor for the viewed task." Router: → new `UiEffect::BeginTitleEdit`
  → `view_state.begin_edit_title` (the `edit_subject` body, generalized
  to take the subject task; `EditTitle` arms seeded from the task's
  title).
- **Bindings — re-arm exists in every preset** (conflict contract runs
  in stage 2; snapshots regenerate):
  - standard: `TaskTitleEdit @ TaskDetail = F2` (free — `F2` is
    StackRename@Board only; TaskDetail@standard carries nav rows only,
    `presets.rs:140-148`; F2 = the CUA rename convention, matching the
    preset's own `StackRename = F2`).
  - emacs: `TaskTitleEdit @ TaskDetail = Return` (free at TaskDetail —
    emacs binds Return only at Board/Today for `task.edit`,
    `presets.rs:207/220`; with auto-arm gone, Return-at-detail is
    unambiguous).
  - vi: the existing `ModeInsert` route (`i`/`a` @ ViNormal →
    `interpret_insert` → TaskDetail arm → title editor, `adapter.rs:859`)
    **is** the bound action (rows `presets.rs:339-340` — the audit
    names them); the new registry row is menu-visible in vi without a
    second key binding (no duplicate-binding conflict).
- **Pointer seam + grammar**: the closed title (`task-view.slint:112-117`)
  gains the due-cell grammar (`task-view.slint:146-160` precedent):
  TouchArea click → routes `TaskTitleEdit` (click ≡ key, §7.1), hover
  surface + cursor, `accessible-role: button` with label/description +
  `accessible-action-default`. The task context menu keeps its Edit
  entry.
- **Full-width field**: the title row stops nesting the field in the
  back-button row's centered column; the field stretches to the detail
  body's full width (the display RoleText already elides full-width).
  Geometry pin: the field's width ≈ the detail body width (the D22
  layout-cell probe shape).
- `TaskEdit`'s registry description updates ("Opens the focused task's
  detail view." — the "title selected for editing" clause dies with the
  auto-arm); label "Edit task" stays (the menu row still opens the
  detail).

## 7. Deliverable 7 — the TextField focus-loss seam (probe recorded)

**Probe verdict (run 2026-10-27, pinned Slint 1.18.1,
`i-slint-backend-testing` 1.18.1, scratch crate `/tmp/focus-probe` —
evidence, not committed):**

1. **Mechanism A works** — the compiler-widgets idiom
   (`i-slint-compiler-1.18.1/widgets/common/lineedit-base.slint:23`):
   a component-level property derived from the inner `TextInput`'s
   `has-focus` + `changed` fires exactly once per focus loss — on
   pointer-click-away (a sibling `FocusScope`) *and* on programmatic
   steals. No spurious fire at init-focus (gained ticks `0`).
2. **Mechanism B exists** — the builtin `FocusScope` element carries
   `focus-gained(reason)` / `focus-lost(reason: FocusReason)`
   (`builtin_elements.rs:1429-1434`; reasons `programmatic`,
   `tab-navigation`, `pointer-click`, `popup-activation`,
   `window-activation`; verified `programmatic` fires). **`TextInput`
   itself does not** — it declares only `has-focus`/`accepted`/`edited`
   (`builtin_elements.rs:~2302`), so the seam must live at the
   `TextField` wrapper level via mechanism A.
3. **Negative control (decisive for the architecture)**: a pointer
   click on a plain `TouchArea` — including one *inside* the field —
   does **not** move focus; `has-focus` stays true and no seam fires.
   ⇒ Focus-loss alone cannot implement R1; the L2 seam-settle (§3.3)
   carries the TouchArea-driven paths, and L1 covers everything that
   grabs focus (all view roots are FocusScopes, so dead-zone clicks are
   L1-covered after all).

**The seam** (`ui/widgets/text-field.slint`):

```slint
callback focus-lost();
property <bool> input-has-focus: input.has-focus;   // derived, mechanism A
changed input-has-focus => {
    if (!input-has-focus) { root.focus-lost(); }
}
```

Every `TextField` call-site with a text flow behind it (create rows,
title, rename/board fields, palette, capture) forwards `focus-lost` to
a new window callback `on_<flow>_focus_lost` → adapter
`settle_pointer_conflict` (the §2 rule; commit non-empty / discard
empty / clean-close due). Non-flow fields (settings, search input) do
not forward it in this chunk.

**Guards (pinned by tests):**

- **No double-commit**: ending a flow unmounts the field, which can
  fire `focus-lost` as a teardown echo — the settle is idempotent on
  `Flow::None` (§2), and the echo fires inside `apply_outcomes`' borrow,
  so the seam's handler uses the `try_borrow_mut`-absorb discipline
  (the `drag_finished` echo precedent). Journey: click-out with text →
  exactly **one** `CreateTask`; pressing Return afterwards dispatches
  nothing.
- **Alive-seam correctness**: a genuine focus move *outside* any borrow
  must settle — borrow succeeds, settle runs.

## 8. Deliverable 8 — buffer-lifetime invariants (W-SM-c)

The adapter's one-shot/gesture buffers may never outlive their owning
flow or surface:

- **`due_buffer` / `due_echo` / `due_picker`** (`adapter.rs:123-125`):
  today wiped only in `commit_due` (`:3360-3362`) — they survive
  discard, leaving-settle, and reconcile drops. Invariant: **non-empty
  ⇔ `Flow::EditDue` is armed**. Enforcement home: the end of
  `refresh()` (which every outcome path and settle flows through,
  `adapter.rs:449-452` where `reconcile` runs) — one wipe point:
  `if !matches!(flow, EditDue) { due_buffer.clear(); due_echo.clear();
  due_picker = None; }`. A `debug_assert!` documents the invariant; the
  journeys pin it behaviourally (after F18's settle-then-open-Settings,
  reopening the due editor shows no stale echo).
  (Chunk 3's D11 buffer *seeding* builds on this wipe — boundary noted.)
- **`width_drag`** (`:136`): lifetime = the live column-resize gesture.
  Dropped by the settle step on a leaving effect / flow settle (the
  header unmounts ⇒ the release can never arrive). Must **not** be
  dropped by refresh (the r2 interleaved-refresh survival pins the
  gesture across refreshes).
- **`scale_drag`** (`:139`): lifetime = the live slider gesture inside
  settings; dropped on settings close (`close_settings`) and on any
  settle triggered while it is `Some` (the settle step drops both
  gesture buffers — they are flow-independent by construction, so the
  settle step owns them, not the per-flow rule).

## 9. Deliverable 9 — test catalogue

### 9.1 The transition-table test (the regression harness; §7-R1)

- Shape: one `const` expectation matrix
  `[(FlowKind, EffectClass, Expectation); N]` + a test that iterates
  **every** `(Flow, UiEffect)` pair (13 flows × 38 effects), looks the
  pair up, and asserts the typed outcome class (commands dispatched /
  mode flipped / flow after / inert). A missing row fails; a new
  variant without a row fails ⇒ **the totality is compiler- plus
  test-enforced**.
- **Must fail on today's code** (the default arm classifies dozens of
  pairs `Inert`) — written first (§0).
- Lives beside the adapter tests; the fixtures reuse the existing
  `view_state` test fixtures.

### 9.2 Journeys (headless driver — `click_at`/`press_stroke` are real
`WindowEvent`s, `tests/support/driver.rs:214-250`)

1. **F7 click-out**: begin create → type nothing → click a dead zone →
   flow `None`, `Mode::Normal`, zero commands; New Task ghost then
   works. Twin: type text first → click out → exactly one `CreateTask`;
   Return after dispatches nothing (double-commit guard).
2. **F8 drop-miss**: arm drag → stale-column drop (index past the
   tables) → flow cancelled, no `MoveTask`, no ghost (model push); the
   `completed = true` echo after a swallowed confirm also cancels.
3. **F9 buttons under any flow**: for each TEXT flow armed: click New
   Stack ghost (settles + opens stack-create), menu-invoke "New Stack",
   hamburger menu opens; under `DragMove`: `BeginCreateStack` cancels +
   tears down + opens the row.
4. **F18 due→settings**: due editor open (typed text present) →
   `OpenSettings` → settings open, due flow closed, **no** `UpdateTask`;
   emacs `C-g` (`CancelMove`) inside the due editor closes it cleanly.
5. **R5 visibility pins**: one per flow — the affordance element is
   present while armed (incl. the empty-board create field).
6. **F26 title**: Return on a focused card opens detail with **no**
   field armed and `Mode::Normal`; click closed title → field armed +
   seeded; commit → closed; click again → re-arms (the exact F26 repro,
   now green); vi `i`, emacs `Return`, standard `F2` each re-arm;
   empty-commit closes without dispatch; full-width geometry pin.
7. **Focus-seam component pin** (mechanism A idiom, in-tree — the
   `tests/virtualisation.rs` `slint!`-host pattern): click a sibling
   FocusScope fires `focus-lost` exactly once; a TouchArea click does
   not (the recorded negative).
8. **Buffer invariants**: after every end path of EditDue (commit /
   discard / settle / reconcile-vanish) the buffers read empty;
   leaving-effect drops `width_drag`/`scale_drag`; a live gesture
   survives refresh (kept r2 pins).

### 9.3 Kept-green set (regression)

All `view_state.rs:1830-3700` flow pins (with the §0 recorded
rewrites), the r1/r2 journeys touching flows (`journeys.rs:261` ten-step,
`:1194`/`:1218` stack-create, `:1752` add-task ghost, `:1981+` settings
choreography, `:3740` move-cancel, `:4141` rename flows, `:4195`
commit-affordance), the keymap snapshot suite (regenerated for the new
rows), and the r2 drag journeys.

## 10. Tier-4 manual checklist (this machine; stage 2 ships it in the report)

For each of the user's five scenarios — exact repro steps + expected
post-fix behaviour:

1. **F7**: open New Task row → type nothing → click another card / dead
   space → row closes, board keys work, no second click needed.
2. **F8**: drag a card across stacks; drop on a stale/edge column (e.g.
   during a background sync refresh) → move either applies cleanly or
   cancels cleanly; never a stuck ghost; no Escape needed.
3. **F9**: arm a create row (typed) → click New Stack / New Task /
   hamburger → each acts (the typed row commits); nothing "flashes and
   does nothing"; Escape never required.
4. **F10**: empty the board (fresh profile) → New Stack opens a
   **visible** field on the empty board; typing + Return creates the
   stack.
5. **F18**: open the due editor → press the settings shortcut →
   settings opens (due editor closed, value untouched); returning to the
   task shows the old due.
6. **Click-out feel**: with text typed, click-out commits (not silently
   discards); with an empty row, click-out discards with no toast noise;
   verify no double-create when click-out is followed by Return.

## 11. Risks & recorded decisions

- **R-design §7-R1 (invasiveness)**: the matrix + transition-table test
  is the harness; kept-arm semantics never change byte-wise (the table
  lists them as "kept").
- **Focus-loss teardown echo** (§7 guards): absorption vs settle is
  borrow-state-determined — pinned by the double-commit journey.
- **`MoveStack` affordance unknown** (§5): verification is an audit
  item; fallback (status hint) recorded if absent.
- ** emacs C-g change** (CancelMove under EditDue/TEXT as K): a
  behaviour change beyond the strict F18 letter — recorded here as
  intended (the "editing modes must allow or cancel-then-allow, never
  fail silently" finding sentence covers it).
- **Fences**: no domain/app diffs; `due_edit.rs` parse/seed untouched
  (chunk 3); board visuals beyond the one empty-leg field stay chunk 2;
  DR-2…DR-6/undo untouched.

## 12. Size & bookkeeping

- Estimated diff: **≈ 1.1–1.4k lines** across ~14 files —
  `view_state.rs` (+~150: `settle_flow`, `begin_edit_title`,
  `activate_focused` rework), `adapter.rs` (+~250/−~90:
  `resolve_conflict`, `settle_pointer_conflict`, drag fixes, refresh
  wipe, seam forwards), `router.rs` (+1 effect), `actions.rs` (+1 row),
  `keymap/presets.rs` (+2 rows) + snapshots, `text-field.slint` (+~8),
  `task-view.slint` (+~45), `board-layout.slint` (+~15), tests
  (+~600).
- Commits (stage 2, standalone): docs(audit) → test(transition matrix,
  red) → feat/fixed per §3-§8 → docs(Tier-4 + CHANGELOG). No push, no
  PR from stage 2; stage 3 rebases, reviews vs acceptance (a)–(j),
  ships.
