# Chunk D1i Implementation Plan — the GUI restore surfaces + ClearReminderRules first-class

- **Date**: 2026-11-22
- **Kind**: per-chunk implementation plan (round-5 backlog reduction,
  undo family, chunk D1i of two).
- **Origin**: the accepted UX journey plan
  `.artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md`
  (this plan maps to its US-1..US-6, W-1..W-12, R-1..R-6,
  D-1..D-6 — **amended 2026-11-23** by the operator's zero-input
  ruling: D-3 superseded by D-5, N1 reopened, US-5/US-6 + W-8..W-12
  added); issues #1146 and #1154; the round-5 scope report §D wave 4;
  the landed base: `RestoreLabel`/`RestoreHabit` engine verbs + pinned
  receipts (#993), the #1187 undoability ruling, UF1's
  `PreImage::ReminderRule` catalogue, and the diagnosis fact that
  `DeleteTimeEntry` already journals `PreImage::TimeEntry { id,
  before: Some(row) }` (the arm-agnostic capture, `undo.rs:589-595`)
  with the `DeletedTimeEntry` inverse synthesizing the restore from
  it — the zero-input *undo* exists; this chunk adds the zero-input
  *out-of-line forward restore*.
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
6. **Adapter: seams + dispatch** — `install_seam!` sites for the
   callbacks dispatching the exact `StateCommand` payloads
   (`RestoreLabel { id }` / `RestoreHabit { id }` /
   `ClearReminderRules { task }` /
   `RestoreTimeEntryFromJournal { from }`); roster index → typed id
   resolution adapter-side (the UI stays id-less); register all in
   `seam_inventory.rs`; receipts ride the unchanged
   `arm_outcome_notice` lane (W-6/W-12).
7. **Domain: the zero-input time-entry restore verb** (W-10, D-5,
   added 2026-11-23): `StateCommand::RestoreTimeEntryFromJournal {
   from: JournalId }` — a thin wrapper that reads the journal entry at
   `from`, requires its pre-image to be
   `PreImage::TimeEntry { before: Some(row) }` (typed refusals:
   not-a-delete-entry, `before: None`, no such journal entry), and
   internally applies the existing serde-pinned `RestoreTimeEntry {
   entry: row }` verbatim. No soft-delete (D-6: the pre-image already
   carries the payload; tombstoning would churn the tracking shape
   for nothing). The verb journals `PreImages` of its own writes so
   undo re-deletes (the #1187 law — the existing
   `restore_time_entry_undo_re_deletes_the_restored_row` fence covers
   the composed path).
   - Tests (`crates/taskboard-domain/src/undo/tests.rs`):
     `us5_ac3_none_preimage_refuses_typed`,
     `us5_s3_from_journal_restores_verbatim_and_journals`,
     `us6_s1_cli_verb_and_gui_verb_share_the_engine_arm`.
8. **Domain: the roster's deleted-entries section** (W-8): widen
   `RestoreRoster` with `deleted_entries: Vec<DeletedEntryRow {
   entry_id, journal_id, label }>` — derived at every publish by
   scanning the bounded journal ring for `PreImage::TimeEntry {
   before: Some(_) }` deletes whose row is currently absent; the
   label is engine-derived (task title + span). Retention is the
   honest bound (journey plan N5).
   - Tests (`crates/taskboard-domain/src/roster/tests.rs`):
     `us5_s1_deleted_entries_derive_from_delete_preimages`,
     `us5_s1_absent_row_or_none_preimage_stays_out`.
9. **Slint + adapter: the track-entries restore section** (W-9, US-5):
   `entry_restore_requested` on `ui/compositions/track-entries.slint`
   — the deleted-entries section, armed confirm, desktop-qualified.
   The confirm dispatches `RestoreTimeEntryFromJournal { from }` —
   the command payload carries the journal id only (the zero-input
   pin is a journey test assertion: `us5_s3_restore_from_journal_dispatches_id_only`).
10. **CLI: `track restore --from <journal-id/prefix>`** (W-11, US-6):
    prefix resolution over the journal ids the roster/`tasks history`
    surface; typed refusal mapping (`not-a-delete-entry`,
    `none-preimage`, unknown/ambiguous prefix). The pre-existing flag
    grammar stays untouched as the out-of-band recovery path — never
    the undo path (D-5).
    - Tests (`crates/taskboard-app/tests/it_restore_cli.rs`):
      `us6_s1_track_restore_from_journal_dispatches_and_receipts`,
      `us6_s2_nondelete_target_refuses_typed`.
11. **Journey tests** (journey plan §8):
    `tests/journeys/undo_family_restore_clear_surfaces_journeys.rs`
    with the fourteen named tests, driven through the existing test
    driver.

## 2. Recorded decisions (implementation-level; the journey plan's D-1..D-4 stand)

| ID | Decision | Rationale |
|---|---|---|
| I-1 | The armed-confirm ceremony reuses the existing ceremony composition (the delete-confirm grammar); no new ceremony widget | Same verb class, same widget (consistency & standards); a bespoke restore dialog would be a third grammar for one concept |
| I-2 | Roster index → id resolution happens at the adapter seam, keyed on the published projection's row order | The UI is id-less; the projection is the stable contract — re-derive races are bounded by the publish lockstep |
| I-3 | The kiosk absence is structural (platform-qualified mount), not a disabled row | R-5; the UF2 kiosk-probe precedent asserts absence structurally |
| I-4 | **(2026-11-23, D-5)** `RestoreTimeEntryFromJournal` is a wrapper, not a reimplementation: the payload-carrying `RestoreTimeEntry` stays the single write path (serde-pinned, #993) and the wrapper only resolves the pre-image | Duplicating the write path would fork the receipt/journal semantics; the `RestoreStack { id }` id-only precedent, journal-sourced |
| I-5 | **(2026-11-23, D-6)** No soft-delete for time entries | The journal pre-image already satisfies the zero-input bar; tombstones would add a second recovery mechanism and churn `tracking`/`appdata`/export/sync shapes |

## 3. Test & gate plan

- Journey tests: the journey plan §8's fourteen, step-ID-named, in the
  per-plan journey file.
- Seam tests: exact-command payload assertions — including the
  zero-input pin (the `RestoreTimeEntryFromJournal` payload carries
  the journal id only); Escape-at-strip asserts zero dispatch (typed,
  never text).
- Kiosk gate: composition probe asserting the four affordances'
  platform-qualified absence.
- Stage-2 self-check: `scripts/ux-coverage.sh
  .artifacts/plans/2026-11-22-undo-family-journeys-restore-clear-surfaces.md
  --gate` must exit 0 before handoff (post-amendment planning-time
  run: 0 AMBIGUOUS, exit 1 solely from the pre-landing MISSING rows —
  stage 2 resolves them by landing the wiring and tests).
- Quality gates: `scripts/run-quality-gates.sh` per AGENTS §10;
  memory-gate routing for any heavy single build.

## 4. Handoff

- PR `feat/restore-clear-surfaces` → `main`; hand off at PR-open +
  `gh pr checks` green (the dispatcher owns the coverage trio, the CI
  watch, teardown, and the at-merge closures).
- On merge: close #1154 and **close #1146 in full** (the D-5
  amendment ships the time-entry GUI leg; the original
  "labels/habits-only" partial close no longer applies); run the
  stage-3 walkthrough protocol over the journey plan (the armed
  strips' copy, the deleted-sections' prominence, and the new Manual
  question — does the deleted-entries label (task + span) identify
  the row well enough? — are the Manual items).
