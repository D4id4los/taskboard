// SPDX-License-Identifier: MIT OR Apache-2.0
//! Strategies for the remote observation views: single views, complete
//! board snapshots with internally consistent refs, and push outcomes.

use chrono::{DateTime, Utc};
use proptest::prelude::*;

use super::{remote_number, string_strategy, timestamp_strategy};
use crate::ids::{
    RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef, RemoteStackId,
    RemoteStackRef,
};
use crate::remote::{
    PushOutcome, PushResult, RemoteBoard, RemoteBoardSnapshot, RemoteEcho, RemoteLabel,
    RemoteStack, RemoteTask,
};
use crate::state::SyncErrorKind;

/// One row of [`snapshot_strategy`]'s stack list before ref assembly.
#[derive(Debug, Clone)]
pub struct SnapshotStackRow {
    /// Remote stack number (unique within the snapshot).
    pub num: u64,
    /// Stack title.
    pub title: String,
    /// Sort order.
    pub order: i64,
    /// Archived flag.
    pub archived: bool,
    /// Soft-delete stamp.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

/// One row of [`snapshot_strategy`]'s label list before ref assembly.
#[derive(Debug, Clone)]
pub struct SnapshotLabelRow {
    /// Remote label number.
    pub num: u64,
    /// Label title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Soft-delete stamp.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

/// One row of [`snapshot_strategy`]'s card list before ref assembly. The
/// index fields are wrapped modulo the actual list lengths during
/// assembly, which keeps the snapshot referentially consistent.
#[derive(Debug, Clone)]
pub struct SnapshotTaskRow {
    /// Card number (used when `remote` is `None`).
    pub card: u64,
    /// An independently generated remote ref (its board/stack are ignored;
    /// the card number is reused so tasks may share refs or not).
    pub remote: Option<RemoteCardRef>,
    /// Card title.
    pub title: String,
    /// Card description.
    pub description: String,
    /// Due date.
    pub duedate: Option<DateTime<Utc>>,
    /// Completion timestamp.
    pub done: Option<DateTime<Utc>>,
    /// Index into the snapshot's stack list.
    pub stack_idx: usize,
    /// Sort order.
    pub order: i64,
    /// Indices into the snapshot's label list.
    pub label_idxs: Vec<usize>,
    /// Archived flag.
    pub archived: bool,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

/// Board-level fields of [`snapshot_strategy`].
#[derive(Debug, Clone)]
pub struct SnapshotBoardRow {
    /// Remote board number.
    pub num: u64,
    /// Board title.
    pub title: String,
    /// Raw color string.
    pub color: String,
    /// Archived flag.
    pub archived: bool,
    /// Soft-delete stamp.
    pub deleted_at: Option<DateTime<Utc>>,
    /// Entity-level version stamp.
    pub last_modified: DateTime<Utc>,
}

fn snapshot_stack_rows() -> impl Strategy<Value = Vec<SnapshotStackRow>> {
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
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(num, title, order, archived, deleted_at, last_modified)| SnapshotStackRow {
                    num,
                    title,
                    order,
                    archived,
                    deleted_at,
                    last_modified,
                },
            )
            .collect()
    })
}

fn snapshot_label_rows() -> impl Strategy<Value = Vec<SnapshotLabelRow>> {
    prop::collection::vec(
        (
            remote_number(),
            string_strategy(),
            string_strategy(),
            proptest::option::of(timestamp_strategy()),
            timestamp_strategy(),
        ),
        0..4,
    )
    .prop_map(|rows| {
        rows.into_iter()
            .map(
                |(num, title, color, deleted_at, last_modified)| SnapshotLabelRow {
                    num,
                    title,
                    color,
                    deleted_at,
                    last_modified,
                },
            )
            .collect()
    })
}

