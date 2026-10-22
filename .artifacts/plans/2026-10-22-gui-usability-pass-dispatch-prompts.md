# [Dispatch Prompts] GUI Usability Pass — Three-Stage Agent Prompts

- **Date**: 2026-10-22
- **Kind**: dispatch prompts — the standard three-stage agent protocol
  (planning → implementation → review/evaluation) for **one chunk**:
  the GUI usability pass. This is deliberately **not** a `[Plan]`
  dispatch plan under `.artifacts/templates/dispatch-plan.md` (that
  form chunks an already-accepted design round); no design round has
  happened yet — the stage-1 planning agent **is** the design step
  and writes the accepted plan.
- **Branch**: `feat/ui-usability-pass` — one branch, one PR. The six
  issues form one coherent deliverable, entirely inside
  `taskboard-ui-slint` (+ its tests).
- **Worktree**: `../taskboard-WT-feat-ui-usability-pass` (house
  convention: branch slash → dash), created by stage 1 after
  `scripts/worktree-doctor.sh` passes, removed by stage 3.
- **Origin**: the user's GUI bug report of 2026-10-22 (§1 below);
  normative design origins cited inside the prompts.
- **Status**: READY — no gating merges; nothing else dispatched.

## 1. The reported issues (binding problem list, user 2026-10-22)

1. Almost no elements change under mouse-hover, and in those that do,
   the change is difficult to perceive.
2. There is no hovertext explaining any elements or what they do.
3. The hamburger menu's icon, instead of being inside the traditional
   square, scales to the left until it has taken up almost half the
   horizontal space.
4. The main menu is very long — a flat list that functions were thrown
   into willy-nilly. It needs organisation and sensible sub-menus.
5. The main menu is so long that on the test kiosk netbook's small
   screen it gets cut off at the bottom. Menu boxes must detect how
   much space is available, fill only the available space, and provide
   a scroll area when that space is insufficient for their contents —
   never be cut off.
6. There is no button to create a new stack; the feature is only
   reachable through the main menu. On a new, empty board adding a
   stack is a basic operation every user performs and needs an
   on-board button.

## 2. Facts established on `main` (2026-10-22 reconnaissance, seeds — verify, don't trust)

- **Invisible hover root cause (issues 1)**: the only `has-hover` in
  the entire UI tree is `ui/widgets/icon-button.slint:27`, and its
  hover background is `Theme.surface` — the same colour as the toolbar
  background behind it (`ui/compositions/toolbar.slint`), so the one
  hover that exists is literally invisible. `ui/theme.slint`'s palette
  has no dedicated hover token at all.
- **Hamburger stretch (issue 3)**: in `toolbar.slint` the title text
  carries `horizontal-stretch: 1` while the `IconButton` pins only
  `min-width: Theme.space-6` (no max/width pin) — layout-stretch
  suspect; reproduce headlessly before fixing (AGENTS §5: a failing
  new test may be a production bug or a test bug — evaluate both).
- **Menu cut-off (issue 5)**: `ui/compositions/action-menu.slint` has
  `min-height: 240px` but no max-height and no scroll; the menu is
  hosted inline in `ui/app-window.slint` under
  `if (KeyboardState.menu-open)`. Kiosk netbook (report
  `2026-10-21-netbook-kiosk-test-environment-report.md`): 1024×600
  LVDS panel, winit guessed scale 1.25 → ≈480 logical px of height.
- **Flat menu (issue 4)**: `src/menu.rs` is a pure projection of
  `REGISTRY` (`src/actions.rs`) sorted by `Category` then registry
  order — the IA fix must keep menus as projections of data, never
  hand-written rows (gui-phase7 design §5.9), and keyboard-operable
  (§7.1 click ≡ action, §7.3 journeys).
- **No add-stack affordance (issue 6)**: nothing in
  `ui/compositions/board-bar.slint`, `ui/views/board-view.slint`, or
  `ui/widgets/empty-state.slint`; the create-stack action exists only
  in the registry/menu path.
