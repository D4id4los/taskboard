# Benchmark lane off PRs + Bencher fix + bench build cost — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-21
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 1 chunk of "move the Performance
  Benchmarking lane off PRs, fix the dead Bencher upload, and cut the
  bench-profile build cost". No code in this commit.
- **Origin**: `.artifacts/reports/2026-10-21-benchmark-ci-cost-analysis-report.md`
  (measured situation §1, variants §2, recommendation §3 — variant A +
  the `BENCHER_API_KEY` fix + variant C1/C2), plus the user's 2026-10-21
  decision: "A + BENCHER_API_KEY fix + C now, single chunk". The user's
  premise — no Bencher baseline discontinuity — is verified fact: the
  original Bencher step (`1690df2`) shipped with `continue-on-error:
  true` and no CLI install; the pinned-CLI step (`33a9350`, 2026-10-18)
  has failed on every main push since. **No upload to Bencher has ever
  succeeded.**
- **Status**: READY — one chunk, dispatchable now. Deliberately **not**
  dispatched here: variant B (per-PR Bencher regression gate — future
  need, report §2-B), threshold/alert configuration, sccache rollout to
  the other `ci.yml` jobs, a compile-only bench smoke on PRs, variants
  D/E. These need their own round if ever wanted.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | Report §2-A, §1.4 (env fix), §2-C1/C2 | `ci/benchmark-lane-off-pr` | New `.github/workflows/benchmark.yml` (push-main + weekly + dispatch), delete the `benchmark:` job from `ci.yml` (+ its dangling comment), `BENCHER_API_KEY` env fix, `[profile.bench]` override in root `Cargo.toml`, sccache in the new workflow, docs touch-ups | M |

**Dependency graph**: single chunk, nothing gated. **Dispatch
mechanics**: one agent session per prompt, in order (1.1 → 1.2 → 1.3);
the chunk works in `../taskboard-WT-ci/benchmark-lane-off-pr` next to
the project root (AGENTS §8), branched off the current `main`. Disk
hygiene (AGENTS §8 worktree lifecycle): the stage-1 agent runs
`scripts/worktree-doctor.sh` from the main checkout before creating the
worktree — nonzero exit (free disk below the floor) stops the chunk;
stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for the chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-ci/benchmark-lane-off-pr` on a new branch off the
   current `main`, writes the TDD-first plan at
   `.artifacts/plans/2026-10-21-benchmark-lane-plan.md` in the house
   `[Plan]` style, resolving every open question with a recorded
   decision, commits it (`docs(plan): …`). No implementation, no push.
   Origin sections marked normative are binding; deviations need a
   recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the approved
   plan — every commit standalone (rebase-merge hygiene), full AGENTS
   §10 gate suite green. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (AGENTS §8's rebase dance: drop
   already-merged duplicate commits; resolve only real conflicts), push
   `--force-with-lease` if the rebase rewrote history — then reviews
   the entire branch diff versus the **post-rebase** base against the
   chunk's acceptance criteria + AGENTS rules **plus the code-quality
   mandate below**, re-runs whatever gates apply, implements fixes for
   every finding on the branch (amend/reword, never duplicate-subject
   fixups). Its last tasks are shipping: push the branch, open the PR
   to `main` with `gh`, watch `gh pr checks --watch` to completion —
   any failing check is a finding; diagnose, fix, push
   `--force-with-lease`, repeat until green — **then verify the PR's
   merge state**: `gh pr view --json mergeStateStatus` must report
   `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, repeat; any other state
   ⇒ diagnose before handing off. **Then teardown**: remove the
   now-disposable worktree — from the project root, `git worktree
   remove --force ../taskboard-WT-ci/benchmark-lane-off-pr` (everything
   precious is pushed; a later round that resumes the chunk recreates
   it on the branch). The handoff contract is "PR open + checks green +
   merge state `CLEAN` + worktree removed" so the user's merge needs no
   additional rebase and the chunk leaves no 36–55G of build output
   behind. Reports a verdict. Never merges; never touches `main`.
   (`main` is branch-protected — merges happen on GitHub by the user,
   after the required checks pass.)

**Code-quality mandate** (stage 3; workflow/Cargo-manifest diff, so
applied to what applies): beyond functional review, evaluate (a) the
diff shape — only the scoped files, no drive-by edits, comments state
constraints not narration; (b) workflow hygiene — deliberately pinned
action/CLI versions with a recorded reason per house pin policy
(`cargo-deny@0.20.2` style), steps guarded by conditions that are
exactly right for each trigger event; (c) manifest hygiene — the
`[profile.bench]` section must be additive and must not perturb the
`dev`/`test`/`release` profiles. Findings are fixed on the branch like
any other finding.

