# Post-Round-9 Parallel Round ("tier-1 lanes") — Implementation & Dispatch Plan

- **Date**: 2026-11-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation/research → review) for the 7 chunks of the accepted
  Tier-1 set: the coverage-gate correctness bundle, the
  restore-machinery follow-ups, the docs/chore compressed sweep, the
  two test/IPC seam-hygiene chunks, the sync small-refactor pair, and
  the chrono time-stack evaluation. No code in this commit.
- **Origin**: the accepted recommendation report
  `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
  §Tier-1 (operator acceptance 2026-11-18). Issue bodies are normative
  for their chunks; the round-level decisions recorded below bind
  their stages. Round-9 (ACCEPTED, in implementation) is **excluded
  territory** per the report's collision framing: no chunk here
  touches `ui/widgets/`, the overlay/habit/keyboard/menu/day-review
  chains, or the census supersession writes.
- **Status**: READY — all 7 chunks dispatchable, under the resource
  constraint below. N-C runs **compressed** (dispatcher-owned,
  `docs/agent_dispatch.org` §7). N-F is research (ends at an operator
  gate). Deliberately not dispatched: the report's Tier-2 set, the
  fold-into-r9 set (#775/#881/#880/#870/#875/#938), and the standing
  deferrals.
- **Resource constraint (binding — r9 is running)**: at most **2
  concurrent chunks from this plan** beside r9's ≤3-agent cap,
  staggered ≥3 minutes (docs/agent_dispatch.org §2 + the memory-gate
  law from #906/#931). Never hold the gate lock long; the flock-
  deferral rule applies to every stage. Long mutant/benchmark-class
  runs stay exclusive — none are planned here.

## 1. Chunks & ordering

| Chunk | Report slice | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| N-A | Tier-1 §N-A | `ci/coverage-gate-correctness` | #920 token scoping + #768 bound-test mapping + #760 seam cross-check; acceptance oracle: the five r9 journey plans | no UX plan in origin (gate tooling) | M |
| N-B | Tier-1 §N-B | `feat/state-restore-family` | #913 restore-of-restore bug + #900 rule pre-images + #901 restore-class for DeleteLabel/TimeEntry/Habit | no UX plan in origin (engine-side) | M/L |
| N-C | Tier-1 §N-C | `docs/hygiene-doc-sweep` | #904 + #892 + #859 + #851 + #924 verify-close + #778 draft — compressed | no UX plan in origin (docs) | S |
| N-D1 | Tier-1 §N-D1 | `test/journey-fixture-seams` | #884 day-vocabulary seam + #897 + #921 + #914 + #905 | no UX plan in origin (test seams) | M |
| N-D2 | Tier-1 §N-D2 | `fix/ipc-stop-ordering-seams` | #886 stop ordering + #917 + #902 mutation-survivor seams | no UX plan in origin | S/M |
| N-E | Tier-1 §N-E | `refactor/sync-validator-grammar-band` | #937 DeckColor grammar dedent + #885 revalidation band | no UX plan in origin | S/M |
| N-F | Tier-1 §N-F | `docs/chrono-time-stack-evaluation` | #891 RUSTSEC-2026-0543 evaluation → ruling, operator-gated | no UX plan in origin (research) | M |

**Dependency graph**: no hard edges between chunks (disjoint lanes).
Default order — **N-C first** (tiny, frees the bookkeeping), **N-A
second** (the gate r9 leans on), then at most two concurrent from
**{N-D1, N-E} → {N-B, N-F} → N-D2**, staggered ≥3 min. N-F is
docs-only research and may run beside anything (it runs builds for
evidence only — no full suites). N-D1's early placement is deliberate:
its fixture-seam fix is the machinery r9's journey chunks extend.

**Dispatch mechanics**: one agent session per prompt, in order (N.x.1
→ N.x.2 → N.x.3); never two agents share a worktree. Worktrees at
`../taskboard-WT-<flat-suffix>` (branch `/` → `-`), branched off the
then-current `main`, no PR ever stacked; landed branch ⇒ resume on
`<branch>-v2`; create with `git worktree add -b <branch>
../taskboard-WT-<flat-suffix>` (never without `-b`, never on `main`).
`scripts/worktree-doctor.sh` first, every agent, nonzero = STOP.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding — `docs/agent_dispatch.org`;
PR #852 + the #879 flock-deferral rule, codified in PR #894)*: a
subagent is killed after ~600s without observable activity, so long
silent commands kill working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). **Flock-deferral rule: when the gate lock is
   contended and no other useful work remains, never block — commit
   all landed work, report "gate deferred: lock held by <holder>",
   and end the stage; the dispatcher runs the gate.** The dispatcher
   runs the full AGENTS §10 suite in its own session between stage 2's
   landing and stage 3's ship; CI's Code Coverage check runs it
   authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures (§10).
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim* (every prompt
   below carries the [D] block).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (`git log origin/main..HEAD` — commits present, tree clean, gate
   evidence re-runnable). A stalled or quota-cut stage is completed
   from its landed commits or resumed with a state-check message —
   **resume over redo**, never re-dispatched blind. Failure playbook:
   `docs/agent_dispatch.org` §8.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: worktree, TDD-first chunk plan at
   `.artifacts/plans/<today>-<topic>-plan.md`, every open question
   resolved with a recorded decision, committed (`docs(plan): …`). No
   implementation, no push. This plan's per-chunk specs are normative;
   deviations need a recorded reason.
2. **Stage 2 — implementation/research agent**: continues in the
   worktree (recreate on the branch if missing; landed branch ⇒
   `<branch>-v2`), executes per the approved plan — TDD where code is
   involved, every commit standalone (rebase-merge hygiene), gate =
   pre-commit hooks + flock-wrapped nextest (defer per the rule). No
   push, no PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main` (fetch
   --prune, rebase onto `origin/main`, drop already-merged duplicates,
   resolve only real conflicts, push `--force-with-lease` if history
   rewritten), reviews the branch diff versus the post-rebase base
   against the chunk's acceptance criteria + AGENTS rules + the
   applicable mandate, re-runs the flock-wrapped nextest gate (defer
   per the rule), implements fixes on the branch (amend/reword, never
   duplicate-subject fixups). Last tasks: push, open the PR to `main`
   with `gh`, report the URL + per-acceptance verdicts. **No CI watch.**

