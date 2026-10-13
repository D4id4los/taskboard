# [Plan] CI `Code Coverage` Job — Flake Investigation & Fix (Dispatch Plan)

- **Date**: 2026-10-13
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the investigation into and fix of the
  flaky CI `Code Coverage` job. No code in this commit.
- **Origin**: flaky-test watchlist (backlog, entries of 2026-10-11 and
  2026-10-13 — four occurrences of the instrumented-llvm-cov flake
  family, three of them CI-side with an identical signature); user
  directive of 2026-10-13 ("4th time … investigation into and fix").
  Escalation follows the watchlist policy (dedicated pass instead of
  opportunistic fixes).
- **Status**: READY — dispatch Prompt F1.

## 1. Evidence (verified 2026-10-13, do not re-derive)

| # | When | Where | Run | Symptom |
|---|---|---|---|---|
| 1 | 2026-10-11 | local, `cargo llvm-cov --workspace`, branch `feat/app-cli-e2e` @ `b22721f` | — | `actor_wiremock` binary fails *as tests* (exit 101); passes in isolation and on re-run |
| 2 | 2026-10-12 16:09Z | CI on `main` | `38275173071` | all 612 tests pass; merge dies: `taskboard-4660-…_0.profraw: invalid instrumentation profile data (file header is corrupt)` |
| 3 | 2026-10-12 16:12Z | CI on PR #47 (`docs/reminders-design`) | `38275544869` (first attempt) | all 612 tests pass; same merge failure on `taskboard-4681-…_1.profraw`; job re-run green (2m16s) |
| 4 | 2026-10-13 12:44Z | CI on `docs/connect-flow-design` | `38399239683` | all 624 tests pass; same merge failure on `taskboard-4778-…_2.profraw` |

Shared signature of #2–#4 (byte-identical failure shape):

```
warning: …/target/llvm-cov-target/taskboard-<pid>-<run-id>_<n>.profraw:
         invalid instrumentation profile data (file header is corrupt)
error: no profile can be merged
error: failed to merge profile data: … llvm-profdata merge -sparse -f
       …/taskboard-profraw-list -o …/taskboard.profdata (exit status: 1)
```

Facts that constrain the cause:

- Exactly **one** profraw is corrupt per failure; a different one
  each time (`_0`, `_1`, `_2`); never more than one; never the same
  file twice; the test suite itself is fully green every time.
- The profraw names are PID-stamped (`taskboard-<pid>-<run-id>_N`),
  i.e. cargo-llvm-cov's per-process naming is in play.
- The job runs the workspace under `cargo llvm-cov … nextest` (see
  `.github/workflows/ci.yml`, the coverage job) into the
  `llvm-cov-target` instrumented target dir.
- Occurrence #2 on `main` postdates the landing of the pty-based
  REPL tests (`c1db45e`, 2026-10-12) by roughly two hours; #3 and #4
  run on branches containing them. Occurrence #1 (local, 2026-10-11)
  predates them and failed *as a test failure*, not a merge failure —
  the two symptom families may or may not share a root cause; the
  planner decides whether to treat them jointly.
- **Working hypothesis (unverified, for the planner to attack):**
  tests that spawn child processes (taskboard-app's pty REPL journey
  tests, the IPC daemon tests spawning `taskboard serve` + CLI
  clients, e2e subprocess tests) inherit the parent's
  `LLVM_PROFILE_FILE`, so a long-lived or overlapping child writes
  the same profraw the parent owns — interleaved/truncated writes
  corrupt the file header, and the corruption surfaces later at
  merge. Fits the one-file-per-run, PID-stamped, different-file-each-
  time pattern and the post-pty-tests timing.

## 2. Scope, hard rules, non-goals

**In scope**: root-cause the flake; fix it at the CI/infrastructure
level (`.github/workflows/ci.yml`, test-spawning plumbing if that is
where the collision lives, `Cargo.toml`/`nextest`/`llvm-cov`
configuration, toolchain/crate version bumps); document the finding
in the watchlist entry and, if the root cause implicates test
plumbing, in `docs/testing_strategy.org`.

**Hard rule (non-negotiable, mirrors WI-C)**: every change is an
environment/infrastructure fix — **no test is deleted, weakened, or
unignored to make the job green**, and the coverage gate itself is
never downgraded (no `fail_ci_if_error: false` as "fix", no
swallowing the merge error, no dropping the instrumented run). A
bounded, *logged* retry of the merge step is acceptable **only** as
part of a fix whose root cause is understood and otherwise addressed
— never as the whole fix.

**Non-goals**: changing what the job measures (same test set, same
coverage computation); perf/benchmark jobs; the weekly mutants/Miri
jobs (unless the same plumbing fix provably applies); local-only
gate flakiness beyond occurrence #1 (the planner may fold it in if
the root cause explains it).

**Acceptance**: the root cause is identified and stated in the plan +
final ADR/notes; the fix addresses the mechanism (not the symptom);
CI evidence: at least **five consecutive green `Code Coverage` runs**
on the fix branch driven through `workflow_dispatch` (a floor for
confidence, not a ceiling on quality); watchlist entry updated with
the verdict; docs amended where the finding lands; backlog entry ✅
DONE.

## 3. Waves & conflicts

