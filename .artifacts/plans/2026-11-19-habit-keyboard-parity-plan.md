# Habit Keyboard Parity (round 9, chunk 8 / H3) — Implementation Plan

- **Date**: 2026-11-19
- **Kind**: dispatch chunk plan (stage 1), TDD-first
- **Branch**: `feat/ui-habit-keyboard-parity` (worktree
  `../taskboard-WT-feat-ui-habit-keyboard-parity`, cut off `origin/main`
  8ac7230c)
- **Normative sources**: UX journey plan
  `.artifacts/plans/2026-11-18-habit-parity-ux-plan.md` (US-3, US-4 —
  ACCEPTED); round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md` §1 O-7
  (Q5: #935 rides this round) and §H3; research
  `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
  (§1.3 keyboard/reorder rows, §1.4 #934/#935, §2 HAB-6/9/10, HAB-13).
- **Preconditions verified 2026-11-19**: chunk 6 (PR #1028,
  `feat/ui-habit-row-rework`) MERGED; its seams are on `main`
  (`menu_row_subject`, `habit_context_page`/`HABIT_CONTEXT`,
  `HabitMoveUp`/`HabitMoveDown` one-shots).
- **Chunk 9 (directional focus) status: NOT landed** (no PR, no
  branch). Binding strategy per the dispatch plan: this chunk
  implements against the **existing traversal seam**
  (`NavNextItem`/`NavPrevItem` over the Today flat walk — no new
  `nav.focus-*` directional ids), and the directional-focus follow-up
  is recorded in §9. Chunk 9, when it lands, widens the same walk; no
  rework of this chunk's ids is expected because none are coined in
  the directional space.

## 1. Scope

Close #934 (habit keyboard traversal + verb keys + keyboard reorder)
and the habit slice of #935 (uniform typed `NoTarget` refusal at
`route()`), plus habit drag reorder via the #918 gap-drop pattern
(US-4 AC-1). Everything renders behind the chunk-6 `RowSubject`
substrate; kiosk gates: traversal/complete render, reorder + verb
keys gate on `editing-enabled` (HAB-14, the D-P13 grammar survives).

Non-goals: habit editing/delete keys (H4's reserved verbs — no dead
rows), compact-move *ceremony* parity for habits (filed as backlog —
this chunk ships the per-step `HabitMoveUp/Down` grammar only), task
compact-move on Today, chunk 9 directional ids.

## 2. Settled decisions

### D1 — No new habit verb ActionIds