**Code-quality mandate** (stage 3, code chunks N-A–N-E — verbatim):
beyond functional review, evaluate (a) code quality: naming, comment
discipline (comments state constraints, not narration), idiomatic use
of the approved stack, no dead code or speculative generality, test
quality per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line — a boundary
violation is an SRP violation; (c) a single level of abstraction per
function body: a function either orchestrates named operations or
works in primitive data, never mixes. Findings are fixed on the
branch like any other finding.

**Evidence & artefact mandate** (stage 3, N-F — verbatim): every
factual claim carries evidence (the command run + observed outcome, or
a verifiable source + what it states) and experiments are reproducible
from the report's method section; recommendations are decision-grade
(options, trade-offs, exactly **one** recommendation, consequences for
later work); repo hygiene (scratch code and captured payloads never
land — evidence is excerpts inside the report; the PR diff is exactly
the chunk's artefacts); scope discipline (out-of-scope ideas go to the
backlog per its template, not into the report).

## 2. Chunk N-A — coverage-gate correctness (`ci/coverage-gate-correctness`)

- **Covers** (normative): the three gate debts, with these
  round-level decisions:
  - **#920 (token prefix collisions)**: the gate resolves
    `us<n>_s<k>` tokens spuriously across plans sharing prefixes. The
    decision: step-token coverage is evaluated **per invoked plan** —
    the gate extracts the invoked plan's own declared step ids and
    requires a token match to (a) exist in that plan's id set and (b)
    be unambiguous within it; a shared-prefix test name from a
    *different* plan must not satisfy this plan's row. Where test
    names alone cannot disambiguate legacy plans, the gate reports
    AMBIGUOUS (a triage state, not a pass) — never a silent credit.
    Going forward, journey test names embed the plan slug (the gate
    documents the convention; no mass rename of landed tests).
  - **#768 (retro-fitted plans)**: the accepted mapping mechanism — a
    `bound-tests:` frontmatter/table row set in the plan itself
    (step id → pre-existing bound test name(s)), which the gate reads
    and verifies the bound tests exist in the workspace; rows verify
    as BOUND (distinct from FOUND). Plans without the table behave as
    today. The five r9 plans (all framework-era) must NOT need it —
    their step tokens are REQUIRED rows (r3 D-9 posture); the mapping
    exists for the retro cohort only.
  - **#760 (seam cross-check)**: every wiring row the gate evaluates
    is cross-checked against `tests/seam_inventory.rs` (the row's
    callback seam has an inventory entry); a row whose seam is absent
    from the inventory reports INVENTORY-MISS (triage state), which
    stage-3 protocols already treat as a finding.
- **Acceptance**: the gate run against the five r9 journey plans
  (`.artifacts/plans/2026-11-18-{habit-parity,widget-framework,keyboard-focus,day-review,menus-remainder}-ux-plan.md`)
  produces no cross-plan spurious FOUND; AMBIGUOUS/BOUND/
  INVENTORY-MISS states behave as specified; the r9 plans' REQUIRED
  rows still resolve (the gate change must not break r9's own
  verification); the mapping + conventions are documented in the
  script's help/README or `docs/ux_planning.org` (recorded choice);
  diff confined to `scripts/ux-coverage.sh`, the seam-inventory
  cross-check surface, and docs.

