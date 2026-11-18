# [Plan] Habit Row Rework (H2, round 9 chunk 6)

- **Date**: 2026-11-18
- **Kind**: implementation plan — TDD-first, single chunk (round plan
  chunk **H2**, branch `feat/ui-habit-row-rework`), stage-1 planning
  artefact for the three-stage dispatch protocol.
- **Origin**: J1 UX journey plan
  (`.artifacts/plans/2026-11-18-habit-parity-ux-plan.md`, ACCEPTED —
  US-1/US-2 + rulings R-1..R-3 normative); round plan §1 **O-2**
  (UI-level `RowSubject`, no domain `TaskKind`) and **O-7** (Q2 no
  board mount, Q3 armed-confirm inside the menu, Q4 editor surface is
  H4's); research
  `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
  §2 (HAB-5/7/8/11/13/14) and §3.3 (the enums-with-data rung); type
  policy AGENTS §2 / `docs/coding_guidelines.org` §2.1.
- **Status**: PLANNED — awaiting stage-2 implementation.
- **Precondition verified**: chunk 4 (H1, PR #1007) merged on main
  2026-11-18T20:40Z — `Habit.description`, `Habit.anchor_day:
  Option<AnchorDay>`, the double-Option `HabitChanges` layers, and
  `nudge_day` are on `origin/main` (c7be6e46). This chunk consumes
  them for display only; the editing surface is H4.
- **Scope**: the Today view's habit rows become task cards behind a
  UI-level row-subject union; the habit right-click context menu with
  the convert submenu (armed confirm inside the menu, O-7 Q3) lands
  here. **Not in scope**: keyboard traversal/verb keys and drag/keyboard
  reorder (H3, #934), the editing modal + reminder nudge GUI seam +
  GUI remove (H4), #935's app-wide notice pass (its own chunk; the new
  habit verbs keep the current typed-NoTarget posture), the M1 quick-bar
  regression (M1 chunk).

## 1. Recorded decisions (settled at planning, per dispatcher instruction)

### D1 — `RowSubject` shape and home: enums-with-data in `presentation.rs`

```rust
/// The UI-layer row subject (O-2): which entity a card/menu/verb
/// addresses. Never a domain type — habits stay a sibling entity
/// (research §3.2); this is the presentation grammar's union only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowSubject {
    Task(TaskId),
    Habit(HabitId),
}
```

- Home: `crates/taskboard-ui-slint/src/presentation.rs`, next to
  `task_menu_subject` (the derivation that consumes it). A1 rung:
  plain enum-of-ids, both variants are already-minted typed ids —
  no new minting needed, no derive crates (policy §2.1).
- **Router insertion point**: the adapter's subject-carrying seams,
  in this order:
  1. `TaskMenuSubject` (menu.rs:89) gains its real `is_habit`
     derivation: `task_menu_subject`'s signature widens from
     `Option<TaskId>` to `Option<RowSubject>`; the
     `is_habit: false` hardcode (presentation.rs:1131) is **deleted**
     — `RowSubject::Habit(_) => true`, `RowSubject::Task(id)` derives
     `false` exactly as today (the recorded deviation note goes with
     it).
  2. The adapter's menu openers: `open_task_context_menu`'s
     `subject: Option<TaskId>` widens to `Option<RowSubject>`; a new
     `open_habit_context_menu` (D4) is its habit-arm sibling.
  3. `DispatchCtx` already carries the focus facts separately
     (`focused_task`, the adapter's `focused_habit` field feeding the
     one-shot `habit_*` ctx cells); H2 does **not** re-plumb dispatch
     onto `RowSubject` — the one-shot ctx-field routing
     (`adapter.rs:468-563`, router.rs habit arms ~870-930) is the
     established path for the habit verbs and H3 (keyboard) is the
     chunk that may unify focus addressing. Keeping the union at the
     *menu-subject* seam only is the minimal change that removes the
     hardcode and opens the habit menu.
- Explicitly **not** created: a domain `TaskKind::Habit`, any
  `AppState` change, any `TaskRef`/`HabitId` domain change.

### D2 — Which task-card slots habits fill vs hide

The habit card mounts the real `TaskCard` widget (O-2's "same
substrate"), driven by a **presentation-side adapter struct**, not by
stuffing habit data into `TaskCardModel`'s task-shaped fields:

| `TaskCardModel` slot | Habit rendering |
|---|---|
| `title` | habit title (fill) |
| done slot (`can-done`/`done`) | **the +1 log control** (fill): `can-done: true`, `done: false` always (surplus logs are first-class, D-P14 — never a done bit); `done-toggled` routes `HabitLog` from the row source. This is US-1/S2's "click complete → LogHabit" |
| `due-phrase`/`due-kind` | **hidden** (empty phrase; habits have no due) |
| `scheduled-phrase` | **the time chip** (HAB-11): `anchor_time` renders as a first-class mono chip (`HH:MM`), replacing the meta-string `"@" + chip`; when `anchor_day` is `Some` (chunk 4's field), a day chip (`Wed` / day-of-month) joins it — display only, the O-1 option-b law |
| `effort-phrase` | **hidden** (empty) |
| `track-state` | **hidden** (tracking is task-only; no tracking verb routes for habits) |
| `label-chips`/`decorations` | **progress + streak** (fill): the `3/8` progress phrase and the streak render as decoration chips in the card's decoration row (typed `TaskDecoration` entries, not text-in-title) |
| selection/anchor/drag flags | `false` (selection & drag are H3's) |

- If the existing `TaskDecoration` vocabulary cannot carry progress/
  streak without semantics theft, **extend the slint enum with habit
  variants** (`HabitProgress`, `HabitStreak`, `HabitTime`, `HabitDay`)
  — additive slint struct/enum change, adapter-side derivation only.
  Do not reuse `DueKind` variants for habit chips.
- `TaskCard` gains **no** habit-specific branches: it stays dumb; the
  model derivation lives in `models.rs::push_habits` (renamed row
  build) + `presentation::habit_rows` (typed facts already exist).

### D3 — Horizontal action set (HAB-8)

The vertical IconButton stack (habits.slint:238-298) is replaced by a
**horizontal strip under the card** (matches a task row's entry height,
AC US-1/AC-1):

- Shown: `+1` (log; the same verb as the done slot — the strip keeps
  the explicit affordance for discoverability), `−1` (undo log), the
  conditional make-up `repair` (existing typed mount flag), — in that
  order, each `IconButton`, editing-gated
  (`presentation::habit_editing_enabled`), kiosk renders **disabled,
  never unmounted** (HAB-14, the D-P13/`editing-enabled` grammar).
- Moved to the context menu (deleted from rows): move-up/move-down
  (reorder → `HabitMove` menu rows, floor-midpoint law unchanged),
  convert (D4's submenu).
- No new affordances beyond the parity set: no inline quota edit
  (#961), no streak badge upgrade (#962) — non-goals.

### D4 — The habit context menu: `habit_context_page`, not a new `MenuKind`

- **`MenuKind` stays `{Main, Object}`.** The habit menu is an anchored
  object menu in every behavioral dimension (chain
  `[Board, Global]`, pointer anchor, dismissal, settle matrix). A
  `MenuKind::HabitRow` variant would add exhaustive-match arms without
  driving any rendering difference — the recorded deviation from the
  UX plan's wiring-table `MenuKind::HabitRow` spelling; the census
  entry cites this decision.
- New projection in `menu.rs` (projection-only, mirroring
  `task_context_page`): `habit_context_page(keymap, chain, subject:
  TaskMenuSubject, states, armed: bool) -> Vec<MenuEntry>` over a new
  `HABIT_CONTEXT: &[ActionId]` curated list:
  - Core rows: `HabitLog` (+1), `HabitUndoLog` (−1), `HabitRepair`
    (greys when no make-up window applies — the states map, never
    withheld: the verb is applicable, precondition-blocked), `HabitEdit`
    **reserved but absent in H2** (the verb is H4's; do not mint a
    dead row), delete row **absent in H2** (GUI remove is H4's staged
    confirm; #944 closes there).
  - Reorder: `HabitMove` as two rows (`Move up` / `Move down`) — the
    one-shot `habit_move` ctx cell gains a direction/slot the seam
    mints (existing `reorder-asked(row, slot)` arithmetic reused
    presentation-side; pointer reorder parity is preserved until H3
    replaces it with drag/keys).
  - **Convert ▸ submenu** (R-3, O-7 Q3): one parent row, children =
    the existing habit Convert family (`HabitConvertArm`,
    `HabitConvertToSeries`) — `is_habit: true` now actually renders
    it via the *existing* `TaskMenuGroup::Convert` arm (menu.rs:307-315),
    so the submenu machinery is reused, not duplicated.
  - **Armed-confirm inside the menu (O-7 Q3)**: `HabitConvertArm`
    arms (consequence text stays in the registry description — the
    consequence-first law reads from the tooltip/menu row, the strip's
    caption text moves nowhere); after the arm routes, the adapter
    **re-projects the open menu in place** (same anchor, same kind):
    `HabitConvertToSeries` flips from greyed to enabled and a
    `ConvertCancel` step disarms. One subject armed at a time (the
    existing `convert-armed-row: -1` global becomes the armed
    `RowSubject`); opening another subject's menu disarms (menus are
    built once at open — the D-1 pin — so the re-projection after the
    arm is the one sanctioned rebuild).
  - Destructive-last law (BF-W-N2): no delete row in H2, so the page
    ends with the Convert parent.
- The task card's menu is untouched: a `RowSubject::Task` subject
  through `task_menu_subject` derives exactly today's facts (pinned
  by existing tests + one new delta test).

### D5 — The old `HabitsSection` widgets: deleted; the create form stays

- **Deleted**: the per-row `ListRow` + vertical button stack + the
  armed confirm strip + their six row callbacks (`plus-clicked`,
  `undo-log-clicked`, `reorder-asked`, `repair-clicked`,
  `convert-armed/confirmed/cancelled` as row-index callbacks — the
  menu/card seams replace them).
- **Kept verbatim**: the section header (title + count + `Add habit…`)
  and the inline create form (`create-asked`, `create-confirmed`,
  `create-cancelled`, `create-quota-edited`, `quota-validity`,
  `create-open` mount gate) — round-7 D-7.2's flow, untouched. The
  section's model property becomes `[HabitCardModel]`-shaped (D2) and
  the row callbacks shrink to `row-activated(index)` (mints
  `focused_habit`, #832) + `context-requested(index, x, y)` +
  `log-clicked(index)` + `undo-log-clicked(index)` +
  `repair-clicked(index)` on the card strip.
- `today-view.slint`'s property surface (`habits-*`) updates with the
  same shape; no board mount (R-1/O-7 Q2 — habits live outside boards).

### D6 — Kiosk posture (HAB-14)

Rows render; every action (done-slot log, strip, menu verbs, create)
gates on `editing-enabled` — disabled tone, never unmounted, the
existing T-7 visible-gating grammar. The context menu itself opens on
kiosk (reading a menu is passive) but every habit verb row greys from
the states map; the convert arm additionally refuses at dispatch (the
existing kiosk refusal). Pinned by a states-map test + the kiosk gate
test.

### D7 — Test discipline for this chunk

- TDD order: (1) `menu.rs` projection tests (pure, no window) →
  (2) `presentation.rs` subject-derivation tests → (3) adapter-level
  `on_slint_thread` journey tests → (4) models-pass fold tests.
- **No text assertions**: menu row sets asserted as
  `Vec<Option<ActionId>>` presence/order; chips as typed decoration
  presence flags; armed-confirm as enablement flips.
- **No sleep-based waits**: poll predicates with deadlines or assert
  terminal `AppState` via the existing journey rig conventions.
- Determinism: fixture clocks anchored for relative-oracle assertions
  (`docs/testing_strategy.org` §6.1); streak/progress fixtures anchor
  the day, never read the wall clock.

## 2. TDD work breakdown (ordered; each item = failing test first)

### Phase A — the subject union + menu projection (pure Rust, no slint)

- **A1** (`menu.rs` tests): a new `habit_context_page` over an
  `is_habit: true` subject — the curated rows in declared order, no
  `MenuKind` change, the Convert parent renders, the task page
  (`task_context_page`) over `is_habit: false` is byte-identical to
  today (delta pin: existing tests unchanged).
- **A2** (`presentation.rs` tests): `task_menu_subject(state,
  Some(RowSubject::Habit(h)))` derives `is_habit: true` and all task
  facts `false`; `Some(RowSubject::Task(t))` derives exactly today's
  task facts (the hardcode's deletion is behavior-covered, not just
  type-covered).
- **A3** (`menu.rs` tests): the armed-confirm grammar —
  `states[HabitConvertToSeries] = false` greys the confirm row;
  the arm flip is the adapter's states map + re-projection (A5), so
  A3 pins only the greying and the child page's id/hint contract.
- **A4** (`presentation.rs`/`models.rs` fold tests): the habit
  card-model derivation — done-slot flag, decoration chips (progress,
  streak, time, day) present iff their facts are, due/scheduled/track
  slots empty/hidden. Property test (`proptest`): for any
  `(count, quota, streak, anchor_time, anchor_day)` the model carries
  exactly the decoration set the facts imply (pure domain→presentation).

### Phase B — adapter wiring (seams + routing)

- **B1** (`adapter.rs`, `on_slint_thread` journey test): right-click a
  habit card opens the habit menu (structural: `MenuKind::Object`,
  entries = `habit_context_page`'s ids, `focused_habit` minted) —
  journey `us2_s1`.
- **B2**: `HabitLog` routed from the menu/card resolves the row-source
  habit (existing one-shot path; test asserts the `LogHabit` command
  reaches the engine and the window's count grows) — journey `us1_s2`.
- **B3**: the convert submenu flow — arm routes `HabitConvertArm`
  (strip flags set), the re-projected menu's confirm row enables,
  confirm routes `ConvertHabitToSeries` (a series task exists, habit
  tombstoned), cancel disarms — journey `us2_s2` (the O-7 Q3
  armed-confirm evidence).
- **B4**: reorder menu rows mint `habit_move` with the floor midpoint
  of the correct free interval (existing `habit_drop_order`
  arithmetic; test asserts the `MoveHabit` command's `order` field).
- **B5**: kiosk gate — with `editing-enabled: false`, every habit verb
  row greys (states map) and the log/strip controls render disabled;
  the menu still opens (structural presence, D6).
- **B6**: task-card regression pin — a task card's right-click still
  yields today's exact `task_context_page` composition through the
  widened `RowSubject` seam (the D1 insertion point's blast radius).

### Phase C — the slint surface

- **C1** (`habits.slint` rebuild + fold test): the section mounts
  `TaskCard`s with the horizontal strip; row height parity asserted
  structurally (the card's min-height against a task row's — same
  ladder step, no fixed px) — journey `us1_s1` evidence.
- **C2**: the create form unchanged — existing create seam tests stay
  green unmodified (the keep-verbatim contract of D5).
- **C3**: stale-index absorption — row callbacks against a vanished
  habit degrade to no-ops (the notification-row guard shape, existing
  convention).

## 3. Census & documentation

- `docs/ux-surface-census.org`: dated supersession entries with
  **symbol anchors** (the #775 convention — `[[menu.rs::habit_context_page]]`-
  style refs, not line numbers): habit-row rows (card-ified, HAB-7/8/11
  closed), the R-3 convert-demotion entry, the `is_habit`-hardcode
  removal (the 2026-11-13 recorded deviation retires), the
  `MenuKind::HabitRow`-naming deviation (D4), HAB-14 kiosk posture
  restated for the card strip.
- `docs/widget_interactions.org` (W1's skeleton, if present on main):
  the habit card mount recorded as a `TaskCard` consumer with
  customization points (D2's slot table).

## 4. Quality gates

Stage-2 gates on pre-commit hooks + flock-wrapped `cargo nextest`
(`scripts/run-quality-gates.sh`) — never the coverage trio inside the
subagent (AGENTS §7 dispatch discipline); the dispatcher owns the full
§10 suites. `scripts/ux-coverage.sh <J1 plan> --gate` at stage-2
self-check and stage 3 (this chunk implements US-1/US-2's wiring rows
W-1/W-2/W-3 of the J1 plan; `us3_*`/`us5_*`/`us6_*` stay for
H3/H4).

## 5. Risks / watch items

- **adapter.rs blast radius**: widening `task_menu_subject` and
  `open_task_context_menu` signatures touches every call site (board,
  stack, today, review card surfaces). Mitigation: B6's regression pin
  lands before any call-site edit; the widening is mechanical
  (`Option<RowSubject>` + one match).
- **TaskDecoration extension** (D2) may collide with W3–W7's
  centralization wave if it lands concurrently — H-chunks are
  file-disjoint from the W/D chain per the round plan §4, so keep this
  branch rebased and the slint enum change additive-only.
- **Menu re-projection after arm** (D4) is new behavior for the
  object-menu pipeline (menus are built-once). If the in-place rebuild
  proves invasive, the fallback grammar is: arm closes the menu and
  the *row* shows the existing armed strip (the current behavior) —
  but that contradicts O-7 Q3's "inside the context menu", so it needs
  an operator ping before adoption; record in the stage report if
  hit.
- **Reorder menu rows** (D3/D4) are a stopgap until H3's drag/keyboard
  reorder; if H3 lands first on main, drop the menu reorder rows and
  rebase.
- The habit menu opening on kiosk (D6) is a new surface for the
  passive device — verify against the D-P13 posture with the operator
  only if the greying verdict reads ambiguously; default to greying,
  not withholding.

## 6. Diff estimate (rough, for the dispatch plan)

- `menu.rs`: +~120 (projection + tests) / −~5
- `presentation.rs`: +~60 / −~15 (subject union, derivation, chip facts)
- `adapter.rs`: +~200 / −~80 (menu openers, seam rewiring, re-projection)
- `models.rs`: +~60 / −~30 (habit card-model push)
- `habits.slint`: rewrite of the row loop (+~120 / −~170; create form kept)
- `today-view.slint`: +~15 / −~25 (property surface)
- `task-card.slint`: +~10 (decoration enum variants, if needed)
- journey/fold tests: +~400
- **Total**: ~+900 / −~325 across ~10 files — **L** per the round plan's
  size column.
