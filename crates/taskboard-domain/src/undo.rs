// SPDX-License-Identifier: MIT OR Apache-2.0
//! The undo planning layer (ADR 0019): the journal wrapper
//! ([`plan_journaled_command`]), the pre-image/watermark capture pass,
//! and the inverse-synthesis catalogue with [`plan_undo`]/[`plan_redo`].
//!
//! Binding contract (R-U8 + the chunk decisions):
//! - **Pure.** Same input set as [`crate::command::plan_command`] plus
//!   the journal; no IO, no runtime.
//! - **Same batch.** The journal entry rides the command's own batch as
//!   a `RecordJournal` action, appended last — the applied batch and the
//!   interpreted batch stay identical (the single-advance rule).
//! - **`plan_command`'s catalogue is untouched.** Pre-images are
//!   captured *after* planning, from the pre-state, by walking the
//!   plan's `Upsert*` actions — arm-agnostic, cascades captured for
//!   free.
//! - **The synthesis match is the lockstep** (§244's pattern): an
//!   exhaustive no-wildcard `match` over `StateCommand` — a new arm
//!   fails compilation until it gets a classification row.
//! - **The checked watermark fields are exactly the inverse's write
//!   set** (D7), never the observation machinery (`remote`,
//!   `remote_seen`, `remote_baseline`, validators — D-U3/R-U12).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};

use crate::command::{CommandError, CommandOutcome, CommandPlan, plan_command};
use crate::days::DayCalendar;
use crate::idgen::IdGenerator;
use crate::journal::{
    EntityStamps, JournalEntity, JournalEntry, JournalState, NotUndoableReason, PreImage,
    UndoFacts, UndoField, UndoWatermark,
};
use crate::messages::{LabelChanges, StateCommand, TaskChanges};
use crate::outbox::{OpId, PendingOp};
use crate::persistence::PersistenceAction;
use crate::reminders::ReminderDefaults;
use crate::state::AppState;

/// The undo/redo scope gate (#355): `Global` is the exact existing
/// semantics of `Undo`/`Redo`; `Board` narrows the frontier question to
/// one board's tag. The scope predicate: an entry is **in scope** iff
/// `entry.board == Some(board)` — entries tagged `None` (reminder/
/// tracking ephemera, reflections, the honest-ambiguity case where
/// `touched_board`'s lookup chain is gone) belong to *no* board scope,
/// while the global scope still reaches them. Scoping gates the
/// frontier, never walks through the middle of the line (the strict
/// frontier rule, D-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoScope {
    /// The whole line, unfiltered (the `Undo`/`Redo` arms' semantics).
    Global,
    /// Only entries tagged with this board are in scope.
    Board(crate::ids::BoardId),
}

/// The scope predicate (D-3): does this entry tag fall inside the scope?
#[must_use]
fn scope_covers(scope: UndoScope, board: Option<crate::ids::BoardId>) -> bool {
    match scope {
        UndoScope::Global => true,
        UndoScope::Board(scope_board) => board == Some(scope_board),
    }
}

/// The scoped undo's typed refusal (D-5): an empty line asked from a
/// board scope still answers the board-scoped question.
fn nothing_to_undo(scope: UndoScope) -> CommandError {
    match scope {
        UndoScope::Global => CommandError::NothingToUndo,
        UndoScope::Board(board) => CommandError::NothingToUndoForBoard { board },
    }
}

/// The scoped redo's typed refusal (D-5), symmetric.
fn nothing_to_redo(scope: UndoScope) -> CommandError {
    match scope {
        UndoScope::Global => CommandError::NothingToRedo,
        UndoScope::Board(board) => CommandError::NothingToRedoForBoard { board },
    }
}

/// Plans one command *with* journal capture — the engine's one planner
/// entry point (ADR 0019 §5).
///
/// `Undo`/`Redo` route to [`plan_undo`]/[`plan_redo`] before the
/// catalogue sees them; everything else plans through
/// [`plan_command`], then — accepted and batch non-empty — captures the
/// pre-images and watermark from the pre-state and appends
/// `RecordJournal`. An accepted no-op (`NoOp` outcomes plan zero
/// actions) and `RequestSync` journal nothing: the empty-batch rule
/// makes "accepted *mutating* command" mechanical (D15).
///
/// # Errors
///
/// [`CommandError`] verbatim from the catalogue, or the undo line's
/// typed refusals.
#[allow(clippy::too_many_arguments)] // the injected seams ARE the constructor (plan_command's own shape)
pub fn plan_journaled_command(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    command: &StateCommand,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    match command {
        StateCommand::Undo => plan_undo(
            app,
            outbox,
            journal,
            UndoScope::Global,
            ids,
            defaults,
            day,
            now,
            author,
        ),
        StateCommand::Redo => plan_redo(
            app,
            outbox,
            journal,
            UndoScope::Global,
            ids,
            defaults,
            day,
            now,
            author,
        ),
        StateCommand::UndoOnBoard { board } => plan_undo(
            app,
            outbox,
            journal,
            UndoScope::Board(*board),
            ids,
            defaults,
            day,
            now,
            author,
        ),
        StateCommand::RedoOnBoard { board } => plan_redo(
            app,
            outbox,
            journal,
            UndoScope::Board(*board),
            ids,
            defaults,
            day,
            now,
            author,
        ),
        StateCommand::UndoTo { target } => plan_undo_to(
            app, outbox, journal, *target, ids, defaults, day, now, author,
        ),
        StateCommand::RedoTo { target } => plan_redo_to(
            app, outbox, journal, *target, ids, defaults, day, now, author,
        ),
        // The zero-input journal-addressed time-entry restore (the
        // undo-family amendment D-5 of 2026-11-23): the wrapper's whole
        // job is reading the journal, so it plans here — before the
        // journal-blind catalogue. The typed refusals (unknown id /
        // not-a-delete / `before: None`) fire before anything plans;
        // the resolved row rides the serde-pinned `RestoreTimeEntry`
        // internally (the single write path, I-4), and the journaled
        // entry records the *wrapper* intent with the restore's own
        // `PreImages` — undoing it re-deletes (the #1187 law). An
        // already-restored equal row is the accepted no-op (the
        // empty-batch rule journals nothing, the terminal-repeat
        // discipline).
        StateCommand::RestoreTimeEntryFromJournal { from } => {
            let row = journaled_time_entry_pre_image(journal, *from)?;
            let wrapper = StateCommand::RestoreTimeEntryFromJournal { from: *from };
            let plan = plan_command(
                app,
                outbox,
                &StateCommand::RestoreTimeEntry {
                    entry: Box::new(row.clone()),
                },
                ids,
                defaults,
                day,
                now,
                author,
            )?;
            if plan.actions.is_empty() {
                // Accepted no-op: the row is already present verbatim.
                return Ok(plan);
            }
            let entry = record_entry(app, &wrapper, &plan.actions, journal, ids, now);
            let mut actions = plan.actions;
            // Appended last (D16), the shared shape.
            actions.push(PersistenceAction::RecordJournal(entry));
            Ok(CommandPlan {
                actions,
                outcome: plan.outcome,
            })
        }
        command => {
            let plan = plan_command(app, outbox, command, ids, defaults, day, now, author)?;
            if plan.actions.is_empty() {
                // Accepted no-op: nothing changed, nothing journaled.
                return Ok(plan);
            }
            let entry = record_entry(app, command, &plan.actions, journal, ids, now);
            let mut actions = plan.actions;
            // Appended last (D16): after all effects; order-free at
            // apply, reads naturally in the batch.
            actions.push(PersistenceAction::RecordJournal(entry));
            Ok(CommandPlan {
                actions,
                outcome: plan.outcome,
            })
        }
    }
}

/// The `PreImages` facts literal shared by every undoable §3 row: the
/// arm-agnostic capture over the plan's actions plus the post-write
/// watermark. The classification arms keep their own constraint
/// comments and distinct dispatch — only the shared mint is factored.
fn journaled_pre_images(app: &AppState, actions: &[PersistenceAction]) -> UndoFacts {
    UndoFacts::PreImages {
        images: pre_images(app, actions),
        watermark: watermark_of(actions),
    }
}

/// The zero-input restore's pre-image resolution (D-5): the journal
/// entry at `from` must be a `DeleteTimeEntry` whose captured facts
/// carry `PreImage::TimeEntry { before: Some(row) }` — the row is the
/// verbatim restore payload. Typed refusals, in resolution order:
/// unknown id (evicted beyond the retention ring or never existed),
/// not-a-delete entry, `before: None`.
fn journaled_time_entry_pre_image(
    journal: &JournalState,
    from: crate::ids::JournalId,
) -> Result<crate::tracking::TimeEntry, CommandError> {
    let entry = journal
        .entry(from)
        .ok_or(CommandError::UnknownJournalEntry(from))?;
    let StateCommand::DeleteTimeEntry { id } = &entry.command else {
        return Err(CommandError::JournalEntryNotTimeEntryDelete(from));
    };
    let UndoFacts::PreImages { images, .. } = &entry.facts else {
        // Unreachable for a #901-era delete (it journals `PreImages`),
        // but the pre-#901 `NotUndoable` decode ghost keeps the arm
        // honest — the refusal names the entry either way.
        return Err(CommandError::JournalEntryNotTimeEntryDelete(from));
    };
    images
        .iter()
        .find_map(|image| match image {
            PreImage::TimeEntry {
                id: pre_id,
                before: Some(row),
            } if pre_id == id => Some((**row).clone()),
            _ => None,
        })
        .ok_or(CommandError::JournalEntryPreImageMissing(from))
}

/// The recorded post-write row for one habit-log window, or `None`
/// when the watermark holds no stamp for it (the caller refuses with
/// the entity in hand — `refuse` names entity + field). Shared by the
/// the `SkipHabit` / `AdjustHabitLog` / `LoggedHabit` inverse classes.
fn habit_log_post(
    watermark: &UndoWatermark,
    key: crate::habit::HabitLogKey,
) -> Option<&crate::habit::HabitLogEntry> {
    match watermark.0.get(&JournalEntity::HabitLog(key)) {
        Some(EntityStamps::HabitLog(post)) => Some(post),
        _ => None,
    }
}

