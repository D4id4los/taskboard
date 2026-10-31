# [Plan] Backlog Round 2 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-31
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **27** chunks of the commissioned
  backlog round 2, grouped into **6 batches**. Chunks within a batch
  are parallel-safe under the lane rules of §1 and dispatch greedily
  in listed order; batches are sequential (a batch begins when every
  PR of the previous batch is merged). No code in this commit.
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
  `Arc::ptr_eq` ⇒ return before reconcile/push. A miss costs one
  wasted pass, never a stale UI. Pin: an unchanged second refresh
  does no derive/diff work (typed seam, e.g. the diff mutation
  counter — no timing assertions). Journeys/keyboard suites
  untouched. **Opens the render lane** (§1 lane 1).
- **Acceptance**: the guard at the top of the refresh pass; the
  unchanged pin; every existing journey/keyboard/golden test green
  unmodified; gate suite green; the 100 ms cadence cost at board
  scale drops to ~0 when idle (argued from the pin, not timed).

**Prompt 1.3.a (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1.3 of backlog round 2 ("unchanged-refresh guard", branch perf/ui-unchanged-refresh-guard) in /home/viktoria/src/taskboard.

1. Read AGENTS.md, issue #463 (normative), and .artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md §3.7 (the numbers this guards).
2. Read the seams: the refresh call path (UiHandle::refresh → reconcile → models::push) in crates/taskboard-ui-slint/src/adapter.rs (and view_state.rs as needed), where the ArcSwap load happens, and the diff mutation counter (or equivalent typed seam) usable as the no-work pin. Identify every refresh caller that must still force a pass (first refresh after mount — the cached-Arc initial state).
3. Doctor, then: git worktree add -b perf/ui-unchanged-refresh-guard ../taskboard-WT-perf-ui-unchanged-refresh-guard off main.
4. Write the TDD-first plan at .artifacts/plans/2026-10-31-unchanged-refresh-guard-plan.md: where the cached Arc lives, the exact guard condition, the initial-pass story, the pin's seam, and the refresh-caller audit (each caller classified: effect-driven (passes the guard naturally) vs. needs-a-forced-pass). No implementation, no push.
5. Commit: docs(plan): unchanged-refresh guard. 

Report: plan path, decisions, risks.
```

**Prompt 1.3.b (implementation):** standard — implement TDD per plan
(guard + pin; zero journey changes), full gate suite, no push.

**Prompt 1.3.c (review):** standard §2 step 3 review; traps: the
guard must compare the *loaded snapshot's* Arc (an engine store of an
identical-values state must cost one wasted pass, never staleness);
the pin must be load-bearing (break the guard, watch it fail);
mount/first-refresh handled; close #463.

### Chunk 1.4 — typed scheduled-phrase direction (#458) (`refactor/scheduled-phrase-typed-direction`)

- **Covers** (normative): #458 — resolution **option 2** (the user
  accepted the report's recommendation): `scheduled_phrase` builds on
  a typed intermediate (enum Future/Past/Now × DurationBucket, A1
  hand-rolled), the wording only in a format step; pin the *enum* per
  direction with structural assertions; text stays untested. Kills
  the `replace < with >` survivor at `presentation.rs:153:18`; the
  two `<`→`<=` equivalents at `:134:14`/`:153:18` need no action
  (dead delta==0 arm — record as understood). Non-goals: no
  ignore-list entries; no wording changes; no other presentation
  refactors.
- **Acceptance**: the survivor dies (re-run the #427 sweep scoped to
  `presentation.rs` — or at minimum the single-mutant check — and
  record zero survivors for the direction flip); enum pinned
  structurally; suite green.

**Prompt 1.4.a/b/c**: standard three; reads = issue #458 + the
mutation report (`.artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md`)
+ `presentation.rs`; stage 2 refactors TDD (enum first, structural
pins, then the format step), re-runs the scoped mutant check as
evidence; stage 3 traps: the enum must be hand-rolled A1 (no derive
crates), the pins structural (`matches!`-class, no text), the bucket
boundaries unchanged (golden/snapshot outputs byte-identical — the
user-visible wording must not move), close #458.

### Chunk 1.5 — worktree-doctor never-pushed blind spot (#450) (`fix/worktree-doctor-never-pushed`)

- **Covers** (normative): #450 — the doctor additionally spares a
  worktree whose branch has **no remote ref** (never pushed), since
  a merged leftover always has `origin/<branch>` or is contained in
  `origin/main`. Must check the false-positive direction (a stale
  never-pushed branch genuinely abandoned) and resolve it with a
  recorded stance. Files: `scripts/worktree-doctor.sh` only.
- **Acceptance**: a fresh `-b` worktree at `origin/main`'s tip with
  zero commits is KEEP; a pushed-and-merged leftover is still
  REMOVE; the floor check always runs; the five existing protection
  cases from the chunk-2 (2026-10-25) plan still hold; exit
  semantics unchanged (nonzero = below floor only).

**Prompt 1.5.a/b/c**: standard three; reads = issue #450 +
`scripts/worktree-doctor.sh` + the 2026-10-25 sweep plan's
verification checklist; stage 2 implements the remote-ref check with
the recorded false-positive stance (recommend: KEEP on never-pushed,
documenting that the user prunes deliberately via the sweep's
existing reports if it ever becomes noise); verification cases run
for real with transcripts (fresh worktree; pushed+merged leftover;
dirty; unmerged; below-floor); stage 3 traps: exit-contract drift is
THE regression; `git worktree list --porcelain` enumeration (landed
in the sweep fix) must stay the source of truth; close #450.

### Chunk 1.6 — IPC disable knob (#194) (`feat/ipc-disable-knob`)

- **Covers** (normative): #194 — a `#[serde(default =
  "default_true")]` `[ipc] enabled` config key plus a `daemon
  --no-ipc` override; when disabled, skip `spawn_server`; the rest of
  the lifecycle unchanged; documented degradation: `status`/`daemon
  stop` surface typed `no-daemon`. A1 discipline: the knob is
  config-validated like its siblings (bools need no mint — keep the
  validation story consistent with `AppConfig`'s patterns).
- **Acceptance**: enabled-by-default semantics unchanged (all
  existing IPC tests green untouched); disabled boot serves nothing
  on the socket and `status` reports the typed no-daemon state;
  `--no-ipc` overrides the file; config docs + CHANGELOG updated.

**Prompt 1.6.a/b/c**: standard three; reads = issue #194 +
`crates/taskboard-app/src/config.rs` (settings surface + validate) +
`main.rs`/`ipc/` spawn path + the existing `no-daemon` typed error;
stage 2 TDD (disabled-boot journey, override precedence, no-daemon
degradation pin); stage 3 traps: disabled must not break the lock
file/instance semantics (only the socket is skipped); the override
beats the file beats the default; docs mention the reduced
`status`/`stop` surface; close #194.

### Chunk 1.7 — clap at-least-one-flag groups (#407) (`fix/tasks-edit-flag-group`)

- **Covers** (normative): #407 — `TasksCmd::Edit` and the labels
  `Edit` arm gain `ArgGroup(required = true, multiple = true)` so a
  zero-flag edit is a clap usage error (exit ≠ 0) instead of falling
  through to a typed `no_op`. The docstrings were already corrected
  to describe actual behavior — they now get to be true.
- **Acceptance**: zero-flag `tasks edit`/`labels edit` rejected by
  clap with a usage error; at-least-one-flag edits unchanged; the
  docstrings now accurate; e2e/CLI journeys green.

**Prompt 1.7.a/b/c**: standard three, compressed; reads = issue #407
+ `crates/taskboard-app/src/cli.rs` (both Edit arms) + the labels
edit command arm; stage 2 TDD at the clap parse boundary (structural:
parse result is an error kind, no text assertions on clap's
message); stage 3 traps: `multiple = true` must still allow the
full flag set; the labels arm is easy to miss — both surfaces pinned;
close #407.

### Chunk 1.8 — Nextcloud version matrix (#116) (`ci/nextcloud-version-matrix`)

- **Covers** (normative): #116 — a matrix leg for the oldest
  supported Nextcloud/Deck major on the docker tier (separate compose
  override or overridable image tag) added to the weekly
  `Nextcloud Integration` workflow. The 1.17.5 gate-tier tooling
  already exists (`DECK_VERSION=v1.17.5` from the C-series) — stage 1
  decides matrix legs (recommend: current stable + the oldest
  supported pair) and the compose mechanism.
- **Acceptance**: the workflow runs both legs (evidence: manual
  `gh workflow run` is the user's — validate instead by local
  compose runs of both legs + `act`-style lint or workflow YAML
  checks); a leg failure is visible per-leg; no secrets added; the
  #448-class image drift becomes structurally visible.

**Prompt 1.8.a/b/c**: standard three; reads = issue #116 +
`.github/workflows/nextcloud-integration.yml` + `docker/nextcloud-test/`
compose + how `DECK_VERSION` is consumed; stage 2 implements the
matrix (workflow edit + compose override), validates both legs
locally with the same nextest invocation the workflow uses; stage 3
traps: matrix legs must not share container names/volumes (parallel
legs); the pinned-image mechanism must be a leg parameter, not a
sed; YAML lint clean; close #116.

### Chunk 1.9 — undo tracking/habit arms (#555) (`feat/undo-tracking-habit-arms`)

- **Covers** (normative): #555 + the classification table in
  `.artifacts/plans/2026-10-30-undo-journal-plan.md` §3 (D20) — the
  mechanically invertible arms join the inverse-synthesis catalogue:
  `LogTime`→`DeleteTimeEntry`, `AdjustTimeEntry`→same-verb pre
  start/end, `CreateHabit`→`DeleteHabit`, `UpdateHabit`→pre
  `HabitChanges`, `MoveHabit`→pre order, `AdjustHabitLog`→pre count.
  **Stay out** (structurally non-invertible/lossy, even in v2):
  `StartTracking`/`StopTracking` (ADR 0017), `LogHabit`. v1's
  `NotUndoable(TrackingSurface)`/`NotUndoable(HabitSurface)`
  classifications shrink to exactly the non-invertible set.
- **Acceptance**: each newly-invertible arm round-trips (undo of the
  verb restores the pre-state, pinned per arm; redo re-applies);
  the excluded arms still classify `NotUndoable` with the recorded
  reasons; the existing undo e2e suite green; ADR 0019 untouched (a
  one-line amendment note only if the classification table it
  references moved).

**Prompt 1.9.a/b/c**: standard three; reads = issue #555 + the undo
journal plan §3/D20 + the inverse-synthesis catalogue +
`plan_undo`/`plan_redo` (domain) + the habit/tracking command arms
(`messages.rs`/`command.rs`) + the undo e2e suite; stage 2 TDD per
arm (undo-journey tests per newly-invertible verb); stage 3 traps:
pre-image fidelity for `UpdateHabit`/`AdjustHabitLog` (the change
mask must express the *pre* values, matching #555's wording); the
excluded set must not silently shrink; the journal's single-advance
rule untouched; close #555.

### Chunk 1.10 — pty flake pass (#521 + #524 + #549; dispositions #287/#336/#384) (`test/pty-flake-pass`)

- **Covers** (normative): the three sibling cli_tty flakes — one
  mechanism (60 s outer pty timeout under full-suite load), three
  observations in three days; each issue's reading points at the
  outer `timeout 60` wrapper as the only bound on a wedged pty. The
  pass: root-cause the load sensitivity (marker-gated blocking reads
  vs. the outer bound), fix deterministically (injected/derived
  budget or a harness seam — per AGENTS §5, never sleeps, never a
  bigger constant), and pin the fix under a stress profile. Then the
  dispositions: #287, #336, #384 (open since the last pass's capture
  policies — re-triage per the watchlist protocol: fix where
  root-caused, else refresh/annotate and keep open or close per the
  evidence).
- **Acceptance**: all three pty tests pass under the recorded stress
  profile (and in isolation); the fix is a seam/budget change, not a
  timeout bump (a bumped constant is a finding); each of the six
  issues carries a standalone disposition comment; no test weakened.

**Prompt 1.10.a/b/c**: standard three; reads = the three issues +
#287/#336/#384 + `crates/taskboard-app/tests/cli_tty.rs` harness +
AGENTS §5/§7; stage 2 reproduces under load (nextest full-suite or
recorded load shape), implements the seam fix, stress-validates;
stage 3 traps: the classic flaky "fixes" (bigger constant,
poll-without-deadline, weakened assertion) are findings; dispositions
stand alone; close #521/#524/#549; disposition comments on
#287/#336/#384 (close only where the evidence supports archive).

## 3. Batch 2 (five parallel chunks; begins when batch 1 fully merges)

### Chunk 2.1 — row-diff mirror (#464) (`perf/ui-row-diff-mirror`) — gate: 1.3 merged

- **Covers** (normative): #464 — `PushModels` keeps a Rust-side
  mirror `Vec<T>` beside each persistent handle; diff compares
  desired vs. mirror (no `row_data` clone round-trip), fires
  `set_row_data` only on difference, updates model+mirror per
  mutation; same lifetime/pruning as `column_tasks`' `retain`; the
  repetition probe and diff unit tests stay green unmodified or are
  strengthened.
- **Acceptance**: no `row_data` reads in the diff path; stable-model
  identity invariants intact (repetition probe, F8/F11 journeys);
  gate suite green.
- Prompts: standard three; reads = issue #464 + `models.rs`
  (`push_rows`/`diff_rows`/`PushModels`) + the repetition probe +
  #463's landed guard (must compose — the mirror sits after the
  guard); stage 3 traps: mirror/model divergence is THE hazard (any
  path mutating the model without the mirror corrupts the next diff
  — audit every `set_row_data`/push site); the pruning rule must
  mirror `retain`; close #464.

### Chunk 2.2 — audit cleanups: domain + state (#270 + #271) (`chore/audit-cleanups-domain-state`) — gate: 1.9 merged

- **Covers** (normative): #270 (push.rs module-map entry; R2/R7/R8
  rule consts + per-rule verdict events or documented umbrella
  mapping at merge.rs:58; `is_noop` consumed at push_exec.rs:128 or
  demoted test-only + false-case assert) and #271 (FlakyRepository
  fake → `tests/common/mod.rs` keeping `rearm`; one
  `sync_status_action` helper; named structs mirroring `EngineParts`
  for Seams/TickSeams; F4-09 no action).
  **Re-verification mandate**: findings date to 2026-10-17 — the
  DR-4/DR-5/undo waves churned exactly these files; re-check every
  finding against the current tree first (fixed-already ⇒ ✅ comment
  on the issue instead of a diff hunk).
- **Acceptance**: every finding addressed (fixed or annotated with a
  reason); zero behavior changes; suite green.
- Prompts: standard three; stage 3 traps: rule consts/events must
  not change logged verdict *semantics* (only add precision); the
  fake move keeps `rearm` semantics and both suites green; close
  #270/#271 (annotating already-fixed findings).

### Chunk 2.3 — audit cleanups: storage + sync (#272 + #273) (`chore/audit-cleanups-storage-sync`) — gate: 1.1 merged

- **Covers** (normative): #272 (duplicated rationale pair; stale
  Phase-5 doc; drifted "eight SELECTs" comment; arity-blind
  `nums[0..2]` decode mappers → `Corrupted` via slice patterns;
  F5-10 domain text-form unification explicitly deferred — M, not
  here; F5-08/F5-11 no action) and #273 (crate description "WebDAV/
  REST" reword + AGENTS §2 row; architecture.org crate row sentence;
  `DeckError::Envelope` → `Json` rename or re-doc both directions;
  `GroupOutcome`/`PushOutcome` doc cross-links). **Re-verification
  mandate** as chunk 2.2.
- **Acceptance**: findings addressed/annotated; decode mappers
  arity-safe (a malformed row yields `Corrupted`, not a panic —
  pinned); renames complete (grep-audited); suite green.
- Prompts: standard three; stage 3 traps: the Envelope→Json rename
  must not collide with this week's sync churn (re-verify the
  finding first); AGENTS §2's table row edit rides the docs change;
  close #272/#273.

### Chunk 2.4 — window-geometry persistence (#412) (`feat/window-geometry-persistence`) — gates: 1.3, 1.6 merged

- **Covers** (normative): #412 — persist the main window's
  size/position across restarts (the cross-restart counterpart of
  r1 D15's within-session stability). Stage 1 decides the storage
  home (device-local config section vs. a small state file —
  geometry is per-device, NOT synced; the config-section home has
  the `[preferences]`-family precedent) and the invalid-geometry
  story (off-screen saves after monitor changes).
- **Acceptance**: geometry survives restart (journey-pinned at the
  seam, no real windowing in tests — the mount handover is typed);
  degenerate/off-screen values clamp sanely (pinned); kiosk mode
  unaffected (fullscreen owns geometry — pinned or explicitly
  out-of-path); CHANGELOG entry.
- Prompts: standard three; reads = issue #412 + r1 plan D15 +
  adapter mount + config write precedents (`AppConfig::write_path`,
  logout flow); stage 3 traps: never sync geometry through the
  sidecar (device-local by design); the save path must not fight
  the config-validation A1 patterns; close #412.

### Chunk 2.5 — timers & alarms surface (#352) (`feat/timers-alarm-surface`) — gate: 1.3 merged; normative: the DR-3 plan

- **Covers** (normative): #352 + the DR-3 plan's timers rulings —
  the thin-client timers composition: active alarms with remaining
  time (the overdue-banner clock precedent), fired-not-acknowledged
  alarms with snooze/cancel affordances dispatching the existing
  `SnoozeReminder`/`CancelAdhocAlarm` commands through the action
  layer, and a creation affordance using the `--in`/`--at` grammar.
  The escalation/alarm-window half already landed (DR-3/4) — stage 1
  confirms remaining scope against the DR-3 plan so nothing double-
  lands. Territory: today/chrome (§1 lane 2); expect `models.rs`
  rebase friction with the render lane (§1 lane 1 note).
- **Acceptance**: the timers list renders live state only
  (`AppState.reminders.alarms`); fired-ack affordances dispatch
  existing commands (no new command surface); creation matches the
  CLI's duration grammar; kiosk banner untouched (task-only by
  contract); goldens/journeys updated; CHANGELOG entry.
- Prompts: standard three; stage 3 traps: no nagging-policy logic in
  the UI (render state, dispatch intents — the dumb-client line);
  time-dependent tests pause/advance, never sleep; the kiosk gate
  (per DR-5/4's precedent) if the surface must hide on kiosk —
  decide with the DR-3 plan's rulings; close #352.

## 4. Batch 3 (six parallel chunks; begins when batch 2 fully merges)

### Chunk 3.1 — kiosk board i16 overflow (#462, rider #536) (`fix/kiosk-board-i16-overflow`) — gate: 2.1 merged (render lane)

- **Covers** (normative): #462 — the board panics (software
  renderer's i16 geometry, `overflow-checks = true`) when a column's
  live content crosses 32 767 px (~2.5k tasks). Fix levers per the
  issue, in order: (1) the #373 ListView mechanism (structural —
  viewport-only instantiation), (2) `clip: true` (visual-design
  decision — goldens for over-deep columns), (3) upstream slint.
  **Stage 1 owns the lever decision** (the report and issue
  recommend the structural route) and the golden question. **Rider
  #536**: the upstream slint report covering both the i16 raster
  facts (this issue's mechanism notes + why Today's ~183k px list
  survives where the board's column scene does not) and the
  testing-backend empty-element-query limitation — one upstream
  artefact, filed as a drafted report in-repo
  (`.artifacts/reports/2026-10-31-slint-upstream-i16-and-testing-backend-report.md`);
  actually posting upstream stays the user's action.
- **Acceptance**: a ≥2.5k-task board renders on the software renderer
  (bench or golden evidence at the formerly-panicking scale — the
  bench's 400-rows/column safe bound updated to the new reality);
  no panic at N = 10 000 draw lanes; existing board journeys/goldens
  updated honestly; the upstream report artefact lands (diff =
  code + that report only).
- Prompts: standard three; stage 3 traps: virtualisation must not
  break the keyboard reading-order walk/focus model (the #373
  round's pinned semantics) or `IdTables` index mapping; the bench's
  loud-skip rule stays loud; close #462 and #536 (the latter as
  "documented + upstream draft filed" — its in-repo actionability is
  exhausted).

### Chunk 3.2 — Unreadable 4xx/5xx split (#401) (`feat/appdata-unreadable-split`) — gate: 2.3 merged

- **Covers** (normative): #401 — additive `AppDataUnreadable`
  variants (or a payload carrying the typed `SyncErrorKind`) plus
  the `sidecar_evidence` mapping in the sync actor; the document
  cache stays untouched on every failure form (the issue's own
  invariant), so widening the enum cannot change ingest semantics.
- **Acceptance**: a 4xx download failure and a 5xx map to distinct
  typed evidence (pinned at the mapping boundary); wire/snapshot
  additions additive; suite green.
- Prompts: standard three (compressed); close #401.

### Chunk 3.3 — delivery-proof pruning (#201) (`feat/delivery-proof-pruning`) — gate: 2.2 merged

- **Covers** (normative): #201 — a pruning rule in the delivery
  interpreter path (drop deliveries whose occurrence is older than N
  days at record time), defined in the domain's shared
  `apply_actions` so fakes and sqlite stay in lockstep (F16-20:
  mechanical, the `DeleteReminderRule` cascade precedent exists).
  Stage 1 picks N with a recorded rationale (the issue says pick N
  when a real deployment justifies churn — propose a conservative
  default, e.g. occurrence-older-than-90d, and record it).
- **Acceptance**: pruning is deterministic (pause/advance tests, no
  wall reads); fakes and sqlite execute the same domain rule
  (the shared-`apply_actions` pin); set-union merge unaffected for
  live occurrences; suite green.
- Prompts: standard three; stage 3 traps: pruning must never drop a
  row whose at-most-once suppression is still reachable (the
  occurrence window vs. re-arm horizon — argue it in the plan); no
  migration (rows already exist); close #201.

### Chunk 3.4 — tracked-time surfaces (#490 + #491) (`feat/tracked-time-surfaces`) — gates: 2.1 (models), 2.5 (today territory)

- **Covers** (normative): #490 — today "Tracked" section: per-task
  tracked-today totals + day total via the DR-5 `tracked_in_window`
  pure projection (day_start_time window split); empty section
  collapses; follows the section pattern. #491 — task-detail
  entry-history list (start/end/duration rows) with adjust/delete
  affordances dispatching `AdjustTimeEntry`/`DeleteTimeEntry`
  (until now CLI-only). Normative: the DR-5 plan's D-T rulings.
- **Acceptance**: totals derived purely (projection unit + property
  tests; window-boundary splits pinned); affordances dispatch
  existing commands only; kiosk gate per the DR-5 UI precedent;
  goldens/journeys; CHANGELOG.
- Prompts: standard three; stage 3 traps: two territories in one
  chunk (today + detail) — keep the projection pure and shared, the
  two surfaces thin; no nagging/auto-start logic; close #490 and
  #491.

### Chunk 3.5 — respite + break reminders (#493 + #495) (`feat/respite-break-reminders`) — gate: 2.2 merged (domain calm)

- **Covers** (normative): #493 — a respite flag/kind on time entries
  (break-activity counting), additive default-off fold on the wire
  (the DR-5 shape reserved the vocabulary). #495 — break reminders
  driven by accumulated tracked-work duration: a new pure trigger fn
  over time entries riding the landed reminders pipeline
  (rules/evaluator/sink), respite-marked entries feeding the reset
  condition. Serial inside the chunk (#493 first — #495 is its
  first consumer). Stage 1 owns the break-reminder rule surface
  (how a user expresses "remind me after 90m tracked" — the
  existing rule grammar vs. a new trigger-source discriminator) per
  the DR-5 plan's fences.
- **Acceptance**: respite folds additively (old data reads as
  non-respite — pinned); the trigger fn is pure and proptest'ed
  (duration accumulation incl. respite resets); evaluator/sink
  untouched beyond the trigger arm; CLI surface for marking respite;
  CHANGELOG.
- Prompts: standard three; stage 3 traps: the trigger must not
  double-fire (the delivery set-union suppression is the guard —
  argue occurrence semantics in the plan); no schema migration
  expected (wire/codec additive); close #493 and #495.

### Chunk 3.6 — app hardening + contracts (#276 + #279 + #280) (`chore/app-hardening-contracts`) — gates: 1.6, 1.7, 1.10 merged

- **Covers** (normative): #279's five one-line-shaped bounds/latency
  fixes (flush inside the shutdown timeout; `join_bounded` the
  storage join; `spawn_blocking`/ADR-note the keyring/config load in
  `attach_from_persisted`; drain-or-document the UI forwarder's ≤64
  queue; match the SYSLOG `Lagged` family) + #280 (one publish-order
  test — subscriber observes the swapped snapshot; collapse the
  `route_for_ipc` dual enumeration) + #276 (reword the daemon-gate
  bail; one `authority_of` helper in the sync crate consumed by
  config validation + `credential_key`; fold the handler-side
  `io.err` write into the warning shapes). **Re-verification
  mandate**: 2026-10-17 findings; the signal-ladder, IPC-knob and
  daemon rounds churned these files — re-check each first.
- **Acceptance**: each re-verified finding fixed or annotated; the
  publish-order test is load-bearing; the dual enumeration is
  collapsed (one match drives both); shutdown bounds pinned with
  pause/advance where clock-adjacent; suite green.
- Prompts: standard three; stage 3 traps: hardening must not change
  observable exit codes (the daemon ladder's contract); the
  `authority_of` helper lives in the sync crate (SRP — #276's own
  ruling) and both consumers adopt it; close #276/#279/#280.

## 5. Batch 4 (three parallel chunks)

### Chunk 4.1 — Today virtualisation (#461) (`perf/ui-today-virtualisation`) — gate: 3.1 merged (render lane)

- **Covers** (normative): #461 — the Today view's section lists via
  the (landed) #373 ListView mechanism; the extra acceptance the
  board didn't have: **anchor-on-removal** — Overdue membership
  changes on every done flip; scroll-anchor semantics under
  above-viewport insertions/removals must be pinned by tests, not
  discovered by users. Normative: the ListView design-round report
  (§2 routes, §3c scroll semantics, §4 what moves, §6 step 3).
- **Acceptance**: today draw cost decouples from list extent (bench
  lanes per the report's view-pinned contract); the anchor-on-removal
  pins; all today journeys/goldens green.
- Prompts: standard three; stage 3 traps: focus/walk semantics under
  virtualisation (the #373 board round's precedents apply); removal
  reflow must keep the user's place (the pins are the point); close
  #461.

### Chunk 4.2 — tracker task kind (#492) (`feat/tracker-task-kind`) — gate: 3.5 merged

- **Covers** (normative): #492 — a never-"done" tracker kind
  suppressing done-affordances and done-triggered cascades while
  tracking stays fully live; additive kind/flag folding onto the
  entity and its wire row as a default-carrying addition (the DR-5
  reservation). Stage 1 decides the minimal domain shape (kind
  flag vs. the reserved vocabulary) and the done-suppression
  semantics (typed rejection vs. hidden affordance at each surface —
  CLI, UI, kiosk).
- **Acceptance**: done on a tracker task is impossible via the
  public API (typed outcome pinned); tracking verbs unaffected;
  cascade non-firing pinned; wire additive (old readers safe);
  CHANGELOG.
- Prompts: standard three; close #492.

### Chunk 4.3 — sidecar cross-board repair (#350) (`fix/sidecar-cross-board-repair`) — gate: 3.2 merged (sync crate)

- **Covers** (normative): #350 — when a board's fold adopts an
  unbound remote card, look up the card id in sibling-board
  documents the same connector owns, migrate the entry into the
  destination document (embedded clocks unchanged), let the origin's
  snapshot-absence GC drop the old entry. Partial by construction
  (single-connector visibility) — the issue's own honesty stays in
  the docs. Stage 1 owns the sibling-document lookup mechanics
  (cacheability, failure isolation: a missing sibling must degrade
  to today's behavior, never block adoption).
- **Acceptance**: the cross-board move preserves sidecar fields when
  both boards share a connector (integration-pinned at the fold
  boundary); the partial-coverage boundary documented
  (`docs/` one-paragraph or the sidecar plan's amendment note);
  failure isolation pinned; suite green.
- Prompts: standard three; stage 3 traps: the migration must carry
  embedded clocks unchanged (a reset clock re-opens LWW races);
  GC/absence semantics unchanged; close #350.

## 6. Batch 5 (two parallel chunks)

### Chunk 5.1 — pomodoro focus mode (#496) (`feat/pomodoro-focus`) — gates: 4.1 (today territory), DR-3 escalation driver (landed)

- **Covers** (normative): #496 — a focus-session runtime (app-layer
  state-machine task beside the DR-3 escalation driver's template)
  driving the existing tracking verbs (session start =
  `StartTracking`, end = stop), plus the concentration overlay
  (countdown, current task, switcher, quick-note, exit;
  configurable timings). The overlay reads the open entry from
  published state (DR-5's open-entry model).
- **Acceptance**: the session is a real time entry (start/stop ride
  the landed verbs — no parallel tracking path); overlay state flows
  from published state (dumb-client line); timing configuration
  validated; kiosk behavior decided and pinned (overlay on kiosk:
  likely no — decide with the DR-5/DR-3 precedents); journeys use
  pause/advance; CHANGELOG.
- Prompts: standard three; stage 3 traps: the runtime task must not
  become a second clock source (inject/inherit the app clock seam);
  timer drift vs. the tracked entry's authority (the entry is
  truth; the countdown is presentation); close #496.

### Chunk 5.2 — ui-slint cleanups (#275) (`chore/ui-slint-cleanups`) — gate: 4.1 merged; re-verification mandate

- **Covers** (normative): #275 — `current_breakpoint` dead fn
  (delete or route); `#[cfg(test)]`-gate the two `UiHandle` seam fns
  or document permanent; macro-only-on-13th-seam stands (no action);
  clock reads no action. **Re-verification mandate** — the r3 round
  may have already fixed or invalidated each item.
- **Acceptance**: re-verified findings fixed/annotated; zero
  behavior changes; suite green.
- Prompts: standard three (compressed); close #275.

## 7. Batch 6 (one chunk)

### Chunk 6.1 — effort-calibration surfaces (#214) (`feat/effort-calibration-surfaces`) — gate: 4.1 merged; normative: the ADHD plan's P14 fence

- **Covers** (normative): #214 — per-task and aggregate
  estimate-vs-actual surfaces in the review/statistics views first
  (pure projection over time entries + the effort field — both
  landed); reference-class suggestion at capture time explicitly
  **out of scope** (the issue's later slice, local-only when it
  comes). Stage 1 owns the surface home (the today territory is
  busy — the review/statistics view is the issue's named first home)
  and the projection semantics (completed-window, rolling windows).
- **Acceptance**: the projection is pure + property-tested; the
  surface renders from published state; empty-state collapses;
  kiosk gate decided; CHANGELOG.
- Prompts: standard three; close #214.

## 8. Completion bookkeeping

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
  natural next frontier is the §5-of-the-report design rounds
  (subtasks #213, review machinery #218, org-mode #184) whenever
  their product appetite returns.
