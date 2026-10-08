// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reusable test support: property strategies, an in-memory
//! [`TaskRepository`](crate::persistence::TaskRepository) fake, and the
//! repository contract harness.
//!
//! Ships behind the non-default `test-support` feature so later-phase
//! crates (which cannot share dev-dependencies) can depend on it in tests.

pub mod contract;
pub mod memory;

use std::collections::BTreeSet;

use chrono::{DateTime, TimeZone, Utc};
use proptest::prelude::*;
use uuid::Uuid;

use crate::entities::{Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks};
use crate::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use crate::ops::{LocalOp, OpId, PendingOp};
use crate::persistence::{PersistedState, SyncValidators, ValidatorKey};
use crate::remote::{
    PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot, RemoteEcho, RemoteLabel,
    RemoteStack, RemoteTask,
};
use crate::state::{AppState, SyncErrorKind, SyncPhase, SyncStatus};

/// Bounded, serde-stable strings (no control characters).
pub fn string_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ._/-]{0,32}"
}

/// Logical timestamps across a sane range (1971–2100), seconds resolution.
pub fn timestamp_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    (31_536_000_i64..=4_102_444_800_i64).prop_map(|secs| Utc.timestamp_opt(secs, 0).unwrap())
}

macro_rules! id_strategy {
    ($fn_name:ident, $id_type:ty) => {
        /// A random local id (from a random 128-bit UUID).
        pub fn $fn_name() -> impl Strategy<Value = $id_type> {
            any::<u128>().prop_map(|raw| <$id_type>::from(Uuid::from_u128(raw)))
        }
    };
}

id_strategy!(board_id_strategy, BoardId);
id_strategy!(stack_id_strategy, StackId);
id_strategy!(task_id_strategy, TaskId);
id_strategy!(label_id_strategy, LabelId);

/// A random op id.
pub fn op_id_strategy() -> impl Strategy<Value = OpId> {
    any::<u128>().prop_map(|raw| OpId(Uuid::from_u128(raw)))
}

fn remote_number() -> impl Strategy<Value = u64> {
    1_u64..=1_000_000
}

fn task_clocks_strategy() -> impl Strategy<Value = TaskClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, d, du, do_, p, l, a, del)| TaskClocks {
            title: t,
            description: d,
            duedate: du,
            done: do_,
            position: p,
            labels: l,
            archived: a,
            deleted: del,
        })
}

fn stack_clocks_strategy() -> impl Strategy<Value = StackClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, o, del)| StackClocks {
            title: t,
            order: o,
            deleted: del,
        })
}

fn label_clocks_strategy() -> impl Strategy<Value = LabelClocks> {
    (
        timestamp_strategy(),
        timestamp_strategy(),
        timestamp_strategy(),
    )
        .prop_map(|(t, c, del)| LabelClocks {
            title: t,
            color: c,
            deleted: del,
        })
}

/// A task entity. Not referentially bound: use [`app_state_strategy`] for
/// state-level tests that need `stack`/`labels` to point at existing rows.
pub fn task_strategy() -> impl Strategy<Value = Task> {
    (
        task_id_strategy(),
        proptest::option::of(remote_card_ref_strategy()),
        string_strategy(),
        string_strategy(),
        proptest::option::of(timestamp_strategy()),
        proptest::option::of(timestamp_strategy()),
        stack_id_strategy(),
        -1_000_i64..=1_000,
        prop::collection::btree_set(label_id_strategy(), 0..3),
        (any::<bool>(), any::<bool>()),
        task_clocks_strategy(),
        prop::option::of(timestamp_strategy()),
    )
        .prop_map(
            |(
                id,
                remote,
                title,
                description,
                duedate,
                done,
                stack,
                order,
                labels,
                (archived, deleted),
                clocks,
                remote_seen,
            )| Task {
                id,
                remote,
                title,
                description,
                duedate,
                done,
                stack,
                order,
                labels,
                archived,
                deleted,
                clocks,
                remote_seen,
            },
        )
}