**Prompt N-A.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-A of the post-round-9 parallel round ("coverage-gate correctness", branch ci/coverage-gate-correctness) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed/structural outcomes — the gate's verdict states (FOUND/MISSING/AMBIGUOUS/BOUND/INVENTORY-MISS) are machine-readable, never prose; (c) never weaken a correct check — #920's fix must not turn today's false positives into blanket passes; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = below the disk floor: STOP and report), then git worktree add -b ci/coverage-gate-correctness ../taskboard-WT-ci-coverage-gate-correctness off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-1 N-A, normative), the dispatch plan .artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md §1/§2, issues #920 #768 #760, and docs/ux_planning.org (the gate's law — the r3 D-9 posture: framework-era plans' step tokens are REQUIRED rows).
2. Read the seams: scripts/ux-coverage.sh (the matcher, its inputs, its exit contract), tests/seam_inventory.rs (the inventory the cross-check reads), and the five r9 journey plans (.artifacts/plans/2026-11-18-*-ux-plan.md — your acceptance oracle) plus one retro-era plan that exhibits the #768 gap.
3. Decide per the dispatch plan's recorded decisions: the per-invoked-plan scoping mechanics (how the plan's declared id set is extracted and matched; the AMBIGUOUS state's exact trigger), the bound-tests table format and the BOUND verification path, the INVENTORY-MISS cross-check mechanics, and the going-forward plan-slug naming convention's doc home (script help vs docs/ux_planning.org — record the choice).
4. Write the chunk plan at .artifacts/plans/2026-11-18-coverage-gate-correctness-plan.md, including the acceptance run: the gate invoked against each of the five r9 plans, before/after outputs excerpted (the before showing at least one spurious resolution #920 describes). Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the coverage-gate correctness chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, the before-evidence, risks, estimated diff size.
```

**Prompt N-A.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk N-A of the post-round-9 parallel round (branch ci/coverage-gate-correctness).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; never block on a contended lock — defer per the rule).
[L] Round laws (binding): (a) machine-readable verdict states; (c) no weakened checks — every state the gate emitted before must still be emitted where honest; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-A (normative), the dispatch plan §1/§2, and the stage-1 plan .artifacts/plans/2026-11-18-coverage-gate-correctness-plan.md in ../taskboard-WT-ci-coverage-gate-correctness (recreate on the branch if missing; landed branch ⇒ ci/coverage-gate-correctness-v2).
2. Implement exactly per the plan: the scoping fix (#920), the bound-tests mechanism (#768), the seam cross-check (#760). The gate is shell — every behavioral change carries a self-test (the script's existing test pattern, or a tests/ harness the plan recorded); the five-r9-plans acceptance run is executed and its outputs excerpted into the chunk report. Every commit standalone; diff confined to scripts/ + the inventory cross-check surface + docs per the plan.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the five-plan acceptance run excerpt, per-gate results, deviations.
```

**Prompt N-A.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-A of the post-round-9 parallel round (branch ci/coverage-gate-correctness).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) machine-readable verdict states; (c) the five r9 plans' REQUIRED rows still resolve after your rebase — re-run the acceptance set yourself; a gate change that breaks r9's own verification is a shipping blocker; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-A (normative), the dispatch plan §1/§2 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-ci-coverage-gate-correctness.
2. Functional focus — this chunk's traps: (a) the spurious-resolution fix must reproduce the #920 failure shape in a test before the fix (the stage-1 before-evidence) — a fix without the demonstrated-before is a finding; (b) AMBIGUOUS must never silently credit — construct the shared-prefix scenario across two plans and check the verdict; (c) BOUND rows verify the bound tests exist (a bound-tests row naming a nonexistent test is a gate failure, not a pass); (d) INVENTORY-MISS is triage, not auto-fail — the stage-3 protocols consume it; (e) the gate's exit-code contract is preserved for CI/UX (a change that flips exit semantics is a finding).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #920 #768 #760 — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 3. Chunk N-B — restore-machinery family (`feat/state-restore-family`)

