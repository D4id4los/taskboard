// SPDX-License-Identifier: MIT OR Apache-2.0
#![forbid(unsafe_code)]
//! Deterministic fixtures for the mapping benches (kiosk wave K7): one
//! builder parameterized by card/task count, every value a pure function
//! of N and the entity index — no rng, no proptest strategies, no clock
//! reads. Shared between the bench target and the fixture-contract test
//! via `#[path]`, mirroring the `taskboard-storage-sqlite` hot-path
//! fixture conventions verbatim in shape and spirit (a bench target
//! cannot import another crate's `benches/support/` — that module itself
//! documents the per-crate-helper precedent). The uuid-base family is a
//! distinct `0xA…` range so the two crates' fixtures cannot be confused.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeZone, Utc};
use taskboard_domain::entities::{
    Board, Color, Label, LabelClocks, Stack, StackClocks, Task, TaskClocks,
};
use taskboard_domain::ids::{
    BoardId, LabelId, RemoteBoardBinding, RemoteBoardId, RemoteCardId, RemoteCardRef,
    RemoteLabelId, RemoteLabelRef, RemoteStackId, RemoteStackRef, StackId, TaskId,
};
use taskboard_domain::outbox::{LocalOp, OpId, PendingOp};
use taskboard_domain::push::{CardShape, NewCardShape};
use taskboard_sync_nextcloud::{
    Board as WireBoard, Card as WireCard, Label as WireLabel, Stack as WireStack,
};

/// The N values the benches measure (the `hot_path` tiers verbatim).
pub const N_SIZES: [usize; 3] = [100, 1_000, 10_000];

/// Stack count (fixed across N; tasks fan out over them round-robin).
pub const STACK_COUNT: usize = 4;
/// Label count (fixed across N; tasks get 1–2 each round-robin).
pub const LABEL_COUNT: usize = 6;

/// Steady outbox ops for N tasks (one `UpdateTask` per 10th task — the
/// `hot_path` `outbox_count` shape verbatim).
#[must_use]
pub fn steady_count(n: usize) -> usize {
    n / 10
}

/// Disjoint uuid bases, one per entity kind: distinct ids across kinds
/// by construction, `Uuid::from_u128(BASE + i)` within a kind. The
/// `0xA…` family is this crate's own (the storage fixture uses
/// `0x1…–0x6…`).
const BOARD_BASE: u128 = 0xA1_0000_0000;
const STACK_BASE: u128 = 0xA2_0000_0000;
const LABEL_BASE: u128 = 0xA3_0000_0000;
const TASK_BASE: u128 = 0xA4_0000_0000;
const OP_BASE: u128 = 0xA5_0000_0000;

/// Fixture time origin: 2024-01-01T00:00:00Z (the `hot_path` origin).
/// Stamps are `EPOCH_SECS + <family offset> + i` — distinct and strictly
/// increasing in build order per family.
const EPOCH_SECS: i64 = 1_704_067_200;

#[must_use]
fn ts(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(EPOCH_SECS + secs, 0)
        .single()
        .expect("fixture stamp in range")
}

#[must_use]
fn id(base: u128, i: usize) -> uuid::Uuid {
    uuid::Uuid::from_u128(base + u128::try_from(i).expect("fixture index fits u128"))
}

#[must_use]
fn ix(i: usize) -> i64 {
    i64::try_from(i).expect("fixture index fits i64")
}

#[must_use]
fn ux(i: usize) -> u64 {
    u64::try_from(i).expect("fixture index fits u64")
}

fn fixture_board() -> Board {
    Board {
        id: BoardId::from(id(BOARD_BASE, 0)),
        remote: Some(RemoteBoardBinding {
            connector: taskboard_domain::ids::ConnectorId::from("nextcloud"),
            board: RemoteBoardId(77),
        }),
        title: "Bench board".into(),
        color: Color::new("3366ff"),
        archived: false,
        deleted: false,
        clocks: taskboard_domain::BoardClocks {
            title: ts(0),
            color: ts(0),
            archived: ts(0),
            deleted: ts(0),
        },
        remote_seen: Some(ts(0)),
    }
}

fn fixture_stack(s: usize, board: &Board) -> Stack {
    let stamp = ts(ix(s) + 1);
    Stack {
        id: StackId::from(id(STACK_BASE, s)),
        remote: Some(RemoteStackRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7 + ux(s)),
        }),
        board: board.id,
        title: format!("Stack {s}"),
        order: ix(s),
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: stamp,
            order: stamp,
            deleted: stamp,
        },
        remote_seen: Some(stamp),
    }
}

fn fixture_label(l: usize, board: &Board) -> Label {
    let stamp = ts(ix(l) + 2);
    Label {
        id: LabelId::from(id(LABEL_BASE, l)),
        remote: Some(RemoteLabelRef {
            board: RemoteBoardId(77),
            label: RemoteLabelId(9 + ux(l)),
        }),
        board: board.id,
        title: format!("Label {l}"),
        color: Color::new("00cc66"),
        deleted: false,
        clocks: LabelClocks {
            title: stamp,
            color: stamp,
            deleted: stamp,
        },
        remote_seen: Some(stamp),
    }
}

