# Backlog-Reduction Round 5 ("bug wave + survivors") — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-22
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the accepted round-5 scope
  (`.artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md`,
  operator-accepted 2026-11-22 with both operator gates ruled
  in-session): the p1/p2 bug wave A1–A6 first, the flaky pass B1, the
  mutation-survivor waves C1–C2, the journey-gated feature follow-ups
  (J1–J4 journeys → D1i/D1ii/D2/D3a/D3b/D4), and the residue tail
  E1–E4. No code in this commit.
- **Origin**: the scope report (§A bug wave, §B flaky pass, §C
  mutation wave, §D feature follow-ups, §E residue, §F deferrals,
  §G shape — normative); issue bodies normative; **the two operator
  rulings are binding**: #1215 → option (b) (wording amendment, no
  storage change; recorded on the issue, comment 7079171410), #1212 →
  option (b) (blank = anchor-time-only posture; the additive domain
  change is pre-authorized; comment 7079171625).
- **Status**: READY. No other round in flight — the standing resource
  law applies (≤3 concurrent, memory gate, flock-deferral). Gates:
  the J1–J4 journey acceptance (one sitting), A2's verdict checkpoint
  (the #1205 root cause may implicate a focus law — dispatcher
  verifies before stage 2 implements), #892's condition check
  (compressed chunk).
- **Orchestration law**: `docs/agent_dispatch.org` (PR #852 + #894's
  flock-deferral rule + the round-3 amendments). Stage prompts
  restate the discipline verbatim.

## 1. Chunks & ordering

| Chunk | Issues | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| A1 | #1198, #1200 | `fix/today-habit-interaction` | habit DnD/move no-ops + mouse selection | no UX plan in origin (bug fixes; census note rides) | M |
| A2 | #1205, #1203, #1204, #1207 | `fix/task-edit-view-focus` | the title-steals-Enter/focus investigation + the edit view's keyboard/focus/undo cluster | no UX plan in origin (bug fixes; law-level fix likely) | M/L |
| A3 | #1197 | `fix/today-severing-sweep` | Today-root sever + the view-root re-sweep (third recurrence = systemic) | no UX plan in origin | S/M |
| A4 | #1206 | `fix/date-picker-clip` | the picker overlay clipping the text field | no UX plan in origin | S |
| A5 | #1150 | `fix/outbox-stranded-ops` | board ops stranded on wiped rows | no UX plan in origin | S/M |
| A6 | #1215, #776, #1160, #892, #907, #1103/#867 | `chore/small-residue-sweep` | ruled wording amendment + dead verb + doc/small-CI sweep — COMPRESSED | no UX plan in origin | S |
| B1 | #1164–#1170, #1214 | `test/retained-etag-flaky-pass` | the five-issue wiremock seam + the masked-row flaky; armed standbys re-checked | no UX plan in origin | M/L |
| C1 | #1231 | `test/commands-predicate-coverage` | the fake-engine unit layer for the decision predicates | no UX plan in origin | M |
| C2 | #1229, #1230, #1175, #1176 | `test/cli-tier-mutation-kills` | exit-code table, prose family, logout survivor, pdf tier | no UX plan in origin | M |
| J1 | #1146/#1154/#1155/#1156/#1226/#1227 | `docs/undo-family-journeys` | the undo family's journeys + 2 chunk plans | **journey plans** | M (docs) |
| J2 | #1147/#1148/#1149 | `docs/status-export-journeys` | status/export polish journey + chunk plan | **journey plans** | S (docs) |
| J3 | #1199/#1201/#1202/#1208/#1196 + #1218/#1144/#1212/#1035/#1048/#1049 | `docs/habit-surface-journeys` | Today-habits + editor polish journeys + 2 chunk plans | **journey plans** | M (docs) |
| J4 | #1225/#1234/#1151/#1152/#1184/#1104 | `docs/misc-gated-journeys` | misc gated journeys + chunk plan (#1104 design-only story) | **journey plans** | S (docs) |
| D1i | #1146, #1154 | `feat/restore-clear-surfaces` | GUI restore surfaces + ClearReminderRules surface | J1 accepted | M |
| D1ii | #1155, #1156, #1226, #1227 | `feat/undo-browse-parity` | CLI undo-to/redo-to + deep history + picker UX | J1 accepted | M |
| D2 | #1147, #1148, #1149 | `feat/status-export-polish` | status command lane + live refresh + export preview | J2 accepted | M |
| D3a | #1199, #1201, #1202, #1208, #1196 | `feat/today-habits-interaction-polish` | habit row redesign + interactions | J3 accepted | M |
| D3b | #1218, #1144, #1212, #1035, #1048, #1049 | `feat/habit-editor-history-polish` | tombstone history entry + heatmap cells + the ruled blank-time semantics + editor follow-ups | J3 accepted | M/L |
| D4 | #1225, #1234, #1151, #1152, #1184, #1104 | `feat/misc-gated-impl` | the misc gated implementations (#1104 design story only) | J4 accepted | M |
| E1 | #1181/#1186/#1164/#1217/#1236/#1237/#1172/#1183 | `test/gate-journey-token-residue` | gate/journey token + harness residue | no UX plan in origin | M |
| E2 | #1211/#1157/#1158/#1137/#1095 | `fix/state-storage-small-residue` | five domain/storage smalls | no UX plan in origin | M |
| E3 | #1017/#1185/#1232/#1216/#1163/#1159 | `test/backend-residue` | six testing-backend smalls | no UX plan in origin | M |
| E4 | #1121/#1125/#1126/#1127/#1128 | `refactor/widget-anatomy-remainder` | the S8 remainder (five widget-anatomy items) | no UX plan in origin | M |

