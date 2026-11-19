# Round 9 Habit UX Parity — Research Report

- **Date**: 2026-11-18
- **Kind**: round-9 GUI-review research (operator findings → inventory,
  gaps, architecture recommendation, proposals, chunk boundaries).
- **Status**: input to the round-9 UX journey plan(s) and dispatch plan;
  no code written, no rulings made — every open decision is listed in §5.
- **Operator mandate**: habits must reach UX parity with tasks in the
  Today view — same interaction and navigation grammar — plus a set of
  concrete affordance changes; the "habit as a special kind of task"
  architectural hypothesis must be evaluated layer by layer.

## 1. Current implementation inventory (habit vs task)

Line references are to `main` at research time; the census's older
`adapter.rs:7094-7562` habit-window refs have drifted (adapter.rs is now
14 368 lines; the habit seams live at ~9178-9720 and the one-shot ctx
fields at ~468-563).

### 1.1 Domain (`taskboard-domain`)

| Aspect | Habit | Task |
|---|---|---|
| Entity | `Habit` (`src/habit.rs:186-225`): id, board, title, `HabitTarget` (PerDay/PerWeek/PerMonth × `NonZeroCount`), `order: i64`, `anchor_time: Option<NaiveTime>`, `HabitClocks`, `continued_from`, tombstone, `by`. **No description field.** | `Task` (`src/entities.rs`): title, description, duedate, scheduled, done, stack+order, labels, effort/importance/urgency, recur, `TaskKind::{Standard,Tracker}`, archived, full `TaskClocks` |
| State home | `HabitState { habits, log }` nested in `AppState.habits` (`src/state.rs:44`) | `AppState.tasks` (+ stacks, boards) |
| Semantics | "Nothing ticks": no spawn/roll/done; a window's state IS its log row keyed `(habit, window)` (`habit.rs:5-10`); streak/`log_window`/grace pure fns (`habit.rs:436-611`) | done/scheduled machinery, recurrence spawn (`recur.rs`), reminders rules |
| Commands | `CreateHabit`, `UpdateHabit { changes: HabitChanges }`, `MoveHabit`, `LogHabit`, `AdjustHabitLog`, `DeleteHabit`, `ConvertHabitToSeries` / `ConvertSeriesToHabit` (`messages.rs:445-530`) | full `TaskChanges` family incl. `SetTaskDone`, moves, reminder rules |
| Sparse edit | `HabitChanges { title, target, anchor_time (double-Option), continued_from }` (`messages.rs:752-784`) — **no description, no per-period day** | `TaskChanges` (all fields) |
| Undo | `CreateHabit`/`MoveHabit` = PreImages; `LogHabit`/creating `AdjustHabitLog` = `NotUndoable(HabitSurface)`; `UpdateHabit` partially (`undo.rs:249-330`) | per-command invertible facts, per-board undo stack |
| Reminders | Tracker nudge only: `anchor_time` arms one nudge per day-window via `tracker_anchor_occurrence` (`habit.rs:1566-1579`; `reminders.rs:1387-1446`), quota-gated, prunable/dismissible/snoozable as `ReminderRef::Tracker` (`reminders.rs:919-927`) | full `ReminderRule` machinery (rules, snooze, insistence, mute) |
| Kind marker | none (separate entity) | `TaskKind::Tracker` precedent (`entities.rs:372-380`): "data plus one engine gate; every other verb unchanged" |
| Sync | habits roam as their own family: `habits_for`/`habit_log_for` builders, board-level flush, lane GC (`sync-nextcloud/src/flush.rs:28-109`); habit log wire retention in `actor.rs:60` | Deck card mapping / files lane per task |

### 1.2 State engine (`taskboard-state`)

- Habit commands plan through the same `plan_command`
  (`domain/src/command.rs:36-41,499-518`); typed errors
  `UnknownHabit`/`HabitDeleted`/`InvalidHabitWindow`
  (`command.rs:167-177`); outcomes `HabitCreated`/`HabitLogged`/
  `HabitLogAdjusted` (`command.rs:331-350`).
- `core.rs:146-230` exposes `habits` to projections (tracker-nudge pass
  reads it). No habit-specific actor machinery — parity at this layer is
  already structural.

### 1.3 UI (`taskboard-ui-slint`)

