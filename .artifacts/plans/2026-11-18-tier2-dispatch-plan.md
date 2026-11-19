# Tier-2 Round ("post-r9 parallel tier-2") — Implementation & Dispatch Plan

- **Date**: 2026-11-18
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  implementation/research → review) for the 9 chunks of the accepted
  Tier-2 set from
  `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
  §Tier-2: the two bug fixes (#846, #893), the CLI/grammar hygiene
  pair (#874+#873), the journey-gated UI/config features (#733,
  #909+#910), the move-ceremony injection (#751), the files-GC pair
  (#718→#755), the files-sync live cadence (#673), and the build-cache
  evaluation (#890). No code in this commit.
- **Origin**: the operator's 2026-11-18 instruction to plan Tier 2
  (= scope acceptance); issue bodies normative; the round-level
  decisions recorded below bind their stages. Verified against
  `main` @ Tier-1's last merge: #874's precondition holds (the
  `--to-connection` twin / `StateCommand::RebindBoard` landed with the
  r8 files-sync chunk); #893's site confirmed (`router.rs:1329`,
  `TaskDueToday` writes `day_window.end`); #909 needs a new `notify`
  file-watcher dependency (an AGENTS §4 stack-table addition — operator
  gate recorded in P6).
- **Status**: READY, under the standing resource constraint: **at most
  2 concurrent chunks beside r9** (W1/M1 in flight; the W-chain
  continues), staggered ≥3 min, flock-deferral per the codified law.
  P4 and P6 carry **journey-plan human gates**; P7's #755 leg and P9
  carry **operator gates**. P1 runs compressed (small diagnosis with a
  bounded fix). Deliberately not dispatched: the fold-into-r9 set
  (#775/#881/#880 — r9 owns them), armed/no-recurrence flakies,
  #327's sharded mutants lane, habit backlog #955–#963, O-5 remainder
  #946–#949, the perf seven, feature-scale items.
- **Excluded territory**: r9's chains (widgets/overlays W1–W7, habits
  H1–H4, keyboard K1–K4, menus M1–M2, day-review D2) — P1/P2 stage-1
  run explicit disjointness verdicts against the in-flight r8/r9
  branches where their seams sit near r9 surfaces; every other chunk
  lives in app/state/sync-files/CLI/test lanes r9 does not enter.

## 1. Chunks & ordering

| Chunk | Issues | Branch | Covers (one line) | UX coverage | Size |
|---|---|---|---|---|---|
| P1 | #846 | `test/golden-solo-render` | golden_visuals solo-fail diagnosis + determinism fix | no UX plan in origin (test rig) | S (compressed) |
| P2 | #893 | `fix/today-due-boundary` | TaskDueToday writes the exclusive end; align with the N-4 day-end law + re-anchor us6_s2 | no UX plan in origin (bug fix; census note rides the fix) | S/M |
| P3 | #874 + #873 | `fix/cli-verb-grammar-hygiene` | move-verb grammar collapse; date-grammar normalization decision landed | no UX plan in origin (CLI grammar; help text unchanged in shape) | S/M |
| P4 | #733 | `feat/ui-notification-unmute` | Unmute toggle on the notification surface | **journey plan P4-JP — mandatory, gated** | S |
| P5 | #751 | `feat/ceremony-identity-injection` | device-identity path injection through run_on/app_world; unlocks the two recorded fallback tests | no UX plan in origin (test seam + injection) | M |
| P6 | #909 + #910 | `feat/config-reload-batch` | live-reload watcher + batch config set | **journey plan P6-JP — mandatory, gated** (one plan, two stories) | M/L |
| P7 | #718 → #755 | `feat/files-gc-verb` | FilesTransport DELETE verb + owner GC pass; then the #755 unbind-subtree decision (**operator gate**) + its implementation | no UX plan in origin (sync internals; unbind surfacing is r8-shipped) | M/L |
| P8 | #673 | `feat/files-sync-live-cadence` | files-sync cadence follows live settings (doorbell re-arm) | no UX plan in origin (sync internals) | S/M |
| P9 | #890 | `docs/build-cache-evaluation` | sccache / shared CARGO_TARGET_DIR evaluation → ruling, operator-gated | no UX plan in origin (research) | M |

**Dependency graph**: no hard edges except P7 → P8 (same crate,
`taskboard-sync-files-nextcloud`; P8's stage 1 re-anchors on P7's
landed transport/actor changes — serialize). P2's disjointness verdict
may defer it past r9's K2/D2. Everything else is independent.
Default dispatch order under the ≤2-concurrent cap:

1. **P1 ∥ P2** (the two bugs; P1 compressed, P2 gated on its verdict).
2. **P3 ∥ P4.1** (journey stage-1).
3. **P5 ∥ P6.1** (journey stage-1).
4. **P7 ∥ P9** (P9 is low-contention research).
5. **P8** + the gated implementations as gates clear
   (P4.2–3, P6.2–3, P7's #755 leg post-gate), never exceeding two
   concurrent.

**Dispatch mechanics**: one agent session per prompt, in order (N.x.1
→ N.x.2 → N.x.3); never two agents share a worktree; worktrees at
`../taskboard-WT-<flat-suffix>`, branched off the then-current `main`
with `git worktree add -b <branch> ../taskboard-WT-<flat-suffix>`
(never without `-b`, never on `main`); landed branch ⇒ resume on
`<branch>-v2`; `scripts/worktree-doctor.sh` first, every agent,
nonzero (disk or MemAvailable floor) = STOP.

**Shared execution model** (binding for every chunk, every stage):

**Long-command discipline** *(binding — `docs/agent_dispatch.org`;
PR #852 + the #879/#894 flock-deferral rule)*: a subagent is killed
after ~600s without observable activity, so long silent commands kill
working agents.

1. *The coverage trio never runs inside a subagent*: a stage-2/stage-3
   agent's gate is the pre-commit hooks (automatic) plus
   `scripts/run-quality-gates.sh cargo nextest run --workspace`
   (flock-safe, ~4–6 min; background + poll with useful work between
   polls if needed). **Flock-deferral rule: when the gate lock is
   contended and no other useful work remains, never block — commit
   all landed work, report "gate deferred: lock held by <holder>",
   and end the stage; the dispatcher runs the gate.** Expected-heavy
   single builds (~>1 min: clippy, release builds) route through
   `scripts/memory-gate.sh <command>` (the #906/#931 law). The
   dispatcher runs the full AGENTS §10 suite in its own session
   between stage 2's landing and stage 3's ship; CI's Code Coverage
   check runs it authoritatively on the PR.
2. *Stage 3's scope ends at PR-open*: the stage-3 agent rebases,
   reviews, fixes, pushes, opens the PR, and reports — it does NOT
   watch CI. The dispatcher owns `gh pr checks --watch`, the
   `mergeStateStatus CLEAN` verification, teardown, and at-merge issue
   closures (§11).
3. *Any command a subagent expects to exceed ~5 minutes runs
   backgrounded* with short interleaved polls and useful work between
   polls — and the agent commits all landed work BEFORE any wait.
4. *Stage prompts restate this discipline verbatim* (every prompt
   below carries the [D] block).
5. *Dispatcher stage-exit verification*: before dispatching stage N+1,
   the dispatcher verifies stage N's landed state in the worktree
   (`git log origin/main..HEAD` — commits present, tree clean, gate
   evidence re-runnable). A stalled or quota-cut stage is completed
   from its landed commits or resumed with a state-check message —
   **resume over redo**, never re-dispatched blind. Failure playbook:
   `docs/agent_dispatch.org` §8.

0. **Every stage, every agent**: out-of-scope findings become
   `backlog:` GitHub issues (or comments on existing ones) the moment
   they are discovered, without asking (AGENTS §7 standing
   authorization). "No push / no PR" governs git refs only — never
   issues.

1. **Stage 1 — planning agent**: worktree, TDD-first chunk plan at
   `.artifacts/plans/<today>-<topic>-plan.md` (P4/P6 additionally the
   UX journey plan), every open question resolved with a recorded
   decision, committed (`docs(plan): …`). No implementation, no push.
   This plan's per-chunk specs are normative; deviations need a
   recorded reason.
2. **Stage 2 — implementation/research agent**: continues in the
   worktree (recreate on the branch if missing; landed branch ⇒
   `<branch>-v2`), executes per the approved plan — TDD where code is
   involved, every commit standalone (rebase-merge hygiene), gate =
   pre-commit hooks + flock-wrapped nextest (defer per the rule). No
   push, no PR. Reports deviations.
3. **Stage 3 — review agent**: begins by rebasing onto `main` (fetch
   --prune, rebase onto `origin/main`, drop already-merged duplicates,
   resolve only real conflicts, push `--force-with-lease` if history
   rewritten), reviews the branch diff versus the post-rebase base
   against the chunk's acceptance criteria + AGENTS rules + the
   applicable mandate, re-runs the flock-wrapped nextest gate (defer
   per the rule), implements fixes on the branch (amend/reword, never
   duplicate-subject fixups). Last tasks: push, open the PR to `main`
   with `gh`, report the URL + per-acceptance verdicts. **No CI watch.**

**Code-quality mandate** (stage 3, code chunks — verbatim): beyond
functional review, evaluate (a) code quality: naming, comment
discipline (comments state constraints, not narration), idiomatic use
of the approved stack, no dead code or speculative generality, test
quality per AGENTS §5; (b) the Single Responsibility Principle: the
crate-role boundaries of AGENTS §2 are the SRP line — a boundary
violation is an SRP violation; (c) a single level of abstraction per
function body: a function either orchestrates named operations or
works in primitive data, never mixes. Findings are fixed on the
branch like any other finding.

**Evidence & artefact mandate** (stage 3, P9 — verbatim): every
factual claim carries evidence (the command run + observed outcome, or
a verifiable source + what it states) and experiments are reproducible
from the report's method section; recommendations are decision-grade
(options, trade-offs, exactly **one** recommendation, consequences for
later work); repo hygiene (scratch code and captured payloads never
land — evidence is excerpts inside the report; the PR diff is exactly
the chunk's artefacts); scope discipline (out-of-scope ideas go to the
backlog per its template, not into the report).

**UX completeness mandate** (stage 3, P4/P6 — verbatim): re-run
`scripts/ux-coverage.sh <journey-plan> --gate` (new-format step tokens
are REQUIRED rows; MISSING on a claimed row is not done); execute the
walkthrough protocol (`docs/ux_planning.org`) over every P1 story —
the four walkthrough questions per step with evidence, then the
six-heuristic pass over the touched surface; BLOCKER/MAJOR findings
are fixed on the branch, MINOR findings become backlog issues.
Reports a verdict.

## 2. Chunk P1 — #846 golden solo-fail (`test/golden-solo-render`) — COMPRESSED

- **Covers** (normative): the observed shape — the test fails when the
  binary is filtered to it alone and passes in the full binary; the
  regenerated PNG itself compared differently, and the committed
  golden matches the suite-configuration render. Diagnosis: renderer/
  window/probe state that depends on test count/ordering, not content.
- **Round-level decision**: the fix makes the golden capture
  **deterministic with respect to test selection** — the probe's
  render configuration (window size, scale, font stack warm-up, probe
  ordering effects) is initialized identically whether one test or 24
  run. The committed golden stays authoritative; no regen lands unless
  the diagnosis proves the committed artifact encodes suite-only state
  (then the regen + the determinism fix land together, both evidence-
  carried). This is a test-rig fix: no widget/product changes.
- **Runs compressed** (`docs/agent_dispatch.org` §7): small diagnosis
  with a bounded fix — the dispatcher is planner and implementer
  (chunk plan `.artifacts/plans/2026-11-18-golden-solo-render-plan.md`
  committed first, stating the compression), with the diagnosis method
  recorded before the fix. Timing note: r9's W-chunks will churn
  widget surfaces — the diagnosis lands now; if the fix must touch a
  golden PNG that a W-chunk is concurrently regenerating, the
  dispatcher defers the regen half to post-W1 and lands the
  determinism fix alone.
- **Acceptance**: the solo run passes deterministically (evidence:
  solo + full runs both green, repeated); the diagnosis (root cause +
  why suite-order masked it) is recorded in the chunk plan/report; no
  product-code change; #846 closes on the evidence or carries the
  armed diagnosis if the root cause resists the compressed pass
  (escalate to full protocol then — honest not-fixed-yet beats a
  speculative fix).

## 3. Chunk P2 — #893 Today due boundary (`fix/today-due-boundary`)

- **Covers** (normative): `router.rs:1329`'s `TaskDueToday` arm writes
  `duedate = day_window.end` — the window's **exclusive** end — so the
  task instantly drops out of every today section
  (membership is `[now, end)`). **Round-level decisions**:
  1. The write aligns with the **N-4 landed law** (r8 chunk 3): a
     bare-day due is the day's last covered instant per the configured
     day end (the same helper the due editor's bare-day commit path
     uses — stage-1 locates it; if the editor and the router use
     different derivations, that divergence is the deeper finding and
     unifying them is in scope). With no special day end: 23:59:59.999.
  2. The membership test stays `[now, end)` — the fix is on the write
     side; a membership change would ripple the day-boundary law
     everywhere and is out of scope.
  3. `us6_s2_today_row_menu_and_preset_key_recommit`'s wrong-task
     premise (it passes only via a second unrelated overdue row) is
     re-anchored to assert on the re-committed row itself — an
     assertion strengthening, never a weakening; the second row stays
     as the control.
  4. Journey/instrumented repro from the issue (the `[0,0,0,0]` after
     re-commit) becomes the regression test.
- **Disjointness gate**: stage-1 runs the verdict against the in-flight
  r9 branches (K2 `feat/ui-directional-focus`, D2
  `feat/ui-day-review-journey` — Today/adapter surfaces) and the
  landed r8 chunk-3 day-boundary module; on symbol-level overlap with
  an unmerged r9 branch, record "defer to post-r9" and STOP planning
  the implementation (the dispatcher re-queues the chunk).
- **Acceptance**: the instrumented repro passes (re-commit keeps the
  task in Today; due phrase renders as today); the write and the
  editor's bare-day commit use the same derivation (or the divergence
  is recorded with a unification note); us6_s2 strengthened, not
  weakened; the whole today-corpus green with no unexplained snapshot
  churn; diffs confined to the router arm + the shared derivation +
  tests; a dated census note rides the fix if the due-today cell
  semantics change (recorded by stage-1's read of the census).

**Prompt P2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P2 of the tier-2 round ("Today due boundary", branch fix/today-due-boundary) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — never due-phrase text assertions; (b) single-read capture or frozen render clock for clock-derived tests (AGENTS §5 clock discipline); (c) never weaken a correct test — us6_s2 is strengthened (re-anchored to the re-committed row), never loosened; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = below a resource floor: STOP and report), then git worktree add -b fix/today-due-boundary ../taskboard-WT-fix-today-due-boundary off the then-current main (never without -b, never on main — AGENTS §8).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§3, issue #893 (the instrumented repro + the us6_s2 wrong-task analysis), and the landed N-4 law (.artifacts/plans/2026-11-17-ui-r8-task-detail-editor-plan.md — the bare-day due derivation: configured day end, else 23:59:59.999).
2. FIRST the disjointness verdict: list the in-flight r9 branches (git branch -r / the round-9 dispatch plan's seeds — K2 feat/ui-directional-focus, D2 feat/ui-day-review-journey) and check symbol-level overlap with your planned diff sites (router.rs TaskDueToday arm ~1329, the today membership test site, the due editor's bare-day commit helper). Overlap with an unmerged branch ⇒ record "defer to post-r9" and STOP (report; do not plan further).
3. Read the seams: the TaskDueToday arm, the today section membership test (the adapter's [now, end) check), the due editor's bare-day commit path (the N-4 helper), days::next_day_window, us6_s2 in crates/taskboard-ui-slint/tests/journeys.rs, and the today snapshot corpus.
4. Decide per the dispatch plan's recorded decisions: the unified bare-day derivation (which helper, where it lives — domain vs the adapter's day vocabulary; a helper duplicated in two layers is the #884 smell — record the owner), the re-anchored us6_s2 assertion shape, the regression test from the instrumented repro, and the census-note text if the due-today cell semantics change.
5. Write the chunk plan at .artifacts/plans/2026-11-18-today-due-boundary-plan.md. Resolve every open question with a recorded decision.
6. Commit as: docs(plan): the today due boundary chunk plan. Do not implement; do not push.
7. Backlog issues per [L](d); list the numbers.

Report: the disjointness verdict, plan path, decisions, risks, estimated diff size.
```

