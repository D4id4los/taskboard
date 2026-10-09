// SPDX-License-Identifier: MIT OR Apache-2.0
//! Strategies for state-level shapes: the full `AppState` (referentially
//! consistent), the persisted dataset, and sync phases.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use proptest::prelude::*;

use super::entities::{label_clocks_strategy, stack_clocks_strategy, task_clocks_strategy};
use super::{
    board_id_strategy, label_id_strategy, op_id_strategy, remote_number, stack_id_strategy,
    string_strategy, task_id_strategy, timestamp_strategy,
};
use crate::entities::{Board, Color, Label, Stack, Task};
use crate::ids::{
    LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use crate::outbox::{LocalOp, PendingOp};
use crate::persistence::{PersistedState, SyncValidators, ValidatorKey};
use crate::state::{AppState, SyncErrorKind, SyncPhase, SyncStatus};

/// A remotely-created entity bound with `board_remote` (or a fixed
/// fallback when the board itself is unbound, keeping refs well-formed).
fn board_remote_or(board_remote: Option<RemoteBoardId>) -> RemoteBoardId {
    board_remote.unwrap_or(RemoteBoardId(1))
}

/// One row of [`app_state_strategy`]'s board.
#[derive(Debug, Clone)]
pub struct BoardRow {
    /// Local board id.
    pub id: crate::ids::BoardId,
    /// Remote binding.
    pub remote: Option<RemoteBoardId>,
    /// Board title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s stacks.
#[derive(Debug, Clone)]
pub struct StackRow {
    /// Local stack id.
    pub id: StackId,
    /// Remote stack number (`None` = unbound).
    pub remote: Option<u64>,
    /// Stack title.
    pub title: String,
    /// Sort order.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::StackClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s labels.
#[derive(Debug, Clone)]
pub struct LabelRow {
    /// Local label id.
    pub id: LabelId,
    /// Remote label number (`None` = unbound).
    pub remote: Option<u64>,
    /// Label title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::LabelClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

/// One row of [`app_state_strategy`]'s tasks. `stack_idx` indexes the
/// generated stack list and `label_idxs` the generated label list; both
/// are wrapped modulo the actual lengths during assembly, which keeps the
/// state referentially consistent.
#[derive(Debug, Clone)]
pub struct TaskRow {
    /// Local task id.
    pub id: TaskId,
    /// Remote card number (`None` = unbound/local-only).
    pub remote: Option<u64>,
    /// Task title.
    pub title: String,
    /// Task description.
    pub description: String,
    /// Due date.
    pub duedate: Option<DateTime<Utc>>,
    /// Completion timestamp.
    pub done: Option<DateTime<Utc>>,
    /// Index into the generated stack list.
    pub stack_idx: usize,
    /// Sort order.
    pub order: i64,
    /// Indices into the generated label list.
    pub label_idxs: Vec<usize>,
    /// Archived flag.
    pub archived: bool,
    /// Tombstone flag.
    pub deleted: bool,
    /// Per-field write clocks.
    pub clocks: crate::entities::TaskClocks,
    /// Remote baseline.
    pub remote_seen: Option<DateTime<Utc>>,
}

fn board_row() -> impl Strategy<Value = BoardRow> {
    (
        board_id_strategy(),
        proptest::option::of(remote_number().prop_map(RemoteBoardId)),
        string_strategy(),
        string_strategy(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of(timestamp_strategy()),
    )
        .prop_map(
            |(id, remote, title, color, archived, deleted, remote_seen)| BoardRow {
                id,
                remote,
                title,
                color,
                archived,
                deleted,
                remote_seen,
            },
        )
}

fn stack_rows() -> impl Strategy<Value = Vec<StackRow>> {
    prop::collection::vec(
        (
            stack_id_strategy(),
            proptest::option::of(remote_number()),
            string_strategy(),
            -100_i64..=100,
            any::<bool>(),
            any::<bool>(),
            stack_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
        1..5,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(id, remote, title, order, archived, deleted, clocks, remote_seen)| StackRow {
                    id,
                    remote,
                    title,
                    order,
                    archived,
                    deleted,
                    clocks,
                    remote_seen,
                },
            )
            .collect()
    })
}

fn label_rows() -> impl Strategy<Value = Vec<LabelRow>> {
    prop::collection::vec(
        (
            label_id_strategy(),
            proptest::option::of(remote_number()),
            string_strategy(),
            string_strategy(),
            any::<bool>(),
            label_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
        0..5,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(id, remote, title, color, deleted, clocks, remote_seen)| LabelRow {
                    id,
                    remote,
                    title,
                    color,
                    deleted,
                    clocks,
                    remote_seen,
                },
            )
            .collect()
    })
}

fn task_rows() -> impl Strategy<Value = Vec<TaskRow>> {
    prop::collection::vec(
        (
            task_id_strategy(),
            proptest::option::of(remote_number()),
            string_strategy(),
            string_strategy(),
            proptest::option::of(timestamp_strategy()),
            proptest::option::of(timestamp_strategy()),
            0_usize..4,
            -100_i64..=100,
            prop::collection::vec(0_usize..5, 0..3),
            // (archived, deleted)
            (any::<bool>(), any::<bool>()),
            task_clocks_strategy(),
            proptest::option::of(timestamp_strategy()),
        ),
        0..12,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(
                    id,
                    remote,
                    title,
                    description,
                    duedate,
                    done,
                    stack_idx,
                    order,
                    label_idxs,
                    (archived, deleted),
                    clocks,
                    remote_seen,
                )| TaskRow {
                    id,
                    remote,
                    title,
                    description,
                    duedate,
                    done,
                    stack_idx,
                    order,
                    label_idxs,
                    archived,
                    deleted,
                    clocks,
                    remote_seen,
                },
            )
            .collect()
    })
}

/// A full `AppState` with referentially consistent rows: one board, stacks
/// and labels bound to it, tasks bound to the stacks, task label sets
/// drawn from the board's labels.
#[allow(clippy::too_many_lines)] // row structs keep it readable; length is inherent
pub fn app_state_strategy() -> impl Strategy<Value = AppState> {
    (board_row(), stack_rows(), label_rows(), task_rows()).prop_map(
        |(board, stack_rows, label_rows, task_rows)| {
            let mut boards = BTreeMap::new();
            boards.insert(
                board.id,
                Board {
                    id: board.id,
                    remote: board.remote,
                    title: board.title,
                    color: Color::new(board.color),
                    archived: board.archived,
                    deleted: board.deleted,
                    remote_seen: board.remote_seen,
                },
            );

            let mut stacks = BTreeMap::new();
            let stack_bindings: Vec<(StackId, Option<RemoteStackId>)> = stack_rows
                .iter()
                .map(|row| (row.id, row.remote.map(RemoteStackId)))
                .collect();
            for row in stack_rows {
                stacks.insert(
                    row.id,
                    Stack {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteStackRef {
                            board: board_remote_or(board.remote),
                            stack: RemoteStackId(num),
                        }),
                        board: board.id,
                        title: row.title,
                        order: row.order,
                        archived: row.archived,
                        deleted: row.deleted,
                        clocks: row.clocks,
                        remote_seen: row.remote_seen,
                    },
                );
            }

            let mut labels = BTreeMap::new();
            let label_ids: Vec<LabelId> = label_rows.iter().map(|row| row.id).collect();
            for row in label_rows {
                labels.insert(
                    row.id,
                    Label {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteLabelRef {
                            board: board_remote_or(board.remote),
                            label: RemoteLabelId(num),
                        }),
                        board: board.id,
                        title: row.title,
                        color: Color::new(row.color),
                        deleted: row.deleted,
                        clocks: row.clocks,
                        remote_seen: row.remote_seen,
                    },
                );
            }

            let mut tasks = BTreeMap::new();
            for row in task_rows {
                let (stack_id, stack_remote) = stack_bindings[row.stack_idx % stack_bindings.len()];
                let label_set: BTreeSet<LabelId> = row
                    .label_idxs
                    .into_iter()
                    .filter(|_| !label_ids.is_empty())
                    .map(|i| label_ids[i % label_ids.len()])
                    .collect();
                tasks.insert(
                    row.id,
                    Task {
                        id: row.id,
                        remote: row.remote.map(|num| RemoteCardRef {
                            board: board_remote_or(board.remote),
                            stack: stack_remote.unwrap_or(RemoteStackId(1)),
                            card: RemoteCardId(num),
                        }),
                        title: row.title,
                        description: row.description,
                        duedate: row.duedate,
                        done: row.done,
                        stack: stack_id,
                        order: row.order,
                        labels: label_set,
                        archived: row.archived,
                        deleted: row.deleted,
                        clocks: row.clocks,
                        remote_seen: row.remote_seen,
                    },
                );
            }

            AppState {
                boards,
                stacks,
                tasks,
                labels,
                sync: SyncStatus::default(),
                last_updated: None,
            }
        },
    )
}

