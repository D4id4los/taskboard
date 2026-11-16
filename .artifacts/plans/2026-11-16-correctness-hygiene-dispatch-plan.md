# Correctness & Hygiene Round (post-round-8) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-16
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation/research → review) for the 7 chunks of the accepted
  correctness & hygiene round: the #731 deny-advisory verdict, the
  #713 recur attribution fix, the flaky-watchlist dedicated pass, the
  config/test-harness hygiene chunk, the sync ETag-retention + skew
  fix, the Retry-After family, and the #708 sidecar-convergence
  design round. No code in this commit.
- **Origin**: the round plan
  `.artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md`
  (§2–§8 are the normative per-chunk specs; §1 excluded territory,
  §9 sequencing, §10 second-tier exclusions), accepted by the
  operator 2026-11-16 together with the recommendation report
  `.artifacts/reports/2026-11-16-post-round8-implementation-recommendation-report.md`.
- **Status**: READY — all 7 chunks dispatchable now. Chunk A runs
  **compressed** (dispatcher-owned, `docs/agent_dispatch.org` §7 — the
  verdict is a short investigation whose outcomes are issue comments ±
  a ≤2-file diff); chunks B–G run the full protocol. Deliberately not
  dispatched: the second-tier issues (#327, #356/#355, #845, #764,
  #843 — operator ruling: later backlog round), the perf-architecture
  seven, feature-scale items, all round-8 territory, and the C-6
  deferrals (round plan §1/§10).
- **Orchestration law**: `docs/agent_dispatch.org` (PR #852) binds
  every chunk. The dispatcher owns stage-exit verification, the full
  AGENTS §10 suite between stage-2 landing and stage-3 ship, the CI
  watch, teardown, and at-merge issue closures (§10 below). Stage
  prompts restate the long-command discipline verbatim.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| A | round plan §2 | `chore/deny-advisory-verdict` | #731 verdict fork (A/B/C) — issue evidence ± `Cargo.lock`/`[workspace.dependencies]`/`deny.toml` diff; non-goals: no other dependency moves | n/a — CI/security, exempt | S (compressed) |
| B | round plan §3 | `fix/domain-recur-done-window` | #713: partition-premise fix in recur `schedule`/`occurrence_outcome`, persisted seed + boundary units; non-goals: no other recur behavior change, no weakened property | n/a — domain, exempt | S/M |
| C | round plan §4 | `test/flaky-watchlist-pass` | the 8-issue flaky pass (§4 disposition matrix); non-goals: no production refactor beyond the diagnosed seams, no new features | n/a — tests, exempt | L |
| D | round plan §5 | `test/config-harness-hygiene` | #728/#729/#730/#607 + #833 read-only rider; non-goals: no production config changes, no helper rewrites beyond the named guards | n/a — tests, exempt | M |
| E | round plan §6 | `fix/sync-etag-skew` | #695+#133: characterization wiremock, validator design (a/b/c), skew tolerance constant, bounded 412 convergence; non-goals: no push-pipeline parallelism, no live-tier code | n/a — sync internals, exempt | L |
| F | round plan §7 | `fix/sync-retry-after` | #220/#221/#222: header honoring (both forms), floor/cap bounding, abort-path rescheduling; non-goals: no new retry states beyond the typed deferral outcome | n/a — sync internals, exempt | M |
| G | round plan §8 | `docs/sidecar-convergence-design` | #708 design round: evidence report + design plan, one recommendation, human gate; non-goals: zero production code | n/a — design docs, exempt | M |

**Dependency graph**: A first, alone (gate-trust prerequisite for
every later PR). B ∥ C ∥ D are mutually independent (domain / app
tests+scripts / cross-cutting test seams — stage 1 confirms
symbol-level disjointness against round-8 branches per round plan §1).
E → F serial (same crate; F's stage 1 re-anchors on E's landed
transport changes). G independent of everything (docs-only; may start
immediately). Default dispatch order: **A → (B ∥ C ∥ D ∥ G) → E → F**.

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); independent chunks may run concurrently as
separate sessions; never two agents share a worktree. Every chunk
works in `../taskboard-WT-<branch-suffix>` next to the project root
(flat suffix: `/` → `-`, AGENTS §8/#395), branched off the
then-current `main`, no PR ever stacked. Branch names are never
reused after their PR lands — a resume works on `<branch>-v2`. Create
worktrees with `git worktree add -b <branch> ../taskboard-WT-<suffix>`
(never without `-b`, never on `main`). Disk hygiene: every agent runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit stops the chunk.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding for every chunk, every stage —
`docs/agent_dispatch.org`; PR #852)*: a subagent is killed after
~600s without observable activity, so long silent commands kill
working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). The dispatcher runs the full AGENTS §10 suite in
   its own session between stage 2's landing and stage 3's ship; CI's
   Code Coverage check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures (the per-chunk closure lists are in §10).
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
   TDD-first chunk plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Round-plan
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — landed branch ⇒
   `<branch>-v2`), executes per the approved plan — TDD, every commit
   standalone (rebase-merge hygiene), gate = pre-commit hooks +
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   backgrounded if slow. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main` (fetch
   --prune, rebase onto `origin/main`, drop already-merged duplicates,
   resolve only real conflicts, push `--force-with-lease` if history
   rewritten), then reviews the full branch diff versus the
   post-rebase base against the chunk's acceptance criteria + AGENTS
   rules + the code-quality mandate, re-runs the flock-wrapped nextest
   gate, implements fixes on the branch (amend/reword, never
   duplicate-subject fixups). Its last tasks: push, open the PR to
   `main` with `gh`, report the URL + a per-acceptance-item verdict.
   **It does not watch CI** — the dispatcher takes over there.

**Code-quality mandate** (stage 3, every code chunk — verbatim):
beyond functional review, evaluate (a) code quality: naming, comment
discipline (comments state constraints, not narration), idiomatic use
of the approved stack, no dead code or speculative generality, test
quality per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line — a boundary
violation is an SRP violation; (c) a single level of abstraction per
function body: a function either orchestrates named operations or
works in primitive data, never mixes. Findings are fixed on the
branch like any other finding.

**Evidence & artefact mandate** (stage 3, chunk G — verbatim): every
factual claim carries evidence (the command run + observed outcome, or
a verifiable source + what it states) and experiments are reproducible
from the report's method section; recommendations are decision-grade
(options, trade-offs, exactly one recommendation, consequences for
later work); repo hygiene (scratch code and captured payloads never
land — evidence is excerpts inside the report; the PR diff is exactly
the chunk's artefacts); scope discipline (out-of-scope ideas go to the
backlog per its template, not into the report).

**Prompt boilerplate** (restated inline in every prompt below —
abbreviated here as [D]/[L]/[R]/[B]; the full text sits inside each
prompt):

- **[D] Discipline**: "Long-command discipline (binding;
  `docs/agent_dispatch.org`, PR #852): the coverage trio never runs
  inside a subagent — your gate is the pre-commit hooks plus
  `scripts/run-quality-gates.sh cargo nextest run --workspace`
  (flock-safe, ~4–6 min; background + poll with useful work between
  polls beyond ~5 minutes, and commit all landed work BEFORE any
  wait). [Stage 3 only:] Your scope ends at PR-open: push, open the
  PR with gh, report the URL — the dispatcher owns `gh pr checks
  --watch`, the mergeStateStatus CLEAN verification, teardown, and
  at-merge issue closures."
- **[L] Laws**: "Round standing laws (binding): (a) typed outcomes
  only — never label/log/UI-text assertions (AGENTS §5); (b)
  time-dependent tests use `tokio::time::pause()`/`advance()` or an
  injected clock, never sleeps; (c) a new test failing is as likely a
  production bug as a test bug — evaluate both; never weaken a
  correct test; no quality ceilings (floors not ceilings); (d)
  out-of-scope findings become `backlog:` issues immediately, without
  asking — 'no push' never means 'no issues'; report the numbers."
- **[R] Doctor/worktree**: "Run `scripts/worktree-doctor.sh` from
  /home/viktoria/src/taskboard first (nonzero = below the disk floor:
  STOP and report), then `git worktree add -b <branch>
  ../taskboard-WT-<flat-suffix>` off the then-current main (never
  without -b, never on main — AGENTS §8)."
- **[B] Backlog**: "(= (d) of [L]; restated because agents skipped
  preamble-only statements — PR #852 §5.)"

## 2. Chunk A — the #731 verdict (`chore/deny-advisory-verdict`) — COMPRESSED

- **Covers**: round plan §2's fork, normative. Runs **compressed**
  (`docs/agent_dispatch.org` §7): the dispatcher acts as planner and
  implementer directly — this compression choice is stated here per
  the law. The dispatcher: commits a short verdict plan
  (`.artifacts/plans/2026-11-16-deny-advisory-verdict-plan.md`) first,
  runs `cargo deny check` on clean `main`, resolves the fork (Path A:
  evidence comment + close, no PR; Path B/C: the ≤2-file fix, gates
  `cargo deny check` + flock-wrapped nextest, PR), and reports.
- **Acceptance**: fork resolved with command output recorded on #731;
  if Path B/C: PR diff is exactly `Cargo.lock` and/or
  `[workspace.dependencies]` and/or `deny.toml`; an ignore path
  always pairs with the #284 register entry; full §10 suite by the
  dispatcher before the PR is handed to the operator if any manifest
  file changed.

## 3. Chunk B — recur attribution fix (`fix/domain-recur-done-window`)

- **Covers**: round plan §3 normative — the partition-premise fix,
  persisted seed, boundary unit tests, unchanged-corpus guarantee.
- **Acceptance**: the persisted case (`cc b6100c06…a3270`) passes via
  the committed regression entry; `stamps_attribute_to_exactly_one_member`
  passes unweakened; new Wed/Thu-Until-Thursday boundary units (anchor
  on first weekday / on non-member weekday) green; existing recur
  schedule corpus output byte-identical; no weakened test; PR diff
  confined to `crates/taskboard-domain` recur module + its tests.

**Prompt B.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B of the correctness-hygiene round ("recur attribution fix", branch fix/domain-recur-done-window) in the taskboard repo at /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only — never label/log/UI-text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() or an injected clock, never sleeps; (c) a new test failing is as likely a production bug as a test bug — evaluate both; never weaken a correct test; no quality ceilings; (d) out-of-scope findings become `backlog:` issues immediately, without asking; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = below the disk floor: STOP and report), then git worktree add -b fix/domain-recur-done-window ../taskboard-WT-fix-domain-recur-done-window off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§3 (normative), the dispatch plan .artifacts/plans/2026-11-16-correctness-hygiene-dispatch-plan.md §1/§3, and issue #713 (the repro seed and the proposed-fix prompt are in the body).
2. Read the seams: crates/taskboard-domain/src/recur.rs — `schedule`'s member enumeration and `occurrence_outcome`'s window attribution around the failing assertion (~line 1770 at report time; re-anchor by symbol); the property `stamps_attribute_to_exactly_one_member` in arithmetic_tests; crates/taskboard-domain/proptest-regressions/recur.txt (existing format).
3. Replay the persisted seed first: add it to the regressions file, run the property, confirm the failure reproduces — the diagnosis works from a red test.
4. Diagnose which side violates the partition premise (`schedule` enumerating a member before the anchor's own window when the anchor's weekday is not the pattern's first weekday, vs `occurrence_outcome` attributing across the window boundary) and record the decision with evidence.
5. Write the TDD-first chunk plan at .artifacts/plans/2026-11-16-recur-attribution-fix-plan.md: the diagnosis, the chosen fix side, the exact assertion(s) to keep unweakened, the new boundary unit tests (anchor on first weekday; anchor on non-member weekday; Until bound on a member day), and the corpus-unchanged guarantee (how you will prove existing schedule output is byte-identical). Resolve every open question with a recorded decision.
6. Commit as: docs(plan): the recur attribution fix chunk plan. Do not implement; do not push.
7. [B] Backlog issues per [L](d); list the numbers in your report.

Report: plan path, the diagnosis decision, risks, estimated diff size.
```