**Dependency graph & order** (≤3 concurrent):

- **Phase 0** (gate-independent + journeys): **J1 ∥ J2 ∥ J3 ∥ J4**
  (docs-only journeys — start first, the gate sits before D-chunks) ∥
  **A1 ∥ A2** (the p1s) — plus **A6 compressed** by the dispatcher
  (any time; it carries the ruled #1215 wording).
- **Phase 1** (gate-independent implementation): **A3 ∥ A4 ∥ A5** →
  **B1** ∥ **C1** → **C2** ∥ **E1** → **E2** ∥ **E3** ∥ **E4**.
- **Phase 2** (after the journey gate): **D1i ∥ D2 ∥ D3a** → **D1ii ∥
  D3b** → **D4**.

**Dispatch mechanics**: one agent session per prompt, in order; never
two agents share a worktree; worktrees at
`../taskboard-WT-<flat-suffix>`, branched off the then-current `main`
with `git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`
(never without `-b`, never on `main`); landed branch ⇒ resume on
`<branch>-v2`; `scripts/worktree-doctor.sh` first, every agent,
nonzero (disk or MemAvailable floor) = STOP.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding — `docs/agent_dispatch.org`;
PR #852 + the #879/#894 flock-deferral rule + the round-3
amendments)*: a subagent is killed after ~600s without observable
activity, so long silent commands kill working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe; background + poll with useful work between polls if
   needed). **Flock-deferral rule: when the gate lock is contended and
   no other useful work remains, never block — commit all landed work,
   report "gate deferred: lock held by <holder>", and end the stage;
   the dispatcher runs the gate.** Expected-heavy single builds route
   through `scripts/memory-gate.sh`. The dispatcher runs the full
   AGENTS §10 suite between stage 2's landing and stage 3's ship; CI's
   Code Coverage check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: push, open the PR, report — the
   dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
   teardown, at-merge closures (§12).
3. *Any command expected over ~5 minutes runs backgrounded* with
   short interleaved polls and useful work between polls — commit all
   landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim*.
5. *Dispatcher stage-exit verification* between stages — resume over
   redo, playbook §8.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` issues immediately (AGENTS §7 standing authorization);
   report the numbers.

**Code-quality mandate** (stage 3, every code chunk — verbatim):
beyond functional review, evaluate (a) code quality: naming, comment
discipline (comments state constraints, not narration), idiomatic use
of the approved stack, no dead code or speculative generality, test
quality per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line; (c) a single
level of abstraction per function body. Findings are fixed on the
branch.

**UX completeness mandate** (stage 3, every journey-carrying chunk —
verbatim): re-run `scripts/ux-coverage.sh <journey-plan> --gate`
(step tokens are REQUIRED rows; MISSING on a claimed row is not
done); execute the walkthrough protocol (`docs/ux_planning.org`) over
every P1 story with evidence, then the six-heuristic pass over the
touched surface; BLOCKER/MAJOR fixed on the branch, MINOR to backlog
issues. Reports a verdict.

**Journey-gated chunk pattern** (D1i/D1ii/D2/D3a/D3b/D4): the J-agent
(Phase 0) lands the accepted journey plan + the chunk plan on the
chunk's branch; stage 2 opens "GATE CLEARED: the operator accepted
the journey — implement exactly its wiring rows"; stage 3 carries the
code-quality + UX completeness mandates. Each such chunk records its
traps below; prompts follow the round-4 HF1 worked example with the
stated substitutions.

## 2. Chunk A1 — Today habit interaction (#1198, #1200, `fix/today-habit-interaction`)

- **Covers** (normative): #1198 — the drag-n-drop gesture starts but
  never moves (survey seams: `habits.slint:226` drag-zone, `:264`
  gap-zone `drop-asked(row, gap)`, the adapter's `MoveHabit` dispatch,
  `ActionId::HabitMoveUp/Down` at `actions.rs:760–761`, menu rows
  `menu.rs:454–455/1785–1786`, dispatch `router.rs:1384–1396`); the
  context-menu moves are no-ops too. #1200 — mouse click cannot select
  a habit row (keyboard selection works). **Diagnosis-first**: the
  issue explicitly says "needs diagnosis, not blind patching".
- **Acceptance**: per-issue red→green: a DnD gesture moves the row
  (pinned at the gap-zone seam), the context-menu moves move it, a
  mouse click selects (pinned structurally); the task DnD/compact-move
  grammar (r9-landed) is unregressed; the r9 multi-select/settle
  interplay checked (the H3 gap-drop pattern is the landed base — the
  fix explains why it didn't fire for habits); diffs confined to the
  named seams.

**Prompt A1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk A1 of round 5 ("Today habit interaction", branch fix/today-habit-interaction) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — never text assertions; (c) diagnosis-first per the issue — the survey's suspected seams are hypotheses to verify, not the fix; never weaken a correct test; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/today-habit-interaction ../taskboard-WT-fix-today-habit-interaction off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md, the scope report .artifacts/reports/2026-11-22-backlog-reduction-round5-scope-report.md §A (normative), the dispatch plan .artifacts/plans/2026-11-22-backlog-reduction-round5-dispatch-plan.md §1/§2, issues #1198 #1200, and the landed base: the r9 habit chunks' plans (2026-11-18-habit-parity-ux-plan.md family) — the H2 row rework and H3 keyboard/DnD patterns.
2. Read the seams (verify the survey, don't trust it): crates/taskboard-ui-slint/ui/compositions/habits.slint:226 (drag-zone) and :264 (gap-zone drop-asked), the adapter's MoveHabit dispatch path, src/actions.rs:760-761, src/menu.rs:454-455/1785-1786, src/router.rs:1384-1396; the task-row DnD that works (the contrast case — what it wires that the habit rows don't); the row's mouse-selection path vs the keyboard selection that works.
3. Diagnose both: why the DnD/move pipeline no-ops for habits (the dropped event, an unhandled ActionId, a settle cancellation — name the exact mechanism) and why mouse selection fails; decide the fixes.
4. Write the chunk plan at .artifacts/plans/2026-11-22-today-habit-interaction-plan.md with the pinning tests (structural for selection, journey/structural for the move). Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the today habit interaction chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, both diagnoses, risks, estimated diff size.
```

