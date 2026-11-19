# Backlog Reduction Round 2026-11-17 ("hygiene r2") — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-17
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation/research → review) for the 11 chunks of the accepted
  backlog-reduction round: the #879 dispatch-law codification, the
  #857+#843 test-infra determinism pair, the #697+#570 flaky
  diagnoses, the #860+#868 audit seams, the #856 domain no-op ruling,
  the #865+#876+#877 hygiene trio, the #327 mutants cap, the undo-v2
  foundation pair (#356→#355), and the journey-gated CLI chunks
  (#845, #764). No code in this commit.
- **Origin**: the accepted round plan
  `.artifacts/plans/2026-11-17-backlog-reduction-round-plan.md`
  (§1 scope boundary, §2 family map, §3 per-chunk normative specs,
  §4 operator-manual items, §5 sequencing, §6 chunking proposal) —
  **ACCEPTED by the operator 2026-11-17**; issue bodies are
  normative for their chunks; the recorded round-level decisions
  (#865 option 2, #856 structural no-op, #843 option A,
  #877 shared-helper-first, #857 artifact persistence) bind their
  stages.
- **Status**: READY. C1 runs **compressed** (dispatcher-owned,
  `docs/agent_dispatch.org` §7). C10/C11 each carry a **journey-plan
  human gate** between stage 1 and stage 2 — the dispatcher pauses at
  the gate and proceeds only on the operator's acceptance. Deliberately
  not dispatched: the round plan §1 exclusion list (watchlist
  negatives #336/#287/#384, r8 territory and its coverage table,
  #353/#354/#357, the perf seven, feature-scale items, the dependabot
  sweep) and §4's operator-manual items.
- **Orchestration law**: `docs/agent_dispatch.org` (PR #852), amended
  by C1's codified **flock-deferral rule** (already binding on this
  round's prompts regardless of C1's merge order — the rule text is
  in the round plan §3-C1 and restated verbatim in every prompt).

## 1. Chunks & ordering

| Chunk | Round-plan slice | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| C1 | §3-C1 | `chore/dispatch-law-flock-deferral` | #879 law codification (org + template), compressed | no UX plan in origin (docs) | S |
| C2 | §3-C2 | `test/proptest-persistence-fixture-clock` | #857 artifact persistence + canary; #843 fixture clock option A; #650 mechanism comment | no UX plan in origin | M |
| C3 | §3-C3 | `test/flaky-diagnosis-r2` | #697 budget inequality + CI gap telemetry; #570 relay diagnosis | no UX plan in origin | M/L |
| C4 | §3-C4 | `fix/audit-finding-seams` | #860 notice push on early-return; #868 CachedAppData seam + skew-band guard | no UX plan in origin | S/M |
| C5 | §3-C5 | `fix/domain-same-ref-collapse` | #856 structural no-op + pinned units | no UX plan in origin | S |
| C6 | §3-C6 | `test/app-storage-hygiene` | #865 option 2; #876 dead-param drop; #877 shared fixture helper | no UX plan in origin | M |
| C7 | §3-C7 | `ci/cargo-mutants-capped` | #327 capped run + report + survivor dispositions | no UX plan in origin | M |
| C8 | §3-C8 | `feat/state-restore-stack` | #356 RestoreStack foundation | no UX plan in origin (engine-side) | L |
| C9 | §3-C9 | `feat/state-per-board-undo` | #355 per-board undo line scoping | no UX plan in origin (engine-side) | M |
| C10 | §3-C10 | `feat/cli-config-set-propagation` | #845 journey plan (gate) + config-set propagation | **journey plan C10-JP — mandatory, gated** | M |
| C11 | §3-C11 | `feat/cli-boards-publish-to-files` | #764 journey plan (gate) + the new verb | **journey plan C11-JP — mandatory, gated** | M |

**Dependency graph**: C1 first (the law amendment; every later stage
benefits). C4 early (r8 carve-out — land before the friction chunk
rebases). Then the parallel wave **C2 ∥ C3 ∥ C5 ∥ C7**, plus C10.1
and C11.1 (the journey-writing stages — their gates pause
independently). **C6** after the wave (shares app-test files with
C2/C3; state/storage call sites with C8). **C8 → C9** strictly serial
(same crate; C9 scopes what C8 builds). **C10.2/C11.2** only after
their operator gates clear. Default dispatch order:
**C1 → C4 → (C2 ∥ C3 ∥ C5 ∥ C7 ∥ C10.1 ∥ C11.1) → C6 → C8 → C9 →
C10.2–3 → C11.2–3**.

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); independent chunks may run concurrently as
separate sessions; never two agents share a worktree. Every chunk
works in `../taskboard-WT-<flat-suffix>` (branch `/` → `-`, AGENTS
§8/#395), branched off the then-current `main`, no PR ever stacked.
Branch names are never reused after their PR lands — a resume works
on `<branch>-v2`. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<flat-suffix>` (never without `-b`, never on
`main`). Disk hygiene: every agent runs `scripts/worktree-doctor.sh`
from the main checkout first — nonzero exit stops the chunk.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding for every chunk, every stage —
`docs/agent_dispatch.org`; PR #852, amended by #879's flock-deferral
rule)*: a subagent is killed after ~600s without observable activity,
so long silent commands kill working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). **Flock-deferral rule (#879, codified by C1): when
   the gate lock is contended and no other useful work remains, never
   block — commit all landed work, report "gate deferred: lock held by
   <holder>", and end the stage; the dispatcher runs the gate.** The
   dispatcher runs the full AGENTS §10 suite in its own session
   between stage 2's landing and stage 3's ship; CI's Code Coverage
   check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures (the per-chunk closure lists are in §12).
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim* (every prompt
   below carries the [D] block).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (`git log origin/main..HEAD` — commits present, tree clean, gate
   evidence re-runnable). A stalled or quota-cut stage is completed
   from its landed commits or resumed with a state-check message
   ("commit X is landed; continue from Y") — **resume over redo**,
   never re-dispatched blind. Failure playbook: `docs/agent_dispatch.org` §8.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: creates the worktree, writes the
   TDD-first chunk plan at `.artifacts/plans/<today>-<topic>-plan.md`
   (C10/C11 additionally write the UX journey plan per §7–§11 of this
   document), resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push.
   Round-plan sections marked normative bind; deviations need a
   recorded reason.
2. **Stage 2 — implementation/research agent**: continues in that
   worktree (recreates it on the branch if missing — landed branch ⇒
   `<branch>-v2`), executes per the approved plan — TDD where code is
   involved, every commit standalone (rebase-merge hygiene), gate =
   pre-commit hooks + flock-wrapped nextest (defer per the rule if
   contended). Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main` (fetch
   --prune, rebase onto `origin/main`, drop already-merged duplicates,
   resolve only real conflicts, push `--force-with-lease` if history
   rewritten), then reviews the branch diff versus the post-rebase
   base against the chunk's acceptance criteria + AGENTS rules + the
   applicable mandate, re-runs the flock-wrapped nextest gate (defer
   per the rule if contended), implements fixes on the branch
   (amend/reword, never duplicate-subject fixups). Last tasks: push,
   open the PR to `main` with `gh`, report the URL + per-acceptance
   verdicts. **It does not watch CI** — the dispatcher takes over.

**Code-quality mandate** (stage 3, code chunks C2–C6, C8–C11 —
verbatim): beyond functional review, evaluate (a) code quality:
naming, comment discipline (comments state constraints, not
narration), idiomatic use of the approved stack, no dead code or
speculative generality, test quality per AGENTS §5; (b) the Single
Responsibility Principle: the crate-role boundaries of AGENTS §2 are
the SRP line — a boundary violation is an SRP violation; (c) a single
level of abstraction per function body: a function either
orchestrates named operations or works in primitive data, never
mixes. Findings are fixed on the branch like any other finding.

**Evidence & artefact mandate** (stage 3, C7 — verbatim): every
factual claim carries evidence (the command run + observed outcome,
or a verifiable source + what it states) and experiments are
reproducible from the report's method section; recommendations are
decision-grade (options, trade-offs, exactly one recommendation,
consequences for later work); repo hygiene (scratch code and captured
payloads never land — evidence is excerpts inside the report; the PR
diff is exactly the chunk's artefacts); scope discipline
(out-of-scope ideas go to the backlog per its template, not into the
report).

**UX completeness mandate** (stage 3, C10/C11 — verbatim): re-run
`scripts/ux-coverage.sh <journey-plan> --gate` (new-format step tokens
are REQUIRED rows; MISSING on a claimed row is not done); execute the
walkthrough protocol (`docs/ux_planning.org`) over every P1 story —
the four walkthrough questions per step with evidence, then the
six-heuristic pass over the touched CLI surface; BLOCKER/MAJOR
findings are fixed on the branch, MINOR findings become backlog
issues. Reports a verdict.

## 2. Chunk C1 — #879 law codification (`chore/dispatch-law-flock-deferral`) — COMPRESSED

- **Covers**: round plan §3-C1 normative. Runs **compressed**
  (`docs/agent_dispatch.org` §7 — ≤4 files, docs-only): the
  dispatcher is planner and implementer; the compression choice is
  stated in the committed chunk plan
  (`.artifacts/plans/2026-11-17-dispatch-law-flock-deferral-plan.md`).
- **Work**: add the flock-deferral rule to `docs/agent_dispatch.org`
  §2 (rule + trigger condition: "lock contended AND no other useful
  work remains") and a §8 playbook row (signal: agent died while the
  lock was held by another session; resolution: landed work intact,
  gate re-run by dispatcher); add the one-line deferral clause to the
  discipline block in `.artifacts/templates/dispatch-plan.md` so
  every future dispatch plan inherits it. AGENTS.md §7 stays
  as-is. PR body references #879 (dispatcher closes at merge).
- **Acceptance**: the rule's wording is consistent across both files
  (same trigger condition, same deferral action); no other law
  wording drift; pre-commit hooks green (docs-only; the dispatcher
  still runs the full §10 suite before handing the PR over, per house
  law).

## 3. Chunk C2 — #857 + #843 (`test/proptest-persistence-fixture-clock`)

- **Covers**: round plan §3-C2 normative — artifact persistence
  mechanism + canary proof; #843 option A (fixture clock anchoring);
  the #650 mechanism comment; fallbacks only as recorded.
- **Acceptance**: (a) the persistence mechanism uploads the
  `proptest-regressions` state on CI failure with ≥30-day retention
  and the artifact link surfaces in the job summary — proven by the
  recorded canary evidence; (b) `us6_s2_today_row_menu_and_preset_key_recommit`
  passes against `fixture::journey_state()` (option A landed) or the
  recorded option-B fallback with its "not injectable" finding;
  (c) #650 carries the armed-mechanism comment (issue stays open);
  (d) no production code change beyond the fixture/harness seams;
  (e) workflow diffs confined to the persistence mechanism.

**Prompt C2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C2 of the backlog-reduction round ("test-infra determinism", branch test/proptest-persistence-fixture-clock) in the taskboard repo at /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — never label/log/text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() or an injected clock, never sleeps; (c) never weaken a correct test; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = below the disk floor: STOP and report), then git worktree add -b test/proptest-persistence-fixture-clock ../taskboard-WT-test-proptest-persistence-fixture-clock off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C2 (normative), the dispatch plan .artifacts/plans/2026-11-17-backlog-reduction-dispatch-plan.md §1/§3, issues #857, #843, #650, and the CI workflow files (.github/workflows/*) plus docs/testing_strategy.org §6.1 (the anchor rule #843's option A cites).
2. Read the seams: where proptest writes regressions (crates/taskboard-domain/proptest-regressions/), the workflow's test steps (where an on-failure upload step attaches), the journey fixture (fixture::journey_state vs fixture::today_overdue_state — the wall-clock anchor vs the harness day-window clock) and the failing-origin test us6_s2_today_row_menu_and_preset_key_recommit (crates/taskboard-ui-slint/tests/journeys.rs).
3. Decide: (a) the persistence mechanism (default: actions upload-artifact on failure, retention >=30 days, link in the job summary — or a recorded better alternative), the canary proof method (deliberately-failing run captured, then the canary disabled/removed — record which survives); (b) #843's option-A design: which clock seam journey_state anchors on, the injection path, and why the harness day window and the fixture now agree by construction; option B only with the recorded "not injectable" finding.
4. Write the chunk plan at .artifacts/plans/2026-11-17-proptest-persistence-fixture-clock-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the proptest persistence and fixture clock chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt C2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C2 of the backlog-reduction round (branch test/proptest-persistence-fixture-clock).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; never block on a contended lock — defer to the dispatcher per the rule).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock in timing-dependent fixtures; (c) never weaken a correct test — the journey test's assertions stay, the fixture changes; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C2 (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-17-proptest-persistence-fixture-clock-plan.md in ../taskboard-WT-test-proptest-persistence-fixture-clock (recreate on the branch if missing; landed branch ⇒ test/proptest-persistence-fixture-clock-v2).
2. Implement exactly per the plan: the on-failure persistence step (+ canary proof per the recorded method — capture the evidence, then apply the recorded post-canary disposition), the #843 fixture clock anchoring (option A) with the journey test green against journey_state, the #650 mechanism comment drafted (posted at stage 3 per the plan's recorded choice). Every commit standalone; diff confined to the workflow + fixture/harness seams per the plan.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, canary evidence, per-gate results, deviations.
```

**Prompt C2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C2 of the backlog-reduction round (branch test/proptest-persistence-fixture-clock).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (b) no sleeps; (c) the journey test's assertions are byte-identical to origin/main — only the fixture's clock anchoring changed; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C2 (normative), the dispatch plan §1/§3 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-proptest-persistence-fixture-clock.
2. Functional focus — this chunk's traps: (a) the upload step fires on failure only and cannot mask a red job green (check its condition placement); (b) the canary's evidence exists and the recorded post-canary disposition was applied (a permanently failing canary in the diff is a finding); (c) the #843 fix changed the fixture's clock anchor, not the day-window logic and not the test — diff hunks outside the fixture seam are findings; (d) the #650 comment was posted (or its draft state matches the plan) — the issue is still open; (e) retention >=30 days and the artifact path matches where proptest actually writes.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR to main with gh (body references #857 #843 — closures are the dispatcher's at merge), report the URL + per-acceptance-item verdicts. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdicts.
```

## 4. Chunk C3 — #697 + #570 (`test/flaky-diagnosis-r2`)

- **Covers**: round plan §3-C3 normative — the budget-inequality fix
  with named margin, CI gap telemetry, the #570 relay diagnosis.
- **Acceptance**: #697 — the delayed route's window is strictly
  shorter than the post-stall wait budget by the named multiple
  (recorded, retiring the thin-headroom finding), the cli_tty CI job
  runs with `TASKBOARD_PTY_TEST_GAP_LOG` on; #570 — the diagnosis
  recorded, the fix on whichever side owns the bug, typed-outcome
  asserted; no sleeps, no weakened assertions; closures on fix
  evidence or dated armed-hypothesis comments (honest not-fixed-yet).

**Prompt C3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C3 of the backlog-reduction round ("flaky diagnosis r2", branch test/flaky-diagnosis-r2) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). Reproduction loops run backgrounded with useful work between polls. When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) stabilization makes tests more deterministic, never more permissive — no sleeps, no loosened timeouts-as-fix, no deleted assertions; floors not ceilings; (c) a flaky test can be a correct test catching a production race — evaluate both sides; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/flaky-diagnosis-r2 ../taskboard-WT-test-flaky-diagnosis-r2 off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 in full), the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C3 (normative), the dispatch plan §1/§4, issues #697 (the dated watchlist records: the 2026-11-11 CI capture, the refined post-stall hypothesis, the capture policy) and #570 (both instance-locked recurrences).
2. Read the seams: crates/taskboard-app/tests/cli_tty.rs — the stalled-line test's wiremock delayed route (its designed 10 s window), the post-stall assertion's wait budget (~line 1687 at capture time; re-anchor by symbol), the budget-derivation constants (the 3x-thin-headroom finding), the PTY gap telemetry env (TASKBOARD_PTY_TEST_GAP_LOG); the cli_ipc relay export path for #570 (the relay's instance locking vs the test's serialized-export assumption).
3. Decide: (a) the reordered inequality for #697 — window and wait budget with an explicit named margin/multiple (e.g. wait >= k * window + slack, k recorded with rationale), plus the CI telemetry enablement (which job, env var, log capture destination); (b) the #570 diagnosis method and both candidate fixes (test premise vs relay race) so stage 2 can land either.
4. Write the chunk plan at .artifacts/plans/2026-11-17-flaky-diagnosis-r2-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the flaky diagnosis r2 chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the reordered budgets, the #570 diagnosis plan, risks, estimated diff size.
```

**Prompt C3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C3 of the backlog-reduction round (branch test/flaky-diagnosis-r2).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; never block on a contended lock — defer per the rule). Reproduction/stability loops (repeated runs of the target tests) run backgrounded with useful work between polls.
[L] Round laws (binding): (a) typed outcomes only; (b) more deterministic, never more permissive — no sleeps, no relaxed timeouts-as-fix, no deleted assertions; (c) evaluate both sides; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C3 (normative), the dispatch plan §1/§4, and the stage-1 plan .artifacts/plans/2026-11-17-flaky-diagnosis-r2-plan.md in ../taskboard-WT-test-flaky-diagnosis-r2 (recreate on the branch if missing; landed branch ⇒ test/flaky-diagnosis-r2-v2).
2. Implement exactly per the plan: the #697 budget reorder + margin + CI telemetry enablement; the #570 fix per the diagnosis (whichever side owns the bug — the plan records both candidates). Per-issue commits; stability evidence per the plan's recorded method (backgrounded repeated runs); diff confined to the diagnosed seams.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, stability evidence, per-gate results, deviations.
```