**Prompt B.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B of the correctness-hygiene round ("recur attribution fix", branch fix/domain-recur-done-window).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only — never label/log/UI-text assertions; (b) never sleeps in tests; (c) never weaken a correct test — the property's assertion stays as written; no quality ceilings; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3 (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-16-recur-attribution-fix-plan.md in the worktree ../taskboard-WT-fix-domain-recur-done-window (recreate on the branch if missing; landed branch ⇒ fix/domain-recur-done-window-v2 per AGENTS §8; never git worktree add <path> main).
2. Implement exactly per the approved plan, TDD: regression seed committed and red, fix, unit tests green. Every commit standalone (rebase-merge hygiene). Diff confined to the recur module + tests (crates/taskboard-domain). SPDX headers on any new file; the crate already forbids unsafe.
3. Prove the corpus-unchanged guarantee per the stage-1 plan's recorded method (snapshot/diff of schedule output over the existing test corpus) — any existing-snapshot change is a finding to diagnose, not a snapshot to refresh.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll). The full AGENTS §10 suite is the dispatcher's between your landing and stage 3.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
6. [B] Backlog issues per [L](d); list the numbers.

Report: per-gate results, the corpus-unchanged evidence, any deviation from the plan and why.
```

**Prompt B.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B of the correctness-hygiene round (branch fix/domain-recur-done-window).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
[L] Round standing laws (binding): (a) typed outcomes only; (b) never sleeps in tests; (c) the property's assertion must be exactly as it was — a diff hunk touching the assertion body (not the fixed production code) is a finding; no quality ceilings; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §3 (normative), the dispatch plan §1/§3 (the code-quality mandate applies to you in full), and the stage-1 plan; then rebase onto main (git fetch origin --prune; rebase onto origin/main; drop already-merged duplicates; resolve only real conflicts; push --force-with-lease if history rewritten) and review the full branch diff versus the post-rebase base in ../taskboard-WT-fix-domain-recur-done-window.
2. Functional focus — this chunk's traps: (a) the fix must be on the partition-premise side the stage-1 diagnosis named — a fix that instead special-cases the Wed/Thu-Until-Thursday shape is a finding (shape-driven special cases break the next pattern family); (b) the persisted seed must be in crates/taskboard-domain/proptest-regressions/recur.txt and its case must pass; (c) the corpus-unchanged evidence must be present and re-runnable; (d) no other recur behavior may change — audit the diff for drive-by edits; (e) no test weakened (compare test hunks against origin/main).
3. Code-quality mandate (verbatim from §1 of this dispatch plan): evaluate naming, comment discipline (constraints, not narration), idiomatic approved stack, no dead code or speculative generality, test quality per AGENTS §5; the crate boundaries of AGENTS §2 are the SRP line; one level of abstraction per function body. Findings are fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (body: closes nothing automatically — reference #713 for the dispatcher's at-merge closure), report the URL with a per-acceptance-item verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](d); list the numbers.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, per-acceptance-item verdict.
```

