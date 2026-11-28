# UI Small Inventory — chunk plan (round 8, B4) — 2026-11-25

Stage-1 deliverable for chunk **B4** (branch `fix/ui-small-inventory`,
dispatch plan `.artifacts/plans/2026-11-25-backlog-reduction-round8-dispatch-plan.md`
§7). Covers the GUI small-fix survivors: #1429 (normative), the
#1208-class leftovers, and anything the round-5/6/7 waves left. Every
item below was verified at current `origin/main` (HEAD `1de55dea` at
inventory time); the inventory is evidence-first — stale items are
recorded as verified-stale, not implemented.

## 1. Inventory

| # | Item | Site (current main) | Disposition | Group |
|---|---|---|---|---|
| #1429 | Right-clicking a habit row opens its menu but never seats `focused_habit` (task seams focus first; the habit seam was specced menu-only before the focus-first discipline) | `crates/taskboard-ui-slint/src/adapter.rs:12957–12964` — `on_habits_context_requested` calls only `open_habit_context_menu`; contrast the left-click seam `on_habits_row_activated` (`adapter.rs:12881–12895`) which seats via `ViewState::activate_habit_row` | **implement** (normative) | A: habit-row focus |
| #1315 | Habit row card tooltip `hint-keys: []` — task rows get theirs from the keymap projection | `crates/taskboard-ui-slint/ui/compositions/habits.slint:373` (hardcoded `[]`); reference grammar `task-card.slint:127–159` | **implement** (r6 A2 deliberate deferral, acceptance per #1311 bullet 2) | B: habit-card presentation |
| #1435 | Edit task/habit views flush against the window edge | `crates/taskboard-ui-slint/ui/views/task-view.slint:510–523` | **implement (residual)** — the horizontal `space-2` inset landed this round (round-8 L1, the E-editor severing inset); the vertical/top edge is still unpadded. Add the vertical leg per the issue's AlarmWindow recipe (`ui/views/alarm-window.slint:90–95`), verify against the kiosk goldens | C: root-view chrome |
| #1478 | A refused `HabitQuotaEdit` menu route keeps the restored pre-close `focused_habit` alive (the survivor exception outruns the refusal) | `crates/taskboard-ui-slint/src/adapter.rs:8299–8339` — `route_menu_action`'s dispatch-scoped disarm: `if action != ActionId::HabitQuotaEdit { self.focused_habit = None; }`; the quota arm's refusal path (`adapter.rs:5598`, `arm_no_target_notice`) never mints, so the restore stands | **implement** (defensive; unreachable through today's menus, the issue records it as deferred-not-fixed from #1303's chunk) | A: habit-row focus |
| #1328 | The MenuBack root-close leg (Escape ladder's final close) bypasses `apply_close_menu` — `menu_entries`/`root_entries`/`menu_chain`/`menu_kind`/`menu_subject` stay stale; the helper's doc law ("reset at every close", `adapter.rs:436`) is false for this leg | `crates/taskboard-ui-slint/src/adapter.rs:7323–7368` — the else-arm disarms habit staging + one-shots (#1115/#1303) but never clears the page models | **implement** — funnel the close leg through `apply_close_menu` (keeping the staging disarm order), or clear the page models in place; one behaviour, pinned at the `MenuBack` effect tier | D: menu close hygiene |
| #1449 | Toolbar title renders the selected board's name on every TaskDetail root (task variant) | `crates/taskboard-ui-slint/src/models.rs:1847–1890` — `push_chrome`'s title match; the habit arm landed this round via #1303 (`models.rs:1881–1886`), the task variant still falls to `board.title` (`models.rs:1885–1887`) | **implement (partial-stale)** — the habit-editor variant is verified-stale (landed); the task-variant decision remains. Plan-of-record: title the open task with its stored title, consistent with the habit arm; stage 2 escalates to the dispatcher if the board-title-as-chrome reading is preferred | C: root-view chrome |
| #1208 | Habit pause editor had no Escape/outside dismissal | closed on the tracker; the dismissal law landed with the pause editor's later work | **verified-stale** (closed) | — |
| #1314 | Habit create from board-free Today offers no target-board choice | open; round 7's B6 explicitly reopened/deferred it — a new user-visible verb → journey-gated | **not in chunk** (journey-gated, already tracked — no new filing) | — |
| #769 / #1360 / #1257 | remaining #1208-class keyboard items (recur Escape pin is a testing pin; the alarm ladder is a two-truth-source restructure; the directional-chord field dead-end is a walk-grammar change) | open, each its own tracked issue | **not in chunk** — none are small GUI fixes; already tracked, no new filings | — |

Surprise finding during verification (recorded, no new issue needed):
#1435's horizontal half was healed mid-round by L1's severing inset —
the issue body predates it; stage 2 implements only the vertical leg
and notes the supersession on the issue at PR time.

