# [Plan] Backlog Round 2 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-31
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **27** chunks of the commissioned
  backlog round 2, grouped into **6 batches**. Chunks within a batch
  are parallel-safe under the lane rules of §1 and dispatch greedily
  in listed order; batches are sequential (a batch begins when every
  PR of the previous batch is merged). **Every chunk carries its
  three prompts fully fleshed out for direct copy-paste dispatch —
  one agent session per prompt, nothing summarized** (user
  requirement, 2026-10-31). No code in this commit.
- **Origin**: `.artifacts/reports/2026-10-31-backlog-readiness-triage-report.md`
  (the triage the user reviewed, §8 commissioning record) — plus the
  user's 2026-10-31 rulings: **F-133 Option A confirmed** (recorded on
  #480); §4 picks — include #352, the time-tracking fences (#490,
  #491, #492, #493, #495, #496), #555, #412 (only UI feature of
  #351/#411/#412), #214, #350, #116, #194; leave out the DR-6
  follow-ups (#504/#505/#506) and #351/#411; everything else per the
  report's ready-now tiers. Per-chunk normative origins are the GitHub
  issue bodies plus the design plans they cite: the DR-3 reminders
  surface pair plan (`.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`)
  for #352, the DR-4/S landed stage-1 plan
  (`.artifacts/plans/2026-10-26-sync-skew-pins-plan.md`) for #480, the
  DR-5 time-tracking plan (`.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`)
  for the fences, the ListView design-round report
  (`.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`)
  and the view-pinned re-measure
  (`.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`)
  for the render lane, and the v0.1.0 audit crate/theme reports for
  the cleanup chunks.
- **Status**: **READY** — batch 1 is dispatchable immediately. One
  chunk is a **resume** (#480: its stage-1 plan is landed on `main`
  as `1fec235a`; its stage-2 implementation commit `44c1fa71` sits
  unlanded on branch `test/sync-skew-pins` — the branch was paused on
  the F-133 ruling, which is now recorded; the branch is *not* fully
  contained in `origin/main`, so resuming it is not a #415
  name-reuse). **Deliberately not dispatched here**: the report's
  wait classes (§5 of the report — external, trigger-gated,
  design-round-gated), the user-skipped #351/#411/#504/#505/#506, and
  the DR-4 recorded extensions (symptom-triggered skew guard D-4c —
  now confirmed deferred by the F-133 ruling; conflict surfacing
  D-1f; baseline scope D-1e).
- **Environment note (2026-10-31)**: the project root
  `/home/viktoria/src/taskboard` is now a symlink onto a relocated
  partition (`/home/viktoria/nextcloud/viktoria/programming/taskboard`);
  the user has ruled this transparent. All house paths remain
  `/home/viktoria/src/...`-based; worktrees still land as siblings of
  the symlink (`/home/viktoria/src/taskboard-WT-<branch>`), and the
  AGENTS §8 absolute-anchor teardown keeps working through the
  symlink. Agents verify nothing about the physical location.

## 1. Batches, chunks & ordering

| Batch | Chunk | Issue(s) | Branch | Covers (one line) | Size |
|---|---|---|---|---|---|
| 1 | 1.1 | #448 + #447 + #449 (+ rider #282; closes into #436) | `fix/anchor-filename-fallback` | anchor-scan filename fallback per Deck 1.19.1 wire shape; C4 flush triage; dead-board verify on the rig | M |
| 1 | 1.2 | #480 (**resume**) | `test/sync-skew-pins` | skew interleaving pins + Clock-seam demo (stage-2 output exists; F-133 ruled) | S |
| 1 | 1.3 | #463 | `perf/ui-unchanged-refresh-guard` | O(1) `Arc::ptr_eq` guard atop the refresh pass | S |
| 1 | 1.4 | #458 | `refactor/scheduled-phrase-typed-direction` | typed Future/Past/Now × bucket kills the direction-flip mutant | S |
| 1 | 1.5 | #450 | `fix/worktree-doctor-never-pushed` | doctor spares never-pushed branches; false-positive check | S |
| 1 | 1.6 | #194 | `feat/ipc-disable-knob` | `[ipc] enabled` + `daemon --no-ipc`; documented `no-daemon` degradation | S |
| 1 | 1.7 | #407 | `fix/tasks-edit-flag-group` | clap at-least-one-flag groups on tasks/labels edit | S |
| 1 | 1.8 | #116 | `ci/nextcloud-version-matrix` | oldest-supported Nextcloud matrix leg for the docker tier | S/M |
| 1 | 1.9 | #555 | `feat/undo-tracking-habit-arms` | invertible tracking/habit arms join the inverse-synthesis catalogue | M |
| 1 | 1.10 | #521 + #524 + #549 (+ dispositions #287/#336/#384) | `test/pty-flake-pass` | the three sibling pty outer-timeout flakes; old-watchlist dispositions | M |
| 2 | 2.1 | #464 | `perf/ui-row-diff-mirror` | Rust-side diff mirror replaces clone-per-row `row_data` | M |
| 2 | 2.2 | #270 + #271 | `chore/audit-cleanups-domain-state` | domain naming/rule-consts/is_noop; state fake/Seams cleanups (re-verified) | M |
| 2 | 2.3 | #272 + #273 | `chore/audit-cleanups-storage-sync` | storage small fixes; sync naming (re-verified) | M |
| 2 | 2.4 | #412 | `feat/window-geometry-persistence` | main-window geometry survives restarts | M |
| 2 | 2.5 | #352 | `feat/timers-alarm-surface` | timers composition + fired-alarm ack surface per DR-3 | M |
| 3 | 3.1 | #462 (+ rider #536) | `fix/kiosk-board-i16-overflow` | board virtualisation/clip kills the i16 panic; shared upstream slint report | M |
| 3 | 3.2 | #401 | `feat/appdata-unreadable-split` | `AppDataUnreadable` 4xx/5xx evidence split | S |
| 3 | 3.3 | #201 | `feat/delivery-proof-pruning` | age-based pruning of delivery rows at record time | S/M |
| 3 | 3.4 | #490 + #491 | `feat/tracked-time-surfaces` | today "Tracked" totals section; task-detail entry history | M |
| 3 | 3.5 | #493 + #495 | `feat/respite-break-reminders` | respite entry kind; tracked-duration break reminders riding it | M |
| 3 | 3.6 | #276 + #279 + #280 | `chore/app-hardening-contracts` | app cleanups; shutdown hardening bundle; pin publish-order + IPC match (re-verified) | M |
| 4 | 4.1 | #461 | `perf/ui-today-virtualisation` | Today section lists via the #373 ListView mechanism + anchor-on-removal | M |
| 4 | 4.2 | #492 | `feat/tracker-task-kind` | never-done tracker kind over time entries | S/M |
| 4 | 4.3 | #350 | `fix/sidecar-cross-board-repair` | sidecar document migration on cross-board card adoption | M |
| 5 | 5.1 | #496 | `feat/pomodoro-focus` | focus-session runtime + concentration overlay | M |
| 5 | 5.2 | #275 | `chore/ui-slint-cleanups` | ui dead-fn/seam cleanups (re-verified) | S |
| 6 | 6.1 | #214 | `feat/effort-calibration-surfaces` | estimate-vs-actual projections; the today slot decision per the ADHD plan | M |

**Single-flight lanes and gates (binding when picking parallel sets
within a batch):**

1. **The diff-machinery lane (hard).** Chunks changing the existing
   refresh/diff mechanics — 1.3 (#463), 2.1 (#464), 3.1 (#462),
   4.1 (#461) — are strictly serial, in that order; the batch table
   already spreads them one per batch. Chunks that merely *add* new
   push sections (#352, #490/#491, #496, #214) may run beside the
   lane but must expect rebase friction in `models.rs` and follow
   the §2 rebase dance.
2. **UI territory rule (soft).** Two ui-slint chunks may share a
   batch only with disjoint view territories: board
   (`board-layout.slint`/`push_board`) vs. today
   (`today-view.slint`/`push_today`) vs. task detail vs.
   settings/window vs. chrome/status bar vs. the pure modules
   (`presentation.rs`/`resolver.rs`/`view_state.rs` — pure-module
   chunks don't count against the ui budget). The batches above are
   composed to satisfy this; if reality diverges, the later chunk
   slips one batch.
3. **Same-crate gates.** Sync-crate chunks: 1.1 → 2.3 → 3.2 → 4.3
   (never two in flight). Domain/state chunks: 1.9 → 2.2 → 3.3/3.5 →
   4.2 (1.2 is test-only and exempt). App `cli.rs`/`route_for_ipc`:
   at most one of 1.6/1.7 in flight (different files — allowed) and
   none beside 3.6.
4. **Migrations.** None of the 27 chunks is expected to carry a
   schema migration (the fences fold additively per the DR-5 plan's
   wire-shape reservations). If a stage 1 proves otherwise, it takes
   the next free number (**0018**) at its rebase, and only one
   migration-carrying chunk may be in flight.
5. **Batch protocol** (from the design-rounds-batch exemplar): a
   batch begins when every PR of the previous batch is merged (the
   user merges on GitHub; `main` is branch-protected). Before
   dispatching any chunk, re-check its gates against the *actual*
   `origin/main` (`git fetch origin --prune && git log origin/main
   --oneline | head -60`); a not-yet-merged gate ⇒ STOP and report.
   Free disk bounds actual concurrency: each worktree costs
   36–55G of build output; `scripts/worktree-doctor.sh` is the gate
   and a nonzero exit stops the dispatch.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` (slashes as dashes)
next to the project root (AGENTS §8), branched off the then-current
`main`, no PR ever stacked. **Branch names are never reused after
their PR lands** (AGENTS §8, issue #415): a resume/second round works
on `<branch>-v2` or re-branches off the fetched `origin/main` — never
on the landed branch. The one deliberate exception is chunk 1.2,
which *resumes* an unlanded branch (see its section). Create worktrees
with `git worktree add -b <branch> ../taskboard-WT-<branch>` (never
without `-b`, never on `main`). Disk hygiene (AGENTS §8): any agent
creating or recreating a worktree runs `scripts/worktree-doctor.sh`
from the main checkout first — nonzero exit (free disk below the
floor) stops the chunk; stage 3 removes the worktree as its final
handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: runs the doctor (nonzero exit ⇒
   STOP and report), creates the worktree on a new branch off the
   current `main`, verifies the chunk's gates against `origin/main`
   (gated chunks: unmerged gate ⇒ STOP and report), writes the
   TDD-first plan at `.artifacts/plans/<today>-<topic>-plan.md` in
   the house `[Plan]` style, resolving every open question with a
   recorded decision (deviations from the origin need a recorded
   reason), commits it (`docs(plan): …`). No implementation, no push.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch's PR
   already landed, use `<branch>-v2` per AGENTS §8), implements per
   the approved plan — TDD, every commit standalone (rebase-merge
   hygiene), full AGENTS §10 gate suite green, `CHANGELOG.md`
   Unreleased entries where behavior is user-observable. Does **not**
   push or open a PR. Reports deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** (`git fetch origin --prune`;
   rebase onto `origin/main`, dropping already-merged duplicate
   commits; resolve only real conflicts; push `--force-with-lease`
   if history was rewritten), then reviews the full branch diff
   versus the **post-rebase** base against the chunk's acceptance
   criteria + AGENTS rules + the mandate below, re-runs the full
   gate suite (any failure is a finding), and fixes every finding on
   the branch (amend/reword — never duplicate-subject fixups). Then
   ships: push, open the PR to `main` with `gh`,
   `gh pr checks --watch` to green (any failing check is a finding;
   fix, push `--force-with-lease`, repeat), verify
   `gh pr view --json mergeStateStatus` = `CLEAN` (`BEHIND`/`DIRTY`
   ⇒ rebase, push, re-run). **Then teardown**: from the project root,
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8; confirm
   via `git worktree list`). Handoff contract: "PR open + checks
   green + merge state `CLEAN` + worktree removed". Never merges;
   never touches `main`.
4. **Issue closures** land as stage-3 bookkeeping after the PR is
   green: one close comment per issue, self-contained (evidence +
   fixing commit/PR).

**Code-quality mandate** (stage 3, every chunk): beyond functional
review — (a) code quality: naming, comment discipline (comments state
constraints, not narration), idiomatic approved stack, no dead code
or speculative generality, test quality per AGENTS §5 (no text
assertions, no sleeps, deterministic by construction, no quality
ceilings, a failing new test may be a production bug and may not be
weakened); (b) SRP: the crate-role boundaries of AGENTS §2 are the
SRP line — domain pure, engine orchestrating, sync actor only
syncing, CLI parsing/dispatching, UI a dumb presentation client; a
boundary violation is an SRP violation; (c) a single level of
abstraction per function body. Findings are fixed on the branch.

**Evidence & artefact mandate** (chunk 3.1's upstream report and any
chunk producing a report artefact): every factual claim carries
evidence (command + observed outcome, or verifiable source + what it
states); experiments reproducible from the method section; exactly
one decision-grade recommendation; scratch never lands — the PR diff
is exactly the chunk's artefacts.

## 2. Batch 1 — open field (nine+one parallel chunks)

All of batch 1 is dispatchable immediately; chunks are mutually
parallel under §1's lanes (1.3 owns the render lane's first step;
1.1 owns the sync crate; 1.9 owns the domain/state source; 1.2 is
test-only).

### Chunk 1.1 — anchor filename fallback (#448 + #447 + #449, rider #282, closes into #436) (`fix/anchor-filename-fallback`)

- **Covers** (normative): #448 (p1 — `normalize_file_name` folds
  Deck 1.19.1's `info.filename` (basename *without* extension) over
  the `data` payload; `stored_file_name` then never matches
  `ANCHOR_ATTACHMENT_FILENAME`; anchor re-created every cycle), #447
  (the test half — pin the measured 1.19.1 listing shape, keep the
  1.17.5 echo decoding; ranked fallback
  `fileName → info.basename → info.filename`), #449 (distinct C4
  failure on the 1.17.5 gate tier — the fallback flush never creates
  the anchor; triage in this chunk with actor-level tracing;
  candidates: 1.17.5 write-path 500s exhausting the budget, or a
  flush-path decode mismatch). **Rider**: #282 (dead-board drain) —
  its precondition "verify Deck's actual write-answer on the tier-2
  rig" is cheap while the rig is up; stage 1 decides verify-only vs.
  fix. **Non-goals**: no anchor format change; no outbox changes; no
  CI workflow edits (the #436 lane re-run is the user's dispatch).
- **Tier records**: `.artifacts/reports/2026-10-25-tier2-convergence-fixtures-tier-records.md`
  (§ F-A / § F-B).
- **Acceptance**: C1/C2 pass on Deck 1.19.1 (anchor adopted, not
  re-created); the 1.17.5 echo shape still decodes (tier-1 contract
  fixtures + wiremock suite pin both); C4 triaged with actor traces —
  fixed or characterized-with-evidence on #449; #282 verified (or
  fixed, if the write-answer evidence supports the carve-out this
  round); zero unrelated production diffs.

**Prompt 1.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.1 of backlog round 2 ("anchor filename fallback", branch fix/anchor-filename-fallback) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, then issues #448, #447, #449 (normative), the tier records .artifacts/reports/2026-10-25-tier2-convergence-fixtures-tier-records.md (§F-A/§F-B), and issue #282 (the rider).
2. Read the seams: crates/taskboard-sync-nextcloud/src/model.rs (normalize_file_name, ExtensionInfo), src/anchor.rs (stored_file_name ~:116, the ANCHOR_ATTACHMENT_FILENAME comparison), the tier-1 contract fixtures and wiremock suite that pin attachment shapes, tests/it_docker.rs (C1/C2 anchor-confirmation waits, C4 deck1175 fallback), and the actor's flush/fallback path for the C4 tracing plan.
3. Gates: none (batch 1). Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b fix/anchor-filename-fallback ../taskboard-WT-fix-anchor-filename-fallback off main (AGENTS §8: never without -b, never on main).
4. Write the TDD-first plan at .artifacts/plans/2026-10-31-anchor-filename-fallback-plan.md: the ExtensionInfo.basename extension and the exact fallback ranking; the measured-shape pins to add (1.19.1 listing AND the preserved 1.17.5 echo) at tier 1; the C4 triage protocol (actor tracing enabled, what to capture, decision points: budget exhaustion vs. decode mismatch vs. write-500s); the #282 rider decision (verify-on-rig only vs. fix); the two-tier run matrix and evidence format. Resolve every open question with a recorded decision.
5. Commit: docs(plan): anchor filename fallback (Deck 1.19.1). No implementation, no push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.1 of backlog round 2 ("anchor filename fallback").

1. Read AGENTS.md, issues #448/#447/#449/#282, the dispatch plan .artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md §1/§2 chunk 1.1, and the stage-1 plan in ../taskboard-WT-fix-anchor-filename-fallback (branch fix/anchor-filename-fallback). Recreate the worktree on that branch if missing.
2. Implement per the plan, TDD, every commit standalone: first the failing tier-1 pin of the 1.19.1 shape, then the fallback-ranking fix. Bring the docker tier up and run C1/C2 on the default image and C4 (plus the f-series untouched) per the plan's matrix; enable the actor tracing the plan specifies for C4 and capture the traces. Hard rules: no sleeps in fixtures (the existing eventually/deadline helpers only); no text assertions; zero production diffs outside model.rs/anchor.rs/the C4 root cause's honest fix.
3. If the C4 evidence lands in production-code territory (e.g. a real flush-path bug), fix it TDD in this chunk and say so in the report; if it is pure environment/budget, record the characterization — do not stretch the chunk.
4. Bookkeeping: none beyond the branch (stage 3 posts tier records and closes #447/#448/#449 — NOT #436, which closes on the lane re-run; NOT #282 unless fixed).
5. Full AGENTS §10 gate suite green. No push, no PR. Never push to or merge main.

Report: tier run records (both images), gate results, C4 verdict, deviations.
```

**Prompt 1.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.1 of backlog round 2 (branch fix/anchor-filename-fallback).

1. Read AGENTS.md, issues #448/#447/#449/#282, the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; then review the full branch diff after rebasing onto origin/main per §2 shared model step 3.
2. Functional traps: the fallback ranking must prefer info.basename ONLY when extendedData.fileName is absent (a regression that prefers basename over a present fileName breaks other Deck versions); the 1.17.5 echo pin must demonstrably still pass (it is the compatibility contract); C4's disposition must be evidence-backed either way (traces in the report, not assertions); the C1/C2 fix must make the anchor ADOPTED (one anchor on the board), not merely make the wait stop failing; #282's verify/fix outcome recorded with the rig evidence.
3. Re-run the plan's tier matrix yourself (or the tiers the environment allows, stating which). Re-run the full gate suite; any failure is a finding. Fix findings on the branch.
4. Ship per §2 step 3 (PR, checks green, mergeStateStatus CLEAN). Post the two-tier records in the PR description. Close #447, #448, #449 (and #282 if fixed) with self-contained evidence comments; explicitly do NOT close #436 — comment on it instead that the fix is merged and the lane re-run (user-dispatched or next weekly schedule) is its closing step.
5. Teardown per §2 step 3 (absolute anchor, git worktree list confirmation).

Report: verdict, findings, fixes, PR URL, green checks, tier records location, issues closed, #436 commented, worktree removed. Never merge; never touch main.
```

### Chunk 1.2 — skew pins, RESUME (#480) (`test/sync-skew-pins`)

- **State** (verified 2026-10-31): stage 1 **landed** on `main`
  (`1fec235a`, plan at
  `.artifacts/plans/2026-10-26-sync-skew-pins-plan.md`); stage 2
  **exists unlanded** on branch `test/sync-skew-pins` (commit
  `44c1fa71`, "the clock-skew interleaving pins (DR-4/S)"). The chunk
  paused solely on the F-133 ruling — **confirmed 2026-10-31 as
  Option A** (recorded on #480). The branch is not contained in
  `origin/main`, so resuming it is the correct continuation, not a
  #415 reuse.
- **Covers** (normative): #480 and the landed plan — (i)-residual and
  (ii) skew interleavings as `…_currently_pins_skew_…` debt pins,
  (iii) seconds-tie as a plain contract pin, one actor-tier hermetic
  Clock-seam demo (wiremock cycle, injected fake clock set
  ahead/behind), **no behavior change anywhere**. Bookkeeping: the
  F11-01 verification comment on #133; **#133 stays open** with its
  documented trigger.
- **Acceptance**: the debt pins pass and carry the
  `currently_pins` naming; the seam demo is hermetic (no wall
  reads); zero production diffs; #133 carries the verification
  comment; #480 closes with the disposition evidence.

**Prompt 1.2.a (stage-2' verification — replaces N.1/N.2):**

```text
You are the stage-2' VERIFICATION agent for chunk 1.2 of backlog round 2 ("skew pins resume", branch test/sync-skew-pins) in /home/viktoria/src/taskboard.

Context: this chunk was paused mid-flight on the F-133 ruling, now confirmed as Option A (2026-10-31, recorded on #480). Stage 1's plan is landed on main (1fec235a, .artifacts/plans/2026-10-26-sync-skew-pins-plan.md); stage 2's implementation commit (44c1fa71) sits unlanded on the branch.

1. Read AGENTS.md, issue #480, the landed plan, and the F-133 ruling comment on #480. 
2. Doctor, then: git fetch origin --prune; git worktree add test/sync-skew-pins ../taskboard-WT-test-sync-skew-pins (the branch already exists — add WITHOUT -b; never on main). Rebase the branch onto origin/main per the §2 rebase dance (this drops the already-merged duplicate of the stage-1 plan commit; the implementation commit 44c1fa71 survives; resolve only real conflicts).
3. Verify the implementation against the landed plan + ruling: all planned pins present ((i)-residual, (ii) both directions, (iii) contract pin) with the …_currently_pins_skew_… naming; the hermetic Clock-seam demo; zero production diffs (git diff origin/main --stat must show test files only); nothing weakened relative to the plan. Complete anything the plan requires that is missing (test-only commits on the branch).
4. Run the suite (cargo nextest -p taskboard-sync-nextcloud -p taskboard-state as the subject; the full gate suite if any doubt). No push, no PR — stage 3 ships.

Report: rebase outcome, completeness verdict vs. plan, gate results, anything you had to add.
```

**Prompt 1.2.b (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.2 of backlog round 2 (branch test/sync-skew-pins) in ../taskboard-WT-test-sync-skew-pins.

1. Read AGENTS.md, issue #480, the landed plan, the dispatch plan §1/§2 (code-quality mandate applies); review the full branch diff versus post-rebase origin/main.
2. Functional traps: a debt pin that would still pass after the semantics CHANGED is not a pin — verify each pin actually constrains today's skew behavior (mentally flip the merge verdict and check the test would fail); the hermetic demo must use the injected fake clock, never a wall read; the pins must be named …_currently_pins_skew_… (AGENTS §5 debt convention) and carry comments naming the trigger (#133) that would replace them.
3. Re-run the gate suite (AGENTS §10); any failure is a finding. Fix on the branch.
4. Ship per §2 step 3. Close #480 with the disposition evidence; post the F11-01 verification comment on #133 and leave it OPEN (the trigger stands; closing is the archive act).
5. Teardown per §2 step 3.

Report: verdict, findings, fixes, PR URL, green checks, #480 closed, #133 commented-and-open, worktree removed. Never merge; never touch main.
```

### Chunk 1.3 — unchanged-refresh guard (#463) (`perf/ui-unchanged-refresh-guard`)

- **Covers** (normative): #463 — keep the last-seen `Arc<AppState>`
  from the `ArcSwap` load beside the adapter's cross-refresh state;
  `Arc::ptr_eq` ⇒ return before reconcile/push. A miss (engine stores
  an identical-values state) costs one wasted pass, never a stale UI.
  Pin: an unchanged second refresh does no derive/diff work (typed
  seam — the diff mutation counter or equivalent; no timing
  assertions). Journeys/keyboard suites untouched. Opens the render
  lane (§1 lane 1).
- **Acceptance**: the guard at the top of the refresh pass; the
  unchanged pin; every existing journey/keyboard/golden test green
  unmodified; gate suite green; the 100 ms cadence cost at board
  scale argued to ~0 when idle (from the pin, not timed).

**Prompt 1.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.3 of backlog round 2 ("unchanged-refresh guard", branch perf/ui-unchanged-refresh-guard) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then GitHub issue #463 (normative) and .artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md §3.7 (the measured refresh numbers this guard eliminates).
2. Read the seams you will extend: the refresh call path (UiHandle::refresh → view_state reconcile → models::push) in crates/taskboard-ui-slint/src/adapter.rs, the ArcSwap load site (where the AppState Arc enters the UI), models.rs's diff mutation counter (or the nearest equivalent typed seam that can observe "no derive/diff work happened"), and every caller of the refresh pass (grep — each must be classified: effect-driven dispatches store a new Arc and pass the guard naturally; anything else needs a forced-pass story).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree: git worktree add -b perf/ui-unchanged-refresh-guard ../taskboard-WT-perf-ui-unchanged-refresh-guard off main (never without -b, never on main — AGENTS §8).
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md: where the cached Arc lives (owner + lifetime), the exact guard condition, the initial-pass story (the very first refresh must run with an empty cache), the pin's seam and its assertion, and the full refresh-caller audit table (caller → classification → why). Resolve every open question with a recorded decision; deviations from the origin need a recorded reason.
5. Commit it as: docs(plan): unchanged-refresh guard. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.3 of backlog round 2 ("unchanged-refresh guard").

1. Read AGENTS.md, issue #463, the dispatch plan .artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md §1/§2/§3 chunk 1.3, and the stage-1 plan (.artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md) in the worktree ../taskboard-WT-perf-ui-unchanged-refresh-guard (branch perf/ui-unchanged-refresh-guard). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene): first the failing pin (an unchanged second refresh does no derive/diff work, observed through the plan's typed seam — never a timing assertion), then the Arc::ptr_eq guard at the top of the refresh pass per the plan's placement. Hard rules: zero changes to journey/keyboard/golden tests (they must pass unmodified — a change there is a deviation to report, not to make); no sleeps; the miss path (different Arc, identical values) must cost one normal pass, never staleness.
3. Run the full gate suite from AGENTS.md §10 and make it green: cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --workspace; cargo test --doc --workspace; cargo deny check; the llvm-cov three-command sequence with the quarantine script and trailing rm of target/llvm-cov-target.
4. No CHANGELOG (no user-observable behavior change). No push, no PR — shipping is stage 3's task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.3 of backlog round 2 (branch perf/ui-unchanged-refresh-guard).

1. Read AGENTS.md, issue #463, the dispatch plan .artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md §1/§2/§3 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-perf-ui-unchanged-refresh-guard after rebasing onto origin/main per §2 shared model step 3 (rebase dance; push --force-with-lease only if history was rewritten).
2. Functional focus — this chunk's specific traps: the guard must compare the Arc AS LOADED from the ArcSwap (a cached handle from an older load defeats it); an engine store of identical values must cost one wasted pass and never staleness (argue or pin — a pin that proves a stale UI is impossible beats an argument); the first refresh after mount must run (empty-cache story verified in a test); the pin must be load-bearing — temporarily invert the guard and confirm the pin fails, then restore (record that you did this); zero journey/keyboard/golden diffs.
3. Apply §2's code-quality mandate in full (naming, comment discipline, SRP, single level of abstraction; no dead code).
4. Re-run the full gate suite yourself (AGENTS §10: fmt, clippy -D warnings, nextest, doc tests, deny, the llvm-cov sequence with quarantine and rm); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (title: perf(ui-slint): the unchanged-refresh guard), watch gh pr checks --watch to completion — any failing check is a finding; diagnose, fix, push --force-with-lease, repeat until green — then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-perf-ui-unchanged-refresh-guard; confirm via git worktree list that the removed path was the intended one. Close #463 with a self-contained evidence comment (the pin + guard + PR link).

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, issue closed, worktree-removed confirmation. Never merge; never touch main.
```

### Chunk 1.4 — typed scheduled-phrase direction (#458) (`refactor/scheduled-phrase-typed-direction`)

- **Covers** (normative): #458 — resolution **option 2** per the
  report's recommendation the user accepted: `scheduled_phrase` builds
  on a hand-rolled typed intermediate (enum Future/Past/Now ×
  DurationBucket, A1 house pattern), the wording living only in a
  format/Display step; pin the *enum* per direction with structural
  assertions; text stays untested. Kills the `replace < with >`
  survivor at `presentation.rs:153:18`. The two `<`→`<=` equivalents
  (`:134:14`, `:153:18`, dead delta==0 arm behind the `Now` early
  return) need no action — record as understood. Non-goals: no
  ignore-list entries, no wording changes, no other presentation
  refactors.
- **Acceptance**: the direction-flip mutant dies (scoped mutant
  re-check with zero survivors for the flip, recorded as evidence);
  enum pinned structurally; user-visible wording byte-identical
  (goldens/snapshots untouched); suite green.

**Prompt 1.4.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.4 of backlog round 2 ("typed scheduled-phrase direction", branch refactor/scheduled-phrase-typed-direction) in /home/viktoria/src/taskboard.

1. Read AGENTS.md (§2 type-driven policy is normative for the newtype/enum shape), GitHub issue #458 (normative — the survivor evidence, the tension with §5, and both candidate resolutions), and .artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md (the sweep evidence).
2. Read the seams: crates/taskboard-ui-slint/src/presentation.rs — scheduled_phrase (~:153), its current test scheduled_phrase_is_empty_when_absent_and_directional_when_set (~:1016), the DurationBucket machinery it builds on, and the goldens/snapshots that render its output.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b refactor/scheduled-phrase-typed-direction ../taskboard-WT-refactor-scheduled-phrase-typed-direction off main (AGENTS §8).
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-scheduled-phrase-typed-direction-plan.md: the enum shape (variants, fields — hand-rolled, private-field, no derive crates), where the wording moves (the format step), the structural pin catalogue (one pin per direction × representative bucket — matches!, never text), and the evidence protocol for the mutant dying (the scoped cargo-mutants invocation with its file filter). Record the decision that the two < → <= equivalents are understood-dead and untouched. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): typed scheduled-phrase direction. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.4.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.4 of backlog round 2 ("typed scheduled-phrase direction").

1. Read AGENTS.md (§2, §5), issue #458, the dispatch plan .artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md §1/§2/§3 chunk 1.4, and the stage-1 plan in ../taskboard-WT-refactor-scheduled-phrase-typed-direction (branch refactor/scheduled-phrase-typed-direction). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: introduce the typed intermediate, pin it structurally per direction (matches!-class assertions — no text assertions), then move the wording into the format step consuming the enum. Hard rules: the user-visible output must be byte-identical (existing snapshots/goldens must pass untouched — a golden diff is a bug in the refactor, not something to regenerate); hand-rolled enum, no derive crates (AGENTS §2); no wording changes.
3. Evidence: run the plan's scoped mutant check (cargo mutants with the file filter over presentation.rs, SQLX_OFFLINE=true, capped per the #427 sweep's conventions) and record zero survivors for the direction flip; verify git status is clean after the in-place run. Paste the summary into the stage-2 report.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No CHANGELOG (no user-observable change). No push, no PR. Never push to or merge main.

Report: per-gate results, the mutant-check evidence, any deviation from the plan and why.
```

**Prompt 1.4.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.4 of backlog round 2 (branch refactor/scheduled-phrase-typed-direction).

1. Read AGENTS.md (§2, §5), issue #458, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the enum must actually discriminate the directions the mutant flips (a pin that passes under BOTH wordings proves nothing — mentally apply the flip and check each pin would fail); the format step must be the ONLY place wording exists; bucket boundaries must be untouched (a moved boundary changes output at edge inputs — the snapshots staying byte-identical is the proof); no ignore-list entry may have appeared in any mutants config; the two < → <= equivalents are untouched.
3. Re-run the scoped mutant check yourself (or state why not) and the full gate suite; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled refactor(ui-slint): the typed scheduled-phrase direction, checks green, mergeStateStatus CLEAN).
5. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-scheduled-phrase-typed-direction (absolute anchor; confirm via git worktree list). Close #458 with the evidence (mutant check summary + PR link).

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.5 — worktree-doctor never-pushed blind spot (#450) (`fix/worktree-doctor-never-pushed`)

- **Covers** (normative): #450 — the doctor additionally spares a
  worktree whose branch has **no remote ref** (never pushed): a
  merged-PR leftover always has `origin/<branch>` or is contained in
  `origin/main` after rebase-merge; a fresh `-b` branch at
  `origin/main`'s tip with zero commits fails no existing protection
  and must be KEEP. The false-positive direction (a stale
  never-pushed branch genuinely abandoned) gets a recorded stance.
  Files: `scripts/worktree-doctor.sh` only.
- **Acceptance**: a fresh never-pushed worktree is KEEP; a
  pushed-and-merged leftover is still REMOVE; dirty/unmerged/on-main
  protections and the floor check behave exactly as before; exit
  semantics unchanged (nonzero = below floor only); verification
  cases run for real with transcripts.

**Prompt 1.5.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.5 of backlog round 2 ("worktree-doctor never-pushed blind spot", branch fix/worktree-doctor-never-pushed) in /home/viktoria/src/taskboard.

1. Read AGENTS.md §8 (the doctor is the only sanctioned bulk-removal tool; its exit contract is "nonzero = free disk below the floor, and only that"), GitHub issue #450 (normative — the swept-worktree incident), and the sweep-fix plan .artifacts/plans/2026-10-25-worktree-doctor-sweep-plan.md (the verification checklist and the porcelain enumeration you must preserve).
2. Read scripts/worktree-doctor.sh end to end: the porcelain enumeration, the KEEP/REMOVE protections (unmerged HEAD, dirty tree, on main), the floor check, every exit path. Reproduce the #450 failure mode locally as evidence: create a throwaway worktree with git worktree add -b <throwaway-name> ../taskboard-WT-<throwaway-name> off origin/main, run the doctor, observe it flagged disposable, remove the worktree immediately after (git worktree remove --force). This transcript goes in the plan.
3. Run scripts/worktree-doctor.sh (expect exit 0 with the throwaway gone), then: git worktree add -b fix/worktree-doctor-never-pushed ../taskboard-WT-fix-worktree-doctor-never-pushed off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-worktree-doctor-never-pushed-plan.md: the spare rule (how to test "branch has no remote ref" — git ls-remote --dry-run cost is unacceptable; use the local refs: no refs/remotes/origin/<branch> AND not fully contained in origin/main ⇒ never-pushed), the false-positive stance (recommend: KEEP on never-pushed, documented — a deliberately abandoned scratch branch is pruned by hand, the doctor is not its tool), the full verification matrix (fresh never-pushed KEEP; pushed-and-merged leftover REMOVE; dirty KEEP; unmerged KEEP; on-main KEEP; below-floor nonzero; bogus sibling tolerated — the 2026-10-25 cases), and the exit-code contract restated. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): worktree-doctor never-pushed blind spot. Do not implement; do not push.

Report: plan path, decisions, risks.
```

**Prompt 1.5.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.5 of backlog round 2 ("worktree-doctor never-pushed blind spot").

1. Read AGENTS.md §8, issue #450, the dispatch plan §1/§2/§3 chunk 1.5, and the stage-1 plan in ../taskboard-WT-fix-worktree-doctor-never-pushed (branch fix/worktree-doctor-never-pushed). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan. Hard rules: `set -euo pipefail` stays; the floor check must be reachable on every path; the new check uses local refs only (no network calls in the doctor); the existing porcelain enumeration and all KEEP protections are preserved verbatim; files changed: scripts/worktree-doctor.sh only.
3. Run every case in the plan's verification matrix for real and capture transcripts: fresh never-pushed worktree (KEEP); a worktree whose branch is pushed and contained in origin/main (REMOVE — create it, let the doctor flag it, then remove it); dirty (KEEP); unmerged HEAD (KEEP); on main (KEEP); below the floor via the script's own knob if it has one else a documented dry argument; a bogus non-git taskboard-WT-* sibling (tolerated, floor still reached — from the 2026-10-25 fix). Paste all transcripts into the stage-2 report. Clean up every throwaway worktree you create (git worktree remove --force, absolute anchor, git worktree list confirmation).
4. Gates: bash -n clean; shellcheck if available; the doctor must never dirty the tree (git status --short clean after every run).
5. No push, no PR. Never push to or merge main.

Report: transcripts per case, any deviation from the plan and why.
```

**Prompt 1.5.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.5 of backlog round 2 (branch fix/worktree-doctor-never-pushed).

1. Read AGENTS.md §8, issue #450, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: exit-contract drift is THE regression (a nonzero exit that means anything but "below the floor" reintroduces the #395/#450 class of false STOP for every future agent); the never-pushed test must not shell out to the network; a branch that IS pushed but whose remote ref was pruned must not read as never-pushed unless it is also not contained in origin/main (check the exact condition against the plan); the containment check reuses the doctor's existing unmerged-HEAD logic rather than duplicating it (DRY at the script level); the AGENTS §8 docs were NOT touched (no doc change is in scope for this chunk).
3. Re-run the plan's verification matrix yourself (or as many cases as the environment allows, stating which); any divergence is a finding. Re-run bash -n / shellcheck.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled fix(scripts): the doctor spares never-pushed branches, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3 (absolute anchor, git worktree list confirmation). Close #450 with the verification transcripts summarized in the close comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.6 — IPC disable knob (#194) (`feat/ipc-disable-knob`)

- **Covers** (normative): #194 — a `#[serde(default = "default_true")]`
  `[ipc] enabled` config key plus a `daemon --no-ipc` override; when
  disabled, skip `spawn_server`; the rest of the lifecycle unchanged;
  documented degradation: `status`/`daemon stop` surface the typed
  `no-daemon` state. Enabled-by-default semantics unchanged.
- **Acceptance**: all existing IPC tests green untouched; disabled
  boot serves nothing on the socket and `status` reports the typed
  no-daemon state; `--no-ipc` overrides the file beats the default;
  docs + CHANGELOG updated.

**Prompt 1.6.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.6 of backlog round 2 ("IPC disable knob", branch feat/ipc-disable-knob) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #194 (normative), and docs/architecture.org's IPC section (what the surface is for, so the degradation is documented honestly).
2. Read the seams: crates/taskboard-app/src/config.rs (the [ipc] section's home — if none exists, where it lands next to its siblings; the serde defaults pattern in use; the validate step), src/main.rs and src/ipc/ (where spawn_server is called and what the daemon does without it), the typed no-daemon error/status path (what status and daemon stop return today when no daemon serves), and the existing IPC tests that pin the enabled path.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/ipc-disable-knob ../taskboard-WT-feat-ipc-disable-knob off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-ipc-disable-knob-plan.md: the config key shape (serde default true) and its exact section/key name; the --no-ipc override precedence (flag > file > default); the disabled boot journey (what starts, what does not — ONLY spawn_server is skipped; lock file, actors, engine untouched); the status/daemon stop degradation (the typed no-daemon outcome, pinned); the docs touch (config guide + architecture.org one-liner) and the CHANGELOG entry wording. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): IPC disable knob. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.6.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.6 of backlog round 2 ("IPC disable knob").

1. Read AGENTS.md, issue #194, the dispatch plan §1/§2/§3 chunk 1.6, and the stage-1 plan in ../taskboard-WT-feat-ipc-disable-knob (branch feat/ipc-disable-knob). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the config key with its serde default, the CLI override, the spawn_server skip, the typed no-daemon surface. Hard rules: zero behavior change when enabled (the entire existing IPC test suite must pass unmodified — any edit there is a deviation to report, not make); ONLY the socket server is skipped (instance lock, actors, engine lifecycle byte-identical); precedence is flag > file > default and pinned by a test; config follows the crate's existing serde/validation patterns (a plain bool needs no newtype — stay consistent with AppConfig's shapes).
3. Update CHANGELOG.md's Unreleased section (one feature entry) and the docs the plan names.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.6.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.6 of backlog round 2 (branch feat/ipc-disable-knob).

1. Read AGENTS.md, issue #194, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: "disabled" must mean exactly "socket not served" — audit the diff for accidental skips (lock file, engine boot, shutdown joins); an existing client connecting to a disabled daemon must get the typed no-daemon outcome, never a hang or a raw connection error (pinned); the override precedence pinned at the parse boundary, not re-derived in the daemon; the docs' degradation statement matches the pinned behavior exactly (status/daemon stop reduced to typed no-daemon — nothing more promised).
3. Re-run the full gate suite yourself; any failure is a finding. Confirm the enabled-path suite is untouched (git diff shows no edits under existing ipc tests).
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(app): the IPC disable knob, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #194 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.7 — clap at-least-one-flag groups (#407) (`fix/tasks-edit-flag-group`)

- **Covers** (normative): #407 — `TasksCmd::Edit` and the labels
  `Edit` arm gain `ArgGroup(required = true, multiple = true)` so a
  zero-flag edit is a clap usage error (nonzero exit) instead of
  falling through to a typed `no_op`. The docstrings were already
  corrected to describe the intended behavior — they now get to be
  true.
- **Acceptance**: zero-flag `tasks edit` and `labels edit` rejected
  at the clap boundary; at-least-one-flag edits unchanged; docstrings
  accurate; CLI journeys green.

**Prompt 1.7.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.7 of backlog round 2 ("tasks-edit flag group", branch fix/tasks-edit-flag-group) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, then GitHub issue #407 (normative) and its origin plan .artifacts/plans/2026-10-22-daily-driver-task-field-surfaces-plan.md (D10 — the review ruling that ordered this entry).
2. Read the seams: crates/taskboard-app/src/cli.rs (the TasksCmd::Edit struct and the labels Edit arm — every flag field, any existing ArgGroup usage to imitate), crates/taskboard-app/src/commands/tasks.rs and commands/labels.rs (the edit arms that today receive an empty changeset and type it as no_op), and the CLI test layer that pins parse outcomes (find how existing tests assert clap rejections structurally — the error kind, never clap's message text).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b fix/tasks-edit-flag-group ../taskboard-WT-fix-tasks-edit-flag-group off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-tasks-edit-flag-group-plan.md: the exact ArgGroup attributes for both arms (id names, required = true, multiple = true), the test catalogue (zero-flag rejected for each arm; single-flag parses; all-flags-together parses — multiple = true must allow the full set), and where the structural rejection is pinned (parse boundary, asserting the clap error kind — no text assertions). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): tasks/labels edit flag group. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.7.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.7 of backlog round 2 ("tasks-edit flag group").

1. Read AGENTS.md, issue #407, the dispatch plan §1/§2/§3 chunk 1.7, and the stage-1 plan in ../taskboard-WT-fix-tasks-edit-flag-group (branch fix/tasks-edit-flag-group). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: first the failing parse-boundary tests (zero-flag rejected, structurally), then the ArgGroup attributes on BOTH arms — the labels arm is as much the subject as the tasks arm. Hard rules: assert the parse outcome (error kind / ok-with-parsed-flags), never clap's message text (AGENTS §5); an all-flags edit must still parse (multiple = true); the command arms themselves stay untouched — a zero-flag edit can no longer reach them, which is the whole point; the docstrings on both arms stay as-is (they describe this behavior already).
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. CHANGELOG: one bug-fix/behavior entry (zero-flag edits now rejected at the CLI). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.7.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.7 of backlog round 2 (branch fix/tasks-edit-flag-group).

1. Read AGENTS.md, issue #407, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: BOTH arms pinned (a tasks-only fix that misses labels is the classic half-landing — check the diff); the group id names don't collide with any existing clap ids in the same command tree; a flags-only-subset edit still parses; the rejection is a clap usage error (nonzero exit) and NOT a new engine error path (the diff must not touch commands/ semantics); no text assertions anywhere in the new tests.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled fix(app): tasks and labels edit demand at least one flag, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #407 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.8 — Nextcloud version matrix (#116) (`ci/nextcloud-version-matrix`)

- **Covers** (normative): #116 — a matrix leg for the oldest
  supported Nextcloud/Deck major on the docker tier, added to the
  weekly `Nextcloud Integration` workflow via a separate compose
  override or an overridable image tag. The 1.17.5 gate-tier tooling
  (`DECK_VERSION=v1.17.5`) already exists — stage 1 decides the legs
  and the mechanism. No secrets added.
- **Acceptance**: the workflow defines both legs (current stable +
  oldest supported) with per-leg failure visibility; both legs
  validated locally with the same nextest invocation the workflow
  uses; legs are parallel-safe (isolated container names/volumes);
  YAML lint clean.

**Prompt 1.8.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.8 of backlog round 2 ("Nextcloud version matrix", branch ci/nextcloud-version-matrix) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #116 (normative), and the origin plan .artifacts/plans/2026-10-08-nextcloud-sync-testing-plan.md (the tier model — what tier 2 promises).
2. Read the seams: .github/workflows/nextcloud-integration.yml (its current single-leg job, schedule, env, artifact handling), docker/nextcloud-test/compose.yaml (how the image/DECK_VERSION is parameterized today), and how tests/it_docker.rs consumes DECK_VERSION (the C-series' existing v1.17.5 leg is the precedent for a second major).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b ci/nextcloud-version-matrix ../taskboard-WT-ci-nextcloud-version-matrix off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-nextcloud-version-matrix-plan.md: the matrix legs (recommend: current stable — the default image — plus the oldest supported pair; name the exact tags and why they are the supported floor), the mechanism (matrix strategy over a compose-override file vs. an env-parameterized tag — decide with reasons), leg isolation (container names, network names, volumes must not collide when legs run in parallel runners), the validation protocol (local compose runs of both legs with the same cargo nextest invocation the workflow uses), and the YAML lint step. No secrets anywhere. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): Nextcloud docker tier version matrix. Do not implement; do not push.

Report: plan path, decisions, risks.
```

**Prompt 1.8.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.8 of backlog round 2 ("Nextcloud version matrix").

1. Read AGENTS.md, issue #116, the dispatch plan §1/§2/§3 chunk 1.8, and the stage-1 plan in ../taskboard-WT-ci-nextcloud-version-matrix (branch ci/nextcloud-version-matrix). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan: the workflow matrix edit and the compose override (or parameterization) it needs. Hard rules: the weekly schedule and the workflow's existing non-docker jobs untouched; per-leg failure visibility (a leg failing must name its leg in the check); no new secrets, no server addresses, no credentials (the zero-secrets-on-GitHub property from the origin plan is binding); the docker tier's fixture expectations apply per leg as they do today (the #448/#449 C-series state on the 1.17.5 image is known — a leg may legitimately show the currently-open C4 bug; that is visibility working, not a workflow failure to paper over: the leg runs, the test outcome is recorded, the workflow's pass/fail policy for known-open bugs is stage 1's recorded decision).
3. Validate: bring up BOTH legs locally per the plan (compose up with each override; run the same nextest invocation the workflow uses; record outcomes per leg), then tear the containers down. actionlint (or yamllint) clean on the workflow.
4. Full gate suite green where it applies (the tree's cargo surface is unchanged — fmt/clippy must stay clean trivially; nextest -p taskboard-sync-nextcloud tier-1 as the canary).
5. No CHANGELOG (CI-only). No push, no PR. Never push to or merge main.

Report: per-leg local run records, lint results, any deviation from the plan and why.
```

**Prompt 1.8.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.8 of backlog round 2 (branch ci/nextcloud-version-matrix).

1. Read AGENTS.md, issue #116, the dispatch plan §1/§2/§3, and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: legs must not share docker resource names (two parallel runners would otherwise collide — read the override file for names/volumes/networks); the image tag must come from the matrix parameter, never a sed into a shared file; the schedule/cron untouched; the known-open-bug pass/fail policy is exactly stage 1's recorded decision (a soft-fail knob that would hide a NEW regression is a finding); zero secrets in the diff (grep the workflow for env blocks).
3. Re-run actionlint/yamllint yourself and one leg's local compose validation (or state why not).
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled ci(sync): the Nextcloud docker tier version matrix, checks green, mergeStateStatus CLEAN). Note in the PR body that the first scheduled run is the user's observation point.
5. Teardown per §2 step 3. Close #116 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.9 — undo tracking/habit arms (#555) (`feat/undo-tracking-habit-arms`)

- **Covers** (normative): #555 + the classification table in
  `.artifacts/plans/2026-10-30-undo-journal-plan.md` §3 (D20) — the
  mechanically invertible arms join the inverse-synthesis catalogue:
  `LogTime`→`DeleteTimeEntry` (on the minted id),
  `AdjustTimeEntry`→same verb with pre start/end,
  `CreateHabit`→`DeleteHabit`, `UpdateHabit`→same verb with pre
  `HabitChanges`, `MoveHabit`→same verb with pre order,
  `AdjustHabitLog`→same verb with pre count. **Stay out** even in v2:
  `StartTracking`/`StopTracking` (structurally non-invertible, ADR
  0017) and `LogHabit` (lossy — a pre-log absent row would become an
  explicit count-0 row, a different stored fact).
- **Acceptance**: each newly-invertible arm round-trips (undo
  restores the pre-state, redo re-applies — pinned per arm); the
  excluded arms still classify `NotUndoable` with the recorded
  reasons; the existing undo e2e suite green; ADR 0019 untouched
  (a dated one-line amendment note only if its classification
  reference moved).

**Prompt 1.9.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.9 of backlog round 2 ("undo tracking/habit arms", branch feat/undo-tracking-habit-arms) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, GitHub issue #555 (normative), the undo journal plan .artifacts/plans/2026-10-30-undo-journal-plan.md (§3's classification table and decision D20), and ADR 0019 (the journal's rules — single-advance, the pushed-edit rule) plus ADR 0017 (why Start/StopTracking are non-invertible).
2. Read the seams: the inverse-synthesis catalogue and plan_undo/plan_redo in crates/taskboard-domain/src (where v1 classified the tracking/habit arms NotUndoable), the tracking and habit command definitions (messages.rs / command.rs — LogTime, AdjustTimeEntry, CreateHabit, UpdateHabit, MoveHabit, AdjustHabitLog: their payloads and what pre-state data each already carries vs. what the planner must capture), the journal wrapper, and the undo e2e suite (the v1 round-trip test patterns to imitate).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/undo-tracking-habit-arms ../taskboard-WT-feat-undo-tracking-habit-arms off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-undo-tracking-habit-arms-plan.md: per arm — the inverse verb shape, where the pre-image comes from (payload vs. journal entry), the round-trip test (verb → undo → pre-state equality → redo), and edge cases (LogTime's minted id: the journal entry must carry it for DeleteTimeEntry; AdjustHabitLog's pre count; MoveHabit's pre order position). The exclusion list (Start/StopTracking, LogHabit) stays NotUndoable with the recorded reasons restated at the classification site. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): undo tracking and habit arms. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.9.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.9 of backlog round 2 ("undo tracking/habit arms").

1. Read AGENTS.md, issue #555, the undo journal plan §3/D20, the dispatch plan §1/§2/§3 chunk 1.9, and the stage-1 plan in ../taskboard-WT-feat-undo-tracking-habit-arms (branch feat/undo-tracking-habit-arms). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: classify the six arms in the inverse-synthesis catalogue with their pre-image sources, then one round-trip test per arm (verb applied → undo → the pre-state asserted equal → redo → the post-state asserted equal) in the e2e suite's established patterns. Hard rules: the journal's single-advance rule and the pushed-edit rule untouched; the exclusion set (StartTracking, StopTracking, LogHabit) remains NotUndoable with reasons — silently shrinking it is a deviation to report, never make; no new command surface (undo reuses existing verbs — if an inverse needs a command that does not exist, STOP and report instead of inventing one); no storage schema change (the journal already holds the pre-images — if it does not for some arm, STOP and report).
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. CHANGELOG: one entry (undo now covers tracking adjustments and habit bookkeeping). ADR 0019: only a dated one-line amendment note if the plan requires it. No push, no PR. Never push to or merge main.

Report: per-gate results, per-arm round-trip outcomes, any deviation from the plan and why.
```

**Prompt 1.9.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.9 of backlog round 2 (branch feat/undo-tracking-habit-arms).

1. Read AGENTS.md, issue #555, the undo journal plan §3/D20, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: pre-image fidelity is THE hazard (UpdateHabit's inverse must restore the full pre HabitChanges, AdjustHabitLog the exact pre count, MoveHabit the pre order position — an inverse built from the change mask's post values passes its own round-trip and is still wrong: check each planner site reads pre); LogTime's inverse needs the minted entry id from the journal entry (verify it is journaled, not re-derived); undoing a pushed LogTime must follow the pushed-edit rule like v1's task arms (retraction vs. re-create semantics identical to the v1 precedents); the exclusion set intact.
3. Re-run the full gate suite and every round-trip test yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(domain): the tracking and habit arms join undo, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #555 with a self-contained comment (per-arm outcome table).

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 1.10 — pty flake pass (#521 + #524 + #549; dispositions #287/#336/#384) (`test/pty-flake-pass`)

- **Covers** (normative): the three sibling cli_tty flakes — one
  mechanism (60 s outer pty timeout under full-suite parallel load),
  three observations in three days (#521 masked-read fails-typed,
  #524 masked-read never-persists, #549 stalled-line cancels). The
  pass: root-cause the load sensitivity, fix deterministically per
  AGENTS §5 (injected/derived budget or a harness seam — never
  sleeps, never a bigger constant), pin under a stress profile. Then
  the watchlist dispositions: #287, #336, #384 (open since the last
  pass's capture policies — re-triage: fix where root-caused, else
  refresh/annotate; close only where the evidence supports archive).
- **Acceptance**: all three pty tests pass under the recorded stress
  profile and in isolation; the fix is a seam/budget change; each of
  the six issues carries a standalone disposition comment; no test
  weakened.

**Prompt 1.10.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.10 of backlog round 2 ("pty flake pass", branch test/pty-flake-pass) in /home/viktoria/src/taskboard.

1. Read AGENTS.md §5 (determinism rules, no sleeps, debt-naming) and §7 (the flaky-test protocol: dedicated pass, one issue per test, never opportunistic), then issues #521, #524, #549 (normative — same mechanism, three observations) and the disposition targets #287, #336, #384 (their capture policies from the previous pass).
2. Read the harness: crates/taskboard-app/tests/cli_tty.rs (the three tests, their marker-gated blocking reads, the outer `timeout 60` wrapper — the only bound on a wedged pty), the pty helper infrastructure, and how the suite's other bounded waits express budgets (the injected/deadline patterns AGENTS §5 prefers).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b test/pty-flake-pass ../taskboard-WT-test-pty-flake-pass off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-pty-flake-pass-plan.md: the repro protocol (the load shape: full-workspace nextest under recorded parallelism, or the coverage-lane instrumentation the observations named; iteration counts; what a negative repro means for the fix decision), the fix design (the outer timeout is the only bound — decide: derive the budget from the harness's own markers with a deadline, inject it, or restructure the wedged-session detection; a bare larger constant is ruled out by AGENTS §5 and named as a finding), the stress-validation protocol, and per-issue disposition plans for #287/#336/#384 (re-triage steps, capture-policy refresh vs. fix vs. archive-close with evidence). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): pty flake pass. Do not implement; do not push.

Report: plan path, decisions, risks.
```

**Prompt 1.10.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1.10 of backlog round 2 ("pty flake pass").

1. Read AGENTS.md §5/§7, issues #521/#524/#549/#287/#336/#384, the dispatch plan §1/§2/§3 chunk 1.10, and the stage-1 plan in ../taskboard-WT-test-pty-flake-pass (branch test/pty-flake-pass). Recreate the worktree on that branch if missing.
2. Execute per the plan: run the repro protocol (a negative repro is a result — record it and proceed to the fix the plan prescribes anyway, since the mechanism reading is confirmed by three observations); implement the seam/budget fix TDD (the plan's design); validate under the recorded stress profile AND in isolation (both must pass; record both). Hard rules: no sleeps anywhere; no constant enlarged (if your fix's diff changes a timeout NUMBER upward, stop — that is the finding class this pass exists to prevent; re-read the plan); the three tests' semantics unchanged (a weakened assertion is a deviation to revert); fixes are test-harness-only unless a leg's root cause lands in production (then TDD, own commit, and the watchlist issue says so).
3. Bookkeeping: draft each disposition (stage 3 posts): #521/#524/#549 close as fixed-with-root-cause; #287/#336/#384 per their plans (fix where root-caused this pass; refresh capture policies and keep open where not; archive-close only with recurrence-free evidence per the watchlist policy).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: repro outcomes, the fix, stress + isolation results, drafted dispositions, any deviation from the plan and why.
```

**Prompt 1.10.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 1.10 of backlog round 2 (branch test/pty-flake-pass).

1. Read AGENTS.md §5/§7, the six issues, the dispatch plan §1/§2/§3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — the classic flaky-fix failures are THE findings to hunt: a timeout constant enlarged instead of a budget injected/derived; a poll without a deadline; any touched assertion weakened relative to its issue's original semantics; production changes without their own TDD commit; a watchlist issue closed without a disposition comment that stands alone (a reader must reconstruct the evidence from the comment alone).
3. Re-run the stress profile for every fixed leg yourself (or state why not), plus the full gate suite; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled test(app): the pty harness budget seam, checks green, mergeStateStatus CLEAN). Post the dispositions: close #521, #524, #549 as fixed; handle #287/#336/#384 per their drafted outcomes.
5. Teardown per §2 step 3.

Report: verdict, per-leg dispositions, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 4. Batch 2 (five parallel chunks; begins when batch 1 fully merges)

### Chunk 2.1 — row-diff mirror (#464) (`perf/ui-row-diff-mirror`) — gate: 1.3 merged

- **Covers** (normative): #464 — `PushModels` keeps a Rust-side
  mirror `Vec<T>` beside each persistent handle; the diff compares
  desired vs. mirror slice (no `row_data` clone round-trip), fires
  `set_row_data` only where they differ, and updates model and mirror
  per mutation; same lifetime and pruning rule as `column_tasks`'
  `retain`; the repetition-semantics probe and diff unit tests stay
  green unmodified or are strengthened, never weakened. Composes with
  #463's landed guard (the mirror sits after the guard).
- **Acceptance**: no `row_data` reads in the diff path; stable-model
  identity invariants intact (repetition probe, F8/F11 journeys);
  gate suite green.

**Prompt 2.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.1 of backlog round 2 ("row-diff mirror", branch perf/ui-row-diff-mirror) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 1.3's PR (#463, perf/ui-unchanged-refresh-guard) is merged into origin/main. If not: STOP and report instead of planning.

1. Read AGENTS.md, GitHub issue #464 (normative — the row_data-by-value contract, the ~13 000 clones at N = 10 000, the push-pass-private mirror design), and .artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md §3.7.
2. Read the seams: crates/taskboard-ui-slint/src/models.rs (push_rows, diff_rows, the PushModels struct and its per-surface slots, the column_tasks retain pruning), the repetition-semantics probe test, the diff unit tests, and the #463 guard you compose with (where the pass short-circuits relative to the diff).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b perf/ui-row-diff-mirror ../taskboard-WT-perf-ui-row-diff-mirror off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-row-diff-mirror-plan.md: the mirror's home in PushModels (per-slot Vec<T> beside each handle), the new diff algorithm (compare desired vs. mirror slice, fire set_row_data only on difference, update model AND mirror per mutation), the initialization story (a fresh slot's mirror starts empty in lockstep with the model), the pruning rule (mirror retain mirrors the handle retain exactly), and the audit table of every site that mutates a live model outside push_rows (each must update the mirror or be refactored through it — divergence is the hazard). Decide the optional PartialEq question from the issue (skip the decorations-handle comparison only if trivially expressible — keep it simple otherwise). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): row-diff mirror. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.1 of backlog round 2 ("row-diff mirror").

1. Read AGENTS.md, issue #464, the dispatch plan §1/§2/§4 chunk 2.1, and the stage-1 plan in ../taskboard-WT-perf-ui-row-diff-mirror (branch perf/ui-row-diff-mirror). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the mirror lands per slot, the diff switches to the mirror slice, the mutation audit's every outside-push site is routed through the mirror-updating path. Hard rules: no row_data reads remain in the diff path (grep-verified); the repetition probe and F8/F11 journeys pass UNMODIFIED (strengthening is allowed, weakening is a deviation to revert); the pruning retains mirror and handle in the same pass so they can never disagree on membership.
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. No CHANGELOG (no user-observable change). No push, no PR. Never push to or merge main.

Report: per-gate results, the mutation-audit outcome table, any deviation from the plan and why.
```

**Prompt 2.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.1 of backlog round 2 (branch perf/ui-row-diff-mirror).

1. Read AGENTS.md, issue #464, the dispatch plan §1/§2/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — mirror/model divergence is THE hazard: hunt for any path that mutates a live VecModel without updating its mirror (set_row_data, row pushes outside push_rows, the prune/retain paths) — one miss corrupts every subsequent diff silently; check the fresh-slot initialization is atomic (a model installed without its mirror, or vice versa, breaks the first diff); confirm the stable-identity invariants (repetition probe unmodified) and that #463's guard composes (guard short-circuits before the diff; the mirror is pass-private state that never crosses passes).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled perf(ui-slint): the row-diff mirror, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #464 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.2 — audit cleanups: domain + state (#270 + #271) (`chore/audit-cleanups-domain-state`) — gate: 1.9 merged

- **Covers** (normative): #270 (push.rs module-map entry; R2/R7/R8
  rule consts + per-rule verdict events, or a documented umbrella
  mapping at the merge.rs site; `MaterializedOp::is_noop` consumed at
  push_exec or demoted test-only + false-case assert) and #271 (the
  FlakyRepository fake hoisted to `tests/common/mod.rs` keeping
  `rearm`; one `sync_status_action` helper for the duplicated
  SyncStatus construction; named structs mirroring `EngineParts` for
  the Seams/TickSeams tuples; F4-09 no action).
  **Re-verification mandate**: findings date to 2026-10-17 — the
  DR-4/DR-5/undo waves churned exactly these files; re-check every
  finding against the current tree first (fixed-already ⇒ a ✅ comment
  on the issue instead of a diff hunk).
- **Acceptance**: every finding addressed (fixed or annotated with a
  reason); zero behavior changes; suite green.

**Prompt 2.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.2 of backlog round 2 ("audit cleanups domain + state", branch chore/audit-cleanups-domain-state) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 1.9's PR (#555, feat/undo-tracking-habit-arms) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, then issues #270 and #271 (normative — each finding by F-number) and the audit reports they cite (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-domain-report.md F3-01/02/03 and .artifacts/reports/2026-10-16-v0.1.0-audit-crate-state-report.md F4-05/06/08).
2. RE-VERIFY every finding against the CURRENT tree first — the DR-4/DR-5/undo waves churned merge.rs, push.rs, the engine, and the state tests since 2026-10-17. For each finding record: fixed-already (cite the commit) / still-drifted / drifted-further. This re-verification table is the plan's core. Then read the current seams: crates/taskboard-domain/src/push.rs (its module-map entry in the crate's lib docs), merge.rs (the R2/R7/R8 verdict logging — the RULE_R4 umbrella mapping the issue names; note the DR-4 round may have restructured this), MaterializedOp::is_noop and push_exec.rs's call site, crates/taskboard-state tests' FlakyRepository fakes (both copies, their rearm), the duplicated SyncStatus constructions, and the Seams/TickSeams positional tuples vs. EngineParts.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b chore/audit-cleanups-domain-state ../taskboard-WT-chore-audit-cleanups-domain-state off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-audit-cleanups-domain-state-plan.md: the re-verification table, the per-finding fix (or annotate) decision, the consts/events design for R2/R7/R8 IF still applicable (rule consts + per-rule verdict events vs. documenting the umbrella mapping — the issue allows either; decide), the is_noop disposition (consume at the current push_exec call site or demote test-only + false-case assert — decide), the fake hoist (one copy in tests/common/mod.rs, rearm kept, both suites consume it), the sync_status_action helper shape, the named seam structs (mirroring EngineParts' style). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): domain and state audit cleanups. Do not implement; do not push.

Report: plan path, the re-verification table, decisions, risks.
```

**Prompt 2.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.2 of backlog round 2 ("audit cleanups domain + state").

1. Read AGENTS.md, issues #270/#271, the dispatch plan §1/§2/§4 chunk 2.2, and the stage-1 plan in ../taskboard-WT-chore-audit-cleanups-domain-state (branch chore/audit-cleanups-domain-state). Recreate the worktree on that branch if missing.
2. Execute exactly per the approved plan, one logical commit per finding-cluster, every commit standalone. Hard rules: ZERO behavior changes (the rule consts/events add precision to logging only — a changed verdict or merge outcome is a bug in this chunk; the round-trip and merge pins prove it); the fake hoist keeps rearm semantics and both consuming suites green unmodified; the named seam structs replace positional tuples at every use site (no mixed representation left behind); grep-audit that no stale reference survives each rename/move.
3. Bookkeeping: for findings the re-verification found already fixed, prepare the ✅ comment text (stage 3 posts).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No CHANGELOG. No push, no PR. Never push to or merge main.

Report: per-gate results, per-finding outcomes, any deviation from the plan and why.
```

**Prompt 2.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.2 of backlog round 2 (branch chore/audit-cleanups-domain-state).

1. Read AGENTS.md, issues #270/#271, the dispatch plan §1/§2/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: "cleanup" commits that smuggle behavior (diff every merge/push hunk: only logging precision, naming, and test scaffolding may change); the R2/R7/R8 consts/events must map to the CURRENT rule ids (the DR-4 round renumbered/restructured — an F-number applied blind can name dead rules); is_noop's disposition must match the plan (a dead pub fn with a surviving mutant was the original complaint — whatever disposition was chosen, the mutant outcome must be recorded); the hoisted fake's rearm call sites all compile against the single copy.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled chore(domain,state): the audit small cleanups, checks green, mergeStateStatus CLEAN). Close #270 and #271, posting the prepared ✅ comments for already-fixed findings.
5. Teardown per §2 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.3 — audit cleanups: storage + sync (#272 + #273) (`chore/audit-cleanups-storage-sync`) — gate: 1.1 merged

- **Covers** (normative): #272 (duplicated inline rationale pair in
  error.rs; the stale Phase-5 doc in the actor; the drifted "eight
  SELECTs" allow-comment; arity-blind `nums[0..2]` decode mappers →
  `Corrupted` via slice patterns; F5-10's domain-side text-form
  unification explicitly deferred — its own M refactor, not here;
  F5-11/F5-08 recorded, no action) and #273 (crate description
  "WebDAV/REST" reword + the AGENTS §2 row; architecture.org crate
  row sentence; `DeckError::Envelope` → `Json` rename or re-doc both
  directions; `GroupOutcome`/`PushOutcome` doc cross-links).
  **Re-verification mandate** as chunk 2.2.
- **Acceptance**: findings addressed/annotated; decode mappers
  arity-safe (malformed row yields `Corrupted`, never a panic —
  pinned); renames complete (grep-audited); suite green.

**Prompt 2.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.3 of backlog round 2 ("audit cleanups storage + sync", branch chore/audit-cleanups-storage-sync) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 1.1's PR (#448, fix/anchor-filename-fallback) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, then issues #272 and #273 (normative — findings by F-number) and the audit reports they cite (.artifacts/reports/2026-10-16-v0.1.0-audit-crate-storage-sqlite-report.md F5-04/05/06/07/10/11 and .artifacts/reports/2026-10-16-v0.1.0-audit-crate-sync-nextcloud-report.md F6-12/13/14/15 + F12-07).
2. RE-VERIFY every finding against the CURRENT tree (the anchor chunk, the DR-1 reminder-sync sidecar work, and the #448 fix churned model.rs, anchor.rs, actor.rs, error.rs since 2026-10-17). Record the re-verification table: fixed-already (cite commit) / still-drifted / drifted-further. Then read the current seams: crates/taskboard-storage-sqlite/src/error.rs (the duplicated rationale pair), the actor doc block, repo.rs's SELECT allow-comment, codec.rs's decode mappers (the nums[0..2] arity-blind sites), crates/taskboard-sync-nextcloud/src/lib.rs description + the Envelope/Json surfaces + GroupOutcome vs. domain PushOutcome, AGENTS §2's table row, docs/architecture.org's crate row.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b chore/audit-cleanups-storage-sync ../taskboard-WT-chore-audit-cleanups-storage-sync off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-audit-cleanups-storage-sync-plan.md: the re-verification table; per-finding fix decisions; the Envelope disposition (rename to Json vs. re-doc both directions — the issue allows either; decide, weighing churn against the week's sync changes); the slice-pattern shapes for the arity-blind mappers (what a malformed arity row yields — Corrupted, pinned how); the docs edits (crate description, AGENTS §2 row, architecture.org sentence, cross-links). F5-10 explicitly deferred: record it as not-here. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): storage and sync audit cleanups. Do not implement; do not push.

Report: plan path, the re-verification table, decisions, risks.
```

**Prompt 2.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.3 of backlog round 2 ("audit cleanups storage + sync").

1. Read AGENTS.md, issues #272/#273, the dispatch plan §1/§2/§4 chunk 2.3, and the stage-1 plan in ../taskboard-WT-chore-audit-cleanups-storage-sync (branch chore/audit-cleanups-storage-sync). Recreate the worktree on that branch if missing.
2. Execute exactly per the approved plan, one logical commit per finding-cluster, every commit standalone. Hard rules: the decode-mapper fix is TDD (first the failing test feeding a malformed-arity row, then the slice patterns mapping it to Corrupted — never a panic); ZERO behavior changes beyond that mapper hardening; the rename (if chosen) is grep-audited complete including docs comments; the AGENTS §2 row edit says exactly what the code now is (Deck REST client, no WebDAV).
3. Bookkeeping: prepare ✅ comment texts for already-fixed findings (stage 3 posts).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No CHANGELOG. No push, no PR. Never push to or merge main.

Report: per-gate results, per-finding outcomes, the mapper test evidence, any deviation and why.
```

**Prompt 2.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.3 of backlog round 2 (branch chore/audit-cleanups-storage-sync).

1. Read AGENTS.md, issues #272/#273, the dispatch plan §1/§2/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the mapper hardening must not tighten ACCEPTED shapes (a row that decoded fine yesterday must still decode — the negative test names the malformed shape, the positive suite proves no collateral); the rename disposition applied consistently (half a rename is worse than none — grep the whole workspace); the AGENTS §2 row and architecture.org sentences must not contradict each other or the crate docs; F5-10 NOT slipped in (its absence is correct — a text-form unification in this diff is a finding).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled chore(storage,sync): the audit small cleanups, checks green, mergeStateStatus CLEAN). Close #272 and #273 with the prepared annotations.
5. Teardown per §2 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.4 — window-geometry persistence (#412) (`feat/window-geometry-persistence`) — gates: 1.3, 1.6 merged

- **Covers** (normative): #412 — persist the main window's
  size/position across restarts (the cross-restart counterpart of r1
  D15's within-session stability). Stage 1 decides the storage home
  (device-local config section vs. a small state file — geometry is
  per-device by design, NEVER synced) and the invalid-geometry story
  (off-screen saves after monitor changes must clamp sanely).
- **Acceptance**: geometry survives restart (journey-pinned at a
  typed seam — no real windowing in tests); degenerate/off-screen
  values clamp (pinned); kiosk mode unaffected (fullscreen owns
  geometry — pinned or explicitly out-of-path); CHANGELOG entry.

**Prompt 2.4.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.4 of backlog round 2 ("window-geometry persistence", branch feat/window-geometry-persistence) in /home/viktoria/src/taskboard.

GATES first: git fetch origin --prune and verify BOTH chunk 1.3's (#463) and chunk 1.6's (#194) PRs are merged into origin/main. If either is not: STOP and report.

1. Read AGENTS.md, GitHub issue #412 (normative), the r1 design plan §5 D15 + §13 (.artifacts/plans/2026-10-22-gui-review-round1-plan.md — the within-session stability this extends), and the A1 policy (docs/coding_guidelines.org §2.1) for any persisted value's typing.
2. Read the seams: the window creation/mount path in crates/taskboard-ui-slint/src/adapter.rs (where initial size/position are set and where the app learns the window's current geometry), the kiosk/fullscreen path (where geometry must NOT be restored), crates/taskboard-app/src/config.rs (the write precedents: AppConfig::write_path, the logout flow; the [preferences] section family as the device-local home precedent), and how UiSettings hands values over the mount boundary (the settings form's landed r3 shape).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/window-geometry-persistence ../taskboard-WT-feat-window-geometry-persistence off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-window-geometry-persistence-plan.md: the storage home decision (recommend a device-local config section via the established write path — a separate state file only with a recorded reason), the persisted shape (a hand-rolled typed struct with a fallible mint from the raw values — A1; degenerate: zero/negative/off-screen dims rejected at mint and defaulted), the save trigger (on graceful exit vs. debounced on change — decide), the restore path (typed handover at mount, clamped to a sane on-screen rect), the kiosk exclusion (fullscreen owns geometry — the path is structurally bypassed, pinned), and the test catalogue (round-trip at the seam, mint rejections, clamp behavior — no real windowing in tests). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): window geometry persistence. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.4.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.4 of backlog round 2 ("window-geometry persistence").

1. Read AGENTS.md, issue #412, the dispatch plan §1/§2/§4 chunk 2.4, and the stage-1 plan in ../taskboard-WT-feat-window-geometry-persistence (branch feat/window-geometry-persistence). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the typed geometry value with its fallible mint, the save trigger, the restore handover with clamping, the kiosk bypass. Hard rules: the geometry value is NEVER carried through the sidecar/sync (device-local by design — grep the diff: nothing under taskboard-domain touches it); the UI crate stays config-free (the value crosses the mount boundary as a typed parameter per the established UiSettings pattern); validation lives app-side at the mint; no real window system calls in tests (the seam is typed).
3. CHANGELOG: one feature entry. Update the config guide if the plan's storage home adds a section.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 2.4.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.4 of backlog round 2 (branch feat/window-geometry-persistence).

1. Read AGENTS.md, issue #412, the dispatch plan §1/§2/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the off-screen/clamp story is where persistence features rot (a monitor unplug must never strand the window off-screen: verify the clamp pins actually reject an off-screen restore, not merely clip it); the save path must not fight the config file's other writers (concurrent [preferences]-family writes, the logout flow — read the write path for a clobber hazard); the kiosk bypass is structural (a kiosk boot cannot accidentally restore geometry); the UI layer renders what it is handed (no config reads under taskboard-ui-slint — the SRP line).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(app,ui-slint): the window geometry persists, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #412 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 2.5 — timers & alarms surface (#352) (`feat/timers-alarm-surface`) — gate: 1.3 merged; normative: the DR-3 plan

- **Covers** (normative): #352 + the DR-3 plan's timers rulings — the
  thin-client timers composition: active alarms with remaining time
  (the overdue-banner clock precedent), fired-not-yet-acknowledged
  alarms with snooze/cancel affordances dispatching the existing
  `SnoozeReminder`/`CancelAdhocAlarm` commands through the action
  layer, and a creation affordance using the `--in`/`--at` duration
  grammar. The escalation/alarm-window half already landed (DR-3/4,
  commit `112bb6b5`) — stage 1 confirms remaining scope against the
  DR-3 plan so nothing double-lands. Territory: today/chrome (§1
  lane 2); expect `models.rs` rebase friction with the render lane.
- **Acceptance**: the timers list renders live state only
  (`AppState.reminders.alarms`); ack affordances dispatch existing
  commands (no new command surface); creation matches the CLI's
  duration grammar; kiosk banner untouched (task-only by contract);
  goldens/journeys updated; CHANGELOG entry.

**Prompt 2.5.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2.5 of backlog round 2 ("timers and alarms surface", branch feat/timers-alarm-surface) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 1.3's PR (#463) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #352 (normative), and the DR-3 plan .artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md (the timers rulings — D-R9's scope and whatever the round ruled about this surface; the escalation/alarm-window half already landed, so enumerate what REMAINS: the active-timers list, the fired-ack affordances beyond the landed alarm window, the creation affordance).
2. Read the seams: crates/taskboard-domain/src/reminders.rs (ReminderState.alarms — the BTreeMap of AdhocAlarm {id, label, fire_at, created_at}, the snooze chain, fired-ness as delivery set-membership), the landed escalation driver + alarm window (what surface already exists — do not rebuild it), the overdue banner's remaining-time clock precedent (the UI's own repaint timer), the action layer's SnoozeReminder/CancelAdhocAlarm dispatch, the CLI's timers add --in/--at grammar (the creation affordance must match it), today-view.slint + models.rs push paths (territory: today/chrome), and the kiosk banner contract (task-only).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/timers-alarm-surface ../taskboard-WT-feat-timers-alarm-surface off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-timers-alarm-surface-plan.md: the remaining-scope table (what the DR-3/4 landing already covers vs. what this chunk adds), the composition shape (list rows: label, remaining time, snooze/cancel affordances), the remaining-time mechanism (UI-side clock + repaint per the banner precedent; time-dependent tests pause/advance, never sleep), the creation affordance (grammar parity with the CLI --in/--at; where it dispatches), the kiosk gate decision (the banner is task-only by contract — decide whether this surface hides on kiosk per the DR-5/4 precedent), the new push function's home in models.rs (expect rebase friction with the render lane — keep the hunk isolated), and the test catalogue (component journeys + goldens). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): timers and alarms surface. Do not implement; do not push.

Report: plan path, the remaining-scope table, decisions, risks.
```

**Prompt 2.5.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2.5 of backlog round 2 ("timers and alarms surface").

1. Read AGENTS.md, issue #352, the DR-3 plan, the dispatch plan §1/§2/§4 chunk 2.5, and the stage-1 plan in ../taskboard-WT-feat-timers-alarm-surface (branch feat/timers-alarm-surface). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the composition, its push function, the affordance dispatches, the creation affordance. Hard rules: the surface renders STATE ONLY (AppState.reminders.alarms and delivery membership — no local timers-of-truth, no nagging policy in the UI: the dumb-client line is the SRP boundary); every affordance dispatches an EXISTING command (SnoozeReminder/CancelAdhocAlarm — a new command variant is a deviation to report, never make); the creation affordance's grammar matches the CLI's exactly (one parser's semantics, not two); time-dependent tests use pause/advance; the kiosk banner file untouched.
3. Update goldens/journeys per the plan; CHANGELOG: one feature entry.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 2.5.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 2.5 of backlog round 2 (branch feat/timers-alarm-surface).

1. Read AGENTS.md, issue #352, the DR-3 plan, the dispatch plan §1/§2/§4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: double-landing (the alarm window landed already — the diff must not rebuild it; check the remaining-scope table against the actual hunks); the remaining-time display derives from state + the injected clock (a UI-side wall-clock read is a finding — the banner precedent is DIRECT-JUSTIFIED display-only, match its exact pattern or improve on it); fired-but-unacked state comes from delivery membership (a local "fired" flag is a divergence); the kiosk gate matches the plan's decision; models.rs hunks stay isolated enough to rebase over the render lane without semantic conflicts.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(ui-slint): the timers and alarms surface, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #352 with a self-contained comment (the remaining-scope table + what landed).

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 5. Batch 3 (six parallel chunks; begins when batch 2 fully merges)

### Chunk 3.1 — kiosk board i16 overflow (#462, rider #536) (`fix/kiosk-board-i16-overflow`) — gate: 2.1 merged (render lane)

- **Covers** (normative): #462 — the board panics (slint 1.18.1
  software renderer's i16 geometry space + `overflow-checks = true`)
  once a column's live content crosses 32 767 px (~2.5k tasks). Fix
  levers per the issue, in order: (1) the #373 ListView mechanism
  (structural — viewport-only instantiation), (2) `clip: true`
  (a visual-design decision — goldens for over-deep columns),
  (3) upstream slint. **Stage 1 owns the lever decision** and the
  golden question. **Rider #536**: one upstream slint report
  artefact covering both the i16 raster facts (this issue's
  mechanism notes + why Today's ~183k px list survives where the
  board's column scene does not) and the testing-backend
  empty-element-query limitation; posting upstream stays the user's.
- **Acceptance**: a ≥2.5k-task board renders on the software renderer
  (bench evidence at the formerly-panicking scale); no panic at the
  N = 10 000 draw lanes; the bench's 400-rows/column loud-skip bound
  updated; board journeys/goldens updated honestly; the upstream
  report lands (diff = code + that report only).
- **Evidence & artefact mandate** applies to the upstream report.

**Prompt 3.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.1 of backlog round 2 ("kiosk board i16 overflow", branch fix/kiosk-board-i16-overflow) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 2.1's PR (#464, perf/ui-row-diff-mirror) is merged into origin/main (the render lane is strictly serial). If not: STOP and report.

1. Read AGENTS.md, then GitHub issues #462 and #536 (normative — the i16 mechanism facts, the empirical bracket, the Today-view contrast, the fix levers), the ListView design-round report .artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md (§2 routes, §3c scroll semantics, §4 what moves), the view-pinned re-measure .artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md, and the #373 board round's landed mechanism (the in-house ListView component and its pinned walk/focus/IdTables semantics).
2. Read the seams: crates/taskboard-ui-slint/ui/compositions/board-layout.slint (the unclipped column body, the drop-zone repeater the backtrace names), the board push path in models.rs, the landed ListView component and how the Today/board chunks consumed it, and the bench (its 400-rows/column safe bound, the loud-skip rule, the draw lanes).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b fix/kiosk-board-i16-overflow ../taskboard-WT-fix-kiosk-board-i16-overflow off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-kiosk-board-i16-overflow-plan.md: the lever decision (recommend the #373 structural route — virtualised columns; `clip: true` only with a recorded reason and the golden story for over-deep columns), what the mechanism needs adapting for board columns (drop zones, the reading-order walk, IdTables board-relative indexing — the #373 round's pinned semantics are binding), the golden question (what over-deep columns look like now: viewport-only), the bench update (the safe bound, which lanes re-run, the formerly-panicking scale to prove), and the upstream report skeleton (i16 facts + the Today contrast + the #536 testing-backend limitation; in-repo artefact only, posting upstream is the user's). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): kiosk board i16 overflow. Do not implement; do not push.

Report: plan path, the lever decision, decisions, risks.
```

**Prompt 3.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.1 of backlog round 2 ("kiosk board i16 overflow").

1. Read AGENTS.md, issues #462/#536, the dispatch plan §1/§2/§5 chunk 3.1, and the stage-1 plan in ../taskboard-WT-fix-kiosk-board-i16-overflow (branch fix/kiosk-board-i16-overflow). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the board columns onto the ListView mechanism (adapting drop zones, walk, and indexing per the plan), the bench bound update, the goldens. Then write the upstream report at .artifacts/reports/2026-10-31-slint-upstream-i16-and-testing-backend-report.md per the plan's skeleton (evidence mandate: every claim cites the renderer source lines, the empirical bracket, or the #536 observation). Hard rules: the #373 round's pinned semantics (walk order, focus restoration, IdTables mapping) hold or are consciously re-pinned with reasons — a silent change there is a deviation to revert; the bench's loud-skip stays loud (never a hidden cell); journeys/goldens updated honestly (a regenerated golden without a named reason is a finding).
3. Prove the fix: re-run the plan's bench lanes at the formerly-panicking scale (N = 2 600 and 10 000) and record the outcome — no panic, draw cost viewport-bounded.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. CHANGELOG: one bug-fix entry (the kiosk panic). No push, no PR. Never push to or merge main.

Report: per-gate results, the bench evidence at scale, the report path, any deviation from the plan and why.
```

**Prompt 3.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.1 of backlog round 2 (branch fix/kiosk-board-i16-overflow).

1. Read AGENTS.md, issues #462/#536, the dispatch plan §1/§2/§5 (the code-quality mandate and the evidence & artefact mandate apply), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the panicking arithmetic must be structurally unreachable (viewport-only instantiation means no 32k-px-deep live geometry — verify the column's live content is actually viewport-bounded, not merely clipped); the walk/focus/IdTables pins (mentally walk a board after the change — order and restoration must match the #373 round's pins or the new pins in the plan); the bench lanes at N = 2 600/10 000 really ran (transcripts, not assertions); the upstream report's claims each carry citations (evidence mandate — an uncited renderer claim is a finding); the diff is exactly code + the one report.
3. Re-run the full gate suite and one bench lane yourself (or state why not).
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled fix(ui-slint): the board survives deep columns, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #462 and #536 (the latter: documented + upstream draft filed; posting upstream is the user's action — say so in the close comment).

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

### Chunk 3.2 — Unreadable 4xx/5xx split (#401) (`feat/appdata-unreadable-split`) — gate: 2.3 merged

- **Covers** (normative): #401 — additive `AppDataUnreadable`
  variants (or a payload carrying the typed `SyncErrorKind`) plus
  the mapping in the sync actor's `sidecar_evidence`; the document
  cache stays untouched on every failure form (the design's own
  invariant), so widening the enum cannot change ingest semantics.
- **Acceptance**: a 4xx download failure and a 5xx map to distinct
  typed evidence (pinned at the mapping boundary); wire/snapshot
  additions additive; suite green.

**Prompt 3.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.2 of backlog round 2 ("appdata unreadable split", branch feat/appdata-unreadable-split) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 2.3's PR (#272/#273, chore/audit-cleanups-storage-sync) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #401 (normative), and the sidecar design plan .artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md (D-S4's evidence table — the design named only the Transport row).
2. Read the seams: crates/taskboard-domain/src/messages.rs (AppDataEvidence / AppDataUnreadable — the enum the report carries), crates/taskboard-sync-nextcloud/src/actor.rs (sidecar_evidence — where every download failure maps to Transport today), the typed SyncErrorKind the payload option would carry, and the existing evidence tests (what pins the mapping today).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/appdata-unreadable-split ../taskboard-WT-feat-appdata-unreadable-split off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-appdata-unreadable-split-plan.md: the shape decision (new variants vs. a SyncErrorKind payload — decide with reasons; the issue shows both are additive), the mapping table (which client outcomes land in which variant; anything unmapped falls back to today's Transport — the conservative default), the pin catalogue (4xx vs 5xx distinct at the mapping boundary; the cache-untouched invariant pinned per form), and the wire/snapshot impact (additive only — old reports still decode). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): appdata unreadable evidence split. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.2 of backlog round 2 ("appdata unreadable evidence split").

1. Read AGENTS.md, issue #401, the dispatch plan §1/§2/§5 chunk 3.2, and the stage-1 plan in ../taskboard-WT-feat-appdata-unreadable-split (branch feat/appdata-unreadable-split). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the enum widening and the mapping, pin-first. Hard rules: the document cache path untouched on EVERY failure form (the invariant the design names — a cache eviction on a failure form is a bug in this chunk); the mapping is total (an unmapped client outcome falls to Transport, never panics); old serialized reports still decode (additive enum, the round-trip pins prove it).
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. CHANGELOG only if the evidence shapes are user-visible in reports — per the plan's decision. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.2 of backlog round 2 (branch feat/appdata-unreadable-split).

1. Read AGENTS.md, issue #401, the dispatch plan §1/§2/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: ingest semantics unchanged (the cache-untouched invariant per form — diff the cache paths); the mapping table matches the plan exactly (a 403 landing in a "persistent fault" bucket would misroute the runbook decision the issue motivates); nothing non-additive in the enum's serialized shape.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(sync): the appdata unreadable evidence split, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #401 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 3.3 — delivery-proof pruning (#201) (`feat/delivery-proof-pruning`) — gate: 2.2 merged

- **Covers** (normative): #201 — a pruning rule in the delivery
  interpreter path (drop deliveries whose occurrence is older than N
  days at record time), defined in the domain's shared
  `apply_actions` so fakes and sqlite stay in lockstep (audit F16-20:
  mechanical; the `DeleteReminderRule` cascade precedent exists).
  Stage 1 picks N with a recorded rationale (conservative default
  recommended) and argues the reachability window (a pruned row's
  at-most-once suppression must be unreachable for re-arm).
- **Acceptance**: pruning deterministic (pause/advance tests, no wall
  reads); fakes and sqlite execute the same domain rule (the shared
  `apply_actions` pin); set-union merge unaffected for live
  occurrences; suite green.

**Prompt 3.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.3 of backlog round 2 ("delivery-proof pruning", branch feat/delivery-proof-pruning) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 2.2's PR (#270/#271) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #201 (normative), the audit's F16-20 note (the cascade precedent), and the reminders design plan's delivery-suppression section (what the (rule_id, occurrence) rows protect).
2. Read the seams: the delivery interpreter path in the domain's shared apply_actions (where delivery rows are recorded — the one place fakes and sqlite provably share semantics), the re-arm horizon (how far in the future occurrences can be scheduled — the reachability argument's backbone), persistence.rs's DeleteReminderRule cascade (the precedent shape), the storage execution + the fakes' delivery sets, and the merge side (set-union over occurrence-keyed rows — pruning must not interact).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/delivery-proof-pruning ../taskboard-WT-feat-delivery-proof-pruning off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-delivery-proof-pruning-plan.md: N with its rationale (recommend a conservative default — e.g. occurrences older than 90 days at record time — and argue it against the re-arm horizon: the largest legal re-arm distance must be < N, or the argument for why a longer re-arm's suppression loss is acceptable); the rule's exact placement in apply_actions (at record time per the issue); the reachability argument written out; the pin catalogue (an old-occurrence delivery is pruned at record, a live one is not; a fake and sqlite agree on the same scenario — the lockstep pin; set-union merge over pruned sets stays conflict-free); the determinism story (the domain's clock seam, pause/advance — no wall reads). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): delivery-proof pruning. Do not implement; do not push.

Report: plan path, N and its rationale, decisions, risks.
```

**Prompt 3.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.3 of backlog round 2 ("delivery-proof pruning").

1. Read AGENTS.md, issue #201, the dispatch plan §1/§2/§5 chunk 3.3, and the stage-1 plan in ../taskboard-WT-feat-delivery-proof-pruning (branch feat/delivery-proof-pruning). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the pruning action in the shared apply_actions, pin-first (old occurrence pruned at record; live occurrence untouched; fake and sqlite agree on the same scenario — the lockstep pin). Hard rules: no wall-clock reads (the domain clock seam, pause/advance in tests); no storage schema change (rows already exist — pruning is DELETEs through the existing action path); the merge side untouched (set-union over whatever rows exist is conflict-free by construction — verify no merge test needed changing).
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. No CHANGELOG (housekeeping, not user-observable). No push, no PR. Never push to or merge main.

Report: per-gate results, the lockstep-pin evidence, any deviation from the plan and why.
```

**Prompt 3.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.3 of backlog round 2 (branch feat/delivery-proof-pruning).

1. Read AGENTS.md, issue #201, the dispatch plan §1/§2/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the suppression-loss window is THE correctness question (re-read the reachability argument against the actual re-arm horizon in code — if a legal re-arm can exceed N, the plan's argument must cover it or the chunk is wrong); pruning at RECORD time only (a bulk retro-prune path would be scope creep); the lockstep pin is load-bearing (fakes and sqlite executing DIFFERENT pruning logic would pass their own suites and diverge in the field — verify both go through apply_actions).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(domain): delivery proofs prune by age, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #201 with a self-contained comment (N, rationale, evidence).

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```
### Chunk 3.4 — tracked-time surfaces (#490 + #491) (`feat/tracked-time-surfaces`) — gates: 2.1, 2.5 merged

- **Covers** (normative): #490 — a today-view "Tracked" section:
  per-task tracked-today totals and the day total, derived from time
  entries via the DR-5 `tracked_in_window` pure projection (spanning
  entries split at the `[preferences] day_start_time` window
  boundary); follows the today-view section pattern; empty section
  collapses. #491 — a per-task entry-history list inside the task
  detail view (start/end/duration rows) with adjust/delete
  affordances dispatching `AdjustTimeEntry`/`DeleteTimeEntry`
  (until now CLI-only). Normative: the DR-5 plan's D-T rulings.
- **Acceptance**: totals derived purely (projection unit + property
  tests; window-boundary splits pinned); affordances dispatch
  existing commands only; kiosk gate per the DR-5 UI precedent;
  goldens/journeys; CHANGELOG.

**Prompt 3.4.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.4 of backlog round 2 ("tracked-time surfaces", branch feat/tracked-time-surfaces) in /home/viktoria/src/taskboard.

GATES first: git fetch origin --prune and verify BOTH chunk 2.1's (#464) and chunk 2.5's (#352) PRs are merged into origin/main. If either is not: STOP and report.

1. Read AGENTS.md, GitHub issues #490 and #491 (normative), and the DR-5 plan .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (the UI-scope ruling that fenced these; the D-T rulings; the tracked_in_window projection's contract).
2. Read the seams: the tracked_in_window pure projection in the domain (its signature, the day_start_time window split), the today-view section pattern (Overdue/Scheduled/Due/Done in push_today + today-view.slint — the section to imitate), the task detail view's rows pattern, the AdjustTimeEntry/DeleteTimeEntry commands (the affordances dispatch these — verify their payload shapes), the tracking chip/affordances landed in DR-5/4 (the kiosk gate precedent), and models.rs's detail/today push paths (expect render-lane rebase friction; keep hunks isolated).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/tracked-time-surfaces ../taskboard-WT-feat-tracked-time-surfaces off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-tracked-time-surfaces-plan.md: the Tracked section's row model and its derivation (pure projection per task + the day total; the window-boundary split cases enumerated and pinned), the detail history list (row shape, adjust/delete affordances → existing commands), the empty-state collapse (both surfaces), the kiosk gate decision per the DR-5/4 precedent, and the test catalogue (projection property tests + boundary pins, component journeys, goldens). Two territories in one chunk (today + detail): keep the projection shared and pure, both surfaces thin. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): tracked-time surfaces. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.4.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.4 of backlog round 2 ("tracked-time surfaces").

1. Read AGENTS.md, issues #490/#491, the DR-5 plan, the dispatch plan §1/§2/§5 chunk 3.4, and the stage-1 plan in ../taskboard-WT-feat-tracked-time-surfaces (branch feat/tracked-time-surfaces). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the projection extensions (pure, property-tested — window splits pinned at the day_start_time boundary including a spanning entry), then the Tracked section, then the detail history list. Hard rules: every affordance dispatches an EXISTING command (AdjustTimeEntry/DeleteTimeEntry — no new command surface); the UI renders state only (dumb-client line); time-dependent tests pause/advance; the kiosk gate matches the plan's decision.
3. Update goldens/journeys per the plan; CHANGELOG: one feature entry.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.4.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.4 of backlog round 2 (branch feat/tracked-time-surfaces).

1. Read AGENTS.md, issues #490/#491, the DR-5 plan, the dispatch plan §1/§2/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the window-split projection is where silent wrongness lives (a spanning entry split at day_start_time must sum correctly across both windows — the property tests must cover the boundary from both sides, midnight-inclusive vs -exclusive per the landed day-window semantics); totals must derive from ENTRIES, not a parallel tally (one source of truth); the detail affordances' dispatch payloads match the commands' expectations (an adjust that silently no-ops is a finding); no nagging or auto-start logic anywhere.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(ui-slint): the tracked-time surfaces, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #490 and #491 with self-contained comments.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

### Chunk 3.5 — respite + break reminders (#493 + #495) (`feat/respite-break-reminders`) — gate: 2.2 merged

- **Covers** (normative): #493 — a respite flag/kind on time entries
  (break-activity counting), additive with a default-off fold on the
  wire (the DR-5 shape reserved the vocabulary). #495 — break
  reminders driven by accumulated tracked-work duration: a new pure
  trigger fn over time entries riding the landed reminders pipeline
  (rules/evaluator/sink), with respite-marked entries feeding the
  reset condition. Serial inside the chunk (#493 first — #495 is its
  first consumer). Stage 1 owns the break-reminder rule surface
  (how a user expresses "remind me after 90m tracked": the existing
  rule grammar vs. a new trigger-source discriminator) per the DR-5
  plan's fences.
- **Acceptance**: respite folds additively (old data reads as
  non-respite — pinned); the trigger fn is pure and proptest'ed
  (duration accumulation incl. respite resets); evaluator/sink
  untouched beyond the trigger arm; a CLI surface for marking
  respite; CHANGELOG.

**Prompt 3.5.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.5 of backlog round 2 ("respite + break reminders", branch feat/respite-break-reminders) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 2.2's PR (#270/#271) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issues #493 and #495 (normative), and the DR-5 plan .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (the fences this chunk lands; the reserved vocabulary on TimeEntry and its wire row; D-T rulings about the trigger surface).
2. Read the seams: the TimeEntry entity + its fold + its wire row (where the additive kind/flag lands with default-off decoding of old data), the reminders pipeline (rule grammar, the evaluator's trigger arms — the issue says the evaluator stays urgency-agnostic and decides WHEN; a duration trigger is a new WHEN source: decide how it enters the rule model without disturbing the existing arms), the delivery suppression (set-union over occurrence keys — the double-fire guard the trigger must compose with), the track CLI family (the respite-marking surface rides it), and the storage entry tables (schema impact — expect none if the kind rides the existing row codec; verify).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/respite-break-reminders ../taskboard-WT-feat-respite-break-reminders off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-respite-break-reminders-plan.md: the respite shape (flag vs. kind; the default-off fold; old-data round-trip pins), the CLI marking surface (which track verb gains it), the break-reminder rule surface decision (grammar extension vs. trigger-source discriminator — decide with reasons; the evaluator's when-only contract is binding), the trigger fn (pure, over time entries + respite resets; proptest laws: accumulation monotonicity, respite resets to zero, no firing below the threshold), the occurrence semantics for the suppression guard (what counts as the occurrence key for a duration trigger — decide; the double-fire hazard), and the test catalogue. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): respite and break reminders. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.5.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.5 of backlog round 2 ("respite + break reminders").

1. Read AGENTS.md, issues #493/#495, the DR-5 plan, the dispatch plan §1/§2/§5 chunk 3.5, and the stage-1 plan in ../taskboard-WT-feat-respite-break-reminders (branch feat/respite-break-reminders). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone — #493 first (the respite kind: entity fold, wire row default-off, the track CLI marking), then #495 (the trigger fn + its pipeline arm). Hard rules: old serialized entries read as non-respite (round-trip pinned); the trigger fn is pure and proptest'ed per the plan's laws (no wall reads, no IO); the evaluator's contract untouched beyond the new trigger arm (it decides when, never how loud); the suppression guard composes (the occurrence key per the plan; the double-fire hazard pinned); no schema migration expected — if the plan's wire approach needs one, STOP and report instead of improvising (§1 migration lane).
3. CHANGELOG: one feature entry. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. No push, no PR. Never push to or merge main.

Report: per-gate results, proptest outcomes, any deviation from the plan and why.
```

**Prompt 3.5.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.5 of backlog round 2 (branch feat/respite-break-reminders).

1. Read AGENTS.md, issues #493/#495, the DR-5 plan, the dispatch plan §1/§2/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the double-fire hazard (the duration trigger's occurrence semantics must be pinned such that a re-evaluation inside the same accumulation window cannot re-notify — trace the suppression guard through one full cycle); respite resets (an entry marked respite mid-accumulation resets the clock — the proptests must actually constrain the reset path); old-data compatibility (every fold/wire touch has its reads-as-non-respite pin); the evaluator's when-only contract intact (a loudness or urgency decision anywhere in the diff is an SRP violation); the respite flag must not leak into tracked-work totals (the break-counting separation pinned or argued).
3. Re-run the full gate suite and the proptests yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(domain,app): respite marking and break reminders, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #493 and #495 with self-contained comments.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

### Chunk 3.6 — app hardening + contracts (#276 + #279 + #280) (`chore/app-hardening-contracts`) — gates: 1.6, 1.7, 1.10 merged

- **Covers** (normative): #279's five bounds/latency fixes (the
  unbounded `engine.flush()` ahead of bounded joins wrapped in the
  shutdown timeout; `join_bounded` the storage join; `spawn_blocking`
  inside `attach_from_persisted` or the latency recorded in ADR 0012;
  drain-or-document the UI forwarder's ≤64 queued intents; match the
  SYSLOG `Lagged` family) + #280 (one publish-order test — a
  subscriber observes the swapped snapshot before the signal; collapse
  the `route_for_ipc` dual enumeration so one match drives both) +
  #276 (reword the daemon-gate bail; one `authority_of` helper in the
  sync crate consumed by config validation + `credential_key`; fold
  the handler-side `io.err` write into the warning shapes).
  **Re-verification mandate**: 2026-10-17 findings — the signal
  ladder, IPC-knob and daemon rounds churned these files; re-check
  each first.
- **Acceptance**: each re-verified finding fixed or annotated; the
  publish-order test load-bearing; the dual enumeration collapsed
  (one match); shutdown bounds pinned with pause/advance where
  clock-adjacent; suite green.

**Prompt 3.6.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3.6 of backlog round 2 ("app hardening + contracts", branch chore/app-hardening-contracts) in /home/viktoria/src/taskboard.

GATES first: git fetch origin --prune and verify chunk 1.6's (#194), chunk 1.7's (#407), and chunk 1.10's (pty flake pass) PRs are merged into origin/main. If any is not: STOP and report.

1. Read AGENTS.md, then issues #279, #280, #276 (normative — findings by F-number) and the audit reports they cite (.artifacts/reports/2026-10-16-v0.1.0-audit-theme-concurrency-report.md F9-02..F9-08, .artifacts/reports/2026-10-16-v0.1.0-audit-crate-app-report.md F8-04/09/10).
2. RE-VERIFY every finding against the CURRENT tree (the daemon ladder, the IPC knob, the signal rounds, and the #450-adjacent tooling churned bootstrap.rs, notifications.rs, the dispatcher, and commands since 2026-10-17). Record the re-verification table: fixed-already (cite commit) / still-drifted / drifted-further. Then read the current seams: bootstrap.rs's shutdown sequence (the flush-before-joins shape, the storage join), notifications.rs (the forwarder's queue bound, the SYSLOG Lagged arm), ipc/ dispatcher (the route_for_ipc dual enumeration), state actor publish (swap-then-signal order), the daemon-gate bail text, the three ad-hoc URL authority parsers (config validation + credential_key), and the warning-shape helpers.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b chore/app-hardening-contracts ../taskboard-WT-chore-app-hardening-contracts off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-app-hardening-contracts-plan.md: the re-verification table; per-finding fix decisions (each of #279's five, both #280 items, all three #276 items); the publish-order test design (a subscriber observes the swapped snapshot — load-bearing: it must fail if the order flips; pause/advance, no sleeps); the enumeration collapse shape (one match drives both sites); the authority_of helper (home: the sync crate per #276's own ruling; both consumers adopt it); the drain-or-document decision for the forwarder. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): app hardening and contracts. Do not implement; do not push.

Report: plan path, the re-verification table, decisions, risks.
```

**Prompt 3.6.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3.6 of backlog round 2 ("app hardening + contracts").

1. Read AGENTS.md, issues #279/#280/#276, the dispatch plan §1/§2/§5 chunk 3.6, and the stage-1 plan in ../taskboard-WT-chore-app-hardening-contracts (branch chore/app-hardening-contracts). Recreate the worktree on that branch if missing.
2. Execute exactly per the approved plan, one logical commit per finding-cluster, every commit standalone. Hard rules: observable exit codes and the shutdown ladder's contract unchanged (a changed exit path is a deviation to revert — #279 hardens bounds, not semantics); the publish-order test is written FIRST and demonstrated load-bearing (flip the order mentally — the test must fail); the enumeration collapse leaves exactly one match site (grep-audited — a second hidden enumeration is the finding class); the authority_of helper lives in the sync crate and BOTH call sites consume it (no third parser remains); clock-adjacent tests pause/advance.
3. Bookkeeping: prepare ✅ comment texts for already-fixed findings (stage 3 posts).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No CHANGELOG. No push, no PR. Never push to or merge main.

Report: per-gate results, per-finding outcomes, the load-bearing demonstration, any deviation and why.
```

**Prompt 3.6.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 3.6 of backlog round 2 (branch chore/app-hardening-contracts).

1. Read AGENTS.md, issues #279/#280/#276, the dispatch plan §1/§2/§5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: hardening that changes observable behavior (exit codes, shutdown sequencing visible to a supervisor — the runbook's contract); the publish-order pin actually pinning (it must observe the swap BEFORE the signal — a test that passes under either order proves nothing); the collapsed enumeration still covering every StateCommand variant exhaustively (a wildcard is forbidden by the v1 catalogue's no-wildcard rule); the forwarder decision (drain vs. documented-loss) matches the plan and is stated where the code implements it.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled chore(app): hardening bounds and pinned contracts, checks green, mergeStateStatus CLEAN). Close #276, #279, #280 with the prepared annotations.
5. Teardown per §2 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issues closed, worktree removed. Never merge; never touch main.
```

## 6. Batch 4 (three parallel chunks)

### Chunk 4.1 — Today virtualisation (#461) (`perf/ui-today-virtualisation`) — gate: 3.1 merged (render lane)

- **Covers** (normative): #461 — the Today view's section lists via
  the (landed) #373 ListView mechanism; the extra acceptance the
  board did not have: **anchor-on-removal** — the Overdue list
  changes membership on every done flip, so scroll-anchor semantics
  under above-viewport insertions/removals must be pinned by tests,
  not discovered by users. Normative: the ListView design-round
  report (§2 routes, §3c scroll semantics, §4 what moves, §6 step 3).
- **Acceptance**: today draw cost decouples from list extent (bench
  lanes per the view-pinned contract); the anchor-on-removal pins;
  all today journeys/goldens green.

**Prompt 4.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4.1 of backlog round 2 ("Today virtualisation", branch perf/ui-today-virtualisation) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 3.1's PR (#462, fix/kiosk-board-i16-overflow) is merged into origin/main (the render lane is strictly serial). If not: STOP and report.

1. Read AGENTS.md, GitHub issue #461 (normative — the uncapped `for` at ui/views/today-view.slint:54, the removal-reflow damage evidence, the anchor-on-removal requirement), the ListView design-round report .artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md (§2 routes, §3c scroll semantics, §4 what moves, §6 recommendation order step 3), and the #373/#462 landed mechanism (the component, its walk/focus/IdTables pins).
2. Read the seams: today-view.slint's section rows, models.rs's push_today (what it caps today: nothing), the today journeys (focus/walk over section rows — what the virtualisation must preserve), the done-flip journey (a filtered-list removal — the anchor case), and the bench's view-pinned lanes (which lanes re-run, what N means after virtualisation).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b perf/ui-today-virtualisation ../taskboard-WT-perf-ui-today-virtualisation off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-today-virtualisation-plan.md: per-section virtualisation decisions (which sections mount the ListView mechanism; small sections may stay plain — decide per section with reasons), the anchor-on-removal semantics (what the user's scroll position means when a row above the viewport is removed by a done flip — pin the slint virtual repeater's offset behavior explicitly; the test catalogue: removal above viewport, insertion above viewport, removal of the anchored row), the walk/focus story under virtualisation (the board round's precedents), the bench lanes update (what N measures now; an unvirtualised lane kept or a size-scaled variant so board-scale cost stays visible — the design report's contract), and the goldens. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): Today virtualisation. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4.1 of backlog round 2 ("Today virtualisation").

1. Read AGENTS.md, issue #461, the ListView report, the dispatch plan §1/§2/§6 chunk 4.1, and the stage-1 plan in ../taskboard-WT-perf-ui-today-virtualisation (branch perf/ui-today-virtualisation). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the anchor-on-removal pins FIRST (they define the semantics), then the per-section mechanism. Hard rules: the today journeys pass or are consciously re-pinned per the plan (a silent focus/walk change is a deviation to revert); the bench contract per the plan (an unvirtualised or size-scaled lane survives so board-scale cost stays visible); removal reflow keeps the user's place per the pins (a jump is a bug in this chunk, not a slint quirk to accept).
3. Prove it: re-run the plan's bench lanes and record the draw-cost decoupling (viewport-bounded at scale).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. CHANGELOG: one performance entry. No push, no PR. Never push to or merge main.

Report: per-gate results, bench evidence, any deviation from the plan and why.
```

**Prompt 4.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 4.1 of backlog round 2 (branch perf/ui-today-virtualisation).

1. Read AGENTS.md, issue #461, the ListView report, the dispatch plan §1/§2/§6 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the anchor pins are the deliverable (re-read them against slint's actual repeater offset behavior — a pin that encodes the DESIRED semantics rather than the pinned actual ones will fail in the field); done-flip from within the viewport vs. above it behave per the pins; IdTables index→id mapping stays board-relative or is consciously re-mapped (the design report's §3c concern); the bench's kept lane actually kept.
3. Re-run the full gate suite and one bench lane yourself (or state why not).
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled perf(ui-slint): the Today sections virtualise, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #461 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 4.2 — tracker task kind (#492) (`feat/tracker-task-kind`) — gate: 3.5 merged

- **Covers** (normative): #492 — a never-"done" tracker kind
  suppressing done-affordances and done-triggered cascades while
  tracking stays fully live; additive kind/flag folding onto the
  entity and its wire row as a default-carrying addition (the DR-5
  reservation). Stage 1 decides the minimal domain shape and the
  done-suppression semantics per surface (typed rejection in the
  engine vs. hidden affordance in each UI: both, decided per
  surface — CLI, desktop UI, kiosk).
- **Acceptance**: done on a tracker task is impossible via the
  public API (typed outcome pinned); tracking verbs unaffected;
  cascade non-firing pinned; wire additive (old readers safe);
  CHANGELOG.

**Prompt 4.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4.2 of backlog round 2 ("tracker task kind", branch feat/tracker-task-kind) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 3.5's PR (#493/#495) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #492 (normative), and the DR-5 plan's reserved vocabulary note (the kind field on TimeEntry AND the task-kind reservation — this issue owns the TASK kind, not the entry kind #493 landed).
2. Read the seams: the task entity's field family (where a kind/flag folds additively), the SetTaskDone engine arm (where the typed rejection lives), the done-triggered cascades (what a done currently triggers — tombstone? reminder re-arm? the non-firing pins), the wire row codec (the default-carrying additive fold; old readers safe), the per-surface done affordances (CLI tasks done, the desktop task card/menu, the kiosk gate), and the CHANGELOG/guide conventions.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/tracker-task-kind ../taskboard-WT-feat-tracker-task-kind off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-tracker-task-kind-plan.md: the domain shape (minimal kind/flag with a fallible mint if validated — A1), the engine rejection (typed outcome for done-on-tracker, pinned), the cascade non-firing enumeration (each done-triggered path listed and pinned inert for tracker tasks), the per-surface affordance suppression (CLI errors typed; UI hides/kiosks — decide per surface), the wire fold (default-carrying; old rows read as normal tasks — round-trip pins), and the test catalogue. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): tracker task kind. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4.2 of backlog round 2 ("tracker task kind").

1. Read AGENTS.md, issue #492, the dispatch plan §1/§2/§6 chunk 4.2, and the stage-1 plan in ../taskboard-WT-feat-tracker-task-kind (branch feat/tracker-task-kind). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the domain kind, the engine rejection, the cascade inertness, the wire fold, the surface suppressions. Hard rules: tracking verbs (track start/stop/log) unaffected on tracker tasks (that is the feature's point — pinned); the engine rejection is typed (matches!-asserted, no text); the wire fold is default-carrying and additive (old serialized tasks decode as normal tasks — round-trip pinned; a schema migration is NOT expected — if the plan's fold needs one, STOP and report per §1's migration lane); per-surface suppression per the plan's decisions.
3. CHANGELOG: one feature entry. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
4. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 4.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 4.2 of backlog round 2 (branch feat/tracker-task-kind).

1. Read AGENTS.md, issue #492, the dispatch plan §1/§2/§6 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: every done-TRIGGERED path enumerated and pinned inert (the plan's cascade enumeration vs. the actual hunks — one missed path means a tracker task silently tombstones); the rejection at the engine boundary (a UI-only suppression with an open engine path is the bug class — verify the typed rejection exists regardless of surface); old-data compatibility (a pre-kind task must behave byte-identically to today — the round-trip and behavior pins); tracking on a tracker task unchanged.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(domain,app): the tracker task kind, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #492 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 4.3 — sidecar cross-board repair (#350) (`fix/sidecar-cross-board-repair`) — gate: 3.2 merged (sync crate)

- **Covers** (normative): #350 — when a board's fold adopts an
  unbound remote card, look up that card id in sibling-board
  documents the same connector owns (cacheable), migrate the entry
  into the destination board's document (embedded clocks carry over
  UNCHANGED), and let the origin's snapshot-absence GC drop the old
  entry. Partial by construction (single-connector visibility) —
  the issue's honesty stays in the docs. Stage 1 owns the
  sibling-lookup mechanics (cacheability, failure isolation: a
  missing sibling must degrade to today's behavior, never block
  adoption).
- **Acceptance**: the cross-board move preserves sidecar fields when
  both boards share a connector (integration-pinned at the fold
  boundary); the partial-coverage boundary documented (one
  paragraph); failure isolation pinned; suite green.

**Prompt 4.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4.3 of backlog round 2 ("sidecar cross-board repair", branch fix/sidecar-cross-board-repair) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 3.2's PR (#401) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #350 (normative — the mechanism, the proposed approach, the partial-coverage honesty), the sidecar design plan .artifacts/plans/2026-10-18-appdata-sidecar-design-plan.md (D-S8, the per-board document lanes), and the deck-roundtrip strategy report §3.2 (card ids unique per server — the lookup key's soundness).
2. Read the seams: the fold/adoption path in crates/taskboard-sync-nextcloud/src/actor.rs (where an unbound remote card becomes a fresh task with no document entry), the per-board document store (how documents are keyed, read, written; what "sibling-board documents the same connector owns" costs to enumerate — the cacheability question), the origin board's snapshot-absence GC (the drop side), the embedded clocks in document entries (they must migrate unchanged), and the existing fold tests (the integration-pin patterns).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b fix/sidecar-cross-board-repair ../taskboard-WT-fix-sidecar-cross-board-repair off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-sidecar-cross-board-repair-plan.md: the lookup mechanics (when during adoption; enumerate-siblings cost and the cacheability decision; what happens on a doc read failure), failure isolation (a missing/unreadable sibling degrades to today's behavior — adoption proceeds without the entry; pinned), the migration write (entry lands in the destination doc, clocks unchanged, byte-for-byte the same entry content), the GC interplay (the origin's absence-GC drops the old entry — no double-migration on a later cycle), the coverage boundary paragraph (where it lands: the sidecar plan's amendment note or docs — decide), and the test catalogue (the both-boards-one-connector integration pin; the no-sibling degradation pin; the clock-preservation pin; the idempotence pin — a second cycle must not re-migrate). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): sidecar cross-board repair. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.3.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4.3 of backlog round 2 ("sidecar cross-board repair").

1. Read AGENTS.md, issue #350, the dispatch plan §1/§2/§6 chunk 4.3, and the stage-1 plan in ../taskboard-WT-fix-sidecar-cross-board-repair (branch fix/sidecar-cross-board-repair). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the sibling lookup, the migration write, the GC interplay, the isolation paths — pin-first per the plan's catalogue (both-boards integration pin, degradation pin, clock preservation, idempotence). Hard rules: embedded clocks migrate UNCHANGED (a reset clock re-opens LWW races — the pin proves byte-for-byte clock equality); failure isolation is total (no lookup failure can block or abort the adoption path); the sync crate stays the sync crate (no new domain surface unless the plan recorded one); no speculative generality (the cache only if the plan chose it, sized to the plan's argument).
3. Land the coverage-boundary paragraph where the plan decided.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. CHANGELOG: one reliability entry. No push, no PR. Never push to or merge main.

Report: per-gate results, the pin outcomes, any deviation from the plan and why.
```

**Prompt 4.3.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 4.3 of backlog round 2 (branch fix/sidecar-cross-board-repair).

1. Read AGENTS.md, issue #350, the dispatch plan §1/§2/§6 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: idempotence under repetition (a second sync cycle after the migration must find NOTHING to migrate — trace the flow twice; a re-migration corrupts clocks or duplicates entries); the migration is copy-then-GC (an entry deleted from the origin doc before the destination write completes loses data — check the ordering); the degradation pin actually degrades (a sibling-enumeration error must not surface as a sync failure); cross-server moves stay honestly out (the boundary paragraph says so).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled fix(sync): the sidecar follows cross-board moves, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #350 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 7. Batch 5 (two parallel chunks)

### Chunk 5.1 — pomodoro focus mode (#496) (`feat/pomodoro-focus`) — gates: 4.1 merged; DR-3 escalation driver (landed)

- **Covers** (normative): #496 — a focus-session runtime (app-layer
  state-machine task beside the DR-3 escalation driver's template)
  driving the EXISTING tracking verbs (session start =
  `StartTracking`, session end = stop), plus the concentration
  overlay (countdown, current task, switcher, quick-note, exit;
  configurable timings). The overlay reads the open entry from
  published state (DR-5's open-entry model).
- **Acceptance**: the session is a real time entry (start/stop ride
  the landed verbs — no parallel tracking path); overlay state flows
  from published state (dumb-client line); timing configuration
  validated; kiosk behavior decided and pinned; journeys use
  pause/advance; CHANGELOG.

**Prompt 5.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5.1 of backlog round 2 ("pomodoro focus mode", branch feat/pomodoro-focus) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 4.1's PR (#461) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #496 (normative), and the DR-5 plan's fence note (the composition guidance: a session IS an open entry; the overlay reads it from published state) plus the DR-3 escalation driver's shape (the runtime-task template this rides beside).
2. Read the seams: the escalation driver's task shape (lifecycle, clock seam, how it avoids being a second clock source), StartTracking/StopTracking (the verbs the session drives — payloads, receipts), the published-state open entry (what the overlay reads), the UI surface conventions (views, router, keymap registration, kiosk gates), the config patterns for validated timings ([preferences]-family; typed durations with fallible mints), and the injected app clock (the pause/advance test pattern).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/pomodoro-focus ../taskboard-WT-feat-pomodoro-focus off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-pomodoro-focus-plan.md: the runtime state machine (states: idle/focus/break; transitions; what each start/stop dispatches — exactly StartTracking/StopTracking; the clock seam inherited, never a new timer source), the timing configuration (work/break durations; validation with typed errors; where in config), the overlay composition (countdown from state + clock; switcher; quick-note; exit affordances; keymap registration), the kiosk decision (likely excluded — decide and pin per the DR-5/DR-3 precedents), and the test catalogue (state-machine journeys pause/advance; overlay renders state; drift story: the tracked entry is truth, the countdown is presentation). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): pomodoro focus mode. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5.1 of backlog round 2 ("pomodoro focus mode").

1. Read AGENTS.md, issue #496, the dispatch plan §1/§2/§7 chunk 5.1, and the stage-1 plan in ../taskboard-WT-feat-pomodoro-focus (branch feat/pomodoro-focus). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the runtime state machine, the config, the overlay. Hard rules: the session drives ONLY the existing tracking verbs (a parallel time-tracking path is the bug class this fence exists to prevent — grep the diff); the runtime inherits the app's injected clock (no new wall-clock reads; tests pause/advance); the overlay renders published state only (dumb-client line — no session truth in the UI layer); timing validation with typed errors at the config boundary (A1); the kiosk decision pinned.
3. Keymap registration + goldens/journeys per the plan; CHANGELOG: one feature entry.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 5.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 5.1 of backlog round 2 (branch feat/pomodoro-focus).

1. Read AGENTS.md, issue #496, the dispatch plan §1/§2/§7 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the countdown vs. the entry (if a user pauses/stops the tracked entry outside the overlay, the session must follow state, not fight it — trace the reconciliation; the entry is truth); the state machine's exit paths (app shutdown mid-session: the open entry persists per the DR-5 model — the overlay must recover, not wedge); no second clock source (every time read traces to the injected seam); the kiosk gate structurally excludes.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(app,ui-slint): the pomodoro focus mode, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #496 with a self-contained comment.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

### Chunk 5.2 — ui-slint cleanups (#275) (`chore/ui-slint-cleanups`) — gate: 4.1 merged; re-verification mandate

- **Covers** (normative): #275 — `current_breakpoint` dead fn
  (delete or route through); `#[cfg(test)]`-gate the two `UiHandle`
  seam fns or document them permanent; the 12× adapter-seam
  boilerplate stands until a 13th seam (macro only then — no action
  now); the two production clock reads are chunk-11-verified
  DIRECT-JUSTIFIED display-only (no action). **Re-verification
  mandate** — the r3 round and the render lane may have already
  fixed or invalidated each item.
- **Acceptance**: re-verified findings fixed/annotated; zero
  behavior changes; suite green.

**Prompt 5.2.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5.2 of backlog round 2 ("ui-slint cleanups", branch chore/ui-slint-cleanups) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 4.1's PR (#461) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #275 (normative — findings F7-03/04/07/08/10 by name), and .artifacts/reports/2026-10-16-v0.1.0-audit-crate-ui-slint-report.md.
2. RE-VERIFY every finding against the CURRENT tree (the r3 round and the render lane churned adapter.rs, view_state.rs, presentation.rs, resolver.rs): record the re-verification table (fixed-already with cite / still-drifted / invalid-now with reason). Then read the current seams: wherever current_breakpoint lives now (callers? still zero?), the two UiHandle seam fns (test-only-called? cfg-gated?), the seam count (13th seam? then the macro question activates — decide), the clock reads (still the two display-only sites?).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b chore/ui-slint-cleanups ../taskboard-WT-chore-ui-slint-cleanups off main.
4. Write a detailed plan at .artifacts/plans/2026-10-31-ui-slint-cleanups-plan.md: the re-verification table; per-finding fix decisions (delete vs. route for the dead fn — decide by whether a caller SHOULD exist; cfg-gate vs. document for the seams; the macro question only if the 13th seam landed). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): ui-slint audit cleanups. Do not implement; do not push.

Report: plan path, the re-verification table, decisions.
```

**Prompt 5.2.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5.2 of backlog round 2 ("ui-slint cleanups").

1. Read AGENTS.md, issue #275, the dispatch plan §1/§2/§7 chunk 5.2, and the stage-1 plan in ../taskboard-WT-chore-ui-slint-cleanups (branch chore/ui-slint-cleanups). Recreate the worktree on that branch if missing.
2. Execute exactly per the approved plan, one logical commit per finding, every commit standalone. Hard rules: ZERO behavior changes; the cfg-gate (if chosen) must make the seams unbuildable outside tests without breaking the test builds; deletions grep-audited (no dangling references).
3. Bookkeeping: prepare ✅ texts for already-fixed findings (stage 3 posts).
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No CHANGELOG. No push, no PR. Never push to or merge main.

Report: per-gate results, per-finding outcomes, any deviation and why.
```

**Prompt 5.2.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 5.2 of backlog round 2 (branch chore/ui-slint-cleanups).

1. Read AGENTS.md, issue #275, the dispatch plan §1/§2/§7 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: a "cleanup" that changes behavior (diff every hunk — deletions of dead code and cfg-gates only); the cfg-gate pattern consistent with how the crate gates other test-only surfaces; the 13th-seam macro question NOT activated unless the re-verification found 13 seams (activating it without cause is scope creep).
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled chore(ui-slint): the audit small cleanups, checks green, mergeStateStatus CLEAN). Close #275 with the prepared annotations.
5. Teardown per §2 step 3.

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 8. Batch 6 (one chunk)

### Chunk 6.1 — effort-calibration surfaces (#214) (`feat/effort-calibration-surfaces`) — gate: 4.1 merged; normative: the ADHD plan's P14 fence

- **Covers** (normative): #214 — per-task and aggregate
  estimate-vs-actual surfaces in the review/statistics views FIRST
  (pure projection over time entries + the effort field — both
  landed by DR-5); the reference-class suggestion at capture time is
  explicitly **out of scope** (the issue's later slice, local-only
  when it comes). Stage 1 owns the surface home (the today territory
  is busy — the review/statistics view is the issue's named first
  home) and the projection semantics (completed-window, rolling
  windows).
- **Acceptance**: the projection is pure + property-tested; the
  surface renders from published state; empty-state collapses; kiosk
  gate decided; CHANGELOG.

**Prompt 6.1.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6.1 of backlog round 2 ("effort-calibration surfaces", branch feat/effort-calibration-surfaces) in /home/viktoria/src/taskboard.

GATE first: git fetch origin --prune and verify chunk 4.1's PR (#461) is merged into origin/main. If not: STOP and report.

1. Read AGENTS.md, GitHub issue #214 (normative), and the ADHD research report's P14 fence (.artifacts/reports/2026-10-14-adhd-support-research-report.md — what the evidence says the surface shows: the user's own historical estimate-vs-actual gap; the local-only constraint).
2. Read the seams: the effort field family (where estimates live on tasks — landed Wave 1), the time entries (the actuals — landed DR-5), the review/statistics view surface (its current composition — the issue's named first home; the today territory is busy and NOT this chunk's), the projection precedents (tracked_in_window's pure-projection pattern from DR-5), and the kiosk gate precedents.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = below the disk floor: STOP and report), then: git worktree add -b feat/effort-calibration-surfaces ../taskboard-WT-feat-effort-calibration-surfaces off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-31-effort-calibration-surfaces-plan.md: the projection semantics (per-task: estimated vs. summed actuals; aggregate: the estimate-vs-actual gap over a window — decide the window semantics, completed-tasks-only vs. all-touched, and the rolling-window shape; enumerate the degenerate cases: no estimate, no actuals, zero-length), the surface composition (review/statistics view rows; empty-state collapse), the kiosk gate decision, and the test catalogue (projection property tests — the laws: actuals sum over the same entries the detail view shows; degenerate cases collapse; no estimate excluded from the gap, not counted as zero — decide and pin). The capture-time suggestion slice is OUT (named in the plan so nobody re-derives it). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): effort-calibration surfaces. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 6.1.b (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6.1 of backlog round 2 ("effort-calibration surfaces").

1. Read AGENTS.md, issue #214, the dispatch plan §1/§2/§8 chunk 6.1, and the stage-1 plan in ../taskboard-WT-feat-effort-calibration-surfaces (branch feat/effort-calibration-surfaces). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone: the pure projection (property-tested per the plan's laws), then the review/statistics surface. Hard rules: the projection is pure (no clock reads, no IO — pause/advance in tests); the surface renders published state (dumb-client line); NO capture-time suggestion (the out-of-scope slice must not appear — a hint text suggesting estimates is already over the line); the kiosk gate per the plan.
3. Goldens/journeys per the plan; CHANGELOG: one feature entry.
4. Run the full gate suite from AGENTS.md §10 and make it green (fmt, clippy -D warnings, nextest, doc tests, deny, llvm-cov sequence with quarantine + rm).
5. No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 6.1.c (review):**

```text
You are the stage-3 REVIEW agent for chunk 6.1 of backlog round 2 (branch feat/effort-calibration-surfaces).

1. Read AGENTS.md, issue #214, the dispatch plan §1/§2/§8 (code-quality mandate applies), and the stage-1 plan; review the full branch diff after rebasing onto origin/main per §2 step 3.
2. Functional focus — this chunk's traps: the projection's degenerate cases (no-estimate tasks must be excluded from the aggregate gap per the plan, not counted as zero — the property tests must actually constrain this); one source of truth (the surface's actuals derive from the same entries the detail view shows — no parallel tally); the suggestion slice absent (a capture-hint anywhere in the diff is scope creep); the kiosk gate per the plan.
3. Re-run the full gate suite yourself; any failure is a finding.
4. Fix findings on the branch (amend/reword). Ship per §2 step 3 (push, PR titled feat(ui-slint): the effort-calibration surfaces, checks green, mergeStateStatus CLEAN).
5. Teardown per §2 step 3. Close #214 with a self-contained comment (the later slice stays tracked by the issue's own text — note it).

Report: verdict, findings, fixes, PR URL, green checks, issue closed, worktree removed. Never merge; never touch main.
```

## 9. Completion bookkeeping

- **Chunk → issue closures**: 1.1→#447, #448, #449 (+ #282 if fixed;
  #436 closes on the lane re-run) · 1.2→#480 (+ #133 commented,
  stays open) · 1.3→#463 · 1.4→#458 · 1.5→#450 · 1.6→#194 ·
  1.7→#407 · 1.8→#116 · 1.9→#555 · 1.10→#521, #524, #549
  (+ dispositions #287/#336/#384) · 2.1→#464 · 2.2→#270, #271 ·
  2.3→#272, #273 · 2.4→#412 · 2.5→#352 · 3.1→#462, #536 · 3.2→#401 ·
  3.3→#201 · 3.4→#490, #491 · 3.5→#493, #495 · 3.6→#276, #279, #280 ·
  4.1→#461 · 4.2→#492 · 4.3→#350 · 5.1→#496 · 5.2→#275 · 6.1→#214.
- **Already executed (2026-10-31, before this plan)**: the F-133
  Option A ruling recorded on #480; hygiene closures #166, #231,
  #210; milestones `remediation-wave-1`, `wave-kiosk`,
  `wave-daily-driver` closed as objects (`v0.1.0` left open as the
  release marker).
- **Deliberately not dispatched** (homes): the report's wait classes
  in full (external/deferred: #440, #428, #429, #444, #494, #219,
  #113–#115, #117, #121, #122; trigger-gated: #131, #133, #135,
  #137–#139, #140, #141, #142, #146, #148, #152–#155, #157, #167,
  #181, #127, #180, #191, #193, #195, #198, #220–#222; design/usage
  -gated: #213, #215–#218, #184, #185, #327, #353–#357); the user
  -skipped #351, #411, #504, #505, #506; and DR-4's recorded
  extensions (D-4c skew guard — symptom-triggered per the F-133
  ruling; D-1e/D-1f).
- **What opens next**: the weekly lane re-run after batch 1 closes
  #436 and with it the last daily-driver tier-2 loose end; the
  upstream slint report (chunk 3.1's artefact) is filed by the user;
  with batches 1–4 landed the render budget work is complete and the
  tracker's open set should be down to the wait classes only — the
  natural next frontier is the report's §5 design rounds (subtasks
  #213, review machinery #218, org-mode #184) whenever their product
  appetite returns.