## 4. Chunk C — flaky-watchlist dedicated pass (`test/flaky-watchlist-pass`)

- **Covers**: round plan §4 normative — the 8-issue disposition
  matrix; the tty-family seam diagnosis first; determinism-only
  stabilization; honest not-fixed-yet dispositions stay open.
- **Acceptance**: per-issue disposition recorded with reproduction
  evidence; the tty family either fixed at the seam (three closures
  on one evidence base) or each carries a dated diagnosis comment;
  #682's fix is a deterministic tie-break with the seed committed; no
  sleep-based stabilization anywhere in the diff; no test made more
  permissive; every closed issue carries reproduction command +
  run-count evidence.

**Prompt C.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk C of the correctness-hygiene round ("flaky-watchlist dedicated pass", branch test/flaky-watchlist-pass) in the taskboard repo at /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Reproduction loops (running a flaky test N times) run backgrounded with useful work between polls.
[L] Round standing laws (binding): (a) typed outcomes only; (b) never sleeps in tests — a stabilization must make the test MORE deterministic, never more permissive; floors not ceilings; (c) a flaky test can be a correct test catching a production race — evaluate both sides; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/flaky-watchlist-pass ../taskboard-WT-test-flaky-watchlist-pass off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 in full — this chunk lives on its laws), the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§4 (normative disposition matrix), the dispatch plan §1/§4, and all eight issues: #697 #521 #846 #287 #336 #384 #570 #650 #682 (nine numbers, eight distinct tests — #697/#521/#846 are three tests on one seam; note #654's folded observation on #697).
2. Read the test seams: crates/taskboard-app/tests/cli_tty.rs (the three Ctrl-C tests' raw-mode read paths and their timeouts), the wiremock harness files for #287/#336, the cli_login/cli_ipc test files for #384/#570, taskboard-domain merge duplicate_reconcile + its proptest for #682, taskboard-domain appdata tests for #650.
3. Diagnose the tty seam FIRST (it owns three issues): read the three tests' shared machinery, form the root-cause hypothesis, and record which side is broken (test determinism vs a production race the tests correctly catch).
4. For each of the other five: record the reproduction method (command/seed), the hypothesis, and the round-plan disposition (fix now | fix now via shared harness fix | document+hypothesis, stays open). #650 is time-boxed per the round plan: no local repro within the box ⇒ documented hypothesis only.
5. Write the chunk plan at .artifacts/plans/2026-11-16-flaky-watchlist-pass-plan.md: the per-issue disposition matrix (issue → root cause → side to fix → exact test/production change → closure evidence), the tty seam design (deterministic deadline/injected clock vs production fix), and the explicit no-weakening list (assertions that must survive byte-identical). Resolve every open question with a recorded decision.
6. Commit as: docs(plan): the flaky-watchlist pass chunk plan. Do not implement; do not push.
7. [B] Backlog issues per [L](d); list the numbers.

Report: plan path, the tty seam diagnosis, per-issue dispositions, risks, estimated diff size.
```

**Prompt C.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk C of the correctness-hygiene round (branch test/flaky-watchlist-pass).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait). Reproduction/stability loops (N repeated runs of a target test) run backgrounded with useful work between polls.
[L] Round standing laws (binding): (a) typed outcomes only; (b) stabilization makes tests more deterministic, never more permissive — no sleeps, no loosened timeouts-as-fix, no assertions deleted; floors not ceilings; (c) evaluate both sides when a test fails; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §4 (normative), the dispatch plan §1/§4, and the stage-1 plan .artifacts/plans/2026-11-16-flaky-watchlist-pass-plan.md in ../taskboard-WT-test-flaky-watchlist-pass (recreate on the branch if missing; landed branch ⇒ test/flaky-watchlist-pass-v2).
2. Implement exactly per the disposition matrix, tty seam first. Every commit standalone; per-issue or per-family commits (rebase-merge hygiene) so closures can cite commits. Confine the diff to the diagnosed seams (tests + the minimal production seam a diagnosis justifies); any broader refactor is a backlog issue, not scope.
3. Verify stability per the stage-1 plan's recorded method (repeated-run counts, backgrounded) and record the evidence per issue — reproduction command, run count, outcome.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. [B] Backlog issues per [L](d); list the numbers.

Report: per-issue outcome (fixed/quarantine-documented/unreproduced), stability evidence, per-gate results, deviations.
```

**Prompt C.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk C of the correctness-hygiene round (branch test/flaky-watchlist-pass).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge issue closures.
[L] Round standing laws (binding): (a) typed outcomes only; (b) any sleep introduced or any assertion weakened anywhere in the diff is a finding — compare every test hunk against origin/main; (c) no quality ceilings; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §4 (normative), the dispatch plan §1/§4 (the code-quality mandate applies to you in full), and the stage-1 plan; then rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; resolve only real conflicts; push --force-with-lease if rewritten) and review the branch diff versus the post-rebase base in ../taskboard-WT-test-flaky-watchlist-pass.
2. Functional focus — this chunk's traps: (a) the determinism law: every change must remove a real-time dependence or fix a diagnosed race — grep the diff for tokio::time::sleep, Thread::sleep, and relaxed timeout constants; (b) the no-weakening law: assertions that the stage-1 plan listed as must-survive are byte-identical; (c) the tty seam fix must cover all three tests or the plan's recorded reason for a narrower fix exists; (d) #682's tie-break fix is determinism (stable secondary key), not seed exclusion or tie-tolerance; (e) closure evidence per issue exists in the stage-2 report and is re-runnable.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself; any failure is a finding.
5. Fix every issue on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR to main with gh (body: references the eight issues; closures happen at merge by the dispatcher per the §10 lists), report the URL + per-issue verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict (fixed/documented/unreproduced).
```

## 5. Chunk D — config/test-harness hygiene (`test/config-harness-hygiene`)

- **Covers**: round plan §5 normative — #728 RAII cwd guard, #729
  env-lock helper, #730 portable sandbox, #607 PID-based script
  cleanup, #833 read-only audit rider.
- **Acceptance**: config suite green under hostile ambient
  `TASKBOARD_*` values; panic-safety of the cwd guard demonstrated
  (deliberate-panic test or recorded structural argument); #730's
  Linux path exercises the identical code path as today, platform
  gaps `cfg`-gated with named-gap comments; scripts' cleanup
  self-tested; #833 produces an audit comment (findings ⇒ backlog
  issues, no helper rewrites); diff confined to test helpers +
  the named scripts.

**Prompt D.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk D of the correctness-hygiene round ("config/test-harness hygiene", branch test/config-harness-hygiene) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only; (b) never sleeps in tests; (c) evaluate both sides when a test fails; never weaken a correct test; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b test/config-harness-hygiene ../taskboard-WT-test-config-harness-hygiene off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§5 (normative), the dispatch plan §1/§5, and issues #728 #729 #730 #607 #833.
2. Read the seams: the config tests' cwd/env handling (crates/taskboard-app tests — locate the manual save/restore sites), the sandbox helper's XDG assumption, scripts/*load* repro scripts' cleanup paths, and the lib/journey test helpers (#833 audit scope — read-only).
3. Decide the guard/helper shapes at chunk-plan level: the RAII cwd guard (Drop-restore, panic-safe under unwind) and its placement in test helpers; the TASKBOARD_* env-lock helper (save/clear/restore on drop, correct under merged harness — do not rely on nextest process isolation); the #730 per-platform sandbox (which env/registry seam per cfg!, which cases get cfg-gated with named-gap comments — the Linux path must keep the identical code path); the #607 PID-recording + trap cleanup shape.
4. Write the chunk plan at .artifacts/plans/2026-11-16-config-harness-hygiene-plan.md: per-issue change list with exact files, the hostile-env proof test, the panic-safety demonstration, the #730 platform matrix, the #607 self-test method, and the #833 audit checklist (read-only; findings ⇒ comments + backlog issues). Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the config harness hygiene chunk plan. Do not implement; do not push.
6. [B] Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt D.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk D of the correctness-hygiene round (branch test/config-harness-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only; (b) never sleeps; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §5 (normative), the dispatch plan §1/§5, and the stage-1 plan .artifacts/plans/2026-11-16-config-harness-hygiene-plan.md in ../taskboard-WT-test-config-harness-hygiene (recreate on the branch if missing; landed branch ⇒ test/config-harness-hygiene-v2).
2. Implement exactly per the plan: #728 guard, #729 env lock, #730 platform matrix, #607 script cleanup; run the #833 audit read-only and draft the findings comment (post it only at stage 3, or per the plan's recorded choice). Per-issue commits; diff confined to test helpers + named scripts; SPDX header on any new file; every commit standalone.
3. Prove per the plan: hostile-env run (TASKBOARD_* set to hostile values in the environment), panic-safety demonstration, #607 self-test.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. [B] Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, proof evidence, per-gate results, deviations.
```

