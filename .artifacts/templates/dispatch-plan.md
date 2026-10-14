# Dispatch Plan Template

<!--
How to use: copy this file to
`.artifacts/plans/YYYY-MM-DD-<topic>-dispatch-plan.md`
(`...-implementation-dispatch-plan.md` when it dispatches an accepted
design plan; `...-wave<N>-dispatch-plan.md` for roadmap waves), delete
these guidance comments, and fill every `<...>` placeholder. Never
commit a filled-in copy into `templates/`.

A dispatch plan chunks an ALREADY-ACCEPTED body of work (a design
plan, a roadmap wave, a triage batch) into independently shippable
pieces and prescribes the three-stage agent execution for each chunk.
It contains no code and no implementation decisions — those belong to
the chunk plans its stage-1 agents write. Design questions that are
still open belong in a design round *before* the dispatch plan, not
in it.

Exemplars to imitate (newest house style first):
- `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  — research/docs chunks, human gates, evidence & artefact mandate
- `.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
  — the canonical implementation-chunk shape + code-quality mandate
- `.artifacts/plans/2026-10-12-reminders-implementation-dispatch-plan.md`
  — serial implementation chunks across crate boundaries

Fill-in quality bar: every chunk's prompts must name their normative
origin sections by number, name the chunk's specific traps (from the
origin's facts/risks — not generic advice), and leave nothing
implicit. A reader who has never seen the origin must be able to
dispatch the chunks from this document alone.
-->

# [Plan] <Topic> — Dispatch Plan & Agent Prompts

- **Date**: <YYYY-MM-DD>
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  <implementation | execution | research> → review) for the <N>
  <chunks | work items> of <one-line description of the accepted
  work being dispatched>. No code in this commit.
- **Origin**: <path(s) of the accepted design plan / roadmap /
  backlog-review session this dispatches, with the normative section
  numbers>, plus the user's <date> chunking/approval decision.
- **Status**: <READY | PARTIALLY READY> — what is dispatchable now;
  which chunks are gated on which merges or human decisions; and what
  is deliberately **not** dispatched here (<deferred items + their
  homes>).

## 1. Chunks & ordering

| Chunk | <Origin> slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | <WI-x / D-y / §z> | `<category>/<scope>-<desc>` | <one line: files + behaviour + explicit non-goals> | S/M/L |
| … | | | | |

**Dependency graph**: <which chunks are order-independent (both off
`main`, disjoint files — serial or parallel), which are gated on
which merges or on a human gate, and the default dispatch order.>

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk); independent chunks may run
concurrently as separate sessions; never two agents share a
worktree. Every chunk works in `../taskboard-WT-<branch>` next to
the project root (AGENTS §8), branched off the then-current `main`,
no PR ever stacked.

**Shared execution model** (binding for every chunk):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the TDD-first plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No implementation, no push. Origin
   sections marked normative are binding; deviations need a recorded
   reason.
2. **Stage 2 — <implementation | execution | research> agent**:
   continues in that worktree (recreates it on the branch if
   missing), executes per the approved plan — <implementation: TDD,
   every commit standalone (rebase-merge hygiene), full AGENTS §10
   gate suite green | research: the experiments and the deliverable
   report>. Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the mandate below**, re-runs whatever gates apply,
   implements fixes for every finding on the branch (amend/reword,
   never duplicate-subject fixups). Its last task is shipping: push
   the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.
   (<`main` is branch-protected — merges happen on GitHub by the
   user, after the required checks pass.>)

**<Code-quality | Evidence & artefact> mandate** (stage 3, every
chunk — pick per chunk kind):

- **Implementation chunks — code-quality mandate**: beyond functional
  review, evaluate (a) code quality: naming, comment discipline
  (comments state constraints, not narration), idiomatic use of the
  approved stack, no dead code or speculative generality, test
  quality per AGENTS §5; (b) the Single Responsibility Principle:
  the crate-role boundaries of AGENTS §2 are the SRP line — a
  boundary violation is an SRP violation; (c) a single level of
  abstraction per function body: a function either orchestrates
  named operations or works in primitive data, never mixes. Findings
  are fixed on the branch like any other finding. <Copy the full
  verbatim text from an existing implementation dispatch plan rather
  than paraphrasing.>
