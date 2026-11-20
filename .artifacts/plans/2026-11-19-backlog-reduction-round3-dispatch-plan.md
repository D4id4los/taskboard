# Backlog-Reduction Round 3 ("runtime + sweep") — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-19
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation/research → review) for the 13 chunks of the accepted
  round-3 scope (`.artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md`,
  operator-accepted 2026-11-19): the runtime seed R1–R4 (#1069/#1071/
  #1072/#1070) first, then the sweep B1–B8. No code in this commit.
- **Origin**: the scope report (§A seed, §B join set, §C deferrals —
  normative); issue bodies normative for their chunks; the recorded
  round-level decisions below bind their stages. The artefact sweep is
  already committed (`chore/artefact-sweep-2026-11-19` @ `73a9e09e` —
  push/PR pending the operator's word; not a chunk here).
- **Status**: READY. No r9/round running — the standing resource law
  applies (≤3 concurrent agents, memory gate for expected-heavy
  builds, flock-deferral rule, exclusive lanes for benchmark-class
  runs). Journey gates: B7 only. Operator gates: B2's boot-policy
  decision (data-loss-adjacent), B7's journey acceptance, R4's binary
  map if it moves a named surface.
- **Orchestration law**: `docs/agent_dispatch.org` (PR #852 + #894's
  flock-deferral rule + #1063's clarification as it lands). Stage
  prompts restate the discipline verbatim.

## 1. Chunks & ordering

| Chunk | Issues | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| R1 | #1069 | `ci/dev-lld-linker` | lld for dev/test profiles + measured delta | no UX plan in origin | S |
| R2 | #1071, #968 | `test/shutdown-budget` | fixture override + actor-teardown investigation (sanctioned #968 pass) | no UX plan in origin | M |
| R3 | #1072 | `test/push-phase-generator` | direct-construction generator for the push-phase property | no UX plan in origin | S |
| R4 | #1070 | `refactor/consolidate-test-binaries` | group the 98 test binaries; nextest filtering preserved | no UX plan in origin | L |
| B1 | #986 | `fix/domain-loghabit-tie` | equal-clock LogHabit tie rule + property strengthening | no UX plan in origin | S/M |
| B2 | #1039, #1038 | `fix/journal-decode-robustness` | boot-load decode policy + typed context (**operator gate**) | no UX plan in origin | M |
| Bui | #1004/#1026/#1033/#1036/#1053 | `fix/ui-r9-residue-bugs` | five root-caused UI residue bugs | no UX plan in origin (bug fixes) | M |
| B3 | #984/#1013/#982/#981 | `test/app-test-hygiene` | env lock, device-home injection, tempdir lifetime, double dedup | no UX plan in origin | M |
| B4a | #1062, #1016 | `refactor/journey-harness-helper` | shared journey-harness module; headless-mix binary split | no UX plan in origin | M |
| B4b | #1023/#1024/#995/#996/#997/#1059/#1018 | `test/journey-backend-pins` | seven testing-backend/journey pins after B4a's helper | no UX plan in origin | M |
| B5 | #965/#967/#970/#964/#1063/#1043/#1042/#1046/#1047 | `ci/gate-law-doc-residue` | gate correctness residue + law/doc codification | no UX plan in origin | M |
| B6 | #993/#994/#991/#990 | `fix/ipc-restore-followups` | serde pin, plan divergence, drain status, idempotent restore | no UX plan in origin | M |
| B7 | #1014, #1015 | `feat/config-batch-unset-reload` | batch unset + manual reload verb | **journey plan B7-JP — mandatory, gated** | M |
| B7b | #1051 | `feat/files-sync-poll-write-lane` | poll_secs live-write lane (apply-registry entry) | no UX plan in origin (rides the accepted C10 contract — stage-1 verifies) | S/M |
| B8 | #978/#1034/#1060/#1065/#1027 | `refactor/ui-small-grammar-bundle` | five small ui refactors with existing cover | no UX plan in origin | S/M |

**Dependency graph & order** (≤3 concurrent; R4 exclusive — its builds
saturate the box):

1. **Wave 0**: **R1** ∥ **B5** (both light; R1's measurement
   unblocks everything).
2. **Wave 1**: **R2** ∥ **R3**.
3. **Wave 2**: **R4 exclusive** ∥ (at most one light chunk: **B7b** or
   **B1**).
4. **Wave 3**: **B1** ∥ **B2** ∥ **Bui**.
5. **Wave 4**: **B4a → B4b** (B4b consumes B4a's helper) ∥ **B3** ∥
   **B6**.
6. **Wave 5**: **B8** ∥ **B7.1** (journey stage-1; gate) → **B7.2–3**.

**Dispatch mechanics**: one agent session per prompt, in order
(N.x.1 → N.x.2 → N.x.3); never two agents share a worktree; worktrees
at `../taskboard-WT-<flat-suffix>`, branched off the then-current
`main` with `git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`
(never without `-b`, never on `main`); landed branch ⇒ resume on
`<branch>-v2`; `scripts/worktree-doctor.sh` first, every agent,
nonzero (disk or MemAvailable floor) = STOP.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding — `docs/agent_dispatch.org`;
PR #852 + the #879/#894 flock-deferral rule)*: a subagent is killed
after ~600s without observable activity, so long silent commands kill
working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). **Flock-deferral rule: when the gate lock is
   contended and no other useful work remains, never block — commit
   all landed work, report "gate deferred: lock held by <holder>",
   and end the stage; the dispatcher runs the gate.** Expected-heavy
   single builds (~>1 min) route through `scripts/memory-gate.sh`.
   The dispatcher runs the full AGENTS §10 suite in its own session
   between stage 2's landing and stage 3's ship; CI's Code Coverage
   check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures (§12).
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
   `.artifacts/plans/<today>-<topic>-plan.md` (B7 additionally the UX
   journey plan), every open question resolved with a recorded
   decision, committed (`docs(plan): …`). No implementation, no push.
2. **Stage 2 — implementation/research agent**: continues in the
   worktree (recreate on the branch if missing; landed branch ⇒
   `<branch>-v2`), executes per the approved plan — TDD where code is
   involved, every commit standalone, gate = pre-commit hooks +
   flock-wrapped nextest (defer per the rule). No push, no PR.
3. **Stage 3 — review agent**: rebases onto `main` (fetch --prune;
   rebase onto `origin/main`; drop already-merged duplicates; resolve
   only real conflicts; push `--force-with-lease` if rewritten),
   reviews the branch diff versus the post-rebase base against the
   chunk's acceptance criteria + AGENTS rules + the code-quality
   mandate, re-runs the flock-wrapped nextest gate (defer per the
   rule), fixes on the branch (amend/reword). Last tasks: push, open
   the PR to `main` with `gh`, report the URL + per-acceptance
   verdicts. **No CI watch.**

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

## 2. Chunk R1 — lld for dev/test (#1069, `ci/dev-lld-linker`)

- **Covers** (normative): the report §A's lever 1 — a
  `.cargo/config.toml` stanza switching the dev/test profiles to lld
  (`-Clinker-features=-ld,+lld` on rustc 1.99; lld at `/usr/bin/llc`…
  verify at `/usr/bin/ld.lld`), plus the measured before/after delta.
- **CI-parity trap (binding decision point)**: the config applies to
  every build on every machine — stage-1 must verify the CI runners
  have lld available (inspect the workflow's toolchain setup; if
  absent, add the setup step to the workflow or scope the config so
  CI is unaffected — the choice recorded with evidence; a config that
  breaks CI is a shipping blocker caught at `gh pr checks`).
- **Acceptance**: the before/after measurement recorded (the report's
  method: incremental `--all-targets --timings`; the report's baseline
  4m25s wall / 2104s CPU, 70% in test-binary units — the delta excerpt
  lives in the chunk plan/report); the app bin's edit loop (~3s)
  verified unaffected; `cargo nextest run --workspace` green via the
  gate; the config commented with the constraint it encodes; diff =
  `.cargo/config.toml` (+ CI setup step if the recorded path requires)
  + the measurement note in the chunk plan.

**Prompt R1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk R1 of round 3 ("lld linker", branch ci/dev-lld-linker) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). Timing/measurement builds route through scripts/memory-gate.sh and run backgrounded with useful work between polls. When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) measured claims carry the command + environment; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b ci/dev-lld-linker ../taskboard-WT-ci-dev-lld-linker off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md (§10 — the coverage/gate context), the scope report .artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md §A (normative), the dispatch plan .artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md §1/§2, issue #1069, and the runtime report .artifacts/reports/2026-11-19-build-test-runtime-analysis-report.md §A (the baseline numbers and the lever ranking).
2. Verify the facts the config depends on: lld's presence and form locally (ld.lld vs lld), rustc 1.99's stable linker-features flag spelling, and the CI runners' toolchain (read .github/workflows/* — is lld present on the runner image? does the workflow install components?). Decide the CI path with evidence: extend the workflow setup, or scope the config so CI is unaffected — a config that breaks CI is a shipping blocker.
3. Capture the BEFORE measurement per the report's method (incremental --all-targets --timings on the warm tree; record wall/CPU and the test-binary share) — backgrounded through the memory gate, committed before any wait.
4. Write the chunk plan at .artifacts/plans/2026-11-19-dev-lld-linker-plan.md: the exact config stanza with its constraint comment, the CI decision, the AFTER measurement method, and the acceptance numbers. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the lld linker chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the CI decision, the before numbers, risks, estimated diff size.
```

**Prompt R1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk R1 of round 3 (branch ci/dev-lld-linker).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended). The AFTER measurement build routes through scripts/memory-gate.sh, backgrounded with polls.
[L] Round laws (binding): (a) the after numbers carry the identical command as the before; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§2, and the stage-1 plan .artifacts/plans/2026-11-19-dev-lld-linker-plan.md in ../taskboard-WT-ci-dev-lld-linker (recreate on the branch if missing; landed branch ⇒ ci/dev-lld-linker-v2).
2. Implement exactly per the plan: the config stanza (+ the CI setup step if the recorded decision requires), run the AFTER measurement with the identical command, verify the app-bin edit loop (~touch src/main.rs rebuild ≈3s) is unaffected, and record both deltas in the chunk report. Diff confined to .cargo/config.toml (+ the workflow step) per the plan.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: the before/after numbers, the app-bin check, per-gate results, deviations.
```

**Prompt R1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk R1 of round 3 (branch ci/dev-lld-linker).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) the numbers are reproducible (re-run one measurement shard if a claim looks off); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§2 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-ci-dev-lld-linker.
2. Functional focus — this chunk's traps: (a) the CI decision is actually safe — trace the runners' toolchain path end-to-end (a config that links with lld on dev but falls back silently, or fails, on CI is the exact shipping blocker; the recorded decision must hold on both); (b) the config's comment states the constraint (why lld, which profiles) — a bare stanza is a finding; (c) the delta is honest (same command, warm tree, recorded environment — not a cherry-picked rerun); (d) no other profile (release) changed.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1069 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 3. Chunk R2 — shutdown budget + teardown investigation (#1071, `test/shutdown-budget`)

