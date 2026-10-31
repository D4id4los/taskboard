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
//!
//! # Logging contract
//!
//! One `debug` span per pipeline entry (`apply_sync_report` /
//! `apply_push_report`), carrying the cycle's shape; `trace` carries the
//! entry identities (board ref, `read_at`, `now`); `debug` events report
//! rule verdicts and the closing summary as typed facts. Unchanged-remote
//! fast paths are silent by design: a steady-state cycle emits only the
//! span and the closing summary. Tests never assert this text (AGENTS
//! §5); the events are behavior-neutral by construction.

use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::entities::{Board, Label, Stack, Task};
use crate::idgen::IdGenerator;
use crate::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteStackId, RemoteStackRef,
    StackId, TaskId,
};
use crate::merge::{
    CreateSignature, RULE_R_DUP, RULE_R1, RULE_R3, RULE_R4, RULE_R5, RULE_R6, RULE_R9,
    adopt_board_after_delete_push, adopt_board_after_push, adopt_label_after_push,
    adopt_remote_board, adopt_remote_label, adopt_remote_stack, adopt_remote_task,
    adopt_stack_after_push, adopt_task_after_push, collapse_for_task,
    detect_crash_window_duplicates, finalize_pushed_task_delete, fold_reminder_dismissals,
    fold_reminder_rules, fold_task_app_data, merge_board, merge_label, merge_stack, merge_task,
    rebind_task_to_survivor, remote_wins, tombstone_board, tombstone_label, tombstone_stack,
    tombstone_task, utc_min,
};
use crate::outbox::{LocalOp, OpId, PendingOp, op_targets_task};
use crate::persistence::PersistenceAction;
use crate::remote::{
    PushOutcome, PushResult, RemoteBoardSnapshot, RemoteEcho, RemoteIndex, RemoteTask,
};
use crate::state::{AppState, SyncErrorKind, SyncPhase};

/// The entity id an op targets, for `tracing` fields on push-outcome
/// events (the map lookup may miss; the op always names its target).
fn op_task_id(op: &LocalOp) -> Option<TaskId> {
    match op {
        LocalOp::CreateTask(id)
        | LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::DeleteTask(id) => Some(*id),
        _ => None,
    }
}

fn op_stack_id(op: &LocalOp) -> Option<StackId> {
    match op {
        LocalOp::CreateStack(id)
        | LocalOp::RenameStack(id)
        | LocalOp::MoveStack(id)
        | LocalOp::DeleteStack(id) => Some(*id),
        _ => None,
    }
}

fn op_label_id(op: &LocalOp) -> Option<LabelId> {
    match op {
        LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => Some(*id),
        _ => None,
    }
}

fn op_board_id(op: &LocalOp) -> Option<BoardId> {
    match op {
        LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => Some(*id),
        _ => None,
    }
}

/// Task fields protected from a pull's LWW adoption because a pending op
/// covering them was queued *after* the cycle read the state (`read_at`).
/// Deck stamps its whole-card `last_modified` on any write, so the pull
/// following our own push carries stamps newer than every local clock —
/// for a field the push never carried, adopting its snapshot value would
/// silently clobber a newer local edit (the "self-clobber" race). Ops the
/// cycle *did* see (queued no later than `read_at`) are not protected:
/// their fields' post-push truth is exactly what the snapshot reports.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // one flag per protected task field
pub(crate) struct PendingTaskIntent {
    pub(crate) title: bool,
    pub(crate) description: bool,
    pub(crate) duedate: bool,
    pub(crate) scheduled: bool,
    pub(crate) done: bool,
    pub(crate) archived: bool,
    pub(crate) position: bool,
    pub(crate) labels: bool,
    pub(crate) effort: bool,
    pub(crate) importance: bool,
    pub(crate) urgency: bool,
    pub(crate) recur: bool,
    /// The D-S6 rules flag: a deliberate non-clock member (rules carry
    /// their own embedded write clocks, not `TaskClocks`).
    pub(crate) reminder_rules: bool,
    pub(crate) deleted: bool,
}

/// Collects per-task pending intents from one pass over the outbox.
fn pending_task_intents(
    outbox: &[PendingOp],
    read_at: DateTime<Utc>,
) -> HashMap<TaskId, PendingTaskIntent> {
    let mut map: HashMap<TaskId, PendingTaskIntent> = HashMap::new();
    for op in outbox {
        if op.queued_at <= read_at {
            continue;
        }
        // The D-S6 guard mirror for the document lane: a queued
        // `UpdateAppData` op the cycle never saw protects all four
        // app-field flags *and* the task's rules (coarse by design —
        // the mirror mirrors both lanes; D-V2 needs no change here).
        // Same boundary convention as the native lanes: `queued_at >
        // read_at` only; `==` was seen by the cycle.
        if let LocalOp::UpdateAppData(task) = op.op {
            let p = map.entry(task).or_default();
            p.effort = true;
            p.importance = true;
            p.urgency = true;
            p.recur = true;
            p.scheduled = true;
            p.reminder_rules = true;
            continue;
        }
        let entry = map.entry(match op.op {
            LocalOp::UpdateTask(t)
            | LocalOp::MoveTask(t)
            | LocalOp::DeleteTask(t)
            | LocalOp::AssignLabel(t, _)
            | LocalOp::UnassignLabel(t, _) => t,
            LocalOp::CreateTask(_)
            | LocalOp::CreateStack(_)
            | LocalOp::RenameStack(_)
            | LocalOp::MoveStack(_)
            | LocalOp::DeleteStack(_)
            | LocalOp::CreateLabel(_)
            | LocalOp::UpdateLabel(_)
            | LocalOp::DeleteLabel(_)
            | LocalOp::UpdateBoard(_)
            | LocalOp::DeleteBoard(_)
            // Ref-deletes target no local task — no intent to guard.
            | LocalOp::DeleteRemoteCard { .. }
            | LocalOp::DeleteRemoteStack { .. }
            // The flush trigger is board-scoped likewise.
            | LocalOp::FlushBoardAppData(_) => continue,
            // Filtered by the guard arm above; the compiler cannot see it.
            LocalOp::UpdateAppData(_) => unreachable!("handled above"),
        });
        match op.op {
            LocalOp::UpdateTask(_) => {
                let p = entry.or_default();
                p.title = true;
                p.description = true;
                p.duedate = true;
                p.scheduled = true;
                p.done = true;
                p.archived = true;
            }
            LocalOp::MoveTask(_) => {
                entry.or_default().position = true;
            }
            LocalOp::DeleteTask(_) => {
                entry.or_default().deleted = true;
            }
            LocalOp::AssignLabel(_, _) | LocalOp::UnassignLabel(_, _) => {
                entry.or_default().labels = true;
            }
            _ => unreachable!("filtered above"),
        }
    }
    map
}

/// Copies protected field values *and* their write clocks back from the
/// local state over the merge result. Binding fields (`remote`,
/// `remote_seen`) keep the merged values: the observation happened either
/// way.
fn keep_task_intent(merged: &mut Task, local: &Task, task_id: TaskId, p: PendingTaskIntent) {
    if p != PendingTaskIntent::default() {
        tracing::debug!(
            task_id = ?task_id,
            title = p.title,
            description = p.description,
            duedate = p.duedate,
            scheduled = p.scheduled,
            done = p.done,
            archived = p.archived,
            position = p.position,
            labels = p.labels,
            deleted = p.deleted,
            "self-clobber guard protected task fields"
        );
    }
    if p.title {
        merged.title.clone_from(&local.title);
        merged.clocks.title = local.clocks.title;
    }
    if p.description {
        merged.description.clone_from(&local.description);
        merged.clocks.description = local.clocks.description;
    }
    if p.duedate {
        merged.duedate = local.duedate;
        merged.clocks.duedate = local.clocks.duedate;
    }
    if p.scheduled {
        merged.scheduled = local.scheduled;
        merged.clocks.scheduled = local.clocks.scheduled;
    }
    if p.done {
        merged.done = local.done;
        merged.clocks.done = local.clocks.done;
    }
    if p.archived {
        merged.archived = local.archived;
        merged.clocks.archived = local.clocks.archived;
    }
    if p.position {
        merged.stack = local.stack;
        merged.order = local.order;
        merged.clocks.position = local.clocks.position;
    }
    if p.recur {
        merged.recur.clone_from(&local.recur);
        merged.clocks.recur = local.clocks.recur;
    }
    if p.labels {
        // The coarse whole-map restore (the guard protects the OR-set
        // map's entries, not a clock — the second deliberate non-clock
        // guard member beside `reminder_rules`).
        merged
            .label_memberships
            .clone_from(&local.label_memberships);
    }
    if p.deleted {
        merged.deleted = local.deleted;
        merged.clocks.deleted = local.clocks.deleted;
    }
}

/// The stack fields a post-read `RenameStack`/`DeleteStack` protects
/// (`(rename, delete)`).
fn pending_stack_intent(
    outbox: &[PendingOp],
    stack: StackId,
    read_at: DateTime<Utc>,
) -> (bool, bool) {
    let mut rename = false;
    let mut delete = false;
    for op in outbox {
        if op.queued_at <= read_at {
            continue;
        }
        match op.op {
            LocalOp::RenameStack(s) | LocalOp::MoveStack(s) if s == stack => rename = true,
            LocalOp::DeleteStack(s) if s == stack => delete = true,
            _ => {}
        }
    }
    (rename, delete)
}

/// Restores a stack's protected fields from the local state.
fn keep_stack_intent(
    merged: &mut Stack,
    local: &Stack,
    stack_id: StackId,
    rename: bool,
    delete: bool,
) {
    if rename || delete {
        tracing::debug!(
            stack_id = ?stack_id,
            rename,
            delete,
            "self-clobber guard protected stack fields"
        );
    }
    if rename {
        merged.title.clone_from(&local.title);
        merged.order = local.order;
        merged.clocks.title = local.clocks.title;
        merged.clocks.order = local.clocks.order;
    }
    if delete {
        merged.deleted = local.deleted;
        merged.clocks.deleted = local.clocks.deleted;
    }
}

/// The board fields a post-read `UpdateBoard`/`DeleteBoard` op protects
/// (`(update, delete)`).
fn pending_board_intent(
    outbox: &[PendingOp],
    board: BoardId,
    read_at: DateTime<Utc>,
) -> (bool, bool) {
    let mut update = false;
    let mut delete = false;
    for op in outbox {
        if op.queued_at <= read_at {
            continue;
        }
        match op.op {
            LocalOp::UpdateBoard(b) if b == board => update = true,
            LocalOp::DeleteBoard(b) if b == board => delete = true,
            _ => {}
        }
    }
    (update, delete)
}

/// Restores the board's protected fields from the local state.
fn keep_board_intent(
    merged: &mut Board,
    local: &Board,
    board_id: BoardId,
    update: bool,
    delete: bool,
) {
    if update || delete {
        tracing::debug!(
            board_id = ?board_id,
            update,
            delete,
            "self-clobber guard protected board fields"
        );
    }
    if update {
        merged.title.clone_from(&local.title);
        merged.color = local.color.clone();
        merged.archived = local.archived;
        merged.clocks.title = local.clocks.title;
        merged.clocks.color = local.clocks.color;
        merged.clocks.archived = local.clocks.archived;
    }
    if delete {
        merged.deleted = local.deleted;
        merged.clocks.deleted = local.clocks.deleted;
    }
}

/// The label fields a post-read `UpdateLabel`/`DeleteLabel` protects
/// (`(update, delete)`).
fn pending_label_intent(
    outbox: &[PendingOp],
    label: LabelId,
    read_at: DateTime<Utc>,
) -> (bool, bool) {
    let mut update = false;
    let mut delete = false;
    for op in outbox {
        if op.queued_at <= read_at {
            continue;
        }
        match op.op {
            LocalOp::UpdateLabel(l) if l == label => update = true,
            LocalOp::DeleteLabel(l) if l == label => delete = true,
            _ => {}
        }
    }
    (update, delete)
}

