# [Plan] GUI Review Round 7 Implementation — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-13
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 3 implementation chunks of the round-7
  GUI review remediation (habit creation, the recurrence-rule editor, the
  per-account Files board roster). No code in this commit.
- **Origin**: `.artifacts/plans/2026-11-13-gui-review-round7-plan.md`
  (the accepted round-7 stage-1 plan; §2 findings table, §3 solutions
  D-7.1 … D-7.3, §3 accessibility ledger, §4 non-goals, §5 chunking),
  plus the user's 2026-11-13 acceptance ("I approve of your plan.
  Please create a dispatch plan and push a PR with your plans.") and the
  stage-1 plan's own chunk proposal (its §5 table). Issue origins: #755
  and #756 are the recorded non-goals this dispatch deliberately leaves
  open; #723, #718, #750 are referenced boundaries.
- **Status**: READY — all three chunks are dispatchable once this plans
  PR (`docs/gui-review-round7-plans`: the stage-1 plan + this dispatch
  plan) lands, so the origin references resolve on `main`. Chunk prompts
  cite origin sections by number and re-verify anchors against the
  landed tree. Deliberately **not** dispatched here: multi-account files
  lanes (#723), the stop-sync subtree GC design (#755, rides #718),
  Deck→Deck board moves (#756), habit target-editing UI beyond create
  (the CLI `habits update` covers it; not requested), and the #750
  adoption-state badge fold (a small follow-up once chunk 3's row
  grammar lands).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | D-7.2 | `feat/ui-r7-habit-create` | Create habits from the Today section: header add affordance + inline create form (title, per-day/week/month target, quota) dispatching `CreateHabit` on the section's board; `HabitCreate` action id in the Today context. `habits.slint`, `today-view.slint`, `app-window.slint`, `adapter.rs`, `models.rs`, `presentation.rs`, `actions.rs`, `keymap/presets.rs`, `router.rs`. No engine/storage changes (the command exists), no target-editing UI, no board picker (the section's board is the board) | S–M |
| 2 | D-7.1 | `feat/ui-r7-recur-editor` | The recurrence-rule editor in the task detail view: domain extraction of the CLI's rule-assembly laws, the `recur-editor.slint` overlay (pattern/interval/weekday/monthly-anchor/bound + live text-form echo + armed "Stop repeating"), add/edit affordances + `SeriesEdit` action id. `taskboard-domain/src/recur.rs`, `commands/repeat.rs` (CLI refactor onto the domain fn), `task-view.slint`, new `ui/compositions/recur-editor.slint`, `app-window.slint`, `adapter.rs`, `actions.rs`, `keymap/presets.rs`. No message/schema change (`TaskChanges.recur` exists), no at-create scheduling, no pause/resume relocation (the recur row keeps those) | M–L |
| 3 | D-7.3 | `feat/ui-r7-files-board-roster` | The per-account Files board roster: `BoardInfo` gains the binding channel tag; badge fold (`FilesBoardBadge`) + per-account sort; roster rows with per-row action buttons ("Stop Sync" / "Switch to Sync via this Connection" / "Sync via this Connection"); two new app arms (`StopBoardFilesSync`, `PublishBoardToFiles`); the `FilesMovePicker` retires. `requests.rs`, `adapter.rs`, `presentation.rs`, `settings-view.slint`, `app-window.slint`, `ui_requests.rs`, `board_move_ceremony.rs` (assign-step reuse), `bootstrap.rs` (constants/decision reuse), `settings_status.rs`. No engine work (Unbind/Bind exist), no multi-lane sync (#723), no subtree GC (#755) | M |

**Dependency graph**: all three chunks are order-independent — each
branches off `main` and the file sets are disjoint (chunk 2 is the only
one below the UI crate: a pure-function extraction in
`taskboard-domain` plus the CLI verb refactored onto it; chunks 1 and 3
touch `taskboard-ui-slint` + `taskboard-app` only, and not the same
files as each other: chunk 1 is the Today/habits surface, chunk 3 is
the settings surface). Serial or parallel dispatch all work. Default
dispatch order: 1 → 2 → 3 (smallest first de-risks the shared
registry/facts machinery before the larger editor lands; the roster is
last because it carries the two new request arms and a picker
retirement).

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); independent chunks may run concurrently as
separate sessions; never two agents share a worktree. Every chunk works
in `../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branched off the then-current `main`, no PR ever stacked. **Branch
names are never reused after their PR lands** (AGENTS §8, issue #415):
a resume/second round of a chunk works on `<branch>-v2` (or re-branches
off the fetched `origin/main`), never on the landed branch. Create
worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`). Disk
hygiene (AGENTS §8 worktree lifecycle): any agent creating or
recreating a worktree runs `scripts/worktree-doctor.sh` from the main
checkout first — nonzero exit (free disk below the floor) stops the
chunk; stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current `main`,
   writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.

