# [Plan] Clock-seam remediation — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-06
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 4 chunks of the clock-seam
  remediation: the dedicated flaky-test pass over the watchlist's
  wall-clock class (issues #627/#610/#658/#659/#660). No code in this
  commit.
- **Origin**: implementation plan
  (`.artifacts/plans/2026-11-06-clock-remediation-plan.md`, normative
  §0–§7 — decisions D-1..D-6 and per-chunk details are binding);
  audit report
  (`.artifacts/reports/2026-11-06-clock-seam-audit-report.md`, §4
  inventory, §5 issue reconciliation, §6 batches, §7 rulings); the
  rule as normative doc (`docs/testing_strategy.org` §6.1–6.2,
  `docs/coding_guidelines.org` §2.2, AGENTS.md §5 clock-discipline
  bullet); the user's 2026-11-06 acceptance of all four rulings
  (R1 now-thread: **accepted**; R2 need-driven seam scope:
  **accepted**; R3 single-read capture: **accepted**; R4 StepClock
  replacement: **yes**). This dispatch executes AGENTS §7's dedicated
  flaky-test pass — the audit report is its backlog.
- **Status**: READY — all four chunks are dispatchable; chunk 3 is
  gated on chunk 2's merge (same files:
  `crates/taskboard-ui-slint/src/flow_table_tests.rs` and
  `tests/journeys.rs`); chunks 1, 2, 4 are mutually independent
  (disjoint crates) and may run concurrently as separate sessions.
  Deliberately **not** dispatched here: the C1–C5 latent clusters
  (impl-plan §0 non-goals; one umbrella backlog issue records them at
  completion bookkeeping), any extension of the freeze seam to the
  bypass passes (R2), and any `now`-threading beyond `track::status`
  (R1 scope).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | impl-plan §2 (R1, A2/B4/B5/B6) | `fix/app-track-status-now-thread` | `track::status`/`status_on` now-thread + the #610/t10 rewrites + the cli_timers 09:30 leg + `card_json` future stamp. Files: `commands/track.rs`, `tests/cli_track.rs`, `tests/cli_timers.rs`, `tests/common/mod.rs`. Non-goals: no other handler signatures, no engine-clock changes. | M |
| 2 | impl-plan §3 (A1, B1, B3) | `fix/ui-slint-clock-freeze-pins` | The clock-freeze family: the #627 kiosk test pin, the four B1 straddle pins, the nine golden journeys + `_at` fixture variants + one regen. Files: `src/flow_table_tests.rs` (one test), `tests/golden_visuals.rs`, `tests/support/fixture.rs`, `tests/journeys.rs` (two tests), `src/models.rs` (test module, one helper). Non-goals: no editor-path changes (chunk 3), no seam-scope growth (R2). | L |
| 3 | impl-plan §4 (B2, R3) | `fix/ui-slint-single-read-capture` | Single-read capture for the exact-`LogTime` family: the three flow-table track-form tests + the setter-grammar journey. Non-goals: no seam field for the editor paths (R3), the already-fine call sites untouched. | S |
| 4 | impl-plan §5 (C6, R4) | `test/clock-fixture-hygiene` | Fixed stamps for the nine ungated wall reads across state/sync crates + the `StepClock` → stepping-clock replacement. Non-goals: no production diffs, no new miri gates (the state test stops reading the clock instead). | S |

