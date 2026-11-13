# [Plan] UX Journey Framework — Phase 2 (Round-7 Pilot) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-13
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the two chunks of the framework's rollout
  phase 2 (§6 of the framework plan): the **blind** authoring of the three
  round-7 UX journey plans, then the **comparison** against the
  already-implemented chunks 1–2 plus the round-7 plan adjustments and the
  census seed. No application code in either chunk.
- **Origin**: `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  (ACCEPTED; landed with PR #761) — normative: §6 phase 2, **§6.1 pilot
  discipline (blind-first, strict ordering)**, §4.1 (template bar), §4.3
  (gate), §4.4 (walkthrough protocol, census); plus
  `.artifacts/plans/2026-11-13-gui-review-round7-plan.md` §2 (D-7.1/D-7.2/
  D-7.3 — the design origin of the three UX plans) and
  `docs/ux_planning.org` (the installed law). The user's 2026-11-13
  instruction to generate phase-2 dispatch prompts.
- **Status**: READY once PR #761 is merged (chunk 1 needs the installed
  template + gate on `main`). Chunk 2 additionally gated on round-7
  chunk 2's state (below).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 — blind UX plans | §6.1 step 1; framework §4.1; round-7 plan §2 | `docs/ux-r7-pilot-plans` | three UX journey plans (habit create, recur editor, files roster) authored **blind** from the round-7 design; validation assertion vs pre-round-7 `main`. Non-goals: no comparison, no census, no code, no reading any ui-r7 implementation | M |
| 2 — comparison + adjustments + census seed | §6.1 steps 2–4; §4.3–4.4 | `docs/ux-r7-pilot-comparison` | `ux-coverage.sh` + walkthrough over chunks 1–2 vs the new plans; the comparison plan with verdict table; amendments to the round-7 dispatch plan (chunk-3 prompts re-authored, re-work chunks appended); superseding stage-1 plans where re-works touch them; `docs/ux-surface-census.org` seeded with the three pilot slices. Non-goals: implementing any re-work, no full census | M–L |

**Dependency graph**: chunk 1 gated on PR #761 merged; chunk 2 gated on
chunk 1's PR merged **and** round-7 chunk 2 (recur editor) either landed
on `origin/main` or pushed with an open PR (compare at a fixed SHA,
re-verify post-merge). Serial: 1 → 2. The re-work chunks and the
re-authored round-7 chunk 3 that chunk 2 prescribes are **dispatched later
from the amended round-7 dispatch plan** — deliberately not from this
document (their shape depends on the comparison verdicts).

**Dispatch mechanics** (binding): one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); each chunk works in its own worktree
`../taskboard-WT-<branch>` next to the project root (AGENTS §8: doctor
first, `-b` branch creation, never on `main`, never reusing a landed
branch name, stage 3 removes the worktree). The round-7 chunk-2 worktree
(`taskboard-WT-feat-ui-r7-recur-editor`) belongs to another agent round —
never enter it. Heavy gates go through `scripts/run-quality-gates.sh`.

**Shared execution model**: as the house dispatch template prescribes —
stage 1 writes the TDD-first chunk plan (`docs(plan): …`) with recorded
decisions; stage 2 executes it with every commit standalone and the full
AGENTS §10 suite green via the wrapper; stage 3 rebases onto
`origin/main` first, reviews the full branch diff against the chunk's
acceptance + the evidence & artefact mandate, re-runs gates, fixes every
finding on the branch, ships (push, `gh pr create`, `gh pr checks
--watch` to green, `mergeStateStatus` `CLEAN`, rebase-dance if
`BEHIND`/`DIRTY`), then tears the worktree down (absolute anchor +
`git worktree list` confirmation). Handoff = PR open + checks green +
`CLEAN` + worktree removed; never merge, never touch `main`. Every stage:
out-of-scope findings become `backlog:` issues immediately (AGENTS §7
standing authorization).

**Blindness constraint (chunk 1, all three stages — the pilot's integrity
depends on it)**: while authoring and reviewing the three UX plans, agents
read the round-7 **stage-1 plan** (design) and the repo as of the **last
`main` commit before the first ui-r7 implementation chunk merged**
(expected `35e6b0a1`; the stage-1 agent verifies via `git log origin/main`
and records the SHA it used). Reading surfaces means `git show <sha>:<file>`.
Forbidden: the `feat/ui-r7-habit-create` and `feat/ui-r7-recur-editor`
branches, diffs, PRs, worktrees, and any `main` content past the baseline
SHA. Chunk 2 has no blindness constraint — comparing against the real
implementation is its job.

## 2. Chunk 1 — blind UX plans (`docs/ux-r7-pilot-plans`)

- **Covers**: three UX journey plans in the house template
  (`.artifacts/templates/ux-journey-plan.md`), one per round-7 solution:
  `<today>-r7-habit-create-ux-plan.md`, `<today>-r7-recur-editor-ux-plan.md`,
  `<today>-r7-files-roster-ux-plan.md` — each with the capability ×
  surface matrix + rulings, stories with entry points and priorities,
  journey specs with escape paths, mermaid flows, wiring inventory, and
  command-contract delta, all derived from round-7 §2 D-7.1/D-7.2/D-7.3 +
  the framework's §4.1 bar. The validation assertion (§6.1 step 1):
  evaluated against the baseline SHA, the three matrices show R7-1/2/3 as
  blank/unruled cells — recorded in each plan. Non-goals: comparison,
  census, code changes, reading ui-r7 implementations.
- **Acceptance**: three plans parse under `scripts/ux-coverage.sh <plan>`
  (informational run; expect all MISSING — that is the correct pre-state,
  and it proves the parser contract); template table headers token-exact;
  every step has an escape path; every matrix absence on a core verb
  carries a ruling; the blindness constraint verifiably held (recorded
  baseline SHA, no ui-r7 content cited as authority); gates green; PR
  open + green + `CLEAN`.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the UX journey framework phase-2 pilot ("blind UX plans", branch docs/ux-r7-pilot-plans) in the taskboard repo. Verify the precondition first: PR #761 (the framework install) must be merged into origin/main — if it is not, STOP and report instead of planning.

THE BLINDNESS CONSTRAINT BINDS YOU TOO (dispatch §1): from now until this chunk's PR ships, you read the round-7 stage-1 design (.artifacts/plans/2026-11-13-gui-review-round7-plan.md) and the repo ONLY as of the baseline SHA — the last main commit before the first ui-r7 implementation chunk merged (expected 35e6b0a1; verify via git log origin/main and record the exact SHA you settle on; read files with git show <sha>:<file>). You must NOT read the feat/ui-r7-habit-create or feat/ui-r7-recur-editor branches, diffs, PRs, worktrees, or any main content past the baseline SHA. Never enter another agent's worktree.

1. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard (nonzero = STOP), then create the worktree: git worktree add -b docs/ux-r7-pilot-plans ../taskboard-WT-docs-ux-r7-pilot-plans (never without -b, never on main).
2. Read AGENTS.md §7/§8, docs/ux_planning.org (the installed framework law), .artifacts/templates/ux-journey-plan.md, the framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md §4.1/§6.1, and this dispatch plan .artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md §1/§2.
3. Read the design origin at the baseline SHA: the round-7 stage-1 plan §2 (D-7.1/D-7.2/D-7.3), and the surfaces each solution touches (git show <sha>:crates/taskboard-ui-slint/ui/compositions/habits.slint, .../views/task-view.slint, .../views/settings-view.slint, plus src/actions.rs, src/requests.rs, crates/taskboard-domain/src/messages.rs as needed for inventory anchors).
4. Write the TDD-first chunk plan at .artifacts/plans/<today>-ux-r7-pilot-plans-chunk-plan.md (house [Plan] style). Settle at minimum, each as a recorded decision:
   - The story/step granularity per solution (aim: every P1 journey completable end-to-end including escapes; creation verbs, entry points — click AND keyboard ActionId per the click≡key doctrine — and the kiosk gating posture per story).
   - Per plan: the capability × surface matrix rows (baseline state!), the rulings you must record (e.g. at-create recurrence exclusion, single-lane #723 posture, #755/#756 known edges), the mermaid flow shapes (flowchart per journey; stateDiagram-v2 for the recur-editor overlay machine), and the wiring-inventory rows keyed on callback/ActionId/command names the round-7 design already names (SeriesEdit, HabitCreate, create-asked, StopBoardFilesSync, PublishBoardToFiles, etc.) — names you cannot know blind are recorded as plan assumptions.
   - How the validation assertion is recorded (each plan's matrix, evaluated at the baseline SHA, shows R7-1/2/3 as blank/unruled cells).
   - Commit sequencing (three plans = standalone commits).
5. Resolve every open question with a recorded decision; deviations from the origins need a recorded reason.
6. Commit as: docs(plan): the ux-r7 pilot blind-plans chunk plan. Do not implement; do not push.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers in your report.

Report: plan path, the baseline SHA you verified, decisions, risks.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the UX journey framework phase-2 pilot ("blind UX plans", branch docs/ux-r7-pilot-plans). Work in /home/viktoria/src/taskboard-WT-docs-ux-r7-pilot-plans (recreate per dispatch §1 mechanics if missing — doctor first, never on main).

THE BLINDNESS CONSTRAINT BINDS YOU (dispatch §1, verified baseline SHA recorded in the stage-1 plan): author from the round-7 stage-1 design + the baseline-SHA repo view (git show <sha>:<file>) only. Do NOT read the ui-r7 implementation branches, diffs, PRs, worktrees, or post-baseline main. Never enter another agent's worktree.

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.1/§6.1, this chunk's dispatch §2, and the stage-1 plan .artifacts/plans/<today>-ux-r7-pilot-plans-chunk-plan.md (binding). Deviations need a recorded reason in your report.
2. Author the three UX journey plans from .artifacts/templates/ux-journey-plan.md, one standalone commit each (docs(plan): …):
   a. .artifacts/plans/<today>-r7-habit-create-ux-plan.md (D-7.2)
   b. .artifacts/plans/<today>-r7-recur-editor-ux-plan.md (D-7.1 — include the stateDiagram-v2 for the editor overlay and the "Stop repeating" armed-confirm path)
   c. .artifacts/plans/<today>-r7-files-roster-ux-plan.md (D-7.3 — include the badge taxonomy (ThisConnection/LocalOnly/OtherConnection) and all three per-row actions with their typed refusal paths)
   Each plan: fixed table headers token-exact per the template; capability × surface matrix with baseline-state cells and rulings for every core-verb absence; stories with entry points (mouse affordance + ActionId), platforms, priorities; journey step tables with a non-empty escape/failure cell per step; mermaid flows; wiring inventory (names the design provides; unknowns as recorded assumptions); command-contract delta; verification plan naming the journey-test tokens (us<n>_s<k>); the validation assertion section (matrix vs baseline SHA shows R7-1/2/3 blank).
3. Verify each plan parses: scripts/ux-coverage.sh <plan> (plain, informational) — expect MISSING everywhere (correct pre-state) and NO parse errors; record the observed outcome per plan as evidence in your report.
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock) and make it green. Docs-only diff; a pre-existing unrelated failure means STOP and report.
5. No push, no PR — shipping is stage 3's. Never push to or merge main.
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: per-gate results, the parse evidence per plan, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the UX journey framework phase-2 pilot (branch docs/ux-r7-pilot-plans). Work in /home/viktoria/src/taskboard-WT-docs-ux-r7-pilot-plans (recreate per dispatch §1 mechanics if missing).

THE BLINDNESS CONSTRAINT BINDS YOU (dispatch §1): review the three plans against the round-7 stage-1 design + the baseline-SHA repo view + the template bar ONLY. Do NOT read the ui-r7 implementations — comparison is chunk 2's job, and peeking would invalidate the pilot. Never enter another agent's worktree.

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.1/§6.1, the dispatch plan §1/§2 (evidence & artefact mandate applies), the stage-1 plan, then review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance (fetch --prune; rebase onto origin/main; push --force-with-lease only if rewritten).
2. Functional focus (this chunk's traps):
   - Blindness held: no plan cites ui-r7 implementation code, PRs, or post-baseline main as authority; the baseline SHA is recorded and verifiable.
   - Template conformance: table headers token-exact (a silent parser mismatch is a finding); scripts/ux-coverage.sh <plan> runs clean-parse on all three (informational MISSING is correct).
   - Journey completeness: every step has an escape/failure path; every story has mouse + keyboard entry points (click≡key) and platform/priority fields; every core-verb matrix absence carries a ruling; the recur-editor plan has the stateDiagram-v2; the files-roster plan covers all three row actions plus typed refusals.
   - Validation assertion recorded per plan (R7-1/2/3 blank at the baseline SHA).
   - Consistency: the three plans' rulings do not contradict each other or docs/ux_planning.org.
3. Evidence & artefact mandate: design claims trace to the round-7 plan by section number; survey/citation hygiene; the diff is exactly the four plans (stage-1 chunk plan + three UX plans).
4. Re-run scripts/ux-coverage.sh on all three plans and the full gate suite via scripts/run-quality-gates.sh; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: the round-7 pilot UX journey plans (blind-authored)"; body citing the framework plan §6.1 and the three plans), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-r7-pilot-plans; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 3. Chunk 2 — comparison, round-7 adjustments, census seed (`docs/ux-r7-pilot-comparison`)

- **Covers** (§6.1 steps 2–4): (1) run `scripts/ux-coverage.sh --gate` per
  plan against current `origin/main` (chunk 1 = habit create, landed) and
  round-7 chunk 2 (recur editor) against merged `main` if landed, else its
  open PR branch at a fixed recorded SHA with a post-merge re-verify
  note; (2) answer the four walkthrough questions per P1 story with
  file:line/test evidence; (3) write the comparison plan
  `<today>-round7-ux-pilot-comparison-plan.md` with the per-story/per-step
  verdict table (satisfied / deviating / missing), every deviation
  resolved as plan-correction (recorded ruling, plan amended) or re-work
  item; (4) amend `2026-11-13-gui-review-round7-dispatch-plan.md` citing
  the comparison plan as authority — chunk 3's prompts re-authored under
  the framework (US/W/R IDs, stage-2 gate, stage-3 walkthrough), re-work
  chunks appended on fresh branch names; supersede any touched stage-1
  implementation plans via new stage-1 plans on the re-work branches'
  behalf; (5) create `docs/ux-surface-census.org` seeded with exactly the
  three pilot slices (Today/habits, Task detail/recurrence, Settings/files
  sync), evidence-anchored, rulings with dates and authorities.
  Non-goals: implementing any re-work; the full census (phase 3); chunk-3
  execution.
- **Acceptance**: verdict table complete for every story/step of all three
  plans; the comparison plan is the recorded authority for every amendment;
  the amended round-7 dispatch plan keeps its untouched chunks intact;
  census contains the three slices with evidence anchors and nothing more;
  gates green; PR open + green + `CLEAN`.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the UX journey framework phase-2 pilot ("comparison + adjustments + census seed", branch docs/ux-r7-pilot-comparison). Verify preconditions first: chunk 1's PR (the three UX plans) must be merged into origin/main, and round-7 chunk 2 (recur editor) must be either merged into origin/main or pushed with an open PR. If either fails, STOP and report instead of planning.

1. Run scripts/worktree-doctor.sh from the project root (nonzero = STOP), then: git worktree add -b docs/ux-r7-pilot-comparison ../taskboard-WT-docs-ux-r7-pilot-comparison.
2. Read AGENTS.md §7/§8, docs/ux_planning.org, the framework plan §4.3/§4.4/§6.1, this dispatch plan §1/§3, the three merged UX plans (.artifacts/plans/*-r7-*-ux-plan.md), and the round-7 dispatch plan .artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md (the document you will amend).
3. Write the TDD-first chunk plan at .artifacts/plans/<today>-ux-r7-pilot-comparison-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - The comparison protocol: per plan, scripts/ux-coverage.sh --gate output + the four walkthrough questions per P1 story, evidence = file:line on origin/main (or the recorded chunk-2 SHA if unmerged) and existing test names; the verdict vocabulary (satisfied / deviating / missing) and the tie-break rule (implementation and plan disagree ⇒ the design intent of round-7 §2 wins unless the UX plan amended it with a recorded ruling).
   - The chunk-2 baseline: merged SHA, or PR branch + fixed SHA + post-merge re-verify step (record which).
   - The amendment mechanics for the round-7 dispatch plan (in-place amendment citing the comparison plan as authority; chunk-3 prompt re-authoring pattern: cite US/W/R IDs, stage-2 runs ux-coverage --gate on the files-roster plan, stage-3 runs the walkthrough protocol; re-work chunks get fresh branch names like feat/ui-r7-habit-create-ux-rework) and when a stage-1 implementation plan must be superseded rather than amended.
   - The census seed structure for docs/ux-surface-census.org (org conventions; one matrix + rulings block per slice; evidence anchors per cell; dates + authorities on rulings).
   - Commit sequencing (every commit standalone).
4. Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit as: docs(plan): the ux-r7 pilot comparison chunk plan. Do not implement; do not push.
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: plan path, preconditions found (chunk-2 state + SHAs), decisions, risks.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the UX journey framework phase-2 pilot ("comparison + adjustments + census seed", branch docs/ux-r7-pilot-comparison). Work in /home/viktoria/src/taskboard-WT-docs-ux-r7-pilot-comparison (recreate per dispatch §1 mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.3/§4.4/§6.1, the dispatch plan §1/§3, the stage-1 plan .artifacts/plans/<today>-ux-r7-pilot-comparison-chunk-plan.md (binding), the three UX plans, and the round-7 dispatch plan. Deviations need a recorded reason in your report.
2. Execute per the stage-1 plan:
   a. Comparison: per UX plan, run scripts/ux-coverage.sh --gate <plan> against the recorded baselines (origin/main for habit-create; the recorded chunk-2 SHA for recur-editor; files-roster expected all-MISSING — its chunk has not run, which is the correct pre-state, record it as such, not as failure); answer the four walkthrough questions per P1 story with file:line/test evidence (drive the existing UI test driver only where the stage-1 plan says it is cheap).
   b. Comparison plan .artifacts/plans/<today>-round7-ux-pilot-comparison-plan.md (house [Plan] style): per-story/per-step verdict table; every deviation resolved as plan-correction (amend the UX plan with a recorded ruling) or re-work item (scope + suggested fresh branch); the validation-assertion outcome (did the framework prove itself?); honest note that an empty re-work list is a valid outcome.
   c. Amend .artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md per the stage-1 decisions, citing the comparison plan as authority: re-authored chunk-3 prompts under the framework; appended re-work chunk blocks (prompts included) for any chunk-1/2 gaps; untouched chunks intact.
   d. Supersede touched stage-1 implementation plans per the stage-1 decisions (new stage-1 plan text riding the comparison plan or the re-work chunk blocks — never rewriting accepted plans in place).
   e. Create docs/ux-surface-census.org: exactly the three pilot slices, evidence-anchored cells, rulings with dates + authorities, noting the full census is phase 3.
3. Verify: the amended round-7 dispatch plan still references all its original chunks; scripts/ux-coverage.sh --gate on the three UX plans post-amendment still parses (any plan amendment keeps table headers token-exact). Record observed outcomes as evidence.
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock); docs-only diff; a pre-existing unrelated failure means STOP and report.
5. No push, no PR — shipping is stage 3's. Never push to or merge main.
6. Backlog issues: open/comment autonomously for out-of-scope findings (e.g. walkthrough MINOR findings); list the issue numbers.

Report: per-gate results, the verdict-table headline counts (satisfied/deviating/missing per plan), any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the UX journey framework phase-2 pilot (branch docs/ux-r7-pilot-comparison). Work in /home/viktoria/src/taskboard-WT-docs-ux-r7-pilot-comparison (recreate per dispatch §1 mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.3/§4.4/§6.1, the dispatch plan §1/§3 (evidence & artefact mandate applies), the stage-1 plan, the three UX plans, and the round-7 dispatch plan; then review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance (fetch --prune; rebase onto origin/main; push --force-with-lease only if rewritten).
2. Functional focus (this chunk's traps):
   - Verdict integrity: every story/step of all three plans has a verdict backed by cited evidence (file:line/test) — re-spot-check at least three verdicts per plan yourself, including one you expect to be subtle (kiosk gating, escape paths, keyboard-parity entry points).
   - Files-roster is all-MISSING recorded as the correct pre-state, not a failure; if round-7 chunk 2 was compared at a PR SHA, the post-merge re-verify step is present.
   - The amended round-7 dispatch plan: chunk-3 prompts genuinely re-authored under the framework (US/W/R IDs cited; stage-2 gate; stage-3 walkthrough), re-work chunks have complete three-stage prompts on fresh branch names (never reusing landed names — AGENTS §8), untouched chunks intact, and the comparison plan is cited as authority for every change.
   - Census discipline: exactly the three slices, evidence anchors present, no phase-3 scope creep.
   - No accepted plan was rewritten in place (supersession pattern only).
3. Evidence & artefact mandate: every factual claim carries evidence; scope discipline (out-of-scope → backlog issues); the diff is exactly the chunk's artefacts.
4. Re-run scripts/ux-coverage.sh --gate on the three UX plans (parse intact post-amendment) and the full gate suite via scripts/run-quality-gates.sh; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: the round-7 UX pilot comparison, dispatch amendments, and census seed"; body citing the framework plan §6.1 and the comparison plan), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-r7-pilot-comparison; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 4. Completion bookkeeping

- Chunk 2's PR closing means phase 2 is done; annotate any open backlog
  issues with the PR URL.
- **Deliberately not dispatched here**: the re-work chunks and round-7
  chunk 3's execution (dispatched from the *amended* round-7 dispatch
  plan once its PR merges), the full census (phase 3), the retrofit
  analysis & triage (phase 4), the mandate flip (phase 6).
- **What opens next**: the amended round-7 dispatch plan's chunks
  (re-works first, then chunk 3 under the framework), then phase 3's
  census stock-take dispatch.
