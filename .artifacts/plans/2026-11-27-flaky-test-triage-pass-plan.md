# Flaky-Test Triage Pass, 2026-11-27 — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-27
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  execution → review) for the dedicated flaky-test triage pass over
  the `flaky-test` + `watchlist` label query (AGENTS §7 flaky law),
  covering the 11 open watchlist issues. No code in this commit.
- **Origin**: AGENTS.md §7 ("Flaky tests" bullet — triage in a
  dedicated pass over that label query; never opportunistically);
  the per-issue comment threads (primary record, cited per issue
  below); watchlist issue query
  `gh issue list --repo D4id4los/taskboard --state open --label flaky-test --limit 100`
  (verified 2026-11-27: #697, #1020, #883, #968, #926, #1336,
  #1486, #336, #287, #384, #650).
- **Status**: READY — chunk 1 (issue bookkeeping) is dispatchable
  immediately, **except** the four CLOSE-AFTER-WINDOW closures
  (#926, #968, #1336, #1486) which are gated on the operator's
  ruling (§4 Rulings requested). The KEEP-WATCHING dated records
  are not gated.

## 0. Verified findings (evidence base)

All file:line references are at `origin/main` `c9fe5407`
(2026-11-27). Every prior hypothesis from the read-only triage was
re-verified against the landed tree; two material corrections
against the preliminary read are flagged.

### Corrections to the preliminary read

1. **#926 — the fix is already on `main`.** The preliminary read
   said the fix lived on unmerged commit `83351dd8` (branch
   `ci/cargo-mutants-capped`). That commit exists and is unmerged,
   but the *same* change landed on `main` as **`6f10ceb0`**
   (2026-11-17, "test(settings): the rejected-verify persistence
   probe is structural" — verified
   `git branch -a --contains 6f10ceb0` shows main-integrated
   branches; the current test at
   `crates/taskboard-app/tests/services/ui_settings.rs:240`
   `a_failing_verify_persists_nothing_and_replies_typed` asserts
   byte-identity `assert_eq!(written, array_config_text(...))`, no
   substring probe anywhere in the file). Clean window: 2026-11-17 →
   2026-11-27, zero recurrence in the CI scan (§0.12).
2. **#968 — the fix is already on `main`.** The preliminary read
   said the fix lived on unmerged commit `8c7d3770` (branch
   `test/shutdown-budget`). That commit is unmerged, but the same
   test-side change landed on `main` as **`adeba1b0`** (2026-11-19,
   "test(app): tighten test shutdown budgets to 1s/2s (#1071)").
   The current test at
   `crates/taskboard-sync-nextcloud/tests/suite/actor_wiremock.rs:825`
   `a12_shutdown_exits_the_loop` awaits the join handle directly
   with the recorded trade-off comment; no `yield_now`/`10_000`
   poll remains anywhere in the suite. Clean window: 2026-11-19 →
   2026-11-27.

### Per-issue verified records

3. **#697** (`tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`,
   `crates/taskboard-app/tests/cli/cli_tty.rs`) — **recurred twice
   more under load**: 2026-11-25 and 2026-11-26, both local
   full-suite flock runs, new signature `left: None, right: Some(0)`
   (the post-cancel session-termination/exit-code assert, not the
   old "line after the stalled one executed" at the old
   `cli_tty.rs:1687`), PASS in isolation immediately after. The
   budget-side determinism fix **is on main** (`DESIGNED_SILENT_WINDOW`
   at `cli_tty.rs:58`, `PTY_IDLE_BUDGET = max(70 s, 6 × D_max + D_max)`
   at `cli_tty.rs:84-88`, compile-time backstop pin at
   `cli_tty.rs:110`, landing r2 `50eecbfb`/`fc1b72f6`), yet the
   flake persists — supporting the armed H1 (production race in the
   PTY cancel path, dated 2026-11-17 comment) or a second seam.
   The gap-telemetry directory mode (`TASKBOARD_PTY_TEST_GAP_LOG`
   as a directory) is armed on CI with failure-only artifact upload,
   but the 11-25/11-26 observations were *local* — no gap log was
   captured for either. Best-supported hypothesis: production-side
   cancel race under load, unresolved; the next occurrence must
   capture the gap log (capture policy, 2026-11-16 comment).
   → **KEEP-WATCHING**.
4. **#1020** (`push_exec::tests::transport_failure_aborts_without_outcomes`,
   `crates/taskboard-sync-nextcloud/tests/suite/push_exec.rs`) —
   single observation (2026-11-20 full-suite flock run) **plus a
   sibling** recorded in the same issue: `create_label_transport_failure_aborts`
   failed the same way (same `Aborted { landed, .. } if landed.is_empty()`
   assert class, `push_exec/tests.rs:3215` area) in a 2026-11-20
   full-suite run. Same family as #287's F-1/F-2 hypotheses
   (transient loopback connect failure under accept-queue pressure);
   both tests pass solo. No captured failure output for either.
   → **KEEP-WATCHING** (capture policy per #287's thread applies
   to the family).