2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or the
   worktree path's PR already landed, create `<branch>-v2` instead per
   AGENTS §8; never `git worktree add <path> main`), implements per the
   approved plan — TDD, every commit standalone (rebase-merge hygiene),
   full AGENTS §10 gate suite green. Does **not** push or open a PR.
   Reports deviations.

3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*: **begins
   by rebasing onto `main`** — `git fetch origin --prune`, rebase the
   branch onto `origin/main` (AGENTS §8's rebase dance: drop
   already-merged duplicate commits; resolve only real conflicts), push
   `--force-with-lease` if the rebase rewrote history — then reviews
   the entire branch diff versus the **post-rebase** base against the
   chunk's acceptance criteria + AGENTS rules **plus the code-quality
   mandate below**, re-runs whatever gates apply, implements fixes for
   every finding on the branch (amend/reword, never duplicate-subject
   fixups). Its last tasks are shipping: push the branch, open the PR
   to `main` with `gh`, watch `gh pr checks --watch` to completion —
   any failing check is a finding; diagnose, fix, push
   `--force-with-lease`, repeat until green — **then verify the PR's
   merge state**: `gh pr view --json mergeStateStatus` must report
   `CLEAN`; `BEHIND`/`DIRTY` ⇒ rebase onto `origin/main`, push
   `--force-with-lease`, let the checks re-run, repeat; any other state
   ⇒ diagnose before handing off. **Then teardown**: remove the
   now-disposable worktree — from the project root,
   `git -C /home/viktoria/src/taskboard worktree remove --force
   ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8 — never a
   bare relative path; everything precious is pushed). Confirm via
   `git worktree list` that the removed path was the intended one. The
   handoff contract is "PR open + checks green + merge state `CLEAN` +
   worktree removed". Reports a verdict. Never merges; never touches
   `main`.

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, evaluate (a) code quality: naming, comment discipline (comments
state constraints, not narration), idiomatic use of the approved stack,
no dead code or speculative generality, test quality per AGENTS §5;
(b) the Single Responsibility Principle: the crate-role boundaries of
AGENTS §2 are the SRP line — a boundary violation is an SRP violation;
(c) a single level of abstraction per function body: a function either
orchestrates named operations or works in primitive data, never mixes.
Findings are fixed on the branch like any other finding.

## 2. Chunk 1 — Habit creation from Today (`feat/ui-r7-habit-create`)

- **Covers** (normative: origin plan §3 D-7.2 + §3 accessibility
  ledger): the `HabitsSection` header (`ui/compositions/habits.slint:63-76`)
  gains an add `IconButton` gated by `editing-visible` (kiosk renders
  rows, never actions — D-T16), new `create-asked()` callback up
  through `today-view.slint` / `app-window.slint`; an inline create form
  mounted below the header while armed (adapter-owned flag — the
  stack-create/inline-row precedent, no transient UI state in Slint):
  title `TippedField` (insert mode), target `SegmentedControl`
  (Per day / Per week / Per month), quota numeric field defaulting to 1,
  confirm/cancel; confirm dispatches
  `StateCommand::CreateHabit { board, title, target }` with board = the
  section's board (the model pass already scopes it, `models.rs:742-744`);
  when no board is active the affordance does not mount. New
  `ActionId::HabitCreate` ("habit.create"), section-scoped, `Today`
  context, free keys per preset. **Non-goals**: no engine/storage
  changes; no board picker; no target-editing after create (`UpdateHabit`
  exists, not requested); no new optional create fields (D-H7a stands).
- **Acceptance**: adapter seam test asserting the exact `CreateHabit`
  command (board resolution + the minted `HabitTarget` with
  `NonZeroCount`); stale-row/one-shot target semantics preserved (the
  existing habit seams' law); the kiosk gate test (actions unmount,
  model intact); keymap preset totality tests stay green with the new
  binding; the engine habit snapshots (s45 family) untouched and green;
  full gate suite green.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the GUI review round 7
implementation ("habit creation from Today", branch
feat/ui-r7-habit-create) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, then
   .artifacts/plans/2026-11-13-gui-review-round7-plan.md §3 D-7.2 and the
   §3 accessibility ledger (normative) and §2 row R7-2, and the dispatch
   plan .artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md
   §1/§2.
2. Read the seams you will extend: ui/compositions/habits.slint (the
   header ~63-76, the editing-visible gate, the row callback grammar),
   ui/views/today-view.slint (section props/callbacks ~93-105),
   ui/app-window.slint (habit props/callbacks/forwarding), src/adapter.rs
   (install_habit_seams ~6795-6964 — the one-shot target-field law, the
   stale-index no-op), src/models.rs (push_habits ~727-765, IdTables),
   src/presentation.rs (habit_rows ~6163+, habit_editing_visible),
   src/actions.rs (the habit verb family ~286-305 and section-scoped
   entries ~1249-1295, ContextId::Today), src/keymap/presets.rs (binding
   rows — find a free key per preset), src/router.rs (habit routing
   ~636-652). Domain truth you dispatch against: CreateHabit at
   crates/taskboard-domain/src/messages.rs:461-515, HabitTarget /
   NonZeroCount at crates/taskboard-domain/src/habit.rs:20-84. Tests you
   must not break: the engine habit snapshots (s45 family,
   taskboard-state/tests/snapshots.rs ~2246-2308) and the existing habit
   seam tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first
   (nonzero exit = free disk below the floor: STOP and report), then
   create the worktree: git worktree add -b feat/ui-r7-habit-create
   ../taskboard-WT-feat-ui-r7-habit-create off main (never without -b,
   never on main — AGENTS §8).
4. Write a TDD-first plan at
   .artifacts/plans/<today>-ui-r7-habit-create-plan.md settling: the
   exact callback/seam names and the armed-form flag's home (adapter
   field + facts global, naming per the settings-facts precedent); the
   form's control grammar (which existing widgets; how quota parses —
   NonZeroCount::new with the typed-refusal hint path, no text
   assertions); the target SegmentedControl → HabitTarget mapping; the
   board resolution (section board; the no-board mounting rule); the
   HabitCreate action registration (Today context, section-scoped menu
   group) and the free-key choice per preset; the test catalogue (seam
   exact-command test, gate test, preset totality). Resolve every open
   question with a recorded decision; deviations from the origin need a
   recorded reason.
5. Commit it as: docs(plan): the ui-r7 habit create chunk. Do not
   implement; do not push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously
   for any out-of-scope finding (AGENTS §7 standing authorization) —
   list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the GUI review
round 7 implementation ("habit creation from Today").

1. Read AGENTS.md, the origin plan §3 D-7.2 (+ the accessibility
   ledger), the dispatch plan §1/§2, and the stage-1 plan in
   ../taskboard-WT-feat-ui-r7-habit-create (recreate the worktree on the
   branch if missing — <branch>-v2 if the PR landed, AGENTS §8).
2. Implement exactly per the approved plan, TDD, every commit standalone.
   Hard rules: SPDX AGPL-3.0-only headers on new files; no transient UI
   state in Slint (adapter-owned flags + facts globals); the one-shot
   target semantics (each seam arms its own target; stale index =
   silent no-op); quota mints through NonZeroCount::new (zero is
   unrepresentable — the field must refuse it structurally, not by
   message text); no tokio sleeps in tests; no text assertions (typed
   outcomes / structural facts only); the kiosk gate empties the ACTION,
   never the model (D-T16).
3. Run the full gate suite from AGENTS.md §10 (through
   scripts/run-quality-gates.sh) and make it green.
4. Bookkeeping: none this chunk (the origin plan owns it).
5. No push, no PR — shipping is stage 3's last task. Never push to or
   merge main.
6. Backlog issues autonomously for out-of-scope findings (AGENTS §7);
   list the numbers in your report.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the GUI review round 7
implementation (branch feat/ui-r7-habit-create).

1. Read AGENTS.md, the origin plan §3 D-7.2 (+ the accessibility
   ledger), the dispatch plan §1/§2 (the code-quality mandate applies to
   you), and the stage-1 plan; then review the full branch diff (git
   diff main...HEAD) in ../taskboard-WT-feat-ui-r7-habit-create.
2. Functional focus: the create command must carry the SECTION's board
   (a board-resolution shortcut to "first board" or a hardcoded board is
   a finding); quota zero and quota overflow must be structurally
   refused (the mint, not a string check); the armed form must no-op
   safely when the board list changes under it (stale-arm law); the
   keyboard binding must exist in EVERY preset (default/vi/emacs) with
   the preset totality tests green; kiosk (editing-visible false) must
   mount neither the header affordance nor the form while the rows
   still render; the existing +1/repair/convert seams must be
   un-weakened.
3. Code-quality mandate: (copy verbatim from the dispatch plan §1.)
4. Re-run the full gate suite yourself (AGENTS §10, through
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then
   gh pr checks --watch; any failing check is a finding — diagnose,
   fix, push --force-with-lease, repeat until green; verify
   mergeStateStatus CLEAN per the dispatch plan §1.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-feat-ui-r7-habit-create; confirm via
   git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR
URL, green-checks confirmation, worktree-removed confirmation. Never
merge; never touch main.
```

