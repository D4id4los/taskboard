# HF2 — Habit Undo Completion & Inline Quota Edit (implementation plan)

- **Date**: 2026-11-21
- **Kind**: implementation chunk plan (HF2 of the habit family wave) —
  undo-journal classification change + row/popover surfaces.
- **Origin**: `.artifacts/plans/2026-11-21-habit-family-ux-plan.md`
  (stories US-3, US-4; wiring W-5; rulings R-7); issues **#956**
  (undo), **#961** (inline quota edit). O-2 (additive domain), O-7 Q4
  (shared editor surface), O-6 (widget centralization: the popover
  rides `AnchoredOverlayHost`, the stepper rides the W-primitives).
- **Branch**: `feat/habit-undo-quota`
- **Status**: DRAFT — chunking after the operator accepts the family UX plan.

## Scope

1. **Recorded ruling R-7 (supersession, with the dated census note)**:
   `LogHabit`'s `UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)`
   becomes a journal-invertible pre-image — the prior `(count, clock)`
   of the affected window's row (or the row's absence). Invert restores
   that exact pre-image (clock included — the #986 bump mint makes the
   restored clock deterministic on replay, so journal replay re-mints
   identically; the B1 clock-bump rule is load-bearing here and is NOT
   changed). `AdjustHabitLog` and `SkipHabit`-adjacent audited writes
   keep the `HabitSurface` fence. **Census note (dated 2026-11-21,
   per the UX plan R-7)**: the habit undo fence cells gain the dated
   supersession text "LogHabit journal-invertible since 2026-11-21
   (habit-family UX plan R-7)". Per-increment undo: each `LogHabit` is
   one journal entry; a same-second burst pops one increment per undo
   (the bump's monotone clocks distinguish rows — engine test US-3
   AC-2).
2. **Engine (`taskboard-state`)**: `undo.rs` classification change +
   invert arm; snapshot tests for the revert and the
   `AdjustHabitLog`-newest typed fence outcome.
3. **Quota edit — engine**: zero new commands; the popover and the
   editor both route the existing `UpdateHabit { changes: HabitChanges
   { target: Some(..) } }`. Zero/empty draft never routes (commit
   blocked, typed invalid-draft state — the `NonZeroCount` mint's law
   surfaced at the UI boundary, no validate-and-forget).
4. **Quota edit — UI (W-5)**: the row's progress phrase ("3/8")
   becomes clickable (desktop; kiosk disabled per R-3) mounting a
   popover on `AnchoredOverlayHost`: stepper over the count (≥ 1) and
   the period-kind picker locked to the current kind (kind change via
   the full editor — inline is a one-number edit, ruling recorded in
   the UX plan §2 habit.quota_edit cell); commit routes
   `UpdateHabit`; D-H12 former-kind rows are never touched (existing
   domain test re-run green). Keyboard: `ActionId::HabitQuotaEdit` on
   the focused row (O-8 parity); #935 notices on refusal.
5. **Keymap**: `HabitQuotaEdit` rows in all three presets (O-8).

## Serialization

Second chunk (HF1 → HF2 → HF4 → HF3). File-adjacency note (research
§6): both HF1 and HF2 touch `adapter.rs` habit seams — keep the chunks
serialized or strictly file-disjoint (they are serialized per the
round shape).

## Test catalogue (typed outcomes only)

- Engine: `log_habit_journal_entry_inverts`,
  `undo_after_burst_pops_one_increment`,
  `adjust_habit_log_newest_keeps_the_typed_fence`,
  `skip_habit_journal_entry_inverts` (HF1 fixture, asserted here for
  the fence boundary),
  `quota_edit_routes_the_exact_update_habit`.
- Journeys: `us3_s2_undo_reverts_the_logged_completion`,
  `us4_s1_quota_popover_mounts_from_the_row_phrase`,
  `us4_s2_quota_commit_routes_update_habit`,
  `us4_s3_zero_draft_blocks_commit` (per the UX plan §6).
- Domain: existing D-H12 inertness test re-run green (US-4 AC-4).

## Out of scope (homes)

- Undo of `AdjustHabitLog` — R-7 keeps the fence (recorded ruling,
  no issue).
- A habit-specific undo *history list* (beyond the global stack) —
  not requested; if wanted, a `backlog:` issue would be filed then.
