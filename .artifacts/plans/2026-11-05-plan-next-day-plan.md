# [Plan] v0.2 Chunk 3 — the plan-next-day surface (#622)

- **Date**: 2026-11-05.
- **Status**: PROPOSED — stage-1 plan for chunk 3 of the v0.2
  focus-reflection dispatch. Branch `feat/plan-next-day`, cut from
  `main` @ `58ae9c55` (`scripts/worktree-doctor.sh` passed
  pre-creation: 220 G free against the 25 G floor; worktree
  `../taskboard-WT-feat-plan-next-day` created with
  `git worktree add -b`).
- **Origin** (normative): the v0.2 implementation plan §5.2
  (`.artifacts/plans/2026-11-04-v0.2-focus-reflection-implementation-plan.md`)
  + ruling **R-6** (interactive placement in v1 — pick, set
  scheduled/effort, place boxes; time-box *arithmetic* deferred) +
  **VF-9** (the today projection is the shared read — no parallel
  projection may grow beside it) + **VF-7** (undo arms exist; nothing
  new); issue **#622** + its 2026-11-04 ruling comment (interactive
  placement; effort-sum budgeting deferred); the ADHD report's
  **P13** (review at temporal landmarks; make re-commitment cheap)
  and **P4** (box time deliberately — the day is blocks with starts).
- **Scope guard**: `taskboard-domain` (the next-day derivation in
  `days.rs` + its tests), `taskboard-ui-slint` (view state, actions,
  router, adapter, models, presentation, the new view, keymap,
  journeys, snapshots), plan + CHANGELOG. **Not in scope**: any
  `taskboard-state` / `taskboard-storage-sqlite` /
  `taskboard-sync-nextcloud` / `taskboard-app` change (the write
  target `StateCommand::UpdateTask` exists; the app already injects
  the day-start pair); time-box budget arithmetic (R-6 defers it);
  the review walk itself (#621, chunk 9+ — this chunk only lands its
  reachability seam); P9 forgiveness affordances (the walk owns
  them); organiser/calendar territory (W3); a kiosk passive display
  (ruled out, D5).
- **All `file:line` anchors are `main` @ `58ae9c55`**; every one below
  was read, not carried over.

## 0. Findings → mechanisms audit (what the code confirms)

The single most load-bearing finding: **everything this surface
writes and reads already exists and is proven.** The write target is
`StateCommand::UpdateTask { id, changes: TaskChanges }`
(`crates/taskboard-domain/src/messages.rs:44`) whose `TaskChanges`
carries the double-option slots `scheduled: Option<Option<DateTime>>`
and `effort: Option<Option<Effort>>` (`messages.rs:547`, clear vs
untouched documented at the fields) — a placement is ONE command with
both masks set, one journal step. The undo arm is already pinned:
`UpdateTask` journals PreImages
(`crates/taskboard-domain/src/undo/tests.rs:263`). The read side is
`days.rs`'s pure day machinery: `day_window`
(`days.rs:55`), `DayCalendar { today, calendar_date, instant }`
(`days.rs:118`), and `today_sections(tasks, now, window)`
(`days.rs:199`) — the four-section classifier whose first-match
precedence and canonical orderings are prop-pinned. The chunk's real
work is: (a) the next-day window composition, (b) the candidate-pool
derivation over that window, (c) the surface + its placement
interaction, (d) the reachability seam.

- **The adapter already owns the day vocabulary.** It holds
  `day_start: (NaiveTime, FixedOffset)` (`adapter.rs:151`), derives
  `DayCalendar` (`adapter.rs:926`) and a fresh `DayCtx { now, window }`
  on every push pass (`day_ctx`, `adapter.rs:939`) — rollover rides
  the existing minute-boundary repaint, no timer. The planner's pass
  derives the next-day window from the same held pair; nothing new is
  injected, `taskboard-app` is untouched.
