# Backlog Cleanup Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-11-16
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  execution → review) for the single work item of the 2026-11-16
  backlog cleanup round: execute the triage report's cleanup verdicts
  on GitHub (closures, label/milestone normalization, cluster wiring,
  liveness pass), fix the #698 CHANGELOG defect, and commit the
  pending artefacts (the untracked 2026-11-14 UX retrofit triage
  dispatch plan, the triage report, this dispatch plan). No code in
  this PR.
- **Origin**:
  `.artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md`
  (§A close-now verdicts, §B cleanup-round actions, §C escalations,
  §D verdict summary — all normative), plus the user's 2026-11-16
  acceptance of that report and the user's 2026-11-16 mandate that
  the untracked `2026-11-14-ux-retrofit-triage-dispatch-plan.md`
  lands in this round's PR.
- **Status**: READY — the single chunk is dispatchable now.
  Deliberately **not** dispatched here (see §3): the #731 CVE
  escalation (immediate separate work, needs the user's go), #782
  (attaches to the active UX review wave, not to cleanup), the
  #113/#114/#115 staleness closures (user intent gate — this round
  only comments), and any milestone promotion beyond the
  DISPATCH-TIME OVERRIDE below.

## 1. Chunks & ordering

| Chunk | Origin slice | Branch | Covers | UX coverage | Size |
|---|---|---|---|---|---|
| 1 | report §A (A1–A3), §B1–B5 (A4–A11), §B-side artefact mandate (A12–A16) | `chore/backlog-cleanup-round` | GitHub issue/label state per the action catalogue; `CHANGELOG.md` conflict-marker fix; three artefact files + execution log appended to the report; explicit non-goals: no Rust/code change, no #731 work, no new `backlog:` issues except findings | no UX plan in origin | M |

**Dependency graph**: single chunk, no dependencies. Default dispatch
order 1.1 → 1.2 → 1.3.