- **Test surface to build on** (docs/testing_strategy.org UI tiers):
  headless platform `src/test_platform.rs` (software renderer,
  `set_size` gives real geometry — layout is testable headlessly);
  Tier-2 journeys `tests/journeys.rs` (driver `tests/support/driver.rs`,
  `fake_engine.rs`); Tier-3 goldens `tests/golden_visuals.rs` (scale
  {0.5, 3.0} × breakpoints {600, 1000, 1500} at height 900; regen =
  `TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint
  --test golden_visuals`, which then FAILS the run by design; the PR's
  PNG diff is the review artefact); token lint + contrast in
  `src/token_tests.rs`; pin guard `tests/pin_guard.rs`.

## 3. Prompt 1 — stage-1 PLANNING agent

```text
You are the stage-1 PLANNING agent for the GUI usability pass in the taskboard repo at /home/viktoria/src/taskboard (branch feat/ui-usability-pass, worktree ../taskboard-WT-feat-ui-usability-pass).

1. Read AGENTS.md first. Then read the dispatch-prompts artefact /home/viktoria/src/taskboard/.artifacts/plans/2026-10-22-gui-usability-pass-dispatch-prompts.md — §1 is the binding six-issue problem list, §2 the reconnaissance facts (verify them; do not trust). Normative origins: .artifacts/plans/2026-10-14-gui-phase7-design-plan.md §3.1 (theme is the only place colours/dimensions live), §5.9 (generated menu — projections of data, never hand-written), §6.1 (base-scale), §6.3–6.4 (responsive breakpoints + degeneration), §7.1 (every click affordance has an action equivalent), §7.3 (keyboard journeys), §10 (golden visuals); .artifacts/plans/2026-10-15-ui-action-keymap-plan.md (action registry, Category); docs/testing_strategy.org (the UI test tiers); .artifacts/reports/2026-10-21-netbook-kiosk-test-environment-report.md (the kiosk netbook: 1024×600 panel at winit scale guess 1.25 → ≈480 logical px of height — the cut-off target).

2. Read the current seams you will extend, all in crates/taskboard-ui-slint: ui/theme.slint (palette — no hover token exists), ui/widgets/icon-button.slint (the only has-hover in the tree; its hover background equals the toolbar's background colour — the invisible-hover root cause), ui/compositions/toolbar.slint (menu-button host; title has horizontal-stretch 1 while IconButton pins only min-width — the hamburger-stretch suspect), ui/compositions/action-menu.slint (min-height 240px, no max-height, no scroll), ui/app-window.slint (menu host under if (KeyboardState.menu-open)), src/menu.rs + src/actions.rs (flat Category-ordered projection), ui/compositions/board-bar.slint + ui/views/board-view.slint + ui/widgets/empty-state.slint (no add-stack affordance), src/responsive.rs + ui/responsive.slint; and the tests you must not break: tests/journeys.rs, tests/golden_visuals.rs, tests/keymap_snapshots.rs, src/token_tests.rs, tests/pin_guard.rs, src/test_platform.rs (headless software renderer; the set_size resize path makes real layout testable with no display).

3. Diagnose the hamburger-stretch bug first: reproduce it in a headless component test at the failing geometry via the test_platform resize path, and record the diagnosed root cause in the plan. Per AGENTS §5, a newly failing test may be a production bug or a test bug — evaluate both before deciding what to change.

4. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first — nonzero exit means free disk is below the floor: STOP and report instead of planning. Then create the worktree: git worktree add ../taskboard-WT-feat-ui-usability-pass -b feat/ui-usability-pass main (off the then-current main).

5. Write a detailed TDD-first plan at .artifacts/plans/2026-10-22-gui-usability-pass-plan.md (inside the worktree) in the house [Plan] style, and copy the dispatch-prompts artefact alongside it. The plan must settle, each with a recorded decision: (a) hover semantics — new palette/semantic token(s) whose hover result is distinguishable from BOTH Theme.surface and Theme.surface-raised parents, an inventory of every interactive element that must respond, and re-running the contrast check in src/token_tests.rs; (b) the tooltip mechanism — Slint has no native tooltip, so decide the component (delayed hover reveal), placement, dismissal, and the text source (must be the same labels the a11y layer uses; tooltips never carry information unavailable elsewhere; keyboard/touch story); (c) the hamburger fix per your step-3 diagnosis, verified at scale extremes; (d) menu information architecture — section grouping or submenus, their mapping from Category (extend it if needed), keeping the menu a pure registry/keymap projection (§5.9) and fully keyboard-operable (§7.1/§7.3), with keymap presets' generated hints still correct; (e) menu sizing — max-height bound to available space under the toolbar, scroll area when content exceeds it, proven at 1024×600 with ≈480 logical px and at the golden matrix's scale extremes; (f) the add-stack button — placement (board bar and/or empty state on an empty board), routed through the same registry action as its keyboard equivalent (§7.1), accessible-role/label per D15/D16; (g) the test catalogue — which journeys extend, which goldens change (regen protocol: TASKBOARD_REGEN_GOLDENS=1 ... --test golden_visuals; the regen run fails afterwards by design; the PNG diff is the PR review artefact), token-lint/contrast impact. Record explicit non-goals (e.g. no theme switching machinery, no icon font, no domain/state-crate changes) and any deviation from the origins with a reason.

6. Commit the plan + artefact as: docs(plan): gui usability pass design and TDD plan. Do not implement; do not push.

Report: plan path, the decisions taken, the diagnosed hamburger root cause, risks, estimated diff size.
```