/// Restores a label's protected fields from the local state.
fn keep_label_intent(
    merged: &mut Label,
    local: &Label,
    label_id: LabelId,
    update: bool,
    delete: bool,
) {
    if update || delete {
        tracing::debug!(
            label_id = ?label_id,
            update,
            delete,
            "self-clobber guard protected label fields"
        );
    }
    if update {
        merged.title.clone_from(&local.title);
        merged.color.clone_from(&local.color);
        merged.clocks.title = local.clocks.title;
        merged.clocks.color = local.clocks.color;
    }
    if delete {
        merged.deleted = local.deleted;
        merged.clocks.deleted = local.clocks.deleted;
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
///
/// # Panics
///
/// Never on any input the engine can produce: the duplicate collapse's
/// survivor observation is looked up only among the members the
/// detection just matched, so the `expect` there is a construction
/// invariant, not an input-validation surface.
// Still one fixed-order sequence after the R9 decision table was extracted
// into `apply_push_outcomes`; the numbered sections ARE the design.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)] // the injected seams ARE the constructor
pub fn apply_sync_report(
    current: &AppState,
    outbox: &[PendingOp],
    snapshot: &RemoteBoardSnapshot,
    pushes: &[PushOutcome],
    read_at: DateTime<Utc>,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
    connector: &crate::ids::ConnectorId,
    app_data: &crate::messages::AppDataEvidence,
    features: crate::capabilities::DeckFeatures,
    day: crate::days::DayCalendar,
) -> (
    AppState,
    Vec<PersistenceAction>,
    Vec<crate::messages::SystemEvent>,
) {
    let _span = tracing::debug_span!(
        "apply_sync_report",
        outbox_len = outbox.len(),
        pushes = pushes.len(),
        snapshot_stacks = snapshot.stacks.len(),
        snapshot_tasks = snapshot.tasks.len(),
        snapshot_labels = snapshot.labels.len(),
    )
    .entered();
    tracing::trace!(
        board = ?snapshot.board.id,
        read_at = ?read_at,
        now = ?now,
        "sync report entered"
    );
    let mut boards = current.boards.clone();
    let mut stacks = current.stacks.clone();
    let mut tasks = current.tasks.clone();
    let mut labels = current.labels.clone();
    // The report's broadcast-lane verdicts (R3's resurrect carve-out is
    // the only producer today). Symmetric with `actions`: the pipeline
    // plans, the engine forwards.
    let mut system_events: Vec<crate::messages::SystemEvent> = Vec::new();
    // R3's resurrect enqueues; held until the persistence batch exists
    // (the batch is derived from the entity diff in section 7).
    let mut resurrect_enqueues: Vec<PersistenceAction> = Vec::new();

    // ---- 1. Push outcomes (R9*) ------------------------------------
    let mut ctx = RemoteIndex::from_state(current);
    let mut op_results = apply_push_outcomes(
        pushes,
        outbox,
        &mut boards,
        &mut tasks,
        &mut stacks,
        &mut labels,
        &mut ctx,
        now,
        read_at,
        features,
    );

    // The card→local matching map, built from the *post-echo* task map:
    // a create pushed in this very cycle already carries its echo binding
    // here, so the pull in the same cycle must match it instead of
    // re-adopting the card as a duplicate local task (observed by the
    // phase-6 J1 journey against a real server). Built before the board
    // cascade runs — a cascade-finalized binding must not re-match.
    let mut task_by_board_card: HashMap<(RemoteBoardId, RemoteCardId), TaskId> = tasks
        .iter()
        .filter_map(|(id, t)| Some((t.remote?, *id)))
        .map(|(r, id)| ((r.board, r.card), id))
        .collect();

    // ---- 2. Board ---------------------------------------------------
    // Attribution keys on the (connector, remote board) pair: remote ids
    // are only unique per server, and N ≥ 2 means overlapping id spaces
    // across connectors. A same-pair hit merges; a miss adopts under the
    // reporting connector's tag.
    let mut board_ids_by_remote: HashMap<
        (crate::ids::ConnectorId, RemoteBoardId),
        crate::ids::BoardId,
    > = current
        .boards
        .iter()
        .filter_map(|(id, b)| {
            let remote = b.remote.as_ref()?;
            Some(((remote.connector.clone(), remote.board), *id))
        })
        .collect();
    let remote_board = &snapshot.board;
    let board_id = if let Some(id) = board_ids_by_remote
        .get(&(connector.clone(), remote_board.id))
        .copied()
    {
        let local = &boards[&id];
        let mut merged = merge_board(local, remote_board);
        let (update, delete) = pending_board_intent(outbox, id, read_at);
        keep_board_intent(&mut merged, local, id, update, delete);
        boards.insert(id, merged);
        tracing::debug!(rule = RULE_R6, board_id = ?id, "board merged");
        id
    } else {
        let id = ids.new_board_id();
        boards.insert(id, adopt_remote_board(remote_board, connector.clone(), id));
        board_ids_by_remote.insert((connector.clone(), remote_board.id), id);
        tracing::debug!(rule = RULE_R1, board_id = ?id, fresh_id = ?id, "board adopted");
        id
    };
    // Per-connector attribution scope (DF-1's recorded extension): the
    // global ref index is unambiguous only within one server's id
    // space — N ≥ 2 means two connectors can mint identical
    // (board, stack, card, label) ids, and a foreign entry would merge
    // server B's data into server A's board. The report names its
    // board; every ref not owned by that board is invisible to this
    // merge (echo bindings from this cycle's pushes belong to it and
    // survive the prune).
    ctx.stack_by_ref
        .retain(|_, id| stacks[id].board == board_id);
    ctx.task_by_ref
        .retain(|_, id| stacks[&tasks[id].stack].board == board_id);
    ctx.label_by_ref
        .retain(|_, id| labels[id].board == board_id);
    ctx.board_by_ref.retain(|_, id| *id == board_id);
    task_by_board_card.retain(|_, id| stacks[&tasks[id].stack].board == board_id);
    let board_deleted = boards[&board_id].deleted;
    if board_deleted {
        tracing::debug!(rule = RULE_R6, board_id = ?board_id, "board deletion cascade fired");
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

    // ---- 2b. Crash-window stack duplicates (#150, rule R-dup) --------
    // The stack variant with the survivorship inversion (DEC-3): the
    // BOUND stack survives (every replayed card resolves into it and
    // the unbound orphan is provably empty), the orphan gets a
    // ref-delete push. Runs before §3 so the orphan's R1 adoption is
    // suppressed — no local duplicate row is ever created. Clause 2 for
    // stacks is the bound-stack requirement below: an unbound stack
    // never pushed, so it has no crash window.
    let mut dup_enqueues: Vec<PersistenceAction> = Vec::new();
    let mut dup_stack_suppressed: std::collections::BTreeSet<RemoteStackRef> =
        std::collections::BTreeSet::new();
    let dup_stack_creates: Vec<&crate::outbox::PendingOp> = outbox
        .iter()
        .filter(|entry| {
            matches!(entry.op, LocalOp::CreateStack(_))
                && op_results.get(&entry.op_id) != Some(&false)
        })
        .collect();
    for entry in &dup_stack_creates {
        let LocalOp::CreateStack(local_stack_id) = entry.op else {
            continue;
        };
        let Some(survivor) = stacks.get(&local_stack_id) else {
            continue;
        };
        // No binding ⇒ no crash window: the stack never pushed.
        let Some(bound_ref) = survivor.remote else {
            continue;
        };
        for remote in &snapshot.stacks {
            if ctx.stack_by_ref.contains_key(&remote.id)
                || dup_stack_suppressed.contains(&remote.id)
            {
                continue;
            }
            // Clause 1: same normalized title, same board.
            if crate::reconcile::normalized_title(&survivor.title)
                != crate::reconcile::normalized_title(&remote.title)
                || remote.id.board != bound_ref.board
            {
                continue;
            }
            // Clause 3: the window on the orphan's stamp — stacks carry
            // no `created_at` (DEC-1), `last_modified` is the proxy.
            if !crate::merge::dup_window_contains(remote.last_modified, entry.queued_at, now) {
                continue;
            }
            // Clause 4: the create-stack body is title + order.
            if remote.order != survivor.order {
                continue;
            }
            // The never-by-construction guard: an orphan holding a ref
            // that resolves to a live bound task is left alone (Deck's
            // stack delete cascades its cards — DEC-3).
            // The domain snapshot flattens cards out of the stacks, so
            // the orphan's nested cards come from `snapshot.tasks`.
            let contested = snapshot.tasks.iter().any(|card| {
                card.id.board == remote.id.board
                    && card.id.stack == remote.id.stack
                    && ctx
                        .task_by_ref
                        .get(&card.id)
                        .is_some_and(|task_id| tasks[task_id].is_live())
            });
            if contested {
                tracing::debug!(
                    rule = RULE_R_DUP,
                    stack = ?remote.id,
                    "orphan stack holds bound cards; collapse skipped"
                );
                continue;
            }
            dup_stack_suppressed.insert(remote.id);
            dup_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                op_id: ids.new_op_id(),
                op: LocalOp::DeleteRemoteStack {
                    board: board_id,
                    stack: remote.id,
                },
                queued_at: now,
            }));
            tracing::debug!(
                rule = RULE_R_DUP,
                stack_id = ?local_stack_id,
                survivor = ?bound_ref,
                orphan = ?remote.id,
                "crash-window duplicate stack collapses to the bound survivor"
            );
        }
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
                let merged_stack = {
                    let mut merged = merge_stack(local, remote);
                    let (rename, delete) = pending_stack_intent(outbox, id, read_at);
                    keep_stack_intent(&mut merged, local, id, rename, delete);
                    merged
                };
                let now_tombstoned = merged_stack.deleted && !local.deleted;
                if now_tombstoned {
                    newly_tombstoned_stack_refs.push(remote.id.stack);
                }
                stacks.insert(id, merged_stack);
                tracing::debug!(
                    rule = RULE_R6,
                    stack_id = ?id,
                    remote_ts = ?remote.last_modified,
                    newly_tombstoned = now_tombstoned,
                    "stack merged"
                );
            }
        } else if dup_stack_suppressed.contains(&remote.id) {
            tracing::debug!(
                rule = RULE_R_DUP,
                stack = ?remote.id,
                "doomed orphan stack adoption suppressed"
            );
        } else {
            let id = ids.new_stack_id();
            stacks.insert(id, adopt_remote_stack(remote, id, board_id));
            ctx.stack_by_ref.insert(remote.id, id);
            tracing::debug!(rule = RULE_R1, stack_id = ?id, fresh_id = ?id, stack = ?remote.id, "stack adopted");
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
                let mut merged = merge_label(local, remote);
                let (update, delete) = pending_label_intent(outbox, id, read_at);
                keep_label_intent(&mut merged, local, id, update, delete);
                labels.insert(id, merged);
                tracing::debug!(
                    rule = RULE_R6,
                    label_id = ?id,
                    remote_ts = ?remote.last_modified,
                    "label merged"
                );
            }
        } else {
            let id = ids.new_label_id();
            labels.insert(id, adopt_remote_label(remote, id, board_id));
            ctx.label_by_ref.insert(remote.id, id);
            tracing::debug!(rule = RULE_R1, label_id = ?id, fresh_id = ?id, label = ?remote.id, "label adopted");
        }
    }

    // ---- 4b. Crash-window card duplicates (#150, rule R-dup) ---------
    // The create signatures in scope: every CreateTask in the outbox
    // slice whose outcome this cycle was not a failure (a pending op,
    // or the replay's echo — the crash signature cannot exist without
    // our own create). No signatures ⇒ the pass cannot fire.
    let mut dup_suppressed: std::collections::BTreeSet<RemoteCardRef> =
        std::collections::BTreeSet::new();
    let dup_creates: Vec<CreateSignature> = outbox
        .iter()
        .filter(|entry| {
            matches!(entry.op, LocalOp::CreateTask(_))
                && op_results.get(&entry.op_id) != Some(&false)
        })
        .map(|entry| {
            let echoed = pushes.iter().any(|o| {
                o.op == entry.op_id
                    && matches!(
                        &o.result,
                        PushResult::Applied {
                            echo: Some(RemoteEcho::Task(_)),
                        }
                    )
            });
            CreateSignature {
                task: match entry.op {
                    LocalOp::CreateTask(task) => task,
                    _ => unreachable!("filtered above"),
                },
                op: entry.op_id,
                queued_at: entry.queued_at,
                echoed_this_cycle: echoed,
            }
        })
        .collect();
    if !dup_creates.is_empty() {
        // The bound card's observation, when this cycle's evidence carries
        // one: the snapshot listing first, this cycle's task echoes second.
        let mut card_observations: HashMap<RemoteCardRef, &RemoteTask> =
            snapshot.tasks.iter().map(|task| (task.id, task)).collect();
        for outcome in pushes {
            if let PushResult::Applied {
                echo: Some(RemoteEcho::Task(echo)),
            } = &outcome.result
            {
                card_observations.insert(echo.id, echo);
            }
        }
        for sig in &dup_creates {
            let Some(local) = tasks.get(&sig.task) else {
                continue;
            };
            let bound_obs = local
                .remote
                .and_then(|ref_| card_observations.get(&ref_).copied());
            // Clause verdicts per unbound snapshot card, then one group
            // formation (DEC-10's multi-orphan generalization).
            let mut matched: Vec<(RemoteCardRef, chrono::DateTime<chrono::Utc>)> = Vec::new();
            let mut satisfied_op = None;
            for candidate in &snapshot.tasks {
                if task_by_board_card.contains_key(&(candidate.id.board, candidate.id.card))
                    || dup_suppressed.contains(&candidate.id)
                {
                    continue;
                }
                if let Some(verdict) = detect_crash_window_duplicates(
                    candidate,
                    bound_obs,
                    &tasks,
                    &stacks,
                    std::slice::from_ref(sig),
                    now,
                    features.card_startdate,
                ) {
                    matched.push((candidate.id, candidate.created_at));
                    if satisfied_op.is_none() {
                        satisfied_op = verdict.satisfied_op;
                    }
                }
            }
            if matched.is_empty() {
                continue;
            }
            let collapse = collapse_for_task(
                sig.task,
                bound_obs.map(|obs| (obs.id, obs.created_at)),
                matched,
                satisfied_op,
            );
            if collapse.rebind {
                // The survivor came from this cycle's evidence by
                // construction (the detection matched on it).
                let survivor_obs = card_observations
                    .get(&collapse.survivor)
                    .copied()
                    .expect("the survivor was matched from this cycle's evidence");
                let old_bound = tasks[&sig.task].remote;
                let rebinded = rebind_task_to_survivor(&tasks[&sig.task], survivor_obs);
                tasks.insert(sig.task, rebinded);
                // The doomed old binding must not drive §5's merge for
                // its card: the task no longer points there.
                if let Some(old) = old_bound {
                    task_by_board_card.remove(&(old.board, old.card));
                }
            }
            // DEC-6: the pending create's effect exists server-side (the
            // survivor IS the first POST's card) — completes true; a
            // false would stamp the board Failed.
            if let Some(op) = collapse.satisfied_op {
                op_results.insert(op, true);
            }
            for doomed in &collapse.doomed {
                dup_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                    op_id: ids.new_op_id(),
                    op: LocalOp::DeleteRemoteCard {
                        board: board_id,
                        card: *doomed,
                    },
                    queued_at: now,
                }));
            }
            dup_suppressed.insert(collapse.survivor);
            dup_suppressed.extend(collapse.doomed.iter().copied());
            tracing::debug!(
                rule = RULE_R_DUP,
                task_id = ?sig.task,
                survivor = ?collapse.survivor,
                rebind = collapse.rebind,
                doomed = collapse.doomed.len(),
                satisfied_op = ?collapse.satisfied_op,
                "crash-window duplicate collapse applied"
            );
        }
    }

    // ---- 5. Tasks (R1/R4/R5) ----------------------------------------
    // Task identity across pulls is (board, card): Deck card ids are unique
    // per board and a cross-stack move (remote, or a just-pushed local one
    // whose binding update rides the next pull) changes the ref's stack
    // component only. Matching by the full ref would read every move as
    // delete+create and tombstone the moved task.
    let mut newly_adopted: HashMap<(RemoteBoardId, RemoteCardId), TaskId> = HashMap::new();
    let task_intents = pending_task_intents(outbox, read_at);
    for remote in &snapshot.tasks {
        let key = (remote.id.board, remote.id.card);
        let bound = task_by_board_card
            .get(&key)
            .copied()
            .or_else(|| newly_adopted.get(&key).copied());
        if let Some(id) = bound {
            // R4 fast path: unchanged remote keeps local entirely.
            let was_deleted = tasks[&id].deleted;
            let local = tasks[&id].clone();
            if remote_wins(
                remote.last_modified,
                local.remote_seen.unwrap_or_else(utc_min),
            ) {
                let mut merged = merge_task(&local, remote, &ctx, features.card_startdate);
                keep_task_intent(
                    &mut merged,
                    &local,
                    id,
                    task_intents.get(&id).copied().unwrap_or_default(),
                );
                let resurrected = was_deleted && !merged.deleted;
                tasks.insert(id, merged);
                tracing::debug!(
                    rule = RULE_R4,
                    task_id = ?id,
                    remote_ts = ?remote.last_modified,
                    resurrected,
                    "task merged"
                );
                // R5 resurrect: the pending DeleteTask op is dropped — but
                // only one the cycle actually saw. A delete queued after
                // the read is intent the push never carried; resurrecting
                // over it would drop it silently.
                if resurrected {
                    for other in outbox {
                        if !op_results.contains_key(&other.op_id)
                            && other.op == LocalOp::DeleteTask(id)
                            && other.queued_at <= read_at
                        {
                            op_results.insert(other.op_id, false);
                            tracing::debug!(
                                rule = RULE_R5,
                                task_id = ?id,
                                op_id = ?other.op_id,
                                "resurrected task delete op dropped"
                            );
                        }
                    }
                }
            }
        } else if dup_suppressed.contains(&remote.id) {
            // R1 suppression (DEC-2): the collapse's verdicts skip
            // before any id burn or row insert — no local duplicate row
            // is ever created, in memory or storage.
            tracing::debug!(
                rule = RULE_R_DUP,
                card = ?remote.id,
                "duplicate adoption suppressed"
            );
        } else {
            // Deferred adoption (None) when the remote stack has no local
            // binding: the card joins a later cycle, once its stack exists
            // (merge.rs unmapped-stack strategy).
            let id = ids.new_task_id();
            if let Some(adopted) = adopt_remote_task(remote, id, &ctx) {
                tasks.insert(id, adopted);
                ctx.task_by_ref.insert(remote.id, id);
                newly_adopted.insert(key, id);
                tracing::debug!(rule = RULE_R1, task_id = ?id, fresh_id = ?id, card = ?remote.id, "task adopted");
            }
        }
    }

    // ---- 5b. The sidecar fold (D-S3) --------------------------------
    // Only `Document` evidence folds (a failed cycle folds nothing; the
    // unreadable fences fold nothing louder). Position: after the
    // task-merge loop (the fold's `merged` input is the post-merge
    // truth, guard flags already applied to the native fields) and
    // before R3 — the fold feeds R3 nothing (no presence facts).
    let mut rule_actions: Vec<PersistenceAction> = Vec::new();
    let mut folded_rules: Option<
        std::collections::BTreeMap<crate::ids::ReminderRuleId, crate::reminders::ReminderRule>,
    > = None;
    let mut dismissal_actions: Vec<PersistenceAction> = Vec::new();
    let mut folded_dismissals: Option<
        std::collections::BTreeSet<crate::reminders::ReminderDismissal>,
    > = None;
    let mut tracking_actions: Vec<PersistenceAction> = Vec::new();
    let mut habit_actions: Vec<PersistenceAction> = Vec::new();
    let mut folded_habits: Option<crate::habit::HabitState> = None;
    let mut folded_tracking: Option<crate::tracking::TrackingState> = None;
    if let crate::messages::AppDataEvidence::Document(document) = app_data {
        let task_intents = pending_task_intents(outbox, read_at);
        for remote in &snapshot.tasks {
            let Some(&task_id) = task_by_board_card.get(&(remote.id.board, remote.id.card)) else {
                continue; // an unresolvable card never reaches the fold
            };
            let card_key = remote.id.card.get().to_string();
            let local = &tasks[&task_id];
            let intent = task_intents.get(&task_id).copied().unwrap_or_default();
            if let Some(entries) = &document.sections.tasks
                && let Some(entry) = entries.get(&card_key)
            {
                let mut merged = local.clone();
                fold_task_app_data(&mut merged, local, entry, intent, features.card_startdate);
                if merged != *local {
                    tasks.insert(task_id, merged);
                }
            }
            if let Some(entries) = &document.sections.reminder_rules
                && let Some(entry) = entries.get(&card_key)
            {
                let rules = folded_rules.get_or_insert_with(|| current.reminders.rules.clone());
                let changes = fold_reminder_rules(rules, task_id, entry, intent.reminder_rules);
                for rule in changes.upserts {
                    rule_actions.push(PersistenceAction::UpsertReminderRule(rule));
                }
                for rule_id in changes.deletes {
                    rule_actions.push(PersistenceAction::DeleteReminderRule(rule_id));
                }
            }
            // The dismissals fold rides the same pass, AFTER the rules
            // fold (design D-9's fold-order property: a rule adopted in
            // this cycle is immediately suppressed by already-present
            // dismissal rows, before the local device can fire once).
            // Guard-immune by construction: no intent flag is consulted
            // — a dismissal is an append-only fact, never edited.
            if let Some(entries) = &document.sections.dismissals
                && let Some(entry) = entries.get(&card_key)
            {
                let dismissals =
                    folded_dismissals.get_or_insert_with(|| current.reminders.dismissals.clone());
                for dismissal in fold_reminder_dismissals(dismissals, entry) {
                    dismissal_actions.push(PersistenceAction::RecordReminderDismissal(dismissal));
                }
            }
        }
        // The time-entries fold is board-level: once per document, AFTER
        // the per-card rules/dismissals loop (design D-T12's fold-order
        // property). Resolution uses the pre-R3 card map, so an entry
        // whose card R3 tombstones in this same cycle still folds —
        // history outlives the card; the task lands soft-deleted with
        // the entry's reference intact.
        if let Some(rows) = &document.sections.time_entries {
            let board_cards: std::collections::BTreeMap<String, TaskId> = task_by_board_card
                .iter()
                .filter(|((board, _), _)| *board == remote_board.id)
                .map(|((_, card), id)| (card.get().to_string(), *id))
                .collect();
            let tracking = folded_tracking.get_or_insert_with(|| current.tracking.clone());
            let changes = crate::merge::fold_time_entries(tracking, rows, &board_cards);
            for entry in changes.upserts {
                tracking_actions.push(PersistenceAction::UpsertTimeEntry(entry));
            }
            for entry_id in changes.deletes {
                tracking_actions.push(PersistenceAction::DeleteTimeEntry(entry_id));
            }
        }
        // The habit folds are board-level too: once per document, right
        // after the time-entries fold (§5b's order). Both sections fold
        // against the same copy-on-write state; no card map is needed —
        // habits carry no card hop at all.
        if document.sections.habits.is_some() || document.sections.habit_log.is_some() {
            let habits = folded_habits.get_or_insert_with(|| current.habits.clone());
            if let Some(rows) = &document.sections.habits {
                let changes = crate::merge::fold_habits(habits, rows, board_id);
                for habit in changes.upserts {
                    habit_actions.push(PersistenceAction::UpsertHabit(habit));
                }
            }
            if let Some(rows) = &document.sections.habit_log {
                let changes = crate::merge::fold_habit_log(habits, rows);
                for (key, entry) in changes.records {
                    habit_actions.push(PersistenceAction::RecordHabitLog(key, entry));
                }
            }
        }
    }

    // ---- 5c. Series fold dedup (§3.6, D-C7, rule R-recur-dup) --------
    // Position: after §5b (a twin adopted this cycle learns its rule
    // through the fold — detecting earlier misses the canonical
    // adoption-cycle race) and before §6 (the neutralized rows are
    // ref-less by then, so R3's bound-ref listing never sees their stale
    // bindings; the doomed cards' presence is decided by the
    // ref-deletes, not by R3). §4b and §5c are disjoint trigger classes:
    // §4b needs this device's create signature, §5c needs only two local
    // rows sharing `(series, reference date)` — the late-discovered race
    // is exactly §5c's case.
    for verdict in crate::merge::detect_series_twins(&tasks, day) {
        let (series, date, original) = {
            let before = &tasks[&verdict.survivor];
            (
                before.recur.as_ref().map(|rule| rule.series),
                crate::recur::roll_reference_date(before, day),
                before.clone(),
            )
        };
        let mut survivor = original.clone();
        let mut ref_deletes = 0usize;
        let mut settled_ops = 0usize;
        for doomed_id in &verdict.doomed {
            let doomed = tasks[doomed_id].clone();
            let merged = crate::merge::series_survivor_merge(&survivor, &doomed);
            if merged != survivor {
                survivor = merged;
            }
            // The ref-less tombstone (the `reconcile` neutralization
            // shape, the doomed row as the bound argument) with the
            // tombstone clock floored at `now`: it LWW-wins every older
            // copy of the row.
            let mut absorbed = crate::reconcile::neutralized_task(&survivor, &doomed);
            absorbed.clocks.deleted = absorbed.clocks.deleted.max(now);
            tasks.insert(*doomed_id, absorbed);
            // The card stops existing server-side, so no later pull can
            // re-adopt the twin (the cross-cycle suppression; within the
            // drain window a re-adoption re-collapses — the pass is
            // stateless and idempotent, DEC-8).
            if let Some(r) = doomed.remote {
                dup_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                    op_id: ids.new_op_id(),
                    op: LocalOp::DeleteRemoteCard {
                        board: board_id,
                        card: r,
                    },
                    queued_at: now,
                }));
                ref_deletes += 1;
            }
            // Every pending op targeting the doomed twin is resolved,
            // never rejected: `false` would dead-letter and stamp the
            // board `Failed` (DEC-6), and the op's effect must never
            // exist. No read-boundary guard (DEC-7): the doom verdict is
            // a permanent state-level decision — a create op queued
            // after `read_at` that survived would mint the twin's card
            // next cycle and restart the race. Ops a push outcome
            // already settled keep their wire truth.
            for other in outbox {
                if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, *doomed_id)
                {
                    op_results.insert(other.op_id, true);
                    settled_ops += 1;
                }
            }
        }
        let changed = survivor != original;
        if changed {
            // Lane ops for the survivor, unconditionally on change
            // (DEC-10): the lanes' push planners full-send and coalesce.
            dup_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                op_id: ids.new_op_id(),
                op: LocalOp::UpdateTask(survivor.id),
                queued_at: now,
            }));
            dup_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                op_id: ids.new_op_id(),
                op: LocalOp::UpdateAppData(survivor.id),
                queued_at: now,
            }));
            tasks.insert(survivor.id, survivor);
        }
        // One applied-verdict trace per group, typed fields only (the
        // grouping itself is pure and silent). No `SystemEvent`, no UI.
        tracing::debug!(
            rule = crate::merge::RULE_R_RECUR_DUP,
            series = ?series,
            date = ?date,
            survivor = ?verdict.survivor,
            doomed = verdict.doomed.len(),
            ref_deletes,
            settled_ops,
            "recurring-series twin collapse applied"
        );
    }

    // ---- 6. Presence reconciliation (R3) ----------------------------
    // Bound-but-absent entities are gone server-side (snapshot
    // completeness contract). Cards: delete-wins fallback — unless the
    // task holds unsynced user work (#130's resurrect carve-out).
    // Cascade territory (a dead stack) stays tombstone-wins: the
    // present-stack set is derived up front so the carve-out's guard
    // can consult it.
    // The listing is read *before* the cycle's pushes (fetch-before-
    // write), so a card this cycle's create echo carried is absent from
    // it by construction. The echo is fresher presence evidence than
    // the listing — without this, R3 would judge a freshly created card
    // "absent" in its very first cycle (a tombstone before this chunk;
    // a spurious resurrect after it).
    let mut present_refs: std::collections::BTreeSet<(RemoteBoardId, RemoteCardId)> = snapshot
        .tasks
        .iter()
        .map(|t| (t.id.board, t.id.card))
        .collect();
    for outcome in pushes {
        if let PushResult::Applied {
            echo: Some(RemoteEcho::Task(echo)),
        } = &outcome.result
        {
            present_refs.insert((echo.id.board, echo.id.card));
        }
    }
    let present_stack_refs: std::collections::BTreeSet<RemoteStackRef> =
        snapshot.stacks.iter().map(|s| s.id).collect();
    let reminder_rules = folded_rules.as_ref().unwrap_or(&current.reminders.rules);
    let bound_task_refs: Vec<(TaskId, (RemoteBoardId, RemoteCardId))> = tasks
        .iter()
        .filter_map(|(id, t)| {
            let r = t.remote?;
            Some((*id, (r.board, r.card)))
        })
        .collect();
    for (id, remote_ref) in bound_task_refs {
        // No `!local.deleted` guard here, unlike stacks/labels below: a
        // tombstoned task that kept its binding must still have it cleared
        // (R9c cache-lag safety).
        if !present_refs.contains(&remote_ref) {
            let local = &tasks[&id];
            let resurrect = local.is_live()
                && local.remote.is_some_and(|r| {
                    present_stack_refs.contains(&RemoteStackRef {
                        board: r.board,
                        stack: r.stack,
                    })
                })
                && crate::merge::has_unsynced_work(local, reminder_rules);
            if resurrect {
                // The carve-out (#130): the work outlives the card. The
                // binding trio clears together — a surviving stale
                // baseline would corrupt the next merge's diff — and
                // exactly one `CreateTask` re-enqueues; the next push
                // mints a fresh remote card and the echo rebinds.
                let mut resurrected = local.clone();
                resurrected.remote = None;
                resurrected.remote_seen = None;
                resurrected.remote_baseline = None;
                tasks.insert(id, resurrected);
                let mut cancelled_ops = 0;
                for other in outbox {
                    if !op_results.contains_key(&other.op_id)
                        && op_targets_task(&other.op, id)
                        // The document lane rides: the flush runs last
                        // against post-echo card ids and an unbound entry
                        // simply rides to the next cycle — cancelling it
                        // would orphan the task's sidecar content.
                        && !matches!(other.op, crate::outbox::LocalOp::UpdateAppData(_))
                    {
                        op_results.insert(other.op_id, false);
                        cancelled_ops += 1;
                    }
                }
                resurrect_enqueues.push(PersistenceAction::EnqueueOp(PendingOp {
                    op_id: ids.new_op_id(),
                    op: crate::outbox::LocalOp::CreateTask(id),
                    queued_at: now,
                }));
                system_events.push(crate::messages::SystemEvent::TaskResurrected(id));
                tracing::debug!(
                    rule = RULE_R3,
                    task_id = ?id,
                    card = ?remote_ref,
                    cancelled_ops,
                    outcome = "resurrected",
                    "task absence resurrected unsynced work"
                );
            } else {
                tasks.insert(id, tombstone_task(local, now));
                let mut cancelled_ops = 0;
                for other in outbox {
                    if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, id) {
                        op_results.insert(other.op_id, false);
                        cancelled_ops += 1;
                    }
                }
                tracing::debug!(
                    rule = RULE_R3,
                    task_id = ?id,
                    card = ?remote_ref,
                    cancelled_ops,
                    "task absence tombstoned"
                );
            }
        }
    }
    let bound_stack_refs: Vec<(StackId, RemoteStackRef)> = stacks
        .iter()
        .filter_map(|(id, s)| Some((*id, s.remote?)))
        .collect();
    for (id, remote_ref) in bound_stack_refs {
        let local = &stacks[&id];
        if !present_stack_refs.contains(&remote_ref) && !local.deleted {
            stacks.insert(id, tombstone_stack(local, now));
            newly_tombstoned_stack_refs.push(remote_ref.stack);
            tracing::debug!(rule = RULE_R3, stack_id = ?id, stack = ?remote_ref, "stack absence tombstoned");
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
            // Entity-death dominance (ADR 0004's labels amendment, D-2e):
            // a dead label's live memberships are force-removed — the
            // membership cannot exist server-side, and its queued
            // assignments die with the entity (the push would 404).
            // `Removed` entries keep their older stamps (max of two
            // removals).
            let removal = crate::entities::MembershipState::Removed { at: now };
            let dominated: Vec<TaskId> = tasks
                .iter()
                .filter(|(_, t)| {
                    t.is_live()
                        && matches!(
                            t.label_memberships.get(&id),
                            Some(crate::entities::MembershipState::Added { .. })
                        )
                })
                .map(|(task_id, _)| *task_id)
                .collect();
            for task_id in &dominated {
                let mut merged = tasks[task_id].clone();
                let entry = merged.label_memberships.remove(&id);
                merged
                    .label_memberships
                    .insert(id, crate::merge::merge_membership_opt(entry, removal));
                tasks.insert(*task_id, merged);
            }
            let mut cancelled_ops = 0usize;
            for other in outbox {
                let targets = matches!(
                    other.op,
                    LocalOp::AssignLabel(_, l) | LocalOp::UnassignLabel(_, l) if l == id
                );
                if targets && !op_results.contains_key(&other.op_id) {
                    op_results.insert(other.op_id, false);
                    cancelled_ops += 1;
                }
            }
            tracing::debug!(
                rule = RULE_R3,
                label_id = ?id,
                label = ?remote_ref,
                dominated_tasks = dominated.len(),
                cancelled_ops,
                "label absence tombstoned"
            );
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
            tracing::debug!(
                rule = RULE_R6,
                task_id = ?task_id,
                stack = ?stack_remote_num,
                "task cascade tombstoned"
            );
        }
    }

    // ---- 7. Sync-status transition + persistence batch ---------------
    let mut actions = actions_for_changed_entities(current, &boards, &stacks, &labels, &tasks);
    // The rules fold's deltas ride the same batch: the persistence view
    // and the memory view below are the same deltas by construction.
    actions.extend(rule_actions);
    actions.extend(dismissal_actions);
    actions.extend(tracking_actions);
    actions.extend(habit_actions);
    // R3's resurrect creates ride last: the re-planning decided in
    // section 6, the batch materializes here. The duplicate collapse's
    // ref-deletes ride beside them (same shape: decided mid-pass,
    // materialized on the final batch).
    actions.extend(resurrect_enqueues);
    actions.extend(dup_enqueues);
    // Outbox order, not the HashMap's: equal inputs must yield equal
    // batches (the convergence property; the map is a lookup index).
    for entry in outbox {
        if let Some(completed) = op_results.get(&entry.op_id) {
            actions.push(if *completed {
                PersistenceAction::CompleteOp(entry.op_id)
            } else {
                PersistenceAction::FailOp(entry.op_id)
            });
        }
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
    // The report's board row transitions; every other row carries over.
    let mut sync = current.sync.clone();
    sync.set_stored(board_id, phase, Some(now));
    // The sidecar health and the connector's capability flags ride the
    // report's board row (memory-only; `append_status_if_changed`
    // compares only phase/last_success, so this never touches the
    // persisted batch).
    if let Some(row) = sync.get_mut(board_id) {
        row.sidecar = crate::state::derive_sidecar_health(app_data);
        row.features = features;
    }
    let mut state = AppState {
        boards,
        stacks,
        tasks,
        labels,
        // No reminder data enters the *native* sync path: the rules
        // section rode the app-data document lane (the fold above);
        // everything else carries through untouched.
        reminders: crate::reminders::ReminderState {
            rules: folded_rules.unwrap_or_else(|| current.reminders.rules.clone()),
            dismissals: folded_dismissals.unwrap_or_else(|| current.reminders.dismissals.clone()),
            ..current.reminders.clone()
        },
        tracking: folded_tracking.unwrap_or_else(|| current.tracking.clone()),
        // Definitions and log rows ride their own sidecar sections; a
        // document carrying them folds into this copy (§5b), an absent
        // one carries the local truth through untouched.
        habits: folded_habits.unwrap_or_else(|| current.habits.clone()),
        // The undo projection is derived from the persisted journal at
        // publish (core.rs); the pipeline's working copy carries the
        // current one through untouched.
        undo: current.undo.clone(),
        sync,
        // Sync ingestion is an engine mutation from a foreign source:
        // stamp it like a local command would be.
        last_updated: Some(now),
    };
    // The projection is a display derivation over the ops that remain
    // queued after this cycle's push outcomes (the engine re-derives
    // after its own interpret, so these values are advisory).
    crate::state::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &remaining_outbox(outbox, &op_results),
        &mut state.sync,
    );
    log_report_applied(
        &state,
        &actions,
        &op_results,
        remaining_ops,
        "sync report applied",
    );
    (state, actions, system_events)
}