- **Covers** (normative): #913 (the restore-of-restore refusal —
  undoing a RestoreTask/RestoreStack entry hits NothingToUndo because
  the CreatedTask synthesis arm doesn't match the restore-class
  outcome), #900 (reminder-rule pre-images so restore-class verbs
  resurrect cascaded rule deletes), #901 (restore-class machinery
  extended to DeleteLabel/DeleteTimeEntry/DeleteHabit).
  Round-level constraints: the C8/C9 landed semantics are the base —
  the strict frontier rule stands; ceremony fences hold
  (NotUndoable ceremony verbs never enter restore machinery); the
  existing undo/journal snapshot corpus is extended, not rewritten
  (any changed existing snapshot needs the plan's recorded diagnosis);
  typed outcomes; no RwLock/Mutex (AGENTS §3); the UI legs
  (#353/#354) stay untouched; DeleteHabit's restore is engine-side
  machinery only — the GUI removal journey (r9 H4) composes with it
  later, no surface work here.
- **Acceptance**: #913's scenario pinned by a snapshot/typed test and
  fixed; cascade pre-images proven by tests where a rule delete rides
  a restored delete (#900); the three new restore-class verbs
  (#901) each have restore + undo round-trip tests; corpus delta is
  new-files-only or per-diagnosis; diff confined to
  `taskboard-state` (+ the domain message types the plan names).

**Prompt N-B.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-B of the post-round-9 parallel round ("restore-machinery family", branch feat/state-restore-family) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance for any undo-window timing — never sleeps; (c) the existing snapshot corpus is extended, not rewritten; the strict frontier rule from C9 stands; ceremony verbs stay NotUndoable; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/state-restore-family ../taskboard-WT-feat-state-restore-family off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-1 N-B, normative), the dispatch plan .artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md §1/§3, issues #913 #900 #901, and the landed base: .artifacts/plans/2026-11-17-state-restore-stack-plan.md + 2026-11-17-state-per-board-undo-plan.md (the strict frontier rule, the CreatedTask synthesis, the restore-class outcome shape).
2. Read the seams: the restore-class machinery in taskboard-state (outcome classification, the synthesis arm #913 names), the cascade paths where a delete takes a ReminderRule with it (#900's pre-image insertion points), the DeleteLabel/DeleteTimeEntry/DeleteHabit command paths (#901's classification targets), and the undo/journal snapshot tests corpus.
3. Decide: the #913 semantics (what undoing a restore-class entry means — re-delete via the recorded inverse, or a typed refusal that is NOT NothingToUndo; record which and why), the pre-image storage shape (#900 — where the rule snapshot rides the cascade, its cleanup), the #901 classification set and per-verb round-trip invariants; the test catalogue (snapshots + typed outcomes; proptest where pure).
4. Write the chunk plan at .artifacts/plans/2026-11-18-state-restore-family-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the state restore family chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the #913 semantics decision, risks, estimated diff size.
```

**Prompt N-B.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk N-B of the post-round-9 parallel round (branch feat/state-restore-family).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance — never sleeps; (c) corpus extended not rewritten; strict frontier rule stands; ceremony fences hold; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-B (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-18-state-restore-family-plan.md in ../taskboard-WT-feat-state-restore-family (recreate on the branch if missing; landed branch ⇒ feat/state-restore-family-v2).
2. Implement exactly per the plan, TDD (catalogue first where pure): #913's scenario red then fixed per the recorded semantics; #900's pre-images; #901's three verb classifications with round-trip tests. Engine-side only: taskboard-state + the domain message types the plan names — no UI crate changes, no #353/#354 work. AGENTS §3 channels only; every commit standalone; SPDX/forbid(unsafe) per crate rules.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, snapshot-corpus delta summary, per-gate results, deviations.
```

**Prompt N-B.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-B of the post-round-9 parallel round (branch feat/state-restore-family).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) changed existing snapshots each need the plan's recorded diagnosis — unexplained churn is a finding; the strict frontier rule and ceremony fences hold — trace every new stack/restore entry site; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-B (normative), the dispatch plan §1/§3 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-state-restore-family.
2. Functional focus — this chunk's traps: (a) the #913 fix must handle the whole restore-class outcome set the synthesis arm can produce, not just the reproduced case — enumerate the outcomes and check each has an undo semantic; (b) #900's pre-image must ride the same transaction/apply path as the cascade (a pre-image that can diverge from what was actually deleted is a correctness hole — check ordering and failure paths); (c) #901's DeleteHabit restore composes with r9 H4's GUI removal later — the machinery must not assume a UI arm exists; (d) no UI crate imports anything new; no RwLock/Mutex in the diff.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code or speculative generality, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #913 #900 #901 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 4. Chunk N-C — docs/chore sweep (`docs/hygiene-doc-sweep`) — COMPRESSED

- **Covers** (normative): #904 (AGENTS.md §8 doctor-floor wording +
  §10 agent-build row mention the memory gate — actionable since #887
  landed), #892 (update the #682 property comment's test
  cross-reference — **conditional**: only if the byte-identical
  constraint has lifted per the issue's recorded condition; else
  record-and-defer on the issue), #859 (`UiHandle::refresh` doc
  carries the unchanged-refresh guard's forced-pass invariant),
  #851 (delete stray `#…#` editor backups under `.artifacts/plans/` —
  inspect each target before deleting; add a `.artifacts/.gitignore`
  pattern or the recorded equivalent), plus two bookkeeping actions:
  **#924 verify-and-close** (confirm systemd-oomd is live —
  `systemctl is-active systemd-oomd` — then close with the evidence;
  if not active, report instead of closing) and **#778 draft-only**
  (draft the upstream Slint issue from the existing evidence report as
  an in-repo artefact; **posting is the operator's call** — an
  upstream post is an external action and needs the explicit go).
