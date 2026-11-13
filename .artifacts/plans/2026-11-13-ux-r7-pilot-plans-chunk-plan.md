# [Plan] UX Journey Framework — Phase 2 Pilot, Chunk 1: the Blind Round-7 UX Plans (Stage 1)

- **Date**: 2026-11-13
- **Status**: STAGE-1 PLAN (authoring spec for three blind UX journey plans;
  no implementation in this commit). Stage 2 authors exactly these three
  plans; stage 3 reviews against this spec + the dispatch's acceptance.
- **Origin**: framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  (ACCEPTED, PR #761 merged; normative §4.1 template bar, §4.3 gate, §6.1
  pilot discipline step 1) + dispatch plan
  `.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` §1/§2
  (chunk 1, branch `docs/ux-r7-pilot-plans`) + the design origin
  `.artifacts/plans/2026-11-13-gui-review-round7-plan.md` §2
  (D-7.1/D-7.2/D-7.3) + the installed law `docs/ux_planning.org`.
- **Blindness baseline (verified, binding for stages 2–3)**: **`35e6b0a1`**
  ("docs(plan): the gui-review-round7 stage-1 and dispatch plans"). Verified
  via `git log origin/main` on 2026-11-13: the first ui-r7 implementation
  chunk (habit create) merged as `8ee2d827` (its plan commit) through
  `6406974e`, followed by the framework-install commits `a4c87291`…`e374e547`
  (PR #761, mergeCommit `e374e547`, mergedAt 2026-11-13T14:21:36Z —
  precondition checked before planning). Everything at and after `8ee2d827`
  is post-baseline. All design/surface reads in this chunk use
  `git show 35e6b0a1:<path>`; anchors cited below are baseline anchors.
- **Diff shape (binding)**: exactly four files on the branch — (1) this
  chunk plan (committed by stage 1), (2)
  `<today>-r7-habit-create-ux-plan.md`, (3)
  `<today>-r7-recur-editor-ux-plan.md`, (4)
  `<today>-r7-files-roster-ux-plan.md`, all under `.artifacts/plans/`, each
  authored from `.artifacts/templates/ux-journey-plan.md`. Non-goals: no
  comparison, no census file, no code changes, no reading of the
  `feat/ui-r7-habit-create` / `feat/ui-r7-recur-editor` branches, PRs,
  worktrees, or any post-baseline `main` content (dispatch §1).

---

## 1. Decisions (each binding for stage 2)

### D1 — Blindness protocol, and the gate's evidence root must be a baseline extraction

The worktree for this branch checks out `main` @ `e374e547`, which already
**contains** the landed habit-create implementation (`6406974e` and
siblings merged before the framework install). Two consequences, both
binding:

1. **Reading**: every surface file, and the round-7 stage-1 plan, is read
   only as `git show 35e6b0a1:<path>`. The working-tree copies of
   `crates/taskboard-ui-slint/**`, `crates/taskboard-app/**`,
   `crates/taskboard-domain/**` are post-baseline and forbidden. Also
   forbidden: `.artifacts/plans/2026-11-13-ui-r7-habit-create-plan.md` (it
   landed at `8ee2d827`), anything named `*ui-r7*` outside the round-7
   stage-1 plan and dispatch plan (both committed *at* the baseline), and
   the `taskboard-WT-feat-ui-r7-recur-editor` worktree (another agent
   round's; never entered).
2. **Gate runs**: `scripts/ux-coverage.sh` greps evidence under
   `--root` (default: the git toplevel — i.e. the contaminated worktree).
   Stage 2 must therefore run every parse against a one-shot baseline
   extraction, created fresh in the worktree (idempotent, outside the
   repo):

   ```sh
   rm -rf /tmp/taskboard-ux-r7-baseline-35e6b0a1
   mkdir -p /tmp/taskboard-ux-r7-baseline-35e6b0a1
   git -C /home/viktoria/src/taskboard archive 35e6b0a1 \
     | tar -x -C /tmp/taskboard-ux-r7-baseline-35e6b0a1
   scripts/ux-coverage.sh <plan.md> \
     --root /tmp/taskboard-ux-r7-baseline-35e6b0a1
   ```

   Expected outcome per plan: **no `PARSE-ERROR` lines, every evidence
   row for a *new* seam `MISSING`** — the new seams the plans name do
   not exist at `35e6b0a1` (that is the correct pre-state the dispatch's
   acceptance names, and it proves the parser contract end-to-end).
   Rows naming commands the design declares *pre-existing*
   (`CreateHabit`, `UpdateTask`, `MoveBoardToFiles`, `UnbindBoard`,
   `BindBoard`) legitimately read `FOUND` at their baseline anchors —
   that is the "exists" claim verified, not a blindness break — and the
   rulings' reason checks read `FOUND` in the plan itself. A plain
   (informational) run
   only — `--gate` is chunk 2's and the implementation chunks' instrument.

Stage 3 re-verifies blindness by checking every authority anchor in the
three plans resolves at `35e6b0a1` (`git show`) and none cites post-baseline
content.

### D2 — Story/step granularity rule (all three plans)

- **One story per completable user outcome** (a verb a user can drive to
  its visible end state from a named entry point). No escape-only stories:
  escapes live in the step tables' mandatory escape/failure column; no
  display-only story unless the display itself is the round-7 finding's
  headline outcome (it is, for the files roster — see D5's US-1).
- **Step granularity**: Cockburn — one user action per step, the exact
  dispatch named in the step's Dispatches cell (or `— display only`), the
  system response naming state change + UI feedback (badge/tone/dot/caption
  semantics, never label prose). Every step's escape cell is non-empty
  (cancel / armed-cancel / typed refusal / accepted no-op / navigation
  back). A step without an escape path is a plan defect (template law).
- **Entry points: click ≡ key, structurally.** Every story lists every
  click affordance *and* the keyboard `ActionId` that routes the same
  intent (the round-7 §3 accessibility ledger is the authority). Free-key
  per-preset choices stay implementation details and are recorded as plan
  assumptions (D7), never as fixed bindings.
- **Platforms**: shared story set with platform tags (framework §8
  decision 4 — no separate kiosk story sets). Kiosk posture is recorded
  per story in the Platforms line and again in §8's kiosk-gates bullet,
  using the D-T16 gate semantics the round-7 plan states ("the gate
  empties the ACTION, never the model").
- **Priorities**: every story in these three plans is P1 — each round-7
  observation is one journey, and the P1-slice rule (framework §4.2) says
  the first slice must complete it end-to-end. If stage 2 finds itself
  wanting a P2/P3 story, that is scope creep: rule it into §1 non-goals
  with a home instead.

### D3 — Plan A: `<today>-r7-habit-create-ux-plan.md` (design origin D-7.2)

**Scope**: create a habit from the Today section — header add affordance +
inline create form (title, target kind, quota) dispatching
`StateCommand::CreateHabit { board, title, target }` with the section's own
board; `ActionId::HabitCreate` ("habit.create") as the section-scoped
keyboard verb in the `Today` context. No engine changes (CreateHabit exists,
`messages.rs:461-466` baseline); no target editing UI (existing
`UpdateHabit`/CLI `habits update` posture unchanged).

**Capability × surface matrix** — rows (target state; cells cite story or
ruling IDs; baseline state lives in the validation-assertion block, not in
cells):

| Row | Cells |
|---|---|
| `habit.create` | Today `C (US-1)`; CLI `C (pre-existing, habits add)`; Board/TaskDetail/Review/Settings/Capture `— (R-1)` |
| `habit.edit` | all GUI surfaces `— (R-2)`; CLI `E (pre-existing, habits update)` |
| `habit.remove` | all GUI surfaces `— (R-3)`; CLI `X (pre-existing, habits rm)` |
| `habit.log` | Today `E (pre-existing, D-P13 grammar)`; CLI `E (pre-existing)`; all other GUI surfaces blank (untouched, not core) |

Rulings:

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | `habit.create` surfaces only in Today (rows are board-scoped there) and the CLI | creation context is the tracker list itself (D-7.2: board comes from the section, no picker); scattering create entry points breaks the section-scoped grammar | this plan §1; round-7 plan §2 D-7.2 |
| R-2 | no GUI habit editing (target retargeting included) | recorded non-goal: "No habit target editing UI beyond create … not requested" (round-7 plan §4); CLI `habits update` covers | round-7 plan §4 |
| R-3 | no GUI habit removal | not requested this round; CLI `habits rm` covers; the Today row grammar stays log/reorder/repair/convert | round-7 plan §1 R7-2 scope; CLI `habits rm` |

**Stories** (one P1 story; granularity per D2):

- **US-1 (P1, desktop; kiosk posture: affordance does not mount)** — As a
  desktop user I want to create a habit from the Today section's header so
  that a new tracker starts counting without leaving the view. Entry
  points: the header `IconButton` (plus icon, "Add habit…" — design-named
  affordance; callback `create-asked()` design-named) and
  `ActionId::HabitCreate` (keyboard, `Today` context). Steps: S1 arm the
  inline form (click or action id); S2 type the title (insert mode, commits
  on Return per the create-row precedent — escape: Return-less focus loss
  discards the field per house text-flow rule); S3 pick the target kind
  (`SegmentedControl` Per day / Per week / Per month); S4 set the quota
  (`NonZeroCount::new`, default 1 — zero unrepresentable, validity fold);
  S5 confirm → `CreateHabit { board: <section board>, title, target }` →
  row appears in the section. Escapes: cancel/Escape disarms and discards
  the draft at every step (stale-arm no-ops follow the one-shot
  target-field law); S2/S4 invalid drafts keep the form armed with the
  validity fold, confirm stays inert; no active board ⇒ the section renders
  no rows and S1's affordance does not mount (design-fixed posture).

**Flows**: one `flowchart TD` for US-1 (both entry points as sources →
armed form → validity decision → confirm → section row; cancel edges to
disarmed). **Plus a small `stateDiagram-v2`** for the inline form machine
(`Disarmed → Armed → (Invalid ⇄ Valid echo) → Committing → Disarmed`;
cancel/Escape edges from every state to Disarmed; kiosk flip mid-flow
unmounts but the adapter keeps the armed flag — the convert-strip
precedent). The template binds a state machine per editor/modal; the
inline form is one, so it gets one (recorded because D-7.2 does not name
it a machine — this is a framework-conformance addition, not a design
change).

**Wiring inventory rows** (design-named seams; unknowns to D7 assumptions):

| ID | Content |
|---|---|
| W-1 | header add `IconButton` in `ui/compositions/habits.slint` (widget gated by `editing-visible`, D-T16), callback `create-asked()`, pass-through via `today-view.slint` / `app-window.slint`, adapter seam = the habits-section seam (name assumed, A-1), `ActionId::HabitCreate`, commands `CreateHabit`, row addressing `—` (section-scoped, no list row), steps `US-1/S1`, test token `us1_s1` |
| W-2 | the inline create form (title `TippedField`, target `SegmentedControl`, quota field, confirm/cancel `IconButton`s), callbacks assumed (A-2), commands `CreateHabit`, steps `US-1/S2–S5`, tests `us1_s2`…`us1_s5` |

**Command-contract delta**: `ActionId::HabitCreate` (new); `StateCommand::CreateHabit`
(exists — no payload change; usage note: board resolved from the section's
own board). No `UiRequest`.

**Validation-assertion block** (§1 subsection, wording per D6): at
   `35e6b0a1`, `habit.create` reads blank/unruled on every GUI surface — the
   section header is a static caption (`habits.slint:63-76` baseline: RoleText
   "Habits" + count, no affordance), the action registry has no create verb
(`actions.rs:286-305` baseline: HabitLog/HabitMove/HabitRepair/
HabitConvertArm/HabitConvertToSeries only, pointer-grade), creation is
   CLI-only (`commands/habits.rs:147` `HabitsInvocation::Add`) ⇒ **R7-2 is an
   unruled absence the framework, applied at planning time, flags before any
   implementation exists.**

### D4 — Plan B: `<today>-r7-recur-editor-ux-plan.md` (design origin D-7.1)

**Scope**: a recurrence-rule editor overlay in the task detail view — new
composition `ui/compositions/recur-editor.slint` (design-named file),
window-level overlay on `ContextId::RecurEditor` (mask semantics per
`TrackEntries`; fields ride `ViInsert`; commit/cancel in-context; Escape
resolves globally); pattern `SegmentedControl` Daily/Weekly/Monthly with
per-pattern fields (`EveryDays` ≥1; weekly interval + seven weekday toggles
pre-checked from the anchor; monthly `DayOfMonth` 1–31 or Nth-weekday
pair); bound `SegmentedControl` Never/Until/Count (Until reuses the
due-editor date grammar `YYYY-MM-DD`/`+Nd`; Count positive integer);
live echo caption renders `rule.text_form()`, invalid drafts render the
typed `InvalidRecur` variant as a hint (presentation-side mapping — tests
assert the mapping, never hint text). Mount affordances: "Add repeat…"
(rule-less tasks, adjacent to the due/sched rows) and "Edit repeat…"
(beside the pattern echo) — both IconButton-with-label, walk-ordered,
hover-tipped; `ActionId::SeriesEdit` ("series.edit") in the `TaskDetail`
context. Commit dispatches `TaskChanges { recur: Some(Some(rule)) }`
inside `StateCommand::UpdateTask`; "Stop repeating" is an armed-confirm
strip inside the editor (the convert precedent) dispatching `Some(None)`.
Rides with it (same round-7 chunk scope, recorded in the plan's §1): the
pure-function extraction `recur::assemble_rule(&Task, …) ->
Result<RecurRule, InvalidRecur>` into `taskboard-domain` (the CLI's
minting laws: keep `series`/`continued_from` when a rule exists, anchor =
`series_reference_date`, weekday/day-of-month defaults from the anchor,
`paused_until` preserved) and the CLI `tasks repeat` refactored onto it —
one behavior, one implementation. No schema/message changes.

**Capability × surface matrix** — rows:

| Row | Cells |
|---|---|
| `task.recur_set` | TaskDetail `E (US-1, US-2)`; CLI `E (pre-existing, tasks repeat)`; Capture `— (R-1)`; Board/Today/Review/Settings `— (R-2)` |
| `task.recur_clear` ("stop repeating") | TaskDetail `E (US-3)`; CLI `E (pre-existing, tasks repeat --off)`; all other GUI surfaces `— (R-2)`; Capture `— (R-1)` |
| `task.recur_preview` (draft live echo) | TaskDetail `D (US-1, US-2 — inside the editor)`; every other surface blank (display is editor-scoped, not core) |
| `task.recur_display` (badge + pattern echo) | Board `D (pre-existing card badge)`; TaskDetail `D (pre-existing echo)`; Today/Review/Settings blank (untouched) |

Rulings:

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | no recurrence scheduling at create — capture overlay and inline create rows stay title-only | the recorded ruling stands (capture overlay header, `capture-overlay.slint:2-6` baseline); the editor is the setting; round-7 §4 reaffirms | recorded ruling `capture-overlay.slint:2-6`; round-7 plan §4 |
| R-2 | the editor mounts only in the task detail view, where the recur row renders | single-surface editor posture (D-7.1); other task lists reach it via the task detail; no per-surface editor duplication | this plan §1; round-7 plan §2 D-7.1 |

**Stories** (three P1 stories; kiosk posture for all: rides the existing
task-detail editing gate like every other action — display (badge/echo)
unaffected):

- **US-1 (P1, desktop)** — As a desktop user I want to set a recurrence
  rule on a rule-less task from its detail view so that the series starts
  without a CLI round-trip. Entry points: the "Add repeat…" IconButton and
  `ActionId::SeriesEdit`. Steps: S1 open the editor overlay (armed empty);
  S2 pick the pattern (segmented; per-pattern fields mount per the
  machine); S3 pick the bound (segmented; Until/Count fields per the
  machine); S4 read the live echo (valid draft → `rule.text_form()`;
  invalid → the typed `InvalidRecur` fold); S5 commit →
  `UpdateTask { changes: TaskChanges { recur: Some(Some(rule)), ..Default::default() } }`
  → overlay closes, recur row + badge appear. Escapes: Escape/cancel at
  any step closes the overlay discarding the draft (global Escape law);
  S2–S4 invalid drafts never block editing other fields but keep commit
  inert (S5's escape = the invalid fold); S5 typed failure surfaces the
  error fold without losing the draft (editor stays open).
- **US-2 (P1, desktop)** — As a desktop user I want to edit or replace an
  existing rule so that a changed routine updates the series without
  detaching it. Entry points: the "Edit repeat…" IconButton beside the
  pattern echo, same `ActionId::SeriesEdit` (armed with the existing
  rule). Steps: S1 open the editor pre-filled from the current rule
  (pre-fill mapping is an assumption, A-5); S2 adjust pattern/bound
  (assembly laws: existing `series`/`continued_from` and `paused_until`
  preserved — the extracted `assemble_rule` semantics); S3 read the echo;
  S4 commit (same command shape as US-1/S5 — sparse `TaskChanges` keeps
  every other task field untouched). Escapes: as US-1 (cancel discards
  edits; the live rule is untouched until commit).
- **US-3 (P1, desktop)** — As a desktop user I want to stop a series from
  repeating so that the head stays as an ordinary task. Entry points: the
  "Stop repeating" control inside the editor overlay (armed-confirm
  strip). Steps: S1 arm the confirm strip (consequence-first caption, the
  convert precedent); S2 confirm → `TaskChanges { recur: Some(None) }`
  (the detach semantic) → overlay closes, badge/row unmount, the task is
  rule-less again ("Add repeat…" posture returns). Escapes: cancel
  disarms back into the editor; the detach itself is a state change with
  the engine's existing stamping/sidecar/completion-spawn machinery
  downstream (already live and tested per D-7.1 — the plan's escape column
  records cancel-before-commit as the recovery; no undo claim is made).

**Flows**: one `flowchart TD` per story (entry points as sources: US-1
"Add repeat…" + `SeriesEdit`; US-2 "Edit repeat…"; US-3 lives inside the
editor machine). **`stateDiagram-v2` is mandatory** for the overlay
machine (framework §4.1.5; dispatch §2): states `Closed → Drafting
(Add|Edit mount) → InvalidEcho ⇄ ValidEcho → ArmedConfirm (stop strip) →
Committing → Closed`; edges: mount from both affordances + action id;
pattern/bound edits self-loop Drafting; validity folds branch Invalid/Valid;
arm stop-confirm from any drafting state; confirm dispatches and closes;
cancel/Escape from every state → Closed with draft discard; kiosk editing
gate keeps the machine unmountable mid-flow (adapter keeps armed flag —
the convert-strip remount precedent).

**Wiring inventory rows**:

| ID | Content |
|---|---|
| W-1 | "Add repeat…" affordance in `ui/views/task-view.slint` (adjacent to the due/sched rows), callback assumed on the `recur-*-asked` grammar (A-3), `ActionId::SeriesEdit`, commands `UpdateTask` (via editor commit), steps `US-1/S1`, test `us1_s1` |
| W-2 | "Edit repeat…" affordance beside the pattern echo (same file), callback assumed (A-3), `ActionId::SeriesEdit`, steps `US-2/S1`, test `us2_s1` |
| W-3 | `ui/compositions/recur-editor.slint` (new; design-named), `ContextId::RecurEditor` (design-named value; enum variant name assumed, A-4), commit/cancel callbacks assumed (A-3), commands `UpdateTask`, steps `US-1/S5, US-2/S4, US-3/S2`, tests `us1_s5`, `us2_s4`, `us3_s2` |
| W-4 | the armed "Stop repeating" strip inside the editor (design-named affordance), callbacks assumed (A-3), commands `UpdateTask` (`recur: Some(None)`), steps `US-3/S1–S2`, tests `us3_s1`, `us3_s2` |

**Command-contract delta**: `ActionId::SeriesEdit` (new);
`StateCommand::UpdateTask` (exists — usage note: the recur payload pair
`Some(Some(rule))` / `Some(None)`; sparse changeset, other fields
untouched). No `UiRequest`. (The `recur::assemble_rule` extraction is
domain truth, not a UI seam — it is recorded in §1 scope, not §7.)

**Validation-assertion block**: at `35e6b0a1`, `task.recur_set` /
`task.recur_clear` read blank/unruled on every GUI surface — the recur row
renders display + pause/resume/convert only (`task-view.slint:726-831`
baseline; callbacks `recur-pause-asked/accepted/cancelled`,
`recur-resume-clicked`, `recur-convert-asked/confirmed/cancelled` at
`task-view.slint:163-169`; no edit affordance), no UI site constructs
`TaskChanges { recur: … }` (round-7 §1 R7-1, verified over every UI
`TaskChanges` construction), the at-create exclusion is a recorded ruling
(`capture-overlay.slint:2-6`), and setting/clearing is CLI-only
(`commands/repeat.rs:306-360` `assemble_rule`; `--off` at
`repeat.rs:33,125`) ⇒ **R7-1 unruled at planning time.**

### D5 — Plan C: `<today>-r7-files-roster-ux-plan.md` (design origin D-7.3)

**Scope**: the Files section of every Nextcloud account card becomes the
per-account board roster — channel-relative badge taxonomy
(`FilesBoardBadge { ThisConnection, LocalOnly, OtherConnection }` with the
design-fixed tone map success/neutral/info onto the existing `Badge`
widget and the design-fixed verbatim labels), the per-account sort fold
(ThisConnection → LocalOnly → OtherConnection; within a group by
(title, id)), one `SettingsButton` per roster row whose label/enabled is
computed Rust-side, snapshot widening `BoardInfo.bound_channel:
Option<BoardChannel>` (`enum BoardChannel { Deck, Files }`, filled
app-side from `binding.board.as_deck()/as_files()` — structural detection
per D-BF4, never a name match), and three row actions: Stop Sync (new
`UiRequest::StopBoardFilesSync { board, reply }` → guards →
`StateCommand::UnbindBoard`, reply = fresh `ConnectionsSnapshot`),
Switch to Sync via this Connection (reuses `UiRequest::MoveBoardToFiles`
verbatim; enabled only on the anchor card with the lane on), Sync via
this Connection (new `UiRequest::PublishBoardToFiles { board, reply }` →
guards → the `BindBoard { remote: RemoteBoardId::files(uuid), connector:
FILES_SYNC_CONNECTOR_NAME }` ceremony). The `FilesMovePicker` dropdown,
its copy global, and the `move_choices` table **retire** — the roster is
the picker. No engine/domain/state/storage changes. The single-lane
posture (#723) stands: non-anchor cards render the full roster with the
actions disabled (the Files row's existing restart caption explains why);
archived boards render flagged archived.

**Capability × surface matrix** — rows (letter convention, recorded: bind
verbs are `E` on the board's binding, unbind is `X` of the binding; the
binding is named in the cell text to avoid implying board removal):

| Row | Cells |
|---|---|
| `board.roster_display` (per-account Files roster with badges) | Settings `D (US-1)`; all other surfaces blank (roster is Settings-scoped, not core) |
| `board.unbind_files` ("Stop Sync") | Settings `X (binding only, US-2)`; CLI `X (pre-existing, boards unbind)`; Board/TaskDetail/Today/Review/Capture `— (R-1)` |
| `board.move_to_files` ("Switch to Sync via this Connection") | Settings `E (US-3)`; CLI `E (pre-existing, boards move --to-files)`; other GUI surfaces `— (R-1)` |
| `board.publish_to_files` ("Sync via this Connection") | Settings `E (US-4)`; CLI `— (R-2, #764)`; other GUI surfaces `— (R-1)` |

Rulings:

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | binding actions live only in the Settings account roster | the roster row *is* the affordance (one action per row, contextually labeled); per-account context is the action's precondition (anchor card, lane on), which only Settings provides | this plan §1; round-7 plan §2 D-7.3 |
| R-2 | no `boards publish --to-files` CLI verb in this slice | deliberate GUI-first slice (the user scenario is the GUI roster); the asymmetry is filed | #764 |
| R-3 | non-anchor accounts show disabled actions with honest captions, not lanes | the single-lane posture (#723) stands; true multi-lane is #723's scope | #723 |
| R-4 | "Stop Sync" unbind leaves the remote `boards/<uuid>/` subtree in place | fold-first discovery defers adoption for unbound boards so nothing re-adopts it, but the bytes linger until the DELETE-verb GC exists | #755 (design question), #718 (GC verb) |
| R-5 | Deck→Deck moves stay out of scope; a Deck-bound board on another connector reads "Synced via other Connection" truthfully | no Deck→Deck rebind ceremony exists | #756 |

**Stories** (four P1 stories; platforms: Settings surface per its existing
platform posture — unchanged by this feature; the fold is kiosk-safe by
construction per D-7.3):

- **US-1 (P1, display)** — When I manage an account's Files sync I want to
  see its board roster with per-board sync badges so that I know what is
  mirrored before I act (job-story form; situation-triggered). Entry
  points: opening the Settings view's account card (existing navigation).
  Steps: S1 open Settings (`— display only`); S2 read a card's Files rows
  (badge per board: taxonomy + tone; sort per the fold; archived flagged);
  S3 read an action's posture (label + enabled state per channel — on the
  anchor card with the lane on vs elsewhere). Escapes: back navigation
  (existing); a stale snapshot re-renders from the next reply (the
  standard refresh discipline — recorded, not new machinery).
- **US-2 (P1)** — As a desktop user I want to stop syncing a board from
  the anchor card's roster so that the board returns to local-only.
  Entry points: the row's "Stop Sync" `SettingsButton` (click; keyboard:
  the Settings context walk — Nav next/prev + invoke, `presets.rs:232-236`
  baseline — reaches every enabled tab stop, so no new ActionId is
  needed; this is the round-7 §3 ledger's parity argument for D-7.3 and
  the plan records it as the keyboard entry point). Steps: S1 press the
  row action → `StopBoardFilesSync { board, reply }`; S2 app arm guards
  (board live, binding Files-typed; anything else → typed refusal); S3
  success → `UnbindBoard` on the engine handle, reply = fresh
  `ConnectionsSnapshot`, badge flips to LocalOnly, row action re-arms as
  "Sync via this Connection". Escapes: S2 typed refusal surfaces the
  error fold (board gone / not Files-bound — the row grammar can't
  normally offer it; the arm still refuses typed); the #755 leftover is
  recorded in the success response cell and R-4 (not hidden).
- **US-3 (P1)** — As a desktop user I want to switch a Deck-bound board to
  this card's Files lane so that the board mirrors through this account.
  Entry points: the row's "Switch to Sync via this Connection"
  `SettingsButton` (enabled only on the anchor card with the lane on;
  keyboard = the same Settings walk). Steps: S1 press →
  `MoveBoardToFiles { board, reply }` (verbatim reuse — the ceremony,
  `BusyDot`, `PendingKind::Move`, typed error fold all exist); S2 success
  → badge flips to ThisConnection. Escapes: typed refusals (board not
  Deck-bound / lane off / outbox still moving) surface the error fold;
  the disabled affordance on non-anchor cards is the honest pre-escape
  (R-3); the retired picker cannot be mis-invoked anymore.
- **US-4 (P1)** — As a desktop user I want to publish a local-only board
  to this card's Files so that it reaches the account's Files without
  ever touching Deck. Entry points: the row's "Sync via this Connection"
  `SettingsButton` (keyboard = the Settings walk). Steps: S1 press →
  `PublishBoardToFiles { board, reply }`; S2 app arm guards
  (`files_sync_spawn_decision` resolvable, board live, binding `None`);
  S3 success → the `BindBoard` ceremony against the files connector,
  reply = fresh snapshot, badge flips to ThisConnection, the files
  actor's lanes re-derive the board set next cycle (fold-first adoption
  covers pre-existing subtrees). Escapes: S2 typed refusals — Deck-bound
  → `BindRefused` pointing at Switch (US-3), Files-bound → the row would
  have offered Stop (US-2), spawn decision unresolvable → typed error
  fold; restart-pending enable (config written, actor not yet running)
  succeeds as a state change with the badge flipping on bind and the lane
  catching up next boot (the On/Off toggle caption's documented posture —
  recorded in the success cell, not as a failure).

**Flows**: one `flowchart TD` per story (US-1: Settings entry → roster
read; US-2/3/4: roster row action → guards → ceremony → badge/snapshot
feedback, with the typed-refusal branches as explicit edges). **No
`stateDiagram-v2`** — recorded reason: this solution introduces no
overlay/editor/modal (it *removes* one: the `FilesMovePicker`); the
move ceremony's busy state is the existing pending machinery, not a new
machine. The template's machine law binds overlays/editors/modals only.

**Wiring inventory rows**:

| ID | Content |
|---|---|
| W-1 | snapshot widening: `BoardInfo.bound_channel` (`BoardChannel`), filled app-side in `current_connections` (structural detection), surface `settings-view.slint` roster rows consume it via the fold; commands `—`; journey-steps cell carries the reason ("snapshot data plumbing — no dispatch") alongside `US-1/S2`; test `us1_s2` |
| W-2 | the badge + sort folds (presentation, sibling of `deck_bound_boards`; `FilesBoardBadge` enum design-named, location assumed A-6), rendered by the existing `Badge` widget; commands `—`; journey-steps cell carries the reason ("pure presentation fold — no dispatch") alongside `US-1/S2–S3`; tests `us1_s2`, `us1_s3` |
| W-3 | the roster row grammar: every live non-deleted board emits once per Nextcloud account card as `ConnectionItem { kind: files-board }` with badge/action label/enabled computed Rust-side (struct field additions assumed, A-6); replaces the title-only rows (`adapter.rs:4867-4883` baseline); the per-row `SettingsButton` joins the walk; callback assumed (A-6); steps `US-1/S2, US-2/S1, US-3/S1, US-4/S1`, tests `us2_s1` etc. |
| W-4 | `UiRequest::StopBoardFilesSync` (new, design-named) + app arm (guards → `UnbindBoard` + snapshot reply); steps `US-2/S1–S3`, tests `us2_s1`…`us2_s3` |
| W-5 | `UiRequest::MoveBoardToFiles` (exists, baseline `requests.rs:425-428`) reused verbatim from the row button; the `FilesMovePicker` component, its copy global, and the `move_choices` table retire; steps `US-3/S1–S2`, tests `us3_s1`, `us3_s2` |
| W-6 | `UiRequest::PublishBoardToFiles` (new, design-named) + app arm (guards → the `BindBoard` files ceremony per `bootstrap.rs:103` baseline); steps `US-4/S1–S3`, tests `us4_s1`…`us4_s3` |

**Command-contract delta**: `UiRequest::StopBoardFilesSync` (new);
`UiRequest::PublishBoardToFiles` (new); `UiRequest::MoveBoardToFiles`
(exists — reused from a new affordance, no shape change);
`StateCommand::UnbindBoard` (exists — now UI-dispatched);
`StateCommand::BindBoard` (exists — now UI-dispatched by the publish
ceremony); `UiRequest`-kind row for `BoardInfo.bound_channel` (the
struct lives in `requests.rs` — the gate's Kind-selected grep resolves
it there).

**Validation-assertion block**: at `35e6b0a1`, `board.unbind_files` /
`board.move_to_files` / `board.publish_to_files` read blank/unruled in
Settings — the Files card emits title-only rows with no actions, duplicated
under every Nextcloud account card (`adapter.rs:4867-4883` baseline, inside
the per-card block), the no-action posture is the round-6 recorded ruling
this round explicitly overturns (`settings-view.slint:1065-1069` baseline
comment: "adoption is automatic, disconnect is a non-goal"), `BoardInfo`
carries no channel tag (`requests.rs:65-70`: `bound_connector:
Option<String>` only), `PublishBoard` targets Deck only
(`requests.rs:412-416`), and `MoveBoardToFiles` refuses non-Deck-bound
boards (`ui_requests.rs:1520-1528`) ⇒ **R7-3 unruled at planning time.**
The overturn of the round-6 ruling is recorded as this plan's authority
note in §1 (the round-7 plan §2 D-7.3 is the overturning authority; the
census diff that records it is chunk 2's).

### D6 — How the validation assertion is recorded (uniform across the three plans)

1. Each plan's **§1** carries a subsection titled
   `### Validation assertion (pilot §6.1 step 1)` stating, per affected
   matrix row: the baseline-SHA cell state (blank/unruled, or CLI-only),
   the baseline file:line anchors proving it, and the closing sentence
   naming the R7 finding it corresponds to ("R7-<n> is an unruled absence
   the framework, applied at planning time, flags before any
   implementation exists").
2. The **§2 matrix** shows the *target* state only (cells cite story or
   ruling IDs). Baseline state is never folded into the cells — the
   parser contract keeps headers and cells minimal, and the assertion
   block is where baseline lives. Directly beneath the matrix table
   stage 2 adds a short bullet list "Baseline state at `35e6b0a1`"
   (prose bullets, **not** a table — a third table under §2 between the
   matrix and Rulings risks the parser's section-table pairing).
3. Stage 3 verifies the assertion by resolving every cited anchor at
   `35e6b0a1` via `git show` (the acceptance's "blindness held" check).

### D7 — Plan assumptions (every seam name the round-7 design does not fix)

Recorded per plan in §6 as a bullet list titled
`Plan assumptions (seam names the design does not fix; the implementation
chunk binds them)` — the blindness-honest inventory. The gate's MISSING
rows on these are *expected*, and chunk 2's comparison reads them as the
plan-vs-implementation delta map:

- **A-1 (habit plan)**: the habits-section adapter seam / `install_seam!`
  site that receives `create-asked` (existing habit seams exist; their
  exact names not read blind beyond the registry block).
- **A-2 (habit plan)**: the inline form's internal callback names
  (title/target/quota/confirm/cancel) and the pass-through callback names
  through `today-view.slint` / `app-window.slint` (default: propagate
  `create-asked` unchanged; confirm/cancel follow the section's
  `convert-confirmed/cancelled` grammar).
- **A-3 (recur plan)**: task-view mount-affordance callbacks (assumed on
  the existing `recur-pause-asked` grammar, e.g. a `recur-edit-asked`
  shape), the editor composition's commit/cancel/field callbacks, and the
  adapter-side draft-buffer global name (due-editor precedent: buffers +
  facts globals, no transient UI state in Slint).
- **A-4 (recur plan)**: the `ContextId` enum variant spelling for
  `RecurEditor` (the design fixes the context's *value/meaning*; the
  variant name follows the enum's existing convention).
- **A-5 (recur plan)**: the pre-fill mapping when editing an existing rule
  (pattern/bound fields extracted from `task.recur`; the assembly laws are
  design-fixed, the field-by-field mapping is implementation).
- **A-6 (roster plan)**: the per-row action callback name (following the
  card's `*-clicked(index)` grammar), the `ConnectionItem` field additions
  (badge text/tone, action label/enabled, archived flag), the
  `FilesBoardBadge` enum's home file, and the adapter handler /
  `install_seam!` names for the two new `UiRequest`s.
- **All plans**: the free-key-per-preset choices for `HabitCreate` /
  `SeriesEdit` (registry-generated hints pick them up; the totality
  invariant tests stay green per D-7.1/§3).

### D8 — Template conformance and the parse-first ("TDD") protocol for stage 2

For a docs-only chunk, test-first means the parser contract is the
executable spec. Stage 2's order per plan:

1. Copy `.artifacts/templates/ux-journey-plan.md` verbatim (the guidance
   comment stays; only `<...>` placeholders get filled — template law).
   The four parser-contract tables keep their headers token-exact:
   §2 matrix `| Capability | Board | TaskDetail | Today | Review |
   Settings | Capture | CLI |`, Rulings `| ID | Ruling | Reason | Home |`,
   §4 per-story steps `| Step | Surface & affordance | User action |
   Dispatches | System response | Escape / failure |`, §6 wiring
   `| ID | Surface file | Widget | Callback | Adapter seam | ActionId |
   Commands | Row addressing | Journey steps | Test |`, §7 delta
   `| Kind | Name | Payload sketch | Journeys |`; §4 subsections are
   `### US-<n>`; step IDs written `US-n/S-k`.
2. Run the parse **first on the skeleton** (D1's baseline extraction +
   `--root`): zero `PARSE-ERROR` lines, all rows MISSING — the contract
   holds before any content exists. Then fill; re-run after each plan and
   record the output as evidence in the stage-2 report.
3. `Test-root:` front-matter stays the default (the UI crate's
   `crates/taskboard-ui-slint/tests/`) for all three plans: the
   step-ID-named tokens (`us<n>_s<k>`) are journey/presentation tests;
   the roster plan's app-arm tests assert exact commands without
   embedding step tokens (one behavior, one test, lowest layer).
4. Plans are authored **Status: DRAFT** — acceptance is the user merging
   this chunk's PR; chunk 2 compares only after that merge (dispatch §1
   dependency graph). Origin fields cite: round-7 plan §2 D-7.x (the
   design origin), the framework plan §4.1/§6.1, the dispatch plan §2,
   and the baseline SHA.

### D9 — Commit sequencing (standalone commits, round-7 chunk order)

| # | Commit | Contents |
|---|---|---|
| 1 | `docs(plan): the ux-r7 pilot blind-plans chunk plan` | this file (already committed by stage 1) |
| 2 | `docs(plan): the r7 habit-create ux journey plan` | `<today>-r7-habit-create-ux-plan.md` |
| 3 | `docs(plan): the r7 recur-editor ux journey plan` | `<today>-r7-recur-editor-ux-plan.md` |
| 4 | `docs(plan): the r7 files-roster ux journey plan` | `<today>-r7-files-roster-ux-plan.md` |

Each commit is docs-only and standalone (builds trivially; no fixup
commits — AGENTS §8 hygiene for rebase merges). Order mirrors the round-7
chunk order 1→2→3 so review and chunk 2's comparison read them in the
same sequence. `<today>` is stage 2's run date (dispatch's `<today>` is
per-agent); stage 2 records the actual three filenames in its report so
stage 3's "diff is exactly the four plans" check maps cleanly.

### D10 — Open questions resolved

- **Q: May stage 2/3 open the landed `2026-11-13-ui-r7-habit-create-plan.md`
  (in-tree at the worktree's checkout)?** No — it is post-baseline `main`
  content (landed `8ee2d827`); dispatch §1 forbids it. Same for the
  `feat/ui-r7-*` worktrees and PRs, and for `git log`/diff reads past
  `35e6b0a1` beyond identifying the baseline itself (already done and
  recorded here).
- **Q: Plain or `--gate` parse runs?** Plain, informational (dispatch §2
  acceptance: "expect all MISSING — the correct pre-state"). `--gate`
  exits 1 here by design; it is chunk 2's and the implementation chunks'
  instrument.
- **Q: Do the plans claim undo coverage for detach/unbind?** No. Escapes
  record cancel-before-commit / armed-cancel / typed refusals. Where undo
  arms exist in the app they are engine-side facts this chunk does not
  verify blind; claiming them would violate the evidence mandate.
- **Q: Kiosk story sets?** None (framework §8 decision 4). Postures are
  recorded per story and in each §8 kiosk-gates bullet.
- **Q: What if stage 2 discovers a journey step the design does not
  cover** (a hole in D-7.x)? Record it as a ruling or a non-goal with a
  home in that plan — never silently, never by inventing design. Material
  holes become a note for chunk 2's comparison (its §3 verdict machinery
  exists for exactly this).
- **Q: Are the three plans one commit?** No — three standalone commits
  (dispatch §2; D9), so chunk 2 and any later amendment can cite one plan
  by its commit.

## 2. Stage-2 acceptance checklist (the "tests" for this docs chunk)

1. Per plan: skeleton parse clean (no exit 2), then filled-plan parse via
   D1's baseline extraction: zero `PARSE-ERROR`, all new-seam evidence
   rows MISSING (pre-existing-command rows FOUND at baseline, per D1);
   outputs recorded as report evidence.
2. Every journey step has a non-empty escape/failure cell (grep-checkable:
   no empty cell before the next `|` in the escape column).
3. Every matrix `—` on a core verb carries `(R-n)` and every R-n has
   non-empty Reason and Home (the gate checks Reason; stage 2 self-checks
   Home).
4. Every story lists click + keyboard entry points (or the recorded
   Settings-walk parity argument) and a Platforms line with kiosk posture.
5. The recur plan contains the `stateDiagram-v2`; the roster plan contains
   its recorded no-state-machine reason; all plans contain their
   flowchart(s).
6. Validation-assertion block present per plan with baseline anchors;
   every anchor resolves at `35e6b0a1`.
7. Template headers token-exact in all four parser-contract tables per
   plan (a silent mismatch is a stage-3 finding).
8. Full quality-gate suite green via `scripts/run-quality-gates.sh` from
   the project root (docs-only diff; a pre-existing unrelated failure
   means STOP and report per dispatch prompt 1.2).

## 3. Risks

- **Blindness contamination by habit** — the worktree's working tree
  contains post-baseline sources; one casual `Read` of
  `crates/taskboard-ui-slint/ui/...` invalidates the pilot. Mitigation:
  D1's protocol is restated at the top of the chunk plan, and stages 2–3
  re-read dispatch §1 + this plan before touching files.
- **Gate evidence greps against the wrong root** — the default
  `--root` greps the contaminated tree and would return spurious FOUND
  rows for the habit plan (the implementation is in it). Mitigation: D1's
  extraction command is copy-paste; stage 3 re-runs with the same root.
- **Assumed seam names drift** — the plans' wiring rows may not match the
  implementations (A-1…A-6). That is the pilot working as intended: the
  deltas are chunk 2's comparison input, not a defect of this chunk. The
  assumptions lists make every delta legible instead of silent.
- **Parser strictness** — any header drift or a stray third table under
  §2 exits 2 (by design). Mitigation: D8's copy-verbatim + skeleton-parse
  protocol; prose bullets instead of tables outside the contract tables.
- **Ruling homes that need issues** — filed ahead of stage 2 where known:
  #764 (CLI publish-to-files). Existing authorities cover the rest
  (#718/#723/#755/#756, the capture-overlay recorded ruling, round-7 §4).
  Any new out-of-scope discovery in stages 2–3 files its own `backlog:`
  issue under the standing authorization and cites the number.
- **Date drift** — if stage 2 runs on a later date the plan filenames
  shift; D9's mapping note keeps stage 3's four-file check exact.

## 4. Estimated diff size & backlog filings

- Four markdown files under `.artifacts/plans/`, ≈ 250–350 lines each for
  the three UX plans plus this ≈ 300-line chunk plan; no other paths.
- Backlog filings: **#764** filed by stage 1 (CLI `boards publish
  --to-files` asymmetry — cited as ruling R-2's home in the roster plan).
  Stages 2–3 file further issues only on genuine out-of-scope discovery,
  reporting the numbers.