## 3. Chunk 2 — Recurrence-rule editor (`feat/ui-r7-recur-editor`)

- **Covers** (normative: origin plan §3 D-7.1 + §3 accessibility
  ledger): (a) **domain extraction** — the CLI's `assemble_rule` laws
  (`crates/taskboard-app/src/commands/repeat.rs:306-360`: keep the
  existing `series`/`continued_from` when a rule already exists — D-S4;
  anchor = `series_reference_date(task, day, now)`; weekday/day-of-month
  defaults from the anchor; `paused_until` preserved) move into
  `taskboard-domain` (e.g. `recur::assemble_rule(&Task, pattern parts,
  bound, now, day) -> Result<RecurRule, InvalidRecur>` — pure, zero
  deps) and the CLI verb refactors onto it, its tests moving/pinning
  with the function; (b) **the editor overlay** — new
  `ui/compositions/recur-editor.slint` mounted window-level (the
  date-picker precedent) with its own `ContextId::RecurEditor` (mask
  semantics like `TrackEntries`; fields ride `ViInsert`; commit/cancel
  in-context): pattern SegmentedControl (Daily/Weekly/Monthly), daily
  interval field (`EveryDays`), weekly interval + weekday toggles
  (`WeekdaySet`, pre-checked from the anchor), monthly anchor
  OnDay/NthWeekday pair, bound SegmentedControl (Never/Until/Count;
  Until reuses `due_edit::parse`; Count = `OccurrenceCount`), live
  echo of `rule.text_form()` + typed `InvalidRecur` hints; (c) **the
  affordances** — task detail "Add repeat…" for rule-less tasks near
  the due/sched rows and "Edit repeat…" in the recur row beside the
  pattern echo; new `ActionId::SeriesEdit` ("series.edit") in
  `TaskDetail`, free keys per preset; adapter-owned draft buffers (the
  due-editor precedent); commit dispatches
  `TaskChanges { recur: Some(Some(rule)) }`; "Stop repeating" (armed
  confirm strip) dispatches `Some(None)`. **Non-goals**: no
  message/schema change; no at-create scheduling (the capture ruling
  stands); no pause/resume relocation (the recur row keeps them); no
  new Tone variants; the CLI verb's user-visible behaviour unchanged.
