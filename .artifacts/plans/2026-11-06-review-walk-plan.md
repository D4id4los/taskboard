# [Plan] The review walk and its gentle surfacing (v0.2 completion chunk 3, #638)

- **Branch / worktree**: `feat/review-walk` @ `../taskboard-WT-feat-review-walk`,
  off `origin/main` @ `bfc735c3`.
- **Origins (normative)**: DR-7 round doc
  `.artifacts/plans/2026-11-05-journal-review-design-round-plan.md`
  §3 Parts E, F, I (D-J8, D-J9 incl. the shape-B amendment and the C9
  history, D-J10, D-J14) + §5; dispatch plan
  `2026-11-05-v0.2-completion-dispatch-plan.md` §4; issue #638; ADHD
  report `2026-10-14-adhd-support-research-report.md` P9/P12/P13.
- **Closes**: #638, #621 (the DR-7 round completes with this PR).
- **Non-goals**: interactive week/month walks (#640), the boundary
  push-notification leg (#641), any reminder/tick/persistence
  machinery (D-J10 declined it), statistics surfaces (#623 slice B),
  mood/energy/flag entry in the walk (CLI-only in v0.2, see D-W9),
  renaming the planner surface.

## 1. Verified base (2026-11-06)

| Precondition | Evidence |
|---|---|
| Chunk 1 (reflection domain) landed | `9adaf678`…`d4002a5f`: `reflection.rs` ships `ReviewCadence`, `ReviewWindow` (A1 mint), `review_window`, `window_misses`, `window_done`, `DEFAULT_TEMPLATES` (`daily-close`, four prompts), `ReflectionState::live_entries`, `StateCommand::RecordReflection { cadence, day, window, text, mood, energy, flags }`. **`review_due` was not built** — it lands here (§3.5). |
| Chunk 2 (storage + journal CLI) landed | `2314b3a5`…`bfc735c3`. |
| #622 plan-next-day landed | `a60bed3f`…`8600407f`; issue CLOSED. It shipped as a **view**, `ViewId::PlanNextDay` (not a flow), opened by `ActionId::ViewPlanNextDay` → `UiEffect::OpenPlanNextDay` (toggle; `Ctrl+Shift+P`; the planner-context `Escape` row rides the same route) → `ViewState::open_plan_next_day`/`close_plan_next_day` with a `planner_return` view slot. Kiosk refuses at `adapter.rs` (`(_, UiEffect::OpenPlanNextDay) if self.kiosk`). Its title is `Plan next day` (see D-W12). |
| GUI round 5 | Landed: multi-line editor (`MultiLineEditor` widget; the single-line field retired its multiline flavour), the window-anchored flyout conversion (F9/F14) + journeys, the ListView row-anchored compensation on the Today sections. Plan-only (implementation not on main at base): the dead-seams chunk. This chunk **consumes** `MultiLineEditor` (prompt fields) and the anchored flyout (the `File to…` stack picker) and touches the Today view only above its sections' `ListView` (D-W8). Stage 3 rebases onto whatever else lands. |

## 2. Decisions

### D-W1 — The flow shape (one deviation, recorded)

```rust
Flow::ReviewWalk { cadence: ReviewCadence, reference: NaiveDate, stage: WalkStage, cursor: usize }
```

Subjectless XOR flow (the `Capture`/`Palette`/`FocusMode` precedent):
`reconcile` leaves it alone; opening any other flow replaces it
without committing; opening it replaces any other flow.

**Deviation from the doc's `{ cadence, stage, cursor }`**: the flow
carries the reviewed window's **reference date**. Reason: the window
must not drift when the wall clock crosses the day boundary mid-walk
(a walk opened at 03:58 with a 04:00 boundary would otherwise
silently re-target the next day). The window is re-derived on every
read as `review_window(cadence, reference, calendar)` — the doc's
"windows are data" and VF-9 hold (no stored instants in the UI, one
derivation). Cadence-genericity is structural: every walk fn takes
`cadence` from the flow; only the *openers* pin `Day` (#640 widens the
openers, nothing else).

### D-W2 — The stage machine

```rust
pub enum WalkStage {
    Inbox { filing: Option<StackId> },        // Some = the File-to picker is armed on the cursor row
    CarryForward { picking_date: bool },      // true = the Pick-a-date field is armed on the cursor row
    Prompts,
}
```

Transitions (all pure `ViewState` methods returning `Vec<FlowOutcome>`):

| From | Trigger | To |
|---|---|---|
| (any flow) | `open_review_walk(cadence, reference, rows)` | `Inbox` (or the first non-empty stage — empty-skip below) |
| `Inbox` | cursor passes the last row (Keep on the last row; File removes the row at refresh, the cursor clamps) / `review.walk-advance` | `CarryForward` |
| `CarryForward` | cursor passes the last row / `review.walk-advance` | **handoff** (D-W3) → flow `None`, planner view mounted |
| planner closes (return hint present) | adapter | `Prompts` |
| `Prompts` | `Record` / `Finish without recording` | `Flow::None` |
| `Inbox{filing: Some}` / `CarryForward{picking_date: true}` | Escape | same stage, sub-state cleared |
| `Prompts` | Escape | `CarryForward` (never back into the planner — P12: no forced detour) |
| `CarryForward` | Escape | `Inbox` |
| `Inbox` | Escape | `Flow::None` (close, no command) |

The Escape ladder is sub-state → stage-back → close (the
`TrackEntryDismiss` shape). **Empty stages auto-advance structurally**
(DR-7 D-J9): the advance/open transition skips a stage whose row set
is empty, and the walk's progress strip renders the skipped stage's
header with its b-line (C5b/C7b) — deterministic, no dwell timer.
An empty Carry forward still performs the handoff (the planner is
useful regardless of misses). `cursor` is the stage's row index,
clamped against the pushed row count on refresh (the palette rule).

### D-W3 — The handoff transition (shape B) and the return hint

- **Seam**: the walk's advance out of `CarryForward` is an adapter
  arm that (1) sets the adapter-local
  `walk_resume: Option<WalkResume { cadence, reference }>`, (2) calls
  `view_state.close_review_walk()` (flow `None` — the walk is not
  committed, there is nothing to commit), (3) re-runs
  `apply_effect(UiEffect::OpenPlanNextDay)` through the existing
  `(Flow::None, UiEffect::OpenPlanNextDay)` arm (the R3
  settle-then-run). **The planner's code is untouched** — the walk
  reaches it exactly the way the action does; zero shared state
  beyond `AppState` (the #622 "reachable-by-command" contract). If the
  view is already `PlanNextDay` (the walk was opened over the
  planner), step 3 is skipped (the toggle would close it).
- **Return**: the planner's close route (`(Flow::None,
  OpenPlanNextDay)` while `view == PlanNextDay`) gains one tail: after
  `close_plan_next_day()`, if `walk_resume.take()` is `Some`, call
  `view_state.open_review_walk_at(cadence, reference, Prompts)` over
  the restored return view. Placed tasks reach Prompts as plain state.
  Closing the planner without placing anything is the built-in skip
  (P12) and resumes the same way (P9: nothing lost, nothing forced).
- **Abandon by navigation**: if the view leaves `PlanNextDay` by any
  other route (palette jump, `nav.show-task`, `view.today`, settings),
  the post-effect reconcile drops `walk_resume` — the user went
  elsewhere; the walk never ambushes them later. Opening any flow over
  the planner leaves the hint alone (the planner is still there).
- **Planner-absent degrade**: not built — the planner is on the base,
  so the skip-the-leg branch would be dead code (the doc's degrade was
  conditional on base state).

### D-W4 — Per-stage data (pure projections, presentation.rs)

All rows are derived on every push from `AppState` + the flow's
window; the view state holds only the cursor/sub-state.

- **Inbox** — `walk_inbox_rows(state) -> Vec<TaskId>`: the same filter
  `presentation::inbox_count` applies (live ∧ open ∧ not an anchor ∧
  in the inbox board), refactored into one shared
  `inbox_tasks(state)` that `inbox_count` now `.len()`s — no second
  inbox reading. Cadence-independent (the inbox has no window).
  Row verbs: `File to…` arms `filing: Some(first candidate stack)`
  over an anchored flyout listing every live non-inbox stack in
  board/stack order (left/right or pointer select); commit dispatches
  `StateCommand::MoveTask { id, stack, order }` with the order the
  existing move flow computes for an end-of-stack drop (reuse that
  helper; no new arithmetic). `Keep in inbox` dispatches nothing and
  advances the cursor.
- **Carry forward** — `walk_carry_rows(state, cadence, window, now)`:
  for `Day`, `today_sections(&state.tasks, now, window)` read
  **verbatim** as the ordered, de-duplicated union `overdue ++
  scheduled ++ due_today` (first occurrence wins; no re-classification
  — VF-9); for `Week`/`Month`, `window_misses(&state.tasks, now,
  window)` (structural arm, unreachable from v0.2 openers, covered by
  a unit). Row verbs:
  - `Move to tomorrow` → `UpdateTask { scheduled: Some(Some(next.start)) }`
    where `next = review_window(cadence, reference.succ, calendar)` —
    "tomorrow" is relative to the reviewed day; the task lands at the
    next window's opening instant (the planner leg that follows
    re-times it). Due dates are never touched (a due is a commitment;
    changing it stays the task view's job).
  - `Pick a date…` → `picking_date: true`; an adapter-local text
    buffer parsed by the existing `due_edit` parser on commit →
    `UpdateTask { scheduled: Some(Some(instant)) }`; unparseable input
    dispatches nothing and keeps the text (the planner-form rule).
  - `Leave as is` → no command, cursor advances.
  - The effort slot is not written (no ruled affordance needs it).
- **Prompts** — `walk_prompt_rows(cadence) -> &'static ReflectionTemplate`:
  the one `DEFAULT_TEMPLATES` entry for the cadence. One
  `MultiLineEditor` per prompt over adapter-local
  `walk_answers: [String; N]` (cleared on walk close; survives
  stage-back and the planner leg). `Record` dispatches exactly one
  `RecordReflection { cadence, day: window.day(), window: (start, end),
  text: compose_reflection_text(template, &answers), mood: None,
  energy: None, flags: ∅ }`; if every answer is blank it dispatches
  **nothing** (the walk ends gently — the doc's "records nothing";
  never relying on the planner's `NoOp`). `Finish without recording`
  dispatches nothing. Both close the flow.
- **`compose_reflection_text`** (domain, `reflection.rs`, pure):
  non-blank answers in prompt order, each as `prompt\nanswer`,
  separated by a blank line; blank answers omitted. Domain-side
  because the text is entry data any lane may read back (the CLI's
  `journal show` prints it verbatim).

### D-W5 — The card derivation (D-J10)

Domain, `reflection.rs`, pure:

```rust
pub struct ReviewDue { window: ReviewWindow }   // private field + getter
pub fn review_due(reflection: &ReflectionState, calendar: DayCalendar, now: DateTime<Utc>) -> Option<ReviewDue>
```

The latest closed day window is `review_window(Day, today(now).pred,
calendar)` where `today` is the calendar's current window label; it
is due iff no **live** `Day` entry has `day == window.day()`. Matching
on the day label (not the stored instants) is deliberate: a changed
`day_start_time` shifts instants but the user still means "that
day". Week/Month never return `Some` (no parameter for them).
Older gaps never surface (only the one window is examined).

UI push (`models.rs`): `push_review_card(window, state, kiosk, day)`
on the Today push — `None` on kiosk (the gate model-side, the
`push_notifications` pattern), else
`presentation::review_card(review_due(…))` →
`Option<ReviewCardRow { title, body, action }>`. **No engine change,
no tick arm, no persistence action, no dismissal state**: the card
re-derives on the existing wall-minute repaint (the driver already
re-pushes Today each minute). Card activation dispatches
`UiEffect::OpenReviewWalk` with the due window's reference date.

### D-W6 — Actions, effects, keymap

- `ActionId::ReviewOpen` — `"review.open"`, `contexts: GLOBAL`,
  `Category::Navigation`, `menu_group: Some(MenuGroup::ViewNav)`,
  palette/menu-reachable, **no preset binding** (P7: reachable, not
  promoted). Gated in `entry_states` (disabled on kiosk — layer 2).
  Routes `UiEffect::OpenReviewWalk`; the opener's reference is
  `review_due(...)`'s day when `Some`, else the current day window
  (the evening review before the boundary).
- `ContextId::ReviewWalk` (mapped while the flow holds) with
  section-scoped rows (no menu section; the #622 planner-verb rule):
  `ReviewWalkCursorUp/Down`, `ReviewWalkBack`, `ReviewWalkAdvance`,
  `ReviewInboxFile`, `ReviewInboxKeep`, `ReviewFileTargetPrev/Next`,
  `ReviewFileCommit`, `ReviewCarryTomorrow`, `ReviewCarryPickDate`,
  `ReviewCarryLeave`, `ReviewCarryDateCommit`, `ReviewRecord`,
  `ReviewFinish`. Preset bindings only inside `ContextId::ReviewWalk`:
  `Up`/`Down` (cursor), `Escape` (back) in every preset; the row verbs
  are reached by focus + `Enter` on the mounted buttons (mount-driven
  focus) and the pointer.
- New `UiEffect`s: `OpenReviewWalk`, `ReviewWalk(WalkIntent)` (one
  carrier enum for the in-walk verbs keeps the effect enum flat), all
  in a new `EffectClass::ReviewWalk`; `shell.rs` lists them with the
  other view intents.
- Registry labels/descriptions are registry data, not walk copy (the
  ruled table governs the rendered walk/card only).
- **Snapshot regen (data only)**:
  `tests/snapshots/keymap_snapshots__registry.snap` and
  `…__resolved_preset_keymaps.snap` via `cargo insta accept`; the
  review diff must show only added rows.

### D-W7 — The phrase-fn vocabulary (presentation.rs, pure, ruled verbatim)

| fn | Copy |
|---|---|
| `review_card_title(cadence)` | C1 `Day review` |
| `review_card_body(cadence, date)` | C2 `A review for {date} is open — about two minutes, four prompts.` (the count = `template.prompts.len()` spelled as a word; the ruled text for 4) |
| `review_card_action(date)` | C3 `Review {date}` |
| `walk_header(cadence, date)` | C4 `Review — {date}` / `Review — week of {date}` (Month: `Review — {month year}`) |
| `walk_stage_header(stage, template)` | C5 `Inbox`, C7 `Carry forward`, C10 `template.name` |
| `walk_stage_empty_line(stage)` | C5b `The inbox is empty.`, C7b `Nothing to carry forward.` |
| `walk_inbox_actions()` | C6 `File to…`, `Keep in inbox` |
| `walk_carry_actions()` | C8 `Move to tomorrow`, `Pick a date…`, `Leave as is` |
| `walk_commit_actions()` | C11 `Record`, `Finish without recording` |

`{date}` is the house's existing day phrase fn (the one the Today and
planner headers use) — no new date format. C9 is retired: nothing
renders for the handoff. The phrase fns' own unit tests are the only
place these strings are asserted (AGENTS §5 allows the phrase
owner); a P9 guard unit asserts no phrase fn output contains
`missed`/`failed`/`overdue` (a floor on garbage, not a label test).
Slint computes nothing: every string and row arrives pushed.

### D-W8 — The UI surfaces

- **Walk**: an overlay mounted from `app-window.slint` while
  `Flow::ReviewWalk` holds (the focus-overlay scrim precedent) — new
  `ui/views/review-walk-overlay.slint` (SPDX AGPL). Progress strip
  (three headers, empty b-lines), the current stage's rows, the
  stage's row/commit buttons, the anchored flyout for `File to…`, the
  `MultiLineEditor`s for Prompts. Equal-prominence C11 buttons (same
  component, same style).
- **Card**: a single optional card in the Today view's header slot,
  **above and outside** the sections' `ListView` — the row-anchored
  compensation (`17d4b0bc`) keys on the list's rows, so the card must
  not become a list row.
- `ViewId::Review` (statistics) is untouched; no new `ViewId`; the
  walk never enters `step_view` or `KioskMode`.

### D-W9 — Attributes out of the walk

No ruled copy exists for mood/energy/flags, and the DR-7 walk spec is
"prompts rendered as fields over one `RecordReflection`". The walk
records text only; attributes stay the CLI's (`journal add/edit`).
Not a fence issue — a future walk extension needs its own copy
ruling.

### D-W10 — Kiosk, three layers (D-J14)

1. **Structural**: `ReviewOpen` disabled on kiosk in `entry_states`
   (the `FocusModeToggle` gate) and the card never pushes → no route
   into the flow.
2. **Effect arm**: `(_, UiEffect::OpenReviewWalk | UiEffect::ReviewWalk(_)) if self.kiosk => Vec::new()`
   — the pinned layer (the `ToggleFocusMode` precedent).
3. **Model push**: `push_review_card` and the walk push return empty
   on kiosk regardless of the flow (frozen even if layers 1–2 drift).

Each layer gets its own pin, added beside the existing kiosk gate
tests.

### D-W11 — No-machinery pins

`review_due` takes `&ReflectionState`, a `DayCalendar`, and `now` —
nothing mutable, no engine handle (the signature is the pin). An
adapter journey pins that the card appears when the clock seam
crosses the boundary **with no command dispatched and no request
sent** (the recorded-outcomes harness asserts the empty command log),
and disappears after a `RecordReflection` lands in the published
state. No new `PersistenceAction`, no reminder arm, no tick channel —
reviewers check the diff touches no file in `taskboard-state`,
`taskboard-storage-sqlite`, or the reminder modules.

### D-W12 — The planner title

DR-7 recorded `Plan tomorrow` as a *recommendation* for the planner
surface's title; #622 shipped `Plan next day` (label, accessible
label, registry). Renaming a landed surface is outside this chunk's
scope (the walk adds no copy for that leg); the recommendation is
reported back to the user for a ruling instead of being applied
here.

## 3. TDD order (each step = failing tests first, then code; one standalone commit each)

1. **`feat(domain): the review-due derivation and the prompted-text composer`**
   — `review_due`, `ReviewDue`, `compose_reflection_text` in
   `reflection.rs`. Units (fixed-stamp calendars, `FixedClock`-style
   instants): no entries → due for yesterday's window; a live Day
   entry for that day → `None`; a tombstoned entry → still due; an
   entry for an older day only → due (older gaps never surface: the
   result is always the latest closed window); a Week entry for the
   week → still due (cadence-matched). Proptest: for any calendar ×
   `now`, `Some(d)` implies `d.window().end() <= now` and
   `d.window().end() > now - 24h` (the latest closed, not an older
   one). Composer units: blanks omitted, order kept, all-blank → `""`.
2. **`refactor(ui-slint): one inbox reading for the chip and the walk`**
   — extract `inbox_tasks`; existing chip tests stay green unchanged.
3. **`feat(ui-slint): the review-walk flow and its stage machine`** —
   `Flow::ReviewWalk`, `WalkStage`, `ViewState::open_review_walk[_at]`,
   cursor/advance/back/close, empty-skip; `reconcile` leaves it alone;
   XOR both ways (opening Capture over it replaces it; opening it over
   Capture replaces that). Table rows in `flow_table_tests.rs`.
4. **`feat(ui-slint): the review-walk phrases and row projections`** —
   D-W7 phrase fns + their units (the only string asserts), the P9
   vocabulary guard, `walk_carry_rows` (Day = the verbatim union;
   equality pin against `today_sections` over generated states;
   Week/Month = `window_misses`), `review_card`.
5. **`feat(ui-slint): the review.open action, the walk verbs, and their routing`**
   — registry rows, `ContextId::ReviewWalk`, router arms, `UiEffect`s,
   `entry_states` kiosk gate, preset context bindings; regen both
   keymap snapshots (data-only diff).
6. **`feat(ui-slint): the review walk overlay and the today-view card`**
   — adapter arms (commit funnels for MoveTask / UpdateTask /
   RecordReflection, the answer and date buffers), the handoff +
   return hint + abandon reconcile (D-W3), the push passes with the
   kiosk model gate, the slint overlay + card.
7. **`test(ui-slint): the review-walk journeys`** — §4 catalogue in
   `tests/journeys.rs` (a `// ---- #638 ----` section), plus the kiosk
   pins beside the existing gate tests and the view-mount inventory
   row for the overlay.
8. **`docs(changelog): the review walk entry`** — CHANGELOG
   Unreleased.

Clock discipline (AGENTS §5, testing_strategy §6.1–6.2): relative
oracles anchor fixtures to the fixture `now`; the card-appears-at-
boundary journey advances the injected render clock
(`UiSettings::clock`); no golden image embeds clock-derived text
unless its render clock is frozen.

## 4. Journey catalogue (typed outcomes only — commands/effects/views, never text)

| # | Journey | Asserts |
|---|---|---|
| J1 | Unreviewed closed day → Today shows the card; click | flow = `ReviewWalk{Day, reference = that day, Inbox}` |
| J2 | Live Day entry for the closed day | card model empty |
| J3 | Boundary crossed via the render clock seam | card appears; command log empty (D-W11) |
| J4 | `review.open` from Board, no closed gap | walk over Board, reference = current day |
| J5 | Inbox `File to…` → pick stack → commit | exactly one `MoveTask{id, stack, order}` |
| J6 | Inbox `Keep in inbox` on the last row | no command; stage = `CarryForward` |
| J7 | Empty inbox | walk opens at `CarryForward`; progress strip carries the Inbox empty flag |
| J8 | `Move to tomorrow` | one `UpdateTask{scheduled: Some(Some(next window start))}`, due untouched |
| J9 | `Pick a date…` valid / invalid | one `UpdateTask` / no command, buffer kept |
| J10 | Advance out of Carry forward | flow `None`, view `PlanNextDay`, `planner_return` = the walk's base view, resume hint set |
| J11 | Planner closed (Escape, nothing placed) | view = base view; flow = `ReviewWalk{…, Prompts}` |
| J12 | Planner place, then close | the placement `UpdateTask` lands; walk resumes at Prompts |
| J13 | Leave planner via `view.today` | no walk; hint dropped (a later planner close does not resume) |
| J14 | Prompts `Record` with two answers | exactly one `RecordReflection` with cadence/day/window of the reference; after the state publishes, card gone |
| J15 | Prompts all blank `Record` / `Finish without recording` | no command; flow `None` |
| J16 | Escape ladder | picker → stage-back ×2 → close; no command at any rung |
| J17 | Capture opened mid-walk | walk replaced, nothing dispatched |
| J18 | Wall clock crosses the boundary mid-walk | reference unchanged; rows re-derive for the same window |
| J19 | Kiosk: `review.open` | no flow, no command (layer 2); action disabled (layer 1); card/walk models empty even with a forced flow (layer 3) |
| J20 | Walk opened over the planner, advance out of Carry forward | planner stays mounted (no toggle-close) |

## 5. Risks

- **R1 — "tomorrow" relative to the reviewed day vs the planner's
  now-relative tomorrow.** A morning-after review (reference =
  yesterday) moves carries to *today*, while the planner leg plans the
  day after today. Copy C8 is ruled; the semantics are the plan's
  decision (D-W4). Surfaced in the report; if the user dislikes it,
  the alternative is "next window after `now`" — a one-line change.
- **R2 — Today-view row-anchor compensation** (`17d4b0bc`): the card's
  appearance on the minute repaint changes the content above the
  list; keeping it outside the `ListView` (D-W8) avoids the anchor,
  but a scroll-offset regression is possible — J3 checks the anchored
  row's index is unchanged.
- **R3 — round-5 churn** in `app-window.slint` / flyout widgets and
  the in-flight dead-seams chunk; stage 3 rebases, expected conflicts
  are mechanical (overlay mount list, effect lists).
- **R4 — Effect-enum growth** (`EffectClass` exhaustiveness, the
  shell's view-intent list, `FlowOutcome` large-variant lint): the
  `WalkIntent` carrier keeps it to two variants.
- **R5 — First run**: a fresh install shows the card the first day
  after install for the install day. Accepted (derived-at-read has no
  "first seen" state by ruling).

## 6. Estimated size

~2.8–3.6k lines total: domain ~250 (incl. tests), view_state + flow
table ~450, actions/router/keymap ~350 (+ snapshot data), presentation
+ models ~500, adapter ~450, slint ~350, journeys + kiosk pins
~900, changelog ~15. Size **M/L**, as dispatched.
