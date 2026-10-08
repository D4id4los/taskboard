// SPDX-License-Identifier: MIT OR Apache-2.0
//! The sync conflict policy: pure, total, clock-free merge functions and
//! the [`apply_sync_report`] pipeline the state engine runs once per
//! `SyncReport::Completed`.
//!
//! Normative rules (ADR 0004):
//!
//! - **Per-field LWW**: a remote field value wins iff
//!   `remote.last_modified > field_clock` (strictly); ties keep local.
//! - **Deletion**: boards/stacks/labels carry a real remote `deleted_at`
//!   and LWW against the local tombstone clock. Cards have no remote
//!   delete timestamp, so an observed remote absence always wins
//!   (delete-wins fallback, R3).
//! - **Labels** merge as one field (whole-set LWW); **position**
//!   (`stack` + `order`) is one composite field.
//! - **Push echoes** are adopted unconditionally (the server normalizes
//!   what we just wrote) with clocks advanced to
//!   `max(local, echo.last_modified)`.
//!
//! All comparisons are cross-machine wall clock; see ADR 0004's skew
//! disclaimer. Tests use logical times and are skew-free by construction.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{DateTime, Utc};

use crate::clock::IdGenerator;
use crate::entities::{Board, Label, Stack, Task};
use crate::ids::{
    LabelId, RemoteBoardId, RemoteCardRef, RemoteLabelId, RemoteStackId, StackId, TaskId,
};
use crate::ops::{LocalOp, PendingOp};
use crate::persistence::PersistenceAction;
use crate::remote::{
    PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot, RemoteEcho, RemoteIndex,
    RemoteLabel, RemoteStack, RemoteTask, remote_index, resolve_label,
};
use crate::state::{AppState, SyncErrorKind, SyncPhase, SyncStatus};

/// Nil placeholder stack id for a task whose remote stack cannot be
/// resolved (total function requirement; the pipeline adopts stacks
/// before tasks, so callers with a complete index never hit this).
fn nil_stack_id() -> StackId {
    StackId::from(uuid::Uuid::nil())
}

/// Smallest representable `DateTime<Utc>`: never-newer than anything.
fn utc_min() -> DateTime<Utc> {
    DateTime::<Utc>::MIN_UTC
}

/// Remote value wins iff strictly newer; ties keep local.
fn remote_wins(remote_ts: DateTime<Utc>, field_clock: DateTime<Utc>) -> bool {
    remote_ts > field_clock
}

fn max_ts(a: DateTime<Utc>, b: DateTime<Utc>) -> DateTime<Utc> {
    if a > b { a } else { b }
}

/// Builds the lookup index for the current bound entities.
#[must_use]
pub fn state_remote_index(state: &AppState) -> RemoteIndex {
    remote_index(
        state
            .tasks
            .iter()
            .filter_map(|(id, t)| Some((t.remote?, *id))),
        state
            .stacks
            .iter()
            .filter_map(|(id, s)| Some((s.remote?, *id))),
        state
            .labels
            .iter()
            .filter_map(|(id, l)| Some((l.remote?, *id))),
    )
}

/// R1 — adopt a remotely-created task under a fresh local id. All clocks
/// are stamped with the remote `last_modified`; remote label ids are
/// resolved through `ctx` (unmapped labels are dropped here; the pipeline
/// adopts the label entities before the tasks, so unmapped means the
/// label was already absent remotely).
#[must_use]
pub fn adopt_remote_task(remote: &RemoteTask, id: TaskId, ctx: &RemoteIndex) -> Task {
    Task {
        id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        description: remote.description.clone(),
        duedate: remote.duedate,
        done: remote.done,
        stack: resolve_local_stack(ctx, remote.id.board, remote.stack).unwrap_or_else(nil_stack_id),
        order: remote.order,
        labels: map_remote_labels(ctx, remote.id.board, &remote.labels),
        archived: remote.archived,
        deleted: false,
        clocks: clocks_from(remote.last_modified),
        remote_seen: Some(remote.last_modified),
    }
}

fn clocks_from(ts: DateTime<Utc>) -> crate::entities::TaskClocks {
    crate::entities::TaskClocks {
        title: ts,
        description: ts,
        duedate: ts,
        done: ts,
        position: ts,
        labels: ts,
        archived: ts,
        deleted: ts,
    }
}

fn resolve_local_stack(
    ctx: &RemoteIndex,
    board: RemoteBoardId,
    stack: RemoteStackId,
) -> Option<StackId> {
    ctx.stack_by_ref
        .get(&crate::ids::RemoteStackRef { board, stack })
        .copied()
}

fn map_remote_labels(
    ctx: &RemoteIndex,
    board: RemoteBoardId,
    remote: &BTreeSet<RemoteLabelId>,
) -> BTreeSet<LabelId> {
    remote
        .iter()
        .filter_map(|rl| resolve_label(ctx, board, *rl))
        .collect()
}

