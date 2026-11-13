# [Plan] UX Journey Framework — Phase 3 (Full Census Stock-Take) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-13
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the two chunks of the framework's rollout
  phase 3: the dedicated stock-take pass over **every** surface that
  completes `docs/ux-surface-census.org`, diffed against the phase-2 seed,
  plus the stock-take report. No application code in either chunk.
- **Origin**: `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  (ACCEPTED) §6 phase 3 — normative for this dispatch — and §4.4 (census
  law); `docs/ux_planning.org` (the installed law); the seeded census
  `docs/ux-surface-census.org` (phase-2 output, baseline `fb5863ed`); the
  phase-2 comparison plan
  `.artifacts/plans/2026-11-13-round7-ux-pilot-comparison-plan.md`
  (zero-deviation verdict; PC rulings now census authority). The user's
  2026-11-13 instruction to prepare phase-3 dispatch prompts.
- **Status**: READY — phase 2 is merged on `main`; no human gates.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 — census schema + task-flow surfaces | §4.4; framework §6 phase 3 | `docs/ux-census-stocktake-1` | census re-organization from feature-scoped seed slices to **surface-scoped** slices (every seed ruling preserved verbatim); the verb-inventory method; slices for Board, Task detail, Today, Capture; this dispatch plan + the phase-2 dispatch plan committed for the record. Non-goals: the remaining surfaces, the report, any code, any new rulings | L |
| 2 — remaining surfaces + consolidation + report | §4.4; framework §6 phase 3 | `docs/ux-census-stocktake-2` | slices for Review, Settings (incl. the files-sync planned-pending resolution), alarm window, CLI; cross-slice consolidation; the diff-against-seed appendix; the stock-take report under `.artifacts/reports/`; the census status flip to *full census complete*. Non-goals: any code, any UX journey plans, any remediation | M–L |

**Dependency graph**: serial 1 → 2 (both edit the same file; chunk 2
rebases on chunk 1's landed PR — never stacked, a fresh branch off the
post-merge `main`). No gating on round-7 chunk 3 (files roster): the
amended round-7 dispatch plan holds its re-authored prompts and it may be
dispatched concurrently or later. The census rule already covers this —
chunk 2 photographs whichever state it finds at its baseline SHA and
either resolves the *planned-pending* cells (chunk 3 landed) or leaves
them *planned-pending* with the standing "updates on merge" note (it has
not). Disjoint files from any `feat/` branch, so no worktree conflicts.

**Dispatch mechanics** (binding): one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); each chunk works in its own worktree
`../taskboard-WT-<branch>` next to the project root (AGENTS §8: doctor
first, `-b` branch creation, never on `main`, never reusing a landed
branch name, stage 3 removes the worktree). Never enter another agent's
worktree. Heavy gates go through `scripts/run-quality-gates.sh`.

**Shared execution model**: as the house dispatch template prescribes —
stage 1 writes the TDD-first chunk plan (`docs(plan): …`) with recorded
decisions; stage 2 executes with every commit standalone and the full
AGENTS §10 suite green via the wrapper; stage 3 rebases onto
`origin/main` first, reviews the full branch diff against the chunk's
acceptance + the evidence & artefact mandate, re-runs gates, fixes every
finding on the branch, ships (push, `gh pr create`, `gh pr checks
--watch` to green, `mergeStateStatus` `CLEAN`, rebase-dance if
`BEHIND`/`DIRTY`), then tears the worktree down (absolute anchor +
`git worktree list` confirmation). Handoff = PR open + checks green +
`CLEAN` + worktree removed; never merge, never touch `main`. Every
stage: out-of-scope findings become `backlog:` issues immediately
(AGENTS §7 standing authorization).

**The census's epistemic rule (both chunks, all stages — binding):**
phase 3 **records reality; it never invents rulings.** Every populated
cell cites `file:line` at the slice's recorded baseline SHA or an
existing test name. Every ruling carries an existing authority (a plan,
a recorded comment in code, a comparison ruling); an absence no authority
ever ruled on is recorded as an ***unruled* observation** (a finding for
phase 4's triage), never dressed up as a ruling. Out-of-scope
improvement ideas still go to `backlog:` issues per the standing
authorization — the distinction is: *gap in UX coverage* → unruled
observation in the report; *work idea outside the census's purpose* →
backlog issue.

## 2. Chunk 1 — census schema + task-flow surfaces (`docs/ux-census-stocktake-1`)

- **Covers**: (1) the schema decisions that make the census
  surface-scoped and complete-able (stage-1 settles them; §2 of the
  chunk plan); (2) slices for **Board**, **Task detail** (the full
  surface — the seeded recurrence slice merges in as one capability
  family among many), **Today** (likewise absorbing the habits slice),
  and **Capture** (capture overlay + search palette); (3) every seed
  ruling preserved verbatim with its authority; (4) this dispatch plan
  and the phase-2 dispatch plan
  (`.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md`,
  currently untracked) committed for the record. Non-goals: Review,
  Settings, alarm window, CLI slices; the report; any code; any new
  rulings; any UX journey plans.
- **Acceptance**: the four slices complete — every capability the verb
  inventory method derives for those surfaces has a cell with evidence
  or an unruled marker; the three seed slices' rulings all present and
  verbatim; the two dispatch plans ride the PR; the census status line
  updated to *phase 3 in flight: task-flow surfaces complete*; gates
  green; PR open + green + `CLEAN`.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the UX census stock-take ("census schema + task-flow surfaces", branch docs/ux-census-stocktake-1) in the taskboard repo at /home/viktoria/src/taskboard. Verify the precondition: the phase-2 census seed (docs/ux-surface-census.org) and the pilot comparison plan are merged into origin/main — if not, STOP and report instead of planning.

1. Run scripts/worktree-doctor.sh from the project root (nonzero exit = STOP and report), then create the worktree: git worktree add -b docs/ux-census-stocktake-1 ../taskboard-WT-docs-ux-census-stocktake-1 (never without -b, never on main).
2. Read AGENTS.md §7/§8, docs/ux_planning.org (the census law, §2.4), docs/ux-surface-census.org (the seed — study its evidence convention, ruling blocks, and planned-pending mechanics), the framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md §4.4/§6, this dispatch plan .artifacts/plans/2026-11-13-ux-census-stocktake-dispatch-plan.md §1/§2, and the comparison plan .artifacts/plans/2026-11-13-round7-ux-pilot-comparison-plan.md.
3. Survey the implementation seams you will inventory (read-only): crates/taskboard-domain/src/messages.rs (StateCommand variants), crates/taskboard-ui-slint/src/requests.rs (UiRequest), src/actions.rs (ActionId registry + contexts), the CLI command family (src/commands/), the view/composition mount set (ui/views/, ui/compositions/), and the adapter's install seams (crates/taskboard-ui-slint/src/adapter.rs, the install_seam! sites).
4. Write the TDD-first plan at .artifacts/plans/<today>-ux-census-stocktake-1-chunk-plan.md (house [Plan] style). Settle at minimum, each as a recorded decision:
   - The verb-inventory method: how capabilities are enumerated from the implementation (command/request/action enums + CLI verbs + display-only affordances from views/compositions and facts globals), the entity.verb naming grammar, and the mapping step from each verb to the surfaces that expose it (adapter seams as the mapping evidence).
   - The slice re-organization: feature-scoped seed slices merge into surface-scoped slices (habits → Today; recurrence → TaskDetail; files sync stays in Settings for chunk 2) with every ruling preserved verbatim, its ID stable, and its authority intact; how merged-slice provenance is noted (the seed's Origin lines survive).
   - The alarm-window representation (settled now so chunk 2 executes mechanically: its own slice block, not a matrix column — or a better scheme you can defend; record it).
   - The unruled-observation marker grammar (how a cell records "absent, no authority ever ruled" so phase 4 can query it).
   - The baseline-SHA convention: one SHA per slice block, stated in the block; how drift between slices is noted.
   - What "complete" means for this chunk's four surfaces (every derived verb for Board/TaskDetail/Today/Capture has a cell; verbs exclusive to other surfaces may stay blank rows if the schema prefers one matrix per slice — your decision, recorded).
   - Commit sequencing (schema+method first or per-slice commits — every commit standalone).
5. Resolve every open question with a recorded decision; deviations from the origins need a recorded reason.
6. Commit as: docs(plan): the ux-census stocktake chunk-1 plan. Do not implement; do not push.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers in your report.

Report: plan path, the verb-inventory headline counts (commands/requests/action ids/CLI verbs found), decisions, risks.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the UX census stock-take ("census schema + task-flow surfaces", branch docs/ux-census-stocktake-1). Work in /home/viktoria/src/taskboard-WT-docs-ux-census-stocktake-1 (recreate per dispatch §1 mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6, this dispatch plan §1/§2, and the stage-1 plan .artifacts/plans/<today>-ux-census-stocktake-1-chunk-plan.md (binding — its method and schema decisions govern). Deviations need a recorded reason in your report.
2. Execute per the stage-1 plan, remembering the census's epistemic rule (dispatch §1): phase 3 records reality and never invents rulings — every cell cites file:line at the slice's recorded baseline SHA or an existing test name; absences with no authority are unruled observations, not rulings.
   a. Commit the record first: add .artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md and .artifacts/plans/2026-11-13-ux-census-stocktake-dispatch-plan.md (docs(plan): …) so the phase-2 and phase-3 dispatch records ride this PR.
   b. Re-organize docs/ux-surface-census.org per the stage-1 schema: surface-scoped slices replace the feature-scoped seed slices; every seed ruling (R-*, PC-*, round-6, round-7 overturn) survives verbatim with stable IDs and authorities; the status line moves to *phase 3 in flight: task-flow surfaces complete*.
   c. Author the four slices — Board, Task detail, Today, Capture — from the verb-inventory method: derive the capability list from the implementation seams (StateCommand, UiRequest, ActionId, CLI verbs, display-only affordances), map each to its surfaces with adapter-seam evidence, and populate only this chunk's four slices (verbs whose exposure lies in chunk 2's surfaces appear as blank/absent cells per the schema, not as findings).
   d. Mark every absence-with-no-authority in these four slices as an unruled observation per the stage-1 grammar (these feed phase 4's triage — record, do not judge).
3. Verify: every cell in the four slices carries evidence or a marker (self-check by re-reading the diff); no ruling text was altered (diff the seed rulings against their new homes token-for-token). Record the observed checks as evidence in your report.
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock) and make it green. Docs-only diff; a pre-existing unrelated failure means STOP and report.
5. Every commit standalone (docs(ux)/docs(plan) subjects per AGENTS §8). No push, no PR — shipping is stage 3's. Never push to or merge main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers.

Report: per-gate results, the four slices' capability counts, the unruled-observation count per slice, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the UX census stock-take (branch docs/ux-census-stocktake-1). Work in /home/viktoria/src/taskboard-WT-docs-ux-census-stocktake-1 (recreate per dispatch §1 mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6, this dispatch plan §1/§2 (the evidence & artefact mandate and the census's epistemic rule apply to you), the stage-1 plan, and the seeded census as it was on origin/main before this branch (git show origin/main:docs/ux-surface-census.org). Review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance (fetch --prune; rebase onto origin/main; push --force-with-lease only if rewritten).
2. Functional focus (this chunk's traps):
   - Ruling preservation: every seed ruling (R-*, PC-*, round-6, round-7 overturn) appears verbatim with a stable ID and its authority — diff them token-for-token; a silently dropped or paraphrased ruling is a BLOCKER.
   - Evidence integrity: spot-check at least ten populated cells across the four slices at the recorded baseline SHAs (git show <sha>:<file> and the cited line range, or the cited test name via the test list); a cell whose anchor does not resolve is a BLOCKER.
   - Epistemic rule: no new rulings appear whose authority is not a real prior artefact or code comment; unruled observations use the stage-1 marker grammar and never masquerade as rulings.
   - Completeness per the stage-1 definition: re-derive the verb list for at least one of the four surfaces yourself (e.g. every ActionId in the Board/Today contexts, every StateCommand the TaskDetail view can reach) and check no capability is missing from its slice.
   - Scope discipline: exactly the census, the chunk plan, and the two dispatch-plan records in the diff; chunk 2's surfaces untouched; no code.
3. Evidence & artefact mandate: census claims carry evidence; the diff is exactly the chunk's artefacts; out-of-scope ideas → backlog issues.
4. Re-run the full gate suite via scripts/run-quality-gates.sh (wait for the flock); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: the ux surface census stock-take — schema and task-flow surfaces"; body citing framework §6 phase 3, the schema decisions, and the slice inventory), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-census-stocktake-1; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 3. Chunk 2 — remaining surfaces, consolidation, report (`docs/ux-census-stocktake-2`)

- **Covers**: (1) slices for **Review** (review view + walk overlay +
  plan-next-day), **Settings** (connections, boards, files sync — the
  seeded files slice merges in, its *planned-pending* cells resolved or
  carried per round-7 chunk 3's actual state at this chunk's baseline
  SHA), **alarm window** (per chunk 1's settled representation), and the
  **CLI** column (every GUI-exposed capability's CLI twin, plus
  CLI-only verbs recorded as such); (2) cross-slice consolidation: one
  verb referenced consistently across slices, the rulings register
  complete; (3) the diff-against-seed appendix (what phase 3 changed
  vs the phase-2 seed, and why); (4) the stock-take report at
  `.artifacts/reports/<today>-ux-surface-census-stocktake-report.md`:
  method, per-surface summaries, capability counts, the complete
  unruled-observation inventory as phase 4's input, the planned-pending
  state of the files roster; (5) the census status flip to *full census
  complete (phase 3)*. Non-goals: any code, any UX journey plans, any
  remediation, any triage judgment beyond recording.
- **Acceptance**: every surface of framework §6 phase 3's list has a
  slice; the report exists with the unruled-observation inventory; the
  status flip landed; seed diff appendix present; gates green; PR open +
  green + `CLEAN`.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the UX census stock-take ("remaining surfaces + consolidation + report", branch docs/ux-census-stocktake-2). Verify the precondition: chunk 1's PR is merged into origin/main (the census carries the four task-flow slices) — if not, STOP and report instead of planning.

1. Run scripts/worktree-doctor.sh from the project root (nonzero = STOP), then: git worktree add -b docs/ux-census-stocktake-2 ../taskboard-WT-docs-ux-census-stocktake-2 (a fresh branch off the post-chunk-1 main — never stacked, never on main).
2. Read AGENTS.md §7/§8, docs/ux_planning.org, docs/ux-surface-census.org (now carrying chunk 1's schema and four slices — you extend, never reorganize), the framework plan §4.4/§6, this dispatch plan §1/§3, and chunk 1's plan (.artifacts/plans/*ux-census-stocktake-1-chunk-plan.md) for the settled schema decisions you must apply mechanically.
3. Survey the chunk-2 surfaces' seams (read-only): ui/views/settings-view.slint + review-view.slint + review-walk-overlay.slint + plan-next-day.slint, alarm-window.slint, the CLI command family, the ConnectorInfo/SettingsSnapshot facts, and the adapter's Settings/review install sites.
4. Establish the files-roster state on the record: check whether round-7 chunk 3 (feat branch, PR, or merged commits — grep for StopBoardFilesSync/PublishBoardToFiles and the roster push in adapter.rs) has landed at your baseline SHA; record which state the Settings slice photographs and the resolution rule (landed → cells populate with evidence + the planned-pending markers retire; not landed → markers stay with the standing updates-on-merge note).
5. Write the TDD-first plan at .artifacts/plans/<today>-ux-census-stocktake-2-chunk-plan.md. Settle at minimum, each as a recorded decision: the per-slice capability derivation for the four remaining surfaces (same verb-inventory method); the CLI column's completion rule (CLI-only verbs recorded as CLI-only capabilities, GUI twins cross-referenced); the consolidation checks (no verb appears under two names; every ruling ID unique across slices); the report's outline and its unruled-observation inventory format (phase 4 consumes it machine-readably — a table with surface, capability, evidence-of-absence, suspected reason if known); commit sequencing.
6. Resolve every open question with a recorded decision; deviations need a recorded reason.
7. Commit as: docs(plan): the ux-census stocktake chunk-2 plan. Do not implement; do not push.
8. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: plan path, the files-roster state found (with SHAs), decisions, risks.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the UX census stock-take ("remaining surfaces + consolidation + report", branch docs/ux-census-stocktake-2). Work in /home/viktoria/src/taskboard-WT-docs-ux-census-stocktake-2 (recreate per dispatch §1 mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6, this dispatch plan §1/§3, chunk 1's plan (settled schema — apply mechanically), and the stage-1 plan .artifacts/plans/<today>-ux-census-stocktake-2-chunk-plan.md (binding). Deviations need a recorded reason in your report. The census's epistemic rule binds you: record reality, never invent rulings.
2. Execute per the stage-1 plan:
   a. Author the four remaining slices in docs/ux-surface-census.org — Review, Settings (merging the seeded files-sync slice; resolving or carrying planned-pending per the recorded files-roster state), alarm window (per chunk 1's representation), CLI column completion — same evidence convention, same markers.
   b. Consolidate: the cross-slice verb-name check, ruling-ID uniqueness, the diff-against-seed appendix (phase 2's seed vs the completed census: slices merged, capabilities added, rulings all preserved), and the status flip to *full census complete (phase 3)*.
   c. Write .artifacts/reports/<today>-ux-surface-census-stocktake-report.md per the stage-1 outline: method (verb inventory + mapping), per-surface summaries with capability counts, the complete unruled-observation inventory as phase 4's input, the files-roster planned-pending record, honest limits (what a census cannot see: label quality, layout, journey ergonomics — phase 4's walkthrough territory).
3. Verify: consolidation checks pass (record the commands run — e.g. the duplicate-verb and duplicate-ruling-ID greps — and their outcomes); every new cell carries evidence or a marker; the seed rulings all still present verbatim.
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock) and make it green. Docs-only diff; a pre-existing unrelated failure means STOP and report.
5. Every commit standalone. No push, no PR — shipping is stage 3's. Never push to or merge main.
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: per-gate results, the final per-surface capability counts, the unruled-observation inventory headline, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the UX census stock-take (branch docs/ux-census-stocktake-2). Work in /home/viktoria/src/taskboard-WT-docs-ux-census-stocktake-2 (recreate per dispatch §1 mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6, this dispatch plan §1/§3 (the evidence & artefact mandate and the census's epistemic rule apply), the stage-1 plan, and the pre-branch census (git show origin/main:docs/ux-surface-census.org). Review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance.
2. Functional focus (this chunk's traps):
   - Files-roster resolution integrity: the Settings slice's planned-pending cells match the recorded files-roster state at the baseline SHA — if chunk 3 landed mid-chunk, the cells cite real evidence and the markers retired; if not, the markers and the updates-on-merge note stand. Verify against the code, not the plan's claim.
   - Evidence integrity: spot-check at least ten new cells across the four slices at their baseline SHAs; a non-resolving anchor is a BLOCKER.
   - Ruling preservation and consolidation: all seed + chunk-1 rulings verbatim, IDs unique across the whole census; run the duplicate-verb and duplicate-ruling-ID checks yourself.
   - The report is phase 4's machine-consumable input: the unruled-observation inventory is complete (every marker in the census appears in the inventory and vice versa — reconcile them), each row carries surface/capability/evidence-of-absence.
   - Status flip landed; scope discipline (census + report + chunk plan only; no code, no UX plans, no triage judgments).
3. Evidence & artefact mandate: census and report claims carry evidence; the diff is exactly the chunk's artefacts; out-of-scope ideas → backlog issues.
4. Re-run the full gate suite via scripts/run-quality-gates.sh (wait for the flock); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: the ux surface census stock-take — completion and report"; body citing framework §6 phase 3, the surface inventory, and the unruled-observation headline), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-census-stocktake-2; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 4. Completion bookkeeping

- Phase 3 closes when chunk 2's PR merges: the census is complete and the
  stock-take report (with the unruled-observation inventory) is phase 4's
  origin document. Annotate any open backlog issues with the PR URL.
- **Deliberately not dispatched here**: round-7 chunk 3 and any re-works
  (already dispatched-able from the amended round-7 dispatch plan, and
  concurrent-safe with this phase), phase 4's per-feature UX plans and
  triage report, phase 5 remediation, phase 6 mandate flip.
- **What opens next**: phase 4 (retrofit analysis & triage) — dispatched
  from the framework plan §6 once this phase's report lands; its first
  input is the unruled-observation inventory this phase produces.