**Prompt D.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk D of the correctness-hygiene round (branch test/config-harness-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round standing laws (binding): (a) typed outcomes only; (b) never sleeps; (c) no weakened tests — diff every test hunk against origin/main; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §5 (normative), the dispatch plan §1/§5 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-config-harness-hygiene.
2. Functional focus — this chunk's traps: (a) the cwd guard must restore on the panic path, not only the happy path — verify the Drop/unwind reasoning or the demonstration test; (b) the env lock must restore every TASKBOARD_* variable it clears, including ones it did not know about beforehand (save-then-clear-all, not clear-a-fixed-list); (c) #730's Linux path must be unchanged in behavior — audit the cfg! arms for accidental Linux drift; (d) #607's cleanup must kill the recorded PIDs on both success and failure paths (trap on EXIT, not on explicit success only); (e) #833 stayed read-only — a helper rewrite in the diff is a finding.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #728 #729 #730 #607 #833 — closures are the dispatcher's at merge), report the URL + per-acceptance-item verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 6. Chunk E — sync ETag retention + skew (`fix/sync-etag-skew`)

- **Covers**: round plan §6 normative — characterization wiremock
  oracle first; validator design (a)/(b)/(c) with recorded choice; the
  named skew-tolerance constant; bounded 412 convergence; unchanged
  non-pathological corpus.
- **Acceptance**: the retained-ETag scenario passes (no false-clean,
  no unbounded 412 loop, typed give-up outcome); skew tests on
  pause/advance or injected clock; existing pull/push corpus
  snapshots unchanged on the non-pathological path; the tolerance is
  a named constant (no call-site magic numbers); PR confined to
  `taskboard-sync-nextcloud` (+ transport types it owns).

**Prompt E.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk E of the correctness-hygiene round ("sync ETag retention + clock skew", branch fix/sync-etag-skew) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only; (b) all timing tests on pause/advance or an injected clock — never sleeps, never the wall clock; (c) never weaken a correct test; the DR-8 characterization corpus is correct until proven otherwise; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b fix/sync-etag-skew ../taskboard-WT-fix-sync-etag-skew off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§6 (normative — the no-false-clean design options and their constraints), the dispatch plan §1/§6, issue #695 (the curl evidence matrix) and #133, plus the sync crate's architecture docs (docs/architecture.org sync sections).
2. Read the seams: the sync actor's pull/compare path (validator maintenance, the ETag-equality conclusions), the push path (conditional writes, 412 handling — is there a retry bound today?), the DR-8 characterization tests (F4/F5) and their wiremock harness, and every remote-vs-local timestamp comparison site (#133's audit list — the `<=` equal-second cases).
3. Design to a recorded decision: pick validator design (a) refresh / (b) revision-field precedence / (c) If-Match discipline per the round plan's constraint set (no false-clean, no 412 loops, no same-size+same-second ⇒ same-content heuristics); name the skew-tolerance constant, its value, and its justification against #695's hosted evidence; specify the bounded-retry + typed give-up outcome for the stale-token path; specify the characterization wiremock (time-controlled, models retained-ETag on same-second+same-size).
4. Write the chunk plan at .artifacts/plans/2026-11-16-sync-etag-skew-plan.md: the design decision with alternatives rejected, the wiremock oracle definition, the per-comparison-site audit table (#133), the bounded-convergence spec, and the corpus-unchanged guarantee method. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the sync etag-skew chunk plan. Do not implement; do not push.
6. [B] Backlog issues per [L](d); list the numbers.

Report: plan path, the design decision, the #133 audit table summary, risks, estimated diff size.
```

**Prompt E.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk E of the correctness-hygiene round (branch fix/sync-etag-skew).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock in every timing test — never sleeps; (c) never weaken a correct test — the DR-8 corpus stays; snapshots change only per the corpus-unchanged guarantee's diagnosis path; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §6 (normative), the dispatch plan §1/§6, and the stage-1 plan .artifacts/plans/2026-11-16-sync-etag-skew-plan.md in ../taskboard-WT-fix-sync-etag-skew (recreate on the branch if missing; landed branch ⇒ fix/sync-etag-skew-v2).
2. Implement exactly per the plan, characterization-oracle-first: the retained-ETag wiremock scenario red before the client change, green after. TDD; every commit standalone; diff confined to taskboard-sync-nextcloud (+ the transport types it owns); SPDX + forbid(unsafe) per crate rules on new files.
3. Apply the #133 audit table: every listed comparison site gets the named tolerance constant; no call-site magic numbers.
4. Prove the corpus-unchanged guarantee per the plan's method on the non-pathological path.
5. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll). The full §10 suite is the dispatcher's.
6. No push, no PR. Never push to or merge main.
7. [B] Backlog issues per [L](d); list the numbers.

Report: per-gate results, the oracle red→green evidence, the audit-table completion state, deviations.
```

**Prompt E.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk E of the correctness-hygiene round (branch fix/sync-etag-skew).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round standing laws (binding): (a) typed outcomes only; (b) no sleeps, no wall-clock reads in tests — grep the diff; (c) no weakened tests; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §6 (normative), the dispatch plan §1/§6 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-sync-etag-skew.
2. Functional focus — this chunk's traps: (a) the false-clean invariant: trace the sameness conclusions — no path may conclude "remote current" from ETag equality alone where the same-second rewrite could have occurred; (b) the 412 path converges with a bound and a typed give-up — an unbounded loop or a silent drop is a finding; (c) the tolerance constant is named, single-sourced, and justified against #695's hosted evidence in the plan — stray literal second-granularity comparisons outside the constant are findings; (d) the characterization wiremock controls time explicitly (no real-clock dependence); (e) the non-pathological corpus is snapshot-identical or each diff has the plan's diagnosis recorded.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code or speculative generality, test quality per AGENTS §5; crate boundaries are the SRP line (sync logic stays out of domain/state; no UI-adjacent types here); one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #695 #133 — closures are the dispatcher's at merge), report the URL + per-acceptance-item verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 7. Chunk F — Retry-After family (`fix/sync-retry-after`)

