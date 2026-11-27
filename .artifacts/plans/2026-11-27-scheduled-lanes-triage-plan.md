# Scheduled CI Lanes Triage (#1403, #1479, #1480, #1457) — Plan & Agent Prompts

- **Date**: 2026-11-27
- **Kind**: stage-1 triage plan (diagnose → decide → plan) for the chunk
  "scheduled CI lanes triage": one implementation chunk with full
  three-stage prompts (#1403) and a dispatcher-owned verification /
  bookkeeping checklist with no agent dispatch (#1479, #1480, #1457).
  No code and no workflow change in this commit.
- **Origin**: issues #1403 (ci p2), #1479, #1480, #1457; prior round
  `.artifacts/plans/2026-11-25-scheduled-lanes-debt-plan.md` (§1.1–§1.3,
  normative for the D1 fixes that already landed); lane-watch law
  `scripts/lane-watch.sh` + `.github/workflows/lane-watch.yml` (#1413).
- **Status**: READY — chunk 1 is dispatchable immediately; the
  dispatcher checklist is dispatchable immediately (Rulings R-1/R-2
  have safe defaults; proceed on them).

## 0. Verified reality (evidence base, gathered 2026-11-27)

All evidence gathered live on this stage via `gh run list/view` and the
worktree at `origin/main` = `cbc3eb4f`. Workflow files read in full;
line numbers below are against that tree.

### 0.1 The D1 fixes already landed on `main` (2026-11-25)

| Commit | Fixes | Mechanism |
|---|---|---|
| `e30dffea` | #1403 (build leg) | apt line in `nextcloud-integration.yml` gains the six Slint GUI packages |
| `526be42a` | #1402 / now #1479 | `miri.yml` re-slices `domain — core` into `miri-domain-core-behavioural` + `miri-domain-core-data` along a measured seam; cap stays 120; comment block at `miri.yml:44-60` |
| `e20cff6d` | #1401 / now #1480 | `mutation-testing.yml` shards the sweep into a matrix (`mutation-testing.yml:41`), drops `continue-on-error: true` (comment at `:123`), `if: always()` upload kept |

**The decisive timing fact**: the last *scheduled* runs of miri and
mutation-testing are 2026-11-23 (45169987760 / 45191860986) — **two
days before the fixes landed**. No scheduled run of either lane has
executed the fixed shape. The next cron windows are Monday
2026-11-30 03:00 UTC (miri) and 04:00 UTC (mutation).

### 0.2 #1403 — Nextcloud Integration: build fixed, real test failure reached

- Lane history (`gh run list --workflow nextcloud-integration.yml`):
  scheduled `failure` every Wednesday since 2026-10-21 while the apt
  build broke; dispatcher's post-merge verification run
  **45728295667** (workflow_dispatch, 2026-11-26) shows the apt fix
  works: the build leg compiles and the Docker tier reaches the test
  step for the first time.
- Failing job/step: `Nextcloud Integration (docker) — stable` →
  "Docker-tier integration tests (app daemon + CLI journeys)".
  All 22 `taskboard-sync-nextcloud::it_docker` tests PASS; the failure
  is the app-tier suite (`taskboard-app::it_docker`, 5/13):
  ```
  FAIL [   3.342s] ( 5/13) taskboard-app::it_docker it_nextcloud_docker_cli_editing_push
  thread 'it_nextcloud_docker_cli_editing_push' panicked at
    crates/taskboard-app/tests/it_docker.rs:607:5:
  assertion `left == right` failed
  ```
- The asserted line (`crates/taskboard-app/tests/it_docker.rs:607`):
  `assert_eq!(server_stacks.len(), 1);` — the first server-side
  assertion after the full CLI journey (publish board → rename board
  → rename stack to `{run}-col2` → **move stack to order 4** (`:520-528`)
  → edit/label/delete tasks via CLI → one sync → read server truth via
  the raw DeckClient).
- Test reads: this is the *first* server-side read of the journey; the
  failure means the Deck server returned **≠ 1 stack** on a board the
  test expects to have exactly one (`{run}-col2`, order 4, one card).
- **This test has never had a green CI observation**: the lane has
  been red since 2026-10-21, all of it at the apt build step until
  2026-11-26. There is no evidence it passed on CI ever — and no
  evidence yet of what the server actually returned. Local Docker is
  available on the dev box (docker 29.6.1, daemon OK), so the run is
  reproducible locally.
- Workflow shape: `nextcloud-integration.yml` — Wed 04:00 UTC cron
  (`:26-27`), `workflow_dispatch` (`:28`), job `timeout-minutes: 30`
  (`:41`). The 30-minute cap was NOT the failure mode here (the run
  died 8 min in).

### 0.3 #1479 (miri) and #1480 (mutation) — the red runs pre-date their fixes

- `miri.yml` red run 45169987760 (schedule, 2026-11-23, **pre-fix**):
  job view shows exactly the #1402 shape — `cargo miri (domain — core)`
  `cancelled` at 11:58:50 → 13:59:05 (120-min cap) while the three
  siblings finished in 4 min (pipeline), 41 min (remote/state
  properties), 99 min (state). The re-sliced five-job shape
  (`miri-domain-core-behavioural` / `-data`) has never run anywhere.
- `mutation-testing.yml` red run 45191860986 (schedule, 2026-11-23,
  **pre-fix**): both matrix jobs `cancelled` at exactly 240 min
  (14:13:24 → 18:13:45 / 18:13:43). The post-fix shape removes the
  silent `continue-on-error` and re-shards, sized with ≥25% headroom.
- The lane-watch verdicts `timeout-shape` on 2026-11-26 (run
  45728300760) classified these stale pre-fix runs; they opened
  #1479/#1480 because the pre-existing #1402/#1401 were closed on
  2026-11-26 when their fixes landed.

### 0.4 #1457 — the premise is falsified: schedule events exist and are green

- `gh run list --workflow benchmark.yml --event schedule --limit 5`
  (2026-11-27):
  | Run | Date | Conclusion |
  |---|---|---|
  | 45829109881 | 2026-11-26 13:26 | **success** (14m15s) |
  | 44523311837 | 2026-11-19 12:22 | failure (10m14s) |
  | 43214253956 | 2026-11-12 12:02 | success |
  | 42021443616 | 2026-11-05 11:44 | success |
  | 40859089232 | 2026-10-29 10:37 | success |
- The workflow file landed 2026-10-21 (`47c1d960`); the first cron
  fired 2026-10-29. **Five schedule-event runs exist** — the D1
  triage's "zero schedule events" claim was wrong (a bad query, not a
  dead cron; note scheduled start times drift hours past 04:00 UTC
  under GitHub load — 13:26 on 11-26 — which is normal, not a filter
  mismatch).
- The single schedule failure (44523311837, 2026-11-19) is a transient
  apt fetch error on the runner ("Could not wait for server fd …
  Resource temporarily unavailable" across the Slint GUI debs,
  exit 100) — not a lane defect; the very next scheduled run (11-26)
  is green.
- `lane-watch` classed `benchmark.yml red stale` and `lane-watch.yml
  red stale` on 2026-11-26 because at 00:23 UTC the latest *scheduled*
  runs it saw were 11-19's failure and (for itself) nothing scheduled
  yet. Both classifications are now stale: benchmark is green as of
  11-26 13:26, and lane-watch's first scheduled run fires Monday
  2026-11-30 06:00 UTC (the 11-26 run was `workflow_dispatch`).
- Note the lane-watch run 45728300760 itself "failed" **by design**:
  `scripts/lane-watch.sh` live path ends `if (( any_red )); then exit
  1` — a red verdict is the dead-man's signal, and a red lane-watch
  workflow run is the alarm, not a second bug.

## 1. Verdicts

| Issue | Verdict | Fix / check |
|---|---|---|
| #1403 | **hypothesis — real integration finding** | Diagnose `it_docker.rs:607` via local Docker repro (chunk 1); fix the defect it found — product or test per the evidence; the test is never weakened |
| #1479 | **needs-a-live-trigger** | Fixes already landed (`526be42a`); prove with a manual `gh workflow run miri.yml` (R-1 default) or the Monday 11-30 03:00 UTC cron; no code change planned |
| #1480 | **needs-a-live-trigger** | Fixes already landed (`e20cff6d`); prove with a manual `gh workflow run mutation-testing.yml` (R-1 default) or the Monday 11-30 04:00 UTC cron; no code change planned |
| #1457 | **root-caused — false premise, close** | Schedule events exist (five since 10-29) and the latest (45829109881, 11-26) is green; the lone failure (44523311837, 11-19) was a transient runner apt-fetch flake. Close with a corrected-data comment; no code change |

## 2. Decisions (constraints honoured)

1. **No timeout is widened anywhere in this chunk** (AGENTS §10 / CI
   law). #1479/#1480's red is fixed by the *already-landed* re-slice
   (miri) and shard + loud-cap-kill (mutation) — budget fitted
   honestly, caps stay 120/240. #1403 died at 8 of 30 minutes — no
   cap interaction at all.
2. **No test is weakened or skipped** (AGENTS §5).
   `it_nextcloud_docker_cli_editing_push` is a REAL integration
   finding: the lane compiled for the first time on 2026-11-26 and
   immediately caught something the native suite cannot see (server
   state after a full CLI editing journey). Chunk 1's stage 2 must
   first *diagnose* — print what the Deck server actually returns —
   then split product-vs-test defect on evidence. Leading hypothesis
   (to be confirmed or killed by the repro, not assumed): the journey
   `stacks::move_`s the board's only stack to **order 4**; Deck may
   materialise placeholder stacks up to that order (or the app's
   stack-move semantics against live Deck differ from the assumption),
   so `server_stacks.len()` is 2–5, failing `assert_eq!(…, 1)` at
   `:607`. If the repro shows the app wrote semantics Deck rejects or
   misrepresents, that is a **product bug** — the product fix is
   planned and executed in chunk 1, and the assertion stands. If the
   repro shows Deck's server behaviour legitimately differs from what
   the test asserts (e.g. placeholder stacks are Deck-canonical for a
   sparse order), the *expectation* is corrected to the documented
   real contract with a comment naming the constraint — an expectation
   fix backed by evidence, never a deletion of the journey's
   server-truth coverage.
3. **#1457 is closed, not repaired**: GitHub's own run data falsifies
   the issue body. The dispatcher posts the corrected table (§0.4) as
   a comment and closes the issue. The 11-19 apt-fetch flake is
   *noted* in the closure comment; it is a transient runner-network
   error, self-healed on the next window — no retry wrapper is added
   (a flake-shaped one-off on a free-tier schedule; if it recurs,
   lane-watch opens a fresh issue and it is triaged then).
4. **#1479/#1480 get no code**: their red runs are pre-fix artifacts;
   the correct action is the live trigger (R-1), then close with the
   green run id. If the manual dispatch shows the re-slice/shard still
   doesn't fit, that is NEW information — comment on the issue,
   re-open the triage (measurement-first, per the D1 plan §1.2/§1.3
   shapes), and do not silently widen caps.
5. **lane-watch self-noise is tolerated**: `lane-watch.yml red stale`
   before its first scheduled run self-resolves Monday 06:00 UTC. No
   watcher change.
6. **The shared apt composite action** (five workflows installing the
   six Slint GUI packages by copy — the drift that caused #1403) is
   out of scope here: filed as a `backlog:` issue (see §6), not
   implemented in this chunk.

## 3. Chunks & ordering

| Chunk | Slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §0.2, decision 2 | `fix/nextcloud-cli-editing-push` | `crates/taskboard-app/tests/it_docker.rs` (diagnose) + product or test fix per evidence | M |
| 2 (dispatcher-only, no agent) | §0.3, §0.4, decisions 3–5 | n/a | live-trigger verification for #1479/#1480; closure of #1457 | S |

**Dependency graph**: chunk 1 and the chunk-2 checklist are
independent (disjoint: one touches Rust/app test + maybe sync code,
the other only `gh` triggers and issue bookkeeping). Chunk 2 may run
immediately; its miri/mutation verdicts do not depend on chunk 1.
Default order: fire chunk 2's manual dispatches first (they queue on
GitHub runners for ~2–4 h anyway), dispatch chunk 1 stage 2 in
parallel, then chunk 2 bookkeeping when runs settle.

## 4. Verification (what the dispatcher runs; what green looks like)

| Item | Dispatcher action | Green |
|---|---|---|
| #1479 | `gh workflow run miri.yml` (after R-1 default granted); poll `gh run list --workflow miri.yml --limit 1` then `gh run view <id> --json jobs` | All five (re-sliced) jobs `success`; each under 120 min; note per-job minutes on the issue when closing |
| #1480 | `gh workflow run mutation-testing.yml`; same polling | Matrix shards + ui-pure + app-pdf jobs `success` (survivors reported is fine — the lane reports survivors, it fails on infra); no job cut at exactly 240 min |
| #1457 | None (already green: run 45829109881) | Issue closed with corrected-data comment |
| #1403 | After chunk 1's PR lands: `gh workflow run nextcloud-integration.yml` | Both legs green, all 13 app-tier `it_docker` tests pass, 22 sync-tier pass; close #1403 |

Lanes that only prove on their own cron: none remain unprovable —
every lane here has `workflow_dispatch`; the Monday crons are the
fallback if a manual dispatch is ruled out (R-1).

## 5. Rulings requested (operator)

- **R-1 — live-trigger route for #1479/#1480** (default: **manual
  dispatch now**). Options: (a) manual `gh workflow run` on
  `main` today — same-day proof, consumes free-tier minutes (~2 h miri,
  ~4 h mutation); (b) wait for the Monday 2026-11-30 cron windows —
  zero minutes spent, verdicts Tuesday in lane-watch. Default (a) for
  miri (cheap, and its re-slice is unproven), (a) for mutation too —
  if minutes are a concern, (b) is acceptable; the issues just stay
  open until Tuesday.
- **R-2 — `timeout-minutes` changes**: none requested. Caps stay
  120 (miri) / 240 (mutation) / 30 (nextcloud-integration). If any
  post-fix manual run lands within 10% of its cap, that goes back to
  triage (measurement first), not to a cap raise.
- **R-3 — product-vs-test defect split authority** (chunk 1): if the
  local repro shows the test's expectation (a one-stack board with the
  stack at order 4) contradicts Deck's documented/canonical behaviour
  while the app behaved correctly, the test expectation may be
  corrected with a constraint comment. If the app mis-modelled Deck
  semantics, the product fix is mandatory and in-chunk. Default:
  evidence decides per decision 2; no pre-granted weakening.
