# [Plan] UX Journey Planning Framework — Phase 1 (Install) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-13
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the single docs/process chunk that installs
  the accepted UX journey planning framework (rollout phase 1). No
  application code in this chunk.
- **Origin**: `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  (ACCEPTED 2026-11-13, riding this PR) — normative: §4 (the framework),
  §6 (rollout; this chunk is phase 1), §7 (changes inventory — the chunk's
  allowed diff), §8 (resolved decisions), Appendix A (UX journey plan
  template draft), Appendix B (dispatch-template amendment), Appendix C
  (gate script sketch). Plus the user's 2026-11-13 dispatch instruction.
- **Status**: READY — one chunk, dispatchable now; no human gates.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 — framework install | §7 items 1–6; App. A–C | `docs/ux-journey-planning-framework` | new UX template; dispatch-plan.md amendment; docs/ux_planning.org; AGENTS.md §7 rule; scripts/ux-coverage.sh + fixture; the framework plan committed. Non-goals: no census file (phase 3), no app-code changes, no CI workflow changes, no mandate flip (phase 6) | M |

**Dependency graph**: single chunk off `main`; nothing gated. Round-7
chunk 2 (`feat/ui-r7-recur-editor`) is in flight in another worktree —
disjoint files, but heavy gates must go through `scripts/run-quality-gates.sh`
(flock) per AGENTS §10 so the two chunk sessions never overlap full
coverage suites.

**Dispatch mechanics**: one agent session per prompt (1.1 → 1.2 → 1.3);
the worktree `../taskboard-WT-docs-ux-journey-planning-framework` on branch
`docs/ux-journey-planning-framework` already exists (created by the
dispatcher with the framework plan and this dispatch plan committed);
stages 2–3 recreate it on the branch if missing (after
`scripts/worktree-doctor.sh`), never on `main`, never reusing a landed
branch name (AGENTS §8). Stage 3 removes the worktree as its final task.

**Shared execution model** (binding):

0. **Every stage, every agent**: out-of-scope findings become `backlog:`
   GitHub issues (or comments on existing ones) the moment they are
   discovered, without asking (AGENTS §7 standing authorization).
1. **Stage 1 — planning agent**: works in the existing worktree, writes
   the TDD-first plan at
   `.artifacts/plans/2026-11-13-ux-journey-framework-install-plan.md` in
   the house `[Plan]` style, resolving every open question with a recorded
   decision, commits it (`docs(plan): …`). No implementation, no push.
   Origin sections marked normative are binding; deviations need a
   recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — doctor first; never
   `git worktree add <path> main`), executes per the approved plan, every
   commit standalone (rebase-merge hygiene), full AGENTS §10 gate suite
   green via `scripts/run-quality-gates.sh`. Does **not** push or open a
   PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main` —
   `git fetch origin --prune`, rebase onto `origin/main`, push
   `--force-with-lease` if history was rewritten — then reviews the entire
   branch diff versus the post-rebase base against the chunk's acceptance
   criteria + AGENTS rules **plus the evidence & artefact mandate below**,
   re-runs the gates, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Ships: push the branch,
   open the PR to `main` with `gh`, `gh pr checks --watch` to green, then
   `gh pr view --json mergeStateStatus` must be `CLEAN`
   (`BEHIND`/`DIRTY` ⇒ rebase, push `--force-with-lease`, repeat). Then
   teardown: `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-docs-ux-journey-planning-framework`, confirm via
   `git worktree list`. Handoff = PR open + checks green + `CLEAN` +
   worktree removed. Never merges; never touches `main`.

**Evidence & artefact mandate** (stage 3; docs chunk): every factual claim
in the new docs carries evidence or a verifiable source; the framework
plan's §3 survey citations must survive intact; repo hygiene (the fixture
is the only test data; scratch never lands; the PR diff is exactly the
chunk's artefacts); scope discipline (out-of-scope ideas → backlog issues,
not into the docs).

## 2. Chunk 1 — framework install (`docs/ux-journey-planning-framework`)

