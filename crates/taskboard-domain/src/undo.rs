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
) -> Result<CommandPlan, CommandError> {
    match command {
        StateCommand::Undo => plan_undo(app, outbox, journal, ids, defaults, day, now),
        StateCommand::Redo => plan_redo(app, outbox, journal, ids, defaults, day, now),
        command => {
            let plan = plan_command(app, outbox, command, ids, defaults, day, now)?;
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
        | StateCommand::DeleteTask { .. }
        | StateCommand::CreateStack { .. }
        | StateCommand::RenameStack { .. }
        | StateCommand::MoveStack { .. }
        | StateCommand::CreateLabel { .. }
        | StateCommand::UpdateLabel { .. }
        | StateCommand::AssignLabel { .. }
        | StateCommand::UnassignLabel { .. }
        | StateCommand::RestoreTask { .. } => UndoFacts::PreImages {
            images: pre_images(app, actions),
            watermark: watermark_of(actions),
        },
        // ---- the typed families (§3 rows; the lockstep's other half)
        StateCommand::DeleteStack { .. } | StateCommand::DeleteLabel { .. } => {
            UndoFacts::NotUndoable(NotUndoableReason::RestoreClass)
        }
        StateCommand::CreateBoard { .. }
        | StateCommand::BindBoard { .. }
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
        | StateCommand::AddReminderRule { .. }
        | StateCommand::RemoveReminderRule { .. }
        | StateCommand::ClearReminderRules { .. }
        | StateCommand::CreateAdhocAlarm { .. }
        | StateCommand::CancelAdhocAlarm { .. } => {
            UndoFacts::NotUndoable(NotUndoableReason::ReminderSurface)
        }
        // ---- the undo v2 tracking/habit widening (#555, §3 rows
        // 35′–42′; three record-time dynamic rules per §6/§7) ----------
        StateCommand::LogTime { .. }
        | StateCommand::CreateHabit { .. }
        | StateCommand::MoveHabit { .. } => UndoFacts::PreImages {
            images: pre_images(app, actions),
            watermark: watermark_of(actions),
        },
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
                UndoFacts::PreImages {
                    images: pre_images(app, actions),
                    watermark: watermark_of(actions),
                }
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
                UndoFacts::PreImages {
                    images: pre_images(app, actions),
                    watermark: watermark_of(actions),
                }
            }
        }
        StateCommand::AdjustHabitLog { habit, window, .. } => {
            // D24/§7.2: an adjust onto a window with no pre-existing row
            // is lossy to invert — the `count: 0` inverse would forge an
            // explicit count-0 row where the pre-state had none (the
            // exact stored-fact lossiness that keeps `LogHabit`
            // excluded). Pre row present (any count, 0 included)
            // inverts exactly.
            let key = crate::habit::HabitLogKey {
                habit: *habit,
                window: *window,
            };
            if app.habits.log.contains_key(&key) {
                UndoFacts::PreImages {
                    images: pre_images(app, actions),
                    watermark: watermark_of(actions),
                }
            } else {
                UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
            }
        }
        // ---- the exclusions stay, reasons restated (§4) ----------------
        StateCommand::StartTracking { .. } | StateCommand::StopTracking => {
            // Structurally non-invertible (ADR 0017): the inverse would
            // have to re-open closed entries — no command expresses
            // that. Also dismissal side effects (D-T5).
            UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
        }
        StateCommand::DeleteTimeEntry { .. } => {
            // No un-delete verb exists (`LogTime` mints fresh ids);
            // restore-class shape (#356 family).
            UndoFacts::NotUndoable(NotUndoableReason::TrackingSurface)
        }
        StateCommand::LogHabit { .. } => {
            // Lossy to invert: a pre-log absent window would become an
            // explicit count-0 row — a different stored fact; no
            // log-row delete action exists.
            UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
        }
        StateCommand::DeleteHabit { .. } => {
            // No `RestoreHabit` verb; tombstone-only delete.
            UndoFacts::NotUndoable(NotUndoableReason::HabitSurface)
        }
        // Never journaled (the wrapper routes Undo/Redo; RequestSync
        // plans an empty batch and never reaches this point — both arms
        // stay for totality).
        StateCommand::Undo | StateCommand::Redo | StateCommand::RequestSync => {
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
        board: touched_board(app, command),
        created_at: now,
    }
}