- **Covers** (normative), two halves:
  - **Fixture override**: the test helpers (`config_for`,
    `common::valid_config`) set `shutdown_timeout_secs = 1` (or the
    recorded smaller budget); `bootstrap.rs:91`'s
    `BOOT_FAILURE_JOIN_BUDGET` likewise. The wiremock 5s-delay tests
    asserting real timeout contracts stay justified (the report's
    verdict) — proportional assertions only where the test's oracle
    allows a shorter delay without weakening the contract; never a
    blanket delay reduction.
  - **The investigation**: WHY an actor needs the full 10s at teardown
    (the report: "a production `taskboard sync` that always burns its
    whole shutdown budget is a real inefficiency"). Deliverable: a
    diagnosis (which actor(s), what they wait on, whether the budget
    is a safety margin or a live stall) ending in a typed fix or an
    armed finding. **#968 rides here as the sanctioned dedicated
    pass**: the flaky's mechanism (a 10k-yield bounded poll losing the
    shutdown race under load) is the same seam; the issue's own
    deterministic-fix candidate (await the join handle directly, or
    poll with a deadline) is the test-side fix; if the production
    diagnosis explains the race, #968 closes with the evidence —
    never opportunistically, this chunk IS the dedicated diagnosis.
- **Acceptance**: the slow-test table re-measured (the 10s-quantized
  family drops to ~1s each; the suite wall drops accordingly —
  recorded); the fixture override is total (no verb test left paying
  the default); the investigation report
  (`.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`)
  with per-actor findings and exactly one recommendation each (fix
  now in-scope vs armed backlog issue); #968 dispositioned per the
  above; no test weakened; diffs confined to the fixtures + the
  diagnosed production seams + the a12 test's determinism fix.

**Prompt R2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk R2 of round 3 ("shutdown budget + teardown investigation", branch test/shutdown-budget) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). The slow-test re-measurement runs backgrounded through the memory gate with useful work between polls. When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — never log/text assertions in the determinism fix; (b) the a12 fix must be deterministic-by-construction (await the join handle or a deadline poll — never a bigger yield count); (c) never weaken a correct test — a wiremock delay test's contract stays pinned; shortening a delay requires the recorded proportional-assertion argument; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/shutdown-budget ../taskboard-WT-test-shutdown-budget off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 clock/determinism laws), the scope report .artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md §A (normative), the dispatch plan .artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md §1/§3, issues #1071 #968, and the runtime report §B (the quantization analysis, the 41-over-5s table, the two outliers).
2. Read the seams: DEFAULT_SHUTDOWN_TIMEOUT_SECS (crates/taskboard-app/src/config.rs:42) and its consumers; the test fixtures config_for/common::valid_config (which test files use them — the inventory); bootstrap.rs:91's BOOT_FAILURE_JOIN_BUDGET; the actor shutdown path (who waits at teardown — spawn/join sites in taskboard-app/src/); the a12 test's bounded poll (crates/taskboard-sync-nextcloud actor_wiremock.rs ~836).
3. Decide: the fixture budget value (1s vs the recorded alternative — justify against the slowest legitimate verb work), the inventory of tests needing the override (mechanical sweep), the investigation method (which actors to instrument, how to observe the wait — a diagnostic run with tracing, reproducible), and the #968 disposition rule (close-on-coverage vs armed).
4. Write the chunk plan at .artifacts/plans/2026-11-19-shutdown-budget-plan.md + the investigation skeleton. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the shutdown budget chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the budget decision, the investigation method, risks, estimated diff size.
```

**Prompt R2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk R2 of round 3 (branch test/shutdown-budget).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended). The re-measurement runs backgrounded through the memory gate.
[L] Round laws (binding): (a) typed outcomes; (b) the a12 fix is deterministic-by-construction; (c) no delay-test contract weakened — the proportional-assertion arguments are recorded per changed test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-19-shutdown-budget-plan.md in ../taskboard-WT-test-shutdown-budget (recreate on the branch if missing; landed branch ⇒ test/shutdown-budget-v2).
2. Implement exactly per the plan: the fixture override sweep (every inventoried test), BOOT_FAILURE_JOIN_BUDGET, the a12 determinism fix; then execute the investigation per the method (instrumented runs, backgrounded; commits before waits) and write the investigation report (.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md — per-actor findings, exactly one recommendation each). Production fixes only where the diagnosis is conclusive and the fix is small; everything else is an armed `backlog:` issue.
3. Re-measure the slow-test table (identical method to the runtime report) and record the delta.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: the re-measured table delta, the investigation's verdict per actor, the #968 disposition, per-gate results, deviations.
```