/// The outbox as it will look once this cycle's outcomes are applied:
/// the ops the cycle settled are dropped, the rest keep their order.
fn remaining_outbox(outbox: &[PendingOp], op_results: &HashMap<OpId, bool>) -> Vec<PendingOp> {
    outbox
        .iter()
        .filter(|entry| !op_results.contains_key(&entry.op_id))
        .cloned()
        .collect()
}

/// The closing summary both pipeline entries emit — the only event a
/// steady-state cycle produces beyond the entry span and trace entry.
/// `message` names the entry ("sync report applied" / "push report
/// applied"); every field is a typed fact.
fn log_report_applied(
    state: &AppState,
    actions: &[PersistenceAction],
    op_results: &HashMap<OpId, bool>,
    remaining_ops: usize,
    message: &'static str,
) {
    tracing::debug!(
        actions = actions.len(),
        ops_completed = op_results.values().filter(|completed| **completed).count(),
        ops_failed = op_results.values().filter(|completed| !**completed).count(),
        remaining_ops,
        phase = ?state.sync.aggregate().phase,
        "{}",
        message,
    );
}

/// The entity-diff half of the persistence batch: one upsert per entity
/// whose merged value differs from `current`.
fn actions_for_changed_entities(
    current: &AppState,
    boards: &BTreeMap<crate::ids::BoardId, Board>,
    stacks: &BTreeMap<StackId, Stack>,
    labels: &BTreeMap<LabelId, Label>,
    tasks: &BTreeMap<TaskId, Task>,
) -> Vec<PersistenceAction> {
    let mut actions = Vec::new();
    for (id, board) in boards {
        if current.boards.get(id) != Some(board) {
            actions.push(PersistenceAction::UpsertBoard(board.clone()));
        }
    }
    for (id, stack) in stacks {
        if current.stacks.get(id) != Some(stack) {
            actions.push(PersistenceAction::UpsertStack(stack.clone()));
        }
    }
    for (id, label) in labels {
        if current.labels.get(id) != Some(label) {
            actions.push(PersistenceAction::UpsertLabel(label.clone()));
        }
    }
    for (id, task) in tasks {
        if current.tasks.get(id) != Some(task) {
            actions.push(PersistenceAction::UpsertTask(task.clone()));
        }
    }
    actions
}

/// The pipeline's push-only composition (steps 1 + 7): consumed by the
/// engine for `SyncReport::Failed` reports that carry successful push
/// outcomes (phase 4 decision 3 — evidence-then-verdict). The completed
/// pushes land (ops complete, create echoes bind) and the status
/// transitions, but **no** board/stack/label/task merge, presence
/// reconciliation, or cascade runs — an empty snapshot would tombstone
/// everything, and this function exists precisely to avoid that.
///
/// `last_success` advances to `now` only when no outcome was rejected;
/// rejections keep the previous last success (a rejected write is not a
/// successful sync). `ids` is currently unused and reserved so the
/// signature matches [`apply_sync_report`]'s (a future push path that
/// adopts remotely-created entities would need it).
pub fn apply_push_report(
    current: &AppState,
    outbox: &[PendingOp],
    pushes: &[PushOutcome],
    read_at: DateTime<Utc>,
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
    features: crate::capabilities::DeckFeatures,
) -> (AppState, Vec<PersistenceAction>) {
    let _ = ids;
    let _span = tracing::debug_span!(
        "apply_push_report",
        outbox_len = outbox.len(),
        pushes = pushes.len(),
    )
    .entered();
    tracing::trace!(
        read_at = ?read_at,
        now = ?now,
        "push report entered"
    );
    let mut boards = current.boards.clone();
    let mut stacks = current.stacks.clone();
    let mut tasks = current.tasks.clone();
    let mut labels = current.labels.clone();

    let mut ctx = RemoteIndex::from_state(current);
    let op_results = apply_push_outcomes(
        pushes,
        outbox,
        &mut boards,
        &mut tasks,
        &mut stacks,
        &mut labels,
        &mut ctx,
        now,
        read_at,
        features,
    );

    let mut actions = actions_for_changed_entities(current, &boards, &stacks, &labels, &tasks);
    // Outbox order, not the HashMap's: equal inputs must yield equal
    // batches (the convergence property; the map is a lookup index).
    for entry in outbox {
        if let Some(completed) = op_results.get(&entry.op_id) {
            actions.push(if *completed {
                PersistenceAction::CompleteOp(entry.op_id)
            } else {
                PersistenceAction::FailOp(entry.op_id)
            });
        }
    }

    let remaining_ops = outbox.len().saturating_sub(op_results.len());
    let any_rejected = pushes
        .iter()
        .any(|outcome| matches!(outcome.result, PushResult::Rejected { .. }));
    let dead_lettered = op_results.values().any(|completed| !completed);
    let phase = if dead_lettered {
        SyncPhase::Failed {
            last_error: SyncErrorKind::BadRequest,
        }
    } else {
        SyncPhase::Idle
    };
    // The cycle has no snapshot, so board identity comes from the pushed
    // ops' attribution: exactly the boards this cycle wrote to transition.
    // A rejection is not a successful sync — each board keeps its own
    // last success instead of advancing it.
    let affected: std::collections::BTreeSet<crate::ids::BoardId> = outbox
        .iter()
        .filter(|entry| op_results.contains_key(&entry.op_id))
        .filter_map(|entry| crate::state::attributed_board(&entry.op, &stacks, &tasks, &labels))
        .collect();
    let mut sync = current.sync.clone();
    for board in &affected {
        let carried = sync.get(*board).and_then(|row| row.last_success);
        sync.set_stored(
            *board,
            phase,
            if any_rejected { carried } else { Some(now) },
        );
        // Each affected board's row carries *its* connector's flags
        // (a board's row is only written by reports for that board);
        // the sidecar verdict is not re-derived here — a push-only
        // cycle produces no fresh evidence, the last one stands.
        if let Some(row) = sync.get_mut(*board) {
            row.features = features;
        }
    }
    let mut state = AppState {
        boards,
        stacks,
        tasks,
        labels,
        // No reminder data enters the *native* push path; rules ride
        // the app-data document lane (a push-only cycle folds nothing).
        reminders: current.reminders.clone(),
        // No tracking data enters the sync path: entries ride their own
        // chunk's lane; history carries through untouched.
        tracking: current.tracking.clone(),
        // No habit data enters the push path: definitions and log rows
        // ride chunk DR-6/6's sidecar sections; both carry through
        // untouched.
        habits: current.habits.clone(),
        // The undo projection is derived from the persisted journal at
        // publish (core.rs); the pipeline's working copy carries the
        // current one through untouched.
        undo: current.undo.clone(),
        sync,
        last_updated: Some(now),
    };
    crate::state::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &remaining_outbox(outbox, &op_results),
        &mut state.sync,
    );
    log_report_applied(
        &state,
        &actions,
        &op_results,
        remaining_ops,
        "push report applied",
    );
    (state, actions)
}

/// Executes the push outcomes (R9) against the current entity maps and
/// returns the op bookkeeping: `op_id -> true` (completed) / `false`
/// (failed/cancelled); ops absent from the map stay queued untouched.
#[allow(clippy::too_many_lines)] // one R9 decision table; splitting hides the cases
#[allow(clippy::too_many_arguments)] // the R9 inputs are exactly these
fn apply_push_outcomes(
    pushes: &[PushOutcome],
    outbox: &[PendingOp],
    boards: &mut BTreeMap<BoardId, Board>,
    tasks: &mut BTreeMap<TaskId, Task>,
    stacks: &mut BTreeMap<StackId, Stack>,
    labels: &mut BTreeMap<LabelId, Label>,
    ctx: &mut RemoteIndex,
    now: DateTime<Utc>,
    read_at: DateTime<Utc>,
    features: crate::capabilities::DeckFeatures,
) -> HashMap<OpId, bool> {
    let mut op_results: HashMap<OpId, bool> = HashMap::new();
    // The self-clobber guard applies to echo adoption too: an echo carries
    // the plan-time values of every shape field, so an edit queued after
    // the cycle's read must survive it exactly like the pull's stamps.
    let task_intents = pending_task_intents(outbox, read_at);
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
                        tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, task_id = ?id, "pushed task delete finalized");
                        continue;
                    }
                    LocalOp::DeleteStack(id) => {
                        if let Some(local) = stacks.get(&id) {
                            let finalized = tombstone_stack(local, now);
                            stacks.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, stack_id = ?id, "pushed stack delete finalized");
                        continue;
                    }
                    LocalOp::DeleteLabel(id) => {
                        if let Some(local) = labels.get(&id) {
                            let finalized = tombstone_label(local, now);
                            labels.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, label_id = ?id, "pushed label delete finalized");
                        continue;
                    }
                    // Ref-deletes (#150): no local row exists, so nothing
                    // finalizes — the echo is None anyway; the op completes.
                    LocalOp::DeleteRemoteCard { .. } | LocalOp::DeleteRemoteStack { .. } => {
                        op_results.insert(outcome.op, true);
                        tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, "pushed ref-delete completed");
                        continue;
                    }
                    LocalOp::CreateTask(_)
                    | LocalOp::UpdateTask(_)
                    | LocalOp::MoveTask(_)
                    | LocalOp::CreateStack(_)
                    | LocalOp::RenameStack(_)
                    | LocalOp::MoveStack(_)
                    | LocalOp::CreateLabel(_)
                    | LocalOp::UpdateLabel(_)
                    | LocalOp::AssignLabel(..)
                    | LocalOp::UnassignLabel(..)
                    | LocalOp::UpdateBoard(_)
                    | LocalOp::UpdateAppData(_)
                    | LocalOp::FlushBoardAppData(_)
                    | LocalOp::DeleteBoard(_) => {}
                }
                match echo {
                    Some(RemoteEcho::Task(echo)) => {
                        if let LocalOp::CreateTask(id) | LocalOp::UpdateTask(id) = op.op
                            && let Some(local) = tasks.get(&id)
                        {
                            let mut merged =
                                adopt_task_after_push(local, echo, ctx, features.card_startdate);
                            keep_task_intent(
                                &mut merged,
                                local,
                                id,
                                task_intents.get(&id).copied().unwrap_or_default(),
                            );
                            tasks.insert(id, merged);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            task_id = ?op_task_id(&op.op),
                            echo = ?echo.id,
                            "task push echo adopted"
                        );
                    }
                    Some(RemoteEcho::Stack(echo)) => {
                        if let LocalOp::CreateStack(id)
                        | LocalOp::RenameStack(id)
                        | LocalOp::MoveStack(id) = op.op
                            && let Some(local) = stacks.get(&id)
                        {
                            let mut merged = adopt_stack_after_push(local, echo);
                            let (rename, delete) = pending_stack_intent(outbox, id, read_at);
                            keep_stack_intent(&mut merged, local, id, rename, delete);
                            stacks.insert(id, merged);
                            ctx.stack_by_ref.insert(echo.id, id);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            stack_id = ?op_stack_id(&op.op),
                            echo = ?echo.id,
                            "stack push echo adopted"
                        );
                    }
                    Some(RemoteEcho::Board(echo)) => {
                        if let LocalOp::UpdateBoard(id) = op.op
                            && let Some(local) = boards.get(&id)
                        {
                            let mut merged = adopt_board_after_push(local, echo);
                            let (update, delete) = pending_board_intent(outbox, id, read_at);
                            keep_board_intent(&mut merged, local, id, update, delete);
                            boards.insert(id, merged);
                            ctx.board_by_ref.insert(echo.id, id);
                        }
                        if let LocalOp::DeleteBoard(id) = op.op
                            && let Some(local) = boards.get(&id)
                        {
                            // The delete echo finalizes the tombstone and
                            // clears the binding (never
                            // `adopt_board_after_push`, which hard-codes
                            // `deleted: false`).
                            let finalized = adopt_board_after_delete_push(local, echo);
                            boards.insert(id, finalized);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            board_id = ?op_board_id(&op.op),
                            echo = ?echo.id,
                            "board push echo adopted"
                        );
                    }
                    Some(RemoteEcho::Label(echo)) => {
                        if let LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) = op.op
                            && let Some(local) = labels.get(&id)
                        {
                            let mut merged = adopt_label_after_push(local, echo);
                            let (update, delete) = pending_label_intent(outbox, id, read_at);
                            keep_label_intent(&mut merged, local, id, update, delete);
                            labels.insert(id, merged);
                            ctx.label_by_ref.insert(echo.id, id);
                        }
                        op_results.insert(outcome.op, true);
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            label_id = ?op_label_id(&op.op),
                            echo = ?echo.id,
                            "label push echo adopted"
                        );
                    }
                    // R9d: reorder returns no echo — complete and reconcile
                    // via the next pull.
                    None => {
                        op_results.insert(outcome.op, true);
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            "push completed without echo"
                        );
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
                        let mut cancelled_ops = 0;
                        for other in outbox {
                            if other.op_id != outcome.op
                                && op_targets_task(&other.op, id)
                                && !op_results.contains_key(&other.op_id)
                            {
                                op_results.insert(other.op_id, false);
                                cancelled_ops += 1;
                            }
                        }
                        tracing::debug!(
                            rule = RULE_R9,
                            op_id = ?outcome.op,
                            task_id = ?id,
                            cancelled_ops,
                            "remote missing, task tombstoned"
                        );
                    }
                    op_results.insert(outcome.op, true);
                }
                LocalOp::CreateStack(id)
                | LocalOp::RenameStack(id)
                | LocalOp::MoveStack(id)
                | LocalOp::DeleteStack(id) => {
                    if let Some(local) = stacks.get(&id) {
                        stacks.insert(id, tombstone_stack(local, now));
                    }
                    op_results.insert(outcome.op, true);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, stack_id = ?id, "remote missing, stack tombstoned");
                }
                LocalOp::UpdateBoard(id) | LocalOp::DeleteBoard(id) => {
                    if let Some(local) = boards.get(&id) {
                        boards.insert(id, tombstone_board(local, now));
                    }
                    op_results.insert(outcome.op, true);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, board_id = ?id, "remote missing, board tombstoned");
                }
                LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                    if let Some(local) = labels.get(&id) {
                        labels.insert(id, tombstone_label(local, now));
                    }
                    op_results.insert(outcome.op, true);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, label_id = ?id, "remote missing, label tombstoned");
                }
                LocalOp::AssignLabel(..) | LocalOp::UnassignLabel(..) => {
                    op_results.insert(outcome.op, true);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, "remote missing, assign op finalized");
                }
                // A ref-delete that 404s achieved its goal: the doomed
                // duplicate is gone either way (the 404-tolerant path,
                // D-5c). No tombstone — there is no local row.
                LocalOp::DeleteRemoteCard { .. } | LocalOp::DeleteRemoteStack { .. } => {
                    op_results.insert(outcome.op, true);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, "remote missing, ref-delete complete");
                }
                // Unreachable while nothing enqueues the op (MD-10) — and
                // even then, a missing document group must never tombstone
                // its task. Chunk 4's document-group completion owns the
                // real semantics; this arm is match totality only.
                LocalOp::UpdateAppData(_) | LocalOp::FlushBoardAppData(_) => {
                    op_results.insert(outcome.op, true);
                }
            },
            PushResult::Rejected { kind } => {
                // BadRequest dead-letters (malformed write, permanent).
                // A `LocalData` rejection dead-letters only the
                // post-wipe signature: an update-class op on a
                // *ref-less* entity whose owning board is bound — the
                // remote reports the ref unknown, and that ref will not
                // come back. Every other stranding class stays queued
                // for retry: create-class ops on ref-less entities
                // re-create once the owning stack re-binds; ref-less
                // entities of an unbound board wait (the standalone
                // precedent); entities bound to another remote carry
                // refs and wait.
                let dead_letter = *kind == SyncErrorKind::BadRequest
                    || (*kind == SyncErrorKind::LocalData
                        && dead_letters_localdata(&op.op, boards, stacks, tasks, labels));
                if dead_letter {
                    op_results.insert(outcome.op, false);
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, kind = ?kind, "push dead-lettered");
                } else {
                    tracing::debug!(rule = RULE_R9, op_id = ?outcome.op, kind = ?kind, "push rejected, stays queued");
                }
            }
        }
    }
    op_results
}

