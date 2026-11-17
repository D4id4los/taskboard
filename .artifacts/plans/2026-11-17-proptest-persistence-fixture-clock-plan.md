# [Plan] C2 — Proptest persistence in CI + journey_state fixture clock (#857 + #843)

- **Date**: 2026-11-17
- **Kind**: stage-1 chunk plan (dispatch chunk C2 of the
  2026-11-17 backlog-reduction round; round plan §3-C2 is normative
  for this file).
- **Branch**: `test/proptest-persistence-fixture-clock` (created off
  `origin/main` @ `44c6f32b`; worktree
  `../taskboard-WT-test-proptest-persistence-fixture-clock`).
- **Issues**: #857 (CI-ephemeral proptest persistence), #843
  (journey_state Today-empty), #650 (armed via #857's mechanism).
- **Laws in force**: typed outcomes only; no sleeps (mock clock /
  injected instant); never weaken a correct test; out-of-scope
  findings become `backlog:` issues.

## 1. Verified seam inventory

- **Workflow**: `.github/workflows/ci.yml` — required check
  *Test Execution* (job `test`), step "Run Tests":
  `cargo nextest run --workspace --no-tests=warn`. The job already
  has the checkout + toolchain + rust-cache preamble an upload step
  needs; no other workflow runs the workspace suite (nextcloud
  integration and miri are separate files; miri deliberately sets
  `PROPTEST_DISABLE_FAILURE_PERSISTENCE=1` — untouched here).
- **Regression writes**: proptest persists minimized counterexamples
  to `crates/taskboard-domain/proptest-regressions/*.txt`
  (per-crate `proptest-regressions/` dir next to the crate; the
  workspace today only has that one checked-in dir, but the upload
  globs `**` so new crates cost nothing). Failure persistence
  triggers only after the case fails all configured retries — with
  nextest's failure-retry shard (issue #650 observed exactly
  "failed twice within the job"), the *second* failure's persistence
  run is what lands the file. That is why the upload must key on
  the job outcome, not step-retry counting.
- **Fixture seams** (`crates/taskboard-ui-slint/tests/support/fixture.rs`):
  - `journey_state()` (line ~344) stamps `task1.duedate` to
    "yesterday at UTC midnight" from `chrono::Utc::now().date_naive()`
    — a raw wall-clock read, not the day-calendar vocabulary.
  - `today_overdue_state_at(now)` (line ~586) is the reference
    pattern: a single captured `now`, the stamp derived through
    `DayCalendar` + `review_window(ReviewCadence::Day, yesterday, …)`
    — the same vocabulary the harness's day window reads.
  - `PINNED_NOW` (line ~390) + `UiSettings::clock` (lib.rs ~109,
    default `None`): the render-content freeze. `Harness::mount`
    (journeys.rs ~56) passes `UiSettings::default()` ⇒ `clock: None`
    ⇒ `Adapter::day_ctx()` (adapter.rs ~1347) reads the live wall
    clock per pass: `now = Utc::now()`, window =
    `day_window(now, settings.day_start_time, boot_offset)`.
  - The adapter's calendar is the `(day_start_time, boot_offset)`
    pair re-derived per pass; `boot_offset` is the device's fixed
    UTC offset. Fixtures hardcode a UTC calendar
    (`FixedOffset::east_opt(0)`). On a UTC-offset-zero dev box these
    coincide; on any other device the "which day is yesterday"
    answer differs between fixture and pass (recorded as backlog —
    see §6).
- **Failing-origin test**: `crates/taskboard-ui-slint/tests/journeys.rs::us6_s2_today_row_menu_and_preset_key_recommit`
  (line ~19825). Today it mounts `fixture::today_overdue_state()` —
  the #843 workaround, in-tree. Deliverable: it passes against
  `journey_state()` again.
- **Domain oracle** (`crates/taskboard-domain/src/days.rs::today_sections`,
  ~209): the overdue section = `reminders::overdue_tasks(tasks, now)`
  = `is_live ∧ is_open ∧ duedate < now`. Note: the overdue predicate
  itself is wall-clock-proof (`due < now`, any run) — which is
  exactly why the #843 observation (all four sections `[0,0,0,0]`)
  is *suspected, not diagnosed* in the issue. The diagnosis step
  below is therefore mandatory before the fix shape is final.

## 2. Decision D1 — #857 persistence mechanism (default accepted)

**Mechanism**: `actions/upload-artifact@v7`, `if: failure()`, on the
*Test Execution* job of `ci.yml`.

- **Path**: `**/proptest-regressions/**` (every crate's dir; the
  `.prop_` persistent-file state, if any run ever enables it, lives
  under the workspace root — covered by a second path entry
  `.prop_*/**` added only if a diagnosis shows the default config
  writes it; proptest's default failure persistence is the
  regressions file, the `.prop_` state is opt-in per config, and the
  workspace's shared `test_support::proptest_config` does not enable
  it).