**Dispatch mechanics**: one agent session per prompt, in order;
never two agents share a worktree. The chunk works in
`../taskboard-WT-chore-backlog-cleanup-round` next to the project
root (AGENTS §8), branched off the then-current `main`, no PR ever
stacked. **Branch names are never reused after their PR lands**
(AGENTS §8, issue #415): a resume works on
`chore/backlog-cleanup-round-v2`, never on the landed branch. Create
the worktree with `git worktree add -b chore/backlog-cleanup-round
../taskboard-WT-chore-backlog-cleanup-round` (never without `-b`,
never on `main`). Disk hygiene: any agent creating or recreating a
worktree runs `scripts/worktree-doctor.sh` from the main checkout
first — nonzero exit stops the chunk; stage 3 removes the worktree as
its final handoff task.

**Shared execution model** (binding for this chunk):

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues. This chunk's stage 2 is itself mostly issue work; findings
   beyond the action catalogue still follow this rule.

1. **Stage 1 — planning agent**: creates the worktree on a new branch
   off the current `main`, verifies every action-catalogue item is
   still true of the live issue state (issues move between sessions —
   an item already closed by someone else is dropped from the plan
   with a recorded reason), resolves the two open decision points
   below, commits the (re)confirmed plan
   (`.artifacts/plans/<today>-backlog-cleanup-round-plan.md`),
   `docs(plan): …`. No implementation, no push.

2. **Stage 2 — execution agent**: continues in that worktree,
   executes the action catalogue exactly per the approved plan in the
   prescribed order (A1–A11 GitHub ops → A16 execution log → A12–A15
   commits). Every `gh` mutation is idempotence-checked first
   (`gh issue view` before every close/label/milestone edit; skip
   already-satisfied items and record the skip in the execution log).
   Does **not** push or open a PR. Reports deviations.

3. **Stage 3 — review agent** *(protocol amended 2026-10-18)*:
   **begins by rebasing onto `main`** — `git fetch origin --prune`,
   rebase onto `origin/main` (drop already-merged duplicate commits;
   resolve only real conflicts), push `--force-with-lease` if history
   was rewritten — then reviews the branch diff versus the
   **post-rebase** base against the acceptance checklist, **re-verifies
   the GitHub state itself** (every A-item is checked with `gh issue
   view`/`gh label list` — a claimed-but-absent issue edit is a
   finding), re-runs the applicable gates, implements fixes on the
   branch (amend/reword, never duplicate-subject fixups). Its last
   tasks are shipping: push, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` — any failing check is a finding; diagnose,
   fix, push `--force-with-lease`, repeat until green — then verify
   `gh pr view --json mergeStateStatus` reports `CLEAN`
   (`BEHIND`/`DIRTY` ⇒ rebase, push, re-run checks). **Then
   teardown**: from the project root, `git -C /home/viktoria/src/taskboard
   worktree remove --force ../taskboard-WT-chore-backlog-cleanup-round`
   (absolute anchor per AGENTS §8 — never a bare relative path);
   confirm via `git worktree list` that the removed path was the
   intended one. Handoff contract: "PR open + checks green + merge
   state `CLEAN` + worktree removed". Never merges; never touches
   `main` (branch-protected — the user merges on GitHub).

**Evidence & artefact mandate** (stage 3, this chunk — docs kind):

- Every factual claim carries evidence (the command run + observed
  outcome, or a verifiable source + what it states) and experiments
  are reproducible from the report's method section; recommendations
  are decision-grade (options, trade-offs, exactly **one**
  recommendation, consequences for later work); repo hygiene (scratch
  notes and raw `gh` dumps never land — evidence is excerpts inside
  the execution log; the PR diff is exactly the chunk's artefacts);
  scope discipline (out-of-scope ideas go to the backlog per its
  template, not into the report).

## 2. Chunk 1 — Backlog cleanup execution (`chore/backlog-cleanup-round`)

- **Covers**: triage report §A + §B, via this action catalogue
  (normative; the report is the rationale, this is the work):

  **GitHub closures (report §A):**
  - **A1** Close #654 as duplicate/not-planned: comment names #697 as
    the canonical watchlist entry for the same test
    (`tty_ctrl_c_during_a_stalled_line_cancels_and_the_next_line_executes`),
    and folds #654's single observation (2026-11-05 "fails under
    full-suite load") into #697 as a comment first (AGENTS §7: one
    issue per distinct test).
  - **A2** Close #767 as completed: comment cites the evidence that
    `.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` is
    tracked on `main` (`git ls-files` + the landing commit) — the
    issue's own closure condition.
  - **A3** Close #842 as completed: comment cites `be95bdad` +
    `1ffe9e64` (the r1 erratum PC-1/PC-5 supersession landing on
    `main`) as satisfying the chunk-6 decision comment's "closes when
    the chunk-6 PR merges".

  **Normalization & wiring (report §B1–B4):**
  - **A4** Uniform priority-label sweep (report §B2's "apply one
    convention" resolved as: every open `backlog:` issue carries a
    priority label; default `p3`): query mechanically
    (`gh issue list --state open --limit 1000 --json number,title,labels`,
    jq filter for titles starting `backlog:` and missing any `p*`
    label) and add `p3` to each hit — do **not** trust any precomputed
    list (known hits at planning time: #140, #213, #185, #184, #843,
    #607; the sweep decides). Flaky-test watchlist issues are not
    `backlog:`-titled and are excluded by the filter itself.
  - **A5** Orphan-label normalization (report §B2): replace the `ux`
    label with `ui` on #759 and #691; a short comment on each records
    the normalization and its reason (the `ux` category is not a
    maintained label in this corpus). Then `gh label list`: if `ux`
    has zero remaining open issues, delete the label.
  - **A6** Milestone-confirmation comments (report §B3) on #708, #695,
    #731, #133, #327, #751: each comment states that the 2026-11-16
    cleanup round deliberately confirms the issue **unscheduled** (no
    milestone — the unscheduled queue per AGENTS §7), and that
    promotion to `v0.2` happens when a scheduling decision is made.
    **DISPATCH-TIME OVERRIDE**: if the operator supplies a promotion
    list when dispatching, apply `--milestone "v0.2"` to exactly those
    issues instead of the confirmation comment.
  - **A7** tty-flaky seam cross-links (report §B1): comments on #697,
    #521, #846 cross-referencing each other and the shared root cause
    (the raw-mode Ctrl-C test family failing under full-suite load on
    one seam), noting that the eventual dedicated flaky triage pass
    over the `flaky-test` label should fix the seam once for all
    three.
  - **A8** Undo-v2 ordering wiring (report §B4): on #356 (RestoreStack
    + cascaded-task restore, the foundation) add a tracking task-list
    comment ordering the family #356 → #355 (per-board undo line
    scoping) → #354 (reminder-surface undo) → #353 (branch-picker UI,
    consumes the journaled tree); #357 (undo/recovery revisit) is
    marked exploratory/out-of-line. One short "part of the undo-v2
    family" pointer comment on each of #355/#354/#353/#357.
  - **A9** Settings-Files family grouping (report §B4): create label
    `settings-files` and apply it to #741, #743, #745, #748, #749,
    #750; add a directory comment on #750 (the adoption-state card,
    largest scope) linking the other five as the settings-Files UX
    gap family a future design round should sweep in one pass.
  - **A10** Liveness pass over the migration cohort (report §B5):
    for #148, #141, #137, #146, #154, #155, #157 verify the issue's
    premise against the current code (named seams: the State Engine's
    clone-per-message path, the pull/push pipeline, the per-card
    refresh loop — locate by reading the current actor code, cite
    file:line) and leave one comment per issue: either "premise
    re-verified 2026-11-16: holds — <evidence>" (leave open) or
    "premise dead — <evidence>" plus close as completed. Close only
    on verifiable in-repo evidence; doubt means leave open.
  - **A11** User-gated intent comments (report §B5) on #113, #114,
    #115: ask the owner to confirm intent or approve closure
    (third-party tooling wishes with no recent signal). **These three
    are never closed by this round.**

  **Repo-side (the PR):**
  - **A12** Fix #698: remove the stray merge-conflict marker lines
    from `CHANGELOG.md`; standalone commit
    `docs(changelog): remove stray merge-conflict marker lines`; the
    PR body carries `Fixes #698`.
  - **A13** Commit the untracked
    `.artifacts/plans/2026-11-14-ux-retrofit-triage-dispatch-plan.md`
    verbatim (operator mandate 2026-11-16) — content untouched; this
    is not this round's document to edit.
  - **A14** Commit
    `.artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md`
    and this dispatch plan.
  - **A15** Stray-file hygiene: inspect
    `.artifacts/reports/#2026-11-14-ux-retrofit-triage-report.md#` —
    if it is an editor backup of the tracked report (diff confirms no
    unique content), delete it in this PR; if it holds unique
    content, STOP and report instead of deleting.
  - **A16** Execution log: append an "Executed actions (2026-11-16)"
    section to the triage report — one line per A-item with its
    outcome (done / skipped-because-already-satisfied / user-gated),
    each with the verifying `gh` command or commit reference. This
    section is the PR's reviewable record of the GitHub-side work,
    which leaves no diff.

- **Explicit non-goals**: no Rust/code change of any kind; no #731
  CVE work; no #782 seam comments; no closures of #113/#114/#115; no
  new feature scope; no editing of the 2026-11-14 retrofit dispatch
  plan's content; no issue creation beyond the standing
  authorization for genuine findings.

- **Deliverable**: the PR (`chore/backlog-cleanup-round` → `main`)
  containing exactly: `CHANGELOG.md` fix + the three artefact files +
  the execution-log edit; plus the mutated GitHub issue state
  verified against the action catalogue.

- **Acceptance** (stage 3 reviews against this):
  - [ ] A1–A3: the three issues are closed with the mandated evidence
        comments; #697 carries the folded observation.
  - [ ] A4: the jq sweep output is reproduced in the execution log;
        every hit has `p3`; no non-`backlog:` issue was touched.
  - [ ] A5: #759/#691 show `ui` not `ux`; the `ux` label is deleted
        or its retention is justified in the log.
  - [ ] A6: all six issues carry the confirmation comment (or the
        milestone per operator override); none has a milestone
        otherwise.
  - [ ] A7–A9: cross-link comments exist; the `settings-files` label
        covers exactly the six issues.
  - [ ] A10: seven comments with file:line evidence; closures (if
        any) cite in-repo evidence only.
  - [ ] A11: three intent questions asked; the three issues remain
        open.
  - [ ] A12: `grep -n '<<<<<<<\|>>>>>>>\|=======' CHANGELOG.md` is
        clean (accounting for any legitimate `=======` inside fenced
        content — inspect, don't blind-grep); PR body has `Fixes
        #698`.
  - [ ] A13–A14: the three artefact files are in the diff unmodified
        (A13) / as written (A14).
  - [ ] A15: resolved one way or the other with evidence in the log.
  - [ ] A16: execution log complete — every A-item has a line.
  - [ ] No code files in the diff; full AGENTS §10 gate suite green
        through `scripts/run-quality-gates.sh` (house law: before
        every handoff — a docs-only diff does not waive it; CI runs
        the identical suite on the PR).
  - [ ] PR checks green, `mergeStateStatus` `CLEAN`, worktree
        removed.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the backlog-cleanup round (branch chore/backlog-cleanup-round) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the origin docs: the triage report .artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md (§A close-now verdicts, §B cleanup-round actions, §B5 liveness-pass rules — all normative) and the dispatch plan .artifacts/plans/2026-11-16-backlog-cleanup-round-dispatch-plan.md §1/§2 (the action catalogue A1–A16 is the binding work list).
2. Verify every A-item against the LIVE issue state before planning: gh issue view each affected issue (#654 #767 #842 #697 #140 #213 #185 #184 #843 #607 #759 #691 #708 #695 #731 #133 #327 #751 #521 #846 #356 #355 #354 #353 #357 #741 #743 #745 #748 #749 #750 #148 #141 #137 #146 #154 #155 #157 #113 #114 #115 #698) — an item already satisfied or closed by someone else is dropped from your plan with a recorded reason, never re-executed.
3. Run scripts/worktree-doctor.sh from the project root /home/viktoria/src/taskboard first (nonzero exit = free disk below the floor: STOP and report instead of planning), then create the worktree with git worktree add -b chore/backlog-cleanup-round ../taskboard-WT-chore-backlog-cleanup-round off main (never without -b and never on main — AGENTS §8 active-worktree safety).
4. Write the execution plan at .artifacts/plans/2026-11-16-backlog-cleanup-round-plan.md: the per-A-item gh command sequence you will run, the exact comment texts (evidence-carrying, per AGENTS §7 conventions), the jq filter for A4's sweep, the two DISPATCH-TIME decisions resolved: (a) milestone override list = whatever the operator passed at dispatch, default none; (b) A15's deletion = conditional on your diff inspection of the stray '#'...'#' file. Resolve every open question with a recorded decision.
5. Inspect CHANGELOG.md for the #698 markers now (they must still exist — if already fixed, drop A12 with a reason) and diff .artifacts/reports/#2026-11-14-ux-retrofit-triage-report.md# against the tracked report to resolve A15's condition.
6. Commit the plan as: docs(plan): the backlog cleanup round execution plan. Do not implement; do not push.
7. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: plan path, dropped A-items with reasons, risks, estimated diff size.
```

**Prompt 1.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 1 of the backlog-cleanup round (branch chore/backlog-cleanup-round).

1. Read AGENTS.md, the triage report .artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md (§A/§B normative), the dispatch plan .artifacts/plans/2026-11-16-backlog-cleanup-round-dispatch-plan.md §1/§2, and the stage-1 plan (.artifacts/plans/2026-11-16-backlog-cleanup-round-plan.md) in the worktree ../taskboard-WT-chore-backlog-cleanup-round (recreate it on the branch if missing — if the branch or worktree path's PR already landed, create chore/backlog-cleanup-round-v2 instead per AGENTS §8; never git worktree add <path> main).
2. Execute the action catalogue A1–A16 exactly per the approved plan, in the prescribed order (A1–A11 GitHub ops → A16 execution log → A12–A15 commits). gh-idiomence: gh issue view before every mutation; skip already-satisfied items and record the skip in the execution log; milestone override list per the stage-1 plan's recorded decision.
3. Comment discipline: every comment you leave names its evidence (commit, git ls-files output, file:line, or the jq sweep output) and stays inside the catalogue's intent. No new scope, no code edits, no edits to the 2026-11-14 retrofit dispatch plan's content.
4. The repo-side diff must be exactly: CHANGELOG.md marker fix; the three artefact files (untracked retrofit dispatch plan verbatim; triage report; dispatch plan); the execution-log section appended to the triage report; A15's deletion if its condition held. Standalone commits (rebase-merge hygiene): docs(changelog): remove stray merge-conflict marker lines for A12; docs(artifacts): land the backlog cleanup round artefacts for A13/A14/A16 (and A15 if taken).
5. Run the gates that can move for this diff locally (cargo fmt --all -- --check as smoke — no Rust surface changes are intended); the full AGENTS §10 suite runs at stage 3 through scripts/run-quality-gates.sh. No push, no PR — shipping is stage 3's last task. Never push to or merge main.
6. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization) — "no push" never means "no issues"; list the issue numbers in your report.

Report: per-A-item outcomes, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the backlog-cleanup round (branch chore/backlog-cleanup-round).

1. Read AGENTS.md, the triage report .artifacts/reports/2026-11-16-backlog-cleanup-triage-report.md (§A/§B normative), the dispatch plan .artifacts/plans/2026-11-16-backlog-cleanup-round-dispatch-plan.md §1/§2 (the evidence & artefact mandate in §1 applies to you in full), and the stage-1 plan; then review the branch diff (git diff main...HEAD) in ../taskboard-WT-chore-backlog-cleanup-round.
2. Begin by rebasing onto main: git fetch origin --prune; rebase onto origin/main (drop already-merged duplicate commits; resolve only real conflicts); push --force-with-lease if history was rewritten. Review versus the post-rebase base.
3. Functional focus — this chunk's specific traps: (a) GitHub-side work leaves no diff, so re-verify every A-item with gh issue view / gh label list yourself — a claimed-but-absent comment, label, closure, or milestone is a finding; (b) A4's sweep must have touched only backlog:-titled open issues — check the log's sweep output against the filter; (c) A10 closures must cite in-repo file:line evidence, never vibes — a closure without verifiable evidence is reverted (reopen the issue) and logged; (d) A13's file must be byte-identical to the operator's working-tree copy; (e) the PR diff must contain no code file; (f) A12's fix must not alter any CHANGELOG entry text beyond removing the stray markers; (g) A11's three issues must still be open.
4. Evidence & artefact mandate (verbatim from §1): every factual claim carries evidence (the command run + observed outcome, or a verifiable source + what it states); recommendations are decision-grade; repo hygiene (scratch notes and raw gh dumps never land — evidence is excerpts inside the execution log; the PR diff is exactly the chunk's artefacts); scope discipline (out-of-scope ideas go to the backlog, not into the report).
5. Re-run the full AGENTS §10 gate suite yourself through scripts/run-quality-gates.sh (house law: before every handoff; docs-only does not waive it); any failure is a finding.
6. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
7. Ship: push the branch, open the PR to main with gh — body carries Fixes #698 and a one-paragraph summary of the GitHub-side actions — then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green; then gh pr view --json mergeStateStatus must report CLEAN (BEHIND/DIRTY ⇒ rebase, push, repeat).
8. Backlog issues: open/comment on `backlog:` GitHub issues autonomously for any out-of-scope finding (AGENTS §7 standing authorization); list the issue numbers in your report.
9. Teardown: from the project root, git -C /home/viktoria/src/taskboard worktree remove --force ../taskboard-WT-chore-backlog-cleanup-round (everything is pushed; a resuming round starts a fresh chore/backlog-cleanup-round-v2 branch per AGENTS §8). Confirm with git worktree list that the removed path was the intended one.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation, merge-state confirmation, worktree-removed confirmation. Never merge; never touch main.
```

## 3. Completion bookkeeping

- **Closes via the PR**: #698 (`Fixes #698` in the body). **Closes
  GitHub-side in stage 2**: #654 (dup), #767, #842, plus any A10
  premise-dead closure (evidence-gated). Everything else in the
  catalogue only annotates — the issues stay open for their owning
  rounds (the UX triage series for the GUI review wave, the settings
  Files family for a future design round, #113/#114/#115 for the
  operator's intent).
- **Deliberately not dispatched here**:
  - **#731** (cargo-deny CVE under sqlx, CI red on `main`, p2/security)
    — per report §C this is immediate separate work, not cleanup:
    diagnose-and-fix on its own branch or an explicit accepted-risk
    entry in register #284, on the operator's go. **Flagged to the
    operator at handoff.**
  - **#782** (`// slint:` seams for ux-coverage.sh) — attaches to the
    active UX review wave (round 8), not to cleanup.
  - **#113/#114/#115** closures — user intent gate; this round asks,
    never closes.
  - **Milestone promotions** — only per DISPATCH-TIME OVERRIDE (A6).
- **What opens next**: the 2026-11-16 round-8 planning-agent intake
  (delivered directly to the operator as a prompt fragment, not an
  artefact of this dispatch) folds the UX triage series #807–#823 and
  the other GUI-wave issues into the gui review round 8 planning; the
  committed retrofit triage dispatch plan (A13) becomes a citable
  origin for that wave.