**Prompt A1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk A1 of round 5 (branch fix/today-habit-interaction).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) TDD — each pin fails pre-fix; the task-row DnD grammar is unregressed; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§2, and the stage-1 plan .artifacts/plans/2026-11-22-today-habit-interaction-plan.md in ../taskboard-WT-fix-today-habit-interaction (recreate on the branch if missing; landed branch ⇒ fix/today-habit-interaction-v2).
2. Implement exactly per the plan, TDD, per-issue commits. Diff confined to the named seams; new slint seams carry `// slint: <callback>`.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence per issue, per-gate results, deviations.
```

**Prompt A1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk A1 of round 5 (branch fix/today-habit-interaction).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the task DnD/compact-move and multi-select grammars are unregressed (run their journeys); each fix's mechanism matches the recorded diagnosis — a fix that works by coincidence (e.g. swallowing the event elsewhere) is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§2 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-today-habit-interaction.
2. Functional focus — this chunk's traps: (a) the DnD fix covers drag-to-gap AND drag-to-row (both drop modes the gap-zone pattern defines); (b) the move actions respect the settled order (a move that works visually but reverts on re-render is the #1033-class settle bug); (c) mouse selection composes with the keyboard selection and the multi-select state (one selection truth).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1198 #1200 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 3. Chunk A2 — task edit view focus cluster (#1205 + #1203/#1204/#1207, `fix/task-edit-view-focus`)

- **Covers** (normative): **#1205's systemic investigation** — Enter
  stolen by the Task Title from every other widget; overlay dismissal
  always restores focus to the title. The investigation decides
  whether this is a focus-restoration law violation (the N-3 law the
  r8 focus chunk installed), a router-level default-focus misconfig,
  or a per-mount grab bug — the fix is law-level, not per-widget
  patches. **#1203** — C-f/b/n/p dead in the edit view (the r9 K2
  directional law's missing arm: the edit view's per-view focus table
  was never registered). **#1204** — the Description field has no
  focused state in focus mode. **#1207** — edits became un-undoable
  mid-session (undo did nothing, never refused — a silent no-op, the
  worst class; diagnose against the landed undo-line/RestoreStack
  machinery before touching anything).
- **Dispatcher verdict checkpoint**: A2.1's diagnosis may conclude the
  fix requires a focus-law amendment or an undo-machinery change —
  the dispatcher verifies the recorded diagnosis before stage 2 and
  escalates to the operator if the fix contradicts a standing law
  (N-3, the undo-line contracts).
- **Acceptance**: #1205 — Enter activates the focused widget on the
  inventoried widget set (a table-driven journey/structural test);
  overlay dismissal restores the opener (the N-3 contract, now
  actually enforced in this view); #1203 — the directional motions
  work per the K2 law (the per-view table registered); #1204 — the
  Description field's focused state renders; #1207 — the undo of an
  edit is either applied or a typed refusal, never silent (pinned);
  the edit-view corpus green; diffs confined to the view/router seams
  + the diagnosed root.

**Prompt A2.1 (planning + diagnosis):**

```text
You are the stage-1 PLANNING agent for chunk A2 of round 5 ("task edit view focus cluster", branch fix/task-edit-view-focus) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes — the #1207 fix's bar is "applied or typed refusal, never silent"; (c) diagnosis-first: #1205 is filed as a systemic investigation — the root cause must be named before any fix; the N-3 focus-restoration law and the K2 directional law are the standing authorities the fix must compose with (a fix that contradicts either is escalated, not implemented); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/task-edit-view-focus ../taskboard-WT-fix-task-edit-view-focus off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§3, issues #1205 #1203 #1204 #1207, and the standing laws: the N-3 focus contract (the r8 focus-restoration plan/journeys), the K2 directional law (the r9 keyboard-focus plan), the landed undo-line machinery (the UF1/B1-era plans).
2. Diagnose #1205 first: trace the Task Title's Enter handling and the overlay dismissal's focus-restore path — name the exact mechanism that makes the title the universal Enter/focus target (a default-focus assignment, a return-focus fallback, a grab scope). Then #1203 (is the edit view's directional table missing from the K2 registration?), #1204 (the field's focus-state wiring), #1207 (which undo path the edit writes take and where the silent no-op happens).
3. Decide the fixes per the diagnosis; record explicitly whether any fix contradicts or amends a standing law (N-3/K2/undo contracts) — such a contradiction is a dispatcher-escalation item, recorded as GATE-ESCALATED in the plan.
4. Write the chunk plan at .artifacts/plans/2026-11-22-task-edit-view-focus-plan.md with the widget-inventory test table for #1205 and per-issue pins. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the task edit view focus chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the #1205 root cause, the law-compliance verdicts, risks, estimated diff size.
```

**Prompt A2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk A2 of round 5 (branch fix/task-edit-view-focus). Dispatcher verdict recorded: the diagnosis complies with the standing laws — implement exactly per the plan.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes — #1207's bar: applied or typed refusal, never silent; (c) TDD — the widget-inventory table fails pre-fix on the stolen-Enter rows; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-22-task-edit-view-focus-plan.md in ../taskboard-WT-fix-task-edit-view-focus (recreate on the branch if missing; landed branch ⇒ fix/task-edit-view-focus-v2).
2. Implement exactly per the plan: the #1205 root-cause fix (law-level), the K2 table registration (#1203), the Description focus state (#1204), the #1207 undo-silence fix. Diff confined to the recorded sites; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence per issue, per-gate results, deviations.
```

