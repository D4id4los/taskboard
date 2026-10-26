# [Plan] Design-Rounds Batch (DR-2…DR-6 + Undo v1) — Implementation Dispatch Plan & Agent Prompts

- **Date**: 2026-10-26
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **24 implementation chunks** of the
  six accepted design rounds completed on 2026-10-26: signal handling
  (DR-2), the reminders surface pair (DR-3), the sync merge-policy
  family (DR-4), time tracking (DR-5), recurring tasks & habits
  (DR-6), and undo v1. No code in this commit.
- **Origin** (each normative; sections per chunk below):
  - `.artifacts/plans/2026-10-26-signal-handling-design-plan.md`
    (DR-2, ACCEPTED — chunks #469/#470/#471)
  - `.artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md`
    (DR-3, ACCEPTED — chunks #474–#477)
  - `.artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md`
    (DR-4, ACCEPTED — chunks #480–#484, **strictly serial by ruling**)
  - `.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
    (DR-5, ACCEPTED — chunks #486–#489)
  - `.artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md`
    (DR-6, ACCEPTED — chunks #498–#503)
  - `.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md`
    (undo v1, READY FOR REVIEW — chunks #509/#510; D-U1…D-U5 of
    `.artifacts/plans/2026-10-19-undo-architecture-design-plan.md` are
    binding) — **plus the user's 2026-10-26 decision to dispatch all
    six rounds as one batch**.
- **Status**: **READY** — all 24 chunks are dispatchable. The undo
  implementation round's §7 refinement rulings (R-U6…R-U12) were
  accepted by the user on 2026-10-26 (recorded in the undo plan's
  Status line), lifting the gate on #509/#510; undo/1 keeps its
  command-surface and storage-lane scheduling gates (§1, rule 6).
  **Deliberately not dispatched here**: DR-5's fence issues
  #490–#496; DR-6's follow-ups #504–#506 (tracker reminders, Kani
  harnesses, habit UX); the undo v2 family #353–#357; DR-4's recorded
  extensions (baseline scope D-1e, symptom-triggered skew guard
  D-4c, conflict surfacing D-1f); DR-3's OS-delivery plumbing queue
  (GUI review round 3 owns it; #444 stays fenced).

## 1. Chunks & ordering

Chunk IDs combine the round and the round's own chunk letter/number.
**(w)** marks a *shape-widener*: the chunk changes the serialized
`AppState` shape (a new field/section on `Task`, rules, state), so its
PR must regenerate the shared insta dump+tail suite — only one such
chunk may be in flight at a time. "Mig" is the canonical migration
number assigned by this plan (§2).

| Wave | Chunk | Issue | Branch | Covers (one line) | Size |
|---|---|---|---|---|---|
| 1 | DR-5/1 | #486 | `feat/tracking-domain-engine` | `TimeEntry`/`TrackingState`, 5 verbs, planner semantics, ADR 0017 (w) | M |
| 1 | DR-4/S | #480 | `test/sync-skew-pins` | skew interleaving pins, Clock-seam demo, #133 bookkeeping | S |
| 1 | DR-3/1 | #474 | `fix/reminder-settlement-retime` | prune/compaction keyed to dismissal, both-reader properties | S/M |
| 1 | DR-2/R | #469 | `feat/repl-signal-cancel` | `signals.rs`, REPL cancel/force, rustyline prompts, tty restore | M |
| 2 | DR-6/1 | #498 | `feat/series-domain-core` | `RecurRule` grammar + arithmetic, `Task.recur` field family (w) | M |
| 2 | DR-2/I | #470 | `feat/ipc-client-cancel` | client cancel + exit 130, read deadlines, `Shutdown` control lane | S/M |
| 3 | DR-4/G | #481 | `feat/sync-remote-baseline` | remote baseline + changed-field merge, mig **0010** (w) | M/L |
| 3 | DR-3/3 | #476 | `feat/timers-alarm-surface` | today-view timers section, status chip, creation form | M |
| 4 | DR-4/L | #482 | `feat/sync-orset-labels` | OR-set label memberships, mig **0011** (w) | L |
| 4 | DR-2/D | #471 | `feat/daemon-signal-ladder` | 3-rung daemon ladder, exit-code table, GUI/kiosk adoption | S/M |
| 5 | DR-6/2 | #499 | `feat/series-engine` | completion spawn + `day_tick` roll, ADR 0018 | M |
| 5 | DR-4/D | #483 | `feat/sync-resurrect-contention` | resurrect-as-new-card + `TaskResurrected` | M |
| 5 | DR-5/4 | #489 | `feat/tracking-ui` | status chip, context-menu arm, Start Task button, kiosk gate | M |
| 6 | DR-3/2 | #475 | `feat/reminder-insistence` | `Insistence` fields, mig **0012**, sidecar additive, CLI flags (w) | M |
| 6 | DR-4/R | #484 | `feat/sync-duplicate-reconcile` | crash-window duplicate reconcile, boards-W2 sweep | M |
| 7 | DR-6/5 | #502 | `feat/habit-domain-engine` | `HabitState`/log, commands, streaks, `continued_from` (w) | M |
| 7 | DR-5/2 | #487 | `feat/tracking-storage-roaming` | mig **0013**, sidecar `time-entries` section, retention aging | M/L |
| 7 | DR-6/4 | #501 | `feat/series-surfaces` | `tasks repeat` CLI, today-view recurring badges | S |
| 8 | DR-6/3 | #500 | `feat/series-storage-sync` | mig **0014** recur columns, appdata facts, fold dedup | M |
| 8 | DR-5/3 | #488 | `feat/track-cli-export` | `track` CLI family, `export` verb | M |
| 8 | DR-3/4 | #477 | `feat/reminder-escalation` | escalation driver, alarm window, rodio sound | M |
| 9 | undo/1 | #509 | `feat/undo-journal` | command journal, inverse synthesis, `RestoreTask`, mig **0015**, ADR 0019 (w) | M+ |
| 10 | DR-6/6 | #503 | `feat/habit-storage-surfaces` | mig **0016** habit tables, `habits`/`habit-log` sections, CLI + today section | M |
| 11 | undo/2 | #510 | `feat/undo-surfaces` | `tasks undo/redo` CLI, UI router/actions wiring | M |

**Dependency graph — the short answer.** Within a wave, chunks are
parallel-safe (disjoint files; own worktree each). Across waves, order
is forced by four single-flight lanes plus per-chunk gates:

1. **The merge/pipeline lane (hard, includes DR-4's binding
   "never parallelized" ruling).** DR-4/S → G → L → D → R, then
   DR-5/2, then DR-6/3, then DR-6/6 — these seven chunks all edit
   `merge.rs` / `pipeline.rs` / `appdata.rs` / the storage
   repo codec. Never two of them in flight; the wave table already
   spreads them (waves 1→3→4→5→6→8→10).
2. **The command-surface lane (hard).** DR-5/1 → DR-6/2 → DR-6/5 →
   undo/1. All four add `StateCommand` arms or edit the
   `SetTaskDone`/`DeleteTask` planner arms and the `PersistedState`
   mirror (`messages.rs`/`command.rs`/`persistence.rs`/`state.rs`),
   and undo/1's inverse-synthesis catalogue is an exhaustive
   no-wildcard match over `StateCommand` — it must be written against
   the *final* command surface, which is why undo/1 waits for the
   other three (waves 1→5→7→9).
3. **The migration/ADR ladder (hard).** At most one migration-carrying
   chunk in flight; numbers are assigned by landing order (§2). The
   waves encode the assignment; if actual merge order diverges, the
   chunk takes the next free number at its rebase and records the
   deviation.
4. **The insta shape-widener lane (hard).** One **(w)** chunk per wave
   — each regenerates the same shared `.snap` suite, and two
   regenerations interleaved produce unreviewable conflicts.
5. **Soft lanes** (observe when picking parallel sets): at most one
   chunk touching app `cli.rs`/`route_for_ipc` in flight (DR-3/2,
   DR-5/3, DR-6/4, DR-6/6, undo/2), and at most one ui-slint chunk
   (DR-3/3, DR-3/4, DR-5/4, DR-6/4, DR-6/6, undo/2). The waves
   already satisfy both.
6. **undo/1 pull-forward rule**: its three command-surface gates land
   in waves 1/5/7; it may be dispatched in any later wave in which no
   other migration-carrying or `PersistenceAction`-extending chunk is
   in flight (that is why the default puts it in wave 9, alone). It is
   additionally gated on the undo plan's acceptance (Status above).

**Reading order**: the chunk prompt sections follow in §4–§14, one section per wave, arranged for top-to-bottom dispatch — dispatching the document from top to bottom dispatches the whole programme. §3 collects the per-round binding notes each chunk inherits.

**Wave protocol**: a wave begins when every PR of the previous wave is
merged into `main` (the user merges on GitHub; `main` is
branch-protected). Within a wave, dispatch greedily in the listed
order — free disk bounds actual concurrency (each worktree costs
36–55G of build output; `scripts/worktree-doctor.sh` is the gate, and
a nonzero exit stops the dispatch). Before dispatching any chunk,
re-check its gates against the *actual* `origin/main`
(`git fetch origin --prune && git log origin/main --oneline | head -50`).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a worktree.
Every chunk works in `../taskboard-WT-<branch>` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume/second round of a chunk works on
`<branch>-v2` (or re-branches off the fetched `origin/main`), never on
the landed branch. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (flat suffix: `/` in the branch name becomes
`-`; never `git worktree add <path> main`). Disk hygiene (AGENTS §8):
any agent creating or recreating a worktree runs
`scripts/worktree-doctor.sh` from the main checkout first — nonzero
exit (free disk below the floor) stops the chunk; stage 3 removes the
worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: runs `scripts/worktree-doctor.sh`
   from `/home/viktoria/src/taskboard` (nonzero exit ⇒ STOP and
   report), creates the worktree `../taskboard-WT-<branch>` on a new
   branch off the current `main`
   (`git worktree add -b <branch> ../taskboard-WT-<branch>`; never
   without `-b`, never on `main`), verifies the chunk's gates against
   `origin/main` (gated chunks: if a gate has not merged, STOP and
   report instead of planning), writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision, and
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead per
   AGENTS §8; never `git worktree add <path> main`), implements per
   the approved plan — TDD, every commit standalone (rebase-merge
   hygiene), full AGENTS §10 gate suite green. Does **not** push or
   open a PR. Reports deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (AGENTS §8's rebase dance:
   drop already-merged duplicate commits; resolve only real
   conflicts), push `--force-with-lease` if the rebase rewrote
   history — then reviews the entire branch diff versus the
   **post-rebase** base against the chunk's acceptance criteria +
   AGENTS rules **plus the code-quality mandate below**, re-runs the
   full gate suite (any failure is a finding), and implements fixes
   for every finding on the branch (amend/reword, never
   duplicate-subject fixups). Its last tasks are shipping: push the
   branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green — **then verify the PR's merge state**: `gh pr view --json
   mergeStateStatus` must report `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase
   onto `origin/main`, push `--force-with-lease`, let checks re-run,
   repeat; any other state ⇒ diagnose before handing off. **Then
   teardown**: from the project root,
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 — never a
   bare relative path), and confirm via `git worktree list` that the
   removed path was the intended one. The handoff contract is
   "PR open + checks green + merge state `CLEAN` + worktree removed".
   Never merges; never touches `main` (`main` is branch-protected —
   merges happen on GitHub by the user, after the required checks
   pass).

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, the stage-3 agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS §5.
- **Single Responsibility Principle**: every module, actor, function,
  and type has exactly one reason to change; domain stays pure (no
  IO, no runtime), the engine orchestrates without legislating, the
  sync actor only syncs, the CLI only parses/dispatches, the UI layer
  is a dumb presentation client — the crate-role boundaries of
  AGENTS §2 are the SRP line, and a violation of the boundary is a
  violation of SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates
  (calls named operations) or implements (works in primitive data),
  never mixes; guard clauses and the happy path at the top level,
  detail pushed into named private functions. A body that interleaves
  channel plumbing with entity surgery and config reads is a finding,
  even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

## 2. Global resource ladder (dispatch rulings)

The origin plans provisionally claimed migration/ADR numbers against
the same 0009/0016 heads; this dispatch assigns them canonically by
landing order. Every affected stage-1/2 agent re-verifies "next free"
at its own base and records any deviation.

| Resource | Assigned to (wave) | Note |
|---|---|---|
| Migration **0010** | DR-4/G (#481, wave 3) | DR-4's own claim |
| Migration **0011** | DR-4/L (#482, wave 4) | DR-4's own claim |
| Migration **0012** | DR-3/2 (#475, wave 6) | DR-3's plan claimed 0010; **superseded by landing order** — update the issue body's number at dispatch |
| Migration **0013** | DR-5/2 (#487, wave 7) | DR-5's plan anticipated renumbering |
| Migration **0014** | DR-6/3 (#500, wave 8) | DR-6's plan claimed 0012; superseded |
| Migration **0015** | undo/1 (#509, wave 9) | undo plan said "next free at landing" |
| Migration **0016** | DR-6/6 (#503, wave 10) | DR-6 planned "one migration for both halves"; **ruled impossible across separate PRs** — chunk 3 lands the recur columns, chunk 6 lands the habit tables as the next migration; the "one family" intent is preserved by the adjacent landing |
| ADR **0017** | DR-5/1 (#486, wave 1) | matches DR-5's claim |
| ADR **0018** | DR-6/2 (#499, wave 5) | matches DR-6's claim |
| ADR **0019** (expected) | undo/1 (#509, wave 9) | undo plan said "next free, expected 0017"; superseded |

Additional dispatch rulings the origin plans left open:

- **DR-5/3 gates on DR-5/2** (wave 7): the plan left this to the
  dispatch round. Ruled yes — the CLI tests boot real storage in
  hermetic tempdirs, and `export` reads full local history; gating
  avoids a throwaway thin-repo layer.
- **The alarm window rides DR-3/4** (not DR-3/3): the plan allowed
  either; ruled with chunk 4, where the escalation driver that opens
  it lands.

## 3. Round binding notes (each applies to its chunks in the waves below)

### 3.1 DR-4 — sync merge-policy family (`2026-10-26-sync-merge-policy-design-plan.md`)

Binding for all five chunks: §1's constraints (purity, additive
storage, A1 types, test discipline) and the dispatch ruling that the
family is **strictly serial, one agent at a time**; §2's numbered
facts; §4's confirmed forks. ADR 0004 amendments are dated
amendment-not-rewrite.

### 3.2 DR-3 — reminders surface pair (`2026-10-26-reminders-surface-pair-design-round-plan.md`)

Binding for all four chunks: DR-1's R3 settlement semantics
(`.artifacts/plans/2026-10-25-reminder-rule-sync-design-round-plan.md`
§4-R3) are the written authority; the kiosk banner-only fence; the
OS-delivery/visual plumbing stays with GUI review round 3.

### 3.3 DR-2 — signal handling, once (`2026-10-26-signal-handling-design-plan.md`)

Binding for all three chunks: §1's constraints (all app-crate; no
domain/state/storage/sync diffs; no new dependencies; no unsafe;
ADR 0014's handshake shape; ADR 0012's wire additivity; handlers never
installed on test paths), D-0's one policy sentence, D-7's platform
shape, D-8's docs amendments riding their chunks, D-9's seams.

### 3.4 DR-5 — time tracking (`2026-10-26-time-tracking-design-round-plan.md`)

Binding for all four chunks: §4's rulings D-T1–D-T18; §1's fences
(follow-on family #490–#496 stays fenced; DR-4 owns merge semantics —
chunk 2's fold is purely additive-section; no `[tracking]` config;
nothing ticks).

### 3.5 DR-6 — recurring tasks & habits (`2026-10-26-recurring-habits-design-round-plan.md`)

Binding for all six chunks: §6's rulings D-C1–D-C13; §3.8/§4's
non-goals (no RRULE, no virtual instances, no registry entity, no
reminder changes, no pause/resume); the A1 discipline for all new
numeric types; days.rs is the only "local date" vocabulary.

### 3.6 Undo v1 (`2026-10-26-undo-v1-implementation-design-plan.md` + the binding `2026-10-19-undo-architecture-design-plan.md`)

Binding for both chunks: D-U1…D-U5 (journal substrate, pushed-edit
rule, observation machinery untouched, tree journal + `UndoView`,
v1 scope); §7's R-U6…R-U12 (**accepted** 2026-10-26 — recorded in the
undo plan's Status line; undo/1's stage-1 precondition check remains
as a belt-and-braces verification).


## 4. Wave 1 — DR-5/1 · DR-4/S · DR-3/1 · DR-2/R

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 4.1 Chunk DR-5/1 — tracking domain & engine (#486, `feat/tracking-domain-engine`, ADR 0017, (w))

- **Covers**: origin §3 Parts A+B (D-T1–D-T8, normative) + D-T18 + §5
  hooks: the `TimeEntry` entity (open entry = the running timer;
  `task_title` immutable snapshot; A1 span validation at the planner),
  `TrackingState` nested like `ReminderState` (no `running` field —
  derived), `TimeEntryId` in the roaming id family, the five verbs
  with typed receipts/errors (`UnknownTimeEntry`, `InvalidSpan`;
  `NoOp` repeats), `StartTracking`'s close-all-then-open + implicit
  dismissal via the `SetTaskDone` machinery + `UpdateAppData`
  enqueues, `StopTracking` closes all, `SetTaskDone`/`DeleteTask`
  close the task's open entries, `AdjustTimeEntry` can close never
  reopen, the single-global soft invariant + convergence properties,
  `tracked_in_window` splitting projection, ADR **0017**. No storage
  migration here (chunk 2); engine tests use the existing repo tiers.
- **Acceptance**: planner tests on `FixedClock` now (typed receipt
  values; `InvalidSpan` shapes; `NoOp` repeats; Start's dismissal
  emission mirrors `SetTaskDone`'s test set; done/deleted-close side
  effects); the single-global convergence property (after any
  stop/start plan: zero/one open entries; the union of two
  single-global states converges ≤1 at the next write);
  `tracked_in_window` splits spanning entries exactly once per
  boundary (proptest); engine crash-resume test (fresh engine over
  the same repo sees the open entry); insta grows the tracking
  section — one deliberate pass; gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-5/1 of the six-rounds dispatch ("tracking domain & engine", branch feat/tracking-domain-engine) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (§3 Parts A and B are normative decision by decision; §1's anchors (SetTaskDone machinery, ids families, state-home pattern); §5 the domain/planner/engine test hooks; §7 chunk 1) and the dispatch plan §1/§2/§4.1. ADR 0017 (this chunk writes it) records the synced-intent deviation from the 2026-10-14 sketch — D-T1's reason is binding.
2. Read the seams: crates/taskboard-domain/src/{command.rs (plan_command, the SetTaskDone arm and its dismissal emission, CommandOutcome), messages.rs (StateCommand), state.rs (AppState nesting + the determinism doc), persistence.rs (PersistedState mirror), ids.rs/idgen.rs (roaming family, IdGenerator), days.rs (day_window)} and the engine tests tiers (state/tests/).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP), then git worktree add -b feat/tracking-domain-engine ../taskboard-WT-feat-tracking-domain-engine.
4. Write a TDD-first plan at .artifacts/plans/<today>-tracking-domain-engine-plan.md: the entity + state home; the five planner arms' exact semantics (order of operations in StartTracking; the close-all loops; the idempotent NoOps; Adjust's change-mask-never-value rule); the SetTaskDone/DeleteTask side-effect insertion points; the typed receipt/error shapes; the A1 span validation seam; the properties (single-global convergence, tracked_in_window splitting, title-snapshot immutability); the engine crash-resume test design; the ADR 0017 draft. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): time tracking domain and engine. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-5/1 of the six-rounds dispatch ("tracking domain & engine").

1. Read AGENTS.md, the origin plan (§3 Parts A/B, §1, §5), the dispatch plan §1/§2/§4.1, and the stage-1 plan in ../taskboard-WT-feat-tracking-domain-engine (branch feat/tracking-domain-engine). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. NOTHING ticks (elapsed derived at read; no clock-driven task); no `running` field; domain purity holds (now is passed as data); the storage crate shows NO diff in this chunk (no migration, no actions — chunk 2's); engine tests run on the existing repo tiers with injected clocks, poll-with-deadline waits only. SPDX/forbid rules hold; no sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green. Insta grows the tracking section — one deliberate regeneration pass, diff-audited.
4. ADR 0017 rides the final commits (from the stage-1 draft). Do NOT touch backlog issues — stage 3 closes #486. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-5/1 of the six-rounds dispatch (branch feat/tracking-domain-engine).

1. Read AGENTS.md, the origin plan (§3 Parts A/B, §1, §5), the dispatch plan §1/§2/§4.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-tracking-domain-engine.
2. Functional focus: the open set is derived (grep for any `running` field or cached elapsed — a ticking anything is the design's cardinal violation); StartTracking's order is resolve → close-all → open → dismiss → UpdateAppData (a dismissal emitted before the close, or an enqueued op missed, is a finding); SetTaskDone and DeleteTask close the task's open entries at their stamps; AdjustTimeEntry can set an end but structurally cannot clear one (the payload is a change mask — a reopen path is a blocking finding); the title snapshot is immutable under rename (pinned); the single-global properties actually pin convergence after arbitrary command sequences; the storage crate diff is empty.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #486 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 4.2 Chunk DR-4/S — skew interleaving pins (#480, `test/sync-skew-pins`)

- **Covers**: origin §3 D-4b (normative): the pure-seam skew test-pack
  with **zero behavior change** — interleaving (i)-residual as
  `…_currently_pins_skew_echo_shadow_residual`; interleaving (ii) both
  directions as `…_currently_pins_skew_resurrect_flip`; seconds-tie
  (iii) as a plain contract test; one actor-tier hermetic demo that
  the sync actor stamps flow through the injected `Clock`. Plus D-4a:
  the #133 verification comment (F11-01 done, `29b1acb`). No
  production diff. D-4c (the Δ-band guard) is **not** built —
  symptom-triggered, stays documented.
- **Acceptance**: diff is tests + the issue comment only; the named
  pins fail if the described skew behavior ever changes (they read as
  debt, AGENTS §5); the Clock-seam demo asserts stamps came from the
  seam (tokio time pause/advance, no wall clock); existing merge
  properties pass unchanged; gates green.

**Prompt S.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-4/S of the 2026-10-26 six-rounds dispatch ("sync skew pins", branch test/sync-skew-pins) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-4 is normative: D-4a bookkeeping, D-4b the test-pack item by item; §2 facts 10-11 and the C1-C9 interleaving map it cites; §4 fork F-133) and the dispatch plan .artifacts/plans/2026-10-26-design-rounds-batch-implementation-dispatch-plan.md §1/§4.2.
2. Read the seams you will test: crates/taskboard-domain/src/merge.rs (merge_task, the R5 resurrect comparison, the existing tests module), crates/taskboard-sync-nextcloud/src/actor.rs (the injected Clock seam), crates/taskboard-domain/src/entities.rs (TaskClocks, remote_seen).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP and report), then create the worktree: git worktree add -b test/sync-skew-pins ../taskboard-WT-test-sync-skew-pins.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-skew-pins-plan.md: the exact four test scenarios with their deliberately skewed stamps (values, not "ahead by a bit"); why each is currently_pins_ vs contract; the Clock-seam demo design; the #133 comment text (verification note + the documented trigger that would ever reopen the guard). Zero production code — say so explicitly as a diff-shape rule. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): sync skew interleaving pins. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt S.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-4/S of the six-rounds dispatch ("sync skew pins").

1. Read AGENTS.md, .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-4, §2 facts 10-11), the dispatch plan §1/§4.2, and the stage-1 plan (.artifacts/plans/*sync-skew-pins*-plan.md) in the worktree ../taskboard-WT-test-sync-skew-pins (branch test/sync-skew-pins). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan. This chunk is tests-only: any production diff is a finding against your own work. No sleeps; skewed stamps are constructed data; tokio::time::pause/advance for the actor-tier demo; typed/structural assertions only.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT close #133 — the stage-3 agent posts the verification comment per D-4a and the issue stays open. No push, no PR.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt S.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-4/S of the six-rounds dispatch (branch test/sync-skew-pins).

1. Read AGENTS.md, the origin plan (§3 D-4, §2 facts 10-11), the dispatch plan §1/§4.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-test-sync-skew-pins.
2. Functional focus: the diff contains tests and nothing else (a production-code hunk is a blocking finding); each currently_pins_ name matches the interleaving it pins (i)-residual and (ii) in BOTH directions — a pin that would also pass under correct behavior is miswritten; the seconds-tie test is a plain contract test, not currently_pins_; the Clock-seam demo proves the actor's stamps come from the injected seam (not the wall clock); no test asserts log/error text (AGENTS §5).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship + bookkeeping: post the D-4a verification comment on issue #133 (F11-01 done, commit 29b1acb; the Δ-band guard's trigger documented; the issue STAYS OPEN), then per dispatch plan §1 stage-3 protocol: push, open the PR to main, gh pr checks --watch to green, mergeStateStatus CLEAN, worktree teardown with the absolute-anchor command.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 4.3 Chunk DR-3/1 — settlement re-time (#474, `fix/reminder-settlement-retime`)

- **Covers**: origin §3 Part A (D-I1a/b/c, normative) + §5 domain/engine
  hooks: `prune_settled_alarms` keys on delivered ∧ **dismissed**;
  `compact_snooze_chains` keys on both-endpoints-**dismissed**; the
  unobservability properties gain the `active_notifications` reader
  (the second reader — the exact hole the resurrection trace slipped
  through); the §1 resurrection trace becomes a deterministic
  regression test; engine test R9 re-timed (fire → row + in-app row
  survive → dismiss → pruned by that batch's post-pass). **No schema
  change**; domain + state/engine tests only. Chunk 1 also annotates
  #201 (age-pruning re-terms to dismissal rows).
- **Acceptance**: the resurrection trace (fire → snooze → fire →
  dismiss → surface stays empty) passes deterministically; delivered-only
  no longer prunes, dismissal does (both mechanisms); the properties
  hold on arbitrary states over both readers; the fence holds — no
  OS-delivery fixes, no schema; gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-3/1 of the six-rounds dispatch ("settlement re-time", branch fix/reminder-settlement-retime) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md (§3 Part A is normative item by item; §1 the bug facts and the resurrection trace; §5 the test hooks; §7 chunk 1) and the dispatch plan §1/§4.3. DR-1's R3 model (the 2026-10-25 reminder-rule-sync round, §4-R3) is the written authority the re-time enforces.
2. Read the seams: crates/taskboard-domain/src/reminders.rs (prune_settled_alarms, compact_snooze_chains, active_notifications, due_firings, is_dismissed), persistence.rs (the post-batch pass ordering — caller-order is load-bearing), and the tests you must re-pin honestly (state/tests/reminders.rs R9, the compaction-unobservability property).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP), then git worktree add -b fix/reminder-settlement-retime ../taskboard-WT-fix-reminder-settlement-retime.
4. Write a TDD-first plan at .artifacts/plans/<today>-reminder-settlement-retime-plan.md: the two new criteria exactly; the property extensions (invariance of active_notifications under the post-pass on arbitrary states — both readers); the regression test script; the R9 re-timing (fire survives, dismiss prunes) with the citation to R3 as the superseded-contract authority (AGENTS §5's no-weakening rule satisfied the honest way); the explicit no-schema-change rule; the #201 annotation. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): reminder settlement keyed to dismissal. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-3/1 of the six-rounds dispatch ("settlement re-time").

1. Read AGENTS.md, the origin plan (§3 Part A, §1 bug facts, §5), the dispatch plan §1/§4.3, and the stage-1 plan in ../taskboard-WT-fix-reminder-settlement-retime (branch fix/reminder-settlement-retime). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Domain + tests only: no migration, no storage diff, no app/ui diff, and NOTHING from the OS-delivery plumbing queue (GUI review round 3's fence). The properties must gain the second reader — extending only due_firings coverage is the exact hole this chunk closes. Fixed/stepping clocks; no sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Annotate #201 in the final commit's PR body notes (age-pruning re-terms to dismissal rows) — do not close it. Do NOT close #474 — stage 3 does. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-3/1 of the six-rounds dispatch (branch fix/reminder-settlement-retime).

1. Read AGENTS.md, the origin plan (§3 Part A, §1, §5), the dispatch plan §1/§4.3 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-fix-reminder-settlement-retime.
2. Functional focus: prune keys on delivered ∧ dismissed and compaction on both-endpoints-dismissed — delivered-ness is gone from both criteria; between fire and dismiss the alarm row AND its delivery survive (SnoozeReminder/CancelAdhocAlarm on a fired alarm resolve — pinned); the resurrection trace test is deterministic and fails under the old criteria; the properties cover active_notifications, not just due_firings (the second reader is the point); R9 is re-timed, not weakened (persistence half of the pin intact); diff shape is domain+tests only — any migration/storage/app hunk is a fence violation.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #474 with the PR (note the #201 annotation), then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 4.4 Chunk DR-2/R — REPL signals (#469, `feat/repl-signal-cancel`)

- **Covers**: origin §3 D-1/D-2/D-3/D-4 (normative) + §6 chunk R: the
  new `taskboard-app/src/signals.rs` (the D-0 policy sentence in its
  doc comment, the pure ladder state machine, the watcher with
  restore-before-exit), the session loop's cancellation `select!` +
  typed `CommandError::Cancelled` ("cancelled" kind, same envelope),
  the force rung **in the watcher**, ceremony `Line`/`Confirm` prompts
  moved onto rustyline (masked stays rpassword; completion suspended
  for prompts; history untouched; the stderr→stdout consequence
  recorded), one-shot ceremony exclusion, the REPL ladder watcher's
  SIGTERM/SIGHUP restore-then-graceful, pty pin updates, ADR 0011 +
  0014 dated amendments. Closes #174/#192/#200.
- **Acceptance**: hermetic loop tests fire the injected cancellation
  mid-dispatch (typed `cancelled`, session continues, exactly one
  `Next`); the ladder state machine is pure and exhaustively
  unit-tested over event interleavings; prompt grammar tests answer
  `Err(Interrupted)`; the pty tier proves: ^C during a stalled line
  cancels and the next line executes; SIGTERM mid-readline leaves the
  pane sane (stty probe); second ^C forces with 130; masked-read ^C
  becomes typed cancel with nothing persisted; gates green.

**Prompt R.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-2/R of the six-rounds dispatch ("REPL signals", branch feat/repl-signal-cancel) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-signal-handling-design-plan.md (§3 D-0/D-1/D-2/D-3/D-4 and §3 D-9's seams are normative; §2 facts F1-F16 — F8/F9 are why the prompt move is not optional; §6 chunk R; §5 risks) and the dispatch plan §1/§4.4.
2. Read the seams: crates/taskboard-app/src/{repl.rs (the editor thread's three states, the session loop's Execute arm, TtyStateGuard), commands/connect.rs (the ceremony prompt machinery), session_ceremonies.rs, cli_tty.rs (the pty harness + the masked-read pin)}, and the ADR 0011/0014 sections D-8 amends.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP), then git worktree add -b feat/repl-signal-cancel ../taskboard-WT-feat-repl-signal-cancel.
4. Write a TDD-first plan at .artifacts/plans/<today>-repl-signal-cancel-plan.md: signals.rs's ladder state machine (events, actions, per-surface rung configs) as a pure exhaustive unit; the watcher's side effects (restore-before-exit ordering, the force exit) as thin untested glue per house rule; the session loop's select! wiring and the injected cancellation seam; the rustyline prompt move (helper suspension, history/completion exclusion, the Interrupted mapping, the stderr→stdout recording); the one-shot exclusion rationale; the SIGTERM/SIGHUP restore-then-graceful path; the pty tier's new/changed cases; the ADR amendments' wording. Handlers never on test paths (D-7). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): REPL signal cancellation and prompt grammar. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt R.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-2/R of the six-rounds dispatch ("REPL signals").

1. Read AGENTS.md, the origin plan (§3 D-0…D-4, D-7, D-9; §2 facts), the dispatch plan §1/§4.4, and the stage-1 plan in ../taskboard-WT-feat-repl-signal-cancel (branch feat/repl-signal-cancel). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. All app-crate; no new dependencies (tokio::signal + nix term are already workspace features); no unsafe; unix-only signals cfg-gated; handlers installed only on production paths; a cancelled line's already-delivered engine command still commits (orphan completion — documented in the envelope's UX text, not hidden); prompts never enter history or completion; secrets never log. No sleeps; typed outcomes; pty tests are the only real-signal tests.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. ADR 0011 + 0014 dated amendments ride the final commits. Do NOT touch backlog issues — stage 3 closes #174/#192/#200/#469. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt R.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-2/R of the six-rounds dispatch (branch feat/repl-signal-cancel).

1. Read AGENTS.md, the origin plan (§3 D-0…D-4/D-7/D-9, §2, §5), the dispatch plan §1/§4.4 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-repl-signal-cancel.
2. Functional focus: the force rung lives in the WATCHER, not the loop (a select! inside the loop can never rescue a future blocked without yielding — a loop-side force is a design finding); every installing surface selects on cancellation or arms the force rung (the never-silently-swallow rule); ^C at the prompt still clears the line (F5's byte world untouched); one-shot ceremonies install no handler and keep death semantics; prompt answers bypass history/completion (structurally pinned); the diff touches only the app crate + docs — any domain/state/storage/sync hunk is a binding-constraint violation; the pty pins' changed semantics cite the design (not weakened: the persistence half is intact).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #174, #192, #200, #469 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 5. Wave 2 — DR-6/1 · DR-2/I

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 5.1 Chunk DR-6/1 — series domain core (#498, `feat/series-domain-core`, (w))

- **Covers**: origin §3.1–§3.3 + §3.9 (properties 1–5) + §7 chunk 1
  (normative): `RecurRule`/`RecurPattern`/`RecurBound` with A1 mints
  (`EveryDays`, `EveryWeeks`, `WeekdaySet`, `MonthlyAnchor` —
  fallible constructors rejecting zero/empty; no derive crates;
  documented leniency at the decode boundary), the total pure
  functions `next_after`/`occurs_on`/`schedule`/`roll_target`/
  `occurrence_outcome` with anchor-aligned semantics (clamping
  included), the `Task.recur: Option<RecurRule>` field-family
  integration (`TASK_FIELDS` row, `TaskClocks::ck_recur`,
  `TaskChanges` double-Option) + `SeriesId`/`HabitId` id kinds +
  `continued_from` seams, the serde round-trip property. Storage
  migration is NOT this chunk.
- **Acceptance**: proptest catalogue 1–5 passes (total/strictly-after/
  member; anchor alignment equals schedule enumeration; monthly
  clamping one-per-month; roll_target laws incl. offline gaps and
  Daily{1}=today; attribution partition + late-completion rule);
  round-trip lossless, truncated views fold as typed failures; the
  field-family lockstep compiles (TASK_FIELDS/TaskClocks/TaskChanges
  all touched); gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/1 of the six-rounds dispatch ("series domain core", branch feat/series-domain-core) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md (§2's type-driven policy) and docs/coding_guidelines.org §2.1, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§3.1-§3.3, §3.5, §3.9 properties 1-5, §5.4's resource claims, §7 chunk 1 — normative) and the dispatch plan §1/§5.1.
2. Read the seams: crates/taskboard-domain/src/{entities.rs (Task, TASK_FIELDS, TaskClocks, the field-family precedent from effort/importance/urgency), messages.rs (TaskChanges double-Option), days.rs (day_window — the only local-date vocabulary), ids.rs (the local_id! macro), appdata.rs (the decode-leniency posture)}.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP), then git worktree add -b feat/series-domain-core ../taskboard-WT-feat-series-domain-core.
4. Write a TDD-first plan at .artifacts/plans/<today>-series-domain-core-plan.md: the type inventory with every A1 mint's invariant and error; the arithmetic functions' exact contracts (anchor alignment; clamping; roll_target's two-case law); the field-family integration checklist (manifest row, clock slot, TaskChanges arm, appdata-lane decision); the UTC-conversion boundary (scheduled/duedate stay DateTime<Utc>); the proptest catalogue mapped to origin §3.9 items 1-5 + round-trip; the decode-leniency behavior for invalid stored rows. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): recurring series domain core. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/1 of the six-rounds dispatch ("series domain core").

1. Read AGENTS.md, the origin plan (§3.1-§3.3, §3.9, §7 chunk 1), the dispatch plan §1/§5.1, and the stage-1 plan in ../taskboard-WT-feat-series-domain-core (branch feat/series-domain-core). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Pure domain only: no IO, no runtime, no clock reads (dates in, dates out); hand-rolled newtypes with fallible mints (no derive crates); every numeric component's zero/empty rejection tested; decode-side leniency documented at the type and folding as typed failures, never poisoning state. SPDX header (MIT OR Apache-2.0) + #![forbid(unsafe_code)]. No sleeps; proptests over generated triples.
3. Run the full gate suite from AGENTS.md §10 and make it green. Insta dumps regenerate (Task.recur joins the shape) — one deliberate pass.
4. Do NOT touch backlog issues — stage 3 closes #498. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/1 of the six-rounds dispatch (branch feat/series-domain-core).

1. Read AGENTS.md, the origin plan (§3.1-§3.3, §3.9), the dispatch plan §1/§5.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-series-domain-core.
2. Functional focus: anchor alignment is the phase law — next_after enumerates schedule() exactly (property 2 is the one that kills drift bugs; verify it exists and is strong); Monthly clamping yields exactly one member per month; roll_target's laws hold for offline gaps (today >> stale) and Daily{1}=today; all mints reject zero/empty at the constructor (a pub tuple-field or non-fallible constructor is an A1 violation); TASK_FIELDS/TaskClocks/TaskChanges moved in lockstep (a field joined without all three is a compile-or-review catch — check all three); dates cross the UTC boundary only at the task-field seam; no RRULE strings anywhere.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #498 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 5.2 Chunk DR-2/I — IPC client cancellation + deadlines (#470, `feat/ipc-client-cancel`)

- **Covers**: origin §3 D-5 (normative, items 1–4) + §6 chunk I: the
  one-shot transport/dispatch cancellation select + typed `cancelled`
  envelope + exit 130 (ceremony verbs excluded per D-3), the derived
  client read deadline (verb's `--timeout-secs` + 10 s margin, else
  30 s default, `[ipc] client_read_timeout_secs` override, 0
  disables) with typed `CommandError::IpcTimeout` ("ipc-timeout",
  exit 1), the `Shutdown` control lane answered in the accept path
  without entering the serial dispatcher, the no-`Cancel`-verb
  recording, ADR 0012's dated amendment. Closes #196.
- **Acceptance**: deadline expiry pinned against a scripted stalling
  server (tokio time pause/advance — never fires before the verb's
  own honest timeout); the control lane answers `daemon stop` while a
  dispatcher task is parked on `pending()`; wire shape unchanged (no
  PROTOCOL_VERSION bump); ^C gives the typed envelope + exit 130 with
  tidy teardown; gates green.

**Prompt I.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-2/I of the six-rounds dispatch ("IPC client cancellation", branch feat/ipc-client-cancel).

Precondition: DR-2/R (feat/repl-signal-cancel) has merged — the signals.rs ladder helper is its dependency. Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-signal-handling-design-plan.md (§3 D-5 is normative item by item; §2 facts F11-F15; §6 chunk I; §5 risks) and the dispatch plan §1/§5.2.
2. Read the seams: crates/taskboard-app/src/ipc/{client.rs (the no-deadline reads, connect budget), server.rs (the serial dispatcher, the accept-path decode, drain_tests), protocol.rs}, config.rs (the [ipc] section seam), signals.rs (DR-2/R's ladder), and ADR 0012 §4.2/§4.8.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/ipc-client-cancel ../taskboard-WT-feat-ipc-client-cancel.
4. Write a TDD-first plan at .artifacts/plans/<today>-ipc-client-cancel-plan.md: the cancellation select's placement (transport arm + in-process dispatch; ceremony exclusion); the deadline derivation table (which verbs have --timeout-secs, the margin, the default, the override, 0-disables) and IpcTimeout's typed shape; the control-lane change in the accept path (why it is wire-additive; the old-daemon behavior); the no-Cancel recording text; the test design (stalling server, parked dispatcher, pause/advance). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): IPC client cancellation, deadlines, control lane. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt I.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-2/I of the six-rounds dispatch ("IPC client cancellation").

1. Read AGENTS.md, the origin plan (§3 D-5, §2 facts F11-F15), the dispatch plan §1/§5.2, and the stage-1 plan in ../taskboard-WT-feat-ipc-client-cancel (branch feat/ipc-client-cancel). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. All app-crate; the wire is additive-only (same variant, same envelope, earlier handling — no PROTOCOL_VERSION bump, no envelope shape change); the deadline is a tokio::time::timeout and never fires before the verb's own budget; the serial dispatcher keeps its one-in-flight fairness for everything except Shutdown. No sleeps (pause/advance); typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. ADR 0012's dated amendment rides the final commits. Do NOT touch backlog issues — stage 3 closes #196/#470. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt I.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-2/I of the six-rounds dispatch (branch feat/ipc-client-cancel).

1. Read AGENTS.md, the origin plan (§3 D-5, §2, §5), the dispatch plan §1/§5.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ipc-client-cancel.
2. Functional focus: Shutdown is answered OUTSIDE the serial queue (a decoded Shutdown reaching the dispatcher is the F12 starvation bug surviving — a blocking finding); everything else keeps one-in-flight fairness; the deadline derivation matches the plan's table exactly and can never preempt an honest long verb; ipc-timeout is distinct from transport garbage in the typed envelope; ^C exits 130 with the typed cancelled envelope; no Cancel verb was added; protocol.rs and PROTOCOL_VERSION are untouched; ceremony verbs are excluded from cancellation.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #196 and #470 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 6. Wave 3 — DR-4/G · DR-3/3

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 6.1 Chunk DR-4/G — remote baseline + changed-field merge (#481, `feat/sync-remote-baseline`, mig 0010, (w))

- **Covers**: origin §3 D-1 (normative, a–f) + §6 chunk G: the
  `RemoteTaskBaseline` record (remote vocabulary, raw ids), the
  `apply_remote(f)` changed-field gate in `merge_task`, baseline
  refresh at **every** adoption point (R1 `adopt_remote_task`, the
  merge, the R9 push echo), tombstone clears, the D-1d blind-spot pin
  (`currently_pins_`), migration 0010 (`tasks.remote_baseline TEXT
  NULL`) + codec + upgrade test, the anti-clobber proptest, the
  granularity-trap pin flipped to the closed behavior, ADR 0004's
  dated amendment. Scope: **tasks only** (D-1e). No conflict surfacing
  (D-1f), no boards/stacks/labels baseline.
- **Acceptance**: for any baseline-equipped task and any remote
  snapshot diffing only in field set S, fields outside S are unchanged
  *including clocks* (proptest); a missing baseline falls back to
  today's blanket LWW for that cycle and converges (upgrade test pins
  a pre-migration row merging identically); the push→echo→pull round
  trip leaves no spurious diff; the old granularity-trap test now pins
  the trap closed; gates green.

**Prompt G.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-4/G of the six-rounds dispatch ("remote baseline", branch feat/sync-remote-baseline).

Precondition: DR-4/S (test/sync-skew-pins) has merged into origin/main. Verify with git log origin/main --oneline; if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-1 a-f is normative; §2 facts 1-4, 9, 16; §4 F-128; §5 the baseline-drift risk) and the dispatch plan §1/§2/§6.1.
2. Read the seams: crates/taskboard-domain/src/{merge.rs (merge_task per-field loop, adopt_remote_task, adopt_task_after_push), entities.rs (Task, TaskClocks, remote_seen), remote.rs (RemoteTask), pipeline.rs (the R4 gate, the R3 tombstone path, the echo path)}, crates/taskboard-storage-sqlite/migrations/ (verify 0010 is next free), the storage codec for tasks, and the merge tests you must not break (the granularity-trap test among them).
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/sync-remote-baseline ../taskboard-WT-feat-sync-remote-baseline.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-remote-baseline-plan.md: the baseline record's exact fields and serde shape (remote vocabulary, raw ids — resolution stays per-cycle via RemoteIndex); the changed-field gate as a pure function; the exhaustive list of adoption points that must refresh the baseline (grep them; missing one is the chunk's classic bug) and the tombstone clears; the fallback rule for a missing baseline; the migration + codec + upgrade test; the proptest catalogue (anti-clobber, idempotence, echo round-trip, the blind-spot pin); the ADR 0004 amendment wording. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): sync remote baseline and changed-field merge. Do not implement; do not push.

Report: plan path, decisions (esp. the adoption-point inventory), risks, estimated diff size.
```

**Prompt G.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-4/G of the six-rounds dispatch ("remote baseline").

1. Read AGENTS.md, the origin plan (§3 D-1, §2 facts 1-4/9/16), the dispatch plan §1/§2/§6.1, and the stage-1 plan in ../taskboard-WT-feat-sync-remote-baseline (branch feat/sync-remote-baseline). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Migration 0010 is additive-only plain SQL (NULL-defaulted; ADR 0005 policy) with an upgrade test pinning bit-for-bit pre-migration behavior. Domain stays pure — the baseline is injected data on the entity, no clocks, no IO. SPDX header + #![forbid(unsafe_code)] rules hold for touched crates. No sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green. The insta dump suite regenerates in this chunk (Task shape widens) — one deliberate pass, diff-audited as "the widening, nothing else".
4. ADR 0004's dated amendment rides this chunk's final commits (D-1's gate + the amended loss-class paragraph). Do NOT touch backlog issues — stage 3 closes #128/#481. No push, no PR.

Report: per-gate results, the adoption points you touched (checklist against the plan), any deviation and why.
```

**Prompt G.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-4/G of the six-rounds dispatch (branch feat/sync-remote-baseline).

1. Read AGENTS.md, the origin plan (§3 D-1 a-f, §2 facts 1-4/9/16), the dispatch plan §1/§2/§6.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-sync-remote-baseline.
2. Functional focus: EVERY adoption point refreshes the baseline — R1 adoption, the merge itself, the R9 push echo (its post-normalization values are the new reference), and the tombstone paths clear it; a missed one turns the next pull's normalization delta into a spurious field change, so grep for adoption paths and check each against the plan's inventory; unapplied fields keep their local clocks (an unapplied observation advances nothing); remote_seen := ts_r always; the missing-baseline fallback is bit-for-bit today's behavior for that cycle (upgrade test present and meaningful); the granularity-trap test now pins the CLOSED trap and the revert blind spot is a currently_pins_ pin; the anti-clobber proptest includes clocks in its "unchanged" assertion; scope is tasks-only (any boards/stacks/labels baseline is scope creep).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #128 and #481 with the PR (comment linking the evidence), then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 6.2 Chunk DR-3/3 — timers & alarms surface (#476, `feat/timers-alarm-surface`)

- **Covers**: origin §3 Part D (D-I6–D-I8, normative) + §5 UI hooks:
  the `timers` composition in the today view + the status-bar chip
  (nearest fire, desktop-only), active rows (label, countdown via the
  wall-minute repaint, insistence badge, cancel), the **notification
  surface owns awaiting rows** with anchor-aware buttons
  (Dismiss/Snooze/Cancel — no Mark Done on timer rows, no Mute; the
  four-buttons-regardless-of-anchor row-template repair rides here),
  the compact awaiting-count pointer, the inline creation form
  (`in`/`at` grammar — new pure absolute-time presentation fn — plus
  the insistence picker), `ActionId::TimerCreate`/`TimerCancel` only,
  no UI-held timer state. The alarm **window** is NOT this chunk
  (dispatch ruling: it rides DR-3/4).
- **Acceptance**: pure row/phrase/countdown functions unit-tested
  (structural, no slint run loop); countdown re-renders on the
  existing minute repaint (no new tick); snoozed alarms leave the
  awaiting set by derivation and return at the target; the kiosk
  model-side gate hides the surface; golden visuals extend per the
  harness; gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-3/3 of the six-rounds dispatch ("timers surface", branch feat/timers-alarm-surface).

Precondition: DR-3/1 (fix/reminder-settlement-retime) has merged into origin/main — the awaiting-row affordances need the re-timed settlement to be testable end-to-end. Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md (§3 Part D is normative; §5 the UI test hooks; §7 chunk 3; ADR 0015's thin-client rules) and the dispatch plan §1/§6.2.
2. Read the seams: crates/taskboard-ui-slint/src/{presentation.rs (notification_phrase precedent), models.rs (push_notifications and its kiosk gate), adapter.rs, actions.rs, router.rs, ui/views/today-view.slint, ui/compositions/status-bar.slint, ui/compositions/notification-surface.slint}, and the domain reads the rows derive from (state.reminders.alarms, alarm_occurrence, effective-occurrence walk).
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/timers-alarm-surface ../taskboard-WT-feat-timers-alarm-surface.
4. Write a TDD-first plan at .artifacts/plans/<today>-timers-alarm-surface-plan.md: the section/chip composition structure; the pure presentation functions (countdown phrase, canonical order, chip visibility, kiosk gate, awaiting-count pointer, the new absolute-time parser) with their unit tests; the anchor-aware button visibility rule as a pure per-row decision; the creation form's grammar and dispatch; the two new ActionIds and router arms; the explicit non-goal (no alarm window — DR-3/4's). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): timers and alarms surface. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-3/3 of the six-rounds dispatch ("timers surface").

1. Read AGENTS.md, the origin plan (§3 Part D, §5), the dispatch plan §1/§6.2, and the stage-1 plan in ../taskboard-WT-feat-timers-alarm-surface (branch feat/timers-alarm-surface). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The UI stays a dumb client: rows derive from the ArcSwap snapshot per frame; data changes repaint via StateUpdated, time passage via the existing slint timers; no UI-held timer state machine; no new view family. Pure presentation functions live in presentation.rs and are unit-tested without a slint run loop; no TTY; no text assertions on labels.
3. Run the full gate suite from AGENTS.md §10 and make it green (golden visuals extend per the existing harness).
4. Do NOT touch backlog issues — stage 3 closes #476. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-3/3 of the six-rounds dispatch (branch feat/timers-alarm-surface).

1. Read AGENTS.md, the origin plan (§3 Part D, §5, §1's notification-surface facts), the dispatch plan §1/§6.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-timers-alarm-surface.
2. Functional focus: awaiting rows live ONLY on the notification surface (a duplicate awaiting list in the timers section is the one-source-of-truth violation); the timer rows' buttons are anchor-aware (no Mark Done, no Mute on timer rows — and the row-template repair means task rows keep all four); countdown is derived per frame (grep for any interval/tick state held UI-side); snoozed alarms exit the awaiting set by derivation and reappear at the target (pinned); ActionId growth is exactly TimerCreate/TimerCancel; the kiosk gate empties the surface model-side; the alarm window is absent (DR-3/4's scope).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #476 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 7. Wave 4 — DR-4/L · DR-2/D

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 7.1 Chunk DR-4/L — OR-set label memberships (#482, `feat/sync-orset-labels`, mig 0011, (w))

- **Covers**: origin §3 D-2 (normative, a–e) + §6 chunk L: the
  `MembershipState` model and `label_memberships: BTreeMap<LabelId,
  MembershipState>` with `live_labels()` (the bare `labels` set
  retires — full reader sweep), local verb stamping
  (`AssignLabel`/`UnassignLabel`), remote events diffed against the
  G baseline's `baseline.labels`, the D-2d outcome table verbatim,
  label-entity death dominance, migration 0011
  (`task_labels.state`/`ck_membership`, epoch backfill), unchanged
  push shape, the proptest battery, ADR 0004's dated amendment.
- **Acceptance**: concurrent two-device adds both survive; a remove
  wins only strictly newer; removed rows are never deleted (re-add
  flap pinned as impossible); unchanged remote sets emit no events
  (Deck echo invisible); entity death forces removal and kills
  concurrent assignment; upgrade test pins epoch backfill
  bit-for-bit; every former `task.labels` reader compiles against the
  accessor; gates green.

**Prompt L.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-4/L of the six-rounds dispatch ("OR-set labels", branch feat/sync-orset-labels).

Precondition: DR-4/G (feat/sync-remote-baseline) has merged into origin/main (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-2 a-e is normative — the outcome table is binding verbatim; §2 facts 2, 8; §4 F-129; §5 the reader-sweep risk) and the dispatch plan §1/§2/§7.1.
2. Read the seams: crates/taskboard-domain/src/{entities.rs (Task.labels and its readers), merge.rs (the R7 labels branch, the baseline diff from G), command.rs (AssignLabel/UnassignLabel planners), pipeline.rs (the labels merge path)}, the label join-table codec + migration 0011 slot (verify next free), the outbox ops, and grep the workspace for every `task.labels` / `.labels` reader (domain, state, ui-slint presentation, app) — the sweep list goes in the plan.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/sync-orset-labels ../taskboard-WT-feat-sync-orset-labels.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-orset-labels-plan.md: the MembershipState A1 shape and live_labels() accessor; the verb stamping; the remote-event derivation from baseline.labels; the D-2d outcome table as an exhaustive pure function; the entity-death dominance rule; the migration + backfill + upgrade test; the reader sweep as an explicit checklist; the proptest battery (commutativity, concurrent-add survival, remove-needs-newer, no-flap, entity-death); the ADR 0004 amendment wording. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): OR-set label memberships. Do not implement; do not push.

Report: plan path, decisions, the reader-sweep inventory, risks, estimated diff size.
```

**Prompt L.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-4/L of the six-rounds dispatch ("OR-set labels").

1. Read AGENTS.md, the origin plan (§3 D-2, §2 facts 2/8), the dispatch plan §1/§2/§7.1, and the stage-1 plan in ../taskboard-WT-feat-sync-orset-labels (branch feat/sync-orset-labels). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The reader sweep is its own mechanical commit; the migration (0011) is additive with epoch backfill + upgrade test; removed membership rows persist (never deleted while either entity lives); the card PUT's label set is unchanged on the wire. SPDX/forbid rules hold; no sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green. Insta dumps regenerate (Task shape changes) — one deliberate pass.
4. ADR 0004's labels amendment rides the final commits. Do NOT touch backlog issues — stage 3 closes #129/#482. No push, no PR.

Report: per-gate results, reader-sweep completion checklist, any deviation and why.
```

**Prompt L.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-4/L of the six-rounds dispatch (branch feat/sync-orset-labels).

1. Read AGENTS.md, the origin plan (§3 D-2 a-e, §2 facts 2/8), the dispatch plan §1/§2/§7.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-sync-orset-labels.
2. Functional focus: the D-2d outcome table is implemented exhaustively and its remove-vs-add strictness is exactly as written (a remove beats an add only when strictly newer; adds at tie survive); removed memberships are never garbage-collected (a forgetting path is the re-add flap — a blocking finding); the baseline diff emits NO events for an unchanged remote set (Deck cache-lag echo invisibility is pinned); entity-death dominance (force-remove + concurrent assignment dies with the entity); the reader sweep is complete — grep the merged tree for any surviving direct field read; the push-side card JSON shape is unchanged; the upgrade test pins epoch backfill.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #129 and #482 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 7.2 Chunk DR-2/D — daemon signal ladder + exit codes (#471, `feat/daemon-signal-ladder`)

- **Covers**: origin §3 D-6/D-7 (normative) + §6 chunk D: the
  three-rung daemon ladder (graceful drain → skip-joins + abort +
  2 s salvage flush + `128+signum` → immediate `_exit`), the exit-code
  contract (0 clean / **1 degraded drain — today's exit-0 lie fixed** /
  130/143 forced), the GUI and kiosk tails migrated onto the shared
  ladder helper (their `drain_and_shutdown` sites), the runbook
  (exit-code table, systemd `Restart=`/`SuccessExitStatus=` mapping,
  `stty sane` recovery), the `contrib/` kiosk service update,
  CHANGELOG Unreleased entry. Closes #162.
- **Acceptance**: the ladder state machine's interleavings (signal ×
  progress) are unit-tested pure; a degraded drain exits 1 (pinned);
  rung-2's salvage flush is one bounded call; GUI/kiosk tails consume
  the same helper (no local ladder logic); the runbook + contrib
  service land; gates green.

**Prompt D.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-2/D of the six-rounds dispatch ("daemon ladder", branch feat/daemon-signal-ladder).

Precondition: DR-2/I (feat/ipc-client-cancel) has merged (the ladder helper is shared). Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-signal-handling-design-plan.md (§3 D-6/D-7 are normative — the rung table and exit-code contract verbatim; §2 facts F1-F3, F15; §6 chunk D; §5 the exit-code-churn risk) and the dispatch plan §1/§7.2.
2. Read the seams: crates/taskboard-app/src/{main.rs (the daemon tail, the second-signal force, the degraded-drain exit), bootstrap.rs (wait_for_signal, drain_and_shutdown), ui.rs (the kiosk watcher)}, signals.rs (the ladder helper from DR-2/R), the runbook docs/kiosk_bringup_runbook.org, and contrib/'s kiosk service file.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/daemon-signal-ladder ../taskboard-WT-feat-daemon-signal-ladder.
4. Write a TDD-first plan at .artifacts/plans/<today>-daemon-signal-ladder-plan.md: the rung configuration for the daemon surface on the shared state machine; the salvage flush's bound and failure handling; the degraded-drain detection (what exactly flips exit 0 → 1); the GUI/kiosk adoption (each tail's diff); the runbook + contrib edits; the CHANGELOG entry; the test catalogue (pure ladder interleavings; the exit-code pins at the seam — how to assert exit codes without process spawns, or justify the spawn). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): daemon signal ladder and exit codes. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt D.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-2/D of the six-rounds dispatch ("daemon ladder").

1. Read AGENTS.md, the origin plan (§3 D-6/D-7, §2 facts), the dispatch plan §1/§7.2, and the stage-1 plan in ../taskboard-WT-feat-daemon-signal-ladder (branch feat/daemon-signal-ladder). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. All app-crate + docs; the ladder logic exists once (in signals.rs — a local reimplementation in a tail is a finding); restore-before-join ordering holds for external termination; unix-only signals cfg-gated; SIGQUIT keeps default disposition; no new dependencies. No sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. The runbook + contrib service + CHANGELOG ride the final commits. Do NOT touch backlog issues — stage 3 closes #162/#471. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt D.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-2/D of the six-rounds dispatch (branch feat/daemon-signal-ladder).

1. Read AGENTS.md, the origin plan (§3 D-6/D-7, §2, §5), the dispatch plan §1/§7.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-daemon-signal-ladder.
2. Functional focus: the rung table matches D-6 exactly (rung 2 skips joins, aborts actors, one bounded salvage flush; rung 3 is _exit); a degraded drain exits 1 — the exit-0 lie is dead and pinned; 130/143 only at rung 2+; clean full drains — including signal-initiated graceful and daemon stop — still exit 0; GUI and kiosk consume the shared helper (grep for duplicated ladder logic); the runbook's systemd mapping matches the implemented codes; SIGQUIT is untouched.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #162 and #471 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 8. Wave 5 — DR-6/2 · DR-4/D · DR-5/4

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 8.1 Chunk DR-6/2 — series engine: spawn + roll (#499, `feat/series-engine`, ADR 0018)

- **Covers**: origin §3.3–§3.5, §3.7 (normative) + §7 chunk 2: the
  `SetTaskDone` spawn arm **inside the same pure plan** (successor
  contents: copied title/description/labels/stack, midpoint insertion
  after the head — the amended ruling, never naive `+1` into an
  occupied slot; scheduled/duedate from `next_after` through the
  day-window conversion with the documented fallbacks; `recur` copied
  verbatim; reminder rules replicated with fresh task id, history
  not copied; muted state replicates; bounds suppress + clear per
  §3.7), the `day_tick` roll arm beside `reminder_tick` (engine-
  internal watermark; stale-head detection; `roll_target` field edit
  with fresh clocks; quiet day = full no-op; boot catch-up = same
  path), bounds lifecycle (`Until`/`Count` evaluated against
  derivable history), task→series / series→task via `UpdateTask`, the
  spawn-race note (fold dedup is chunk 3's), snapshot/insta coverage,
  ADR **0018**. Serial after DR-5/1 (command-surface lane:
  `SetTaskDone` is shared).
- **Acceptance**: spawn pinned by insta + planner tests (successor
  contents item by item; midpoint insertion; bounds; no history
  copy); day_tick pinned (stale head rolls to target; quiet day
  no-ops; watermark never persists as user state; idempotent under
  repeated ticks); `SetTaskDone` non-recur behavior byte-identical;
  reminder re-arm by construction (new occurrence keys) pinned; gates
  green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/2 of the six-rounds dispatch ("series engine", branch feat/series-engine).

Preconditions: DR-6/1 (#498) has merged, and DR-5/1 (#486) has merged (command-surface lane: both edit the SetTaskDone arm — this chunk follows the other). Verify both (git log origin/main --oneline); if either is missing, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§3.3-§3.5, §3.7 are normative decision by decision; §1 facts 4-6 (the planner seam, the reminder occurrence model, the engine tick); §5.3 the undo interplay note; §7 chunk 2) and the dispatch plan §1/§8.1.
2. Read the seams: crates/taskboard-domain/src/command.rs (the SetTaskDone planner arm — including whatever DR-5/1 added there), crates/taskboard-state/src/actor.rs (reminder_tick, the injected Clock), the reminder-rule upsert machinery the replication reuses, the midpoint/ordering convention in MoveTask/the UI drag, and the insta snapshot corpus.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/series-engine ../taskboard-WT-feat-series-engine.
4. Write a TDD-first plan at .artifacts/plans/<today>-series-engine-plan.md: the spawn sequence inside plan_command (every successor field with its source; the midpoint rule with the occupied-slot case; the bound checks; the rule-clearing terminal transitions); the day_tick arm (watermark mechanics, stale predicate, batch persistence ordering, the no-op contract); the interplays (reminder replication without history; muted replication; the undo note recorded for the undo round); the test catalogue (planner + insta scenarios + tick determinism). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): series completion spawn and day-boundary roll. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/2 of the six-rounds dispatch ("series engine").

1. Read AGENTS.md, the origin plan (§3.3-§3.5, §3.7, §1 facts 4-6), the dispatch plan §1/§8.1, and the stage-1 plan in ../taskboard-WT-feat-series-engine (branch feat/series-engine). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. plan_command stays pure and total (the spawn is more plan output — no async, no tick inside the planner); the day_tick mirrors reminder_tick's shape (read the injected Clock once, persist-then-publish); the watermark is engine-internal and never user state; the roll is an ordinary field edit with fresh clocks and ordinary outbox push; Count evaluation derives from completion history (no counter column). Fixed/stepping clocks; no sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green (new insta scenarios for spawn/roll; existing dumps shift only by the scenario's own recur content).
4. ADR 0018 rides the final commits (from the stage-1 draft). Do NOT touch backlog issues — stage 3 closes #499. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/2 of the six-rounds dispatch (branch feat/series-engine).

1. Read AGENTS.md, the origin plan (§3.3-§3.5, §3.7, §1 facts 4-6), the dispatch plan §1/§8.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-series-engine.
2. Functional focus: the spawn lives in the same pure plan (any async/IO/tick in the planner is a blocking finding); successor insertion uses the midpoint rule (naive head.order+1 into an occupied slot is the exact reviewed-against bug); reminder rules replicate but delivery/dismissal/snooze history does not (new occurrence keys re-arm by construction — pinned); Until/Count suppress and clear at the right transitions (Count against derived history, no counter column); the day_tick watermark survives restart correctly (boot catch-up rolls once, not per tick); a quiet day is a full no-op (no writes, no publishes — pinned); non-recur SetTaskDone behavior is byte-identical (existing suite untouched).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #499 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 8.2 Chunk DR-4/D — resurrect-as-new-card (#483, `feat/sync-resurrect-contention`)

- **Covers**: origin §3 D-3 (normative, a–e) + §6 chunk D: the pure
  `has_unsynced_work` predicate, the R3 branch split (tombstone as
  today vs resurrect-as-new-card: clear remote/remote_seen/baseline,
  cancel other pending ops, enqueue exactly one `CreateTask`),
  `SystemEvent::TaskResurrected(TaskId)` + GUI/CLI toast/status surfacing
  (no interactive prompt), the D-3d scope edges (stack-cascade stays
  tombstone; R5 untouched), engine snapshot scenes, the property that
  an unsynced-work task never reaches `deleted: true` through R3, ADR
  0004's dated amendment. No config hatch (D-3e ruled against).
- **Acceptance**: the four snapshot scenes (synced-delete disappears;
  unsynced-edit resurrects with re-planned ops + event; the
  resurrect-then-delete-again loop fires once per episode; cascade
  contention stays tombstoned) pin the policy; the predicate is pure
  and derived (no new persisted flag); kiosk/daemon never blocks on a
  dialog; gates green.

**Prompt D.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-4/D of the six-rounds dispatch ("resurrect contention", branch feat/sync-resurrect-contention).

Precondition: DR-4/L (feat/sync-orset-labels) has merged into origin/main (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-3 a-e is normative; §2 facts 5-7, 14; §4 F-130) and the dispatch plan §1/§8.2.
2. Read the seams: crates/taskboard-domain/src/pipeline.rs (the R3 absence-tombstone branch and its op cancellation), merge.rs (R5, the note that resurrection there is adoption-not-contention), messages.rs (SystemEvent), the GUI/CLI event surfacing points (ui-slint adapter, app status handling), and the existing S-series engine snapshot suite.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/sync-resurrect-contention ../taskboard-WT-feat-sync-resurrect-contention.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-resurrect-contention-plan.md: the predicate's exact clause list (field clocks vs remote_seen; the reminder-rules clause; how the echo maxes clocks so synced work compares <=); the R3 branch rewrite as pure re-planning (which ops cancel, which one CreateTask enqueues, what clears); the event + surfacing contract; the four snapshot scenes and the never-deleted property; the scope edges (cascade, R5) as explicit non-goals; the ADR 0004 amendment wording. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): resurrect-as-new-card on delete contention. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt D.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-4/D of the six-rounds dispatch ("resurrect contention").

1. Read AGENTS.md, the origin plan (§3 D-3, §2 facts 5-7/14), the dispatch plan §1/§8.2, and the stage-1 plan in ../taskboard-WT-feat-sync-resurrect-contention (branch feat/sync-resurrect-contention). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The pipeline stays the policy executor; the predicate is pure domain; the event is internal-additive on the broadcast lane; the surfacing is a toast/status line, never a blocking prompt (kiosk/daemon rule). SPDX/forbid rules hold; no sleeps; typed outcomes; no text assertions on the toast copy.
3. Run the full gate suite from AGENTS.md §10 and make it green (the new snapshot scenes are additions — existing dumps must not shift; this chunk does not widen the state shape).
4. ADR 0004's dated amendment rides the final commits. Do NOT touch backlog issues — stage 3 closes #130/#483. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt D.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-4/D of the six-rounds dispatch (branch feat/sync-resurrect-contention).

1. Read AGENTS.md, the origin plan (§3 D-3, §2 facts 5-7/14), the dispatch plan §1/§8.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-sync-resurrect-contention.
2. Functional focus: the synced case is byte-identical to today (the 99% path — a synced-delete still disappears, pinned); the resurrect branch re-plans exactly one CreateTask and cancels the OTHER pending ops (an orphaned UpdateTask riding along is a finding); remote/remote_seen/baseline all clear together (a stale baseline surviving a resurrect corrupts the next merge — cross-check against DR-4/G's inventory); no loop: the re-created card's second remote delete tombstones (its work is synced by then); stack-cascade contention stays tombstone-wins; R5 untouched; the event fires only on the resurrect branch and no interactive prompt exists anywhere in the path.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #130 and #483 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 8.3 Chunk DR-5/4 — tracking UI (#489, `feat/tracking-ui`)

- **Covers**: origin §3 Part E's D-T16–D-T17 (normative) + §5 UI
  hooks: the status-bar chip (title + minute-bucketed elapsed + Stop
  IconButton; **kiosk renders read-only** — the model-side gate
  empties the action, the passive display stays), task-card context
  menu arm (Start/Stop tracking) + the tracking badge on the open
  entry's card (**cards keep their no-per-row-buttons rule**), the
  task-detail Start/Stop button, the notification surface's **Start
  Task** button on task-anchored rows only (dispatches exactly one
  command — implicit dismissal lives planner-side; the
  adapter.rs:3300-3304 reservation resolves), `ActionId::TaskTrackStart`/
  `TaskTrackStop` only, pure presentation functions
  (`open_elapsed_phrase`, chip/badge visibility, kiosk gate), no
  UI-held timer state.
- **Acceptance**: pure-fn units (elapsed buckets, visibility, kiosk
  gate) structural-tested without a run loop; action routing asserts
  the exact `StateCommand` dispatched; Start Task's visibility is
  anchor-aware; golden visuals extend per the harness; gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-5/4 of the six-rounds dispatch ("tracking UI", branch feat/tracking-ui).

Precondition: DR-5/1 (#486) has merged (the verbs and published tracking state exist; storage "rides whatever exists at base" per the origin's own sequencing note). Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (§3 D-T16/D-T17 are normative; §1's UI anchors (push_notifications kiosk gate, the no-per-row-buttons rule, the wall-minute repaint, the adapter reservation); §5 the UI test hooks; §7 chunk 4) and the dispatch plan §1/§8.3.
2. Read the seams: crates/taskboard-ui-slint/src/{presentation.rs, models.rs (push_notifications' gate pattern), adapter.rs (the :3300-3304 reservation), actions.rs, router.rs, ui/compositions/status-bar.slint, ui/compositions/task-card.slint, ui/views/task-view.slint, ui/compositions/notification-surface.slint}.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/tracking-ui ../taskboard-WT-feat-tracking-ui.
4. Write a TDD-first plan at .artifacts/plans/<today>-tracking-ui-plan.md: the chip composition + kiosk gate; the context-menu arm + badge derivation; the detail button; the Start Task row rule (anchor-aware visibility, single-command dispatch); the two ActionIds + router arms; the pure presentation function catalogue; the explicit non-goals (totals section and detail history are fenced #490/#491). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): tracking UI affordances. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-5/4 of the six-rounds dispatch ("tracking UI").

1. Read AGENTS.md, the origin plan (§3 D-T16/D-T17, §1, §5), the dispatch plan §1/§8.3, and the stage-1 plan in ../taskboard-WT-feat-tracking-ui (branch feat/tracking-ui). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The UI stays a dumb client: elapsed is derived per frame from the ArcSwap snapshot; minute-bucketed text rides the wall-minute repaint (no repaint-driver change); no UI-held timer state; cards gain NO per-row buttons (the context menu is the affordance); the kiosk gate is model-side. Pure presentation functions unit-tested structurally; action routing asserts exact commands.
3. Run the full gate suite from AGENTS.md §10 and make it green (golden visuals extend per the harness).
4. Do NOT touch backlog issues — stage 3 closes #489. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-5/4 of the six-rounds dispatch (branch feat/tracking-ui).

1. Read AGENTS.md, the origin plan (§3 D-T16/D-T17, §1, §5), the dispatch plan §1/§8.3 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-tracking-ui.
2. Functional focus: the kiosk chip renders read-only (the gate empties the ACTION, not the display — a hidden chip on kiosk is a finding); Start Task dispatches exactly one command (a UI-side dismissal call alongside it duplicates planner machinery — blocking); Start Task never renders on Adhoc rows; cards carry no new buttons (context menu only); the two ActionIds are the entire action-layer growth; the adapter reservation comment is resolved (removed, not orphaned); no elapsed computation lives in slint code (grep .slint for arithmetic).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #489 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 9. Wave 6 — DR-3/2 · DR-4/R

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 9.1 Chunk DR-3/2 — insistence flavour fields (#475, `feat/reminder-insistence`, mig 0012, (w))

- **Covers**: origin §3 Part B (D-I2–D-I4, normative) + §5 hooks: the
  `Insistence { Normal, Alarm }` enum (**named Insistence — never
  Urgency**, the task-field collision), fields on `ReminderRule` /
  `AdhocAlarm` / `Notification` (fire-time snapshot), set-at-creation
  immutable, the implicit default reminder stays `Normal`, the
  evaluator stays insistence-agnostic (`due_firings`/
  `active_notifications` untouched; only `notifications_for` copies),
  migration **0012** (this dispatch's renumber; both columns, default
  `'normal'`) + codec roundtrips + upgrade test, the sidecar
  `RuleEntry` additive field with default-on-missing fold, CLI
  `--insistence` flags on `tasks remind` and `timers add` (+ list
  rendering, typed unknown-value errors). No dismissal/delivery row
  changes; no config knobs.
- **Acceptance**: serde roundtrips + unknown-value rejection typed;
  migration upgrade folds existing rows `normal`; sidecar fold
  default-on-missing and emission includes the field; mixed-fleet
  verbatim relay property extended; CLI receipts carry the level and
  unknown values are typed errors; the evaluator's signatures/logic
  are untouched (diff proves it); gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-3/2 of the six-rounds dispatch ("insistence fields", branch feat/reminder-insistence).

Precondition: DR-3/1 (fix/reminder-settlement-retime) has merged into origin/main (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md (§3 Part B is normative; §1 the urgency-collision anchor row; §5 the chunk-2 test hooks; §7 chunk 2) and the dispatch plan §1/§2/§9.1. NOTE: this dispatch reassigns the migration to 0012 (landing order) — verify it is the next free number at your base and record the check.
2. Read the seams: crates/taskboard-domain/src/reminders.rs (ReminderRule, AdhocAlarm, Notification, notifications_for), appdata.rs (RuleEntry and its additive-field precedent), storage migrations + codecs (reminder_rules, adhoc_alarms), app commands (reminders.rs, timers.rs, cli.rs, output.rs), and the DR-1 rules-section fixtures the sidecar extension builds on.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/reminder-insistence ../taskboard-WT-feat-reminder-insistence.
4. Write a TDD-first plan at .artifacts/plans/<today>-reminder-insistence-plan.md: the enum (kebab-case serde, Default, ordering decision pinned per D-I2's either/or); the three field placements and the snapshot copy; the immutability rule (no rule-edit verbs; re-create semantics unchanged); migration 0012 (both columns, NOT NULL DEFAULT 'normal') + upgrade test; the RuleEntry additive fold; the CLI surface (flags, receipts, typed unknown-value error per the KeymapPreset precedent); the eval-agnostic fence as an explicit non-goal; the test catalogue from origin §5. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): insistence flavour fields. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-3/2 of the six-rounds dispatch ("insistence fields").

1. Read AGENTS.md, the origin plan (§3 Part B, §5), the dispatch plan §1/§2/§9.1, and the stage-1 plan in ../taskboard-WT-feat-reminder-insistence (branch feat/reminder-insistence). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Migration 0012 is additive-only with upgrade test; the sidecar field folds default-on-missing (old documents relay verbatim — extend the mixed-fleet fixture); the evaluator (due_firings/active_notifications) must show NO logic diff; CLI follows the timers two-lane recipe; unknown --insistence values are typed errors, never stringly failures. SPDX/forbid rules hold; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green. Insta dumps regenerate (rules/state widen) — one deliberate pass.
4. Do NOT touch backlog issues — stage 3 closes #475. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-3/2 of the six-rounds dispatch (branch feat/reminder-insistence).

1. Read AGENTS.md, the origin plan (§3 Part B, §1, §5), the dispatch plan §1/§2/§9.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-reminder-insistence.
2. Functional focus: the type is Insistence (grep for any 'urgency' naming slip — the collision is the recorded misdirection bug); the evaluator's signatures and logic are untouched (only notifications_for copies the field — verify by diff); fields are set-at-creation and immutable (no edit path); the migration carries BOTH columns with 'normal' defaults and a meaningful upgrade test; the sidecar folds default-on-missing and a pre-round device's rule-less row folds Normal; dismissal/delivery rows are untouched; CLI unknown values are typed errors (no text assertions in tests).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #475 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 9.2 Chunk DR-4/R — duplicate-create crash-window reconcile (#484, `feat/sync-duplicate-reconcile`)

- **Covers**: origin §3 D-5 (normative, a–f) + §6 chunk R:
  `RemoteTask.created_at` (+ `RemoteStack` if the DTO carries it; else
  title+window fallback), the four-clause detection predicate (pull
  pass after the task-merge loop, before R3), keep-the-older
  survivorship with rebind + delete-push for the newer (404-tolerant),
  R1-adoption suppression, the stack variant, the boards-W2
  adopt-by-title sweep in the publish ceremony (F-150b confirmed),
  tracing-only honesty (rule tag R-dup), the wiremock crash journey,
  ADR 0007's dated note (+ ADR 0010's W2 sentence).
- **Acceptance**: the wiremock journey (create POST → simulated crash
  → restart → replay POST → duplicate injected → pull reconciles to
  one card, zero duplicate local rows) passes; each detection clause
  pinned separately including the near-miss negatives (same title
  different stack; outside window; content mismatch); the pass is a
  structural no-op without a pending/just-pushed create (property);
  gates green.

**Prompt R.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-4/R of the six-rounds dispatch ("duplicate reconcile", branch feat/sync-duplicate-reconcile).

Precondition: DR-4/D (feat/sync-resurrect-contention) has merged into origin/main (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-sync-merge-policy-design-plan.md (§3 D-5 a-f is normative; §2 facts 9, 12-13, 16; §4 F-150/F-150b; §5 the false-positive risk) and the dispatch plan §1/§9.2.
2. Read the seams: crates/taskboard-sync-nextcloud/src/{model.rs (created_at parsing), mapping.rs, actor.rs}, crates/taskboard-domain/src/{remote.rs (RemoteTask/RemoteStack), pipeline.rs (the pull pass ordering: after the task-merge loop, before R3), reconcile.rs (normalized_title, the neutralization shape), outbox.rs (queued_at)}, the boards publish ceremony in the app crate (commands/boards or connect path — the W2 sweep site), and the wiremock journey harness.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/sync-duplicate-reconcile ../taskboard-WT-feat-sync-duplicate-reconcile.
4. Write a TDD-first plan at .artifacts/plans/<today>-sync-duplicate-reconcile-plan.md: the created_at mapping (epoch sentinel rule); the four-clause predicate as a pure function with each clause's unit pin and near-miss negatives; the survivorship/rebind/delete mechanics and the R1 suppression; the stack variant (DTO verification step included); the W2 sweep design in the publish ceremony; the crash-journey script; the honesty-surface rule; the ADR 0007/0010 notes. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): duplicate-create crash-window reconcile. Do not implement; do not push.

Report: plan path, decisions (esp. the stack-DTO verification), risks, estimated diff size.
```

**Prompt R.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-4/R of the six-rounds dispatch ("duplicate reconcile").

1. Read AGENTS.md, the origin plan (§3 D-5, §2 facts 9/12-13/16), the dispatch plan §1/§9.2, and the stage-1 plan in ../taskboard-WT-feat-sync-duplicate-reconcile (branch feat/sync-duplicate-reconcile). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The sync actor stays a translator (the created_at mapping is its only change); detection/survivorship are pure domain; the delete push is the existing 404-tolerant path; honesty is tracing-only (typed facts, no SystemEvent, no UI churn). SPDX/forbid rules hold; the wiremock journey uses tokio time pause/advance where budgets apply; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. ADR 0007's dated note (+ ADR 0010's W2 sentence per F-150b) rides the final commits. Do NOT touch backlog issues — stage 3 closes #150/#484. No push, no PR.

Report: per-gate results, the stack-DTO verification outcome, any deviation and why.
```

**Prompt R.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-4/R of the six-rounds dispatch (branch feat/sync-duplicate-reconcile).

1. Read AGENTS.md, the origin plan (§3 D-5, §2 facts 9/12-13/16), the dispatch plan §1/§9.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-sync-duplicate-reconcile.
2. Functional focus: all four detection clauses are pinned independently AND their conjunction (a near-miss negative firing the pass is the false-positive class the origin flags — check same-title-different-stack, out-of-window, content-mismatch all reject); survivorship keeps the OLDER card regardless of arrival order (permutation-pinned); the R1 adoption of the duplicate is suppressed (no local duplicate row ever exists); the delete push for the newer card tolerates 404; the pass is a structural no-op without a pending/just-pushed create (property present); the W2 sweep binds the older board and deletes the newer only inside the ceremony window; no SystemEvent, no UI surface — tracing typed facts only.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #150 and #484 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 10. Wave 7 — DR-6/5 · DR-5/2 · DR-6/4

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 10.1 Chunk DR-6/5 — habit trackers: domain & engine (#502, `feat/habit-domain-engine`, (w))

- **Covers**: origin §4.1–§4.3 + §4.7 + §7 chunk 5 (normative):
  `HabitState` (habits + log maps, the third nested state home),
  `Habit` (board-scoped, `HabitTarget` A1 mints with NonZero counts,
  manual `order`, optional `anchor_time` **never a sort key**,
  per-field `HabitClocks`, `continued_from: Option<SeriesId>`), the
  command family (`CreateHabit`, `UpdateHabit` sparse double-Option,
  `MoveHabit` midpoints à la MoveTask, `LogHabit` monotone +1,
  `AdjustHabitLog` audited correction, `DeleteHabit` tombstone),
  window attribution (+24 h overflow, no knob) and streak functions
  parameterized by period kind, `RecurRule.continued_from:
  Option<HabitId>` (the conversion seam's other half), the proptest
  battery. Storage is chunk 6; engine tests on existing tiers.
  Serial after DR-6/2 (command-surface lane).
- **Acceptance**: planner tests on `FixedClock` (every verb, typed
  outcomes, NoOp/idempotence shapes); the log is keyed set-union with
  monotone counts under clocks (property); attribution partitions
  completions to exactly one period; streaks match a hand model on
  generated sequences; a single miss breaks the streak exactly after
  its grace-free window; anchor_time never influences order (pinned
  negatively); gates green.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/5 of the six-rounds dispatch ("habit domain & engine", branch feat/habit-domain-engine).

Precondition: DR-6/2 (#499) has merged (command-surface lane order). Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§4.1-§4.3 and §4.7 are normative; §6 rulings D-C8/D-C9/D-C10/D-C13; §3.9 properties 5-6 by analogy; §7 chunk 5) and the dispatch plan §1/§10.1.
2. Read the seams: crates/taskboard-domain/src/{state.rs (the nested-state pattern), reminders.rs (the (reminder, occurrence) fact shape the log applies), command.rs (planner catalogue style), messages.rs, persistence.rs (PersistedState mirror), entities.rs (where RecurRule gains continued_from)} and the MoveTask midpoint machinery.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/habit-domain-engine ../taskboard-WT-feat-habit-domain-engine.
4. Write a TDD-first plan at .artifacts/plans/<today>-habit-domain-engine-plan.md: the state home + entity with every A1 mint; the six commands' planner semantics (sparse changes, monotone log, tombstone); the period/attribution/streak functions' exact contracts (+24 h; ISO weeks for PerWeek; one parameterized function); the continued_from seams both directions; the properties (set-union monotonicity, attribution partition, streak hand-model, anchor_time-never-sorts); the engine coverage. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): habit tracker domain and engine. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/5 of the six-rounds dispatch ("habit domain & engine").

1. Read AGENTS.md, the origin plan (§4.1-§4.3, §4.7, §6 D-C8/C9/C10/C13), the dispatch plan §1/§10.1, and the stage-1 plan in ../taskboard-WT-feat-habit-domain-engine (branch feat/habit-domain-engine). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Pure domain + planner only: no storage diff (chunk 6's tables), no surfaces; never-done is native (no spawn/roll/done machinery — a habit has none of it); LogHabit is idempotent per invocation and monotone under clocks; AdjustHabitLog takes a fresh clock (audited edit); the streak/attribution functions are pure and total. Fixed clocks; no sleeps; proptests over generated sequences. SPDX/forbid rules hold.
3. Run the full gate suite from AGENTS.md §10 and make it green. Insta grows the habits section — one deliberate pass.
4. Do NOT touch backlog issues — stage 3 closes #502. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/5 of the six-rounds dispatch (branch feat/habit-domain-engine).

1. Read AGENTS.md, the origin plan (§4.1-§4.3, §4.7), the dispatch plan §1/§10.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-habit-domain-engine.
2. Functional focus: the log key is (habit, window-start) with set-union semantics (a count overwrite that can lose an increment is the LWW misapplication — counts are monotone under clocks, pinned); attribution is window-based with the +24 h overflow and NO knob (a grace parameter is a D-C8 violation); streaks are consecutive counting periods ending at the current one, parameterized once by period kind; anchor_time appears nowhere in ordering logic (grep the sort/comparison paths — a clock-sorted list is the D-C13 violation); MoveHabit mirrors MoveTask's midpoint contract; target mints reject zero; DeleteHabit tombstones (resurrection-safe), it does not purge the log.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #502 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 10.2 Chunk DR-5/2 — time entry storage & sidecar roaming (#487, `feat/tracking-storage-roaming`, mig 0013)

- **Covers**: origin §3 Parts C+D (D-T9–D-T13, normative) + §5 hooks:
  migration **0013** (`time_entries`, no FKs — entries outlive soft
  deletes), `PersistenceAction::UpsertTimeEntry`/`DeleteTimeEntry`
  (the exhaustive storage match forces the arms), codec +
  `decode_time_entry`, no intent table (the open row is the intent),
  the board-level `"time-entries"` sidecar section (entry-UUID keyed;
  `TimeEntryRecord { Entry, Tombstone, Verbatim }`; unbound-task
  entries never emitted), per-key clock-LWW union, **no
  `prune_absent_cards` arm** (board-level keys are invisible — the
  history-outlives-deletion ruling), derived tombstones with the
  truncated-view guard (within-window ∧ card-resolvable), 180-day
  document-side retention aging (`lane_gc_scheduled` shape; overlay
  filtered by the same window; local DB never drops), the fold
  (`fold_time_entries` after `fold_reminder_dismissals`), flush
  trigger via `UpdateAppData`, the stale outbox.rs doc-comment fix in
  passing. **Serial after DR-4/R** (the merge/pipeline lane).
- **Acceptance**: migration upgrade test (empty table folds); codec
  roundtrips incl. `end: null`; actions in one transaction; union
  convergence proptests (open→closed, tombstone races, verbatim);
  tombstone guard fail-closed (a truncated view never deletes);
  `lane_gc_old` strips and the overlay never re-emits; fold
  import/tombstone/skip paths pinned; mixed-fleet verbatim relay
  holds; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-5/2 of the six-rounds dispatch ("tracking storage & roaming", branch feat/tracking-storage-roaming).

Preconditions: DR-5/1 (#486) has merged, and DR-4/R (#484) has merged (the merge/pipeline lane is single-flight — this chunk follows it). Verify both (git log origin/main --oneline); if either is missing, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (§3 Parts C and D are normative; §1's sidecar-substrate anchors (the dismissals-wave 8-step recipe, prune_absent_cards card-keyedness, lane_gc_scheduled); §5 the storage/sidecar test hooks; §7 chunk 2) and the dispatch plan §1/§2/§10.2. NOTE: this dispatch assigns migration 0013 (landing order) — verify next free at your base and record the check.
2. Read the seams: crates/taskboard-storage-sqlite (migrations, repo.rs codec + the exhaustive PersistenceAction match), crates/taskboard-domain/src/{appdata.rs (AppDataSections, RuleEntry row-enum shape, union(), prune_absent_cards, lane_gc_scheduled, overlay builder, tombstone derivation + truncated-view guard), merge.rs (fold_reminder_rules / fold_reminder_dismissals as the pattern), pipeline.rs (the fold pass ordering), persistence.rs (PersistenceAction), outbox.rs (the stale doc comment at the UpdateAppData op)}.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/tracking-storage-roaming ../taskboard-WT-feat-tracking-storage-roaming.
4. Write a TDD-first plan at .artifacts/plans/<today>-tracking-storage-roaming-plan.md: the migration DDL; the codec + decode rules; the section struct + row enum with hand-written tolerant serde; the union function; the overlay builder + window filter + unbound skip; the tombstone derivation guard; lane GC; the fold function + its pipeline call site (after dismissals); the flush-trigger enqueues; the outbox comment fix; the proptest catalogue from origin §5. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): time entry storage and sidecar roaming. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-5/2 of the six-rounds dispatch ("tracking storage & roaming").

1. Read AGENTS.md, the origin plan (§3 Parts C/D, §1, §5), the dispatch plan §1/§2/§10.2, and the stage-1 plan in ../taskboard-WT-feat-tracking-storage-roaming (branch feat/tracking-storage-roaming). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The migration is additive plain SQL, no FKs, upgrade test; storage actions join the existing single-transaction batch contract; the sidecar work follows the twice-proven additive-section recipe (tolerant serde, verbatim relay, truncated-view guard fail-closed); prune_absent_cards gains NO arm; retention is document-side only — the local DB never drops a row; no `[tracking]` config (the 180-day constant is named in code). SPDX/forbid rules hold; no sleeps; typed outcomes.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch backlog issues — stage 3 closes #487. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-5/2 of the six-rounds dispatch (branch feat/tracking-storage-roaming).

1. Read AGENTS.md, the origin plan (§3 Parts C/D, §1, §5), the dispatch plan §1/§2/§10.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-tracking-storage-roaming.
2. Functional focus: prune_absent_cards has NO new arm (board-level keys must stay invisible — the history-outlives-deletion ruling; an arm here is a blocking finding); the tombstone guard is fail-closed (within-window AND card-resolvable — a truncated view that deletes is the exact bug the dismissals wave fixed); aged entries are never re-emitted (overlay filter + GC agree on the window); deletes roam but aging does not (asymmetric by design — verify tombstones derive for resolvable rows only); the fold applies per-entry LWW against local updated_at and skips unresolvable tasks; unknown sections/rows relay verbatim (mixed-fleet property extended); the storage match stays exhaustive with no wildcard.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #487 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 10.3 Chunk DR-6/4 — series surfaces (#501, `feat/series-surfaces`)

- **Covers**: origin §3.7's verbs + §7 chunk 4 (normative): the CLI
  `tasks repeat` family (`--daily/--weekly/--monthly` grammar,
  `--until/--count` bounds, `--off` to stop repeating) built on
  `UpdateTask` (anchor = the task's scheduled/duedate local date, else
  today — through the day-window convention), typed errors for
  invalid grammar; the today-view next-occurrence badge + the
  overdue-recurring display (pure presentation over `recur` +
  arithmetic; no UI-held state). Two-lane CLI recipe; kiosk reads the
  badge passively.
- **Acceptance**: hermetic CLI tests (typed receipts, typed grammar
  errors, two-lane routing); the anchor-derivation rule pinned
  (scheduled preferred, duedate fallback, today last); badge
  visibility/overdue derivation unit-tested structurally; gates
  green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/4 of the six-rounds dispatch ("series surfaces", branch feat/series-surfaces).

Precondition: DR-6/2 (#499) has merged (spawn/roll semantics exist to display). Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§3.7 and §4.6's CLI conventions, §7 chunk 4 — normative) and the dispatch plan §1/§10.3.
2. Read the seams: crates/taskboard-app/src/{cli.rs, commands/ (the two-lane recipe), output.rs}, crates/taskboard-ui-slint/src/{presentation.rs, ui/views/today-view.slint}, the domain recurrence arithmetic (next_after/occurs_on from chunk 1), and days.rs's window plumbing.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/series-surfaces ../taskboard-WT-feat-series-surfaces.
4. Write a TDD-first plan at .artifacts/plans/<today>-series-surfaces-plan.md: the clap grammar (subcommands/flags, the typed error taxonomy for bad patterns/bounds); the anchor-derivation rule and its tests; the badge/overdue presentation functions (pure; structural tests); the route_for_ipc rows; explicit non-goals (no pause/resume, no per-series settings, no auto-archive). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): series CLI and today-view badges. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/4 of the six-rounds dispatch ("series surfaces").

1. Read AGENTS.md, the origin plan (§3.7, §7 chunk 4), the dispatch plan §1/§10.3, and the stage-1 plan in ../taskboard-WT-feat-series-surfaces (branch feat/series-surfaces). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. CLI only parses/dispatches — repeat/off are UpdateTask carries (no new StateCommand); grammar validation is typed at the parse boundary; the badge derives from published state per frame (no UI-held recurrence state). Hermetic tests; typed outcomes; no text assertions.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch backlog issues — stage 3 closes #501. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/4 of the six-rounds dispatch (branch feat/series-surfaces).

1. Read AGENTS.md, the origin plan (§3.7, §7 chunk 4), the dispatch plan §1/§10.3 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-series-surfaces.
2. Functional focus: repeat/--off ride UpdateTask (a new StateCommand arm here duplicates the field-family surface — blocking); the anchor derivation matches the ruled fallback chain and goes through the day-window conversion (a raw-UTC date treat-as-local is the timezone bug class); grammar errors are typed (unknown pattern text is never a stringly match); the badge never fabricates dates (it renders next_after/roll outputs, derived per frame).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #501 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 11. Wave 8 — DR-6/3 · DR-5/3 · DR-3/4

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 11.1 Chunk DR-6/3 — series storage & sync: fold dedup (#500, `feat/series-storage-sync`, mig 0014)

- **Covers**: origin §3.6 + §4.4's recur columns + §7 chunk 3
  (normative): migration **0014** (`tasks.recur TEXT NULL` JSON +
  `ck_recur`), the appdata `tasks`-row recur facts (rule + clock ride
  the existing per-task entry — no new section), the fold-time dedup
  pass in `apply_sync_report` **after the task-merge loop** (DR-4
  D-5's shape with the typed `(series, scheduled-local-date)` key:
  trigger only on two live open tasks sharing the key; survivorship
  keep-older-created; merge field clocks; neutralize the newer as a
  ref-less tombstone; suppress duplicate adoption; tracing-only rule
  tag `R-recur-dup`), the wire round-trip journey. **Serial after
  DR-5/2** (the merge/pipeline lane). Habit tables are NOT this
  chunk (they land with chunk 6 as migration 0016 — dispatch
  ruling).
- **Acceptance**: the dedup predicate is a structural no-op without a
  twin (property, incl. the reopen-while-successor-lives
  false-positive analysis); survivorship is older-created regardless
  of arrival order (permutation-invariance property); the neutralized
  twin is ref-less and adoption-suppressed; recur facts round-trip
  appdata → decode losslessly, truncated views fold as tombstones;
  migration upgrade test; wiremock journey (adopt an occurrence card,
  learn the series); gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/3 of the six-rounds dispatch ("series storage & sync", branch feat/series-storage-sync).

Preconditions: DR-6/2 (#499) has merged, and DR-5/2 (#487) has merged (the merge/pipeline lane is single-flight). Verify both (git log origin/main --oneline); if either is missing, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§3.6 and §4.4's recur columns are normative; §1 facts 8-11 (appdata substrate, DR-4 D-5 precedent, sync lanes); §3.9 properties 7-8; §5.4's resource table as amended by the dispatch (migration 0014, not 0012); §7 chunk 3) and the dispatch plan §1/§2/§11.1.
2. Read the seams: crates/taskboard-storage-sqlite (migrations — verify 0014 next free; the tasks codec), crates/taskboard-domain/src/{appdata.rs (the tasks-section row shape + union + truncated-view guard), pipeline.rs (apply_sync_report's pass ordering after the task-merge loop), reconcile.rs (the ref-less tombstone neutralization shape), merge.rs (fold vocabulary)}, and the wiremock journey harness.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/series-storage-sync ../taskboard-WT-feat-series-storage-sync.
4. Write a TDD-first plan at .artifacts/plans/<today>-series-storage-sync-plan.md: the migration + codec; the tasks-row recur facts (serde shape, leniency at decode); the dedup pass as a pure predicate + survivorship function with its pipeline call site; the honesty surface (typed tracing, tag R-recur-dup); the property catalogue (no-twin no-op, permutation-invariant survivorship, round-trip/truncation); the wiremock journey script; the upgrade test. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): series storage and sync fold dedup. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/3 of the six-rounds dispatch ("series storage & sync").

1. Read AGENTS.md, the origin plan (§3.6, §4.4, §3.9 props 7-8), the dispatch plan §1/§2/§11.1, and the stage-1 plan in ../taskboard-WT-feat-series-storage-sync (branch feat/series-storage-sync). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The migration is additive plain SQL with upgrade test; the dedup is pure domain called from the pipeline pass (the engine executes the merge — no sync-actor logic); the honesty surface is tracing typed facts only (no SystemEvent, no UI churn); invalid stored/wire recur folds as typed failures, never poisoning. SPDX/forbid rules hold; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch backlog issues — stage 3 closes #500. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/3 of the six-rounds dispatch (branch feat/series-storage-sync).

1. Read AGENTS.md, the origin plan (§3.6, §4.4, §3.9), the dispatch plan §1/§2/§11.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-series-storage-sync.
2. Functional focus: the dedup trigger is structural (two LIVE OPEN tasks sharing series+date — anything looser, e.g. title heuristics or done tasks, is the false-positive class DR-4 D-5 avoided); survivorship keeps the older-created regardless of arrival order (permutation property present); the neutralized twin is a ref-less tombstone AND pending duplicate adoptions are suppressed (an un-suppressed adoption recreates the twin next cycle); the pass is provably a no-op without a twin; recur rides the EXISTING tasks appdata rows (a new section is a design deviation needing the recorded reason); adopt-an-occurrence-learn-the-series is pinned by the wiremock journey.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #500 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 11.2 Chunk DR-5/3 — track CLI family & export verb (#488, `feat/track-cli-export`)

- **Covers**: origin §3 Part E's D-T14–D-T15 (normative) + §5 CLI
  hooks: the `track start/stop/status/list/log/adjust/delete` family
  (timers two-lane recipe: one-shot wrappers + session `*_on` seams +
  one `collapse()` definition site), task-selector reuse, entry-id
  resolution (`resolve_alarm` pattern: exact uuid → unique ≥8-char
  prefix), `status`'s tracked-today total via `tracked_in_window` with
  the `[preferences] day_start_time` window, `1h 30m` human phrases +
  whole-seconds machine columns, `Track` → `IpcOrLocal`, the new
  top-level `export` verb (stdout, CSV default with the
  `entry_id, task_id, task_title, start, end, duration_seconds`
  columns, JSON option, inclusive start filters, raw payload — NOT
  the `--json` receipt envelope; daemon relays captured stdout
  verbatim), typed `--for`/`--from/--to` parse errors. Gates on
  DR-5/2 (dispatch ruling).
- **Acceptance**: `cli_track.rs` in the `cli_timers.rs` shape —
  hermetic tempdir boots, typed receipts, IPC relay leg, `--json`
  envelopes; export golden CSV/JSON against constructed fixtures
  (golden files are data, not labels); parse outcomes typed; no text
  assertions on error paths; gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-5/3 of the six-rounds dispatch ("track CLI & export", branch feat/track-cli-export).

Preconditions: DR-5/1 (#486) and DR-5/2 (#487) have merged (verbs + durable storage). Verify (git log origin/main --oneline); if either is missing, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-time-tracking-design-round-plan.md (§3 D-T14/D-T15 are normative; §1's CLI-recipe anchors (timers two-lane, route_for_ipc exhaustiveness, main.rs relay); §5 the CLI/app test hooks; §7 chunk 3) and the dispatch plan §1/§11.2.
2. Read the seams: crates/taskboard-app/src/{cli.rs (the verb enum + route_for_ipc exhaustive match), commands/timers.rs (the two-lane recipe to mirror), commands/mod.rs (dispatch arms), output.rs (typed receipts), main.rs (the daemon stdout relay), config.rs (the day_start_time parse seam)}, and the task-resolution + resolve_alarm machinery the selectors reuse.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/track-cli-export ../taskboard-WT-feat-track-cli-export.
4. Write a TDD-first plan at .artifacts/plans/<today>-track-cli-export-plan.md: the verb/flag surface exactly as D-T14 lists it; the collapse() site; receipt shapes; the export verb's lane + relay + envelope exception; the tracked-today projection call; the parse-error taxonomy; the test matrix (hermetic boots, relay leg, goldens). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): track CLI family and export verb. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-5/3 of the six-rounds dispatch ("track CLI & export").

1. Read AGENTS.md, the origin plan (§3 D-T14/D-T15, §1, §5), the dispatch plan §1/§11.2, and the stage-1 plan in ../taskboard-WT-feat-track-cli-export (branch feat/track-cli-export). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The CLI only parses/dispatches (SRP line): durations/phrases are pure functions; state is read back from the published snapshot, never echoed; export is raw stdout data (pipes are the contract — no file writing, no clipboard); `route_for_ipc` gains its rows without protocol change; mutating verbs route IpcOrLocal. Hermetic tempdir boots; typed outcomes; no text assertions.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch backlog issues — stage 3 closes #488. No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-5/3 of the six-rounds dispatch (branch feat/track-cli-export).

1. Read AGENTS.md, the origin plan (§3 D-T14/D-T15, §1, §5), the dispatch plan §1/§11.2 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-track-cli-export.
2. Functional focus: every verb has exactly one collapse() definition site (duplicated wrapper logic is the recipe's named hazard); status's tracked-today splits spanning entries at the day-window boundary via the pure projection (entries are never physically split); adjust/delete take explicit ids with prefix resolution and typed ambiguity; export's payload bypasses the receipt envelope but the daemon still relays it verbatim (pinned); --json envelopes carry typed receipts everywhere else; a one-shot local boot cannot silently orphan a tracking intent (lane routing pinned to IpcOrLocal).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #488 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```

### 11.3 Chunk DR-3/4 — escalation driver, alarm window, owned sound (#477, `feat/reminder-escalation`)

- **Covers**: origin §3 Part C's driver half (D-I5) + Part E
  (D-I9–D-I10, normative) + §5 driver hooks: the app-layer escalation
  driver task beside the notifier (wakes on its own cadence only while
  ≥1 un-dismissed `alarm` notification exists; acts via
  `NotificationSink::deliver` + rodio — **no new channel, no
  SystemEvent growth**), the always-on-top alarm window (state-reader,
  no focus steal, attention request via the windowing system's urgent
  hint; **rides this chunk per dispatch ruling**), the re-toast
  cadence (≥1 min awaiting → every 2 min, tracked-handle replacement),
  the rodio sound ramp (synthesized chime, no decode features,
  30%→100% over 5 min, degrade-never-fatal, Desktop mode only), the
  DesktopSink critical-urgency hint (payload data only), the pure
  `escalation_step(awaiting, now) -> Action` with an injected clock,
  `rodio` pinned once in `[workspace.dependencies]` (stack approval
  granted 2026-10-26). Kiosk: none of it (LogSink fence).
- **Acceptance**: the cadence/volume ladder is a pure function tested
  over constructed states (no sleeps); the fake sink records
  re-toasts and proves replacement-not-stacking; audio-init failure
  degrades to visual-only (typed outcome, not a panic); the window's
  open/closed predicate is the same pure derivation, tested as such;
  `cargo deny` gates the rodio tree; gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-3/4 of the six-rounds dispatch ("escalation", branch feat/reminder-escalation).

Precondition: DR-3/2 (#475) and DR-3/3 (#476) have merged into origin/main (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-reminders-surface-pair-design-round-plan.md (§3 Part C item 3 + Part E are normative; §5 the driver test hooks; §6 the rodio/Wayland risks; §7 chunk 4) and the dispatch plan §1/§11.3.
2. Read the seams: crates/taskboard-app/src/{bootstrap.rs (spawn site beside the notifier), notifications.rs (NotificationSink, DesktopSink tracked handles, resolve_sink)}, the published-state ArcSwap handle, the ui-slint window machinery for the always-on-top overlay, and root Cargo.toml [workspace.dependencies] for the rodio pin.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/reminder-escalation ../taskboard-WT-feat-reminder-escalation.
4. Write a TDD-first plan at .artifacts/plans/<today>-reminder-escalation-plan.md: the driver task's lifecycle (wake cadence, the derived awaiting predicate, shutdown); escalation_step's exact signature and ladder table (the named constants from D-I9); the rodio pin (verify the minimal no-decode feature set at planning; record MSRV + deny outcome plan); the chime synthesis approach (samples in code); the alarm window as a pure state-reader (open/closed predicate, re-raise behavior, the no-focus-steal attention request); the DesktopSink urgency hint (payload-only); the degradation paths (audio failure, missing compositor hints); the test design (fake sink, injected clock, constructed states). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): escalation driver, alarm window, owned sound. Do not implement; do not push.

Report: plan path, decisions (esp. the rodio feature verification), risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-3/4 of the six-rounds dispatch ("escalation").

1. Read AGENTS.md, the origin plan (§3 Parts C/E, §5, §6), the dispatch plan §1/§11.3, and the stage-1 plan in ../taskboard-WT-feat-reminder-escalation (branch feat/reminder-escalation). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The driver is app-layer process glue: no engine change, no new channel topology, no SystemEvent variant; it reads the ArcSwap and acts through the sink port + rodio. All audio lives behind the driver (port-shaped confinement); failure degrades to visual-only, logged, never fatal; sound only in Desktop mode with the desktop sink resolved; kiosk gets nothing. The cadence/volume constants live in the driver, named, not config. rodio pinned once in workspace dependencies, minimal features (no decode), re-verified per the house rule. No sleeps — the ladder is a pure function over an injected clock.
3. Run the full gate suite from AGENTS.md §10 and make it green — cargo deny explicitly gates the new rodio/cpal tree per target.
4. Do NOT touch backlog issues — stage 3 closes #477. No push, no PR.

Report: per-gate results, the deny outcome for the rodio tree, any deviation and why.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-3/4 of the six-rounds dispatch (branch feat/reminder-escalation).

1. Read AGENTS.md, the origin plan (§3 Parts C/E, §5, §6), the dispatch plan §1/§11.3 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-reminder-escalation.
2. Functional focus: the driver wakes ONLY while an un-dismissed alarm-level notification exists (a busy-loop or always-armed timer is a finding); re-toasts replace via the tracked-handle map (no toast stacking — pinned by the fake sink); the ladder matches D-I9 exactly (t0 fire, +1 min sound start, 2-min re-toast, 30-s sound repeat, 30%→100% over 5 min, capped); no focus steal anywhere — attention request + always-on-top + re-toast + sound are independent channels, each individually no-op-able; E-14 is untouched (grep messages.rs for diff — there must be none); audio failure is a typed degraded outcome, never a panic; kiosk/Daemon resolve_sink paths never reach audio or the window; the rodio pin is minimal-feature and workspace-pinned once.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #477 (and #203 — insistence levels' behavior is complete with this chunk) with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 12. Wave 9 — undo/1

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 12.1 Chunk undo/1 — journal substrate + inverse synthesis + RestoreTask (#509, `feat/undo-journal`, mig 0015, ADR 0019, (w))

- **Covers**: origin §5 chunk 1 (normative, item by item) + §4
  R-U6…R-U12 + §6 tests: domain `journal.rs` (`JournalId`,
  `JournalEntry`, `UndoFacts`/`PreImages`/`NotUndoable`, `PreImage`,
  `UndoWatermark`, `JournalState` with persisted head, `UndoView`);
  `PersistenceAction::RecordJournal`/`MarkJournalPushed` + apply arms
  (R-U11 eviction in the record arm); `pre_images` +
  `journal_push_marks` pure helpers; `plan_journaled_command` wrapper
  (`plan_command` untouched); the **exhaustive no-wildcard synthesis
  catalogue over the FULL command surface** — including DR-5's five
  tracking verbs and DR-6's habit family and the
  spawn/entry-closing `SetTaskDone` side effects (the §3 table plus
  the new arms; `StartTracking`/habit arms classified with recorded
  reasons; the SetTaskDone row accounts for the spawned successor per
  DR-6 §5.3 and the closed time entries per DR-5 D-T6); `RestoreTask`
  three-case planner; `Undo`/`Redo` + typed refusals; `AppState.undo:
  UndoView` (memory-only); storage migration **0015** (`command_journal`
  + head encoding) + codec + boot hydration + `MarkJournalPushed`
  idempotence test; engine switch to the wrapper + the R-U9 post-pass
  in both report ingestion paths before `repo.apply`; the one
  deliberate insta regeneration pass; ADR **0019** + the dated ADR
  0006 amendment.
- **Acceptance**: per-arm unit tests (pre-images restored / entity
  retracted, fresh clocks, `remote_seen`/`remote_baseline` never
  rewound; watermark refusal on any interleaved write; R-U6
  refuse-and-name; `CreateStack` occupied outcome); proptests
  (undo-all restoration, retraction empties the outbox, pushed class
  enqueues inverse ops, redo round-trips, watermark monotonicity);
  Undo/Redo join the insta corpus; the regeneration diff audited as
  "the widening, nothing else"; the catalogue fails to compile on a
  new arm (the #244 lockstep); upgrade path (old DB ⇒ empty journal);
  chunk independently green end-to-end through `EngineHandle::execute`;
  gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk undo/1 of the six-rounds dispatch ("undo journal", branch feat/undo-journal) in the taskboard repo at /home/viktoria/src/taskboard.

Preconditions: (a) the undo implementation design plan (.artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md) is ACCEPTED — check its Status line; if still READY FOR REVIEW, STOP and report. (b) DR-5/1 (#486), DR-6/2 (#499) and DR-6/5 (#502) have merged (the command-surface lane — the synthesis catalogue is written against the final StateCommand set). (c) No migration-carrying chunk is currently in flight (check open PRs / the wave plan). If any check fails, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-19-undo-architecture-design-plan.md (D-U1…D-U5 binding) and .artifacts/plans/2026-10-26-undo-v1-implementation-design-plan.md (§5 chunk 1 normative; §4 R-U6…R-U12; §3 the 33-arm table; §6 the test plan; §9 the risks), plus the dispatch plan §1/§2/§12.1. NOTE: this dispatch assigns migration 0015 and ADR 0019 (landing order superseded the plan's "expected 0017") — verify next free at your base and record the check.
2. Read the seams: crates/taskboard-domain/src/{journal.rs (new), command.rs, messages.rs (StateCommand AS IT STANDS post-DR-5/DR-6 — enumerate every arm), persistence.rs, state.rs}, crates/taskboard-state/src/{actor.rs (execute_command's planner call; both report-ingestion paths), core.rs}, crates/taskboard-storage-sqlite (migrations, codec, boot hydration), and the snapshot corpus (the 19 dump+tail pairs).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard (nonzero exit: STOP), then git worktree add -b feat/undo-journal ../taskboard-WT-feat-undo-journal.
4. Write a TDD-first plan at .artifacts/plans/<today>-undo-journal-plan.md: the complete classification table — EVERY current StateCommand arm (re-derive it; the origin's 33-row table is the seed, not the truth), each row's class/inverse/v1-undo answer with a recorded reason for the new arms (tracking verbs, habit verbs, the enriched SetTaskDone); the journal data shapes; the wrapper's contract (pure, same batch, pre-images from pre-state only); the watermark mechanics; the R-U9 post-pass placement in both ingestion paths; the migration + head encoding decision (R-U10 freedom); retention eviction; the test catalogue from origin §6. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): undo command journal and inverse synthesis. Do not implement; do not push.

Report: plan path, the classification table summary (esp. the new arms' rows), decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk undo/1 of the six-rounds dispatch ("undo journal").

1. Read AGENTS.md, the two origin plans (D-U1…D-U5; §5 chunk 1, §4 R-U6…R-U12, §6), the dispatch plan §1/§2/§12.1, and the stage-1 plan in ../taskboard-WT-feat-undo-journal (branch feat/undo-journal). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. plan_command's catalogue is NOT edited (the wrapper is the seam); the synthesis match is exhaustive with no wildcard (a new arm must break compilation); undo never rewinds observation machinery (remote_seen/remote_baseline monotone); the applied batch and the interpreted batch stay identical (single-advance rule — RecordJournal and MarkJournalPushed ride the command's/report's own batch); Undo/Redo ride EngineEnvelope::Execute (no new envelope/channel/actor); EngineSignal stays Copy-only. Fixed/stepping clocks; no sleeps; typed outcomes; matches! assertions.
3. Run the full gate suite from AGENTS.md §10 and make it green. The insta regeneration is ONE deliberate pass over the dump+tail pairs — diff-audited as the widening, nothing else.
4. ADR 0019 + the dated ADR 0006 amendment ride the final commits. Do NOT touch backlog issues — stage 3 closes #509. No push, no PR.

Report: per-gate results, the classification-vs-code crosscheck summary, any deviation and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk undo/1 of the six-rounds dispatch (branch feat/undo-journal).

1. Read AGENTS.md, the two origin plans (D-U1…D-U5; §5/§4/§6/§9), the dispatch plan §1/§2/§12.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-undo-journal.
2. Functional focus: the catalogue is exhaustive with no wildcard arm and covers EVERY current command (derive the arm list from messages.rs yourself — a missed new arm is the lockstep failure); the watermark refuses on stamp divergence and never force-applies (the silent-lossy-win class); pre-images come from the pre-state via the post-plan pass (any capture inside plan_command is a contract violation); MarkJournalPushed is idempotent and lands before repo.apply in BOTH ingestion paths (double-flip pinned); the head survives restart (pinned); RestoreTask's three cases match the spec (pending-delete retraction / pushed resurrect with fresh clocks + create op / local resurrect); the pushed-edit rule's two op behaviors are per-entry and correct; eviction removes leaves only and a pruned head reports NothingToUndo; the regeneration diff is exactly the widening (review every .snap hunk).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #509 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 13. Wave 10 — DR-6/6

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 13.1 Chunk DR-6/6 — habit trackers: storage, sync, surfaces (#503, `feat/habit-storage-surfaces`, mig 0016)

- **Covers**: origin §4.4 (habit tables), §4.5, §4.6 + §7 chunk 6
  (normative): migration **0016** (`habits` + `habit_log` tables per
  §4.4's DDL), the board-level `habits` and `habit-log` sidecar
  sections (`RuleEntry`-row pattern, embedded-clock union,
  tombstones, verbatim relay), **180-day wire retention** on
  `habit-log` (document-side GC mirroring DR-5's ruling; local truth
  keeps everything), zero Deck cards (trackers ride the sidecar
  only), the today-view Habits section (one progress row per tracker,
  +1 dispatching `LogHabit`, streak decoration, stored manual order),
  the CLI `taskboard habits add/list/log/adjust/rm` family (two-lane
  recipe). **Serial after DR-6/3** (merge/pipeline + storage lane)
  and DR-6/5.
- **Acceptance**: migration upgrade test; section union convergence +
  tombstone + verbatim properties (the rules/dismissals shape);
  retention GC strips the document and the overlay never re-emits;
  `prune_absent_cards` untouched (board-level keys invisible);
  today-view renders in stored order with +1 → LogHabit routing
  pinned; CLI family hermetic-tested with typed receipts; gates
  green.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk DR-6/6 of the six-rounds dispatch ("habit storage, sync, surfaces", branch feat/habit-storage-surfaces).

Preconditions: DR-6/5 (#502) and DR-6/3 (#500) have merged (domain first; the storage/fold lane is single-flight). Verify both (git log origin/main --oneline); if either is missing, STOP and report.

1. Read AGENTS.md, then .artifacts/plans/2026-10-26-recurring-habits-design-round-plan.md (§4.4-§4.6 are normative; §1 fact 8 (prune_absent_cards card-keyedness) and fact 14 (DR-5's retention ruling to mirror); §5.4's resource table as amended (migration 0016); §7 chunk 6) and the dispatch plan §1/§2/§13.1.
2. Read the seams: crates/taskboard-storage-sqlite (migrations — verify 0016 next free; codec), crates/taskboard-domain/src/{appdata.rs (the additive-section recipe + the DR-5 time-entries section as the freshest precedent), pipeline.rs (fold pass)}, crates/taskboard-app/src/{cli.rs, commands/}, crates/taskboard-ui-slint/src/{presentation.rs, ui/views/today-view.slint}.
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/habit-storage-surfaces ../taskboard-WT-feat-habit-storage-surfaces.
4. Write a TDD-first plan at .artifacts/plans/<today>-habit-storage-surfaces-plan.md: the migration DDL; the two sections' row enums + union + fold call sites; the habit-log retention GC (document-side, 180-day constant, overlay filter agreement); the today-view section (manual order, +1 routing, streak decoration, kiosk posture); the CLI family; the property catalogue (union/tombstone/verbatim/retention). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): habit storage, roaming and surfaces. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk DR-6/6 of the six-rounds dispatch ("habit storage, sync, surfaces").

1. Read AGENTS.md, the origin plan (§4.4-§4.6, §1 facts 8/14), the dispatch plan §1/§2/§13.1, and the stage-1 plan in ../taskboard-WT-feat-habit-storage-surfaces (branch feat/habit-storage-surfaces). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. Zero Deck cards for trackers (habits never appear in card push/pull — grep the push paths); the sections follow the additive recipe (tolerant serde, verbatim relay, tombstones compete by clock); retention is document-side wire-compaction only (the local DB never drops); prune_absent_cards gains no arm; the today section renders stored manual order (midpoints on drag) — anchor_time is a display chip only. SPDX/forbid rules hold; hermetic tests; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Do NOT touch backlog issues — stage 3 closes #503 (and #212, the round's origin gate, with a completion comment). No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk DR-6/6 of the six-rounds dispatch (branch feat/habit-storage-surfaces).

1. Read AGENTS.md, the origin plan (§4.4-§4.6, §1 facts 8/14), the dispatch plan §1/§2/§13.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-habit-storage-surfaces.
2. Functional focus: zero card traffic for habits (any LocalOp/Habit card path is the D-C9 cost regression — blocking); habit-log retention mirrors DR-5's asymmetry exactly (wire GC yes, local deletes no; tombstone-vs-live clock races pinned); board-level keys stay invisible to prune_absent_cards; union is per-key clock LWW with verbatim relay (old devices fold nothing, lose nothing); the today section orders by stored `order` (an anchor_time sort anywhere is the D-C13 violation); the CLI family has one collapse() site and typed receipts.
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #503 and #212 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 14. Wave 11 — undo/2

**Parallel set** — every chunk in this wave is order-independent of the others in it: dispatch greedily in the listed order, one worktree each, once every PR of the previous wave has merged and the chunk's own preconditions hold (each stage-1 prompt verifies them).

### 14.1 Chunk undo/2 — undo/redo surfaces (#510, `feat/undo-surfaces`)

- **Covers**: origin §5 chunk 2 (normative) + §6: CLI `tasks undo` /
  `tasks redo` (awaited `execute` like every verb — REPL and daemon
  socket inherit per ADR 0012; receipts print the entry label; typed
  refusals render as typed outcomes), the UI flip (`router.rs`'s
  `AppUndo | AppRedo` arm from `Disabled` to dispatch; the
  enabled-set exclusions + `menu.rs`; `UndoView` bound into the
  adapter's derived state; the toast labels — undo label, R-U6's
  refusal label, redo copy that never promises remote-identity
  preservation), CHANGELOG Unreleased entry. Keymap: nothing to add
  (all three presets already bind the strokes).
- **Acceptance**: router dispatch tests for the two enabled actions;
  `UndoView` binding in the adapter's derived-state tests; CLI verbs
  hermetic-tested (typed receipts, refusals structural); no text
  assertions; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk undo/2 of the six-rounds dispatch ("undo surfaces", branch feat/undo-surfaces).

Precondition: undo/1 (feat/undo-journal) has merged (the verbs and UndoView exist). Verify (git log origin/main --oneline); if not, STOP and report.

1. Read AGENTS.md, then the two undo origin plans (§5 chunk 2, §6; D-U5's risk (b) for the toast copy), and the dispatch plan §1/§14.1.
2. Read the seams: crates/taskboard-app/src/{cli.rs (TasksCmd), commands/ (an awaited-verb wrapper to mirror), output.rs}, crates/taskboard-ui-slint/src/{router.rs (the Disabled arm), actions.rs (the registry entries + enabled-set exclusions), menu.rs, adapter.rs (derived-state binding)}, and the keymap presets (verify nothing to add).
3. Run scripts/worktree-doctor.sh (nonzero exit: STOP), then git worktree add -b feat/undo-surfaces ../taskboard-WT-feat-undo-surfaces.
4. Write a TDD-first plan at .artifacts/plans/<today>-undo-surfaces-plan.md: the CLI verb shapes + receipts; the router/registry/menu flips (each site listed); the UndoView binding; the toast-copy rules (D-U5 risk (b): never promise identity preservation); the test matrix. Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): undo and redo surfaces. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk undo/2 of the six-rounds dispatch ("undo surfaces").

1. Read AGENTS.md, the two undo origin plans (§5 chunk 2, §6), the dispatch plan §1/§14.1, and the stage-1 plan in ../taskboard-WT-feat-undo-surfaces (branch feat/undo-surfaces). Recreate the worktree if missing.
2. Implement per the approved plan, TDD, every commit standalone. The CLI only parses/dispatches; the UI stays a dumb client (UndoView is read from the published snapshot; toasts render typed outcomes); refusals render structurally (tests assert variants, never label text). Hermetic tests; no sleeps.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. The CHANGELOG Unreleased entry rides the final commits. Do NOT touch backlog issues — stage 3 closes #510 and #176 (the umbrella closes with this chunk). No push, no PR.

Report: per-gate results, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk undo/2 of the six-rounds dispatch (branch feat/undo-surfaces).

1. Read AGENTS.md, the two undo origin plans (§5 chunk 2, §6), the dispatch plan §1/§14.1 (code-quality mandate applies), and the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-undo-surfaces.
2. Functional focus: the reserved rows are enabled and dispatched (no dead menu entries; the enabled-set exclusions updated consistently — grep each site from the stage-1 list); keymaps untouched (verify the presets still bind the strokes — a new binding is scope creep); redo copy never promises remote identity preservation; the CLI verbs inherit the daemon socket per ADR 0012 (route_for_ipc rows present); refusals are typed end-to-end (a stringly refusal path is a finding).
3. Code-quality mandate: per dispatch plan §1, verbatim.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: close #510 and #176 with the PR, then per dispatch plan §1 stage-3 protocol: push, PR, checks green, mergeStateStatus CLEAN, worktree teardown.

Report: verdict, findings, what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation. Never merge; never touch main.
```


## 15. Completion bookkeeping

- **Issue closures** (each chunk's stage 3 closes its own chunk issue
  with the PR): DR-2/R → #174, #192, #200, #469; DR-2/I → #196, #470;
  DR-2/D → #162, #471. DR-3/1 → #474; DR-3/2 → #475; DR-3/3 → #476,
  #352; DR-3/4 → #477, #203 (the origin pair's behavior is complete).
  DR-4/S → #480 (#133 gets its verification comment and **stays
  open** with the documented trigger); DR-4/G → #128, #481; DR-4/L →
  #129, #482; DR-4/D → #130, #483; DR-4/R → #150, #484. DR-5/1 →
  #486; DR-5/2 → #487; DR-5/3 → #488; DR-5/4 → #489, #210 (the
  subsystem carrier; the fences #490–#496 stay open). DR-6/1 → #498;
  DR-6/2 → #499; DR-6/3 → #500; DR-6/4 → #501; DR-6/5 → #502; DR-6/6
  → #503, #212 (the Wave-3 gate). undo/1 → #509; undo/2 → #510,
  #176 (umbrella).
- **Annotations**: DR-3/1 annotates #201 (age-pruning re-terms to
  dismissal rows). DR-3/2's issue body number is corrected 0010 →
  0012 at dispatch. DR-6/3's and DR-6/6's issue bodies correct their
  migration numbers (0014 / 0016) at dispatch.
- **Docs riding chunks** (never this plan): ADR 0017 (DR-5/1), ADR
  0018 (DR-6/2), ADR 0019 + ADR 0006 amendment (undo/1); ADR 0004
  amendments (DR-4 G/L/D), ADR 0007 + 0010 notes (DR-4/R); ADR 0011 +
  0014 amendments (DR-2/R), ADR 0012 amendment (DR-2/I), runbook +
  `contrib/` service + CHANGELOG (DR-2/D); CHANGELOG (undo chunks).
- **Deliberately not dispatched here** (nobody re-derives this
  boundary): DR-5 fences #490–#496; DR-6 follow-ups #504 (tracker
  reminders, F-1), #505 (Kani harnesses, F-3 — rides #426), #506
  (habit UX, F-2); the undo v2 family #353–#357 (reaffirmed deferred
  2026-10-26); DR-4's recorded-not-built items: boards/stacks/labels
  baseline scope (D-1e extension path), the Δ-band skew guard
  (D-4c — symptom-triggered only), user-visible conflict surfacing
  (D-1f — new backlog entry if wanted); DR-3's OS-delivery/visual
  plumbing queue (GUI review round 3; #444 native toast buttons
  fenced); recurring-series `paused_until` (DR-6 F-2).
- **What opens next**: once the last chunk of a round merges, its
  follow-on queue is live — DR-5's Wave-2 statistics work rides the
  landed `export` verb; DR-6's F-1 tracker reminders can design
  against the landed `ReminderRef` seams; the undo v2 family's first
  ruling (#353 branch-picker) can consider the real refusal-rate
  data undo/1's watermark produces. A wave-N+1 dispatch round (new
  plan, per AGENTS §7) chunks any of these when the user schedules
  them.