The chunk-6 family is complete for this chunk:
`HabitLog`, `HabitUndoLog`, `HabitRepair`, `HabitMoveUp`,
`HabitMoveDown`, `HabitConvertArm`, `HabitConvertToSeries`,
`HabitCreate`, `HabitCreateCommit` (crate/taskboard-ui-slint/src/actions.rs::ActionId,
slugs at `Actions::slug`). Keyboard parity binds **existing** ids —
click ≡ key by construction (the chunk-6 law). No `HabitEdit`/
`HabitRemove`/`HabitMenu` ids are coined (H4's reserved verbs).

### D2 — Traversal: widen the Today walk domain to `RowSubject`

The walk domain is tasks-only today:
`today_domain` returns `Vec<TaskId>` (crate/taskboard-ui-slint/src/view_state.rs::ViewState::today_domain)
and the focused fact is `focused_task: Option<TaskId>`
(crate/taskboard-ui-slint/src/view_state.rs::ViewState::focused_task).
Changes:

- New walk row union `TodayRow` (enum `Task(TaskId) | Habit(HabitId)`)
  in `view_state.rs`; `today_domain` is widened into
  `today_rows(state, kiosk, day) -> Vec<TodayRow>` — task sections in
  the existing order, then the habit section in board-scoped order
  (the `habits` list order; the same order `IdTables.habits` keys).
  `today_domain` stays as a thin adapter over `today_rows` for the
  call sites that genuinely need `TaskId`s.
- New focused fact `focused_habit: Option<HabitId>` beside
  `focused_task` (the two are mutually exclusive; the walk enforces
  it — stepping onto a habit row clears `focused_task` and vice
  versa). This is the missing "#934 focus fact": the research's
  grep confirmed no such symbol exists; chunk 8 added only the
  menu one-shot `habit_convert_target`.
- `step_flat_domain` is generalized over `TodayRow`
  (crate/taskboard-ui-slint/src/view_state.rs::ViewState::step_flat_domain);
  the TaskDetail/compact arms are untouched (habit rows are
  Today-only — census R-1).
- Traversal is mount/refresh driven via the existing `FlowOutcome`
  machinery; no new hints beyond the existing focus-scroll hint leg,
  which gains the habit list as a filter target (the per-ListView
  hint filter reads it like the other legs).

Closes US-3/AC-1 (#934 traversal half). Kiosk: rows enter the walk;
verb keys gate at route.

### D3 — Key translation: the focused-subject resolver at the dispatch seam

Task rows bind bare keys (`TaskComplete` → `Space`,
`TaskEdit` → `Return`, keymap/presets.rs Today rows). Duplicating
those strokes for the habit ids in the same context is impossible
(one stroke, one binding per context) and would fork the grammar.
Instead: **the focused-subject key translation** — at the dispatch
entry (crate/taskboard-ui-slint/src/adapter.rs, the `route()` entry /
`Resolution::Match` arm), when the resolved id is a task row verb and
the focused subject is `RowSubject::Habit`, the id is translated to
its habit twin before `route`:

| Focused subject | Stroke resolves | Translated to |
|---|---|---|
| Habit | `TaskComplete` (`Space`) | `HabitLog` |
| Habit | `TaskEdit` (`Return`) | (H4 reserved — **no translation**; the stroke falls through to the `NoTarget` refusal of §D6 rather than a dead row) |
| Habit | the row context-menu key (the same id the task row's keyboard menu invocation uses) | opens the Object menu with `RowSubject::Habit` subject (the chunk-6 `menu_row_subject` path — `habits-context-requested`'s keyboard twin) |

The translation is a pure function
`translate_for_subject(id, subject) -> ActionId` (presentation.rs),
unit-tested table-style. `TaskDelete` is *not* translated (no habit
delete verb until H4). This keeps the preset tables the single source
of stroke truth (the keymap coverage gate keeps binding counts
unchanged for task verbs).

### D4 — Keyboard reorder: `Shift+Up` / `Shift+Down` → the chunk-6 one-shot pair

The `HabitMoveUp`/`HabitMoveDown` one-shots exist and carry
`(HabitId, i64 slot-order)` (crate/taskboard-ui-slint/src/router.rs::RouteCtx one-shot
fields `habit_move_up`/`habit_move_down`; minted at
crate/taskboard-ui-slint/src/adapter.rs around the menu-open path via
`presentation::habit_drop_order`). Grammar:

- Preset bindings: `HabitMoveUp` → `Shift+Up`, `HabitMoveDown` →
  `Shift+Down` in `ContextId::Today`, **in all three preset tables**
  (keymap/presets.rs — the three lists at lines ~341/~695/~1096).
  The board's shift-walk selection extension is Board-scoped
  (R-1, adapter.rs shift-walk gate), so Today's `Shift+Up/Down` is
  free; the arrows' shift-fold leniency applies only to
  printable-bound rows, so the shifted arrows reach the keymap intact.
- Dispatch: when the focused subject is a habit row, the adapter
  mints the one-shot pair from the habit's position — prev/next
  neighbor orders → `habit_drop_order` midpoint
  (crate/taskboard-ui-slint/src/presentation.rs::habit_drop_order) —
  then routes `HabitMoveUp`/`HabitMoveDown` exactly as the menu row
  does. Same ids, same routing arm, click ≡ key.
- On a focused **task** row the same stroke resolves `HabitMoveUp/
  Down` → `NoTarget` → §D6's typed refusal (task compact-move on
  Today is the filed backlog, not silently absorbed).
- Kiosk: the pair routes `MoveHabit` which is an editing verb —
  gated by the `editing-enabled` grammar at the existing gate
  (HAB-14). Binding stays visible; dispatch refuses.

Closes US-3/AC-2 (#934 keyboard-reorder half; HAB-10).

### D5 — Drag reorder: the #918 gap-drop pattern over the habit list

The board's gap-drop machinery (crate/taskboard-ui-slint/src/adapter.rs
`Flow::Move`/`Flow::StackMove` interpret arms — `target-a` aimed
column, `target-b` the gap's clamp row, midpoint order, nonce
re-edge) is the pattern; the habit list is a single flat list, so
only the gap half applies:

- `ui/compositions/habits.slint`: the row card gains drop-target
  geometry — `habits-drop-asked(row, gap: int)` callback (the
  `reorder-asked(row, slot)` chevron seam's gap-shaped sibling; the
  chevron pair stays — it is the pointer-free fallback and the
  journey test's control).
- Adapter: `habits-drop-asked` computes the target order from the
  bracketing neighbors via `habit_drop_order(prev, next)` (the same
  function the menu pair mints through — one arithmetic, three
  invocation paths) and dispatches `StateCommand::MoveHabit`.
  Clamp law identical to the board's `min(gap, len-1)`.
- No drag *visual ceremony* beyond the gap indicator the board rows
  already render (reuse, not a new hint family).

Closes US-4/AC-1 (HAB-9). Kiosk-gated like D4.

### D6 — #935 (habit slice + the uniform law): typed `NoTarget` refusal at `route()`

Today the arm `DispatchOutcome::Disabled | DispatchOutcome::NoTarget => {}`
(crate/taskboard-ui-slint/src/adapter.rs::Adapter::route, ~line 1637)
absorbs every target-less dispatch. Per O-7 Q5:

- **Split the arm**: `Disabled` keeps its current posture (the
  disabled-row notice is already armed at the registry arm); `NoTarget`
  invokes the existing every-click-reacts notice primitive
  (crate/taskboard-ui-slint/src/adapter.rs::Adapter::push_notice).
  One law, at one place — the uniform pass, not per-verb spot fixes
  (the #935 issue text's exact ask). The habit verbs' NoTarget
  producers (subject-less dispatch, stale one-shots, un-armed
  convert, Shift+Up on a task row) are covered by construction.
- Typed tests only: a `NoTarget` dispatch (e.g. `HabitConvertArm`
  with no subject) asserts the notice surfaced (the notice flag/
  counter the existing push_notice tests already read — structural
  fact) and that no command left. No text assertions.
- The comment at adapter.rs:4229 ("the silent NoTarget arm is
  unchanged — the plan's backlog") is updated; #935 closes with this
  chunk only if the reviewer agrees the one law covers the issue's
  scope — otherwise the issue stays open with a comment pointing at
  the landed law (the issue asks for exactly this uniform pass, so
  it should close).

### D7 — Preset & chorded-letter assignments (collision-checked)

Convention (chunk-1 case-law): chorded letters are
`Ctrl+Shift+<lowercase>`. Taken letters on main (all preset tables):
a b f g h j k l m n p q r s t z. Free: c d e i o u v w x y.
New bindings, all three preset tables:

| ActionId | Stroke | Context | Rationale |
|---|---|---|---|
| `HabitMoveUp` | `Shift+Up` | Today | row-scoped movement grammar; free on Today (board-scoped shift-walk) |
| `HabitMoveDown` | `Shift+Down` | Today | twin |
| `HabitConvertArm` | `Ctrl+Shift+c` | Today | the armed confirm lives in the menu (O-7 Q3); the arm key opens the subject's menu flow |
| `HabitUndoLog` | `Ctrl+Shift+u` | Today | −1 valve parity with `Space`=+1 |

Deliberately **unassigned** (left for chunk 9's directional ids and
H4's reserved verbs): c… remaining letters d e i o v w x y. No
round-8 collisions (`Ctrl+Shift+c`/`u` absent from main), no chunk-1
re-homes touched (`HabitCreate` keeps `Ctrl+Shift+h`). The vi-normal
preset table carries the same four rows (its deviations live in the
table, the grammar is identical).

## 3. TDD sequence (each step red → green → commit)

1. **`translate_for_subject`** (presentation.rs): table tests for the
   D3 map + the no-translation cases. Commit `test(ui)` + `feat(ui)`.
2. **Today walk widening** (view_state.rs): `today_rows` ordering
   (task sections then habits, board-scoped), `focused_habit`
   exclusivity, `step_flat_domain` over `TodayRow` — property test
   for order stability + unit tests for the mutual-exclusion law.
   Snapshot: a Today walk with 2 tasks + 3 habits steps end-to-end
   (insta). Commit.
3. **Keyboard reorder** (adapter.rs + presets.rs): preset entry test
   (all three tables), one-shot minting test (midpoint arithmetic at
   list head/middle/tail — floor law from `habit_drop_order`'s
   existing table), kiosk refusal test. Commit.
4. **Drag gap-drop** (habits.slint + adapter.rs): seam test
   `habits-drop-asked` → `MoveHabit` order at head/gap/tail + clamp;
   journey test us4_s1. Commit.
5. **NoTarget refusal** (adapter.rs): typed notice test (structural),
   no-command assertion, Disabled-arm unchanged pin. Commit.
6. **Journey tests** (`tests/habits_journey.rs`): us3_s1 (traversal
   enters/leaves habit rows with task keys), us3_s2 (verb keys on a
   focused habit ≡ pointer path), us4_s1 (gap-drop across the list).
   These are the UX plan's bound tests — the §6 wiring rows W-6 and
   the US-3/US-4 acceptance evidence. Commit.

## 4. Census deltas (symbol anchors, #775 convention)

- `keymap/presets.rs` habit.create row → gains siblings
  (`habit.move-up/move-down/convert-arm/unlog`); census cell
  "Keyboard: only HabitCreate bound" superseded with dated entry
  (2026-11-19, this plan's PR).
- `presentation.rs::habit_drop_order` → invocation count grows from
  1 to 3 paths (menu pair, keyboard pair, drag gap) — census
  reorder row re-homed.
- `adapter.rs::route` NoTarget arm → dated census entry (the T-19
  family gains the NoTarget twin).
- No R-1/R-2/R-3 changes (Today-only mount stands; no board column).

## 5. Risks / watch items

- **`Shift+Up/Down` shadowing**: if any Today-surface row handler
  consumes shifted arrows before the keymap (the printable fold
  leniency is arrows-adjacent), the binding starves. Mitigation: the
  preset entry test dispatches the stroke through the full
  resolution path, not just the table.
- **Walk widening touches `step_flat_domain`**, shared by the
  TaskDetail arm — the generalization must keep the detail arm's
  `Vec<TaskId>` behavior byte-identical (snapshot pins it).
- **`focused_habit` + menu one-shots interplay**: the menu-open path
  already mints `habit_move_up/down` from `menu_row_subject`; the
  keyboard path mints from the walk focus. Two mint sites, one
  arithmetic — the mint helper must be shared or the journeys will
  diverge (TDD step 3 asserts both paths through the same helper).
- #935's closure scope (D6): if stage 3 reads the issue as broader
  than the route arm, leave it open with the comment + follow-up
  note (the dispatch plan's recorded-follow-up law).

## 6. Follow-up note (chunk 9 unlanded — recorded binding)

This chunk coins **no** ids in the directional space and binds no
`nav.focus-*` slugs; traversal rides `NavNextItem`/`NavPrevItem` over
the widened walk (D2). When chunk 9 lands its directional ids, they
replace nothing here — they add view-level jumps over the same
`TodayRow` domain. The plan for chunk 9 should cite §D2's `TodayRow`
union as the walk domain it jumps across.

## 7. Diff estimate

| File | ± lines |
|---|---|
| view_state.rs (D2 + tests) | ~220 |
| adapter.rs (D3/D4/D5/D6 + tests) | ~300 |
| presentation.rs (translate_for_subject + tests) | ~90 |
| keymap/presets.rs (4 bindings × 3 tables) | ~30 |
| actions.rs | ~0 (no new ids) |
| ui/compositions/habits.slint (focus styling + drop seams) | ~90 |
| tests/habits_journey.rs + keymap/menu tests | ~350 |
| **Total** | **~1080** |

## 8. Quality gates

Per AGENTS §10, via `scripts/run-quality-gates.sh` from the worktree;
pre-commit hooks + flock-wrapped `cargo nextest`; the coverage trio
never runs inside this stage. `cargo fmt`/`clippy -D warnings` clean
before each handoff; `#![forbid(unsafe_code)]` and SPDX headers on
any new file (none expected).