## 4. Prompt 2 — stage-2 IMPLEMENTATION agent

```text
You are the stage-2 IMPLEMENTATION agent for the GUI usability pass (branch feat/ui-usability-pass).

1. Read AGENTS.md, the dispatch-prompts artefact .artifacts/plans/2026-10-22-gui-usability-pass-dispatch-prompts.md (§1 problem list = acceptance criteria, §2 facts) and the normative origins it cites, and the stage-1 plan .artifacts/plans/2026-10-22-gui-usability-pass-plan.md — all in the worktree ../taskboard-WT-feat-ui-usability-pass (recreate it on the branch if missing: git worktree add ../taskboard-WT-feat-ui-usability-pass feat/ui-usability-pass).

2. Implement exactly per the approved plan, TDD where code is involved (failing headless tests first — journeys and component tests; goldens last), every commit standalone (rebase-merge hygiene: correct build, meaningful <type>(<module>) subject, no WIP noise). Chunk-specific hard rules:
   - Presentation only: no business logic in the UI layer; compositions stay layout-only (Rule E); menus stay projections of registry/keymap data — never hand-written rows (§5.9).
   - Colours and dimensions only via Theme/Semantic tokens (token lint R1); new tokens go through the palette → semantic layer and the contrast check in src/token_tests.rs.
   - Every click affordance routes the same registry action as its keyboard equivalent (§7.1); new interactive elements carry accessible-role and accessible-label (D15/D16).
   - Tests: headless only via src/test_platform.rs; no sleeps (mock time only); typed/structural assertions — never error/log/UI-label text (AGENTS §5); one behaviour, one test, lowest layer; no quality ceilings.
   - Golden regen is the explicit protocol: TASKBOARD_REGEN_GOLDENS=1 cargo nextest run -p taskboard-ui-slint --test golden_visuals (that run fails afterwards by design; CI compares only) — never hand-edit PNGs; record which goldens changed and why.
   - New .rs/.slint files carry the // SPDX-License-Identifier: AGPL-3.0-only header.
   - Do not touch version pins or [workspace.dependencies] (tests/pin_guard.rs guards the slint/i-slint-backend-testing pair).

3. Run the full gate suite from AGENTS.md §10 and make it green.

4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, goldens changed, any deviation from the plan and why.
```