**Dependency graph**: chunks 1, 2, 4 are order-independent (disjoint
files, all off `main`) — serial or parallel. Chunk 3 is **gated on
chunk 2's merge** (both edit `flow_table_tests.rs` and `journeys.rs`;
chunk 3's stage 1 must re-read the merged helper state, and its stage
3 performs the stacked-PR rebase dance only if dispatched before the
gate clears — default: dispatch last). Default dispatch order:
1 → 2 → 4 → 3.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` next to
the project root (AGENTS §8), branched off the then-current `main`,
no PR ever stacked. **Branch names are never reused after their PR
lands** (AGENTS §8, issue #415): a resume/second round of a chunk
works on `<branch>-v2` (or re-branches off the fetched `origin/main`),
never on the landed branch — the doctor treats any worktree whose
`HEAD` is fully contained in `origin/main` as disposable. Create
worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`).
Disk hygiene (AGENTS §8 worktree lifecycle): any
agent creating or recreating a worktree runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit (free disk below the floor) stops the chunk; stage 3 removes
the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**:
   continues in that worktree (recreates it on the branch if
   missing — if the branch or the worktree path's PR already landed,
   create `<branch>-v2` instead per AGENTS §8; never
   `git worktree add <path> main`), executes per the approved plan — TDD,
   every commit standalone (rebase-merge hygiene), full AGENTS §10
   gate suite green. Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (AGENTS §8's rebase dance:
   drop already-merged duplicate commits; resolve only real
   conflicts), push `--force-with-lease` if the rebase rewrote
   history — then reviews the entire branch diff versus the
   **post-rebase** base against the chunk's acceptance criteria +
   AGENTS rules **plus the mandate below**, re-runs whatever gates
   apply, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its last tasks
   are shipping: push the branch, open the PR to `main` with `gh`,
   watch `gh pr checks --watch` to completion — any failing check is
   a finding; diagnose, fix, push `--force-with-lease`, repeat until
   green — **then verify the PR's merge state**: `gh pr view --json
   mergeStateStatus` must report `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase
   onto `origin/main`, push `--force-with-lease`, let the checks
   re-run, repeat; any other state ⇒ diagnose before handing off.
   **Then teardown**: remove the now-disposable worktree — from the
   project root, `git -C /home/viktoria/src/taskboard worktree remove
   --force ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 —
   never a bare relative path, which resolves against whatever cwd the
   session happens to be in and can name a neighbour's worktree;
   everything precious is pushed; a later round that resumes the chunk
   recreates it on a fresh branch). Confirm via `git worktree list`
   that the removed path was the intended one. The handoff contract is
   "PR open + checks green + merge state `CLEAN` + worktree removed" so
   the
   user's merge needs no additional rebase and the chunk leaves no
   36–55G of build output behind. Reports a verdict. Never merges;
   never touches `main`. (`main` is
   branch-protected — merges happen on GitHub by the user, after
   the required checks pass.)

**Code-quality mandate** (stage 3, every chunk):

- **Implementation chunks — code-quality mandate**: beyond functional
  review, evaluate (a) code quality: naming, comment discipline
  (comments state constraints, not narration), idiomatic use of the
  approved stack, no dead code or speculative generality, test
  quality per AGENTS §5; (b) the Single Responsibility Principle:
  the crate-role boundaries of AGENTS §2 are the SRP line — a
  boundary violation is an SRP violation; (c) a single level of
  abstraction per function body: a function either orchestrates
  named operations or works in primitive data, never mixes. Findings
  are fixed on the branch like any other finding.

## 2. Chunk 1 — the CLI now-thread + app-crate races (`fix/app-track-status-now-thread`)

- **Covers**: impl-plan §2 (normative), audit rows A2/B4/B5/B6 and
  ruling R1. Exactly: `status_on` and `track::status` gain
  `now: DateTime<Utc>` (impl-plan §2.1 lists every caller —
  dispatch site `track.rs:1001`, REPL leg `:1078`, the two test
  helpers); the #610 test passes its own `now` and keeps the `2_000`
  assertion; t10 threads one `now`; the 09:30 timer leg becomes the
  typed time + neighbor-day assertion; `card_json` stamps
  `now + 1 s`. **Non-goals**: no other handler signatures (impl-plan
  §0), no engine-clock changes, no weakening of any assertion
  (D-6), engine-stamped-fixture tests (t1 et al.) untouched.
- **Acceptance**: (a) the rewritten tests contain no wall-clock
  dependency in their asserted values — every time-derived input is
  the test's own captured `now` (stage 3 verifies by reading each
  test, not by running at a chosen hour); (b) `cargo nextest run -p
  taskboard-app` green; (c) full AGENTS §10 gates green; (d) the
  D-F6 doc comment on `status_on` updated to name both `day` and
  `now` as data; (e) issue #610 is *not* closed by the agents — the
  user closes after merge (bookkeeping §5 below).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the clock-seam
remediation dispatch ("the CLI now-thread + app-crate races", branch
fix/app-track-status-now-thread) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 clock-discipline bullet, §8, §10), then the
   normative origins: implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§2
   (decisions D-1..D-6 and the caller list are binding) and audit
   report .artifacts/reports/2026-11-06-clock-seam-audit-report.md
   §4.1 row A2, §4.2 rows B4/B5/B6, §7 ruling R1 (accepted: the
   thread), plus docs/testing_strategy.org §6.1–6.2.
2. Read the current seams you will extend:
   crates/taskboard-app/src/commands/track.rs (status_on :69,
   status :76, dispatch :1001, REPL leg :1078) — enumerate EVERY
   caller of status_on and track::status yourself (grep
   'status_on|track::status' across src+tests) before planning the
   signature; tests/cli_track.rs (run_status :213, the #610 test
   :1320, t10 :590, and the already-fine bracket tests :251 you must
   NOT touch); tests/cli_timers.rs:37-104; tests/common/mod.rs:286
   (card_json) + its consumers (tests/cli_verbs.rs v5 leg :255-292,
   tests/cli_attach.rs:44).
3. Run scripts/worktree-doctor.sh from the project root
   /home/viktoria/src/taskboard first (nonzero exit = free disk below
   the floor: STOP and report instead of planning), then create the
   worktree with git worktree add -b fix/app-track-status-now-thread
   ../taskboard-WT-fix-app-track-status-now-thread off main (never
   without -b and never on main — AGENTS §8 active-worktree safety).
4. Write a detailed TDD-first plan at
   .artifacts/plans/<today>-track-status-now-thread-plan.md settling:
   the exact signature order for status_on/status (now after day),
   each caller's capture point, whether the #610 expectation stays
   hardcoded 2_000 (preferred, impl-plan §2.1) or is derived — with
   the reason; the typed 09:30 assertion shape (NaiveTime equality +
   neighbor-day set, NO text assertions); the card_json +1s comment;
   and the test catalogue: which tests change, which must not
   (engine-stamped fixtures keep brackets).
5. Commit it as: docs(plan): the track status now-thread. Do not
   implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the
clock-seam remediation dispatch ("the CLI now-thread + app-crate
races").

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§2
   (binding), the dispatch plan
   .artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md
   §1/§2, and the stage-1 plan
   (.artifacts/plans/*track-status-now-thread*-plan.md) in the
   worktree ../taskboard-WT-fix-app-track-status-now-thread (branch
   fix/app-track-status-now-thread). Recreate the worktree on that
   branch if missing.
2. Implement exactly per the approved plan, TDD: thread now through
   status_on + track::status (every caller from the stage-1
   enumeration), rewrite the #610 and t10 tests to own their now,
   the cli_timers 09:30 leg typed assertion, the card_json strictly-
   future stamp. Hard rules: no text assertions (typed NaiveTime /
   neighbor-day sets only), no weakening of the 2_000 / respite
   semantics, engine-stamped tests untouched, every commit
   standalone (rebase-merge hygiene: fix/ui-slint or fix(app)
   subjects, fix(...) for the thread commit with the D-F6 rationale
   in the body).
3. Run the full gate suite from AGENTS.md §10 via
   scripts/run-quality-gates.sh (shared box — never raw parallel
   suites) and make it green.
4. Bookkeeping: none in this chunk — do NOT comment on or close any
   GitHub issue (the user does at merge; dispatch plan §5).
5. No push, no PR — shipping is stage 3's last task. Never push to
   or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the clock-seam
remediation dispatch (branch fix/app-track-status-now-thread).

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§2,
   the dispatch plan
   .artifacts/plans/2026-11-06-clock-remediation-dispatch-plan.md
   §1/§2 (the code-quality mandate applies to you), and the stage-1
   plan; then review the full branch diff (git diff main...HEAD)
   after first rebasing onto origin/main per the dispatch plan's
   stage-3 protocol.
2. Functional focus — this chunk's traps: (a) verify by READING each
   rewritten test that its asserted values contain no wall-clock
   input — every time-derived value must trace to the test's own
   captured now (the acceptance is structural, not "it passes
   now"); (b) the 2_000 assertion's semantics must be intact (work
   counts, respite does not — production clipping untouched); (c)
   the now-thread must not have introduced a mid-flight re-read
   (one capture per invocation — check the dispatch table site and
   the REPL leg each capture exactly once); (d) the 09:30 assertion
   must be typed (no format-string text matches); (e) card_json's
   future stamp must not trip any skew logic (the merge path uses
   the actor's injected read_at, not the echo — audit §2).
3. Code-quality mandate: beyond functional review, evaluate (a)
   code quality: naming, comment discipline (comments state
   constraints, not narration), idiomatic use of the approved
   stack, no dead code or speculative generality, test quality per
   AGENTS §5; (b) the Single Responsibility Principle: the
   crate-role boundaries of AGENTS §2 are the SRP line — a boundary
   violation is an SRP violation; (c) a single level of abstraction
   per function body: a function either orchestrates named
   operations or works in primitive data, never mixes. Findings are
   fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10, via
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr
   checks --watch; any failing check is a finding — diagnose, fix,
   push --force-with-lease, repeat until all checks are green. main
   is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-fix-app-track-status-now-thread (everything is
   pushed; a resuming round starts a fresh -v2 branch per AGENTS
   §8). Confirm with git worktree list that the removed path was the
   intended one.

Report: verdict, findings (functional + mandate), what you fixed,
PR URL, green-checks confirmation, worktree-removed confirmation.
Never merge; never touch main.
```

