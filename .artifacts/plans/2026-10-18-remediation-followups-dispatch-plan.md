# [Plan] Remediation follow-ups — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 5 chunks of the remediation-wave-1
  follow-up fixes (wave-1 leftovers + the post-wave findings #311–#330),
  delivered as **one branch and one PR**. No code in this commit.
- **Origin**: the issue bodies are normative — #242 (wave-1 leftover:
  sync actor/executor test gaps F6-04/05/06/23/24), #314 (update_task
  classified-rejection arms; analysis
  `.artifacts/reports/2026-10-17-update-label-rejection-arms-analysis-report.md`
  §Summary/§F1–F5/§Proposed fix shape), #319 (delete_card stale binding;
  `.artifacts/plans/2026-10-17-sync-stale-binding-clock-plan.md`
  §non-goals + ADR 0007), #316 (engine exit unreachability;
  `.artifacts/plans/2026-10-17-state-storage-gaps-plan.md` decision D2),
  #313 (sqlite test sidecar leak;
  `.artifacts/reports/2026-10-17-flaky-test-watchlist-investigation-report.md`
  §5), #323 (reminder persistence roundtrips;
  `.artifacts/plans/2026-10-17-execute-batch-exhaustive-plan.md` §7),
  #330 (changelog backfill; `.artifacts/plans/2026-10-18-arch-org-release-plan.md`
  §risks bookkeeping item) — plus the user's 2026-10-18 rulings:
  (i) dispatch every unblocked leftover/new issue in a single round,
  (ii) **all fixes land as ONE PR** — the ~10-minute Performance
  Benchmarking job runs on every PR, so N small PRs are no longer
  economical, (iii) milestone hygiene: #256 was verified implemented
  (PR #328, twin #296) and closed 2026-10-18 with evidence. Amended
  same-day with the **disk-hygiene protocol** (origin:
  `.artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md`;
  AGENTS §8/§10 amended in the same PR): doctor pre-flight before
  (re)creating the round's worktree, coverage-dir cleanup on every
  full-gate run, and chunk 5's stage-3 tearing the worktree down
  after the CLEAN handoff.
- **Status**: READY — all 5 chunks are dispatchable now, off the current
  `main` (62dd839), strictly serially on one shared branch. Nothing is
  gated on a human decision; the two open design micro-decisions are
  **settled in this document** (§2 D1/D2 for #314/#319, §4 D3 for #316 —
  stage-1 agents may deviate only with a recorded reason). **Deliberately
  not dispatched here**:
  - **#311** (zbus advisories) — the locally actionable remedy already
    landed with PR #328: `deny.toml` carries ignore-with-reason entries
    for RUSTSEC-2026-0370/0371/0372, and `cargo deny check` is green on
    `main`. The remaining zbus-6 refresh is upstream-gated: the 5.x line
    arrives transitively via slint → accesskit → atspi
    (`cargo tree -i zbus@5.19.0`), so no safe upgrade exists inside this
    repo. #311 stays open as the revisit anchor recorded in `deny.toml`.
  - **#327** (capped cargo-mutants one-off for taskboard-app) — its own
    body defers it: run the sweep fresh at the start of the future
    app-test-hardening chunk; running it now produces survivor evidence
    that ages unused (the F14-02 inversion).
  - Everything owned by other waves: the wave-multi-board design round
    (Stage-0 in flight), wave-kiosk (#262/#263/#299/#300 …), and the P2
    sync-correctness design round (#128/#129/#130/#150) that needs its
    own design round first.

## 1. Chunks & ordering

| Chunk | Origin (issues · findings) | Covers | Size |
|---|---|---|---|
| 1 | #314, #319 · F-series of their reports | sync push semantics: per-op classified outcomes in `update_task` label-delta arms; `delete_card` stale-binding re-resolve + ADR 0007 note | M |
| 2 | #242 · F6-04/F6-05/F6-06/F6-23/F6-24 | five wiremock/actor-suite tests completing the sync gap set; **test-only, no production changes** | M |
| 3 | #316 · state-storage-gaps D2 | make the engine's all-sources-closed exit reachable (WeakSender seam) + the #240 join-completes test | S/M |
| 4 | #313, #323 · watchlist §5, execute-batch §7 | storage test hygiene: pool disposal before temp drop (no `-shm`/`-wal` leaks) + reminder-partition roundtrip fixtures | S/M |
| 5 | #330 · arch-org-release §risks | `[Unreleased]` changelog backfill for everything since v0.1.0 **including chunks 1–4** + release-cut checklist step | S |

All five chunks share **one branch**: `fix/remediation-wave1-followups`.

**Dependency graph**: chunks 3, 4 touch crates disjoint from each other
and from 1/2; chunk 2 follows chunk 1 only because both live in
`taskboard-sync-nextcloud` (serialize to avoid same-crate conflicts);
chunk 5 runs **last by design** — it sweeps the commit range since
`v0.1.0`, which must include chunks 1–4's commits. Default order:
1 → 2 → 3 → 4 → 5, strictly serial.

**Dispatch mechanics — AMENDED for single-PR delivery** (user ruling
2026-10-18; supersedes this protocol's default one-branch-per-chunk for
this round only):

- ONE worktree `../taskboard-WT-fix-remediation-wave1-followups` is
  created by chunk 1's stage-1 agent on the single branch off the
  then-current `main`; every later stage of every chunk continues in
  that same worktree (recreate it on the branch if missing). Agents run
  strictly one at a time — serial discipline replaces worktree
  isolation; never two agents in one worktree.
- **Stage-3 shipping is amended per position**: chunks 1–4's stage-3
  reviews its chunk's increment only, rebases the branch onto
  `origin/main` when `main` diverged (fetch; rebase; resolve only real
  conflicts; no push — intermediate pushes re-trigger the ~10-min
  benchmark job for nothing), and runs the full AGENTS §10 gate suite
  locally. **No push, no PR.** Chunk 5's stage-3 performs the single
  shipping task for the whole round: push the branch once, open the ONE
  PR to `main`, `gh pr checks --watch` to green, `mergeStateStatus` must
  be `CLEAN` (the standard amended protocol), never merge.
- Commit hygiene is unchanged and load-bearing: every commit standalone
  (`<type>(<scope>): <subject>`), fixes amended in place — the PR is
  rebase-merged, so all commits land on `main` verbatim.
- **Disk hygiene (protocol amended 2026-10-18; AGENTS §8/§10 carry
  the rules)**: any agent that creates or recreates the round's
  worktree runs `scripts/worktree-doctor.sh` from the main checkout
  first (nonzero exit ⇒ free disk below the floor ⇒ STOP and report).
  Every full-gate run deletes `target/llvm-cov-target` after
  `lcov.info` is written (AGENTS §10's trailing step — 13–25G per
  run otherwise). Chunk 5's stage-3 removes the round's worktree as
  its final act, after the CLEAN handoff: from the project root,
  `git worktree remove --force ../taskboard-WT-fix-remediation-wave1-followups`
  — the round's single worktree must not outlive its PR (a future
  fix round recreates it on the branch).

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: continues in the shared worktree
   (chunk 1's stage-1 creates it), writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree,
   executes per the approved plan — TDD, every commit standalone
   (rebase-merge hygiene), full AGENTS §10 gate suite green. Does not
   push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `origin/main`
   (`git fetch origin --prune`, rebase the branch; resolve only real
   conflicts; keep local — chunks 1–4), then reviews the **chunk's
   increment** (`git diff <previous-chunk-tip>..HEAD` for chunks 2–5;
   `git diff main...HEAD` for chunk 1) against the chunk's acceptance
   criteria + AGENTS rules **plus the code-quality mandate below**,
   re-runs the full gate suite, implements fixes for every finding on
   the branch (amend/reword, never duplicate-subject fixups). Chunk 5's
   stage-3 additionally ships the round: push once, open the single PR,
   watch checks to green, verify `CLEAN`, hand off. Reports a verdict.
   Never merges; never touches `main`.

**Code-quality mandate** (stage 3, every chunk):

- **Implementation chunks — code-quality mandate**: beyond functional
  review, evaluate (a) code quality: naming, comment discipline
  (comments state constraints, not narration), idiomatic use of the
  approved stack, no dead code or speculative generality, test quality
  per AGENTS §5; (b) the Single Responsibility Principle: the
  crate-role boundaries of AGENTS §2 are the SRP line — a boundary
  violation is an SRP violation; (c) a single level of abstraction per
  function body: a function either orchestrates named operations or
  works in primitive data, never mixes. Findings are fixed on the
  branch like any other finding.

## 2. Chunk 1 — sync push semantics: #314 + #319

- **Covers** (normative: the two issue bodies + their origin artefacts):
  - **#314**: in `crates/taskboard-sync-nextcloud/src/push_exec/execute.rs`
    (the module-tree home of the former monolithic `push_exec.rs` — the
    2026-10-17 analysis was written against the pre-tree file; re-anchor
    its line references, its §Summary/§F1–F5 still hold), rework the
    two classified-rejection early returns of `PushExecutor::update_task`'s
    label-delta phase (`assignLabel`/`removeLabel` error arms) so
    subsumed ops are emitted per-op exactly like the post-#310 success
    tail (`execute.rs` ~line 687) instead of being completed with a
    synthetic `Applied { echo: None }`:
    - **D1 (settled)**: primary keeps `Applied { echo: Some(..) }` (the
      field PUT landed); subsumed update/move ops → synthetic `Applied`;
      unresolvable label ops → `Rejected { LocalData }`; label ops
      whose own sub-endpoint write was classified-rejected → the
      classified result per-op; transport failures keep `Aborted`.
      Op → label → remote-number mapping via `overlay.label_by_local`.
    - Tests (F5): 403/404 mounted on `assignLabel`/`removeLabel` inside
      an update group that has subsumed label ops, asserting typed
      per-op outcomes at the wiremock tier.
  - **#319**: `delete_card` (same file) deletes at the overlay's stale
    stack. Give it the same pre-write treatment as update/move after
    #258: one re-resolve against the fresh listings on a delete-path
    404 before classifying (mirror `fetch_card_for_write`), or an
    explicit pre-write fetch — whichever matches the #258 seam shape.
    - **D2 (settled)**: a bare 404 on delete after the re-resolve (the
      card is genuinely gone from every stack) stays
      `RemoteMissing`-converging — the tombstone is then correct.
      Document D2 as an ADR 0007 amendment note alongside the #258
      amendment (policy note, no ADR rewrite).
    - Tests: the #319 interleaving — remote move between local intent
      and push → delete 404s at the stale binding → re-resolve →
      deletes at the fresh binding, no twin re-adoption; plus the
      bare-404 convergence arm.
  - **Non-goals**: no changes to `apply_push_report`'s completion
    semantics, no pull-side changes, no outbox reordering, nothing
    outside `taskboard-sync-nextcloud` (+ the ADR note).
- **Acceptance**: the five #314 behaviours pinned by typed-outcome
  tests (flip the fix off — each test must fail); #319's two arms
  pinned; #310's success-tail tests and chunk-5's stale-binding
  wiremock tests (`tests/actor_wiremock.rs`) unregressed; first-Applied
  outbox semantics preserved for genuinely-applied ops; ADR 0007 note
  states D2 without rewriting the decision; gates green; diff confined
  to the sync crate + ADR file.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of remediation-followups
("sync push semantics: #314 + #319", branch fix/remediation-wave1-followups)
in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origins: issue #314 (gh issue view
   314) with .artifacts/reports/2026-10-17-update-label-rejection-arms-analysis-report.md
   (§Summary, §F1–F5, §Proposed fix shape), issue #319 (gh issue view
   319) with .artifacts/plans/2026-10-17-sync-stale-binding-clock-plan.md
   (§non-goals), and this dispatch plan §1/§2 — decisions D1 and D2 are
   settled; deviating needs a recorded reason.
2. Read the current seams: crates/taskboard-sync-nextcloud/src/push_exec/
   {execute.rs,overlay.rs,outcomes.rs} (the analysis predates the chunk-10
   module split — re-anchor every line reference), the post-#310 success
   tail (~execute.rs:687) whose per-op emission shape you are extending,
   the #258 re-resolve seam in the update/move paths, and the tests you
   must not break: tests/actor_wiremock.rs (stale-binding + pinning
   suites), src/push_exec/tests.rs.
3. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning). Then create the worktree ../taskboard-WT-fix-remediation-wave1-followups on a new branch fix/remediation-wave1-followups off main. (You are the first agent of the round; later chunks reuse this worktree.)
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-sync-push-semantics-plan.md:
   the exact per-op outcome emission for every subsumed-op class in the
   two classified-rejection arms (D1), the op→label→remote-number
   mapping via overlay.label_by_local, the delete_card re-resolve seam
   choice (mirror fetch_card_for_write vs pre-write fetch) and its
   bare-404 arm (D2), the wiremock test catalogue for both, and the
   ADR 0007 amendment note text. Resolve every open question with a
   recorded decision.