/// One row of [`persisted_state_strategy`]'s outbox.
#[derive(Debug, Clone)]
pub struct OutboxRow {
    /// Operation id.
    pub op_id: crate::outbox::OpId,
    /// Operation kind (index into `LocalOp` variants; resolved against the
    /// generated entity lists, rows without a matching entity are dropped).
    pub kind: usize,
    /// Queue timestamp.
    pub queued_at: DateTime<Utc>,
}

fn outbox_rows() -> impl Strategy<Value = Vec<OutboxRow>> {
    prop::collection::vec((op_id_strategy(), 0_usize..13, timestamp_strategy()), 0..6).prop_map(
        |rows| {
            rows.into_iter()
                .map(|(op_id, kind, queued_at)| OutboxRow {
                    op_id,
                    kind,
                    queued_at,
                })
                .collect()
        },
    )
}

/// A fully populated persisted state (same referential guarantees as
/// [`app_state_strategy`], plus outbox and validators).
pub fn persisted_state_strategy() -> impl Strategy<Value = PersistedState> {
    (
        app_state_strategy(),
        outbox_rows(),
        prop::option::of(remote_number().prop_map(RemoteBoardId)),
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy()),
        prop::option::of(timestamp_strategy()),
        any::<u32>(),
        prop::option::of(sync_phase_strategy()),
    )
        .prop_map(
            |(
                state,
                op_rows,
                validator_board,
                etag,
                last_modified,
                last_success,
                pending_ops,
                phase,
            )| {
                let outbox = build_outbox(&state, op_rows);
                let validators = build_validators(validator_board, etag, last_modified);
                let sync = SyncStatus {
                    phase: phase.unwrap_or(SyncPhase::Idle),
                    last_success,
                    pending_ops,
                };
                PersistedState {
                    boards: state.boards,
                    stacks: state.stacks,
                    tasks: state.tasks,
                    labels: state.labels,
                    outbox,
                    validators,
                    sync,
                }
            },
        )
}