**Prompt R2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk R2 of round 3 (branch test/shutdown-budget).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the fixture override must not mask a real hang as a fast pass — a test that now fails only because the budget shrank below legitimate work is a finding (the budget value's justification is the defense); every changed delay-test keeps its contract assertion; the production diff hunk traces to a conclusive diagnosis line; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§3 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-shutdown-budget.
2. Functional focus — this chunk's traps: (a) the override inventory is complete — grep for config constructors in tests that bypass the fixtures (a verb test still on the 10s default is a finding); (b) the a12 fix awaits the structural fact (the join handle) rather than tuning the poll — a tuned loop is a finding; (c) the investigation's per-actor recommendations are decision-grade (evidence per actor, not vibes); (d) the production diff is minimal and each hunk cites the diagnosis line that justifies it.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1071 — closure is the dispatcher's at merge; #968 closes here too if the disposition rule's evidence bar is met, else its dated comment), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, the #968 disposition.
```

## 4. Chunk R3 — push-phase generator (#1072, `test/push-phase-generator`)

- **Covers** (normative): replace the five-clause `prop_filter`
  rejection sampling in
  `taskboard-domain::pipeline::tests::apply_push_report_equals_the_pipeline_push_phase`
  with a `persisted_state_strategy()` variant that constructs the
  accepted shape directly (bound board, all remote_seen set, non-empty
  outbox). The property's assertions stay byte-identical; the filter's
  *intent* moves into the generator. `PROPTEST_CASES` tuning may be
  recorded but the generator is the fix.
- **Acceptance**: the 75s test drops to a normal profile (recorded
  before/after); the property's case coverage does not shrink (the
  generator produces the accepted shape class; any shape the filter
  admitted that the generator misses is a coverage hole — stage-1
  enumerates the filter's admitted set and proves the generator covers
  it); the strategy is reusable where the same shape is needed;
  diffs confined to the domain test/strategy code.

**Prompt R3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk R3 of round 3 ("push-phase generator", branch test/push-phase-generator) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (c) the property's assertions are byte-identical after the change; the filter's admitted set is enumerated and the generator proven to cover it — a generator that shrinks coverage is a finding you must not create; (d) backlog issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/push-phase-generator ../taskboard-WT-test-push-phase-generator off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 proptest law), the scope report §A (normative), the dispatch plan §1/§4, issue #1072, and the runtime report §B (the outlier analysis).
2. Read the seam: crates/taskboard-domain/src/pipeline.rs tests (the property, the five filter clauses, persisted_state_strategy's shape) and the strategy's existing helpers.
3. Decide: the direct-construction strategy's shape (which clauses become construction invariants vs post-generation asserts), its name/reusability (other filtered properties may want it — record), the coverage-equivalence argument (enumerate the filter's admitted set; show the generator's output space covers it), and the case-count decision (PROPTEST_CASES if the direct construction warrants a different default — recorded).
4. Write the chunk plan at .artifacts/plans/2026-11-19-push-phase-generator-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the push-phase generator chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the coverage-equivalence argument, risks, estimated diff size.
```

**Prompt R3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk R3 of round 3 (branch test/push-phase-generator).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (c) the property's assertions byte-identical; the before/after timing recorded with the identical command; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§4, and the stage-1 plan .artifacts/plans/2026-11-19-push-phase-generator-plan.md in ../taskboard-WT-test-push-phase-generator (recreate on the branch if missing; landed branch ⇒ test/push-phase-generator-v2).
2. Implement exactly per the plan: the direct-construction strategy, the property rewired to it, the before/after timing. Diff confined to the domain test/strategy code; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: before/after timing, the coverage argument's execution, per-gate results, deviations.
```

**Prompt R3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk R3 of round 3 (branch test/push-phase-generator).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (c) the property hunk must show assertions unchanged and only the strategy rewired — any assertion edit is a finding; the generator cannot produce a state the old filter would have rejected (spot-construct the boundary shapes); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§4 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-push-phase-generator.
2. Functional focus — this chunk's traps: (a) the strategy's construction invariants hold under shrinking (a shrunken case must still satisfy the accepted shape or the failure reproduction breaks — check the strategy's shrink behavior); (b) the reusability claim in the plan is real (no speculative generic if only one caller — the SRP/no-speculative-generality line); (c) the timing delta is honest.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1072 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 5. Chunk R4 — consolidate test binaries (#1070, `refactor/consolidate-test-binaries`) — EXCLUSIVE lane

- **Covers** (normative): merge the 19 ui-slint `*_probe.slint` +
  `*_probe_tests.rs` pairs into grouped test targets and the ~34 app
  `cli_*` files into a small set of binaries, per a recorded binary
  map. The one-behavior-one-test convention is untouched (test *cases*
  keep their names; `cargo nextest` name filtering works — the report
  §A lever 2). Target: the 1471s test-binary CPU share toward
  ~300–400s.
- **Operator gate**: the binary map (which files group where) is
  reviewable — stage-1's map is confirmed by the dispatcher before
  stage 2 (a lightweight gate: the map is in the plan; the dispatcher
  checks it against the report's numbers and the census-visible test
  names used by ux-coverage evidence — renaming binaries moves the
  gate's evidence paths, so the ux-coverage scripts' expectations must
  be updated in the same change or the map adjusted).
- **Acceptance**: the recorded map implemented; the test-binary count
  and the `--all-targets` CPU measured before/after (identical
  method); the full suite green with zero test-case renames (binary
  grouping only — a renamed test case is a finding); the ux-coverage/
  seam-inventory surfaces that reference test paths updated
  consistently; CI unchanged in behavior (same checks, faster); diffs
  confined to Cargo.toml targets, test-file moves/merges, and the
  recorded tooling-path updates.

**Prompt R4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk R4 of round 3 ("consolidate test binaries", branch refactor/consolidate-test-binaries) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). The --all-targets measurements route through scripts/memory-gate.sh, backgrounded with polls — R4's lane is EXCLUSIVE: no other chunk runs beside it.
[L] Round laws (binding): (c) zero test-case renames — binary grouping only; a gate/tooling path that moves must be updated in the same change; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b refactor/consolidate-test-binaries ../taskboard-WT-refactor-consolidate-test-binaries off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5 — one-behavior-one-test is about cases, not binaries), the scope report §A (normative), the dispatch plan §1/§5, issue #1070, and the runtime report §A (the 98-binary inventory, the per-unit costs, the ~300–400s target).
2. Read the seams: the 19 ui-slint probe pairs (which probes group by surface family), the 34 app cli_* files (group by verb family without losing the harness distinctions the tests rely on — shared fixtures vs per-file), the Cargo.toml [[test]] targets per crate, the ux-coverage/seam-inventory scripts' path expectations (scripts/ux-coverage.sh, tests/seam_inventory.rs — which evidence paths name test binaries), and the census/journey docs that cite test paths.
3. Decide the binary map (per crate: which files merge into which target, the target names, what stays separate and why — harness conflicts like B4a's DriverOptions mix are the precedent for keeping a split), the tooling-path update list (every script/doc that names a moved path), and the measurement plan (before/after --all-targets, the suite wall, the nextest filtering smoke — a named test filter still resolves).
4. Write the chunk plan at .artifacts/plans/2026-11-19-consolidate-test-binaries-plan.md with the map as the reviewable artifact. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the test binary consolidation chunk plan. Do not implement; do not push. Report that the map is GATE-PAUSED for dispatcher confirmation.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the map summary, the tooling-path list, risks, estimated diff size.
```

**Prompt R4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk R4 of round 3 (branch refactor/consolidate-test-binaries). GATE CLEARED: the dispatcher confirmed the binary map — implement exactly it.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended). The before/after --all-targets measurements route through scripts/memory-gate.sh, backgrounded with polls; your lane stays exclusive.
[L] Round laws (binding): (c) zero test-case renames (verify: the test-name set before and after is identical — a diff of `cargo nextest list` outputs is the evidence); every tooling path in the update list is updated in the same commit series; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§5, and the stage-1 plan .artifacts/plans/2026-11-19-consolidate-test-binaries-plan.md in ../taskboard-WT-refactor-consolidate-test-binaries (recreate on the branch if missing; landed branch ⇒ refactor/consolidate-test-binaries-v2).
2. Implement exactly per the map, per-crate commits: the [[test]] target changes, the file merges (git mv preserves history), the tooling-path updates. The before/after measurements + the test-name-set identity diff recorded in the chunk report.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: before/after numbers, the name-set identity evidence, per-gate results, deviations.
```

**Prompt R4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk R4 of round 3 (branch refactor/consolidate-test-binaries).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (c) the test-name set is identical (re-derive the two lists yourself); no test body changed except mechanical import/merge adjustments — a semantic test edit is a finding; every tooling path that names a moved binary is updated (grep the scripts/tests/docs for the old paths); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §A (normative), the dispatch plan §1/§5 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-refactor-consolidate-test-binaries.
2. Functional focus — this chunk's traps: (a) merged harness distinctions survive (shared OnceLock/state assumptions from B4a's finding class — a merge that recreates the #1016 DriverOptions mix is a finding); (b) Cargo.toml target declarations match the map exactly (no orphaned files, no dead [[test]] entries); (c) the measurement delta is honest and hits the recorded target band or explains the miss; (d) CI behavior identical (same check commands — only faster).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1070 — closure is the dispatcher's at merge; the body notes #327's mutants lane should schedule after this lands), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, the delta vs the target band.
```

## 6. Chunk B1 — LogHabit equal-clock tie (#986, `fix/domain-loghabit-tie`)