- **Acceptance**: the domain assembly unit tests + a proptest (moved
  with the function; the CLI's existing repeat tests stay green against
  the refactored verb); an adapter seam test asserting the exact
  `UpdateTask` command with the assembled rule; a detach test asserting
  `recur: Some(None)`; the edit-an-existing-rule path preserves
  `series`/`continued_from`/`paused_until` (D-S4 pin); keymap preset
  totality green with the `SeriesEdit` binding; the engine recur
  stamping/sidecar tests untouched and green; full gate suite green.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the GUI review round 7
implementation ("recurrence-rule editor", branch feat/ui-r7-recur-editor)
in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then
   .artifacts/plans/2026-11-13-gui-review-round7-plan.md §3 D-7.1 and the
   §3 accessibility ledger (normative) and §2 row R7-1, and the dispatch
   plan .artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md
   §1/§3.
2. Read the seams: crates/taskboard-domain/src/recur.rs (the grammar
   ~123-701, series_reference_date ~714, InvalidRecur ~34 — the
   extraction's new home), crates/taskboard-app/src/commands/repeat.rs
   (assemble_rule ~306-360 — the laws to extract verbatim; the verb's
   dispatch ~254-261), crates/taskboard-domain/src/messages.rs
   (TaskChanges.recur ~676, the double-Option set/clear law ~692),
   crates/taskboard-state/src/command.rs (the UpdateTask recur arm
   ~1072-1076 — read-only truth for what commit must produce),
   crates/taskboard-ui-slint/ui/views/task-view.slint (the recur row
   ~726-831, the due-row region for the Add-repeat affordance),
   ui/compositions/date-picker.slint and the TrackEntries context (the
   overlay + ContextId precedents), src/adapter.rs (install_recur_seams
   ~6800-6898, the recur flags ~292-293, settle_date_editor/commit_date
   ~8615-8681 — the buffer/commit precedent), src/due_edit.rs (the date
   parser Until reuses), src/actions.rs (the series verb family
   ~303-318, ContextId), src/keymap/presets.rs, src/router.rs
   (~664-692). Tests you must not break: the CLI repeat tests
   (commands/repeat.rs), the engine recur tests/snapshots, the
   due-editor tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard
   first (nonzero exit = free disk below the floor: STOP and report),
   then create the worktree: git worktree add -b feat/ui-r7-recur-editor
   ../taskboard-WT-feat-ui-r7-recur-editor off main (never without -b,
   never on main — AGENTS §8).
4. Write a TDD-first plan at
   .artifacts/plans/<today>-ui-r7-recur-editor-plan.md settling: the
   domain function's exact signature and module placement (and which
   repeat.rs tests move vs stay); the editor's draft model (adapter
   buffer shape, per-field parse/validity mapping to the typed
   InvalidRecur variants); the ContextId::RecurEditor mask semantics
   and its resolver-chain registration; the commit funnel (draft →
   assemble_rule → TaskChanges) and the detach strip; the two
   affordance mount sites and the SeriesEdit registration + free keys
   per preset; the test catalogue. Resolve every open question with a
   recorded decision; deviations from the origin need a recorded reason.