- **Covers**: round plan §7 normative — both header forms, floor/cap
  bounding, 503 coverage, abort-path floor honoring, typed
  deferred-outcome distinction.
- **Acceptance**: delta-seconds and HTTP-date scenarios each produce
  the correct injected-clock wait; absent/unparseable header falls
  back to current backoff (typed, no error); floor ≤ honored wait ≤
  cap asserted structurally; the abort path's rescheduling honors
  the floor; existing retry behavior without a header unchanged;
  depends on E having landed (stage 1 re-anchors on E's transport
  changes).

**Prompt F.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk F of the correctness-hygiene round ("Retry-After family", branch fix/sync-retry-after) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed outcomes only — distinguish "deferred by policy" from "deferred by server hint" as typed variants, never by message text; (b) all timing tests on pause/advance or an injected clock — never sleeps; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b fix/sync-retry-after ../taskboard-WT-fix-sync-retry-after off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§7 (normative), the dispatch plan §1/§7, issues #220 #221 #222. Chunk E (fix/sync-etag-skew) must have landed: verify its PR is merged (gh pr list --state merged or the issue's state) — if not, STOP and report instead of planning.
2. Read the seams, re-anchored on E's landed changes: the retry policy (backoff computation, floor/cap constants), the response-header handling (today's Retry-After support, if any), the push-abort path's rescheduling (#222's storm window), and the wiremock harness.
3. Decide at chunk-plan level: where the header parse lives (delta-seconds + HTTP-date via chrono; unparseable ⇒ absent, never an error), the exact floor/cap interaction formula (max(policy_floor, min(retry_after, policy_cap)) per the round plan — record the cap's value with rationale), the 503 coverage change, the abort-path floor mechanics (#222), and the typed deferred-outcome shape.
4. Write the chunk plan at .artifacts/plans/2026-11-16-sync-retry-after-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the sync retry-after chunk plan. Do not implement; do not push.
6. [B] Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt F.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk F of the correctness-hygiene round (branch fix/sync-retry-after).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait).
[L] Round standing laws (binding): (a) typed deferred outcomes; (b) pause/advance or injected clock — never sleeps; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §7 (normative), the dispatch plan §1/§7, and the stage-1 plan .artifacts/plans/2026-11-16-sync-retry-after-plan.md in ../taskboard-WT-fix-sync-retry-after (recreate on the branch if missing; landed branch ⇒ fix/sync-retry-after-v2).
2. Implement exactly per the plan: both header forms, floor/cap bounding, 503 coverage, abort-path floor. TDD (each wiremock scenario red→green); every commit standalone; diff confined to taskboard-sync-nextcloud's retry/transport surface per the plan.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. [B] Backlog issues per [L](d); list the numbers.