**Prompt C3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C3 of the backlog-reduction round (branch test/flaky-diagnosis-r2).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (b) any sleep introduced or assertion weakened anywhere in the diff is a finding — compare every test hunk against origin/main; (c) no quality ceilings; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C3 (normative), the dispatch plan §1/§4 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-flaky-diagnosis-r2.
2. Functional focus — this chunk's traps: (a) the margin is an explicit named constant/derivation, not a magic relaxation of one timeout — audit that the budget inequality is structurally ordered (window < wait by construction), not just numerically currently-true; (b) the telemetry change touches the CI job definition only as far as the plan recorded; (c) the #570 fix is on the diagnosed side with a typed assertion pinning it — a blame-the-test fix without diagnosis evidence is a finding; (d) no other cli_tty/cli_ipc test changed.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #697 #570 — closures are the dispatcher's at merge per the disposition evidence), report the URL + per-issue verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict.
```

## 5. Chunk C4 — #860 + #868 (`fix/audit-finding-seams`)

- **Covers**: round plan §3-C4 normative — the notice seam fix per the
  #860 issue prompt; the CachedAppData fetch-time seam + actor clock +
  skew-band guard (#868); the r8 carve-out check.
- **Acceptance**: a test proves the `TaskResurrected` notice surfaces
  through `observe_connector_episode` (early-return path runs
  `push_notice`, or the recorded seam decoupling); #868's band test
  (guard fires inside the skew band, silent outside; pause/advance or
  injected clock); stage-1's disjointness check against the six
  in-flight r8 branches recorded; diffs confined to the named seams;
  no behavior change outside the guard band.

**Prompt C4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C4 of the backlog-reduction round ("audit-finding seams", branch fix/audit-finding-seams) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock — never sleeps; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/audit-finding-seams ../taskboard-WT-fix-audit-finding-seams off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C4 (normative, including the r8 carve-out), the dispatch plan §1/§5, issues #860 (the fix prompt with the seam line numbers) and #868 (the row-5 verdict + hardening prerequisite).
2. Read the seams: crates/taskboard-ui-slint/src/lib.rs observe_connector_episode (~979), adapter.refresh's unchanged early-return (~1531–1539) vs the tick loop's unconditional push_notice (~671) and adapter.rs:5013's notice record; taskboard-sync-nextcloud actor.rs's row-5 skip (cached.last_modified == candidate.last_modified && filesize equal), CachedAppData's fetch path (where the fetch-time seam goes), and the actor's clock source (what #133's tolerance constant already uses).
3. FIRST verify the r8 carve-out: list the in-flight r8 branches (git branch -r | grep ui-r8 or the round-8 dispatch plan's branch seeds) and record symbol-level file overlap with your planned diff; if the friction chunk's branch already touches the early-return site, record "fold #860 into r8" as your recommendation and STOP planning the #860 half (the #868 half proceeds) — the dispatcher decides at the gate review.
4. Decide: (a) the #860 fix shape (early-return runs push_notice, matching the tick loop — or the recorded decoupling alternative with rationale); the test that pins the notice surfacing; (b) #868's seam design (fetch-time seam on CachedAppData, actor clock injection) and the band guard (which constant bounds the band — reuse #133's named tolerance, do not introduce a second).
5. Write the chunk plan at .artifacts/plans/2026-11-17-audit-finding-seams-plan.md. Resolve every open question with a recorded decision.
6. Commit as: docs(plan): the audit-finding seams chunk plan. Do not implement; do not push.
7. Backlog issues per [L](d); list the numbers.

Report: plan path, the carve-out verdict, decisions, risks, estimated diff size.
```