- **Retention**: `retention-days: 90` (≥ 30 required; 90 matches the
  flaky-watchlist triage cadence — a counterexample found near a
  release gap still outlives the watchlist pass that collects it).
- **Discovery**: the step appends one line to
  `$GITHUB_STEP_SUMMARY` naming the artifact (typed fact, not a
  message assertion anywhere in test code — §5 does not apply to
  workflow YAML, but the summary is deliberately a *link + artifact
  name*, so the triaging agent finds it from the job page without
  log-diving).
- **Scope**: only the `test` job. Miri keeps its persistence
  disablement; coverage runs the same suite via llvm-cov but the
  canonical artifact attaches where the required check lives — if a
  coverage-lane counterexample matters, the next run of `test`
  reproduces it (same seeds are not guaranteed across lanes; that is
  acceptable, the required check is the oracle).
- **Not chosen**: a fail-comment bot (needs a token with PR-comment
  scope in CI — a new credential surface the repo does not otherwise
  grant; artifact latency is irrelevant since triage is a human/agent
  pass, not a hot loop). Rejected and recorded per the round plan's
  "or a recorded better alternative" clause: no better alternative
  found; the default stands.

**Decision D1a — canary proof method**: a *deliberately-failing
temporary test*, not a disabled/skipped one — a `#[ignore]`d or
always-skipped test cannot fail, so it cannot prove the `if:
failure()` trigger. Shape:

1. A commit on this branch adds
   `crates/taskboard-domain/tests/canary_persistence.rs`: a proptest
   whose property is trivially false for one generated input
   (deterministic counterexample; retries exhausted ⇒ the regression
   file is written by the runner itself).
2. Push; the *Test Execution* required check goes red; capture from
   that run: (a) the artifact exists with the regression `.txt`
   inside, (b) the job summary carries the link. Both recorded in
   the PR description (evidence block) and on #857.
3. A follow-up commit **removes the canary** — it does not survive
   the merge. The mechanism (workflow step + summary line) survives.
   Recorded: **canary removed post-capture; step + summary survive.**
   The red-then-green commit history on the branch is the audit
   trail; the PR opens only after the removal commit lands locally
   (handoff gate stays "PR open + checks green").

**Decision D1b — #650 arming**: after the mechanism lands, comment
on #650: mechanism (artifact-on-failure, 90d retention, summary
link), what the triaging agent does when the next CI occurrence
fires (download artifact → paste counterexample into
`proptest-regressions/appdata.txt` → replay locally → fix or
re-arm). The issue **stays open** — the counterexample has not
occurred yet; the comment records the armed state.

## 3. Decision D2 — #843 option A design (anchor on the harness's clock)

**Decision: option A** (round-mandated). The fallback (option B:
document Today-empty-by-construction and redirect journeys to
`today_overdue_state`) activates **only** if stage 2's diagnosis
finds the harness clock is not injectable at the fixture layer —
a recorded finding, never a silent switch.

**Which clock seam `journey_state` anchors on**: the harness's day
window reads `Adapter::day_ctx()` — with `UiSettings::default()`
(`clock: None`) that is `Utc::now()` through the
`(settings.day_start_time, boot_offset)` calendar pair. The fixture
cannot share the *pass's* instant (the pass re-reads per repaint),
so the anchor is the **vocabulary**, not the instant: derive the
overdue stamp through the same `DayCalendar`/`review_window`
construction the pass derives its window from, seeded from a single
captured `now`.

**Injection path** (test-support only; zero production change):

1. Rename the body to `journey_state_at(now: DateTime<Utc>) ->
   (Fixture, AppState)`; `journey_state()` becomes the single-read
   capture wrapper (`let now = chrono::Utc::now();` captured once,
   passed down — testing_strategy.org §6.1 tool 6). The due stamp is
   `calendar.today(now).pred_opt()`'s midnight in the fixture's
   UTC calendar, mirroring `today_overdue_state_at` line-for-line —
   the two fixtures become the same idiom, differing only in board
   shape. Midnight-alignment stays (round-3 D9: date-only due
   phrases).
2. The precondition is asserted in the fixture's contract test
   (`bench_fixture_contract.rs` or a sibling — stage 2 picks the
   file the existing fixture tests live in): given
   `journey_state_at(now)`, the domain oracle
   `today_sections(&state.tasks, now, window.bounds())` yields
   `overdue == {task1}` for any pass `now ∈ [now, today-window end)`
   — typed set-membership, no text. This is the
   Today-nonempty-by-construction proof; it is what makes the
   harness day window and the fixture *agree by construction*: both
   sides classify through `today_sections`, the fixture's stamp is
   derived from the same window vocabulary, and the stamp sits
   strictly before the window start.