/// Assembles the journal entry for one accepted mutating command: the
/// classification decides `PreImages` vs `NotUndoable` (the §3 table;
/// the synthesis match in [`synthesize_undo`] is the same table's other
/// half and the compilation lockstep).
#[allow(clippy::too_many_lines, clippy::match_same_arms)] // one classification table; the arms ARE the §3 rows
fn record_entry(
    app: &AppState,
    command: &StateCommand,
    actions: &[PersistenceAction],
    journal: &JournalState,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> JournalEntry {
    let facts = match command {
        // ---- the 13 v1-undoable arms (§3) -------------------------
        StateCommand::CreateTask { .. }
        | StateCommand::CaptureTask { .. }
        | StateCommand::UpdateTask { .. }
        | StateCommand::SetTaskDone { .. }
        | StateCommand::MoveTask { .. }
        // The Today compact-move pair (US-14): positional edits like
        // `MoveTask` — the pre-image task row drives the inverse, so
        // the pair shares the `MoveTask` inverse class verbatim.
        | StateCommand::TaskMoveUp { .. }
        | StateCommand::TaskMoveDown { .. }
        | StateCommand::DeleteTask { .. }
        | StateCommand::CreateStack { .. }
        | StateCommand::RenameStack { .. }
        | StateCommand::MoveStack { .. }
        | StateCommand::CreateLabel { .. }
        | StateCommand::UpdateLabel { .. }
        | StateCommand::AssignLabel { .. }
        | StateCommand::UnassignLabel { .. }
        | StateCommand::RestoreTask { .. }
        // The #356 reclassification: `DeleteStack` journals the full
        // cascade pre-images (stack + every tombstoned task — the
        // arm-agnostic capture gets them for free), so its inverse is
        // synthesizable (D-1/D-3). `RestoreStack` rides the same
        // surface (D-2): its pre-image is the tombstoned stack, and its
        // inverse is the ordinary `DeleteStack` cascade (D-5).
        | StateCommand::DeleteStack { .. }
        | StateCommand::RestoreStack { .. }
        // The #901 restore-family reclassification (D-2): the three
        // deletes journal their cascade pre-images (label + stamped
        // membership removals / the entry row / the habit tombstone —
        // the arm-agnostic capture gets them for free), and the three
        // restore verbs journal `PreImages` of their own writes, so
        // undoing a restore re-deletes (D-1/D-5).
        | StateCommand::DeleteLabel { .. }
        | StateCommand::DeleteTimeEntry { .. }
        | StateCommand::DeleteHabit { .. }
        | StateCommand::RestoreLabel { .. }
        | StateCommand::RestoreTimeEntry { .. }
        | StateCommand::RestoreHabit { .. }
        // The bulk rosters journal ONE fact carrying every row's
        // pre-image (#1345): the arm-agnostic capture reads the merged
        // plan's upserts verbatim, so one undo synthesizes one
        // re-delete per restored row.
        | StateCommand::RestoreHabitsBulk { .. }
        | StateCommand::RestoreLabelsBulk { .. }
        // The zero-input wrapper journals the restore's own `PreImages`
        // exactly like its payload-carrying sibling (D-5): undoing it
        // re-deletes (the #1187 law).
        | StateCommand::RestoreTimeEntryFromJournal { .. }
        // The pause is a one-field task edit under the recur clock
        // (issue #506): the recur pre-image inverts it exactly (the
        // UpdateTask class restores the whole rule verbatim).
        | StateCommand::PauseSeries { .. } => journaled_pre_images(app, actions),
        // ---- the typed families (§3 rows; the lockstep's other half)
        StateCommand::CreateBoard { .. }
        | StateCommand::BindBoard { .. }
        | StateCommand::RebindBoard { .. }
        | StateCommand::RenameBoard { .. }
        | StateCommand::SetBoardColor { .. }
        | StateCommand::SetBoardArchived { .. }
        | StateCommand::DeleteBoard { .. }
        | StateCommand::ResetBoard { .. }
        | StateCommand::UnbindBoard { .. }
        | StateCommand::ReconcileByTitle { .. } => {
            UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony)
        }
        StateCommand::SnoozeReminder { .. }
        | StateCommand::SetTaskRemindersMuted { .. }
        | StateCommand::DismissReminder { .. }
        | StateCommand::CreateAdhocAlarm { .. }
        | StateCommand::CancelAdhocAlarm { .. }
        | StateCommand::CreateBreakRule { .. }
        | StateCommand::DeleteBreakRule { .. } => {
            UndoFacts::NotUndoable(NotUndoableReason::ReminderSurface)
        }
        // The #354 rule-verb reclassification (UF1): the three task-rule
        // edits journal their rule pre-images (the arm-agnostic capture
        // walks the `UpsertReminderRule` / `DeleteReminderRule` actions —
        // the `PreImage::ReminderRule` row exists since #900), so the
        // add's inverse is the ordinary remove and a remove/clear's
        // inverse is the #900 verbatim rule-resurrect (no fresh id, no
        // re-stamp — the D-3 law). Mute stays `ReminderSurface` (one
        // self-healing toggle at its own seam), and the snooze /
        // dismissal / ad-hoc rows stay out by the hard constraint: a
        // delivered notification is a recorded fact, never retractable
        // (the UF1 dispatch trap (a)).
        StateCommand::AddReminderRule { .. }
        | StateCommand::RemoveReminderRule { .. }
        | StateCommand::ClearReminderRules { .. } => journaled_pre_images(app, actions),
        // ---- the undo v2 tracking/habit widening (#555, §3 rows
        // 35′–42′; three record-time dynamic rules per §6/§7) ----------
        StateCommand::LogTime { .. }
        | StateCommand::CreateHabit { .. }
        | StateCommand::MoveHabit { .. }
        // The reorders journal every touched row's pre-image (chunk A1
        // D1 for habits; B3 #1247 extends the law to task stacks):
        // the inverse restores the whole section, not one row.
        | StateCommand::HabitReorder { .. }
        | StateCommand::TaskReorder { .. } => journaled_pre_images(app, actions),
        StateCommand::AdjustTimeEntry { id, .. } => {
            // D23/§7.1: an adjust that stamps an end onto an open entry
            // is inexpressible to invert — the change mask structurally
            // cannot clear an end (ADR 0017 §4), so refuse-and-name at
            // record time instead of journaling facts the verb cannot
            // deliver on.
            let closes_open_entry = matches!(
                actions
                    .iter()
                    .find(|action| matches!(action, PersistenceAction::UpsertTimeEntry(entry) if entry.id == *id)),
                Some(PersistenceAction::UpsertTimeEntry(entry)) if entry.end.is_some()
            ) && app
                .tracking
                .entries
                .get(id)
                .is_some_and(|entry| entry.end.is_none());
            if closes_open_entry {
                UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
            } else {
                journaled_pre_images(app, actions)
            }
        }
        StateCommand::UpdateHabit { changes, .. } => {
            // D25/§7.3: the conversion seam carries no clock slot
            // (D-H7a), so a seam-addressed edit has nothing to
            // watermark against — the whole edit refuses, never a
            // partial restore with the seam left unrestored.
            if changes.continued_from.is_some() {
                UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
            } else {
                journaled_pre_images(app, actions)
            }
        }
        StateCommand::AdjustHabitLog { habit, window, .. } => {
            // D24/§7.2: an adjust onto a window with no pre-existing row
            // is lossy to invert — the `count: 0` inverse would forge an
            // explicit count-0 row where the pre-state had none (the
            // exact stored-fact lossiness; `LogHabit` no longer shares
            // this fence — journal-invertible since 2026-11-21
            // (habit-family UX plan R-7) — the audited correction valve
            // itself keeps it, per R-7's scope). Pre row present (any
            // count, 0 included) inverts exactly.
            let key = crate::habit::HabitLogKey {
                habit: *habit,
                window: *window,
            };
            if app.habits.log.contains_key(&key) {
                journaled_pre_images(app, actions)
            } else {
                UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
            }
        }
        // ---- the exclusions stay, reasons restated (§4) ----------------
        StateCommand::StartHabitTracking { .. } | StateCommand::StopHabitTracking { .. } => {
            // Structurally non-invertible (the tracking-surface law):
            // the inverse would have to re-open closed entries — no
            // command expresses that (ADR 0017's ruling, habit-side).
            UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
        }
        StateCommand::StartTracking { .. } | StateCommand::StopTracking => {
            // Structurally non-invertible (ADR 0017): the inverse would
            // have to re-open closed entries — no command expresses
            // that. Also dismissal side effects (D-T5).
            UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
        }
        StateCommand::LogHabit { .. } => {
            // Journal-invertible since 2026-11-21 (habit-family UX plan
            // R-7 — the dated supersession of the former
            // `NotUndoable(HabitSurface)` classification, #956): the
            // pre-image is the prior row shape **or none**
            // (`PreImage::HabitLogRow` over the touched window) and the
            // inverse restores the prior count through the existing
            // audited `AdjustHabitLog` — the #986 bump mint re-stamps
            // the restored row deterministically, so journal replay
            // re-mints identically (the B1 clock-bump rule is
            // load-bearing here and unchanged). Per-increment
            // undoability: one `LogHabit` is one journal entry; a
            // same-second burst pops one increment per undo (the
            // bump's monotone clocks distinguish rows). `AdjustHabitLog`
            // (D24) and the conversion seam (D25) keep their fences.
            journaled_pre_images(app, actions)
        }
        StateCommand::SkipHabit { .. } | StateCommand::UnskipHabit { .. } => {
            // Journal-invertible (HF1, 2026-11-21 — the habit-family UX
            // plan R-7's spirit): the pre-image is the prior row shape
            // **or none** (`PreImage::HabitLogRow` before: Option); the
            // inverse flips the typed mark via the mirror verb, never
            // forging a row shape. The count stays untouched either way.
            journaled_pre_images(app, actions)
        }
        StateCommand::PauseHabit { .. } => {
            // The pause lives on the habit row, but `HabitChanges` has
            // no `paused_until` leg (the sparse-edit change mask never
            // grew one), so the standard UpdateHabit inverse cannot
            // express the restore — refuse-and-name, the D25 posture.
            UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
        }
        StateCommand::RecordReflection { .. }
        | StateCommand::UpdateReflection { .. }
        | StateCommand::DeleteReflection { .. } => {
            // DR-7 D-J7: the inverse of a record is a tombstone (a
            // different stored fact than absence); un-delete would
            // resurrect with a minted-then-inverted clock; the family's
            // own verbs are the recovery surface.
            UndoFacts::NotUndoable(NotUndoableReason::ReflectionSurface)
        }
        StateCommand::ConvertSeriesToHabit { .. }
        | StateCommand::ConvertHabitToSeries { .. } => {
            // The seam-addressed composites (issue #506, the D25
            // extension): the `continued_from` seam deliberately
            // carries no clock slot (D-H7a), so there is nothing to
            // watermark the seam restore against, and the inverse —
            // restore the rule *and* remove the minted habit /
            // un-tombstone the retired one — is unexpressible in the
            // one-inverse-command synthesis. The chain stays readable
            // from state; the undo line refuses-and-names.
            UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
        }
        // Never journaled (the wrapper routes the undo/redo arms;
        // RequestSync plans an empty batch and never reaches this point
        // — all arms stay for totality).
        StateCommand::Undo
        | StateCommand::Redo
        | StateCommand::UndoOnBoard { .. }
        | StateCommand::RedoOnBoard { .. }
        | StateCommand::UndoTo { .. }
        | StateCommand::RedoTo { .. }
        | StateCommand::RequestSync
        | StateCommand::UpdateSettings { .. } => {
            UndoFacts::NotUndoable(NotUndoableReason::BoardCeremony)
        }
    };
    JournalEntry {
        id: ids.new_journal_id(),
        parent: journal.head,
        command: command.clone(),
        facts,
        ops: actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::EnqueueOp(op) => Some(op.op_id),
                _ => None,
            })
            .collect(),
        // Flipped only by `MarkJournalPushed` (the R-U9 post-pass) —
        // never by planning.
        pushed: false,
        board: touched_board(app, journal, command),
        created_at: now,
    }
}

/// The best-effort touched-board tag (D-U4's per-board note deferred to
/// v2): task → stack → board; stack/label → their board; `None` when
/// not board-scoped or the lookup chain is gone (ambiguity is honest).
#[allow(clippy::match_same_arms)] // the tag table reads per §3 row; merging hides it
fn touched_board(
    app: &AppState,
    journal: &JournalState,
    command: &StateCommand,
) -> Option<crate::ids::BoardId> {
    use crate::ids::BoardId;
    let task_board = |task: crate::ids::TaskId| -> Option<BoardId> {
        app.tasks
            .get(&task)
            .and_then(|t| app.stacks.get(&t.stack))
            .map(|s| s.board)
    };
    match command {
        StateCommand::CreateTask { stack, .. } => app.stacks.get(stack).map(|s| s.board),
        StateCommand::CaptureTask { board, .. } => *board,
        StateCommand::UpdateTask { id, .. }
        | StateCommand::SetTaskDone { id, .. }
        | StateCommand::MoveTask { id, .. }
        | StateCommand::TaskMoveUp { id }
        | StateCommand::TaskMoveDown { id }
        | StateCommand::TaskReorder { id, .. }
        | StateCommand::PauseSeries { head: id, .. }
        | StateCommand::DeleteTask { id }
        | StateCommand::SetTaskRemindersMuted { task: id, .. }
        | StateCommand::AddReminderRule { task: id, .. }
        | StateCommand::ClearReminderRules { task: id }
        | StateCommand::StartTracking { task: id, .. }
        | StateCommand::LogTime { task: id, .. }
        | StateCommand::AssignLabel { task: id, .. }
        | StateCommand::UnassignLabel { task: id, .. } => task_board(*id),
        StateCommand::CreateStack { board, .. }
        | StateCommand::CreateLabel { board, .. }
        | StateCommand::BindBoard { board, .. }
        | StateCommand::RebindBoard { board, .. }
        | StateCommand::RenameBoard { board, .. }
        | StateCommand::SetBoardColor { board, .. }
        | StateCommand::SetBoardArchived { board, .. }
        | StateCommand::DeleteBoard { board }
        | StateCommand::ResetBoard { board }
        | StateCommand::UnbindBoard { board }
        | StateCommand::ReconcileByTitle { board } => Some(*board),
        StateCommand::RenameStack { id, .. }
        | StateCommand::DeleteStack { id }
        | StateCommand::RestoreStack { id }
        | StateCommand::MoveStack { id, .. } => app.stacks.get(id).map(|s| s.board),
        StateCommand::UpdateLabel { id, .. }
        | StateCommand::DeleteLabel { id }
        | StateCommand::RestoreLabel { id } => app.labels.get(id).map(|l| l.board),
        StateCommand::AdjustTimeEntry { id, .. } | StateCommand::DeleteTimeEntry { id } => app
            .tracking
            .entries
            .get(id)
            .and_then(|entry| task_board(entry.task)),
        // The restore rides the payload (the row is absent — that is
        // the point); entry → task → stack → board, best-effort `None`
        // when the chain is gone (the C9 D-3 honesty).
        StateCommand::RestoreTimeEntry { entry } => task_board(entry.task),
        // The wrapper's board rides the pre-image payload's own chain
        // (the resolved row is in hand before this runs; the honest
        // `None` when the chain is gone — the sibling arm's C9 D-3).
        StateCommand::RestoreTimeEntryFromJournal { from } => journal
            .entry(*from)
            .and_then(|entry| journaled_time_entry_pre_image(journal, entry.id).ok())
            .and_then(|row| task_board(row.task)),
        StateCommand::CreateHabit { board, .. } => Some(*board),
        StateCommand::ConvertSeriesToHabit { head, .. } => task_board(*head),
        StateCommand::ConvertHabitToSeries { habit, .. } => {
            app.habits.habits.get(habit).map(|h| h.board)
        }
        // CreateBoard mints its id inside the planner — the receipt
        // names it; the tag stays honest `None` here.
        StateCommand::CreateBoard { .. } => None,
        // Reflection entries are not board-scoped (device-local, no
        // board reference in the row).
        StateCommand::RecordReflection { .. }
        | StateCommand::UpdateReflection { .. }
        | StateCommand::DeleteReflection { .. } => None,
        // The bulk rosters are board-free aggregates (the Today
        // confirm) — honest `None` (the CreateBoard arm's shape).
        StateCommand::RestoreHabitsBulk { .. } | StateCommand::RestoreLabelsBulk { .. } => None,
        // Ambiguous or not board-scoped: honest `None`.
        StateCommand::SnoozeReminder { .. }
        | StateCommand::DismissReminder { .. }
        | StateCommand::RemoveReminderRule { .. }
        | StateCommand::CreateAdhocAlarm { .. }
        | StateCommand::CancelAdhocAlarm { .. }
        | StateCommand::CreateBreakRule { .. }
        | StateCommand::DeleteBreakRule { .. }
        | StateCommand::StopTracking => None,
        StateCommand::UpdateHabit { habit, .. }
        | StateCommand::MoveHabit { habit, .. }
        | StateCommand::HabitReorder { habit, .. }
        | StateCommand::StartHabitTracking { habit }
        | StateCommand::StopHabitTracking { habit }
        | StateCommand::LogHabit { habit }
        | StateCommand::SkipHabit { habit }
        | StateCommand::UnskipHabit { habit }
        | StateCommand::PauseHabit { habit, .. }
        | StateCommand::AdjustHabitLog { habit, .. }
        | StateCommand::DeleteHabit { habit }
        | StateCommand::RestoreHabit { id: habit } => app.habits.habits.get(habit).map(|h| h.board),
        StateCommand::RestoreTask { id, .. } => task_board(*id),
        StateCommand::Undo
        | StateCommand::Redo
        | StateCommand::UndoOnBoard { .. }
        | StateCommand::RedoOnBoard { .. }
        | StateCommand::UndoTo { .. }
        | StateCommand::RedoTo { .. }
        | StateCommand::RequestSync
        | StateCommand::UpdateSettings { .. } => None,
    }
}

