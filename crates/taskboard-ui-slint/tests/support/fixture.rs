// SPDX-License-Identifier: AGPL-3.0-only

//! The journey/golden fixture (chunk plan §5.1): the two-stack board
//! shape of the §6 component tests, extracted for the integration
//! binaries (the lib's private copy stays — duplication across test
//! binaries is the cargo reality, noted in the plan).
//!
//! R7/R8: titles are ASCII-only so golden rendering can never fall
//! back to a system font for a missing glyph.

use chrono::TimeDelta;
use std::collections::BTreeMap;
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
        scheduled: epoch(),
        done: epoch(),
        position: epoch(),
        effort: epoch(),
        importance: epoch(),
        urgency: epoch(),
        recur: epoch(),

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
        scheduled: None,
        done: None,
        stack,
        order,
        label_memberships: BTreeMap::new(),
        effort: None,
        importance: None,
        urgency: None,
        recur: None,

        archived: false,
        deleted: false,
        clocks: task_clocks(),
        remote_seen: None,
        remote_baseline: None,
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

/// The timers-surface helpers (DR-3/3, D-P13): mint a live alarm into
/// `state` and hand back its id — the journeys address rows through
/// these, never hand-building the domain rows inline.
#[must_use]
pub fn add_alarm(
    state: &mut AppState,
    label: &str,
    fire_at: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::AdhocAlarmId {
    let id = taskboard_domain::AdhocAlarmId::from_uuid(uuid::Uuid::now_v7());
    state.reminders.alarms.insert(
        id,
        taskboard_domain::AdhocAlarm {
            insistence: taskboard_domain::Insistence::Normal,
            id,
            label: label.to_owned(),
            fire_at,
            created_at: fire_at - TimeDelta::minutes(1),
        },
    );
    id
}

/// The tracking-surface helper (DR-5/4, D-TU12): mints one open entry
/// into `state` and hands back its id — the journeys address the chip
/// and the badge through these, never hand-building the domain row.
#[must_use]
pub fn add_open_entry(
    state: &mut AppState,
    task: taskboard_domain::TaskId,
    title: &str,
    start: chrono::DateTime<chrono::Utc>,
) -> taskboard_domain::TimeEntryId {
    let id = taskboard_domain::TimeEntryId::from_uuid(uuid::Uuid::now_v7());
    state.tracking.entries.insert(
        id,
        taskboard_domain::TimeEntry {
            id,
            task,
            task_title: title.to_owned(),
            start,
            end: None,
            created_at: start,
            updated_at: start,
        },
    );
    id
}

/// Records the delivery receipt for an alarm's occurrence (the fire
/// fact that makes the row awaiting).
pub fn deliver_alarm(
    state: &mut AppState,
    alarm: taskboard_domain::AdhocAlarmId,
    occurrence: chrono::DateTime<chrono::Utc>,
) {
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: taskboard_domain::ReminderRef::Adhoc(alarm),
            occurrence,
            delivered_at: occurrence,
        });
}

/// A delivered (fired ∧ ¬dismissed) task-anchored notification: an
/// open task gains a due instant plus an explicit at-due rule whose
/// delivery receipt exists — the notification surface's task row
/// (the anchor side that keeps all four task-shaped buttons).
pub fn deliver_task_notification(
    state: &mut AppState,
    task: taskboard_domain::TaskId,
    due: chrono::DateTime<chrono::Utc>,
) {
    if let Some(t) = state.tasks.get_mut(&task) {
        t.duedate = Some(due);
    }
    let rule_id = taskboard_domain::ReminderRuleId::from_uuid(uuid::Uuid::now_v7());
    let now = chrono::Utc::now();
    state.reminders.rules.insert(
        rule_id,
        taskboard_domain::ReminderRule {
            insistence: taskboard_domain::Insistence::Normal,
            id: rule_id,
            task,
            offset: taskboard_domain::ReminderOffset::at_due(),
            created_at: now,
            updated_at: now,
        },
    );
    state
        .reminders
        .deliveries
        .insert(taskboard_domain::ReminderDelivery {
            reminder: taskboard_domain::ReminderRef::Rule(rule_id),
            occurrence: due,
            delivered_at: due,
        });
}