- **R-4 — shared apt composite action** (backlog issue, this chunk):
  schedule later; default unscheduled (no milestone).
- **R-5 — lane-watch self-noise**: tolerate the self `red stale`
  classification until its first scheduled run (Mon 06:00 UTC).
  Default: no watcher change.

## 6. Out-of-scope findings → backlog issues

- Shared GUI-deps composite action (drift root cause behind #1403):
  filed this stage — **#1536**.
- (No other out-of-scope findings; the 11-19 benchmark apt flake is
  recorded in the #1457 closure comment, not an issue — one transient
  observation, self-healed.)

## 7. Chunk 1 — the #1403 integration finding (`fix/nextcloud-cli-editing-push`)

- **Covers**: diagnosis and fix of
  `it_nextcloud_docker_cli_editing_push` panicking at
  `crates/taskboard-app/tests/it_docker.rs:607` on run 45728295667;
  closes #1403 once the lane is green (dispatcher verification, §4).
- **Normative**: this plan §0.2, §2 (decision 2), §5 (R-3); AGENTS §5
  (test law — no weakening, a failing test is a bug candidate),
  §2 (crate boundaries — any product fix lands in the crate that owns
  the defect, no boundary crossing), §4 (approved stack).

### 7.1 Stage 2 prompt (IMPLEMENTATION) — paste verbatim

> You are the STAGE 2 (IMPLEMENTATION) agent for chunk "nextcloud
> CLI editing push" in the taskboard repo (D4id4los/taskboard),
> branch `fix/nextcloud-cli-editing-push`, worktree
> `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-nextcloud-cli-editing-push`.
> The stage-1 plan is normative:
> `.artifacts/plans/2026-11-27-scheduled-lanes-triage-plan.md` — read
> §0.2, §2 (decision 2), §5 (R-3) first.
>
> **Environment — setup, in order**
> 1. `cd /home/viktoria/src/taskboard && scripts/worktree-doctor.sh`;
>    nonzero = STOP and report.
> 2. `git -C /home/viktoria/src/taskboard fetch origin --prune`
> 3. If the worktree/branch already exist from stage 1, continue in
>    them; if the branch's PR already landed, work on
>    `fix/nextcloud-cli-editing-push-v2` off the fetched
>    `origin/main` in a fresh worktree
>    (`git worktree add -b fix/nextcloud-cli-editing-push-v2
>    ../taskboard-WT-fix-nextcloud-cli-editing-push-v2 origin/main`).
>    Never `git worktree add <path> main`. Do not touch any other
>    `taskboard-WT-*` directory.
>
> **Task.** Run 45728295667 (2026-11-26) fails
> `it_nextcloud_docker_cli_editing_push` (crates/taskboard-app/tests/
> it_docker.rs:607, `assert_eq!(server_stacks.len(), 1)`) on the
> Nextcloud Integration lane. Local Docker is available — reproduce
> locally first:
> 1. Reproduce: run the single test against the local Docker Nextcloud
>    harness exactly as the docker-tier harness sets it up (see the
>    test module's `common` harness and
>    `.github/workflows/nextcloud-integration.yml`'s Docker-tier
>    step for the env/container contract). Any cargo invocation routes
>    through `scripts/memory-gate.sh`; the run may exceed 5 minutes —
>    background it and poll, committing landed work first (discipline
>    item 3). If the test does NOT reproduce locally, that is itself
>    the finding — capture the environment delta (server image
>    version, data, ordering) and diagnose from it; do not shrug.
> 2. Diagnose at 607: log/print the actual `server_stacks` the
>    DeckClient sees (stacks, titles, orders) in a temporary
>    diagnostic run (not committed as a text-assertion change). Decide
>    the product-vs-test split per plan §2 decision 2 and §5 R-3:
>    - App mis-models Deck (wrong stack-move semantics, phantom
>      writes, sync pushes more than the journey did) → fix the
>      product in the owning crate (likely `taskboard-sync-nextcloud`
>      or the app command layer), add/keep the regression coverage,
>      the assertion at 607 stands unchanged.
>    - Deck legitimately returns a different shape than the test
>      assumes (e.g. placeholder stacks for a sparse order) → correct
>      the test's expectation to the documented real contract, with a
>      comment stating the constraint (Deck version/behaviour) — do
>      not delete coverage, do not loosen the journey's other
>      assertions.
> 3. Implement TDD: the failing state first, then the fix, then the
>    green run of this test plus the full `it_docker` suites in
>    `taskboard-app` and `taskboard-sync-nextcloud` (docker tier).
>    Every commit standalone (rebase-merge hygiene):
>    `<type>(<module>): <short-desc>` + body.
> 4. Gate: pre-commit hooks run automatically; the flock-safe fast
>    gate is `scripts/run-quality-gates.sh cargo nextest run
>    --workspace`. Background + poll; the full §10 suite is the
>    dispatcher's, not yours. Never touch `main`, never push, never
>    open a PR, never `gh workflow run` anything.
>
> **Tests that must not have been weakened**: the entire
> `it_nextcloud_docker_cli_editing_push` journey — its assertions at
> `it_docker.rs:607` (`server_stacks.len() == 1`), 608–609
> (title/order), 610 (doomed card gone), the due-date and label
> server-truth reads, and the local-truth block at the end. A diff
> that deletes, skips, `#[ignore]`s, or loosens any of these without
> the R-3 evidence trail is a rejected stage.
>
> **Diff-shape rules**: only files the diagnosis implicates (the test
> file, the owning product crate(s), and nothing else); no drive-by
> refactors; no new dependencies; no timeout/CI-workflow edits (this
> chunk is Rust, not CI). Out-of-scope findings → `backlog:` issues
> immediately (AGENTS §7 standing authorization; search first).
>
> **Report back**: the repro result, the diagnosis (what the server
> actually returned, and which defect split the evidence supports),
> the fix summary, commit SHAs, gate evidence, and any issue numbers.

### 7.2 Stage 3 prompt (REVIEW) — paste verbatim

> You are the STAGE 3 (REVIEW) agent for chunk "nextcloud CLI editing
> push" in the taskboard repo (D4id4los/taskboard), branch
> `fix/nextcloud-cli-editing-push`, worktree
> `/home/viktoria/nextcloud/viktoria/programming/taskboard-WT-fix-nextcloud-cli-editing-push`.
> The stage-1 plan is normative:
> `.artifacts/plans/2026-11-27-scheduled-lanes-triage-plan.md` §0.2,
> §2 (decision 2), §5 (R-3), §7.
>
> **Discipline (binding — docs/agent_dispatch.org; restate it verbatim
> in any stage prompt you write)**: A subagent is killed after ~600s
> without observable activity. **Long-command discipline**: (1) *The
> coverage trio never runs inside a subagent*: a stage-2/stage-3
> agent's gate is the pre-commit hooks (automatic) plus
> `scripts/run-quality-gates.sh cargo nextest run --workspace`
> (flock-safe; background + poll with useful work between polls if
> needed); the dispatcher runs the full AGENTS §10 suite between
> stage 2's landing and stage 3's ship. *Flock-deferral rule: when the
> gate lock is contended and no other useful work remains, never
> block — commit all landed work, report "gate deferred: lock held by
> <holder>", and end the stage; the dispatcher runs the gate.* (2)
> *Stage 3's scope ends at PR-open*: push, open the PR, report — the
> dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
> teardown, and at-merge issue closures. (3) *Any command expected
> over ~5 minutes runs backgrounded* with short interleaved polls and
> useful work between polls — commit all landed work BEFORE any wait.
> Large scratch under /var/tmp, never /tmp. (4) *Every agent-launched
> cargo invocation routes through `scripts/memory-gate.sh`* — nothing
> bare. (5) *Stage prompts restate this discipline verbatim.* Any
> docker-tier test run you re-run for review is a self-capped long
> run: route `memory-gate.sh --preflight` only, never hold the gate
> lock across it.
>
> **Begin by rebasing onto `main`**: `git fetch origin --prune`;
> rebase the branch onto `origin/main` (AGENTS §8's rebase dance:
> drop already-merged duplicate commits, resolve only real
> conflicts); `git push --force-with-lease origin
> fix/nextcloud-cli-editing-push` if history was rewritten. Then
> review the entire post-rebase branch diff against the plan's
> acceptance criteria: the diagnosis evidence matches the fix shape
> (§2 decision 2 — product fix if the app mis-modelled Deck,
> evidence-backed expectation correction only if Deck's canonical
> behaviour differs); every journey assertion from stage 2's
> "must not have been weakened" list is intact or corrected WITH its
> constraint comment; diff stays inside the diagnosed files; commits
> are standalone with `<type>(<module>):` subjects.
>
> **Code-quality mandate** (stage 3): beyond functional review,
> evaluate (a) code quality: naming, comment discipline (comments
> state constraints, not narration), idiomatic use of the approved
> stack, no dead code or speculative generality, test quality per
> AGENTS §5; (b) the Single Responsibility Principle: the crate-role
> boundaries of AGENTS §2 are the SRP line; (c) a single level of
> abstraction per function body. Findings are fixed on the branch.
>
> Re-run the fast gate if you changed anything
> (`scripts/run-quality-gates.sh cargo nextest run --workspace`,
> background + poll; flock-deferral rule applies). Then push, open
> the PR to `main` with `gh` (body: the diagnosis, the fix, the
> evidence trail, `Fixes #1403` once the lane-green dispatcher
> verification in plan §4 is the closure step — link the verification
> run you expect), and report the URL. Do NOT watch checks, do NOT
> merge, do NOT remove the worktree (dispatcher owns teardown), do
> NOT `gh workflow run` anything.
>
> **Report back**: verdict, findings (functional + mandate), what you
> fixed, PR URL. Never merge; never touch main.

## 8. Chunk 2 — dispatcher checklist (no agent dispatch)

1. R-1 default: `gh workflow run miri.yml` and `gh workflow run
   mutation-testing.yml` (both have `workflow_dispatch`); poll to
   completion (miri ~≤2 h, mutation ~≤4 h); on green, close #1479 and
   #1480 with the run ids and per-job minutes; on red, comment the
   finding on the issue and leave it open (new triage round).
2. Close #1457 with the corrected schedule-run table from §0.4,
   noting the D1 query error and the transient 11-19 apt flake.
3. After chunk 1's PR lands on `main`: `gh workflow run
   nextcloud-integration.yml`; on both legs green, close #1403 (plan
   §4).
4. Monday 2026-11-30 after 06:00 UTC: confirm lane-watch's first
   scheduled run classifies `kani green fresh`, `benchmark green
   fresh`, and (if R-1 defaults ran) green across the board; the
   `lane-watch.yml red stale` self-classification should clear itself.

## 9. Completion bookkeeping

- **Closes on evidence**: #1457 (chunk 2 step 2), #1479/#1480 (chunk 2
  step 1, green runs), #1403 (chunk 2 step 3, green lane).
- **Deliberately not dispatched here**: the shared apt composite
  action (#1536); self-hosted runners (#117, its own issue); any
  PROPTEST_CASES budget change (D1 plan §1.2 step 3 — needs its own
  measurement); the mutants delta/full-sweep cadence split (D1 plan
  §1.3 step 3 follow-up).
- **Estimated size**: chunk 1 = M (one test diagnosis + a bounded
  product-or-test fix, docker-tier verification); chunk 2 = S
  (triggers + issue bookkeeping). This plan commit: docs-only.
