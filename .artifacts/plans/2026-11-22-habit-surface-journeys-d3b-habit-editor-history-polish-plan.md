# Chunk D3b — Habit editor/history polish (#1218, #1144, #1212, #1035, #1048, #1049) — `feat/habit-editor-history-polish`

- **Date**: 2026-11-22
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md`
  (US-6..US-11; W-9..W-15; R-1, R-2, R-6). Round-5 dispatch plan
  chunk D3b. Authorities: **the #1212 operator ruling (comment
  7079171625) is the contract — blank = anchor-time-only, the 09:00
  mint retired, the additive `Option` time change pre-authorized**;
  the O-2 law (the widening vocabulary RowSubject::Task | Habit); the
  O-1 window law and D-H12 (amended by this plan's recorded US-8
  decision, subject to plan acceptance); the US-7 tombstone-matching
  resolver (AC-5 pinned); the dead-subject push pass (US-11's twin).
- **Status**: DRAFT — GATE-PAUSED on the journey plan's operator
  acceptance.
- **Issues**: #1218, #1144, #1212, #1035, #1048, #1049.

## 1. Scope (journey-id mapped)

1. **#1212 ruled blank-time semantics (US-6)** — W-9:
   `HabitEditDraft::minted_reminder_override` stops minting the 09:00
   fallback; a blank time field with valid base/offset/snooze mints
   an override carrying **no time** (`HabitReminderOverride.time`
   becomes `Option<NaiveTime>` — the pre-authorized additive domain
   change; serde absent/None round-trips; the nudge arm reads
   `anchor_time` first, then the override time, then no clock time
   arms). The field's placeholder/doc state "blank = anchor time"
   (walkthrough-judged, never text-asserted). The clear leg and the
   `reminder_valid` typed save gate are unregressed. CLI twins of the
   override parse get the same optional-time shape (one semantics,
   every boundary).
2. **#1218 tombstoned habit history entry (US-7)** — W-10: the search
   surface lists tombstoned habits (typed tombstone bit, no forged
   live-row state) with a History affordance; activation routes
   ActionId::HabitHistory → UiEffect::HabitHistoryOpen through the
   tombstone-matching resolver; the dialog projects the retained log
   ("log outlives"), display-only, cancel-writes-nothing. The
   R-6 decision: search-mount, independent of D1i sequencing; a
   restore-surface affordance stays a P2 home on #1146.
3. **#1144 per-day heatmap cells (US-8)** — W-12: an additive
   per-day log overlay fact keyed (habit, day), written alongside the
   window log by the landed LogHabit/AdjustHabitLog path (no new
   command); W-11: the history dialog's month grid renders one cell
   per day from the overlay; missing facts render empty (no
   invention); **the stats oracle stays window-canonical (R-2)** —
   the D-H12 amendment is this plan's recorded decision, valid only
   under the journey plan's acceptance. Storage/sync shape for the
   overlay is stage-1's to detail (additive; no LWW semantic change —
   the #1095 whole-second guard composes untouched).
4. **#1035 nudge-schedule preview (US-9)** — W-13: a display-only
   preview row in the editor's schedule/override section projecting
   the derived nudge day/time/snooze offers from the draft via the
   landed `nudge_day` / `tracker_anchor_occurrence` (pure read, no
   dispatch); invalid draft → neutral state; blank time (US-6)
   → anchor-time phrasing, never an invented clock time.
5. **#1048 mirrors onto shared flows (US-10)** — W-14:
   Flow::EditTitle / Flow::EditDescription widen to
   RowSubject::Task | Habit; the habit variant mounts the chunk-5
   shared editor surfaces; its two private draft-backed mirror fields
   go. Task behavior is the regression oracle (US-10/S3).
6. **#1049 push-side kiosk belt (US-11)** — W-15: the reconcile/push
   pass steps the view back when `kiosk && subject_habit().is_some()`
   (the dead-subject pass's twin); the dispatch-time gates stay the
   first line; kiosk-rig journey test pins the belt; desktop
   unregressed.

## 2. Work items

- W-9 (US-6): optional-time override. Tests:
  `us6_s1_blank_mints_time_none`, `us6_s2_named_time_mints_some`,
  `us6_s3_clear_leg_unregressed`,
  `us6_s4_save_gate_refuses_invalid_drafts`; the CLI twin's seam test
  rides the same domain change.
- W-10 (US-7): search-mounted tombstone history. Tests:
  `us7_s1_search_mounts_tombstone_history_row`,
  `us7_s2_dialog_projects_tombstoned_log`,
  `us7_s3_dialog_cancel_writes_nothing`,
  `us7_s4_live_row_history_unregressed`.
- W-11/W-12 (US-8): per-day overlay + day cells. Tests:
  `us8_s1_day_cells_render_overlay_facts`,
  `us8_s2_window_adherence_oracle_unregressed`; storage/engine-side
  unit tests for the overlay write/read at the lowest layer
  (one-behavior-one-test — the GUI projection test does not
  duplicate them).
- W-13 (US-9): nudge preview. Tests:
  `us9_s1_preview_projects_derived_schedule`,
  `us9_s2_invalid_draft_hides_preview`,
  `us9_s3_blank_time_preview_phrases_anchor`.
- W-14 (US-10): flow widening. Tests:
  `us10_s1_habit_variant_mounts_shared_editors`,
  `us10_s2_habit_description_flow_mounts_shared_surface`,
  `us10_s3_task_flows_unregressed`.
- W-15 (US-11): kiosk belt. Tests:
  `us11_s1_kiosk_push_steps_view_back`,
  `us11_s2_desktop_push_unregressed`.

All in `crates/taskboard-ui-slint/tests/journeys/habit_surface_journeys.rs`
(except the overlay's engine/storage units, which live at their own
lowest layer).

## 3. Traps (from the dispatch plan + rulings)

- **#1212 is the contract, not a choice**: the 09:00 mint retires;
  `unwrap_or`-class fallbacks are forbidden at the mint; the stage-1
  verification (can the override type express absent-time?) is
  already answered — it cannot (`time: chrono::NaiveTime`,
  habit.rs:290), so the pre-authorized additive change applies.
  Serde compatibility: old payloads with a concrete time decode to
  `Some`; new payloads without one decode to `None`; no
  default-to-09:00 anywhere.
- **#1144's overlay is the plan's riskiest item**: it is a deliberate
  D-H12 amendment (R-2), accepted only via the journey plan. The
  window log stays the adherence oracle; the overlay is never
  authoritative for stats; a window log and its overlay disagreeing
  is a bug class to pin at the write path (both facts written in one
  transaction of the landed command handling).
- **#1218 reads, never forges**: the tombstone row carries no
  mutable live-row verbs (log/edit/quota stay absent — the matrix's
  R-6 posture); the resolver's tombstone match is the landed US-7
  AC-5 path, not a new lookup.
- **#1048's widening is the O-2 vocabulary only** — no extra flow
  variants, no behavior change for tasks; #951's LabeledValueRow is
  not this chunk.
- **#1049's belt mirrors the dead-subject pass** — one mechanism, no
  second close path; the h4 kiosk dispatch-gate journey stays green.
- All refusals typed; no text assertions; clock-dependent fixtures
  follow the clock discipline (AGENTS §5 / testing_strategy §6.1–6.2).

## 4. Verification

- Journey tests in `tests/journeys/habit_surface_journeys.rs`.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh`; heavy builds
  through `scripts/memory-gate.sh`.

## 5. Definition of done

- [ ] Journey plan's US-6..US-11 wiring rows implemented and green
- [ ] Gate exits 0 on the journey plan
- [ ] The ruled #1212 semantics land verbatim (09:00 mint gone; the
      placeholder/doc carry the blank meaning)
- [ ] The window-canonical stats oracle pinned green beside the new
      day cells
- [ ] #1218, #1144, #1212, #1035, #1048, #1049 close (closures are
      the dispatcher's at merge)
