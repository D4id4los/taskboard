# [Report] GUI Review Round 4, Chunk 6 — Task-View Focus & Insert Engagement (Stage 3 Review)

- **Date**: 2026-11-03
- **Branch**: `fix/ui-r4-taskview-focus` — rebased onto `origin/main`
  (the respite/breaks landing 9f3508ee; no conflicts) and shipped as a
  PR (URL in the handoff message). Plan:
  `.artifacts/plans/2026-11-01-ui-r4-taskview-focus-plan.md`.
- **Verdict**: PASS — all dispatch §7 acceptance items (a)–(f)
  verified against the post-rebase diff; two findings raised and
  fixed on the branch (§4); one plan-vs-runtime deviation accepted
  with its mechanism finding recorded (§5.2).

## 1. Functional review (dispatch prompt §2)

- **(a) D20 is genuinely systemic — confirmed.** The review greps:
  `SetMode` writers after the chunk are exactly the flow outcomes
  (`view_state.rs`) plus the two intercept pulls (`adapter.rs` —
  engage/disengage, both guarded: disengage never under an armed flow
  or the picker grid); no `form-edited`/engage/commit-mode residue
  survives anywhere (the timers chain and the settings flips died
  through all layers, the buffer mirroring stayed); the text-focus
  publish exists in exactly one file — `widgets/text-field.slint`
  (publish + init publish + per-widget latch). A new TextField
  anywhere is masked by construction; views cannot forget.
- **(b) The Return-never-hijacks matrix — confirmed.**
  `return_never_hijacks_on_a_focused_field_under_any_preset` runs the
  full grid: standard/emacs/vi × custom reminder offset (the F24
  repro verbatim — type `45m`, Return commits `AddReminderRule`,
  `edit_open` stays false, `vi_insert` mirrors the mask), title field,
  description field, and the today timers' timing field (Return
  creates the timer, never hijacks into a task view). 12 cells, typed
  outcome assertions only.
- **(c) The walk reaches every stop — confirmed.**
  `the_task_view_stop_inventory_walks_in_reading_order` asserts the
  §1.1 inventory in reading order (back/tracking/custom-field occupy
  steps unreported, done=2, title=3, due=5, scheduled=6,
  chips=90–93, description=11), the wrap, and both open-editor
  branches (field replaces cell, one stop). The focused-cell golden
  pins the ring; the manual Tier-4 checklist restates the walk for
  the user (§7).
- **(d) vi/standard unchanged — confirmed.** `presets.rs` moves only
  two EMACS TaskDetail rows and adds two EMACS Settings rows;
  `resolved_preset_keymaps.snap` regenerates with exactly those two
  hunks and nothing else. The generic conflict/shadow/coverage/vi-shape
  contracts pass unmodified; the board-chords contract was extended
  into `emacs_four_chords_walk_controls_on_the_detail_chain` (four
  chords × five contexts, the exclusions pinned positively).
- **(e) The palette still works — confirmed.**
  `the_palette_return_jumps_under_every_preset` pins the exception
  under all three presets (query, Return jumps and closes, the
  needle's board selected, zero commands).
- **(f) Hygiene — confirmed.** Empty diff over
  `taskboard-domain/-state/-storage-sqlite/-sync-nextcloud/-app`; the
  new journeys assert typed outcomes and structural facts only (labels
  appear as finders and a11y echo, never as assertions; the
  populated-description pin is a non-empty structural check).

## 2. The recorded runtime deltas (stage-2, reviewed as deliberate)

- **The control walk was dead since round 3** and this chunk revived
  it: the synthetic Tab re-injection dispatched re-entrantly inside
  the key callback (inert), and every flowless dispatch re-grabbed
  key-root focus over the step just taken. The fix (zero-shot
  `slint::Timer` deferral held on the adapter; the D15 refocus gated
  on the flow-end *transition* via `FlowLiveness`) rides commit
  b314cf64's message and the walk journeys. Reviewed as correct — it
  is what makes D18 testable at all.
- **The `EditingFacts` singleton grew a third property (`generation`)**
  beyond the plan's "exactly two": the reset seams need the stale-
  decrement guard because a focused field that unmounts fires no loss
  event. Justified; the lifecycle journey pins the re-baselining.
- **View swaps now also re-focus the view scope** (`invoke_refocus_view`
  on the view-identity tail): the swap unmounts whatever held slint
  focus; the walk journeys and goldens confirm no focus steal on the
  walk path.

## 3. Scope guard

`taskboard-ui-slint` only, plus CHANGELOG and the plan/report
artifacts. No date-picker phase machinery (chunk 7's seam contract is
recorded in the plan §3.6, not implemented), no reminder-row content.

## 4. Findings raised by this review, fixed on the branch (c95f7808)

- **Latch/publish triplication** (mandate: single definition of a
  subtle discipline): the 18-line focus-count latch was copy-pasted in
  `ActivationStop` (local to task-view), `PresetChip`, and the done
  box. Consolidated into `widgets/activation-stop.slint`; five of the
  six sites now derive from it. The done box must keep its own scope —
  Slint requires `accessible-role` to be a constant expression and
  that stop is a checkbox — its copy documents the constraint and
  points at the shared definition. Its key handler gained the modifier
  guards the shared grammar always had (bare-stroke activation,
  matching the reservation arm; a behaviour tightening, journeys
  green).
- **Dangling artifact reference**: the hover-sweep journey's doc
  comment pointed at a "slice report" that exists nowhere; the
  mechanism finding is now stated inline.

## 5. Deviations accepted, not fixed

1. **Field hover tips (D21's per-field arm) are not delivered.** The
   plan's TextField tip arm is infeasible on the pinned runtime: the
   inner `TextInput` accepts its own hover moves, so a sibling
   `TouchArea` never hovers and one above breaks caret handling. The
   commit (e0918ffe) and the journey doc-comment record the finding;
   the CHANGELOG entry claims only cells and chips. **Follow-up: file
   a `backlog:` ui issue for field-site tooltips** (a labelled-field
   wrapper like settings' `LabeledField` is the likely mechanism).
2. The dispatch acceptance (e) reads "every task-view interactive
   element" — with the fields tip-less, the sweep covers the cells,
   chips, checkbox, header IconButtons, and the tracked total (the
   chunk-5 arms). The gap is exactly the recorded limitation above,
   not a wiring forget.

## 6. Flake note (not this branch's bug)

The first gate run failed in `taskboard-app::cli_habits h02_*` — the
watchlisted local-vs-UTC calendar-window flake (issue #560), active
because the run crossed local midnight. The full suite re-ran after
01:00 CET; that failure is unrelated to this diff (empty over
`taskboard-app`).

## 7. Tier-4 manual checklist (for the user, on the reference machine)

- Tab walk over the whole task view under all three presets: ring
  visible at every stop (title, done, due, scheduled, description,
  chips), no dead positions, wrap behaves.
- Emacs: C-n/C-p/C-f/C-b all walk; walk to due → Return opens the
  editor (the two-phase entry); type into the custom reminder field
  and press Return — commits the offset, never arms the title.
- Click-away commit on a settings field then immediate typing in
  another field — no dead keys between fields.
- Palette under emacs: type, arrows, Return jumps; Escape closes.
- Hover sweep: every closed cell and chip tips on hover; no
  empty-line tips.
- Scale 0.5 / 3.0: rings and chip hit-targets stay legible.
