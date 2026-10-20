// SPDX-License-Identifier: AGPL-3.0-only

//! The journey/golden fixture (chunk plan §5.1): the two-stack board
//! shape of the §6 component tests, extracted for the integration
//! binaries (the lib's private copy stays — duplication across test
//! binaries is the cargo reality, noted in the plan).
//!
//! R7/R8: titles are ASCII-only so golden rendering can never fall
//! back to a system font for a missing glyph.

use chrono::TimeDelta;
use std::collections::BTreeSet;
use taskboard_domain::{
    AppState, Board, BoardClocks, BoardId, Color, Stack, StackClocks, StackId, SyncPhase, Task,
    TaskClocks, TaskId,
};

fn epoch() -> chrono::DateTime<chrono::Utc> {
    BoardClocks::epoch()
}

fn task_clocks() -> TaskClocks {
    TaskClocks {
        title: epoch(),
        description: epoch(),
        duedate: epoch(),
        done: epoch(),
        position: epoch(),
        labels: epoch(),
        archived: epoch(),
        deleted: epoch(),
    }
}

fn stack_clocks() -> StackClocks {
    StackClocks {
        title: epoch(),
        order: epoch(),
        deleted: epoch(),
    }
}

fn task(id: TaskId, stack: StackId, order: i64) -> Task {
    Task {
        id,
        remote: None,
        title: format!("task-{order}"),
        description: String::new(),
        duedate: None,
        done: None,
        stack,
        order,
        labels: BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: None,
    }
}

fn stack(id: StackId, board: BoardId, title: &str, order: i64) -> Stack {
    Stack {
        id,
        remote: None,
        board,
        title: title.to_owned(),
        order,
        archived: false,
        deleted: false,
        clocks: stack_clocks(),
        remote_seen: None,
    }
}

fn board(id: BoardId) -> Board {
    Board {
        id,
        remote: None,
        title: "board".to_owned(),
        color: Color::new(""),
        archived: false,
        deleted: false,
        clocks: BoardClocks {
            title: epoch(),
            color: taskboard_domain::BoardClocks::epoch(),
            archived: taskboard_domain::BoardClocks::epoch(),
            deleted: taskboard_domain::BoardClocks::epoch(),
        },
        remote_seen: None,
    }
}

/// The fixture's ids the journeys address at the typed seams (the
/// command payloads' stacks, and the task `journey_state` makes
/// overdue).
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    pub stack_a: StackId,
    pub stack_b: StackId,
    /// Stack "a"'s second task; `journey_state` marks it overdue.
    pub task1: TaskId,
}

/// The journey fixture (§5.1): one board, two stacks ("a" with tasks
/// 0 and 1, 1 overdue; "b" with task 2), sync `Offline` with two
/// pending ops — the step-9 status read's subject.
#[must_use]
pub fn journey_state() -> (Fixture, AppState) {
    let (fixture, mut state) = golden_state();
    let mut overdue = state.tasks.get(&fixture.task1).expect("task").clone();
    overdue.duedate = Some(chrono::Utc::now() - TimeDelta::hours(2));
    state.tasks.insert(fixture.task1, overdue);
    // The projection is derived: two fabricated ops give the badge its
    // pending count; the phase carries the stored Offline.
    let board = *state.boards.keys().next().expect("fixture board");
    state.sync.set_stored(board, SyncPhase::Offline, None);
    let ops: Vec<taskboard_domain::PendingOp> = (0..2)
        .map(|i| taskboard_domain::PendingOp {
            op_id: taskboard_domain::OpId(uuid::Uuid::from_u128(
                u128::try_from(0x9000 + i).expect("fixture op index fits"),
            )),
            op: taskboard_domain::LocalOp::UpdateTask(fixture.task1),
            queued_at: epoch(),
        })
        .collect();
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &ops,
        &mut state.sync,
    );
    (fixture, state)
}

/// The clean two-stack board (the golden tier's fixture, chunk plan
/// §7): no overdue banner (banner labels derive from wall-clock
/// relative time and would flake across hour boundaries), no flow
/// hint, sync idle.
#[must_use]
pub fn golden_state() -> (Fixture, AppState) {
    let board_id = BoardId::from_uuid(uuid::Uuid::now_v7());
    let stack_a = StackId::from_uuid(uuid::Uuid::now_v7());
    let stack_b = StackId::from_uuid(uuid::Uuid::now_v7());
    let task0 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let task1 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let task2 = TaskId::from_uuid(uuid::Uuid::now_v7());
    let mut state = AppState::default();
    state.boards.insert(board_id, board(board_id));
    state
        .stacks
        .insert(stack_a, stack(stack_a, board_id, "a", 0));
    state
        .stacks
        .insert(stack_b, stack(stack_b, board_id, "b", 1));
    state.tasks.insert(task0, task(task0, stack_a, 0));
    state.tasks.insert(task1, task(task1, stack_a, 1));
    state.tasks.insert(task2, task(task2, stack_b, 0));
    (
        Fixture {
            stack_a,
            stack_b,
            task1,
        },
        state,
    )
}

/// The deterministic board ids and the second board's stack of
/// `two_board_state` (board one < board two, so board one seeds).
#[derive(Debug, Clone, Copy)]
pub struct TwoBoardFixture {
    pub board_one: BoardId,
    pub board_two: BoardId,
    /// Board two's single stack ("c").
    pub stack_c: StackId,
}

/// The two-board journey fixture (chunk-7 plan D9): board one
/// ("board one") with stacks "a" (tasks 0, 1) and "b" (task 2),
/// board two ("board two") with stack "c" (task 3); sync rows
/// derived idle. Deterministic board ids fix the canonical order —
/// board one is the seed.
#[must_use]
pub fn two_board_state() -> (TwoBoardFixture, AppState) {
    let board_one = BoardId::from_uuid(uuid::Uuid::from_u128(0x1001));
    let board_two = BoardId::from_uuid(uuid::Uuid::from_u128(0x1002));
    let stack_a = StackId::from_uuid(uuid::Uuid::from_u128(0x2001));
    let stack_b = StackId::from_uuid(uuid::Uuid::from_u128(0x2002));
    let stack_c = StackId::from_uuid(uuid::Uuid::from_u128(0x2003));
    let mut state = AppState::default();
    for (id, title) in [(board_one, "board one"), (board_two, "board two")] {
        let mut board = board(id);
        title.clone_into(&mut board.title);
        state.boards.insert(id, board);
    }
    state
        .stacks
        .insert(stack_a, stack(stack_a, board_one, "a", 0));
    state
        .stacks
        .insert(stack_b, stack(stack_b, board_one, "b", 1));
    state
        .stacks
        .insert(stack_c, stack(stack_c, board_two, "c", 0));
    for (id, st, order) in [
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3001)), stack_a, 0),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3002)), stack_a, 1),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3003)), stack_b, 0),
        (TaskId::from_uuid(uuid::Uuid::from_u128(0x3004)), stack_c, 0),
    ] {
        state.tasks.insert(id, task(id, st, order));
    }
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    (
        TwoBoardFixture {
            board_one,
            board_two,
            stack_c,
        },
        state,
    )
}