## 2. Chunk 1 — benchmark lane off PRs (`ci/benchmark-lane-off-pr`)

- **Covers** (origin sections normative — report §2-A, §1.4, §2-C1,
  §2-C2):
  1. **New `.github/workflows/benchmark.yml`**, house style of
     `.github/workflows/nextcloud-integration.yml` (schedule +
     `workflow_dispatch`) and `miri.yml`. Triggers: `push` to `main`
     only, `schedule` weekly cron **Thursdays 04:00 UTC** (the free
     slot: miri Mon 03:00, mutation Mon 04:00, nextcloud Wed 04:00),
     `workflow_dispatch`. Concurrency group
     `benchmark-${{ github.ref }}`, `cancel-in-progress: true`. Same
     job content as today's lane: ubuntu-latest, checkout, the libdbus
     + Mesa/fontconfig apt set, stable toolchain,
     `Swatinem/rust-cache@v2`, pinned bencher CLI (`bencherdev/bencher`
     + CLI `0.6.13`, keep the existing pin and its rationale comment),
     the `bencher --version` proof step. Bench execution runs **exactly
     once per event**: on `github.ref == 'refs/heads/main'` inside
     `bencher run --project taskboard --branch main --testbed
     ubuntu-latest --adapter rust_criterion "cargo bench --workspace"`;
     on any other ref (dispatch from a branch) as plain `cargo bench
     --workspace` with **no upload** — the guard is on the **ref**, not
     the event name, so a manual dispatch on a feature branch can never
     write to Bencher branch `main`. The Bencher step's env is
     **`BENCHER_API_KEY: ${{ secrets.BENCHER_API_TOKEN }}`** — the
     existing secret name, the new env name the 0.6.13 CLI requires
     (report §1.4); keep the "failures are loud — never re-add
     continue-on-error" posture and its comment, updated to name the
     new env var.
  2. **sccache in the new workflow only** (report §2-C2): install via
     `mozilla-actions/sccache-action` **pinned to a concrete version**
     (resolve the current one in stage 1 and record it + date in a
     comment, house pin policy), set `SCCACHE_GHA_ENABLED: "on"` and
     `RUSTC_WRAPPER: sccache` at job level.
  3. **Delete the `benchmark:` job from `.github/workflows/ci.yml`**
     and fix the dangling reference: the `Run Tests` step's comment
     (ci.yml ~line 180, "bench targets are the benchmark job's lane
     (every PR)") must be reworded to the new reality. No other
     `ci.yml` changes.
  4. **Root `Cargo.toml` `[profile.bench]` override** (report §2-C1):
     `lto = false`, `codegen-units = 16` — undoing the cascade of the
     shipped `[profile.release]` (`lto = "thin"`, `codegen-units = 1`)
     into bench builds via Cargo's `[profile.bench] inherits =
     "release"`. Comment states the constraint: the release profile
     serves the kiosk runtime binary; the bench profile must not pay
     thin-LTO/1-CGU build cost, and since no Bencher upload has ever
     succeeded (verified — origin header) there is no trend baseline to
     break. The section must not touch any other profile key.
  5. **Docs touch-ups**: `docs/testing_strategy.org` ~line 225 names
     `BENCHER_API_TOKEN` among Actions secrets — the secret name is
     unchanged but the text must not contradict the new env name; scan
     the surrounding section (and any other doc statement about when
     benches run) and amend statements of "every PR" cadence to
     main-push + weekly. Follow the CHANGELOG convention observed in
     recent `ci:` commits (stage-1 decision, recorded).
- **Explicit non-goals** (boundary not re-derived): no benches on PRs in
  any form (no compile-only smoke); no Bencher thresholds, alerts,
  quick-mode flags, or PR-branch flow (variant B is future work); no
  sccache on the other `ci.yml` jobs; no bench source or criterion
  config changes; no changes to `--testbed` or `--branch main`; no
  branch-protection changes inside the repo (the required-check removal
  is the user's GitHub-side action, surfaced in the PR body — see
  Acceptance).
- **Acceptance** (stage 3 checklist):
  - Diff is exactly: new `benchmark.yml`, `ci.yml` benchmark-job
    deletion + comment fix, root `Cargo.toml` `[profile.bench]`
    section, doc/CHANGELOG touch-ups, the stage-1 plan file. Nothing
    else.
  - AGENTS §10 gates green on the branch (the `[profile.bench]` change
    must leave fmt/clippy/nextest/doc/deny untouched in outcome).
  - **Live evidence**: after the workflow file exists on the branch, a
    `gh workflow run benchmark.yml --ref <branch>` + `gh run watch`
    dispatch run executes the full bench suite, **skips the upload
    step**, and its duration is recorded in the PR body as evidence
    for the report's ≤16→~10-minute expectation (exact number is
    evidence, not a gate — no quality ceiling; the gate is that the
    lane runs green end-to-end and no longer runs on PRs).
  - The PR body states loudly, for the user to do at merge time:
    (1) remove "Performance Benchmarking" from the PR **required
    checks** in branch protection — the check no longer exists on PRs,
    and leaving it required blocks all future PRs; (2) after merging,
    the next push to `main` (or a manual dispatch on `main`) must show
    the `Track Benchmarks with Bencher` step green — the first-ever
    successful upload, verifiable in the Bencher UI.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the benchmark-lane dispatch ("benchmark lane off PRs + Bencher fix + bench build cost", branch ci/benchmark-lane-off-pr) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the origin analysis .artifacts/reports/2026-10-21-benchmark-ci-cost-analysis-report.md (§1 measured situation, §2 variants A/C1/C2, §3 recommendation) — its variant-A and C sections are NORMATIVE — and the dispatch plan .artifacts/plans/2026-10-21-benchmark-lane-dispatch-plan.md §1/§2.
2. Read the current seams you will change: .github/workflows/ci.yml (the whole `benchmark:` job, plus the `Run Tests` step comment that references it), .github/workflows/nextcloud-integration.yml and miri.yml (house style for schedule+dispatch workflows and their concurrency groups), root Cargo.toml ([profile.release], [profile.dev] — the new [profile.bench] sits beside them and must not perturb them), docs/testing_strategy.org around line 225 (BENCHER_API_TOKEN mention and any bench-cadence statements).
3. Verify the CLI facts you plan around: the failure mode is `invalid value '***' for '--token'` — the 0.6.13 CLI requires --key/BENCHER_API_KEY (origin report §1.4). Resolve the current mozilla-actions/sccache-action version to pin and the correct env vars (SCCACHE_GHA_ENABLED, RUSTC_WRAPPER) from its README; record both in the plan.
4. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree ../taskboard-WT-ci/benchmark-lane-off-pr on a new branch ci/benchmark-lane-off-pr off main.
5. Write the plan at .artifacts/plans/2026-10-21-benchmark-lane-plan.md. It must settle, with a recorded decision each: exact workflow structure and step ordering (benches run exactly once per event; upload guard on github.ref == 'refs/heads/main', NOT on event name — a dispatch from a feature branch must never write to Bencher branch main); the Thursday 04:00 UTC cron; the sccache pin and env placement (job-level); the [profile.bench] keys (lto = false, codegen-units = 16) and their constraint comment wording; the ci.yml comment fix; which docs/CHANGELOG edits the house convention demands (check recent ci:* commits for CHANGELOG practice); and the acceptance-evidence procedure (gh workflow run benchmark.yml --ref <branch>, watch to green, record duration in the PR body).
6. Commit it as: docs(plan): the benchmark-lane off-PR dispatch chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks (especially: anything that could make the first real main-push Bencher upload fail), estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the benchmark-lane dispatch ("benchmark lane off PRs + Bencher fix + bench build cost").

1. Read AGENTS.md, the origin report .artifacts/reports/2026-10-21-benchmark-ci-cost-analysis-report.md (§2-A/§2-C normative), the dispatch plan .artifacts/plans/2026-10-21-benchmark-lane-dispatch-plan.md §1/§2, and the stage-1 plan .artifacts/plans/2026-10-21-benchmark-lane-plan.md in the worktree ../taskboard-WT-ci/benchmark-lane-off-pr (branch ci/benchmark-lane-off-pr). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan: new .github/workflows/benchmark.yml (push-main + Thursday-04:00-UTC cron + workflow_dispatch; pinned bencher CLI 0.6.13 and the stage-1-resolved sccache pin; BENCHER_API_KEY: ${{ secrets.BENCHER_API_TOKEN }} on the bencher step — the old BENCHER_API_TOKEN env name is the exact bug this chunk kills; upload guarded on github.ref == 'refs/heads/main'; plain cargo bench --workspace on other refs); delete the benchmark: job from .github/workflows/ci.yml and reword its dangling "benchmark job's lane (every PR)" comment; add [profile.bench] (lto = false, codegen-units = 16) to root Cargo.toml with the constraint comment from the plan; make the doc/CHANGELOG edits the plan records. Every commit standalone (rebase-merge hygiene), subjects like ci(benchmark): ... / build(profile): ....
3. Run the full AGENTS §10 gate suite and make it green. Workflow YAML is not compiled locally — validate structure by inspection against the sibling workflows and, if available, a YAML linter; the live-dispatch evidence run is stage 3's acceptance step, not yours.
4. Do NOT touch the backlog, do NOT change branch protection (impossible from the repo anyway), no bench sources, no criterion config.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the exact diff summary, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the benchmark-lane dispatch (branch ci/benchmark-lane-off-pr).

1. Read AGENTS.md, the origin report .artifacts/reports/2026-10-21-benchmark-ci-cost-analysis-report.md (§2-A/§2-C), the dispatch plan .artifacts/plans/2026-10-21-benchmark-lane-dispatch-plan.md §1/§2 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-ci/benchmark-lane-off-pr.
2. Begin with the rebase: git fetch origin --prune; rebase onto origin/main per AGENTS §8's rebase dance (drop already-merged duplicates; resolve only real conflicts); push --force-with-lease if history was rewritten. Review versus the post-rebase base.
3. Functional focus — this chunk's specific traps: (a) the env fix: the bencher step must carry BENCHER_API_KEY (secret name unchanged) — any residual BENCHER_API_TOKEN env reference is the exact bug recurring; (b) the upload guard must key on github.ref == 'refs/heads/main', so a feature-branch dispatch cannot write to Bencher branch main, and benches must run exactly once per event (no leftover double-run structure from ci.yml); (c) no continue-on-error may appear on the bencher step — failures are loud by design; (d) [profile.bench] must be strictly additive and must not change dev/test/release outcomes — check the Cargo.toml diff shape; (e) no dangling ci.yml references to the removed job (the nextest-step comment), and the required-check implication — "Performance Benchmarking" stops existing as a PR check — must be called out in the PR body for the user's branch-protection action; (f) every third-party action/CLI is version-pinned with a recorded-reason comment (house pin policy).
4. Code-quality mandate (verbatim from §1): beyond functional review, evaluate (a) the diff shape — only the scoped files, no drive-by edits, comments state constraints not narration; (b) workflow hygiene — deliberately pinned action/CLI versions with a recorded reason per house pin policy (cargo-deny@0.20.2 style), steps guarded by conditions that are exactly right for each trigger event; (c) manifest hygiene — the [profile.bench] section must be additive and must not perturb the dev/test/release profiles. Findings are fixed on the branch like any other finding.
5. Re-run the full AGENTS §10 gate suite yourself; any failure is a finding.
6. Acceptance evidence: gh workflow run benchmark.yml --ref ci/benchmark-lane-off-pr, then gh run watch to completion — the run must execute the bench suite green and skip the Bencher upload; record the observed duration in the PR body (evidence, not a gate — no quality ceiling).
7. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
8. Ship: push the branch, open the PR to main with gh (body includes: the evidence duration, the loud branch-protection instruction, and the "first-ever successful Bencher upload is expected on the next main push — verify the Track Benchmarks with Bencher step green" note), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, repeat. main is protected: never merge; the user merges on GitHub.
9. Teardown: from the project root, git worktree remove --force ../taskboard-WT-ci/benchmark-lane-off-pr (everything is pushed; a resuming round recreates the worktree on the branch via git worktree add).

Report: verdict, findings (functional + mandate), what you fixed, PR URL, the evidence-run duration, green-checks confirmation, merge-state confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Completion bookkeeping

- No backlog entries close here (this work originates from the
  2026-10-21 analysis report, not a numbered backlog idea). If the
  implementing agent spots out-of-scope improvement ideas (e.g.
  per-PR regression gate, sccache for all jobs), they go to new
  `backlog:`-prefixed GitHub issues per AGENTS §7 — body summarizing +
  permalink to the analysis report — not into this chunk's diff.
- **Deliberately not dispatched here**: variant B (per-PR Bencher
  regression gate with quick-mode criterion + thresholds), threshold/
  alert configuration on the Bencher server, sccache rollout to the
  remaining `ci.yml` jobs, compile-only bench smoke on PRs, runner
  changes (variant D), iai-callgrind (variant E). Homes: future rounds
  against the analysis report §2; open issues if the user wants them
  scheduled.
- **Opens after merge**: the user removes "Performance Benchmarking"
  from required PR checks and merges; the next main push (or a
  dispatch on `main`) delivers the first-ever successful Bencher
  upload — verify the step green and the data in the Bencher UI; from
  then on the Bencher main-branch trend is fed by every main push plus
  the Thursday cron.
