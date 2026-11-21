# Backlog-Reduction Round 4 ("feature wave") — Implementation & Dispatch Plan

- **Date**: 2026-11-21
- **Kind**: dispatch plan — three-stage agent prompts for the accepted
  round-4 scope (`.artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md`,
  operator-accepted 2026-11-21, **with #353/#354 reaffirmed into the
  round**): the journey wave J1–J4 (docs-only, one gate sitting), the
  feature implementation chunks HF1–HF4 / OP1–OP3 / RS / UF1–UF2 /
  SB1–SB2 / PR1–PR2, and the support wave S1–S9. No code in this
  commit.
- **Origin**: the scope report §A (feature families — normative),
  §B (support wave), §C (#1095 in, #1094 deferred), §D (deferrals),
  §E (round shape); issue bodies normative; the r9 research reports
  and the O-1/O-2/O-5/O-7/O-8 operator rulings are standing authority
  for the feature chunks; PR #1142 (golden review) gates S7.
- **Status**: READY — dispatch proceeds in phases. **Phase 0** = the
  journey wave J1–J4 (parallel, docs-only) + the ungated support
  chunks that need no journeys + S3's background lane; the operator
  accepts the journeys at one gate sitting. **Phase 1** = the feature
  implementation chunks (stage 2 starts from the J-agent's landed
  chunk plans — their stage-1 work is already done). **Phase 2** =
  S7 (after #1142 merges), S8/S9 (capacity). Resource law: ≤3
  concurrent, memory gate, flock-deferral; S3 is the exclusive
  background lane.
- **Orchestration law**: `docs/agent_dispatch.org` (PR #852 + #894's
  flock-deferral rule + the round-3 amendments #1063/#1110 landed).
  Stage prompts restate the discipline verbatim.

## 1. Chunks & ordering

| Chunk | Issues | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| J1 | #955–#963 | `docs/habit-family-journeys` | the habit family's journey plan(s) + 4 chunk plans | **journey plans — the gate artifacts** | M (docs) |
| J2 | #945–#949, #979 | `docs/parity-restore-journeys` | O-5 parity + restore-surface journey plan(s) + 4 chunk plans | **journey plans** | M (docs) |
| J3 | #353, #354 | `docs/undo-ui-journeys` | the reaffirmed undo-v2 UI family's journeys + 2 chunk plans | **journey plans** | M (docs) |
| J4 | #976/#1011/#1045/#1083/#1093, #1030–#1091 set | `docs/small-gated-journeys` | small gated bundle + parity-remainder journeys + 4 chunk plans | **journey plans** | M (docs) |
| HF1 | #955, #958 | `feat/habit-skip-pause` | typed skip + habit pause (window/streak engine) | J1's accepted plan | M/L |
| HF2 | #956, #961 | `feat/habit-undo-quota` | undo completion (fence supersession) + inline quota edit | J1 | M |
| HF4 | #959, #960 | `feat/habit-schedules` | per-habit reminder overrides + per-weekday schedules | J1 | L |
| HF3 | #957, #962, #963 | `feat/habit-history-view` | heatmap/history view + streak badge + statistics | J1 | L |
| OP1 | #945, #946, #947 | `feat/gui-board-ceremonies` | GUI archive/reset/reconcile (+ consent letter) | J2 | L |
| OP2 | #948 | `feat/gui-export` | the export dialog | J2 | L |
| OP3 | #949, #1111 | `feat/gui-status-surface` | GUI status surface + draining line | J2 | M |
| RS | #979 | `feat/restore-cli-surfaces` | restore verbs' first CLI surface | J2 | S/M |
| UF1 | #354 | `feat/domain-reminder-undo` | reminder-verb undo synthesis (pre-images) | J3 | M |
| UF2 | #353 | `feat/ui-undo-branch-picker` | the v2 branch-picker UI | J3 | L |
| SB1 | #976, #1011 | `feat/ui-dialog-semantics-unmute` | R-4 outside-click flip + banner Unmute | J4 | S/M |
| SB2 | #1045, #1083, #1093 | `feat/ui-config-files-signals` | ConfigReload lane + poll_secs field + degraded-boot signal | J4 | M |
| PR1 | #1032/#1057/#1066/#1067/#1068/#1091 | `feat/ui-keyboard-parity-remainder` | keyboard bindings bundle | J4 | M |
| PR2 | #1030, #1031, #1041 | `feat/ui-walk-affordance-parity` | walk/cursor affordances bundle | J4 | M |
| S1 | #1080 | `fix/sync-report-attribution` | push/sync report board attribution divergence | no UX plan | S/M |
| S2a | #1090 | `perf/nextest-run-tuning` | run-side wall regression tuning | no UX plan | M |
| S2b | #1087 | `refactor/consolidate-state-test-binaries` | phase-2 binary consolidation (state/sqlite/sync) | no UX plan | M/L |
| S3 | #327 | `ci/mutants-commands-lane` | sharded mutants one-off, exclusive background lane | no UX plan | XL (machine) |
| S4 | #1113/#1075/#1134/#1120/#1106/#1100 | `test/boot-fixture-residue` | boot/fixture/test-infra residue | no UX plan | M |
| S5 | #1073/#1074/#1076/#1108/#1112/#1097/#1133/#1137/#1110 | `ci/gate-harness-residue` | gate/harness residue + law | no UX plan | M |
| S6 | #1078 | `docs/lld-prereq` | lld prerequisite doc (+ #1103 operator note) | no UX plan | S (compressed) |
| S7 | #1141/#1140/#1139 | `test/golden-coverage-wave` | 7 missing goldens + manifest + comparator upgrade | no UX plan | L |
| S8 | #1096/#1121/#1125/#1126/#1127/#1128 | `refactor/widget-anatomy-bundle` | six widget-anatomy refactors | no UX plan | M |
| S9 | #1029, #1037 | `refactor/severing-settle-matrix` | the two bigger refactors (capacity-permitting) | no UX plan | L |

**Dependency graph & phase order:**

- **Phase 0**: J1 ∥ J2 ∥ J3 ∥ J4 (journey agents) ∥ S1 ∥ S4 ∥ S5 ∥
  S6 (compressed) — and **S3 starts its background lane now** (it
  needs no journeys and its machine time is the round's longest).
  S2a also Phase-0 (its tuning verdict shapes S2b).
- **Gate**: the operator accepts the J1–J4 journeys at one sitting
  (per-plan accept/reject; a rejected plan returns to its J agent).
- **Phase 1** (after the gate): HF1 → HF2 → HF4 → HF3 (domain
  overlaps serialize the first three; HF3 is display-side and may run
  beside HF4 once HF1's engine lands); OP1 ∥ OP2 ∥ OP3; RS (after
  OP3's status work if the surface shares the CLI receipts — else
  parallel); UF1 → UF2 (the picker consumes UF1's catalogue); SB1 ∥
  SB2; PR1 ∥ PR2; S2b (after S2a's verdict); S7 (after #1142 merges).
- **Phase 2** (capacity): S8 ∥ S9.

**Dispatch mechanics**: one agent session per prompt, in order (N.x.1
→ N.x.2 → N.x.3 within implementation chunks; J-chunks are
single-stage); never two agents share a worktree; worktrees at
`../taskboard-WT-<flat-suffix>`, branched off the then-current `main`
with `git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`
(never without `-b`, never on `main`); landed branch ⇒ resume on
`<branch>-v2`; `scripts/worktree-doctor.sh` first, every agent,
nonzero (disk or MemAvailable floor) = STOP.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding — `docs/agent_dispatch.org`;
PR #852 + the #879/#894 flock-deferral rule + the round-3
amendments)*: a subagent is killed after ~600s without observable
activity, so long silent commands kill working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe; background + poll with useful work between polls if
   needed). **Flock-deferral rule: when the gate lock is contended and
   no other useful work remains, never block — commit all landed work,
   report "gate deferred: lock held by <holder>", and end the stage;
   the dispatcher runs the gate.** Expected-heavy single builds route
   through `scripts/memory-gate.sh`. The dispatcher runs the full
   AGENTS §10 suite between stage 2's landing and stage 3's ship; CI's
   Code Coverage check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: push, open the PR, report — the
   dispatcher owns `gh pr checks --watch`, `mergeStateStatus CLEAN`,
   teardown, at-merge closures (§20).
3. *Any command expected over ~5 minutes runs backgrounded* with
   short interleaved polls and useful work between polls — commit all
   landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim*.
5. *Dispatcher stage-exit verification* between stages — landed
   commits present, tree clean, gate evidence re-runnable; resume
   over redo, playbook §8.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` issues immediately (AGENTS §7 standing authorization);
   report the numbers.

**Code-quality mandate** (stage 3, every code chunk — verbatim):
beyond functional review, evaluate (a) code quality: naming, comment
discipline (comments state constraints, not narration), idiomatic use
of the approved stack, no dead code or speculative generality, test
quality per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line; (c) a single
level of abstraction per function body. Findings are fixed on the
branch.

**UX completeness mandate** (stage 3, every journey-carrying chunk —
verbatim): re-run `scripts/ux-coverage.sh <journey-plan> --gate`
(step tokens are REQUIRED rows; MISSING on a claimed row is not
done); execute the walkthrough protocol (`docs/ux_planning.org`) over
every P1 story — the four questions per step with evidence, then the
six-heuristic pass over the touched surface; BLOCKER/MAJOR fixed on
the branch, MINOR to backlog issues. Reports a verdict.

**Journey-gated chunk pattern** (applies to HF1–HF4, OP1–OP3, RS,
UF1–UF2, SB1–SB2, PR1–PR2): the J-agent (Phase 0) lands the accepted
journey plan + the chunk plan on the chunk's branch; **stage 2 opens
from the landed plans** ("GATE CLEARED: the operator accepted the
journey — implement exactly its wiring rows"), stage 3 carries the
code-quality + UX completeness mandates. Each such chunk below
records its distinctive decisions and traps; its prompts follow the
HF1 worked example with the stated substitutions.

## 2. The journey wave (Phase 0, docs-only, single-stage agents)

Common prompt spine for J1–J4 (restated fully in J1; J2–J4 carry the
same spine with their own step-2/3 content): read AGENTS.md +
docs/ux_planning.org + the scope report §A + the family's origin
reports/issues; run the doctor; create the worktree
`git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`; write
the journey plan(s) from `.artifacts/templates/ux-journey-plan.md`
(citing the O-rulings as authority) **and** the per-chunk TDD-first
chunk plans (wiring rows, test catalogues, command deltas); commit
`docs(plan): …`; report GATE-PAUSED with the per-plan summary.
Backlog issues per the standing authorization.

**Prompt J1 (habit family journeys):**

```text
You are the stage-1 JOURNEY-PLANNING agent for round 4's habit family (branch docs/habit-family-journeys) in /home/viktoria/src/taskboard. Your plans gate the round's centerpiece — the operator accepts them before any implementation dispatch.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): docs-only stage — pre-commit hooks are your gate; commit landed work BEFORE any wait; long searches run backgrounded with useful work between polls.
[L] Round laws (binding): (a) the journey plan(s) follow .artifacts/templates/ux-journey-plan.md and cite the O-rulings (O-1 window/nudge law, O-2 no-TaskKind, O-7's Q-answers) as authority; recorded decisions, never silent choices; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b docs/habit-family-journeys ../taskboard-WT-docs-habit-family-journeys off the then-current main (never without -b, never on main).

1. Read AGENTS.md, docs/ux_planning.org, the scope report .artifacts/reports/2026-11-21-backlog-reduction-round4-scope-report.md §A1 (normative), the r9 habit research report (.artifacts/reports/2026-11-18-round9-habit-parity-research.md §3/§4), issues #955 #956 #958 #959 #960 #961 #962 #963, and the landed base: the habit engine (habit.rs windows/streak walk, the B1 clock-bump rule), the habit row/editor surfaces (r9 H2/H4), the reminder engine's anchor_time.
2. Decide the plan structure: one family journey plan or per-chunk plans (record the choice against the framework's granularity law). Cover the four chunks' stories: HF1 skip/pause (the window semantics: a skip is a typed non-miss inside the canonical window — O-1's display-day law; pause as the paused_until twin of series), HF2 undo/quota (the NotUndoable(HabitSurface) fence supersession is a RECORDED RULING with a dated census note — #956's undo integrates the global undo line; #961's quota edit is the click-the-3/8-phrase inline editor), HF4 schedules (per-weekday day sets, per-habit reminder overrides riding the nudge law: day-relative or period-start, snooze in hours/days), HF3 history (the heatmap/month-grid view, streak badge, statistics — adherence %, best streak, per-period totals).
3. Write the journey plan(s) + the four chunk plans (.artifacts/plans/2026-11-21-habit-family-ux-plan.md, 2026-11-21-habit-skip-pause-plan.md, -habit-undo-quota-plan.md, -habit-schedules-plan.md, -habit-history-view-plan.md) with US-n/S-k/W-x/R-x ids, wiring rows, test catalogues (typed outcomes; the Σ property extended where the window semantics change), and command deltas. Resolve every open question with a recorded decision.
4. Commit as: docs(plan): the habit family journey and chunk plans. Do not implement; do not push.
5. Backlog issues per [L](d); list the numbers.

Report: plan paths, the structure decision, per-chunk summaries, GATE-PAUSED.
```

**Prompt J2 (O-5 parity + restore surfaces):** same spine; step-2/3
content: the sweep report's CP register (CP-6/7/8/9 + status) as
origin; OP1's consent-letter grammar (R-10/R-11 supersessions land
with the surfaces; archive/reset/reconcile ride the ceremony fences —
the r1 erratum law holds: the verbs stay NotUndoable, the surfaces
are informed-consent confirmations); OP2's export dialog contract
(format, filters, pdf destination, open-after-export); OP3's status
surface + #1111's draining line (typed Draining status landed in B6);
RS's restore CLI surface (the `tasks restore` verb family parity,
RestoreLabel/RestoreTimeEntry/RestoreHabit receipts via #993's
pinned CommandOutcome serde). Branch `docs/parity-restore-journeys`;
plans named `2026-11-21-parity-restore-journeys-*.md` per chunk
(board-ceremonies / export / status-surface / restore-cli).

**Prompt J3 (undo-v2 UI family):** same spine; step-2/3 content: the
undo architecture design plan §3.4/§3.5/§8 as origin; the operator's
2026-11-21 reaffirmation recorded as the plan's authority note; UF1's
journey covers the reminder verbs becoming undoable (the hard
constraint stands: delivery proofs/snooze links are never retractable
— a delivered notification happened) with the pre-image synthesis
catalogue entries; UF2's journey covers the branch-picker UI over the
journal tree (sibling-branch labels, the v1 line composes). Branch
`docs/undo-ui-journeys`; plans `2026-11-21-undo-ui-*-plan.md`.

**Prompt J4 (small gated bundle + parity remainder):** same spine;
step-2/3 content: #976's flip rides the J2 R-4 ruling (the W1
proof-mount's today-behavior flips to edit-kind per the W2 migration
debt), #1011's banner Unmute (the #733 twin grammar), #1045's
ConfigReload read-only lane, #1083's Files-row poll_secs field (the
#1083 CLI-only exception retires), #1093's JournalDegraded signal
surface (B2's hybrid policy completes); the parity remainder's nine
items map to PR1 (keyboard: #1032 context-menu key, #1057 HabitRepair
chord, #1066 cursor rollout remainder, #1067 series pause/resume
chord, #1068 tracker-kind toggle binding, #1091 rule-cursor disarm)
and PR2 (walk affordances: #1030 File-to keyboard activation,
#1031 Today compact-move, #1041 planner in-pane cursor). Branch
`docs/small-gated-journeys`; plans `2026-11-21-small-gated-journeys-*.md`.

## 3. Chunk HF1 — habit skip + pause (#955, #958) — the worked journey-implementation exemplar

- **Covers**: J1's accepted HF1 plan (normative). The O-1 window law
  binds: a skip is a typed non-miss **inside** the canonical window;
  pause is `paused_until` (the series twin) — the streak walk
  respects paused windows (neither miss nor opportunity).
- **Acceptance**: the accepted journey's wiring rows implemented;
  domain: the skip row kind / pause semantics with typed outcomes;
  the streak-walk rule changes pinned by the Σ property (extended per
  the plan) + window unit tests; the row affordances (skip action,
  pause/resume) per the wiring rows; existing habit corpus snapshots
  unchanged or per-diagnosis; no migration unless the plan recorded
  one.

**Prompt HF1.2 (implementation — stage 2 opens from the landed plans):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk HF1 of round 4 ("habit skip + pause", branch feat/habit-skip-pause). GATE CLEARED: the operator accepted the J1 journey plan — implement exactly its wiring rows.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock in window tests — never sleeps; (c) the O-1 window law binds — a skip/pause that changes canonical window boundaries is a finding; the Σ property extends, never weakens; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.

1. Read AGENTS.md, the accepted journey plan .artifacts/plans/2026-11-21-habit-family-ux-plan.md (the US-n/S-k/W-x/R-x ids are normative), the chunk plan .artifacts/plans/2026-11-21-habit-skip-pause-plan.md, and the dispatch plan §1/§3, in ../taskboard-WT-feat-habit-skip-pause (recreate on the branch if missing; landed branch ⇒ feat/habit-skip-pause-v2).
2. Implement exactly the accepted wiring rows: the domain skip/pause semantics + the streak-walk changes + the row affordances per the plan. TDD (catalogue first where pure); every commit standalone; embed journey-step tokens in test names; diff confined to the plan's named seams; new slint seams carry `// slint: <callback>`.
3. Self-check: scripts/ux-coverage.sh <journey-plan> --gate — MISSING on a claimed row is not done.
4. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: per-row implementation state, gate output, per-gate results, deviations.
```

**Prompt HF1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk HF1 of round 4 (branch feat/habit-skip-pause).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the accepted journey is the contract — implementation beyond its wiring rows is a finding; the O-1 window law and the Σ-property extension hold; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the accepted journey plan + chunk plan + dispatch plan §1/§3 (the code-quality AND UX completeness mandates apply in full); rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-habit-skip-pause.
2. Functional focus — this chunk's traps: (a) skip vs miss vs grace are three distinct typed outcomes — construct the boundary matrix in review (skip inside window, skip attempt outside, pause during skip) and check each; (b) pause interacts with the streak walk and the nudge schedule — a paused habit neither accumulates misses nor fires nudges (pin both); (c) the census supersession notes (D-H12-adjacent) ride the commits citing the plan ids.
3. Code-quality mandate (verbatim from §1) + UX completeness mandate (verbatim from §1: coverage gate + walkthrough protocol per P1 story + six-heuristic pass; verdict reported). Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #955 #958 — closures are the dispatcher's at merge), report the URL + verdicts. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings (functional + mandates), fixes, PR URL, walkthrough verdict.
```

**HF2–HF4, OP1–OP3, RS, UF1–UF2, SB1–SB2, PR1–PR2** follow the HF1
worked example verbatim with these substitutions (branch, plans,
issues, and the chunk-specific traps below; each stage-2 opens "GATE
CLEARED" citing its accepted plan; each stage-3 carries both
mandates):

- **HF2** (`feat/habit-undo-quota`, plans `-habit-undo-quota-*`):
  traps: (a) the NotUndoable(HabitSurface) supersession is a recorded
  ruling with a dated census note — the fence removal is scoped to
  LogHabit only (other habit verbs keep their classifications unless
  the plan says otherwise); (b) the undo integrates the global line
  (the RestoreStack/undo-line machinery — a parallel undo path is a
  finding); (c) the quota editor's inline grammar composes with the
  escape rungs (#1115's contradiction if still open — check).
- **HF4** (`feat/habit-schedules`, plans `-habit-schedules-*`): traps:
  (a) per-weekday day sets change the window/streak walk inputs — the
  O-1 canonical-window law holds (ISO-Monday/1st unchanged); (b) the
  reminder overrides ride the nudge law (day-relative or period-start,
  snooze hours/days) and the reminder engine's anchor_time; (c) the
  sync wire carries the new fields additively (O-2's additive-only
  law) — a breaking changeset is a finding.
- **HF3** (`feat/habit-history-view`, plans `-habit-history-view-*`):
  traps: (a) the heatmap/history view reads HabitState.log — no new
  persistence; (b) the statistics math (adherence %, best streak,
  per-period totals) is domain-pure and proptest-covered; (c) the
  base-widget layer (W1/W2 landed) is consumed, not re-invented (an
  ad-hoc widget is an O-6 violation).
- **OP1** (`feat/gui-board-ceremonies`, plans
  `-board-ceremonies-*`): traps: (a) the ceremony fences hold — the
  surfaces are informed-consent confirmations, the verbs stay
  NotUndoable (the r1 erratum law); (b) the consent-letter grammar
  (R-10/R-11 supersessions) carries the ceremony's actual effects —
  the letter text derives from the typed ceremony plan, never
  hand-written prose; (c) staged-confirm grammar per the task-delete
  precedent.
- **OP2** (`feat/gui-export`, plans `-export-*`): traps: (a) the GUI
  dialog drives the SAME export engine as the CLI verb (one engine,
  two frontends — a GUI-side reimplementation is the O-5 violation);
  (b) the pdf/fold leg's layout options are typed, never free-text;
  (c) open-after-export is typed-permission-aware (no shell-out on
  kiosk).
- **OP3** (`feat/gui-status-surface`, plans `-status-surface-*`):
  traps: (a) the draining line consumes the B6-typed Draining status
  (no new IPC surface); (b) the status surface reads the same state
  the CLI `status` verb prints (one projection).
- **RS** (`feat/restore-cli-surfaces`, plans `-restore-cli-*`):
  traps: (a) the receipts decode through the #993-pinned
  CommandOutcome serde (a hand-rolled receipt format is a finding);
  (b) the restore verbs are typed-refusal-aware (NothingToUndo-class
  outcomes surfaced); (c) parity with the existing `tasks restore`
  verb grammar.
- **UF1** (`feat/domain-reminder-undo`, plans `-reminder-undo-*`):
  traps: (a) delivery proofs and snooze links are NEVER retractable
  (the design-plan hard constraint — a synthesis entry that would
  retract a delivery is a shipping blocker); (b) the pre-image
  catalogue entries follow the N-B precedent (PreImage::ReminderRule
  exists — extend, don't duplicate); (c) the DeleteBoard cascade
  interplay per #980's recorded fact (board-level restore is #357's
  vehicle — out of scope here).
- **UF2** (`feat/ui-undo-branch-picker`, plans `-branch-picker-*`):
  traps: (a) the picker consumes the journal tree's parent pointers
  (no journal migration — #353's additive law); (b) the v1 undo line
  composes (the picker is the v2 extension, not a replacement); (c)
  sibling-branch labels are typed projections, not UI-side guesses.
- **SB1** (`feat/ui-dialog-semantics-unmute`, plans
  `-dialog-semantics-unmute-*`): traps: (a) #976's flip is scoped to
  the capture overlay + recur editor exactly (the W1 proof mount's
  other hosts keep their behavior unless the plan rows say otherwise);
  (b) the R-4 kind test (edit-kind accepts, destructive-kind cancels)
  is the grammar, pinned per host.
- **SB2** (`feat/ui-config-files-signals`, plans
  `-config-files-signals-*`): traps: (a) #1045's lane is read-only
  (reload outcomes surface, never mutate from the UI); (b) #1083's
  field writes through the landed poll_secs lane (B7b) — no second
  writer; (c) #1093's signal is the B2-typed JournalDegraded event
  (no new event kind).
- **PR1** (`feat/ui-keyboard-parity-remainder`, plans
  `-keyboard-parity-remainder-*`): traps: (a) the O-8 case law
  (Ctrl+Shift+t spelling) and the K1 binding-identity rules govern
  every new chord; (b) no preset-row silent drops — the keymap
  contract suite regenerates (C-5's resolution rule); (c) #1091's
  disarm affordance composes with the armed-flow grammar.
- **PR2** (`feat/ui-walk-affordance-parity`, plans
  `-walk-affordance-parity-*`): traps: (a) the walk-stop ladder owns
  the corrections (#1034's landed subsumption is the base — B8
  landed); (b) #1041's in-pane cursor rides the K4 cursor state
  (landed) — a second cursor mechanism is a finding; (c) the O-8
  strict keyboard-mouse parity is the acceptance bar.

## 4. Support chunks (ungated, full three-stage)

**Prompt S1.1 (planning — #1080 attribution):**

```text
You are the stage-1 PLANNING agent for chunk S1 of round 4 ("sync report attribution", branch fix/sync-report-attribution) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes; (c) the equivalence property is the oracle — the fix makes the property pass on the cross-board class WITHOUT narrowing the generator (the R3 generator's coverage is the round's asset; a narrowed generator is a finding); (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP), then git worktree add -b fix/sync-report-attribution ../taskboard-WT-fix-sync-report-attribution off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the scope report §B-S1 (normative), the dispatch plan §1/§4, issue #1080 (the divergence analysis), and the R3 plan (.artifacts/plans/2026-11-19-push-phase-generator-plan.md — the generator that exposed it).
2. Read the seams: apply_push_report vs apply_sync_report's per-board bookkeeping (sync.last_success and the op-target vs bound-board attribution), the pipeline push phase's semantics.
3. Decide: which attribution is correct (the op's target board vs the bound board — the sync bookkeeping's meaning decides; record the semantic argument), the fix, and the property's cross-board case additions.
4. Write the chunk plan at .artifacts/plans/2026-11-21-sync-report-attribution-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the sync report attribution chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the attribution decision, risks, estimated diff size.
```

**Prompt S1.2 (implementation):** standard stage-2 block — read the
plan in `../taskboard-WT-fix-sync-report-attribution` (recreate/
landed⇒v2 rules), implement exactly per the plan TDD (the cross-board
property case red first), diff confined to the sync seams, gate =
hooks + flock-wrapped nextest (defer per rule), no push/PR, backlog
issues per [L](d).

**Prompt S1.3 (review):** standard stage-3 block — rebase onto main;
review vs post-rebase base; traps: (a) the attribution semantics is
argued in the plan and matches the implementation; (b) the property
passes with the generator UNCHANGED (verify the generator hunk is
absent); (c) the non-pathological corpus unchanged. Ship per the
standard block (body references #1080). Backlog issues per [L](d).

**Prompt S2a.x (#1090 run-side tuning):** planning decides the
tuning levers from the #1090 measurements (the doubled wall: 47
binaries halved compile but the run-side lost cross-binary parallelism
— candidates: nextest profile tuning, per-binary test grouping,
`--test-threads` profile, splitting the merged hot binaries);
implement + measure with the identical method; acceptance = the wall
regression recovered to ≤ the 86.9s baseline band or the recorded
residual accepted with evidence; traps: no test-case renames, the
R4 name-set identity law holds. Standard stage-2/3 blocks; body
references #1090.

**Prompt S2b.x (#1087 phase-2 consolidation):** gated on S2a's
verdict; planning produces the binary map for state/sqlite/sync (the
R4 map discipline: zero test-case renames, tooling-path updates,
nextest-list identity diff); implement/review per the R4 pattern;
body references #1087. If S2a's verdict says consolidation is net-
negative, the chunk records that and closes #1087 as
wontfix-with-evidence (dispatcher closes).

**Prompt S3.x (#327 mutants lane):** the established C7 research-chunk
pattern (round-2's plan §8) adapted: the exclusive background lane
(memory-gate'd, `--jobs 1` shards per the #931 law, background +
polls); the cap arithmetic from the issue + the R4-landed binary
structure; the report at
`.artifacts/reports/2026-11-21-cargo-mutants-commands-report.md`;
every survivor dispositioned (trivial kill TDD-first or `backlog:`
issue); traps: (a) the two-mode gate law (preflight-only for the
self-capped long run — never holding the gate lock); (b) no
mutant-driven production change without its failing test. Body
references #327.

**Prompt S4.1 (planning — boot/fixture residue):** covers six items,
each body normative: #1113 (re-home the ui_requests port/guard legs
that compose process env into the integration suite — the C6/#984
pattern), #1075 (drop the subprocess fixtures' explicit shutdown
budgets — actionable since R2c's drain contract; the budgets' removal
must not reintroduce the 10s quantizer: re-measure the slow table),
#1134 (paused-time tokio clock breaks sqlx pool connect under
auto-advance — the clock-law seam: pause AFTER the pool connects or
inject the pool's timer), #1120 (boot-pull settle burns the full
budget when only deconfigured boards are live — the R2b predicate's
unbound-world rule extended to the deconfigured class), #1106
(default_device_json test injection seam), #1100 (exactly-once
invoke count for focus_detail_cell — split the push seam per the
issue). Decide per-item shapes; write the chunk plan
`.artifacts/plans/2026-11-21-boot-fixture-residue-plan.md`; standard
commit/backlog blocks. **Stage 2/3**: standard blocks; traps: (a)
#1075's removal is evidence-backed (the re-measured table shows no
quantizer return); (b) #1134's fix is a seam, not a sleep; (c) the
hostile-env pass stays green. Body references all six.

**Prompt S5.1 (planning — gate/harness residue):** covers nine items:
#1073 (by-name evidence universes order-nondeterministic), #1074
(all_rs_files universe never consumed — remove or consume),
#1076 (bound-test grep includes the fixture tree), #1108 (the
2026-11-14 plans resolve AMBIGUOUS/MISSING post-R4 — the bound-test
mapping applied to the retro cohort), #1112 (habits_journey onto the
B4a helper), #1097 (ci.yml comment names a nonexistent file — fix the
comment), #1133 (rename/split ConfigBatchError once unset reuses it),
#1137 (the diagnostics hop's Mutex cell → the plan-named watch
channel), #1110 (gate-lock hung-holder liveness — the round-3
incident law; a wedged holder must be detectable). Decide per-item;
the gate fixes carry self-tests (the N-A/B5 precedent); the law
amendments (#1110) state constraints; write the chunk plan
`.artifacts/plans/2026-11-21-gate-harness-residue-plan.md`. **Stage
2/3**: standard blocks; traps: (a) zero FOUND→MISSING flips on the
live plans (the B5 acceptance bar); (b) #1137's watch channel
matches the plan's named design (read the plan first); (c) #1110's
liveness check cannot kill legitimate long holders (the two-mode law
composes). Body references all nine.

**Prompt S6 (compressed — #1078 lld doc + #1103 operator note):**
dispatcher-owned compression per `agent_dispatch.org` §7: commit a
short plan, write the lld-prerequisite doc (rustc/OS/CI versions +
the CI apt additions from R1), post the #1103 operator note (the
unregistered worktree directory needs the operator's manual removal —
verify it is not a registered worktree first: `git worktree list`),
PR, watch. Body references #1078; #1103 closes on the operator's
confirmation only.

**Prompt S7.x (golden coverage wave — after #1142 merges):**
planning turns the review report's §6 priority lists (D1–D10/R1–R6)
into the wave plan: #1139's comparator upgrade FIRST (chroma-aware +
scale-aware cells; the doc-comment drift fixed; the mutation audit's
blind spots become pinned cases), then #1140's manifest, then
#1141's gap goldens (the sparse `golden_state_at` fixture widened per
the report's fixture description); acceptance: the new goldens pass
the upgraded comparator deterministically (frozen render clock — the
AGENTS §5 law), the manifest self-describes every golden, the
comparator's upgrade does not flake the existing 27 (the tolerance
recalibration is recorded); traps: (a) the comparator upgrade may
expose real drift — a newly-caught regression is a finding to fix,
not a tolerance to loosen; (b) goldens are committed artifacts —
the frozen render clock law binds. Standard stage-2/3; body
references #1139 #1140 #1141.

**Prompt S8.1 (planning — widget anatomy bundle):** six items, each
body normative: #1096 (the form-row variant — "may eventually" is the
issue's own framing: decide build-now vs defer-with-evidence),
#1121 (the hint caption is not SectionHeader anatomy — revisit or
record), #1125 (AnchoredOverlayHost pointer-anchor + fill-to-floor —
the ActionMenu hand mount's stay), #1126 (DropdownTone → the Tone
ladder with a Tone.accent arm — one consumer, deliberate), #1127
(labels roster rows onto the form-row primitive), #1128 (MenuShell's
cap-rows knob has no false consumer — REMOVE the speculative point;
the O-6 law cuts both ways). The O-6 law is the frame: base widgets
with customization points, no speculative knobs. Write the chunk plan;
standard stage-2/3; traps: behavior-identical refactors (corpus is
the oracle); body references all six.

**Prompt S9.x (#1029 + #1037):** capacity-permitting; planning: #1029
(the layout-severing max-width rule generalized to every view root +
probe family — the `8698fc0f` and plan-next-day precedents) and #1037
(the flow-settle matrix's FK kind vocabulary — per-variant kept arms
within one variant). Both are M/L refactors with the corpus as
oracle; if capacity is short, record-and-defer on the issues. Standard
blocks; body references both.

## 18. The journey-gate mechanics (binding)

Phase 0's J1–J4 report GATE-PAUSED with per-plan summaries. The
dispatcher collects the plans, hands them to the operator, and
records per-plan accept/reject. A rejected plan returns to its J
agent (resume, same worktree) with the operator's notes; its
implementation chunks stay unqueued. Accepted plans unqueue their
chunks (Phase 1). The gate is one sitting unless the operator splits
it.

## 19. Resource & lane law (binding)

≤3 concurrent chunks; memory gate for expected-heavy builds;
flock-deferral everywhere; **S3 is the exclusive background lane**
(it holds preflight-only per the two-mode law and runs for hours —
nothing else shares the box's heavy lane while it runs); R4's
exclusive-lane lesson applies to S2b's measurement windows.

## 20. Dispatcher runbook & at-merge closures (binding)

**Runbook**: stage-exit verification between stages (resume over
redo, playbook §8); full §10 suite between stage-2 landing and
stage-3 ship of every code chunk; CI watch + `mergeStateStatus CLEAN`
+ absolute-path teardown after each PR-open; the merge loop uses the
round-3 standing merge authority (rebase-merge + fresh CLEAN in the
same command) with the premature-close lesson applied (close issues
at the real merge, verified).

**At-merge closures** (dispatcher-owned): HF1 → #955, #958; HF2 →
#956, #961; HF4 → #959, #960; HF3 → #957, #962, #963; OP1 → #945,
#946, #947; OP2 → #948; OP3 → #949, #1111; RS → #979; UF1 → #354;
UF2 → #353; SB1 → #976, #1011; SB2 → #1045, #1083, #1093; PR1 →
#1032, #1057, #1066, #1067, #1068, #1091; PR2 → #1030, #1031, #1041;
S1 → #1080; S2a → #1090; S2b → #1087 (or wontfix-with-evidence); S3
→ #327; S4 → #1113, #1075, #1134, #1120, #1106, #1100; S5 → #1073,
#1074, #1076, #1108, #1112, #1097, #1133, #1137, #1110; S6 → #1078
(#1103 on operator confirmation); S7 → #1141, #1140, #1139; S8 →
#1096, #1121, #1125, #1126, #1127, #1128; S9 → #1029, #1037 (or
record-and-defer).

**Deliberately not dispatched here** (scope report §C/§D): #1094
(CRDT design round — separate), the watch sets, the flaky watchlist,
the perf family, the feature-scale items, C-6 deferrals, #113/#114/
#115, #357.

**What opens next**: round 5 (the cut-line remainder: OP1/OP3, A5,
S4-tail, S6, S8, #1029/#1037 if deferred here); the #1094 design
round; the chrono ruling's follow-ups per #988; the golden-driven doc
screenshots per #1140's manifest.