/// Maps op-kind rows onto the generated entities; rows whose targets don't
/// exist are skipped (the outbox never references missing entities).
fn build_outbox(state: &AppState, op_rows: Vec<OutboxRow>) -> Vec<PendingOp> {
    let task_id = state.tasks.keys().next().copied();
    let stack_id = state.stacks.keys().next().copied();
    let label_id = state.labels.keys().next().copied();
    let mut outbox = Vec::new();
    for row in op_rows {
        let t = task_id;
        let s = stack_id;
        let l = label_id;
        let op = match row.kind {
            0 => t.map(LocalOp::CreateTask),
            1 => t.map(LocalOp::UpdateTask),
            2 => t.map(LocalOp::MoveTask),
            3 => t.map(LocalOp::DeleteTask),
            4 => s.map(LocalOp::CreateStack),
            5 => s.map(LocalOp::RenameStack),
            6 => s.map(LocalOp::DeleteStack),
            7 => l.map(LocalOp::CreateLabel),
            8 => l.map(LocalOp::UpdateLabel),
            9 => l.map(LocalOp::DeleteLabel),
            10 => t.zip(l).map(|(t, l)| LocalOp::AssignLabel(t, l)),
            11 => t.zip(l).map(|(t, l)| LocalOp::UnassignLabel(t, l)),
            _ => t.zip(l).map(|(t, l)| LocalOp::AssignLabel(t, l)),
        };
        if let Some(op) = op {
            outbox.push(PendingOp {
                op_id: row.op_id,
                op,
                queued_at: row.queued_at,
            });
        }
    }
    outbox
}

fn build_validators(
    board: Option<RemoteBoardId>,
    etag: Option<String>,
    last_modified: Option<String>,
) -> BTreeMap<ValidatorKey, SyncValidators> {
    let mut validators = BTreeMap::new();
    if let Some(board) = board {
        validators.insert(
            ValidatorKey::Boards,
            SyncValidators {
                etag: etag.clone(),
                last_modified: last_modified.clone(),
            },
        );
        validators.insert(
            ValidatorKey::Stacks(board),
            SyncValidators {
                etag: etag.clone(),
                last_modified: last_modified.clone(),
            },
        );
        validators.insert(
            ValidatorKey::ArchivedStacks(board),
            SyncValidators {
                etag,
                last_modified,
            },
        );
    }
    validators
}

fn sync_phase_strategy() -> impl Strategy<Value = SyncPhase> {
    prop_oneof![
        Just(SyncPhase::Idle),
        Just(SyncPhase::Syncing),
        Just(SyncPhase::Offline),
        (0_usize..7).prop_map(|kind| SyncPhase::Failed {
            last_error: match kind {
                0 => SyncErrorKind::Network,
                1 => SyncErrorKind::Auth,
                2 => SyncErrorKind::Forbidden,
                3 => SyncErrorKind::Server,
                4 => SyncErrorKind::BadRequest,
                5 => SyncErrorKind::LocalData,
                _ => SyncErrorKind::NoBoard,
            },
        }),
    ]
}
