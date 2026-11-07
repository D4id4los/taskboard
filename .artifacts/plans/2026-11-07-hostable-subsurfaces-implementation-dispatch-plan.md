# [Plan] Hostable Sub-Surfaces (W5b) — Implementation Dispatch Plan & Agent Prompts

- **Date**: 2026-11-07
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 3 chunks of the W5b recursive
  surface chain (views and flows as levels of one owned chain;
  innermost-first routing and settle; per-level state; the existing
  hand-rolled nestings ported onto it). No code in this commit.
- **Origin**: `.artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md`
  — §0 rulings Q1–Q7 and §3 D-H1…D-H8 **normative**; §2 (the N1–N4
  inventory) and §4 (fences) binding. The user's 2026-11-07 approval
  (Q5) and PR-shape decision (§1 below).
- **Status**: **READY** — chunk 1 dispatchable now off `main`;
  chunks 2 and 3 are each gated on the previous chunk's PR having
  **merged** (serial, never stacked). Not dispatched here: the
  embedded planner stage (Q1 = b; views-as-root makes it a small
  later round), multi-child trees (design §4).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | D-H1, D-H4, D-H5, D-H6, D-H7 (state side), Q6, Q7 | `refactor/ui-slint-surface-chain` | `Surfaces` chain with the view root replaces `ViewState.view` + `.flow` + `settings_return`/`planner_return`; innermost-first effect routing; the settle fold; scope-stack concatenation; `AlreadyInChain` refusal; the chain proptest. **Behaviour byte-identical.** No per-level buffers, no consumer ports | L |
| 2 | D-H2, D-H7 (focus anchor) | `refactor/ui-slint-level-state` | `LevelState`: the shared `ViewState.buffer` and the adapter-side per-flow text (`TrackFormState`, `walk_date_text`, `walk_answers`, `plan_*_text`, due buffers) move into their level; one drop-with-level lifetime replaces every per-flow wipe rule. **Behaviour byte-identical.** No consumer ports | M–L |
| 3 | D-H3, D-H8 steps 3–4, §2 N1–N3 | `refactor/ui-slint-nested-surface-ports` | The exit protocol (`on_child_exit`, default ignore); N2 (entries-form date picker), N3 (walk File-to / Pick-a-date — `WalkStage` loses `filing`/`picking_date`), N1 (entries list → form as parent → child, the sibling variant retires). N4 (shape-B handoff) untouched | M |

**PR shape — the user's question, answered.** The work *does*
decompose into separately viable PRs, and this plan recommends it:
chunks 1 and 2 are each behaviour-preserving refactors that leave
`main` green and coherent (chunk 1 already retires the two return
slots — value on its own), and chunk 3 is the consumer. Reasons over
one PR: (a) `view_state.rs` (~5.2k lines) and `adapter.rs` (~9k) are
the repo's hottest files — a long-lived branch spanning three
refactors accrues conflicts with every UI chunk that lands meanwhile;
(b) a single diff touching every flow's state, settle, routing, and
buffer is beyond what one stage-3 review can check reliably, whereas
"byte-identical behaviour, journeys untouched" is a sharp per-PR
acceptance bar; (c) a regression bisects to one layer. The cost of
splitting: chunk 1 lands an abstraction whose nesting is exercised
only by its proptest until chunk 3 — accepted, since chunk 3 is
scheduled, not speculative. **Serial, not stacked**: each chunk
branches off `main` after the previous PR merges, so no rebase
dance is ever needed. *If the user prefers one PR*, collapse to one
branch (`refactor/ui-slint-surface-chain`): three stage-1/stage-2
passes in sequence on it, one stage-3 at the end — the chunk
sections below stay valid as the commit grouping.

