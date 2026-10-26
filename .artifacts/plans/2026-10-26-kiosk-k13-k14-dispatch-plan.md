# [Plan] Kiosk Milestone Closure — Dispatch Plan & Agent Prompts (K13 + K14)

- **Date**: 2026-10-26
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation → review) for the kiosk milestone's last two
  chunks: **K13** (ambient view + display rotation, #363) and
  **K14** (docs + wave exit). No code in this commit.
- **Origin**: the wave implementation plan
  `.artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md`
  (§3 K13/K14, §2 KDF-10/11, §5 sequencing, §6 bookkeeping —
  normative); the framing plan's ACCEPTED record (2026-10-19,
  D-K1…D-K10); the views-finding design's D-V1/D-V3 kiosk rulings;
  the milestone-status assessment
  `.artifacts/reports/2026-10-21-kiosk-milestone-remaining-steps-report.md`
  (steps 4–5, the authoritative "what remains" record); the landed
  phase-A/C artefacts it cites; and the user's 2026-10-26 dispatch
  request.
- **Status**: **READY — gated on exactly one external event**: the
  in-flight **#373** round (`feat/ui-listview-virtualisation`, its
  dispatch plan landed as `d58e0b2`) must **merge before K13's
  stage 1 dispatches** (MD-1 below — a real file-set collision,
  not ceremony). K14 follows K13 (its exit report inventories
  K13's landed state). Everything else the milestone contained has
  landed and closed: phase A complete (#237/#299/#300/#262/#263/
  #156/#145/#233/#160), phase C complete (#234 via K10+K11,
  #159+#126 via K12), all spike-exit rulings recorded (renderer
  posture, i686 lane, sd_notify — PR #388 lineage). The daily-
  driver gate that held K13 since 2026-10-21 is **satisfied**
  (milestone `wave-daily-driver`: 13/13 closed — `ViewId::Today`
  and the rotation-ready today view are on `main`).

## 0. Evaluation record (the readiness assessment this plan encodes)

Verified 2026-10-26 against `origin/main` @ `d58e0b2` and the live
tracker:

1. **Phase C is not "leftover" — it is done.** The web dashboard
   (`[web]` config, token ceremony, server, renderers — #234
   closed), the encrypted-file credential store (chacha20poly1305
   + getrandom — #159/#126 closed), and the deployment profile
   (`[kiosk]` section with `fixed_board`, the system unit, the
   runbook) are all in the tree. The user's "phase C" premise is
   superseded by the 2026-10-21 assessment.
2. **The spike is done and ruled.** D250 verdicts recorded (RSS
   49.7–50.2 MB ≤ 200 MB; cold start 2.11 s ≤ 5 s; full-frame
   repaint within budget to ~1 000 tasks; partial bounded-pending
   #390); renderer posture sanctioned; i686 a documented lane;
   #160 closed not-needed. The Q6 default-mode input rides K14.
3. **The true remaining work** is the 2026-10-21 report's steps
   4–5: **K13** (the milestone's last code, unblocked since the
   daily-driver wave closed) and **K14** (the exit).
4. **The #373 gate is real**: K13's file set (`view_state.rs` —
   the rotation state machine lands beside the focus model —
   `models.rs`, the board/Today surfaces it composes) overlaps
   #373's (`view_state.rs` focus-into-view seam, board-layout
   columns, Today section lists, the stack list, the bench).
   Dispatching K13 concurrently with an in-flight #373 stage 2
   would reproduce the DD-9/10 collision class. **Gate: #373's PR
   merged.** K13 also *benefits* from landing second: the ambient
   view composes board columns that #373 is virtualising.
5. **Known follow-ups that are NOT milestone gates** (linked by
   K14, never re-scope it): #390 (true-partial repaint verdict —
   the partial budget stays "bounded above, verdict pending"),
   #373's own on-device re-measure, #463/#464 (detect-side), #386
   (the `connect` 0600-rewrite bug), the optional NF310 leg.

## 1. Chunks & ordering

| Chunk | Impl-plan slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §3 K13 | `feat/ui-kiosk-ambient-view` | the kiosk display-mode state machine: board-pinned, compact, passive degeneration per D-V3; kanban ⇄ today/upcoming rotation (future-directions §3.7) driven by a UI-side timer; `[kiosk]` rotation keys (`dwell`/mode enables — KDF-10's reserved key space); the overdue-banner + offline-tone carry-over; no capture/edit surface; **closes #363** | M |
| 2 | §3 K14 | `docs/kiosk-wave` | architecture.org + README promotion of the whole shipped kiosk subsystem set, CHANGELOG release-line promotion, the exit report (D250 verdicts, the ruled decisions, the Q6 input recorded for the user's separate ruling, the follow-up ledger #390/#373/#463/#464/#386), the milestone closure review; **empties `wave-kiosk`** | S |

**Dependency graph**: `1 → 2` (K14's exit report inventories
K13's landed state; the milestone empties on K13's merge + K14's
bookkeeping). Chunk 1's stage 1 additionally carries the **MD-1
gate**: #373's branch `feat/ui-listview-virtualisation` merged on
`origin/main`. No other gate exists.

**Dispatch mechanics**: one agent session per prompt, in order
(1.1 → 1.2 → 1.3 → 2.1 → 2.2 → 2.3); never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` next to
the project root (AGENTS §8), branched off the then-current
`main` — no PR is ever stacked (after #373 and K13 merge, each
chunk branches off main unstacked). Disk hygiene (AGENTS §8):
`scripts/worktree-doctor.sh` from the main checkout before any
worktree creation — nonzero exit stops the chunk; stage 3 removes
the worktree as its final handoff task.

**Shared execution model** (binding for both chunks — the house
text):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, re-locates the chunk's named symbols at its own start
   (finding IDs and symbols are the stable keys; line numbers
   anywhere are plan-day references), writes the TDD-first plan at
   `.artifacts/plans/<today>-kiosk-<slug>-plan.md` in the house
   `[Plan]` style, resolving every open question with a recorded
   decision, commits it (`docs(plan): …`). No implementation, no
   push. Normative input per MD-4; deviations need a recorded
   reason; a deviation from an ACCEPTED ruling goes back to the
   user (MD-5).
2. **Stage 2 — implementation agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the
   approved plan — TDD per `docs/testing_strategy.org`, every
   commit standalone (rebase-merge hygiene), the full AGENTS §10
   gate suite green. Does **not** push or open a PR. Reports
   deviations.
3. **Stage 3 — review agent**: **begins by rebasing onto `main`**
   — `git fetch origin --prune`, rebase onto `origin/main` (the
   AGENTS §8 rebase dance: drop already-merged duplicate commits;
   resolve only real conflicts), push `--force-with-lease` if
   history was rewritten; review the whole branch diff versus the
   post-rebase base against the acceptance criteria + AGENTS
   rules **plus the mandate below**, re-run the gates, fix every
   finding on the branch (amend/reword). Then ship: push, open
   the PR to `main` with `gh`, `gh pr checks --watch` — any
   failing check is a finding; fix, push `--force-with-lease`,
   repeat until green; verify `gh pr view --json mergeStateStatus`
   reports `CLEAN` (`BEHIND`/`DIRTY` ⇒ rebase, force-with-lease,
   re-run). **Teardown**: `git worktree remove --force
   ../taskboard-WT-<branch>` from the project root. Handoff
   contract: **"PR open + checks green + merge state `CLEAN` +
   worktree removed"**. Reports a verdict. Never merges; never
   touches `main`.

**Code-quality mandate** (stage 3, chunk 1; the house text):
beyond functional review: (a) **code quality**: naming, comment
discipline (comments state constraints, not narration), idiomatic
use of the approved stack, no dead code or speculative
generality, test quality per AGENTS §5; (b) the **Single
Responsibility Principle**: the crate-role boundaries of AGENTS §2
are the SRP line — the UI presents and dispatches intents, the
engine decides, nothing kiosk-side produces `FlowOutcome::Command`
from rotation; (c) **a single level of abstraction per function
body**: orchestration or primitive data, never mixed. Findings
are fixed on the branch like any other finding.

**Evidence & artefact mandate** (stage 3, chunk 2 — the docs
chunk): every factual claim carries evidence (the command run +
observed outcome, or a verifiable source + what it states);
recommendations are decision-grade; repo hygiene (the PR diff is
exactly the chunk's artefacts); scope discipline (out-of-scope
ideas go to the exit report's follow-up ledger — **no autonomous
issue creation, no tracker edits beyond MD-2's list, no production
code**).

**Recorded method decisions**:

- **MD-1 — the #373 gate**: chunk 1's stage-1 prompt verifies
  `feat/ui-listview-virtualisation` is merged on `origin/main`
  (command inline). Until it is, K13 does **not** dispatch — the
  overlap (`view_state.rs` focus model, the board/Today surfaces
  the ambient view composes) is the DD-9/10 collision class, and
  landing K13 second lets it compose the virtualised columns
  as-built. K14 has no #373 gate (only its K13 predecessor).
- **MD-2 — tracker-edit mechanics**: chunk 1's PR body carries
  `Closes #363`; chunk 2 posts **no** comments and closes nothing
  itself — its PR body carries no Closes lines; the milestone
  empties mechanically when #363 closes (all other issues already
  closed). K14's exit report records the follow-up ledger
  (#390, #373's re-measure, #463/#464, #386, the optional NF310
  leg) **by reference, without editing those issues**. Nothing
  else: no label edits, no new issues, no milestone edits beyond
  the closure review's verification.
- **MD-3 — mandate split**: code-quality mandate for chunk 1;
  evidence & artefact mandate for chunk 2; the round-wide clauses
  (PR diff = the chunk's artefacts; scratch never lands) bind
  both.
- **MD-4 — normative rank**: the implementation plan §2 (KDF-10/
  KDF-11, amended by landed reality) / §3 K13–K14 / §4 / §5 / §6;
  then D-V1/D-V3's kiosk rulings; then the framing's §4 record;
  then the 2026-10-21 assessment's steps 4–5; then each stage-1
  plan. **Landed-reality amendment** (recorded here so nobody
  re-derives): `ViewId::Today`, `today-view.slint`, the
  `[kiosk]` section (with `fixed_board`), the sync badge's
  offline tone, the overdue banner, and the D250-ruled
  software-renderer posture all exist on `main`; the #373 round
  has virtualised the board columns, Today section lists, and the
  stack list by K13's start — K13 composes them, never re-derives
  them.
- **MD-5 — anchor discipline + ruling guard**: stage-1 agents
  re-locate every symbol at their own start; a deviation from
  D-V3's board-pinned/passive/no-capture rulings or from the
  KDF-11 "rotation never produces engine commands" rule goes back
  to the user, never silently adapted.
- **MD-6 — stage-1 plan paths**:
  `.artifacts/plans/<today>-kiosk-ambient-view-plan.md` and
  `.artifacts/plans/<today>-kiosk-wave-exit-docs-plan.md`.

Agent-session count: **6** (2 chunks × 3 stages), plus this
docs-only dispatch chunk and the user's merges between chunks.

## 2. Chunk 1 — kiosk ambient view + display rotation (`feat/ui-kiosk-ambient-view`)

- **Covers** (implementation plan §3 K13; D-V3; KDF-11; the issue
  #363 body — normative): the kiosk **display-mode state machine**
  over the pinned board: the board view's compact degeneration
  (kanban) ⇄ the today view's upcoming-oriented reading, per
  D-V3's board-pinned/compact/passive statement; **rotation** = a
  UI-side timer (dwell per mode) that flips the active mode with
  zero input; `[kiosk]` config keys (`enabled_modes`/`dwell_secs`
  or the stage-1 plan's equivalent — the section exists with
  `fixed_board`; rotation keys join it under
  `deny_unknown_fields`); the pin resolves through the existing
  `fixed_board` semantics (absent pin + 1 live board = that
  board; absent pin + N boards = typed config error or the
  stage-1 plan's recorded fallback — D-V3 gives no ring, the pin
  is the scope); banner/offline carry-over (the overdue banner
  rows and the badge's offline tone render in both modes);
  **time-dependent tests via `pause()`/`advance()` — no sleeps**;
  journey-test legs for the rotation and the passive reading.
  **Non-goals**: no multi-board rotation ring (backlog), no
  capture/edit/quick-capture surface (DD-10's kiosk rule), no
  animation work, no renderer/backend change (the D250 posture is
  ruled), no engine commands from rotation (`FlowOutcome::
  Command` never produced), no `IdTables` re-keying, no
  detect-side work (#463/#464).
- **Acceptance**: rotation flips modes on the timer with no input
  (stepped-clock test, no sleeps); the pin resolves exactly per
  the recorded rule and fails typed otherwise; both modes render
  the banner + offline tone; no capture affordance reachable on
  kiosk; the desktop paths are byte-identical with `[kiosk]`
  rotation absent/unenabled; rotation emits no engine command
  (structural assertion); gates green; PR body `Closes #363`.
- **Traps for stage 3**: rotation driving engine commands or UI
  command dispatch (the D-V3/KDF-11 line); a second board entering
  the rotation (the pin is one board); capture surfaces reachable
  in a kiosk mode; sleeps in the rotation tests; the desktop
  today/board views changing for non-kiosk configs; rotation keys
  landing outside `[kiosk]` or silently defaulted on invalid
  values; `IdTables` or model semantics changed (chunk's non-goal
  — #373 just landed them, do not disturb); focus/scroll state
  carried across a rotation flip in a way that breaks the
  focus-into-view seam #373 added (the stage-1 plan must state
  the flip resets or restores focus deliberately).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the kiosk closure round ("kiosk ambient view + display rotation", branch feat/ui-kiosk-ambient-view) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-1, check INLINE): #373's branch feat/ui-listview-virtualisation is merged on origin/main — git fetch origin --prune, then git branch -r --contains origin/feat/ui-listview-virtualisation (or verify its ListView component exists in the board/Today compositions on origin/main). If not merged, STOP and report.

1. Read AGENTS.md, then: the implementation plan .artifacts/plans/2026-10-19-kiosk-wave-implementation-plan.md (§2 KDF-10/11, §3 K13, §4-§6); the views-finding design .artifacts/plans/2026-10-19-views-finding-design-plan.md (D-V1 rotation-readiness, D-V3 the kiosk scope ruling, §7's kiosk degeneration consolidation); issue #363's body (the scope fence); the framing §4 record (D-K2/D-K10); the 2026-10-21 remaining-steps report (step 4); the ListView design-round report + #373's dispatch plan (what the compositions now are — you compose them); and this dispatch plan §0/§2.
2. Read the current seams (re-locate every symbol yourself): crates/taskboard-ui-slint/src/view_state.rs (ViewId cycle incl. Today, the focus model + the focus-into-view seam #373 landed, select_board, ViewState), actions.rs + router.rs (the action registry, UiEffect, how flows dispatch — rotation must ride the existing effect vocabulary, never StateCommand), models.rs (push/refresh semantics post-#373, IdTables), presentation.rs (banner_rows, the badge fold read, compact degeneration helpers), the new ListView component + board-layout.slint (both legs), today-view.slint, the [kiosk] section in crates/taskboard-app/src/config.rs (fixed_board resolution + the KioskSection shape), ui.rs (the appliance hand-over: where kiosk mode is passed at mount), tests/journeys.rs (the journey conventions) and the bench seams (do not break them).
3. Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero exit = STOP and report), then create the worktree ../taskboard-WT-feat-ui-kiosk-ambient-view on a new branch feat/ui-kiosk-ambient-view off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-kiosk-ambient-view-plan.md settling at minimum: the display-mode state machine's states/transitions and where it lives (ViewState extension vs a kiosk-shell struct — one recorded decision); the rotation timer mechanics (slint Timer vs the repaint driver's tick; dwell semantics; pause/advance test seam); the [kiosk] rotation keys' exact names/types/validation (invalid = typed error, never silent default); the pin-resolution rule (absent pin × board-count matrix, each cell recorded); what each mode renders (which existing compositions, which composition files are new); banner/offline carry-over wiring; the focus/scroll story across a flip (deliberate reset or restore — record it, #373's seam must not break); the no-engine-command structural assertion's mechanism; the journey legs; the insta/golden regen scope. Resolve every open question with a recorded decision; deviations from the D-V3 rulings go back to the user — STOP and report.
5. Commit it as: docs(plan): kiosk ambient view and display rotation. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 1 of the kiosk closure round ("kiosk ambient view + display rotation").

1. Read AGENTS.md, the implementation plan (§2 KDF-10/11, §3 K13, §4), the views-finding design's D-V1/D-V3, issue #363, the dispatch plan §0/§2, and the stage-1 plan (.artifacts/plans/*kiosk-ambient-view*-plan.md) in the worktree ../taskboard-WT-feat-ui-kiosk-ambient-view (branch feat/ui-kiosk-ambient-view). Recreate the worktree on that branch if missing.
2. Implement exactly per the approved plan, TDD, every commit standalone (rebase-merge hygiene). Hard rules: rotation is a UI-side timer only — it NEVER produces FlowOutcome::Command or touches the engine; one pinned board, no ring; no capture/edit surface reachable in any kiosk mode; the desktop paths are byte-identical when rotation is absent/disabled; invalid [kiosk] values are typed config errors; rotation tests use pause()/advance() and stepped pushes — never sleeps; no IdTables/model-semantic changes; the focus story across flips is exactly the stage-1 plan's; SPDX headers (this crate is AGPL-3.0-only, unsafe permitted only here).
3. Run the full gate suite from AGENTS.md §10 and make it green (the CI-identical three-command llvm-cov sequence included). Any snapshot/golden regeneration is ONE deliberate pass with the diff reviewed as the intended widening; no .pending files ship.
4. Bookkeeping: the CHANGELOG Unreleased line. Do NOT touch any tracker issue (MD-2). No push, no PR. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the kiosk closure round (branch feat/ui-kiosk-ambient-view).

1. First, rebase onto main: in ../taskboard-WT-feat-ui-kiosk-ambient-view run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§2 KDF-10/11, §3 K13), the views-finding design's D-V1/D-V3, issue #363, the dispatch plan §0/§2 (the code-quality mandate applies in full), and the stage-1 plan.
3. Functional focus — this chunk's traps: rotation dispatching any engine command (structural check the assertion exists and holds); a second board or a ring in the rotation; capture/edit affordances reachable on kiosk; sleeps in the timer tests; desktop-path diffs when [kiosk] rotation is absent; rotation keys silently defaulted; focus-into-view (#373's seam) broken across flips; IdTables/model semantics touched; the pin matrix cells not all tested; banner/offline missing in either mode.
4. Code-quality mandate: (a) naming/comment discipline/no speculative generality (no ring scaffolding — it is backlog); (b) SRP — the UI presents and rotates, the engine is untouched; (c) one level of abstraction per body.
5. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh (body carries "Closes #363"), then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green. Verify gh pr view --json mergeStateStatus reports CLEAN (BEHIND/DIRTY ⇒ rebase, push --force-with-lease, re-check). No comment posting beyond the PR body (MD-2). Never merge; never touch main.
8. Teardown: from the project root, git worktree remove --force ../taskboard-WT-feat-ui-kiosk-ambient-view.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 3. Chunk 2 — docs + wave exit (`docs/kiosk-wave`)

- **Covers** (implementation plan §3 K14, as amended by the 2026-
  10-21 assessment — normative): `docs/architecture.org` (the
  shipped kiosk subsystem set: appliance mode, the deployment
  profile, the dashboard, the metrics surfaces, the file
  credential store, the ambient view — the Executive Summary's
  "kiosk planned" language becomes shipped-reality), `README.org`
  (the kiosk deployment + dashboard sections completed with the
  ambient view/rotation), `CHANGELOG.md` (promotion of the kiosk
  per-chunk Unreleased lines into the release line per the house
  pattern), the **exit report** at
  `.artifacts/reports/<date>-kiosk-wave-exit-report.md`: the D250
  verdict table (RSS, cold start, repaint, sync cycle) with the
  partial-repaint budget's honest "bounded above, verdict pending
  #390" status, the ruled decisions ledger (renderer posture,
  i686 lane, sd_notify, dashboard stack/auth, D-K1…D-K10), the
  **Q6 default-mode input** recorded for the user's separate
  ruling (the wave does not decide it), the follow-up ledger
  (#390, #373's on-device re-measure, #463/#464, #386, the
  optional NF310 leg — by reference, no issue edits), and the
  exit-criteria table walked with evidence pointers (the
  implementation plan §5 table plus the ambient-view row). The
  milestone closure review: verify `wave-kiosk` shows 0 open
  issues after #363's merge. **Non-goals**: no code, no issue
  edits (MD-2), no Q6 implementation, no NF310 execution (user
  leg).
- **Acceptance**: every exit-criterion row carries its evidence
  pointer; the follow-up ledger is complete; the docs match
  shipped reality (spot-checked against the tree, not memory);
  CHANGELOG promotion leaves no kiosk items stranded under
  Unreleased; gates green; the diff is exactly the artefacts.
- **Traps**: docs written from the plans instead of the tree
  (verify every claim); the partial-repaint verdict stated as
  passed (it is pending — #390); Q6 decided instead of recorded
  (it is the user's ruling); follow-up issues edited or created
  (MD-2); kiosk CHANGELOG items left unpromoted.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the kiosk closure round ("docs + wave exit", branch docs/kiosk-wave) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition (MD-1): chunk 1's branch feat/ui-kiosk-ambient-view is merged on origin/main (git fetch origin --prune; verify). If not, STOP and report.

1. Read AGENTS.md, the implementation plan (§3 K14, §5 exit-criteria table, §6), the 2026-10-21 remaining-steps report (steps 1-5 + the verdicts/ledger), the netbook measurement report, the spike-exit ruling records (the PR #388 lineage: ADR 0016 amendment, runbook rulings), the framing §4 record, the briefing §3 (the spike-ruled items and their outcomes), and this dispatch plan §0/§3.
2. Read the shipped reality you will document (verify in the tree, never from plans): docs/architecture.org + README.org + CHANGELOG.md current state; the kiosk subsystem surfaces ([kiosk] incl. the rotation keys chunk 1 landed, [web], credential_store="file", the metrics in status output, the ambient view + rotation behaviour, ADR 0016); the harness/bench scripts and their recorded numbers; gh issue view 363 (closed by chunk 1's PR) and the wave-kiosk milestone state.
3. Run scripts/worktree-doctor.sh first (nonzero = STOP), then create ../taskboard-WT-docs-kiosk-wave on a new branch docs/kiosk-wave off main.
4. Write a plan at .artifacts/plans/<today>-kiosk-wave-exit-docs-plan.md settling: the exact architecture.org/README section edits (shipped-reality language per subsystem); the CHANGELOG promotion mechanics (which Unreleased lines, which release line per the house pattern); the exit report's outline (verdict table incl. the pending-#390 honesty, the ruled-decisions ledger, the Q6 input as recorded-not-decided, the follow-up ledger by reference, the evidence-pointer table walking the §5 exit criteria + the ambient-view row); the closure-review verification commands. Record every decision.
5. Commit as: docs(plan): kiosk wave exit docs. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk 2 of the kiosk closure round ("docs + wave exit").

1. Read AGENTS.md, the implementation plan (§3 K14, §5, §6), the 2026-10-21 report, the dispatch plan §0/§3, and the stage-1 plan in ../taskboard-WT-docs-kiosk-wave (branch docs/kiosk-wave). Recreate the worktree if missing.
2. Implement exactly per the approved plan, commits standalone. Hard rules: every factual claim verified against the tree/tracker at write time (the evidence mandate); the partial-repaint status is "bounded above, verdict pending (#390)" — never a pass; Q6 is recorded as the user's pending ruling, never decided; the follow-up ledger references issues without editing them; no code changes; the CHANGELOG promotion follows the house pattern exactly.
3. Run the full gate suite (AGENTS §10) green.
4. Bookkeeping: this chunk IS the bookkeeping (MD-2: no issue edits, no closes — the milestone empties via #363's earlier merge; verify and record the 0-open count in the exit report). No push, no PR.

Report: per-gate results, the verification outcomes, deviations.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the kiosk closure round (branch docs/kiosk-wave).

1. Rebase onto main first (fetch, rebase origin/main, push --force-with-lease if rewritten); review git diff origin/main...HEAD against the post-rebase base.
2. Read AGENTS.md, the implementation plan (§3 K14, §5 exit criteria, §6), the 2026-10-21 report, the dispatch plan §0/§3 (the evidence & artefact mandate applies in full), and the stage-1 plan.
3. Functional focus — this chunk's traps: claims not verified against the tree (spot-check subsystem surfaces + numbers yourself); the partial-repaint verdict asserted as passed; Q6 decided; the follow-up ledger editing issues or missing entries (#390, #373 re-measure, #463/#464, #386, NF310); stranded kiosk Unreleased CHANGELOG lines; docs describing planned rather than shipped behaviour; a diff containing anything but the artefacts.
4. Evidence & artefact mandate: every claim carries its evidence; the PR diff is exactly the artefacts; no tracker edits.
5. Re-run the gates (AGENTS §10) yourself; failures are findings.
6. Fix everything on the branch (amend/reword).
7. Ship: push, open the PR to main (body: the exit-report path + the closure statement; NO Closes lines), gh pr checks --watch to green, mergeStateStatus CLEAN (rebase+force-with-lease if needed). Never merge; never touch main.
8. Teardown: git worktree remove --force ../taskboard-WT-docs-kiosk-wave.

Report: verdict, findings, fixes, PR URL, green-checks + CLEAN, worktree removed.
```

## 4. Completion bookkeeping & what closure looks like

- Chunk 1's PR merge **closes #363** — the last open milestone
  issue.
- Chunk 2's merge lands the exit report; with #363 already closed,
  `wave-kiosk` reads **0 open issues** and the closure review is
  the exit report's recorded verification. Per AGENTS §7, closing
  is the archive: nothing is deleted, nothing reopened.
- **The milestone's aftermath is explicitly out of scope here**
  (recorded so nobody re-derives): #390 (the true-partial repaint
  verdict), #373's on-device re-measure leg, #463/#464 (damage
  detect-side), #386 (the `connect` 0600 bug), the optional NF310
  measurement leg, and the **Q6 default-[app]-mode ruling** — the
  user decides it from the exit report's recorded input; a ruling
  there is a small follow-up chunk, not milestone work.
