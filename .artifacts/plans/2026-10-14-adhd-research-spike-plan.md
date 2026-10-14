# [Plan] ADHD Design-Principles Research Spike (D6)

- **Date**: 2026-10-14
- **Kind**: stage-1 research plan — chunk 2 of the future-directions
  wave 0 dispatch (prompt 2.1). This commit is the plan only: no
  research executed, no report, no code.
- **Origin**: wave analysis
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.4 (the ADHD row — normative for the question), §4.1 decision D6
  (normative for scope and timing), §6 (Wave 0 row); GUI requirements
  brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md` (the
  surfaces the principles must map onto); dispatch plan
  `.artifacts/plans/2026-10-14-future-directions-wave0-dispatch-plan.md`
  §1 (shared execution model + evidence mandate, binding) and §3
  (chunk 2: deliverable, guardrails, acceptance criteria).
- **Branch / worktree**: `docs/adhd-research-spike`,
  `../taskboard-WT-docs-adhd-research-spike`, off `main` @ `3682da7`.
- **Goal**: a decision-grade research report that surveys the
  ADHD-oriented app space, grades the research basis behind the four
  factors (novelty, interest, challenge, urgency) and the adjacent
  literatures (task initiation, time perception, habit formation,
  gamification), and distils a **principles catalogue** whose every
  entry names a taskboard surface and a build direction — so the GUI
  design session (chunk 3, which consumes this report after merge)
  and the Wave-1/2 feature designs read a sourced lens, not
  listicle folklore.
- **Status**: READY for stage 2 — every open question resolved in §8.

---

## 1. Context & mandate (anchored; do not re-derive)

1. The wave analysis rates the item fit **open**, effort **S**: "a
   research report first (the brainstorm itself says so): survey the
   space, distil design principles. Afterwards it is a *lens* applied
   to features (focus mode, novelty surfacing, urgency games) rather
   than one feature. Cheap, high leverage, do early as input to
   GUI/feature design" (§3.4).
2. Decision D6 (§4.1) fixes scope and timing: run the spike *early*;
   its principles — novelty, interest, challenge, urgency; friction
   reduction; forgiveness in UX — inform the GUI and the focus/habit
   features **as they are designed**, not as a bolt-on module
   afterwards.
3. The Wave-0 row (§6) schedules the spike now precisely because the
   GUI design session is imminent; neither wave-0 spike touches
   product code.
4. The dispatch plan (§3) fixes the deliverable
   (`.artifacts/reports/<today>-adhd-support-research-report.md`), the
   expected survey content (body-doubling, task-chunking, timeboxing,
   novelty/urgency mechanics, friction reduction, forgiveness UX), the
   catalogue skeleton (each principle: stated, sourced, mapped to
   GUI-brief R-items / roadmap waves with a "what we would build"
   line), and the guardrails (tool-not-therapy; no medical or
   diagnostic claims; privacy note for future wellbeing data). Its
   acceptance criteria are binding: every principle actionable,
   citations resolve and support their claims, survey claims
   distinguish user reports from research findings, mappings
   reference real sections, house style, gates green.
5. The brainstorm's own framing (carried by D6): the ADHD-app space
   is crowded — survey it — and examine how the four factors "one
   psychologist named" can be *purposefully created*. Tracing that
   provenance and grading it is itself a research question (§2 RQ2,
   decision D4), not an assumption.
6. **Canonical mapping targets** (stage 2 maps principles onto
   exactly this vocabulary; stage 3 verifies the section numbers
   exist): wave analysis §3.1 (field family incl. importance/urgency
   and recurrence/habits rows), §3.2 (reminder flavours, N reminders
   per task, standalone timers), §3.3 (time tracking, break
   reminders, pomodoro focus mode, habit tracking), §3.4
   (review/journal, plan-next-day, decision support, day-start-time),
   §3.7 (today/organiser/overview views, inbox capture + triage,
   fuzzy search, matrix visualisation), §6 (waves 1–3 rows); GUI brief
   R1–R10 and its §3 feature-surface items (inbox capture, search
   palette, pomodoro overlay, reminder surfaces, review/journal
   views, kiosk switching, …). Expected high-traffic targets (mapping
   *expectations*, not pre-written catalogue entries): decision
   support (§3.4) is the natural home of the four factors; quick
   capture (§3.7, GUI §3) and the action layer (GUI R5) of friction
   reduction; the reminder flavours (§3.2) of urgency/persistence;
   pomodoro (§3.3) of challenge/timeboxing; forgiveness UX of the
   undo direction (§3.7, GUI §3 undo-tree view).
7. **Constraint frame for the transfer question (RQ3)**, from
   AGENTS.md and the accepted architecture: keyboard-first
   interaction (GUI brief R4/R5), privacy-respecting and
   telemetry-free (GUI brief R9's explicit note), offline-first /
   standalone-by-default (ADR 0010), low-power kiosk targets (glance,
   don't grab), and a dumb-presentation UI over a headless core (no
   business logic in `taskboard-ui-slint`).

## 2. Research questions

**RQ1 — the app space**: what does the ADHD-oriented app space ship
today, and what do users report actually helping?

- Which mechanics recur across products (the dispatch plan's expected
  families — body-doubling, task-chunking, timeboxing, novelty/
  urgency mechanics, friction reduction, forgiveness UX — are the
  starting hypothesis; the survey confirms, corrects, or extends the
  list)?
- Which of those are ADHD-specific design vs generic task management
  wearing ADHD branding?
- What do user communities praise and criticise, and what has failed
  (sunset products, abandoned mechanics) and why?
- *Done when*: every category in the §3.2 survey frame is covered,
  every recurring mechanic is attributed to the products shipping it
  and to labelled user-report evidence, and the mechanics inventory
  states which class each mechanic belongs to.

**RQ2 — the research basis**: what is the evidence base for the
principles the space implies?

- The four factors' provenance (expected to trace to the
  "interest-based nervous system" framing popularised by a
  practitioner columnist — verify, do not assume) and the adjacent
  peer-reviewed constructs (delay aversion, optimal stimulation,
  reward/novelty processing).
- Task initiation and initiation-friction reduction (incl.
  implementation intentions).
- Time perception in ADHD (the colloquial "time blindness" vs the
  duration-reproduction/reward-discounting findings).
- Habit formation (habit-formation trajectories, context cues) and
  gamification efficacy (meta-analytic verdicts, not marketing).
- *Done when*: each topic carries its strongest available evidence
  class (§3.1 ladder), and every load-bearing claim in the catalogue
  anchors to a source that actually states it — where the best
  available class is practitioner or user-report material, the report
  says so as a finding.

**RQ3 — transfer**: which principles transfer to a keyboard-first,
privacy-respecting, offline-first task manager with a
dumb-presentation GUI, onto which existing taskboard surfaces, and
which are rejected because they conflict with the constraint frame
(§1.7) or the guardrails (§5)?

- *Done when*: every transfer principle names ≥1 mapping target from
  §1.6 plus a build direction that names a surface; every rejected
  principle carries its reason.

## 3. Source strategy

### 3.1 Evidence ladder (tags every source, apps included)

- **E1** — peer-reviewed research (journal articles, meta-analyses,
  systematic reviews; open access preferred; DOI/PMC-stable links).
- **E2** — clinical/practitioner expert material (e.g. Barkley-class
  books and lectures, practitioner columns such as ADDitude's) —
  authoritative but not peer-reviewed; always labelled.
- **E3** — primary vendor material (app documentation, changelogs,
  feature/pricing pages).
- **E4** — community user reports (Reddit threads, Hacker News
  discussions, app-store review themes, named users' workflow
  write-ups).
- **E5** — aggregator/SEO listicles ("15 best ADHD apps …"):
  **leads to apps only, never cited as evidence.**

Standing rules: every citation records the URL, a one-line "what it
states", and the access date; a user-report claim and a research
finding never share a sentence without a label; where E2/E4 is the
strongest available basis, that fact is stated — a weak basis is a
recorded finding, not a failure (the chunk-1 precedent: absence is a
legitimate, well-evidenced result).

### 3.2 App-space survey frame (category sampling, ~15–25 apps)

Seven categories fix the coverage; within each, apps are picked for
category coverage + community prominence (E4 signal):

1. ADHD-marketed task managers / life organisers.
2. Mainstream task managers with ADHD-relevant modes (e.g. strategy
   presets, filters, timeboxing planners).
3. Focus and body-doubling tools (timers, focus sprints, live
   body-doubling sessions).
4. Habit trackers with streak/gamification mechanics (incl.
   open-source/privacy-oriented ones — the closest cousins to
   taskboard's values).
5. Time-perception tools (visual timers; persistent/insistent
   reminder apps).
6. AI-assisted breakdown and structuring tools (task-chunking
   assistants).
7. Coaching/therapy-adjacent apps — surveyed as the **guardrail
   contrast case**, never as a model.

Per app, the survey records: what it ships (E3), what users report
(E4), and any efficacy claim with its evidence class. Plus one
non-app lane: **keyboard-first prior art** — org-mode/GTD-with-ADHD
workflow write-ups, the community practice closest to taskboard's
interaction model.

### 3.3 Communities (public archives, read-only)

r/ADHD and adjacent app/tool recommendation threads, Hacker News
threads on ADHD tooling, app-store review themes for the surveyed
apps. Read-only (D10): no posting, no surveys, no DMs — archived
public discussion only.

### 3.4 Research literatures and stopping rule

Topics fixed by RQ2; discovery via Google Scholar / PubMed /
open-access repositories. Per topic: 1–2 reviews or meta-analyses
where they exist plus up to ~4 primary or authoritative sources;
**stop at saturation** — when new sources stop changing the picture
for the design question. This is a bounded narrative grading for a
design spike, explicitly not a systematic review (D3).

### 3.5 Weighting rules (primary over secondary, at every level)

Vendor docs over press coverage; the paper over a blog post about the
paper; a meta-analysis over a single study; user threads over
listicle summaries of user sentiment. Conflicts between evidence
classes are recorded, not averaged away.

## 4. Principles-catalogue format (the report's core artefact)

Each **transfer principle**:

- **ID & name** — P1…Pn, short, imperative-friendly.
- **Statement** — 1–2 imperative sentences, design-directed.
- **Source basis** — anchors with evidence class; user-report vs
  research labelled.
- **Taskboard mapping** — ≥1 target by section number from the §1.6
  vocabulary (wave analysis §3.x row and/or §6 wave; GUI brief R# or
  §3 item).
- **Build direction** — the "what we would build" line: concrete
  enough to name the surface and the rough mechanism (e.g. "quick
  capture as a global action in every context, riding GUI R5's
  action layer + §3.7's inbox capture"). A principle whose build
  direction cannot name a surface does not enter the catalogue
  unmapped (D6) — platitudes are stage-3 findings.

Plus a **rejected-principles sub-catalogue**: mechanics the space
ships that taskboard deliberately does not adopt, each with its
conflict (guardrail, privacy, offline-first, keyboard-first,
dumb-UI architecture) — the negative record so later agents do not
re-litigate (D7).

Expected size: ~10–15 transfer + ~5–8 rejected — an estimate, not a
quota (D5); the inclusion bar is sourcing + a named surface, not
count.

## 5. Guardrails (binding for the report's content and language)

1. **Tool, not therapy**: everything is framed as design principles
   for a tool; no treatment, management, or improvement-of-symptoms
   claims; category-7 apps exist in the survey as the contrast case
   that shows where the line is.
2. **No medical or diagnostic claims or framing**: nothing that
   assesses, scores, or implies assessing whether a user "has ADHD";
   principles are framed as universal-design wins with outsized
   value for ADHD users (curb-cut framing). Any principle that would
   require diagnostic data or framing is auto-rejected (D11).
3. **Privacy note for future wellbeing data**: journal entries,
   health flags, and ADHD-related usage patterns are
   privacy-sensitive; catalogue principles must not push features
   that flow such data to third parties (external AI breakdown would
   be opt-in-at-most, and only if ever built); a design note points
   at the D5 app-data substrate round (wave analysis §4.1 D5) and the
   GUI brief R9 no-telemetry stance.
4. **Language discipline**: no "suffers from", no cure verbs;
   "ADHD users" register; the four factors always cited with their
   provenance (D4); community terminology used respectfully.

## 6. Report outline (the stage-2 deliverable)

`.artifacts/reports/<stage-2-today>-adhd-support-research-report.md`,
house `[Report]` style:

1. **Executive summary** — headline findings; the shape of the
   catalogue (counts, strongest mappings).
2. **Method** — sources searched, ladder, search terms, access dates;
   reproducible per the evidence mandate.
3. **The ADHD-app space survey** — by category (§3.2 frame); the
   recurring-mechanics inventory; failures; all user-report evidence
   labelled.
4. **The research basis** — one subsection per RQ2 topic with an
   evidence-class verdict: what is solid, what is practitioner
   folklore, what is marketing.
5. **Principles catalogue** — §4 format, the core artefact.
6. **Rejected principles** — with reasons.
7. **Guardrails & privacy design notes** — §5, restated as
   consequences for taskboard.
8. **Open questions** — research-side only; product improvement
   ideas go to the potential-improvements backlog per its entry
   template (dispatch §3 scope rule), never into report scope.
9. **References** — URL + what-it-states + access date + evidence
   class; E5 listicles excluded by construction.

## 7. Out of scope (recorded, not silently dropped)

No code or product changes; no design decisions binding on Phase 7 —
the catalogue *informs* the GUI session and Wave-1/2 design rounds,
it commits nothing (D12); no systematic review (bounded narrative
grading, D3); no user studies, interviews, or community interaction
(read-only archives, D10); no feature commitments (build directions
are directions, not plans); no repo changes beyond the stage-2
report + backlog entries per template.

## 8. Decisions (D1–D12, resolved)

| # | Decision | Reason |
|---|---|---|
| D1 | Citation practice: web research via search/fetch tools; every citation = URL + one-line what-it-states + access date; paywalled papers cited from abstracts (no purchases); DOI/PMC-stable links preferred | Reproducibility for the evidence mandate; honesty about what was actually read |
| D2 | Survey shape: category-based sampling (7 categories, ~15–25 apps) chosen for coverage + prominence, not exhaustive enumeration | The space is crowded and churny; the *categories* are the durable artefact, membership is illustrative |
| D3 | Literature depth: bounded narrative grading with a saturation stopping rule per topic; explicitly not a systematic review | A design-principles spike, not a Cochrane review; the question is "strong enough basis to design by", not completeness |
| D4 | The four factors' provenance is a research question: trace the originating psychologist (expected: the "interest-based nervous system" framing via practitioner channels — E2), grade it against adjacent peer-reviewed constructs (delay aversion, optimal stimulation — E1), and present the factors at their true evidence level | The brainstorm itself says "one psychologist named" them; laundering practitioner folklore as findings would fail the acceptance criteria |
| D5 | Catalogue sizing is an estimate (~10–15 transfer, ~5–8 rejected), not a quota; inclusion gates on sourcing + named surface | Fewer well-sourced, well-mapped beats more thin; a count target would reward padding |
| D6 | Mapping discipline: a principle with no §1.6 mapping target is either rejected-with-reason or parked to the backlog; it cannot enter the catalogue unmapped | The dispatch's "platitudes are findings" acceptance, made structural |
| D7 | The rejected-principles sub-catalogue ships in the report | Records negative decisions with reasons so later agents do not re-litigate (the wave analysis's own §3.5-vehicle precedent) |
| D8 | Report path/name: `<stage-2-date>-adhd-support-research-report.md`, exactly the dispatch's deliverable name | Dispatch §3 is normative |
| D9 | Evidence ladder E1–E5 with the E5-never-cited rule; user-report vs research labelling is per-claim, structural | Satisfies the dispatch acceptance "survey claims distinguish user reports from research findings" by construction |
| D10 | Community protocol: read-only public archives; no posting, surveys, or DMs | Both the ethical posture for a tool project and the practical one for an agent session |
| D11 | Guardrail auto-rejection: any principle implying diagnosis, treatment claims, or third-party data flows is rejected on sight, citing the guardrail | Keeps §5 enforceable rather than aspirational |
| D12 | Deliverable boundary: the catalogue informs but binds nothing; Phase 7 scope decisions stay with the GUI design session | D6-of-the-wave-analysis's "input, not a feature" — an ADHD-*module* is exactly the bolt-on this spike exists to prevent |

## 9. Risks (source-strategy first)

- **SEO/listicle contamination** — the ADHD-app space is
  content-marketing-heavy: mitigated by the E5 rule (leads only) and
  E3-over-press weighting (§3.5).
- **Folklore laundering** — pop-psychology constructs presented as
  findings (the four factors themselves are the live example):
  mitigated by D4 + per-claim classes; the grading is a deliverable,
  not a footnote.
- **User-report/research conflation** — the dispatch's acceptance
  criterion: mitigated structurally by the per-claim label (D9).
- **Selection bias in community praise** — vocal happy users
  dominate: RQ1 explicitly asks for failures and criticism; E4
  evidence is labelled directional, never efficacy.
- **Citation rot and paywalls** — mitigated by stable links, access
  dates, and abstract-level honesty (D1); a dead link found by stage
  3 is a finding, so links are checked before handoff.
- **Mapping drift** — catalogue citations of wave-analysis/GUI-brief
  sections that do not exist: mitigated by pinning the canonical
  vocabulary (§1.6); stage 3 verifies the numbers.
- **Overrun blocking chunk 3** — the GUI design round gates on this
  report's merge: mitigated by the saturation rule (§3.4), the fixed
  report outline (§6), and the effort fence below; if a topic stalls,
  record the open question and move on — that is the §6.8 mechanism.
- **Scope creep into product design or therapy territory** —
  mitigated by §5 guardrails, D11, and §7's out-of-scope record.

## 10. Effort estimate

Wave analysis: **S**; dispatch chunk table: **S/M (research)**. This
plan estimates **2–3 working days** end to end: ~1.5–2 days research
(app space §3.2 + literatures §3.4), ~0.5–1 day report writing,
backlog entries included. Stage 2 stops at saturation, not at
exhaustion.

## 11. Gates & repo hygiene

Docs-only diff: this plan now; stage 2 adds the report + backlog
entries per template; stage 3 ships the PR. The full gate suite runs
per AGENTS §10 (docs-only PR; the pre-commit fmt/clippy hooks run on
commit regardless). No push, no PR from stages 1–2. Repo stays
byte-identical outside `.artifacts/` and the backlog file.