/// The dead-letter predicate for a `Rejected { LocalData }` outcome:
/// `true` only for an *update-class* op on an entity that exists,
/// carries no remote ref, and whose owning board is bound — the
/// post-bind-wipe signature (design D3-3). Create-class ops never
/// dead-letter here (they re-create on the new remote); board ops and
/// deletes are not update-class.
fn dead_letters_localdata(
    op: &LocalOp,
    boards: &BTreeMap<BoardId, Board>,
    stacks: &BTreeMap<StackId, Stack>,
    tasks: &BTreeMap<TaskId, Task>,
    labels: &BTreeMap<LabelId, Label>,
) -> bool {
    let candidate = match op {
        LocalOp::UpdateTask(id)
        | LocalOp::MoveTask(id)
        | LocalOp::AssignLabel(id, _)
        | LocalOp::UnassignLabel(id, _) => tasks
            .get(id)
            .map(|t| (t.remote.is_none(), stacks.get(&t.stack).map(|s| s.board))),
        LocalOp::RenameStack(id) | LocalOp::MoveStack(id) => {
            stacks.get(id).map(|s| (s.remote.is_none(), Some(s.board)))
        }
        LocalOp::UpdateLabel(id) => labels.get(id).map(|l| (l.remote.is_none(), Some(l.board))),
        _ => None,
    };
    match candidate {
        Some((refless, owning_board)) => {
            refless
                && owning_board
                    .and_then(|board| boards.get(&board))
                    .is_some_and(|board| board.remote.is_some())
        }
        None => false,
    }
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

    /// The UTC-midnight calendar every pipeline test injects (plain
    /// offset dates; the dedup key never consults the window start).
    fn test_day() -> crate::days::DayCalendar {
        crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }
    use super::*;
    use crate::merge_testutil::*;
    use crate::reminders::Insistence;
    use crate::test_support;

    use crate::remote::{RemoteIndex, RemoteLabel};
    use proptest::prelude::*;
    use std::collections::BTreeSet;
    use uuid;

    /// Lockstep assertion 1 (#244, D-F3): the domain structures agree
    /// with the `TASK_FIELDS` manifest exactly. Lives here, not in
    /// `entities.rs`, because `PendingTaskIntent` is private to this
    /// module — the assertion is the only consumer of its serde derives.
    #[test]
    fn task_fields_manifest_pins_the_domain_structures() {
        use crate::entities::TASK_FIELDS;
        use crate::messages::TaskChanges;

        let manifest_clocks: BTreeSet<&str> =
            TASK_FIELDS.iter().map(|(clock, _, _)| *clock).collect();

        // (a) A populated TaskClocks serializes exactly the clock set.
        let e = crate::entities::BoardClocks::epoch();
        let clocks = crate::entities::TaskClocks {
            title: e,
            description: e,
            duedate: e,
            scheduled: e,
            done: e,
            position: e,
            effort: e,
            importance: e,
            urgency: e,
            recur: e,

            archived: e,
            deleted: e,
        };
        let clock_keys_value = serde_json::to_value(clocks).unwrap();
        let clock_keys: BTreeSet<&str> = clock_keys_value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(clock_keys, manifest_clocks);

        // (b) An all-true PendingTaskIntent: the guard key set covers the
        // clock set and differs from it by exactly the two deliberate
        // non-clock members — `reminder_rules` (D-S6: rules carry their
        // own embedded write clocks) and `labels` (the OR-set guard
        // protects the membership map's entries, not a clock).
        let intent = PendingTaskIntent {
            title: true,
            description: true,
            duedate: true,
            scheduled: true,
            done: true,
            archived: true,
            position: true,
            labels: true,
            effort: true,
            importance: true,
            urgency: true,
            recur: true,
            reminder_rules: true,
            deleted: true,
        };
        let intent_keys_value = serde_json::to_value(intent).unwrap();
        let intent_keys: BTreeSet<&str> = intent_keys_value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        let extra: BTreeSet<&str> = intent_keys.difference(&manifest_clocks).copied().collect();
        assert_eq!(extra, BTreeSet::from(["reminder_rules", "labels"]));
        assert!(manifest_clocks.is_subset(&intent_keys));

        // (d) A fully-populated TaskChanges (every slot `Some`, so
        // `skip_serializing_if` hides nothing) names only mergeable
        // fields.
        let changes = TaskChanges {
            title: Some("t".into()),
            description: Some("d".into()),
            duedate: Some(Some(e)),
            scheduled: Some(Some(e)),
            effort: Some(Some(crate::entities::Effort::new(1))),
            importance: Some(Some(crate::entities::Importance::Low)),
            urgency: Some(Some(crate::entities::Urgency::Low)),
            recur: None,

            archived: Some(false),
        };
        let changes_keys_value = serde_json::to_value(changes).unwrap();
        let changes_keys: BTreeSet<&str> = changes_keys_value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        assert!(changes_keys.is_subset(&manifest_clocks));
    }

    // Lockstep assertion 1(c): every `Task` serializes at least the
    // union of the manifest's task fields.
    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(16))]

        #[test]
        fn task_key_set_covers_the_manifest_task_fields(task in test_support::task_strategy()) {
            use crate::entities::TASK_FIELDS;
            let value = serde_json::to_value(task).unwrap();
            let task_keys: BTreeSet<&str> = value
                .as_object()
                .expect("object")
                .keys()
                .map(String::as_str)
                .collect();
            for (_, fields, _) in TASK_FIELDS {
                for field in *fields {
                    assert!(task_keys.contains(field), "Task lacks `{field}`");
                }
            }
        }
    }

    // ---- chunk 4: the sidecar fold through apply_sync_report ----------

    use crate::appdata::{
        AppDataDocument, AppDataSections, ModeledValue, RuleEntry, RulesEntry, TaskEntry,
        TaskFieldWrite,
    };
    use crate::capabilities::DeckFeatures;
    use crate::messages::AppDataEvidence;

    fn fw(value: ModeledValue, clock: i64) -> TaskFieldWrite {
        TaskFieldWrite {
            value,
            clock: ts(clock),
        }
    }

    fn document_with(
        entries: &[(u64, TaskEntry)],
        rules: Option<&[(u64, RulesEntry)]>,
    ) -> AppDataEvidence {
        document_with_dismissals(entries, rules, None)
    }

    fn document_with_dismissals(
        entries: &[(u64, TaskEntry)],
        rules: Option<&[(u64, RulesEntry)]>,
        dismissals: Option<&[(u64, crate::appdata::DismissalsEntry)]>,
    ) -> AppDataEvidence {
        let mut tasks_map = std::collections::BTreeMap::new();
        for (card, entry) in entries {
            tasks_map.insert(card.to_string(), entry.clone());
        }
        let rules_map = rules.map(|rs| {
            let mut m = std::collections::BTreeMap::new();
            for (card, entry) in rs {
                m.insert(card.to_string(), entry.clone());
            }
            m
        });
        let dismissals_map = dismissals.map(|ds| {
            let mut m = std::collections::BTreeMap::new();
            for (card, entry) in ds {
                m.insert(card.to_string(), entry.clone());
            }
            m
        });
        AppDataEvidence::Document(Box::new(AppDataDocument {
            format: crate::appdata::APPDATA_FORMAT,
            board: board(REMOTE_BOARD_NUM),
            sections: AppDataSections {
                tasks: (!tasks_map.is_empty()).then_some(tasks_map),
                reminder_rules: rules_map,
                dismissals: dismissals_map,
                time_entries: None,
                habits: None,
                habit_log: None,
                unknown: std::collections::BTreeMap::new(),
            },
            unknown: std::collections::BTreeMap::new(),
        }))
    }

    /// A document evidence carrying only a `time-entries` section (the
    /// other sections left empty — the DR-5/2 fold's fixture).
    fn document_with_time_entries(
        rows: &[(uuid::Uuid, crate::appdata::TimeEntryRecord)],
    ) -> AppDataEvidence {
        let mut doc = match document_with_dismissals(&[], None, None) {
            AppDataEvidence::Document(doc) => *doc,
            _ => unreachable!("document_with_dismissals always builds a Document"),
        };
        doc.sections.time_entries = Some(
            rows.iter()
                .map(|(uuid, row)| (uuid.to_string(), row.clone()))
                .collect(),
        );
        AppDataEvidence::Document(Box::new(doc))
    }

    fn document_with_habit_sections(
        habits: &[(uuid::Uuid, crate::appdata::HabitRecord)],
        log: &[(crate::habit::HabitLogKey, crate::appdata::HabitLogRecord)],
    ) -> AppDataEvidence {
        let mut doc = match document_with_dismissals(&[], None, None) {
            AppDataEvidence::Document(doc) => *doc,
            _ => unreachable!("document_with_dismissals always builds a Document"),
        };
        doc.sections.habits = Some(
            habits
                .iter()
                .map(|(uuid, row)| (uuid.to_string(), row.clone()))
                .collect(),
        );
        doc.sections.habit_log = Some(
            log.iter()
                .map(|(key, row)| (key.wire_form(), row.clone()))
                .collect(),
        );
        AppDataEvidence::Document(Box::new(doc))
    }

    fn wire_live_habit(clock: i64) -> crate::appdata::HabitRecord {
        crate::appdata::HabitRecord::Live {
            title: "roamed".to_owned(),
            target: crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(3).unwrap()),
            order: 4,
            anchor_time: None,
            clocks: crate::habit::HabitClocks::all(ts(clock)),
            continued_from: None,
        }
    }

    fn wire_entry(task: Option<&str>, clock: i64) -> crate::appdata::TimeEntryRecord {
        crate::appdata::TimeEntryRecord::Entry {
            task: task.map(str::to_owned),
            title: "wire title".to_owned(),
            start: ts(clock - 1_000),
            end: None,
            created: ts(clock - 2_000),
            clock: ts(clock),
        }
    }

    fn fold_report(
        state: &AppState,
        outbox: &[crate::outbox::PendingOp],
        snapshot: &RemoteBoardSnapshot,
        evidence: &AppDataEvidence,
        features: DeckFeatures,
        read_at: i64,
    ) -> (
        AppState,
        Vec<PersistenceAction>,
        Vec<crate::messages::SystemEvent>,
    ) {
        apply_sync_report(
            state,
            outbox,
            snapshot,
            &[],
            ts(read_at),
            &CountingIds::default(),
            ts(read_at),
            &crate::test_support::test_connector(),
            evidence,
            features,
            test_day(),
        )
    }

    #[test]
    fn the_fold_adopts_newer_document_app_fields_and_records_the_row_facts() {
        let task = bound_task(1);
        let state = base_state(task);
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(
                    ModeledValue::Effort(Some(crate::entities::Effort::new(900))),
                    BASE + 300,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let evidence = document_with(&[(1, entry)], None);
        let (app, _actions, _events) = fold_report(
            &state,
            &[],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        let merged = task_by_card(&app, 1);
        assert_eq!(merged.effort, Some(crate::entities::Effort::new(900)));
        assert_eq!(merged.clocks.effort, ts(BASE + 300));
        let row = app.sync.get(*state.boards.keys().next().unwrap()).unwrap();
        assert_eq!(row.sidecar, crate::state::SidecarHealth::Healthy);
        assert_eq!(row.features, DeckFeatures::CONSERVATIVE);
    }

    #[test]
    fn the_rules_fold_lands_deltas_in_state_and_batch() {
        use crate::reminders::ReminderRule;
        let task = bound_task(1);
        let mut state = base_state(task);
        let existing = ReminderRule {
            insistence: Insistence::Normal,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: state.tasks.values().next().unwrap().id,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE),
            updated_at: ts(BASE),
        };
        state.reminders.rules.insert(existing.id, existing.clone());
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let rules_entry = RulesEntry {
            rules: [
                // A strictly newer tombstone for the existing rule.
                (
                    "00000000-0000-0000-0000-00000000000a".to_owned(),
                    RuleEntry::Tombstone {
                        clock: ts(BASE + 300),
                    },
                ),
                // A fresh rule the local side never had: adopted verbatim.
                (
                    "00000000-0000-0000-0000-00000000000b".to_owned(),
                    RuleEntry::Rule {
                        insistence: Insistence::Normal,
                        offset: crate::reminders::ReminderOffset::before(
                            chrono::TimeDelta::try_minutes(-15).unwrap(),
                        )
                        .unwrap(),
                        created: ts(BASE + 100),
                        clock: ts(BASE + 100),
                    },
                ),
            ]
            .into(),
            unknown: std::collections::BTreeMap::new(),
        };
        let evidence = document_with(&[], Some(&[(1, rules_entry)]));
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        // Memory view: the tombstone removed, the document rule adopted.
        assert!(!app.reminders.rules.contains_key(&existing.id));
        let adopted_id = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xb));
        let adopted = app.reminders.rules.get(&adopted_id).expect("adopted");
        assert_eq!(adopted.task, existing.task);
        assert_eq!(adopted.updated_at, ts(BASE + 100));
        // Batch: the same deltas, by construction.
        assert!(actions.iter().any(|a| matches!(
            a,
            PersistenceAction::DeleteReminderRule(id) if *id == existing.id
        )));
        assert!(
            actions.iter().any(
                |a| matches!(a, PersistenceAction::UpsertReminderRule(r) if r.id == adopted_id)
            )
        );
    }

    // ---- chunk B: the dismissals fold (design D-4/D-9) ----------------

    use crate::appdata::{DismissalEntry, dismissal_wire_key};
    use crate::reminders::{ReminderDefaults, ReminderDismissal, ReminderRef};

    fn dismissals_entry_for_test(
        entries: &[(String, DismissalEntry)],
    ) -> crate::appdata::DismissalsEntry {
        crate::appdata::DismissalsEntry {
            dismissals: entries.iter().cloned().collect(),
            unknown: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn the_dismissals_fold_suppresses_the_adopted_rules_occurrences() {
        // The fold-order property (D-9/P4): a rule and its dismissal
        // rows arrive in the SAME document pass — the adopted rule must
        // be suppressed before this device can fire once, while a
        // sibling rule without dismissals still fires.
        let task = bound_task(1);
        let mut state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        let mut task = state.tasks.get(&task_id).unwrap().clone();
        task.duedate = Some(ts(BASE + 1_000));
        task.clocks.duedate = ts(BASE + 1_000);
        state.tasks.insert(task_id, task);

        let dismissed_rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xa));
        let live_rule = crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xb));
        let rule = |id| crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id,
            task: task_id,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE + 100),
            updated_at: ts(BASE + 100),
        };
        let rules_entry = RulesEntry {
            rules: [
                (
                    "00000000-0000-0000-0000-00000000000a".to_owned(),
                    RuleEntry::Rule {
                        insistence: Insistence::Normal,
                        offset: crate::reminders::ReminderOffset::at_due(),
                        created: ts(BASE + 100),
                        clock: ts(BASE + 100),
                    },
                ),
                (
                    "00000000-0000-0000-0000-00000000000b".to_owned(),
                    RuleEntry::Rule {
                        insistence: Insistence::Normal,
                        offset: crate::reminders::ReminderOffset::at_due(),
                        created: ts(BASE + 100),
                        clock: ts(BASE + 100),
                    },
                ),
            ]
            .into(),
            unknown: std::collections::BTreeMap::new(),
        };
        let occurrence = ts(BASE + 1_000);
        let dismissals = dismissals_entry_for_test(&[(
            dismissal_wire_key(dismissed_rule, occurrence),
            DismissalEntry::Dismissed {
                dismissed_at: ts(BASE + 200),
            },
        )]);
        let evidence =
            document_with_dismissals(&[], Some(&[(1, rules_entry)]), Some(&[(1, dismissals)]));
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE),
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        // Both rules adopted; the dismissal row folded into the state.
        let adopted_dismissed = rule(dismissed_rule);
        let adopted_live = rule(live_rule);
        assert_eq!(
            app.reminders.rules.get(&dismissed_rule),
            Some(&adopted_dismissed)
        );
        assert_eq!(app.reminders.rules.get(&live_rule), Some(&adopted_live));
        let folded_row = ReminderDismissal {
            reminder: ReminderRef::Rule(dismissed_rule),
            occurrence,
            dismissed_at: ts(BASE + 200),
        };
        assert!(app.reminders.dismissals.contains(&folded_row));
        assert!(actions.contains(&PersistenceAction::RecordReminderDismissal(folded_row)));

        // The suppression, evaluated the way the tick does: the
        // dismissed rule fires nothing, the sibling rule still fires its
        // occurrence — same pass, one derivation.
        let firings = crate::reminders::due_firings(
            &app.tasks,
            &app.reminders,
            &ReminderDefaults::default(),
            ts(BASE + 3_600),
        );
        assert_eq!(firings.len(), 1);
        assert_eq!(firings[0].reminder, ReminderRef::Rule(live_rule));
        assert_eq!(firings[0].occurrence, occurrence);
    }

    #[test]
    fn the_dismissals_fold_is_guard_immune() {
        // The D-S6 intent flag protects local rule EDITS; a dismissal is
        // an append-only fact, so the guarded task still folds its
        // dismissal rows (D-9) — only the rules fold collapses.
        let task = bound_task(1);
        let state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(555)),
            op: LocalOp::UpdateAppData(task_id),
            queued_at: ts(BASE + 10), // after read_at: the guard holds
        };
        let rules_entry = RulesEntry {
            rules: [(
                "00000000-0000-0000-0000-00000000000a".to_owned(),
                RuleEntry::Rule {
                    insistence: Insistence::Normal,
                    offset: crate::reminders::ReminderOffset::at_due(),
                    created: ts(BASE + 100),
                    clock: ts(BASE + 100),
                },
            )]
            .into(),
            unknown: std::collections::BTreeMap::new(),
        };
        let occurrence = ts(BASE + 1_000);
        let dismissals = dismissals_entry_for_test(&[(
            dismissal_wire_key(
                crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(0xa)),
                occurrence,
            ),
            DismissalEntry::Dismissed {
                dismissed_at: ts(BASE + 200),
            },
        )]);
        let evidence =
            document_with_dismissals(&[], Some(&[(1, rules_entry)]), Some(&[(1, dismissals)]));
        let (app, actions, _events) = fold_report(
            &state,
            &[op],
            &single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE),
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE,
        );
        assert!(
            app.reminders.rules.is_empty(),
            "the guarded rules fold collapses"
        );
        assert_eq!(app.reminders.dismissals.len(), 1, "the dismissal folded");
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::RecordReminderDismissal(_)))
        );
    }

    #[test]
    fn the_guard_protects_a_post_read_app_edit_value_and_clock_through_the_fold() {
        // D3's clock-copy rationale: a flush that failed (its op stays
        // queued, stamped after read_at) plus a concurrent remote write
        // must not adopt the remote value one cycle late — the guard's
        // clock copy makes the next fold's comparison honest.
        let task = bound_task(1);
        let state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(555)),
            op: LocalOp::UpdateAppData(task_id),
            queued_at: ts(BASE + 10), // after read_at: never seen by the cycle
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(
                    ModeledValue::Effort(Some(crate::entities::Effort::new(900))),
                    BASE + 300,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let evidence = document_with(&[(1, entry)], None);
        let (app, actions, _events) = fold_report(
            &state,
            &[op],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE,
        );
        let merged = task_by_card(&app, 1);
        assert_eq!(merged.effort, None, "the protected local truth stands");
        assert_eq!(merged.clocks.effort, ts(BASE), "the clock copies too");
        // The op the cycle never saw is not settled by the fold.
        assert!(
            !actions.iter().any(|a| matches!(
                a,
                PersistenceAction::CompleteOp(id) | PersistenceAction::FailOp(id)
                if *id == crate::outbox::OpId(uuid::Uuid::from_u128(555))
            )),
            "the fold completes nothing"
        );
    }

    #[test]
    fn the_guard_boundary_sees_ops_queued_no_later_than_read_at() {
        let task = bound_task(1);
        let state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(556)),
            op: LocalOp::UpdateAppData(task_id),
            queued_at: ts(BASE), // == read_at: seen by the cycle
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let entry = TaskEntry {
            fields: [(
                "effort".to_owned(),
                fw(
                    ModeledValue::Effort(Some(crate::entities::Effort::new(900))),
                    BASE + 300,
                ),
            )]
            .into(),
            unmodeled: std::collections::BTreeMap::new(),
        };
        let evidence = document_with(&[(1, entry)], None);
        let (app, _actions, _events) = fold_report(
            &state,
            &[op],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE,
        );
        assert_eq!(
            task_by_card(&app, 1).effort,
            Some(crate::entities::Effort::new(900)),
            "an op the cycle saw is not protected"
        );
    }

    #[test]
    fn unreadable_evidence_folds_nothing_and_warns_on_the_row() {
        let task = bound_task(1);
        let mut state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        // A local app value the fold must not disturb.
        let mut seeded = state.tasks.get(&task_id).unwrap().clone();
        seeded.effort = Some(crate::entities::Effort::new(7));
        seeded.clocks.effort = ts(BASE);
        state.tasks.insert(task_id, seeded);
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let evidence =
            AppDataEvidence::Unreadable(crate::messages::AppDataUnreadable::FormatTooNew {
                format: 99,
            });
        let (app, _actions, _events) = fold_report(
            &state,
            &[],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        assert_eq!(
            task_by_card(&app, 1).effort,
            Some(crate::entities::Effort::new(7))
        );
        let row = app.sync.get(*state.boards.keys().next().unwrap()).unwrap();
        assert_eq!(
            row.sidecar,
            crate::state::SidecarHealth::Unreadable(
                crate::messages::AppDataUnreadable::FormatTooNew { format: 99 }
            )
        );
        // Absent evidence is healthy silence.
        let (app, _, _events) = fold_report(
            &state,
            &[],
            &snap,
            &AppDataEvidence::Absent,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        let row = app.sync.get(*state.boards.keys().next().unwrap()).unwrap();
        assert_eq!(row.sidecar, crate::state::SidecarHealth::Healthy);
    }

    #[test]
    fn a_failed_push_report_folds_nothing() {
        // The `Failed` path's fold ban: apply_push_report has no
        // evidence parameter — app fields and rules ride through
        // untouched, the last verdict stands on the row.
        let task = bound_task(1);
        let mut state = base_state(task);
        let task_id = task_by_card(&state, 1).id;
        let mut seeded = state.tasks.get(&task_id).unwrap().clone();
        seeded.effort = Some(crate::entities::Effort::new(7));
        seeded.clocks.effort = ts(BASE);
        state.tasks.insert(task_id, seeded);
        // One completed op: the report's affected-board set is exactly
        // the boards of the ops that got outcomes.
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(557)),
            op: LocalOp::UpdateTask(task_id),
            queued_at: ts(BASE),
        };
        let outcome = crate::remote::PushOutcome {
            op: op.op_id,
            result: crate::remote::PushResult::Applied { echo: None },
        };
        let (app, _actions) = apply_push_report(
            &state,
            &[op],
            &[outcome],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 3_600),
            DeckFeatures::NATIVE,
        );
        assert_eq!(
            task_by_card(&app, 1).effort,
            Some(crate::entities::Effort::new(7))
        );
        assert!(app.reminders.rules.is_empty());
        let row = app.sync.get(*state.boards.keys().next().unwrap()).unwrap();
        assert_eq!(row.features, DeckFeatures::NATIVE, "the flags ride the row");
    }

    #[test]
    fn a_document_carrying_habit_rows_folds_them_and_one_without_carries_through() {
        // §5b, D-16's reworked pin: the SYNC half folds the two habit
        // sidecar sections (the new semantics); the PUSH half below
        // stays a pin for the permanent rule — a push cycle never folds,
        // because it *is* the overlay's source.
        let task = bound_task(1);
        let mut state = base_state(task);
        let habit = crate::habit::Habit {
            id: crate::ids::HabitId::from(uuid::Uuid::from_u128(700)),
            board: *state.boards.keys().next().unwrap(),
            title: "meditate".into(),
            target: crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: crate::habit::HabitClocks::all(ts(BASE)),
            continued_from: None,
            deleted: false,
        };
        let key = crate::habit::HabitLogKey {
            habit: habit.id,
            window: crate::habit::WindowStart::of_period(
                crate::habit::PeriodKind::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
            ),
        };
        state.habits.habits.insert(habit.id, habit.clone());
        state.habits.log.insert(
            key,
            crate::habit::HabitLogEntry {
                count: 2,
                clock: ts(BASE),
            },
        );

        // A document without the sections carries everything through.
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &snap,
            &AppDataEvidence::Absent,
            DeckFeatures::CONSERVATIVE,
            BASE,
        );
        assert_eq!(app.habits, state.habits, "no sections, no fold");
        assert!(actions.iter().all(|a| !matches!(
            a,
            PersistenceAction::UpsertHabit(_) | PersistenceAction::RecordHabitLog(..)
        )),);

        // A document carrying a newer definition and a newer log row
        // folds both into the state and emits the persistence actions.
        let remote_id = uuid::Uuid::from_u128(701);
        let evidence = document_with_habit_sections(
            &[(remote_id, wire_live_habit(BASE + 500))],
            &[(
                key,
                crate::appdata::HabitLogRecord::Entry {
                    count: 6,
                    clock: ts(BASE + 400),
                },
            )],
        );
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &snap,
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE,
        );
        let roamed = &app.habits.habits[&crate::ids::HabitId::from(remote_id)];
        assert_eq!(roamed.title, "roamed", "the newer definition adopted");
        assert_eq!(
            roamed.board,
            *state.boards.keys().next().unwrap(),
            "the fold stamps the locally-bound board"
        );
        assert_eq!(app.habits.log[&key].count, 6, "the newer log row adopted");
        assert!(actions.iter().any(|a| matches!(a,
            PersistenceAction::UpsertHabit(h) if h.id == crate::ids::HabitId::from(remote_id))));
        assert!(actions.iter().any(|a| matches!(a,
            PersistenceAction::RecordHabitLog(k, e) if *k == key && e.count == 6)));
        // The local habit the document does not name stands untouched.
        assert_eq!(app.habits.habits[&habit.id], habit);
    }

    #[test]
    fn a_push_cycle_never_folds_the_habit_sections() {
        // The permanent rule (D-16's kept half): a push cycle IS the
        // overlay's source — its reports carry the habit state through
        // untouched; a default here would wipe every tracker.
        let task = bound_task(1);
        let mut state = base_state(task);
        let habit = crate::habit::Habit {
            id: crate::ids::HabitId::from(uuid::Uuid::from_u128(700)),
            board: *state.boards.keys().next().unwrap(),
            title: "meditate".into(),
            target: crate::habit::HabitTarget::PerDay(crate::habit::NonZeroCount::new(3).unwrap()),
            order: 0,
            anchor_time: None,
            clocks: crate::habit::HabitClocks::all(ts(BASE)),
            continued_from: None,
            deleted: false,
        };
        let key = crate::habit::HabitLogKey {
            habit: habit.id,
            window: crate::habit::WindowStart::of_period(
                crate::habit::PeriodKind::Day,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
            ),
        };
        state.habits.habits.insert(habit.id, habit);
        state.habits.log.insert(
            key,
            crate::habit::HabitLogEntry {
                count: 2,
                clock: ts(BASE),
            },
        );
        let expected = state.habits.clone();
        let (pushed, _actions) = apply_push_report(
            &state,
            &[],
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 3_600),
            DeckFeatures::NATIVE,
        );
        assert_eq!(pushed.habits, expected, "a push cycle never touches habits");
    }

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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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

    // ---- R3's resurrect carve-out (#130) ---------------------------

    /// A task holding unsynced work: a fresh field clock plus one live
    /// membership (the fixture the resurrect scenes build on).
    fn unsynced_task() -> Task {
        let mut task = bound_task(1);
        task.clocks.title = ts(BASE + 300);
        task.label_memberships.insert(
            crate::ids::LabelId::from(uuid::Uuid::from_u128(500)),
            crate::entities::MembershipState::Added { at: ts(BASE + 300) },
        );
        task
    }

    fn pending(op: LocalOp, num: u128) -> PendingOp {
        PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(num)),
            op,
            queued_at: ts(BASE),
        }
    }

    fn events_of(
        app: &(
            AppState,
            Vec<PersistenceAction>,
            Vec<crate::messages::SystemEvent>,
        ),
    ) -> &[crate::messages::SystemEvent] {
        &app.2
    }

    #[test]
    fn remote_absence_resurrects_unsynced_work_as_new_card() {
        let task = unsynced_task();
        let state = base_state(task);
        let update_op = pending(LocalOp::UpdateTask(task_by_card(&state, 1).id), 555);
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[update_op],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(!merged.deleted, "the work outlives the card");
        assert!(merged.remote.is_none(), "unbound, ready to re-create");
        assert!(merged.remote_seen.is_none());
        assert!(
            merged.remote_baseline.is_none(),
            "the trio clears together: a stale baseline would corrupt the next merge"
        );
        let creates: Vec<_> = app
            .1
            .iter()
            .filter(|a| matches!(a, PersistenceAction::EnqueueOp(p) if matches!(p.op, LocalOp::CreateTask(id) if id == merged.id)))
            .collect();
        assert_eq!(creates.len(), 1, "exactly one CreateTask re-enqueued");
        assert!(
            app.1.iter().any(|a| matches!(
                a,
                PersistenceAction::FailOp(id) if *id == OpId(uuid::Uuid::from_u128(555))
            )),
            "the subsumed update op is cancelled"
        );
        assert_eq!(
            events_of(&app),
            &[crate::messages::SystemEvent::TaskResurrected(merged.id)],
        );
    }

    #[test]
    fn create_echo_survives_a_stale_same_cycle_listing() {
        // Fetch-before-write: a card created and echoed in this cycle is
        // absent from the pre-push listing. The echo is presence
        // evidence — the fresh card must neither tombstone nor
        // resurrect in its first cycle.
        let task = unsynced_task();
        let state = base_state(task);
        let id = task_by_card(&state, 1).id;
        let create_op = pending(LocalOp::CreateTask(id), 557);
        let stale_listing = single_stack_snapshot(vec![], BASE + 60);
        let mut echo = remote_task(1, 5, BASE + 60);
        echo.title = "local 1".into();

        let app = apply_sync_report(
            &state,
            &[create_op],
            &stale_listing,
            &[PushOutcome {
                op: OpId(uuid::Uuid::from_u128(557)),
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(echo)),
                },
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(!merged.deleted, "the echo's card is present");
        assert_eq!(
            merged.remote.map(|r| r.card),
            Some(crate::ids::RemoteCardId(5))
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::EnqueueOp(_))),
            "no second create"
        );
        assert_eq!(events_of(&app).len(), 0);
    }

    #[test]
    fn remote_absence_resurrect_keeps_the_document_lane() {
        let task = unsynced_task();
        let state = base_state(task);
        let id = task_by_card(&state, 1).id;
        let ops = vec![
            pending(LocalOp::UpdateTask(id), 555),
            pending(LocalOp::UpdateAppData(id), 556),
        ];
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &ops,
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(
            app.1.iter().any(|a| matches!(
                a,
                PersistenceAction::FailOp(id) if *id == OpId(uuid::Uuid::from_u128(555))
            )),
            "the native-lane op is cancelled"
        );
        assert!(
            !app.1.iter().any(|a| matches!(
                a,
                PersistenceAction::FailOp(id) | PersistenceAction::CompleteOp(id)
                    if *id == OpId(uuid::Uuid::from_u128(556))
            )),
            "the document lane rides to the next cycle untouched"
        );
    }

    #[test]
    fn remote_absence_resurrect_preserves_work_and_memberships() {
        let task = unsynced_task();
        let state = base_state(task.clone());
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert_eq!(merged.clocks, task.clocks, "no re-stamping on resurrect");
        assert_eq!(merged.label_memberships, task.label_memberships);
        assert_eq!(merged.title, task.title);
    }

    #[test]
    fn synced_absence_emits_no_event() {
        let state = base_state(bound_task(1));
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted, "the 99% path keeps delete-wins");
        assert!(
            events_of(&app).is_empty(),
            "no broadcast on the synced path"
        );
    }

    #[test]
    fn cascade_card_present_under_dead_stack_never_resurrects() {
        // R6's cache-lag fixture: the card is listed under the deleted
        // stack, so R3 skips it and R6 tombstones — even unsynced.
        let state = base_state(unsynced_task());
        let mut remote = remote_stack(1, BASE + 420);
        remote.deleted_at = Some(ts(BASE + 420));
        let snap = snapshot(vec![remote], vec![remote_task(1, 1, BASE)]);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted, "cascade territory stays tombstone-wins");
        assert_eq!(events_of(&app).len(), 0);
    }

    #[test]
    fn cascade_both_card_and_stack_absent_never_resurrects() {
        // Guard 2: a collapsing world — resurrecting there would push a
        // fresh card into a dead stack.
        let state = base_state(unsynced_task());
        let empty_snapshot = snapshot(vec![], vec![]);

        let app = apply_sync_report(
            &state,
            &[],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted, "absent stack keeps the tombstone");
        assert_eq!(events_of(&app).len(), 0);
    }

    #[test]
    fn tombstoned_task_absence_stays_tombstone() {
        // Guard 1: a tombstoned task's last intent was deletion — fresh
        // rule stamps do not resurrect it.
        let mut task = unsynced_task();
        task.deleted = true;
        task.clocks.deleted = ts(BASE + 300);
        let state = base_state(task);
        let mut rule = crate::reminders::ReminderRule {
            insistence: Insistence::Normal,
            id: crate::ids::ReminderRuleId::from(uuid::Uuid::from_u128(10)),
            task: task_by_card(&state, 1).id,
            offset: crate::reminders::ReminderOffset::at_due(),
            created_at: ts(BASE + 300),
            updated_at: ts(BASE + 300),
        };
        rule.task = task_by_card(&state, 1).id;
        let mut state = state;
        state.reminders.rules.insert(rule.id, rule.clone());
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);

        let app = apply_sync_report(
            &state,
            &[],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.values().next().unwrap();
        assert!(merged.deleted);
        assert!(merged.remote.is_none());
        assert_eq!(events_of(&app).len(), 0);
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(256))]

        /// The design's property with the D-3d carve-out explicit: a
        /// bound, live task holding unsynced work, absent from a
        /// snapshot whose stack is still present, never reaches
        /// `deleted: true` through the R3 task branch.
        #[test]
        fn unsynced_work_never_tombstones_through_r3(
            task in crate::test_support::task_strategy(),
        ) {
            let mut task = task;
            task.remote = Some(card_ref(1, 1));
            task.stack = local_stack().id;
            task.deleted = false;
            // `None` rides the epoch fallback: the predicate is true by
            // construction for any stamp set.
            task.remote_seen = None;
            let state = base_state(task);

            let app = apply_sync_report(
                &state,
                &[],
                &single_stack_snapshot(vec![], BASE + 3_600),
                &[],
                DateTime::<chrono::Utc>::MAX_UTC,
                &CountingIds::default(),
                ts(BASE + 3_600),
                &crate::test_support::test_connector(),
                &crate::messages::AppDataEvidence::Absent,
                crate::capabilities::DeckFeatures::CONSERVATIVE,

                test_day());

            let merged = app.0.tasks.values().next().unwrap();
            prop_assert!(!merged.deleted);
            prop_assert!(merged.remote.is_none());
            prop_assert_eq!(app.2.len(), 1);
        }
    }

    #[test]
    fn resurrect_then_rebind_then_second_delete_tombstones() {
        // One new card per contention episode: resurrect → create's echo
        // rebinds → a second delete of the *fresh* card is an ordinary
        // synced delete (the work is synced now).
        let state = base_state(unsynced_task());
        let empty_snapshot = single_stack_snapshot(vec![], BASE + 3_600);
        let (state_after_resurrect, actions, _) = apply_sync_report(
            &state,
            &[],
            &empty_snapshot,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        let create: PendingOp = actions
            .iter()
            .find_map(|a| match a {
                PersistenceAction::EnqueueOp(p) => Some(p.clone()),
                _ => None,
            })
            .expect("the resurrect enqueued one create");

        // The create's push lands; the echo rebinds the fresh card.
        let mut echo = remote_task(1, 5, BASE + 7_200);
        echo.title = state_after_resurrect
            .tasks
            .values()
            .next()
            .unwrap()
            .title
            .clone();
        let (rebound, _) = apply_push_report(
            &state_after_resurrect,
            std::slice::from_ref(&create),
            &[PushOutcome {
                op: create.op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(echo)),
                },
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 7_200),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );
        let rebound_task = rebound.tasks.values().next().unwrap();
        assert!(!rebound_task.deleted && rebound_task.remote.is_some());

        // Second delete of the fresh card: synced ⇒ tombstone, no event.
        let second_absence = single_stack_snapshot(vec![], BASE + 10_800);
        let (final_state, _, events) = apply_sync_report(
            &rebound,
            &[],
            &second_absence,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 10_800),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        let merged = final_state.tasks.values().next().unwrap();
        assert!(merged.deleted, "the episode is over: synced delete wins");
        assert_eq!(events.len(), 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 600),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        let merged_task = app.0.tasks.values().next().unwrap();
        assert!(merged_task.deleted, "stack tombstone cascades to its tasks");
    }

    /// Per-connector attribution: remote ids are only unique per server,
    /// so a report from another connector whose remote board id collides
    /// with an already-bound board adopts a fresh local board under its
    /// own tag — it must never merge server B's data into server A's
    /// board.
    #[test]
    fn a_foreign_connectors_colliding_board_adopts_fresh_not_merges() {
        let state = base_state(bound_task(1)); // bound to `test_connector`, remote board 77
        let snap = snapshot(vec![], vec![]); // reports remote board 77 too
        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::ids::ConnectorId::from("beta"),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(app.0.boards.len(), 2, "adoption, not a merge");
        let alpha = app
            .0
            .boards
            .values()
            .find(|b| {
                b.remote
                    .as_ref()
                    .is_some_and(|r| r.connector == crate::test_support::test_connector())
            })
            .expect("the pre-bound board keeps its binding");
        assert_eq!(alpha.remote.as_ref().map(|r| r.board), Some(board(77)));
        let beta = app
            .0
            .boards
            .values()
            .find(|b| {
                b.remote
                    .as_ref()
                    .is_some_and(|r| r.connector.as_str() == "beta")
            })
            .expect("the colliding board is adopted under beta's tag");
        assert_eq!(beta.remote.as_ref().map(|r| r.board), Some(board(77)));
        assert_ne!(alpha.id, beta.id, "two distinct local boards");
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
        let merged = adopt_task_after_push(&local, &echo, &ctx, true);

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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_660),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            0,
            "reorder op completes"
        );
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            1,
            "retryable rejection stays queued"
        );
        assert_eq!(app.0.sync.aggregate().phase, SyncPhase::Idle);

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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            0,
            "BadRequest dead-letters"
        );
        assert_eq!(
            app.0.sync.aggregate().phase,
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            adopted_task.live_labels(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );
        let second_state = app_into_state(&first);
        let second = apply_sync_report(
            &second_state,
            &[],
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 180),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_660),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        // R9a echo adoption applies to create/update pushes only; a move
        // has no echo (R9d) and must not consume one addressed to it.
        let state_after = app_into_state(&app);
        assert_eq!(task_by_card(&state_after, 1).title, "local 1");
    }

    #[test]
    fn a_create_pushed_and_pulled_in_the_same_cycle_is_not_re_adopted() {
        // The J1 finding: the create's echo binds the local task to card
        // 9 during the push phase; the same cycle's pull then reports
        // card 9. The card→local matching map must be built from the
        // *post-echo* task map, or the pull re-adopts the card as a
        // duplicate local task (which then wedges the update binding).
        let mut created = bound_task(9);
        created.remote = None;
        created.remote_seen = None;
        created.title = "created".into();
        let created_id = created.id;
        let state = base_state(created);

        let echo = remote_task(1, 9, BASE + 60);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(9));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::CreateTask(created_id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![echo.clone()], BASE + 60);

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
            ts(BASE + 30), // the cycle read after the create was queued
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let live_count = state_after.tasks.values().filter(|t| !t.deleted).count();
        assert_eq!(
            live_count, 1,
            "the echoed create must match the pulled card, not re-adopt it as a duplicate"
        );
        let merged = task_by_card(&state_after, 9);
        assert_eq!(merged.id, created_id, "the pull matched the pushed task");
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == op_id)),
            "the create op completed"
        );
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 420),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
        assert_eq!(app.0.sync.aggregate().pending_ops, 1);

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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
        assert_eq!(app2.0.sync.aggregate().pending_ops, 1);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            app.0.sync.aggregate().pending_ops,
            1,
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 180),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 90),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            app.0.sync.aggregate().pending_ops,
            1,
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
    fn apply_push_report_applies_completed_pushes_without_a_snapshot() {
        // A cycle that pushed an update and then lost the network reports
        // Failed { pushes }: the completed push lands (op completes, echo
        // binds) and nothing is tombstoned for lack of a snapshot.
        let mut local = bound_task(1);
        local.title = "local edit".into();
        local.clocks.title = ts(BASE + 300);
        let state = base_state(local);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(60));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let mut echo = remote_task(1, 1, BASE + 3_600);
        echo.title = "normalized".into();

        let app = apply_push_report(
            &state,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Applied {
                    echo: Some(crate::remote::RemoteEcho::Task(echo)),
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 3_660),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        let state_after = app.0.clone();
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.title, "normalized", "echo adopted");
        assert!(!merged.deleted, "no snapshot means no tombstones");
        assert_eq!(app.0.sync.aggregate().pending_ops, 0, "the push completed");
        assert_eq!(app.0.sync.aggregate().phase, SyncPhase::Idle);
        assert_eq!(app.0.sync.aggregate().last_success, Some(ts(BASE + 3_660)));
    }

    #[test]
    fn apply_push_report_keeps_last_success_when_an_outcome_was_rejected() {
        let task = bound_task(1);
        let mut base = base_state(task);
        let bound = base.boards.keys().copied().next().expect("bound board");
        base.sync.set_stored(bound, SyncPhase::Idle, Some(ts(BASE)));
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(61));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&base, 1).id),
            queued_at: ts(BASE),
        };

        let app = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::Forbidden,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            1,
            "rejected op stays queued"
        );
        assert_eq!(app.0.sync.aggregate().phase, SyncPhase::Idle);
        assert_eq!(
            app.0.sync.aggregate().last_success,
            Some(ts(BASE)),
            "a rejected write is not a successful sync"
        );
    }

    #[test]
    fn apply_push_report_dead_letters_bad_request_like_the_pipeline() {
        let task = bound_task(1);
        let base = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(62));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&base, 1).id),
            queued_at: ts(BASE),
        };

        let app = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::BadRequest,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        assert_eq!(
            app.0.sync.aggregate().phase,
            SyncPhase::Failed {
                last_error: SyncErrorKind::BadRequest
            }
        );
    }

    /// A `LocalData`-rejected update on the post-wipe signature: the
    /// dead-letter rule (D3-3) removes the op and counts it.
    #[test]
    fn localdata_rejected_update_on_refless_entity_of_bound_board_failops() {
        let mut task = bound_task(1);
        task.remote = None; // the bind-wipe signature
        let base = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(63));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(TaskId::from(uuid::Uuid::from_u128(1))),
            queued_at: ts(BASE),
        };
        let (merged, actions) = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::LocalData,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );
        assert!(
            actions.contains(&PersistenceAction::FailOp(op_id)),
            "the stranded update dead-letters"
        );
        assert_eq!(merged.sync.aggregate().pending_ops, 0);
    }

    #[test]
    fn localdata_rejected_update_on_never_bound_board_stays_queued() {
        let mut task = bound_task(1);
        task.remote = None;
        let mut base = base_state(task);
        // The board itself is unbound: the standalone precedent — ops wait.
        let board = base.boards.values_mut().next().unwrap();
        board.remote = None;
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(64));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(TaskId::from(uuid::Uuid::from_u128(1))),
            queued_at: ts(BASE),
        };
        let (_, actions) = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::LocalData,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "an unbound board's ops wait, they do not die"
        );
    }

    #[test]
    fn localdata_rejected_op_on_entity_bound_elsewhere_stays_queued() {
        // The entity carries a ref (bound to another remote): it waits
        // (chunk 4's ownership filter owns that world).
        let base = base_state(bound_task(1));
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(65));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_by_card(&base, 1).id),
            queued_at: ts(BASE),
        };
        let (_, actions) = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::LocalData,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "a ref-carrying entity's op waits"
        );
    }

    #[test]
    fn localdata_rejected_create_class_stays_queued() {
        // The re-create promise: create-ops on ref-less entities of a
        // bound board survive (they re-create on the new remote).
        let mut task = bound_task(1);
        task.remote = None;
        let base = base_state(task);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(66));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::CreateTask(TaskId::from(uuid::Uuid::from_u128(1))),
            queued_at: ts(BASE),
        };
        let (_, actions) = apply_push_report(
            &base,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: crate::remote::PushResult::Rejected {
                    kind: SyncErrorKind::LocalData,
                },
            }],
            chrono::DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "create-class ops re-create, they never die here"
        );
    }

    proptest! {
            #![proptest_config(test_support::proptest_config(256))]

            #[test]
            fn apply_push_report_equals_the_pipeline_push_phase(
                state in test_support::persisted_state_strategy().prop_filter(
                    "bound board and seen baselines",
                    |state| {
                        // Equivalence holds when the snapshot mirrors the bound
                        // state exactly (so the merge/reconciliation phases are
                        // no-ops) and no board cascade can fire.
                        let bound_board = state
                            .boards
                            .values()
                            .find(|b| b.remote.is_some());
                        let Some(board) = bound_board else { return false };
                        board.remote_seen.is_some()
                            && !board.deleted
                            && state.stacks.values().filter(|s| s.remote.is_some())
                                .all(|s| s.remote_seen.is_some())
                            && state.labels.values().filter(|l| l.remote.is_some())
                                .all(|l| l.remote_seen.is_some())
                            && state.tasks.values().filter(|t| t.remote.is_some())
                                .all(|t| t.remote_seen.is_some())
                            // Push evidence must reference real outbox ops.
                            && !state.outbox.is_empty()
                    },
                ),
                pushes in proptest::collection::vec(
                    (
                        test_support::op_id_strategy(),
                        proptest::option::of(
                            test_support::remote_task_strategy().prop_map(crate::remote::RemoteEcho::Task),
                        ),
                    )
                        .prop_map(|(op, echo)| crate::remote::PushOutcome {
                            op,
                            result: crate::remote::PushResult::Applied { echo },
                        }),
                    0..6,
                )
                .prop_filter("the push phase needs board evidence", |pushes| !pushes.is_empty())
            ) {
                let Some(mut snapshot) = mirror_snapshot(&state) else {
                    prop_assume!(false, "bound board available");
                    return Ok(());
                };
                // A real cycle's push outcomes reference the outbox ops the
                // cycle planned, and its create-echoes bind cards the next
                // pull actually contains. The mirror cannot know a fresh
                // echo, so echoes are kept only where they restate a ref the
                // state already binds (an update echo); junk op references
                // carry no board attribution. Both pipelines receive the
                // same evidence either way — the equivalence under test.
                let outbox_ids: Vec<crate::outbox::OpId> =
                    state.outbox.iter().map(|entry| entry.op_id).collect();
                let bound_refs: std::collections::BTreeSet<_> = state
                    .tasks
                    .values()
                    .filter_map(|t| Some((t.remote?.board, t.remote?.card)))
                    .collect();
                let pushes: Vec<crate::remote::PushOutcome> = pushes
                    .into_iter()
                    .enumerate()
                    .map(|(i, outcome)| crate::remote::PushOutcome {
                        op: outbox_ids[i % outbox_ids.len()],
                        result: match outcome.result {
                            crate::remote::PushResult::Applied { echo } => {
                                let mirror_bound = echo.as_ref().is_some_and(|echo| {
                                    let task_ref = match echo {
                                        crate::remote::RemoteEcho::Task(t) => t.id,
                                        _ => return false,
                                    };
                                    bound_refs.contains(&(task_ref.board, task_ref.card))
                                });
                                crate::remote::PushResult::Applied {
                                    echo: if mirror_bound { echo } else { None },
                                }
                            }
                            other => other,
                        },
                    })
                    .collect();
                // The mirror is the *post-push* server truth: a Deck listing
                // never returns tombstoned resources, and a delete pushed in
                // this very cycle is already server-side gone. Drop tombstones
                // and the refs this cycle's applied deletes finalized.
                let op_by_id: std::collections::HashMap<_, _> = state
                    .outbox
                    .iter()
                    .map(|entry| (entry.op_id, &entry.op))
                    .collect();
                let pushed_stack_deletes: std::collections::BTreeSet<_> = pushes
                    .iter()
                    .filter_map(|outcome| match outcome.result {
                        crate::remote::PushResult::Applied { .. } => op_by_id.get(&outcome.op),
                        _ => None,
                    })
                    .filter_map(|op| match op {
                        LocalOp::DeleteStack(id) => state.stacks.get(id).and_then(|s| s.remote),
                        _ => None,
                    })
                    .collect();
                let pushed_label_deletes: std::collections::BTreeSet<_> = pushes
                    .iter()
                    .filter_map(|outcome| match outcome.result {
                        crate::remote::PushResult::Applied { .. } => op_by_id.get(&outcome.op),
                        _ => None,
                    })
                    .filter_map(|op| match op {
                        LocalOp::DeleteLabel(id) => state.labels.get(id).and_then(|l| l.remote),
                        _ => None,
                    })
                    .collect();
                let pushed_task_deletes: std::collections::BTreeSet<_> = pushes
                    .iter()
                    .filter_map(|outcome| match outcome.result {
                        crate::remote::PushResult::Applied { .. } => op_by_id.get(&outcome.op),
                        _ => None,
                    })
                    .filter_map(|op| match op {
                        LocalOp::DeleteTask(id) => {
                            state.tasks.get(id).and_then(|t| t.remote.map(|r| (r.board, r.card)))
                        }
                        _ => None,
                    })
                    .collect();
                snapshot.stacks.retain(|s| {
                    s.deleted_at.is_none() && !pushed_stack_deletes.contains(&s.id)
                });
                snapshot.labels.retain(|l| {
                    l.deleted_at.is_none() && !pushed_label_deletes.contains(&l.id)
                });
                snapshot.tasks.retain(|t| {
                    !pushed_task_deletes.contains(&(t.id.board, t.id.card))
                });

                let app: AppState = state.clone().into();
                let now = ts(2_000_000_000);
                let (pushed_state, pushed_actions) = apply_push_report(
                    &app, &state.outbox, &pushes, chrono::DateTime::<chrono::Utc>::MAX_UTC, &CountingIds::default(), now,
                    crate::capabilities::DeckFeatures::CONSERVATIVE);
                let (full_state, full_actions, _events) = apply_sync_report(
                    &app, &state.outbox, &snapshot, &pushes, DateTime::<chrono::Utc>::MAX_UTC, &CountingIds::default(), now,
                &crate::test_support::test_connector(),
                    &crate::messages::AppDataEvidence::Absent,
    crate::capabilities::DeckFeatures::CONSERVATIVE, test_day());
                prop_assert_eq!(pushed_state, full_state);
                prop_assert_eq!(pushed_actions, full_actions);
            }
        }

    /// Builds a snapshot that mirrors every bound entity of `state`
    /// (P2 helper): same fields, `last_modified` equal to each entity's
    /// `remote_seen`, so the pipeline's merge and reconciliation phases
    /// reduce to no-ops and only the push phase can act.
    fn mirror_snapshot(state: &crate::persistence::PersistedState) -> Option<RemoteBoardSnapshot> {
        use crate::remote::{RemoteBoard, RemoteLabel as RL, RemoteStack as RS, RemoteTask as RT};
        let board = state.boards.values().find(|b| b.remote.is_some())?;
        let board_id = board.remote.as_ref()?.board;
        let remote_board = RemoteBoard {
            id: board_id,
            title: board.title.clone(),
            color: board.color.as_str().to_owned(),
            archived: board.archived,
            deleted_at: board
                .deleted
                .then_some(board.remote_seen.unwrap_or_else(utc_min)),
            last_modified: board.remote_seen?,
        };
        let stacks: Vec<RS> = state
            .stacks
            .values()
            .filter_map(|s| {
                let r = s.remote?;
                Some(RS {
                    id: r,
                    title: s.title.clone(),
                    order: s.order,
                    archived: s.archived,
                    deleted_at: s.deleted.then_some(s.remote_seen.unwrap_or_else(utc_min)),
                    last_modified: s.remote_seen?,
                })
            })
            .collect();
        let labels: Vec<RL> = state
            .labels
            .values()
            .filter_map(|l| {
                let r = l.remote?;
                Some(RL {
                    id: r,
                    title: l.title.clone(),
                    color: l.color.as_str().to_owned(),
                    deleted_at: l.deleted.then_some(l.remote_seen.unwrap_or_else(utc_min)),
                    last_modified: l.remote_seen?,
                })
            })
            .collect();
        let label_refs: std::collections::BTreeMap<LabelId, crate::ids::RemoteLabelRef> = state
            .labels
            .values()
            .filter_map(|l| Some((l.id, l.remote?)))
            .collect();
        let tasks: Vec<RT> = state
            .tasks
            .values()
            .filter_map(|t| {
                let r = t.remote?;
                Some(RT {
                    id: r,
                    title: t.title.clone(),
                    description: t.description.clone(),
                    duedate: t.duedate,
                    scheduled: t.scheduled,
                    done: t.done,
                    stack: r.stack,
                    order: t.order,
                    labels: t
                        .live_labels()
                        .iter()
                        .filter_map(|id| label_refs.get(id).map(|ref_| ref_.label))
                        .collect(),
                    archived: t.archived,
                    last_modified: t.remote_seen?,
                    created_at: t.remote_seen?,
                })
            })
            .collect();
        Some(RemoteBoardSnapshot {
            board: remote_board,
            stacks,
            tasks,
            labels,
        })
    }

    #[test]
    fn a_cross_stack_move_pulls_into_the_same_task() {
        // Identity regression: the card moved from stack 1 to stack 3
        // server-side (or via a just-pushed local move). Matching by the
        // full card ref would read that as delete+create and tombstone the
        // moved task; matching by (board, card) merges in place.
        let state = base_state(bound_task(1));
        let task_id = task_by_card(&state, 1).id;
        let snap = snapshot(
            vec![remote_stack(1, BASE), remote_stack(3, BASE + 120)],
            vec![],
        );
        let mut moved = remote_task(3, 1, BASE + 120); // stack 3, card 1
        moved.order = 4;
        let mut snap = snap;
        snap.tasks.push(moved);

        let app = apply_sync_report(
            &state,
            &[],
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 180),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.tasks.get(&task_id).expect("same local task");
        assert!(!merged.deleted, "the moved task must not tombstone");
        assert_eq!(
            merged.remote,
            Some(card_ref(3, 1)),
            "the binding follows the card to the new stack"
        );
        assert_eq!(
            merged.stack.as_uuid().as_u128(),
            app.0
                .stacks
                .values()
                .find(|s| s.remote == Some(stack_ref(3)))
                .expect("stack 3 adopted")
                .id
                .as_uuid()
                .as_u128(),
            "the local stack moves with the card"
        );
        assert_eq!(merged.order, 4);
        // No second (adopted) copy of the card exists.
        assert_eq!(app.0.tasks.len(), 1);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let task_id = task_by_card(&state, 1).id;
        let merged = &app.0.tasks[&task_id];
        assert!(merged.deleted);
        assert!(
            merged.remote.is_none(),
            "R9c clears the binding immediately"
        );
        assert!(merged.remote_seen.is_none());
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.stacks[stack_id];
        assert_eq!(merged.remote, Some(stack_ref(1)));
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.labels[&label_id];
        assert_eq!(merged.remote, Some(expected_ref));
        assert_eq!(merged.title, "normalized");
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(app.0.stacks[&stack_id].deleted);
        assert!(app.0.stacks[&stack_id].remote.is_none());
        assert!(app.0.labels[&label_id].deleted);
        assert!(app.0.labels[&label_id].remote.is_none());
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(app.0.stacks[&stack_id].deleted);
        assert!(app.0.stacks[&stack_id].remote.is_none());
        assert!(app.0.labels[&label_id].deleted);
        assert!(app.0.labels[&label_id].remote.is_none());
        assert_eq!(app.0.sync.aggregate().pending_ops, 0);
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            0,
            "all three ops resolved"
        );
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            1,
            "sibling assign cancelled, stack op queued"
        );
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::outbox::OpId(uuid::Uuid::from_u128(29)))),
        );
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            1,
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.labels[&label_id];
        assert!(merged.deleted);
        assert!(merged.remote.is_none());
    }

    #[test]
    fn a_vanishing_label_force_removes_its_memberships_and_kills_queued_assignments() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(5);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        // The live task holds the label; a queued assign op (queued after
        // the read) rides the same cycle.
        let task_id = task_by_card(&state, 1).id;
        let mut carrying = task_by_card(&state, 1).clone();
        carrying.label_memberships.insert(
            label_id,
            crate::entities::MembershipState::Added { at: ts(BASE) },
        );
        state.tasks.insert(carrying.id, carrying);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(45)),
            op: LocalOp::AssignLabel(task_id, label_id),
            queued_at: ts(BASE + 30),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 60)], BASE + 60);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let (final_state, actions, _events) = app;
        let merged = task_by_card(&final_state, 1);
        assert!(
            !merged.live_labels().contains(&label_id),
            "the dead label's membership is force-removed"
        );
        assert_eq!(
            merged.label_memberships[&label_id],
            crate::entities::MembershipState::Removed { at: ts(BASE + 60) },
            "stamped at the observation time, remembered not deleted"
        );
        assert!(
            actions.iter().any(|a| matches!(
                a,
                PersistenceAction::FailOp(op_id)
                    if op_id.0 == uuid::Uuid::from_u128(45)
            )),
            "the queued assignment dies with the entity"
        );
        assert!(actions.iter().any(|a| matches!(
            a,
            PersistenceAction::UpsertTask(t) if t.id == task_id && !t.live_labels().contains(&label_id)
        )));
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            app.0.sync.aggregate().pending_ops,
            0,
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let task_id = task_by_card(&state, 1).id;
        assert!(app.0.tasks[&task_id].deleted);
        assert_eq!(
            app.0.sync.aggregate().pending_ops,
            0,
            "cascade cancels the moved op"
        );
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 420),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
        let (second, _, _events) = apply_sync_report(
            &app_into_state(&first),
            &[],
            &next_snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 180),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let adopted = app
            .0
            .boards
            .values()
            .find(|b| b.remote == crate::test_support::test_binding(board(88)))
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
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
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        // Deliberately not `assert!(app.1.is_empty())`: CI's newer stable
        // clippy denies that shape (clippy::assert_is_empty) while the
        // lint is unknown to the pinned dev toolchain; the length check
        // is accepted by both.
        assert_eq!(app.1.len(), 0, "unknown op outcome must be a no-op");
    }

    #[test]
    // One test per R9-arm × ghost-entity combination; splitting would
    // scatter the decision table it enumerates.
    #[allow(clippy::too_many_lines)]
    fn push_outcomes_for_entities_absent_from_state_are_no_ops() {
        // Engine/repo desync defense: an outbox op may reference an entity
        // that vanished from state. Every R9 arm must no-op on the missing
        // entity (no panic, no fabricated entry) while still resolving the
        // op. An unbound board/label in state is likewise skipped by the
        // remote index.
        let mut state = base_state(bound_task(1));
        let unbound_board = crate::entities::Board {
            id: crate::ids::BoardId::from(uuid::Uuid::from_u128(810)),
            remote: None,
            title: "unbound".into(),
            color: crate::entities::Color::new("0000ff"),
            archived: false,
            deleted: false,
            clocks: crate::entities::BoardClocks {
                title: crate::entities::BoardClocks::epoch(),
                color: crate::entities::BoardClocks::epoch(),
                archived: crate::entities::BoardClocks::epoch(),
                deleted: crate::entities::BoardClocks::epoch(),
            },
            remote_seen: None,
        };
        state.boards.insert(unbound_board.id, unbound_board);
        let unbound_label = bound_label(20);
        let unbound_label_id = unbound_label.id;
        let mut unbound_label = unbound_label;
        unbound_label.remote = None;
        state.labels.insert(unbound_label_id, unbound_label);

        let ghost_task = TaskId::from(uuid::Uuid::from_u128(811));
        let ghost_stack = StackId::from(uuid::Uuid::from_u128(812));
        let ghost_label = LabelId::from(uuid::Uuid::from_u128(813));
        let ops: Vec<PendingOp> = [
            LocalOp::DeleteTask(ghost_task),
            LocalOp::UpdateTask(ghost_task),
            LocalOp::DeleteStack(ghost_stack),
            LocalOp::DeleteLabel(ghost_label),
            LocalOp::RenameStack(ghost_stack),
            LocalOp::UpdateLabel(ghost_label),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, op)| PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(600 + i as u128)),
            op,
            queued_at: ts(BASE),
        })
        .collect();
        let pushes = vec![
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(600)),
                result: PushResult::Applied { echo: None },
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(601)),
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(remote_task(1, 9, BASE + 60))),
                },
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(602)),
                result: PushResult::Applied { echo: None },
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(603)),
                result: PushResult::RemoteMissing,
            },
            // A stack/label RemoteMissing against the ghost entities.
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(602)),
                result: PushResult::RemoteMissing,
            },
            // DeleteLabel ghost accepted (Applied arm on a missing entity).
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(603)),
                result: PushResult::Applied { echo: None },
            },
            // RemoteMissing for the non-delete ghost ops: update-task,
            // rename-stack, update-label arms on missing entities.
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(601)),
                result: PushResult::RemoteMissing,
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(604)),
                result: PushResult::RemoteMissing,
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(605)),
                result: PushResult::RemoteMissing,
            },
            // Malformed outcomes: an echo kind addressed to an op kind it
            // can never belong to must be ignored, not adopted. Addressed
            // to non-delete ops — a delete op short-circuits before the
            // echo match.
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(601)),
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Stack(remote_stack(1, BASE + 60))),
                },
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(601)),
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Label(remote_label(1, BASE + 60))),
                },
            },
            PushOutcome {
                op: crate::outbox::OpId(uuid::Uuid::from_u128(604)),
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(remote_task(1, 9, BASE + 60))),
                },
            },
        ];

        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);
        let (merged, actions, _events) = apply_sync_report(
            &state,
            &ops,
            &snap,
            &pushes,
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        // No ghost entity was fabricated into state; every ghost op is
        // resolved (completed), so only the outbox is empty afterwards.
        assert!(!merged.tasks.contains_key(&ghost_task));
        assert!(!merged.stacks.contains_key(&ghost_stack));
        assert!(!merged.labels.contains_key(&ghost_label));
        assert_eq!(merged.sync.aggregate().pending_ops, 0);
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, PersistenceAction::CompleteOp(_)))
                .count(),
            6,
            "all six ghost ops completed",
        );
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

            let a = apply_sync_report(&state, &[], &snap, &[], DateTime::<chrono::Utc>::MAX_UTC, &CountingIds::default(), ts(20_000),
                &crate::test_support::test_connector(),
                &crate::messages::AppDataEvidence::Absent,
    crate::capabilities::DeckFeatures::CONSERVATIVE, test_day());
            let b = apply_sync_report(&state, &[], &snap, &[], DateTime::<chrono::Utc>::MAX_UTC, &CountingIds::default(), ts(20_000),
                &crate::test_support::test_connector(),
                &crate::messages::AppDataEvidence::Absent,
    crate::capabilities::DeckFeatures::CONSERVATIVE, test_day());
            prop_assert_eq!(a.0.tasks, b.0.tasks);
        }
        }

    // ---- self-clobber guard: ops queued after `read_at` -------------

    #[test]
    fn a_post_read_scheduled_edit_survives_the_same_cycle_pull() {
        // The D6 race, scheduled edition: the cycle pushed a title PUT
        // (advancing Deck's whole-card stamp), and the pull then reported
        // `scheduled: null` stamped newer than the local edit's clock.
        // The edit was queued after the cycle's read — the push never
        // carried it — so the merge must keep value AND clock.
        let mut local = bound_task(1);
        local.scheduled = Some(ts(BASE + 120));
        local.clocks.scheduled = ts(BASE + 120);
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE + 120),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE), // the cycle read before the edit was queued
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(
            merged.scheduled,
            Some(ts(BASE + 120)),
            "protected intent survives"
        );
        assert_eq!(merged.clocks.scheduled, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_done_edit_survives_the_same_cycle_pull() {
        // The D6 race: the cycle planned before `SetTaskDone` executed,
        // pushed a due-date PUT (advancing Deck's whole-card stamp), and
        // the pull then reported `done: null` stamped newer than the
        // local edit's clock. The edit was queued after the cycle's read
        // — the push never carried it — so the merge must keep it.
        let mut local = bound_task(1);
        local.done = Some(ts(BASE + 120));
        local.clocks.done = ts(BASE + 120);
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE + 120),
        };
        // Remote: no done, whole-card stamp newer than the local edit's
        // clock (our own push advanced it).
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE), // the cycle read before the edit was queued
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(
            merged.done,
            Some(ts(BASE + 120)),
            "protected intent survives"
        );
        assert_eq!(merged.clocks.done, ts(BASE + 120));
        // The pending op is field-coarse (UpdateTask covers title too),
        // so the title is protected as well; the observation stamp still
        // advances.
        assert_eq!(merged.title, "local 1");
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn an_op_queued_before_the_read_is_not_protected() {
        // The cycle saw the edit (queued before `read_at`): the snapshot
        // is the post-push truth for its fields and adoption proceeds.
        let mut local = bound_task(1);
        local.done = Some(ts(BASE - 120));
        local.clocks.done = ts(BASE - 120);
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE - 60),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.done, None, "the cycle saw the op: remote truth wins");
    }

    #[test]
    fn a_post_read_delete_is_not_resurrected_away() {
        // Local deleted the task after the cycle read; the pull reports a
        // live remote card stamped newer. The tombstone must stand and
        // the pending DeleteTask must stay queued (not dropped as
        // "resurrected" — the cycle never saw it).
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 120);
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(77)),
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE + 120),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert!(merged.deleted, "post-read delete intent must survive");
        assert!(
            app.1
                .iter()
                .all(|a| !matches!(a, PersistenceAction::FailOp(id) if *id == op.op_id)),
            "the unseen delete op must stay queued"
        );
    }

    #[test]
    fn a_pre_read_delete_still_yields_to_a_newer_remote_write() {
        // The cycle saw the local delete (queued before the read) but the
        // remote moved on afterwards: R5 resurrect applies and drops the op.
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE - 120);
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(78)),
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE - 60),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert!(!merged.deleted, "R5: the newer remote write resurrects");
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == op.op_id)),
            "the seen delete op is dropped by the resurrect"
        );
    }

    #[test]
    fn a_post_read_stack_rename_survives_the_same_cycle_pull() {
        let mut local = local_stack();
        local.title = "renamed".into();
        local.clocks.title = ts(BASE + 120);
        let mut state = base_state(bound_task(1));
        state.stacks.insert(local.id, local.clone());

        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::RenameStack(local.id),
            queued_at: ts(BASE + 120),
        };
        let mut remote = remote_stack(1, BASE + 300);
        remote.title = "remote rename".into();
        let snap = crate::merge_testutil::snapshot(vec![remote], vec![]);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.stacks[&local.id].clone();
        assert_eq!(merged.title, "renamed", "protected intent survives");
        assert_eq!(merged.clocks.title, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_stack_move_survives_the_same_cycle_pull() {
        // A MoveStack is a full-send push like a rename: the pending op
        // protects the stack's order (and title) from the same-cycle
        // pull whose stamps advanced with the previous cycle's writes.
        let mut local = local_stack();
        local.order = 9;
        local.clocks.order = ts(BASE + 120);
        let mut state = base_state(bound_task(1));
        state.stacks.insert(local.id, local.clone());

        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::MoveStack(local.id),
            queued_at: ts(BASE + 120),
        };
        let mut remote = remote_stack(1, BASE + 300);
        remote.order = 2;
        let snap = crate::merge_testutil::snapshot(vec![remote], vec![]);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.stacks[&local.id].clone();
        assert_eq!(merged.order, 9, "protected move intent survives");
        assert_eq!(merged.clocks.order, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_board_rename_survives_the_same_cycle_pull() {
        let mut local = crate::merge_testutil::base_board();
        local.title = "locally renamed".into();
        local.clocks.title = ts(BASE + 120);
        let mut state = base_state(bound_task(1));
        state.boards.insert(local.id, local.clone());

        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::UpdateBoard(local.id),
            queued_at: ts(BASE + 120),
        };
        let mut remote = crate::merge_testutil::remote_board(REMOTE_BOARD_NUM, BASE + 300);
        remote.title = "remote rename".into();
        let snap = single_stack_snapshot(vec![], BASE + 300);
        let mut snap = snap;
        // Rebuild the snapshot's board with the remote title.
        snap.board = remote;

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = app.0.boards[&local.id].clone();
        assert_eq!(merged.title, "locally renamed", "protected intent survives");
        assert_eq!(merged.clocks.title, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_label_edit_survives_the_same_cycle_pull() {
        let mut state = base_state(bound_task(1));
        let label_id = LabelId::from(uuid::Uuid::from_u128(950));
        let mut local = crate::merge_testutil::bound_label(1);
        local.id = label_id;
        local.title = "local label".into();
        local.clocks.title = ts(BASE + 120);
        state.labels.insert(label_id, local.clone());

        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(9)),
            op: LocalOp::UpdateLabel(label_id),
            queued_at: ts(BASE + 120),
        };
        let mut remote = crate::merge_testutil::remote_label(1, BASE + 300);
        remote.title = "remote label".into();
        let mut snap = single_stack_snapshot(vec![], BASE + 300);
        snap.labels = vec![remote];

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.labels[&label_id];
        assert_eq!(merged.title, "local label", "protected intent survives");
        assert_eq!(merged.clocks.title, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    // ---- F3-10: the board arms of the R9 decision table ---------------

    #[test]
    fn remote_missing_board_push_tombstones_the_bound_board() {
        // Twin of `remote_missing_stack_and_label_ops_finalize_tombstones`
        // for the board arm: a 404 on the board rename push finalizes the
        // board itself (tombstone_board clears the binding); cascading to
        // the board's children belongs to the sync path, not to the push.
        let state = base_state(bound_task(1));
        let board_id = *state.boards.keys().next().unwrap();
        let stack_id = *state.stacks.keys().next().unwrap();
        let task_id = task_by_card(&state, 1).id;
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(70));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateBoard(board_id),
            queued_at: ts(BASE),
        };

        let app = apply_push_report(
            &state,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: PushResult::RemoteMissing,
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 60),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        let merged_board = &app.0.boards[&board_id];
        assert!(merged_board.deleted, "the board arm tombstones");
        assert!(merged_board.remote.is_none(), "binding cleared");
        assert!(merged_board.remote_seen.is_none());
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == op_id)),
            "the op completes"
        );
        assert!(
            !app.0.stacks[&stack_id].deleted && !app.0.tasks[&task_id].deleted,
            "R9c finalizes the entity itself; no push-path cascade"
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "no op is cancelled by the push path"
        );
    }

    #[test]
    fn board_rename_push_echo_is_adopted() {
        // Twin of `create_stack_push_echo_binds_remote` for the board arm:
        // the rename echo is adopted unconditionally with the max clock.
        let state = base_state(bound_task(1));
        let board_id = *state.boards.keys().next().unwrap();
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(71));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateBoard(board_id),
            queued_at: ts(BASE),
        };
        let echo = remote_board(REMOTE_BOARD_NUM, BASE + 300);
        let expected_ref = echo.id;

        let app = apply_push_report(
            &state,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Board(echo)),
                },
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 360),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        let merged = &app.0.boards[&board_id];
        assert_eq!(merged.title, "board 77", "echo adopted unconditionally");
        assert_eq!(
            merged.clocks.title,
            ts(BASE + 300),
            "clock is max(local, echo stamp)"
        );
        assert_eq!(
            merged.remote,
            crate::test_support::test_binding(expected_ref),
            "the echo preserves the local board's binding"
        );
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == op_id)),
        );
    }

    #[test]
    fn board_delete_push_echo_finalizes_the_tombstone_and_clears_the_binding() {
        // The delete echo must never adopt `deleted: false`
        // (`adopt_board_after_push` is off-limits for deletes).
        let mut board = crate::merge_testutil::base_board();
        board.deleted = true;
        board.clocks.deleted = ts(BASE + 120);
        let state = base_state(bound_task(1));
        let board_id = board.id;
        let mut state = state;
        state.boards.insert(board_id, board);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(72));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::DeleteBoard(board_id),
            queued_at: ts(BASE),
        };
        let echo = remote_board(REMOTE_BOARD_NUM, BASE + 300);

        let app = apply_push_report(
            &state,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Board(echo)),
                },
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 360),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        let merged = &app.0.boards[&board_id];
        assert!(merged.deleted, "the tombstone is preserved");
        assert_eq!(merged.clocks.deleted, ts(BASE + 300));
        assert!(merged.remote.is_none(), "the binding clears");
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == op_id)),
        );
    }

    #[test]
    fn remote_missing_on_a_board_delete_push_tombstones_the_board() {
        let state = base_state(bound_task(1));
        let board_id = *state.boards.keys().next().unwrap();
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(73));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::DeleteBoard(board_id),
            queued_at: ts(BASE),
        };

        let app = apply_push_report(
            &state,
            std::slice::from_ref(&op),
            &[PushOutcome {
                op: op_id,
                result: PushResult::RemoteMissing,
            }],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 360),
            crate::capabilities::DeckFeatures::CONSERVATIVE,
        );

        assert!(app.0.boards[&board_id].deleted);
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == op_id)),
        );
    }

    // ---- F3-12: the apply_sync_report guard conjunction differentials -

    #[test]
    fn a_live_stack_merge_does_not_re_cascade_its_tasks() {
        // The (false, false) arm of the newly-tombstoned detection: an
        // ordinary changed-but-live stack merge must not enter the
        // cascade list — its tasks stay live and their ops untouched.
        let state = base_state(bound_task(1));
        let stack_id = *state.stacks.keys().next().unwrap();
        let task_id = task_by_card(&state, 1).id;
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(80));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::UpdateTask(task_id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            DateTime::<chrono::Utc>::MAX_UTC,
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert!(
            !app.0.stacks[&stack_id].deleted,
            "the live merge must not tombstone the stack"
        );
        assert!(
            !app.0.tasks[&task_id].deleted,
            "a live-stack merge must not cascade its tasks"
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(_))),
            "no op cancellation without a tombstone"
        );
    }

    #[test]
    fn a_standing_tombstone_keeps_its_pending_delete_queued() {
        // The tombstone-stands arm of the R5 drop-window: the local
        // tombstone is newer than the remote write, so nothing
        // resurrects and the pre-read DeleteTask op stays queued to
        // re-push (twin of
        // `a_pre_read_delete_still_yields_to_a_newer_remote_write`).
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300);
        let state = base_state(local);
        let op_id = crate::outbox::OpId(uuid::Uuid::from_u128(81));
        let op = crate::outbox::PendingOp {
            op_id,
            op: LocalOp::DeleteTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        // Newer than remote_seen (the merge runs) but older than the
        // tombstone clock (the tombstone stands).
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 180)], BASE + 180);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let final_state = app_into_state(&app);
        let merged = task_by_card(&final_state, 1);
        assert!(merged.deleted, "the standing tombstone is not resurrected");
        assert_eq!(
            merged.remote,
            Some(card_ref(1, 1)),
            "R9c pre-push semantics keep the binding"
        );
        assert_eq!(merged.remote_seen, Some(ts(BASE + 180)));
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) | PersistenceAction::CompleteOp(id) if *id == op_id)),
            "the delete op stays queued untouched"
        );
        assert_eq!(app.0.sync.aggregate().pending_ops, 1);
    }

    #[test]
    fn a_push_settled_op_verdict_survives_absence_cancellation() {
        // First verdict wins: an op this cycle's push echo already
        // settled must stay CompleteOp when the same task is then
        // absence-tombstoned; only still-pending ops of the task are
        // failed by R3. The tombstone route runs through cascade
        // territory (the card's stack is absent too): the #130 carve-
        // out counts a card this cycle's echo carried as present, so a
        // live-stack absence can no longer tombstone a just-echoed
        // card at all.
        let state = base_state(bound_task(1));
        let task_id = task_by_card(&state, 1).id;
        let echo_op_id = crate::outbox::OpId(uuid::Uuid::from_u128(82));
        let pending_op_id = crate::outbox::OpId(uuid::Uuid::from_u128(83));
        let outbox = [
            crate::outbox::PendingOp {
                op_id: echo_op_id,
                op: LocalOp::UpdateTask(task_id),
                queued_at: ts(BASE),
            },
            crate::outbox::PendingOp {
                op_id: pending_op_id,
                op: LocalOp::MoveTask(task_id),
                queued_at: ts(BASE),
            },
        ];
        let mut echo = remote_task(1, 1, BASE + 60);
        echo.title = "echoed".into();
        let snap = snapshot(vec![], vec![]);

        let app = apply_sync_report(
            &state,
            &outbox,
            &snap,
            &[PushOutcome {
                op: echo_op_id,
                result: PushResult::Applied {
                    echo: Some(RemoteEcho::Task(echo)),
                },
            }],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 120),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.tasks[&task_id];
        assert!(merged.deleted, "the absence tombstones the task");
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::CompleteOp(id) if *id == echo_op_id)),
            "the pushed op keeps its first (complete) verdict"
        );
        assert!(
            !app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == echo_op_id)),
        );
        assert!(
            app.1
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == pending_op_id)),
            "the still-pending op of the tombstoned task is cancelled"
        );
    }

    // ---- F3-08/F3-12: intent-quartet arms, boundaries, id-scoping ----
    // Boundary convention: `queued_at == read_at` is a *pre-read* op —
    // remote truth wins (the same `<=` the four guards share).

    #[test]
    fn a_post_read_task_move_survives_the_same_cycle_pull() {
        let mut local = bound_task(1);
        local.order = 9;
        local.clocks.position = ts(BASE + 120);
        let state = base_state(local);
        let stack_before = task_by_card(&state, 1).stack;
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(90)),
            op: LocalOp::MoveTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE + 120),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE + 300)], BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let final_state = app_into_state(&app);
        let merged = task_by_card(&final_state, 1);
        assert_eq!(merged.stack, stack_before, "protected stack half");
        assert_eq!(merged.order, 9, "protected order half");
        assert_eq!(merged.clocks.position, ts(BASE + 120));
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_label_assignment_survives_the_same_cycle_pull() {
        let mut state = base_state(bound_task(1));
        let label = bound_label(5);
        let label_id = label.id;
        state.labels.insert(label_id, label);
        let mut local = bound_task(1);
        local.label_memberships.insert(
            label_id,
            crate::entities::MembershipState::Added { at: ts(BASE + 120) },
        );
        state.tasks.insert(local.id, local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(91)),
            op: LocalOp::AssignLabel(task_by_card(&state, 1).id, label_id),
            queued_at: ts(BASE + 120),
        };
        // The snapshot must still list the label: entity-death dominance
        // would otherwise (correctly) kill the queued assignment.
        let snap = crate::remote::RemoteBoardSnapshot {
            board: remote_board(REMOTE_BOARD_NUM, BASE + 300),
            stacks: vec![remote_stack(1, BASE + 300)],
            tasks: vec![remote_task(1, 1, BASE + 300)],
            labels: vec![remote_label(5, BASE + 300)],
        };

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let final_state = app_into_state(&app);
        let merged = task_by_card(&final_state, 1);
        assert_eq!(
            merged.live_labels(),
            BTreeSet::from([label_id]),
            "protected set"
        );
        assert_eq!(
            merged.label_memberships[&label_id],
            crate::entities::MembershipState::Added { at: ts(BASE + 120) }
        );
        assert_eq!(merged.remote_seen, Some(ts(BASE + 300)));
    }

    #[test]
    fn a_post_read_stack_delete_survives_the_same_cycle_pull() {
        // The live remote entry would resurrect; the unseen local delete
        // intent must stand (the pull never carried it).
        let mut local = local_stack();
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 120);
        let mut state = base_state(bound_task(1));
        state.stacks.insert(local.id, local.clone());
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(92)),
            op: LocalOp::DeleteStack(local.id),
            queued_at: ts(BASE + 120),
        };
        let snap = snapshot(vec![remote_stack(1, BASE + 300)], vec![]);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.stacks[&local.id];
        assert!(merged.deleted, "post-read delete intent survives");
        assert_eq!(merged.clocks.deleted, ts(BASE + 120));
    }

    #[test]
    fn a_post_read_label_delete_survives_the_same_cycle_pull() {
        let mut local = bound_label(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 120);
        let mut state = base_state(bound_task(1));
        state.labels.insert(local.id, local.clone());
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(93)),
            op: LocalOp::DeleteLabel(local.id),
            queued_at: ts(BASE + 120),
        };
        let mut snap = single_stack_snapshot(vec![], BASE + 300);
        snap.labels = vec![remote_label(1, BASE + 300)];

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let merged = &app.0.labels[&local.id];
        assert!(merged.deleted, "post-read delete intent survives");
        assert_eq!(merged.clocks.deleted, ts(BASE + 120));
    }

    #[test]
    fn an_op_queued_exactly_at_read_at_is_not_protected_task() {
        // Equality is pre-read: the cycle saw the op, so the snapshot is
        // the post-push truth for its fields.
        let mut local = bound_task(1);
        local.done = Some(ts(BASE));
        let state = base_state(local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(94)),
            op: LocalOp::UpdateTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        // The snapshot must still list the label: entity-death dominance
        // would otherwise (correctly) kill the queued assignment.
        let snap = crate::remote::RemoteBoardSnapshot {
            board: remote_board(REMOTE_BOARD_NUM, BASE + 300),
            stacks: vec![remote_stack(1, BASE + 300)],
            tasks: vec![remote_task(1, 1, BASE + 300)],
            labels: vec![remote_label(5, BASE + 300)],
        };

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        let final_state = app_into_state(&app);
        assert_eq!(
            task_by_card(&final_state, 1).done,
            None,
            "remote truth wins at the boundary"
        );
    }

    #[test]
    fn an_op_queued_exactly_at_read_at_is_not_protected_stack() {
        let mut local = local_stack();
        local.title = "renamed locally".into();
        local.clocks.title = ts(BASE);
        let mut state = base_state(bound_task(1));
        state.stacks.insert(local.id, local.clone());
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(95)),
            op: LocalOp::RenameStack(local.id),
            queued_at: ts(BASE),
        };
        let mut remote = remote_stack(1, BASE + 300);
        remote.title = "remote rename".into();
        let snap = snapshot(vec![remote], vec![]);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.stacks[&local.id].title, "remote rename",
            "remote truth wins at the boundary"
        );
    }

    #[test]
    fn an_op_queued_exactly_at_read_at_is_not_protected_label() {
        let mut state = base_state(bound_task(1));
        let local = bound_label(1);
        let label_id = local.id;
        state.labels.insert(label_id, local);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(96)),
            op: LocalOp::UpdateLabel(label_id),
            queued_at: ts(BASE),
        };
        let mut snap = single_stack_snapshot(vec![], BASE + 300);
        snap.labels = vec![remote_label(1, BASE + 300)];

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.labels[&label_id].title, "label 1 renamed",
            "remote truth wins at the boundary"
        );
    }

    #[test]
    fn an_op_queued_exactly_at_read_at_is_not_protected_board() {
        let state = base_state(bound_task(1));
        let board_id = *state.boards.keys().next().unwrap();
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(97)),
            op: LocalOp::UpdateBoard(board_id),
            queued_at: ts(BASE),
        };
        let mut snap = single_stack_snapshot(vec![], BASE + 300);
        snap.board = remote_board(REMOTE_BOARD_NUM, BASE + 300);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.boards[&board_id].title, "board 77",
            "remote truth wins at the boundary"
        );
    }

    #[test]
    fn a_post_read_stack_op_protects_only_its_own_stack() {
        let stack1 = local_stack();
        let mut stack2 = local_stack();
        stack2.id = StackId::from(uuid::Uuid::from_u128(901));
        stack2.remote = Some(stack_ref(2));
        stack2.title = "stack 2".into();
        stack2.order = 2;
        let mut state = base_state(bound_task(1));
        state.stacks.insert(stack1.id, stack1.clone());
        state.stacks.insert(stack2.id, stack2.clone());
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(98)),
            op: LocalOp::RenameStack(stack1.id),
            queued_at: ts(BASE + 120),
        };
        let mut remote1 = remote_stack(1, BASE + 300);
        remote1.title = "remote one".into();
        let mut remote2 = remote_stack(2, BASE + 300);
        remote2.title = "remote two".into();
        let snap = snapshot(vec![remote1, remote2], vec![]);

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.stacks[&stack1.id].title, "stack 1",
            "the op's own stack is protected"
        );
        assert_eq!(
            app.0.stacks[&stack2.id].title, "remote two",
            "the other stack adopts the remote title"
        );
    }

    #[test]
    fn a_post_read_label_op_protects_only_its_own_label() {
        let mut state = base_state(bound_task(1));
        let label1 = bound_label(1);
        let label2 = bound_label(2);
        let (label1_id, label2_id) = (label1.id, label2.id);
        state.labels.insert(label1_id, label1);
        state.labels.insert(label2_id, label2);
        let op = crate::outbox::PendingOp {
            op_id: crate::outbox::OpId(uuid::Uuid::from_u128(99)),
            op: LocalOp::UpdateLabel(label1_id),
            queued_at: ts(BASE + 120),
        };
        let mut snap = single_stack_snapshot(vec![], BASE + 300);
        snap.labels = vec![remote_label(1, BASE + 300), remote_label(2, BASE + 300)];

        let app = apply_sync_report(
            &state,
            std::slice::from_ref(&op),
            &snap,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 360),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::CONSERVATIVE,
            test_day(),
        );

        assert_eq!(
            app.0.labels[&label1_id].title, "label 1",
            "the op's own label is protected"
        );
        assert_eq!(
            app.0.labels[&label2_id].title, "label 2 renamed",
            "the other label adopts the remote title"
        );
    }
    // ---- DR-5/2: the time-entries fold (D-T12's position pins) ------

    #[test]
    fn the_pipeline_folds_time_entries_after_dismissals() {
        use crate::appdata::{DismissalEntry, dismissal_wire_key};
        use crate::ids::ReminderRuleId;
        let task = bound_task(1);
        let state = base_state(task);
        let dismissed_rule = ReminderRuleId::from(uuid::Uuid::from_u128(0xa));
        let occurrence = ts(BASE + 1_000);
        let dismissal_entry = crate::appdata::DismissalsEntry {
            dismissals: [(
                dismissal_wire_key(dismissed_rule, occurrence),
                DismissalEntry::Dismissed {
                    dismissed_at: ts(BASE + 200),
                },
            )]
            .into(),
            unknown: std::collections::BTreeMap::new(),
        };
        let entry_uuid = uuid::Uuid::from_u128(0x20);
        let mut evidence_doc =
            match document_with_dismissals(&[], None, Some(&[(1, dismissal_entry)])) {
                AppDataEvidence::Document(doc) => *doc,
                _ => unreachable!(),
            };
        evidence_doc.sections.time_entries = Some(
            [(entry_uuid.to_string(), wire_entry(Some("1"), BASE + 500))]
                .into_iter()
                .collect(),
        );
        let evidence = AppDataEvidence::Document(Box::new(evidence_doc));
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE),
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        // Both the dismissal row and the entry landed (the position pin:
        // the entry fold runs after dismissals in the same pass).
        assert_eq!(app.reminders.dismissals.len(), 1);
        let task_id = task_by_card(&app, 1).id;
        assert_eq!(app.tracking.entries.len(), 1);
        let folded = &app.tracking.entries[&crate::ids::TimeEntryId::from(entry_uuid)];
        assert_eq!(folded.task, task_id);
        assert_eq!(folded.updated_at, ts(BASE + 500));
        // The batch carries the entry upsert (batch and view identical
        // by construction).
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, PersistenceAction::UpsertTimeEntry(_))),
        );
    }

    #[test]
    fn an_entry_whose_card_vanishes_this_cycle_still_folds() {
        let task = bound_task(1);
        let state = base_state(task);
        let entry_uuid = uuid::Uuid::from_u128(0x21);
        let evidence =
            document_with_time_entries(&[(entry_uuid, wire_entry(Some("1"), BASE + 500))]);
        // The snapshot carries no cards at all — R3 will tombstone the
        // bound task this same cycle, but the fold resolves against the
        // pre-R3 card map.
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &single_stack_snapshot(vec![], BASE),
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        assert_eq!(app.tracking.entries.len(), 1, "history outlives the card");
        assert!(
            actions
                .iter()
                .any(|action| matches!(action, PersistenceAction::UpsertTimeEntry(_))),
        );
        let vanished = &app.tasks[&crate::ids::TaskId::from(uuid::Uuid::from_u128(1))];
        assert!(
            vanished.deleted && vanished.remote.is_none(),
            "R3 still tombstoned the vanished card's task (binding cleared)"
        );
    }

    #[test]
    fn a_null_task_wire_row_is_skipped_entirely() {
        let task = bound_task(1);
        let state = base_state(task);
        let evidence = document_with_time_entries(&[(
            uuid::Uuid::from_u128(0x22),
            wire_entry(None, BASE + 500),
        )]);
        let (app, actions, _events) = fold_report(
            &state,
            &[],
            &single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE),
            &evidence,
            DeckFeatures::CONSERVATIVE,
            BASE + 3_600,
        );
        assert!(
            app.tracking.entries.is_empty(),
            "neither imported nor deleted"
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, PersistenceAction::UpsertTimeEntry(_))),
        );
    }

    // ---- DR-6/3: §5c — the series fold dedup through the pipeline ----

    /// A bound, open, recur-bearing twin candidate: id `id_seed` (the
    /// u128 order *is* the v7 creation order), card `card`, scheduled on
    /// 1970-01-02 (the UTC day every test calendar reads).
    fn series_twin(id_seed: u128, series_seed: u128, card: u64, scheduled: Option<i64>) -> Task {
        let mut task = bound_task(0);
        task.id = crate::ids::TaskId::from(uuid::Uuid::from_u128(id_seed));
        task.remote = Some(card_ref(1, card));
        task.remote_seen = Some(ts(BASE));
        task.title = format!("twin {id_seed}");
        task.recur = Some(crate::recur::RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(series_seed)),
            anchor: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            pattern: crate::recur::RecurPattern::Daily {
                every: crate::recur::EveryDays::new(1).unwrap(),
            },
            bound: crate::recur::RecurBound::None,
            continued_from: None,
        });
        task.scheduled = scheduled.map(ts);
        task
    }

    fn day1() -> i64 {
        86_400 // 1970-01-02, UTC midnight calendar
    }

    fn twin_state(older: Task, newer: Task) -> AppState {
        let mut state = base_state(older);
        state.tasks.insert(newer.id, newer);
        state
    }

    fn run_sync(
        state: &AppState,
        outbox: &[PendingOp],
        snapshot: &RemoteBoardSnapshot,
    ) -> (
        AppState,
        Vec<PersistenceAction>,
        Vec<crate::messages::SystemEvent>,
    ) {
        apply_sync_report(
            state,
            outbox,
            snapshot,
            &[],
            ts(BASE),
            &CountingIds::default(),
            ts(BASE + 3_600),
            &crate::test_support::test_connector(),
            &AppDataEvidence::Absent,
            DeckFeatures::CONSERVATIVE,
            test_day(),
        )
    }

    fn enqueued_ops(actions: &[PersistenceAction]) -> Vec<LocalOp> {
        actions
            .iter()
            .filter_map(|action| match action {
                PersistenceAction::EnqueueOp(pending) => Some(pending.op),
                _ => None,
            })
            .collect()
    }

    /// T-5a/T-5f: the canonical late race — both twins long-bound with
    /// independent clocks, no create signature anywhere. §4b structurally
    /// cannot fire; §5c collapses once: the older-created survivor
    /// absorbs the doomed twin field-wise, the twin is neutralized as a
    /// ref-less tombstone, its card is ref-deleted, the survivor's lanes
    /// re-arm, and no `SystemEvent` rides the collapse.
    #[test]
    fn the_series_race_collapses_to_the_older_created_survivor() {
        let mut older = series_twin(100, 7, 10, Some(day1()));
        let mut newer = series_twin(200, 7, 20, Some(day1()));
        // The doomed twin's newer title and label must win the merge;
        // the survivor's newer effort must stand.
        newer.title = "the doomed twin's newer title".to_owned();
        newer.clocks.title = ts(BASE + 200);
        newer.label_memberships.insert(
            crate::ids::LabelId::from(uuid::Uuid::from_u128(9)),
            crate::entities::MembershipState::Added { at: ts(BASE + 50) },
        );
        older.effort = Some(crate::entities::Effort::new(3));
        older.clocks.effort = ts(BASE + 300);
        let state = twin_state(older, newer);
        let snap = single_stack_snapshot(
            vec![remote_task(1, 10, BASE), remote_task(1, 20, BASE)],
            BASE,
        );

        let (app, actions, events) = run_sync(&state, &[], &snap);

        assert!(events.is_empty(), "the collapse is tracing-only");
        let survivor = app
            .tasks
            .get(&crate::ids::TaskId::from(uuid::Uuid::from_u128(100)))
            .unwrap();
        assert!(!survivor.deleted);
        assert_eq!(survivor.title, "the doomed twin's newer title");
        assert_eq!(survivor.clocks.title, ts(BASE + 200));
        assert_eq!(survivor.effort, Some(crate::entities::Effort::new(3)));
        assert!(
            matches!(
                survivor
                    .label_memberships
                    .get(&crate::ids::LabelId::from(uuid::Uuid::from_u128(9))),
                Some(crate::entities::MembershipState::Added { .. })
            ),
            "the doomed twin's label OR-folds onto the survivor"
        );
        assert_eq!(
            survivor.remote,
            Some(card_ref(1, 10)),
            "the survivor keeps its own binding trio"
        );

        let doomed = app
            .tasks
            .get(&crate::ids::TaskId::from(uuid::Uuid::from_u128(200)))
            .unwrap();
        assert!(doomed.deleted, "the twin is neutralized");
        assert_eq!(doomed.remote, None, "the tombstone is ref-less");
        assert_eq!(doomed.remote_seen, None);
        assert_eq!(doomed.remote_baseline, None);
        assert_eq!(
            doomed.clocks.deleted,
            ts(BASE + 3_600),
            "the tombstone clock floors at now"
        );

        let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(800));
        let ops = enqueued_ops(&actions);
        assert!(
            ops.contains(&LocalOp::DeleteRemoteCard {
                board,
                card: card_ref(1, 20),
            }),
            "the doomed card is ref-deleted: {ops:?}"
        );
        let doomed_id = crate::ids::TaskId::from(uuid::Uuid::from_u128(200));
        let survivor_id = crate::ids::TaskId::from(uuid::Uuid::from_u128(100));
        assert!(
            ops.contains(&LocalOp::UpdateTask(survivor_id))
                && ops.contains(&LocalOp::UpdateAppData(survivor_id)),
            "the survivor's lanes re-arm: {ops:?}"
        );
        assert!(
            !ops.iter()
                .any(|op| matches!(op, LocalOp::UpdateTask(t) if *t == doomed_id)
                    || matches!(op, LocalOp::UpdateAppData(t) if *t == doomed_id)),
            "the doomed twin's lanes never re-arm"
        );
    }

    /// T-5b: the unbound arm — the survivor is the adopted remote twin,
    /// the doomed row has a pending `CreateTask` and no card. The op
    /// completes `true` (the board phase stays `Idle`), the row
    /// neutralizes without a ref-delete.
    #[test]
    fn the_dominated_twin_s_pending_create_completes_without_a_failure_stamp() {
        let older = series_twin(100, 7, 10, Some(day1()));
        let mut newer = series_twin(200, 7, 20, Some(day1()));
        newer.remote = None; // the race's unbound half: created offline, push pending
        newer.remote_seen = None;
        let doomed_id = newer.id;
        let state = twin_state(older, newer);
        let create_op = PendingOp {
            op_id: OpId(uuid::Uuid::from_u128(555)),
            op: LocalOp::CreateTask(doomed_id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 10, BASE)], BASE);

        let (app, actions, events) = run_sync(&state, &[create_op], &snap);

        assert_eq!(*events, []);
        let doomed = app
            .tasks
            .get(&crate::ids::TaskId::from(uuid::Uuid::from_u128(200)))
            .unwrap();
        assert!(doomed.deleted && doomed.remote.is_none());
        let ops = enqueued_ops(&actions);
        let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(800));
        assert!(
            !ops.iter()
                .any(|op| matches!(op, LocalOp::DeleteRemoteCard { .. })),
            "no card to delete: {ops:?}"
        );
        assert!(
            actions.contains(&PersistenceAction::CompleteOp(OpId(uuid::Uuid::from_u128(
                555
            )))),
            "the collapse-resolved create completes true"
        );
        assert!(
            !actions.contains(&PersistenceAction::FailOp(OpId(uuid::Uuid::from_u128(555)))),
            "never a dead letter"
        );
        let row = app.sync.get(board).expect("the bound board has a sync row");
        assert!(
            !matches!(row.phase, crate::state::SyncPhase::Failed { .. }),
            "the board never stamps Failed"
        );
    }

    /// T-5c/T-5d: the false-positive fences through the full pipeline —
    /// same series different dates (the reopen-while-successor-lives
    /// shape), same date different series, a done twin, a dateless pair.
    /// Zero structural effects anywhere.
    #[test]
    fn the_dedup_fences_hold_through_the_full_pipeline() {
        let id = |n: u128| crate::ids::TaskId::from(uuid::Uuid::from_u128(n));
        let scenarios: Vec<(Task, Task)> = vec![
            // Reopened occurrence + its successor: same series, other day.
            (
                series_twin(100, 7, 10, Some(day1())),
                series_twin(200, 7, 20, Some(day1() + 86_400)),
            ),
            // Same day, different series.
            (
                series_twin(100, 7, 10, Some(day1())),
                series_twin(200, 8, 20, Some(day1())),
            ),
            // One done twin.
            {
                let mut t = series_twin(200, 7, 20, Some(day1()));
                t.done = Some(ts(BASE + 10));
                (series_twin(100, 7, 10, Some(day1())), t)
            },
            // Dateless pair (unkeyed).
            (series_twin(100, 7, 10, None), series_twin(200, 7, 20, None)),
        ];
        for (older, newer) in scenarios {
            let state = twin_state(older, newer);
            let snap = single_stack_snapshot(
                vec![remote_task(1, 10, BASE), remote_task(1, 20, BASE)],
                BASE,
            );
            let (app, actions, events) = run_sync(&state, &[], &snap);
            assert_eq!(*events, []);
            assert!(
                enqueued_ops(&actions).is_empty(),
                "no lane ops without a twin"
            );
            for id_seed in [100_u128, 200] {
                let before = state.tasks.get(&id(id_seed)).unwrap();
                let after = app.tasks.get(&id(id_seed)).unwrap();
                assert_eq!(before, after, "task {id_seed} untouched");
            }
        }
    }

    /// T-5g: idempotence — a second cycle over the same world collapses
    /// nothing (the neutralized twin is no longer a candidate), and a
    /// re-adopted doomed card (the ref-delete still in flight) collapses
    /// again to the same survivor.
    #[test]
    fn the_pass_is_idempotent_and_self_healing_over_re_adoption() {
        let older = series_twin(100, 7, 10, Some(day1()));
        let newer = series_twin(200, 7, 20, Some(day1()));
        let state = twin_state(older, newer);
        let snap = single_stack_snapshot(
            vec![remote_task(1, 10, BASE), remote_task(1, 20, BASE)],
            BASE,
        );
        let (app, _actions, _events) = run_sync(&state, &[], &snap);

        // Cycle 2: the tombstoned twin is invisible.
        let (app2, actions2, _) = run_sync(&app, &[], &snap);
        assert!(enqueued_ops(&actions2).is_empty(), "no second collapse");
        assert!(
            app2.tasks
                .get(&crate::ids::TaskId::from(uuid::Uuid::from_u128(200)))
                .unwrap()
                .deleted
        );

        // Re-adoption: the doomed card re-pulled while the ref-delete
        // drains — the row is bound and open again; the same survivor
        // wins and the pass fires once more.
        let mut readopted = series_twin(200, 7, 20, Some(day1()));
        readopted.title = "the doomed twin's newer title".to_owned();
        let mut state3 = app2.clone();
        state3.tasks.insert(readopted.id, readopted.clone());
        let (app3, actions3, _) = run_sync(&state3, &[], &snap);
        let survivor = app3
            .tasks
            .get(&crate::ids::TaskId::from(uuid::Uuid::from_u128(100)))
            .unwrap();
        assert_eq!(survivor.title, "the doomed twin's newer title");
        assert!(app3.tasks.get(&readopted.id).unwrap().deleted);
        let board = crate::ids::BoardId::from(uuid::Uuid::from_u128(800));
        assert!(
            enqueued_ops(&actions3).contains(&LocalOp::DeleteRemoteCard {
                board,
                card: card_ref(1, 20),
            })
        );
    }
}

