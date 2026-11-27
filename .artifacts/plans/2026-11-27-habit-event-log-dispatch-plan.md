# Habit Event Log (#1519) — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-27
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 5 chunks of the per-completion
  habit event-log refactor (one source of truth; window/day/skip
  derivations; union wire fold; sqlite migration). No code in this
  commit.
- **Origin**: the #1519 design round
  `.artifacts/plans/2026-11-27-habit-event-log-design.md` (normative:
  §0 Rulings A–I, §1 event model, §2 derivations, §3 writes/undo, §4
  wire, §5 deletions, §6 sqlite migration, §7 consumer churn list,
  §8 issue dispositions) and the UX journey plan
  `.artifacts/plans/2026-11-27-habit-event-log-ux-plan.md`
  (normative: US-1..US-6, US-n/S-k steps, W-1..W-11 wiring rows,
  R-1..R-6 rulings); plus the granularity audit §F-1..F-3
  (`.artifacts/reports/2026-11-27-source-of-truth-granularity-audit.md`)
  and the operator ruling on #1519 (2026-11-27). The operator's
  chunking decision: dispatchable as staged below (defaults adopted;
  overruled rulings re-open the affected chunks' stage 1 only).
- **Status**: READY — all five chunks dispatch after the origin plans
  are accepted (the UX plan's Status field flips to ACCEPTED). Nothing
  here is gated on a human merge; chunk order is strict (§1 graph).
  **Deliberately not dispatched**: F-4 time-entry stamps (#1521,
  follow-up design round), F-5 reflection row (#1522), files
  habit-time lane (#1363), pause-span events (#1524), legacy
  `habit-log` shim removal beyond chunk 5's gate (§5 below).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | design §1 (event model, ContextV7/#1488), §2 (derivations) | `feat/domain-habit-events` | idgen `ContextV7`; `HabitEventId`/`HabitEvent`/kinds; `derive_window_counts`/`derive_skip_posture`/`derive_day_counts`; proptests. No command changes yet (old log map stays; new module is additive) | W-7 (`us1_s3`), W-11 (`us6_s1`) test targets minted here | M |
| 2 | design §3 (writes + exact inverse), §8 fences | `feat/domain-habit-event-commands` | LogHabit/Adjust/Skip/Unskip arms onto events; receipts carry event ids; journal pre-images → event ids; `HabitSurface` D24/D25 deleted; persistence actions swapped | US-1..US-3, US-5 engine halves (US-n/S-k engine tests) | L |
| 3 | design §6 (sqlite migration), §0-I (v5 ids) | `feat/storage-habit-event-migration` | migration 0032; load-then-remap + marker; flush/load/rewind on `habit_event`; uuid `v5` feature | US-4/S2 storage half (`us4_ac2` token: `us4_s2` migration test) | M |
| 4 | design §4 (wire codec, fold, GC), §0-D legacy shim | `feat/sync-habit-events-wire` | `habit-events` section + `HabitEventRecord`; `fold_habit_events`; `union_habit_events`; builders + lane GC; legacy `habit-log` read-side shim; both connectors | US-4, US-5/S3 fold tests (`us5_s3`, `us4_s2`) | L |
| 5 | design §5 (deletions), §7 (churn 1,2,5,19–21) | `feat/ui-habit-event-consumers` | heatmap derives (`presentation.rs:8796`, `models.rs:7576`); CLI surfaces; delete `HabitLogKey`/`HabitLogEntry`/`HabitDayLogEntry`/`day_log`/`mint_log_clock`/legacy shim; backlog bookkeeping (§8 dispositions) | W-2 (`us4_s1`), US-1/S3, US-3/S1 presentation halves | M |

**Dependency graph**: strictly serial — 1 → 2 → 3 → 4 → 5. Chunk 2
mints ids from chunk 1's generator; chunk 3 persists chunk 2's
actions; chunk 4 folds chunk 2's wire shape (its legacy shim reads
chunk 3's remap results); chunk 5 deletes what 2–4 stopped reading.
No parallel dispatch (the touched file sets overlap in
`taskboard-domain`).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume/second round of a chunk works on
`<branch>-v2` (or re-branches off the fetched `origin/main`), never on
the landed branch — the doctor treats any worktree whose `HEAD` is
fully contained in `origin/main` as disposable. Create worktrees with
`git worktree add -b <branch> ../taskboard-WT-<branch> origin/main`
(explicit fetched start point, #1185). Disk hygiene (AGENTS §8):
any agent creating or recreating a worktree runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit (free disk / MemAvailable below the floor) stops the chunk;
stage 3 removes the worktree as its final handoff task.

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
   apart (cold pre-commit build peaks must not align); the
   **dispatcher's full §10 suite counts as ONE slot toward the ≤3
   cap** (agents + suite runs ≤ 3 total; `run-quality-gates.sh`'s
   flock still serializes suites; the memory gate still guards every
   agent build) — the suite is the heaviest single slot on the box, so
   stagger it ≥3 minutes from any agent start like any other slot. The
   strict **zero-agent free window is reserved for when
   `memory-gate.sh --status` shows pressure** (MemAvailable below the
   floor — the #1063 demotion). Bench/measurement worktrees live
   outside the `taskboard-WT-*` glob. Docs-only stages count toward
   the cap (their commits trigger hook builds). Any real build (>~1
   min) and the pre-commit hook's clippy step route through
   `scripts/memory-gate.sh` (cross-worktree flock + `MemAvailable`
   floor, bounded wait, `--status`); the full §10 suite keeps
   `scripts/run-quality-gates.sh`. *Two-mode routing*: builds and the
   hook's clippy step route gated-hold; self-capped long runs route
   `memory-gate.sh --preflight` only; never hold the gate lock across
   a multi-hour run. Gate deferral follows item 3. Evidence: the
   2026-11-17 global OOM
   (`.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`);
   full law: `docs/agent_dispatch.org` §2.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: creates the worktree, writes the
   TDD-first chunk plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead per
   AGENTS §8; never `git worktree add <path> main`), implements per
   the approved plan — TDD, every commit standalone (rebase-merge
   hygiene), fast gate green per the long-command discipline above
   (the dispatcher owns the full suite). The UX journey plan is
   normative where this chunk's row above names it: implement exactly
   the wiring inventory rows this chunk claims, embed journey-step
   tokens in test names, and run `scripts/ux-coverage.sh
   .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate`
   before reporting done (MISSING on a claimed row is not done).
   Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`** —
   `git fetch origin --prune`, rebase the branch onto `origin/main`
   (AGENTS §8's rebase dance: drop already-merged duplicate commits;
   resolve only real conflicts), push `--force-with-lease` if the
   rebase rewrote history — then reviews the entire branch diff versus
   the **post-rebase** base against the chunk's acceptance criteria +
   AGENTS rules **plus the code-quality mandate below**, re-runs
   whatever gates apply (fast gate per the discipline above),
   implements fixes for every finding on the branch (amend/reword,
   never duplicate-subject fixups). **Its scope ends at PR-open**:
   push the branch, open the PR to `main` with `gh`, and report the
   URL — the *dispatcher* owns `gh pr checks --watch` (any failing
   check is a finding), the `mergeStateStatus CLEAN` verification
   (`BEHIND`/`DIRTY` ⇒ rebase, push `--force-with-lease`, repeat),
   **the teardown** — from the project root, `git -C
   /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8; confirm
   via `git worktree list`) — and at-merge issue closures. The handoff
   contract is "PR open + checks green + merge state `CLEAN` +
   worktree removed". Chunks 2, 4 and 5 additionally carry the **UX
   completeness mandate**: re-run
   `scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate`;
   then execute the walkthrough protocol (docs/ux_planning.org) over
   every P1 story — the four walkthrough questions per step with
   evidence, then the six-heuristic pass over touched surfaces;
   BLOCKER/MAJOR findings are fixed on the branch, MINOR findings
   become backlog issues. Reports a verdict. Never merges; never
   touches `main` (<`main` is branch-protected — merges happen on
   GitHub by the user, after the required checks pass.>).

**Code-quality mandate** (stage 3, every chunk — implementation
chunks): beyond functional review, evaluate (a) code quality: naming,
comment discipline (comments state constraints, not narration),
idiomatic use of the approved stack, no dead code or speculative
generality, test quality per AGENTS §5; (b) the Single Responsibility
Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a
boundary violation is an SRP violation (network IO never leaves the
sync-transport crate; the derivation fold never leaves the domain
crate; the UI never computes habit semantics); (c) a single level of
abstraction per function body: a function either orchestrates named
operations or works in primitive data, never mixes. Findings are
fixed on the branch like any other finding.

## 2. Chunk 1 — domain event model + derivations (`feat/domain-habit-events`)

- **Covers**: design §1 (types per the A1 pattern, `HabitState.events`
  added additively, `new_habit_event_id`, process-global
  `uuid::ContextV7` in `idgen.rs` closing #1488 with its
  behavior-identity note), §2 (`derive_window_counts` reproducing
  `log_window` as-of-event attribution over (at, id) order;
  `derive_skip_posture` per Ruling B; `derive_day_counts`;
  mark-window re-derivation under kind change — the property test the
  design names). **Non-goals**: no command/persistence changes (the
  old `log` map and every existing test keep passing untouched); no
  sqlite, wire, or UI work; no uuid `v5` (chunk 3).
- **Acceptance**: new module `habit_event.rs` with proptests pinning
  (a) derivation ≡ old semantics on event sets that correspond 1:1 to
  legacy window rows (the equivalence oracle — the old `log_window`
  stays in-tree this chunk and is the oracle), (b) mark re-derivation
  under kind/day-start change, (c) same-second completions stay
  distinct under (at, id) order, (d) `ContextV7` mints a locally
  total order (a dedicated test, #1488). Existing suite green; no
  existing test weakened. SPDX headers; `#![forbid(unsafe_code)]`
  holds.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the habit event-log dispatch ("domain event model + derivations", branch feat/domain-habit-events) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origins: .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings A, B, I — note I is chunk-3-scoped), §1 (event model, §1.2 ContextV7 mechanism verified at uuid 1.18+), §2 (derivations); docs/coding_guidelines.org §2.1 (A1 type-driven policy); docs/testing_strategy.org §6 (clock discipline); the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§2.
2. Read the seams you extend: crates/taskboard-domain/src/idgen.rs (the IdGenerator trait + UuidV7Generator — today Uuid::now_v7(), NoContext, issue #1488), crates/taskboard-domain/src/habit.rs (log_window 913–967 — your attribution oracle; streak_with_break; HabitState 835; mint_log_clock 801 — deleted only in chunk 5), crates/taskboard-domain/src/habit_event.rs (you create it), the tests you must not break (habit.rs unit tests, persistence.rs apply tests).
3. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = resource floor breached: STOP and report instead of planning), then create the worktree: git worktree add -b feat/domain-habit-events ../taskboard-WT-feat-domain-habit-events origin/main (never without -b, never on main — AGENTS §8 active-worktree safety).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-habit-events-domain-plan.md settling: the exact HabitEventId/HabitEvent/HabitEventKind shapes (A1 newtypes, fallible minting, documented leniency); the OnceLock<ContextV7> placement in idgen.rs and the behavior-identity note for #1488; derive_window_counts' fold contract (order, oracle-vs-legacy equivalence test design); derive_skip_posture's (at, id) max law; derive_day_counts' day attribution; the mark-window re-derivation property test; the test catalogue with journey-step tokens (us1_s3, us6_s1, us1_s1_log_habit_mints_one_event is chunk 2). Resolve every open question with a recorded decision; deviations from the origin need a recorded reason.
5. Commit it as: docs(plan): the habit events domain chunk plan. Do not implement; do not push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the habit event-log dispatch ("domain event model + derivations").

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§1/§2, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§2, and the stage-1 plan (.artifacts/plans/*habit-events-domain*-plan.md) in the worktree ../taskboard-WT-feat-domain-habit-events (branch feat/domain-habit-events). Recreate the worktree on that branch if missing (git worktree add -b feat/domain-habit-events ../taskboard-WT-feat-domain-habit-events origin/main; if the branch's PR already landed, use feat/domain-habit-events-v2 per AGENTS §8).
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: SPDX headers on every new .rs file; #![forbid(unsafe_code)] (crate-wide already); no tokio::time::sleep in tests — tokio::time::pause/advance or pure fixtures; no text assertions (typed matches! only); proptests for the derivation laws; the legacy log_window stays as the equivalence oracle (it is deleted in chunk 5, not here); scratch never lands.
3. Run the fast gate per the long-command discipline: the pre-commit hooks run automatically; the full suite is scripts/run-quality-gates.sh cargo nextest run --workspace — background it if slow, poll, do useful work between polls; commit all landed work BEFORE any wait. The coverage trio (cargo fmt --check, clippy -D warnings, llvm-cov) never runs in this session — the dispatcher owns it. Every cargo invocation routes through scripts/memory-gate.sh, never bare. If the gate lock is contended and nothing else remains: commit, report "gate deferred: lock held by <holder>", and end.
4. Do NOT touch the backlog origin entries (#1519/#1488 annotations land in chunk 5); do not modify HabitState.log's readers — chunk 2's job. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate and report the honest steady state (rows this chunk's tests should flip to FOUND must be FOUND; design-stage MISSING on chunks 2–5's rows is expected and reported, never papered over).
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization); list the issue numbers in your report.

Report: per-gate results, the #1488 behavior-identity evidence, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the habit event-log dispatch (branch feat/domain-habit-events).

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§1/§2, the dispatch plan .artifacts/plans/2026-11-27-habit-event-log-dispatch-plan.md §1/§2 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-domain-habit-events.
2. Functional focus, named from the origin: (a) derive_window_counts must reproduce log_window's attribution (natural window, 24 h grace, guarded surplus-repair) AS OF each event's completion instant over (at, id) order — a subtle recursion; verify the equivalence-oracle test really pins 1:1 legacy-row correspondence, not a self-confirming reimplementations; (b) the #1488 ContextV7 adoption must not change the IdGenerator trait surface or break merge_testutil::CountingIds injection; (c) no existing test weakened; (d) A1 discipline — no derive crates, fallible minting at the boundary, documented leniency, no validate-and-forget; (e) zero network/async-runtime dependencies in taskboard-domain (AGENTS §2 rule).
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crate; the derivation fold never leaves the domain crate; the UI never computes habit semantics); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. Re-run the fast gate yourself per the long-command discipline (pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace, backgrounded with polls, work committed before any wait; flock deferral rule applies; every cargo invocation through scripts/memory-gate.sh). The coverage trio never runs in this session.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then report the URL — do NOT run gh pr checks --watch; the dispatcher owns CI watch and merge-state verification. Teardown is yours: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-domain-habit-events (everything is pushed; a resuming round starts a fresh -v2 branch per AGENTS §8). Confirm with git worktree list that the removed path was the intended one.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization); list the issue numbers in your report.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — commands, undo, journal (`feat/domain-habit-event-commands`)

- **Covers**: design §3 (LogHabit mints one Completion event — receipt
  gains the event id; AdjustHabitLog as the reconcile diff per Ruling
  C/H, newest-first tombstone preference, the current-kind window gate
  kept; Skip/Unskip mark events; exact inverses — per-event
  tombstones, adjust pre-image restore), the D24/D25 `HabitSurface`
  fence deletion (`journal.rs:131`, `undo.rs:421`, `:442`),
  persistence actions swapped (`persistence.rs:519–530, 690–704`),
  messages.rs receipt deltas (`:601–613`). **Non-goals**: no sqlite
  (chunk 3), no wire fold (chunk 4), no UI/CLI (chunk 5), no deletion
  of the legacy `log` map readers yet (chunk 5) — the legacy map and
  the event map coexist in memory this chunk, with the command arms
  writing ONLY events (the legacy map freezes; chunk 3's remap
  consumes it).
- **Acceptance**: snapshot tests (insta) of the state engine for
  log/adjust/undo/skip sequences over events; the exact-inverse tests
  (undo restores the derived pre-state on every window kind — US-3
  AC-1/AC-2, tokens `us3_s1`, `us2_s2`); absent-window adjust applies
  (US-2 AC-2, `us2_ac2`); fence-deletion evidence (the
  `NotUndoableReason::HabitSurface` variant and its classifiers are
  gone, with the remaining policy fences — Reflection/Reminder —
  untouched); convergence unit test: two independent LogHabit plans
  merged by union keep both (the chunk-4 fold's unit seed). Existing
  habit tests re-based onto events; none weakened without a recorded
  bug.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the habit event-log dispatch ("commands, undo, journal", branch feat/domain-habit-event-commands) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then: the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings B, C, H — normative), §3 (writes and the exact inverse), §7 rows 7–10 and 14; the UX journey plan .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md (US-1..US-3, US-5 engine halves; §7 command-contract delta); the dispatch plan §1/§3; docs/testing_strategy.org §6. The chunk-1 branch must have landed (verify: git fetch origin --prune && git log origin/main --oneline | head -20 shows the feat/domain-habit-events PR merge) — if not, STOP and report.
2. Read the seams you rewrite: crates/taskboard-domain/src/command.rs:3226–3436 (LogHabit/AdjustHabitLog/SkipHabit/UnskipHabit arms — the transactional-twin law and mint_log_clock bumps you retire here in the command layer), persistence.rs:519–530/:690–704 (the Record pair), journal.rs:121–131/:171–175 (HabitSurface + PreImage::HabitLogRow), undo.rs:415–510/:643–647/:752–803 (classification + pre-images), messages.rs:601–613 (receipts); the tests you must not weaken: command/undo/journal unit tests, insta snapshots.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero = STOP and report), then: git worktree add -b feat/domain-habit-event-commands ../taskboard-WT-feat-domain-habit-event-commands origin/main.
4. Write the TDD-first plan at .artifacts/plans/<today>-habit-event-commands-plan.md settling: the PersistenceAction pair's replacement shape (RecordHabitEvent/TombstoneHabitEvent vs one action — decide and record); the reconcile-diff algorithm (newest-first tombstone preference; synthesized events for upward adjusts — chunk 3's remap must never see them, record the invariant); receipt payload changes and the journal's event-id pre-image grammar; the coexistence rule (legacy log map frozen, events authoritative, the chunk-3 remap contract); the snapshot-test catalogue with journey-step tokens (us1_s1, us2_s1, us2_s2, us2_ac2, us3_s1, us5_s1, us5_s2). Recorded decisions only, no open questions left.
5. Commit as: docs(plan): the habit event commands chunk plan. No implementation; no push.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the habit event-log dispatch ("commands, undo, journal").

1. Read AGENTS.md, the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0/§3/§7, the UX journey plan, the dispatch plan §1/§3, and the stage-1 plan (.artifacts/plans/*habit-event-commands*-plan.md) in the worktree ../taskboard-WT-feat-domain-habit-event-commands (branch feat/domain-habit-event-commands). Recreate the worktree on that branch if missing (-v2 rule if landed).
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: SPDX headers; forbid(unsafe_code); no sleeps in tests (pause/advance); typed-outcome assertions only; insta snapshots updated deliberately (each snapshot change cites the behavior change in the commit body); the D24/D25 fence deletion is behavior-identity-relevant — a dedicated commit; no weakening a correct test (AGENTS §5: a failing new test may be a production bug — evaluate both, leave it failing and report if the fix is out of scope).
3. Run the fast gate per the long-command discipline: pre-commit hooks automatic; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls, work committed BEFORE any wait; every cargo invocation through scripts/memory-gate.sh; the coverage trio never runs here; flock deferral = commit + "gate deferred" + end.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate: every row this chunk claims (US-1..US-3, US-5 engine tests named in the stage-1 plan) must read FOUND; report the full honest steady state.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: per-gate results, snapshot-churn justification, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the habit event-log dispatch (branch feat/domain-habit-event-commands).

1. Read AGENTS.md, the design §0/§3/§7, the UX journey plan, the dispatch plan §1/§3 (the code-quality mandate applies), and the stage-1 plan; review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-domain-habit-event-commands.
2. Functional focus, named from the origin: (a) the inverse must be EXACT — undo of LogHabit tombstones exactly the receipt's event id; the adjust pre-image restores tombstone state exactly; no approximation on Week/Month (#1341's phantom-cell class must be pinned dead by a test); (b) the reconcile diff must never mint synthesized events that chunk 3's remap would double-count (the stage-1 plan's recorded invariant — verify it is enforced by test); (c) concurrent-increment convergence seed: two plans, union keeps both (#1094 dissolution evidence); (d) HabitSurface deleted with no surviving classifier referencing it; Reflection/Reminder fences untouched; (e) no RwLock/Mutex/orchestrator shortcuts (AGENTS §3); (f) no weakened tests.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crate; the derivation fold never leaves the domain crate; the UI never computes habit semantics); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch.
4. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest, memory-gate.sh on every cargo call, commit before waits, flock deferral). The coverage trio never runs in this session.
5. Fix every issue on the branch (amend/reword, no duplicate-subject fixups).
6. Ship: push, open the PR with gh, report the URL — no CI watch (dispatcher-owned). Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-domain-habit-event-commands; confirm via git worktree list.
7. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate; execute the walkthrough protocol (docs/ux_planning.org) over every P1 story this chunk implements (US-1..US-3, US-5 engine halves) — the four walkthrough questions per step with evidence, then the six-heuristic pass over the touched surfaces; BLOCKER/MAJOR fixed on the branch, MINOR to backlog issues.
8. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: verdict, findings, fixes, PR URL, walkthrough verdict, worktree-removed confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — sqlite migration (`feat/storage-habit-event-migration`)

- **Covers**: design §6 (migration `0032_habit_events.sql`; the
  load-then-remap pass with the one-row migration marker; flush/load/
  rewind on `habit_event`; the whole-second LWW guards die — INSERT OR
  REPLACE by uuid), §0-A (Ruling A1 attribution incl. the day-twin
  refinement), §0-I (UUIDv5 synthetic ids — **adds the uuid `v5`
  feature to the workspace pin, `Cargo.toml:93`**), §0-F (legacy
  tables dormant). **Non-goals**: no wire work (chunk 4); no UI/CLI;
  no domain command changes; the legacy `habit_log`/`habit_day_log`
  tables stay on disk.
- **Acceptance**: round-trip tests (persist → load → identical event
  map, `repo.rs` load/flush tests re-based); the remap test: a
  fixture sqlite with legacy rows migrates to the Ruling-A1 events
  (deterministic ids — run twice, byte-identical; the marker prevents
  re-run); rewind tests; sqlx prepare refreshed; migration 0032 is
  immutable-style (no edits to shipped migrations).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the habit event-log dispatch ("sqlite migration", branch feat/storage-habit-event-migration) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then: the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings A, F, I), §6 (schema, load-then-remap, marker, rewind); the dispatch plan §1/§4; ADR 0005 (shipped migrations immutable). Chunk 2 must have landed (verify origin/main log; if not, STOP and report).
2. Read the seams: crates/taskboard-storage-sqlite/src/repo.rs (load 1426–1450, in-memory apply 1816/:1860, row collection 2257–2319, flush guards 2889–2941 — note the #1095 whole-second guards you retire), rewind.rs, migrations/ (0017/0023/0028/0030 lineage), Cargo.toml:93 (uuid pin — v5 feature add), sqlx prepare workflow (.sqlx cache — note repo.rs:1357–1361's prepared-cache trap: a migration this chunk adds needs cargo sqlx prepare against it).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero = STOP and report), then: git worktree add -b feat/storage-habit-event-migration ../taskboard-WT-feat-storage-habit-event-migration origin/main.
4. Write the TDD-first plan at .artifacts/plans/<today>-habit-event-migration-plan.md settling: the exact 0032 DDL (design §6.1 as the base; the marker table shape); the remap algorithm (Ruling A1: window rows at row clock; Day-kind rows with a twin row at the twin clock; the UUIDv5 id recipe with the fixed namespace and the "legacy-habit-log|..." wire form; idempotence proof); the marker discipline (never re-runs after the user logs mid-upgrade); rewind's event grammar; the fixture-story test catalogue (us4_s2 token: us4_s2_migration_synthesizes_a1_events). Recorded decisions only.
5. Commit as: docs(plan): the habit event sqlite migration chunk plan. No implementation; no push.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: plan path, decisions, risks (the sqlx prepare trap explicitly), estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the habit event-log dispatch ("sqlite migration").

1. Read AGENTS.md, the design §0/§6, the dispatch plan §1/§4, and the stage-1 plan (.artifacts/plans/*habit-event-migration*-plan.md) in the worktree ../taskboard-WT-feat-storage-habit-event-migration (branch feat/storage-habit-event-migration). Recreate the worktree if missing (-v2 if landed).
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: never edit a shipped migration (ADR 0005); the uuid v5 feature lands as its own commit with the workspace pin single-sourced (root Cargo.toml [workspace.dependencies], AGENTS §4); scratch fixtures under /var/tmp never land; no sleeps; typed assertions; run cargo sqlx prepare through scripts/memory-gate.sh and commit the .sqlx refresh with the migration.
3. Run the fast gate per the long-command discipline (pre-commit hooks; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls; commit BEFORE waits; memory-gate.sh on every cargo call; the coverage trio never runs here; flock deferral = commit + "gate deferred" + end).
4. scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate — honest steady state reported; this chunk's claimed row (us4_s2 migration test) must read FOUND.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: per-gate results, remap idempotence evidence, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the habit event-log dispatch (branch feat/storage-habit-event-migration).

1. Read AGENTS.md, the design §0/§6, the dispatch plan §1/§4 (the code-quality mandate applies), and the stage-1 plan; review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-storage-habit-event-migration.
2. Functional focus, named from the origin: (a) the remap is deterministic and idempotent (two runs byte-identical; the marker forbids re-runs — a user logging between upgrade passes must never duplicate events); (b) Ruling A1 attribution exactly (twin-clock refinement only for Day-kind + twin present); (c) no shipped migration edited; sqlx cache refreshed against 0032; (d) the retired whole-second guards leave NO stale callers; (e) rewind round-trips; (f) network IO still absent from this crate (AGENTS §2).
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch.
4. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest; memory-gate.sh everywhere; commit before waits; flock deferral). The coverage trio never runs in this session.
5. Fix every issue on the branch (amend/reword, no duplicate-subject fixups).
6. Ship: push, open the PR, report the URL — no CI watch. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-storage-habit-event-migration; confirm via git worktree list.
7. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: verdict, findings, fixes, PR URL, worktree-removed confirmation. Never merge; never touch main.
```

## 5. Chunk 4 — wire codec + fold + both connectors (`feat/sync-habit-events-wire`)

- **Covers**: design §4 (`habit-events` section field — additive, no
  `APPDATA_FORMAT` bump; `HabitEventRecord` with the `deleted` fence;
  `fold_habit_events` union-by-uuid with the tombstone (at, by) max;
  `union_habit_events`; `habit_events_for` builder + the fail-closed
  filter; `lane_gc_old_habit_events` per Ruling E1; the legacy
  `habit-log` read-side shim per Ruling D1 — fold legacy rows into
  deterministic-id events, stop emitting the legacy section; Deck
  flush `flush.rs:28–29, 173, 439–500` and files lanes
  `board_cycle.rs:1663–1727`). **Non-goals**: no UI/CLI (chunk 5); no
  deletion of `fold_habit_log`'s legacy shim (chunk 5); the
  `habit_time_entries: None` files boundary untouched (#1363).
- **Acceptance**: fold tests: two devices' logs union keeping both
  (the S1 headline case — pinned with the #1094 same-second token
  `us5_ac2`/`us5_s3` union test), tombstone newest-wins, verbatim
  never folds, orphan facts import inertly (D-H6 posture); GC test at
  the Ruling E1 boundary; legacy-shim test (an old document's rows
  synthesize the same deterministic ids as chunk 3's remap recipe);
  both connectors' round-trip corpora.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the habit event-log dispatch ("wire codec + fold + both connectors", branch feat/sync-habit-events-wire) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then: the design .artifacts/plans/2026-11-27-habit-event-log-design.md §0 (Rulings D, E), §4 (wire codec, fold, GC, connectors); the granularity audit §F-1 items 2–3 and §3's wire-section sweep; the dispatch plan §1/§5; the UX journey plan US-4/US-5 fold halves. Chunk 3 must have landed (verify origin/main; else STOP and report).
2. Read the seams: appdata.rs (sections record :116, the tolerant-decoder grammar HabitLogRecord :1224–1340, union call :1634 and union_habit_log :1811, lane_gc_old_habit_time_entries :2545, cutoff :2634–2646, habit_log_for :2699, lane_gc_old_habit_log :2729; the envelope law :30–32 — additive never bumps APPDATA_FORMAT), merge.rs:1501–1561 (fold_habit_log — D-H6 posture and the tuple duel you retire), flush.rs:28–29/:173/:439–500, board_cycle.rs:1663–1727 (the lane structure; habit_time_entries None untouched), the corpora/snapshot fixtures both connector suites carry.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero = STOP and report), then: git worktree add -b feat/sync-habit-events-wire ../taskboard-WT-feat-sync-habit-events-wire origin/main.
4. Write the TDD-first plan at .artifacts/plans/<today>-habit-events-wire-plan.md settling: HabitEventRecord's decoder (the deleted-key fence grammar mirroring :1283); the section key grammar (event uuid string; the habit id rides the row — GC/fold habit-agnostic); the fold's tombstone (at, by) max law and D-H6 posture preservation; the shim's deterministic-id recipe REUSING chunk 3's UUIDv5 namespace verbatim (one recipe, two call sites — the single-source invariant); the emission cut-over (habit_log_for stops being called by both flush paths this chunk; its deletion is chunk 5); the GC constant rename; corpus/snapshot fixture churn list. Recorded decisions only.
5. Commit as: docs(plan): the habit events wire chunk plan. No implementation; no push.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: plan path, decisions, risks (mixed-fleet transition semantics), estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the habit event-log dispatch ("wire codec + fold + both connectors").

1. Read AGENTS.md, the design §0/§4, the dispatch plan §1/§5, the UX journey plan, and the stage-1 plan (.artifacts/plans/*habit-events-wire*-plan.md) in the worktree ../taskboard-WT-feat-sync-habit-events-wire (branch feat/sync-habit-events-wire). Recreate the worktree if missing (-v2 if landed).
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: APPDATA_FORMAT unchanged (a bump here is a bug — the section is additive); tolerant-decoder discipline (never a decode failure; Verbatim demotion); network IO only in the two connector crates (AGENTS §2); the shim reuses the chunk-3 UUIDv5 namespace (import it, never re-declare); no sleeps; typed assertions; fixture corpora updated deliberately with cited reasons.
3. Run the fast gate per the long-command discipline (pre-commit hooks; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls; commit BEFORE waits; memory-gate.sh on every cargo call; coverage trio never runs here; flock deferral = commit + "gate deferred" + end).
4. scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate — this chunk's rows (us5_s3, us4_s2 fold tests) must read FOUND; honest steady state reported.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: per-gate results, convergence-test evidence, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the habit event-log dispatch (branch feat/sync-habit-events-wire).

1. Read AGENTS.md, the design §0/§4, the dispatch plan §1/§5 (the code-quality mandate applies), the UX journey plan, and the stage-1 plan; review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-sync-habit-events-wire.
2. Functional focus, named from the origin: (a) the union fold has NO clock arbitration anywhere — grep for any surviving (clock, by) duel on the new section; identical uuids are the same fact; (b) the S1 headline pinned: a two-device same-window log test proves both increments survive; (c) the tombstone max law is (at, by) newest-wins and idempotent under re-fold; (d) GC is wire-only (a local row is never deleted — the lane_gc posture); the E1 cutoff boundary test exists; (e) the legacy shim's ids equal chunk 3's remap ids for the same facts (cross-chunk determinism test present); (f) both connectors emit the new section; the files habit_time None boundary untouched; (g) APPDATA_FORMAT untouched; (h) unknown-section relay still honest.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation (network IO never leaves the sync-transport crates; the fold never leaves the domain crate); (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch.
4. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest; memory-gate.sh everywhere; commit before waits; flock deferral). The coverage trio never runs in this session.
5. Fix every issue on the branch (amend/reword, no duplicate-subject fixups).
6. Ship: push, open the PR, report the URL — no CI watch. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-sync-habit-events-wire; confirm via git worktree list.
7. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate; walkthrough protocol over US-4 and US-5 (fold halves) per docs/ux_planning.org; BLOCKER/MAJOR fixed, MINOR to backlog.
8. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: verdict, findings, fixes, PR URL, walkthrough verdict, worktree-removed confirmation. Never merge; never touch main.
```

## 6. Chunk 5 — consumers + deletions + bookkeeping (`feat/ui-habit-event-consumers`)

- **Covers**: design §5 (delete `HabitLogKey`/`HabitLogEntry`/
  `HabitDayKey`/`HabitDayLogEntry`/`day_log`/`log` map/
  `mint_log_clock`/the legacy `habit-log` shim/`fold_habit_log`/
  `habit_log_for`/`lane_gc_old_habit_log` and every dead reader) and
  §7 rows 1–2, 5, 19–21 (heatmap derivation `presentation.rs:8796` +
  `models.rs:7576`; CLI surfaces `commands/habits.rs`), plus §8's
  bookkeeping: #1340/#1341/#1489/#1094/#1520 dissolution comments,
  #1488 closure note, #1519 closure note for the dispatcher.
  **Non-goals**: no F-7 pause work; no F-4/F-5; the dormant sqlite
  tables stay.
- **Acceptance**: heatmap presentation tests (`us4_s1` token:
  `us4_s1_day_cells_derive_from_events`) — Week/Month undo phantom
  cell gone (US-3 presentation half), migration attribution visible
  (US-4 AC-2 render half); CLI outcomes over derived state; the
  workspace compiles with **zero** references to the deleted types
  (grep-verified in review); ux-coverage gate: every P1 journey row
  FOUND; backlog dispositions commented (issues themselves closed by
  the dispatcher at merge).

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the habit event-log dispatch ("consumers + deletions + bookkeeping", branch feat/ui-habit-event-consumers) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then: the design .artifacts/plans/2026-11-27-habit-event-log-design.md §5 (deletions), §7 (churn rows 1–2, 5, 19–21), §8 (issue dispositions); the UX journey plan (W-2, US-1/S3, US-3/S1, US-4 presentation halves); the dispatch plan §1/§6. Chunk 4 must have landed (verify origin/main; else STOP and report).
2. Read the seams: crates/taskboard-ui-slint/src/presentation.rs:8796 (habit_history_day_cells — day_log feed) and models.rs:7576 (wiring); crates/taskboard-app/src/commands/habits.rs (rows 100–105, 310–332, 576–650, 998–1007, 1146–1190); the deletion surface: habit.rs:634–812 (key/entry/day types, mint_log_clock), merge.rs:1501–1561, appdata.rs legacy builders/GC/shim, persistence.rs legacy actions, and every remaining reader (grep the exact type names); the journey tests this chunk must flip FOUND.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero = STOP and report), then: git worktree add -b feat/ui-habit-event-consumers ../taskboard-WT-feat-ui-habit-event-consumers origin/main.
4. Write the TDD-first plan at .artifacts/plans/<today>-habit-event-consumers-plan.md settling: the heatmap derivation call-site contract (clock discipline — docs/testing_strategy.org §6: the render clock freezes for committed artifacts); the CLI outcome text/typed-shape deltas; the exact deletion list as a checklist (each entry: file:line at the chunk-4-landed main — re-verify, never trust this plan's 9fc0d5ee lines); the snapshot/corpus churn; the bookkeeping comment texts per issue (#1340/#1341/#1489/#1094/#1520 dissolve-with-links, #1488 closure); the final grep-verification command list. Recorded decisions only.
5. Commit as: docs(plan): the habit event consumers chunk plan. No implementation; no push.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the habit event-log dispatch ("consumers + deletions + bookkeeping").

1. Read AGENTS.md, the design §5/§7/§8, the UX journey plan, the dispatch plan §1/§6, and the stage-1 plan (.artifacts/plans/*habit-event-consumers*-plan.md) in the worktree ../taskboard-WT-feat-ui-habit-event-consumers (branch feat/ui-habit-event-consumers). Recreate the worktree if missing (-v2 if landed).
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules: the UI stays a dumb presentation client (AGENTS §1 — the derivation calls live in domain, presentation only renders); clock discipline per docs/testing_strategy.org §6 (frozen render clock for committed artifacts embedding clock-derived text); deletions are their own commit(s) with the grep evidence in the body; no sleeps; typed assertions; issue comments are permitted and expected (AGENTS §7) — do NOT close issues (dispatcher-owned at merge).
3. Run the fast gate per the long-command discipline (pre-commit hooks; scripts/run-quality-gates.sh cargo nextest run --workspace backgrounded with polls; commit BEFORE waits; memory-gate.sh on every cargo call; coverage trio never runs here; flock deferral = commit + "gate deferred" + end).
4. scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate — every P1 journey row this dispatch claims must read FOUND; report the full gate output honestly (design-stage rows that never materialized are findings, not noise).
5. No push, no PR. Never push to or merge main.
6. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: per-gate results, deletion-list completion evidence, gate output summary, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the habit event-log dispatch (branch feat/ui-habit-event-consumers).

1. Read AGENTS.md, the design §5/§7/§8, the dispatch plan §1/§6 (the code-quality mandate applies), the UX journey plan, and the stage-1 plan; review the full branch diff (git diff origin/main...HEAD) in ../taskboard-WT-feat-ui-habit-event-consumers.
2. Functional focus, named from the origin: (a) the deletion is total — grep for HabitLogKey, HabitLogEntry, HabitDayKey, HabitDayLogEntry, day_log, mint_log_clock, fold_habit_log, habit_log_for, lane_gc_old_habit_log, the legacy shim: zero production references survive; (b) the heatmap derives from events with the clock-discipline seams intact (frozen render clock in committed artifacts); (c) the #1341 phantom-cell and #1340 cross-device parity presentation tests exist and pass; (d) CLI outcomes over derived state, typed assertions only; (e) dormant sqlite tables untouched; (f) no UI-side habit semantics (AGENTS §1/§2); (g) bookkeeping comments posted with the design-doc permalink.
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch.
4. Re-run the fast gate per the long-command discipline (backgrounded flock-wrapped nextest; memory-gate.sh everywhere; commit before waits; flock deferral). The coverage trio never runs in this session.
5. Fix every issue on the branch (amend/reword, no duplicate-subject fixups).
6. Ship: push, open the PR, report the URL — no CI watch. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-habit-event-consumers; confirm via git worktree list.
7. **UX completeness mandate**: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-habit-event-log-ux-plan.md --gate; execute the walkthrough protocol (docs/ux_planning.org) over every P1 story (US-1..US-5) with evidence — the four questions per step plus the six-heuristic pass over Today habit rows and the Review history grid; BLOCKER/MAJOR fixed on the branch, MINOR to backlog issues.
8. Backlog issues per AGENTS §7 standing authorization; list numbers.

Report: verdict, findings, fixes, PR URL, walkthrough verdict, worktree-removed confirmation. Never merge; never touch main.
```

## 7. Completion bookkeeping

- **Backlog closures at merge (dispatcher-owned)**: #1519 (the
  refactor), #1488 (chunk 1), #1520 (chunks 2+4), #1340, #1341,
  #1094 (chunks 4+5), #1489 (chunk 5 — closed as dissolved with the
  local-retention caveat recorded in the closure comment; if local
  event growth ever shows, a new backlog item owns it). #1363, #1521,
  #1522, #1524 stay open with a "re-triaged unaffected/deferred"
  comment on #1363 only (its lane is untouched).
- **Deliberately not dispatched here**: F-4 (#1521) — follow-up design
  round; F-5 (#1522) — small standalone plan; F-7 (#1524) — watch;
  legacy-shim removal is chunk 5's cut-over with the retention window
  (≤ 180 d) as the safety margin.
- **What opens next**: the #1318–#1319 habit-enhancement family
  schedules on top of the event log; the F-4 design round may now
  reuse the same additive-section playbook (§4's codec grammar).