- **Covers** (normative: framework plan §7 + Appendices A–C): (1)
  `.artifacts/templates/ux-journey-plan.md` — new, from Appendix A;
  (2) `.artifacts/templates/dispatch-plan.md` — amended per Appendix B,
  every new mechanic conditional on "the origin cites a UX journey plan";
  (3) `docs/ux_planning.org` — new: framework law from §4, activation
  state machine from §6, method survey from §3 + the plan's Sources;
  (4) `AGENTS.md` §7 — UX journey plan artefact type + trigger rule worded
  advisory-with-activation-condition (mandatory at phase 6; §8 decision 1);
  (5) `scripts/ux-coverage.sh` — executable, Appendix C shape, `--gate`
  and a `--self-test` exercising a fixture plan's PASS and MISSING rows;
  (6) the framework plan file itself (status ACCEPTED, amended 2026-11-13).
  Non-goals: no census file, no app-code changes, no CI workflow changes,
  no mandate activation.
- **Acceptance**: the six diff items above and nothing else; the
  template↔script table-header contract matches token-for-token;
  `scripts/ux-coverage.sh --self-test` exits 0 and demonstrably fails on a
  deliberately-broken fixture row; AGENTS.md keeps the rule advisory with
  the phase-6 activation condition; gates green via
  `scripts/run-quality-gates.sh`; every commit standalone.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the UX journey planning framework phase-1 install ("framework install", branch docs/ux-journey-planning-framework) in the taskboard repo. Work in the existing worktree /home/viktoria/src/taskboard-WT-docs-ux-journey-planning-framework (branch docs/ux-journey-planning-framework, already created by the dispatcher with the accepted framework plan and the dispatch plan committed). Never touch main or any other worktree.

1. Read AGENTS.md, then the accepted framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md (normative: §4 framework, §6 rollout, §7 changes inventory, §8 resolved decisions, Appendix A UX template draft, Appendix B dispatch-template amendment, Appendix C gate sketch) and the dispatch plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-dispatch-plan.md §1/§2.
2. Read the seams you extend: .artifacts/templates/dispatch-plan.md (you will amend it), docs/testing_strategy.org and docs/flow_model.org (org conventions + the mermaid precedent), scripts/ (house bash style: worktree-doctor.sh, run-quality-gates.sh), and AGENTS.md §7 (the section amended).
3. Write the TDD-first plan at .artifacts/plans/2026-11-13-ux-journey-framework-install-plan.md in the house [Plan] style. Settle at minimum, each as a recorded decision:
   - The exact table-header contract between the ux-journey-plan template and scripts/ux-coverage.sh: column names and order for the §2 capability matrix + rulings, §4 journey step tables, §6 wiring inventory, §7 command delta; which heading anchors the parser keys on; the parser must fail loudly (not silently skip) on a malformed table.
   - The script's implementation shape per Appendix C in house bash style (set -euo pipefail, usage header documenting the honest limits, --gate, --self-test), the fixture's home (you decide; e.g. scripts/fixtures/), and how --self-test exercises both a PASS row and a MISSING row with a nonzero exit on the MISSING path — the evidence command + expected outcome recorded in the plan.
   - The exact AGENTS.md §7 amendment wording: advisory-with-activation-condition per framework §7 item 4 and §8 decision 1 (mandatory for new user-visible work only once phases 3-5 complete; phase 6 flips it).
   - The docs/ux_planning.org outline: the framework law from §4, the advisory→mandatory activation state machine from §6, the method survey from §3 with the plan's Sources section carried over.
   - Commit sequencing so every commit stands alone (rebase-merge hygiene).
4. Resolve every open question with a recorded decision; deviations from the origin need a recorded reason.
5. Commit as: docs(plan): the ux-journey-framework install chunk plan. Do not implement; do not push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the UX journey planning framework phase-1 install ("framework install", branch docs/ux-journey-planning-framework). Work in /home/viktoria/src/taskboard-WT-docs-ux-journey-planning-framework (if missing, run scripts/worktree-doctor.sh from the project root first, then git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-docs-ux-journey-planning-framework docs/ux-journey-planning-framework — never on main).

