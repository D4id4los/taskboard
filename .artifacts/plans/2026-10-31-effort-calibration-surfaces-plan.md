# [Plan] Effort-calibration surfaces (#214) — chunk 6.1 plan (stage 1)

- **Date**: 2026-10-31 (the dispatch prompt's fixed artefact name;
  written 2026-11-03).
- **Kind**: stage-1 TDD plan for chunk 6.1 of backlog round 2, branch
  `feat/effort-calibration-surfaces` (off `origin/main` = `03332937`).
  Zero production code in this commit; implementation is stage 2's
  (dispatch prompt 6.1.b), review + shipping stage 3's (6.1.c).
- **Origin**: dispatch plan
  `.artifacts/plans/2026-10-31-backlog-round2-dispatch-plan.md`
  §1/§8 chunk 6.1, dispatching GitHub issue #214, which rests on the
  ADHD research report's P14 fence
  (`.artifacts/reports/2026-10-14-adhd-support-research-report.md`,
  §5 P14 at :712–730, §11 bookkeeping at :1231–1235) and on the wave
  analysis' §3.4 "Past-entries view + export + statistics view" row
  (`.artifacts/reports/2026-10-14-future-directions-analysis-report.md`
  :133).
- **Gate**: chunk 4.1 (issue #461 "Today virtualisation", branch
  `perf/ui-today-virtualisation`) — merged and verified. The dispatch
  table's "#461 PR" is the *issue* number; the PR carrying the branch
  is **#594**, merged 2026-11-03T10:36Z (`gh pr list --state merged`),
  issue #461 CLOSED, and the branch's content commits (`0618bb4d`
  bench-lane semantics, `18dac217` anchor case 3, `5adf16b6`/`c38f5a28`
  stage reports) are ancestors of `origin/main` (rebase-rewritten
  SHAs). Batch 5 is complete too (#596 `chore/ui-slint-cleanups`
  13:02Z, #598 `feat/pomodoro-focus` 13:54Z), so batch 6 is unblocked
  per the dispatch plan's §batch protocol. Doctor: exit 0 (229 GiB
  free, floor 25 GiB); worktree created with `git worktree add -b
  feat/effort-calibration-surfaces
  ../taskboard-WT-feat-effort-calibration-surfaces origin/main`.

## 0. The premise correction (read this first, stage 2)