/// The best-effort touched-board tag (D-U4's per-board note deferred to
/// v2): task → stack → board; stack/label → their board; `None` when
/// not board-scoped or the lookup chain is gone (ambiguity is honest).
#[allow(clippy::match_same_arms)] // the tag table reads per §3 row; merging hides it
fn touched_board(app: &AppState, command: &StateCommand) -> Option<crate::ids::BoardId> {
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
        | StateCommand::DeleteTask { id }
        | StateCommand::SetTaskRemindersMuted { task: id, .. }
        | StateCommand::AddReminderRule { task: id, .. }
        | StateCommand::ClearReminderRules { task: id }
        | StateCommand::StartTracking { task: id }
        | StateCommand::LogTime { task: id, .. }
        | StateCommand::AssignLabel { task: id, .. }
        | StateCommand::UnassignLabel { task: id, .. } => task_board(*id),
        StateCommand::CreateStack { board, .. }
        | StateCommand::CreateLabel { board, .. }
        | StateCommand::BindBoard { board, .. }
        | StateCommand::RenameBoard { board, .. }
        | StateCommand::SetBoardColor { board, .. }
        | StateCommand::SetBoardArchived { board, .. }
        | StateCommand::DeleteBoard { board }
        | StateCommand::ResetBoard { board }
        | StateCommand::UnbindBoard { board }
        | StateCommand::ReconcileByTitle { board } => Some(*board),
        StateCommand::RenameStack { id, .. }
        | StateCommand::DeleteStack { id }
        | StateCommand::MoveStack { id, .. } => app.stacks.get(id).map(|s| s.board),
        StateCommand::UpdateLabel { id, .. } | StateCommand::DeleteLabel { id } => {
            app.labels.get(id).map(|l| l.board)
        }
        StateCommand::AdjustTimeEntry { id, .. } | StateCommand::DeleteTimeEntry { id } => app
            .tracking
            .entries
            .get(id)
            .and_then(|entry| task_board(entry.task)),
        StateCommand::CreateHabit { board, .. } => Some(*board),
        // CreateBoard mints its id inside the planner — the receipt
        // names it; the tag stays honest `None` here.
        StateCommand::CreateBoard { .. } => None,
        // Ambiguous or not board-scoped: honest `None`.
        StateCommand::SnoozeReminder { .. }
        | StateCommand::DismissReminder { .. }
        | StateCommand::RemoveReminderRule { .. }
        | StateCommand::CreateAdhocAlarm { .. }
        | StateCommand::CancelAdhocAlarm { .. }
        | StateCommand::StopTracking => None,
        StateCommand::UpdateHabit { habit, .. }
        | StateCommand::MoveHabit { habit, .. }
        | StateCommand::LogHabit { habit }
        | StateCommand::AdjustHabitLog { habit, .. }
        | StateCommand::DeleteHabit { habit } => app.habits.habits.get(habit).map(|h| h.board),
        StateCommand::RestoreTask { id, .. } => task_board(*id),
        StateCommand::Undo | StateCommand::Redo | StateCommand::RequestSync => None,
    }
}

