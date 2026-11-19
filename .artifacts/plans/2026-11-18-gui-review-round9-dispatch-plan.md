# [Plan] GUI Review Round 9 — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the 18 chunks of round 9 (habit UX
  parity, widget framework + centralization, keyboard/focus
  architecture, menus remainder, day-review journey). No code in this
  commit.
- **Origin**: the round plan
  `.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md`
  (normative: §1 rulings O-1..O-9, §2 facts, §4 chunk ledger, §5
  deferrals, §6 verification) and the five ACCEPTED UX journey plans
  (2026-11-18, operator acceptance after full review):
  `2026-11-18-habit-parity-ux-plan.md` (J1),
  `2026-11-18-widget-framework-ux-plan.md` (J2),
  `2026-11-18-keyboard-focus-ux-plan.md` (J3),
  `2026-11-18-day-review-ux-plan.md` (J4),
  `2026-11-18-menus-remainder-ux-plan.md` (J5). Research basis:
  `.artifacts/reports/2026-11-18-round9-*.md` (habit-parity,
  widget-framework, keyboard-focus, menus-controls,
  dayreview-details, cli-parity-sweep, widget-inventory-centralization).
- **Status**: READY — all 18 chunks dispatchable; ordering and gates in
  §1. Deliberately not dispatched here: the O-5 backlog remainder
  (#945–#949), the approved habit features (#955–#963), labels
  multi-toggle design (J5 R-6), and everything in round-plan §5.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | J3 R-6, K1 | `fix/ui-keymap-chord-case` | chorded-letter case law in stroke/keymap resolve; preset re-spelling to Ctrl+Shift+t; from_event contract tests | US-3/S1-S2 + W-1 | S |
| 2 | J5 R-2..R-4, M1 | `fix/ui-context-menu-bar` | quick-bar live regression root-cause + structural GUI test + fix; multi-selection bar; hint-keys; #927 disarm; ActionMenu chevron | US-1..US-4 + W-1..W-5 | M |
| 3 | J2 §1 R-3/R-4/R-6, W1 | `feat/ui-widget-framework-base` | ModalDialog + host, AnchoredOverlay helper, dismissal law, scrim, width floors/caps, geometry assertions; docs/widget_interactions.org skeleton | US-1/S1-S4, US-2/S1-S3 | L |
| 4 | J1 US-6, H1 | `feat/domain-habit-fields` | habit description field (domain→CLI→sync wire); optional-day field (display/nudge day in canonical window) | US-6/S1-S2 | S |
| 5 | J2 §migration, W2 | `refactor/ui-overlay-migration` | migrate time picker/recur/labels/palette/date-picker/track-entries onto chunk-3 bases; anchor-mirror race fixes; description-editor stretch; Effort Estimate always-on textbox (commit on focus loss, J2 R-9) | US-1/S1, US-3/S1-S2 + W-3/W-4 | L |
| 6 | J1 US-1/US-2, H2 | `feat/ui-habit-row-rework` | RowSubject::Task\|Habit; TaskCard rendering, horizontal action row; habit context menu incl. convert submenu (armed confirm kept) | US-1/S1-S2, US-2/S1-S2 + W-1..W-3 | L |
| 7 | J4, D2 | `feat/ui-day-review-journey` | 6-step guided review on ModalDialog; embedded planner child (J4 R-3, rides #716); empty-stage auto-skip + CarryTomorrow fix; PlanNextDay severing fix | US-1..US-4 + W-1..W-4 | L |
| 8 | J1 US-3, H3 | `feat/ui-habit-keyboard-parity` | habit shortcuts, traversal, keyboard reorder (closes #934); drag-n-drop gap-drop; #935 uniform refusal pass rides (O-7 Q5) | US-3/S1-S2 | M |
| 9 | J3 R-1/R-3/R-4/R-5, K2 | `feat/ui-directional-focus` | nav.focus-* + FocusDirectional per-view table; preset strategy (emacs/vi-motions/Standard-arrows); TaskDetail subject walk removed; Today control-walk; planner grammar; board move kept-arms fix | US-1/S1-S3, US-2/S1 + W-2/W-3 | L |
| 10 | J1 US-5, H4 | `feat/ui-habit-editing` | habit editing on generalized task-detail surface; nudge law (day-relative/period-start + hour/day snooze); GUI habit remove (staged confirm; closes #944) | US-5/S1-S2 | M |
| 11 | J3 US-4/US-5 R-8, K3 | `feat/ui-taskdetail-shortcuts` | 22-verb completion: done, entries, effort edit/clear, back; one smart track-toggle incl. Board context; hint coverage | US-4/S1-S2, US-5 + W-4/W-5 | M |
| 12 | J3 US-6 R-7, K4 | `feat/ui-row-cursor-walk` | rule-list selected-row cursor walk; app-wide cursor heuristic recorded for W7 | US-6 + W-5 | S |
| 13 | O-6 W3 | `refactor/ui-button-primitives` | TextButton absorbing CtaButton/SettingsButton/VerbButton/ghosts + 8× button grammar; CheckBox primitive (closes #950, #953) | n/a — widget-swap parity (journeys re-pass) | M |
| 14 | O-6 W4 | `refactor/ui-labeled-value-row` | LabeledValueRow display/cell/editor modes absorbing the 10 rows + 3 edit grammars (closes #951); enables H4 surface generalization retrofits | n/a — same | M |
| 15 | J5 R-5, M2 | `refactor/ui-priority-picker-dropdown` | dropdown de-siting (self-registering mount) + priority picker adoption; time combobox/labels untouched | J5 W-x addendum | M |
| 16 | O-6 W5 | `refactor/ui-small-widget-primitives` | ActionChip, SectionHeader, StatusNotice, ConfirmStrip, GhostOpener; Theme tone-color/tone-fill (closes #952) | n/a — same | M |
| 17 | O-6 W6 | `refactor/ui-menu-shell` | MenuShell+MenuRow; AnchoredOverlayHost absorbing anchor-facts mirroring incl. twin pickers (closes #954) | n/a — same | M |
| 18 | O-6 W7 | `docs/ui-widget-interactions` | widget_interactions.org full taxonomy, usage rules, sizing defaults, cursor heuristic (from chunk 12) | n/a — docs | S |

**Dependency graph**: chunks 1 and 2 are order-independent (disjoint
files; both touch neither's seams — serialize 1→2 anyway for
presets.rs-free conflict on 2) and dispatch first. Chunk 3 gates 5, 7.
Chunk 4 gates 6, 8, 10. Chunk 5 gates 7. Chunks 6/7 may run as the one
parallel pair (file-disjoint: habits surfaces vs review surfaces)
under the ≤3-agent cap; everything else serial. 9 after 1 (presets.rs).
11 after 9 (ActionId vocabulary). 13–17 strictly serial among
themselves (all touch ui/widgets/ + shared surfaces), after 5; 15
after 14 (task-view row surface); 18 last. Default dispatch order is
the table order (the operator delegated ordering to the dispatcher).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` next to the
project root (AGENTS §8), branched off the then-current `main`, no PR
ever stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume/second round of a chunk works on
`<branch>-v2` (or re-branches off the fetched `origin/main`), never on
the landed branch. Create worktrees with `git worktree add -b <branch>
../taskboard-WT-<branch>` (never `git worktree add <path> main`).
Disk hygiene (AGENTS §8 worktree lifecycle): any agent creating or
recreating a worktree runs `scripts/worktree-doctor.sh` from the main
checkout first — nonzero exit (free disk below the floor) stops the
chunk; stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

**Long-command discipline** *(binding for every chunk, every stage —
added 2026-11-16 from the framework rollout; full law:
`docs/agent_dispatch.org`)*: a subagent is killed after ~600s without
observable activity, so long silent commands kill working agents.

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
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures.
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
   *Flock deferral (#879)*: when the gate lock is contended and no
   other useful work remains, never block — commit all landed work,
   report "gate deferred: lock held by <holder>", and end the stage;
   the dispatcher runs the gate.
4. *Stage prompts restate this discipline verbatim* (an agent that
   must find it in another file will not carry it).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (commits present, tree clean, gate evidence re-runnable). A stalled
   or quota-cut stage is completed from its landed commits or resumed
   with a state-check message — never re-dispatched blind.
6. *Resource law (concurrency cap + memory gate)*: at most **3 concurrent work-performing agents per box**, staggered ≥3 minutes
   apart (cold pre-commit build peaks must not align); the
   **dispatcher's full §10 suite runs only in a free window** — zero
   agents mid-build (llvm-cov counts as an agent slot of its own);
   **OPERATOR CLARIFICATION 2026-11-19 (round-9 dispatch)**: the
   dispatcher §10 suite instead COUNTS AS ONE SLOT toward the ≤3 cap
   during normal operation — total agents + suite runs ≤ 3, flock
   still serializes multiple suites, memory-gate still guards agent
   builds — and the strict zero-agents window is reserved for when
   memory-gate --status shows pressure. Rationale: the suite is
   agent-equivalent resource-wise; the zero-window rule is
   belt-and-braces against coincidental llvm-cov/build spikes, not a
   standing requirement.
   docs-only stages count toward the cap (their commits trigger hook
   builds). Any real build (>~1 min) and the pre-commit hook's clippy
   step route through `scripts/memory-gate.sh` (cross-worktree flock +
   `MemAvailable` floor, bounded wait, `--status`); the full §10 suite
   keeps `scripts/run-quality-gates.sh`. *Two-mode routing*: builds and
   the hook's clippy step route gated-hold; self-capped long runs
   (mutants/miri-style — own job cap + wall-clock cap) route
   `memory-gate.sh --preflight` only; never hold the gate lock across a
   multi-hour run. Gate deferral follows item 3.
   Evidence: the 2026-11-17 global OOM
   (`.artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md`);
   full law: `docs/agent_dispatch.org` §2.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current `main`,
   writes the TDD-first plan at `.artifacts/plans/<today>-<topic>-plan.md`
   in the house `[Plan]` style, resolving every open question with a
   recorded decision, commits it (`docs(plan): …`). No implementation,
   no push. Origin sections marked normative are binding; deviations
   need a recorded reason.
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing — if the branch or worktree
   path's PR already landed, create `<branch>-v2` instead per AGENTS
   §8; never `git worktree add <path> main`), executes per the approved
   plan — TDD, every commit standalone (rebase-merge hygiene), fast
   gate green per the long-command discipline above (the dispatcher
   owns the full suite). When the origin cites a UX journey plan:
   implement exactly the wiring inventory rows this chunk claims,
   embed journey-step tokens in test names, and run
   `scripts/ux-coverage.sh <plan> --gate` before reporting done
   (MISSING on a claimed row is not done). Does **not** push or open a
   PR. Reports deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`** —
   `git fetch origin --prune`, rebase the branch onto `origin/main`
   (AGENTS §8's rebase dance: drop already-merged duplicate commits;
   resolve only real conflicts), push `--force-with-lease` if the
   rebase rewrote history — then reviews the entire branch diff versus
   the **post-rebase** base against the chunk's acceptance criteria +
   AGENTS rules **plus the code-quality mandate below**, re-runs
   whatever gates apply (fast gate per the discipline above),
   implements fixes for every finding on the branch (amend/reword,
   never duplicate-subject fixups). **Its scope ends at PR-open**: push
   the branch, open the PR to `main` with `gh`, and report the URL —
   the *dispatcher* owns `gh pr checks --watch` (any failing check is a
   finding: the dispatcher sends it back or fixes it), the
   `mergeStateStatus CLEAN` verification (`BEHIND`/`DIRTY` ⇒ rebase,
   push `--force-with-lease`, repeat), **the teardown** — from the
   project root, `git -C /home/viktoria/src/taskboard worktree remove
   --force ../taskboard-WT-<branch>` (absolute anchor per AGENTS §8;
   confirm via `git worktree list`) — and at-merge issue closures. The
   handoff contract remains "PR open + checks green + merge state
   `CLEAN` + worktree removed". When the origin cites a UX journey
   plan, stage 3 additionally carries the **UX completeness mandate**:
   re-run `scripts/ux-coverage.sh <plan> --gate`; then execute the
   walkthrough protocol (docs/ux_planning.org) over every P1 story —
   the four walkthrough questions per step with evidence, then the
   six-heuristic pass over touched surfaces; BLOCKER/MAJOR findings
   are fixed on the branch, MINOR findings become backlog issues.
   Reports a verdict. Never merges; never touches `main` (`main` is
   branch-protected — merges happen on GitHub by the user, after the
   required checks pass).

**Code-quality mandate** (stage 3, every chunk): beyond functional
review, evaluate (a) code quality: naming, comment discipline
(comments state constraints, not narration), idiomatic use of the
approved stack, no dead code or speculative generality, test quality
per AGENTS §5 (no text assertions, no quality ceilings, deterministic
by construction, on_slint_thread for window/rig tests); (b) the Single
Responsibility Principle: the crate-role boundaries of AGENTS §2 are
the SRP line — a boundary violation is an SRP violation; (c) a single
level of abstraction per function body: a function either orchestrates
named operations or works in primitive data, never mixes. Findings are
fixed on the branch like any other finding.

**Chunk-hard rules (every implementation stage)**: SPDX headers per
AGENTS §6; `#![forbid(unsafe_code)]` everywhere except
taskboard-ui-slint; workspace-pinned dependency versions only; no
sleeps in tests (pause/advance the tokio clock); UI stays a dumb
presentation client (no business logic in taskboard-ui-slint); census
supersessions (round-plan §6) land with the chunk that earns them.

The per-chunk sections below add chunk-specific covers, acceptance,
and prompt bodies; the shared text above is included by reference and
restated in the prompts only where the discipline requires verbatim
carriage.

## 2. Chunk 1 — keymap chord case law (`fix/ui-keymap-chord-case`)

- **Covers**: J3 R-6 + US-3; research
  `2026-11-18-round9-keyboard-focus-research.md` §5 finding 5. Binding
  identity for chorded ASCII letters = (modifier set incl. shift bit,
  case-insensitive letter), so `parse("Ctrl+T")` ≡
  `parse("Ctrl+Shift+t")` and both differ from `parse("Ctrl+t")`;
  `to_stroke`/delivery feeds lowercase letters with a shift bit, so the
  resolver must match accordingly. Re-spell every preset definition to
  the `Ctrl+Shift+t` convention (today view included). Surface any
  preset pair that collides under normalization (the old deliberate
  Ctrl+Z/Ctrl+Shift+Z distinction is void per O-8) — re-home colliding
  chords to free chords and record each in the chunk plan. Non-goals:
  no directional ids (chunk 9), no preset retunes beyond spelling.
- **Acceptance**: US-3/S1-S2 journeys; `from_event` contract tests
  over every preset's chorded-letter rows (uppercase spelling, shifted
  delivery, unshifted negative); fold test that no preset row spells a
  chorded uppercase letter without Shift; #855 comment updated with the
  fix.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of round 9 ("keymap chord case law", branch fix/ui-keymap-chord-case) in the taskboard repo at /home/viktoria/src/taskboard.
1. Read AGENTS.md, .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md (J3; ruling R-6 + US-3 are normative), .artifacts/plans/2026-11-18-gui-review-round9-round-plan.md §1 O-8, and .artifacts/reports/2026-11-18-round9-keyboard-focus-research.md §5 finding 5.
2. Read the seams: the Stroke/keymap resolve path (stroke.rs, keymap/mod.rs), presets.rs, and the existing parse/from_event tests. Note why tests fed parse() tokens never caught the seam.
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = STOP and report), then git worktree add -b fix/ui-keymap-chord-case ../taskboard-WT-fix-ui-keymap-chord-case off main (never without -b, never on main).
4. Write a TDD-first plan at .artifacts/plans/<today>-keymap-chord-case-plan.md settling: the exact binding-identity normalization point, how the shift bit is preserved (Ctrl+Z ≡ Ctrl+Shift+z ≠ Ctrl+z), the collision sweep over presets.rs with a re-home table for any voided pairs, and the re-spelling convention.
5. Commit as docs(plan): the chorded-letter case-law fix plan. Do not implement; do not push.
6. Backlog issues: open/comment on `backlog:` issues for out-of-scope findings without asking; list numbers in your report.
Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of round 9 ("keymap chord case law").
1. Read AGENTS.md, J3 (R-6, US-3), the dispatch plan §1/§2, and the stage-1 plan in the worktree ../taskboard-WT-fix-ui-keymap-chord-case (branch fix/ui-keymap-chord-case; recreate on the branch if missing, -v2 if landed).
2. Implement per the plan, TDD: the normalization seam, the from_event delivery contract tests (us3_s1/us3_s2 tokens embedded in test names), the preset re-spelling sweep, collision re-homes exactly as the plan's table records.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate for the US-3 rows this chunk claims; MISSING is not done.
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
6. Backlog issues: open/comment autonomously per AGENTS §7; list numbers.
Report: per-gate results, deviations from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of round 9 (branch fix/ui-keymap-chord-case).
1. Read AGENTS.md, J3 (R-6, US-3), the dispatch plan §1/§2 (code-quality mandate applies), and the stage-1 plan; review the full branch diff (git diff main...HEAD) in ../taskboard-WT-fix-ui-keymap-chord-case.
2. Functional focus: the negative case (bare Ctrl+t must NOT fire an uppercase binding) is the trap — verify the unshifted-delivery tests exist per preset row; verify no preset row still spells a chorded uppercase letter without Shift; verify collisions were re-homed, not silently dropped; no test was weakened.
3. Code-quality mandate: (a) naming/comment discipline, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries are the SRP line; (c) one abstraction level per function. Fix findings on the branch (amend/reword, never duplicate-subject fixups).
4. Re-run the fast gate (pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace); any failure is a finding.
5. Rebase first: git fetch origin --prune; rebase onto origin/main (drop merged duplicates; resolve only real conflicts); push --force-with-lease if rewritten.
6. Ship: push the branch, open the PR to main with gh. Your scope ends at PR-open — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, and issue closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate; execute the walkthrough protocol over J3 US-3; BLOCKER/MAJOR fixed on branch, MINOR to backlog.
8. Backlog issues autonomously (AGENTS §7); list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ui-keymap-chord-case; confirm via git worktree list.
Report: verdict, findings, fixes, PR URL, green-checks confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — context-menu bar remainder (`fix/ui-context-menu-bar`)

- **Covers**: J5 US-1..US-4 + rulings R-2..R-4; menus research
  §1–§4. The quick bar is a **confirmed live regression** (operator
  post-#933 build; screenshot Screenshot_20261118_141914.png):
  root-cause why `task_quick_bar` (menu.rs:559, action-menu.slint:135-153)
  does not render — check MenuKind discriminator arms, feature/config
  gates, model wiring, and the build actually shipped — and prove
  presence with a structural GUI test (slint rig on `on_slint_thread`;
  never text assertions). Then: multi-selection bar (multi-sensible
  elements, J5 R-3; the empty `open_context_menu(..., Vec::new())` arm
  at adapter.rs ~:10159), `hint-keys` on bar buttons, #927
  reconcile-driven disarm, and the `ActionMenu` submenu chevron
  (action-menu.slint:256-274; asset vendored). Non-goals: dropdown
  de-siting (chunk 15), #938.
- **Acceptance**: US-1..US-4 journeys incl. the structural bar-presence
  test; #927 closed at merge; golden/fold chevron test; #855-adjacent
  none.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of round 9 ("context-menu bar remainder", branch fix/ui-context-menu-bar) in the taskboard repo at /home/viktoria/src/taskboard.
1. Read AGENTS.md, .artifacts/plans/2026-11-18-menus-remainder-ux-plan.md (J5; rulings R-2..R-4 normative), the round plan §1 O-9, and .artifacts/reports/2026-11-18-round9-menus-controls-research.md.
2. Read the seams: menu.rs task_quick_bar (~:559), action-menu.slint (bar row ~:135-153, row renderer ~:256-274), the adapter open_context_menu arms incl. the multi path (~:10159), the MenuKind pin machinery (#927's hook), presets stroke lookup for hint-keys.
3. THE REGRESSION IS LIVE (operator-confirmed on a post-#933 build): plan the root-cause ladder first — build a repro as a structural slint-rig test before touching production code; enumerate hypotheses (MenuKind arm, gate, model wiring, mount condition) and how each is eliminated.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b fix/ui-context-menu-bar ../taskboard-WT-fix-ui-context-menu-bar off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-context-menu-bar-plan.md: root-cause ladder, the bar-presence test design, the multi-bar action set (multi-sensible per J5 R-3), hint-keys wiring, #927 disarm design, chevron change + golden re-baseline.
6. Commit as docs(plan): the context-menu bar regression and remainder plan. No implementation; no push.
7. Backlog issues autonomously (AGENTS §7); list numbers.
Report: plan path, the root-cause hypothesis ranking, risks, diff estimate.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of round 9 ("context-menu bar remainder").
1. Read AGENTS.md, J5, dispatch plan §1/§3, and the stage-1 plan in ../taskboard-WT-fix-ui-context-menu-bar (branch fix/ui-context-menu-bar; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: FIRST the failing structural bar-presence test (us1_s1 token; on_slint_thread rig), then root-cause and fix the regression; then the multi bar (us2_s1), hint-keys, #927 reconcile disarm (us4_s2), chevron (us3_s1 + golden re-baseline).
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-menus-remainder-ux-plan.md --gate; MISSING on a claimed row is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: root cause found (with evidence), per-gate results, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of round 9 (branch fix/ui-context-menu-bar).
1. Read AGENTS.md, J5, dispatch plan §1/§3 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-fix-ui-context-menu-bar.
2. Functional focus: the root cause must be evidenced, not guessed — verify the repro test fails on the pre-fix commit (or the plan records why it cannot); the bar-presence test must be structural (no text assertions); the multi bar must contain only multi-sensible verbs; #927's disarm must not break the consequence-first armed-confirm grammar; the chevron golden re-baseline must not weaken other goldens.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate (hooks + scripts/run-quality-gates.sh cargo nextest run --workspace); failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN verification, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J5> --gate; walkthrough over J5 US-1..US-4; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-fix-ui-context-menu-bar; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 4. Chunk 3 — widget framework base (`feat/ui-widget-framework-base`)

- **Covers**: J2 §1 foundations + rulings R-1..R-9; widget-framework
  research §7–§8; inventory report §3/§4 (sizing defaults). Build
  `ModalDialog` + host (centered, soft translucent scrim per J2 R-3,
  content-bounded with 280/560 px bounds ×base-scale per R-6, button
  model with accept/dismiss/custom roles and large left/right-arrayed
  buttons, role-based outside-click dismissal per R-4 — edit dialogs
  save, destructive cancel, `MaskWalkFacts` armed by construction,
  absorbing panel root), the `AnchoredOverlay` geometry helper
  (below/above flip + clamp; absorbs the ~10 copy-pasted expressions),
  geometry non-intersection + layout-shift assertions, and the
  docs/widget_interactions.org skeleton. Non-goals: no consumer
  migration (chunk 5) beyond two proof mounts; no AccessibleRole
  commitment before the stage-1 agent settles availability (Q4 of the
  research).
- **Acceptance**: US-1/S1-S4 + US-2/S1-S3 journeys on the proof
  mounts; geometry tests (anchor non-intersection both flip
  directions; sibling rects unchanged across open/close); fold tests
  for button roles and bounds; doc skeleton lands.

**Prompt 3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 3 of round 9 ("widget framework base", branch feat/ui-widget-framework-base) in the taskboard repo at /home/viktoria/src/taskboard.
1. Read AGENTS.md, .artifacts/plans/2026-11-18-widget-framework-ux-plan.md (J2; rulings R-1..R-9 + US-1/US-2 normative), the round plan §1 O-6, .artifacts/reports/2026-11-18-round9-widget-framework-research.md, and .artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md (§3/§4 sizing).
2. Read the seams: app-window.slint key-root sibling mounts, the MaskWalkFacts clamp machinery from PR #872, the absorbing-TouchArea pattern, Theme tokens (base-scale), and two candidate proof mounts (one modal, one anchored).
3. Settle open design points with recorded decisions: the ModalDialog host mount protocol (how a dialog registers, arms the walk mask, restores focus to the opener), the button model shape (roles + custom slots), the AnchoredOverlay geometry interface (anchor facts in, placed rect out), scrim styling tokens, and AccessibleRole.dialog availability in our Slint version (fallback recorded if absent).
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-widget-framework-base ../taskboard-WT-feat-ui-widget-framework-base off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-widget-framework-base-plan.md incl. the geometry assertion catalogue and the docs/widget_interactions.org skeleton outline.
6. Commit as docs(plan): the widget framework base plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 3 of round 9 ("widget framework base").
1. Read AGENTS.md, J2, dispatch plan §1/§4, and the stage-1 plan in ../taskboard-WT-feat-ui-widget-framework-base (branch feat/ui-widget-framework-base; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: ui/widgets/modal-dialog.slint + host, ui/widgets/anchored-overlay.slint helper, dismissal-law wiring, scrim, width floors/caps as Theme tokens, the two proof mounts, geometry tests (us1_s1..us1_s4, us2_s1..us2_s3 tokens; on_slint_thread), docs/widget_interactions.org skeleton.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-widget-framework-ux-plan.md --gate for this chunk's rows; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 3 of round 9 (branch feat/ui-widget-framework-base).
1. Read AGENTS.md, J2, dispatch plan §1/§4 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-widget-framework-base.
2. Functional focus: the dismissal law is the trap — verify role-based outside-click (edit=saves, destructive=cancels) is tested both ways, Tab containment is armed BY CONSTRUCTION (a mount that forgets the mask must fail a test), inside clicks never close, and the geometry tests assert non-intersection in BOTH flip directions and zero sibling-rect drift across open/close. No migration shortcuts: only the two proof mounts change.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no speculative generality (no unused customization points); (b) AGENTS §2 crate boundaries = SRP line — no business logic in the UI crate; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J2> --gate; walkthrough over J2 US-1/US-2; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-widget-framework-base; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 5. Chunk 4 — habit domain fields (`feat/domain-habit-fields`)

- **Covers**: J1 US-6; habit research §HAB-2 + Q1 ruling (option b).
  Additive only: habit description field threaded domain →
  HabitChanges changeset → CLI (`habits edit`) → sync wire (migration
  if the wire is versioned), and the optional-day field as a
  display/nudge day inside the canonical window (windows stay
  ISO-Monday/1st; streak walk untouched — O-1 supersession recorded in
  the census with this chunk). Non-goals: any GUI surface (chunks
  6/10), anchor-day window semantics, per-weekday schedules (#960).
- **Acceptance**: US-6/S1 seam test (UpdateHabit round-trip incl. both
  fields); property test that window derivation is unchanged by the
  new field; sync round-trip; census D-H12-adjacent supersession note
  (O-1 option b) lands.

**Prompt 4.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 4 of round 9 ("habit domain fields", branch feat/domain-habit-fields) in the taskboard repo at /home/viktoria/src/taskboard.
1. Read AGENTS.md, .artifacts/plans/2026-11-18-habit-parity-ux-plan.md (J1; US-6 + R-2 normative), the round plan §1 O-1 (SUPERSEDED — option b) and O-7, and .artifacts/reports/2026-11-18-round9-habit-parity-research.md (§HAB-2, §Q1).
2. Read the seams: the Habit domain type (habit.rs), HabitChanges/UpdateHabit, the sync wire + migrations, the CLI habits edit path, the streak/window walk (habit.rs:328-339 — MUST remain byte-identical in behavior).
3. Settle with recorded decisions: the optional-day representation (typed Option over the window's day domain, A1-minted if it carries meaning), description field shape (plain string vs bounded), sync wire versioning, and the census supersession note text.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/domain-habit-fields ../taskboard-WT-feat-domain-habit-fields off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-habit-domain-fields-plan.md incl. the property tests pinning window invariance.
6. Commit as docs(plan): the habit domain additive-fields plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 4.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 4 of round 9 ("habit domain fields").
1. Read AGENTS.md, J1, dispatch plan §1/§5, the stage-1 plan in ../taskboard-WT-feat-domain-habit-fields (branch feat/domain-habit-fields; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: description + optional-day fields end-to-end (domain, changeset, engine handling, CLI, sync wire + migration), property tests proving window/streak invariance (us6_s1 token), the census supersession edit.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md --gate for the US-6 rows this chunk claims; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 4.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 4 of round 9 (branch feat/domain-habit-fields).
1. Read AGENTS.md, J1, dispatch plan §1/§5 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-domain-habit-fields.
2. Functional focus: additive-only is the trap — the window walk (habit.rs:328-339) and streak computation must be behaviorally unchanged (property tests prove it, none weakened); no GUI surface may appear; the sync migration must round-trip old payloads; type-driven A1 discipline on the new day type (no validate-and-forget).
3. Code-quality mandate: (a) naming/comments, idiomatic thiserror/chrono/uuid usage, no dead code; (b) AGENTS §2 crate boundaries = SRP line (zero network/async in domain); (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J1> --gate for US-6; walkthrough over US-6's seam-level steps; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-domain-habit-fields; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 6. Chunk 5 — overlay migration (`refactor/ui-overlay-migration`)

- **Covers**: J2 §migration (US-1/S1 proof at scale, US-3) + inventory
  addendum W-3/W-4. Migrate onto chunk 3's bases: due/scheduled editor
  calendar + time panel (the operator's pinned repro, J2 R-7/R-8 —
  both anchor-mirror races at task-view.slint:315-333 and
  time-field.slint:92-126 fixed), recur editor, add-label, labels
  flyout, palette (becomes focus-trapped modal, J2 R-5), date-picker,
  track-entries picker. Task-details polish rides: description editor
  vertical-stretch fix (task-view.slint:1301-1319), Effort Estimate
  always-on one-line textbox committing on focus loss (J2 R-9), the
  in-flow swap machinery deleted. Non-goals: day-review (chunk 7),
  habit editor (chunk 10), LabeledValueRow generalization (chunk 14 —
  keep the migrated rows' shapes compatible with it).
- **Acceptance**: US-1/S1 + US-3/S1-S2 journeys; geometry tests per
  migrated mount (zero sibling drift; anchor non-intersection);
  palette containment journey; effort commit-on-focus-loss test;
  net-negative or neutral diff on the swap machineries.

**Prompt 5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 5 of round 9 ("overlay migration", branch refactor/ui-overlay-migration) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 3 (feat/ui-widget-framework-base) must be merged on main — verify (git log origin/main) before starting; if absent, STOP and report.
1. Read AGENTS.md, J2 (R-4..R-9 + US-1/US-3 normative), dispatch plan §1/§6, .artifacts/reports/2026-11-18-round9-widget-framework-research.md §4-5, and .artifacts/reports/2026-11-18-round9-dayreview-details-research.md (task-details cluster, root causes with line refs).
2. Read the seams: every mount named in the chunk covers, the anchor-facts structs, the DueEditor swap (task-view.slint:650-706), the editor mounts (task-view.slint:1301-1319, 891-936), time-field.slint re-mirror, the palette mount.
3. Settle per-mount migration order (picker repro first — it is the operator's pinned case), each mount's dismissal role (edit=saves vs destructive=cancels, J2 R-4), and how migrated rows stay chunk-14-compatible.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b refactor/ui-overlay-migration ../taskboard-WT-refactor-ui-overlay-migration off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-overlay-migration-plan.md with the per-mount geometry-test catalogue.
6. Commit as docs(plan): the overlay migration plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 5 of round 9 ("overlay migration").
1. Read AGENTS.md, J2, dispatch plan §1/§6, and the stage-1 plan in ../taskboard-WT-refactor-ui-overlay-migration (branch refactor/ui-overlay-migration; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD, mount by mount in the planned order: migrate onto ModalDialog/AnchoredOverlay, fix both anchor-mirror races, land the description-editor stretch and the Effort Estimate row (commit on focus loss), delete the swap machineries. RIDER (2026-11-18 backlog-reduction fold): add the FakeEngine `StateCommand::DeleteLabel` settle arm (#881) so the migrated labels journeys read like the other lifecycle journeys — settle stays fixture continuity, not an oracle, per the file's contract. Journey/geometry tests embed us1_s1, us3_s1, us3_s2, us2_s1..us2_s3 tokens; on_slint_thread for rig tests.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-widget-framework-ux-plan.md --gate for this chunk's rows; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-mount results, per-gate results, deviations.
```

**Prompt 5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 5 of round 9 (branch refactor/ui-overlay-migration).
1. Read AGENTS.md, J2, dispatch plan §1/§6 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-refactor-ui-overlay-migration.
2. Functional focus: regression risk dominates — every migrated mount must keep its command dispatch identical (diff the callback wiring, not just the .slint); the anchor races must be fixed at the geometry seam, not by clamping; effort commit-on-focus-loss must not double-dispatch on close; no mount may keep a hand-copied dismissal/clamp expression that the bases now own; palette containment must be structural (mask armed by host), not a local patch.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, dead swap machinery fully deleted; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J2> --gate; walkthrough over J2 US-1/US-2/US-3 on the migrated surfaces; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-refactor-ui-overlay-migration; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 7. Chunk 6 — habit row rework (`feat/ui-habit-row-rework`)

- **Covers**: J1 US-1/US-2 (RowSubject::Task|Habit at the
  adapter/router layer — O-2; NO domain TaskKind); habits render on
  the task-card substrate in Today with a horizontal action row at
  task-row height; habit right-click context menu (MenuKind::HabitRow)
  with task-grammar entries + habit verbs; convert-to-recurring-task
  demoted to a submenu entry that KEEPS its armed-confirm step (O-7
  Q3); time chip display (anchor_time). Non-goals: editing (chunk 10),
  keyboard/drag (chunk 8), board mounts (J1 R-1).
- **Acceptance**: US-1/S1-S2 + US-2/S1-S2 journeys; fold test for the
  horizontal action row shape; menu seam tests; the
  `TaskMenuSubject.is_habit` hardcode (presentation.rs:1131) replaced
  by real subject routing.

**Prompt 6.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 6 of round 9 ("habit row rework", branch feat/ui-habit-row-rework) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 4 (feat/domain-habit-fields) merged on main — verify; if absent, STOP and report.
1. Read AGENTS.md, J1 (US-1/US-2 + R-1..R-3 normative), the round plan §1 O-2/O-7, and .artifacts/reports/2026-11-18-round9-habit-parity-research.md (§2-§3 architecture, enums-with-data rung).
2. Read the seams: habits.slint (the parallel implementation being replaced), TaskCard substrate, adapter.rs row routing, menu.rs TaskMenuSubject/is_habit (:1131 hardcode) and the MenuKind grammar, the convert-arm verbs.
3. Settle with recorded decisions: the RowSubject enum shape and its router insertion point, which task-card slots habits fill vs hide (streak/quota vs due), the horizontal action set, the HabitRow MenuKind arm and its submenu+armed-confirm grammar, and what happens to the old HabitsSection widgets (delete; keep create form).
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-habit-row-rework ../taskboard-WT-feat-ui-habit-row-rework off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-habit-row-rework-plan.md.
6. Commit as docs(plan): the habit row rework plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 6.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 6 of round 9 ("habit row rework").
1. Read AGENTS.md, J1, dispatch plan §1/§7, and the stage-1 plan in ../taskboard-WT-feat-ui-habit-row-rework (branch feat/ui-habit-row-rework; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: RowSubject routing, TaskCard-backed habit rows with horizontal actions, MenuKind::HabitRow + convert submenu keeping the armed confirm, time chip. Journey tests embed us1_s1, us1_s2, us2_s1, us2_s2 tokens; on_slint_thread.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md --gate for US-1/US-2; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 6.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 6 of round 9 (branch feat/ui-habit-row-rework).
1. Read AGENTS.md, J1, dispatch plan §1/§7 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-habit-row-rework.
2. Functional focus: the unification must be at the adapter/router layer ONLY — any domain TaskKind leakage is a blocker; task rows' behavior must be byte-identical (their journeys must pass untouched); the convert submenu must keep the consequence-first armed confirm; kiosk gating (display+complete only) must survive the rework; the old parallel widgets deleted, not commented out.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line — UI stays dumb presentation; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J1> --gate; walkthrough over J1 US-1/US-2; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-habit-row-rework; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 8. Chunk 7 — day-review journey (`feat/ui-day-review-journey`)

- **Covers**: J4 in full (US-1..US-4, rulings R-1..R-3). The 6-step
  guided review on the chunk-3 ModalDialog (entry → inbox → carry →
  planner → prompts → completion); the planner embedded as a step
  (rides #716's embedded-child integration; built NOW per J4 R-3);
  empty-stage auto-skip incl. the CarryTomorrow None-cursor fix
  (adapter.rs:3174-3177, clamp_review_walk extension at
  view_state.rs:2977); WalkStep::Planner; PlanNextDay severing fix
  (Theme.layout-severing-max-width + EmptyState mount,
  plan-next-day-view.slint:327); abort-with-confirm dismissal (J4 R-2);
  walk_resume discoverability replaced by the guided step order.
  Non-goals: standalone PlanNextDay removal (stays), notification
  changes.
- **Acceptance**: J4 US-1..US-4 journeys (us1_s1..s3, us2_s1, us3_s1-s2,
  us4_s1); geometry severing test; seam test that CarryTomorrow is
  never dispatched with a None cursor; #716-linked census evidence.

**Prompt 7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 7 of round 9 ("day-review journey", branch feat/ui-day-review-journey) in the taskboard repo at /home/viktoria/src/taskboard.
Preconditions: chunks 3 and 5 merged on main — verify; if absent, STOP and report.
1. Read AGENTS.md, .artifacts/plans/2026-11-18-day-review-ux-plan.md (J4; all rulings + stories normative), the round plan §1 O-3, and .artifacts/reports/2026-11-18-round9-dayreview-details-research.md (day-review cluster with line refs).
2. Read the seams: review-walk-overlay.slint, the walk cursor/clamp machinery (view_state.rs clamp_review_walk), CarryTomorrow (adapter.rs:3174), WalkStep/hand_off_to_planner (adapter.rs:3055), walk_resume (:4247), plan-next-day-view.slint, issue #716 (gh issue view) for the embedded-child integration contract.
3. Settle with recorded decisions: the stage-machine shape (steps, auto-skip rules, progress presentation flags), the embedded planner mount contract, abort-with-confirm flow, focus/step interaction with chunk 9's ids (design for them; bind only what exists), and the completion summary payload.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-day-review-journey ../taskboard-WT-feat-ui-day-review-journey off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-day-review-journey-plan.md.
6. Commit as docs(plan): the guided day-review journey plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 7 of round 9 ("day-review journey").
1. Read AGENTS.md, J4, dispatch plan §1/§8, and the stage-1 plan in ../taskboard-WT-feat-ui-day-review-journey (branch feat/ui-day-review-journey; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: remount the review on the ModalDialog host, the stage machine with auto-skip, WalkStep::Planner + embedded child (issue #716 contract), the CarryTomorrow/empty-stage fixes, the severing fix with its geometry probe, abort-with-confirm. Journey tests embed us1_s1..us1_s3, us2_s1, us3_s1, us3_s2, us4_s1 tokens; on_slint_thread.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-day-review-ux-plan.md --gate; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 7 of round 9 (branch feat/ui-day-review-journey).
1. Read AGENTS.md, J4, dispatch plan §1/§8 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-day-review-journey.
2. Functional focus: the journey must always have a visible completion path (the operator's core complaint) — verify every stage's next/prev and the completion step in tests; auto-skip must never skip the WHOLE walk (empty everything ⇒ straight to completion summary); the planner step must not regress the standalone PlanNextDay view; the embedded child must integrate focus per #716's contract; abort-with-confirm must not lose decisions already made in earlier stages without the confirm.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code (the old hand_off mystery path deleted or explicitly retained with a recorded reason); (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J4> --gate; walkthrough over ALL J4 P1 stories (this chunk owns the whole plan); BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-day-review-journey; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 9. Chunk 8 — habit keyboard parity (`feat/ui-habit-keyboard-parity`)

- **Covers**: J1 US-3/US-4; habit research §keyboard + #934/#935.
  Habit action shortcuts; traversal through the habit section on the
  task-row keys (closes #934); keyboard reorder; drag-n-drop via the
  #918 gap-drop pattern; the #935 uniform typed-refusal (NoTarget)
  pass rides here per O-7 Q5. Non-goals: editing fields (chunk 10),
  directional ids (chunk 9 lands the vocabulary; this chunk binds
  habit rows into the walk once available — if chunk 9 has not landed,
  implement against the existing traversal seam and leave a recorded
  follow-up note).
- **Acceptance**: US-3/S1-S2 + US-4 journeys; #934 closed at merge; #935
  closed at merge (uniform refusal journeys); drag journey with
  gap-drop.

**Prompt 8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 8 of round 9 ("habit keyboard parity", branch feat/ui-habit-keyboard-parity) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 6 (feat/ui-habit-row-rework) merged on main — verify; if absent, STOP and report. Note whether chunk 9 has landed; the plan records the binding strategy for whichever state holds.
1. Read AGENTS.md, J1 (US-3/US-4 normative), the round plan §1 O-7 (Q5: #935 rides here), and .artifacts/reports/2026-11-18-round9-habit-parity-research.md (keyboard + #934/#935 sections; gh issue view 934 935).
2. Read the seams: the RowSubject router from chunk 6, actions.rs ActionId families, the board gap-drop machinery (#918 pattern), route()'s NoTarget paths (#935), presets.
3. Settle: the habit ActionId family and preset assignments (avoid round-8 collisions), the keyboard-reorder grammar (task-parity), the drag adapter for the habit list, and the #935 uniform refusal design.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-habit-keyboard-parity ../taskboard-WT-feat-ui-habit-keyboard-parity off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-habit-keyboard-parity-plan.md.
6. Commit as docs(plan): the habit keyboard parity plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 8 of round 9 ("habit keyboard parity").
1. Read AGENTS.md, J1, dispatch plan §1/§9, and the stage-1 plan in ../taskboard-WT-feat-ui-habit-keyboard-parity (branch feat/ui-habit-keyboard-parity; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: habit shortcuts, traversal, keyboard reorder, gap-drop drag, the #935 uniform typed-refusal pass. Journey tests embed us3_s1, us3_s2, and the drag/refusal tokens; on_slint_thread.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md --gate for US-3/US-4; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 8 of round 9 (branch feat/ui-habit-keyboard-parity).
1. Read AGENTS.md, J1, dispatch plan §1/§9 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-habit-keyboard-parity.
2. Functional focus: keyboard–mouse parity (O-8 policy) — every pointer verb from chunk 6 must be keyboard-reachable; #935's refusals must be typed outcomes (no text assertions); drag and keyboard reorder must dispatch the same MoveHabit seam; preset assignments must not collide with round-8 or chunk-1-re-homed chords.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures (#934, #935 at merge). Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J1> --gate; walkthrough over J1 US-3/US-4; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-habit-keyboard-parity; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 10. Chunk 9 — directional focus (`feat/ui-directional-focus`)

- **Covers**: J3 US-1/US-2 + rulings R-1/R-3/R-4/R-5; keyboard research
  §2–§4. Four `nav.focus-*` ActionIds → one
  `UiEffect::FocusDirectional` with a per-view interpretation table;
  preset strategy per R-1 (emacs C-f/b/n/p; vi motion keys; Standard
  arrows — all behind the existing text-entry guards); TaskDetail
  C-n/p → the walk ladder and the subject_task walk REMOVED (R-3;
  round-8 D-2 supersession in the census); Today control-walk (R-4);
  planner two-pane grammar (R-5); board move kept-arms fix —
  StackNext/Prev as kept arms of Flow::Move hopping the drop column
  (settle matrix, adapter.rs:4992 + kept arms :3742).
- **Acceptance**: US-1/S1-S3 + US-2/S1 journeys per view; the
  text-entry guard negative tests per preset family; move-survives-
  hops journey; census D-2 supersession lands.

**Prompt 9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 9 of round 9 ("directional focus", branch feat/ui-directional-focus) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 1 (fix/ui-keymap-chord-case) merged — presets.rs must carry the re-spelled convention; verify; if absent, STOP and report.
1. Read AGENTS.md, J3 (R-1..R-5 + US-1/US-2 normative), the round plan §1 O-8, and .artifacts/reports/2026-11-18-round9-keyboard-focus-research.md (§2-§4: the pipeline, the per-view audit table, the settle-matrix root cause).
2. Read the seams: intercept/to_stroke → resolver → ActionRouter → interpret flow×effect table → settle matrix (adapter.rs:4992, kept arms :3742), view_state.rs subject_task (:1652-1665 — to be removed), TaskViewWalkFacts, presets.rs, the push_scroll_page_hint precedent for per-view interpretation.
3. Settle: the four ActionIds' context membership, the per-view table contents (every view in the audit table gets a row), which keys each preset binds (R-1 strategy), the subject-walk removal blast radius, AND the #875 fold (2026-11-18 backlog-reduction): a dedicated selection-extend ActionId (+ explicit Shift+letter preset rows) so printable-bound walk rows (j/k) get Shift+walk parity — named-key arrow extension already works, do not weaken it (its round-8 journeys must be retired with the census supersession, not weakened), and the kept-arm design for move-hop.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-directional-focus ../taskboard-WT-feat-ui-directional-focus off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-directional-focus-plan.md.
6. Commit as docs(plan): the directional focus dispatcher plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 9.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 9 of round 9 ("directional focus").
1. Read AGENTS.md, J3, dispatch plan §1/§10, and the stage-1 plan in ../taskboard-WT-feat-ui-directional-focus (branch feat/ui-directional-focus; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: the ActionIds + UiEffect + per-view table, preset bindings per R-1, subject-walk removal with retired round-8 journeys + census supersession, Today control-walk, planner grammar, the board move kept-arms fix. Journey tests embed us1_s1..us1_s3, us2_s1 tokens per view; on_slint_thread.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate for US-1/US-2; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 9 of round 9 (branch feat/ui-directional-focus).
1. Read AGENTS.md, J3, dispatch plan §1/§10 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-directional-focus.
2. Functional focus: the law is "focus, not active task" — hunt for any view where the four directions still mutate selection/subject instead of focus; the text-entry guards must cover ALL three preset families (emacs chords, vi motions, Standard arrows); the move kept-arms fix must not break the existing move-settle journeys; retired round-8 journeys must be retirement-by-supersession (documented), not silent deletion or weakening.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function — the interpretation table must stay declarative, not sprouting per-view ifs in the router. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J3> --gate; walkthrough over J3 US-1/US-2 in every audited view; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-directional-focus; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 11. Chunk 10 — habit editing (`feat/ui-habit-editing`)

- **Covers**: J1 US-5 (+W-4/W-5). Habit editing on the generalized
  task-detail editor surface (O-7 Q4 — share as much as possible so
  visual adjustments transfer; cooperate with chunk 14's
  LabeledValueRow if landed, else keep shapes compatible); fields:
  description, optional time, optional day; the reminder nudge GUI
  seam with the O-1 nudge law (day-relative when set, else
  period-start; snooze in user-chosen hours/days — extends
  ReminderRef::Tracker, reminders.rs:1387-1446); GUI habit remove
  (staged-confirm grammar like task delete; closes #944). Kiosk:
  editor absent.
- **Acceptance**: US-5/S1-S2 journeys; UpdateHabit round-trip seam
  tests; nudge-arming journey with the test clock (pause/advance);
  #944 closed at merge.

**Prompt 10.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 10 of round 9 ("habit editing", branch feat/ui-habit-editing) in the taskboard repo at /home/viktoria/src/taskboard.
Preconditions: chunks 4 and 6 merged on main — verify; if absent, STOP and report. Record whether chunks 5/14 landed (editor-surface generalization level depends on it).
1. Read AGENTS.md, J1 (US-5 + W-4/W-5 normative), the round plan §1 O-1 (nudge law) and O-7 (Q4), and .artifacts/reports/2026-11-18-round9-habit-parity-research.md (editing + reminder sections).
2. Read the seams: the task-detail editor surface (task-view.slint; its habit-compatible regions), UpdateHabit/HabitChanges (post-chunk-4), the tracker-nudge engine (reminders.rs:1387-1446, ReminderRef::Tracker snooze/dismiss), the task delete staged-confirm grammar, kiosk gates.
3. Settle: the shared-surface factoring (what the habit variant mounts/hides), the nudge arming rules per the O-1 law, the snooze choice UI (hours/days), the remove staging, and kiosk posture.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-habit-editing ../taskboard-WT-feat-ui-habit-editing off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-habit-editing-plan.md.
6. Commit as docs(plan): the habit editing plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 10.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 10 of round 9 ("habit editing").
1. Read AGENTS.md, J1, dispatch plan §1/§11, and the stage-1 plan in ../taskboard-WT-feat-ui-habit-editing (branch feat/ui-habit-editing; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: the habit editor on the shared surface, nudge arming per the O-1 law with snooze choices, GUI remove with staging. Journey tests embed us5_s1, us5_s2 tokens; the nudge test uses tokio pause/advance, never sleeps; on_slint_thread for rigs.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md --gate for US-5; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 10.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 10 of round 9 (branch feat/ui-habit-editing).
1. Read AGENTS.md, J1, dispatch plan §1/§11 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-habit-editing.
2. Functional focus: the nudge law must be exact (day-relative iff a day is set, else period-start; snooze choices in hours AND days); the shared editor surface must not grow habit-only forks that chunk 14 cannot absorb; remove staging must be the task-delete grammar, not a new one; kiosk absence of the editor is tested; no reminder spam regressions (snooze/dismiss paths from the tracker engine still honored).
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line — reminder shaping is presentation-triggered, engine-owned; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures (#944 at merge). Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J1> --gate; walkthrough over J1 US-5; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-habit-editing; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 12. Chunk 11 — task-detail shortcut completion (`feat/ui-taskdetail-shortcuts`)

- **Covers**: J3 US-4/US-5 (R-8: one smart toggle); keyboard research
  §6–§7 (the 22-verb table). Bind: done checkbox, entries opener,
  effort edit/clear, back/close-subject; `task.track-toggle` as the
  ONE smart toggle over the focused task incl. Board/StackDetail
  contexts (existing F9/F10/C-c i/o start-stop pairs remain); hint
  projections on tooltips. Non-goals: directional ids (chunk 9),
  rule-list cursor (chunk 12).
- **Acceptance**: US-4/S1-S2 + US-5 journeys; the verb table fully
  bound-or-ruled with zero Tab-only verbs left (fold test); hint
  coverage projections.

**Prompt 11.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 11 of round 9 ("task-detail shortcut completion", branch feat/ui-taskdetail-shortcuts) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 9 merged (ActionId vocabulary + preset strategy) — verify; if absent, STOP and report.
1. Read AGENTS.md, J3 (US-4/US-5 + R-8 normative), dispatch plan §1/§12, and .artifacts/reports/2026-11-18-round9-keyboard-focus-research.md §6-§7 (the inventory + assignment table).
2. Read the seams: actions.rs context memberships, presets.rs, the task-view widgets in the inventory, the tracker verbs' current bindings, the hint/tooltip projection machinery.
3. Settle: final binding assignments (no collisions with round-8, chunk-1 re-homes, or chunk-9 directional ids), the smart-toggle semantics (start if idle, stop if running), and every remaining Tab-only verb's ruling.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-taskdetail-shortcuts ../taskboard-WT-feat-ui-taskdetail-shortcuts off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-taskdetail-shortcuts-plan.md.
6. Commit as docs(plan): the task-detail shortcut completion plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, the binding table, risks, diff estimate.
```

**Prompt 11.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 11 of round 9 ("task-detail shortcut completion").
1. Read AGENTS.md, J3, dispatch plan §1/§12, and the stage-1 plan in ../taskboard-WT-feat-ui-taskdetail-shortcuts (branch feat/ui-taskdetail-shortcuts; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: the new bindings incl. the smart toggle in TaskDetail + Board contexts, the hint projections, the verb-table fold test. Journey tests embed us4_s1, us4_s2, us5 tokens; on_slint_thread.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate for US-4/US-5; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 11.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 11 of round 9 (branch feat/ui-taskdetail-shortcuts).
1. Read AGENTS.md, J3, dispatch plan §1/§12 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-taskdetail-shortcuts.
2. Functional focus: the operator philosophy — every regular verb reachable WITHOUT Tab; sweep for any activatable task-details widget still shortcut-less (a miss is a finding, not a nicety); the smart toggle must be state-correct in both directions and both contexts; bindings must not collide with anything landed in chunks 1/9; tooltips must show real preset strokes.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J3> --gate; walkthrough over J3 US-4/US-5 with the full verb table in hand; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-taskdetail-shortcuts; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 13. Chunk 12 — row cursor walk (`feat/ui-row-cursor-walk`)

- **Covers**: J3 US-6 (R-7). Selected-rule cursor (new walk state)
  walkable by the directional keys; records the app-wide
  interactable-row cursor heuristic as a note that chunk 18's doc
  formalizes. Non-goals: converting every list widget this round.
- **Acceptance**: US-6 journey (cursor walks, verbs act on the
  selected rule); heuristic note lands for chunk 18.

**Prompt 12.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 12 of round 9 ("row cursor walk", branch feat/ui-row-cursor-walk) in the taskboard repo at /home/viktoria/src/taskboard.
Precondition: chunk 9 merged (directional ids) — verify; if absent, STOP and report.
1. Read AGENTS.md, J3 (US-6 + R-7 normative), dispatch plan §1/§13, and .artifacts/reports/2026-11-18-round9-keyboard-focus-research.md (§9 chunk K4 note).
2. Read the seams: the rule-list surfaces and their row models, the walk-state machinery precedent (BoardSelection/TaskViewWalkFacts), directional id routing.
3. Settle: the cursor state shape, its lifecycle (clear on view exit/foreign settle), and which verbs act on the selected rule.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b feat/ui-row-cursor-walk ../taskboard-WT-feat-ui-row-cursor-walk off main.
5. Write the TDD-first plan at .artifacts/plans/<today>-row-cursor-walk-plan.md.
6. Commit as docs(plan): the rule-list cursor walk plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, decisions, risks, diff estimate.
```

**Prompt 12.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 12 of round 9 ("row cursor walk").
1. Read AGENTS.md, J3, dispatch plan §1/§13, and the stage-1 plan in ../taskboard-WT-feat-ui-row-cursor-walk (branch feat/ui-row-cursor-walk; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: the cursor walk state + directional routing + selected-rule verbs + the heuristic note (us6 token journeys; on_slint_thread).
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. Run scripts/ux-coverage.sh .artifacts/plans/2026-11-18-keyboard-focus-ux-plan.md --gate for US-6; MISSING is not done.
5. No push, no PR. Never push to or merge main.
6. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 12.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 12 of round 9 (branch feat/ui-row-cursor-walk).
1. Read AGENTS.md, J3, dispatch plan §1/§13 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-feat-ui-row-cursor-walk.
2. Functional focus: the cursor must be adapter-owned interaction state (never synced, never journaled — the task.select precedent); foreign settles and view exits clear it; pointer selection and cursor stay coherent (click moves the cursor).
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures. Never merge; never touch main.
7. UX completeness: re-run scripts/ux-coverage.sh <J3> --gate; walkthrough over J3 US-6; BLOCKER/MAJOR on-branch, MINOR to backlog.
8. Backlog issues autonomously; list numbers.
9. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-feat-ui-row-cursor-walk; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 14. Chunks 13–18 — the O-6 centralization wave

These six chunks share one shape: absorb ad-hoc widget families into
base widgets per the inventory report
(`2026-11-18-round9-widget-inventory-centralization-report.md` §3
taxonomy, §4 sizing, §5 migration order), with journey/golden tests
re-passed unchanged (widget swaps must be behavior-preserving; any
intentional behavior change needs a recorded reason and a new test).
Each stage-1 plan records the absorption map (which instances
convert, which stay, why) before any code.

| Chunk | Branch | Absorbs | Closes |
|---|---|---|---|
| 13 | `refactor/ui-button-primitives` | TextButton ← CtaButton/SettingsButton/VerbButton/ghosts + the 8× grammar; CheckBox primitive | #950, #953 |
| 14 | `refactor/ui-labeled-value-row` | LabeledValueRow (display/cell/editor) ← the 10 rows + 3 edit grammars | #951 |
| 15 | `refactor/ui-priority-picker-dropdown` | dropdown de-siting + priority picker adoption (J5 R-5; time combobox/labels untouched) | — |
| 16 | `refactor/ui-small-widget-primitives` | ActionChip, SectionHeader, StatusNotice, ConfirmStrip, GhostOpener; Theme tone-color/tone-fill | #952 |
| 17 | `refactor/ui-menu-shell` | MenuShell+MenuRow; AnchoredOverlayHost ← anchor-facts mirroring ×7 incl. twin pickers | #954 |
| 18 | `docs/ui-widget-interactions` | widget_interactions.org full taxonomy + sizing defaults + cursor heuristic (from chunk 12) | — |

**Prompt 13–18.1 (planning — substitute N, branch, and absorption map
per the table):**

```text
You are the stage-1 PLANNING agent for chunk <N> of round 9 ("<name>", branch <branch>) in the taskboard repo at /home/viktoria/src/taskboard.
Preconditions: chunks 5 and the preceding centralization chunk(s) per the dispatch plan §1 order merged on main — verify; if absent, STOP and report.
1. Read AGENTS.md, the round plan §1 O-6, .artifacts/reports/2026-11-18-round9-widget-inventory-centralization-report.md (§3/§4/§5 normative for this chunk's family), and J2 (rulings + the inventory addendum).
2. Read the seams: every instance listed in this chunk's absorption map (file:line from the inventory report) plus the base widgets they migrate onto (ModalDialog/AnchoredOverlay from chunk 3 where relevant). For chunk 17 (menu shell) add: the #870 fold (2026-11-18 backlog-reduction) — the action menu's window-level host sits outside the single key scope's subtree, so physical focus grabs dead-end raw keys on unrealized layouts; stage 1 investigates whether the rebuilt MenuShell mount can live inside the key scope and records the disposition (fix here or a consciously deferred ruling) as a decision, time-boxed.
3. Settle: the base widget's customization points (properties/callbacks), the exact absorption map (convert/stay/why per instance), behavior-preservation guarantees and how the existing journeys/goldens prove them, and any sizing-token adoption from §4. Census writes by any chunk in this round use symbol anchors (`file::symbol`) per the #775 convention — bare line numbers only where the symbol is ambiguous.
4. Run scripts/worktree-doctor.sh first (nonzero = STOP), then git worktree add -b <branch> ../taskboard-WT-<branch> off main (flat suffix: replace / in branch with -).
5. Write the TDD-first plan at .artifacts/plans/<today>-<topic>-plan.md.
6. Commit as docs(plan): the <name> plan. No implementation; no push.
7. Backlog issues autonomously; list numbers.
Report: plan path, absorption map summary, risks, diff estimate.
```

**Prompt 13–18.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk <N> of round 9 ("<name>").
1. Read AGENTS.md, the round plan §1 O-6, the inventory report sections your stage-1 plan cites, dispatch plan §1/§14, and the stage-1 plan in ../taskboard-WT-<branch> (branch <branch>; recreate if missing, -v2 if landed).
2. Implement per the plan, TDD: the base widget(s), the conversions exactly per the absorption map, sizing tokens where the plan adopts them, journeys/goldens re-passed (behavior-preserving; deviations only with the plan's recorded reason + new test). For chunk 18 the deliverable is the document (taxonomy, contracts, usage rules, sizing defaults, the cursor heuristic) — code changes only where a doc example demands a token rename already planned.
3. Long-command discipline (binding, verbatim): the coverage trio never runs inside a subagent — your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll with useful work between polls if needed). Any command expected to exceed ~5 minutes runs backgrounded with short interleaved polls, and you commit all landed work BEFORE any wait. Flock deferral: if the lock is contended with no useful work left, commit everything, report "gate deferred: lock held by <holder>", and end the stage.
4. No push, no PR. Never push to or merge main.
5. Backlog issues autonomously; list numbers.
Report: per-gate results, deviations.
```

**Prompt 13–18.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk <N> of round 9 (branch <branch>).
1. Read AGENTS.md, the round plan §1 O-6, the inventory report, dispatch plan §1/§14 (code-quality mandate applies), the stage-1 plan; review git diff main...HEAD in ../taskboard-WT-<branch>.
2. Functional focus: behavior preservation is the bar — every existing journey/golden that touched a converted instance passes unchanged or carries the plan's recorded-reason exception; no leftover near-duplicate survives outside the base widget (grep the old shapes); customization points added must each have ≥1 consumer (no speculative generality); SRP: widgets stay presentation-only.
3. Code-quality mandate: (a) naming/comments, idiomatic stack, no dead code; (b) AGENTS §2 crate boundaries = SRP line; (c) one abstraction level per function. Fix on the branch (amend/reword).
4. Re-run the fast gate; failures are findings.
5. Rebase first: fetch --prune, rebase onto origin/main, push --force-with-lease if rewritten.
6. Ship: push, open the PR with gh. Scope ends at PR-open — dispatcher owns checks-watch, CLEAN, teardown, closures (<issue numbers from the table> at merge). Never merge; never touch main.
7. Backlog issues autonomously; list numbers.
8. Teardown: git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-<branch>; confirm with git worktree list.
Report: verdict, findings, fixes, PR URL, green checks, worktree removed.
```

## 15. Completion bookkeeping

- **Issue closures at merge**: #944 (chunk 10), #934 + #935 (chunk
  8), #927 (chunk 2), #950/#953 (13), #951 (14), #952 (16), #954 (17).
  Annotation-only: #855 (chunk 1 comment), #808 (already annotated by
  the CLI sweep), #716 (chunk 7 evidence), #753/#869-adjacent none.
- **Census supersessions landing with their chunks**: round-8 D-2
  (chunk 9), habit R-2 2026-11-13 (chunk 6), O-1 option-b note
  (chunk 4 — landed). Round-9 status entry lands after the final chunk, as the
  round-8 entry did (PR #943 precedent), and carries two backlog-reduction
  folds (2026-11-18): the #775 symbol-anchor convention is enforced on all
  round-9 census rows with the existing line-number anchors swept in the same
  docs-only pass, and #880 closes with evidence once chunk 5's always-mounted
  Effort textbox lands (the Escape-rung framing dissolves — verify, then close
  citing J2 R-9 + the chunk-5 journeys). #938 stays deferred per the O-4
  decline; #870 disposes at chunk 17 per its stage-1 decision.
- **Deliberately not dispatched here**: O-5 backlog remainder
  (#945–#949), approved habit features (#955–#963), labels
  multi-toggle design (J5 R-6), deferred standing items from round-plan
  §5 (#920, #921, #873, #874, #927-adjacent none, #762 family), the
  N-19 operator re-test.
- **What opens next**: the round-10 operator pass should walk the
  reshaped surfaces (habit rows/editor/nudges, migrated overlays +
  modals, directional focus in every view, the guided day-review, the
  centralized widgets) and re-try the control-walk feel (O-8 R-4 trial
  commitment). Backlog candidates already visible: the heatmap/history
  view #957 (the operator's stated prime motivator — schedule early).
- **Round-close additions (operator, 2026-11-19)**: (a) a shared
  journey-harness helper module — extract the per-file journey
  fixture/mount/preamble helpers (the #1055/#1061 dedup rounds' pattern)
  into one support module so new journey files stop tripping the
  SonarCloud new-code duplication gate; lands with the round-close docs
  pass or its own small `test/` chunk if the extraction exceeds a
  docs-only diff; (b) the resource-law clarification — dispatcher full
  §10 suites count as one slot toward the ≤3 cap (recorded under §1
  item 6, operator 2026-11-19); carry the same amendment into
  `docs/agent_dispatch.org` §2 and the dispatch-plan template with the
  round-close docs pass so future dispatchers read it from the standing
  law, not just this plan.
