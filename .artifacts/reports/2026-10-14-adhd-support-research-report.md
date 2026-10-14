# [Report] ADHD-Support Design Principles — Research Spike (D6)

- **Date**: 2026-10-14
- **Kind**: research report — chunk 2 of the future-directions wave 0
  dispatch (decision D6: run the ADHD design-principles spike early, as
  *input* to the GUI design session and the Wave-1/2 feature designs).
- **Origin**: stage-1 plan
  `.artifacts/plans/2026-10-14-adhd-research-spike-plan.md` (executed as
  written; deviations recorded in §10). Normative inputs: wave analysis
  `.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  §3.4/§4.1-D6/§6, GUI requirements brief
  `.artifacts/plans/2026-10-14-gui-design-requirements-plan.md`,
  dispatch plan §1/§3.
- **Deliverable summary**: a survey of the ADHD-oriented app space (~24
  products across 7 categories plus a keyboard-first/analog prior-art
  lane), a graded research basis for the constructs the space implies,
  and a **principles catalogue** — **14 transfer principles** (P1–P14)
  and **9 rejected mechanics** (X1–X9) — each transfer principle mapped
  to named taskboard surfaces (wave-analysis rows and GUI-brief items by
  section) with a build direction. This report *informs* design; it
  commits nothing (plan D12).
- **Reading guide for the GUI session (chunk 3)**: §5 is the catalogue
  to consume; P1, P2, P7, P9, P12 land inside Phase 7's foundation
  (R1/R2/R4/R5/R7 surfaces); the rest land with Wave-1/2/3 features.

---

## 1. Executive summary

1. **The four factors are a practitioner heuristic, partially confirmed
   by research — and that is good enough to design by.** "Novelty,
   interest, challenge, urgency" traces verbatim to Dr. William Dodson's
   ADDitude columns based on his clinical observations, carrying no
   research citations (E2) [30][31]. The peer-reviewed neighbours are
   real: urgency ↔ delay aversion [1] and steeper delay discounting
   (d = 0.43) [2]; novelty ↔ atypical novelty-as-reward processing [6];
   interest ↔ dopamine reward-pathway motivation deficits [5]; the
   general stimulation-seeking premise ↔ the optimal-stimulation model
   [4]. The "challenge" leg has the thinnest support. Design
   consequence: use the factors as a lens, cite the constructs, never
   as clinical claims (§7 guardrails).
2. **The app space's best ideas are already on taskboard's roadmap.**
   The mechanics ADHD users repeatedly credit — instant capture,
   visual/externalised time, timeboxing, insistent-but-controllable
   reminders, visible progress, forgiveness on missed days — map onto
   existing roadmap items (inbox capture §3.7, today view §3.7,
   scheduled field §3.1, reminder flavours §3.2, review/journal §3.4,
   undo §3.7) rather than demanding new subsystems. The catalogue
   sharpens *how* to build what is already planned.
3. **The space's worst ideas are cautionary tales with evidence.**
   Punitive streaks produce documented shame-abandonment spirals (user
   reports [77]–[79] + the self-forgiveness literature [27]);
   full gamification economies show small/mixed effects and
   counterproductive-reward complaints ([21][23]); subscription-priced
   "ADHD content" apps churn on billing practices and finite content
   ([38]–[41], [67]–[68]); unconstrained flexibility becomes its own
   failure mode ([44], [69], [84], [91]).
4. **Strongest mappings** (detail in §5): quick capture → GUI §3 inbox
   capture riding R5's action layer (P1); externalised time → §3.3
   pomodoro/time tracking + §3.7 today view + GUI §3 overlay (P2);
   honest urgency → §3.2 reminder flavours + §3.1 urgency field + §3.4
   decision support (P5); forgiveness UX → §3.7 undo tree + §3.3 habits
   (P9); constrained defaults → R5 keymap presets + R2 theming
   discipline (P7); kiosk visibility → §3.7 kiosk switching/overview
   (P10).
5. **Body doubling** — the space's signature "ADHD-specific" mechanic —
   rests on one peer-reviewed *survey* (no efficacy trial) plus expert
   opinion and anecdote [29][36]; it is also architecturally off-limits
   for an offline-first, telemetry-free tool (X1). Honesty about that
   evidence class is itself a deliverable of this spike.

## 2. Method

Per the stage-1 plan §3 (binding). Reproducibility notes for every
claim follow.

- **Lanes.** Six parallel research lanes, executed 2026-10-14 via
  web-search and page-fetch tooling: (1) ADHD-marketed organisers +
  mainstream managers with ADHD-relevant modes; (2) focus/body-doubling
  + time-perception tools; (3) habit trackers/gamification + AI
  breakdown tools; (4) core ADHD constructs (four-factor provenance,
  delay aversion, optimal stimulation, discounting, EF/time); (5)
  task-initiation/time/habit/goal literatures; (6) keyboard-first and
  analog prior art (org-mode, GTD, bullet journal, CLI tools, HN).
- **Evidence ladder** (tags every source): **E1** peer-reviewed;
  **E2** clinical/practitioner expert; **E3** primary vendor material;
  **E4** community user reports; **E5** SEO listicles — used as leads
  to discover apps only, **never cited as evidence** (no E5 source
  appears in §9).
- **Citation contract.** Every reference records URL, one-line
  what-it-states, access date (2026-10-14 for all), and evidence class.
  Reddit blocks direct fetching, so community claims are anchored to
  search-result snippets or thread titles and labelled "(snippet)" —
  they are directional user-report evidence, never efficacy. Paywalled
  papers are cited from abstract records (PubMed/PMC/Semantic
  Scholar/Crossref) and say so.
- **Spot-checks.** The seven most load-bearing citations were
  independently re-verified by the lead after the lanes returned
  ([1][2][9][13][21][23][30] — verbatim quotes re-read from the page or
  its DOI-matched record). One correction resulted: the delay-discounting
  meta-analysis publishes in *Biological Psychiatry: CNNI*, not the
  journal the plan's draft expected.
- **Stopping rule.** Saturation per topic (plan §3.4): when new
  sources stopped changing the design picture. Lane budgets capped at
  ~12–15 sources; reached without exhausting any topic's central
  claims. This is a bounded narrative grading for a design spike,
  explicitly **not** a systematic review (plan D3).
- **Community protocol.** Read-only public archives; no posting, no
  surveys, no DMs (plan D10).
- **Search terms** (representative): "ADHD task manager app review
  reddit", "Amazing Marvin strategies braindump", "body doubling ADHD
  study", "virtual coworking ADHD research", "time perception ADHD
  meta-analysis", "delay discounting ADHD meta-analysis", "Dodson
  interest-based nervous system four factors", "implementation
  intentions ADHD", "habit formation 66 days Lally", "gamification
  meta-analysis", "streak shame ADHD reddit", "Goblin Tools ADHD",
  "org-mode ADHD", "bullet journal migration ADHD", "Taskwarrior ADHD".

## 3. The ADHD-app space survey

### 3.1 Category 1 — ADHD-marketed task managers / life organisers

| App | What it ships [E3] | User-report signal [E4] | ADHD-specific or generic? |
|---|---|---|---|
| Inflow [38] | Clinician-created psychoeducation program + coaching + community; no public pricing (quiz-gated) | Price debate at ~$199/yr [67]; sentiment split "genuinely helpful" vs "replicable free" (snippet-level) | ADHD-specific *content*, zero task mechanics — a course, not a tool |
| Shimmer [39] | Human 1:1 ADHD coaching, daily body doubling, coach chat — "$240 every 4 weeks" | Positive but price-sensitive (snippet-level) | A human service with an app scaffold |
| Numo [40] | Learn content, gamified "Brain" points, routines, Squad community, Brain Dump → tasks; $15.99/mo | In-store review balks at the subscription [40] | Generic gamified productivity in a thick ADHD skin; Brain Dump is the one ADHD-shaped mechanic |
| Sensa [41] | One generic CBT wellness funnel; ADHD is one checkbox among anxiety/stress/burnout | "Horror stories" refund-refusal thread [68] | White-label CBT course wearing ADHD branding — the clearest contrast case |
| Routinery [42] | Step-timed routine timers, streaks, reminders | (thin this session) | Generic routine mechanics; ADHD is marketing garnish |
| Unstuck [43] | Goal → "simple next steps" breakdown for task paralysis (listing snippet) | (thin this session) | ADHD-*intent* (paralysis → next step); mechanics ungraded for thin evidence |

**Category verdict**: the ADHD-branded products sell *content and
humans* at therapy-adjacent prices; the durable mechanics they contain
(psychoeducation, next-step decomposition, brain dump) are the
transferable parts. The billing/dark-pattern failure cluster lives here
(§3.8).

### 3.2 Category 2 — mainstream task managers with ADHD-relevant modes

| App | ADHD-relevant surface [E3] | Verdict |
|---|---|---|
| Amazing Marvin [69] | Library of switchable *strategies*: Braindump, Eat the Frog, Time Blocking, Tomato Tracker, Auto-schedule | Generic-configurable core; the composable strategy library is the best ADHD metacognitive scaffold in the space — and its own tinkering trap |
| Todoist [44] | Official ADHD guide telling users to *subtract*: "Endless projects and labels can overcomplicate things and cause way too much wasted time." | Generic tool + vendor advice that ADHD-mode = less structure |
| TickTick [45] | Built-in pomodoro; tasks/habits/calendar | Generic all-in-one; user theme: "allows too much freedom" (snippet) |
| Sunsama [46] | Daily planning + shutdown rituals, timeboxing, workload caps | Generic professional discipline — ADHD-adjacent by *constraint*, zero ADHD marketing |
| Motion [47] | AI auto-schedules and reprioritises | Generic AI scheduler — outsources planning EF |
| Structured [48] | Visual day timeline unifying tasks/calendar/focus | Generic visual planner that accidentally externalises time perception |

**Category verdict**: the mainstream winners for ADHD users are
*constrained* by design or by advised discipline — constraint, not
feature count, is the ADHD-relevant variable. This anchors P7.

### 3.3 Category 3 — focus & body-doubling tools

- **Focusmate** [49]: 1:1 video coworking sessions (25/50/75 min),
  greet → goals → silent work → wrap. User reports: "life changer" /
  "game changer" threads [70][71] (snippet-level), with a visible
  skeptic minority [76].
- **Flow Club** [50]: group sessions, host ritual ("what are you
  working on?"), self-reports 62% of 245 respondents identify as ADHD —
  vendor self-survey, not research.
- **Pomodoro** (official Cirillo material [51]): a full loop — plan
  (inventory, estimates) → focus → record/reflect; official tone:
  "Be gentle with yourself." **Pomofocus** [52] adds per-task estimates
  and reports; **Focus To-Do** [53] fuses pomodoro with task lists.
- **Forest** [54]: gamified focus — "Open another app, and the tree
  dies"; group stakes. Documented bypass: users plant fake trees and
  delete dead ones [74].
- User reports on pomodoro itself are **split**: fixed breaks disrupt
  hyperfocus ("the breaks are too disruptive to my workflow" [75];
  timers "associated with a lot of anxiety and self-shame" [90]).

### 3.4 Category 4 — habit trackers & gamification

- **Habitica** [57]: RPG economy (XP/gold/HP); missed Dailies damage the
  avatar; streak bonuses; *no purchasable streak freeze* — repair is via
  class skills, a manual "Adjust Streak", and the Orb of Rebirth reset.
- **Streaks** [59]: hard line — "Don't break the chain, or your streak
  will reset to zero days."
- **Loop Habit Tracker** [58]: FOSS, GPLv3 — "All data provided to Loop
  Habit Tracker is only stored locally in your device"; no streak-shame
  framing; frequently the recommended replacement in shame threads
  (snippet-level).
- **Beeminder** [60]: money-backed commitment contracts — "If you cross
  the line, we charge your payment method!" Evidence for the *mechanism*
  exists (smoking-cessation deposit-contract RCT [24]), not for the
  product; its own community flags selection bias [82].
- **Finch** [61]: self-care pet with no penalties — the low-guilt
  pole of the category.
- Research verdict on the category: gamification effects are real but
  **small and domain-dependent** (§4.5), and reward design quality
  matters more than reward quantity [23].

### 3.5 Category 5 — time-perception tools

- **Time Timer** [55]: the canonical visual timer — a shrinking red
  disk that "shows" the passage of time; 30+ years, explicitly
  positioned for ADHD/autism transitions. Two small peer-reviewed
  studies support the family: a multimodal time-skills + assistive-device
  RCT in ADHD children [14], and a within-subjects study where a visible
  timer reduced anticipatory anxiety and off-task behaviour with *huge*
  individual usage differences and no performance change [15].
- **Due** [56]: the insistence mechanic — "Due repeatedly notifies you
  of overdue reminders until you mark them complete, reschedule them,
  or turn off their auto snooze" (1/5/10/15/30-min re-nags). User
  reports: "The auto-snooze function … is a godsend" [72]; peers
  described as "the equivalent of nagging" [73] — by design.

### 3.6 Category 6 — AI-assisted breakdown

- **Goblin Tools** [62]: "small, simple tools, for when things feel too
  big or complicated" — Magic ToDo with adjustable breakdown granularity,
  plus braindump→actions ("Compiler"), estimator, and one-task-at-a-time
  focus mode. ADHD-community reception is warm but polarised on
  granularity ("helpful for my ADHD self" [80] vs "too dumb for Goblin
  Tools" — snippet). No peer-reviewed evaluation exists.
- **Numo's** AI breakdown rides inside the subscription app above [40].

### 3.7 Category 7 — coaching/therapy-adjacent (the guardrail contrast case)

Inflow, Sensa, and Shimmer (§3.1) are this category: psychoeducation
courses, CBT-flavoured programs, human coaching. They exist in this
report **only** to mark where the tool-not-therapy line sits (§7): their
value proposition is clinical content and humans; taskboard's is a
durable tool. Nothing from this category transfers except the negative
lessons (subscription churn, finite content, billing dark patterns
[67][68]).

### 3.8 Prior-art lane — keyboard-first, plain-text, analog

- **org-mode** (ADHD users, HN [84]): credited mechanisms — instant
  capture into a daily file; one integrated system; agenda views as
  visibility. Failure reports: refiling friction and multi-machine drift
  kill the habit; "the customizability lets me never be satisfied and
  spend more time screwing with stuff than actually working."
- **GTD** (official [63]): step one is offloading — "Write, record, or
  gather any and everything that has your attention into a collection
  tool." The Weekly Review is the cadence anchor.
- **Bullet Journal** (official [64][65][66]): created by Ryder Carroll
  out of his own ADHD; Rapid Logging = capture at thought speed;
  Migration = *intentional* rewrite friction — "The purpose of Migration
  is to surface what's worth the effort … and to separate the signal
  from the noise." ADHD reception split: the migration review is "where
  some of the magic … lies" for some [88], unsustainable for others
  [89].
- **Taskwarrior** (ADHD users, HN [86]): "as I've ADHD, I lost track of
  my task if it's not easily visible"; "I lose track of the entire task
  tracking system if it doesn't remind me of its existence."
- **The long-run endpoint**: "I tried every todo app and ended up with a
  .txt file" [91] — durable systems converge on plain, visible, local.

### 3.9 Recurring-mechanics inventory (the durable artefact)

| Mechanic | Ships in | Class | Evidence strength |
|---|---|---|---|
| Instant/low-friction capture | GTD, BuJo, org-mode, Marvin Braindump, Numo | universal, outsized ADHD value | E3+E4 strong, no RCT |
| Visual/externalised time | Time Timer, Structured, pomodoro timers | near-universal | E1 small studies [14][15] + E2/E3/E4 |
| Timeboxing / day-shaping | Sunsama, Marvin, TickTick | universal | E3+E4 (urgency/delay-aversion E1 backs proximity) |
| Insistent re-reminding | Due and peers | ADHD-shaped | E3+E4; grounded in delay discounting/aversion E1 [1][2] |
| Task breakdown / next-step | GTD, Goblin Tools, Unstuck, Marvin | ADHD-marketed, universally useful | E3+E4; goal-specificity E1 [28] |
| Streaks (punitive) | Streaks, Habitica | ADHD-market norm | **negative** evidence [27][77]–[79] |
| Streaks (flexible/grace) | Finch, Loop, Habitica's soft repairs | corrective trend | E4 + habit-formation E1 [16] |
| Gamified economies | Habitica, Forest, Numo | ADHD-marketed | E1 small/mixed [21][22][23] |
| Body doubling (live) | Focusmate, Flow Club, Shimmer | ADHD-specific | thin: 1 survey E1 [29] + E2/E4 |
| Commitment contracts | Beeminder (+stickK lineage) | niche | mechanism E1 [24], product none |
| Progress statistics | Pomofocus, TickTick, Loop | universal | E1 small overall effects [21] + E3/E4 |
| Brain dump → triage | GTD inbox, Marvin, Numo, BuJo migration | universal, outsized ADHD value | E3+E4 |
| AI breakdown | Goblin Tools, Numo | new, unproven | E3+E4 only |
| Psychoeducation content | Inflow, Sensa, Numo | therapy-adjacent | out of scope (guardrail) |

### 3.10 Failures and churn modes (with evidence)

1. **Streak shame → abandonment** [77][78][79] — the single
   best-attested failure of the space.
2. **Subscription fatigue & billing dark patterns** [39][40][67][68] —
   the ADHD-branded segment prices at coaching rates and churns on
   refunds.
3. **App-hopping / novelty decay** — "in a couple months my brain wants
   novelty and I start wanting to switch to a new app" [92]; the
   setup becomes the hyperfixation [84][91][93].
4. **Configuration burden** — unconstrained flexibility becomes the new
   failure [44][69][84].
5. **Sync/visibility loss kills systems** — multi-device drift and
   invisible tools get abandoned [84][86].
6. **Rigid timers harm hyperfocus** [75][90]; gamified focus gets
   gamed [74].
7. **Vendor sunset/consolidation** even at FDA-cleared scale (Akili's
   exit [96]) — durability of the *vendor* is a user-side risk a
   local-first, plain-data tool answers structurally.

## 4. The research basis (graded per topic)

### 4.1 The four factors: provenance and partial confirmation

**Provenance (verified)**: Dodson's ADDitude column names the routes
into "the zone" verbatim — interest ("get in the zone by being
interested in"), challenge/competition, novelty ("Novelty is
short-lived, though"), urgency ("when the task is urgent — a do-or-die
deadline") [30]; a companion column lists "Interest, Competition,
Novelty, Urgency" [31]. The columns cite no studies — they are a
clinician's decade of practice observations, editorially
"medically reviewed" but carrying ADDitude's own no-medical-advice
disclaimer. **Grade: E2.** The ICNU/INCUP/PINCH acronyms are later
coaching-community packaging, not Dodson's wording [32] (E4).

**Peer-reviewed neighbours (E1), leg by leg**:

- **Urgency** — delay aversion: ADHD framed as "a motivational style
  characterised by attempts to escape or avoid delay" arising from
  reward-circuitry differences [1]; and steeper delay discounting of
  delayed rewards, d = 0.43 across 21 case-control studies (N = 3,913),
  robust across age and reward type [2], with real-world corroboration
  (discounting ↔ adverse financial outcomes) [3]. **Strongest leg.**
- **Novelty** — a double-blind fMRI/RL study found off-medication ADHD
  participants showed impaired reward learning *plus* a distinct
  "novelty bonus" (novelty processed as reward-like value), normalised
  by stimulants [6]. **Strong.**
- **Interest** — PET evidence of reduced dopamine D2/D3 receptor and
  transporter availability in nucleus accumbens associated with
  motivation deficits in unmedicated ADHD adults [5]. **Moderate**
  (motivation construct, not "interest" verbatim).
- **Challenge** — no fetched E1 source isolates challenge/competition
  as an ADHD-specific motivator. **Thin — recorded as a gap** (§8).
- **General premise** ("stimulation-seeking brains") — the 1983
  optimal-stimulation model: disordered activity as self-generated
  stimulation compensating under-arousal [4]. **Classical theory**
  (record-only access this session).

**Design consequence**: purposefully *creating* near-term structure
(urgency), surfacing varied/interesting work (novelty/interest) is
evidence-aligned; "challenge" claims should stay qualitative.

### 4.2 Time perception ("time blindness")

"Time blindness" is vocabulary, not a construct: Cleveland Clinic —
"Time blindness isn't a diagnosis or a specific symptom" [35]; Barkley's
own writing prefers the temporal-myopia metaphor ("to time what
nearsightedness is to spatial vision") and prescribes "making time
itself more externally represented" [33] (E2, on top of his E1
EF model [7]). The measurable finding is robust: ADHD shows duration
discrimination/reproduction deficits on all tasks tested [11],
meta-analytically across tasks and modalities [12], and in the largest
synthesis to date 824 effect sizes with mean g = 0.688, moderated by age
and working memory [13]. Assistive-device evidence is small-N but
directionally supportive [14][15]. **Design consequence**: externalise
time everywhere; never require an internal clock (P2).

### 4.3 Task initiation & implementation intentions

If-then planning ("when cue X, I do Y") has a medium-to-large average
effect on goal attainment, d = 0.65 over 94 studies [8][9]. In children
with ADHD, if-then plans improved response inhibition "to the same
level observed in children without ADHD", best when combined with
medication [10] (abstract-level; paediatric samples — adult trials
absent, §8). **Design consequence**: tasks should carry concrete
when/where anchors and the tool should speak in cues (P11).

### 4.4 Habit formation (and ADHD)

Automaticity develops asymptotically — median 66 days to 95% of
plateau, range 18–254 — and crucially "missing one opportunity to
perform the behaviour did not materially affect the habit formation
process" [16]; the 21-day folklore is refuted by the 2024 synthesis
[17]. Habits are context-response learning — "perception of contexts
triggers the associated response without a mediating goal" [18].
ADHD-specific picture: procedural sequence learning appears *preserved*
[19]; the link between ADHD symptoms and habitual-control balance is
"only modest" [20] — the machinery works; initiation and reinforcement
are the struggle. **Design consequence**: cue-anchored repetition,
grace for missed days, no streak cliff (P9, P11).

### 4.5 Gamification & commitment devices

- Meta-analytically (learning contexts): "significant small effects"
  — cognitive g = .49, motivational g = .36, behavioural g = .25; only
  the cognitive effect is stable across the high-rigour subsplit, and
  the element-level moderators that mattered were game fiction and
  competition-combined-with-collaboration (behavioural outcomes) [21].
  mHealth reviews: mixed, modest, and mostly without follow-up
  durability data [22].
- Reward *design* dominates reward volume: in an optimised-gamification
  model, points for enacting a behaviour should be largest when the
  habit is weak and shrink toward zero as it strengthens (penalties
  conversely grow) — aligning immediate incentives with long-run
  goals [23]; the same paper documents that "only 49% of Habitica's
  users rate its rewards as (rather) appropriate".
- Deposit/commitment contracts have real RCT evidence for behaviour
  change (smoking cessation, +3pp pass rates persisting 12 months [24])
  — but for self-selected samples and nothing app-specific.

**Design consequence**: adopt the *cheap* winners (visible progress,
  completion feedback; badges/graphs at most) and reject economies (P8,
  X4).

### 4.6 Body doubling

Exactly one peer-reviewed study surfaced: a self-report survey (N = 220)
of how/when/why neurodivergent people body-double — definitional and
descriptive, "using the presence of others to start, stay focused on, or
accomplish a task"; no efficacy trial exists [29]. Expert explainers
(external executive functioning [36]) and vendor self-surveys fill the
rest; community enthusiasm is strong but split [70][71][76]. **Grade:
thin E1 + strong E2/E4 — recorded as a finding** (the chunk-1
"absence is a legitimate result" precedent).

### 4.7 Adjacent design-relevant findings

- **Fresh-start effect**: goal-directed activity spikes after temporal
  landmarks (new year/month/week, birthdays) across three field
  studies [25]. → reviews at day/week boundaries (P13).
- **Planning fallacy**: "Fewer than one third of the respondents
  (29.7%) finished in the time they reported as their most accurate
  prediction" — despite prior experience [26]. → estimate-vs-actual
  feedback (P14).
- **Self-forgiveness**: forgiving oneself for procrastinating predicts
  *less* future procrastination, mediated by reduced negative affect
  [27]. → forgiveness UX (P9).
- **Goal-setting**: "specific, difficult goals consistently led to
  higher performance than urging people to do their best" [28]. →
  concrete next-step framing (P3).

## 5. Principles catalogue (the core artefact)

Format per stage-1 plan §4: statement / source basis / taskboard
mapping (wave-analysis row + GUI-brief item, by section) / build
direction. Wave tags (W1/W2/W3) follow wave-analysis §6. Every mapping
target was verified to exist in the named sections.

### P1 — Capture at the speed of thought

- **Statement**: any thought must be capturable in seconds, from any
  context, with a title alone — structure is optional and deferred.
- **Source basis**: GTD's capture step [63] (E3); bullet-journal Rapid
  Logging [64] (E3); org-mode ADHD users credit instant capture above
  all [84] (E4); Marvin's Braindump strategy [69] (E3). The initiation
  literature adds the flip side: floating intentions underperform
  cue-anchored ones (d = 0.65) [8][9] (E1).
- **Taskboard mapping**: wave analysis §3.7 "Inbox capture + triage
  interface" (W1); GUI brief §3 "Inbox capture + triage interface"
  ("global hotkey → small input, no navigation"); GUI R5 (the capture
  hotkey is an action available in every context — the brief says so
  verbatim).
- **Build direction**: quick capture as a global action in every
  context riding R5's action layer, writing to the §3.7 inbox stack
  (designated stack + capture verb, core-side cost "tiny" per the wave
  analysis); the capture surface accepts one line, no required field,
  triage happens later (P13). Also surfaced in the REPL/CLI as the
  cheapest first landing.

### P2 — Externalise time; never require an internal clock

- **Statement**: every time-relevant surface shows time as visible,
  spatial structure — countdowns, elapsed displays, day shapes —
  instead of implying the user will "just sense" durations.
- **Source basis**: time-perception deficits meta-analytic (g = 0.688)
  [13], all-tasks discrimination thresholds [11], task-modality meta
  [12] (E1); Barkley's prescription to make "time itself more
  externally represented" [33] (E2); visual-timer studies [14][15]
  (E1, small-N); Time Timer's 30-year product practice [55] (E3).
- **Taskboard mapping**: wave analysis §3.3 "Pomodoro focus mode +
  concentration overlay" and "Time tracking for tasks" (W2); §3.7
  "Today view" (W1) and "Kiosk kanban ⇄ upcoming-list switching"
  (W1+); GUI brief §3 "Pomodoro focus overlay" and "Today view".
- **Build direction**: the focus overlay's countdown is the primary
  display; task cards in the today view show time-of-day shape and
  time-remaining decorations; tracked-time surfaces (W2) show elapsed
  time persistently; kiosk rotation surfaces what is due *now vs next*.

### P3 — Make the next concrete step the unit of work

- **Statement**: surface "the next action", not the project; splitting
  a task into steps must be a cheap, first-class verb.
- **Source basis**: specific goals outperform vague ones [28] (E1);
  GTD's next-action discipline [63] (E3); Goblin Tools' whole value
  proposition [62] (E3) with warm ADHD reception [80] (E4);
  paralysis→next-step is Unstuck's explicit pitch [43] (E3).
- **Taskboard mapping**: wave analysis §3.1 "Subtasks … + split-into-
  subtasks" (W3, design round exists in backlog); §3.4 "Decision
  support ('what next?')" (W1); GUI brief §3 "Importance–urgency
  matrix visualisation + task-list decorations" (decorations belong to
  the task-card primitive R1).
- **Build direction**: the split command (already a W3 design-round
  item) should be reachable in ≤2 actions from any task surface and
  create untitled-optional steps; decision support (§3.4) always
  presents *one* suggested next task, not a ranked wall; task-card
  primitive (R1) carries a "next step exists" decoration pre-subtask —
  the description's first checklist/next-action line can serve until
  subtasks land.

### P4 — Box time deliberately

- **Statement**: bounded work windows are first-class objects; the day
  is planned as blocks with starts, not a list with deadlines only.
- **Source basis**: timeboxing as shipped practice in Sunsama/Marvin/
  TickTick [46][69][45] (E3); pomodoro's official method is a planning
  loop with estimates [51] (E3); deadline proximity motivating via
  delay aversion/discounting [1][2] (E1) — a start time is a
  self-created near-term deadline.
- **Taskboard mapping**: wave analysis §3.1 "Split deadline vs
  scheduled start" (W1 — "the single highest-value" prerequisite of the
  today view); §3.7 "Today view" (W1) and "Organiser view" (W3);
  §3.4 "Plan-for-next-day function" (W2).
- **Build direction**: ship the scheduled-start field on time (W1) and
  make the today view render scheduled blocks as a day shape (P2);
  plan-next-day (W2) closes each day by placing tomorrow's boxes.

### P5 — Manufacture honest urgency

- **Statement**: since delayed consequences don't motivate (they are
  steeply discounted), the tool must make real deadlines feel present —
  visible countdowns, escalating-but-consented reminders, imminent
  due-states — without ever faking scarcity.
- **Source basis**: Dodson's urgency factor [30] (E2) grounded in delay
  aversion [1] and discounting d = 0.43 [2] (E1); Due's auto-snooze
  insistence praised as "a godsend" [56][72] (E3+E4); "nagging by
  design" [73] (E4).
- **Taskboard mapping**: wave analysis §3.2 "Two reminder flavours
  (notification vs insistent alarm)" and "Multiple reminders per task"
  (W1, backlog entries exist); §3.1 "Importance–urgency matrix" (W1
  fields); §3.4 "Decision support" (W1); GUI brief §3 "Reminder
  surfaces" (overdue banner, toasts with snooze/mute, escalation
  overlay).
- **Build direction**: insistence is chosen per task (P12 pairs this
  with consent); due-countdown and time-remaining decorations on list
  rows and the matrix (§3.7 decorations); decision support can weight
  imminence — that is the four factors' home (plan §1.6 expected it).
  Honest means: real due dates and real reminder rules, escalated in
  volume/channel only — no fake streaks-at-risk, no manufactured FOMO
  (those are X8).

### P6 — Feed the need for novelty through content, not churn

- **Statement**: novelty-seeking is real and decays predictably
  ("Novelty is short-lived" [30]); the tool stays stable and boring
  while offering sanctioned, harmless novelty valves — view rotation,
  theming, varied review prompts — so the answer to boredom is never
  "switch apps".
- **Source basis**: novelty processed as reward-like value [6] (E1);
  optimal-stimulation model [4] (E1); Dodson's novelty factor and its
  short life [30] (E2); the app-hopping failure mode is the direct
  evidence that undirected novelty seeking destroys systems [92][93][94]
  (E4).
- **Taskboard mapping**: wave analysis §3.7 "Kiosk kanban ⇄
  upcoming-list switching" (W1+ — rotation policy already an app-side
  config item); §3.4 "review … custom prompt templates" (W2 — varied
  prompts); GUI brief R2 ("theme development as a powerful tool" — the
  brief's own words) and R8/R9 as the presentation-side valves.
- **Build direction**: kiosk rotation cycles views on a policy;
  end-of-day review draws from user-authored prompt templates (W2);
  R2's theming makes restyling the sanctioned tinkering surface —
  tokens change, structure doesn't. Explicitly *not* shipped:
  engagement-bait novelty mechanics (X8), feature churn as retention.

### P7 — Constrain by default; treat configuration as a hazard

- **Statement**: ship opinionated defaults and curated presets; deep
  configuration exists but is deliberately incremental, because
  unconstrained flexibility becomes the new failure mode (the system
  becomes the hyperfixation).
- **Source basis**: Todoist's own ADHD guide — "Endless projects and
  labels can overcomplicate things and cause way too much wasted
  time" [44] (E3); TickTick users self-reporting "too much freedom"
  [69-context, snippet] (E4); Marvin's tinkering trap [69] and
  org-mode's "customizability lets me never be satisfied" [84] (E4);
  GTD's subtractive advice [63] (E3).
- **Taskboard mapping**: GUI brief R5 ("default presets:
  standard/emacs/vi" — already the design); R2 (text-role vocabulary
  "inventing roles per view is how this decays" — the brief's warning);
  wave analysis §3.7 "Filter view … Saved-filter queries" (W3 — ship
  curated examples, not a blank predicate editor).
- **Build direction**: preset-first everywhere the brief already plans
  it (keymaps, themes); saved views start from a small set of good
  defaults (Today, Due, Inbox); no user-facing workflow automation
  surface whose setup cost rivals the work it saves. The R7 heuristic
  "interaction-count-per-task-frequency" is the audit that polices this.

### P8 — Make completion feel like progress

- **Statement**: completing a task gives immediate, satisfying
  feedback and feeds visible accumulation (counts, history, streaks of
  *done*, not of unbroken chains).
- **Source basis**: gamification's effects are real but small, the
  cognitive/achievement effect the most stable [21] (E1); reward design
  should fit the actor — adaptive points shrink as habits strengthen
  [23] (E1); user praise centres on visible progress (Pomofocus/Loop
  reports [52][58]) (E3+E4).
- **Taskboard mapping**: GUI brief R1 (task-card primitive — the
  completion affordance is a component-library decision) and §3
  "task-list decorations"; wave analysis §3.3 "Time tracking … export"
  statistics (W2); §3.4 "Past-entries view + export + statistics view"
  (W2); §3.7 "Desktop/tablet overview" (W2).
- **Build direction**: a deliberately satisfying completion interaction
  on the task-card primitive (motion/sound optional, themeable via
  R2); "done today" counts on today/overview surfaces; review
  statistics chart completed work over time. No XP, currency, or
  avatar economies (X4) — accumulation is real work, made visible.

### P9 — Design for the missed day (forgiveness as a mechanism)

- **Statement**: missing is normal data, not failure — every surface
  that can be broken by a missed day offers graceful recovery instead:
  reschedule flows without guilt copy, streaks with grace or consistency
  percentages, undo everywhere.
- **Source basis**: "Missing one opportunity … did not materially
  affect the habit formation process" [16] (E1); self-forgiveness
  reduces future procrastination [27] (E1); the streak-shame spiral is
  the space's best-attested failure [77][78][79] (E4); bullet-journal
  Migration is the analog precedent — intentional, forgiving review
  friction [65] (E3); Cirillo's "Be gentle with yourself" [51] (E3).
- **Taskboard mapping**: wave analysis §3.7 "Full undo system (leaning
  undo tree)" (W3, backlog entry exists — "confirmation-free
  destructive verbs — already the CLI's philosophy") and GUI brief §3
  "Undo tree view" + the action layer reserving undo/redo as
  first-class global actions; §3.3 "Habit progress tracking" (W3,
  recurrence design round D3); §3.4 "End-of-day review" (W2).
- **Build direction**: overdue triage is a reschedule flow with neutral
  copy ("move to today?", never "you failed"); habit streaks (when W3
  lands) compute consistency or offer grace days — never a hard zero
  reset; undo is global and always available (the GUI brief already
  reserves the actions); review prompts frame misses as data for
  planning, not verdicts.

### P10 — One system, always visible

- **Statement**: a system the user doesn't see stops existing — the
  tool must be glanceably present (kiosk, today view, persistent
  surfaces) and reachable in one action from anywhere.
- **Source basis**: "I lose track of the entire task tracking system
  if it doesn't remind me of its existence" [86] (E4); org-mode agenda
  views credited as the visibility mechanism [84] (E4); the project's
  own kiosk identity (AGENTS §1: passive, glanceable displays) is the
  architectural expression.
- **Taskboard mapping**: wave analysis §3.7 "Kiosk kanban ⇄
  upcoming-list switching" (W1+) and "Desktop/tablet overview" (W2);
  §3.7 "Today view" (W1 — "the single highest-value desktop view");
  GUI brief §3 "Kiosk mode" and "Today view"; GUI R8 (discoverability —
  what's possible is always one key away).
- **Build direction**: today view as the default desktop surface; the
  kiosk as the ambient presence the architecture was built for
  (low-power, offline-resilient — a visibility feature no
  cloud-subscription app can match); the search palette and command
  layer keep the whole system one action away (R5/R8, §3.7 fuzzy
  search W1).

### P11 — Anchor work to concrete cues

- **Statement**: tasks carry when/where anchors ("at 9:00, after
  breakfast, at the desk") rather than floating intentions; reminders
  and routines fire on those anchors.
- **Source basis**: implementation intentions d = 0.65 [8][9], with
  ADHD-child evidence restoring control-level inhibition [10] (E1);
  Lally recommends *situations* as cues [16] and context-dependent
  repetition is the mechanism [18] (E1).
- **Taskboard mapping**: wave analysis §3.1 "Split deadline vs
  scheduled start" (W1) and "Repeatable tasks/habits" (W3 recurrence
  design round, backlog entry exists); §3.2 "Multiple reminders per
  task" (W1); §3.4 "Plan-for-next-day function" (W2) and "Day-start-time
  setting" (W1 — the day boundary is itself the master cue).
- **Build direction**: scheduled-start + reminder-rule pairing is the
  if-then structure (the domain already models N rules per task); the
  W3 habit design round should model an explicit cue field (time,
  event, or context) rather than only a period; plan-next-day's output
  is tomorrow's cue set.

### P12 — Respect attention: gentle by default, insistent by consent

- **Statement**: notifications and timers inform without assaulting —
  insistence is a per-task choice, focus sessions are interruptible,
  and nothing in the UI manufactures anxiety to compel action.
- **Source basis**: pomodoro abandonment when breaks disrupt hyperfocus
  [75] and timer anxiety/shame reports [90] (E4); timer usage varies
  hugely between individuals (25% monitored a visible timer heavily,
  others barely [15]) (E1); the kiosk's "glance, don't grab" identity
  (AGENTS §1); Duolingo-style re-engagement nagging is the space's
  anti-example (categorised under X8).
- **Taskboard mapping**: wave analysis §3.2 "Two reminder flavours
  (notification vs insistent alarm)" (W1) — the flavours are exactly
  this principle's data model; §3.3 "Pomodoro focus mode +
  concentration overlay" (W2) and GUI brief §3 "Pomodoro focus overlay"
  ("one window, few exits" — and the exits must be obvious and cheap);
  GUI R7 (accessibility/usability checks).
- **Build direction**: every reminder rule carries a flavour;
  escalation happens only in the insistent flavour and is visible and
  revocable (snooze/mute commands already exist in the domain); the
  focus overlay has a permanent, unobscured exit and configurable or
  disabled break enforcement; urgency colour language in R2's theme
  tokens is calibrated, not alarming.

### P13 — Review at temporal landmarks; make re-commitment cheap

- **Statement**: planning and review happen at natural boundaries —
  day close, week start — where motivation measurably peaks, and their
  core interaction is triage-and-forgive, not audit.
- **Source basis**: the fresh-start effect — goal activity spikes
  after temporal landmarks across three field studies [25] (E1); GTD's
  Weekly Review cadence [63] (E3); bullet-journal Migration as the
  analog review ritual [65] (E3); BuJo community: migration is "where
  some of the magic … lies" [88] (E4).
- **Taskboard mapping**: wave analysis §3.4 "End-of-day review /
  reflection" (W2), "Plan-for-next-day function" (W2), and "Day-start-
  time setting" (W1 — night-owl 05:00 rollover makes the boundary
  honest); §3.7 "Inbox capture + triage interface" (W1 — triage is
  migration); GUI brief §3 "End-of-day review / journal / statistics
  views" (W2).
- **Build direction**: land the day-boundary preference *before* its
  consumers (the wave analysis already sequences it into W1 for this
  reason); the end-of-day review opens at the day boundary and walks:
  inbox triage (P1) → misses rescheduled without ceremony (P9) →
  tomorrow's boxes placed (P4); weekly and monthly anchors are natural
  fresh-start hooks for the statistics views.

### P14 — Estimate with data, not hope

- **Statement**: effort estimates are captured at planning time and
  confronted with actuals at review time, so the user's own history —
  not optimism — calibrates the next plan.
- **Source basis**: the planning fallacy — fewer than a third of
  students finished within their own "most accurate" prediction,
  despite prior experience [26] (E1); pomodoro's official loop already
  prescribes record-and-compare-estimates [51] (E3).
- **Taskboard mapping**: wave analysis §3.1 "Effort estimation field"
  (W1); §3.3 "Time tracking for tasks + good export" (W2); §3.4
  "Decision support" (effort-fit term of the scoring function) and
  "Past-entries view + statistics" (W2).
- **Build direction**: the effort field (W1) stores the estimate;
  time tracking (W2) supplies actuals; review/statistics surfaces show
  estimate vs actual per task and aggregate. The deeper standing
  reference-class feedback loop (suggesting estimates from history at
  capture time) is out of report scope — parked as a backlog entry
  (§11 bookkeeping).

## 6. Rejected principles (the negative record)

Each entry records the mechanic, why the space ships it, and the
specific conflict with taskboard's constraint frame (keyboard-first,
privacy-respecting/telemetry-free, offline-first, low-power kiosk,
dumb-presentation UI) or the §7 guardrails — so later agents do not
re-litigate (plan D7).

### X1 — Live body-doubling sessions

Focusmate/Flow Club mechanics (video presence, session booking) [49][50].
**Rejected because**: requires a always-on network service and camera
presence (conflicts offline-first, AGENTS §1/ADR 0010; privacy frame);
evidence is thin (§4.6); the dumb-UI/headless-core architecture gains
nothing from hosting presence sessions. A *local* focus-session runtime
(§3.3 pomodoro, W2) captures the cue-and-ritual part without the
service.

### X2 — Real-money commitment contracts

Beeminder-style stakes ("If you cross the line, we charge your payment
method!" [60]). **Rejected because**: financial stakes plus a
third-party payment integration violate the offline-first/privacy frame
and skirt the tool-not-therapy line (financial penalties for symptom-
adjacent behaviour is coaching territory). The mechanism's RCT evidence
[24] is real but for clinical behaviour change in opt-in samples, not
task tools.

### X3 — Punitive streaks and loss mechanics

Hard zero-resets ("your streak will reset to zero days" [59]) and
avatar damage for missed dailies [57]. **Rejected because**: the
best-attested failure of the space (shame → abandonment [77][78][79],
E4) directly contradicted by the habit-formation and self-forgiveness
evidence [16][27] (E1). Superseded by P9's grace mechanics.

### X4 — Full gamification economies (XP, currency, avatars, pets)

Habitica/Forest/Numo-class systems [54][57][40]. **Rejected because**:
meta-analytic effects are small and mixed [21][22]; reward
appropriateness is the documented weak point (49% Habitica stat [23]);
economy content is endless UI surface at odds with the low-power kiosk
and the component-library discipline (R1); gamified focus demonstrably
gets gamed [74]. Progress visibility (P8) keeps the evidence-backed
subset.

### X5 — ADHD scoring, symptom tracking, or diagnostic personalisation

**Auto-rejected per plan D11** (guardrail 2): anything that assesses,
scores, or implies assessing whether a user "has ADHD" — including
"ADHD mode" toggles framed clinically and symptom severity logging.
Taskboard ships universal-design wins with outsized ADHD value
(curb-cut framing), never assessment.

### X6 — Therapeutic content modules (CBT courses, psychoeducation)

The Inflow/Sensa model [38][41]. **Rejected because**: tool-not-therapy
(guardrail 1); finite content is a churn mode (§3.10); category 7
exists in this report precisely as the contrast case showing where the
line sits.

### X7 — Cloud-AI task breakdown by default

Goblin-Tools-style magic decomposition is genuinely valued [62][80] —
but routing user task content to a third-party model by default
violates the telemetry-free stance (GUI brief R9's explicit note) and
offline-first. **Rejected as a default**; an opt-in, user-configured
local-or-explicitly-consented path could exist someday (privacy note
§7.3). Manual cheap splitting (P3) is the in-scope answer.

### X8 — Engagement-loop push notifications and re-engagement nags

Duolingo-class retention mechanics. **Rejected because**: the kiosk and
desktop product are glance tools, not attention merchants (AGENTS §1);
manufactured anxiety contradicts P12 and the honesty requirement of
P5; re-engagement pressure is a documented annoyance driver in the
surveyed space.

### X9 — Usage-telemetry-driven adaptive UI

Reordering/hiding features from usage patterns. **Rejected for now
because**: the GUI brief R9 itself parks it — usage telemetry is a
prerequisite and a privacy decision the project has declined to make
(R9's no-telemetry note); ADHD-relevant adaptivity from local data
would still need the D5 substrate and a design round. Revisit only
behind an explicit, local-only data policy.

## 7. Guardrails & privacy design notes (binding restatement)

1. **Tool, not therapy.** The catalogue's language and mechanics are
   tool design. Nothing here treats, manages, or improves symptoms; no
   outcome claims attach to any feature. Category-7 apps (§3.7) mark
   the contrast. The report deliberately cites clinical research only
   as *design-relevant findings about constructs*, never as claims
   about taskboard's effects on users.
2. **No medical or diagnostic framing.** No scoring, screening,
   "ADHD-mode" clinical toggles, or symptom language in product copy
   (X5 is the auto-rejection rule). Principles are framed as universal
   design with outsized ADHD value. Community terms ("time blindness")
   appear in this report with their evidence grade ([35]: "isn't a
   diagnosis"); product UI should prefer plain language ("shows time
   remaining").
3. **Privacy note for future wellbeing data.** Journal entries (§3.4,
   W2), health flags, and ADHD-adjacent usage patterns are
   privacy-sensitive: they stay device-local by default; catalogue
   principles must not push third-party flows (X7 is the enforced
   instance). When such features sync, they ride the D5 app-data
   substrate decision (wave analysis §4.1 D5) with an explicit
   user-facing choice; the GUI brief R9 no-telemetry stance is the
   standing rule. Loop Habit Tracker's posture ("only stored locally
   in your device" [58]) is the category's trust anchor — taskboard's
   local-first architecture is structurally there already.
4. **Language discipline.** "ADHD users" register (no
   "suffers-from"/cure verbs); the four factors always cited with
   provenance (E2, Dodson) when named in future design docs; community
   terminology used respectfully.

## 8. Open questions (research-side only)

1. **The "challenge" factor** has no fetched E1 anchor — is
   competition/challenge an independently evidenced ADHD motivator, or
   interest-by-another-name? (Design proceeds without leaning on it.)
2. **Adult implementation-intention trials**: the ADHD evidence for
   if-then planning is paediatric [10]; adult trials did not surface.
3. **Body doubling efficacy**: no controlled trial exists; a future
   trial could change X1's evidence line (not its architectural one).
4. **Time-assistive tools in adults**: both timer studies are in
   children [14][15]; adult kiosk/desktop evidence is absent.
5. **Adaptive reward decay outside the lab**: Lieder's shrinking-points
   principle [23] is model+proof-of-concept; no product-scale
   validation surfaced. P8 takes only the direction, not the maths.
6. **Glance-display effects on task awareness**: no research at all
   surfaced on ambient/kiosk task displays — taskboard's kiosk bet is
   architecturally motivated, not literature-backed. A future
   self-observation (via the R6 harness + user reports) is the only
   feasible evidence path.
7. **Prior-art lane evidence class**: org-mode/GTD/BuJo ADHD claims
   rest on E3/E4 (well-replicated user reports, no trials) — stated so
   wherever cited.

## 9. References

All accessed 2026-10-14. Classes: **E1** peer-reviewed, **E2**
practitioner/clinical expert, **E3** primary vendor, **E4** community
user report. "(snippet)" = search-result snippet or thread title;
Reddit blocks direct fetches. Paywalled E1 items are cited from
abstract records as noted.

**E1 — peer-reviewed**

1. [E1] https://pubmed.ncbi.nlm.nih.gov/14624804/ — Sonuga-Barke 2003,
   *Neurosci Biobehav Rev* 27(7):593–604, dual-pathway model — states
   ADHD involves a second, dissociable pathway of *delay aversion*
   ("a motivational style characterised by attempts to escape or avoid
   delay" from reward-circuitry disturbances). (PubMed record; abstract
   via record page.)
2. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC5049699/ — Jackson &
   MacKillop 2016, *Biol Psychiatry CNNI* 1(4):316–325 — meta-analysis,
   21 studies / N = 3,913: ADHD discounts delayed rewards more steeply,
   "d=0.43; p < 10−15", robust across age/reward type/comorbidity.
3. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC5421775/ — Beauchaine
   et al. 2017 — ADHD symptoms associate with greater delay discounting
   and adverse real-world financial outcomes.
4. [E1] https://pubmed.ncbi.nlm.nih.gov/6657825/ — Zentall & Zentall
   1983, *Psychol Bull* 94(3):446–471 — the optimal-stimulation model:
   disordered activity as self-generated stimulation compensating
   under-arousal. (Record only; no abstract available.)
5. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC3010326/ — Volkow et
   al. 2011, *Mol Psychiatry* 16(11):1147–1154 — PET: reduced dopamine
   D2/D3 receptor/DAT availability in nucleus accumbens associated
   with motivation deficits in unmedicated ADHD adults.
6. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC5917772/ — Sethi et
   al. 2018, *Brain* 141(5):1545–1557 — RL+fMRI double-blind: ADHD
   shows impaired reward learning plus an aberrant novelty bonus,
   normalised by stimulants.
7. [E1] https://pubmed.ncbi.nlm.nih.gov/9000892/ — Barkley 1997,
   *Psychol Bull* 121(1):65–94 — behavioural inhibition linked to four
   executive functions oriented to self-regulation toward future
   goals. (Record.)
8. [E1] https://www.sciencedirect.com/science/chapter/bookseries/pii/S0065260106380021 —
   Gollwitzer & Sheeran 2006, *Adv Exp Soc Psychol* 38:69–119 —
   meta-analysis of implementation intentions on goal achievement
   (abstract via chapter page; stat anchored in [9]).
9. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC4500900 — Wieber,
   Thürmer & Gollwitzer 2015, *Front Hum Neurosci* — review stating
   "implementation intentions have a medium-to-large average effect on
   goal attainment (d= 0.65)" over 94 studies.
10. [E1] https://link.springer.com/article/10.1007/s10608-007-9150-1 —
    Gawrilow & Gollwitzer, *Cognit Ther Res* 32(2):261–280 — if-then
    plans improved Go/No-Go inhibition in children with ADHD "to the
    same level observed in children without ADHD". (Abstract.)
11. [E1] https://pubmed.ncbi.nlm.nih.gov/16195956/ — Toplak & Tannock
    2005, *J Abnorm Child Psychol* 33(5):639–654 — ADHD participants
    had higher thresholds "on all of the duration discrimination
    tasks". (Abstract.)
12. [E1] https://pubmed.ncbi.nlm.nih.gov/32757699/ — Nejati & Yazdani
    2020, *Child Neuropsychol* — meta-analysis: "Children with ADHD
    have difficulties in time perception (TP)". (Abstract.)
13. [E1] https://pubmed.ncbi.nlm.nih.gov/38145491/ — Metcalfe, McFeats
    & Voyer 2024, *Dev Neuropsychol* — systematic review/meta-analysis,
    824 effect sizes, "mean g of 0.688" for time-perception deficits in
    ADHD across the lifespan. (Abstract; record re-verified via
    Semantic Scholar DOI match.)
14. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC5852175 — Wennberg et
    al. 2017 — RCT (N = 38, ADHD children): time-skills training +
    time-assistive devices (incl. visual timers) improved
    time-processing ability (p = 0.019), not time management.
15. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC12731990 — Hallez
    2025 — within-subjects (N = 44 children): a visible Time Timer
    reduced anticipatory anxiety (d = 0.42) and inattentive behaviour
    with no performance change; usage frequency varied hugely.
16. [E1] https://doi.org/10.1002/ejsp.674 — Lally, van Jaarsveld,
    Potts & Wardle 2010, *Eur J Soc Psychol* 40:998–1009 — "The median
    time to reach 95% of asymptote was 66 days, with a range from 18 to
    254 days"; "Missing one opportunity … did not materially affect
    the habit formation process". (Full text read via an open mirror
    PDF; DOI is the stable citation.)
17. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC11641623 — Singh et
    al. 2024, *Healthcare* 12(23):2488 — systematic review/meta of
    health-behaviour habit formation; refutes the 21-day folklore.
18. [E1] https://doi.org/10.1037/0033-295X.114.4.843 — Wood & Neal
    2007, *Psychol Rev* 114(4):843–863 — "Once a habit is formed,
    perception of contexts triggers the associated response without a
    mediating goal". (Full text read via the author's USC PDF; DOI is
    the stable citation.)
19. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC7655644 — Sanjeevan,
    Cardy & Anagnostou 2020, *Front Psychol* — meta-analysis: procedural
    sequence learning "appears to be preserved in ADHD".
20. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC6733985 — Ceceli,
    Esposito & Tricomi 2019, *Front Psychol* — "only a modest link
    between ADHD symptomology and motivational control" of habits.
21. [E1] https://link.springer.com/article/10.1007/s10648-019-09498-w —
    Sailer & Homner 2020, *Educ Psychol Rev* 32:77–112 — meta-analysis:
    "significant small effects of gamification on cognitive
    (g = .49), motivational (g = .36) …" and behavioural (g = .25)
    outcomes; the cognitive effect alone is stable in the high-rigour
    subsplit; "inclusion of game fiction and social interaction were
    significant moderators of the effect … on behavioral learning
    outcomes". (Abstract.)
22. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC8855282 — Xu et al.
    2022 — systematic review (50 studies) of mHealth gamification for
    physical activity: "results were mixed, and modest changes were
    attained"; 76% of studies had no follow-up.
23. [E1] https://pmc.ncbi.nlm.nih.gov/articles/PMC10998180 — Lieder et
    al. 2024, *JMIR Serious Games* — optimised gamification: points
    for enacting a behaviour should shrink as the habit strengthens;
    cites "only 49% of Habitica's users rate its rewards as (rather)
    appropriate".
24. [E1] https://www.aeaweb.org/articles?id=10.1257/app.2.4.213 —
    Giné, Karlan & Zinman 2010, *AEJ: Applied* — smoking-cessation
    deposit-contract RCT: offered CARES smokers 3pp more likely to pass
    the 6-month test; effects persisted 12 months. (Abstract.)
25. [E1] https://doi.org/10.1287/mnsc.2014.1901 — Dai, Milkman & Riis
    2014, *Management Science* 60(10):2563–2582 — "Across three
    archival field studies, we provide evidence of a 'fresh start
    effect'". (Publisher page bot-walled; DOI Crossref-verified,
    snippet-anchored.)
26. [E1] https://bear.warrington.ufl.edu/brenner/mar7588/Papers/buehler-et-al-1994.pdf —
    Buehler, Griffin & Ross 1994, *JPSP* 67(3):366–381 — planning
    fallacy: "Fewer than one third of the respondents (29.7%) finished
    in the time they reported as their most accurate prediction."
    (Author-hosted full text; DOI 10.1037/0022-3514.67.3.366.)
27. [E1] https://www.sciencedirect.com/science/article/pii/S0191886910000474 —
    Wohl, Pychyl & Bennett 2010, *Pers Individ Dif* 48(7):803–808 —
    self-forgiveness for procrastinating reduces future procrastination
    (N = 119), mediated by reduced negative affect. (Abstract page;
    snippet-anchored.)
28. [E1] https://med.stanford.edu/content/dam/sm/s-spire/documents/PD.locke-and-latham-retrospective_Paper.pdf —
    Locke & Latham 2002, *Am Psychol* 57(9):705–717 — "specific,
    difficult goals consistently led to higher performance than urging
    people to do their best". (Author-hosted full text; DOI
    10.1037/0003-066X.57.9.705.)
29. [E1] https://dl.acm.org/doi/full/10.1145/3689648 — Eagle,
    Baltaxe-Admony & Ringland 2024, *ACM TACCESS* — self-report survey
    (N = 220) defining body doubling as "using the presence of others
    to start, stay focused on, or accomplish a task"; descriptive, no
    efficacy trial. (Publisher page Cloudflare-blocked; abstract via
    Semantic Scholar record; DOI verified.)

**E2 — practitioner/clinical expert**

30. [E2] https://www.additudemag.com/secrets-of-the-adhd-brain/ —
    Dodson, "Secrets of Your ADHD Brain" — names the four routes
    (interest, challenge/competition, novelty "short-lived", urgency);
    based on his clinical caseload, no studies cited; "Medically
    reviewed" label with no-medical-advice disclaimer.
31. [E2] https://www.additudemag.com/adhd-brain-chemistry-video/ —
    ADDitude companion column listing "Interest, Competition, Novelty,
    Urgency"; educational-use disclaimer.
32. [E4] https://neurodivergentinsights.com/interest-based-nervous-system/ —
    Dr. Megan Anna Neff (psychologist blog) attributing the ICNU
    packaging to Dodson and flagging it "comes from his clinical
    practice rather than research studies". (Placed here as
    provenance-grading material.)
33. [E2] https://www.russellbarkley.org/factsheets/ADHD_EF_and_SR.pdf —
    Barkley factsheet: ADHD as a self-regulation disorder; EF deficits
    "are to time what nearsightedness is to spatial vision"; prescribes
    making time externally represented; the phrase "time blindness"
    does not appear in the document.
34. [E2] https://www.additudemag.com/webinar/adhd-time-blindness-prioritization-teens/ —
    ADDitude webinar page attributing the time-blindness definition to
    Barkley ("the impaired ability to sense time passing…").
35. [E2] https://health.clevelandclinic.org/time-blindness — Cleveland
    Clinic: "Time blindness isn't a diagnosis or a specific symptom."
36. [E2] https://health.clevelandclinic.org/body-doubling-for-adhd —
    Cleveland Clinic body-doubling explainer ("external executive
    functioning"); cites no studies.
37. [E2] https://add.org/adhd-time-blindness — ADHD.org guidance
    recommending visual timers that "show how much time has elapsed".
    (Snippet.)

**E3 — primary vendor**

38. [E3] https://www.getinflow.io — Inflow: clinician-created ADHD
    program app; science-based framing; no public pricing.
39. [E3] https://www.shimmer.care/cost — Shimmer: 1:1 coaching
    membership, "just $240 every 4 weeks"; daily body doubling
    included.
40. [E3] https://apps.apple.com/gb/app/numo-adhd-focus-wellbeing/id1628994767 —
    Numo App Store listing: Learn content, gamified Brain, routines,
    Squad community, Brain Dump; "auto-renewing monthly subscription at
    $15.99/month"; a visible review balks at the subscription.
41. [E3] https://sensa.health — Sensa: generic CBT wellness app, ADHD
    one checkbox among many; terms at
    https://sensa.health/terms-of-services make cancellation
    self-serve.
42. [E3] https://www.routinery.app — Routinery: routine timer/streak
    app, "recognized for supporting ADHD users". (Snippet.)
43. [E3] https://play.google.com/store/apps/details?id=com.chuiyanqi.adhd —
    Unstuck: ADHD "Next Step Coach", big goals → simple next steps.
    (Snippet.)
44. [E3] https://www.todoist.com/inspiration/managing-adhd-todoist —
    Todoist's official ADHD guide: "Endless projects and labels can
    overcomplicate things and cause way too much wasted time."
    (Snippet.)
45. [E3] https://help.ticktick.com/articles/7055782025193586688 —
    TickTick pomodoro doc: "divide time into several 'Pomodoro
    timers'" with 25/5 defaults.
46. [E3] https://www.sunsama.com (+ /pricing) — Sunsama: daily
    planning/shutdown rituals, "Visualize and block time for work on
    your calendar"; $17/mo yearly.
47. [E3] https://www.usemotion.com/pricing — Motion: "Automatically
    prioritize tasks, schedule meetings, and resolve calendar
    conflicts." (Snippet.)
48. [E3] https://structured.app — Structured: tasks + calendar +
    focus sessions in one visual timeline. (Snippet.)
49. [E3] https://www.focusmate.com/ — Focusmate: 25/50/75-min 1:1
    video coworking; body doubling definition quoted.
50. [E3] https://www.flow.club/coworking-for-adhd — Flow Club ADHD
    page: host ritual quoted; "62% of 245 respondents identify as
    living with ADHD" (vendor self-survey).
51. [E3] https://www.pomodorotechnique.com — Cirillo's official site:
    pomodoro as plan→focus→record loop; "Be gentle with yourself."
52. [E3] https://pomofocus.io/ — Pomofocus: free pomodoro timer with
    per-task estimates and reports.
53. [E3] https://www.focustodo.cn/ — Focus To-Do: pomodoro fused with
    task management, per-project statistics.
54. [E3] https://www.forestapp.cc/ — Forest: "Open another app, and
    the tree dies"; Deep Focus blocking; group stakes.
55. [E3] https://www.timetimer.com/ — Time Timer: patented shrinking
    red-disk visual timer; 4M+ sold; ADHD/autism positioning.
56. [E3] https://www.dueapp.com/ — Due: "repeatedly notifies you of
    overdue reminders until you mark them complete, reschedule them, or
    turn off their auto snooze".
57. [E3] https://habitica.fandom.com/wiki/Streaks — Habitica wiki:
    streak bonuses, day-roll resets, class-skill streak protection,
    manual Adjust Streak, Orb of Rebirth; no purchasable freeze.
58. [E3] https://loophabits.org/privacy.html (+ https://loophabits.org/) —
    Loop Habit Tracker: "All data provided to Loop Habit Tracker is
    only stored locally in your device"; GPLv3, no ads/tracking.
59. [E3] https://streaks.app/ — Streaks: "Don't break the chain, or
    your streak will reset to zero days."
60. [E3] https://www.beeminder.com/ — Beeminder: "If you cross the
    line, we charge your payment method!"; escalating pledges.
61. [E3] https://finchcare.com/ — Finch: "take care of your pet by
    taking care of yourself!"; no penalties marketed.
62. [E3] https://goblin.tools — Goblin Tools: "small, simple tools,
    for when things feel too big or complicated"; Magic ToDo
    adjustable-granularity breakdown, Compiler, Estimator, Taskmaster.
63. [E3] https://gettingthingsdone.com/what-is-gtd/ — official GTD
    method: "Write, record, or gather any and everything that has your
    attention into a collection tool."
64. [E3] https://bulletjournal.com/blogs/faq/what-is-rapid-logging-understand-rapid-logging-bullets-and-signifiers —
    Rapid Logging: "quickly jotting down information in a concise and
    structured manner".
65. [E3] https://bulletjournal.com/pages/migration — Migration:
    rewrite friction is intentional — "surface what's worth the effort
    … separate the signal from the noise".
66. [E3] https://bulletjournal.com/pages/rydercarroll — Carroll bio:
    Bujo created out of his own ADHD ("I struggled tremendously with
    distraction, procrastination, hyperactivity, overwhelm").

**E4 — community user reports** (all snippet- or title-anchored unless
noted; directional evidence, not efficacy)

67. [E4] https://www.reddit.com/r/ADHD/comments/1kxea3v/ — "Inflow app
    is the $199 per year subscription cost…" (thread title).
68. [E4] https://www.reddit.com/r/ConsumerAdvice/comments/1d6y4ns/ —
    Sensa: "stating no refund, they will cancel, but refuse to refund
    the amount".
69. [E4] https://www.reddit.com/r/amazingmarvin/comments/17nvo4k/ —
    Marvin tips: "Get everything in your brain into Marvin, break it
    down into actionable tasks…"
70. [E4] https://www.reddit.com/r/ADHD/comments/18xnpkq/ — "Body
    doubling is a game changer!"
71. [E4] https://www.reddit.com/r/adhdwomen/comments/zy5dh5/ —
    "FocusMate is a life changer for me!" (title).
72. [E4] https://www.reddit.com/r/ADHD/comments/r2zi3r/ — 500-day app
    review: Due's "auto-snooze function … is a godsend".
73. [E4] https://www.reddit.com/r/iphone/comments/1b6an6d/ — Duo/
    Alarmed: "the auto snooze feature is the equivalent of nagging."
74. [E4] https://www.reddit.com/r/forestapp/comments/lp0lc1/ — "Do you
    cheat?" — planting trees without being productive.
75. [E4] https://www.reddit.com/r/adhdwomen/comments/12p9qva/ —
    pomodoro: "the breaks are too disruptive to my workflow".
76. [E4] https://www.reddit.com/r/AdultADHDSupportGroup/comments/1sfnqbz/ —
    "does [virtual body doubling] actually work?" (title).
77. [E4] https://www.reddit.com/r/ADHD/comments/1qyhu7w/ — "Streaks
    destroyed me…": "Streak: 0 days. My brain: 'See? You're a
    failure.'"
78. [E4] https://www.reddit.com/r/getdisciplined/comments/1twjjxh/ —
    "I have ADHD. I've read Atomic Habits twice. I always started
    strong. Then missed a day."
79. [E4] https://www.reddit.com/r/getdisciplined/comments/1swhkpu/ —
    streak apps' "fatal flaw: they punish you for missing a day."
80. [E4] https://www.reddit.com/r/adhdwomen/comments/13oreld/ — Goblin
    Tools: "helpful for my ADHD self".
81. [E4] https://www.reddit.com/r/ADHD/comments/1hbjq1c/ — "I've tried
    50+ productivity apps…" (title).
82. [E4] https://forum.beeminder.com/t/12257 — community thread on the
    science of Beeminder; selection-bias caveat. (Snippet.)
83. [E3] https://flown.com/blog/adhd/body-doubling-adhd-study — Flown
    vendor self-survey of 117 adults. (Snippet; vendor data, not
    research.)
84. [E4] https://news.ycombinator.com/item?id=28704636 — HN: ADHD +
    org-mode — capture, agenda, refiling friction, tinkering trap
    (fetched thread).
85. [E4] https://www.reddit.com/r/orgmode/comments/1414ha1/ — org-mode
    + ADHD question thread. (Snippet.)
86. [E4] https://news.ycombinator.com/item?id=41372482 — HN:
    Taskwarrior + ADHD — "I lose track of the entire task tracking
    system if it doesn't remind me of its existence" (fetched thread).
87. [E4] https://www.reddit.com/r/linux/comments/52xdco/ — Taskwarrior
    recommendation, ADHD-in-mind alternative mentioned. (Snippet.)
88. [E4] https://www.reddit.com/r/ADHD/comments/wessj8/ — BuJo +
    ADHD: migration is "where some of the magic … lies". (Snippet.)
89. [E4] https://www.reddit.com/r/bujo/comments/yuiune/ — "I've tried
    bujo before but failed." (Snippet.)
90. [E4] https://news.ycombinator.com/item?id=35471129 — HN pomodoro
    app thread: timers "associated with a lot of anxiety and
    self-shame" (fetched thread).
91. [E4] https://news.ycombinator.com/item?id=44864134 — HN: "I tried
    every todo app and ended up with a .txt file" — app-hopping as
    procrastination (fetched thread).
92. [E4] https://www.reddit.com/r/AutisticWithADHD/comments/1b6ugk3/ —
    "in a couple months my brain wants novelty and I start wanting to
    switch to a new app." (Snippet.)
93. [E4] https://www.reddit.com/r/ProductivityApps/comments/1icpdre/ —
    the going-all-in-then-abandoning pattern. (Snippet.)
94. [E4] https://www.reddit.com/r/ProductivityApps/comments/1shui5d/ —
    "The productivity app market has a dirty secret about ADHD"
    (title).
95. [E4] https://news.ycombinator.com/item?id=38274782 — HN "Hacking
    ADHD" discussion (fetched thread; meds/body doubling/hardware, not
    apps).
96. [E4/press] https://www.biopharmadive.com/news/akili-sell-34m-virtual-therapeutics-digital/717576 —
    Akili (FDA-cleared ADHD game) wound down/sold for ~$34M. (Snippet.)

## 10. Deviations from the stage-1 plan

1. **None in scope or structure.** The report follows the plan's §6
   outline and §4 catalogue format exactly; guardrails §5 are restated
   in §7; research questions RQ1–RQ3 are answered in §3/§4/§5.
2. **Corrections made during execution** (recorded per the plan's
   honesty rules): (a) the plan's expected journal for the delay-
   discounting meta-analysis was wrong — it is *Biological Psychiatry:
   CNNI* [2]; (b) the plan's candidate "Toplak, Dockrell & Tannock
   ~2006 review" could not be verified anywhere and is **not cited** —
   the time-perception verdict rests on stronger replacements
   [11][12][13]; (c) catalogue sizing exceeded the plan's estimate
   (14 transfer vs ~10–15 expected: within range; 9 rejected vs ~5–8
   expected: one over) — the plan's D5 makes sizing an estimate, not a
   quota, and each rejected entry carries its reason; (d) stage-3
   review corrections: ref [44]'s URL was missing the `/inspiration/`
   path segment (dead link; the quote is verbatim at the corrected
   URL); the [91] title quote had dropped the word "up" ("ended up
   with a .txt file"); and §4.5's element-level summary of [21] was
   corrected — the meta-analysis's moderator findings concern game
   fiction and social interaction, not a badges/leaderboards/graphs
   vs avatars/stories outcome split (P8's source basis and the §3.9
   inventory row were aligned to the same correction).
3. **Method addition (within the plan's spirit)**: research was
   executed as six parallel sub-agent lanes under the citation
   contract, followed by lead re-verification of the seven most
   load-bearing citations (§2). This is reported for reproducibility.

## 11. Bookkeeping

Two out-of-scope improvement ideas surfaced during the research and
are parked in the potential-improvements backlog (they are product
features beyond this report's catalogue, per dispatch §3's scope
rule): an effort-estimate calibration feedback loop (from P14's
deeper implications + [26][51]) and curated workflow presets beyond
keymaps (from P7's evidence base + [69]). No other repo files changed.
