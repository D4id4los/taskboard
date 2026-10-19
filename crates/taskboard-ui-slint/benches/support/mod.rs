// SPDX-License-Identifier: AGPL-3.0-only
//! Deterministic fixtures for the refresh+draw benches (stage-1 plan
//! D7): one builder parameterized by task count, every value a pure
//! function of N and the entity index — no rng, no clock reads.
//! Board + stacks + tasks only: labels, outbox ops, reminder rules
//! and validators have no UI-model surface (`task_card_model` reads
//! none of them). Shared between the bench target and the
//! fixture-contract test via `#[path]` (the storage bench's D1
//! convention).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, TimeZone, Utc};
use taskboard_domain::entities::{Board, Color, Stack, Task};
use taskboard_domain::ids::{BoardId, StackId, TaskId};
use taskboard_domain::state::SyncStates;
use taskboard_domain::{AppState, BoardClocks, StackClocks, TaskClocks};

/// The N values the benches measure (the storage benches' tiering).
pub const N_SIZES: [usize; 3] = [100, 1_000, 10_000];

/// Stack count (fixed across N; tasks fan out over them round-robin).
pub const STACK_COUNT: usize = 4;

/// The bench window size — the row-3 budget's own 1366×768 (used by
/// the bench target; the contract test doesn't render).
#[allow(dead_code)]
pub const BENCH_WIDTH: usize = 1366;
#[allow(dead_code)]
pub const BENCH_HEIGHT: usize = 768;

/// Disjoint uuid bases, one per entity kind: distinct ids across
/// kinds by construction, `Uuid::from_u128(BASE + i)` within a kind.
const BOARD_BASE: u128 = 0x110_0000_0000;
const STACK_BASE: u128 = 0x120_0000_0000;
const TASK_BASE: u128 = 0x130_0000_0000;

/// Fixture time origin: 2024-01-01T00:00:00Z (the storage benches'
/// stamp family; the UI models render the date phrases from these).
const EPOCH_SECS: i64 = 1_704_067_200;

/// The fixed duedate pattern: every 3rd task carries a due date.
#[must_use]
pub fn has_duedate(order: i64) -> bool {
    order.rem_euclid(3) == 0
}

/// The fixed done pattern: every 10th task is done.
#[must_use]
pub fn has_done(order: i64) -> bool {
    order.rem_euclid(10) == 0
}

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
fn fixture_board() -> Board {
    Board {
        id: BoardId::from(id(BOARD_BASE, 0)),
        remote: None,
        title: "Bench board".into(),
        color: Color::new("3366ff"),
        archived: false,
        deleted: false,
        clocks: BoardClocks {
            title: ts(0),
            color: ts(0),
            archived: ts(0),
            deleted: ts(0),
        },
        remote_seen: None,
    }
}

#[must_use]
fn fixture_stack(s: usize, board: &Board) -> Stack {
    let stamp = ts(i64::try_from(s).expect("fixture index fits i64") + 1);
    Stack {
        id: StackId::from(id(STACK_BASE, s)),
        remote: None,
        board: board.id,
        title: format!("Stack {s}"),
        order: i64::try_from(s).expect("fixture index fits i64"),
        archived: false,
        deleted: false,
        clocks: StackClocks {
            title: stamp,
            order: stamp,
            deleted: stamp,
        },
        remote_seen: None,
    }
}

#[must_use]
fn fixture_task(i: usize, stacks: &[Stack]) -> Task {
    let order = i64::try_from(i).expect("fixture index fits i64");
    Task {
        id: TaskId::from(id(TASK_BASE, i)),
        remote: None,
        // ASCII-only (the golden lane's R7/R8 rule): software text
        // layout never falls back to a system font for a glyph.
        title: format!("Task {i:05} bench fixture"),
        description: format!("Deterministic bench fixture task number {i}."),
        duedate: has_duedate(order).then(|| ts(order + 1_000_000)),
        done: has_done(order).then(|| ts(order + 2_000_000)),
        stack: stacks[i % STACK_COUNT].id,
        order,
        labels: BTreeSet::new(),
        archived: false,
        deleted: false,
        clocks: {
            let t = ts(order);
            TaskClocks {
                title: t,
                description: t,
                duedate: t,
                done: t,
                position: t,
                labels: t,
                archived: t,
                deleted: t,
            }
        },
        remote_seen: None,
    }
}

/// The deterministic UI-mount state at task count `n`: exactly one
/// live board, [`STACK_COUNT`] live stacks, exactly `n` non-deleted
/// tasks round-robinned across them.
#[must_use]
pub fn build_state(n: usize) -> AppState {
    let board = fixture_board();
    let stacks: Vec<Stack> = (0..STACK_COUNT).map(|s| fixture_stack(s, &board)).collect();
    let tasks: Vec<Task> = (0..n).map(|i| fixture_task(i, &stacks)).collect();
    AppState {
        boards: BTreeMap::from([(board.id, board)]),
        stacks: stacks.into_iter().map(|s| (s.id, s)).collect(),
        tasks: tasks.into_iter().map(|t| (t.id, t)).collect(),
        labels: BTreeMap::new(),
        reminders: taskboard_domain::reminders::ReminderState::default(),
        sync: SyncStates::new(),
        last_updated: None,
    }
}
