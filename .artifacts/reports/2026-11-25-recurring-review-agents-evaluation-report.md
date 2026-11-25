# Recurring Review Agents — Evaluation Report

- **Date**: 2026-11-25
- **Kind**: evaluation report — a backward-looking analysis of the
  project's review practice and CI lanes plus a forward-looking roster
  proposal. No code, template, or schedule is changed here; the prompt
  templates follow once the operator settles the roster (§8).
- **Origin**: operator request of 2026-11-25 — evaluate which analyses
  merit a prepared, regularly recurring prompt template for ZCode
  idle-time runs, and propose further review types serving long-term
  usability, stability, security, and functionality.
- **Scope**: the three recent analyses the operator named
  (`.artifacts/reports/2026-11-24-ui-slint-architecture-review-report.md`,
  `…-2026-11-24-test-coverage-health-report.md`,
  `…-2026-11-21-golden-coverage-review-report.md`), the planned
  usability/design review, the workspace's CI lanes and artefact corpus.
- **Working tree**: `main` @ `e8025b04`. Read-only; the only writes are
  this file and the `backlog:` issues in §9.
- **Side findings**: five `backlog:` issues filed per AGENTS §7
  (#1401–#1405) — three scheduled CI lanes have been dead or red for
  five to six weeks (§4) — and four more in the follow-up
  (#1411–#1414, §10).

---

## 1. Verdict in brief

| Candidate | Template? | Cadence | Form |
|---|---|---|---|
| UI-crate architecture review | yes, narrowed | monthly, one crate slice per run | delta since the previous report + finding ledger; the regex-detectable half moves to a lint |
| Test coverage health | yes — the strongest candidate | fortnightly | delta + "wired but never driven" census + mutation-survivor triage |
| Golden coverage analysis | no | — | a coverage preflight inside the design review |
| Usability / design review | yes — the most valuable new one | fortnightly, one surface slice per run | after a one-time baseline run |

Test used throughout, per candidate: does the subject **drift** between
runs; is the check **judgemental** (not a lint); is the output
**actionable without duplicating** what is already filed. A check that
fails the second test belongs in CI, not in a weekly agent.

---

## 2. Grounding

### 2.1 Velocity and corpus

| Fact | Value (2026-11-25) |
|---|---|
| Age / commits | 7 weeks (since 2026-10-07); 2,773 commits, 1,014 in the last 14 days |
| Executable lines | 143,265 (coverage report); ~24.8k on 2026-10-18 — about 5.7× in five weeks |
| Tests | 5,506, 90.8 s under nextest |
| Open issues | 174 (667 closed); 163 titled `backlog:`; 137 carry `p3` |
| Artefacts | 141 reports, 568 plans |
| Goldens | 34 PNG + `manifest.toml`; scale × width matrices only for Board, Today, Task detail; 15 other surfaces have one image each |
| Theme | one dark palette; 8 base colour tokens, 1 translucent scrim, 5 semantic colours (+2 aliases) |
| Accessibility markup in `ui/` | 75 `accessible-role`, 68 `accessible-label`, 16 `accessible-description` against 64 `TouchArea` + 39 `FocusScope` + 101 `IconButton` uses |
| Releases | `v0.1.0` (2026-10-16); `release.yml` runs only on dispatch/tag |

The 2026-10-16 `v0.1.0` audit (eleven themes: concurrency, docs drift,
perf budgets, security, sync persistence, test quality, time/DST, …)
was a one-off pass over a codebase roughly a sixth of today's size.

### 2.2 What CI already decides

Recurring agents must not re-derive these.

| Lane | Trigger | State on 2026-11-25 |
|---|---|---|
| `check`: fmt, Slint token lint, clippy, MCP/pdf feature-tree gates, sqlx cache, changelog self-test, doctor test | PR + main | green |
| `msrv`, `deny` (advisories, licenses, bans, domain-purity and dumb-UI ban configs) | PR + main | green |
| `test` (nextest + doc tests), `ui-mcp-smoke` | PR + main | green |
| `coverage` → Codecov (patch status informational by config), SonarCloud | PR + main | green |
| `benchmark` → Bencher | main push + Thu | green |
| `mutation-testing` (3 lanes) | Mon | **cut at the 240-min cap every week since 2026-10-19** (#1401) |
| `miri` (4 lanes) | Mon | **core lane cut at the 120-min cap every week since 2026-10-26** (#1402) |
| `kani` | Mon | green |
| `nextcloud-integration` (stable + floor) | Wed | **stable leg failed every week since 2026-10-21** (#1403) |
| `release` | dispatch / tag | not exercised since `v0.1.0` |
| Dependabot (cargo, github-actions) | weekly | configured |

Main-branch CI, last 15 runs: 13 success, 2 cancelled (not inspected;
consistent with the workflow's `cancel-in-progress` group).

Already mechanised inside the UI crate, beyond the CI lanes: WCAG
contrast over a declared list of token pairs (`src/token_tests.rs`,
design §9.1), the keypress ceilings (design §9.2,
`tests/support/guard.rs`), and a family of source-scan inventory tests
(`tests/inventory/`: view-mount callbacks, layout severing, icon
mapping, adapter seams) over the shared `source_scan.rs` scanner.

Not mechanised today, though hard rules: the in-file purity rules of
the UI crate (only the crate-level dependency ban is a gate), key
handlers and raw `TouchArea`/`FocusScope` outside the sanctioned
leaves, accessible names on elements no journey addresses,
scheduled-lane liveness. (Licence headers: 403 of 403 `.rs` files
carry one; `#![forbid(unsafe_code)]`: all 8 crate roots.)

### 2.3 Constraints the idle-time service puts on template design

Per the tool contract read in this session:

- A task is **one-off**, queued immediately, and runs when the server
  grants off-peak compute — **no guaranteed start time**. A cadence is
  therefore a ticket rhythm, not a calendar: every template anchors its
  window on the previous report of its family (base commit + date), never
  on "the last N days".
- It runs **unattended, in full-automatic mode, at the highest reasoning
  level**, continuing the session that created it. Templates must be
  self-contained and carry their own safety frame (§7).
- Creation consumes a **limited ticket quota**. The roster stays small
  and staggered; each template finishes in one run.
- An idle-time run **cannot create further tasks**. Someone (or a
  separate reminder) queues each run.
- Open risk, to validate in the first trial: whether an unattended run
  survives the ~10–14 minute silent stretch of the coverage trio — the
  duration that AGENTS §7 flags as exceeding the *subagent* inactivity
  cutoff. Mitigation: start the suite in the background and poll a
  predicate, never block silently.

### 2.4 Tooling note

The ZCode shell exposes `grep` as a function wrapping `ugrep -I`
(binary files are skipped). A source file containing a raw NUL byte is
therefore invisible to every agent search — observed on
`crates/taskboard-app/src/secrets.rs`, which a header census reported
as the only one of 403 `.rs` files without an SPDX line; it has one.
Counts in this report were cross-checked with `command grep` / Python.
Filed as #1405.

---

## 3. The four candidates

### 3.1 UI-crate architecture review — recurring, narrowed

**What the 2026-11-24 run did.** A full read of ~45k production lines
of Rust and 22k lines of `.slint` against four mandates; ten new issues
(#1349–#1362; nine still open on 2026-11-25) and three comments. Its
verdict: "architecturally healthy
at its seams and over-concentrated in its middle" — concentration debt
(`adapter.rs` ≈18.8k lines), widget-policy residue (21 rows), two
mirrored policies without parity pins.

**Drift: yes, and fast.** Most residue was four days old at review time
(it dated from the round-9 widget wave of 11-18..20). At ~70 commits a
day, a full re-read every fortnight would mostly re-find the same
issues — low yield per run.

**Judgement: about half.** The evidence sections are greps: purity
(`std::fs|env|process`, `async fn`, `tokio::spawn`, locks,
`.send().await`), key handlers in views/compositions, literal colours
and pixels, hand-rolled `TouchArea`/`FocusScope` blocks. That half
belongs in a lint next to `scripts/check_slint_tokens.py`, where it
would gate every PR. The judgemental half — business logic drifting
into the UI, mirrored policies without pins (F-3, F-5), clock
discipline, file and function growth, hand-maintained lists (#854) —
is what a recurring agent should carry.

**Scope gap.** No crate other than `taskboard-ui-slint` has had an
architecture review since the 2026-10-16 audit; `taskboard-app`
(103k lines) and `taskboard-domain` (95k) have roughly sextupled since.

**Verdict.** A monthly **architecture drift review** over one crate
slice per run, delta-oriented, with the previous findings as a ledger.
The slice is chosen by churn since that crate's last review, so the
rotation needs no operator decision. A full re-read is event-triggered
(after a wave like round 9), not calendrical.

### 3.2 Test coverage health — the strongest recurring candidate

**What the 2026-11-24 run did.** A local instrumented run merged
per-line across test binaries, a Codecov trend series, and — the
expensive part — a detached worktree at a core-era commit as a
baseline. Ten gaps (#1366–#1375), each with a test approach.

**Drift: intrinsic.** The headline is a sawtooth: feature waves drop
coverage 3–4 points, backfill waves restore it within days. The
valuable findings were invisible to the headline: a verb wired for
three weeks without a driver (#1367), five subcommands never driven
(#1368), dispatch closures that no handler-level suite touches
(#1370). None of those need an instrumented run to find — they are a
static cross-reference of handlers, verbs, and commands against test
trees.

**Judgement: the ranking and the remedy.** Codecov holds the trend
and, through its API, per-file and per-line coverage for every commit
(§10.3). What no coverage oracle shows is an entry point nobody drives
— the per-verb gap (structural blind spot 3 of the report) — and the
patch status is informational by config.

**"Only exercising the happy path" is the mutation lanes' question.**
Line coverage cannot see a test that executes a branch and asserts
nothing about it; a surviving mutant says so directly. The weekly CI
lanes exist for exactly this and are dead (#1401) — so the template
reads the newest survivor artifact and states its staleness, or the
lane is revived first.

**Verdict.** Fortnightly **test-strength review**: Codecov first —
trend, per-file totals at base and head, line level at head — then the
undriven-verb and undriven-command census (static), then survivor
triage. The local instrumented run is a fallback, since Codecov
answers for the base commit too, which no local run can (the 2026-11-24
run built a detached worktree for it). The first draft of the template
kept the local run as the main step and named a wrong Codecov URL;
corrected the same day after the operator's question (§10.3).

### 3.3 Golden coverage analysis — no standalone template

A structural analysis that has already converged: all three of its
issues (#1139–#1141) were closed within four days, the manifest it
asked for exists (`tests/goldens/manifest.toml`), and each of the
seven zero-coverage surfaces now has a golden. What remains is a
census the design review needs anyway.

**Verdict.** No template. It becomes the **coverage preflight** of
§3.4: parse `manifest.toml`, compare with the census slice under
review, and state plainly what the run could not judge. The report's
verdict on the comparator (luma-only, cell-averaged) concerns
regression diffing only; an absolute review reads the PNGs directly.

### 3.4 Usability and design review — the most valuable new template

The operator's list of factors is essentially **WCAG 2.2** (W3C, *Web
Content Accessibility Guidelines*) plus **Nielsen's ten usability
heuristics**. The relevant criteria:

| Factor | Standard | How it can be checked |
|---|---|---|
| Contrast | SC 1.4.3 Contrast (Minimum): 4.5:1 text, 3:1 large text; SC 1.4.6 (Enhanced): 7:1 / 4.5:1; SC 1.4.11 Non-text Contrast: 3:1 | token pairs by script; rendered edge cases by eye |
| Labels | SC 3.3.2 Labels or Instructions; SC 4.1.2 Name, Role, Value; heuristic 6 (recognition over recall) | static inventory from `ui/`, then judgement |
| Icons and icon opportunities | heuristic 2 (match with the real world), 4 (consistency) | judgement over the 19 vendored icons × their usage sites |
| Alignment, padding, sizes across layouts | SC 1.4.4 Resize text; SC 2.5.8 Target Size (Minimum) 24×24; heuristic 8 (minimalist design) | needs layout coverage (below) |
| Heuristics | Nielsen's ten, incl. SC 1.4.1 Use of Color, 2.4.7 Focus Visible | judgement over goldens + source |

The factors split by method, and the split is the design:

1. **Computable — and already gated.** `src/token_tests.rs` asserts
   the design §9.1 minimums over a declared list of token pairs. What
   a review must add is the pairs the markup uses that the list does
   not declare. Measured on 2026-11-25: the state colours and
   `Theme.border` are declared against the two base surfaces only;
   against raised and hover, `Theme.border` is 2.75 / 2.34 and
   `Semantic.danger` 4.92 / 4.19 — below their classes — and the
   markup renders several of them (the tooltip's boundary at
   `widgets/tooltip.slint:134-136`). Filed as #1414.
2. **Judgement — the agent's real work.** Label clarity, icon fit,
   heuristic violations, error and notice copy, kiosk legibility.
   This complements the stage-3 walkthrough protocol
   (`docs/ux_planning.org` §2.4), which covers a *new feature's*
   journey with a six-heuristic rubric at review time; the periodic
   review covers what accumulates *across* features and the factors the
   rubric leaves out (contrast, spacing, sizing, icons).
3. **Measurement-hungry — needs tooling first.** "At every layout"
   cannot be answered for 15 of the 18 surfaces that have goldens:
   they have a single image each (mostly scale 1.0, 1000 px). Two
   sources exist: pixels of the software-rendered goldens (flat token
   colours make element rectangles segmentable; brittle at
   anti-aliased edges), and element geometry, which the test driver
   already exposes (`tests/support/driver.rs`: `find_all_by_label`,
   `find_all_by_id`, `Element::size()`, `absolute_position()`). A
   per-surface layout atlas — every surface at 600 / 1000 / 1500 ×
   0.5 / 1.0 / 2.0 / 3.0, with a geometry dump — needs no new API;
   filed as #1411 at the operator's request.

**Verdict.** Yes — fortnightly, **one census slice per run** (Board;
Today + habits; Task detail + editors; Capture / search / action menu;
Review + plan-next-day; Settings; Alarm / notification / focus overlay;
and a cross-cutting widget slice), chosen by churn, each opened by the
coverage preflight of §3.3. A one-time **baseline run over the whole
UI** precedes the rotation: it fixes the checklist, the measurement
method, and the ledger, and says whether the atlas is needed before
the rotation starts. Vision alone should not decide 1–2 px questions;
the baseline should say which measurements it can make reliably.

---

## 4. Scheduled-lane health snapshot (2026-11-25)

A side check while grounding §2.2. All three would have been caught by
a liveness check after the first bad week.

| Lane | Symptom | Since | Issue |
|---|---|---|---|
| `mutation-testing` | both `cargo mutants` jobs `cancelled` at exactly `timeout-minutes: 240`; third lane never starts; uploaded `mutants.out/` holds only the mutants evaluated before the kill | 2026-10-19 (last complete sweep 10-12, 149 min) | #1401 |
| `miri` — `domain — core` | `cancelled` at `timeout-minutes: 120`; on 11-16 two sibling lanes took 109 and 111 min | 2026-10-26 | #1402 |
| `nextcloud-integration` — stable leg | fails ~3 min in, at build of `taskboard-app`: `yeslogic-fontconfig-sys` finds no `fontconfig`; the workflow's apt step never gained the six GUI system packages every other workflow installs | 2026-10-21 | #1403 |

Effects: the mutation survivors → report → triage loop has had no
complete input for six weeks; the core-domain Miri verdict is five
weeks stale; the daemon + CLI journeys against current Nextcloud + Deck
have not run live in six scheduled runs (the sync-actor step on the
same leg does run and pass).

Cross-links: #1338 (its wider mutants lane would land on a dead lane),
#436 (closed 11-01; the lane it re-enabled has been red every week
since).

Take-away for the roster: lane liveness is a **mechanisable** check (a
dead-man's switch over the last N runs of each scheduled workflow), not
an agent review.

---

## 5. Proposed further review types

Ranked by expected yield for the four goals. "Mechanise instead" names
the part that should become a gate, shrinking the agent's share.

| # | Review | Cadence | Why here | Mechanise instead |
|---|---|---|---|---|
| P1 | Watchlist and flaky-test pass | fortnightly | 10 open `watchlist` issues (#697 at 21 comments); formal passes on 10-17, 10-26, 11-04, then ad-hoc comments; AGENTS §7 mandates a dedicated pass. Token-cheap, compute-heavy — ideal idle work | — |
| P2 | Security and trust-boundary review | monthly delta; quarterly deep-dive per boundary | Surfaces grown since the 10-16 audit theme: credential stores (`secrets.rs`), IPC, web dashboard, MCP server, authority/escalation, lock remediation (kills and restarts by PID), files-sync ingest of remote content, export paths. Open: #284 (accepted-risk register), #139. `cargo deny` and Dependabot cover dependencies only | supply-chain checklist, quarterly |
| P3 | Docs and rules drift | monthly | #1404: `AGENTS.md` §2/§6 omit two crates for three weeks; a stale comparator doc comment found 11-21; census vs code; runbooks | crate-list-vs-workspace check; raw-byte check (#1405) |
| P4 | Correctness and reliability rotation — silent failures, data-loss vectors, time/DST logic | monthly, one focus per run | Warn-and-drop is the house idiom and logging arms are uncovered by rule; kiosk offline resilience is a project goal; `merge.rs` is 8.9k lines; time/DST was a one-off audit theme | — |
| P5 | Backlog hygiene and finding ledger | monthly | Several agents file into 174 open issues (137 `p3`) — duplicate risk is structural; the ledger is how found potential is tracked (§7) | per-family labels |
| P6 | Agent-process retrospective | monthly | Every incident law to date is reactive (10-13, 10-18, 10-23, 11-06, 11-16, 11-17, 11-23); mining stage-2/3 and stop reports for recurring friction finds the next one earlier | — |
| P7 | CI and release hygiene | mostly mechanise | §4; `release.yml` unexercised since `v0.1.0` while the workspace doubled | dead-man's switch (#1413); one shared system-deps step; monthly release dry-run |
| P8 | Performance and budget trend | quarterly, once alerts exist | Bencher receives weekly results; kiosk draw/refresh and memory budgets were a one-off audit theme | Bencher threshold alerts |

Not recommended as templates: golden coverage and census/parity drift
(preflights of the design review); dependency watching (Dependabot +
`cargo deny`; the quarterly supply-chain pass suffices).

---

## 6. Recommended roster

Start small, measure yield for four weeks, then grow. Yield is
reported by every run (§7): new issues, comments, duplicates
suppressed, previously filed findings now closed.

| Tier | Template | Cadence | Prerequisite |
|---|---|---|---|
| Start | `test-strength-review` | fortnightly | none; mutant section flags lane staleness until #1401 |
| Start | `ui-ux-design-review` | baseline, then fortnightly per slice | none for the baseline; the atlas (#1411) for exact layout facts |
| Start | `architecture-drift-review` | monthly per crate slice | none; shrinks as the inventories of #1412 land |
| Start | `watchlist-flaky-pass` | fortnightly | none |
| Add after four weeks | `security-review` | monthly | #284 as the register anchor |
| Add after four weeks | `docs-drift-review` | monthly | none |
| Add after four weeks | `correctness-reliability-review` | monthly, rotating | none |
| Add after four weeks | `backlog-hygiene-review` | monthly | per-family labels |
| Optional | `agent-process-retro`, `budget-trend-review` | monthly / quarterly | — |

That is about two tickets a week at the start and three with the full
roster. If the quota is tighter, cut from the bottom of the "Add" tier
first; the four "Start" templates carry most of the value.

---

## 7. Cross-cutting template rules

1. **Self-anchoring window.** Each report records its base commit and
   date in the header. The next run of the family finds the newest
   previous report by slug and reviews `base..HEAD`; with no previous
   report it runs in baseline mode. No template says "last N days".
2. **Churn-driven slices.** Rotating reviews compute commits per slice
   since that slice's last review and take the most-churned one.
3. **Ledger before findings.** Every run first reconciles the previous
   report's issues (open / closed / fixed / unchanged) and files a
   yield line. Findings map to issues per AGENTS §7 with search-first
   dedupe; "already filed" is a ledger entry, never a new issue.
4. **One label per family** (proposed: `review-<family>`) on every
   issue a family files, so `gh issue list --label` is the trend line
   of found-versus-realised potential. The three existing families'
   issues (#1349–#1362, #1366–#1375, #1139–#1141) can be labelled
   retroactively so the ledger starts complete.
5. **Lint-first.** Each template names the checks that are lint
   candidates and, when its evidence confirms one, proposes the gate in
   the report instead of re-finding the same class next run. Current
   candidates: UI purity and widget-policy scans (#1412); the contrast
   pairs the declared list skips (#1414 — the gate itself exists);
   accessible names on interactive elements; scheduled-lane liveness
   (#1413); raw control bytes in sources (#1405); crate list vs
   workspace members (#1404); one shared CI system-deps step (comment
   on #1403).
6. **Safety frame** (the run is unattended and in full-automatic mode):
   read-only on the product — no commits, pushes, PRs, merges, branches,
   or worktrees unless the template says so (then only after
   `scripts/worktree-doctor.sh` passes, with teardown at the end); every
   `cargo` invocation through `scripts/memory-gate.sh`, the coverage
   trio through `scripts/run-quality-gates.sh`; never
   `cargo mutants --in-place` in the main checkout; probe processes are
   killed by recorded PID, never by name (`docs/ui_testing_instrumentation.org` §6).
7. **No questions.** Assumptions and open questions are recorded in
   the report; a broken prerequisite narrows the stated scope rather
   than inviting improvisation.
8. **Reproducibility.** Commands, commit, tool versions, and known
   pitfalls (the lcov line counts, the grep wrapper of §2.4, the
   Codecov URL shapes of §10.3) travel in the
   template.
9. **One lean session per family.** An idle-time run continues the
   session that created it; tickets created from a long working session
   carry that history. A short prompt pointing at the template file is
   all a ticket needs.

---

## 8. Decisions for the operator

1. **Roster and operating point** — §6 as proposed, trimmed, or
   extended; how many tickets per week the quota allows.
2. **Analysis-only** — recommended: idle-time runs produce reports and
   issues; fixes flow through the normal plan → dispatch path. The
   alternative (runs may prepare fix branches) trades review control
   for speed.
3. **Family labels** and the retro-labelling of the three existing
   families.
4. **Lint-first items** — whether to schedule the gate work (§7.5)
   alongside the templates; each shrinks a running review.
5. **Design review start** — baseline now on images and source (the
   recommended first step; it decides whether the layout atlas is
   needed), or wait for the atlas.
6. **Template location** — recommended:
   `.artifacts/templates/reviews/<family>.md`.

---

## 9. Side findings filed

| Issue | Finding | Labels |
|---|---|---|
| #1401 | weekly mutation-testing sweep cut at the 240-min cap since 2026-10-19 | ci, testing, p2 |
| #1402 | Miri `domain — core` lane cut at the 120-min cap since 2026-10-26 | ci, testing, p3 |
| #1403 | Nextcloud Integration stable leg fails at build since 2026-10-21 (apt step lacks the GUI system deps) | ci, sync, p2 |
| #1404 | `AGENTS.md` §2 and §6 omit `taskboard-sync-files` and `taskboard-sync-files-nextcloud` | documentation, p3 |
| #1405 | raw NUL byte in a `secrets.rs` test literal makes the file binary to tooling | dx, p3 |

Comments: #1338 (its lane never completes), #436 (the lane it re-enabled
has been red every week since). #1403 is the one to look at first: the
daemon + CLI journeys against current Nextcloud + Deck have had no live
run for six scheduled weeks, and the fix is a one-line apt change.

---

## 10. Operator decisions and follow-up (2026-11-25)

**Decisions.**

1. Start with four templates: test strength, design, architecture
   drift, watchlist pass.
2. Analysis only — idle-time runs produce reports and issues; fixes
   take the normal plan → dispatch path.
3. A label per review family: questioned by the operator, measured in
   §10.1, approved the same day — four labels created and 25 earlier
   issues labelled (§10.2).
4. The design baseline runs on the functionality that exists once
   backlog-reduction round 7 has landed; a full layout atlas is filed
   as its own issue (#1411) and later runs may build on it.

**Correction of this report's first draft.** §2.2, §3.4 (items 1 and
3), and §7.5 were corrected the same day. The first draft proposed a
contrast gate and a geometry API as new work; both already exist
(`src/token_tests.rs`, `tests/support/driver.rs`). What remains of
each is filed: #1414 (the pairs the declared list skips) and #1411
(the atlas over the existing seams).

**10.1 What a per-family label buys — measured.** Every issue filed
under AGENTS §7 carries an `Origin:` line naming its report, and
GitHub's body search finds that line. So the question is what a label
adds over searching by report slug. Tested on the three existing
families (`gh issue list --state all --search "<report slug>
in:body"`):

| Family | Issues the review filed | Hits by report slug |
|---|---|---|
| test coverage (11-24) | 10 (#1366–#1375) | 11 |
| UI architecture (11-24) | 10 (#1349–#1362) | 14 |
| golden coverage (11-21) | 3 (#1139–#1141) | 9 |

The difference is issues that mention the report without having been
filed by that run: as evidence in a later issue (#1401, #1404, #1412),
as a follow-up the report's recommendations produced (#1312, #1313), or
as older context (#680, #1239, #1390, #1409, #237, #252). A search
cannot tell *origin* from *mention*; a label is the origin. The rest of the benefit: one exact
query for open and closed per family, a filter in the GitHub interface,
and independence from the artefact files, which are untracked until a
docs change commits them. The cost is one label per family; the
templates apply it automatically when it exists and never create it.

**10.2 Labels.** Created, all in one colour so they read as a family:
`review-test-strength`, `review-ui-ux-design`,
`review-architecture-drift`, `review-watchlist-pass`. Each template
adds its own to every issue it files. Retroactively labelled by the
`Origin:` line of each issue: the UI architecture review's ten issues
(#1349–#1362) carry `review-architecture-drift`; the test coverage
report's ten (#1366–#1375) carry `review-test-strength`; the golden
coverage review's five (#1139–#1141, #1312, #1313) carry
`review-ui-ux-design`, since golden coverage is that review's
preflight. Issues that only cite a report were left unlabelled, and so
were the findings of one-off sessions such as this one (#1401–#1405,
#1411–#1415), whose attribution is their `Origin:` line.

**A limit of the contrast gate, shown by an operator screenshot.**
`token_tests.rs` cannot see text drawn on a model-supplied fill. The
task card's label chips fill with the label's own colour and pin the
text to `text-secondary` (`ui/widgets/task-card.slint:356-369`):
1.10–2.05:1 over the six palette swatches, on every label chip in the
app. Filed as #1415 with three measured fix directions; the design
template's contrast step gained a check for text over model-supplied
fills.

**10.3 Codecov's API (operator question, 2026-11-25).** The 2026-11-24
coverage report states that Codecov's `file_tree`/`file_report`
endpoints returned 404, so per-file data had to come from a local run.
Checked against the published schema
(`https://api.codecov.io/api/v2/schema/`) and live against the
repository, without a token:

- The deeper analysis exists and is documented: `totals/` (commit
  totals plus a breakdown per file), `report/` (line-by-line coverage
  for every file, 1.3 MB), `report/tree` (directory rollups),
  `file_report/{path}` (one file), `coverage/` (a trend series by
  interval), and `compare/` (a per-file base-to-head diff). Base
  `https://api.codecov.io/api/v2/github/D4id4los/repos/taskboard`.
  Reads of a public repository need no token, although the schema
  marks every operation `tokenAuth`.
- The agent's names were wrong. `file_tree` does not exist (it is
  `report/tree`), and it did not try `totals/`, whose description names
  the per-file breakdown.
- One documented URL is broken on Codecov's side: `file_report/{path}/`
  *with* its trailing slash answers 404 (`coverage info not found for
  path '…/'`). Without the slash it works.
- Why Codecov and the local run disagreed — 95.21% against 95.53% at
  the same commit — is the compiler, not the lcov format. CI built
  with rustc 1.100.0; this machine's stable is still 1.99.0. Filed as
  #1421. The report's mechanism note is wrong as well:
  `cargo llvm-cov report --lcov` writes one `SF` record per file (194
  for 194 files); the ~4% overstatement is in the `LF:`/`LH:` summary
  lines, which count a line with several coverage segments more than
  once.

Consequences: the test-strength template reads Codecov first, for base
and head alike, and runs the local suite only as a fallback; the lcov
note in the common rules is corrected.

**10.4 Landing the templates (2026-11-25).** The operator approved the
templates and asked for a pull request, merged once its checks are
green. Landing them followed the prescribed flow — the doctor, a
worktree off the fetched `origin/main`, a commit, a push — and surfaced
two more findings, both filed:

- #1424 — the pre-commit hook admits a docs-only commit's clippy
  *without* the exclusive lock (#1274's preflight mode, which assumes a
  warm cache). In a fresh worktree that is a cold, unlocked build:
  measured at 13.8 GiB of `rustc` seventy-five seconds in. The commit
  here was made only after warming the cache under the lock at four
  jobs.
- #1425 — `scripts/worktree-doctor.sh` reports every registered
  worktree as unregistered when it is run through the symlinked
  checkout path (nine warnings, against three by the real path).

The full local gate suite was not run for this change. It is templates
and one report; no test reads `.artifacts/`; and a cold full-suite
worktree needs more disk than was free beside the round-7 worktrees.
Only the pre-commit hook ran locally (fmt, the Slint lint, shellcheck,
clippy). CI runs the suite on the pull request.

**Templates.** `.artifacts/templates/reviews/`:
`_common-rules.md` (bound by all four), `test-strength-review.md`,
`ui-ux-design-review.md`, `architecture-drift-review.md`,
`watchlist-flaky-pass.md`, and `README.md` with the one-line queueing
prompts. Untracked at the time of writing. Two things the operator
should know:

- The design template refuses its baseline until round 7's
  layout-readiness chunks have landed (#1239, #1241, #1312, #1313
  closed and no round-7 branch still open as a pull request); it then
  writes an ABORTED report naming what is open.
- Beyond `git fetch`, a run may fast-forward the `main` checkout, only
  when it is on `main`, clean apart from `.artifacts/`, and merely
  behind `origin/main` — the local checkout was 19 commits behind when
  this was written. That is the one git write the templates allow.
- The test-strength template reads its coverage from Codecov — for the
  base and the head commit alike — and runs the local instrumented
  suite only as a fallback (§10.3).

**Issues filed in the follow-up.**

| Issue | Finding | Labels |
|---|---|---|
| #1411 | layout atlas: every surface across the layout axes, with an element-geometry sidecar (operator-requested) | ui, testing, p3 |
| #1412 | source-scan inventories for the widget policy and the UI purity rules — no gate exists for either | ui, testing, architecture, p3 |
| #1413 | dead-man's switch for the scheduled workflows | ci, p2 |
| #1414 | `token_tests`' pair list skips the raised and hover surfaces; those pairs measure below their minimum and the markup renders them | ui, accessibility, p3 |
| #1415 | task-card label chips draw light-grey text on the label's own colour — 1.1:1 to 2.1:1 on every palette swatch (operator screenshot) | bug, ui, accessibility, p2 |
| #1421 | the local stable toolchain lags CI by one release (rustc 1.99.0 against 1.100.0) — local gates and coverage run on a different compiler | ci, dx, testing, p2 |
| #1424 | a docs-only commit in a fresh worktree runs a cold clippy outside the memory-gate lock | ci, dx, p2 |
| #1425 | the worktree doctor reports registered worktrees as unregistered when run through the symlinked checkout path | dx, p3 |

Comments: #1401 (where survivors are recorded — the testing strategy
§13 and the workflow comment disagree), #1403 (a shared system-deps
step).

**Not yet done.** `AGENTS.md` does not point at the templates folder;
a one-line pointer in §7 is a separate change, since it edits the
rulebook. No idle-time run has been queued — the templates are
untested, and the first run of each family should be watched rather
than left alone.