5. **#883** (`actor_wiremock l3_new_backoff_values_shape_the_next_retry`)
   — single observation (2026-11-17 full-suite flock run), no
   assertion output captured (the issue body records this), passes
   solo and in a loaded crate-suite re-run. Same wiremock-under-load
   family. → **KEEP-WATCHING**.
6. **#926** — see correction 1. Root cause fully established (not a
   flake, not a production bug: substring probe vs `/home` TMPDIR;
   4 deterministic repro observations recorded in-thread). Fix on
   `main` since 2026-11-17. → **CLOSE-AFTER-WINDOW**.
7. **#968** — see correction 2. Root cause fully established
   (10 000-yield `is_finished` poll lottery under load; teardown
   refuted by `.artifacts/reports/2026-11-19-actor-teardown-investigation-report.md`).
   Deterministic join-await fix on `main` since 2026-11-19; under a
   paused clock a hang cannot be a load flake, so the observed
   mechanism is structurally gone. → **CLOSE-AFTER-WINDOW**.
8. **#1336** (`golden_visuals::the_today_goldens_hold_across_the_scale_breakpoint_matrix`)
   — root cause established (zero-shot-timer deferral raced the next
   instantiation batch → mid-settle capture); fix `d5ed1aab`
   (2026-11-24, "fix(ui): the row-extent report writes its section
   extent in the same pass") **is on main** (PR #1389 merged at
   `e8025b04`), plus the deterministic-pinned-encoder corpus. Post-fix
   evidence: matrix green ×3 consecutive (in-thread, commit day) and
   5/5 consecutive green runs of both scale-matrix tests on the
   re-based corpus (2026-11-25 scope-check comment). Window is short
   (3 days). → **CLOSE-AFTER-WINDOW** (ruling requested; recommendation:
   close — the mechanism is removed by construction, not by luck).