1. Read AGENTS.md, the framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md (§4, §7, §8, Appendices A-C), the dispatch plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-dispatch-plan.md §1/§2, and the stage-1 plan (.artifacts/plans/2026-11-13-ux-journey-framework-install-plan.md). Follow the stage-1 plan exactly; deviations need a recorded reason in your report.
2. Implement the chunk's entire allowed diff (the six items of dispatch §2 Covers; nothing else):
   a. .artifacts/templates/ux-journey-plan.md — from framework Appendix A, keeping usage comments in the dispatch-plan.md style; the fixed table headers exactly per the stage-1 contract.
   b. .artifacts/templates/dispatch-plan.md — amended per framework Appendix B: UX coverage column in §1's chunks table; stage-2 gate self-check and stage-3 UX completeness mandate, both strictly conditional on the origin citing a UX journey plan; the fill-in-bar line naming UX plan sections by ID. The advisory-mode conditionality IS the semantics — never phrase the new mechanics as unconditional.
   c. docs/ux_planning.org — per the stage-1 outline, in the org conventions of docs/testing_strategy.org.
   d. AGENTS.md §7 — the UX journey plan artefact type + advisory-with-activation-condition trigger rule + pointer to docs/ux_planning.org.
   e. scripts/ux-coverage.sh — executable (chmod +x), house bash style, per Appendix C + the stage-1 decisions; --gate exits nonzero on MISSING; --self-test exercises the fixture's PASS and MISSING rows.
   f. the fixture plan at the stage-1-decided home, containing at least one row of each parsed kind with deliberately MISSING rows for the self-test.
3. Verify: scripts/ux-coverage.sh --self-test exits 0; a plain run over the fixture reports the MISSING rows; a deliberately-passed real UX plan is not required (none exists yet — the fixture stands in). Record the observed command + outcome as evidence in your report (excerpts only; no scratch files land).
4. Run the full gate suite via scripts/run-quality-gates.sh from the project root (AGENTS §10 wrapper — another agent may hold the flock; wait for it) and make it green. No Rust code changes are expected; if a gate surfaces a pre-existing failure unrelated to this diff, STOP and report instead of fixing unrelated code.
5. Every commit standalone (docs(...)/chore(...) subjects per AGENTS §8). No push, no PR — shipping is stage 3's job. Never push to or merge main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers in your report.

Report: per-gate results, self-test evidence, any deviation from the stage-1 plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the UX journey planning framework phase-1 install (branch docs/ux-journey-planning-framework). Work in /home/viktoria/src/taskboard-WT-docs-ux-journey-planning-framework (if missing, run scripts/worktree-doctor.sh from the project root first, then git -C /home/viktoria/src/taskboard worktree add ../taskboard-WT-docs-ux-journey-planning-framework docs/ux-journey-planning-framework).

1. Read AGENTS.md, the framework plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md (§4/§7/§8, Appendices A-C), the dispatch plan .artifacts/plans/2026-11-13-ux-journey-planning-framework-dispatch-plan.md §1/§2 (the evidence & artefact mandate applies to you), and the stage-1 plan.
2. Begin by rebasing: git fetch origin --prune; rebase the branch onto origin/main (drop already-merged duplicates; resolve only real conflicts); push --force-with-lease only if history was rewritten. Then review the full branch diff (git diff origin/main...HEAD).
3. Functional focus (this chunk's traps, named from the origin):
   - The template↔script table-header contract must match token-for-token — a renamed or reordered column silently zeroes a check; verify the parser keys on the agreed heading anchors.
   - --self-test must actually FAIL when a check goes MISSING: verify by deliberately breaking a fixture row (edit, observe nonzero exit, restore with git checkout -- ; never commit the break).
   - AGENTS.md must keep the rule advisory with the phase-6 activation condition — never mandatory-now wording.
   - The dispatch-plan.md amendment must keep every new mechanic conditional on "the origin cites a UX journey plan" (advisory-mode semantics).
   - Diff shape: exactly the six Covers items; no census file, no app code, no CI changes.
4. Evidence & artefact mandate: factual claims in the new docs carry evidence or verifiable sources; the §3 survey citations survive intact; the fixture is the only test data; scope discipline (out-of-scope ideas → backlog issues).
5. Re-run the full gate suite yourself via scripts/run-quality-gates.sh (wait for the flock); any failure is a finding. Re-run scripts/ux-coverage.sh --self-test.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (title "docs: install the UX journey planning framework (phase 1)"; body summarizing the six diff items and citing the framework plan as origin), then gh pr checks --watch to green — any failing check is a finding: diagnose, fix, push --force-with-lease, repeat. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat. main is branch-protected: never merge; the user merges on GitHub.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding; list the issue numbers in your report.
9. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-docs-ux-journey-planning-framework; confirm via git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Completion bookkeeping

- This chunk closes nothing on the backlog by itself; any `backlog:` issues
  its agents open are annotated with the PR URL.
- **Deliberately not dispatched here**: the census file (phase 3), the
  round-7 pilot (phase 2 — dispatched separately after this merges), the
  mandate flip (phase 6).
- **What opens next**: phase 2 (pilot) once this PR merges — three blind
  UX journey plans for the round-7 scope per framework §6.1.