fn snapshot_task_rows() -> impl Strategy<Value = Vec<SnapshotTaskRow>> {
    prop::collection::vec(
        (
            remote_number(),
            proptest::option::of(super::entities::remote_card_ref_strategy()),
            string_strategy(),
            string_strategy(),
            proptest::option::of(timestamp_strategy()),
            proptest::option::of(timestamp_strategy()),
            0_usize..4,
            -100_i64..=100,
            prop::collection::vec(0_usize..4, 0..3),
            any::<bool>(),
            timestamp_strategy(),
        ),
        0..8,
    )
    .prop_map(|rows| {
        rows.into_iter()
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
                    last_modified,
                )| {
                    SnapshotTaskRow {
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
                        last_modified,
                    }
                },
            )
            .collect()
    })
}

fn snapshot_board_row() -> impl Strategy<Value = SnapshotBoardRow> {
    (
        remote_number(),
        string_strategy(),
        string_strategy(),
        any::<bool>(),
        proptest::option::of(timestamp_strategy()),
        timestamp_strategy(),
    )
        .prop_map(|(num, title, color, archived, deleted_at, last_modified)| {
            SnapshotBoardRow {
                num,
                title,
                color,
                archived,
                deleted_at,
                last_modified,
            }
        })
}

/// A complete remote board snapshot with internally consistent refs: all
/// members share the board id, card stack ids come from the stack list,
/// and card label ids come from the label list.
pub fn snapshot_strategy() -> impl Strategy<Value = RemoteBoardSnapshot> {
    (
        snapshot_board_row(),
        snapshot_stack_rows(),
        snapshot_label_rows(),
        snapshot_task_rows(),
    )
        .prop_map(|(board, stack_rows, label_rows, task_rows)| {
            let board_id = RemoteBoardId(board.num);
            let stacks: Vec<RemoteStack> = stack_rows
                .into_iter()
                .map(|row| RemoteStack {
                    id: RemoteStackRef {
                        board: board_id,
                        stack: RemoteStackId(row.num),
                    },
                    title: row.title,
                    order: row.order,
                    archived: row.archived,
                    deleted_at: row.deleted_at,
                    last_modified: row.last_modified,
                })
                .collect();
            let labels: Vec<RemoteLabel> = label_rows
                .into_iter()
                .map(|row| RemoteLabel {
                    id: RemoteLabelRef {
                        board: board_id,
                        label: RemoteLabelId(row.num),
                    },
                    title: row.title,
                    color: row.color,
                    deleted_at: row.deleted_at,
                    last_modified: row.last_modified,
                })
                .collect();

            let stack_count = stacks.len();
            let label_count = labels.len();
            let tasks: Vec<RemoteTask> = task_rows
                .into_iter()
                .map(|row| {
                    let stack = stacks
                        .get(row.stack_idx % stack_count.max(1))
                        .map_or(RemoteStackId(1), |s| s.id.stack);
                    let labels = row
                        .label_idxs
                        .into_iter()
                        .filter(|_| label_count > 0)
                        .map(|i| labels[i % label_count].id.label)
                        .collect();
                    RemoteTask {
                        id: RemoteCardRef {
                            board: board_id,
                            stack,
                            card: RemoteCardId(row.remote.map_or(row.card, |r| r.card.0)),
                        },
                        title: row.title,
                        description: row.description,
                        duedate: row.duedate,
                        done: row.done,
                        stack,
                        order: row.order,
                        labels,
                        archived: row.archived,
                        last_modified: row.last_modified,
                    }
                })
                .collect();

            RemoteBoardSnapshot {
                board: RemoteBoard {
                    id: board_id,
                    title: board.title,
                    color: board.color,
                    archived: board.archived,
                    deleted_at: board.deleted_at,
                    last_modified: board.last_modified,
                },
                stacks,
                tasks,
                labels,
            }
        })
}

/// A remote card view.
pub fn remote_task_strategy() -> impl Strategy<Value = RemoteTask> {
    (
        super::entities::remote_card_ref_strategy(),
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

/// A push outcome.
pub fn push_outcome_strategy() -> impl Strategy<Value = PushOutcome> {
    (
        super::op_id_strategy(),
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
