# [Plan] Future Directions Wave 0 — Dispatch Plan & Agent Prompts

- **Date**: 2026-10-14
- **Kind**: dispatch plan — three-stage agent prompts (planning →
  execution → review) for the chunks of **Wave 0** of the accepted
  future-directions roadmap. Wave 0 = the two cheap de-risking spikes
  (D1 Deck round-trip strategy, D6 ADHD design principles) + the
  Phase 7 GUI design round with its session support and the
  derivation of the follow-up implementation dispatch plan. No code
  in this commit.
- **Origin**: `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §6 (Wave 0 row; waves and order approved by the user 2026-10-14),
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md` (the
  design session's input), and the dispatch-plan house pattern of
  `2026-10-12-connect-flow-implementation-dispatch-plan.md` /
  `2026-10-12-reminders-implementation-dispatch-plan.md`.
- **Status**: READY — chunks 1 and 2 are **order-independent** (both
  branch off `main`, disjoint files); dispatch them serially or in
  parallel. Chunk 3 dispatches only after **both** have merged (its
  session consumes both reports). Chunk 4 dispatches only after the
  user has accepted the design plan (explicit human gate — see §4).
  The GUI **implementation** chunks are deliberately **not dispatched
  here**: they are chunk 4's deliverable, derived from the accepted
  design plan (provisional shapes in §5).

## 1. Chunks & ordering

| Chunk | Wave-0 slice | Branch | Covers | Size |
|---|---|---|---|---|
| 1 | D1 keystone spike | `docs/deck-roundtrip-spike` | Deck round-trip strategy investigation (custom fields vs attachment sidecar vs rejected alternatives) → decision-grade report + backlog annotation | M (research) |
| 2 | D6 research spike | `docs/adhd-research-spike` | ADHD-support research (app-space survey + research basis) → design-principles report mapped to roadmap waves + GUI brief | S/M (research) |
| 3 | GUI design round | `docs/gui-phase7-design` | Slint stress-test findings report + Phase 7 design plan **DRAFT** with the user-session decision agenda + pre-session review | M |
| 4 | dispatch derivation | `docs/gui-phase7-dispatch` | Phase 7 **implementation dispatch plan** (chunks + three-stage prompts) authored from the accepted design plan | S (docs) |

**Dependency graph**: chunk 1 ∥ chunk 2 (both off `main`, no shared
files) → chunk 3 (wants both reports merged so the session reads
them) → **user session gate** (design plan Status flips DRAFT →
ACCEPTED with recorded decisions; connect-flow precedent: iterate in
review rounds on the PR) → chunk 4 → the derived implementation
chunks (wave 0's build-out; not this plan's scope).

**Dispatch mechanics**: one agent session per prompt, in order
(N.1 → N.2 → N.3 within a chunk). Chunks 1 and 2 may run
concurrently as separate sessions; never let two agents share one
worktree. Every chunk works in its own worktree
`../taskboard-WT-<branch>` next to the project root (AGENTS §8),
branch off the then-current `main`, no PR ever stacked.

**Shared execution model** (binding for every chunk, adapted from
the connect-flow dispatch plan §1):

1. **Stage 1 — planning agent**: creates the worktree
   `../taskboard-WT-<branch>` on a new branch off the current
   `main`, writes the plan at
   `.artifacts/plans/<today>-<topic>-plan.md` in the house `[Plan]`
   style, resolving every open question with a recorded decision,
   commits it (`docs(plan): …`). No execution of the investigation,
   no implementation, no push. Origin sections marked normative are
   binding; deviations need a recorded reason.
2. **Stage 2 — execution agent**: continues in that worktree
   (recreates it on the branch if missing), executes per the
   approved plan — research, experiments, and the deliverable
   report/doc — runs the full gate suite (AGENTS §10) if any code
   changed, and keeps the repo diff to exactly the artefacts the
   chunk owns. Does **not** push or open a PR. Reports deviations.
3. **Stage 3 — review agent**: reviews the entire branch diff versus
   its base against the chunk's acceptance criteria + AGENTS rules
   **plus the evidence & artefact mandate below**, re-runs whatever
   gates apply, implements fixes for every finding on the branch
   (amend/reword, never duplicate-subject fixups). Its last task is
   shipping: push the branch, open the PR to `main` with `gh`, watch
   `gh pr checks --watch` to completion — any failing check is a
   finding; diagnose, fix, push `--force-with-lease`, repeat until
   green. Reports a verdict. Never merges; never touches `main`.

**Evidence & artefact mandate (stage 3, every chunk in this wave —
the research/docs variant of the code-quality mandate):**

- **Evidence**: every factual claim in a report carries its evidence
  — the command run plus the observed outcome (docker-tier
  experiments), or the verifiable source (link + what it states).
  Experiments must be reproducible from the report's method section
  alone (tier-2 harness: `docker/nextcloud-test/compose.yaml` +
  `scripts/nextcloud_it_setup.sh`; versions pinned in
  `docs/testing_strategy.org` — currently nextcloud:35 / Deck
  1.19.0).
- **Decision-grade**: recommendations name the options, the
  trade-offs, exactly **one** recommendation, and its consequences
  for later waves; a follow-on planner must be able to act without
  re-deriving the analysis.
- **Repo hygiene**: scratch/exploration code and captured payloads
  never land in the repo — evidence is *excerpts inside the report*,
  not files. The PR diff is exactly the chunk's plan + report +
  bookkeeping edits.
- **Scope discipline**: no speculative content beyond the chunk's
  questions; out-of-scope ideas discovered along the way go to the
  backlog per its template, not into the report's scope.

The connect-flow dispatch plan's **code-quality mandate** (naming,
comment discipline, SRP via crate boundaries, single level of
abstraction) is not directly applicable to this wave's research
chunks; chunk 4 must copy it **verbatim** into the derived
implementation dispatch plan, where it becomes binding again.

