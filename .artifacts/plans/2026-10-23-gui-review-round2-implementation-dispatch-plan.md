# [Plan] GUI Review Round 2 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-23
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **8** implementation chunks of the
  GUI review round 2 design round (board surface & stack width
  semantics, interaction repairs, action registry round-up & object
  context menus, task-view/text-field ergonomics, settings form
  rework, connection management rework, visual identity,
  drag-to-move). No code in this commit.
- **Origin**:
  `.artifacts/plans/2026-10-23-gui-review-round2-plan.md` — §1 facts
  (F-A1…F-E8, anchored; do not re-derive), §2–§9 decisions (D1–D38,
  normative), §10 testing strategy, §11 code inventory, §12 slicing,
  §13 backlog interactions, §14 risks. Round-1 design round
  (`.artifacts/plans/2026-10-22-gui-review-round1-plan.md`) remains
  normative where round 2 does not amend it (R1-D-prefix citations).
  Plus the user's 2026-10-23 review findings (design §1.0, items
  1–32) and the user's instruction to fold in the remediation-wave-1
  leftovers #226/#227/#229/#232.
- **Status**: READY — pending the user's acceptance of the design
  round (same branch as this file). Chunk 1 is dispatchable
  immediately after acceptance+merge of these artefacts; every later
  chunk is gated on its predecessor's PR being **merged into `main`**
  (§1 dependency graph). Deliberately **not** dispatched here: quick
  capture (#347), undo/redo (#356/#353), free-form toml editing
  (#351), board scrolling / ListView virtualisation (#373),
  board-selector dropdown scalability (#411 — D36 is the visual tab
  restyle only), window-geometry persistence across restarts (#412 —
  D15 completes within-session stability only).

## 1. Chunks & ordering

| Chunk | Design slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §2 (D1–D8) | `feat/ui-r2-board-surface` | stack widths become stretch floors + 160-px floor + load re-clamp; fixed header height; regular-mode selection paint; square plus glyph; add-stack tooltip; compact-mode hiding; per-column Add Task ghost — `taskboard-ui-slint` only | L |
| 2 | §3 (D9–D15) | `fix/ui-r2-interaction-repairs` | incremental model push per #390's fix direction (stable identity → double-click + drag); submenu pointer sync + compact index space; tooltip lifecycle + hint-row width; window-stability probe — `taskboard-ui-slint` only | L |
| 3 | §4 (D16–D21) | `feat/ui-r2-actions-context-menus` | `task.move.cancel`, `stack.next/prev`, `board.create/delete`, `stack.delete`, `task.reopen` (#229), `nav.first/last-item` (#232), emacs `stack.create` gap, task/stack/board context menus with rendered hints — `taskboard-ui-slint` only | L |
| 4 | §5 (D22–D25) | `feat/ui-r2-taskview-textfields` | TextField preferred-width fix + checkmark commit; reminder presets row + inline custom offset; due-date editor with calendar picker + typed entry (#227) — `taskboard-ui-slint` only | L |
| 5 | §6 (D26–D31) | `feat/ui-r2-settings-form` | SegmentedControl + keymap echo; scale slider + field; settings Flickable; status line w/ mapped text (reverses R1-R5); InfoTip help — `taskboard-ui-slint` only | L |
| 6 | §7 (D32–D34) | `feat/ui-r2-connection-management` | board-scoped connection surface; Attach/Detach UiRequests (verify→attach→BindBoard / detach→UnbindBoard); busy/spinner + typed status text + info logs — `taskboard-ui-slint` + `taskboard-app` | L |
| 7 | §8 (D35–D37) | `feat/ui-r2-visual-identity` | stack card chrome + token ladder; board tab strip; golden re-baseline commit — `taskboard-ui-slint` only | M |
| 8 | §9 (D38) | `feat/ui-r2-drag-to-move` | press-hold-drag ghost, column hit-testing, MoveTask dispatch, cancel paths (#226) — `taskboard-ui-slint` only | M |

**Dependency graph**: strictly serial 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8
by default (shared seams: `adapter.rs`, `models.rs`, `theme.slint`,
`board-layout.slint`, `settings-view.slint`). The design round permits
parallel lanes **only on the user's explicit direction** (design §12):
α = 1→2→7→8, β = 3 (after 2), γ = 4→5→6 (after 2); a parallel
schedule inherits the AGENTS §8 rebase-dance obligations for every
descendant branch. Each chunk's stage 1 has a **precondition gate**:
the previous chunk's PR is merged into `main` (verify with
`git fetch origin --prune && git log origin/main --oneline -5`); if
not merged, STOP and report instead of planning. Chunk 7 additionally
verifies chunks 1 and 4 are merged; chunk 8 verifies chunk 7.

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); never two agents share a worktree. Every
chunk works in `../taskboard-WT-<branch-with-slashes-as-dashes>` next
to the project root (AGENTS §8), branched off the then-current
`main`. Disk hygiene (AGENTS §8 worktree lifecycle): any agent
creating or recreating a worktree runs `scripts/worktree-doctor.sh`
from the main checkout first — nonzero exit (free disk below the
floor) stops the chunk; stage 3 removes the worktree as its final
handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. The
   design round's cited sections are normative input, not
   suggestions — deviations need a recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), implements per the
   approved plan, every commit standalone (rebase-merge hygiene),
   runs the full gate suite (AGENTS §10: fmt, clippy `-D warnings`,
   nextest, doc-tests, deny, llvm-cov) and makes it green. Updates
   `CHANGELOG.md`'s Unreleased section as part of the feature
   commits. Does **not** push or open a PR. Reports gate results,
   the Tier-4 manual checklist (chunk 2, 5, 6, 8), and deviations.
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
   repeat until green — **then verify the PR's merge state**:
   `gh pr view --json mergeStateStatus` must report `CLEAN`;
   `BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, repeat; any other
   state ⇒ diagnose before handing off. **Then teardown**: remove
   the now-disposable worktree — from the project root,
   `git worktree remove --force ../taskboard-WT-<branch>`
   (everything precious is pushed; a later round that resumes the
   chunk recreates it on the branch). The handoff contract is "PR
   open + checks green + merge state `CLEAN` + worktree removed" so
   the user's merge needs no additional rebase and the chunk leaves
   no 36–55G of build output behind. Reports a verdict. Never
   merges; never touches `main`. (`main` is branch-protected —
   merges happen on GitHub by the user, after the required checks
   pass.)

**Code-quality mandate** (stage 3, every chunk — user-directed):
beyond functional review, the stage-3 agent evaluates:

- **Code quality**: naming, comment discipline (comments state
  constraints, not narration), idiomatic use of the approved stack,
  no dead code or speculative generality, test quality per AGENTS §5.
- **Single Responsibility Principle**: every module, actor, function,
  and type has exactly one reason to change; domain stays pure (no
  IO, no runtime), the engine orchestrates without legislating, the
  connector actor only syncs, the CLI only parses/dispatches — the
  crate-role boundaries of AGENTS §2 are the SRP line, and a
  violation of the boundary is a violation of SRP.
- **Single level of abstraction per function body**: every function
  body works at one level — a function either orchestrates
  (calls named operations) or implements (works in primitive data),
  never mixes; guard clauses and the happy path at the top level,
  detail pushed into named private functions. A body that interleaves
  channel-half plumbing with actor spawning and config surgery is a
  finding.
- **No text assertions** (round-1 F19/R5 test rule, still binding):
  assertions on UI label/description/tooltip/status strings —
  including substrings and disjunctions — are findings; assert
  structure, visibility, enablement, typed outcomes, or mapping
  totality instead.

## 2. Chunk 1 — Board surface & stack width semantics (`feat/ui-r2-board-surface`)

- **Covers**: design §2 (D1–D8), facts §1.1 (F-A1–F-A11), risks R2.
  Files: `board-layout.slint`, `models.rs`, `widths.rs`,
  `view_state.rs` (load/reclamp touch points), `adapter.rs` (mount
  path only), `theme.slint` (width-floor token if staged here).
  Non-goals: no visual-identity paint (chunk 7 owns card chrome and
  tabs — keep surfaces transparent), no drag changes (chunk 2 owns
  the gesture), no `UiRequest`/app changes, no settings files.
- **Acceptance**: (a) a board with stored widths fills the window
  (every column stretch 1, stored value as floor) — journey +
  model-level pin; (b) widths below 160 logical px re-clamp at load —
  proptest extends `widths.rs`; (c) header height identical for
  0-task and >0-task stacks — geometry pin via the testing backend
  (golden-adjacent) or model-level proof + Tier-4 item; (d) selected
  stack paints in regular mode (surface-raised + focus ring), empty
  or not — journey via `select_stack`; (e) plus glyph geometry
  square — structural pin; (f) add-stack tooltip wiring present with
  `stack.create` hint (empty hint renders fine); (g) compact mode
  mounts exactly one add-stack entry; (h) per-column Add Task ghost
  routes `select_stack` + create flow, click ≡ key journey; (i)
  `never_resized_board_pushes_auto_widths_and_no_requests` still
  passes unchanged; (j) goldens: this chunk must NOT need golden
  regen (no paint changes) — if it does, that is a finding (scope
  leak into chunk 7).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of GUI review round 2 ("board surface & stack width semantics", branch feat/ui-r2-board-surface) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §2 (D1–D8) and §1.1 (F-A1–F-A11) — normative — plus §10/§11/§14-R2, and the dispatch plan .artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md §1/§2.
2. Read the current seams you will extend: ui/compositions/board-layout.slint (sentinel width arms 115-128, header 136-214, add-col 238-273, compact switcher 295-349), src/models.rs (stack_width_px 204-221, push_board 258-267), src/widths.rs (floor 13, ceiling 43-46), src/view_state.rs (select_stack 510, load path), src/adapter.rs (mount/seeds 1195-1212), ui/widgets/icon-button.slint (the square-lock pattern 45-47, tooltip trigger 196-202). Tests you must not break: tests/journeys.rs 1169-1380, tests/golden_visuals.rs, src/models.rs 545, src/view_state.rs 1737-1840.
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit = STOP and report), then create the worktree ../taskboard-WT-feat-ui-r2-board-surface on a new branch feat/ui-r2-board-surface off main.
4. Write a detailed TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-board-surface-plan.md settling, with recorded decisions: the exact stretch/distribution mechanism for D1 (probe the pinned Slint 1.18: does equal horizontal-stretch distribute surplus equally across min-width-ed columns? record the observed rule; fallback = proportional stretch weights); where the load-time re-clamp of D2 hooks (ViewState seed vs push time) so persisted configs heal; the header fixed-height constant (D3) and which element absorbs the surplus; the selected-paint arm (D4) and its interplay with move-target; the icon-box square lock (D5); the tooltip trigger adoption (D6) incl. hint source; the compact-mount condition (D7); the Add Task ghost anatomy and its exact dispatch path (D8: select_stack then the existing create flow — name the functions); the test catalogue (journeys, model pins, proptest, Tier-4 items: header-height feel, ghost-row feel, fill-the-window feel on a wide monitor). No paint/token changes (chunk 7's).
5. Commit it as: docs(plan): board surface & width semantics for review round 2. Do not implement; do not push.

Report: plan path, decisions (esp. the stretch probe result), risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of GUI review round 2 ("board surface & stack width semantics").

1. Read AGENTS.md, the design round §2/§1.1/§10, the dispatch plan §1/§2, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-board-surface-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-board-surface (branch feat/ui-r2-board-surface). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD where code is involved, every commit standalone (rebase-merge hygiene). Hard rules: SPDX headers on new files, #![forbid(unsafe_code)] (ui crate excepted from the forbid by licensing tier — still no unsafe), no sleeps in tests (mock clock), no text assertions (labels/hints are asserted structurally), the existing journeys must pass UNCHANGED (if one genuinely conflicts with D1/D3, treat it as design §14-R2 evidence: stop, record, resolve in the plan before editing the test).
3. Run the full gate suite from AGENTS.md §10 and make it green: cargo fmt --all -- --check; cargo clippy --workspace --all-targets -- -D warnings; cargo nextest run --workspace; cargo test --doc --workspace; cargo deny check. Goldens must be untouched.
4. Update CHANGELOG.md Unreleased with the user-visible changes (width fill semantics, floor raise, Add Task buttons, selection highlight). Do NOT touch the backlog issues; chunk bookkeeping happens at merge time per dispatch plan §10.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the Tier-4 manual checklist items for the user, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of GUI review round 2 (branch feat/ui-r2-board-surface).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §2 (D1–D8) + §1.1 + §14-R2, the dispatch plan .artifacts/plans/2026-10-23-gui-review-round2-implementation-dispatch-plan.md §1/§2 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-board-surface after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) D1 must make ALL columns stretch — verify no residual stretch-0 arm and that the sentinel test never_resized_board… still passes verbatim; (b) the re-clamp (D2) must run at load only, never write back config implicitly (no UiRequest on load — the never-persist-on-refresh invariant); (c) the header height (D3) must come from explicit min/max-height, not from clipping alone; (d) selection paint (D4) must be driven by the already-pushed selected-stack, no new state; (e) the Add Task ghost (D8) must reuse task.create's action path (click ≡ key), not a parallel command; (f) empty diff over taskboard-app and taskboard-domain; goldens byte-identical; (g) no text assertions anywhere in the new tests.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green; verify mergeStateStatus CLEAN as in §1. main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-r2-board-surface.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, merge-state confirmation, worktree-removed confirmation, the Tier-4 items restated for the user. Never merge; never touch main.
```

## 3. Chunk 2 — Interaction repairs (`fix/ui-r2-interaction-repairs`)

- **Covers**: design §3 (D9–D15), facts §1.2 (F-B1–F-B7), risks R1,
  R2, R6. Files: `models.rs` (stable identity), `board-layout.slint`
  / `task-card.slint` (only where model identity requires),
  `adapter.rs` (menu sync 1227-1248, tooltip dismissal, invoke
  seams), `action-menu.slint` (submenu emission, row unhover),
  `tooltip.slint` (width), `lib.rs` (dismiss hook), `tooltip.rs`,
  view/router files for the probe. Non-goals: no drag semantics
  change (they start working via D9), no settings scroll (chunk 5),
  no new actions (chunk 3).
- **Acceptance**: (a) refresh no longer re-instantiates unchanged
  repetitions — the incremental push follows #390's fix direction
  (row-level updates into persistent `VecModel`s); stage-1 records
  the proof mechanism (identity probe via the testing backend or a
  structural model pin) and it lands as a test; the push seam
  carries the #373-pointing constraint from D9; (b) a seam-level
  test interleaves refresh between press and move and the drag
  completes; (c) the double-click Slint wiring is unchanged and its
  survivability is covered by (a)'s proof + a Tier-4 item; (d)
  parent-row pointer clicks open the submenu in wide AND compact
  modes (journeys for both; compact resolves rows against the
  displayed page); (e) tooltip does not survive view switch / menu
  invoke (usability-components journeys under the mock clock); (f)
  tooltip frame width includes the hint row (wide-hint geometry
  pin); (g) window-stability probe matrix (scale {0.5,1.0,
  3.0} × board/stack/task views at min floors) recorded in the chunk
  plan with fixes only via content min-width capping — Window
  width/height bindings remain absent (grep pin); (h) full existing
  suite green UNCHANGED except where a test pinned the buggy
  behaviour (any such edit is a recorded decision, never a silent
  weakening); (i) NO bench work in this chunk — #390's true-partial
  re-measurement stays with the issue (stage 3 annotates #390, it
  does not close).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of GUI review round 2 ("interaction repairs", branch fix/ui-r2-interaction-repairs) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §3 (D9–D15) and §1.2 (F-B1–F-B7) — normative — plus §10/§14-R1/R2/R6, and the dispatch plan §1/§3. Then read issue #390 in full (gh issue view 390) — D9 implements its recorded fix direction, and its cited evidence (the kiosk/netbook reports) is background you must not re-derive; #373 stays out of scope but the push design must not preclude it.
2. Read the current seams: src/models.rs (push_board 258-267 — the wholesale ModelRc replacement #390 documents), ui/widgets/task-card.slint (double-clicked 80), ui/compositions/board-layout.slint (resize-handle 162-182), src/adapter.rs (drag seam 1309-1390, menu pointer handlers 1227-1248, activate_parent 593-600, apply_close_menu 585-587, route's trailing sync 256-257, KeyboardState sync 130-153), ui/compositions/action-menu.slint (invoke-entry 166, declared-but-unused invoke-submenu-entry 45), ui/app-window.slint (menu mount 241-308, tooltip host 314-316, dismissal init 234/242), ui/widgets/tooltip.slint (width 88-89, hint row 124-126), src/lib.rs (tooltip timer 230-273). Tests you must not break silently: journeys 1109-1163 (context menu), 1301-1350 (drag seam), 1357-1380 (sentinel), usability_components 144-452, plus the whole suite.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-fix-ui-r2-interaction-repairs on a new branch fix/ui-r2-interaction-repairs off main. PRECONDITION: chunk 1's PR (feat/ui-r2-board-surface) is merged into origin/main — verify with git fetch origin --prune && git log origin/main --oneline -5; if not merged, STOP and report.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-interaction-repairs-plan.md settling, with recorded decisions: the incremental-push mechanism for D9 (row-diff on push vs caller-side dirty flags — probe on the pinned Slint that row-level VecModel updates avoid repetition re-instantiation; record how unchanged rows are detected and how the #373 non-preclusion constraint is honoured structurally) and its proof test; how D10/D11 pin what headless can (refresh-interleaved drag test) and enumerate the Tier-4 items (double-click feel, drag feel); the D12 sync refactor shape (shared post-mutation sync; compact page-aware row resolution — spell out the index-space rule) and the submenu emission change; the D13 dismissal hooks (exact effect-application points) + the ActionMenu row unhover; the D14 width formula (max of caption-natural and hint-row-natural, same caps) and its geometry pin; the D15 probe design (instrumentation method, the matrix, expected offenders, fix-via-min-width-capping rule, escalation path if the hypothesis fails). Test catalogue per design §10.
5. Commit it as: docs(plan): interaction repairs for review round 2. Do not implement; do not push.

Report: plan path, decisions (esp. the model-identity probe result and the D15 probe design), risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of GUI review round 2 ("interaction repairs").

1. Read AGENTS.md, the design round §3/§1.2/§10, the dispatch plan §1/§3, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-interaction-repairs-plan.md) in the worktree ../taskboard-WT-fix-ui-r2-interaction-repairs (branch fix/ui-r2-interaction-repairs). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. This chunk lands regressions-first: each fix (D9–D14) lands with the test that fails before it. Hard rules as in chunk 1 (SPDX, no sleeps, no text assertions, standalone commits) plus: any edit to an existing test that pinned the buggy behaviour needs the recorded-decision comment naming this plan section. The Window width/height invariant (app-window.slint 84-90) must survive untouched.
3. Run the full gate suite from AGENTS.md §10 and make it green; the full existing journey suite must pass except for recorded decisions.
4. Update CHANGELOG.md Unreleased (double-click works, drag works, submenu clicks work, tooltips no longer stick, tooltip fits hints, view switches keep window size). Do NOT touch the backlog.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, regression-test list (fix → test), the Tier-4 manual checklist for the user (double-click, drag feel, window size across view switches incl. scale 3.0), deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of GUI review round 2 (branch fix/ui-r2-interaction-repairs).

1. Read AGENTS.md, the design round §3 (D9–D15) + §1.2 + §14-R1/R2/R6, the dispatch plan §1/§3 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-fix-ui-r2-interaction-repairs after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) D9 is the round's highest-risk change and implements #390's fix direction: verify the incremental push is proven by a test, that row-level updates keep push paths pushing COMPLETE state (no incremental-update drift where a reconcile would have fixed it — reconcile-on-vanish paths must still run), that the #373 non-preclusion constraint is honoured (for-repeater structure and stable per-column addressing intact), and that no journey was weakened silently (every test edit carries the recorded-decision comment); (b) the drag repair must keep live per-tick width updates and release-only persistence (drag seam journey semantics unchanged); (c) the menu sync refactor must not double-sync or regress leaf clicks (an_outside_click_dismisses_the_menu still green) and the compact page resolution must be proven by a journey, not just the wide-mode one; (d) tooltip dismissal hooks must be effect-application points in the adapter, not scattered Slint timers; (e) the D15 probe results must be in the plan and fixes must be content-min cappings only — grep the diff for any Window width/height binding (finding if present); (f) no text assertions; empty diff over taskboard-app/taskboard-domain.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, gh pr checks --watch to green, mergeStateStatus CLEAN, as in §1. Then post the bookkeeping comment on #390: the incremental-push fix direction has landed via this PR, the true-partial bench re-measurement stays open with the issue (annotate, do NOT close). main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-fix-ui-r2-interaction-repairs from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, Tier-4 items restated. Never merge; never touch main.
```

## 4. Chunk 3 — Action registry round-up & object context menus (`feat/ui-r2-actions-context-menus`)

- **Covers**: design §4 (D16–D21), facts §1.4 (F-E1–F-E4) and the
  F-B7 chain fix, risks R3. Files: `actions.rs`, `keymap/presets.rs`,
  keymap snapshots, `router.rs`, `view_state.rs` (jump_focus, stack
  next/prev, cancel arm), `adapter.rs` (interpret table, context
  seams), `menu.rs` (group pages, chains), `board-layout.slint`
  (header right-click), `board-bar.slint` (segment right-click),
  `task-card.slint` (unchanged pattern reference). Non-goals: no
  domain/state commands added (all exist, facts §1.6); no drag-to-
  move (chunk 8); no reminder-action bindings beyond what D16-D21
  name (reminder.* unbound rows stay as recorded debt).
- **Acceptance**: (a) new registry rows exactly as D16–D20 name them
  (ids, labels, enablement contexts) with bindings in all three
  presets, registry + resolved-keymap snapshots regenerated in the
  same commits; (b) coverage contracts extended
  (`every_preset_covers_the_core_verbs` + a move-cancel-in-every-
  preset contract) — conflict/shadow contracts green without
  weakening; (c) Escape cancels a move flow in all presets and C-g
  cancels in emacs — journey per preset; the lone-C-g-vs-app.redo
  prefix resolution is pinned by a keymap test; (d) stack.next/prev
  wrap and land focus on the new stack's first task — journey;
  (e) board/stack create/delete route the existing StateCommands —
  journeys asserting the dispatched command, not engine effects
  beyond the engine's own suite; (f) task.reopen dispatches
  `SetTaskDone { done: false }`, enabled only on a completed focused
  task — journey + enablement pin; (g) first/last jump — journey
  from both ends, no-wrap respected at the target; (h) context menus:
  task chain fix renders hints (pin: task-menu rows carry non-empty
  hint vecs where the board chain has bindings), stack and board
  menus open from header/segment right-click at pointer anchor with
  the D21 entry sets — journeys invoking the new seams; (i) emacs
  `stack.create` binding lands (D6's hint source becomes non-empty).

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of GUI review round 2 ("action registry round-up & object context menus", branch feat/ui-r2-actions-context-menus) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §4 (D16–D21) and §1.4 (F-E1–F-E4) + §1.2 F-B7 — normative — plus §10/§14-R3, and the dispatch plan §1/§4.
2. Read the current seams: src/actions.rs (ActionId 150-227, REGISTRY 302-648, coverage test 675-716), src/keymap/presets.rs (STANDARD 62-108, EMACS 114-166, VI 173-215), src/keymap/mod.rs (first_binding_display 111-138, coverage/conflict contracts 426-605), src/router.rs (TaskComplete arm 159-162, RequestMove 206), src/view_state.rs (Flow::Move table, select_stack 510, step_focus 434-505), src/adapter.rs (interpret table 406-508, open_task_context_menu 1433-1448, context seams 1455-1494), src/menu.rs (row 50-61, context_entries 113-118, group tests 240-290), ui/widgets/task-card.slint (right-click pattern 85-91), ui/compositions/board-layout.slint (header TouchArea 136-185), ui/compositions/board-bar.slint (segments 43-94). Tests: keymap_snapshots (regen discipline), menu.rs T-1..T-6, journeys.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-actions-context-menus on a new branch feat/ui-r2-actions-context-menus off main. PRECONDITION: chunk 2's PR is merged (git fetch origin --prune && git log origin/main --oneline -5); if not, STOP and report.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-actions-context-menus-plan.md settling, with recorded decisions: the exact registry rows (ids, labels, descriptions, enablement chains, group membership incl. the new Stack/Board groups) and the final binding table across all three presets for D16-D20 + the emacs stack.create fill — run the conflict/shadow contract mentally per candidate and record the resolution for the C-g-vs-app.redo ordering (D16); the interpret-table arms (CancelMove, SetMode-during-Move, stack next/prev wrap semantics); jump_focus(Edge) shape; the context-chain fix (task → [Board, Global]) and the per-object entry sets for stack/board menus incl. which entries are enablement-gated; the right-click seams for header/segment (callback signatures, pointer anchor); snapshot regen list; test catalogue (per-preset move-cancel journeys, wrap journey, dispatch-pinning journeys for create/delete/reopen, hint-presence pins, menu-open journeys). Binding deviations from the design's concrete key proposals are permitted with a recorded reason (constraint: every preset binds every new action; R3).
5. Commit it as: docs(plan): action registry round-up & object context menus. Do not implement; do not push.

Report: plan path, decisions (final binding table, C-g resolution), risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of GUI review round 2 ("action registry round-up & object context menus").

1. Read AGENTS.md, the design round §4/§1.4/§10, the dispatch plan §1/§4, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-actions-context-menus-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-actions-context-menus (branch feat/ui-r2-actions-context-menus). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules as before; additionally: registry/keymap snapshot files regenerate in the SAME commit as the rows that change them (insta --accept is not enough — eyeball the diff and record it in the commit body); new a11y labels/descriptions are presentation constants, never inline strings at use sites; destructive deletes route the existing StateCommands with their typed refusals — no new confirmation UI this round.
3. Run the full gate suite from AGENTS.md §10 and make it green; the conflict/shadow keymap contracts must pass unweakened.
4. Update CHANGELOG.md Unreleased (new actions + bindings, context menus for stacks/boards, move-cancel via Escape/C-g, reopen, first/last jumps). Issue closures happen at merge time (dispatch plan §10): stage 3 posts the #229 and #232 ✅ DONE comments after the PR is green.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, snapshot-diff summary, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of GUI review round 2 (branch feat/ui-r2-actions-context-menus).

1. Read AGENTS.md, the design round §4 (D16–D21) + §1.4 + §14-R3, the dispatch plan §1/§4 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-actions-context-menus after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) every new action must be bound in ALL THREE presets (coverage contract, not spot checks) and the snapshots must match the rows exactly; (b) the C-g ordering resolution must be pinned by a keymap test (lone C-g during a move flow cancels; completing the sequence outside a flow still redoes); (c) the task context-menu chain fix (F-B7) must actually render hints — assert hint-vec presence structurally, never hint strings; (d) stack/board context menus must dispatch the EXISTING StateCommands (CreateBoard/DeleteBoard/CreateStack/DeleteStack/MoveStack/RenameStack) — no domain diff; (e) task.reopen enablement must mirror task.complete's machinery inverted; (f) wrap semantics: stacks wrap, reading-order jumps respect no-wrap at the target; (g) no text assertions; empty diff over taskboard-app/taskboard-domain.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. Then post the bookkeeping comments: ✅ DONE on #229 (task reopen) and #232 (first/last-item navigation) with one-line notes + PR link. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-actions-context-menus from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, bookkeeping confirmations. Never merge; never touch main.
```

## 5. Chunk 4 — Task view & text-field ergonomics (`feat/ui-r2-taskview-textfields`)

- **Covers**: design §5 (D22–D25), facts F-C4/F-C5/F-E5/F-E6, risk
  R2. Files: `text-field.slint`, `rule-list.slint`,
  `task-view.slint`, new `date-picker.slint` (calendar popover), new
  `src/due_edit.rs` (parse + picker state), `presentation.rs`
  (due helpers reuse), adapter seams for due commit. Non-goals: no
  settings-view edits (chunk 5 consumes the widget fix), no
  natural-language date grammar ("next friday" stays deferred), no
  reminder toast/nag surface.
- **Acceptance**: (a) the TextInput preferred-width no longer
  propagates into ancestor layouts — geometry pin (field fills its
  cell at fixed width while text is typed; stage-1 records the
  Slint mechanism); (b) settings fields are NOT touched but a
  reuse-note in the plan records chunk 5's inheritance; (c)
  checkmark button: visible when focused + non-empty, click ≡
  Return (structural + journey pins on title and reminder fields);
  (d) preset chips have padding/min-width — geometry pin; custom
  offset shares the presets row with stretch, parse-miss clears the
  buffer — journey; (e) due editor typed path: valid `YYYY-MM-DD`
  commits `UpdateTask { duedate: Some(Some(..)) }`, empty commits
  `Some(None)`, invalid refuses typed and keeps the buffer —
  journeys + parse unit tests (property test on the parser grammar
  bounds); live echo uses the presentation due phrase (structure
  only, no text assertion); (f) due editor calendar path: the
  popover month grid opens from the field, arrow keys move the day
  cursor, month switch works, Enter commits the cursor day through
  the SAME seam as the typed path (click ≡ key), Escape closes
  without committing, clicking a day commits — journeys at the seam
  level; today marker + selected-day highlight present
  structurally; (g) the picker's `duedate` write lands in ONE place
  (the shared due-commit seam) — no parallel dispatch paths.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of GUI review round 2 ("task view & text-field ergonomics", branch feat/ui-r2-taskview-textfields) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §5 (D22–D25) + facts F-C4/F-C5/F-E5/F-E6 — normative — plus §10/§14-R2/R5, and the dispatch plan §1/§5.
2. Read the current seams: ui/widgets/text-field.slint (whole widget — the width/alignment fix and the checkmark hook), ui/views/task-view.slint (title field 86-93, metadata row 104-143, the R1-D13 comment 117-129), ui/compositions/rule-list.slint (presets 69-88, offset field 90-94), src/adapter.rs (commit_text 1671-1679, on_rule_add_custom_accepted 1789-1801), src/view_state.rs (commit_edit 674-721), src/presentation.rs (due helpers 26-161), domain TaskChanges.duedate (messages.rs 263-276, command.rs 358-360 — read-only anchoring, NO domain edits). Tests: journeys rule-list seams 1933+, usability_components.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-taskview-textfields on a new branch feat/ui-r2-taskview-textfields off main. PRECONDITION: chunk 3's PR is merged; if not, STOP and report.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-taskview-textfields-plan.md settling, with recorded decisions: the exact D22 mechanism on pinned Slint 1.18 (probe how to neutralize TextInput preferred-width propagation — wrapping container vs explicit geometry — and the geometry proof test); the checkmark button anatomy (D23: visibility rule, icon source, click seam = accepted()); the reminder row composition (D24: chip padding/min-width tokens, field stretch + min-width, buffer-clear-on-parse-miss); the due editor (D25, AMENDED: calendar picker + typed entry are BOTH in scope — the typed grammar (chrono, accepted forms — record exactly, e.g. YYYY-MM-DD only vs +Ndmoy), the calendar popover anatomy (month grid, prev/next month, today marker, selected highlight, due-tint reuse from presentation DueKind), the full keyboard contract (open, arrows, month switch keys, Enter commits, Escape closes clean), the ONE shared commit seam both paths funnel into, the empty-input-clears and invalid-refuses flows, where the parse/picker-state module lives); Tier-4 items (checkmark feel, field growth visual, calendar popover feel at min viewport and scale 3.0); test catalogue (typed journeys, picker seam journeys, grammar proptest).
5. Commit it as: docs(plan): task view & text-field ergonomics. Do not implement; do not push.

Report: plan path, decisions (esp. the D22 probe result and the due grammar), risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of GUI review round 2 ("task view & text-field ergonomics").

1. Read AGENTS.md, the design round §5 + §10, the dispatch plan §1/§5, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-taskview-textfields-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-taskview-textfields (branch feat/ui-r2-taskview-textfields). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules as before; additionally: the TextField widget change must keep every existing call site compiling and its journeys green (the widget is shared by ~15 call sites — no per-call-site behaviour forks unless the plan records one); the due-date editor replaces the R1-D13 scope-cut comment with a pointer to design D25, and BOTH interaction paths (typed + calendar) ship in this chunk — the calendar must be fully keyboard-operable (the user's review demands both paths); SPDX header on src/due_edit.rs and any new slint files.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Update CHANGELOG.md Unreleased (text fields stop growing leftward, checkmark commit buttons, reminder row layout, due-date editing). Do NOT touch the backlog; stage 3 posts the #227 ✅ DONE comment.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, call-site inventory touched, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of GUI review round 2 (branch feat/ui-r2-taskview-textfields).

1. Read AGENTS.md, the design round §5 (D22–D25) + §14-R2/R5, the dispatch plan §1/§5 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-taskview-textfields after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the D22 fix must be in the WIDGET, not per-call-site patches — a call-site width override is a finding; (b) the checkmark must invoke the same accepted() seam as Return (click ≡ key journey), and must not fire on empty buffers or unfocused fields; (c) the reminder offset parse-miss must clear the buffer AND dispatch nothing (the D12 absorb rule survives); (d) the due editor: BOTH paths must funnel through ONE commit seam — a calendar commit that bypasses the typed path's validation/clear semantics is a finding; the keyboard contract of the picker must be complete (arrows, month switch, Enter, Escape) and seam-journey-tested, not just the mouse clicks; invalid typed input refuses with buffer kept, empty clears (Some(None)); no domain edits; (e) empty diff over taskboard-app/taskboard-domain; no text assertions (due-phrase echo and month captions asserted structurally).
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. Then post the bookkeeping comment: ✅ DONE on #227 (due-date editor) with a one-line note + PR link. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-taskview-textfields from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, bookkeeping confirmation, Tier-4 items restated. Never merge; never touch main.
```

## 6. Chunk 5 — Settings form, feedback, help (`feat/ui-r2-settings-form`)

- **Covers**: design §6 (D26–D31), facts F-C1/F-C2/F-C3/F-C6,
  F-C4-inheritance from chunk 4, risks R2/R5. Files: new
  `segmented-control.slint`, `slider.slint`, `info-tip.slint`, new
  `src/settings_status.rs`, `settings-view.slint`, `adapter.rs`
  (echo at `apply_keymap_update` 893-898, outcome wiring 936-943),
  `theme.slint` (tokens if needed), app-side: none expected (status
  text is UI-side mapping over existing typed outcomes) — an app
  diff needs a recorded reason. Non-goals: no connection-management
  surface changes (chunk 6 owns §7; the D28 status line is built
  here and consumed there), no ceremony changes.
- **Acceptance**: (a) SegmentedControl: single-selection visual
  contract (one selected: surface-raised + text-primary; others
  text-secondary), a11y radio-group role, keyboard operability —
  structural + usability pins; (b) `apply_keymap_update` writes
  `preset-index` — journey: pick → reply → selection reflects the
  REPLIED preset (and a synthetic reply for a different preset moves
  it); (c) the inverted-enabled picker is gone; (d) the bare green
  Rectangle is gone; the status line is fixed-height, capped,
  tone dot + mapped text, auto-clears under the mock clock —
  mapping totality proptest over every `SettingsOutcome`/`SettingsError`
  variant (coverage, never strings); (e) scale slider: 0.5–3.0 step
  0.1, release-commits one `SetScale`, field stays
  Return-authoritative, both clamp identically — journeys; (f)
  settings content in a Flickable with pinned header; at scale 3.0
  min-floor window the scale control is reachable and the Window
  size invariant holds — journey + Tier-4; (g) InfoTip beside the
  D31 fields, each wired to the generalized tooltip trigger,
  content from presentation constants, non-empty coverage pin.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of GUI review round 2 ("settings form, feedback, help", branch feat/ui-r2-settings-form) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §6 (D26–D31) + facts F-C1/F-C2/F-C3/F-C6 + §14-R2/R5 — normative — and the dispatch plan §1/§6. Note explicitly: this chunk REVERSES round-1 R5 (status text) by user direction; the no-text-assertions TEST rule still binds.
2. Read the current seams: ui/views/settings-view.slint (whole view: picker 269-296, scale 300-321, outcome marker 323-332, LabeledField 92-120), src/adapter.rs (push_settings 803-944 incl. apply_keymap_update 893-898 and outcome 936-943, mount seed 1205-1212, scale seam 1112-1137, preset seam 1085-1108), src/requests.rs (SettingsOutcome/SettingsError 67-116), ui/compositions/action-menu.slint (the in-repo Flickable pattern 95+), ui/widgets/kbd-hint.slint + tooltip.slint (trigger pattern for InfoTip). Tests: journeys 1503-1931 (settings journeys — the preset-pick journey J7 1873 must be EXTENDED, not weakened), ui_settings.rs (app-side, must stay green unchanged).
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-settings-form on a new branch feat/ui-r2-settings-form off main. PRECONDITION: chunk 4's PR is merged; if not, STOP and report.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-settings-form-plan.md settling, with recorded decisions: the SegmentedControl API (properties, selection model, a11y role) and its reuse story; the keymap echo path (which function writes preset-index and when — including REPL-initiated changes arriving via a later refresh); the status-line state machine (states, tone dot, text mapping table location = new src/settings_status.rs with an exhaustive match per variant, auto-clear timing under the mock clock — the exact duration and where the timer lives); the slider anatomy (step/release-commit semantics, preview path, one-request-per-gesture proof); the Flickable composition (what stays pinned: header; scrollbar widget choice; min-content contract at scale 3.0); the InfoTip widget + the help-text constants module + the per-field placement list; the inverse-enabled purge; test catalogue incl. the mapping-totality proptest and the scale-3.0 reachability journey + Tier-4 items (scroll feel, slider feel, status-line legibility).
5. Commit it as: docs(plan): settings form, feedback, and help. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of GUI review round 2 ("settings form, feedback, help").

1. Read AGENTS.md, the design round §6 + §10, the dispatch plan §1/§6, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-settings-form-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-settings-form (branch feat/ui-r2-settings-form). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules as before; additionally: the status-text mapping lives in src/settings_status.rs behind an exhaustive match (compiler-enforced totality over SettingsOutcome/SettingsError — a new variant must fail to compile until mapped); help texts are presentation constants; the auto-clear timer is mock-clock driven (no real sleeps; usability_components pattern); SPDX headers on new files.
3. Run the full gate suite from AGENTS.md §10 and make it green; the app-side ui_settings.rs tests must pass unchanged (an app-crate diff here needs the recorded reason).
4. Update CHANGELOG.md Unreleased (keymap picker, scale slider, scrollable settings, visible feedback text, help tooltips). Do NOT touch the backlog.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, the Tier-4 manual checklist (scroll/scale-3.0 reachability, slider feel, status legibility), deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of GUI review round 2 (branch feat/ui-r2-settings-form).

1. Read AGENTS.md, the design round §6 (D26–D31) + §14-R2/R5, the dispatch plan §1/§6 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-settings-form after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) the echo (D27) must source the preset from the REPLIED keymap, not echo the click — a click-echo is a finding (REPL/config-initiated changes must land too); (b) the status line must be structurally unable to absorb layout height (fixed min/max, stretch 0) and must auto-clear under the mock clock — a permanently-visible status is the F-C3 bug reborn; (c) the mapping totality must be compiler-enforced (match without a wildcard arm), asserted by coverage, never by strings; (d) the slider commits once per gesture (no request spam) and reverts on typed error exactly like the field; (e) the Flickable must keep the header pinned and the Window invariant intact (no width/height bindings); (f) no text assertions anywhere (status/help strings are pinned via the exhaustive-match + non-empty coverage only); empty diff over taskboard-app/taskboard-domain.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-settings-form from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, Tier-4 items restated. Never merge; never touch main.
```

## 7. Chunk 6 — Connection management rework (`feat/ui-r2-connection-management`)

- **Covers**: design §7 (D32–D34), facts F-C7/F-C8/F-C9/F-D1–F-D4,
  risks R4. Files: `requests.rs` (new UiRequests + reply payloads),
  `models.rs` (boards/connectors push), `adapter.rs` (seams),
  `settings-view.slint` (board selector, connector rows, backend
  picker, busy states), `presentation.rs`/`settings_status.rs`
  (status text for the new outcomes), app-side: `ui.rs` (UiSettings
  payload), `ui_requests.rs` (Attach/Detach handlers: verify →
  attach → BindBoard / detach → UnbindBoard via the engine command
  path the REPL bind uses, `commands/boards.rs:707` pattern; info
  logs at save/attach start+success), `connections.rs` (overlay
  helpers if needed). Non-goals: no domain/state/sync-crate diffs
  (§1.6); no multi-backend connectors (Nextcloud only, picker renders
  future kinds disabled); no schedule/pull-semantics changes; the
  secret path (F-C9) untouched.
- **Acceptance**: (a) UiSettings carries boards (id, name,
  bound_connector) + connectors (name, backend, server_url,
  username, attached) at mount and after every settings reply —
  wiremock choreography extends `ui_settings.rs`; (b) Attach:
  happy path attaches + binds (engine receives BindBoard with the
  right ids) and replies the refreshed pair; typed refusals
  (ConnectorNameInUse/Draining/Credential/…) map through the
  existing tables — table tests extended; (c) Detach: detaches +
  unbinds; (d) busy/spinner states: form and rows expose per-op
  busy (mock-clock state machine; no concurrent second op — the
  `pending` gate reused); (e) every Save click produces a visible
  reaction: disabled-Save surfaces its reason (stage-1 mechanism),
  enabled Save runs the ceremony with start+success info logs and
  the typed outcome in the status line; failing verify persists
  nothing (existing test A-family still green); (f) board selector
  reflects AppState boards and binding state, and Bind/Remove
  journeys update rows from replies; (g) secret/redaction pins
  (requests.rs 168-211, ui_requests.rs 1138-1151, journeys 1734)
  pass UNTOUCHED; (h) REPL E2E suites (cli_connect.rs etc.) pass
  unchanged.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of GUI review round 2 ("connection management rework", branch feat/ui-r2-connection-management) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §7 (D32–D34) + facts F-C7/F-C8/F-C9/F-D1–F-D4 + §14-R4 — normative — and the dispatch plan §1/§7.
2. Read the current seams: src/requests.rs (UiRequest enum, Secret, reply payload patterns 25-165), src/adapter.rs (SettingsVm/draft 690-760, save seam 992-1031, push_settings 803-944, install 1195-1212), src/models.rs (SettingsModel), ui/views/settings-view.slint (Connections section 155-266), taskboard-app/src/ui.rs (UiSettings mount 194-267), taskboard-app/src/ui_requests.rs (save_connection 291-354, test 272-283, current_connectors 444-452, mapping tables 481-552), taskboard-app/src/commands/boards.rs (the REPL bind path 700-730 — the engine-command route you will reuse), taskboard-app/src/bootstrap.rs (ConnectorControl 141-269, AttachError 646-672), taskboard-domain/src/messages.rs (BindBoard 139-148, UnbindBoard 242-250 — read-only), taskboard-app/src/connections.rs (overlay writers 131-232). Tests: ui_settings.rs A-family, cli_connect.rs, journeys 1686-1931, redaction pins.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-connection-management on a new branch feat/ui-r2-connection-management off main. PRECONDITION: chunk 5's PR is merged; if not, STOP and report.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-connection-management-plan.md settling, with recorded decisions: the exact UiRequest/reply shapes for Attach/Detach (payloads: board id + connector name; reply = refreshed boards+connectors pair or typed error — name the types); the app-side handler orchestration order (verify-if-needed → CredentialStore → attach_connector → BindBoard via the engine command sender — WHICH sender/handle, prove it exists at that call site; settle via wait_for_boot_pull; detach = detach_connector_named → UnbindBoard) and its failure/rollback story (attach ok but bind refused: detach again? record it); the UiSettings payload extension (types, mount + reply refresh points); the board-selector widget choice (BoardBar language vs list) and the connector-row anatomy (badges, per-row Test/Bind/Remove enablement rules — e.g. Remove on an attached connector detaches first); the busy state machine (per-op pending gate, spinner rendering, mock-clock); the visible-reactions contract for Save (disabled reason surfacing, start/success tracing::info fields — host only, never credentials); the backend picker rendering (Nextcloud enabled, others disabled "coming soon" — as structure, not text assertion targets); Tier-4 items (spinner feel, selector feel); test catalogue over wiremock extending ui_settings.rs patterns + journeys.
5. Commit it as: docs(plan): board-scoped connection management. Do not implement; do not push.

Report: plan path, decisions (esp. the engine-command route and the bind-failure rollback), risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of GUI review round 2 ("connection management rework").

1. Read AGENTS.md, the design round §7 + §10, the dispatch plan §1/§7, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-connection-management-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-connection-management (branch feat/ui-r2-connection-management). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules as before; additionally: the oneshot-in-mpsc lane discipline (AGENTS §3) for the new requests — dropped-reply and closed-channel cases typed and non-blocking; secrets never on models, logs, or Debug (the redaction pins must pass untouched); tracing fields carry host/name only, never credentials; no domain/state/sync-crate diffs (a missing engine handle at the call site is a STOP-and-record, not a domain patch).
3. Run the full gate suite from AGENTS.md §10 and make it green; REPL E2E (cli_connect) and the ui_settings A-family must pass unchanged.
4. Update CHANGELOG.md Unreleased (board-scoped connection management, attach/detach from settings, visible save ceremony). Do NOT touch the backlog.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, Tier-4 checklist (spinner, selector, save-reaction feel), deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of GUI review round 2 (branch feat/ui-r2-connection-management).

1. Read AGENTS.md, the design round §7 (D32–D34) + §14-R4, the dispatch plan §1/§7 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-connection-management after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) trace the app-side Attach orchestration against the plan's recorded order — any behaviour drift in verify/store/attach/bind ordering is a finding; prove bind goes through the SAME engine command path the REPL uses; (b) secrets: prove the token path (form → request → CredentialStore) never reaches toml, logs, Debug, or UI models (redaction pins untouched); (c) the reply lane: oneshot-in-mpsc, dropped-reply/closed-channel typed and non-blocking, UI never blocks on a reply; (d) busy gate: no second op while one is pending; the spinner state machine is mock-clock tested; (e) rows/selector refresh ONLY from replies/mount — no speculative polling; (f) empty diff over taskboard-domain/taskboard-state/taskboard-storage-sqlite/taskboard-sync-nextcloud; REPL E2E green UNCHANGED; (g) no text assertions (backend "coming soon" rows asserted structurally).
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-connection-management from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, Tier-4 items restated. Never merge; never touch main.
```

## 8. Chunk 7 — Visual identity: stacks as cards, boards as tabs (`feat/ui-r2-visual-identity`)

- **Covers**: design §8 (D35–D37), facts F-A9/F-A10, risks R2.
  Files: `theme.slint` (token ladder), `board-layout.slint` (card
  wrap, gutter), `board-bar.slint` (tab strip), goldens. Non-goals:
  no behaviour changes — every journey from chunks 1–6 passes
  unchanged; no tooltip/menu paint; no settings paint.
- **Acceptance**: (a) token ladder recorded in `theme.slint` with
  the card/surface steps named and documented; (b) stack columns
  render as cards (background, radius, gutters as the visible board
  strip) with headers/cards/ghost row adjusted to the ladder —
  goldens regenerated in ONE dedicated commit whose body lists the
  reviewed visual diffs; (c) board bar renders as tabs per D36
  (active merged-with-content rounded top, inactive bordered
  markers, baseline rule) with fixed height/elide/tooltip behaviour
  preserved — board-bar journeys unchanged; (d) full suite green
  with ONLY golden-snapshot updates as allowed test diffs (each
  reviewed and listed).

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of GUI review round 2 ("visual identity", branch feat/ui-r2-visual-identity) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §8 (D35–D37) + facts F-A9/F-A10 + §14-R2 — normative — and the dispatch plan §1/§8.
2. Read the current seams: ui/theme.slint (whole token set 20-123), ui/compositions/board-layout.slint (columns row 114-128, header, cards, ghost row as of chunk 1+4), ui/compositions/board-bar.slint (segments 36-94), tests/golden_visuals.rs + tests/goldens/ (the matrix). PRECONDITION: chunks 1 AND 4 are merged (git fetch origin --prune && git log origin/main --oneline -5); if not, STOP and report.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-visual-identity on a new branch feat/ui-r2-visual-identity off main.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-visual-identity-plan.md settling, with recorded decisions: the token ladder (which existing tokens become what; any new tokens; contrast sanity at both scale breakpoints); the card anatomy (which element wraps, padding, radius; how headers/cards/ghost/ad-tasks rest on the ladder); the gutter token decision (space-3 kept or space-4 per probe); the tab strip anatomy (active/inactive paint, baseline rule, focus ring interplay, max-width/elide preserved); the golden regen discipline (one commit, diff list reviewed); Tier-4 items (tab look at min viewport, card contrast on the user's monitor). NO behaviour changes — the plan's acceptance is the unchanged journey suite.
5. Commit it as: docs(plan): visual identity — stack cards & board tabs. Do not implement; do not push.

Report: plan path, decisions (token ladder), risks, estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of GUI review round 2 ("visual identity").

1. Read AGENTS.md, the design round §8, the dispatch plan §1/§8, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-visual-identity-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-visual-identity (branch feat/ui-r2-visual-identity). Recreate the worktree if missing.
2. Implement exactly per the approved plan. Paint only: zero Rust behaviour diffs expected (an adapter/models diff is a finding — stop and record); journeys green unchanged; goldens regenerate in ONE dedicated commit (tests/goldens/*.png) whose body lists each visual diff.
3. Run the full gate suite from AGENTS.md §10 and make it green (golden updates are the only permitted test diffs).
4. Update CHANGELOG.md Unreleased (stack cards, board tabs). Do NOT touch the backlog.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, golden-diff list, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of GUI review round 2 (branch feat/ui-r2-visual-identity).

1. Read AGENTS.md, the design round §8 (D35–D37), the dispatch plan §1/§8 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-visual-identity after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) paint only — any .rs behaviour change is a finding; (b) the tab strip must preserve the fixed one-control height, elide, tooltips, focus rings, and the selected-state semantics from D20b/R1 (board-bar journeys unchanged); (c) the active tab's merged-with-content paint must not break the compact breakpoint (check the compact switcher still reads correctly against the new tokens); (d) goldens: exactly one regen commit, matrix unchanged, every diff listed; (e) token ladder documented in theme.slint as constraints, not narration.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-visual-identity from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations. Never merge; never touch main.
```

## 9. Chunk 8 — Pointer drag-to-move (`feat/ui-r2-drag-to-move`)

- **Covers**: design §9 (D38), issue #226 body (approach + Slint
  re-check), facts F-E3, risks R1/R2. Files: `task-card.slint`
  (press-hold-drag), `board-layout.slint` (drop-target highlight,
  column geometry out), `app-window.slint` (ghost overlay layer),
  `adapter.rs`/`view_state.rs` (drag state machine, MoveTask
  dispatch), models as needed. Non-goals: no touch/kiosk-specific
  gestures, no multi-select drag, no reordering within a stack
  (append/target semantics follow the move flow's existing
  confirm_move — stage-1 records the drop-position semantics:
  end-of-stack, matching the keyboard flow).