- **Research/docs chunks — evidence & artefact mandate**: every
  factual claim carries evidence (the command run + observed
  outcome, or a verifiable source + what it states) and experiments
  are reproducible from the report's method section; recommendations
  are decision-grade (options, trade-offs, exactly **one**
  recommendation, consequences for later work); repo hygiene
  (scratch code and captured payloads never land — evidence is
  excerpts inside the report; the PR diff is exactly the chunk's
  artefacts); scope discipline (out-of-scope ideas go to the backlog
  per its template, not into the report).

## 2. Chunk 1 — <name> (`<branch>`)

- **Covers**: <origin sections by number — normative. The exact
  files/commands/behaviours, and the explicit non-goals ("no X, no
  Y, no Z") so the boundary is not re-derived.>
- **Deliverable** <research/docs chunks only>: <report/doc paths +
  bookkeeping edits that make up the entire allowed diff.>
- **Acceptance**: <the checklist stage 3 reviews against: behaviours
  pinned by which tests/journeys, gates green, the exact diff shape,
  the bookkeeping edits.>

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk <N> of <dispatch-plan short name> ("<chunk name>", branch <branch>) in the taskboard repo at /home/viktoria/src/taskboard.

<Only if gated — precondition line: verify <merge/acceptance/state> before starting (git log / status header); if it does not hold, STOP and report instead of planning.>

1. Read AGENTS.md, then <origin docs with their normative section numbers> and the dispatch plan <path> §1/§<n>.
2. Read the current seams you will extend: <files + what to look for in each>, plus the tests you must not break: <paths>.
3. Create the worktree ../taskboard-WT-<branch> on a new branch <branch> off main.
4. Write a detailed TDD-first plan at .artifacts/plans/<today>-<topic>-plan.md: <enumerate the concrete decisions this plan must settle — shapes, semantics, seams, test catalogue>. Resolve every open question with a recorded decision; deviations from the origin need a recorded reason.
5. Commit it as: docs(plan): <subject>. Do not implement; do not push.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt 1.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk <N> of <dispatch-plan short name> ("<chunk name>").

1. Read AGENTS.md, <origin docs with section numbers>, the dispatch plan <path> §1/§<n>, and the stage-1 plan (.artifacts/plans/*<topic>*-plan.md) in the worktree ../taskboard-WT-<branch> (branch <branch>). Recreate the worktree on that branch if missing.
2. <Implement | Execute | Research> exactly per the approved plan, TDD where code is involved, every commit standalone (rebase-merge hygiene). <Chunk-specific hard rules: SPDX headers, #![forbid(unsafe_code)] boundaries, no sleeps in tests, hermetic-only tests, no text assertions, scratch code never lands, etc.>
3. Run the full gate suite from AGENTS.md §10 and make it green <research chunks: whatever gates apply — the pre-commit hooks run regardless>.
4. <Bookkeeping this chunk owns — backlog ✅ DONE / annotations, ADRs, architecture.org/README amendments — or: do NOT touch the backlog origin entries, chunk <M> lands them.>
5. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: per-gate results, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk <N> of <dispatch-plan short name> (branch <branch>).

1. Read AGENTS.md, <origin docs with section numbers>, the dispatch plan <path> §1/§<n> (the <code-quality | evidence & artefact> mandate applies to you), and the stage-1 plan; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-<branch>.
2. Functional focus: <this chunk's specific traps, named from the origin's facts/risks — the invariants that could silently break, the tests that must not have been weakened, the diff-shape rules. Not generic advice.>
3. <Code-quality | Evidence & artefact> mandate: <spell out the applicable one in full — copy verbatim from §1 of this dispatch plan.>
4. Re-run the full gate suite yourself (AGENTS §10); any failure is a finding.
5. Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until all checks are green. <main is protected: never merge; the user merges on GitHub.>

Report: verdict, findings (functional + mandate), what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — <name> (`<branch>`)

<Repeat the per-chunk block for every chunk: heading, Covers,
Deliverable (if research/docs), Acceptance, Prompts N.1/N.2/N.3 —
each prompt adapted to the chunk, never pasted verbatim from
another chunk.>

## <N+1>. Completion bookkeeping

- <Which backlog entries each chunk closes (✅ DONE + one-line note)
  and which it only annotates (entry stays open — the future chunk
  that will close it); where ADRs and docs/architecture.org /
  README amendments land.>
- **Deliberately not dispatched here**: <the deferred items with the
  backlog entries / future rounds that own them, so nobody
  re-derives the boundary.>
- <What opens next once the last chunk merges — the follow-up plan
  or dispatch round this work feeds.>