#[cfg(test)]
mod duplicate_reconcile_tests {
    //! T-7: the collapse composed end-to-end through
    //! [`apply_sync_report`] — the echo arm's rebind + delete push, the
    //! pending arm's op completion, the stranger's-twin negative, the
    //! stack variant with its survivorship inversion, and the honesty
    //! pin (no `SystemEvent` on the collapse path).

    use super::*;
    use crate::merge_testutil::*;
    use uuid::Uuid;

    /// The UTC-midnight calendar every pipeline test injects (plain
    /// offset dates; the dedup key never consults the window start).
    fn test_day() -> crate::days::DayCalendar {
        crate::days::DayCalendar {
            day_start_time: chrono::NaiveTime::MIN,
            offset: chrono::FixedOffset::east_opt(0).expect("zero is a valid offset"),
        }
    }

    const NOW: i64 = BASE + 30;

    /// A matching card observation: title normalizes to the bound
    /// fixture's, content equal (order differs — the excluded field).
    fn twin(stack: u64, card: u64, created_at: i64) -> RemoteTask {
        let mut view = remote_task(stack, card, created_at);
        view.id = card_ref(stack, card);
        view.title = "LOCAL 800".into();
        view.created_at = ts(created_at);
        view
    }

    fn echo_push(op_num: u128, echo: RemoteTask) -> PushOutcome {
        PushOutcome {
            op: OpId(Uuid::from_u128(op_num)),
            result: PushResult::Applied {
                echo: Some(RemoteEcho::Task(echo)),
            },
        }
    }

