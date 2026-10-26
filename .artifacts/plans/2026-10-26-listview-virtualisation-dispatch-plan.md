# [Plan] ListView virtualisation across the N-task surfaces (#373) — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-26
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the single implementation chunk of
  #373: Slint ListView virtualisation for every view that draws N
  tasks, per the accepted design round. No code in this commit.
- **Origin**:
  `.artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md`
  — §2 (feasibility: the two routes, the compiler contract, the
  runtime mechanics; route 2 — a custom in-house `ListView` component
  — is the chosen one), §3 (the four adjudicated trade-offs:
  focus-into-view seam, board-relative IdTables, scroll-survives-
  refresh by construction, the bench contract), §5 (the D9
  composition: `row_changed` in-place / off-viewport no-op / `reset`
  full-re-instantiation; persistent handles are load-bearing), §6
  (the analyse-positive recommendation and its internal order), §7
  (consequences) — plus
  `.artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md`
  §2-§3 (the per-view before-numbers and the guard/skip instrument)
  and §5 (the board i16 render ceiling, #462 — virtualisation is its
  structural fix). Issues: #373 (the ask), #461 (Today lists +
  anchor-on-removal), #462 (the crash), #460 (the instrument, done by
  the carrying PR). **The user's 2026-10-26 commissioning decision**
  widened the scope from "board columns" to "any view that draws N
  tasks" and dispatched the implementation (superseding the design
  round's "implementation gated on the kiosk-wave D250 verdict" — the
  D250 session will now measure the virtualised board, which is
  strictly better data for budget row 3).
- **Status**: **READY** (after the carrying PR — branch
  `fix/bench-view-pinned-lanes` — merges; the origin reports ride it).
  Deliberately **not** dispatched here: the detect-side refresh wins
  (#463 O(1) unchanged-guard, #464 mirror-based row diff — separate,
  independent of virtualisation), the refresh-lane regression (#390's
  side finding), upstream Slint per-row damage tracking (#390's other
  lever), and the on-device D250 measurement (kiosk wave).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | Design-round §2/§3/§5/§6; re-measure §2-§3/§5; #461; #462 | `feat/ui-listview-virtualisation` | One in-house `ListView` component; applied to the board columns (regular + compact legs), the Today section lists, and the stack-detail list; focus-into-view seam; bench board/10 000 un-skip as the success instrument. Non-goals: no palette/banner/notification lists (capped at 8/3 rows), no model/diff/IdTables semantic changes, no detect-side work (#463/#464), no upstream renderer work. | L |

**Dependency graph**: single chunk — no ordering constraints beyond
"dispatch after the carrying PR merges" (the origin reports are on
it). Dispatches as one agent session per prompt, in order (1.1 → 1.2
→ 1.3).

