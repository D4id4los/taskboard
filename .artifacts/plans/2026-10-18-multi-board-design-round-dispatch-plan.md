# [Plan] Multi-board Wave — Stage-0 Design Round — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  research → review) for the 2 chunks that take the multi-board wave
  from "questions open" to "detailed design accepted + wave
  implementation plan ready to dispatch". No code in this commit.
- **Origin**: the accepted framing plan
  `.artifacts/plans/2026-10-18-multi-board-wave-design-plan.md`
  (PR #333) — §3 verified baseline (normative anchors @ `878bb12`),
  §4 wave-level decision record D-W1…D-W9 (binding; D-W9's ruling is
  reserved to the user), §5 the nine Stage-0 design questions
  (chunk 1's work list), §6 implementation chunk sketch (advisory
  input only — the round may re-chunk), §7 sequencing. Plus the
  user's 2026-10-18 dispatch request, which introduces the **stage-3
  protocol amendment**: review agents rebase onto `origin/main`
  *first* and hand off only at merge-state `CLEAN` (§1; the
  dispatch-plan template is amended in the same PR so all future
  dispatch plans inherit it). Amended same-day with the
  **disk-hygiene protocol** (origin:
  `.artifacts/reports/2026-10-18-agent-worktree-disk-usage-report.md`;
  AGENTS §8/§10 and the template amended in the same PR): doctor
  pre-flight before (re)creating a worktree, coverage-dir cleanup on
  every full-gate run, and stage-3 worktree teardown at handoff.
- **Status**: READY for chunk 1 once the framing plan's PR (#333)
  merges; chunk 2 is **gated on a human decision session** — the
  user's rulings on chunk 1's recommendations (D-W9 SyncStatus
  keying included) are an input the dispatcher appends verbatim to
  the chunk-2 prompts. **Deliberately not dispatched here**: the
  implementation round — per the framing plan §7 item 3 and the
  dispatch-plan template's own rule ("a dispatch plan chunks an
  already-accepted body of work"), implementation gets its own
  dispatch plan after the wave implementation plan (chunk 2's
  deliverable) is accepted and remediation-wave-1 closes.

## 1. Chunks & ordering

| Chunk | Framing-plan slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | §5 Q1–Q9 (all nine) | `docs/multi-board-detailed-design` | the detailed design-of-record **draft**: every question resolved with options, trade-offs, exactly one recommendation; decision record left PROPOSED for the user | L |
| 2 | §7 item 3 | `docs/multi-board-impl-plan` | post-acceptance: write the user's rulings as the ACCEPTED decision record; update the framing plan; write the wave implementation plan (kiosk-exemplar style) | M |

**Dependency graph**: chunk 1 → **human gate** (the user's decision
session on chunk 1's PR) → chunk 2. Strictly serial; no stacked PRs.

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); the two chunks never run
concurrently. Every chunk works in `../taskboard-WT-<branch>` next
to the project root (AGENTS §8), branched off the then-current
`main`, no PR ever stacked. Disk hygiene (AGENTS §8 worktree
lifecycle, amended 2026-10-18): any agent creating or recreating a
worktree runs `scripts/worktree-doctor.sh` from the main checkout
first — nonzero exit (free disk below the floor) stops the chunk;
stage 3 removes the worktree as its final handoff task.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the chunk's work plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style — for these research chunks: the investigation method per
   question (which seams and docs to re-verify, which option space
   to consider, what evidence each option needs), the deliverable's
   outline, and every method-level open question resolved with a
   recorded decision. Commits it (`docs(plan): …`). Does not write
   the deliverable itself; does not push.
2. **Stage 2 — research agent**: continues in that worktree
   (recreates it on the branch if missing), executes the approved
   plan, produces the deliverable artefact(s). Reports deviations.
   Does not push or open a PR.
3. **Stage 3 — review agent** — *protocol amended 2026-10-18 at the
   user's request; the dispatch-plan template carries the same text
   from now on*: **begins by rebasing onto `main`** — `git fetch
   origin --prune`, then rebase the branch onto `origin/main`
   (AGENTS §8's rebase dance: resolve nothing unless real conflicts
   appear; drop already-merged duplicate commits), and push
   `--force-with-lease` if the rebase rewrote history; the review
   examines the diff against the **post-rebase** base. Then: reviews
   the entire branch diff versus that base against the chunk's
   acceptance criteria + AGENTS rules **plus the mandate below**,
   re-runs whatever gates apply, implements fixes for every finding
   on the branch (amend/reword, never duplicate-subject fixups). Its
   last tasks are shipping: push the branch, open the PR to `main`
   with `gh`, watch `gh pr checks --watch` to completion — any
   failing check is a finding; diagnose, fix, push
   `--force-with-lease`, repeat until green — **and then verify the
   PR's merge state**: `gh pr view --json mergeStateStatus` must
   report `CLEAN`; `BEHIND` or `DIRTY` ⇒ rebase onto `origin/main`,
   push `--force-with-lease`, let the checks re-run, and repeat;
   any other state ⇒ diagnose before handing off. **Then teardown**:
   remove the now-disposable worktree — from the project root,
   `git worktree remove --force ../taskboard-WT-<branch>` (everything
   precious is pushed; a later round that resumes the chunk recreates
   it on the branch). The handoff contract is **"PR open + checks
   green + merge state `CLEAN` + worktree removed"** so the user's
   merge on GitHub needs no additional rebase and the chunk leaves no
   36–55G of build output behind. Reports a verdict. Never merges;
   never touches `main`.

**Evidence & artefact mandate** (stage 3, every chunk — research
flavour, per the dispatch-plan template):

- every factual claim carries evidence (the command run + observed
  outcome, or a verifiable source + what it states) and experiments
  are reproducible from the artefact's method section;
- recommendations are decision-grade (options, trade-offs, exactly
  **one** recommendation, consequences for later work);
- repo hygiene (scratch code and captured payloads never land —
  evidence is excerpts inside the artefact; the PR diff is exactly
  the chunk's artefacts);
- scope discipline (out-of-scope ideas are recorded in the
  deliverable's backlog-candidates section for the user to promote
  to GitHub issues per AGENTS §7 — **no autonomous issue creation,
  no tracker edits, no production code from these chunks**).

## 2. Chunk 1 — detailed design of the wave (`docs/multi-board-detailed-design`)

- **Covers** (framing plan §5, normative — all nine questions, one
  coherent design; a per-question answer that contradicts another
  is a defect):
  1. board parameter shape across **all four vocabularies**
     (Q1 · F16-06 + F16-23 — the nine `live_board` sites; the
     outbox ops are already board-addressed post-#334);
  2. SyncStatus keying + the F16-22 three-layer migration (Q2 —
     options with one recommendation; **the user rules**, D-W9);
  3. connector addressing home (Q3 · F16-05 — incl. the resume-path
     pick and `App.connectors.first()`);
  4. trait promotion shape + draft ADR 0010 amendment text (Q4 ·
     D-W4 — the `SyncStateReader`/`BoxFuture` port pattern; narrow
     vocabulary seam, not a plugin system);
  5. ceremony redesign: multi-binding, unbind (#179), title-match
     attach (#170), reset/delete of one board among N generalising
     #334's `destruct_on` ceremony (Q5 · F16-24);
  6. config `[[sync.connectors]]` + the legacy-scalar resolution
     rule with loudness (Q6);
  7. all-down network-authority mechanics (Q7 · D-W7 — who
     aggregates, who emits, UI banner semantics);
  8. UI board dimension + keyboard-first switcher in the
     actions/keymap registry (Q8 · F7-05, R4/R1/R9);
  9. migration & compat plan under D-W6 (Q9 — the 0005 precedents:
     epoch-default columns, in-migration re-tag with decode
     rejecting the legacy tag).
  Binding constraints: D-W1…D-W8 may not be contradicted (deviations
  need a recorded reason and go back to the user); the WI-J §4.5
  topology invariant is the design's acceptance test; AGENTS §2
  domain purity for the trait port; the audit's "no speculative
  prep" stance — this round produces a design, zero production code.
- **Deliverable**: `.artifacts/plans/<today>-multi-board-detailed-design-plan.md`
  (status PROPOSED; its decision record is explicitly left for the
  user's session) plus the stage-1 work plan — the entire allowed
  diff.
- **Acceptance**: the checklist stage 3 reviews against —
  (a) every question answered with options/trade-offs/exactly one
  recommendation/consequences; (b) Q1's shape shown applied to all
  four vocabularies (commands, outbox ops, `CommandError` variants,
  status payloads) — a design scoping one but not all four is
  half-run and is a finding; (c) Q2's migration written against
  0001/0005 conventions and the D-W6 policy, with the keying
  options posed for the user rather than self-ruled; (d) Q4 includes
  the draft ADR 0010 amendment text; (e) Q8 names the concrete
  actions-registry/keymap integration points; (f) every code anchor
  cited was re-verified on the branch's main base (report drift by
  finding ID — IDs are the stable keys, line numbers are not); (g)
  gates green; (h) PR merge state `CLEAN`.

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the multi-board design-round dispatch ("detailed design of the wave", branch docs/multi-board-detailed-design) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: verify the framing plan .artifacts/plans/2026-10-18-multi-board-wave-design-plan.md is on current main (git log --oneline main -- <path> after git fetch origin --prune; it arrives with PR #333). If it is not there, STOP and report instead of planning.

1. Read AGENTS.md, then the framing plan in full (all sections normative except §6, which is advisory input) and the dispatch plan .artifacts/plans/2026-10-18-multi-board-design-round-dispatch-plan.md §1/§2.
2. Read the inputs the design must digest: audit findings report entries F16-05/06/07/08/22/23/24/25, F3-04, F7-05, F8-06 (with their evidence anchors); connect-flow design plan §4.2/§4.5 + ADR 0014 (the topology invariant); ADR 0010 (the vocabulary stance the trait amendment rewrites); the #334-landed seams named in framing §3; the GUI plans' R1/R4/R9.
3. Create the worktree ../taskboard-WT-docs-multi-board-detailed-design on a new branch docs/multi-board-detailed-design off main.
4. Write the work plan at .artifacts/plans/<today>-multi-board-design-round-workplan-plan.md: per question — the seams to re-verify (record today's line numbers; finding IDs are the stable keys), the option space to consider, the evidence each option needs; then the deliverable's outline and a risk list. Resolve every method-level open question with a recorded decision. You are planning HOW stage 2 investigates — you do not answer the design questions here.
5. Commit it as: docs(plan): plan the multi-board detailed design round. Do not write the design; do not push.

Report: plan path, method decisions, risks, estimated artefact size.
```

**Prompt 1.2 (research):**

```text
You are the stage-2 RESEARCH agent for chunk 1 of the multi-board design-round dispatch ("detailed design of the wave").

1. Read AGENTS.md, the framing plan (normative except §6), the dispatch plan §1/§2, and the stage-1 work plan (.artifacts/plans/*multi-board-design-round-workplan*-plan.md) in the worktree ../taskboard-WT-docs-multi-board-detailed-design (branch docs/multi-board-detailed-design; recreate the worktree on that branch if missing).
2. Execute the work plan: re-verify every anchor you will cite against the worktree's code, then write .artifacts/plans/<today>-multi-board-detailed-design-plan.md resolving framing §5 Q1–Q9 — each with options, trade-offs, exactly one recommendation, and consequences for later work; one coherent design, no per-question contradictions. Binding constraints: D-W1…D-W8 (D-W9 is posed with options + recommendation, never self-ruled); the WI-J §4.5 topology invariant is the acceptance test — any answer that needs to change connect-flow §4.2's topology is wrong; AGENTS §2 domain purity for the trait port; zero production code, zero tracker edits. Include the draft ADR 0010 amendment text (Q4) and the F16-22 migration design against 0001/0005 conventions (Q9). Out-of-scope ideas go to a backlog-candidates section.
3. Run the full gate suite from AGENTS.md §10 (docs-only diff; the pre-commit hooks run regardless).
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-question recommendation summary, deviations from the work plan and why, artefact path.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the multi-board design-round dispatch (branch docs/multi-board-detailed-design).

1. First, rebase onto main: in ../taskboard-WT-docs-multi-board-detailed-design run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the framing plan (§3/§4/§5 normative, §6 advisory), and the dispatch plan §1/§2 — the evidence & artefact mandate applies to you in full.
3. Functional focus — this chunk's specific traps: (a) one-motion violations: a Q1 answer scoping commands but not outbox/errors/status payloads is half-run; (b) answers contradicting D-W1…D-W8, or D-W9 self-ruled instead of posed to the user; (c) any design that re-opens connect-flow §4.2's channel topology (the WI-J invariant is the acceptance test); (d) unevidenced or stale-anchored claims — spot-check anchors and doc citations against the worktree; (e) migration design ignoring the 0005 precedents or 0001's conventions, or breaking D-W6 without concrete benefit; (f) trait design drifting toward a plugin system; (g) scope leakage (implementation detail, tracker edits, code) instead of a design; (h) per-question answers that contradict each other.
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last tasks — ship: push the branch, open the PR to main with gh, then gh pr checks --watch to green; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat; any other state ⇒ diagnose. Handoff contract: PR open + checks green + merge state CLEAN — the user's merge must need no additional rebase. Never merge; never touch main.

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation.
```

## 3. Chunk 2 — acceptance finalisation + wave implementation plan (`docs/multi-board-impl-plan`)

- **Precondition (human gate)**: chunk 1's PR is merged AND the
  user's decision session is recorded — the dispatcher appends the
  user's rulings verbatim to prompts 2.1 and 2.2 (including the
  D-W9 ruling). If either is missing, the chunk does not start.
- **Covers**: (a) write the ACCEPTED decision record into the
  detailed design (each Q with the user's ruling; where a ruling
  deviates from the recommendation, note it and its consequences);
  (b) update the framing plan — mark §5's questions resolved with
  pointers into the design doc, progress §7's sequencing;
  (c) write `.artifacts/plans/<today>-multi-board-wave-implementation-plan.md`
  in the kiosk-exemplar style (`2026-10-16-kiosk-mode-wave1-plan.md`):
  a "design facts every chunk relies on" section distilled from the
  accepted design, one section per implementation chunk with branch
  name (AGENTS §8 conventions), files/behaviours/explicit
  non-goals, and size; the quality-gates section; the sequencing
  section; and a completion-bookkeeping section naming which
  milestone issues (#125, #170, #179, #187, #298) each chunk closes
  or annotates. Re-chunk the framing §6 sketch freely per the
  accepted design — the sketch was advisory.
- **Deliverable**: the design-doc decision record edit, the framing
  plan edit, the new implementation plan — the entire allowed diff.
- **Acceptance**: the implementation plan's chunks each name their
  normative origin sections in the accepted design; every branch
  name is valid per AGENTS §8; no accepted decision is re-opened
  (deviations need a recorded reason and go back to the user); a
  reader who never saw the design doc can plan an implementation
  chunk from the implementation plan alone; gates green; merge
  state `CLEAN`.

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the multi-board design-round dispatch ("acceptance finalisation + wave implementation plan", branch docs/multi-board-impl-plan) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: verify chunk 1's design doc (.artifacts/plans/*multi-board-detailed-design*-plan.md) is on current main and carries the user's ACCEPTED decision record. If either is missing, STOP and report instead of planning.

The user's rulings from the decision session (verbatim, dispatcher-appended):
<USER RULINGS — append verbatim before dispatching this prompt>

1. Read AGENTS.md, the framing plan, the accepted design doc, the dispatch plan §1/§3, and the kiosk exemplar 2026-10-16-kiosk-mode-wave1-plan.md (the style to imitate).
2. Create the worktree ../taskboard-WT-docs-multi-board-impl-plan on a new branch docs/multi-board-impl-plan off main.
3. Write the work plan at .artifacts/plans/<today>-multi-board-impl-plan-workplan-plan.md: the implementation-plan chunk decomposition you propose (mapping each chunk to the accepted design's normative sections and to the milestone issues it closes/annotates), the framing-plan edit list, and every method-level open question resolved with a recorded decision.
4. Commit it as: docs(plan): plan the multi-board implementation-plan round. Do not write the deliverables; do not push.

Report: plan path, proposed chunk decomposition, risks.
```

**Prompt 2.2 (research):**

```text
You are the stage-2 RESEARCH agent for chunk 2 of the multi-board design-round dispatch ("acceptance finalisation + wave implementation plan").

1. Read AGENTS.md, the framing plan, the accepted design doc, the dispatch plan §1/§3, the stage-1 work plan, and the kiosk exemplar in the worktree ../taskboard-WT-docs-multi-board-impl-plan (branch docs/multi-board-impl-plan; recreate the worktree on that branch if missing).
2. Execute the work plan: (a) write the ACCEPTED decision record into the design doc using the user's rulings recorded on main; (b) update the framing plan (§5 questions resolved with pointers, §7 sequencing progressed); (c) write .artifacts/plans/<today>-multi-board-wave-implementation-plan.md in the kiosk-exemplar style — design facts, per-chunk sections with branch names and explicit non-goals, quality gates, sequencing, completion bookkeeping (milestone issues #125/#170/#179/#187/#298 per chunk). Zero production code; zero tracker edits.
3. Run the full gate suite from AGENTS.md §10 (docs-only diff; the pre-commit hooks run regardless).
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: chunk decomposition summary, deviations from the work plan and why, artefact paths.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the multi-board design-round dispatch (branch docs/multi-board-impl-plan).

1. First, rebase onto main: in ../taskboard-WT-docs-multi-board-impl-plan run git fetch origin --prune, then git rebase origin/main (drop already-merged duplicate commits; resolve only real conflicts), and git push --force-with-lease if the rebase rewrote history. Review the full branch diff (git diff origin/main...HEAD) against the post-rebase base.
2. Read AGENTS.md, the accepted design doc, the framing plan, and the dispatch plan §1/§3 — the evidence & artefact mandate applies to you in full.
3. Functional focus — this chunk's specific traps: (a) the decision record must reproduce the user's rulings faithfully — no silently adopted recommendations the user overruled; (b) the implementation plan must not re-open accepted decisions or smuggle in new ones; (c) every implementation chunk needs normative origin references, a valid branch name, and explicit non-goals — a chunk whose boundary must be re-derived is a finding; (d) the framing-plan edit must leave the historical decision record (§4) intact — it records, it does not rewrite; (e) scope leakage (code, tracker edits, speculative chunks beyond the accepted design).
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Ship: push the branch, open the PR to main with gh, then gh pr checks --watch to green; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat. Then verify gh pr view --json mergeStateStatus reports CLEAN; BEHIND/DIRTY ⇒ rebase onto origin/main, push --force-with-lease, let checks re-run, repeat; any other state ⇒ diagnose. Handoff contract: PR open + checks green + merge state CLEAN + worktree removed — the user's merge must need no additional rebase. Never merge; never touch main.
7. Teardown: from the project root, git worktree remove --force ../taskboard-WT-docs-multi-board-impl-plan (everything is pushed; a resuming round recreates the worktree on the branch via git worktree add).

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks + CLEAN confirmation, worktree-removed confirmation.
```

## 4. Completion bookkeeping

- **Chunk 1** closes and annotates no tracker issues — the design
  round is not any milestone issue's completion; its PR is the
  design-of-record draft's home pending the user's session.
- **Chunk 2** updates the framing plan and the design doc statuses;
  the implementation plan's own bookkeeping section plans the
  milestone-issue closures for the implementation round.
- **Deliberately not dispatched here**: the implementation round
  (its own dispatch plan, written off the accepted implementation
  plan after remediation-wave-1 closes); any tracker edits (issues,
  labels, milestones stay the user's / the implementation round's
  business); ADR text beyond the draft (the ADR 0010 amendment
  lands with the trait-promotion implementation chunk).
- **What opens next**: the user merges chunk 2's PR and accepts the
  implementation plan → the implementation dispatch round (template:
  `.artifacts/templates/dispatch-plan.md`, inheriting the amended
  stage-3 protocol).