- **Acceptance**: (a) re-check recorded: Slint 1.18 native DnD
  surface absent/insufficient → DIY as designed (the issue's
  precondition); (b) press-hold beyond a small slop starts the drag
  (ghost visible, source card dimmed), pointer move highlights the
  hovered column, release dispatches the SAME `MoveTask` the
  keyboard flow confirms (journey: drag seam ≡ confirm_move
  command); (c) release outside any column, Escape (D16's cancel),
  or a leaving effect cancels with no dispatch — journeys; (d) the
  ghost and highlight survive refreshes (chunk-2 stable models) —
  interleaved-refresh journey; (e) single-click and double-click
  semantics unchanged (task-card journeys green); (f) Tier-4:
  drag feel, slop threshold, ghost look.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of GUI review round 2 ("pointer drag-to-move", branch feat/ui-r2-drag-to-move) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-23-gui-review-round2-plan.md §9 (D38) + §14-R1/R2, the issue #226 body (gh issue view 226 — its approach section is normative context), and the dispatch plan §1/§9.
2. Read the current seams: ui/widgets/task-card.slint (TouchArea 68-91), ui/compositions/board-layout.slint (columns geometry as of chunk 7, move-target highlight 184/305-310), ui/app-window.slint (overlay layers — where the ghost mounts), src/view_state.rs (Flow::Move, start_move 736-748, confirm_move 772-786, cancel_move 789-795), src/adapter.rs (the interpret table, board task seams 1590-1617), src/models.rs (post-chunk-2 stable models). PRECONDITION: chunk 7's PR is merged; if not, STOP and report.
3. Run scripts/worktree-doctor.sh, then create the worktree ../taskboard-WT-feat-ui-r2-drag-to-move on a new branch feat/ui-r2-drag-to-move off main.
4. Write the TDD-first plan at .artifacts/plans/2026-10-23-ui-r2-drag-to-move-plan.md settling, with recorded decisions: the Slint 1.18 DnD re-check result (native support? record the probe; DIY expected); the drag state machine (slop threshold in px, states idle/armed/dragging, where it lives — view_state flow vs adapter-local — and why); the ghost (what renders, how it follows the pointer — event source, overlay mount, refresh-survival); the column hit-testing mechanism (geometry source: per-column out-properties vs Slint-side drop-target designation — record the probe); the drop semantics (end-of-stack append via the SAME MoveTask command confirm_move builds — equivalence proof; no position-within-stack this round); the cancel matrix (outside release, Escape, leaving effect, reconcile-on-vanish); touch-early-press interplay with existing click/double-click semantics (the regression risks); test catalogue (seam-level drag journeys incl. interleaved refresh, command-equivalence pin, cancel journeys) + Tier-4 items (feel, threshold, ghost look).
5. Commit it as: docs(plan): pointer drag-to-move. Do not implement; do not push.

Report: plan path, decisions (esp. the DnD probe and hit-testing mechanism), risks, estimated diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of GUI review round 2 ("pointer drag-to-move").

1. Read AGENTS.md, the design round §9, the dispatch plan §1/§9, and the stage-1 plan (.artifacts/plans/2026-10-23-ui-r2-drag-to-move-plan.md) in the worktree ../taskboard-WT-feat-ui-r2-drag-to-move (branch feat/ui-r2-drag-to-move). Recreate the worktree if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone. Hard rules as before; additionally: the drop MUST dispatch the same MoveTask command the keyboard flow confirms (command-equivalence pin); no new StateCommands; single/double-click task-card semantics regress nothing.
3. Run the full gate suite from AGENTS.md §10 and make it green.
4. Update CHANGELOG.md Unreleased (drag-to-move). Do NOT touch the backlog; stage 3 posts the #226 ✅ DONE comment.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, Tier-4 checklist (drag feel), deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of GUI review round 2 (branch feat/ui-r2-drag-to-move).

1. Read AGENTS.md, the design round §9 (D38) + §14-R1/R2, the dispatch plan §1/§9 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-r2-drag-to-move after rebasing onto origin/main.
2. Functional focus — this chunk's traps: (a) command equivalence: the drop path builds/dispatches the identical MoveTask confirm_move produces — the pin must compare commands, not outcomes; (b) the cancel matrix is complete (outside release, Escape, leaving effect, task-vanish reconcile) and each arm is journey-tested; (c) ghost/highlight survive interleaved refreshes (the chunk-2 invariant holds under the new gesture); (d) click/double-click/context-menu semantics on cards are unchanged (existing journeys green); (e) no domain diff; no text assertions; the DnD re-check result is recorded in the plan.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push, open PR, checks green + mergeStateStatus CLEAN per §1. Then post the bookkeeping comment: ✅ DONE on #226 (pointer drag-to-move) with a one-line note + PR link. main is protected: never merge.
7. Teardown: git worktree remove --force ../taskboard-WT-feat-ui-r2-drag-to-move from the project root.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + merge-state + worktree-removed confirmations, bookkeeping confirmation, Tier-4 items restated. Never merge; never touch main.
```

## 10. Completion bookkeeping

- **Backlog closures on merge** (stage 3 posts ✅ DONE + one-line
  note + PR link; the closing comment IS the archive, AGENTS §7):
  #229 + #232 (chunk 3), #227 (chunk 4), #226 (chunk 8). No other
  issue closes: none of the 2026-10-23 findings map to existing
  backlog issues — this design round is their home.
- **Stays open, annotated where a chunk narrows or extends an
  issue's scope**: #351 (settings v2 is still not free-form toml
  editing — chunk 6 may annotate), #347 (D8's per-stack Add Task is
  not quick capture), #411 (D36 is the tab *restyle*; the many-board
  dropdown stays deferred), #412 (D15 completes within-session
  stability; cross-restart persistence stays deferred).
- **New issues to file after this round's chunks merge**: none
  planned; out-of-scope ideas discovered during stages go to
  `backlog:` issues per AGENTS §7 (title prefix, category labels,
  body = summary + permalink to the originating plan/report §).
- **Deliberately not dispatched here** (with their owners): quick
  capture implementation (#347), undo/redo enablement (#356, UI
  picker #353), free-form toml editing (#351), horizontal board
  scrolling / ListView virtualisation (#373 — D9's incremental push
  is designed not to preclude it; the virtualisation analysis round
  it asks for is its own design round), board-selector
  dropdown for many boards (#411), window-geometry persistence
  across restarts (#412), the #390 true-partial bench
  re-measurement (chunk 2 lands the incremental push and annotates
  the issue; the verdict measurement stays with #390), reminder-action preset bindings
  (recorded debt at `actions.rs:545-547`), the reminders toast/nag
  surface (presentation.rs:253 note), `KeyFeedback` rendering
  (declared, unrendered — design §1.5 F-E8).
- **What opens next**: the user's GUI review round 3 takes this
  round's merges as its baseline; Tier-4 manual checklists
  accumulate in each chunk's PR for the user to run against the
  merged result.