## 2. Chunk 1 — D1: Deck round-trip strategy spike (`docs/deck-roundtrip-spike`)

- **Covers**: analysis report §4.1 (decision D1, the keystone) + the
  backlog entry "[2026-10-14] Deck round-trip strategy for app-only
  fields". Question: how do fields Deck cannot represent
  (scheduled-start, effort, importance/urgency, recurrence
  parameters, reminder rules) round-trip through a Deck sync
  connection? Candidate vehicles to investigate:
  (a) **Deck custom fields** — establish *factually* whether any
  custom-fields API surface exists on Deck 1.19.0 (capabilities
  endpoint, OCS route survey, `occ app:list`; absence is a
  legitimate, well-evidenced finding, not a failure);
  (b) **attachment-stored sidecar** — a structured JSON document per
  board (or per card) written/read as a Deck attachment, merged
  set-union-style locally;
  (c) alternatives to reject *for stated reasons* (description-
  embedded markers, comments) so later agents do not re-litigate.
  Known starting fact: our client implements attachment *metadata
  listing* only (`client.rs` `attachments()`); writes/content
  download are the existing "[2026-10-08] Deck attachment writes and
  content download" backlog entry — the spike probes the endpoints
  ad hoc (curl/scratch), it does not extend the production client.
- **Deliverable**:
  `.artifacts/reports/<today>-deck-roundtrip-strategy-report.md`
  (house `[Report]` style) + an annotation on the D1 backlog entry
  (report landed; entry stays open — Wave 1 implements).
- **Acceptance**: the vehicle matrix is fully evidenced per tier-2
  experiments (availability, API shape, permissions/scopes, payload
  limits, per-field granularity, conflict/merge semantics, bandwidth
  cost on the kiosk budget); exactly one recommendation with a
  merge-policy implication section checked against the *actual*
  `apply_sync_report` / `LocalOp` vocabulary
  (`crates/taskboard-domain/src/merge.rs`, `pipeline.rs`,
  `outbox.rs` — claims must match the code, not paraphrase it); a
  Wave-1 impact verdict per planned field (syncs day one vs
  local-only-first with its migration story); reproducible method
  section; repo diff = plan + report + backlog annotation only;
  gates green (docs-only PR; pre-commit runs fmt/clippy regardless).

