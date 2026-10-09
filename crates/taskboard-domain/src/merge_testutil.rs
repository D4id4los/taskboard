// SPDX-License-Identifier: MIT OR Apache-2.0
//! Shared test fixtures for the conflict-policy tests in [`crate::merge`]
//! and [`crate::pipeline`]: a deterministic id generator, logical-time
//! helpers, and builders for bound entities, remote views, and snapshots
//! with referentially consistent refs.

use std::collections::BTreeSet;

use chrono::{DateTime, TimeZone, Utc};

use crate::entities::{Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks};
use crate::ids::{
    BoardId, LabelId, RemoteBoardId, RemoteCardId, RemoteCardRef, RemoteLabelId, RemoteLabelRef,
    RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use crate::outbox::PendingOp;
use crate::remote::{RemoteBoard, RemoteBoardSnapshot, RemoteLabel, RemoteStack, RemoteTask};
use crate::state::AppState;

pub(crate) fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(secs, 0).unwrap()
}

// The deterministic id generator is the promoted, feature-gated
// `test_support::CountingIds` (public so downstream crates reuse it).
pub(crate) use crate::test_support::CountingIds;

pub(crate) const BASE: i64 = 36_000; // 10:00:00Z
pub(crate) const REMOTE_BOARD_NUM: u64 = 77;

pub(crate) fn board(num: u64) -> RemoteBoardId {
    RemoteBoardId(num)
}

pub(crate) fn card_ref(stack: u64, card: u64) -> RemoteCardRef {
    RemoteCardRef {
        board: board(REMOTE_BOARD_NUM),
        stack: RemoteStackId(stack),
        card: RemoteCardId(card),
    }
}

pub(crate) fn stack_ref(stack: u64) -> RemoteStackRef {
    RemoteStackRef {
        board: board(REMOTE_BOARD_NUM),
        stack: RemoteStackId(stack),
    }
}

pub(crate) fn remote_stack(num: u64, last_modified: i64) -> RemoteStack {
    RemoteStack {
        id: stack_ref(num),
        title: format!("stack {num}"),
        order: num.cast_signed(),
        archived: false,
        deleted_at: None,
        last_modified: ts(last_modified),
    }
}

pub(crate) fn remote_task(stack: u64, card: u64, last_modified: i64) -> RemoteTask {
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

pub(crate) fn clocks_at(t: i64) -> TaskClocks {
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

pub(crate) fn bound_task(card: u64) -> Task {
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

pub(crate) fn local_stack() -> Stack {
    Stack {
        id: StackId::from(uuid::Uuid::from_u128(900)),
        remote: Some(stack_ref(1)),
        board: BoardId::from(uuid::Uuid::from_u128(800)),
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

pub(crate) fn base_state(task: Task) -> AppState {
    let board_id = BoardId::from(uuid::Uuid::from_u128(800));
    let mut state = AppState::default();
    state.boards.insert(
        board_id,
        Board {
            id: board_id,
            remote: Some(board(REMOTE_BOARD_NUM)),
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

pub(crate) fn snapshot(stacks: Vec<RemoteStack>, tasks: Vec<RemoteTask>) -> RemoteBoardSnapshot {
    RemoteBoardSnapshot {
        board: RemoteBoard {
            id: board(REMOTE_BOARD_NUM),
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

pub(crate) fn single_stack_snapshot(
    tasks: Vec<RemoteTask>,
    last_modified: i64,
) -> RemoteBoardSnapshot {
    snapshot(vec![remote_stack(1, last_modified)], tasks)
}

pub(crate) fn task_by_card(state: &AppState, card: u64) -> &Task {
    state
        .tasks
        .values()
        .find(|t| t.remote == Some(card_ref(1, card)))
        .unwrap()
}

/// Clones the merged state out of a pipeline result (assertion helper).
pub(crate) fn app_into_state(
    app: &(AppState, Vec<crate::persistence::PersistenceAction>),
) -> AppState {
    app.0.clone()
}

pub(crate) fn bound_label(num: u64) -> Label {
    Label {
        id: LabelId::from(uuid::Uuid::from_u128(600 + u128::from(num))),
        remote: Some(RemoteLabelRef {
            board: board(REMOTE_BOARD_NUM),
            label: RemoteLabelId(num),
        }),
        board: BoardId::from(uuid::Uuid::from_u128(800)),
        title: format!("label {num}"),
        color: Color::new("00ff00"),
        deleted: false,
        clocks: LabelClocks {
            title: ts(BASE),
            color: ts(BASE),
            deleted: ts(BASE),
        },
        remote_seen: Some(ts(BASE)),
    }
}

pub(crate) fn remote_label(num: u64, last_modified: i64) -> RemoteLabel {
    RemoteLabel {
        id: RemoteLabelRef {
            board: board(REMOTE_BOARD_NUM),
            label: RemoteLabelId(num),
        },
        title: format!("label {num} renamed"),
        color: "0000ff".into(),
        deleted_at: None,
        last_modified: ts(last_modified),
    }
}

pub(crate) fn remote_board(num: u64, last_modified: i64) -> RemoteBoard {
    RemoteBoard {
        id: board(num),
        title: format!("board {num}"),
        color: "ff0000".into(),
        archived: false,
        deleted_at: None,
        last_modified: ts(last_modified),
    }
}

/// Type witness keeping op-channel imports honest if fixtures change.
#[allow(dead_code)]
fn op_type_witness(pending: PendingOp) -> PendingOp {
    pending
}