**Dispatch mechanics**: one agent session per prompt, in order;
the chunk works in `../taskboard-WT-feat-ui-listview-virtualisation`
next to the project root (AGENTS §8), branched off the then-current
`main`, no PR ever stacked. **Branch names are never reused after
their PR lands** (AGENTS §8, issue #415): a resume/second round works
on `feat/ui-listview-virtualisation-v2` (or re-branches off the
fetched `origin/main`), never on the landed branch — the doctor
treats any worktree whose `HEAD` is fully contained in `origin/main`
as disposable. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`).
Disk hygiene (AGENTS §8 worktree lifecycle): any agent creating or
recreating a worktree runs `scripts/worktree-doctor.sh` from the main
checkout first — nonzero exit (free disk below the floor) stops the
chunk; stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every stage):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-feat-ui-listview-virtualisation` on a new branch
   off the current `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-listview-virtualisation-plan.md` in the
   house `[Plan]` style, resolving every open question with a
   recorded decision, commits it (`docs(plan): …`). No
   implementation, no push. Origin sections marked normative are
   binding; deviations need a recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create
   `feat/ui-listview-virtualisation-v2` instead per AGENTS §8; never
   `git worktree add <path> main`), executes per the approved plan —
   TDD, every commit standalone (rebase-merge hygiene), full AGENTS
   §10 gate suite green. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase the branch onto `origin/main` (AGENTS §8's rebase dance:
   drop already-merged duplicate commits; resolve only real
   conflicts), push `--force-with-lease` if the rebase rewrote
   history — then reviews the entire branch diff versus the
   **post-rebase** base against the chunk's acceptance criteria +
   AGENTS rules **plus the code-quality mandate below**, re-runs
   whatever gates apply, implements fixes for every finding on the
   branch (amend/reword, never duplicate-subject fixups). Its last
   tasks are shipping: push the branch, open the PR to `main` with
   `gh`, watch `gh pr checks --watch` to completion — any failing
   check is a finding; diagnose, fix, push `--force-with-lease`,
   repeat until green — **then verify the PR's merge state**: `gh pr
   view --json mergeStateStatus` must report `CLEAN`;
   `BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, repeat; any other
   state ⇒ diagnose before handing off. **Then teardown**: remove the
   now-disposable worktree — from the project root, `git -C
   /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-feat-ui-listview-virtualisation` (absolute anchor
   per AGENTS §8 — never a bare relative path, which resolves against
   whatever cwd the session happens to be in and can name a
   neighbour's worktree; everything precious is pushed; a resuming
   round starts a fresh `-v2` branch). Confirm via `git worktree
   list` that the removed path was the intended one. The handoff
   contract is "PR open + checks green + merge state `CLEAN` +
   worktree removed" so the user's merge needs no additional rebase
   and the chunk leaves no 36-55G of build output behind. Reports a
   verdict. Never merges; never touches `main` (`main` is
   branch-protected — merges happen on GitHub by the user, after the
   required checks pass).

**Code-quality mandate** (stage 3, this chunk — implementation
chunk): beyond functional review, evaluate (a) code quality: naming,
comment discipline (comments state constraints, not narration),
idiomatic use of the approved stack, no dead code or speculative
generality, test quality per AGENTS §5; (b) the Single
Responsibility Principle: the crate-role boundaries of AGENTS §2 are
the SRP line — a boundary violation is an SRP violation; (c) a single
level of abstraction per function body: a function either
orchestrates named operations or works in primitive data, never
mixes. Findings are fixed on the branch like any other finding.

## 2. Chunk 1 — ListView virtualisation across the N-task surfaces (`feat/ui-listview-virtualisation`)