- **Runs compressed** (`docs/agent_dispatch.org` §7): the dispatcher
  is planner and implementer — commit a short chunk plan first
  (`.artifacts/plans/2026-11-18-hygiene-doc-sweep-plan.md`, stating
  the compression), make the edits, run the gates that apply
  (pre-commit hooks; docs-only — the flock-wrapped nextest as smoke if
  any doc-adjacent code moved, which it should not), open the PR
  (body references #904 #892 #859 #851 #778), watch checks per the
  runbook. #924's closure is a direct GitHub action (no PR).

## 5. Chunk N-D1 — journey-fixture seams (`test/journey-fixture-seams`)

- **Covers** (normative): #884 (journey fixtures derive the day
  vocabulary from a hardcoded UTC calendar while the adapter's
  calendar differs — fix: the fixture reads the same calendar seam the
  adapter uses, injected or shared per stage-1's recorded choice),
  #897 (extract_raw validity-only unit assertions stay ambient-exposed
  against undecodable TASKBOARD_* env — apply the recorded #865
  option-2 pattern's remainder), #921 (us2_s2's open-time seed oracle
  re-reads the wall clock — single-read capture per the clock-
  discipline §6.2 seam), #914 (dedupe the C10 daemon-lane fixture onto
  a shared bootstrap helper, killing the mem::forget tempdir leak),
  #905 (the env-lock test's own ambient TASKBOARD_CONFIG observation —
  diagnose the lock's save/restore against undecodable values, fix or
  record).
- **Acceptance**: per-issue fix with the recorded evidence; #884's
  fixture change keeps every existing journey test green (the
  vocabulary change must not silently re-anchor expectations — any
  test whose outcome changes is a diagnosis, not a refresh); the
  hostile-env pass stays green; diffs confined to test helpers/
  fixtures; no production code change beyond a recorded seam need.