/// The journey fixture (§5.1): one board, two stacks ("a" with tasks
/// 0 and 1, 1 overdue; "b" with task 2), sync `Offline` with two
/// pending ops — the step-9 status read's subject.
#[must_use]
pub fn journey_state() -> (Fixture, AppState) {
    let (fixture, mut state) = golden_state();
    let mut overdue = state.tasks.get(&fixture.task1).expect("task").clone();
    // Midnight-aligned (round-3 D9): the due phrases now carry the
    // UTC `· HH:MM` suffix for non-midnight instants — a wall-clock
    // stamp would make the goldens run-dependent. Midnight renders
    // exactly the date-only shape the goldens pin.
    let today = chrono::Utc::now().date_naive();
    overdue.duedate = Some(
        today
            .checked_sub_signed(TimeDelta::try_days(1).unwrap())
            .expect("yesterday exists")
            .and_hms_opt(0, 0, 0)
            .expect("midnight")
            .and_utc(),
    );
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
    // The seeded running timer (DR-5/4, D-TU12): task1 holds the open
    // entry, 12 minutes old at fixture construction — the chip and
    // the badge appear in the golden matrix (wall-clock-derived text
    // is the established golden class; a minute-boundary straddle is
    // the recorded residual risk).
    let open_task1 = add_open_entry(
        &mut state,
        task1,
        "task-1",
        chrono::Utc::now() - TimeDelta::minutes(12),
    );
    debug_assert!(state.tracking.entries.contains_key(&open_task1));
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

/// The today-view fixture (chunk 10 D12): the two-board journey state
/// with overdue stamps on one row per board — overdue is the one
/// section whose membership is wall-clock-proof (due < now, any run),
/// so the journey's walk is deterministic without injecting a
/// `DayCtx`. Returns the two walked task ids (board one's first, then
/// board two's task in display order: overdue sorts (order, id)).
#[must_use]
pub fn today_overdue_state() -> (TwoBoardFixture, AppState, [TaskId; 2]) {
    let (fixture, mut state) = two_board_state();
    let now = chrono::Utc::now();
    // Midnight-aligned (round-3 D9): the due phrases carry the UTC
    // `· HH:MM` suffix for non-midnight instants, and a minute-
    // relative stamp flips its caption bucket across minute
    // boundaries — midnight renders the date-only shape, stable for
    // the run's lifetime and identical across runs.
    let overdue_stamp = now
        .date_naive()
        .checked_sub_signed(chrono::TimeDelta::try_days(1).unwrap())
        .expect("yesterday exists")
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc();
    let board_one_task = TaskId::from_uuid(uuid::Uuid::from_u128(0x3001));
    let board_two_task = TaskId::from_uuid(uuid::Uuid::from_u128(0x3004));
    for id in [board_one_task, board_two_task] {
        let task = state.tasks.get_mut(&id).expect("fixture task");
        task.duedate = Some(overdue_stamp);
    }
    // The seeded running timer (DR-5/4, D-TU12): the today matrix
    // carries the chip and the overdue row's badge, same minute-
    // relative stamp class the board goldens accepted.
    let _ = add_open_entry(
        &mut state,
        board_one_task,
        "task one",
        now - TimeDelta::minutes(12),
    );
    (fixture, state, [board_one_task, board_two_task])
}

/// The triage fixture (chunk 10 D12): the global Inbox board ("Inbox",
/// one stack, one open task) plus the two-board journey state's board
/// two as the move target.
#[must_use]
pub fn triage_state() -> (TwoBoardFixture, AppState, TaskId) {
    let (fixture, mut state) = two_board_state();
    let inbox_board = BoardId::from_uuid(uuid::Uuid::from_u128(0x1003));
    let inbox_stack = StackId::from_uuid(uuid::Uuid::from_u128(0x2004));
    state.boards.insert(inbox_board, board(inbox_board));
    title_board(&mut state, inbox_board, "Inbox");
    state
        .stacks
        .insert(inbox_stack, stack(inbox_stack, inbox_board, "Inbox", 0));
    let caught = TaskId::from_uuid(uuid::Uuid::from_u128(0x3005));
    let mut t = task(caught, inbox_stack, 0);
    "caught thought".clone_into(&mut t.title);
    state.tasks.insert(caught, t);
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    (fixture, state, caught)
}

fn title_board(state: &mut AppState, id: BoardId, title: &str) {
    title.clone_into(&mut state.boards.get_mut(&id).expect("board").title);
}

/// The empty board (round-3 R5/F10): one live board, zero stacks —
/// the leg the create-stack field must render in.
#[must_use]
pub fn empty_board_state() -> AppState {
    let (_fixture, mut state) = golden_state();
    let stacks: Vec<StackId> = state.stacks.keys().copied().collect();
    for stack in stacks {
        state.stacks.remove(&stack);
    }
    state.tasks.clear();
    taskboard_domain::derive_sync_states(
        &state.boards,
        &state.stacks,
        &state.tasks,
        &state.labels,
        &[],
        &mut state.sync,
    );
    state
}
