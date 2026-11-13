# [Plan] UX Journey Framework Phase-2 Pilot — Chunk 2 (Comparison, Round-7 Adjustments, Census Seed)

- **Date**: 2026-11-13
- **Kind**: stage-1 chunk plan for pilot chunk 2 — the TDD-first plan
  the stage-2 implementation agent executes and the stage-3 review agent
  reviews against. No application code; docs/plans/census artefacts only.
- **Origin**: the pilot dispatch plan
  `.artifacts/plans/2026-11-13-ux-journey-pilot-dispatch-plan.md` §1/§3
  (authority for this chunk's scope and prompts); the framework plan
  `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md`
  §4.3 (gate), §4.4 (walkthrough protocol + census), §6.1 (pilot
  discipline steps 2–4); the installed law `docs/ux_planning.org`
  §2.3–2.4/§3; the three blind UX plans
  (`.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md`,
  `.artifacts/plans/2026-11-13-r7-recur-editor-ux-plan.md`,
  `.artifacts/plans/2026-11-13-r7-files-roster-ux-plan.md`, merged via
  PR #766); the document this chunk will amend,
  `.artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md`
  (§1/§4, chunk 3), with the round-7 design plan §2 D-7.1/D-7.2/D-7.3
  as the tie-break design intent.
- **Status**: READY — both preconditions verified (below). Stage 2
  executes §3; stage 3 reviews against §6 acceptance.

## 1. Preconditions & baselines (verified 2026-11-13)

| Input | State | Evidence |
|---|---|---|
| Chunk 1's PR (three blind UX plans) | MERGED | PR #766, mergedAt 2026-11-13T18:20:59Z; the three `2026-11-13-r7-*-ux-plan.md` files on `origin/main` |
| Round-7 chunk 2 (recur editor) | MERGED (not merely open) | PR #765 `feat/ui-r7-recur-editor`, mergedAt 2026-11-13T17:19:32Z; commits `fef28b9d` (domain extraction) … `c807f88b` (editor overlay) are on `origin/main` verbatim (rebase merge) |
| Round-7 chunk 1 (habit create) | MERGED (earlier pilot input) | PR #758, commits `6406974e`/`814008e1` on `origin/main` |

**Decision D1 — comparison baseline is a merged-main SHA, not a PR
branch.** Both round-7 implementation chunks are already merged, so
dispatch §3's fallback ("open PR branch at a fixed SHA + post-merge
re-verify") is **not used** — recorded as the reason that mechanics
exist but does not apply. Stage 2 pins the evidence baseline at
**`origin/main` @ `fb5863ed`** (the tip as of this plan; the three UX
plans themselves are the three commits immediately below it). If any
further merge lands on `origin/main` while this chunk runs, the
evidence anchors stay pinned at the recorded SHA (the worktree checkout
is the pinned SHA; `--root` needs no override — the gate defaults to
the script's git toplevel, which inside the worktree *is* the pinned
tree). A post-merge re-verify step is unnecessary; if the round-7
dispatch's chunk 3 merges before this chunk's PR opens, stage 2 re-records
the baseline SHA in the comparison plan rather than silently comparing
across two trees. The files-roster plan is compared against the same
tree with **all-MISSING recorded as the correct pre-state** (its chunk
has not run), per dispatch §3/§6.1 — informational evidence, not a
failure.

## 2. Decision D2 — the comparison protocol

Per UX plan, in this order:

1. **Mechanical gate.** `scripts/ux-coverage.sh <plan> --gate` from the
   worktree at the pinned baseline (habit-create and recur-editor
   expected mostly FOUND; files-roster expected all-MISSING as the
   recorded pre-state). The full per-row output (FOUND `file:line` /
   MISSING lines + counts) is pasted into the comparison plan's
   evidence appendix — the gate output *is* the mechanical evidence,
   not summarized prose. Exit codes: 0 (green), 1 (MISSING rows —
   data, not error), 2 (parse error — always a finding), 64 (usage —
   always a stage-2 error).

2. **Walkthrough protocol.** For every P1 story of each plan, per
   journey step, the four cognitive-walkthrough questions
   (`docs/ux_planning.org` §2.4) answered with evidence:
   - **Evidence form**: `file:line` anchors in the implementation tree
     at the pinned SHA plus existing test names
     (`file::test_name`) — never prose-only claims. Question 1
     (see the control) anchors the affordance/callback in the
     `.slint` source; question 2 (recognize) cites the label/hint
     source site as a *judgment citation* (no-text-assertion rule:
     judged, never asserted in tests); question 3 (feedback)
     anchors the badge/tone/echo/fold computation; question 4
     (recover) anchors the escape path or cites the test asserting
     the typed outcome.
   - **Method**: reasoned over code by default. The existing UI test
     driver (`i-slint-backend-testing`) is consulted **only** where a
     journey test already exists (cheap read, no authoring) — this
     chunk is docs-only and writes no tests; if an evidence question
     cannot be answered without a new test, the verdict is `missing`
     with the gap named, and a re-work item or backlog issue carries it.
   - **Assumption resolution**: each plan's recorded assumptions
     (habit-create A-1/A-2, recur-editor A-3/A-4/A-5, files-roster
     A-6) were explicitly unfixed at plan time — implementation
     choices differing from an assumption's *default suggestion* are
     **not re-work findings**; they resolve as **plan corrections**
     (the assumption section is amended with the bound name,
     `recorded ruling` style). A deviation on something the plan fixed
     as *normative* (entry points, escape paths, command shapes,
     kiosk posture) is a real finding.

3. **Heuristic pass.** One six-heuristic pass over the touched
   surfaces (Today/habits, Task detail/recur, Settings/files) per
   §2.4's rubric; findings classify BLOCKER / MAJOR / MINOR. BLOCKER /
   MAJOR become re-work items in the comparison plan (this chunk does
   not fix code); MINOR become `backlog:` issues immediately
   (AGENTS §7 standing authorization).

**Decision D3 — verdict vocabulary** (per story-step, in the verdict
table; one verdict per step, plus a per-story roll-up):

- `satisfied` — the plan's claim for this step is implemented and
  evidenced (gate row FOUND + walkthrough answers evidenced). The
  plan's §8 verification names a test that does not exist yet *only*
  blocks if the step's behavior has no test at all elsewhere; a
  differently-named test covering the behavior is a plan correction
  (test-name mismatch is the cheapest class of correction).
- `deviating` — implemented, but differs from what the plan fixed:
  different affordance shape, different command usage, missing entry
  point, weaker escape path, kiosk posture change. Every deviation
  resolves as **plan correction** (amend the UX plan with a recorded
  ruling — the amendment cites the comparison plan as authority) or
  **re-work item** (scope + suggested fresh branch), decided by the
  tie-break rule below and recorded in the comparison plan.
- `missing` — the plan's claim has no implementation at all (gate row
  MISSING and no walkthrough evidence). Always a re-work item (or, for
  the files-roster plan, the recorded pre-state).

**Decision D4 — tie-break rule.** Implementation and plan disagree ⇒
**the design intent of the round-7 plan §2 (D-7.1/D-7.2/D-7.3) wins**
unless the UX plan amended that intent with a recorded ruling (its
Rulings table, a non-goal with home, or an explicit recorded deviation)
— in that order of authority: UX-plan recorded ruling > round-7 plan
§2 > blind-plan default > implementation choice. An implementation that
matches round-7 §2 but deviates from the blind plan's *unruled* claim
is a plan correction, not re-work; an implementation that deviates from
both is re-work. Every deviation's resolution names its authority
explicitly — "the comparison plan is the recorded authority for every
amendment" (dispatch §3) is binding.

## 3. Stage-2 execution order

1. Pin and record the baseline SHA (D1) at the top of the work log;
   `git checkout` the worktree is already at the branch tip — evidence
   greps read the pinned tree via `git show <sha>:<file>` where a
   file moved after `fb5863ed`.
2. Gate + walkthrough per plan (D2), habit-create → recur-editor →
   files-roster (files-roster last: it is the recorded pre-state, so
   its evidence is a short gate run + a no-implementation note).
3. Write `.artifacts/plans/2026-11-13-round7-ux-pilot-comparison-plan.md`
   (house `[Plan]` style): per-plan verdict tables (D3), deviation
   resolutions with authorities (D4), the validation-assertion outcome
   (§5), the honest note that an empty re-work list is a valid
   outcome, and the gate-output appendix.
4. Amend the UX plans that received plan corrections (ruling-style
   recorded amendments; table headers stay token-exact — the gate must
   still parse, verified before commit).
5. Amend `.artifacts/plans/2026-11-13-gui-review-round7-dispatch-plan.md`
   (D5/D6).
6. Create `docs/ux-surface-census.org` (D7).
7. Gates via `scripts/run-quality-gates.sh`; docs-only diff, so a
   pre-existing unrelated failure is STOP-and-report (dispatch §3).

## 4. Decision D5 — amendment mechanics for the round-7 dispatch plan

The round-7 dispatch plan is amended **in place** (framework §6.1 step
3a: it is "a living dispatch document until its chunks complete" —
this is the sanctioned in-place case; accepted *analysis* plans are
never rewritten, which is why the UX plans get ruling-style amendments
instead). Every change carries a bracketed amendment note:
`[amended 2026-11-13 — authority: <comparison-plan-path>, §<n>]`.
Untouched chunks stay verbatim: chunk 1/§2 and chunk 2/§3 prompts are
**not** edited (the re-work mechanism handles their gaps additively);
only a status note in the header records which chunks merged and which
prompts were superseded by the framework re-authoring.

1. **Chunk-3 prompt re-authoring** (§4 of the round-7 dispatch plan):
   prompts 3.1–3.3 are replaced with framework-shaped prompts,
   following the pilot dispatch plan's own prompt grammar:
   - 3.1 reads the **files-roster UX plan as the normative origin**
     (citing `US-n`, `US-n/S-k`, `W-x`, and `R-x` IDs the same way it
     now cites origin sections by number — the dispatch-template
     Appendix B fill-in rule), alongside round-7 plan §3 D-7.3;
   - 3.2's hard rules gain: "implement exactly the wiring-inventory
     rows this chunk claims; embed journey-step tokens in test names;
     run `scripts/ux-coverage.sh <files-roster-plan> --gate` before
     reporting done (MISSING on a claimed row is not done)";
   - 3.3 gains the **UX completeness mandate**: re-run the gate, then
     the walkthrough protocol (four questions per P1 story with
     evidence, six-heuristic pass); BLOCKER/MAJOR fixed on the
     branch, MINOR → backlog issues.
   - The §4 "Covers" line gains the UX-coverage mapping (the W-rows
     and stories the chunk implements; the P1-slice rule checked at
     planning time — all four US stories are P1 and this is the only
     chunk for the slice, so the chunk set must cover all four
     end-to-end or the dispatch is invalid).
2. **Re-work chunks**: appended as new chunk blocks (complete
   three-stage prompts included) after §4, one per re-work item whose
   resolution chose re-work, with **fresh branch names** —
   `feat/ui-r7-habit-create-ux-rework`, `feat/ui-r7-recur-editor-ux-rework`
   (never reusing a landed branch name — AGENTS §8 / issue #415). An
   **empty re-work list is a valid outcome** (framework §6.1 step 2):
   the amendment then records only the header status note and the
   chunk-3 re-authoring, and the comparison plan says so explicitly.
3. **Supersession rule (D6)**: a stage-1 implementation plan is
   **superseded, not amended**, when a re-work item changes that
   chunk's *scope or behavioral decisions* (new/removed affordances,
   changed command usage, changed escape/kiosk posture); the
   superseding stage-1 plan rides the re-work chunk's stage-1 agent
   (written fresh on the re-work branch, citing the comparison plan)
   — the round-6→round-7 overturn precedent, never in-place rewriting
   of accepted analysis. Name-level deltas within an unchanged scope
   (e.g. a bound assumption A-n) do **not** warrant supersession; the
   re-work chunk's own stage-1 plan records them.
4. The amended dispatch plan must still reference **all** of its
   original chunks (verified before commit — dispatch §3 acceptance).

## 5. Validation-assertion outcome (pilot proof)

Stage 2 records, in the comparison plan, the phase-2 exit criterion
(framework §6 phase 2): whether the blind plans, evaluated at
`35e6b0a1`, showed R7-1/2/3 as blank/unruled (the plans' own §1
validation-assertion sections claim this with baseline anchors — the
comparison plan re-checks the claim, e.g. re-running the anchors
`git show 35e6b0a1:…`, and records the verdict); and whether the gate
demonstrated real PASS and MISSING rows on the two merged plans
(the files-roster all-MISSING run supplies the MISSING demonstration).

## 6. Decision D7 — census seed structure (`docs/ux-surface-census.org`)

Org-mode, conventions matching `docs/ux_planning.org` (`#+title`,
`#+options: toc:t`, `*` headings, pipe tables):

- **Header block**: what the census is (framework §4.4 /
  `docs/ux_planning.org` §2.4: the living app-scale capability ×
  surface matrix + every standing ruling with date and authority);
  its status line (`Seeded: phase 2 pilot slices only (2026-11-13);
  full census = phase 3`); the evidence convention (every cell cites
  `file:line` at the recorded baseline SHA or an existing test name;
  the SHA is stated once in the header).
- **Exactly three slices, one `* Slice: …` heading each**:
  `Today (habits)` (from the habit-create plan), `Task detail
  (recurrence)` (from the recur-editor plan), `Settings (files sync)`
  (from the files-roster plan). **No phase-3 scope creep** — no Board,
  Review, Capture, or alarm-window sections; the header says the full
  census is phase 3's dedicated pass.
- **Per slice, two blocks**:
  1. the capability × surface matrix — the same fixed column set the
     UX plans use (Board / TaskDetail / Today / Review / Settings /
     Capture / CLI), rows = the union of that plan's matrix rows, each
     cell carrying the letter (D/E/C/X) + story ID + evidence anchor
     (`file:line` in the *implemented* tree at the pinned SHA — the
     census reflects current reality, not plan intent); a cell the
     comparison marked deviating/missing carries its verdict marker.
  2. a **Rulings** block — one table: `ID | Ruling | Reason | Home |
     Date | Authority`. Rows are the slice's UX-plan rulings (R-1…)
     **plus** standing rulings the slice touches, with date +
     authority on every row — including the round-6 → round-7
     overturn: the files-roster slice records the round-6
     "adoption is automatic, disconnect is a non-goal" ruling *and*
     its overturn by round-7 §2 D-7.3, dated, so the diff is visible
     (the exact C4 failure the census exists to kill). Authority
     values are artefact paths + section numbers (or issue numbers).
- The files-roster slice's matrix rows reflect the **baseline
  reality** (pre-chunk-3: title-only rows, no actions) with the UX
  plan's target cells annotated as planned-pending — the census is a
  snapshot of the app, and chunk 3 will update its slice (framework:
  "each UX plan updates its slice").

## 7. Commit sequencing (every commit standalone — rebase-merge hygiene)

Stage 2's branch carries this plan plus its artefacts; each commit
below builds and gates green on its own (docs-only, so "green" = the
suite passing, which the wrapper runs once at the end; standalone
means each commit tells one complete story):

1. `docs(plan): the ux-r7 pilot comparison chunk plan` — this file
   (already committed by stage 1).
2. `docs(plan): the round-7 ux pilot comparison plan` — the
   comparison plan (verdict tables, resolutions, gate appendix) **and**
   any UX-plan ruling amendments it authorizes ride this commit (one
   logical act: the comparison is the authority and the amendments
   implement it; the UX-plan files are named in the body).
3. `docs(plan): amend the gui-review-round7 dispatch for the ux pilot`
   — the round-7 dispatch amendment (D5/D6), body citing the
   comparison plan.
4. `docs(ux): seed the ux surface census with the pilot slices` — the
   census (D7).

No push, no PR — shipping is stage 3's. Stage 3 reviews
`git diff origin/main...HEAD` post-rebase against §6.

## 8. Acceptance (stage-3 checklist, condensed from dispatch §3)

- Verdict table complete for every story/step of all three plans,
  every verdict evidence-backed (file:line/test at the pinned SHA);
  stage 3 re-spot-checks ≥3 verdicts per plan incl. one subtle one
  (kiosk gating, escape paths, keyboard parity).
- Files-roster all-MISSING recorded as the correct pre-state.
- Comparison plan is the cited authority on every amendment; UX-plan
  amendments keep the gate parsing (headers token-exact).
- Amended round-7 dispatch plan: chunk-3 prompts genuinely
  framework-shaped, re-work blocks complete on fresh branches, chunks
  1–2 intact, all original chunks still referenced.
- Census: exactly three slices, evidence anchors, rulings dated +
  authorized, no phase-3 creep.
- No accepted plan rewritten in place.
- Gates green via the wrapper; PR open + green + `CLEAN`.

## Open questions — none outstanding

All decisions recorded (D1–D7). Deviation from the dispatch prompt's
text: none material — the prompt's "PR branch + fixed SHA + post-merge
re-verify" alternative was settled as not-applicable by evidence
(D1), and the prompt's comparison-plan filename
`<today>-round7-ux-pilot-comparison-plan.md` is adopted verbatim.

## Backlog issues opened by this stage

- #767 — the pilot dispatch plan itself is untracked/local-only
  (commit it). Listed here per AGENTS §7; stage 2/3 must list any
  further findings.