**Prompt N-D1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-D1 of the post-round-9 parallel round ("journey-fixture seams", branch test/journey-fixture-seams) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) single-read capture or injected clock for wall-clock-oracle tests — never sleeps; (c) never weaken a correct test — #884's vocabulary fix must not re-anchor any expectation without a recorded diagnosis; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/journey-fixture-seams ../taskboard-WT-test-journey-fixture-seams off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-1 N-D1, normative), the dispatch plan .artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md §1/§6, issues #884 #897 #921 #914 #905, and docs/testing_strategy.org §6.1–6.2 (the clock seams #921 cites).
2. Read the seams: the journey fixtures' day-vocabulary derivation vs the adapter's calendar (issue #884 names the divergence), the extract_raw validity-only assertion sites (#897), the us2_s2 open-time oracle (#921), the C10 daemon-lane fixture + its mem::forget leak (#914), the C6 env-lock helper's save/restore path (#905).
3. Decide per issue: the calendar seam's injection/shared shape (#884 — and the inventory of journey tests whose anchoring shifts, each with a keep-or-diagnose call), the #897 remedy per the recorded option-2 pattern, the #921 single-read shape, the #914 helper API + leak fix, the #905 diagnosis method.
4. Write the chunk plan at .artifacts/plans/2026-11-18-journey-fixture-seams-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the journey fixture seams chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the #884 shift inventory, decisions, risks, estimated diff size.
```

**Prompt N-D1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk N-D1 of the post-round-9 parallel round (branch test/journey-fixture-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) single-read capture — never sleeps; (c) a journey test whose outcome changes under the #884 vocabulary fix gets a recorded diagnosis, never a silent snapshot refresh; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-D1 (normative), the dispatch plan §1/§6, and the stage-1 plan .artifacts/plans/2026-11-18-journey-fixture-seams-plan.md in ../taskboard-WT-test-journey-fixture-seams (recreate on the branch if missing; landed branch ⇒ test/journey-fixture-seams-v2).
2. Implement exactly per the plan, per-issue commits. Diff confined to test helpers/fixtures (+ a recorded seam need only). New test-support code follows the crate's forbid(unsafe)/edition-2024 env rules (no env mutation in lib crates — the C6/#865 patterns).
3. Prove per the plan: the hostile-env pass green; the #884 shift inventory's keep-or-diagnose calls executed as recorded.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, hostile-env evidence, per-gate results, deviations.
```

**Prompt N-D1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-D1 of the post-round-9 parallel round (branch test/journey-fixture-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) every snapshot/journey-expectation change traces to the plan's shift inventory — unexplained expectation changes are findings; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-D1 (normative), the dispatch plan §1/§6 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-journey-fixture-seams.
2. Functional focus — this chunk's traps: (a) #884's seam must be the adapter's actual calendar source (not a second hardcoded table that drifts again — the issue is about parallel hand-maintenance); (b) the #914 helper must not reintroduce the leak (no mem::forget on the new path; the old leak sites are gone); (c) #921's single-read capture actually removes the minute-boundary window (construct the boundary scenario in review); (d) #905's diagnosis conclusion is recorded even if the fix is "not reproducible" — an honest armed state beats a speculative change.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #884 #897 #921 #914 #905 — closures are the dispatcher's at merge per the disposition evidence), report the URL + per-issue verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict.
```

## 6. Chunk N-D2 — stop ordering + mutation survivors (`fix/ipc-stop-ordering-seams`)

- **Covers** (normative): #886 (graceful-stop ordering —
  `daemon:false` becomes client-visible before the instance lock is
  released; fix the ordering so the flag transition reflects or follows
  the lock release, typed and pinned), #917 (kill the
  IpcServerHandle::join is_finished-guard mutation survivor via a
  drain-budget test seam), #902 (kill the try_connect error-class-guard
  survivor via a latency-seam test).
- **Acceptance**: the ordering fix pinned by a test that fails under
  the old order (or an equivalent structural proof); both survivors
  killed by tests that assert the guarded behavior for real (not
  tautologies — the mutation score evidence or an equivalent argument
  recorded); no production behavior change beyond the ordering fix;
  diffs confined to the ipc/app seams + tests.

**Prompt N-D2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-D2 of the post-round-9 parallel round ("ipc stop ordering + mutation survivors", branch fix/ipc-stop-ordering-seams) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected latency seams — never sleeps; (c) never weaken a correct test; the mutation survivors indicate tests that assert the guard's shape, not its behavior — the fix is behavioral coverage; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/ipc-stop-ordering-seams ../taskboard-WT-fix-ipc-stop-ordering-seams off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-1 N-D2, normative), the dispatch plan .artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md §1/§7, issues #886 #917 #902, and the C7 mutants report (.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md — the survivor contexts).
2. Read the seams: the ipc graceful-stop path (where daemon:false is set vs where the instance lock releases — the ordering window #886 names), the IpcServerHandle::join drain/guard (#917), the try_connect error-class guard (#902) and its current test.
3. Decide: the #886 ordering semantics (which transition moves, what the client observes during the window, the pinned test's failure mode under the old order), the drain-budget seam shape (#917), the latency-seam shape (#902) — each seam must make the guard's behavior observable, not its syntax.
4. Write the chunk plan at .artifacts/plans/2026-11-18-ipc-stop-ordering-seams-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ipc stop ordering seams chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the ordering semantics decision, risks, estimated diff size.
```

**Prompt N-D2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk N-D2 of the post-round-9 parallel round (branch fix/ipc-stop-ordering-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) injected seams — never sleeps; (c) TDD order — the pinning test fails under the old behavior first; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-D2 (normative), the dispatch plan §1/§7, and the stage-1 plan .artifacts/plans/2026-11-18-ipc-stop-ordering-seams-plan.md in ../taskboard-WT-fix-ipc-stop-ordering-seams (recreate on the branch if missing; landed branch ⇒ fix/ipc-stop-ordering-seams-v2).
2. Implement exactly per the plan, TDD: #886's ordering fix red-under-old-order first; #917/#902's seams with tests that kill the named survivors. Diff confined to the ipc/app seams + tests; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, red-under-old-order evidence, per-gate results, deviations.
```

**Prompt N-D2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-D2 of the post-round-9 parallel round (branch fix/ipc-stop-ordering-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) the new tests must fail under the pre-fix behavior — revert the production hunk mentally (or actually, on a scratch commit you drop) and check the test goes red; a green-anyway test is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-D2 (normative), the dispatch plan §1/§7 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-ipc-stop-ordering-seams.
2. Functional focus — this chunk's traps: (a) the #886 reordering must not introduce a new window the other way (lock released before the flag is safe to read — check both orderings' client-visible invariants); (b) the drain-budget/latency seams are injected (deterministic), never real-time waits; (c) the survivor tests assert behavior the guard exists for, not the guard's syntax (the mutation rationale from the C7 report).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #886 #917 #902 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 7. Chunk N-E — sync validator grammar + band (`refactor/sync-validator-grammar-band`)

- **Covers** (normative): #937 (DeckColor's validate re-implements the
  six-hex colour grammar the domain owns — delete the
  re-implementation, call the domain grammar; stage-1 verifies the
  dependency direction is legal per AGENTS §2 and records it),
  #885 (the cadence-style revalidation band replacing the
  single-constant SKEW_BAND_SECS shape — the band-shape alternative
  #885 tracks; rides #868's landed fetch-time seam + actor clock).
- **Acceptance**: #937 — the sync crate's colour validation delegates
  to the domain grammar (one grammar, one owner), behavior identical
  on the valid/invalid matrix (pin with the transferred tests);
  #885 — the band shape per #885's recorded alternative, time-
  controlled tests proving inside-band refresh and outside-band skip;
  the #133 tolerance constant's usages reconciled (no orphaned
  constant); diffs confined to `taskboard-sync-nextcloud` (+ domain
  exports the plan names).

**Prompt N-E.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-E of the post-round-9 parallel round ("sync validator grammar + band", branch refactor/sync-validator-grammar-band) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or the actor clock — never sleeps; (c) never weaken a correct test — the valid/invalid colour matrix transfers, it does not shrink; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b refactor/sync-validator-grammar-band ../taskboard-WT-refactor-sync-validator-grammar-band off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-1 N-E, normative), the dispatch plan .artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md §1/§8, issues #937 #885, and the landed base: the #868 seam (fetch-time + actor clock) and the #133 tolerance constant's definition/usages.
2. Read the seams: taskboard-sync-nextcloud's DeckColor validate (the re-implemented grammar + its tests), the domain grammar's owner location and exports, the sidecar skip's SKEW_BAND_SECS guard and its band test, and the crate dependency graph (sync → domain legality per AGENTS §2).
3. Decide: the delegation shape (#937 — which domain fn, what sync-side shell remains, the transferred test matrix), the band shape (#885 — per the issue's recorded alternative: the cadence window, its constants, reconciliation with the #133 constant — one owner, no orphan), the test catalogue.
4. Write the chunk plan at .artifacts/plans/2026-11-18-sync-validator-grammar-band-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the sync validator grammar and band chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, the dependency-legality check, risks, estimated diff size.
```

**Prompt N-E.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk N-E of the post-round-9 parallel round (branch refactor/sync-validator-grammar-band).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) actor clock / pause-advance — never sleeps; (c) the colour matrix transfers in full; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-E (normative), the dispatch plan §1/§8, and the stage-1 plan .artifacts/plans/2026-11-18-sync-validator-grammar-band-plan.md in ../taskboard-WT-refactor-sync-validator-grammar-band (recreate on the branch if missing; landed branch ⇒ refactor/sync-validator-grammar-band-v2).
2. Implement exactly per the plan: #937's delegation (grammar tests transferred to the domain owner's suite or kept as sync-side contract tests per the plan), #885's band with the time-controlled two-arm test. Diff confined to taskboard-sync-nextcloud (+ the domain exports the plan names); every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the transferred-matrix evidence, per-gate results, deviations.
```

**Prompt N-E.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-E of the post-round-9 parallel round (branch refactor/sync-validator-grammar-band).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) one grammar, one owner — any residual hex-parsing in the sync crate after the diff is a finding; the band constants are single-sourced with the #133 constant reconciled; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-E (normative), the dispatch plan §1/§8 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-refactor-sync-validator-grammar-band.
2. Functional focus — this chunk's traps: (a) the delegation must preserve the sync-side error typed outcomes (a domain error type leaking to the wire path unchanged is a boundary smell — the plan's shell should map it); (b) the band's two arms are both pinned (inside fires, outside skips) under controlled time; (c) the refactor is behavior-identical on the Deck path (the round-trip corpus unchanged or per-diagnosis).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #937 #885 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 8. Chunk N-F — chrono time-stack evaluation (`docs/chrono-time-stack-evaluation`) — research

- **Covers** (normative): #891 — RUSTSEC-2026-0543 marks chrono
  unmaintained; the workspace approves and pins chrono (AGENTS §4).
  Deliverable: an evidence report + a ruling recommendation, ending at
  an **operator gate**. Zero production code.
- **Acceptance**: exposure assessment (what the unmaintained status
  actually risks for this crate's usage: Y2K-style gate, tzdata
  staleness, no security patches — with the advisory's verbatim
  claims + the workspace's chrono call-site inventory as evidence);
  the alternatives assessed (at minimum: stay-on-chrono with dated
  re-evaluation, jiff, time — others if the evidence warrants), each
  with migration-cost surface (sync HTTP-date handling, recurrence/
  schedule math, habit windows, the #133 skew constants, serialization
  boundaries); exactly **one** recommendation with consequences; the
  report names the gate handoff (what the operator accepts, what a
  migration dispatch would chunk if the ruling is "move").

**Prompt N-F.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk N-F of the post-round-9 parallel round ("chrono time-stack evaluation", branch docs/chrono-time-stack-evaluation) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): research builds (cargo tree, call-site counting) run backgrounded with useful work between polls; commit landed work BEFORE any wait; defer gate runs per the rule if contended.
[L] Round laws (binding): (a) evidence-carrying claims only — recorded reality, never invented; (c) options + trade-offs + exactly one recommendation; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b docs/chrono-time-stack-evaluation ../taskboard-WT-docs-chrono-time-stack-evaluation off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§4's stack table — chrono is an approved pinned dependency), the recommendation report §Tier-1 N-F (normative), the dispatch plan §1/§9, issue #891, and the advisory (RUSTSEC-2026-0543 — fetch its verbatim claims), plus the #891 deny-ignore context from the correctness round (#891 was carried by C1's PR).
2. Write the research chunk plan at .artifacts/plans/2026-11-18-chrono-time-stack-evaluation-chunk-plan.md: the call-site inventory method (cargo tree + grep census by crate/module), the exposure-assessment frame, the alternative set, the migration-cost surface checklist, and the report/ruling skeletons. Zero production code is a hard boundary — the plan states it.
3. Commit as: docs(plan): the chrono evaluation chunk plan. Do not implement; do not push.
4. Backlog issues per [L](d); list the numbers.

Report: plan path, the inventory method, risks.
```