fn fixture_task(i: usize, stacks: &[Stack], labels: &[Label]) -> Task {
    let mut label_set = BTreeSet::new();
    label_set.insert(labels[i % LABEL_COUNT].id);
    if !i.is_multiple_of(2) {
        label_set.insert(labels[(i + 3) % LABEL_COUNT].id);
    }
    Task {
        id: TaskId::from(id(TASK_BASE, i)),
        remote: Some(RemoteCardRef {
            board: RemoteBoardId(77),
            stack: RemoteStackId(7),
            card: RemoteCardId(1_000 + ux(i)),
        }),
        title: format!("Task {i:05} bench fixture"),
        description: format!("Deterministic bench fixture task number {i}."),
        duedate: i.is_multiple_of(3).then(|| ts(ix(i) + 1_000_000)),
        scheduled: i.is_multiple_of(4).then(|| ts(ix(i) + 1_500_000)),
        done: i.is_multiple_of(10).then(|| ts(ix(i) + 2_000_000)),
        stack: stacks[i % STACK_COUNT].id,
        order: ix(i),
        labels: label_set,
        effort: None,
        importance: None,
        urgency: None,
        archived: false,
        deleted: false,
        clocks: task_clocks(i),
        remote_seen: Some(ts(ix(i) + 500)),
    }
}

fn task_clocks(i: usize) -> TaskClocks {
    let t = ts(ix(i));
    TaskClocks {
        title: t,
        description: t,
        duedate: t,
        scheduled: t,
        done: t,
        position: t,
        labels: t,
        effort: t,
        importance: t,
        urgency: t,
        archived: t,
        deleted: t,
    }
}

fn fixture_op(j: usize, tasks: &[Task]) -> PendingOp {
    PendingOp {
        op_id: OpId(id(OP_BASE, j)),
        op: LocalOp::UpdateTask(tasks[10 * j].id),
        queued_at: ts(ix(j) + 3_000_000),
    }
}

/// The steady outbox: one `UpdateTask` per 10th task, distinct targets,
/// no coalescing (the `hot_path` outbox shape verbatim).
fn steady_outbox(n: usize, tasks: &[Task]) -> Vec<PendingOp> {
    (0..steady_count(n)).map(|j| fixture_op(j, tasks)).collect()
}

/// The burst outbox: one op per task — the offline-editing-session worst
/// case. Kind rotates `[UpdateTask, AssignLabel, CreateTask, DeleteTask,
/// MoveTask][i % 5]`, so creates materialize `NewCardShape`s, deletes
/// dominate their groups, and every task forms exactly one group.
fn burst_outbox(n: usize, tasks: &[Task], labels: &[Label]) -> Vec<PendingOp> {
    (0..n)
        .map(|i| {
            let op = match i % 5 {
                0 => LocalOp::UpdateTask(tasks[i].id),
                1 => LocalOp::AssignLabel(tasks[i].id, labels[i % LABEL_COUNT].id),
                2 => LocalOp::CreateTask(tasks[i].id),
                3 => LocalOp::DeleteTask(tasks[i].id),
                _ => LocalOp::MoveTask(tasks[i].id),
            };
            PendingOp {
                op_id: OpId(id(OP_BASE, 1_000_000 + i)),
                op,
                queued_at: ts(ix(i) + 3_500_000),
            }
        })
        .collect()
}

/// The card fields of a create, by the same field list as the private
/// `NewCardShape::from_task` (it is not reachable from a bench target;
/// the shapes are the planner's own output type, constructed here as
/// `plan_pushes` would materialize them).
#[must_use]
pub fn new_card_shape(task: &Task) -> NewCardShape {
    NewCardShape {
        title: task.title.clone(),
        order: task.order,
        description: task.description.clone(),
        duedate: task.duedate,
        done: task.done,
        archived: task.archived,
        labels: task.labels.clone(),
    }
}

/// The full-send PUT fields, by the same field list as the private
/// `CardShape::from_task`.
#[must_use]
pub fn card_shape(task: &Task) -> CardShape {
    CardShape {
        title: task.title.clone(),
        description: task.description.clone(),
        duedate: task.duedate,
        done: task.done,
        order: task.order,
        archived: task.archived,
        labels: task.labels.clone(),
    }
}