## 5. Prompt 3 — stage-3 REVIEW / evaluation agent

```text
You are the stage-3 REVIEW agent for the GUI usability pass (branch feat/ui-usability-pass). Your review is the user's acceptance pass: evaluate what they will see and experience, not just what compiles.

1. Begin by rebasing onto main, in the worktree ../taskboard-WT-feat-ui-usability-pass (recreate it on the branch if missing): git fetch origin --prune; rebase the branch onto origin/main (drop already-merged duplicate commits per AGENTS §8's rebase dance; resolve only real conflicts); push --force-with-lease if history was rewritten.

2. Read AGENTS.md, the dispatch-prompts artefact .artifacts/plans/2026-10-22-gui-usability-pass-dispatch-prompts.md (§1 is the acceptance criteria), the normative origins it cites, and the stage-1 plan; then review the full branch diff versus the post-rebase base (git diff origin/main...HEAD).

3. Functional focus — this chunk's specific traps:
   - Hover: EVERY interactive element responds perceptibly; the hover token is distinct from both Theme.surface and Theme.surface-raised (the old bug was hover == parent toolbar background); no information is reachable only via hover.
   - Tooltips: delayed, dismissible, never obscuring their target; text sourced from the same labels the a11y layer uses; keyboard journeys still pass unchanged.
   - Hamburger: the fix holds at 1024×600 AND at scale extremes 0.5/3.0 — check the goldens actually cover the failing geometry, not just the golden matrix defaults.
   - Menu IA: sections/submenus remain a pure projection (src/menu.rs + src/actions.rs); keymap presets still generate correct hints (tests/keymap_snapshots.rs); keyboard navigation through the new structure is covered by journeys.
   - Menu sizing: no cut-off at the kiosk netbook's reality — 1024×600 at winit scale 1.25 (≈480 logical px) — proven by a headless set_size component test exercising the scroll path; menus never render beyond available space.
   - Add-stack: present and functional on an empty board; routed through the registry action with journey coverage of the click ≡ action equivalence; does not duplicate the menu entry's semantics.
   - The four UI test tiers still hold: no weakened or ceilinged tests (AGENTS §5), the golden PNG diff reviewed as images (it IS the review artefact), token lint + contrast green, pin guard untouched.

4. Code-quality mandate (binding): beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — taskboard-ui-slint stays presentation-only with no IO-crate dependencies, compositions stay layout-only — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.

5. Re-run the full gate suite yourself (AGENTS.md §10); any failure is a finding.

6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).

7. Ship: push the branch, open the PR to main with gh (body: the six issues and the golden-PNG diff note), then gh pr checks --watch to completion — any failing check is a finding: diagnose, fix, push --force-with-lease, repeat until green. Then verify the merge state: gh pr view --json mergeStateStatus must report CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat; any other state ⇒ diagnose before handing off. main is branch-protected: never merge; the user merges on GitHub.

8. Teardown: from the project root /home/viktoria/src/taskboard, git worktree remove --force ../taskboard-WT-feat-ui-usability-pass (everything precious is pushed; a resuming round recreates the worktree via git worktree add ../taskboard-WT-feat-ui-usability-pass feat/ui-usability-pass).

Report: a verdict against each of the six issues, findings (functional + mandate), what you fixed, the PR URL, green-checks + mergeStateStatus confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 6. Handoff & bookkeeping

- Stage 3's handoff contract: **PR open + checks green + merge state
  `CLEAN` + worktree removed**; the user merges on GitHub.
- The stage-1 plan lands on the branch via `docs(plan)`; this
  artefact travels with it (copied into the worktree in prompt 1 §5).
- Deliberately not dispatched here: any state-engine/storage/domain
  work the planning agent might discover a need for goes to the
  backlog or a follow-up round — this chunk is UI-crate only.
