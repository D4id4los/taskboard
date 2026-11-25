# Chunk C3b — Schedule Start+Due + Colour UX (#1428/#1440/#1439) — Plan Skeleton

<!-- Skeleton written by the J-C2/C3 journey-stage agent (2026-11-25).
The stage-2 agent COMPLETES this plan at gate clearance. Do not
dispatch before the journey plan is accepted. -->

- **Branch**: `feat/schedule-colour-ux`
- **Origin**: journey plan
  `.artifacts/plans/2026-11-25-sound-today-colour-journeys-schedule-colour-ux-plan.md`
  (status: GATE-PAUSED — operator acceptance pending); issues
  #1428, #1440, #1439.
- **Body references**: #1428, #1440, #1439. All close on merge.

## 1. Diagnosis (code base, journey-confirmed)

- The schedule quick verbs (`router.rs:2189-2203`) write
  `scheduled`-only and are mutually inconsistent: today commits
  `today_last_instant` = `last_covered_instant(window_end)`;
  tomorrow commits the **raw exclusive end** of
  `days::next_day_window`. Routing context already carries
  `day_window`/`today_last_instant`/`tomorrow_window_end`
  (adapter.rs:1965-1980); `day_start_time` is typed
  (`SettingsEntry::day_start_time`, settings.rs:223); `DayCalendar`
  (days.rs:177) with `instant(date, time)`, `window_start`,
  `day_window_end`, `last_covered_instant` are the N-4 building
  blocks.
- Colour: the 6-swatch const `SWATCHES` (presentation.rs:606) feeds
  the labels-overlay swatch row (labels-overlay.slint:287-299 via
  models.rs:3193-3197) and the board colour strip (models.rs:2252-2256,
  board-bar.slint). The colour draft model is tag-color D-5
  (colour-field :243-272, validity echo, swatch changed-watch
  re-seed, live preview). `swatch_hex` (models.rs:3345) is the A1
  lenient mint; no picker exists.

## 2. Decisions (ruled by the journey plan — implement as written)

- D-1: new pure mints `day_first_instant` / `day_last_instant` on
  `days.rs`; both schedule verbs commit `UpdateTask` with
  `scheduled` + `duedate` in one sparse mask; `TaskDueToday`
  unchanged; no new command variant.
- D-2: tomorrow commits the last *covered* instant (raw exclusive
  end retired).
- D-3: picker ↔ HEX field: the draft string is the single truth; the
  picker is a projection of the last valid draft; invalid
  intermediates never clobber; hex→HSV→hex proptest identity.
- D-4: picker = hue strip + SV square, pure Slint, HSV internal,
  keyboard-operable by arrow nudges (the keyboard-first law).
- D-5: 9 bases (6 current + orange, brown, turquoise) × 3 tones,
  HSL-derived (±~15% lightness, saturation held; brown = low-
  lightness orange ladder; grey varies lightness only), pure
  proptest-ed derivation in Rust presentation code; row wraps.
- D-6: the board strip consumes the same derived palette.

## 3. Implementation outline

1. `days.rs`: the two mints + doc (the N-4 citation); unit +
   proptest window-membership coverage across `day_start_time`.
2. `router.rs`: both schedule arms emit the pair-write (ctx already
   has the facts; extend the minting site in the adapter ctx for the
   first-instant + tomorrow-last-covered facts).
3. `presentation.rs`: `derived_swatches()` (pure) replacing the const
   consumers; models.rs swatch row + board strip feed from it.
4. `ui/compositions/colour-picker.slint`: the new widget (drag
   surfaces, arrow nudges); mount in labels-overlay beside the
   colour field; the two-way draft path (W-3/W-4 seams of the
   journey plan §6).
5. Journey tests: `crates/taskboard-ui-slint/tests/journeys/schedule_colour_journeys.rs`
   — the step-ID-named list in the journey plan §8; proptests for
   hex round-trip + tone ladders.

## 4. Quality gates

- Pre-commit hooks + flock-wrapped full nextest (dispatcher owns the
  coverage trio).
- `scripts/ux-coverage.sh <journey plan> --gate` green once the
  journey tests land.
- Contrast: the palette tones used against chip text pass the
  extended token tests (#1415 treatment; journey plan US-26 AC-4).

## 5. Risks

- Round-trip drift (hex↔HSV quantization): the proptest identity
  must hold on the *minted* 6-hex domain — define the projection
  quantization before writing the widget.
- Menu row count growth: the schedule verbs keep their existing rows
  (effects-only change) — no menu reshuffle.
- Swatch row at 27 tones: wrap grid, not scroll; verify kiosk overlay
  layout at narrow widths.

## 6. Out-of-scope findings → backlog issues

Filed by the journey stage: runtime contrast warning; free-form board
colours (R-3); alpha/eyedropper/wheel (R-4); a schedule-without-due
variant. New findings → `backlog:` issues immediately.

## 7. Estimated diff

~500–800 lines: days mints + tests (~100), router arms (~40),
derived palette (~120 + proptests), picker widget + overlay mount
(~250), journey tests (~250).

## 8. Stage-2 completion checklist (fill at gate clearance)

- [ ] days.rs mint signatures + line cites; the router arm diffs
- [ ] Adapter ctx minting site for the start/tomorrow facts
- [ ] Picker widget seams (`hue-picked` / `sat-val-picked`) in
  slint + adapter + seam_inventory.rs
- [ ] Palette derivation constants + proptest names
- [ ] Contrast token-test extension result
- [ ] Full local hook/nextest evidence recorded
