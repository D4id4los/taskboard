# [Plan] GUI Review Round 8 ("ux-r8") — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-16
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 8 code chunks implementing the
  accepted round-8 UX journey plans. No code in this commit.
- **Origin**: the accepted round-8 plan
  `.artifacts/plans/2026-11-16-gui-review-round8-plan.md` (§1 notes
  N-1..N-23 are the authority record; §2 the family map; §3 the
  ruling/collision ledger C-1..C-8; §4 the coverage table; §7 the
  chunking proposal — all normative) and the eight accepted UX plans
  it cites (`2026-11-16-{board-interaction,task-card-controls,task-detail-editor,focus-restoration,keymap-parity,files-sync-status,tag-color,cross-surface-friction}-ux-plan.md`,
  each ACCEPTED 2026-11-16; their §2 matrices, §3 stories, §4 steps,
  §6 wiring rows, §7 deltas, and rulings are the chunks' normative
  IDs), plus the user's 2026-11-16 acceptance ("the plan looks good").
  **Prompt structure follows `docs/agent_dispatch.org`** (the
  agent-dispatch discipline, PR #852) and the amended dispatch
  template — the long-command discipline, the stage-3 scope split,
  dispatcher stage-exit verification, and resume-over-redo below are
  that law's, restated.
- **Status**: READY — all 8 chunks are dispatchable from accepted
  plans. Default order 1→2→3→4→5→6→7→8; chunk 6 may run in parallel
  with 3–5 once 2 has merged (disjoint files: settings/status-bar/
  files crate vs the editor/board surfaces). **Deliberately not
  dispatched here**: the six standing triage deferrals (#807, #809,
  #812, #813, #819, #820 — round plan C-6; their r6 reasons stand,
  the issues stay open without milestones); the machinery enablers
  #715/#716/#762 (fold rules in §10); the framework-debt riders'
  dedicated work (#768 stays homed; #782/#760/#775 fold into every
  chunk per §1's mandate).

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | keymap-parity plan (all) | `fix/ui-r8-keymap-parity` | emacs motion rows (presets.rs), the punctuation keysym normalization (keymap parser), the M-< M-> menu-route fix (adapter/router), the detail shortcut ids (TaskDueEdit/TaskScheduledEdit/TaskDescriptionEdit + tracking/rule-add preset rows). Non-goals: no vi redesign, no Standard re-binds, no three-stroke chords | keymap-parity US-1..US-3 (all steps), W-1..W-3, R-1/R-2 | M |
| 2 | focus-restoration plan (all) | `fix/ui-r8-focus-restoration` | the focus-origin fact + restore seams over every overlay site (the census table is a stage-2 deliverable), the Tab-trap clamp, the menu focused-open, the Today walk handoff, title/description focus flags, the notification-surface keyboard grammar (new context + ids). Non-goals: no theme work, no caret history, no alarm-window work (R-2) | focus-restoration US-1..US-6 (all steps), W-1..W-6, R-1..R-3 | L |
| 3 | task-detail-editor plan (all) | `fix/ui-r8-task-detail-editor` | the `days::` date-only stamping law (domain, pure) + the due_edit/CLI funnels, the HH:MM focus fix + quick-options autoscroll, the effort detail row, the paused_until preservation (#759), the restoration guard (#706), the labels armed-identity clear (#840). Non-goals: no stored-instant migration (R-1), no CLI grammar change | task-detail-editor US-1..US-6 (all steps), W-1..W-7, R-1..R-3 | M–L |
| 4 | board-interaction plan (all) | `feat/ui-r8-board-interaction` | the move flow's position slots, the gap drop grammar, the compact segment drop targets, the compact title dropdown, the selection VM + multi-verb menu grammar, the move-aim bit, the roster glyph. Non-goals: no inline editing (T-26 stands), no kiosk work, no batch verb (unless walkthrough demands — filed, not built) | board-interaction US-1..US-7 (all steps), W-1..W-7, R-1..R-4 | L |
| 5 | task-card-controls plan (all) | `feat/ui-r8-task-card-controls` | the quick-actions bar (three new schedule ids + tracking twin + preset rows), the applicable-row filter + submenu grammar, the context-popup model pin (the N-11 fix, clock-tested), the card play/pause control, the create-focus handoff, the hover flag, the compact field height fix. Non-goals: TaskDueToday's scope untouched (R-1), no inline editing, no batch verb | task-card-controls US-1..US-7 (all steps), W-1..W-7, R-1..R-4 | L |
| 6 | files-sync-status plan (all) | `fix/ui-r8-files-sync-status` | the chip tone folds, the N-19 investigation→fix (test-first), the auto-list backoff, per-key env-lock captions, the card-local publish phase, the undo-window Restore, the #749 re-verification, the adoption phase, the publish-picker site, the Deck-to-Deck rebind (UiRequest + `RebindBoard`). Non-goals: no #723 lanes, no GC/leftover work, no new Tones | files-sync-status US-1..US-10 (all steps), W-1..W-9, R-1..R-3 | L |
| 7 | tag-color plan (all) | `feat/ui-r8-tag-color` | `Color::parse` in domain (A1) + the CLI refactored onto it, the overlay's free-form colour field, the board-menu roster-only mount (`LabelManage` widened), the card chips. Non-goals: no card-chip assignment (R-2), no colour derivation | tag-color US-1..US-3 (all steps), W-1..W-3, R-1..R-3 | M |
| 8 | cross-surface-friction plan (all) | `fix/ui-r8-cross-surface-friction` | the dropdown empty-model guard, the conversion-arm routes, the settings view-switch disarm, the `search` alias, the per-action one-shot consumption set, the Replace chain pre-pass. Non-goals: #762/#715/#716 stay on the machinery lane, no renames beyond the alias | cross-surface-friction US-1..US-6 (all steps), W-1..W-6, R-1/R-2 | M |

**P1-slice rule check (valid at planning time)**: every plan's P1
stories are claimed whole by their chunk (the UX-coverage column
claims all steps, escapes included), so the chunk set completes every
P1 journey end-to-end. Cross-chunk composition points are named where
they exist: chunk 5's quick-bar keys ride chunk 1's preset rows;
chunk 5's bar composes with chunk 4's selection grammar (W-5's
multi-selection branch); chunk 3's overlays ride chunk 2's restore
seams; chunk 8's dropdown guard sees the final grammar from chunks
4/5.

**Dependency graph**: 1 and 2 both touch `actions.rs`/`presets.rs` →
serial 1→2. 3 and 4 each ride 2's restore/selection infrastructure →
3, 4 after 2. 5 rides 1's keys and 4's selection grammar → after 4.
7's board-menu row rides 4's board grammar → after 4. 8 is last (it
sees the final Dropdown and route tables). 6 is file-disjoint from
3–5 and may run parallel once 2 has merged. No PR stacking: every
chunk branches off the then-current `main` (fetched `origin/main` if
the local checkout is behind — the day-boundary precedent) after its
predecessor's PR merges.

**Dispatch mechanics**: one agent session per prompt, in order (N.1 →
N.2 → N.3 within a chunk); independent chunks may run concurrently as
separate sessions; never two agents share a worktree. Every chunk
works in `../taskboard-WT-<branch>` next to the project root (AGENTS
§8), branched off the then-current `main`, no PR ever stacked.
**Branch names are never reused after their PR lands** (AGENTS §8,
issue #415): a resume/second round of a chunk works on
`<branch>-v2`, never on the landed branch. Create worktrees with
`git worktree add -b <branch> ../taskboard-WT-<branch>` (never
without `-b`, never `<path> main`). Disk hygiene: any agent creating
or recreating a worktree runs `scripts/worktree-doctor.sh` from the
main checkout first — nonzero exit stops the chunk.

**Shared execution model** (binding for every chunk):

**Long-command discipline** *(binding for every chunk, every stage —
`docs/agent_dispatch.org`; PR #852)*: a subagent is killed after
~600s without observable activity, so long silent commands kill
working agents.

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
   `mergeStateStatus CLEAN` verification, teardown, and at-merge
   issue closures (the per-chunk closure lists are in §10).
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim* (every prompt
   below carries the block).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (`git log origin/main..HEAD` — commits present, tree clean, gate
   evidence re-runnable). A stalled or quota-cut stage is completed
   from its landed commits or resumed with a state-check message
   ("commit X is landed; continue from Y") — **resume over redo**,
   never re-dispatched blind. The failure playbook:
   `docs/agent_dispatch.org` §8 (coverage-run stalls, CI-watch
   stalls, quota cuts, CI flakes on re-run, worktree anomalies and
   the foreign-content check).

**Coverage-gate steady state (per chunk, stated once here and in each
stage-2/3 prompt)**: round-8 plans are framework-era — the honest
green is **every wiring row and step token FOUND** under
`scripts/ux-coverage.sh <plan> --gate`. A MISSING on a claimed row is
a plan-vs-code defect: fix code or plan until FOUND; never weaken a
row. Rows whose Commands cell is `—` carry their reason in the
journey-steps column (the reason law) and must resolve as FOUND.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: doctor first, create the worktree on
   the chunk branch off current `main`, write the TDD-first chunk
   plan at `.artifacts/plans/2026-11-17-ui-r8-<topic>-plan.md` in the
   house `[Plan]` style — resolving every seam name, test name, and
   open design question the origin left as an assumption, each as a
   recorded decision — commit it (`docs(plan): …`). No implementation,
   no push. Origin sections/stories/rows marked normative are
   binding; deviations need a recorded reason.
2. **Stage 2 — implementation agent**: continue in that worktree
   (recreate on the branch if missing; `<branch>-v2` if the branch's
   PR landed), implement per the approved chunk plan, TDD, every
   commit standalone; embed journey-step tokens in test names exactly
   as the plan's §6 rows name them; update the census cells the plan's
   §9 lists (same commits as the code that makes them true); run
   `scripts/ux-coverage.sh <plan> --gate` (every claimed row FOUND)
   plus the fast gate per the discipline above. Does not push or open
   a PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `origin/main`
   (rebase dance; `push --force-with-lease` only if history rewrote),
   reviews the full post-rebase branch diff against the chunk's
   acceptance + the code-quality mandate below, re-runs the gate
   steady state and the fast gate, fixes every finding on the branch,
   then pushes and opens the PR (`gh pr create`) and **stops** — the
   dispatcher owns checks-watch, CLEAN, teardown, closures. When the
   origin cites a UX plan (all eight do), stage 3 additionally carries
   the **UX completeness mandate**: re-run `scripts/ux-coverage.sh
   <plan> --gate` (every claimed row FOUND); execute the walkthrough
   protocol (`docs/ux_planning.org` §2.4) over every P1 story — the
   four questions per step with evidence, then the six-heuristic pass
   over the touched surfaces; BLOCKER/MAJOR fixed on the branch,
   MINOR to backlog; the doctrine bar (`ux_planning.org` §2.6) is the
   regression bar. Never merges; never touches `main`.

**Code-quality mandate** (stage 3, every chunk — implementation
chunks): beyond functional review, evaluate (a) code quality: naming,
comment discipline (comments state constraints, not narration),
idiomatic use of the approved stack, no dead code or speculative
generality, test quality per AGENTS §5; (b) the Single Responsibility
Principle: the crate-role boundaries of AGENTS §2 are the SRP line —
a boundary violation is an SRP violation; (c) a single level of
abstraction per function body: a function either orchestrates named
operations or works in primitive data, never mixes. Findings are
fixed on the branch like any other finding.

**Round-8 standing laws** (cross-cutting, binding on every stage,
restated in every prompt): (a) typed outcomes only — never label,
log, or UI text assertions (AGENTS §5); (b) time-dependent tests use
`tokio::time::pause()`/`advance()` — never sleeps (the N-11 menu
dwell and the N-19 sync polling are clock-tested); (c) census updates
ride the code commits, citing the owning plan's US/W ids; every new
slint seam carries the `// slint: <callback>` comment and any
pre-framework seam a gate row must resolve gains one (#782 fold);
(d) the doctrine bar (`docs/ux_planning.org` §2.6) is the regression
bar — grey-within-applicable (BF-W-N2) beside the
withheld-when-inapplicable grammar (round plan C-4); (e) the focus
contract: every overlay exit restores the invoking element or its
container anchor (focus-restoration US-1) — any overlay a chunk
builds or touches composes with it.

Shared prompt blocks (the canonical texts — **embedded verbatim
in every stage prompt below**, per the dispatch law's
restatement rule; repeated here once for the record):

- **[D] Discipline**: "Long-command discipline (binding;
  docs/agent_dispatch.org): the coverage trio never runs inside a
  subagent — your gate is the pre-commit hooks plus
  scripts/run-quality-gates.sh cargo nextest run --workspace
  (flock-safe, ~4–6 min; background + poll with useful work between
  polls beyond ~5 minutes, and commit all landed work BEFORE any
  wait). [Stage 3 only:] Your scope ends at PR-open: push, open the
  PR with gh, report the URL — the dispatcher owns gh pr checks
  --watch, the mergeStateStatus CLEAN verification, teardown, and
  at-merge issue closures."
- **[L] Laws**: "Round-8 standing laws (binding): (a) typed outcomes
  only — never label/log/UI text assertions (AGENTS §5); (b)
  time-dependent tests use tokio::time::pause()/advance(), never
  sleeps; (c) census updates ride the code commits citing the owning
  plan's US/W ids, and every new slint seam carries a `// slint:
  <callback>` comment (add one to any pre-framework seam your gate
  rows must resolve — #782); (d) the doctrine bar
  (docs/ux_planning.org §2.6) is the regression bar — grey-within-
  applicable beside withheld-when-inapplicable (round plan C-4);
  (e) the focus contract: every overlay exit restores the invoking
  element or its container anchor (focus-restoration plan US-1) —
  any overlay you build or touch composes with it."
- **[R] Doctor/worktree**: "Run scripts/worktree-doctor.sh from
  /home/viktoria/src/taskboard first (nonzero = below the disk floor:
  STOP and report), then git worktree add -b <branch>
  ../taskboard-WT-<branch> off the then-current main (never without
  -b, never on main — AGENTS §8)."
- **[B] Backlog**: "Backlog issues: open/comment on `backlog:` GitHub
  issues autonomously for any out-of-scope finding (AGENTS §7
  standing authorization) — 'no push' never means 'no issues'; list
  the issue numbers in your report."
- **[X] Gate steady state**: "scripts/ux-coverage.sh <plan> --gate:
  every claimed wiring row and step token FOUND (framework-era plan);
  a MISSING on a claimed row is a plan-vs-code defect — fix code or
  plan until FOUND; `—`-command rows resolve via their journey-steps
  reason."

## 2. Chunk 1 — keymap parity (`fix/ui-r8-keymap-parity`)

- **Covers**: keymap-parity plan §3 US-1 (the emacs motion rows:
  TaskDetail C-n/C-p item-walk members; TaskDetail C-f/C-b and
  Settings C-n/p/f/b control-walk rows; Board/Today C-f/C-b
  uniformity candidates), US-2 (the M-< M-> menu-route fix — the
  TaskDetail no-op absorb — and the punctuation keysym
  normalization; the parser change is additive, R-2), US-3 (the
  detail shortcut ids `TaskDueEdit`/`TaskScheduledEdit`/
  `TaskDescriptionEdit` + preset rows for `ReminderRuleAdd`/
  `TaskTrackStart`/`TaskTrackStop`; the C-5 collision rule governs
  `C-c C-s`), §6 W-1..W-3, §7 delta. Files: `src/keymap/presets.rs`,
  `src/actions.rs`, `src/router.rs`, `src/keymap/` (the parser),
  `src/adapter.rs` (the menu-invocation resolution), the keymap
  contract tests + snapshots. Explicit non-goals: no vi redesign, no
  Standard re-binds, no three-stroke chords.
- **Acceptance**: the contract suite green with the new rows bound in
  all three presets (no dropped row — a C-5 retune is recorded);
  parser unit tests pin the normalization cases and the additive
  fall-through; `us2_s1_menu_first_last_matches_the_key_route` proves
  menu ≡ key; the step-token rows of §8 all FOUND; snapshots
  regenerated; census `nav.*` + detail-verb evidence updated; the
  TE-W-N2 retirement note recorded.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the ux-r8 dispatch ("keymap parity", branch fix/ui-r8-keymap-parity) in the taskboard repo at /home/viktoria/src/taskboard.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md (incl. §7's dispatch-law pointer), then the accepted origin .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md (ALL sections normative: §3 US-1..US-3 with acceptance AC-1..AC-3/AC-1..AC-3/AC-1..AC-4, §4 step tables, §6 W-1..W-3, §7 delta, §8 test names) and the dispatch plan .artifacts/plans/2026-11-16-gui-review-round8-dispatch-plan.md §1/§2.
2. Read the seams: src/keymap/presets.rs (the emacs rows — Board/Today C-n/C-p at ~433-451, TaskDetail Alt+< / Alt+> at ~561-571, the occupied C-c chords ~425-431), src/keymap/'s key parser (how a preset token matches an event: glyph vs base-key+modifiers — the N-17 hypothesis), src/router.rs (NavFirstItem/NavLastItem routes — why a TaskDetail menu invocation absorbs), src/actions.rs (contexts), and the keymap contract tests you must not weaken.
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the TDD-first chunk plan at .artifacts/plans/2026-11-17-ui-r8-keymap-parity-plan.md. Settle at minimum, each as a recorded decision: the final preset rows per preset (candidates → contract-suite resolutions; any C-c C-s / ReminderSnooze resolution per round plan C-5 — context precedence or a recorded retune, never a dropped row); the parser normalization's exact mechanism (base-key+modifiers vs keysym map) and its additive guarantee; the menu-invocation item-resolution fix for NavFirst/LastItem in TaskDetail; the three new ActionIds' registry entries, contexts, and per-preset keys; the test catalogue (parser unit cases, the menu≡key journey, the shortcut arm-parity journeys) with the §8 names; commit sequencing.
5. Commit it as docs(plan): the ui-r8 keymap-parity chunk plan. Do not implement; do not push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions (esp. the C-5 resolution and the parser mechanism), risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the ux-r8 dispatch ("keymap parity"). Work in ../taskboard-WT-fix-ui-r8-keymap-parity (branch fix/ui-r8-keymap-parity); recreate it on that branch if missing (never <path> main; <branch>-v2 if the PR landed).

1. Read AGENTS.md, the origin plan 2026-11-16-keymap-parity-ux-plan.md, the dispatch plan §1/§2, and the stage-1 chunk plan 2026-11-17-ui-r8-keymap-parity-plan.md (binding; deviations need a recorded reason in your report).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone: the preset rows (all three presets, snapshots regenerated), the parser normalization + unit cases, the menu-invocation route fix, the three new ActionIds (registry entries, contexts, hints registry-generated), the preset rows for ReminderRuleAdd/TaskTrackStart/TaskTrackStop. The contract suite must stay green — a colliding stroke is re-picked from the same context's free set and recorded, never a dropped row.
4. Update the census cells the origin §9 lists (nav.item_walk / nav.control_walk evidence, the detail verbs' key evidence, the TE-W-N2 retirement note) in the same commits as the code.
5. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results (gate rows FOUND count), deviations.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the ux-r8 dispatch ("keymap parity", branch fix/ui-r8-keymap-parity). Work in ../taskboard-WT-fix-ui-r8-keymap-parity.

1. Read AGENTS.md, the origin plan 2026-11-16-keymap-parity-ux-plan.md, the dispatch plan §1/§2 (the code-quality mandate applies to you), and the stage-1 chunk plan; rebase onto origin/main (fetch --prune, rebase, push --force-with-lease only if rewritten); review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-keymap-parity-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): no preset row was dropped or silently retuned (the C-5 resolution is recorded and snapshotted); the parser normalization is additive — the pre-existing punctuation tokens' behavior unchanged (contract snapshots prove it); menu ≡ key for NavFirstItem/NavLastItem in TaskDetail (us2_s1 journey) with the empty-set NoTarget surfaced, never silent; the three new ids bind in all three presets with registry-generated hints; no weakening of the keymap contract tests.
4. Code-quality mandate (verbatim from the dispatch §1): naming and comment discipline, SRP crate boundaries (the parser stays in the keymap module; the router fix in routing), one abstraction level per body, no speculative generality.
5. Re-run the gate steady state and the fast gate; any failure is a finding. Fix everything on the branch (amend/reword).
6. Walkthrough protocol over US-1..US-3 (the four questions per step with evidence; six-heuristic pass over the affected views; the doctrine bar held).
7. Ship: push, gh pr create (title "fix(ui): keymap parity — emacs motions, M-</M-> routes, task-action shortcuts"; body citing the origin plan + N-1/N-2/N-17/N-21), report the URL — then STOP (the dispatcher owns checks-watch, teardown, closures).
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings (functional + mandate), fixes, PR URL. Never merge; never touch main.
```

## 3. Chunk 2 — focus restoration & modal containment (`fix/ui-r8-focus-restoration`)

- **Covers**: focus-restoration plan §3 US-1 (the focus-origin fact +
  restore at every exit arm, per overlay site — the site census table
  is a stage-2 deliverable and its enumeration is the test list),
  US-2 (the Tab clamp at the mask boundary; insert-mode text grammar
  untouched), US-3 (the menu focused-open), US-4 (the Today walk
  single handoff), US-5 (the title/description focus flags + the
  census rider), US-6 (the notification-surface walk + per-verb ids,
  the r5 alarm grammar's twin), §6 W-1..W-6, §7 delta
  (`ContextId::NotificationSurface` + the row-verb ids). Files:
  `src/adapter.rs`, `src/view_state.rs`, `src/actions.rs`,
  `src/keymap/presets.rs`, the overlay hosts (`ui/app-window.slint`,
  the compositions), `ui/compositions/notification-surface.slint`,
  `ui/views/task-view.slint`. Non-goals: no theme work, no caret
  history, no alarm-window changes (R-2).
- **Acceptance**: every census site restore-tested (no site without a
  journey test — AC-4's enumeration); `us2_s1` wrap + the degenerate
  case; `us3_s1` focused-open; `us4_s1` cross-board parking;
  `us6_s2` verb-id ≡ button-command structural equality; the
  notification ids bound in every preset (totality); the RW-W-N1
  Escape-preset positive intact; gate rows FOUND; census rows minted
  (R-1's scoping).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the ux-r8 dispatch ("focus restoration", branch fix/ui-r8-focus-restoration) in /home/viktoria/src/taskboard.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md (all sections normative: US-1..US-6 with their ACs, §4 tables, §6 W-1..W-6, §7 delta incl. the focus-origin fact's contract, rulings R-1..R-3) and the dispatch plan §1/§3. Also read docs/agent_dispatch.org §5 (the discipline you are given in prompts is the law's).
2. Read the seams: the overlay open/dismiss funnels in src/adapter.rs (due-editor, recur editor, labels overlay, priority pickers, track entries, action menu, palette, capture, timer setter, focus overlay, notification surface, walk overlay — enumerate the open/close pairs; THIS enumeration becomes the census table), the CONTROL_NAV walk (src/actions.rs, the mask contexts incl. the r5 ContextId::AlarmWindow grammar as the US-6 template), the Today walk handoff (src/view_state.rs / adapter refocus-view), task-view.slint's title/description field models, notification-surface.slint's row buttons and their dispatches.
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-focus-restoration-plan.md. Settle at minimum: the focus-origin fact's exact typed shape (seam id + row identity; one-shot, last-open-wins) and its recording/restoration seams per site family; the census table (site → open seam → exit arms → test name — complete, no "decide later"); the walk clamp mechanism (context-bound boundary; the RW-W-N1 Escape preset must not regress); the menu focused-open seam; the walk handoff fix (suppress the second refocus); the new NotificationSurface context + per-verb ids and their per-preset keys (candidates; contract-suite oracle); the focus-flag plumbing for the title/description fields; the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 focus-restoration chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, the census table headline (site count), decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the ux-r8 dispatch ("focus restoration"). Work in ../taskboard-WT-fix-ui-r8-focus-restoration (branch fix/ui-r8-focus-restoration).

1. Read AGENTS.md, the origin plan 2026-11-16-focus-restoration-ux-plan.md, the dispatch plan §1/§3, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) the focus-origin fact + the restore calls across every census site (the census table rides commit 1 — its enumeration is the plan's test list; a site discovered mid-work joins the table and gets its test, never silently skipped); (2) the Tab clamp (walk boundary; text insert grammar untouched); (3) the menu focused-open; (4) the Today walk single handoff; (5) the focus flags; (6) the notification-surface context + ids + walk + per-preset bindings (the r5 D-7 grammar verbatim; click ≡ key structural equality tests).
4. Update the census cells the origin §9 lists (the three new app.* rows minted with R-1, nav.item_walk evidence, indicator evidence, notification keyboard evidence) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, the census table's final site count, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the ux-r8 dispatch ("focus restoration", branch fix/ui-r8-focus-restoration). Work in ../taskboard-WT-fix-ui-r8-focus-restoration.

1. Read AGENTS.md, the origin plan 2026-11-16-focus-restoration-ux-plan.md, the dispatch plan §1/§3 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-focus-restoration-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the census table is complete — every overlay site in the codebase appears with its restore test; no site without a test and no test without a site; the restore is one-shot last-open-wins and the vanished-origin fallback lands on the container anchor, never no-control; the Tab clamp governs the walk only — the RW-W-N1 command-free Escape preset and the insert-mode text grammar are untouched; us6_s2's verb-id ≡ button-command equality is structural (no text); the new context is a total mask with per-preset totality proven; the vanished-origin fallback branch is tested (us1_s2).
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-6 (drive at least the picker-close and notification journeys through the driver; the doctrine bar held).
7. Ship: push, gh pr create (title "fix(ui): focus restoration and modal containment — the N-3 rule app-wide, #668 #669 #670 #724"), report the URL — then STOP (dispatcher owns the rest).
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 4. Chunk 3 — task detail editor (`fix/ui-r8-task-detail-editor`)

- **Covers**: task-detail-editor plan §3 US-1 (the `days::` date-only
  stamping law — domain-pure — plus the due_edit funnel and the CLI
  `--due/--scheduled` parity), US-2 (the HH:MM focus fix + the
  quick-options focused slot), US-3 (the effort detail row + the
  walk-parse parity), US-4 (the paused_until preservation — find the
  drop layer first, reproduction test first), US-5 (the restoration
  guard), US-6 (the armed-identity clear, #840), §6 W-1..W-7, §7
  delta (the domain function; no message change). Files:
  `crates/taskboard-domain/src/days.rs`, `src/due_edit.rs`, the
  datetime composition + `ui/compositions/due-editor.slint`,
  `ui/views/task-view.slint` (effort row),
  `ui/compositions/labels-overlay.slint`, `src/adapter.rs`,
  `src/view_state.rs`, `crates/taskboard-app/src/commands/tasks.rs`.
  Non-goals: no stored-instant migration (R-1), no CLI grammar
  change, no recur-editor redesign.
- **Acceptance**: the domain proptest (every date × legal day start)
  green; `us1_s3` CLI≡GUI instant parity; the N-4 law lands with the
  day-end decision's recorded posture (the operator may overturn —
  the plan's §1 note); `us2_s1` stays-open fact; `us4_s1`
  preservation round-trip over both fronts; `us5_s1` fallback; `us6`
  rung count; effort masks exact + parity; gate rows FOUND.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of the ux-r8 dispatch ("task detail editor", branch fix/ui-r8-task-detail-editor) in /home/viktoria/src/taskboard.

Gated precondition: chunk 2's PR (fix/ui-r8-focus-restoration) is merged — verify with git log origin/main; if not, STOP and report.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md (all sections normative; §1's day-end decision is the recorded posture — derive from day_start+24h, no second knob) and the dispatch plan §1/§4. Also read .artifacts/plans/2026-10-21-daily-driver-day-boundary-plan.md §2 D4/D5 (the days.rs purity law and proptest shapes you inherit).
2. Read the seams: days.rs (day_window; the consumers contract), due_edit.rs (the parse: date-only branch at ~133 stamping 00:00 UTC; parse_time_lenient/step_minutes/quarter_slots; format_entry), the datetime slots composition (date-time-slots / date-time-nearest-row), the HH:MM fields' TouchArea/funnel ordering (why an intra-editor click dismisses — N-5), the labels overlay's delete seams (the #840 rung accounting), the recur set path (where paused_until is dropped — mint vs setter; #759), the restoration funnel (view_state over-view restore; #706), TaskChanges' effort mask + the walk's effort parse (the parity predicate), and the CLI tasks edit due/scheduled parse (the parity twin).
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-task-detail-editor-plan.md. Settle at minimum: the domain function's exact signature/name and its proptest catalogue (pure: no chrono::Local/Utc::now — the day-boundary grep gate applies); the funnel plumbing (typed day facts into due_edit + the CLI call sites); the +Nd/quick-set forms' coverage; the HH:MM fix mechanism (focus ordering vs funnel branch); the quick-options focused-slot derivation; the effort row's editor shape (seeded buffer, exclusivity with the date editors) + the shared parse predicate's home; the #759 root-cause finding plan (reproduction test FIRST — mint vs setter); the restoration guard's typed shape; the armed-identity clear's exact seam; the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 task-detail-editor chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions (esp. the domain signature and the #759 hypothesis), risks, estimated diff size.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of the ux-r8 dispatch ("task detail editor"). Work in ../taskboard-WT-fix-ui-r8-task-detail-editor (branch fix/ui-r8-task-detail-editor).

1. Read AGENTS.md, the origin plan 2026-11-16-task-detail-editor-ux-plan.md, the dispatch plan §1/§4, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) the domain law (proptests + unit examples FIRST — the grep gate `grep -rn "chrono::Local\|Utc::now" crates/taskboard-domain/src/days.rs` must stay empty); (2) the GUI funnel + picker day-click + quick-set forms (exact-mask seam tests; the instant parity with the CLI); (3) the CLI twin refactored onto the same law (app-arm parity test us1_s3); (4) the HH:MM focus fix + quick-options slot (us2 journeys); (5) the effort row (masks + refusal + the walk-parse parity); (6) #759: the reproduction test first (watch it fail), then the preservation fix at whatever layer drops paused_until, then the round-trip over both fronts; (7) the restoration guard (#706); (8) the armed-identity clear (#840, us6 rung tests).
4. Update the census cells the origin §9 lists (the stamping-law evidence on due/scheduled incl. the CLI note, the effort row minted closing #838, the recur preservation, the restoration guard, the label.delete evidence) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, the #759 root cause found, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the ux-r8 dispatch ("task detail editor", branch fix/ui-r8-task-detail-editor). Work in ../taskboard-WT-fix-ui-r8-task-detail-editor.

1. Read AGENTS.md, the origin plan 2026-11-16-task-detail-editor-ux-plan.md, the dispatch plan §1/§4 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-detail-editor-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the domain law is pure (the days.rs grep gate empty; the proptest genuinely covers the date × day-start product); the explicit-time forms are unchanged (us1_s2 — the law is date-only); the CLI and GUI mint the identical instant (us1_s3 asserted on the instant, not the phrase); #759's reproduction test exists and failed before the fix (history shows it); the preservation does not wedge resume (the existing resume tests stand); the TE-W-N3 staged-clear echo survives the funnel change (doctrine bar); the effort masks are sparse-exact; #840's rung tests pin the cleared identity on BOTH the double-click and the cancel paths.
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-6; the doctrine bar held.
7. Ship: push, gh pr create (title "fix(ui): the date-only day-boundary law and editor repairs — N-4/N-5/N-6, #838 #759 #706 #840"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 5. Chunk 4 — board interaction (`feat/ui-r8-board-interaction`)

- **Covers**: board-interaction plan §3 US-1 (the move flow's
  position slots), US-2 (the gap drop grammar), US-3 (the compact
  segment drop targets, R-3 append), US-4 (the selection VM +
  modifier gestures + the multi menu + the per-task verb sequences),
  US-5 (the move-aim bit + insertion indicator), US-6 (the compact
  title dropdown), US-7 (the roster glyph), §6 W-1..W-7, §7 delta
  (MoveTask usage notes; no new commands). Files:
  `ui/compositions/board-layout.slint` (both legs),
  `ui/compositions/board-bar.slint`, `ui/widgets/task-card.slint`,
  `ui/app-window.slint`, `src/adapter.rs`, `src/view_state.rs`,
  `src/models.rs`, `src/presentation.rs`, `src/menu.rs`. Non-goals:
  no inline editing (T-26 stands, C-6), no kiosk work, no batch verb.
- **Acceptance**: `us1_s3` exact-slot MoveTask; the in-place no-ops
  (US-2/AC-2, US-3/AC-2); `us4` selection journeys (build, drag-all
  in anchor order, multi-menu composition, Escape-clears-nothing-
  dispatched); the stale-frame absorbs; `us5_s1` bit+label;
  `us6_s2/s3` compact dropdown; the glyph flag; gate rows FOUND;
  golden re-baseline contained.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of the ux-r8 dispatch ("board interaction", branch feat/ui-r8-board-interaction) in /home/viktoria/src/taskboard.

Gated precondition: chunk 2's PR is merged (git log origin/main) — else STOP and report.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-board-interaction-ux-plan.md (all sections normative; rulings R-1..R-4) and the dispatch plan §1/§5.
2. Read the seams: the task move flow (src/view_state.rs — Flow arms, slot cycling, the ring; where position slots join), the card drag drop resolver (src/adapter.rs — what order a drop passes today; the staleness re-checks), the compact leg (board-layout.slint — compact-active, the switcher segments, selected-column, the compact task viewport's drag handling), the board bar (segment seams, the dropdown sites incl. the r4 US-2/S4 target-free flow and the archived roster site), the context-menu hide-set (open_task_context_menu — the #492 filter chunk 5 extends), the FocusScrollHint/selected-card machinery (the create-focus precedent for chunk 5, but note the compact scroll behavior you need for US-4 drops), the id tables (stack/task/board), the golden visuals suite (what re-baselines).
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-board-interaction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-board-interaction-plan.md. Settle at minimum: the position-slot grammar (slot enumeration, order computation over the engine's sort orders — presentation-side id/direction math only); the gap drop target model (hover states, the indicator's data path); the compact segment drop (highlight + append + the no-op guard); the selection VM's typed shape (adapter-owned; anchor identity; the range definition over board reading order; the clear paths) and its composition with the context menu (the multi grammar's row set; where the withheld rule plugs in — chunk 5 builds the menu, you build the selection branch); the multi verb sequences' dispatch order + per-task absorb; the move-aim bit's model path; the compact title dropdown's anchor; the glyph flag; the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 board-interaction chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of the ux-r8 dispatch ("board interaction"). Work in ../taskboard-WT-feat-ui-r8-board-interaction (branch feat/ui-r8-board-interaction).

1. Read AGENTS.md, the origin plan 2026-11-16-board-interaction-ux-plan.md, the dispatch plan §1/§5, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-board-interaction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone: (1) the move flow's position slots (us1 journeys; the ring/indicator data path); (2) the gap drop grammar + indicator (us2; in-place no-op + stale absorbs); (3) the compact segment drop (us3; append + no-op); (4) the selection VM + modifier grammar + Shift-walk extension (us4_s1/s4; Escape/foreign-settle clears dispatch nothing); (5) the multi-drag sequences + the multi menu branch + the armed multi delete (us4_s2/s3/s5 — per-task commands in anchor order, each journal PreImage); (6) the move-aim bit + badge label (us5); (7) the compact title dropdown (us6; the r4 target-free flow's compact leg) + the roster glyph (us7). Golden visuals re-baseline rides the commits that change the models.
4. Update the census cells the origin §9 lists (task.move evidence, the task.select row minted with R-1/R-2, board.select compact evidence, the move-aim note closing #823, the roster glyph closing #834) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-board-interaction-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of the ux-r8 dispatch ("board interaction", branch feat/ui-r8-board-interaction). Work in ../taskboard-WT-feat-ui-r8-board-interaction.

1. Read AGENTS.md, the origin plan 2026-11-16-board-interaction-ux-plan.md, the dispatch plan §1/§5 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-board-interaction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the selection VM lives adapter-side (zero selection logic in .slint files — AGENTS §2's letter); the multi sequences dispatch one command per task in anchor order with per-task stale absorb (no batch verb appeared); the multi menu withholds single-target rows rather than greying them (C-4) while destructive rows stay last (BF-W-N2); the escapes are real on every new flow (Escape/foreign-settle/vanish clear without dispatch); the compact append rule (R-3) is enforced (no positional drop onto segments); the stale-frame absorbs survive (the width-drag law's twins); the doctrine bar held.
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-7 (drive the reorder and multi-drag journeys through the driver; judge the indicator's readability — manual, never asserted).
7. Ship: push, gh pr create (title "feat(ui): board reorder, compact movement, and multi-select — N-10/N-15/N-16/N-22/N-23, #823 #834"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 6. Chunk 5 — task card controls & the menu grammar (`feat/ui-r8-task-card-controls`)

- **Covers**: task-card-controls plan §3 US-1 (the quick-actions bar
  + `TaskScheduleToday`/`TaskScheduleTomorrow`/`TaskUnschedule` +
  the tracking twin + preset rows), US-2 (the applicable-row filter +
  the four submenus), US-3 (the context-popup model pin — the N-11
  fix, clock-tested), US-4 (the card play/pause control — round plan
  C-1), US-5 (the create-focus handoff), US-6 (the hover flag), US-7
  (the compact field height), §6 W-1..W-7, §7 delta (three new
  ActionIds + the menu grammar; no new commands). Files:
  `src/menu.rs`, `src/adapter.rs` (`open_task_context_menu` + the
  popup seams), the context popup host composition,
  `ui/widgets/task-card.slint`, `ui/compositions/board-layout.slint`
  (compact leg), `src/view_state.rs`, `src/actions.rs`,
  `src/keymap/presets.rs`. Non-goals: TaskDueToday's scope untouched
  (R-1), no inline editing, no batch verb.
- **Acceptance**: the exact sparse masks (every quick action);
  click ≡ key per bar entry; the withheld row-sets per subject state
  (typed compositions) with destructive rows last; the submenu
  Escape ladder; `us3_s1`'s dwell-survival test (paused clock) green
  with the main menu's own behavior unchanged; the card control's
  state flips; the create-focus one-shot; the hover flag; the height
  constraint; gate rows FOUND.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of the ux-r8 dispatch ("task card controls", branch feat/ui-r8-task-card-controls) in /home/viktoria/src/taskboard.

Gated preconditions: chunks 1 and 4 are merged (git log origin/main) — else STOP and report.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md (all sections normative; rulings R-1..R-4 — R-1 keeps TaskDueToday Today-only) and the dispatch plan §1/§6. Cross-chunk inputs (normative where consumed): chunk 1's landed preset rows (the quick-bar keys bind against them), chunk 4's selection branch (W-5's multi grammar composes with your menu).
2. Read the seams: TASK_CONTEXT + the group-page machinery (src/menu.rs — context_entries, group_page, the two-tier dwell path your US-3 pin scopes), open_task_context_menu's hide-set (the #492 kind filter you extend to state), the menu popup host composition + its model globals (where a re-derivation could swap contents mid-open — the N-11 hypothesis to verify), the tracking ids' routes (card-focused), the create funnels (view_state CreateTask + the FocusScrollHint precedent), the checkbox model, the compact leg's add-task field (the surplus-height leak), the day-facts path (the quick actions' window-end instants — chunk 3's law is landed by now).
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-task-card-controls-plan.md. Settle at minimum: the bar's menu-model section (icon kind + id + enabled + visible per entry) and its popup rendering; the three new ids' dispatch funnels (the exact scheduled-window-end instants via the landed day facts); the tracking twin's shown-half rule; the per-subject withheld filter's exact state conditions per family (tracking/reminder/series/habit/timer — the C-4 hidden-vs-grey rule per row); the submenu pages' grammar (child ids/keys preserved; the ladder order); the pin fix (what re-derives today — prove it with the paused-clock reproduction FIRST); the card control's seams; the create-focus handoff (one-shot, scroll-into-view); the hover flag; the height constraint fix; the per-preset key candidates; the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 task-card-controls chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions (esp. the N-11 root cause), risks, estimated diff size.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of the ux-r8 dispatch ("task card controls"). Work in ../taskboard-WT-feat-ui-r8-task-card-controls (branch feat/ui-r8-task-card-controls).

1. Read AGENTS.md, the origin plan 2026-11-16-task-card-controls-ux-plan.md, the dispatch plan §1/§6, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) the N-11 pin (the reproduction test FIRST — open a context menu, tokio::time::advance() past the dwell + a refresh tick, assert the model unchanged; then the fix scoping the dwell to the main menu); (2) the three schedule ids + funnels (exact sparse masks; window-end instants) + the tracking twin; (3) the bar (rendering + seams + preset rows from chunk 1's landed table); (4) the withheld filter + submenus (typed row-set composition tests; the ladder); (5) the card play/pause control (us4 journeys + the one-shot flip); (6) the create-focus handoff (us5; empty-settle hands off nothing); (7) the hover flag + the compact height fix (us6/us7; golden re-baseline).
4. Update the census cells the origin §9 lists (task.scheduled_edit Board U→E, track_start/stop Board U→E per C-1's dated supersession note on r6 D-6, task.create focus evidence, the app.menu task-context evidence) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, the N-11 root cause, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of the ux-r8 dispatch ("task card controls", branch feat/ui-r8-task-card-controls). Work in ../taskboard-WT-feat-ui-r8-task-card-controls.

1. Read AGENTS.md, the origin plan 2026-11-16-task-card-controls-ux-plan.md, the dispatch plan §1/§6 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-task-card-controls-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the dwell test is clock-driven (pause/advance — a sleep anywhere is a finding); the pin's fix does not regress the main menu's two-tier behavior (its own tests stand); the withheld sets match the C-4 rule per family AND the doctrine bar's grey-within-applicable rows still grey (destructive last); the quick actions commit exact sparse masks (every other TaskChanges slot None — the r3 shapes); TaskDueToday's contexts are untouched (R-1 — diff actions.rs to prove it); the tracking twin cannot double-dispatch (one-shot per state flip); the create-focus is one-shot and the empty settle hands off nothing; the menu's count caption composes with chunk 4's selection branch (drive the combined journey).
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-7 (drive the quick-bar and menu journeys through the driver; the SuperProductivity reference is direction, judged at walkthrough).
7. Ship: push, gh pr create (title "feat(ui): the task quick-actions bar and menu grammar — N-7/N-8/N-9/N-11/N-12/N-13, #811"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 7. Chunk 6 — files sync status & the Settings-Files family (`fix/ui-r8-files-sync-status`)

- **Covers**: files-sync-status plan §3 US-1 (the chip folds), US-2
  (the N-19 investigation→fix — the reproduction test precedes the
  fix), US-3 (the auto-list backoff), US-4 (per-key env-lock), US-5
  (the card-local publish phase), US-6 (the undo-window Restore),
  US-7 (#749 re-verify → evidence-close or fix residue), US-8 (the
  adoption phase), US-9 (the picker site), US-10 (the rebind
  ceremony — `UiRequest::MoveBoardToDeck` + `StateCommand::RebindBoard`,
  the round's one engine delta), §6 W-1..W-9, §7 delta. Files:
  `ui/compositions/status-bar.slint`, `src/presentation.rs`,
  `src/models.rs`, `src/adapter.rs`, `ui/views/settings-view.slint`,
  `crates/taskboard-app/src/ui_requests.rs`,
  `crates/taskboard-domain/src/messages.rs` (the delta only),
  `crates/taskboard-sync-files-nextcloud/`. Non-goals: no #723
  lanes, no GC/leftover work, no new Tones.
- **Acceptance**: the five-state fold; the reproduction test exists
  and failed first (US-2/AC-1); the stalled-truth fold; the backoff's
  request-count tests + the refresh override; the per-key bits; the
  phase fold + no full-page disable; the Restore window gates; #749
  disposition recorded (evidence comment or residue fix); the
  adoption phase fold; the picker anchor; `us10_ac1` remote-identity
  preservation + typed refusals; gate rows FOUND.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of the ux-r8 dispatch ("files sync status", branch fix/ui-r8-files-sync-status) in /home/viktoria/src/taskboard.

Ungated: file-disjoint from chunks 3–5; may run in parallel once chunk 2 has merged.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-files-sync-status-ux-plan.md (all sections normative; rulings R-1..R-3) and the dispatch plan §1/§7. Also read docs/files-sync-runbook.org and the r7 files-roster plan's action grammar (the roster row seams your row actions extend).
2. Read the seams: status-bar.slint's chip block (~356-373 — the warning costume on Ok) and its push path (presentation.rs::files_sync_chip_caption, models.rs::files_sync_chip_state); the files-sync actor's board cycle (where pushes derive and write — the N-19 suspects: lane derivation, anchor resolution, the pending counter's source); the per-board pending/state model; the settings mount auto-list (ListRemoteBoards trigger); the snapshot's env facts (per-key granularity for US-4); the publish pending lane (PendingKind, the full-page disable); the move-stage derivation (derive_move_stage — the US-6 window) and the CLI boards move --undo arm (the request twin's shape); the roster rows (Stop Sync — the #749 verification target; the files-action grammar your rebind row extends); the FilesSyncStatus watch (US-8); the publish picker's current anchor (US-9); the Deck binding ceremony (what RebindBoard must preserve — the remote id).
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-files-sync-status-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-files-sync-status-plan.md. Settle at minimum: the chip fold table (tone/flag per state); the N-19 investigation protocol (the hermetic remote fixture's shape; the reproduction test's assertion — pending falls on a successful cycle — and the candidate causes to instrument); the stalled-truth state's fold path; the backoff's typed state + window + the manual-override seam; the per-key caption bits' source; the publish phase's model path + the page-usable guard; the undo request's exact shape + the arm's stage gate; the adoption phase's fold; the picker anchor site; the rebind ceremony's full contract (the request, the arm's precondition list, the RebindBoard payload + journal class, the CLI twin) — this is the round's one domain delta, spell it; the test catalogue per §8; commit sequencing (investigation report → fix → surface work).
5. Commit as docs(plan): the ui-r8 files-sync-status chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions (esp. the N-19 hypotheses ranked and the RebindBoard contract), risks, estimated diff size.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of the ux-r8 dispatch ("files sync status"). Work in ../taskboard-WT-fix-ui-r8-files-sync-status (branch fix/ui-r8-files-sync-status).

1. Read AGENTS.md, the origin plan 2026-11-16-files-sync-status-ux-plan.md, the dispatch plan §1/§7, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-files-sync-status-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) the chip folds (us1; the five-state presentation tests); (2) the N-19 investigation: build the hermetic remote fixture, write the reproduction test (pending falls on a pushed cycle), instrument, find the root cause, record it, fix, watch the test go green (polling paths are clock-tested — pause/advance, never sleeps); (3) the stalled-truth fold (never Syncing against a rising count); (4) the backoff (request-count tests + override); (5) the per-key captions; (6) the publish phase (the page stays usable); (7) the undo-window Restore (window-gated mount; typed refusals); (8) the #749 verification (drive Stop Sync through the seams; record the evidence comment's content for stage 3's dispatcher closure — or fix the residue if the premise partially holds); (9) the adoption phase + the picker anchor; (10) the rebind ceremony (domain delta first: RebindBoard + its tests; then the UiRequest arm; then the roster row + the CLI twin; us10 journeys + typed refusals).
4. Update the census cells the origin §9 lists (the chip row minted, the per-key/phase/adoption/undo-window/rebind evidence, remote_list backoff) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-files-sync-status-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, the N-19 root cause + fix shape, the #749 disposition, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of the ux-r8 dispatch ("files sync status", branch fix/ui-r8-files-sync-status). Work in ../taskboard-WT-fix-ui-r8-files-sync-status.

1. Read AGENTS.md, the origin plan 2026-11-16-files-sync-status-ux-plan.md, the dispatch plan §1/§7 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-files-sync-status-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the reproduction test precedes the fix in history and asserts the pending FALL (not "no crash"); the sync tests are hermetic (temp remote, no network, pause/advance for polling — a sleep is a finding); the chip's Ok is neutral with no badge and Forked stays danger (the five-state fold is exhaustive); the stalled truth cannot render Syncing against a rising count (the fold's law); the rebind preserves the remote identity exactly (us10_ac1 asserts the id unchanged) and is atomic (no unowned window — inspect the ceremony's command sequence); the refusals are typed (no stringly errors); the #749 disposition is evidence-backed (the Stop Sync drive is real, not assumed); R-17's no-request-lane-undo posture is untouched (the undo window is the ceremony's own, not journal work).
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-10 (drive the chip states, the backoff, and the rebind through the seams; the doctrine bar's SC-W-N4 captions held).
7. Ship: push, gh pr create (title "fix(ui): honest files-sync status and the Settings-Files family — N-18/N-19, #741 #743 #745 #748 #749 #750 #746 #756"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 8. Chunk 7 — tag colour (`feat/ui-r8-tag-color`)

- **Covers**: tag-color plan §3 US-1 (the free-form colour field via
  `Color::parse` — round plan C-3), US-2 (the board-menu roster-only
  mount; `LabelManage` widened), US-3 (the card chips), §6 W-1..W-3,
  §7 delta (the domain constructor + the CLI refactor). Files:
  `crates/taskboard-domain/src/entities.rs` (the parse),
  `crates/taskboard-app/src/commands/labels.rs` (the refactor),
  `ui/compositions/labels-overlay.slint`, `ui/views/task-view.slint`,
  `ui/widgets/task-card.slint`, `src/menu.rs`, `src/actions.rs`,
  `src/keymap/presets.rs`, `src/adapter.rs`, `src/models.rs`,
  `src/presentation.rs`. Non-goals: no card-chip assignment (R-2),
  no CLI grammar change, no colour derivation.
- **Acceptance**: `Color::parse` unit tests (the CLI corpus as the
  contract) + the CLI parity standing; `us1_ac1` exact parsed
  payload + `us1_ac2` inert invalid; the lenient transparent echo;
  `us2` roster-only mount journeys (no assignment toggles; the
  staged delete grammar intact; the #840 cleared identity
  composing); `LabelManage` totality after the context widening; the
  chip set fold + overflow flag; gate rows FOUND.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of the ux-r8 dispatch ("tag colour", branch feat/ui-r8-tag-color) in /home/viktoria/src/taskboard.

Gated precondition: chunk 4's PR is merged (the board-menu row rides its grammar) — else STOP and report.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-tag-color-ux-plan.md (all sections normative; rulings R-1..R-3; the round plan C-3 supersession) and the dispatch plan §1/§8. Also read docs/coding_guidelines.org §2.1 (the A1 pattern your Color::parse instantiates).
2. Read the seams: entities.rs's Color newtype (the lenient new; where a fallible parse belongs), the CLI's parse_color + labels create/edit (the grammar's owner — refactor onto the domain fn, one behavior one implementation), the overlay's forms + swatch palette (the field's draft funnel; the preview), the labels section + overlay mounts (the r3 D-2 grammar your roster-only mount extends; the #840 cleared identity from chunk 3 — landed), BOARD_CONTEXT + LabelManage's registry row (the context widening), the card decoration region (the chip row's slot; the overflow posture), swatch_hex (the echo hygiene), IdTables::label_ids / task_labels.
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-tag-color-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-tag-color-plan.md. Settle at minimum: Color::parse's signature, error type, accepted/rejected corpus (the CLI's forms verbatim), and the CLI refactor's parity tests; the field's draft/echo/preview plumbing and the invalid echo; the roster-only mount's subject resolution (selected board; no toggles) and its composition with the detail mount (one overlay, two subjects); the board-menu row + the widened binding's per-preset keys; the chip model path (live_labels → row entries; canonical order; the compact overflow); the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 tag-color chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of the ux-r8 dispatch ("tag colour"). Work in ../taskboard-WT-feat-ui-r8-tag-color (branch feat/ui-r8-tag-color).

1. Read AGENTS.md, the origin plan 2026-11-16-tag-color-ux-plan.md, the dispatch plan §1/§8, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-tag-color-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) Color::parse in domain (unit tests first; the CLI's corpus as the contract) + the CLI refactored onto it (parity tests standing); (2) the overlay's colour field (draft, preview, invalid echo inert; palette ↔ field agreement); (3) the roster-only mount (board-menu row + the widened LabelManage + the subject-resolved overlay; no toggles without a task subject; the staged delete and the chunk-3 cleared identity composing; us2 journeys); (4) the card chips (us3; the fold from Task::live_labels; the lenient transparent echo; the compact overflow flag).
4. Update the census cells the origin §9 lists (label.create/edit Board cells, roster_display Board/Today, the task.label_chips_display row minted, #839's closure with the C-3 note) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-tag-color-ux-plan.md --gate until every claimed row is FOUND; then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of the ux-r8 dispatch ("tag colour", branch feat/ui-r8-tag-color). Work in ../taskboard-WT-feat-ui-r8-tag-color.

1. Read AGENTS.md, the origin plan 2026-11-16-tag-color-ux-plan.md, the dispatch plan §1/§8 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-tag-color-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): the parse lives in domain (no UI-side validation — the SRP line; the CLI mints through the same fn — grep for a duplicated validator); the invalid echo commits nothing; the roster-only mount shows no assignment toggles (R-2 — the row-set composition test proves it) and its delete stages through the armed strip with the cleared identity (chunk 3's law); SC-W-N2's `*-picked` seam-name consistency extends to the new field's seam; the chips derive from live_labels with the transparent echo for lenient values (never re-derived).
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-3; the doctrine bar held.
7. Ship: push, gh pr create (title "feat(ui): tag colour configuration and the card chips — N-14, #839"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 9. Chunk 8 — cross-surface friction & routing repairs (`fix/ui-r8-cross-surface-friction`)

- **Covers**: cross-surface-friction plan §3 US-1 (the dropdown
  guard), US-2 (the conversion-arm routes — reproduction test
  first), US-3 (the settings view-switch disarm), US-4 (the `search`
  alias + the cross-reference), US-5 (the per-action one-shot
  consumption set — reproduction test first), US-6 (the Replace
  chain pre-pass with the TEXT mask), §6 W-1..W-6, §7 (delta-free).
  Files: `ui/widgets/dropdown.slint` (or the flyout host),
  `src/adapter.rs` (route + the effect-interpret arm + begin_capture),
  `src/view_state.rs`, `ui/views/settings-view.slint`,
  `crates/taskboard-app/src/cli.rs`, `src/router.rs`. Non-goals:
  #762/#715/#716 stay on the machinery lane, no renames beyond the
  alias.
- **Acceptance**: both reproductions (US-2/AC-1, US-5/AC-1) exist and
  failed first; the empty-model guard's mount fact + the populated
  path unchanged; the armed grammars and settle laws survive (each
  story's AC-2/AC-3 guards); the alias ≡ find; the stale one-shot
  reconciliation; the walk-child closes with TEXT semantics kept;
  gate rows FOUND on the delta-free plan.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of the ux-r8 dispatch ("cross-surface friction", branch fix/ui-r8-cross-surface-friction) in /home/viktoria/src/taskboard.

Gated precondition: chunks 4 and 5 are merged (this chunk sees their final Dropdown grammar and route tables) — else STOP and report.

Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report), then create the worktree: git worktree add -b <branch> ../taskboard-WT-<branch> off the then-current main (fetched origin/main if the local checkout is behind; never without -b, never on main — AGENTS §8 active-worktree safety).

1. Read AGENTS.md, then the accepted origin .artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md (all sections normative; delta-free §7) and the dispatch plan §1/§9. Also skim the six issues' bodies (#753 #832 #836 #772 #763 #712 — each names its discovery context and suggested shape).
2. Read the seams: the Dropdown widget + its open paths (where a zero-row model reaches the flyout — #753), the effect-interpret arm's `(_, UiEffect::HabitConvertArm | UiEffect::SeriesConvertArm) => Vec::new()` (#832 — unconditional), the real arming seams it must route to (the habit row's strip flag; the recur row's convert target) and their subject resolution; the settings unmount path (#836 — where armed_item survives); the find subcommand (the clap alias point); route()'s one-shot table (every .take() — the per-action consumption set's design; what each armed journey's arm names); begin_capture + the chain/settle machinery (the D-H4 foreign rule, the TEXT mask's exception).
3. Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
    Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
4. Write the chunk plan at .artifacts/plans/2026-11-17-ui-r8-cross-surface-friction-plan.md. Settle at minimum: the guard's exact branch (mount suppression vs typed no-op per open site); the conversion routes' subject resolution + NoTarget surfacing; the disarm seam (unmount path) and its interaction with the r2 grammar; the alias mechanics + the palette cross-reference's home; the per-action consumption set's representation (how an arm declares its one-shots) and the reconciliation law (stale clears at flow end); the chain pre-pass's fold order + the TEXT mask's precise boundary; the reproduction tests' designs (deterministic interleavings); the test catalogue per §8; commit sequencing.
5. Commit as docs(plan): the ui-r8 cross-surface-friction chunk plan. No implementation; no push.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of the ux-r8 dispatch ("cross-surface friction"). Work in ../taskboard-WT-fix-ui-r8-cross-surface-friction (branch fix/ui-r8-cross-surface-friction).

1. Read AGENTS.md, the origin plan 2026-11-16-cross-surface-friction-ux-plan.md, the dispatch plan §1/§9, and the stage-1 chunk plan (binding).
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait).
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Implement per the chunk plan, TDD, every commit standalone, in this order: (1) #832's reproduction + the conversion routes (us2; NoTarget surfaced); (2) #763's reproduction + the per-action consumption set (us5; the armed journeys' own pairs still pass — their tests are the guard); (3) #753's guard (us1; the populated path untouched); (4) #836's disarm (us3; the r2 tests standing); (5) #712's chain pre-pass (us6; the capture-over-TEXT tests standing); (6) #772's alias + census note (us4).
4. Update the census cells the origin §9 lists (the guard evidence, the conversion arms' menu-route evidence, the disarm evidence, palette.search's alias, the route-consumption note, the Replace composition) in the same commits.
5. scripts/ux-coverage.sh .artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md --gate until every claimed row is FOUND (delta-free plan: the rows carry existing tokens); then the fast gate.
6. No push, no PR. Never push to or merge main.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-gate results, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of the ux-r8 dispatch ("cross-surface friction", branch fix/ui-r8-cross-surface-friction). Work in ../taskboard-WT-fix-ui-r8-cross-surface-friction.

1. Read AGENTS.md, the origin plan 2026-11-16-cross-surface-friction-ux-plan.md, the dispatch plan §1/§9 (code-quality mandate applies), the stage-1 chunk plan; rebase onto origin/main; review the full post-rebase diff.
2. Long-command discipline (binding; docs/agent_dispatch.org): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls beyond ~5 minutes, and commit all landed work BEFORE any wait). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, the mergeStateStatus CLEAN verification, teardown, and at-merge issue closures.
    Round-8 standing laws (binding): (a) typed outcomes only — never label, log, or UI text assertions (AGENTS §5); (b) time-dependent tests use tokio::time::pause()/advance() — never sleeps; (c) census updates ride the code commits citing the owning plan's US/W ids, and every new slint seam carries a `// slint: <callback>` comment (add one to any pre-framework seam your gate rows must resolve — #782); (d) the doctrine bar (docs/ux_planning.org §2.6) is the regression bar — grey-within-applicable beside withheld-when-inapplicable (round plan C-4); (e) the focus contract: every overlay exit restores the invoking element or its container anchor (focus-restoration plan US-1) — any overlay you build or touch composes with it.
Gate steady state: scripts/ux-coverage.sh .artifacts/plans/2026-11-16-cross-surface-friction-ux-plan.md --gate must report every claimed wiring row and step token FOUND (framework-era plan); a MISSING on a claimed row is a plan-vs-code defect — fix code or plan until FOUND; rows whose Commands cell is — carry their reason in the journey-steps column and resolve as FOUND.
3. Functional focus (this chunk's traps): both reproductions are in history before their fixes; the consumption set names exactly each arm's needs — no over-broad set that recreates take-all, no under-broad set that breaks an armed pair (the existing armed-journey tests are the oracle); the settle laws are untouched (foreign settle still disarms — the fix governs consumption, not settling); the TEXT mask keeps the capture fire-drill's discard semantics; the guard is the empty branch only; the alias is non-breaking (`find` primary); no machinery-lane scope crept in (#762/#715/#716 absent from the diff).
4. Code-quality mandate (verbatim from the dispatch §1).
5. Re-run the gate steady state and the fast gate; failures are findings. Fix on the branch.
6. Walkthrough protocol over US-1..US-6; the doctrine bar held.
7. Ship: push, gh pr create (title "fix(ui): routing and friction repairs — #753 #832 #836 #772 #763 #712"), report the URL — then STOP.
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: verdict, findings, fixes, PR URL. Never merge; never touch main.
```

## 10. Completion bookkeeping (dispatcher-owned at each merge)

At-merge issue closures (close as completed with the PR URL — the
archive convention, AGENTS §7):

| Chunk PR closes | Notes |
|---|---|
| 2 | #668, #669, #670, #724 |
| 3 | #838, #759, #706, #840 |
| 4 | #823 (C-2), #834 |
| 5 | #811 (C-1 — the dated supersession note on r6 D-6 rides the census cell) |
| 6 | #741, #743, #745, #748, #750, #746, #756; **#749 evidence-close** (the Stop Sync verification comment) or residue-fix |
| 7 | #839 (with the C-3 supersession note) |
| 8 | #753, #832, #836, #772, #763, #712 |
| 1 | none — census evidence only (the TE-W-N2 retirement note) |

Commented-not-closed: the six standing deferrals stay open (#807,
#809, #812, #813, #819, #820) — a chunk that *learns* something
relevant to one comments per the standing authorization, never
closes.

- **Machinery folds**: #762 is re-checked by chunk 8 (its Dropdown
  work touches the flyout grammar) — if the guard work makes the
  modal-mount question trivial or moot, close it with the evidence;
  otherwise it stays homed. #715/#716 fold into whichever chunk first
  touches the child-effect-bubble sites (none currently plans to);
  they stay homed.
- **Framework riders**: #782 folds per chunk (stage 2's seam-comment
  duty, in [L]); #760 is every chunk's stage-3 cross-check against
  `tests/seam_inventory.rs`; #775's convention (symbol + baseline-SHA
  anchors) governs every census edit.
- **Post-merge human checks**: chunk 1 — the operator's Dvorak
  International confirmation of M-< / M-> (the AC-2 manual oracle);
  chunk 6 — the operator's real Nextcloud confirms the N-19 fix (the
  boards' files beside `identity.json`; the pending counts fall).
- **Deliberately not dispatched here**: the standing deferrals (§
  Status); the mandate-flip follow-ups (phase 6 is complete — nothing
  further opens); the round's walkthrough MINORs become backlog
  issues at stage 3 per the standing authorization and are triaged in
  a later pass, not in this dispatch.
- **What opens next**: once chunk 8's PR merges, the round closes —
  the census status line gains the round-8 entry, and the next GUI
  review round starts from a fresh operator pass.