- **Covers** (normative): design-round report §2 (route 2: a custom
  in-house component named exactly `ListView` carrying
  `content-y`/`content-height`/`content-width`/`visible-height`/
  `visible-width` as length properties — the compiler's nominal
  contract, `i-slint-compiler-1.18.1/object_tree.rs:2891-2905`; the
  runtime mechanics of `i-slint-core-1.18.1/model/repeater.rs:263-446`
  `update_visible_instances`: viewport-only live instances, splice
  churn, avg-height content estimate, 1.5-screen random seek);
  §3a-c (focus-into-view seam; IdTables stay board-relative — model
  indices never become viewport-relative; scroll position survives
  refreshes by construction under D9's persistent handles); §5 (the
  D9 trichotomy — never swap a `ModelRc` per refresh); §6 step 3
  widened by the user's decision — **every surface that draws N
  tasks**: the board columns' regular leg
  (`ui/compositions/board-layout.slint:386-409`), the compact leg's
  single column (`board-layout.slint:603-610`), the Today view's four
  section lists (`ui/views/today-view.slint:54`), and the
  stack-detail task list (`ui/views/stack-view.slint`, the
  `detail_tasks` surface). Explicit non-goals: the palette (8-row
  cap), banner/notification surfaces (3-row cap), settings/rule lists
  (not N-task), any `models.rs` diff/push semantic change, any
  `IdTables` shape change, any crate-boundary change, the detect-side
  items (#463/#464), and renderer/upstream work. The bench contract
  (§3d + re-measure §2): once the board is virtualised, lift
  `BOARD_SAFE_ROWS_PER_COLUMN`'s skip for the board cells and measure
  `draw-full/board/10000` + `draw-partial/board/10000` — that
  before/after pair (eager: i16 panic, #462; virtualised: measured
  numbers) is #373 Ask 2's success instrument.
- **Acceptance** (stage 3 reviews against):
  1. M1 — the `ListView` component exists in `ui/widgets/`, engages
     the compiler's virtual layout (M1's probe observably
     distinguishes virtualised from eager instantiation — stage 1
     picks the observable; candidates: an instantiation-counting
     probe on the repetition-probe pattern, or render-geometry
     facts), and renders a small viewport over a large model
     correctly.
  2. M2 — the board's regular and compact legs render through it:
     goldens for both legs visual-identity-clean; the keyboard
     journeys (F8 double-click, F11 drag, the model-identity and
     drag journeys) green unmodified; a focus move to an
     off-viewport card scrolls it fully into view (pinned by test);
     drag arm/drop across virtualised columns works (seams carry
     global model indices; `IdTables::board_task` resolves the same
     card before and after).
  3. M3 — Today's section lists and the stack-detail list render
     through it: an overdue done-flip (a filtered-list removal,
     #461's exact scenario) keeps the scroll anchor row-anchored
     (pinned by test — the anchor is a row, not a pixel offset);
     compact-leg stack switching re-mounts the single column
     cleanly (r2 D8 semantics preserved).
  4. M4 — the bench's board cells at N = 10 000 are un-skipped and
     measured; the before/after pair is recorded (PR body + a short
     addendum section in the re-measure report or a small follow-up
     report, stage 3's choice); the skip's removal does not weaken
     the Today lanes or the visible-flip guard.
  5. Full AGENTS §10 gates green; no existing test weakened; the
     diff shape matches the Covers list (no `models.rs` semantics,
     no `IdTables` shape, no crate-boundary changes).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the #373 ListView
virtualisation dispatch ("N-task surfaces", branch
feat/ui-listview-virtualisation) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, then the normative origins by section: the design-round
   report .artifacts/reports/2026-10-26-listview-virtualisation-design-round-report.md
   (§2 routes + compiler contract + runtime mechanics — route 2, the custom
   in-house ListView component, is decided; §3 the four adjudicated
   trade-offs; §5 the D9 trichotomy; §6 the recommendation order), the
   re-measure report .artifacts/reports/2026-10-26-bench-view-pinned-remeasure-report.md
   (§2-3 the before-numbers and the guard instrument; §5 the i16 ceiling),
   and the dispatch plan .artifacts/plans/2026-10-26-listview-virtualisation-dispatch-plan.md
   §1-2. Issues #373, #461, #462 carry scope detail.
2. Read the seams you will extend: ui/compositions/board-layout.slint (the
   inner `for task` loops at ~386-409 regular and ~603-610 compact; the
   DragArea wrapper and its height binding; the DropArea/drop-zone
   composition), ui/views/today-view.slint (the section `for row` at ~54),
   ui/views/stack-view.slint (the detail task list), src/view_state.rs (the
   focus walk: step_focus/step_flat_domain/focus_domain — state-side,
   TaskId-based) and src/adapter.rs (where focus outcomes become refreshes —
   the focus-into-view seam lands beside them), src/models.rs (push_board /
   push_today / the persistent handles — read-only for you: no semantic
   changes), crates/taskboard-ui-slint/benches/refresh_draw.rs
   (BOARD_SAFE_ROWS_PER_COLUMN and the two skip sites you will lift at M4),
   and the tests you must not break: the goldens lane, the keyboard journeys
   (F8 double-click, F11 drag), the model-identity/drag journeys, the
   repetition-semantics probe, tests/bench_fixture_contract.rs. Also read
   the pinned slint sources the design round cites:
   i-slint-compiler-1.18.1/object_tree.rs:2891-2905 (the five-property
   nominal ListView contract) and passes/repeater_component.rs:84-99
   (delegate height = preferred-height, width = listview-width), plus
   widgets/common/listview.slint for the shape of the thing you are
   re-implementing in-house.
3. Run scripts/worktree-doctor.sh from the project root
   /home/viktoria/src/taskboard first (nonzero exit = free disk below the
   floor: STOP and report instead of planning), then create the worktree
   with git worktree add -b feat/ui-listview-virtualisation
   ../taskboard-WT-feat-ui-listview-virtualisation off main (never without
   -b and never on main — AGENTS §8 active-worktree safety).
4. Write a detailed TDD-first plan at
   .artifacts/plans/<today>-listview-virtualisation-plan.md covering the
   four milestones in order: M1 the in-house ListView component (exact
   property set and the element name the compiler contract requires; the
   chrome/scrollbar decision recorded against the theme ladder; the
   virtualisation-engagement observable — stage 1 picks it and names its
   failure mode: a silent non-virtualising ListView renders fine and
   forfeits every win, so M1's probe MUST be able to tell them apart); M2
   the board legs (regular + compact) with the DragArea delegate, the
   focus-into-view seam's API shape (stage 1 decides: per-column scroll
   property vs function; acceptance is "the focused card is fully visible
   after the move", computed through the id tables because card heights
   vary), and the journey/golden updates; M3 Today sections + stack-detail
   with the anchor-on-removal pins (#461: the anchor is a row, not a pixel
   offset); M4 the bench un-skip + measurement mechanics. Resolve every
   open question with a recorded decision; deviations from the origins
   need a recorded reason.
5. Commit it as: docs(plan): the ListView virtualisation chunk plan. Do
   not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the #373 ListView
virtualisation dispatch ("N-task surfaces").

1. Read AGENTS.md, the design-round report §2/§3/§5/§6, the re-measure
   report §2-§3/§5, the dispatch plan
   .artifacts/plans/2026-10-26-listview-virtualisation-dispatch-plan.md
   §1-2, and the stage-1 plan (.artifacts/plans/*listview*-plan.md) in the
   worktree ../taskboard-WT-feat-ui-listview-virtualisation (branch
   feat/ui-listview-virtualisation). Recreate the worktree on that branch
   if missing (if its PR already landed, work on
   feat/ui-listview-virtualisation-v2 per AGENTS §8 — never recreate on a
   landed branch).
2. Implement exactly per the approved plan, TDD, milestones M1→M4 in
   order, every commit standalone (rebase-merge hygiene). Chunk-specific
   hard rules: the ListView element must be NAMED ListView and carry all
   five length properties (content-y, content-height, content-width,
   visible-height, visible-width) or the compiler silently renders eager —
   M1's probe guards this, keep it green forever; delegate = the existing
   DragArea+TaskCard pair moved inside the ListView unchanged (height:
   drag-card.preferred-height survives — do not let the ListView default
   height binding fight it); callbacks and IdTables keep GLOBAL model
   indices — nothing viewport-relative may leak into any seam; never swap
   a ModelRc per refresh (D9's reset() would re-instantiate the viewport
   every pass and forfeit the win — the repetition probe and the
   model-identity journeys pin this); SPDX headers (AGPL-3.0-only for
   this crate) and #![forbid(unsafe_code)] on any new .rs file; no sleeps
   in tests, no text assertions, deterministic-by-construction (poll a
   predicate or inject a clock seam); scratch code never lands.
3. Run the full gate suite from AGENTS.md §10 and make it green (fmt,
   clippy --workspace --all-targets -D warnings, nextest, doc-tests,
   cargo deny, the CI-identical coverage sequence). M4's bench numbers
   (board/10000 with the skip lifted) go in the PR-ready block of your
   report — they are references, never gates.
4. Bookkeeping this chunk owns: annotate #461 (anchor-on-removal landed)
   and #462 (virtualisation is the structural fix; the board renders at
   N=10000 again) in your report text for stage 3 to post — do NOT edit
   issues yourself; leave #373 open (stage 3's PR closes it with the
   user's merge).
5. No push, no PR — shipping is stage 3's last task. Never push to or
   merge main.

Report: per-milestone summary, per-gate results, the M4 before/after
numbers, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the #373 ListView
virtualisation dispatch (branch feat/ui-listview-virtualisation).

1. Read AGENTS.md, the design-round report §2/§3/§5/§6, the re-measure
   report §2-§3/§5, the dispatch plan
   .artifacts/plans/2026-10-26-listview-virtualisation-dispatch-plan.md
   §1-2 (the code-quality mandate applies to you), and the stage-1 plan;
   then review the full branch diff (git diff main...HEAD) in
   ../taskboard-WT-feat-ui-listview-virtualisation.
2. Functional focus — this chunk's specific traps, named: (a) the silent
   non-virtualisation failure: verify M1's engagement probe exists, runs
   in CI, and would fail if the component lost its name or any of the
   five properties (rendering would still LOOK right — that is the trap);
   (b) index integrity: every callback seam (task-activated/clicked/
   context, board-card-drag-armed/dropped/drag-finished, today row seams,
   stack-detail seams) still carries global model indices, and
   IdTables::board_task resolves the same card before and after a
   scroll-and-back (spot-check the diff for any viewport-relative math);
   (c) D9 survival: no ModelRc is swapped on refresh anywhere in the
   diff; the repetition probe and model-identity journeys are unweakened;
   (d) anchor-on-removal: the Today removal test flips a filtered-list
   row and pins row-anchored (not pixel-anchored) scroll behavior — check
   the test fails if the anchor code is removed, not just that it passes;
   (e) focus-into-view: the acceptance is "the focused card is fully
   visible after the move" through id-table row heights, not an exact
   content-y pin; (f) the bench un-skip: BOARD_SAFE_ROWS_PER_COLUMN's
   skip is lifted only for the virtualised board, the visible-flip guard
   still runs, and the recorded board/10000 numbers are plausible against
   the eager before-picture (design-round report §4 table); (g) no
   existing test was weakened to fit the new widget (goldens updated,
   not deleted; journeys extended, not relaxed); (h) diff shape: no
   models.rs semantic change, no IdTables shape change, no crate
   boundary crossed.
3. Code-quality mandate (verbatim from §1): beyond functional review,
   evaluate (a) code quality: naming, comment discipline (comments state
   constraints, not narration), idiomatic use of the approved stack, no
   dead code or speculative generality, test quality per AGENTS §5; (b)
   the Single Responsibility Principle: the crate-role boundaries of
   AGENTS §2 are the SRP line — a boundary violation is an SRP violation;
   (c) a single level of abstraction per function body: a function either
   orchestrates named operations or works in primitive data, never mixes.
   Findings are fixed on the branch like any other finding.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a
   finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (body: summary,
   the M4 before/after numbers as a dev-references block, closes #373,
   annotates #461/#462 — post those annotations as issue comments — and
   relates #390/#460/#461/#462), then gh pr checks --watch; any failing
   check is a finding — diagnose, fix, push --force-with-lease, repeat
   until all checks are green. main is protected: never merge; the user
   merges on GitHub.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-feat-ui-listview-virtualisation (everything is pushed;
   a resuming round starts a fresh feat/ui-listview-virtualisation-v2
   branch per AGENTS §8 — never recreates on the landed branch). Confirm
   with git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR
URL, green-checks confirmation, worktree-removed confirmation. Never
merge; never touch main.
```

## 3. Completion bookkeeping

- **#373** closes via stage 3's PR (Ask 1's analysis landed in the
  design-round report; Ask 2's instrument = M4's board/10 000
  before/after pair). **#461** closes with M3 (its anchor-on-removal
  acceptance is the chunk's test). **#462** gets stage 3's annotation
  (virtualisation is the structural fix; the board renders at
  N = 10 000 again) — its disposition (close vs keep-open for an
  upstream report) is the user's at merge. **#460** was closed by the
  carrying PR.
- **Deliberately not dispatched here**: #463 (O(1) unchanged-refresh
  guard) and #464 (mirror-based row diff) — independent detect-side
  wins; the refresh-lane regression (#390's side finding — a profile
  round of its own); upstream Slint per-row damage tracking (#390's
  other lever); the on-device D250 measurement (kiosk wave — which
  now measures the virtualised board).
- **What opens next once this lands**: the kiosk wave's D250 session
  against the virtualised board (budget row 3's real verdict), and —
  if the M4 numbers still miss the ≤ 20 ms partial envelope at
  kiosk-class scales — the upstream damage-tracking conversation,
  armed with per-view numbers.
