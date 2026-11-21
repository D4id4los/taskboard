# HF3 — Habit History View: Heatmap, Streak Badge, Statistics (implementation plan)

- **Date**: 2026-11-21
- **Kind**: implementation chunk plan (HF3 of the habit family wave) —
  the history/visualization view on the W1 ModalDialog/base-widget
  layer + display-side upgrades.
- **Origin**: `.artifacts/plans/2026-11-21-habit-family-ux-plan.md`
  (stories US-7, US-8, US-9; wiring W-8, W-9, W-10; rulings R-3, R-9,
  R-10); issues **#957** (heatmap/history — the prime motivator),
  **#962** (streak badge), **#963** (statistics). O-6 (widget
  centralization: `ModalDialog`, `Badge`, `AnchoredOverlayHost`
  base widgets), O-7 Q2 (Today-only habit mount — the dialog mounts
  from Today/editor, not boards).
- **Branch**: `feat/habit-history-view`
- **Status**: DRAFT — chunking after the operator accepts the family UX plan.

## Scope

1. **Domain (pure statistics, `taskboard-domain/src/habit.rs`)**:
   - `adherence(habit, log, scheduled, now, day) -> AdherenceRatio` —
     met windows ÷ scheduled windows; transparent windows (skipped /
     paused / off-schedule, HF1+HF4's semantics) leave both numerator
     and denominator; **empty history ⇒ the typed empty state** (no
     0%-of-nothing posture, US-9 AC-3).
   - `best_streak(habit, log, ..) -> u32` — max over historical runs
     of the walk (bounded backward scan; the walk's own saturation
     logic).
   - `per_period_totals(habit, log, ..) -> BTreeMap<WindowStart, u32>`
     — the count column of the grid.
   - All pure, total, projection-only over `HabitState.log`; D-H12
     former-kind rows inert (never render as misses).
2. **History view (`ui/views/habit-history-view.slint`, W-8)** — a W1
   `ModalDialog` in the **read role** (outside-click = close, the
   dismissal law's read-dialog posture):
   - month grid: one cell per canonical window of the visible month,
     colored by the typed per-window state (met / unmet-pending /
     skipped / paused / off-schedule / missed / inert) — the states
     are an enum in the presentation model, tones via the Theme
     tone-ladder (O-6), never text-bits;
   - month pager (back/forward, clamped at the log's edges);
   - statistics section (US-9) rendering the domain fns' typed values;
   - mount points: habit context menu History row, habit editor
     History row, `ActionId::HabitHistory`; desktop-only (kiosk
     absent, R-3); zero commands — pure projection (W-8's no-command
     reason per journey steps US-7/S2–S4).
3. **Streak badge (W-9, #962)**: the row model's 🔥N emoji slot is
   replaced by the `Badge` base widget (O-6) carrying the streak
   value + tooltip semantics (typed model value asserted; label text
   judged at walkthrough). Row model change in
   `presentation.rs::habit_row_model`; kiosk renders it (display-only).
4. **CLI**: `habits history <id>` (text projection of the window
   states) and `habits stats <id>` (adherence/best/totals) — O-5
   parity (every GUI verb has its CLI twin and vice versa).
5. **Tests**: journey tests in the family file (§6 of the UX plan);
   domain unit/property tests for the three statistics fns incl. the
   scheduled-window denominator (HF4's law) and the empty-history
   typed state; fold tests for the state enum and D-H12 inert cell.

## Serialization

Fourth chunk, display-side last (scope report §E: HF1 → HF2 → HF4 →
HF3 display-side): consumes HF1's transparency states and HF4's
scheduled-window denominator + statistics fns' denominator law.

## Test catalogue (typed outcomes only)

- Domain: `adherence_denominator_is_scheduled_windows`,
  `transparent_windows_leave_numerator_and_denominator`,
  `best_streak_is_the_max_historical_run`,
  `empty_history_is_the_typed_empty_state`,
  `former_kind_rows_render_inert_never_missed` (fold/property).
- Journeys: `us7_s1_history_dialog_mounts_from_the_menu`,
  `us7_s2_history_grid_renders_the_typed_window_states`,
  `us7_s3_pager_clamps_at_the_edges`,
  `us7_s4_close_leaves_zero_state_change`,
  `us8_s1_row_renders_the_streak_badge`,
  `us9_s1_statistics_render_the_typed_values` (per the UX plan §6).
- Kiosk gates: dialog absent on kiosk; badge renders (fold test).

## Out of scope (homes)

- Per-day cells inside weekly/monthly windows — `backlog:` issue
  (window-grain rendering only in this plan; the log's grain is the
  window).
- History on the Review view — R-9 (recorded ruling).
- Cross-habit aggregate dashboards (all-habits heatmap) — not
  requested; would be a separate journey if ever asked.