Report: per-gate results, scenario red→green evidence, deviations.
```

**Prompt F.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk F of the correctness-hygiene round (branch fix/sync-retry-after).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round standing laws (binding): (a) typed deferred outcomes; (b) no sleeps/wall-clock in the diff; (c) no weakened tests; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §7 (normative), the dispatch plan §1/§7 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-sync-retry-after.
2. Functional focus — this chunk's traps: (a) an unparseable HTTP-date must fall back to the policy backoff as a typed absent case — an error outcome or a zero-wait is a finding; (b) the cap must genuinely bound server hints — construct the adversarial Retry-After case in review (huge delta, date far future) and check the computed wait; (c) the abort path (#222) honors the floor — trace the reschedule from an aborted push end-to-end; (d) no-header behavior is byte-identical to origin/main's retry corpus.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #220 #221 #222 — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 8. Chunk G — sidecar-convergence design round (`docs/sidecar-convergence-design`)

- **Covers**: round plan §8 normative — evidence report + design plan
  for the F1 sidecar local-author leg (#708), one recommendation,
  **zero production code**, ends at the operator's human gate.
- **Deliverable**: PR containing exactly the evidence report
  (`.artifacts/reports/<today>-sidecar-convergence-design-report.md`)
  and the design plan
  (`.artifacts/plans/<today>-sidecar-convergence-design-plan.md`);
  nothing else.
- **Acceptance**: every behavioral claim carries file:line or a repro
  command; the convergence-law formalization is stated; exactly one
  recommendation with recorded alternatives; the migration/compat
  story for existing on-disk sidecars is present; the report names
  the human-gate handoff.

**Prompt G.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk G of the correctness-hygiene round ("sidecar-convergence design round", branch docs/sidecar-convergence-design) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): your gate is the pre-commit hooks (automatic on commits). Any repo-wide search or long build runs backgrounded with useful work between polls; commit landed work BEFORE any wait.
[L] Round standing laws (binding): (a) every behavioral claim carries file:line or a repro command — record reality, never invent; (b) decision-grade output: options + trade-offs + exactly one recommendation; (c) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b docs/sidecar-convergence-design ../taskboard-WT-docs-sidecar-convergence-design off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the round plan .artifacts/plans/2026-11-16-correctness-hygiene-round-plan.md §1/§8 (normative), the dispatch plan §1/§8, issue #708, and the convergence-law context it cites (the F1 sidecar fold design docs — locate via the issue body and docs/architecture.org).
2. Write the research chunk plan at .artifacts/plans/2026-11-16-sidecar-convergence-design-chunk-plan.md: the characterization method (which write paths, which orderings, what a third reader observes — with the file:line inventory), the formalization approach for the convergence law over sidecar folds, the candidate design axes, and the report/design-plan skeletons. Zero production code is a hard boundary — the plan itself states it.
3. Commit as: docs(plan): the sidecar-convergence design chunk plan. Do not implement; do not push.
4. [B] Backlog issues per [L](c); list the numbers.

Report: plan path, the inventory summary, risks.
```

