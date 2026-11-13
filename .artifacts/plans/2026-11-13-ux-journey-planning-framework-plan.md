# [Plan] UX Journey Planning Framework — Design & Rollout

- **Date**: 2026-11-13
- **Kind**: process/planning framework design plan. Defines a new planning
  artefact type (the **UX journey plan**), an automated completeness gate,
  a review-stage usability-walkthrough protocol, and the doc/template/script
  changes that install them. No application code.
- **Origin**: the user's 2026-11-13 problem statement — features repeatedly
  land complete in domain/engine/storage/sync/CLI but unsatisfying or absent
  in the GUI (display-without-control, missing creation paths, incomplete
  interaction coverage, inconvenient Settings UX), requiring several GUI
  review rounds to converge. The user asked for a planning framework with
  user stories + interaction flowcharts (their two seed ideas) plus whatever
  else research supports.
- **Status**: ACCEPTED 2026-11-13 with the user's four decisions (§8) and a
  restructured rollout (§6): advisory during integration, mandatory for new
  user-visible work only after the census + retrofit + remediation backlog
  (phases 3–5) completes; the pilot plans blind to the already-implemented
  round-7 chunks 1–2, then compares implementations against the new plans
  and adjusts the round-7 plans. §7's changes land as the phase-1
  docs/process chunk.
- **Verified against**: `main` @ `35e6b0a1`. Evidence anchors were read at
  that SHA; expect drift.

---

## 1. Problem & evidence — why GUI integration keeps under-delivering

The pattern is measurable in this repository, not anecdotal:

- **C1 — Engine-first planning, UI as an afterthought.** Every capability
  wave planned domain/engine/storage exhaustively with thin-to-zero UI
  content: the recurring/habits design round carries one UI subsection
  (§4.6, "Surface (today view, CLI)",
  `2026-10-26-recurring-habits-design-round-plan.md:514-527`) that assigns
  habit *creation to the CLI* with no entry-point analysis; the five
  files-sync waves (`2026-11-05-filesync-substrate…`, `…sync-files-core…`,
  `…sync-files-nextcloud…`, `2026-11-07-boards-over-files…`,
  `2026-11-08-boards-over-files…`) contain **zero mentions of the UI crate
  or `.slint`** (verified by grep across all five). Completeness was defined
  at the engine seam, never at the user surface.
- **C2 — UI planning happens reactively, as remediation.** Seven "GUI review
  round" cycles exist (rounds 1–7, 2026-10-22 → 2026-11-13), each written
  from the user's *observed* gaps. Round 7's headline
  (`2026-11-13-gui-review-round7-plan.md:26-33`): "the engine, domain, and
  storage layers already implement every capability the user asks for —
  what is missing is exclusively the GUI surface". The same failure mode —
  capability reachable only via CLI — recurred for recurrence rules, habit
  creation, and files-board management.
- **C3 — No journey, entry-point, or capability artefact exists anywhere.**
  No plan enumerates user journeys or asserts "every user-visible verb has
  a GUI entry point or a recorded ruling". The word "journey" in house
  vocabulary means *keyboard test journeys* (`tests/journeys.rs`, the ten
  core verbs of v0.1.0). Acceptance criteria are seam-level (exact command
  dispatched, focus parity, kiosk gates) and pass green even when the
  creation path is CLI-only — **nothing can fail for a missing entry
  point**.
- **C4 — Oscillating chunk-local rulings instead of a stable UX model.**
  Round 6 explicitly ruled "no action buttons (non-goals: no
  disconnect/bind UI)" for the Files card
  (`2026-11-12-ui-r6-files-boards-surface-plan.md:255-256`) and landed a
  `FilesMovePicker` dropdown; round 7 overturns the ruling
  (`2026-11-13-gui-review-round7-plan.md:182-184`) and retires the
  one-round-old picker ("the roster is the picker"). Decisions about *what
  a surface is for* were made chunk-locally, with no cross-feature object
  to diff against.
- **C5 — Structural-test discipline can only see what a plan pinned.** The
  UI is deliberately id-less and index/role-addressed (`ItemAddr` tables,
  "no id enters the model", `settings-view.slint:83`), tests assert typed
  outcomes under the no-text-assertion rule — so an affordance no plan ever
  named (an "Add habit…" button, a recur editor) has no failing test to
  expose its absence. The dispatch-plan template (238 lines) spends its
  specificity budget on process mechanics; it has **no section for journeys,
  entry points, view/element inventories, or UI completeness criteria**.

## 2. Gap classes the framework must kill

