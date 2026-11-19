# [Plan] Habit Editing (H4, round 9 chunk 10)

- **Date**: 2026-11-19
- **Kind**: implementation plan — TDD-first, single chunk (round plan
  chunk **H4**, branch `feat/ui-habit-editing`), stage-1 planning
  artefact for the three-stage dispatch protocol.
- **Origin**: J1 UX journey plan
  (`.artifacts/plans/2026-11-18-habit-parity-ux-plan.md`, ACCEPTED —
  **US-5 + US-6** and wiring rows **W-4/W-5** normative); round plan
  §1 **O-1** (the nudge law, superseded form: display/nudge day inside
  the canonical window, snooze in hours AND days) and **O-7 Q4/Q6**
  (reuse the task detail editor surface, generalized so visual
  adjustments transfer; GUI habit remove in scope, #944 closes here);
  research `.artifacts/reports/2026-11-18-round9-habit-parity-research.md`
  §1.3 (edit gap), §2 HAB-1/2/3/4/12, §3.3–3.4; type policy AGENTS §2 /
  `docs/coding_guidelines.org` §2.1.
- **Status**: PLANNED — awaiting stage-2 implementation.
- **Preconditions verified 2026-11-19**:
  - Chunk 4 (H1, PR #1007) **merged** — `Habit.description`,
    `Habit.anchor_day: Option<AnchorDay>`, the double-Option
    `HabitChanges.anchor_time`/`anchor_day` layers, and the pure
    `nudge_day` derivation are on `origin/main`. The reminders.rs
    engine consumption was **deliberately deferred to this chunk**
    (chunk-4 plan phase 1 step 2: "No reminders.rs change in this
    chunk — the engine consumption (arm relative to nudge_day) is H4").
  - Chunk 6 (H2, PR #1028) **merged** — `RowSubject::Task|Habit`,
    `habit_context_page`/`HABIT_CONTEXT` (edit/delete rows ABSENT,
    reserved verbs of H4 per `menu.rs:447`), the re-projection
    machinery (`reproject_habit_menu`), the kiosk greyed-menu gate.
  - Chunk 5 (PR #1025) **landed (MERGED)** — task-view carries the
    M8/M9 posture this chunk inherits verbatim: the always-mounted
    Effort field, the description `vertical-stretch: 1` editor mount,
    the migrated overlay mounts, the walk insert-mode posture. The
    habit variant consumes these as-is.
  - Chunk 14 (W4, `refactor/ui-labeled-value-row`, #951) **NOT
    landed** — no `LabeledValueRow` exists on `origin/main`. This
    chunk therefore keeps **chunk-14-compatible shapes** (D6): the
    habit editor's field rows follow the existing hand-rolled
    display/cell/editor row grammar so W4 absorbs both variants later;
    no new ad-hoc row widget is minted, none consumed.
  - Naming note: the dispatcher prompt's "chunk 8's
    `translate_for_subject`" is the H2-landed `task_menu_subject`
    widening over `RowSubject` (`presentation.rs`) — same seam, no
    symbol of that exact name exists. `TaskEdit`/`TaskDelete` remain
    deliberately untranslated (task-only) as recorded; this chunk owns
    the habit twins (`HabitEdit`/`HabitRemove`), not the task verbs.
- **Scope**: habit editing on the generalized task-detail editor
  surface (title, description, quota, optional anchor time, optional
  anchor day); the reminder-nudge engine wiring per the O-1 nudge law;
  the snooze choice UI (hours AND days); GUI habit remove with the
  staged-confirm grammar (#944). **Not in scope**: per-habit reminder
  overrides (#959), per-weekday schedules (#960), inline quota edit on
  the row (#961), skip days (#955), the W4 centralization itself, the
  #935 notice pass.

## 1. Recorded decisions (settled at planning, per dispatcher instruction)

### D1 — The shared editor surface: a subject-union variant of `task-view.slint`, not a second editor

O-7 Q4's law is *reuse with transfer*: the habit editing experience
mounts the **same `ui/views/task-view.slint` surface** the task detail
uses, parameterized by the existing `RowSubject` union — no parallel
habit-editor view is created.

- The adapter opens the surface with
  `RowSubject::Habit(habit)` (new `UiEffect` variant / view-state
  focused-subject widening beside `focused_task`; `focused_habit`
  already exists adapter-side from H2/H3 and mints the subject —
  row activation already sets it, `adapter.rs:9859`).
- `TaskDetailModel` gains the habit variant's fields **behind one
  mount discriminator** (`is-habit: bool`, model-side, the
  `track-entries-visible` display-stays precedent): task-only sections
  (due/scheduled dates, importance/urgency pickers, labels, time
  entries, tracker verbs, done checkbox, reminder-rule rows, Effort
  field) are **hidden by mount flag, never dead-rendered**; shared
  sections (title editor, description editor + its chunk-5 stretch,
  the walk/insert-mode posture, the activation stop) mount verbatim —
  every chunk-5 visual adjustment transfers by construction.
- Habit-only rows mount in the substrate's existing row grammar
  (D6): quota, anchor time, anchor day.
- The surface's kiosk gate follows the display-stays law: the
  editor surface is **unmountable on kiosk** (model-side
  `!kiosk && subject present`, task-view.slint:59's exact posture);
  the habit section's own kiosk posture (greyed affordances, D-P13)
  is untouched.
- Routing: `ActionId::HabitEdit` (slug `habit.edit`) routes
  `UiEffect::Activate`-shaped open of the surface with the one-shot
  habit subject (the `TaskEdit` → `UiEffect::Activate` shape,
  router.rs:1344). The menu row joins `HABIT_CONTEXT` after the
  reorder pair, before the Convert arm (non-destructive rank zone).
- Escape/cancel discards: the surface's existing editor-flow
  discipline (arm → commit / disarm) applies unchanged; no draft
  persistence for habit fields (the recur-editor's draft-discard
  finding is J2's migrated-mount law, not re-opened here).

### D2 — Nudge arming: the O-1 law wired into `reminders.rs` via the existing pure `nudge_day`

The engine change is minimal and total: `tracker_effective_occurrence`
(`taskboard-domain/src/reminders.rs:1373`) gains the anchor-day gate —

```
window = WindowStart::of_period(kind, day.today(now))
arm    = nudge_day(habit, window)
None   when today < arm            // before the anchor day: silent
else   base = tracker_anchor_occurrence(anchor_time, arm-anchored date, day)
```

- **No anchor day set** → `nudge_day` returns the period start →
  byte-identical behavior to today (the law's "otherwise at the
  period's start"; `PerDay` habits unchanged by construction).
- **Anchor day set** → the nudge arms ON the day and stays armed
  through the window's end (daily at `anchor_time`, quota-gated as
  today) — Wednesday-anchored weekly habits nudge Wed..Sun, never
  Mon/Tue; a month-17 habit nudges from the 17th; the clamped 31 in
  February arms from the 28th (documented leniency, chunk-4 D1).
- `tracker_eligible` (quota gate) and the snooze-walk
  (`walk_chain` over `ReminderRef::Tracker`) are untouched — the gate
  decides WHEN only, exactly like the eligibility doc comment says.
- Variant-mismatched anchor days stay inert for free (`nudge_day`
  falls through) — the domain's documented leniency is consumed, not
  re-policed.
- New pure property: arming is monotone within a window and always
  inside the canonical window (extends the chunk-4
  `anchor_day_window_invariance` family — the streak walk and window
  keys are untouched, pinned again by the existing property).

### D3 — Snooze choice UI: hours AND days on the notification row

O-1's "user picks hours/days" lands on the notification banner's
snooze affordance (`notification-surface.slint:122-125` — today a
single fixed `SNOOZE_POSTPONE = 1h` hop, router.rs:15):

- The snooze control becomes a **choice flyout** (the
  `quick-list-flyout` widget host) with a fixed span set spanning
  both units: **1 h, 4 h, 1 day, 3 days** — hours first, days after,
  destructive-last style ordering by span. Every choice routes the
  existing `StateCommand::SnoozeReminder { postpone: TimeDelta }`
  (typed duration, engine-stamped via the injected clock; the
  `dur ≤ 0` typed rejection unreachable by construction).
- The choice set is **reminder-family-agnostic** (tracker nudges,
  task rules, alarms all snooze through the same banner) — the O-1
  ask is satisfied by the tracker nudge inheriting it; no per-habit
  span grammar here (#959 stays homed).
- The fixed-span keyboard verb (`reminder.snooze` /
  `ReminderSnooze`) keeps `SNOOZE_POSTPONE` as its default span —
  the pointer flyout adds choice, the key grammar gains none
  (keyboard–mouse parity note: the flyout is walk-reachable; no new
  chord).
- `AnchoredOverlay`/flyout geometry per the W1/W2 migrated mounts
  (both landed) — no anchor-mirror race re-introduced.

### D4 — GUI habit remove: the task-delete staged-confirm grammar, closes #944

The arm lives adapter-side, staging touches no engine state (the
`BreakRuleRemove` grammar verbatim, router.rs:1013-1024):

- New one-shot ids: `ActionId::HabitRemove` (arm — routes
  `DispatchOutcome::NoTarget`, the adapter records
  `habit_remove_armed: Option<HabitId>`) and
  `ActionId::HabitRemoveConfirm` (confirm — routes
  `StateCommand::DeleteHabit`). One armed subject at a time; opening
  another subject's menu disarms (the `habit_convert_armed` twin,
  adapter.rs:6089); the arm flips through the **states map** +
  `reproject_habit_menu` (the convert trio's exact precedent —
  `states[HabitRemoveConfirm] == true` IS armed, no separate flag
  rides the menu model).
- Menu placement: the remove row joins `HABIT_CONTEXT` at
  **destructive-last** (after the Convert parent), the confirm rides
  as a child step while armed (the armed cancel step's shape —
  while armed, the confirm replaces the arm row's enabled state and
  the cancel step joins the page; H2's recorded deviation (2) is the
  template).
- Tombstone semantics are the engine's existing `DeleteHabit` (log
  untouched, history outlives — research §1.1); the undo story is
  the engine's existing inverse machinery, not re-litigated.
- Kiosk: `HabitRemove`/`HabitRemoveConfirm` join the greyed set in
  `habit_menu_states` (visible-disabled posture; the dispatch side
  refuses too — the arm's adapter check mirrors the convert arm's).
- Census: dated supersession of R-3 ("GUI habit removal exists,
  2026-11-19, this chunk") written with **symbol anchors** per #775
  (`HabitRemove`, `HabitRemoveConfirm`, `HABIT_CONTEXT`), plus the
  `habit.remove` matrix row.

### D5 — Editor fields and the sparse changeset mapping

The editor's save routes **one** `StateCommand::UpdateHabit { habit,
changes: HabitChanges }` with the sparse discipline (`None` =
untouched, clocks preserved — messages.rs:794):

| Field | Widget (existing) | `HabitChanges` mapping |
|---|---|---|
| Title | the shared title editor (verbatim) | `title: Some(s)` on change |
| Description | the shared description editor + chunk-5 stretch (verbatim) | `description: Some(s)`; `Some("")` clears |
| Quota | `SegmentedControl` (period kind) + count field (`NonZeroCount` mint at the seam) | `target: Some(HabitTarget)` |
| Anchor time | `time-field.slint` + an explicit clear affordance | `anchor_time: Some(None)` clears / `Some(Some(t))` sets; omitted when untouched |
| Anchor day | variant-gated picker (below) | `anchor_day: Some(None)` clears / `Some(Some(day))` sets |

- **Anchor-day variant gating (the UI-side trust boundary)**:
  `PerDay` habits mount **no** day field; `PerWeek` mounts a 7-slot
  weekday picker (the `SegmentedControl` host); `PerMonth` mounts a
  1..=31 day picker (`MonthDay::try_new` mint at the seam — 0 and
  >31 unrepresentable). The UI never mints a kind-mismatched
  `AnchorDay`; the domain's documented leniency stays a decode-time
  concern only. Changing the quota's period kind with a now-mismatched
  day set either re-prompts the day row or carries the inert day —
  **decision: carry it** (the domain accepts it inert; the picker
  re-renders for the new kind; no silent destructive clearing).
- **Setting the time arms the nudge automatically** (US-5/AC-2): no
  reminder UI in the editor beyond the time field — the engine
  derives the nudge from `anchor_time` (+ D2's day gate). US-6/S2's
  "snooze offers hours/days" is D3's banner grammar.
- Save is idempotent-safe: a changeset where every field is `None`
  (no actual edit) is not dispatched — the save affordance greys
  (the empty-dispatch law; a repeat is the engine's accepted no-op
  anyway).
- Field clocks: untouched fields keep clocks by construction — the
  sparse map above is the whole contract; no adapter-side clock
  stamping.

### D6 — Chunk-14 compatibility: hand-rolled row grammar, absorbable shapes

`LabeledValueRow` (W4/#951) is not landed. Therefore:

- The quota/time/day rows are laid out with the **existing hand-rolled
  display/cell/editor row grammar** used by task-view's own rows
  (label + value cell + click-to-edit swap) — no new widget type, no
  new edit grammar (the third divergent grammar would poison W4's
  absorption).
- Row *shapes* (label text, cell heights, walk stops) are written so
  a later `LabeledValueRow` adoption is a mechanical re-host; the
  plan's wiring rows name the rows so W4's audit finds them.
- No dependency is taken on any W3–W7 centralization output
  (`TextButton`, `ActionChip`, …) beyond what main already carries.

### D7 — Kiosk posture

- The editor surface: unmountable on kiosk (D1); `HabitEdit` and the
  remove pair greyed in the menu (`habit_menu_states` kiosk set).
- Nudge display and snooze flyout: present on kiosk (the notification
  surface is display+affordance, not an editor — the alarm window
  precedent).
- Everything else (log, repair, convert) — unchanged from H2.

## 2. TDD sequence (tests first, lowest layer)

### Phase 1 — the nudge-day engine gate (domain, `taskboard-domain`)

1. **Unit tests** (`reminders.rs` tests / `habit.rs` seam tests
   module, next to the existing tracker family):
   - `tracker_firing_waits_for_the_anchor_day`: weekly habit,
     `anchor_day: Weekday(Wed)`, quota unmet — no firing Mon/Tue;
     fires from Wed (at `anchor_time`) through Sun.
   - `tracker_firing_without_a_day_starts_at_the_period_start`: the
     no-day weekly/monthly behavior is byte-identical to the
     pre-change engine (pin the invariant, not today's accident —
     this IS the law's else-branch).
   - `tracker_firing_month_day_clamps_into_the_window`: day 31 in
     February arms from the 28th/29th.
   - `tracker_firing_per_day_ignores_the_day_field`: `PerDay` +
     `Some(Weekday(..))` (inert leniency) = daily behavior.
   - `tracker_snooze_chain_composes_with_the_day_gate`: a snoozed
     occurrence past the day gate still fires (the walk reads the
     snoozes map after the gate — ordering pinned).
   - dismissal/delivery receipts unchanged (existing tests stay
     green unmodified — the gate touches occurrence derivation only).
2. **Property test** (`proptest`, the chunk-4 family's successor):
   for arbitrary (kind, anchor_day, date-in-window): the gate's
   armed set is a suffix of the window's days, starts exactly at
   `nudge_day`, and never escapes the canonical window.
3. Implement D2 in `tracker_effective_occurrence`. (A new test
   failing here is a bug in the derivation OR the engine — evaluate
   both; do not weaken the suffix-property.)

### Phase 2 — the editor changeset path (state/UI seam)

4. **Seam tests**: `UpdateHabit` round-trip for every D5 mapping row
   (title, description set/clear, quota kind change with carried day,
   `anchor_time` set/clear, `anchor_day` set/clear) — sparse
   discipline pinned: untouched fields' clocks byte-equal. (US-6/S1's
   engine-tier evidence exists from chunk 4 — this extends the
   family; do not duplicate its window-invariance case.)
5. Engine snapshot (`insta`) grows the editor round-trip case.

### Phase 3 — the editor surface (UI)

6. **Journey tests** (`habits_journey.rs`, step-ID tokens, on
   `on_slint_thread` rig):
   - `us5_s1_habit_editor_opens_and_cancel_discards` — menu row and
     `habit.edit` key both open the surface; cancel/escape discards
     (no `UpdateHabit` dispatched — typed seam assertion, not log
     text).
   - `us5_s2_habit_editor_round_trips_time_and_day` — set time +
     Wednesday on a weekly habit, save; the row shows the time chip
     (W-5 fold assertion: chip presence flag, structural) and
     `UpdateHabit` carried the sparse changeset (adapter-seam
     capture).
   - `h4_remove_s1_remove_arms_and_the_confirm_flips` — arm greys →
     confirm enables inside the open menu (the armed-flip re-project);
     opening another subject's menu disarms.
   - `h4_remove_s2_confirm_tombstones_and_closes` — confirm routes
     `DeleteHabit`; the row leaves the section; the log survives
     (structural state assertion).
   - `h4_kiosk_editor_gate` — kiosk rig: surface unmountable,
     `HabitEdit`/remove pair greyed (extends the H2 kiosk-greyed
     journey).
7. **Fold/presentation tests**: `is-habit` mount discriminator hides
   every task-only section structurally (section-presence flags, no
   text assertions); walk traversal crosses the habit variant's rows
   (the walk-facts probe shape).
8. Implement D1/D5/D6 (`task-view.slint` variant + adapter model
   push + router/menu/keymap rows + coverage-gate ActionId tokens
   `habit.edit`, `habit.remove`, `habit.remove-confirm`).

### Phase 4 — the snooze choice flyout (UI + wiring)

9. **Journey test**: `us6_s2_snooze_offers_hours_and_days` — a fired
   tracker nudge's banner snooze opens the choice flyout; picking
   `1 day` routes `SnoozeReminder` with a `TimeDelta::days(1)`
   postpone (seam-captured); the nudge's effective occurrence moves
   (engine assertion via the state seam).
10. **Wiring test**: the fixed-span keyboard verb still routes
    `SNOOZE_POSTPONE` (existing tests stay green unmodified).
11. Implement D3.

### Phase 5 — census + coverage gate

12. Census supersessions (symbol anchors per #775): R-3 overturn,
    `habit.remove`/`habit.edit` matrix rows, the `HABIT_CONTEXT`
    growth note.
13. `scripts/ux-coverage.sh` gate at stage 2 self-check; stage 3
    re-runs it + the walkthrough protocol. **Recorded deviation**:
    the remove journey's step IDs (`h4_remove_s1/s2`) are NOT in the
    ACCEPTED J1 plan (its journeys predate O-7 Q6's scope join);
    stage 2 appends a "stage-landing deviations" section to J1
    (the H2 precedent, J1 §8) binding `bound-test:` lines for the
    remove steps before the gate runs.

## 3. Wiring inventory (J1 W-4/W-5 plus this chunk's additions)

| ID | Surface file | Widget | Callback | Adapter seam | ActionId | Commands | Journey steps | Test |
|---|---|---|---|---|---|---|---|---|
| W-4 | `views/task-view.slint` (habit variant) | shared editors + D5 rows | `save`/per-field commits | `UpdateHabit` seam | `HabitEdit` (`habit.edit`) | `StateCommand::UpdateHabit` | US-5/S1, S2 | `us5_s1`, `us5_s2` |
| W-5 | `widgets/task-card.slint` / models | time chip | — | presentation flag | — | — | US-5/S2 | fold test |
| W-8 | `compositions/notification-surface.slint` | snooze choice flyout | `snooze-span-picked(int)` | one-shot row + `TimeDelta` | `NotificationSnooze` span route | `StateCommand::SnoozeReminder` | US-6/S2 | `us6_s2` |
| W-9 | `menu.rs` `HABIT_CONTEXT` + adapter | remove rows | menu route | `habit_remove_armed` staging | `HabitRemove`, `HabitRemoveConfirm` | `StateCommand::DeleteHabit` | h4_remove_s1/s2 | journey |
| W-10 | `reminders.rs` (domain) | — | — | `tracker_effective_occurrence` gate | — | — | US-6/S2 (engine half) | unit + property |

## 4. Command-contract delta

| Kind | Name | Payload | Journeys |
|---|---|---|---|
| ActionId | `HabitEdit` (`habit.edit`) | one-shot habit subject → surface open | US-5 |
| ActionId | `HabitRemove` (`habit.remove`) | arm step, NoTarget route | h4_remove_s1 |
| ActionId | `HabitRemoveConfirm` (`habit.remove-confirm`) | routes `DeleteHabit` | h4_remove_s2 |
| UiEffect | surface-open variant carrying `RowSubject::Habit` | the D1 union | US-5 |
| UiRequest/callback | `snooze-span-picked` | index into the span const | US-6/S2 |
| StateCommand | (existing) `UpdateHabit`/`DeleteHabit`/`SnoozeReminder` | **no domain/message changes** — every payload type already exists | — |

Zero domain-type churn: this chunk consumes chunk 4's fields and the
existing message vocabulary end-to-end. The only domain *code* change
is the D2 gate inside `reminders.rs`.

## 5. Risks / watch items

- **task-view.slint is the hottest file in the round** (W4/K3/D2 all
  queue on it) — keep the habit variant's diff tight (mount
  discriminator + habit rows only); no drive-by refactors of task
  sections, even where tempting.
- **The `is-habit` discriminator vs a full model split**: if the
  model push grows beyond ~10 habit fields, split `HabitDetailModel`
  as a sibling struct carried beside the discriminator rather than
  widening `TaskDetailModel` unboundedly — decide at implementation
  by the size the D5 rows actually need (either shape is
  chunk-14-compatible).
- **Snooze spans are global** (all reminder families see the
  flyout): acceptable per O-1's phrasing (the ask is tracker-side);
  per-family/per-habit span grammar is #959's home — file nothing
  new there.
- **The day gate vs stale deliveries**: before the anchor day, a
  window's earlier days must produce NO firing and no delivered
  receipt rows (the gate sits in occurrence derivation, so
  `stale_deliveries` consults the same None — verify the consult
  test, research §1.1's "one definition" comment).
- **Journey-token gate**: `h4_remove_*` tokens are outside J1's
  token list until stage 2 appends the deviations section (phase 5
  step 13) — the ux-coverage gate fails otherwise.
- **Kiosk unmountable ≠ unmentioned**: the greyed menu rows must
  still project (the visible-gating grammar, #790) — a withheld row
  would regress the H2 kiosk journey.

## 6. Quality gates

Per AGENTS §10, through `scripts/run-quality-gates.sh`; heavy builds
through `scripts/memory-gate.sh`. Subagents gate on pre-commit hooks +
flock-wrapped `cargo nextest` only (dispatch discipline — the
dispatcher owns the coverage trio).

## 7. Definition of done

- [ ] Phases 1–5 tests first, green, no text assertions, no sleep-
      based waits (test clock / seam captures only)
- [ ] US-5 and US-6 journeys complete end-to-end incl. escapes;
      `h4_remove_*` journeys bound in J1's deviations section
- [ ] `scripts/ux-coverage.sh <J1 plan> --gate` exits 0
- [ ] Census supersessions dated, symbol-anchored (#775)
- [ ] `cargo fmt --check`, clippy `-D warnings`, nextest, doc tests,
      `cargo deny` all green
- [ ] #944 closes on the PR; #959/#961 untouched (still homed)
