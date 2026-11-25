# Chunk J-B3 — Habit repeat/frequency editor redesign (#1430) — `fix/habit-repeat-editor`

- **Date**: 2026-11-25
- **Origin**: the amended J3 habit-surface UX journey plan
  `.artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md` —
  the 2026-11-25 amendment appendix (§10) and **US-12 (P1)** with
  wiring row **W-17**, rulings **R-7/R-8**; round-8 scope report
  chunk B3
  (.artifacts/reports/2026-11-25-backlog-reduction-round8-scope-report.md
  §B). Normative source: **issue #1430**. Supporting authority: the
  layout review's a11y label inventory
  (.artifacts/reports/2026-11-25-layout-review-analyzer-findings.md;
  SC 3.3.2, SC 4.1.2, recognition over recall).
- **Status**: DRAFT — GATE-PAUSED on the journey amendment's operator
  acceptance. The stage-2 agent opens **GATE CLEARED** once the
  operator accepts the amendment; dispatch must not start before.
- **Issues**: #1430 (p2, accessibility, ui, enhancement).
- **Implementation branch**: `fix/habit-repeat-editor` (eventual; one
  PR for the chunk — the redesign is one coherent deliverable).

## 1. Scope (journey-id mapped)

All presentation-only over the landed habit editor grammar — no
command, UiRequest or ActionId delta (journey §7). Everything below
lives in `ui/views/task-view.slint` (habit variant, the #1430 line
refs 583–644), the ToggleStrip widget variant it mounts, and
`ui/assets/icons/`.

1. **Labelled day strip (US-12/S1)** — the PerWeek ToggleStrip day
   options (task-view.slint:583–603) render their visible weekday
   label beside each toggle, each with accessible-label and tooltip;
   no bare checkmark glyphs remain in the repeat section.
2. **Two day grammars titled/distinguished or merged (US-12/S2)** —
   the weekly-anchor strip and the "Scheduled days" checkbox group
   (task-view.slint:610–627, the checked-label "Remove <Day>" /
   unchecked "Add <Day>" rows) carry distinct section titles and
   one-line purpose captions, or are merged — the walk-aﬀordance
   decision is stage-2's to make within the journey's escape clause
   (titles alone must pass recognition-over-recall at walkthrough).
   No domain-semantics merge without an operator ruling (the
   amendment's non-goal).
3. **Icons, not checkmarks, on the mode selector (US-12/S3)** — the
   Nudge Day / Period start ToggleStrip (task-view.slint:637–644
   under the 635–636 caption) swaps checkmarks for semantically
   fitting Lucide icons from the vendored set
   (`crates/taskboard-ui-slint/ui/assets/icons/`, via the IconKind
   ternary chain, ui/widgets/icon.slint:94+); calendar-day / sun /
   moon / alarm family evaluated first per #1430's icon inventory.
   A second icon set only with licensing + slint @image-url
   compatibility research recorded in the PR — never hand-drawn
   one-offs.
4. **Name/role/value pass (US-12/S4)** — every interactive control in
   the repeat/frequency section carries accessible-label +
   accessible-description; the audit oracle is the layout-review
   accessible-name inventory method.
5. **Add-repeat arm structurally absent for habits (US-12/S5, R-8)** —
   the #1294 clear-all precedent: removed from the habit editor's
   widget tree, not display-gated. Repetition stays task-subject
   only.
6. **Vertical-space compaction (US-12/S6)** — the labels section and
   repeat rows fold to the compact labelled grammar per #1430's
   screenshots; no labelled control is removed in the process.
7. **Save/reopen unregression (US-12/S7)** — the redesigned controls
   write the same typed draft the landed grammar wrote; the journey
   test pins it.

## 2. Work items

- W-17 (US-12): all seven steps. Tests in
  `crates/taskboard-ui-slint/tests/journeys/habit_surface_journeys.rs`:
  `us12_s1_day_options_carry_visible_labels`,
  `us12_s2_day_grammars_titled_and_distinguished`,
  `us12_s3_mode_selector_icons_not_checkmarks`,
  `us12_s4_every_control_named_role_value`,
  `us12_s5_add_repeat_absent_for_habits`,
  `us12_s6_repeat_section_compact`,
  `us12_s7_save_reopen_unregressed`.
  Presentation legs assert structural facts (typed bits, icon-kind
  enum values, inventory presence) — never label or tooltip text
  (AGENTS.md §5 no-text-assertions law). Stage-1 of the dispatch
  re-runs `scripts/ux-coverage.sh <journey-plan> --gate` as the
  self-check; stage 3 re-runs it plus the walkthrough protocol for
  the amended journey (the gate is never CI).

## 3. Traps (from the issue + the amendment's rulings)

- **Presentation-only discipline**: any behavior change to the
  schedule/override semantics is out of scope (US-6's ruled
  semantics untouched); a semantics defect discovered during the
  redesign becomes a `backlog:` issue immediately (AGENTS.md §7
  standing authorization), never an in-chunk fix.
- **R-8 is structural**: hiding the add-repeat row with a visibility
  bit fails US-12/S5; the habit-subject arm goes from the widget
  tree.
- **Icon law**: evaluate the vendored Lucide family first; the
  second-set research (licensing + @image-url) is a recorded PR
  artefact only if actually needed — do not pre-build it.
- **Kiosk**: the habit editor stays dispatch-refused on kiosk (the
  landed typed gates); the redesign changes nothing about the gates.
- **The quality gates run through `scripts/run-quality-gates.sh`**
  and every cargo command through `scripts/memory-gate.sh`
  (AGENTS.md §10); pre-commit hooks are the docs-stage gate, the
  full trio is the dispatcher's.

## 4. Verification

- Journey tests listed in §2 (all in the per-plan journeys file,
  names embedding their step tokens).
- `scripts/ux-coverage.sh
  .artifacts/plans/2026-11-22-habit-surface-journeys-ux-plan.md
  --gate` exits 0 (US-12 rows resolve).
- Full quality-gate sequence per AGENTS.md §10 before handoff; PR
  open + `gh pr checks` green is the handoff state.

## 5. Definition of done

- [ ] Operator acceptance of the 2026-11-25 journey amendment
      (GATE-PAUSED until then; stage-2 opens GATE CLEARED)
- [ ] US-12/S1..S7 land with their journey tests green
- [ ] The a11y walkthrough pass over the repeat section is executed
      at stage 3; findings resolved or homed as backlog issues
- [ ] No checkmark glyphs remain in the habit repeat section
- [ ] The add-repeat arm is structurally absent for the habit
      subject
- [ ] Save/reopen unregressed (US-12/S7 green)
- [ ] Quality gates green; PR `fix/habit-repeat-editor` open
