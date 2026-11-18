# Coverage-Gate Correctness — Chunk N-A Implementation Plan

- **Date**: 2026-11-18
- **Kind**: implementation plan (stage-1 output, dispatch plan
  `.artifacts/plans/2026-11-18-post-round9-parallel-dispatch-plan.md`
  §2 chunk N-A; origin report
  `.artifacts/reports/2026-11-18-post-round9-parallel-recommendation-report.md`
  §Tier-1 N-A)
- **Covers**: #920 (per-invoked-plan step-token scoping), #768
  (bound-test-name mapping for the retro cohort), #760 (wiring-row ↔
  `seam_inventory.rs` cross-check)
- **Branch**: `ci/coverage-gate-correctness`; diff confined to
  `scripts/ux-coverage.sh`, `scripts/fixtures/ux-coverage-fixture/`,
  `docs/ux_planning.org`
- **Status**: READY for stage 2. Every open question below carries a
  recorded decision (§D1–D10). No code written at planning time.

## 1. Problem statement — the before-evidence

`scripts/ux-coverage.sh <plan> --gate` resolves a journey-step token
`us<n>_s<k>` if *any* test function under the Test-root contains the
token as a substring (`grep -F "fn us1_s1"`). Step ids are reused
across plans by the naming law's design, so cross-plan collisions are
structural (#920). Captured 2026-11-18 against the current tree (the
five r9 plans are framework-era and their journey tests are not yet
written — every FOUND below is by construction a foreign plan's test):

```
$ scripts/ux-coverage.sh .artifacts/plans/2026-11-18-habit-parity-ux-plan.md
US-1/S1 journey test → FOUND (…/tests/journeys.rs:10922)   # fn us1_s1_settings_cards_render_the_files_roster — the files-sync-status plan
US-1/S2 journey test → FOUND (…/tests/journeys.rs:10962)   # fn us1_s2_roster_badges_sort_and_tones — files-roster
US-2/S1 journey test → FOUND (…/tests/journeys.rs:11100)   # fn us2_s1_remove_arms_then_confirm_sends_exact_request_once — files-sync-status
US-3/S1 journey test → FOUND (…/tests/journeys.rs:11825)   # fn us3_s1_switch_action_dispatches_move_board_to_files — files-sync-status
US-6/S1 journey test → FOUND (…/tests/task_card_hover_probe_tests.rs:33)  # fn us6_s1_the_done_checkbox_carries_the_hover_flag — hover-probe
```

The other four plans resolve to the same foreign lines — e.g.
widget-framework `US-1/S1` → `journeys.rs:10922` (files roster),
keyboard-focus `US-1/S1` → the same `journeys.rs:10922`, day-review
`US-1/S1` → the same, menus-remainder `US-1/S1` → the same. Meanwhile
the habit-parity plan's own §6 Test column nominates
`habits_journey.rs::us1_s2` — a file that does not exist yet — and the
gate never consults it. Every step row of all five plans resolves
today, so the gate would stay green even if none of r9's journey
tests were ever written. This is the exact false-green #920 reports
(found live during the ui-r8 task-detail-editor stage-3 review).

Retro-cohort exhibit (#768): the round-7 pilot plans (e.g.
`.artifacts/plans/2026-11-13-r7-habit-create-ux-plan.md`) have
comprehensive behavior tests under descriptive names
(`lib.rs::habit_create_sends_the_exact_command`,
`lib.rs::recur_editor_commits_the_exact_assembled_rule`); their
`us<n>_s<k>` gate rows can never resolve as `fn`-token matches without
renaming shipped tests — the token law is retroactively unsatisfiable
for the whole retrofit cohort.

#760: the wiring-inventory callback check stops at "`.slint` hit +
adapter hit"; a callback claimed wired but absent from the adapter's
structural manifest (`crates/taskboard-ui-slint/tests/seam_inventory.rs`,
the two-sided totality census) is never flagged.

## 2. The verdict-state contract ([L](a) — machine-readable)

Every check line ends in exactly one of five states, each with
parenthesised machine-locatable evidence:

| State | Meaning | Counts into |
|---|---|---|
| `FOUND (file:line)` | the plan's own declared evidence resolves, scoped to this plan | n_found |
| `BOUND (file:line)` | a retro plan's `bound-test:` line names a pre-existing test that exists | n_bound |
| `MISSING` | declared evidence absent | n_missing |
| `AMBIGUOUS (k candidates: f1:l1, f2:l2)` | token-named tests exist but cannot be attributed to this plan — triage, never a credit | n_ambiguous |
| `INVENTORY-MISS (slint:line, adapter:line)` | wiring resolves but the callback has no `SEAM_INVENTORY` entry — triage | n_invmiss |

Summary line becomes:
`checks: N, FOUND: a, MISSING: b, AMBIGUOUS: c, BOUND: d, INVENTORY-MISS: e`.
`--gate` exits 1 when `MISSING + AMBIGUOUS + INVENTORY-MISS > 0`
(BOUND and FOUND are the only passing states). Exit 2 (parse) and 64
(usage) unchanged. [L](c): this strictly *narrows* what counts as a
pass — every row that passes today under scoping failure becomes a
triage state, never a blanket pass.

## 3. Decisions (all open questions resolved)

**D1 — per-invoked-plan scoping anchor (#920).** The plan nominates
its own evidence; the gate trusts nothing ambient. The nomination
surface is the plan's §6 wiring `Test` column, already populated in
all five r9 plans in the form `<file>.rs::<token-or-name>` (e.g.
`habits_journey.rs::us1_s2`, `focus_law_journeys.rs::us1_s1`,
`lib.rs::us5_s1_the_title_…` for src unit tests). Extraction: from
every wiring row's Test cell, parse cells matching
`([A-Za-z0-9_/.-]+\.rs)::([A-Za-z0-9_]+)`; prose cells ("fold test")
are ignored. Build two per-plan maps:
  - *exact-name nominations*: `<fn_name>` (right side is not a bare
    `us<n>_s<k>` token) → set of files;
  - *file-scoped tokens*: right side matches `^us[0-9]+_s[0-9]+$` →
    file → token.
  A file side that contains no `/` resolves under the Test-root
  directory; a side with `/` resolves under `--root`. Nomination is
  optional per step: steps named in no Test cell fall through to D3.

**D2 — the AMBIGUOUS trigger (exact).** For a step token `t`:
  1. If an exact-name nomination covering `t` exists: grep
     `fn <name>` in the nominated file(s) (or workspace-wide for
     src-relative paths) → single hit = `FOUND (file:line)`; none =
     `MISSING`.
  2. Else if a file-scoped nomination exists for `t`: boundary match
     (`fn <t>_` or `fn <t>(`) restricted to the nominated file →
     exactly one hit = `FOUND (file:line)`; zero = `MISSING`; ≥2 =
     `AMBIGUOUS` with both candidates excerpted.
  3. Else (no nomination for `t`): boundary match across the whole
     Test-root. Zero hits = `MISSING`. Any hits = `AMBIGUOUS` — a
     bare token in a shared tree cannot be attributed to this plan
     (that attribution failure *is* #920), so every unscoped hit is a
     triage state, never a credit. This is what converts today's
     spurious FOUNDs into honest rows without weakening anything.
  Boundary matching replaces `grep -F "fn ${token}"` everywhere
  (today it also matches `fn us1_s1_settings_cards…` when looking for
  `fn us1_s1`).

**D3 — plans that nominate nothing.** Behaviour for a plan with no
parseable Test-column nominations: every step is D2 case 3
(AMBIGUOUS-if-hit / MISSING-if-absent). No special mode, no
fallback-to-today (that fallback is the bug).

**D4 — bound-tests mapping format (#768).** Retro plans carry one
line per mapped step, anywhere in the plan body (§8 recommended):

```
bound-test: US-1/S2 -> lib.rs::habit_create_sends_the_exact_command
```

Grammar: `^bound-test:[[:space:]]*(US-[0-9]+/S-[0-9]+)[[:space:]]*->[[:space:]]*([A-Za-z0-9_/.-]+\.rs)::([A-Za-z0-9_]+)$`.
Verification: grep `fn <name>` (exact) workspace-wide under `--root`
(bound tests may live in `src/` unit-test modules, not just the
Test-root) → exists = `BOUND (file:line)`; absent = `MISSING` (a
bound line naming a non-existent test is a lie, never a pass). A
`bound-test:` line for a step also covered by a Test-column
nomination: the nomination wins (BOUND is the retro path; do not
double-report). `bound-test:` lines for step ids the plan does not
declare in §4 are a `PARSE-ERROR: bound-tests: unknown step id`
(exit 2) — fail loudly, never silently ignore.

**D5 — INVENTORY-MISS cross-check mechanics (#760).** For every
wiring-row callback whose wiring check resolves (slint hit AND
adapter hit): locate `seam_inventory.rs` once via `find` under
`--root` (same name-based universe as the other file classes), then
fixed-string grep the exact literal `"<callback>"` in it (the
inventory is a `&["…"]` array of exact callback names). Present →
the callback verdict stays `FOUND`. Absent → the callback verdict is
`INVENTORY-MISS (slint file:line, adapter file:line)` — a triage
state that fails `--gate`. Rationale for gate-failing: the inventory's
side-2 count assertion forces every *installed* seam to be listed, so
an inventoriless callback means either not-yet-installed (stage-2
timing — correctly not green) or a wiring row claiming something the
adapter never installed (exactly the declared-but-never-installed
class #760 names). Callback checks that already MISS keep `MISSING`
(no inventory consultation — the cross-check is an *additional*
honesty pass on resolved wiring, not a substitute). If no
`seam_inventory.rs` exists in the tree (e.g. the miniature self-test
fixture tree), the cross-check is skipped for that run — absence of
the inventory file is a fixture-shape fact, not a finding (recorded
so the fixture tree stays minimal).

**D6 — capability-matrix and ruling checks.** Unchanged semantically,
but their FOUND/MISSING inputs are now the scoped step verdicts: a
story's capability citation is covered only when none of its steps is
MISSING/AMBIGUOUS (AMBIGUOUS poisons the citation — again never a
silent credit).

**D7 — going-forward naming convention and its doc home.** Choice:
**`docs/ux_planning.org` is the law home; the script's `--help`/
header carries a two-line pointer only** (usage text stays usage;
normative conventions live with the framework law, which is the file
the template and AGENTS §7 already cite). Documented in §2.3:
  - journey-test names carry the plan slug as an underscore segment:
    `<file>` is the per-plan journey file
    `<topic>_journeys.rs` (slug = the plan filename's `<topic>`
    between the date and `-ux-plan`, `-`→`_`; e.g. `habits_journey.rs`
    is grandfathered — the law binds new plans, no mass rename) and
    test names are `us<n>_s<k>_<description>` inside that per-plan
    file;
  - the §6 `Test` column MUST nominate each journey step's file (the
    r9 plans already do);
  - retro plans use the `bound-test:` line grammar (D4).
  The r3 D-9 posture stands: framework-era plans' step tokens are
  REQUIRED rows — they may not use `bound-test:` to swap the token
  law out; the mapping exists for the retro cohort only.

**D8 — self-test extension.** `--self-test` keeps its exit-codes-and-
counts-only contract (AGENTS §5 no-text-assertions: state tokens are
structure, not prose) and gains fixture variants:
  - PASS fixture extended: plan nominating a per-plan file + a
    `bound-test:` line; asserts exit 0 and BOUND count.
  - AMBIGUOUS fixture: two files in the fixture tree carrying the
    same bare token, plan nominating neither → asserts the AMBIGUOUS
    count (not text).
  - INVENTORY-MISS fixture: wiring resolving in the fixture tree with
    a `seam_inventory.rs` lacking the callback → asserts exit 1 and
    the inventory-miss count.
  - Existing malformed-header (exit 2) and MISSING (exit 1, count 3)
    variants unchanged, adjusted only for the new counts line shape.

**D9 — exit-contract compatibility.** Callers today: stage-2/3
protocols treat exit 1 as "rows open" and exit 0 as green. No caller
branches on the counts line's exact text (the no-text-assertion rule
forbids it), so widening the counts line is compatible. The gate
remains fail-closed: any state other than FOUND/BOUND keeps exit 1.

**D10 — implementation order (TDD).** (1) Fixture variants first
(D8) — they encode the acceptance contract and fail against today's
script; (2) scoping engine (D1–D3); (3) bound-tests parser (D4);
(4) inventory cross-check (D5); (5) docs (D7). Each step lands a
green `--self-test` before the next. The five r9 plans are the
acceptance oracle (§4), not unit tests — no text assertions on plan
prose.

## 4. Acceptance run

Gate invoked against each of the five r9 plans
(`.artifacts/plans/2026-11-18-{habit-parity,widget-framework,keyboard-focus,day-review,menus-remainder}-ux-plan.md`)
before and after the change. Note for stage 2: as of 2026-11-18 the
five r9 plans are untracked in the main checkout (r9 is still landing
them); the acceptance run must execute in an environment where they
exist — copy them into the worktree for the run, do not commit them
here (they land with r9's own docs commits).

Required after-shape (per plan, expected — the journey tests do not
exist yet, so honest states are MISSING/AMBIGUOUS, never foreign
FOUND):

- **Zero cross-plan FOUND**: no journey row may resolve to a test
  function whose file the plan does not nominate (the before-evidence
  §1 shows every row doing exactly that).
- Steps named in the plan's §6 Test column resolve against the
  nominated file once r9 lands it; today they read
  `MISSING` (nominated file absent) — the honest predecessor of
  FOUND, and the demonstration that the false green is gone.
- Steps nominated nowhere read `AMBIGUOUS (k candidates…)` where the
  shared tree has foreign `fn us<n>_s<k>…` matches, `MISSING`
  otherwise — excerpted before/after in the stage-2/3 report.
- Wiring rows that resolve against the current tree (e.g. habit-parity
  `W-3 action habit.convert → FOUND` and `W-1 command LogHabit →
  FOUND` from the §1 capture) are inventory-cross-checked; callbacks
  absent from `SEAM_INVENTORY` read `INVENTORY-MISS`.
- The fixture self-test exits 0 on all variants (D8).

Acceptance verdicts are per plan: file path + the state counts line
before/after + the one spurious FOUND from §1 that the change
eliminates, quoted verbatim.

## 5. Out-of-scope findings → backlog

- **Wiring-token boundary matching** (#965, filed at planning time):
  the wiring
  checks' `first_match`/`adapter_match` are fixed-string greps —
  `on_save` matches `on_save_extra`, `LogHabit` matches
  `LogHabitOverflow`. Same structural class as #920 but lower stakes
  (callback/command names are globally unique by the seam-grammar
  law, so cross-plan collision does not arise; prefix-collision
  inside one name family does). Not fixed here: it predates the
  round's mandate and the fix belongs with a seam-grammar review, not
  a gate-correctness drive-by.

## 6. Risks

- **R1 — r9 stage-2 agents write tests to the *current* naming law**
  (bare tokens anywhere in the Test-root). If they do not follow the
  §6 Test-column nomination convention their own plans already
  declare, their rows will read AMBIGUOUS/MISSING under the fixed
  gate and r9's verification would see new failures. Mitigation (in
  this order): the plans already nominate per-plan files (D1 reads
  them); D7's law lands in `docs/ux_planning.org` in the same PR,
  before r9's later chunks rebase over it; AMBIGUOUS is a triage
  state a stage-3 reviewer resolves by adding the nomination, not a
  blocked build. This is the intended fail-loud behaviour, not a
  regression.
- **R2 — inventory churn**: `SEAM_INVENTORY` changes on purpose as
  seams land (its own header says so). During r9, wiring rows may
  read INVENTORY-MISS until the inventory commit lands. Honest
  (stage-2 timing), but stage-3 protocols must treat it as the
  finding class it is, not as a gate bug. The dispatch plan's
  "INVENTORY-MISS … stage-3 protocols already treat as a finding"
  wording anticipates exactly this.
- **R3 — untracked acceptance inputs**: the five r9 plans are
  untracked in the main checkout today (§4 note). If they are still
  untracked at stage-3 time, the acceptance run copies them from the
  main checkout; the committed chunk plan cannot link their blobs —
  path references only.
- **R4 — self-test runtime**: the added fixture variants keep the
  miniature tree; no workspace build is introduced (bash + grep
  only), so gate runtime stays sub-second.

## 7. Estimated diff size

`scripts/ux-coverage.sh` +~180 lines (nomination parser, scoping
engine, bound-test parser, inventory cross-check, counts line,
self-test variants); fixtures +~120 lines (fixture plan variants +
two 2-file token trees); `docs/ux_planning.org` §2.3 +~35 lines.
Total ~330–350 lines, no production-crate code touched.
