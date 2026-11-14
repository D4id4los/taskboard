# [Plan] UX Retrofit — Chunk 1: task-flow-core retrofits

- **Date**: 2026-11-14
- **Kind**: stage-1 chunk plan for chunk 1 of the UX retrofit & triage
  phase (framework plan §6 phase 4). Analysis-artefact planning only —
  the chunk produces the feature registry and four retrofit UX journey
  plans describing journeys-as-implemented; no application code.
- **Origin**: the dispatch plan
  `.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  §1/§2 (the retrofit posture and prompts 1.1–1.3); the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.1/§4.4/§6 phase 4; `docs/ux_planning.org`; the completed census
  `docs/ux-surface-census.org` (*full census complete*, 2026-11-14);
  the stock-take report
  `.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`
  (§3 = the 121-entry unruled inventory — chunk 1 works the
  Board/Today/Capture/TaskDetail rows); the three pilot UX plans
  `.artifacts/plans/2026-11-13-r7-{habit-create,recur-editor,files-roster}-ux-plan.md`
  (format exemplars **and** the already-retrofitted features not to
  duplicate); `.artifacts/templates/ux-journey-plan.md`.
- **Status**: READY — precondition verified (census + stock-take report
  on `origin/main` at `8698fc0f`); worktree
  `../taskboard-WT-docs-ux-retrofit-core` created per AGENTS §8
  (doctor exit 0, `git worktree add -b docs/ux-retrofit-core … origin/main`).
- **Binding inputs of stage 2**: this plan's D-1 (registry), D-3
  (rubric), D-5 (steady-state rule), D-6 (commit sequencing). Deviations
  need a recorded reason in the stage-2 report.

## 1. Posture (inherited, restated for the chunk)

Retrofit = **describe what exists**. The four plans read the census
slices (evidence-anchored at the slices' baseline `dc290df2`), the code
at those baselines, and the live `main`; they document journeys
**as implemented**, including escape paths where present. Where the
description exposes a hole, the plan records it as a **finding** in its
findings section; it never fixes, never prescribes redesign — redesign
is phase 5's, dispatched from the accepted triage report. A retrofit
plan may cross into a **ruling** only per the rubric (D-3); the plan
then becomes the ruling's authority, dated, with the census evidence
cited. Every `U` cell a plan does not convert to a ruling or a finding
stays `U`.

## 2. Decisions

### D-1 — The feature registry

Every census capability row (137 rows over the seven surface slices)
belongs to exactly one feature. Features: the four task-flow-core
features (dispatch §2, this chunk), the five auxiliary features
(dispatch §3, chunk 2), and the pilot-retrofitted set (referenced,
never duplicated). `nav.item_walk` appears as a row in three slices
(Board, Today, Task detail) — one capability, one owner (the
capture-search plan, per dispatch §2's explicit assignment).

| # | Feature | Chunk / plan | Census rows (capability names) | Slice rows touched |
|---|---|---|---|---|
| F-1 | Board & stack management | chunk 1, `<today>-board-stack-management-ux-plan.md` | board.create, board.rename, board.delete, board.select, board.color, board.archive, board.reset, board.reconcile, board.default_preset, stack.create, stack.rename, stack.delete, stack.move, stack.resize, stack.count_display | 15 (Board) |
| F-2 | Task lifecycle | chunk 1, `<today>-task-lifecycle-ux-plan.md` | task.create, task.complete, task.open, task.move, task.delete, task.location_display, task.badges_display, task.due_phrase_display, task.due_today, app.undo, app.redo | 8 (Board) + 4 (Task detail: open/complete/move/delete) + 1 (Today: due_today) + 2 (Capture: undo/redo) |
| F-3 | Task editing | chunk 1, `<today>-task-editing-ux-plan.md` | task.back, task.title_edit, task.description_edit, task.due_edit, task.scheduled_edit, task.reminder_rules_edit, task.reminders_mute, task.reminder_snooze, task.track_start, task.track_stop, task.track_entry_log, task.track_entry_adjust, task.track_entry_delete, task.track_total_display, track_entry.roster_display, track.status_display, label.create, label.edit, label.delete, label.roster_display, task.label_assign, task.label_unassign | 22 (Task detail) |
| F-4 | Capture & search | chunk 1, `<today>-capture-search-ux-plan.md` | capture.quick, capture.commit, capture.dismiss, capture.hint_display, palette.search, palette.jump, palette.results_display, nav.item_walk, app.menu, app.focus_toggle | 7 (Capture) + 3 (nav.item_walk rows: Board/Today/Task detail) + 2 (Capture: menu/focus) |
| F-5 | Habit family | chunk 2 | habit.create *(pilot — referenced)*, habit.edit, habit.remove *(ruled absences R-2/R-3 — referenced, not re-journeyed)*, habit.log, habit.reorder, habit.repair, habit.convert_to_series, habit.state_display | 8 (Today) |
| F-6 | Recurrence family | chunk 2 | task.recur_set, task.recur_clear, task.recur_preview *(pilot recur-editor — referenced)*, task.recur_pause, task.recur_convert_to_habit, task.recur_display | 3 (Task detail) + pilot rows |
| F-7 | Timers & alarms | chunk 2 | alarm.create_adhoc, alarm.cancel_adhoc, alarm.display (Today), alarm.dismiss, alarm.snooze, alarm.cancel, alarm.window_display, notification.dismiss, notification.done, notification.snooze, notification.mute, notification.cancel, notification.track_start, notification.open, break_rule.create, break_rule.remove, break_rule.roster_display | 4 (Today) + 11 (Alarm window) + 3 (Today break_rule) |
| F-8 | Review walk & reflection | chunk 2 | review.open, review.walk_cursor, review.walk_back, review.walk_advance, review.inbox_file, review.inbox_keep, review.file_target, review.file_commit, review.carry_tomorrow, review.carry_pick, review.carry_leave, reflection.record, review.finish, plan.open, plan.begin_place, plan.commit_place, plan.box_earlier, plan.box_later, plan.box_remove, review.ledger_display, review.stats_display, reflection.ledger_display, reflection.edit, reflection.delete, review.walk_preview | 24 (Review) + 1 (Today walk_preview) |
| F-9 | Settings & connections | chunk 2 | connection.save, connection.remove, board.remote_list, board.attach, board.create_local, board.detach, board.publish, files.sync_enabled_set, settings.knob_set, app.settings, connection.roster_display, settings.facts_display, settings.env_override_display, board.roster_display *(pilot files-roster — referenced)*, board.unbind_files *(pilot)*, board.move_to_files *(pilot)*, board.publish_to_files *(pilot)* | 13 (Settings) + 4 pilot rows (Settings) |
| — | Already retrofitted (pilot plans) | referenced only | habit.create → `2026-11-13-r7-habit-create-ux-plan.md`; task.recur_set, task.recur_clear, task.recur_preview → `2026-11-13-r7-recur-editor-ux-plan.md`; board.roster_display, board.unbind_files, board.move_to_files, board.publish_to_files → `2026-11-13-r7-files-roster-ux-plan.md` | 8 rows |
| — | CLI-only entity-less register | no plan | gui, status, daemon stop, config print, config path, export, find, sync, boards use (dead, #776) | census CLI-only register, not matrix rows |

Arithmetic check: F-1 15 + F-2 15 + F-3 22 + F-4 12 + F-5 7 (habit
rows minus the pilot create) + F-6 3 + F-7 17 + F-8 25 + F-9 13 = 129
owned rows + 8 pilot-owned rows = **137 = the census row total** (24 +
17 + 11 + 33 + 17 + 24 + 11). Zero orphans (D-2 resolves the four
registration ambiguities the dispatch's feature lists left open).

### D-2 — Registry findings and their resolution

Registry derivation surfaced four gaps in the dispatch §2/§3 feature
lists; each is resolved here as a recorded decision (deviations from
the origins' letter, with reasons):

- **D-2.1 — the `app.*` global rows are unowned by the dispatch's
  feature lists** (app.menu, app.undo, app.redo, app.focus_toggle; the
  Capture slice records them because no other slice's view mounts
  them). Resolution: `app.undo`/`app.redo` → F-2 Task lifecycle (undo
  is the universal escape path of complete/delete/move — the lifecycle
  plan's escape-path audit needs their seams anyway);
  `app.menu`/`app.focus_toggle` → F-4 Capture & search (the Capture
  slice's app-level-overlay family; the menu also launches
  settings/review, which the plan records cross-referentially, not as
  journeys). `app.settings` keeps its Settings slice owner (F-9).
- **D-2.2 — `task.due_today` is unowned** (a Today-only task verb,
  `actions.rs:1076-1078`). Resolution: F-2 Task lifecycle — it is a
  task verb; the dispatch's lifecycle list ("location/badge/due
  display — Board + Today rows") covers the display twin, so the edit
  verb rides the same plan.
- **D-2.3 — the `break_rule.*` rows are unowned** (CLI-only, Today
  slice, "timer/alarm-adjacent daily surface" per the census).
  Resolution: F-7 Timers & alarms (chunk 2) — same rationale as the
  census's placement note.
- **D-2.4 — "priority" is named in dispatch §2's task-editing list but
  no priority capability exists in the census** — the phase-3 verb
  inventory missed the importance/urgency family: engine-editable
  (`UpdateTask` fields `messages.rs:655-668`), CLI-editable
  (`--importance`/`--urgency` grammar `cli.rs:592-637`), displayed as
  card decorations (`models.rs:2394-2416`), no GUI edit seam (adapter
  grep empty), and no census row. This is a census-completeness hole,
  **not** fixable inside this chunk (the diff is exactly the chunk's
  artefacts; the census amendment is a dedicated pass). Filed as
  backlog **#781**; F-3's plan records the family in its findings
  section (display-without-control, census hole) without minting
  census rows. The 137-row arithmetic in D-1 is checked against the
  census as it stands; #781's future amendment rides its own pass.

Orphan resolution: none — after D-2.1–D-2.3 every capability row has
exactly one owner (the arithmetic above is the proof). The registry is
a section of this plan (dispatch §2 item 1) and is referenced by
chunk 2.

### D-3 — The scoping-ruling rubric (converting `U` markers)

Dispatch §1 rule 1 is the floor: a ruling needs a scoping decision the
surface grammar or a prior artefact already justifies; everything else
is a finding. Made concrete — a `U` cell converts to a **ruling**
 only when the census's own evidence matches one of these four
patterns (the plan cites the pattern + evidence and becomes the
ruling's authority, dated):

- **R-Pattern 1 — context-scoped binding**: the verb's `ActionId`
  registry binding names the surface(s) exclusively (`ContextId::X`
  members, "binds only ContextId::X" sweep evidence). The absence
  elsewhere is the binding's semantics, not a gap. (Precedents: the
  walk/planner verbs' `ReviewWalk`/`PlanNextDay` scopes; recur-editor
  ruling R-2's single-surface editor posture.)
- **R-Pattern 2 — entity-mount grammar**: the affordance mounts only
  where the entity renders, and the sweep shows no other surface
  renders that entity at all (habit rows mount only where habit rows
  mount; the track-entries overlay only where the detail view mounts
  it). The absence follows from "no mount outside the section"
  evidence.
- **R-Pattern 3 — display-scoping**: a pure-display capability
  (`D`-verb) absent on surfaces whose data it would echo is a
  presentation-scoping fact, not a product gap — displays have no
  escape to lose and no control to demand (that judgment, if the
  display is a *core* one the user needs elsewhere, is the walkthrough's
  business and lands as a finding instead).
- **R-Pattern 4 — standing authority**: an existing census ruling or
  accepted plan already covers the cell (pilot R-1/R-2/R-3, the
  recorded non-goals); the retrofit plan cites it and transfers
  authority — it never re-argues it.

A `U` cell **demands a finding** (never dressed as a ruling) when the
evidence matches any of these:

- **F-Pattern 1 — CLI-only mutation of user data**: the only entry
  point is the CLI and the verb mutates user data (labels,
  break rules, board.color/archive/reset/reconcile, reflection
  edit/delete, task reminder_snooze's CLI absence). Gap classes G2/G3;
  classification happens in the triage report, not here.
- **F-Pattern 2 — display-without-control**: a capability renders
  (badge/echo/decoration) with no seam to act on it anywhere in the
  GUI.
- **F-Pattern 3 — journey integrity**: a journey cannot complete, a
  step has no escape path where user data is at risk, a control
  exists without feedback, or a practical lockout exists (archived
  boards excluded from the board bar — unreachable in the GUI).
- **F-Pattern 4 — convenience**: the cell's absence would merely be
  more convenient to fill — "it would be handy on this surface too"
  is the walkthrough's territory, filed as a finding, never a ruling.

Conversion law: exactly one of {ruling (pattern cited), finding
(findings section), `U` stays `U`} per cell; a cell whose evidence
matches a ruling pattern is converted; a cell matching a finding
pattern is recorded in the plan's findings section **and stays `U`**
in the census (the triage report owns classification); every
conversion cites the plan (dated). Conversions never touch seed/phase-3
rulings (R-*/PC-* rows are immutable input).

### D-4 — Per-plan story sets, census feeds, escape-path audit, walkthrough

Each retrofit plan is template-conformant (`ux-journey-plan.md`): its
§2 matrix re-states the census cells of its D-1 rows (populated cells
verbatim with their baseline anchors; `U` cells carried as `U` or
converted per D-3); stories are **journeys-as-implemented** with
**P1 = the journey a user actually must complete today** (the path
that exists and works end-to-end — not what should exist; a P1 story
whose step is escape-less still ships as P1 with the hole as a
finding). Wiring inventory cites **existing** seams only
(callbacks, `ActionId`s, command variants, `ItemAddr` kinds, existing
tests); command-contract delta is **empty by definition** (retrofit:
the contract already exists — the section states "none — retrofit" and
lists the existing variants the journeys dispatch for greppability).

| Plan | Story set (P1 unless noted) | Census feed |
|---|---|---|
| board-stack-management | US-1 create board (Ctrl+Shift+N/field commit); US-2 select/switch board (segment click, BoardNext/Prev/Select); US-3 create stack (+stack, S); US-4 rename stack (F2) & board (Ctrl+F2); US-5 delete stack & board (context menu, confirm gates); US-6 move stack; US-7 resize stack (divider drag, StackWider/Narrower); US-8 default preset CTA (empty board); US-9 count display (D). US-10 (P2, CLI-only verbs): color/archive/reset/reconcile — journeys the CLI completes; the GUI absence is findings per F-Pattern 1/3 (archive lockout). | Board slice rows (15) |
| task-lifecycle | US-1 create task (per-column ghost/inline row, N); US-2 complete/reopen (card checkbox, Today row toggle, detail toggle; CLI --undo); US-3 open detail (card click/Enter, today row activation); US-4 move task (card drag → MoveTask, move flow, TaskMove); US-5 delete task (context menu/Delete; escape audit: AppUndo path); US-6 due-today re-commit (TaskDueToday); US-7 location/badge/due-phrase displays (D); US-8 undo/redo as the global escape (AppUndo/AppRedo). | Board 8 rows + Task detail 4 + Today due_today + Capture undo/redo rows |
| task-editing | US-1 title edit (F2, detail seams); US-2 description editor; US-3 due editor (date picker, PickerCommit); US-4 scheduled editor; US-5 back navigation (task.back); US-6 reminder rules (add/remove rows); US-7 mute/unmute; US-8 snooze (banner); US-9 tracker start/stop (chip + status bar); US-10 entry log/adjust/delete; US-11 tracked totals (D) + track rosters (D); US-12 (P2, CLI-only): labels — the family's GUI absence is a finding per F-Pattern 1/2 (labels render nowhere; importance/urgency rides decorations — #781). | Task detail slice rows (22 minus pilot recur rows) |
| capture-search | US-1 quick capture (Ctrl+K global → overlay → CaptureTask → inbox/backlog); US-2 dismiss (Escape); US-3 search palette (Ctrl+F global, query re-rank); US-4 jump to task (NavShowTask row activation); US-5 item walk (NavNext/Prev/First/Last per VIEW_NAV contexts); US-6 app menu (two-tier, AppMenu); US-7 focus toggle (FocusModeToggle overlay). CLI twins: `capture` (commits-and-exits — F-Pattern evidence note), `find` (same engine ranking, no jump — F-Pattern 4 finding). | Capture slice rows (7 + menu/focus) + nav.item_walk rows ×3 |

**Escape-path audit method** (per journey step, executed at
implementation time): for each step name the escape mechanism that
exists — (a) **global undo**: `AppUndo`/`AppRedo` (`actions.rs:678-690`,
`Undo`/`Redo`/`RestoreTask` `messages.rs:589-601`) for state mutations;
(b) **overlay escape**: Escape presets on the surface's `ContextId`
(`presets.rs`), overlay-local cancel seams (capture dismiss
`adapter.rs:8524-8530`, recur-editor arm-strip precedent, review walk
back-then-stage-then-close `presets.rs:312`); (c) **typed error /
no-op guard**: dispatches that refuse (unparseable date dispatches
nothing `adapter.rs:2252-2299`, stale-index no-ops `adapter.rs:9410`,
SettingsError oneshots); (d) **confirm gates**: destructive CLI flags
(`--confirm`) and armed strips. Each step's escape column cites the
seam `file:line` or the test name; a step with none of (a)–(d) records
`escape-less` and becomes a finding (F-Pattern 3). Audit shortcuts are
forbidden: every P1 step gets its own column entry even when it shares
the mechanism with its sibling.

**Walkthrough questions at implementation time** (framework §4.4's
four, applied descriptively — each answered per P1 step with
file:line/test evidence, findings descriptive only):

1. *Will the user see the control?* — the plan's entry points vs the
   surface's actual affordance (mounted where? greyed on kiosk?
   reachable-not-promoted?).
2. *Will they recognize what it does?* — label/hint semantics judged
   (consequence-style doctrine, PC-8 precedent), never text-asserted.
3. *Will they understand the feedback?* — badge/tone/busy-dot/caption
   vs the system-response column; silent-success steps are findings.
4. *Can they recover?* — the escape column holds per the audit above.

Plus the six-heuristic pass (Nielsen subset) over the plan's touched
surfaces, findings only.

### D-5 — Gate steady-state rule (`scripts/ux-coverage.sh`)

Every retrofit plan runs `scripts/ux-coverage.sh <plan>` (plain,
informational) at stage 2 and stage 3. Because the plans document
existing wiring, the steady state is **FOUND for every wiring row and
journey step**. A MISSING row means the plan misdescribes reality
(wrong callback name, unmounted step, stale anchor):

- **Plan defect** (the common case): fix the plan; record the
  correction in the plan's *Plan corrections* section (`PC-n` IDs, the
  pilot precedent) inside the same commit. The correction note names
  what was wrong and the evidence for the right anchor.
- **Real unmounted step** (the plan is right and the code genuinely
  lacks the seam): that is a **finding** — the step's wiring row
  cannot exist, so the row is removed from the inventory, the step is
  marked escape-less/absent in the journey spec, and the finding
  section records it. Never manufacture an anchor.

The per-plan gate outcome (FOUND/MISSING counts, corrections applied)
is recorded as evidence in the stage-2 report. `--gate` is never CI
(house law).

### D-6 — Commit sequencing

One plan per commit, standalone, in this order (census slice updates
ride their plan's commit — the conversion's authority file and the
census cells it lands land together):

1. `docs(plan): the ux retrofit chunk-1 plan` — this file.
2. `docs(plan): the board-stack-management retrofit ux plan` (+ Board
   slice conversions).
3. `docs(plan): the task-lifecycle retrofit ux plan` (+ Board/Today/
   Capture slice conversions it owns).
4. `docs(plan): the task-editing retrofit ux plan` (+ Task detail
   slice conversions).
5. `docs(plan): the capture-search retrofit ux plan` (+ Capture slice
   conversions).

Each commit builds standalone (docs-only; the §10 gate suite runs
once at the end of stage 2 via `scripts/run-quality-gates.sh` — wait
for the flock). No push, no PR from stage 2; shipping is stage 3's
(dispatch §1 mechanics). The census diff is confined to the four
chunk-1 slices' `U` cells and their registers (conversion = cell
becomes `— (R-n)`-style ruling citation or populated evidence; the
register row notes the conversion); no seed/phase-3 ruling is altered.

### D-7 — Minor decisions

- **The dispatch plan file is untracked** on the user's checkout
  (absent from `origin/main`'s tree). Stages 2/3 read it from the main
  checkout's absolute path
  (`/home/viktoria/src/taskboard/.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`).
  Committing it is not this chunk's artefact set; reported to the user
  for their own commit.
- **CLI columns of retrofitted rows**: the plans' matrices restate the
  census CLI cells verbatim (pre-existing verbs); the CLI-only register
  entries (gui/status/daemon stop/config/export/sync/boards-use) are
  not features and get no retrofit plan — their user-facing story is
  the owning capability's CLI cell (e.g. `find` under palette.search).
- **Format exemplars**: the pilot plans' structure, ruling-table and
  PC-n correction conventions are the format bar; the blindness
  baseline discipline does not apply (this is a retrofit — the code is
  the object of study).

## 3. Stage-2 execution outline

Per dispatch prompt 1.2: recreate/enter the worktree; read the binding
inputs; write the four plans in commit order (D-6) with journeys,
escape audits, walkthrough passes, findings sections; run the gate per
plan (D-5); convert census cells per D-3 riding each plan's commit;
run the full gate suite via `scripts/run-quality-gates.sh` from the
project root; backlog issues for out-of-scope findings; report
per-gate results, findings headlines, conversions, deviations.

## 4. Acceptance checklist (stage-3 verifies)

- [ ] Registry complete: every Board/Today/Capture/TaskDetail
      capability row in exactly one plan; no orphans; pilots
      referenced, not duplicated (D-1 arithmetic holds).
- [ ] Four retrofit plans template-conformant; P1 stories = journeys
      as implemented; wiring rows FOUND under the gate (or corrections
      recorded per D-5); escape audits cite seam/test per step.
- [ ] Retrofit posture held: no fixes, no redesign prescriptions;
      findings descriptive with evidence.
- [ ] Census conversions match D-3 (no convenience rulings; findings
      stay `U`); no prior ruling altered.
- [ ] Diff is exactly: this plan + four retrofit plans + census
      conversions.
- [ ] Gates green via the wrapper; PR open + green + `CLEAN` (stage 3).

## 5. Risks

- **Anchor drift**: chunk-1 census cells anchor at `dc290df2`; live
  `main` has moved (`8698fc0f`, incl. the task-view max-width fix).
  Plans cite anchors as the census records them (baseline-sha law,
  stock-take §7); any anchor resolved stale at live `main` is re-verified
  against `git show dc290df2:<file>` before being called a defect.
- **#781 ripple**: if the user schedules the importance/urgency census
  amendment before stage 2 runs, F-3's feed grows by the new rows and
  the D-1 arithmetic shifts — stage 2 re-checks the census head before
  writing and records the delta if it lands mid-chunk.
- **Ruling/finding misclassification** is the chunk's named trap
  (dispatch 1.3): the rubric's four patterns are the tie-breaker;
  a borderline cell stays `U` + finding rather than converting.
- **The untracked dispatch plan** (D-7): if the user's checkout
  changes, stages read the file at the recorded absolute path; the
  plan content is quoted where binding (§1 posture, §2 features) so
  this plan survives its absence.

## 6. Backlog issues opened at stage 1

- **#781** — the census missed the importance/urgency capability
  family (D-2.4).
