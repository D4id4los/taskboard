# HF4 — Habit Per-Weekday Schedules & Reminder Overrides (implementation plan)

- **Date**: 2026-11-21
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
     empty set at the write boundary; decode lenient). R-6 documented
     leniency at the field: **inert for PerWeek/PerMonth habits**
     (period grain already schedules them; O-1 keeps windows
     ISO-Monday/calendar-1st — no D-H12 reopening).
   - `Habit.scheduled_days: Option<WeekdaySet>` — `None` = every day
     (the today posture; the missing field must not retroactively
     de-schedule existing habits). `Some(set)` = scheduled weekdays
     only.
   - Transparency: a PerDay habit's window on a non-scheduled weekday
     is **transparent** in the streak walk (never counts, never
     breaks) — the third transparent source, composing with skip and
     pause (Σ property extension, items (a)/(b)/(d)).
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
   - Statistics denominator: `adherence`/stats pure fns (HF3's) count
     **scheduled windows only** — the denominator law lands here
     because it is a window-semantics decision (US-5 AC-5).
2. **Nudge engine (`habit.rs::tracker_anchor_occurrence` /
   `nudge_day` + `reminders.rs` pass)**: occurrence = the override's
   base date (`nudge_day(habit, window)` or the period start) shifted
   by `offset_days`, at `anchor_time` when set, else the override's
   own `time`; disarmed entirely on non-scheduled days and while
   paused (HF1); snooze applies the habit's chosen hours/days grammar
   (the `ReminderRef::Tracker` snooze plumbing gains the per-habit
   unit). No override ⇒ today's O-1 default, byte-identical.
3. **Messages/planner**: no new commands — both edits ride
   `UpdateHabit`; typed rejections unchanged.
4. **CLI**: `habits update --days mo,we,fr` / `--days all`;
   `--reminder base=nudge-day,offset=1,time=07:30,snooze=2h|1d`;
   `--reminder-clear`.
5. **UI (W-6, W-7)**: the habit editor (shared task-detail surface,
   O-7 Q4) gains the weekday picker (CheckBox primitives, W5/W4
   base widgets; visible for PerDay habits only — fold test US-5
   AC-4) and the reminder-override section (base/offset/time/snooze
   rows + clear leg). Draft-backed `HabitEditDraft` extension; commit
   computes the sparse `HabitChanges` at the trust boundary.
   `ActionId::HabitScheduleEdit` / `HabitReminderEdit` + keymap rows
   (O-8). Today row renders the off-schedule and armed-chip states
   (display model bits).
6. **Snapshot/proptest extensions**: planner arms for the two new
   changeset fields; the Σ transparency family extended with the
   off-day arm; the `anchor_time_never_influences_the_canonical_order`
   proptest re-run (the new fields enter no ordering path — extend the
   property's habit strategy to include them).

## Serialization

Third chunk (HF1 → HF2 → HF4 → HF3): the scheduled-window denominator
must exist before HF3 renders statistics; HF4 needs HF1's transparency
composition.

## Test catalogue (typed outcomes only)

- Domain: `off_schedule_window_is_transparent` (property),
  `transparency_composes_across_skip_pause_offday` (property),
  `scheduled_denominator_excludes_transparent_windows`,
  `override_occurrence_matches_the_o1_law_shifted`,
  `no_override_keeps_the_o1_default_occurrence`,
  `empty_weekday_set_is_unmintable`,
  `new_fields_never_influence_the_canonical_order` (proptest).
- Engine: `schedule_save_routes_the_exact_update_habit`,
  `override_save_routes_the_exact_update_habit`,
  `clear_leg_reverts_to_the_o1_default`,
  `off_schedule_day_arms_no_nudge`.
- Journeys: `us5_s2_schedule_save_routes_the_exact_command`,
  `us5_s3_row_shows_the_off_schedule_state`,
  `us6_s2_override_save_routes_the_exact_command` (per the UX plan §6).
- Fold: `editor_gates_the_weekday_picker_by_target_kind`.

## Out of scope (homes)

- Per-weekday scheduling for PerWeek/PerMonth habits — R-6; rides the
  backlog issue on the week/month schedule generalization.
- Task-side ReminderRule editing parity for habits (insistence ladders
  etc.) — the override is deliberately compact (O-1's grammar);
  broader rule parity is the standing CLI-parity family's territory,
  not this chunk's.