**Prompt C4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C4 of the backlog-reduction round (branch fix/audit-finding-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; never block on a contended lock — defer per the rule).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock in the band test — never sleeps; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C4 (normative), the dispatch plan §1/§5, and the stage-1 plan .artifacts/plans/2026-11-17-audit-finding-seams-plan.md in ../taskboard-WT-fix-audit-finding-seams (recreate on the branch if missing; landed branch ⇒ fix/audit-finding-seams-v2). If the stage-1 carve-out verdict was "fold #860 into r8", implement the #868 half only.
2. Implement exactly per the plan, TDD: the notice-surfacing test red before the early-return fix, green after; the CachedAppData seam + actor clock + band guard with the time-controlled wiremock band test. Every commit standalone; diff confined to the named seams; new `// slint: <callback>` comments on any new slint seam (house convention, #782 fold).
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, red→green evidence, per-gate results, deviations.
```

**Prompt C4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C4 of the backlog-reduction round (branch fix/audit-finding-seams).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (b) no sleeps/wall-clock in the diff; (c) no weakened tests; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C4 (normative), the dispatch plan §1/§5 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-audit-finding-seams.
2. Functional focus — this chunk's traps: (a) the early-return fix must not push stale notices on every unchanged refresh (idempotence: a notice pushed once per event, not per tick) — the test must pin push-once semantics, and any test that would pass with push-per-tick is insufficient; (b) the band guard reuses #133's named tolerance constant — a second band constant is a finding; (c) the band test controls time explicitly and covers both arms (inside the band fires, outside stays silent); (d) no behavior change outside the guard band — the non-band corpus is unchanged.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #860 #868 — closures are the dispatcher's at merge; if the carve-out verdict folded #860 into r8, the body references only #868 and says so), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 6. Chunk C5 — #856 (`fix/domain-same-ref-collapse`)

- **Covers**: round plan §3-C5 normative — the structural no-op
  ruling, consumer verification, pinned units, unchanged property.
- **Acceptance**: same-ref candidate = no-op (bound member survives,
  `rebind` false, `remote_seen` never rolls backwards) unless
  stage-1's consumer verification records why fold-into-bound is the
  correct shape instead; unit tests pin the degenerate shape + the
  rewind-forbidden invariant; the #682 property suite exactly as
  landed (#864) green; no other merge behavior change.

**Prompt C5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C5 of the backlog-reduction round ("domain same-ref collapse", branch fix/domain-same-ref-collapse) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) n/a-timing; (c) never weaken a correct test — the #682 property premise stays exactly as PR #864 landed it; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/domain-same-ref-collapse ../taskboard-WT-fix-domain-same-ref-collapse off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C5 (normative), the dispatch plan §1/§6, and issue #856 (the shape, the proposed direction, the origin in #682's diagnosis).
2. Read the seams: crates/taskboard-domain/src/merge.rs collapse_for_task (~1660 at report time; re-anchor by symbol) — the survivor choice, the doomed set, the rebind flag; every consumer of collapse_for_task and the rebind/survivor decision (grep the workspace — a consumer relying on older-stamp-wins for same-ref is the finding that would change the ruling); the #682 proptest premise and its regressions entry.
3. Decide: no-op vs fold-into-bound (the round plan binds no-op as the ruling; fold-into-bound only with a recorded consumer-driven reason — a deviation needs the reason verbatim in the chunk plan); the exact unit tests pinning the shape (same-ref older-stamp candidate: no rewind, no doomed entries, bound survives) and the invariant test (remote_seen monotonicity across a collapse sequence).
4. Write the chunk plan at .artifacts/plans/2026-11-17-domain-same-ref-collapse-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the domain same-ref collapse chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the ruling confirmation (or deviation + reason), risks, estimated diff size.
```

**Prompt C5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C5 of the backlog-reduction round (branch fix/domain-same-ref-collapse).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C5 (normative), the dispatch plan §1/§6, and the stage-1 plan .artifacts/plans/2026-11-17-domain-same-ref-collapse-plan.md in ../taskboard-WT-fix-domain-same-ref-collapse (recreate on the branch if missing; landed branch ⇒ fix/domain-same-ref-collapse-v2).
2. Implement exactly per the plan, TDD: the pinned unit tests red against current behavior, the no-op (or recorded fold) fix, green. Diff confined to the merge module + tests; every commit standalone; SPDX/forbid(unsafe) per crate rules on new files.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-gate results, red→green evidence, deviations.
```

**Prompt C5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C5 of the backlog-reduction round (branch fix/domain-same-ref-collapse).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) the #682 property and its regressions entry are byte-identical to origin/main — any hunk touching them is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C5 (normative), the dispatch plan §1/§6 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-domain-same-ref-collapse.
2. Functional focus — this chunk's traps: (a) the no-op must not silently swallow a candidate that is genuinely duplicate-but-different-ref — audit the ref-equality branch's exact predicate; (b) remote_seen monotonicity is pinned by a test, not just by the unit shape; (c) no other merge path changed (diff confined to the same-ref branch + tests); (d) the consumer verification is recorded in the plan (or the deviation reason is).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #856 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 7. Chunk C6 — #865 + #876 + #877 (`test/app-storage-hygiene`)

- **Covers**: round plan §3-C6 normative — #865 option 2 (file-layer
  decode + integration twin), #876 unconditional dead-param drop,
  #877 shared rewind helper (cross-check test as recorded fallback).
- **Acceptance**: the hostile-env full-workspace run green with the
  ~12 unit tests on file-layer decode; exactly one integration twin
  carries composed-load; the `#[allow(unused_variables)]` attrs and
  dead params gone, clippy-clean; the fixtures migrated onto the
  helper and the throwaway-migration evidence recorded on #877; diff
  confined to the named seams.

**Prompt C6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C6 of the backlog-reduction round ("app/storage hygiene", branch test/app-storage-hygiene) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (c) never weaken a correct test — option 2 reroutes assertions, it does not delete coverage; the composed-load behavior must remain covered exactly once (the integration twin); (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/app-storage-hygiene ../taskboard-WT-test-app-storage-hygiene off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C6 (normative — the option-2 and shared-helper decisions bind), the dispatch plan §1/§7, and issues #865, #876, #877.
2. Read the seams: the ~12 ambient-sensitive AppConfig::load unit tests (issue #865 names three; sweep for the rest) and the file-layer decode helpers (extract_file_only/extract_raw); the config integration suite's env lock (where the twin lands); apply_sync_report/apply_files_board_snapshot's dead author params + the state actor's Some(author) call sites (#876); the four hand-edited fixtures (connect.rs 0004/0015/0017, roundtrip.rs pre-0023) and the migration ledger (#877's declarative table source).
3. Decide: the twin's home (which integration test file) and its exact composed-load scenario; the helper's API (rewind_to(conn, N) + the declarative strip table shape — generated from the ledger where feasible; the cross-check-test fallback only with the recorded infeasibility finding); the #876 call-site inventory.
4. Write the chunk plan at .artifacts/plans/2026-11-17-app-storage-hygiene-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the app storage hygiene chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the twin/helper designs, the call-site inventory, risks, estimated diff size.
```

**Prompt C6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C6 of the backlog-reduction round (branch test/app-storage-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (c) rerouted assertions keep the same semantics at the file layer — a weakened assertion is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C6 (normative), the dispatch plan §1/§7, and the stage-1 plan .artifacts/plans/2026-11-17-app-storage-hygiene-plan.md in ../taskboard-WT-test-app-storage-hygiene (recreate on the branch if missing; landed branch ⇒ test/app-storage-hygiene-v2).
2. Implement exactly per the plan: #865 reroute + twin; #876 param drop + call sites (remove the allow attrs); #877 helper + fixture migration + the throwaway-migration evidence run per the plan's recorded method. Per-issue commits; every commit standalone; diff confined to the named seams.
3. Prove per the plan: the hostile-env run (TASKBOARD_* hostile values) green workspace-wide.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, hostile-env evidence, per-gate results, deviations.
```

**Prompt C6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C6 of the backlog-reduction round (branch test/app-storage-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) composed-load coverage is exactly-once — the twin exists and no unit test still asserts env-composable fields through the composed provider; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C6 (normative), the dispatch plan §1/§7 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-app-storage-hygiene.
2. Functional focus — this chunk's traps: (a) the reroute actually bypasses the env provider (extract_file_only/extract_raw) — a reroute that still composes figment's env layer is the bug this chunk exists to kill; (b) the twin covers the composed-load path with the env lock (not a second file-layer copy of the same assertions); (c) #876's removal is complete — grep for the params and the allow attrs; the state actor compiles clean without Some(author); (d) #877's helper is the single rewind path — a fixture still carrying its own hand strip list is a finding unless the plan recorded the fallback; the throwaway-migration evidence is on #877.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #865 #876 #877 — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 8. Chunk C7 — #327 (`ci/cargo-mutants-capped`) — research kind

- **Covers**: round plan §3-C7 normative — the capped one-off over
  `commands/` + `ipc/`, the report, survivor dispositions.
- **Deliverable**: the mutants report
  (`.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md`)
  + any trivial-kill test fixes; nothing else in the diff.
- **Acceptance**: method + cap recorded before the run; every
  survivor dispositioned (backlog issue or in-round trivial kill with
  its failing-test-first evidence); no mutant-driven production
  change without the failing test landing first; the run's full log
  evidence excerpted in the report (not committed raw).

**Prompt C7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C7 of the backlog-reduction round ("cargo-mutants capped", branch ci/cargo-mutants-capped) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): the mutants run is exactly the kind of long silent command the law governs — it runs BACKGROUND in your own session with short interleaved polls and useful work between polls, and all landed work is committed BEFORE the run starts. Your gate is the pre-commit hooks; the flock-wrapped nextest runs deferred per the rule if contended.
[L] Round laws (binding): (a) evidence-carrying report claims; (c) no mutant-driven production change without a failing test first (TDD order); (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b ci/cargo-mutants-capped ../taskboard-WT-ci-cargo-mutants-capped off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C7 (normative), the dispatch plan §1/§8, and issue #327 (its cap spec is normative — time/budget bound, commands/ + ipc/ first).
2. Read the seams: crates/taskboard-app/src/commands/ and src/ipc/ (module inventory, test coverage shape), the existing test suites covering them, and any prior mutants tooling in the repo (scripts/, CI) to reuse.
3. Decide: the exact cap (from the issue + your module-count arithmetic — record the arithmetic), the tool invocation (cargo-mutants with the file/exclude set), the survivor triage threshold for in-round trivial kills (the round plan suggests <=10-line kill tests; record yours), and the report skeleton.
4. Write the chunk plan at .artifacts/plans/2026-11-17-cargo-mutants-capped-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the cargo-mutants capped chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the cap arithmetic, risks.
```

**Prompt C7.2 (research/execution):**

```text
You are the stage-2 RESEARCH agent for chunk C7 of the backlog-reduction round (branch ci/cargo-mutants-capped).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): the mutants run goes BACKGROUND with short interleaved polls and useful work between polls (draft the report skeleton, triage completed shards); all landed work is committed BEFORE the run starts; never block on a contended flock — defer per the rule.
[L] Round laws (binding): (a) evidence-carrying claims; (c) trivial kills are TDD — the failing test lands before the production change; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C7 (normative), the dispatch plan §1/§8 (the evidence & artefact mandate applies to you in full), and the stage-1 plan .artifacts/plans/2026-11-17-cargo-mutants-capped-plan.md in ../taskboard-WT-ci-cargo-mutants-capped (recreate on the branch if missing; landed branch ⇒ ci/cargo-mutants-capped-v2).
2. Execute per the plan: the capped run (backgrounded, polled), the report (.artifacts/reports/2026-11-17-cargo-mutants-taskboard-app-report.md — method, cap, arithmetic, results, survivor triage with evidence excerpts), and the dispositioned survivors: trivial kills as TDD test commits, everything else as `backlog:` issues (numbers in the report).
3. Gate: pre-commit hooks on every commit; the flock-wrapped nextest after any test commits (defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: run outcome vs cap, survivor count + dispositions, deviations.
```

**Prompt C7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C7 of the backlog-reduction round (branch ci/cargo-mutants-capped).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): commit landed work BEFORE any wait; defer gate runs per the rule if contended. Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) spot-check the report's claims against the run evidence (re-run a shard if a claim looks off); (c) every production hunk has its failing-test commit before it; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C7 (normative), the dispatch plan §1/§8 (the evidence & artefact mandate applies to you in full — verbatim in §1 of this dispatch plan), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-ci-cargo-mutants-capped.
2. Functional focus — this chunk's traps: (a) the diff is exactly the report + the trivial-kill tests (+ their production fixes) — anything else is a finding; (b) the cap was honored (arithmetic vs actual runtime/mutant count in the report); (c) every survivor is dispositioned with a traceable outcome (issue numbers or in-round kill); (d) no raw logs committed.
3. Evidence & artefact mandate (verbatim, §1 of this dispatch plan).
4. Pre-commit hooks + the flock-wrapped nextest are your gate (defer per the rule if contended); the dispatcher runs the full §10 suite.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #327 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 9. Chunks C8 + C9 — #356 → #355 (undo-v2 state-side, serial)

- **C8 covers** (round plan §3-C8 normative): the RestoreStack
  foundation in `taskboard-state` (+ domain message types): stack
  semantics (depth, cascade granularity, interaction with the
  recent-window undo), ceremony fences intact (`NotUndoable`
  ceremony verbs stay — the r1 erratum ruling, #842's supersession),
  snapshot corpus extended. **Acceptance**: typed undo outcomes; no
  `RwLock`/`Mutex` (AGENTS §3); insta snapshots over command
  sequences; proptest for cascade invariants where pure; the UI legs
  (#353/#354) untouched.
- **C9 covers** (round plan §3-C9 normative): per-board undo line
  scoping over C8's stack. **Acceptance**: same laws; scoping
  behavior pinned by snapshots; C8's corpus still green.

**Prompt C8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C8 of the backlog-reduction round ("RestoreStack foundation", branch feat/state-restore-stack) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance for any time-dependent undo-window test — never sleeps; (c) never weaken a correct test — the existing undo/journal snapshot corpus is extended, not rewritten; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/state-restore-stack ../taskboard-WT-feat-state-restore-stack off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C8 (normative), the dispatch plan §1/§9, and issue #356 (plus the family ordering from the cleanup round: #356 -> #355 -> #354 -> #353; #357 exploratory).
2. Read the seams: the state engine's undo/journal machinery (journal.rs, the recent-window undo, the AppUndo path), the ceremony fences (NotUndoable(BoardCeremony) — the r1 erratum PC-1/PC-5 supersession note), the command enums the UI legs will eventually drive, and the undo snapshot tests (insta) corpus.
3. Decide the chunk plan's core: RestoreStack semantics (what enters the stack, depth/bound, cascade granularity for cascaded-task restore, per-board lines as the C9 seam — design the data shape now so C9 scopes without reworking), the message/engine-command surface (typed outcomes for stack ops), the interaction rules with the recent-window undo (ordering, mutual exclusion or composition — recorded), and the ceremony-fence invariants (which verbs never enter the stack).
4. Write the chunk plan at .artifacts/plans/2026-11-17-state-restore-stack-plan.md with the test catalogue (snapshots + proptest invariants). Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the restore stack foundation chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the semantics decisions, risks, estimated diff size.
```

**Prompt C8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C8 of the backlog-reduction round (branch feat/state-restore-stack).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance — never sleeps; (c) the existing corpus is extended, not rewritten — a changed existing snapshot without the plan's recorded diagnosis is a finding you must not create; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C8 (normative), the dispatch plan §1/§9, and the stage-1 plan .artifacts/plans/2026-11-17-state-restore-stack-plan.md in ../taskboard-WT-feat-state-restore-stack (recreate on the branch if missing; landed branch ⇒ feat/state-restore-stack-v2).
2. Implement exactly per the plan, TDD (snapshot + proptest catalogue first where pure). Engine-side only: taskboard-state + the domain message types the plan names — no UI crate changes, no #353/#354 work. Cross-actor communication per AGENTS §3 (mpsc/broadcast/oneshot; no RwLock/Mutex). Every commit standalone; SPDX/forbid(unsafe) per crate rules.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-gate results, snapshot-corpus delta summary, deviations.
```

**Prompt C8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C8 of the backlog-reduction round (branch feat/state-restore-stack).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) changed existing snapshots each need the plan's recorded diagnosis — unexplained snapshot churn is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3-C8 (normative), the dispatch plan §1/§9 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-state-restore-stack.
2. Functional focus — this chunk's traps: (a) the ceremony fence holds: no ceremony-guarded verb (board archive/delete/move class) enters the RestoreStack — trace every stack-entry site against the NotUndoable classification; (b) the recent-window undo interaction is as the plan recorded (composition or exclusion) — an undocumented interaction path is a finding; (c) no RwLock/Mutex entered the engine (grep the diff); (d) the C9 seam exists as designed (per-board lines derivable without reworking C8's data shape); (e) no UI crate imports the new surface yet.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code or speculative generality, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #356 — closure is the dispatcher's at merge; the body notes C9 depends on this landing), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

*(C9's prompts mirror C8's exactly, with these substitutions: branch
`feat/state-per-board-undo`, worktree
`../taskboard-WT-feat-state-per-board-undo`, plan
`2026-11-17-state-per-board-undo-plan.md`, origin sections §3-C9,
issue #355; precondition line in C9.1: "verify #356's PR is merged
(`gh pr list --state merged` or the issue state) — if not, STOP and
report instead of planning"; stage-1 re-anchors on C8's landed
RestoreStack shape; C9.3's body references #355 and notes the
dependency chain for the dispatcher's closure.)*

## 10. Chunk C10 — #845 (`feat/cli-config-set-propagation`) — journey-gated

- **Covers**: round plan §3-C10 normative. **Gate mechanics**: stage
  1 produces (a) the UX journey plan
  (`.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md`,
  from `.artifacts/templates/ux-journey-plan.md`, citing
  `docs/ux_planning.org` law) and (b) the chunk plan; the dispatcher
  then **pauses for the operator's journey acceptance** — stage 2
  dispatches only after acceptance. Implementation: the CLI write
  path + the engine/config seam the accepted journey names.
- **Acceptance**: the accepted journey's P1 stories walked at stage 3
  (UX completeness mandate); the divergence window from the issue is
  closed or surfaced per the accepted design; typed outcomes; the
  existing config-set tests (us10_s1 et al) green under the hostile
  env from C6's hygiene; `scripts/ux-coverage.sh <journey-plan>
  --gate` green on the claimed rows.

**Prompt C10.1 (planning — writes the journey plan; the gate follows):**

```text
You are the stage-1 PLANNING agent for chunk C10 of the backlog-reduction round ("config-set propagation", branch feat/cli-config-set-propagation) in /home/viktoria/src/taskboard. Your chunk is JOURNEY-GATED: after your stage, the dispatcher pauses for the operator's acceptance of the UX journey plan — you do not implement, and nothing proceeds until the gate clears.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks (docs-stage); commit all landed work BEFORE any wait.
[L] Round laws (binding): (a) the journey plan is written from .artifacts/templates/ux-journey-plan.md and cites docs/ux_planning.org (the framework is mandatory — phase 6 active); (c) recorded decisions, never silent choices; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/cli-config-set-propagation ../taskboard-WT-feat-cli-config-set-propagation off the then-current main (never without -b, never on main).

1. Read AGENTS.md, docs/ux_planning.org (the framework law + journey-plan phases), the round plan .artifacts/plans/2026-11-17-backlog-reduction-round-plan.md §3-C10 (normative), the dispatch plan §1/§10, issue #845 (the divergence semantics), and one recent format exemplar (.artifacts/plans/2026-11-13-r7-*-ux-plan.md or a 2026-11-16-*-ux-plan.md).
2. Read the seams: the config-set CLI path (crates/taskboard-app commands/config), the config overlay merge (defaults -> taskboard.toml -> TASKBOARD_* env, figment), the running app's config consumption points (what a live knob write would need to reach), and the C6-era hostile-env tests that pin the file layer.
3. Design the journey: settle the semantics question the round plan posed (write-through to the running app vs explicit restart contract vs surfaced divergence) with a recorded recommendation and alternatives; enumerate the user-visible steps (US-n stories, S-k steps, W-x wiring rows, R-x rulings per the template); the CLI surfaces touched (verb flags, help text, typed errors).
4. Write BOTH: the UX journey plan (.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md) and the chunk plan (.artifacts/plans/2026-11-17-config-set-propagation-plan.md — TDD-first, referencing the journey's story/step ids). Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the config-set propagation journey and chunk plans. Do not implement; do not push. Report that the chunk is now GATE-PAUSED.
6. Backlog issues per [L](d); list the numbers.

Report: plan paths, the semantics recommendation, the gate-pause statement, risks, estimated diff size.
```

**Prompt C10.2 (implementation — dispatch only after the operator
accepts the journey):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C10 of the backlog-reduction round (branch feat/cli-config-set-propagation). GATE CLEARED: the operator accepted the journey plan — implement exactly its wiring inventory.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only — never help-text/label assertions; (b) pause/advance for any deferred-apply timing test; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the accepted journey plan .artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md (the US-n/S-k/W-x/R-x ids are normative), the chunk plan .artifacts/plans/2026-11-17-config-set-propagation-plan.md, and the dispatch plan §1/§10, in ../taskboard-WT-feat-cli-config-set-propagation (recreate on the branch if missing; landed branch ⇒ feat/cli-config-set-propagation-v2).
2. Implement exactly the accepted journey's wiring rows (W-x) — nothing beyond them. TDD; every commit standalone; embed journey-step tokens in test names; diff confined to the CLI path + the engine/config seam the journey named.
3. Self-check: run scripts/ux-coverage.sh .artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md --gate — MISSING on a claimed row is not done.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-row implementation state, gate output, per-gate results, deviations.
```

**Prompt C10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C10 of the backlog-reduction round (branch feat/cli-config-set-propagation).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852 + the #879 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) the accepted journey is the contract — implementation beyond its wiring rows is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the accepted journey plan + chunk plan + dispatch plan §1/§10 (the code-quality mandate AND the UX completeness mandate apply to you in full — both verbatim in §1 of this dispatch plan); rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-cli-config-set-propagation.
2. Functional focus — this chunk's traps: (a) the divergence window from #845 is actually closed or surfaced per the accepted design — construct the concurrent-knob scenario in review and check the observed behavior matches the journey's R-x rulings; (b) the env/file precedence is unchanged except where the journey named it; (c) us10_s1 and the connect overlay twins stay green under hostile env; (d) the journey-step tokens appear in the test names the wiring rows claimed.
3. Code-quality mandate (verbatim, §1) + UX completeness mandate (verbatim, §1): re-run scripts/ux-coverage.sh <journey-plan> --gate; execute the walkthrough protocol over every P1 story (four questions per step with evidence), then the six-heuristic pass over the touched CLI surface; BLOCKER/MAJOR fixed on the branch, MINOR to backlog issues. Verdict reported.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #845 — closure is the dispatcher's at merge), report the URL + verdicts. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings (functional + mandates), fixes, PR URL, walkthrough verdict.
```

## 11. Chunk C11 — #764 (`feat/cli-boards-publish-to-files`) — journey-gated

Identical gate mechanics to C10 (stage 1 writes
`2026-11-17-boards-publish-to-files-ux-plan.md` + the chunk plan; the
dispatcher pauses for operator acceptance; stages 2–3 proceed on the
accepted journey). C10 and C11's journeys may be written in parallel
(separate branches); their implementations are independent (different
verb families) and may also run in parallel after both gates clear.

*(C11's prompts mirror C10's verbatim with these substitutions:
branch `feat/cli-boards-publish-to-files`, worktree
`../taskboard-WT-feat-cli-boards-publish-to-files`, journey plan
`2026-11-17-boards-publish-to-files-ux-plan.md`, chunk plan
`2026-11-17-boards-publish-to-files-plan.md`, origin section §3-C11,
issue #764; seams: the boards publish verb family (Deck-first-then-move
boundary per the issue), the files-sync local-to-Files path the verb
exposes, the command-family conventions the REPL twins follow; the
semantics question settles the verb's contract per the issue's
recorded boundary — local-to-Files direct, no new remote path;
C11.2's coverage gate and C11.3's mandates point at C11's journey plan;
bodies reference #764.)*

## 12. Dispatcher runbook & at-merge closures (binding)

**Runbook** (per `docs/agent_dispatch.org`, as C1 amends it): between
every pair of stages verify landed state in the worktree (commits
present, tree clean, gate evidence re-runnable) — resume over redo on
stall/quota, playbook §8. Between stage-2 landing and stage-3 ship of
every code chunk: run the full AGENTS §10 sequence through
`scripts/run-quality-gates.sh` in your own session (background +
poll). At each stage-3 PR-open: `gh pr checks --watch` to green,
`mergeStateStatus` must report `CLEAN`, then teardown from the main
checkout with the absolute-path form and `git worktree list`
confirmation. At C10/C11's gates: verify the journey plan's presence
and dispatch-readiness, hand it to the operator, proceed only on the
recorded acceptance.

**At-merge closures** (dispatcher-owned, per chunk): C1 → #879; C2 →
#857, #843 (+ the #650 mechanism comment posted at stage 3, issue
stays open); C3 → #697/#570 or their dated armed-hypothesis comments
(per the disposition evidence); C4 → #860/#868 (or #868 only, if the
carve-out folded #860 into r8); C5 → #856; C6 → #865/#876/#877;
C7 → #327 (+ the survivors' backlog issues filed during the chunk);
C8 → #356; C9 → #355; C10 → #845; C11 → #764.

**Deliberately not dispatched here** (round plan §1/§4): #336/#287/#384
(watchlist stands), #353/#354/#357, r8 territory and its coverage
table, the perf seven, feature-scale items, the dependabot sweep, and
the operator-manual hosted evidence session (#695 re-verify + #867).

**What opens next**: the #843-armed #650 re-diagnosis on the next CI
recurrence (post-C2); #353/#354's journey planning once r8 completes;
the hosted-evidence findings' sync work if the matrices surprise; the
next backlog round sweeps whatever this round files.
