# HF1 — Habit Skip & Pause (implementation plan)

- **Date**: 2026-11-21
- **Kind**: implementation chunk plan (HF1 of the habit family wave) —
  domain/engine semantics + row/menu surfaces.
- **Origin**: `.artifacts/plans/2026-11-21-habit-family-ux-plan.md`
  (stories US-1, US-2; wiring W-1..W-4; rulings R-3/R-4/R-5/R-7-adjacent);
  issues **#955** (skip), **#958** (pause). O-1's display-day law and
  O-2's additive-only domain law bind.
- **Branch**: `feat/habit-skip-pause`
- **Status**: DRAFT — chunking after the operator accepts the family UX plan.

## Scope

1. **Domain (`taskboard-domain/src/habit.rs`, additive, O-2)**:
   - `HabitLogEntry.skip: bool` — `#[serde(default,
     skip_serializing_if = "std::ops::Not::not")]`; decoded old rows read
     `false` (leniency one layer up, the mint-strict/decode-lenient
     posture). `skip == true` marks the window a **typed planned skip**.
   - Streak walk (`streak_with_break`): the per-period classification
     gains the **transparent** outcome — a window with `skip == true`
     is transparent regardless of quota or grace (neither counts nor
     breaks). Consultation order: quota-met ⇒ counts; else `skip` ⇒
     transparent; else pending/missed as today (R-4: skip is
     current-window intent made durable — the row lives in the log
     like any window's state).
   - `PauseSeries` twin: `Habit.paused_until: Option<NaiveDate>`
     (inclusive pause-through local date; `#[serde(default,
     skip_serializing_if = "Option::is_none")]`); new clock slot
     `paused_until` on `HabitClocks` (contested roaming content).
     Covered windows — those ending on/before `until` — are transparent
     in the walk (no miss, no credit, D-C10 semantics verbatim); the
     transparency composes with skip (Σ property extension, property
     (d): one transparent outcome, not three behaviors).
   - **Σ property extension** (the plan §8 items a–e): transparency
     never counts, never breaks; Σ log invariant unchanged.
2. **Messages/planner (`messages.rs`, `command.rs`)**: `SkipHabit
   { habit }` (window minted planner-side via
   `WindowStart::of_period(kind, day.today(now))` — the `LogHabit`
   posture), `PauseHabit { habit, until: Option<NaiveDate> }`
   (read-modify-write on the field only; `None` resume clears for badge
   hygiene; past-date until ⇒ typed `InvalidHabitPause`; unchanged
   value ⇒ accepted no-op — the `PauseSeries` docs verbatim). Typed
   rejections join `UnknownHabit`/`HabitDeleted`.
3. **Undo journal (`undo.rs`)**: `SkipHabit` gets a pre-image (prior
   row shape or none) — journal-invertible, matching R-7's spirit; the
   skip undo pops via global undo (US-1/S4).
4. **Reminder engine (`reminders.rs`)**: paused habits disarm the
   tracker nudge pass (the quota-gated arm skips paused rows).
5. **CLI (`taskboard-app`)**: `habits skip <id>`;
   `habits pause <id> --until <date>`; `habits resume <id>`.
6. **UI**: W-1 (menu Skip row + `ActionId::HabitSkip` + keymap rows in
   all three presets), W-2 (typed skip chip), W-3 (pause editor dialog,
   destructive-role dismissal law), W-4 (paused badge + through-date),
   `HabitResume` in the context menu; #935 typed NoTarget notices on
   the new verbs (Q5 law); kiosk gating per R-3.
7. **Snapshot tests** (`taskboard-state`): skip mint, skip transparency
   through the engine, pause/resume round-trip, `InvalidHabitPause`,
   nudge disarm; `insta` snapshot updates.

## Serialization

First chunk of the wave (scope report §E: HF1 → HF2 → HF4 → HF3): the
transparent-window walk is the semantics HF3's grid cells and HF4's
scheduled-day rule (weekly/monthly, R-6) build on.

## Test catalogue (typed outcomes only)

- Domain: `skip_window_is_transparent_in_the_streak_walk`,
  `met_window_on_a_skipped_row_still_counts`,
  `pause_covered_windows_are_transparent`,
  `skip_and_pause_transparency_compose` (property),
  `streak_pending_bound_holds_with_transparent_windows` (Σ extension).
- Engine: `skip_habit_routes_the_exact_command`,
  `pause_past_date_is_typed_invalid_pause`,
  `resume_clears_for_badge_hygiene`,
  `log_habit_journal_entry_inverts` (shared with HF2's fixture),
  `paused_habit_arms_no_nudge`.
- Journeys: `us1_s2_skip_dispatches_the_exact_command`,
  `us1_s3_row_shows_the_typed_skip_state`,
  `us2_s2_pause_commit_routes_the_exact_command`,
  `us2_s4_row_shows_the_paused_state` (per the UX plan §6).

## Out of scope (homes)

- Skip of *past* windows — R-4 ruling; `AdjustHabitLog` remains the
  valve. No issue (recorded ruling).
- Week/month per-day skips — R-5; the per-day heatmap backlog issue
  carries the grain question.