**Prompt N-F.2 (research):**

```text
You are the stage-2 RESEARCH agent for chunk N-F of the post-round-9 parallel round (branch docs/chrono-time-stack-evaluation).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): census builds/greps run backgrounded with useful work between polls; commit landed work BEFORE any wait; defer gate runs per the rule if contended. (Docs-only chunk: pre-commit hooks are your gate; the dispatcher runs the full §10 suite.)
[L] Round laws (binding): (a) evidence-carrying claims — the advisory's verbatim text, call-site counts with the counting commands, alternative assessments with sources; (c) exactly one recommendation, decision-grade, consequences stated; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-F (normative), the dispatch plan §1/§9 (the evidence & artefact mandate applies to you in full), and the stage-1 chunk plan .artifacts/plans/2026-11-18-chrono-time-stack-evaluation-chunk-plan.md in ../taskboard-WT-docs-chrono-time-stack-evaluation (recreate on the branch if missing; landed branch ⇒ docs/chrono-time-stack-evaluation-v2).
2. Execute per the plan: the call-site census, the exposure assessment, the alternatives (stay / jiff / time / others as evidence warrants) each with a migration-cost surface against the named seams (sync HTTP-date, recurrence/schedule math, habit windows, skew constants, serialization), and the ruling recommendation with consequences (including what a "move" dispatch would chunk and its risk profile).
3. Write the report (.artifacts/reports/2026-11-18-chrono-time-stack-evaluation-report.md) + the ruling doc (.artifacts/plans/2026-11-18-chrono-time-stack-ruling.md) per the skeletons. Every claim carries its evidence inline.
4. Gate: pre-commit hooks (docs-only; the dispatcher runs the full §10 suite).
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: the recommendation summary, census totals, deviations.
```

