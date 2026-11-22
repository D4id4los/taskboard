# Chunk D1i Implementation Plan — the GUI restore surfaces + ClearReminderRules first-class

- **Date**: 2026-11-22
- **Kind**: per-chunk implementation plan (round-5 backlog reduction,
  undo family, chunk D1i of two).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md`
  (this plan maps to its US-1..US-4, W-1..W-7, R-1..R-6, D-1..D-4);
  issues #1146 (labels/habits legs) and #1154; the round-5 scope
  report §D wave 4; the landed base: `RestoreLabel`/`RestoreHabit`
  engine verbs + pinned receipts (#993), the #1187 undoability ruling,
  UF1's `PreImage::ReminderRule` catalogue.
- **Status**: DRAFT — the journey plan is GATE-PAUSED pending operator
  acceptance; dispatch may not start until the operator accepts it.
- **Branch**: `feat/restore-clear-surfaces` (one PR; worktree
  `../taskboard-WT-feat-restore-clear-surfaces`, created with
  `git worktree add -b` per AGENTS §8 after `scripts/worktree-doctor.sh`).
- **Issues**: #1146, #1154 (close on merge; #1146's time-entry leg
  stays open per the journey plan's R-1 — comment the partial close).

## 1. Work breakdown

Ordered so every step keeps the tree green:

1. **Domain: the `RestoreRoster` projection** (journey plan W-1/G1,
   D-1): a publish-derived, additive `AppState` field —
   `RestoreRoster { tombstoned_labels: Vec<RestoreRow>,
   tombstoned_habits: Vec<RestoreRow> }`,
   `RestoreRow { id, name }` (hand-rolled newtypes per the A1 law; no
   new ids minted — `LabelId`/`HabitId` ride verbatim). Derive at
   every publish beside `UndoView::derive`; bounded (cap at the
   roster's natural size — tombstones are rare; assert the bound in a
   fold test); never persisted. Type-driven check: `RestoreRow` is a
   plain projection record, not a trust-boundary mint — no fallible
   constructor needed; document the leniency at the type.
   - Tests: `crates/taskboard-domain/src/roster/tests.rs` —
     `us1_s1_restore_roster_derives_tombstoned_rows`,
     `us1_s1_live_rows_stay_out_of_the_roster`, plus the bound test.
   - Snapshot: the one deliberate insta regeneration pass; any corpus
     diff beyond the additive field is a finding.
2. **Domain: `ClearReminderRules` undo fence** (W-7): no engine
   change — UF1's synthesis already landed. Add the fold test
   `undo/tests.rs::us3_s5_clear_reminder_rules_undo_restores_all_rules`
   (one undo step restores n rules) if UF1's corpus does not already
   carry it under another name; if it does, bind instead of
   duplicating (one-behavior-one-test).
3. **Slint: the labels overlay restore section** (W-2, US-1):
   `LabelsPanel` gains a bounded deleted-labels section fed from the
   roster projection; the new `label_restore_requested` callback;
   desktop-qualified (R-5 — absent on kiosk). Armed-confirm ceremony
   per D-2 before dispatch.
4. **Slint + adapter: the habits context restore row** (W-3, US-2):
   `habit_restore_asked` on the habits composition's context menu,
   armed confirm, desktop-qualified.
5. **Slint + adapter: the rule-list clear-all row** (W-4, US-3):
   `rules_clear_asked`, destructive-last (BF-W-N2), disabled at zero
   rules, armed confirm.
6. **Adapter: seams + dispatch** — `install_seam!` sites for the three
   callbacks dispatching the exact `StateCommand` payloads
   (`RestoreLabel { id }` / `RestoreHabit { id }` /
   `ClearReminderRules { task }`); roster index → typed id resolution
   adapter-side (the UI stays id-less); register all three in
   `seam_inventory.rs`; receipts ride the unchanged
   `arm_outcome_notice` lane (W-6).
7. **Journey tests** (journey plan §8):
   `tests/journeys/undo_family_restore_clear_surfaces_journeys.rs`
   with the ten named tests, driven through the existing test driver.

## 2. Recorded decisions (implementation-level; the journey plan's D-1..D-4 stand)

| ID | Decision | Rationale |
|---|---|---|
| I-1 | The armed-confirm ceremony reuses the existing ceremony composition (the delete-confirm grammar); no new ceremony widget | Same verb class, same widget (consistency & standards); a bespoke restore dialog would be a third grammar for one concept |
| I-2 | Roster index → id resolution happens at the adapter seam, keyed on the published projection's row order | The UI is id-less; the projection is the stable contract — re-derive races are bounded by the publish lockstep |
| I-3 | The kiosk absence is structural (platform-qualified mount), not a disabled row | R-5; the UF2 kiosk-probe precedent asserts absence structurally |

## 3. Test & gate plan

- Journey tests: the journey plan §8's ten, step-ID-named, in the
  per-plan journey file (the naming law).
- Seam tests: exact-command payload assertions; Escape-at-strip
  asserts zero dispatch (typed, never text).
- Kiosk gate: composition probe asserting the three affordances'
  platform-qualified absence.
- Stage-2 self-check: `scripts/ux-coverage.sh
  .artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md
  --gate` must exit 0 before handoff (planning-time run: 34 checks,
  0 AMBIGUOUS, exit 1 solely from the pre-landing MISSING rows —
  stage 2 resolves them by landing the wiring and tests).
- Quality gates: `scripts/run-quality-gates.sh` per AGENTS §10;
  memory-gate routing for any heavy single build.

## 4. Handoff

- PR `feat/restore-clear-surfaces` → `main`; hand off at PR-open +
  `gh pr checks` green (the dispatcher owns the coverage trio, the CI
  watch, teardown, and the at-merge closures).
- On merge: close #1154; comment #1146 (labels/habits legs landed;
  the time-entry GUI leg stays open per R-1/D-3); run the stage-3
  walkthrough protocol over the journey plan (the armed strips' copy
  and the deleted-section prominence are the Manual items).