/// R4/R5/R7/R8 — per-field LWW merge of a bound task against a changed
/// remote snapshot entry (`remote.last_modified > local.remote_seen` is
/// the caller's responsibility; unchanged remotes never reach this).
///
/// The `deleted` field participates like any other: a local tombstone is
/// resurrected iff the remote write is strictly newer than the tombstone
/// (R5); otherwise the tombstone stands and its pending delete re-pushes.
#[must_use]
pub fn merge_task(local: &Task, remote: &RemoteTask, ctx: &RemoteIndex) -> Task {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.description) {
        merged.description.clone_from(&remote.description);
        merged.clocks.description = ts_r;
    }
    if remote_wins(ts_r, local.clocks.duedate) {
        merged.duedate = remote.duedate;
        merged.clocks.duedate = ts_r;
    }
    if remote_wins(ts_r, local.clocks.done) {
        merged.done = remote.done;
        merged.clocks.done = ts_r;
    }
    // R8: position is one composite field.
    if remote_wins(ts_r, local.clocks.position)
        && let Some(stack) = resolve_local_stack(ctx, remote.id.board, remote.stack)
    {
        merged.stack = stack;
        merged.order = remote.order;
        merged.clocks.position = ts_r;
    }
    // R7: the label set is one field; unmapped remote labels are dropped
    // (the pipeline adopts label entities first).
    if remote_wins(ts_r, local.clocks.labels) {
        merged.labels = map_remote_labels(ctx, remote.id.board, &remote.labels);
        merged.clocks.labels = ts_r;
    }
    if remote_wins(ts_r, local.clocks.archived) {
        merged.archived = remote.archived;
        merged.clocks.archived = ts_r;
    }

    // R5, `deleted`: resurrect iff the remote write is strictly newer.
    if local.deleted && remote_wins(ts_r, local.clocks.deleted) {
        merged.deleted = false;
        merged.clocks.deleted = ts_r;
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R3 — observed remote absence of a bound card: delete-wins fallback.
/// The tombstone's write time is the *observation* time (`observed_at`,
/// i.e. "now" passed in by the engine); the remote binding is cleared.
#[must_use]
pub fn tombstone_task(local: &Task, observed_at: DateTime<Utc>) -> Task {
    let mut tombstoned = local.clone();
    tombstoned.deleted = true;
    tombstoned.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    tombstoned.remote = None;
    tombstoned.remote_seen = None;
    tombstoned
}

/// R9a/R9b — unconditional echo adoption after a create/update push. The
/// server normalizes what we just wrote, so values are taken from the
/// echo as-is (NOT LWW) and clocks advance to
/// `max(local, echo.last_modified)` — a seconds-resolution echo can never
/// be "newer" than the just-made local edit, yet its normalization must
/// land.
#[must_use]
pub fn adopt_after_push(local: &Task, echo: &RemoteTask, ctx: &RemoteIndex) -> Task {
    let lm = echo.last_modified;
    Task {
        id: local.id,
        remote: Some(echo.id),
        title: echo.title.clone(),
        description: echo.description.clone(),
        duedate: echo.duedate,
        done: echo.done,
        stack: resolve_local_stack(ctx, echo.id.board, echo.stack).unwrap_or(local.stack),
        order: echo.order,
        labels: map_remote_labels(ctx, echo.id.board, &echo.labels),
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::TaskClocks {
            title: max_ts(local.clocks.title, lm),
            description: max_ts(local.clocks.description, lm),
            duedate: max_ts(local.clocks.duedate, lm),
            done: max_ts(local.clocks.done, lm),
            position: max_ts(local.clocks.position, lm),
            labels: max_ts(local.clocks.labels, lm),
            archived: max_ts(local.clocks.archived, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R9c — the delete push was accepted: finalize the tombstone, clear the
/// remote binding. The next pull cannot resurrect: the tombstone clock is
/// `>=` any stale listing entry (cache-lag safe by construction).
#[must_use]
pub fn finalize_pushed_delete(local: &Task) -> Task {
    let mut finalized = local.clone();
    finalized.deleted = true;
    finalized.remote = None;
    finalized.remote_seen = None;
    finalized
}

/// R6 — merge a bound stack against a changed remote entry. The remote
/// `deleted_at` is a timestamped fact and LWWs against the local
/// tombstone clock exactly like the card `deleted` field (R5).
#[must_use]
pub fn merge_stack(local: &Stack, remote: &RemoteStack) -> Stack {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.order) {
        merged.order = remote.order;
        merged.clocks.order = ts_r;
    }
    // No local archived commands yet: adopt remote content on change.
    merged.archived = remote.archived;

    // The local side defends with its latest local activity: a rename
    // newer than the remote delete stamp keeps the stack live (worked
    // example 5), while an untouched stack loses to the delete.
    let local_latest = max_ts(
        max_ts(local.clocks.title, local.clocks.order),
        local.clocks.deleted,
    );
    match remote.deleted_at {
        Some(deleted_at) if remote_wins(deleted_at, local_latest) => {
            merged.deleted = true;
            merged.clocks.deleted = deleted_at;
        }
        _ if local.deleted && remote_wins(ts_r, local.clocks.deleted) => {
            // Remote is live again (or live and newer than our tombstone).
            merged.deleted = false;
            merged.clocks.deleted = ts_r;
        }
        _ => {}
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R1 for stacks: adopt a remotely-created stack under a fresh local id.
#[must_use]
pub fn adopt_remote_stack(remote: &RemoteStack, id: StackId, board: crate::ids::BoardId) -> Stack {
    Stack {
        id,
        remote: Some(remote.id),
        board,
        title: remote.title.clone(),
        order: remote.order,
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::StackClocks {
            title: remote.last_modified,
            order: remote.last_modified,
            deleted: remote.deleted_at.unwrap_or(remote.last_modified),
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R9a for stacks: unconditional echo adoption after create/rename push.
#[must_use]
pub fn adopt_stack_after_push(local: &Stack, echo: &RemoteStack) -> Stack {
    let lm = echo.last_modified;
    Stack {
        id: local.id,
        remote: Some(echo.id),
        board: local.board,
        title: echo.title.clone(),
        order: echo.order,
        archived: echo.archived,
        deleted: false,
        clocks: crate::entities::StackClocks {
            title: max_ts(local.clocks.title, lm),
            order: max_ts(local.clocks.order, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R6 — merge a bound label against a changed remote entry.
#[must_use]
pub fn merge_label(local: &Label, remote: &RemoteLabel) -> Label {
    let ts_r = remote.last_modified;
    let mut merged = local.clone();

    if remote_wins(ts_r, local.clocks.title) {
        merged.title.clone_from(&remote.title);
        merged.clocks.title = ts_r;
    }
    if remote_wins(ts_r, local.clocks.color) {
        merged.color = crate::entities::Color::new(remote.color.clone());
        merged.clocks.color = ts_r;
    }

    // Same defense as stacks: the latest local activity beats an older
    // remote soft delete.
    let local_latest = max_ts(
        max_ts(local.clocks.title, local.clocks.color),
        local.clocks.deleted,
    );
    match remote.deleted_at {
        Some(deleted_at) if remote_wins(deleted_at, local_latest) => {
            merged.deleted = true;
            merged.clocks.deleted = deleted_at;
        }
        _ if local.deleted && remote_wins(ts_r, local.clocks.deleted) => {
            merged.deleted = false;
            merged.clocks.deleted = ts_r;
        }
        _ => {}
    }

    merged.remote = Some(remote.id);
    merged.remote_seen = Some(ts_r);
    merged
}

/// R1 for labels: adopt a remotely-created label under a fresh local id.
#[must_use]
pub fn adopt_remote_label(remote: &RemoteLabel, id: LabelId, board: crate::ids::BoardId) -> Label {
    Label {
        id,
        remote: Some(remote.id),
        board,
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        deleted: remote.deleted_at.is_some(),
        clocks: crate::entities::LabelClocks {
            title: remote.last_modified,
            color: remote.last_modified,
            deleted: remote.deleted_at.unwrap_or(remote.last_modified),
        },
        remote_seen: Some(remote.last_modified),
    }
}

/// R9a for labels: unconditional echo adoption after create/update push.
#[must_use]
pub fn adopt_label_after_push(local: &Label, echo: &RemoteLabel) -> Label {
    let lm = echo.last_modified;
    Label {
        id: local.id,
        remote: Some(echo.id),
        board: local.board,
        title: echo.title.clone(),
        color: crate::entities::Color::new(echo.color.clone()),
        deleted: false,
        clocks: crate::entities::LabelClocks {
            title: max_ts(local.clocks.title, lm),
            color: max_ts(local.clocks.color, lm),
            deleted: max_ts(local.clocks.deleted, lm),
        },
        remote_seen: Some(lm),
    }
}

/// R6 for boards: adopt changed remote board content. Boards have no
/// local-edit commands in the MVP, so the remote content is authoritative
/// whenever it is strictly newer than `remote_seen`.
#[must_use]
pub fn merge_board(local: &Board, remote: &RemoteBoard) -> Board {
    if !remote_wins(
        remote.last_modified,
        local.remote_seen.unwrap_or_else(utc_min),
    ) {
        return local.clone();
    }
    Board {
        id: local.id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        remote_seen: Some(remote.last_modified),
    }
}

/// R1 for boards: adopt a remotely-created board under a fresh local id.
#[must_use]
pub fn adopt_remote_board(remote: &RemoteBoard, id: crate::ids::BoardId) -> Board {
    Board {
        id,
        remote: Some(remote.id),
        title: remote.title.clone(),
        color: crate::entities::Color::new(remote.color.clone()),
        archived: remote.archived,
        deleted: remote.deleted_at.is_some(),
        remote_seen: Some(remote.last_modified),
    }
}

/// Does `op` target `task`?
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

/// The pipeline result the engine applies: new entity maps, the new sync
/// status, and the persistence batch (upserts + op transitions).
#[derive(Debug, Clone, PartialEq)]
pub struct SyncApplication {
    /// Merged boards.
    pub boards: BTreeMap<crate::ids::BoardId, Board>,
    /// Merged stacks.
    pub stacks: BTreeMap<StackId, Stack>,
    /// Merged tasks.
    pub tasks: BTreeMap<TaskId, Task>,
    /// Merged labels.
    pub labels: BTreeMap<LabelId, Label>,
    /// Resulting sync status (phase transition + outbox depth).
    pub sync: SyncStatus,
    /// Persistence batch for the storage actor.
    pub actions: Vec<PersistenceAction>,
}

/// The pipeline the engine calls once per `SyncReport::Completed`.
///
/// Processing order is fixed (and tested): **push outcomes first** (they
/// establish new baselines), then board → stacks → labels → tasks (R1
/// before R7 resolution), then presence reconciliation (R3/R6) for
/// bound-but-absent entities, then the sync-status transition.
///
/// `ids` provides fresh local ids for remotely-created entities (R1);
/// `now` is the observation time for absence tombstones. Neither is ever
/// read by the per-entity merge primitives.
#[allow(clippy::too_many_lines)] // one fixed-order pipeline; splitting would obscure it
pub fn apply_sync_report(
    current: &AppState,
    outbox: &[PendingOp],
    snapshot: &RemoteBoardSnapshot,
    pushes: &[PushOutcome],
    ids: &dyn IdGenerator,
    now: DateTime<Utc>,
) -> SyncApplication {
    let mut boards = current.boards.clone();
    let mut stacks = current.stacks.clone();
    let mut tasks = current.tasks.clone();
    let mut labels = current.labels.clone();

    // Op bookkeeping: op_id -> (index into outbox, op). Values: true =
    // completed, false = failed/cancelled; None = untouched.
    let mut op_results: HashMap<crate::ops::OpId, bool> = HashMap::new();

    // ---- 1. Push outcomes (R9*) ------------------------------------
    let mut ctx = state_remote_index(current);
    let op_by_id: HashMap<crate::ops::OpId, &PendingOp> =
        outbox.iter().map(|op| (op.op_id, op)).collect();

    for outcome in pushes {
        let Some(op) = op_by_id.get(&outcome.op) else {
            continue;
        };
        match &outcome.result {
            PushResult::Applied { echo } => match echo {
                Some(RemoteEcho::Task(echo)) => {
                    if let LocalOp::CreateTask(id) | LocalOp::UpdateTask(id) = op.op
                        && let Some(local) = tasks.get(&id)
                    {
                        let merged = adopt_after_push(local, echo, &ctx);
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
            },
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
                        stacks.insert(id, finalize_stack_tombstone(local, now));
                    }
                    op_results.insert(outcome.op, true);
                }
                LocalOp::CreateLabel(id) | LocalOp::UpdateLabel(id) | LocalOp::DeleteLabel(id) => {
                    if let Some(local) = labels.get(&id) {
                        labels.insert(id, finalize_label_tombstone(local, now));
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

    // ---- 2. Board ---------------------------------------------------
    let mut board_ids_by_remote: HashMap<RemoteBoardId, crate::ids::BoardId> = current
        .boards
        .iter()
        .filter_map(|(id, b)| Some((b.remote?, *id)))
        .collect();
    let remote_board = &snapshot.board;
    let board_id = if let Some(id) = board_ids_by_remote.get(&remote_board.id).copied() {
        let local = &boards[&id];
        let merged = merge_board(local, remote_board);
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
    let mut live_stack_refs = BTreeSet::new();
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
            if !merged_is_deleted(&stacks[&id]) {
                live_stack_refs.insert(remote.id.stack);
            }
        } else {
            let id = ids.new_stack_id();
            stacks.insert(id, adopt_remote_stack(remote, id, board_id));
            ctx.stack_by_ref.insert(remote.id, id);
            if !stacks[&id].deleted {
                live_stack_refs.insert(remote.id.stack);
            }
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
            let id = ids.new_task_id();
            tasks.insert(id, adopt_remote_task(remote, id, &ctx));
            ctx.task_by_ref.insert(remote.id, id);
        }
    }

    // ---- 6. Presence reconciliation (R3) ----------------------------
    // Bound-but-absent entities are gone server-side (snapshot
    // completeness contract). Cards: delete-wins fallback.
    let present_refs: std::collections::BTreeSet<RemoteCardRef> =
        snapshot.tasks.iter().map(|t| t.id).collect();
    for (id, local) in tasks.clone() {
        let Some(remote_ref) = local.remote else {
            continue;
        };
        if !present_refs.contains(&remote_ref) {
            tasks.insert(id, tombstone_task(&local, now));
            for other in outbox {
                if !op_results.contains_key(&other.op_id) && op_targets_task(&other.op, id) {
                    op_results.insert(other.op_id, false);
                }
            }
        }
    }
    let present_stack_refs: std::collections::BTreeSet<crate::ids::RemoteStackRef> =
        snapshot.stacks.iter().map(|s| s.id).collect();
    for (id, local) in stacks.clone() {
        let Some(remote_ref) = local.remote else {
            continue;
        };
        if !present_stack_refs.contains(&remote_ref) && !local.deleted {
            stacks.insert(id, finalize_stack_tombstone(&local, now));
            newly_tombstoned_stack_refs.push(remote_ref.stack);
        }
    }
    let present_label_refs: std::collections::BTreeSet<crate::ids::RemoteLabelRef> =
        snapshot.labels.iter().map(|l| l.id).collect();
    for (id, local) in labels.clone() {
        let Some(remote_ref) = local.remote else {
            continue;
        };
        if !present_label_refs.contains(&remote_ref) && !local.deleted {
            labels.insert(id, finalize_label_tombstone(&local, now));
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

    let remaining_ops = outbox.len() - op_results.len();
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

    SyncApplication {
        boards,
        stacks,
        tasks,
        labels,
        sync,
        actions,
    }
}

fn merged_is_deleted(stack: &Stack) -> bool {
    stack.deleted
}

fn finalize_stack_tombstone(local: &Stack, observed_at: DateTime<Utc>) -> Stack {
    let mut s = local.clone();
    s.deleted = true;
    s.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    s.remote = None;
    s.remote_seen = None;
    s
}

fn finalize_label_tombstone(local: &Label, observed_at: DateTime<Utc>) -> Label {
    let mut l = local.clone();
    l.deleted = true;
    l.clocks.deleted = max_ts(local.clocks.deleted, observed_at);
    l.remote = None;
    l.remote_seen = None;
    l
}

fn cascade_board(
    board_id: &crate::ids::BoardId,
    stacks: &mut BTreeMap<StackId, Stack>,
    tasks: &mut BTreeMap<TaskId, Task>,
    labels: &mut BTreeMap<LabelId, Label>,
    now: DateTime<Utc>,
    outbox: &[PendingOp],
    op_results: &mut HashMap<crate::ops::OpId, bool>,
) {
    let stack_ids: Vec<StackId> = stacks
        .iter()
        .filter(|(_, s)| s.board == *board_id && s.deleted)
        .map(|(id, _)| *id)
        .collect();
    for id in stack_ids {
        let stack = &stacks[&id];
        stacks.insert(id, finalize_stack_tombstone(stack, now));
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
        labels.insert(id, finalize_label_tombstone(label, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::Color;
    use crate::entities::{StackClocks, TaskClocks};
    use crate::ids::{RemoteStackId, StackId, TaskId};
    use chrono::TimeZone;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(secs, 0).unwrap()
    }

    /// Deterministic id source: uuids from a counter.
    #[derive(Debug, Default)]
    struct CountingIds(std::sync::atomic::AtomicU64);
    impl CountingIds {
        fn next(&self) -> u128 {
            u128::from(self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1)
        }
    }
    impl IdGenerator for CountingIds {
        fn new_board_id(&self) -> crate::ids::BoardId {
            unreachable!("MVP binds an existing board; no board adoption ids")
        }
        fn new_stack_id(&self) -> StackId {
            StackId::from(uuid::Uuid::from_u128(self.next()))
        }
        fn new_task_id(&self) -> TaskId {
            TaskId::from(uuid::Uuid::from_u128(self.next()))
        }
        fn new_label_id(&self) -> LabelId {
            LabelId::from(uuid::Uuid::from_u128(self.next()))
        }
        fn new_op_id(&self) -> crate::ops::OpId {
            crate::ops::OpId(uuid::Uuid::from_u128(self.next()))
        }
    }

    const BASE: i64 = 36_000; // 10:00:00Z
    const REMOTE_BOARD: u64 = 77;

    fn board(num: u64) -> crate::ids::RemoteBoardId {
        crate::ids::RemoteBoardId(num)
    }

    fn card_ref(stack: u64, card: u64) -> RemoteCardRef {
        RemoteCardRef {
            board: board(REMOTE_BOARD),
            stack: RemoteStackId(stack),
            card: crate::ids::RemoteCardId(card),
        }
    }

    fn stack_ref(stack: u64) -> crate::ids::RemoteStackRef {
        crate::ids::RemoteStackRef {
            board: board(REMOTE_BOARD),
            stack: RemoteStackId(stack),
        }
    }

    fn remote_stack(num: u64, last_modified: i64) -> RemoteStack {
        RemoteStack {
            id: stack_ref(num),
            title: format!("stack {num}"),
            order: num.cast_signed(),
            archived: false,
            deleted_at: None,
            last_modified: ts(last_modified),
        }
    }

    fn remote_task(stack: u64, card: u64, last_modified: i64) -> RemoteTask {
        RemoteTask {
            id: card_ref(stack, card),
            title: format!("card {card}"),
            description: String::new(),
            duedate: None,
            done: None,
            stack: RemoteStackId(stack),
            order: card.cast_signed(),
            labels: BTreeSet::new(),
            archived: false,
            last_modified: ts(last_modified),
        }
    }

    fn clocks_at(t: i64) -> TaskClocks {
        TaskClocks {
            title: ts(t),
            description: ts(t),
            duedate: ts(t),
            done: ts(t),
            position: ts(t),
            labels: ts(t),
            archived: ts(t),
            deleted: ts(t),
        }
    }

    fn bound_task(card: u64) -> Task {
        let id = TaskId::from(uuid::Uuid::from_u128(u128::from(card)));
        let stack_id = StackId::from(uuid::Uuid::from_u128(900));
        Task {
            id,
            remote: Some(card_ref(1, card)),
            title: format!("local {card}"),
            description: String::new(),
            duedate: None,
            done: None,
            stack: stack_id,
            order: card.cast_signed(),
            labels: BTreeSet::new(),
            archived: false,
            deleted: false,
            clocks: clocks_at(BASE),
            remote_seen: Some(ts(BASE)),
        }
    }

    fn local_stack() -> Stack {
        Stack {
            id: StackId::from(uuid::Uuid::from_u128(900)),
            remote: Some(stack_ref(1)),
            board: crate::ids::BoardId::from(uuid::Uuid::from_u128(800)),
            title: "stack 1".into(),
            order: 1,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: ts(BASE),
                order: ts(BASE),
                deleted: ts(BASE),
            },
            remote_seen: Some(ts(BASE)),
        }
    }

    fn base_state(task: Task) -> AppState {
        let board_id = crate::ids::BoardId::from(uuid::Uuid::from_u128(800));
        let mut state = AppState::default();
        state.boards.insert(
            board_id,
            Board {
                id: board_id,
                remote: Some(board(REMOTE_BOARD)),
                title: "board".into(),
                color: Color::new("ff0000"),
                archived: false,
                deleted: false,
                remote_seen: Some(ts(BASE)),
            },
        );
        let stack = local_stack();
        state.stacks.insert(stack.id, stack);
        state.tasks.insert(task.id, task);
        state
    }

    fn snapshot(stacks: Vec<RemoteStack>, tasks: Vec<RemoteTask>) -> RemoteBoardSnapshot {
        RemoteBoardSnapshot {
            board: RemoteBoard {
                id: board(REMOTE_BOARD),
                title: "board".into(),
                color: "ff0000".into(),
                archived: false,
                deleted_at: None,
                last_modified: ts(BASE),
            },
            stacks,
            tasks,
            labels: Vec::new(),
        }
    }

    fn single_stack_snapshot(tasks: Vec<RemoteTask>, last_modified: i64) -> RemoteBoardSnapshot {
        snapshot(vec![remote_stack(1, last_modified)], tasks)
    }

    fn task_by_card(state: &AppState, card: u64) -> &Task {
        state
            .tasks
            .values()
            .find(|t| t.remote == Some(card_ref(1, card)))
            .unwrap()
    }

    // ---- §7.4 example 1: disjoint field edits both survive ------------
    #[test]
    fn disjoint_field_edits_both_survive() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let mut remote = remote_task(1, 1, ts_millis(BASE + 180)); // 10:03:00
        remote.description = "edited remotely".into();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert_eq!(merged.title, "edited locally");
        assert_eq!(merged.description, "edited remotely");
    }

    fn ts_millis(secs: i64) -> i64 {
        secs
    }

    // ---- §7.4 example 2: granularity trap (documented loss) -----------
    #[test]
    fn granularity_trap_remote_entity_touch_overwrites_local_title() {
        let mut local = bound_task(1);
        local.title = "edited locally".into();
        local.clocks.title = ts(BASE + 330); // 10:05:30
        let remote = remote_task(1, 1, BASE + 360); // 10:06:00, description edit

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        // The unchanged remote title wins over the newer local edit —
        // Deck's entity-level granularity. Pinned as intentional (ADR 0004).
        assert_eq!(merged.title, remote.title);
        assert_eq!(merged.description, remote.description);
    }

    // ---- Tie-break: strict `>`, ties keep local ------------------------
    #[test]
    fn tie_keeps_local_every_field() {
        let mut local = bound_task(1);
        local.title = "local title".into();
        local.description = "local description".into();
        local.order = 42;
        local.archived = true;
        let mut remote = remote_task(1, 1, BASE); // ts_r == every clock
        remote.title = "remote title".into();
        remote.description = "remote description".into();
        remote.order = 99;
        remote.archived = false;

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert_eq!(merged.title, "local title");
        assert_eq!(merged.description, "local description");
        assert_eq!(merged.order, 42);
        assert!(merged.archived);
    }

    // ---- §7.4 example 3: remote absence → delete-wins (R3) -------------
    #[test]
    fn remote_absence_tombstones_and_cancels_pending_ops() {
        let task = bound_task(1);
        let state = base_state(task);
        let op = crate::ops::PendingOp {
            op_id: crate::ops::OpId(uuid::Uuid::from_u128(555)),
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

        let merged = app.tasks.values().next().unwrap();
        assert!(merged.deleted, "R3: delete-wins on observed absence");
        assert!(merged.remote.is_none());
        assert!(merged.remote_seen.is_none());
        assert!(
            app.actions
                .iter()
                .any(|a| matches!(a, PersistenceAction::FailOp(id) if *id == crate::ops::OpId(uuid::Uuid::from_u128(555)))),
            "the task's pending ops must be cancelled"
        );
    }

    // ---- §7.4 example 4: local delete vs remote edit, both directions --
    #[test]
    fn newer_remote_edit_resurrects_local_tombstone() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300); // delete at 10:05
        let remote = remote_task(1, 1, BASE + 360); // remote edit at 10:06

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert!(!merged.deleted, "newer remote edit resurrects");
        assert_eq!(merged.remote, Some(card_ref(1, 1)));
    }

    #[test]
    fn older_remote_edit_keeps_tombstone() {
        let mut local = bound_task(1);
        local.deleted = true;
        local.clocks.deleted = ts(BASE + 300); // delete at 10:05
        let remote = remote_task(1, 1, BASE + 180); // remote edit at 10:03

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        assert!(merged.deleted, "older remote edit cannot resurrect");
    }

    // ---- §7.4 example 5: stack soft delete is a timestamped fact (R6) --
    #[test]
    fn newer_local_rename_beats_older_stack_delete() {
        let mut local = local_stack();
        local.title = "renamed".into();
        local.clocks.title = ts(BASE + 480); // 10:08
        let mut remote = remote_stack(1, BASE + 420); // 10:07
        remote.deleted_at = Some(ts(BASE + 420));

        let merged = merge_stack(&local, &remote);
        assert!(!merged.deleted, "rename is newer than the soft delete");
        assert_eq!(merged.title, "renamed");
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
        let merged_task = app.tasks.values().next().unwrap();
        assert!(merged_task.deleted, "stack tombstone cascades to its tasks");
    }

    // ---- §7.4 example 6: echo normalization (R9a) ----------------------
    #[test]
    fn echo_is_adopted_unconditionally_with_max_clocks() {
        let mut local = bound_task(1);
        local.remote = None;
        local.title = "  untidied  ".into();
        local.clocks = clocks_at(BASE + 3_599); // 10:59:59 sub-second edits
        let mut echo = remote_task(1, 1, BASE + 3_600); // 11:00:00
        echo.title = "untidied".into(); // server-trimmed

        let ctx = RemoteIndex::default();
        let merged = adopt_after_push(&local, &echo, &ctx);

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
            app.actions
                .iter()
                .all(|a| !matches!(a, PersistenceAction::UpsertTask(_))),
            "pull must not undo the echo adoption"
        );
    }

    // ---- R9d: reorder push with no echo completes the op ----------------
    #[test]
    fn reorder_push_completes_without_touching_state() {
        let task = bound_task(1);
        let state = base_state(task);
        let op = crate::ops::PendingOp {
            op_id: crate::ops::OpId(uuid::Uuid::from_u128(1)),
            op: LocalOp::MoveTask(task_by_card(&state, 1).id),
            queued_at: ts(BASE),
        };
        let snap = single_stack_snapshot(vec![remote_task(1, 1, BASE)], BASE);

        let app = apply_sync_report(
            &state,
            &[op],
            &snap,
            &[PushOutcome {
                op: crate::ops::OpId(uuid::Uuid::from_u128(1)),
                result: PushResult::Applied { echo: None },
            }],
            &CountingIds::default(),
            ts(BASE + 60),
        );

        assert_eq!(app.sync.pending_ops, 0, "reorder op completes");
        let state_after = app_into_state(&app);
        let merged = task_by_card(&state_after, 1);
        assert_eq!(merged.order, 1, "baseline untouched until the next pull");
    }

    fn app_into_state(app: &SyncApplication) -> AppState {
        AppState {
            boards: app.boards.clone(),
            stacks: app.stacks.clone(),
            tasks: app.tasks.clone(),
            labels: app.labels.clone(),
            sync: app.sync.clone(),
            last_updated: None,
        }
    }

    // ---- R9e: RemoteMissing falls through to R3 ------------------------
    #[test]
    fn remote_missing_on_update_push_tombstones() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::ops::OpId(uuid::Uuid::from_u128(2));
        let op = crate::ops::PendingOp {
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

        let merged = app.tasks.values().next().unwrap();
        assert!(merged.deleted);
    }

    // ---- R9f: Rejected stays queued; BadRequest dead-letters ------------
    #[test]
    fn rejected_op_stays_queued_but_bad_request_dead_letters() {
        let task = bound_task(1);
        let state = base_state(task);
        let op_id = crate::ops::OpId(uuid::Uuid::from_u128(3));
        let op = crate::ops::PendingOp {
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
        assert_eq!(app.sync.pending_ops, 1, "retryable rejection stays queued");
        assert_eq!(app.sync.phase, SyncPhase::Idle);

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
        assert_eq!(app.sync.pending_ops, 0, "BadRequest dead-letters");
        assert_eq!(
            app.sync.phase,
            SyncPhase::Failed {
                last_error: SyncErrorKind::BadRequest
            }
        );
    }

    // ---- R1: new remote task adopts with fresh id and mapped labels -----
    #[test]
    fn new_remote_task_adopts_with_resolved_labels() {
        let label_id = LabelId::from(uuid::Uuid::from_u128(700));
        let mut label_map = RemoteIndex::default();
        label_map.label_by_ref.insert(
            crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD),
                label: crate::ids::RemoteLabelId(5),
            },
            label_id,
        );

        let mut remote = remote_task(1, 9, BASE + 60);
        remote.labels.insert(crate::ids::RemoteLabelId(5));

        let state = base_state(bound_task(1));
        // The index for the merge comes from the state; the label binding
        // arrives through the pipeline after label adoption. Simulate the
        // post-adoption index by merging directly with the extended map.
        let adopted = adopt_remote_task(
            &remote,
            TaskId::from(uuid::Uuid::from_u128(777)),
            &label_map,
        );
        assert_eq!(adopted.labels, BTreeSet::from([label_id]));
        assert_eq!(adopted.clocks.title, remote.last_modified);
        assert_eq!(adopted.remote_seen, Some(remote.last_modified));
        assert!(!adopted.deleted);
        let _ = state;
    }

    #[test]
    fn pipeline_adopts_new_remote_entities_before_resolving_task_labels() {
        // A remote label (11) and a task referencing it, neither known
        // locally: after the pipeline, the task's label set references the
        // freshly adopted label entity.
        let mut label = RemoteLabel {
            id: crate::ids::RemoteLabelRef {
                board: board(REMOTE_BOARD),
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
            .labels
            .values()
            .find(|l| l.remote == Some(label.id))
            .expect("remote label adopted");
        let adopted_task = app
            .tasks
            .values()
            .find(|t| t.remote == Some(remote.id))
            .expect("remote task adopted");
        assert_eq!(
            adopted_task.labels,
            BTreeSet::from([adopted_label.id]),
            "R7 resolution uses the post-adoption mapping"
        );
    }

    // ---- Idempotence: re-applying the same snapshot is a no-op ----------
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
            second.actions.is_empty(),
            "second application must be a no-op, got {:?}",
            second.actions
        );
    }

    // ---- Convergence: equal replicas produce equal states ---------------
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
        prop_assert_eq!(a.tasks, b.tasks);
    }
    }

    // ---- Clock monotonicity under merge ---------------------------------
    proptest! {
    #[test]
    fn merge_never_decreases_field_clocks(
        local_ts in 0_i64..10_000,
        remote_ts in 0_i64..10_000,
        card in 1_u64..100,
    ) {
        let mut local = bound_task(card);
        local.clocks = clocks_at(local_ts);
        let remote = remote_task(1, card, remote_ts);

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);

        prop_assert!(merged.clocks.title >= local.clocks.title);
        prop_assert!(merged.clocks.description >= local.clocks.description);
        prop_assert!(merged.clocks.duedate >= local.clocks.duedate);
        prop_assert!(merged.clocks.done >= local.clocks.done);
        prop_assert!(merged.clocks.position >= local.clocks.position);
        prop_assert!(merged.clocks.labels >= local.clocks.labels);
        prop_assert!(merged.clocks.archived >= local.clocks.archived);
        prop_assert!(merged.clocks.deleted >= local.clocks.deleted);
    }
    }

    // ---- No-panic bombardment + tie-break property ----------------------
    proptest! {
    #[test]
    fn merge_is_total_and_strictly_newer_wins(
        local_title_clock in 0_i64..10_000,
        remote_ts in 0_i64..10_000,
        card in 1_u64..100,
        local_title in "[a-z]{0,16}",
        remote_title in "[a-z]{0,16}",
    ) {
        let mut local = bound_task(card);
        local.title = local_title.clone();
        local.clocks.title = ts(local_title_clock);
        let mut remote = remote_task(1, card, remote_ts);
        remote.title = remote_title.clone();

        let ctx = RemoteIndex::default();
        let merged = merge_task(&local, &remote, &ctx);
        // Total: no panic. Tie-break: strict `>` — equal timestamps keep
        // the local value (this kills `>=` mutants).
        let expected = if remote_ts > local_title_clock {
            remote_title
        } else {
            local_title
        };
        prop_assert_eq!(merged.title.clone(), expected);

        // Idempotence: merging again with the same remote changes nothing.
        let again = merge_task(&merged, &remote, &ctx);
        prop_assert_eq!(again, merged);
    }
    }

    // ---- Push-outcome-first ordering (baselines before snapshot) --------
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
        let op_id = crate::ops::OpId(uuid::Uuid::from_u128(9));
        let op = crate::ops::PendingOp {
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
        assert_eq!(app.sync.pending_ops, 0);
    }
}