fn remote_card_ref_strategy() -> impl Strategy<Value = RemoteCardRef> {
    (remote_number(), remote_number(), remote_number()).prop_map(|(board, stack, card)| {
        RemoteCardRef {
            board: RemoteBoardId(board),
            stack: RemoteStackId(stack),
            card: RemoteCardId(card),
        }
    })
}

/// A remote card view.
pub fn remote_task_strategy() -> impl Strategy<Value = RemoteTask> {
    (
        remote_card_ref_strategy(),
        string_strategy(),
        string_strategy(),
        proptest::option::of(timestamp_strategy()),
        proptest::option::of(timestamp_strategy()),
        remote_number(),
        -1_000_i64..=1_000,
        prop::collection::btree_set(remote_number().prop_map(RemoteLabelId), 0..3),
        any::<bool>(),
        timestamp_strategy(),
    )
        .prop_map(
            |(
                id,
                title,
                description,
                duedate,
                done,
                stack,
                order,
                labels,
                archived,
                last_modified,
            )| {
                RemoteTask {
                    id,
                    title,
                    description,
                    duedate,
                    done,
                    stack: RemoteStackId(stack),
                    order,
                    labels,
                    archived,
                    last_modified,
                }
            },
        )
}

/// A complete remote board snapshot with internally consistent refs (all
/// members share the board id; card stack ids come from the stack list).
#[allow(clippy::too_many_lines)] // flat row-tuple -> entity-tree assembly
pub fn snapshot_strategy() -> impl Strategy<Value = RemoteBoardSnapshot> {
    (
        remote_number(),
        string_strategy(),
        string_strategy(),
        any::<bool>(),
        proptest::option::of(timestamp_strategy()),
        timestamp_strategy(),
        prop::collection::vec(
            (
                remote_number(),
                string_strategy(),
                -100_i64..=100,
                any::<bool>(),
                proptest::option::of(timestamp_strategy()),
                timestamp_strategy(),
            ),
            0..4,
        ),
        prop::collection::vec(
            (
                remote_number(),
                string_strategy(),
                string_strategy(),
                proptest::option::of(timestamp_strategy()),
                timestamp_strategy(),
            ),
            0..4,
        ),
        prop::collection::vec(
            (
                remote_number(),
                prop::option::of(remote_card_ref_strategy()),
                string_strategy(),
                string_strategy(),
                prop::option::of(timestamp_strategy()),
                prop::option::of(timestamp_strategy()),
                // stack index into the stacks vec, wrapped later
                0_usize..4,
                -100_i64..=100,
                // label indices, wrapped later
                prop::collection::vec(0_usize..4, 0..3),
                any::<bool>(),
                timestamp_strategy(),
            ),
            0..8,
        ),
    )
        .prop_map(
            |(
                board_num,
                board_title,
                board_color,
                board_archived,
                board_deleted_at,
                board_lm,
                stack_rows,
                label_rows,
                task_rows,
            )| {
                let board_id = RemoteBoardId(board_num);
                let stacks: Vec<RemoteStack> = stack_rows
                    .into_iter()
                    .map(
                        |(num, title, order, archived, deleted_at, lm)| RemoteStack {
                            id: RemoteStackRef {
                                board: board_id,
                                stack: RemoteStackId(num),
                            },
                            title,
                            order,
                            archived,
                            deleted_at,
                            last_modified: lm,
                        },
                    )
                    .collect();
                let labels: Vec<RemoteLabel> = label_rows
                    .into_iter()
                    .map(|(num, title, color, deleted_at, lm)| RemoteLabel {
                        id: RemoteLabelRef {
                            board: board_id,
                            label: RemoteLabelId(num),
                        },
                        title,
                        color,
                        deleted_at,
                        last_modified: lm,
                    })
                    .collect();
                let stack_count = stacks.len();
                let label_count = labels.len();
                let tasks: Vec<RemoteTask> = task_rows
                    .into_iter()
                    .map(
                        |(
                            card,
                            remote,
                            title,
                            description,
                            duedate,
                            done,
                            stack_idx,
                            order,
                            label_idxs,
                            archived,
                            lm,
                        )| {
                            let stack = stacks
                                .get(stack_idx % stack_count.max(1))
                                .map_or(RemoteStackId(1), |s| s.id.stack);
                            let labels = label_idxs
                                .into_iter()
                                .filter(|_| label_count > 0)
                                .map(|i| labels[i % label_count].id.label)
                                .collect();
                            RemoteTask {
                                id: RemoteCardRef {
                                    board: board_id,
                                    stack,
                                    card: RemoteCardId(remote.map_or(card, |r| r.card.0)),
                                },
                                title,
                                description,
                                duedate,
                                done,
                                stack,
                                order,
                                labels,
                                archived,
                                last_modified: lm,
                            }
                        },
                    )
                    .collect();
                RemoteBoardSnapshot {
                    board: RemoteBoard {
                        id: board_id,
                        title: board_title,
                        color: board_color,
                        archived: board_archived,
                        deleted_at: board_deleted_at,
                        last_modified: board_lm,
                    },
                    stacks,
                    tasks,
                    labels,
                }
            },
        )
}

