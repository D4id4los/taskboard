# [Plan] The board dead-zone click-out — implementation chunk plan (#1543, #1490)

- **Date**: 2026-11-27
- **Kind**: dispatch-plan-style implementation chunk plan — one chunk,
  three stages (this design round → implementation → review), on the
  existing branch `fix/ui-focus-remount-wrap` (the click-out rides the
  #1490 PR; the operator ruled the PR merges only with the click-out
  landed). No code in this commit.
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-27-board-click-out-ux-plan.md` (normative:
  §0 rulings R-1–R-4, §2 matrix, §3 stories US-1/US-2/US-3, §4 journey
  specs, §6 wiring inventory W-1–W-4, §7 command-contract delta, §8
  verification plan), issue #1543 (the evidence), issue #1419 (the
  clip-focus law, option A), the wrap chunk plan
  `.artifacts/plans/2026-11-27-focus-remount-wrap-plan.md` (the
  invariants this chunk must not break), and the operator ruling of
  2026-11-27 recorded on #1490 (option (a)).
- **Status**: READY — dispatchable once the dispatcher accepts the UX
  plan. Not dispatched here: nothing; the chunk is one stage-2 + one
  stage-3 session on the existing worktree.

## 1. Recorded reality (branch `fix/ui-focus-remount-wrap`, rebased 2026-11-27 onto origin/main; wrap `d741b4e0`, mechanism `ca69959f`)

- **The red**: `a_dead_zone_click_settles_the_create_flow`
  `crates/taskboard-ui-slint/tests/journeys.rs:12669`. Its three legs:
  empty-buffer click at `(10, 450)` discards + normal mode returns +
  `assert_commands(&[])` + re-arm (AC-1/AC-3); typed-buffer click
  commits exactly one `StateCommand::CreateTask` and a subsequent
  Return dispatches nothing (AC-2). **Assertions stay byte-identical —
  this is the R-1 ruling's oracle, never weakened.**
- **The handler-less dead zone**: `board-layout.slint:202-211` — the
  full-board `DropArea` (`width/height: 100%`, declared first = lowest
  z) handles `can-drop` (drag ghost tracking, r2 chunk-8 D-D) and has
  NO click handler; a `DropArea` consumes nothing on plain clicks —
  the click reaches no seam at all (#1543's instrumented evidence).
- **The Flickable**: `board-layout.slint:329` `columns-row :=
  Flickable` wraps `columns := HorizontalLayout` (line 330); the
  per-column create field (the `Flow::Create` field) mounts INSIDE the
  wrapped subtree — the #1490 mechanism's subject. The Flickable's pan
  handling consumes presses over the columns row's empty regions
  (`FORWARD_DELAY` pan grab), so a board-level layer behind the
  Flickable cannot own in-viewport dead space → the two-layer design
  (UX plan R-3).
- **The gating feed already exists**: `board-layout.slint:177`
  `in property <bool> flow-open`, bound at `app-window.slint:716` from
  `board-flow-open` (declared `app-window.slint:593`), pushed
  flow-generically at `models.rs:2414` (`window.set_board_flow_open(flow
  != Flow::None)`). No new presentation state is needed. Precedent
  consumers: `allow-move: !root.flow-open` (board-layout.slint:631,
  :1017 — the D9 XOR belt).
- **The settle funnel**: `Adapter::settle_flow`
  (`src/adapter/walk.rs:40`) — every settle trigger funnels here; it
  early-returns on `Flow::None` (walk.rs:63), so a flowless click is a
  no-op at the adapter with zero new logic. The pure rule is
  `ViewState::settle_by_rule` (`src/view_state.rs:4380`): TEXT flows
  commit a trimmed non-empty buffer, discard an empty one — the R-1
  semantics, already implemented and pinned.
- **The seam precedent**: pointer seams call `settle_flow` directly
  (`src/adapter/seams/drag.rs:78,127,132,170,175,211` — the L2 pointer
  seams of the D-H4 taxonomy); the `[try]` install shape
  (`install_seam! ... [try]`, exemplar `src/adapter/seams/task.rs:32`)
  absorbs the settle's own teardown `focus-lost` echo
  (`try_borrow_mut` discipline, task.rs:26-31). The new click seam
  copies exactly this shape.
- **The #1490 mechanism (invariants to keep intact)**:
  `src/adapter/seams/task.rs:33-77` — the echo-window absorb + the
  narrowed flow-aware classification (`Flow::Create { .. } | Flow::None`
  absorbed; every other armed flow's first loss settles genuinely —
  the stop-inventory journey pins it); the consumed-once re-grab arm
  (`refocus_create_armed`, `src/adapter/mod.rs:408`, disarmed at every
  genuine settle — task.rs:79-81 and mod.rs:979-1047); the
  `TryRefocus.create-nonce` global consumed edge-triggered by the
  mounted field. **The click-out must not touch any of this.**
- **The two protected pins** (assertions byte-identical, in-crate):
  `insert_intent_opens_the_create_row_and_escape_returns`
  (`src/lib.rs:2996`) and `editing_dispatches_nothing_and_exits_via_escape`
  (`src/lib.rs:3493`).
- **The wrap's other green**: the columns seek probe
  (`tests/columns_seek_probe.slint`,
  `tests/probes/columns_seek_probe_tests.rs`) and the clip law it
  pins — stays green, untouched.
- **The driver**: `click_at` (`tests/support/driver.rs:265`) dispatches
  PointerMoved + PointerPressed + PointerReleased at a logical
  position — a stationary click, exactly the gesture the seam must
  own; `press_at`/`drag_move_to`/`release_at` (driver.rs:285+) exist
  for a drag-vs-click probe if E1 needs one.
- **The outside-click dismissal-layer precedent** (z-order pinned by
  behavior, not comments): `an_outside_click_dismisses_the_menu`
  (`tests/journeys.rs:452`). NOTE: the app-menu layer is a
  full-window layer that STEALS aimed clicks deliberately; the click-out
  must NOT adopt that shape (the no-fire law, UX plan US-2) — the
  precedent is for the journey/test style, not the layer geometry.

## 2. Decisions (all settled; the UX plan's §0 rulings are normative)

- **D-1 (owner, R-3)**: two gated layers, one slint callback
  `board-dead-zone-clicked`, one adapter seam
  `on_board_dead_zone_clicked`:
  - **W-1 (in-viewport)**: a `TouchArea` declared INSIDE
    `columns-row := Flickable` immediately BEFORE `columns := HorizontalLayout`
    (declared-first = hit-tested last = beneath every column), filling
    the viewport, `enabled: root.flow-open`,
    `clicked => root.board-dead-zone-clicked();`. This layer owns the
    journey's `(10, 450)` coordinate (left padding region of the
    columns row).
  - **W-2 (board-level)**: a `TouchArea` declared at board level
    beside the dead-zone `DropArea` (board-layout.slint ~202, still in
    the lowest-z block), same gating + callback — owns the board
    background OUTSIDE the viewport (the empty-board CreateStack leg
    US-3/S1, the margins below the row).
- **D-2 (semantics, R-1)**: the seam body calls
  `a.settle_flow(&window, &state)` — the same funnel the Escape
  resolution and the drag seams land in; `settle_by_rule` owns
  commit-vs-discard. **No new ActionId, no new StateCommand, no new
  UiRequest** (UX plan §7) — the click-out is an L2 pointer seam.
- **D-3 (flowless click, R-4)**: no seam-side flow check at all —
  `settle_flow`'s own `Flow::None` early-return IS the adapter-owned
  decision. The UI never branches on flow content.
- **D-4 (gating, R-2)**: both layers `enabled: root.flow-open`. When
  no flow is live the layers are disabled → hit-testing skips them →
  pan, card clicks, and every existing behavior are untouched. While
  a flow is live, a stationary press+release on dead space fires
  `clicked`; a press that pans is consumed by the Flickable's native
  gesture (no new threshold; TouchArea pan-start loss while a text
  flow is modal-ish is the recorded acceptable cost).
- **D-5 (echo safety)**: the seam installs `[try]` — the settle's
  teardown `focus-lost` echo is absorbed by the borrow discipline
  exactly as at every other settle seam (task.rs:26-31); the journey's
  third leg (`Return` after the typed click-out dispatches nothing)
  is its oracle.
- **D-6 (E1 experiment, ≤30 min, FIRST)**: before writing the three
  new journeys' assertions, verify with a scratch probe (never
  committed) that (a) the W-1 layer receives `click_at(10, 450)` while
  the flow is armed, and (b) the W-2 layer receives an outside-viewport
  click while the empty-board stack-create flow is armed. If the
  Flickable still consumes in-viewport stationary clicks past the W-1
  layer, the recorded fallback is: move W-1 inside the Flickable's
  viewport as a child declared AFTER `columns` but with
  `z: -1`-equivalent stacking is NOT available in slint — instead
  hoist W-1 to wrap the columns layout content region via the
  viewport's own padding geometry (a full-size TouchArea as the
  Flickable's first child IS the primary shape; the fallback is a
  `TouchArea` co-located per-column beneath each `drop-zone :=
  DropArea`, same callback — three bisected compile attempts' worth of
  evidence says try the primary shape first and only fall back with
  recorded evidence). Record the E1 outcome in the stage-2 report.
- **D-7 (no-fire tests read structure, not labels)**: US-2/S1 clicks
  the field itself and asserts the flow stays open +
  `assert_commands(&[])`; US-2/S2 clicks dead space with no flow and
  asserts `assert_commands(&[])`. No text assertions anywhere (AGENTS
  §5).

## 3. TDD order

1. **The red already exists**: `a_dead_zone_click_settles_the_create_flow`
   (journeys.rs:12669) — failing on the branch per #1490's evidence;
   do not touch it.
2. **Commit 1 (test)**: add the three journeys from UX plan §8 —
   `us2_s1_click_on_the_field_itself_does_not_settle`,
   `us2_s2_flowless_dead_zone_click_dispatches_nothing`,
   `us3_s1_board_level_click_out_settles_stack_create` (all in
   `crates/taskboard-ui-slint/tests/journeys.rs`, next to the red
   journey; step tokens embedded in the names). Run after D-6's E1:
   these are red for the right reason (no callback exists to wire —
   compile error is an acceptable red for slint seams; the red to
   record before implementing is each test failing on the missing
   route, never on a fixture defect).
3. **Commit 2 (slint)**: the W-1 + W-2 layers and the
   `board-dead-zone-clicked` callback (board-layout.slint only).
4. **Commit 3 (adapter)**: `on_board_dead_zone_clicked` in
   `src/adapter/seams/board.rs`, the `[try]` settle shape; run the
   crate gate — all four journeys green, the two pins untouched,
   the seek probe green.
5. Every commit standalone (rebase-merge hygiene): a correct build,
   `<type>(<module>): <short-desc>` + body. Suggested subjects:
   `test(ui): the click-out's no-fire and board-level journeys (#1543)`,
   `feat(ui): the gated dead-zone click layers (#1543)`,
   `feat(ui): the dead-zone click seam settles through the one funnel (#1543)`.

