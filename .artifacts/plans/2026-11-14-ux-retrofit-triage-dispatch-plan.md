# [Plan] UX Journey Framework — Phase 4 (Retrofit Analysis & Triage) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-14
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the two chunks of the framework's rollout
  phase 4: per-feature UX journey plans retrofitted for every existing UX
  feature, then the consolidated triage report over all findings (the
  phase-5 remediation input). Analysis artefacts only — no application
  code changes in either chunk.
- **Origin**: the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md` §6
  phase 4 — normative — and §4.1 (template bar), §4.4 (walkthrough
  protocol as the analysis instrument); `docs/ux_planning.org` (the
  installed law); the completed census
  `docs/ux-surface-census.org` (phase-3 output, *full census complete*,
  2026-11-14); the stock-take report
  `.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md`
  — **§3, the 121-entry unruled-observation inventory, is the mandatory
  first input of this phase**. The three pilot UX plans (habit create,
  recur editor, files roster — `.artifacts/plans/2026-11-13-r7-*-ux-plan.md`)
  already exist and are retrofits-complete: they are referenced, never
  duplicated. The user's 2026-11-14 instruction to prepare phase-4
  dispatch prompts.
- **Status**: READY — phase 3 is merged on `origin/main`; no machine
  gates. One **human gate**: the triage report requires the user's
  acceptance before phase 5 (remediation dispatch) may be written — the
  gate is at report acceptance, not at chunk-2's PR.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 — task-flow-core retrofits | framework §6 phase 4; census slices Board/Today/Capture/TaskDetail | `docs/ux-retrofit-core` | the feature registry (all features enumerated, owned slices named); retrofit UX plans for the task-flow-core features (§2 list); the scoping-ruling census updates those plans record. Non-goals: auxiliary features, the triage report, any code | L |
| 2 — auxiliary retrofits + triage report | framework §6 phase 4; census slices Settings/Review/AlarmWindow + the §3 inventory | `docs/ux-retrofit-triage` | retrofit UX plans for the auxiliary features (§3 list); the triage rubric; the consolidated triage report (BLOCKER/MAJOR/MINOR, everything homed); backlog issues per finding; the census's status note. Non-goals: remediation, mandate flip, any code | L |

**Dependency graph**: serial 1 → 2 (chunk 2's triage report consumes
chunk 1's retrofits; both update the same census file, so chunk 2
branches fresh off the post-chunk-1 `main` — never stacked). No gating
on any feature branch. The **human gate** sits after chunk 2's PR merges
and the user has read the triage report: phase 5 is dispatched only on
the user's acceptance.

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

**The retrofit posture (both chunks, all stages — binding):** phase 4
**describes what exists** — the mirror image of phase 2's blind
authoring. Retrofit plans read the census slices (evidence-anchored),
the code at the slice baselines, and the live `main`; they document
journeys **as implemented**, including their escape paths where present.
Where the description exposes a hole, the plan records it as a
**finding** (§2.4 of the template's verification plan becomes a findings
section); it never fixes, never prescribes redesign. Two rules follow:

1. **Rulings vs findings — the boundary a retrofit plan may cross:**
   a retrofit UX plan MAY record a ruling (and update its census slice
   cells accordingly) only for scoping decisions the surface grammar or
   a prior artefact already justifies — the plan itself then becomes the
   ruling's authority, dated, with the prior evidence cited. A gap no
   authority justifies — a journey that cannot complete, a display
   without control, an escape-less step, a control without feedback —
   is a **finding**, never dressed up as a ruling. The stage-1 agents
   settle the concrete rubric (§3 of this dispatch is the minimum it
   must contain); the census keeps its `U` markers for every cell a
   retrofit plan does not convert to either a ruling or a finding.
2. **Mechanical validation**: every retrofit plan runs
   `scripts/ux-coverage.sh <plan>` (plain, informational). Because the
   plan documents existing wiring, the expected steady state is **FOUND
   for every wiring row and journey step** — a MISSING row means the
   plan misdescribes reality (wrong callback name, unmounted step), a
   plan defect to fix, not a product gap. This makes the gate the
   retrofit's truth-check: the plan and the code must agree.

## 2. Chunk 1 — task-flow-core retrofits (`docs/ux-retrofit-core`)

- **Covers**: (1) the **feature registry** — the complete enumeration of
  the app's existing UX features with, per feature: its census slice(s),
  its capability rows, its existing UX plan if any (the three pilot
  plans), and its retrofit chunk assignment; committed as a section of
  the chunk plan and referenced by chunk 2; (2) retrofit UX plans —
  template-conformant, journeys-as-implemented — for the task-flow-core
  features:
  - **Board & stack management** (board/stack create, rename, delete,
    move, resize, select, color, archive, presets — census Board slice),
  - **Task lifecycle** (create, complete, open/detail, move, delete,
    location/badge/due display — Board + Today rows),
  - **Task editing** (title, description, due, scheduled, labels,
    trackers, priority — TaskDetail slice),
  - **Capture & search** (capture overlay, search palette, nav.item_walk
    — Capture slice + the nav row);
  (3) the census slice updates each plan records (scoping rulings
  converted from `U` markers, per the retrofit posture's rule 1).
  Non-goals: auxiliary features (chunk 2); the triage report (chunk 2);
  findings homing beyond what the plans' findings sections record; any
  code.
- **Acceptance**: the registry complete (every census capability row
  belongs to exactly one feature); four+ retrofit plans
  template-conformant with journeys-as-implemented; per plan: all
  wiring rows FOUND under `scripts/ux-coverage.sh` (or the discrepancy
  recorded as a plan correction applied); findings sections populated
  where holes exist; census `U` markers converted only per the rubric;
  gates green; PR open + green + `CLEAN`.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the UX retrofit & triage phase ("task-flow-core retrofits", branch docs/ux-retrofit-core) in the taskboard repo at /home/viktoria/src/taskboard. Verify the precondition: the phase-3 census (docs/ux-surface-census.org, status *full census complete*) and the stock-take report (.artifacts/reports/2026-11-14-ux-surface-census-stocktake-report.md) are merged into origin/main — if not, STOP and report instead of planning.

1. Run scripts/worktree-doctor.sh from the project root (nonzero exit = STOP and report), then create the worktree: git worktree add -b docs/ux-retrofit-core ../taskboard-WT-docs-ux-retrofit-core (never without -b, never on main).
2. Read AGENTS.md §7/§8, docs/ux_planning.org, the framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md §4.1/§4.4/§6 phase 4, this dispatch plan .artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md §1/§2 (the retrofit posture binds you), the completed census docs/ux-surface-census.org, the stock-take report (§3 is the 121-entry unruled inventory — chunk 1 works only the rows whose surface is Board/Today/Capture/TaskDetail), the three pilot UX plans (.artifacts/plans/2026-11-13-r7-*-ux-plan.md — your format exemplars AND the already-retrofitted features you must not duplicate), and .artifacts/templates/ux-journey-plan.md.
3. Derive the feature registry (read-only): enumerate every existing UX feature of the app from the census's seven slices + CLI register, assign each to exactly one retrofit plan (the four task-flow-core features of dispatch §2, chunk 2's auxiliary list per its §3, or "already retrofitted — pilot plan X"), and note per feature whether any capability row of its slice falls outside any feature (an orphan — a registry finding).
4. Write the TDD-first plan at .artifacts/plans/<today>-ux-retrofit-core-chunk-plan.md (house [Plan] style). Settle at minimum, each as a recorded decision:
   - The feature registry (the full table; orphan resolution).
   - The scoping-ruling rubric for converting U markers to rulings (dispatch §1 rule 1 is the floor — make it concrete: which census evidence patterns justify a scoping ruling, e.g. "the verb's context binding names this surface exclusively" vs which patterns demand a finding, e.g. "the only entry point is the CLI and the verb mutates user data").
   - Per retrofit plan: the story set (journeys-as-implemented, P1 = the journey a user actually must complete today), which census rows feed its matrix, the escape-path audit method for existing journeys (does cancel/undo/typed-error exist per step? cite seam or test), and the walkthrough questions you will apply at implementation time to surface convenience holes (framework §4.4's four questions, applied descriptively).
   - The gate steady-state rule (dispatch §1 rule 2): what a MISSING row means here and how a plan correction is recorded.
   - Commit sequencing (one plan per commit; census updates ride their plan's commit).
5. Resolve every open question with a recorded decision; deviations from the origins need a recorded reason.
6. Commit as: docs(plan): the ux retrofit chunk-1 plan. Do not implement; do not push.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers in your report.

Report: plan path, the registry headline (feature count, orphans, already-retrofitted), the rubric's shape, decisions, risks.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the UX retrofit & triage phase ("task-flow-core retrofits", branch docs/ux-retrofit-core). Work in /home/viktoria/src/taskboard-WT-docs-ux-retrofit-core (recreate per dispatch §1 mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.1/§4.4/§6 phase 4, this dispatch plan §1/§2 (the retrofit posture binds you: describe what exists — never fix, never prescribe redesign), and the stage-1 plan .artifacts/plans/<today>-ux-retrofit-core-chunk-plan.md (binding — the registry and rubric govern). Deviations need a recorded reason in your report.
2. Execute per the stage-1 plan, one standalone commit per retrofit plan (its census slice updates ride the same commit):
   a. .artifacts/plans/<today>-board-stack-management-ux-plan.md — from the census Board slice + the live code; journeys-as-implemented for board/stack create, rename, delete, move, resize, select, color, archive, presets; template-conformant (matrix, stories with entry points, journey specs with the escape-path audit column, mermaid flows, wiring inventory, command delta, findings section).
   b. .artifacts/plans/<today>-task-lifecycle-ux-plan.md — create, complete, open/detail, move, delete, display rows (Board + Today).
   c. .artifacts/plans/<today>-task-editing-ux-plan.md — title, description, due, scheduled, labels, trackers, priority (TaskDetail).
   d. .artifacts/plans/<today>-capture-search-ux-plan.md — capture overlay, search palette, item walk (Capture + nav).
   Each plan: retrofit posture — the wiring inventory cites the EXISTING seams (callbacks, ActionIds, command variants, ItemAddr kinds) and existing tests; the escape-path audit names the seam or test per step or records the escape-less step as a finding; the findings section lists the holes the walkthrough questions surfaced (descriptive: what is missing or inconvenient, with evidence — no redesign proposals, those are phase 5's).
   e. Census updates: convert U markers to rulings only per the stage-1 rubric (the plan is the authority, dated); leave genuine gaps as U markers or record them in the plan's findings section; every conversion cites the plan.
3. Validate mechanically: scripts/ux-coverage.sh <plan> per plan (plain, informational). Expected steady state: every wiring row and journey step FOUND. A MISSING row is a plan-vs-code disagreement — fix the plan (record the correction) unless it exposes a real unmounted step, which is a finding. Record the per-plan gate outcome as evidence.
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock) and make it green. Docs-only diff; a pre-existing unrelated failure means STOP and report.
5. Every commit standalone. No push, no PR — shipping is stage 3's. Never push to or merge main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers.

Report: per-gate results (per retrofit plan: FOUND/MISSING counts), the findings headline per plan, the U-marker conversions made, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the UX retrofit & triage phase (branch docs/ux-retrofit-core). Work in /home/viktoria/src/taskboard-WT-docs-ux-retrofit-core (recreate per dispatch §1 mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.1/§4.4/§6 phase 4, this dispatch plan §1/§2 (the evidence & artefact mandate and the retrofit posture apply to you), the stage-1 plan, the three pilot UX plans (format exemplars), the census as it was on origin/main (git show origin/main:docs/ux-surface-census.org), and the stock-take report §3. Review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance (fetch --prune; rebase onto origin/main; push --force-with-lease only if rewritten).
2. Functional focus (this chunk's traps):
   - Retrofit posture held: no plan prescribes redesign or fixes; findings are descriptive with evidence; no code changes ride the PR.
   - Ruling-vs-finding boundary: re-judge every U-marker conversion against the stage-1 rubric yourself — a conversion whose only justification is "it would be convenient" is a mis-ruled gap (BLOCKER: restore the U marker and move the item to the plan's findings section). Conversely, a hole the plan dressed as a finding when the rubric justifies a scoping ruling is a plan defect.
   - Plan-vs-code truth: re-run scripts/ux-coverage.sh per plan; spot-check at least eight FOUND anchors across the four plans against the real seams (the cited callback exists in the .slint file and the adapter; the cited test exists and matches the step). A plan describing wiring that does not exist is a BLOCKER.
   - Escape-path audit honesty: pick one journey per plan and audit the escape column against the code yourself — a step the plan claims has cancel/undo where the code has none is a BLOCKER.
   - Registry completeness: every Board/Today/Capture/TaskDetail capability row belongs to exactly one feature plan; no orphan rows; the pilot features are referenced, not duplicated.
   - Census integrity: conversions cite their plans; no seed/phase-3 ruling was altered or dropped; the diff touches only the census and the five plan files.
3. Evidence & artefact mandate: every claim carries evidence; the diff is exactly the chunk's artefacts; out-of-scope ideas → backlog issues.
4. Re-run scripts/ux-coverage.sh per plan and the full gate suite via scripts/run-quality-gates.sh (wait for the flock); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: retrofit the task-flow-core UX journey plans"; body citing framework §6 phase 4, the registry, and the findings headline), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-retrofit-core; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 3. Chunk 2 — auxiliary retrofits + triage report (`docs/ux-retrofit-triage`)

- **Covers**: (1) retrofit UX plans for the auxiliary features:
  - **Habit family** (log, repair, reorder, convert, state display,
    pause/resume/convert management — Today slice + the pilot
    habit-create plan referenced),
  - **Recurrence family** (the pilot recur-editor plan referenced;
    retrofit covers the *management* verbs — pause, resume, convert —
    and completion-spawn behavior as-displayed),
  - **Timers & alarms** (Today timers, alarm window, notifications —
    Today + AlarmWindow slices),
  - **Review walk & reflection** (review view, walk overlay,
    plan-next-day, reflection edit/delete — Review slice),
  - **Settings & connections** (accounts, Deck connectors, files sync,
    knobs — Settings slice; the pilot files-roster plan referenced; the
    user's named worst offender — the walkthrough questions apply with
    extra care here);
  (2) the **triage rubric** settled at stage 1 (dispatch §4 below is the
  floor); (3) the **consolidated triage report**
  `.artifacts/reports/<today>-ux-retrofit-triage-report.md`: every
  finding from all retrofit plans (chunk 1's + chunk 2's) plus the
  still-unmarked `U` cells of the §3 inventory, classified BLOCKER /
  MAJOR / MINOR, each with evidence, the affected journeys, and a home
  (backlog issue; P1/P2/P3 per the rubric); journeys that cannot
  complete end-to-end named explicitly; the Settings surfaces expected
  to dominate — the report says so if they do and says so if they do
  not; (4) backlog issues per finding (AGENTS §7); (5) the census
  status note updated (*phase 4 complete; triage accepted ⇒ phase 5*).
  Non-goals: remediation designs (phase 5 consumes the report), the
  mandate flip (phase 6), any code.
- **Acceptance**: five auxiliary retrofit plans template-conformant with
  the gate steady state (FOUND or recorded correction); the triage
  report complete — every finding and every unconverted `U` cell from
  the whole census appears exactly once with a classification and a
  home; every finding has its backlog issue; the human gate (user
  acceptance) is recorded as the report's next step; gates green; PR
  open + green + `CLEAN`.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the UX retrofit & triage phase ("auxiliary retrofits + triage report", branch docs/ux-retrofit-triage). Verify the precondition: chunk 1's PR (the feature registry + four task-flow-core retrofits) is merged into origin/main — if not, STOP and report instead of planning.

1. Run scripts/worktree-doctor.sh from the project root (nonzero = STOP), then: git worktree add -b docs/ux-retrofit-triage ../taskboard-WT-docs-ux-retrofit-triage (fresh branch off post-chunk-1 main — never stacked, never on main).
2. Read AGENTS.md §7/§8, docs/ux_planning.org, the framework plan §4.4/§6 phase 4, this dispatch plan §1/§3 (the retrofit posture binds you), chunk 1's plan (registry + rubric — apply mechanically), the four merged retrofit plans (format exemplars + findings sections you will consume), the three pilot UX plans, the completed census, the stock-take report §3 (the 121-entry inventory — chunk 2 owns the Settings/Review/AlarmWindow rows plus every row chunk 1 did not convert), and .artifacts/templates/ux-journey-plan.md.
3. Write the TDD-first plan at .artifacts/plans/<today>-ux-retrofit-triage-chunk-plan.md. Settle at minimum, each as a recorded decision:
   - The five auxiliary retrofit plans' story sets and census-row feeds (dispatch §3 list; the pilot plans referenced where a feature is partially retrofitted).
   - The triage rubric — the classification floor (extend, never weaken): BLOCKER = a journey cannot complete end-to-end, or a step has no escape path where user data is at risk, or display-without-control on a core verb (create/edit/remove of a primary entity); MAJOR = a heuristic violation with real friction (no undo where the sibling verbs have it, feedback absent after dispatch, entry-point asymmetry click-vs-key), or a core verb reachable only via CLI without a recorded justification; MINOR = polish (label semantics, ordering, discoverability). Each classification carries evidence and affects named journeys.
   - The consolidation method: how every finding from the six retrofit plans + the three pilots' open items + every still-unconverted U cell of the 121-entry inventory is enumerated exactly once (dedup keys: surface + capability + evidence), and the P1/P2/P3 homing rule per severity.
   - The report outline (machine-consumable by phase 5: one table per severity, columns = finding id, surfaces, capabilities, journeys affected, evidence, home issue).
   - Commit sequencing.
4. Resolve every open question with a recorded decision; deviations need a recorded reason.
5. Commit as: docs(plan): the ux retrofit-triage chunk-2 plan. Do not implement; do not push.
6. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.

Report: plan path, decisions, risks.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the UX retrofit & triage phase ("auxiliary retrofits + triage report", branch docs/ux-retrofit-triage). Work in /home/viktoria/src/taskboard-WT-docs-ux-retrofit-triage (recreate per dispatch §1 mechanics if missing — doctor first, never on main; never enter another agent's worktree).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6 phase 4, this dispatch plan §1/§3 (the retrofit posture binds you: describe, never fix), chunk 1's plan (registry + rubric), and the stage-1 plan .artifacts/plans/<today>-ux-retrofit-triage-chunk-plan.md (binding). Deviations need a recorded reason in your report.
2. Execute per the stage-1 plan, one standalone commit per retrofit plan (census updates ride their plan's commit):
   a-e. .artifacts/plans/<today>-{habit-family,recurrence-family,timers-alarms,review-walk-reflection,settings-connections}-ux-plan.md — journeys-as-implemented per the stage-1 story sets; existing seams cited; escape-path audits; findings sections populated via the four walkthrough questions (extra care on settings-connections — the user's named worst offender; describe the actual friction, cite seams).
   f. Census updates per the chunk-1 rubric (conversions cite their plans; genuine gaps stay U or become findings).
3. Validate mechanically: scripts/ux-coverage.sh <plan> per plan — steady state FOUND or recorded plan correction; record outcomes as evidence.
4. Write the consolidated triage report .artifacts/reports/<today>-ux-retrofit-triage-report.md per the stage-1 outline: enumerate every finding (six retrofit plans' findings sections + pilot plans' open items + every still-unconverted U cell of the 121-entry inventory) exactly once via the dedup keys; classify per the rubric; home every finding as a backlog issue (label ui + p1/p2/p3; body = summary + permalink to the report section — never the whole table); name the journeys that cannot complete end-to-end explicitly; report whether the Settings surfaces dominate as expected; end with the human-gate note (user acceptance gates phase 5).
5. Run the full gate suite via scripts/run-quality-gates.sh from the project root (wait for the flock) and make it green. Docs-only diff; a pre-existing unrelated failure means STOP and report.
6. Every commit standalone. No push, no PR — shipping is stage 3's. Never push to or merge main.
7. Backlog issues: open/comment autonomously for out-of-scope findings (beyond the triage homing itself); list the issue numbers.

Report: per-gate results, the triage headline (BLOCKER/MAJOR/MINOR counts, issues filed), whether Settings dominated, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the UX retrofit & triage phase (branch docs/ux-retrofit-triage). Work in /home/viktoria/src/taskboard-WT-docs-ux-retrofit-triage (recreate per dispatch §1 mechanics if missing).

1. Read AGENTS.md, docs/ux_planning.org, the framework plan §4.4/§6 phase 4, this dispatch plan §1/§3 (the evidence & artefact mandate and the retrofit posture apply), the stage-1 plan, chunk 1's plan and merged plans, the pilot plans, the pre-branch census (git show origin/main:docs/ux-surface-census.org), and the stock-take report §3. Review the full branch diff (git diff origin/main...HEAD) after the standard rebase dance.
2. Functional focus (this chunk's traps):
   - Triage completeness (the report is phase 5's origin — incompleteness here poisons remediation): reconcile the finding enumeration yourself — every findings-section item of the six retrofit plans, every pilot open item, and every U cell of the 121-entry inventory that chunk 1 did not convert appears exactly once in the report (spot-check by re-deriving the U-cell list for two slices from the census and matching against the report).
   - Classification honesty: re-classify at least five findings yourself against the rubric, including one BLOCKER and one you expect to be borderline; a finding inflated or deflated against the rubric's floor is a BLOCKER (the rubric is the floor — never weaker).
   - Homing integrity: every finding's backlog issue exists (gh issue view), carries the right labels, and its body permalinks the report section — not the whole table.
   - Retrofit posture: no remediation designs in the report (findings + evidence + homes only; phase 5 designs the fixes); no code changes; the settings-connections walkthrough friction is described with seams cited.
   - Census integrity: conversions cite their plans; no prior ruling altered; the status note updated.
3. Evidence & artefact mandate: every claim carries evidence; the diff is exactly the chunk's artefacts (five plans + census + report + chunk plan); out-of-scope ideas → backlog issues.
4. Re-run scripts/ux-coverage.sh per plan and the full gate suite via scripts/run-quality-gates.sh (wait for the flock); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title "docs: retrofit the auxiliary UX journey plans; the triage report"; body citing framework §6 phase 4 and the triage headline), gh pr checks --watch to green, then mergeStateStatus CLEAN (rebase-dance if BEHIND/DIRTY). Never merge; the user merges on GitHub.
7. Backlog issues: open/comment autonomously for out-of-scope findings; list the issue numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-retrofit-triage; confirm via git worktree list.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN + worktree-removed confirmations.
```

## 4. Completion bookkeeping

- Phase 4 closes when chunk 2's PR merges **and the user has accepted the
  triage report** — the human gate the framework §6 phase 4→5 transition
  requires. The report's P1/P2/P3 homing is the dispatch input for phase
  5; the user's acceptance (or their edits to it) is the recorded
  decision.
- **Deliberately not dispatched here**: remediation (phase 5 — dispatched
  from the accepted triage report), the mandate flip (phase 6), any code
  changes, any re-derivation of pilot plans.
- **What opens next**: phase 5 (remediation dispatch) — chunked from the
  accepted triage report's severity tables once the user accepts it.
