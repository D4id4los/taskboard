# [Plan] GUI Review Round 1 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-22
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the **3** implementation chunks of the
  GUI review round 1 design round (menu system, tooltips, navigation,
  window stability, stack column sizing with device-local persistence,
  settings view with sync connection management). No code in this
  commit.
- **Origin**:
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md` — §1 facts
  (F1–F19, anchored; do not re-derive), §2–§9 decisions (D1–D28b,
  normative), §10 testing strategy, §11 code inventory, §13 backlog
  interactions, §14 risks. Plus the user's 2026-10-22 review findings
  (design §1.0) and the user's chunking decision: **exactly 3 PRs this
  round**, dispatched serially, each gated on the previous chunk's
  merge (no stacks).
- **Status**: READY — pending the user's acceptance of the design
  round (same branch as this file). Chunk 1 is dispatchable
  immediately after acceptance and merge of these artefacts; chunk 2
  is gated on chunk 1's PR being **merged into `main`**; chunk 3 on
  chunk 2's merge. Deliberately **not** dispatched here: quick capture
  implementation (#347), undo/redo enablement (#356/#353), the
  free-form `taskboard.toml` GUI editor (#351), horizontal board
  scrolling / ListView virtualisation (#373), board-selector dropdown
  redesign and window-geometry persistence (new backlog candidates,
  design §13).

## 1. Chunks & ordering

| Chunk | Design slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §2 §3 §4 §5 §7 §9 | `feat/ui-review1-menu-tooltips-navigation` | menu system (toggle, dismissal, close-on-invoke, bounds, ranking, two-tier submenus, descriptions, curation), tooltips (generalized trigger, wrap+cap, in-window placement, shortcut hints), task-view back button + right-click task context menu (+ double-click pin), window min floors, board-bar height/hover + add-stack relocation — `taskboard-ui-slint` only | L |
| 2 | §6 | `feat/ui-stack-width-persistence` | stack column resize (mouse drag + keyboard actions + header elide) and the `UiRequest` lane with debounced device-local `[ui]` persistence (`UiSection.stack_widths`) — `taskboard-ui-slint` + `taskboard-app` | M |
| 3 | §8 | `feat/ui-settings-sync-connections` | settings surface (connections/keymap preset/scale), ceremony extraction shared with the REPL, connection test/save/remove over the chunk-2 lane with oneshot replies, enable `AppSettings` + `Ctrl+,` — `taskboard-ui-slint` + `taskboard-app` | L |

**Dependency graph**: strictly serial 1 → 2 → 3. Chunk 2 shares the
UI seams chunk 1 reshapes (`adapter.rs`, `app-window.slint`,
`theme.slint`) and chunk 3 extends the `UiRequest` lane chunk 2
introduces — so each chunk's stage 1 has a **precondition gate**: the
previous chunk's PR is merged into `main` (verify with
`git fetch origin --prune && git log origin/main --oneline -5`); if
not merged, STOP and report instead of planning. No PR is ever
stacked, so the AGENTS §8 rebase dance should never trigger; if the
user stacks anyway, every stage-3 agent expects it
(`git rebase --onto origin/main <landed-tip> <branch>` +
`--force-with-lease`).

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
   nextest, doc-tests, deny, llvm-cov) and makes it green. Does
   **not** push or open a PR. Reports gate results and deviations.
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
   state ⇒ diagnose before handing off. **Then teardown**: remove the
   now-disposable worktree — from the project root,
   `git worktree remove --force ../taskboard-WT-<branch>`
   (everything precious is pushed; a later round that resumes the
   chunk recreates it on the branch). The handoff contract is "PR
   open + checks green + merge state `CLEAN` + worktree removed" so
   the user's merge needs no additional rebase and the chunk leaves
   no 36–55G of build output behind. Reports a verdict. Never merges;
   never touches `main`. (`main` is branch-protected — merges happen
   on GitHub by the user, after the required checks pass.)

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
  finding, even if it passes tests.

Findings from the mandate are fixed on the branch like any other
finding (refactor commits with proper `refactor(<module>):` subjects,
or amend where the commit is not yet shipped).

## 2. Chunk 1 — Menu, tooltips, navigation, window stability (`feat/ui-review1-menu-tooltips-navigation`)

- **Covers**: design §2 (D1–D7: hamburger toggle, outside-click
  dismissal layer, close-on-invoke on both input paths, panel bounded
  on all four edges incl. narrow fallback, `menu_rank` display
  priority, two-tier submenu with regular/expansive fly-out vs
  compact replace+back, per-row descriptions), §3 (D8–D11:
  generalized tooltip trigger out of `IconButton`, word-wrap + width
  cap + content-hugging size, degenerate-clamp-proof in-window
  placement, `binding_hint` shortcut row), §4 (D12–D14: task-view
  back button routing `focus_board`, right-click task context menu
  reusing the generated-menu pipeline anchored at the pointer,
  double-click journey pin), §5 (D15: scale-aware window min floors,
  no `width`/`height` bindings anywhere, all views lay out at the
  floors), §7 (D20b–D23: board-bar fixed height + elide + segment
  tooltips, "+ New stack" relocated into the columns row with compact
  reachability, selector stays segments), §9 (D10 curation). UI crate
  only. **Non-goals**: no stack widths or `UiRequest` lane (chunk 2),
  no settings view and **no enabling of `ActionId::AppSettings`**
  (chunk 3 — the row renders top-level but stays disabled), no
  domain/state/storage/sync changes, no horizontal board scroller, no
  keymap semantics changes beyond added bindings for the context-menu
  era (none required in this chunk).
- **Acceptance**: hamburger toggles (open → second click closes →
  third click shows the **same** entries as the first open — pins F1's
  fix); outside click closes (journey pins the dismissal layer's
  z-order behavior); mouse-invoking "Edit task" navigates **and**
  closes the menu (keyboard path unchanged, `lib.rs:718-720` pin
  intact); the panel stays inside the window at every tested window
  size incl. the narrow fallback; submenu navigation works in both
  presentation modes with Escape returning level-by-level; Quit is
  the last row; tooltip rect ⊆ window rect property holds for
  adversarial labels (proptest), long text wraps, `\n` honored,
  tooltip anchored to the hovered element; shortcut hint appears
  exactly when a binding exists; back button routes to board;
  right-click opens the task context menu and its invoke routes
  typed; double-click journey green; window-size checklist
  (manual/Tier-4, recorded in the PR body) shows no view switch
  changing the window rect; board bar is single-control-height with
  segment tooltips; add-stack reachable in regular/expansive **and**
  compact; `menu_path_stays_live_mid_edit` reconciled per D3 without
  weakening a correct assertion; registry snapshot regenerated in the
  same commits as registry changes; `git diff main...HEAD --
  crates/taskboard-domain crates/taskboard-state
  crates/taskboard-storage-sqlite crates/taskboard-sync-nextcloud`
  empty; gates green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of GUI review round 1 ("Menu, tooltips, navigation, window stability", branch feat/ui-review1-menu-tooltips-navigation) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the accepted design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md — §1 facts F1–F11 (anchored root causes; do not re-derive), §2 (D1–D7), §3 (D8–D11), §4 (D12–D14), §5 (D15), §7 (D20b–D23), §9 (D10), §10 (testing), §14 risks R1–R3 (normative), and the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§2.
2. Read the current seams you will extend: crates/taskboard-ui-slint/src/adapter.rs (toolbar seam ~488-495, OpenMenu arm ~312-338, InvokeSelected ~340-347, invoke_entry ~440-449, sync ~76-82), src/shell.rs (MenuState, chain), src/menu.rs (entries projection), src/actions.rs (REGISTRY + category labels + reserved rows), src/router.rs (AppMenu/InvokeSelected/disabled arms), src/lib.rs (menu_path_stays_live_mid_edit ~1313-1335, keyboard-close pin ~718-720, tooltip timer ~198-232), src/view_state.rs (focus_board ~299-302), src/responsive.rs, ui/app-window.slint (panel host ~157-168, tooltip placement ~177-183, key capture ~88-98), ui/compositions/action-menu.slint (min-width/Flickable caveat ~28-46), ui/compositions/board-bar.slint, ui/compositions/board-layout.slint (columns ~91-147, empty-board add ~65-77, compact ~159-240), ui/views/task-view.slint, ui/widgets/tooltip.slint, ui/widgets/icon-button.slint (~150-156), ui/widgets/kbd-hint.slint, ui/widgets/task-card.slint (~57-60), plus the tests you must not break (tests/support/fake_engine.rs, keymap snapshot).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree ../taskboard-WT-feat-ui-review1-menu-tooltips-navigation on a new branch feat/ui-review1-menu-tooltips-navigation off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-review1-menu-tooltips-navigation-plan.md settling concretely: the exact MenuState extension for the two-tier model (active level, fly-out anchor) and its sync onto KeyboardState; the toggle seam change and the pre-menu-chain entries recomputation on every open; the dismissal layer's position in the root tree and its verified hit-test order (probe on the pinned Slint version, record the result); the D3 reconciliation of menu_path_stays_live_mid_edit — state precisely which invariant that test really guards and how each assertion is preserved, replaced, or renamed; the panel width/anchor/clamp math incl. the narrow fallback and the Flickable-cavity resolution (definite width verified empirically, R2); the registry's menu_rank/description shape and the full curation table (top-level order incl. disabled rows, submenu grouping, Quit pinned last); the tooltip generalized trigger API and its adopters; the D9/D10 width cap numbers and placement algorithm; binding_hint's resolution path over presets; the back button and the right-click context-menu reuse (pointer-event probe result recorded, R3); the window min-floor numbers (compact-usable) and the empirical probe plan for R1 with the fixed-preferred-size fallback decision point; the board-bar height/elide/tooltip change and the add-stack relocation incl. its compact placement; the test catalogue mapping every acceptance item to Tier-1 unit / Tier-2 journey / proptest / manual checklist (no text assertions, a11y labels as addresses). Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): menu, tooltip, navigation and window-stability rework. Do not implement; do not push.

Report: plan path, decisions, risks (esp. R1 probe outcome plan, the menu_path_stays_live_mid_edit reading), estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of GUI review round 1 ("Menu, tooltips, navigation, window stability").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§2–§5, §7, §9, §10, §14; §1 facts for the root causes), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§2, and the stage-1 chunk plan (.artifacts/plans/*ui-review1-menu-tooltips-navigation*-plan.md) in the worktree ../taskboard-WT-feat-ui-review1-menu-tooltips-navigation (branch feat/ui-review1-menu-tooltips-navigation). Recreate the worktree on that branch if missing (run scripts/worktree-doctor.sh from the main checkout first).
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: taskboard-ui-slint stays IO-free and is the one crate allowed unsafe; SPDX header AGPL-3.0-only on every new .rs and .slint file; no sleeps in tests (mock/pause time); no text assertions on labels/descriptions/tooltips — structural facts and typed outcomes only; the registry insta snapshot is regenerated in the same commit as the registry change; do not touch crates/taskboard-domain, taskboard-state, taskboard-storage-sqlite, taskboard-sync-nextcloud; do not enable ActionId::AppSettings and do not start the settings view or the UiRequest lane (chunk 2/3 scope); keymap resolution semantics unchanged (menu_rank is presentation-only).
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo llvm-cov included, then rm -rf target/llvm-cov-target).
4. Record the manual window-size checklist results (view-switch pairs, resize probe) in the worktree as a short section appended to the stage-1 plan file — the stage-3 agent copies the outcome into the PR body.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, the probe outcomes (R1/R2/R3), the manual checklist results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of GUI review round 1 (branch feat/ui-review1-menu-tooltips-navigation).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§2–§5, §7, §9, §10, §14), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§2 (the code-quality mandate applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-review1-menu-tooltips-navigation after rebasing onto origin/main.
2. Functional focus — this chunk's specific traps: (a) the F1 regeneration trap: with the menu open, a second hamburger click must CLOSE, and the next open must show the same entries as the first (entries recomputed from the pre-menu view chain on every open — a regression here silently reintroduces the Menu-context-rows bug); (b) menu_path_stays_live_mid_edit must have been reconciled, not weakened — verify the surviving assertions still guard a real invariant and that the menu-closes-after-mouse-invoke behavior is pinned by some test; (c) the dismissal layer's z-order claim is pinned by a journey, not just comments; (d) the Flickable caveat: confirm the definite-width binding actually bounds the panel at small window sizes (no right-edge overflow, narrow fallback engages); (e) tooltip placement: the property test really feeds adversarial over-wide labels and asserts rect ⊆ window; no clamp formula duplicates the old degenerate form; (f) no binding anywhere sets the Window's width/height; min floors are scale-aware and compact stays usable at them; (g) add-stack affordance reachable in compact mode; (h) keyboard: menu.next/previous/invoke and Escape behave level-by-level in submenus; keymap resolution order untouched by menu_rank; (i) registry snapshot regenerated in the same commits; (j) the cross-crate diff is empty (domain/state/storage/sync); (k) no test asserts label/description/tooltip text; (l) AppSettings is still disabled and no settings/UiRequest code leaked in.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh (include the manual window-size checklist outcome from the stage-1 plan appendix in the body), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green; verify gh pr view --json mergeStateStatus reports CLEAN (BEHIND/DIRTY ⇒ rebase, push --force-with-lease, repeat). main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-review1-menu-tooltips-navigation (everything is pushed; a resuming round recreates the worktree on the branch via git worktree add).

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, merge-state confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — Stack column resize + device-local persistence (`feat/ui-stack-width-persistence`)

- **Covers**: design §6 (D16 widths map in `ViewState`, D17 mouse
  drag affordance with clamps, D18 `stack.wider`/`stack.narrower`
  registry actions + bindings + focusable headers, D19 header elide
  with auto-width preservation, D20 `[ui]`-config persistence home
  with the domain explicitly rejected, D21 `UiRequest` lane:
  mount's second sender, `StackWidthsChanged` full-snapshot variant,
  app-side debounced forwarder merging via `write_config_overlay`
  with stale-id pruning, `UiSettings` load-back). Crates:
  `taskboard-ui-slint` + `taskboard-app` (config). **Non-goals**: no
  domain/state/storage/sync changes (the empty-diff assertion is an
  acceptance item), no horizontal board scroller (#373), no settings
  surface (chunk 3), no `PersistenceAction`/migration/engine command
  (D20 rejected it), no new config file — the overlay merges into
  the existing resolved `taskboard.toml` path.
- **Acceptance**: dragging a column edge resizes live and clamps;
  keyboard actions resize the focused header in steps; resized
  widths survive an app restart (round-trip: write → reload →
  applied); never-resized boards render byte-identically to today
  (auto layout untouched — regression journey); headers elide under
  user width with the full name reachable via tooltip; the overlay
  writer preserves comments and unrelated keys, prunes deleted
  board/stack entries, and is covered by tests under
  `tokio::time::pause`/`advance` (no sleeps); old configs (without
  the new key) still parse; the UI crate performs no IO beyond the
  channel send; cross-crate diff empty; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of GUI review round 1 ("Stack column resize + device-local persistence", branch feat/ui-stack-width-persistence) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition gate: chunk 1's PR (feat/ui-review1-menu-tooltips-navigation) is merged into main — verify with git fetch origin --prune && git log origin/main --oneline -5; if not merged, STOP and report instead of planning.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md — §1 facts F7/F8/F13/F14/F15/F16 (anchored), §6 (D16–D21, normative), §10 (testing), §14 risks R5/R6, and the dispatch plan §1/§3.
2. Read the current seams you will extend: crates/taskboard-ui-slint/src/view_state.rs (ViewState, board selection ownership), src/lib.rs (mount signature, UiSettings), src/adapter.rs (refresh/push, try_send pattern ~134-140), src/actions.rs + src/keymap/presets.rs (registry + bindings for stack.wider/narrower), ui/compositions/board-layout.slint (columns row, header RoleText ~115-120), ui/compositions/stack-view.slint (compact), and on the app side crates/taskboard-app/src/config.rs (UiSection ~342-351, deny_unknown_fields, tests), src/ui.rs (ui_command_channel/spawn_ui_command_forwarder/resolve_ui_settings), src/commands/connect.rs (write_config_overlay ~523 — reuse, do not duplicate), plus the config and app test suites you must not break.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-ui-stack-width-persistence on a new branch feat/ui-stack-width-persistence off the freshly fetched origin/main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-stack-width-persistence-plan.md settling concretely: the ViewState widths-map shape and its load/apply path from UiSettings; the drag affordance geometry/cursor decision (probe pointer-cursor support on the pinned Slint version, record it) and the clamp bounds; the two new registry actions' contexts, bindings in all three presets, and step size; the header elide rule that provably preserves today's auto layout for never-resized boards; the UiRequest enum placement and the exact mount signature change; the forwarder task design — debounce interval via injected time, single-owner overlay-write rule (R6: never hold a lock across .await; the REPL ceremonies' own writes are accounted for), the merge+prune algorithm over write_config_overlay, and the shared_state read used for pruning; the UiSection.stack_widths serde shape (defaults, uuid keys, px values) and the deny_unknown_fields forward-compat note; the failure story when the config path is unwritable (log + keep UI state, never crash the UI); the full test catalogue (config round-trips, overlay merge preserves comments/unrelated keys, prune, debounce under tokio::time::pause, journey: drag→release→persisted→restart-applied, journey: auto boards unchanged, keyboard resize). Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): stack column resize with device-local persistence. Do not implement; do not push.

Report: plan path, decisions, risks (esp. overlay writer contention and auto-layout regression), estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of GUI review round 1 ("Stack column resize + device-local persistence").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§6, §10, §14 R5/R6; §1 facts F7/F8/F13-F16), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§3, and the stage-1 chunk plan (.artifacts/plans/*ui-stack-width-persistence*-plan.md) in the worktree ../taskboard-WT-feat-ui-stack-width-persistence (branch feat/ui-stack-width-persistence). Recreate the worktree on that branch if missing (run scripts/worktree-doctor.sh from the main checkout first).
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: no changes in crates/taskboard-domain, taskboard-state, taskboard-storage-sqlite, taskboard-sync-nextcloud (the diff over them must be empty); taskboard-ui-slint stays IO-free — the only new UI-side capability is sending on the provided channel handle; SPDX headers (MIT OR Apache-2.0 for any new app-crate file, AGPL-3.0-only for UI-crate files); no sleeps — the debounce is tested under tokio::time::pause/advance; no text assertions; the registry snapshot is regenerated in the same commit as the registry change; secrets/config invariants: the overlay write goes through the existing comment-preserving writer, never a bare serde re-serialize of the whole file.
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo llvm-cov included, then rm -rf target/llvm-cov-target).
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of GUI review round 1 (branch feat/ui-stack-width-persistence).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§6, §10, §14 R5/R6), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§3 (the code-quality mandate applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-stack-width-persistence after rebasing onto origin/main.
2. Functional focus — this chunk's specific traps: (a) the cross-crate diff is empty — any PersistenceAction/migration/engine change is a design violation (D20), not a shortcut; (b) auto-layout preservation: a board whose stacks were never resized must render exactly as before — find the journey that pins this and check it would fail if elide/width code leaked into the auto path; (c) the overlay writer: comments and unrelated keys preserved (find the test asserting a hand-written config with comments round-trips through a widths save), stale-id pruning cannot delete unrelated tables or live entries, and concurrent-looking writes are serialized by the single-owner rule without holding anything across .await; (d) debounce determinism: no real sleeps anywhere in the test suite; the quiet-window logic is advanced with tokio::time; (e) clamp bounds and the keyboard step are pure, unit-tested math; (f) UiSettings load-back applies widths before first render of a board (no flash of auto layout is acceptable to assert either way — but the load path must be a test, not folklore); (g) the UI crate gained no IO capability beyond the channel send; (h) restart round-trip test proves persistence end-to-end at the app layer; (i) no text assertions.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green; verify mergeStateStatus CLEAN as in §1. main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-stack-width-persistence.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, merge-state confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — Settings view + sync connection management (`feat/ui-settings-sync-connections`)

- **Covers**: design §8 (D24b settings scope: connections list with
  live status, add/edit/remove/test, keymap preset, UI scale; the
  surface sits outside the view cycle; D25b request-lane extension
  with oneshot replies per the AGENTS §3 query pattern + extraction
  of the connect/logout ceremony core into shared app-layer
  functions; D26b write-only secret handling through the configured
  `CredentialStore`; D27b live-apply semantics; D28b enable
  `ActionId::AppSettings`, `UiEffect::OpenSettings`, `Ctrl+,`,
  Escape/back). Crates: `taskboard-ui-slint` + `taskboard-app`
  (`commands/connect.rs` refactor, new request handling). **Non-goals**:
  no sync-actor or sync-crate changes (attach/detach already
  exist), no free-form `taskboard.toml` editor (#351), no new
  credential store backends, no domain/state/storage changes (empty
  diff asserted), no restart-requiring settings.
- **Acceptance**: settings opens from the menu row and `Ctrl+,`,
  never appears in the next/prev-view cycle, and returns to the
  previous view via back/Escape; the connector list reflects
  configured connectors with live status; add/edit/remove flows write
  through the ceremony (config overlay + attach/detach) and are
  observable in-app without restart; the test button returns a typed
  result via the reply lane; secrets never appear in the written
  config or in logs; keymap preset and scale apply live and persist;
  REPL `connect`/`logout` E2E suites green unchanged (extraction did
  not rewrite behavior); `AppSettings` enabled with router arm and
  menu row live; legacy `[nextcloud]` singleton configs are rejected
  by the settings save path with a typed error, not silently
  migrated; cross-crate diff empty; gates green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of GUI review round 1 ("Settings view + sync connection management", branch feat/ui-settings-sync-connections) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition gate: chunk 2's PR (feat/ui-stack-width-persistence) is merged into main — verify with git fetch origin --prune && git log origin/main --oneline -5; if not merged, STOP and report instead of planning.

1. Read AGENTS.md, then the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md — §1 facts F12–F17 (anchored; esp. F17: every connection-management primitive already exists), §8 (D24b–D28b, normative), §10 (testing), §14 risks R4/R5, and the dispatch plan §1/§4.
2. Read the current seams you will extend: crates/taskboard-app/src/commands/connect.rs (the connect/logout ceremonies, verify ~394-404, write_config_overlay ~523 — the extraction target), src/bootstrap.rs (attach_connector ~314-391, detach ~205/265), src/config.rs (ConnectorCfg ~242-253, legacy singleton exclusion ~627-633, CredentialStore ~141-155, UiSection), src/ui.rs (forwarders, resolve_ui_settings — extend for the request lane from chunk 2), and in the UI crate src/lib.rs + src/adapter.rs (the chunk-2 UiRequest lane and the timer drains — replies must ride a non-blocking UI-side path), src/view_state.rs (settings surface placement outside the cycle), src/actions.rs + src/router.rs + src/keymap/presets.rs (AppSettings enablement, UiEffect::OpenSettings, Ctrl+,), src/models.rs (connector status push), ui/views/ + ui/widgets/ (text-field for forms), plus the connect/logout E2E suites you must not break.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit: STOP and report), then create the worktree ../taskboard-WT-feat-ui-settings-sync-connections on a new branch feat/ui-settings-sync-connections off the freshly fetched origin/main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-settings-sync-connections-plan.md settling concretely: the ceremony extraction boundary (which functions move where, the REPL call sites, why behavior is byte-stable — R4); the UiRequest settings variants with oneshot replies and the UI-side non-blocking reply drain (Slint-timer polling pattern, no blocking recv on the UI thread, no tokio in the UI crate); the settings surface placement in ViewState and its entry/exit choreography (menu row, Ctrl+,, back/Escape, outside the next/prev cycle); the connections form model (fields, validation mirrored from config.rs, the write-only secret path into CredentialStore, the legacy-singleton typed rejection); the live status projection (SyncPhase + ConnectorLost/Restored into the list rows); the keymap-preset and scale live-apply + persist flows; the exact D28b enablement set; the test catalogue (ceremony extraction equivalence, request/reply round trips, config overlay through the settings path, secret never in written config/logs — structural assertions, form validation, view-cycle exclusion journey, E2E suites unchanged). Resolve every open question with a recorded decision; deviations from the design round need a recorded reason.
5. Commit it as: docs(plan): settings view with sync connection management. Do not implement; do not push.

Report: plan path, decisions, risks (esp. the extraction boundary and the reply-lane threading), estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of GUI review round 1 ("Settings view + sync connection management").

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§8, §10, §14 R4/R5; §1 facts F12–F17), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§4, and the stage-1 chunk plan (.artifacts/plans/*ui-settings-sync-connections*-plan.md) in the worktree ../taskboard-WT-feat-ui-settings-sync-connections (branch feat/ui-settings-sync-connections). Recreate the worktree on that branch if missing (run scripts/worktree-doctor.sh from the main checkout first).
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: extract the ceremonies, do not rewrite them — the REPL connect/logout E2E suites must pass unchanged; no changes in crates/taskboard-domain, taskboard-state, taskboard-storage-sqlite, taskboard-sync-nextcloud; the UI crate stays IO-free apart from channel handles, uses no tokio APIs beyond the channel/oneshot handles it already receives, and never blocks the UI thread; secrets are write-only in the form and never serialized into config or logs; SPDX headers per crate (app: AGPL-3.0-only; UI: AGPL-3.0-only); no sleeps in tests; no text assertions; registry snapshot regenerated in the same commit as the registry change.
3. Run the full gate suite from AGENTS.md §10 and make it green (cargo llvm-cov included, then rm -rf target/llvm-cov-target).
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of GUI review round 1 (branch feat/ui-settings-sync-connections).

1. Read AGENTS.md, the design round .artifacts/plans/2026-10-22-gui-review-round1-plan.md (§8, §10, §14 R4/R5), the dispatch plan .artifacts/plans/2026-10-22-gui-review-round1-implementation-dispatch-plan.md §1/§4 (the code-quality mandate applies to you), and the stage-1 chunk plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-feat-ui-settings-sync-connections after rebasing onto origin/main.
2. Functional focus — this chunk's specific traps: (a) the ceremony extraction: diff the moved code mentally against its origin — any behavior change in verify/overlay-write/attach/detach ordering must be a recorded decision, not drift; the REPL E2E suites are the gate and must pass UNCHANGED; (b) secrets: trace the token from form field to CredentialStore and prove (test + reading) it never reaches taskboard.toml, a log line, or a UI echo; (c) the reply lane: oneshot-in-mpsc per AGENTS §3, dropped-reply and closed-channel cases typed and non-blocking; the UI never blocks; (d) the settings surface is unreachable via nav.next-view/nav.prev-view and always returns to the exact previous view; (e) AppSettings enablement is complete: router arm, effect, menu row live, Ctrl+, bound in the standard preset — and nothing else got enabled; (f) legacy [nextcloud] singleton configs hit a typed rejection in the save path; (g) cross-crate diff empty; no sync-actor changes; (h) no text assertions; (i) the registry snapshot matches the registry change commits.
3. Code-quality mandate: spell it out in full from §1 of this dispatch plan and apply it.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green; verify mergeStateStatus CLEAN as in §1. main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-settings-sync-connections.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, merge-state confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 5. Completion bookkeeping

- **Backlog closures**: none — none of the user's 2026-10-22
  findings map to existing backlog issues; this design round is their
  home. #351 (GUI settings editor) is only **annotated** by chunk 3's
  PR (settings v1 is deliberately narrower); it stays open.
- **New issues to file after the design round merges** (one issue per
  idea, `backlog:` title prefix, category labels per AGENTS §7, body
  = summary + permalink to
  `.artifacts/plans/2026-10-22-gui-review-round1-plan.md` §13):
  board-selector dropdown/popup for many boards (deferred by D23);
  window-geometry persistence across restarts (outside D15's
  within-session scope).
- **Deliberately not dispatched here** (with their owners): quick
  capture implementation (#347), undo/redo enablement (#356, UI
  picker #353), free-form toml editing (#351), horizontal board
  scrolling / ListView virtualisation (#373), pointer drag-to-move
  (#226), kiosk ambient rotation (#363).
- **What opens next**: the user's next GUI review session ("many
  more such sessions") starts a round-2 design round; chunk merges
  from this round are its baseline.