## 4. Invariants (a stage-3 violation of any is a BLOCKER)

1. `a_dead_zone_click_settles_the_create_flow` byte-identical in its
   assertions; it passes for the real mechanism, never vacuously
   (litmus: with the click route deleted, it must fail).
2. The #1490 mechanism intact: the seams/task.rs absorb + narrowed
   classification (`Flow::Create { .. } | Flow::None`) unmodified; the
   consumed-once law (`refocus_create_armed` disarmed at every genuine
   settle) unmodified; the `TryRefocus` nonce path unmodified.
3. The two protected pins (`lib.rs:2996`, `lib.rs:3493`)
   byte-identical in their assertions and green.
4. The wrap's clip law green: `columns_seek_probe` tests green; the
   board goldens untouched (no snapshot drift — the layers paint
   nothing and are disabled without a flow, so goldens must not
   change; any golden diff is a finding).
5. No UI-side business logic: board-layout.slint dispatches the one
   callback; the adapter owns commit-vs-discard-vs-noop (the UI layer
   is a dumb presentation client, AGENTS §1).
6. The layers disabled without a flow (D-4) — pan and aimed-surface
   behavior with no flow open are byte-for-byte today's behavior.
7. `#![forbid(unsafe_code)]` and the SPDX headers per AGENTS §6
   (board-layout.slint is slint; any new .rs file carries
   `// SPDX-License-Identifier: AGPL-3.0-only` — but none should be
   needed).