- **Covers** (normative): the equal-clock LWW tie silently drops the
  second increment. The DEBT PIN test
  (`log_same_instant_double_currently_pins_the_lww_tie_loss`) and the
  Σ property's `dt >= 1s` exclusion are the rails. **Round-level
  decision frame** (stage-1 picks with recorded rationale): the tie
  must be resolved by *ordering*, not by dropping — candidates: (a) a
  monotone per-habit sequence/ordinal on log rows (ties break by
  arrival order, deterministic across replay), (b) sub-second stamp
  precision, (c) equal-instant increments add (the tie becomes
  additive). The sync/replay determinism constraint decides: whichever
  rule is chosen must give every replica the same result from the same
  journal (deterministic replay is the law). The pin test flips to the
  desired semantics (rename per the AGENTS §5 convention — the
  `currently_pins` name reads as debt), the Σ property **includes**
  the degenerate sequence (a strengthening — the `dt >= 1s`
  exclusion's removal is the acceptance evidence), and the sync wire's
  compatibility is checked (a journal-format change is a migration
  question — stage-1 records whether the chosen rule needs one).
- **Acceptance**: the chosen rule implemented with typed outcomes; the
  pin test now asserts the second increment survives; the Σ property
  runs with the degenerate sequence included and passes; existing
  habit corpus snapshots unchanged or per-diagnosis; the
  replay-determinism argument recorded; diffs confined to the domain
  habit module (+ the storage/replay seam if the rule needs it).

**Prompt B1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B1 of round 3 ("LogHabit equal-clock tie", branch fix/domain-loghabit-tie) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) the Σ property's exclusion is REMOVED (the degenerate sequence joins) — a fix that keeps the exclusion is a finding; the pin test flips to desired semantics per the AGENTS §5 naming convention; deterministic replay across replicas is the law the rule must satisfy; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/domain-loghabit-tie ../taskboard-WT-fix-domain-loghabit-tie off the then-current main (never without -b, never on main).

1. Read AGENTS.md (§5), the scope report .artifacts/reports/2026-11-19-backlog-reduction-round3-scope-report.md §B1 (normative), the dispatch plan .artifacts/plans/2026-11-19-backlog-reduction-round3-dispatch-plan.md §1/§6, issue #986 (the DEBT PIN test reference), and the r9 H1 plan's habit-domain context.
2. Read the seams: crates/taskboard-domain/src/habit.rs (RecordHabitLog's LWW tie, the pin test, the Σ property's dt>=1s exclusion), the journal/changeset encoding of log rows (does a sequence/ordinal field exist? what would a migration cost?), and the sync replay path (where habit logs apply on a replica).
3. Decide the tie rule among the recorded frame's candidates (sequence-ordinal / sub-second precision / additive tie) with the replay-determinism argument and the migration-cost verdict (the #843/#913 precedents: a domain rule change that needs a journal migration says so explicitly). Also decide the Σ-property's new degenerate-sequence case shape.
4. Write the chunk plan at .artifacts/plans/2026-11-19-domain-loghabit-tie-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the loghabit tie chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the chosen rule + determinism argument, the migration verdict, risks, estimated diff size.
```

**Prompt B1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B1 of round 3 (branch fix/domain-loghabit-tie).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) the pin test flips (desired semantics, renamed), the Σ property's exclusion removed and the degenerate case included — both are acceptance evidence; existing snapshots unchanged or per-diagnosis; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B1 (normative), the dispatch plan §1/§6, and the stage-1 plan .artifacts/plans/2026-11-19-domain-loghabit-tie-plan.md in ../taskboard-WT-fix-domain-loghabit-tie (recreate on the branch if missing; landed branch ⇒ fix/domain-loghabit-tie-v2).
2. Implement exactly per the plan, TDD: the pin test red under the current drop behavior, the rule, green; the Σ property with the degenerate sequence included. Diff confined to the plan's recorded seams; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence, the snapshot delta, per-gate results, deviations.
```

**Prompt B1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B1 of round 3 (branch fix/domain-loghabit-tie).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the replay-determinism argument holds for the chosen rule — construct the replica scenario in review (two journals with the same rows in different arrival orders converge to the same state) and check it; the exclusion removal is in the diff; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B1 (normative), the dispatch plan §1/§6 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-domain-loghabit-tie.
2. Functional focus — this chunk's traps: (a) the rule's ordering key is total (no new tie inside the tiebreak — a sequence collision under the chosen scheme is a finding); (b) the sync wire/migration verdict is implemented as recorded (a needed migration silently skipped, or an unneeded one added, is a finding); (c) existing habit behavior for distinct-instant logs is unchanged (the corpus snapshots prove it).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #986 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 7. Chunk B2 — journal decode robustness (#1039 + #1038, `fix/journal-decode-robustness`) — operator gate on the policy

- **Covers** (normative): one undecodable row aborts the entire state
  load at boot; `RepositoryError::Corrupted` carries no decode context
  (one bad row costs a bisect). **Round-level decision frame**: the
  boot policy is data-loss-adjacent — the safe default is **abort
  with full typed context** (row index/table/reason in `Corrupted`),
  and row-quarantine (skip-and-continue) is only acceptable with an
  explicit, surfaced, typed degraded-boot signal the user can act on —
  never a silent drop. Stage-1 records the recommendation; the
  dispatcher confirms the policy with the operator before stage 2.
- **Acceptance**: the policy implemented as recorded; `Corrupted`
  carries the decode context (typed fields, not a string dump — the
  #1038 acceptance); the failure path pinned by tests (a corrupted
  row at index N produces the recorded outcome with the context);
  the boot corpus unchanged on healthy data; diffs confined to the
  storage/replay seam + the error type.

**Prompt B2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B2 of round 3 ("journal decode robustness", branch fix/journal-decode-robustness) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes — the context is typed fields (row identity, table, decode stage, reason kind), never a formatted string; (c) never weaken a correct test; the healthy-data boot path is untouched; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/journal-decode-robustness ../taskboard-WT-fix-journal-decode-robustness off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B1 (normative), the dispatch plan §1/§7, issues #1039 #1038, and the storage architecture docs (docs/architecture.org's persistence section).
2. Read the seams: the state-load/journal replay path (where the abort fires), RepositoryError::Corrupted's construction sites (what context exists at each), the storage-sqlite row decode, and the boot gate's error surfacing.
3. Decide the policy per the dispatch plan's frame: abort-with-context vs quarantine-with-signal — record the recommendation with the data-loss analysis (what a skip loses, what a user sees). Specify Corrupted's typed fields, the context capture at each construction site, and the test matrix (corrupt row at head/middle/tail, multi-table).
4. Write the chunk plan at .artifacts/plans/2026-11-19-journal-decode-robustness-plan.md. Resolve every open question with a recorded decision. Report that the policy is GATE-PAUSED for the operator.
5. Commit as: docs(plan): the journal decode robustness chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the policy recommendation + data-loss analysis, risks, estimated diff size.
```

**Prompt B2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B2 of round 3 (branch fix/journal-decode-robustness). GATE CLEARED: the operator confirmed the policy — implement exactly the recorded recommendation.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed context fields; (c) healthy-data path untouched; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B1 (normative), the dispatch plan §1/§7, and the stage-1 plan .artifacts/plans/2026-11-19-journal-decode-robustness-plan.md in ../taskboard-WT-fix-journal-decode-robustness (recreate on the branch if missing; landed branch ⇒ fix/journal-decode-robustness-v2).
2. Implement exactly per the plan, TDD: the corrupt-row matrix red under the current context-free abort, the policy + typed context, green. Diff confined to the storage/replay seam + the error type; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence, per-gate results, deviations.
```

**Prompt B2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B2 of round 3 (branch fix/journal-decode-robustness).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) the context is typed and complete at every construction site (a site that can only fill partial context is a finding); (c) the healthy boot corpus is snapshot-identical; the policy's data-loss analysis matches what the code does (a quarantine implemented without the surfaced signal is a finding); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B1 (normative), the dispatch plan §1/§7 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-journal-decode-robustness.
2. Functional focus — this chunk's traps: (a) the context capture happens where the information still exists (a catch-site that re-derives context after it is lost is the #1038 bug re-created); (b) the error type's change is backward-compatible for its consumers (grep the matches! sites); (c) the test matrix covers head/middle/tail and multi-table as recorded.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1039 #1038 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 8. Chunk Bui — UI residue bugs (`fix/ui-r9-residue-bugs`)

- **Covers** (normative), five root-caused filings, one chunk:
  **#1004** (capture modal lost panel border + drop-shadow on the
  ModalDialogHost chrome move — restore the chrome via the base
  widget's styling, not a per-consumer patch), **#1026** (the palette's
  ModalDialogHost mount wires a dead `accepted()` seam; the J2 W-4/§7
  ActionId rows disagree — fix the seam and reconcile the rows),
  **#1033** (legacy stack-walk chords cancel a live board move —
  settle-then-run ordering), **#1036** (the embedded planner's mount
  steals focus — `PlanNextDayView` init's `self.focus()` inside the
  walk), **#1053** (the walk dialog's walk-stops arithmetic excludes
  the host button row). Each issue's body is normative for its fix.