**Dependency graph**: 1 → 2 → 3, strictly serial, each gated on the
predecessor's PR merged to `main`. Default order 1, 2, 3.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch-flat>`
(slashes → dashes, AGENTS §8) branched off the then-current `main`,
no PR ever stacked. **Branch names are never reused after their PR
lands** (AGENTS §8, #415): a resume works on `<branch>-v2`. Create
worktrees with `git worktree add -b <branch> ../taskboard-WT-<branch-flat>`
(never `git worktree add <path> main`). Any agent creating or
recreating a worktree first runs `scripts/worktree-doctor.sh` from
the main checkout — nonzero exit stops the chunk; stage 3 removes the
worktree as its final handoff task. Heavy gate runs go through
`scripts/run-quality-gates.sh`.

**Shared execution model** (binding for every chunk):

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.
1. **Stage 1 — planning agent**: creates the worktree on a new
   branch off the current `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — `<branch>-v2` if the PR
   already landed), implements per the approved plan, TDD, every
   commit standalone (rebase-merge hygiene), full AGENTS §10 gate
   suite green. Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`** —
   `git fetch origin --prune`, rebase onto `origin/main` (drop
   already-merged duplicates; resolve only real conflicts), push
   `--force-with-lease` if history was rewritten — then reviews the
   entire branch diff versus the post-rebase base against the chunk's
   acceptance criteria + AGENTS rules **plus the mandate below**,
   re-runs the gates, fixes every finding on the branch (amend/reword,
   never duplicate-subject fixups). Ships: push, open the PR with `gh`,
   `gh pr checks --watch` until green (a failing check is a finding),
   then `gh pr view --json mergeStateStatus` must be `CLEAN`
   (`BEHIND`/`DIRTY` ⇒ rebase, push `--force-with-lease`, re-check).
   Teardown: `git -C /home/viktoria/src/taskboard worktree remove
   --force ../taskboard-WT-<branch-flat>`, confirmed via `git worktree
   list`. Handoff = "PR open + checks green + `CLEAN` + worktree
   removed". Never merges; never touches `main`.

**Code-quality mandate** (stage 3, every chunk):

- Beyond functional review, evaluate (a) code quality: naming,
  comment discipline (comments state constraints, not narration),
  idiomatic use of the approved stack, no dead code or speculative
  generality, test quality per AGENTS §5 (no text assertions, no
  quality ceilings, deterministic by construction, one behavior one
  test at the lowest layer); (b) the Single Responsibility Principle:
  the crate-role boundaries of AGENTS §2 are the SRP line — a
  boundary violation is an SRP violation; (c) a single level of
  abstraction per function body: a function either orchestrates
  named operations or works in primitive data, never mixes. Findings
  are fixed on the branch like any other finding.

## 2. Chunk 1 — the surface chain with the view root (`refactor/ui-slint-surface-chain`)

- **Covers**: design D-H1 (the chain, `Surface::{View, Flow}`, the
  truncate-only removal invariant, `chain[0]` always a view, the
  return slots retire), D-H4 (local settle = top level only; foreign
  settle = innermost-first fold, each level by its existing kind rule
  — `settle_flow`'s taxonomy is reused per level, not duplicated),
  D-H5 (innermost-first `Handled`/`Bubble` routing over the
  **unchanged** `resolve(FlowKind, EffectClass)` table), D-H6
  (`scope_stack` concatenates per-level contexts innermost-first,
  stopping at the first mask — the current modal branches become each
  kind's declared contexts), D-H7 state side (mount gating reads "kind
  present in chain"), Q6 (`AlreadyInChain` typed refusal at the one
  push site), Q7 (view as root). **Non-goals**: no `LevelState`, the
  shared `buffer` stays where it is, no consumer ports (no surface
  opens a child yet beyond what the flat model already does), no
  `.slint` layout change, no new user-visible behaviour, no change to
  `walk_resume` behaviour (whether the shape-B handoff is re-expressed
  as chain levels is a stage-1 decision; behaviour must stay
  identical either way).
- **Acceptance**: every existing journey, `flow_table_tests`, the
  keyscope ordering tests (`scope_stack_follows_the_ordering_specification`
  and the T-S1 port checklist) and the seam/mount inventories pass
  **without edits to their assertions** (moved call sites are fine;
  changed expectations are a finding). New: the chain `proptest`
  (random push/settle/effect sequences) asserting — chain[0] is always
  a view; a child never outlives its parent; a foreign settle empties
  the flow levels innermost-first; an effect a level `Handled` never
  reaches an outer level; settle is idempotent; a duplicate kind push
  is `AlreadyInChain`. Unit tests for the Settings/planner open-close
  exact-restore via truncation. `settings_return` and `planner_return`
  no longer exist. Gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the W5b hostable sub-surfaces dispatch ("the surface chain with the view root", branch refactor/ui-slint-surface-chain) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§0 rulings Q1–Q7 and §3 D-H1, D-H4, D-H5, D-H6, D-H7 are normative; §4 fences binding) and the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§2.
2. Read the seams you will replace: crates/taskboard-ui-slint/src/view_state.rs (the `Flow` enum ~:220-326, `ViewState` fields `view`/`flow`/`settings_return`/`planner_return` ~:369-470, `open_/close_settings` and `open_/close_plan_next_day` ~:620-655, `settle_flow` ~:2180, `reconcile`), crates/taskboard-ui-slint/src/adapter.rs (`flow_kind` ~:3295, `resolve_conflict`/`resolve` ~:3330-3430, the `(Flow, UiEffect)` dispatch table and its `(_, effect)` fallthrough ~:2930, `walk_resume` ~:146/2051/2855), crates/taskboard-ui-slint/src/keyscope.rs (`ScopeInputs`, `scope_stack`), crates/taskboard-ui-slint/src/router.rs (`EffectClass`). Tests you must not break and whose assertions you may not change: src/flow_table_tests.rs, the keyscope tests, every journey under crates/taskboard-ui-slint/tests/, tests/seam_inventory.rs and the view-mount inventory.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit: STOP and report), then git worktree add -b refactor/ui-slint-surface-chain ../taskboard-WT-refactor-ui-slint-surface-chain off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-ui-slint-surface-chain-plan.md settling: the exact `Surfaces`/`Level`/`Surface` types (house A1 — private fields, the push site as the one fallible minting constructor returning a thiserror `AlreadyInChain`); how a "kind" is identified for the once-per-chain rule (discriminant, not data); how the top-level XOR replace is expressed (truncate to depth 1, then push); how view navigation (replace vs. push-over: Settings and the planner push a view level; view.today / palette jump / nav.show-task truncate to root and swap — enumerate every current view transition and classify it); the routing shape (`Handled`/`Bubble` per level over the unchanged `resolve`) and how the existing kept arms map onto it with zero behaviour change; the settle fold and its local vs foreign trigger classification (enumerate every current settle trigger L1/L2/leaving/R3/Replace); how `scope_stack` derives each level's contexts (the five modal branches become declarations) while staying byte-equal for every legacy state; whether `walk_resume` becomes chain-expressed or stays (behaviour identical either way — record the decision); the accessor surface the adapter uses so the diff outside view_state stays small; the proptest strategy and its invariants (dispatch §2 Acceptance); the commit sequence, each commit building and green.
5. Commit it as: docs(plan): the surface chain with the view root. Do not implement; do not push.
6. Backlog issues: open/comment on `backlog:` issues autonomously for any out-of-scope finding (AGENTS §7); list the numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the W5b hostable sub-surfaces dispatch ("the surface chain with the view root").