| # | Class | Real instance |
|---|---|---|
| G1 | GUI not integrated at all | files-sync waves: full engine, zero Settings surface until round 6 |
| G2 | Display without control | recurrence badge + pause/resume/convert rows, but no UI constructs `TaskChanges { recur: … }` (R7-1) |
| G3 | Partial interaction coverage | task↔habit conversion exists; habit *creation* CLI-only (R7-2); Deck→Files move exists; local→Files publish missing (R7-3) |
| G4 | Workable but hole-ridden, inconvenient UX | Settings sync-connection management across multiple editions; no cancel/escape paths; entry points scattered |

Every framework component below states which classes it kills; a component
that kills none is cut (§3's rejections).

## 3. Method survey & evaluation

Researched 2026-11-13; adopted/adapted/rejected with the gap classes as the
acceptance test.

### 3.1 Adopted

| Method | What it contributes | Why it fits here | Kills |
|---|---|---|---|
| **User stories** (Connextra format + INVEST qualities; acceptance as Given/When/Then from BDD) | ID-able intent anchors (`US-n`) a plan, a dispatch chunk, a test name, and a review verdict can all reference | The repo already speaks in stable IDs (D-x, R7-x, WI-x, #issue); stories slot into that discipline. G/When/Then maps onto the house seam tests (assert typed outcomes, never text) | G1, G3 |
| **User story mapping** (Patton) — backbone activities in narrative order, stories as ribs, **release slicing** so every slice completes an end-to-end walk | The discipline that a shippable increment must complete journeys, not accumulate backend halves | Directly answers the user's "complet able after first implementation": the first dispatch slice must finish every P1 story end-to-end. Maps 1:1 onto dispatch-plan chunking | G1, G3 |
| **Job stories** (Klement: "When ⟨situation⟩, I want to ⟨motivation⟩, so I can ⟨outcome⟩") | Situation-triggered framing without personas | taskboard's kiosk mode, offline resilience, and sync-conflict moments are *situations*, not roles — job stories fit these better than role stories | G4 |
| **Use-case extensions** (Cockburn) — main success scenario + numbered extensions (2a: step fails ⇒ …) | Failure/branch paths as first-class numbered items | Becomes the mandatory **escape-path column** of every journey step (cancel/undo/typed-error handling) | G4 |
| **Interaction flowcharts** (user's seed idea #2) — mermaid `flowchart TD` for multi-surface journeys, `stateDiagram-v2` for modal/editor machines | Entry points, decision points, and reachability made visually explicit and reviewable in the plan | Mermaid is already house-validated for backend flow (`docs/flow_model.org`); GitHub renders it natively in plans, PRs, and issues. This would be its first UI use | G1, G3, G4 |
| **Statecharts** (Harel; statecharts.dev) for overlay/dialog logic — hierarchy, guards, impossible states excluded by construction | Editors like the round-7 recur-editor (pattern/bound draft validity, armed-confirm strip) are literally state machines; a plan-time statechart forces the invalid-draft, armed-confirm, and escape transitions to be designed *before* implementation | Slint itself has native `states`; the chart stays plan-time documentation (no XState-style runtime machinery — Slint owns state) | G4 |
| **Cognitive walkthrough** (Polson/Rieman; "will the user see the control, recognize the action, understand the feedback, recover from the error?") as a **review-stage agent protocol** | Task-focused learnability inspection that needs no users | 2025 research shows LLM agents perform credible scaled walkthroughs (Synthetic Cognitive Walkthrough, arXiv 2512.03568; Amazon's UXAgent). Our stage-3 review agent can role-play each story against the real UI through the existing `i-slint-backend-testing` driver | G4 |
| **Heuristic evaluation** (Nielsen's 10, subset) | Cheap cross-cutting quality rubric | Paired with the walkthrough as its second pass; six heuristics chosen below (§4.5) | G4 |
| **E2E journey automation** | Journeys that click the real widgets and assert the dispatched commands | Already house-proven: 21 integration test files under `crates/taskboard-ui-slint/tests/` incl. `journeys.rs`, `seam_inventory.rs`, `golden_visuals.rs`, driven through `tests/support/driver.rs` over `i-slint-backend-testing`. The framework only mandates coverage, invents no harness | G1, G2, G3 (mechanically) |

### 3.2 Adapted

- **Gherkin/Cucumber**: adopt the Given/When/Then *discipline* inside
  journey steps and acceptance criteria; reject Cucumber tooling — the
  scenarios compile to ordinary Rust tests whose names embed journey-step
  IDs. (The no-text-assertion rule means scenarios must assert typed
  outcomes — commands, enums, flags — never label prose.)
- **Customer journey maps / HTA (hierarchical task analysis)**: rejected as
  standalone methods (marketing-flavored / heavyweight for one maintainer),
  but their skeleton — stages → actions → thoughts → pain points — is
  folded into the journey-step table columns.

### 3.3 Rejected

- **Persona construction** — a single-maintainer personal tool; personas
  would be theater. Roles in stories stay concrete (desktop user, kiosk
  viewer).
- **Runtime state-machine libraries / XState-style machinery** — the UI is
  a dumb presentation client (AGENTS §2); adding a UI-side orchestration
  runtime violates the architecture. Statecharts live in plans only.
- **Full UML use-case suites** — the journey-step table with escape
  columns is Cockburn's format already, minus the ceremony.

## 4. The framework

Four components, one per root cause:

| Component | Kills | Root cause |
|---|---|---|
| 4.1 UX journey plan artefact | G1–G4 | C1, C3 |
| 4.2 Journey-first derivation + P1-slice rule | G1, G3 | C1, C2 |
| 4.3 Automated completeness gate (`scripts/ux-coverage.sh`) | G1–G3 mechanically | C5 |
| 4.4 Review-stage walkthrough protocol + UX surface census | G4 | C2, C4 |

### 4.1 The UX journey plan (new artefact type, new template)

**Trigger rule**: any plan or design round that adds or changes a
user-visible capability — any new verb, badge, view, or setting a human can
perceive or act on — must include (or cite) an accepted UX journey plan
meeting this bar before a dispatch plan may chunk the work. Pure engine/
storage/sync internals with no surface delta are exempt. Per the user's
2026-11-13 decision the rule is **advisory during the integration phases
(§6 phases 1–5)** — it binds a chunk whenever that chunk's origin cites a
UX journey plan — and becomes **mandatory for all new user-visible work at
phase 6** (mandate activation).

File: `.artifacts/plans/YYYY-MM-DD-<topic>-ux-plan.md`, from
`.artifacts/templates/ux-journey-plan.md` (full draft in Appendix A). Its
sections, each with the gap class it kills:

1. **Scope & goals** — what the user can now do that they couldn't;
   explicit non-goals with homes (backlog issues), house style.
2. **Capability × surface matrix** (kills G2, G3) — rows: every verb on
   every entity the feature touches (`task.create`, `habit.create`,
   `board.publish_to_files`, `connection.disable`, …), **creation verbs are
   enumerated by default**; columns: the app's surfaces (Board, Task
   detail, Today, Review, Settings, Capture, CLI; platform-scoped where
   kiosk differs). Cells: `D` display / `E` edit / `C` create / `X`
   remove, each citing its story ID; `—` = deliberately absent. **Every
   `—` on a core verb (create/edit/remove of an entity the feature
   touches) requires a numbered ruling** in the rulings sub-table with a
   reason and a home. This makes "creation is CLI-only" a *visible,
   reviewable decision* (kills the round-6/round-7 oscillation, C4) and
   makes display-without-control a blank cell a reviewer can see at a
   glance.
3. **User stories** (`US-n`) — Connextra format (job-story format allowed
   for situation-triggered behavior), each with: platforms (desktop /
   kiosk), **entry points** (every affordance that starts the journey:
   button-in-view, menu, keyboard `ActionId`, CLI), priority (**P1** =
   must ship in the first slice; P2/P3 = later slices, each with a home),
   and acceptance criteria as Given/When/Then citing the verification
   method (journey test / seam test / fold test / manual).
4. **Journey specs** — per story, a step table `US-n/S-k`:
   surface+affordance, user action (gesture and/or `ActionId`), dispatches
   (the exact `StateCommand`/`UiRequest` variant), system response (state
   change + UI feedback: badge/tone/busy-dot/caption), and a **mandatory
   escape/failure column** (cancel, undo, typed-error handling — Cockburn's
   extensions). A step without an escape path is a plan defect.
5. **Interaction flow** (kills G1, G3, G4) — mermaid `flowchart TD` per
   multi-surface journey (entry points as sources, step IDs on edges) and
   `stateDiagram-v2` for every overlay/editor/modal (mount/armed/invalid/
   committing states, escape transitions). This is the user's seed idea #2,
   made ID-addressable so every node is greppable.
6. **Wiring inventory** (kills G1, G2 mechanically) — one row per new or
   changed affordance: surface file, widget/composition, **slint callback
   name**, adapter seam (method or `install_seam!` site), `ActionId`,
   command variant(s) with row addressing (`ItemAddr` kind for list rows —
   the UI is id-less by design, so callbacks + action ids + command
   variants are the greppable contract, not element ids), journey steps
   covered, and the test (file::test name). Rows with no command variant
   must say why (pure navigation/display).
7. **Command-contract delta** — every new/changed `StateCommand`,
   `UiRequest`, and `ActionId` the journeys imply, enumerated here. This
   section is the derivation hinge (§4.2).
8. **Verification plan** — which journey steps get journey tests (names
   embed the step ID: `us1_s2_edit_repeat_mounts_editor`), seam tests,
   presentation fold tests, kiosk gates; which acceptance criteria are
   manual and who runs them.
9. **Definition of done** — the §4.3 gate green; walkthrough protocol
   executed (§4.5); all matrix holes ruled; non-goals filed.

**Acceptance** works like design rounds already do: the user accepts the
UX plan; the dispatch plan's Origin field must cite it; dispatch chunks map
to its IDs.

### 4.2 Journey-first derivation and the P1-slice rule (kills C1)

Invert the current direction of derivation. Today: engine waves plan
verbs → UI is expected to "surface" them eventually. Under the framework:
**journeys enumerate the verbs**. The UX plan's command-contract delta
(§4.1.7) is written *before* the engine chunks, and engine work items
exist because a journey step dispatches them. "Engine done, UI missing"
becomes structurally hard: an engine verb with no journey step is
unplanned scope; a journey step with no engine verb is a missing chunk.

**P1-slice rule** (story-mapping's release slicing, applied to dispatch
plans): the set of chunks dispatched for a feature must, as a whole,
complete every P1 story's journey end-to-end — entry point to visible
outcome including escape paths. A dispatch plan whose chunk sum leaves a
P1 step unwired is invalid at planning time, not discovered in review
round N. P2/P3 stories may trail, but only *after* the P1 slice shipped.

### 4.3 Automated completeness gate — `scripts/ux-coverage.sh` (kills C5)

A parser + grepper over the UX plan's fixed-format tables (the template
pins column order, so parsing is stable markdown-table extraction):

```text
scripts/ux-coverage.sh <ux-plan.md> [--gate]
```

- For each **wiring-inventory row**: the callback name must appear in a
  `.slint` file under `crates/taskboard-ui-slint/ui/` **and** in the
  adapter (an `install_seam!`/`on_*` handler in `adapter.rs`); the command
  variant must exist in `crates/taskboard-domain/src/messages.rs`
  (`StateCommand`), `crates/taskboard-ui-slint/src/requests.rs`
  (`UiRequest`), or `crates/taskboard-ui-slint/src/actions.rs`
  (`ActionId`), as the row names.
- For each **journey step ID**: the token `us<n>_s<k>` must appear in a
  test name under `crates/taskboard-ui-slint/tests/` (or the crate the
  plan nominates) — the template's naming law makes test names the
  completion proof.
- For each **matrix cell** marked with a story: that story's steps are
  covered per the above; each `—` ruling must have a non-empty reason.
- Output: a PASS/MISS table (row → evidence file:line found or MISSING);
  `--gate` exits nonzero on any MISSING.

Honest limits, stated in the script's header: the gate proves *wiring
exists and is claimed-tested*, not that the UX is good — quality is the
walkthrough protocol's job (§4.5). The two layers together are the answer
to "agents can automatically analyse if the UX journey is fully
implemented": layer 1 is mechanical and cheap, layer 2 is agentic and
run at review time.

Consistency hook: the wiring inventory's callback column should be
cross-checkable against `tests/seam_inventory.rs` (a declared-but-never-
installed callback shows up as a wiring row with no seam — the gate flags
it rather than inventing new machinery).

### 4.4 Review-stage walkthrough protocol + UX surface census (kills C4)

**Walkthrough protocol** (added to the stage-3 review mandate when the
origin includes a UX plan — dispatch-template amendment in Appendix B):
for each P1 story, the review agent walks the journey against the actual
UI — driven through the existing test driver where practical, reasoned
over code otherwise — and answers, per step, the four cognitive-walkthrough
questions with file:line or test evidence:

1. Will the user *see* the control (entry point enumerated in the plan →
   present in the surface)?
2. Will they *recognize* what it does (label/hint semantics — judged, not
   text-asserted)?
3. Will they *understand the feedback* (badge/tone/busy-dot/caption
   follows the system-response column)?
4. Can they *recover* (escape column holds: cancel/undo/typed error)?

Then one heuristic pass over the touched surfaces using a six-heuristic
rubric (Nielsen subset): visibility of system status; user control &
freedom (escape/undo everywhere); error prevention & recovery; consistency
& standards (same verb = same widget/tone across surfaces); recognition
over recall (state visible, not remembered); keyboard/mouse parity (already
house law — the rubric keeps it in the UX frame). Findings classify as
BLOCKER (journey can't complete / escape missing — fixed on the branch),
MAJOR (heuristic violation — fixed on the branch or P2'd with the user's
call), MINOR (polish — backlog issue per AGENTS §7 standing authorization).

**UX surface census** — a living app-scale capability × surface matrix at
`docs/ux-surface-census.org`: the union of every feature's §4.1.2 matrix,
plus every standing ruling with date and authority. Each UX plan updates
its slice; each GUI review round diffs against it. This is the stable
cross-feature object whose absence let round 6 rule "disconnect is a
non-goal" and round 7 silently overturn it — overturning a ruling becomes
a visible census diff, made at UX-plan level, not a chunk-local surprise.

### 4.5 What lives where (no new sprawl)

| Artefact | Home | Cadence |
|---|---|---|
| UX journey plan | `.artifacts/plans/…-ux-plan.md` from new template | per feature |
| Framework law | `docs/ux_planning.org` (new, like `testing_strategy.org`) + AGENTS.md §7 pointer | stable |
| Census | `docs/ux-surface-census.org` | updated by every UX plan + review round |
| Gate | `scripts/ux-coverage.sh` | run by stage-2 (self-check) and stage-3 (mandate) |
| Dispatch integration | `.artifacts/templates/dispatch-plan.md` amendment | stable |

GitHub issues remain what AGENTS §7 already says: one-per-idea backlog
items and findings — stories live in the plan file, not as issues (avoids
issue sprawl and keeps the plan the single reviewable object).

## 5. House-rule compatibility (checked deliberately)

- **No-text-assertions (AGENTS §5)**: walkthrough judgments and plan tables
  may *cite* labels; tests assert typed outcomes (commands, enums, flags).
  The naming law (`us<n>_s<k>` tokens in test names) asserts structure,
  not prose.
- **Kiosk/desktop duality**: stories carry platform scope; matrix cells may
  be platform-qualified (`E (desktop)` / `D (kiosk)`); kiosk action-gating
  laws (D-T16) apply as today.
- **UI-crate purity (AGENTS §2)**: the framework adds zero UI-side logic;
  journeys name commands, the census is documentation, the gate reads
  files.
- **Id-less UI**: the wiring contract keys on callback names, `ActionId`s,
  command variants, and `ItemAddr` row kinds — the repo's real seams — not
  element ids.
- **One behavior, one test (AGENTS §5)**: journey tests live at the UI
  layer and assert dispatch + visible-model effects only; engine behavior
  stays snapshot-tested in `taskboard-state` — journeys don't re-test it.
- **Plans-follow-templates (AGENTS §7)**: the UX plan is a first-class plan
  type with its own template; dispatch plans cite it in Origin.

## 6. Rollout (restructured per the user's 2026-11-13 decisions)

Advisory first, mandatory later (§8 decision 1). Phase numbering note: the
census stock-take the user slotted "between old phases 2 and 3" is phase 3
here; the old "phase 3 — mandate" is phase 6. The pilot must respect live
round-7 state: chunk 1 (habit create, `feat/ui-r7-habit-create`) is
already merged, chunk 2 (recur editor) is in flight under the old-style
plans — so the pilot plans **blind first, compares after** (§6.1).

| Phase | Contents | Exit criteria |
|---|---|---|
| 1 — install (docs/process chunk, `docs/ux-journey-planning-framework`) | Appendix A template into `.artifacts/templates/ux-journey-plan.md`; Appendix B amendment to `dispatch-plan.md` (all mechanics conditional: "when the origin cites a UX journey plan" — that conditionality *is* the advisory mode); `docs/ux_planning.org` from §4 + the method survey; AGENTS.md §7 rule worded advisory-with-activation-condition (§7 item 4); `scripts/ux-coverage.sh` from Appendix C + fixture smoke test | chunk merged; gate green on the fixture's PASS and MISSING paths |
| 2 — pilot on round-7 scope (§6.1 discipline) | three UX journey plans authored blind from the round-7 design; comparison against the chunk-1/2 implementations; round-7 plan adjustments; chunk 3 + any re-works dispatched under the framework; census seeded with **only** the slices the pilot needs (Today/habits, Task detail/recurrence, Settings/files) | §6.1 steps 1–4 complete; gate proven on real PASS and MISSING rows |
| 3 — full census stock-take | dedicated agent pass taking stock of the *current implementation* over every surface (Board, Task detail, Today, Review, Settings, Capture, alarm window, CLI): completes `docs/ux-surface-census.org` with evidence anchors (every cell cites code/tests, round-7 verified-root-cause style), diffed against the phase-2 seed; standing rulings recorded with dates and authorities | census complete; stock-take report filed under `.artifacts/reports/` |
| 4 — retrofit analysis & triage | per existing UX feature: its own `…-ux-plan.md` authored from census + code evidence; then a consolidated **triage report**: journeys that cannot complete end-to-end, unruled absences, escape-path holes — classified BLOCKER/MAJOR/MINOR, each finding homed (backlog issues per AGENTS §7 standing authorization). The Settings sync-connection surfaces — the user's named worst offender — are expected to dominate | every existing UX feature has a ux-plan; triage report accepted by the user |
| 5 — remediation dispatch | dispatch plan(s) chunking the triage report's findings: fix the identified flaws and holes — the framework's full dress rehearsal at scale (advisory mode, used end-to-end) | remediation chunks merged; triage findings resolved or explicitly homed as P2/P3 |
| 6 — mandate activation | tiny docs chunk flips the AGENTS.md §7 wording from advisory to mandatory; `docs/ux_planning.org` records the activation and its date | from here on, new user-visible work requires an accepted UX journey plan before dispatch; GUI review rounds shrink to quality polish on complete surfaces |

### 6.1 Pilot discipline (live round-7 state)

Round-7 chunk 1 is merged; chunk 2 is in flight in its own worktree — the
pilot never touches it (AGENTS §8; round-7 chunks are independent, all off
`main`, no stacking — §5 of the round-7 plan). Strict ordering:

1. **Blind planning first.** Three UX journey plans (one per round-7
   solution — separate files so chunks cite precisely; a combined file is
   acceptable only for tightly-coupled features) are authored from the
   round-7 stage-1 plan (§2 D-7.1/D-7.2/D-7.3 is the design origin) plus
   this framework, reading surfaces at `git show <pre-chunk-1-sha>:<file>`
   (the last pre-round-7-implementation commit). The planning agent must
   NOT read the chunk-1/2 branches, diffs, worktrees, PRs, or any
   post-landed affordance code — the point is to prove the framework
   produces complete journeys from *design*, not to reverse-engineer
   implementations. Seam names the planner cannot know blind are recorded
   as plan assumptions. **Validation assertion**: evaluated against
   pre-round-7 `main`, the plans' capability matrices must show R7-1/2/3
   as blank/unruled cells — the framework's proof it would have caught
   them at planning time.
2. **Comparison after planning.** Run `scripts/ux-coverage.sh` per plan
   against the code — chunk 1 against merged `main`; chunk 2 against its
   landed `main` if merged, else its open PR branch at a fixed SHA
   (re-verified post-merge) — plus the walkthrough questions per story.
   Output: a comparison plan (house style) with a per-story/per-step
   verdict table — satisfied / deviating / missing — where every deviation
   resolves either as a plan correction (recorded ruling, plan amended) or
   a re-work item. An empty re-work list is a valid outcome: it validates
   the gate's PASS path and the round-7 plans' specificity, not a pilot
   failure.
3. **Round-7 plan adjustments.** The comparison plan is the authority for:
   (a) amending `2026-11-13-gui-review-round7-dispatch-plan.md` — a living
   dispatch document until its chunks complete — so chunk 3's prompts are
   re-authored under the framework (cite the UX plan's US/W/R IDs; stage 2
   runs the coverage gate; stage 3 runs the walkthrough protocol), and
   re-work chunks are appended for chunk-1/2 gaps on fresh branches
   (never reusing landed names, AGENTS §8; e.g.
   `feat/ui-r7-habit-create-ux-rework`); (b) the already-written stage-1
   implementation plans whose scope a re-work touches get a superseding
   stage-1 plan on the re-work branch citing the comparison plan — the
   round-6/round-7 overturn precedent, not in-place rewriting of accepted
   analysis.
4. **Census seed (minimal, deliberate).** Only the slices the pilot
   touches: Today (habits), Task detail (recurrence), Settings (files
   sync). The full census is phase 3's job.

## 7. Changes inventory (the Phase-1 chunk's diff)

1. `.artifacts/templates/ux-journey-plan.md` — new (Appendix A, minus
   guidance comments marked "delete after fill-in").
2. `.artifacts/templates/dispatch-plan.md` — amended (Appendix B).
3. `docs/ux_planning.org` — new: §4 of this plan promoted to standing law,
   plus the method survey for future reference.
4. `AGENTS.md` §7 — add the UX journey plan artefact type + the trigger
   rule **worded as advisory-with-activation-condition** (mandatory for new
   user-visible work once §6 phases 3–5 complete; the phase-6 chunk flips
   the wording) + pointer to `docs/ux_planning.org`; §10 unchanged — the
   gate stays a stage-2 self-check / stage-3 mandate, never CI (§8
   decision 2).
5. `scripts/ux-coverage.sh` — new (Appendix C), executable, with
   `tests/fixtures/ux-coverage-fixture-plan.md` exercising PASS and MISSING
   paths in a smoke test.
6. `.artifacts/plans/2026-11-13-ux-journey-planning-framework-plan.md` —
   this file, status → ACCEPTED (amended 2026-11-13: §6 restructured, §8
   resolved).

## 8. Resolved decisions (the user, 2026-11-13)

1. **Mandate strictness**: advisory during integration (phases 1–5); the
   trigger rule becomes mandatory for new user-visible features only after
   the backlog work — full census + retrofit of existing UX to the new
   standard *including fixing the identified flaws and holes* (phases
   3–5) — completes. Phase 6 flips the AGENTS.md wording.
2. **Gate placement**: stage-2 self-check + stage-3 mandate only; **no CI
   integration**, now or later (Appendix B already phrases all mechanics
   conditionally on the origin citing a UX plan).
3. **Census scope**: phase 2 seeds only the census it currently needs
   (Today/habits, Task detail/recurrence, Settings/files); the **full
   census is its own phase (3)** between the pilot and the retrofit.
4. **Kiosk stories**: platform tags on shared stories; no separate kiosk
   story sets (the template encodes this; matrix cells may
   platform-qualify, e.g. "E (desktop)").

## Appendix A — UX journey plan template (draft, verbatim for templates/)

```markdown
# [Plan] UX Journey Plan — <feature>
<!--
How to use: copy to .artifacts/plans/YYYY-MM-DD-<topic>-ux-plan.md,
delete guidance comments, fill every <...>. Written BEFORE the dispatch
plan that chunks this feature (the dispatch plan's Origin must cite this
file). Sections 2, 4, 6, 7 use fixed table headers — scripts/ux-coverage.sh
parses them; do not rename columns.
-->

- **Date**: <YYYY-MM-DD>
- **Kind**: UX journey plan — user stories, journeys, flows, wiring
  contract for <feature>. No implementation decisions beyond the command
  contract.
- **Origin**: <design round / plan / user request this expresses>
- **Status**: <DRAFT | ACCEPTED — dispatch may chunk from it>
- **Surfaces touched**: <views/compositions/files>
- **Platform scope**: desktop / kiosk / both (per story below)

## 1. Scope & goals
<What the user can now do that they couldn't. Non-goals, each with a home
(backlog issue #).>

## 2. Capability × surface matrix
<!-- Rows: every verb on every entity this feature touches — CREATE VERBS
ENUMERATED BY DEFAULT. Cells: D display, E edit, C create, X remove +
story ID. "—" = deliberately absent; every "—" on a core verb needs a
ruling below. Platform-qualify where kiosk differs: "E (desktop)". -->

| Capability | Board | TaskDetail | Today | Review | Settings | Capture | CLI |
|---|---|---|---|---|---|---|---|
| <entity>.<verb> | | | | | | | |

**Rulings** (every deliberate absence):

| ID | Ruling | Reason | Home |
|---|---|---|---|
| R-1 | | | |

## 3. User stories
<!-- Connextra: As a <role> I want <capability> so that <benefit>.
Job-story form allowed for situation-triggered behavior: When
<situation> I want to <motivation> so I can <outcome>. Entry points list
EVERY way the journey starts. P1 = must ship in the first slice. -->

### US-<n> — <title>
- **Story**: <As a … / When …>
- **Platforms**: <desktop | kiosk | both>
- **Entry points**: <button in <view>, menu <item>, ActionId::<id>, CLI
  `taskboard …`>
- **Priority**: <P1 | P2 | P3>
- **Acceptance**:
  - AC-1: Given <state> When <action> Then <typed outcome> — verified by
    <journey test | seam test | fold test | manual>.

## 4. Journey specs
<!-- One table per story. Every row needs an escape/failure path — a step
without one is a plan defect. Step IDs are greppable tokens; tests embed
them in snake_case (us1_s2_…). -->

### US-<n> — <story title>

| Step | Surface & affordance | User action | Dispatches | System response | Escape / failure |
|---|---|---|---|---|---|
| US-n/S1 | | | <StateCommand::… / UiRequest::… / ActionId::… / — display only> | | |
| US-n/S2 | | | | | |

## 5. Interaction flows
<!-- mermaid flowchart per multi-surface journey (entry points as sources,
step IDs on edges); stateDiagram-v2 per overlay/editor/modal. GitHub
renders these. -->

## 6. Wiring inventory
<!-- One row per NEW or CHANGED affordance. Row addressing: ItemAddr kind
for list rows (the UI is id-less; callbacks + action ids + command
variants are the contract). A row with no command variant must say why.
Test column: file::name embedding the step-ID token. -->

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Row addressing | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|---|
| W-1 | | | | | | | | | |

## 7. Command-contract delta
<!-- Every new/changed StateCommand, UiRequest, ActionId the journeys
imply — enumerated here BEFORE engine chunks are planned. -->

| Kind | Name | Payload sketch | Journeys |
|---|---|---|---|

## 8. Verification plan
- **Journey tests** (step-ID-named): <list>
- **Seam tests**: <exact-command assertions>
- **Fold/presentation tests**: <enum/flag assertions, never label text>
- **Kiosk gates**: <which affordances render disabled/absent>
- **Manual**: <what only a human judges, and the walkthrough covers it at
  review>

## 9. Definition of done
- [ ] `scripts/ux-coverage.sh <this file> --gate` exits 0
- [ ] Every P1 story's journey completes end-to-end including escapes
- [ ] Walkthrough protocol executed at stage 3; findings resolved or homed
- [ ] All matrix absences carry rulings; census updated
- [ ] Non-goals filed as backlog issues
```

## Appendix B — dispatch-plan.md amendment (diff-shaped)

- §1 chunks table gains a **UX coverage** column: `<US-n/S-k + W-x IDs this
  chunk implements>` (empty + "n/a — no surface delta" for engine-only
  chunks). The chunk set as a whole must cover every P1 step (P1-slice
  rule); a dispatch plan whose Origin includes a UX plan and whose chunks
  leave a P1 step uncovered is invalid.
- Shared execution model, stage 2 gains: "when the origin cites a UX
  journey plan: implement exactly the wiring inventory rows this chunk
  claims; embed journey-step tokens in test names; run
  `scripts/ux-coverage.sh <plan> --gate` before reporting done (MISSING on
  a claimed row is not done)."
- Shared execution model, stage 3 gains the **UX completeness mandate**:
  "when the origin cites a UX journey plan: re-run the coverage gate; then
  execute the walkthrough protocol (docs/ux_planning.org) over every P1
  story — the four walkthrough questions per step with evidence, then the
  six-heuristic pass over touched surfaces; BLOCKER/MAJOR findings are
  fixed on the branch, MINOR findings become backlog issues."
- Fill-in quality bar gains: "prompts name the UX journey plan's sections
  by ID (US/W/R) the same way they name origin sections by number."

## Appendix C — ux-coverage.sh sketch (for the implementing agent)

```text
inputs : <plan.md>; --gate for nonzero-on-missing
parse  : awk over pipe tables under fixed headings:
         "## 2." capability matrix + rulings, "## 4." journey step tables
         (headers contain "Step"), "## 6." wiring inventory,
         "## 7." command delta
checks : per wiring row — callback token present in
           crates/taskboard-ui-slint/ui/**.slint AND adapter.rs (handler
           or install_seam!); each command/ActionId token present in
           messages.rs / requests.rs / actions.rs
         per journey step — token us<n>_s<k> present in a #[test] fn name
           under crates/taskboard-ui-slint/tests/ (or the crate the plan's
           §8 nominates)
         per matrix cell — cited story's steps all covered; per ruling —
           reason non-empty
output : row → FOUND (file:line) | MISSING table; counts; --gate exit 1
         on any MISSING
header : states the honest limit — proves wiring + claimed tests exist,
         not UX quality; quality is the walkthrough protocol's job
tests  : fixture plan (PASS rows + deliberately MISSING rows) asserted by
         a smoke test in the phase-1 chunk
```

## Sources

- Jeff Patton, [*The New User Story Backlog is a Map*](https://jpattonassociates.com/the-new-backlog) and [story-mapping concepts (PDF)](https://www.jpattonassociates.com/wp-content/uploads/2015/03/story_mapping.pdf); *User Story Mapping* (O'Reilly, 2014).
- Alan Klement, job stories ("When ⟨situation⟩, I want to ⟨motivation⟩, so I can ⟨outcome⟩") — situational alternative to persona-based stories.
- Alistair Cockburn, *Writing Effective Use Cases* (2000) — main scenario + numbered extensions.
- Erik Mogensen, [Welcome to the World of Statecharts](https://statecharts.dev) and [Statecharts in User Interfaces](https://statecharts.dev/use-case-statecharts-in-user-interfaces.html); David Harel, "Statecharts: A Visual Formalism for Complex Systems" (1987); Ian Horrocks, *Constructing the User Interface with Statecharts* (1999).
- Nielsen Norman Group, [10 Usability Heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/); MeasuringU, [Heuristic Evaluation vs. Cognitive Walkthrough](https://measuringu.com/he-cw).
- LLM-driven usability inspection (precedent for the walkthrough protocol): [Synthetic Cognitive Walkthrough (arXiv 2512.03568)](https://arxiv.org/html/2512.03568v1); [UXAgent, Amazon Science 2025](https://www.amazon.science/publications/uxagent-an-llm-agent-based-usability-testing-framework-for-web-design).
- Gherkin/Given-When-Then discipline: [Cucumber documentation](https://cucumber.io/docs) (discipline adopted; tooling rejected).