**Prompt P2.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P2 of the tier-2 round (branch fix/today-due-boundary).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) frozen render clock / single-read capture in clock-derived tests; (c) strengthen, never weaken — us6_s2's re-anchor asserts the re-committed row and keeps the control row; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P2 (normative), the dispatch plan §1/§3, and the stage-1 plan .artifacts/plans/2026-11-18-today-due-boundary-plan.md in ../taskboard-WT-fix-today-due-boundary (recreate on the branch if missing; landed branch ⇒ fix/today-due-boundary-v2).
2. Implement exactly per the plan, TDD: the instrumented-repro regression test red under current behavior, the unified bare-day write per the recorded derivation, green; the us6_s2 re-anchor. Diff confined to the recorded sites; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence, snapshot-corpus delta, per-gate results, deviations.
```

**Prompt P2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P2 of the tier-2 round (branch fix/today-due-boundary).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) the exclusive-end membership convention is untouched — a diff hunk changing the membership test is a finding unless the plan recorded it; every changed snapshot traces to the plan; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P2 (normative), the dispatch plan §1/§3 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-today-due-boundary.
2. Functional focus — this chunk's traps: (a) the write uses the SAME derivation as the editor's bare-day commit — a second, near-identical derivation in the router is the exact #884-class drift this fix must not create; (b) the boundary instant itself (day end configured specially vs default) is pinned by a test on both arms of the N-4 law; (c) TaskScheduleToday (~router.rs:1346) shares the arm shape — check whether it has the same exclusive-end defect; an unexamined twin is a finding (fix in-scope if same-class, else a backlog issue); (d) the whole today corpus is green with per-plan diagnosis for any expectation change.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #893 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 4. Chunk P3 — CLI verb & grammar hygiene (`fix/cli-verb-grammar-hygiene`)

- **Covers** (normative), two items:
  - **#874**: collapse `BoardsCmd::Move`'s dead fenced `--to-deck` arm
    (`ReverseFenced`, commands/mod.rs ~1073) and the landed
    `--to-connection` twin into one verb grammar (`--to-files` /
    `--to-connection` / `--undo`), removing `ReverseFenced` and the
    dual-flag parse. Precondition verified on `main` (the twin
    landed). Receipt/output helpers update with the parse; the
    command-family conventions hold.
  - **#873**: the date-entry grammar's non-padded leniency —
    **round-level decision: normalize + document**. The parse accepts
    the two documented absolute forms with non-padded day/month fields
    and canonicalizes to padded (stored values stay canonical — the
    quick-list filter's digit-prefix assumption holds); the grammar
    doc-comment records the leniency as deliberate (the A1
    documented-leniency rule). Refusal was rejected: it breaks
    tolerated input and buys nothing canonicalization doesn't.
    Stage-1 may overturn to refuse with a recorded reason if the
    grammar tests reveal a conflict.
- **Acceptance**: #874 — `ReverseFenced` gone, the collapsed grammar
  pinned by typed parse tests (each target kind, the undo arm, the
  fenced refusal's replacement semantics per the landed ceremony),
  help/output updated, the old dual-flag invocation now a typed
  ambiguous-flags error (or the recorded shape); #873 — non-padded
  inputs canonicalize (unit matrix: `2030-01-3`, `2030-1-03`,
  `2030-1-3` → padded; already-padded unchanged; the grammar doc
  updated); sync-direction tests' recorded comment updated to point at
  the decision; diffs confined to the CLI command surface + the
  due_edit grammar + tests.

**Prompt P3.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P3 of the tier-2 round ("CLI verb & grammar hygiene", branch fix/cli-verb-grammar-hygiene) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — never help-text assertions; (c) never weaken a correct test — the grammar tests' documented leniency comment is superseded by the recorded decision, not deleted; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b fix/cli-verb-grammar-hygiene ../taskboard-WT-fix-cli-verb-grammar-hygiene off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§4, issues #874 #873, and the landed context: the r8 files-sync-status plan's D-11 CLI twin (the --to-connection grammar), the rebind ceremony's StateCommand::RebindBoard, and the r2 D25 grammar doc (the two absolute forms).
2. Read the seams: BoardsCmd::Move's parse + the ReverseFenced fence (crates/taskboard-app/src/commands/mod.rs ~1073, boards.rs), the --to-connection twin's parse/receipt (boards.rs ~1383, output.rs ~1027), the due_edit.rs grammar tests (the recorded leniency comment on sync_directions_follow_the_pinned_rules), the quick-list filter's digit-prefix conventions.
3. Decide: the collapsed --to-files/--to-connection/--undo grammar (what happens to an invocation carrying both targets — the typed error shape), the fence's replacement semantics (the ceremony's own typed refusals now cover the reverse direction — verify by reading the landed ceremony), the #873 normalization function's shape and its doc-comment text, and the test matrix for both.
4. Write the chunk plan at .artifacts/plans/2026-11-18-cli-verb-grammar-hygiene-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the cli verb grammar hygiene chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, risks, estimated diff size.
```