- **Acceptance**: each fix pinned by a structural/journey test named
  per convention (no text assertions); the five fixes do not regress
  each other (the bundle's shared surface — the walk/dialog grammar —
  is tested as a set); the r9 journey corpus stays green; diffs
  confined to the named sites.

**Prompt Bui.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk Bui of round 3 ("UI residue bugs", branch fix/ui-r9-residue-bugs) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) structural/journey tests only — never visual-text assertions; (c) never weaken a correct test; each issue's recorded root cause is the fix's contract — re-verify it against current main first (the filings predate the last merges; a cause that moved needs a recorded re-anchor); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/ui-r9-residue-bugs ../taskboard-WT-fix-ui-r9-residue-bugs off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B2 (normative), the dispatch plan §1/§8, and issues #1004 #1026 #1033 #1036 #1053 (each body names its site and cause).
2. Read the seams per issue: the ModalDialogHost chrome vs the capture mount (#1004), the palette's accepted() wiring + the J2 W-4/§7 rows (#1026), the settle matrix's StackNext/Prev arms mid-move (#1033), PlanNextDayView's init focus call (#1036), the walk-stops arithmetic's row inventory (#1053).
3. Decide per issue: the fix shape and the pinning test (structural for geometry/chrome, journey for walk/focus semantics; step-ID tokens where a journey row applies). Note shared sites (the walk grammar serves #1033/#1036/#1053) — order the fixes to avoid churn.
4. Write the chunk plan at .artifacts/plans/2026-11-19-ui-r9-residue-bugs-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ui residue bugs chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, per-issue fix shapes, risks, estimated diff size.
```

**Prompt Bui.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk Bui of round 3 (branch fix/ui-r9-residue-bugs).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) structural/journey tests with step-ID tokens where applicable; (c) TDD — each pin fails pre-fix; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B2 (normative), the dispatch plan §1/§8, and the stage-1 plan .artifacts/plans/2026-11-19-ui-r9-residue-bugs-plan.md in ../taskboard-WT-fix-ui-r9-residue-bugs (recreate on the branch if missing; landed branch ⇒ fix/ui-r9-residue-bugs-v2).
2. Implement exactly per the plan, per-issue commits in the recorded order. Diff confined to the named sites; every commit standalone; new slint seams carry the `// slint: <callback>` comment.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-issue red→green evidence, per-gate results, deviations.
```

**Prompt Bui.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk Bui of round 3 (branch fix/ui-r9-residue-bugs).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) no text assertions; (c) the five fixes compose — run the walk/dialog journey set as a whole (the bundle's shared-surface argument); each fix's diff hunk traces to its issue's recorded cause — a drive-by refactor is a finding; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B2 (normative), the dispatch plan §1/§8 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-ui-r9-residue-bugs.
2. Functional focus — this chunk's traps: (a) #1004's chrome restore uses the base widget's styling path (a per-consumer restyle re-creates the duplication O-6 banned); (b) #1026's row reconciliation updates the J2 doc rows it disagrees with (the doc and code land together); (c) #1033's ordering fix does not break the legitimate cancel path (the pre-move cancel semantics stay pinned); (d) #1036's focus fix respects the walk's ownership (the planner step still receives focus when the journey reaches it — the N-3 restoration law composes).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references all five issues — closures are the dispatcher's at merge), report the URL + per-issue verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict.
```

## 9. Chunk B3 — app-test hygiene (#984/#1013/#982/#981, `test/app-test-hygiene`)

- **Covers** (normative): **#984** — the app test binaries compose
  ambient `TASKBOARD_*` without the env lock (hostile-operator
  exposure; extend the landed C6/#865 env-lock pattern to every
  composing binary); **#1013** — bootstrap's device-identity home
  stays ambient (in-process suites mint `device.json` in the
  operator's real home — inject the identity home per the #751
  pattern's remainder); **#982** — `test_workspace_with_app` drops its
  tempdir while the booted app still points at it (ownership/
  lifetime fix); **#981** — fold bootstrap.rs's and ui_requests.rs's
  local `RefusingStore` doubles onto the shared `test_support`.
- **Acceptance**: the hostile-env pass green across the app test
  binaries; no test writes outside its workspace (grep-verifiable: the
  device-home injection covers every bootstrap consumer); the tempdir
  lifetime is pinned (the app's shutdown precedes the drop — the
  ordering testable); the doubles are gone (one shared definition);
  diffs confined to test helpers/fixtures + the recorded injection
  seams.

**Prompt B3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B3 of round 3 ("app-test hygiene", branch test/app-test-hygiene) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) never weaken a correct test — the sweep extends coverage; the edition-2024 env rules apply (the C6/#865 patterns; unsafe only where the integration-test crate allows it, never in a forbid(unsafe) lib); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/app-test-hygiene ../taskboard-WT-test-app-test-hygiene off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B3 (normative), the dispatch plan §1/§9, issues #984 #1013 #982 #981, and the landed patterns: the C6 env-lock helper, the #865 option-2 decision, the #751 identity-injection shape.
2. Read the seams: the app test binaries' config composition sites (#984's sweep inventory), bootstrap's device-identity home resolution and every in-process suite consumer (#1013), test_workspace_with_app's ownership chain (#982), the two RefusingStore doubles + the shared test_support crate (#981).
3. Decide per issue: the env-lock application shape (per-binary fixture vs shared helper import), the identity-home injection's parameter path (which boot functions take it; the default stays ambient for the real binary), the lifetime fix (who owns the tempdir, what the drop ordering guarantee is), the dedup's shared shape.
4. Write the chunk plan at .artifacts/plans/2026-11-19-app-test-hygiene-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the app test hygiene chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the sweep inventory, decisions, risks, estimated diff size.
```

**Prompt B3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B3 of round 3 (branch test/app-test-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) the hostile-env pass is the acceptance run (TASKBOARD_* hostile values, full app test set); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B3 (normative), the dispatch plan §1/§9, and the stage-1 plan .artifacts/plans/2026-11-19-app-test-hygiene-plan.md in ../taskboard-WT-test-app-test-hygiene (recreate on the branch if missing; landed branch ⇒ test/app-test-hygiene-v2).
2. Implement exactly per the plan, per-issue commits. Diff confined to test helpers/fixtures + the recorded injection seams; every commit standalone.
3. Prove per the plan: hostile-env pass, the no-real-home-write grep, the lifetime ordering.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, the proof evidence, per-gate results, deviations.
```

**Prompt B3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B3 of round 3 (branch test/app-test-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the injection's default path keeps the real binary ambient (a production behavior change is a finding); no test writes outside its workspace after the diff (re-run the grep yourself); the doubles' dedup loses no behavior (diff the two old doubles' semantics against the shared one); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B3 (normative), the dispatch plan §1/§9 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-app-test-hygiene.
2. Functional focus — this chunk's traps: (a) the env lock's save/clear/restore is complete (the C6 lesson: save-then-clear-all, not a fixed list); (b) #982's fix is an ownership change, not a sleep-before-drop; (c) #1013's injection covers the boot gate AND the ceremony paths a suite can reach (grep every device.json consumer).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #984 #1013 #982 #981 — closures are the dispatcher's at merge), report the URL + per-issue verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-issue verdict.
```

## 10. Chunk B4a — journey-harness helper + binary split (#1062 + #1016, `refactor/journey-harness-helper`)

- **Covers** (normative): **#1062** (p2 — the journey files re-trip
  SonarCloud's new-code duplication; extract the shared setup into a
  journey-harness helper module the journey files import) and
  **#1016** (p2 — the `journeys` binary mixes `DriverOptions { headless_renderer: true/false }` and plain `cargo test` fails wholesale on the OnceLock assert; split the headless:false variants into their own binary — or drop them, only if the recorded check proves no snapshot/golden tier depends on them). Both structural; the helper lands first, the split rides it.
- **Acceptance**: plain `cargo test --test journeys` (and the new
  sibling binary) passes wholesale — the OnceLock mix is gone; the
  helper absorbs the duplicated setup (the SonarCloud finding's shape
  — measured: the duplication count before/after or the recorded
  equivalent); the journey corpus green under nextest AND plain cargo
  test; diffs confined to the journey files + the helper + Cargo.toml
  targets.

**Prompt B4a.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B4a of round 3 ("journey-harness helper + split", branch refactor/journey-harness-helper) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) the journey corpus is behavior-identical — a journey test whose outcome changes is a finding; the split's drop-option requires the recorded snapshot-tier check; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b refactor/journey-harness-helper ../taskboard-WT-refactor-journey-harness-helper off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§10, issues #1062 #1016, and the r9 habit-row-rework context the #1016 body cites.
2. Read the seams: the journey files' shared setup shape (what duplicates — the inventory of the duplicated blocks), the driver::run OPTIONS OnceLock (the assert site), which of the 9 headless:false tests actually take snapshots/goldens (the drop-vs-split evidence), the Cargo.toml journey targets.
3. Decide: the helper module's API (what it owns: driver setup, window/rig bootstrapping, the settle helpers), the split shape (new binary name, which tests move, the golden-tier verification), and the SonarCloud-duplication measurement method.
4. Write the chunk plan at .artifacts/plans/2026-11-19-journey-harness-helper-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the journey harness helper chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the helper API, the split decision + evidence, risks, estimated diff size.
```

**Prompt B4a.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B4a of round 3 (branch refactor/journey-harness-helper).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) the plain cargo test acceptance is explicit (cargo test --test journeys AND the new binary, wholesale green); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§10, and the stage-1 plan .artifacts/plans/2026-11-19-journey-harness-helper-plan.md in ../taskboard-WT-refactor-journey-harness-helper (recreate on the branch if missing; landed branch ⇒ refactor/journey-harness-helper-v2).
2. Implement exactly per the plan: the helper extraction, the binary split per the recorded evidence, the duplication measurement before/after. Diff confined per the plan; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended) PLUS the plain-cargo-test acceptance per the plan. The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: the plain-cargo-test evidence, the duplication delta, per-gate results, deviations.
```

**Prompt B4a.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B4a of round 3 (branch refactor/journey-harness-helper).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) no journey test's semantics changed (the diff is extraction + moves — a semantic edit is a finding); the split's tier evidence holds (a dropped test that took goldens is a finding); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§10 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-refactor-journey-harness-helper.
2. Functional focus — this chunk's traps: (a) the helper is the single setup path (a journey file still hand-rolling setup is the duplication the chunk exists to kill); (b) the new binary's Cargo.toml target is complete (no orphaned file); (c) the ux-coverage evidence paths that name the journeys binary still resolve (grep the scripts — the #1070 lesson).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1062 #1016 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 11. Chunk B4b — journey/backend pins (#1023/#1024/#995/#996/#997/#1059/#1018, `test/journey-backend-pins`) — after B4a

- **Covers** (normative), seven small pins/fixes on the testing
  backend and journey corpus, using B4a's helper: **#1023** (stale
  geometry for conditional remounts — changed-mirror + element-query
  fail), **#1024** (FakeEngine's UpdateTask settle arm folds the
  effort change mask), **#995** (raw focus restored onto a disabled
  FocusScope across a visibility toggle), **#996** (DialogButton
  initial-focus index is site-derived — Slint functions have no model
  reduce; decide the seam), **#997** (bind the chunk-10 habit editor
  journey tests into J1's wiring inventory — the us6 gate rows
  resolve), **#1059** (pin the habit editor's invalid-input and
  armed-remove escape legs as journeys), **#1018** (geometry probe for
  the habit card's entry-height parity).
- **Acceptance**: each item pinned/fixed per its body; the harness
  fixes (#1023/#995/#996) are typed-backend changes with behavior
  tests; the journey additions carry step-ID tokens where a plan row
  applies (#997's wiring rows resolve through the gate); diffs confined
  to the testing backend + journey files + the wiring inventory.

**Prompt B4b.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B4b of round 3 ("journey/backend pins", branch test/journey-backend-pins) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) never weaken a correct test; #997's gate rows must RESOLVE (run scripts/ux-coverage.sh on the affected plan — MISSING is not done); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b test/journey-backend-pins ../taskboard-WT-test-journey-backend-pins off the then-current main (never without -b, never on main). B4a must have landed — verify (gh pr list --state merged or the issue states); if not, STOP and report.

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§11, the seven issues, and the landed B4a helper (its API is the composition base).
2. Read the seams per issue (bodies name them): the testing backend's geometry/changed-mirror machinery (#1023), the FakeEngine settle arms (#1024), the focus-restore path (#995), DialogButton's focus model (#996), J1's wiring inventory + the chunk-10 habit journeys (#997), the habit editor's escape legs (#1059), the habit card's entry-height probe point (#1018).
3. Decide per item: the fix/pin shape, the test, and the shared-seam ordering (the backend items first — #1023/#995/#996 — then the journey pins that ride them).
4. Write the chunk plan at .artifacts/plans/2026-11-19-journey-backend-pins-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the journey backend pins chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, per-item shapes, risks, estimated diff size.
```

**Prompt B4b.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B4b of round 3 (branch test/journey-backend-pins).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) TDD — backend fixes red-first where the behavior is testable; journey pins follow the naming/step-token conventions; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§11, and the stage-1 plan .artifacts/plans/2026-11-19-journey-backend-pins-plan.md in ../taskboard-WT-test-journey-backend-pins (recreate on the branch if missing; landed branch ⇒ test/journey-backend-pins-v2).
2. Implement exactly per the plan in the recorded order: backend fixes, then the journey pins/wiring. Diff confined per the plan; every commit standalone.
3. Run the affected plan's coverage gate where the plan recorded it (#997) — MISSING on a claimed row is not done.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the gate run evidence, per-gate results, deviations.
```

**Prompt B4b.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B4b of round 3 (branch test/journey-backend-pins).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the backend fixes don't change journey semantics beyond the pinned behaviors (the corpus green with per-plan diagnosis for any expectation change); every new journey uses the B4a helper (hand-rolled setup is a finding); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B4 (normative), the dispatch plan §1/§11 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-test-journey-backend-pins.
2. Functional focus — this chunk's traps: (a) #1023's staleness fix addresses the remount invalidation, not a symptom patch (the changed-mirror contract is the check); (b) #996's seam decision respects the Slint function limitation the issue names (a model-reduce workaround that doesn't compile is the recorded trap); (c) #1024's mask fold matches the production UpdateTask semantics (compare against the real settle path); (d) the wiring inventory update keeps the inventory ↔ seam cross-check green (#760's landed state).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references all seven issues — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 12. Chunk B5 — gate/law/doc residue (`ci/gate-law-doc-residue`)

- **Covers** (normative), one scripts+docs chunk: **#965** (the gate's
  token matching is fixed-substring — `on_save` matches
  `on_save_extra`; token-boundary matching), **#967** (the evidence
  universe includes the self-test fixture tree — exclude
  `messages.rs`/`requests.rs` fixtures), **#970** (the universe scan
  includes `target/` artifacts under `--root` — exclude build
  output), **#964** (stroke.rs duplicated doc-comment paragraph —
  dedup), **#1063** (the resource-law clarification — dispatcher suite
  = one slot — into `docs/agent_dispatch.org`), **#1043** (scratch
  dirs on tmpfs — the ENOSPC law: codify the TMPDIR-off-tmpfs routing
  the memory gate already does, extend to bench/scratch scripts),
  **#1042** (the doctor sweep vs bench/scratch worktrees — protect
  measured worktrees via the recorded convention), **#1046** (the
  tokio advance-to-deadline gotcha into the clock-law docs),
  **#1047** (the `visible:false` layout-slot footgun into the widget
  docs).
- **Acceptance**: each item landed per its body; the gate residue
  fixes carry self-tests (the #920/#768/#760 precedent — the gate's
  behavioral states verified); the law/doc edits are consistent with
  the existing text (no drift); diffs confined to scripts/, docs/,
  and the named test-support file.

**Prompt B5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B5 of round 3 ("gate/law/doc residue", branch ci/gate-law-doc-residue) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) machine-readable gate states; (c) no weakened checks — #965's boundary matching must not turn honest matches into misses; the exclusions (#967/#970) are evidence-scoped, not blanket; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b ci/gate-law-doc-residue ../taskboard-WT-ci-gate-law-doc-residue off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B5 (normative), the dispatch plan §1/§12, the nine issues, and the landed gate base (the N-A scoping/bound/inventory states in scripts/ux-coverage.sh).
2. Read the seams per issue (bodies name them): the matcher (#965), the evidence-universe scan (#967/#970), stroke.rs (#964), agent_dispatch.org's resource law (#1063), the memory gate's TMPDIR routing + the bench/scratch scripts (#1043), worktree-doctor.sh's sweep predicate (#1042), testing_strategy.org §6 (#1046), the widget docs' home for #1047.
3. Decide per item: the fix/doc shape and the self-test where behavioral (#965/#967/#970), the law text's exact wording where it amends the dispatch law (#1063/#1043/#1042 — the codification must not contradict the existing two-mode law).
4. Write the chunk plan at .artifacts/plans/2026-11-19-gate-law-doc-residue-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the gate law doc residue chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt B5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B5 of round 3 (branch ci/gate-law-doc-residue).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) the gate's behavioral fixes carry self-tests (the N-A precedent); (c) the law amendments state constraints, not narration; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B5 (normative), the dispatch plan §1/§12, and the stage-1 plan .artifacts/plans/2026-11-19-gate-law-doc-residue-plan.md in ../taskboard-WT-ci-gate-law-doc-residue (recreate on the branch if missing; landed branch ⇒ ci/gate-law-doc-residue-v2).
2. Implement exactly per the plan, grouped commits (gate fixes / law docs / small docs). Diff confined to scripts/, docs/, and the named file; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the self-test evidence, per-gate results, deviations.
```

**Prompt B5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B5 of round 3 (branch ci/gate-law-doc-residue).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) re-run the gate self-tests yourself; (c) the boundary matching's edge cases are pinned (a token that is a prefix of another, and the reverse); the exclusions keep honest evidence reachable (construct the case where a fixture-tree hit WAS the evidence and check it still is when legitimately outside the fixtures); the law text agrees with the memory-gate/doctor code (read both); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B5 (normative), the dispatch plan §1/§12 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-ci-gate-law-doc-residue.
2. Functional focus — this chunk's traps: (a) the five r9/N-A-era gate states still behave (the residue fixes are additive); (b) #1042's protection mechanism cannot be abused to hoard stale worktrees (the doctor's disposal law still wins for landed-PR worktrees — read the interaction); (c) the doc additions cite their issue/plan origins per the house convention.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references the nine issues — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 13. Chunk B6 — IPC/state follow-ups (#993/#994/#991/#990, `fix/ipc-restore-followups`)

- **Covers** (normative): **#993** (CommandOutcome serde round-trip
  pin — the restore-family receipts' decode stability), **#994** (the
  stop-ordering plan D-2.3 over-counts the fallback sites — the
  logout/connect nudges never boot a daemon; correct the divergence:
  plan or impl, per stage-1's read), **#991** (status during the
  stop-drain window distinguishes a draining daemon from no daemon —
  typed), **#990** (RestoreTimeEntry repeat of an identical present
  row plans an upsert + flush trigger instead of a no-op — idempotent
  restore).