5. Commit it as: docs(plan): the ui-r7 recur editor chunk. Do not
   implement; do not push.
6. Backlog issues autonomously (AGENTS §7 standing authorization); list
   the numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the GUI review
round 7 implementation ("recurrence-rule editor").

1. Read AGENTS.md, the origin plan §3 D-7.1 (+ the accessibility
   ledger), the dispatch plan §1/§3, and the stage-1 plan in
   ../taskboard-WT-feat-ui-r7-recur-editor (recreate the worktree on the
   branch if missing — <branch>-v2 if the PR landed, AGENTS §8).
2. Implement exactly per the approved plan, TDD, every commit standalone.
   Hard rules: the extraction is behaviour-preserving — the CLI repeat
   verb's tests are the pin and must pass against the refactored verb
   UNCHANGED (a behavioural delta here is a finding, not a test update);
   the domain function stays pure (zero new crate deps); SPDX headers
   (MIT OR Apache-2.0 for taskboard-domain, AGPL-3.0-only for the UI
   crate); draft state lives adapter-side; the typed mint constructors
   are the only entry points (no struct-literal RecurRule outside the
   assembly path); no sleeps in tests; no text assertions (the echo
   renders text_form() but tests assert the assembled rule, never
   strings); edit-preserves-series (D-S4) is pinned by a test.
3. Run the full gate suite from AGENTS.md §10 (through
   scripts/run-quality-gates.sh) and make it green.
4. Bookkeeping: none this chunk.
5. No push, no PR — shipping is stage 3's last task. Never push to or
   merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers in your
   report.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the GUI review round 7
implementation (branch feat/ui-r7-recur-editor).

1. Read AGENTS.md, the origin plan §3 D-7.1 (+ the accessibility
   ledger), the dispatch plan §1/§3 (the code-quality mandate applies to
   you), and the stage-1 plan; then review the full branch diff (git
   diff main...HEAD) in ../taskboard-WT-feat-ui-r7-recur-editor.