- **The today pass is the push-pass template.** `push_today`
  (`models.rs:2192`): read `today_sections` → filter (anchor tasks,
  kiosk board scope) → build row models (phrases derived in
  `presentation.rs`; slint computes nothing) → diff into persistent
  `MirroredRows` handles → fill `IdTables` (`tables.today_tasks`, the
  focus walk's domain). A `push_plan_next_day` is this shape with the
  planner's two sections.
- **The off-cycle view pattern is proven twice.** `ViewId::Settings`
  is excluded from the `step_view` cycle (`view_state.rs:559` — the
  cycle is exactly `[Today, Board, StackDetail, TaskDetail, Review]`),
  `enter()` refuses it (`view_state.rs:629`), it is opened only by
  `ActionId::AppSettings → UiEffect::OpenSettings`
  (`router.rs:794`, the interpretation row at `adapter.rs:2008` gated
  on `Flow::None`), and it records `settings_return`
  (`view_state.rs:321`) to restore the exact return view. Journeys
  J3/J4 pin both behaviors
  (`tests/journeys.rs:3010`–`:3055`: close restores StackDetail; the
  full cycle walk never lands on Settings). `ViewId::Review` shows
  the subjectless-view arm (`enter` at `view_state.rs:595` is
  subject-free) and the kiosk exclusion pin
  (`the_kiosk_rotation_never_lands_on_review`, `view_state.rs:2562`;
  `KioskMode::view`'s mapping, `view_state.rs:74`, is total over
  Board/Today only). The new surface joins these two precedents.
- **Time-of-day entry machinery exists.** `due_edit::parse_time_lenient`
  (`adapter.rs:6350`, `:6377`) parses box times; `due_edit::quarter_slots()`
  (`adapter.rs:1213`) feeds the picker grid; `Effort::parse_text_form`
  (`entities.rs:75`) parses estimates (`30`, `1h30m`; overflow = None).
  A placement form reuses these parsers verbatim.
- **Row affordance discipline is settled.** Pointer-grade row verbs
  ship as registry actions with no preset bindings, dispatch-time
  gating only (the habits verbs' rule, `actions.rs:281`–`301`); the
  kiosk gate empties affordances and the planner's idempotent refusals
  own the rest. The today view's section body is deliberately inlined
  into the section loop's delegate (`today-view.slint:9`–`14`, the
  #373 model-through-child-component instantiation trap) — the
  planner's sections follow the same proven shape.
- **Subject-affine flows over lists are proven.** `Flow::TrackEntries/
  TrackEntryForm` (`view_state.rs:225`–`240`): mount gated on the
  subject, `reconcile` drops the flow when the subject dies, the
  Escape ladder (form → list → close) resolves against the current
  flow (`router.rs:66`–`72`), refresh never touches text buffers
  while a flow holds them (the D9 rule 4). The placement form is this
  pattern's third tenant.
- **Keymap snapshots are documentation-as-a-test** (`tests/keymap_snapshots.rs:6`–`12`):
  registry + resolved-preset dumps, reviewed by hand, regenerated via
  an explicit `INSTA_UPDATE=always cargo nextest run -p
  taskboard-ui-slint` run recorded in the commit message.

## 1. Decisions (stage-1 settlements)

### D-1 — Surface shape: a subjectless `ViewId::PlanNextDay`, OFF the view cycle (the Settings precedent)

A new `ViewId::PlanNextDay` variant (`view_state.rs:42`), a new
`plan-next-day.slint` view mounted conditionally in
`app-window.slint` (the `current-view` block, `app-window.slint:302`–`403`;
new `View.plan-next-day` enum variant at `:73`).

**View, not flow/overlay**: the surface needs its own persistent
keyboard domain — two panes (placed boxes, candidate pool), reorder
verbs, a form ladder — and must survive until deliberately closed and
be re-enterable mid-ritual. Flows are transient, XOR-held editors over
a workspace (the D9 rule); the planner *is* a workspace for the
minutes of the ritual. The overlay tenants (capture, focus mode,
track entries) are all momentary relative to this.

**Off the cycle, Settings-style — not Review-style cycle membership**:
Review is read-only, so a cycle stop through it is harmless; the
planner is the wave's only *interactive editing* workspace, and a
cycle stop would put the view walk's focus into a placement-capable
context on every trip from Today to Board (an accidental-placement
hazard, and the opposite of R-7's gentle posture). Deliberate entry is
also what P13 asks for: the ritual is *chosen* at the landmark.
Mechanics mirror Settings exactly: `step_view`'s cycle list stays
5-long, `enter()` refuses the variant, opening rides a routed action
(D-4), `planner_return: Option<ViewId>` (the `settings_return` twin)
restores the exact return view on close.

### D-2 — The candidate-pool derivation: today's projection, evaluated as of tomorrow's opening (pure, in `days.rs`)

Two new pure functions in `crates/taskboard-domain/src/days.rs` —
compositions over the existing machinery, **not** a parallel
projection (VF-9 verbatim: the section read routes through
`today_sections` unchanged):

1. `next_day_window(now, day_start_time, offset) -> (DateTime, DateTime)` —
   the window *after* the one containing `now`:
   `(day_window(now,…).1, +24h)`. Total, deterministic, DST-safe by
   the fixed-offset rule (the `trailing_days_window` composition
   precedent, `days.rs:80`).
2. `plan_next_day(tasks, now, day_start_time, offset) -> PlanNextDaySections` —
   literally `today_sections` **evaluated at `next_start`**:
   `today_sections(tasks, next_start, next_day_window(…))`, plus one
   complement pass. The as-of-tomorrow's-opening evaluation is the
   semantic heart: an open task due later *today* is carried (a
   candidate to re-commit), never "already placed"; a task due
   tomorrow 03:00 under a night-owl 05:00 start is likewise carried
   (the window rule decides, not the calendar date).

The returned struct (ids only, the `TodaySections` discipline):

- `placed_scheduled: Vec<TaskId>` — the scheduled section verbatim,
  ordered `(scheduled, order, id)`. The day's boxes.
- `placed_due: Vec<TaskId>` — the due section verbatim, ordered
  `(order, id)` (must finish tomorrow; already committed by the due
  field).
- `carried: Vec<TaskId>` — the overdue-at-open section verbatim (open
  ∧ `duedate < next_start`): yesterday's misses, visibly pickable.
  No ceremony attached (P9 rides the walk); the card's own overdue
  decoration communicates the miss.
- `candidates: Vec<TaskId>` — live ∧ open ∧ in **no** section above:
  the live pool (inbox, stacks, unplaced everything), ordered
  `(order, id)`.

The four sets partition every live open task; tombstoned and done
tasks appear nowhere. Both `#622` and the chunk-9 walk consume this
one read; the CLI/REPL legs share it too.

### D-3 — The placement interaction: pick → place (one mask) → swap boxes; no arithmetic

Two-pane layout: **placed boxes** first (the day shape, P4 — rows in
scheduled order, each showing time-of-day + effort phrases), then the
**candidate pool** (carried + candidates, one list). Keyboard walking
reuses the view's reading order (`reading_order`, `view_state.rs:510`
— the planner returns its own row list; the focus machinery is
view-agnostic).

1. **Place** — `Activate` on a candidate opens `Flow::PlanPlace
   { task: TaskId }` (new flow variant, the `TrackEntryForm`
   precedent: subject-affine, reconcile drops it on subject death,
   buffers survive refresh). Inline form: time-of-day (date pinned to
   tomorrow — "no horizon beyond the next day", §5.2) + optional
   effort. Commit funnels **one** `UpdateTask { scheduled:
   Some(Some(calendar.instant(next_date, t))), effort: Some(Some(e)) }`
   (effort mask present only when entered) — atomic placement, one
   undo step. Parsers reused verbatim (§0): `parse_time_lenient`,
   `Effort::parse_text_form`. Empty effort = mask absent (untouched).
2. **Reorder** — `PlanBoxEarlier` / `PlanBoxLater` on a placed row
   swap its scheduled instant with the temporal neighbor's (two
   `UpdateTask` writes, each journal-armed; the display re-sorts).
   Swap-instants means reordering can never create or need arithmetic
   — the instant set is invariant (R-6 honored: the surface places,
   it does not compute load; boxes may share an instant and ties
   render honestly in `(scheduled, order, id)` order). Undo steps a
   swap back as its two honest writes (accepted; D-7).
3. **Unplace** — `PlanRemoveBox` on a placed row routes
   `UpdateTask { scheduled: Some(None) }`; the task returns to the
   pool on the next pass.
4. **Escape ladder** — form → pane → close view (the TrackEntryForm
   ladder's three rungs; closing the view restores `planner_return`).

Re-opening placement on an already-placed row edits that box (the
form seeds from the row's instant) — the same funnel, the same mask
shape.

### D-4 — Reachability: one routed action; the review-walk seam is command-level

- `ActionId::ViewPlanNextDay` (registry row: Global context,
  Navigation/ViewNav category) → `UiEffect::OpenPlanNextDay`
  (`router.rs:30` family) → interpretation row
  `(Flow::None, OpenPlanNextDay) => view_state.open_plan_next_day()`
  — the `(Flow::None, UiEffect::OpenSettings)` row's twin
  (`adapter.rs:2008`). A held flow keeps it (the modal discipline);
  Escape out of the flow first.
- **The walk contract (what chunk 9+ may rely on)**: the review
  walk's final leg dispatches `ActionId::ViewPlanNextDay` — the same
  routed intent a keypress takes. The walk reads and writes no
  planner-internal state; the only shared medium is `AppState` via
  the engine's command stream (the §5.2 seam verbatim: "reachable by
  command, no shared state beyond AppState"). This chunk pins the
  seam's dispatchable side; the walk itself does not exist yet.

### D-5 — Kiosk posture: excluded, not degraded

v1 shows nothing on kiosk. `KioskMode` stays Board/Today — the total
`view()` mapping can never select the planner (the review pin's
mechanism, `view_state.rs:2562`; extend that test's assertion to the
new variant). The open intent is structurally refused on kiosk at the
adapter gate (the `TrackEntriesOpen` kiosk refusal precedent, D-F13).
No passive tomorrow display in v1 (§5.2: "passive display only, if
any" — we rule none; a future one would extend the today pass, not
this surface). Editing is desktop-only.

### D-6 — ActionId / keymap growth (the registry discipline)

- New `ContextId::PlanNextDay` (`actions.rs:26`) — a non-masking view
  context like `Today`; the placement form's text fields ride the
  existing `ViInsert` masking (no new mask context — the entries
  overlay needed `TrackEntries` only because its *rows* bind while
  mounted; the planner's walking verbs live in the view context).
- Registry additions (`ActionId`, `actions.rs:187`):
  `ViewPlanNextDay` (Global context; preset-bound), `PlanBeginPlace`,
  `PlanCommitPlace`, `PlanBoxEarlier`, `PlanBoxLater`, `PlanRemoveBox`
  (`PlanNextDay` context; pointer-grade row verbs follow the habits
  rule — registry rows with no preset bindings, dispatch-time
  gating). Every row carries label/description/menu_group so the
  registry dump stays the action documentation.
- Standard preset: exactly ONE new binding — the open action (stroke
  chosen at implementation next to `AppSettings`/`FocusToday`; the
  snapshot regen documents it). Menu: one ViewNav-group entry.
- Snapshots regen: `INSTA_UPDATE=always cargo nextest run -p
  taskboard-ui-slint keymap_snapshots`, recorded in the commit
  message, diff reviewed by hand.

### D-7 — Undo: nothing new (VF-7)

Every placement write is `UpdateTask`; the PreImages journal arm is
already pinned (`undo/tests.rs:263`). `AppUndo` steps a placement,
an edit, or an unplace back one write; a swap steps back as its two
writes (honest clocks; no new journal vocabulary, no compound
command). A journey pins place → undo → the box returns to the pool.

### D-8 — Day-window awareness (VF-4) and push-pass purity

The adapter derives the next-day window fresh on every pass from the
held pair (the `day_ctx` precedent, `adapter.rs:939`): midnight
rollover advances "tomorrow" on the existing minute-boundary repaint
— proven by two `push` calls with a stepped `DayCtx`, no timer, no
sleeps. `push_plan_next_day` (`models.rs`, the `push_today` shape):
read `plan_next_day` → anchor-filter at the row source (the today
pass's rule) → row models with phrases derived in `presentation.rs`
(time-of-day phrase, effort phrase — reusing the existing phrase
helpers where present) → diff through two `MirroredRows` handles →
fill `IdTables::plan_rows` (the focus walk's domain). Slint computes
nothing. The placement form's commit funnel follows `commit_date`
(`adapter.rs` due-edit funnel): parse → mask → command outcome, the
view never sees a `StateCommand`.

## 2. TDD order and test catalogue (red first, one behavior one test, lowest layer)

### Phase 1 — domain derivation (`days.rs` + its test modules; no UI)

Units (hand-computed fixtures, the D7a–d discipline):
1. `next_day_window` is exactly today's window advanced: today's
   `.1` == next `.0`, next spans 24h (consecutive-window pin).
2. `next_day_window(end_of_today, …) == next_day_window(now, …)` —
   the composition is stable across the boundary instant.
3. Night-owl + offset honored (the D7b fixture stepped a day).
4. DST boundary instants: composed bounds across every
   `dst_boundary_instants()` fixture (the `trailing_days_window`
   pin's mirror).
5. `plan_next_day` placement: scheduled-in-window task lands in
   `placed_scheduled`; due-in-window lands in `placed_due`.
6. The as-of-open pin: a task due later *today* is `carried` (in
   candidates), not placed.
7. The night-owl edge pin: due tomorrow 03:00 with a 05:00 start is
   `carried`, not `placed_due`.
8. `carried` == open ∧ `duedate < next_start` (misses are visible
   picks).
9. Partition: `placed_scheduled ∪ placed_due ∪ candidates` covers
   every live open task exactly once; `carried ⊆ candidates`; done
   and tombstoned tasks appear nowhere.
10. Orderings: `(scheduled, order, id)` / `(order, id)` per section
    (the P5 prop's mirror).

Props (`proptest`, the module's existing configs):
11. `next_day_window` contains `day_window(now).1` and is 24h;
    stepping `now` across today's boundary flips both windows by one
    day (the stepper prop mirrored); equal inputs, equal outputs.
12. The partition + ordering props over generated task maps (the
    `span_with_tasks_strategy` shape re-used with the next-day span).

### Phase 2 — view state, actions, router (pure UI logic)

13. `open_plan_next_day` records the return view and flips;
    `close_plan_next_day` restores the exact return view
    (`settings_return` twin; unit).
14. `step_view` cycle exclusion: next/prev never land on
    `PlanNextDay` from any cycle member; `enter()` refuses it (the
    J4 pin as a unit; the journey repeats it at layer 4).
15. Router row: `ViewPlanNextDay → UiEffect::OpenPlanNextDay` (the
    `AppSettings` row's twin).
16. Flow-table rows: `(Flow::None, OpenPlanNextDay)` opens; every
    held flow leaves the effect inert (the modal discipline, the
    OpenSettings settle rows' shape — `flow_table_tests.rs`).

### Phase 3 — push pass + presentation

17. `push_plan_next_day` model test: fixture with placed + carried +
    candidate tasks — the placed pane shows them in scheduled order
    with phrases; the pool excludes placed; `tables.plan_rows` filled
    (the focus walk's domain).
18. Rollover: a second push with `now + 24h` moves the window —
    yesterday's tomorrow becomes today's, boxes re-derive (the
    `push_today` two-push precedent).
19. Presentation: the box time phrase and effort phrase units (pure;
    typed outcomes, no text assertions beyond structural facts the
    phrase functions own).

### Phase 4 — journeys, kiosk pins, snapshots (layer 4; routing only)

20. J-P1 open/close: `ViewPlanNextDay` from the board → view is
    Plan; Escape → StackDetail exactly (the J3 shape).
21. J-P2 cycle exclusion journey (J4's list unchanged — assert the
    walk still never lands on Plan; the cycle stays 5-long).
22. J-P3 placement: select candidate → begin-place → type time +
    effort → commit → `assert_commands(&[UpdateTask { … }])` with
    both masks set (typed masks at the forwarder boundary); the row
    moves to the placed pane; `AppUndo` returns it to the pool.
23. J-P4 reorder: two placed boxes; `PlanBoxEarlier` routes the two
    `UpdateTask` swaps; display order flips; undo ×2 restores.
24. J-P5 view-graph walk: Plan reachable from the board by its
    action, escapable — no dead ends (the cold-start walk extended).
25. Kiosk pins: the rotation never renders the new view (unit, D-5);
    the kiosk dispatch of `ViewPlanNextDay` no-ops (structural
    exclusion assert, the entries-overlay refusal's pattern).
26. Keymap snapshot regen (registry + resolved presets) — reviewed
    by hand, regen recorded in the commit message.

**Engine note**: no `taskboard-state` tests — nothing new routes
there; placement semantics are `UpdateTask`'s, already tested at the
engine and undo layers (the one-behavior-one-test-lowest-layer rule;
the journeys assert *routing*, not engine semantics).

## 3. Implementation order (commits on `feat/plan-next-day`)

1. `feat(domain)`: `next_day_window` + `plan_next_day` + the Phase-1
   catalogue (pure, red first).
2. `feat(ui-slint)`: view-state arms + actions/registry/context +
   router row + interpretation rows + kiosk refusals + Phase-2 tests.
3. `feat(ui-slint)`: `push_plan_next_day` + presentation phrases +
   Phase-3 tests.
4. `feat(ui-slint)`: `plan-next-day.slint` + `app-window.slint`
   mount + the PlanPlace flow funnel + journeys + snapshot regen.
5. `docs(changelog)`: the Unreleased entry (house rule) — may ride
   commit 4.

Each commit stands alone (builds, meaningful subject — the
rebase-merge hygiene rule). Hand-off at the quality gates green +
PR open (one PR; the gates run through
`scripts/run-quality-gates.sh` in the worktree).

**Stage-3 amendment (deviation from D-3, reviewed 2026-11-05):** the
planner's keyboard row walking rides the view's Tab traversal (the
slint `FocusScope` chain over box/pool rows — every row's verbs are
pointer-grade with no preset bindings, so a `focus_domain` arm over
`tables.plan_rows` would address no bound verb) and D-3's other
walking sentence — "the planner returns its own row list" — is
honored by `IdTables::plan_rows`/`plan_position` (the seams'
resolution base), not by `ViewState::focus_domain`. The board-side
`reading_order` walk is *not* wired for `ViewId::PlanNextDay`: the
planner context binds no board-adjacent row verbs, so a stray
`nav.next-task` there moves a highlight no bound verb can act on —
the placement interaction itself is affordance-driven by design.
Wiring a planner focus domain remains available when the review
walk's keyboard leg (chunk 9+) needs it; it was deliberately not
grown speculatively here. D-3's form-seed sentence *was* honored at
review: re-opening a placed box seeds the time field from the row's
instant (the stale-stamp guard seeds nothing; pinned by
`the_placed_box_reopen_seeds_the_form_from_the_instant`).

## 4. Risks

- **The as-of-tomorrow's-opening semantics surprise**: a task due
  later today shows as carried, not placed. Mitigation: pins 6–7 name
  the rule; the carried set is *visible* (nothing disappears; the
  card's overdue decoration marks it). If the user round disagrees,
  it is a one-constant change (the evaluation instant), not a shape
  change.
- **Swap-as-two-writes doubles undo steps** for one perceived move.
  Accepted for v1 (honest per-field clocks; a compound command would
  be new engine vocabulary — a deviation needing a ruling, not a
  silent addition).
- **Recurring tasks**: placing a recurring head writes `scheduled` on
  the head — exactly what the task view's scheduled editor does
  today; the planner adds no recur special-casing and inherits the
  existing semantics verbatim. No new behavior, but the journey
  fixture should include one recur-bearing task to keep the surface
  honest about what it shows.
- **Flow reconciliation over a non-task view**: `PlanPlace` is
  subject-affine; `reconcile` must drop it on subject death but must
  *not* drop it because the view is not TaskDetail (the entries
  overlay's mount gate keys on the subject, not the view — reuse that
  exact predicate).
- **Snapshot churn**: registry + preset snapshots grow by design;
  reviewed by hand per the harness's rule, never blind-accepted.
- **Review-view-style growth pressure**: the surface must stay a
  focused workflow (no calendar grid, no horizon beyond tomorrow —
  §5.2); the pane pair is the whole layout.

## 5. Estimated diff size

≈ 1.7k–2.4k lines: domain ~250 (two fns + catalogue), view_state
~180, actions/router ~120, adapter ~250 (interpretation + kiosk gates
+ commit funnel), models ~300 (pass + tables + tests), presentation
~70, `plan-next-day.slint` ~350, `app-window.slint` ~40, keymap
presets ~40, journeys ~350, snapshots regen small. An M-sized chunk,
five commits.