Single item; dispatch immediately (no gating). Branch
`ci/coverage-flake-fix` touches `.github/workflows/ci.yml` and
possibly `crates/taskboard-app` test plumbing — serial dispatch
relative to other CI-touching work avoids trivial rebases.

## 4. Prompts

**Prompt F1 (planning):**

```text
You are the stage-1 PLANNING agent for "CI Code Coverage job: flake investigation & fix" in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the flaky-watchlist entries of 2026-10-11 and 2026-10-13 in .artifacts/taskboard-potential-improvements-backlog.md, and the dispatch plan .artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md (§1 evidence and the hard rule are normative).
2. Read .github/workflows/ci.yml (the coverage job), docs/testing_strategy.org (§10 Miri/scope conventions), and the test-spawning surfaces named in the hypothesis: crates/taskboard-app pty REPL tests, IPC daemon tests, and any test that spawns a child process (grep for Command::new / std::process / spawn). Check cargo-llvm-cov and nextest versions (Cargo.lock) and their known issues around LLVM_PROFILE_FILE and profraw corruption.
3. Create the worktree ../taskboard-WT-ci-coverage-flake-fix on a new branch ci/coverage-flake-fix off main.
4. Write the investigation-and-fix plan at .artifacts/plans/<today>-coverage-ci-flake-fix-plan.md in the house [Plan] style: (a) the root-cause analysis method (local reproduction recipe: run the coverage build repeatedly with the suspect test subsets in isolation vs together, capture which profraw corrupts and which process writes it — LLVM_PROFILE_FILE tracing / strace-level evidence acceptable); (b) the hypothesis ladder to test (child-process LLVM_PROFILE_FILE inheritance → concurrent same-file writers; nextest/llvm-cov version interactions; anything else the evidence suggests); (c) the candidate fix ladder once the mechanism is confirmed (per-process unique profraw for spawned children / env sanitization in test plumbing; %m continuous-merge naming; toolchain or crate upgrade; last resort a logged bounded merge retry — only as a component of a confirmed-mechanism fix, per the hard rule); (d) how the fix is validated via workflow_dispatch from the branch (≥5 consecutive green coverage runs); (e) what gets documented (watchlist verdict, testing_strategy note if test plumbing changed).
5. Commit it as: docs(plan): coverage CI flake investigation and fix. Do not implement; do not push.

Report: plan path, the hypothesis ladder, the reproduction recipe's first step, risks.
```

**Prompt F2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for "CI Code Coverage job: flake investigation & fix".

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md (§1 evidence, §2 hard rule — no test weakened, no gate downgraded), and the stage-1 item plan (.artifacts/plans/*coverage-ci-flake-fix*-plan.md) in the worktree ../taskboard-WT-ci-coverage-flake-fix (branch ci/coverage-flake-fix). Recreate the worktree on that branch if missing.
2. Investigate per the plan's method until the mechanism is confirmed or a hypothesis is eliminated with evidence; then implement the approved fix ladder. Every commit standalone (rebase-merge hygiene).
3. Validate: drive workflow_dispatch runs from the branch (gh run watch) until ≥5 consecutive green Code Coverage runs on the final config. Any failure in those runs is evidence — diagnose before touching anything.
4. Pushing the branch is authorized for this item to feed workflow_dispatch iteration (the WI-C-style exception) — never open a PR; the PR and CI watch are stage 3's last task. Never push to or merge main.
5. Final commit: update the 2026-10-13 watchlist entry with the confirmed root cause and fix (verdict), apply the docs amendments the plan names (testing_strategy if test plumbing changed), and mark the watchlist escalation note resolved.

Report: confirmed root cause (with the evidence that confirms it), the fix, the 5-green-runs run URLs, deviations.
```

**Prompt F3 (review):**

```text
You are the stage-3 REVIEW agent for "CI Code Coverage job: flake investigation & fix" (branch ci/coverage-flake-fix).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-13-coverage-ci-flake-fix-plan.md (§1 evidence, §2 hard rule, acceptance), and the stage-1 item plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-ci-coverage-flake-fix.
2. Focus: the hard rule held — diff every touched test and workflow: no test deleted/weakened/unignored, fail_ci_if_error untouched (still true), the coverage job still measures the same test set; the fix addresses the confirmed mechanism, not a symptom (a bare retry without a root cause is a rejection); the ≥5 consecutive green runs stage 2 linked used the final config; watchlist + docs amendments match what shipped; workflow YAML sanity.
3. Independently verify the fix's stability: drive at least one fresh workflow_dispatch coverage run yourself and confirm green.
4. Re-run the local gate suite (AGENTS §10) for the touched surfaces; any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups); if the fix changed during review, re-establish the 5-green-runs evidence.
6. Last task — ship: push the branch (--force-with-lease if updating the stage-2 push), open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green.

Report: verdict, confirmed root cause, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 5. Completion bookkeeping

- The 2026-10-13 watchlist entry gets the verdict in chunk F2's final
  commit; the 2026-10-11 entry gets a cross-reference only if the
  investigation proves (or disproves) a shared root cause.
- When the fix PR merges, the user may prune the watchlist entries
  per the archive policy.
- PR #47 (this branch) carries this dispatch plan as a docs commit so
  the prompts are on `main` once merged — the fix branch itself is
  dispatched separately off `main`.