2. Functional focus: editing an EXISTING rule must preserve
   series/continued_from/paused_until (D-S4) — a test must pin it and a
   weakening of it is a finding; the detach path must send recur
   Some(None) and nothing else; the assembly must go through the domain
   mint (a UI-side struct-literal RecurRule is an SRP violation); the
   CLI verb must behave bit-for-bit as before (diff the dispatch order
   and the outcome mapping); Escape/commit in the overlay context must
   respect the global Escape law (ModeNormal resolution); the two
   affordances (Add repeat for rule-less tasks, Edit repeat in the recur
   row) must both be walk-ordered with hover tips and converge on
   SeriesEdit; the weekly default pre-check derives from the anchor,
   never from "today".
3. Code-quality mandate: (copy verbatim from the dispatch plan §1.)
4. Re-run the full gate suite yourself (AGENTS §10, through
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then
   gh pr checks --watch; any failing check is a finding — diagnose,
   fix, push --force-with-lease, repeat until green; verify
   mergeStateStatus CLEAN per the dispatch plan §1.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-feat-ui-r7-recur-editor; confirm via
   git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR
URL, green-checks confirmation, worktree-removed confirmation. Never
merge; never touch main.
```

## 4. Chunk 3 — Per-account Files board roster (`feat/ui-r7-files-board-roster`)

- **Covers** (normative: origin plan §3 D-7.3 + §3 accessibility
  ledger): (a) `BoardInfo` gains `bound_channel: Option<BoardChannel>`
  (`Deck | Files`), filled app-side in `current_connections`
  (`ui_requests.rs:1413-1432`) from `binding.board.as_deck()/as_files()`
  — structural detection per D-BF4, never a connector-name match;
  (b) the presentation badge fold (`FilesBoardBadge { ThisConnection,
  LocalOnly, OtherConnection }`, tones success/neutral/info on the
  existing `Badge` widget, labels verbatim from the origin) and the
  per-account sort (ThisConnection → LocalOnly → OtherConnection, ties
  by title then id); (c) the roster rows — the files-board loop
  (`adapter.rs:4867-4883`) replaced so every live board emits once per
  Nextcloud account card with badge text/tone + action label/enabled
  computed Rust-side; each row's `SettingsButton` joins the walk;
  (d) the actions — "Stop Sync": new `UiRequest::StopBoardFilesSync
  { board, reply }` → guards (board live, binding Files-typed) →
  `StateCommand::UnbindBoard` on the engine handle → fresh
  `ConnectionsSnapshot` reply; "Switch to Sync via this Connection":
  enabled only on the anchor card with the lane on, reuses
  `MoveBoardToFiles` verbatim (BusyDot/PendingKind::Move/typed error
  fold as today); "Sync via this Connection" (LocalOnly rows): new
  `UiRequest::PublishBoardToFiles { board, reply }` → guards
  (files spawn decision resolvable, board live, binding None; Deck-bound
  → typed BindRefused; Files-bound → refused) → the move ceremony's
  final assign step (`BindBoard { remote: RemoteBoardId::files(
  board_uuid), connector: FILES_SYNC_CONNECTOR_NAME }`) reusing the
  engine-handle run pattern; (e) the `FilesMovePicker` retires
  (component `settings-view.slint:592-630`, copy global
  `SettingsMoveCopy` ~473-481, `move_choices` table, the
  `settings-move-picked` seam) — the roster is the picker. The
  round-6 "disconnect is a non-goal" ruling is overturned by the
  origin plan (D-7.3) — the row comment records this with the origin
  as authority. **Non-goals**: no engine work (Unbind/Bind exist); no
  multi-lane sync (#723 — non-anchor cards render the roster with the
  files actions disabled, existing restart captions carry the why); no
  subtree GC or retirement marker (#755 — copy must not promise
  deletion); no new Tone variants; no Deck→Deck switch (#756).
- **Acceptance**: presentation fold tests over the anchor/non-anchor/
  lane-off matrix (badge, sort, action label/enabled asserted as
  enums/flags, never label text); adapter roster-push tests (one row
  per board per card; archived-board visibility per the stage-1
  decision); app-arm tests for both new requests (typed refusals; the
  planned `UnbindBoard`/`BindBoard` + snapshot reply) mirroring the
  move arm's tests; the existing move-arm tests stay green; picker
  retirement leaves no dead copy/table entries; full gate suite green.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the GUI review round 7
implementation ("per-account Files board roster", branch
feat/ui-r7-files-board-roster) in the taskboard repo at
/home/viktoria/src/taskboard.

1. Read AGENTS.md, then
   .artifacts/plans/2026-11-13-gui-review-round7-plan.md §3 D-7.3 and the
   §3 accessibility ledger (normative) and §2 row R7-3, issue #723 (the
   single-lane boundary), and the dispatch plan
   .artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md §1/§4.
2. Read the seams: crates/taskboard-ui-slint/src/requests.rs (BoardInfo
   ~65-70, ConnectorInfo ~51-58, SettingsSnapshot.files_sync_anchor
   ~250, UiRequest variants ~344-430 — MoveBoardToFiles ~425 is the
   Switch payload), src/adapter.rs (connection_items_model ~4764-4998 —
   the files-board loop ~4867-4883 is what the roster replaces;
   files_on ~4841-4842; ItemAddr ~3886-3912; move_choices ~3962-3964;
   push_settings/push_move_options ~5004-5076; settings_move_picked
   ~5600-5623; PendingKind), src/presentation.rs (deck_bound_boards
   ~5157-5177 and account_cards ~5182-5211 — the fold siblings to
   mirror), ui/views/settings-view.slint (ConnectionItemKind ~71-79 and
   ConnectionItem ~84-107, the Files row ~1016-1064, the files-board
   rows ~1070-1086 with their round-6 ruling comment, FilesMovePicker
   ~592-630), ui/widgets/badge.slint (the Badge primitive),
   src/settings_status.rs (the sentence registry ~30-89, move sentences
   ~80-88); crates/taskboard-app/src/ui_requests.rs (current_connections
   ~1413-1432, handle_settings ~396-399, move_board_to_files
   ~1506-1542, map_move_error ~1547-1565, detach_connection
   ~1172-1198), crates/taskboard-app/src/board_move_ceremony.rs
   (AppWorld::assign_files ~506-516 — the assign step the publish arm
   reuses), crates/taskboard-app/src/bootstrap.rs (FILES_SYNC_
   CONNECTOR_NAME ~103, files_sync_spawn_decision ~120-134). Tests you
   must not break: the move-arm app tests, the settings adapter tests,
   the presentation folds' tests.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard
   first (nonzero exit = free disk below the floor: STOP and report),
   then create the worktree: git worktree add -b
   feat/ui-r7-files-board-roster
   ../taskboard-WT-feat-ui-r7-files-board-roster off main (never
   without -b, never on main — AGENTS §8).
4. Write a TDD-first plan at
   .artifacts/plans/<today>-ui-r7-files-board-roster-plan.md settling:
   the BoardChannel shape and where it lives (requests.rs twin vs
   deriving from RemoteBoardId at the seam); the exact badge enum +
   tone map + the verbatim labels' home (presentation constants); the
   archived-board visibility decision (the origin says "show them
   flagged archived" — settle the flag/copy and whether actions are
   enabled on archived rows); the per-account fold's inputs (how the
   anchor account joins ConnectorInfo) and its determinism; the two new
   request variants' payloads/replies and their SettingsError variants +
   status sentences; the Stop/Publish arms' guard order and ceremony
   reuse (assign_files on the engine handle — the publish-board arm
   precedent); the pending/busy model for the two new actions; the
   picker retirement's exact deletion list; the row comment recording
   the round-6 ruling's overturn (origin D-7.3 as authority); the test
   catalogue. Resolve every open question with a recorded decision;
   deviations from the origin need a recorded reason.
5. Commit it as: docs(plan): the ui-r7 files board roster chunk. Do not
   implement; do not push.
6. Backlog issues autonomously (AGENTS §7 standing authorization); list
   the numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the GUI review
round 7 implementation ("per-account Files board roster").

1. Read AGENTS.md, the origin plan §3 D-7.3 (+ the accessibility
   ledger), issue #723, the dispatch plan §1/§4, and the stage-1 plan in
   ../taskboard-WT-feat-ui-r7-files-board-roster (recreate the worktree
   on the branch if missing — <branch>-v2 if the PR landed, AGENTS §8).
2. Implement exactly per the approved plan, TDD, every commit standalone.
   Hard rules: the channel tag is detected STRUCTURALLY from the
   binding (as_deck/as_files) — any connector-name string match is a
   D-BF4 violation; the UI stays a dumb client (badge text/tone and
   action label/enabled are computed Rust-side and pushed); the Switch
   action reuses MoveBoardToFiles VERBATIM (no new ceremony parameters);
   the Stop/Publish arms run on the engine handle (the publish-board
   precedent) and reply with the fresh ConnectionsSnapshot; typed
   SettingsError variants + registry sentences only (no string errors);
   the stale-index no-op law for the new row seams; no new Tone
   variants; SPDX AGPL-3.0-only headers on new files; no sleeps in
   tests; no text assertions.
3. Run the full gate suite from AGENTS.md §10 (through
   scripts/run-quality-gates.sh) and make it green.
4. Bookkeeping: none this chunk (#755/#756 stay open as recorded
   non-goals; do not close or reopen anything).
5. No push, no PR — shipping is stage 3's last task. Never push to or
   merge main.
6. Backlog issues autonomously (AGENTS §7); list the numbers in your
   report.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the GUI review round 7
implementation (branch feat/ui-r7-files-board-roster).

1. Read AGENTS.md, the origin plan §3 D-7.3 (+ the accessibility
   ledger), issue #723, the dispatch plan §1/§4 (the code-quality
   mandate applies to you), and the stage-1 plan; then review the full
   branch diff (git diff main...HEAD) in
   ../taskboard-WT-feat-ui-r7-files-board-roster.
2. Functional focus: per-account attribution must be correct on the
   multi-account matrix (anchor card vs non-anchor vs lane-off — a
   badge computed from a connector-name match instead of the anchor
   join is a finding); the sort must be deterministic (title, id ties)
   and per-account; Stop Sync must refuse exactly Files-typed bindings
   (a Deck-bound board hitting Stop is a mis-targeting bug — the
   stale-index law covers the UI side); Publish must refuse Deck-bound
   and Files-bound boards typed and must reuse the ceremony's assign
   step (a reimplemented bind path is an SRP violation); the files
   actions must be disabled (not hidden) on non-anchor cards with the
   restart caption carrying the why; the picker retirement must leave
   zero dead references (copy global, choices table, seam, component);
   the row action buttons must be walk-ordered tab stops; the Stop-Sync
   copy must NOT promise remote deletion (#755 — the subtree lingers);
   the existing move-arm tests must be un-weakened.
3. Code-quality mandate: (copy verbatim from the dispatch plan §1.)
4. Re-run the full gate suite yourself (AGENTS §10, through
   scripts/run-quality-gates.sh); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no
   duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then
   gh pr checks --watch; any failing check is a finding — diagnose,
   fix, push --force-with-lease, repeat until green; verify
   mergeStateStatus CLEAN per the dispatch plan §1.
7. Backlog issues autonomously (AGENTS §7); list the numbers.
8. Teardown: from the project root, git -C /home/viktoria/src/taskboard
   worktree remove --force
   ../taskboard-WT-feat-ui-r7-files-board-roster; confirm via
   git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR
URL, green-checks confirmation, worktree-removed confirmation. Never
merge; never touch main.
```

## 5. Completion bookkeeping

- **No tracker issues auto-close from this round**: the three round-7
  observations have no tracking issues of their own (the review list is
  the origin); the chunk PRs describe the retired observation in their
  bodies. #755 (stop-sync subtree leftover) and #756 (Deck→Deck move)
  stay open as recorded non-goals — chunk 3 must not close them.
  #750 (adoption-state badges) gets its fold-in assessed as a small
  follow-up once chunk 3's row grammar lands; comment on it from chunk
  3's stage 3 with the PR link rather than closing.
- **Deliberately not dispatched here**: #723 (true multi-account files
  sync — non-anchor cards stay action-disabled), #718 (the DELETE verb
  GC — #755 is its stop-sync consumer), habit target-editing UI beyond
  create, and the round-7 plan's §4 non-goals list in full.
- **What opens next**: once all three chunks merge, re-verify the whole
  surface against the origin plan's §2 table (the next GUI review
  round's input); chunk 3's roster row grammar is the seam #750's
  adoption-state fold rides.