**Prompt P3.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P3 of the tier-2 round (branch fix/cli-verb-grammar-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (c) the normalization matrix is exhaustive over the documented forms' padded and non-padded variants — a case the matrix misses is a finding you must not create; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P3 (normative), the dispatch plan §1/§4, and the stage-1 plan .artifacts/plans/2026-11-18-cli-verb-grammar-hygiene-plan.md in ../taskboard-WT-fix-cli-verb-grammar-hygiene (recreate on the branch if missing; landed branch ⇒ fix/cli-verb-grammar-hygiene-v2).
2. Implement exactly per the plan: #874's grammar collapse (ReverseFenced removed, typed parse tests per target kind + undo + the dual-target error), #873's normalization + doc-comment + matrix. Diff confined to the CLI command surface + due_edit grammar + tests; every commit standalone; SPDX per crate rules.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the matrix evidence, per-gate results, deviations.
```

**Prompt P3.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P3 of the tier-2 round (branch fix/cli-verb-grammar-hygiene).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) the removed fence must not open a hole — trace the reverse-direction case end-to-end (the ceremony's own refusal now covers it; construct the case and check the typed outcome); the grammar doc's two absolute forms remain the documented surface with the leniency now explicit; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P3 (normative), the dispatch plan §1/§4 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-fix-cli-verb-grammar-hygiene.
2. Functional focus — this chunk's traps: (a) no orphaned helpers — ReverseFenced's removal leaves no dead receipt/output arms (grep); (b) the REPL/completion surfaces that mention the move flags stay consistent (a stale completion suggestion for --to-deck is a finding); (c) the normalization happens at the parse boundary, not at every consumer — a per-consumer re-check is a finding; (d) already-padded input is byte-identical through the parse (no double-normalization artifacts).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #874 #873 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 5. Chunk P4 — #733 notification Unmute (`feat/ui-notification-unmute`) — journey-gated

- **Covers** (normative): `NotificationRowModel` gains `muted: bool`
  (derived where the model is built — the row knows its task anchor);
  the Mute button renders as **Unmute** when true and dispatches
  `SetTaskRemindersMuted { muted: false }`. Perceptual law: never a
  silent no-op button (the R6-2 class defect).
- **Journey gate**: stage 1 writes the UX journey plan
  (`.artifacts/plans/2026-11-18-notification-unmute-ux-plan.md`, from
  the template) + the chunk plan; the dispatcher pauses for operator
  acceptance; stages 2–3 proceed on the accepted journey. Small chunk —
  the journey plan is expected to be lean (single surface, one story).
- **Acceptance**: the accepted journey's wiring rows implemented;
  muted-state truth flows from engine state (never a UI-side guess);
  typed dispatch verified for both arms; the can-mute gate's
  interaction with the muted state is pinned by a journey test
  (step-ID tokens); `scripts/ux-coverage.sh <plan> --gate` green;
  diffs confined to the notification surface + the model build site.

*(Prompts P4.1–P4.3 follow the C10 pattern from the backlog-reduction
dispatch plan, adapted: branch `feat/ui-notification-unmute`, worktree
`../taskboard-WT-feat-ui-notification-unmute`, journey plan
`2026-11-18-notification-unmute-ux-plan.md`, chunk plan
`2026-11-18-notification-unmute-plan.md`, origin section §5, issue
#733; seams: `ui/compositions/notification-surface.slint` (the
can-mute gate, the Mute button), the NotificationRowModel build site,
`SetTaskRemindersMuted`'s dispatch path; stage 1 reports GATE-PAUSED;
stage 2 opens with "GATE CLEARED"; stage 3 carries the code-quality +
UX completeness mandates and references #733.)*

## 6. Chunk P6 — #909 + #910 config reload & batch (`feat/config-reload-batch`) — journey-gated

- **Covers** (normative), two stories under one journey plan (siblings
  of the accepted #845 config journey — its rulings R-3/R-4 scoped
  them out; this round scopes them in):
  - **#909 live-reload watcher**: watch `taskboard.toml` for external
    edits (editor rename storms, partial writes → debounce), reload
    through the same overlay-merge path the C10 write-through lane
    uses. **Dependency decision (operator gate, recorded in P6.1's
    plan)**: the file-watcher crate (`notify`) is not currently a
    workspace dependency — adding it is an AGENTS §4 stack-table
    entry. The stage-1 plan records the addition with rationale
    (alternatives: polling stat checks — recorded and rejected or
    accepted per evidence); the dispatcher confirms the stack
    addition with the operator at the journey gate (one gate, both
    decisions).
  - **#910 batch config set**: multiple key/value pairs in one
    invocation, one atomic write (one intent = one proven document —
    the overlay invariant holds; batch is ergonomics, not a new
    propagation semantic).
- **Journey gate**: stage 1 writes `.artifacts/plans/2026-11-18-config-reload-batch-ux-plan.md`
  (two stories) + the chunk plan; dispatcher pauses (journey + stack
  addition); stages 2–3 proceed on acceptance.
- **Acceptance**: the accepted journeys' wiring rows implemented;
  #909 — rename-storm and partial-write scenarios pinned by
  deterministic tests (debounce on pause/advance or an injected timer
  seam — never sleeps), external-edit reload converges to the same
  state the C10 lane produces; #910 — one invocation = one atomic
  document (pinned: a failing mid-batch key rolls back the whole
  write, typed error names the offending key — never a partial
  document); hostile-env pass green (the C6/#865 patterns); diffs
  confined to the config command surface + the watcher module + tests.

*(Prompts P6.1–P6.3 follow the C10 pattern, adapted: branch
`feat/config-reload-batch`, worktree
`../taskboard-WT-feat-config-reload-batch`, journey plan
`2026-11-18-config-reload-batch-ux-plan.md`, chunk plan
`2026-11-18-config-reload-batch-plan.md`, origin section §6, issues
#909 #910; stage 1 additionally records the `notify` stack addition +
the watcher design (debounce window, rename-storm handling, the
reload path's reuse of the C10 overlay merge) and reports GATE-PAUSED
with the two gate decisions; stage 2 opens "GATE CLEARED"; stage 3
carries both mandates and references #909 #910.)*

## 7. Chunk P5 — #751 ceremony identity injection (`feat/ceremony-identity-injection`)

- **Covers** (normative): the move ceremony's `app_world`
  (`crates/taskboard-app/src/board_move_ceremony.rs`) loads the device
  identity from the hard-coded global path; take the `device.json`
  path (or a resolved `DeviceIdentity`) as a parameter of
  `run_on`/`app_world` — CLI verb passes the resolved default, the UI
  arm passes the workspace's, tests pass a tempdir (mirroring the
  round-6 chunk-5 `EngineHandle` re-key, D-3). This unlocks the two
  recorded fallback tests: `move_commits_a_deck_board_onto_files` and
  `move_reports_a_queued_outbox_typed` (hermetic, past `app_world`).
- **Acceptance**: the parameter is injected through the whole
  `app_world` path (no ambient global read left on the ceremony path —
  grep-verified); the two fallback tests land hermetic (tempdir
  identity, no `XDG_DATA_HOME` mutation, no cross-test race); the CLI
  and UI arms behavior-identical (existing ceremony corpus green,
  snapshot-delta per plan); diffs confined to the ceremony module +
  its call sites + the new tests.

**Prompt P5.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P5 of the tier-2 round ("ceremony identity injection", branch feat/ceremony-identity-injection) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (c) never weaken a correct test — the existing ceremony tests' pre-app_world coverage stays; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/ceremony-identity-injection ../taskboard-WT-feat-ceremony-identity-injection off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§7, issue #751, and the round-6 chunk-5 plan's D-3 re-key pattern (.artifacts/plans/2026-11-12-ui-r6-files-boards-surface-plan.md) plus the r6 files-boards surface plan's recorded fallback tests.
2. Read the seams: board_move_ceremony.rs (app_world's identity load, run_on's signature), the boot gate's use of the same global path (shared or divergent — record), the CLI verb's ceremony invocation, the UI arm's (settings MoveBoardToFiles), the existing move tests and the convergence tests' FakeFilesTransport shape, and the r9 files-sync-status chunk's landed ceremony state (the twin ceremony shares app_world? — record).
3. Decide: the injection shape (path parameter vs resolved DeviceIdentity — record which and why; the resolved form is preferred if construction is cheap and pure, the path form if the ceremony owns loading semantics), the call-site updates (CLI/UI/tests), the two fallback tests' hermetic shape (tempdir identity fixture, the convergence harness reuse), and whether the boot gate's global read stays ambient (out of scope unless the same parameterization is trivial — else a backlog note).
4. Write the chunk plan at .artifacts/plans/2026-11-18-ceremony-identity-injection-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the ceremony identity injection chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the injection decision, risks, estimated diff size.
```

**Prompt P5.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P5 of the tier-2 round (branch feat/ceremony-identity-injection).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (c) the ceremony's behavior is identical for the default-path callers — the injection is a seam, not a semantic change; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P5 (normative), the dispatch plan §1/§7, and the stage-1 plan .artifacts/plans/2026-11-18-ceremony-identity-injection-plan.md in ../taskboard-WT-feat-ceremony-identity-injection (recreate on the branch if missing; landed branch ⇒ feat/ceremony-identity-injection-v2).
2. Implement exactly per the plan: the injection through run_on/app_world, the call-site updates (CLI resolved default, UI workspace identity, tests tempdir), the two fallback tests hermetic. TDD where the new tests can fail pre-injection (they cannot run at all before — record the before-shape as the plan's evidence instead). Diff confined per the plan; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-item outcome, the hermetic-run evidence, per-gate results, deviations.
```

**Prompt P5.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P5 of the tier-2 round (branch feat/ceremony-identity-injection).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) no ambient global read remains on the ceremony path (grep device.json / data_dir in the ceremony module — a leftover is a finding); the two new tests run hermetic under parallel execution (no XDG mutation, no shared tempdir); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P5 (normative), the dispatch plan §1/§7 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-ceremony-identity-injection.
2. Functional focus — this chunk's traps: (a) the twin ceremony (r8's --to-connection) shares app_world — its call site must be updated consistently (an inconsistent twin is a finding); (b) the fallback tests actually exercise past-app_world behavior (assert the outbox quiescence and the commit — not just construction); (c) the UI arm's identity source (workspace's) matches what production resolves — a test-only identity leaking into a production call site is a finding.
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #751 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 8. Chunk P7 — #718 → #755 files GC (`feat/files-gc-verb`)

- **Covers** (normative), two legs:
  - **Leg 1 (#718)**: the `delete(&self, path)` verb on
    `FilesTransport` (client impl + fake + the docker/live tier's
    404-on-missing pin), and the owner's post-snapshot GC pass in
    `board_cycle::snapshot_duty` walking `gc_eligible`. The design law
    stands (boards-over-files design plan §3 Part D, D-BF7): tombstones
    never GC'd; only chunks `≤` a **published** snapshot's watermark;
    owner-scoped.
  - **Leg 2 (#755)**: the unbind leftover (Stop Sync leaves the remote
    `boards/<uuid>/` subtree). The choice — GC-on-unbind vs
    retirement-marker-then-lazy-GC — **belongs to the operator**:
    stage-1 records both options with a recommendation and the
    dispatcher pauses for the operator's decision (a lighter gate than
    the journey gates — one decision, both options specified by the
    issue). Stage 2 implements the chosen option on top of leg 1.
- **Acceptance**: leg 1 — the DELETE verb pinned at all three tier
  fakes (client, fake, docker/live 404 pin); the GC pass deletes only
  eligible chunks (tombstone preservation, watermark bound, owner
  scope — each pinned by a test); the non-GC corpus unchanged. Leg 2 —
  the chosen option implemented with its recorded rationale; the
  subtree no longer lingers per the chosen semantics (or the marker
  path's discovery prune is pinned); discovery fold tests updated per
  the chosen behavior; diffs confined to the sync-files crate (+ the
  engine unbind command if leg 2's choice requires the marker).

**Prompt P7.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P7 of the tier-2 round ("files GC verb", branch feat/files-gc-verb) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only — the DELETE verb's outcomes are typed (deleted / already-absent / transport error), never logged-string checks; (c) the D-BF7 law is binding — tombstones never GC'd, published-watermark bound, owner scope; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/files-gc-verb ../taskboard-WT-feat-files-gc-verb off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§8, issues #718 #755, and the design base: .artifacts/plans/2026-11-07-boards-over-files-design-round-plan.md §3 Part D (D-BF7) + the chunk-5 module doc deferral in crates/taskboard-sync-files-nextcloud/src/board_cycle.rs.
2. Read the seams: FilesTransport's verb set (client impl, fake, the docker/live tier harnesses), board_gc::gc_eligible, snapshot_duty's cycle (where the post-snapshot GC pass attaches), the per-device watermarks + the published snapshot's seq floor, StateCommand::UnbindBoard's path (leg 2's insertion point), and the discovery fold-first logic (board_cycle.rs ~208-212).
3. Decide: leg 1's verb signature + tier pins + the GC pass's placement and ordering within snapshot_duty (after publish, before/after watermark advance — record); leg 2's two options fully specified with a recommendation (GC-on-unbind: immediate subtree delete at unbind, the failure semantics if the transport errors; marker-then-lazy-GC: the write_move_marker analog, discovery's prune rule, the lazy pass's trigger) — the dispatcher takes BOTH to the operator.
4. Write the chunk plan at .artifacts/plans/2026-11-18-files-gc-verb-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the files gc verb chunk plan. Do not implement; do not push. Report that leg 2 is GATE-PAUSED on the operator's option choice.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, the leg-1 design, both leg-2 options + recommendation, risks, estimated diff size.
```

**Prompt P7.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P7 of the tier-2 round (branch feat/files-gc-verb). GATE CLEARED for leg 2: the operator chose <recorded option> — implement it exactly as specified in the stage-1 plan.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes; (b) no real-time waits in the GC pass tests — deterministic sequencing; (c) the D-BF7 law holds — a GC test that deletes a tombstone or an unpublished chunk must fail by construction; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P7 (normative), the dispatch plan §1/§8, and the stage-1 plan .artifacts/plans/2026-11-18-files-gc-verb-plan.md in ../taskboard-WT-feat-files-gc-verb (recreate on the branch if missing; landed branch ⇒ feat/files-gc-verb-v2).
2. Implement exactly per the plan: leg 1 (verb + three tier pins + the snapshot_duty GC pass with the eligibility tests), then leg 2 per the operator's recorded choice. TDD; diff confined to the sync-files crate (+ the engine unbind command if the choice requires it); every commit standalone; SPDX per crate rules.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: per-leg outcome, the eligibility-test evidence, per-gate results, deviations.
```

**Prompt P7.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P7 of the tier-2 round (branch feat/files-gc-verb).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes; (c) the D-BF7 law audited line-by-line on the GC pass: tombstone preservation, the published-watermark bound, owner scoping — any eligibility shortcut is a shipping blocker (this code deletes remote data); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P7 (normative), the dispatch plan §1/§8 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-files-gc-verb.
2. Functional focus — this chunk's traps: (a) the GC pass runs AFTER publish — an ordering where a crash between delete and watermark-advance can re-serve deleted chunks (or vice versa) must be analyzed and pinned by the plan's recorded ordering; (b) leg 2's chosen semantics handle transport failure (an unreachable server at unbind must not strand the state — the recorded failure story is implemented, not just designed); (c) the discovery fold behaves per the chosen option (foreign-subtree entries pruned or GC'd — the fold-first adoption logic at board_cycle.rs ~208 is not silently changed); (d) the fake transport's delete matches the client's semantics exactly (a fake that never 404s hides the pin).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #718 #755 — closures are the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 9. Chunk P8 — #673 files-sync live cadence (`feat/files-sync-live-cadence`)

- **Covers** (normative): thread a settings reader into the files-sync
  gate (the actor has no engine facet — DR-8's boundary stays: read,
  don't join the connector family) and re-arm the cadence on
  `SystemEvent::SettingsChanged` (the connector actor's min rule via
  `reschedule`). `poll_secs` changes apply live; the boot fallback
  (the `[sync]` triple) keeps its semantics.
- **Acceptance**: a cadence change is observed live (pause/advance or
  injected clock: change `poll_secs`, the next wake honors it — both
  shorten and lengthen); the doorbell's ignore-arm no-op is replaced
  by the re-arm (the documented no-op comment updated/superseded);
  DR-8's boundary note intact (a reader facet, not connector
  membership); diffs confined to the files-sync actor + its gate +
  tests.

**Prompt P8.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P8 of the tier-2 round ("files-sync live cadence", branch feat/files-sync-live-cadence) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe, ~4–6 min; background + poll beyond ~5 minutes; commit all landed work BEFORE any wait). When the gate lock is contended and no other useful work remains, never block: commit, report "gate deferred: lock held by <holder>", end the stage.
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock — never sleeps; (c) never weaken a correct test; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b feat/files-sync-live-cadence ../taskboard-WT-feat-files-sync-live-cadence off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§10, issue #673, and the round-5 live-settings plan's D-5.5 (.artifacts/plans/2026-11-06-app-live-settings-plan.md — the doorbell + reschedule pattern) — P7 (feat/files-gc-verb) must have landed or be file-disjoint per its stage-1 plan: verify before proceeding (gh pr list / the branch state); if P7 is unmerged and overlapping, sequence after it and say so.
2. Read the seams: the files-sync actor's cadence gate (poll_secs read at boot, the [sync]-triple fallback), the connector actor's live-cadence implementation (the min rule + reschedule — the pattern to mirror), SyncStateReader::read_settings, the SystemEvent::SettingsChanged broadcast path, and the files-sync actor's ignore arm (the documented no-op).
3. Decide: the reader-facet shape (what the actor receives — a settings reader handle per the connector pattern), the re-arm rule (the connector min rule semantics adapted), the clock seam for tests (the actor's existing clock or an injected one per the #868/#133 precedents), and the test matrix (shorten/lengthen/doorbell-during-cycle).
4. Write the chunk plan at .artifacts/plans/2026-11-18-files-sync-live-cadence-plan.md. Resolve every open question with a recorded decision.
5. Commit as: docs(plan): the files sync live cadence chunk plan. Do not implement; do not push.
6. Backlog issues per [L](d); list the numbers.

Report: plan path, decisions, the P7 sequencing verdict, risks, estimated diff size.
```

**Prompt P8.2 (implementation):**

```text
You are the stage-2 IMPLEMENTATION agent for chunk P8 of the tier-2 round (branch feat/files-sync-live-cadence).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll beyond ~5 min; commit all landed work BEFORE any wait; defer per the rule if contended).
[L] Round laws (binding): (a) typed outcomes only; (b) pause/advance or injected clock — the live-rearm tests are deterministic; (c) never weaken a correct test; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P8 (normative), the dispatch plan §1/§10, and the stage-1 plan .artifacts/plans/2026-11-18-files-sync-live-cadence-plan.md in ../taskboard-WT-feat-files-sync-live-cadence (recreate on the branch if missing; landed branch ⇒ feat/files-sync-live-cadence-v2).
2. Implement exactly per the plan, TDD: the reader facet, the doorbell re-arm, the test matrix (shorten/lengthen/during-cycle). Diff confined to the files-sync actor + gate + tests; every commit standalone.
3. Gate: pre-commit hooks + scripts/run-quality-gates.sh cargo nextest run --workspace (background + poll; defer per the rule if contended). The full §10 suite is the dispatcher's.
4. No push, no PR. Never push to or merge main.
5. Backlog issues per [L](d); list the numbers.

Report: red→green evidence, per-gate results, deviations.
```

**Prompt P8.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P8 of the tier-2 round (branch feat/files-sync-live-cadence).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): your gate is the pre-commit hooks plus scripts/run-quality-gates.sh cargo nextest run --workspace (flock-safe; background + poll; defer per the rule if contended). Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns gh pr checks --watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) typed outcomes only; (c) DR-8's boundary intact — the actor grew a reader facet, not connector membership (grep for any connector-family join); the doorbell cannot lose an event between the ignore-arm removal and the re-arm (the subscribe path is analyzed); (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P8 (normative), the dispatch plan §1/§10 (the code-quality mandate applies in full), the stage-1 plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-feat-files-sync-live-cadence.
2. Functional focus — this chunk's traps: (a) a settings change during an in-flight cycle is honored at the NEXT wake, not mid-cycle (the min rule's semantics — pinned); (b) an invalid poll_secs from a live edit falls back typed (the boot fallback's validation now guards the live path too — or the recorded equivalent); (c) the re-arm does not double-schedule (one timer, re-armed — a wake-storm under rapid settings edits is a finding).
3. Code-quality mandate (verbatim from §1): naming, comment discipline, idiomatic stack, no dead code, test quality per AGENTS §5; crate boundaries are the SRP line; one abstraction level per body. Fixed on the branch.
4. Re-run scripts/run-quality-gates.sh cargo nextest run --workspace yourself (defer per the rule if contended); any failure is a finding.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #673 — closure is the dispatcher's at merge), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 10. Chunk P9 — #890 build-cache evaluation (`docs/build-cache-evaluation`) — research

- **Covers** (normative): the per-worktree `target/` cost (36–55G each;
  the OOM incidents' peak) vs a shared cache — `sccache` or a shared
  `CARGO_TARGET_DIR` scheme. Deliverable: evidence report + ruling
  recommendation, **operator-gated** (like N-F/#891 and the #708
  pattern). Zero production/build-config code — a rollout, if
  recommended, is its own follow-up chunk after the gate.
- **Acceptance**: the evaluation covers the recorded risk axes —
  cache poisoning across branches (stale incremental state, feature
  flag divergence), concurrent-access semantics (two worktrees
  building simultaneously against one shared dir/sccache server),
  CI parity (CI stays isolated or adopts the same cache — recorded),
  and the measured local evidence (a controlled A/B: cold build time
  and peak memory, isolated vs shared — reproducible method); exactly
  one recommendation with consequences and the rollout shape; the
  report names the gate handoff.

**Prompt P9.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk P9 of the tier-2 round ("build-cache evaluation", branch docs/build-cache-evaluation) in /home/viktoria/src/taskboard.

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): benchmark builds run through scripts/memory-gate.sh and BACKGROUND with short interleaved polls and useful work between polls; commit landed work BEFORE any wait; defer gate runs per the rule if contended.
[L] Round laws (binding): (a) evidence-carrying claims — every timing/memory number carries the command + environment; (c) options + trade-offs + exactly one recommendation; (d) out-of-scope findings become `backlog:` issues immediately; report the numbers.
[R] Run scripts/worktree-doctor.sh from /home/viktoria/src/taskboard first (nonzero = STOP and report), then git worktree add -b docs/build-cache-evaluation ../taskboard-WT-docs-build-cache-evaluation off the then-current main (never without -b, never on main).

1. Read AGENTS.md, the recommendation report .artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md (§Tier-2, normative), the dispatch plan .artifacts/plans/2026-11-18-tier2-dispatch-plan.md §1/§10, issue #890, and the incident base: .artifacts/reports/2026-11-17-oom-incident-concurrency-learning-report.md (L4) + the worktree disk report (2026-10-18).
2. Write the research chunk plan at .artifacts/plans/2026-11-18-build-cache-evaluation-chunk-plan.md: the A/B method (what is measured, in what worktree configuration, how the box's other load is controlled — the memory-gate law), the candidate schemes (sccache server; shared CARGO_TARGET_DIR per-machine; per-branch target dirs; the hybrid), the risk axes to evidence, and the report/ruling skeletons. Zero build-config code is a hard boundary — the plan states it.
3. Commit as: docs(plan): the build cache evaluation chunk plan. Do not implement; do not push.
4. Backlog issues per [L](d); list the numbers.

Report: plan path, the A/B method, risks.
```

**Prompt P9.2 (research):**

```text
You are the stage-2 RESEARCH agent for chunk P9 of the tier-2 round (branch docs/build-cache-evaluation).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): benchmark builds route through scripts/memory-gate.sh, run BACKGROUND with short interleaved polls and useful work between polls; commit landed work BEFORE any wait; defer gate runs per the rule if contended. (Docs-only chunk: pre-commit hooks are your gate; the dispatcher runs the full §10 suite.)
[L] Round laws (binding): (a) evidence-carrying claims — every number has its command; (c) exactly one recommendation, consequences stated; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P9 (normative), the dispatch plan §1/§10 (the evidence & artefact mandate applies in full), and the stage-1 chunk plan .artifacts/plans/2026-11-18-build-cache-evaluation-chunk-plan.md in ../taskboard-WT-docs-build-cache-evaluation (recreate on the branch if missing; landed branch ⇒ docs/build-cache-evaluation-v2).
2. Execute per the plan: the controlled A/B (cold/warm builds, isolated vs each candidate scheme, peak memory + wall time recorded per run), the risk-axis evidence (concurrent access between two worktrees — actually exercise it; feature-flag divergence; cache poisoning vectors), CI-parity analysis, and the recommendation with the rollout shape (what a rollout chunk would change, what the operator must coordinate).
3. Write the report (.artifacts/reports/2026-11-18-build-cache-evaluation-report.md) + the ruling doc (.artifacts/plans/2026-11-18-build-cache-ruling.md) per the skeletons.
4. Gate: pre-commit hooks (docs-only; the dispatcher runs the full §10 suite).
5. No push, no PR. Never push to or merge main.
6. Backlog issues per [L](d); list the numbers.

Report: the recommendation summary, the A/B numbers, deviations.
```

**Prompt P9.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk P9 of the tier-2 round (branch docs/build-cache-evaluation).

[D] Long-command discipline (binding; docs/agent_dispatch.org; PR #852 + the #879/#894 flock-deferral rule): commit landed work BEFORE any wait; defer gate runs per the rule if contended. Your scope ends at PR-open: push, open the PR with gh, report the URL — the dispatcher owns the CI watch, mergeStateStatus CLEAN, teardown, at-merge closures.
[L] Round laws (binding): (a) spot-check the A/B numbers (re-run one benchmark shard if a claim looks off); (c) exactly one recommendation, alternatives recorded, gate handoff explicit; (d) backlog issues immediately; report the numbers.

1. Read AGENTS.md, the recommendation report §Tier-2 P9 (normative), the dispatch plan §1/§10 (the evidence & artefact mandate verbatim in §1), the stage-1 chunk plan; rebase onto main (fetch --prune; rebase onto origin/main; drop merged duplicates; push --force-with-lease if rewritten); review the branch diff versus the post-rebase base in ../taskboard-WT-docs-build-cache-evaluation.
2. Functional focus — this chunk's traps: (a) the diff is exactly the report + ruling doc (+ the chunk plan) — anything else is a finding; (b) the concurrent-access evidence is actually exercised (two simultaneous builds against the shared scheme), not asserted from documentation; (c) the rollout shape names what stays operator-coordinated (fstab, CI config, per-agent law text); (d) invented performance claims without a command are findings.
3. Evidence & artefact mandate (verbatim, §1 of this dispatch plan).
4. Pre-commit hooks are your gate; the dispatcher runs the full §10 suite.
5. Fix every issue on the branch (amend/reword).
6. Ship: push, open the PR to main with gh (body references #890 — it stays open through the operator gate, mirroring the #708/#891 pattern), report the URL + verdict. Do NOT watch CI.
7. Backlog issues per [L](d); list the numbers.

Report: verdict, findings, fixes, PR URL.
```

## 11. Dispatcher runbook & at-merge closures (binding)

**Runbook**: per `docs/agent_dispatch.org` — stage-exit verification
between stages (resume over redo, playbook §8); the full §10 suite
between stage-2 landing and stage-3 ship of every code chunk (P2, P3,
P4, P5, P6, P7, P8); CI watch + `mergeStateStatus CLEAN` +
absolute-path teardown after each PR-open; **≤2 concurrent chunks
beside r9**, staggered ≥3 min, expected-heavy builds through
`scripts/memory-gate.sh`; P4/P6 pause at their journey gates (P6's
gate also carries the `notify` stack-table addition), P7 pauses at
its leg-2 option choice, P9 at its ruling.

**At-merge closures** (dispatcher-owned): P1 → #846 (or the armed
diagnosis comment); P2 → #893; P3 → #874, #873; P4 → #733; P5 →
#751; P6 → #909, #910; P7 → #718, #755; P8 → #673; P9 → #890 at the
operator's ruling acceptance.

**Deliberately not dispatched here**: the fold-into-r9 set
(#775/#881/#880 — close via r9's chunks), armed flakies (#697/#650 —
recurrence-armed), no-recurrence flakies (#336/#287/#384/#883), #327's
sharded mutants lane, habit backlog #955–#963 (journey plans,
post-H-chain), O-5 remainder #946–#949, #919 (evidence-conditional),
the perf seven, feature-scale items, #930/#904-class operator items
already homed.

**What opens next**: the post-r9 round (undo UI legs #353/#354
journeys, habit backlog #955–#963, the #946–#949 CLI-parity
remainder); P7/P9's gated follow-ups; the rollout chunk if the P9
ruling recommends a shared cache.
