// SPDX-License-Identifier: AGPL-3.0-only

//! The walk/planner journeys' shared fixture builders (the D4 dedup):
//! the inbox board/stack, the triage-row task literal, the yesterday
//! carry anchor, and the stack-b box placement. Included once per
//! `journey_suites` binary; both gated suites route through it so the
//! fixture shapes are one construction site.

use crate::support;

use support::fixture;

use taskboard_domain::AppState;

/// A fresh triage row on `stack` (the walk fixtures' shared task
/// literal — the `walk_new_task` shape).
pub fn walk_new_task(stack: taskboard_domain::StackId, title: &str) -> taskboard_domain::Task {
    taskboard_domain::Task {
        id: taskboard_domain::TaskId::from_uuid(uuid::Uuid::now_v7()),
        remote: None,
        label_memberships: std::collections::BTreeMap::new(),
        title: title.to_owned(),
        description: String::new(),
        duedate: None,
        scheduled: None,
        done: None,
        stack,
        order: 0,
        effort: None,
        importance: None,
        urgency: None,
        recur: None,
        kind: taskboard_domain::entities::TaskKind::Standard,
        archived: false,
        deleted: false,
        clocks: taskboard_domain::entities::TaskClocks {
            title: taskboard_domain::entities::BoardClocks::epoch(),
            description: taskboard_domain::entities::BoardClocks::epoch(),
            duedate: taskboard_domain::entities::BoardClocks::epoch(),
            scheduled: taskboard_domain::entities::BoardClocks::epoch(),
            done: taskboard_domain::entities::BoardClocks::epoch(),
            position: taskboard_domain::entities::BoardClocks::epoch(),
            effort: taskboard_domain::entities::BoardClocks::epoch(),
            importance: taskboard_domain::entities::BoardClocks::epoch(),
            urgency: taskboard_domain::entities::BoardClocks::epoch(),
            recur: taskboard_domain::entities::BoardClocks::epoch(),
            kind: taskboard_domain::entities::BoardClocks::epoch(),
            archived: taskboard_domain::entities::BoardClocks::epoch(),
            deleted: taskboard_domain::entities::BoardClocks::epoch(),
        },
        authors: taskboard_domain::entities::TaskAuthors::default(),
        remote_seen: None,
        remote_baseline: None,
    }
}

/// An inbox board + stack to hang the triage rows on (the walk
/// fixtures' `insert_inbox_board_and_stack` shape).
pub fn insert_inbox_stack(state: &mut AppState) -> taskboard_domain::StackId {
    use taskboard_domain::entities::{
        Board, BoardAuthors, BoardClocks, Stack, StackAuthors, StackClocks,
    };
    let inbox = taskboard_domain::BoardId::from_uuid(uuid::Uuid::new_v4());
    state.boards.insert(
        inbox,
        Board {
            id: inbox,
            remote: None,
            title: "Inbox".to_owned(),
            color: taskboard_domain::Color::new(""),
            archived: false,
            deleted: false,
            clocks: BoardClocks {
                title: BoardClocks::epoch(),
                color: BoardClocks::epoch(),
                archived: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: BoardAuthors::default(),
            remote_seen: None,
        },
    );
    let inbox_stack = taskboard_domain::StackId::from_uuid(uuid::Uuid::new_v4());
    state.stacks.insert(
        inbox_stack,
        Stack {
            id: inbox_stack,
            remote: None,
            board: inbox,
            title: "Inbox".to_owned(),
            order: 0,
            archived: false,
            deleted: false,
            clocks: StackClocks {
                title: BoardClocks::epoch(),
                order: BoardClocks::epoch(),
                deleted: BoardClocks::epoch(),
            },
            authors: StackAuthors::default(),
            remote_seen: None,
        },
    );
    inbox_stack
}

/// Anchors `task` inside yesterday's window (the walk's carry row) —
/// the clock discipline's relative-oracle anchor over the harness
/// calendar's vocabulary (never a wall-clock assert).
pub fn anchor_yesterday(state: &mut AppState, task: taskboard_domain::TaskId) {
    let calendar = fixture::harness_calendar();
    let yesterday = calendar
        .today(chrono::Utc::now())
        .pred_opt()
        .expect("yesterday");
    let carry = calendar.instant(
        yesterday,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let mut row = state.tasks.get(&task).expect("fixture task").clone();
    row.scheduled = Some(carry);
    state.tasks.insert(task, row);
}

/// Schedules stack b's task into tomorrow's 9:00 (the planner
/// fixtures' placed box row) and returns its id.
pub fn place_stack_b_box(
    state: &mut AppState,
    stack_b: taskboard_domain::StackId,
) -> taskboard_domain::TaskId {
    let calendar = fixture::harness_calendar();
    let tomorrow = calendar
        .today(chrono::Utc::now())
        .succ_opt()
        .expect("tomorrow");
    let box_instant = calendar.instant(
        tomorrow,
        chrono::NaiveTime::from_hms_opt(9, 0, 0).expect("9:00"),
    );
    let box_task = state
        .tasks
        .values()
        .find(|task| task.stack == stack_b)
        .expect("stack b's task")
        .id;
    let mut boxed = state.tasks.get(&box_task).expect("box task").clone();
    boxed.scheduled = Some(box_instant);
    state.tasks.insert(box_task, boxed);
    box_task
}
