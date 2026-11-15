# [Plan] UX Remediation — Chunk 4: board verbs + entry-point asymmetry

- **Date**: 2026-11-15
- **Kind**: stage-1 chunk plan for chunk 4 of the UX remediation phase
  (framework plan §6 phase 5; the dress rehearsal's fourth chunk).
  Planning only — no application code in this stage.
- **Origin**: the remediation dispatch plan
  `.artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md` §1/§5
  (the shared execution model, the long-command discipline, and the
  dress-rehearsal mandate are binding); the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` §3 rows
  T-8, T-9, T-10, T-11, T-12, T-13, T-20, T-21; home issues #791, #792,
  #793, #794, #795, #796, #803, #804; the owning UX plans
  `2026-11-14-board-stack-management-ux-plan.md` (T-8..T-12),
  `2026-11-14-task-lifecycle-ux-plan.md` (T-13),
  `2026-11-14-recurrence-family-ux-plan.md` (T-20),
  `2026-11-14-review-walk-reflection-ux-plan.md` (T-21); the census
  `docs/ux-surface-census.org`; `docs/ux_planning.org` §2; the landed
  chunks' patterns (chunk 1's visible-gating grammar and archived
  roster, chunk 2's armed-confirm Flow, chunk 3's picker / overlay /
  ActionId patterns and swatch palette).
- **Status**: READY — preconditions verified by the dispatcher (chunks
  1–3 merged; the triage report accepted 2026-11-15; `main` carries the
  dispatch plan with the long-command discipline amendment); worktree
  `../taskboard-WT-fix-ux-r4-board-entry-points` on branch
  `fix/ux-r4-board-entry-points` off post-chunk-3 `main`
  (`d7a8173c`).
- **Binding inputs of stage 2**: every decision D-1..D-12 below and the
  commit sequencing §8. Deviations need a recorded reason in the
  stage-2 report. The M-b letter (a CLI-only or asymmetric verb needs a
  *recorded justification*, not necessarily GUI parity) governs the
  affordance-vs-ruling split; the user's acceptance of the triage
  report is the cover for both directions.

## 1. Posture (inherited, restated for the chunk)

Two finding classes meet in this chunk:

- **M-b** (CLI-only mutation without recorded justification): T-8
  board.color, T-9 board.reset, T-10 board.reconcile. The remedy is
  either an affordance or a *dated ruling with justification* — the
  dispatch's §5 letter, applied per finding below.
- **M-a** (entry-point asymmetry — the verb exists but is reachable
  only through one buried or pointer-only path): T-11 BoardSelect,
  T-12 the default-preset CTA, T-13 TaskDueToday, T-20 the recur
  verbs, T-21 ReviewOpen. The remedy is parity: registry-driven keys
  and/or a promoted affordance so click ≡ key everywhere the verb
  renders.

The chunk's binding rule from the dispatch: **a silently skipped
finding is a BLOCKER; a ruling without recorded justification is a
BLOCKER** (the stage-3 trap list). Every finding below lands as
affordance or ruling exactly as decided here.

## 2. Code-seam inventory (verified at this worktree's HEAD `d7a8173c`)

| Seam | Live anchor | What it gives this chunk |
|---|---|---|
| `SetBoardColor` | `crates/taskboard-domain/src/messages.rs:170` — `{ board, color: Color }` | T-8's write path exists; zero GUI dispatch sites (the finding's sweep evidence) |
| `Color` + `DEFAULT_BOARD_COLOR` | `crates/taskboard-domain/src/entities.rs:26-42, 303` (`"0082c9"`) | the lenient newtype and the engine-side default the CLI create applies (`commands/boards.rs:85`); the GUI create's debt comment is `view_state.rs:1993-1994` |
| Swatch vocabulary + echo | `crates/taskboard-ui-slint/src/presentation.rs:427` (`LABEL_SWATCHES`, six lowercase hex bodies) and `models.rs::swatch_hex` (~`:2508`) | chunk 3's palette precedent: fixed swatch set + the lenient-color echo fn, reusable verbatim for boards (D-1) |
| Board bar | `ui/compositions/board-bar.slint` (strip segments ~`:186+`, dropdown at the budget, the archived roster W-12) | T-8's mount site (segment context menu + echo dots); `segment-hint` is the keymap-projection display input (round-1 D21b) |
| `BOARD_CONTEXT` | `crates/taskboard-ui-slint/src/menu.rs:221-227` | the board segment context menu (create family, rename, delete last) — T-8's menu row lands here |
| `ResetBoard` / `ReconcileByTitle` | `messages.rs:199 / :296`; CLI `boards reset --confirm` / `boards reconcile --confirm` (`cli.rs` Reset/Reconcile subcommands) | T-9/T-10's CLI verbs; both `NotUndoable(BoardCeremony)` with `SetBoardColor`/`SetBoardArchived`/`DeleteBoard`/`UnbindBoard` (`undo.rs:129-140`) |
| `Flow::ConfirmDelete` armed grammar | chunk 2's landed two-phase route (`view_state.rs::delete_stack/delete_board`, `adapter.rs::install_delete_strip_seams` ~`:8716+`) | the confirmation pattern T-9/T-10 would inherit **if afforded** (they are not — D-2/D-3; recorded here as the counterfactual the rulings must beat) |
| `BoardSelect` | `actions.rs:906-915` (GLOBAL_VIEW_NAV, menu rank 80); click route `adapter.rs::install_board_bar_seam` ~`:8672+` (sets `board_target`, then routes); `board-bar.slint:49-51` "empty today" hint projection | T-11: the action exists with a target-carrying route; no preset binds it; no target-free behavior |
| Board-bar dropdown walk | the shared `Dropdown` widget (`field-text`, `row-activated`, `BoardBarFacts.open/highlight` ~`:28-33`) | T-11's keyboard picker: the flyout's own walk exists and is the mechanism chunk 1's roster reused |
| Default-preset CTA | `adapter.rs::on_board_default_preset_clicked` ~`:8757-8765` → `view_state::apply_board_preset` (`:1904`; board-less refusal `:4657` family; the debt comment at `:1993`) | T-12: direct dispatch, no ActionId, no key (`board-layout.slint` empty-board leg) |
| `TaskDueToday` | `actions.rs:1160+` (contexts `&[ContextId::Today]`, menu row, no preset); router `:983-923` family (`ctx.focused_task` + `ctx.day_window` → `UpdateTask { duedate: Some(Some(window end)) }`); the "no default stroke" note `keymap/presets.rs:80-82` (round-10 D11) and `tests/journeys.rs:9050-9056` | T-13: menu-dispatch only; today rows' context menu is `open_task_context_menu` (`adapter.rs:8495-8502`) → `TASK_CONTEXT` shared with board cards |
| `TASK_CONTEXT` | `menu.rs` (~`:150-208`) — includes `SeriesPause/SeriesPauseCommit/SeriesResume/SeriesConvertArm/SeriesConvertToHabit`; the tracker hide-set filter precedent (`adapter.rs::open_task_context_menu`, issue #492) | T-20's corrected evidence (D-7): every card's right-click already routes the Series* verbs |
| Recur row | `ui/views/task-view.slint:788-832` (pause/resume/convert); `series-edit-enabled` kiosk flag (chunk 1) | the promoted detail-row affordance stands; kiosk posture untouched |
| `ReviewOpen` | `actions.rs:1541-1549` (GLOBAL, menu rank 75, no preset); routes `UiEffect::OpenReviewWalk` (`router.rs:991`); the Today walk-preview card routes the same | T-21: reachable-not-promoted; kiosk greys via `entry_states` (unchanged) |
| Presets | `crates/taskboard-ui-slint/src/keymap/presets.rs` (Standard `:69+`, Emacs, Vi with `g`-chord families `:635-636,665-674`) | the free-key decision (D-9); the keymap contract suite is the totality oracle |
| Context menus' hint chain | `menu.rs::object_entries` over `[Board, Global]` (the F-B7 fix) | new menu rows get binding hints for free in every preset |

## 3. Decisions

### D-1 — T-8 (#791): board.color is an affordance — a swatch picker armed from the board context menu, an echo dot on the strip segments, and the create default

M-b's letter is satisfied by *control + display*, because the color is
a per-board attribute the board bar (the board entity's owning mount,
ruling R-1's grammar) can both set and echo with existing machinery:

- **Entry point**: `ActionId::BoardColorEdit` (new, `ContextId::Board`,
  "Board color…", menu row in `BOARD_CONTEXT` between `BoardRename`
  and `BoardDelete`; registry hint generated). Context-menu row and
  keyboard route the same id (click ≡ key).
- **Picker shape**: the chunk-3 swatch-picker pattern (the importance
  picker's slot rows + the labels overlay's `LABEL_SWATCHES`
  vocabulary). Arming opens a small swatch strip mounted under the
  board bar (six swatch cells, the shared six-hex vocabulary — the
  const generalizes to `SWATCHES` used by labels and boards; one
  rename, no duplicated data). The board's current color is
  pre-marked (a ring on the matching swatch; no match = no mark, the
  lenient value rendered honestly). Activation of a swatch dispatches
  exactly `StateCommand::SetBoardColor { board: selected, color:
  Color::new(hex) }` + the models refresh; the strip closes.
- **Escape paths**: Escape / outside click / a foreign settle disarm
  the flow with nothing dispatched — a view-state flow marker (the
  `Flow::CreateBoard` shape: armed-with-no-buffer, the settle law
  applies verbatim). `SetBoardColor` is `NotUndoable(BoardCeremony)`
  (`undo.rs:132-137`), so the picker's own re-openability is the
  recovery: a mis-pick is reversed by picking again — the verb is
  self-reversing, which is the recorded reason no armed-confirm stage
  applies (the T-1 staging bar targets *irreversible* steps).
- **Echo**: the strip segments gain a color dot derived through the
  chunk-3 `swatch_hex` fn (generalized `pub(crate)`; a lenient
  non-hex value renders transparent — the GUI never re-derives, the
  D-5 hygiene verbatim). Dropdown mode's echo stays the field text
  (the dot rides strip segments only; recorded scoping, not a gap —
  the picker's pre-marked ring is the color's second echo).
- **Create debt retired**: the GUI create flow commits
  `Color::new(DEFAULT_BOARD_COLOR)` — the same engine const the CLI
  create applies when `--color` is omitted (`commands/boards.rs:85`)
  — replacing the lenient empty payload and deleting the
  "colour picker is recorded debt" comment (`view_state.rs:1993-1994`).
  No drift: the GUI now matches the CLI's default exactly (D-6's
  parity rule).

### D-2 — T-9 (#792): board.reset stays CLI-only — recorded GUI-absence ruling R-10

**Ruling (dated 2026-11-15, authority: this plan + the user's
acceptance of the triage report)**: `ResetBoard` has no GUI affordance,
by justification rather than omission:

1. Reset is the *local repair twin* of delete (same blast radius —
   stacks, tasks, labels, outbox, validators, reminder facts —
   `messages.rs:199-205`), `NotUndoable(BoardCeremony)`, and
   `--confirm`-gated in the CLI with a typed refusal that states what
   would be cleared. Affording it in the GUI would mint a new
   irrecoverable surface — the exact class chunk 2 just finished
   fencing; adding a staged strip would normalize a
   wipe-everything verb into the board bar's row set.
2. Every GUI-reachable need the verb serves is already covered by
   GUI-afforded verbs: clearing a board's content = armed
   `BoardDelete` (chunk 2) + `BoardCreate`; the bound-board
   re-materialization scenario reset exists for (fresh local ids from
   the next pull) is a sync-repair operation performed at the CLI
   administration surface, where the `--confirm` refusal text carries
   the match/clear facts.

The census GUI cells convert `U (BF-3)` → `— (R-10)`; the CLI cell
stays `X (pre-existing --confirm)`. The counterfactual is recorded:
**if** stage 3's walkthrough or the user surfaces a GUI journey that
terminates in reset, the affordance enters through chunk 2's
armed-confirm grammar (`Flow::ConfirmDelete`'s two-phase route) — the
pattern is already landed, so the ruling is revisitable at near-zero
design cost.

### D-3 — T-10 (#793): board.reconcile stays CLI-only — recorded GUI-absence ruling R-11

**Ruling (same date/authority)**: `ReconcileByTitle` has no GUI
affordance, by justification:

1. Reconcile repairs one state: same-title duplicates adopted by the
   bind ceremony's pull alongside pre-existing local rows — a state
   reached through the CLI-managed bind/publish flow, not through any
   GUI journey (the GUI cannot produce it: create never matches by
   title, "titles are not uniqueness keys", `cli.rs` Create doc).
2. The CLI gate is informed consent: `--confirm`'s typed refusal
   prints the match counts before the merge. A GUI affordance would
   either re-implement the match-count preview (a new request/render
   lane for a once-ever operation) or stage *without* the counts —
   strictly less informed than the CLI twin. Merging neutralizes
   duplicate rows as ref-less tombstones and is `NotUndoable` — an
   irreversible, preview-dependent verb fails the picker bar
   (D-1's self-reversing test) and the armed-strip bar (the strip
   would name a consequence the user cannot yet see).

The census GUI cells convert `U (BF-4)` → `— (R-11)`; CLI cell stays
`E (pre-existing --confirm)`. Revisitability note (same shape as
D-2's): a future GUI duplicates roster would carry the affordance with
armed-confirm + counts — filed nowhere until such a surface is
requested (no speculative issue; the ruling *is* the record).

### D-4 — T-11 (#794): BoardSelect gains a target-free keyboard flow — the stroke opens the switcher flyout

`BoardSelect`'s route is target-carrying (click sets `board_target`,
then routes). A bare key with no target is `NoTarget` — useless. The
affordance gives the target-free stroke a picker behavior:

- **Behavior**: firing `BoardSelect` with no one-shot target opens the
  board bar's switcher flyout (the shared `Dropdown`, site 1) in its
  existing keyboard walk (`BoardBarFacts.open/highlight` already
  mirror it, `board-bar.slint:28-33`); Return (or click) on a row
  routes the existing `segment-activated` seam — the same
  `BoardSelect`-with-target route. Escape closes with nothing
  dispatched (the dropdown's own dismissal contract). In dropdown
  mode (past the 8-segment budget) the same flow applies — the
  flyout *is* the switcher there.
- **Bindings** (candidates; the keymap contract tests are the oracle,
  the chunk-3 D-4 pattern): Standard `Ctrl+G` in `ContextId::Board`
  (the jump-to mnemonic; `b`/`B` stay the walk); Emacs per its own
  table; Vi `g b` in `ViNormal` (the `g`-chord family beside
  `gn`/`gR`/`gd`; `gt`/`gT` are taken by view nav). A colliding
  stroke is re-picked from the same context's free set — never by
  weakening a contract test.
- The segment hints (`segment-hint`, the keymap projection) populate
  automatically once the binding exists — the "empty today" comment at
  `board-bar.slint:49-51` retires to history.

### D-5 — T-12 (#795): the default-preset CTA routes through a new `BoardDefaultPreset` ActionId with an empty-board dispatch gate

- **ActionId**: `ActionId::BoardDefaultPreset` (new,
  `ContextId::Board`, "Apply default kanban preset"), registry entry
  with generated hint, `menu_group: Some(MenuGroup::Board)` rank
  beside the stack verbs.
- **Click ≡ key**: the CTA seam (`on_board_default_preset_clicked`)
  stops dispatching `apply_board_preset` directly and routes the
  ActionId (the §7.1 every-click-has-an-action-equivalent law — the
  current direct dispatch is the asymmetry the finding names).
- **Dispatch-time gate**: the keyboard stroke must not fire on a
  non-empty board (the CTA renders only in the empty-board leg, so
  today the pointer path cannot do this; the key could). The route
  resolves the *selected board's* stack count: empty →
  `apply_board_preset`; non-empty → the typed no-op (`NoTarget`
  family — the planner's dispatch-time-gating template, the
  `FocusModeToggle` row's comment is the precedent). The board-less
  refusal `apply_board_preset` already carries stands unchanged.
- **Bindings** (candidates, oracle as D-4): Standard `P` in
  `ContextId::Board`; Vi `g p`; Emacs per its table. The preset's
  creates are journal PreImages, so `AppUndo` reverses the whole
  preset (US-8's existing escape, unchanged).

### D-6 — CLI parity: the engine family is the contract; the GUI adds no semantics

- The GUI dispatches `SetBoardColor` with the picker swatch's exact
  six-hex body — the CLI's `parse_color` vocabulary — and never feeds
  `Color` free text (chunk-3 D-5's letter, restated and applied).
- The GUI create commits `DEFAULT_BOARD_COLOR` — the CLI's own
  omission default — so both front ends mint identical creates; no
  GUI-side default invention.
- No new `StateCommand`, no new `UiRequest`, no undo-class change:
  **the command-contract delta of this chunk is ActionIds only**
  (`BoardColorEdit`, `BoardDefaultPreset`).

### D-7 — T-20 (#803): ruling with corrected evidence — the card context menu already routes the Series* verbs

The triage's T-20 evidence ("the buried detail row is the universal
path") is incomplete: `TASK_CONTEXT` — the right-click menu of *every*
task card (board cards, stack-detail cards, and today rows via
`open_task_context_menu`, `adapter.rs:8495-8502`) — already carries
`SeriesPause`, `SeriesResume`, `SeriesConvertArm`, and
`SeriesConvertToHabit` (`menu.rs`, the list tail before `STACK_CONTEXT`),
and the context seams focus the addressed card first (`activate_card`)
so the verbs resolve against the clicked series. The recurrence plan's
own Today cell recorded the reach as "the GLOBAL menu rows, W-N1" —
the *card context menu* is the third, non-buried path both the
census cell and the finding missed.

**Ruling RC-1′ (dated 2026-11-15, authority: this plan + the
accepted triage)**: the pause/resume/convert entry points are already
asymmetric-free at the card level — pointer parity exists on every
surface that renders task cards; the detail recur row remains the
*promoted* surface (buttons vs menu rows — prominence, not
reachability); the recur badge stays display-only (a decoration is
not a control — the T-26/T-42 presentation-scoping class, and the
T-46 doctrine bar records card badges as display atoms). Keyboard
parity rides the GLOBAL menu rows' existing routes; no new key binds
this chunk (the verbs are contextual to a focused series; adding
keys would promote series mutation onto every card surface's stroke
space — prominence the detail row already owns).

Census updates: `task.recur_pause` / `task.recur_convert_to_habit`
Board and Today cells `U (RC-1/RC-2)` → `E` citing the corrected
`TASK_CONTEXT` evidence + the detail row; Review/Settings/Capture
cells → `—` under R-Pattern 3 (no task cards render there — nothing
for the entry points to mount on; the recurrence plan's own display
ruling R-2 (display row) carries the same sweep). The recurrence plan
gains a dated correction note on W-N1 (the badge-display fact stands;
the *reach* statement is corrected to the context menu).

### D-8 — T-13 (#796) and T-21 (#804): TaskDueToday and ReviewOpen gain preset bindings in every keymap

Both verbs are complete at the seam level (router arms, typed
context) and asymmetry-class only at the keymap — the M-a remedy is
bindings, with one deliberate note overturned:

- **TaskDueToday**: `ContextId::Today` rows in all three presets
  (candidates: Standard `T`; Emacs and Vi per their tables — Vi
  candidate `t` in `ViNormal`'s Today rows; the contract tests are
  the oracle). **The round-10 D11 note** ("task.due-today stays
  menu-reachable without default strokes", `presets.rs:80-82`) **is
  overturned with this recorded reason**: the accepted triage classed
  the absence MAJOR (T-13/M-a) — a today-re-commit verb living only
  in the two-tier menu fails keyboard/mouse parity for the Today
  surface's core loop; the acceptance of the report is the authority
  that retunes the round-10 posture. The journey test comment at
  `journeys.rs:9050` gains the same dated pointer.
- **Pointer twin**: today rows' context menu gains `TaskDueToday` as a
  *today-only* row via the tracker hide-set filter precedent
  (`open_task_context_menu` already filters `TASK_CONTEXT` per
  subject; the today seam extends the id list with `TaskDueToday`).
  The verb binds `ContextId::Today` only, so board/stack-detail
  cards' menus are untouched — the today-only filter is what keeps
  the row honest there.
- **ReviewOpen**: `ContextId::Global` rows in all three presets
  (candidates: Standard `Ctrl+Shift+R`; Emacs/Vi per their tables;
  oracle as above). The menu rank 75 and the kiosk `entry_states`
  greying stand unchanged; the Today walk-preview card remains the
  pointer path. The review-walk plan's US-1 entry-point list gains
  the key.

### D-9 — Key binding table and totality (the candidates the contract tests own)

| ActionId | Context | Standard (candidate) | Emacs | Vi | Note |
|---|---|---|---|---|---|
| `BoardSelect` (target-free flow) | Board | `Ctrl+G` | per table | `g b` | D-4 |
| `BoardColorEdit` (new) | Board | `Ctrl+F3` | per table | `g c` | D-1; beside `Ctrl+F2` rename |
| `BoardDefaultPreset` (new) | Board | `P` | per table | `g p` | D-5 |
| `TaskDueToday` | Today | `T` | per table | `t` | D-8 |
| `ReviewOpen` | Global | `Ctrl+Shift+R` | per table | `g w` | D-8 |

All candidates are recorded as *candidates*: stage 2 verifies against
the existing keymap contract suite (per-preset core-verb coverage,
conflict detection, snapshot regeneration — the chunk-3 D-4 posture
verbatim). A collision is re-picked from the same context's free set
and recorded in the stage-2 report; no preset loses a row (the
no-second-class contract). Hints are registry-generated — the menu
rows and `segment-hint` projection pick them up for free.

### D-10 — Amended journey steps and wiring rows (the owning plans' amendments)

- **board-stack-management plan**: new **US-12 — Set the board's
  color** (steps US-12/S1 arm the picker via `BoardColorEdit` /
  menu row; US-12/S2 pick a swatch → exact `SetBoardColor` dispatch +
  echo re-render; US-12/S3 Escape/outside-click → nothing dispatched;
  escape column: re-pick reverses, the settle law disarms). Wiring
  row **W-13** (the swatch strip + seams + `ActionId::BoardColorEdit`
  + `StateCommand::SetBoardColor`; row addressing: the selected board
  via the id-table pass). US-8 gains the keyboard entry point
  (`BoardDefaultPreset`) and an S2 step (the non-empty board's typed
  no-op); US-2 gains an S4 step (the target-free `BoardSelect` flow:
  stroke → flyout walk → Return); US-1/AC-1's lenient-color wording
  gains the dated default-color correction; US-10/S1's "no GUI twin"
  retires to history; US-10/S3/S4 gain the R-10/R-11 ruling pointers.
  §7 gains the ActionId delta row (D-6's "ActionIds only" line).
- **task-lifecycle plan**: US-6's entry points gain the preset keys
  and the today-row context-menu row; US-6/S1's escape cell's "no key
  and no pointer affordance" retires to history (dated T-13 pointer).
- **recurrence-family plan**: the D-7 ruling correction (W-N1 dated
  note; matrix cells per D-7).
- **review-walk-reflection plan**: US-1's entry points gain the
  preset key; the reachable-not-promoted finding RW-3 gains its
  dated remediation pointer.

All amendments follow the chunk-1/3 house form (new US blocks with
step tables, W-rows in the PC-2 cell grammar — bare
`StateCommand::`/`ActionId::` tokens, hyphenated callbacks in the
Adapter-seam column). The remediation-era step tokens (`us12_s*`,
`us8_s2`, `us2_s4`, `us6_s2`) are REQUIRED gate rows — stage 2 runs
`scripts/ux-coverage.sh <each amended plan> --gate` and every new
wiring row + step token must come back FOUND.

### D-11 — Census updates (beyond the plan matrices)

- Board slice: `board.color` Board cell `U (BF-1)` → `E (US-12)`
  (picker + echo evidence); other GUI columns → `—` under the
  board-entity mount grammar (R-1's letter: the board bar is the
  owning mount); CLI `E` stands. `board.reset` GUI cells → `— (R-10)`;
  `board.reconcile` GUI cells → `— (R-11)`; `board.select` Board cell
  gains the keyboard evidence; `board.default_preset` Board cell
  gains the ActionId/key evidence.
- Today slice: `task.due_today` Today cell `E (US-6)` evidence gains
  the keys + context row; the CLI cell stays `U` (T-24's family).
- Task detail slice: the T-20 cells per D-7.
- Review slice: `review.open` cells gain the binding evidence.
- The stocktake fates: T-8/T-11/T-12/T-13/T-21 resolve as
  remediated-by-code; T-9/T-10/T-20 resolve as ruled (dated, this
  plan's authority) — the issues close on merge per the dispatch's
  completion bookkeeping, the rulings' census annotations are the
  durable record.

### D-12 — Test catalogue (TDD-first; typed outcomes, never text)

- **Seam tests (exact commands)**, the `journeys.rs`/`view_state.rs`
  house shapes:
  - a swatch click dispatches `SetBoardColor { board: <selected>,
    color: Color::new(<swatch hex>) }` exactly (per-pick
    parameterized over the six-hex vocabulary) and closes the strip;
  - Escape / outside click / a foreign settle while armed dispatch
    nothing and clear the flow (the armed triple, the r2 shape
    verbatim);
  - the create flow's commit carries `Color::new(DEFAULT_BOARD_COLOR)`
    (the corrected AC-1);
  - `BoardDefaultPreset` on an empty board dispatches the explicit
    ordered `CreateStack` set (the existing preset test's assertion,
    now routed through the action); on a non-empty board it is the
    typed no-op — nothing drains; board-less stays the refusal;
  - the target-free `BoardSelect` stroke opens the flyout
    (`BoardBarFacts.open` fact) and a row activation dispatches the
    selection (the existing walk test's route); Escape closes with
    nothing dispatched;
  - the today-row context menu includes `TaskDueToday` on today rows
    and does not on board cards (structural menu-composition
    assertion — the tracker hide-set test's shape);
  - `ReviewOpen` routes from the registry (the router verb-test
    shape) — existing, unchanged route; only bindings are new.
- **Presentation tests** (typed flags/rows): the segment dot derives
  from the board's color through `swatch_hex` (a six-hex board echoes
  the typed color; a lenient value echoes transparent — the chunk-3
  hygiene test's shape); the picker strip enumerates the six swatch
  vocabulary in order with the current color's ring index; the
  `segment-hint` projection is non-empty for `BoardSelect` once bound
  (structural: the hint row exists, not its text).
- **Keymap totality**: the existing contract suite covers the five
  bindings + two new ActionIds (per-preset coverage, conflict
  detection, snapshot regeneration). No new test file.
- **Kiosk gates**: none new — the board surface does not mount on
  kiosk (the census grammar; the color picker and its echo are
  board-surface widgets), and T-13/T-21's keys are desktop keymap
  rows (kiosk's `entry_states` greying of `ReviewOpen` stands
  unchanged). Recorded so the absence is decided, not overlooked
  (the chunk-3 D-6 posture).
- **Journey-step token rows**: REQUIRED and gate-checked (D-10).

## 4. Non-goals (recorded, with homes)

- No GUI reset/reconcile affordances (D-2/D-3 rulings; revisit notes
  recorded in the rulings themselves).
- No badge-as-control for the recur badge, no series-verb keys (D-7's
  scoping; if the user wants badge controls it is a follow-up issue).
- No board color display beyond the segment dot + picker ring (the
  dropdown-mode scoping in D-1).
- No free-hex color input in the GUI (the CLI covers arbitrary
  values; chunk-3 D-5's scope line applies verbatim).
- No `boards` CLI additions, no archive-from-GUI verb (chunk 1's
  non-goal stands; the unarchive roster is chunk 1's landed work).
- No Settings, no break-rule work (chunk 5's covers).

## 5. Risks (recorded for stages 2–3)

- **R-1 — keymap snapshot churn**: five new binding rows regenerate
  the keymap snapshots; an Emacs/Vi retune is possible and must be
  recorded, never silently dropped from a preset.
- **R-2 — the `BoardSelect` flow's interplay with the flyout's
  pointer path**: opening the flyout from a stroke must not fight
  `settle_pointer_conflict` or the archived roster's flyout (site 3);
  stage 2 pins the settle ordering in a seam test (open → walk →
  Return; open → outside-click → closed, nothing dispatched).
- **R-3 — the preset gate's stack-count read**: the non-empty check
  reads the *selected board's* live stacks at dispatch time; a
  refresh mid-stroke (sync landing stacks) flips the verdict — the
  typed no-op is the safe side and the seam test pins both sides.
- **R-4 — `swatch_hex` generalization**: lifting the fn out of the
  label-chip path must not change the label echo (the existing
  presentation tests are the guard; rename-only, no behavior edit).

## 6. Kiosk posture

No new kiosk-visible surface: every affordance in this chunk mounts
on desktop board/Today/review surfaces under the existing mount
grammar (board surface desktop-only; today rows' context menu and
keymap rows are the existing desktop grammar; `ReviewOpen`'s kiosk
greying is unchanged). The chunk-1 visible-gating grammar has no new
instance — recorded (D-12's kiosk-gate line).

## 7. Estimated diff size

**M** (the dispatch's own sizing for an L chunk resolves lower
because T-9/T-10/T-20 land as rulings): ~550–800 lines — adapter
(+~120: color seams, preset route, today-context filter,
target-free select), view_state (+~80: color flow, create default,
preset gate), models/presentation (+~60: dot, picker rows, swatch
generalization), `board-bar.slint`/`app-window.slint`/
`board-layout.slint` (+~120), `actions.rs`/`menu.rs`/`presets.rs`
(+~80), tests (+~250–350), four plan amendments + census (+~150).

## 8. Commit sequencing (every commit standalone; hooks green per commit)

1. `feat(ui): the board create commits the default color and segments echo it`
   — presentation half first (TDD): `DEFAULT_BOARD_COLOR` create
   payload (debt comment deleted), `swatch_hex` generalization, the
   segment dot, presentation tests.
2. `feat(ui): board color picker arms from the board menu and dispatches SetBoardColor`
   — `BoardColorEdit` (registry + `BOARD_CONTEXT` row + presets +
   snapshots), the flow marker, the swatch strip, seams, seam tests
   (the armed triple + exact dispatch).
3. `feat(ui): the default-preset CTA routes BoardDefaultPreset with the empty-board gate`
   — ActionId + key + click ≡ key route + typed no-op gate + tests.
4. `feat(ui): BoardSelect opens the switcher flyout when fired without a target`
   — the target-free flow + bindings + tests (R-2's settle pins).
5. `feat(ui): due-today binds per preset and joins the today-row menu`
   — `TaskDueToday` Today rows in all presets + the today-only
   context filter + tests.
6. `feat(ui): review-open binds in every preset`
   — `ReviewOpen` Global rows + tests. (Small but standalone; kept
   separate from 5 so a retune of one verb never rewrites the
   other's snapshot diff.)
7. `docs(ux): the r4 amendments — board verb journeys, rulings R-10/R-11, census cells`
   — the four amended plans, the census updates (D-11), the dated
   pointers (round-10 D11 override, US-10 retirements, W-N1
   correction). The `ux-coverage.sh --gate` green run over each
   amended plan precedes this commit (evidence in the stage-2
   report).

Order rationale: presentation → seams → bindings → docs, the chunk-3
ladder; each commit compiles and tests standalone (the rebase-merge
commit-hygiene rule).

## 9. Long-command discipline (restated as binding for stage 2)

The coverage trio is forbidden in the subagent; the stage-2 gate is
the pre-commit hooks plus
`scripts/run-quality-gates.sh cargo nextest run --workspace`
(flock-safe, ~4–6 min — background + poll with useful work between
polls; all landed work committed before any wait). The full AGENTS §10
suite is dispatcher-owned; CI's coverage check runs it authoritatively
on the PR.

## 10. Backlog findings filed during planning

- **The r1 plan's journal-eligibility claim for `SetBoardArchived` is
  wrong** (new `backlog:` issue): the chunk-1 plan's amendment and
  US-11's acceptance record "`SetBoardArchived` is journal-eligible
  (`undo.rs:324`), so `AppUndo` reverses" — at this HEAD the verb is
  `NotUndoable(BoardCeremony)` (`undo.rs:132-140`; the `:324` site is
  the board-extraction helper, not a class). The unarchive journey's
  escape column cites AppUndo as a recovery path that does not exist.
  Out of this chunk's covers (it is chunk 1's landed artefact and no
  test asserts the reversal); filed, with a proposed fix prompt
  (amend the plan's escape cell to the honest "unarchive reverses by
  re-archiving from the bar" + correct the `undo.rs` citation).
- Comments on #791/#792/#793/#794/#795/#796/#803/#804 recording the
  stage-1 decisions (affordance or ruling, D-refs) and the closing
  evidence path.