1. Read AGENTS.md, .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§0, §3 D-H1/D-H4/D-H5/D-H6/D-H7 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§2, and the stage-1 plan (.artifacts/plans/*ui-slint-surface-chain-plan.md) in ../taskboard-WT-refactor-ui-slint-surface-chain (branch refactor/ui-slint-surface-chain). Recreate the worktree on that branch if missing (doctor first).
2. Implement exactly per the approved plan, TDD (the proptest and the restore-by-truncation unit tests first), every commit standalone and green. Hard rules: behaviour byte-identical — you may move call sites in existing tests but never change an existing assertion or expectation (if one seems to need changing, that is a behaviour change: stop and report it); proptest for the chain invariants; no text assertions; no sleeps; `Flow` variants unchanged; SPDX AGPL-3.0-only on any new .rs file; no new dependencies beyond the pinned workspace `proptest`.
3. Run the full gate suite from AGENTS.md §10 via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: if the plan records a docs touch (the keymap/flow description in the round-5 audit docs or docs/architecture.org's UI section describing view/flow ownership), make it; nothing else.
5. No push, no PR. Never push to or merge main.
6. Backlog issues: open/comment autonomously for out-of-scope findings (AGENTS §7); list the numbers.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the W5b hostable sub-surfaces dispatch (branch refactor/ui-slint-surface-chain).

1. Read AGENTS.md, .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§0, §3 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§2 (the code-quality mandate applies to you), and the stage-1 plan. Rebase onto origin/main first (dispatch §1 stage 3), then review git diff origin/main...HEAD in ../taskboard-WT-refactor-ui-slint-surface-chain.
2. Functional focus: (a) behaviour byte-identical — diff every existing test file: any changed assertion/expectation in flow_table_tests.rs, the keyscope tests, the journeys or the inventories is a finding unless it is a pure call-site move; (b) the truncate-only invariant: grep for any other way levels are removed or mutated in place; chain[0] can never be a flow; (c) every view transition classified (push-over vs truncate-and-swap) and the Settings/planner exact-restore preserved — settings_return/planner_return gone; (d) the settle fold reuses the per-kind taxonomy (no second copy of the kind rules) and local vs foreign triggers match the stage-1 enumeration; (e) the routing uses the unchanged resolve table — no (host, guest, effect) cells introduced; (f) scope_stack byte-equal for every legacy state (the T-S1 checklist); (g) AlreadyInChain is the one refusal and is typed, asserted by matches!, not text; (h) the proptest actually generates nested chains (check the strategy, not just that it passes) and has no upper-bound/quality-ceiling assertions; (i) walk_resume behaviour unchanged (journeys J10–J13, J20 green untouched).
3. Code-quality mandate: (a) naming, comment discipline (constraints, not narration), idiomatic stack, no dead code or speculative generality, test quality per AGENTS §5; (b) AGENTS §2 crate boundaries — all of this stays in taskboard-ui-slint, no IO; (c) one abstraction level per function body.
4. Re-run the full gate suite yourself (AGENTS §10, via scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open the PR to main with gh, gh pr checks --watch until green (failures are findings: fix, push --force-with-lease, repeat), then gh pr view --json mergeStateStatus must be CLEAN (BEHIND/DIRTY: rebase, push --force-with-lease, re-check). main is protected: never merge.
7. Backlog issues: open/comment autonomously for out-of-scope findings (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-slint-surface-chain; confirm with git worktree list.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — per-level state (`refactor/ui-slint-level-state`)

- **Covers**: design D-H2 (pure interaction state stays in the `Flow`
  variant; every adapter-side text/state slot moves into the owning
  level's `LevelState`, dropped with the level; `ViewState.buffer`
  becomes per-level; zero shared state beyond `AppState`), D-H7 focus
  side (the mount-time focus anchor stored in the parent's
  `LevelState`, restored on child exit — wired but only exercised by
  the existing flat flows here). **Non-goals**: no consumer ports, no
  exit protocol, no new behaviour.
- **Gate**: chunk 1's PR merged to `main`.
- **Acceptance**: `ViewState.buffer`, `TrackFormState`'s ad-hoc wipe
  rule, `walk_answers`, `walk_date_text`, `plan_time_text`,
  `plan_effort_text`, and the due-edit buffers no longer exist as
  free-standing fields — each lives in its level; every per-flow
  "wipe when the flow is not X" rule is deleted (the drop is the
  wipe). Existing tests' assertions unchanged (as chunk 1). New: the
  chunk-1 proptest extended — a level's state is unobservable after
  its level is truncated; unit tests per migrated slot that re-opening
  a flow starts from a clean state. Gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the W5b hostable sub-surfaces dispatch ("per-level state", branch refactor/ui-slint-level-state) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 1 (refactor/ui-slint-surface-chain) has merged — git fetch origin && git log origin/main shows the surface-chain commits and view_state.rs has no settings_return/planner_return. If not, STOP and report.

1. Read AGENTS.md, .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§3 D-H2 and D-H7 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§3, and chunk 1's landed plan (.artifacts/plans/*ui-slint-surface-chain-plan.md).
2. Inventory every state slot to migrate: grep crates/taskboard-ui-slint/src/view_state.rs for `buffer` (every read/write — the text flows' commit paths: commit_edit, commit_description, discard_edit, the refresh "never touch text while a flow holds the buffers" rule D9 rule 4) and crates/taskboard-ui-slint/src/adapter.rs for TrackFormState, walk_answers, walk_date_text, plan_time_text, plan_effort_text, the due-edit buffers/DuePickerState, and every site that wipes them. Tests you must not break (assertions unchanged): everything chunk 1 named.
3. Run scripts/worktree-doctor.sh first (nonzero: STOP), then git worktree add -b refactor/ui-slint-level-state ../taskboard-WT-refactor-ui-slint-level-state off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-ui-slint-level-state-plan.md settling: the `LevelState` shape (an enum keyed by surface kind vs. a struct of options — A1, illegal combinations unrepresentable where cheap); how the adapter-side slots reach their level without the pure view_state crate-module depending on slint types (the split between ViewState-held and adapter-held level state, and how both drop together under truncation); the per-slot migration table (old field → new home → deleted wipe sites); the Slint text-property push path (the D9 rule-4 "refresh never touches text while a flow holds the buffers" must hold per level); the focus-anchor slot; the commit sequence — one slot family per commit, each green.
5. Commit it as: docs(plan): per-level surface state. Do not implement; do not push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the W5b hostable sub-surfaces dispatch ("per-level state").

1. Read AGENTS.md, the design plan .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§3 D-H2/D-H7 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§3, and the stage-1 plan (.artifacts/plans/*ui-slint-level-state-plan.md) in ../taskboard-WT-refactor-ui-slint-level-state (branch refactor/ui-slint-level-state). Recreate the worktree if missing (doctor first).
2. Implement per the plan, TDD, one slot family per commit, every commit green. Hard rules: behaviour byte-identical — existing assertions unchanged (a needed change is a behaviour change: stop and report); every migrated slot's old wipe sites deleted, not left as dead belt-and-braces; no text assertions; no sleeps; SPDX AGPL-3.0-only on new files.
3. Full AGENTS §10 gate suite via scripts/run-quality-gates.sh, green.
4. No docs/bookkeeping beyond what the plan records.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the W5b hostable sub-surfaces dispatch (branch refactor/ui-slint-level-state).

1. Read AGENTS.md, the design plan (§3 D-H2/D-H7 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§3 (the code-quality mandate applies to you), and the stage-1 plan. Rebase onto origin/main first, then review git diff origin/main...HEAD in ../taskboard-WT-refactor-ui-slint-level-state.
2. Functional focus: (a) the migration table is complete — grep that ViewState.buffer, walk_answers, walk_date_text, plan_time_text, plan_effort_text and the free-standing TrackFormState/due buffers are gone, and no "wipe when flow is not X" site survives; (b) truncation drops both the view_state-held and the adapter-held halves of a level together (no half-dropped level reachable — check the proptest covers it); (c) the D9 rule-4 text-push discipline holds per level (no refresh overwrites a live level's text); (d) existing test assertions unchanged; (e) the pure view_state module gained no slint dependency; (f) commit shape: one slot family per commit, each building.
3. Code-quality mandate: (a) naming, comment discipline, idiomatic stack, no dead code or speculative generality, AGENTS §5 test quality; (b) AGENTS §2 boundaries; (c) one abstraction level per function body.
4. Re-run the full gate suite (AGENTS §10 via scripts/run-quality-gates.sh); failures are findings.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, gh PR to main, gh pr checks --watch until green, mergeStateStatus CLEAN (rebase + --force-with-lease if not). Never merge.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-slint-level-state; confirm with git worktree list.

Report: verdict, findings, fixes, PR URL, green + CLEAN confirmation, worktree removed.
```

## 4. Chunk 3 — the nested-surface ports (`refactor/ui-slint-nested-surface-ports`)

- **Covers**: design D-H3 (`GuestExit::{Completed(LevelResult),
  Cancelled}` delivered to the parent's `on_child_exit`, default
  ignore; payload only for parent-local continuation; subject death
  delivers nothing — Q4), D-H4's Escape-ladder generalization (a
  level's own rungs, then one `Cancelled` rung, never two levels per
  stroke), Q3 (a foreign settle of a text level follows the text
  rule), and the ports of design §2: **N2** the entries-form date
  picker becomes a child level of the form; **N3** the walk's File-to
  stack picker and Pick-a-date field become child levels of the walk —
  `WalkStage::{Inbox{filing}, CarryForward{picking_date}}` lose their
  sub-state fields; **N1** the entries list → form becomes parent →
  child, `Flow::TrackEntryForm` as a sibling-replacing variant
  retires into a child level. **Non-goals**: N4 (the shape-B planner
  handoff) untouched; no embedded planner; no new user-visible
  surfaces.
- **Gate**: chunk 2's PR merged to `main`.
- **Acceptance**: user-observable behaviour of the entries overlay,
  its form and picker, and the walk's Inbox/Carry-forward row
  affordances unchanged — their existing journeys pass; where a test
  asserted the *flattened representation* (e.g. matches on
  `WalkStage::Inbox { filing: Some(_) }` or `Flow::TrackEntryForm`),
  stage 1 lists each such assertion and its re-expression over the
  chain; any journey-level expectation change is a finding. The
  hand-written ladder rungs for these surfaces are deleted (the
  generic rule replaces them). New: exit-protocol unit tests
  (Completed payload reaches the parent; Cancelled; subject death =
  no exit), and a nested-chain case in the proptest drawn from the
  real ports (form → picker; walk → picker). Gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the W5b hostable sub-surfaces dispatch ("the nested-surface ports", branch refactor/ui-slint-nested-surface-ports) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunks 1 and 2 have merged — git fetch origin && git log origin/main shows the surface-chain and per-level-state commits. If not, STOP and report.

1. Read AGENTS.md, .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§0 Q3/Q4 and §3 D-H3/D-H4/D-H8 normative; §2 N1–N4 binding; §4 fences), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§4, and the landed plans of chunks 1–2 (.artifacts/plans/*ui-slint-surface-chain-plan.md, *ui-slint-level-state-plan.md). Also .artifacts/plans/2026-11-06-review-walk-plan.md D-W2/D-W3 (the walk's stage machine and Escape ladder — the behaviour to preserve) and the r4 entries-overlay design (grep .artifacts/plans for D-TS2/D-TS4).
2. Read the code to port: view_state.rs `WalkStage`, `review_walk_file`, the walk Escape ladder, `Flow::TrackEntries`/`TrackEntryForm` and their open/close paths; adapter.rs the walk's filing/picking-date arms (~:2141, ~:2205), the entries-form picker, the per-surface Escape rungs; the .slint mount sites for the form picker and the walk's row affordances. Journeys to keep green: the entries-overlay journeys, the review-walk journeys (J1–J20), the due-picker journeys.
3. Run scripts/worktree-doctor.sh first (nonzero: STOP), then git worktree add -b refactor/ui-slint-nested-surface-ports ../taskboard-WT-refactor-ui-slint-nested-surface-ports off main.
4. Write a TDD-first plan at .artifacts/plans/<today>-ui-slint-nested-surface-ports-plan.md settling: the `GuestExit`/`LevelResult` types (A1; payload variants only for parent-local continuation — File-to's StackId, Pick-a-date's date; the entry form needs none); `on_child_exit` placement and default; the order N2 → N3 → N1, one port per commit group, each green; per port: which Flow variants/fields are created or retire, which ladder rungs delete, which existing test assertions match on the flattened shape and their exact re-expression (journey-level expectations must not change); how the picker child reuses the existing picker kind (Q6: a picker inside the form inside the entries list is three distinct kinds — verify no kind repeats in any real chain, or record the AlreadyInChain consequence); focus-anchor restore on child exit for each port.
5. Commit it as: docs(plan): the nested-surface ports. Do not implement; do not push.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the W5b hostable sub-surfaces dispatch ("the nested-surface ports").

1. Read AGENTS.md, the design plan .artifacts/plans/2026-11-07-hostable-subsurfaces-design-round-plan.md (§0, §2, §3 D-H3/D-H4/D-H8 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§4, and the stage-1 plan (.artifacts/plans/*ui-slint-nested-surface-ports-plan.md) in ../taskboard-WT-refactor-ui-slint-nested-surface-ports (branch refactor/ui-slint-nested-surface-ports). Recreate the worktree if missing (doctor first).
2. Implement per the plan, TDD (exit-protocol tests first), ports in the order N2 → N3 → N1, every commit green. Hard rules: journey-level expectations unchanged; only the representation-level assertions the plan lists may be re-expressed, exactly as the plan states; the replaced ladder rungs and flattened sub-state fields are deleted, not left dormant; the shape-B handoff (walk_resume) untouched; no text assertions; no sleeps; SPDX AGPL-3.0-only on new files.
3. Full AGENTS §10 gate suite via scripts/run-quality-gates.sh, green.
4. Bookkeeping: CHANGELOG.md gets no entry (no user-visible change) unless the plan records otherwise; amend the design plan's status line to note the round is implemented, if the plan says so.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers.

Report: per-gate results, deviations and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the W5b hostable sub-surfaces dispatch (branch refactor/ui-slint-nested-surface-ports).

1. Read AGENTS.md, the design plan (§0, §2, §3 normative), the dispatch plan .artifacts/plans/2026-11-07-hostable-subsurfaces-implementation-dispatch-plan.md §1/§4 (the code-quality mandate applies to you), and the stage-1 plan. Rebase onto origin/main first, then review git diff origin/main...HEAD in ../taskboard-WT-refactor-ui-slint-nested-surface-ports.
2. Functional focus: (a) journey-level expectations unchanged — every re-expressed assertion is on the stage-1 list and re-expresses the same fact, not a weaker one (AGENTS §5: never weaken a correct test); (b) WalkStage has no filing/picking_date fields and Flow::TrackEntryForm no longer exists as a sibling variant; the per-surface ladder rungs are gone; (c) Escape over a child never closes two levels in one stroke; (d) Q3: a navigation away from the walk with a half-typed Pick-a-date / the form with typed text follows the text rule; (e) Q4: subject death drops the subtree with no exit delivered; (f) payloads exist only for parent-local continuation; (g) walk_resume / the planner handoff untouched (J10–J13, J20); (h) no kind repeats in any real chain (or AlreadyInChain handled as the plan recorded).
3. Code-quality mandate: (a) naming, comment discipline, idiomatic stack, no dead code or speculative generality, AGENTS §5 test quality; (b) AGENTS §2 boundaries; (c) one abstraction level per function body.
4. Re-run the full gate suite (AGENTS §10 via scripts/run-quality-gates.sh); failures are findings.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, gh PR to main, gh pr checks --watch until green, mergeStateStatus CLEAN (rebase + --force-with-lease if not). Never merge.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-slint-nested-surface-ports; confirm with git worktree list.

Report: verdict, findings, fixes, PR URL, green + CLEAN confirmation, worktree removed.
```

## 5. Completion bookkeeping

- **Docs**: chunk 1 updates whatever doc describes view/flow
  ownership (the round-5 keymap/flow audit description and, if
  present, `docs/architecture.org`'s UI section); chunk 3 marks the
  design plan's status "implemented".
- **Tracking**: issue #704
  is closed by chunk 3's PR.
- **Deliberately not dispatched here**: the embedded planner stage
  (Q1 = b; now a small round since views are chain levels); multiple
  live children per level (design §4); N4's re-expression beyond
  what chunk 1 decides.
- **What opens next**: nothing is gated on this round; the embedded
  planner becomes a candidate whenever the walk is next revisited.