3. Retarget `us6_s2_today_row_menu_and_preset_key_recommit` to
   `fixture::journey_state()` (reverting the #843 workaround switch)
   and delete the workaround's framing comment. `today_overdue_state`
   keeps its own callers — nothing else moves.

**Why the diagnosis step is mandatory first**: the overdue predicate
(`due < now`) is satisfied by the current fixture for any pass time,
yet the chunk-4 run observed `[0,0,0,0]`. Either the observation
predates a since-landed fix, or the emptiness originates in a
different input of the today pass (the review-window bounds, the
reflection pin `golden_state_at` plants, the kiosk/board-focus
filter, or the UTC-vs-`boot_offset` calendar divergence — §1, §6).
Stage 2 must reproduce (mount `journey_state()`, press ViewToday,
count the four sections) and pin the *actual* failing input with a
typed precondition before editing the fixture. If reproduction shows
`journey_state` already renders non-empty on current `main`, the
deliverable collapses to steps 1–3 (the by-construction fix) **plus**
a reproduction of the original observation against the
chunk-4-origin commit recorded in #843 — an observation that no
longer reproduces gets a dated comment on #843 and the
by-construction fix still lands (it removes the fragile raw
`Utc::now().date_naive()` read either way; never weaken, never
close on "works for me").

**Fallback record (option B)**: trigger condition = the diagnosis
shows the today pass's section input is not derivable from anything
the fixture can construct (i.e. no fixture-side stamp makes the
precondition test hold). Then: keep the `journey_state_at`
parameterization (it is correct regardless), document the
Today-empty-by-construction property on the fixture doc comment,
keep `us6_s2…` on `today_overdue_state`, and record the finding on
#843 before closing.

## 4. Acceptance

- [ ] Canary evidence block in the PR description (artifact present,
      summary line present) + #857 comment; canary commit removed on
      the branch tip.
- [ ] `ci.yml` Test Execution carries the failure-keyed upload
      (retention 90) + summary line; no other workflow touched.
- [ ] `us6_s2_today_row_menu_and_preset_key_recommit` passes against
      `journey_state()` (or the recorded option-B fallback).
- [ ] Fixture contract test proves the overdue section via
      `today_sections` for the parameterized `now` (typed).
- [ ] No production code change outside the fixture/harness seams
      (round plan §3-C2 acceptance).
- [ ] #650 carries the armed-mechanism comment and stays open.
- [ ] Pre-commit hooks green; the flock-wrapped
      `scripts/run-quality-gates.sh cargo nextest run --workspace`
      run green (or the deferral reported per the #879 law).

## 5. Non-goals

- No `PROPTEST_` env/config change in the workspace test support
  (miri's disablement stands).
- No retry-count, seed, or case-count tuning of the proptest
  configs (#650's flakiness itself is C3-adjacent watchlist work,
  not this chunk).
- No `boot_offset` fixture work (§6 backlog).
- No new fixture variants beyond `journey_state_at` +
  `journey_state`.

## 6. Risks & out-of-scope findings

- **Risk (D1)**: `upload-artifact` globs over a clean tree when the
  failure is non-proptest — the step must tolerate
  no-match (artifact upload with a missing path fails the step
  unless `if-no-files: warn` is set; the plan pins
  `if-no-files: warn` so an unrelated failure does not grow a
  second red X). Recorded as a stage-2 checklist item.
- **Risk (D1a)**: the canary's red run is on a *required* check —
  acceptable: it never reaches `main`; the branch head handed off is
  green. If the round's dispatcher prefers zero red runs, the
  canary can run in a fork-style `workflow_dispatch`-only lane, but
  that proves a different trigger path; the on-failure trigger of
  the real check is the thing being proven. Decision stands.
- **Risk (D2)**: the `[0,0,0,0]` observation may not reproduce —
  handled above (dated comment + by-construction fix still lands).
- **Backlog finding (filed)**: the journey fixtures derive their day
  vocabulary in a hardcoded UTC calendar while the adapter's
  `calendar()` uses the device `boot_offset` — on any non-UTC-offset
  device "yesterday" differs between fixture and pass. Harmless
  today (dev box is UTC; overdue is offset-insensitive), a latent
  anchor divergence per testing_strategy.org §6.1. Issue: see §7.
- **Estimated diff size**: ~S — `ci.yml` (+~15 lines),
  `fixture.rs` (~40 lines refactored), one fixture-contract test
  (~25), journeys.rs retarget (~6), canary commit + removal
  (net 0), issue comments. No production diff at all.

## 7. Backlog issues opened by this stage

Listed here and reported; bodies carry a permalink to this plan.

- #884 — the UTC-calendar vs `boot_offset` fixture/adapter anchor
  divergence (labels `testing`, `dx`, `p3`).