| Aspect | Habit (Today section) | Task (Today cards / board) |
|---|---|---|
| Composition | `HabitsSection` (`ui/compositions/habits.slint`, 314 lines), mounted only in Today (`today-view.slint:224-243`; census ruling R-1) | `TaskCard` widget reused across Today/board/review (`ui/widgets/task-card.slint`) |
| Row model | `HabitRowModel { title, progress, streak, chip, met, repair, repair-undoable }` (`habits.slint:25-40`) — plain `ListRow` text + emoji streak, **not a card**; no selection/focus/hover card grammar | `TaskCardModel` with done box, track control, due/scheduled/effort chips, labels, quick-actions bar (round-8 #933), hover, activation |
| Row actions | **Vertically stacked** `VerticalLayout` of IconButtons per row: move-up, move-down, +1, −1, conditional repair, convert (`habits.slint:223-283`) | horizontal quick bar on the card popup (round-8 chunk 5, #933) |
| Right-click | none — no `context-requested` on habit rows; habit verbs appear only in the **GLOBAL** menu's Convert group (`menu.rs:311-313, 483-490`) with `TaskMenuSubject.is_habit` permanently `false` for card menus (`presentation.rs:1107-1131`) | `context-requested(x,y)` → `MenuKind` pinned card menu (`TASK_CONTEXT` curated page, `menu.rs:444+`, N-11 fix) |
| Keyboard | only `HabitCreate` bound (`Ctrl+Shift+H`, Today context; `keymap/presets.rs:334,671,1064`). All other habit verbs pointer-only, keyed by row index through `IdTables.habits`; no row focus in the keyboard walk — issue **#934** | full keymap parity family (round-8 #862: task-action shortcuts, emacs motions, M-</M->), focus restoration (#872) |
| Reorder | chevron-button pair → `reorder-asked(row, slot)` → adapter computes midpoint (`adapter.rs:9615-9641`, `presentation::habit_drop_order`) → `MoveHabit`. **No drag-n-drop** (the census's "drag reorder adapter.rs:7467-7496" line ref is stale — that region is settings code; the seam is button-driven) | board drag gap-drop + compact movement + multi-select (#918) |
| Edit | none in GUI (census ruling R-2, 2026-11-13: "CLI `habits update` covers; not requested") — now overturned by the operator | task detail editor (round-8 #922: date-only day-boundary law, editor repairs) |
| Create | inline form under section header (round-7 D-7.2): title + quota + PerDay/PerWeek/PerMonth segmented picker (`habits.slint:139-210`) | capture overlay / card create |
| Convert to series | **always-visible** per-row button + armed confirm strip (`habits.slint:276-311`) | series→habit conversion lives in card context menu + detail recur row (RC-1′) |
| Time display | anchor chip in the meta string: `"@" + row.chip` (`habits.slint:219`) | mono due/scheduled chips |
| ActionId tokens | `HabitLog/HabitUndoLog/HabitMove/HabitCreate/HabitCreateCommit/HabitRepair/HabitConvertArm/HabitConvertToSeries` (+ `SeriesConvertArm/SeriesConvertToHabit`) (`actions.rs:384-436`, registry labels `actions.rs:1724-1802,2141`) | full task family |
| Routing | one-shot ctx fields `habit_target/habit_undo/habit_move/habit_repair/habit_convert_target` + `habit_create_draft` (`adapter.rs:468-563`, consumed at 1364-1369) → `router.rs:870-930` maps to `StateCommand`s | `TaskRef`-addressed routing, per-board undo, focus facts |
| CLI | `habits add/list/update/rm/adjust` + `--order` (`app/src/cli.rs:1236-1362`); no convert verb (census HB-4 U) | full `tasks` family |

### 1.4 Related issues folded in

- **#934** (open, p3/ui): habit section is pointer-only; chunk 8 added
  only the *focus fact* so menu-routed `HabitConvertArm` can resolve its
  subject — no Tab/arrow traversal, no key-invocable +1/undo/convert.
  Note: no `focused_habit` symbol grep-hits in `src/` today; the landed
  seam is the one-shot `habit_convert_target` ctx field — verify the
  chunk-8 focus-fact name during planning.
- **#935** (open, p3/dx/ui): `route()`'s `Disabled|NoTarget => {}` arm
  silently absorbs refusals surface-wide; habit verbs (subject-less,
  stale-index, un-armed) are prime NoTarget producers — the uniform
  typed-notice pass belongs in the same round or is explicitly deferred.
- **#938** (open, ui): card context menu could carry the Labels row —
  orthogonal to habits but the same "what belongs in a context menu"
  grammar decision; fold into the menu-grammar chunk only if cheap.

## 2. Gap list (operator findings → IDs)

| ID | Finding (operator intent) | Current state | Layer(s) touched |
|---|---|---|---|
| HAB-1 | Editable habits (GUI) | R-2 ruling kept editing CLI-only; `UpdateHabit` + `HabitChanges` exist and plan fine | UI only |
| HAB-2 | Optional description | **No field anywhere in the domain** (`Habit`, `HabitChanges`, CLI, sync wire) | domain + state + sync wire + storage + UI + CLI |
| HAB-3 | Optional time | `anchor_time` exists (domain, CLI, nudge engine); GUI edit and list display upgrades missing | UI (domain done) |
| HAB-4 | Optional day for per-week/per-month habits | **Not representable**: Week windows are ISO-Mondays, Month windows the 1st (`habit.rs:328-339`); no anchor-day concept | domain (window vocabulary!) + state + sync + UI |
| HAB-5 | Right-click context menu like tasks | rows have no `context-requested`; habit verbs only in GLOBAL menu | UI |
| HAB-6 | Keyboard shortcuts on habit actions | only `HabitCreate` bound; #934 open (traversal AND verb keys) | UI |
| HAB-7 | Convert-to-recurring-task demoted to right-click submenu entry | always-visible row button + armed strip | UI |
| HAB-8 | Action buttons horizontal, reasonable entry size | vertical stack of 4-6 IconButtons per row (`habits.slint:223-283`) | UI |
| HAB-9 | Drag-n-drop reordering | button-only; task board has gap-drop machinery to borrow (#918 shapes) | UI |
| HAB-10 | Keyboard reordering | none; task board has compact movement (#918) | UI |
| HAB-11 | Habit time displayed in list | anchor chip exists in meta string (`@HH:MM`); not a first-class chip like task due chips | UI (polish) |
| HAB-12 | Habit with a time gets an automatic reminder like tasks | **largely exists**: tracker nudge from `anchor_time` (`reminders.rs:1387-1446`), quota-gated, once per day-window, snooze/dismiss/prune via `ReminderRef::Tracker`. Gaps: no GUI way to set the time (HAB-1/3); nudge is day-window-shaped even for weekly/monthly quotas; no per-habit override of insistence/snooze grammar vs task rules | UI + possibly reminders |

Cross-cutting: HAB-13 (fold #935's typed NoTarget notices into the new
habit verb surfaces so the parity work doesn't multiply silent
refusals), HAB-14 (kiosk posture: rows render, actions gate — the
D-P13/`editing-enabled` grammar must survive the row rework).

## 3. Architecture: "habit as task variant" — evaluation

### 3.1 What is habit-specific vs shared

- **Habit-specific**: quota-per-period (`HabitTarget`), the
  `(habit, window)` count log, streak/grace/attribution pure functions,
  `continued_from` conversion seam, board-scoped (not stack-scoped)
  ordering, the "nothing ticks" law, tracker nudge, habit sync family +
  wire retention.
- **Shared in effect, not in kind**: ordering (i64 midpoints — same
  arithmetic as task positions), tombstone/LWW posture, undo journal
  plumbing, reminders delivery machinery, kiosk gating, CLI grammar.

### 3.2 Domain/state modeling change (rejected as recommendation, documented)

Making `Habit` a `TaskKind::Habit` variant of `Task` is precedented in
shape (`TaskKind::Tracker`, `entities.rs:372-380`) but fails on the
data: a habit has no stack, no done, no duedate, and *a second
collection* (the window log) that tasks do not have; its clocks
(`HabitClocks`), sync wire family, storage shape, and undo semantics
all differ. Collapsing the entities would (a) break the roaming sync
wire (flush.rs habit builders, `habit-log` wire retention), (b) force
`Option<HabitPayload>` fields onto every task — validate-and-forget
risk the type-driven policy §2 forbids, (c) re-litigate the accepted
"nothing ticks / never-done is native" design law (`habit.rs:5-10`) and
the D-H12 window-canonicality rulings, and (d) buy nothing the UI
cannot get from a presentation-level union. **Recommendation: do not
change the domain modeling; habits stay a sibling entity.**

### 3.3 Recommended: UI-level unification (presentation grammar), with one small domain additive wave

1. **UI subject union (the core move)**: introduce a UI-layer row
   subject enum — `RowSubject::Task(TaskId) | RowSubject::Habit(HabitId)`
   (adapter/router level, `enums-with-data` — exactly the rung the
   type-driven policy says still suffices). The context-menu grammar
   (`MenuKind`, `TaskMenuSubject` gains a real `is_habit` derivation),
   the keymap (`ContextId::Today` entries for the habit verb family),
   focus/selection walk, and the quick-bar composition address both
   variants uniformly. Habits then inherit *by construction*: right-
   click menus, keyboard verbs, focus traversal (#934), drag-drop and
   keyboard reorder (reusing the board's #918 midpoint machinery —
   `MoveHabit` already shares the arithmetic), detail editing, and the
   every-click-reacts notice law.
2. **Row rework**: replace the vertical button stack with the task-card
   quick-bar pattern (horizontal, editing-gated, kiosk renders-disabled)
   — HAB-7/8/11; convert becomes a context-menu submenu entry with the
   existing armed-confirm step preserved inside the menu (HAB-7→HAB-5
   interaction).
3. **Domain additive wave (small, boundary-respecting)**: `Habit.description`
   (+ `HabitChanges.description`, CLI, sync wire additive with default —
   HAB-2) and the per-week/per-month anchor day (HAB-4 — the one genuine
   domain design decision, see §5 Q1). GUI edit routes the existing
   `UpdateHabit` (HAB-1/3). Reminder parity (HAB-12) mostly needs the
   GUI seam plus possibly one reminders.rs refinement for
   weekly/monthly nudge shaping.

### 3.4 Layer-by-layer impact (recommended shape)

| Layer | Impact | Risk |
|---|---|---|
| taskboard-domain | `Habit.description`; anchor-day representation (Q1); `HabitChanges` extension; possibly `tracker_anchor_occurrence` generalization. Zero changes to Task/messages enums beyond habit arms | HAB-4's anchor day touches the window vocabulary — the D-H12 inert-row law and streak/proptest harnesses must be re-derived for non-Monday/non-1st windows; sync wire is additive but the LWW clock list grows a `description` slot |
| taskboard-state | planner arms for the new changeset fields; snapshot tests extend. No actor surgery | low; follow the A1 mint + clock-slot discipline |
| taskboard-storage-sqlite / sync | additive column / wire fields with defaults; habit-log retention untouched | migration + wire-compat (older devices omit the fields — the `by`-slot precedent for lenient decode) |
| taskboard-ui-slint | the bulk: `RowSubject` union, habit row card-ification, `MenuKind::Habit` + `HABIT_CONTEXT` list, keymap preset entries (+ coverage-gate ActionId tokens), drag/keyboard reorder, detail/editor reuse, time chip | largest surface; must keep the on_slint_thread test rule, single greppable ActionId tokens, and the census R-1 ruling (Today-only mount) unless the operator overturns it (Q2) |
| taskboard-app | CLI `habits update` gains `--description`/day flags; notification surface already handles `Tracker` refs | low |

### 3.5 Risks / watch items

- The census's habit-family cells (habit.create/log/reorder/repair/
  convert rows + R-1/R-2/R-3 rulings) must be re-homed with dated
  supersessions when the UX plan lands — R-2/R-3 (no GUI edit/remove)
  are **overturned by the operator**, which per the handoff §5 requires
  exactly this: a fresh operator pass at UX-plan level (this round).
- #934 and #935 interact: new habit verb keys will produce NoTarget on
  stale subjects; decide whether the uniform notice pass rides this
  round (recommended) or stays separate.
- Journey/snapshot tests: StateEngine snapshots for the new changeset
  fields; `insta` updates; proptest harnesses for anchor-day windows.

## 4. Additional-features proposals (beyond the operator list — proposals only, each needs a UX-plan home or backlog issue)

1. **Skip day / planned skip** — an explicit typed skip (distinct from a
   miss) that does not break the streak; today only the grace window and
   manual `AdjustHabitLog` exist. Domain: a skip row kind or a
   streak-walk rule change.
2. **Undo completion via the undo stack** — `LogHabit` is
   `NotUndoable(HabitSurface)`; the −1 valve covers single clicks, but a
   global-undo integration (or at least multi-count undo) is a natural
   parity ask.
3. **History / calendar view** — the log data exists
   (`HabitState.log`); no surface visualizes past windows (heatmap,
   month grid).
4. **Archive / pause habit** — tombstone exists (`DeleteHabit`) but
   there is no pause (series have `paused_until` — the habit twin).
5. **Per-habit reminder overrides** — insistence/snooze grammar per
   habit (task ReminderRule parity for tracker nudges).
6. **Notes per completion** — attribution is window-count-only by
   design (D-H5); a note would need a schema decision (likely rejected
   by the existing law — record as such if declined).
7. **Per-weekday schedule** (e.g. Mon/Wed/Fri daily habit) — generalizes
   HAB-4's anchor day; the recur-pattern vocabulary is the precedent.
8. **Quota edit inline on the row** (click the "3/8" phrase).
9. **Streak display upgrade** — the 🔥N emoji → the badge widget
   (`widgets/badge.slint`) with tooltip semantics.
10. **Statistics** — adherence %, best streak, per-period totals.

## 5. Open questions needing an operator / UX-plan decision

- **Q1 (HAB-4, the big one)**: what does "optional day" mean for
  per-week/per-month habits — (a) an anchor day that *shifts the window
  boundary* (my week runs Sun–Sat; my month runs 1st or anniversary),
  or (b) only a *display/nudge day* inside the canonical window? Option
  (a) re-opens the D-H12 window-canonicality law, the streak walk, the
  sync wire keys, and the proptest harnesses; option (b) is additive
  and cheap. Recommend asking before any dispatch plan is chunked.
- **Q2**: do habit rows stay Today-section-only (census R-1), or does
  parity mean habits also appear on the board as card-like rows?
  (Operator said "Today view" — recommended scope is Today-only; board
  mount would be a separate journey.)
- **Q3**: does "convert to recurring task" keep its armed-confirm step
  inside the context menu (recommended — consequence-first law), and
  does the GLOBAL-menu Convert group keep the habit rows once the
  context menu exists?
- **Q4**: habit detail editing — reuse the task detail editor surface
  (`task-view.slint`) with habit fields, or an inline/popover editor on
  the row? (Task detail is the parity answer; kiosk posture follows.)
- **Q5**: should #935's uniform NoTarget notice pass ride this round
  (recommended — the new habit keys multiply its exposure) or stay a
  standalone chunk?
- **Q6**: is GUI habit removal (R-3 overturn) in scope too? Editable
  implies manage; the operator list does not name delete.
- **Q7**: weekly/monthly nudge shaping (HAB-12): is the day-window
  nudge acceptable for non-daily quotas, or should the nudge fire on
  the period's last day (a reminders.rs refinement)?

## 6. Suggested dispatch-plan chunk boundaries

Each chunk = one branch/PR, one UX journey plan above all of them
(mandatory per AGENTS §7 / `docs/ux_planning.org` — this round's
operator pass is that authority's fresh input):

1. **Chunk 1 — domain additive wave**: `Habit.description` +
   `HabitChanges` extension + clock slot + storage/sync wire + CLI flags
   + planner arms + snapshot/proptest extensions. (HAB-2; plus Q1's
   anchor-day ruling if option (a) is chosen — else defer to its own
   chunk given D-H12 blast radius.)
2. **Chunk 2 — habit row rework + context menu**: `RowSubject` union,
   card-ified row with horizontal quick bar (HAB-7/8/11),
   `MenuKind::Habit` right-click menu with convert submenu (HAB-5/7),
   kiosk gating preserved (HAB-14), #935 notices for the new verbs if
   Q5 says ride.
3. **Chunk 3 — keyboard parity**: habit-row focus/traversal + verb keys
   + keyboard reorder (HAB-6/10; closes #934), keymap preset entries +
   coverage-gate ActionId tokens, on_slint_thread journey tests.
4. **Chunk 4 — editing + time**: GUI edit flow routing `UpdateHabit`
   (title/description/quota/anchor time/day; HAB-1/3/4), detail-editor
   reuse per Q4, reminder-parity seam (HAB-12, Q7).
5. **Chunk 5 — drag-n-drop reorder**: gap-drop borrow from the board
   machinery onto habit rows (HAB-9); presentation midpoint already
   shared.
6. **Chunk 6 (optional)** — proposals from §4 the operator adopts
   (skip-day and pause are the likeliest P1s); census re-homing +
   supersession entries closes each landed chunk.

Serialization note (handoff §4): one agent at a time until the
memory-guard coverage is complete; chunks 2/3 and 4/5 are file-adjacent
in adapter.rs — keep them serialized or strictly file-disjoint.

## 7. Files this report read

`crates/taskboard-domain/src/{habit.rs,messages.rs,entities.rs,state.rs,
command.rs,undo.rs,reminders.rs}`, `crates/taskboard-state/src/{core.rs,
actor.rs}`, `crates/taskboard-ui-slint/src/{actions.rs,adapter.rs,router.rs,
menu.rs,presentation.rs,keymap/presets.rs}`, `crates/taskboard-ui-slint/ui/
{compositions/habits.slint,views/today-view.slint,widgets/task-card.slint}`,
`crates/taskboard-sync-nextcloud/src/flush.rs`, `docs/ux_planning.org`,
`docs/ux-surface-census.org` (habit rows), issues #934/#935/#938,
`.artifacts/reports/2026-11-18-round8-handoff-to-round9-analysis.md`.