/// The R-U8 pre-image capture: one entry per distinct entity the batch
/// upserts, with the entity's state *before* the batch (`before: None`
/// = a create). Walks the plan's `Upsert*` actions against the
/// pre-state — arm-agnostic, so cascades are captured for free (D19:
/// `RecordReminderDismissal` rows and deletes carry no pre-image; a
/// `RecordHabitLog` row does since the undo v2 widening #555 — the log
/// map lives outside the habit entity).
#[must_use]
pub fn pre_images(app: &AppState, actions: &[PersistenceAction]) -> Vec<PreImage> {
    let mut seen = BTreeSet::new();
    let mut images = Vec::new();
    for action in actions {
        let (key, image) = match action {
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
/// # Errors
///
/// `NothingToUndo` (empty or pruned line), `TargetNotUndoable` (R-U6),
/// `TargetNoLongerInvertible` (the R-U7 watermark), or anything the
/// inverse's own planning rejects.
pub fn plan_undo(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
) -> Result<CommandPlan, CommandError> {
    // 1-2. NothingToUndo → TargetNotUndoable: deterministic, cheapest
    // first (§6's check order).
    let head_id = journal.head.ok_or(CommandError::NothingToUndo)?;
    let entry = journal.entry(head_id).ok_or(CommandError::NothingToUndo)?;
    let UndoFacts::PreImages { images, watermark } = &entry.facts else {
        return Err(CommandError::TargetNotUndoable(match entry.facts {
            UndoFacts::NotUndoable(reason) => reason,
            UndoFacts::PreImages { .. } => unreachable!("checked above"),
        }));
    };
    // 3-5. Watermark check + inverse synthesis (one per-arm walk) and
    // the CreateStack occupancy guard.
    let inverse = synthesize_undo(app, entry, images, watermark)?;
    if let StateCommand::DeleteStack { id } = &inverse {
        let occupied = app
            .tasks
            .values()
            .any(|task| task.stack == *id && task.is_live());
        if occupied {
            return Err(CommandError::UndoTargetHasDependents { stack: *id });
        }
    }
    // 5. The inverse plans through the ordinary catalogue: typed
    // rejections, no-op semantics, bound/unbound splits, cascades, op
    // enqueue, nudge — all inherited.
    let plan = plan_command(app, outbox, &inverse, ids, defaults, day, now)?;
    // 6. The pushed-edit rule (D-U2), two behaviours per entry.
    let mut actions = plan.actions;
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
/// case). Redo re-plans the *forward* intent through the catalogue — a
/// fresh edit with fresh stamps by construction; no watermark check,
/// but the catalogue's typed rejections still apply.
///
/// # Errors
///
/// `NothingToRedo` (no head or no child), `TargetNotUndoable` (a
/// `NotUndoable` child is unreachable as a redo target — defensive), or
/// the forward command's own rejection.
pub fn plan_redo(
    app: &AppState,
    outbox: &[PendingOp],
    journal: &JournalState,
    ids: &dyn IdGenerator,
    defaults: &ReminderDefaults,
    day: DayCalendar,
    now: DateTime<Utc>,
) -> Result<CommandPlan, CommandError> {
    let head_id = journal.head.ok_or(CommandError::NothingToRedo)?;
    let target = journal
        .children_of(head_id)
        .into_iter()
        .max_by_key(|entry| (entry.created_at, entry.id))
        .ok_or(CommandError::NothingToRedo)?;
    if let UndoFacts::NotUndoable(reason) = target.facts {
        return Err(CommandError::TargetNotUndoable(reason));
    }
    let plan = plan_command(app, outbox, &target.command, ids, defaults, day, now)?;
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
    /// `CreateTask`/`CaptureTask`/`RestoreTask`: undo deletes the created
    /// task; the checked scope is the created task's content stamps.
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
}

/// The §3 classification of a journaled command — the narrow extract of
/// the command arms the synthesis needs.
impl JournalEntry {
    #[allow(clippy::match_same_arms)] // the classification table reads per §3 row
    fn command_class(&self) -> Result<InverseClass, CommandError> {
        match self.command {
            StateCommand::CreateTask { .. }
            | StateCommand::CaptureTask { .. }
            | StateCommand::RestoreTask { .. } => Ok(InverseClass::CreatedTask),
            StateCommand::UpdateTask { .. } => Ok(InverseClass::UpdateTask),
            StateCommand::SetTaskDone { .. } => Ok(InverseClass::SetTaskDone),
            StateCommand::MoveTask { .. } => Ok(InverseClass::MoveTask),
            StateCommand::DeleteTask { .. } => Ok(InverseClass::DeleteTask),
            StateCommand::CreateStack { .. } => Ok(InverseClass::CreatedStack),
            StateCommand::RenameStack { .. } => Ok(InverseClass::RenameStack),
            StateCommand::MoveStack { .. } => Ok(InverseClass::MoveStack),
            StateCommand::CreateLabel { .. } => Ok(InverseClass::CreatedLabel),
            StateCommand::UpdateLabel { .. } => Ok(InverseClass::UpdateLabel),
            StateCommand::AssignLabel { .. } | StateCommand::UnassignLabel { .. } => {
                Ok(InverseClass::Membership)
            }
            // NotUndoable heads refuse before the class is consulted;
            // every other arm is unreachable here (the wrapper only
            // journals the arms above). The engine-routed/no-op arms
            // are carried for the lockstep: one arm, one row.
            StateCommand::DeleteStack { .. }
            | StateCommand::DeleteLabel { .. }
            | StateCommand::CreateBoard { .. }
            | StateCommand::BindBoard { .. }
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
            | StateCommand::AddReminderRule { .. }
            | StateCommand::RemoveReminderRule { .. }
            | StateCommand::ClearReminderRules { .. }
            | StateCommand::CreateAdhocAlarm { .. }
            | StateCommand::CancelAdhocAlarm { .. }
            | StateCommand::StartTracking { .. }
            | StateCommand::StopTracking
            | StateCommand::LogTime { .. }
            | StateCommand::AdjustTimeEntry { .. }
            | StateCommand::DeleteTimeEntry { .. }
            | StateCommand::CreateHabit { .. }
            | StateCommand::UpdateHabit { .. }
            | StateCommand::MoveHabit { .. }
            | StateCommand::LogHabit { .. }
            | StateCommand::AdjustHabitLog { .. }
            | StateCommand::DeleteHabit { .. }
            | StateCommand::Undo
            | StateCommand::Redo
            | StateCommand::RequestSync => Err(CommandError::TargetNotUndoable(
                NotUndoableReason::BoardCeremony,
            )),
        }
    }
}

/// The synthesis catalogue: builds the inverse `StateCommand` for the
/// head's class, running the watermark check over exactly the class's
/// write set (D7) on the way. No wildcard — a new `StateCommand` arm
/// fails compilation in both classification matches until it gets a
/// row (the lockstep, #244's pattern).
#[allow(clippy::too_many_lines)] // one catalogue; splitting hides the table
fn synthesize_undo(
    app: &AppState,
    entry: &JournalEntry,
    images: &[PreImage],
    watermark: &UndoWatermark,
) -> Result<StateCommand, CommandError> {
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
                (clocks.archived, current.archived, UndoField::Archived),
            ];
            for (recorded, now_clock, field) in content {
                if recorded != now_clock {
                    return Err(refuse(entity, field));
                }
            }
            Ok(StateCommand::DeleteTask { id })
        }
        InverseClass::UpdateTask => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            let Some(current) = app.tasks.get(&pre.id) else {
                return Err(refuse(JournalEntity::Task(pre.id), UndoField::Title));
            };
            let changes = task_changes_from_pre(pre, current, watermark)?;
            Ok(StateCommand::UpdateTask {
                id: pre.id,
                changes,
            })
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
            Ok(StateCommand::SetTaskDone {
                id: pre.id,
                done: pre.done.is_some(),
            })
        }
        InverseClass::MoveTask => {
            let Some(pre) = task_image(images) else {
                return Err(CommandError::NothingToUndo);
            };
            check_task_field(app, pre.id, watermark, UndoField::Position, |clocks| {
                clocks.position
            })?;
            Ok(StateCommand::MoveTask {
                id: pre.id,
                stack: pre.stack,
                order: pre.order,
            })
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
            Ok(StateCommand::RestoreTask {
                id: pre.id,
                resurrect: Box::new(pre.clone()),
            })
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
            Ok(StateCommand::DeleteStack { id })
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
            Ok(StateCommand::RenameStack {
                id: pre.id,
                new_title: pre.title.clone(),
            })
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
            Ok(StateCommand::MoveStack {
                id: pre.id,
                order: pre.order,
            })
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
            Ok(StateCommand::DeleteLabel { id })
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
            Ok(StateCommand::UpdateLabel {
                id: pre.id,
                changes,
            })
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
                Ok(StateCommand::UnassignLabel { task, label })
            } else {
                Ok(StateCommand::AssignLabel { task, label })
            }
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
    Ok(changes)
}

#[cfg(test)]
mod tests;