- **Acceptance**: the serde pin round-trips every CommandOutcome
  variant (a missing variant is a finding); #994's correction is
  recorded on the issue and in the affected plan/doc; #991's typed
  status is pinned (draining vs absent are distinct outcomes);
  #990's idempotency pinned (repeat restore = no-op, no flush
  trigger); diffs confined to the state/ipc seams + tests + the
  recorded doc corrections.

**Prompt B6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B6 of round 3 ("IPC/state follow-ups", branch fix/ipc-restore-followups) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) never weaken a correct test; #994's divergence correction follows the recorded reality — the plan is amended only where the code is right, the code fixed where the plan is right (record which per site); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/ipc-restore-followups ../taskboard-WT-fix-ipc-restore-followups off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B6 (normative), the dispatch plan §1/§13, the four issues, and the tier-2 landed base (the stop-ordering plan D-2.3, the restore-family machinery).
2. Read the seams: CommandOutcome's serde derive + variants (#993), the stop-ordering sites D-2.3 names vs the code (#994), the status query's daemon-state source (#991), the RestoreTimeEntry apply path's duplicate detection (#990).
3. Decide per issue: the pin's shape (a round-trip over every variant — generated or enumerated), the divergence disposition per site, the draining status's type (new variant vs a field), the idempotency rule (what "identical present row" means exactly — the comparison's fields).
4. Write the chunk plan at .artifacts/plans/2026-11-19-ipc-restore-followups-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ipc restore followups chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt B6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B6 of round 3 (branch fix/ipc-restore-followups).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) TDD; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B6 (normative), the dispatch plan §1/§13, and the stage-1 plan .artifacts/plans/2026-11-19-ipc-restore-followups-plan.md in ../taskboard-WT-fix-ipc-restore-followups (recreate on the branch if missing; landed branch ⇒ fix/ipc-restore-followups-v2).
2. Implement exactly per the plan, per-issue commits. Diff confined per the plan; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-issue outcome, per-gate results, deviations.
```

**Prompt B6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B6 of round 3 (branch fix/ipc-restore-followups).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the serde pin fails on a variant addition (verify the enumeration is exhaustive — a hand-list that can drift is a finding; prefer a generated/exhaustive form); #990's comparison fields are the recorded set exactly; #991's new state does not break existing status consumers (grep the matches! sites); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B6 (normative), the dispatch plan §1/§13 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-ipc-restore-followups.
2. Functional focus — this chunk's traps: (a) #994's per-site disposition is recorded in the doc it corrects (the plan and code land together); (b) the drain-status distinction is observable from the client side (the pin tests the wire, not the internal enum); (c) the idempotent restore's no-op is a typed outcome, not a silent early-return the flush path misses.
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references the four issues — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 14. Chunk B7 — config batch unset + manual reload (#1014 + #1015, `feat/config-batch-unset-reload`) — journey-gated

- **Covers** (normative), two stories under one journey plan (the P6
  precedent; siblings of the accepted #845/#909/#910 config family):
  **#1014** batch config unset (multi-key transaction parity with the
  landed batch set — one intent, one atomic proven document) and
  **#1015** manual config reload CLI verb (the watcher lane's escape
  hatch — explicit trigger of the same reload path). **Journey gate**:
  stage 1 writes `.artifacts/plans/2026-11-19-config-batch-unset-reload-ux-plan.md`
  + the chunk plan; the dispatcher pauses for operator acceptance;
  stages 2–3 proceed on the accepted journey.
- **Acceptance**: the accepted journeys' wiring rows implemented;
  batch unset is atomic (a failing key rolls back the whole write,
  typed error names the key); the reload verb produces the same
  converged state as the watcher lane (pinned equivalence); typed
  outcomes throughout; hostile-env pass green; `scripts/ux-coverage.sh
  <journey-plan> --gate` green on the claimed rows.

*(Prompts B7.1–B7.3 follow the established journey-gated pattern
(C10/P4/P6), adapted: branch `feat/config-batch-unset-reload`,
worktree `../taskboard-WT-feat-config-batch-unset-reload`, journey
plan `2026-11-19-config-batch-unset-reload-ux-plan.md`, chunk plan
`2026-11-19-config-batch-unset-reload-plan.md`, origin section §14,
issues #1014 #1015; stage 1 reports GATE-PAUSED; stage 2 opens "GATE
CLEARED"; stage 3 carries the code-quality + UX completeness mandates
and references both issues.)*

## 15. Chunk B7b — files_sync.poll_secs live-write lane (#1051, `feat/files-sync-poll-write-lane`)

- **Covers** (normative): `files_sync.poll_secs` has no live-write
  lane — no overlay writer, no apply-registry entry — so `config set`
  of it cannot propagate to the running actor (the #673 reader-side
  re-arm landed; the writer side is missing). Implement the
  apply-registry entry + overlay writer for the key so the accepted
  C10 propagation contract covers it.
- **Stage-1 verification**: the accepted C10 journey's contract covers
  this key-class (record the citation); if the journey's scope
  excluded it, the finding goes to the operator at plan review (a
  possible journey amendment), not silent scope growth.
- **Acceptance**: `config set files_sync.poll_secs <n>` propagates
  live (the actor's next wake honors it — the #673 test pattern);
  the apply-registry entry matches the registered keys' shape; the
  hostile-env pass green; diffs confined to the config writer lane +
  the registry + tests.

**Prompt B7b.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B7b of round 3 ("files-sync poll write lane", branch feat/files-sync-poll-write-lane) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) the C10 contract citation is recorded — scope growth beyond it goes to the operator, never silent; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/files-sync-poll-write-lane ../taskboard-WT-feat-files-sync-poll-write-lane off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B7 (normative), the dispatch plan §1/§15, issue #1051, and the accepted base: the C10 journey (.artifacts/plans/2026-11-17-config-set-propagation-ux-plan.md — its contract and apply-registry shape), the #673 reader-side re-arm, the #845/#915 landed writer lane.
2. Read the seams: the apply-registry (registered keys' shape), the overlay writer lane, the files-sync actor's settings read (the #673 seam), and the poll_secs key's config path.
3. Decide: the registry entry + writer shape (mirroring an existing registered key), the live-propagation test (the #673 pattern: shorten/lengthen), and the C10-contract citation (or the amendment finding).
4. Write the chunk plan at .artifacts/plans/2026-11-19-files-sync-poll-write-lane-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the files sync poll write lane chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the citation verdict, decisions, risks, estimated diff size.
```

**Prompt B7b.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B7b of round 3 (branch feat/files-sync-poll-write-lane).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (b) pause/advance for the propagation test; (c) TDD; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B7 (normative), the dispatch plan §1/§15, and the stage-1 plan .artifacts/plans/2026-11-19-files-sync-poll-write-lane-plan.md in ../taskboard-WT-feat-files-sync-poll-write-lane (recreate on the branch if missing; landed branch ⇒ feat/files-sync-poll-write-lane-v2).
2. Implement exactly per the plan, TDD: the registry entry + writer, the live-propagation test. Diff confined per the plan; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence, per-gate results, deviations.
```

**Prompt B7b.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B7b of round 3 (branch feat/files-sync-poll-write-lane).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the registry entry matches the registered keys' contract (an inconsistent shape is a finding); the propagation honors invalid values typed (the #673 fallback rule); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B7 (normative), the dispatch plan §1/§15 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-files-sync-poll-write-lane.
2. Functional focus — this chunk's traps: (a) the write lane and the #673 reader meet (a set during an in-flight cycle honors at the next wake — the recorded min-rule semantics); (b) no other key gained a lane silently (diff audit).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #1051 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 16. Chunk B8 — small refactor bundle (#978/#1034/#1060/#1065/#1027, `refactor/ui-small-grammar-bundle`)

- **Covers** (normative), five refactors with existing test cover,
  one chunk: **#978** (apply the R-6 menu width tokens at the dropdown
  consumer mounts), **#1034** (subsume the ControlNext Tab re-injection
  under the directional walk stop ladder), **#1060** (switch_plan_pane's
  wrap arithmetic → a two-branch match), **#1065** (de-duplicate the
  FocusScope/Return-Space button grammar across the centralized
  widgets), **#1027** (pin the date-picker/editor overlay-exclusivity
  fold row the chunk-5 plan promised in om_us2_s1).
- **Acceptance**: each refactor behavior-identical (the existing corpus
  green, per-plan diagnosis for any expectation change); #1027's pin
  lands as the promised row (the plan's debt retired); diffs confined
  to the named widgets/files.

**Prompt B8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk B8 of round 3 ("small refactor bundle", branch refactor/ui-small-grammar-bundle) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) behavior-identical refactors — the corpus is the oracle; any expectation change needs the recorded diagnosis; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b refactor/ui-small-grammar-bundle ../taskboard-WT-refactor-ui-small-grammar-bundle off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B8 (normative), the dispatch plan §1/§16, the five issues, and the O-6 centralization base (the widget inventory report — the grammar shapes #1065 dedups).
2. Read the seams per issue (bodies name them): the dropdown consumer mounts + the R-6 width tokens (#978), the ControlNext re-injection site + the stop ladder (#1034), switch_plan_pane's arithmetic (#1060), the FocusScope/Return-Space grammar across Segmented et al (#1065), the om_us2_s1 fold row's promised state (#1027).
3. Decide per item: the refactor shape, the behavior-identity evidence, and the shared-site ordering (#1034/#1065 touch the walk grammar — order to avoid churn).
4. Write the chunk plan at .artifacts/plans/2026-11-19-ui-small-grammar-bundle-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ui small grammar bundle chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, per-item shapes, risks, estimated diff size.
```

**Prompt B8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk B8 of round 3 (branch refactor/ui-small-grammar-bundle).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (c) behavior-identical — the corpus is the oracle; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B8 (normative), the dispatch plan §1/§16, and the stage-1 plan .artifacts/plans/2026-11-19-ui-small-grammar-bundle-plan.md in ../taskboard-WT-refactor-ui-small-grammar-bundle (recreate on the branch if missing; landed branch ⇒ refactor/ui-small-grammar-bundle-v2).
2. Implement exactly per the plan in the recorded order, per-item commits. Diff confined per the plan; every commit standalone; new slint seams carry the `// slint: <callback>` comment.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the corpus-delta evidence, per-gate results, deviations.
```

**Prompt B8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk B8 of round 3 (branch refactor/ui-small-grammar-bundle).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) each refactor's hunk is mechanical (a semantic change hiding in a refactor is a finding); #978's tokens match the R-6 recorded values exactly; #1027's pin asserts the promised fold semantics; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the scope report §B8 (normative), the dispatch plan §1/§16 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-refactor-ui-small-grammar-bundle.
2. Functional focus — this chunk's traps: (a) #1065's dedup keeps the per-widget customization points (the O-6 rule: base widgets with customization, not uniformity by force — a variant that loses its distinct behavior is a finding); (b) #1034's subsumption removes the re-injection only where the stop ladder covers it (trace every ControlNext path).
3. Code-quality mandate (verbatim from §1). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references the five issues — closures are the dispatcher's at merge), report the URL + per-item verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL, per-item verdict.
```

## 17. Dispatcher runbook & at-merge closures (binding)

**Runbook**: per `docs/agent_dispatch.org` — stage-exit verification
between stages (resume over redo, playbook §8); the full §10 suite
between stage-2 landing and stage-3 ship of every code chunk; CI watch
+ `mergeStateStatus CLEAN` + absolute-path teardown after each
PR-open; **≤3 concurrent chunks**, staggered ≥3 min; **R4's lane is
exclusive** (its builds saturate the box); expected-heavy builds
through `scripts/memory-gate.sh`; gates pause at B2 (policy), B4.1's
map (dispatcher-confirmed), B7 (journey).

**At-merge closures** (dispatcher-owned): R1 → #1069; R2 → #1071
(+#968 per the disposition evidence); R3 → #1072; R4 → #1070; B1 →
#986; B2 → #1039, #1038; Bui → #1004, #1026, #1033, #1036, #1053; B3
→ #984, #1013, #982, #981; B4a → #1062, #1016; B4b → #1023, #1024,
#995, #996, #997, #1059, #1018; B5 → #965, #967, #970, #964, #1063,
#1043, #1042, #1046, #1047; B6 → #993, #994, #991, #990; B7 → #1014,
#1015; B7b → #1051; B8 → #978, #1034, #1060, #1065, #1027.

**Deliberately not dispatched here** (scope report §C): the journey-
gated feature families (#955–#963, #945–#949, #353/#354, #979, #1045,
#1011, #976), the r9-parity remainder sweep, bigger refactors
(#1029/#1037), the watch sets (chrono trio, Slint upstream,
conditionals), the flaky watchlist (armed #697/#650; #336/#287/#384/
#883/#1020), #327 (schedule after R4 lands), and the standing
perf/feature-scale deferrals.

**What opens next**: the r9-parity remainder round; the CLI-parity
feature round (#948 export largest); #327's sharded mutants lane
(post-R4); the chrono ruling's follow-ups per #988's watch.

## 18. Round amendment (2026-11-20): R2's armed findings scheduled in-round (operator instruction)

The operator scheduled R2's two armed findings as round-3 chunks:

- **R2b** (#1082, branch `fix/boot-pull-settle-predicate`): the
  boot-pull settle wait's predicate misses the skipped NoBoard cycle —
  the 10s boot quantizer R2's investigation convicted
  (`bootstrap.rs:1741`'s `wait_for_boot_pull`). The R2 investigation
  report (`.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`)
  is the diagnosis base; stage 1 plans the predicate fix directly from
  it (the fix is a production seam change with the settle contract
  pinned). Size S/M; three stages; the R2 report is normative context.
- **R2c** (#1085, branch `fix/files-sync-drain-contract`): the
  files-sync actor pins the engine's user-data drain across an
  in-flight cycle (Shutdown checked only between cycles → 30s trace).
  **Operator gate**: the fix shapes each change a contract — stage 1
  records the options with a recommendation (mid-cycle Shutdown check;
  cooperative cycle abort; keep the pin and shorten the budget — each
  with its semantics/risk) and the dispatcher pauses for the operator's
  choice before stage 2. Size M.

**Ordering**: both dispatch in the wave-3/4 window (Bui stays after
R4's landing — its ui-slint test-file diffs would collide with R4's
consolidation); ≤3 concurrent with the standing waves. At-merge
closures: R2b → #1082; R2c → #1085.