## 2. Stage-2 work list (by group)

**Group A — habit-row focus (adapter seams, `adapter.rs`)**
- #1429: in `on_habits_context_requested`, settle the pointer conflict,
  resolve the index through the tables, seat `focused_habit` +
  `ViewState::activate_habit_row` (the exclusivity law,
  `src/view_state.rs` ~2508–2526 clears the task cursor), and open the
  menu only if focus landed — the `on_today_row_context_requested` /
  `focus_today_row` pattern verbatim. Mirror `on_habits_row_activated`'s
  stale-index absorption.
- #1478: on the quota route, clear `focused_habit` when the quota arm
  refuses (or mint the survivor inside the quota arm only on the
  successful-popover path — the issue names both shapes; pick the
  arm-refusal clear, the smaller diff). Typed-outcome test: a refused
  quota route leaves `focused_habit == None`.
- Tests: typed seam-level pins (no text assertions); the #1429 pin
  asserts `focused_habit` after a context-requested at an index, plus
  the menu-open gate (focus landed ⇒ menu opens; stale index ⇒ no
  menu, no panic).

**Group B — habit-card presentation (`habits.slint` + card model)**
- #1315: populate the habit row's `hint-keys` from the keymap
  projection (the `task-card.slint` grammar; the habit card model gains
  the projected first-binding rows). The context-menu chain fix
  (#1311/r6-A2) is a different seam — do not touch it.

**Group C — root-view chrome (`task-view.slint`, `models.rs`)**
- #1435 residual: add the vertical outer padding leg to TaskView's
  root `VerticalLayout` (space-2 vertical per the AlarmWindow recipe;
  horizontal already inset). Verify the kiosk goldens — this moves
  geometry the analyzer and goldens read.
- #1449: extend `push_chrome`'s title match so the `TaskDetail` task
  variant titles with the open task's stored title (the habit arm's
  exact shape, one arm up). Board views untouched.

**Group D — menu close hygiene (`adapter.rs`)**
- #1328: make the MenuBack root-close leg honour the reset law —
  either funnel through `apply_close_menu` (preferred; keep the
  #1115/#1303 staging disarm semantics identical) or clear the page
  models explicitly and fix `apply_close_menu`'s doc claim. One
  behaviour, one test at the lowest layer (the MenuBack effect tier):
  after the ladder's final close, page models are reset.

## 3. Stage-3 traps

- **Goldens move with #1435**: any TaskView-root geometry change
  re-renders editor-family goldens — regenerate through the
  deterministic capture path only, never hand-edit; check the
  layout-review analyzer's expectations (the C1 fix-now class lives
  here).
- **#1429 changes the walk cursor** — the exclusivity law means a
  right-click can now clear a task's `focused_task`; pins elsewhere
  (today-row context tests, walk pins) may legitimately flip. Treat a
  newly failing test as a possible real interaction, not noise.
- **#1478 is unreachable today** — its test must construct the refusal
  path directly (arm-time dead subject), not via a menu walkthrough
  that today cannot produce it.
- **#1449's title text**: assert the title source structurally (the
  pushed `toolbar_title` value equals the stored task title), never
  assert label prose beyond that equality.
- **#1328 ordering**: `apply_close_menu` also routes sync/repaint
  helpers; funneling must not double-`finish_mutation` or reorder the
  habit-staging disarm ahead of the model reset if the close leg's
  current order is load-bearing (read the #1115 rationale before
  rearranging).
- Diff confinement: four surfaces, no new verbs, no journey plan
  required (bug fixes; the #1429 journey note extends the existing
  habits context-menu journey — one journey-amendment line in the PR
  body, not a new plan).

## 4. Stage-1 record

- Worktree `../taskboard-WT-fix-ui-small-inventory`, branch
  `fix/ui-small-inventory` off fetched `origin/main` (`1de55dea`).
- Backlog filings: none new — every out-of-scope finding surfaced by
  this inventory was already tracked (#1314, #769, #1360, #1257).
- Gate: the full nextest run is the dispatcher's between-stage gate;
  stage 1 landed no production code (docs-only commit).

## 5. Journey amendment (2026-11-28, the UI-clip-fixes chunk, plan D5)

R-x (2026-11-28): the task-detail composition gains the workspace's
outer page scroll (ScrollPage substrate) so the flexible description
section is no longer the only vertical absorber; the #1435 vertical
inset legs stay. New affordance: the detail page scrolls when content
exceeds the viewport; the description viewport's internal scroll
remains the nested surface. Journey impact: no story step changes —
the detail's verbs, badges and fields are untouched; S-steps that
assert on-screen presence of below-fold content now assert
scroll-then-visible. Coverage: no W-row changes; the geometry probe
T2 is the wiring evidence.
