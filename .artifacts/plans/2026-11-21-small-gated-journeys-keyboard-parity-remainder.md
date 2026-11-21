# Chunk PR1 — keyboard parity remainder (#1032, #1057, #1066-roster, #1067, #1068, #1091) — `feat/ui-keyboard-parity-remainder`

- **Date**: 2026-11-21
- **Origin**: UX journey plan
  `.artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md`
  (US-7..US-12; W-8..W-13; R-6, R-7, R-9, R-10). Round-4 dispatch plan
  chunk PR1. Authorities: the O-8 standing policy + case law
  (`.artifacts/plans/2026-11-18-gui-review-round9-round-plan.md`), the
  K1 binding-identity rules (`fix/ui-keymap-chord-case`), the K4
  cursor state (`.artifacts/plans/2026-11-19-row-cursor-walk-plan.md` §9),
  chunk-8's uniform typed-refusal law (#935's route-arm pass).
- **Status**: DRAFT — gated on the journey plan's operator acceptance.
- **Issues**: #1032, #1057, #1066 (roster half; the chunk stays open
  for the remainder per R-6), #1067, #1068, #1091.

## 1. Scope

1. **#1032 row context-menu key** — a new `ActionId::RowMenu` in the
   Board and Today contexts (all three preset tables) opens the
   focused row's context menu through the existing `menu_row_subject`
   fact — click ≡ key by construction (US-7). Task rows gain their
   first keyboard menu opener; habit rows gain the direct one (the
   HabitConvertArm chord's arm-and-reproject stays, untouched).
2. **#1057 HabitRepair chord** — `Ctrl+Shift+d` in the Today context,
   all three preset tables, resolving `ActionId::HabitRepair` through
   the card strip's seam (US-8); census cell updated.
3. **#1066 roster half (R-6 split)** — the break-rules roster
   (Today's TimersSection) adopts the K4 selected-row cursor:
   arm-on-first-vertical-stroke, clamp/prune vs the roster id table,
   clear on view exit / foreign settle, pointer-click coherence, row
   verbs through the row's own seams (US-9). File-to and the planner
   ride PR2; further adopters stay on #1066.
4. **#1067 series pause/resume chords** — `Ctrl+Shift+r` /
   `Ctrl+Shift+u` at TaskDetail, all three presets (US-11), retiring
   chunk 11's Tab-only ruling now that the verbs are scheduled.
5. **#1068 tracker-kind toggle binding** — Std `Ctrl+Shift+k` /
   emacs `C-c k` / vi `g k` at TaskDetail (US-12), adopting the
   research §6 candidates; distinct from `TaskTrackToggle` (the R-8
   smart verb; the chunk-11 naming fork stands).
6. **#1091 rule-cursor disarm** — a new `ActionId::RowCursorDisarm`,
   `Ctrl+Shift+a` at TaskDetail while the rule cursor is armed
   (US-10), composing with the armed-flow grammar; verticals return
   to the chunk-9 control ladder without a view exit or foreign
   settle.

## 2. Work items (journey-id mapped)

- W-8: RowMenu id + adapter resolution + preset rows. Tests:
  `us7_s1_row_menu_key_opens_task_menu`,
  `us7_s2_row_menu_key_opens_habit_menu`,
  `us7_s3_row_menu_no_focus_typed_refusal`.
- W-9: HabitRepair preset rows. Tests:
  `us8_s1_habit_repair_chord_resolves`,
  `us8_s2_repair_no_window_typed_refusal`.
- W-10: roster cursor. Tests: `us9_s1_roster_cursor_arms`,
  `us9_s2_roster_cursor_walks_clamped`, `us9_s3_roster_row_verb_uses_own_seam`.
- W-11: RowCursorDisarm id + preset rows. Tests:
  `us10_s1_disarm_chord_disarms_cursor`, `us10_s2_disarm_unarmed_inert`.
- W-12: series chord rows. Tests: `us11_s1_series_pause_chord_resolves`,
  `us11_s2_series_resume_chord_resolves`.
- W-13: kind-toggle rows. Tests: `us12_s1_kind_toggle_chord_resolves`.
- Keymap contract suite regeneration for every new preset row (no
  silent drops — C-5's resolution rule); census cells for every new
  binding.

## 3. Traps (from the dispatch plan + rulings)

- The **O-8 case law and the K1 binding-identity rules govern every
  new chord**: spelling convention `Ctrl+Shift+t`; binding identity =
  shift bit + case-insensitive letter; `parse()` accepts both
  spellings. Contract tests over every preset's chorded-letter rows.
- **No preset-row silent drops** — the keymap contract suite
  regenerates; a collision with a round-8 or re-homed chord fails the
  suite and the spelling moves, it is never dropped quietly (R-7).
- #1091's disarm **composes with the armed-flow grammar** — it clears
  the cursor, it does not invent a second disarm site beside the
  existing clear sites (view exit, foreign settle, global cancel).
- The chunk-8 D1 no-coining ban is superseded **only** for the three
  recorded ids this plan coins (`RowMenu`, `RowCursorDisarm`, plus
  PR2's none) — that supersession is R-7's recorded decision, not a
  chunk-local choice.
- All refusals are typed outcomes (#935's law); no text assertions.

## 4. Verification

- Journey tests in `tests/journeys/small_gated_journeys.rs`; keymap
  case seam (`keymap_case_seam.rs`) extended per the K1 law.
- `scripts/ux-coverage.sh .artifacts/plans/2026-11-21-small-gated-journeys-ux-plan.md --gate`
  as the stage-2 self-check.
- Quality gates via `scripts/run-quality-gates.sh`.

## 5. Definition of done

- [ ] Journey plan's US-7..US-12 wiring rows implemented and green
- [ ] Keymap contract suite green with the new rows; no drops
- [ ] Gate exits 0 on the journey plan
- [ ] #1032, #1057, #1067, #1068, #1091 close; #1066 comments the
      roster landing and stays open for the remainder (R-6)