**Prompt G.2 (research):**

```text
You are the stage-2 RESEARCH agent for chunk G of the correctness-hygiene round (branch docs/sidecar-convergence-design).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): long searches/builds run backgrounded with useful work between polls; commit landed work BEFORE any wait. (No test gate applies — docs-only chunk; the pre-commit hooks run on every commit regardless.)
[L] Round standing laws (binding): (a) evidence-carrying claims only — file:line or repro commands, recorded reality; (b) options + trade-offs + exactly one recommendation; (c) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §8 (normative), the dispatch plan §1/§8, and the stage-1 chunk plan .artifacts/plans/2026-11-16-sidecar-convergence-design-chunk-plan.md in ../taskboard-WT-docs-sidecar-convergence-design (recreate on the branch if missing; landed branch ⇒ docs/sidecar-convergence-design-v2).
2. Execute the characterization per the plan: trace the sidecar write paths, build the third-reader observation model, formalize the convergence law over sidecar folds, develop the candidate designs, and select one with recorded alternatives and the migration/compat story for existing on-disk sidecars.
3. Write the evidence report (.artifacts/reports/<today>-sidecar-convergence-design-report.md) and the design plan (.artifacts/plans/<today>-sidecar-convergence-design-plan.md) per the plan's skeletons. Every claim carries its evidence inline.
4. Gate: pre-commit hooks (docs-only chunk — no cargo gate expected to move; the dispatcher still runs the full §10 suite before shipping).
5. No push, no PR. Never push to or merge main.
6. [B] Backlog issues per [L](c); list the numbers.

Report: the recommendation summary, evidence coverage, deviations.
```