/// A push outcome.
pub fn push_outcome_strategy() -> impl Strategy<Value = PushOutcome> {
    (
        op_id_strategy(),
        proptest::option::of(
            proptest::option::of(remote_task_strategy())
                .prop_map(|echo| echo.map(RemoteEcho::Task)),
        ),
    )
        .prop_flat_map(|(op, echo)| {
            prop::option::of(Just(echo)).prop_map(move |applied| PushOutcome {
                op,
                result: match applied {
                    Some(echo) => PushResult::Applied {
                        echo: echo.flatten(),
                    },
                    None => PushResult::Rejected {
                        kind: SyncErrorKind::Network,
                    },
                },
            })
        })
}

/// A full `AppState` with referentially consistent rows: one board, stacks
/// and labels bound to it, tasks bound to the stacks, task label sets
/// drawn from the board's labels.
#[allow(clippy::too_many_lines)] // flat row-tuple -> entity-tree assembly
pub fn app_state_strategy() -> impl Strategy<Value = AppState> {
    (
        board_id_strategy(),
        proptest::option::of(remote_number().prop_map(RemoteBoardId)),
        string_strategy(),
        string_strategy(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of(timestamp_strategy()),
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
        ),
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
        ),
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
                (any::<bool>(), any::<bool>()),
                task_clocks_strategy(),
                proptest::option::of(timestamp_strategy()),
            ),
            0..12,
        ),
    )
        .prop_map(
            |(
                board_id,
                board_remote,
                board_title,
                board_color,
                board_archived,
                board_deleted,
                board_seen,
                stack_rows,
                label_rows,
                task_rows,
            )| {
                let mut boards = std::collections::BTreeMap::new();
                boards.insert(
                    board_id,
                    Board {
                        id: board_id,
                        remote: board_remote,
                        title: board_title,
                        color: Color::new(board_color),
                        archived: board_archived,
                        deleted: board_deleted,
                        remote_seen: board_seen,
                    },
                );
                let mut stacks = std::collections::BTreeMap::new();
                let stack_ids: Vec<(StackId, Option<RemoteStackId>)> = stack_rows
                    .iter()
                    .map(|(id, remote, _, _, _, _, _, _)| (*id, remote.map(RemoteStackId)))
                    .collect();
                for (id, remote, title, order, archived, deleted, clocks, seen) in stack_rows {
                    stacks.insert(
                        id,
                        Stack {
                            id,
                            remote: remote.map(|num| RemoteStackRef {
                                board: board_remote.unwrap_or(RemoteBoardId(1)),
                                stack: RemoteStackId(num),
                            }),
                            board: board_id,
                            title,
                            order,
                            archived,
                            deleted,
                            clocks,
                            remote_seen: seen,
                        },
                    );
                }
                let mut labels = std::collections::BTreeMap::new();
                let label_ids: Vec<LabelId> = label_rows
                    .iter()
                    .map(|(id, _, _, _, _, _, _)| *id)
                    .collect();
                for (id, remote, title, color, deleted, clocks, seen) in label_rows {
                    labels.insert(
                        id,
                        Label {
                            id,
                            remote: remote.map(|num| RemoteLabelRef {
                                board: board_remote.unwrap_or(RemoteBoardId(1)),
                                label: RemoteLabelId(num),
                            }),
                            board: board_id,
                            title,
                            color: Color::new(color),
                            deleted,
                            clocks,
                            remote_seen: seen,
                        },
                    );
                }
                let mut tasks = std::collections::BTreeMap::new();
                for (
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
                    seen,
                ) in task_rows
                {
                    let (stack_id, stack_remote) = stack_ids[stack_idx % stack_ids.len()];
                    let label_set: BTreeSet<LabelId> = label_idxs
                        .into_iter()
                        .filter(|_| !label_ids.is_empty())
                        .map(|i| label_ids[i % label_ids.len()])
                        .collect();
                    tasks.insert(
                        id,
                        Task {
                            id,
                            remote: remote.map(|num| RemoteCardRef {
                                board: board_remote.unwrap_or(RemoteBoardId(1)),
                                stack: stack_remote.unwrap_or(RemoteStackId(1)),
                                card: RemoteCardId(num),
                            }),
                            title,
                            description,
                            duedate,
                            done,
                            stack: stack_id,
                            order,
                            labels: label_set,
                            archived,
                            deleted,
                            clocks,
                            remote_seen: seen,
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

/// Sparse task changes.
pub fn task_changes_strategy() -> impl Strategy<Value = crate::messages::TaskChanges> {
    (
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy()),
        prop::option::of(prop::option::of(timestamp_strategy())),
        prop::option::of(any::<bool>()),
    )
        .prop_map(
            |(title, description, duedate, archived)| crate::messages::TaskChanges {
                title,
                description,
                duedate,
                archived,
            },
        )
}

/// Sparse label changes.
pub fn label_changes_strategy() -> impl Strategy<Value = crate::messages::LabelChanges> {
    (
        prop::option::of(string_strategy()),
        prop::option::of(string_strategy().prop_map(Color::new)),
    )
        .prop_map(|(title, color)| crate::messages::LabelChanges { title, color })
}

/// A fully populated persisted state (same referential guarantees as
/// [`app_state_strategy`], plus outbox and validators).
pub fn persisted_state_strategy() -> impl Strategy<Value = PersistedState> {
    (
        app_state_strategy(),
        board_id_strategy(),
        prop::collection::vec((op_id_strategy(), 0_usize..13, timestamp_strategy()), 0..6),
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
                _board_id,
                op_rows,
                validator_board,
                etag,
                last_modified,
                last_success,
                pending_ops,
                phase,
            )| {
                let mut outbox = Vec::new();
                let task_ids: Vec<TaskId> = state.tasks.keys().copied().collect();
                let stack_ids: Vec<StackId> = state.stacks.keys().copied().collect();
                let label_ids: Vec<LabelId> = state.labels.keys().copied().collect();
                for (op_id, kind, queued_at) in op_rows {
                    let t = task_ids.first().copied();
                    let s = stack_ids.first().copied();
                    let l = label_ids.first().copied();
                    let op = match kind {
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
                    let Some(op) = op else {
                        continue;
                    };
                    outbox.push(PendingOp {
                        op_id,
                        op,
                        queued_at,
                    });
                }
                let mut validators = std::collections::BTreeMap::new();
                if let Some(board) = validator_board {
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
                            etag,
                            last_modified,
                        },
                    );
                }
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

fn sync_phase_strategy() -> impl Strategy<Value = SyncPhase> {
    prop_oneof![
        Just(SyncPhase::Idle),
        Just(SyncPhase::Syncing),
        Just(SyncPhase::Offline),
        (0_usize..6).prop_map(|kind| SyncPhase::Failed {
            last_error: match kind {
                0 => SyncErrorKind::Network,
                1 => SyncErrorKind::Auth,
                2 => SyncErrorKind::Forbidden,
                3 => SyncErrorKind::Server,
                4 => SyncErrorKind::BadRequest,
                _ => SyncErrorKind::LocalData,
            },
        }),
    ]
}