/// The R-U8 pre-image capture: one entry per distinct entity the batch
/// upserts, with the entity's state *before* the batch (`before: None`
/// = a create). Walks the plan's `Upsert*` actions against the
/// pre-state — arm-agnostic, so cascades are captured for free (D19:
/// `RecordReminderDismissal` rows carry no pre-image; a
/// `RecordHabitLog` row does since the undo v2 widening #555 — the log
/// map lives outside the habit entity; a `DeleteReminderRule` does
/// since #900 — the capture reads the rule row from the same
/// pre-state snapshot the cascade planned from, so the image cannot
/// diverge from what the cascade actually deletes).
#[must_use]
pub fn pre_images(app: &AppState, actions: &[PersistenceAction]) -> Vec<PreImage> {
    let mut seen = BTreeSet::new();
    let mut images = Vec::new();
    for action in actions {
        let (key, image) = match action {
            // The #354 rule-verb widening (UF1): the add's upsert is a
            // create (`before: None` — ids mint once, so a journaled
            // rule upsert is always the mint; the #900 resurrect's
            // action-level upserts never journal). The `SetTaskDone`
            // recur-spawn's per-head rule copies capture the same
            // honest create shape; no inverse class reads their
            // `before: None` rows, and the resurrect walk skips
            // `before: None` — inert for every other entry.
            PersistenceAction::UpsertReminderRule(rule) => (
                JournalEntity::ReminderRule(rule.id),
                PreImage::ReminderRule {
                    id: rule.id,
                    before: app.reminders.rules.get(&rule.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::DeleteReminderRule(rule_id) => (
                JournalEntity::ReminderRule(*rule_id),
                PreImage::ReminderRule {
                    id: *rule_id,
                    before: app.reminders.rules.get(rule_id).cloned().map(Box::new),
                },
            ),
            // The hard row delete carries no upsert to walk — the
            // pre-image reads the row off the delete action itself (the
            // `DeleteReminderRule` shape; the #901 restore source).
            PersistenceAction::DeleteTimeEntry(id) => (
                JournalEntity::TimeEntry(*id),
                PreImage::TimeEntry {
                    id: *id,
                    before: app.tracking.entries.get(id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertBoard(entity) => (
                JournalEntity::Board(entity.id),
                PreImage::Board {
                    id: entity.id,
                    before: app.boards.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertStack(entity) => (
                JournalEntity::Stack(entity.id),
                PreImage::Stack {
                    id: entity.id,
                    before: app.stacks.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertTask(entity) => (
                JournalEntity::Task(entity.id),
                PreImage::Task {
                    id: entity.id,
                    before: app.tasks.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertLabel(entity) => (
                JournalEntity::Label(entity.id),
                PreImage::Label {
                    id: entity.id,
                    before: app.labels.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertHabit(entity) => (
                JournalEntity::Habit(entity.id),
                PreImage::Habit {
                    id: entity.id,
                    before: app.habits.habits.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::UpsertTimeEntry(entity) => (
                JournalEntity::TimeEntry(entity.id),
                PreImage::TimeEntry {
                    id: entity.id,
                    before: app.tracking.entries.get(&entity.id).cloned().map(Box::new),
                },
            ),
            PersistenceAction::RecordHabitLog(key, _) => (
                JournalEntity::HabitLog(*key),
                PreImage::HabitLogRow {
                    key: *key,
                    before: app.habits.log.get(key).copied(),
                },
            ),
            _ => continue,
        };
        if seen.insert(key) {
            images.push(image);
        }
    }
    images
}

/// The watermark recording (R-U7/D8): per distinct upserted entity, the
/// *post-write* clock set the payload carries. Mechanical — the per-arm
/// *checked* subset is the check site's decision.
fn watermark_of(actions: &[PersistenceAction]) -> UndoWatermark {
    let mut seen = BTreeSet::new();
    let mut map = UndoWatermark::default();
    for action in actions {
        let (key, stamps) = match action {
            PersistenceAction::UpsertTask(task) => (
                JournalEntity::Task(task.id),
                EntityStamps::Task {
                    clocks: task.clocks,
                    memberships: task.label_memberships.clone(),
                },
            ),
            PersistenceAction::UpsertStack(stack) => (
                JournalEntity::Stack(stack.id),
                EntityStamps::Stack(stack.clocks),
            ),
            PersistenceAction::UpsertBoard(board) => (
                JournalEntity::Board(board.id),
                EntityStamps::Board(board.clocks),
            ),
            PersistenceAction::UpsertLabel(label) => (
                JournalEntity::Label(label.id),
                EntityStamps::Label(label.clocks),
            ),
            PersistenceAction::UpsertHabit(habit) => (
                JournalEntity::Habit(habit.id),
                EntityStamps::Habit(habit.clocks),
            ),
            PersistenceAction::UpsertTimeEntry(entry) => (
                JournalEntity::TimeEntry(entry.id),
                EntityStamps::TimeEntry(entry.updated_at),
            ),
            PersistenceAction::RecordHabitLog(key, post) => {
                (JournalEntity::HabitLog(*key), EntityStamps::HabitLog(*post))
            }
            _ => continue,
        };
        if seen.insert(key) {
            map.0.insert(key, stamps);
        }
    }
    map
}

/// The R-U9 post-pass (engine-side, one call site serving both report
/// arms): for every `CompleteOp` in an ingestion batch, the not-yet-
/// pushed entries whose `ops` contain it — deduped, one
/// `MarkJournalPushed` per entry. The engine appends these to the batch
/// before `repo.apply`, so the flag flip is crash-consistent with the
/// completion it annotates and the applied batch stays identical to the
/// interpreted one.
#[must_use]
pub fn journal_push_marks(
    journal: &JournalState,
    actions: &[PersistenceAction],
) -> Vec<PersistenceAction> {
    let completed: Vec<OpId> = actions
        .iter()
        .filter_map(|action| match action {
            PersistenceAction::CompleteOp(op_id) => Some(*op_id),
            _ => None,
        })
        .collect();
    if completed.is_empty() {
        return Vec::new();
    }
    let mut seen = Vec::new();
    let mut marks = Vec::new();
    for entry in &journal.entries {
        if entry.pushed {
            continue;
        }
        if entry.ops.iter().any(|op| completed.contains(op)) && !seen.contains(&entry.id) {
            seen.push(entry.id);
            marks.push(PersistenceAction::MarkJournalPushed(entry.id));
        }
    }
    marks
}

/// Undoes the line head's journaled command (ADR 0019 §7): refuse-or-
/// synthesize, plan the inverse through the ordinary catalogue, apply
/// the pushed-edit rule's two op behaviours, move the head.
///
/// `scope` gates the frontier (#355): under [`UndoScope::Board`] the
/// head must exist *and* be tagged with that board (`None`-tagged
/// entries are out of scope for every board — D-3); otherwise the
/// scoped refusal is typed and nothing else runs. The head is checked,
/// never skipped past — a board-A undo never undoes (or walks past) a
/// board-B edit.
///
/// # Errors
///
/// `NothingToUndo` / `NothingToUndoForBoard` (empty or pruned line, or
/// a scoped head out of scope), `TargetNotUndoable` (R-U6),
/// `TargetNoLongerInvertible` (the R-U7 watermark), or anything the
/// inverse's own planning rejects.
#[allow(clippy::too_many_arguments)] // plan_command's own shape, threaded
pub fn plan_undo(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    scope: UndoScope,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    // 1-2. NothingToUndo → TargetNotUndoable: deterministic, cheapest
    // first (§6's check order). The scope gate sits on the frontier
    // before the undoability check: an out-of-scope head refuses as
    // "nothing on this board", not as its classification (D-1/D-5).
    let head_id = journal.head.ok_or_else(|| nothing_to_undo(scope))?;
    let entry = journal
        .entry(head_id)
        .ok_or_else(|| nothing_to_undo(scope))?;
    if let UndoScope::Board(board) = scope
        && !scope_covers(scope, entry.board)
    {
        return Err(CommandError::NothingToUndoForBoard { board });
    }
    let UndoFacts::PreImages { images, watermark } = &entry.facts else {
        return Err(CommandError::TargetNotUndoable(match entry.facts {
            UndoFacts::NotUndoable(reason) => reason,
            UndoFacts::PreImages { .. } => unreachable!("checked above"),
        }));
    };
    // 3-5. Watermark check + inverse synthesis (one per-arm walk). Most
    // classes synthesize one inverse; `DeletedStack` synthesizes the
    // composite restore batch (D-3.2: stack first, members in pre-image
    // order).
    let inverses = synthesize_undo(app, entry, images, watermark)?;
    // The occupancy guard (D-2): every synthesized `DeleteStack`
    // inverse carries it — the `CreatedStack` inverse and the
    // `RestoredStack` inverse alike (undo never silently cascades
    // entities newer than the entry it inverts). The composite
    // `DeletedStack` batch contains no `DeleteStack` inverse: its
    // atomicity is the watermark check above.
    for inverse in &inverses {
        if let StateCommand::DeleteStack { id } = inverse {
            let occupied = app
                .tasks
                .values()
                .any(|task| task.stack == *id && task.is_live());
            if occupied {
                return Err(CommandError::UndoTargetHasDependents { stack: *id });
            }
        }
    }
    // 5. Each inverse plans through the ordinary catalogue: typed
    // rejections, no-op semantics, bound/unbound splits, cascades, op
    // enqueue, nudge — all inherited. All plan against the pre-state
    // (the batch applies in emission order, the `DeleteStack` cascade's
    // own shape); the merged batch is one undo. The one exception
    // (recorded deviation, D-5): the membership re-adds of a
    // `DeletedLabel` undo (`AssignLabel` inverses riding a `RestoreLabel`
    // in the same batch) plan against a forward planning view with the
    // restored label marked live — the one-pass catalogue would refuse
    // them `LabelDeleted` against the pre-state, while the applied batch
    // runs in emission order (label first), so the applied result is
    // identical to planning against the post-restore state.
    let restored_labels: BTreeSet<crate::ids::LabelId> = inverses
        .iter()
        .filter_map(|inverse| match inverse {
            StateCommand::RestoreLabel { id } => Some(*id),
            _ => None,
        })
        .collect();
    let mut actions = Vec::new();
    for inverse in &inverses {
        let plan = match inverse {
            StateCommand::AssignLabel { label, .. } | StateCommand::UnassignLabel { label, .. }
                if restored_labels.contains(label) =>
            {
                let mut view = app.clone();
                if let Some(row) = view.labels.get_mut(label) {
                    row.deleted = false;
                }
                plan_command(&view, outbox, inverse, ids, defaults, day, now, author)?
            }
            _ => plan_command(app, outbox, inverse, ids, defaults, day, now, author)?,
        };
        actions.extend(plan.actions);
    }
    // 5b. The #900 rule resurrect (D-3): the entry's own
    // `PreImage::ReminderRule` rows ride the batch action-level — rule
    // resurrection is not a user intent (no surface verb exists), so it
    // is not a `StateCommand`. The payload is verbatim (same id, same
    // `created_at`, same stamps — `AddReminderRule` would mint a fresh
    // id and re-stamp, forging a different stored fact); a present
    // same-id row is skipped, not refused (only a prior resurrect can
    // have placed it — the head-move idempotency the redo path needs).
    // The flush trigger mirrors the delete's own enqueue (rules ride the
    // document lane). Placement before the pushed-edit rule is
    // deliberate: an unpushed entry's retraction drops these enqueues
    // (the server never knew the delete, so its document still holds the
    // rules), while a pushed entry's flush restores the document.
    for image in images {
        let PreImage::ReminderRule {
            id: rule_id,
            before: Some(rule),
        } = image
        else {
            continue;
        };
        if app.reminders.rules.contains_key(rule_id) {
            continue;
        }
        actions.push(PersistenceAction::UpsertReminderRule(rule.as_ref().clone()));
        actions.push(crate::command::enqueue(
            crate::outbox::LocalOp::UpdateAppData(rule.task),
            ids,
            now,
        ));
    }
    // 6. The pushed-edit rule (D-U2), two behaviours per entry.
    if !entry.pushed {
        // Retraction: the server never knew — drop the inverse's fresh
        // enqueues and fail the entry's own ops (all still pending by
        // the flag's honesty). Ops the inverse's own plan already
        // failed (its unbound-cancel) are not failed twice. No nudge:
        // no enqueues remain.
        let already_failed: Vec<OpId> = actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::FailOp(op_id) => Some(*op_id),
                _ => None,
            })
            .collect();
        actions.retain(|action| !matches!(action, PersistenceAction::EnqueueOp(_)));
        for op_id in &entry.ops {
            if !already_failed.contains(op_id) {
                actions.push(PersistenceAction::FailOp(*op_id));
            }
        }
    }
    // 7. The head move is always in the batch (D4: the engine's
    // empty-batch early return can never eat a line move).
    actions.push(PersistenceAction::MoveJournalHead(entry.parent));
    Ok(CommandPlan {
        actions,
        outcome: CommandOutcome::Undone { entry: entry.id },
    })
}

/// Redoes the head's most recent child (D5: greatest `(created_at, id)`
/// — stateless, deterministic, editor-equivalent in every v1-reachable
/// case). Under [`UndoScope::Board`] the D5 freedom narrows: the
/// greatest child *among the in-scope children* — siblings are
/// alternative continuations, so an out-of-scope newer sibling never
/// blocks an in-scope child (#355). Redo re-plans the *forward* intent
/// through the catalogue — a fresh edit with fresh stamps by
/// construction; no watermark check, but the catalogue's typed
/// rejections still apply.
///
/// # Errors
///
/// `NothingToRedo` / `NothingToRedoForBoard` (no head, or no child in
/// scope), `TargetNotUndoable` (a `NotUndoable` child is unreachable as
/// a redo target — defensive), or the forward command's own rejection.
#[allow(clippy::too_many_arguments)] // plan_command's own shape, threaded
pub fn plan_redo(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    scope: UndoScope,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    let head_id = journal.head.ok_or_else(|| nothing_to_redo(scope))?;
    let target = journal
        .children_of(head_id)
        .into_iter()
        .filter(|entry| scope_covers(scope, entry.board))
        .max_by_key(|entry| (entry.created_at, entry.id))
        .ok_or_else(|| nothing_to_redo(scope))?;
    plan_redo_of_entry(
        app, outbox, journal, target, ids, defaults, day, now, author,
    )
}

/// The redo of one *chosen* child (D5's freedom made explicit): replan
/// the forward intent through the catalogue, apply the D-U2 push-boundary
/// flip, move the head. Shared by [`plan_redo`] (the greatest child) and
/// [`plan_redo_to`] (the picker's chosen sibling — the one thing repeated
/// `Redo` cannot express).
#[allow(clippy::too_many_arguments)] // plan_command's own shape, threaded
fn plan_redo_of_entry(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    target: &JournalEntry,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    if let UndoFacts::NotUndoable(reason) = target.facts {
        return Err(CommandError::TargetNotUndoable(reason));
    }
    // The wrapper is journal-addressed: redo re-plans the same journal
    // read (the pre-image resolution is pure and deterministic), then
    // rides the payload verb — no fresh journal entry (the target node
    // *is* the entry; the head move below re-arms it).
    let plan = match &target.command {
        StateCommand::RestoreTimeEntryFromJournal { from } => {
            let row = journaled_time_entry_pre_image(journal, *from)?;
            plan_command(
                app,
                outbox,
                &StateCommand::RestoreTimeEntry {
                    entry: Box::new(row),
                },
                ids,
                defaults,
                day,
                now,
                author,
            )?
        }
        command => plan_command(app, outbox, command, ids, defaults, day, now, author)?,
    };
    let mut actions = plan.actions;
    // A redo that enqueues is a push-boundary crossing (D-U2: redo is a
    // new edit, *always* — its fresh ops ride the outbox to the server).
    // Flip the target's flag in the same batch, or a later undo would
    // "retract" an entry whose ops the server is about to receive.
    if actions
        .iter()
        .any(|action| matches!(action, PersistenceAction::EnqueueOp(_)))
    {
        actions.push(PersistenceAction::MarkJournalPushed(target.id));
    }
    actions.push(PersistenceAction::MoveJournalHead(Some(target.id)));
    Ok(CommandPlan {
        actions,
        outcome: CommandOutcome::Redone { entry: target.id },
    })
}

/// The inverse's coarse shape: which classification row drives the
/// synthesis and its watermark scope. Kept separate from the entry's
/// stored command so the check reads off the *classification*, not the
/// payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InverseClass {
    /// `CreateTask`/`CaptureTask`: undo deletes the created task; the
    /// checked scope is the created task's content stamps.
    CreatedTask,
    /// `UpdateTask`: inverse changeset from the pre-image diff.
    UpdateTask,
    /// `SetTaskDone`: the `done` clock only (D9's enriched row).
    SetTaskDone,
    /// `MoveTask`: the position clock.
    MoveTask,
    /// `DeleteTask`: the `deleted` clock; inverse restores the pre-image.
    DeleteTask,
    /// `CreateStack`: title + order stamps, then the occupancy guard.
    CreatedStack,
    /// `DeleteStack` (#356): the composite restore — the stack row must
    /// still stand tombstoned on the recorded stamp, every cascaded
    /// member likewise (all-or-nothing, D-3.1); the inverse is
    /// `RestoreStack` + N × `RestoreTask`.
    DeletedStack,
    /// `RestoreStack` (#356): the restored stack must still stand live
    /// on the recorded title/order stamps; the inverse is the ordinary
    /// `DeleteStack` cascade with the same occupancy guard (D-2/D-5).
    RestoredStack,
    /// `RestoreTask` (the #913 fix, D-1): the restored task must still
    /// stand live on the full content-stamp array; the inverse is the
    /// ordinary `DeleteTask` (no occupancy guard — the task's own
    /// rules/tracking closes are the honest content of the undo).
    RestoredTask,
    /// `DeleteLabel` (#901, D-5): the label row must still stand
    /// tombstoned on the recorded `deleted` stamp, and each membership
    /// the delete stamped `Removed` must still carry that stamp
    /// (covered-field: only the deleted label's own membership entry);
    /// the inverse is `RestoreLabel` + N × `AssignLabel` (label first).
    DeletedLabel,
    /// `DeleteTimeEntry` (#901, D-5): the row must still be absent (a
    /// present same-id row is divergence — ids mint once); the
    /// pre-image payload is the resurrect source; the inverse is
    /// `RestoreTimeEntry { entry }`.
    DeletedTimeEntry,
    /// `DeleteHabit` (#901, D-5): the habit row must still stand
    /// tombstoned on the recorded `deleted` stamp; the inverse is
    /// `RestoreHabit { id }`.
    DeletedHabit,
    /// `RestoreLabel` (#901, D-5): the restored label must still stand
    /// live on the title/color stamps; the inverse is `DeleteLabel`.
    RestoredLabel,
    /// `RestoreTimeEntry` (#901, D-5): the restored row must still be
    /// present on the recorded `updated_at` stamp; the inverse is
    /// `DeleteTimeEntry`.
    RestoredTimeEntry,
    /// `RestoreHabit` (#901, D-5): the restored habit must still stand
    /// live on the title/target/order stamps; the inverse is
    /// `DeleteHabit`.
    RestoredHabit,
    /// `RestoreHabitsBulk` (#1345): every restored row must still
    /// stand live on the same stamps; the inverse is N ×
    /// `DeleteHabit` — one fact, one undo, the whole roster.
    RestoredHabitsBulk,
    /// `RestoreLabelsBulk` (#1345): the labels twin — the inverse is
    /// N × `DeleteLabel`.
    RestoredLabelsBulk,
    /// `RenameStack`: the title clock.
    RenameStack,
    /// `MoveStack`: the order clock.
    MoveStack,
    /// `CreateLabel`: title + color stamps.
    CreatedLabel,
    /// `UpdateLabel`: the changed clocks (title / color).
    UpdateLabel,
    /// `AssignLabel`/`UnassignLabel`: the membership stamp.
    Membership,
    /// `LogTime`: undo deletes the created entry; the checked scope is
    /// the minted entry's `updated_at` (its one clock covers the span
    /// and the title snapshot — D27).
    CreatedTimeEntry,
    /// `AdjustTimeEntry`: inverse mask diffed pre vs current; the
    /// entry's `updated_at` covers both mask fields.
    AdjustTimeEntry,
    /// `CreateHabit`: undo tombstones the created habit; the checked
    /// scope is the four content clocks (`deleted` excluded, matching
    /// the `CreatedTask` scope — D27).
    CreatedHabit,
    /// `UpdateHabit`: inverse changeset from the pre-image diff over
    /// the three clocked content fields (`continued_from` never — D25).
    UpdateHabit,
    /// `MoveHabit`: the order clock.
    MoveHabit,
    /// `HabitReorder` (chunk A1 D1, #1198): the section's pre-move
    /// orders restore as one journaled inverse step — the synthesis
    /// batches one `MoveHabit` per touched pre-image row (each stamps
    /// its own clock). The single-pre-image `MoveHabit` shape cannot
    /// express a multi-row renumber and stays the CLI verb's class.
    HabitReorder,
    /// `TaskReorder` (B3, 2026-11-24, #1247) and the compact-move pair
    /// that delegates to it: the stack's pre-move orders restore as
    /// one journaled inverse step — the synthesis batches one
    /// `MoveTask` per touched pre-image row (each stamps its own
    /// position clock, guarded by its own watermark). The
    /// single-pre-image `MoveTask` shape cannot express a multi-row
    /// renumber and stays the CLI/cross-stack verb's class.
    TaskReorder,
    /// `AdjustHabitLog`: the touched window's post-write row compared
    /// as a whole (count AND clock).
    AdjustHabitLog,
    /// `SkipHabit`/`UnskipHabit` (HF1): the touched window's post-write
    /// row compared as a whole (mark AND clock); the inverse is the
    /// mirror verb (skip ↔ unskip), never a forged row shape.
    SkipHabit,
    /// `LogHabit` (R-7, 2026-11-21): the touched window's post-write
    /// row compared as a whole (count AND clock); the inverse restores
    /// the pre-image count via `AdjustHabitLog` — the pre row's absence
    /// restores as the honest count-0 row (the legal correction shape;
    /// absence-equivalent in every count read).
    LoggedHabit,
    /// `AddReminderRule` (#354, UF1): the inverse is the ordinary
    /// `RemoveReminderRule` over the minted rule's id (the entry's
    /// `before: None` `PreImage::ReminderRule`). Rule rows carry no
    /// clock stamps (D19), so the checked scope is the catalogue's own
    /// typed refusal — an already-removed rule refuses
    /// `UnknownReminderRule` at planning, never a silent no-op. A
    /// delivery that fired between add and undo is untouched by
    /// construction: no inverse action addresses the delivery record
    /// (the UF1 hard constraint).
    AddedReminderRule,
    /// `RemoveReminderRule`/`ClearReminderRules` (#354, UF1): no
    /// synthesized command — the inverse is the #900 action-level
    /// verbatim rule-resurrect over the entry's `before: Some`
    /// pre-images (same id, same stamps — the D-3 law; the synthesis
    /// batch is empty and `plan_undo`'s batch walk carries the
    /// upserts). Pushed/unpushed composes through the D-U2 rule
    /// unchanged: the resurrect's flush trigger survives a pushed
    /// entry and is dropped by an unpushed entry's retraction.
    RemovedReminderRules,
}

/// The §3 classification of a journaled command — the narrow extract of
/// the command arms the synthesis needs.
impl JournalEntry {
    #[allow(clippy::match_same_arms)] // the classification table reads per §3 row
    fn command_class(&self) -> Result<InverseClass, CommandError> {
        match self.command {
            StateCommand::CreateTask { .. } | StateCommand::CaptureTask { .. } => {
                Ok(InverseClass::CreatedTask)
            }
            StateCommand::RestoreTask { .. } => Ok(InverseClass::RestoredTask),
            StateCommand::UpdateTask { .. }
            // The pause inverts through the recur pre-image (issue
            // #506): undoing it restores the pre-pause rule
            // byte-for-byte via the standard UpdateTask inverse.
            | StateCommand::PauseSeries { .. } => Ok(InverseClass::UpdateTask),
            StateCommand::SetTaskDone { .. } => Ok(InverseClass::SetTaskDone),
            StateCommand::MoveTask { .. } => Ok(InverseClass::MoveTask),
            // The Today compact-move pair and the same-stack reorder
            // verb (B3, 2026-11-24, #1247): the multi-row
            // `TaskReorder` inverse — a swap-expressing move changes
            // two or more rows' orders, which the single-pre-image
            // `MoveTask` class cannot express (D-4).
            StateCommand::TaskMoveUp { .. }
            | StateCommand::TaskMoveDown { .. }
            | StateCommand::TaskReorder { .. } => Ok(InverseClass::TaskReorder),
            StateCommand::DeleteTask { .. } => Ok(InverseClass::DeleteTask),
            StateCommand::CreateStack { .. } => Ok(InverseClass::CreatedStack),
            StateCommand::DeleteStack { .. } => Ok(InverseClass::DeletedStack),
            StateCommand::RestoreStack { .. } => Ok(InverseClass::RestoredStack),
            StateCommand::RenameStack { .. } => Ok(InverseClass::RenameStack),
            StateCommand::MoveStack { .. } => Ok(InverseClass::MoveStack),
            StateCommand::CreateLabel { .. } => Ok(InverseClass::CreatedLabel),
            StateCommand::UpdateLabel { .. } => Ok(InverseClass::UpdateLabel),
            StateCommand::AssignLabel { .. } | StateCommand::UnassignLabel { .. } => {
                Ok(InverseClass::Membership)
            }
            StateCommand::LogTime { .. } => Ok(InverseClass::CreatedTimeEntry),
            StateCommand::AdjustTimeEntry { .. } => Ok(InverseClass::AdjustTimeEntry),
            StateCommand::CreateHabit { .. } => Ok(InverseClass::CreatedHabit),
            StateCommand::UpdateHabit { .. } => Ok(InverseClass::UpdateHabit),
            StateCommand::MoveHabit { .. } => Ok(InverseClass::MoveHabit),
            StateCommand::HabitReorder { .. } => Ok(InverseClass::HabitReorder),
            StateCommand::SkipHabit { .. } | StateCommand::UnskipHabit { .. } => {
                Ok(InverseClass::SkipHabit)
            }
            StateCommand::AdjustHabitLog { .. } => Ok(InverseClass::AdjustHabitLog),
            StateCommand::LogHabit { .. } => Ok(InverseClass::LoggedHabit),
            // The #901 restore-family rows (D-5's table).
            StateCommand::DeleteLabel { .. } => Ok(InverseClass::DeletedLabel),
            StateCommand::DeleteTimeEntry { .. } => Ok(InverseClass::DeletedTimeEntry),
            StateCommand::DeleteHabit { .. } => Ok(InverseClass::DeletedHabit),
            StateCommand::RestoreLabel { .. } => Ok(InverseClass::RestoredLabel),
            StateCommand::RestoreTimeEntry { .. } => Ok(InverseClass::RestoredTimeEntry),
            // The zero-input wrapper shares the sibling's class verbatim
            // (D-5): its journaled `PreImages` shape is identical (the
            // row upserted from absence), so the re-delete inverse and
            // its watermark checks are one behavior, not two.
            StateCommand::RestoreTimeEntryFromJournal { .. } => {
                Ok(InverseClass::RestoredTimeEntry)
            }
            StateCommand::RestoreHabit { .. } => Ok(InverseClass::RestoredHabit),
            // The bulk rosters (#1345): the multi-row shapes of the
            // single restore — the synthesis walks every pre-image of
            // the fact, one re-delete inverse per restored row.
            StateCommand::RestoreHabitsBulk { .. } => Ok(InverseClass::RestoredHabitsBulk),
            StateCommand::RestoreLabelsBulk { .. } => Ok(InverseClass::RestoredLabelsBulk),
            // The #354 rule-verb rows (UF1): the add's inverse is the
            // ordinary remove; a remove/clear's inverse is the #900
            // verbatim resurrect (an empty synthesis batch — the
            // action-level walk carries it).
            StateCommand::AddReminderRule { .. } => Ok(InverseClass::AddedReminderRule),
            StateCommand::RemoveReminderRule { .. }
            | StateCommand::ClearReminderRules { .. } => Ok(InverseClass::RemovedReminderRules),
            // NotUndoable heads refuse before the class is consulted;
            // every other arm is unreachable here (the wrapper only
            // journals the arms above). The engine-routed/no-op arms
            // are carried for the lockstep: one arm, one row.
            StateCommand::CreateBoard { .. }
            | StateCommand::BindBoard { .. }
            | StateCommand::RebindBoard { .. }
            | StateCommand::RenameBoard { .. }
            | StateCommand::SetBoardColor { .. }
            | StateCommand::SetBoardArchived { .. }
            | StateCommand::DeleteBoard { .. }
            | StateCommand::ResetBoard { .. }
            | StateCommand::UnbindBoard { .. }
            | StateCommand::ReconcileByTitle { .. }
            | StateCommand::SnoozeReminder { .. }
            | StateCommand::SetTaskRemindersMuted { .. }
            | StateCommand::DismissReminder { .. }
            | StateCommand::CreateAdhocAlarm { .. }
            | StateCommand::CancelAdhocAlarm { .. }
            | StateCommand::CreateBreakRule { .. }
            | StateCommand::DeleteBreakRule { .. }
            | StateCommand::StartTracking { .. }
            | StateCommand::StopTracking
            | StateCommand::StartHabitTracking { .. }
            | StateCommand::StopHabitTracking { .. }
            | StateCommand::PauseHabit { .. }
            | StateCommand::RecordReflection { .. }
            | StateCommand::UpdateReflection { .. }
            | StateCommand::DeleteReflection { .. }
            | StateCommand::ConvertSeriesToHabit { .. }
            | StateCommand::ConvertHabitToSeries { .. }
            | StateCommand::Undo
            | StateCommand::Redo
            | StateCommand::UndoOnBoard { .. }
            | StateCommand::RedoOnBoard { .. }
            | StateCommand::UndoTo { .. }
            | StateCommand::RedoTo { .. }
            | StateCommand::RequestSync
            | StateCommand::UpdateSettings { .. } => Err(CommandError::TargetNotUndoable(
                NotUndoableReason::BoardCeremony,
            )),
        }
    }
}

/// The synthesis catalogue: builds the inverse `StateCommand` for the
/// head's class, running the watermark check over exactly the class's
/// write set (D7) on the way. Most classes synthesize one inverse;
/// `DeletedStack` synthesizes the composite `RestoreStack` + N ×
/// `RestoreTask` batch (D-3.2). No wildcard — a new `StateCommand` arm
/// fails compilation in both classification matches until it gets a
/// row (the lockstep, #244's pattern).
#[allow(clippy::too_many_lines)] // one catalogue; splitting hides the table
fn synthesize_undo(
    app: &AppState,
    entry: &JournalEntry,
    images: &[PreImage],
    watermark: &UndoWatermark,
) -> Result<Vec<StateCommand>, CommandError> {
    fn task_image(images: &[PreImage]) -> Option<&crate::entities::Task> {
        images.iter().find_map(|image| match image {
            PreImage::Task {
                before: Some(task), ..
            } => Some(task.as_ref()),
            _ => None,
        })
    }
    let class = entry.command_class()?;
    let refuse = |entity: JournalEntity, field: UndoField| CommandError::TargetNoLongerInvertible {
        entity,
        field,
    };
    let created_task = |images: &[PreImage]| -> Option<crate::ids::TaskId> {
        images.iter().find_map(|image| match image {
            PreImage::Task {
                id, before: None, ..
            } => Some(*id),
            _ => None,
        })
    };
    let task_clocks =
        |app: &AppState, id: crate::ids::TaskId| -> Option<crate::entities::TaskClocks> {
            app.tasks.get(&id).map(|task| task.clocks)
        };
    // The clock-clock comparison: current field clock == recorded stamp.
    let check_task_field = |app: &AppState,
                            id: crate::ids::TaskId,
                            watermark: &UndoWatermark,
                            field: UndoField,
                            get: fn(&crate::entities::TaskClocks) -> DateTime<Utc>|
     -> Result<(), CommandError> {
        let entity = JournalEntity::Task(id);
        let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
            return Err(refuse(entity, field));
        };
        let Some(current) = task_clocks(app, id) else {
            // The row is gone (a clear-board ceremony took it): diverged.
            return Err(refuse(entity, field));
        };
        if get(&current) == get(clocks) {
            Ok(())
        } else {
            Err(refuse(entity, field))
        }
    };
    // The habit-log window key from the journal entry's
    // `PreImage::HabitLogRow` for `habit` — never recomputed from
    // "today" (the day may have rolled over since the write). Shared
    // by the SkipHabit and LoggedHabit classes.
    let habit_log_key =
        |images: &[PreImage], habit: crate::ids::HabitId| -> Option<crate::habit::HabitLogKey> {
            images.iter().find_map(|image| match image {
                PreImage::HabitLogRow { key, .. } if key.habit == habit => Some(*key),
                _ => None,
            })
        };
    match class {
        InverseClass::CreatedTask => {
            // Undo deletes the created task. The checked scope: the
            // created task's content stamps (§3 row 1) — any later edit
            // refuses; the push echo's adoption of remote/remote_seen
            // must not (they are never watermarked, D-U3).
            let Some(id) = created_task(images) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Task(id);
            let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = task_clocks(app, id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let content = [
                (clocks.title, current.title, UndoField::Title),
                (
                    clocks.description,
                    current.description,
                    UndoField::Description,
                ),
                (clocks.duedate, current.duedate, UndoField::Duedate),
                (clocks.scheduled, current.scheduled, UndoField::Scheduled),
                (clocks.done, current.done, UndoField::Done),
                (clocks.position, current.position, UndoField::Position),
                (clocks.effort, current.effort, UndoField::Effort),
                (clocks.importance, current.importance, UndoField::Importance),
                (clocks.urgency, current.urgency, UndoField::Urgency),
                (clocks.recur, current.recur, UndoField::Recur),
                (clocks.kind, current.kind, UndoField::Kind),
                (clocks.archived, current.archived, UndoField::Archived),
            ];
            for (recorded, now_clock, field) in content {
                if recorded != now_clock {
                    return Err(refuse(entity, field));
                }
            }
            Ok(vec![StateCommand::DeleteTask { id }])
        }
        InverseClass::UpdateTask => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(current) = app.tasks.get(&pre.id) else {
                return Err(refuse(JournalEntity::Task(pre.id), UndoField::Title));
            };
            let changes = task_changes_from_pre(pre, current, watermark)?;
            Ok(vec![StateCommand::UpdateTask {
                id: pre.id,
                changes,
            }])
        }
        InverseClass::SetTaskDone => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            // The enriched row (D9): the done clock *only* — the spawn's
            // successor, the closed entries, the dismissals, and the
            // suppressed recur clear are one-way facts the undo never
            // reverts, so an unrelated touch on them must not refuse.
            check_task_field(app, pre.id, watermark, UndoField::Done, |clocks| {
                clocks.done
            })?;
            Ok(vec![StateCommand::SetTaskDone {
                id: pre.id,
                done: pre.done.is_some(),
            }])
        }
        InverseClass::MoveTask => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            check_task_field(app, pre.id, watermark, UndoField::Position, |clocks| {
                clocks.position
            })?;
            Ok(vec![StateCommand::MoveTask {
                id: pre.id,
                stack: pre.stack,
                order: pre.order,
            }])
        }
        InverseClass::DeleteTask => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Task(pre.id);
            let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            let Some(current) = app.tasks.get(&pre.id) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            // The tombstone must still stand: a resurrect since would
            // make the inverse an honest no-op — refuse instead.
            if !current.deleted || current.clocks.deleted != clocks.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            Ok(vec![StateCommand::RestoreTask {
                id: pre.id,
                resurrect: Box::new(pre.clone()),
            }])
        }
        InverseClass::CreatedStack => {
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Stack {
                    id, before: None, ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Stack(id);
            let Some(EntityStamps::Stack(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.stacks.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.clocks.title != clocks.title {
                return Err(refuse(entity, UndoField::Title));
            }
            if current.clocks.order != clocks.order {
                return Err(refuse(entity, UndoField::Order));
            }
            // The occupancy guard runs in `plan_undo` (presence, not
            // clocks); the synthesis returns the plain inverse.
            Ok(vec![StateCommand::DeleteStack { id }])
        }
        InverseClass::DeletedStack => {
            // The composite restore (D-3): the entry's pre-images hold
            // the stack (before: `Some`) and every cascaded task. The
            // watermark check is all-or-nothing over the whole set
            // (D-3.1): the stack row must still exist, still be
            // tombstoned, on the recorded `deleted` stamp — and every
            // member likewise — before *any* inverse is returned, so a
            // divergence refuses the whole line with zero actions.
            let Some(stack_id) = images.iter().find_map(|image| match image {
                PreImage::Stack {
                    id,
                    before: Some(_),
                    ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let stack_entity = JournalEntity::Stack(stack_id);
            let Some(EntityStamps::Stack(clocks)) = watermark.0.get(&stack_entity) else {
                return Err(refuse(stack_entity, UndoField::Deleted));
            };
            let Some(current_stack) = app.stacks.get(&stack_id) else {
                return Err(refuse(stack_entity, UndoField::Deleted));
            };
            if !current_stack.deleted || current_stack.clocks.deleted != clocks.deleted {
                return Err(refuse(stack_entity, UndoField::Deleted));
            }
            // The members: exactly the entry's own pre-image set (D-4),
            // in pre-image order; anything individually deleted or
            // restored since diverges on its `deleted` stamp.
            let mut inverses = vec![StateCommand::RestoreStack { id: stack_id }];
            for image in images {
                let PreImage::Task {
                    id,
                    before: Some(pre),
                    ..
                } = image
                else {
                    continue;
                };
                let entity = JournalEntity::Task(*id);
                let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
                    return Err(refuse(entity, UndoField::Deleted));
                };
                let Some(current) = app.tasks.get(id) else {
                    return Err(refuse(entity, UndoField::Deleted));
                };
                if !current.deleted || current.clocks.deleted != clocks.deleted {
                    return Err(refuse(entity, UndoField::Deleted));
                }
                inverses.push(StateCommand::RestoreTask {
                    id: *id,
                    resurrect: Box::new(pre.as_ref().clone()),
                });
            }
            Ok(inverses)
        }
        InverseClass::RestoredStack => {
            // Undoing a restore re-deletes the stack through the
            // ordinary `DeleteStack` cascade (D-5); the checked scope is
            // the CreatedStack-style title+order stamps on the restored
            // row, plus the restore still standing (not re-tombstoned).
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Stack {
                    id,
                    before: Some(_),
                    ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Stack(id);
            let Some(EntityStamps::Stack(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.stacks.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            if current.clocks.title != clocks.title {
                return Err(refuse(entity, UndoField::Title));
            }
            if current.clocks.order != clocks.order {
                return Err(refuse(entity, UndoField::Order));
            }
            // The occupancy guard runs in `plan_undo` (the inverse is
            // the `DeleteStack` cascade; presence, not clocks).
            Ok(vec![StateCommand::DeleteStack { id }])
        }
        InverseClass::RestoredTask => {
            // Undoing a restore re-deletes the task through the
            // ordinary `DeleteTask` (D-1 — the #913 fix; the
            // `RestoredStack` precedent generalized; NOT a new refusal).
            // The checked scope: the restore still standing (a
            // re-tombstone since refuses, field-named), then the full
            // content-stamp array against the entry's own watermark
            // (the `CreatedTask` array verbatim); any later edit
            // refuses field-named, whole-line, zero actions. The push
            // echo's remote/remote_seen adoption never refuses (never
            // watermarked, D-U3). No occupancy guard — `DeleteTask`
            // cascades only the task's own rules/tracking closes,
            // which is the honest content of the undo.
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Task {
                    id,
                    before: Some(_),
                    ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Task(id);
            let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            let Some(current) = app.tasks.get(&id) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            if current.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            let content = [
                (clocks.title, current.clocks.title, UndoField::Title),
                (
                    clocks.description,
                    current.clocks.description,
                    UndoField::Description,
                ),
                (clocks.duedate, current.clocks.duedate, UndoField::Duedate),
                (
                    clocks.scheduled,
                    current.clocks.scheduled,
                    UndoField::Scheduled,
                ),
                (clocks.done, current.clocks.done, UndoField::Done),
                (
                    clocks.position,
                    current.clocks.position,
                    UndoField::Position,
                ),
                (clocks.effort, current.clocks.effort, UndoField::Effort),
                (
                    clocks.importance,
                    current.clocks.importance,
                    UndoField::Importance,
                ),
                (clocks.urgency, current.clocks.urgency, UndoField::Urgency),
                (clocks.recur, current.clocks.recur, UndoField::Recur),
                (clocks.kind, current.clocks.kind, UndoField::Kind),
                (
                    clocks.archived,
                    current.clocks.archived,
                    UndoField::Archived,
                ),
            ];
            for (recorded, now_clock, field) in content {
                if recorded != now_clock {
                    return Err(refuse(entity, field));
                }
            }
            Ok(vec![StateCommand::DeleteTask { id }])
        }
        InverseClass::DeletedLabel => {
            // The composite label restore (D-5): the label row must
            // still stand tombstoned on the recorded `deleted` stamp;
            // then, per member task in pre-image order whose
            // pre-image membership was `Added` for this label, that
            // task's watermark membership entry **for this label
            // only** is compared against current (covered-field
            // discipline — an unrelated membership edit must not
            // refuse). The inverse is `RestoreLabel` + N ×
            // `AssignLabel`, label first.
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Label {
                    before: Some(label),
                    ..
                } => Some(label.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let id = pre.id;
            let entity = JournalEntity::Label(id);
            let Some(EntityStamps::Label(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            let Some(current) = app.labels.get(&id) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            if !current.deleted || current.clocks.deleted != clocks.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            let mut inverses = vec![StateCommand::RestoreLabel { id }];
            for image in images {
                let PreImage::Task {
                    id: task_id,
                    before: Some(pre_task),
                    ..
                } = image
                else {
                    continue;
                };
                if !matches!(
                    pre_task.label_memberships.get(&id),
                    Some(crate::entities::MembershipState::Added { .. })
                ) {
                    continue;
                }
                let task_entity = JournalEntity::Task(*task_id);
                let Some(EntityStamps::Task { memberships, .. }) = watermark.0.get(&task_entity)
                else {
                    return Err(refuse(task_entity, UndoField::Membership));
                };
                let Some(recorded) = memberships.get(&id) else {
                    return Err(refuse(task_entity, UndoField::Membership));
                };
                let Some(current_task) = app.tasks.get(task_id) else {
                    return Err(refuse(task_entity, UndoField::Membership));
                };
                if current_task.label_memberships.get(&id) != Some(recorded) {
                    return Err(refuse(task_entity, UndoField::Membership));
                }
                inverses.push(StateCommand::AssignLabel {
                    task: *task_id,
                    label: id,
                });
            }
            Ok(inverses)
        }
        InverseClass::DeletedTimeEntry => {
            // The pre-image payload is the resurrect source (the row
            // is hard-deleted — nothing else holds the facts). The
            // guard: the row absent — a present same-id row is
            // divergence (ids are minted once); refuse, never clobber.
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::TimeEntry {
                    before: Some(entry),
                    ..
                } => Some(entry.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            if app.tracking.entries.contains_key(&pre.id) {
                return Err(refuse(JournalEntity::TimeEntry(pre.id), UndoField::Deleted));
            }
            Ok(vec![StateCommand::RestoreTimeEntry {
                entry: Box::new(pre.clone()),
            }])
        }
        InverseClass::DeletedHabit => {
            // The tombstone must still stand on the recorded stamp; the
            // inverse is the id-only `RestoreHabit` (the log is never
            // touched — D26, history outlives the tracker).
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Habit {
                    before: Some(habit),
                    ..
                } => Some(habit.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Habit(pre.id);
            let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            let Some(current) = app.habits.habits.get(&pre.id) else {
                return Err(refuse(entity, UndoField::Deleted));
            };
            if !current.deleted || current.clocks.deleted != clocks.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            Ok(vec![StateCommand::RestoreHabit { id: pre.id }])
        }
        InverseClass::RestoredLabel => {
            // Undoing a restore re-deletes the label through the
            // ordinary `DeleteLabel`; the checked scope is the
            // CreatedLabel-style title+color stamps on the restored
            // row, plus the restore still standing (the
            // `RestoredStack` scope shape).
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Label {
                    id,
                    before: Some(_),
                    ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Label(id);
            let Some(EntityStamps::Label(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.labels.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            if current.clocks.title != clocks.title {
                return Err(refuse(entity, UndoField::Title));
            }
            if current.clocks.color != clocks.color {
                return Err(refuse(entity, UndoField::Color));
            }
            Ok(vec![StateCommand::DeleteLabel { id }])
        }
        InverseClass::RestoredHabitsBulk | InverseClass::RestoredLabelsBulk => {
            // The bulk undo (#1345): ONE fact, N re-deletes — every
            // restored row of the bundle must still stand live on the
            // same stamps the single restore checks (the per-row
            // watermark scope, iterated instead of first-match).
            let mut inverses = Vec::new();
            for image in images {
                match image {
                    PreImage::Habit {
                        id,
                        before: Some(_),
                        ..
                    } => {
                        let entity = JournalEntity::Habit(*id);
                        let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                            return Err(refuse(entity, UndoField::Title));
                        };
                        let Some(current) = app.habits.habits.get(id) else {
                            return Err(refuse(entity, UndoField::Title));
                        };
                        if current.deleted {
                            return Err(refuse(entity, UndoField::Deleted));
                        }
                        let content = [
                            (clocks.title, current.clocks.title, UndoField::Title),
                            (clocks.target, current.clocks.target, UndoField::HabitTarget),
                            (clocks.order, current.clocks.order, UndoField::Order),
                        ];
                        for (recorded, now_clock, field) in content {
                            if recorded != now_clock {
                                return Err(refuse(entity, field));
                            }
                        }
                        inverses.push(StateCommand::DeleteHabit { habit: *id });
                    }
                    PreImage::Label {
                        id,
                        before: Some(_),
                        ..
                    } => {
                        let entity = JournalEntity::Label(*id);
                        let Some(EntityStamps::Label(clocks)) = watermark.0.get(&entity) else {
                            return Err(refuse(entity, UndoField::Title));
                        };
                        let Some(current) = app.labels.get(id) else {
                            return Err(refuse(entity, UndoField::Title));
                        };
                        if current.deleted {
                            return Err(refuse(entity, UndoField::Deleted));
                        }
                        if current.clocks.title != clocks.title {
                            return Err(refuse(entity, UndoField::Title));
                        }
                        if current.clocks.color != clocks.color {
                            return Err(refuse(entity, UndoField::Color));
                        }
                        inverses.push(StateCommand::DeleteLabel { id: *id });
                    }
                    _ => {}
                }
            }
            if inverses.is_empty() {
                return Err(CommandError::NothingToUndo);
            }
            Ok(inverses)
        }
        InverseClass::RestoredTimeEntry => {
            // Undoing a restore re-deletes the entry through the
            // ordinary `DeleteTimeEntry`; the checked scope: the row
            // present and its `updated_at` stamp == the entry's
            // watermark stamp (the `CreatedTimeEntry` scope). The
            // pre-image's `before` leg is by-construction `None` — the
            // row was hard-absent pre-restore, so the arm-agnostic
            // capture has no prior to carry; the class is only ever
            // produced by a `RestoreTimeEntry` head (#1187: demanding
            // `Some` here made the verb's own undo unreachable).
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::TimeEntry { id, .. } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::TimeEntry(id);
            let Some(EntityStamps::TimeEntry(recorded)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            let Some(current) = app.tracking.entries.get(&id) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            if current.updated_at != *recorded {
                return Err(refuse(entity, UndoField::UpdatedAt));
            }
            Ok(vec![StateCommand::DeleteTimeEntry { id }])
        }
        InverseClass::RestoredHabit => {
            // Undoing a restore re-tombstones the habit through the
            // ordinary `DeleteHabit`; the checked scope: still live +
            // the title/target/order stamps (the `CreatedHabit` scope
            // shape, `deleted`/`anchor_time` excluded).
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Habit {
                    id,
                    before: Some(_),
                    ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Habit(id);
            let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.habits.habits.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.deleted {
                return Err(refuse(entity, UndoField::Deleted));
            }
            let content = [
                (clocks.title, current.clocks.title, UndoField::Title),
                (clocks.target, current.clocks.target, UndoField::HabitTarget),
                (clocks.order, current.clocks.order, UndoField::Order),
            ];
            for (recorded, now_clock, field) in content {
                if recorded != now_clock {
                    return Err(refuse(entity, field));
                }
            }
            Ok(vec![StateCommand::DeleteHabit { habit: id }])
        }
        InverseClass::RenameStack => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Stack {
                    before: Some(stack),
                    ..
                } => Some(stack.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Stack(pre.id);
            let Some(EntityStamps::Stack(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.stacks.get(&pre.id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.clocks.title != clocks.title {
                return Err(refuse(entity, UndoField::Title));
            }
            Ok(vec![StateCommand::RenameStack {
                id: pre.id,
                new_title: pre.title.clone(),
            }])
        }
        InverseClass::MoveStack => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Stack {
                    before: Some(stack),
                    ..
                } => Some(stack.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Stack(pre.id);
            let Some(EntityStamps::Stack(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Order));
            };
            let Some(current) = app.stacks.get(&pre.id) else {
                return Err(refuse(entity, UndoField::Order));
            };
            if current.clocks.order != clocks.order {
                return Err(refuse(entity, UndoField::Order));
            }
            Ok(vec![StateCommand::MoveStack {
                id: pre.id,
                order: pre.order,
            }])
        }
        InverseClass::CreatedLabel => {
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Label {
                    id, before: None, ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Label(id);
            let Some(EntityStamps::Label(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.labels.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            if current.clocks.title != clocks.title {
                return Err(refuse(entity, UndoField::Title));
            }
            if current.clocks.color != clocks.color {
                return Err(refuse(entity, UndoField::Color));
            }
            Ok(vec![StateCommand::DeleteLabel { id }])
        }
        InverseClass::UpdateLabel => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Label {
                    before: Some(label),
                    ..
                } => Some(label.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(current) = app.labels.get(&pre.id) else {
                return Err(refuse(JournalEntity::Label(pre.id), UndoField::Title));
            };
            let mut changes = LabelChanges::default();
            if pre.title != current.title {
                check_label_clock(app, pre.id, watermark, UndoField::Title, |clocks| {
                    clocks.title
                })?;
                changes.title = Some(pre.title.clone());
            }
            if pre.color != current.color {
                check_label_clock(app, pre.id, watermark, UndoField::Color, |clocks| {
                    clocks.color
                })?;
                changes.color = Some(pre.color.clone());
            }
            Ok(vec![StateCommand::UpdateLabel {
                id: pre.id,
                changes,
            }])
        }
        InverseClass::Membership => {
            // Assign/Unassign share one shape: the command names the
            // pair; the inverse flips it. The checked scope is the
            // membership's OR-set stamp.
            let Some((task, label)) = (match &entry.command {
                StateCommand::AssignLabel { task, label }
                | StateCommand::UnassignLabel { task, label } => Some((*task, *label)),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Task(task);
            let Some(EntityStamps::Task { memberships, .. }) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Membership));
            };
            let Some(recorded) = memberships.get(&label) else {
                return Err(refuse(entity, UndoField::Membership));
            };
            let Some(current) = app.tasks.get(&task) else {
                return Err(refuse(entity, UndoField::Membership));
            };
            if current.label_memberships.get(&label) != Some(recorded) {
                return Err(refuse(entity, UndoField::Membership));
            }
            // The inverse flips the pair; the catalogue's own
            // Added/Removed semantics stamp the fresh state.
            let currently_added = matches!(
                current.label_memberships.get(&label),
                Some(crate::entities::MembershipState::Added { .. })
            );
            if currently_added {
                Ok(vec![StateCommand::UnassignLabel { task, label }])
            } else {
                Ok(vec![StateCommand::AssignLabel { task, label }])
            }
        }
        InverseClass::CreatedTimeEntry => {
            // Undo deletes the created entry: the minted id comes from
            // the journal entry — there is no other source for it (the
            // stage-3 trap). The checked scope: the entry's `updated_at`
            // (any later adjust refuses — the CreatedTask
            // content-stamp precedent).
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::TimeEntry {
                    id, before: None, ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::TimeEntry(id);
            let Some(EntityStamps::TimeEntry(recorded)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            let Some(current) = app.tracking.entries.get(&id) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            if current.updated_at != *recorded {
                return Err(refuse(entity, UndoField::UpdatedAt));
            }
            Ok(vec![StateCommand::DeleteTimeEntry { id }])
        }
        InverseClass::AdjustTimeEntry => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::TimeEntry {
                    before: Some(entry),
                    ..
                } => Some(entry.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::TimeEntry(pre.id);
            let Some(EntityStamps::TimeEntry(recorded)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            let Some(current) = app.tracking.entries.get(&pre.id) else {
                return Err(refuse(entity, UndoField::UpdatedAt));
            };
            // One stamp covers both mask fields (the entry's single
            // clock). A `pre.end: None` divergence is unreachable here:
            // an adjust that closed an open entry never journals
            // PreImages (D23 refuses at record time).
            if current.updated_at != *recorded {
                return Err(refuse(entity, UndoField::UpdatedAt));
            }
            Ok(vec![StateCommand::AdjustTimeEntry {
                id: pre.id,
                start: (pre.start != current.start).then_some(pre.start),
                end: if pre.end == current.end {
                    None
                } else {
                    pre.end
                },
                kind: None,
            }])
        }
        InverseClass::CreatedHabit => {
            // Undo tombstones the created habit. No occupancy guard
            // (D26): `DeleteHabit` cascades nothing — the log map
            // outlives the tombstone by construction. The checked
            // scope: the four content clocks; any later editor of any
            // refuses field-named.
            let Some(id) = images.iter().find_map(|image| match image {
                PreImage::Habit {
                    id, before: None, ..
                } => Some(*id),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Habit(id);
            let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let Some(current) = app.habits.habits.get(&id) else {
                return Err(refuse(entity, UndoField::Title));
            };
            let content = [
                (clocks.title, current.clocks.title, UndoField::Title),
                (clocks.target, current.clocks.target, UndoField::HabitTarget),
                (clocks.order, current.clocks.order, UndoField::Order),
                (
                    clocks.anchor_time,
                    current.clocks.anchor_time,
                    UndoField::HabitAnchorTime,
                ),
            ];
            for (recorded, now_clock, field) in content {
                if recorded != now_clock {
                    return Err(refuse(entity, field));
                }
            }
            Ok(vec![StateCommand::DeleteHabit { habit: id }])
        }
        InverseClass::UpdateHabit => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Habit {
                    before: Some(habit),
                    ..
                } => Some(habit.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(current) = app.habits.habits.get(&pre.id) else {
                return Err(refuse(JournalEntity::Habit(pre.id), UndoField::Title));
            };
            // Per-field diff with per-field clock checks — the
            // `task_changes_from_pre` shape over the three clocked
            // content fields. `continued_from` is outside the write set
            // and the check (D25): a seam-addressed edit never journals
            // PreImages, and the diff never touches the seam.
            let entity = JournalEntity::Habit(pre.id);
            let check = |field: UndoField,
                         get: fn(&crate::habit::HabitClocks) -> chrono::DateTime<chrono::Utc>|
             -> Result<(), CommandError> {
                let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                    return Err(CommandError::TargetNoLongerInvertible { entity, field });
                };
                if get(&current.clocks) == get(clocks) {
                    Ok(())
                } else {
                    Err(CommandError::TargetNoLongerInvertible { entity, field })
                }
            };
            let mut changes = crate::messages::HabitChanges::default();
            if pre.title != current.title {
                check(UndoField::Title, |clocks| clocks.title)?;
                changes.title = Some(pre.title.clone());
            }
            if pre.target != current.target {
                check(UndoField::HabitTarget, |clocks| clocks.target)?;
                changes.target = Some(pre.target);
            }
            if pre.anchor_time != current.anchor_time {
                // The double-option round-trip: a cleared chip
                // re-clears; a set chip restores.
                check(UndoField::HabitAnchorTime, |clocks| clocks.anchor_time)?;
                changes.anchor_time = Some(pre.anchor_time);
            }
            Ok(vec![StateCommand::UpdateHabit {
                habit: pre.id,
                changes,
            }])
        }
        InverseClass::MoveHabit => {
            let Some(pre) = images.iter().find_map(|image| match image {
                PreImage::Habit {
                    before: Some(habit),
                    ..
                } => Some(habit.as_ref()),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::Habit(pre.id);
            let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                return Err(refuse(entity, UndoField::Order));
            };
            let Some(current) = app.habits.habits.get(&pre.id) else {
                return Err(refuse(entity, UndoField::Order));
            };
            // The pre order position comes from the pre-image entity,
            // never from the command payload.
            if current.clocks.order != clocks.order {
                return Err(refuse(entity, UndoField::Order));
            }
            Ok(vec![StateCommand::MoveHabit {
                habit: pre.id,
                order: pre.order,
            }])
        }
        InverseClass::HabitReorder => {
            // The section's inverse (chunk A1 D1): one `MoveHabit` per
            // touched pre-image row, each guarded by its own order
            // watermark — the batch restores the whole pre-move
            // sequence or refuses typed, never a partial restore.
            let mut batch = Vec::new();
            for image in images {
                let Some(pre) = (match image {
                    PreImage::Habit {
                        before: Some(habit),
                        ..
                    } => Some(habit.as_ref()),
                    _ => None,
                }) else {
                    continue;
                };
                let entity = JournalEntity::Habit(pre.id);
                let Some(EntityStamps::Habit(clocks)) = watermark.0.get(&entity) else {
                    return Err(refuse(entity, UndoField::Order));
                };
                let Some(current) = app.habits.habits.get(&pre.id) else {
                    return Err(refuse(entity, UndoField::Order));
                };
                // The pre order position comes from the pre-image
                // entity, never from the command payload.
                if current.clocks.order != clocks.order {
                    return Err(refuse(entity, UndoField::Order));
                }
                batch.push(StateCommand::MoveHabit {
                    habit: pre.id,
                    order: pre.order,
                });
            }
            if batch.is_empty() {
                return Err(CommandError::NothingToUndo);
            }
            Ok(batch)
        }
        InverseClass::TaskReorder => {
            // The stack's inverse (B3, 2026-11-24, #1247 — the
            // `HabitReorder` precedent carried verbatim): one
            // `MoveTask` per touched pre-image row, each guarded by
            // its own position watermark — the batch restores the
            // whole pre-move sequence or refuses typed, never a
            // partial restore. `MoveTask` stays the inverse's payload
            // (the CLI verb and the cross-stack verb are untouched).
            let mut batch = Vec::new();
            for image in images {
                let Some(pre) = (match image {
                    PreImage::Task {
                        before: Some(task), ..
                    } => Some(task.as_ref()),
                    _ => None,
                }) else {
                    continue;
                };
                check_task_field(app, pre.id, watermark, UndoField::Position, |clocks| {
                    clocks.position
                })?;
                batch.push(StateCommand::MoveTask {
                    id: pre.id,
                    stack: pre.stack,
                    order: pre.order,
                });
            }
            if batch.is_empty() {
                return Err(CommandError::NothingToUndo);
            }
            Ok(batch)
        }
        InverseClass::SkipHabit => {
            // The mirror-verb inverse (HF1): the mark flips via
            // `UnskipHabit`/`SkipHabit`; the guard compares the touched
            // window's row to the recorded post-write row as a whole
            // (any later write to the window refuses). The window key
            // comes from the journal entry's `PreImage::HabitLogRow` —
            // never recomputed from "today" (the day may have rolled
            // over since the skip).
            let Some(habit) = (match &entry.command {
                StateCommand::SkipHabit { habit } | StateCommand::UnskipHabit { habit } => {
                    Some(*habit)
                }
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(key) = habit_log_key(images, habit) else {
                // Unreachable for a journaled skip (the mark always
                // records the row pre-image); defensive.
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::HabitLog(key);
            let Some(recorded_post) = habit_log_post(watermark, key) else {
                return Err(refuse(entity, UndoField::HabitLogCount));
            };
            if app.habits.log.get(&key) != Some(recorded_post) {
                return Err(refuse(entity, UndoField::HabitLogCount));
            }
            let inverse = match &entry.command {
                StateCommand::SkipHabit { .. } => StateCommand::UnskipHabit { habit },
                _ => StateCommand::SkipHabit { habit },
            };
            Ok(vec![inverse])
        }
        InverseClass::AdjustHabitLog => {
            // The key comes from the stored command payload; the pre
            // count from the journal entry's `PreImage::HabitLogRow` —
            // never from the change payload (the pre-image-fidelity
            // hazard). The guard compares the post-write row as a
            // whole: any later write to the window (another adjust; a
            // `LogHabit` landing there) moved the clock and refuses.
            let Some((habit, window)) = (match &entry.command {
                StateCommand::AdjustHabitLog { habit, window, .. } => Some((*habit, *window)),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let key = crate::habit::HabitLogKey { habit, window };
            let Some(recorded_pre) = images.iter().find_map(|image| match image {
                PreImage::HabitLogRow {
                    before: Some(row), ..
                } => Some(*row),
                _ => None,
            }) else {
                // Unreachable for a journaled-undoable adjust (D24 —
                // absent windows refuse at record time); defensive.
                return Err(CommandError::NothingToUndo);
            };
            let entity = JournalEntity::HabitLog(key);
            let Some(recorded_post) = habit_log_post(watermark, key) else {
                return Err(refuse(entity, UndoField::HabitLogCount));
            };
            if app.habits.log.get(&key) != Some(recorded_post) {
                return Err(refuse(entity, UndoField::HabitLogCount));
            }
            Ok(vec![StateCommand::AdjustHabitLog {
                habit,
                window,
                count: recorded_pre.count,
            }])
        }
        InverseClass::LoggedHabit => {
            // The R-7 inverse (2026-11-21): the habit and window come
            // from the journal entry's `PreImage::HabitLogRow` — never
            // recomputed from "today" (the day may have rolled over
            // since the log) — and the pre count from the same image
            // (`before: None` = the pre-log absent window; the inverse
            // is the count-0 correction, the legal row shape,
            // absence-equivalent in every count read). The guard
            // compares the touched window's row to the recorded
            // post-write row as a whole (any later write to the window
            // — another log, an adjust — moved the clock and refuses).
            // The inverse itself is the existing audited
            // `AdjustHabitLog`: its #986 bump mint re-stamps the
            // restored row deterministically (journal replay re-mints
            // identically; the B1 rule unchanged), and its ordinary
            // catalogue planning answers the D-H12 kind gate typed if
            // the habit's kind moved since the log.
            let Some(habit) = (match &entry.command {
                StateCommand::LogHabit { habit } => Some(*habit),
                _ => None,
            }) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(key) = habit_log_key(images, habit) else {
                // Unreachable for a journaled log (every LogHabit
                // records the row pre-image); defensive.
                return Err(CommandError::NothingToUndo);
            };
            let recorded_pre = images.iter().find_map(|image| match image {
                PreImage::HabitLogRow {
                    key: image_key,
                    before,
                } if *image_key == key => before.as_ref().map(|row| row.count),
                _ => None,
            });
            let entity = JournalEntity::HabitLog(key);
            let Some(recorded_post) = habit_log_post(watermark, key) else {
                return Err(refuse(entity, UndoField::HabitLogCount));
            };
            // Count-only guard (not the AdjustHabitLog class's
            // whole-row compare): this class's own inverse re-stamps
            // the row (the #986 bump mint), so the clock legitimately
            // moves between consecutive pops — a whole-row guard would
            // deadlock the second undo of a burst (US-3/AC-2's
            // per-increment law). The count is the class's full
            // semantic payload (one log = exactly one increment), and
            // the bump's monotone mint keeps the LWW ordering intact.
            if app.habits.log.get(&key).map(|row| row.count) != Some(recorded_post.count) {
                return Err(refuse(entity, UndoField::HabitLogCount));
            }
            Ok(vec![StateCommand::AdjustHabitLog {
                habit,
                window: key.window,
                count: recorded_pre.unwrap_or(0),
            }])
        }
        InverseClass::AddedReminderRule => {
            // The #354 inverse (UF1): the ordinary `RemoveReminderRule`
            // over the minted rule's id, read off the entry's
            // `before: None` rule pre-image (the mint is the only
            // journaled upsert shape this class sees). Rule rows carry
            // no stamps (D19) — the checked scope is the catalogue's
            // own typed refusal: if the rule is already gone, the
            // remove plans `UnknownReminderRule`; no silent no-op. The
            // delivery proofs the rule may have fired are untouched by
            // construction — no action in the batch addresses the
            // delivery record (the UF1 hard constraint, trap (a)).
            let Some(rule) = images.iter().find_map(|image| match image {
                PreImage::ReminderRule { id, before: None } => Some(*id),
                _ => None,
            }) else {
                // Unreachable for a journaled add (the capture records
                // the mint); defensive.
                return Err(CommandError::NothingToUndo);
            };
            Ok(vec![StateCommand::RemoveReminderRule { rule }])
        }
        InverseClass::RemovedReminderRules => {
            // The #354 inverse (UF1): deliberately empty — the remove /
            // clear inverts through the #900 action-level verbatim
            // rule-resurrect in `plan_undo`'s batch walk (same id, same
            // `created_at`, same stamps — the D-3 law; a synthesized
            // `AddReminderRule` would mint fresh identities and forge a
            // different stored fact). Rule rows carry no stamps (D19),
            // so there is nothing to watermark-check here; the
            // resurrect's skip-not-refuse guard is the divergence valve
            // a prior resurrect already placed. Pushed/unpushed (D-U2)
            // composes after the walk: a pushed entry keeps the
            // resurrect's document-lane flush; an unpushed entry's
            // retraction drops it (the server never saw the removal).
            Ok(Vec::new())
        }
    }
}

/// The label-clock comparison (the label entity's row must still exist
/// and carry the recorded post-write stamp on the named field).
fn check_label_clock(
    app: &AppState,
    id: crate::ids::LabelId,
    watermark: &UndoWatermark,
    field: UndoField,
    get: fn(&crate::entities::LabelClocks) -> DateTime<Utc>,
) -> Result<(), CommandError> {
    let entity = JournalEntity::Label(id);
    let Some(EntityStamps::Label(clocks)) = watermark.0.get(&entity) else {
        return Err(CommandError::TargetNoLongerInvertible { entity, field });
    };
    let Some(current) = app.labels.get(&id) else {
        return Err(CommandError::TargetNoLongerInvertible { entity, field });
    };
    if get(&current.clocks) == get(clocks) {
        Ok(())
    } else {
        Err(CommandError::TargetNoLongerInvertible { entity, field })
    }
}

/// Builds the inverse `TaskChanges` by diffing the pre-image against the
/// current entity over the recordable fields — sound because the
/// watermark check proves each divergent field untouched since the
/// command (the check runs per divergent field, right here).
fn task_changes_from_pre(
    pre: &crate::entities::Task,
    current: &crate::entities::Task,
    watermark: &UndoWatermark,
) -> Result<TaskChanges, CommandError> {
    let entity = JournalEntity::Task(pre.id);
    let check = |field: UndoField,
                 get: fn(&crate::entities::TaskClocks) -> DateTime<Utc>|
     -> Result<(), CommandError> {
        let Some(EntityStamps::Task { clocks, .. }) = watermark.0.get(&entity) else {
            return Err(CommandError::TargetNoLongerInvertible { entity, field });
        };
        if get(&current.clocks) == get(clocks) {
            Ok(())
        } else {
            Err(CommandError::TargetNoLongerInvertible { entity, field })
        }
    };
    let mut changes = TaskChanges::default();
    if pre.title != current.title {
        check(UndoField::Title, |clocks| clocks.title)?;
        changes.title = Some(pre.title.clone());
    }
    if pre.description != current.description {
        check(UndoField::Description, |clocks| clocks.description)?;
        changes.description = Some(pre.description.clone());
    }
    if pre.duedate != current.duedate {
        check(UndoField::Duedate, |clocks| clocks.duedate)?;
        changes.duedate = Some(pre.duedate);
    }
    if pre.scheduled != current.scheduled {
        check(UndoField::Scheduled, |clocks| clocks.scheduled)?;
        changes.scheduled = Some(pre.scheduled);
    }
    if pre.effort != current.effort {
        check(UndoField::Effort, |clocks| clocks.effort)?;
        changes.effort = Some(pre.effort);
    }
    if pre.importance != current.importance {
        check(UndoField::Importance, |clocks| clocks.importance)?;
        changes.importance = Some(pre.importance);
    }
    if pre.urgency != current.urgency {
        check(UndoField::Urgency, |clocks| clocks.urgency)?;
        changes.urgency = Some(pre.urgency);
    }
    if pre.recur != current.recur {
        check(UndoField::Recur, |clocks| clocks.recur)?;
        changes.recur = Some(pre.recur.clone());
    }
    if pre.archived != current.archived {
        check(UndoField::Archived, |clocks| clocks.archived)?;
        changes.archived = Some(pre.archived);
    }
    if pre.kind != current.kind {
        check(UndoField::Kind, |clocks| clocks.kind)?;
        changes.kind = Some(Some(pre.kind));
    }
    Ok(changes)
}

#[cfg(test)]
mod tests;

// ---- the UF2 walk planners (#353) --------------------------------------
//
// `UndoTo`/`RedoTo` wrap the existing planners in a bounded walk: one
// command, N internal steps, each step's batch planned by the same
// `plan_undo`/`plan_redo` machinery and *simulated forward* between
// steps so every subsequent step plans against the state its
// predecessor produced. The simulation is the engine's own executable
// contract — `apply_actions` over a `PersistedState` mirror, the
// `Harness::exec` precedent — so the interpreted intermediate states
// are identical to what the engine will apply from the merged batch.

/// The walk's forward mirror: the persisted shape `apply_actions`
/// advances, seeded from the planner's three inputs. Pure — the engine
/// applies the merged batch to the real store; this mirror only lets
/// step *k+1* plan against step *k*'s result.
struct WalkMirror {
    disk: crate::persistence::PersistedState,
}

impl WalkMirror {
    fn seed(app: &AppState, outbox: &[PendingOp], journal: &JournalState) -> Self {
        // Validators and sync-status rows are not carried: undo/redo
        // plans never read or emit them (board ceremonies — the only
        // validators' writers — are `NotUndoable`, so no walk step can
        // reach them; the derived `sync` projection re-derives from the
        // outbox on every view).
        let mut disk = crate::persistence::PersistedState {
            boards: app.boards.clone(),
            stacks: app.stacks.clone(),
            tasks: app.tasks.clone(),
            labels: app.labels.clone(),
            reminders: app.reminders.clone(),
            tracking: app.tracking.clone(),
            habits: app.habits.clone(),
            reflection: app.reflection.clone(),
            roaming: app.roaming.clone(),
            ..crate::persistence::PersistedState::default()
        };
        disk.outbox = outbox.to_vec();
        disk.journal = journal.clone();
        Self { disk }
    }

    fn app(&self) -> AppState {
        let mut persisted = self.disk.clone();
        crate::state::derive_sync_states(
            &persisted.boards,
            &persisted.stacks,
            &persisted.tasks,
            &persisted.labels,
            &persisted.outbox,
            &mut persisted.sync,
        );
        AppState::from(persisted)
    }

    fn advance(&mut self, actions: &[PersistenceAction]) {
        crate::persistence::apply_actions(&mut self.disk, actions);
    }

    fn outbox(&self) -> Vec<PendingOp> {
        self.disk.outbox.clone()
    }

    fn journal(&self) -> &JournalState {
        &self.disk.journal
    }
}

/// Walk budget: the line's own length bounds every walk (each step
/// consumes one head or takes one child), so the loop is total — the
/// guard is a belt-and-braces against a planner that stops moving the
/// head, not a semantic cap.
fn walk_budget(journal: &JournalState) -> usize {
    journal.entries.len() + 1
}

/// Jumps the undo line back to `target` (UF2, #353): the ancestor verb.
/// `target` must be an ancestor of the current head through the
/// journal's `parent` pointers — no journal migration, the tree shape
/// is consumed, never rewritten (the UF2 trap a). The walk repeats
/// [`plan_undo`] (global scope) until the head lands exactly on
/// `target`; every intermediate step's batch (inverse upserts, the
/// D-U2 retraction/flush op behaviours, the head move) is merged in
/// order, and the receipt names the walk's final undone entry. A
/// target already at the head is the accepted no-op (zero actions, the
/// engine's empty-batch reply).
///
/// # Errors
///
/// [`CommandError::TargetNotInLine`] when the id is not an ancestor of
/// the head (or the line is empty); every planner refusal mid-walk
/// surfaces verbatim and leaves the head unmoved (an error plan emits
/// no actions — the merged batch never half-applies).
#[allow(clippy::too_many_arguments)] // plan_command's own shape, threaded
pub fn plan_undo_to(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    target: crate::ids::JournalId,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    // The line-membership question is the parent walk: the head must
    // reach `target` through `parent` pointers (eviction-pruned ids
    // refuse here too — `entry` returns `None` and the chain breaks).
    let mut on_line = false;
    let mut cursor = journal.head;
    while let Some(id) = cursor {
        if id == target {
            on_line = true;
            break;
        }
        cursor = journal.entry(id).and_then(|entry| entry.parent);
    }
    if !on_line {
        return Err(CommandError::TargetNotInLine);
    }
    let mut mirror = WalkMirror::seed(app, outbox, journal);
    let mut merged = Vec::new();
    let mut final_entry = target;
    let budget = walk_budget(journal);
    for _ in 0..budget {
        let Some(head) = mirror.journal().head else {
            return Err(CommandError::TargetNotInLine);
        };
        if head == target {
            break;
        }
        let plan = plan_undo(
            &mirror.app(),
            &mirror.outbox(),
            mirror.journal(),
            UndoScope::Global,
            ids,
            defaults,
            day,
            now,
            author,
        )?;
        if let CommandOutcome::Undone { entry } = plan.outcome {
            final_entry = entry;
        }
        mirror.advance(&plan.actions);
        merged.extend(plan.actions);
    }
    Ok(CommandPlan {
        actions: merged,
        outcome: CommandOutcome::Undone { entry: final_entry },
    })
}

/// Redoes down a chosen branch to `target` (UF2, #353): the sibling
/// verb. Repeated `Redo` (D5 greatest-child) cannot express this
/// choice, which is the verb's entire reason to exist. The walk's
/// three legs, per the plan's §7 law ("undo to the target's fork then
/// redo down the chosen sibling to the tip"): undo back to the fork
/// (the deepest common ancestor of head and target, via the same
/// `plan_undo` machinery), replan the forward intents along the
/// fork→target path ([`plan_redo_of_entry`] per step), then keep the
/// greatest-child walk going ([`plan_redo`]) to the branch's tip — the
/// head lands on the tip and the receipt names it (the plan's S2 law:
/// the presented state reflects the branch's head).
///
/// # Errors
///
/// [`CommandError::TargetNotInLine`] when the target shares no fork
/// with the head (a stale picker row after an intervening edit, or a
/// foreign/evicted id); planner refusals mid-walk surface verbatim,
/// head unmoved from the last committed step — and since an error plan
/// emits no actions, from the walk's start.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)] // plan_command's shape; three documented walk legs in one plan
pub fn plan_redo_to(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    target: crate::ids::JournalId,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
    author: Option<crate::ids::AuthorId>,
) -> Result<CommandPlan, CommandError> {
    // The reachability question (the plan's §7 law: "undo to the
    // target's fork then redo down the chosen sibling to the tip"):
    // walk both ancestor chains to their deepest common node — the
    // fork. No common node ⇒ the target is not on the head's tree ⇒
    // `TargetNotInLine` (the stale-picker-row case).
    let chain = |journal: &JournalState, mut id: Option<crate::ids::JournalId>| {
        let mut chain = Vec::new();
        while let Some(entry_id) = id {
            chain.push(entry_id);
            id = journal.entry(entry_id).and_then(|entry| entry.parent);
        }
        chain
    };
    let head = journal.head.ok_or(CommandError::TargetNotInLine)?;
    let head_chain = chain(journal, Some(head));
    let target_chain = chain(journal, Some(target));
    if target_chain.is_empty() {
        return Err(CommandError::TargetNotInLine);
    }
    let fork = target_chain
        .iter()
        .find(|id| head_chain.contains(id))
        .ok_or(CommandError::TargetNotInLine)?;
    let mut mirror = WalkMirror::seed(app, outbox, journal);
    let mut merged = Vec::new();
    let mut final_entry = head;
    let budget = walk_budget(journal);
    // Phase 0: the undo-to-fork leg. While the head sits above the
    // fork, the walk repeats `plan_undo` until the head lands on it —
    // the same per-entry D-U2 behaviour `UndoTo` applies.
    let mut steps = 0;
    while mirror.journal().head != Some(*fork) {
        steps += 1;
        if steps > budget {
            return Err(CommandError::TargetNotInLine);
        }
        let plan = plan_undo(
            &mirror.app(),
            &mirror.outbox(),
            mirror.journal(),
            UndoScope::Global,
            ids,
            defaults,
            day,
            now,
            author,
        )?;
        if let CommandOutcome::Undone { entry } = plan.outcome {
            final_entry = entry;
        }
        mirror.advance(&plan.actions);
        merged.extend(plan.actions);
    }
    // Phase 1: the redo-down-the-chosen-sibling leg — the child path
    // fork→target replanned forward, one [`plan_redo_of_entry`] per
    // step (each against the previous step's mirror).
    let child_path: Vec<crate::ids::JournalId>;
    {
        // The fork→target path: walk the target's parent pointers back
        // to the fork, then reverse.
        let mut path = Vec::new();
        let mut cursor = Some(target);
        while let Some(id) = cursor {
            if id == *fork {
                break;
            }
            path.push(id);
            cursor = journal.entry(id).and_then(|entry| entry.parent);
        }
        if cursor != Some(*fork) {
            return Err(CommandError::TargetNotInLine);
        }
        path.reverse();
        child_path = path;
    }
    for step_id in &child_path {
        let entry = mirror
            .journal()
            .entry(*step_id)
            .ok_or(CommandError::TargetNotInLine)?;
        let plan = plan_redo_of_entry(
            &mirror.app(),
            &mirror.outbox(),
            mirror.journal(),
            entry,
            ids,
            defaults,
            day,
            now,
            author,
        )?;
        if let CommandOutcome::Redone { entry } = plan.outcome {
            final_entry = entry;
        }
        mirror.advance(&plan.actions);
        merged.extend(plan.actions);
    }
    // Phase 2: the greatest-child walk to the branch's tip (S2: the
    // head lands on the tip — the branch's own head).
    for _ in 0..budget {
        let Some(tip) = mirror.journal().head else {
            break;
        };
        if mirror.journal().children_of(tip).is_empty() {
            break;
        }
        let plan = plan_redo(
            &mirror.app(),
            &mirror.outbox(),
            mirror.journal(),
            UndoScope::Global,
            ids,
            defaults,
            day,
            now,
            author,
        )?;
        if let CommandOutcome::Redone { entry } = plan.outcome {
            final_entry = entry;
        }
        mirror.advance(&plan.actions);
        merged.extend(plan.actions);
    }
    Ok(CommandPlan {
        actions: merged,
        outcome: CommandOutcome::Redone { entry: final_entry },
    })
}