## 5. Verification commands

```sh
scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint
scripts/ux-coverage.sh .artifacts/plans/2026-11-27-board-click-out-ux-plan.md \
  --root /home/viktoria/src/taskboard-WT-fix-ui-focus-remount-wrap --gate
scripts/run-quality-gates.sh cargo nextest run --workspace   # stage-2's gate per the discipline
```

Expected ux-coverage steady state at stage-3 ship: every row FOUND or
BOUND (the W-1/W-2 callbacks resolve once the slint + adapter code
lands; the three stage-2 journeys resolve once written); exit 0.
Stage-2's mid-flight steady state: only the rows for code not yet
written are MISSING.

## 6. Diff-shape rules

- `board-layout.slint`: two `TouchArea` blocks + one callback
  declaration + gating. Nothing else moves — no reformat, no comment
  rewrite of neighboring blocks.
- `adapter/seams/board.rs`: one `install_seam!` block in the house
  flat-wiring style (comments state constraints: the settle funnel,
  the `[try]` echo absorb, the flowless no-op).
- `tests/journeys.rs`: three new journey fns adjacent to the red
  journey. The red journey and every other existing test: zero bytes
  changed.
- Nothing else: no models.rs change expected (the gating feed exists);
  if stage 2 believes models.rs must change, STOP and report — that
  contradicts D-3/D-4 and re-opens the design.