9. **#1486** (`today_two_column_journeys::us17_s3`) — root cause
   established (fixture's Done row stamped `now − 30 m` falls into
   yesterday across the UTC-midnight window start → Done row dropped
   → `< 3` rows precondition). Fix **is on main**: `92bd704e`
   (2026-11-26, "test(ui): anchor the two-column fixture's Done row
   to the day window (#1436)"), anchoring through
   `days::day_window` + `fixture::harness_calendar` and minting the
   fixture from a single captured `now`. The issue's own closure
   condition: "one full day-window cycle passes with the anchored
   fixture green". The fix landed 2026-11-26 ~midday; the
   2026-11-27 00:00 UTC window boundary has since passed. →
   **CLOSE-AFTER-WINDOW** (ruling requested; recommendation: close
   once stage 2 verifies one green run of the test after the
   2026-11-27 00:00 UTC boundary — which the chunk's verification
   step does).
10. **#336** (`push_exec::tests::delete_404_re_resolves_and_deletes_at_the_fresh_binding`)
    — single observation 2026-10-18 (21 ms, never captured);
    triaged as mock-setup/verify under load, #287 family. Negative
    repro protocols 2026-10-26 (3× workspace, 10× crate) and
    2026-11-16 (15/15 loaded); CI recurrence scans 2026-11-01,
    2026-11-04, 2026-11-16 (300 runs) — zero recurrence. Standing
    in-thread ruling (2026-11-01): "one clean pass is not
    recurrence-free evidence for a never-captured failure" — but
    that ruling has now been re-earned three times over ~30 days.
    → **CLOSE-AFTER-WINDOW** (ruling requested; recommendation:
    close — the family root causes F-1/F-2 were fixed by PR #312
    and the specific test has a month of clean windows; reopening
    on a recurrence is cheap and the capture policy is on record).
11. **#287** (family entry: `actor_wiremock` plain-nextest + coverage
    lanes) — both mechanisms root-caused and fixed by PR #312
    (commit 1: wall-clock deadlines replacing virtual-deadline
    waits; commit 2: `refused_client` probe-connect hardening).
    Closure condition in-thread: "≥5 green dispatch runs soak AND no
    new occurrence". Last occurrence 2026-10-18 (coverage lane);
    ~40 days recurrence-free since, with scans on 11-01, 11-04,
    11-16 and this pass's 60-run scan (§0.12). → **CLOSE-AFTER-WINDOW**
    (ruling requested; recommendation: close — both closure
    conditions are satisfied many times over).
12. **Recurrence scan, this pass (2026-11-27)**: `gh run list
    --limit 60` → exactly one failed run, `45922334002`
    (2026-11-27 01:24Z, PR #1437 merge), both Test Execution and
    Code Coverage lanes failing on
    `schedule_colour_journeys::us27_s3_day_start_time_is_honoured`
    ("05:30 day start: start stamp", journey driver `RecvError`
    secondary). **Not a watchlist member** — it was triaged and its
    issue **#1513 opened and closed the same day** (00:00–05:30
    local window, calendar-today oracle vs day-start-anchored verb),
    outside this pass's scope. No failure in the scan window touched
    #697's, #336's, #287's, #384's, #650's, #883's or #1020's tests.
    (Scan depth 60 runs ≈ 2026-11-2x window; deeper windows were
    covered by the in-thread 150/300-run scans cited above.)
13. **#384** (`cli_login l13_alias_happy_path_writes_stderr_connect_keeps_it_empty`)
    — single observation 2026-10-21, content lost (static assert
    message); the *fix that landed* is the diagnostics capture
    (`60f8965a` on main: the assert now carries the buffer bytes),
    not a root-cause fix — the cause is undetermined. The next
    occurrence is high-signal (the negative repro protocol was
    spent once, 2026-10-26). → **KEEP-WATCHING** (closing would
    discard an armed, never-yet-fired high-signal capture).
14. **#650** (`taskboard-domain appdata::tests::no_local_ids_reach_the_dismissals_wire`)
    — CI-only single observation; armed hypothesis (proptest
    counterexample lost with ephemeral runner) and the substrate fix
    **landed**: #857 (2026-11-17) persists proptest regressions as a
    CI failure artifact (90-day retention) with a recorded triage
    recipe. The counterexample has not recurred since. →
    **KEEP-WATCHING** until the next occurrence delivers the
    captured case (or a long clean window earns closure — operator's
    call; recommendation: keep watching, the capture is armed and
    one occurrence would fully resolve the issue).

### Disposition table

| Issue | Test | Disposition | One-line reason |
|---|---|---|---|
| #697 | `tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes` | KEEP-WATCHING | Budget determinism landed yet two fresh load-correlated recurrences (11-25, 11-26, new exit-code signature); gap-log capture never obtained |
| #1020 | `push_exec::tests::transport_failure_aborts_without_outcomes` (+ sibling `create_label_transport_failure_aborts`) | KEEP-WATCHING | Two single load-correlated observations, #287-family, no captured output |
| #883 | `actor_wiremock::l3_new_backoff_values_shape_the_next_retry` | KEEP-WATCHING | Single load-correlated observation, no captured output |
| #926 | `ui_settings::a_failing_verify_persists_nothing_and_replies_typed` | CLOSE-AFTER-WINDOW | Root-caused (substring probe vs /home TMPDIR); fix `6f10ceb0` on main since 11-17; 10-day clean window |
| #968 | `actor_wiremock::a12_shutdown_exits_the_loop` | CLOSE-AFTER-WINDOW | Root-caused (yield poll-lottery); join-await fix `adeba1b0` on main since 11-19; mechanism structurally gone |
| #1336 | `golden_visuals::the_today_goldens_hold_across_the_scale_breakpoint_matrix` | CLOSE-AFTER-WINDOW | Root-caused (extent-report race); settle fix `d5ed1aab` on main since 11-24; 3×+5× green post-fix |
| #1486 | `today_two_column_journeys::us17_s3` | CLOSE-AFTER-WINDOW | Root-caused (day-window boundary); anchored fixture `92bd704e` on main since 11-26; verify one green run past the 11-27 00:00 UTC boundary, then close |
| #336 | `push_exec::tests::delete_404_re_resolves_and_deletes_at_the_fresh_binding` | CLOSE-AFTER-WINDOW | Family causes fixed (PR #312); single never-captured observation, 30+ days recurrence-free |
| #287 | `actor_wiremock` family (a10, coverage lane) | CLOSE-AFTER-WINDOW | Both mechanisms root-caused + fixed (PR #312); in-thread closure conditions (5-run soak + no recurrence) satisfied since 10-18 |
| #384 | `cli_login::l13_alias_happy_path_writes_stderr_connect_keeps_it_empty` | KEEP-WATCHING | Capture instrumentation landed but root cause undetermined; next occurrence is high-signal |
| #650 | `appdata::tests::no_local_ids_reach_the_dismissals_wire` | KEEP-WATCHING | Proptest-artifact persistence armed (#857); resolution needs the captured counterexample |

**Net: zero FIX / zero HARDEN items.** Every issue is either
root-caused with the fix already on `main` (the four closures plus
#336/#287) or an armed single-observation watch entry whose capture
policy must simply wait for the next occurrence. This pass's
dispatchable work is therefore **bookkeeping, not code**: dated
triage records on the KEEP-WATCHING issues, and (post-ruling) the
closures with label removal.

## 4. Rulings requested (operator)

| # | Ruling | Recommendation |
|---|---|---|
| R-1 | Close **#926** (remove `flaky-test`+`watchlist`) on the evidence of §0.6? Fix landed 11-17, 10 days clean, root cause deterministic and reproduced 4×. | **Close.** |
| R-2 | Close **#968** on §0.7? Fix landed 11-19; the observed mechanism cannot recur by construction. | **Close.** |
| R-3 | Close **#1336** on §0.8 despite the short (3-day) window? Mechanism removed by construction; 3× + 5× consecutive green. | **Close.** (The window is short, but a settle race is either fixed or it is not; more soak days add little.) |
| R-4 | Close **#1486** on §0.9 once stage 2 shows one green run of `us17_s3` after the 2026-11-27 00:00 UTC boundary? | **Close, conditional on that green run.** If the run cannot be shown, defer the closure to the next pass. |
| R-5 | Close **#336** on §0.10 contrary to the standing 2026-11-01 in-thread ruling ("one clean pass is not recurrence-free evidence for a never-captured failure")? That ruling has since been re-earned by two further protocols and three scans. | **Close.** A never-captured single observation with a month of clean windows is not worth a permanent watchlist slot; recurrence re-opens cheaply. |
| R-6 | Close **#287** on §0.11 (its own in-thread closure conditions met)? | **Close.** |
| R-7 | **#384**: keep the armed capture watching, or close as "instrumentation landed, cause unknown"? | **Keep watching** — closing an armed high-signal capture before it ever fires wastes its one-shot value. |

Not requested as rulings (house law decides): #697/#883/#1020/#650
stay open per the flaky law (recurrence without captured evidence or
armed capture awaiting its first sample).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | §0 dispositions + §4 rulings | `test/flaky-pass-2026-11-closures` | Issue bookkeeping only: dated triage-record comments on #697/#1020/#883/#384/#650; closure comments + `flaky-test`+`watchlist` label removal on the ruled-close set (#926, #968, #1336, #1486, #336, #287); the §0 verified evidence (file:line, run IDs, landed-commit SHAs) pasted into each comment. Explicit non-goals: no production or test code changes, no test weakening, no new assertions, no CI/workflow edits, no label additions. | no UX plan in origin | S |

**Dependency graph**: single chunk; R-1…R-7 gated on the operator's
rulings (the closures are held until they arrive — the KEEP-WATCHING
comments are not). No code, so no gate beyond pre-commit hooks.

**Dispatch mechanics**: one agent session per prompt, in order
(1.1 → 1.2 → 1.3); never two agents share a worktree. The chunk
works in `../taskboard-WT-test-flaky-pass-2026-11-closures` next to
the project root (AGENTS §8), branched off the then-current `main`.
**Branch names are never reused after their PR lands** (AGENTS §8,
issue #415): a resume works on `<branch>-v2`, never on the landed
branch. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch> origin/main` (explicit fetched start point,
#1185). Disk hygiene: run `scripts/worktree-doctor.sh` from the main
checkout before creating/recreating the worktree — nonzero exit
STOPs the chunk. Stage 3 removes the worktree as its final handoff
task.

**Shared execution model** (binding for every chunk):

**Long-command discipline** *(binding for every chunk, every stage —
added 2026-11-16 from the framework rollout; full law:
`docs/agent_dispatch.org`)*: a subagent is killed after ~600s without
observable activity, so long silent commands kill working agents.

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
   closures.
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
   *Flock deferral (#879)*: when the gate lock is contended and no
   other useful work remains, never block — commit all landed work,
   report "gate deferred: lock held by <holder>", and end the stage;
   the dispatcher runs the gate.
4. *Stage prompts restate this discipline verbatim* (an agent that
   must find it in another file will not carry it).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (commits present, tree clean, gate evidence re-runnable). A stalled
   or quota-cut stage is completed from its landed commits or resumed
   with a state-check message — never re-dispatched blind.
6. *Resource law (concurrency cap + memory gate)*: at most **3
   concurrent work-performing agents per box**, staggered ≥3 minutes
   apart; the dispatcher's full §10 suite counts as ONE slot; every
   agent-launched cargo invocation routes through
   `scripts/memory-gate.sh`. Evidence: the 2026-11-17 global OOM
   report; full law: `docs/agent_dispatch.org` §2.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: this document. The chunk is fully
   specified below; no per-chunk stage-1 prompt is dispatched.
2. **Stage 2 — execution agent**: continues in
   `../taskboard-WT-test-flaky-pass-2026-11-closures` (recreates it
   on the branch if missing), posts the issue comments and label
   changes per the acceptance list, commits the (empty-diff-safe)
   evidence note, does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main`,
   reviews the branch diff versus post-rebase base against the
   acceptance criteria + AGENTS rules plus the mandate below,
   re-runs whatever gates apply, fixes findings on the branch,
   pushes, opens the PR, reports the URL — scope ends at PR-open.

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, evaluate (a) code quality: naming, comment discipline
(comments state constraints, not narration), idiomatic use of the
approved stack, no dead code or speculative generality, test quality
per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line; (c) a single
level of abstraction per function body. Findings are fixed on the
branch.

## 2. Chunk 1 — issue bookkeeping & closures (`test/flaky-pass-2026-11-closures`)

- **Covers**: this plan §0 (dispositions + evidence) and §4
  (rulings). Concretely:
  1. **KEEP-WATCHING dated records** — post one comment per issue,
     each containing: this pass's date, the verified evidence from
     §0 (file:line at `c9fe5407`, the run/commit IDs), the current
     disposition and what the next occurrence must capture
     (restating the standing capture policy verbatim):
     - **#697**: the two fresh observations (2026-11-25
       `fix/severing-geometry-completions` stage-3 run; 2026-11-26
       `fix/dayreview-quota-bugs` stage-3 run, `left: None,
       right: Some(0)`, isolation PASS 10.2 s) recorded with the
       budget-fix-on-main fact (`cli_tty.rs:58,84-88,110`), the
       surviving H1 status, and the capture policy (full failure
       block + `TASKBOARD_PTY_TEST_GAP_LOG` directory-mode rerun).
     - **#1020**: the sibling `create_label_transport_failure_aborts`
       observation folded in (one issue per distinct test is
       violated by the sibling living in this issue's thread —
       note that #1020 remains the family anchor; do **not** open a
       second issue), the #287-family hypothesis reference, capture
       policy.
     - **#883**: single-observation status confirmed still standing,
       scan result (no recurrence in the 60-run window), capture
       policy.
     - **#384**: scan result, armed-capture status restated
       (assert carries buffer bytes since `60f8965a`), the
       high-signal warning.
     - **#650**: scan result, #857 armed-artifact status + triage
       recipe restated.
  2. **Closures (only for rulings R-1…R-6 granted as recommended;
     any declined ruling converts that issue to a KEEP-WATCHING
     comment instead)** — per issue: a closure comment citing this
     plan's §0 entry (evidence, landed fix SHA + date, clean-window
     dates and the scans backing them), then close as completed and
     remove both `flaky-test` and `watchlist` labels
     (`gh issue edit <n> --remove-label flaky-test --remove-label watchlist`).
     For **#1486** specifically: first run
     `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journey_suites us17_s3`
     and confirm green (today is past the 2026-11-27 00:00 UTC
     boundary the fix needed to cross); if it cannot be shown green,
     do not close — post the KEEP-WATCHING record instead and report.
  3. **Labels are never silently removed** (AGENTS §7): every label
     action rides the closure comment naming this plan.
  4. A one-commit docs artefact:
     `.artifacts/reports/2026-11-27-flaky-test-triage-pass-report.md`
     — the disposition table, the scan evidence, per-issue action
     log (comment posted / closed / labels removed), and any ruling
     deviations. This is the entire allowed diff besides nothing
     else — **no code files may change**.
- **Deliverable**: the report above + the five KEEP-WATCHING
  comments + up to six closure comments/label removals. GitHub-side
  state is the primary artefact; the report is the audit trail.
- **Acceptance** (stage 3 reviews against):
  - Every one of the 11 issues has exactly one new dated comment
    from this pass; no issue received two.
  - Closed issues match the granted rulings exactly; each closure
    comment cites the plan §0 entry and landed fix SHA; both labels
    removed on each; no label removed on any open issue.
  - `git diff origin/main...HEAD` touches exactly
    `.artifacts/reports/2026-11-27-flaky-test-triage-pass-report.md`.
  - No test weakened, no text assertion introduced, no fixed sleep,
    no quality ceiling — trivially satisfied by the empty code diff,
    and stage 3 verifies the diff is in fact empty of code.
  - Pre-commit hooks green (they run on the docs commit);
    `scripts/run-quality-gates.sh cargo nextest run --workspace`
    per the discipline (a docs-only commit still triggers hook
    builds — route through the memory gate).

**Prompt 1.1 (planning)**: not dispatched — this document is the
stage-1 output (written and committed by the dispatcher's stage-1
session on `test/flaky-pass-2026-11`).

**Prompt 1.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 1 of the flaky-test triage pass 2026-11-27 ("issue bookkeeping & closures", branch test/flaky-pass-2026-11-closures) in the taskboard repo at /home/viktoria/src/taskboard.

**Long-command discipline** (binding, from docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through `scripts/memory-gate.sh` — nothing bare. (5) This discipline block is restated verbatim in your stage prompts. For your own evidence gathering prefer reading code, CI logs (`gh run view --log-failed`, `gh run list --workflow`), and landed commits; run targeted tests through the memory gate only where reading cannot settle it.

1. Read AGENTS.md (§5, §7, §8, §10), the dispatch plan .artifacts/plans/2026-11-27-flaky-test-triage-pass-plan.md §0–§4 (normative), and docs/agent_dispatch.org.
2. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = STOP and report), then `git -C /home/viktoria/src/taskboard fetch origin --prune` and create the worktree: `git -C /home/viktoria/src/taskboard worktree add -b test/flaky-pass-2026-11-closures ../taskboard-WT-test-flaky-pass-2026-11-closures origin/main` (explicit start point per #1185; if the branch exists, STOP and report). Work ONLY inside that worktree by absolute path; never touch other taskboard-WT-* directories.
3. Verify the operator's rulings on R-1…R-7 (the dispatcher passes them in its dispatch message). Default when a ruling is absent: post the KEEP-WATCHING record for that issue and close nothing.
4. Post the five KEEP-WATCHING comments (#697, #1020, #883, #384, #650) exactly per the plan §2 item 1 — dated, evidence-cited (file:line, run IDs, landed SHAs), capture policy restated verbatim from the standing in-thread policies. One comment per issue, no more.
5. For each granted closure ruling (#926, #968, #1336, #1486, #336, #287): post the closure comment per §2 item 2 (citing the plan §0 entry, the landed fix SHA + date, the clean-window evidence), close as completed, and remove BOTH labels with `gh issue edit <n> --remove-label flaky-test --remove-label watchlist`. For #1486 first run `scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint --test journey_suites us17_s3` and confirm green (the 2026-11-27 00:00 UTC boundary must be behind you); if not green or not runnable, do NOT close — post the KEEP-WATCHING record instead and report.
6. Never remove a label on an issue that stays open; never silently (every label action is named in the closure comment). Never reopen a closed issue.
7. Write .artifacts/reports/2026-11-27-flaky-test-triage-pass-report.md per §2 item 4 (disposition table, scan evidence, per-issue action log, ruling deviations) and commit it as `docs(report): the flaky-test triage pass 2026-11-27 (watchlist #697 #1020 #883 #968 #926 #1336 #1486 #336 #287 #384 #650)` with a body. The diff must contain ONLY this report file.
8. Run the gate per the discipline above (background + poll; flock-deferral rule applies). No push, no PR — shipping is stage 3's last task.
9. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-issue action log (comment posted / closed / labels removed / held by ruling), the gate result (or "gate deferred: lock held by <holder>"), any deviation from the plan and why, backlog issue numbers.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the flaky-test triage pass 2026-11-27 (branch test/flaky-pass-2026-11-closures).

**Long-command discipline** (binding, from docs/agent_dispatch.org): a subagent is killed after ~600s without observable activity, so long silent commands kill working agents. (1) The coverage trio never runs inside a subagent: your gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through `scripts/memory-gate.sh` — nothing bare. (5) This discipline block is restated verbatim in your stage prompts.

**Code-quality mandate** (stage 3): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line; (c) a single level of abstraction per function body. Findings are fixed on the branch.

1. Read AGENTS.md (§5, §7, §8, §10), the dispatch plan .artifacts/plans/2026-11-27-flaky-test-triage-pass-plan.md §0–§4, and the stage-2 report; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-test-flaky-pass-2026-11-closures (recreate on the branch if missing — if its PR landed, use <branch>-v2 per AGENTS §8). Begin by rebasing onto origin/main (fetch --prune first; drop already-merged duplicates; resolve only real conflicts; push --force-with-lease if history was rewritten).
2. Functional focus (this chunk's specific traps): (a) the code diff must be EMPTY except .artifacts/reports/2026-11-27-flaky-test-triage-pass-report.md — any .rs/.toml/.yml/.slint change is a BLOCKER (this pass writes no code, weakens no test, adds no assertion); (b) exactly one new dated comment per each of the 11 watchlist issues — verify on GitHub, not from the report's claims; (c) every closure matches a granted ruling exactly, and every closure comment cites the plan §0 entry + landed fix SHA; (d) flaky-test AND watchlist labels removed on every closed issue, and NO label removed on any still-open issue (#697, #1020, #883, #384, #650 keep both); (e) #1486 was closed only if its us17_s3 green run past the 2026-11-27 00:00 UTC boundary is on record — otherwise it must still be open with a KEEP-WATCHING comment.
3. Re-run the gate yourself per the discipline above; any failure is a finding. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
4. Ship: push the branch, open the PR to main with gh, then report the URL — do NOT watch checks (the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge bookkeeping). main is protected: never merge; the user merges on GitHub.
5. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.
6. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-test-flaky-pass-2026-11-closures (everything is pushed; a resuming round starts a fresh <branch>-v2 branch per AGENTS §8 — never recreates on the landed branch). Confirm with git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, worktree-removed confirmation. Never merge; never touch main.
```

## 5. Completion bookkeeping

- **Closed on granted rulings** (R-1…R-6): #926, #968, #1336,
  #1486, #336, #287 — closure comment cites this plan + landed fix;
  labels removed. **Stay open**: #697, #1020, #883, #384, #650 with
  dated KEEP-WATCHING records.
- **Deliberately not dispatched here**: any FIX/HARDEN work — none
  was warranted by the evidence (§0). #697's armed production-race
  hypothesis, if the next gap-log capture confirms it, becomes a
  production-fix design round (a new plan, not a watchlist patch);
  #650's triage recipe executes when the proptest artifact first
  fires.
- **What opens next**: the next dedicated pass re-runs the label
  query; its entry point is the per-issue KEEP-WATCHING comments
  this chunk lands. #1513 (closed same-day, outside this pass) is
  the pattern to follow for fresh single observations: open,
  root-cause, fix, close — inside one triage pass when the cause is
  immediate.
