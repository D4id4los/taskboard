# [Plan] UX Retrofit — Chunk 2: auxiliary retrofits + the triage report

- **Date**: 2026-11-14
- **Kind**: stage-1 chunk plan for chunk 2 of the UX retrofit & triage
  phase (framework plan §6 phase 4). Analysis-artefact planning only —
  the chunk produces five retrofit UX journey plans for the auxiliary
  features and the consolidated triage report; no application code.
- **Origin**: the dispatch plan
  `/home/viktoria/src/taskboard/.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  §1/§3 (the retrofit posture and prompts 2.1–2.3; see D-8 for the
  untracked-file handling); the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.4/§6 phase 4; `docs/ux_planning.org` (§2.4 walkthrough protocol +
  census law); **chunk 1's plan**
  `.artifacts/plans/2026-11-14-ux-retrofit-core-chunk-plan.md` (D-1
  registry and D-3 rubric are binding on this chunk — applied
  mechanically, never re-argued); the four merged retrofit plans
  (format exemplars; their findings sections BF-1..BF-9/W-N*, TF-1..4,
  TE-1..5, CF-1..3 are consumed by the triage); the three pilot UX
  plans (`.artifacts/plans/2026-11-13-r7-{habit-create,recur-editor,files-roster}-ux-plan.md`);
  the completed census `docs/ux-surface-census.org`; the stock-take
  report `.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`
  §3 (the 121-entry unruled-observation inventory — chunk 2 owns the
  Settings/Review/AlarmWindow rows plus every row chunk 1 did not
  convert); `.artifacts/templates/ux-journey-plan.md`.
- **Status**: READY — precondition verified (chunk 1 merged:
  PR #783, `origin/main` at `694c35aa`); worktree
  `../taskboard-WT-docs-ux-retrofit-triage` created per AGENTS §8
  (doctor exit 0, 163G free; `git worktree add -b
  docs/ux-retrofit-triage … origin/main` off post-chunk-1 main — never
  stacked).
- **Binding inputs of stage 2**: this plan's D-1 (story sets + census
  feeds), D-2 (rubric application), D-3 (triage rubric floor), D-4
  (consolidation method), D-5 (report outline), D-6 (commit
  sequencing), D-7 (gate steady state, inherited). Deviations need a
  recorded reason in the stage-2 report.

## 1. Posture (inherited, restated for the chunk)

Retrofit = **describe what exists** (dispatch §1): the five plans
document journeys **as implemented** with escape paths where present;
holes become findings, never fixes, never redesign prescriptions —
remediation is phase 5, dispatched from the *accepted* triage report
(the human gate sits after chunk 2's PR merges and the user has read
the report). A retrofit plan converts a `U` cell to a ruling only per
chunk-1 D-3's patterns; the plan is the ruling's authority, dated.
Chunk 1's census conversions (31 cells, R-n rulings citing its four
plans) are immutable input — never re-ruled, never weakened.

## 2. Decisions

### D-1 — The five auxiliary retrofit plans: story sets and census-row feeds

Chunk 1's registry (its D-1 table) is applied mechanically: F-5..F-9
are chunk 2's, with the pilot-owned rows referenced, never
re-journeyed. Census feeds restate each plan's §2 matrix rows. Stories
are **journeys-as-implemented** (P1 = the journey a user actually
completes today); CLI-only capability families ride as P2 stories
documenting the CLI journeys (chunk-1 D-4's US-10/US-12 pattern).

| Plan (file stem `<today>-<stem>-ux-plan.md`) | Story set (P1 unless noted) | Census feed (from chunk-1 D-1) | Pilot references |
|---|---|---|---|
| habit-family | US-1 log a habit (Today row toggle → `LogHabit`; quota/streak display post-log); US-2 edit habit (detail/edit seam → `UpdateHabit`); US-3 reorder (up/down seams → `ReorderHabit`); US-4 repair (repair seam → `RepairHabit`); US-5 convert to series (`ConvertHabitToSeries`-family); US-6 state display (streak/quota row display, D); US-7 (P2, CLI journeys where CLI cells exist). habit.create is the pilot plan's — its §2 row cites `2026-11-13-r7-habit-create-ux-plan.md`, journeys not re-walked; habit.remove cites the census's R-2/R-3 absences verbatim. | 8 Today habit rows: habit.create *(pilot)*, habit.edit, habit.remove *(R-2/R-3)*, habit.log, habit.reorder, habit.repair, habit.convert_to_series, habit.state_display | habit-create pilot |
| recurrence-family | US-1 pause a series (detail recur row → pause verb); US-2 resume; US-3 convert series → habit; US-4 completion-spawn behavior **as displayed** (what the user sees when a recurring task completes — echo/badge, D rows); US-5 (P2, CLI: `tasks repeat` family as-implemented). set/clear/preview are the pilot recur-editor plan's — cited, not re-walked. | 3 Task detail rows: task.recur_pause, task.recur_convert_to_habit, task.recur_display + the pilot's task.recur_set/task.recur_clear/task.recur_preview rows referenced | recur-editor pilot |
| timers-alarms | US-1 create ad-hoc timer/alarm (Today section → create seam); US-2 cancel ad-hoc; US-3 timers display (Today section + status bar, D); US-4 alarm window as-displayed (the second window's rows, D + the always-on-top overlay shape; census D-P11 anchor-aware shape — no Done/Mute — restated as evidence, its classification is the report's); US-5 dismiss occurrence (window button); US-6 snooze occurrence (window); US-7 cancel/silence notification family (`notification.*`); US-8 notification open/jump (if a seam exists — the census's CLI absence is recorded either way); US-9 break rules (P2, CLI-only — the CLI journeys; the GUI absence is findings per F-Pattern 1). | 17 rows: alarm.create_adhoc, alarm.cancel_adhoc, alarm.display, break_rule.{create,remove,roster_display} (3, CLI-only), alarm window's 11: alarm.dismiss, alarm.snooze, alarm.cancel, alarm.window_display, notification.{dismiss,done,snooze,mute,cancel,track_start,open} | — |
| review-walk-reflection | US-1 open review (menu-reachable entry → review view); US-2 the walk overlay (enter → `ContextId::ReviewWalk`; cursor/advance/back verbs; stage-then-close Escape preset); US-3 inbox triage (file/keep; file target pick + commit → `MoveTask` funnel); US-4 carry (carry_tomorrow → `UpdateTask` scheduled commit; carry_pick; carry_leave); US-5 reflection record (prompts editor → `RecordReflection`); US-6 review finish; US-7 planner (`ViewPlanNextDay` global menu entry → `ContextId::PlanNextDay`; begin_place/commit_place/box_earlier/box_later/box_remove); US-8 ledgers & stats (review ledger, stats, reflection ledger, walk preview — D rows); US-9 (P2, CLI: the `journal` family as-implemented). reflection.edit/delete are UI-unreachable — findings per F-Pattern 1, CLI journeys only if a CLI verb exists (the census sweep says none — the finding stands regardless). | 25 rows: review.{open,walk_cursor,walk_back,walk_advance,inbox_file,inbox_keep,file_target,file_commit,carry_tomorrow,carry_pick,carry_leave,finish,ledger_display,stats_display,walk_preview}, reflection.{record,edit,delete,ledger_display}, plan.{open,begin_place,commit_place,box_earlier,box_later,box_remove} + the Today walk_preview row | — |
| settings-connections | US-1 save a connection (form → `UiRequest` lane); US-2 remove a connection (roster row action); US-3 remote board list/attach (`board.remote_list`, `board.attach`); US-4 create local board (`board.create_local` — no `ActionId`, the census records it); US-5 detach / publish (`board.detach`, `board.publish`); US-6 files-sync enable toggle (`files.sync_enabled_set`); US-7 knob edits (`settings.knob_set` — the grouped 18-variant row, every variant named); US-8 facts & env-override display (D); US-9 connection/board rosters (D; board.roster_display cites the files-roster pilot). **The user's named worst offender**: the walkthrough four questions + six heuristics run with extra care — describe the actual multi-edition friction (G4's real instance lives here), cite seams, no redesign. The pilot files-roster rows (`board.unbind_files`, `board.move_to_files`, `board.publish_to_files`) are referenced, not re-walked. | 13 Settings rows + 4 pilot rows referenced | files-roster pilot |

Feed arithmetic is chunk-1 D-1's (F-5 7 + F-6 3 + F-7 17 + F-8 25 +
F-9 13 = 65 owned rows; +8 pilot-owned rows +64 chunk-1-owned rows =
137 census rows) — stage 2 re-checks it against the census head
before writing and records any drift (see §5 risks).

### D-2 — Census `U`-cell handling in chunk 2 (chunk-1 rubric applied mechanically)

Chunk-1 D-3's R-Patterns 1–4 and F-Patterns 1–4 govern every chunk-2
conversion; no new patterns are minted. Expected distribution per the
census evidence (stage 2 verifies per cell; this is a forecast, not a
ruling):

- **R-Pattern 1 (context-scoped binding) dominates the Settings and
  Review slices' GUI-absence cells**: settings verbs mount only in the
  settings view; walk/planner verbs bind only
  `ContextId::ReviewWalk`/`PlanNextDay` (the census cites the
  `actions.rs` ranges). A cell converting under R-Pattern 1 becomes a
  ruling citing this chunk's plan, dated.
- **R-Pattern 2 (entity-mount grammar)** covers the habit rows (mount
  only in the Today section) and the track/alarm overlays.
- **R-Pattern 3 (display-scoping)** covers pure-display absences
  (rosters, facts panels, badge echoes) **unless** the display is a
  core one the user needs elsewhere — then F-Pattern 4 finding.
- **R-Pattern 4 (standing authority)** covers cells already ruled
  (R-2/R-3 habit.remove; the seed rulings; chunk-1's conversions).
- **F-Pattern 1 (CLI-only mutation of user data)** cells **stay `U`**
  and land in the plan's findings section: break_rule.* (UI-unreachable
  engine verbs), reflection.edit/delete, the AlarmWindow CLI-absence
  cells (dismiss/snooze/open twins), settings.knob_set /
  files.sync_enabled_set CLI cells.
- A cell matching no pattern stays `U` (register entry unchanged;
  the triage report owns its classification).

Conversion law (chunk-1 D-3 verbatim): exactly one of {ruling,
finding, `U` stays `U`} per cell; conversions cite the plan (dated);
seed/phase-3 rulings and chunk-1's conversions are immutable. Census
diffs are confined to the five slices' `U` cells and their registers.

### D-3 — The triage rubric (classification floor — extend, never weaken)

Dispatch §3's floor, made operational. Every classification carries:
**evidence** (census register row, plan finding id, `file:line`/test
anchors), **the affected journeys named by ID** (`US-n`, any plan),
and **a home** (backlog issue, AGENTS §7 conventions). The report
tables are the machine-consumable form (D-5).

- **BLOCKER** — any of:
  - B-a: a journey cannot complete end-to-end (the plan's journey
    spec terminates in an unmountable/unreachable step — includes
    **practical lockouts**: a user action strands an entity where no
    GUI path restores it, e.g. the archive lockout BF-2's unarchive
    half);
  - B-b: a step has no escape path where user data is at risk
    (escape-less destructive steps — BF-5/BF-6's posture; data-at-risk
    means the mutation is `NotUndoable` *and* unconfirmed);
  - B-c: display-without-control on a **core verb** (create/edit/remove
    of a primary entity: boards, stacks, tasks, habits, connections —
    TE-4's label family and TE-5's importance/urgency match here);
- **MAJOR** — any of:
  - M-a: a heuristic violation with real friction (no undo where the
    sibling verbs have it — BF-6-vs-task-delete contrast; feedback
    absent after dispatch — silent absorbs W-N3; entry-point asymmetry
    click-vs-key — BF-9, TF-1);
  - M-b: a core verb reachable only via CLI **without a recorded
    justification** (F-Pattern 1 cells whose census evidence carries
    no scoping authority);
  - M-c: kiosk/desktop control asymmetry where the display half ships
    and the control half is silently gated (W-N3/W-N4 posture) on a
    core verb;
- **MINOR** — polish: label semantics, ordering, discoverability,
  recognition-over-recall observations (W-N1 dual-purpose tone,
  BF-8's vacuous CLI absences).

**Decision rules** (extensions, recorded):

- **Strongest-match rule**: a finding matching several patterns takes
  the highest severity whose pattern it matches; the reviewer's doubt
  is recorded inline in the evidence cell (never silently upgraded or
  downgraded). Never inflate: "it would be convenient" is not MAJOR.
- **Never weaken**: a finding the rubric's floor classifies BLOCKER
  cannot ship as MINOR for scope comfort; if remediation would be
  outsized, the P-homing still says P1 and phase 5 schedules it.
- **CLI-absence cells**: the 121-entry inventory's CLI cells classify
  against M-b only when the verb mutates user data *and* no recorded
  authority justifies the absence; read-only/echo CLI twins (BF-8,
  CF-3, TF-4) are MINOR (polish of the command surface) unless a
  journey depends on them (then M-a's friction arm).
- **Display rows (D-verbs)**: absence of an *edit* twin on a display
  classifies per B-c/M-b by the verb's coreness; absence of the
  display itself elsewhere is F-Pattern 4/MINOR territory unless a
  named journey needs it (M-a).
- **Walkthrough W-N notes** from all plans are first-class triage
  inputs — each is classified, not just the numbered BF/TF/TE/CF/…
  findings.

### D-4 — The consolidation method (everything enumerated exactly once)

**Sources** (complete set):

- **S1** — the six retrofit plans' findings sections: BF-1..BF-9,
  W-N1/W-N2 (board-stack), TF-1..TF-4, W-N1..W-N3 (lifecycle),
  TE-1..TE-5, W-N1..W-N3 (editing), CF-1..CF-3, W-N1..W-N4
  (capture-search), plus chunk-2's five plans' findings sections
  (minted at stage 2; per-plan ID prefixes: **HB-** habit family,
  **RC-** recurrence, **TA-** timers/alarms, **RW-** review walk,
  **SC-** settings/connections; W-N walkthrough notes per plan).
- **S2** — the three pilot plans' open items: their findings R7-1/R7-2
  (GUI-absence findings remediated by the landed round-7 chunks —
  recorded as *resolved, no report row*), R7-3 (same), the D-T16
  kiosk-gating posture (open; dedup-merges with the lifecycle W-N3 /
  capture W-N4 silent-gating findings), and each pilot's PC-* bind
  records (plan mechanics, not findings — no rows).
- **S3** — every still-unconverted entry of the stock-take §3
  inventory: the 121 register entries **minus** those chunk 1 retired
  by conversion (31 `U` cells → rulings; stage 2 derives the exact
  retired set by diffing the census `U` registers on `origin/main`
  against the stock-take's §3 table — the census register rows are
  the authority, not the report's snapshot).

**Method**:

1. Build the **master ledger**: one row per *item*, keyed
   `(surface, capability, evidence-class)` — the dispatch's dedup
   keys. `surface` = the census slice(s); `capability` = the
   `entity.verb` row(s); `evidence-class` = the gap class
   (CLI-only-mutation / display-without-control / escape-less /
   no-feedback / asymmetry / kiosk-gating / polish).
2. **Dedup**: items sharing the key merge into one **finding** with
   all citations listed (plan ids + register rows). The house pattern:
   a CLI-only *family* is one finding covering its rows (TE-4's six
   label rows are one finding) — the per-row surface lists are
   enumerated inside the finding's evidence cell, so every register
   entry maps to exactly one finding id.
3. **Reconciliation appendix**: a mapping table — every S3 entry
   (by inventory row) → finding id, or → "retired by chunk-1
   conversion (R-n)", or → "resolved (pilot remediation)". This is
   the exactly-once proof: sum of mappings = 121 + S1/S2-only items.
   Chunk-1 plan findings not tied to `U` cells (e.g. BF-8, W-N notes)
   appear with a "no register entry" marker rather than being forced
   onto one.
4. **Cross-checks** (stage 2 runs, stage 3 re-runs): (a) every
   findings-section bullet of the six plans appears in the ledger at
   least once; (b) every still-unconverted register entry maps to
   exactly one finding id; (c) no finding id appears twice across
   severity tables; (d) the Settings-slice row count is stated so the
   domination claim (D-5) is verifiable.

### D-5 — The triage report outline (machine-consumable by phase 5)

File: `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md`.
Sections, in order:

1. **Header** — date, kind, origin citations (framework §6 phase 4;
   dispatch §3; the six plans + three pilots + census + stock-take
   §3), posture (findings + evidence + homes only — no remediation
   designs).
2. **Headline** — BLOCKER/MAJOR/MINOR counts, issues-filed count,
   **the Settings-domination statement**: whether the Settings
   surfaces dominate the findings table as the user named them the
   worst offender — the report says so if they do and says so if they
   do not (with the per-slice counts to back it).
3. **Severity tables** — one table per severity, in order
   BLOCKER → MAJOR → MINOR. Columns (fixed, machine-consumable):
   `finding id | surfaces | capabilities | journeys affected |
   evidence | home issue`. One row per finding; `journeys affected`
   cites `plan-stem US-n` IDs; `evidence` cites the deduped anchors
   (`file:line`/test names) and the originating plan finding ids;
   `home issue` is the `#NNN` of the finding's backlog issue.