    fn run(
        state: &AppState,
        outbox: &[PendingOp],
        snapshot: &RemoteBoardSnapshot,
        pushes: &[PushOutcome],
    ) -> (
        AppState,
        Vec<PersistenceAction>,
        Vec<crate::messages::SystemEvent>,
    ) {
        apply_sync_report(
            state,
            outbox,
            snapshot,
            pushes,
            ts(BASE + 1),
            &CountingIds::default(),
            ts(NOW),
            &crate::test_support::test_connector(),
            &crate::messages::AppDataEvidence::Absent,
            crate::capabilities::DeckFeatures::NATIVE,
            test_day(),
        )
    }

    fn enqueued_ref_deletes(actions: &[PersistenceAction]) -> Vec<LocalOp> {
        actions
            .iter()
            .filter_map(|a| match a {
                PersistenceAction::EnqueueOp(p) => Some(p.op),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_echo_arm_rebinds_to_the_older_card_and_dooms_the_newer() {
        // Crash journey's replay cycle: the replayed create's echo bound
        // T to the NEWER card B; the listing also carries the first
        // POST's older card A — the collapse rebinds T→A and ref-deletes B.
        let a = twin(1, 900, BASE + 10);
        let b = twin(1, 800, BASE + 20);
        let task = bound_task(800); // bound to B
        let state = base_state(task);
        let create_op = PendingOp {
            op_id: OpId(Uuid::from_u128(700)),
            op: LocalOp::CreateTask(task_by_card(&state, 800).id),
            queued_at: ts(BASE),
        };
        let (app, actions, events) = run(
            &state,
            &[create_op],
            &single_stack_snapshot(vec![a.clone(), b.clone()], BASE),
            &[echo_push(700, b.clone())],
        );

        // Zero new task rows: B's card is bound, A's adoption suppressed.
        assert_eq!(app.tasks.len(), 1, "no duplicate local row");
        let merged = task_by_card(&app, 900);
        assert_eq!(merged.remote, Some(a.id), "rebound to the older card");
        assert_eq!(merged.remote_seen, Some(a.last_modified));
        assert!(
            merged.remote_baseline.is_some(),
            "the rebind refreshes the baseline (D-1c)"
        );
        // One ref-delete enqueue for the doomed newer card.
        let deletes = enqueued_ref_deletes(&actions);
        assert_eq!(deletes.len(), 1);
        assert!(matches!(
            &deletes[0],
            LocalOp::DeleteRemoteCard { card, .. } if *card == b.id
        ));
        // The create op completed via its echo (§1), the phase stays Idle.
        assert!(actions.iter().any(
            |a| matches!(a, PersistenceAction::CompleteOp(id) if *id == OpId(Uuid::from_u128(700)))
        ));
        assert_eq!(*events, [], "the honesty rule: no SystemEvent");
    }

    #[test]
    fn the_pending_arm_completes_the_create_and_rebinds() {
        // Landing batch lost, pull succeeded on a later cycle: T still
        // unbound, the first POST's card A in the listing, no echo.
        let a = twin(1, 900, BASE + 10);
        let task = {
            let mut task = bound_task(800);
            task.remote = None;
            task.remote_seen = None;
            task
        };
        let state = base_state(task);
        let create_op = PendingOp {
            op_id: OpId(Uuid::from_u128(700)),
            op: LocalOp::CreateTask(TaskId::from(Uuid::from_u128(800))),
            queued_at: ts(BASE),
        };
        let (app, actions, events) = run(
            &state,
            &[create_op],
            &single_stack_snapshot(vec![a.clone()], BASE),
            &[],
        );

        assert_eq!(app.tasks.len(), 1);
        let merged = app.tasks.get(&TaskId::from(Uuid::from_u128(800))).unwrap();
        assert_eq!(merged.remote, Some(a.id), "rebound to the survivor");
        assert!(
            enqueued_ref_deletes(&actions).is_empty(),
            "the survivor is the only card"
        );
        // DEC-6: the create completes true — the board phase stays Idle.
        assert!(actions.iter().any(
            |a| matches!(a, PersistenceAction::CompleteOp(id) if *id == OpId(Uuid::from_u128(700)))
        ));
        assert_eq!(app.sync.aggregate().phase, crate::state::SyncPhase::Idle);
        assert_eq!(*events, []);
    }

    /// T-5e: the composition pin — a recur-bearing race twin with a
    /// create signature collapses in §4b (the rebind + `CompleteOp`
    /// path); §5c fires nothing afterward. The two passes are disjoint
    /// by trigger class: after §4b only one open task carries the key.
    #[test]
    fn a_crash_window_twin_collapses_in_4b_and_the_series_pass_stays_silent() {
        let day1 = 86_400; // 1970-01-02, UTC
        let mut a = twin(1, 900, BASE + 10);
        a.scheduled = Some(ts(day1)); // content-equal clause 4 under NATIVE
        let mut task = {
            let mut task = bound_task(800);
            task.remote = None;
            task.remote_seen = None;
            task
        };
        task.recur = Some(crate::recur::RecurRule {
            series: crate::ids::SeriesId::from_uuid(uuid::Uuid::from_u128(7)),
            anchor: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap(),
            pattern: crate::recur::RecurPattern::Daily {
                every: crate::recur::EveryDays::new(1).unwrap(),
            },
            bound: crate::recur::RecurBound::None,
            continued_from: None,
        });
        task.scheduled = Some(ts(day1));
        let state = base_state(task);
        let create_op = PendingOp {
            op_id: OpId(Uuid::from_u128(700)),
            op: LocalOp::CreateTask(TaskId::from(Uuid::from_u128(800))),
            queued_at: ts(BASE),
        };
        let (app, actions, events) = run(
            &state,
            &[create_op],
            &single_stack_snapshot(vec![a.clone()], BASE),
            &[],
        );

        // §4b fired: the rebind + the create completing true.
        assert_eq!(app.tasks.len(), 1, "exactly one survivor");
        let merged = app.tasks.get(&TaskId::from(Uuid::from_u128(800))).unwrap();
        assert_eq!(merged.remote, Some(a.id));
        assert!(actions.iter().any(
            |action| matches!(action, PersistenceAction::CompleteOp(id) if *id == OpId(Uuid::from_u128(700)))
        ));
        // §5c stayed silent: no ref-delete, no survivor lane op, no event.
        assert_eq!(enqueued_ref_deletes(&actions), []);
        assert!(
            !actions.iter().any(|action| matches!(
                action,
                PersistenceAction::EnqueueOp(pending)
                    if matches!(pending.op, LocalOp::UpdateTask(_) | LocalOp::UpdateAppData(_))
            )),
            "the series pass never re-arms lanes without a twin"
        );
        assert_eq!(*events, []);
    }

    #[test]
    fn a_strangers_twin_card_is_adopted_normally() {
        // Clause 2 through the pipeline: no create in scope ⇒ the pass
        // cannot fire; the twin adopts as its own task (today's behavior).
        let a = twin(1, 900, BASE + 10);
        let state = base_state(bound_task(800));
        let (app, actions, _) = run(
            &state,
            &[],
            &single_stack_snapshot(vec![a.clone()], BASE),
            &[],
        );
        assert_eq!(app.tasks.len(), 2, "the twin R1-adopts unchanged");
        assert_eq!(task_by_card(&app, 900).remote, Some(a.id));
        assert_eq!(enqueued_ref_deletes(&actions), []);
    }

    #[test]
    fn the_stack_variant_deletes_the_orphan_and_keeps_the_bound_stack() {
        // DEC-3's inversion: the bound stack survives (it holds the
        // cards), the unbound same-titled orphan gets a ref-delete.
        let mut orphan = remote_stack(2, BASE + 10);
        orphan.title = "stack 1".into();
        orphan.order = 1; // clause 4: the create body's order matches
        let state = base_state(bound_task(800));
        let stack_id = local_stack().id;
        let create_op = PendingOp {
            op_id: OpId(Uuid::from_u128(710)),
            op: LocalOp::CreateStack(stack_id),
            queued_at: ts(BASE),
        };
        let (app, actions, events) = run(
            &state,
            &[create_op],
            &snapshot(vec![remote_stack(1, BASE), orphan.clone()], vec![]),
            &[],
        );
        assert_eq!(app.stacks.len(), 1, "the orphan's adoption is suppressed");
        let deletes = enqueued_ref_deletes(&actions);
        assert_eq!(deletes.len(), 1);
        assert!(matches!(
            &deletes[0],
            LocalOp::DeleteRemoteStack { stack, .. } if *stack == orphan.id
        ));
        assert_eq!(*events, []);
    }

    #[test]
    fn an_orphan_holding_a_bound_card_is_left_alone() {
        // The never-by-construction guard (DEC-3): the orphan's nested
        // card resolves to a live bound local task — the collapse refuses.
        let mut orphan = remote_stack(2, BASE + 10);
        orphan.title = "stack 1".into();
        orphan.order = 1;
        let mut nested = twin(2, 950, BASE + 10);
        nested.title = "nested".into();
        let mut task = bound_task(800);
        task.remote = Some(nested.id); // a live binding into the orphan
        task.title = "nested".into();
        let mut state = base_state(task);
        // The fixture board's stack binding is ref 1; point the task's
        // stack row at a second local stack bound to the orphan's ref.
        let orphan_stack_row = Stack {
            id: StackId::from(Uuid::from_u128(901)),
            remote: Some(stack_ref(2)),
            board: BoardId::from(Uuid::from_u128(800)),
            title: "stack 1".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: crate::entities::StackClocks {
                title: ts(BASE),
                order: ts(BASE),
                deleted: ts(BASE),
            },
            remote_seen: Some(ts(BASE)),
        };
        state.stacks.insert(orphan_stack_row.id, orphan_stack_row);
        let (app, actions, _) = run(
            &state,
            &[],
            &snapshot(
                vec![remote_stack(1, BASE), orphan],
                vec![twin(2, 950, BASE + 10)],
            ),
            &[],
        );
        assert_eq!(
            app.stacks.len(),
            2,
            "the contested orphan is adopted, not collapsed"
        );
        assert!(
            enqueued_ref_deletes(&actions)
                .iter()
                .all(|op| !matches!(op, LocalOp::DeleteRemoteStack { .. })),
            "no stack delete fires for a contested orphan"
        );
    }
}