**Prompt 1.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 1 of the future-directions wave 0 ("D1: Deck round-trip strategy spike", branch docs/deck-roundtrip-spike) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, then the wave analysis .artifacts/reports/2026-10-14-future-directions-analysis-report.md §4.1 (decision D1 — normative for the question being answered) and §6 Wave 0 row, the backlog entry "[2026-10-14] Deck round-trip strategy for app-only fields", and the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§2 (shared execution model + evidence mandate are binding).
2. Read the surfaces the spike reasons about (do not modify them): crates/taskboard-sync-nextcloud/src/client.rs (note attachments() is metadata-listing only), src/model.rs, and the merge vocabulary the recommendation must respect — crates/taskboard-domain/src/{merge.rs, pipeline.rs, outbox.rs} — plus docs/testing_strategy.org Tier 2 (harness + pinned versions) and the backlog entry "[2026-10-08] Deck attachment writes and content download".
3. Create the worktree ../taskboard-WT-docs-deck-roundtrip-spike on a new branch docs/deck-roundtrip-spike off main.
4. Write the investigation plan at .artifacts/plans/<today>-deck-roundtrip-spike-plan.md (house [Plan] style): the vehicle comparison matrix (custom fields / attachment sidecar / rejected alternatives) with one evidenced row per question (availability on Deck 1.19.0, API shape, auth scope, payload limits, granularity, conflict semantics, kiosk bandwidth); the docker-tier experiment protocol (compose up, app-password minting via scripts/nextcloud_it_setup.sh, the exact ad-hoc HTTP calls per vehicle, what evidence to capture — request/response pairs, occ outputs); the sidecar merge sketch (document granularity board-vs-card, set-union keys, how it would ride apply_sync_report's evidence-then-verdict without weakening per-field LWW); the report outline; explicit out-of-scope (no production code, no client extension, no schema). Resolve every open question with a recorded decision.
5. Commit it as: docs(plan): deck round-trip strategy spike. Do not execute the investigation; do not push.

Report: plan path, decisions, risks, estimated effort.
```

**Prompt 1.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 1 of the future-directions wave 0 ("D1: Deck round-trip strategy spike").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§2, and the stage-1 plan (.artifacts/plans/*deck-roundtrip-spike*-plan.md) in the worktree ../taskboard-WT-docs-deck-roundtrip-spike (branch docs/deck-roundtrip-spike). Recreate the worktree on that branch if missing.
2. Execute the investigation exactly per the approved plan: bring up the tier-2 Nextcloud (docker/nextcloud-test/compose.yaml + scripts/nextcloud_it_setup.sh), run the experiment matrix with ad-hoc HTTP calls from the worktree or /tmp (curl or a scratch reqwest binary — NEVER committed), and capture the evidence. Absence of a custom-fields surface must be established positively (capabilities endpoint, OCS route survey, occ app:list), not assumed.
3. Write .artifacts/reports/<today>-deck-roundtrip-strategy-report.md (house [Report] style): context, method (reproducible), the evidenced matrix, exactly one recommendation, merge-policy implications stated against the actual apply_sync_report/LocalOp code, per-field Wave-1 impact (syncs day one vs local-only-first + migration story), risks, open questions.
4. Annotate the backlog entry "[2026-10-14] Deck round-trip strategy for app-only fields" with a "> Update <date>: report landed at <path>" note (the entry stays open — Wave 1 implements it). Nothing else in the backlog changes.
5. The repo diff must be exactly: the stage-1 plan, the report, the backlog annotation. Scratch code and captured payloads stay out of the repo (evidence lives as excerpts in the report). No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: recommendation reached, evidence highlights, any deviation from the plan and why.
```

**Prompt 1.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 1 of the future-directions wave 0 (branch docs/deck-roundtrip-spike).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§2 (the evidence & artefact mandate applies to you), the stage-1 plan, and the wave analysis §4.1; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-docs-deck-roundtrip-spike.
2. Evidence review: every factual claim in .artifacts/reports/*deck-roundtrip-strategy-report.md traces to captured evidence (command + observed outcome); the method section reproduces the experiments from a clean tier-2 boot; the custom-fields availability question was established positively; the rejected alternatives carry reasons.
3. Decision-grade review: exactly one recommendation, argued; the merge-policy implications match the real code (read crates/taskboard-domain/src/merge.rs, pipeline.rs, outbox.rs and diff the report's claims against them — a paraphrase drift is a finding); the per-field Wave-1 verdicts are complete for the fields named in the wave analysis §3.1; a Wave-1 planner can act without re-deriving.
4. Artefact review: the diff is exactly plan + report + the single backlog annotation; no scratch files, no captured payloads, no scope creep; house [Report] style holds.
5. Re-run whatever gates apply (docs-only PR; the pre-commit fmt/clippy hooks run on commit). Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
6. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 3. Chunk 2 — D6: ADHD design-principles research spike (`docs/adhd-research-spike`)

- **Covers**: analysis report §3.4 (ADHD row) + §4.1 decision D6: run
  the research *early* so its principles inform the GUI design
  session's vocabulary and the Wave-1/2 feature designs, instead of
  being bolted on later. The brainstorm's own framing: survey the
  crowded ADHD-app space, and examine how novelty, interest,
  challenge, and urgency (the four factors one psychologist named)
  can be purposefully created. Guardrails: taskboard is a tool, not
  therapy — no medical claims, no diagnostic framing; wellbeing /
  ADHD-related data is privacy-sensitive (a design note for the
  future journal/review features, D5 substrate).
- **Deliverable**:
  `.artifacts/reports/<today>-adhd-support-research-report.md` —
  survey of the ADHD-oriented app space (what works, what
  repeatedly ships: body-doubling, task-chunking, timeboxing,
  novelty/urgency mechanics, friction reduction, forgiveness UX),
  the research basis (task initiation, time perception, executive
  function, habit formation), and a distilled **principles
  catalogue**: each principle stated, sourced, and mapped to
  concrete taskboard surfaces (GUI brief R-items, roadmap waves
  1–3 features) with an explicit "what we would build" line.
  Out-of-scope ideas discovered along the way → backlog entries per
  template, not report scope.
- **Acceptance**: every principle is actionable (names a taskboard
  surface and a build direction — platitudes are findings);
  citations resolve and support the claims they are attached to;
  the guardrails hold (tool-not-therapy, privacy note present);
  mappings reference real roadmap items (wave analysis §3/§6) and
  GUI brief sections; house style; gates green (docs-only).

**Prompt 2.1 (planning):**

```text
You are the stage-1 PLANNING agent for chunk 2 of the future-directions wave 0 ("D6: ADHD design-principles research spike", branch docs/adhd-research-spike) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the wave analysis .artifacts/reports/2026-10-14-future-directions-analysis-report.md §3.4 + §4.1 D6 + §6 Wave 0 row, the GUI requirements brief .artifacts/plans/2026-10-14-gui-design-requirements-plan.md (the surfaces principles must map onto), and the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§3.
2. Create the worktree ../taskboard-WT-docs-adhd-research-spike on a new branch docs/adhd-research-spike off main.
3. Write the research plan at .artifacts/plans/<today>-adhd-research-spike-plan.md (house [Plan] style): the research questions (what does the ADHD-app space ship and what do users report helping; what is the research basis for novelty/interest/challenge/urgency, task initiation, time perception, and habit formation; which principles transfer to a keyboard-first, privacy-respecting, offline-first task manager); the source strategy (which app ecosystems and communities to survey, which research literatures, how to weight primary sources over listicles); the principles-catalogue format (statement / source basis / taskboard mapping / build direction, each mapped to wave-analysis or GUI-brief items by section number); the guardrails (tool-not-therapy, no medical or diagnostic claims, privacy note for future wellbeing data); the report outline. Resolve every open question with a recorded decision.
4. Commit it as: docs(plan): adhd design-principles research spike. Do not execute; do not push.

Report: plan path, decisions, source-strategy risks, estimated effort.
```

**Prompt 2.2 (execution):**

```text
You are the stage-2 EXECUTION agent for chunk 2 of the future-directions wave 0 ("D6: ADHD design-principles research spike").

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§3, and the stage-1 plan (.artifacts/plans/*adhd-research-spike*-plan.md) in the worktree ../taskboard-WT-docs-adhd-research-spike (branch docs/adhd-research-spike). Recreate the worktree on that branch if missing.
2. Execute the research per the approved plan (web research; cite sources with what each states). Write .artifacts/reports/<today>-adhd-support-research-report.md (house [Report] style): method, the app-space survey, the research basis, the principles catalogue (each principle: statement, source basis, taskboard mapping by wave-analysis/GUI-brief section, build direction), guardrails section, open questions, references.
3. Out-of-scope improvement ideas discovered along the way: append each to the potential-improvements backlog using its entry template (they are NOT report scope). No other repo file changes.
4. No push, no PR — shipping is stage 3's last task. Never push to or merge main.

Report: principle count, strongest mappings, any deviation and why.
```

**Prompt 2.3 (review):**

```text
You are the stage-3 REVIEW agent for chunk 2 of the future-directions wave 0 (branch docs/adhd-research-spike).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§3 (the evidence & artefact mandate applies to you), the stage-1 plan, and the wave analysis §3.4/§4.1 D6; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-docs-adhd-research-spike.
2. Evidence review: citations resolve and actually support the claims they anchor (spot-check every load-bearing one); survey claims distinguish user reports from research findings.
3. Decision-grade review: every principle names a taskboard surface and a build direction (platitudes and unmapped principles are findings); mappings reference real sections of the wave analysis / GUI brief (verify the section numbers exist); the guardrails hold (no medical/diagnostic claims, privacy note present); scope discipline (new ideas went to the backlog per template, not into the report).
4. Re-run whatever gates apply (docs-only PR; pre-commit hooks). Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
5. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch; any failing check is a finding — diagnose, fix, push --force-with-lease, repeat until green.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation. Never merge; never touch main.
```

## 4. Chunk 3 — GUI Phase 7 design round (`docs/gui-phase7-design`)

- **Covers**: the design session's agent support, in three stages:
  (1) the **Slint stress-test research** — execute the GUI brief's
  §4 checklist against the current Slint release (the ui crate has
  no pinned version yet — record the version surveyed and propose
  the pin): focus traversal control + programmatic focus, global
  key-event interception (the action-dispatch seam), runtime
  theming via singletons/design tokens + text-role abstraction,
  scale-factor control as a user setting, `slint::testing` API
  coverage (input injection, property reads) for the R6 logical
  harness, accessibility/screen-reader integration status,
  `StyledText` rich-text status (editing vs rendering — the org-mode
  reports' open gate), renderer choice for kiosk hardware
  (skia vs femtovg, the "[2026-10-07] Add the slint dependency"
  backlog entry owns the evaluation), and the Wayland+X11
  screenshot/capture tooling options for R6's visual half;
  (2) the **design plan draft** for the user session; (3) the
  **pre-session review** that ships the draft PR.
- **The human gate**: the design plan lands with
  `Status: DRAFT — for the user design session`. The session (the
  user, iterating on the PR in review rounds — the connect-flow
  precedent) flips it to `ACCEPTED` with its decisions section
  filled. **No agent merges this branch**; amendment rounds during
  the session may be dispatched as follow-up stage-2-style
  invocations against the same branch.
- **Deliverables**:
  `.artifacts/reports/<today>-slint-gui-stress-test-report.md` and
  `.artifacts/plans/<today>-gui-phase7-design-plan.md` (house
  styles). The design plan must contain: the v0.1.0 scope fence
  (FOUNDATION vs POST-1.0 per the GUI brief's tagging — Phase 7's
  exit criterion itself stays as the growth roadmap defined it);
  normative foundation sections (theming/tokens, component-library
  layering, the action-dispatch keybinding architecture, scaling,
  focus/kbd journeys, automation-harness seam, the two first
  usability checks); the **decision agenda** for the session — every
  GUI-brief §6 open question plus everything the stress test
  surfaced, each with a proposed default, alternatives, and
  consequences; the proposed implementation chunk decomposition
  (chunk 4's input); a UI-crate testing strategy
  (`slint::testing` journeys, golden screenshots at scale
  extremes, the contrast-over-tokens CI check); an ADR draft for
  the UI architecture decision (verify the next free number by
  listing `docs/adr/` on `main` **and** grepping recent
  plans/branches for referenced-but-unlanded numbers — the
  numbering currently runs ahead: `main` holds 0001–0012 while
  landed-branch plans reference 0013/0014; flag the gap, do not
  collide silently).
- **Acceptance**: traceability — every GUI-brief requirement
  R1–R10, every §3 feature-surface item, and every §5 acceptance
  criterion appears in the design or is explicitly deferred with a
  recorded reason; facts are consistent with the stress-test report
  (version-pinned); no architecture violations (the ui crate stays a
  dumb presentation client: no IO crates, no business logic, unsafe
  only where Slint requires); the decision agenda is complete and
  answerable in one session; gates green.

**Prompt 3.1 (stress-test research):**

```text
You are the stage-1 STRESS-TEST RESEARCH agent for chunk 3 of the future-directions wave 0 ("GUI Phase 7 design round", branch docs/gui-phase7-design) in the taskboard repo at /home/viktoria/src/taskboard.

1. Read AGENTS.md, the GUI requirements brief .artifacts/plans/2026-10-14-gui-design-requirements-plan.md (§4 stress-test list is your catalogue; R1–R10 give each item's why), the wave analysis §6 Wave 0 row, and the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§4.
2. Read the prior art that already touched these questions so you verify rather than rediscover: .artifacts/reports/2026-10-11-org-mode-support-viability-report.md and 2026-10-11-emacs-org-edit-backend-viability-report.md (StyledText/rich-text status, Slint adapter assessments), plus the backlog entries "[2026-10-07] Add the slint dependency to taskboard-ui-slint" (renderer question) and "[2026-10-09] UI command forwarder bootstrap task".
3. Create the worktree ../taskboard-WT-docs-gui-phase7-design on a new branch docs/gui-phase7-design off main.
4. Execute the stress test against the current stable Slint release (record the exact version; a throwaway cargo project outside the repo, e.g. /tmp, may compile minimal probes where documentation is ambiguous — never commit it): each §4 checklist item gets a verdict (supported / partial / unsupported / unclear-with-evidence) with version-pinned sources (docs.rs, changelogs, upstream issues). Include the renderer comparison (skia vs femtovg on software-rendering/kiosk constraints) and the Wayland+X11 capture-tooling survey (external to Slint; options + permissions story). Where a verdict gates a GUI-brief requirement, say which one.
5. Write .artifacts/reports/<today>-slint-gui-stress-test-report.md (house [Report] style) and commit the stage-1 research plan first at .artifacts/plans/<today>-slint-gui-stress-test-plan.md (what you will verify, how, with which sources — one commit before the research starts). Commit as: docs(plan): slint gui stress test scope, then docs(report): slint gui stress test findings. Do not draft the design plan; do not push.

Report: verdict per checklist item, surprises, the pin proposal, estimated effort.
```

**Prompt 3.2 (design draft):**

```text
You are the stage-2 DESIGN-DRAFT agent for chunk 3 of the future-directions wave 0 ("GUI Phase 7 design round").

1. Read AGENTS.md; the GUI requirements brief .artifacts/plans/2026-10-14-gui-design-requirements-plan.md (normative input — every R1–R10 requirement, §3 feature surface, and §5 criterion must be traceable); the stress-test report (.artifacts/reports/*slint-gui-stress-test-report.md) and its stage-1 plan; the wave analysis .artifacts/reports/2026-10-14-future-directions-analysis-report.md (§6 Wave 0; §3.7 view inventory); the growth roadmap .artifacts/plans/2026-10-08-growth-roadmap-plan.md Phase 7 (the exit criterion is fixed — the fence, not the scope creep); docs/architecture.org (ui crate contract); and the dispatch plan §1/§4.
2. Work in the worktree ../taskboard-WT-docs-gui-phase7-design (branch docs/gui-phase7-design). Recreate the worktree on that branch if missing.
3. Write .artifacts/plans/<today>-gui-phase7-design-plan.md (house [Plan] style, Status: DRAFT — for the user design session) with: the v0.1.0 scope fence (FOUNDATION vs POST-1.0, adopting the brief's tagging and the stress-test verdicts); normative foundation sections — theming/tokens + text-role vocabulary, component-library layering rules, the action-dispatch keybinding architecture (action namespace, keymap-as-data, preset strategy, the Rust-side module decision), scaling + responsive composition, keyboard-journey acceptance bar, the automation-harness seam (logical now / visual tooling follow-up), the first two usability checks; a UI-crate testing strategy; the decision agenda (GUI-brief §6 questions + stress-test surprises, each with proposed default, alternatives, consequences); the proposed implementation chunk decomposition for chunk 4; an ADR draft for the UI architecture (next free number: list docs/adr on main AND grep recent plans/branches for referenced-but-unlanded numbers — the numbering currently runs ahead of main; record the gap in the plan). Every open question resolves into either a recorded proposal or an agenda row — nothing stays implicit.
4. Commit as: docs(plan): gui phase7 design draft for session. No implementation, no push, no PR.

Report: plan path, agenda size (question count), proposed defaults you are least sure about, estimated effort.
```

**Prompt 3.3 (pre-session review & ship):**

```text
You are the stage-3 REVIEW agent for chunk 3 of the future-directions wave 0 (branch docs/gui-phase7-design).

1. Read AGENTS.md, the dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§4 (the evidence & artefact mandate applies to you), the GUI requirements brief, the stress-test report, and the design-plan draft; then review the full branch diff (git diff main...HEAD) in ../taskboard-WT-docs-gui-phase7-design.
2. Traceability review: build the matrix — every brief requirement R1–R10, every §3 feature-surface item, every §5 acceptance criterion maps to a design section or an explicit deferral with a recorded reason; a silent drop is a finding. Facts in the design must not contradict the stress-test report (version-pinned).
3. Architecture review: the design keeps the ui crate a dumb presentation client (no IO crates, no business logic, no state mutation; AGPL-3.0-only header; unsafe only where Slint's generated code requires); the Phase 7 exit criterion is unchanged (foundation seams added, feature surface not scope-crept); the decision agenda is answerable in one session (each row has a default + alternatives + consequences, no open-ended questions).
4. Re-run whatever gates apply (docs-only PR; pre-commit hooks). Fix every issue directly on the branch (amend/reword — no duplicate-subject fixups).
5. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch until green. The PR description must state: this PR is the design session's substrate — review rounds with the user follow; it must NOT merge until the plan's Status is flipped to ACCEPTED with its decisions section filled.
6. Never merge; never touch main. If the user later asks for amendment rounds during the session, those are fresh stage-2-style invocations against this same branch, ending with the same never-merge rule.

Report: verdict, traceability gaps found and fixed, PR URL, green-checks confirmation.
```

## 5. Chunk 4 — Phase 7 implementation dispatch derivation (`docs/gui-phase7-dispatch`)

- **Precondition**: the design plan's Status is **ACCEPTED** with its
  decisions section filled (verify the header and the git log of the
  merged design branch; if DRAFT, STOP and report). Chunks 1–3
  merged.
- **Covers**: authoring
  `.artifacts/plans/<today>-gui-phase7-implementation-dispatch-plan.md`
  in this plan's house structure: the chunks table + dependency
  graph + the shared three-stage execution model **copied verbatim**
  from here (§1) together with the **code-quality mandate copied
  verbatim** from `.artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md`
  §1 (it becomes binding for every implementation chunk); per chunk:
  Covers (normative design-plan sections), Acceptance, and the three
  prompts. Provisional chunk shapes to reshape against the accepted
  design's own decomposition: (a) slint dependency + renderer choice
  + app/window shell + theme token module (+ contrast CI check);
  (b) action-dispatch/keymap layer with tests; (c) component library
  + board/stack/task views + focus/keyboard journeys; (d) automation
  seam + golden-screenshot gate; (e) ADR + architecture.org
  amendments + backlog ✅ bookkeeping. This is a docs-only chunk —
  two stages (author, review & ship), the implementation stage is
  inherent to its successor, not to it.
- **Acceptance**: every chunk's prompts name their normative design
  sections and acceptance criteria; the shared model + code-quality
  mandate are carried verbatim; worktree/branch naming follows
  AGENTS §8 (`feat/ui-*` branches expected); the no-push/no-merge
  stage boundaries are stated; the backlog ✅ mapping (slint-dep,
  renderer, UI-forwarder entries) is complete; gates green.

**Prompt 4.1 (author):**

```text
You are the AUTHOR agent for chunk 4 of the future-directions wave 0 ("Phase 7 implementation dispatch derivation", branch docs/gui-phase7-dispatch) in the taskboard repo at /home/viktoria/src/taskboard.

Precondition: the GUI design plan (.artifacts/plans/*gui-phase7-design-plan.md) is merged with Status: ACCEPTED and a filled decisions section. Verify before starting; if it is DRAFT or unmerged, STOP and report.

1. Read AGENTS.md; the accepted GUI design plan (normative — its chunk decomposition, scope fence, foundation sections, decision record); the GUI requirements brief; the stress-test report; the wave analysis §6; this dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1 (the shared execution model you must copy verbatim); and the code-quality mandate in .artifacts/plans/2026-10-12-connect-flow-implementation-dispatch-plan.md §1 (copy verbatim as well).
2. Create the worktree ../taskboard-WT-docs-gui-phase7-dispatch on a new branch docs/gui-phase7-dispatch off main.
3. Write .artifacts/plans/<today>-gui-phase7-implementation-dispatch-plan.md (house [Plan]/dispatch style, mirroring this plan's structure): chunks table + dependency graph (mark order-independent pairs); the shared execution model and code-quality mandate; per chunk: Covers (normative design-plan sections by number), Acceptance (gates, journeys, traceability), and the three stage prompts (planning / implementation / review — house voice, worktree per branch, no-push stage boundaries, stage-3 ships and never merges); completion bookkeeping (which backlog entries each chunk marks ✅ — at minimum "[2026-10-07] Add the slint dependency to taskboard-ui-slint" and "[2026-10-09] UI command forwarder bootstrap task" — and where the ADR + architecture.org amendments land). Adopt the design plan's decomposition; reshape the provisional shapes in the wave-0 dispatch plan §5 only where the accepted design says otherwise, recording why.
4. Commit as: docs(plan): gui phase7 implementation dispatch. Do not implement; do not push.

Report: chunk count + dependency graph, where you deviated from the provisional shapes and why, estimated total effort.
```

**Prompt 4.2 (review & ship):**

```text
You are the REVIEW agent for chunk 4 of the future-directions wave 0 (branch docs/gui-phase7-dispatch).

1. Read AGENTS.md, the accepted GUI design plan, this wave-0 dispatch plan .artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md §1/§5, and the connect-flow dispatch plan §1; then review the new implementation dispatch plan (git diff main...HEAD) in ../taskboard-WT-docs-gui-phase7-dispatch.
2. Completeness review: every design-plan normative section is owned by exactly one chunk (orphans and double-owners are findings); every chunk has Covers + Acceptance + three prompts; the shared execution model and the code-quality mandate are verbatim copies (diff them — paraphrase drift is a finding); prompt hygiene (repo path, worktree naming, AGENTS §8 branch categories, stage boundaries: stages 1–2 never push, stage 3 ships and never merges, --force-with-lease only); the backlog ✅ mapping is complete and names the entries by their exact titles; the dependency graph is honest about order-independence.
3. Sanity-check the acceptance criteria against AGENTS §5/§10 (no text assertions, no sleeps, deterministic journeys, full gates) and the ui-crate rules (AGPL header, no IO crates, unsafe only via Slint).
4. Re-run whatever gates apply (docs-only PR; pre-commit hooks). Fix every issue directly on the branch (amend/reword).
5. Last task — ship: push the branch, open the PR to main with gh, then gh pr checks --watch until green. Never merge; never touch main.

Report: verdict, findings, what you fixed, PR URL, green-checks confirmation.
```

## 6. Completion bookkeeping

- **Chunk 1** annotates (does not close) the "[2026-10-14] Deck
  round-trip strategy for app-only fields" entry; the answer feeds
  the Wave-1 field-family plan, which closes it.
- **Chunk 2** may append parking-lot backlog entries for
  out-of-scope ideas per template; the report itself is referenced
  from Wave-1/2 planning rounds and the GUI session.
- **Chunk 3** consumes the "[2026-10-07] Add the slint dependency"
  entry's renderer question (stress-test report answers it; the
  entry closes only when the dependency actually lands); the
  design-plan PR stays open through the user's review rounds.
- **Chunk 4**'s derived plan owns every later ✅.
- **Deliberately not dispatched here**: the GUI implementation
  chunks (chunk 4's deliverable), reminders "plan 5" desktop toast
  sink (Wave 1; existing backlog entry), all Wave-1+ features, and
  the daemon-side attach IPC verb (still the connect-flow round's
  deferred list).
- After chunk 4 merges, **wave 0's remaining work is executing the
  derived dispatch plan's chunks**; wave 1 opens with the
  D1-informed field-family plan.