fn wire_card(i: usize) -> WireCard {
    let mut labels = vec![taskboard_sync_nextcloud::CardLabel::from(
        9 + ux(i % LABEL_COUNT),
    )];
    if !i.is_multiple_of(2) {
        labels.push(taskboard_sync_nextcloud::CardLabel::from(
            9 + ux((i + 3) % LABEL_COUNT),
        ));
    }
    WireCard {
        id: 1_000 + ux(i),
        title: format!("Task {i:05} bench fixture"),
        stack_id: 7 + ux(i % STACK_COUNT),
        description: format!("Deterministic bench fixture task number {i}."),
        kind: "plain".into(),
        order: ix(i),
        created_at: EPOCH_SECS + 400 + ix(i),
        last_modified: EPOCH_SECS + 500 + ix(i),
        duedate: i.is_multiple_of(3).then(|| ts(ix(i) + 1_000_000)),
        done: i.is_multiple_of(10).then(|| ts(ix(i) + 2_000_000)),
        archived: false,
        owner: None,
        last_editor: None,
        labels,
        assigned_users: Vec::new(),
        attachments: Vec::new(),
        attachment_count: 0,
        comments_unread: 0,
        overdue: 0,
    }
}

/// The deterministic mapping fixture at card count `n`: the domain
/// entity tables `plan_pushes` borrows, both outbox input shapes, the
/// wire family the read mappers and `apply_card_shape` consume, and the
/// planner shapes for the write builders.
#[must_use]
pub fn build_fixture(n: usize) -> MappingFixture {
    let board = fixture_board();
    let stacks: Vec<Stack> = (0..STACK_COUNT).map(|s| fixture_stack(s, &board)).collect();
    let labels: Vec<Label> = (0..LABEL_COUNT).map(|l| fixture_label(l, &board)).collect();
    let tasks: Vec<Task> = (0..n).map(|i| fixture_task(i, &stacks, &labels)).collect();

    let task_map: BTreeMap<TaskId, Task> = tasks.iter().map(|t| (t.id, t.clone())).collect();
    let stack_map: BTreeMap<StackId, Stack> = stacks.iter().map(|s| (s.id, s.clone())).collect();
    let label_map: BTreeMap<LabelId, Label> = labels.iter().map(|l| (l.id, l.clone())).collect();
    let board_map: BTreeMap<BoardId, Board> = BTreeMap::from([(board.id, board.clone())]);

    let wire_board = WireBoard {
        id: 77,
        title: "Bench board".into(),
        color: taskboard_sync_nextcloud::DeckColor::from_hex("00c2e0")
            .expect("fixture color is valid hex"),
        deleted_at: 0,
        last_modified: EPOCH_SECS,
        archived: false,
        shared: 0,
        owner: None,
        users: Vec::new(),
        labels: Vec::new(),
        acl: Vec::new(),
        permissions: None,
    };
    let wire_stacks: Vec<WireStack> = (0..STACK_COUNT)
        .map(|s| WireStack {
            id: 7 + ux(s),
            title: format!("Stack {s}"),
            board_id: 77,
            deleted_at: 0,
            order: ix(s),
            last_modified: EPOCH_SECS + 1 + ix(s),
            archived: false,
            cards: Vec::new(),
        })
        .collect();
    let wire_labels: Vec<WireLabel> = (0..LABEL_COUNT)
        .map(|l| WireLabel {
            id: 9 + ux(l),
            title: format!("Label {l}"),
            color: taskboard_sync_nextcloud::DeckColor::from_hex("00cc66")
                .expect("fixture color is valid hex"),
            board_id: 77,
            last_modified: EPOCH_SECS + 2 + ix(l),
        })
        .collect();
    let wire_cards: Vec<WireCard> = (0..n).map(wire_card).collect();
    let steady = steady_outbox(n, &tasks);
    let burst = burst_outbox(n, &tasks, &labels);

    MappingFixture {
        board,
        stacks,
        labels,
        tasks,
        task_map,
        stack_map,
        label_map,
        board_map,
        steady_outbox: steady,
        burst_outbox: burst,
        wire_board,
        wire_stacks,
        wire_labels,
        wire_cards,
    }
}

/// A built fixture: the domain family (entity tables + both outbox
/// shapes), the wire family, and the derived planner shapes.
pub struct MappingFixture {
    /// The domain board.
    pub board: Board,
    /// The domain stacks.
    pub stacks: Vec<Stack>,
    /// The domain labels.
    pub labels: Vec<Label>,
    /// The domain tasks (n).
    pub tasks: Vec<Task>,
    /// The task table (the planner's input map).
    pub task_map: BTreeMap<TaskId, Task>,
    /// The stack table.
    pub stack_map: BTreeMap<StackId, Stack>,
    /// The label table.
    pub label_map: BTreeMap<LabelId, Label>,
    /// The board table.
    pub board_map: BTreeMap<BoardId, Board>,
    /// The steady outbox (`n/10` `UpdateTask` ops).
    pub steady_outbox: Vec<PendingOp>,
    /// The burst outbox (n ops, five-kind rotation).
    pub burst_outbox: Vec<PendingOp>,
    /// The wire board.
    pub wire_board: WireBoard,
    /// The wire stacks (4).
    pub wire_stacks: Vec<WireStack>,
    /// The wire labels (6).
    pub wire_labels: Vec<WireLabel>,
    /// The wire cards (n).
    pub wire_cards: Vec<WireCard>,
}
