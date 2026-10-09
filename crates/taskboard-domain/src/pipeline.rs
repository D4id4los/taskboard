// SPDX-License-Identifier: MIT OR Apache-2.0
//! The fixed-order composition of the conflict policy: the pipeline the
//! state engine runs once per `SyncReport::Completed`.
//!
//! Processing order is fixed (and tested): **push outcomes first** (they
//! establish new baselines; R9 deletes finalize immediately), then board
//! -> stacks -> labels -> tasks (R1 before R7 resolution), then presence
//! reconciliation (R3/R6) for bound-but-absent entities, then the
//! sync-status transition and persistence batch.
//!
//! This module owns no policy of its own: every per-entity decision comes
//! from [`crate::merge`]'s primitives, which never read clocks or
//! generate ids. `ids` and `now` are injected here, not there.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};

use crate::entities::{Label, Stack, Task};
use crate::idgen::IdGenerator;
use crate::ids::{
    LabelId, RemoteBoardId, RemoteCardRef, RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use crate::merge::{
    adopt_board_if_newer, adopt_label_after_push, adopt_remote_board, adopt_remote_label,
    adopt_remote_stack, adopt_remote_task, adopt_stack_after_push, adopt_task_after_push,
    finalize_pushed_task_delete, merge_label, merge_stack, merge_task, remote_wins,
    tombstone_label, tombstone_stack, tombstone_task, utc_min,
};
use crate::outbox::{LocalOp, OpId, PendingOp};
use crate::persistence::PersistenceAction;
use crate::remote::{PushOutcome, PushResult, RemoteBoardSnapshot, RemoteEcho, RemoteIndex};
use crate::state::{AppState, SyncErrorKind, SyncPhase, SyncStatus};

fn op_targets_task(op: &LocalOp, task: TaskId) -> bool {
    match op {
        LocalOp::CreateTask(id)
        | LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::DeleteTask(id)
        | LocalOp::AssignLabel(id, _)
        | LocalOp::UnassignLabel(id, _) => *id == task,
        LocalOp::CreateStack(_)
        | LocalOp::RenameStack(_)
        | LocalOp::DeleteStack(_)
        | LocalOp::CreateLabel(_)
        | LocalOp::UpdateLabel(_)
        | LocalOp::DeleteLabel(_) => false,
    }
}

/// The pipeline the engine calls once per `SyncReport::Completed`.
///
/// Processing order is fixed (and tested): **push outcomes first** (they
/// establish new baselines), then board → stacks → labels → tasks (R1
/// before R7 resolution), then presence reconciliation (R3/R6) for
/// bound-but-absent entities, then the sync-status transition.
///
/// Returns the merged [`AppState`] (with `last_updated` stamped to `now`)
/// and the persistence batch for the storage actor.
///
/// `ids` provides fresh local ids for remotely-created entities (R1);
/// `now` is the observation time for absence tombstones. Neither is ever
/// read by the per-entity merge primitives.
// Still one fixed-order sequence after the R9 decision table was extracted
// into `apply_push_outcomes`; the numbered sections ARE the design.
#[allow(clippy::too_many_lines)]
pub fn apply_sync_report(
    current: &AppState,
    outbox: &[PendingOp],
    snapshot: &RemoteBoardSnapshot,
    pushes: &[PushOutcome],
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> (AppState, Vec<PersistenceAction>) {
    let mut boards = current.boards.clone();
    let mut stacks = current.stacks.clone();
    let mut tasks = current.tasks.clone();
    let mut labels = current.labels.clone();

    // ---- 1. Push outcomes (R9*) ------------------------------------
    let mut ctx = RemoteIndex::from_state(current);
    let mut op_results = apply_push_outcomes(
        pushes,
        outbox,
        &mut tasks,
        &mut stacks,
        &mut labels,
        &mut ctx,
        now,
    );

    // ---- 2. Board ---------------------------------------------------
    let mut board_ids_by_remote: HashMap<RemoteBoardId, crate::ids::BoardId> = current
        .boards
        .iter()
        .filter_map(|(id, b)| Some((b.remote?, *id)))
        .collect();
    let remote_board = &snapshot.board;
    let board_id = if let Some(id) = board_ids_by_remote.get(&remote_board.id).copied() {
        let local = &boards[&id];
        let merged = adopt_board_if_newer(local, remote_board);
        boards.insert(id, merged);
        id
    } else {
        let id = ids.new_board_id();
        boards.insert(id, adopt_remote_board(remote_board, id));
        board_ids_by_remote.insert(remote_board.id, id);
        id
    };
    let board_deleted = boards[&board_id].deleted;
    if board_deleted {
        cascade_board(
            &board_id,
            &mut stacks,
            &mut tasks,
            &mut labels,
            now,
            outbox,
            &mut op_results,
        );
    }

    // ---- 3. Stacks (R1/R4/R5/R6) ------------------------------------
    let mut newly_tombstoned_stack_refs: Vec<RemoteStackId> = Vec::new();
    for remote in &snapshot.stacks {
        if let Some(id) = ctx.stack_by_ref.get(&remote.id).copied() {
            let local = &stacks[&id];
            if remote_wins(
                remote.last_modified,
                local.remote_seen.unwrap_or_else(utc_min),
            ) {
                let merged = merge_stack(local, remote);
                let now_tombstoned = merged.deleted && !local.deleted;
                if now_tombstoned {
                    newly_tombstoned_stack_refs.push(remote.id.stack);
                }
                stacks.insert(id, merged);
            }
        } else {
            let id = ids.new_stack_id();
            stacks.insert(id, adopt_remote_stack(remote, id, board_id));
            ctx.stack_by_ref.insert(remote.id, id);
        }
    }

    // ---- 4. Labels (R1 before R7) -----------------------------------
    for remote in &snapshot.labels {
        if let Some(id) = ctx.label_by_ref.get(&remote.id).copied() {
            let local = &labels[&id];
            if remote_wins(
                remote.last_modified,
                local.remote_seen.unwrap_or_else(utc_min),
            ) {
                let merged = merge_label(local, remote);
                labels.insert(id, merged);
            }
        } else {
            let id = ids.new_label_id();
            labels.insert(id, adopt_remote_label(remote, id, board_id));
            ctx.label_by_ref.insert(remote.id, id);
        }
    }

    // ---- 5. Tasks (R1/R4/R5) ----------------------------------------
    for remote in &snapshot.tasks {
        if let Some(id) = ctx.task_by_ref.get(&remote.id).copied() {
            // R4 fast path: unchanged remote keeps local entirely.
            let was_deleted = tasks[&id].deleted;
            let local = tasks[&id].clone();
            if remote_wins(
                remote.last_modified,
                local.remote_seen.unwrap_or_else(utc_min),
            ) {
                let merged = merge_task(&local, remote, &ctx);
                let resurrected = was_deleted && !merged.deleted;
                tasks.insert(id, merged);
                // R5 resurrect: the pending DeleteTask op is dropped.
                if resurrected {
                    for other in outbox {
                        if !op_results.contains_key(&other.op_id)
                            && other.op == LocalOp::DeleteTask(id)
                        {
                            op_results.insert(other.op_id, false);
                        }
                    }
                }
            }
        } else {
            // Deferred adoption (None) when the remote stack has no local
            // binding: the card joins a later cycle, once its stack exists
            // (merge.rs unmapped-stack strategy).
            let id = ids.new_task_id();
            if let Some(adopted) = adopt_remote_task(remote, id, &ctx) {
                tasks.insert(id, adopted);
                ctx.task_by_ref.insert(remote.id, id);
            }
        }
    }

    // ---- 6. Presence reconciliation (R3) ----------------------------
    // Bound-but-absent entities are gone server-side (snapshot
    // completeness contract). Cards: delete-wins fallback.
    let present_refs: std::collections::BTreeSet<RemoteCardRef> =
        snapshot.tasks.iter().map(|t| t.id).collect();
    let bound_task_refs: Vec<(TaskId, RemoteCardRef)> = tasks
        .iter()
        .filter_map(|(id, t)| Some((*id, t.remote?)))
        .collect();
    for (id, remote_ref) in bound_task_refs {
        // No `!local.deleted` guard here, unlike stacks/labels below: a
        // tombstoned task that kept its binding must still have it cleared
        // (R9c cache-lag safety).
        if !present_refs.contains(&remote_ref) {
            let local = &tasks[&id];
            tasks.insert(id, tombstone_task(local, now));
            for other in outbox {
                if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, id) {
                    op_results.insert(other.op_id, false);
                }
            }
        }
    }
    let present_stack_refs: std::collections::BTreeSet<RemoteStackRef> =
        snapshot.stacks.iter().map(|s| s.id).collect();
    let bound_stack_refs: Vec<(StackId, RemoteStackRef)> = stacks
        .iter()
        .filter_map(|(id, s)| Some((*id, s.remote?)))
        .collect();
    for (id, remote_ref) in bound_stack_refs {
        let local = &stacks[&id];
        if !present_stack_refs.contains(&remote_ref) && !local.deleted {
            stacks.insert(id, tombstone_stack(local, now));
            newly_tombstoned_stack_refs.push(remote_ref.stack);
        }
    }
    let present_label_refs: std::collections::BTreeSet<crate::ids::RemoteLabelRef> =
        snapshot.labels.iter().map(|l| l.id).collect();
    let bound_label_refs: Vec<(LabelId, crate::ids::RemoteLabelRef)> = labels
        .iter()
        .filter_map(|(id, l)| Some((*id, l.remote?)))
        .collect();
    for (id, remote_ref) in bound_label_refs {
        let local = &labels[&id];
        if !present_label_refs.contains(&remote_ref) && !local.deleted {
            labels.insert(id, tombstone_label(local, now));
        }
    }

    // ---- 6b. Stack cascade (R6) -------------------------------------
    // Cards moved out before the delete survive: their remote ref points
    // at a different (live) stack, so only refs inside the deleted stack
    // cascade.
    for stack_remote_num in &newly_tombstoned_stack_refs {
        let cascade_ids: Vec<TaskId> = tasks
            .iter()
            .filter(|(_, task)| {
                task.is_live()
                    && task
                        .remote
                        .is_some_and(|r| r.board == remote_board.id && r.stack == *stack_remote_num)
            })
            .map(|(id, _)| *id)
            .collect();
        for task_id in cascade_ids {
            let local = &tasks[&task_id];
            tasks.insert(task_id, tombstone_task(local, now));
            for other in outbox {
                if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, task_id) {
                    op_results.insert(other.op_id, false);
                }
            }
        }
    }

    // ---- 7. Sync-status transition + persistence batch ---------------
    let mut actions = Vec::new();
    for (id, board) in &boards {
        if current.boards.get(id) != Some(board) {
            actions.push(PersistenceAction::UpsertBoard(board.clone()));
        }
    }
    for (id, stack) in &stacks {
        if current.stacks.get(id) != Some(stack) {
            actions.push(PersistenceAction::UpsertStack(stack.clone()));
        }
    }
    for (id, label) in &labels {
        if current.labels.get(id) != Some(label) {
            actions.push(PersistenceAction::UpsertLabel(label.clone()));
        }
    }
    for (id, task) in &tasks {
        if current.tasks.get(id) != Some(task) {
            actions.push(PersistenceAction::UpsertTask(task.clone()));
        }
    }
    for (op_id, completed) in &op_results {
        actions.push(if *completed {
            PersistenceAction::CompleteOp(*op_id)
        } else {
            PersistenceAction::FailOp(*op_id)
        });
    }

    // saturating_sub defensively: a corrupted persisted outbox with
    // duplicate op ids must not underflow the pending count.
    let remaining_ops = outbox.len().saturating_sub(op_results.len());
    let dead_lettered = op_results.values().any(|completed| !completed);
    let phase = if dead_lettered {
        SyncPhase::Failed {
            last_error: SyncErrorKind::BadRequest,
        }
    } else {
        SyncPhase::Idle
    };
    let sync = SyncStatus {
        phase,
        last_success: Some(now),
        #[allow(clippy::cast_possible_truncation)] // outbox depth is display-scale, not data
        pending_ops: remaining_ops as u32,
    };

    let state = AppState {
        boards,
        stacks,
        tasks,
        labels,
        sync,
        // Sync ingestion is an engine mutation from a foreign source:
        // stamp it like a local command would be.
        last_updated: Some(now),
    };
    (state, actions)
}

