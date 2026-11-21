# HF4 — Habit Per-Weekday Schedules (weekly/monthly) & Reminder Overrides (implementation plan)

- **Date**: 2026-11-21 (revised same day per the operator gate
  feedback: day sets re-scoped onto the weekly/monthly habits)
- **Kind**: implementation chunk plan (HF4 of the habit family wave) —
  domain schedule vocabulary + nudge-override parameters + editor
  surfaces.
- **Origin**: `.artifacts/plans/2026-11-21-habit-family-ux-plan.md`
  (stories US-5, US-6; wiring W-6, W-7, W-11; rulings R-6, R-8);
  issues **#960** (per-weekday schedules), **#959** (reminder
  overrides). **O-1's nudge law binds verbatim** (day-relative by
  default, else period start; snooze in hours/days); O-2 additive-only.
- **Branch**: `feat/habit-schedules-reminders`
- **Status**: DRAFT — chunking after the operator accepts the family UX plan.

## Scope

1. **Domain (`taskboard-domain/src/habit.rs`, additive)**:
   - `WeekdaySet` — hand-rolled A1 set over `chrono::Weekday`
     (bitflag-u8 private field, minting constructor rejecting the
     empty set at the write boundary; decode lenient).
   - `Habit.scheduled_days: Option<WeekdaySet>` — the scheduled
     weekdays *inside* a weekly/monthly canonical window. **R-6
     (operator ruling, 2026-11-21 gate feedback): PerWeek/PerMonth
     habits only; inert for PerDay** (documented leniency at the
     field). The terminology law: a daily habit happens every day —
     a habit done Mon/Wed/Fri *is* a weekly habit; a daily habit with
     weekday subsets would work identically to a weekly habit with the
     same days, so daily is excluded by construction, not by
     convenience. Windows stay ISO-Monday / calendar-1st (O-1
     untouched — no D-H12 reopening); the day sets never move a
     boundary.
   - Transparency: a weekly/monthly window **before its first
     scheduled day has elapsed** is transparent in the streak walk
     (no due posture, no miss — the walk's transparent class's
     third subject, composing with skip and pause; Σ property
     extension items (a)/(b)/(d)). Once a scheduled day has elapsed,
     the window walks normally (pending/count/miss as today). With no
     day set, the window joins at period start — today's posture,
     byte-identical.
   - `HabitReminderOverride { base: NudgeBase, offset_days: i32,
     time: NaiveTime, snooze: HabitSnooze }` with
     `NudgeBase::{ NudgeDay, PeriodStart }` (R-8: default `NudgeDay` =
     O-1's nudge day; `PeriodStart` the explicit alternative) and
     `HabitSnooze::{ Hours(u8), Days(u8) }` (O-1's "user picks
     hours/days"). Carried on `Habit.reminder: Option<HabitReminderOverride>`
     (`#[serde(default, skip_serializing_if)]`).
   - `HabitChanges.scheduled_days: Option<WeekdaySet>` (plain option —
     a set replaces; clearing = the full seven-day set) and
     `HabitChanges.reminder: Option<Option<HabitReminderOverride>>`
     (double option, the `anchor_time` test twin); new clock slots
     (`scheduled_days`, `reminder`) on `HabitClocks`.
   - Statistics denominator: `adherence`/stats pure fns (HF3's) derive
     the denominator from the weekly/monthly day sets — a window joins
     the denominator once its first scheduled day has elapsed (no day
     set ⇒ at period start); **never from a daily habit** (daily
     windows enter at period start as today). The denominator law
     lands here because it is a window-semantics decision (US-5
     AC-5).
2. **Nudge engine (`habit.rs::nudge_day` / `tracker_anchor_occurrence`
   + `reminders.rs` pass)**: for a weekly/monthly habit with a day
   set, the nudge arms **on each scheduled day** inside the window
   (quota-gated; once per scheduled day — the O-1 single-anchor-day
   generalization, subject now the day set), at `anchor_time` when
   set, else the override's own `time`; the override's base selects
   the nudge day (default) or the period start. For a habit with no
   day set, today's O-1 default — byte-identical. Nudges stay disarmed
   while paused (HF1). Snooze applies the habit's chosen hours/days
   grammar (the `ReminderRef::Tracker` snooze plumbing gains the
   per-habit unit).
3. **Messages/planner**: no new commands — both edits ride
   `UpdateHabit`; the planner rejects `scheduled_days` on a PerDay
   habit with a typed rejection (the field is inert at read time per
   R-6, but the write path refuses rather than minting inert data —
   no validate-and-forget); other typed rejections unchanged.
4. **CLI**: `habits update --days mo,we,fr` / `--days all`
   (refused for PerDay targets, typed); `--reminder
   base=nudge-day,offset=1,time=07:30,snooze=2h|1d`; `--reminder-clear`.
5. **UI (W-6, W-7)**: the habit editor (shared task-detail surface,
   O-7 Q4) gains the weekday picker (CheckBox primitives, W5/W4 base
   widgets; visible for PerWeek/PerMonth habits only — fold test
   US-5 AC-4) and the reminder-override section (base/offset/time/snooze
   rows + clear leg). Draft-backed `HabitEditDraft` extension; commit
   computes the sparse `HabitChanges` at the trust boundary.
   `ActionId::HabitScheduleEdit` / `HabitReminderEdit` + keymap rows
   (O-8). Today row renders the scheduled/off-day state for
   weekly/monthly habits (display model bits; daily rows unchanged).
6. **Snapshot/proptest extensions**: planner arms for the two new
   changeset fields; the Σ transparency family extended with the
   pre-schedule arm (weekly/monthly); the
   `anchor_time_never_influences_the_canonical_order` proptest re-run
   (the new fields enter no ordering path — extend the property's
   habit strategy to include them).

## Serialization

Third chunk (HF1 → HF2 → HF4 → HF3): the scheduled-window denominator
must exist before HF3 renders statistics; HF4 needs HF1's transparency
composition.

## Test catalogue (typed outcomes only)

- Domain: `pre_schedule_window_is_transparent` (property, weekly +
  monthly arms), `transparency_composes_across_skip_pause_preschedule`
  (property), `scheduled_denominator_derives_from_the_day_sets`,
  `daily_denominator_unchanged` (PerDay habit: period-start windows,
  day sets inert),
  `override_occurrence_matches_the_o1_law_shifted`,
  `no_override_keeps_the_o1_default_occurrence`,
  `nudge_arms_on_each_scheduled_day_only`,
  `empty_weekday_set_is_unmintable`,
  `scheduled_days_on_a_daily_habit_is_a_typed_rejection`,
  `new_fields_never_influence_the_canonical_order` (proptest).
- Engine: `schedule_save_routes_the_exact_update_habit`,
  `override_save_routes_the_exact_update_habit`,
  `clear_leg_reverts_to_the_o1_default`,
  `preschedule_window_arms_no_nudge`,
  `nudge_arms_on_each_scheduled_day_only_engine_arm`.
- Journeys: `us5_s2_schedule_save_routes_the_exact_command`,
  `us5_s3_row_shows_the_off_schedule_state` (weekly/monthly row),
  `us6_s2_override_save_routes_the_exact_command` (per the UX plan §6).
- Fold: `editor_gates_the_weekday_picker_by_target_kind`
  (visible for PerWeek/PerMonth, absent for PerDay).

## Out of scope (homes)

- Day-of-month sets (e.g. "the 1st and 15th") for PerMonth habits —
  the day vocabulary here is weekday-grain per #960's ask; month-day
  scheduling would extend `AnchorDay::MonthDay`'s single day to a set
  and is a separate design question. Home: `backlog:` issue if the
  operator ever wants it (not filed preemptively — the ask is not on
  record).
- Task-side ReminderRule editing parity for habits (insistence ladders
  etc.) — the override is deliberately compact (O-1's grammar);
  broader rule parity is the standing CLI-parity family's territory,
  not this chunk's.
