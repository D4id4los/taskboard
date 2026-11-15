# [Plan] UX Journey Framework — Phase 5 (Remediation) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-15
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the six remediation chunks that fix the
  triage report's findings: the framework's full dress rehearsal at
  scale. This phase DOES change application code (the framework's first
  such phase).
- **Origin**: the accepted triage report
  `.artifacts/reports/2026-11-14-ux-retrofit-triage-report.md` — **the
  user accepted it and its classifications on 2026-11-15** (the recorded
  human gate; the report's §6 requirement is satisfied) — normative: §3
  severity tables (finding ids T-1..T-47, home issues #784–#830), §4
  B-a sublist (the first-pass scope candidate), §2's honest ledger.
  Plus the framework plan §6 phase 5, the owning retrofit UX plans
  (`.artifacts/plans/2026-11-14-*-ux-plan.md` + the three
  `2026-11-13-r7-*-ux-plan.md` pilots), the census
  `docs/ux-surface-census.org`, and `docs/ux_planning.org`.
- **Status**: READY — #831 merged; `main` @ `3a2edea0`. Dispatch order:
  1 → 2 → 3 → 4 → 5 → 6 (default serial; chunk 3 may run in parallel
  with 1–2 — disjoint files — and 5 may parallel 4 once their file
  surfaces are checked against the stage-1 plans; never two agents in
  one worktree).

## 1. Chunks & ordering

| # | Chunk | Findings | Branch | Size |
|---|---|---|---|---|
| 1 | completability (the B-a first pass) | T-2, T-7 | `fix/ux-r1-completability` | M |
| 2 | destructive-action safety | T-1, T-3, T-4 | `fix/ux-r2-destructive-safety` | M |
| 3 | task-detail display-without-control | T-5, T-6 | `feat/ux-r3-task-detail-controls` | L |
| 4 | Board verbs + entry-point asymmetry | T-8, T-9, T-10, T-11, T-12, T-13, T-20, T-21 | `fix/ux-r4-board-entry-points` | L |
| 5 | surface friction: timers, review, capture, settings | T-14, T-15, T-16, T-17, T-18, T-19, T-22, T-23 | `fix/ux-r5-surface-friction` | L |
| 6 | P3 posture & polish | T-24–T-47 (each resolved as code fix, docs/ruling, or recorded deferral) | `docs/ux-r6-p3-posture` | M–L |

**Dependency graph**: 1–3 are the P1 set (user-visible severity order;
1 is the report's named first pass). 4 rebases on 1 (both touch the
board surface: `board-bar.slint`, `view_state.rs`); 5 rebases on 2
(both touch the settings adapter lanes). 6 last (it resolves the
posture findings that 1–5 may partially answer — its stage-1 re-reads
the landed state and resolves only what remains). No PR stacking:
every chunk branches off the then-current `main` after its
predecessor's PR merges.

**Acceptance settles the recorded doubts**: the user accepted the
report's classifications, so (a) T-4's re-creatability doubt resolves
to the rubric's floor — confirmation stages are required; (b) T-7's
display-first doubt resolves to its letter — the *silent* part is the
defect; kiosk gating must become visible (disabled affordance or hint),
not removed.

**Shared execution model** (binding for every chunk — the house
three-stage protocol): one agent session per prompt (N.1 → N.2 → N.3);
doctor first; worktree `../taskboard-WT-<branch-flat>`; stage 1 writes
the TDD-first chunk plan (`docs(plan): …`) resolving every design
question with a recorded decision; stage 2 implements per that plan
(TDD where code is involved, every commit standalone, fast gate green —
see the long-command discipline below); stage 3 rebases onto
`origin/main`, reviews the full branch diff against acceptance + the
code-quality mandate (this is a code phase), re-runs the fast gate,
fixes every
finding on the branch, ships (push, `gh pr create`, checks to green via
CI — the dispatcher watches them,
`mergeStateStatus` `CLEAN`, rebase-dance if `BEHIND`/`DIRTY`), then
removes the worktree (absolute anchor + `git worktree list`
confirmation). Never merge; never touch `main`. Every stage files
`backlog:` issues for out-of-scope findings without asking.

**Long-command discipline (amended 2026-11-15 after three stage-2/3
agents stalled on the coverage suite; binding for every chunk, every
stage):** the subagent harness kills an agent after ~600s without
observable activity, and the gate suite's coverage trio
(`cargo llvm-cov nextest` + quarantine + report) runs 10–14 minutes of
silent compile+test — longer than any subagent may wait. Therefore:

1. **The coverage trio never runs inside a subagent.** A stage-2 or
   stage-3 agent's gate is the fast subset: the pre-commit hooks (fmt +
   clippy + slint lint, already run on every commit) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (the wrapper's flock-safe arbitrary-command mode, ~4–6 min). The
   full AGENTS §10 suite is **dispatcher-owned**: the dispatcher runs
   it in its own session (background + poll) after stage 2's commits
   land and before stage 3 ships — and CI's `Code Coverage` check runs
   it authoritatively on the PR regardless.
2. **Any single command a subagent expects to exceed ~5 minutes runs
   via a background task with short interleaved polls and productive
   work between polls** — and the agent commits all landed work before
   beginning any long wait, so a stall loses nothing.
3. **Stage-exit verification by the dispatcher**: before dispatching
   stage N+1, the dispatcher verifies stage N's landed state directly
   (commits present, tree clean, gate evidence re-runnable) rather
   than trusting the agent's report; a stalled stage is completed by
   the dispatcher from the worktree state, never re-dispatched blind.

**The framework dress-rehearsal mandate (binding for every chunk —
this is what makes phase 5 the rehearsal)**: every remediation changes
user-visible behavior, so each chunk runs the framework end-to-end on
its own work:

- **Stage 1** amends the owning UX journey plan(s) (named per chunk
  below) — the remediation design recorded as changed/new journey steps
  (US/step tables, wiring-inventory rows, command-contract delta,
  escape paths for new confirmations), citing the triage report's
  finding ids and home issues as authority. Where a remediation retires
  a census `U` marker or changes a cell, the amendment plans the census
  update; where it resolves a finding whose right answer is a ruling
  rather than code, the amendment records the ruling (dated, authority
  = the amended plan + the user's acceptance).
- **Stage 2** implements, then runs `scripts/ux-coverage.sh <amended
  plan> --gate` — every new wiring row FOUND; a MISSING row means the
  implementation or the plan is wrong. Census cell updates ride the
  implementation commits. The kiosk/platform-qualified rows follow the
  census's platform-qualification grammar.
- **Stage 3** re-runs the gate, then executes the walkthrough protocol
  (framework §4.4: the four questions per remediated P1/P2 journey step
  with evidence, then the six-heuristic pass over the touched surfaces)
  — BLOCKER/MAJOR walkthrough findings fixed on the branch, MINOR to
  backlog. T-46's doctrine bar (#829) is the regression bar: the
  remediations must not break a recorded positive.

## 2. Chunk 1 — completability (`fix/ux-r1-completability`)

- **Covers**: **T-2** (#785, board-stack US-10): a GUI recovery path
  for archived boards — the board bar's exclusion stands for normal
  navigation, but an unarchive affordance must exist (candidate shapes
  for stage 1: a filtered/archived section in the board bar, or a
  Settings-managed archived-boards roster — the design decision is the
  stage-1 plan's, evidence-board). **T-7** (#790, the kiosk family):
  every kiosk-gated control journey becomes visibly gated — disabled
  affordance or hint at the mount site (habit rows, capture/focus
  toggles, recur row actions, the alarm window's absence posture per
  its recorded fence) per the census's platform-qualification grammar;
  the display halves stay as they are. Owning plans to amend:
  `2026-11-14-board-stack-management-ux-plan.md` (T-2),
  `2026-11-14-habit-family-ux-plan.md`,
  `2026-11-14-capture-search-ux-plan.md`,
  `2026-11-14-recurrence-family-ux-plan.md`,
  `2026-11-14-timers-alarms-ux-plan.md` (T-7's per-family sections).
  Census: the affected kiosk-qualified cells gain the visible-gating
  evidence; T-2's board.archive cell gains the unarchive path.
  Non-goals: no new kiosk capabilities (gating stays), no alarm-window
  mounting change (its R3.5.3 fence stands — only its absence gets an
  honest signal if stage 1 decides one is owed), no T-1 confirmations
  (chunk 2).
- **Acceptance**: both B-a journeys complete end-to-end (unarchive from
  the GUI; every listed kiosk journey visibly gated — walkthrough-
  verified at stage 3); amended plans gate-green; census updated; full
  gates green; PR open + green + `CLEAN`.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the UX remediation phase ("completability — the B-a first pass", branch fix/ux-r1-completability) in the taskboard repo at /home/viktoria/src/taskboard. Preconditions: the phase-4 triage report is merged AND accepted (recorded 2026-11-15); main carries the retrofitted UX plans and the completed census — if not, STOP and report.

1. Run scripts/worktree-doctor.sh from the project root (nonzero = STOP), then: git worktree add -b fix/ux-r1-completability ../taskboard-WT-fix-ux-r1-completability (never without -b, never on main).
2. Read AGENTS.md, docs/ux_planning.org, the triage report .artifacts/reports/2026-11-14-ux-retrofit-triage-report.md §3 (T-2, T-7 rows + §4 B-a sublist), home issues #785/#790 (gh issue view), the owning UX plans, the census's kiosk-qualification grammar, and the code seams each finding cites (board-bar.slint, view_state.rs:1252, presentation.rs habit_editing_visible, habits.slint:7-10,36, adapter.rs:2846-2852, lib.rs:322-330).
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r1-completability-chunk-plan.md (house [Plan] style). Settle at minimum, each as a recorded decision:
   - T-2's unarchive affordance shape (where it mounts, what it dispatches — SetBoardArchived exists as a command; verify — and what the archived-boards visibility rule becomes).
   - T-7's visible-gating grammar per family (disabled affordance vs hint vs both; how it renders on kiosk; which existing widget/state carries it; the alarm window's absence posture — signal or none, decided).
   - The amended journey steps per owning plan (US/step tables, wiring rows, command delta, escape paths), the census cell updates, and the platform-qualification format.
   - The test catalogue: seam tests for new dispatch sites, presentation tests for the gating flags (typed outcomes, never text), kiosk gate tests.
   - Commit sequencing.
4. Resolve every open question with a recorded decision; deviations from the origins need a recorded reason.
5. Commit as: docs(plan): the ux-r1 completability chunk plan (do not implement; do not push).
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the UX remediation phase ("completability", branch fix/ux-r1-completability). Work in /home/viktoria/src/taskboard-WT-fix-ux-r1-completability (recreate per house mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-11-15-ux-remediation-dispatch-plan.md §1/§2 (the dress-rehearsal mandate binds you), and the stage-1 plan .artifacts/plans/<today>-ux-r1-completability-chunk-plan.md (binding). Deviations need a recorded reason.
2. Implement per the stage-1 plan, TDD: the unarchive affordance (T-2) and the visible-gating grammar (T-7) across the five owning surfaces. Every commit standalone. New seams: slint callbacks + adapter install seams + command dispatch as the plan names them; typed-outcome tests only (no text assertions); kiosk gate tests per the existing suite's shapes.
3. Amend the owning UX journey plans per the stage-1 decisions (journey steps, wiring inventory, command delta, escape paths); update the census cells (visible-gating evidence on the kiosk-qualified cells; the board.archive unarchive path; retire the T-2/T-7 register fates per the census grammar).
4. Run scripts/ux-coverage.sh <each amended plan> --gate — every new wiring row FOUND; fix plan or code until green. Then the full gate suite via scripts/run-quality-gates.sh (wait for the flock); any pre-existing unrelated failure = STOP and report.
5. No push, no PR. Never push to or merge main.
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: per-gate results (quality suite + per-plan coverage), census updates made, deviations.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the UX remediation phase (branch fix/ux-r1-completability). Work in /home/viktoria/src/taskboard-WT-fix-ux-r1-completability (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-2/T-7 + §4), the dispatch plan §1/§2 (the code-quality mandate and the dress-rehearsal mandate apply), the stage-1 plan, and the amended plans; rebase onto origin/main (fetch --prune, rebase, push --force-with-lease only if rewritten); review the full branch diff (git diff origin/main...HEAD).
2. Functional focus (this chunk's traps):
   - The two B-a journeys genuinely complete: drive the unarchive journey yourself through the seams (or its journey test) and verify at least three kiosk families render visible gating (the flag is presentation-tested, the affordance mounts disabled, not absent).
   - No text assertions crept in; the gating is typed-state driven (editing-visible facts or their successor), not label-driven.
   - The alarm window's recorded fence (R3.5.3) stands — the chunk signals absence honestly without mounting the window on kiosk.
   - Amended plans: gate-green (re-run scripts/ux-coverage.sh --gate per plan); census cells cite the new evidence; no prior ruling altered.
   - Code quality mandate: SRP crate boundaries (presentation-only UI; command minting where the plan put it), one abstraction level per function body, no dead code.
3. Walkthrough protocol: the four questions per remediated journey step (see the board be unarchived; see the kiosk gate) with evidence; the six-heuristic pass over board bar + today + capture + task detail touched surfaces; fix BLOCKER/MAJOR on the branch, MINOR to backlog.
4. Re-run the full gate suite via scripts/run-quality-gates.sh; any failure is a finding.
5. Fix every issue on the branch (amend/reword). 6. Ship: push, gh pr create (title "fix(ux): the completability remediations — T-2 unarchive, T-7 visible kiosk gating"; body citing the triage report + home issues), checks to green, mergeStateStatus CLEAN. 7. Backlog issues autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ux-r1-completability; confirm via git worktree list.

Report: verdict, findings, walkthrough outcome, PR URL, green + CLEAN + teardown confirmations.
```

## 3. Chunk 2 — destructive-action safety (`fix/ux-r2-destructive-safety`)

- **Covers**: **T-1** (#784): confirmation stages for board/stack delete
  (the house armed-confirm precedent: the convert strip, the recur
  editor's stop-strip); the task-delete contrast (already undoable)
  needs no change — consistency lands by the confirmations, not by
  weakening task delete. **T-3** (#786): habit log's mis-click class
  gets a recovery path — stage 1 decides between confirmation-on-commit
  and an undo arm (`LogHabit` is `NotUndoable(HabitSurface)`; a
  targeted inverse may be the domain's call — the surplus-logs
  first-class design is preserved, not broken). **T-4** (#787):
  confirmation stages for connection.remove / board.detach /
  Stop-Sync (the acceptance settles the re-creatability doubt to the
  floor). Owning plans: board-stack-management (T-1), habit-family
  (T-3), settings-connections (T-4). Census: the confirmation evidence
  on the affected cells; escape-path columns filled. Non-goals: no
  journal redesign (T-17 is chunk 5), no CLI changes beyond what an
  undo arm requires.
- **Acceptance**: every destructive step named above has a staged
  confirmation (and, where stage 1 decided, an undo arm) with escape
  paths; amended plans gate-green; the walkthrough verifies the armed
  strips; PR open + green + `CLEAN`.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the UX remediation phase ("destructive-action safety", branch fix/ux-r2-destructive-safety). Preconditions: chunk 1's PR is merged (or: you branch off main carrying chunk 1's state — verify with git log origin/main); the triage report is accepted.

1. Run scripts/worktree-doctor.sh from the project root, then: git worktree add -b fix/ux-r2-destructive-safety ../taskboard-WT-fix-ux-r2-destructive-safety.
2. Read AGENTS.md, docs/ux_planning.org, the triage report §3 (T-1/T-3/T-4 rows), home issues #784/#786/#787, the owning UX plans, and the seams: actions.rs:935-944, view_state.rs:2924-2925, undo.rs:104-138/231-237, adapter.rs:10301-10309/5892/4117-4129, the convert-strip and stop-strip precedents (recur-editor.slint, the due-editor), habits.slint row grammar.
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r2-destructive-safety-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - The confirmation pattern per finding (armed-confirm strip vs two-step invoke vs typed confirm; reuse the house precedents where they fit — consistency is a heuristic).
   - T-3's recovery: confirmation vs a targeted undo arm for LogHabit (domain change implications if any — the surplus-logs design must survive; if an undo arm is chosen, its NotUndoable class change is a domain decision to record).
   - T-4's scope: which of remove/detach/Stop-Sync get confirmations and their exact armed states; the request lane's interaction (confirm before UiRequest dispatch).
   - The amended journey steps (escape columns now real), wiring rows, command delta; census cell updates.
   - Test catalogue: seam tests (exact commands post-confirmation), presentation tests (armed states), kiosk gates.
   - Commit sequencing.
4. Resolve every open question with a recorded decision. 5. Commit as docs(plan): the ux-r2 destructive-safety chunk plan (no implementation; no push). 6. Backlog issues autonomously.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the UX remediation phase ("destructive-action safety", branch fix/ux-r2-destructive-safety). Work in /home/viktoria/src/taskboard-WT-fix-ux-r2-destructive-safety (recreate per house mechanics if missing).

1. Read AGENTS.md, the dispatch plan §1/§3 (the dress-rehearsal mandate binds you), and the stage-1 plan .artifacts/plans/<today>-ux-r2-destructive-safety-chunk-plan.md (binding). Deviations need a recorded reason.
2. Implement per the stage-1 plan, TDD: the confirmation stages (T-1/T-4) and the chosen T-3 recovery across board/stack delete, habit log, and the settings removal family. Every commit standalone. Typed-outcome tests only; armed-state presentation tests; exact-command seam tests post-confirmation.
3. Amend the owning UX plans (escape columns, wiring rows, command delta); update the census cells (confirmation evidence; T-4's doubt resolved to the floor is recorded where the census carries it).
4. scripts/ux-coverage.sh <each amended plan> --gate green; then the full gate suite via scripts/run-quality-gates.sh (wait for the flock); pre-existing unrelated failure = STOP and report.
5. No push, no PR. Never push to or merge main. 6. Backlog issues autonomously.

Report: per-gate results, census updates, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the UX remediation phase (branch fix/ux-r2-destructive-safety). Work in /home/viktoria/src/taskboard-WT-fix-ux-r2-destructive-safety (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-1/T-3/T-4), the dispatch plan §1/§3, the stage-1 plan, the amended plans; rebase onto origin/main; review the full branch diff.
2. Functional focus (this chunk's traps):
   - Every destructive step now stages: verify each of board.delete, stack.delete, habit.log, connection.remove, board.detach, Stop-Sync commits only after its armed state, and Escape/confirm both resolve through the action layer (click ≡ key).
   - The surplus-logs design survives T-3's recovery (first-class surplus actions unchanged); if an undo arm was added, its domain class change is recorded and tested.
   - T-4's request-lane interaction: confirmations fire before dispatch, no double-dispatch on re-click; the busy lane's posture untouched (T-38 is chunk 6's).
   - Amended plans gate-green (re-run per plan); census cells updated; no prior ruling altered.
   - Code-quality mandate (SRP crate boundaries, one abstraction level, no dead code).
3. Walkthrough protocol: the four questions per remediated step (armed state visible? feedback after confirm? recovery honest?) with evidence; six-heuristic pass over board/today/settings touched surfaces (T-46's bar: the staged-clear positive must not regress). Fix BLOCKER/MAJOR on branch; MINOR to backlog.
4. Re-run the full gate suite; any failure is a finding. 5. Fix on branch (amend/reword). 6. Ship: push, gh pr create (title "fix(ux): destructive-action safety — confirmations for T-1/T-3/T-4"), checks green, CLEAN. 7. Backlog autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ux-r2-destructive-safety; confirm.

Report: verdict, findings, walkthrough outcome, PR URL, green + CLEAN + teardown confirmations.
```

## 4. Chunk 3 — task-detail display-without-control (`feat/ux-r3-task-detail-controls`)

- **Covers**: **T-5** (#788): importance/urgency edit controls in the
  task detail view (the card decorations already render both levels;
  `TaskChanges.importance/urgency` exists — the control and its commit
  funnel are the work). **T-6** (#789): the labels GUI — create/edit/
  delete labels, assign/unassign on tasks, roster display; the census
  hole family (#781 rides this). This is the phase's one true feature
  chunk; the CLI label family's semantics are the domain truth to
  surface. Owning plan: task-editing (its US-12's CLI journey gains
  GUI siblings). Census: label.* and importance/urgency cells fill
  with evidence. Non-goals: no new domain verbs (use
  `TaskChanges`/existing label commands; a missing verb is a recorded
  domain delta for the stage-1 plan), no card-inline label editing
  (T-26 stays P3).
- **Acceptance**: both families editable in the GUI with escape paths;
  amended task-editing plan gate-green; PR open + green + `CLEAN`.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the UX remediation phase ("task-detail display-without-control", branch feat/ux-r3-task-detail-controls). Preconditions: the triage report is accepted; main carries the retrofitted plans.

1. Run scripts/worktree-doctor.sh, then: git worktree add -b feat/ux-r3-task-detail-controls ../taskboard-WT-feat-ux-r3-task-detail-controls.
2. Read AGENTS.md, docs/ux_planning.org, the triage report §3 (T-5/T-6), home issues #788/#789 + #781, the task-editing UX plan, and the seams: models.rs:2394-2422 (decorations render), messages.rs TaskChanges importance/urgency + the label command family, cli.rs the label verbs' semantics, task-view.slint the detail rows grammar, the recur-editor/due-editor overlay precedents, actions.rs context grammar.
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r3-task-detail-controls-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - The importance/urgency control shape (where in the detail view; picker vs segmented control per the house L1 widgets; the commit funnel — buffer, exact TaskChanges dispatch).
   - The labels GUI: where the roster lives, the assign/unassign affordance on the detail view, create/edit/delete flows and their confirmations (T-1's lesson: destructive label delete stages), CLI semantic parity (the CLI family is the behavior contract — no drift).
   - Any missing domain verb = a recorded domain delta with its exact message shape (the framework's command-contract delta section does this — engine work exists because a journey dispatches it).
   - ActionIds + keymap preset totality (free keys per preset, registry-generated hints).
   - Amended journey steps (new US-n blocks), wiring rows, command delta; census cell updates.
   - Test catalogue: domain tests if a delta lands, seam tests (exact commands), presentation tests, keymap totality.
   - Commit sequencing.
4. Resolve every open question with a recorded decision. 5. Commit as docs(plan): the ux-r3 task-detail controls chunk plan (no implementation; no push). 6. Backlog issues autonomously.

Report: plan path, decisions (esp. any domain delta), risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the UX remediation phase ("task-detail controls", branch feat/ux-r3-task-detail-controls). Work in /home/viktoria/src/taskboard-WT-feat-ux-r3-task-detail-controls (recreate per house mechanics if missing).

1. Read AGENTS.md, the dispatch plan §1/§4 (the dress-rehearsal mandate binds you), and the stage-1 plan (binding). Deviations need a recorded reason.
2. Implement per the stage-1 plan, TDD, every commit standalone: the importance/urgency controls and the labels GUI (roster, assign/unassign, create/edit/delete with the decided staging), any recorded domain delta first (domain → engine → UI order, each layer's tests), ActionId registry entries with preset totality, adapter seams, detail-view mounting.
3. Amend the task-editing UX plan (new US blocks, wiring rows, command delta, escape paths); update the census label.*/importance/urgency cells.
4. scripts/ux-coverage.sh <amended plan> --gate green; full gate suite via scripts/run-quality-gates.sh (wait for the flock); pre-existing unrelated failure = STOP and report.
5. No push, no PR. Never push to or merge main. 6. Backlog issues autonomously.

Report: per-gate results, census updates, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the UX remediation phase (branch feat/ux-r3-task-detail-controls). Work in /home/viktoria/src/taskboard-WT-feat-ux-r3-task-detail-controls (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-5/T-6), the dispatch plan §1/§4, the stage-1 plan, the amended task-editing plan; rebase onto origin/main; review the full branch diff.
2. Functional focus (this chunk's traps):
   - CLI parity: the GUI's label semantics match the CLI family exactly (one behavior, one implementation — no GUI-side re-derivation of domain law); importance/urgency commit the exact TaskChanges the plan pinned (seam tests).
   - Crate boundaries: any domain delta is pure domain (no IO/UI leakage); UI stays presentation-only; adapter converts rows to typed values.
   - Preset totality holds (the keymap tests); new ActionIds have registry hints, not hand-written ones.
   - Amended plan gate-green (re-run); census cells cite evidence; the #781 census hole is closed or explicitly annotated.
   - Code-quality mandate (naming, comment discipline, SRP boundaries, one abstraction level, no speculative generality).
3. Walkthrough protocol: the four questions per new journey step (edit journey completes: see field → change → see echo → recover); six-heuristic pass over the detail view. Fix BLOCKER/MAJOR on branch; MINOR to backlog.
4. Re-run the full gate suite. 5. Fix on branch (amend/reword). 6. Ship: push, gh pr create (title "feat(ux): task-detail importance/urgency and labels controls — T-5/T-6"), checks green, CLEAN. 7. Backlog autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ux-r3-task-detail-controls; confirm.

Report: verdict, findings, walkthrough outcome, PR URL, green + CLEAN + teardown confirmations.
```

## 5. Chunk 4 — Board verbs + entry-point asymmetry (`fix/ux-r4-board-entry-points`)

- **Covers**: **T-8** (#791) board.color GUI affordance; **T-9** (#792)
  board.reset's GUI warning surface or recorded GUI-absence ruling
  (destructive reset — chunk 2's confirmation pattern applies if
  afforded); **T-10** (#793) reconcile affordance or ruling; **T-11**
  (#794) `BoardSelect` key binding per preset; **T-12** (#795) the
  default-preset CTA's ActionId; **T-13** (#796) `TaskDueToday` preset
  binding; **T-20** (#803) recur pause/convert entry points beyond the
  buried detail row; **T-21** (#804) `ReviewOpen` binding. Several
  findings' right answer may be a *ruling with recorded justification*
  rather than an affordance (M-b's letter requires justification, not
  necessarily GUI parity) — stage 1 decides per finding with the
  acceptance as cover. Owning plans: board-stack-management (T-8..T-12),
  task-lifecycle (T-13), recurrence-family (T-20), review-walk-
  reflection (T-21). Non-goals: no Settings work (chunk 5), no
  break-rules (chunk 5).
- **Acceptance**: each finding resolved as affordance or recorded
  ruling; amended plans gate-green; PR open + green + `CLEAN`.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the UX remediation phase ("Board verbs + entry-point asymmetry", branch fix/ux-r4-board-entry-points). Preconditions: chunks 1–2 merged (chunk 4 rebases on 1's board-surface state); the triage report accepted.

1. Run scripts/worktree-doctor.sh, then: git worktree add -b fix/ux-r4-board-entry-points ../taskboard-WT-fix-ux-r4-board-entry-points.
2. Read AGENTS.md, docs/ux_planning.org, the triage report §3 (T-8..T-13, T-20, T-21), home issues #791–#796, #803, #804, the owning UX plans, and the cited seams (messages.rs:170/296, cli.rs:429, board-bar.slint:49-51, adapter.rs:8335-8341/8220-8226, journeys.rs:8926, actions.rs:1366-1414/1541-1549, task-card.slint:22-24, task-view.slint:788-832, presets.rs).
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r4-board-entry-points-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - Per finding: affordance or ruling (the M-b decision rule — a CLI-only destructive verb like ResetBoard may rule GUI-absence with justification; a color affordance likely lands where the board bar/context menu lives).
   - The key bindings per preset (free keys, registry-generated hints, totality tests).
   - T-20's entry-point placement (card badge control vs context menu vs detail-only stays with the menu promoted).
   - Amended journey steps, wiring rows, command delta (color needs a picker command path — decide the shape); census cell updates.
   - Test catalogue; commit sequencing.
4. Resolve every open question with a recorded decision. 5. Commit as docs(plan): the ux-r4 board entry-points chunk plan (no implementation; no push). 6. Backlog issues autonomously.

Report: plan path, decisions (esp. affordance-vs-ruling per finding), risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the UX remediation phase ("board entry points", branch fix/ux-r4-board-entry-points). Work in /home/viktoria/src/taskboard-WT-fix-ux-r4-board-entry-points (recreate per house mechanics if missing).

1. Read AGENTS.md, the dispatch plan §1/§5 (the dress-rehearsal mandate binds you), the stage-1 plan (binding). Deviations need a recorded reason.
2. Implement per the stage-1 plan, TDD, every commit standalone: the afforded verbs (color picker path, reconcile, select/CTA/due-today/review-open bindings, T-20's placement) and the ruled-absent ones (census cell annotations only). Registry entries + preset totality; adapter seams; no text assertions.
3. Amend the owning UX plans; update the census cells (affordance evidence or ruling annotations).
4. scripts/ux-coverage.sh <each amended plan> --gate green; full gate suite via scripts/run-quality-gates.sh (wait for the flock); pre-existing unrelated failure = STOP and report.
5. No push, no PR. Never push to or merge main. 6. Backlog issues autonomously.

Report: per-gate results, census updates, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the UX remediation phase (branch fix/ux-r4-board-entry-points). Work in /home/viktoria/src/taskboard-WT-fix-ux-r4-board-entry-points (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-8..T-13, T-20, T-21), the dispatch plan §1/§5, the stage-1 plan, the amended plans; rebase onto origin/main; review the full branch diff.
2. Functional focus (this chunk's traps):
   - Every finding lands as affordance-or-ruling exactly as the stage-1 plan decided; a silently skipped finding is a BLOCKER; a ruling without recorded justification is a BLOCKER (the M-b letter).
   - Preset totality holds; bindings are registry-driven; click ≡ key for every new affordance.
   - T-9/T-10's destructive verbs, if afforded, carry chunk 2's confirmation pattern.
   - Amended plans gate-green (re-run); census consistent; no prior ruling altered.
   - Code-quality mandate.
3. Walkthrough protocol: the four questions per remediated journey (reach the verb click-free; see the color echo; recover from the picker); six-heuristic pass over board bar + cards + detail. Fix BLOCKER/MAJOR on branch; MINOR to backlog.
4. Re-run the full gate suite. 5. Fix on branch. 6. Ship: push, gh pr create (title "fix(ux): board verbs and entry-point parity — T-8..T-13, T-20, T-21"), checks green, CLEAN. 7. Backlog autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ux-r4-board-entry-points; confirm.

Report: verdict, findings, walkthrough outcome, PR URL, green + CLEAN + teardown confirmations.
```

## 6. Chunk 5 — surface friction: timers, review, capture, settings (`fix/ux-r5-surface-friction`)

- **Covers**: **T-14** (#797) break-rules GUI (roster display +
  create/remove affordances — timers plan); **T-15** (#798) reflection
  edit/delete GUI (the journal ledger's read-only posture); **T-16**
  (#799) the env-locked silent absorb gets an honest visible refusal
  (the surface's own doctrine); **T-17** (#800) the settings-lane undo
  posture (stage 1 decides: journal arms for the request lane, or
  recorded justification + confirmation interplay with chunk 2's
  stages); **T-18** (#801) the CLI write path for knobs/sync toggle;
  **T-19** (#802) disabled menu rows' feedback; **T-22** (#805) alarm
  window keyboard parity (ActionIds on the window or its own context);
  **T-23** (#806) habit repair's conditional-invert signal. Owning
  plans: timers-alarms (T-14, T-22), review-walk-reflection (T-15),
  capture-search (T-19), settings-connections (T-16, T-17, T-18),
  habit-family (T-23). Non-goals: no board files (chunk 4), no
  task-detail files (chunk 3).
- **Acceptance**: each finding resolved per its stage-1 decision;
  amended plans gate-green; PR open + green + `CLEAN`.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the UX remediation phase ("surface friction", branch fix/ux-r5-surface-friction). Preconditions: chunks 1–2 merged (chunk 5 rebases on 2's settings-confirmation state); the triage report accepted.

1. Run scripts/worktree-doctor.sh, then: git worktree add -b fix/ux-r5-surface-friction ../taskboard-WT-fix-ux-r5-surface-friction.
2. Read AGENTS.md, docs/ux_planning.org, the triage report §3 (T-14..T-19, T-22, T-23), home issues #797–#802, #805, #806, the owning UX plans, and the cited seams (messages.rs:324/343/549/581, cli.rs:1425/1455/1518/1522, adapter.rs:5707-5712/6068-6075/4117-4129/509-510/2300-2339, review-view.slint:347-351, menu.rs:68-71, lib.rs:483-485/778-786, alarm-window.slint, undo.rs:199-215).
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r5-surface-friction-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - T-14's break-rule surface (where the roster renders; create/remove with chunk 2's staging).
   - T-15's reflection edit/delete affordance shape (ledger rows activate? an edit overlay? the tombstone semantics per DR-7 D-J7).
   - T-16's visible refusal (render the typed refusal outcome — the doctrine says never a silent absorb).
   - T-17's undo posture: journal arms for the UiRequest lane vs recorded justification + the chunk-2 confirmations as the recovery surface (the architectural weight is stage-1's call — biased to the smallest honest fix).
   - T-18's CLI verbs (config set family or taskboard settings set — exact shapes; figment layering respect).
   - T-19's disabled-row feedback (why-text or tooltip posture within the menu grammar); T-22's keyboard parity (window-local ActionIds/context); T-23's invert-condition signal (typed state in the row model).
   - Amended journey steps, wiring rows, command delta (new CLI verbs + any new requests), census cell updates.
   - Test catalogue (app-arm tests for the CLI verbs; seam tests; presentation tests); commit sequencing.
4. Resolve every open question with a recorded decision. 5. Commit as docs(plan): the ux-r5 surface friction chunk plan (no implementation; no push). 6. Backlog issues autonomously.

Report: plan path, decisions (esp. T-17's posture), risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the UX remediation phase ("surface friction", branch fix/ux-r5-surface-friction). Work in /home/viktoria/src/taskboard-WT-fix-ux-r5-surface-friction (recreate per house mechanics if missing).

1. Read AGENTS.md, the dispatch plan §1/§6 (the dress-rehearsal mandate binds you), the stage-1 plan (binding). Deviations need a recorded reason.
2. Implement per the stage-1 plan, TDD, every commit standalone: the eight findings' resolutions in the stage-1 decided order (CLI verbs + app arms first where they exist; UI affordances; feedback postures). Typed-outcome tests; hermetic; no sleeps.
3. Amend the owning UX plans; update the census cells.
4. scripts/ux-coverage.sh <each amended plan> --gate green; full gate suite via scripts/run-quality-gates.sh (wait for the flock); pre-existing unrelated failure = STOP and report.
5. No push, no PR. Never push to or merge main. 6. Backlog issues autonomously.

Report: per-gate results, census updates, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the UX remediation phase (branch fix/ux-r5-surface-friction). Work in /home/viktoria/src/taskboard-WT-fix-ux-r5-surface-friction (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-14..T-19, T-22, T-23), the dispatch plan §1/§6, the stage-1 plan, the amended plans; rebase onto origin/main; review the full branch diff.
2. Functional focus (this chunk's traps):
   - T-16's refusal is rendered, typed, and never silent (the doctrine holds — a regression here is a BLOCKER).
   - T-17's chosen posture is implemented exactly (if journal arms: the lane participates in undo honestly; if justification: recorded in census + plan, and chunk 2's confirmations compose).
   - T-18's CLI verbs respect figment layering and the config read family's shapes; app-arm tests mirror the house move-arm shapes.
   - T-22's parity is keyboard-complete on the window (all three modal buttons reachable in every preset); T-15's tombstone semantics survive.
   - Amended plans gate-green (re-run); census consistent; code-quality mandate (SRP boundaries, one abstraction level).
3. Walkthrough protocol: the four questions per remediated journey (break-rule create→roster echo→remove staged; reflection mistype→edit→ledger echo; env-locked toggle→visible refusal→understand); six-heuristic pass over settings/review/alarm/menu surfaces. Fix BLOCKER/MAJOR on branch; MINOR to backlog.
4. Re-run the full gate suite. 5. Fix on branch. 6. Ship: push, gh pr create (title "fix(ux): surface friction remediations — T-14..T-19, T-22, T-23"), checks green, CLEAN. 7. Backlog autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ux-r5-surface-friction; confirm.

Report: verdict, findings, walkthrough outcome, PR URL, green + CLEAN + teardown confirmations.
```

## 7. Chunk 6 — P3 posture & polish (`docs/ux-r6-p3-posture`)

- **Covers**: the remaining findings T-24–T-47 (home issues
  #807–#830), each resolved as **code fix**, **docs/ruling**, or
  **recorded deferral** (deferred = stays open on its issue, no
  milestone, reason recorded): T-24's CLI-polish family and T-46's
  doctrine bar are expected to resolve as rulings/docs (the doctrine
  bar lands in `docs/ux_planning.org` as the regression bar and in the
  census's posture note); the convenience findings (T-26 inline card
  editing, T-30 capture target, T-37 board-create consistency, T-40
  badge feedback, T-41 log acknowledgement) are candidates for small
  code fixes; the rest resolve per stage-1 judgment. This chunk also
  closes the phase: the census status line moves to *phase 5 complete*;
  the framework plan §6's phase-5 exit is recorded. Owning plans: all
  nine retrofit plans (amendments only where a code fix changes a
  journey). Non-goals: re-opening accepted classifications; no new
  feature work.
- **Acceptance**: every finding T-24–T-47 has a recorded resolution
  (fix shipped / ruling recorded / deferral with reason on its issue);
  amended plans (if any) gate-green; census status updated; PR open +
  green + `CLEAN`.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the UX remediation phase ("P3 posture & polish", branch docs/ux-r6-p3-posture). Preconditions: chunks 1–5 merged; the triage report accepted.

1. Run scripts/worktree-doctor.sh, then: git worktree add -b docs/ux-r6-p3-posture ../taskboard-WT-docs-ux-r6-p3-posture.
2. Read AGENTS.md, docs/ux_planning.org, the triage report §3 MINOR table (T-24..T-47), home issues #807–#830 (gh issue view each), the owning plans, and the landed main (chunks 1–5 may have already resolved posture findings — re-derive what remains per finding).
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-r6-p3-posture-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - Per finding: fix (exact shape), ruling (text + census home), or deferral (reason; the issue stays open, no milestone). T-24 and T-46's expected rulings are the dispatch's prior — overturn them only with recorded reasons.
   - The code fixes' designs (small: acknowledgement transients, feedback echoes, consistency aligns to the chunk-1..5 patterns), their owning plan amendments, census cell updates.
   - The doctrine bar's landing in docs/ux_planning.org (T-46) and the census posture note.
   - The phase-closure edits (census status line; the completion bookkeeping).
   - Commit sequencing.
4. Resolve every open question with a recorded decision. 5. Commit as docs(plan): the ux-r6 p3 posture chunk plan (no implementation; no push). 6. Backlog issues autonomously.

Report: plan path, the fix/ruling/deferral split headline, decisions, risks.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the UX remediation phase ("P3 posture & polish", branch docs/ux-r6-p3-posture). Work in /home/viktoria/src/taskboard-WT-docs-ux-r6-p3-posture (recreate per house mechanics if missing).

1. Read AGENTS.md, the dispatch plan §1/§7 (the dress-rehearsal mandate binds you where code changes), the stage-1 plan (binding). Deviations need a recorded reason.
2. Execute per the stage-1 plan, TDD where code, every commit standalone: the small fixes; the rulings into census + docs/ux_planning.org (T-46's doctrine bar verbatim from its evidence); the deferrals annotated on their issues (gh issue comment — permitted); the owning-plan amendments where a fix changed a journey; the census status line.
3. scripts/ux-coverage.sh <each amended plan> --gate green (where plans changed); full gate suite via scripts/run-quality-gates.sh (wait for the flock); pre-existing unrelated failure = STOP and report.
4. No push, no PR. Never push to or merge main. 5. Backlog issues autonomously.

Report: per-gate results, the resolution ledger (fix/ruling/deferral per finding id), deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the UX remediation phase (branch docs/ux-r6-p3-posture). Work in /home/viktoria/src/taskboard-WT-docs-ux-r6-p3-posture (recreate per house mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the triage report (T-24..T-47), the dispatch plan §1/§7, the stage-1 plan, the amended plans; rebase onto origin/main; review the full branch diff.
2. Functional focus (this chunk's traps):
   - The resolution ledger is complete: every finding id T-24..T-47 appears exactly once with its fate; a silently dropped finding is a BLOCKER; deferrals carry reasons on their issues (gh issue view to spot-check three).
   - No accepted classification was re-opened; rulings cite their authorities; the doctrine bar lands verbatim (T-46's evidence preserved).
   - Code fixes are small and typed-outcome tested; no scope creep into P1/P2 territory (that landed in chunks 1–5).
   - The census status line and the phase-closure record land.
3. Evidence & artefact mandate + code-quality mandate (where code). 4. Re-run the full gate suite; any failure is a finding. 5. Fix on branch (amend/reword). 6. Ship: push, gh pr create (title "docs(ux): the P3 posture resolutions and phase-5 closure"), checks green, CLEAN. 7. Backlog autonomously. 8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-r6-p3-posture; confirm.

Report: verdict, findings, PR URL, green + CLEAN + teardown confirmations.
```

## 8. Completion bookkeeping

- Each chunk's PR closes its findings' home issues on merge (close as
  completed with the PR URL — the archive convention, AGENTS §7) except
  deferred findings, whose issues stay open without milestones.
- **Deliberately not dispatched here**: T-24's CLI-twin implementation
  (expected: deferral ruling; the issues stay the record), any new
  feature work beyond the findings, the mandate flip (phase 6 — its own
  tiny docs chunk after phase 5's last PR merges).
- **What opens next**: phase 6 — the mandate activation chunk flips
  AGENTS.md §7's advisory wording to mandatory (framework §6 phase 6),
  after which every new user-visible work requires an accepted UX
  journey plan before dispatch.