**Prompt A2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk A2 of round 5 (branch fix/task-edit-view-focus).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the widget-inventory table covers every widget the issue inventoried (a missing row is a finding); the N-3/K2 laws are composed, not patched around (the plan's law-compliance verdicts re-checked against the diff); #1207's pin proves the never-silent bar on every edit-write path; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§3 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-task-edit-view-focus.
2. Functional focus — this chunk's traps: (a) the title's special behavior is preserved where legitimate (title-edit activation still works — Enter ON the title activates it); (b) the K2 registration doesn't double-register with the global table (the C-5 collision rule); (c) #1207's fix doesn't weaken the undo-line contracts (the restore-family semantics from N-B/C9 stand).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1205 #1203 #1204 #1207 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 4. Chunks A3–A6 (the bug-wave remainder + the compressed sweep)

**A3 — #1197 (`fix/today-severing-sweep`, S/M).** Covers: the
`Theme.layout-severing-max-width` sever applied at the Today view root
+ a **re-sweep of every view root** (the issue: "third recurrence
warrants a systemic fix") — the inventory enumerates each view root's
sever presence; missing ones are fixed in the same change; the
upstream-facing #778 note rides if the root cause is the Slint
bounded-max behavior again. Acceptance: the Today width is uncapped
(pinned structurally); the sweep inventory recorded with per-root
verdicts; a census note recording the systemic posture. *(Prompts:
standard three-stage pattern; stage-1's sweep inventory is the
plan's core; body references #1197.)*

**A4 — #1206 (`fix/date-picker-clip`, S).** Covers: the date picker
overlay clips the text field (field peeking from behind the month
grid) — a W2-migrated overlay geometry bug on the landed
AnchoredOverlayHost/ModalDialogHost base. Acceptance: the field is
fully visible with the grid open (structural geometry pin); the flip
geometry (below/above) re-checked; diffs confined to the overlay
mount. *(Standard pattern; body references #1206.)*

**A5 — #1150 (`fix/outbox-stranded-ops`, S/M).** Covers: the
`owning_board` doc-vs-behavior divergence (board ops return `Some`
unconditionally; wiped rows strand outside both the cycle and the
orphan lane). Round-level decision frame: either the doc is wrong
(board ops return `None` when the row is gone — the op must then
carry enough to route) or the orphan lane gains the board-op catch.
Stage-1 decides with the convergence argument (which choice can't
strand an op and can't double-apply one). Acceptance: the strand
scenario pinned red-then-green; the choice's semantics typed; the
sync corpus unchanged on healthy paths. *(Standard pattern; body
references #1150.)*

**A6 — COMPRESSED (`chore/small-residue-sweep`).** Dispatcher-owned
per `agent_dispatch.org` §7 (plan committed first stating the
compression): **#1215** — the ruled wording amendment: amend
`.artifacts/plans/2026-11-21-undo-ui-reminder-undo-plan.md` R-5/US-2-S3
to "the undo adds no delivery-addressing action; the action-layer
prune is the standing delete contract" with the dated supersession
note citing the ruling comment (7079171410); **#776** — remove the
dead `boards use` verb (verify unreachability by dispatch grep; the
parse/completion/receipt arms go; typed removal); **#1160** — the
idle-box discard rule documented (where wall-clock measurements are
recorded — the runtime report's home + the dispatch law's
measurement notes); **#892** — conditional: verify the
byte-identical constraint lifted (R4's consolidation renamed
binaries; check the #682 comment's reference), update or
record-and-defer; **#907** — the shellcheck gate added small (CI
step or pre-commit hook over `scripts/*.sh`; the failure list
triaged — fix-or-waive per file with recorded rationale); **#1103/
#867** — operator notes posted (the unregistered worktree directory;
the hosted Deck curl matrix still pending). Acceptance: each item's
evidence on its issue; the PR diff confined to the named files;
gates that apply run (the shellcheck addition is self-applied to the
scripts).

## 5. Chunk B1 — retained-ETag flaky pass (#1164–#1170, #1214, `test/retained-etag-flaky-pass`)

- **Covers** (normative): the five `retained_etag_wiremock` flakies —
  #1166, #1167, #1168, #1169, #1170 (the S1 band tests' shared
  wiremock timing seam; five issues, likely one root cause) — plus
  **#1214** (the flow_table masked-row flaky, different seam, rides
  the pass). The armed standbys (#697/#650/#968) are re-checked for
  capture state only — no speculative fixes (the standing watchlist
  law).
- **Acceptance**: the shared-seam hypothesis confirmed or refuted
  with load-reproduction evidence (the R2-method: backgrounded
  loaded runs, counts recorded); the fix deterministic (await the
  structural fact or deadline-poll — never a bigger yield count,
  never a sleep); the five issues close on one evidence base (or the
  honest per-issue split if the seams differ); #1214 dispositioned
  separately; the band tests' contracts unchanged (no tolerance
  loosening — the #695-class convergence semantics stay pinned).

**Prompt B1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B1 of round 5 ("retained-ETag flaky pass", branch test/retained-etag-flaky-pass) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). Load-reproduction loops run backgrounded through the memory gate with useful work between polls. When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (b) the fix is deterministic-by-construction — await the structural fact or poll with a deadline; never a bigger yield count, never a sleep, never a tolerance loosening; (c) the band tests' convergence contracts (the #695 semantics) stay pinned byte-for-byte; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/retained-etag-flaky-pass ../taskboard-WT-test-retained-etag-flaky-pass off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 determinism laws), the scope report §B (normative), the dispatch plan §1/§5, issues #1164 #1165 #1166 #1167 #1168 #1169 #1170 #1214, and the S1 plan (.artifacts/plans/*sync-etag* — the band tests' design).
2. Read the seam: crates/taskboard-sync-files-nextcloud/tests/retained_etag_wiremock.rs (the five tests' shared harness — the wait/poll mechanics, the wiremock timing assumptions) and the flow_table test's masked-row path (#1214, different seam — separate disposition).
3. Decide: the reproduction method (loaded parallel runs, counts), the root-cause hypothesis per the harness read, the deterministic fix (await/deadline), the five-issues-one-evidence-base closure structure, and #1214's separate disposition method.
4. Write the chunk plan at .artifacts/plans/2026-11-22-retained-etag-flaky-pass-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the retained-etag flaky pass chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the hypothesis, risks, estimated diff size.
```

**Prompt B1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B1 of round 5 (branch test/retained-etag-flaky-pass).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended). Reproduction loops run backgrounded through the memory gate with useful work between polls.
[L] Round laws (binding): (a) typed outcomes; (b) deterministic-by-construction; (c) contracts pinned byte-for-byte; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B (normative), the dispatch plan §1/§5, and the stage-1 plan .artifacts/plans/2026-11-22-retained-etag-flaky-pass-plan.md in ../taskboard-WT-test-retained-etag-flaky-pass (recreate on the branch if missing; landed branch ⇒ test/retained-etag-flaky-pass-v2).
2. Implement exactly per the plan: reproduce under load (counts recorded), the deterministic fix, the stability evidence (backgrounded repeated runs), #1214's disposition. Diff confined to the harness/tests; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: reproduction counts, stability evidence, per-gate results, deviations.
```

**Prompt B1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B1 of round 5 (branch test/retained-etag-flaky-pass).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) no tolerance loosened, no sleep added, no yield count tuned (grep the diff); the five tests' convergence assertions are byte-identical; the stability evidence's run counts are stated; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B (normative), the dispatch plan §1/§5 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-retained-etag-flaky-pass.
2. Functional focus — this chunk's traps: (a) the fix addresses the recorded mechanism (a wait that can lose a race) — a reordering that merely makes the race unlikely is a finding; (b) #1214's disposition is separate and evidence-carried (not bundled into the wiremock fix); (c) the armed standbys (#697/#650/#968) were re-checked, not touched (their state notes updated only if the pass learned something relevant).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1164–#1170 and #1214 — closures are the dispatcher's at merge per the evidence), report the URL + per-issue verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict.
```

## 6. Chunks C1 + C2 — the mutation survivors

**C1 — #1231 (`test/commands-predicate-coverage`, M).** Covers: the
commands-layer decision predicates are mutation-survivable wholesale —
the fix is a fake-engine unit layer exercising the predicates'
decisions at the lowest layer (the S3 report's sequel; the issue's
proposal is normative). Acceptance: the predicate decisions pinned
unit-side (each decision branch has a test whose outcome the mutant
would flip); no integration-layer duplication (one-behavior-one-test
— the unit layer is the lowest layer for these); kills evidenced.
*(Standard three-stage pattern; the S3 report
`.artifacts/reports/2026-11-21-cargo-mutants-commands-report.md` is
the origin; body references #1231. Traps: the fake engine must be
the shared test_support one (#981's dedup), not a new local double.)*

**C2 — #1229/#1230/#1175/#1176 (`test/cli-tier-mutation-kills`, M).**
Covers: #1230 (the `*_command` exit-code family across seven verbs —
one CLI-tier table test), #1229 (the `render_text_*` prose family),
#1175 (logout_command's survivor), #1176 (the pdf-export dispatch
branch untested in its own feature tier). Acceptance: the table test
pins exit codes per verb class; the prose pins are structural
(which sections render for which outcome kinds — typed, never
string-equality on prose); each kill TDD-first; the survivors list in
the S3 report updated with their kills. *(Standard pattern; bodies
normative; body references all four.)*

## 7. The journey wave (J1–J4, Phase 0, docs-only)

Common spine (as round 4): read AGENTS.md + docs/ux_planning.org +
the scope report §D + the family's issues/origins; doctor; worktree
`git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`; write
the journey plan(s) from the template **and** the per-chunk plans;
commit `docs(plan): …`; report GATE-PAUSED. Backlog issues per the
standing authorization.

**Prompt J1 (undo family, `docs/undo-family-journeys`):** origin:
issues #1146 #1154 #1155 #1156 #1226 #1227 + the UF2-landed
branch-picker + the #993-pinned receipts + the N-B restore machinery.
Stories: GUI restore surfaces (#1146), ClearReminderRules
first-class (#1154 — note #1153's NotUndoable posture is adjacent,
not reopened), CLI undo-to/redo-to (#1155), deep-history browsing
(#1156), picker keyboard selection (#1227) + payload labels (#1226).
Plans: `2026-11-22-undo-family-journeys-*.md` + chunk plans for
D1i/D1ii.

**Prompt J2 (status/export polish, `docs/status-export-journeys`):**
origin: #1147 (the status command lane: kill holder / force-unlock /
restart daemon), #1148 (watch-channel live refresh), #1149 (export
csv/json preview + clipboard). Stories per surface; the B6-typed
Draining status and the #993 serde receipts are the consumed
contracts. Plans: `2026-11-22-status-export-journeys-*.md` + the D2
chunk plan.

**Prompt J3 (habit surfaces, `docs/habit-surface-journeys`):**
origin: #1199/#1201/#1202/#1208/#1196 (Today-habits interaction
polish), #1218 (tombstoned habit history entry), #1144 (per-day
heatmap cells), **#1212 — THE RULED SEMANTICS IS NORMATIVE** (blank =
anchor-time-only; the additive domain change pre-authorized; ruling
comment 7079171625), #1035 (nudge-schedule preview), #1048 (editor
mirrors onto shared flows), #1049 (kiosk belt). Plans:
`2026-11-22-habit-surface-journeys-*.md` + chunk plans for
D3a/D3b.

**Prompt J4 (misc gated, `docs/misc-gated-journeys`):** origin:
#1225 (reload-lane placement), #1234 (review walk's in-pane cursor),
#1151 (capture accidental-commit recovery — the R-4 flip's
counterpart), #1152 (kiosk placement inventory for signal lanes),
#1184 (p2: the quick-row budget saturation — a menu design decision),
#1104 (p2: labels multi-toggle dropdown — **design-only story**; the
plan records the design, implementation queues as a follow-up issue).
Plans: `2026-11-22-misc-gated-journeys-*.md` + the D4 chunk plan.

## 8. Journey-carrying implementation chunks (Phase 2)

All six follow the round-4 HF1 worked example (stage-2 "GATE CLEARED"
from the landed plans; stage-3 with both mandates; standard blocks).
Substitutions and traps:

- **D1i** (`feat/restore-clear-surfaces`, plans from J1): traps:
  (a) the receipts decode through the #993-pinned serde; (b)
  ClearReminderRules' surface composes with the reminder-undo family
  (#1153's posture adjacent, not reopened); (c) the consent/staging
  grammar follows the task-delete precedent for destructive-class
  surfaces.
- **D1ii** (`feat/undo-browse-parity`, plans from J1): traps: (a) the
  CLI undo-to/redo-to drives the same journal tree projection the
  picker uses (one projection); (b) deep-history browsing respects
  the bounded-windows decision the plan records (#1156's ask vs the
  UI budget); (c) #1226's labels are typed projections.
- **D2** (`feat/status-export-polish`, plans from J2): traps: (a)
  the command lane's actions are typed-permission-aware (kiosk
  posture — no shell-outs where the plan forbids); (b) the
  live-refresh consumes the watch channel (the #1137/E2 seam —
  sequencing note: if E2 hasn't landed, the journey's refresh story
  degrades to the snapshot lane per the plan's recorded fallback);
  (c) the preview renders the same engine output the export writes.
- **D3a** (`feat/today-habits-interaction-polish`, plans from J3):
  traps: (a) the button redesign composes with A1's fixed selection/
  DnD grammar (sequence after A1 or rebase); (b) the per-action
  shortcuts register per the O-8 case law; (c) #1196's typed seed
  path (no silent i32 fallback).
- **D3b** (`feat/habit-editor-history-polish`, plans from J3):
  traps: (a) **#1212's ruled semantics is the contract** — blank =
  anchor-time-only, the 09:00 mint retired, the additive domain
  change pre-authorized (ruling comment 7079171625); (b) the
  tombstoned-history entry reads the retained log (#1218) without
  forging rows; (c) the heatmap cells respect the O-1 window law.
- **D4** (`feat/misc-gated-impl`, plans from J4): traps: (a) #1151's
  recovery path is the R-4 flip's counterpart (an accidental commit
  must be recoverable — undo or a confirm-stage, per the plan);
  (b) #1234's cursor arms the K4 state (no second mechanism); (c)
  #1104 is design-only — the plan records the design; implementation
  queues as a follow-up issue, not scope here.

## 9. Chunks E1–E4 (the residue tail)

**E1 — gate/journey token residue (#1181/#1186/#1164/#1217/#1236/
#1237/#1172/#1183, `test/gate-journey-token-residue`, M).** Covers:
#1181 (inline code spans defeat token parsing — strip/backtick-aware
matching), #1186 (`--root` relative path bypasses the #1073 fixture
prune — canonicalize before the scan), #1164 (the day-review plan's
prose W-1 cell — amend the plan row to a token), #1217/#1236 (the
habit/small-gated plans' prose rows — same amendment class; plans are
committed artefacts: dated erratum notes per the supersession
pattern), #1237 (the seed echo at three sites — one helper), #1172
(three journeys members' private mount preambles onto the B4a
helper), #1183 (the tombstone arm/confirm seam test). Acceptance:
the gate self-tests extended for the new matching rules (the N-A/B5
precedent); the amended plans carry dated notes; the helper
adoption loses no behavior. *(Standard pattern; bodies normative;
body references all eight.)*

**E2 — domain/storage smalls (#1211/#1157/#1158/#1137/#1095,
`fix/state-storage-small-residue`, M).** Covers: #1211 (zero-duration
snooze accepted at the CLI, refused in the GUI — unify typed;
stage-1 decides the rule — the A1 lens favors typed refusal at both
boundaries, recorded), #1157 (finish the clean_ambient re-homing
class — the remaining gated ui_requests legs), #1158
(derive_sync_states materializes rows for unbound boards — skip or
record-why), #1137 (the diagnostics hop's Mutex cell → the plan-named
watch channel), #1095 (the habit-log LWW guard's whole-second RFC
3339 strings — sub-second or typed comparison; the B1 clock-bump
rule composes). Acceptance: per-item typed fixes; the hostile-env
pass green; diffs confined to the named seams. *(Standard pattern;
bodies normative; body references all five.)*

**E3 — testing-backend residue (#1017/#1185/#1232/#1216/#1163/#1159,
`test/backend-residue`, M/L).** Covers: #1017 (habit rows rebuild
decorations ModelRc every pass — cache per the handle-identity diff),
#1185 (the doctor sweeps a fresh worktree branched off a stale local
main — fetch/origin-compare before the containment check), #1232
(the interactive connect prompts' stdin test seam), #1216 (the
headless backend drops raw key events when the modal grab doesn't
survive the pipeline), #1163 (insta snapshot filenames embed the
binary name — decouple for future consolidations), #1159 (ip16 pins
the 60s suite floor — diagnose: the wiremock 5s delays were ruled
justified-ish; why 60s? the shutdown/pull path per the R2b/R2c
landed state). Acceptance: per-item fix or armed diagnosis; #1159's
diagnosis is evidence-carried (the R2 investigation method); diffs
confined to the backend/seams. *(Standard pattern; bodies normative;
body references all six.)*

**E4 — widget anatomy remainder (#1121/#1125/#1126/#1127/#1128,
`refactor/widget-anatomy-remainder`, M).** Covers (the S8 remainder):
#1121 (the hint caption is not SectionHeader anatomy — revisit or
record), #1125 (AnchoredOverlayHost pointer-anchor + fill-to-floor —
the ActionMenu hand mount's stay), #1126 (DropdownTone → the Tone
ladder with a Tone.accent arm — one consumer, deliberate), #1127
(labels roster rows onto the form-row primitive), #1128 (MenuShell's
cap-rows knob has no false consumer — REMOVE the speculative point;
O-6 cuts both ways). Acceptance: behavior-identical (the corpus is
the oracle); the O-6 law governs. *(Standard pattern; bodies
normative; body references all five.)*

## 10. Chunk A6 — COMPRESSED (`chore/small-residue-sweep`) — detail

Dispatcher-owned per `agent_dispatch.org` §7 (the compression stated
in the committed chunk plan
`.artifacts/plans/2026-11-22-small-residue-sweep-plan.md`):

- **#1215 (ruled)**: amend
  `.artifacts/plans/2026-11-21-undo-ui-reminder-undo-plan.md`
  R-5/US-2-S3 to "the undo adds no delivery-addressing action; the
  action-layer prune is the standing delete contract", dated
  supersession note citing ruling comment 7079171410.
- **#776**: remove the dead `boards use` verb — verify
  unreachability (dispatch grep), remove parse/completion/receipt
  arms, typed.
- **#1160**: document the idle-box discard rule for wall-clock
  measurements (where measurement reports live + the law text).
- **#892 (conditional)**: verify the byte-identical constraint lifted
  (R4 renamed binaries — check the #682 comment's reference);
  update the cross-reference or record-and-defer on the issue.
- **#907**: add the shellcheck gate small — CI step or pre-commit
  hook over `scripts/*.sh`; triage the current failure list
  fix-or-waive per file with recorded rationale.
- **#1103/#867**: operator notes posted (the unregistered
  `taskboard-WT-feat-reflection-domain` directory — verify it is not
  a registered worktree first; the hosted Deck curl matrix still
  pending).

Acceptance: each item's evidence on its issue; the PR diff confined
to the named files; pre-commit hooks green (the shellcheck addition
self-applies); #1215's amendment is the ruling's verbatim recording.

## 11. Dispatcher runbook (binding)

Per `docs/agent_dispatch.org`: stage-exit verification between stages
(resume over redo, playbook §8); the full §10 suite between stage-2
landing and stage-3 ship of every code chunk; CI watch +
`mergeStateStatus CLEAN` + absolute-path teardown after each PR-open;
≤3 concurrent chunks, staggered ≥3 min; expected-heavy builds through
`scripts/memory-gate.sh`; the J1–J4 gate is one sitting (per-plan
accept/reject; rejects resume their J agent with the operator's
notes); A2's verdict checkpoint verified by the dispatcher before
A2.2 dispatches.

## 12. At-merge closures & bookkeeping (dispatcher-owned)

- A1 → #1198, #1200; A2 → #1205, #1203, #1204, #1207; A3 → #1197;
  A4 → #1206; A5 → #1150; A6 → #1215 (the amendment closes it),
  #776, #1160, #892 (or record-and-defer), #907 (#1103/#867 stay
  operator notes); B1 → #1164–#1170, #1214 (per-issue evidence);
  C1 → #1231; C2 → #1229, #1230, #1175, #1176; D1i → #1146, #1154;
  D1ii → #1155, #1156, #1226, #1227; D2 → #1147, #1148, #1149; D3a
  → #1199, #1201, #1202, #1208, #1196; D3b → #1218, #1144, #1212,
  #1035, #1048, #1049; D4 → #1225, #1234, #1151, #1152, #1184
  (#1104 closes at design acceptance, or stays open with the design
  linked — per the J4 plan's recorded disposition); E1 → #1181,
  #1186, #1164, #1217, #1236, #1237, #1172, #1183; E2 → #1211,
  #1157, #1158, #1137, #1095; E3 → #1017, #1185, #1232, #1216,
  #1163, #1159 (or armed diagnosis); E4 → #1121, #1125, #1126,
  #1127, #1128.

**Deliberately not dispatched here** (scope report §F): #1094 (CRDT
design round — separate), the watch sets (#988/#983/#987, #975/#972/
#977/#971, #1021, #1052, #919, #980/#357, #1079), the flaky armed
standbys and no-recurrence watchlist (#697/#650/#968/#336/#287/#384/
#883/#1020/#926), the perf family (#131–#157 set), feature-scale
items (#122/#121/#127/#181/#184/#185/#191/#193/#195/#198/#213/
#215–#219/#224/#225/#444/#440/#494/#117/#639–#642/#680/#691/#700/
#701/#715/#716/#723/#762/#769/#770/#775/#778/#782), C-6 deferrals
(#807/#809/#812/#813/#819/#820), #113/#114/#115, #938/#912/#875/#870
(standing small-deferrals).

**What opens next**: round 6 (whatever this round defers + the
#1094 CRDT design round + the next user-review batch); the chrono
ruling follow-ups per #988; #867's hosted evidence session when the
operator runs it.
