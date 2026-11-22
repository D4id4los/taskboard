# Chunk D3a — Today-habits interaction polish (#1199, #1201, #1202, #1208, #1196) — `feat/today-habits-interaction-polish`

- **Date**: 2026-11-22
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md`
  (US-1..US-5; W-1..W-8; R-3, R-4, R-5). Round-5 dispatch plan chunk
  D3a. Authorities: the round-5 scope report §D; A1's fixed
  selection/DnD grammar (chunk `fix/today-habit-interaction` — this
  chunk sequences after A1 or rebases onto it); the K4 cursor law and
  the O-8 chord case law; the R-11 section-scope ruling as superseded
  for the Log chord (R-5, plan §2 — the census diff recorded there);
  the T-23 typed habit-log undo side (landed).
- **Status**: DRAFT — GATE-PAUSED on the journey plan's operator
  acceptance.
- **Issues**: #1199, #1201, #1202, #1208, #1196.

## 1. Scope (journey-id mapped)

1. **#1199 card redesign (US-1)** — W-1: TaskCard gains an optional
   trailing (right-side) action slot (the base-widget ask — optional
   buttons on both sides, not a habit-specific hack); W-3: the
   detached strip goes, the redundant Log habit button with it; W-2:
   the done slot carries a typed target-met bit and renders the
   filled/done visual until the window resets. The landed
   `undo-log-clicked` / `repair-clicked` seams re-home into the card
   unchanged — same callbacks, same commands (US-1/S2–S3), so the
   typed undo side (T-23) and #1057's repair chord compose untouched.
2. **#1201 double-click opens the editor (US-2)** — W-4: new
   `habit-edit-asked` callback on the card → ActionId::HabitEdit →
   UiEffect::HabitEdit; the kiosk refusal arm is the landed gate, not
   re-plumbed (US-2/S2); single-click keeps selecting only (composes
   with A1's fixed selection grammar — one selection truth).
3. **#1202 keyboard parity remainder (US-3)** — W-6: habit rows join
   the Today item walk (the K4 cursor state; no second mechanism);
   Enter activates (US-3/S2, the W-4 seam); W-5: ActionId::HabitLog
   gains the Ctrl+Shift+l Today preset chord in all three preset
   tables — the R-5 supersession (dated, in the journey plan §2);
   US-3/S5 re-checks the landed RowMenu path; the keymap contract
   suite regenerates (no silent drops, the C-5 rule).
4. **#1208 pause editor escape (US-4)** — W-7: the section-level
   pause editor gains Escape (routed through the global escape/cancel
   preset) and outside-click dismissal, both resolving into the
   existing `habits-pause-cancelled` seam — cancel writes nothing,
   the destructive-role dismissal law; commit path pinned unregressed.
5. **#1196 typed quota seed (US-5)** — W-8: the
   UiEffect::HabitQuotaEdit arm's seed drops `i32::try_from(...)
   .unwrap_or(1)`; the popover draft widens so no fallback exists
   (preferred) or the seed clamps at the draft max with the clamp
   documented at the type (value-preserving alternative — stage 2
   picks per what NonZeroCount's inner type admits; both are typed,
   neither can seed 1 silently). Kiosk refusal arm untouched.

## 2. Work items

- W-1/W-2/W-3 (US-1): TaskCard slot + strip removal + target-met bit.
  Tests: `us1_s1_log_button_removed_checkbox_logs`,
  `us1_s2_undo_repair_in_card_trailing`,
  `us1_s3_undo_log_dispatches_adjust`,
  `us1_s4_target_renders_filled_slot`,
  `us1_s5_keyboard_paths_survive_the_move`.
- W-4 (US-2 + US-3/S2): double-click/Enter → editor. Tests:
  `us2_s1_double_click_opens_habit_editor`,
  `us2_s2_kiosk_double_click_typed_refusal`,
  `us2_s3_single_click_selects_only`,
  `us3_s2_enter_activates_habit_editor`.
- W-5/W-6 (US-3): walk + log chord. Tests:
  `us3_s1_walk_crosses_habit_rows`,
  `us3_s3_log_chord_resolves_over_focused_row`,
  `us3_s4_no_focus_typed_refusal`,
  `us3_s5_row_menu_opens_habit_menu`; keymap contract suite
  regeneration.
- W-7 (US-4): pause-editor escape/outside. Tests:
  `us4_s1_escape_cancels_pause_editor`,
  `us4_s2_outside_click_cancels_clean`,
  `us4_s3_commit_path_unregressed`.
- W-8 (US-5): typed seed. Tests: `us5_s1_seed_clamps_typed_no_floor`,
  `us5_s2_quota_commit_routes_update_habit`.

All in `crates/taskboard-ui-slint/tests/journeys/habit_surface_journeys.rs`.

## 3. Traps (from the dispatch plan + rulings)

- **A1 composition**: this chunk starts after A1 lands (or rebases
  onto it). The habit row's click/drag/gap grammar is A1's fixed
  base — the double-click and trailing-button work must not
  reintroduce the gesture conflicts A1 diagnoses; single-click
  selects (US-2/S3) is the regression tripwire.
- The **R-5 supersession is plan-level** (journey plan §2, dated,
  operator acceptance pending): the chord routes over the focused row
  exactly like the landed HabitQuotaEdit key; no per-row dispatch
  bypass of `ctx.habit_target`.
- The target-met bit is **window-aware** (day/week/month resets via
  the landed period vocabulary) — a bit that only ever flips on log
  is a finding.
- #1208's escape composes with the global escape/cancel preset — it
  must not register a second global Escape handler (the C-5 collision
  rule).
- #1196's fix is at the seed site only — the draft type change does
  not touch the quota commit's NonZeroCount mint.
- All refusals typed (never text assertions); no `tokio::time::sleep`
  in tests; walk tests poll predicates with deadlines.

## 4. Verification

- Journey tests in `tests/journeys/habit_surface_journeys.rs`.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh`; heavy builds
  through `scripts/memory-gate.sh`.

## 5. Definition of done

- [ ] Journey plan's US-1..US-5 wiring rows implemented and green
- [ ] Gate exits 0 on the journey plan
- [ ] Keymap contract suite green with the W-5 row; no drops
- [ ] A1's selection/DnD journeys unregressed
- [ ] #1199, #1201, #1202, #1208, #1196 close (closures are the
      dispatcher's at merge)