/// Executes the push outcomes (R9) against the current entity maps and
/// returns the op bookkeeping: `op_id -> true` (completed) / `false`
/// (failed/cancelled); ops absent from the map stay queued untouched.
#[allow(clippy::too_many_lines)] // one R9 decision table; splitting hides the cases
fn apply_push_outcomes(
    pushes: &[PushOutcome],
    outbox: &[PendingOp],
    tasks: &mut BTreeMap<TaskId, Task>,
    stacks: &mut BTreeMap<StackId, Stack>,
    labels: &mut BTreeMap<LabelId, Label>,
    ctx: &mut RemoteIndex,
    now: DateTime<Utc>,
) -> HashMap<OpId, bool> {
    let mut op_results: HashMap<OpId, bool> = HashMap::new();
    let op_by_id: HashMap<OpId, &PendingOp> = outbox.iter().map(|op| (op.op_id, op)).collect();

    for outcome in pushes {
        let Some(op) = op_by_id.get(&outcome.op) else {
            continue;
        };
        match &outcome.result {
            PushResult::Applied { echo } => {
                // R9c: delete pushes finalize immediately — Deck's DELETE
                // response is authoritative, so the tombstone clears its
                // binding now instead of waiting for the next pull.
                match op.op {
                    LocalOp::DeleteTask(id) => {
                        if let Some(local) = tasks.get(&id) {
                            let finalized = finalize_pushed_task_delete(local);
                            tasks.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        continue;
                    }
                    LocalOp::DeleteStack(id) => {
                        if let Some(local) = stacks.get(&id) {
                            let finalized = tombstone_stack(local, now);
                            stacks.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        continue;
                    }
                    LocalOp::DeleteLabel(id) => {
                        if let Some(local) = labels.get(&id) {
                            let finalized = tombstone_label(local, now);
                            labels.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        continue;
                    }
                    LocalOp::CreateTask(_)
                    | LocalOp::UpdateTask(_)
                    | LocalOp::MoveTask(_)
                    | LocalOp::CreateStack(_)
                    | LocalOp::RenameStack(_)
                    | LocalOp::CreateLabel(_)
                    | LocalOp::UpdateLabel(_)
                    | LocalOp::AssignLabel(..)
                    | LocalOp::UnassignLabel(..) => {}
                }
                match echo {
                    Some(RemoteEcho::Task(echo)) => {
                        if let LocalOp::CreateTask(id) | LocalOp::UpdateTask(id) = op.op
                            && let Some(local) = tasks.get(&id)
                        {
                            let merged = adopt_task_after_push(local, echo, ctx);
                            tasks.insert(id, merged);
                        }
                        op_results.insert(outcome.op, true);
                    }
                    Some(RemoteEcho::Stack(echo)) => {
                        if let LocalOp::CreateStack(id) | LocalOp::RenameStack(id) = op.op
                            && let Some(local) = stacks.get(&id)
                        {
                            let merged = adopt_stack_after_push(local, echo);
                            stacks.insert(id, merged);
                            ctx.stack_by_ref.insert(echo.id, id);
                        }
                        op_results.insert(outcome.op, true);
                    }
                    Some(RemoteEcho::Label(echo)) => {
                        if let LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) = op.op
                            && let Some(local) = labels.get(&id)
                        {
                            let merged = adopt_label_after_push(local, echo);
                            labels.insert(id, merged);
                            ctx.label_by_ref.insert(echo.id, id);
                        }
                        op_results.insert(outcome.op, true);
                    }
                    // R9d: reorder returns no echo — complete and reconcile
                    // via the next pull.
                    None => {
                        op_results.insert(outcome.op, true);
                    }
                }
            }
            // R9e: the resource is gone server-side; fall through to the
            // delete rules.
            PushResult::RemoteMissing => match op.op {
                LocalOp::CreateTask(id)
                | LocalOp::UpdateTask(id)
                | LocalOp::MoveTask(id)
                | LocalOp::DeleteTask(id) => {
                    if let Some(local) = tasks.get(&id) {
                        let tombstoned = tombstone_task(local, now);
                        tasks.insert(id, tombstoned);
                        for other in outbox {
                            if other.op_id != outcome.op
                                && op_targets_task(&other.op, id)
                                && !op_results.contains_key(&other.op_id)
                            {
                                op_results.insert(other.op_id, false);
                            }
                        }
                    }
                    op_results.insert(outcome.op, true);
                }
                LocalOp::CreateStack(id) | LocalOp::RenameStack(id) | LocalOp::DeleteStack(id) => {
                    if let Some(local) = stacks.get(&id) {
                        stacks.insert(id, tombstone_stack(local, now));
                    }
                    op_results.insert(outcome.op, true);
                }
                LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                    if let Some(local) = labels.get(&id) {
                        labels.insert(id, tombstone_label(local, now));
                    }
                    op_results.insert(outcome.op, true);
                }
                LocalOp::AssignLabel(..) | LocalOp::UnassignLabel(..) => {
                    op_results.insert(outcome.op, true);
                }
            },
            PushResult::Rejected { kind } => {
                // BadRequest dead-letters; everything else stays queued
                // untouched for retry.
                if *kind == SyncErrorKind::BadRequest {
                    op_results.insert(outcome.op, false);
                }
            }
        }
    }
    op_results
}

fn cascade_board(
    board_id: &crate::ids::BoardId,
    stacks: &mut BTreeMap<StackId, Stack>,
    tasks: &mut BTreeMap<TaskId, Task>,
    labels: &mut BTreeMap<LabelId, Label>,
    now: DateTime<Utc>,
    outbox: &[PendingOp],
    op_results: &mut HashMap<crate::outbox::OpId, bool>,
) {
    // A deleted board takes its stacks with it: finalize every stack of
    // the board (live ones become tombstones too).
    let stack_ids: Vec<StackId> = stacks
        .iter()
        .filter(|(_, s)| s.board == *board_id)
        .map(|(id, _)| *id)
        .collect();
    for id in stack_ids {
        let stack = &stacks[&id];
        stacks.insert(id, tombstone_stack(stack, now));
    }
    let task_ids: Vec<TaskId> = tasks
        .iter()
        .filter(|(_, t)| t.is_live() && stacks.get(&t.stack).is_some_and(|s| s.board == *board_id))
        .map(|(id, _)| *id)
        .collect();
    for id in task_ids {
        let task = &tasks[&id];
        tasks.insert(id, tombstone_task(task, now));
        for other in outbox {
            if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, id) {
                op_results.insert(other.op_id, false);
            }
        }
    }
    let label_ids: Vec<LabelId> = labels
        .iter()
        .filter(|(_, l)| l.board == *board_id && !l.deleted)
        .map(|(id, _)| *id)
        .collect();
    for id in label_ids {
        let label = &labels[&id];
        labels.insert(id, tombstone_label(label, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_testutil::*;
    use crate::remote::{RemoteIndex, RemoteLabel};
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use uuid;

    #[test]
    fn remote_absence_tombstones_and_cancels_pending_ops() {
        let task = bound_task(1);
        let state = base_state(task);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(555)),
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[op],
            &empty_snapshot,
            &[],
            &CountingIds::default(),
            ts(BASE + 3_600),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted, "R3: delete-wins on observed absence");
        assert!(merged.remote.is_none());
        assert!(merged.remote_seen.is_none());
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(555)))),
            "the task's pending ops must be cancelled"
        );
    }

    #[test]
    fn newer_stack_delete_beats_older_local_rename_and_cascades() {
        let mut local = local_stack();
        local.title = "renamed".into();
        local.clocks.title = ts(BASE + 300); // 10:05
        let mut remote = remote_stack(1, BASE + 420); // 10:07
        remote.deleted_at = Some(ts(BASE + 420));

        let merged = merge_stack(&local, &remote);
        assert!(merged.deleted);

        // Pipeline cascade: the stack's bound tasks are tombstoned.
        let task = bound_task(1);
        let state = base_state(task);
        let snap = snapshot(
            vec![remote],
            // The card is still listed under the deleted stack (cache lag):
            // the cascade applies regardless, per R6.
            vec![remote_task(1, 1, BASE)],
        );
        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 600),
        );
        let merged_task = app.0.tasks.values().next().unwrap();
        assert!(merged_task.deleted, "stack tombstone cascades to its tasks");
    }

    #[test]
    fn echo_is_adopted_unconditionally_with_max_clocks() {
        let mut local = bound_task(1);
        local.remote = None;
        local.title = "  untidied  ".into();
        local.clocks = clocks_at(BASE + 3_599); // 10:59:59 sub-second edits
        let mut echo = remote_task(1, 1, BASE + 3_600); // 11:00:00
        echo.title = "untidied".into(); // server-trimmed

        let ctx = RemoteIndex::default();
        let merged = adopt_task_after_push(&local, &echo, &ctx);

        // Unconditional adoption, not LWW: the normalization lands even
        // though the echo timestamp is not "newer" than the local edit.
        assert_eq!(merged.title, "untidied");
        assert_eq!(merged.clocks.title, echo.last_modified);
        assert_eq!(merged.remote_seen, Some(echo.last_modified));
        assert_eq!(merged.remote, Some(echo.id));

        // The next pull (same ts_r) is a fast-path no-op.
        let state = base_state(merged);
        let snap = single_stack_snapshot(vec![echo], BASE + 3_600);
        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 3_660),
        );
        assert!(
            app.1
                .iter()
                .all(|a| !matches!(a, PersistenceAction::UpsertTask(_))),
            "pull must not undo the echo adoption"
        );
    }

    #[test]
    fn reorder_push_completes_without_touching_state() {
        let task = bound_task(1);
        let state = base_state(task);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(1)),
            op: LocalOp::MoveTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(1)),
                result: PushResult::Applied { echo: None },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert_eq!(app.0.sync.pending_ops, 0, "reorder op completes");
        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.order, 1, "baseline untouched until the next pull");
    }

    #[test]
    fn remote_missing_on_update_push_tombstones() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(2));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        // The card is absent from the snapshot too (consistent "gone").
        let snap = single_stack_snapshot(vec![], BASE);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::RemoteMissing,
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted);
    }

    #[test]
    fn rejected_op_stays_queued_but_bad_request_dead_letters() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(3));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Rejected {
                    kind: SyncErrorKind::Network,
                },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );
        assert_eq!(
            app.0.sync.pending_ops, 1,
            "retryable rejection stays queued"
        );
        assert_eq!(app.0.sync.phase, SyncPhase::Idle);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Rejected {
                    kind: SyncErrorKind::BadRequest,
                },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );
        assert_eq!(app.0.sync.pending_ops, 0, "BadRequest dead-letters");
        assert_eq!(
            app.0.sync.phase,
            SyncPhase::Failed {
                last_error: SyncErrorKind::BadRequest
            }
        );
    }

    #[test]
    fn pipeline_adopts_new_remote_entities_before_resolving_task_labels() {
        // A remote label (11) and a task referencing it, neither known
        // locally: after the pipeline, the task's label set references the
        // freshly adopted label entity.
        let mut label = RemoteLabel {
            id: crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD_NUM),
                label: crate::ids::RemoteLabelId(11),
            },
            title: "urgent".into(),
            color: "00ff00".into(),
            deleted_at: None,
            last_modified: ts(BASE + 60),
        };
        label.last_modified = ts(BASE + 60);
        let mut remote = remote_task(1, 9, BASE + 60);
        remote.labels.insert(crate::ids::RemoteLabelId(11));

        let state = base_state(bound_task(1));
        let snap = snapshot(
            vec![remote_stack(1, BASE)],
            vec![remote_task(1, 1, BASE), remote.clone()],
        );
        let mut snap_labels = snap.clone();
        snap_labels.labels.push(label.clone());

        let app = apply_sync_report(
            &state,
            &[],
            &snap_labels,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        let adopted_label = app
            .0
            .labels
            .values()
            .find(|l| l.remote == Some(label.id))
            .expect("remote label adopted");
        let adopted_task = app
            .0
            .tasks
            .values()
            .find(|t| t.remote == Some(remote.id))
            .expect("remote task adopted");
        assert_eq!(
            adopted_task.labels,
            BTreeSet::from([adopted_label.id]),
            "R7 resolution uses the post-adoption mapping"
        );
        assert_eq!(
            adopted_task.stack,
            state.stacks.values().next().unwrap().id,
            "remote stack resolves through the index",
        );
        assert_eq!(adopted_task.order, remote.order);
    }

    #[test]
    fn reapplying_same_snapshot_emits_no_actions() {
        let state = base_state(bound_task(1));
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 60)], BASE + 60);

        let first = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );
        let second_state = app_into_state(&first);
        let second = apply_sync_report(
            &second_state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 180),
        );

        assert!(
            second.1.is_empty(),
            "second application must be a no-op, got {:?}",
            second.1
        );
    }

    #[test]
    fn push_outcomes_establish_baselines_before_snapshot_merge() {
        // Local edited title at 10:05; push succeeded, echo at 11:00 with
        // the normalized title; snapshot carries the same ts_r = 11:00.
        // Without push-first, the snapshot merge (ts_r == remote_seen of
        // nothing yet) could re-process; with push-first the R4 fast path
        // swallows the snapshot entry.
        let mut local = bound_task(1);
        local.title = "local edit".into();
        local.clocks.title = ts(BASE + 300);
        let state = base_state(local);

        let mut echo = remote_task(1, 1, BASE + 3_600);
        echo.title = "normalized".into();
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(9));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![echo.clone()], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(echo)),
                },
            }],
            &CountingIds::default(),
            ts(BASE + 3_660),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.pending_ops, 0);
    }

    #[test]
    fn task_echo_for_a_move_op_is_not_adopted() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(45));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::MoveTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let mut echo = remote_task(1, 1, BASE + 60);
        echo.title = "echoed title".into();
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(echo)),
                },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        // R9a echo adoption applies to create/update pushes only; a move
        // has no echo (R9d) and must not consume one addressed to it.
        let state_after = app_into_state(&app);
        assert_eq!(task_by_card(&state_after, 1).title, "local 1");
    }

    #[test]
    fn resurrect_and_remote_missing_leave_unrelated_ops_queued() {
        // Resurrect path (R5): only the task's DeleteTask op is dropped;
        // an unrelated pending op stays queued.
        let mut task = bound_task(1);
        task.deleted = true;
        task.clocks.deleted = ts(BASE + 300);
        let state = base_state(task);
        let stack = local_stack();
        let mut stack = stack;
        stack.remote = None;
        let stack_id = stack.id;
        let mut state = state;
        state.stacks.insert(stack_id, stack);
        let delete_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(46)),
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(47)),
            op: LocalOp::CreateStack(stack_id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 360)], BASE + 360);

        let app = apply_sync_report(
            &state,
            &[delete_op, stack_op],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 420),
        );

        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(46)))),
            "the resurrected task's delete op is dropped",
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(47)))),
            "the unrelated stack op must stay queued",
        );
        assert_eq!(app.0.sync.pending_ops, 1);

        // R3 path: same expectation — cancellation is op-targeted.
        let task2 = bound_task(2);
        let state2 = base_state(task2);
        let update_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(48)),
            op: LocalOp::UpdateTask(task_by_card(&state2, 2).id),
            queued_at: ts(BASE),
        };
        let stack_op2 = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(49)),
            op: LocalOp::CreateStack(*state2.stacks.keys().next().unwrap()),
            queued_at: ts(BASE),
        };
        // Pure pull (no push outcomes): the presence reconciliation itself
        // cancels the task's ops.
        let snap2 = single_stack_snapshot(vec![], BASE);

        let app2 = apply_sync_report(
            &state2,
            &[update_op, stack_op2],
            &snap2,
            &[],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(
            app2.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(48)))),
            "R3 cancels the absent task's own op",
        );
        assert!(
            !app2.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(49)))),
            "R3 cancellation must not touch unrelated ops",
        );
        assert_eq!(app2.0.sync.pending_ops, 1);
    }

    #[test]
    fn stack_cascade_spares_moved_out_and_offline_tasks() {
        let task_a = bound_task(1); // lives in stack 1, gets cascaded
        let mut task_b = bound_task(2); // moved out to stack 2 before the delete
        let stack2 = Stack {
            id: StackId::from(uuid::Uuid::from_u128(901)),
            remote: Some(stack_ref(2)),
            board: crate::ids::BoardId::from(uuid::Uuid::from_u128(800)),
            title: "stack 2".into(),
            order: 2,
            archived: false,
            deleted: false,
            clocks: crate::entities::StackClocks {
                title: ts(BASE),
                order: ts(BASE),
                deleted: ts(BASE),
            },
            remote_seen: Some(ts(BASE)),
        };
        task_b.stack = stack2.id;
        task_b.remote = Some(card_ref(2, 5));
        let mut offline = bound_task(4); // local-only draft (R2)
        offline.remote = None;
        offline.remote_seen = None;
        offline.title = "offline draft".into();

        let task_a_id = task_a.id;
        let moved_id = task_b.id;
        let offline_id = offline.id;
        let mut state = base_state(task_a);
        let stack1_id = *state.stacks.keys().next().unwrap();
        state.stacks.insert(stack2.id, stack2);
        state.tasks.insert(task_b.id, task_b);
        state.tasks.insert(offline.id, offline);

        let move_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(52)),
            op: LocalOp::MoveTask(task_a_id),
            queued_at: ts(BASE),
        };
        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(53)),
            op: LocalOp::CreateStack(stack1_id),
            queued_at: ts(BASE),
        };

        let mut deleted_stack = remote_stack(1, BASE + 60);
        deleted_stack.deleted_at = Some(ts(BASE + 60));
        let snap = snapshot(
            vec![deleted_stack, remote_stack(2, BASE)],
            vec![remote_task(1, 1, BASE), remote_task(2, 5, BASE)],
        );

        let app = apply_sync_report(
            &state,
            &[move_op, stack_op],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        assert!(
            app.0.tasks[&task_a_id].deleted,
            "task in the deleted stack cascades"
        );
        assert!(
            !app.0.tasks[&offline_id].deleted,
            "offline local-only task is outside every cascade filter",
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.id == offline_id)),
        );
        assert!(
            !app.0.tasks[&moved_id].deleted,
            "moved-out card survives via its target-stack listing",
        );
        assert_eq!(
            app.0.sync.pending_ops, 1,
            "only the cascaded task's op is cancelled; the stack op stays queued",
        );
    }

    #[test]
    fn older_remote_than_remote_seen_keeps_local_entity() {
        // A remote entity OLDER than our baseline must not merge, even with
        // different content (R4 guards stacks, labels, and tasks alike).
        let mut stack = local_stack();
        stack.remote_seen = Some(ts(BASE + 120));
        stack.title = "locally newer".into();
        let older = remote_stack(1, BASE + 60); // older than remote_seen

        let mut state = base_state(bound_task(1));
        let stack_id = stack.id;
        state.stacks.insert(stack_id, stack);
        let snap = snapshot(vec![older], vec![remote_task(1, 1, BASE)]);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 180),
        );
        assert_eq!(
            app.0.stacks[&stack_id].title, "locally newer",
            "older remote must not overwrite the baseline",
        );
    }

    #[test]
    fn already_finalized_entities_are_not_refinalized_on_absence() {
        // A tombstoned stack/label that kept its binding (cache-lag re-bind)
        // must not be re-finalized when absent: no clock bump, no action.
        let mut state = base_state(bound_task(1));
        let stack_id = *state.stacks.keys().next().unwrap();
        let mut stack = local_stack();
        stack.deleted = true;
        stack.clocks.deleted = ts(BASE + 30);
        state.stacks.insert(stack_id, stack);
        let mut label = bound_label(9);
        label.deleted = true;
        label.clocks.deleted = ts(BASE + 30);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let snap = single_stack_snapshot(vec![], BASE);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 90),
        );

        assert_eq!(
            app.0.stacks[&stack_id].clocks.deleted,
            ts(BASE + 30),
            "absence must not bump an existing tombstone clock",
        );
        assert_eq!(app.0.labels[&label_id].clocks.deleted, ts(BASE + 30));
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertStack(s) if s.id == stack_id)),
        );
    }

    #[test]
    fn board_cascade_finalizes_present_entities_and_spares_the_dead() {
        // Cache-lag snapshot: the board is gone but its stacks/tasks/labels
        // are still listed — only the cascade can finalize them here (the
        // presence reconciliation never fires for present resources).
        let mut state = base_state(bound_task(1));
        let live_label = bound_label(6);
        let live_label_id = live_label.id;
        state.labels.insert(live_label_id, live_label);
        let mut dead_label = bound_label(10);
        dead_label.deleted = true;
        dead_label.clocks.deleted = ts(BASE + 30);
        let dead_label_id = dead_label.id;
        state.labels.insert(dead_label_id, dead_label);
        let stack_id = *state.stacks.keys().next().unwrap();
        let task_id = task_by_card(&state, 1).id;
        let mut dead_task = bound_task(3);
        dead_task.deleted = true;
        dead_task.clocks.deleted = ts(BASE + 30);
        dead_task.remote = None; // already finalized; outside every filter here
        dead_task.remote_seen = None;
        let dead_task_id = dead_task.id;
        state.tasks.insert(dead_task_id, dead_task);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(50)),
            op: LocalOp::UpdateTask(task_id),
            queued_at: ts(BASE),
        };
        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(51)),
            op: LocalOp::CreateStack(stack_id),
            queued_at: ts(BASE),
        };

        let mut board = remote_board(REMOTE_BOARD_NUM, BASE + 60);
        board.deleted_at = Some(ts(BASE + 60));
        let snap = RemoteBoardSnapshot {
            board,
            stacks: vec![remote_stack(1, BASE)],
            tasks: vec![remote_task(1, 1, BASE)],
            labels: vec![remote_label(6, BASE)],
        };

        let app = apply_sync_report(
            &state,
            &[op, stack_op],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        assert!(
            app.0.stacks[&stack_id].deleted,
            "cascade finalizes the listed stack"
        );
        assert!(
            app.0.tasks[&task_id].deleted,
            "cascade tombstones the listed task"
        );
        assert_eq!(
            app.0.sync.pending_ops, 1,
            "stack op survives; task op cancelled"
        );
        assert!(app.0.labels[&live_label_id].deleted);
        assert_eq!(
            app.0.labels[&dead_label_id].clocks.deleted,
            ts(BASE + 30),
            "already-dead label is not re-finalized",
        );
        assert_eq!(
            app.0.tasks[&dead_task_id].clocks.deleted,
            ts(BASE + 30),
            "already-dead task is not re-tombstoned",
        );
    }

    #[test]
    fn op_targets_task_matches_only_that_tasks_ops() {
        let t = TaskId::from(uuid::Uuid::from_u128(1));
        let other = TaskId::from(uuid::Uuid::from_u128(2));
        let s = StackId::from(uuid::Uuid::from_u128(3));
        let l = LabelId::from(uuid::Uuid::from_u128(4));

        for op in [
            LocalOp::CreateTask(t),
            LocalOp::UpdateTask(t),
            LocalOp::MoveTask(t),
            LocalOp::DeleteTask(t),
            LocalOp::AssignLabel(t, l),
            LocalOp::UnassignLabel(t, l),
        ] {
            assert!(op_targets_task(&op, t));
            assert!(!op_targets_task(&op, other));
        }
        for op in [
            LocalOp::CreateStack(s),
            LocalOp::RenameStack(s),
            LocalOp::DeleteStack(s),
            LocalOp::CreateLabel(l),
            LocalOp::UpdateLabel(l),
            LocalOp::DeleteLabel(l),
        ] {
            assert!(!op_targets_task(&op, t));
        }
    }

    #[test]
    fn pushed_delete_finalizes_tombstone_immediately() {
        let mut task = bound_task(1);
        task.deleted = true;
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(21));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        // The delete has fully propagated: the card is absent remotely.
        let snap = single_stack_snapshot(vec![], BASE);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied { echo: None },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let task_id = task_by_card(&state, 1).id;
        let merged = &app.0.tasks[&task_id];
        assert!(merged.deleted);
        assert!(
            merged.remote.is_none(),
            "R9c clears the binding immediately"
        );
        assert!(merged.remote_seen.is_none());
        assert_eq!(app.0.sync.pending_ops, 0);
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "the delete op completes, not fails"
        );
    }

    #[test]
    fn create_stack_push_echo_binds_remote() {
        let mut stack = local_stack();
        stack.remote = None;
        let state = base_state(bound_task(1));
        let stack_id = state.stacks.keys().next().unwrap();
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(22));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::CreateStack(*stack_id),
            queued_at: ts(BASE),
        };
        let mut echo = remote_stack(1, BASE + 60);
        echo.title = "normalized".into();
        let snap = snapshot(vec![remote_stack(1, BASE)], vec![]);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Stack(echo)),
                },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let merged = &app.0.stacks[stack_id];
        assert_eq!(merged.remote, Some(stack_ref(1)));
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.pending_ops, 0);
    }

    #[test]
    fn create_label_push_echo_binds_remote() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(1);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(23));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::CreateLabel(label_id),
            queued_at: ts(BASE),
        };
        let mut echo = remote_label(1, BASE + 60);
        echo.title = "normalized".into();
        let mut snap = single_stack_snapshot(vec![], BASE);
        snap.labels.push(remote_label(1, BASE));
        let expected_ref = echo.id;

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Label(echo)),
                },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let merged = &app.0.labels[&label_id];
        assert_eq!(merged.remote, Some(expected_ref));
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.pending_ops, 0);
    }

    #[test]
    fn pushed_stack_and_label_deletes_finalize_immediately() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(8);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let stack_id = *state.stacks.keys().next().unwrap();
        let mut stack = local_stack();
        stack.deleted = true;
        state.stacks.insert(stack_id, stack);
        let mut label = bound_label(8);
        label.deleted = true;
        state.labels.insert(label_id, label);
        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(41)),
            op: LocalOp::DeleteStack(stack_id),
            queued_at: ts(BASE),
        };
        let label_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(42)),
            op: LocalOp::DeleteLabel(label_id),
            queued_at: ts(BASE),
        };
        // Fully propagated deletes: nothing listed anymore.
        let snap = snapshot(vec![], vec![]);

        let app = apply_sync_report(
            &state,
            &[stack_op, label_op],
            &snap,
            &[
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(41)),
                    result: PushResult::Applied { echo: None },
                },
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(42)),
                    result: PushResult::Applied { echo: None },
                },
            ],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(app.0.stacks[&stack_id].deleted);
        assert!(app.0.stacks[&stack_id].remote.is_none());
        assert!(app.0.labels[&label_id].deleted);
        assert!(app.0.labels[&label_id].remote.is_none());
        assert_eq!(app.0.sync.pending_ops, 0);
    }

    #[test]
    fn remote_missing_stack_and_label_ops_finalize_tombstones() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(2);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let stack_id = *state.stacks.keys().next().unwrap();

        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(24)),
            op: LocalOp::DeleteStack(stack_id),
            queued_at: ts(BASE),
        };
        let label_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(25)),
            op: LocalOp::DeleteLabel(label_id),
            queued_at: ts(BASE),
        };
        let snap = snapshot(vec![], vec![]);

        let app = apply_sync_report(
            &state,
            &[stack_op, label_op],
            &snap,
            &[
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(24)),
                    result: PushResult::RemoteMissing,
                },
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(25)),
                    result: PushResult::RemoteMissing,
                },
            ],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(app.0.stacks[&stack_id].deleted);
        assert!(app.0.stacks[&stack_id].remote.is_none());
        assert!(app.0.labels[&label_id].deleted);
        assert!(app.0.labels[&label_id].remote.is_none());
        assert_eq!(app.0.sync.pending_ops, 0);
    }

    #[test]
    fn remote_missing_move_op_tombstones_and_assign_op_completes() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(3);
        let label_id = label.id;
        let task_id = task_by_card(&state, 1).id;
        state.labels.insert(label_id, label);
        let move_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(26)),
            op: LocalOp::MoveTask(task_id),
            queued_at: ts(BASE),
        };
        let assign_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(27)),
            op: LocalOp::AssignLabel(task_id, label_id),
            queued_at: ts(BASE),
        };
        let delete_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(43)),
            op: LocalOp::DeleteTask(task_id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![], BASE);

        let app = apply_sync_report(
            &state,
            &[move_op, assign_op, delete_op],
            &snap,
            &[
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(26)),
                    result: PushResult::RemoteMissing,
                },
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(27)),
                    result: PushResult::RemoteMissing,
                },
                PushOutcome {
                    op: crate::outbox::OpId(uuid::Uuid::from_u128(43)),
                    result: PushResult::RemoteMissing,
                },
            ],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(app.0.sync.pending_ops, 0, "all three ops resolved");
    }

    #[test]
    fn remote_missing_cancels_sibling_ops_of_the_task() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(4);
        let label_id = label.id;
        let task_id = task_by_card(&state, 1).id;
        state.labels.insert(label_id, label);
        let update_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(28)),
            op: LocalOp::UpdateTask(task_id),
            queued_at: ts(BASE),
        };
        let assign_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(29)),
            op: LocalOp::AssignLabel(task_id, label_id),
            queued_at: ts(BASE),
        };
        let stack_op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(54)),
            op: LocalOp::CreateStack(*state.stacks.keys().next().unwrap()),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![], BASE);

        let app = apply_sync_report(
            &state,
            &[update_op, assign_op, stack_op],
            &snap,
            // Only the update reports gone; the assign must be cancelled
            // with it (the task no longer exists).
            &[PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(28)),
                result: PushResult::RemoteMissing,
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(
            app.0.sync.pending_ops, 1,
            "sibling assign cancelled, stack op queued"
        );
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(29)))),
        );
        assert_eq!(
            app.0.sync.pending_ops, 1,
            "the unrelated stack op stays queued"
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(54)))),
        );
    }

    #[test]
    fn stack_absent_from_snapshot_is_finalized() {
        let task = bound_task(1);
        let state = base_state(task);
        let stack_id = *state.stacks.keys().next().unwrap();
        // The card is still listed (Deck's cascade lag), but its stack is
        // gone from the stacks listing.
        let snap = RemoteBoardSnapshot {
            board: remote_board(REMOTE_BOARD_NUM, BASE),
            stacks: vec![],
            tasks: vec![remote_task(1, 1, BASE)],
            labels: vec![],
        };

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let merged = &app.0.stacks[&stack_id];
        assert!(merged.deleted);
        assert!(merged.remote.is_none());
    }

    #[test]
    fn label_absent_from_snapshot_is_finalized() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(5);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        let merged = &app.0.labels[&label_id];
        assert!(merged.deleted);
        assert!(merged.remote.is_none());
    }

    #[test]
    fn board_tombstone_cascades_stacks_tasks_and_labels() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(6);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let stack_id = *state.stacks.keys().next().unwrap();
        let task_id = task_by_card(&state, 1).id;
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(44)),
            op: LocalOp::UpdateTask(task_id),
            queued_at: ts(BASE),
        };

        let mut board = remote_board(REMOTE_BOARD_NUM, BASE + 60);
        board.deleted_at = Some(ts(BASE + 60));
        // Completeness contract: a gone board lists nothing.
        let snap = RemoteBoardSnapshot {
            board,
            stacks: vec![],
            tasks: vec![],
            labels: vec![],
        };

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        assert!(app.0.boards.values().all(|b| b.deleted));
        assert!(app.0.stacks[&stack_id].deleted, "stacks finalized");
        assert!(app.0.stacks[&stack_id].remote.is_none());
        assert!(
            app.0.tasks.values().all(|t| t.deleted),
            "all the board's tasks tombstoned"
        );
        assert!(app.0.labels[&label_id].deleted, "labels finalized");
        assert_eq!(
            app.0.sync.pending_ops, 0,
            "the board's tasks' ops are cancelled by the cascade"
        );
    }

    #[test]
    fn stack_delete_cascade_cancels_cascaded_tasks_ops() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(30));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::MoveTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        // Cache lag: the card is still listed under the now-deleted stack.
        let mut stack = remote_stack(1, BASE + 60);
        stack.deleted_at = Some(ts(BASE + 60));
        let snap = snapshot(vec![stack], vec![remote_task(1, 1, BASE)]);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        let task_id = task_by_card(&state, 1).id;
        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(app.0.sync.pending_ops, 0, "cascade cancels the moved op");
    }

    #[test]
    fn resurrect_via_pipeline_drops_pending_delete_op() {
        let mut task = bound_task(1);
        task.deleted = true;
        task.clocks.deleted = ts(BASE + 300);
        let state = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(31));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 360)], BASE + 360);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 420),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert!(!merged.deleted, "newer remote edit resurrects");
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == op_id)),
            "the pending DeleteTask op is dropped"
        );
    }

    #[test]
    fn card_with_unresolvable_stack_is_deferred_then_adopted() {
        // Referentially inconsistent snapshot (Deck cache lag): a card is
        // listed but its stack is not. The card is NOT adopted this cycle
        // (no fabricated stack binding); it joins as soon as its stack is
        // listed in a later snapshot.
        let state = base_state(bound_task(1));
        let orphan_snap = single_stack_snapshot(vec![remote_task(3, 9, BASE + 60)], BASE + 60);

        let first = apply_sync_report(
            &state,
            &[],
            &orphan_snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );
        assert!(
            first
                .0
                .tasks
                .values()
                .all(|t| t.remote != Some(card_ref(3, 9))),
            "orphan-stack card is not adopted",
        );
        assert!(
            !first.1.iter().any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.remote == Some(card_ref(3, 9)))),
            "the deferred card emits no persistence action",
        );

        let next_snap = snapshot(
            vec![remote_stack(1, BASE + 60), remote_stack(3, BASE + 60)],
            vec![remote_task(1, 1, BASE), remote_task(3, 9, BASE + 60)],
        );
        let (second, _) = apply_sync_report(
            &app_into_state(&first),
            &[],
            &next_snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 180),
        );
        let adopted = second
            .tasks
            .values()
            .find(|t| t.remote == Some(card_ref(3, 9)))
            .expect("card adopted once its stack is listed");
        assert_eq!(
            adopted.stack,
            second
                .stacks
                .values()
                .find(|s| s.remote == Some(stack_ref(3)))
                .unwrap()
                .id,
        );
    }

    #[test]
    fn local_only_task_survives_sync_untouched() {
        let mut state = base_state(bound_task(1));
        let mut offline = bound_task(2);
        offline.remote = None;
        offline.remote_seen = None;
        offline.title = "offline draft".into();
        state.tasks.insert(offline.id, offline);
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 60)], BASE + 60);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        let draft = app
            .0
            .tasks
            .values()
            .find(|t| t.remote.is_none())
            .expect("offline task still present");
        assert_eq!(draft.title, "offline draft");
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertTask(t) if t.remote.is_none())),
            "R2: the never-pushed task is untouched"
        );
    }

    #[test]
    fn new_remote_stack_adopts_in_pipeline() {
        let state = base_state(bound_task(1));
        let snap = snapshot(
            vec![remote_stack(1, BASE), remote_stack(3, BASE + 60)],
            vec![remote_task(1, 1, BASE)],
        );

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        let adopted = app
            .0
            .stacks
            .values()
            .find(|s| s.remote == Some(stack_ref(3)))
            .expect("remote stack adopted");
        assert_eq!(adopted.title, "stack 3");
        assert_eq!(adopted.remote_seen, Some(ts(BASE + 60)));
    }

    #[test]
    fn unknown_remote_board_is_adopted() {
        let state = base_state(bound_task(1));
        let snap = RemoteBoardSnapshot {
            board: remote_board(88, BASE + 60),
            stacks: vec![remote_stack(1, BASE)],
            tasks: vec![],
            labels: vec![],
        };

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        let adopted = app
            .0
            .boards
            .values()
            .find(|b| b.remote == Some(board(88)))
            .expect("second board adopted (multi-board-ready shape)");
        assert_eq!(adopted.title, "board 88");
    }

    #[test]
    fn changed_remote_board_content_emits_board_upsert() {
        let state = base_state(bound_task(1));
        let snap = RemoteBoardSnapshot {
            board: remote_board(REMOTE_BOARD_NUM, BASE + 60),
            stacks: vec![remote_stack(1, BASE)],
            tasks: vec![],
            labels: vec![],
        };

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::UpsertBoard(b) if b.title == "board 77")),
        );
    }

    #[test]
    fn changed_remote_label_merges_in_pipeline() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(7);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let mut snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        snap.labels.push(remote_label(7, BASE + 60));

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            &CountingIds::default(),
            ts(BASE + 120),
        );

        assert_eq!(app.0.labels[&label_id].title, "label 7 renamed");
        assert!(app.1.iter().any(
            |a| matches!(a, PersistenceAction::UpsertLabel(l) if l.title == "label 7 renamed")
        ),);
    }

    #[test]
    fn push_outcome_for_unknown_op_is_ignored() {
        let state = base_state(bound_task(1));
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(99)),
                result: PushResult::Applied { echo: None },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert!(app.1.is_empty());
    }

    proptest! {
    #[test]
    fn equal_replicas_converge(
        remote_last_modified in 0_i64..10_000,
        local_title_edit in 0_i64..10_000,
        card in 1_u64..100,
    ) {
        let mut local = bound_task(card);
        local.title = format!("local {local_title_edit}");
        local.clocks.title = ts(local_title_edit);
        let state = base_state(local);
        let remote = remote_task(1, card, remote_last_modified);
        let snap = single_stack_snapshot(vec![remote], remote_last_modified.max(BASE));

        let a = apply_sync_report(&state, &[], &snap, &[], &CountingIds::default(), ts(20_000));
        let b = apply_sync_report(&state, &[], &snap, &[], &CountingIds::default(), ts(20_000));
        prop_assert_eq!(a.0.tasks, b.0.tasks);
    }
    }
}