**Prompt N-F.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk N-F of the post-round-9 parallel round (branch docs/chrono-time-stack-evaluation).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): commit landed work BEFORE any wait; defer gate runs per the rule if contended. Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) spot-check every load-bearing claim's evidence (re-run the counting commands; open the advisory); (c) exactly one recommendation, alternatives recorded, gate handoff explicit; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §N-F (normative), the dispatch plan §1/§9 (the evidence & artefact mandate verbatim in §1), the stage-1 chunk plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-docs-chrono-time-stack-evaluation.
2. Functional focus — this chunk's traps: (a) the diff is exactly the report + ruling doc (+ the chunk plan already committed) — anything else is a finding; (b) invented claims about chrono/jiff/time capabilities without a cited source are findings; (c) the migration-cost surface covers the named seams (an alternative assessed without the recurrence-math cost is incomplete); (d) the ruling names the gate and the follow-on dispatch shape if "move".
3. Evidence & artefact mandate (verbatim, §1 of this dispatch plan).
4. Pre-commit hooks are your gate; the dispatcher runs the full §10 suite.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #891 — it stays open through the operator gate, mirroring the #708 pattern), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 9. Dispatcher runbook (binding)

Per `docs/agent_dispatch.org` (as codified through #894): between
every pair of stages verify landed state in the worktree (commits,
clean tree, gate evidence re-runnable) — resume over redo, playbook
§8. Between stage-2 landing and stage-3 ship of every code chunk
(N-A, N-B, N-D1, N-D2, N-E): the full AGENTS §10 suite through
`scripts/run-quality-gates.sh` in your own session (background +
poll). At each stage-3 PR-open: `gh pr checks --watch`, fix-forward
loop only if a check fails and needs the branch, `mergeStateStatus`
`CLEAN`, then absolute-path teardown + `git worktree list`
confirmation. **Concurrency: never more than 2 chunks from this plan
in flight beside r9**, staggered ≥3 min (memory-gate law, #906/#931);
N-C and N-F are the low-contention neighbors. N-C runs compressed per
§4 (compression stated in its chunk plan).

## 10. At-merge closures & bookkeeping (dispatcher-owned)

- **N-A** → #920, #768, #760.
- **N-B** → #913, #900, #901.
- **N-C** → #904, #892 (only if its condition held; else the
  record-and-defer comment), #859, #851; #924 closed directly on
  oomd evidence; #778 stays open with the draft artefact linked and
  the operator-gate note.
- **N-D1** → #884, #897, #921, #914; #905 per its disposition
  (close on fix evidence, or dated armed-hypothesis comment).
- **N-D2** → #886, #917, #902.
- **N-E** → #937, #885.
- **N-F** → #891 closes at the operator's ruling acceptance (the
  dispatcher posts the decision comment linking the ruling doc; if
  the ruling is "move", the migration round is a new dispatch).
- **Deliberately not dispatched here**: the report's Tier-2 set
  (#873/#874, #751, #718+#755, #673, #733, #909+#910, #890, #846,
  #893), the fold-into-r9 set (#775/#881/#880/#870/#875/#938), armed
  flakies (#697/#650), no-recurrence flakies (#336/#287/#384/#883),
  #327's sharded mutants lane, habit backlog #955–#963, O-5 remainder
  #946–#949, and the standing deferrals.
- **What opens next**: the Tier-2 pull-ins when capacity frees; the
  chrono migration dispatch if the N-F ruling says "move"; the post-r9
  round (undo UI legs #353/#354 journeys, habit backlog #955–#963
  journeys, the #946–#949 parity remainder).