The dispatch prompt's step 2 says to read "the review/statistics view
surface (its current composition — the issue's named first home)".
The verified fact is: **there is no review/statistics surface to
read.** The GUI's views are board, today, stack, task, settings
(`crates/taskboard-ui-slint/ui/views/`); the CLI's tracking surface is
`track status` / `track list` / `track export` only
(`crates/taskboard-app/src/commands/track.rs`); no `review` verb or
statistics view exists anywhere. Both prior design rounds that touched
tracking explicitly *reserved* this territory rather than building it:

- The DR-5 time-tracking design round's non-goals: "a today-view
  totals section and task-detail entry history (session ruling:
  backlog issues, not v1); **statistics/charts/PDF (Wave 2 follow-ons
  ride the entries later)**"
  (`.artifacts/plans/2026-10-26-time-tracking-design-round-plan.md`
  §2).
- The tracking-UI implementation plan fenced "#490 today-view totals,
  #491 task-detail entry history — the detail view gains ONE button,
  no list… no entry-listing surface to host them (the CLI owns those
  verbs)" (`.artifacts/plans/2026-10-28-tracking-ui-plan.md` non-goals).

So chunk 6.1 **creates** the review/statistics home; nothing extends
one. That is consistent with the wave analysis' row for it ("Read-only
projections over journal/time entries; pure functions + views") and
with issue #214's own text ("per-task and aggregate estimate-vs-actual
surfaces in the review/statistics views first (pure projection over
time entries — cheap)"). Every other seam named by the prompt exists
and is verified in §2.

## 1. Mission & binding constraints

**Mission**: close the estimate-vs-actual feedback loop's *display*
half (P14: "effort estimates are captured at planning time and
confronted with actuals at review time, so the user's own history —
not optimism — calibrates the next plan"). The surface shows the
user's own historical estimate-vs-actual gap: per task (settled
comparisons) and in aggregate (the gap over a window). Evidence
posture: the planning fallacy is robust even with task experience
(Buehler et al. 1994, fewer than a third finished within their own
"most accurate" prediction — report [26], E1); pomodoro's official
loop already prescribes record-and-compare ([51], E3).

**Binding constraints** (each enforced by a decision in §3):

1. **Local-only**: the projection reads the user's own `AppState`
   history (tasks' effort fields + time entries). No third-party data,
   no telemetry, no network — trivially true for a pure projection,
   and the reason the capture-time suggestion slice stays out (it is
   the slice that would need reference-class search; #214's own text
   defers it).
2. **Pure projection**: no clock reads, no IO inside the projection —
   `now` and window bounds arrive as data (the `tracked_in_window`
   pattern, tracking.rs:195). Tests use fixed stamps; the UI derives
   bounds per refresh pass and passes them in.
3. **Dumb-client rendering**: the surface renders from the published
   `ArcSwap<AppState>` snapshot through the one-way `push` pass; no
   UI-held state, no UI-held timer (the tracking-ui plan's doctrine:
   "elapsed is derived per frame from the ArcSwap snapshot").
4. **Today territory is off-limits.** The today view just went through
   r4 + virtualisation (PRs #589/#594); today-totals is #490's
   reserved territory, not this chunk's. Decision D-1 records the
   today-slot ruling.
5. **The capture-time suggestion slice is OUT.** No reference-class
   hint anywhere in the diff — not a row, not a tooltip, not a hint
   string ("similar tasks took ~45 min" is the *forbidden shape*).
   Stage 3's review names its absence as a trap; any capture-hint text
   in the diff is scope creep to revert.
6. **Empty-state collapse** at every tier of the surface (D-9), using
   the house `EmptyState` widget.
7. **Kiosk gate decided** (D-10) — structural + push-level, per the
   landed precedents.
8. **Quality gates** per AGENTS §10, run through
   `scripts/run-quality-gates.sh`; CHANGELOG per the acceptance line.

## 2. Verified baseline — numbered facts

All facts verified on `origin/main` = `03332937` (2026-11-03). Line
numbers are anchors, not contracts — stage 2 re-resolves after any
rebase.

1. **The estimate side.** `Task.effort: Option<Effort>`, `None` =
   unestimated (`entities.rs:516`); `Effort(u64)` whole seconds,
   unsigned by construction, `Effort::new`/`text_form`
   (`entities.rs:48–60`; text form consumed at
   `commands/tasks.rs:691–695`). Estimates are *current* fields: there
   is no estimate history — the write clocks exist, the prior values
   do not (limitation recorded in D-6).
2. **The actuals side.** `TimeEntry { id, task: TaskId, task_title
   (snapshotted at start), start, end: Option, created_at, updated_at,
   kind: TimeEntryKind }` (`tracking.rs:38–61`). `kind` defaults to
   `Work` on old serialized rows (`#[serde(default)]`,
   `tracking.rs:57–60`). `TrackingState.entries:
   BTreeMap<TimeEntryId, TimeEntry>` (`tracking.rs:109–113`).
   `TimeEntry::duration(now)` = `end.unwrap_or(now) − start` clamped
   ≥ 0 — a skew-produced inverted span contributes nothing
   (`tracking.rs:167–170`). The task reference survives soft delete:
   "history outlives tasks (ADR 0017)" (`tracking.rs:42–44`).
3. **The projection precedent.** `tracked_in_window(entries, window,
   now)` — clip each entry's span to the half-open window, open
   accrual clamped at `min(window_end, now)`, overlap-sum; rows never
   physically split (`tracking.rs:195–213`). `work_in_window` is the
   `Work`-filtered wrapper — the **D-11 ruling (DR-5)**: respite
   entries count against the work streak, *never toward it*
   (`tracking.rs:221–233`). `status_view` uses exactly this for the
   tracked-today total (`commands/track.rs:741–763`).
4. **The review/statistics home does not exist** (§0). `ViewId =
   { Today, Board (default), StackDetail, TaskDetail, Settings }`
   (`view_state.rs:42–51`); Settings is cycle-external; the desktop
   cycle is the order array `[Today, Board, StackDetail, TaskDetail]`
   in `step_view` (`view_state.rs:530–548`); `enter(state, view)` is
   the mount seam (Today enters subjectless — the precedent for a
   subjectless read-only view).
5. **The task-detail tracked summary** (r4 D17):
   `presentation::tracked_total(state, subject, now)` sums
   `duration(now)` over *all* of the task's entries — **no kind
   filter** (`presentation.rs:724–738`); glue
   `tracked_total_phrase` (`presentation.rs:745–755`); rendered as the
   task view's "Tracked" row (`task-view.slint:555–595`); pushed at
   `models.rs:1441–1451`. Respite landed *after* this summary (PR
   #587 21:26Z vs #585 19:48Z, both 2026-11-02), so the summary
   silently counts break activity — a latent D-11 inconsistency
   (ruling of fact 3). The task view shows no estimate anywhere;
   effort chips exist only on cards (`task-card.slint:203–207`, fed by
   `models.rs:1683` + `presentation.rs:251–253`).
6. **The push pattern.** `models::push(window, state, vs, keymap,
   chain, day, calendar, kiosk, reminder_defaults, models, due) ->
   IdTables` — one deterministic, timer-free, one-way full re-derive
   per refresh (`models.rs:684–696`); per-view pushers
   `push_X(...) -> usize` (board :1057, stack :1250, task :1300, today
   :1699); the `View` slint enum is set at `models.rs:738–744`. Pure
   projection helpers live in `presentation.rs` (the mutation-testing
   report pins this split:
   `.artifacts/reports/2026-10-26-mutation-testing-ui-pure-modules-report.md`).
   UI reads the snapshot per pass: `adapter.rs:616` `self.state.load()`
   → `models::push` at `adapter.rs:861`; the engine owns the write
   side (`taskboard-state/src/actor.rs:620–626`).
7. **Empty-state machinery.** `EmptyState { text, hint }`
   (`widgets/empty-state.slint`, 28 lines, token styling, accessible).
   Precedents: `stack-view.slint:61–64`, `today-view.slint:124–127`,
   `board-layout.slint:216–219`.
8. **Kiosk precedents.** `KioskMode::{Board, Today}` — the rotation's
   `view()` mapping is total over exactly those two (`view_state.rs:
   57–74`), so any new `ViewId` is structurally unreachable on the
   kiosk display. Push-level gates exist for surfaces that *are*
   partially shown: `push_notifications` kiosk ⇒ empty rows
   (`models.rs:612–621`), `push_timers` `set_timers_visible(!kiosk)`
   (`models.rs:529–556`), `push_chrome` desktop-only inbox count
   (`models.rs:842–844`); pure gate helpers
   `tracking_chip_action_visible(kiosk)` / `habit_editing_visible(kiosk)`
   (`presentation.rs:607/:3531`); the per-second tick gate
   `compute_seconds_surfaces_active` carries the three-layer exclusion
   (`adapter.rs:787–805`). `ui_settings.kiosk` is threaded as a plain
   `kiosk: bool` parameter into every pusher.
9. **Day windows.** `DayCalendar { day_start_time, offset }`
   (`days.rs:93–99`), `day_window(now, day_start_time, offset)` —
   DST-safe local-day window (`days.rs:55–70`); `window_start` is
   exported (`lib.rs:91`). The composition seam for trailing-N-day
   windows exists; no trailing-window helper exists yet (D-4 adds
   one).
10. **List cost machinery.** The #373 in-house `ListView` mechanism
    (re-pinned by chunk 4.1's PR #594) decouples draw cost from list
    extent; the today sections run inside `row-viewport := ListView`
    instances. The bench (`benches/refresh_draw.rs`) names lanes
    `draw-{full,partial}/{today|board}/{n}` — **view-selective, not
    exhaustive**: adding a view forces no bench change (lane set is a
    bench-internal list, refresh_draw.rs:40–43, :277–:319).
11. **Test harness surface.** Journeys: `tests/journeys.rs` (driver +
    fake engine + fixture + keypress guard; assertion discipline:
    terminal state + focus ownership + typed commands; "Accessible
    labels are element *addresses*; status/error message text is never
    matched"). The cold-start view-graph walk
    (`the_view_graph_walks_from_a_cold_start_without_dead_ends`,
    journeys.rs:1045) covers "every view reachable by action" — a new
    cycle member extends it. Goldens: `tests/golden_visuals.rs` +
    `tests/goldens/`. Domain property tests: `proptest` with in-crate
    strategies (the `tracking.rs` oracle pattern:
    `tracked_in_window_matches_the_boundary_split_oracle_prop`,
    tracking.rs:614).
12. **Domain export conventions.** `lib.rs` re-exports per module
    (`pub use tracking::{…}`, `pub use days::{…}` — lib.rs:91,
    :155–158); every new domain file carries the MIT OR Apache-2.0
    SPDX header; the crate is `#![forbid(unsafe_code)]`.

## 3. The decisions

### D-1 — Surface home: a new `ViewId::Review`, cycle-member, desktop-only by routing

Add `ViewId::Review` (`view_state.rs`), a new
`ui/views/review-view.slint`, a `push_review` pusher, and a
`presentation.rs` row-shaping layer. The desktop cycle order becomes
`[Today, Board, StackDetail, TaskDetail, Review]` — Review joins the
cycle (it is a first-class reading surface, not chrome; Settings stays
cycle-external). `enter(state, Review)` is subjectless exactly like
Today's (no `ContextId` subject, no engine command). `KioskMode` is
untouched — the rotation maps only Board/Today, so kiosk routing can
never select Review (fact 8, layer (a)).

**The today-slot decision** (the dispatch table's "the today slot
decision per the ADHD plan"): the Today view gains **no** calibration
section in this chunk. Three independent reasons, recorded so nobody
re-litigates: (1) P14's build direction names "review/statistics
surfaces", not the today view; (2) today-totals is #490's reserved
territory (DR-5's session ruling, fact 4's reservation record) —
folding a tracked-today section into this chunk would swallow another
chunk's scope; (3) today territory is busy (r4 rework + virtualisation
both landed this week) and the dispatch plan's UI-territory rule keeps
this chunk off it. The review view is the home; #214 closes with the
review surface; a future today-section idea belongs to #490's
disposition, not here.

**Per-task surface location**: the review view's per-task rows are the
per-task surface (issue #214: "per-task and aggregate estimate-vs-actual
surfaces in the review/statistics views first" — both surfaces live in
the review view). The task-detail view is **unchanged** by this chunk
except for the one-line kind fix of D-7; showing estimate-beside-total
in the detail view is a follow-up idea (§7, out-of-scope list).

### D-2 — The projection lives in `taskboard-domain` (`calibration.rs`)

New module `crates/taskboard-domain/src/calibration.rs`, re-exported
from `lib.rs` beside `tracking`. Rationale: the semantics (which tasks
count, which entries sum, what is excluded) are domain semantics, not
UI row-shaping — the CLI could consume the same projection later (a
future `review` verb), and the property tests belong at the lowest
layer that owns the behavior (AGENTS §5 "one behavior, one test,
lowest layer"). The UI's `presentation.rs` only shapes phrases/rows
*from* the minted structs. The module header carries the SPDX line;
the projection is pure: `&AppState` + window bounds + `now` in, typed
report out, no IO, no clock, no runtime — the `tracked_in_window`
discipline (fact 3).

Proposed shape (guidance, not a straitjacket — the compiler may
negotiate details, the *laws* of §4 may not move):

```rust
pub struct CalibrationRow {
    pub task: TaskId,
    pub title: String,          // the live task's title
    pub done: DateTime<Utc>,    // the completion stamp that admitted it
    pub estimate: Option<Effort>,
    pub actual: chrono::TimeDelta, // Σ Work durations, lifetime (D-3)
    pub tracked: bool,          // ≥ 1 Work entry exists (observed actual)
}
impl CalibrationRow {
    /// `Some(actual − estimate)` iff both sides are observed
    /// (`estimate.is_some() && tracked`) — the settled comparison.
    pub fn gap(&self) -> Option<chrono::TimeDelta>;
    pub fn compared(&self) -> bool; // == self.gap().is_some()
}

pub struct CalibrationAggregate {
    pub estimated_seconds: u64,   // Σ estimates over compared rows
    pub actual_seconds: u64,      // Σ actuals over compared rows
    pub gap_seconds: i64,         // actual − estimated (signed; > 0 = under-estimate)
    pub compared: usize,
    pub skipped_unestimated: usize, // tracked completions with no estimate
    pub skipped_untracked: usize,   // estimated completions never tracked
}

pub struct CalibrationReport {
    pub window: (DateTime<Utc>, DateTime<Utc>),
    pub rows: Vec<CalibrationRow>,    // sorted: done desc, task asc (D-5)
    pub aggregate: CalibrationAggregate, // == reduce(rows) by construction
}

pub fn calibrate(state: &AppState, window: (DateTime<Utc>, DateTime<Utc>),
                 now: DateTime<Utc>) -> CalibrationReport;
```

One entry point; the aggregate is derived from the same row vector the
surface lists — the "no parallel tally" trap is then structural (the
aggregate literally folds the rows), and a test still pins
`aggregate == fold(rows)` so future drift cannot reintroduce a second
path.

### D-3 — Per-task semantics: completed-in-window, lifetime actuals, Work-only

- **Membership**: rows are tasks with `done: Some(d)`, `d ∈ window`
  (half-open `[start, end)` — the `tracked_in_window` boundary
  convention), `deleted == false` (tombstones never surface — the
  estimate/actual pair of a deleted task is deliberately not
  resurrected; ADR 0017's history-outlives-tasks is honored by leaving
  deleted tasks out of UI lists, not by re-showing them).
- **Admission**: a task appears as a row iff it has **either** an
  estimate **or** at least one `Work` entry (`tracked`). A completed
  task with neither is noise and renders nowhere.
- **Actuals are lifetime, not window-clipped**: the actual is Σ
  `duration(now)` over the task's `TimeEntryKind::Work` entries of
  *all time*. The window selects tasks by *completion date*; it does
  not clip their work. Clipping would understate every actual (a task
  completed Tuesday whose work happened last week would show a
  fragment), and it would break the single-source law against the
  detail view's total (which is lifetime). This is the reading of
  "the actuals sum over the same entries the detail view shows".
- **Kind filter: `Work` only.** The D-11 ruling (DR-5) is the recorded
  semantics: respite counts against the work streak, never toward
  effort. Calibration asks "how long did the work take" — break
  activity marked respite is not work. Pinned by a property law (§4
  L6).
- **Open entries of a completed task** (possible: the done flip does
  not force-stop a running timer) contribute `elapsed-to-now`,
  exactly as `tracked_total` does — same entries, same accrual.
- **Title**: the live task's current title. (Entries' snapshotted
  `task_title` exists, but the row is a projection of the task; the
  detail view the row's drill-down would open shows the live title.
  Renames therefore re-label history rows — accepted; the alternative
  pins a stale label against the task the user can still see.)

### D-4 — Window shape: day-aligned trailing windows, two fixed sizes (7 and 30 days)

- **New domain helper** `days::trailing_days_window(now, day_start_time,
  offset, days) -> (DateTime<Utc>, DateTime<Utc>)`: the half-open
  window covering the last `days` local day-windows *including the
  one `now` falls in* — `start = window_start(today_date − (days−1))`,
  `end = window_start(today_date + 1)`. Composing from `window_start`
  per day keeps it DST-safe (fact 9) and reuses the exported seam; no
  new date arithmetic is invented. Unit tests pin: the window
  containing `now` is included; day_start_time and offset are honored;
  a DST-transition span produces still-correct bounds.
- **Why day-aligned**: the app's whole reading vocabulary (tracked
  today, today view, day windows) is local-day-based; "the last 7
  days" means calendar days to a reviewer, not `now − 168h`.
- **Why two fixed sizes**: the aggregate wants *a* recent horizon and
  *a* longer trend; 7 and 30 are the smallest defensible pair. A
  config knob is a v2 decision (out-of-scope list, §7); fixed constants
  keep the surface and its goldens deterministic. The window sizes are
  named constants in the UI layer (`REVIEW_WINDOWS_DAYS: [7, 30]`
  style), not config.
- **Models-layer plumbing**: `push_review` derives both bounds from
  `DayCtx`/`DayCalendar` + `now` via the helper and passes them into
  `calibrate` as data (constraint 2 — the projection itself never
  reads a clock).
- **Aggregate membership: completed-tasks-only** (vs. all-touched —
  the prompt's named open question). Only *settled* comparisons
  calibrate: an open task's actual is still accruing, so its gap is a
  moving target and its row would drift under the user's cursor while
  they read it. In-flight work remains visible in its existing homes
  (today view, task-detail tracked total, track status). Pinned as a
  law (§4 L2's contrapositive: a task with `done: None` yields no row
  at any window position).

### D-5 — Aggregate semantics: compared-only gap, visible skips, signed gap

- **The gap counts only compared rows** — tasks with **both** sides
  observed (`estimate.is_some() && tracked`). The two exclusion rules
  are symmetric and both are *exclusions*, never zeros:
  - **No estimate ⇒ excluded from the gap, not counted as zero.**
    Zero-counting an unestimated task would drag `estimated_seconds`
    down and masquerade as a perfect over-performance; the task is
    real work with an unobserved plan. Its work *is* acknowledged —
    as `skipped_unestimated` and as a row (D-3 admission).
  - **No actuals ⇒ excluded from the gap, not counted as zero.**
    Zero-actual would read as an over-estimate ("finished instantly")
    when the truth is *unobserved* (the user never tracked). Counted
    as `skipped_untracked`, rendered as a row with an untracked
    actual.
  Both rules are property-pinned (§4 L3, L4) — stage 3's review trap
  names exactly these.
- **The skips are visible, not silent**: the aggregate carries
  `skipped_unestimated` / `skipped_untracked` counts so the gap's
  denominator is honest ("3 of 5 finished tasks compared" is the
  reading). The counts are part of the typed aggregate, so the surface
  cannot drop them without dropping the type.
- **Gap sign**: `gap_seconds = actual_seconds − estimated_seconds`
  (i64, signed). Positive = under-estimated (planned less than taken —
  the planning-fallacy direction); negative = over-estimated. The
  phrase layer renders direction structurally (§D-8); the *typed* sign
  is the contract, tests pin the sign convention on the struct, not on
  wording.
- **Zero-length entries**: a `Work` entry of zero (or clamped-inverted)
  duration still marks the task `tracked` — the user *observed* the
  work, the observation summed to ~0. A completed task whose only
  entries are zero-length **is compared** with actual 0. Unit-pinned
  (§4 unit list).
- **Sorted determinism**: rows sort `done` descending (newest
  completion first — a ledger reads newest-up), tie-break `task`
  ascending. Goldens and snapshots depend on this.

### D-6 — Known limitation, recorded: the estimate compared is the current one

`Task.effort` is a current field (fact 1) — there is no estimate
history. A task re-estimated *after* completion is compared against
its post-hoc estimate. This is accepted for v1: the field is the
task's estimate of record, estimating-after-the-fact is itself
calibration-relevant behavior, and modeling estimate history is a
domain extension far beyond this chunk. The limitation is stated here
and in the module doc comment so a future reader does not mistake it
for a bug.

### D-7 — Fold the `tracked_total` kind fix into this chunk

`presentation::tracked_total` (fact 5) sums all kinds — a latent D-11
inconsistency: the r4 D17 summary (PR #585) predates respite's landing
(PR #587) by two hours and missed the filter that `status_view`'s
tracked-today total applies via `work_in_window`. This chunk's
single-source law ("the surface's actuals derive from the same entries
the detail view shows") is only expressible against a kind-consistent
universe, so the fix rides with the chunk: `tracked_total` filters to
`TimeEntryKind::Work` (one line), plus a unit test pinning that a
respite entry never inflates the detail view's total. Per AGENTS §5
("a new test failing can be a bug"), this is the production bug case:
the fix is production code, the new pin is the test. The existing
flow-table test (`the_task_detail_model_carries_the_tracked_total_…`,
flow_table_tests.rs:1162) uses Work-only fixtures and is unaffected.

### D-8 — Surface composition: two aggregate sections, one ledger, read-only rows

`review-view.slint` renders, top to bottom:

1. **Aggregate sections** — one per window, 7-day then 30-day, each a
   compact stat row: estimated-total phrase, actual-total phrase, gap
   phrase with direction (over/under/on-target — *structural*
   rendering of the sign: positive/negative/zero; wording is UX copy,
   tests assert structure not text per the house discipline), and the
   comparison counts ("N compared" + skips when nonzero). Phrases are
   second-granular like the task view's `tracked_total_phrase` (r4
   D17's precision contract; consistency over the CLI's
   minute-granular `duration_phrase`, which is a different consumer).
2. **The ledger** — ONE per-task row list over the 30-day window (the
   wider horizon; the 7-day window's members are its prefix by
   completion date and speak through the aggregate section). Row
   content: title, completion day phrase, estimate phrase (or the
   unestimated dash), actual phrase (or the untracked dash), per-row
   gap phrase when compared. Rows render through the in-house
   `ListView` mechanism (fact 10) — membership is window-bounded but
   not otherwise capped, and the board/today precedent is to mount the
   mechanism rather than argue about row counts.
3. **Rows are read-only**: no activation, no context actions, no walk
   targets — a ledger, not a task list. The view's walk/focus story:
   the view participates in the view-graph walk (journeys.rs:1045's
   contract — every view reachable, no dead ends) with page-level
   scroll only (`ScrollPageHint` machinery); rows are plain text rows
   and carry no focus container. This is pinned explicitly so stage 2
   does not invent row actions (a drill-down into task detail is on
   the out-of-scope list, §7).

Phrase glue lives in `presentation.rs` beside `tracked_total_phrase`
(fact 6's pure-module split); `models.rs` gains `ReviewModel` wiring +
`push_review` in the `push()` pass; the `View` slint enum and
`ContextId` mapping gain their `Review` variants (the ContextId
variant exists for the keymap context resolution even though no action
targets it yet).

### D-9 — Empty-state collapse: three tiers

Using the house `EmptyState` (fact 7):

1. **Whole view**: when the 30-day row set is empty AND both
   aggregate sections are empty (no comparable tasks in either
   window), the view collapses to `EmptyState` — the same shape as
   today's "nothing due" collapse (`today-view.slint:124–127`).
2. **Per-window aggregate**: a window with no comparable tasks renders
   its section in the collapsed/absent-dash form (the section does not
   vanish structurally — the two-section skeleton stays stable so the
   view does not reflow as windows age out; the *values* collapse).
3. **Ledger**: an empty ledger hides the list (no ghost chrome), with
   the whole-view tier taking over when it is the last thing standing.

Each tier is pinned by a push-level test (§4 UI list) — the dispatch
acceptance line "empty-state collapses" maps to these three.

### D-10 — Kiosk gate: structural + push-level; no ticking surface

Three layers, matching the strictest landed precedent (fact 8):

- **(a) Structural** — `KioskMode` untouched; the kiosk rotation
  renders Board/Today only, so Review is unreachable on the appliance
  display. Pinned by a journey assertion that the kiosk cycle never
  lands on Review.
- **(b) Push-level belt** — `push_review` emits the empty model when
  `kiosk` is true (the `push_notifications` banner-only precedent,
  models.rs:612–621). Cost: one guard. Benefit: even a future routing
  bug cannot project the ledger onto the kiosk display, and the
  property "kiosk ⇒ no calibration model" is testable without routing
  the kiosk anywhere new.
- **(c) No per-second surface** — the projection derives per refresh
  pass from stamped data; nothing ticks, so
  `compute_seconds_surfaces_active` gains no term (the open-entry
  accrual in a row updates on the next refresh, which is the same
  behavior the task view's frozen-when-closed total exhibits —
  fact 5's test at flow_table_tests.rs:1162 pins that class already).

### D-11 — CHANGELOG: two entries

Under `[Unreleased]`: one `### Added` entry for the calibration
surfaces (#214), one `### Fixed` entry for the tracked-total kind
filter (D-7). Both follow the existing entry style (user-visible
behavior, no implementation narration).

### D-12 — Out of scope, named so nobody re-derives it

- **The capture-time reference-class suggestion** ("similar tasks took
  ~45 min" at capture/planning time): #214's own later slice, local
  -only when it comes. *Nothing* of it appears in this chunk — no
  hint strings, no suggestion rows, no estimate prompts. This is
  stage 3's named trap; the forbidden shape is any capture-hint text.
- **Today-view totals / a today calibration section**: #490's
  territory (D-1).
- **Task-detail entry history**: #491's territory.
- **Task-detail estimate-beside-total display**: follow-up idea, not
  in this chunk (the detail view was reworked in r4; its territory is
  not this chunk's).
- **Window-size configuration**: fixed 7/30 constants (D-4); a knob is
  a v2 decision.
- **A CLI `review` verb**: natural follow-up consumer of the domain
  projection, not in this chunk (the chunk is the GUI surface; the
  domain module makes the verb cheap later).
- **Estimate history** (at-completion estimates): not modeled (D-6).
- **Charts/graphs**: the wave analysis notes "statistics wants a chart
  story in the GUI" — that is a separate design decision; this chunk
  ships stat rows and a ledger, no chart machinery.

## 4. TDD catalogue — stage 2's commits, in order

Every commit standalone (AGENTS §8 commit hygiene); tests first within
each commit pair where the listed test is expected to fail against the
then-current tree (the TDD red step), production code in the same or
the immediately following commit per the house habit of
`test(...)` → `feat(...)` pairs. Property tests use `proptest` with
strategies composed from `taskboard-domain`'s `test_support` module
(the tracking_strategies precedent); every law names its failure
symptom so a red test is diagnosable.

### Commit pair 1 — the domain projection (laws first)

`test(domain): the calibration projection's laws` (red) →
`feat(domain): the effort-calibration projection`.

New `calibration.rs` + `days::trailing_days_window` + `lib.rs`
re-exports. The laws:

- **L1 — Single source (the oracle law).** For any state, for any
  task: `CalibrationRow::actual` equals an *independently written*
  sum — filter `state.tracking.entries` by `entry.task == row.task`
  and `entry.kind == Work`, map `duration(now)`, sum — the same
  oracle-split discipline as
  `tracked_in_window_matches_the_boundary_split_oracle_prop`
  (tracking.rs:614). *Failure symptom*: a parallel tally (filtering by
  title, or re-deriving durations differently) diverges on generated
  states.
- **L2 — Window membership, half-open.** A task with `done == start`
  is admitted; `done` arbitrarily close below `start` is not; `done ==
  end` is not. Membership keys on `done` only — a task with `done:
  None` yields no row however many entries it has. *Failure symptom*:
  boundary off-by-one or open tasks leaking into the ledger.
- **L3 — The no-estimate exclusion.** For a compared row, flipping
  `effort: Some(e) → None` leaves `aggregate.estimated_seconds`
  unchanged (that row's estimate leaves the sum) and decrements
  `compared` by one and increments `skipped_unestimated` by one — the
  task is *never* folded as zero. *Failure symptom*: an unestimated
  task dragging the estimated total (the zero-count bug this law
  exists to forbid).
- **L4 — The no-actuals exclusion.** Removing all `Work` entries of a
  compared row moves it out of the gap (`compared` −1,
  `skipped_untracked` +1, `actual_seconds` loses exactly that row's
  contribution) without removing the row from the ledger (it stays,
  admitted by its estimate). *Failure symptom*: untracked completions
  counted as instant work.
- **L5 — Lifetime actuals.** An entry starting long before the window
  contributes its full duration to its task's actual (the window
  never clips entries — only admission is windowed). *Failure
  symptom*: window-clipping the spans (copying `tracked_in_window`'s
  clip into the wrong place).
- **L6 — Respite never counts.** Appending `TimeEntryKind::Respite`
  entries of arbitrary spans to any task leaves every row, every
  aggregate field, and `tracked` unchanged. *Failure symptom*: the
  D-11 ruling violated.
- **L7 — Open-entry accrual passes `now` through.** A compared task
  with an open entry has `actual ≥ closed-span-sum`, and its actual is
  monotone non-decreasing in `now` while the entry stays open. *Failure
  symptom*: dropping open entries or freezing them at their start.
- **L8 — Aggregate == fold(rows).** `aggregate.compared ==
  rows.iter().filter(|r| r.compared()).count()`; estimated/actual
  seconds equal the sums over compared rows; `gap_seconds ==
  actual_seconds as i64 − estimated_seconds as i64`. *Failure
  symptom*: a second aggregation path drifting from the ledger.
- **L9 — Determinism.** The report is a pure function of `(state,
  window, now)` and rows sort `done` desc, `task` asc — the same
  generated state yields the identical row order across runs. *Failure
  symptom*: map-iteration order leaking into the ledger.

Unit pins beside the laws: zero-length-only task is `tracked` and
compared with actual 0 (D-5); tombstone (`deleted`) exclusion (D-3);
neither-side task has no row (D-3 admission); empty selection yields
empty rows + zero aggregate (the whole-view collapse's domain half);
`trailing_days_window` bounds (D-4: inclusion of now's window,
day_start_time/offset honored, DST span).

### Commit pair 2 — the kind fix

`test(ui-slint): the tracked total counts work only` (red) →
`fix(ui-slint): the task view's tracked total applies the work filter`.

One-line filter in `presentation::tracked_total` + the respite pin
(D-7). The existing flow-table test stays green (Work-only fixture).

### Commit 3 — the view plumbing

`feat(ui-slint): the review view joins the cycle`.

`ViewId::Review` + cycle order `[Today, Board, StackDetail,
TaskDetail, Review]` + subjectless `enter` + the `View`/`ContextId`
slint-enum mappings. Tests: the cycle reaches Review and returns
(the view-graph walk extends — journeys.rs:1045's journey updated
*consciously*, its no-dead-ends contract now covering the new view);
kiosk rotation never offers Review (the D-10(a) pin). Any
cycle-length-derived snapshot re-pins here, deliberately, with the
change reviewed in the diff (risk R3).

### Commit pair 4 — the push layer

`test(ui-slint): push_review projects the published state` (red) →
`feat(ui-slint): push_review and the review phrases`.

`push_review` + `presentation.rs` phrase/row glue + `ReviewModel`
wiring. Tests (push-level, against published state through the
existing test rig — fact 6):

- rows in the model mirror `calibrate`'s output for the same state
  and window bounds (the models layer forwards, never re-derives —
  the dumb-client line);
- `kiosk: true` ⇒ empty model (D-10(b));
- empty-state tiers D-9: whole-view flag when everything is empty;
  per-window collapsed values when a window has no comparable tasks;
  ledger-hidden when its list is empty;
- phrase structure: gap phrases distinguish the three sign classes;
  unestimated/untracked dashes are structural absences (empty
  phrase), not zero phrases.

### Commit 5 — the surface

`feat(ui-slint): the effort-calibration surfaces`.

`review-view.slint` (aggregate skeleton + ListView ledger +
EmptyState), `app-window.slint` mounting. Goldens: an empty-state
golden and a populated-ledger golden (naming per
`golden_visuals.rs`'s convention); a journey entering the review view
through the cycle and asserting terminal render state (the journeys'
assertion discipline: element addresses and structure, never status
text — fact 11). Optionally (only if the virtualisation suite's
structure invites it) a review-ledger pin in `virtualisation.rs`
following the today precedent — not required for acceptance.

### Commit 6 — bookkeeping

`docs(changelog): the effort-calibration surfaces`.

The two CHANGELOG entries (D-11). Then the full gate suite per
AGENTS §10 through `scripts/run-quality-gates.sh` (fmt, clippy -D
warnings, nextest, doc tests, deny, the llvm-cov sequence with
quarantine + the trailing rm).

## 5. Milestones

- **M1 — the domain projection is green**: laws L1–L9 + unit pins +
  `trailing_days_window`; `calibrate` consumed by nothing yet. The
  semantics are then frozen; later milestones only shape.
- **M2 — the universe is kind-consistent**: D-7's fix + pin.
- **M3 — the surface exists**: plumbing (commit 3), push layer
  (commit 4), composition + goldens + journeys (commit 5).
- **M4 — handoff-ready**: CHANGELOG, full gate suite green, per-gate
  results in stage 2's report.

## 6. Acceptance checklist (maps 1:1 to the dispatch plan's)

| Dispatch acceptance | Where |
|---|---|
| "the projection is pure + property-tested" | D-2, §4 commit 1 (L1–L9; no clock, no IO — bounds/now as data) |
| "the surface renders from published state" | D-8, §4 commit 4 (push-level tests against the snapshot; adapter.rs:616 read path) |
| "empty-state collapses" | D-9's three tiers, pinned in §4 commit 4 |
| "kiosk gate decided" | D-10 (structural (a) + push belt (b) + no tick (c)), pinned in commits 3–4 |
| "CHANGELOG" | D-11, §4 commit 6 |
| "NO capture-time suggestion" | D-12's first bullet — the forbidden shape named; stage 3 re-checks the diff for any hint text |

## 7. Risks, resolved questions, and the out-of-scope ledger

**Risks**

- **R1 — models.rs rebase friction.** The dispatch plan warns exactly
  this for chunk #214 ("must expect rebase friction in `models.rs`
  and follow the §2 rebase dance"); models.rs is the week's hottest
  file (five PRs touched it since 2026-11-02). Mitigation: this
  plan's models.rs footprint is one new pusher + one model struct +
  two enum variants — small and appenditive; rebase at dispatch time,
  resolve in the push-call region only.
- **R2 — the view-graph walk and focus machinery must absorb a new
  view.** The walk journey (journeys.rs:1045) and the walk-facts
  machinery assume the current view set. Mitigation: Review is
  read-only with no focus containers (D-8), the simplest shape the
  walk can visit; commit 3 updates the walk journey deliberately and
  its diff shows exactly what changed.
- **R3 — cycle-length-derived snapshots.** Adding a cycle member can
  shift snapshots that pin cycle order/length (keymap or state
  snapshots). Mitigation: commit 3 owns every such re-pin, consciously,
  in one commit — never scattered.
- **R4 — the respite fix's blast radius.** `tracked_total` has one
  consumer (task detail) and one direct behavior test (Work-only
  fixture, unaffected). Any *other* test that turns red under the
  filter is signal (a fixture quietly relying on respite-in-total) and
  is triaged as a bug finding, not silenced.
- **R5 — `now` in completed tasks' open entries.** A completed task
  with a still-open timer accrues in its row until the entry closes
  (D-3). This is the tracked_total behavior already in production;
  the ledger merely shows the same truth. No new machinery.
- **R6 — the estimate-history limitation** (D-6): post-completion
  re-estimates shift settled comparisons. Recorded, not solved.

**Resolved questions** (the prompt's list, each answered above):
window semantics → D-4 (day-aligned trailing 7/30, bounds derived per
pass, passed as data); completed-tasks-only vs all-touched → D-4
(completed-only, settled comparisons; open work stays in its existing
homes); rolling-window shape → D-4 (two fixed sizes, half-open,
DST-safe composition); degenerate no-estimate → D-5 (excluded from
gap, visible as row + skip count, never zero); degenerate no-actuals →
D-5 (excluded from gap, visible as row + skip count, never zero;
zero-length-but-observed *is* compared at 0); zero-length spans → D-5
(tracked, contribute 0; inverted clamp to 0 by the domain read); the
surface composition → D-8 (two aggregate sections + one 30-day
ledger, read-only rows); empty-state collapse → D-9 (three tiers);
kiosk gate → D-10; today slot → D-1 (no today section; #490's
territory); capture-time suggestion → D-12 (out, the forbidden shape
named).

**Estimated diff size**: ≈ 900–1,300 lines across ~18 files —
domain ~400 (calibration.rs with laws ~300, days.rs +40, lib.rs +2),
ui-slint ~700 (view_state ~25, models ~120, presentation ~100,
review-view.slint ~180, app-window ~15, tests/journeys/goldens ~250,
misc rig/fixtures ~30), CHANGELOG ~25. Consistent with the batch
table's M size.

**Artefact hygiene**: this plan lives under `.artifacts/plans/` per
AGENTS §7; it is the chunk's normative implementation contract for
stage 2 and stage 3's review baseline. Issue #214 stays open until
stage 3 closes it with the self-contained comment (the capture-time
slice stays tracked by the issue's own text, per the dispatch plan's
bookkeeping note).
