# [Plan] GUI Review Round 5 — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-05
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 8 chunks of the GUI review round 5
  fix round. No code in this commit.
- **Origin**: `.artifacts/plans/2026-11-05-gui-review-round5-plan.md`
  (normative: §2 root-cause map, §3 decisions D1–D8, §4.1–§4.8 chunk
  designs, §6 test hooks, §7 chunk mapping, §8 risks), plus the
  user's 2026-11-05 session rulings Q1–Q6 recorded there as D1–D6
  (D7 ruled accepted 2026-11-05 — see chunk 8's remaining gate).
- **Status**: READY except **chunk 8** (gated only on DR-8 chunk 2 /
  #630 landing or the config-detection fallback; the D7 account-first
  flow was ruled accepted by the user on 2026-11-05) and the deferred items in §5 of the
  origin (markdown view-mode chunk D1b dispatches after its spike is
  written into a follow-up; W5b hostable sub-surfaces is a separate
  future design round, deliberately not dispatched here).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §4.1, D8 | `fix/ui-slint-dead-seams` | Time Entries forwarding line + the view-mount callback inventory test + the through-the-callback journey | S |
| 2 | §4.2, D1 | `feat/ui-slint-multi-line-editor` | The `MultiLineEditor` L1 widget (view/edit modes, viewport growth probe, scrollbar, Shift+Enter/Enter/Esc grammar) + task-description adoption | M |
| 3 | §4.3, D6 | `refactor/ui-slint-anchored-flyouts` | Dropdown + TimeField flyouts → window-anchored overlays; quick-time latch + open-time filter seeding; visible empty state | M |
| 4 | §4.4, D5 | `feat/ui-slint-focus-coverage` | SegmentedControl ring + raw keys; Slider keyboard; SettingsButton stop; settings mount focus; the per-view focus-coverage invariant test | M |
| 5 | §4.5, D3 | `feat/app-live-settings` | Domain `SettingsSnapshot` + `UpdateSettings`; engine ownership + `SettingsChanged`; actor live re-reads; UI live badges + apply-semantics registry | L |
| 6 | §4.6, D2 | `feat/state-deadline-reminders` | Deadline-driven firing loop; tick default 30→1; per-tick cost audit + sweep gating | M |
| 7 | §4.7, D4 | `refactor/ui-slint-keybind-scopes` | The registered key-scope stack; the editor scope + preset command tables (emacs/vi, single-slot clipboard); the printable monopoly; retire the four intercept special cases | L |
| 8 | §4.8, D7 | `feat/app-account-connections` | Account-first connections section; `ListRemoteBoards`; explicit remote bind; create-local-from-remote | M |

**Dependency graph**: default dispatch order 1 → 2 → 3 → 4 → 7 → 5 →
6 → 8. Chunks 1, 2, 3 are mutually order-independent (disjoint files
except small `app-window.slint` mount blocks — parallel lanes
acceptable, rebase-light); 4 may parallel 3; **7 is gated on 2
merging** (the editor is the scope's first consumer); **6 is gated on
5** (the same engine loop; tick becomes live-adjustable); **8 is
gated on DR-8 #630 or config detection (the human D7 ruling landed
2026-11-05)**.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` next to the
project root (AGENTS §8), branched off the then-current `main`,
no PR ever stacked. **Branch names are never reused after their PR
lands** (AGENTS §8, issue #415): a resume/second round of a chunk
works on `<branch>-v2` (or re-branches off the fetched `origin/main`),
never on the landed branch — the doctor treats any worktree whose
`HEAD` is fully contained in `origin/main` as disposable. Create
worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`).
Disk hygiene (AGENTS §8 worktree lifecycle): any agent creating or
recreating a worktree runs `scripts/worktree-doctor.sh` from the main
checkout first — nonzero exit (free disk below the floor) stops the
chunk; stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead
   per AGENTS §8; never `git worktree add <path> main`), executes per
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
   AGENTS rules **plus the mandate below**, re-runs whatever gates
   apply, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its last tasks
   are shipping: push the branch, open the PR to `main` with `gh`,
   watch `gh pr checks --watch` to completion — any failing check is
   a finding; diagnose, fix, push `--force-with-lease`, repeat until
   green — **then verify the PR's merge state**: `gh pr view --json
   mergeStateStatus` must report `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase
   onto `origin/main`, push `--force-with-lease`, let the checks
   re-run, repeat; any other state ⇒ diagnose before handing off.
   **Then teardown**: remove the now-disposable worktree — from the
   project root, `git -C /home/viktoria/src/taskboard worktree remove
   --force ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 —
   never a bare relative path, which resolves against whatever cwd the
   session happens to be in and can name a neighbour's worktree;
   everything precious is pushed; a later round that resumes the chunk
   recreates it on a fresh branch). Confirm via `git worktree list`
   that the removed path was the intended one. The handoff contract is
   "PR open + checks green + merge state `CLEAN` + worktree removed" so
   the user's merge needs no additional rebase and the chunk leaves no
   36–55G of build output behind. Reports a verdict. Never merges;
   never touches `main`. (`main` is branch-protected — merges happen
   on GitHub by the user, after the required checks pass.)

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

## 2. Chunk 1 — dead seams (`fix/ui-slint-dead-seams`)

- **Covers**: origin §4.1 + D8, normative. The forwarding line for
  `task-track-entries-clicked()` at the `TaskView` mount
  (`ui/app-window.slint:403-440`); the new source-scan inventory test
  (every `pure callback` a view/composition declares has a handler at
  its mount site — the Slint-side complement of
  `tests/seam_inventory.rs`); the journey invoking
  `window.invoke_task_track_entries_clicked()` and asserting the
  track-entries flow opens. Non-goals: no other callback changes, no
  Rust-side seam work (the seam exists at `adapter.rs:6948`), no
  panel changes.
- **Acceptance**: the journey passes through the real callback; the
  inventory test fails on a deliberately-removed forwarding (spot-
  check in review, revert); all gates green; diff is one markup line
  + two test files.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of GUI review round 5 ("dead seams", branch fix/ui-slint-dead-seams) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the round plan .artifacts/plans/2026-11-05-gui-review-round5-plan.md §2 (row F1), §3 D8, §4.1 — normative — and this dispatch plan §1/§2.
2. Read the current seams: ui/app-window.slint (the TaskView mount at ~403-440 and the window callback declarations ~1041-1180), ui/views/task-view.slint (its `pure callback` declarations), src/adapter.rs install_tracking_seams (~6926) for the Rust side that already exists, tests/seam_inventory.rs for the manifest-test house style, and tests/journeys.rs for the journey helper grammar (interpret_for_test at flow_table_tests.rs:3230 is the WRONG layer — the point of this chunk is the callback path).
3. Run scripts/worktree-doctor.sh from the project root first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree with git worktree add -b fix/ui-slint-dead-seams ../taskboard-WT-fix-ui-slint-dead-seams off main (never without -b and never on main — AGENTS §8).
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-r5-dead-seams-plan.md settling: the source-scan test's exact parse strategy (which files scanned, how mount blocks and component callback declarations are extracted — brittle-regex avoidance), the scope of components covered (views AND compositions mounted by them — pick the boundary and record it), and the journey's assertions (flow opens, subject correct, kiosk refusal arm unchanged).
5. Commit it as: docs(plan): the round-5 dead-seams chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of GUI review round 5 ("dead seams").

1. Read AGENTS.md, the round plan §4.1, the dispatch plan §2, and the stage-1 plan (.artifacts/plans/*dead-seams*-plan.md) in the worktree ../taskboard-WT-fix-ui-slint-dead-seams (branch fix/ui-slint-dead-seams). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD: the failing journey first (callback unforwarded = dead), then the forwarding line, then the inventory test. Every commit standalone (rebase-merge hygiene). SPDX AGPL-3.0-only headers for new test files per AGENTS §6; no sleeps in tests; assert typed outcomes, never label text.
3. Run the full gate suite from AGENTS.md §10 via scripts/run-quality-gates.sh and make it green.
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of GUI review round 5 (branch fix/ui-slint-dead-seams).

1. Read AGENTS.md, the round plan §4.1, the dispatch plan §2 (the code-quality mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD after the opening rebase dance) in ../taskboard-WT-fix-ui-slint-dead-seams.
2. Functional focus: the inventory test must catch the actual bug class — in review, temporarily delete the new forwarding line and confirm the test fails, then restore it. The test must scan, not hardcode: a newly declared unforwarded callback in any covered component must fail it. The journey must go through window.invoke_task_track_entries_clicked, not interpret_for_test.
3. Code-quality mandate: (a) naming/comment discipline, idiomatic stack, no dead code; test quality per AGENTS §5 (no text assertions, deterministic, lowest layer); (b) AGENTS §2 crate boundaries; (c) one abstraction level per function.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green. main is protected: never merge; the user merges on GitHub.
7. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ui-slint-dead-seams. Confirm with git worktree list that the removed path was the intended one.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — the MultiLineEditor (`feat/ui-slint-multi-line-editor`)

- **Covers**: origin §4.2 + D1, normative. The new
  `ui/widgets/multi-line-editor.slint` (view mode: wrapped inset
  `RoleText` in a scroll viewport; edit mode: `TextInput {
  single-line: false }` in the viewport with content-height growth
  and cursor-following scroll; scrollbar on overflow; Shift+Enter →
  accepted, Enter → newline, Escape → discarded); the
  viewport-growth probe test (gate for lifting the fence at
  `text-field.slint:133-139`); the task-view description adoption
  (both branches swap; F2/F5/F6 fixed by construction). Non-goals:
  editor keybind scope (chunk 7), markdown view mode (D1b), changes
  to the single-line `TextField` beyond what adoption requires, any
  Rust commit-funnel change (newlines already round-trip — verify,
  don't touch).
- **Acceptance**: description display wraps at the column edge with
  no clipped left edge; edit mode grows with content past the old
  3-line window (probe-gated), scrolls via scrollbar and via cursor
  movement; Enter inserts newlines that persist through commit;
  Shift+Enter commits; Escape discards; the pinned probe/tests from
  `tests/focus_seam_probe_tests.rs` still hold for the new widget's
  focus-lost behavior; gates green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of GUI review round 5 ("MultiLineEditor", branch feat/ui-slint-multi-line-editor) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the round plan §2 (rows F2-F7, F12), §3 D1 (including the licensing constraint: Slint's textedit-base.slint is the REFERENCE GRAMMAR, never copied code), §4.2 — normative — and this dispatch plan §1/§3.
2. Read the current field machinery: ui/widgets/text-field.slint (the fence at 133-139 and the recorded probes at 64-69, 82-84, 96-113), ui/widgets/tipped-field.slint, ui/views/task-view.slint 838-912 (the description section), ui/theme.slint (RoleText, tokens), src/view_state.rs commit_description (~1084) and settle_flow's EditDescription arm (~1803), src/adapter.rs 6107-6152 (the focus-lost/commit seams). Also read ~/.cargo/registry/src/*/i-slint-compiler-1.18.1/widgets/common/textedit-base.slint as the reference grammar ONLY (single-line flag, viewport content-height, cursor-position-changed scroll math, page-height).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b feat/ui-slint-multi-line-editor ../taskboard-WT-feat-ui-slint-multi-line-editor off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-r5-multi-line-editor-plan.md settling: the probe design (viewport growth + width-change re-wrap, no bare-layout constraint — the exact panic the fence recorded), the fallback if the probe fails (paged viewport; the fence stays, recorded negative), the scrollbar's house-token shape and when it shows, the widget's callback surface (accepted/discard/focus-lost/edited) so chunk 7 can attach the editor scope later, commit/Escape/focus-lost semantics mapped onto the existing settle funnel, and the description adoption diff shape.
5. Commit it as: docs(plan): the round-5 multi-line editor chunk plan. Do not implement; do not push.

Report: plan path, decisions (especially the probe verdict expectation), risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of GUI review round 5 ("MultiLineEditor").

1. Read AGENTS.md, the round plan §4.2, the dispatch plan §3, and the stage-1 plan in the worktree ../taskboard-WT-feat-ui-slint-multi-line-editor (branch feat/ui-slint-multi-line-editor). Recreate the worktree on that branch if missing.
2. Implement per the approved plan, TDD, probe FIRST: the growth probe decides fence-lifting vs the paged fallback before the widget is written. Then the widget, then the adoption, then the journeys (wrap at column edge; no clipped left character; growth past 3 lines; scroll by bar and by cursor; Enter newline persists; Shift+Enter commits; Escape discards). Every commit standalone. AGPL-3.0-only headers; no sleeps; typed outcome assertions; do not weaken the existing task-view journeys.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, the probe verdict (fence lifted or fallback taken and why), any deviation.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of GUI review round 5 (branch feat/ui-slint-multi-line-editor).

1. Read AGENTS.md, the round plan §4.2 + D1, the dispatch plan §3 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase) in ../taskboard-WT-feat-ui-slint-multi-line-editor.
2. Functional focus: no GPL Slint code was copied (grammar re-implemented — compare the scroll math for verbatim lifts); the fence at text-field.slint:133-139 is either lifted with a passing probe or kept with the recorded negative — not silently deleted; the single-line TextField's other consumers are untouched; Enter/Shift+Enter/Escape semantics hold in BOTH the field-focus and focus-lost settle paths (the r4 F24 confusion class must not return); the widget publishes focus facts compatible with EditingFacts (insert-mode pulls depend on them).
3. Code-quality mandate: as spelled out in dispatch §1 — naming/comments, AGENTS §5 test quality, §2 boundaries, one abstraction level per function.
4. Re-run the full gate suite (AGENTS §10); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR with gh, gh pr checks --watch until green, verify mergeStateStatus CLEAN (rebase + force-with-lease if not). Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-slint-multi-line-editor; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 4. Chunk 3 — anchored flyouts (`refactor/ui-slint-anchored-flyouts`)

- **Covers**: origin §4.3 + D6, normative. The `Dropdown` and
  `TimeField` flyouts convert to window-anchored overlays (facts
  globals mirroring anchor geometry, window-level conditional mounts
  with clamp/flip, dismissal underlays — the DatePickerFacts/
  app-window.slint:530-557 grammar); the displaced-underlay code at
  `time-field.slint:142-159` and `dropdown.slint:256-273` is
  deleted; the quick-times chevron and the due-editor calendar
  toggle route through adapter seams that set the `due_focus_steal`
  latch; the quick-list filter seeds unfiltered at open; the
  empty-rows dropdown state becomes a visible disabled field with a
  caption. D6 explicitly overrides the recorded no-overlay decision
  at `dropdown.slint:251-257` — the override is recorded in the
  chunk plan. Non-goals: no keymap/chain changes (chunk 7), no
  settings-connections redesign (chunk 8), no DatePicker overlay
  changes beyond the TimeField's relocation into it.
- **Acceptance**: every flyout mounts under its anchor with flip/
  clamp (journeys per mount site: settings board picker, board-bar
  switcher, timer-setter, track-entries form, picker quick-times);
  the quick-time button opens the menu without closing the picker
  regardless of prior keyboard state; a set time still shows the
  full quick list at open; typed-digit narrowing still narrows; the
  empty board list shows the disabled+caption state; the dropdown
  ladder journeys (journeys.rs:12219+) stay green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of GUI review round 5 ("anchored flyouts", branch refactor/ui-slint-anchored-flyouts) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the round plan §2 (rows F8-F10, F14), §3 D6, §4.3 — normative — and this dispatch plan §1/§4.
2. Read the current machinery: ui/compositions/date-picker.slint + ui/menu-facts.slint (DatePickerFacts) + ui/app-window.slint 516-557 (the window-anchored grammar to copy) and 616-681 (the track-entries anchored mount); ui/widgets/time-field.slint and ui/widgets/dropdown.slint in full (the underlay displacement being deleted, the key ladders being preserved); src/adapter.rs 6224-6233 (a focus seam), 6299-6357 (picker toggle + time-edited seams), 6467-6515 (apply_picker_key's latch), 6586-6628 (settle_date_editor and the latch consumption), 1055-1074 + src/due_edit.rs 300-330 (the quick-list filter), 4897-4919 (the dropdown ladder decline). Mount sites to cover: settings-view.slint 366-430, board-bar.slint:79, timer-setter.slint:190, track-entries.slint 206/219.
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b refactor/ui-slint-anchored-flyouts ../taskboard-WT-refactor-ui-slint-anchored-flyouts off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-r5-anchored-flyouts-plan.md settling: one facts global per widget class vs one shared anchored-flyout global (pick, record); where the window mounts live and the dismissal ordering (underlay before panel, before other overlays); how the widget-local `open` state stays the driver while geometry moves window-side; the two latch seams (quick-times chevron, due calendar toggle) and their exact steal-latch contract; the open-time filter reset (which state is the typed-filter vs the value mirror); the empty-rows visible state's copy source; the D6 override note replacing the dropdown.slint:251-257 decision.
5. Commit it as: docs(plan): the round-5 anchored-flyouts chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of GUI review round 5 ("anchored flyouts").

1. Read AGENTS.md, the round plan §4.3, the dispatch plan §4, and the stage-1 plan in ../taskboard-WT-refactor-ui-slint-anchored-flyouts (branch refactor/ui-slint-anchored-flyouts). Recreate the worktree if missing.
2. Implement per the plan, TDD: journeys first per mount site (anchor position incl. a flip case, dismissal, ladder behavior unchanged), then the conversion, then the latch seams and filter seeding. The quick-time session journey: open picker with mouse only → click quick-times → menu appears under the field, picker stays open, full list shown with a time set. Every commit standalone; AGPL headers; no sleeps; typed assertions.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of GUI review round 5 (branch refactor/ui-slint-anchored-flyouts).

1. Read AGENTS.md, the round plan §4.3 + D6, the dispatch plan §4 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: no leftover displaced-underlay geometry (grep the old pattern); the dropdown ladder decline in adapter.rs 4897-4919 still matches the new mount reality (its decline keys on SettingsBoardFacts/BoardBarFacts.open — confirm those facts still exist and fire); the steal latch covers BOTH new seams and the settle journey (field focus-loss with picker open + quick-time open) is pinned; typed-digit narrowing per keystroke is NOT broken by the open-time reset; kiosk posture unchanged.
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-slint-anchored-flyouts; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 5. Chunk 4 — focus coverage (`feat/ui-slint-focus-coverage`)

- **Covers**: origin §4.4 + D5, normative. `SegmentedControl` focus
  ring + raw Return/Space activation; `Slider` FocusScope + arrows +
  Home/End with the existing preview/commit ceremony; `SettingsButton`
  → stop (ActivationStop grammar); settings-view root mount-focus
  parity; the per-view focus-coverage invariant (`tests/focus_coverage.rs`:
  synthetic-Tab stop walk + item-tree scan for click-bearing
  TouchAreas outside stops). Non-goals: no keymap/preset changes, no
  view redesigns beyond the named widgets, no Today-view item-walk
  semantics change (C-n/p stays item walk there by design — origin
  §2 F13).
- **Acceptance**: tabbing through settings shows a ring on every
  segment and activates via Enter/Space; the slider takes focus and
  steps with arrows; Save/Test/Bind/Remove/Detach are stops; the
  invariant test fails when a segment's FocusScope is removed and
  passes across all views; the task-view stop-inventory journey
  (journeys.rs:6424+) still passes; gates green.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of GUI review round 5 ("focus coverage", branch feat/ui-slint-focus-coverage) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the round plan §2 (rows F13, F15, F19), §3 D5, §4.4 — normative — and this dispatch plan §1/§5.
2. Read the current machinery: ui/widgets/segmented-control.slint and ui/widgets/icon-button.slint (the raw-key + ring grammar to copy, icon-button.slint 33, 44-64), ui/widgets/slider.slint, ui/widgets/activation-stop.slint, ui/views/settings-view.slint (484-531 root/init, 230-259 SettingsButton), the synthetic-Tab mechanism (src/adapter.rs dispatch_traversal ~2212-2247, walk_reentry), tests/walk_probe_tests.rs (P1-P5 — especially the P5 conditional-mount negative and the visibility-filter facts), tests/journeys.rs 6387-6529 (the TaskViewWalkFacts inventory-walk grammar to generalize), and the i-slint-core-1.18.1 item-tree/focus APIs the probes already use (item_focus traversal, is_visible_or_clipped_by_flickable semantics).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b feat/ui-slint-focus-coverage ../taskboard-WT-feat-ui-slint-focus-coverage off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-r5-focus-coverage-plan.md settling: the invariant test's two halves (stop-walk enumeration via a generalized walk-facts publication vs the TouchArea-coverage scan — exact item-tree traversal, which TouchAreas count as interactive and the recorded exemption list for non-interactive ones like scroll surfaces), the per-view fixtures, the Slider ceremony mapping (arrow step = preview, release/commit rule — reuse the r4 D38 release-commit decision), SettingsButton conversion, and the settings-root focus parity.
5. Commit it as: docs(plan): the round-5 focus-coverage chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of GUI review round 5 ("focus coverage").

1. Read AGENTS.md, the round plan §4.4, the dispatch plan §5, and the stage-1 plan in ../taskboard-WT-feat-ui-slint-focus-coverage (branch feat/ui-slint-focus-coverage). Recreate the worktree if missing.
2. Implement per the plan, TDD: the invariant test first (it should FAIL on the current tree — that red is the finding F15/F19 formalized), then the widget fixes turn it green view by view. Segment activation must go through picked() (pointer, raw keys, and AT all funnel there); slider arrows reuse the existing committed/preview ceremony. Every commit standalone; AGPL headers; no sleeps; typed assertions; floors not ceilings (the invariant must fail on removed stops, not pin today's order accidents beyond reading order).
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, the initial-red inventory (what the invariant found beyond the known F13/F19 widgets), deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of GUI review round 5 (branch feat/ui-slint-focus-coverage).

1. Read AGENTS.md, the round plan §4.4 + D5, the dispatch plan §5 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: the invariant genuinely guards — in review, remove a FocusScope from a covered widget and confirm the test fails, then restore; the exemption list for non-interactive TouchAreas is explicit and commented (each entry names why it is not a stop); segmented raw keys do not double-fire with accessible-action-default; the slider's arrow steps honor the r4 release-commit decision (no live persistence per step); settings traversal order remains tree/reading order without newly skipped stops.
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-slint-focus-coverage; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 6. Chunk 5 — live settings (`feat/app-live-settings`)

- **Covers**: origin §4.5 + D3, normative. Domain
  `SettingsSnapshot` + `StateCommand::UpdateSettings`; `AppState`
  gains the settings field (boot seeds from figment via the existing
  resolve accessors turned into constructors); the engine applies,
  publishes, and emits the settings-changed notification (broadcast
  `SystemEvent` vs dedicated watch — the chunk plan settles, bias
  broadcast); consumers re-read per iteration (reminder loop, sync
  actors' poll/backoff re-arm, notifier sink, UI day-start/due-soon
  derivations); app-lane persist-then-command ordering; UI badges →
  live + the apply-semantics registry with the empty `OnRestart`
  exception list and the justification-must-exist test. Non-goals:
  the deadline-driven loop (chunk 6 — this chunk only makes the
  reminder loop re-read tick/enabled/deadlines each iteration),
  config schema changes, sqlite persistence of settings.
- **Acceptance**: changing each General/Reminders/Sync-Timing/
  Display setting in a running (test-harness) app is observable in
  the consuming actor without restart (paused-clock journeys: poll
  re-arm within one cycle, tick change visible at the next loop
  iteration, day-start/due-soon derivations on the next refresh);
  env-locked settings stay locked; the file overlay still
  prove-and-writes and a failed write sends no command; the registry
  test fails an unjustified `OnRestart` entry; gates green.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of GUI review round 5 ("live settings", branch feat/app-live-settings) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the round plan §2 (row F17), §3 D3, §4.5 — normative — and this dispatch plan §1/§6. Also docs/architecture.org §the actor/channel rules (no shared-mutable locks; ArcSwap reads; broadcast for system events).
2. Read the current machinery: taskboard-app/src/config.rs (the settings sections, resolve_* accessors 1449-1511, env_overridden_keys 1179), settings_overlay.rs (prove_and_write), ui_requests.rs handle_settings (335-422) + resolve_settings_snapshot (ui.rs 423-451), taskboard-domain/src/messages.rs (StateCommand surface) + state.rs (AppState), taskboard-state/src/actor.rs (the run loop's select arms 559-597, the reminder_cfg capture at 266), the sync actors' poll loops (taskboard-sync-nextcloud/src/actor.rs), the notifier/escalation bootstrap captures (bootstrap.rs 860-864, 1115-1126), and the UI side (src/requests.rs 189-194 SectionApply, src/settings_form.rs, settings-view.slint badges 477-481).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b feat/app-live-settings ../taskboard-WT-feat-app-live-settings off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-app-live-settings-plan.md settling: the SettingsSnapshot type shape (A1 minting for constrained values — reuse existing validation kinds), broadcast SystemEvent vs watch channel (pick one, record the trade-off), the persist-then-command ordering and its failure arm, per-consumer re-read points and the exact wake mechanism for sleeping actors, the boot seeding path, the registry's location and the justification field, and the journey catalogue per setting.
5. Commit it as: docs(plan): the round-5 live-settings chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of GUI review round 5 ("live settings").

1. Read AGENTS.md, the round plan §4.5, the dispatch plan §6, and the stage-1 plan in ../taskboard-WT-feat-app-live-settings (branch feat/app-live-settings). Recreate the worktree if missing.
2. Implement per the plan, TDD, domain-first: the snapshot type + command, then the engine apply/publish/notify (insta snapshots of AppState), then the app-lane ordering, then consumers one at a time with a paused-clock journey each (no sleeps — tokio::time pause/advance), then the UI badges + registry test. Every commit standalone; SPDX MIT OR Apache-2.0 for domain/state/app lib files per AGENTS §6 (note: taskboard-app is AGPL); #![forbid(unsafe_code)] preserved; no text assertions.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of GUI review round 5 (branch feat/app-live-settings).

1. Read AGENTS.md, the round plan §4.5 + D3, the dispatch plan §6 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: no actor kept a spawn-time settings copy (grep the old captures — reminder_cfg, escalation defaults, ui resolve copies); the write path cannot desync (failed prove-and-write sends no command; command carries the same snapshot that was persisted); env locks hold (an env-set value cannot be overwritten via UI and stays flagged); sleeping actors demonstrably wake (the paused-clock poll re-arm journey); no new Mutex/RwLock cross-actor state crept in (AGENTS §3).
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-app-live-settings; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 7. Chunk 6 — deadline-driven reminders (`feat/state-deadline-reminders`)

- **Covers**: origin §4.6 + D2, normative. Domain
  `next_due_instant` (pure, proptested against the firing
  evaluation); the run loop's tick arm → `sleep_until(min(deadline,
  now + tick))` with re-arm on due-changing command batches; the
  default tick 30 → 1 (config default, settings placeholder, help
  copy, state-crate docs); the per-tick cost audit with sweep gating
  if the idle path touches the repo. **Gated on chunk 5 merging.**
  Non-goals: the UI countdown cadence (already 1 Hz, untouched), the
  escalation/alarm minute-cadence pass, any settings UI change
  beyond the default/copy.
- **Acceptance**: paused-clock journeys fire a reminder at its due
  instant (not the next tick) after task mutations at arbitrary
  offsets; the day-boundary roll still arms; the audit note in the
  chunk plan shows the idle per-tick path (clock read + min, no repo
  IO) or the sweep gating landed; tick remains adjustable live via
  chunk 5's path; gates green.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of GUI review round 5 ("deadline-driven reminders", branch feat/state-deadline-reminders) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 5 (feat/app-live-settings) must be merged in origin/main — verify (git log origin/main) before starting; if absent, STOP and report instead of planning.

1. Read AGENTS.md, then the round plan §2 (row F16 — including the premise correction: countdowns are already 1 Hz UI-side; the lag is firing), §3 D2, §4.6 — normative — and this dispatch plan §1/§7.
2. Read the current machinery: taskboard-state/src/actor.rs (run loop 559-597, reminder_tick 1198-1248, the stale-delivery sweep, day_tick arming 579-596, the SettingsChanged re-read points chunk 5 added), taskboard-state/src/core.rs due_reminder_firings (124-136), taskboard-domain's firing evaluation, taskboard-app/src/config.rs:57 + the settings placeholder/help copy sites, taskboard-state/src/config.rs docs (16-17, 35).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b feat/state-deadline-reminders ../taskboard-WT-feat-state-deadline-reminders off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-state-deadline-reminders-plan.md settling: next_due_instant's shape and the proptest tying it to the firing evaluation (they must never disagree), which command outcomes re-arm (the exhaustive list — every CommandOutcome that can change a due or a rule), first-tick and day-roll interactions, the sweep gating decision (measured or reasoned, recorded), and the default/copy changes.
5. Commit it as: docs(plan): the round-5 deadline-reminders chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of GUI review round 5 ("deadline-driven reminders").

1. Read AGENTS.md, the round plan §4.6, the dispatch plan §7, and the stage-1 plan in ../taskboard-WT-feat-state-deadline-reminders (branch feat/state-deadline-reminders). Recreate the worktree if missing (on -v2 if the branch landed).
2. Implement per the plan, TDD: the pure query + proptest first, then the loop change with paused-clock journeys (fire at the instant after: boot, a due edit, a rule add, an unbind), then the sweep gating per the plan's decision, then the default/copy changes. Every commit standalone; MIT OR Apache-2.0 headers for lib crates; no sleeps; typed assertions.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, the audit note (idle path cost), deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of GUI review round 5 (branch feat/state-deadline-reminders).

1. Read AGENTS.md, the round plan §4.6 + D2, the dispatch plan §7 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: the re-arm list is exhaustive (a due-changing command that does not re-arm is a missed-firing bug — cross-check every CommandOutcome variant against the plan's list); a deadline in the past fires immediately without a busy loop (clamp rules); the day-roll still fires exactly once; the 1 s default did not silently change the disabled-test fallback semantics; the sweep gating did not lose stale-delivery coverage (its journey still exists at the gated cadence).
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-state-deadline-reminders; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 8. Chunk 7 — the keybind context stack (`refactor/ui-slint-keybind-scopes`)

- **Covers**: origin §4.7 + D4 (W5a), normative. The `KeyScope`
  registration mechanism (surfaces register `(ContextId, mask)` on
  mount; focusable widgets register on focus gain — the
  grid-focus seam generalized); the resolver consumes the ordered
  stack (focused-element scope → enclosing surfaces innermost-first →
  view → mode → global), replacing `KeymapShell::chain`'s boolean
  reconstruction; the `Editor` scope with preset command tables
  (emacs: C-p/n/f/b, C-a/e, C-k, C-d, C-Space set-mark, C-w, M-w,
  C-y, Shift+Enter + C-c C-c commit, C-c C-k + Esc discard; vi: the
  stripped normal-mode ladder) wired to the `MultiLineEditor` via a
  target seam, single-slot adapter clipboard; the **printable
  monopoly** rule in `intercept`; retirement of the four special
  cases (dropdown ladder decline, timer-setter Escape, due Down-arm,
  activation reservation) into scope/ladder declarations. Non-goals:
  hostable sub-surfaces (W5b — separate design round), new flows,
  preset binding content changes beyond the editor tables.
- **Acceptance**: the F11 session journey passes — picker open,
  field focused, typing inserts characters while C-p/n/f/b move the
  calendar selection, both directions, both picker phases; under vi,
  printables still reach the field while typing and h/j/k/l arm
  after Esc; the editor scope gives the MultiLineEditor the full
  emacs/vi command set incl. mark/region cut/copy/paste; every
  retired special case's journey (dropdown ladder, timer-setter
  Escape, due ladder, activation matrix, walk re-entry, C-g
  contract) passes unchanged; `flow_table_tests` green untouched;
  gates green.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of GUI review round 5 ("keybind context stack", branch refactor/ui-slint-keybind-scopes) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: chunk 2 (feat/ui-slint-multi-line-editor) must be merged in origin/main — verify before starting; if absent, STOP and report instead of planning.

1. Read AGENTS.md, then the round plan §2 (rows F11, F18), §3 D4, §4.7 — normative — and this dispatch plan §1/§8. Also the flow audit .artifacts/reports/2026-10-26-ui-flow-state-audit-report.md §6 (the settle matrix the stack must not disturb).
2. Read the current machinery end to end: src/adapter.rs intercept (578-686) + on_intercept_key (4880-4922) + the four special cases (613-618, 627-655, 646-668, 4897-4919); src/shell.rs chain (83-135); src/keymap/ (mod resolve/masks, presets, stroke — including Alt/Meta handling in Stroke::from_event: verify M-w maps on all backends, record the finding); src/resolver.rs; ui/editing-facts.slint; the grid-focus seam (adapter.rs 6224-6233, date-picker.slint 40-64); ui/widgets/multi-line-editor.slint (chunk 2's callback surface); the pinned journeys (journeys.rs: the picker control-walk 7166+, combobox 8152+, two-phase ladder 8267+, dropdown decline 12219+, walk re-entry flow_table_tests.rs 983-1015, C-g contract keymap/mod.rs 903-955).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b refactor/ui-slint-keybind-scopes ../taskboard-WT-refactor-ui-slint-keybind-scopes off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-ui-r5-keybind-scopes-plan.md settling: the KeyScope data model and its registration/unregistration lifecycle (mount/focus/unmount ordering, the EditingFacts generation problem — stale registrations after unmount are the P5-class trap), the stack construction and its exact equality with today's chains for every existing state (enumerate: this is the port checklist), the printable-monopoly predicate (what counts as printable: no modifiers + text non-empty + not a named control key — align with Stroke::from_event's control-char rejection), the editor command table shape per preset and the editor target seam, the clipboard slot, and the retirement plan for each of the four special cases with its journey as the acceptance pin.
5. Commit it as: docs(plan): the round-5 keybind-scopes chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks (this is the round's highest-risk chunk — say so honestly), estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of GUI review round 5 ("keybind context stack").

1. Read AGENTS.md, the round plan §4.7, the dispatch plan §8, and the stage-1 plan in ../taskboard-WT-refactor-ui-slint-keybind-scopes (branch refactor/ui-slint-keybind-scopes). Recreate the worktree if missing.
2. Implement per the plan, TDD, in the plan's stage order (the stage-1 plan sequences it; expected: scope registry + stack construction behind the existing chain call sites first with chain-equality tests, then the monopoly rule, then the editor scope + tables, then the special-case retirements one at a time, each gated by its existing journey). The F11 session journey (type + chord-navigate, both presets) is written FIRST and stays red until the mechanism lands. Every commit standalone; AGPL headers; no sleeps; typed assertions.
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. No push, no PR. Never push to or merge main.

Report: per-gate results, the chain-equality checklist result (any intentional behavioral diff beyond F11 must be empty), deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of GUI review round 5 (branch refactor/ui-slint-keybind-scopes).

1. Read AGENTS.md, the round plan §4.7 + D4, the dispatch plan §8 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: the port checklist — every pinned journey listed in the stage-1 plan passes UNCHANGED (a semantic diff is a finding, not an accepted improvement; the pinned semantics are the contract); no special-case logic survived outside scope declarations (intercept contains only the monopoly rule, the re-entrancy guard, and the resolver feed — grep-verify); stale-scope registrations cannot outlive their surface (the unmount/reset seams — cross-check EditingFacts reset sites); M-w resolves on the testing backend (the Alt mapping finding from stage 1 is addressed or its limitation recorded); the vi preset's printable behavior under the monopoly is pinned by a test.
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-slint-keybind-scopes; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 9. Chunk 8 — account-first connections (`feat/app-account-connections`)

- **Covers**: origin §4.8 + D7, normative. The
  account-card connections section (sign-in form; account list
  grouped from connectors by `(server_url, username)`; per-account
  service toggles — Deck = connector presence, Files =
  `[files_sync]` presence/detection); the app-lane
  `ListRemoteBoards` request; explicit remote-id bind (title-match
  adoption retires); create-local-from-remote (`CreateBoard` +
  `BindBoard`); keyboard operability via the stop grammar; the
  local-board picker reuses the overlay dropdown. **Gated on: DR-8 chunk 2
  (#630) landed or the Files toggle hidden behind config detection
  (the user's D7 ruling landed 2026-11-05).** Non-goals: `[[accounts]]`
  config reshaping (fenced), Deck board-creation push (backlog),
  account-removal cascade (backlog), any filesync crate changes.
- **Acceptance**: the fake-engine journey completes sign-in →
  enable Deck → list remotes → bind a local board explicitly →
  verify binding in state; create-local-from-remote lands a bound
  board; a typed failure (no secret, network) surfaces without
  state change; the full session is keyboard-only operable
  (stop-walk); existing connection journeys updated, none weakened
  beyond the retired title-match adoption; gates green.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of GUI review round 5 ("account-first connections", branch feat/app-account-connections) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: DR-8 chunk 2 (#630) must be merged in origin/main, OR this plan takes the config-detection fallback for the Files toggle — verify and record which. (The D7 account-first ruling landed 2026-11-05; no human gate remains.)

1. Read AGENTS.md, then the round plan §2 (row F14), §3 D7 (including the rejected alternatives and the three-layer concept model), §4.8 — normative — and this dispatch plan §1/§9. Also the r4 connections plan .artifacts/plans/2026-11-01-ui-r4-settings-connections-plan.md (what D39 built and which of its decisions this chunk supersedes) and the filesync round's D-SF10 (config/credential shape this must not disturb).
2. Read the current machinery: settings-view.slint connections section (640-847) + BoardPicker (366-430); src/adapter.rs settings seams (4058-4190) + push_settings/board_selector_model (3552-3718); src/requests.rs (the settings request surface + ConnectionsSnapshot 36-62); taskboard-app/src/ui_requests.rs (attach_connection 662-767 — the title-match arm being retired, SaveConnection 537-606, current_connections 920-959); connectors.rs + connections.rs (config groupings, overlay writes); secrets.rs (the account-shaped credential key); taskboard-domain BindBoard/UnbindBoard (command.rs 1734-1863).
3. Run scripts/worktree-doctor.sh first (nonzero exit: STOP and report), then git worktree add -b feat/app-account-connections ../taskboard-WT-feat-app-account-connections off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-app-account-connections-plan.md settling: the account-card information architecture and its keyboard walk; the grouping rule (connectors by (server_url, username) — name collisions, the auto-derived connector name for new sign-ins and config uniqueness); the ListRemoteBoards request/reply/failure taxonomy; the explicit-bind request shape (extend AttachConnector vs new request — pick, record); create-local-from-remote's command sequence; the Files toggle's detection/gate; busy/error surfacing reusing the existing ceremony; and which r4 D39 decisions are superseded (list them for the review).
5. Commit it as: docs(plan): the round-5 account-connections chunk plan. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of GUI review round 5 ("account-first connections").

1. Read AGENTS.md, the round plan §4.8, the dispatch plan §9, and the stage-1 plan in ../taskboard-WT-feat-app-account-connections (branch feat/app-account-connections). Recreate the worktree if missing.
2. Implement per the plan, TDD: app-lane request tests first (fake DeckClient/credential paths per AGENTS §5 fakes-over-mocks), then the UI model/seams, then the section markup, then the keyboard-walk and session journeys over the fake engine. Every commit standalone; AGPL/MIT-Apache headers per crate; no sleeps; typed assertions (no label-text pins beyond copy owned by this chunk).
3. Run the full gate suite via scripts/run-quality-gates.sh and make it green.
4. Bookkeeping: open the two backlog issues the round plan fences (Deck push-to-remote creation; account-removal cascade) — title-prefixed `backlog:` per AGENTS §7, body = summary + permalink to the round plan.
5. No push, no PR. Never push to or merge main.

Report: per-gate results, deviations, the two issue URLs.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of GUI review round 5 (branch feat/app-account-connections).

1. Read AGENTS.md, the round plan §4.8 + D7, the dispatch plan §9 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (post-rebase).
2. Functional focus: the title-match adoption arm is fully retired (no dead NoRemoteMatch paths); config writes stay shape-compatible (the filesync D-SF10 contract untouched — [files_sync] never rewritten by this UI beyond its enabled flag); credentials never enter a model property (the D26b rule); the keyboard walk covers every new interactive element (the chunk-4 invariant test covers this view too — it must be extended in this chunk, not bypassed); the busy ceremony cannot wedge on a dropped reply (existing guard pattern reused).
3. Code-quality mandate: as in dispatch §1.
4. Re-run the full gate suite; any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, PR with gh, checks --watch until green, mergeStateStatus CLEAN. Never merge.
7. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-app-account-connections; confirm via git worktree list.

Report: verdict, findings, fixes, PR URL, green checks, worktree removed. Never merge; never touch main.
```

## 10. Completion bookkeeping

- **Issues this round opens** (chunk 8 stage 2, or the user on
  acceptance if chunk 8 stays gated): `backlog: Deck push-to-remote
  board creation` and `backlog: account-removal cascade` (labels
  `ui` + `architecture`), each permalinking the round plan §5.
- **Docs**: chunk 5 amends `docs/architecture.org` (the settings
  distribution section: snapshot ownership, the notification, the
  apply-semantics registry policy "live by default, OnRestart needs
  a recorded justification"); chunk 7 amends the keymap/flow
  documentation the audit report carries (the scope stack replaces
  the chain-construction description); chunk 6 updates the §5.4
  cadence design note the state crate cites.
- **Deliberately not dispatched here**: the markdown view-mode chunk
  (D1b — its spike is written when dispatched; nothing else gates
  on it), the W5b hostable-sub-surfaces design round (feeds the
  journal walk's stage 3), the focus debug aid (D5), the
  `[[accounts]]` config reshaping, board-first connect shortcut.
- **What opens next**: after chunk 7 merges, the W5b design round is
  the natural follow-up (its first consumer — the journal review
  walk's planner embedding — is scheduled behind v0.2 chunk 3); after
  chunk 8, the connections backlog items above.