**Prompt G.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk G of the correctness-hygiene round (branch docs/sidecar-convergence-design).

[D] Long-command discipline (binding; docs/agent_dispatch.org, PR #852): commit landed work BEFORE any wait; your scope ends at PR-open — push, open the PR with gh, report the URL; the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round standing laws (binding): (a) spot-check every load-bearing claim's evidence (re-run the repro commands or open the cited lines); (b) exactly one recommendation, decision-grade; (c) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the round plan §8 (normative), the dispatch plan §1/§8 (the evidence & artefact mandate applies to you in full — copy: every factual claim carries evidence (the command run + observed outcome, or a verifiable source + what it states); recommendations are decision-grade (options, trade-offs, exactly one recommendation, consequences for later work); repo hygiene (scratch code and captured payloads never land — evidence is excerpts inside the report; the PR diff is exactly the chunk's artefacts); scope discipline (out-of-scope ideas go to the backlog, not into the report)), and the stage-1 chunk plan; rebase onto main (fetch --prune; rebase; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-docs-sidecar-convergence-design.
2. Functional focus — this chunk's traps: (a) the PR diff is exactly two artefact files — any other file is a finding; (b) zero production code — a stray code change is a finding; (c) inventiveness check: claims about server or actor behavior that lack cited evidence are findings (recorded reality, never invented); (d) the human-gate handoff is explicit in the design plan (what the operator accepts, what dispatch may chunk afterward).
3. Evidence & artefact mandate (verbatim, §1 of this dispatch plan).
4. Pre-commit hooks are your gate; the dispatcher runs the full §10 suite before shipping.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #708 — it stays open through the human gate), report the URL + verdict. Do NOT watch CI.
7. [B] Backlog issues per [L](c); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 9. Dispatcher runbook (binding, per `docs/agent_dispatch.org`)

Between every pair of stages, in the chunk's worktree: verify landed
state (`git log origin/main..HEAD`, clean tree, gate evidence
re-runnable) — **resume over redo** on stall/quota (state-check
message; failure playbook §8 of the law). Between stage-2 landing and
stage-3 ship of every code chunk (B–F): run the full AGENTS §10
sequence through `scripts/run-quality-gates.sh` in the dispatcher's
own session (background + poll). After each stage-3 PR-open: `gh pr
checks --watch` to green, fix-forward loop with the stage-3 agent
only if a check fails and the fix needs the branch (else
dispatcher-fix directly), `mergeStateStatus` must report `CLEAN`,
then teardown from the main checkout with the absolute-path form and
`git worktree list` confirmation. Chunk A runs compressed (§2): the
dispatcher is planner and implementer, stating that choice in its
verdict plan.

## 10. Completion bookkeeping & at-merge closures (dispatcher-owned)

- **Chunk A**: #731 closed-with-evidence (Path A) or fixed-then-closed
  at merge (Path B/C); Path B/C's ignore arm also comments register
  #284.
- **Chunk B**: closes #713 at merge (PR body references it; the
  dispatcher closes with the run evidence).
- **Chunk C**: closes the flaky issues whose disposition matrix marked
  them fixed (expected: #697/#521/#846 together; #682; any of
  #336/#287/#384/#570 fixed this pass), each with reproduction
  evidence; #650 (and any not-fixed-yet issue) stays open with the
  dated watchlist comment.
- **Chunk D**: closes #728/#729/#730/#607; posts the #833 audit
  comment (closes #833 — the audit is the deliverable; new findings
  become `backlog:` issues per the standing authorization).
- **Chunk E**: closes #695/#133 (plus the manual live-tier
  verification note on #695).
- **Chunk F**: closes #220/#221/#222.
- **Chunk G**: #708 stays open — the dispatcher comments the design
  plan link and hands the human gate to the operator.
- **Deliberately not dispatched here** (round plan §10): #327,
  #356/#355, #845, #764, #843 (later backlog round, operator ruling);
  the perf-architecture seven; feature-scale items; all round-8
  territory and C-6 deferrals.
- **What opens next**: the second-tier backlog round (operator
  scheduling decision), the #708 implementation dispatch after the
  human gate, and chunk F's live-tier follow-ups if the operator
  schedules #117.