## 7. Stage prompts (paste-ready, verbatim)

**Long-command discipline (binding for every stage — docs/agent_dispatch.org, restated verbatim):**
A subagent is killed after ~600s without observable activity. (1) *The coverage trio never runs inside a subagent*: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus `scripts/run-quality-gates.sh cargo nextest run --workspace` (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. *Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate.* (2) *Stage 3's scope ends at PR-open*: push, open the PR, report — the dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`, teardown, and at-merge issue closures. (3) *Any command expected over ~5 minutes runs backgrounded* with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) *Every agent-launched cargo invocation routes through `scripts/memory-gate.sh`* — nothing bare. (5) *Stage prompts restate this discipline verbatim.* Out-of-scope findings become `backlog:` GitHub issues (or comments) immediately (AGENTS §7; search first).

### Prompt 7.1 — stage 2 (IMPLEMENTATION)

```text
You are the stage-2 IMPLEMENTATION agent for the board dead-zone click-out chunk (#1543) on the existing branch fix/ui-focus-remount-wrap, in the worktree /home/viktoria/src/taskboard-WT-fix-ui-focus-remount-wrap (this worktree already exists, is rebased onto origin/main, and carries the wrap + #1490 mechanism commits; do NOT recreate it, do NOT push).

Discipline (binding — docs/agent_dispatch.org; restated verbatim): A subagent is killed after ~600s without observable activity. Long-command discipline: (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh — nothing bare. (5) Stage prompts restate this discipline verbatim. Out-of-scope findings become backlog: GitHub issues (or comments) immediately (AGENTS §7; search first).

1. Read AGENTS.md, then the accepted UX journey plan .artifacts/plans/2026-11-27-board-click-out-ux-plan.md (normative: §0 rulings R-1–R-4, §3 US-1/US-2/US-3, §4 journey specs, §6 wiring rows W-1–W-4, §7 command delta, §8 verification) and the chunk plan .artifacts/plans/2026-11-27-board-click-out-plan.md (normative: §1 recorded reality, §2 decisions D-1–D-7, §3 TDD order, §4 invariants, §6 diff-shape rules).
2. Implement exactly per those plans. The red a_dead_zone_click_settles_the_create_flow (crates/taskboard-ui-slint/tests/journeys.rs:12669) stays BYTE-IDENTICAL — do not weaken it, do not reformat it. TDD order per the chunk plan §3: first the E1 scratch experiment (D-6, ≤30 min, scratch probe never committed — under /var/tmp if it needs files), then the three new journeys, then the two slint TouchArea layers + board-dead-zone-clicked callback (W-1 inside columns-row := Flickable declared BEFORE columns, W-2 at board level beside the dead-zone DropArea, both enabled: root.flow-open), then the on_board_dead_zone_clicked seam in src/adapter/seams/board.rs in the [try] settle shape (exemplar: seams/task.rs:32; precedent: the drag.rs pointer seams).
3. Hard rules: no new ActionId, no new StateCommand, no new UiRequest (UX plan §7). The seam body calls a.settle_flow(&window, &state) and nothing else — the flowless no-op is settle_flow's own early-return; no UI-side branching on flow content. The layers are enabled: root.flow-open — with no flow open, pan and every aimed surface behave byte-for-byte as today. The #1490 mechanism (seams/task.rs absorb + narrowed classification, the consumed-once refocus_create_armed law, the TryRefocus nonce path) must not be touched. No text assertions in tests (AGENTS §5); the two protected pins lib.rs:2996 and lib.rs:3493 stay byte-identical; board goldens must not drift (the layers paint nothing).
4. Commits: every commit standalone (rebase-merge hygiene), subjects per the chunk plan §3 step 5. Any build or test run routes through scripts/memory-gate.sh; the fast crate gate is scripts/memory-gate.sh cargo nextest run -p taskboard-ui-slint (background + poll if slow; expect the new journeys green, the red journey green, the pins green, the columns seek probe green). The workspace gate is scripts/run-quality-gates.sh cargo nextest run --workspace per the discipline above (background + poll; flock-deferral rule applies).
5. UX gate: run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-board-click-out-ux-plan.md --root /home/viktoria/src/taskboard-WT-fix-ui-focus-remount-wrap --gate — at done, every row reads FOUND/BOUND and the script exits 0 (a MISSING on a claimed row is not done).
6. No push, no PR — shipping is stage 3's last task.
7. Backlog issues: open/comment on backlog: GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: the E1 outcome (which layer owns (10, 450); any fallback used, with evidence), per-journey results, the crate-gate and workspace-gate results (or "gate deferred: lock held by <holder>"), the ux-coverage steady state, any deviation from the plans and why, backlog issue numbers.
```

### Prompt 7.2 — stage 3 (REVIEW)

```text
You are the stage-3 REVIEW agent for the board dead-zone click-out chunk (#1543, branch fix/ui-focus-remount-wrap) in the worktree /home/viktoria/src/taskboard-WT-fix-ui-focus-remount-wrap.

Discipline (binding — docs/agent_dispatch.org; restated verbatim): A subagent is killed after ~600s without observable activity. Long-command discipline: (1) The coverage trio never runs inside a subagent: a stage-2/stage-3 agent's gate is the pre-commit hooks (automatic) plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll with useful work between polls if needed); the dispatcher runs the full AGENTS §10 suite between stage 2's landing and stage 3's ship. Flock-deferral rule: when the gate lock is contended and no other useful work remains, never block — commit all landed work, report "gate deferred: lock held by <holder>", and end the stage; the dispatcher runs the gate. (2) Stage 3's scope ends at PR-open: push, open the PR, report — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and at-merge issue closures. (3) Any command expected over ~5 minutes runs backgrounded with short interleaved polls and useful work between polls — commit all landed work BEFORE any wait. Large scratch under /var/tmp, never /tmp. (4) Every agent-launched cargo invocation routes through scripts/memory-gate.sh — nothing bare. (5) Stage prompts restate this discipline verbatim. Out-of-scope findings become backlog: GitHub issues (or comments) immediately (AGENTS §7; search first).

1. Begin by rebasing onto main: git fetch origin --prune, then git rebase origin/main (AGENTS §8 rebase dance; drop already-merged duplicate commits; resolve only real conflicts; push --force-with-lease if the rebase rewrote history). Then read AGENTS.md, the UX journey plan .artifacts/plans/2026-11-27-board-click-out-ux-plan.md, the chunk plan .artifacts/plans/2026-11-27-board-click-out-plan.md, and review the ENTIRE branch diff versus the post-rebase base (git diff origin/main...HEAD) — this one PR ships three things together: the Flickable wrap re-land, the #1490 remount-re-grab mechanism, and the click-out. Review all three.
2. Functional focus — this chunk's specific traps: (a) the red journey a_dead_zone_click_settles_the_create_flow must be byte-identical in its assertions and pass for the real mechanism (delete the click route mentally: would it fail? if not, it passes vacuously — BLOCKER); (b) the #1490 invariants (chunk plan §4.2): the seams/task.rs narrowed absorb, the consumed-once refocus_create_armed law, the TryRefocus nonce path — any edit there that the click-out did not need is a finding; (c) the two protected pins (lib.rs:2996, lib.rs:3493) byte-identical; (d) the columns seek probe and the board goldens — the TouchArea layers paint nothing and are disabled without a flow, so any golden drift is a finding; (e) the no-fire law: the layers must steal no aimed click (a card click, a field click, chrome) and must be inert without a flow — check the gating and z-order in board-layout.slint against the chunk plan D-1/D-4, not against comments; (f) no UI-side business logic: the slint side dispatches board-dead-zone-clicked and nothing else (chunk plan §4.5); (g) no new ActionId/StateCommand/UiRequest exists (UX plan §7).
3. Code-quality mandate: beyond functional review, evaluate (a) code quality: naming, comment discipline (comments state constraints, not narration), idiomatic use of the approved stack, no dead code or speculative generality, test quality per AGENTS §5; (b) the Single Responsibility Principle: the crate-role boundaries of AGENTS §2 are the SRP line — a boundary violation is an SRP violation; (c) a single level of abstraction per function body: a function either orchestrates named operations or works in primitive data, never mixes. Findings are fixed on the branch like any other finding.
4. UX completeness mandate: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-27-board-click-out-ux-plan.md --root /home/viktoria/src/taskboard-WT-fix-ui-focus-remount-wrap --gate — it must exit 0 (every row FOUND/BOUND). Then execute the walkthrough protocol (docs/ux_planning.org §2.4) over the P1 story US-1 — the four cognitive-walkthrough questions per step (see / recognize / understand feedback / recover) with file:line or test evidence, then the six-heuristic pass over the touched surfaces (board-layout.slint's board body, the create field). BLOCKER/MAJOR findings are fixed on the branch; MINOR findings become backlog issues.
5. Re-run the gates that apply to you per the discipline: the pre-commit hooks run regardless; the workspace suite is scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; flock-deferral rule applies). The dispatcher owns the full AGENTS §10 suite.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups; every commit standalone).
7. Ship — your scope ends at PR-open: push the branch (git push --force-with-lease origin fix/ui-focus-remount-wrap), open the PR to main with gh (title and body covering all three: the wrap re-land per the clip-focus law #1419 option A, the #1490 remount re-grab, the #1543 dead-zone click-out; note in the body that the PR merges only with all three, per the operator ruling recorded on #1490), and report the URL. Do NOT watch checks, do NOT verify merge state, do NOT remove the worktree — the dispatcher owns all of that ("do not remove the worktree"). main is protected: never merge; the user merges on GitHub.
8. Backlog issues: open/comment on backlog: GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings (functional + both mandates), what you fixed, the ux-coverage result, the walkthrough verdict, PR URL, backlog issue numbers. Never merge; never touch main.
```

## 8. Estimated size

S: ~60–90 lines of slint (two TouchArea blocks + one callback + a
constraint comment each), ~10 lines of adapter seam, ~120–180 lines of
test (three journeys), plus the two plan documents. One PR (the
existing #1490 branch), three commits on top.