5. Commit it as: docs(plan): sync push semantics chunk plan (#314/#319).
   Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of
remediation-followups ("sync push semantics: #314 + #319").

1. Read AGENTS.md, the normative origins (issues #314/#319 + their
   artefacts, dispatch plan .artifacts/plans/2026-10-18-remediation-followups-dispatch-plan.md
   §1/§2), and the stage-1 plan (.artifacts/plans/*sync-push-semantics*-plan.md)
   in the worktree ../taskboard-WT-fix-remediation-wave1-followups
   (branch fix/remediation-wave1-followups). Recreate the worktree on
   that branch if missing.
2. Implement exactly per the approved plan, TDD: tests first for each
   behaviour (typed outcome assertions only — no log/label text; no
   fixed sleeps — poll-with-deadline; wiremock hermetic tier), then the
   production change. Every commit standalone (rebase-merge hygiene:
   fix(sync): ... subjects, fixups amended in place).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: the ADR 0007 amendment note (D2 policy, no rewrite).
   Do NOT touch CHANGELOG.md — chunk 5 owns the [Unreleased] entries
   for this round.
5. No push, no PR — shipping is chunk 5's stage-3 task. Never push to
   or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of remediation-followups
(branch fix/remediation-wave1-followups, shared by the whole round).

1. Read AGENTS.md, the normative origins (issues #314/#319 + their
   artefacts), the dispatch plan §1/§2 (the code-quality mandate applies
   to you), and the stage-1 plan; then review the full branch increment
   (git diff main...HEAD) in ../taskboard-WT-fix-remediation-wave1-followups.
2. Functional focus: each new test fails when the fix is mentally
   reverted (F5's untested-arms trap is the chunk's core debt — verify
   coverage of BOTH assignLabel and removeLabel arms with subsumed
   label ops, update and move subsumption, unresolvable labels →
   Rejected { LocalData }, classified sub-endpoint failures NOT
   completed as Applied); apply_push_report semantics unchanged for
   genuinely-applied ops; delete_card re-resolves exactly once (no
   retry storm) and bare-404 converges to RemoteMissing (D2); the
   #310 success-tail tests and the #258 stale-binding wiremock tests
   are unregressed; assertions are typed outcomes, never text; the ADR
   0007 note states policy without rewriting the decision.
3. Code-quality mandate: beyond functional review, evaluate (a) code
   quality: naming, comment discipline (comments state constraints, not
   narration), idiomatic use of the approved stack, no dead code or
   speculative generality, test quality per AGENTS §5; (b) the Single
   Responsibility Principle: the crate-role boundaries of AGENTS §2 are
   the SRP line — a boundary violation is an SRP violation; (c) a
   single level of abstraction per function body: a function either
   orchestrates named operations or works in primitive data, never
   mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Rebase hygiene only — NO push, NO PR (single-PR round; chunk 5's
   stage-3 ships): if origin/main diverged, git fetch origin --prune and
   rebase the branch onto origin/main, resolving only real conflicts.

Report: verdict, findings (functional + mandate), what you fixed,
confirmation the branch is gate-green and rebased. Never push; never
touch main.
```

## 3. Chunk 2 — sync actor/executor test gaps: #242

- **Covers** (normative: #242's body; test shapes from the chunk-6
  findings index of
  `.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md`):
  five tests completing the sync gap set —
  F6-04 pulled labels unasserted below tier 2; F6-05 board-switch cache
  bleed; F6-06 absorber/streak/bound-board arms (the streak test needs
  the domain `test-support` fail-reads fake flag — a cross-crate
  test-support change is in scope and its only allowed production-adjacent
  diff); F6-23 archived-listing dedupe + flag-stamp; F6-24 create-path
  transport/rejection discrimination (the re-POST hazard the `landed`
  field exists to prevent). All at the wiremock/actor-suite tier,
  `tests/actor_wiremock.rs` (+ `src/push_exec/tests.rs` where the
  findings index says unit-tier suffices).
  **Test-only chunk: no production behavior changes.** If a new test
  exposes a real production bug, follow AGENTS §5 — do not weaken the
  test and do not fix the bug inline; leave the test failing only if
  that is impossible to avoid, and open a new issue + record it in the
  report for dispatch (a #314-style follow-up).
- **Non-goals**: no changes to sync production code, no e2e/it_live/it_docker
  additions (hermetic wiremock tier only), no bench work.
- **Acceptance**: five new tests, one per finding, each pinning the
  exact behaviour the findings index names; gap analysis table in the
  stage-1 plan mapping every finding to its test; gates green; diff is
  tests + (if needed) the domain test-support flag only.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of remediation-followups
("sync actor/executor test gaps: #242") in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #242 (gh issue view 242), its findings index
   (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md,
   chunk-6 findings — the F6-04/05/06/23/24 entries are normative), and
   the dispatch plan §1/§3.
2. Read the seams the tests will mount: tests/actor_wiremock.rs (the
   chunk-2/5 suites you must not break), tests/common/hermetic.rs,
   src/push_exec/{execute.rs,tests.rs}, the pull/listing paths the
   findings name, and taskboard-domain's existing test-support surface
   (for the F6-06 fail-reads fake flag). NOTE: chunk 1 landed on this
   branch first — re-anchor to post-chunk-1 code where the push arms
   shifted.
3. Continue in the existing worktree ../taskboard-WT-fix-remediation-wave1-followups
   on branch fix/remediation-wave1-followups (create it on that branch
   off main only if missing).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-sync-gap-tests-plan.md:
   a gap table (finding → existing coverage → new test), the exact
   fixture/wiremock mounting per test, the domain test-support flag
   shape for F6-06, and the tier placement per docs/testing_strategy.org.
   Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): sync gap tests chunk plan (#242). Do not
   implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of
remediation-followups ("sync actor/executor test gaps: #242").

1. Read AGENTS.md, issue #242 + its findings index, the dispatch plan
   §1/§3, and the stage-1 plan (.artifacts/plans/*sync-gap-tests*-plan.md)
   in the worktree ../taskboard-WT-fix-remediation-wave1-followups
   (branch fix/remediation-wave1-followups). Recreate the worktree on
   that branch if missing.
2. Implement exactly per the approved plan: five tests (F6-04/05/06/23/24),
   hermetic wiremock tier only, typed outcome assertions (no text),
   deterministic by construction (poll-with-deadline, never fixed
   sleeps). Test-only: if a test exposes a production bug, do NOT fix
   or weaken — record it in your report for a new issue per AGENTS §5.
   Every commit standalone (test(sync): ... subjects).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: none — do NOT touch CHANGELOG.md (chunk 5 owns it).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, the gap-table outcome, any production-bug
discovery (with repro), any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of remediation-followups
(branch fix/remediation-wave1-followups, shared by the whole round).

1. Read AGENTS.md, issue #242 + its findings index, the dispatch plan
   §1/§3 (the code-quality mandate applies to you), and the stage-1
   plan; then review the chunk increment (git diff <chunk-1-tip>..HEAD;
   chunk-1-tip = the last commit before your stage-1 plan commit) in
   ../taskboard-WT-fix-remediation-wave1-followups.
2. Functional focus: each finding's test pins the exact behaviour the
   findings index names (not a weaker neighbour); F6-06's domain
   test-support change is test-support only (no production surface
   added to taskboard-domain); F6-24 actually discriminates transport
   vs rejection on the create path (the re-POST hazard — the `landed`
   field must be what the test trips); no production behavior changed
   anywhere in the diff; no test weakened or skipped; determinism
   (no wall-clock waits anywhere new).
3. Code-quality mandate: beyond functional review, evaluate (a) code
   quality: naming, comment discipline (comments state constraints, not
   narration), idiomatic use of the approved stack, no dead code or
   speculative generality, test quality per AGENTS §5; (b) the Single
   Responsibility Principle: the crate-role boundaries of AGENTS §2 are
   the SRP line — a boundary violation is an SRP violation; (c) a
   single level of abstraction per function body: a function either
   orchestrates named operations or works in primitive data, never
   mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Rebase hygiene only — NO push, NO PR: if origin/main diverged,
   git fetch origin --prune and rebase the branch onto origin/main,
   resolving only real conflicts.

Report: verdict, findings (functional + mandate), what you fixed,
confirmation the branch is gate-green and rebased. Never push; never
touch main.
```

## 4. Chunk 3 — engine exit reachability: #316

- **Covers** (normative: #316's body; state-storage-gaps plan decision
  D2 context): in `crates/taskboard-state/src/actor.rs`, the
  all-sources-closed exit (`!draining && !commands_open && !reports_open
  && !system_open`) is structurally unreachable because `run_loop`
  destructures and holds the `broadcast::Sender<SystemEvent>`
  (`system_out`, the `StorageFailed` emission seam) for the loop's
  lifetime.
  - **D3 (settled)**: remediation (a) — hold the emission seam as a
    `broadcast::WeakSender` (upgrade on send) so external-producer
    death closes the channel and the exit becomes reachable. Rationale:
    with zero external receivers, today's strong sender delivers
    nothing anyway (broadcast to no subscribers is a vacuous no-op) —
    it only suppresses `Closed`; the WeakSender changes no observable
    emission while the app runs (the runner always holds a sender
    clone), unlocks the audit-prescribed join-completes test from
    #240, and makes the three surviving `delete !` conjunct mutants
    killable. Rejected: (b) deleting the conjunct removes a real
    lifecycle guarantee; (c) a spawn-side close-watcher adds a task for
    no additional guarantee. Stage-1 verifies `broadcast::WeakSender`
    exists in the pinned tokio (1.53.2); if it does not, record the
    deviation and choose between (a′) restructuring `EngineParts` so
    `run_loop` borrows rather than owns the sender and (c), with the
    reasoning written down.
  - The **#240 join-completes test rides this chunk**: drop
    command/report handles + all external system sender clones → the
    engine task joins under a deadline (tokio time pause/advance, no
    wall-clock sleep).
  - **Non-goals**: no drain-path changes (biased select order
    preserved), no changes to `AppState`/snapshot semantics, no UI or
    app-runner behavior changes beyond what the seam swap forces
    (compile-level).
- **Acceptance**: the join-completes test passes and fails on
  pre-change code; a test asserting a live external subscriber still
  receives `StorageFailed` while running; engine insta snapshots
  unregressed; drain exit untouched; gates green; diff confined to
  taskboard-state (+ app-runner compile fixes if the seam type moves).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of remediation-followups
("engine exit reachability: #316") in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #316 (gh issue view 316) with
   .artifacts/plans/2026-10-17-state-storage-gaps-plan.md (decision D2
   context, #240's test prescription), and the dispatch plan §1/§4 —
   decision D3 (WeakSender, candidate (a)) is settled; deviating needs
   a recorded reason.
2. Read the seams: crates/taskboard-state/src/actor.rs (run_loop's
   EngineParts destructuring, the ~line-350 conjunct, the StorageFailed
   emission), the engine test suite (insta snapshots you must not
   churn), how taskboard-app spawns the engine and holds system
   senders, and tokio 1.53.2's broadcast::WeakSender surface (verify it
   exists; if not, record the deviation and pick (a′)/(c) with reasons).
3. Continue in the existing worktree ../taskboard-WT-fix-remediation-wave1-followups
   on branch fix/remediation-wave1-followups (create it on that branch
   off main only if missing).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-engine-exit-reachability-plan.md:
   the WeakSender seam shape, the upgrade-on-send failure semantics
   (= today's no-subscriber semantics), the #240 join-completes test
   design (pause/advance clock, deadline join), the still-receives-
   StorageFailed test, and the mutant killability argument for the
   three delete-! conjuncts. Resolve every open question with a
   recorded decision.
5. Commit it as: docs(plan): engine exit reachability chunk plan (#316).
   Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of
remediation-followups ("engine exit reachability: #316").

1. Read AGENTS.md, issue #316 + its artefacts, the dispatch plan §1/§4,
   and the stage-1 plan
   (.artifacts/plans/*engine-exit-reachability*-plan.md) in the worktree
   ../taskboard-WT-fix-remediation-wave1-followups (branch
   fix/remediation-wave1-followups). Recreate the worktree on that
   branch if missing.
2. Implement exactly per the approved plan, TDD: the join-completes
   test and the StorageFailed-still-received test first (both against
   current code — join-completes must fail), then the WeakSender seam.
   tokio time pause/advance only (no sleeps, no wall-clock waits);
   typed assertions. Every commit standalone (fix(state): ... subjects).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: none — do NOT touch CHANGELOG.md (chunk 5 owns it).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of remediation-followups
(branch fix/remediation-wave1-followups, shared by the whole round).

1. Read AGENTS.md, issue #316 + its artefacts, the dispatch plan §1/§4
   (the code-quality mandate applies to you), and the stage-1 plan;
   then review the chunk increment (git diff <chunk-2-tip>..HEAD) in
   ../taskboard-WT-fix-remediation-wave1-followups.
2. Functional focus: the join-completes test genuinely fails on
   pre-change code (verify by mentally reverting the seam swap — or
   check out the parent commit and run it); StorageFailed emission
   still reaches a live subscriber; the drain exit and biased select
   order untouched; no insta snapshot churn beyond what the seam forces
   (if snapshots changed, each change must be a real semantic delta);
   the upgrade-on-send path cannot panic or block the loop; the exit
   conjunct is now reachable, not deleted (D3 is (a), not (b)).
3. Code-quality mandate: beyond functional review, evaluate (a) code
   quality: naming, comment discipline (comments state constraints, not
   narration), idiomatic use of the approved stack, no dead code or
   speculative generality, test quality per AGENTS §5; (b) the Single
   Responsibility Principle: the crate-role boundaries of AGENTS §2 are
   the SRP line — a boundary violation is an SRP violation; (c) a
   single level of abstraction per function body: a function either
   orchestrates named operations or works in primitive data, never
   mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Rebase hygiene only — NO push, NO PR: if origin/main diverged,
   git fetch origin --prune and rebase the branch onto origin/main,
   resolving only real conflicts.

Report: verdict, findings (functional + mandate), what you fixed,
confirmation the branch is gate-green and rebased. Never push; never
touch main.
```

## 5. Chunk 4 — storage test hygiene + reminder roundtrips: #313 + #323

- **Covers** (normative: the two issue bodies):
  - **#313**: sqlite test pools/connections in
    `crates/taskboard-storage-sqlite/tests/` (the raw
    `SqlitePool::connect` uses in `tests/roundtrip.rs` ~lines 310/362/388
    and every pool-returning fixture) outlive-or-drop alongside
    `NamedTempFile`/`TempDir` handles without being closed, so sqlite's
    `-shm`/`-wal` sidecars survive cleanup (10,923 leaked files
    observed; watchlist report §5). Fix: dispose/close the pool (and
    raw connections) **before** the temp handle drops — an RAII fixture
    wrapper or explicit close at test end — plus one teardown-guard
    test asserting the fixture's own db path leaves no sidecar
    siblings. The guard must scope to the fixture's path, never scan
    global /tmp. The issue's optional fold-in (three leftover IPC
    daemon tempdirs): include only if they are test-owned paths in
    current code; prod daemon tmp handling is out of scope.
  - **#323**: `actions_from_state`
    (`crates/taskboard-storage-sqlite/tests/common/mod.rs`) emits only
    7 of the 14 `PersistenceAction` variants — extend it (or add a
    dedicated roundtrip fixture) so the reminder partitions
    (`UpsertReminderRule`, `DeleteReminderRule`,
    `RecordReminderDelivery`, `RecordReminderSnooze`,
    `SetTaskRemindersMuted`) round-trip through `apply` + `load` at the
    storage crate's own tier, mirroring
    `persisted_state_roundtrips_through_sqlite`.
  - **Non-goals**: no storage production changes, no schema changes, no
    changes to the app-tier `tests/cli_reminders.rs`.
- **Acceptance**: no new test leaves sidecar siblings (guard test
  passes; manually verified pattern applied to every fixture that
  creates a db); reminder roundtrip test covers all five reminder
  variants; the other seven variants' coverage unchanged; gates green;
  diff confined to taskboard-storage-sqlite tests (+ domain test
  fixtures if the reminder builders live there).

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of remediation-followups
("storage test hygiene + reminder roundtrips: #313 + #323") in the
taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #313 (gh issue view 313) with
   .artifacts/reports/2026-10-17-flaky-test-watchlist-investigation-report.md
   §5, issue #323 (gh issue view 323) with
   .artifacts/plans/2026-10-17-execute-batch-exhaustive-plan.md §7, and
   the dispatch plan §1/§5.
2. Read the seams: crates/taskboard-storage-sqlite/tests/ (roundtrip.rs
   raw connects ~310/362/388, common/mod.rs actions_from_state's 7
   variants, every fixture that creates a db), the PersistenceAction
   enum's 14 variants in taskboard-domain, and the reference roundtrip
   test persisted_state_roundtrips_through_sqlite. Decide the fixture
   wrapper shape (RAII guard closing the pool before the TempDir drop)
   and audit EVERY db-creating test site for the pattern.
3. Continue in the existing worktree
   ../taskboard-WT-fix-remediation-wave1-followups on branch
   fix/remediation-wave1-followups (create it on that branch off main
   only if missing).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-storage-test-hygiene-plan.md:
   the fixture wrapper design, the complete list of call sites to
   convert, the sidecar-guard test design (scoped to the fixture path),
   the reminder roundtrip fixture shape (extend actions_from_state vs
   dedicated fixture — pick one, record why), and the IPC-tempdir
   fold-in verdict. Resolve every open question with a recorded
   decision.
5. Commit it as: docs(plan): storage test hygiene chunk plan (#313/#323).
   Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of
remediation-followups ("storage test hygiene + reminder roundtrips:
#313 + #323").

1. Read AGENTS.md, the issues #313/#323 + their artefacts, the dispatch
   plan §1/§5, and the stage-1 plan
   (.artifacts/plans/*storage-test-hygiene*-plan.md) in the worktree
   ../taskboard-WT-fix-remediation-wave1-followups (branch
   fix/remediation-wave1-followups). Recreate the worktree on that
   branch if missing.
2. Implement exactly per the approved plan, TDD: the sidecar-guard test
   first (it must fail against at least one unconverted fixture), then
   the wrapper conversion; the reminder roundtrip test; typed
   assertions; no sleeps. Test-only diff. Every commit standalone
   (test(storage): ... subjects).
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Bookkeeping: none — do NOT touch CHANGELOG.md (chunk 5 owns it).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of remediation-followups
(branch fix/remediation-wave1-followups, shared by the whole round).

1. Read AGENTS.md, the issues #313/#323 + their artefacts, the dispatch
   plan §1/§5 (the code-quality mandate applies to you), and the
   stage-1 plan; then review the chunk increment (git diff
   <chunk-3-tip>..HEAD) in ../taskboard-WT-fix-remediation-wave1-followups.
2. Functional focus: the guard test is scoped to the fixture's own path
   (no global /tmp scan — that would be flaky by construction); EVERY
   db-creating fixture is converted (grep for SqlitePool::connect /
   SqlitePoolOptions / connect( in the crate's tests — no stragglers);
   pools genuinely close BEFORE the temp drop (RAII order or explicit
   close — verify the drop order argument); the reminder roundtrip
   covers all five reminder variants and asserts loaded state equals
   applied state structurally; no production code changed; no test
   weakened.
3. Code-quality mandate: beyond functional review, evaluate (a) code
   quality: naming, comment discipline (comments state constraints, not
   narration), idiomatic use of the approved stack, no dead code or
   speculative generality, test quality per AGENTS §5; (b) the Single
   Responsibility Principle: the crate-role boundaries of AGENTS §2 are
   the SRP line — a boundary violation is an SRP violation; (c) a
   single level of abstraction per function body: a function either
   orchestrates named operations or works in primitive data, never
   mixes. Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Rebase hygiene only — NO push, NO PR: if origin/main diverged,
   git fetch origin --prune and rebase the branch onto origin/main,
   resolving only real conflicts.

Report: verdict, findings (functional + mandate), what you fixed,
confirmation the branch is gate-green and rebased. Never push; never
touch main.
```

## 6. Chunk 5 — changelog backfill + release-cut guard: #330

- **Covers** (normative: #330's body; arch-org-release plan §risks
  bookkeeping item): one pass over the commit range since `v0.1.0`
  adding the missing `[Unreleased]` entries in `CHANGELOG.md`
  (Keep-a-Changelog format, issue-number refs, matching the existing
  board-verbs entry style). Known missing (verified 2026-10-18): the
  stale-binding re-resolve (#258's chunk), userinfo reject/strip +
  cleartext warning (#260/#261), the typed daemon-shutdown tail
  (#257), the subsumed-label fix (#310), release overflow-checks
  (#303) — **plus every behavior fix this round's chunks 1–4 land on
  the branch below this chunk** (which is why this chunk runs last).
  Sweep the whole `v0.1.0..HEAD` range so nothing else is missed (e.g.
  the #312 flaky-test remediations if any are user-visible). Then add
  the release-cut checklist step: a grep of the release commit range
  against the changelog section, landing where chunk-14's release
  docs/live (locate the file; PR #332 introduced the release docs).
- **Non-goals**: no rewriting of historical released sections, no code
  changes, no version bump.
- **Acceptance**: every behavior-relevant commit in `v0.1.0..HEAD` is
  either represented in `[Unreleased]` or explicitly judged
  non-user-visible in the stage-1 plan's sweep table; the checklist
  step exists in the release docs; `bump_version.sh`'s promotion step
  will pick the entries up verbatim; gates green (fmt/clippy/doc — no
  code paths); diff is CHANGELOG.md + the one release-doc file.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of remediation-followups
("changelog backfill + release-cut guard: #330") in the taskboard repo
at /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #330 (gh issue view 330) with
   .artifacts/plans/2026-10-18-arch-org-release-plan.md §risks, and the
   dispatch plan §1/§6. Read CHANGELOG.md's [Unreleased] section and
   the release docs chunk 14 landed (PR #332) to find where the
   release-cut checklist lives.
2. Build the sweep table: git log v0.1.0..HEAD (on this branch — it
   includes chunks 1–4) mapped commit-by-commit to "entry added /
   already present / not user-visible (why)". Include this round's
   chunks 1–4 fixes.
3. Continue in the existing worktree
   ../taskboard-WT-fix-remediation-wave1-followups on branch
   fix/remediation-wave1-followups (create it on that branch off main
   only if missing).
4. Write the chunk plan at .artifacts/plans/<today>-changelog-backfill-plan.md:
   the sweep table (normative output), the exact entries to add
   (section, wording, issue refs, matching the board-verbs entry
   style), and the checklist-step text + its destination file. Resolve
   every open question with a recorded decision.
5. Commit it as: docs(plan): changelog backfill chunk plan (#330). Do
   not implement; do not push.

Report: plan path, sweep-table summary, decisions, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of
remediation-followups ("changelog backfill + release-cut guard: #330").

1. Read AGENTS.md, issue #330 + its artefacts, the dispatch plan §1/§6,
   and the stage-1 plan (.artifacts/plans/*changelog-backfill*-plan.md)
   in the worktree ../taskboard-WT-fix-remediation-wave1-followups
   (branch fix/remediation-wave1-followups). Recreate the worktree on
   that branch if missing.
2. Implement exactly per the approved plan: the [Unreleased] entries
   per the sweep table (Keep-a-Changelog sections, issue refs, house
   entry style) and the release-cut checklist step in the located
   release doc. Docs-only diff: CHANGELOG.md + the one release-doc
   file. Commit standalone (docs(changelog): ... subject).
3. Run the gate suite from AGENTS.md §10 (docs-only, but fmt/clippy/
   nextest still verify nothing else drifted) and make it green.
4. No push, no PR — your stage-3 ships the round's single PR.
   Never push to or merge main.

Report: per-gate results, entries added, any deviation and why.
```

**Prompt 5.3 (review + SHIP the round's single PR):**

```text
You are the stage-3 REVIEW agent for chunk 5 of remediation-followups
(branch fix/remediation-wave1-followups) — and the round's SHIPPING
agent: yours is the only push of the round.

1. Read AGENTS.md, issue #330 + its artefacts, the dispatch plan §1/§6
   (the code-quality mandate applies to you), and the stage-1 plan;
   then review the chunk increment (git diff <chunk-4-tip>..HEAD) AND
   spot-check the round's full diff (git diff origin/main...HEAD) for
   cross-chunk coherence (no stray files, no leftover scratch, the
   chunks' commits all standalone).
2. Functional focus: the sweep table is complete for v0.1.0..HEAD
   (re-run git log yourself; every behavior commit entry-added or
   accounted); entries match the house style with issue refs; the
   checklist step lands in the file the release flow actually reads;
   no historical released section rewritten.
3. Code-quality mandate: beyond functional review, evaluate (a) code
   quality: naming, comment discipline (comments state constraints,
   not narration), idiomatic use of the approved stack, no dead code
   or speculative generality, test quality per AGENTS §5; (b) the
   Single Responsibility Principle: the crate-role boundaries of
   AGENTS §2 are the SRP line — a boundary violation is an SRP
   violation; (c) a single level of abstraction per function body: a
   function either orchestrates named operations or works in primitive
   data, never mixes. Findings are fixed on the branch like any other
   finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding. Fix everything on the branch (amend/reword — no
   duplicate-subject fixups).
5. Ship the round's SINGLE PR: git fetch origin --prune; if origin/main
   diverged, rebase the branch onto origin/main (resolve only real
   conflicts), then push ONCE with --force-with-lease if the rebase
   rewrote history. Open ONE PR to main titled "fix: remediation-wave-1
   follow-ups — sync push semantics, engine exit, test hygiene, changelog"
   with a body listing the five chunks and carrying: Closes #242,
   Closes #313, Closes #314, Closes #316, Closes #319, Closes #323,
   Closes #330. Then gh pr checks --watch to completion — any failing
   check is a finding: diagnose, fix, push --force-with-lease, repeat
   until green. Finally verify gh pr view --json mergeStateStatus is
   CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push
   --force-with-lease, let checks re-run, repeat. main is protected:
   never merge; the user merges on GitHub.
6. Round teardown — your final act: cd /home/viktoria/src/taskboard and
   git worktree remove --force ../taskboard-WT-fix-remediation-wave1-followups
   (everything is pushed; a future fix round recreates the worktree on
   the branch).

Report: verdict, findings (functional + mandate), what you fixed, PR
URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
Never merge; never touch main.
```

## 7. Completion bookkeeping

- **Issue closure**: the single PR's body carries `Closes #242`,
  `Closes #313`, `Closes #314`, `Closes #316`, `Closes #319`,
  `Closes #323`, `Closes #330` — on merge GitHub closes all seven.
- **Milestone hygiene**: closing #242 empties `remediation-wave-1`
  (39/39) — close the milestone on GitHub then (issue → milestone →
  close; or `gh api -X PATCH repos/D4id4los/taskboard/milestones/2
  -f state=closed`). #256 was verified implemented (PR #328, twin
  #296) and closed 2026-10-18 by the dispatcher with evidence; it was
  never broken, just never referenced.
- **If the user wants this round tracked as a unit**: create milestone
  `remediation-followups` and assign the seven issues before the PR
  merges (optional; the `Closes` lines work without it).
- **Deliberately not dispatched here** (do not re-derive): #311 —
  upstream-gated zbus-6 refresh (slint → accesskit → atspi); local
  remedy (ignore-with-reason in `deny.toml`) already landed with
  PR #328, `cargo deny check` green on main; the issue stays open as
  the revisit anchor named in `deny.toml`. #327 — the capped
  cargo-mutants one-off for taskboard-app, owned by the future
  app-test-hardening chunk per its own body. All wave-kiosk and
  wave-multi-board scope, and the P2 sync-correctness design round
  (#128/#129/#130/#150), belong to their waves/rounds.
- **What opens next**: nothing forced by this round. If chunk 2's
  tests expose a production bug (the #242 gap-set is the likeliest
  source), it gets a new issue + analysis in that chunk's report and a
  follow-up chunk like #314 was for #310. The multi-board wave
  proceeds independently; if its chunks merge while this branch is
  open, the stage-3 rebase discipline above handles it (AGENTS §8's
  rebase dance — this branch is NOT stacked on anything, so only real
  conflicts matter).