4. **Journeys that cannot complete end-to-end** — the B-a sublist
   named explicitly with their terminal steps (the dispatch requires
   these called out).
5. **Reconciliation appendix** — D-4's mapping table (the exactly-once
   proof) + the retired/resolved sets.
6. **Next step** — the **human gate**: phase 5 (remediation dispatch)
   is written only after the user reads and accepts this report (or
   edits it); the P1/P2/P3 homing is phase 5's dispatch input.

Issue homing per finding: one `backlog:` issue per finding (not per
cell), labels `ui` + `p1`/`p2`/`p3` (P1=BLOCKER, P2=MAJOR, P3=MINOR —
D-3's homing rule) + a category label where one applies
(`architecture`, `cli`, …); body = one-paragraph summary + a permalink
to the finding's report section (`…/reports/2026-11-14-ux-retrofit-triage-report.md#…`)
— never the whole table (dispatch §3's binding wording). Issues are
filed at stage 2 while the report is assembled; stage 3 verifies each
id exists, carries the labels, and permalinks the section.

### D-6 — Commit sequencing

Docs-only, every commit standalone, in this order:

1. `docs(plan): the ux retrofit-triage chunk-2 plan` — this file.
2. `docs(plan): the habit-family retrofit ux plan` (+ Today-slice
   conversions it owns).
3. `docs(plan): the recurrence-family retrofit ux plan` (+ Task-detail
   conversions it owns).
4. `docs(plan): the timers-alarms retrofit ux plan` (+ Today
   break-rule/alarm conversions + AlarmWindow conversions it owns).
5. `docs(plan): the review-walk-reflection retrofit ux plan` (+
   Review-slice conversions).
6. `docs(plan): the settings-connections retrofit ux plan` (+
   Settings-slice conversions).
7. `docs(report): the ux retrofit-triage report` (+ the census status
   note flip: *phase 4 complete; triage accepted ⇒ phase 5*; + the
   reconciliation-driven register touches if the report's ledger
   retires any entry).

Each retrofit plan's census conversions ride its own commit (the
authority file and the cells it lands land together — chunk-1 D-6's
law). The full §10 gate suite runs once at the end of stage 2 via
`scripts/run-quality-gates.sh` (wait for the flock). No push, no PR
from stage 2; shipping is stage 3's.

### D-7 — Gate steady-state rule (inherited from chunk-1 D-5, unchanged)

`scripts/ux-coverage.sh <plan>` per plan at stage 2 and stage 3:
steady state is **FOUND for every wiring row and ruling**, with the
documented retrofit limitation — journey-step token rows MISSING
(backlog #768, pilot PC-3 precedent, chunk-1 PC-2's corrected row
representation). A MISSING wiring row is a plan defect (fix the plan,
record `PC-n` in the same commit) or a real unmounted step (a
finding — never manufacture an anchor). Per-plan gate outcomes are
recorded as evidence in the stage-2 report. `--gate` is never CI.

### D-8 — Minor decisions

- **The dispatch plan file is untracked** on the user's checkout;
  stage 2/3 read it at the absolute path
  `/home/viktoria/src/taskboard/.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  (chunk-1 D-7's handling, reused; a copy rides this worktree's
  `.artifacts/plans/` untracked for convenience — committing it is
  not this chunk's artefact set).
- **#781 ripple**: if the importance/urgency census amendment lands
  mid-chunk, stage 2 re-checks the census head, records the delta,
  and the new rows ride no chunk-2 plan (the amendment pass owns
  them); the triage ledger maps them to the TE-5/#781 finding.
- **The census status note** flips only in the final commit (D-6.7) —
  phase 4 is complete only when the whole chunk is on the branch.
- **Format exemplars**: chunk-1's four merged plans are the format bar
  (structure, finding-id conventions, PC-n corrections, escape-audit
  column discipline); chunk-1 D-4's escape-path audit method and
  walkthrough-questions wording apply verbatim to the five plans.
- **Alarm-window display shape**: the census's D-P11 anchor-aware
  finding (no Done/Mute on the anchor-aware shape) is restated as
  evidence in the timers-alarms plan and classified in the report —
  never re-ruled inside the plan.

## 3. Stage-2 execution outline

Per dispatch prompt 2.2: enter/recreate the worktree; read the binding
inputs; write the five plans in commit order (D-6) — journeys,
escape-path audits, walkthrough passes (settings-connections with
extra care), findings sections; run the gate per plan (D-7); convert
census cells per D-2 riding each plan's commit; derive the ledger
(D-4), file the per-finding issues (D-5's homing), write the report
(D-5) + census status note; run the full gate suite via the wrapper;
report per-gate results, the triage headline, whether Settings
dominated, deviations.

## 4. Acceptance checklist (stage-3 verifies)

- [ ] Five retrofit plans template-conformant; P1 stories =
      journeys-as-implemented; wiring rows FOUND (or corrections per
      D-7); escape audits cite seam/test per step.
- [ ] Census conversions match chunk-1 D-3 (no convenience rulings;
      findings stay `U`); chunk-1's conversions and all prior rulings
      untouched.
- [ ] Triage completeness: the reconciliation appendix maps every
      stock-take §3 entry exactly once (retired/resolved/findings);
      every findings-section item of the six plans and every pilot
      open item appears in the ledger.
- [ ] Classification honesty: re-classified findings hold against
      D-3's floor (strongest-match, never-weaken recorded).
- [ ] Homing integrity: every finding's issue exists, labels `ui` +
      `p1`/`p2`/`p3`, body permalinks the report section.
- [ ] Retrofit posture: no remediation designs in the report; no code
      changes; Settings friction described with seams cited.
- [ ] Diff is exactly: this plan + five plans + census updates + the
      report.
- [ ] Gates green via the wrapper; PR open + green + `CLEAN`
      (stage 3).

## 5. Risks

- **Ledger blow-up**: the still-unconverted remainder of the
  121-entry inventory is large; without the dedup keys the report
  could exceed its purpose. Mitigation is D-4's family-level finding
  rule + the reconciliation appendix carrying the per-cell enumeration
  so exactness never depends on table brevity.
- **Ruling/finding misclassification is the chunk's named trap**
  (dispatch 2.3): R-Pattern 1's dominance in Settings/Review means
  many cells *will* legitimately convert; the tie-breaker stays
  chunk-1 D-3's patterns, and a borderline cell stays `U` + finding.
- **Anchor drift**: Settings/Review/AlarmWindow cells anchor at
  `84c04a0b`, chunk-1 slices at `dc290df2`; live `main` has moved
  (post-chunk-1 plans' PC-1 live-correction posture applies verbatim:
  baseline anchors verbatim in matrix cells, live seams in wiring
  rows, corrections recorded).
- **#781 ripple** mid-chunk (D-8): stage 2 re-checks the census head;
  the delta rides no chunk-2 plan.
- **The human gate**: the report is not "accepted" when the PR merges —
  phase 5 stays blocked until the user's recorded acceptance; the
  report's §6 says so and stage 3's PR body repeats it.

## 6. Backlog issues opened at stage 1

- None at stage 1: the chunk-1 stage-1 findings (#781 census hole;
  #768 journey-step-token gate limitation; #776 dead CLI verb) already
  carry the out-of-scope discoveries; no new ones surfaced during
  planning (the D-1 story-set derivation worked from existing
  artefacts). Stage 2 files the per-finding homing issues (D-5) and
  any new out-of-scope findings under the standing authorization.