## 3. Chunk 2 — the clock-freeze family in ui-slint (`fix/ui-slint-clock-freeze-pins`)

- **Covers**: impl-plan §3 (normative), audit rows A1/B1/B3. Exactly:
  the #627 kiosk test (fixed-instant `closed_entry_at` sibling helper
  + `UiSettings { kiosk: true, clock: Some(PINNED_NOW) }` + the
  09:00–10:00 / 11:00–12:00 UTC stamps at PINNED_NOW =
  2026-11-06T12:00Z); the four B1 pins (journeys :13761/:13677,
  flow_table :3610, models `tracked()` — pin or single-read per the
  rig's shape, recorded); the nine golden journeys + the
  `golden_state_at`/`today_overdue_state_at` fixture variants + the
  missing `pin_utc_zone()` in the focus-overlay golden; one regen
  (`TASKBOARD_REGEN_GOLDENS=1`), then a compare-green run.
  **Non-goals**: no editor-path changes (chunk 3's tests), no
  production diffs of any kind (the seam exists), no other
  `closed_entry_on` call-site conversions, no assertion weakening
  (the rows/`vec![task_two]`/"1 h" semantics stay exact).
- **Acceptance**: (a) the eleven touched tests + the untouched
  review-ledger/empty goldens pass under the D-5 TZ matrix (`TZ=UTC`,
  `TZ=Pacific/Kiritimati`, `TZ=Etc/GMT+12`) — structural zone
  independence; (b) the regen diff is confined to chip/row strips
  (stage 3 eyeballs the rendered PNGs on the PR); (c) `cargo nextest
  run -p taskboard-ui-slint` green; (d) full §10 gates green; (e) a
  grep of the touched tests shows no remaining `Utc::now()`/`Local::
  now()` in their bodies (the pinned constant is the only time
  source).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the clock-seam
remediation dispatch ("the clock-freeze family in ui-slint", branch
fix/ui-slint-clock-freeze-pins) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md (§5 clock-discipline bullet, §8, §10), then the
   normative origins: implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§3
   (D-1 the pinned instant, D-4 the regen, D-5 the TZ matrix are
   binding; the per-journey seed table is the work catalogue),
   audit report §4.1 row A1, §4.2 rows B1/B3, and
   docs/testing_strategy.org §6.1–6.2 (the decision rule you are
   applying).
2. Read the current seams you will extend: the #660 template —
   tests/golden_visuals.rs:833-885 (the pinned-instant shape) and
   src/lib.rs:97-104 + src/adapter.rs:966-970 (what the seam freezes
   — content via day_ctx, never cadence); the fixtures
   tests/support/fixture.rs:340-360 (golden_state) and :460-500
   (today_overdue_state); the B1 tests' current shapes
   (tests/journeys.rs:13677/:13761 with the self-recorded race
   :13912-13915; src/flow_table_tests.rs:3610; src/models.rs:3894
   — decide pin vs single-read for the models rig by reading how its
   DayCtx is built, and record the choice); the helper you will
   sibling (closed_entry_on, src/flow_table_tests.rs:2806) and its
   five call sites (convert ONLY :3556-3557's test).
3. Run scripts/worktree-doctor.sh from the project root
   /home/viktoria/src/taskboard first (nonzero exit: STOP and
   report), then create the worktree with git worktree add -b
   fix/ui-slint-clock-freeze-pins
   ../taskboard-WT-fix-ui-slint-clock-freeze-pins off main.
4. Write a detailed TDD-first plan at
   .artifacts/plans/<today>-ui-slint-clock-freeze-pins-plan.md
   settling: the PINNED_NOW constant's home (shared per test binary
   where practical), the closed_entry_at signature, the exact seed
   stamps for the #627 test (entries strictly inside the frozen
   window and fully counted at the frozen now — the 09:00-10:00 /
   11:00-12:00 shape from impl-plan §3.1), the models-rig decision,
   the nine journeys' seed re-relativization list, and the regen +
   TZ-matrix verification procedure.
5. Commit it as: docs(plan): the ui-slint clock-freeze pins. Do not
   implement; do not push.

Report: plan path, decisions, risks, estimated diff size (PNG count
included).
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the
clock-seam remediation dispatch ("the clock-freeze family in
ui-slint").

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§3
   (binding), the dispatch plan §1/§3, and the stage-1 plan
   (.artifacts/plans/*ui-slint-clock-freeze-pins*-plan.md) in the
   worktree ../taskboard-WT-fix-ui-slint-clock-freeze-pins (branch
   fix/ui-slint-clock-freeze-pins). Recreate the worktree on that
   branch if missing.
2. Implement exactly per the approved plan, TDD: the _at fixture
   variants first (golden_state_at / today_overdue_state_at — the
   no-arg forms delegate with Utc::now() and their journey consumers
   stay unchanged), then the eleven tests (A1, the four B1 pins, the
   nine goldens — nine includes the two already touched by nothing
   else), then ONE regen: TASKBOARD_REGEN_GOLDENS=1 cargo nextest
   run -p taskboard-ui-slint --test golden_visuals (it fails by
   design after rewriting), then a plain run green. Hard rules: no
   production diffs, no assertion weakening, comments cite the rule
   (docs/testing_strategy.org §6.2) and the issue, every commit
   standalone, the PNG regen is its own commit.
3. Verify per D-5: the touched tests green under TZ=UTC,
   TZ=Pacific/Kiritimati, TZ=Etc/GMT+12 (nextest per-process TZ is
   safe — the zone pin's doc). Then the full gate suite via
   scripts/run-quality-gates.sh, green.
4. Bookkeeping: none — do NOT comment on or close GitHub issues.
5. No push, no PR — shipping is stage 3's last task. Never push to
   or merge main.

Report: per-gate results, the TZ-matrix results, the regen's PNG
list, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the clock-seam
remediation dispatch (branch fix/ui-slint-clock-freeze-pins).

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§3,
   the dispatch plan §1/§3 (the code-quality mandate applies to
   you), and the stage-1 plan; then review the full branch diff (git
   diff main...HEAD) after first rebasing onto origin/main per the
   dispatch plan's stage-3 protocol.
2. Functional focus — this chunk's traps: (a) the regen diff is the
   review artefact: open the changed PNGs on the PR and confirm the
   pixel changes are confined to the tracking-chip strip (and the
   notification golden's alarm row) — any unexplained region is a
   finding; (b) the #627 test's entries must lie strictly inside the
   frozen window AND be fully counted at the frozen now (an entry
   ending after the frozen now would clip and break the 1h phrase —
   read the stamps); (c) the _at variants must not have leaked into
   the structural-only journey consumers (golden_state's no-arg form
   still exists and still stamps now-12m); (d) no production diffs
   (the entire diff is tests + PNGs + comments); (e) the untouched
   closed_entry_on call sites are genuinely untouched; (f) re-run
   the D-5 TZ matrix yourself on the touched binaries.
3. Code-quality mandate: beyond functional review, evaluate (a)
   code quality: naming, comment discipline (comments state
   constraints, not narration), idiomatic use of the approved
   stack, no dead code or speculative generality, test quality per
   AGENTS §5; (b) the Single Responsibility Principle: the
   crate-role boundaries of AGENTS §2 are the SRP line — a boundary
   violation is an SRP violation; (c) a single level of abstraction
   per function body: a function either orchestrates named
   operations or works in primitive data, never mixes. Findings are
   fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10, via
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr
   checks --watch; any failing check is a finding — diagnose, fix,
   push --force-with-lease, repeat until all checks are green and
   mergeStateStatus is CLEAN. main is protected: never merge.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-fix-ui-slint-clock-freeze-pins; confirm with git
   worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), the PNG-diff
verdict, what you fixed, PR URL, green-checks confirmation,
worktree-removed confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — single-read capture on the editor paths (`fix/ui-slint-single-read-capture`)

- **Precondition**: chunk 2's PR is merged (`git log origin/main`
  shows the clock-freeze-pins commits); otherwise STOP and report.
- **Covers**: impl-plan §4 (normative), audit row B2, ruling R3.
  Exactly: the three exact-`LogTime` flow-table tests
  (`:2933`, `:2852`, `:3260`) and the setter-grammar journey
  (`:10936`) each collapse to ONE `Local::now().date_naive()` capture
  feeding both seeds and expectations (helper gains a date parameter
  or a dated sibling); each test carries the residual-race comment
  (the adapter's own `Local::today` read stays — accepted ms-at-
  local-midnight residual per R2/R3). **Non-goals**: no seam field,
  no production diffs, the already-fine call sites (`:3149`,
  `:3420`) untouched beyond what the shared helper refactor needs.
- **Acceptance**: each rewritten test contains exactly one clock read
  (grep-verifiable); expectations derive from that capture; `cargo
  nextest run -p taskboard-ui-slint` green; full §10 gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the clock-seam
remediation dispatch ("single-read capture on the editor paths",
branch fix/ui-slint-single-read-capture) in the taskboard repo at
/home/viktoria/src/taskboard.

Precondition: chunk 2 (fix/ui-slint-clock-freeze-pins) must be
merged into origin/main — verify (git fetch origin --prune; git log
origin/main --oneline | head -20 shows its commits); if it is not,
STOP and report instead of planning.

1. Read AGENTS.md (§5 clock-discipline bullet, §8, §10), then the
   normative origins: implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§4
   (D-3 the single-read shape is binding; the residual-race comment
   requirement is binding), audit report §4.2 row B2, ruling R3.
2. Read the tests you will rewrite in their POST-CHUNK-2 state:
   src/flow_table_tests.rs (closed_entry_on — chunk 2 may have
  added a sibling; the three LogTime tests :2933/:2852/:3260) and
   tests/journeys.rs:10936 (the setter grammar, expectation
   :10998-11006); note the adapter reads that remain unshared
   (adapter.rs:1570/1618/1302) — the residual you must name in each
   test's comment, not fix.
3. Run scripts/worktree-doctor.sh from the project root first
   (nonzero exit: STOP and report), then create the worktree with
   git worktree add -b fix/ui-slint-single-read-capture
   ../taskboard-WT-fix-ui-slint-single-read-capture off main.
4. Write a detailed TDD-first plan at
   .artifacts/plans/<today>-ui-slint-single-read-capture-plan.md
   settling: the helper's dated parameter shape, per-test where the
   single capture lives, and the exact residual comment wording.
5. Commit it as: docs(plan): the editor-path single-read capture. Do
   not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the
clock-seam remediation dispatch ("single-read capture on the editor
paths").

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§4
   (binding), the dispatch plan §1/§4, and the stage-1 plan
   (.artifacts/plans/*ui-slint-single-read-capture*-plan.md) in the
   worktree ../taskboard-WT-fix-ui-slint-single-read-capture (branch
   fix/ui-slint-single-read-capture). Recreate the worktree on that
   branch if missing.
2. Implement exactly per the approved plan: one Local::today capture
   per test feeding seeds AND expectations; the dated helper
   parameter; the residual-race comment at each test naming the
   adapter's unshared read and the accepted ms-at-local-midnight
   window (R2/R3). Hard rules: no production diffs, no assertion
   weakening, the already-fine call sites untouched, every commit
   standalone.
3. Run the full gate suite via scripts/run-quality-gates.sh and
   make it green.
4. Bookkeeping: none — do NOT comment on or close GitHub issues.
5. No push, no PR — shipping is stage 3's last task. Never push to
   or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the clock-seam
remediation dispatch (branch fix/ui-slint-single-read-capture).

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§4,
   the dispatch plan §1/§4 (the code-quality mandate applies to
   you), and the stage-1 plan; then review the full branch diff (git
   diff main...HEAD) after first rebasing onto origin/main per the
   dispatch plan's stage-3 protocol.
2. Functional focus — this chunk's traps: (a) count the clock reads
   per rewritten test: exactly one (a grep of the test body for
   'Local::now|Utc::now' must show a single hit, the capture); (b)
   every expectation must visibly derive from that capture — a
   second derivation from a fresh read is the bug class this chunk
   exists to remove; (c) the residual comments must name the
   adapter's unshared read, not hand-wave; (d) no production diffs;
   (e) the exact-LogTime assertions' semantics unchanged (same
   instants, same tolerance).
3. Code-quality mandate: beyond functional review, evaluate (a)
   code quality: naming, comment discipline (comments state
   constraints, not narration), idiomatic use of the approved
   stack, no dead code or speculative generality, test quality per
   AGENTS §5; (b) the Single Responsibility Principle: the
   crate-role boundaries of AGENTS §2 are the SRP line — a boundary
   violation is an SRP violation; (c) a single level of abstraction
   per function body: a function either orchestrates named
   operations or works in primitive data, never mixes. Findings are
   fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10, via
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr
   checks --watch; any failing check is a finding — diagnose, fix,
   push --force-with-lease, repeat until all checks are green and
   mergeStateStatus is CLEAN. main is protected: never merge.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-fix-ui-slint-single-read-capture; confirm with
   git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed,
PR URL, green-checks confirmation, worktree-removed confirmation.
Never merge; never touch main.
```

## 5. Chunk 4 — fixture hygiene below the UI (`test/clock-fixture-hygiene`)

- **Covers**: impl-plan §5 (normative), audit row C6, ruling R4.
  Exactly: `state/tests/engine.rs:1504` fixed stamp (clears the §10
  violation in a Miri-scoped binary — do NOT add a miri gate; the
  read disappears); `sync-nextcloud/src/push_exec/tests.rs:46/:103/
  :122/:156` fixed `ts()`-style stamps (local helper if none);
  `sync-files/src/identity.rs:96` fixed RFC-3339 literal (sibling
  pattern `:57`); `sync-files-nextcloud/src/device.rs:139/:156/:175/
  :191` fixed-literal `FixedClock`; **R4**: `sync-files-nextcloud/
  src/actor.rs:205-214` `StepClock` becomes an atomic fetch-add
  stepping clock (mirror `SteppingClock`,
  `taskboard-sync-nextcloud/tests/common/mod.rs:43-68`), doc comment
  corrected. **Non-goals**: no production diffs (all sites are
  test-module code), no live/docker-tier changes, no assertion
  changes (the values were never observed).
- **Acceptance**: the impl-plan §5 grep returns zero inline-test
  `Utc::now()` hits at the named sites; each crate's suite green;
  full §10 gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the clock-seam
remediation dispatch ("fixture hygiene below the UI", branch
test/clock-fixture-hygiene) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md (§5, §8, §10) and docs/testing_strategy.org §10
   ("fixture builders must not read the clock" — the rule this chunk
   enforces), then the normative origin: implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§5,
   audit report §4.3 row C6.
2. Read every site: state/tests/engine.rs:1504 (and the common ts()
   helper it should use), sync-nextcloud/src/push_exec/tests.rs:46/
   :103/:122/:156, sync-files/src/identity.rs:96 (+ the sibling
   fixed-literal pattern :57), sync-files-nextcloud/src/device.rs
   :139/:156/:175/:191 (+ the asserting sibling :124), and
   sync-files-nextcloud/src/actor.rs:181-214 (StepClock + the
   SteppingClock mirror at sync-nextcloud/tests/common/mod.rs:43-68)
   — confirm each site's value is truly unobserved (the audit's
   claim; verify by reading the consuming assertions).
3. Run scripts/worktree-doctor.sh from the project root first
   (nonzero exit: STOP and report), then create the worktree with
   git worktree add -b test/clock-fixture-hygiene
   ../taskboard-WT-test-clock-fixture-hygiene off main.
4. Write a detailed TDD-first plan at
   .artifacts/plans/<today>-clock-fixture-hygiene-plan.md settling:
   each site's replacement stamp value (fixed, distinct where
   ordering could ever matter), the ts-helper question in
   push_exec, and the StepClock replacement's exact shape (atomic
   base + step; keep or rename the type — record the choice).
5. Commit it as: docs(plan): the clock fixture hygiene. Do not
   implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the
clock-seam remediation dispatch ("fixture hygiene below the UI").

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§5
   (binding), the dispatch plan §1/§5, and the stage-1 plan
   (.artifacts/plans/*clock-fixture-hygiene*-plan.md) in the worktree
   ../taskboard-WT-test-clock-fixture-hygiene (branch
   test/clock-fixture-hygiene). Recreate the worktree on that branch
   if missing.
2. Implement exactly per the approved plan: the fixed stamps, the
   StepClock stepping replacement with an honest doc comment. Hard
   rules: test-module code only (verify each hunk sits inside
   #[cfg(test)] or a tests/ file), no assertion changes, no new
   miri gates, every commit standalone.
3. Run the full gate suite via scripts/run-quality-gates.sh and
   make it green.
4. Bookkeeping: none — do NOT comment on or close GitHub issues.
5. No push, no PR — shipping is stage 3's last task. Never push to
   or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the clock-seam
remediation dispatch (branch test/clock-fixture-hygiene).

1. Read AGENTS.md, the implementation plan
   .artifacts/plans/2026-11-06-clock-remediation-plan.md §0/§1/§5,
   the dispatch plan §1/§5 (the code-quality mandate applies to
   you), and the stage-1 plan; then review the full branch diff (git
   diff main...HEAD) after first rebasing onto origin/main per the
   dispatch plan's stage-3 protocol.
2. Functional focus — this chunk's traps: (a) run the impl-plan §5
   grep yourself — zero inline-test Utc::now() hits at the named
   sites; (b) every hunk is test-module code (a production-hunk is
   a finding); (c) the StepClock replacement must be deterministic
   (atomic fetch-add from a fixed base — no chrono read anywhere in
   it) and its doc comment must now be true; (d) no assertion
   changed anywhere in the diff; (e) the state test must NOT have
   gained a cfg_attr(miri, ignore) — the read disappears, the gate
   was never needed.
3. Code-quality mandate: beyond functional review, evaluate (a)
   code quality: naming, comment discipline (comments state
   constraints, not narration), idiomatic use of the approved
   stack, no dead code or speculative generality, test quality per
   AGENTS §5; (b) the Single Responsibility Principle: the
   crate-role boundaries of AGENTS §2 are the SRP line — a boundary
   violation is an SRP violation; (c) a single level of abstraction
   per function body: a function either orchestrates named
   operations or works in primitive data, never mixes. Findings are
   fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10, via
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr
   checks --watch; any failing check is a finding — diagnose, fix,
   push --force-with-lease, repeat until all checks are green and
   mergeStateStatus is CLEAN. main is protected: never merge.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force ../taskboard-WT-test-clock-fixture-hygiene;
   confirm with git worktree list that the removed path was the
   intended one.

Report: verdict, findings (functional + mandate), what you fixed,
PR URL, green-checks confirmation, worktree-removed confirmation.
Never merge; never touch main.
```

## 6. Completion bookkeeping

- **Issues the user closes at merge** (agents never close them in
  the chunks): #610 with chunk 1's PR reference (the determinism
  argument is impl-plan §2.1); #627 with chunk 2's PR reference
  (the pin rationale is impl-plan §3.1). Suggested close comments
  cite the audit report §5 reconciliation.
- **Already-fixed siblings**: #658, #659, #660 each get one closing
  comment referencing the audit report §5 (the framework explains
  the chosen tool for each) — landed by any chunk's stage 3 as a
  comment only, or by the user; default owner: chunk 2's stage 3
  after its PR is green (it is the seam-family chunk).
- **One backlog issue opened at completion** (not before — the
  deferred items only become backlog once their owning chunks
  merged): `backlog: deferred latent clock-drift sites from the
  clock-seam audit` — body: summary + permalink to audit report
  §4.3 (C1–C5); labels `testing` + `p3`. C1's row explicitly records
  R2's need-driven trigger: reopen the seam-scope question when an
  exact-artifact test renders the alarm window, the notice line, or
  editor validity.
- **Deliberately not dispatched here**: the C1–C5 clusters (above);
  any `now`-threading beyond `track::status` (impl-plan §0); the
  live/docker-tier wall reads (deliberate, tier-gated).
- **What opens next**: nothing — this dispatch retires the
  wall-clock class from the flaky watchlist. The watchlist's
  remaining open items (#650, #570, #654, #521, #384, #336, #287)
  are confirmed non-clock-class (audit §5) and keep their existing
  lanes.
